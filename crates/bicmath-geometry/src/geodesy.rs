//! Spherical geodesy: haversine distance, initial bearing, and destination
//! points on a sphere of caller-supplied radius (default mean Earth radius).
//!
//! Points are `[longitude, latitude]` in degrees. All results are approximate:
//! the spherical model ignores ellipsoid flattening and the trigonometric
//! functions are binary64.

use std::sync::Arc;

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, CostClass, Example, Function, FunctionDescriptor, Outcome, ParamDescriptor,
    SimpleFunction,
};
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::number::{Decimal, Number, NumericMode};
use bicmath_core::schema::ValueSchema;
use bicmath_core::value::Value;

use crate::common::{
    approximate_modes, descriptor, field, number_schema, record_schema, vector_schema,
};
use crate::f64math;
use crate::scalar::{Scalar, ensure_finite, parse_vector};

/// Mean Earth radius in kilometres (IUGG arithmetic mean).
const DEFAULT_RADIUS_KM: f64 = 6371.0088;

fn parse_lonlat(args: &Args, name: &str, ctx: &ExecContext) -> Result<(f64, f64), EngineError> {
    let point = parse_vector(args, name, 2, ctx)?;
    ensure_finite(&point[0], &format!("{name} longitude"))?;
    ensure_finite(&point[1], &format!("{name} latitude"))?;
    let longitude = point[0].to_f64();
    let latitude = point[1].to_f64();
    if !(-180.0..=180.0).contains(&longitude) {
        return Err(EngineError::domain(format!(
            "{name} longitude must lie in [-180, 180] degrees"
        )));
    }
    if !(-90.0..=90.0).contains(&latitude) {
        return Err(EngineError::domain(format!(
            "{name} latitude must lie in [-90, 90] degrees"
        )));
    }
    Ok((longitude, latitude))
}

fn parse_radius(args: &Args, ctx: &ExecContext) -> Result<(f64, Number), EngineError> {
    match args.optional_number("radius")? {
        Some(number) => {
            let scalar = Scalar::from_number(number, ctx)?;
            ensure_finite(&scalar, "radius")?;
            if scalar.is_negative() || scalar.is_zero() {
                return Err(EngineError::domain("radius must be positive"));
            }
            Ok((scalar.to_f64(), number.clone()))
        }
        None => Ok((
            DEFAULT_RADIUS_KM,
            Number::Decimal(
                Decimal::parse_default("6371.0088")
                    .map_err(|_| EngineError::internal("default radius is not representable"))?,
            ),
        )),
    }
}

fn require_approximate_mode(ctx: &ExecContext, function: &str) -> Result<(), EngineError> {
    if ctx.numeric.mode == NumericMode::Exact {
        return Err(EngineError::new(
            ErrorCode::UnsupportedNumericMode,
            format!(
                "{function} is defined through spherical trigonometry and is always \
                 approximate; use auto or scientific mode"
            ),
        ));
    }
    Ok(())
}

fn haversine_descriptor() -> FunctionDescriptor {
    descriptor(
        "geometry.haversine",
        "Haversine distance",
        "Great-circle distance between two points on a sphere.",
        "haversine",
    )
    .with_description(
        "Points are `[longitude, latitude]` in degrees. The haversine formula is evaluated \
         in binary64 on a sphere whose radius defaults to 6371.0088 km (mean Earth radius). \
         The result is labelled approximate: the spherical model ignores ellipsoid \
         flattening. Exact mode is rejected.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "point_a",
            "First point `[longitude, latitude]` in degrees.",
            vector_schema(2),
        ),
        ParamDescriptor::required(
            "point_b",
            "Second point `[longitude, latitude]` in degrees.",
            vector_schema(2),
        ),
        ParamDescriptor::optional("radius", "Sphere radius in kilometres.", number_schema()),
    ])
    .with_output(
        record_schema(vec![
            field("distance_km", number_schema()),
            field("method", ValueSchema::text()),
            field("approximate", ValueSchema::Bool),
            field("radius_km", number_schema()),
        ]),
        "Spherical distance in kilometres with method metadata.",
    )
    .with_modes(approximate_modes())
    .with_cost(CostClass::Constant)
    .with_examples(vec![
        Example::new(
            "London to Paris",
            crate::common::example_args(&[
                ("point_a", serde_json::json!([-0.1276, 51.5074])),
                ("point_b", serde_json::json!([2.3522, 48.8566])),
            ]),
        )
        .with_contains("haversine_sphere"),
    ])
}

