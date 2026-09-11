//! End-to-end tests for the extension modules, workflow recipes, budget
//! presets, and dimensional checking.

use bicmath_core::envelope::Exactness;
use bicmath_core::value::Value;
use bicmath_engine::{
    BatchRequest, CallRequest, Engine, EngineConfig, EvaluateRequest, FunctionFilter,
};

fn engine() -> Engine {
    Engine::full(EngineConfig::default()).expect("engine builds")
}

fn call(
    engine: &Engine,
    function: &str,
    arguments: serde_json::Value,
) -> Result<bicmath_core::envelope::ResultEnvelope, bicmath_core::error::EngineError> {
    let request: CallRequest =
        serde_json::from_value(serde_json::json!({"function": function, "arguments": arguments}))
            .expect("request");
    engine.call(&request, &engine.base_context())
}

fn call_ctx(
    engine: &Engine,
    function: &str,
    arguments: serde_json::Value,
    context: serde_json::Value,
) -> Result<bicmath_core::envelope::ResultEnvelope, bicmath_core::error::EngineError> {
    let request: CallRequest = serde_json::from_value(
        serde_json::json!({"function": function, "arguments": arguments, "context": context}),
    )
    .expect("request");
    engine.call(&request, &engine.base_context())
}

fn field<'a>(value: &'a Value, name: &str) -> &'a Value {
    value
        .as_record()
        .expect("record")
        .get(name)
        .unwrap_or_else(|| panic!("field {name:?} present"))
}

#[test]
fn all_extension_modules_are_registered() {
    let engine = engine();
    let modules: Vec<String> = engine
        .list_modules()
        .into_iter()
        .map(|module| module.id)
        .collect();
    for expected in [
        "interval", "verify", "symbolic", "optimize", "algebra", "geometry", "business", "format",
        "plan",
    ] {
        assert!(
            modules.contains(&expected.to_string()),
            "missing {expected}"
        );
    }
    let total = engine.list_functions(&FunctionFilter::default()).total;
    assert!(total > 250, "expected a broad registry, found {total}");
}

#[test]
fn every_documented_example_still_executes() {
    let engine = engine();
    let failures = engine.validate_examples();
    assert!(failures.is_empty(), "example failures: {failures:#?}");
}

#[test]
fn interval_enclosure_contains_the_exact_value() {
    let engine = engine();
    let envelope = call(
        &engine,
        "interval.evaluate",
        serde_json::json!({
            "expression": "x^2 - 2",
            "variables": {
                "x": {"lo": {"kind": "integer", "value": "1"}, "hi": {"kind": "integer", "value": "2"}}
            }
        }),
    )
    .unwrap();
    let lo = field(&envelope.result, "lo")
        .as_number()
        .unwrap()
        .to_f64()
        .unwrap();
    let hi = field(&envelope.result, "hi")
        .as_number()
        .unwrap()
        .to_f64()
        .unwrap();
    let root = 2f64.sqrt();
    assert!(
        lo <= root && root <= hi,
        "enclosure [{lo}, {hi}] misses {root}"
    );
}

#[test]
fn verify_confirms_and_refutes_claims() {
    let engine = engine();
    let confirmed = call(
        &engine,
        "verify.equality",
        serde_json::json!({"left": "0.1 + 0.2", "right": "0.3"}),
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(field(&confirmed.result, "status")).unwrap(),
        serde_json::json!("confirmed")
    );
    let refuted = call(
        &engine,
        "verify.equality",
        serde_json::json!({"left": "1/3", "right": "0.333"}),
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(field(&refuted.result, "status")).unwrap(),
        serde_json::json!("refuted")
    );
}

#[test]
fn symbolic_derivative_runs_through_the_engine() {
    let engine = engine();
    let envelope = call(
        &engine,
        "symbolic.derivative",
        serde_json::json!({"expression": "x^2", "variable": "x"}),
    )
    .unwrap();
    let printed = envelope.result.as_text().unwrap().to_string();
    assert!(printed.contains('x'), "unexpected derivative {printed:?}");
}

