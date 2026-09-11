//! Sequential and always-valid inference.
//!
//! `sprt_bernoulli` runs Wald's sequential probability ratio test for a
//! Bernoulli proportion. The confidence sequences are time-uniform: at each
//! observation time `t` a pointwise concentration inequality is inverted at
//! level `alpha_t = alpha * 6 / (pi^2 t^2)`. Since the schedule sums to one
//! over `t = 1, 2, ...`, a union bound makes the intervals simultaneously
//! valid with probability at least `1 - alpha`. This discrete schedule follows
//! the time-uniform construction of Howard, Ramdas, McAuliffe, and Sekhon
//! (2021, Annals of Statistics 49(2), 1055-1080); the bound is deliberately
//! reported with the schedule it uses.

use std::sync::Arc;

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, Assumption, CostClass, Example, FunctionDescriptor, Outcome, ParamDescriptor,
    SimpleFunction, require_mode,
};
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::schema::ValueSchema;
use num_bigint::BigInt;

use crate::common::*;
use crate::mathfn::{ln, sqrt};

// ---------------------------------------------------------------------------
// Time-uniform schedule
// ---------------------------------------------------------------------------

/// `alpha_t = alpha * 6 / (pi^2 t^2)`, the discrete schedule whose sum over
/// `t = 1, 2, ...` is exactly `alpha`.
fn time_uniform_alpha(alpha: f64, t: f64) -> f64 {
    alpha * 6.0 / (std::f64::consts::PI * std::f64::consts::PI * t * t)
}

fn validate_count_pair(
    successes: &BigInt,
    n: &BigInt,
    prefix: &str,
) -> Result<(u64, u64), EngineError> {
    let n_value = non_negative_u64(n, &format!("{prefix}n"))?;
    if n_value == 0 {
        return Err(EngineError::domain("n must be at least 1").with_path(format!("{prefix}n")));
    }
    let s_value = non_negative_u64(successes, &format!("{prefix}successes"))?;
    if s_value > n_value {
        return Err(
            EngineError::domain("successes must be between 0 and n inclusive")
                .with_path(format!("{prefix}successes")),
        );
    }
    Ok((s_value, n_value))
}

fn attach_assumptions(mut outcome: Outcome, prefix: &str, statements: &[&str]) -> Outcome {
    for (index, statement) in statements.iter().enumerate() {
        outcome = outcome.with_assumption(Assumption::unverified(
            format!("{prefix}_{index}"),
            *statement,
        ));
    }
    outcome
}

// ---------------------------------------------------------------------------
// sprt_bernoulli
// ---------------------------------------------------------------------------

