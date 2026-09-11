//! Descriptive statistics.
//!
//! Integer, rational, and decimal inputs are computed exactly; float64 inputs
//! are computed with stable float algorithms and marked approximate. Input
//! arrays are never mutated and non-finite or non-numeric entries are rejected.

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, CostClass, Example, FunctionDescriptor, Outcome, ParamDescriptor, SimpleFunction,
    require_mode,
};
use bicmath_core::envelope::Exactness;
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::number::{Number, NumericMode};
use bicmath_core::schema::ValueSchema;
use bicmath_core::value::Value;
use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::Zero;
use std::sync::Arc;

use crate::common::*;
use crate::mathfn::sqrt;

fn moment_value(moment: &Moment) -> Result<Value, EngineError> {
    match moment {
        Moment::Exact(value) => Ok(rational_value(value.clone())),
        Moment::Float(value) => float_value(*value),
    }
}

// ---------------------------------------------------------------------------
// count
// ---------------------------------------------------------------------------

fn count_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "statistics.count",
        "statistics",
        "1.0.0",
        "Count",
        "Count the observations in an array.",
    )
    .with_description(
        "Returns the number of entries. Every entry must be a finite number; non-numeric or \
         non-finite entries are rejected rather than silently skipped.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "values",
        "Observations to count.",
        array_schema(any_number_schema()),
    )])
    .with_output(integer_schema(), "Number of observations.")
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_method_ref("docs/methods/statistics.md#count")
    .with_examples(vec![
        Example::new(
            "count three values",
            example_args(&[("values", serde_json::json!([1, 2, 3]))]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "integer", "value": "3"}),
        )),
    ])
}

fn invoke_count(args: &Args, _ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let values = args.array("values")?;
    for (index, item) in values.iter().enumerate() {
        match item {
            Value::Number(number) => {
                ensure_finite(number).map_err(|e| e.with_path(format!("values[{index}]")))?;
            }
            other => {
                return Err(EngineError::malformed(format!(
                    "expected a number at values[{index}], found {}",
                    other.kind_name()
                ))
                .with_path(format!("values[{index}]")));
            }
        }
    }
    Ok(Outcome::exact(Value::integer(BigInt::from(values.len()))))
}

// ---------------------------------------------------------------------------
// mean
// ---------------------------------------------------------------------------

fn mean_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "statistics.mean",
        "statistics",
        "1.0.0",
        "Arithmetic mean",
        "Exact arithmetic mean of a non-empty array.",
    )
    .with_description(
        "Integer, rational, and decimal inputs produce an exact integer or rational mean. \
         Float64 inputs require auto or scientific mode, are computed in binary64, and are \
         marked approximate. An empty array is rejected with insufficient_observations.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "values",
        "Observations.",
        array_schema(any_number_schema()),
    )])
    .with_output(any_number_schema(), "Arithmetic mean.")
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_method_ref("docs/methods/statistics.md#mean")
    .with_examples(vec![
        Example::new(
            "exact integer mean",
            example_args(&[("values", serde_json::json!([1, 2, 3]))]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "integer", "value": "2"}),
        )),
        Example::new(
            "exact rational mean",
            example_args(&[("values", serde_json::json!([1, 2]))]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "rational", "numerator": "3", "denominator": "2"}),
        )),
        Example::new(
            "empty array",
            example_args(&[("values", serde_json::json!([]))]),
        )
        .with_error(ErrorCode::InsufficientObservations),
    ])
}

fn invoke_mean(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let series = classify_series(args, "values", ctx)?;
    let moment = mean_moment(&series)?;
    match moment {
        Moment::Exact(value) => Ok(Outcome::exact(rational_value(value))),
        Moment::Float(value) => Ok(Outcome::approximate(float_value(value)?)),
    }
}

// ---------------------------------------------------------------------------
// weighted_mean
// ---------------------------------------------------------------------------

