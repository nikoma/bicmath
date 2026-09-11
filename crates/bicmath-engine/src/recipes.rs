//! Multi-step workflow recipes.
//!
//! Each recipe follows the same shape:
//!
//! 1. `plan` — interpretation guidance for the method families involved.
//! 2. Calculation — registered functions execute the method.
//! 3. `verify` — specific checks that state exactly what they established and
//!    what they did not. There is deliberately no generic `verified: true`.
//! 4. `format` — the returned values rendered as Markdown.
//!
//! Recipes are executed by the engine (which owns the registry) and are
//! reachable through `batch` nodes of type `recipe`.

use serde::Serialize;

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{Assumption, Outcome, Warning};
use bicmath_core::envelope::Exactness;
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::value::Value;

use crate::envelope::{FingerprintInput, build_envelope};
use crate::registry::Registry;
use crate::request::CallOptions;

/// Metadata describing a recipe.
#[derive(Clone, Debug, Serialize)]
pub struct RecipeDescriptor {
    pub name: String,
    pub title: String,
    pub description: String,
    pub parameters: Vec<String>,
    pub composes: Vec<String>,
}

/// All available recipes.
pub fn list() -> Vec<RecipeDescriptor> {
    vec![
        RecipeDescriptor {
            name: "ab_test_full".to_string(),
            title: "Stratified A/B test".to_string(),
            description: "Standardized experiment analysis plus input-count reconciliation, \
                          interpretation notes, and a Markdown summary. Effect estimates and \
                          uncertainty are not independently re-derived."
                .to_string(),
            parameters: vec![
                "strata".into(),
                "confidence".into(),
                "fixed_cost".into(),
                "rollout_fraction".into(),
                "variance_ddof".into(),
            ],
            composes: vec![
                "statistics.stratified_experiment".into(),
                "verify.total".into(),
                "format.to_markdown".into(),
                "plan.interpretation_notes".into(),
            ],
        },
        RecipeDescriptor {
            name: "loan_compare".to_string(),
            title: "Compare loan offers".to_string(),
            description: "Amortizes each offer, reconciles scheduled principal against the \
                          stated principal, renders a comparison table, and states the rate \
                          and rounding conventions."
                .to_string(),
            parameters: vec!["loans".into(), "payment_timing".into(), "rounding".into()],
            composes: vec![
                "finance.amortization".into(),
                "verify.total".into(),
                "format.to_markdown".into(),
                "plan.interpretation_notes".into(),
            ],
        },
        RecipeDescriptor {
            name: "cohort_ltv".to_string(),
            title: "Cohort retention and revenue".to_string(),
            description: "Cohort retention and revenue projection with a total-revenue \
                          reconciliation and a Markdown table."
                .to_string(),
            parameters: vec!["cohort_sizes".into(), "revenue_per_user".into()],
            composes: vec![
                "business.retention_rates".into(),
                "business.cohort_revenue".into(),
                "verify.total".into(),
                "format.to_markdown".into(),
                "plan.interpretation_notes".into(),
            ],
        },
        RecipeDescriptor {
            name: "budget_scenario".to_string(),
            title: "Budget scenario analysis".to_string(),
            description: "Unit economics at several volumes, with an exact check that the \
                          reported whole-unit break-even covers fixed costs and that one \
                          fewer unit does not."
                .to_string(),
            parameters: vec![
                "price".into(),
                "unit_variable_cost".into(),
                "fixed_costs".into(),
                "volumes".into(),
            ],
            composes: vec![
                "business.unit_economics".into(),
                "verify.inequality".into(),
                "format.to_markdown".into(),
                "plan.interpretation_notes".into(),
            ],
        },
    ]
}

/// Whether a recipe name exists.
pub fn exists(name: &str) -> bool {
    list().iter().any(|recipe| recipe.name == name)
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn call(
    registry: &Registry,
    function: &str,
    arguments: serde_json::Value,
    ctx: &ExecContext,
) -> Result<(Value, Exactness, Vec<Warning>, Vec<Assumption>), EngineError> {
    let options = CallOptions::default();
    let envelope = registry.call(function, &arguments, ctx, &options)?;
    Ok((
        envelope.result,
        envelope.exactness,
        envelope.warnings,
        envelope.assumptions,
    ))
}

fn record(entries: Vec<(&str, Value)>) -> Value {
    Value::Record(
        entries
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect(),
    )
}

fn text(value: impl Into<String>) -> Value {
    Value::text(value)
}

fn field<'a>(value: &'a Value, name: &str) -> Result<&'a Value, EngineError> {
    value
        .as_record()?
        .get(name)
        .ok_or_else(|| EngineError::internal(format!("recipe intermediate missing field {name:?}")))
}

