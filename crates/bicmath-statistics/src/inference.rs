//! Statistical inference: confidence intervals, contingency tests, and
//! normal-approximation sample-size and power planning.
//!
//! Every method is named explicitly and every result carries its assumptions.
//! No equal-variance, normality, or paired-samples assumption is ever applied
//! silently.

use std::sync::Arc;

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, Assumption, CostClass, Example, FunctionDescriptor, Outcome, ParamDescriptor,
    SimpleFunction, Warning, require_mode,
};
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::schema::{FieldSchema, ValueSchema};
use bicmath_core::value::Value;
use num_bigint::BigInt;

use crate::common::*;
use crate::mathfn::{chi_square_sf, normal_cdf, normal_quantile, sqrt, student_t_quantile};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn numeric_record_schema(fields: Vec<(&str, ValueSchema)>) -> ValueSchema {
    record_schema(
        fields
            .into_iter()
            .map(|(name, schema)| FieldSchema::required(name, schema))
            .collect(),
        false,
    )
}

fn assumptions_value(statements: &[&str]) -> Value {
    array_value(
        statements
            .iter()
            .map(|statement| text(*statement))
            .collect(),
    )
}

fn normal_critical(confidence: f64) -> Result<f64, EngineError> {
    normal_quantile(1.0 - (1.0 - confidence) / 2.0, 0.0, 1.0)
}

fn t_critical(confidence: f64, df: f64) -> Result<f64, EngineError> {
    student_t_quantile(1.0 - (1.0 - confidence) / 2.0, df)
}

fn sample_mean_variance(values: &[f64]) -> Result<(f64, f64), EngineError> {
    if values.len() < 2 {
        return Err(insufficient(
            "the method requires at least two observations to estimate the variance",
        ));
    }
    Ok((mean_f64(values), variance_f64(values, 1)?))
}

fn wilson_interval(successes: f64, n: f64, z: f64) -> (f64, f64) {
    let p = successes / n;
    let denominator = 1.0 + z * z / n;
    let center = (p + z * z / (2.0 * n)) / denominator;
    let half = z * sqrt(p * (1.0 - p) / n + z * z / (4.0 * n * n)) / denominator;
    (
        (center - half).clamp(0.0, 1.0),
        (center + half).clamp(0.0, 1.0),
    )
}

fn wald_proportion_interval(p: f64, n: f64, z: f64) -> (f64, f64) {
    let standard_error = sqrt(p * (1.0 - p) / n);
    (
        (p - z * standard_error).clamp(0.0, 1.0),
        (p + z * standard_error).clamp(0.0, 1.0),
    )
}

fn validate_successes(
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

fn parse_count_table(args: &Args, name: &str) -> Result<(usize, usize, Vec<f64>), EngineError> {
    let value = args.require(name)?;
    let (rows, cols, data): (usize, usize, Vec<&Value>) = match value {
        Value::Matrix { rows, cols, data } => {
            (*rows as usize, *cols as usize, data.iter().collect())
        }
        Value::Array(items) => {
            if items.is_empty() {
                return Err(
                    EngineError::domain("the contingency table must not be empty")
                        .with_path(name.to_string()),
                );
            }
            let mut rows = Vec::new();
            let mut cols = None;
            for (index, row) in items.iter().enumerate() {
                let row = row.as_array().map_err(|e| {
                    EngineError::malformed(format!(
                        "row {index} of {name} must be an array: {}",
                        e.message
                    ))
                    .with_path(format!("{name}[{index}]"))
                })?;
                if row.is_empty() {
                    return Err(
                        EngineError::domain("the contingency table must not be empty")
                            .with_path(format!("{name}[{index}]")),
                    );
                }
                match cols {
                    None => cols = Some(row.len()),
                    Some(expected) if expected != row.len() => {
                        return Err(EngineError::malformed(format!(
                            "ragged contingency table: row {index} has {} entries, expected {expected}",
                            row.len()
                        ))
                        .with_path(format!("{name}[{index}]")));
                    }
                    Some(_) => {}
                }
                rows.push(row.iter().collect::<Vec<_>>());
            }
            let cols = cols.unwrap_or(0);
            (rows.len(), cols, rows.into_iter().flatten().collect())
        }
        other => {
            return Err(EngineError::malformed(format!(
                "expected a matrix or an array of equal-length arrays at {name}, found {}",
                other.kind_name()
            ))
            .with_path(name.to_string()));
        }
    };
    if rows < 2 || cols < 2 {
        return Err(EngineError::domain(
            "the contingency table must have at least two rows and two columns",
        )
        .with_path(name.to_string()));
    }
    let mut counts = Vec::with_capacity(data.len());
    for (index, cell) in data.iter().enumerate() {
        let number = cell.as_number().map_err(|_| {
            EngineError::malformed(format!("table cell {index} must be a number"))
                .with_path(format!("{name}[{index}]"))
        })?;
        let count = number_to_f64(number).map_err(|e| e.with_path(format!("{name}[{index}]")))?;
        if count < 0.0 {
            return Err(
                EngineError::domain("contingency table counts must be non-negative")
                    .with_path(format!("{name}[{index}]")),
            );
        }
        counts.push(count);
    }
    Ok((rows, cols, counts))
}

fn integer_from_f64(value: f64, name: &str) -> Result<BigInt, EngineError> {
    if !value.is_finite() || value < 0.0 {
        return Err(EngineError::domain(format!(
            "{name} could not be represented as a non-negative sample size"
        )));
    }
    let rounded = value.ceil();
    if rounded > u64::MAX as f64 {
        return Err(EngineError::new(
            ErrorCode::ResourceLimit,
            format!("{name} exceeds the supported sample-size range"),
        ));
    }
    Ok(BigInt::from(rounded as u64))
}

// ---------------------------------------------------------------------------
// ci_mean
// ---------------------------------------------------------------------------

fn ci_mean_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "statistics.ci_mean",
        "statistics",
        "1.0.0",
        "Confidence interval for a mean",
        "Confidence interval for the mean of one sample.",
    )
    .with_description(
        "method = \"t\" (default) uses the Student-t critical value with n - 1 degrees of \
         freedom; method = \"z\" uses the normal critical value with the sample standard \
         deviation (a large-sample approximation). At least two observations are required. \
         The output reports estimate, lower, upper, standard_error, method, df, and the \
         assumptions that were applied; no normality or independence assumption is silently \
         added.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("values", "Observations.", array_schema(any_number_schema())),
        ParamDescriptor::optional(
            "confidence",
            "Confidence level in (0, 1); default 0.95.",
            any_number_schema(),
        ),
        ParamDescriptor::optional(
            "method",
            "Critical value method: t (default) or z.",
            ValueSchema::Enum {
                variants: vec!["t".to_string(), "z".to_string()],
            },
        ),
    ])
    .with_output(
        numeric_record_schema(vec![
            ("estimate", any_number_schema()),
            ("lower", any_number_schema()),
            ("upper", any_number_schema()),
            ("standard_error", any_number_schema()),
            ("method", text_schema()),
            ("df", ValueSchema::Any),
            ("assumptions", array_schema(text_schema())),
        ]),
        "Confidence interval record.",
    )
    .with_modes(inferential_modes())
    .with_cost(CostClass::Linear)
    .with_method_ref("docs/methods/statistics.md#ci_mean")
    .with_examples(vec![
        Example::new(
            "single observation is not enough",
            example_args(&[("values", serde_json::json!([1]))]),
        )
        .with_error(ErrorCode::InsufficientObservations),
    ])
}