fn weighted_mean_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "statistics.weighted_mean",
        "statistics",
        "1.0.0",
        "Weighted mean",
        "Weighted arithmetic mean with frequency or reliability weights.",
    )
    .with_description(
        "Frequency weights must be non-negative integers and count repeated observations; \
         reliability weights must be non-negative numbers. Weights may be zero but the total \
         weight must be positive. Negative weights, length mismatches, and a zero total weight \
         are rejected. Exact inputs produce an exact rational result; float64 inputs require \
         auto or scientific mode and are marked approximate.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("values", "Observations.", array_schema(any_number_schema())),
        ParamDescriptor::required(
            "weights",
            "Non-negative weights, one per observation.",
            array_schema(any_number_schema()),
        ),
        ParamDescriptor::optional(
            "weight_type",
            "Weight semantics: frequency (default) or reliability.",
            bicmath_core::schema::ValueSchema::Enum {
                variants: vec!["frequency".to_string(), "reliability".to_string()],
            },
        ),
    ])
    .with_output(any_number_schema(), "Weighted mean.")
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_method_ref("docs/methods/statistics.md#weighted_mean")
    .with_examples(vec![
        Example::new(
            "frequency weights",
            example_args(&[
                ("values", serde_json::json!([1, 2])),
                ("weights", serde_json::json!([1, 1])),
            ]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "rational", "numerator": "3", "denominator": "2"}),
        )),
        Example::new(
            "negative weight",
            example_args(&[
                ("values", serde_json::json!([1, 2])),
                ("weights", serde_json::json!([1, -1])),
            ]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_weighted_mean(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let weight_type = parse_choice(
        args,
        "weight_type",
        "frequency",
        &["frequency", "reliability"],
    )?;
    let values = collect_numbers(args, "values")?;
    let weights = collect_numbers(args, "weights")?;
    if values.len() != weights.len() {
        return Err(EngineError::malformed(format!(
            "values and weights must have the same length ({} vs {})",
            values.len(),
            weights.len()
        )));
    }
    if values.is_empty() {
        return Err(insufficient(
            "weighted_mean requires at least one observation",
        ));
    }
    for (index, weight) in weights.iter().enumerate() {
        if weight.is_negative() {
            return Err(EngineError::domain("weights must be non-negative")
                .with_path(format!("weights[{index}]")));
        }
        if weight_type == "frequency" {
            let integral = match weight {
                Number::Integer(_) => true,
                Number::Rational(value) => value.is_integer(),
                Number::Decimal(value) => value.to_bigint_if_integral().is_some(),
                Number::Float64(value) => value.get().fract() == 0.0,
            };
            if !integral {
                return Err(
                    EngineError::domain("frequency weights must be non-negative integers")
                        .with_path(format!("weights[{index}]")),
                );
            }
        }
    }
    let has_float = values.iter().any(Number::is_float) || weights.iter().any(Number::is_float);
    if has_float {
        if ctx.numeric.mode == NumericMode::Exact {
            return Err(EngineError::new(
                ErrorCode::UnsupportedNumericMode,
                "weighted_mean with float64 values or weights requires auto or scientific mode",
            ));
        }
        let mut numerator = 0.0f64;
        let mut numerator_compensation = 0.0f64;
        let mut denominator = 0.0f64;
        let mut denominator_compensation = 0.0f64;
        for (value, weight) in values.iter().zip(weights.iter()) {
            let value = number_to_f64(value)?;
            let weight = number_to_f64(weight)?;
            let term = value * weight;
            let adjusted = term - numerator_compensation;
            let next = numerator + adjusted;
            numerator_compensation = (next - numerator) - adjusted;
            numerator = next;

            let adjusted = weight - denominator_compensation;
            let next = denominator + adjusted;
            denominator_compensation = (next - denominator) - adjusted;
            denominator = next;
        }
        if denominator == 0.0 {
            return Err(EngineError::domain(
                "total weight must be strictly positive",
            ));
        }
        return Ok(Outcome::approximate(float_value(numerator / denominator)?));
    }
    let mut numerator = BigRational::zero();
    let mut denominator = BigRational::zero();
    for (value, weight) in values.iter().zip(weights.iter()) {
        let value = value
            .as_exact_rational()
            .ok_or_else(|| EngineError::internal("exact value could not be converted"))?;
        let weight = weight
            .as_exact_rational()
            .ok_or_else(|| EngineError::internal("exact weight could not be converted"))?;
        numerator += &value * &weight;
        denominator += weight;
    }
    if denominator.is_zero() {
        return Err(EngineError::domain(
            "total weight must be strictly positive",
        ));
    }
    Ok(Outcome::exact(rational_value(numerator / denominator)))
}

// ---------------------------------------------------------------------------
// median
// ---------------------------------------------------------------------------

fn median_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "statistics.median",
        "statistics",
        "1.0.0",
        "Median",
        "Median of a non-empty array.",
    )
    .with_description(
        "The values are sorted numerically without modifying the input. An odd count returns \
         the middle value; an even count returns the exact mean of the two middle values. \
         Exact inputs stay exact (median([1, 2, 10, 100]) = 6); float64 inputs require auto \
         or scientific mode.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "values",
        "Observations.",
        array_schema(any_number_schema()),
    )])
    .with_output(any_number_schema(), "Median value.")
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_method_ref("docs/methods/statistics.md#median")
    .with_examples(vec![
        Example::new(
            "even count",
            example_args(&[("values", serde_json::json!([1, 2, 10, 100]))]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "integer", "value": "6"}),
        )),
        Example::new(
            "odd count",
            example_args(&[("values", serde_json::json!([2, 10, 30]))]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "integer", "value": "10"}),
        )),
    ])
}

