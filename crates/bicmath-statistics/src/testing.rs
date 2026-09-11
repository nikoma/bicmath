//! Hypothesis-testing helpers: one-way ANOVA, multiple-comparison p-value
//! adjustment, and two one-sided tests (TOST) for equivalence of means.
//!
//! Every result is a float64 record and requires auto or scientific mode. The
//! methods are named explicitly in the output so that the assumptions of each
//! test travel with the result.

use std::sync::Arc;

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, CostClass, Example, FunctionDescriptor, Outcome, ParamDescriptor, SimpleFunction,
    require_mode,
};
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::schema::ValueSchema;
use bicmath_core::value::Value;

use crate::common::*;
use crate::mathfn::{f_sf, sqrt, student_t_cdf, student_t_quantile, student_t_sf};

fn collect_f64(args: &Args, name: &str) -> Result<Vec<f64>, EngineError> {
    let numbers = collect_numbers(args, name)?;
    if numbers.is_empty() {
        return Err(insufficient(format!("{name} must not be empty")));
    }
    numbers
        .iter()
        .enumerate()
        .map(|(index, number)| {
            number_to_f64(number).map_err(|error| error.with_path(format!("{name}[{index}]")))
        })
        .collect()
}

fn float_array(values: &[f64]) -> Result<Value, EngineError> {
    values
        .iter()
        .map(|value| float_value(*value))
        .collect::<Result<Vec<_>, _>>()
        .map(array_value)
}

// ---------------------------------------------------------------------------
// anova_one_way
// ---------------------------------------------------------------------------

