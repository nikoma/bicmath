//! Shared AST helpers: work budgets, constructors, validation, and traversal.
//!
//! Everything here operates on [`bicmath_core::expr::Expr`]; the symbolic layer
//! never builds a parallel expression representation.

use std::collections::BTreeMap;

use bicmath_core::context::ExecContext;
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::expr::{BinaryOp, CallArg, Expr, UnaryOp};
use bicmath_core::limits::Limits;
use bicmath_core::number::Number;

/// Names the symbolic layer treats as built-in constants.
pub(crate) const CONSTANTS: [&str; 2] = ["pi", "e"];

/// Functions the symbolic layer knows how to differentiate and evaluate.
pub(crate) const SUPPORTED_FUNCTIONS: [&str; 17] = [
    "sin", "cos", "tan", "asin", "acos", "atan", "sinh", "cosh", "tanh", "exp", "ln", "log",
    "log10", "log2", "sqrt", "abs", "pow",
];

/// A cooperative work budget shared by every recursive AST walk.
pub(crate) struct Budget {
    used: u64,
    max: u64,
}

impl Budget {
    pub(crate) fn new(limits: &Limits) -> Budget {
        Budget {
            used: 0,
            max: limits.max_operations,
        }
    }

    pub(crate) fn tick(&mut self, ctx: &ExecContext) -> Result<(), EngineError> {
        self.used = self.used.saturating_add(1);
        if self.used > self.max {
            return Err(EngineError::new(
                ErrorCode::ResourceLimit,
                format!("symbolic operation budget of {} exceeded", self.max),
            ));
        }
        ctx.check()
    }
}

/// Build an expression from a number, normalizing negative values to an
/// explicit negation of the positive value so printed output stays unambiguous.
pub(crate) fn number_expr(value: Number) -> Expr {
    if value.is_negative() {
        Expr::Unary {
            op: UnaryOp::Neg,
            expr: Box::new(Expr::Number(value.neg())),
        }
    } else {
        Expr::Number(value)
    }
}

pub(crate) fn zero_expr() -> Expr {
    Expr::Number(Number::integer(0))
}

pub(crate) fn one_expr() -> Expr {
    Expr::Number(Number::integer(1))
}

/// Negate an expression, folding numeric literals and double negation.
pub(crate) fn negate(expr: Expr) -> Expr {
    match expr {
        Expr::Number(value) => number_expr(value.neg()),
        Expr::Unary {
            op: UnaryOp::Neg,
            expr,
        } => *expr,
        other => Expr::Unary {
            op: UnaryOp::Neg,
            expr: Box::new(other),
        },
    }
}

pub(crate) fn is_zero(expr: &Expr) -> bool {
    matches!(expr, Expr::Number(value) if value.is_zero())
}

pub(crate) fn is_one(expr: &Expr) -> bool {
    matches!(expr, Expr::Number(value) if value.numeric_eq(&Number::integer(1)).unwrap_or(false))
}

pub(crate) fn is_constant(name: &str) -> bool {
    CONSTANTS.contains(&name)
}

pub(crate) fn is_supported_function(name: &str) -> bool {
    SUPPORTED_FUNCTIONS.contains(&name)
}

pub(crate) fn binary(op: BinaryOp, left: Expr, right: Expr) -> Expr {
    Expr::Binary {
        op,
        left: Box::new(left),
        right: Box::new(right),
    }
}

pub(crate) fn add(left: Expr, right: Expr) -> Expr {
    binary(BinaryOp::Add, left, right)
}

pub(crate) fn sub(left: Expr, right: Expr) -> Expr {
    binary(BinaryOp::Sub, left, right)
}

pub(crate) fn mul(left: Expr, right: Expr) -> Expr {
    binary(BinaryOp::Mul, left, right)
}

pub(crate) fn div(left: Expr, right: Expr) -> Expr {
    binary(BinaryOp::Div, left, right)
}

pub(crate) fn pow(left: Expr, right: Expr) -> Expr {
    binary(BinaryOp::Pow, left, right)
}

pub(crate) fn call(name: &str, args: Vec<Expr>) -> Expr {
    Expr::Call {
        name: name.to_string(),
        args: args
            .into_iter()
            .map(|value| CallArg { name: None, value })
            .collect(),
    }
}

