//! Real roots and angle unit conversions.

use std::sync::Arc;

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, CostClass, Example, Function, FunctionDescriptor, Outcome, ParamDescriptor,
    SimpleFunction,
};
use bicmath_core::error::{EngineError, ErrorCode};
use num_traits::ToPrimitive;

use crate::common::*;
use crate::f64math;

// ---------------------------------------------------------------------------
// Cube root
// ---------------------------------------------------------------------------

pub(crate) fn cbrt_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "scientific.cbrt",
        MODULE,
        VERSION,
        "Cube root",
        "Real cube root of a number.",
    )
    .with_description(
        "Defined for every finite real number, including negatives. The result is the \
         binary64 cube root approximation.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "x",
        "Real number.",
        any_number_schema(),
    )])
    .with_output(float64_schema(), "Real cube root of x.")
    .with_modes(scientific_modes())
    .with_cost(CostClass::Constant)
    .with_method_ref("docs/methods/scientific.md#cbrt")
    .with_examples(vec![
        Example::new(
            "cube root of a perfect cube",
            example_args(&[("x", serde_json::json!(27))]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "float64", "value": "3"}),
        )),
        Example::new(
            "cube root of a negative number",
            example_args(&[("x", serde_json::json!(-8))]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "float64", "value": "-2"}),
        )),
    ])
}

pub(crate) fn invoke_cbrt(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_scientific(ctx, "scientific.cbrt")?;
    let x = real_argument(args, "x")?;
    Ok(Outcome::approximate(float_value(f64math::cbrt(x))?))
}

// ---------------------------------------------------------------------------
// Integer n-th root
// ---------------------------------------------------------------------------

