//! Symbolic differentiation over the restricted expression grammar.
//!
//! Supported: `+ - * / ^`, unary sign, and the calls listed in
//! [`crate::ast::SUPPORTED_FUNCTIONS`]. `pi` and `e` are constants unless they
//! are the differentiation variable. The result is an expression, not a string;
//! callers simplify and print it.

use bicmath_core::context::ExecContext;
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::expr::{BinaryOp, CallArg, Expr, UnaryOp};
use bicmath_core::number::Number;

use crate::ast::{self, Budget};

/// Differentiate `expr` with respect to `variable`.
pub(crate) fn differentiate(
    expr: &Expr,
    variable: &str,
    ctx: &ExecContext,
) -> Result<Expr, EngineError> {
    let mut budget = Budget::new(&ctx.limits);
    differentiate_inner(expr, variable, ctx, &mut budget)
}

fn differentiate_inner(
    expr: &Expr,
    variable: &str,
    ctx: &ExecContext,
    budget: &mut Budget,
) -> Result<Expr, EngineError> {
    budget.tick(ctx)?;
    match expr {
        Expr::Number(_) => Ok(ast::zero_expr()),
        Expr::Ident(name) => Ok(if name == variable {
            ast::one_expr()
        } else {
            ast::zero_expr()
        }),
        Expr::Unary { op, expr } => match op {
            UnaryOp::Neg => Ok(ast::negate(differentiate_inner(
                expr, variable, ctx, budget,
            )?)),
            UnaryOp::Pos => differentiate_inner(expr, variable, ctx, budget),
            UnaryOp::Not => Err(EngineError::malformed(
                "logical negation is not differentiable",
            )),
        },
        Expr::Binary { op, left, right } => {
            differentiate_binary(*op, left, right, variable, ctx, budget)
        }
        Expr::Call { name, args } => differentiate_call(name, args, variable, ctx, budget),
        Expr::Compare { .. } => Err(EngineError::malformed("comparisons are not differentiable")),
        Expr::Text(_) | Expr::Bool(_) | Expr::Array(_) | Expr::Record(_) => Err(
            EngineError::malformed("symbolic functions require a numeric expression"),
        ),
    }
}

fn differentiate_binary(
    op: BinaryOp,
    left: &Expr,
    right: &Expr,
    variable: &str,
    ctx: &ExecContext,
    budget: &mut Budget,
) -> Result<Expr, EngineError> {
    match op {
        BinaryOp::Add => Ok(ast::add(
            differentiate_inner(left, variable, ctx, budget)?,
            differentiate_inner(right, variable, ctx, budget)?,
        )),
        BinaryOp::Sub => Ok(ast::sub(
            differentiate_inner(left, variable, ctx, budget)?,
            differentiate_inner(right, variable, ctx, budget)?,
        )),
        BinaryOp::Mul => {
            let d_left = differentiate_inner(left, variable, ctx, budget)?;
            let d_right = differentiate_inner(right, variable, ctx, budget)?;
            Ok(ast::add(
                ast::mul(d_left, right.clone()),
                ast::mul(left.clone(), d_right),
            ))
        }
        BinaryOp::Div => {
            let d_left = differentiate_inner(left, variable, ctx, budget)?;
            let d_right = differentiate_inner(right, variable, ctx, budget)?;
            let numerator = ast::sub(
                ast::mul(d_left, right.clone()),
                ast::mul(left.clone(), d_right),
            );
            Ok(ast::div(
                numerator,
                ast::pow(right.clone(), ast::number_expr(Number::integer(2))),
            ))
        }
        BinaryOp::Pow => differentiate_power(left, right, variable, ctx, budget),
        BinaryOp::Rem => Err(EngineError::unsupported(
            "the remainder operator is not differentiable",
        )),
        BinaryOp::And | BinaryOp::Or => Err(EngineError::malformed(
            "logical operators are not differentiable",
        )),
    }
}