fn required<'a>(
    arguments: &'a serde_json::Map<String, serde_json::Value>,
    name: &str,
) -> Result<&'a serde_json::Value, EngineError> {
    arguments.get(name).ok_or_else(|| {
        EngineError::malformed(format!("recipe is missing required argument {name:?}"))
            .with_path(name.to_string())
    })
}

fn json_value(value: &Value) -> Result<serde_json::Value, EngineError> {
    serde_json::to_value(value)
        .map_err(|error| EngineError::internal(format!("value is not serializable: {error}")))
}

/// Extract the exact numeric amount from a money value.
fn money_amount(value: &Value) -> Result<Value, EngineError> {
    let (amount, _currency) = value.as_money()?;
    Ok(Value::Number(amount.clone()))
}

/// Call `verify.total` and return `(confirmed, verification_record)`.
fn verify_total(
    registry: &Registry,
    values: Vec<Value>,
    claimed: Value,
    ctx: &ExecContext,
) -> Result<(bool, Value), EngineError> {
    let (result, _, _, _) = call(
        registry,
        "verify.total",
        serde_json::json!({
            "values": values,
            "claimed_total": json_value(&claimed)?,
        }),
        ctx,
    )?;
    let confirmed = field(&result, "status")?.as_text()? == "confirmed";
    Ok((confirmed, result))
}

/// Call `verify.inequality` and return `(confirmed, verification_record)`.
fn verify_inequality(
    registry: &Registry,
    left: &str,
    relation: &str,
    right: &str,
    bindings: serde_json::Value,
    ctx: &ExecContext,
) -> Result<(bool, Value), EngineError> {
    let (result, _, _, _) = call(
        registry,
        "verify.inequality",
        serde_json::json!({
            "left": left,
            "relation": relation,
            "right": right,
            "bindings": bindings,
        }),
        ctx,
    )?;
    let confirmed = field(&result, "status")?.as_text()? == "confirmed";
    Ok((confirmed, result))
}

fn verification_check(
    check: &str,
    status: &str,
    established: &str,
    not_established: &[&str],
    evidence: Value,
) -> Value {
    record(vec![
        ("check", text(check)),
        ("status", text(status)),
        ("established", text(established)),
        (
            "not_established",
            Value::Array(not_established.iter().map(|item| text(*item)).collect()),
        ),
        ("evidence", evidence),
    ])
}

fn plan_notes(
    registry: &Registry,
    functions: &[&str],
    ctx: &ExecContext,
) -> Result<Value, EngineError> {
    let (result, _, _, _) = call(
        registry,
        "plan.interpretation_notes",
        serde_json::json!({ "functions": functions }),
        ctx,
    )?;
    Ok(result)
}

fn markdown(
    registry: &Registry,
    value: Value,
    title: &str,
    ctx: &ExecContext,
) -> Result<Value, EngineError> {
    let (result, _, _, _) = call(
        registry,
        "format.to_markdown",
        serde_json::json!({ "value": json_value(&value)?, "title": title }),
        ctx,
    )?;
    Ok(result)
}

// ---------------------------------------------------------------------------
// Execution
// ---------------------------------------------------------------------------

