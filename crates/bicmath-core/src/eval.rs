//! Shared restricted-expression evaluators.
//!
//! `bicmath-core` owns the expression AST and parser. This module provides the
//! one shared evaluation walk used by modules that must evaluate expressions
//! they receive as data (verification, optimization, interval analysis).
//! Function resolution is delegated to a caller-supplied table, so each module
//! controls exactly which functions are reachable. No arbitrary code is ever
//! evaluated.

use std::collections::BTreeMap;

use crate::error::{EngineError, ErrorCode};
use crate::expr::{BinaryOp, Expr, UnaryOp};
use crate::limits::Limits;
use crate::number::{Number, NumericContext};

/// Function table for exact-number evaluation.
pub type NumberCalls<'a> = dyn Fn(&str, &[Number]) -> Result<Number, EngineError> + 'a;

/// Function table for float64 evaluation.
pub type FloatCalls<'a> = dyn Fn(&str, &[f64]) -> Result<f64, EngineError> + 'a;

struct Budget {
    ops: u64,
    max_ops: u64,
    max_depth: usize,
}

impl Budget {
    fn new(limits: &Limits) -> Budget {
        Budget {
            ops: 0,
            max_ops: limits.max_operations,
            max_depth: limits.max_ast_depth,
        }
    }

    fn tick(&mut self, depth: usize) -> Result<(), EngineError> {
        if depth > self.max_depth {
            return Err(EngineError::new(
                ErrorCode::ResourceLimit,
                format!(
                    "expression nesting depth {depth} exceeds the limit of {}",
                    self.max_depth
                ),
            ));
        }
        self.ops += 1;
        if self.ops > self.max_ops {
            return Err(EngineError::new(
                ErrorCode::ResourceLimit,
                format!("expression operation budget of {} exceeded", self.max_ops),
            ));
        }
        Ok(())
    }
}

/// Evaluate an expression exactly, using the shared numeric contract.
///
/// Supported: integer/decimal/rational literals, bindings, unary sign,
/// `+ - * / % ^`, and calls resolved by `calls`. Comparisons, booleans, arrays,
/// records, and text are rejected with a clear error.
pub fn evaluate_number(
    expr: &Expr,
    bindings: &BTreeMap<String, Number>,
    numeric: &NumericContext,
    limits: &Limits,
    calls: &NumberCalls<'_>,
) -> Result<Number, EngineError> {
    let mut budget = Budget::new(limits);
    eval_number(expr, bindings, numeric, limits, calls, &mut budget, 0)
}

fn eval_number(
    expr: &Expr,
    bindings: &BTreeMap<String, Number>,
    numeric: &NumericContext,
    limits: &Limits,
    calls: &NumberCalls<'_>,
    budget: &mut Budget,
    depth: usize,
) -> Result<Number, EngineError> {
    budget.tick(depth)?;
    match expr {
        Expr::Number(number) => Ok(number.clone()),
        Expr::Ident(name) => bindings.get(name).cloned().ok_or_else(|| {
            let mut available: Vec<&String> = bindings.keys().collect();
            available.sort();
            EngineError::new(
                ErrorCode::NotFound,
                format!("unknown binding {name:?}; available: {available:?}"),
            )
        }),
        Expr::Unary { op, expr } => {
            let value = eval_number(expr, bindings, numeric, limits, calls, budget, depth + 1)?;
            match op {
                UnaryOp::Neg => Ok(value.neg()),
                UnaryOp::Pos => Ok(value),
                UnaryOp::Not => Err(EngineError::malformed(
                    "logical negation is not valid in a numeric expression",
                )),
            }
        }
        Expr::Binary { op, left, right } => {
            let a = eval_number(left, bindings, numeric, limits, calls, budget, depth + 1)?;
            let b = eval_number(right, bindings, numeric, limits, calls, budget, depth + 1)?;
            let result = match op {
                BinaryOp::Add => a.add(&b, numeric, limits)?,
                BinaryOp::Sub => a.sub(&b, numeric, limits)?,
                BinaryOp::Mul => a.mul(&b, numeric, limits)?,
                BinaryOp::Div => a.div(&b, numeric, limits)?,
                BinaryOp::Rem => a.rem(&b, numeric, limits)?,
                BinaryOp::Pow => a.pow(&b, numeric, limits)?,
                BinaryOp::And | BinaryOp::Or => {
                    return Err(EngineError::malformed(
                        "logical operators are not valid in a numeric expression",
                    ));
                }
            };
            Ok(result.value)
        }
        Expr::Call { name, args } => {
            let mut values = Vec::with_capacity(args.len());
            for arg in args {
                if arg.name.is_some() {
                    return Err(EngineError::malformed(
                        "named arguments are not supported in this expression context",
                    ));
                }
                values.push(eval_number(
                    &arg.value,
                    bindings,
                    numeric,
                    limits,
                    calls,
                    budget,
                    depth + 1,
                )?);
            }
            calls(name, &values)
        }
        Expr::Compare { .. } => Err(EngineError::malformed(
            "comparisons are not valid in a numeric expression",
        )),
        Expr::Bool(_) | Expr::Text(_) | Expr::Array(_) | Expr::Record(_) => Err(
            EngineError::malformed("expression must produce a single numeric value"),
        ),
    }
}

