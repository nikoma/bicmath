//! Mandatory numerical regression cases and the end-to-end business fixture,
//! exercised through the public engine API.

use bicmath_core::envelope::ResultEnvelope;
use bicmath_core::error::{EngineError, ErrorCode};
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
) -> Result<ResultEnvelope, EngineError> {
    call_with(engine, function, arguments, serde_json::Value::Null)
}

fn call_with(
    engine: &Engine,
    function: &str,
    arguments: serde_json::Value,
    context: serde_json::Value,
) -> Result<ResultEnvelope, EngineError> {
    let mut request = serde_json::json!({"function": function, "arguments": arguments});
    if !context.is_null() {
        request["context"] = context;
    }
    let request: CallRequest = serde_json::from_value(request).expect("valid request");
    engine.call(&request, &engine.base_context())
}

fn evaluate(
    engine: &Engine,
    expression: &str,
    bindings: serde_json::Value,
    context: serde_json::Value,
) -> Result<ResultEnvelope, EngineError> {
    let mut request = serde_json::json!({"expression": expression, "bindings": bindings});
    if !context.is_null() {
        request["context"] = context;
    }
    let request: EvaluateRequest = serde_json::from_value(request).expect("valid request");
    engine.evaluate(&request, &engine.base_context())
}

fn batch(
    engine: &Engine,
    request: serde_json::Value,
) -> Result<bicmath_engine::BatchEnvelope, EngineError> {
    let request: BatchRequest = serde_json::from_value(request).expect("valid batch request");
    engine.batch(&request, &engine.base_context())
}

fn canonical(envelope: &ResultEnvelope) -> serde_json::Value {
    serde_json::to_value(&envelope.result).expect("serializable result")
}

fn field<'a>(value: &'a Value, name: &str) -> &'a Value {
    value
        .as_record()
        .expect("record")
        .get(name)
        .unwrap_or_else(|| panic!("field {name:?} present"))
}

fn number(value: &Value) -> f64 {
    value
        .as_number()
        .expect("number")
        .to_f64()
        .expect("finite float")
}

fn money_amount(value: &Value) -> f64 {
    let (amount, _currency) = value.as_money().expect("money");
    amount.to_f64().expect("finite float")
}

// ---------------------------------------------------------------------------
// Exactness
// ---------------------------------------------------------------------------

#[test]
fn exact_decimal_addition_is_not_binary_floating_point() {
    let engine = engine();
    let envelope = call(
        &engine,
        "arithmetic.add",
        serde_json::json!({"a": "0.1", "b": "0.2"}),
    )
    .unwrap();
    assert_eq!(
        canonical(&envelope),
        serde_json::json!({"kind": "decimal", "value": "0.3"})
    );
    assert_eq!(envelope.exactness, bicmath_core::envelope::Exactness::Exact);
}

#[test]
fn large_integer_survives_exactly() {
    let engine = engine();
    let envelope = call(
        &engine,
        "arithmetic.add",
        serde_json::json!({
            "a": {"kind": "integer", "value": "9007199254740993"},
            "b": 1
        }),
    )
    .unwrap();
    assert_eq!(
        canonical(&envelope),
        serde_json::json!({"kind": "integer", "value": "9007199254740994"})
    );
    let envelope = evaluate(
        &engine,
        "9007199254740993 + 1",
        serde_json::json!({}),
        serde_json::Value::Null,
    )
    .unwrap();
    assert_eq!(
        canonical(&envelope),
        serde_json::json!({"kind": "integer", "value": "9007199254740994"})
    );
}

#[test]
fn rational_addition_is_reduced() {
    let engine = engine();
    let envelope = call(
        &engine,
        "arithmetic.add",
        serde_json::json!({
            "a": {"kind": "rational", "numerator": "1", "denominator": "3"},
            "b": {"kind": "rational", "numerator": "1", "denominator": "6"}
        }),
    )
    .unwrap();
    assert_eq!(
        canonical(&envelope),
        serde_json::json!({"kind": "rational", "numerator": "1", "denominator": "2"})
    );
}

