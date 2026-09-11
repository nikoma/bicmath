//! Exact constant folding and identity elimination for the restricted grammar.
//!
//! Simplification is deliberately conservative: only exact numeric operations
//! are folded, and any operation without an exact value is left symbolic. This
//! keeps `simplify` from silently introducing binary64 approximations.

use bicmath_core::context::ExecContext;
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::expr::{BinaryOp, CallArg, Expr, UnaryOp};
use bicmath_core::number::{Number, NumericContext, NumericMode};

use crate::ast::{self, Budget};
use crate::evaluate;

/// Simplify an expression under the caller's limits.
pub(crate) fn simplify(expr: &Expr, ctx: &ExecContext) -> Result<Expr, EngineError> {
    let mut budget = Budget::new(&ctx.limits);
    simplify_inner(expr, ctx, &mut budget)
}

fn exact_context(ctx: &ExecContext) -> NumericContext {
    let mut numeric = ctx.numeric.clone();
    numeric.mode = NumericMode::Exact;
    numeric
}

fn is_fatal(error: &EngineError) -> bool {
    matches!(error.code, ErrorCode::ResourceLimit | ErrorCode::Cancelled)
}

fn simplify_inner(
    expr: &Expr,
    ctx: &ExecContext,
    budget: &mut Budget,
) -> Result<Expr, EngineError> {
    budget.tick(ctx)?;
    match expr {
        Expr::Number(value) => Ok(ast::number_expr(value.clone())),
        Expr::Text(_) | Expr::Bool(_) | Expr::Ident(_) => Ok(expr.clone()),
        Expr::Array(items) => Ok(Expr::Array(
            items
                .iter()
                .map(|item| simplify_inner(item, ctx, budget))
                .collect::<Result<Vec<_>, _>>()?,
        )),
        Expr::Record(fields) => Ok(Expr::Record(
            fields
                .iter()
                .map(|(key, value)| Ok((key.clone(), simplify_inner(value, ctx, budget)?)))
                .collect::<Result<Vec<_>, EngineError>>()?,
        )),
        Expr::Call { name, args } => simplify_call(name, args, ctx, budget),
        Expr::Unary { op, expr: inner } => {
            let inner = simplify_inner(inner, ctx, budget)?;
            match op {
                UnaryOp::Pos => Ok(inner),
                UnaryOp::Neg => match inner {
                    Expr::Number(value) => Ok(ast::number_expr(value.neg())),
                    Expr::Unary {
                        op: UnaryOp::Neg,
                        expr,
                    } => Ok(*expr),
                    other => Ok(Expr::Unary {
                        op: UnaryOp::Neg,
                        expr: Box::new(other),
                    }),
                },
                UnaryOp::Not => Ok(Expr::Unary {
                    op: UnaryOp::Not,
                    expr: Box::new(inner),
                }),
            }
        }
        Expr::Binary { op, left, right } => {
            let left = simplify_inner(left, ctx, budget)?;
            let right = simplify_inner(right, ctx, budget)?;
            simplify_binary(*op, left, right, ctx, budget)
        }
        Expr::Compare { operands, ops } => Ok(Expr::Compare {
            operands: operands
                .iter()
                .map(|operand| simplify_inner(operand, ctx, budget))
                .collect::<Result<Vec<_>, _>>()?,
            ops: ops.clone(),
        }),
    }
}

fn simplify_call(
    name: &str,
    args: &[CallArg],
    ctx: &ExecContext,
    budget: &mut Budget,
) -> Result<Expr, EngineError> {
    let mut simplified = Vec::with_capacity(args.len());
    let mut all_numbers = true;
    for arg in args {
        let value = simplify_inner(&arg.value, ctx, budget)?;
        if arg.name.is_some() || !matches!(value, Expr::Number(_)) {
            all_numbers = false;
        }
        simplified.push(CallArg {
            name: arg.name.clone(),
            value,
        });
    }
    if all_numbers {
        let numbers: Vec<Number> = simplified
            .iter()
            .map(|arg| match &arg.value {
                Expr::Number(value) => value.clone(),
                _ => Number::integer(0),
            })
            .collect();
        match evaluate::exact_call(name, &numbers, ctx) {
            Ok(value) => return Ok(ast::number_expr(value)),
            Err(error) if is_fatal(&error) => return Err(error),
            Err(_) => {}
        }
    }
    Ok(Expr::Call {
        name: name.to_string(),
        args: simplified,
    })
}