#[test]
fn exact_linear_program_solves_with_duality() {
    let engine = engine();
    let envelope = call(
        &engine,
        "optimize.linear_program",
        serde_json::json!({
            "objective": [3, 5],
            "constraints": [
                {"coefficients": [1, 0], "relation": "le", "rhs": 4},
                {"coefficients": [0, 2], "relation": "le", "rhs": 12},
                {"coefficients": [3, 2], "relation": "le", "rhs": 18}
            ]
        }),
    )
    .unwrap();
    assert_eq!(envelope.exactness, Exactness::Exact);
    let x = field(&envelope.result, "x").as_array().unwrap();
    assert_eq!(x[0].as_number().unwrap().to_string(), "2");
    assert_eq!(x[1].as_number().unwrap().to_string(), "6");
    assert_eq!(
        field(&envelope.result, "objective")
            .as_number()
            .unwrap()
            .to_string(),
        "36"
    );
}

#[test]
fn algebra_number_theory_and_polynomials() {
    let engine = engine();
    let prime = call(
        &engine,
        "algebra.is_prime",
        serde_json::json!({"n": {"kind": "integer", "value": "2305843009213693951"}}),
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(&prime.result).unwrap(),
        serde_json::json!(true)
    );
    let roots = call(
        &engine,
        "algebra.polynomial_roots",
        serde_json::json!({"coefficients": [2, -3, 1]}),
    )
    .unwrap();
    assert!(!roots.result.as_array().unwrap().is_empty());
}

#[test]
fn geometry_and_business_run() {
    let engine = engine();
    let triangle = call(
        &engine,
        "geometry.triangle_solve",
        serde_json::json!({"givens": {"a": 3, "b": 4, "angle_c": 90}}),
    )
    .unwrap();
    assert!(
        (field(&triangle.result, "area")
            .as_number()
            .unwrap()
            .to_f64()
            .unwrap()
            - 6.0)
            .abs()
            < 1e-9
    );
    let eoq = call(
        &engine,
        "business.eoq",
        serde_json::json!({
            "annual_demand": 10000,
            "order_cost": 50,
            "holding_cost_per_unit": 2
        }),
    )
    .unwrap();
    let eoq_value = field(&eoq.result, "eoq")
        .as_number()
        .unwrap()
        .to_f64()
        .unwrap();
    assert!((eoq_value - 707.1067811865476).abs() < 1e-6);
}

#[test]
fn format_renders_markdown_and_csv() {
    let engine = engine();
    let markdown = call(
        &engine,
        "format.to_markdown",
        serde_json::json!({
            "value": {"kind": "matrix", "rows": 2, "cols": 2, "data": [1, 2, 3, 4]}
        }),
    )
    .unwrap();
    let text = field(&markdown.result, "markdown").as_text().unwrap();
    assert!(text.contains('|') && text.contains("---"));
    let csv = call(
        &engine,
        "format.to_csv",
        serde_json::json!({"value": [["a", "b"], ["c", "d"]]}),
    )
    .unwrap();
    assert!(field(&csv.result, "csv").as_text().unwrap().contains("a,b"));
}

#[test]
fn plan_recommends_real_functions() {
    let engine = engine();
    let envelope = call(
        &engine,
        "plan.recommend",
        serde_json::json!({"task": "compare_proportions"}),
    )
    .unwrap();
    let recommended = field(&envelope.result, "recommended_functions")
        .as_array()
        .unwrap();
    assert!(!recommended.is_empty());
    let known: Vec<String> = engine
        .registry()
        .function_descriptors()
        .map(|descriptor| descriptor.id.clone())
        .collect();
    for function in recommended {
        let id = function.as_text().unwrap();
        assert!(
            known.contains(&id.to_string()),
            "plan recommends unknown {id}"
        );
    }
}

#[test]
fn sequential_and_bayesian_statistics_run() {
    let engine = engine();
    let sequence = call(
        &engine,
        "statistics.confidence_sequence_mean",
        serde_json::json!({"values": [1, 2, 3, 4, 5], "lower": 0, "upper": 10}),
    )
    .unwrap();
    let lower = field(&sequence.result, "lower")
        .as_number()
        .unwrap()
        .to_f64()
        .unwrap();
    let upper = field(&sequence.result, "upper")
        .as_number()
        .unwrap()
        .to_f64()
        .unwrap();
    assert!(lower <= 3.0 && 3.0 <= upper);
    let posterior = call(
        &engine,
        "statistics.beta_binomial_update",
        serde_json::json!({
            "successes": 3, "trials": 10, "prior_alpha": 1, "prior_beta": 1
        }),
    )
    .unwrap();
    let mean = field(&posterior.result, "posterior_mean")
        .as_number()
        .unwrap()
        .to_f64()
        .unwrap();
    assert!((mean - 4.0 / 12.0).abs() < 1e-12);
}