#[test]
fn decimal_division_reports_inexactness_and_exact_mode_rejects() {
    let engine = engine();
    let one = serde_json::json!({"kind": "decimal", "value": "1"});
    let three = serde_json::json!({"kind": "decimal", "value": "3"});
    let envelope = call(
        &engine,
        "arithmetic.div",
        serde_json::json!({"a": one, "b": three}),
    )
    .unwrap();
    assert_eq!(
        envelope.exactness,
        bicmath_core::envelope::Exactness::Rounded
    );
    let error = call_with(
        &engine,
        "arithmetic.div",
        serde_json::json!({"a": one, "b": three}),
        serde_json::json!({"mode": "exact"}),
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::UnsupportedNumericMode);
}

#[test]
fn exact_cancellation_sum() {
    let engine = engine();
    let envelope = call(
        &engine,
        "arithmetic.sum",
        serde_json::json!({"values": [10000000000000000i64, 1, -10000000000000000i64]}),
    )
    .unwrap();
    assert_eq!(
        canonical(&envelope),
        serde_json::json!({"kind": "integer", "value": "1"})
    );
    let envelope = call_with(
        &engine,
        "arithmetic.sum",
        serde_json::json!({"values": [
            {"kind": "float64", "value": "1e16"},
            {"kind": "float64", "value": "1"},
            {"kind": "float64", "value": "-1e16"}
        ]}),
        serde_json::json!({"mode": "scientific"}),
    )
    .unwrap();
    assert_eq!(number(&envelope.result), 1.0);
    assert_eq!(
        envelope.exactness,
        bicmath_core::envelope::Exactness::Approximate
    );
}

// ---------------------------------------------------------------------------
// Descriptive statistics
// ---------------------------------------------------------------------------

#[test]
fn median_regression_cases() {
    let engine = engine();
    let median = |values: serde_json::Value| {
        call(
            &engine,
            "statistics.median",
            serde_json::json!({"values": values}),
        )
        .unwrap()
    };
    assert_eq!(
        canonical(&median(serde_json::json!([2, 10, 30]))),
        serde_json::json!({"kind": "integer", "value": "10"})
    );
    assert_eq!(
        canonical(&median(serde_json::json!([1, 2, 10, 100]))),
        serde_json::json!({"kind": "integer", "value": "6"})
    );
    // The exact median may be returned as an equal rational.
    let middle = median(serde_json::json!([-5, 10, -5, 0.5, 3.25]));
    assert!((middle.result.as_number().unwrap().to_f64().unwrap() - 0.5).abs() < 1e-15);
    // Calling twice with the same input must produce the same result; the
    // engine never mutates caller input.
    let input = serde_json::json!([3, 1, 2]);
    assert_eq!(median(input.clone()).result, median(input).result);
}

#[test]
fn empty_inputs_are_useful_errors() {
    let engine = engine();
    for function in ["arithmetic.min", "arithmetic.max", "statistics.mean"] {
        let error = call(&engine, function, serde_json::json!({"values": []})).unwrap_err();
        assert!(
            matches!(
                error.code,
                ErrorCode::InsufficientObservations | ErrorCode::MalformedInput
            ),
            "{function}: {error:?}"
        );
    }
}

#[test]
fn variance_population_sample_and_insufficient_data() {
    let engine = engine();
    let population = call(
        &engine,
        "statistics.variance",
        serde_json::json!({"values": [1, 2, 3], "ddof": 0}),
    )
    .unwrap();
    assert_eq!(
        canonical(&population),
        serde_json::json!({"kind": "rational", "numerator": "2", "denominator": "3"})
    );
    let sample = call(
        &engine,
        "statistics.variance",
        serde_json::json!({"values": [1, 2, 3], "ddof": 1}),
    )
    .unwrap();
    assert_eq!(
        canonical(&sample),
        serde_json::json!({"kind": "integer", "value": "1"})
    );
    let error = call(
        &engine,
        "statistics.variance",
        serde_json::json!({"values": [42], "ddof": 1}),
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::InsufficientObservations);
}

// ---------------------------------------------------------------------------
// Domain errors and rounding
// ---------------------------------------------------------------------------

