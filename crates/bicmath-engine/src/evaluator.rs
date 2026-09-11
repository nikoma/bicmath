//! Restricted expression evaluation.
//!
//! The evaluator resolves calls through the same registry, descriptor schemas,
//! and argument validation used by the typed `calculate` path. It has no
//! loops, recursion, mutation, reflection, imports, file access, or remote
//! code.

use std::collections::{BTreeMap, BTreeSet};

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, Assumption, ErrorEstimate, FunctionRef, Outcome, Trace, Warning,
};
use bicmath_core::envelope::Exactness;
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::expr::{BinaryOp, CallArg, CompareOp, Expr, UnaryOp};
use bicmath_core::number::{Number, NumberResult};
use bicmath_core::value::Value;

use crate::registry::Registry;
use crate::request::require_mode;

/// Accumulated evaluation output.
#[derive(Clone, Debug)]
pub struct EvalOutput {
    pub value: Value,
    pub exactness: Exactness,
    pub warnings: Vec<Warning>,
    pub assumptions: Vec<Assumption>,
    pub error_estimate: Option<ErrorEstimate>,
    pub trace: Option<Trace>,
    pub conversions: Vec<bicmath_core::context::Conversion>,
    pub functions: Vec<FunctionRef>,
}

struct State {
    exactness: Exactness,
    warnings: Vec<Warning>,
    assumptions: Vec<Assumption>,
    error_estimate: Option<ErrorEstimate>,
    trace: Option<Trace>,
    conversions: Vec<bicmath_core::context::Conversion>,
    functions: Vec<FunctionRef>,
    ops: u64,
}

impl State {
    fn new() -> State {
        State {
            exactness: Exactness::Exact,
            warnings: Vec::new(),
            assumptions: Vec::new(),
            error_estimate: None,
            trace: None,
            conversions: Vec::new(),
            functions: Vec::new(),
            ops: 0,
        }
    }

    fn tick(&mut self, ctx: &ExecContext) -> Result<(), EngineError> {
        self.ops += 1;
        if self.ops > ctx.limits.max_operations {
            return Err(EngineError::new(
                ErrorCode::ResourceLimit,
                format!(
                    "expression operation budget of {} exceeded",
                    ctx.limits.max_operations
                ),
            ));
        }
        ctx.check()
    }

    fn observe(&mut self, outcome: &Outcome) {
        self.exactness = self.exactness.combine(outcome.exactness);
        self.warnings.extend(outcome.warnings.iter().cloned());
        self.assumptions.extend(outcome.assumptions.iter().cloned());
        if self.error_estimate.is_none() {
            self.error_estimate = outcome.error_estimate.clone();
        }
        self.conversions.extend(outcome.conversions.iter().cloned());
        if let Some(trace) = &outcome.trace {
            let target = self.trace.get_or_insert_with(Trace::new);
            target.steps.extend(trace.steps.iter().cloned());
        }
    }
}

/// Evaluate a parsed expression with named bindings.
pub fn evaluate(
    registry: &Registry,
    expression: &Expr,
    bindings: &BTreeMap<String, Value>,
    ctx: &ExecContext,
) -> Result<EvalOutput, EngineError> {
    let mut state = State::new();
    let value = eval(registry, expression, bindings, ctx, &mut state)?;
    Ok(EvalOutput {
        value,
        exactness: state.exactness,
        warnings: state.warnings,
        assumptions: state.assumptions,
        error_estimate: state.error_estimate,
        trace: state.trace,
        conversions: state.conversions,
        functions: state.functions,
    })
}