fn sprt_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "statistics.sprt_bernoulli",
        "statistics",
        "1.0.0",
        "Wald sequential probability ratio test for a Bernoulli proportion",
        "Sequential log-likelihood ratio test between two simple Bernoulli hypotheses.",
    )
    .with_description(
        "Tests H0: p = p0 against H1: p = p1 with Wald's sequential probability ratio test, \
         where 0 < p0 < p1 < 1. successes_a and successes_b are the successes in two \
         successive looks (or two independent batches) with n_a and n_b trials; the batches \
         are pooled into total successes S and total trials N, and the Bernoulli \
         log-likelihood ratio LLR = S ln(p1 / p0) + (N - S) ln((1 - p1) / (1 - p0)) is \
         compared with the Wald boundaries ln((1 - beta) / alpha) and ln(beta / (1 - alpha)). \
         The decision is accept_h1 when LLR >= the upper boundary, accept_h0 when LLR <= the \
         lower boundary, and continue otherwise. alpha (default 0.05) and beta (default 0.1) \
         are the type I and type II error probabilities, each strictly between 0 and 1. The \
         output reports method = \"wald_sprt\", the pooled counts, the thresholds, and the \
         assumptions; the error rates hold under optional stopping because the test is \
         sequential by construction.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "successes_a",
            "Successes in the first batch; integer in 0..=n_a.",
            integer_schema(),
        ),
        ParamDescriptor::required(
            "n_a",
            "Trials in the first batch; integer >= 1.",
            integer_schema(),
        ),
        ParamDescriptor::required(
            "successes_b",
            "Successes in the second batch; integer in 0..=n_b.",
            integer_schema(),
        ),
        ParamDescriptor::required(
            "n_b",
            "Trials in the second batch; integer >= 1.",
            integer_schema(),
        ),
        ParamDescriptor::required(
            "p0",
            "Null success probability; strictly between 0 and p1.",
            any_number_schema(),
        ),
        ParamDescriptor::required(
            "p1",
            "Alternative success probability; strictly between p0 and 1.",
            any_number_schema(),
        ),
        ParamDescriptor::optional(
            "alpha",
            "Type I error probability in (0, 1); default 0.05.",
            any_number_schema(),
        ),
        ParamDescriptor::optional(
            "beta",
            "Type II error probability in (0, 1); default 0.1.",
            any_number_schema(),
        ),
    ])
    .with_output(
        record_schema(
            vec![
                field("log_likelihood_ratio", float64_schema()),
                field("decision", text_schema()),
                field("threshold_upper", float64_schema()),
                field("threshold_lower", float64_schema()),
                field("total_successes", integer_schema()),
                field("total_trials", integer_schema()),
                field("alpha", float64_schema()),
                field("beta", float64_schema()),
                field("method", text_schema()),
                field("assumptions", array_schema(text_schema())),
            ],
            false,
        ),
        "Wald SPRT record with the pooled counts and decision boundaries.",
    )
    .with_modes(inferential_modes())
    .with_cost(CostClass::Constant)
    .with_method_ref("docs/methods/statistics.md#sprt_bernoulli")
    .with_examples(vec![
        Example::new(
            "clear evidence for the alternative",
            example_args(&[
                ("successes_a", serde_json::json!(9)),
                ("n_a", serde_json::json!(10)),
                ("successes_b", serde_json::json!(9)),
                ("n_b", serde_json::json!(10)),
                ("p0", serde_json::json!(0.2)),
                ("p1", serde_json::json!(0.4)),
            ]),
        )
        .with_contains("accept_h1"),
        Example::new(
            "the null is not below the alternative",
            example_args(&[
                ("successes_a", serde_json::json!(1)),
                ("n_a", serde_json::json!(10)),
                ("successes_b", serde_json::json!(1)),
                ("n_b", serde_json::json!(10)),
                ("p0", serde_json::json!(0.4)),
                ("p1", serde_json::json!(0.2)),
            ]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_sprt(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.sprt_bernoulli")?;
    let (successes_a, n_a) =
        validate_count_pair(&args.integer("successes_a")?, &args.integer("n_a")?, "a_")?;
    let (successes_b, n_b) =
        validate_count_pair(&args.integer("successes_b")?, &args.integer("n_b")?, "b_")?;
    let p0 = scalar_f64(args, "p0")?;
    let p1 = scalar_f64(args, "p1")?;
    if !(0.0 < p0 && p0 < p1 && p1 < 1.0) {
        return Err(EngineError::domain(
            "the simple hypotheses must satisfy 0 < p0 < p1 < 1",
        ));
    }
    let alpha = optional_f64_param(args, "alpha")?.unwrap_or(0.05);
    let beta = optional_f64_param(args, "beta")?.unwrap_or(0.1);
    if !(0.0..1.0).contains(&alpha) {
        return Err(
            EngineError::domain("alpha must be strictly between 0 and 1")
                .with_path("alpha".to_string()),
        );
    }
    if !(0.0..1.0).contains(&beta) {
        return Err(EngineError::domain("beta must be strictly between 0 and 1")
            .with_path("beta".to_string()));
    }
    let total_successes = successes_a.checked_add(successes_b).ok_or_else(|| {
        EngineError::new(
            ErrorCode::ResourceLimit,
            "the total success count overflows",
        )
    })?;
    let total_trials = n_a.checked_add(n_b).ok_or_else(|| {
        EngineError::new(ErrorCode::ResourceLimit, "the total trial count overflows")
    })?;
    let successes = total_successes as f64;
    let failures = (total_trials - total_successes) as f64;
    let log_likelihood_ratio = successes * ln(p1 / p0) + failures * ln((1.0 - p1) / (1.0 - p0));
    let threshold_upper = ln((1.0 - beta) / alpha);
    let threshold_lower = ln(beta / (1.0 - alpha));
    let decision = if log_likelihood_ratio >= threshold_upper {
        "accept_h1"
    } else if log_likelihood_ratio <= threshold_lower {
        "accept_h0"
    } else {
        "continue"
    };
    let assumptions = [
        "the observations are independent Bernoulli draws with a constant success probability",
        "successes_a/n_a and successes_b/n_b are successive looks at the same stream (or two independent batches) and are pooled before the test",
        "the hypotheses are simple, H0: p = p0 versus H1: p = p1, and the test statistic is the Bernoulli log-likelihood ratio",
        "the thresholds are the Wald boundaries ln((1 - beta) / alpha) and ln(beta / (1 - alpha)); no optional-stopping correction is needed because the test is sequential by construction",
    ];
    let value = record(vec![
        ("log_likelihood_ratio", float_value(log_likelihood_ratio)?),
        ("decision", text(decision)),
        ("threshold_upper", float_value(threshold_upper)?),
        ("threshold_lower", float_value(threshold_lower)?),
        ("total_successes", integer_value(total_successes)),
        ("total_trials", integer_value(total_trials)),
        ("alpha", float_value(alpha)?),
        ("beta", float_value(beta)?),
        ("method", text("wald_sprt")),
        ("assumptions", assumptions_value(&assumptions)),
    ]);
    Ok(attach_assumptions(
        Outcome::approximate(value),
        "sprt_bernoulli",
        &assumptions,
    ))
}

// ---------------------------------------------------------------------------
// confidence_sequence_mean
// ---------------------------------------------------------------------------

fn confidence_sequence_mean_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "statistics.confidence_sequence_mean",
        "statistics",
        "1.0.0",
        "Time-uniform confidence sequence for a mean",
        "Always-valid confidence sequence for a mean using a time-uniform concentration bound.",
    )
    .with_description(
        "Computes a confidence sequence for the mean that is valid simultaneously for every \
         observation time t = 1, ..., n. At each t the bound inverts a pointwise \
         concentration inequality at level alpha_t = alpha * 6 / (pi^2 t^2), where \
         alpha = 1 - confidence; the schedule sums to alpha over t = 1, 2, ..., so the union \
         bound gives coverage at least confidence for all t. method = \"hoeffding\" (default) \
         requires either the known sub-Gaussian standard deviation sigma, or a known bounded \
         range supplied as lower and upper (every observation must lie inside it). \
         method = \"empirical_bernstein\" uses the observed range and the empirical (biased) \
         variance and ignores sigma. The output reports estimate, lower, upper, half_width, n, \
         method, the variance_source actually used, confidence, and the assumptions. The \
         Howard-Ramdas-McAuliffe-Sekhon time-uniform construction is used with this discrete \
         schedule; widths shrink at a root-logarithmic rate rather than the 1/sqrt(n) rate of \
         a fixed-time interval.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "values",
            "Observations in observation order.",
            array_schema(any_number_schema()),
        ),
        ParamDescriptor::optional(
            "confidence",
            "Confidence level in (0, 1); default 0.95.",
            any_number_schema(),
        ),
        ParamDescriptor::optional(
            "sigma",
            "Known sub-Gaussian standard deviation (variance proxy sigma^2); when supplied the sub-Gaussian bound is used instead of a bounded range. Only valid with method = \"hoeffding\".",
            any_number_schema(),
        ),
        ParamDescriptor::optional(
            "method",
            "Bound method: hoeffding (default) or empirical_bernstein.",
            ValueSchema::Enum {
                variants: vec!["hoeffding".to_string(), "empirical_bernstein".to_string()],
            },
        ),
        ParamDescriptor::optional(
            "lower",
            "Known lower bound of every observation; required for the hoeffding method without sigma.",
            any_number_schema(),
        ),
        ParamDescriptor::optional(
            "upper",
            "Known upper bound of every observation; required for the hoeffding method without sigma.",
            any_number_schema(),
        ),
    ])
    .with_output(
        record_schema(
            vec![
                field("estimate", float64_schema()),
                field("lower", float64_schema()),
                field("upper", float64_schema()),
                field("half_width", float64_schema()),
                field("n", integer_schema()),
                field("method", text_schema()),
                field("variance_source", text_schema()),
                field("confidence", float64_schema()),
                field("assumptions", array_schema(text_schema())),
            ],
            false,
        ),
        "Time-uniform confidence sequence record for the mean.",
    )
    .with_modes(inferential_modes())
    .with_cost(CostClass::Linear)
    .with_method_ref("docs/methods/statistics.md#confidence_sequence_mean")
    .with_examples(vec![
        Example::new(
            "bounded Hoeffding sequence",
            example_args(&[
                ("values", serde_json::json!([0, 1, 2, 3, 4])),
                ("lower", serde_json::json!(0)),
                ("upper", serde_json::json!(4)),
            ]),
        )
        .with_contains("hoeffding"),
        Example::new(
            "empirical Bernstein sequence",
            example_args(&[
                ("values", serde_json::json!([1.0, 2.0, 3.0])),
                ("method", serde_json::json!("empirical_bernstein")),
            ]),
        )
        .with_contains("empirical_bernstein"),
        Example::new(
            "hoeffding without bounds",
            example_args(&[("values", serde_json::json!([1, 2, 3]))]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_confidence_sequence_mean(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(
        ctx,
        &inferential_modes(),
        "statistics.confidence_sequence_mean",
    )?;
    let confidence = confidence_param(args)?;
    let method = parse_choice(
        args,
        "method",
        "hoeffding",
        &["hoeffding", "empirical_bernstein"],
    )?;
    let values = classify_series(args, "values", ctx)?.to_f64_vec()?;
    if values.is_empty() {
        return Err(insufficient("values must not be empty"));
    }
    let n = values.len();
    let t = n as f64;
    let alpha = 1.0 - confidence;
    let alpha_t = time_uniform_alpha(alpha, t);
    let estimate = mean_f64(&values);
    let sigma = optional_f64_param(args, "sigma")?;
    let (half_width, variance_source, assumptions): (f64, &str, Vec<&str>) = if method
        == "empirical_bernstein"
    {
        if sigma.is_some() {
            return Err(EngineError::domain(
                    "sigma applies only to the hoeffding method; empirical_bernstein uses the observed range",
                )
                .with_path("sigma".to_string()));
        }
        let mut minimum = values[0];
        let mut maximum = values[0];
        let mut sum_squares = 0.0f64;
        for value in &values {
            minimum = minimum.min(*value);
            maximum = maximum.max(*value);
            sum_squares += (value - estimate) * (value - estimate);
        }
        let range = maximum - minimum;
        let variance = sum_squares / t;
        let log_term = ln(6.0 / alpha_t);
        let half = sqrt(2.0 * variance * log_term / t) + 3.0 * range * log_term / t;
        (
            half,
            "observed_range",
            vec![
                "observations are independent and identically distributed",
                "the empirical Bernstein bound uses the observed range and the empirical (biased) variance",
                "the time-uniform schedule alpha_t = alpha * 6 / (pi^2 t^2) makes the interval valid for every t simultaneously with probability at least confidence",
            ],
        )
    } else if let Some(sigma) = sigma {
        if !sigma.is_finite() || sigma <= 0.0 {
            return Err(EngineError::domain("sigma must be strictly positive")
                .with_path("sigma".to_string()));
        }
        let half = sigma * sqrt(2.0 * ln(2.0 / alpha_t) / t);
        (
            half,
            "known_sigma",
            vec![
                "observations are independent and sub-Gaussian with variance proxy sigma^2",
                "sigma is treated as known rather than estimated from the sample",
                "the time-uniform schedule alpha_t = alpha * 6 / (pi^2 t^2) makes the interval valid for every t simultaneously with probability at least confidence",
            ],
        )
    } else {
        let lower = optional_f64_param(args, "lower")?.ok_or_else(|| {
            EngineError::domain(
                "the hoeffding method requires lower and upper bounds or a known sigma",
            )
            .with_path("lower".to_string())
        })?;
        let upper = optional_f64_param(args, "upper")?.ok_or_else(|| {
            EngineError::domain(
                "the hoeffding method requires lower and upper bounds or a known sigma",
            )
            .with_path("upper".to_string())
        })?;
        if !lower.is_finite() || !upper.is_finite() || upper <= lower {
            return Err(EngineError::domain(
                "lower and upper must be finite with lower < upper",
            ));
        }
        for (index, value) in values.iter().enumerate() {
            if *value < lower || *value > upper {
                return Err(EngineError::domain(format!(
                    "observation {value} lies outside the declared range [{lower}, {upper}]"
                ))
                .with_path(format!("values[{index}]")));
            }
        }
        let range = upper - lower;
        let half = range * sqrt(ln(2.0 / alpha_t) / (2.0 * t));
        (
            half,
            "bounded_range",
            vec![
                "observations are independent and lie in the declared range [lower, upper] (checked for the supplied values)",
                "the range is treated as known rather than estimated from the sample",
                "the time-uniform schedule alpha_t = alpha * 6 / (pi^2 t^2) makes the interval valid for every t simultaneously with probability at least confidence",
            ],
        )
    };
    let lower_bound = estimate - half_width;
    let upper_bound = estimate + half_width;
    let value = record(vec![
        ("estimate", float_value(estimate)?),
        ("lower", float_value(lower_bound)?),
        ("upper", float_value(upper_bound)?),
        ("half_width", float_value(half_width)?),
        ("n", integer_value(n as u64)),
        ("method", text(method)),
        ("variance_source", text(variance_source)),
        ("confidence", float_value(confidence)?),
        ("assumptions", assumptions_value(&assumptions)),
    ]);
    Ok(attach_assumptions(
        Outcome::approximate(value),
        "confidence_sequence_mean",
        &assumptions,
    ))
}

// ---------------------------------------------------------------------------
// confidence_sequence_proportion
// ---------------------------------------------------------------------------

fn confidence_sequence_proportion_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "statistics.confidence_sequence_proportion",
        "statistics",
        "1.0.0",
        "Time-uniform confidence sequence for a Bernoulli proportion",
        "Always-valid confidence sequence for a proportion using the time-uniform Hoeffding bound on [0, 1].",
    )
    .with_description(
        "Computes a time-uniform confidence sequence for a Bernoulli success probability from \
         successes successes in n trials. The pointwise Hoeffding bound on [0, 1] is inverted \
         at level alpha_t = alpha * 6 / (pi^2 t^2) with alpha = 1 - confidence and t = n, so \
         the interval covers the true proportion for every n simultaneously with probability \
         at least confidence. The endpoints are clipped to [0, 1]. The output reports estimate, \
         lower, upper, half_width, n, method = \"hoeffding\", confidence, and the assumptions.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "successes",
            "Number of successes; integer in 0..=n.",
            integer_schema(),
        ),
        ParamDescriptor::required("n", "Number of trials; integer >= 1.", integer_schema()),
        ParamDescriptor::optional(
            "confidence",
            "Confidence level in (0, 1); default 0.95.",
            any_number_schema(),
        ),
        ParamDescriptor::optional(
            "method",
            "Bound method; only hoeffding is supported.",
            ValueSchema::Enum {
                variants: vec!["hoeffding".to_string()],
            },
        ),
    ])
    .with_output(
        record_schema(
            vec![
                field("estimate", float64_schema()),
                field("lower", float64_schema()),
                field("upper", float64_schema()),
                field("half_width", float64_schema()),
                field("n", integer_schema()),
                field("method", text_schema()),
                field("confidence", float64_schema()),
                field("assumptions", array_schema(text_schema())),
            ],
            false,
        ),
        "Time-uniform confidence sequence record for a proportion.",
    )
    .with_modes(inferential_modes())
    .with_cost(CostClass::Constant)
    .with_method_ref("docs/methods/statistics.md#confidence_sequence_proportion")
    .with_examples(vec![
        Example::new(
            "three successes in ten trials",
            example_args(&[
                ("successes", serde_json::json!(3)),
                ("n", serde_json::json!(10)),
            ]),
        )
        .with_contains("hoeffding"),
        Example::new(
            "successes exceed trials",
            example_args(&[
                ("successes", serde_json::json!(12)),
                ("n", serde_json::json!(10)),
            ]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_confidence_sequence_proportion(
    args: &Args,
    ctx: &ExecContext,
) -> Result<Outcome, EngineError> {
    require_mode(
        ctx,
        &inferential_modes(),
        "statistics.confidence_sequence_proportion",
    )?;
    let (successes, n) = validate_count_pair(&args.integer("successes")?, &args.integer("n")?, "")?;
    let confidence = confidence_param(args)?;
    let method = parse_choice(args, "method", "hoeffding", &["hoeffding"])?;
    let estimate = successes as f64 / n as f64;
    let alpha = 1.0 - confidence;
    let alpha_t = time_uniform_alpha(alpha, n as f64);
    let half_width = sqrt(ln(2.0 / alpha_t) / (2.0 * n as f64));
    let lower = (estimate - half_width).max(0.0);
    let upper = (estimate + half_width).min(1.0);
    let assumptions = [
        "trials are independent Bernoulli draws with a constant success probability",
        "the time-uniform Hoeffding bound uses the support [0, 1] and the schedule alpha_t = alpha * 6 / (pi^2 t^2)",
        "the interval is valid for every n simultaneously with probability at least confidence, at the cost of a wider width than a fixed-n interval",
    ];
    let value = record(vec![
        ("estimate", float_value(estimate)?),
        ("lower", float_value(lower)?),
        ("upper", float_value(upper)?),
        ("half_width", float_value(half_width)?),
        ("n", integer_value(n)),
        ("method", text(method)),
        ("confidence", float_value(confidence)?),
        ("assumptions", assumptions_value(&assumptions)),
    ]);
    Ok(attach_assumptions(
        Outcome::approximate(value),
        "confidence_sequence_proportion",
        &assumptions,
    ))
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

pub fn functions() -> Vec<Arc<dyn bicmath_core::contract::Function>> {
    vec![
        SimpleFunction::arc(sprt_descriptor(), invoke_sprt),
        SimpleFunction::arc(
            confidence_sequence_mean_descriptor(),
            invoke_confidence_sequence_mean,
        ),
        SimpleFunction::arc(
            confidence_sequence_proportion_descriptor(),
            invoke_confidence_sequence_proportion,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use bicmath_core::value::Value;
    use std::collections::BTreeMap;

    fn call(id: &str, raw: serde_json::Value) -> Result<Outcome, EngineError> {
        let module = crate::module();
        let function = module
            .functions
            .iter()
            .find(|f| f.descriptor().id == id)
            .expect("function exists");
        let ctx = ExecContext::scientific();
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

    fn record_of(outcome: &Outcome) -> &BTreeMap<String, Value> {
        match &outcome.value {
            Value::Record(fields) => fields,
            other => panic!("expected record, got {other:?}"),
        }
    }

    fn field_f64(fields: &BTreeMap<String, Value>, name: &str) -> f64 {
        match fields.get(name) {
            Some(Value::Number(number)) => number.to_f64().expect("number"),
            other => panic!("expected numeric field {name}, got {other:?}"),
        }
    }

    fn field_text(fields: &BTreeMap<String, Value>, name: &str) -> String {
        match fields.get(name) {
            Some(Value::Text(text)) => text.clone(),
            other => panic!("expected text field {name}, got {other:?}"),
        }
    }

    fn close(actual: f64, expected: f64, tolerance: f64) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "expected {expected}, got {actual} (tolerance {tolerance})"
        );
    }

    #[test]
    fn sprt_decision_flips_with_the_observed_difference() {
        // Provenance: Python 3.14 stdlib values for p0 = 0.2, p1 = 0.4,
        // alpha = 0.05, beta = 0.1: upper = ln(18) = 2.8903717578961645,
        // lower = ln(0.1 / 0.95) = -2.251291798606495. With 2 successes in
        // 20 trials the LLR is -3.7919829430121688 (accept_h0); with 18 in 20
        // it is 11.901285105175454 (accept_h1); with 5 in 20 it is
        // -0.8494951839769898 (continue).
        let small = call(
            "statistics.sprt_bernoulli",
            serde_json::json!({
                "successes_a": 1, "n_a": 10,
                "successes_b": 1, "n_b": 10,
                "p0": 0.2, "p1": 0.4,
                "alpha": 0.05, "beta": 0.1
            }),
        )
        .unwrap();
        let small_fields = record_of(&small);
        assert_eq!(field_text(small_fields, "decision"), "accept_h0");
        close(
            field_f64(small_fields, "log_likelihood_ratio"),
            -3.791_982_943_012_168_8,
            1e-12,
        );
        close(
            field_f64(small_fields, "threshold_upper"),
            2.890_371_757_896_164_5,
            1e-12,
        );
        close(
            field_f64(small_fields, "threshold_lower"),
            -2.251_291_798_606_495,
            1e-12,
        );

        let large = call(
            "statistics.sprt_bernoulli",
            serde_json::json!({
                "successes_a": 9, "n_a": 10,
                "successes_b": 9, "n_b": 10,
                "p0": 0.2, "p1": 0.4,
                "alpha": 0.05, "beta": 0.1
            }),
        )
        .unwrap();
        let large_fields = record_of(&large);
        assert_eq!(field_text(large_fields, "decision"), "accept_h1");
        close(
            field_f64(large_fields, "log_likelihood_ratio"),
            11.901_285_105_175_454,
            1e-12,
        );

        let middle = call(
            "statistics.sprt_bernoulli",
            serde_json::json!({
                "successes_a": 3, "n_a": 10,
                "successes_b": 2, "n_b": 10,
                "p0": 0.2, "p1": 0.4,
                "alpha": 0.05, "beta": 0.1
            }),
        )
        .unwrap();
        let middle_fields = record_of(&middle);
        assert_eq!(field_text(middle_fields, "decision"), "continue");
        close(
            field_f64(middle_fields, "log_likelihood_ratio"),
            -0.849_495_183_976_989_8,
            1e-12,
        );
    }

    #[test]
    fn sprt_rejects_invalid_hypotheses_and_counts() {
        let error = call(
            "statistics.sprt_bernoulli",
            serde_json::json!({
                "successes_a": 1, "n_a": 10,
                "successes_b": 1, "n_b": 10,
                "p0": 0.4, "p1": 0.2
            }),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::DomainViolation);
        let error = call(
            "statistics.sprt_bernoulli",
            serde_json::json!({
                "successes_a": 11, "n_a": 10,
                "successes_b": 1, "n_b": 10,
                "p0": 0.2, "p1": 0.4
            }),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::DomainViolation);
    }

    #[test]
    fn confidence_sequence_mean_width_shrinks_and_covers() {
        // Provenance: Python 3.14 stdlib evaluation of
        // (upper - lower) * sqrt(ln(2 / alpha_t) / (2 t)) with
        // alpha_t = 0.05 * 6 / (pi^2 t^2) for values 0.1, ..., 1.0 on the
        // declared range [0, 1]: half_width(10) = 0.6630139494223621.
        let outcome = call(
            "statistics.confidence_sequence_mean",
            serde_json::json!({
                "values": [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0],
                "lower": 0,
                "upper": 1
            }),
        )
        .unwrap();
        let fields = record_of(&outcome);
        close(field_f64(fields, "estimate"), 0.55, 1e-12);
        close(
            field_f64(fields, "half_width"),
            0.663_013_949_422_362_1,
            1e-12,
        );
        assert!(field_f64(fields, "lower") <= field_f64(fields, "estimate"));
        assert!(field_f64(fields, "upper") >= field_f64(fields, "estimate"));

        let short = call(
            "statistics.confidence_sequence_mean",
            serde_json::json!({
                "values": [0.1, 0.2, 0.3, 0.4, 0.5],
                "lower": 0,
                "upper": 1
            }),
        )
        .unwrap();
        let long = call(
            "statistics.confidence_sequence_mean",
            serde_json::json!({
                "values": [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0],
                "lower": 0,
                "upper": 1
            }),
        )
        .unwrap();
        assert!(
            field_f64(record_of(&long), "half_width") < field_f64(record_of(&short), "half_width"),
            "the sequence width must shrink as n grows"
        );
    }

    #[test]
    fn confidence_sequence_mean_empirical_bernstein_width_shrinks() {
        // Provenance: Python 3.14 stdlib evaluation of
        // sqrt(2 v ln(6 / alpha_t) / t) + 3 R ln(6 / alpha_t) / t for the
        // alternating values [0.1, 0.9]: for t = 10 the half width is
        // 2.936262789405273 and for t = 20 it is 1.7779652039242477; the
        // bound shrinks with t because the schedule grows only
        // logarithmically.
        let short_values: Vec<f64> = std::iter::repeat_n([0.1, 0.9], 5).flatten().collect();
        let long_values: Vec<f64> = std::iter::repeat_n([0.1, 0.9], 10).flatten().collect();
        let short = call(
            "statistics.confidence_sequence_mean",
            serde_json::json!({"values": short_values, "method": "empirical_bernstein"}),
        )
        .unwrap();
        let long = call(
            "statistics.confidence_sequence_mean",
            serde_json::json!({"values": long_values, "method": "empirical_bernstein"}),
        )
        .unwrap();
        close(
            field_f64(record_of(&short), "half_width"),
            2.936_262_789_405_273,
            1e-12,
        );
        close(
            field_f64(record_of(&long), "half_width"),
            1.777_965_203_924_247_7,
            1e-12,
        );
        assert!(
            field_f64(record_of(&long), "half_width") < field_f64(record_of(&short), "half_width"),
            "the empirical Bernstein sequence width must shrink as n grows"
        );
        assert_eq!(
            field_text(record_of(&long), "variance_source"),
            "observed_range"
        );
    }

    #[test]
    fn confidence_sequence_mean_rejects_missing_bounds_and_out_of_range_values() {
        let error = call(
            "statistics.confidence_sequence_mean",
            serde_json::json!({"values": [1, 2, 3]}),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::DomainViolation);
        let error = call(
            "statistics.confidence_sequence_mean",
            serde_json::json!({"values": [0, 5], "lower": 0, "upper": 4}),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::DomainViolation);
    }

    #[test]
    fn confidence_sequence_proportion_shrinks_and_covers() {
        let outcome = call(
            "statistics.confidence_sequence_proportion",
            serde_json::json!({"successes": 3, "n": 10}),
        )
        .unwrap();
        let fields = record_of(&outcome);
        close(field_f64(fields, "estimate"), 0.3, 1e-12);
        assert!(field_f64(fields, "lower") <= 0.3);
        assert!(field_f64(fields, "upper") >= 0.3);
        assert_eq!(field_text(fields, "method"), "hoeffding");

        let short = call(
            "statistics.confidence_sequence_proportion",
            serde_json::json!({"successes": 3, "n": 10}),
        )
        .unwrap();
        let long = call(
            "statistics.confidence_sequence_proportion",
            serde_json::json!({"successes": 12, "n": 40}),
        )
        .unwrap();
        assert!(
            field_f64(record_of(&long), "half_width") < field_f64(record_of(&short), "half_width"),
            "the proportion sequence width must shrink as n grows"
        );
    }
}