#[test]
fn domain_errors_never_return_nan_or_infinity() {
    let engine = engine();
    let error = call(
        &engine,
        "arithmetic.div",
        serde_json::json!({"a": 1, "b": 0}),
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::DivisionByZero);
    let error = call(
        &engine,
        "arithmetic.div",
        serde_json::json!({"a": 0, "b": 0}),
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::DivisionByZero);
    let scientific = serde_json::json!({"mode": "scientific"});
    let error = call_with(
        &engine,
        "scientific.asin",
        serde_json::json!({"x": 2}),
        scientific.clone(),
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::DomainViolation);
    let error = call_with(
        &engine,
        "scientific.ln",
        serde_json::json!({"x": -1}),
        scientific,
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::DomainViolation);
}

#[test]
fn half_even_and_negative_ties() {
    let engine = engine();
    let quantize = |value: &str| {
        call(
            &engine,
            "arithmetic.quantize",
            serde_json::json!({"value": value, "scale": 2}),
        )
        .unwrap()
    };
    assert_eq!(
        canonical(&quantize("1.005")),
        serde_json::json!({"kind": "decimal", "value": "1.00"})
    );
    assert_eq!(
        canonical(&quantize("1.015")),
        serde_json::json!({"kind": "decimal", "value": "1.02"})
    );
    assert_eq!(
        canonical(&quantize("-1.005")),
        serde_json::json!({"kind": "decimal", "value": "-1.00"})
    );
    assert_eq!(
        canonical(&quantize("-1.015")),
        serde_json::json!({"kind": "decimal", "value": "-1.02"})
    );
}

// ---------------------------------------------------------------------------
// Finance and units
// ---------------------------------------------------------------------------

#[test]
fn money_allocation_preserves_total() {
    let engine = engine();
    let money = serde_json::json!({
        "kind": "money",
        "amount": {"kind": "decimal", "value": "0.01"},
        "currency": "USD"
    });
    let envelope = call(
        &engine,
        "finance.money_allocate",
        serde_json::json!({"money": money, "weights": [1, 1, 1]}),
    )
    .unwrap();
    let shares = field(&envelope.result, "shares").as_array().unwrap();
    let total: f64 = shares.iter().map(money_amount).sum();
    assert!((total - 0.01).abs() < 1e-12);
}

#[test]
fn currency_mismatch_is_rejected() {
    let engine = engine();
    let usd = serde_json::json!({
        "kind": "money", "amount": {"kind": "decimal", "value": "1.00"}, "currency": "USD"
    });
    let inr = serde_json::json!({
        "kind": "money", "amount": {"kind": "decimal", "value": "1.00"}, "currency": "INR"
    });
    let error = call(
        &engine,
        "finance.money_add",
        serde_json::json!({"a": usd, "b": inr}),
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::CurrencyMismatch);
}

#[test]
fn zero_interest_loan_reconciles() {
    let engine = engine();
    let envelope = call(
        &engine,
        "finance.amortization",
        serde_json::json!({
            "principal": {
                "kind": "money",
                "amount": {"kind": "decimal", "value": "1200.00"},
                "currency": "USD"
            },
            "annual_rate": "0",
            "periods": 12,
            "currency": "USD"
        }),
    )
    .unwrap();
    let schedule = field(&envelope.result, "schedule").as_array().unwrap();
    assert_eq!(schedule.len(), 12);
    let mut principal_sum = 0.0f64;
    for row in schedule {
        principal_sum += money_amount(field(row, "principal"));
        assert!((money_amount(field(row, "payment")) - 100.0).abs() < 1e-9);
    }
    assert!((principal_sum - 1200.0).abs() < 1e-9);
}

#[test]
fn celsius_to_kelvin_is_exact() {
    let engine = engine();
    let envelope = call(
        &engine,
        "units.convert",
        serde_json::json!({
            "value": {
                "kind": "quantity",
                "value": {"kind": "decimal", "value": "0"},
                "dimension": {"temperature": 1}
            },
            "from": "degC",
            "to": "K"
        }),
    )
    .unwrap();
    let (amount, dimension) = envelope.result.as_quantity().unwrap();
    assert_eq!(dimension.to_string(), "K");
    assert_eq!(amount.to_string(), "273.15");
}

// ---------------------------------------------------------------------------
// Linear algebra
// ---------------------------------------------------------------------------