fn invoke_ci_mean(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.ci_mean")?;
    let confidence = confidence_param(args)?;
    let method = parse_choice(args, "method", "t", &["t", "z"])?;
    let values = classify_series(args, "values", ctx)?.to_f64_vec()?;
    let (estimate, variance) = sample_mean_variance(&values)?;
    let n = values.len() as f64;
    let standard_error = sqrt(variance / n);
    let (critical, df_value, method_assumptions): (f64, Value, &[&str]) = if method == "t" {
        (
            t_critical(confidence, n - 1.0)?,
            float_value(n - 1.0)?,
            &[
                "observations are independent and identically distributed",
                "the sample is representative of the target population",
                "the population is approximately normal, or the sample is large enough for the t interval",
            ],
        )
    } else {
        (
            normal_critical(confidence)?,
            Value::Null,
            &[
                "observations are independent and identically distributed",
                "the sample is representative of the target population",
                "the population standard deviation is known; the sample standard deviation is used as a large-sample approximation",
            ],
        )
    };
    let lower = estimate - critical * standard_error;
    let upper = estimate + critical * standard_error;
    let value = record(vec![
        ("estimate", float_value(estimate)?),
        ("lower", float_value(lower)?),
        ("upper", float_value(upper)?),
        ("standard_error", float_value(standard_error)?),
        ("method", text(method)),
        ("df", df_value),
        ("assumptions", assumptions_value(method_assumptions)),
    ]);
    let mut outcome = Outcome::approximate(value);
    for (index, statement) in method_assumptions.iter().enumerate() {
        outcome = outcome.with_assumption(Assumption::unverified(
            format!("ci_mean_{index}"),
            *statement,
        ));
    }
    Ok(outcome)
}

// ---------------------------------------------------------------------------
// ci_proportion
// ---------------------------------------------------------------------------

fn ci_proportion_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "statistics.ci_proportion",
        "statistics",
        "1.0.0",
        "Confidence interval for a proportion",
        "Confidence interval for a binomial proportion.",
    )
    .with_description(
        "method = \"wilson\" (default) uses the Wilson score interval; method = \"wald\" uses \
         the normal approximation p_hat +/- z * sqrt(p_hat (1 - p_hat) / n) with endpoints \
         clipped to [0, 1]. Wald intervals emit a warning when n * p_hat < 5 or \
         n * (1 - p_hat) < 5. successes must be an integer in 0..=n and n must be at least 1.",
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
            "Interval method: wilson (default) or wald.",
            ValueSchema::Enum {
                variants: vec!["wilson".to_string(), "wald".to_string()],
            },
        ),
    ])
    .with_output(
        numeric_record_schema(vec![
            ("estimate", any_number_schema()),
            ("lower", any_number_schema()),
            ("upper", any_number_schema()),
            ("method", text_schema()),
            ("assumptions", array_schema(text_schema())),
        ]),
        "Proportion confidence interval record.",
    )
    .with_modes(inferential_modes())
    .with_cost(CostClass::Constant)
    .with_method_ref("docs/methods/statistics.md#ci_proportion")
    .with_examples(vec![
        Example::new(
            "zero trials",
            example_args(&[
                ("successes", serde_json::json!(0)),
                ("n", serde_json::json!(0)),
            ]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_ci_proportion(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.ci_proportion")?;
    let (successes, n) = validate_successes(&args.integer("successes")?, &args.integer("n")?, "")?;
    let confidence = confidence_param(args)?;
    let method = parse_choice(args, "method", "wilson", &["wilson", "wald"])?;
    let z = normal_critical(confidence)?;
    let successes_f = successes as f64;
    let n_f = n as f64;
    let p = successes_f / n_f;
    let (lower, upper, assumptions, warning) = if method == "wilson" {
        let (lower, upper) = wilson_interval(successes_f, n_f, z);
        (
            lower,
            upper,
            vec![
                "trials are independent Bernoulli draws",
                "the sample is representative of the target population",
            ],
            None,
        )
    } else {
        let (lower, upper) = wald_proportion_interval(p, n_f, z);
        let small = n_f * p < 5.0 || n_f * (1.0 - p) < 5.0;
        let warning = small.then(|| {
            Warning::new(
                "wald_small_sample",
                "the Wald interval is unreliable when n * p_hat < 5 or n * (1 - p_hat) < 5",
            )
        });
        (
            lower,
            upper,
            vec![
                "trials are independent Bernoulli draws",
                "the sample is representative of the target population",
                "the normal approximation to the binomial is adequate",
            ],
            warning,
        )
    };
    let value = record(vec![
        ("estimate", float_value(p)?),
        ("lower", float_value(lower)?),
        ("upper", float_value(upper)?),
        ("method", text(method)),
        ("assumptions", assumptions_value(&assumptions)),
    ]);
    let mut outcome = Outcome::approximate(value);
    if let Some(warning) = warning {
        outcome = outcome.with_warning(warning);
    }
    Ok(outcome)
}

// ---------------------------------------------------------------------------
// welch_ci
// ---------------------------------------------------------------------------

fn welch_ci_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "statistics.welch_ci",
        "statistics",
        "1.0.0",
        "Welch confidence interval for a mean difference",
        "Welch interval for mean_a - mean_b without an equal-variance assumption.",
    )
    .with_description(
        "Returns the difference of means with the Welch-Satterthwaite degrees of freedom and \
         a Student-t interval. The equal-variance assumption of the pooled two-sample interval \
         is deliberately not applied. Each sample must contain at least two observations and \
         the standard error must be positive (two constant samples have an undefined interval).",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "sample_a",
            "First sample.",
            array_schema(any_number_schema()),
        ),
        ParamDescriptor::required(
            "sample_b",
            "Second sample.",
            array_schema(any_number_schema()),
        ),
        ParamDescriptor::optional(
            "confidence",
            "Confidence level in (0, 1); default 0.95.",
            any_number_schema(),
        ),
    ])
    .with_output(
        numeric_record_schema(vec![
            ("estimate", any_number_schema()),
            ("lower", any_number_schema()),
            ("upper", any_number_schema()),
            ("standard_error", any_number_schema()),
            ("df", any_number_schema()),
            ("method", text_schema()),
            ("assumptions", array_schema(text_schema())),
        ]),
        "Welch confidence interval record.",
    )
    .with_modes(inferential_modes())
    .with_cost(CostClass::Linear)
    .with_method_ref("docs/methods/statistics.md#welch_ci")
    .with_examples(vec![
        Example::new(
            "one observation per sample",
            example_args(&[
                ("sample_a", serde_json::json!([1])),
                ("sample_b", serde_json::json!([2])),
            ]),
        )
        .with_error(ErrorCode::InsufficientObservations),
    ])
}

