//! Exact and binary64 evaluation tables for the restricted expression grammar.
//!
//! Exact evaluation is attempted first for Taylor coefficients. It only
//! succeeds when every operation has an exact rational value; functions without
//! an exact value at the requested point return
//! [`ErrorCode::UnsupportedOperation`], which the caller treats as a signal to
//! fall back to binary64.

use std::collections::BTreeMap;

use num_rational::BigRational;
use num_traits::{One, Zero};

use bicmath_core::context::ExecContext;
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::eval::{SimpleCalls, evaluate_f64, evaluate_number};
use bicmath_core::expr::Expr;
use bicmath_core::number::{Number, NumericMode};

/// Largest exponent searched when recognising an exact integer logarithm.
const MAX_LOG_SEARCH: u32 = 64;

/// Evaluate an expression exactly under the caller's limits.
pub(crate) fn evaluate_exact(
    expr: &Expr,
    bindings: &BTreeMap<String, Number>,
    ctx: &ExecContext,
) -> Result<Number, EngineError> {
    let mut numeric = ctx.numeric.clone();
    numeric.mode = NumericMode::Exact;
    let calls = |name: &str, args: &[Number]| exact_call(name, args, ctx);
    evaluate_number(expr, bindings, &numeric, &ctx.limits, &calls)
}

/// Evaluate an expression in binary64 under the caller's limits.
pub(crate) fn evaluate_float(
    expr: &Expr,
    bindings: &BTreeMap<String, f64>,
    ctx: &ExecContext,
) -> Result<f64, EngineError> {
    let calls = float_calls();
    evaluate_f64(expr, bindings, &ctx.limits, &calls.as_table())
}

/// Exact evaluation of a single call.
pub(crate) fn exact_call(
    name: &str,
    args: &[Number],
    ctx: &ExecContext,
) -> Result<Number, EngineError> {
    match name {
        "abs" => {
            expect_arity(name, args, 1)?;
            Ok(args[0].abs())
        }
        "sqrt" => {
            expect_arity(name, args, 1)?;
            exact_sqrt(&args[0])
        }
        "pow" => {
            expect_arity(name, args, 2)?;
            exact_pow(&args[0], &args[1], ctx)
        }
        "sin" => {
            expect_arity(name, args, 1)?;
            exact_if(name, args[0].is_zero(), 0)
        }
        "cos" => {
            expect_arity(name, args, 1)?;
            exact_if(name, args[0].is_zero(), 1)
        }
        "tan" => {
            expect_arity(name, args, 1)?;
            exact_if(name, args[0].is_zero(), 0)
        }
        "asin" => {
            expect_arity(name, args, 1)?;
            exact_if(name, args[0].is_zero(), 0)
        }
        "acos" => {
            expect_arity(name, args, 1)?;
            exact_if(name, is_integer_value(&args[0], 1), 0)
        }
        "atan" => {
            expect_arity(name, args, 1)?;
            exact_if(name, args[0].is_zero(), 0)
        }
        "sinh" => {
            expect_arity(name, args, 1)?;
            exact_if(name, args[0].is_zero(), 0)
        }
        "cosh" => {
            expect_arity(name, args, 1)?;
            exact_if(name, args[0].is_zero(), 1)
        }
        "tanh" => {
            expect_arity(name, args, 1)?;
            exact_if(name, args[0].is_zero(), 0)
        }
        "exp" => {
            expect_arity(name, args, 1)?;
            exact_if(name, args[0].is_zero(), 1)
        }
        "ln" => {
            expect_arity(name, args, 1)?;
            exact_if(name, is_integer_value(&args[0], 1), 0)
        }
        "log10" => {
            expect_arity(name, args, 1)?;
            exact_if(name, is_integer_value(&args[0], 1), 0)
        }
        "log2" => {
            expect_arity(name, args, 1)?;
            exact_if(name, is_integer_value(&args[0], 1), 0)
        }
        "log" => {
            expect_arity(name, args, 2)?;
            exact_log(&args[0], &args[1])
        }
        other => Err(EngineError::new(
            ErrorCode::UnknownFunction,
            format!("unknown function {other:?}"),
        )),
    }
}

fn expect_arity(name: &str, args: &[Number], expected: usize) -> Result<(), EngineError> {
    if args.len() == expected {
        Ok(())
    } else {
        Err(EngineError::malformed(format!(
            "function {name} expects {expected} argument(s), found {}",
            args.len()
        )))
    }
}

fn no_exact(name: &str) -> EngineError {
    EngineError::new(
        ErrorCode::UnsupportedOperation,
        format!("no exact rational value for {name} at this point"),
    )
}

fn exact_if(name: &str, condition: bool, value: i64) -> Result<Number, EngineError> {
    if condition {
        Ok(Number::integer(value))
    } else {
        Err(no_exact(name))
    }
}

fn is_integer_value(number: &Number, value: i64) -> bool {
    number.numeric_eq(&Number::integer(value)).unwrap_or(false)
}