/// Evaluate an expression in binary64, using a caller-supplied function table.
pub fn evaluate_f64(
    expr: &Expr,
    bindings: &BTreeMap<String, f64>,
    limits: &Limits,
    calls: &FloatCalls<'_>,
) -> Result<f64, EngineError> {
    let mut budget = Budget::new(limits);
    eval_f64(expr, bindings, calls, &mut budget, 0)
}

fn eval_f64(
    expr: &Expr,
    bindings: &BTreeMap<String, f64>,
    calls: &FloatCalls<'_>,
    budget: &mut Budget,
    depth: usize,
) -> Result<f64, EngineError> {
    budget.tick(depth)?;
    match expr {
        Expr::Number(number) => number
            .to_f64()
            .ok_or_else(|| EngineError::domain("numeric literal is not representable as float64")),
        Expr::Ident(name) => bindings.get(name).copied().ok_or_else(|| {
            let mut available: Vec<&String> = bindings.keys().collect();
            available.sort();
            EngineError::new(
                ErrorCode::NotFound,
                format!("unknown binding {name:?}; available: {available:?}"),
            )
        }),
        Expr::Unary { op, expr } => {
            let value = eval_f64(expr, bindings, calls, budget, depth + 1)?;
            match op {
                UnaryOp::Neg => Ok(-value),
                UnaryOp::Pos => Ok(value),
                UnaryOp::Not => Err(EngineError::malformed(
                    "logical negation is not valid in a numeric expression",
                )),
            }
        }
        Expr::Binary { op, left, right } => {
            let a = eval_f64(left, bindings, calls, budget, depth + 1)?;
            let b = eval_f64(right, bindings, calls, budget, depth + 1)?;
            match op {
                BinaryOp::Add => Ok(a + b),
                BinaryOp::Sub => Ok(a - b),
                BinaryOp::Mul => Ok(a * b),
                BinaryOp::Div => {
                    if b == 0.0 {
                        Err(EngineError::division_by_zero("float division by zero"))
                    } else {
                        Ok(a / b)
                    }
                }
                BinaryOp::Rem => {
                    if b == 0.0 {
                        Err(EngineError::division_by_zero("float remainder by zero"))
                    } else {
                        Ok(a % b)
                    }
                }
                BinaryOp::Pow => {
                    let value = a.powf(b);
                    if value.is_finite() {
                        Ok(value)
                    } else {
                        Err(EngineError::domain(format!("power {a}^{b} is not finite")))
                    }
                }
                BinaryOp::And | BinaryOp::Or => Err(EngineError::malformed(
                    "logical operators are not valid in a numeric expression",
                )),
            }
        }
        Expr::Call { name, args } => {
            let mut values = Vec::with_capacity(args.len());
            for arg in args {
                if arg.name.is_some() {
                    return Err(EngineError::malformed(
                        "named arguments are not supported in this expression context",
                    ));
                }
                values.push(eval_f64(&arg.value, bindings, calls, budget, depth + 1)?);
            }
            let value = calls(name, &values)?;
            if value.is_finite() {
                Ok(value)
            } else {
                Err(EngineError::domain(format!(
                    "function {name} produced a non-finite result"
                )))
            }
        }
        Expr::Compare { .. } => Err(EngineError::malformed(
            "comparisons are not valid in a numeric expression",
        )),
        Expr::Bool(_) | Expr::Text(_) | Expr::Array(_) | Expr::Record(_) => Err(
            EngineError::malformed("expression must produce a single numeric value"),
        ),
    }
}