fn simplify_binary(
    op: BinaryOp,
    left: Expr,
    right: Expr,
    ctx: &ExecContext,
    budget: &mut Budget,
) -> Result<Expr, EngineError> {
    match op {
        BinaryOp::Add | BinaryOp::Sub => {
            let sum = ast::binary(op, left, right);
            let mut terms = Vec::new();
            collect_sum(&sum, 1, &mut terms, ctx, budget)?;
            rebuild_sum(terms, ctx)
        }
        BinaryOp::Mul => {
            let product = ast::binary(BinaryOp::Mul, left, right);
            let mut constant = Number::integer(1);
            let mut factors = Vec::new();
            collect_product(&product, 1, &mut constant, &mut factors, ctx, budget)?;
            Ok(rebuild_product(constant, factors))
        }
        BinaryOp::Div => {
            if ast::is_one(&right) {
                return Ok(left);
            }
            if ast::is_zero(&left) && is_nonzero_literal(&right) {
                return Ok(ast::zero_expr());
            }
            if left == right && is_nonzero_literal(&left) {
                return Ok(ast::one_expr());
            }
            if let (Expr::Number(a), Expr::Number(b)) = (&left, &right)
                && let Some(value) = fold_binary(BinaryOp::Div, a, b, ctx)?
            {
                return Ok(ast::number_expr(value));
            }
            Ok(ast::binary(BinaryOp::Div, left, right))
        }
        BinaryOp::Rem => {
            if let (Expr::Number(a), Expr::Number(b)) = (&left, &right)
                && let Some(value) = fold_binary(BinaryOp::Rem, a, b, ctx)?
            {
                return Ok(ast::number_expr(value));
            }
            Ok(ast::binary(BinaryOp::Rem, left, right))
        }
        BinaryOp::Pow => {
            if let Expr::Number(exponent) = &right {
                if exponent.numeric_eq(&Number::integer(1)).unwrap_or(false) {
                    return Ok(left);
                }
                if exponent.is_zero() {
                    return Ok(ast::one_expr());
                }
            }
            if ast::is_one(&left) {
                return Ok(ast::one_expr());
            }
            if let (Expr::Number(base), Expr::Number(exponent)) = (&left, &right)
                && let Some(value) = fold_binary(BinaryOp::Pow, base, exponent, ctx)?
            {
                return Ok(ast::number_expr(value));
            }
            Ok(ast::binary(BinaryOp::Pow, left, right))
        }
        BinaryOp::And | BinaryOp::Or => Ok(ast::binary(op, left, right)),
    }
}

fn is_nonzero_literal(expr: &Expr) -> bool {
    matches!(expr, Expr::Number(value) if !value.is_zero())
}

/// Read a numeric constant from a term, including an explicit negation of a
/// positive literal (`-1` is stored as `Unary(Neg, Number(1))`).
fn constant_value(expr: &Expr) -> Option<Number> {
    match expr {
        Expr::Number(value) => Some(value.clone()),
        Expr::Unary {
            op: UnaryOp::Neg,
            expr,
        } => match &**expr {
            Expr::Number(value) => Some(value.neg()),
            _ => None,
        },
        _ => None,
    }
}

fn fold_binary(
    op: BinaryOp,
    a: &Number,
    b: &Number,
    ctx: &ExecContext,
) -> Result<Option<Number>, EngineError> {
    let numeric = exact_context(ctx);
    let outcome = match op {
        BinaryOp::Add => a.add(b, &numeric, &ctx.limits),
        BinaryOp::Sub => a.sub(b, &numeric, &ctx.limits),
        BinaryOp::Mul => a.mul(b, &numeric, &ctx.limits),
        BinaryOp::Div => a.div(b, &numeric, &ctx.limits),
        BinaryOp::Rem => a.rem(b, &numeric, &ctx.limits),
        BinaryOp::Pow => a.pow(b, &numeric, &ctx.limits),
        BinaryOp::And | BinaryOp::Or => return Ok(None),
    };
    match outcome {
        Ok(result) => Ok(Some(result.value)),
        Err(error) if is_fatal(&error) => Err(error),
        Err(_) => Ok(None),
    }
}

fn fold_mul(
    constant: &Number,
    value: &Number,
    ctx: &ExecContext,
) -> Result<Option<Number>, EngineError> {
    let numeric = exact_context(ctx);
    match constant.mul(value, &numeric, &ctx.limits) {
        Ok(result) => Ok(Some(result.value)),
        Err(error) if is_fatal(&error) => Err(error),
        Err(_) => Ok(None),
    }
}

fn signed_number(value: &Number, sign: i32) -> Expr {
    if sign >= 0 {
        ast::number_expr(value.clone())
    } else {
        ast::number_expr(value.neg())
    }
}