/// Execute a recipe by name.
pub fn run(
    registry: &Registry,
    name: &str,
    arguments: &serde_json::Value,
    ctx: &ExecContext,
    options: &CallOptions,
) -> Result<bicmath_core::envelope::ResultEnvelope, EngineError> {
    let empty = serde_json::Map::new();
    let map = match arguments {
        serde_json::Value::Null => &empty,
        serde_json::Value::Object(map) => map,
        other => {
            return Err(EngineError::malformed(format!(
                "recipe arguments must be a JSON object, found {other}"
            )));
        }
    };
    let (value, exactness, warnings, assumptions) = match name {
        "ab_test_full" => ab_test_full(registry, map, ctx)?,
        "loan_compare" => loan_compare(registry, map, ctx)?,
        "cohort_ltv" => cohort_ltv(registry, map, ctx)?,
        "budget_scenario" => budget_scenario(registry, map, ctx)?,
        other => {
            return Err(EngineError::new(
                ErrorCode::NotFound,
                format!(
                    "unknown recipe {other:?}; available: {:?}",
                    list().iter().map(|r| r.name.clone()).collect::<Vec<_>>()
                ),
            ));
        }
    };
    let mut outcome = Outcome::new(value, exactness);
    outcome.warnings = warnings;
    outcome.assumptions = assumptions;
    outcome.assumptions.push(Assumption::unverified(
        "recipe_composition",
        "this result composes multiple methods; inspect each method's assumptions in the \
         referenced function outputs",
    ));
    let request = serde_json::json!({"recipe": name, "arguments": arguments});
    build_envelope(
        registry,
        None,
        outcome,
        ctx,
        options,
        FingerprintInput::Raw { request: &request },
    )
}

fn ab_test_full(
    registry: &Registry,
    arguments: &serde_json::Map<String, serde_json::Value>,
    ctx: &ExecContext,
) -> Result<(Value, Exactness, Vec<Warning>, Vec<Assumption>), EngineError> {
    let strata = required(arguments, "strata")?;
    let mut request = serde_json::json!({ "strata": strata });
    for key in [
        "confidence",
        "fixed_cost",
        "rollout_fraction",
        "variance_ddof",
    ] {
        if let Some(value) = arguments.get(key) {
            request[key] = value.clone();
        }
    }
    let (analysis, exactness, warnings, assumptions) =
        call(registry, "statistics.stratified_experiment", request, ctx)?;

    // Verify exactly what the inputs can establish: each arm's outcome counts
    // sum to its assigned count. This does not re-derive the effect estimates
    // or their uncertainty.
    let mut checks = Vec::new();
    if let Some(strata) = strata.as_array() {
        for (index, stratum) in strata.iter().enumerate() {
            let name = stratum
                .get("name")
                .and_then(|value| value.as_str())
                .unwrap_or("stratum");
            for arm in ["treatment", "control"] {
                let Some(arm_value) = stratum.get(arm) else {
                    continue;
                };
                let assigned = arm_value.get("assigned").cloned().unwrap_or_default();
                let counts: Vec<serde_json::Value> = arm_value
                    .get("outcomes")
                    .and_then(|value| value.as_array())
                    .map(|outcomes| {
                        outcomes
                            .iter()
                            .filter_map(|outcome| outcome.get("count").cloned())
                            .collect()
                    })
                    .unwrap_or_default();
                let (confirmed, evidence) = verify_total(
                    registry,
                    counts
                        .iter()
                        .map(|count| Value::from_json(count, &ctx.limits))
                        .collect::<Result<Vec<_>, _>>()?,
                    Value::from_json(&assigned, &ctx.limits)?,
                    ctx,
                )?;
                checks.push(verification_check(
                    &format!("input_count_reconciliation[{index}:{name}:{arm}]"),
                    if confirmed { "confirmed" } else { "refuted" },
                    "the outcome counts for this arm sum exactly to its assigned count",
                    &[
                        "effect estimates and uncertainty were not independently verified",
                        "causal validity, randomization, independence, and comparability are not established",
                        "outcome completeness and refund-window completeness are supplied assumptions",
                    ],
                    evidence,
                ));
            }
        }
    }

    let standardized = field(&analysis, "standardized")?;
    let scaled = field(&analysis, "scaled")?;
    let metrics = Value::Array(vec![
        record(vec![
            ("metric", text("standardized difference")),
            ("value", field(standardized, "difference")?.clone()),
        ]),
        record(vec![
            ("metric", text("standard error")),
            ("value", field(standardized, "standard_error")?.clone()),
        ]),
        record(vec![
            ("metric", text("confidence interval")),
            ("value", field(standardized, "confidence_interval")?.clone()),
        ]),
        record(vec![
            ("metric", text("scaled total")),
            ("value", field(scaled, "total")?.clone()),
        ]),
        record(vec![
            ("metric", text("net after fixed cost")),
            ("value", field(scaled, "net_difference")?.clone()),
        ]),
    ]);
    let presentation = markdown(registry, metrics, "Experiment summary", ctx)?;
    let plan = plan_notes(registry, &["statistics.stratified_experiment"], ctx)?;

    let value = record(vec![
        ("recipe", text("ab_test_full")),
        ("plan", plan),
        ("calculation", analysis),
        ("verification", Value::Array(checks)),
        ("presentation", presentation),
        (
            "conventions",
            Value::Array(vec![
                text("standardized estimates use the supplied target mix, not the observed mix"),
                text("the interval is a normal approximation around the standardized difference"),
                text(
                    "a known fixed cost shifts the estimate and interval without adding sampling variance",
                ),
            ]),
        ),
    ]);
    Ok((value, exactness, warnings, assumptions))
}