fn invoke_median(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let series = classify_series(args, "values", ctx)?;
    let (value, exactness) = median_number(&series)?;
    Ok(Outcome::new(Value::Number(value), exactness))
}

// ---------------------------------------------------------------------------
// mode
// ---------------------------------------------------------------------------

fn mode_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "statistics.mode",
        "statistics",
        "1.0.0",
        "Mode",
        "All tied modes with their counts.",
    )
    .with_description(
        "Returns every value that occurs most often, sorted ascending, with the matching \
         counts. Ties are reported in full; there is no arbitrary winner. The method label is \
         always all_modes.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "values",
        "Observations.",
        array_schema(any_number_schema()),
    )])
    .with_output(
        record_schema(
            vec![
                field("modes", array_schema(any_number_schema())),
                field("counts", array_schema(integer_schema())),
                field("method", text_schema()),
            ],
            false,
        ),
        "Record with modes, counts, and the method label all_modes.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_method_ref("docs/methods/statistics.md#mode")
    .with_examples(vec![
        Example::new(
            "single mode",
            example_args(&[("values", serde_json::json!([1, 2, 2, 3]))]),
        )
        .with_value(parse_value(serde_json::json!({
            "modes": [2],
            "counts": [2],
            "method": "all_modes"
        }))),
    ])
}

fn invoke_mode(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let series = classify_series(args, "values", ctx)?;
    let (modes, counts) = mode_numbers(&series)?;
    let value = record(vec![
        (
            "modes",
            array_value(modes.into_iter().map(number_value).collect()),
        ),
        (
            "counts",
            array_value(
                counts
                    .into_iter()
                    .map(|count| Value::integer(BigInt::from(count)))
                    .collect(),
            ),
        ),
        ("method", text("all_modes")),
    ]);
    let exactness = if series.is_float() {
        Exactness::Approximate
    } else {
        Exactness::Exact
    };
    Ok(Outcome::new(value, exactness))
}

// ---------------------------------------------------------------------------
// min / max
// ---------------------------------------------------------------------------

fn minmax_descriptor(id: &str, title: &str, summary: &str, minimum: bool) -> FunctionDescriptor {
    FunctionDescriptor::new(id, "statistics", "1.0.0", title, summary)
        .with_description(
            "Returns the smallest/largest observation without modifying the input. An empty \
             array is rejected with insufficient_observations. Exact inputs return the exact \
             selected value; float64 inputs require auto or scientific mode.",
        )
        .with_parameters(vec![ParamDescriptor::required(
            "values",
            "Observations.",
            array_schema(any_number_schema()),
        )])
        .with_output(any_number_schema(), "Selected extreme value.")
        .with_modes(all_modes())
        .with_cost(CostClass::Linear)
        .with_method_ref("docs/methods/statistics.md#min-max")
        .with_examples(vec![
            Example::new(
                "extreme value",
                example_args(&[("values", serde_json::json!([3, 1, 2]))]),
            )
            .with_value(parse_value(serde_json::json!({
                "kind": "integer",
                "value": if minimum { "1" } else { "3" }
            }))),
        ])
}

fn invoke_min(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    select_minmax(args, ctx, true)
}

fn invoke_max(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    select_minmax(args, ctx, false)
}

fn select_minmax(args: &Args, ctx: &ExecContext, minimum: bool) -> Result<Outcome, EngineError> {
    let series = classify_series(args, "values", ctx)?;
    let (value, exactness) = extreme_number(&series, minimum)?;
    Ok(Outcome::new(Value::Number(value), exactness))
}

// ---------------------------------------------------------------------------
// quantile
// ---------------------------------------------------------------------------