pub(crate) fn nth_root_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "scientific.nth_root",
        MODULE,
        VERSION,
        "Integer n-th root",
        "Real n-th root for a positive integer degree.",
    )
    .with_description(
        "Computes the real n-th root of x. The degree n must be a positive integer. Odd \
         degrees are defined for negative x; even degrees require x >= 0. For perfect \
         powers the result is refined with Newton iterations so that exact roots are \
         returned exactly.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("x", "Real number.", any_number_schema()),
        ParamDescriptor::required("n", "Positive integer degree.", integer_schema()),
    ])
    .with_output(float64_schema(), "Real n-th root of x.")
    .with_modes(scientific_modes())
    .with_cost(CostClass::Iterative)
    .with_method_ref("docs/methods/scientific.md#nth_root")
    .with_examples(vec![
        Example::new(
            "odd root of a negative number",
            example_args(&[("x", serde_json::json!(-27)), ("n", serde_json::json!(3))]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "float64", "value": "-3"}),
        )),
        Example::new(
            "even root of a negative number",
            example_args(&[("x", serde_json::json!(-4)), ("n", serde_json::json!(2))]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

pub(crate) fn invoke_nth_root(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_scientific(ctx, "scientific.nth_root")?;
    let x = real_argument(args, "x")?;
    let degree = args.integer("n")?;
    if degree.sign() == num_bigint::Sign::Minus || degree == num_bigint::BigInt::from(0) {
        return Err(
            EngineError::domain("nth_root requires a positive integer degree")
                .with_path("n".to_string()),
        );
    }
    let n = degree.to_u32().ok_or_else(|| {
        EngineError::new(
            ErrorCode::ResourceLimit,
            "nth_root degree exceeds the supported 32-bit range",
        )
        .with_path("n".to_string())
    })?;
    if n > ctx.limits.max_exponent {
        return Err(EngineError::new(
            ErrorCode::ResourceLimit,
            format!(
                "nth_root degree {n} exceeds the configured limit of {}",
                ctx.limits.max_exponent
            ),
        )
        .with_path("n".to_string()));
    }
    if x < 0.0 && n % 2 == 0 {
        return Err(
            EngineError::domain("nth_root with an even degree requires x >= 0")
                .with_path("x".to_string()),
        );
    }
    let magnitude = positive_nth_root(x.abs(), n);
    let root = if x < 0.0 { -magnitude } else { magnitude };
    Ok(Outcome::approximate(float_value(root)?))
}

/// Positive n-th root with Newton refinement so perfect powers are exact.
fn positive_nth_root(x: f64, n: u32) -> f64 {
    match n {
        1 => x,
        2 => f64math::sqrt(x),
        3 => f64math::cbrt(x),
        _ => {
            let degree = n as f64;
            let mut root = f64math::pow(x, 1.0 / degree);
            for _ in 0..2 {
                let denominator = f64math::pow(root, (n - 1) as f64);
                if denominator <= 0.0 || !denominator.is_finite() {
                    break;
                }
                root = ((n - 1) as f64 * root + x / denominator) / degree;
            }
            root
        }
    }
}

// ---------------------------------------------------------------------------
// Angle conversions
// ---------------------------------------------------------------------------

fn angle_conversion_descriptor(
    id: &str,
    title: &str,
    summary: &str,
    description: &str,
    example_input: serde_json::Value,
    example_output: &str,
) -> FunctionDescriptor {
    FunctionDescriptor::new(id, MODULE, VERSION, title, summary)
        .with_description(description)
        .with_parameters(vec![ParamDescriptor::required(
            "x",
            "Real number in the source unit.",
            any_number_schema(),
        )])
        .with_output(
            bicmath_core::schema::ValueSchema::Quantity {
                dimension: Some(bicmath_core::value::Dimension::ANGLE),
                allow_delta: false,
            },
            "Angle quantity with a float64 payload in the target unit.",
        )
        .with_modes(scientific_modes())
        .with_cost(CostClass::Constant)
        .with_method_ref(format!("docs/methods/scientific.md#{}", method_name(id)))
        .with_examples(vec![
            Example::new("zero", example_args(&[("x", serde_json::json!(0))])).with_value(
                parse_value(serde_json::json!({
                    "kind": "quantity",
                    "value": {"kind": "float64", "value": "0"},
                    "dimension": {"angle": 1}
                })),
            ),
            Example::new("straight angle", example_args(&[("x", example_input)])).with_value(
                parse_value(serde_json::json!({
                    "kind": "quantity",
                    "value": {"kind": "float64", "value": example_output},
                    "dimension": {"angle": 1}
                })),
            ),
        ])
}

pub(crate) fn degrees_to_radians_descriptor() -> FunctionDescriptor {
    angle_conversion_descriptor(
        "scientific.degrees_to_radians",
        "Degrees to radians",
        "Convert an angle in degrees to radians.",
        "Returns an angle quantity whose float64 payload is x * (pi / 180). The input is a \
         plain real number in degrees.",
        serde_json::json!(180),
        "3.141592653589793",
    )
}

pub(crate) fn invoke_degrees_to_radians(
    args: &Args,
    ctx: &ExecContext,
) -> Result<Outcome, EngineError> {
    require_scientific(ctx, "scientific.degrees_to_radians")?;
    let x = real_argument(args, "x")?;
    let radians = x * (std::f64::consts::PI / 180.0);
    Ok(Outcome::approximate(angle_value(radians)?))
}

pub(crate) fn radians_to_degrees_descriptor() -> FunctionDescriptor {
    angle_conversion_descriptor(
        "scientific.radians_to_degrees",
        "Radians to degrees",
        "Convert an angle in radians to degrees.",
        "Returns an angle quantity whose float64 payload is x * (180 / pi). The input is a \
         plain real number in radians.",
        serde_json::json!(std::f64::consts::PI),
        "180",
    )
}

pub(crate) fn invoke_radians_to_degrees(
    args: &Args,
    ctx: &ExecContext,
) -> Result<Outcome, EngineError> {
    require_scientific(ctx, "scientific.radians_to_degrees")?;
    let x = real_argument(args, "x")?;
    let degrees = x * (180.0 / std::f64::consts::PI);
    Ok(Outcome::approximate(angle_value(degrees)?))
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

pub(crate) fn functions() -> Vec<Arc<dyn Function>> {
    vec![
        SimpleFunction::arc(cbrt_descriptor(), invoke_cbrt),
        SimpleFunction::arc(nth_root_descriptor(), invoke_nth_root),
        SimpleFunction::arc(degrees_to_radians_descriptor(), invoke_degrees_to_radians),
        SimpleFunction::arc(radians_to_degrees_descriptor(), invoke_radians_to_degrees),
    ]
}