fn loan_compare(
    registry: &Registry,
    arguments: &serde_json::Map<String, serde_json::Value>,
    ctx: &ExecContext,
) -> Result<(Value, Exactness, Vec<Warning>, Vec<Assumption>), EngineError> {
    let loans = required(arguments, "loans")?
        .as_array()
        .ok_or_else(|| EngineError::malformed("loans must be an array"))?
        .clone();
    if loans.is_empty() {
        return Err(EngineError::malformed("loans must not be empty"));
    }
    let mut results = Vec::new();
    let mut checks = Vec::new();
    let mut table = Vec::new();
    let mut best_index = 0usize;
    let mut best_interest: Option<f64> = None;
    let mut warnings = Vec::new();
    let mut assumptions = Vec::new();
    let mut exactness = Exactness::Exact;
    for (index, loan) in loans.iter().enumerate() {
        let mut request = loan.clone();
        for key in ["payment_timing", "rounding"] {
            if let Some(value) = arguments.get(key) {
                request[key] = value.clone();
            }
        }
        let (amortization, arm_exactness, arm_warnings, arm_assumptions) =
            call(registry, "finance.amortization", request, ctx)?;
        exactness = exactness.combine(arm_exactness);
        warnings.extend(arm_warnings);
        assumptions.extend(arm_assumptions);

        let reconciliation = field(&amortization, "reconciliation")?;
        let total_principal = field(reconciliation, "total_principal")?.clone();
        let total_interest = field(reconciliation, "total_interest")?.clone();
        let total_payments = field(reconciliation, "total_payments")?.clone();
        let reconciles = field(reconciliation, "reconciles")?.clone();

        // Reconcile the sum of scheduled principal payments against the stated
        // total principal. This establishes arithmetic consistency of the
        // schedule; it does not validate the rate, timing, or fee model.
        let schedule = field(&amortization, "schedule")?
            .as_array()
            .map_err(|_| EngineError::internal("amortization schedule is not an array"))?;
        let principal_values: Vec<Value> = schedule
            .iter()
            .map(|row| money_amount(field(row, "principal")?))
            .collect::<Result<Vec<_>, _>>()?;
        let claimed = money_amount(&total_principal)?;
        let (confirmed, evidence) = verify_total(registry, principal_values, claimed, ctx)?;
        checks.push(verification_check(
            &format!("principal_reconciliation[offer:{index}]"),
            if confirmed { "confirmed" } else { "refuted" },
            "the sum of scheduled principal payments equals the stated total principal for this offer",
            &[
                "payment accuracy beyond the stated rate, payment timing, and rounding conventions",
                "fees, insurance, taxes, or penalties that were not supplied",
                "lender-specific day-count, settlement, or prepayment rules",
            ],
            evidence,
        ));

        if let Some(interest) = total_interest
            .as_money()
            .ok()
            .and_then(|(amount, _)| amount.to_f64())
            && (best_interest.is_none() || interest < best_interest.unwrap())
        {
            best_interest = Some(interest);
            best_index = index;
        }
        table.push(record(vec![
            (
                "offer",
                Value::integer(num_bigint::BigInt::from(index as i64)),
            ),
            (
                "principal",
                Value::from_json(
                    loan.get("principal").unwrap_or(&serde_json::Value::Null),
                    &ctx.limits,
                )?,
            ),
            (
                "rate",
                Value::from_json(
                    loan.get("annual_rate").unwrap_or(&serde_json::Value::Null),
                    &ctx.limits,
                )?,
            ),
            (
                "periods",
                Value::from_json(
                    loan.get("periods").unwrap_or(&serde_json::Value::Null),
                    &ctx.limits,
                )?,
            ),
            ("total_payments", total_payments),
            ("total_interest", total_interest),
            ("reconciles", reconciles),
        ]));
        results.push(record(vec![
            (
                "index",
                Value::integer(num_bigint::BigInt::from(index as i64)),
            ),
            ("amortization", amortization),
        ]));
    }
    let presentation = markdown(
        registry,
        Value::Array(table),
        "Loan comparison (per-period rate)",
        ctx,
    )?;
    let plan = plan_notes(registry, &["finance.amortization"], ctx)?;
    let value = record(vec![
        ("recipe", text("loan_compare")),
        ("plan", plan),
        ("loans", Value::Array(results)),
        ("verification", Value::Array(checks)),
        ("presentation", presentation),
        (
            "cheapest_by_total_interest",
            Value::integer(num_bigint::BigInt::from(best_index as i64)),
        ),
        (
            "conventions",
            Value::Array(vec![
                text(
                    "`annual_rate` is the rate applied per payment period in this function; multiply by the number of periods per year for an annual rate",
                ),
                text("the default rounding is per-period at the currency settlement scale"),
                text(
                    "the default payment timing is ordinary (end of period); annuity_due shifts payments to the start",
                ),
                text(
                    "total interest ranks offers here; total payments or present value may rank them differently",
                ),
            ]),
        ),
    ]);
    Ok((value, exactness, warnings, assumptions))
}