fn quantile_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "statistics.quantile",
        "statistics",
        "1.0.0",
        "Quantile",
        "Quantile with an explicit interpolation convention.",
    )
    .with_description(
        "Let h = (n - 1) * q. linear (default, R-7) returns \
         x[floor(h)] + (h - floor(h)) * (x[floor(h)+1] - x[floor(h)]); lower returns \
         x[floor(h)]; higher returns x[ceil(h)]; midpoint returns the mean of those two order \
         statistics; nearest returns x[round_half_to_even(h)]. q must be in [0, 1]. Exact \
         inputs stay exact; float64 inputs require auto or scientific mode.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("values", "Observations.", array_schema(any_number_schema())),
        ParamDescriptor::required("q", "Quantile level in [0, 1].", any_number_schema()),
        ParamDescriptor::optional(
            "method",
            "Interpolation method: linear (default), lower, higher, midpoint, nearest.",
            bicmath_core::schema::ValueSchema::Enum {
                variants: vec![
                    "linear".to_string(),
                    "lower".to_string(),
                    "higher".to_string(),
                    "midpoint".to_string(),
                    "nearest".to_string(),
                ],
            },
        ),
    ])
    .with_output(any_number_schema(), "Quantile value.")
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_method_ref("docs/methods/statistics.md#quantile")
    .with_examples(vec![
        Example::new(
            "linear interpolation",
            example_args(&[
                ("values", serde_json::json!([1, 2, 3, 4])),
                ("q", serde_json::json!(0.5)),
            ]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "rational", "numerator": "5", "denominator": "2"}),
        )),
        Example::new(
            "out of range q",
            example_args(&[
                ("values", serde_json::json!([1, 2, 3])),
                ("q", serde_json::json!(1.5)),
            ]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_quantile(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let method = parse_choice(
        args,
        "method",
        "linear",
        &["linear", "lower", "higher", "midpoint", "nearest"],
    )?;
    let q = args.number("q")?.clone();
    let series = classify_series(args, "values", ctx)?;
    let (value, exactness) = quantile_number(&series, &q, &method)?;
    Ok(Outcome::new(Value::Number(value), exactness))
}

// ---------------------------------------------------------------------------
// variance / stddev / covariance / correlation
// ---------------------------------------------------------------------------

fn variance_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "statistics.variance",
        "statistics",
        "1.0.0",
        "Variance",
        "Sample or population variance with explicit delta degrees of freedom.",
    )
    .with_description(
        "Computes sum((x - mean)^2) / (n - ddof). ddof must be a non-negative integer and n \
         must be strictly greater than ddof, otherwise insufficient_observations is returned. \
         Exact inputs use exact rational arithmetic (variance([1, 2, 3], ddof=0) = 2/3); \
         float64 inputs use Welford's stable one-pass algorithm and require auto or scientific \
         mode.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("values", "Observations.", array_schema(any_number_schema())),
        ParamDescriptor::required(
            "ddof",
            "Delta degrees of freedom: 0 for the population variance, 1 for the sample variance.",
            integer_schema(),
        ),
    ])
    .with_output(any_number_schema(), "Variance.")
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_method_ref("docs/methods/statistics.md#variance")
    .with_examples(vec![
        Example::new(
            "population variance",
            example_args(&[
                ("values", serde_json::json!([1, 2, 3])),
                ("ddof", serde_json::json!(0)),
            ]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "rational", "numerator": "2", "denominator": "3"}),
        )),
        Example::new(
            "sample variance",
            example_args(&[
                ("values", serde_json::json!([1, 2, 3])),
                ("ddof", serde_json::json!(1)),
            ]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "integer", "value": "1"}),
        )),
        Example::new(
            "too few observations",
            example_args(&[
                ("values", serde_json::json!([1])),
                ("ddof", serde_json::json!(1)),
            ]),
        )
        .with_error(ErrorCode::InsufficientObservations),
    ])
}

fn invoke_variance(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let ddof = non_negative_u64(&args.integer("ddof")?, "ddof")?;
    let series = classify_series(args, "values", ctx)?;
    let moment = variance_moment(&series, ddof)?;
    match moment {
        Moment::Exact(value) => Ok(Outcome::exact(rational_value(value))),
        Moment::Float(value) => Ok(Outcome::approximate(float_value(value)?)),
    }
}

fn stddev_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "statistics.stddev",
        "statistics",
        "1.0.0",
        "Standard deviation",
        "Square root of the variance with explicit exactness handling.",
    )
    .with_description(
        "Computes sqrt(variance(values, ddof)). When the variance is a perfect square in the \
         selected representation the result is exact. Otherwise exact mode returns \
         unsupported_numeric_mode, auto mode returns a decimal approximation marked \
         approximate, and scientific mode returns float64. Float64 inputs require auto or \
         scientific mode.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("values", "Observations.", array_schema(any_number_schema())),
        ParamDescriptor::required(
            "ddof",
            "Delta degrees of freedom: 0 for the population standard deviation, 1 for the sample.",
            integer_schema(),
        ),
    ])
    .with_output(any_number_schema(), "Standard deviation.")
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_method_ref("docs/methods/statistics.md#stddev")
    .with_examples(vec![
        Example::new(
            "exact perfect square",
            example_args(&[
                ("values", serde_json::json!([1, 2, 3])),
                ("ddof", serde_json::json!(1)),
            ]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "integer", "value": "1"}),
        )),
        Example::new(
            "non-square variance in exact mode",
            example_args(&[
                ("values", serde_json::json!([1, 2, 3])),
                ("ddof", serde_json::json!(0)),
            ]),
        )
        .with_error(ErrorCode::UnsupportedNumericMode),
    ])
}

