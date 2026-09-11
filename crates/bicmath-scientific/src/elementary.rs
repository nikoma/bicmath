//! Elementary transcendental functions: trigonometric, hyperbolic, exponential,
//! and logarithmic.
//!
//! Angles are radians. Plain numbers are interpreted as radians and quantities
//! with the angle dimension are accepted after unit normalization; other
//! dimensions are rejected. Every result is a binary64 approximation, so all
//! functions require [`bicmath_core::number::NumericMode::Scientific`].

use std::sync::Arc;

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, CostClass, Example, Function, FunctionDescriptor, Outcome, ParamDescriptor,
    SimpleFunction,
};
use bicmath_core::error::{EngineError, ErrorCode};

use crate::common::*;
use crate::f64math;

fn value_example(title: &str, input: serde_json::Value, expected: &str) -> Example {
    Example::new(title, example_args(&[("x", input)])).with_value(parse_value(
        serde_json::json!({"kind": "float64", "value": expected}),
    ))
}

fn error_example(title: &str, input: serde_json::Value, code: ErrorCode) -> Example {
    Example::new(title, example_args(&[("x", input)])).with_error(code)
}

fn unary_descriptor(
    id: &str,
    title: &str,
    summary: &str,
    description: &str,
    input_description: &str,
    examples: Vec<Example>,
) -> FunctionDescriptor {
    FunctionDescriptor::new(id, MODULE, VERSION, title, summary)
        .with_description(description)
        .with_parameters(vec![ParamDescriptor::required(
            "x",
            input_description,
            any_number_schema(),
        )])
        .with_output(float64_schema(), "Function value as float64.")
        .with_modes(scientific_modes())
        .with_cost(CostClass::Constant)
        .with_method_ref(format!("docs/methods/scientific.md#{}", method_name(id)))
        .with_examples(examples)
}

// ---------------------------------------------------------------------------
// Trigonometric functions
// ---------------------------------------------------------------------------

pub(crate) fn sin_descriptor() -> FunctionDescriptor {
    unary_descriptor(
        "scientific.sin",
        "Sine",
        "Sine of an angle in radians.",
        "Accepts a plain number interpreted as radians or a quantity with the angle \
         dimension. The result is the correctly rounded binary64 sine approximation.",
        "Angle in radians, or an angle quantity.",
        vec![
            value_example("sine of zero", serde_json::json!(0), "0"),
            error_example(
                "length is not an angle",
                serde_json::json!({"kind": "quantity", "value": 1, "dimension": {"length": 1}}),
                ErrorCode::IncompatibleUnits,
            ),
        ],
    )
}

pub(crate) fn invoke_sin(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_scientific(ctx, "scientific.sin")?;
    let x = angle_argument(args, "x")?;
    Ok(Outcome::approximate(float_value(f64math::sin(x))?))
}

pub(crate) fn cos_descriptor() -> FunctionDescriptor {
    unary_descriptor(
        "scientific.cos",
        "Cosine",
        "Cosine of an angle in radians.",
        "Accepts a plain number interpreted as radians or a quantity with the angle \
         dimension. The result is the correctly rounded binary64 cosine approximation.",
        "Angle in radians, or an angle quantity.",
        vec![value_example("cosine of zero", serde_json::json!(0), "1")],
    )
}

pub(crate) fn invoke_cos(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_scientific(ctx, "scientific.cos")?;
    let x = angle_argument(args, "x")?;
    Ok(Outcome::approximate(float_value(f64math::cos(x))?))
}

pub(crate) fn tan_descriptor() -> FunctionDescriptor {
    unary_descriptor(
        "scientific.tan",
        "Tangent",
        "Tangent of an angle in radians.",
        "Accepts a plain number interpreted as radians or a quantity with the angle \
         dimension. Binary64 cannot represent pi/2 exactly, so the tangent is finite but \
         very large near odd multiples of pi/2.",
        "Angle in radians, or an angle quantity.",
        vec![value_example("tangent of zero", serde_json::json!(0), "0")],
    )
}

pub(crate) fn invoke_tan(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_scientific(ctx, "scientific.tan")?;
    let x = angle_argument(args, "x")?;
    Ok(Outcome::approximate(float_value(f64math::tan(x))?))
}