fn cohort_ltv(
    registry: &Registry,
    arguments: &serde_json::Map<String, serde_json::Value>,
    ctx: &ExecContext,
) -> Result<(Value, Exactness, Vec<Warning>, Vec<Assumption>), EngineError> {
    let cohort_sizes = required(arguments, "cohort_sizes")?;
    let revenue_per_user = required(arguments, "revenue_per_user")?;
    let (retention, ret_exactness, mut warnings, assumptions) = call(
        registry,
        "business.retention_rates",
        serde_json::json!({ "cohort_sizes": cohort_sizes }),
        ctx,
    )?;
    let (revenue, rev_exactness, rev_warnings, rev_assumptions) = call(
        registry,
        "business.cohort_revenue",
        serde_json::json!({
            "cohort_sizes": cohort_sizes,
            "revenue_per_user": revenue_per_user
        }),
        ctx,
    )?;
    warnings.extend(rev_warnings);
    let mut assumptions = assumptions;
    assumptions.extend(rev_assumptions);

    // Reconcile the reported total against the sum of per-cohort revenue.
    let per_cohort = field(&revenue, "revenue_per_cohort")?;
    let total = field(&revenue, "total_revenue")?.clone();
    let values = per_cohort.as_array()?.to_vec();
    let (confirmed, evidence) = verify_total(registry, values, total, ctx)?;
    let checks = vec![verification_check(
        "cohort_revenue_total",
        if confirmed { "confirmed" } else { "refuted" },
        "the reported total revenue equals the exact sum of the per-cohort revenues",
        &[
            "retention forecasts and the revenue-per-user assumption",
            "discounting, churn timing within periods, and seasonality",
            "whether the supplied cohorts are complete or comparable",
        ],
        evidence,
    )];
    let presentation = markdown(
        registry,
        field(&revenue, "revenue_per_cohort")?.clone(),
        "Revenue per cohort",
        ctx,
    )?;
    let plan = plan_notes(registry, &["business.cohort_revenue"], ctx)?;
    let value = record(vec![
        ("recipe", text("cohort_ltv")),
        ("plan", plan),
        ("retention", retention),
        ("revenue", revenue),
        ("verification", Value::Array(checks)),
        ("presentation", presentation),
        (
            "notes",
            Value::Array(vec![text(
                "revenue projections assume the supplied per-user revenue applies to every \
                 retained user and ignore discounting unless modeled explicitly",
            )]),
        ),
    ]);
    Ok((
        value,
        ret_exactness.combine(rev_exactness),
        warnings,
        assumptions,
    ))
}