fn invoke_stddev(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let ddof = non_negative_u64(&args.integer("ddof")?, "ddof")?;
    let series = classify_series(args, "values", ctx)?;
    let moment = variance_moment(&series, ddof)?;
    let (value, exactness) = stddev_moment(&moment, ctx.numeric.mode)?;
    Ok(Outcome::new(Value::Number(value), exactness))
}

fn covariance_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "statistics.covariance",
        "statistics",
        "1.0.0",
        "Covariance",
        "Sample or population covariance of two paired series.",
    )
    .with_description(
        "Computes sum((x - mean_x) * (y - mean_y)) / (n - ddof) for paired observations. The \
         two arrays must have equal length; n must be strictly greater than ddof. Exact inputs \
         use exact rational arithmetic; float64 inputs use a stable two-pass centered \
         algorithm and require auto or scientific mode.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("xs", "First series.", array_schema(any_number_schema())),
        ParamDescriptor::required(
            "ys",
            "Second series, paired with xs.",
            array_schema(any_number_schema()),
        ),
        ParamDescriptor::required("ddof", "Delta degrees of freedom.", integer_schema()),
    ])
    .with_output(any_number_schema(), "Covariance.")
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_method_ref("docs/methods/statistics.md#covariance")
    .with_examples(vec![
        Example::new(
            "covariance of identical series",
            example_args(&[
                ("xs", serde_json::json!([1, 2, 3])),
                ("ys", serde_json::json!([1, 2, 3])),
                ("ddof", serde_json::json!(0)),
            ]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "rational", "numerator": "2", "denominator": "3"}),
        )),
        Example::new(
            "length mismatch",
            example_args(&[
                ("xs", serde_json::json!([1, 2, 3])),
                ("ys", serde_json::json!([1, 2])),
                ("ddof", serde_json::json!(0)),
            ]),
        )
        .with_error(ErrorCode::MalformedInput),
    ])
}

fn invoke_covariance(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let ddof = non_negative_u64(&args.integer("ddof")?, "ddof")?;
    let xs = classify_series(args, "xs", ctx)?;
    let ys = classify_series(args, "ys", ctx)?;
    if xs.len() != ys.len() {
        return Err(EngineError::malformed(format!(
            "xs and ys must have the same length ({} vs {})",
            xs.len(),
            ys.len()
        )));
    }
    let moment = covariance_moment(&xs, &ys, ddof)?;
    match moment {
        Moment::Exact(value) => Ok(Outcome::exact(rational_value(value))),
        Moment::Float(value) => Ok(Outcome::approximate(float_value(value)?)),
    }
}

