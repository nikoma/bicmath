//! Causal and quasi-experimental estimators.
//!
//! `difference_in_differences` is a means-based 2x2 estimator with an explicit
//! parallel-trends assumption. `propensity_score_weighting` fits a logistic
//! propensity model with the shared IRLS helper in `regression` and estimates
//! the average treatment effect on the treated by inverse-probability
//! weighting. Positivity is checked rather than assumed silently.

use std::sync::Arc;

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, Assumption, CostClass, Example, FunctionDescriptor, Outcome, ParamDescriptor,
    SimpleFunction, Warning, require_mode,
};
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::schema::ValueSchema;
use bicmath_core::value::Value;

use crate::common::*;
use crate::mathfn::{normal_quantile, sqrt};
use crate::regression::logistic_probabilities;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn interval_schema() -> ValueSchema {
    record_schema(
        vec![
            field("lower", float64_schema()),
            field("upper", float64_schema()),
        ],
        false,
    )
}

fn interval_value(lower: f64, upper: f64) -> Result<Value, EngineError> {
    Ok(record(vec![
        ("lower", float_value(lower)?),
        ("upper", float_value(upper)?),
    ]))
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

fn parse_covariates(value: &Value) -> Result<Vec<Vec<f64>>, EngineError> {
    let items = value
        .as_array()
        .map_err(|error| error.with_path("covariates".to_string()))?;
    if items.is_empty() {
        return Err(insufficient("covariates must not be empty"));
    }
    let mut rows = Vec::with_capacity(items.len());
    let mut width: Option<usize> = None;
    for (index, item) in items.iter().enumerate() {
        let row = item
            .as_array()
            .map_err(|error| error.with_path(format!("covariates[{index}]")))?;
        if row.is_empty() {
            return Err(
                EngineError::domain(format!("row {index} of covariates is empty"))
                    .with_path(format!("covariates[{index}]")),
            );
        }
        match width {
            None => width = Some(row.len()),
            Some(expected) if expected != row.len() => {
                return Err(EngineError::malformed(format!(
                    "ragged covariates: row {index} has {} entries, expected {expected}",
                    row.len()
                ))
                .with_path(format!("covariates[{index}]")));
            }
            Some(_) => {}
        }
        let mut values = Vec::with_capacity(row.len());
        for (column, cell) in row.iter().enumerate() {
            let number = cell
                .as_number()
                .map_err(|error| error.with_path(format!("covariates[{index}][{column}]")))?;
            values.push(
                number_to_f64(number)
                    .map_err(|error| error.with_path(format!("covariates[{index}][{column}]")))?,
            );
        }
        rows.push(values);
    }
    Ok(rows)
}

/// Reject propensities outside (0, 1) and warn when any is within 1e-6 of the
/// boundary. An empty overlap means a unit has zero probability of receiving
/// one of the two treatment arms, so the inverse-probability weight is not
/// defined.
fn validate_overlap(propensities: &[f64]) -> Result<Option<Warning>, EngineError> {
    let mut near_boundary = 0usize;
    for (index, propensity) in propensities.iter().enumerate() {
        if !propensity.is_finite() || *propensity <= 0.0 || *propensity >= 1.0 {
            return Err(EngineError::domain(format!(
                "empty overlap: unit {index} has propensity {propensity}, which is not strictly \
                 inside (0, 1); the propensity model assigns zero probability to one of the \
                 treatment arms"
            ))
            .with_path(format!("treatment[{index}]")));
        }
        if *propensity <= 1e-6 || *propensity >= 1.0 - 1e-6 {
            near_boundary += 1;
        }
    }
    Ok((near_boundary > 0).then(|| {
        Warning::new(
            "propensity_near_boundary",
            format!(
                "{near_boundary} propensity score(s) are within 1e-6 of 0 or 1; \
                 inverse-probability weights may be unstable"
            ),
        )
    }))
}

// ---------------------------------------------------------------------------
// difference_in_differences
// ---------------------------------------------------------------------------

fn did_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "statistics.difference_in_differences",
        "statistics",
        "1.0.0",
        "Difference-in-differences (2x2)",
        "Means-based difference-in-differences estimator with a Welch-style standard error.",
    )
    .with_description(
        "Estimates (treatment_post - treatment_pre) - (control_post - control_pre) from the \
         four group-period samples. The standard error combines the four unbiased sample \
         variances as var(control_pre) / n_control_pre + var(control_post) / n_control_post + \
         var(treatment_pre) / n_treatment_pre + var(treatment_post) / n_treatment_post, and \
         the interval is estimate +/- z * standard_error with the normal critical value at the \
         requested confidence level. Every sample needs at least two observations. The output \
         reports estimate, standard_error, the confidence interval, method = \
         \"difference_in_differences_2x2\", and the parallel-trends and no-interference \
         assumptions that the estimator requires.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "control_pre",
            "Control-group observations before the intervention.",
            array_schema(any_number_schema()),
        ),
        ParamDescriptor::required(
            "control_post",
            "Control-group observations after the intervention.",
            array_schema(any_number_schema()),
        ),
        ParamDescriptor::required(
            "treatment_pre",
            "Treatment-group observations before the intervention.",
            array_schema(any_number_schema()),
        ),
        ParamDescriptor::required(
            "treatment_post",
            "Treatment-group observations after the intervention.",
            array_schema(any_number_schema()),
        ),
        ParamDescriptor::optional(
            "confidence",
            "Confidence level in (0, 1); default 0.95.",
            any_number_schema(),
        ),
    ])
    .with_output(
        record_schema(
            vec![
                field("estimate", float64_schema()),
                field("standard_error", float64_schema()),
                field("confidence_interval", interval_schema()),
                field("confidence", float64_schema()),
                field("method", text_schema()),
                field("assumptions", array_schema(text_schema())),
            ],
            false,
        ),
        "Difference-in-differences record with its parallel-trends assumption.",
    )
    .with_modes(inferential_modes())
    .with_cost(CostClass::Linear)
    .with_method_ref("docs/methods/statistics.md#difference_in_differences")
    .with_examples(vec![
        Example::new(
            "treatment change exceeds control change",
            example_args(&[
                ("control_pre", serde_json::json!([1, 2, 3])),
                ("control_post", serde_json::json!([2, 3, 4])),
                ("treatment_pre", serde_json::json!([1, 2, 3])),
                ("treatment_post", serde_json::json!([4, 5, 6])),
            ]),
        )
        .with_contains("difference_in_differences_2x2"),
        Example::new(
            "one observation per group period",
            example_args(&[
                ("control_pre", serde_json::json!([1])),
                ("control_post", serde_json::json!([2])),
                ("treatment_pre", serde_json::json!([1])),
                ("treatment_post", serde_json::json!([4])),
            ]),
        )
        .with_error(ErrorCode::InsufficientObservations),
    ])
}