pub(crate) fn call1(name: &str, arg: Expr) -> Expr {
    call(name, vec![arg])
}

/// Validate a differentiation or substitution variable name.
pub(crate) fn validate_variable_name(name: &str) -> Result<(), EngineError> {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => {
            return Err(EngineError::malformed(format!(
                "invalid variable name {name:?}: expected an identifier"
            )));
        }
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(EngineError::malformed(format!(
            "invalid variable name {name:?}: expected an identifier"
        )));
    }
    if matches!(name, "true" | "false" | "and" | "or" | "not") {
        return Err(EngineError::domain(format!(
            "variable name {name:?} is a reserved keyword"
        )));
    }
    Ok(())
}

/// Reject expressions that are not single real-valued numeric expressions.
pub(crate) fn validate_numeric_expression(
    expr: &Expr,
    ctx: &ExecContext,
) -> Result<(), EngineError> {
    let mut budget = Budget::new(&ctx.limits);
    validate_numeric_inner(expr, ctx, &mut budget)
}

fn validate_numeric_inner(
    expr: &Expr,
    ctx: &ExecContext,
    budget: &mut Budget,
) -> Result<(), EngineError> {
    budget.tick(ctx)?;
    match expr {
        Expr::Number(_) | Expr::Ident(_) => Ok(()),
        Expr::Unary {
            op: UnaryOp::Neg | UnaryOp::Pos,
            expr,
        } => validate_numeric_inner(expr, ctx, budget),
        Expr::Unary {
            op: UnaryOp::Not, ..
        } => Err(EngineError::malformed(
            "logical negation is not valid in a symbolic numeric expression",
        )),
        Expr::Binary { op, left, right } => match op {
            BinaryOp::Add
            | BinaryOp::Sub
            | BinaryOp::Mul
            | BinaryOp::Div
            | BinaryOp::Rem
            | BinaryOp::Pow => {
                validate_numeric_inner(left, ctx, budget)?;
                validate_numeric_inner(right, ctx, budget)
            }
            BinaryOp::And | BinaryOp::Or => Err(EngineError::malformed(
                "logical operators are not valid in a symbolic numeric expression",
            )),
        },
        Expr::Call { args, .. } => {
            for arg in args {
                validate_numeric_inner(&arg.value, ctx, budget)?;
            }
            Ok(())
        }
        Expr::Compare { .. } | Expr::Text(_) | Expr::Bool(_) | Expr::Array(_) | Expr::Record(_) => {
            Err(EngineError::malformed(
                "symbolic functions require a single numeric expression",
            ))
        }
    }
}

/// Reject calls to functions the symbolic layer does not know.
pub(crate) fn validate_calls(expr: &Expr, ctx: &ExecContext) -> Result<(), EngineError> {
    let mut budget = Budget::new(&ctx.limits);
    validate_calls_inner(expr, ctx, &mut budget)
}

fn validate_calls_inner(
    expr: &Expr,
    ctx: &ExecContext,
    budget: &mut Budget,
) -> Result<(), EngineError> {
    budget.tick(ctx)?;
    match expr {
        Expr::Call { name, args } => {
            if !is_supported_function(name) {
                return Err(EngineError::new(
                    ErrorCode::UnknownFunction,
                    format!(
                        "unknown function {name:?}; supported functions: {}",
                        SUPPORTED_FUNCTIONS.join(", ")
                    ),
                ));
            }
            for arg in args {
                validate_calls_inner(&arg.value, ctx, budget)?;
            }
            Ok(())
        }
        Expr::Unary { expr, .. } => validate_calls_inner(expr, ctx, budget),
        Expr::Binary { left, right, .. } => {
            validate_calls_inner(left, ctx, budget)?;
            validate_calls_inner(right, ctx, budget)
        }
        Expr::Compare { operands, .. } => {
            for operand in operands {
                validate_calls_inner(operand, ctx, budget)?;
            }
            Ok(())
        }
        Expr::Array(items) => {
            for item in items {
                validate_calls_inner(item, ctx, budget)?;
            }
            Ok(())
        }
        Expr::Record(fields) => {
            for (_, value) in fields {
                validate_calls_inner(value, ctx, budget)?;
            }
            Ok(())
        }
        Expr::Number(_) | Expr::Text(_) | Expr::Bool(_) | Expr::Ident(_) => Ok(()),
    }
}