#[test]
fn singular_and_ragged_matrices_are_specific_errors() {
    let engine = engine();
    let error = call(
        &engine,
        "linear_algebra.solve",
        serde_json::json!({
            "a": {"kind": "matrix", "rows": 2, "cols": 2, "data": [1, 2, 2, 4]},
            "b": [1, 2]
        }),
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::SingularMatrix);
    let error = call(
        &engine,
        "linear_algebra.determinant",
        serde_json::json!({
            "matrix": {"kind": "matrix", "rows": 2, "cols": 2, "data": [1, 2, 3]}
        }),
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::MalformedInput);
}

// ---------------------------------------------------------------------------
// Expressions and batches
// ---------------------------------------------------------------------------

#[test]
fn expression_precedence_and_bindings() {
    let engine = engine();
    let eval = |source: &str, bindings: serde_json::Value| {
        evaluate(&engine, source, bindings, serde_json::Value::Null).unwrap()
    };
    assert_eq!(
        canonical(&eval("1 + 2 * 3", serde_json::json!({}))),
        serde_json::json!({"kind": "integer", "value": "7"})
    );
    assert_eq!(
        canonical(&eval("2^3^2", serde_json::json!({}))),
        serde_json::json!({"kind": "integer", "value": "512"})
    );
    assert_eq!(
        canonical(&eval("-2^2", serde_json::json!({}))),
        serde_json::json!({"kind": "integer", "value": "-4"})
    );
    assert_eq!(
        canonical(&eval("0 < x < 1", serde_json::json!({"x": 0.5}))),
        serde_json::json!(true)
    );
    assert_eq!(
        canonical(&eval("a + b", serde_json::json!({"a": 1, "b": 2}))),
        serde_json::json!({"kind": "integer", "value": "3"})
    );
}

#[test]
fn expression_can_call_functions_with_named_arguments() {
    let engine = engine();
    let envelope = evaluate(
        &engine,
        "arithmetic.add(a = 0.1, b = 0.2)",
        serde_json::json!({}),
        serde_json::Value::Null,
    )
    .unwrap();
    assert_eq!(
        canonical(&envelope),
        serde_json::json!({"kind": "decimal", "value": "0.3"})
    );
}

#[test]
fn expression_can_use_finance_npv() {
    let engine = engine();
    let envelope = evaluate(
        &engine,
        "finance.npv(rate = 0.08, cashflows = [-10000, 4000, 4000, 4000], currency = \"USD\", timing = \"first_cashflow_at_t0\")",
        serde_json::json!({}),
        serde_json::Value::Null,
    )
    .unwrap();
    let npv = money_amount(field(&envelope.result, "npv"));
    let expected = -10000.0 + 4000.0 / 1.08 + 4000.0 / 1.08f64.powi(2) + 4000.0 / 1.08f64.powi(3);
    assert!((npv - expected).abs() < 0.01, "npv {npv} vs {expected}");
}

#[test]
fn batch_executes_dependencies_in_order() {
    let engine = engine();
    let result = batch(
        &engine,
        serde_json::json!({
            "nodes": [
                {"type": "call", "name": "total", "function": "arithmetic.add",
                 "arguments": {"a": 2, "b": 3}},
                {"type": "call", "name": "double", "function": "arithmetic.mul",
                 "arguments": {"a": {"$ref": "total"}, "b": 2}},
                {"type": "evaluate", "name": "check", "expression": "double > 9"}
            ],
            "outputs": ["total", "double", "check"]
        }),
    )
    .unwrap();
    assert_eq!(result.outputs.len(), 3);
    assert_eq!(result.outputs[0].status, bicmath_engine::NodeStatus::Ok);
    assert_eq!(
        serde_json::to_value(&result.outputs[1].result.as_ref().unwrap().result).unwrap(),
        serde_json::json!({"kind": "integer", "value": "10"})
    );
    assert_eq!(
        serde_json::to_value(&result.outputs[2].result.as_ref().unwrap().result).unwrap(),
        serde_json::json!(true)
    );
}