fn anova_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "statistics.anova_one_way",
        "statistics",
        "1.0.0",
        "One-way analysis of variance",
        "Classical one-way fixed-effects ANOVA F test.",
    )
    .with_description(
        "groups must contain at least two non-empty arrays of observations. Computes the \
         between-group and within-group sums of squares, the F statistic (mean square ratio), \
         and the upper-tail p-value from the F distribution. At least one residual degree of \
         freedom is required. Returns f_statistic, df_between, df_within, p_value, \
         ss_between, ss_within, group_means, and method = \"one_way_anova_f\". A zero \
         within-group sum of squares with a positive between-group sum of squares makes the F \
         statistic unbounded and is rejected as a domain violation.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "groups",
        "Groups of observations: an array of non-empty numeric arrays.",
        ValueSchema::Any,
    )])
    .with_output(
        record_schema(
            vec![
                field("f_statistic", float64_schema()),
                field("df_between", float64_schema()),
                field("df_within", float64_schema()),
                field("p_value", float64_schema()),
                field("ss_between", float64_schema()),
                field("ss_within", float64_schema()),
                field("group_means", array_schema(float64_schema())),
                field("method", text_schema()),
            ],
            false,
        ),
        "One-way ANOVA record.",
    )
    .with_modes(inferential_modes())
    .with_cost(CostClass::Linear)
    .with_method_ref("docs/methods/statistics.md#anova_one_way")
    .with_examples(vec![
        Example::new(
            "two shifted groups",
            example_args(&[("groups", serde_json::json!([[1, 2, 3], [4, 5, 6]]))]),
        )
        .with_contains("one_way_anova_f"),
        Example::new(
            "a single group",
            example_args(&[("groups", serde_json::json!([[1, 2, 3]]))]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_anova_one_way(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.anova_one_way")?;
    let groups_value = args.require("groups")?;
    let groups_array = groups_value
        .as_array()
        .map_err(|error| error.with_path("groups".to_string()))?;
    if groups_array.len() < 2 {
        return Err(
            EngineError::domain("anova_one_way requires at least two groups")
                .with_path("groups".to_string()),
        );
    }
    let mut groups: Vec<Vec<f64>> = Vec::with_capacity(groups_array.len());
    for (group_index, group) in groups_array.iter().enumerate() {
        let items = group
            .as_array()
            .map_err(|error| error.with_path(format!("groups[{group_index}]")))?;
        if items.is_empty() {
            return Err(
                EngineError::domain("each group must contain at least one observation")
                    .with_path(format!("groups[{group_index}]")),
            );
        }
        let mut values = Vec::with_capacity(items.len());
        for (value_index, item) in items.iter().enumerate() {
            let number = item.as_number().map_err(|error| {
                error.with_path(format!("groups[{group_index}][{value_index}]"))
            })?;
            values.push(number_to_f64(number).map_err(|error| {
                error.with_path(format!("groups[{group_index}][{value_index}]"))
            })?);
        }
        groups.push(values);
    }
    let group_count = groups.len();
    let total: usize = groups.iter().map(Vec::len).sum();
    if total <= group_count {
        return Err(insufficient(
            "anova_one_way requires at least one residual degree of freedom",
        ));
    }
    let all: Vec<f64> = groups.iter().flatten().copied().collect();
    let grand_mean = mean_f64(&all);
    let mut ss_between = 0.0f64;
    let mut ss_within = 0.0f64;
    let mut group_means = Vec::with_capacity(group_count);
    for group in &groups {
        let mean = mean_f64(group);
        group_means.push(mean);
        ss_between += group.len() as f64 * (mean - grand_mean) * (mean - grand_mean);
        for value in group {
            ss_within += (value - mean) * (value - mean);
        }
    }
    if ss_within == 0.0 {
        return Err(EngineError::domain(
            "the F statistic is unbounded because the within-group variance is zero",
        ));
    }
    let df_between = (group_count - 1) as f64;
    let df_within = (total - group_count) as f64;
    let f_statistic = (ss_between / df_between) / (ss_within / df_within);
    let p_value = f_sf(f_statistic, df_between, df_within)?;
    let value = record(vec![
        ("f_statistic", float_value(f_statistic)?),
        ("df_between", float_value(df_between)?),
        ("df_within", float_value(df_within)?),
        ("p_value", float_value(p_value)?),
        ("ss_between", float_value(ss_between)?),
        ("ss_within", float_value(ss_within)?),
        ("group_means", float_array(&group_means)?),
        ("method", text("one_way_anova_f")),
    ]);
    Ok(Outcome::approximate(value))
}

// ---------------------------------------------------------------------------
// p_adjust
// ---------------------------------------------------------------------------

const ADJUSTMENT_METHODS: [&str; 4] = ["bonferroni", "holm", "hochberg", "bh"];

fn p_adjust_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "statistics.p_adjust",
        "statistics",
        "1.0.0",
        "Multiple-comparison p-value adjustment",
        "Adjust p-values for multiple testing.",
    )
    .with_description(
        "Returns the adjusted p-values in the same order as the input. bonferroni multiplies \
         each p-value by the number of tests m and caps at 1. holm is the step-down \
         Bonferroni-Holm method. hochberg is the step-up Hochberg method. bh is the \
         Benjamini-Hochberg false discovery rate procedure. All p-values must lie in [0, 1] \
         and the array must not be empty.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "p_values",
            "Raw p-values in [0, 1].",
            array_schema(any_number_schema()),
        ),
        ParamDescriptor::required(
            "method",
            "Adjustment method: bonferroni, holm, hochberg, or bh.",
            ValueSchema::Enum {
                variants: ADJUSTMENT_METHODS
                    .iter()
                    .map(|method| (*method).to_string())
                    .collect(),
            },
        ),
    ])
    .with_output(
        record_schema(
            vec![
                field("adjusted", array_schema(float64_schema())),
                field("method", text_schema()),
            ],
            false,
        ),
        "Adjusted p-values in input order plus the method label.",
    )
    .with_modes(inferential_modes())
    .with_cost(CostClass::Linear)
    .with_method_ref("docs/methods/statistics.md#p_adjust")
    .with_examples(vec![
        Example::new(
            "holm adjustment",
            example_args(&[
                ("p_values", serde_json::json!([0.01, 0.02, 0.03, 0.04])),
                ("method", serde_json::json!("holm")),
            ]),
        )
        .with_contains("holm"),
        Example::new(
            "p-value above one",
            example_args(&[
                ("p_values", serde_json::json!([0.5, 1.5])),
                ("method", serde_json::json!("bonferroni")),
            ]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_p_adjust(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.p_adjust")?;
    let method = args.text("method")?.to_string();
    if !ADJUSTMENT_METHODS.contains(&method.as_str()) {
        return Err(EngineError::domain(format!(
            "unknown method {method:?}; expected one of {ADJUSTMENT_METHODS:?}"
        ))
        .with_path("method".to_string()));
    }
    let values = collect_f64(args, "p_values")?;
    for (index, value) in values.iter().enumerate() {
        if !(0.0..=1.0).contains(value) {
            return Err(EngineError::domain("p-values must lie in [0, 1]")
                .with_path(format!("p_values[{index}]")));
        }
    }
    let m = values.len();
    let mut order: Vec<usize> = (0..m).collect();
    order.sort_by(|a, b| values[*a].total_cmp(&values[*b]));
    let mut adjusted = vec![0.0f64; m];
    match method.as_str() {
        "bonferroni" => {
            let factor = m as f64;
            for (index, value) in values.iter().enumerate() {
                adjusted[index] = (factor * value).min(1.0);
            }
        }
        "holm" => {
            let mut running = 0.0f64;
            for (rank, index) in order.iter().enumerate() {
                let factor = (m - rank) as f64;
                running = running.max(factor * values[*index]);
                adjusted[*index] = running.min(1.0);
            }
        }
        "hochberg" => {
            let mut running = 1.0f64;
            for (rank, index) in order.iter().enumerate().rev() {
                let factor = (m - rank) as f64;
                running = running.min(factor * values[*index]);
                adjusted[*index] = running.min(1.0);
            }
        }
        "bh" => {
            let mut running = 1.0f64;
            for (rank, index) in order.iter().enumerate().rev() {
                let factor = m as f64 / (rank + 1) as f64;
                running = running.min(factor * values[*index]);
                adjusted[*index] = running.min(1.0);
            }
        }
        other => {
            return Err(EngineError::domain(format!(
                "unknown method {other:?}; expected one of {ADJUSTMENT_METHODS:?}"
            ))
            .with_path("method".to_string()));
        }
    }
    let value = record(vec![
        ("adjusted", float_array(&adjusted)?),
        ("method", text(method)),
    ]);
    Ok(Outcome::approximate(value))
}

// ---------------------------------------------------------------------------
// tost_two_means
// ---------------------------------------------------------------------------

fn tost_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "statistics.tost_two_means",
        "statistics",
        "1.0.0",
        "Two one-sided tests for equivalence of means",
        "TOST equivalence test for two independent means (Welch).",
    )
    .with_description(
        "Tests H0: |mean_a - mean_b| >= margin against the two one-sided alternatives at \
         alpha = 1 - confidence, using the Welch standard error and Welch-Satterthwaite \
         degrees of freedom. The reported p_value is the larger of the two one-sided \
         p-values; equivalence holds when p_value < 1 - confidence. ci_lower and ci_upper \
         are the two-sided confidence interval for the difference with the same alpha. Each \
         sample needs at least two observations, margin must be strictly positive, and the \
         Welch standard error must be positive.",
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
        ParamDescriptor::required(
            "margin",
            "Equivalence margin; must be > 0.",
            any_number_schema(),
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
                field("p_value", float64_schema()),
                field("ci_lower", float64_schema()),
                field("ci_upper", float64_schema()),
                field("equivalent", bool_schema()),
                field("method", text_schema()),
                field("df", float64_schema()),
            ],
            false,
        ),
        "TOST equivalence record.",
    )
    .with_modes(inferential_modes())
    .with_cost(CostClass::Linear)
    .with_method_ref("docs/methods/statistics.md#tost_two_means")
    .with_examples(vec![
        Example::new(
            "equivalent samples",
            example_args(&[
                ("sample_a", serde_json::json!([1, 2, 3, 4])),
                ("sample_b", serde_json::json!([1.1, 2.1, 3.1, 4.1])),
                ("margin", serde_json::json!(2)),
            ]),
        )
        .with_contains("tost_welch"),
        Example::new(
            "zero margin",
            example_args(&[
                ("sample_a", serde_json::json!([1, 2, 3])),
                ("sample_b", serde_json::json!([2, 3, 4])),
                ("margin", serde_json::json!(0)),
            ]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_tost_two_means(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.tost_two_means")?;
    let margin = scalar_f64(args, "margin")?;
    if margin <= 0.0 {
        return Err(
            EngineError::domain("margin must be strictly positive").with_path("margin".to_string())
        );
    }
    let confidence = confidence_param(args)?;
    let sample_a = classify_series(args, "sample_a", ctx)?.to_f64_vec()?;
    let sample_b = classify_series(args, "sample_b", ctx)?.to_f64_vec()?;
    if sample_a.len() < 2 || sample_b.len() < 2 {
        return Err(insufficient(
            "tost_two_means requires at least two observations per sample",
        ));
    }
    let mean_a = mean_f64(&sample_a);
    let mean_b = mean_f64(&sample_b);
    let variance_a = variance_f64(&sample_a, 1)?;
    let variance_b = variance_f64(&sample_b, 1)?;
    let n_a = sample_a.len() as f64;
    let n_b = sample_b.len() as f64;
    let component_a = variance_a / n_a;
    let component_b = variance_b / n_b;
    let standard_error = sqrt(component_a + component_b);
    if standard_error <= 0.0 {
        return Err(EngineError::domain(
            "the TOST is undefined when both samples are constant (zero standard error)",
        ));
    }
    let df = (component_a + component_b).powi(2)
        / (component_a.powi(2) / (n_a - 1.0) + component_b.powi(2) / (n_b - 1.0));
    if !df.is_finite() || df <= 0.0 {
        return Err(EngineError::domain(
            "Welch-Satterthwaite degrees of freedom are undefined",
        ));
    }
    let difference = mean_a - mean_b;
    let lower_test = student_t_sf((difference + margin) / standard_error, df)?;
    let upper_test = student_t_cdf((difference - margin) / standard_error, df)?;
    let p_value = lower_test.max(upper_test).min(1.0);
    let critical = student_t_quantile(confidence, df)?;
    let ci_lower = difference - critical * standard_error;
    let ci_upper = difference + critical * standard_error;
    let equivalent = p_value < 1.0 - confidence;
    let value = record(vec![
        ("p_value", float_value(p_value)?),
        ("ci_lower", float_value(ci_lower)?),
        ("ci_upper", float_value(ci_upper)?),
        ("equivalent", bool_value(equivalent)),
        ("method", text("tost_welch")),
        ("df", float_value(df)?),
    ]);
    Ok(Outcome::approximate(value))
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

pub fn functions() -> Vec<Arc<dyn bicmath_core::contract::Function>> {
    vec![
        SimpleFunction::arc(anova_descriptor(), invoke_anova_one_way),
        SimpleFunction::arc(p_adjust_descriptor(), invoke_p_adjust),
        SimpleFunction::arc(tost_descriptor(), invoke_tost_two_means),
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

    fn field_numbers(fields: &BTreeMap<String, Value>, name: &str) -> Vec<f64> {
        match fields.get(name) {
            Some(Value::Array(items)) => items
                .iter()
                .map(|item| item.as_number().expect("number").to_f64().expect("f64"))
                .collect(),
            other => panic!("expected array field {name}, got {other:?}"),
        }
    }

    fn close(actual: f64, expected: f64, tolerance: f64) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "expected {expected}, got {actual} (tolerance {tolerance})"
        );
    }

    fn close_slice(actual: &[f64], expected: &[f64], tolerance: f64) {
        assert_eq!(actual.len(), expected.len());
        for (a, e) in actual.iter().zip(expected.iter()) {
            close(*a, *e, tolerance);
        }
    }

    #[test]
    fn anova_reference_values() {
        // Provenance: hand calculation for the shifted groups
        // [1, 2, 3], [4, 5, 6], [7, 8, 9]: grand mean 5, SS_between 54,
        // SS_within 6, F(2, 6) = 27. The p-value 0.001 is the exact closed
        // form for F(2, 6): P(F > 27) = 1 / 1000.
        let outcome = call(
            "statistics.anova_one_way",
            serde_json::json!({"groups": [[1, 2, 3], [4, 5, 6], [7, 8, 9]]}),
        )
        .unwrap();
        let fields = record_of(&outcome);
        close(field_f64(fields, "f_statistic"), 27.0, 1e-12);
        close(field_f64(fields, "df_between"), 2.0, 0.0);
        close(field_f64(fields, "df_within"), 6.0, 0.0);
        close(field_f64(fields, "ss_between"), 54.0, 1e-12);
        close(field_f64(fields, "ss_within"), 6.0, 1e-12);
        close(field_f64(fields, "p_value"), 0.001, 1e-9);
        close_slice(
            &field_numbers(fields, "group_means"),
            &[2.0, 5.0, 8.0],
            1e-12,
        );
    }

    #[test]
    fn anova_rejects_bad_inputs() {
        let error = call(
            "statistics.anova_one_way",
            serde_json::json!({"groups": [[1, 2, 3]]}),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::DomainViolation);
        let error = call(
            "statistics.anova_one_way",
            serde_json::json!({"groups": [[1, 2, 3], []]}),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::DomainViolation);
    }

    #[test]
    fn p_adjust_reference_values() {
        // Provenance: hand calculation (and Python 3.14 stdlib check) of the
        // step-wise procedures for p = [0.01, 0.02, 0.03, 0.04].
        let cases = [
            ("bonferroni", [0.04, 0.08, 0.12, 0.16]),
            ("holm", [0.04, 0.06, 0.06, 0.06]),
            ("hochberg", [0.04, 0.04, 0.04, 0.04]),
            ("bh", [0.04, 0.04, 0.04, 0.04]),
        ];
        for (method, expected) in cases {
            let outcome = call(
                "statistics.p_adjust",
                serde_json::json!({"p_values": [0.01, 0.02, 0.03, 0.04], "method": method}),
            )
            .unwrap();
            let fields = record_of(&outcome);
            close_slice(&field_numbers(fields, "adjusted"), &expected, 1e-12);
        }
    }

    #[test]
    fn p_adjust_preserves_input_order() {
        let outcome = call(
            "statistics.p_adjust",
            serde_json::json!({"p_values": [0.04, 0.01, 0.03, 0.02], "method": "holm"}),
        )
        .unwrap();
        let fields = record_of(&outcome);
        close_slice(
            &field_numbers(fields, "adjusted"),
            &[0.06, 0.04, 0.06, 0.06],
            1e-12,
        );
    }

    #[test]
    fn p_adjust_rejects_out_of_range_values() {
        let error = call(
            "statistics.p_adjust",
            serde_json::json!({"p_values": [0.5, 1.5], "method": "holm"}),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::DomainViolation);
    }

    #[test]
    fn tost_reference_values() {
        // Provenance: Python 3.14 stdlib Welch TOST for the samples below:
        // difference -0.1, standard error 0.9128709291752768, df 6,
        // p = 0.04129045682405402, 90% interval [-1.873872788229078,
        // 1.6738727882290778].
        let outcome = call(
            "statistics.tost_two_means",
            serde_json::json!({
                "sample_a": [1, 2, 3, 4],
                "sample_b": [1.1, 2.1, 3.1, 4.1],
                "margin": 2
            }),
        )
        .unwrap();
        let fields = record_of(&outcome);
        close(field_f64(fields, "p_value"), 0.041_290_456_824_054_02, 1e-9);
        close(field_f64(fields, "df"), 6.0, 1e-9);
        close(field_f64(fields, "ci_lower"), -1.873_872_788_229_078, 1e-9);
        close(field_f64(fields, "ci_upper"), 1.673_872_788_229_077_8, 1e-9);
        match fields.get("equivalent") {
            Some(Value::Bool(true)) => {}
            other => panic!("expected equivalent = true, got {other:?}"),
        }
    }

    #[test]
    fn tost_nearly_identical_samples_are_equivalent() {
        // Provenance: Python 3.14 stdlib Welch TOST: p = 0.0001327261042549249,
        // df = 4, difference -0.01, standard error 0.08164965809277268.
        let outcome = call(
            "statistics.tost_two_means",
            serde_json::json!({
                "sample_a": [5.0, 5.1, 5.2],
                "sample_b": [5.01, 5.11, 5.21],
                "margin": 1.0
            }),
        )
        .unwrap();
        let fields = record_of(&outcome);
        close(
            field_f64(fields, "p_value"),
            0.000_132_726_104_254_924_9,
            1e-9,
        );
        close(field_f64(fields, "df"), 4.0, 1e-9);
        match fields.get("equivalent") {
            Some(Value::Bool(true)) => {}
            other => panic!("expected equivalent = true, got {other:?}"),
        }
    }

    #[test]
    fn tost_rejects_non_equivalent_samples() {
        let outcome = call(
            "statistics.tost_two_means",
            serde_json::json!({
                "sample_a": [1, 2, 3, 4],
                "sample_b": [1.1, 2.1, 3.1, 4.1],
                "margin": 0.05
            }),
        )
        .unwrap();
        let fields = record_of(&outcome);
        match fields.get("equivalent") {
            Some(Value::Bool(false)) => {}
            other => panic!("expected equivalent = false, got {other:?}"),
        }
    }

    #[test]
    fn tost_rejects_invalid_inputs() {
        let error = call(
            "statistics.tost_two_means",
            serde_json::json!({
                "sample_a": [1, 2],
                "sample_b": [1, 2],
                "margin": 0
            }),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::DomainViolation);
        let error = call(
            "statistics.tost_two_means",
            serde_json::json!({
                "sample_a": [1],
                "sample_b": [1, 2],
                "margin": 1
            }),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::InsufficientObservations);
    }
}