fn invoke_did(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(
        ctx,
        &inferential_modes(),
        "statistics.difference_in_differences",
    )?;
    let confidence = confidence_param(args)?;
    let control_pre = float_vector(args, "control_pre")?;
    let control_post = float_vector(args, "control_post")?;
    let treatment_pre = float_vector(args, "treatment_pre")?;
    let treatment_post = float_vector(args, "treatment_post")?;
    for (name, values) in [
        ("control_pre", &control_pre),
        ("control_post", &control_post),
        ("treatment_pre", &treatment_pre),
        ("treatment_post", &treatment_post),
    ] {
        if values.len() < 2 {
            return Err(insufficient(format!(
                "difference_in_differences requires at least two observations for {name}"
            )));
        }
    }
    let control_change = mean_f64(&control_post) - mean_f64(&control_pre);
    let treatment_change = mean_f64(&treatment_post) - mean_f64(&treatment_pre);
    let estimate = treatment_change - control_change;
    let variance = variance_f64(&control_pre, 1)? / control_pre.len() as f64
        + variance_f64(&control_post, 1)? / control_post.len() as f64
        + variance_f64(&treatment_pre, 1)? / treatment_pre.len() as f64
        + variance_f64(&treatment_post, 1)? / treatment_post.len() as f64;
    let standard_error = sqrt(variance);
    let critical = normal_quantile(1.0 - (1.0 - confidence) / 2.0, 0.0, 1.0)?;
    let lower = estimate - critical * standard_error;
    let upper = estimate + critical * standard_error;
    let assumptions = [
        "parallel trends: absent treatment, the treatment group's mean change would equal the control group's observed mean change",
        "the four group-period samples are independent and representative of their populations",
        "no interference or spillover between the groups",
        "the interval uses a normal approximation with a Welch-style sum of the four group-period variances divided by their sample sizes",
        "the estimate is a difference of means and does not adjust for covariates or changes in group composition",
    ];
    let value = record(vec![
        ("estimate", float_value(estimate)?),
        ("standard_error", float_value(standard_error)?),
        ("confidence_interval", interval_value(lower, upper)?),
        ("confidence", float_value(confidence)?),
        ("method", text("difference_in_differences_2x2")),
        ("assumptions", assumptions_value(&assumptions)),
    ]);
    Ok(attach_assumptions(
        Outcome::approximate(value),
        "difference_in_differences",
        &assumptions,
    ))
}