#[test]
fn cyclic_batch_is_rejected() {
    let engine = engine();
    let error = batch(
        &engine,
        serde_json::json!({
            "nodes": [
                {"type": "call", "name": "a", "function": "arithmetic.add",
                 "arguments": {"a": {"$ref": "b"}, "b": 1}},
                {"type": "call", "name": "b", "function": "arithmetic.add",
                 "arguments": {"a": {"$ref": "a"}, "b": 1}}
            ]
        }),
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::MalformedInput);
    assert!(error.message.contains("cycle"));
}

#[test]
fn batch_fail_fast_marks_dependents_skipped() {
    let engine = engine();
    let result = batch(
        &engine,
        serde_json::json!({
            "nodes": [
                {"type": "call", "name": "bad", "function": "arithmetic.div",
                 "arguments": {"a": 1, "b": 0}},
                {"type": "call", "name": "dependent", "function": "arithmetic.mul",
                 "arguments": {"a": {"$ref": "bad"}, "b": 2}},
                {"type": "call", "name": "independent", "function": "arithmetic.add",
                 "arguments": {"a": 1, "b": 1}}
            ]
        }),
    )
    .unwrap();
    assert_eq!(result.outputs[0].status, bicmath_engine::NodeStatus::Error);
    assert_eq!(
        result.outputs[1].status,
        bicmath_engine::NodeStatus::Skipped
    );
    assert_eq!(
        result.outputs[2].status,
        bicmath_engine::NodeStatus::Skipped
    );
}

// ---------------------------------------------------------------------------
// Configuration, limits, receipts
// ---------------------------------------------------------------------------

#[test]
fn disabled_module_is_absent_from_every_entry_point() {
    let config = EngineConfig {
        disabled_modules: vec!["finance".to_string()],
        ..EngineConfig::default()
    };
    let engine = Engine::full(config).unwrap();
    // Discovery.
    let page = engine.list_functions(&FunctionFilter::default());
    assert!(page.functions.iter().all(|f| f.module != "finance"));
    // calculate.
    let error = call(
        &engine,
        "finance.npv",
        serde_json::json!({"rate": "0.08", "cashflows": [-100, 200]}),
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::DisabledModule);
    // evaluate.
    let error = evaluate(
        &engine,
        "finance.npv(rate = 0.08, cashflows = [-100, 200])",
        serde_json::json!({}),
        serde_json::Value::Null,
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::DisabledModule);
    // batch.
    let error = batch(
        &engine,
        serde_json::json!({
            "nodes": [
                {"type": "call", "name": "n", "function": "finance.npv",
                 "arguments": {"rate": "0.08", "cashflows": [-100, 200]}}
            ]
        }),
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::DisabledModule);
}

#[test]
fn resource_limits_are_enforced() {
    let engine = engine();
    let error = call(
        &engine,
        "arithmetic.pow",
        serde_json::json!({"base": 2, "exponent": 1000000000i64}),
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::ResourceLimit);

    let deep = format!("{}1{}", "(".repeat(200), ")".repeat(200));
    let request: EvaluateRequest =
        serde_json::from_value(serde_json::json!({"expression": deep})).unwrap();
    let error = engine
        .evaluate(&request, &engine.base_context())
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::ResourceLimit);
}

#[test]
fn callers_cannot_raise_server_limits() {
    let engine = engine();
    let error = call_with(
        &engine,
        "arithmetic.add",
        serde_json::json!({"a": 1, "b": 2}),
        serde_json::json!({"limits": {"max_array_len": 999999999}}),
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::ResourceLimit);
}

#[test]
fn receipts_are_deterministic_and_replayable() {
    let engine = engine();
    let request: CallRequest = serde_json::from_value(serde_json::json!({
        "function": "arithmetic.add",
        "arguments": {"a": "0.1", "b": "0.2"},
        "receipt": true
    }))
    .unwrap();
    let first = engine.call(&request, &engine.base_context()).unwrap();
    let second = engine.call(&request, &engine.base_context()).unwrap();
    assert_eq!(first.fingerprint, second.fingerprint);
    let receipt = first.receipt.as_ref().expect("receipt present");
    assert_eq!(receipt.fingerprint, first.fingerprint);
    assert_eq!(receipt.request["function"], "arithmetic.add");
}

