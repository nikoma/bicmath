//! Mixed exact/float scalar used by the geometry algorithms.
//!
//! Geometry keeps exact rational arithmetic for as long as an operation is
//! algebraic (sums of squares, shoelace products, Hamilton products). When an
//! operation is transcendental — a square root that is not a perfect rational
//! square, or a trigonometric function — the value is promoted to binary64 and
//! the result is labelled approximate.
//!
//! Conversion back to the wire model follows the shared numeric contract:
//! exact values become integers or rationals, while binary64 values become
//! decimal approximations in `auto` mode, float64 in `scientific` mode, and are
//! rejected in `exact` mode.

use std::cmp::Ordering;

use num_bigint::BigInt;
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::{One, Signed, ToPrimitive, Zero};

use bicmath_core::context::ExecContext;
use bicmath_core::contract::Args;
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::number::{Decimal, Number, NumericMode};
use bicmath_core::value::Value;

use crate::f64math;

/// Parse a fixed-length vector of exact-or-float scalars from a wire array.
pub(crate) fn parse_vector(
    args: &Args,
    name: &str,
    dimension: usize,
    ctx: &ExecContext,
) -> Result<Vec<Scalar>, EngineError> {
    let items = args.array(name)?;
    if items.len() != dimension {
        return Err(EngineError::malformed(format!(
            "{name} must have exactly {dimension} coordinates, found {}",
            items.len()
        ))
        .with_path(name.to_string()));
    }
    let mut out = Vec::with_capacity(dimension);
    for (index, item) in items.iter().enumerate() {
        out.push(Scalar::from_value(item, &format!("{name}[{index}]"), ctx)?);
    }
    Ok(out)
}

/// Angle unit for trigonometric geometry inputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AngleUnit {
    Degrees,
    Radians,
}

impl AngleUnit {
    pub(crate) fn parse(text: &str) -> Result<AngleUnit, EngineError> {
        match text {
            "degrees" => Ok(AngleUnit::Degrees),
            "radians" => Ok(AngleUnit::Radians),
            other => Err(EngineError::domain(format!(
                "unknown angle unit {other:?}; expected \"degrees\" or \"radians\""
            ))),
        }
    }

    /// The measure of a straight angle in this unit.
    pub(crate) fn straight(self) -> f64 {
        match self {
            AngleUnit::Degrees => 180.0,
            AngleUnit::Radians => std::f64::consts::PI,
        }
    }

    fn to_radians(self, value: f64) -> f64 {
        match self {
            AngleUnit::Degrees => value.to_radians(),
            AngleUnit::Radians => value,
        }
    }

    fn angle_from_radians(self, value: f64) -> f64 {
        match self {
            AngleUnit::Degrees => value.to_degrees(),
            AngleUnit::Radians => value,
        }
    }
}

/// A scalar that is either an exact rational or a binary64 approximation.
#[derive(Clone, Debug)]
pub(crate) enum Scalar {
    Exact(BigRational),
    Float(f64),
}

impl Scalar {
    pub(crate) fn exact_i64(value: i64) -> Scalar {
        Scalar::Exact(BigRational::from_integer(BigInt::from(value)))
    }

    pub(crate) fn zero() -> Scalar {
        Scalar::exact_i64(0)
    }

    pub(crate) fn one() -> Scalar {
        Scalar::exact_i64(1)
    }

    pub(crate) fn half() -> Scalar {
        Scalar::Exact(BigRational::new(BigInt::one(), BigInt::from(2u32)))
    }

    /// Read a scalar from a wire number. Float64 inputs require scientific mode.
    pub(crate) fn from_number(number: &Number, ctx: &ExecContext) -> Result<Scalar, EngineError> {
        match number {
            Number::Float64(value) => {
                if ctx.numeric.mode != NumericMode::Scientific {
                    return Err(EngineError::new(
                        ErrorCode::UnsupportedNumericMode,
                        "float64 geometry inputs require scientific mode",
                    ));
                }
                Ok(Scalar::Float(value.get()))
            }
            other => {
                let rational = other
                    .to_exact_rational()
                    .ok_or_else(|| EngineError::internal("exact number is not representable"))?;
                Ok(Scalar::Exact(rational))
            }
        }
    }

    pub(crate) fn from_value(
        value: &Value,
        path: &str,
        ctx: &ExecContext,
    ) -> Result<Scalar, EngineError> {
        let number = value
            .as_number()
            .map_err(|error| error.with_path(path.to_string()))?;
        Scalar::from_number(number, ctx).map_err(|error| error.with_path(path.to_string()))
    }

