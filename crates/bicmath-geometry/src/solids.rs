//! Circle and sphere mensuration.
//!
//! Pi is irrational, so areas, circumferences, volumes, and surface areas are
//! always approximate. Auto mode returns decimal approximations to the context
//! precision and attaches an error estimate for the binary64 value of pi;
//! scientific mode returns float64; exact mode is rejected.

use std::sync::Arc;

use num_bigint::BigInt;

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, CostClass, ErrorEstimate, Example, Function, FunctionDescriptor, Outcome,
    ParamDescriptor, SimpleFunction,
};
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::number::{Decimal, NumericMode};
use bicmath_core::value::Value;

use crate::common::{descriptor, field, number_schema, record_schema};
use crate::scalar::{Scalar, ensure_finite};

/// Binary64 approximation of pi.
const PI: f64 = std::f64::consts::PI;

fn pi_error_estimate() -> ErrorEstimate {
    ErrorEstimate::new(
        "binary64_pi_relative",
        Value::decimal(Decimal::from_parts(BigInt::from(23u32), 17)),
        "pi is evaluated as the binary64 constant 3.141592653589793; the true value \
         differs by less than one ulp (relative error below 2.3e-16)",
    )
    .with_notes(
        "The result is approximate: pi is transcendental and cannot be represented exactly \
         as a decimal or rational.",
    )
}

fn require_approximate_mode(ctx: &ExecContext, function: &str) -> Result<(), EngineError> {
    if ctx.numeric.mode == NumericMode::Exact {
        return Err(EngineError::new(
            ErrorCode::UnsupportedNumericMode,
            format!(
                "{function} involves pi and is always approximate; use auto or scientific mode"
            ),
        ));
    }
    Ok(())
}

fn parse_radius(args: &Args, ctx: &ExecContext) -> Result<f64, EngineError> {
    let radius = Scalar::from_value(args.require("radius")?, "radius", ctx)?;
    ensure_finite(&radius, "radius")?;
    if radius.is_negative() {
        return Err(EngineError::domain("radius must be non-negative"));
    }
    Ok(radius.to_f64())
}

fn circle_descriptor() -> FunctionDescriptor {
    descriptor(
        "geometry.circle",
        "Circle",
        "Area and circumference of a circle.",
        "circle",
    )
    .with_description(
        "Returns `area = pi r^2` and `circumference = 2 pi r`. Because pi is irrational the \
         results are approximate: decimal approximations in auto mode and float64 in \
         scientific mode, with an error estimate for the binary64 value of pi. Exact mode \
         is rejected.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "radius",
        "Non-negative radius.",
        number_schema(),
    )])
    .with_output(
        record_schema(vec![
            field("area", number_schema()),
            field("circumference", number_schema()),
        ]),
        "Area and circumference.",
    )
    .with_modes(crate::common::approximate_modes())
    .with_cost(CostClass::Constant)
    .with_examples(vec![
        Example::new(
            "unit circle",
            crate::common::example_args(&[("radius", serde_json::json!(1))]),
        )
        .with_contains("circumference"),
    ])
}

fn sphere_descriptor() -> FunctionDescriptor {
    descriptor(
        "geometry.sphere",
        "Sphere",
        "Volume and surface area of a sphere.",
        "sphere",
    )
    .with_description(
        "Returns `volume = (4/3) pi r^3` and `surface_area = 4 pi r^2`. Because pi is \
         irrational the results are approximate: decimal approximations in auto mode and \
         float64 in scientific mode, with an error estimate for the binary64 value of pi. \
         Exact mode is rejected.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "radius",
        "Non-negative radius.",
        number_schema(),
    )])
    .with_output(
        record_schema(vec![
            field("volume", number_schema()),
            field("surface_area", number_schema()),
        ]),
        "Volume and surface area.",
    )
    .with_modes(crate::common::approximate_modes())
    .with_cost(CostClass::Constant)
    .with_examples(vec![
        Example::new(
            "unit sphere",
            crate::common::example_args(&[("radius", serde_json::json!(1))]),
        )
        .with_contains("surface_area"),
    ])
}

fn invoke_circle(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    require_approximate_mode(ctx, "geometry.circle")?;
    let radius = parse_radius(args, ctx)?;
    let area = PI * radius * radius;
    let circumference = 2.0 * PI * radius;
    let value = Value::record([
        ("area", Scalar::Float(area).to_value(ctx)?),
        ("circumference", Scalar::Float(circumference).to_value(ctx)?),
    ]);
    Ok(Outcome::approximate(value).with_error_estimate(pi_error_estimate()))
}

fn invoke_sphere(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    require_approximate_mode(ctx, "geometry.sphere")?;
    let radius = parse_radius(args, ctx)?;
    let volume = (4.0 / 3.0) * PI * radius * radius * radius;
    let surface_area = 4.0 * PI * radius * radius;
    let value = Value::record([
        ("volume", Scalar::Float(volume).to_value(ctx)?),
        ("surface_area", Scalar::Float(surface_area).to_value(ctx)?),
    ]);
    Ok(Outcome::approximate(value).with_error_estimate(pi_error_estimate()))
}

pub(crate) fn register(functions: &mut Vec<Arc<dyn Function>>) {
    functions.push(SimpleFunction::arc(circle_descriptor(), invoke_circle));
    functions.push(SimpleFunction::arc(sphere_descriptor(), invoke_sphere));
}