fn correlation_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "statistics.correlation",
        "statistics",
        "1.0.0",
        "Pearson correlation",
        "Pearson product-moment correlation coefficient.",
    )
    .with_description(
        "Returns the Pearson correlation r = cov(x, y) / (sd(x) * sd(y)) as float64. If either \
         series has zero variance the result is undefined and a domain_violation is returned \
         instead of NaN. At least two paired observations are required. The output is always \
         approximate, so exact mode is not supported.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("xs", "First series.", array_schema(any_number_schema())),
        ParamDescriptor::required(
            "ys",
            "Second series, paired with xs.",
            array_schema(any_number_schema()),
        ),
    ])
    .with_output(float64_schema(), "Pearson correlation coefficient.")
    .with_modes(inferential_modes())
    .with_cost(CostClass::Linear)
    .with_method_ref("docs/methods/statistics.md#correlation")
    .with_examples(vec![
        Example::new(
            "perfect positive correlation",
            example_args(&[
                ("xs", serde_json::json!([1, 2, 3])),
                ("ys", serde_json::json!([2, 4, 6])),
            ]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "float64", "value": "1"}),
        )),
        Example::new(
            "zero variance",
            example_args(&[
                ("xs", serde_json::json!([1, 1, 1])),
                ("ys", serde_json::json!([1, 2, 3])),
            ]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_correlation(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.correlation")?;
    let xs = collect_numbers(args, "xs")?;
    let ys = collect_numbers(args, "ys")?;
    if xs.len() != ys.len() {
        return Err(EngineError::malformed(format!(
            "xs and ys must have the same length ({} vs {})",
            xs.len(),
            ys.len()
        )));
    }
    if xs.len() < 2 {
        return Err(insufficient(
            "correlation requires at least two paired observations",
        ));
    }
    let xs: Vec<f64> = xs
        .iter()
        .enumerate()
        .map(|(index, value)| number_to_f64(value).map_err(|e| e.with_path(format!("xs[{index}]"))))
        .collect::<Result<_, _>>()?;
    let ys: Vec<f64> = ys
        .iter()
        .enumerate()
        .map(|(index, value)| number_to_f64(value).map_err(|e| e.with_path(format!("ys[{index}]"))))
        .collect::<Result<_, _>>()?;
    let n = xs.len();
    let mean_x = mean_f64(&xs);
    let mean_y = mean_f64(&ys);
    let mut sxx = 0.0f64;
    let mut syy = 0.0f64;
    let mut sxy = 0.0f64;
    for index in 0..n {
        let dx = xs[index] - mean_x;
        let dy = ys[index] - mean_y;
        sxx += dx * dx;
        syy += dy * dy;
        sxy += dx * dy;
    }
    if sxx <= 0.0 || syy <= 0.0 {
        return Err(EngineError::domain(
            "correlation is undefined when either series has zero variance",
        ));
    }
    let denominator = sqrt(sxx * syy);
    if denominator <= 0.0 {
        return Err(EngineError::domain(
            "correlation is undefined when either series has zero variance",
        ));
    }
    let r = (sxy / denominator).clamp(-1.0, 1.0);
    Ok(Outcome::approximate(float_value(r)?))
}

// ---------------------------------------------------------------------------
// summary
// ---------------------------------------------------------------------------

fn summary_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "statistics.summary",
        "statistics",
        "1.0.0",
        "Summary statistics",
        "One-call descriptive summary with quantiles.",
    )
    .with_description(
        "Returns count, mean, min, max, median, sample variance (ddof = 1), sample standard \
         deviation, and the linear quantiles 0.25, 0.5, and 0.75. At least two observations \
         are required. Exact inputs stay exact wherever the representation allows; the \
         standard deviation follows the stddev exactness rules.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "values",
        "Observations.",
        array_schema(any_number_schema()),
    )])
    .with_output(
        record_schema(
            vec![
                field("count", integer_schema()),
                field("mean", any_number_schema()),
                field("min", any_number_schema()),
                field("max", any_number_schema()),
                field("median", any_number_schema()),
                field("variance", any_number_schema()),
                field("stddev", any_number_schema()),
                field("quantiles", ValueSchema::Any),
            ],
            false,
        ),
        "Record with count, mean, min, max, median, variance, stddev, and quantiles.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_method_ref("docs/methods/statistics.md#summary")
    .with_examples(vec![
        Example::new(
            "too few observations",
            example_args(&[("values", serde_json::json!([1]))]),
        )
        .with_error(ErrorCode::InsufficientObservations),
    ])
}

