//! Plan module: pure rule-based method recommendations, experiment checklists,
//! field-level analysis plans, and interpretation notes.
//!
//! Every function in this module is a deterministic lookup over fixed rules:
//! no model, no network, no randomness. Recommendations name only function ids
//! that are registered in this build; methods that are not available are called
//! out in the caveats rather than silently recommended.

use std::collections::BTreeMap;
use std::sync::Arc;

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, CostClass, Example, FunctionDescriptor, Module, ModuleDescriptor, Outcome,
    ParamDescriptor, SimpleFunction,
};
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::number::{Number, NumericMode};
use bicmath_core::schema::{FieldSchema, NumberKind, ValueSchema};
use bicmath_core::value::Value;

const TASK_VARIANTS: [&str; 14] = [
    "estimate_mean",
    "compare_means",
    "compare_proportions",
    "estimate_proportion",
    "correlation",
    "regression",
    "experiment_design",
    "break_even",
    "cashflow_analysis",
    "unit_conversion",
    "root_finding",
    "optimization",
    "time_series",
    "causal_effect",
];

const SHAPE_VARIANTS: [&str; 7] = [
    "one_sample",
    "two_independent",
    "two_paired",
    "many_groups",
    "bivariate",
    "multivariate",
    "none",
];

const GOAL_VARIANTS: [&str; 4] = ["estimate", "test", "predict", "decide"];

const FIELD_KIND_VARIANTS: [&str; 6] = [
    "count",
    "continuous",
    "categorical",
    "money",
    "date",
    "text",
];

const EXPERIMENT_KIND_VARIANTS: [&str; 4] =
    ["two_proportions", "two_means", "stratified", "general"];

// ---------------------------------------------------------------------------
// Enumerations
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Task {
    EstimateMean,
    CompareMeans,
    CompareProportions,
    EstimateProportion,
    Correlation,
    Regression,
    ExperimentDesign,
    BreakEven,
    CashflowAnalysis,
    UnitConversion,
    RootFinding,
    Optimization,
    TimeSeries,
    CausalEffect,
}