#[test]
fn finance_derivatives_and_portfolio_run() {
    let engine = engine();
    let option = call(
        &engine,
        "finance.black_scholes",
        serde_json::json!({
            "option_type": "call", "spot": 100, "strike": 100,
            "rate": "0.05", "volatility": "0.2", "time": 1
        }),
    )
    .unwrap();
    let price = field(&option.result, "price")
        .as_number()
        .unwrap()
        .to_f64()
        .unwrap();
    assert!((price - 10.450583572185565).abs() < 1e-9, "price {price}");
    let var = call(
        &engine,
        "finance.value_at_risk",
        serde_json::json!({"returns": [0.01, -0.02, 0.03, -0.01, 0.0], "confidence": "0.95"}),
    )
    .unwrap();
    assert!(
        field(&var.result, "var")
            .as_number()
            .unwrap()
            .to_f64()
            .unwrap()
            >= 0.0
    );
}

#[test]
fn dimensional_check_catches_unit_errors() {
    let engine = engine();
    let ok = call(
        &engine,
        "units.check",
        serde_json::json!({
            "expression": "distance / time",
            "bindings": {
                "distance": {"kind": "quantity", "value": 100, "dimension": {"length": 1}},
                "time": {"kind": "quantity", "value": 10, "dimension": {"time": 1}}
            }
        }),
    )
    .unwrap();
    assert_eq!(field(&ok.result, "symbol").as_text().unwrap(), "m*s^-1");
    let error = call(
        &engine,
        "units.check",
        serde_json::json!({
            "expression": "mass + distance",
            "bindings": {
                "mass": {"kind": "quantity", "value": 1, "dimension": {"mass": 1}},
                "distance": {"kind": "quantity", "value": 1, "dimension": {"length": 1}}
            }
        }),
    )
    .unwrap_err();
    assert_eq!(error.code, bicmath_core::ErrorCode::IncompatibleUnits);
}

#[test]
fn budget_preset_changes_effective_precision() {
    let engine = engine();
    let fast = call_ctx(
        &engine,
        "arithmetic.div",
        serde_json::json!({
            "a": {"kind": "decimal", "value": "1"},
            "b": {"kind": "decimal", "value": "3"}
        }),
        serde_json::json!({"budget": "fast"}),
    )
    .unwrap();
    assert_eq!(fast.context.numeric.precision, 15);
    let precise = call_ctx(
        &engine,
        "arithmetic.div",
        serde_json::json!({
            "a": {"kind": "decimal", "value": "1"},
            "b": {"kind": "decimal", "value": "3"}
        }),
        serde_json::json!({"budget": "precise"}),
    )
    .unwrap();
    assert_eq!(precise.context.numeric.precision, 100);
}