fn invoke_summary(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let series = classify_series(args, "values", ctx)?;
    if series.len() < 2 {
        return Err(insufficient(
            "summary requires at least two observations because variance uses ddof = 1",
        ));
    }
    let mean = mean_moment(&series)?;
    let variance = variance_moment(&series, 1)?;
    let (stddev, stddev_exactness) = stddev_moment(&variance, ctx.numeric.mode)?;
    let (minimum, _) = extreme_number(&series, true)?;
    let (maximum, _) = extreme_number(&series, false)?;
    let (median, _) = median_number(&series)?;
    let quarter = rational_to_number(BigRational::new(BigInt::from(1), BigInt::from(4)));
    let half = rational_to_number(BigRational::new(BigInt::from(1), BigInt::from(2)));
    let three_quarters = rational_to_number(BigRational::new(BigInt::from(3), BigInt::from(4)));
    let (q25, _) = quantile_number(&series, &quarter, "linear")?;
    let (q50, _) = quantile_number(&series, &half, "linear")?;
    let (q75, _) = quantile_number(&series, &three_quarters, "linear")?;
    let value = record(vec![
        ("count", integer_value(series.len() as u64)),
        ("mean", moment_value(&mean)?),
        ("min", Value::Number(minimum)),
        ("max", Value::Number(maximum)),
        ("median", Value::Number(median)),
        ("variance", moment_value(&variance)?),
        ("stddev", Value::Number(stddev)),
        (
            "quantiles",
            record(vec![
                ("0.25", Value::Number(q25)),
                ("0.5", Value::Number(q50)),
                ("0.75", Value::Number(q75)),
            ]),
        ),
    ]);
    let exactness = Exactness::Exact.combine(stddev_exactness);
    Ok(Outcome::new(value, exactness))
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

pub fn functions() -> Vec<Arc<dyn bicmath_core::contract::Function>> {
    vec![
        SimpleFunction::arc(count_descriptor(), invoke_count),
        SimpleFunction::arc(mean_descriptor(), invoke_mean),
        SimpleFunction::arc(weighted_mean_descriptor(), invoke_weighted_mean),
        SimpleFunction::arc(median_descriptor(), invoke_median),
        SimpleFunction::arc(mode_descriptor(), invoke_mode),
        SimpleFunction::arc(
            minmax_descriptor(
                "statistics.min",
                "Minimum",
                "Smallest observation in a non-empty array.",
                true,
            ),
            invoke_min,
        ),
        SimpleFunction::arc(
            minmax_descriptor(
                "statistics.max",
                "Maximum",
                "Largest observation in a non-empty array.",
                false,
            ),
            invoke_max,
        ),
        SimpleFunction::arc(quantile_descriptor(), invoke_quantile),
        SimpleFunction::arc(variance_descriptor(), invoke_variance),
        SimpleFunction::arc(stddev_descriptor(), invoke_stddev),
        SimpleFunction::arc(covariance_descriptor(), invoke_covariance),
        SimpleFunction::arc(correlation_descriptor(), invoke_correlation),
        SimpleFunction::arc(summary_descriptor(), invoke_summary),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn call(id: &str, raw: serde_json::Value) -> Result<Outcome, EngineError> {
        call_with_mode(id, raw, NumericMode::Auto)
    }

    fn call_with_mode(
        id: &str,
        raw: serde_json::Value,
        mode: NumericMode,
    ) -> Result<Outcome, EngineError> {
        let module = crate::module();
        let function = module
            .functions
            .iter()
            .find(|f| f.descriptor().id == id)
            .expect("function exists");
        let ctx = ExecContext {
            numeric: bicmath_core::number::NumericContext {
                mode,
                ..bicmath_core::number::NumericContext::default()
            },
            ..ExecContext::conservative()
        };
        let args_json = raw.as_object().expect("object args");
        let mut values = BTreeMap::new();
        for (name, value) in args_json {
            let param = function
                .descriptor()
                .parameter(name)
                .expect("parameter exists");
            values.insert(
                name.clone(),
                param
                    .schema
                    .coerce(value, name, &ctx.limits, true)
                    .expect("argument coerces"),
            );
        }
        function.invoke(&Args::new(values), &ctx)
    }

    fn text_of(outcome: &Outcome) -> String {
        match &outcome.value {
            Value::Number(number) => number.to_string(),
            other => panic!("expected number, got {other:?}"),
        }
    }

    #[test]
    fn exact_descriptive_values() {
        assert_eq!(
            text_of(&call("statistics.mean", serde_json::json!({"values": [1, 2, 3]})).unwrap()),
            "2"
        );
        assert_eq!(
            text_of(&call("statistics.mean", serde_json::json!({"values": [1, 2]})).unwrap()),
            "3/2"
        );
        assert_eq!(
            text_of(
                &call(
                    "statistics.median",
                    serde_json::json!({"values": [2, 10, 30]})
                )
                .unwrap()
            ),
            "10"
        );
        assert_eq!(
            text_of(
                &call(
                    "statistics.median",
                    serde_json::json!({"values": [1, 2, 10, 100]})
                )
                .unwrap()
            ),
            "6"
        );
        assert_eq!(
            text_of(
                &call(
                    "statistics.variance",
                    serde_json::json!({"values": [1, 2, 3], "ddof": 0})
                )
                .unwrap()
            ),
            "2/3"
        );
        assert_eq!(
            text_of(
                &call(
                    "statistics.variance",
                    serde_json::json!({"values": [1, 2, 3], "ddof": 1})
                )
                .unwrap()
            ),
            "1"
        );
    }

    #[test]
    fn original_input_is_not_mutated() {
        let raw = serde_json::json!({"values": [3, 1, 2]});
        let outcome = call("statistics.median", raw.clone()).unwrap();
        assert_eq!(text_of(&outcome), "2");
        let args = raw.as_object().unwrap();
        assert_eq!(args["values"], serde_json::json!([3, 1, 2]));
    }

    #[test]
    fn float_inputs_are_approximate() {
        let outcome = call(
            "statistics.mean",
            serde_json::json!({"values": [
                {"kind": "float64", "value": "1"},
                {"kind": "float64", "value": "2"}
            ]}),
        )
        .unwrap();
        assert_eq!(outcome.exactness, Exactness::Approximate);
        assert_eq!(text_of(&outcome), "1.5");
    }

    #[test]
    fn exact_mode_rejects_float_inputs() {
        let err = call_with_mode(
            "statistics.mean",
            serde_json::json!({"values": [{"kind": "float64", "value": "1"}]}),
            NumericMode::Exact,
        )
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::UnsupportedNumericMode);
    }

    #[test]
    fn non_numeric_entries_are_rejected() {
        let module = crate::module();
        let function = module
            .functions
            .iter()
            .find(|f| f.descriptor().id == "statistics.mean")
            .expect("function exists");
        let mut values = BTreeMap::new();
        values.insert(
            "values".to_string(),
            Value::Array(vec![
                Value::Number(Number::Integer(BigInt::from(1))),
                Value::text("not a number"),
            ]),
        );
        let err = function
            .invoke(&Args::new(values), &ExecContext::conservative())
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::MalformedInput);
    }

    #[test]
    fn mode_returns_all_ties() {
        let outcome = call(
            "statistics.mode",
            serde_json::json!({"values": [1, 2, 2, 3, 3]}),
        )
        .unwrap();
        let Value::Record(fields) = &outcome.value else {
            panic!("expected record");
        };
        assert_eq!(
            fields.get("modes"),
            Some(&Value::Array(vec![
                Value::Number(Number::Integer(BigInt::from(2))),
                Value::Number(Number::Integer(BigInt::from(3))),
            ]))
        );
    }

    #[test]
    fn weighted_mean_rejects_bad_weights() {
        let err = call(
            "statistics.weighted_mean",
            serde_json::json!({"values": [1, 2], "weights": [1, -1]}),
        )
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::DomainViolation);
        let err = call(
            "statistics.weighted_mean",
            serde_json::json!({"values": [1, 2], "weights": [0, 0]}),
        )
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::DomainViolation);
        let err = call(
            "statistics.weighted_mean",
            serde_json::json!({"values": [1, 2], "weights": [1]}),
        )
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::MalformedInput);
    }

    #[test]
    fn stddev_exact_and_approximate_paths() {
        let exact = call(
            "statistics.stddev",
            serde_json::json!({"values": [1, 2, 3], "ddof": 1}),
        )
        .unwrap();
        assert_eq!(exact.exactness, Exactness::Exact);
        let err = call_with_mode(
            "statistics.stddev",
            serde_json::json!({"values": [1, 2, 3], "ddof": 0}),
            NumericMode::Exact,
        )
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::UnsupportedNumericMode);
        let approximate = call(
            "statistics.stddev",
            serde_json::json!({"values": [1, 2, 3], "ddof": 0}),
        )
        .unwrap();
        assert_eq!(approximate.exactness, Exactness::Approximate);
    }

    #[test]
    fn correlation_detects_zero_variance() {
        let err = call(
            "statistics.correlation",
            serde_json::json!({"xs": [1, 1, 1], "ys": [1, 2, 3]}),
        )
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::DomainViolation);
    }

    #[test]
    fn quantile_methods() {
        let lower = call(
            "statistics.quantile",
            serde_json::json!({"values": [1, 2, 3, 4], "q": "0.5", "method": "lower"}),
        )
        .unwrap();
        assert_eq!(text_of(&lower), "2");
        let higher = call(
            "statistics.quantile",
            serde_json::json!({"values": [1, 2, 3, 4], "q": "0.5", "method": "higher"}),
        )
        .unwrap();
        assert_eq!(text_of(&higher), "3");
        // h = 1.5 rounds half-to-even to index 2, so the nearest value is 3.
        let nearest = call(
            "statistics.quantile",
            serde_json::json!({"values": [1, 2, 3, 4], "q": "0.5", "method": "nearest"}),
        )
        .unwrap();
        assert_eq!(text_of(&nearest), "3");
    }

    #[test]
    fn summary_exact_record() {
        let outcome = call(
            "statistics.summary",
            serde_json::json!({"values": [1, 2, 3]}),
        )
        .unwrap();
        assert_eq!(outcome.exactness, Exactness::Exact);
        let Value::Record(fields) = &outcome.value else {
            panic!("expected record");
        };
        assert_eq!(
            fields.get("variance"),
            Some(&Value::Number(Number::Integer(BigInt::from(1))))
        );
    }
}