fn budget_scenario(
    registry: &Registry,
    arguments: &serde_json::Map<String, serde_json::Value>,
    ctx: &ExecContext,
) -> Result<(Value, Exactness, Vec<Warning>, Vec<Assumption>), EngineError> {
    let price = required(arguments, "price")?.clone();
    let unit_variable_cost = required(arguments, "unit_variable_cost")?.clone();
    let fixed_costs = required(arguments, "fixed_costs")?.clone();
    let volumes = required(arguments, "volumes")?
        .as_array()
        .ok_or_else(|| EngineError::malformed("volumes must be an array"))?
        .clone();
    if volumes.is_empty() {
        return Err(EngineError::malformed("volumes must not be empty"));
    }
    let mut scenarios = Vec::new();
    let mut checks = Vec::new();
    let mut table = Vec::new();
    let mut warnings = Vec::new();
    let mut assumptions = Vec::new();
    let mut exactness = Exactness::Exact;
    for volume in &volumes {
        let (economics, arm_exactness, arm_warnings, arm_assumptions) = call(
            registry,
            "business.unit_economics",
            serde_json::json!({
                "price": price,
                "unit_variable_cost": unit_variable_cost,
                "fixed_costs": fixed_costs,
                "volume": volume
            }),
            ctx,
        )?;
        exactness = exactness.combine(arm_exactness);
        warnings.extend(arm_warnings);
        assumptions.extend(arm_assumptions);
        let break_even_units = field(&economics, "break_even_units")?.clone();
        let unit_contribution = field(&economics, "unit_contribution")?.clone();
        let bindings = serde_json::json!({
            "break_even_units": json_value(&break_even_units)?,
            "price": price,
            "unit_variable_cost": unit_variable_cost,
            "fixed_costs": fixed_costs,
        });

        // Exact whole-unit coverage: at the reported break-even quantity the
        // contribution covers fixed costs, and at one unit fewer it does not.
        let (covers, covers_evidence) = verify_inequality(
            registry,
            "break_even_units * (price - unit_variable_cost)",
            "ge",
            "fixed_costs",
            bindings.clone(),
            ctx,
        )?;
        checks.push(verification_check(
            &format!("break_even_unit_coverage[volume:{}]", volume),
            if covers { "confirmed" } else { "refuted" },
            "at the reported break-even units, total contribution covers fixed costs exactly (>=)",
            &[
                "demand at that volume",
                "cost variability, nonlinear pricing, or capacity limits",
                "whether fixed costs are truly fixed over the volume range",
            ],
            covers_evidence,
        ));
        let one_fewer = break_even_units
            .as_number()
            .ok()
            .and_then(|number| number.to_f64())
            .is_some_and(|value| value >= 1.0);
        if one_fewer {
            let (minimal, minimal_evidence) = verify_inequality(
                registry,
                "(break_even_units - 1) * (price - unit_variable_cost)",
                "lt",
                "fixed_costs",
                bindings,
                ctx,
            )?;
            checks.push(verification_check(
                &format!("break_even_minimality[volume:{}]", volume),
                if minimal { "confirmed" } else { "refuted" },
                "one unit fewer than the reported break-even does not cover fixed costs (<)",
                &[
                    "that break-even is the smallest possible whole-unit quantity for other cost schedules",
                ],
                minimal_evidence,
            ));
        }
        table.push(record(vec![
            ("volume", Value::from_json(volume, &ctx.limits)?),
            ("unit_contribution", unit_contribution),
            ("gross_profit", field(&economics, "gross_profit")?.clone()),
            ("break_even_units", break_even_units),
            (
                "margin_of_safety_units",
                field(&economics, "margin_of_safety_units")?.clone(),
            ),
        ]));
        scenarios.push(record(vec![
            ("volume", Value::from_json(volume, &ctx.limits)?),
            ("economics", economics),
        ]));
    }
    let presentation = markdown(
        registry,
        Value::Array(table),
        "Budget scenarios (exact decimal inputs)",
        ctx,
    )?;
    let plan = plan_notes(registry, &["business.unit_economics"], ctx)?;
    let value = record(vec![
        ("recipe", text("budget_scenario")),
        ("plan", plan),
        ("scenarios", Value::Array(scenarios)),
        ("verification", Value::Array(checks)),
        ("presentation", presentation),
        (
            "notes",
            Value::Array(vec![text(
                "scenario outputs are conditional on the supplied price, cost, and volume \
                 assumptions; they are not forecasts",
            )]),
        ),
    ]);
    Ok((value, exactness, warnings, assumptions))
}