fn invoke_welch_ci(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.welch_ci")?;
    let confidence = confidence_param(args)?;
    let a = classify_series(args, "sample_a", ctx)?.to_f64_vec()?;
    let b = classify_series(args, "sample_b", ctx)?.to_f64_vec()?;
    let (mean_a, variance_a) = sample_mean_variance(&a)?;
    let (mean_b, variance_b) = sample_mean_variance(&b)?;
    let n_a = a.len() as f64;
    let n_b = b.len() as f64;
    let component_a = variance_a / n_a;
    let component_b = variance_b / n_b;
    let standard_error = sqrt(component_a + component_b);
    if standard_error <= 0.0 {
        return Err(EngineError::domain(
            "Welch interval is undefined when both samples are constant (zero standard error)",
        ));
    }
    let df = (component_a + component_b).powi(2)
        / (component_a.powi(2) / (n_a - 1.0) + component_b.powi(2) / (n_b - 1.0));
    if !df.is_finite() || df <= 0.0 {
        return Err(EngineError::domain(
            "Welch-Satterthwaite degrees of freedom are undefined",
        ));
    }
    let critical = t_critical(confidence, df)?;
    let estimate = mean_a - mean_b;
    let assumptions = [
        "the two samples are independent",
        "each sample is representative of its population",
        "the sampling distribution of the difference is approximately Student-t (normal populations or large samples)",
    ];
    let value = record(vec![
        ("estimate", float_value(estimate)?),
        ("lower", float_value(estimate - critical * standard_error)?),
        ("upper", float_value(estimate + critical * standard_error)?),
        ("standard_error", float_value(standard_error)?),
        ("df", float_value(df)?),
        ("method", text("welch")),
        ("assumptions", assumptions_value(&assumptions)),
    ]);
    let mut outcome = Outcome::approximate(value);
    for (index, statement) in assumptions.iter().enumerate() {
        outcome = outcome.with_assumption(Assumption::unverified(
            format!("welch_ci_{index}"),
            *statement,
        ));
    }
    Ok(outcome)
}

// ---------------------------------------------------------------------------
// proportions_difference
// ---------------------------------------------------------------------------

