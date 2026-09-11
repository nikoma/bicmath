//! Stratified experiment analysis.
//!
//! The functions here are generic and data-driven: nothing is specific to a
//! campaign. Each stratum carries a categorical per-person outcome distribution
//! for a treatment and a control arm, and the arm summaries are standardized to
//! a caller-supplied target mix. Positive outcome rates, negative outcome
//! rates, and the contribution difference are reported in separate blocks so
//! that a change in retention can never be conflated with a change in net
//! contribution.

use std::collections::{BTreeMap, BTreeSet};
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

const MIX_TOLERANCE: f64 = 1e-9;

// ---------------------------------------------------------------------------
// Input model
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct OutcomeCategory {
    value: f64,
    count: u64,
}

#[derive(Clone, Debug)]
struct ArmInput {
    assigned: u64,
    outcomes: Vec<OutcomeCategory>,
}

#[derive(Clone, Debug)]
struct StratumInput {
    name: String,
    target_weight: f64,
    treatment: ArmInput,
    control: ArmInput,
}

#[derive(Clone, Debug)]
struct ArmStats {
    assigned: u64,
    mean: f64,
    variance: f64,
    positive_count: u64,
    negative_count: u64,
    zero_count: u64,
    positive_rate: f64,
    negative_rate: f64,
    net_positive_rate: f64,
}

impl ArmStats {
    fn n(&self) -> f64 {
        self.assigned as f64
    }

    /// Per-person variance of the indicator 1{positive} - 1{negative}.
    fn net_variance(&self) -> f64 {
        self.positive_rate + self.negative_rate - self.net_positive_rate.powi(2)
    }
}

#[derive(Clone, Debug)]
struct StratumStats {
    name: String,
    target_weight: f64,
    weight: f64,
    treatment: ArmStats,
    control: ArmStats,
}

#[derive(Clone, Debug)]
struct Block {
    treatment: f64,
    control: f64,
    difference: f64,
    variance: f64,
    standard_error: f64,
    ci_lower: f64,
    ci_upper: f64,
}

impl Block {
    fn new(treatment: f64, control: f64, variance: f64, z: f64) -> Result<Block, EngineError> {
        if !variance.is_finite() {
            return Err(EngineError::domain("standardized variance is not finite"));
        }
        let variance = variance.max(0.0);
        let standard_error = sqrt(variance);
        let difference = treatment - control;
        Ok(Block {
            treatment,
            control,
            difference,
            variance,
            standard_error,
            ci_lower: difference - z * standard_error,
            ci_upper: difference + z * standard_error,
        })
    }
}

#[derive(Clone, Debug)]
struct PooledRates {
    treatment_positive_rate: f64,
    control_positive_rate: f64,
    positive_rate_difference: f64,
    treatment_negative_rate: f64,
    control_negative_rate: f64,
    negative_rate_difference: f64,
    treatment_net_positive_rate: f64,
    control_net_positive_rate: f64,
    net_positive_rate_difference: f64,
}

#[derive(Clone, Debug)]
struct Analysis {
    strata: Vec<StratumStats>,
    standardized: Block,
    positive_rate: Block,
    negative_rate: Block,
    net_positive_rate: Block,
    observed_mix_matches: bool,
    pooled: PooledRates,
    confidence: f64,
    z: f64,
    /// Sum of the target weights: the represented population scale used to
    /// convert per-person standardized quantities to totals.
    population: f64,
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

fn path_error(error: EngineError, path: &str) -> EngineError {
    error.with_path(path.to_string())
}

fn record_at<'a>(value: &'a Value, path: &str) -> Result<&'a BTreeMap<String, Value>, EngineError> {
    value.as_record().map_err(|e| path_error(e, path))
}

fn text_field(
    record: &BTreeMap<String, Value>,
    key: &str,
    path: &str,
) -> Result<String, EngineError> {
    match record.get(key) {
        Some(Value::Text(value)) => Ok(value.clone()),
        Some(other) => Err(path_error(
            EngineError::malformed(format!(
                "field {key:?} must be text, found {}",
                other.kind_name()
            )),
            path,
        )),
        None => Err(path_error(
            EngineError::malformed(format!("missing required field {key:?}")),
            path,
        )),
    }
}

fn number_field(
    record: &BTreeMap<String, Value>,
    key: &str,
    path: &str,
) -> Result<f64, EngineError> {
    match record.get(key) {
        Some(Value::Number(number)) => {
            number_to_f64(number).map_err(|e| path_error(e, &format!("{path}.{key}")))
        }
        Some(other) => Err(path_error(
            EngineError::malformed(format!(
                "field {key:?} must be a number, found {}",
                other.kind_name()
            )),
            path,
        )),
        None => Err(path_error(
            EngineError::malformed(format!("missing required field {key:?}")),
            path,
        )),
    }
}

fn array_field<'a>(
    record: &'a BTreeMap<String, Value>,
    key: &str,
    path: &str,
) -> Result<&'a [Value], EngineError> {
    match record.get(key) {
        Some(Value::Array(values)) => Ok(values),
        Some(other) => Err(path_error(
            EngineError::malformed(format!(
                "field {key:?} must be an array, found {}",
                other.kind_name()
            )),
            path,
        )),
        None => Err(path_error(
            EngineError::malformed(format!("missing required field {key:?}")),
            path,
        )),
    }
}

fn parse_arm(raw: Option<&Value>, path: &str) -> Result<ArmInput, EngineError> {
    let raw =
        raw.ok_or_else(|| path_error(EngineError::malformed("missing required arm record"), path))?;
    let record = record_at(raw, path)?;
    let assigned = match record.get("assigned") {
        Some(Value::Number(number)) => {
            non_negative_u64(&number_to_bigint(number, path)?, "assigned")?
        }
        Some(other) => {
            return Err(path_error(
                EngineError::malformed(format!(
                    "field \"assigned\" must be an integer, found {}",
                    other.kind_name()
                )),
                path,
            ));
        }
        None => {
            return Err(path_error(
                EngineError::malformed("missing required field \"assigned\""),
                path,
            ));
        }
    };
    if assigned == 0 {
        return Err(path_error(
            EngineError::domain("assigned must be at least 1"),
            path,
        ));
    }
    let outcomes = parse_outcomes(record, path)?;
    Ok(ArmInput { assigned, outcomes })
}