fn initial_bearing_descriptor() -> FunctionDescriptor {
    descriptor(
        "geometry.initial_bearing",
        "Initial bearing",
        "Initial great-circle bearing from one point to another on a sphere.",
        "initial-bearing",
    )
    .with_description(
        "Points are `[longitude, latitude]` in degrees. The bearing is returned in degrees, \
         normalized to [0, 360). The result is approximate; exact mode is rejected.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "a",
            "Starting point `[longitude, latitude]` in degrees.",
            vector_schema(2),
        ),
        ParamDescriptor::required(
            "b",
            "Destination point `[longitude, latitude]` in degrees.",
            vector_schema(2),
        ),
    ])
    .with_output(
        record_schema(vec![
            field("bearing_degrees", number_schema()),
            field("method", ValueSchema::text()),
            field("approximate", ValueSchema::Bool),
        ]),
        "Initial bearing in degrees with method metadata.",
    )
    .with_modes(approximate_modes())
    .with_cost(CostClass::Constant)
    .with_examples(vec![
        Example::new(
            "London to Paris bearing",
            crate::common::example_args(&[
                ("a", serde_json::json!([-0.1276, 51.5074])),
                ("b", serde_json::json!([2.3522, 48.8566])),
            ]),
        )
        .with_contains("initial_bearing_sphere"),
    ])
}

fn destination_point_descriptor() -> FunctionDescriptor {
    descriptor(
        "geometry.destination_point",
        "Destination point",
        "Point reached by travelling a distance along a great-circle bearing.",
        "destination-point",
    )
    .with_description(
        "Starting from `origin` (`[longitude, latitude]` in degrees), travel \
         `distance_km` along the initial great-circle bearing `bearing_degrees`. The \
         result is a spherical approximation on the given radius (default 6371.0088 km); \
         exact mode is rejected. Longitudes are normalized to [-180, 180).",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "origin",
            "Origin point `[longitude, latitude]` in degrees.",
            vector_schema(2),
        ),
        ParamDescriptor::required(
            "bearing_degrees",
            "Initial bearing in degrees.",
            number_schema(),
        ),
        ParamDescriptor::required(
            "distance_km",
            "Distance to travel in kilometres.",
            number_schema(),
        ),
        ParamDescriptor::optional("radius", "Sphere radius in kilometres.", number_schema()),
    ])
    .with_output(
        record_schema(vec![
            field(
                "point",
                ValueSchema::Array {
                    items: Box::new(number_schema()),
                    min_len: Some(2),
                    max_len: Some(2),
                },
            ),
            field("method", ValueSchema::text()),
            field("approximate", ValueSchema::Bool),
        ]),
        "Destination `[longitude, latitude]` with method metadata.",
    )
    .with_modes(approximate_modes())
    .with_cost(CostClass::Constant)
    .with_examples(vec![
        Example::new(
            "travel north from London",
            crate::common::example_args(&[
                ("origin", serde_json::json!([-0.1276, 51.5074])),
                ("bearing_degrees", serde_json::json!(0)),
                ("distance_km", serde_json::json!(100)),
            ]),
        )
        .with_contains("destination_point_sphere"),
    ])
}

fn invoke_haversine(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    require_approximate_mode(ctx, "geometry.haversine")?;
    let (longitude_a, latitude_a) = parse_lonlat(args, "point_a", ctx)?;
    let (longitude_b, latitude_b) = parse_lonlat(args, "point_b", ctx)?;
    let (radius, radius_number) = parse_radius(args, ctx)?;
    let phi_a = latitude_a.to_radians();
    let phi_b = latitude_b.to_radians();
    let delta_phi = phi_b - phi_a;
    let delta_lambda = (longitude_b - longitude_a).to_radians();
    let sin_half_phi = f64math::sin(delta_phi / 2.0);
    let sin_half_lambda = f64math::sin(delta_lambda / 2.0);
    let haversine = sin_half_phi * sin_half_phi
        + f64math::cos(phi_a) * f64math::cos(phi_b) * sin_half_lambda * sin_half_lambda;
    let central_angle = 2.0 * f64math::asin(haversine.sqrt().clamp(0.0, 1.0));
    let distance = radius * central_angle;
    let value = Value::record([
        ("distance_km", Scalar::Float(distance).to_value(ctx)?),
        ("method", Value::text("haversine_sphere")),
        ("approximate", Value::Bool(true)),
        ("radius_km", Value::Number(radius_number)),
    ]);
    Ok(Outcome::approximate(value))
}