fn differentiate_power(
    left: &Expr,
    right: &Expr,
    variable: &str,
    ctx: &ExecContext,
    budget: &mut Budget,
) -> Result<Expr, EngineError> {
    if !contains_variable(right, variable, ctx, budget)? {
        // Power rule: d(u^v) = v * u^(v-1) * u'.
        let d_left = differentiate_inner(left, variable, ctx, budget)?;
        let inner = ast::mul(
            ast::pow(left.clone(), ast::sub(right.clone(), ast::one_expr())),
            d_left,
        );
        Ok(ast::mul(right.clone(), inner))
    } else if !contains_variable(left, variable, ctx, budget)? {
        // Exponential rule: d(u^v) = u^v * ln(u) * v'.
        let d_right = differentiate_inner(right, variable, ctx, budget)?;
        let inner = ast::mul(ast::call1("ln", left.clone()), d_right);
        Ok(ast::mul(ast::pow(left.clone(), right.clone()), inner))
    } else {
        // General rule: d(u^v) = u^v * (v' * ln(u) + v * u' / u).
        let d_left = differentiate_inner(left, variable, ctx, budget)?;
        let d_right = differentiate_inner(right, variable, ctx, budget)?;
        let term = ast::add(
            ast::mul(d_right, ast::call1("ln", left.clone())),
            ast::div(ast::mul(right.clone(), d_left), left.clone()),
        );
        Ok(ast::mul(ast::pow(left.clone(), right.clone()), term))
    }
}

fn differentiate_call(
    name: &str,
    args: &[CallArg],
    variable: &str,
    ctx: &ExecContext,
    budget: &mut Budget,
) -> Result<Expr, EngineError> {
    for arg in args {
        if arg.name.is_some() {
            return Err(EngineError::malformed(
                "named arguments are not supported in symbolic differentiation",
            ));
        }
    }
    if !ast::is_supported_function(name) {
        return Err(EngineError::new(
            ErrorCode::UnknownFunction,
            format!(
                "unknown function {name:?}; supported functions: {}",
                ast::SUPPORTED_FUNCTIONS.join(", ")
            ),
        ));
    }
    let expected_arity = if matches!(name, "log" | "pow") { 2 } else { 1 };
    if args.len() != expected_arity {
        return Err(EngineError::malformed(format!(
            "function {name} expects {expected_arity} argument(s), found {}",
            args.len()
        )));
    }
    match name {
        "sin" => {
            let (u, d_u) = unary_chain(args, variable, ctx, budget)?;
            Ok(ast::mul(d_u, ast::call1("cos", u)))
        }
        "cos" => {
            let (u, d_u) = unary_chain(args, variable, ctx, budget)?;
            Ok(ast::negate(ast::mul(d_u, ast::call1("sin", u))))
        }
        "tan" => {
            let (u, d_u) = unary_chain(args, variable, ctx, budget)?;
            Ok(ast::div(
                d_u,
                ast::pow(ast::call1("cos", u), ast::number_expr(Number::integer(2))),
            ))
        }
        "asin" => {
            let (u, d_u) = unary_chain(args, variable, ctx, budget)?;
            Ok(ast::div(d_u, sqrt_one_minus_square(&u)))
        }
        "acos" => {
            let (u, d_u) = unary_chain(args, variable, ctx, budget)?;
            Ok(ast::negate(ast::div(d_u, sqrt_one_minus_square(&u))))
        }
        "atan" => {
            let (u, d_u) = unary_chain(args, variable, ctx, budget)?;
            Ok(ast::div(d_u, one_plus_square(&u)))
        }
        "sinh" => {
            let (u, d_u) = unary_chain(args, variable, ctx, budget)?;
            Ok(ast::mul(d_u, ast::call1("cosh", u)))
        }
        "cosh" => {
            let (u, d_u) = unary_chain(args, variable, ctx, budget)?;
            Ok(ast::mul(d_u, ast::call1("sinh", u)))
        }
        "tanh" => {
            let (u, d_u) = unary_chain(args, variable, ctx, budget)?;
            Ok(ast::div(
                d_u,
                ast::pow(ast::call1("cosh", u), ast::number_expr(Number::integer(2))),
            ))
        }
        "exp" => {
            let (u, d_u) = unary_chain(args, variable, ctx, budget)?;
            Ok(ast::mul(d_u, ast::call1("exp", u)))
        }
        "ln" => {
            let (u, d_u) = unary_chain(args, variable, ctx, budget)?;
            Ok(ast::div(d_u, u))
        }
        "log10" => {
            let (u, d_u) = unary_chain(args, variable, ctx, budget)?;
            Ok(ast::div(
                d_u,
                ast::mul(u, ast::call1("ln", ast::number_expr(Number::integer(10)))),
            ))
        }
        "log2" => {
            let (u, d_u) = unary_chain(args, variable, ctx, budget)?;
            Ok(ast::div(
                d_u,
                ast::mul(u, ast::call1("ln", ast::number_expr(Number::integer(2)))),
            ))
        }
        "sqrt" => {
            let (u, d_u) = unary_chain(args, variable, ctx, budget)?;
            Ok(ast::div(
                d_u,
                ast::mul(ast::number_expr(Number::integer(2)), ast::call1("sqrt", u)),
            ))
        }
        "abs" => {
            let (u, d_u) = unary_chain(args, variable, ctx, budget)?;
            Ok(ast::mul(d_u, ast::div(u.clone(), ast::call1("abs", u))))
        }
        "log" => {
            let value = args[0].value.clone();
            let base = args[1].value.clone();
            let d_value = differentiate_inner(&value, variable, ctx, budget)?;
            let d_base = differentiate_inner(&base, variable, ctx, budget)?;
            let ln_value = ast::call1("ln", value.clone());
            let ln_base = ast::call1("ln", base.clone());
            let numerator = ast::sub(
                ast::mul(ast::div(d_value, value), ln_base.clone()),
                ast::mul(ln_value, ast::div(d_base, base)),
            );
            Ok(ast::div(
                numerator,
                ast::pow(ln_base, ast::number_expr(Number::integer(2))),
            ))
        }
        "pow" => {
            let expression = ast::pow(args[0].value.clone(), args[1].value.clone());
            differentiate_inner(&expression, variable, ctx, budget)
        }
        other => Err(EngineError::new(
            ErrorCode::UnknownFunction,
            format!("unknown function {other:?}"),
        )),
    }
}