#[test]
fn batch_recipe_node_runs_a_full_workflow() {
    let engine = engine();
    let strata = serde_json::json!([
        {
            "name": "beginners",
            "target_weight": 30000,
            "treatment": {"assigned": 8000, "outcomes": [
                {"value": 100, "count": 320}, {"value": -10, "count": 80}, {"value": 0, "count": 7600}]},
            "control": {"assigned": 2000, "outcomes": [
                {"value": 100, "count": 76}, {"value": -10, "count": 4}, {"value": 0, "count": 1920}]}
        },
        {
            "name": "advanced",
            "target_weight": 10000,
            "treatment": {"assigned": 2000, "outcomes": [
                {"value": 100, "count": 320}, {"value": -10, "count": 80}, {"value": 0, "count": 1600}]},
            "control": {"assigned": 8000, "outcomes": [
                {"value": 100, "count": 1140}, {"value": -10, "count": 60}, {"value": 0, "count": 6800}]}
        }
    ]);
    let request: BatchRequest = serde_json::from_value(serde_json::json!({
        "nodes": [
            {"type": "recipe", "name": "experiment", "recipe": "ab_test_full",
             "arguments": {"strata": strata, "fixed_cost": 12000}}
        ],
        "outputs": ["experiment"]
    }))
    .unwrap();
    let result = engine.batch(&request, &engine.base_context()).unwrap();
    assert_eq!(result.outputs[0].status, bicmath_engine::NodeStatus::Ok);
    let envelope = result.outputs[0].result.as_ref().unwrap();
    let calculation = field(&envelope.result, "calculation");
    let net = field(field(calculation, "scaled"), "net_difference");
    assert!((net.as_number().unwrap().to_f64().unwrap() - 5850.0).abs() < 1e-6);
    // Verification states exactly what it established.
    let checks = field(&envelope.result, "verification").as_array().unwrap();
    assert!(!checks.is_empty());
    for check in checks {
        let status = field(check, "status").as_text().unwrap();
        assert!(status == "confirmed" || status == "refuted");
        assert!(!field(check, "established").as_text().unwrap().is_empty());
        assert!(
            !field(check, "not_established")
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
    // Presentation and plan blocks are present.
    assert!(
        field(field(&envelope.result, "presentation"), "markdown")
            .as_text()
            .unwrap()
            .contains('|')
    );
    assert!(field(&envelope.result, "plan").as_record().is_ok());
}

#[test]
fn loan_recipe_verifies_principal_reconciliation() {
    let engine = engine();
    let request: BatchRequest = serde_json::from_value(serde_json::json!({
        "nodes": [{
            "type": "recipe", "name": "loans", "recipe": "loan_compare",
            "arguments": {"loans": [
                {"principal": {"kind": "money", "amount": {"kind": "decimal", "value": "1200.00"}, "currency": "USD"},
                 "annual_rate": "0.005", "periods": 12, "currency": "USD"}
            ]}
        }],
        "outputs": ["loans"]
    }))
    .unwrap();
    let result = engine.batch(&request, &engine.base_context()).unwrap();
    let envelope = result.outputs[0].result.as_ref().unwrap();
    let checks = field(&envelope.result, "verification").as_array().unwrap();
    assert!(checks.iter().any(|check| {
        field(check, "check")
            .as_text()
            .unwrap()
            .starts_with("principal_reconciliation")
    }));
    assert!(
        field(field(&envelope.result, "presentation"), "markdown")
            .as_text()
            .unwrap()
            .contains("total_payments")
    );
}

#[test]
fn budget_recipe_verifies_break_even_minimality() {
    let engine = engine();
    let request: BatchRequest = serde_json::from_value(serde_json::json!({
        "nodes": [{
            "type": "recipe", "name": "budget", "recipe": "budget_scenario",
            "arguments": {
                "price": "10", "unit_variable_cost": "6", "fixed_costs": "1000",
                "volumes": [100, 250, 400]
            }
        }],
        "outputs": ["budget"]
    }))
    .unwrap();
    let result = engine.batch(&request, &engine.base_context()).unwrap();
    let envelope = result.outputs[0].result.as_ref().unwrap();
    let checks = field(&envelope.result, "verification").as_array().unwrap();
    assert!(checks.iter().any(|check| {
        field(check, "check")
            .as_text()
            .unwrap()
            .starts_with("break_even_unit_coverage")
    }));
    assert!(checks.iter().any(|check| {
        field(check, "check")
            .as_text()
            .unwrap()
            .starts_with("break_even_minimality")
    }));
    for check in checks {
        assert_eq!(field(check, "status").as_text().unwrap(), "confirmed");
    }
}

#[test]
fn interval_results_are_labelled_not_certified() {
    let engine = engine();
    let envelope = call(
        &engine,
        "interval.enclose",
        serde_json::json!({"value": "0.1"}),
    )
    .unwrap();
    assert_eq!(
        field(&envelope.result, "assurance").as_text().unwrap(),
        "heuristic_padding_not_certified"
    );
    assert!(
        envelope
            .warnings
            .iter()
            .any(|warning| warning.code == "not_certified")
    );
}

#[test]
fn unknown_recipe_is_rejected() {
    let engine = engine();
    let request: BatchRequest = serde_json::from_value(serde_json::json!({
        "nodes": [{"type": "recipe", "name": "x", "recipe": "nope"}]
    }))
    .unwrap();
    let error = engine.batch(&request, &engine.base_context()).unwrap_err();
    assert_eq!(error.code, bicmath_core::ErrorCode::NotFound);
}

#[test]
fn evaluate_can_call_new_modules() {
    let engine = engine();
    let request: EvaluateRequest = serde_json::from_value(serde_json::json!({
        "expression": "algebra.fibonacci(n = 10)"
    }))
    .unwrap();
    let envelope = engine.evaluate(&request, &engine.base_context()).unwrap();
    assert_eq!(
        serde_json::to_value(&envelope.result).unwrap(),
        serde_json::json!({"kind": "integer", "value": "55"})
    );
}