fn proportions_difference_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "statistics.proportions_difference",
        "statistics",
        "1.0.0",
        "Confidence interval for a difference of proportions",
        "Newcombe or Wald interval for p_a - p_b.",
    )
    .with_description(
        "method = \"newcombe\" (default) combines the two Wilson score intervals with the \
         square-and-add hybrid; method = \"wald\" uses the normal approximation. Each group \
         needs n >= 1 and 0 <= successes <= n. The Wald interval emits a small-sample warning \
         when any expected count is below 5.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "successes_a",
            "Successes in group A; integer in 0..=n_a.",
            integer_schema(),
        ),
        ParamDescriptor::required("n_a", "Trials in group A; integer >= 1.", integer_schema()),
        ParamDescriptor::required(
            "successes_b",
            "Successes in group B; integer in 0..=n_b.",
            integer_schema(),
        ),
        ParamDescriptor::required("n_b", "Trials in group B; integer >= 1.", integer_schema()),
        ParamDescriptor::optional(
            "confidence",
            "Confidence level in (0, 1); default 0.95.",
            any_number_schema(),
        ),
        ParamDescriptor::optional(
            "method",
            "Interval method: newcombe (default) or wald.",
            ValueSchema::Enum {
                variants: vec!["newcombe".to_string(), "wald".to_string()],
            },
        ),
    ])
    .with_output(
        numeric_record_schema(vec![
            ("difference", any_number_schema()),
            ("lower", any_number_schema()),
            ("upper", any_number_schema()),
            ("method", text_schema()),
            ("assumptions", array_schema(text_schema())),
        ]),
        "Difference of proportions confidence interval record.",
    )
    .with_modes(inferential_modes())
    .with_cost(CostClass::Constant)
    .with_method_ref("docs/methods/statistics.md#proportions_difference")
    .with_examples(vec![
        Example::new(
            "successes exceed trials",
            example_args(&[
                ("successes_a", serde_json::json!(5)),
                ("n_a", serde_json::json!(2)),
                ("successes_b", serde_json::json!(1)),
                ("n_b", serde_json::json!(2)),
            ]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_proportions_difference(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(
        ctx,
        &inferential_modes(),
        "statistics.proportions_difference",
    )?;
    let (successes_a, n_a) =
        validate_successes(&args.integer("successes_a")?, &args.integer("n_a")?, "a_")?;
    let (successes_b, n_b) =
        validate_successes(&args.integer("successes_b")?, &args.integer("n_b")?, "b_")?;
    let confidence = confidence_param(args)?;
    let method = parse_choice(args, "method", "newcombe", &["newcombe", "wald"])?;
    let z = normal_critical(confidence)?;
    let p_a = successes_a as f64 / n_a as f64;
    let p_b = successes_b as f64 / n_b as f64;
    let difference = p_a - p_b;
    let (lower, upper, assumptions, warning) = if method == "newcombe" {
        let (lower_a, upper_a) = wilson_interval(successes_a as f64, n_a as f64, z);
        let (lower_b, upper_b) = wilson_interval(successes_b as f64, n_b as f64, z);
        let lower = difference - sqrt((p_a - lower_a).powi(2) + (upper_b - p_b).powi(2));
        let upper = difference + sqrt((upper_a - p_a).powi(2) + (p_b - lower_b).powi(2));
        (
            lower,
            upper,
            vec![
                "the two groups are independent",
                "each group is a representative Bernoulli sample",
                "the Newcombe square-and-add hybrid combines two Wilson score intervals",
            ],
            None,
        )
    } else {
        let standard_error = sqrt(p_a * (1.0 - p_a) / n_a as f64 + p_b * (1.0 - p_b) / n_b as f64);
        let small = n_a as f64 * p_a < 5.0
            || n_a as f64 * (1.0 - p_a) < 5.0
            || n_b as f64 * p_b < 5.0
            || n_b as f64 * (1.0 - p_b) < 5.0;
        let warning = small.then(|| {
            Warning::new(
                "wald_small_sample",
                "the Wald interval is unreliable when any expected count is below 5",
            )
        });
        (
            difference - z * standard_error,
            difference + z * standard_error,
            vec![
                "the two groups are independent",
                "each group is a representative Bernoulli sample",
                "the normal approximation to the binomial is adequate",
            ],
            warning,
        )
    };
    let value = record(vec![
        ("difference", float_value(difference)?),
        ("lower", float_value(lower)?),
        ("upper", float_value(upper)?),
        ("method", text(method)),
        ("assumptions", assumptions_value(&assumptions)),
    ]);
    let mut outcome = Outcome::approximate(value);
    if let Some(warning) = warning {
        outcome = outcome.with_warning(warning);
    }
    Ok(outcome)
}

// ---------------------------------------------------------------------------
// chi_square_contingency
// ---------------------------------------------------------------------------

fn chi_square_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "statistics.chi_square_contingency",
        "statistics",
        "1.0.0",
        "Pearson chi-square test of independence",
        "Pearson chi-square test for a two-way contingency table.",
    )
    .with_description(
        "Accepts a matrix or an array of equal-length arrays of non-negative counts. Returns \
         the Pearson statistic, degrees of freedom (r - 1)(c - 1), p-value, the expected \
         count matrix, and diagnostics (minimum expected count, number of cells below 5, and \
         a warning when expected counts are small). Ragged or empty tables and tables with a \
         zero margin are rejected.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "table",
        "Two-way table of non-negative counts: a matrix or an array of equal-length arrays.",
        ValueSchema::Any,
    )])
    .with_output(
        numeric_record_schema(vec![
            ("statistic", any_number_schema()),
            ("df", any_number_schema()),
            ("p_value", any_number_schema()),
            ("expected", ValueSchema::Any),
            ("method", text_schema()),
            ("diagnostics", ValueSchema::Any),
        ]),
        "Chi-square test record with the expected-count matrix and diagnostics.",
    )
    .with_modes(inferential_modes())
    .with_cost(CostClass::Quadratic)
    .with_method_ref("docs/methods/statistics.md#chi_square_contingency")
    .with_examples(vec![
        Example::new(
            "zero margins",
            example_args(&[("table", serde_json::json!([[0, 0], [0, 0]]))]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_chi_square_contingency(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(
        ctx,
        &inferential_modes(),
        "statistics.chi_square_contingency",
    )?;
    let (rows, cols, counts) = parse_count_table(args, "table")?;
    let total: f64 = counts.iter().sum();
    if total <= 0.0 {
        return Err(EngineError::domain(
            "the contingency table has a zero grand total",
        ));
    }
    let mut row_totals = vec![0.0f64; rows];
    let mut col_totals = vec![0.0f64; cols];
    for row in 0..rows {
        for col in 0..cols {
            let value = counts[row * cols + col];
            row_totals[row] += value;
            col_totals[col] += value;
        }
    }
    if row_totals.contains(&0.0) {
        return Err(EngineError::domain(
            "the contingency table has a zero row margin",
        ));
    }
    if col_totals.contains(&0.0) {
        return Err(EngineError::domain(
            "the contingency table has a zero column margin",
        ));
    }
    let mut statistic = 0.0f64;
    let mut expected = Vec::with_capacity(rows * cols);
    let mut min_expected = f64::INFINITY;
    let mut cells_below_5 = 0u64;
    for row in 0..rows {
        for col in 0..cols {
            let expected_value = row_totals[row] * col_totals[col] / total;
            let observed = counts[row * cols + col];
            statistic += (observed - expected_value).powi(2) / expected_value;
            if expected_value < min_expected {
                min_expected = expected_value;
            }
            if expected_value < 5.0 {
                cells_below_5 += 1;
            }
            expected.push(float_value(expected_value)?);
        }
    }
    let df = ((rows - 1) * (cols - 1)) as f64;
    let p_value = chi_square_sf(statistic, df)?;
    let expected_value = Value::Matrix {
        rows: rows as u32,
        cols: cols as u32,
        data: expected,
    };
    let diagnostics = record(vec![
        ("min_expected", float_value(min_expected)?),
        ("cells_below_5", integer_value(cells_below_5)),
        (
            "warning",
            if cells_below_5 > 0 {
                text(
                    "one or more expected counts are below 5; the chi-square approximation may be inaccurate",
                )
            } else {
                Value::Null
            },
        ),
    ]);
    let value = record(vec![
        ("statistic", float_value(statistic)?),
        ("df", float_value(df)?),
        ("p_value", float_value(p_value)?),
        ("expected", expected_value),
        ("method", text("pearson_chi_square")),
        ("diagnostics", diagnostics),
    ]);
    let mut outcome = Outcome::approximate(value);
    if cells_below_5 > 0 {
        outcome = outcome.with_warning(Warning::new(
            "chi_square_small_expected",
            "one or more expected counts are below 5; the chi-square approximation may be inaccurate",
        ));
    }
    Ok(outcome)
}

// ---------------------------------------------------------------------------
// Sample size and power (normal approximation)
// ---------------------------------------------------------------------------

fn planning_assumption_text() -> &'static str {
    "a power or sample-size figure is a probability under the stated planning assumptions, not a guarantee of the observed result"
}

fn sample_size_means_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "statistics.sample_size_two_means",
        "statistics",
        "1.0.0",
        "Sample size for two means",
        "Per-group sample size for a two-sample mean comparison (normal approximation).",
    )
    .with_description(
        "Uses the normal approximation: n1 = (z_(1-alpha') + z_(1-power))^2 * (1 + 1/r) / d^2 \
         and n2 = ceil(r * n1), where d is the standardized effect size, r is \
         allocation_ratio = n2 / n1, and alpha' is alpha / 2 for a two-sided test or alpha for \
         a one-sided test. Sizes are rounded up. A power figure is a probability under the \
         planning assumptions, not a guarantee.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "effect_size",
            "Standardized effect size d = (mean1 - mean2) / sd; must be > 0.",
            any_number_schema(),
        ),
        ParamDescriptor::optional(
            "alpha",
            "Type I error rate; default 0.05.",
            any_number_schema(),
        ),
        ParamDescriptor::optional("power", "Target power; default 0.8.", any_number_schema()),
        ParamDescriptor::optional(
            "allocation_ratio",
            "n2 / n1; default 1.",
            any_number_schema(),
        ),
        ParamDescriptor::optional(
            "sided",
            "two (default) or one.",
            ValueSchema::Enum {
                variants: vec!["two".to_string(), "one".to_string()],
            },
        ),
    ])
    .with_output(
        numeric_record_schema(vec![
            ("n1", integer_schema()),
            ("n2", integer_schema()),
            ("total", integer_schema()),
            ("allocation_ratio", any_number_schema()),
            ("effect_size", any_number_schema()),
            ("alpha", any_number_schema()),
            ("power", any_number_schema()),
            ("sided", text_schema()),
            ("method", text_schema()),
            ("assumptions", array_schema(text_schema())),
        ]),
        "Sample-size plan record.",
    )
    .with_modes(inferential_modes())
    .with_cost(CostClass::Constant)
    .with_method_ref("docs/methods/statistics.md#sample_size_two_means")
    .with_examples(vec![
        Example::new(
            "zero effect size",
            example_args(&[("effect_size", serde_json::json!(0))]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_sample_size_two_means(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(
        ctx,
        &inferential_modes(),
        "statistics.sample_size_two_means",
    )?;
    let effect_size = scalar_f64(args, "effect_size")?;
    if effect_size <= 0.0 {
        return Err(EngineError::domain("effect_size must be strictly positive")
            .with_path("effect_size".to_string()));
    }
    let alpha = alpha_param(args)?;
    let power = power_param(args)?;
    let ratio = allocation_ratio_param(args)?;
    let sided = sided_param(args)?;
    let z_alpha = if sided == "two" {
        normal_quantile(1.0 - alpha / 2.0, 0.0, 1.0)?
    } else {
        normal_quantile(1.0 - alpha, 0.0, 1.0)?
    };
    let z_power = normal_quantile(power, 0.0, 1.0)?;
    let n1_float = (z_alpha + z_power).powi(2) * (1.0 + 1.0 / ratio) / (effect_size * effect_size);
    let n1 = integer_from_f64(n1_float, "n1")?;
    let n2 = integer_from_f64(ratio * n1_float, "n2")?;
    let total = &n1 + &n2;
    let assumptions = [
        "the comparison uses the normal approximation to the sampling distribution",
        "the effect size is a planning value supplied by the caller",
        planning_assumption_text(),
    ];
    let value = record(vec![
        (
            "n1",
            Value::Number(bicmath_core::number::Number::Integer(n1)),
        ),
        (
            "n2",
            Value::Number(bicmath_core::number::Number::Integer(n2)),
        ),
        (
            "total",
            Value::Number(bicmath_core::number::Number::Integer(total)),
        ),
        ("allocation_ratio", float_value(ratio)?),
        ("effect_size", float_value(effect_size)?),
        ("alpha", float_value(alpha)?),
        ("power", float_value(power)?),
        ("sided", text(sided)),
        ("method", text("normal_approximation")),
        ("assumptions", assumptions_value(&assumptions)),
    ]);
    let mut outcome = Outcome::approximate(value);
    for (index, statement) in assumptions.iter().enumerate() {
        outcome = outcome.with_assumption(Assumption::unverified(
            format!("sample_size_two_means_{index}"),
            *statement,
        ));
    }
    Ok(outcome)
}

fn sample_size_proportions_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "statistics.sample_size_two_proportions",
        "statistics",
        "1.0.0",
        "Sample size for two proportions",
        "Per-group sample size for a two-proportion comparison (normal approximation).",
    )
    .with_description(
        "Uses the normal approximation with pooled variance under the null and unpooled \
         variance under the alternative: n1 = (z_(1-alpha') * sqrt((1 + 1/r) * pbar * \
         (1 - pbar)) + z_(1-power) * sqrt(p1 (1 - p1) + p2 (1 - p2) / r))^2 / (p1 - p2)^2, \
         where pbar = (p1 + r * p2) / (1 + r) and r = allocation_ratio = n2 / n1. p1 and p2 \
         must differ. A power figure is a probability under the planning assumptions, not a \
         guarantee.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "p1",
            "Proportion in group 1, in [0, 1].",
            any_number_schema(),
        ),
        ParamDescriptor::required(
            "p2",
            "Proportion in group 2, in [0, 1].",
            any_number_schema(),
        ),
        ParamDescriptor::optional(
            "alpha",
            "Type I error rate; default 0.05.",
            any_number_schema(),
        ),
        ParamDescriptor::optional("power", "Target power; default 0.8.", any_number_schema()),
        ParamDescriptor::optional(
            "allocation_ratio",
            "n2 / n1; default 1.",
            any_number_schema(),
        ),
        ParamDescriptor::optional(
            "sided",
            "two (default) or one.",
            ValueSchema::Enum {
                variants: vec!["two".to_string(), "one".to_string()],
            },
        ),
    ])
    .with_output(
        numeric_record_schema(vec![
            ("n1", integer_schema()),
            ("n2", integer_schema()),
            ("total", integer_schema()),
            ("allocation_ratio", any_number_schema()),
            ("p1", any_number_schema()),
            ("p2", any_number_schema()),
            ("alpha", any_number_schema()),
            ("power", any_number_schema()),
            ("sided", text_schema()),
            ("method", text_schema()),
            ("assumptions", array_schema(text_schema())),
        ]),
        "Sample-size plan record.",
    )
    .with_modes(inferential_modes())
    .with_cost(CostClass::Constant)
    .with_method_ref("docs/methods/statistics.md#sample_size_two_proportions")
    .with_examples(vec![
        Example::new(
            "equal proportions",
            example_args(&[
                ("p1", serde_json::json!(0.5)),
                ("p2", serde_json::json!(0.5)),
            ]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_sample_size_two_proportions(
    args: &Args,
    ctx: &ExecContext,
) -> Result<Outcome, EngineError> {
    require_mode(
        ctx,
        &inferential_modes(),
        "statistics.sample_size_two_proportions",
    )?;
    let p1 = probability_param(args, "p1", 0.5)?;
    let p2 = probability_param(args, "p2", 0.5)?;
    if (p1 - p2).abs() == 0.0 {
        return Err(EngineError::domain(
            "sample_size_two_proportions requires p1 != p2",
        ));
    }
    let alpha = alpha_param(args)?;
    let power = power_param(args)?;
    let ratio = allocation_ratio_param(args)?;
    let sided = sided_param(args)?;
    let z_alpha = if sided == "two" {
        normal_quantile(1.0 - alpha / 2.0, 0.0, 1.0)?
    } else {
        normal_quantile(1.0 - alpha, 0.0, 1.0)?
    };
    let z_power = normal_quantile(power, 0.0, 1.0)?;
    let pbar = (p1 + ratio * p2) / (1.0 + ratio);
    let pooled = z_alpha * sqrt((1.0 + 1.0 / ratio) * pbar * (1.0 - pbar));
    let unpooled = z_power * sqrt(p1 * (1.0 - p1) + p2 * (1.0 - p2) / ratio);
    let n1_float = (pooled + unpooled).powi(2) / (p1 - p2).powi(2);
    let n1 = integer_from_f64(n1_float, "n1")?;
    let n2 = integer_from_f64(ratio * n1_float, "n2")?;
    let total = &n1 + &n2;
    let assumptions = [
        "the comparison uses the normal approximation to the binomial",
        "p1 and p2 are planning values supplied by the caller",
        planning_assumption_text(),
    ];
    let value = record(vec![
        (
            "n1",
            Value::Number(bicmath_core::number::Number::Integer(n1)),
        ),
        (
            "n2",
            Value::Number(bicmath_core::number::Number::Integer(n2)),
        ),
        (
            "total",
            Value::Number(bicmath_core::number::Number::Integer(total)),
        ),
        ("allocation_ratio", float_value(ratio)?),
        ("p1", float_value(p1)?),
        ("p2", float_value(p2)?),
        ("alpha", float_value(alpha)?),
        ("power", float_value(power)?),
        ("sided", text(sided)),
        ("method", text("normal_approximation")),
        ("assumptions", assumptions_value(&assumptions)),
    ]);
    let mut outcome = Outcome::approximate(value);
    for (index, statement) in assumptions.iter().enumerate() {
        outcome = outcome.with_assumption(Assumption::unverified(
            format!("sample_size_two_proportions_{index}"),
            *statement,
        ));
    }
    Ok(outcome)
}

fn power_means_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "statistics.power_two_means",
        "statistics",
        "1.0.0",
        "Power for two means",
        "Achieved power for a two-sample mean comparison (normal approximation).",
    )
    .with_description(
        "Computes power = Phi(d * sqrt(n / 2) - z_(1-alpha')) for equal groups, where d is the \
         standardized effect size and alpha' is alpha / 2 for a two-sided test or alpha for a \
         one-sided test. The result is a probability under the planning assumptions, not a \
         guarantee.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "n_per_group",
            "Observations per group; integer >= 2.",
            integer_schema(),
        ),
        ParamDescriptor::required(
            "effect_size",
            "Standardized effect size d >= 0.",
            any_number_schema(),
        ),
        ParamDescriptor::optional(
            "alpha",
            "Type I error rate; default 0.05.",
            any_number_schema(),
        ),
        ParamDescriptor::optional(
            "sided",
            "two (default) or one.",
            ValueSchema::Enum {
                variants: vec!["two".to_string(), "one".to_string()],
            },
        ),
    ])
    .with_output(
        numeric_record_schema(vec![
            ("power", any_number_schema()),
            ("n_per_group", integer_schema()),
            ("effect_size", any_number_schema()),
            ("alpha", any_number_schema()),
            ("sided", text_schema()),
            ("method", text_schema()),
            ("assumptions", array_schema(text_schema())),
        ]),
        "Achieved power record.",
    )
    .with_modes(inferential_modes())
    .with_cost(CostClass::Constant)
    .with_method_ref("docs/methods/statistics.md#power_two_means")
    .with_examples(vec![
        Example::new(
            "one observation per group",
            example_args(&[
                ("n_per_group", serde_json::json!(1)),
                ("effect_size", serde_json::json!(0.5)),
            ]),
        )
        .with_error(ErrorCode::InsufficientObservations),
    ])
}

fn invoke_power_two_means(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.power_two_means")?;
    let n = args.usize_param("n_per_group")?;
    if n < 2 {
        return Err(insufficient(
            "power_two_means requires at least two observations per group",
        ));
    }
    let effect_size = scalar_f64(args, "effect_size")?;
    if effect_size < 0.0 {
        return Err(EngineError::domain("effect_size must be non-negative")
            .with_path("effect_size".to_string()));
    }
    let alpha = alpha_param(args)?;
    let sided = sided_param(args)?;
    let z_alpha = if sided == "two" {
        normal_quantile(1.0 - alpha / 2.0, 0.0, 1.0)?
    } else {
        normal_quantile(1.0 - alpha, 0.0, 1.0)?
    };
    let z_beta = effect_size * sqrt(n as f64 / 2.0) - z_alpha;
    let achieved = normal_cdf(z_beta, 0.0, 1.0)?;
    let assumptions = [
        "the comparison uses the normal approximation to the sampling distribution",
        "equal group sizes and a known planning effect size",
        planning_assumption_text(),
    ];
    let value = record(vec![
        ("power", float_value(achieved)?),
        ("n_per_group", integer_value(n as u64)),
        ("effect_size", float_value(effect_size)?),
        ("alpha", float_value(alpha)?),
        ("sided", text(sided)),
        ("method", text("normal_approximation")),
        ("assumptions", assumptions_value(&assumptions)),
    ]);
    let mut outcome = Outcome::approximate(value);
    for (index, statement) in assumptions.iter().enumerate() {
        outcome = outcome.with_assumption(Assumption::unverified(
            format!("power_two_means_{index}"),
            *statement,
        ));
    }
    Ok(outcome)
}

fn power_proportions_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "statistics.power_two_proportions",
        "statistics",
        "1.0.0",
        "Power for two proportions",
        "Achieved power for a two-proportion comparison (normal approximation).",
    )
    .with_description(
        "Computes power = Phi((|p1 - p2| * sqrt(n) - z_(1-alpha') * sqrt(2 * pbar * \
         (1 - pbar))) / sqrt(p1 (1 - p1) + p2 (1 - p2))) for equal groups, where pbar = \
         (p1 + p2) / 2. The result is a probability under the planning assumptions, not a \
         guarantee.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "n_per_group",
            "Observations per group; integer >= 1.",
            integer_schema(),
        ),
        ParamDescriptor::required(
            "p1",
            "Proportion in group 1, in [0, 1].",
            any_number_schema(),
        ),
        ParamDescriptor::required(
            "p2",
            "Proportion in group 2, in [0, 1].",
            any_number_schema(),
        ),
        ParamDescriptor::optional(
            "alpha",
            "Type I error rate; default 0.05.",
            any_number_schema(),
        ),
        ParamDescriptor::optional(
            "sided",
            "two (default) or one.",
            ValueSchema::Enum {
                variants: vec!["two".to_string(), "one".to_string()],
            },
        ),
    ])
    .with_output(
        numeric_record_schema(vec![
            ("power", any_number_schema()),
            ("n_per_group", integer_schema()),
            ("p1", any_number_schema()),
            ("p2", any_number_schema()),
            ("alpha", any_number_schema()),
            ("sided", text_schema()),
            ("method", text_schema()),
            ("assumptions", array_schema(text_schema())),
        ]),
        "Achieved power record.",
    )
    .with_modes(inferential_modes())
    .with_cost(CostClass::Constant)
    .with_method_ref("docs/methods/statistics.md#power_two_proportions")
    .with_examples(vec![
        Example::new(
            "zero observations",
            example_args(&[
                ("n_per_group", serde_json::json!(0)),
                ("p1", serde_json::json!(0.4)),
                ("p2", serde_json::json!(0.5)),
            ]),
        )
        .with_error(ErrorCode::InsufficientObservations),
    ])
}