fn eval(
    registry: &Registry,
    expr: &Expr,
    bindings: &BTreeMap<String, Value>,
    ctx: &ExecContext,
    state: &mut State,
) -> Result<Value, EngineError> {
    state.tick(ctx)?;
    match expr {
        Expr::Number(number) => Ok(Value::Number(number.clone())),
        Expr::Text(text) => Ok(Value::Text(text.clone())),
        Expr::Bool(value) => Ok(Value::Bool(*value)),
        Expr::Ident(name) => bindings.get(name).cloned().ok_or_else(|| {
            let mut available: Vec<&String> = bindings.keys().collect();
            available.sort();
            EngineError::new(
                ErrorCode::NotFound,
                format!("unknown binding {name:?}; available bindings: {available:?}"),
            )
        }),
        Expr::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(eval(registry, item, bindings, ctx, state)?);
            }
            Ok(Value::Array(out))
        }
        Expr::Record(fields) => {
            let mut out = BTreeMap::new();
            for (key, value) in fields {
                out.insert(key.clone(), eval(registry, value, bindings, ctx, state)?);
            }
            Ok(Value::Record(out))
        }
        Expr::Unary { op, expr } => {
            let value = eval(registry, expr, bindings, ctx, state)?;
            match op {
                UnaryOp::Neg => match value {
                    Value::Number(number) => {
                        let result = number.neg();
                        state.exactness = state.exactness.combine(if result.is_float() {
                            Exactness::Approximate
                        } else {
                            Exactness::Exact
                        });
                        Ok(Value::Number(result))
                    }
                    other => Err(EngineError::malformed(format!(
                        "unary minus requires a number, found {}",
                        other.kind_name()
                    ))),
                },
                UnaryOp::Pos => match value {
                    Value::Number(_) => Ok(value),
                    other => Err(EngineError::malformed(format!(
                        "unary plus requires a number, found {}",
                        other.kind_name()
                    ))),
                },
                UnaryOp::Not => match value {
                    Value::Bool(value) => Ok(Value::Bool(!value)),
                    other => Err(EngineError::malformed(format!(
                        "logical not requires a boolean, found {}",
                        other.kind_name()
                    ))),
                },
            }
        }
        Expr::Binary { op, left, right } => {
            eval_binary(registry, *op, left, right, bindings, ctx, state)
        }
        Expr::Compare { operands, ops } => {
            let mut values = Vec::with_capacity(operands.len());
            for operand in operands {
                values.push(eval(registry, operand, bindings, ctx, state)?);
            }
            for (index, op) in ops.iter().enumerate() {
                let ordering = compare_values(&values[index], &values[index + 1], *op)?;
                if !ordering {
                    return Ok(Value::Bool(false));
                }
            }
            Ok(Value::Bool(true))
        }
        Expr::Call { name, args } => eval_call(registry, name, args, bindings, ctx, state),
    }
}

fn eval_binary(
    registry: &Registry,
    op: BinaryOp,
    left: &Expr,
    right: &Expr,
    bindings: &BTreeMap<String, Value>,
    ctx: &ExecContext,
    state: &mut State,
) -> Result<Value, EngineError> {
    if matches!(op, BinaryOp::And | BinaryOp::Or) {
        let left_value = eval(registry, left, bindings, ctx, state)?;
        let left_bool = left_value.as_bool().map_err(|_| {
            EngineError::malformed(format!(
                "logical operator requires booleans, found {}",
                left_value.kind_name()
            ))
        })?;
        match (op, left_bool) {
            (BinaryOp::And, false) => return Ok(Value::Bool(false)),
            (BinaryOp::Or, true) => return Ok(Value::Bool(true)),
            _ => {}
        }
        let right_value = eval(registry, right, bindings, ctx, state)?;
        let right_bool = right_value.as_bool().map_err(|_| {
            EngineError::malformed(format!(
                "logical operator requires booleans, found {}",
                right_value.kind_name()
            ))
        })?;
        return Ok(Value::Bool(right_bool));
    }
    let left_value = eval(registry, left, bindings, ctx, state)?;
    let right_value = eval(registry, right, bindings, ctx, state)?;
    let left_number = left_value.as_number().map_err(|_| {
        EngineError::malformed(format!(
            "arithmetic requires numbers, found {}",
            left_value.kind_name()
        ))
    })?;
    let right_number = right_value.as_number().map_err(|_| {
        EngineError::malformed(format!(
            "arithmetic requires numbers, found {}",
            right_value.kind_name()
        ))
    })?;
    let result: NumberResult = match op {
        BinaryOp::Add => left_number.add(right_number, &ctx.numeric, &ctx.limits)?,
        BinaryOp::Sub => left_number.sub(right_number, &ctx.numeric, &ctx.limits)?,
        BinaryOp::Mul => left_number.mul(right_number, &ctx.numeric, &ctx.limits)?,
        BinaryOp::Div => left_number.div(right_number, &ctx.numeric, &ctx.limits)?,
        BinaryOp::Rem => left_number.rem(right_number, &ctx.numeric, &ctx.limits)?,
        BinaryOp::Pow => left_number.pow(right_number, &ctx.numeric, &ctx.limits)?,
        BinaryOp::And | BinaryOp::Or => unreachable!("handled above"),
    };
    state.exactness = state
        .exactness
        .combine(classify(&result.value, result.rounded));
    Ok(Value::Number(result.value))
}