fn exact_sqrt(value: &Number) -> Result<Number, EngineError> {
    if value.is_negative() {
        return Err(EngineError::domain("square root of a negative number"));
    }
    match value {
        Number::Integer(integer) => {
            let root = integer.sqrt();
            if &root * &root == *integer {
                Ok(Number::Integer(root))
            } else {
                Err(no_exact("sqrt"))
            }
        }
        Number::Rational(rational) => {
            let numer = rational.numer().sqrt();
            let denom = rational.denom().sqrt();
            if &numer * &numer == *rational.numer() && &denom * &denom == *rational.denom() {
                Ok(Number::Rational(BigRational::new(numer, denom)))
            } else {
                Err(no_exact("sqrt"))
            }
        }
        Number::Decimal(decimal) => match decimal.sqrt_exact() {
            Some(root) => Ok(Number::Decimal(root)),
            None => Err(no_exact("sqrt")),
        },
        Number::Float64(_) => Err(no_exact("sqrt")),
    }
}

fn exact_pow(base: &Number, exponent: &Number, ctx: &ExecContext) -> Result<Number, EngineError> {
    let Some(rational) = exponent.to_exact_rational() else {
        return Err(no_exact("pow"));
    };
    if !rational.is_integer() {
        return Err(no_exact("pow"));
    }
    let mut numeric = ctx.numeric.clone();
    numeric.mode = NumericMode::Exact;
    Ok(base.pow(exponent, &numeric, &ctx.limits)?.value)
}

fn exact_log(value: &Number, base: &Number) -> Result<Number, EngineError> {
    if value.is_zero() || value.is_negative() {
        return Err(EngineError::domain("log requires x > 0"));
    }
    if base.is_zero() || base.is_negative() {
        return Err(EngineError::domain("log requires base > 0"));
    }
    if is_integer_value(base, 1) {
        return Err(EngineError::domain("log requires base != 1"));
    }
    if is_integer_value(value, 1) {
        return Ok(Number::integer(0));
    }
    let (Some(value_rational), Some(base_rational)) =
        (value.to_exact_rational(), base.to_exact_rational())
    else {
        return Err(no_exact("log"));
    };
    let mut power = BigRational::one();
    for exponent in 0..=MAX_LOG_SEARCH {
        if power == value_rational {
            return Ok(Number::integer(exponent));
        }
        if !power.is_zero() && BigRational::one() / &power == value_rational {
            return Ok(Number::integer(-(exponent as i64)));
        }
        power *= &base_rational;
    }
    Err(no_exact("log"))
}

fn float_calls() -> SimpleCalls {
    SimpleCalls::new()
        .add("sin", 1, |args| Ok(libm::sin(args[0])))
        .add("cos", 1, |args| Ok(libm::cos(args[0])))
        .add("tan", 1, |args| Ok(libm::tan(args[0])))
        .add("asin", 1, |args| {
            if args[0].abs() > 1.0 {
                Err(EngineError::domain("asin requires |x| <= 1"))
            } else {
                Ok(libm::asin(args[0]))
            }
        })
        .add("acos", 1, |args| {
            if args[0].abs() > 1.0 {
                Err(EngineError::domain("acos requires |x| <= 1"))
            } else {
                Ok(libm::acos(args[0]))
            }
        })
        .add("atan", 1, |args| Ok(libm::atan(args[0])))
        .add("sinh", 1, |args| Ok(libm::sinh(args[0])))
        .add("cosh", 1, |args| Ok(libm::cosh(args[0])))
        .add("tanh", 1, |args| Ok(libm::tanh(args[0])))
        .add("exp", 1, |args| Ok(libm::exp(args[0])))
        .add("ln", 1, |args| {
            if args[0] <= 0.0 {
                Err(EngineError::domain("ln requires x > 0"))
            } else {
                Ok(libm::log(args[0]))
            }
        })
        .add("log", 2, |args| {
            let (value, base) = (args[0], args[1]);
            if value <= 0.0 {
                return Err(EngineError::domain("log requires x > 0"));
            }
            if base <= 0.0 || base == 1.0 {
                return Err(EngineError::domain("log requires base > 0 and base != 1"));
            }
            Ok(libm::log(value) / libm::log(base))
        })
        .add("log10", 1, |args| {
            if args[0] <= 0.0 {
                Err(EngineError::domain("log10 requires x > 0"))
            } else {
                Ok(libm::log10(args[0]))
            }
        })
        .add("log2", 1, |args| {
            if args[0] <= 0.0 {
                Err(EngineError::domain("log2 requires x > 0"))
            } else {
                Ok(libm::log2(args[0]))
            }
        })
        .add("sqrt", 1, |args| {
            if args[0] < 0.0 {
                Err(EngineError::domain("sqrt requires x >= 0"))
            } else {
                Ok(libm::sqrt(args[0]))
            }
        })
        .add("abs", 1, |args| Ok(libm::fabs(args[0])))
        .add("pow", 2, |args| Ok(libm::pow(args[0], args[1])))
}
