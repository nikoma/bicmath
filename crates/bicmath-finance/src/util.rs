//! Shared helpers for the finance module.
//!
//! Money is always an exact decimal (or integer) amount plus a currency code.
//! Float64 amounts are rejected everywhere; rate-solving internals may use f64
//! but must convert back to a decimal explicitly and mark the result rounded.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use chrono::NaiveDate;
use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{Signed, ToPrimitive, Zero};

use bicmath_core::context::ExecContext;
use bicmath_core::envelope::Exactness;
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::number::{Decimal, Number, NumericMode, RoundingMode};
use bicmath_core::schema::{FieldSchema, NumberKind, ValueSchema};
use bicmath_core::value::{Value, validate_currency};

use bicmath_core::contract::Args;

/// Settlement scale used by every money operation that rounds to minor units.
pub(crate) const SETTLEMENT_SCALE: u32 = 2;
/// Largest scale accepted by `money_allocate`.
pub(crate) const MAX_ALLOCATION_SCALE: u32 = 18;

pub(crate) fn all_modes() -> Vec<NumericMode> {
    vec![
        NumericMode::Exact,
        NumericMode::Auto,
        NumericMode::Scientific,
    ]
}

pub(crate) fn exact_schema() -> ValueSchema {
    ValueSchema::exact()
}

pub(crate) fn float64_schema() -> ValueSchema {
    ValueSchema::number(NumberKind::Float64)
}

pub(crate) fn integer_schema() -> ValueSchema {
    ValueSchema::number(NumberKind::Integer)
}

pub(crate) fn exact_array_schema(min_len: usize) -> ValueSchema {
    ValueSchema::array_with_len(exact_schema(), min_len, None)
}

pub(crate) fn text_schema() -> ValueSchema {
    ValueSchema::text()
}

pub(crate) fn money_schema() -> ValueSchema {
    ValueSchema::Money
}

pub(crate) fn enum_schema(variants: &[&str]) -> ValueSchema {
    ValueSchema::Enum {
        variants: variants.iter().map(|v| (*v).to_string()).collect(),
    }
}

pub(crate) fn money_field(name: &str) -> FieldSchema {
    FieldSchema::required(name, ValueSchema::Money)
}

pub(crate) fn field(name: &str, schema: ValueSchema) -> FieldSchema {
    FieldSchema::required(name, schema)
}

pub(crate) fn optional_field(name: &str, schema: ValueSchema) -> FieldSchema {
    FieldSchema::optional(name, schema)
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

/// Build a money value from an exact decimal amount.
pub(crate) fn money_value(amount: Decimal, currency: &str) -> Value {
    Value::Money {
        amount: Box::new(Value::Number(Number::Decimal(amount))),
        currency: currency.to_string(),
    }
}

/// Convert an exact number to a decimal. Float64 and rational values are
/// rejected for money amounts and rates: money must be decimal or integer.
pub(crate) fn decimal_from_number(number: &Number, path: &str) -> Result<Decimal, EngineError> {
    match number {
        Number::Integer(value) => Ok(Decimal::from_bigint(value.clone())),
        Number::Decimal(value) => Ok(value.clone()),
        other => Err(EngineError::malformed(format!(
            "{path} must be an exact decimal or integer, found {}; \
             float64 and rational amounts are not accepted",
            other.kind_name()
        ))
        .with_path(path.to_string())),
    }
}

/// Convert an exact number (integer, rational, or decimal) to a rational.
/// Float64 is rejected.
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

/// Parse a money value, enforcing the decimal/integer amount rule.
pub(crate) fn money_from_value(
    value: &Value,
    path: &str,
) -> Result<(Decimal, String), EngineError> {
    match value {
        Value::Money { amount, currency } => {
            let number = amount
                .as_number()
                .map_err(|error| error.with_path(path.to_string()))?;
            let decimal = decimal_from_number(number, path)?;
            validate_currency(currency).map_err(|error| error.with_path(path.to_string()))?;
            Ok((decimal, currency.clone()))
        }
        other => Err(EngineError::malformed(format!(
            "expected money at {path}, found {}",
            other.kind_name()
        ))
        .with_path(path.to_string())),
    }
}

pub(crate) fn arg_money(args: &Args, name: &str) -> Result<(Decimal, String), EngineError> {
    let value = args.require(name)?;
    money_from_value(value, name)
}

pub(crate) fn optional_arg_money(
    args: &Args,
    name: &str,
) -> Result<Option<(Decimal, String)>, EngineError> {
    match args.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(_) => Ok(Some(arg_money(args, name)?)),
    }
}

pub(crate) fn require_currency(expected: &str, actual: &str) -> Result<(), EngineError> {
    if expected == actual {
        Ok(())
    } else {
        Err(EngineError::new(
            ErrorCode::CurrencyMismatch,
            format!("currency mismatch: expected {expected}, found {actual}"),
        )
        .with_details(serde_json::json!({
            "expected": expected,
            "found": actual,
        })))
    }
}

/// Quantize to the settlement scale with half-even rounding, reporting whether
/// the value changed.
pub(crate) fn quantize(amount: &Decimal, scale: u32) -> (Decimal, bool) {
    let rounded = amount.round_to(scale, RoundingMode::HalfEven);
    let changed = rounded.numeric_cmp(amount) != Ordering::Equal;
    (rounded, changed)
}

pub(crate) fn pow10(exponent: u32) -> BigInt {
    BigInt::from(10u32).pow(exponent)
}

pub(crate) fn rational_to_decimal(
    value: &BigRational,
    ctx: &ExecContext,
) -> Result<(Decimal, bool), EngineError> {
    Decimal::from_rational(value, &ctx.numeric, &ctx.limits)
}