fn number_to_bigint(
    number: &bicmath_core::number::Number,
    path: &str,
) -> Result<num_bigint::BigInt, EngineError> {
    match number {
        bicmath_core::number::Number::Integer(value) => Ok(value.clone()),
        other => other
            .to_exact_rational()
            .and_then(|rational| {
                if rational.is_integer() {
                    Some(rational.to_integer())
                } else {
                    None
                }
            })
            .ok_or_else(|| path_error(EngineError::malformed("expected an integer value"), path)),
    }
}

fn parse_outcomes(
    record: &BTreeMap<String, Value>,
    path: &str,
) -> Result<Vec<OutcomeCategory>, EngineError> {
    let raw = array_field(record, "outcomes", path)?;
    if raw.is_empty() {
        return Err(path_error(
            EngineError::domain("each arm must have at least one outcome category"),
            path,
        ));
    }
    let mut outcomes = Vec::with_capacity(raw.len());
    for (index, item) in raw.iter().enumerate() {
        let entry_path = format!("{path}.outcomes[{index}]");
        let entry = record_at(item, &entry_path)?;
        let value = number_field(entry, "value", &entry_path)?;
        let count = match entry.get("count") {
            Some(Value::Number(number)) => {
                non_negative_u64(&number_to_bigint(number, &entry_path)?, "count")?
            }
            Some(other) => {
                return Err(path_error(
                    EngineError::malformed(format!(
                        "field \"count\" must be an integer, found {}",
                        other.kind_name()
                    )),
                    &entry_path,
                ));
            }
            None => {
                return Err(path_error(
                    EngineError::malformed("missing required field \"count\""),
                    &entry_path,
                ));
            }
        };
        outcomes.push(OutcomeCategory { value, count });
    }
    Ok(outcomes)
}

fn parse_strata(args: &Args, name: &str) -> Result<Vec<StratumInput>, EngineError> {
    let items = args.array(name)?;
    if items.is_empty() {
        return Err(EngineError::domain("strata must not be empty").with_path(name.to_string()));
    }
    let mut seen = BTreeSet::new();
    let mut strata = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let path = format!("{name}[{index}]");
        let record = record_at(item, &path)?;
        let stratum_name = text_field(record, "name", &path)?;
        if !seen.insert(stratum_name.clone()) {
            return Err(path_error(
                EngineError::malformed(format!("duplicate stratum name {stratum_name:?}")),
                &path,
            ));
        }
        let target_weight = number_field(record, "target_weight", &path)?;
        if target_weight < 0.0 {
            return Err(path_error(
                EngineError::domain("target_weight must be non-negative"),
                &format!("{path}.target_weight"),
            ));
        }
        let treatment = parse_arm(record.get("treatment"), &format!("{path}.treatment"))?;
        let control = parse_arm(record.get("control"), &format!("{path}.control"))?;
        if treatment.outcomes.iter().map(|o| o.count).sum::<u64>() != treatment.assigned {
            return Err(path_error(
                EngineError::domain(
                    "treatment outcome counts must sum exactly to treatment.assigned",
                ),
                &format!("{path}.treatment"),
            ));
        }
        if control.outcomes.iter().map(|o| o.count).sum::<u64>() != control.assigned {
            return Err(path_error(
                EngineError::domain("control outcome counts must sum exactly to control.assigned"),
                &format!("{path}.control"),
            ));
        }
        strata.push(StratumInput {
            name: stratum_name,
            target_weight,
            treatment,
            control,
        });
    }
    // The sum of target weights is validated during analysis; an all-zero mix
    // is rejected there, which also allows prospective inputs whose weights are
    // supplied only by the pooled combination.
    Ok(strata)
}

// ---------------------------------------------------------------------------
// Arm statistics
// ---------------------------------------------------------------------------

fn arm_stats(arm: &ArmInput, ddof: u64) -> Result<ArmStats, EngineError> {
    if arm.assigned <= ddof {
        return Err(insufficient(format!(
            "each arm needs more than variance_ddof = {ddof} assigned observations"
        )));
    }
    let n = arm.assigned as f64;
    let mut sum = 0.0f64;
    let mut sum_squares = 0.0f64;
    let mut positive_count = 0u64;
    let mut negative_count = 0u64;
    let mut zero_count = 0u64;
    for outcome in &arm.outcomes {
        let count = outcome.count as f64;
        sum += outcome.value * count;
        sum_squares += outcome.value * outcome.value * count;
        if outcome.value > 0.0 {
            positive_count += outcome.count;
        } else if outcome.value < 0.0 {
            negative_count += outcome.count;
        } else {
            zero_count += outcome.count;
        }
    }
    let mean = sum / n;
    let raw_variance = (sum_squares - n * mean * mean) / (n - ddof as f64);
    // The mandated formula can cancel catastrophically; treat only tiny
    // negative artifacts as zero and reject anything materially negative.
    let variance = if raw_variance < 0.0 {
        if raw_variance > -1e-9 * (1.0 + sum_squares.abs()) {
            0.0
        } else {
            return Err(EngineError::domain(
                "computed arm variance is negative; outcome values are too large for the \
                 mandated sum-of-squares formula",
            ));
        }
    } else {
        raw_variance
    };
    let positive_rate = positive_count as f64 / n;
    let negative_rate = negative_count as f64 / n;
    Ok(ArmStats {
        assigned: arm.assigned,
        mean,
        variance,
        positive_count,
        negative_count,
        zero_count,
        positive_rate,
        negative_rate,
        net_positive_rate: positive_rate - negative_rate,
    })
}

// ---------------------------------------------------------------------------
// Standardization
// ---------------------------------------------------------------------------

