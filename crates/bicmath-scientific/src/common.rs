//! Shared helpers for the scientific module.
//!
//! Input extraction is explicit: plain numbers and dimensionless quantities are
//! accepted as real values, quantities with the angle dimension are accepted by
//! the trigonometric functions (the wire model carries no angle unit, so the
//! numeric payload is the SI coherent unit, the radian), and any other
//! dimension is rejected with [`ErrorCode::IncompatibleUnits`].

use std::collections::BTreeMap;

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{Args, require_mode};
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::number::{Number, NumericMode};
use bicmath_core::schema::{NumberKind, ValueSchema};
use bicmath_core::value::{Dimension, Value};

pub(crate) const MODULE: &str = "scientific";
pub(crate) const VERSION: &str = "1.0.0";

/// The scientific module only produces binary64 results, so every function
/// declares and requires [`NumericMode::Scientific`].
pub(crate) fn scientific_modes() -> Vec<NumericMode> {
    vec![NumericMode::Scientific]
}

pub(crate) fn require_scientific(ctx: &ExecContext, id: &str) -> Result<(), EngineError> {
    require_mode(ctx, &scientific_modes(), id)
}

/// Schema for scalar numeric parameters. `ValueSchema::Any` is used because the
/// scientific functions accept either a plain number or a quantity; the invoke
/// functions reject non-numeric and incompatible inputs with structured errors.
pub(crate) fn any_number_schema() -> ValueSchema {
    ValueSchema::Any
}

pub(crate) fn float64_schema() -> ValueSchema {
    ValueSchema::number(NumberKind::Float64)
}

pub(crate) fn integer_schema() -> ValueSchema {
    ValueSchema::number(NumberKind::Integer)
}

/// Schema for the restricted expression parameter. The actual variable name is
/// supplied by the caller in the `variable` parameter; the placeholder listed
/// here is documentation only and the schema itself only requires text.
pub(crate) fn expression_schema() -> ValueSchema {
    ValueSchema::Expression {
        variables: vec!["x".to_string()],
    }
}

pub(crate) fn method_name(id: &str) -> &str {
    id.strip_prefix("scientific.").unwrap_or(id)
}

pub(crate) fn parse_value(raw: serde_json::Value) -> Value {
    serde_json::from_value(raw).expect("example value must be valid")
}

pub(crate) fn example_args(pairs: &[(&str, serde_json::Value)]) -> BTreeMap<String, Value> {
    pairs
        .iter()
        .map(|(name, raw)| (name.to_string(), parse_value(raw.clone())))
        .collect()
}

/// Build a float64 wire value, rejecting non-finite results.
pub(crate) fn float_value(value: f64) -> Result<Value, EngineError> {
    Ok(Value::Number(Number::float(value)?))
}

/// Build an angle quantity carrying a float64 radian value.
pub(crate) fn angle_value(radians: f64) -> Result<Value, EngineError> {
    Ok(Value::Quantity {
        value: Box::new(Value::Number(Number::float(radians)?)),
        dimension: Dimension::ANGLE,
    })
}

/// Convert a numeric payload to a finite f64.
pub(crate) fn finite_f64(number: &Number, path: &str) -> Result<f64, EngineError> {
    let value = number.to_f64().ok_or_else(|| {
        EngineError::domain(format!(
            "{} value is not representable as float64",
            number.kind_name()
        ))
        .with_path(path.to_string())
    })?;
    if !value.is_finite() {
        return Err(EngineError::domain("value overflows float64").with_path(path.to_string()));
    }
    Ok(value)
}

fn quantity_number(value: &Value, path: &str) -> Result<f64, EngineError> {
    let number = value
        .as_number()
        .map_err(|error| error.with_path(path.to_string()))?;
    finite_f64(number, path)
}

/// A real scalar: a plain number or a dimensionless quantity.
pub(crate) fn real_f64(value: &Value, path: &str) -> Result<f64, EngineError> {
    match value {
        Value::Number(number) => finite_f64(number, path),
        Value::Quantity { value, dimension } if dimension.is_dimensionless() => {
            quantity_number(value, path)
        }
        Value::Quantity { dimension, .. } => Err(EngineError::new(
            ErrorCode::IncompatibleUnits,
            format!("expected a dimensionless number, found dimension {dimension}"),
        )
        .with_path(path.to_string())),
        other => Err(EngineError::malformed(format!(
            "expected a number, found {}",
            other.kind_name()
        ))
        .with_path(path.to_string())),
    }
}

/// An angle scalar: a plain number (interpreted as radians), a dimensionless
/// quantity, or a quantity whose dimension is exactly the angle dimension.
pub(crate) fn angle_f64(value: &Value, path: &str) -> Result<f64, EngineError> {
    match value {
        Value::Number(number) => finite_f64(number, path),
        Value::Quantity { value, dimension }
            if dimension.is_angle_only() || dimension.is_dimensionless() =>
        {
            quantity_number(value, path)
        }
        Value::Quantity { dimension, .. } => Err(EngineError::new(
            ErrorCode::IncompatibleUnits,
            format!(
                "expected a dimensionless number or an angle quantity, found dimension {dimension}"
            ),
        )
        .with_path(path.to_string())),
        other => Err(EngineError::malformed(format!(
            "expected a number or angle quantity, found {}",
            other.kind_name()
        ))
        .with_path(path.to_string())),
    }
}

/// Extract a real scalar together with its declared dimension, used by
/// two-argument functions that require matching dimensions.
pub(crate) fn extract_real(
    value: &Value,
    path: &str,
) -> Result<(f64, Option<Dimension>), EngineError> {
    match value {
        Value::Number(number) => Ok((finite_f64(number, path)?, None)),
        Value::Quantity { value, dimension } => {
            Ok((quantity_number(value, path)?, Some(*dimension)))
        }
        other => Err(EngineError::malformed(format!(
            "expected a number or quantity, found {}",
            other.kind_name()
        ))
        .with_path(path.to_string())),
    }
}

pub(crate) fn dimension_label(dimension: Option<Dimension>) -> String {
    match dimension {
        Some(dimension) => dimension.to_string(),
        None => "1".to_string(),
    }
}

/// A required argument parsed as an angle.
pub(crate) fn angle_argument(args: &Args, name: &str) -> Result<f64, EngineError> {
    angle_f64(args.require(name)?, name)
}

/// A required argument parsed as a real scalar.
pub(crate) fn real_argument(args: &Args, name: &str) -> Result<f64, EngineError> {
    real_f64(args.require(name)?, name)
}