pub(crate) fn round_rational_to_bigint(value: &BigRational, mode: RoundingMode) -> BigInt {
    bicmath_core::number::div_round(value.numer(), value.denom(), mode)
}

/// Convert an exact decimal amount to integer minor units at `scale`.
///
/// The amount must be exactly representable at that scale; otherwise the
/// caller should quantize it first.
pub(crate) fn decimal_to_minor(amount: &Decimal, scale: u32) -> Result<BigInt, EngineError> {
    let scaled = amount.to_rational() * BigRational::from_integer(pow10(scale));
    if !scaled.is_integer() {
        return Err(EngineError::domain(format!(
            "amount {amount} has more precision than the settlement scale of {scale}; \
             quantize the amount first or use a finer scale"
        )));
    }
    Ok(scaled.to_integer())
}

pub(crate) fn minor_to_decimal(minor: &BigInt, scale: u32) -> Decimal {
    Decimal::from_parts(minor.clone(), scale)
}

/// Build a float64 value, rejecting non-finite results.
pub(crate) fn float_value(value: f64) -> Result<Value, EngineError> {
    if !value.is_finite() {
        return Err(EngineError::domain(
            "calculation produced a non-finite float64 result",
        ));
    }
    Ok(Value::Number(Number::float(value)?))
}

/// Convert an exact number to float64 for an explicit approximate computation.
pub(crate) fn number_to_f64(number: &Number, path: &str) -> Result<f64, EngineError> {
    let value = number.to_f64().ok_or_else(|| {
        EngineError::domain(format!("{path} is not representable as float64"))
            .with_path(path.to_string())
    })?;
    if !value.is_finite() {
        return Err(
            EngineError::domain(format!("{path} overflows float64")).with_path(path.to_string())
        );
    }
    Ok(value)
}

/// Parse an array of exact numbers into rationals.
pub(crate) fn exact_array(args: &Args, name: &str) -> Result<Vec<BigRational>, EngineError> {
    let values = args.array(name)?;
    let mut out = Vec::with_capacity(values.len());
    for (index, value) in values.iter().enumerate() {
        let number = value
            .as_number()
            .map_err(|error| error.with_path(format!("{name}[{index}]")))?;
        out.push(number_to_rational(number, &format!("{name}[{index}]"))?);
    }
    Ok(out)
}

/// Square root of a non-negative rational as a decimal.
///
/// When the rational is an exact decimal with an exact decimal square root the
/// result is exact; otherwise the square root is computed in binary64 and
/// reported as rounded.
pub(crate) fn rational_sqrt(
    value: &BigRational,
    ctx: &ExecContext,
) -> Result<(Decimal, bool), EngineError> {
    if value.is_negative() {
        return Err(EngineError::domain(
            "cannot take the square root of a negative variance",
        ));
    }
    if value.is_zero() {
        return Ok((Decimal::zero(), false));
    }
    let (decimal, inexact) = Decimal::from_rational(value, &ctx.numeric, &ctx.limits)?;
    if !inexact && let Some(root) = decimal.sqrt_exact() {
        return Ok((root, false));
    }
    let as_f64 = value
        .to_f64()
        .ok_or_else(|| EngineError::domain("variance is not representable as float64"))?;
    let root = crate::mathfn::sqrt(as_f64);
    let decimal = Decimal::from_f64_display(root)
        .ok_or_else(|| EngineError::domain("square root is not representable as a decimal"))?;
    Ok((decimal, true))
}

/// Convert a rational number of minor units to a decimal major-unit amount.
pub(crate) fn rational_minor_to_decimal(
    value: &BigRational,
    scale: u32,
    ctx: &ExecContext,
) -> Result<(Decimal, bool), EngineError> {
    let major = value / BigRational::from_integer(pow10(scale));
    Decimal::from_rational(&major, &ctx.numeric, &ctx.limits)
}

pub(crate) fn parse_date(text: &str, path: &str) -> Result<NaiveDate, EngineError> {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").map_err(|_| {
        EngineError::malformed(format!(
            "invalid date {text:?} at {path}; expected an unambiguous YYYY-MM-DD date"
        ))
        .with_path(path.to_string())
    })
}

pub(crate) fn positive_periods(
    args: &Args,
    name: &str,
    ctx: &ExecContext,
) -> Result<u32, EngineError> {
    let value = args.integer(name)?;
    let periods = value
        .to_u32()
        .ok_or_else(|| EngineError::domain(format!("{name} must be a positive 32-bit integer")))?;
    if periods == 0 {
        return Err(EngineError::domain(format!("{name} must be at least 1")));
    }
    if periods as usize > ctx.limits.max_array_len {
        return Err(EngineError::new(
            ErrorCode::ResourceLimit,
            format!(
                "{name}={periods} exceeds the schedule limit of {}",
                ctx.limits.max_array_len
            ),
        ));
    }
    Ok(periods)
}

pub(crate) fn compounding_frequency(args: &Args, name: &str) -> Result<u32, EngineError> {
    match args.optional_integer(name)? {
        None => Ok(1),
        Some(value) => {
            let n = value
                .to_u32()
                .ok_or_else(|| EngineError::domain(format!("{name} must be a positive integer")))?;
            if n == 0 {
                return Err(EngineError::domain(format!("{name} must be at least 1")));
            }
            Ok(n)
        }
    }
}

pub(crate) fn example_args(pairs: &[(&str, serde_json::Value)]) -> BTreeMap<String, Value> {
    pairs
        .iter()
        .map(|(name, raw)| (name.to_string(), parse_value(raw.clone())))
        .collect()
}

pub(crate) fn parse_value(raw: serde_json::Value) -> Value {
    serde_json::from_value(raw).expect("example value must be valid")
}