fn analyze(
    inputs: &[StratumInput],
    confidence: f64,
    variance_ddof: u64,
) -> Result<Analysis, EngineError> {
    let total_target: f64 = inputs.iter().map(|stratum| stratum.target_weight).sum();
    if total_target <= 0.0 {
        return Err(EngineError::domain(
            "the sum of target weights must be strictly positive",
        ));
    }
    let mut strata = Vec::with_capacity(inputs.len());
    for input in inputs {
        let treatment = arm_stats(&input.treatment, variance_ddof)?;
        let control = arm_stats(&input.control, variance_ddof)?;
        strata.push(StratumStats {
            name: input.name.clone(),
            target_weight: input.target_weight,
            weight: input.target_weight / total_target,
            treatment,
            control,
        });
    }
    let z = normal_quantile(1.0 - (1.0 - confidence) / 2.0, 0.0, 1.0)?;

    let mut treatment_mean = 0.0f64;
    let mut control_mean = 0.0f64;
    let mut mean_variance = 0.0f64;
    let mut treatment_positive = 0.0f64;
    let mut control_positive = 0.0f64;
    let mut positive_variance = 0.0f64;
    let mut treatment_negative = 0.0f64;
    let mut control_negative = 0.0f64;
    let mut negative_variance = 0.0f64;
    let mut treatment_net = 0.0f64;
    let mut control_net = 0.0f64;
    let mut net_variance = 0.0f64;

    for stratum in &strata {
        let weight = stratum.weight;
        let weight_squared = weight * weight;
        let treatment = &stratum.treatment;
        let control = &stratum.control;

        treatment_mean += weight * treatment.mean;
        control_mean += weight * control.mean;
        mean_variance +=
            weight_squared * (treatment.variance / treatment.n() + control.variance / control.n());

        treatment_positive += weight * treatment.positive_rate;
        control_positive += weight * control.positive_rate;
        positive_variance += weight_squared
            * (treatment.positive_rate * (1.0 - treatment.positive_rate) / treatment.n()
                + control.positive_rate * (1.0 - control.positive_rate) / control.n());

        treatment_negative += weight * treatment.negative_rate;
        control_negative += weight * control.negative_rate;
        negative_variance += weight_squared
            * (treatment.negative_rate * (1.0 - treatment.negative_rate) / treatment.n()
                + control.negative_rate * (1.0 - control.negative_rate) / control.n());

        treatment_net += weight * treatment.net_positive_rate;
        control_net += weight * control.net_positive_rate;
        net_variance += weight_squared
            * (treatment.net_variance() / treatment.n() + control.net_variance() / control.n());
    }

    let total_assigned: f64 = strata
        .iter()
        .map(|stratum| stratum.treatment.n() + stratum.control.n())
        .sum();
    let mut observed_mix_matches = true;
    for stratum in &strata {
        let observed = (stratum.treatment.n() + stratum.control.n()) / total_assigned;
        if (observed - stratum.weight).abs() > MIX_TOLERANCE {
            observed_mix_matches = false;
        }
    }

    let pooled = pooled_rates(&strata);

    Ok(Analysis {
        standardized: Block::new(treatment_mean, control_mean, mean_variance, z)?,
        positive_rate: Block::new(treatment_positive, control_positive, positive_variance, z)?,
        negative_rate: Block::new(treatment_negative, control_negative, negative_variance, z)?,
        net_positive_rate: Block::new(treatment_net, control_net, net_variance, z)?,
        strata,
        observed_mix_matches,
        pooled,
        confidence,
        z,
        population: total_target,
    })
}

fn pooled_rates(strata: &[StratumStats]) -> PooledRates {
    let mut treatment_assigned = 0.0f64;
    let mut control_assigned = 0.0f64;
    let mut treatment_positive = 0.0f64;
    let mut control_positive = 0.0f64;
    let mut treatment_negative = 0.0f64;
    let mut control_negative = 0.0f64;
    for stratum in strata {
        treatment_assigned += stratum.treatment.n();
        control_assigned += stratum.control.n();
        treatment_positive += stratum.treatment.positive_count as f64;
        control_positive += stratum.control.positive_count as f64;
        treatment_negative += stratum.treatment.negative_count as f64;
        control_negative += stratum.control.negative_count as f64;
    }
    let treatment_positive_rate = treatment_positive / treatment_assigned;
    let control_positive_rate = control_positive / control_assigned;
    let treatment_negative_rate = treatment_negative / treatment_assigned;
    let control_negative_rate = control_negative / control_assigned;
    let treatment_net = treatment_positive_rate - treatment_negative_rate;
    let control_net = control_positive_rate - control_negative_rate;
    PooledRates {
        treatment_positive_rate,
        control_positive_rate,
        positive_rate_difference: treatment_positive_rate - control_positive_rate,
        treatment_negative_rate,
        control_negative_rate,
        negative_rate_difference: treatment_negative_rate - control_negative_rate,
        treatment_net_positive_rate: treatment_net,
        control_net_positive_rate: control_net,
        net_positive_rate_difference: treatment_net - control_net,
    }
}

// ---------------------------------------------------------------------------
// Output construction
// ---------------------------------------------------------------------------

fn confidence_interval_value(lower: f64, upper: f64) -> Result<Value, EngineError> {
    Ok(record(vec![
        ("lower", float_value(lower)?),
        ("upper", float_value(upper)?),
    ]))
}

fn block_value(
    treatment_key: &str,
    control_key: &str,
    block: &Block,
    z: f64,
    confidence: f64,
) -> Result<Value, EngineError> {
    Ok(record(vec![
        (treatment_key, float_value(block.treatment)?),
        (control_key, float_value(block.control)?),
        ("difference", float_value(block.difference)?),
        ("variance", float_value(block.variance)?),
        ("standard_error", float_value(block.standard_error)?),
        ("z", float_value(z)?),
        (
            "confidence_interval",
            confidence_interval_value(block.ci_lower, block.ci_upper)?,
        ),
        ("confidence", float_value(confidence)?),
        ("method", text("normal_approximation_z_interval")),
    ]))
}

fn arm_value(arm: &ArmStats) -> Result<Value, EngineError> {
    Ok(record(vec![
        ("assigned", integer_value(arm.assigned)),
        ("positive_count", integer_value(arm.positive_count)),
        ("negative_count", integer_value(arm.negative_count)),
        ("zero_count", integer_value(arm.zero_count)),
        ("mean", float_value(arm.mean)?),
        ("variance", float_value(arm.variance)?),
        ("positive_rate", float_value(arm.positive_rate)?),
        ("negative_rate", float_value(arm.negative_rate)?),
        ("net_positive_rate", float_value(arm.net_positive_rate)?),
    ]))
}