fn unary_chain(
    args: &[CallArg],
    variable: &str,
    ctx: &ExecContext,
    budget: &mut Budget,
) -> Result<(Expr, Expr), EngineError> {
    let u = args[0].value.clone();
    let d_u = differentiate_inner(&u, variable, ctx, budget)?;
    Ok((u, d_u))
}

fn sqrt_one_minus_square(u: &Expr) -> Expr {
    ast::call1(
        "sqrt",
        ast::sub(
            ast::one_expr(),
            ast::pow(u.clone(), ast::number_expr(Number::integer(2))),
        ),
    )
}

fn one_plus_square(u: &Expr) -> Expr {
    ast::add(
        ast::one_expr(),
        ast::pow(u.clone(), ast::number_expr(Number::integer(2))),
    )
}

fn contains_variable(
    expr: &Expr,
    variable: &str,
    ctx: &ExecContext,
    budget: &mut Budget,
) -> Result<bool, EngineError> {
    budget.tick(ctx)?;
    match expr {
        Expr::Ident(name) => Ok(name == variable),
        Expr::Unary { expr, .. } => contains_variable(expr, variable, ctx, budget),
        Expr::Binary { left, right, .. } => {
            if contains_variable(left, variable, ctx, budget)? {
                return Ok(true);
            }
            contains_variable(right, variable, ctx, budget)
        }
        Expr::Compare { operands, .. } => {
            for operand in operands {
                if contains_variable(operand, variable, ctx, budget)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Expr::Call { args, .. } => {
            for arg in args {
                if contains_variable(&arg.value, variable, ctx, budget)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Expr::Array(items) => {
            for item in items {
                if contains_variable(item, variable, ctx, budget)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Expr::Record(fields) => {
            for (_, value) in fields {
                if contains_variable(value, variable, ctx, budget)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Expr::Number(_) | Expr::Text(_) | Expr::Bool(_) => Ok(false),
    }
}