fn invoke_power_two_proportions(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(
        ctx,
        &inferential_modes(),
        "statistics.power_two_proportions",
    )?;
    let n = args.usize_param("n_per_group")?;
    if n == 0 {
        return Err(insufficient(
            "power_two_proportions requires at least one observation per group",
        ));
    }
    let p1 = probability_param(args, "p1", 0.5)?;
    let p2 = probability_param(args, "p2", 0.5)?;
    let alpha = alpha_param(args)?;
    let sided = sided_param(args)?;
    let z_alpha = if sided == "two" {
        normal_quantile(1.0 - alpha / 2.0, 0.0, 1.0)?
    } else {
        normal_quantile(1.0 - alpha, 0.0, 1.0)?
    };
    let pbar = (p1 + p2) / 2.0;
    let unpooled = sqrt(p1 * (1.0 - p1) + p2 * (1.0 - p2));
    let z_beta = if (p1 - p2).abs() == 0.0 {
        -z_alpha
    } else if unpooled == 0.0 {
        f64::INFINITY
    } else {
        ((p1 - p2).abs() * sqrt(n as f64) - z_alpha * sqrt(2.0 * pbar * (1.0 - pbar))) / unpooled
    };
    let achieved = normal_cdf(z_beta, 0.0, 1.0)?;
    let assumptions = [
        "the comparison uses the normal approximation to the binomial",
        "equal group sizes and known planning proportions",
        planning_assumption_text(),
    ];
    let value = record(vec![
        ("power", float_value(achieved)?),
        ("n_per_group", integer_value(n as u64)),
        ("p1", float_value(p1)?),
        ("p2", float_value(p2)?),
        ("alpha", float_value(alpha)?),
        ("sided", text(sided)),
        ("method", text("normal_approximation")),
        ("assumptions", assumptions_value(&assumptions)),
    ]);
    let mut outcome = Outcome::approximate(value);
    for (index, statement) in assumptions.iter().enumerate() {
        outcome = outcome.with_assumption(Assumption::unverified(
            format!("power_two_proportions_{index}"),
            *statement,
        ));
    }
    Ok(outcome)
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

pub fn functions() -> Vec<Arc<dyn bicmath_core::contract::Function>> {
    vec![
        SimpleFunction::arc(ci_mean_descriptor(), invoke_ci_mean),
        SimpleFunction::arc(ci_proportion_descriptor(), invoke_ci_proportion),
        SimpleFunction::arc(welch_ci_descriptor(), invoke_welch_ci),
        SimpleFunction::arc(
            proportions_difference_descriptor(),
            invoke_proportions_difference,
        ),
        SimpleFunction::arc(chi_square_descriptor(), invoke_chi_square_contingency),
        SimpleFunction::arc(sample_size_means_descriptor(), invoke_sample_size_two_means),
        SimpleFunction::arc(
            sample_size_proportions_descriptor(),
            invoke_sample_size_two_proportions,
        ),
        SimpleFunction::arc(power_means_descriptor(), invoke_power_two_means),
        SimpleFunction::arc(power_proportions_descriptor(), invoke_power_two_proportions),
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

    fn field<'a>(outcome: &'a Outcome, name: &str) -> &'a Value {
        match &outcome.value {
            Value::Record(fields) => fields.get(name).expect("field exists"),
            other => panic!("expected record, got {other:?}"),
        }
    }

    fn as_f64(value: &Value) -> f64 {
        match value {
            Value::Number(number) => number.to_f64().expect("number"),
            other => panic!("expected number, got {other:?}"),
        }
    }

    #[test]
    fn ci_mean_matches_reference() {
        // values 1..=10: mean 5.5, sample variance 9.1666..., se = 0.9574...
        let outcome = call(
            "statistics.ci_mean",
            serde_json::json!({"values": [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]}),
        )
        .unwrap();
        assert_eq!(as_f64(field(&outcome, "estimate")), 5.5);
        let se = as_f64(field(&outcome, "standard_error"));
        assert!((se - (9.166_666_666_666_667f64 / 10.0).sqrt()).abs() < 1e-12);
        assert_eq!(as_f64(field(&outcome, "df")), 9.0);
        assert_eq!(field(&outcome, "method"), &text("t"));
        // Independent reference: t_{0.975, 9} = 2.2621571628540993
        let critical = 2.262_157_162_854_099_3;
        assert!((as_f64(field(&outcome, "lower")) - (5.5 - critical * se)).abs() < 1e-9);
    }

    #[test]
    fn ci_proportion_wilson_and_wald() {
        let outcome = call(
            "statistics.ci_proportion",
            serde_json::json!({"successes": 5, "n": 10}),
        )
        .unwrap();
        // Wilson interval for 5/10 at 95% is (0.2366, 0.7634).
        assert!((as_f64(field(&outcome, "lower")) - 0.236_593).abs() < 1e-5);
        assert!((as_f64(field(&outcome, "upper")) - 0.763_407).abs() < 1e-5);
        let wald = call(
            "statistics.ci_proportion",
            serde_json::json!({"successes": 1, "n": 10, "method": "wald"}),
        )
        .unwrap();
        assert!(!wald.warnings.is_empty());
    }

    #[test]
    fn welch_interval_uses_welch_df() {
        let outcome = call(
            "statistics.welch_ci",
            serde_json::json!({
                "sample_a": [1, 2, 3, 4, 5],
                "sample_b": [2, 4, 6, 8, 10]
            }),
        )
        .unwrap();
        assert_eq!(as_f64(field(&outcome, "estimate")), -3.0);
        let df = as_f64(field(&outcome, "df"));
        assert!(df > 4.0 && df < 8.0, "welch df out of range: {df}");
        assert_eq!(field(&outcome, "method"), &text("welch"));
    }

    #[test]
    fn proportions_difference_newcombe() {
        let outcome = call(
            "statistics.proportions_difference",
            serde_json::json!({
                "successes_a": 20, "n_a": 100,
                "successes_b": 30, "n_b": 100
            }),
        )
        .unwrap();
        assert!((as_f64(field(&outcome, "difference")) + 0.1).abs() < 1e-12);
        let lower = as_f64(field(&outcome, "lower"));
        let upper = as_f64(field(&outcome, "upper"));
        assert!(lower < -0.1 && upper > -0.1);
    }

    #[test]
    fn chi_square_reference_table() {
        let outcome = call(
            "statistics.chi_square_contingency",
            serde_json::json!({"table": [[10, 20], [30, 40]]}),
        )
        .unwrap();
        let statistic = as_f64(field(&outcome, "statistic"));
        // Pearson statistic for [[10,20],[30,40]] is 50/63 ~ 0.7936507936507937
        assert!((statistic - 50.0 / 63.0).abs() < 1e-12);
        assert_eq!(as_f64(field(&outcome, "df")), 1.0);
        assert!(matches!(field(&outcome, "expected"), Value::Matrix { .. }));
    }

    #[test]
    fn sample_size_and_power_are_consistent() {
        let plan = call(
            "statistics.sample_size_two_means",
            serde_json::json!({"effect_size": "0.5"}),
        )
        .unwrap();
        let n = as_f64(field(&plan, "n1"));
        assert!((n - 63.0).abs() < 1.0, "expected ~63 per group, got {n}");
        let power = call(
            "statistics.power_two_means",
            serde_json::json!({"n_per_group": 64, "effect_size": "0.5"}),
        )
        .unwrap();
        assert!((as_f64(field(&power, "power")) - 0.801).abs() < 0.01);
    }

    #[test]
    fn no_probability_true_effect_field() {
        let outcome = call(
            "statistics.power_two_proportions",
            serde_json::json!({"n_per_group": 100, "p1": "0.4", "p2": "0.5"}),
        )
        .unwrap();
        if let Value::Record(fields) = &outcome.value {
            assert!(!fields.contains_key("probability_true_effect_positive"));
        }
    }
}