fn classify(number: &Number, rounded: bool) -> Exactness {
    if number.is_float() {
        Exactness::Approximate
    } else if rounded {
        Exactness::Rounded
    } else {
        Exactness::Exact
    }
}

fn compare_values(left: &Value, right: &Value, op: CompareOp) -> Result<bool, EngineError> {
    use std::cmp::Ordering;
    let ordering = match (left, right) {
        (Value::Number(a), Value::Number(b)) => a.compare(b)?,
        (Value::Bool(a), Value::Bool(b)) => a.cmp(b),
        (Value::Text(a), Value::Text(b)) => a.cmp(b),
        (Value::Null, Value::Null) => Ordering::Equal,
        (a, b) => {
            return Err(EngineError::malformed(format!(
                "cannot compare {} with {}",
                a.kind_name(),
                b.kind_name()
            )));
        }
    };
    Ok(match op {
        CompareOp::Eq => ordering == Ordering::Equal,
        CompareOp::Ne => ordering != Ordering::Equal,
        CompareOp::Lt => ordering == Ordering::Less,
        CompareOp::Le => ordering != Ordering::Greater,
        CompareOp::Gt => ordering == Ordering::Greater,
        CompareOp::Ge => ordering != Ordering::Less,
    })
}

fn eval_call(
    registry: &Registry,
    name: &str,
    args: &[CallArg],
    bindings: &BTreeMap<String, Value>,
    ctx: &ExecContext,
    state: &mut State,
) -> Result<Value, EngineError> {
    let function = registry.function(name)?;
    let descriptor = function.descriptor();
    require_mode(ctx, &descriptor.modes, name)?;
    let mut values: BTreeMap<String, Value> = BTreeMap::new();
    let mut used: BTreeSet<String> = BTreeSet::new();
    let positional_params: Vec<&str> = descriptor
        .parameters
        .iter()
        .filter(|p| p.positional)
        .map(|p| p.name.as_str())
        .collect();
    let mut positional_index = 0usize;
    for arg in args {
        let value = eval(registry, &arg.value, bindings, ctx, state)?;
        let param_name = match &arg.name {
            Some(explicit) => {
                if descriptor.parameter(explicit).is_none() {
                    return Err(EngineError::malformed(format!(
                        "unknown argument {explicit:?} for function {name}"
                    ))
                    .with_path(explicit.clone()));
                }
                explicit.clone()
            }
            None => {
                let next = positional_params.get(positional_index).ok_or_else(|| {
                    EngineError::malformed(format!(
                        "too many positional arguments for function {name}"
                    ))
                })?;
                positional_index += 1;
                (*next).to_string()
            }
        };
        if used.contains(&param_name) {
            return Err(EngineError::malformed(format!(
                "argument {param_name:?} supplied more than once"
            ))
            .with_path(param_name));
        }
        used.insert(param_name.clone());
        if value.is_null() {
            continue;
        }
        crate::request::validate_argument(descriptor, &param_name, &value, &ctx.limits)?;
        values.insert(param_name, value);
    }
    for param in &descriptor.parameters {
        if param.required && !values.contains_key(&param.name) {
            return Err(EngineError::malformed(format!(
                "missing required argument {:?} for function {name}",
                param.name
            ))
            .with_path(param.name.clone()));
        }
    }
    let outcome = function.invoke(&Args::new(values), ctx)?;
    state.observe(&outcome);
    state.functions.push(FunctionRef {
        id: descriptor.id.clone(),
        version: descriptor.version.clone(),
    });
    Ok(outcome.value)
}
