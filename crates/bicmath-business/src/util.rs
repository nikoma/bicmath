//! Shared helpers for the business module.
//!
//! Money-like quantities (prices, costs, ARPU, CAC, cohort revenue) are always
//! exact integers, rationals, or decimals; float64 is rejected. Probability and
//! queueing models convert explicitly to binary64 and report approximate
//! results.

use std::collections::BTreeMap;

use num_rational::BigRational;

use bicmath_core::context::ExecContext;
use bicmath_core::contract::Args;
use bicmath_core::envelope::Exactness;
use bicmath_core::error::EngineError;
use bicmath_core::number::{Decimal, Number, NumericMode};
use bicmath_core::schema::{FieldSchema, NumberKind, ValueSchema};
use bicmath_core::value::Value;

pub(crate) fn all_modes() -> Vec<NumericMode> {
    vec![
        NumericMode::Exact,
        NumericMode::Auto,
        NumericMode::Scientific,
    ]
}

/// Exact numeric schema: integer, rational, or decimal, but never float64.
pub(crate) fn exact_schema() -> ValueSchema {
    ValueSchema::exact()
}

pub(crate) fn integer_schema() -> ValueSchema {
    ValueSchema::number(NumberKind::Integer)
}

pub(crate) fn decimal_schema() -> ValueSchema {
    ValueSchema::number(NumberKind::Decimal)
}

pub(crate) fn float_schema() -> ValueSchema {
    ValueSchema::float64()
}

pub(crate) fn text_schema() -> ValueSchema {
    ValueSchema::text()
}

pub(crate) fn field(name: &str, schema: ValueSchema) -> FieldSchema {
    FieldSchema::required(name, schema)
}

pub(crate) fn record_schema(fields: Vec<FieldSchema>) -> ValueSchema {
    ValueSchema::Record {
        fields,
        allow_extra: false,
    }
}

pub(crate) fn exactness(rounded: bool) -> Exactness {
    if rounded {
        Exactness::Rounded
    } else {
        Exactness::Exact
    }
}

/// Convert a required argument to an exact rational, rejecting float64.
pub(crate) fn exact_rational(args: &Args, name: &str) -> Result<BigRational, EngineError> {
    let number = args.number(name)?;
    number_to_rational(number, name)
}

/// Convert an optional argument to an exact rational, rejecting float64.
pub(crate) fn optional_exact_rational(
    args: &Args,
    name: &str,
) -> Result<Option<BigRational>, EngineError> {
    match args.optional_number(name)? {
        None => Ok(None),
        Some(number) => Ok(Some(number_to_rational(number, name)?)),
    }
}

pub(crate) fn number_to_rational(number: &Number, path: &str) -> Result<BigRational, EngineError> {
    match number {
        Number::Integer(value) => Ok(BigRational::from_integer(value.clone())),
        Number::Decimal(value) => Ok(value.to_rational()),
        Number::Rational(value) => Ok(value.clone()),
        Number::Float64(_) => Err(EngineError::malformed(format!(
            "{path} must be exact (integer, rational, or decimal); float64 is not accepted"
        ))
        .with_path(path.to_string())),
    }
}

/// Convert an exact rational to a decimal value, recording inexact rounding.
pub(crate) fn rational_value(
    value: &BigRational,
    ctx: &ExecContext,
    rounded: &mut bool,
) -> Result<Value, EngineError> {
    let (decimal, inexact) = Decimal::from_rational(value, &ctx.numeric, &ctx.limits)?;
    *rounded |= inexact;
    Ok(Value::decimal(decimal))
}

pub(crate) fn float_value(value: f64) -> Result<Value, EngineError> {
    Ok(Value::Number(Number::float(value)?))
}

pub(crate) fn example_args(pairs: &[(&str, serde_json::Value)]) -> BTreeMap<String, Value> {
    pairs
        .iter()
        .map(|(name, raw)| ((*name).to_string(), parse_value(raw.clone())))
        .collect()
}

pub(crate) fn parse_value(raw: serde_json::Value) -> Value {
    serde_json::from_value(raw).expect("example value must be valid")
}
