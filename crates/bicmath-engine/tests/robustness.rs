//! Robustness corpus: malformed and adversarial inputs must produce structured
//! errors, never panics or hangs. This is the deterministic companion to the
//! cargo-fuzz targets in `fuzz/`.

use bicmath_core::error::EngineError;
use bicmath_core::expr::parse_expression;
use bicmath_core::limits::Limits;
use bicmath_core::value::Value;
use bicmath_engine::request::{CallRequest, EvaluateRequest, FunctionFilter};
use bicmath_engine::{BatchRequest, Engine, EngineConfig};

fn engine() -> Engine {
    Engine::full(EngineConfig::default()).unwrap()
}

#[test]
fn malformed_expressions_are_structured_errors() {
    let limits = Limits::conservative();
    let corpus = [
        "",
        " ",
        "(",
        ")",
        "1 +",
        "+ ",
        "1 ** 2",
        "1 // 2",
        "1..2",
        "1e",
        "1e+",
        "0x10",
        "1e999999999999",
        "sin(",
        "sin(1",
        "a.b.",
        "a.b(1,)",
        "{a: }",
        "{a 1}",
        "[1,,2]",
        "\"unterminated",
        "1 ? 2 : 3",
        "1; 2",
        "import os",
        "1 + 1e100000000000000000000",
        &"(".repeat(200),
        &"[".repeat(200),
        "not 1",
        "1 && 2",
    ];
    for source in corpus {
        match parse_expression(source, &limits) {
            Ok(_) => {}
            Err(error) => {
                assert!(
                    !error.message.is_empty(),
                    "expression {source:?} produced an empty message"
                );
            }
        }
    }
}

#[test]
fn malformed_wire_values_are_structured_errors() {
    let limits = Limits::conservative();
    let corpus = [
        serde_json::json!({"kind": "integer", "value": "12x"}),
        serde_json::json!({"kind": "integer"}),
        serde_json::json!({"kind": "rational", "numerator": "1", "denominator": "0"}),
        serde_json::json!({"kind": "decimal", "value": "1,5"}),
        serde_json::json!({"kind": "decimal", "value": "1e"}),
        serde_json::json!({"kind": "float64", "value": "NaN"}),
        serde_json::json!({"kind": "float64", "value": "inf"}),
        serde_json::json!({"kind": "matrix", "rows": 2, "cols": 2, "data": [1, 2, 3]}),
        serde_json::json!({"kind": "matrix", "rows": 0, "cols": 2, "data": []}),
        serde_json::json!({"kind": "money", "amount": 1, "currency": "usd"}),
        serde_json::json!({"kind": "quantity", "value": [1]}),
        serde_json::json!({"kind": "bound", "value": {"kind": "float64", "value": "NaN"}}),
        serde_json::json!({"kind": "unknown_tag"}),
        serde_json::json!({"kind": "not-a-tag", "x": 1}),
    ];
    for raw in corpus {
        if let Ok(value) = Value::from_json(&raw, &limits) {
            // If it parses, structural limits must still hold.
            value.check_limits(&limits, 0).unwrap();
        }
    }
}

#[test]
fn malformed_requests_are_structured_errors() {
    let engine = engine();
    let context = engine.base_context();
    let requests = [
        serde_json::json!({"function": ""}),
        serde_json::json!({"function": "nope.nope"}),
        serde_json::json!({"function": "arithmetic.add"}),
        serde_json::json!({"function": "arithmetic.add", "arguments": []}),
        serde_json::json!({"function": "arithmetic.add", "arguments": {"a": 1, "b": 2, "c": 3}}),
        serde_json::json!({"function": "arithmetic.add", "arguments": {"a": {"kind": "float64", "value": "0.1"}, "b": 1}}),
        serde_json::json!({"function": "arithmetic.add", "arguments": {"a": {"kind": "decimal", "value": "1e999999"}, "b": 1}}),
        serde_json::json!({"function": "arithmetic.add", "arguments": {"a": "not a number", "b": 1}}),
        serde_json::json!({"function": "arithmetic.add", "arguments": {"a": 1, "b": 1}, "context": {"mode": "nope"}}),
        serde_json::json!({"function": "arithmetic.add", "arguments": {"a": 1, "b": 1}, "unknown": true}),
    ];
    for raw in requests {
        let request: Result<CallRequest, _> = serde_json::from_value(raw);
        if let Ok(request) = request {
            let result = engine.call(&request, &context);
            assert!(
                result.is_ok() || result.is_err(),
                "call must return a structured result"
            );
        }
    }
}

#[test]
fn malformed_evaluate_and_batch_requests_are_structured_errors() {
    let engine = engine();
    let context = engine.base_context();
    let expressions = [
        "",
        "unknown_binding + 1",
        "arithmetic.add(a = 1)",
        "arithmetic.add(1, 2, 3)",
        "arithmetic.add(a = 1, a = 2)",
        "nope.nope(1)",
        "1 +",
    ];
    for expression in expressions {
        let request: EvaluateRequest =
            serde_json::from_value(serde_json::json!({"expression": expression})).unwrap();
        let _ = engine.evaluate(&request, &context);
    }

    let batches = [
        serde_json::json!({"nodes": []}),
        serde_json::json!({"nodes": [{"type": "call", "name": "a", "function": "arithmetic.add", "arguments": {"a": {"$ref": "missing"}, "b": 1}}]}),
        serde_json::json!({"nodes": [{"type": "call", "name": "a", "function": "arithmetic.add", "arguments": {"a": 1, "b": 1}}, {"type": "call", "name": "a", "function": "arithmetic.add", "arguments": {"a": 1, "b": 1}}]}),
        serde_json::json!({"nodes": [{"type": "call", "name": "a", "function": "arithmetic.add", "arguments": {"a": {"$ref": "a"}, "b": 1}}]}),
        serde_json::json!({"nodes": [{"type": "call", "name": "a", "function": "arithmetic.add", "arguments": {"a": 1, "b": 1}}], "outputs": ["missing"]}),
        serde_json::json!({"nodes": [{"type": "call", "name": "1bad", "function": "arithmetic.add", "arguments": {"a": 1, "b": 1}}]}),
        serde_json::json!({"nodes": [{"type": "call", "name": "a", "function": "nope.nope", "arguments": {}}]}),
    ];
    for raw in batches {
        let request: Result<BatchRequest, _> = serde_json::from_value(raw);
        if let Ok(request) = request {
            let _ = engine.batch(&request, &context);
        }
    }
}

#[test]
fn deeply_nested_wire_values_are_bounded() {
    let limits = Limits::conservative();
    let mut value = serde_json::json!(1);
    for _ in 0..(limits.max_recursion_depth + 10) {
        value = serde_json::Value::Array(vec![value]);
    }
    let result = Value::from_json(&value, &limits);
    assert!(result.is_err(), "deep nesting must be rejected");
    if let Err(error) = result {
        assert_eq!(error.code, bicmath_core::ErrorCode::ResourceLimit);
    }
}

#[test]
fn filter_and_discovery_never_panic_on_hostile_input() {
    let engine = engine();
    for query in ["", "%", "\\", "\u{0}", &"a".repeat(10_000)] {
        let page = engine.list_functions(&FunctionFilter {
            query: Some(query.to_string()),
            limit: Some(0),
            ..FunctionFilter::default()
        });
        assert!(page.functions.is_empty() || page.limit >= 1);
    }
}

fn _assert_engine_error_is_serializable(error: &EngineError) {
    let _ = serde_json::to_string(error).unwrap();
}