impl Task {
    fn parse(text: &str) -> Result<Task, EngineError> {
        match text {
            "estimate_mean" => Ok(Task::EstimateMean),
            "compare_means" => Ok(Task::CompareMeans),
            "compare_proportions" => Ok(Task::CompareProportions),
            "estimate_proportion" => Ok(Task::EstimateProportion),
            "correlation" => Ok(Task::Correlation),
            "regression" => Ok(Task::Regression),
            "experiment_design" => Ok(Task::ExperimentDesign),
            "break_even" => Ok(Task::BreakEven),
            "cashflow_analysis" => Ok(Task::CashflowAnalysis),
            "unit_conversion" => Ok(Task::UnitConversion),
            "root_finding" => Ok(Task::RootFinding),
            "optimization" => Ok(Task::Optimization),
            "time_series" => Ok(Task::TimeSeries),
            "causal_effect" => Ok(Task::CausalEffect),
            other => Err(EngineError::domain(format!(
                "unknown task {other:?}; expected one of {TASK_VARIANTS:?}"
            ))),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Task::EstimateMean => "estimate_mean",
            Task::CompareMeans => "compare_means",
            Task::CompareProportions => "compare_proportions",
            Task::EstimateProportion => "estimate_proportion",
            Task::Correlation => "correlation",
            Task::Regression => "regression",
            Task::ExperimentDesign => "experiment_design",
            Task::BreakEven => "break_even",
            Task::CashflowAnalysis => "cashflow_analysis",
            Task::UnitConversion => "unit_conversion",
            Task::RootFinding => "root_finding",
            Task::Optimization => "optimization",
            Task::TimeSeries => "time_series",
            Task::CausalEffect => "causal_effect",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DataShape {
    OneSample,
    TwoIndependent,
    TwoPaired,
    ManyGroups,
    Bivariate,
    Multivariate,
    None,
}

impl DataShape {
    fn parse(text: &str) -> Result<DataShape, EngineError> {
        match text {
            "one_sample" => Ok(DataShape::OneSample),
            "two_independent" => Ok(DataShape::TwoIndependent),
            "two_paired" => Ok(DataShape::TwoPaired),
            "many_groups" => Ok(DataShape::ManyGroups),
            "bivariate" => Ok(DataShape::Bivariate),
            "multivariate" => Ok(DataShape::Multivariate),
            "none" => Ok(DataShape::None),
            other => Err(EngineError::domain(format!(
                "unknown data_shape {other:?}; expected one of {SHAPE_VARIANTS:?}"
            ))),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            DataShape::OneSample => "one_sample",
            DataShape::TwoIndependent => "two_independent",
            DataShape::TwoPaired => "two_paired",
            DataShape::ManyGroups => "many_groups",
            DataShape::Bivariate => "bivariate",
            DataShape::Multivariate => "multivariate",
            DataShape::None => "none",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Goal {
    Estimate,
    Test,
    Predict,
    Decide,
}

impl Goal {
    fn parse(text: &str) -> Result<Goal, EngineError> {
        match text {
            "estimate" => Ok(Goal::Estimate),
            "test" => Ok(Goal::Test),
            "predict" => Ok(Goal::Predict),
            "decide" => Ok(Goal::Decide),
            other => Err(EngineError::domain(format!(
                "unknown goal {other:?}; expected one of {GOAL_VARIANTS:?}"
            ))),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Goal::Estimate => "estimate",
            Goal::Test => "test",
            Goal::Predict => "predict",
            Goal::Decide => "decide",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExperimentKind {
    TwoProportions,
    TwoMeans,
    Stratified,
    General,
}

impl ExperimentKind {
    fn parse(text: &str) -> Result<ExperimentKind, EngineError> {
        match text {
            "two_proportions" => Ok(ExperimentKind::TwoProportions),
            "two_means" => Ok(ExperimentKind::TwoMeans),
            "stratified" => Ok(ExperimentKind::Stratified),
            "general" => Ok(ExperimentKind::General),
            other => Err(EngineError::domain(format!(
                "unknown kind {other:?}; expected one of {EXPERIMENT_KIND_VARIANTS:?}"
            ))),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            ExperimentKind::TwoProportions => "two_proportions",
            ExperimentKind::TwoMeans => "two_means",
            ExperimentKind::Stratified => "stratified",
            ExperimentKind::General => "general",
        }
    }

    fn sample_size_functions(self) -> &'static [&'static str] {
        match self {
            ExperimentKind::TwoProportions => &[
                "statistics.sample_size_two_proportions",
                "statistics.power_two_proportions",
            ],
            ExperimentKind::TwoMeans => &[
                "statistics.sample_size_two_means",
                "statistics.power_two_means",
            ],
            ExperimentKind::Stratified => &[
                "statistics.sample_size_two_means",
                "statistics.sample_size_two_proportions",
                "statistics.power_two_means",
                "statistics.power_two_proportions",
                "statistics.stratified_experiment",
                "statistics.prospective_pool",
            ],
            ExperimentKind::General => &[
                "statistics.sample_size_two_means",
                "statistics.sample_size_two_proportions",
            ],
        }
    }

    fn sample_size_inputs(self) -> &'static [&'static str] {
        match self {
            ExperimentKind::TwoProportions => &[
                "baseline_rate",
                "minimum_detectable_effect",
                "alpha",
                "power",
                "allocation_ratio",
            ],
            ExperimentKind::TwoMeans => &[
                "standard_deviation",
                "minimum_detectable_effect",
                "alpha",
                "power",
                "allocation_ratio",
            ],
            ExperimentKind::Stratified => &[
                "endpoint_type",
                "baseline_rate_or_standard_deviation",
                "minimum_detectable_effect",
                "alpha",
                "power",
                "allocation_ratio",
                "strata",
                "target_mix",
            ],
            ExperimentKind::General => &[
                "endpoint_type",
                "baseline_rate_or_standard_deviation",
                "minimum_detectable_effect",
                "alpha",
                "power",
                "allocation_ratio",
            ],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FieldKind {
    Count,
    Continuous,
    Categorical,
    Money,
    Date,
    Text,
}

impl FieldKind {
    fn parse(text: &str) -> Result<FieldKind, EngineError> {
        match text {
            "count" => Ok(FieldKind::Count),
            "continuous" => Ok(FieldKind::Continuous),
            "categorical" => Ok(FieldKind::Categorical),
            "money" => Ok(FieldKind::Money),
            "date" => Ok(FieldKind::Date),
            "text" => Ok(FieldKind::Text),
            other => Err(EngineError::domain(format!(
                "unknown field type {other:?}; expected one of {FIELD_KIND_VARIANTS:?}"
            ))),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            FieldKind::Count => "count",
            FieldKind::Continuous => "continuous",
            FieldKind::Categorical => "categorical",
            FieldKind::Money => "money",
            FieldKind::Date => "date",
            FieldKind::Text => "text",
        }
    }

    fn rank(self) -> u8 {
        match self {
            FieldKind::Count => 0,
            FieldKind::Continuous => 1,
            FieldKind::Money => 2,
            FieldKind::Categorical => 3,
            FieldKind::Text => 4,
            FieldKind::Date => 5,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Family {
    RateOfReturn,
    Finance,
    Standardization,
    Inference,
    Planning,
    Descriptive,
    Regression,
    RootFinding,
    Units,
    Money,
    Distribution,
    Unknown,
}

impl Family {
    fn classify(id: &str) -> Family {
        if id == "finance.irr" || id == "finance.xirr" {
            return Family::RateOfReturn;
        }
        if id == "statistics.stratified_experiment" || id == "statistics.prospective_pool" {
            return Family::Standardization;
        }
        if id.starts_with("statistics.sample_size_") || id.starts_with("statistics.power_") {
            return Family::Planning;
        }
        if id.starts_with("statistics.ci_")
            || id == "statistics.welch_ci"
            || id == "statistics.proportions_difference"
            || id == "statistics.chi_square_contingency"
        {
            return Family::Inference;
        }
        if is_distribution_id(id) {
            return Family::Distribution;
        }
        if id == "linear_algebra.least_squares" {
            return Family::Regression;
        }
        if id == "scientific.root_find" {
            return Family::RootFinding;
        }
        if id.starts_with("units.") {
            return Family::Units;
        }
        if id.starts_with("finance.money_") || id == "finance.convert_money" {
            return Family::Money;
        }
        if id.starts_with("finance.") {
            return Family::Finance;
        }
        if id.starts_with("statistics.") {
            return Family::Descriptive;
        }
        Family::Unknown
    }

    fn as_str(self) -> &'static str {
        match self {
            Family::RateOfReturn => "rate_of_return",
            Family::Finance => "finance",
            Family::Standardization => "standardization",
            Family::Inference => "inference",
            Family::Planning => "planning",
            Family::Descriptive => "descriptive",
            Family::Regression => "regression",
            Family::RootFinding => "root_finding",
            Family::Units => "units",
            Family::Money => "money",
            Family::Distribution => "distribution",
            Family::Unknown => "unknown",
        }
    }

    fn notes(self) -> &'static [&'static str] {
        match self {
            Family::RateOfReturn => &RATE_OF_RETURN_NOTES,
            Family::Finance => &FINANCE_NOTES,
            Family::Standardization => &STANDARDIZATION_NOTES,
            Family::Inference => &INFERENCE_NOTES,
            Family::Planning => &PLANNING_NOTES,
            Family::Descriptive => &DESCRIPTIVE_NOTES,
            Family::Regression => &REGRESSION_NOTES,
            Family::RootFinding => &ROOT_FINDING_NOTES,
            Family::Units => &UNITS_NOTES,
            Family::Money => &MONEY_NOTES,
            Family::Distribution => &DISTRIBUTION_NOTES,
            Family::Unknown => &UNKNOWN_NOTES,
        }
    }
}

fn is_distribution_id(id: &str) -> bool {
    const SUFFIXES: [&str; 4] = ["_cdf", "_pdf", "_sf", "_quantile"];
    id.starts_with("statistics.normal_")
        || id.starts_with("statistics.student_t_")
        || id.starts_with("statistics.chi_square_")
        || id.starts_with("statistics.binomial_")
        || SUFFIXES.iter().any(|suffix| id.ends_with(suffix))
}

// ---------------------------------------------------------------------------
// Plan records
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Plan {
    recommended_functions: Vec<String>,
    why_eligible: Vec<(String, String)>,
    required_inputs: Vec<String>,
    assumptions: Vec<String>,
    diagnostics: Vec<String>,
    alternatives: Vec<(String, String)>,
    caveats: Vec<String>,
    unknowns: Vec<String>,
}

impl Plan {
    fn to_value(&self) -> Value {
        let why_eligible: Vec<Value> = self
            .why_eligible
            .iter()
            .map(|(function, reason)| {
                Value::record([
                    ("function", Value::text(function.as_str())),
                    ("reason", Value::text(reason.as_str())),
                ])
            })
            .collect();
        let alternatives: Vec<Value> = self
            .alternatives
            .iter()
            .map(|(function, when)| {
                Value::record([
                    ("function", Value::text(function.as_str())),
                    ("when", Value::text(when.as_str())),
                ])
            })
            .collect();
        Value::record([
            (
                "recommended_functions",
                string_array(&self.recommended_functions),
            ),
            ("why_eligible", Value::Array(why_eligible)),
            ("required_inputs", string_array(&self.required_inputs)),
            ("assumptions", string_array(&self.assumptions)),
            ("diagnostics", string_array(&self.diagnostics)),
            ("alternatives", Value::Array(alternatives)),
            ("caveats", string_array(&self.caveats)),
            ("unknowns", string_array(&self.unknowns)),
        ])
    }
}

fn plan(
    recommended: &[&str],
    inputs: &[&str],
    assumptions: &[&str],
    diagnostics: &[&str],
    alternatives: &[(&str, &str)],
    caveats: &[&str],
) -> Plan {
    Plan {
        recommended_functions: recommended.iter().map(|item| item.to_string()).collect(),
        why_eligible: Vec::new(),
        unknowns: Vec::new(),
        required_inputs: inputs.iter().map(|item| item.to_string()).collect(),
        assumptions: assumptions.iter().map(|item| item.to_string()).collect(),
        diagnostics: diagnostics.iter().map(|item| item.to_string()).collect(),
        alternatives: alternatives
            .iter()
            .map(|(function, when)| (function.to_string(), when.to_string()))
            .collect(),
        caveats: caveats.iter().map(|item| item.to_string()).collect(),
    }
}

fn push_all(target: &mut Vec<String>, items: &[&str]) {
    target.extend(items.iter().map(|item| item.to_string()));
}

fn set_functions(plan: &mut Plan, functions: &[&str]) {
    plan.recommended_functions.clear();
    push_all(&mut plan.recommended_functions, functions);
}

fn ensure_function(plan: &mut Plan, id: &str) {
    if !plan
        .recommended_functions
        .iter()
        .any(|existing| existing == id)
    {
        plan.recommended_functions.push(id.to_string());
    }
}

// ---------------------------------------------------------------------------
// Eligibility reasons and unknowns
// ---------------------------------------------------------------------------

/// Fixed vocabulary for `plan.recommend` unknowns. An item is listed only when
/// the supplied inputs genuinely leave it open; nothing is ever inferred to
/// hold.
const UNKNOWNS_VOCABULARY: [&str; 8] = [
    "randomization",
    "independence between units",
    "comparability of groups at baseline",
    "absence of interference",
    "missing-data mechanism",
    "outcome completeness",
    "multiplicity across multiple endpoints",
    "data provenance",
];

const UNKNOWN_RANDOMIZATION: &str = "randomization";
const UNKNOWN_INDEPENDENCE: &str = "independence between units";
const UNKNOWN_BASELINE: &str = "comparability of groups at baseline";
const UNKNOWN_INTERFERENCE: &str = "absence of interference";
const UNKNOWN_MISSINGNESS: &str = "missing-data mechanism";
const UNKNOWN_COMPLETENESS: &str = "outcome completeness";
const UNKNOWN_MULTIPLICITY: &str = "multiplicity across multiple endpoints";
const UNKNOWN_PROVENANCE: &str = "data provenance";

/// Deterministic reason for recommending a function in a given context.
///
/// Every recommended id receives exactly one reason; the fallback still names
/// the registered family and the rule context, so `why_eligible` is never
/// empty for a recommended function.
fn eligibility_reason(id: &str, task: Task, shape: DataShape, goal: Goal) -> String {
    match id {
        "statistics.welch_ci" if shape == DataShape::TwoIndependent => {
            "two independent samples with continuous outcomes -> Welch interval for the \
             difference of means"
                .to_string()
        }
        "statistics.welch_ci" => {
            "continuous outcomes compared across groups -> Welch interval for the difference \
             of means"
                .to_string()
        }
        "statistics.ci_mean" if shape == DataShape::TwoPaired => {
            "paired differences are independent by design -> t confidence interval for the \
             mean difference"
                .to_string()
        }
        "statistics.ci_mean" => {
            "single numeric sample -> t confidence interval for the mean".to_string()
        }
        "statistics.mean" => {
            "point estimate of the arithmetic mean for a numeric sample".to_string()
        }
        "statistics.median" => {
            "skewed or outlier-prone numeric field -> median instead of the mean".to_string()
        }
        "statistics.summary" => {
            "compact description of count, center, spread, and quantiles for a numeric field"
                .to_string()
        }
        "statistics.correlation" => {
            "two numeric fields -> linear association summary (Pearson r)".to_string()
        }
        "statistics.covariance" => {
            "two numeric fields -> scale-dependent joint variation".to_string()
        }
        "statistics.proportions_difference" => {
            "two independent groups with binary outcomes -> interval for the difference of \
             proportions"
                .to_string()
        }
        "statistics.ci_proportion" => {
            "single binary outcome -> interval for one proportion from successes and trials"
                .to_string()
        }
        "statistics.chi_square_contingency" => {
            "categorical outcome across two or more groups -> chi-square contingency test"
                .to_string()
        }
        "statistics.sample_size_two_means" => {
            "continuous endpoint planning -> per-group sample size for a two-sample mean \
             comparison"
                .to_string()
        }
        "statistics.power_two_means" => {
            "continuous endpoint planning -> achieved power for a two-sample mean comparison"
                .to_string()
        }
        "statistics.sample_size_two_proportions" => {
            "binary endpoint planning -> per-group sample size for a two-proportion comparison"
                .to_string()
        }
        "statistics.power_two_proportions" => {
            "binary endpoint planning -> achieved power for a two-proportion comparison".to_string()
        }
        "statistics.stratified_experiment" => {
            "stratified design -> standardized estimate over a declared target mix".to_string()
        }
        "statistics.prospective_pool" => {
            "observed and planned strata -> prospective pooling over the declared mix".to_string()
        }
        "linear_algebra.least_squares" => "linear model fitting -> QR least squares".to_string(),
        "linear_algebra.solve" => {
            "linear system with a square coefficient matrix -> direct solve".to_string()
        }
        "scientific.root_find" => {
            "continuous expression on a bracket -> bracketed root find".to_string()
        }
        "finance.break_even" => {
            "single-period linear cost model -> break-even quantity".to_string()
        }
        "finance.contribution" => {
            "unit price and unit variable cost -> contribution margin".to_string()
        }
        "finance.margin" => "cost and price -> margin ratio".to_string(),
        "finance.markup" => "cost and price -> markup ratio".to_string(),
        "finance.npv" => {
            "evenly spaced cash flows -> net present value at a declared discount rate".to_string()
        }
        "finance.irr" => "evenly spaced cash flows -> internal rate of return".to_string(),
        "finance.xnpv" => "dated cash flows -> net present value with explicit dates".to_string(),
        "finance.xirr" => {
            "dated cash flows -> internal rate of return with explicit dates".to_string()
        }
        "finance.present_value" => {
            "single future amount -> present value at a declared discount rate".to_string()
        }
        "finance.percentage_change" => "relative change between two amounts".to_string(),
        "finance.money_add" => {
            "monetary amounts in one currency -> exact decimal addition".to_string()
        }
        "finance.money_allocate" => {
            "monetary total -> allocation with an explicit remainder policy".to_string()
        }
        "finance.money_compare" => {
            "monetary amounts in one currency -> exact decimal comparison".to_string()
        }
        "units.convert" => {
            "quantity and target unit -> dimension-checked unit conversion".to_string()
        }
        "units.describe_unit" => "unit definition and notes lookup".to_string(),
        "units.list_units" => "registered unit listing".to_string(),
        "units.temperature_difference" => {
            "temperature difference -> difference-preserving conversion".to_string()
        }
        "statistics.count" => "category or group frequencies".to_string(),
        "statistics.mode" => "categorical field -> most frequent category".to_string(),
        "statistics.stddev" => "numeric field -> standard deviation".to_string(),
        "statistics.variance" => "numeric field -> variance".to_string(),
        "statistics.quantile" => "numeric field -> requested quantile".to_string(),
        other => format!(
            "registered {} method selected by the deterministic rule for task {}, \
             data_shape {}, and goal {}",
            Family::classify(other).as_str(),
            task.as_str(),
            shape.as_str(),
            goal.as_str()
        ),
    }
}

/// Deterministic unknown list for `plan.recommend`.
///
/// The supplied task, shape, goal, and `randomized` flag cannot establish any
/// of these; items are added by fixed rule and are never inferred to hold.
/// `randomized = true` only removes the randomization item for the
/// two-independent shape; independence between units remains unknown.
fn recommend_unknowns(task: Task, shape: DataShape, randomized: bool) -> Vec<&'static str> {
    let mut unknowns = vec![
        UNKNOWN_MISSINGNESS,
        UNKNOWN_COMPLETENESS,
        UNKNOWN_PROVENANCE,
    ];
    match shape {
        DataShape::TwoIndependent => {
            if !randomized {
                unknowns.push(UNKNOWN_RANDOMIZATION);
            }
            unknowns.push(UNKNOWN_INDEPENDENCE);
            unknowns.push(UNKNOWN_BASELINE);
        }
        DataShape::TwoPaired => {
            unknowns.push(UNKNOWN_INDEPENDENCE);
        }
        DataShape::ManyGroups => {
            unknowns.push(UNKNOWN_INDEPENDENCE);
            unknowns.push(UNKNOWN_BASELINE);
            unknowns.push(UNKNOWN_MULTIPLICITY);
        }
        DataShape::Bivariate | DataShape::Multivariate => {
            unknowns.push(UNKNOWN_MULTIPLICITY);
        }
        DataShape::OneSample | DataShape::None => {}
    }
    if matches!(
        task,
        Task::CompareMeans | Task::CompareProportions | Task::ExperimentDesign | Task::CausalEffect
    ) {
        unknowns.push(UNKNOWN_INTERFERENCE);
    }
    debug_assert!(
        unknowns
            .iter()
            .all(|item| UNKNOWNS_VOCABULARY.contains(item)),
        "unknowns must come from the fixed vocabulary"
    );
    unknowns
}

fn finalize_plan(plan: &mut Plan, task: Task, shape: DataShape, goal: Goal, randomized: bool) {
    plan.why_eligible = plan
        .recommended_functions
        .iter()
        .map(|id| (id.clone(), eligibility_reason(id, task, shape, goal)))
        .collect();
    plan.unknowns = recommend_unknowns(task, shape, randomized)
        .into_iter()
        .map(str::to_string)
        .collect();
}

// ---------------------------------------------------------------------------
// Rule tables
// ---------------------------------------------------------------------------

const SAMPLE_SIZE_NOTES: [&str; 4] = [
    "The registered sample-size and power functions use normal approximations; \
     check the exact distribution when the planned sample is small.",
    "Round the computed per-group size up and add an allowance for attrition and non-compliance.",
    "The result is only as good as the assumed effect size, variance, and baseline rate.",
    "Sample-size functions return per-group sizes unless their descriptor states otherwise.",
];

const PRE_REGISTRATION: [&str; 7] = [
    "State the primary endpoint, the target population, and the estimand before data collection.",
    "Fix the primary analysis, the test, and the significance threshold (alpha).",
    "Fix the target sample size and the stopping rule before looking at outcome data.",
    "Pre-specify exclusions, missing-data handling, and subgroup analyses.",
    "Record the assignment mechanism and the unit of analysis.",
    "Pre-specify the direction of the effect or commit to a two-sided test.",
    "Register the plan with a timestamp before enrollment begins.",
];

const STOPPING_RULES: [&str; 5] = [
    "Do not peek at outcome data and stop early: fixed-horizon p-values and intervals \
     are invalid under optional stopping.",
    "Repeatedly testing as data accumulate inflates the false-positive rate.",
    "If interim monitoring is required, pre-specify an alpha-spending or group-sequential \
     design with the number and timing of looks.",
    "Stopping for futility, harm, or administrative reasons must also be part of the rule.",
    "Any analysis chosen after seeing the data must be reported as exploratory.",
];

const CHECKLIST_ASSUMPTIONS: [&str; 4] = [
    "The effect size used for planning is plausible and justified by prior evidence or a pilot.",
    "Observations are independent and the unit of analysis matches the randomization unit.",
    "The endpoint distribution matches the planning model (normal approximation or proportion).",
    "Expected attrition and non-compliance are accounted for in the target sample size.",
];

const RATE_OF_RETURN_NOTES: [&str; 4] = [
    "IRR is a root of the NPV equation; a cash-flow series with more than one sign \
     change can have multiple real roots.",
    "An IRR may not exist at all when the NPV equation has no real root.",
    "IRR assumes reinvestment at the IRR itself; NPV is usually the safer ranking criterion.",
    "Report the cash-flow series and the number of sign changes together with the IRR.",
];

const FINANCE_NOTES: [&str; 3] = [
    "Financial projections are assumptions, not observations.",
    "NPV and IRR depend on the discount rate, the timing of cash flows, and the currency.",
    "Compare alternatives over the same horizon, currency, and convention.",
];

const STANDARDIZATION_NOTES: [&str; 4] = [
    "Standardized estimates reweight stratum-specific effects to a target mix; pooled or \
     raw rates do not replace standardization.",
    "Standardization is meaningful only when the target mix is pre-specified and not chosen \
     after seeing outcomes.",
    "Report the stratum-specific effects, the weights, and the target mix.",
    "Sparse or empty strata make the standardized estimate unstable; check positivity.",
];

const INFERENCE_NOTES: [&str; 4] = [
    "A p-value is the probability of data at least as extreme as observed under the null; \
     it is not the probability that the null is true.",
    "A p-value is not an effect size; report the estimate, its confidence interval, and the units.",
    "Statistical significance is not practical significance.",
    "Intervals depend on the sampling design, the model, and the missing-data handling.",
];

const PLANNING_NOTES: [&str; 3] = [
    "Sample-size and power results are based on the assumed effect size and a normal approximation.",
    "The achieved power will differ if the assumptions used for planning are wrong.",
    "Round the sample size up and add an allowance for attrition and non-compliance.",
];

const DESCRIPTIVE_NOTES: [&str; 3] = [
    "Descriptive statistics describe the sample; population statements require a sampling design.",
    "Correlation measures linear association and does not establish causation.",
    "The mean is sensitive to outliers and skew; report the median or quantiles as well.",
];

const REGRESSION_NOTES: [&str; 3] = [
    "Least-squares estimates are sensitive to outliers, leverage, and collinearity.",
    "In-sample fit is not out-of-sample predictive accuracy.",
    "Check residual structure, rank, and the condition number before interpreting coefficients.",
];

const ROOT_FINDING_NOTES: [&str; 3] = [
    "A located root is one root in the bracket, not all roots.",
    "Non-convergence is reported as an error; the last iterate is never returned as a success.",
    "Inspect the residual and the bracket before using the root.",
];

const UNITS_NOTES: [&str; 3] = [
    "Unit conversion preserves the physical quantity; only the representation changes.",
    "Choose the rounding policy and significant digits explicitly.",
    "Temperature differences and absolute temperatures follow different rules.",
];

const MONEY_NOTES: [&str; 3] = [
    "Money arithmetic rounds to the currency scale; allocation remainders must be assigned explicitly.",
    "Amounts in different currencies must be converted explicitly before they are combined.",
    "Money results are exact decimals under the declared currency scale; do not compare across currencies.",
];

const DISTRIBUTION_NOTES: [&str; 2] = [
    "Distribution functions assume the stated distribution and parameterization.",
    "Tail probabilities are approximations; verify the parameterization before comparing values.",
];

const UNKNOWN_NOTES: [&str; 2] = [
    "No family-specific interpretation notes are registered for this id.",
    "Treat the output as a computational result that still needs domain review.",
];

const GENERAL_NOTES: [&str; 2] = [
    "These notes are generic interpretation guidance, not a substitute for domain or statistical review.",
    "The notes are fixed rules keyed by function family; no language model is used.",
];

fn base_plan(task: Task) -> Plan {
    match task {
        Task::EstimateMean => plan(
            &[
                "statistics.ci_mean",
                "statistics.mean",
                "statistics.summary",
            ],
            &["values", "confidence_level"],
            &[
                "observations are independent",
                "the sample is representative of the target population",
                "the t interval assumes the mean is approximately normally distributed or the sample is large",
            ],
            &[
                "inspect the distribution for skew and outliers",
                "report the confidence interval together with the point estimate",
                "state whether the variance is treated as known or estimated",
            ],
            &[
                (
                    "statistics.median",
                    "the distribution is skewed or outliers dominate the mean",
                ),
                (
                    "statistics.summary",
                    "one call should return count, center, spread, and quantiles",
                ),
            ],
            &[
                "a confidence interval describes the mean under the sampling design, not the spread of individuals",
                "with a small sample, check approximate normality before trusting the t interval",
            ],
        ),
        Task::CompareMeans => plan(
            &[
                "statistics.welch_ci",
                "statistics.sample_size_two_means",
                "statistics.power_two_means",
            ],
            &[
                "group_a_values",
                "group_b_values",
                "confidence_level",
                "effect_size",
                "alpha",
                "power",
            ],
            &[
                "observations are independent within and between groups",
                "group membership is defined before outcomes are observed",
                "the Welch interval does not require equal variances",
            ],
            &[
                "report the difference of means with its confidence interval",
                "check group sizes and variances for large imbalance",
                "report a standardized effect size in addition to the raw difference",
            ],
            &[
                ("statistics.ci_mean", "each group mean is needed separately"),
                (
                    "statistics.sample_size_two_means",
                    "the question is how many observations are needed",
                ),
            ],
            &[
                "overlapping group intervals do not imply the difference is not significant; use the difference interval",
                "many pairwise comparisons inflate false positives; pre-specify comparisons and control multiplicity",
                "a dedicated t-test or one-way ANOVA is not registered in this build; use the Welch interval for the difference",
            ],
        ),
        Task::CompareProportions => plan(
            &[
                "statistics.proportions_difference",
                "statistics.ci_proportion",
                "statistics.sample_size_two_proportions",
                "statistics.power_two_proportions",
            ],
            &[
                "group_a_successes",
                "group_a_trials",
                "group_b_successes",
                "group_b_trials",
                "confidence_level",
                "baseline_rate",
                "minimum_detectable_effect",
                "alpha",
                "power",
            ],
            &[
                "independent Bernoulli trials within each group",
                "the groups are independent",
                "the normal approximation needs enough expected successes and failures per group",
            ],
            &[
                "report the absolute difference of proportions with its interval",
                "check the number of successes and failures in each group",
                "report each group rate with its own interval",
            ],
            &[
                (
                    "statistics.ci_proportion",
                    "the goal is a single group rate",
                ),
                (
                    "statistics.chi_square_contingency",
                    "there are more than two groups or more than two outcome categories",
                ),
            ],
            &[
                "a difference of proportions is scale-dependent; report absolute and relative effects",
                "Wald intervals can leave [0, 1] near the boundaries; the default Wilson and Newcombe methods are safer",
                "there is no dedicated two-proportion hypothesis test in this build; intervals and the chi-square test cover the comparison",
            ],
        ),
        Task::EstimateProportion => plan(
            &["statistics.ci_proportion"],
            &["successes", "trials", "confidence_level"],
            &[
                "independent Bernoulli trials",
                "a fixed number of trials or a random sample",
            ],
            &[
                "prefer the Wilson score interval for small samples or extreme rates",
                "report the raw count and denominator with the estimate",
            ],
            &[
                (
                    "statistics.proportions_difference",
                    "two groups are being compared",
                ),
                (
                    "statistics.sample_size_two_proportions",
                    "sample size for a planned two-group comparison",
                ),
            ],
            &[
                "the Wald interval can be invalid near 0 or 1; Wilson is the default",
                "a proportion without its denominator is not interpretable",
            ],
        ),
        Task::Correlation => plan(
            &[
                "statistics.correlation",
                "statistics.covariance",
                "statistics.summary",
            ],
            &["xs", "ys"],
            &[
                "pairs are independent",
                "the association is linear for the Pearson correlation",
                "no extreme leverage point drives the result",
            ],
            &[
                "inspect a scatterplot or quantile summaries before interpreting r",
                "check for outliers and non-linearity",
                "report the sample size because r is unstable in small samples",
            ],
            &[
                (
                    "statistics.covariance",
                    "the scale-dependent joint variation is of interest",
                ),
                (
                    "linear_algebra.least_squares",
                    "a predictive linear model is wanted",
                ),
            ],
            &[
                "correlation is not causation",
                "r = 0 does not rule out non-linear dependence",
                "the correlation is undefined when either series has zero variance",
            ],
        ),
        Task::Regression => plan(
            &[
                "linear_algebra.least_squares",
                "statistics.correlation",
                "statistics.covariance",
            ],
            &["design_matrix", "response", "model_specification"],
            &[
                "the mean response is linear in the parameters",
                "errors are independent with constant variance",
                "the design matrix has full column rank",
            ],
            &[
                "inspect the residual norm and the residual structure",
                "check the rank and condition number of the design matrix",
                "avoid extrapolating beyond the observed predictor range",
            ],
            &[
                (
                    "statistics.correlation",
                    "a single association summary is enough",
                ),
                (
                    "scientific.root_find",
                    "solving for a predictor value that reaches a target response",
                ),
            ],
            &[
                "least squares is sensitive to outliers and leverage",
                "a dedicated OLS or logistic regression function is not registered in this build; only QR least squares is available",
                "fitting many models and selecting the best inflates apparent performance",
            ],
        ),
        Task::ExperimentDesign => plan(
            &[
                "statistics.sample_size_two_means",
                "statistics.sample_size_two_proportions",
                "statistics.power_two_means",
                "statistics.power_two_proportions",
                "statistics.stratified_experiment",
                "statistics.prospective_pool",
            ],
            &[
                "endpoint_type",
                "baseline_rate_or_standard_deviation",
                "minimum_detectable_effect",
                "alpha",
                "power",
                "allocation_ratio",
                "strata_and_target_mix",
            ],
            &[
                "the primary endpoint and analysis are fixed before data collection",
                "the effect size used for planning is plausible and justified",
                "observations are independent and there is no interference between units",
            ],
            &[
                "compute the required sample size before looking at outcome data",
                "document the analysis plan, exclusions, and the primary comparison",
                "define the analysis population and the missing-data handling in advance",
            ],
            &[
                (
                    "statistics.stratified_experiment",
                    "the design is stratified and a target mix must be reported",
                ),
                (
                    "statistics.prospective_pool",
                    "observed strata may be pooled with planned strata",
                ),
            ],
            &[
                "do not peek at outcome data and stop early; fixed-horizon p-values and intervals are invalid under optional stopping",
                "if sequential monitoring is required, pre-specify an alpha-spending or group-sequential rule",
                "planning inputs are assumptions, not guarantees; the power functions use normal approximations",
            ],
        ),
        Task::BreakEven => plan(
            &[
                "finance.break_even",
                "finance.contribution",
                "finance.margin",
                "finance.markup",
            ],
            &["fixed_costs", "unit_price", "unit_variable_cost"],
            &[
                "costs split into fixed and variable components",
                "unit price and unit variable cost are constant over the relevant range",
                "production equals sales, so inventory does not absorb cost",
            ],
            &[
                "check that the contribution margin is positive before solving",
                "state the period and the currency",
                "sensitivity-test price and variable cost",
            ],
            &[
                (
                    "finance.contribution",
                    "the contribution margin itself is the quantity of interest",
                ),
                (
                    "finance.npv",
                    "the decision spans multiple periods with a discount rate",
                ),
            ],
            &[
                "break-even is a single-period, linear-cost view",
                "all monetary inputs must use the same currency",
            ],
        ),
        Task::CashflowAnalysis => plan(
            &[
                "finance.npv",
                "finance.irr",
                "finance.xnpv",
                "finance.xirr",
                "finance.present_value",
            ],
            &["cash_flows", "discount_rate"],
            &[
                "cash flows are known or scenario-based projections",
                "the discount rate reflects the opportunity cost of capital and risk",
                "periods are evenly spaced for npv/irr and dates are explicit for xnpv/xirr",
            ],
            &[
                "tabulate or plot the NPV profile over a range of discount rates",
                "count the sign changes in the cash-flow series",
                "compare NPV and IRR rankings when projects differ in scale or timing",
            ],
            &[
                (
                    "finance.xnpv",
                    "cash flows are dated rather than evenly spaced",
                ),
                (
                    "finance.present_value",
                    "a single future amount is being discounted",
                ),
            ],
            &[
                "projections are not observations",
                "IRR may be non-unique or may not exist when cash-flow signs change more than once",
                "IRR assumes reinvestment at the IRR, which may be unrealistic",
            ],
        ),
        Task::UnitConversion => plan(
            &["units.convert", "units.describe_unit", "units.list_units"],
            &["quantity", "target_unit"],
            &[
                "the source and target units share the same dimension",
                "the unit registry version is fixed for reproducibility",
            ],
            &[
                "check the dimension before converting",
                "state the rounding policy and significant digits",
                "verify whether a temperature is absolute or a difference",
            ],
            &[
                (
                    "units.describe_unit",
                    "the unit definition or notes are needed",
                ),
                (
                    "units.temperature_difference",
                    "the quantity is a temperature difference rather than an absolute temperature",
                ),
            ],
            &[
                "conversion changes the representation, not the underlying quantity",
                "rounding and display precision must be chosen explicitly",
            ],
        ),
        Task::RootFinding => plan(
            &["scientific.root_find"],
            &["expression", "variable", "lower", "upper"],
            &[
                "the function is continuous on the bracket",
                "the bracket endpoints have opposite signs or one endpoint is a root",
                "the expression uses the restricted expression grammar",
            ],
            &[
                "check the residual and the bracket reported with the root",
                "verify the root is inside the intended interval",
                "split or widen the bracket when the search fails to converge",
            ],
            &[(
                "scientific.root_find",
                "a different bracket is needed to locate another root",
            )],
            &[
                "a located root is one root, not all roots",
                "non-convergence is reported as an error; the last iterate is never returned as a result",
                "bracket selection determines which root is found",
            ],
        ),
        Task::Optimization => plan(
            &["linear_algebra.least_squares", "scientific.root_find"],
            &["objective", "variables", "constraints"],
            &[
                "the objective is well-defined and the feasible region is non-empty",
                "for least squares, the design matrix has full column rank",
                "for stationary points, the objective is differentiable",
            ],
            &[
                "check residuals, rank, and condition number for least-squares fits",
                "verify candidate optima against the constraints",
                "test sensitivity to starting values or brackets",
            ],
            &[
                (
                    "scientific.root_find",
                    "stationary points where the derivative is zero",
                ),
                (
                    "linear_algebra.solve",
                    "equality-constrained linear systems",
                ),
            ],
            &[
                "a general linear-programming solver is not registered in this build",
                "local methods give local answers; a stationary point need not be a global optimum",
                "convexity must be established separately for global guarantees",
            ],
        ),
        Task::TimeSeries => plan(
            &[
                "statistics.correlation",
                "statistics.covariance",
                "statistics.summary",
                "finance.percentage_change",
            ],
            &["series", "lag"],
            &[
                "observations are ordered and equally spaced",
                "the process is approximately stationary over the window analyzed",
                "a lagged correlation uses fewer independent pairs than the raw series length",
            ],
            &[
                "summarize the series and its changes over time",
                "check for trend, seasonality, and structural breaks",
                "use the reduced effective sample size when correlating lagged values",
            ],
            &[
                ("finance.xnpv", "the series is a dated cash flow"),
                ("statistics.correlation", "autocorrelation at a chosen lag"),
            ],
            &[
                "autocorrelation in a series violates the independence assumption of many tests",
                "a dedicated time-series model such as ARIMA or spectral analysis is not registered in this build",
                "correlation between series does not establish a leading or causal relationship",
            ],
        ),
        Task::CausalEffect => plan(
            &[
                "statistics.stratified_experiment",
                "statistics.welch_ci",
                "statistics.proportions_difference",
                "statistics.chi_square_contingency",
                "statistics.prospective_pool",
            ],
            &["treatment", "outcome", "covariates", "strata", "target_mix"],
            &[
                "treatment assignment is randomized, or ignorable given measured covariates",
                "no interference between units",
                "positivity: every unit has a non-zero probability of each treatment",
                "covariates are measured before treatment and are unaffected by it",
            ],
            &[
                "check covariate balance between arms before analyzing outcomes",
                "pre-specify the estimand and the adjustment set",
                "report unadjusted and adjusted estimates with intervals",
            ],
            &[
                (
                    "statistics.chi_square_contingency",
                    "the outcome is categorical",
                ),
                (
                    "statistics.proportions_difference",
                    "the outcome is binary and unadjusted",
                ),
            ],
            &[
                "standardization over a target mix is not the same as pooling raw rates",
                "observational adjustment cannot remove unmeasured confounding",
                "do not condition on post-treatment variables",
            ],
        ),
    }
}

fn apply_shape(plan: &mut Plan, task: Task, shape: DataShape) {
    match (task, shape) {
        (Task::EstimateMean, DataShape::TwoIndependent) => {
            set_functions(
                plan,
                &[
                    "statistics.welch_ci",
                    "statistics.ci_mean",
                    "statistics.summary",
                ],
            );
            plan.required_inputs = vec![
                "group_a_values".to_string(),
                "group_b_values".to_string(),
                "confidence_level".to_string(),
            ];
            push_all(
                &mut plan.assumptions,
                &["observations are independent within and between groups"],
            );
            push_all(
                &mut plan.diagnostics,
                &["report the difference of means with its confidence interval"],
            );
            push_all(
                &mut plan.caveats,
                &[
                    "do not pool the groups; estimate the difference from the two independent samples",
                ],
            );
        }
        (Task::EstimateMean, DataShape::TwoPaired) => {
            set_functions(plan, &["statistics.ci_mean"]);
            plan.required_inputs = vec![
                "paired_differences".to_string(),
                "confidence_level".to_string(),
            ];
            push_all(
                &mut plan.assumptions,
                &["pairs are independent and the pairing is by design"],
            );
            push_all(
                &mut plan.diagnostics,
                &["analyze the within-pair differences"],
            );
            push_all(
                &mut plan.caveats,
                &[
                    "a dedicated paired test is not registered in this build; use the interval for the mean difference",
                ],
            );
        }
        (Task::EstimateMean, DataShape::ManyGroups) => {
            set_functions(plan, &["statistics.summary", "statistics.ci_mean"]);
            plan.required_inputs = vec![
                "values".to_string(),
                "group".to_string(),
                "confidence_level".to_string(),
            ];
            push_all(
                &mut plan.diagnostics,
                &["summarize each group and inspect the group sizes"],
            );
            push_all(
                &mut plan.caveats,
                &[
                    "a dedicated one-way ANOVA is not registered in this build; pairwise intervals require multiplicity control",
                ],
            );
        }
        (Task::EstimateMean, DataShape::Bivariate | DataShape::Multivariate) => {
            set_functions(plan, &["statistics.ci_mean", "statistics.summary"]);
            push_all(&mut plan.required_inputs, &["variables"]);
            push_all(
                &mut plan.caveats,
                &[
                    "testing many variables inflates false positives; pre-specify the primary variables",
                ],
            );
        }
        (Task::CompareMeans, DataShape::TwoPaired) => {
            set_functions(plan, &["statistics.ci_mean"]);
            plan.required_inputs = vec![
                "paired_differences".to_string(),
                "confidence_level".to_string(),
            ];
            push_all(
                &mut plan.assumptions,
                &["pairs are independent and the pairing is by design"],
            );
            push_all(
                &mut plan.caveats,
                &[
                    "a dedicated paired test is not registered in this build; use the interval for the mean difference",
                ],
            );
        }
        (Task::CompareMeans, DataShape::ManyGroups) => {
            set_functions(plan, &["statistics.summary", "statistics.welch_ci"]);
            push_all(&mut plan.required_inputs, &["group"]);
            push_all(
                &mut plan.caveats,
                &[
                    "a dedicated one-way ANOVA is not registered in this build; with many groups pre-specify pairwise contrasts and control multiplicity",
                ],
            );
        }
        (Task::CompareMeans, DataShape::OneSample) => {
            set_functions(plan, &["statistics.ci_mean"]);
            plan.required_inputs = vec![
                "values".to_string(),
                "reference_value".to_string(),
                "confidence_level".to_string(),
            ];
            push_all(
                &mut plan.diagnostics,
                &["compare the interval with the pre-specified reference value"],
            );
        }
        (Task::CompareProportions, DataShape::ManyGroups | DataShape::Multivariate) => {
            ensure_function(plan, "statistics.chi_square_contingency");
            push_all(&mut plan.required_inputs, &["group", "outcome_category"]);
            push_all(
                &mut plan.caveats,
                &[
                    "with more than two groups use the chi-square test and pre-specify the comparisons",
                ],
            );
        }
        (Task::EstimateProportion, DataShape::TwoIndependent | DataShape::ManyGroups) => {
            set_functions(
                plan,
                &[
                    "statistics.ci_proportion",
                    "statistics.proportions_difference",
                ],
            );
            push_all(&mut plan.required_inputs, &["group"]);
            if shape == DataShape::ManyGroups {
                ensure_function(plan, "statistics.chi_square_contingency");
                push_all(
                    &mut plan.caveats,
                    &[
                        "with more than two groups use the chi-square test and pre-specify the comparisons",
                    ],
                );
            }
        }
        (Task::Correlation, DataShape::Multivariate) => {
            push_all(
                &mut plan.caveats,
                &[
                    "with many variables, pre-specify the pairs to correlate and control multiplicity",
                ],
            );
        }
        (Task::Regression, DataShape::Multivariate) => {
            push_all(
                &mut plan.diagnostics,
                &["check for collinearity among predictors"],
            );
            push_all(
                &mut plan.caveats,
                &[
                    "model selection driven by the data inflates apparent performance; pre-specify the model",
                ],
            );
        }
        (Task::Optimization, DataShape::Multivariate) => {
            push_all(
                &mut plan.diagnostics,
                &["state the full constraint set and check feasibility"],
            );
        }
        (Task::TimeSeries, DataShape::Multivariate) => {
            push_all(
                &mut plan.caveats,
                &["with several series, pre-specify the lead-lag relationships to test"],
            );
        }
        (Task::CausalEffect, DataShape::OneSample | DataShape::None) => {
            push_all(
                &mut plan.caveats,
                &[
                    "an effect estimate needs a comparison group or an explicit counterfactual model",
                ],
            );
        }
        _ => {}
    }
}

fn apply_goal(plan: &mut Plan, task: Task, goal: Goal) {
    let statistical = matches!(
        task,
        Task::EstimateMean
            | Task::CompareMeans
            | Task::CompareProportions
            | Task::EstimateProportion
            | Task::Correlation
            | Task::Regression
            | Task::ExperimentDesign
            | Task::TimeSeries
            | Task::CausalEffect
    );
    let decision = matches!(
        task,
        Task::BreakEven
            | Task::CashflowAnalysis
            | Task::Optimization
            | Task::ExperimentDesign
            | Task::CausalEffect
    );
    match goal {
        Goal::Estimate => {}
        Goal::Test if statistical => {
            push_all(
                &mut plan.diagnostics,
                &["pre-specify the null, the test, and the significance threshold"],
            );
            push_all(
                &mut plan.caveats,
                &[
                    "a p-value is not an effect size; report the estimate and its confidence interval",
                ],
            );
        }
        Goal::Predict
            if matches!(
                task,
                Task::Regression | Task::Correlation | Task::TimeSeries
            ) =>
        {
            ensure_function(plan, "linear_algebra.least_squares");
            push_all(
                &mut plan.diagnostics,
                &[
                    "validate predictions on held-out data; in-sample fit is not predictive accuracy",
                ],
            );
            push_all(
                &mut plan.caveats,
                &["predictions are conditional on the model and the observed predictor range"],
            );
        }
        Goal::Decide if decision => {
            push_all(
                &mut plan.diagnostics,
                &["state the decision rule, the loss function, and the cost of each error"],
            );
            push_all(
                &mut plan.caveats,
                &["a statistical result is an input to a decision, not the decision itself"],
            );
        }
        _ => {}
    }
}

fn build_plan(task: Task, shape: DataShape, goal: Goal, randomized: bool) -> Plan {
    let mut plan = base_plan(task);
    apply_shape(&mut plan, task, shape);
    apply_goal(&mut plan, task, goal);
    finalize_plan(&mut plan, task, shape, goal, randomized);
    plan
}

// ---------------------------------------------------------------------------
// Field analysis rules
// ---------------------------------------------------------------------------

fn field_suggestions(kind: FieldKind) -> (&'static [&'static str], &'static [&'static str]) {
    match kind {
        FieldKind::Count => (
            &[
                "statistics.count",
                "statistics.mean",
                "statistics.summary",
                "statistics.variance",
                "statistics.quantile",
            ],
            &[
                "counts are non-negative integers; check for overdispersion before treating them as continuous",
            ],
        ),
        FieldKind::Continuous => (
            &[
                "statistics.summary",
                "statistics.mean",
                "statistics.median",
                "statistics.stddev",
                "statistics.ci_mean",
                "statistics.quantile",
            ],
            &["inspect skew and outliers; the mean and the median answer different questions"],
        ),
        FieldKind::Categorical => (
            &[
                "statistics.mode",
                "statistics.count",
                "statistics.ci_proportion",
            ],
            &[
                "report counts per category with the denominator; binary categories can use proportion intervals",
            ],
        ),
        FieldKind::Money => (
            &[
                "finance.money_add",
                "finance.money_allocate",
                "finance.percentage_change",
            ],
            &["keep one currency per field; money arithmetic rounds to the currency scale"],
        ),
        FieldKind::Date => (
            &["units.convert", "finance.xnpv", "finance.xirr"],
            &[
                "there is no calendar arithmetic function; convert dates to elapsed periods and use dated cash-flow methods for valuation",
            ],
        ),
        FieldKind::Text => (
            &["statistics.count", "statistics.mode"],
            &[
                "encode free text as explicit categories before analysis; there is no text-analysis function",
            ],
        ),
    }
}

fn combination(
    left: FieldKind,
    right: FieldKind,
) -> Option<(&'static [&'static str], &'static str)> {
    let (a, b) = if left.rank() <= right.rank() {
        (left, right)
    } else {
        (right, left)
    };
    match (a, b) {
        (FieldKind::Count, FieldKind::Count) => Some((
            &["statistics.correlation", "statistics.covariance"],
            "joint variation of two count variables; check for overdispersion",
        )),
        (FieldKind::Continuous, FieldKind::Continuous) => Some((
            &[
                "statistics.correlation",
                "statistics.covariance",
                "linear_algebra.least_squares",
            ],
            "both fields are numeric and a joint or predictive relationship is of interest",
        )),
        (FieldKind::Count, FieldKind::Continuous) => Some((
            &[
                "statistics.correlation",
                "statistics.covariance",
                "linear_algebra.least_squares",
            ],
            "modeling a count against a numeric field; verify the count model",
        )),
        (FieldKind::Money, FieldKind::Money) => Some((
            &[
                "finance.money_add",
                "finance.money_compare",
                "finance.percentage_change",
            ],
            "aggregating or comparing monetary amounts in one currency",
        )),
        (FieldKind::Count, FieldKind::Money) | (FieldKind::Continuous, FieldKind::Money) => Some((
            &["statistics.correlation", "finance.percentage_change"],
            "relating a numeric field to a monetary amount; convert the amount explicitly",
        )),
        (FieldKind::Categorical, FieldKind::Categorical)
        | (FieldKind::Categorical, FieldKind::Text)
        | (FieldKind::Text, FieldKind::Text) => Some((
            &[
                "statistics.chi_square_contingency",
                "statistics.proportions_difference",
            ],
            "association between two categorical fields after any text is encoded",
        )),
        (FieldKind::Categorical, FieldKind::Count)
        | (FieldKind::Categorical, FieldKind::Continuous) => Some((
            &[
                "statistics.welch_ci",
                "statistics.ci_mean",
                "statistics.summary",
            ],
            "comparing a numeric outcome across groups",
        )),
        (FieldKind::Categorical, FieldKind::Money) => Some((
            &["finance.money_add", "finance.percentage_change"],
            "comparing monetary totals across groups in one currency",
        )),
        (FieldKind::Count, FieldKind::Text) | (FieldKind::Continuous, FieldKind::Text) => Some((
            &["statistics.summary", "statistics.welch_ci"],
            "summarizing a numeric outcome by text category after encoding",
        )),
        (FieldKind::Money, FieldKind::Text) => Some((
            &["finance.money_add"],
            "summarizing monetary totals by text category after encoding",
        )),
        (FieldKind::Date, FieldKind::Date) => Some((
            &["statistics.correlation", "statistics.covariance"],
            "association between two date fields after conversion to elapsed periods",
        )),
        (FieldKind::Count, FieldKind::Date) | (FieldKind::Continuous, FieldKind::Date) => Some((
            &["statistics.correlation", "statistics.covariance"],
            "trend over time after converting dates to elapsed periods",
        )),
        (FieldKind::Categorical, FieldKind::Date) | (FieldKind::Date, FieldKind::Text) => Some((
            &["statistics.count"],
            "date frequency by group; there is no calendar or survival function in this build",
        )),
        (FieldKind::Date, FieldKind::Money) => Some((
            &["finance.xnpv", "finance.xirr", "finance.npv"],
            "valuing a dated cash-flow stream",
        )),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Value helpers
// ---------------------------------------------------------------------------

fn string_array(values: &[String]) -> Value {
    Value::Array(values.iter().cloned().map(Value::text).collect())
}

fn str_array(values: &[&str]) -> Value {
    Value::Array(values.iter().map(|item| Value::text(*item)).collect())
}

fn enum_schema(variants: &[&str]) -> ValueSchema {
    ValueSchema::Enum {
        variants: variants.iter().map(|variant| variant.to_string()).collect(),
    }
}

fn text_array_schema() -> ValueSchema {
    ValueSchema::array(ValueSchema::text())
}

fn any_number_schema() -> ValueSchema {
    ValueSchema::number(NumberKind::Any)
}

fn integer_schema() -> ValueSchema {
    ValueSchema::number(NumberKind::Integer)
}

fn all_modes() -> Vec<NumericMode> {
    vec![
        NumericMode::Exact,
        NumericMode::Auto,
        NumericMode::Scientific,
    ]
}

fn example_args(pairs: &[(&str, serde_json::Value)]) -> BTreeMap<String, Value> {
    pairs
        .iter()
        .map(|(name, raw)| (name.to_string(), parse_value(raw.clone())))
        .collect()
}

fn parse_value(raw: serde_json::Value) -> Value {
    serde_json::from_value(raw).expect("example value must be valid")
}

// ---------------------------------------------------------------------------
// Output schemas
// ---------------------------------------------------------------------------

fn plan_schema() -> ValueSchema {
    ValueSchema::Record {
        fields: vec![
            FieldSchema::required("recommended_functions", text_array_schema())
                .with_description("Registered function ids in recommendation order."),
            FieldSchema::required(
                "why_eligible",
                ValueSchema::array(ValueSchema::Record {
                    fields: vec![
                        FieldSchema::required("function", ValueSchema::text())
                            .with_description("Recommended registered function id."),
                        FieldSchema::required("reason", ValueSchema::text())
                            .with_description("Deterministic reason the function is eligible."),
                    ],
                    allow_extra: false,
                }),
            )
            .with_description(
                "One {function, reason} record per recommended function, in the same order.",
            ),
            FieldSchema::required("required_inputs", text_array_schema())
                .with_description("Inputs the recommended methods need."),
            FieldSchema::required("assumptions", text_array_schema())
                .with_description("Assumptions the recommendation relies on."),
            FieldSchema::required("diagnostics", text_array_schema())
                .with_description("Checks to run before trusting the result."),
            FieldSchema::required(
                "alternatives",
                ValueSchema::array(ValueSchema::Record {
                    fields: vec![
                        FieldSchema::required("function", ValueSchema::text())
                            .with_description("Alternative registered function id."),
                        FieldSchema::required("when", ValueSchema::text())
                            .with_description("Situation in which the alternative is preferable."),
                    ],
                    allow_extra: false,
                }),
            )
            .with_description("Alternative methods and when to prefer them."),
            FieldSchema::required("caveats", text_array_schema())
                .with_description("Limitations and warnings."),
            FieldSchema::required("unknowns", text_array_schema()).with_description(
                "Fixed-vocabulary items the supplied inputs do not establish; never inferred.",
            ),
        ],
        allow_extra: false,
    }
}

fn checklist_schema() -> ValueSchema {
    ValueSchema::Record {
        fields: vec![
            FieldSchema::required("design_kind", ValueSchema::text()),
            FieldSchema::required("sample_size_functions", text_array_schema()),
            FieldSchema::required("sample_size_inputs", text_array_schema()),
            FieldSchema::required("sample_size_notes", text_array_schema()),
            FieldSchema::required("pre_registration", text_array_schema()),
            FieldSchema::required("stopping_rules", text_array_schema()),
            FieldSchema::required("assumptions", text_array_schema()),
            FieldSchema::required("unknowns", text_array_schema()).with_description(
                "Evidence the supplied planning inputs do not establish; never inferred.",
            ),
            FieldSchema::required("not_established", text_array_schema())
                .with_description("What this checklist does not guarantee."),
            FieldSchema::required(
                "provided",
                ValueSchema::Record {
                    fields: vec![
                        FieldSchema::optional("baseline_rate", any_number_schema()),
                        FieldSchema::optional("minimum_detectable_effect", any_number_schema()),
                        FieldSchema::optional("alpha", any_number_schema()),
                        FieldSchema::optional("power", any_number_schema()),
                    ],
                    allow_extra: false,
                },
            ),
        ],
        allow_extra: false,
    }
}

fn describe_fields_output_schema() -> ValueSchema {
    let described = ValueSchema::Record {
        fields: vec![
            FieldSchema::required("name", ValueSchema::text()),
            FieldSchema::required("type", enum_schema(&FIELD_KIND_VARIANTS)),
            FieldSchema::optional("n", integer_schema()),
            FieldSchema::required("suggested_analyses", text_array_schema()),
            FieldSchema::required("notes", text_array_schema()),
        ],
        allow_extra: false,
    };
    let combination = ValueSchema::Record {
        fields: vec![
            FieldSchema::required("fields", text_array_schema()),
            FieldSchema::required("analyses", text_array_schema()),
            FieldSchema::required("when", ValueSchema::text()),
        ],
        allow_extra: false,
    };
    ValueSchema::Record {
        fields: vec![
            FieldSchema::required("fields", ValueSchema::array(described)),
            FieldSchema::required("combinations", ValueSchema::array(combination)),
            FieldSchema::required("caveats", text_array_schema()),
            FieldSchema::required("unknowns", text_array_schema())
                .with_description("Data-quality evidence the field descriptions do not establish."),
        ],
        allow_extra: false,
    }
}

fn interpretation_notes_schema() -> ValueSchema {
    ValueSchema::Record {
        fields: vec![
            FieldSchema::required(
                "notes",
                ValueSchema::array(ValueSchema::Record {
                    fields: vec![
                        FieldSchema::required("function", ValueSchema::text()),
                        FieldSchema::required("family", ValueSchema::text()),
                        FieldSchema::required("notes", text_array_schema()),
                    ],
                    allow_extra: false,
                }),
            ),
            FieldSchema::required("general", text_array_schema()),
            FieldSchema::required("scope", ValueSchema::text())
                .with_description("What these notes do and do not establish."),
        ],
        allow_extra: false,
    }
}

// ---------------------------------------------------------------------------
// plan.recommend
// ---------------------------------------------------------------------------

fn recommend_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "plan.recommend",
        "plan",
        "1.0.0",
        "Recommend methods",
        "Rule-based method recommendations for a task, data shape, and goal.",
    )
    .with_description(
        "Pure rule-based planning: the mapping from task, data shape, and goal to recommended \
         function ids is fixed and uses no model, network, or randomness. Recommendations name \
         only functions registered in this build; unavailable methods are called out in the \
         caveats.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("task", "Analysis task.", enum_schema(&TASK_VARIANTS)),
        ParamDescriptor::optional(
            "data_shape",
            "Shape of the data; default none.",
            enum_schema(&SHAPE_VARIANTS),
        ),
        ParamDescriptor::optional(
            "goal",
            "Analysis goal; default estimate.",
            enum_schema(&GOAL_VARIANTS),
        ),
    ])
    .with_output(plan_schema(), "Method recommendation record.")
    .with_modes(all_modes())
    .with_cost(CostClass::Constant)
    .with_method_ref("docs/methods/plan.md#recommend")
    .with_examples(vec![
        Example::new(
            "compare two proportions",
            example_args(&[
                ("task", serde_json::json!("compare_proportions")),
                ("data_shape", serde_json::json!("two_independent")),
                ("goal", serde_json::json!("test")),
            ]),
        )
        .with_contains("statistics.proportions_difference"),
        Example::new(
            "plan an experiment",
            example_args(&[("task", serde_json::json!("experiment_design"))]),
        )
        .with_contains("statistics.sample_size_two_means"),
        Example::new(
            "unknown task is rejected",
            example_args(&[("task", serde_json::json!("forecast"))]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_recommend(args: &Args, _ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let task = Task::parse(args.text("task")?)?;
    let shape = match args.optional_text("data_shape")? {
        Some(text) => DataShape::parse(text)?,
        None => DataShape::None,
    };
    let goal = match args.optional_text("goal")? {
        Some(text) => Goal::parse(text)?,
        None => Goal::Estimate,
    };
    let randomized = args.optional_bool("randomized")?.unwrap_or(false);
    Ok(Outcome::exact(
        build_plan(task, shape, goal, randomized).to_value(),
    ))
}

// ---------------------------------------------------------------------------
// plan.experiment_checklist
// ---------------------------------------------------------------------------

fn checklist_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "plan.experiment_checklist",
        "plan",
        "1.0.0",
        "Experiment checklist",
        "Sample-size guidance, pre-registration checklist, and stopping-rule warnings.",
    )
    .with_description(
        "Returns fixed planning guidance for an experiment kind. When planning inputs are \
         supplied they are range-checked and echoed; no sample size is computed here. The \
         checklist points at the registered sample-size and power functions and warns about \
         peeking and sequential testing.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "kind",
            "Experiment kind.",
            enum_schema(&EXPERIMENT_KIND_VARIANTS),
        ),
        ParamDescriptor::optional(
            "baseline_rate",
            "Baseline event rate in [0, 1], when known.",
            any_number_schema(),
        ),
        ParamDescriptor::optional(
            "minimum_detectable_effect",
            "Smallest effect worth detecting; must be positive.",
            any_number_schema(),
        ),
        ParamDescriptor::optional(
            "alpha",
            "Significance threshold in (0, 1).",
            any_number_schema(),
        ),
        ParamDescriptor::optional("power", "Target power in (0, 1).", any_number_schema()),
    ])
    .with_output(
        checklist_schema(),
        "Checklist record with sample-size guidance and stopping rules.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Constant)
    .with_method_ref("docs/methods/plan.md#experiment_checklist")
    .with_examples(vec![
        Example::new(
            "two-proportion experiment",
            example_args(&[
                ("kind", serde_json::json!("two_proportions")),
                ("baseline_rate", serde_json::json!("0.20")),
                ("minimum_detectable_effect", serde_json::json!("0.05")),
                ("alpha", serde_json::json!("0.05")),
                ("power", serde_json::json!("0.80")),
            ]),
        )
        .with_contains("statistics.sample_size_two_proportions"),
        Example::new(
            "alpha outside the unit interval",
            example_args(&[
                ("kind", serde_json::json!("two_proportions")),
                ("alpha", serde_json::json!("1.5")),
            ]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn check_open_unit(name: &str, number: &Number) -> Result<(), EngineError> {
    let value = number
        .to_f64()
        .ok_or_else(|| EngineError::domain(format!("{name} must be a finite number")))?;
    if value <= 0.0 || value >= 1.0 {
        return Err(EngineError::domain(format!(
            "{name} must lie strictly between 0 and 1"
        )));
    }
    Ok(())
}

fn check_closed_unit(name: &str, number: &Number) -> Result<(), EngineError> {
    let value = number
        .to_f64()
        .ok_or_else(|| EngineError::domain(format!("{name} must be a finite number")))?;
    if !(0.0..=1.0).contains(&value) {
        return Err(EngineError::domain(format!("{name} must lie in [0, 1]")));
    }
    Ok(())
}

fn check_positive(name: &str, number: &Number) -> Result<(), EngineError> {
    if number.is_negative() || number.is_zero() {
        return Err(EngineError::domain(format!("{name} must be positive")));
    }
    Ok(())
}

/// Fixed-vocabulary unknowns for an experiment plan. Items are removed only
/// when the corresponding input was explicitly supplied; nothing is inferred.
fn checklist_unknowns(
    mde: Option<&Number>,
    alpha: Option<&Number>,
    power: Option<&Number>,
) -> Vec<&'static str> {
    let mut unknowns = vec![
        "randomization",
        "independence between units",
        "absence of interference between units",
        "comparability of groups at baseline",
        "outcome completeness",
        "missing-data mechanism",
        "attrition and non-compliance",
    ];
    if mde.is_none() {
        unknowns.push("minimum detectable effect");
    }
    if alpha.is_none() {
        unknowns.push("significance threshold");
    }
    if power.is_none() {
        unknowns.push("target power");
    }
    unknowns
}

fn invoke_experiment_checklist(args: &Args, _ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let kind = ExperimentKind::parse(args.text("kind")?)?;
    let baseline_rate = args.optional_number("baseline_rate")?;
    let mde = args.optional_number("minimum_detectable_effect")?;
    let alpha = args.optional_number("alpha")?;
    let power = args.optional_number("power")?;
    if let Some(rate) = baseline_rate {
        check_closed_unit("baseline_rate", rate)?;
    }
    if let Some(value) = mde {
        check_positive("minimum_detectable_effect", value)?;
    }
    if let Some(value) = alpha {
        check_open_unit("alpha", value)?;
    }
    if let Some(value) = power {
        check_open_unit("power", value)?;
    }
    let mut provided = BTreeMap::new();
    if let Some(rate) = baseline_rate {
        provided.insert("baseline_rate".to_string(), Value::Number(rate.clone()));
    }
    if let Some(value) = mde {
        provided.insert(
            "minimum_detectable_effect".to_string(),
            Value::Number(value.clone()),
        );
    }
    if let Some(value) = alpha {
        provided.insert("alpha".to_string(), Value::Number(value.clone()));
    }
    if let Some(value) = power {
        provided.insert("power".to_string(), Value::Number(value.clone()));
    }
    let value = Value::record([
        ("design_kind", Value::text(kind.as_str())),
        (
            "sample_size_functions",
            str_array(kind.sample_size_functions()),
        ),
        ("sample_size_inputs", str_array(kind.sample_size_inputs())),
        ("sample_size_notes", str_array(&SAMPLE_SIZE_NOTES)),
        ("pre_registration", str_array(&PRE_REGISTRATION)),
        ("stopping_rules", str_array(&STOPPING_RULES)),
        ("assumptions", str_array(&CHECKLIST_ASSUMPTIONS)),
        (
            "unknowns",
            str_array(&checklist_unknowns(mde, alpha, power)),
        ),
        (
            "not_established",
            str_array(&[
                "this checklist does not verify that a plan was pre-registered or followed",
                "it does not establish randomization, independence, or absence of interference",
                "it does not validate the plausibility of the planning assumptions",
            ]),
        ),
        ("provided", Value::Record(provided)),
    ]);
    Ok(Outcome::exact(value))
}

// ---------------------------------------------------------------------------
// plan.describe_fields
// ---------------------------------------------------------------------------

fn field_input_schema() -> ValueSchema {
    ValueSchema::array_with_len(
        ValueSchema::Record {
            fields: vec![
                FieldSchema::required("name", ValueSchema::text()).with_description("Field name."),
                FieldSchema::required("type", enum_schema(&FIELD_KIND_VARIANTS)).with_description(
                    "Field type. The wire format reserves the key \"kind\" for tagged \
                         values, so the type is carried under \"type\".",
                ),
                FieldSchema::optional("n", integer_schema())
                    .with_description("Number of observations, when known."),
            ],
            allow_extra: false,
        },
        1,
        None,
    )
}

fn describe_fields_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "plan.describe_fields",
        "plan",
        "1.0.0",
        "Describe fields",
        "Suggested analyses per field and per field combination, with data-quality caveats.",
    )
    .with_description(
        "Takes a list of field records with a name, a type, and an optional observation count \
         and returns fixed rule-based analysis suggestions for each field and for pairs of \
         fields, plus caveats about missingness and multiplicity. The field type is carried \
         under the \"type\" key because the wire format reserves \"kind\" for tagged values.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "fields",
        "Fields to describe.",
        field_input_schema(),
    )])
    .with_output(
        describe_fields_output_schema(),
        "Field-level analysis plan.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_method_ref("docs/methods/plan.md#describe_fields")
    .with_examples(vec![
        Example::new(
            "mixed field types",
            example_args(&[(
                "fields",
                serde_json::json!([
                    {"name": "region", "type": "categorical", "n": 240},
                    {"name": "channel", "type": "categorical", "n": 240},
                    {"name": "spend", "type": "continuous", "n": 240},
                    {"name": "revenue", "type": "money", "n": 240},
                    {"name": "signup_date", "type": "date", "n": 240}
                ]),
            )]),
        )
        .with_contains("statistics.chi_square_contingency"),
        Example::new(
            "negative observation count",
            example_args(&[(
                "fields",
                serde_json::json!([{"name": "age", "type": "continuous", "n": -1}]),
            )]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

struct DescribedField {
    name: String,
    kind: FieldKind,
    n: Option<Number>,
}

fn invoke_describe_fields(args: &Args, _ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let raw_fields = args.array("fields")?;
    let mut parsed = Vec::with_capacity(raw_fields.len());
    for (index, raw) in raw_fields.iter().enumerate() {
        let path = format!("fields[{index}]");
        let record = raw
            .as_record()
            .map_err(|error| error.with_path(path.clone()))?;
        let name = record
            .get("name")
            .ok_or_else(|| EngineError::malformed("missing field name").with_path(path.clone()))?
            .as_text()
            .map_err(|error| error.with_path(format!("{path}.name")))?
            .to_string();
        let kind = FieldKind::parse(
            record
                .get("type")
                .ok_or_else(|| {
                    EngineError::malformed("missing field type").with_path(path.clone())
                })?
                .as_text()
                .map_err(|error| error.with_path(format!("{path}.type")))?,
        )?;
        let n = match record.get("n") {
            None | Some(Value::Null) => None,
            Some(Value::Number(number)) => {
                if number.is_negative() {
                    return Err(EngineError::domain("n must be non-negative")
                        .with_path(format!("{path}.n")));
                }
                Some(number.clone())
            }
            Some(other) => {
                return Err(EngineError::malformed(format!(
                    "n must be an integer, found {}",
                    other.kind_name()
                ))
                .with_path(format!("{path}.n")));
            }
        };
        parsed.push(DescribedField { name, kind, n });
    }

    let mut field_values = Vec::with_capacity(parsed.len());
    let mut any_small_n = false;
    for field in &parsed {
        let (suggestions, notes) = field_suggestions(field.kind);
        if let Some(n) = &field.n
            && n.to_f64().is_some_and(|value| value < 2.0)
        {
            any_small_n = true;
        }
        let mut entries = vec![
            ("name", Value::text(field.name.as_str())),
            ("type", Value::text(field.kind.as_str())),
        ];
        if let Some(n) = &field.n {
            entries.push(("n", Value::Number(n.clone())));
        }
        entries.push(("suggested_analyses", str_array(suggestions)));
        entries.push(("notes", str_array(notes)));
        field_values.push(Value::record(entries));
    }

    let mut combinations = Vec::new();
    for i in 0..parsed.len() {
        for j in (i + 1)..parsed.len() {
            if let Some((analyses, when)) = combination(parsed[i].kind, parsed[j].kind) {
                combinations.push(Value::record([
                    (
                        "fields",
                        Value::Array(vec![
                            Value::text(parsed[i].name.as_str()),
                            Value::text(parsed[j].name.as_str()),
                        ]),
                    ),
                    ("analyses", str_array(analyses)),
                    ("when", Value::text(when)),
                ]));
            }
        }
    }

    let distinct_counts = {
        let mut counts: Vec<&Number> = Vec::new();
        for field in &parsed {
            if let Some(n) = &field.n
                && !counts.contains(&n)
            {
                counts.push(n);
            }
        }
        counts.len() > 1
    };

    let mut caveats: Vec<&str> = vec![
        "missing values are not imputed by the engine; decide and document a missing-data strategy before analysis",
        "complete-case analysis can bias estimates when missingness is related to the outcome; report how much data is missing per field",
        "testing many field pairs inflates the false-positive rate; pre-specify primary comparisons and adjust thresholds for multiplicity",
        "a multiple-testing adjustment function is not registered in this build; a Bonferroni bound or a pre-specified ordering are simple alternatives",
    ];
    if any_small_n {
        caveats.push("some fields have fewer than two observations; inference needs at least two and usually more");
    }
    if distinct_counts {
        caveats.push("fields have different observation counts; confirm they can be joined on a unit identifier before combining them");
    }

    let value = Value::record([
        ("fields", Value::Array(field_values)),
        ("combinations", Value::Array(combinations)),
        ("caveats", str_array(&caveats)),
        (
            "unknowns",
            str_array(&[
                "missingness mechanism",
                "data provenance and collection method",
                "measurement error",
                "selection into the dataset",
                "multiplicity across the suggested analyses",
            ]),
        ),
    ]);
    Ok(Outcome::exact(value))
}

// ---------------------------------------------------------------------------
// plan.interpretation_notes
// ---------------------------------------------------------------------------

fn interpretation_notes_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "plan.interpretation_notes",
        "plan",
        "1.0.0",
        "Interpretation notes",
        "Generic interpretation caveats for the families of the supplied function ids.",
    )
    .with_description(
        "Maps each supplied function id to a method family and returns fixed interpretation \
         caveats for that family: p-values are not effect sizes, IRR may be non-unique, pooled \
         rates do not replace standardization, projections are not observations, and so on. \
         Unknown ids receive a generic note rather than an error.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "functions",
        "Function ids to interpret.",
        ValueSchema::array_with_len(ValueSchema::text(), 1, None),
    )])
    .with_output(
        interpretation_notes_schema(),
        "Interpretation notes grouped by function.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_method_ref("docs/methods/plan.md#interpretation_notes")
    .with_examples(vec![
        Example::new(
            "IRR and standardization",
            example_args(&[(
                "functions",
                serde_json::json!(["finance.irr", "statistics.stratified_experiment"]),
            )]),
        )
        .with_contains("multiple real roots"),
        Example::new(
            "empty function list",
            example_args(&[("functions", serde_json::json!([]))]),
        )
        .with_error(ErrorCode::InsufficientObservations),
    ])
}

fn note_for(id: &str) -> Value {
    let family = Family::classify(id);
    Value::record([
        ("function", Value::text(id)),
        ("family", Value::text(family.as_str())),
        ("notes", str_array(family.notes())),
    ])
}

fn invoke_interpretation_notes(args: &Args, _ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let functions = args.array("functions")?;
    let mut notes = Vec::with_capacity(functions.len());
    for (index, item) in functions.iter().enumerate() {
        let id = item
            .as_text()
            .map_err(|error| error.with_path(format!("functions[{index}]")))?;
        notes.push(note_for(id));
    }
    let value = Value::record([
        ("notes", Value::Array(notes)),
        ("general", str_array(&GENERAL_NOTES)),
        (
            "scope",
            Value::text(
                "Guidance only: these notes describe how to read method families. They do not \
                 establish causal validity, future comparability, or business value for any \
                 specific dataset.",
            ),
        ),
    ]);
    Ok(Outcome::exact(value))
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

/// Build the plan module with all of its registered functions.
pub fn module() -> Module {
    let functions: Vec<Arc<dyn bicmath_core::contract::Function>> = vec![
        SimpleFunction::arc(recommend_descriptor(), invoke_recommend),
        SimpleFunction::arc(checklist_descriptor(), invoke_experiment_checklist),
        SimpleFunction::arc(describe_fields_descriptor(), invoke_describe_fields),
        SimpleFunction::arc(
            interpretation_notes_descriptor(),
            invoke_interpretation_notes,
        ),
    ];
    let descriptor = ModuleDescriptor::new(
        "plan",
        "Plan",
        "1.0.0",
        "Rule-based method recommendations, experiment checklists, field analysis plans, \
         and interpretation notes.",
    )
    .with_capabilities(vec![
        "method_recommendation",
        "experiment_planning",
        "field_analysis_planning",
        "interpretation_guidance",
    ])
    .with_dependencies(vec!["core"])
    .with_modes(all_modes())
    .with_source("crates/bicmath-plan");
    Module::new(descriptor, functions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bicmath_core::contract::ExampleExpectation;

    fn ctx() -> ExecContext {
        ExecContext::conservative()
    }

    fn try_call(id: &str, raw: &serde_json::Value) -> Result<Outcome, EngineError> {
        let module = module();
        let function = module
            .functions
            .iter()
            .find(|f| f.descriptor().id == id)
            .expect("function exists");
        let object = raw.as_object().expect("object args");
        let mut values = BTreeMap::new();
        for (name, value) in object {
            let param = function
                .descriptor()
                .parameter(name)
                .expect("parameter exists");
            let coerced =
                param
                    .schema
                    .coerce(value, name, &ctx().limits, param.numeric_shorthand)?;
            values.insert(name.clone(), coerced);
        }
        function.invoke(&Args::new(values), &ctx())
    }

    fn call(id: &str, raw: serde_json::Value) -> Outcome {
        try_call(id, &raw).expect("call succeeds")
    }

    fn field<'a>(value: &'a Value, key: &str) -> &'a Value {
        match value {
            Value::Record(fields) => fields
                .get(key)
                .unwrap_or_else(|| panic!("missing field {key}")),
            other => panic!("expected record, got {other:?}"),
        }
    }

    fn array_field<'a>(value: &'a Value, key: &str) -> &'a [Value] {
        field(value, key).as_array().expect("array field")
    }

    fn text_field<'a>(value: &'a Value, key: &str) -> &'a str {
        field(value, key).as_text().expect("text field")
    }

    fn contains_text(value: &Value, key: &str, needle: &str) -> bool {
        array_field(value, key)
            .iter()
            .any(|item| item.as_text().is_ok_and(|text| text == needle))
    }

    const KNOWN_FUNCTION_IDS: [&str; 41] = [
        "finance.break_even",
        "finance.contribution",
        "finance.irr",
        "finance.margin",
        "finance.markup",
        "finance.money_add",
        "finance.money_allocate",
        "finance.money_compare",
        "finance.npv",
        "finance.percentage_change",
        "finance.present_value",
        "finance.xirr",
        "finance.xnpv",
        "linear_algebra.least_squares",
        "linear_algebra.solve",
        "scientific.root_find",
        "statistics.chi_square_contingency",
        "statistics.ci_mean",
        "statistics.ci_proportion",
        "statistics.correlation",
        "statistics.count",
        "statistics.covariance",
        "statistics.mean",
        "statistics.median",
        "statistics.mode",
        "statistics.power_two_means",
        "statistics.power_two_proportions",
        "statistics.proportions_difference",
        "statistics.prospective_pool",
        "statistics.quantile",
        "statistics.sample_size_two_means",
        "statistics.sample_size_two_proportions",
        "statistics.stddev",
        "statistics.stratified_experiment",
        "statistics.summary",
        "statistics.variance",
        "statistics.welch_ci",
        "units.convert",
        "units.describe_unit",
        "units.list_units",
        "units.temperature_difference",
    ];

    fn check_example(descriptor: &FunctionDescriptor, example: &Example) -> Result<(), String> {
        let raw = serde_json::to_value(&example.arguments).map_err(|error| error.to_string())?;
        let outcome = try_call(&descriptor.id, &raw);
        match (&example.expected, outcome) {
            (Some(ExampleExpectation::Value(expected)), Ok(outcome)) => {
                if &outcome.value != expected {
                    return Err(format!(
                        "{} example {:?}: expected {expected:?}, got {:?}",
                        descriptor.id, example.title, outcome.value
                    ));
                }
            }
            (Some(ExampleExpectation::Error(expected)), Err(error)) => {
                if error.code != *expected {
                    return Err(format!(
                        "{} example {:?}: expected error {expected:?}, got {:?}",
                        descriptor.id, example.title, error.code
                    ));
                }
            }
            (Some(ExampleExpectation::Error(expected)), Ok(_)) => {
                return Err(format!(
                    "{} example {:?}: expected error {expected:?}, but the call succeeded",
                    descriptor.id, example.title
                ));
            }
            (Some(ExampleExpectation::Contains(needle)), Ok(outcome)) => {
                let rendered = serde_json::to_string(&outcome.value).unwrap_or_default();
                if !rendered.contains(needle) {
                    return Err(format!(
                        "{} example {:?}: result does not contain {needle:?}",
                        descriptor.id, example.title
                    ));
                }
            }
            (Some(ExampleExpectation::Contains(needle)), Err(error)) => {
                return Err(format!(
                    "{} example {:?}: expected a result containing {needle:?}, got {error}",
                    descriptor.id, example.title
                ));
            }
            (Some(ExampleExpectation::Value(_)), Err(error)) => {
                return Err(format!(
                    "{} example {:?}: expected a value, got {error}",
                    descriptor.id, example.title
                ));
            }
            (None, _) => {}
        }
        Ok(())
    }

    #[test]
    fn module_metadata_is_stable() {
        let module = module();
        assert_eq!(module.descriptor.id, "plan");
        assert_eq!(module.descriptor.version, "1.0.0");
        assert_eq!(module.functions.len(), 4);
    }

    #[test]
    fn every_function_has_an_example() {
        for function in module().functions {
            assert!(
                !function.descriptor().examples.is_empty(),
                "function {} has no examples",
                function.descriptor().id
            );
        }
    }

    #[test]
    fn all_examples_execute_as_documented() {
        for function in module().functions {
            for example in &function.descriptor().examples {
                check_example(function.descriptor(), example)
                    .unwrap_or_else(|message| panic!("{message}"));
            }
        }
    }

    #[test]
    fn every_task_returns_at_least_one_function() {
        for task in TASK_VARIANTS {
            let outcome = call("plan.recommend", serde_json::json!({"task": task}));
            assert!(
                !array_field(&outcome.value, "recommended_functions").is_empty(),
                "task {task} recommended nothing"
            );
        }
    }

    #[test]
    fn compare_proportions_includes_proportions_methods() {
        let outcome = call(
            "plan.recommend",
            serde_json::json!({"task": "compare_proportions", "data_shape": "two_independent"}),
        );
        assert!(contains_text(
            &outcome.value,
            "recommended_functions",
            "statistics.proportions_difference"
        ));
        assert!(contains_text(
            &outcome.value,
            "recommended_functions",
            "statistics.sample_size_two_proportions"
        ));
    }

    #[test]
    fn experiment_design_points_to_sample_size_and_warns_about_peeking() {
        let outcome = call(
            "plan.recommend",
            serde_json::json!({"task": "experiment_design"}),
        );
        assert!(contains_text(
            &outcome.value,
            "recommended_functions",
            "statistics.sample_size_two_means"
        ));
        assert!(contains_text(
            &outcome.value,
            "recommended_functions",
            "statistics.sample_size_two_proportions"
        ));
        let rendered = serde_json::to_string(&outcome.value).unwrap();
        assert!(
            rendered.contains("peek"),
            "no peeking warning in {rendered}"
        );
    }

    #[test]
    fn experiment_checklist_warns_about_peeking_and_points_to_sample_size() {
        let outcome = call(
            "plan.experiment_checklist",
            serde_json::json!({"kind": "two_proportions"}),
        );
        assert!(contains_text(
            &outcome.value,
            "sample_size_functions",
            "statistics.sample_size_two_proportions"
        ));
        let rendered = serde_json::to_string(&outcome.value).unwrap();
        assert!(
            rendered.contains("peek"),
            "no peeking warning in {rendered}"
        );
        assert!(
            rendered.contains("sequential"),
            "no sequential-testing warning in {rendered}"
        );
        assert!(
            !array_field(&outcome.value, "pre_registration").is_empty(),
            "pre-registration checklist is empty"
        );
    }

    #[test]
    fn describe_fields_handles_mixed_kinds() {
        let outcome = call(
            "plan.describe_fields",
            serde_json::json!({"fields": [
                {"name": "segment", "type": "categorical", "n": 120},
                {"name": "orders", "type": "count", "n": 120},
                {"name": "revenue", "type": "money", "n": 120},
                {"name": "opened_at", "type": "date", "n": 120},
                {"name": "notes", "type": "text", "n": 120}
            ]}),
        );
        assert_eq!(array_field(&outcome.value, "fields").len(), 5);
        assert!(!array_field(&outcome.value, "combinations").is_empty());
        let rendered = serde_json::to_string(&outcome.value).unwrap();
        assert!(
            rendered.contains("statistics.chi_square_contingency"),
            "missing categorical combination in {rendered}"
        );
        assert!(
            rendered.contains("finance.xnpv"),
            "missing money/date combination in {rendered}"
        );
        assert!(rendered.contains("missing"), "no missingness caveat");
        assert!(rendered.contains("multiplicity"), "no multiplicity caveat");
    }

    #[test]
    fn describe_fields_rejects_negative_observation_count() {
        let error = try_call(
            "plan.describe_fields",
            &serde_json::json!({"fields": [{"name": "age", "type": "continuous", "n": -3}]}),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::DomainViolation);
    }

    #[test]
    fn describe_fields_accepts_a_single_field() {
        let outcome = call(
            "plan.describe_fields",
            serde_json::json!({"fields": [{"name": "age", "type": "continuous"}]}),
        );
        assert!(array_field(&outcome.value, "combinations").is_empty());
        let described = &array_field(&outcome.value, "fields")[0];
        assert!(contains_text(
            described,
            "suggested_analyses",
            "statistics.ci_mean"
        ));
    }

    #[test]
    fn irr_notes_warn_about_multiple_roots() {
        let outcome = call(
            "plan.interpretation_notes",
            serde_json::json!({"functions": ["finance.irr"]}),
        );
        let rendered = serde_json::to_string(&outcome.value).unwrap();
        assert!(
            rendered.contains("multiple real roots"),
            "no multiple-root warning in {rendered}"
        );
        assert!(
            rendered.contains("sign change"),
            "no sign-change warning in {rendered}"
        );
    }

    #[test]
    fn stratified_experiment_notes_warn_about_standardization() {
        let outcome = call(
            "plan.interpretation_notes",
            serde_json::json!({"functions": ["statistics.stratified_experiment"]}),
        );
        let rendered = serde_json::to_string(&outcome.value).unwrap();
        assert!(
            rendered.contains("standardization"),
            "no standardization warning in {rendered}"
        );
        assert!(
            rendered.contains("pooled"),
            "no pooling warning in {rendered}"
        );
    }

    #[test]
    fn unknown_function_ids_get_generic_notes() {
        let outcome = call(
            "plan.interpretation_notes",
            serde_json::json!({"functions": ["made.up"]}),
        );
        let note = &array_field(&outcome.value, "notes")[0];
        assert_eq!(text_field(note, "family"), "unknown");
    }

    #[test]
    fn all_recommended_ids_are_known_registered_functions() {
        for task in TASK_VARIANTS {
            for shape in SHAPE_VARIANTS {
                for goal in GOAL_VARIANTS {
                    let outcome = call(
                        "plan.recommend",
                        serde_json::json!({
                            "task": task,
                            "data_shape": shape,
                            "goal": goal,
                        }),
                    );
                    for item in array_field(&outcome.value, "recommended_functions") {
                        let id = item.as_text().unwrap();
                        assert!(
                            KNOWN_FUNCTION_IDS.contains(&id),
                            "{id} is not a registered function id"
                        );
                    }
                    for alternative in array_field(&outcome.value, "alternatives") {
                        let id = text_field(alternative, "function");
                        assert!(
                            KNOWN_FUNCTION_IDS.contains(&id),
                            "{id} is not a registered function id"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn checklist_range_checks_are_enforced() {
        for raw in [
            serde_json::json!({"kind": "two_means", "baseline_rate": "1.5"}),
            serde_json::json!({"kind": "two_means", "minimum_detectable_effect": 0}),
            serde_json::json!({"kind": "two_means", "alpha": 0}),
            serde_json::json!({"kind": "two_means", "power": 1}),
        ] {
            let error = try_call("plan.experiment_checklist", &raw).unwrap_err();
            assert_eq!(error.code, ErrorCode::DomainViolation, "input {raw}");
        }
    }

    #[test]
    fn checklist_echoes_provided_inputs() {
        let outcome = call(
            "plan.experiment_checklist",
            serde_json::json!({
                "kind": "two_means",
                "baseline_rate": "0.4",
                "alpha": "0.05",
            }),
        );
        let provided = field(&outcome.value, "provided");
        assert!(matches!(field(provided, "baseline_rate"), Value::Number(_)));
        assert!(matches!(field(provided, "alpha"), Value::Number(_)));
    }
}