fn strata_value(analysis: &Analysis) -> Result<Value, EngineError> {
    let mut entries = Vec::with_capacity(analysis.strata.len());
    for stratum in &analysis.strata {
        let treatment = &stratum.treatment;
        let control = &stratum.control;
        entries.push(record(vec![
            ("name", text(stratum.name.clone())),
            ("target_weight", float_value(stratum.target_weight)?),
            ("weight", float_value(stratum.weight)?),
            ("treatment", arm_value(treatment)?),
            ("control", arm_value(control)?),
            (
                "difference",
                record(vec![
                    ("mean", float_value(treatment.mean - control.mean)?),
                    (
                        "positive_rate",
                        float_value(treatment.positive_rate - control.positive_rate)?,
                    ),
                    (
                        "negative_rate",
                        float_value(treatment.negative_rate - control.negative_rate)?,
                    ),
                    (
                        "net_positive_rate",
                        float_value(treatment.net_positive_rate - control.net_positive_rate)?,
                    ),
                ]),
            ),
        ]));
    }
    Ok(array_value(entries))
}

fn diagnostics_value(
    analysis: &Analysis,
    variance_ddof: u64,
    notes: &[String],
) -> Result<Value, EngineError> {
    let mut observed_weights = Vec::with_capacity(analysis.strata.len());
    let mut target_weights = Vec::with_capacity(analysis.strata.len());
    let total_assigned: f64 = analysis
        .strata
        .iter()
        .map(|stratum| stratum.treatment.n() + stratum.control.n())
        .sum();
    for stratum in &analysis.strata {
        observed_weights.push(float_value(
            (stratum.treatment.n() + stratum.control.n()) / total_assigned,
        )?);
        target_weights.push(float_value(stratum.weight)?);
    }
    let pooled = &analysis.pooled;
    Ok(record(vec![
        (
            "observed_mix_vs_target",
            bool_value(analysis.observed_mix_matches),
        ),
        ("observed_weights", array_value(observed_weights)),
        ("target_weights", array_value(target_weights)),
        (
            "pooled",
            record(vec![
                (
                    "treatment_positive_rate",
                    float_value(pooled.treatment_positive_rate)?,
                ),
                (
                    "control_positive_rate",
                    float_value(pooled.control_positive_rate)?,
                ),
                (
                    "positive_rate_difference",
                    float_value(pooled.positive_rate_difference)?,
                ),
                (
                    "treatment_negative_rate",
                    float_value(pooled.treatment_negative_rate)?,
                ),
                (
                    "control_negative_rate",
                    float_value(pooled.control_negative_rate)?,
                ),
                (
                    "negative_rate_difference",
                    float_value(pooled.negative_rate_difference)?,
                ),
                (
                    "treatment_net_positive_rate",
                    float_value(pooled.treatment_net_positive_rate)?,
                ),
                (
                    "control_net_positive_rate",
                    float_value(pooled.control_net_positive_rate)?,
                ),
                (
                    "net_positive_rate_difference",
                    float_value(pooled.net_positive_rate_difference)?,
                ),
            ]),
        ),
        ("variance_ddof", integer_value(variance_ddof)),
        (
            "notes",
            array_value(notes.iter().map(|note| text(note.clone())).collect()),
        ),
    ]))
}

fn assumption_texts(prospective: bool) -> Vec<&'static str> {
    let mut texts = vec![
        "treatment and control arms are independently assigned within each stratum",
        "the supplied target weights define the standardization mix and are not estimated from the data",
        "outcome values are complete and measured over comparable windows across arms unless the caller states otherwise",
        "the normal approximation is used for the sampling distribution of the standardized difference",
        "fixed_cost is treated as known and adds no sampling variance",
        "rollout_fraction scales the expected difference and its variance linearly; it is not a probabilistic guarantee",
        "a positive outcome rate difference is not the same as a positive contribution difference",
    ];
    if prospective {
        texts.push("the additional data is hypothetical and the result is not an observed result");
    }
    texts
}

fn build_output(
    analysis: &Analysis,
    rollout_fraction: f64,
    fixed_cost: f64,
    variance_ddof: u64,
    prospective: bool,
) -> Result<Value, EngineError> {
    let standardized = &analysis.standardized;
    let population = analysis.population;
    let scaled_difference = standardized.difference * rollout_fraction;
    let scaled_variance = standardized.variance * rollout_fraction * rollout_fraction;
    let scaled_standard_error = standardized.standard_error * rollout_fraction;
    let scaled_lower = standardized.ci_lower * rollout_fraction;
    let scaled_upper = standardized.ci_upper * rollout_fraction;
    // Totals are the per-person standardized quantities multiplied by the
    // represented population scale (the sum of target weights). Fixed cost is
    // applied at the total level after the interval and adds no variance.
    let total_difference = scaled_difference * population;
    let total_lower = scaled_lower * population;
    let total_upper = scaled_upper * population;
    let net_difference = total_difference - fixed_cost;
    let net_lower = total_lower - fixed_cost;
    let net_upper = total_upper - fixed_cost;
    let mut notes = vec![
        "the standardized estimate is computed from the supplied target mix, not from the observed assigned mix"
            .to_string(),
        "pooled rates are reported for diagnostics only and do not replace the standardized estimate"
            .to_string(),
    ];
    if !analysis.observed_mix_matches {
        notes.push(
            "the observed assigned mix differs from the target weights by more than 1e-9; pooled rates are not a substitute for the standardized estimate"
                .to_string(),
        );
    }
    Ok(record(vec![
        ("prospective", bool_value(prospective)),
        ("strata", strata_value(analysis)?),
        (
            "standardized",
            record(vec![
                ("treatment_mean", float_value(standardized.treatment)?),
                ("control_mean", float_value(standardized.control)?),
                ("difference", float_value(standardized.difference)?),
                ("variance", float_value(standardized.variance)?),
                ("standard_error", float_value(standardized.standard_error)?),
                ("z", float_value(analysis.z)?),
                (
                    "confidence_interval",
                    confidence_interval_value(standardized.ci_lower, standardized.ci_upper)?,
                ),
                ("confidence", float_value(analysis.confidence)?),
                ("df", Value::Null),
                ("method", text("normal_approximation_z_interval")),
            ]),
        ),
        (
            "positive_rate",
            block_value(
                "treatment_rate",
                "control_rate",
                &analysis.positive_rate,
                analysis.z,
                analysis.confidence,
            )?,
        ),
        (
            "negative_rate",
            block_value(
                "treatment_rate",
                "control_rate",
                &analysis.negative_rate,
                analysis.z,
                analysis.confidence,
            )?,
        ),
        (
            "net_positive_rate",
            block_value(
                "treatment_rate",
                "control_rate",
                &analysis.net_positive_rate,
                analysis.z,
                analysis.confidence,
            )?,
        ),
        (
            "scaled",
            record(vec![
                ("rollout_fraction", float_value(rollout_fraction)?),
                ("difference", float_value(scaled_difference)?),
                ("variance", float_value(scaled_variance)?),
                ("standard_error", float_value(scaled_standard_error)?),
                (
                    "confidence_interval",
                    confidence_interval_value(scaled_lower, scaled_upper)?,
                ),
                ("total", float_value(total_difference)?),
                (
                    "total_confidence_interval",
                    confidence_interval_value(total_lower, total_upper)?,
                ),
                ("fixed_cost", float_value(fixed_cost)?),
                ("net_difference", float_value(net_difference)?),
                (
                    "net_confidence_interval",
                    confidence_interval_value(net_lower, net_upper)?,
                ),
            ]),
        ),
        (
            "diagnostics",
            diagnostics_value(analysis, variance_ddof, &notes)?,
        ),
        (
            "assumptions",
            array_value(
                assumption_texts(prospective)
                    .into_iter()
                    .map(text)
                    .collect(),
            ),
        ),
    ]))
}