fn collect_sum(
    expr: &Expr,
    sign: i32,
    terms: &mut Vec<Expr>,
    ctx: &ExecContext,
    budget: &mut Budget,
) -> Result<(), EngineError> {
    budget.tick(ctx)?;
    match expr {
        Expr::Binary {
            op: BinaryOp::Add,
            left,
            right,
        } => {
            collect_sum(left, sign, terms, ctx, budget)?;
            collect_sum(right, sign, terms, ctx, budget)
        }
        Expr::Binary {
            op: BinaryOp::Sub,
            left,
            right,
        } => {
            collect_sum(left, sign, terms, ctx, budget)?;
            collect_sum(right, -sign, terms, ctx, budget)
        }
        Expr::Unary {
            op: UnaryOp::Neg,
            expr,
        } => collect_sum(expr, -sign, terms, ctx, budget),
        Expr::Number(value) => {
            terms.push(signed_number(value, sign));
            Ok(())
        }
        other => {
            terms.push(if sign >= 0 {
                other.clone()
            } else {
                ast::negate(other.clone())
            });
            Ok(())
        }
    }
}

fn split_negative(term: &Expr) -> Option<Expr> {
    match term {
        Expr::Unary {
            op: UnaryOp::Neg,
            expr,
        } => Some((**expr).clone()),
        Expr::Binary {
            op: BinaryOp::Mul,
            left,
            right,
        } => match &**left {
            Expr::Unary {
                op: UnaryOp::Neg,
                expr,
            } => Some(ast::binary(
                BinaryOp::Mul,
                (**expr).clone(),
                (**right).clone(),
            )),
            _ => None,
        },
        _ => None,
    }
}

fn rebuild_sum(terms: Vec<Expr>, ctx: &ExecContext) -> Result<Expr, EngineError> {
    let mut used = vec![false; terms.len()];
    let mut kept: Vec<Expr> = Vec::new();
    for (index, term) in terms.iter().enumerate() {
        if used[index] {
            continue;
        }
        used[index] = true;
        let opposite = ast::negate(term.clone());
        let mut cancelled = false;
        for (other_index, candidate) in terms.iter().enumerate().skip(index + 1) {
            if !used[other_index] && *candidate == opposite {
                used[other_index] = true;
                cancelled = true;
                break;
            }
        }
        if !cancelled {
            kept.push(term.clone());
        }
    }

    let numeric = exact_context(ctx);
    let mut constant = Number::integer(0);
    let mut has_constant = false;
    let mut constant_index = 0usize;
    let mut rest: Vec<Expr> = Vec::new();
    for term in kept {
        match constant_value(&term) {
            Some(value) => match constant.add(&value, &numeric, &ctx.limits) {
                Ok(result) => {
                    constant = result.value;
                    if !has_constant {
                        constant_index = rest.len();
                        has_constant = true;
                    }
                }
                Err(error) if is_fatal(&error) => return Err(error),
                Err(_) => rest.push(term),
            },
            None => rest.push(term),
        }
    }
    if has_constant && !constant.is_zero() {
        rest.insert(constant_index, ast::number_expr(constant));
    }

    let mut iter = rest.into_iter();
    let Some(mut accumulator) = iter.next() else {
        return Ok(ast::zero_expr());
    };
    for term in iter {
        accumulator = match split_negative(&term) {
            Some(positive) => ast::sub(accumulator, positive),
            None => ast::add(accumulator, term),
        };
    }
    Ok(accumulator)
}

fn collect_product(
    expr: &Expr,
    sign: i32,
    constant: &mut Number,
    factors: &mut Vec<Expr>,
    ctx: &ExecContext,
    budget: &mut Budget,
) -> Result<(), EngineError> {
    budget.tick(ctx)?;
    match expr {
        Expr::Binary {
            op: BinaryOp::Mul,
            left,
            right,
        } => {
            collect_product(left, sign, constant, factors, ctx, budget)?;
            collect_product(right, sign, constant, factors, ctx, budget)
        }
        Expr::Unary {
            op: UnaryOp::Neg,
            expr,
        } => collect_product(expr, -sign, constant, factors, ctx, budget),
        Expr::Number(value) => {
            let signed = if sign >= 0 {
                value.clone()
            } else {
                value.neg()
            };
            match fold_mul(constant, &signed, ctx)? {
                Some(next) => {
                    *constant = next;
                    Ok(())
                }
                None => {
                    factors.push(ast::number_expr(signed));
                    Ok(())
                }
            }
        }
        other => {
            factors.push(if sign >= 0 {
                other.clone()
            } else {
                ast::negate(other.clone())
            });
            Ok(())
        }
    }
}

fn rebuild_product(constant: Number, factors: Vec<Expr>) -> Expr {
    if constant.is_zero() {
        return ast::zero_expr();
    }
    let mut parts = Vec::with_capacity(factors.len() + 1);
    if !constant.numeric_eq(&Number::integer(1)).unwrap_or(false) {
        parts.push(ast::number_expr(constant));
    }
    parts.extend(factors);
    let mut iter = parts.into_iter();
    let Some(mut accumulator) = iter.next() else {
        return ast::one_expr();
    };
    for factor in iter {
        accumulator = ast::mul(accumulator, factor);
    }
    accumulator
}
