//! Taylor expansion with exact rational coefficients when possible.
//!
//! Coefficients are `f^(k)(at) / k!`. Each derivative is evaluated exactly
//! first; when any coefficient has no exact rational value, the whole expansion
//! falls back to binary64 and is labelled approximate by the caller.

use std::collections::BTreeMap;

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{One, ToPrimitive};

use bicmath_core::context::ExecContext;
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::expr::Expr;
use bicmath_core::number::{Number, NumericMode};

use crate::ast;
use crate::differentiate;
use crate::evaluate;
use crate::simplify;

/// The result of a Taylor expansion.
pub(crate) struct TaylorExpansion {
    pub(crate) polynomial: Expr,
    pub(crate) coefficients: Vec<Number>,
    pub(crate) approximate: bool,
}

enum ExactAttempt {
    Coefficients(Vec<Number>),
    Approximate,
    Fatal(EngineError),
}

/// Expand `expr` around `at` up to `order` terms.
pub(crate) fn taylor(
    expr: &Expr,
    variable: &str,
    at: &Number,
    order: u32,
    ctx: &ExecContext,
) -> Result<TaylorExpansion, EngineError> {
    let mut derivatives = Vec::with_capacity(order as usize + 1);
    let mut current = expr.clone();
    for index in 0..=order {
        ctx.check()?;
        if index > 0 {
            current = differentiate::differentiate(&current, variable, ctx)?;
        }
        derivatives.push(simplify::simplify(&current, ctx)?);
    }

    match exact_coefficients(&derivatives, variable, at, ctx) {
        ExactAttempt::Coefficients(coefficients) => {
            let polynomial = build_polynomial(&coefficients, variable, at, ctx)?;
            Ok(TaylorExpansion {
                polynomial,
                coefficients,
                approximate: false,
            })
        }
        ExactAttempt::Fatal(error) => Err(error),
        ExactAttempt::Approximate => {
            if ctx.numeric.mode == NumericMode::Exact {
                return Err(EngineError::new(
                    ErrorCode::UnsupportedNumericMode,
                    "the expansion point has no exact rational value at this order; \
                     use auto or scientific mode for an approximate expansion",
                ));
            }
            let coefficients = float_coefficients(&derivatives, variable, at, ctx)?;
            let polynomial = build_polynomial(&coefficients, variable, at, ctx)?;
            Ok(TaylorExpansion {
                polynomial,
                coefficients,
                approximate: true,
            })
        }
    }
}

fn exact_coefficients(
    derivatives: &[Expr],
    variable: &str,
    at: &Number,
    ctx: &ExecContext,
) -> ExactAttempt {
    if matches!(at, Number::Float64(_)) {
        return ExactAttempt::Approximate;
    }
    let mut bindings = BTreeMap::new();
    bindings.insert(variable.to_string(), at.clone());
    let mut coefficients = Vec::with_capacity(derivatives.len());
    let mut factorial = BigInt::one();
    for (index, derivative) in derivatives.iter().enumerate() {
        if let Err(error) = ctx.check() {
            return ExactAttempt::Fatal(error);
        }
        if index > 0 {
            factorial *= BigInt::from(index);
        }
        let value = match evaluate::evaluate_exact(derivative, &bindings, ctx) {
            Ok(value) => value,
            Err(error) => {
                return match error.code {
                    ErrorCode::UnsupportedOperation
                    | ErrorCode::UnsupportedNumericMode
                    | ErrorCode::NotFound => ExactAttempt::Approximate,
                    _ => ExactAttempt::Fatal(error),
                };
            }
        };
        let Some(rational) = value.to_exact_rational() else {
            return ExactAttempt::Approximate;
        };
        let coefficient = rational / BigRational::from_integer(factorial.clone());
        coefficients.push(rational_number(coefficient));
    }
    ExactAttempt::Coefficients(coefficients)
}

fn float_coefficients(
    derivatives: &[Expr],
    variable: &str,
    at: &Number,
    ctx: &ExecContext,
) -> Result<Vec<Number>, EngineError> {
    let at_value = at.to_f64().ok_or_else(|| {
        EngineError::domain("the expansion point is not representable as float64")
    })?;
    let mut bindings = BTreeMap::new();
    bindings.insert("pi".to_string(), std::f64::consts::PI);
    bindings.insert("e".to_string(), std::f64::consts::E);
    bindings.insert(variable.to_string(), at_value);
    let mut coefficients = Vec::with_capacity(derivatives.len());
    let mut factorial = BigInt::one();
    for (index, derivative) in derivatives.iter().enumerate() {
        ctx.check()?;
        if index > 0 {
            factorial *= BigInt::from(index);
        }
        let value = evaluate::evaluate_float(derivative, &bindings, ctx)?;
        let rational = Number::float(value)?
            .to_exact_rational()
            .ok_or_else(|| EngineError::internal("float value is not representable exactly"))?;
        let coefficient = rational / BigRational::from_integer(factorial.clone());
        let coefficient = coefficient.to_f64().ok_or_else(|| {
            EngineError::domain("taylor coefficient is not representable as float64")
        })?;
        coefficients.push(Number::float(coefficient)?);
    }
    Ok(coefficients)
}

fn build_polynomial(
    coefficients: &[Number],
    variable: &str,
    at: &Number,
    ctx: &ExecContext,
) -> Result<Expr, EngineError> {
    let variable_expr = Expr::Ident(variable.to_string());
    let offset = if at.is_zero() {
        variable_expr
    } else {
        ast::sub(variable_expr, ast::number_expr(at.clone()))
    };
    let mut terms = Vec::new();
    for (index, coefficient) in coefficients.iter().enumerate() {
        if coefficient.is_zero() {
            continue;
        }
        let term = if index == 0 {
            ast::number_expr(coefficient.clone())
        } else {
            let power = if index == 1 {
                offset.clone()
            } else {
                ast::pow(offset.clone(), ast::number_expr(Number::integer(index)))
            };
            ast::mul(ast::number_expr(coefficient.clone()), power)
        };
        terms.push(term);
    }
    let mut iter = terms.into_iter();
    let Some(mut accumulator) = iter.next() else {
        return Ok(ast::zero_expr());
    };
    for term in iter {
        accumulator = ast::add(accumulator, term);
    }
    simplify::simplify(&accumulator, ctx)
}

fn rational_number(value: BigRational) -> Number {
    if value.is_integer() {
        Number::Integer(value.to_integer())
    } else {
        Number::Rational(value)
    }
}