fn attach_common(mut outcome: Outcome, analysis: &Analysis, prospective: bool) -> Outcome {
    if !analysis.observed_mix_matches {
        outcome = outcome.with_warning(Warning::new(
            "observed_mix_differs_from_target",
            "the observed assigned mix differs from the supplied target weights by more than 1e-9",
        ));
    }
    if prospective {
        outcome = outcome.with_warning(Warning::new(
            "prospective_hypothetical_data",
            "the result uses hypothetical additional data and is not an observed result",
        ));
    }
    for (index, statement) in assumption_texts(prospective).iter().enumerate() {
        outcome = outcome.with_assumption(Assumption::unverified(
            format!("stratified_{index}"),
            *statement,
        ));
    }
    outcome
}

// ---------------------------------------------------------------------------
// stratified_experiment
// ---------------------------------------------------------------------------

fn stratified_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "statistics.stratified_experiment",
        "statistics",
        "1.0.0",
        "Stratified experiment analysis",
        "Standardize per-stratum treatment-minus-control differences to a target mix.",
    )
    .with_description(
        "strata is an array of records {name, target_weight, treatment, control}, where each \
         arm is {assigned, outcomes: [{value, count}]}. Outcome counts must sum exactly to \
         assigned. For each arm the mean of the per-person outcome and the unbiased sample \
         variance (sum(value^2 * count) - n * mean^2) / (n - variance_ddof) are computed, \
         together with the positive, negative, and net-positive outcome rates. Strata are \
         standardized with weight = target_weight / sum(target_weight); the standardized \
         treatment-minus-control variance is sum(weight^2 * (var_t / n_t + var_c / n_c)) and \
         the interval is estimate +/- z * se with z = normal_quantile(1 - (1 - confidence)/2) \
         (method normal_approximation_z_interval). Rate differences use the per-arm binomial \
         variance p (1 - p) / n, and the net-positive rate uses the per-person variance \
         p_positive + p_negative - net^2. Contribution, positive-rate, negative-rate, and \
         net-positive-rate differences are reported in separate blocks; a positive outcome \
         rate difference is not a positive contribution difference. The sum of target weights \
         is also the represented population scale: per-person standardized quantities are \
         multiplied by it to report totals. fixed_cost is subtracted from the scaled total \
         contribution after the interval and adds no sampling variance. rollout_fraction \
         scales the expected difference and its variance by fraction^2; the full and scaled \
         results are both reported.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "strata",
            "Array of stratum records {name, target_weight, treatment, control}.",
            array_schema(ValueSchema::Any),
        ),
        ParamDescriptor::optional(
            "confidence",
            "Confidence level in (0, 1); default 0.95.",
            any_number_schema(),
        ),
        ParamDescriptor::optional(
            "fixed_cost",
            "Known fixed cost subtracted from the scaled contribution difference; default 0.",
            any_number_schema(),
        ),
        ParamDescriptor::optional(
            "rollout_fraction",
            "Fraction of the population receiving the treatment; in [0, 1], default 1.",
            any_number_schema(),
        ),
        ParamDescriptor::optional(
            "variance_ddof",
            "Delta degrees of freedom for the per-arm variance; default 1.",
            integer_schema(),
        ),
    ])
    .with_output(
        record_schema(
            vec![
                bicmath_core::schema::FieldSchema::required("prospective", bool_schema()),
                bicmath_core::schema::FieldSchema::required(
                    "strata",
                    array_schema(ValueSchema::Any),
                ),
                bicmath_core::schema::FieldSchema::required("standardized", ValueSchema::Any),
                bicmath_core::schema::FieldSchema::required("positive_rate", ValueSchema::Any),
                bicmath_core::schema::FieldSchema::required("negative_rate", ValueSchema::Any),
                bicmath_core::schema::FieldSchema::required("net_positive_rate", ValueSchema::Any),
                bicmath_core::schema::FieldSchema::required("scaled", ValueSchema::Any),
                bicmath_core::schema::FieldSchema::required("diagnostics", ValueSchema::Any),
                bicmath_core::schema::FieldSchema::required(
                    "assumptions",
                    array_schema(text_schema()),
                ),
            ],
            true,
        ),
        "Standardized experiment analysis with separate contribution and rate blocks.",
    )
    .with_modes(inferential_modes())
    .with_cost(CostClass::Linear)
    .with_method_ref("docs/methods/statistics.md#stratified_experiment")
    .with_examples(vec![
        Example::new(
            "outcome counts must sum to assigned",
            example_args(&[(
                "strata",
                serde_json::json!([{
                    "name": "beginners",
                    "target_weight": 1,
                    "treatment": {"assigned": 3, "outcomes": [{"value": 10, "count": 2}]},
                    "control": {"assigned": 2, "outcomes": [{"value": 10, "count": 2}]}
                }]),
            )]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_stratified(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(
        ctx,
        &inferential_modes(),
        "statistics.stratified_experiment",
    )?;
    let strata = parse_strata(args, "strata")?;
    let confidence = confidence_param(args)?;
    let fixed_cost = optional_f64_param(args, "fixed_cost")?.unwrap_or(0.0);
    let rollout_fraction = optional_f64_param(args, "rollout_fraction")?.unwrap_or(1.0);
    if !(0.0..=1.0).contains(&rollout_fraction) {
        return Err(EngineError::domain("rollout_fraction must be in [0, 1]")
            .with_path("rollout_fraction".to_string()));
    }
    let variance_ddof = match args.optional_integer("variance_ddof")? {
        Some(value) => non_negative_u64(&value, "variance_ddof")?,
        None => 1,
    };
    let analysis = analyze(&strata, confidence, variance_ddof)?;
    let value = build_output(
        &analysis,
        rollout_fraction,
        fixed_cost,
        variance_ddof,
        false,
    )?;
    Ok(attach_common(Outcome::approximate(value), &analysis, false))
}

// ---------------------------------------------------------------------------
// prospective_pool
// ---------------------------------------------------------------------------

fn prospective_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "statistics.prospective_pool",
        "statistics",
        "1.0.0",
        "Prospective pooled experiment analysis",
        "Pool observed and hypothetical additional strata and standardize the combination.",
    )
    .with_description(
        "Combines two strata specifications with the same structure into pooled per-stratum \
         arm summaries (assigned counts and outcome counts summed by value), then applies the \
         same standardization as stratified_experiment. target_weights may override the \
         pooled target weights as either an array aligned with the original strata order or a \
         record keyed by stratum name. The output is labelled prospective: true and carries a \
         warning that the result uses hypothetical additional data and is not an observed \
         result.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "original",
            "Observed strata, same structure as stratified_experiment.",
            array_schema(ValueSchema::Any),
        ),
        ParamDescriptor::required(
            "additional",
            "Hypothetical additional strata with the same names and structure.",
            array_schema(ValueSchema::Any),
        ),
        ParamDescriptor::optional(
            "target_weights",
            "Optional override: an array aligned with original strata order, or a record keyed by name.",
            ValueSchema::Any,
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
                bicmath_core::schema::FieldSchema::required("prospective", bool_schema()),
                bicmath_core::schema::FieldSchema::required("strata", array_schema(ValueSchema::Any)),
                bicmath_core::schema::FieldSchema::required("standardized", ValueSchema::Any),
                bicmath_core::schema::FieldSchema::required("positive_rate", ValueSchema::Any),
                bicmath_core::schema::FieldSchema::required("negative_rate", ValueSchema::Any),
                bicmath_core::schema::FieldSchema::required("net_positive_rate", ValueSchema::Any),
                bicmath_core::schema::FieldSchema::required("scaled", ValueSchema::Any),
                bicmath_core::schema::FieldSchema::required("diagnostics", ValueSchema::Any),
                bicmath_core::schema::FieldSchema::required(
                    "assumptions",
                    array_schema(text_schema()),
                ),
            ],
            true,
        ),
        "Prospective pooled analysis, labelled with prospective: true.",
    )
    .with_modes(inferential_modes())
    .with_cost(CostClass::Linear)
    .with_method_ref("docs/methods/statistics.md#prospective_pool")
    .with_examples(vec![Example::new(
        "stratum names must match",
        example_args(&[
            (
                "original",
                serde_json::json!([{
                    "name": "a",
                    "target_weight": 1,
                    "treatment": {"assigned": 1, "outcomes": [{"value": 1, "count": 1}]},
                    "control": {"assigned": 1, "outcomes": [{"value": 0, "count": 1}]}
                }]),
            ),
            (
                "additional",
                serde_json::json!([{
                    "name": "b",
                    "target_weight": 1,
                    "treatment": {"assigned": 1, "outcomes": [{"value": 1, "count": 1}]},
                    "control": {"assigned": 1, "outcomes": [{"value": 0, "count": 1}]}
                }]),
            ),
        ]),
    )
    .with_error(ErrorCode::MalformedInput)])
}