    pub(crate) fn is_float(&self) -> bool {
        matches!(self, Scalar::Float(_))
    }

    pub(crate) fn is_zero(&self) -> bool {
        match self {
            Scalar::Exact(value) => value.is_zero(),
            Scalar::Float(value) => *value == 0.0,
        }
    }

    pub(crate) fn is_negative(&self) -> bool {
        match self {
            Scalar::Exact(value) => value.is_negative(),
            Scalar::Float(value) => *value < 0.0,
        }
    }

    pub(crate) fn to_f64(&self) -> f64 {
        match self {
            Scalar::Exact(value) => value.to_f64().unwrap_or(f64::NAN),
            Scalar::Float(value) => *value,
        }
    }

    pub(crate) fn add(&self, other: &Scalar) -> Scalar {
        match (self, other) {
            (Scalar::Exact(a), Scalar::Exact(b)) => Scalar::Exact(a + b),
            _ => Scalar::Float(self.to_f64() + other.to_f64()),
        }
    }

    pub(crate) fn sub(&self, other: &Scalar) -> Scalar {
        match (self, other) {
            (Scalar::Exact(a), Scalar::Exact(b)) => Scalar::Exact(a - b),
            _ => Scalar::Float(self.to_f64() - other.to_f64()),
        }
    }

    pub(crate) fn mul(&self, other: &Scalar) -> Scalar {
        match (self, other) {
            (Scalar::Exact(a), Scalar::Exact(b)) => Scalar::Exact(a * b),
            _ => Scalar::Float(self.to_f64() * other.to_f64()),
        }
    }

    pub(crate) fn div(&self, other: &Scalar) -> Result<Scalar, EngineError> {
        if other.is_zero() {
            return Err(EngineError::division_by_zero(
                "division by zero in a geometry computation",
            ));
        }
        match (self, other) {
            (Scalar::Exact(a), Scalar::Exact(b)) => Ok(Scalar::Exact(a / b)),
            _ => Ok(Scalar::Float(self.to_f64() / other.to_f64())),
        }
    }

    pub(crate) fn neg(&self) -> Scalar {
        match self {
            Scalar::Exact(value) => Scalar::Exact(-value),
            Scalar::Float(value) => Scalar::Float(-value),
        }
    }

    pub(crate) fn cmp(&self, other: &Scalar) -> Ordering {
        match (self, other) {
            (Scalar::Exact(a), Scalar::Exact(b)) => a.cmp(b),
            _ => self
                .to_f64()
                .partial_cmp(&other.to_f64())
                .unwrap_or(Ordering::Equal),
        }
    }

    pub(crate) fn equals(&self, other: &Scalar) -> bool {
        self.cmp(other) == Ordering::Equal
    }

    /// Square root: exact when the value is a perfect rational square, and a
    /// binary64 approximation otherwise. Negative inputs are a domain error.
    pub(crate) fn sqrt(&self) -> Result<Scalar, EngineError> {
        match self {
            Scalar::Exact(value) => {
                if value.is_negative() {
                    return Err(EngineError::domain("square root of a negative value"));
                }
                match rational_sqrt_exact(value) {
                    Some(root) => Ok(Scalar::Exact(root)),
                    None => Ok(Scalar::Float(f64math::sqrt(value.to_f64().unwrap_or(0.0)))),
                }
            }
            Scalar::Float(value) => {
                if *value < 0.0 {
                    return Err(EngineError::domain("square root of a negative value"));
                }
                Ok(Scalar::Float(f64math::sqrt(*value)))
            }
        }
    }

    pub(crate) fn sin(&self, unit: AngleUnit) -> Scalar {
        if let Scalar::Exact(value) = self
            && unit == AngleUnit::Degrees
            && let Some(exact) = exact_sin_degrees(value)
        {
            return Scalar::Exact(exact);
        }
        if let Scalar::Exact(value) = self
            && unit == AngleUnit::Radians
            && value.is_zero()
        {
            return Scalar::zero();
        }
        Scalar::Float(f64math::sin(unit.to_radians(self.to_f64())))
    }

    pub(crate) fn cos(&self, unit: AngleUnit) -> Scalar {
        if let Scalar::Exact(value) = self
            && unit == AngleUnit::Degrees
            && let Some(exact) = exact_cos_degrees(value)
        {
            return Scalar::Exact(exact);
        }
        if let Scalar::Exact(value) = self
            && unit == AngleUnit::Radians
            && value.is_zero()
        {
            return Scalar::one();
        }
        Scalar::Float(f64math::cos(unit.to_radians(self.to_f64())))
    }