pub(crate) fn asin_descriptor() -> FunctionDescriptor {
    unary_descriptor(
        "scientific.asin",
        "Arcsine",
        "Inverse sine in radians.",
        "Defined for |x| <= 1; the result lies in [-pi/2, pi/2]. Values outside the \
         domain are rejected with a domain violation.",
        "Value in [-1, 1].",
        vec![
            value_example("arcsine of one", serde_json::json!(1), "1.5707963267948966"),
            error_example(
                "outside the domain",
                serde_json::json!(2),
                ErrorCode::DomainViolation,
            ),
        ],
    )
}

pub(crate) fn invoke_asin(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_scientific(ctx, "scientific.asin")?;
    let x = real_argument(args, "x")?;
    if x.abs() > 1.0 {
        return Err(EngineError::domain("asin requires |x| <= 1").with_path("x".to_string()));
    }
    Ok(Outcome::approximate(float_value(f64math::asin(x))?))
}

pub(crate) fn acos_descriptor() -> FunctionDescriptor {
    unary_descriptor(
        "scientific.acos",
        "Arccosine",
        "Inverse cosine in radians.",
        "Defined for |x| <= 1; the result lies in [0, pi]. Values outside the domain are \
         rejected with a domain violation.",
        "Value in [-1, 1].",
        vec![
            value_example("arccosine of one", serde_json::json!(1), "0"),
            error_example(
                "outside the domain",
                serde_json::json!(2),
                ErrorCode::DomainViolation,
            ),
        ],
    )
}

pub(crate) fn invoke_acos(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_scientific(ctx, "scientific.acos")?;
    let x = real_argument(args, "x")?;
    if x.abs() > 1.0 {
        return Err(EngineError::domain("acos requires |x| <= 1").with_path("x".to_string()));
    }
    Ok(Outcome::approximate(float_value(f64math::acos(x))?))
}

pub(crate) fn atan_descriptor() -> FunctionDescriptor {
    unary_descriptor(
        "scientific.atan",
        "Arctangent",
        "Inverse tangent in radians.",
        "Defined for every finite real number; the result lies in (-pi/2, pi/2).",
        "Real number.",
        vec![value_example(
            "arctangent of one",
            serde_json::json!(1),
            "0.7853981633974483",
        )],
    )
}

pub(crate) fn invoke_atan(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_scientific(ctx, "scientific.atan")?;
    let x = real_argument(args, "x")?;
    Ok(Outcome::approximate(float_value(f64math::atan(x))?))
}

pub(crate) fn atan2_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "scientific.atan2",
        MODULE,
        VERSION,
        "Two-argument arctangent",
        "Angle of the point (x, y) in radians.",
    )
    .with_description(
        "Returns the angle in radians between the positive x-axis and the point (x, y), in \
         (-pi, pi]. Both arguments must be plain numbers or quantities with the same \
         dimension; the ratio is then dimensionless. Mixed dimensions are rejected.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("y", "Ordinate.", any_number_schema()),
        ParamDescriptor::required("x", "Abscissa.", any_number_schema()),
    ])
    .with_output(float64_schema(), "Angle in radians.")
    .with_modes(scientific_modes())
    .with_cost(CostClass::Constant)
    .with_method_ref("docs/methods/scientific.md#atan2")
    .with_examples(vec![
        Example::new(
            "first quadrant diagonal",
            example_args(&[("y", serde_json::json!(1)), ("x", serde_json::json!(1))]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "float64", "value": "0.7853981633974483"}),
        )),
        Example::new(
            "mismatched dimensions",
            example_args(&[
                (
                    "y",
                    serde_json::json!({"kind": "quantity", "value": 1, "dimension": {"length": 1}}),
                ),
                ("x", serde_json::json!(1)),
            ]),
        )
        .with_error(ErrorCode::IncompatibleUnits),
    ])
}

pub(crate) fn invoke_atan2(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_scientific(ctx, "scientific.atan2")?;
    let (y, y_dimension) = extract_real(args.require("y")?, "y")?;
    let (x, x_dimension) = extract_real(args.require("x")?, "x")?;
    if y_dimension != x_dimension {
        return Err(EngineError::new(
            ErrorCode::IncompatibleUnits,
            format!(
                "atan2 requires arguments of the same dimension, found {} and {}",
                dimension_label(y_dimension),
                dimension_label(x_dimension)
            ),
        )
        .with_path("y".to_string()));
    }
    Ok(Outcome::approximate(float_value(f64math::atan2(y, x))?))
}