fn canonical_bits(value: f64) -> u64 {
    if value == 0.0 {
        0.0f64.to_bits()
    } else {
        value.to_bits()
    }
}

fn merge_arms(original: &ArmInput, additional: &ArmInput) -> ArmInput {
    let mut merged: BTreeMap<u64, (f64, u64)> = BTreeMap::new();
    for arm in [original, additional] {
        for outcome in &arm.outcomes {
            let entry = merged
                .entry(canonical_bits(outcome.value))
                .or_insert((outcome.value, 0));
            entry.1 += outcome.count;
        }
    }
    ArmInput {
        assigned: original.assigned + additional.assigned,
        outcomes: merged
            .into_values()
            .map(|(value, count)| OutcomeCategory { value, count })
            .collect(),
    }
}

fn pool_strata(
    original: &[StratumInput],
    additional: &[StratumInput],
) -> Result<Vec<StratumInput>, EngineError> {
    let original_names: BTreeSet<&str> = original
        .iter()
        .map(|stratum| stratum.name.as_str())
        .collect();
    for stratum in additional {
        if !original_names.contains(stratum.name.as_str()) {
            return Err(EngineError::malformed(format!(
                "stratum {:?} is present in additional but not in original",
                stratum.name
            )));
        }
    }
    let additional_by_name: BTreeMap<&str, &StratumInput> = additional
        .iter()
        .map(|stratum| (stratum.name.as_str(), stratum))
        .collect();
    let mut pooled = Vec::with_capacity(original.len());
    for stratum in original {
        let extra = additional_by_name
            .get(stratum.name.as_str())
            .ok_or_else(|| {
                EngineError::malformed(format!(
                    "stratum {:?} is missing from additional",
                    stratum.name
                ))
            })?;
        pooled.push(StratumInput {
            name: stratum.name.clone(),
            target_weight: stratum.target_weight + extra.target_weight,
            treatment: merge_arms(&stratum.treatment, &extra.treatment),
            control: merge_arms(&stratum.control, &extra.control),
        });
    }
    Ok(pooled)
}