    /// Arcsine in the requested unit, with exact special values in degrees.
    pub(crate) fn asin(&self, unit: AngleUnit) -> Scalar {
        if let Scalar::Exact(value) = self
            && unit == AngleUnit::Degrees
            && let Some(exact) = exact_asin_degrees(value)
        {
            return Scalar::Exact(exact);
        }
        let radians = f64math::asin(self.to_f64());
        Scalar::Float(unit.angle_from_radians(radians))
    }

    /// Arccosine in the requested unit, with exact special values in degrees.
    pub(crate) fn acos(&self, unit: AngleUnit) -> Scalar {
        if let Scalar::Exact(value) = self
            && unit == AngleUnit::Degrees
            && let Some(exact) = exact_acos_degrees(value)
        {
            return Scalar::Exact(exact);
        }
        let radians = f64math::acos(self.to_f64());
        Scalar::Float(unit.angle_from_radians(radians))
    }

    /// Convert to the wire model. Binary64 values become decimals in `auto`
    /// mode, float64 in `scientific` mode, and an error in `exact` mode.
    pub(crate) fn to_number(&self, ctx: &ExecContext) -> Result<Number, EngineError> {
        match self {
            Scalar::Exact(value) => Ok(rational_number(value.clone())),
            Scalar::Float(value) => match ctx.numeric.mode {
                NumericMode::Scientific => Number::float(*value),
                NumericMode::Auto => {
                    let canonical = if *value == 0.0 { 0.0 } else { *value };
                    Decimal::from_f64_display(canonical)
                        .map(Number::Decimal)
                        .ok_or_else(|| {
                            EngineError::internal(
                                "approximate value is not representable as decimal",
                            )
                        })
                }
                NumericMode::Exact => Err(EngineError::new(
                    ErrorCode::UnsupportedNumericMode,
                    "this geometry result is not exact; use auto or scientific mode",
                )),
            },
        }
    }

    pub(crate) fn to_value(&self, ctx: &ExecContext) -> Result<Value, EngineError> {
        Ok(Value::Number(self.to_number(ctx)?))
    }
}

/// Whether any scalar in a slice required a binary64 approximation.
pub(crate) fn any_float(values: &[Scalar]) -> bool {
    values.iter().any(Scalar::is_float)
}

/// Clamp a mathematically non-negative approximate value to `[0, inf)`.
///
/// Binary64 roundoff can make a value that is non-negative in exact arithmetic
/// slightly negative; clamping keeps subsequent square roots real. Exact values
/// are returned unchanged so that genuine domain errors still surface.
pub(crate) fn clamp_non_negative(value: Scalar) -> Scalar {
    match value {
        Scalar::Float(inner) => Scalar::Float(inner.max(0.0)),
        exact => exact,
    }
}

/// Convert an exact rational to the canonical exact wire number.
pub(crate) fn rational_number(value: BigRational) -> Number {
    if value.is_integer() {
        Number::Integer(value.to_integer())
    } else {
        Number::Rational(value)
    }
}

/// Exact square root of a non-negative rational, if one exists.
pub(crate) fn rational_sqrt_exact(value: &BigRational) -> Option<BigRational> {
    if value.is_negative() {
        return None;
    }
    let numer = value.numer().sqrt();
    let denom = value.denom().sqrt();
    if &numer * &numer == *value.numer() && &denom * &denom == *value.denom() {
        Some(BigRational::new(numer, denom))
    } else {
        None
    }
}

/// Square root of an exact squared value, preserving exactness when possible.
///
/// Returns `(value, approximate)`. In `auto` mode a non-perfect square is
/// evaluated as a decimal to the context precision; in `scientific` mode it is
/// evaluated as float64; in `exact` mode it is an error.
pub(crate) fn sqrt_to_value(
    squared: &Scalar,
    ctx: &ExecContext,
) -> Result<(Value, bool), EngineError> {
    match squared {
        Scalar::Exact(value) => {
            if value.is_negative() {
                return Err(EngineError::domain("square root of a negative value"));
            }
            if let Some(root) = rational_sqrt_exact(value) {
                return Ok((Value::Number(rational_number(root)), false));
            }
            match ctx.numeric.mode {
                NumericMode::Exact => Err(EngineError::new(
                    ErrorCode::UnsupportedNumericMode,
                    "the squared value is not a perfect rational square; \
                     use auto or scientific mode for an approximation",
                )),
                NumericMode::Auto => {
                    let (decimal, _) = Decimal::from_rational(value, &ctx.numeric, &ctx.limits)?;
                    let root = decimal_sqrt(&decimal, ctx)?;
                    Ok((Value::Number(Number::Decimal(root)), true))
                }
                NumericMode::Scientific => Ok((
                    Value::Number(Number::float(f64math::sqrt(
                        value.to_f64().unwrap_or(f64::NAN),
                    ))?),
                    true,
                )),
            }
        }
        Scalar::Float(value) => {
            if *value < 0.0 {
                return Err(EngineError::domain("square root of a negative value"));
            }
            Ok((Value::Number(Number::float(f64math::sqrt(*value))?), true))
        }
    }
}