// ---------------------------------------------------------------------------
// propensity_score_weighting
// ---------------------------------------------------------------------------

fn ipw_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "statistics.propensity_score_weighting",
        "statistics",
        "1.0.0",
        "Propensity-score weighting (ATT)",
        "Inverse-probability-weighted average treatment effect on the treated.",
    )
    .with_description(
        "treatment must contain only 0 and 1 values, covariates is an array of covariate rows \
         (one row per unit), and outcomes holds one outcome per unit. A logistic regression of \
         treatment on the covariates (with an intercept, fitted by the shared IRLS helper) \
         gives the propensity score e(x) for each unit. The ATT is mean(outcome | treated) \
         minus the inverse-probability-weighted control mean. method = \"hajek\" (default) \
         normalizes the control weights e / (1 - e) by their sum; method = \
         \"horvitz_thompson\" divides the weighted control total by the number of treated \
         units. The standard error combines the treated sample variance with the weighted \
         control variance and treats the fitted propensities as fixed. Positivity is checked: \
         a propensity outside (0, 1) is rejected as an empty overlap, and propensities within \
         1e-6 of 0 or 1 raise a warning. The output reports att, standard_error, \
         weights_summary, propensity_summary, diagnostics {overlap_min, overlap_max, \
         extreme_weights}, method = \"ipw_att\", the estimator actually used, and the \
         no-unmeasured-confounding and model assumptions.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "treatment",
            "Binary treatment indicator per unit: 0 or 1.",
            array_schema(any_number_schema()),
        ),
        ParamDescriptor::required(
            "covariates",
            "Covariate rows, one row per unit (an array of equal-length numeric arrays).",
            ValueSchema::Any,
        ),
        ParamDescriptor::required(
            "outcomes",
            "Outcome per unit, aligned with treatment and covariates.",
            array_schema(any_number_schema()),
        ),
        ParamDescriptor::optional(
            "method",
            "Weighting estimator: hajek (default) or horvitz_thompson.",
            ValueSchema::Enum {
                variants: vec!["hajek".to_string(), "horvitz_thompson".to_string()],
            },
        ),
    ])
    .with_output(
        record_schema(
            vec![
                field("att", float64_schema()),
                field("standard_error", float64_schema()),
                field("weights_summary", ValueSchema::Any),
                field("propensity_summary", ValueSchema::Any),
                field("method", text_schema()),
                field("estimator", text_schema()),
                field("diagnostics", ValueSchema::Any),
                field("assumptions", array_schema(text_schema())),
            ],
            false,
        ),
        "Inverse-probability-weighted ATT record with propensity and weight diagnostics.",
    )
    .with_modes(inferential_modes())
    .with_cost(CostClass::Iterative)
    .with_method_ref("docs/methods/statistics.md#propensity_score_weighting")
    .with_examples(vec![
        Example::new(
            "small overlapping dataset",
            example_args(&[
                ("treatment", serde_json::json!([0, 0, 1, 0, 1, 1])),
                (
                    "covariates",
                    serde_json::json!([[0], [1], [2], [3], [4], [5]]),
                ),
                ("outcomes", serde_json::json!([1, 2, 3, 4, 5, 6])),
            ]),
        )
        .with_contains("ipw_att"),
        Example::new(
            "non-binary treatment",
            example_args(&[
                ("treatment", serde_json::json!([0, 2, 1])),
                ("covariates", serde_json::json!([[0], [1], [2]])),
                ("outcomes", serde_json::json!([1, 2, 3])),
            ]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_ipw(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(
        ctx,
        &inferential_modes(),
        "statistics.propensity_score_weighting",
    )?;
    let treatment = float_vector(args, "treatment")?;
    for (index, value) in treatment.iter().enumerate() {
        if *value != 0.0 && *value != 1.0 {
            return Err(
                EngineError::domain("treatment must contain only 0 and 1 values")
                    .with_path(format!("treatment[{index}]")),
            );
        }
    }
    let covariates = parse_covariates(args.require("covariates")?)?;
    let outcomes = float_vector(args, "outcomes")?;
    let n = treatment.len();
    if covariates.len() != n {
        return Err(EngineError::malformed(format!(
            "treatment has {n} units but covariates has {} rows",
            covariates.len()
        )));
    }
    if outcomes.len() != n {
        return Err(EngineError::malformed(format!(
            "treatment has {n} units but outcomes has {} entries",
            outcomes.len()
        )));
    }
    let mut design = covariates;
    for row in &mut design {
        row.insert(0, 1.0);
    }
    let columns = design.first().map_or(0, Vec::len);
    let elements = design.len().saturating_mul(columns);
    if elements > ctx.limits.max_matrix_elements {
        return Err(EngineError::resource(format!(
            "the propensity design has {elements} elements, exceeding the limit of {}",
            ctx.limits.max_matrix_elements
        )));
    }
    let method = parse_choice(args, "method", "hajek", &["hajek", "horvitz_thompson"])?;
    let propensities = logistic_probabilities(&treatment, &design, 1e-8, 100)?;
    let overlap_warning = validate_overlap(&propensities)?;
    let treated_count = treatment.iter().filter(|value| **value == 1.0).count();
    let control_count = n - treated_count;
    if treated_count < 2 || control_count < 2 {
        return Err(insufficient(
            "propensity_score_weighting requires at least two treated and two control units \
             to estimate the ATT and its standard error",
        ));
    }
    let n1 = treated_count as f64;
    let treated_outcomes: Vec<f64> = treatment
        .iter()
        .zip(outcomes.iter())
        .filter(|(value, _)| **value == 1.0)
        .map(|(_, outcome)| *outcome)
        .collect();
    let treated_mean = mean_f64(&treated_outcomes);
    let weights: Vec<f64> = propensities
        .iter()
        .map(|propensity| propensity / (1.0 - propensity))
        .collect();
    let mut control_weight_sum = 0.0f64;
    let mut control_weighted_outcome = 0.0f64;
    for index in 0..n {
        if treatment[index] == 0.0 {
            control_weight_sum += weights[index];
            control_weighted_outcome += weights[index] * outcomes[index];
        }
    }
    let (control_mean, control_variance) = if method == "hajek" {
        let control_mean = control_weighted_outcome / control_weight_sum;
        let mut sum = 0.0f64;
        for index in 0..n {
            if treatment[index] == 0.0 {
                let residual = weights[index] * (outcomes[index] - control_mean);
                sum += residual * residual;
            }
        }
        (
            control_mean,
            sum / (control_weight_sum * control_weight_sum),
        )
    } else {
        let control_mean = control_weighted_outcome / n1;
        let mut sum = 0.0f64;
        for index in 0..n {
            if treatment[index] == 0.0 {
                let residual = weights[index] * (outcomes[index] - control_mean);
                sum += residual * residual;
            }
        }
        (control_mean, sum / (n1 * n1))
    };
    let att = treated_mean - control_mean;
    let treated_variance = variance_f64(&treated_outcomes, 1)? / n1;
    let standard_error = sqrt(treated_variance + control_variance);

    let effective_weights: Vec<f64> = (0..n)
        .map(|index| {
            if treatment[index] == 1.0 {
                1.0
            } else {
                weights[index]
            }
        })
        .collect();
    let weight_min = effective_weights
        .iter()
        .copied()
        .fold(f64::INFINITY, f64::min);
    let weight_max = effective_weights
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    let weight_mean = mean_f64(&effective_weights);
    let weight_sum = compensated_sum(&effective_weights);
    let extreme_weights = effective_weights
        .iter()
        .filter(|weight| **weight > 10.0)
        .count();

    let propensity_min = propensities.iter().copied().fold(f64::INFINITY, f64::min);
    let propensity_max = propensities
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    let propensity_mean = mean_f64(&propensities);
    let treated_propensity_mean = mean_f64(
        &treatment
            .iter()
            .zip(propensities.iter())
            .filter(|(value, _)| **value == 1.0)
            .map(|(_, propensity)| *propensity)
            .collect::<Vec<_>>(),
    );
    let control_propensity_mean = mean_f64(
        &treatment
            .iter()
            .zip(propensities.iter())
            .filter(|(value, _)| **value == 0.0)
            .map(|(_, propensity)| *propensity)
            .collect::<Vec<_>>(),
    );

    let assumptions = [
        "treatment assignment is conditionally exchangeable given the observed covariates (no unmeasured confounding)",
        "the logistic propensity model is correctly specified",
        "positivity/overlap holds: every fitted propensity lies strictly inside (0, 1) (checked for the supplied data)",
        "the ATT is the average treatment effect among the treated, estimated by inverse-probability weighting of the control outcomes",
        "the standard error treats the fitted propensities as fixed and does not include propensity-model uncertainty",
        "outcomes are independent across units",
    ];
    let value = record(vec![
        ("att", float_value(att)?),
        ("standard_error", float_value(standard_error)?),
        (
            "weights_summary",
            record(vec![
                ("min", float_value(weight_min)?),
                ("max", float_value(weight_max)?),
                ("mean", float_value(weight_mean)?),
                ("sum", float_value(weight_sum)?),
            ]),
        ),
        (
            "propensity_summary",
            record(vec![
                ("min", float_value(propensity_min)?),
                ("max", float_value(propensity_max)?),
                ("mean", float_value(propensity_mean)?),
                ("treated_mean", float_value(treated_propensity_mean)?),
                ("control_mean", float_value(control_propensity_mean)?),
            ]),
        ),
        ("method", text("ipw_att")),
        ("estimator", text(method)),
        (
            "diagnostics",
            record(vec![
                ("overlap_min", float_value(propensity_min)?),
                ("overlap_max", float_value(propensity_max)?),
                ("extreme_weights", integer_value(extreme_weights as u64)),
            ]),
        ),
        ("assumptions", assumptions_value(&assumptions)),
    ]);
    let mut outcome = attach_assumptions(Outcome::approximate(value), "ipw_att", &assumptions);
    if let Some(warning) = overlap_warning {
        outcome = outcome.with_warning(warning);
    }
    Ok(outcome)
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

pub fn functions() -> Vec<Arc<dyn bicmath_core::contract::Function>> {
    vec![
        SimpleFunction::arc(did_descriptor(), invoke_did),
        SimpleFunction::arc(ipw_descriptor(), invoke_ipw),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
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

    fn field_record<'a>(
        fields: &'a BTreeMap<String, Value>,
        name: &str,
    ) -> &'a BTreeMap<String, Value> {
        match fields.get(name) {
            Some(Value::Record(record)) => record,
            other => panic!("expected record field {name}, got {other:?}"),
        }
    }

    fn close(actual: f64, expected: f64, tolerance: f64) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "expected {expected}, got {actual} (tolerance {tolerance})"
        );
    }

    fn sigmoid(value: f64) -> f64 {
        if value >= 0.0 {
            1.0 / (1.0 + (-value).exp())
        } else {
            let exponential = value.exp();
            exponential / (1.0 + exponential)
        }
    }

    /// Deterministic synthetic units: the treatment depends on the covariate
    /// through a logistic model and the outcome is 1 + 0.5 x + 2 T + noise, so
    /// the true ATT is exactly 2.
    fn synthetic_units() -> (Vec<f64>, Vec<Vec<f64>>, Vec<f64>) {
        let n = 120usize;
        let mut treatment = Vec::with_capacity(n);
        let mut covariates = Vec::with_capacity(n);
        let mut outcomes = Vec::with_capacity(n);
        for index in 0..n {
            let x = -2.0 + 4.0 * index as f64 / (n as f64 - 1.0);
            let propensity = sigmoid(0.8 * x + 0.2);
            let uniform = ((index as f64 + 1.0) * 0.618_033_988_749_894_9).fract();
            let assigned = if uniform < propensity { 1.0 } else { 0.0 };
            treatment.push(assigned);
            covariates.push(vec![x]);
            outcomes.push(1.0 + 0.5 * x + 2.0 * assigned + 0.3 * (index as f64).sin());
        }
        (treatment, covariates, outcomes)
    }

    #[test]
    fn did_recovers_a_known_effect() {
        // Provenance: Python 3.14 stdlib evaluation for the four samples
        // below: estimate 2, standard error sqrt(4 / 3) =
        // 1.1547005383792515, and 95% interval
        // [-0.2631714681523434, 4.263171468152343].
        let outcome = call(
            "statistics.difference_in_differences",
            serde_json::json!({
                "control_pre": [1, 2, 3],
                "control_post": [2, 3, 4],
                "treatment_pre": [1, 2, 3],
                "treatment_post": [4, 5, 6]
            }),
        )
        .unwrap();
        let fields = record_of(&outcome);
        close(field_f64(fields, "estimate"), 2.0, 1e-12);
        close(
            field_f64(fields, "standard_error"),
            1.154_700_538_379_251_5,
            1e-12,
        );
        let interval = field_record(fields, "confidence_interval");
        close(field_f64(interval, "lower"), -0.263_171_468_152_343_4, 1e-9);
        close(field_f64(interval, "upper"), 4.263_171_468_152_343, 1e-9);
        assert!(
            outcome
                .assumptions
                .iter()
                .any(|assumption| assumption.statement.contains("parallel trends")),
            "the result must carry the parallel-trends assumption"
        );
    }

    #[test]
    fn did_rejects_short_samples() {
        let error = call(
            "statistics.difference_in_differences",
            serde_json::json!({
                "control_pre": [1],
                "control_post": [2],
                "treatment_pre": [1],
                "treatment_post": [4]
            }),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::InsufficientObservations);
    }

    #[test]
    fn overlap_validation_rejects_and_warns() {
        let error = validate_overlap(&[0.5, 1.0]).unwrap_err();
        assert_eq!(error.code, ErrorCode::DomainViolation);
        assert!(error.message.contains("empty overlap"));
        let warning = validate_overlap(&[0.5, 1e-9]).unwrap();
        let warning = warning.expect("a near-boundary propensity must warn");
        assert_eq!(warning.code, "propensity_near_boundary");
        assert!(validate_overlap(&[0.2, 0.8]).unwrap().is_none());
    }

    #[test]
    fn ipw_att_recovers_a_known_effect() {
        // Provenance: Python 3.14 stdlib reproduction of the deterministic
        // synthetic dataset and the same IRLS iteration; the Hajek ATT is
        // 2.0090315198875226 against a true effect of 2, and the propensity
        // range is [0.18910524655880626, 0.8556417534252211].
        let (treatment, covariates, outcomes) = synthetic_units();
        let outcome = call(
            "statistics.propensity_score_weighting",
            serde_json::json!({
                "treatment": treatment,
                "covariates": covariates,
                "outcomes": outcomes
            }),
        )
        .unwrap();
        let fields = record_of(&outcome);
        let att = field_f64(fields, "att");
        assert!(
            (att - 2.0).abs() <= 0.1,
            "expected the ATT within 0.1 of the known effect 2, got {att}"
        );
        assert!(field_f64(fields, "standard_error") > 0.0);
        let propensity_summary = field_record(fields, "propensity_summary");
        close(
            field_f64(propensity_summary, "min"),
            0.189_105_246_558_806_26,
            1e-9,
        );
        close(
            field_f64(propensity_summary, "max"),
            0.855_641_753_425_221_1,
            1e-9,
        );
        let diagnostics = field_record(fields, "diagnostics");
        assert!(field_f64(diagnostics, "overlap_min") > 0.0);
        assert!(field_f64(diagnostics, "overlap_max") < 1.0);
        close(field_f64(diagnostics, "extreme_weights"), 0.0, 0.0);
        assert_eq!(
            match fields.get("method") {
                Some(Value::Text(text)) => text.as_str(),
                other => panic!("expected method text, got {other:?}"),
            },
            "ipw_att"
        );
    }

    #[test]
    fn ipw_rejects_bad_inputs() {
        let error = call(
            "statistics.propensity_score_weighting",
            serde_json::json!({
                "treatment": [0, 2, 1],
                "covariates": [[0], [1], [2]],
                "outcomes": [1, 2, 3]
            }),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::DomainViolation);
        let error = call(
            "statistics.propensity_score_weighting",
            serde_json::json!({
                "treatment": [0, 1, 1],
                "covariates": [[0], [1]],
                "outcomes": [1, 2, 3]
            }),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::MalformedInput);
    }
}