// ---------------------------------------------------------------------------
// Hyperbolic functions
// ---------------------------------------------------------------------------

pub(crate) fn sinh_descriptor() -> FunctionDescriptor {
    unary_descriptor(
        "scientific.sinh",
        "Hyperbolic sine",
        "Hyperbolic sine of a real number.",
        "Defined for every finite real number; overflow to a non-finite value is reported \
         as a domain violation.",
        "Real number.",
        vec![value_example(
            "hyperbolic sine of zero",
            serde_json::json!(0),
            "0",
        )],
    )
}

pub(crate) fn invoke_sinh(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_scientific(ctx, "scientific.sinh")?;
    let x = real_argument(args, "x")?;
    Ok(Outcome::approximate(float_value(f64math::sinh(x))?))
}

pub(crate) fn cosh_descriptor() -> FunctionDescriptor {
    unary_descriptor(
        "scientific.cosh",
        "Hyperbolic cosine",
        "Hyperbolic cosine of a real number.",
        "Defined for every finite real number; the result is always at least one.",
        "Real number.",
        vec![value_example(
            "hyperbolic cosine of zero",
            serde_json::json!(0),
            "1",
        )],
    )
}

pub(crate) fn invoke_cosh(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_scientific(ctx, "scientific.cosh")?;
    let x = real_argument(args, "x")?;
    Ok(Outcome::approximate(float_value(f64math::cosh(x))?))
}

pub(crate) fn tanh_descriptor() -> FunctionDescriptor {
    unary_descriptor(
        "scientific.tanh",
        "Hyperbolic tangent",
        "Hyperbolic tangent of a real number.",
        "Defined for every finite real number; the result lies in (-1, 1).",
        "Real number.",
        vec![value_example(
            "hyperbolic tangent of zero",
            serde_json::json!(0),
            "0",
        )],
    )
}

pub(crate) fn invoke_tanh(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_scientific(ctx, "scientific.tanh")?;
    let x = real_argument(args, "x")?;
    Ok(Outcome::approximate(float_value(f64math::tanh(x))?))
}

pub(crate) fn asinh_descriptor() -> FunctionDescriptor {
    unary_descriptor(
        "scientific.asinh",
        "Inverse hyperbolic sine",
        "Inverse hyperbolic sine of a real number.",
        "Defined for every finite real number.",
        "Real number.",
        vec![value_example(
            "inverse hyperbolic sine of zero",
            serde_json::json!(0),
            "0",
        )],
    )
}

pub(crate) fn invoke_asinh(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_scientific(ctx, "scientific.asinh")?;
    let x = real_argument(args, "x")?;
    Ok(Outcome::approximate(float_value(f64math::asinh(x))?))
}

pub(crate) fn acosh_descriptor() -> FunctionDescriptor {
    unary_descriptor(
        "scientific.acosh",
        "Inverse hyperbolic cosine",
        "Inverse hyperbolic cosine of a real number.",
        "Defined for x >= 1; values below one are rejected with a domain violation.",
        "Value greater than or equal to one.",
        vec![
            value_example(
                "inverse hyperbolic cosine of one",
                serde_json::json!(1),
                "0",
            ),
            error_example(
                "below the domain",
                serde_json::json!(0.5),
                ErrorCode::DomainViolation,
            ),
        ],
    )
}

pub(crate) fn invoke_acosh(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_scientific(ctx, "scientific.acosh")?;
    let x = real_argument(args, "x")?;
    if x < 1.0 {
        return Err(EngineError::domain("acosh requires x >= 1").with_path("x".to_string()));
    }
    Ok(Outcome::approximate(float_value(f64math::acosh(x))?))
}

pub(crate) fn atanh_descriptor() -> FunctionDescriptor {
    unary_descriptor(
        "scientific.atanh",
        "Inverse hyperbolic tangent",
        "Inverse hyperbolic tangent of a real number.",
        "Defined for |x| < 1; the endpoints are rejected with a domain violation.",
        "Value strictly between -1 and 1.",
        vec![
            value_example(
                "inverse hyperbolic tangent of zero",
                serde_json::json!(0),
                "0",
            ),
            error_example(
                "at the pole",
                serde_json::json!(1),
                ErrorCode::DomainViolation,
            ),
        ],
    )
}