fn invoke_initial_bearing(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    require_approximate_mode(ctx, "geometry.initial_bearing")?;
    let (longitude_a, latitude_a) = parse_lonlat(args, "a", ctx)?;
    let (longitude_b, latitude_b) = parse_lonlat(args, "b", ctx)?;
    let phi_a = latitude_a.to_radians();
    let phi_b = latitude_b.to_radians();
    let delta_lambda = (longitude_b - longitude_a).to_radians();
    let y = f64math::sin(delta_lambda) * f64math::cos(phi_b);
    let x = f64math::cos(phi_a) * f64math::sin(phi_b)
        - f64math::sin(phi_a) * f64math::cos(phi_b) * f64math::cos(delta_lambda);
    let bearing = f64math::atan2(y, x).to_degrees().rem_euclid(360.0);
    let value = Value::record([
        ("bearing_degrees", Scalar::Float(bearing).to_value(ctx)?),
        ("method", Value::text("initial_bearing_sphere")),
        ("approximate", Value::Bool(true)),
    ]);
    Ok(Outcome::approximate(value))
}

fn invoke_destination_point(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    require_approximate_mode(ctx, "geometry.destination_point")?;
    let (longitude, latitude) = parse_lonlat(args, "origin", ctx)?;
    let bearing = Scalar::from_value(args.require("bearing_degrees")?, "bearing_degrees", ctx)?;
    ensure_finite(&bearing, "bearing_degrees")?;
    let distance = Scalar::from_value(args.require("distance_km")?, "distance_km", ctx)?;
    ensure_finite(&distance, "distance_km")?;
    if distance.is_negative() {
        return Err(EngineError::domain("distance_km must be non-negative"));
    }
    let (radius, _) = parse_radius(args, ctx)?;
    let phi = latitude.to_radians();
    let lambda = longitude.to_radians();
    let theta = bearing.to_f64().to_radians();
    let angular_distance = distance.to_f64() / radius;
    let sin_phi = f64math::sin(phi);
    let cos_phi = f64math::cos(phi);
    let sin_delta = f64math::sin(angular_distance);
    let cos_delta = f64math::cos(angular_distance);
    let destination_phi = f64math::asin(
        (sin_phi * cos_delta + cos_phi * sin_delta * f64math::cos(theta)).clamp(-1.0, 1.0),
    );
    let destination_lambda = lambda
        + f64math::atan2(
            f64math::sin(theta) * sin_delta * cos_phi,
            cos_delta - sin_phi * f64math::sin(destination_phi),
        );
    let longitude_out = ((destination_lambda.to_degrees() + 180.0).rem_euclid(360.0)) - 180.0;
    let latitude_out = destination_phi.to_degrees();
    let value = Value::record([
        (
            "point",
            Value::Array(vec![
                Scalar::Float(longitude_out).to_value(ctx)?,
                Scalar::Float(latitude_out).to_value(ctx)?,
            ]),
        ),
        ("method", Value::text("destination_point_sphere")),
        ("approximate", Value::Bool(true)),
    ]);
    Ok(Outcome::approximate(value))
}

pub(crate) fn register(functions: &mut Vec<Arc<dyn Function>>) {
    functions.push(SimpleFunction::arc(
        haversine_descriptor(),
        invoke_haversine,
    ));
    functions.push(SimpleFunction::arc(
        initial_bearing_descriptor(),
        invoke_initial_bearing,
    ));
    functions.push(SimpleFunction::arc(
        destination_point_descriptor(),
        invoke_destination_point,
    ));
}