/// Decimal square root rounded to the context precision.
pub(crate) fn decimal_sqrt(value: &Decimal, ctx: &ExecContext) -> Result<Decimal, EngineError> {
    let target_scale = ctx.numeric.precision.max(1) as i64;
    let scale = value.scale() as i64;
    let exponent = 2 * target_scale - scale;
    let mantissa = value.mantissa().clone();
    if exponent < 0 {
        let extra_scale = (scale + 1) / 2;
        let scaled = mantissa * pow10((2 * extra_scale - scale) as u32);
        let root = scaled.sqrt();
        return Ok(Decimal::from_parts(root, extra_scale as u32)
            .round_to_scale(target_scale, ctx.numeric.rounding));
    }
    let scaled = mantissa * pow10(exponent as u32);
    let root = scaled.sqrt();
    let raw = Decimal::from_parts(root, target_scale as u32);
    Ok(raw.round_to_scale(target_scale, ctx.numeric.rounding))
}

fn pow10(exponent: u32) -> BigInt {
    BigInt::from(10u32).pow(exponent)
}

/// Validate that a value is finite and report a domain error otherwise.
pub(crate) fn ensure_finite(value: &Scalar, what: &str) -> Result<(), EngineError> {
    let as_float = value.to_f64();
    if as_float.is_finite() {
        Ok(())
    } else {
        Err(EngineError::domain(format!("{what} must be finite")))
    }
}

fn exact_sin_degrees(angle: &BigRational) -> Option<BigRational> {
    let degrees = integral_degrees(angle)?;
    match degrees {
        0 | 180 => Some(rational(0, 1)),
        30 | 150 => Some(rational(1, 2)),
        90 => Some(rational(1, 1)),
        210 | 330 => Some(rational(-1, 2)),
        270 => Some(rational(-1, 1)),
        _ => None,
    }
}

fn exact_cos_degrees(angle: &BigRational) -> Option<BigRational> {
    let degrees = integral_degrees(angle)?;
    match degrees {
        0 => Some(rational(1, 1)),
        60 | 300 => Some(rational(1, 2)),
        90 | 270 => Some(rational(0, 1)),
        120 | 240 => Some(rational(-1, 2)),
        180 => Some(rational(-1, 1)),
        _ => None,
    }
}

fn exact_asin_degrees(value: &BigRational) -> Option<BigRational> {
    if *value == rational(-1, 1) {
        Some(rational(-90, 1))
    } else if *value == rational(-1, 2) {
        Some(rational(-30, 1))
    } else if value.is_zero() {
        Some(rational(0, 1))
    } else if *value == rational(1, 2) {
        Some(rational(30, 1))
    } else if *value == rational(1, 1) {
        Some(rational(90, 1))
    } else {
        None
    }
}

fn exact_acos_degrees(value: &BigRational) -> Option<BigRational> {
    if *value == rational(-1, 1) {
        Some(rational(180, 1))
    } else if *value == rational(-1, 2) {
        Some(rational(120, 1))
    } else if value.is_zero() {
        Some(rational(90, 1))
    } else if *value == rational(1, 2) {
        Some(rational(60, 1))
    } else if *value == rational(1, 1) {
        Some(rational(0, 1))
    } else {
        None
    }
}

fn integral_degrees(angle: &BigRational) -> Option<i64> {
    if !angle.is_integer() {
        return None;
    }
    let integer = angle.to_integer();
    let reduced = integer.mod_floor(&BigInt::from(360u32));
    reduced.to_i64()
}

fn rational(numer: i64, denom: i64) -> BigRational {
    BigRational::new(BigInt::from(numer), BigInt::from(denom))
}