pub(crate) fn invoke_atanh(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_scientific(ctx, "scientific.atanh")?;
    let x = real_argument(args, "x")?;
    if x.abs() >= 1.0 {
        return Err(EngineError::domain("atanh requires |x| < 1").with_path("x".to_string()));
    }
    Ok(Outcome::approximate(float_value(f64math::atanh(x))?))
}

// ---------------------------------------------------------------------------
// Exponential and logarithmic functions
// ---------------------------------------------------------------------------

pub(crate) fn exp_descriptor() -> FunctionDescriptor {
    unary_descriptor(
        "scientific.exp",
        "Exponential",
        "Natural exponential of a real number.",
        "Defined for every finite real number; overflow to a non-finite value is reported \
         as a domain violation.",
        "Real number.",
        vec![value_example(
            "exponential of zero",
            serde_json::json!(0),
            "1",
        )],
    )
}

pub(crate) fn invoke_exp(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_scientific(ctx, "scientific.exp")?;
    let x = real_argument(args, "x")?;
    Ok(Outcome::approximate(float_value(f64math::exp(x))?))
}

pub(crate) fn ln_descriptor() -> FunctionDescriptor {
    unary_descriptor(
        "scientific.ln",
        "Natural logarithm",
        "Natural logarithm of a positive real number.",
        "Defined for x > 0; zero and negative values are rejected with a domain violation.",
        "Positive real number.",
        vec![
            value_example("logarithm of one", serde_json::json!(1), "0"),
            error_example(
                "negative input",
                serde_json::json!(-1),
                ErrorCode::DomainViolation,
            ),
        ],
    )
}

pub(crate) fn invoke_ln(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_scientific(ctx, "scientific.ln")?;
    let x = real_argument(args, "x")?;
    if x <= 0.0 {
        return Err(EngineError::domain("ln requires x > 0").with_path("x".to_string()));
    }
    Ok(Outcome::approximate(float_value(f64math::ln(x))?))
}

pub(crate) fn log10_descriptor() -> FunctionDescriptor {
    unary_descriptor(
        "scientific.log10",
        "Common logarithm",
        "Base-10 logarithm of a positive real number.",
        "Defined for x > 0; zero and negative values are rejected with a domain violation.",
        "Positive real number.",
        vec![
            value_example("logarithm of one", serde_json::json!(1), "0"),
            error_example(
                "zero input",
                serde_json::json!(0),
                ErrorCode::DomainViolation,
            ),
        ],
    )
}

pub(crate) fn invoke_log10(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_scientific(ctx, "scientific.log10")?;
    let x = real_argument(args, "x")?;
    if x <= 0.0 {
        return Err(EngineError::domain("log10 requires x > 0").with_path("x".to_string()));
    }
    Ok(Outcome::approximate(float_value(f64math::log10(x))?))
}

pub(crate) fn log2_descriptor() -> FunctionDescriptor {
    unary_descriptor(
        "scientific.log2",
        "Binary logarithm",
        "Base-2 logarithm of a positive real number.",
        "Defined for x > 0; zero and negative values are rejected with a domain violation.",
        "Positive real number.",
        vec![
            value_example("logarithm of one", serde_json::json!(1), "0"),
            error_example(
                "negative input",
                serde_json::json!(-1),
                ErrorCode::DomainViolation,
            ),
        ],
    )
}

pub(crate) fn invoke_log2(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_scientific(ctx, "scientific.log2")?;
    let x = real_argument(args, "x")?;
    if x <= 0.0 {
        return Err(EngineError::domain("log2 requires x > 0").with_path("x".to_string()));
    }
    Ok(Outcome::approximate(float_value(f64math::log2(x))?))
}