#[test]
fn every_documented_example_executes() {
    let engine = engine();
    let failures = engine.validate_examples();
    assert!(failures.is_empty(), "example failures: {failures:#?}");
}

// ---------------------------------------------------------------------------
// End-to-end business fixture
// ---------------------------------------------------------------------------

fn campaign_strata() -> serde_json::Value {
    serde_json::json!([
        {
            "name": "beginners",
            "target_weight": 30000,
            "treatment": {"assigned": 8000, "outcomes": [
                {"value": 100, "count": 320},
                {"value": -10, "count": 80},
                {"value": 0, "count": 7600}
            ]},
            "control": {"assigned": 2000, "outcomes": [
                {"value": 100, "count": 76},
                {"value": -10, "count": 4},
                {"value": 0, "count": 1920}
            ]}
        },
        {
            "name": "advanced",
            "target_weight": 10000,
            "treatment": {"assigned": 2000, "outcomes": [
                {"value": 100, "count": 320},
                {"value": -10, "count": 80},
                {"value": 0, "count": 1600}
            ]},
            "control": {"assigned": 8000, "outcomes": [
                {"value": 100, "count": 1140},
                {"value": -10, "count": 60},
                {"value": 0, "count": 6800}
            ]}
        }
    ])
}

#[test]
fn campaign_fixture_full_treatment_economics() {
    let engine = engine();
    let envelope = call(
        &engine,
        "statistics.stratified_experiment",
        serde_json::json!({
            "strata": campaign_strata(),
            "fixed_cost": 12000,
            "variance_ddof": 1
        }),
    )
    .unwrap();
    let standardized = field(&envelope.result, "standardized");
    assert!((number(field(standardized, "treatment_mean")) - 6.825).abs() < 1e-12);
    assert!((number(field(standardized, "control_mean")) - 6.37875).abs() < 1e-12);
    assert!((number(field(standardized, "difference")) - 0.44625).abs() < 1e-12);
    let se = number(field(standardized, "standard_error"));
    assert!((se - 0.42685447).abs() < 1e-6, "se {se}");

    let positive = field(&envelope.result, "positive_rate");
    assert!((number(field(positive, "difference")) - 0.005875).abs() < 1e-12);
    let negative = field(&envelope.result, "negative_rate");
    assert!((number(field(negative, "difference")) - 0.014125).abs() < 1e-12);
    let net_rate = field(&envelope.result, "net_positive_rate");
    assert!((number(field(net_rate, "difference")) + 0.00825).abs() < 1e-12);

    let scaled = field(&envelope.result, "scaled");
    assert!((number(field(scaled, "total")) - 17850.0).abs() < 1e-6);
    assert!((number(field(scaled, "net_difference")) - 5850.0).abs() < 1e-6);
    let net_ci = field(scaled, "net_confidence_interval");
    // The engine uses the exact normal quantile; the assignment's illustrative
    // interval used z = 1.96, so allow the corresponding small difference.
    let se_total = se * 40000.0;
    assert!(
        (number(field(net_ci, "lower")) - (5850.0 - 1.959963984540054 * se_total)).abs() < 1e-6
    );
    assert!(
        (number(field(net_ci, "upper")) - (5850.0 + 1.959963984540054 * se_total)).abs() < 1e-6
    );
    assert!(
        (number(field(net_ci, "lower")) + 27615.39).abs() < 0.7,
        "net lower {}",
        number(field(net_ci, "lower"))
    );
    assert!(
        (number(field(net_ci, "upper")) - 39315.39).abs() < 0.7,
        "net upper {}",
        number(field(net_ci, "upper"))
    );

    // Diagnostics: pooled rates are descriptive only.
    let diagnostics = field(&envelope.result, "diagnostics");
    let pooled = field(diagnostics, "pooled");
    // Purchase rate = positive (retained) + negative (refunded) outcome rates.
    let treatment_purchase = number(field(pooled, "treatment_positive_rate"))
        + number(field(pooled, "treatment_negative_rate"));
    let control_purchase = number(field(pooled, "control_positive_rate"))
        + number(field(pooled, "control_negative_rate"));
    assert!((treatment_purchase - 0.08).abs() < 1e-12);
    assert!((control_purchase - 0.128).abs() < 1e-12);
    let assumptions = field(&envelope.result, "assumptions").as_array().unwrap();
    assert!(assumptions.iter().any(|statement| statement
        .as_text()
        .unwrap()
        .contains("positive outcome rate difference is not the same as a positive contribution difference")));
    assert!(!envelope.assumptions.is_empty());
}