/// Collect the free variable names of an expression, excluding `pi` and `e`.
pub(crate) fn collect_free_variables(
    expr: &Expr,
    ctx: &ExecContext,
) -> Result<Vec<String>, EngineError> {
    let mut budget = Budget::new(&ctx.limits);
    let mut names = BTreeMap::new();
    collect_free_inner(expr, ctx, &mut budget, &mut names)?;
    Ok(names.into_keys().collect())
}

fn collect_free_inner(
    expr: &Expr,
    ctx: &ExecContext,
    budget: &mut Budget,
    names: &mut BTreeMap<String, ()>,
) -> Result<(), EngineError> {
    budget.tick(ctx)?;
    match expr {
        Expr::Ident(name) => {
            if !is_constant(name) {
                names.insert(name.clone(), ());
            }
        }
        Expr::Array(items) => {
            for item in items {
                collect_free_inner(item, ctx, budget, names)?;
            }
        }
        Expr::Record(fields) => {
            for (_, value) in fields {
                collect_free_inner(value, ctx, budget, names)?;
            }
        }
        Expr::Unary { expr, .. } => collect_free_inner(expr, ctx, budget, names)?,
        Expr::Binary { left, right, .. } => {
            collect_free_inner(left, ctx, budget, names)?;
            collect_free_inner(right, ctx, budget, names)?;
        }
        Expr::Compare { operands, .. } => {
            for operand in operands {
                collect_free_inner(operand, ctx, budget, names)?;
            }
        }
        Expr::Call { args, .. } => {
            for arg in args {
                collect_free_inner(&arg.value, ctx, budget, names)?;
            }
        }
        Expr::Number(_) | Expr::Text(_) | Expr::Bool(_) => {}
    }
    Ok(())
}

/// Structurally replace every reference to `variable` with `replacement`.
pub(crate) fn substitute(
    expr: &Expr,
    variable: &str,
    replacement: &Expr,
    ctx: &ExecContext,
) -> Result<Expr, EngineError> {
    let mut budget = Budget::new(&ctx.limits);
    substitute_inner(expr, variable, replacement, ctx, &mut budget)
}

fn substitute_inner(
    expr: &Expr,
    variable: &str,
    replacement: &Expr,
    ctx: &ExecContext,
    budget: &mut Budget,
) -> Result<Expr, EngineError> {
    budget.tick(ctx)?;
    match expr {
        Expr::Ident(name) if name == variable => Ok(replacement.clone()),
        Expr::Unary { op, expr } => Ok(Expr::Unary {
            op: *op,
            expr: Box::new(substitute_inner(expr, variable, replacement, ctx, budget)?),
        }),
        Expr::Binary { op, left, right } => Ok(Expr::Binary {
            op: *op,
            left: Box::new(substitute_inner(left, variable, replacement, ctx, budget)?),
            right: Box::new(substitute_inner(right, variable, replacement, ctx, budget)?),
        }),
        Expr::Compare { operands, ops } => Ok(Expr::Compare {
            operands: operands
                .iter()
                .map(|operand| substitute_inner(operand, variable, replacement, ctx, budget))
                .collect::<Result<Vec<_>, _>>()?,
            ops: ops.clone(),
        }),
        Expr::Call { name, args } => Ok(Expr::Call {
            name: name.clone(),
            args: args
                .iter()
                .map(|arg| {
                    Ok(CallArg {
                        name: arg.name.clone(),
                        value: substitute_inner(&arg.value, variable, replacement, ctx, budget)?,
                    })
                })
                .collect::<Result<Vec<_>, EngineError>>()?,
        }),
        Expr::Array(items) => Ok(Expr::Array(
            items
                .iter()
                .map(|item| substitute_inner(item, variable, replacement, ctx, budget))
                .collect::<Result<Vec<_>, _>>()?,
        )),
        Expr::Record(fields) => Ok(Expr::Record(
            fields
                .iter()
                .map(|(key, value)| {
                    Ok((
                        key.clone(),
                        substitute_inner(value, variable, replacement, ctx, budget)?,
                    ))
                })
                .collect::<Result<Vec<_>, EngineError>>()?,
        )),
        Expr::Number(_) | Expr::Text(_) | Expr::Bool(_) | Expr::Ident(_) => Ok(expr.clone()),
    }
}