fn apply_target_weights(strata: &mut [StratumInput], raw: &Value) -> Result<(), EngineError> {
    match raw {
        Value::Array(items) => {
            if items.len() != strata.len() {
                return Err(EngineError::malformed(format!(
                    "target_weights has {} entries but there are {} strata",
                    items.len(),
                    strata.len()
                ))
                .with_path("target_weights".to_string()));
            }
            for (stratum, item) in strata.iter_mut().zip(items.iter()) {
                let number = item.as_number().map_err(|e| {
                    EngineError::malformed(format!(
                        "target_weights entries must be numbers: {}",
                        e.message
                    ))
                    .with_path("target_weights".to_string())
                })?;
                let weight = number_to_f64(number)?;
                if weight < 0.0 {
                    return Err(EngineError::domain("target_weights must be non-negative")
                        .with_path("target_weights".to_string()));
                }
                stratum.target_weight = weight;
            }
        }
        Value::Record(record) => {
            for stratum in strata.iter_mut() {
                let raw_weight = record.get(&stratum.name).ok_or_else(|| {
                    EngineError::malformed(format!(
                        "target_weights is missing stratum {:?}",
                        stratum.name
                    ))
                    .with_path("target_weights".to_string())
                })?;
                let number = raw_weight.as_number().map_err(|e| {
                    EngineError::malformed(format!(
                        "target_weights entries must be numbers: {}",
                        e.message
                    ))
                    .with_path("target_weights".to_string())
                })?;
                let weight = number_to_f64(number)?;
                if weight < 0.0 {
                    return Err(EngineError::domain("target_weights must be non-negative")
                        .with_path("target_weights".to_string()));
                }
                stratum.target_weight = weight;
            }
            for key in record.keys() {
                if !strata.iter().any(|stratum| &stratum.name == key) {
                    return Err(EngineError::malformed(format!(
                        "target_weights has unknown stratum {key:?}"
                    ))
                    .with_path("target_weights".to_string()));
                }
            }
        }
        other => {
            return Err(EngineError::malformed(format!(
                "target_weights must be an array or a record, found {}",
                other.kind_name()
            ))
            .with_path("target_weights".to_string()));
        }
    }
    Ok(())
}