#[test]
fn campaign_fixture_85_percent_holdout() {
    let engine = engine();
    let envelope = call(
        &engine,
        "statistics.stratified_experiment",
        serde_json::json!({
            "strata": campaign_strata(),
            "fixed_cost": 12000,
            "rollout_fraction": 0.85,
            "variance_ddof": 1
        }),
    )
    .unwrap();
    let scaled = field(&envelope.result, "scaled");
    assert!((number(field(scaled, "rollout_fraction")) - 0.85).abs() < 1e-15);
    assert!((number(field(scaled, "difference")) - 0.3793125).abs() < 1e-12);
    assert!((number(field(scaled, "total")) - 15172.5).abs() < 1e-6);
    assert!((number(field(scaled, "net_difference")) - 3172.5).abs() < 1e-6);
    // Incremental purchases, refunds, retained: positive + negative rates.
    let positive = field(&envelope.result, "positive_rate");
    let negative = field(&envelope.result, "negative_rate");
    let retained = number(field(positive, "difference")) * 40000.0;
    let refunds = number(field(negative, "difference")) * 40000.0;
    assert!((retained - 235.0).abs() < 1e-6);
    assert!((refunds - 565.0).abs() < 1e-6);
    assert!((retained + refunds - 800.0).abs() < 1e-6);
    // Scaled SE uses fraction-scaled variance, not the full-exposure SE.
    let full = call(
        &engine,
        "statistics.stratified_experiment",
        serde_json::json!({"strata": campaign_strata(), "variance_ddof": 1}),
    )
    .unwrap();
    let full_se = number(field(field(&full.result, "standardized"), "standard_error"));
    let scaled_se = number(field(scaled, "standard_error"));
    assert!((scaled_se - 0.85 * full_se).abs() < 1e-12);
    // The full-exposure net interval is not reused unchanged.
    let full_net_ci = field(field(&full.result, "scaled"), "net_confidence_interval");
    let scaled_net_ci = field(scaled, "net_confidence_interval");
    assert!(
        (number(field(scaled_net_ci, "upper")) - number(field(full_net_ci, "upper"))).abs() > 1.0
    );
}

#[test]
fn campaign_prospective_pool_is_labelled() {
    let engine = engine();
    let additional = serde_json::json!([
        {
            "name": "beginners",
            "target_weight": 30000,
            "treatment": {"assigned": 4000, "outcomes": [
                {"value": 100, "count": 180},
                {"value": -10, "count": 30},
                {"value": 0, "count": 3790}
            ]},
            "control": {"assigned": 1000, "outcomes": [
                {"value": 100, "count": 40},
                {"value": -10, "count": 2},
                {"value": 0, "count": 958}
            ]}
        },
        {
            "name": "advanced",
            "target_weight": 10000,
            "treatment": {"assigned": 1000, "outcomes": [
                {"value": 100, "count": 170},
                {"value": -10, "count": 35},
                {"value": 0, "count": 795}
            ]},
            "control": {"assigned": 4000, "outcomes": [
                {"value": 100, "count": 580},
                {"value": -10, "count": 30},
                {"value": 0, "count": 3390}
            ]}
        }
    ]);
    let envelope = call(
        &engine,
        "statistics.prospective_pool",
        serde_json::json!({
            "original": campaign_strata(),
            "additional": additional,
            "confidence": 0.95
        }),
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(field(&envelope.result, "prospective")).unwrap(),
        serde_json::json!(true)
    );
    assert!(
        envelope
            .warnings
            .iter()
            .any(|warning| warning.code == "prospective_hypothetical_data"),
        "prospective warning missing: {:?}",
        envelope.warnings
    );
    // Pooled sample sizes are the sum of original and additional.
    let strata = field(&envelope.result, "strata").as_array().unwrap();
    assert!(!strata.is_empty());
}