/// A function table built from a list of `(name, arity, fn)` entries.
/// One entry in a restricted function table.
type FloatCall = Box<dyn Fn(&[f64]) -> Result<f64, EngineError>>;

pub struct SimpleCalls {
    entries: Vec<(String, usize, FloatCall)>,
}

impl SimpleCalls {
    pub fn new() -> SimpleCalls {
        SimpleCalls {
            entries: Vec::new(),
        }
    }

    pub fn add<F>(mut self, name: &str, arity: usize, function: F) -> SimpleCalls
    where
        F: Fn(&[f64]) -> Result<f64, EngineError> + 'static,
    {
        self.entries
            .push((name.to_string(), arity, Box::new(function)));
        self
    }

    pub fn dispatch(&self, name: &str, args: &[f64]) -> Result<f64, EngineError> {
        for (candidate, arity, function) in &self.entries {
            if candidate == name {
                if args.len() != *arity {
                    return Err(EngineError::malformed(format!(
                        "function {name} expects {arity} argument(s), found {}",
                        args.len()
                    )));
                }
                return function(args);
            }
        }
        Err(EngineError::new(
            ErrorCode::UnknownFunction,
            format!("unknown function {name:?} in restricted expression"),
        ))
    }

    pub fn as_table(&self) -> impl Fn(&str, &[f64]) -> Result<f64, EngineError> + '_ {
        move |name, args| self.dispatch(name, args)
    }
}

impl Default for SimpleCalls {
    fn default() -> Self {
        SimpleCalls::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::parse_expression;

    fn limits() -> Limits {
        Limits::conservative()
    }

    #[test]
    fn exact_evaluation_uses_shared_arithmetic() {
        let expr = parse_expression("0.1 + 0.2 * 2", &limits()).unwrap();
        let result = evaluate_number(
            &expr,
            &BTreeMap::new(),
            &NumericContext::default(),
            &limits(),
            &|name, _| Err(EngineError::new(ErrorCode::UnknownFunction, name)),
        )
        .unwrap();
        assert_eq!(result.to_string(), "0.5");
    }

    #[test]
    fn exact_evaluation_is_rational_for_division() {
        let expr = parse_expression("1 / 3 + 1 / 6", &limits()).unwrap();
        let result = evaluate_number(
            &expr,
            &BTreeMap::new(),
            &NumericContext::default(),
            &limits(),
            &|name, _| Err(EngineError::new(ErrorCode::UnknownFunction, name)),
        )
        .unwrap();
        assert_eq!(result.to_string(), "1/2");
    }

    #[test]
    fn float_evaluation_calls_provider() {
        let calls = SimpleCalls::new().add("double", 1, |args| Ok(args[0] * 2.0));
        let expr = parse_expression("double(x) + 1", &limits()).unwrap();
        let mut bindings = BTreeMap::new();
        bindings.insert("x".to_string(), 3.0);
        let result = evaluate_f64(&expr, &bindings, &limits(), &calls.as_table()).unwrap();
        assert_eq!(result, 7.0);
    }

    #[test]
    fn unknown_function_is_structured() {
        let expr = parse_expression("nope(1)", &limits()).unwrap();
        let error = evaluate_f64(
            &expr,
            &BTreeMap::new(),
            &limits(),
            &SimpleCalls::new().as_table(),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::UnknownFunction);
    }
}