fn invoke_prospective(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.prospective_pool")?;
    let original = parse_strata(args, "original")?;
    let additional = parse_strata(args, "additional")?;
    let mut pooled = pool_strata(&original, &additional)?;
    if let Some(raw) = args.get("target_weights")
        && !raw.is_null()
    {
        apply_target_weights(&mut pooled, raw)?;
    }
    let confidence = confidence_param(args)?;
    let analysis = analyze(&pooled, confidence, 1)?;
    let value = build_output(&analysis, 1.0, 0.0, 1, true)?;
    Ok(attach_common(Outcome::approximate(value), &analysis, true))
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

pub fn functions() -> Vec<Arc<dyn bicmath_core::contract::Function>> {
    vec![
        SimpleFunction::arc(stratified_descriptor(), invoke_stratified),
        SimpleFunction::arc(prospective_descriptor(), invoke_prospective),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn campaign_strata() -> serde_json::Value {
        serde_json::json!([
            {
                "name": "beginners",
                "target_weight": 30000,
                "treatment": {
                    "assigned": 8000,
                    "outcomes": [
                        {"value": 100, "count": 320},
                        {"value": -10, "count": 80},
                        {"value": 0, "count": 7600}
                    ]
                },
                "control": {
                    "assigned": 2000,
                    "outcomes": [
                        {"value": 100, "count": 76},
                        {"value": -10, "count": 4},
                        {"value": 0, "count": 1920}
                    ]
                }
            },
            {
                "name": "advanced",
                "target_weight": 10000,
                "treatment": {
                    "assigned": 2000,
                    "outcomes": [
                        {"value": 100, "count": 320},
                        {"value": -10, "count": 80},
                        {"value": 0, "count": 1600}
                    ]
                },
                "control": {
                    "assigned": 8000,
                    "outcomes": [
                        {"value": 100, "count": 1140},
                        {"value": -10, "count": 60},
                        {"value": 0, "count": 6800}
                    ]
                }
            }
        ])
    }

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

    fn field<'a>(value: &'a Value, name: &str) -> &'a Value {
        match value {
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

    fn close(actual: f64, expected: f64, tolerance: f64) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "expected {expected}, got {actual} (tolerance {tolerance})"
        );
    }

    #[test]
    fn campaign_fixture_matches_expected_values() {
        let outcome = call(
            "statistics.stratified_experiment",
            serde_json::json!({
                "strata": campaign_strata(),
                "fixed_cost": 12000,
                "variance_ddof": 1
            }),
        )
        .unwrap();

        // Per-stratum means and differences.
        let strata = field(&outcome.value, "strata");
        let Value::Array(strata) = strata else {
            panic!("strata must be an array");
        };
        let beginners = &strata[0];
        let advanced = &strata[1];
        close(
            as_f64(field(field(beginners, "treatment"), "mean")),
            3.90,
            1e-12,
        );
        close(
            as_f64(field(field(beginners, "control"), "mean")),
            3.78,
            1e-12,
        );
        close(
            as_f64(field(field(beginners, "difference"), "mean")),
            0.12,
            1e-12,
        );
        close(
            as_f64(field(field(advanced, "treatment"), "mean")),
            15.60,
            1e-12,
        );
        close(
            as_f64(field(field(advanced, "control"), "mean")),
            14.175,
            1e-12,
        );
        close(
            as_f64(field(field(advanced, "difference"), "mean")),
            1.425,
            1e-12,
        );

        // Standardized contribution difference.
        let standardized = field(&outcome.value, "standardized");
        close(as_f64(field(standardized, "difference")), 0.44625, 1e-12);
        let se = as_f64(field(standardized, "standard_error"));
        close(se, 0.426_854_469_9, 1e-6 * 0.426_854_469_9);

        // Total contribution and interval over 40000 people.
        let total = 40000.0;
        let total_difference = as_f64(field(standardized, "difference")) * total;
        close(total_difference, 17850.0, 1e-6);
        let total_se = se * total;
        close(total_se, 17_074.178_796, 1e-6 * 17_074.178_796);

        let scaled = field(&outcome.value, "scaled");
        close(as_f64(field(scaled, "total")), 17850.0, 1e-6);
        let net_difference = as_f64(field(scaled, "net_difference"));
        close(net_difference, 5850.0, 1e-9);

        // The published fixture endpoints round the critical value to z = 1.96.
        // Assert that arithmetic within 0.05 as required, then verify the
        // implementation, which uses the exact z = normal_quantile(0.975).
        let published_lower = 5850.0 - 1.96 * total_se;
        let published_upper = 5850.0 + 1.96 * total_se;
        assert!((published_lower - (-27615.39)).abs() < 0.05);
        assert!((published_upper - 39315.39).abs() < 0.05);
        let net_interval = field(scaled, "net_confidence_interval");
        let net_lower = as_f64(field(net_interval, "lower"));
        let net_upper = as_f64(field(net_interval, "upper"));
        let z = 1.959_963_984_540_054;
        close(net_lower, 5850.0 - z * total_se, 1e-9);
        close(net_upper, 5850.0 + z * total_se, 1e-9);

        // Positive, negative, and net-positive rate differences.
        let positive = field(&outcome.value, "positive_rate");
        let negative = field(&outcome.value, "negative_rate");
        let net = field(&outcome.value, "net_positive_rate");
        close(as_f64(field(positive, "difference")), 0.005875, 1e-12);
        close(as_f64(field(negative, "difference")), 0.014125, 1e-12);
        close(as_f64(field(net, "difference")), -0.00825, 1e-12);
        close(as_f64(field(positive, "difference")) * total, 235.0, 1e-9);
        close(as_f64(field(negative, "difference")) * total, 565.0, 1e-9);
        close(as_f64(field(net, "difference")) * total, -330.0, 1e-9);
    }

    #[test]
    fn rollout_fraction_scales_variance_linearly() {
        let outcome = call(
            "statistics.stratified_experiment",
            serde_json::json!({
                "strata": campaign_strata(),
                "fixed_cost": 12000,
                "rollout_fraction": "0.85",
                "variance_ddof": 1
            }),
        )
        .unwrap();
        let full = field(&outcome.value, "standardized");
        let scaled = field(&outcome.value, "scaled");
        let full_se = as_f64(field(full, "standard_error"));
        let scaled_se = as_f64(field(scaled, "standard_error"));
        close(scaled_se, 0.85 * full_se, 1e-12);
        close(as_f64(field(scaled, "difference")), 0.44625 * 0.85, 1e-12);
        close(as_f64(field(scaled, "total")), 17850.0 * 0.85, 1e-9);
        close(
            as_f64(field(scaled, "net_difference")),
            17850.0 * 0.85 - 12000.0,
            1e-9,
        );
        let full_var = as_f64(field(full, "variance"));
        let scaled_var = as_f64(field(scaled, "variance"));
        close(scaled_var, 0.85 * 0.85 * full_var, 1e-12);
    }

    #[test]
    fn observed_mix_mismatch_warns() {
        // Target weights 25/75 but the observed assignment mix is 50/50.
        let outcome = call(
            "statistics.stratified_experiment",
            serde_json::json!({
                "strata": [
                    {
                        "name": "a",
                        "target_weight": 1,
                        "treatment": {"assigned": 8000, "outcomes": [{"value": 1, "count": 8000}]},
                        "control": {"assigned": 2000, "outcomes": [{"value": 1, "count": 2000}]}
                    },
                    {
                        "name": "b",
                        "target_weight": 3,
                        "treatment": {"assigned": 2000, "outcomes": [{"value": 1, "count": 2000}]},
                        "control": {"assigned": 8000, "outcomes": [{"value": 1, "count": 8000}]}
                    }
                ]
            }),
        )
        .unwrap();
        assert!(
            outcome
                .warnings
                .iter()
                .any(|warning| warning.code == "observed_mix_differs_from_target")
        );
        let diagnostics = field(&outcome.value, "diagnostics");
        assert_eq!(
            field(diagnostics, "observed_mix_vs_target"),
            &Value::Bool(false)
        );
    }

    #[test]
    fn ragged_and_zero_margin_tables_are_rejected() {
        let err = call(
            "statistics.chi_square_contingency",
            serde_json::json!({"table": [[1, 2], [3]]}),
        )
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::MalformedInput);
        let err = call(
            "statistics.chi_square_contingency",
            serde_json::json!({"table": [[1, 0], [3, 0]]}),
        )
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::DomainViolation);
    }

    #[test]
    fn count_mismatch_is_rejected() {
        let err = call(
            "statistics.stratified_experiment",
            serde_json::json!({
                "strata": [{
                    "name": "a",
                    "target_weight": 1,
                    "treatment": {"assigned": 3, "outcomes": [{"value": 1, "count": 2}]},
                    "control": {"assigned": 3, "outcomes": [{"value": 1, "count": 3}]}
                }]
            }),
        )
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::DomainViolation);
    }

    #[test]
    fn prospective_pool_pools_counts_and_flags_hypothetical() {
        let original = campaign_strata();
        let additional = serde_json::json!([
            {
                "name": "beginners",
                "target_weight": 0,
                "treatment": {"assigned": 1000, "outcomes": [{"value": 100, "count": 40}, {"value": 0, "count": 960}]},
                "control": {"assigned": 1000, "outcomes": [{"value": 100, "count": 30}, {"value": 0, "count": 970}]}
            },
            {
                "name": "advanced",
                "target_weight": 0,
                "treatment": {"assigned": 1000, "outcomes": [{"value": 100, "count": 200}, {"value": 0, "count": 800}]},
                "control": {"assigned": 1000, "outcomes": [{"value": 100, "count": 150}, {"value": 0, "count": 850}]}
            }
        ]);
        let outcome = call(
            "statistics.prospective_pool",
            serde_json::json!({
                "original": original,
                "additional": additional,
                "target_weights": {"beginners": 30000, "advanced": 10000}
            }),
        )
        .unwrap();
        assert_eq!(field(&outcome.value, "prospective"), &Value::Bool(true));
        assert!(
            outcome
                .warnings
                .iter()
                .any(|warning| warning.code == "prospective_hypothetical_data")
        );
        let strata = field(&outcome.value, "strata");
        let Value::Array(strata) = strata else {
            panic!("strata must be an array");
        };
        let beginners_treatment = field(&strata[0], "treatment");
        assert_eq!(as_f64(field(beginners_treatment, "assigned")), 9000.0);
    }
}