pub(crate) fn log_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "scientific.log",
        MODULE,
        VERSION,
        "Logarithm with arbitrary base",
        "Logarithm of x to a positive base other than one.",
    )
    .with_description(
        "Returns log_base(x) = ln(x) / ln(base), defined for x > 0, base > 0, and base != 1.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("x", "Positive real number.", any_number_schema()),
        ParamDescriptor::required(
            "base",
            "Positive real number different from one.",
            any_number_schema(),
        ),
    ])
    .with_output(float64_schema(), "Logarithm of x in the given base.")
    .with_modes(scientific_modes())
    .with_cost(CostClass::Constant)
    .with_method_ref("docs/methods/scientific.md#log")
    .with_examples(vec![
        Example::new(
            "base two logarithm",
            example_args(&[("x", serde_json::json!(8)), ("base", serde_json::json!(2))]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "float64", "value": "3"}),
        )),
        Example::new(
            "base one is undefined",
            example_args(&[("x", serde_json::json!(8)), ("base", serde_json::json!(1))]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

pub(crate) fn invoke_log(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_scientific(ctx, "scientific.log")?;
    let x = real_argument(args, "x")?;
    let base = real_argument(args, "base")?;
    if x <= 0.0 {
        return Err(EngineError::domain("log requires x > 0").with_path("x".to_string()));
    }
    if base <= 0.0 || base == 1.0 {
        return Err(EngineError::domain("log requires base > 0 and base != 1")
            .with_path("base".to_string()));
    }
    Ok(Outcome::approximate(float_value(
        f64math::ln(x) / f64math::ln(base),
    )?))
}

pub(crate) fn log1p_descriptor() -> FunctionDescriptor {
    unary_descriptor(
        "scientific.log1p",
        "Logarithm of one plus x",
        "Accurate ln(1 + x) for small x.",
        "Computes ln(1 + x) without the cancellation of computing 1 + x first. Defined for \
         x > -1; other values are rejected with a domain violation.",
        "Real number greater than -1.",
        vec![
            value_example("log1p of zero", serde_json::json!(0), "0"),
            error_example(
                "below the domain",
                serde_json::json!(-2),
                ErrorCode::DomainViolation,
            ),
        ],
    )
}

pub(crate) fn invoke_log1p(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_scientific(ctx, "scientific.log1p")?;
    let x = real_argument(args, "x")?;
    if x <= -1.0 {
        return Err(EngineError::domain("log1p requires x > -1").with_path("x".to_string()));
    }
    Ok(Outcome::approximate(float_value(f64math::log1p(x))?))
}

pub(crate) fn expm1_descriptor() -> FunctionDescriptor {
    unary_descriptor(
        "scientific.expm1",
        "Exponential minus one",
        "Accurate exp(x) - 1 for small x.",
        "Computes exp(x) - 1 without the cancellation of computing exp(x) first. Defined \
         for every finite real number.",
        "Real number.",
        vec![value_example("expm1 of zero", serde_json::json!(0), "0")],
    )
}

pub(crate) fn invoke_expm1(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_scientific(ctx, "scientific.expm1")?;
    let x = real_argument(args, "x")?;
    Ok(Outcome::approximate(float_value(f64math::expm1(x))?))
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

pub(crate) fn functions() -> Vec<Arc<dyn Function>> {
    vec![
        SimpleFunction::arc(sin_descriptor(), invoke_sin),
        SimpleFunction::arc(cos_descriptor(), invoke_cos),
        SimpleFunction::arc(tan_descriptor(), invoke_tan),
        SimpleFunction::arc(asin_descriptor(), invoke_asin),
        SimpleFunction::arc(acos_descriptor(), invoke_acos),
        SimpleFunction::arc(atan_descriptor(), invoke_atan),
        SimpleFunction::arc(atan2_descriptor(), invoke_atan2),
        SimpleFunction::arc(sinh_descriptor(), invoke_sinh),
        SimpleFunction::arc(cosh_descriptor(), invoke_cosh),
        SimpleFunction::arc(tanh_descriptor(), invoke_tanh),
        SimpleFunction::arc(asinh_descriptor(), invoke_asinh),
        SimpleFunction::arc(acosh_descriptor(), invoke_acosh),
        SimpleFunction::arc(atanh_descriptor(), invoke_atanh),
        SimpleFunction::arc(exp_descriptor(), invoke_exp),
        SimpleFunction::arc(ln_descriptor(), invoke_ln),
        SimpleFunction::arc(log10_descriptor(), invoke_log10),
        SimpleFunction::arc(log2_descriptor(), invoke_log2),
        SimpleFunction::arc(log_descriptor(), invoke_log),
        SimpleFunction::arc(log1p_descriptor(), invoke_log1p),
        SimpleFunction::arc(expm1_descriptor(), invoke_expm1),
    ]
}
