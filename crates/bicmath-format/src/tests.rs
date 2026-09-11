//! Tests for the format module: descriptor completeness, executable examples,
//! and the documented rendering behaviour.

use std::collections::BTreeMap;

use num_bigint::BigInt;

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{Args, ExampleExpectation, Purity};
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::value::Value;

use crate::render::{csv_render, latex_render, markdown_render, text_render};

fn ctx() -> ExecContext {
    ExecContext::conservative()
}

fn call(id: &str, raw: serde_json::Value) -> Result<bicmath_core::contract::Outcome, EngineError> {
    let module = crate::module();
    let function = module
        .functions
        .iter()
        .find(|f| f.descriptor().id == id)
        .expect("function exists");
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
                .coerce(value, name, &ctx().limits, true)
                .expect("argument coerces"),
        );
    }
    function.invoke(&Args::new(values), &ctx())
}

fn record_field<'a>(outcome: &'a bicmath_core::contract::Outcome, field: &str) -> &'a str {
    match &outcome.value {
        Value::Record(fields) => fields
            .get(field)
            .and_then(|value| value.as_text().ok())
            .expect("field is text"),
        other => panic!("expected a record result, found {other:?}"),
    }
}

fn integer(value: i64) -> Value {
    Value::integer(BigInt::from(value))
}

#[test]
fn matrix_markdown_contains_header_and_pipes() {
    let outcome = call(
        "format.to_markdown",
        serde_json::json!({
            "value": {"kind": "matrix", "rows": 2, "cols": 2, "data": [1, 2, 3, 4]}
        }),
    )
    .unwrap();
    let markdown = record_field(&outcome, "markdown");
    assert!(markdown.starts_with('|'));
    assert!(markdown.contains("| --- |"));
    assert!(markdown.lines().count() >= 3);
}

#[test]
fn record_array_csv_quotes_commas_and_quotes() {
    let outcome = call(
        "format.to_csv",
        serde_json::json!({
            "value": [{"name": "Smith, Jane", "note": "say \"hi\""}]
        }),
    )
    .unwrap();
    let csv = record_field(&outcome, "csv");
    assert!(csv.contains("\"Smith, Jane\""));
    assert!(csv.contains("\"say \"\"hi\"\"\""));
}

#[test]
fn latex_escapes_percent_and_ampersand() {
    let outcome = call("format.to_latex", serde_json::json!({"value": "50% & 7"})).unwrap();
    let latex = record_field(&outcome, "latex");
    assert!(latex.contains("\\%"));
    assert!(latex.contains("\\&"));
}

#[test]
fn exact_decimal_scale_is_preserved() {
    let decimal = serde_json::json!({"kind": "decimal", "value": "0.10"});
    let markdown = call(
        "format.to_markdown",
        serde_json::json!({"value": decimal.clone()}),
    )
    .unwrap();
    assert_eq!(record_field(&markdown, "markdown"), "0.10");
    let number = call("format.number", serde_json::json!({"value": decimal})).unwrap();
    assert_eq!(record_field(&number, "text"), "0.10");
}

#[test]
fn money_renders_currency() {
    let outcome = call(
        "format.to_markdown",
        serde_json::json!({
            "value": {
                "kind": "money",
                "amount": {"kind": "decimal", "value": "10.00"},
                "currency": "USD"
            }
        }),
    )
    .unwrap();
    assert_eq!(record_field(&outcome, "markdown"), "10.00 USD");
}

#[test]
fn quantities_render_dimensions() {
    let outcome = call(
        "format.to_markdown",
        serde_json::json!({
            "value": {
                "kind": "quantity",
                "value": {"kind": "decimal", "value": "9.81"},
                "dimension": {"length": 1, "time": -2}
            }
        }),
    )
    .unwrap();
    assert_eq!(record_field(&outcome, "markdown"), "9.81 m*s^-2");
}

#[test]
fn scalar_arrays_render_as_bullet_lists() {
    let outcome = call("format.to_markdown", serde_json::json!({"value": [1, 2]})).unwrap();
    assert_eq!(record_field(&outcome, "markdown"), "- 1\n- 2");
}

#[test]
fn title_is_prepended_as_a_heading() {
    let outcome = call(
        "format.to_markdown",
        serde_json::json!({"value": 5, "title": "Result"}),
    )
    .unwrap();
    assert_eq!(record_field(&outcome, "markdown"), "# Result\n\n5");
}

#[test]
fn number_styles_are_consistent() {
    let rational = serde_json::json!({"kind": "rational", "numerator": "4", "denominator": "2"});
    let plain = call(
        "format.number",
        serde_json::json!({"value": rational.clone()}),
    )
    .unwrap();
    assert_eq!(record_field(&plain, "text"), "2");
    assert_eq!(record_field(&plain, "method"), "plain");
    let canonical = call(
        "format.number",
        serde_json::json!({"value": rational, "style": "canonical"}),
    )
    .unwrap();
    assert_eq!(record_field(&canonical, "text"), "2/1");
    let scientific = call(
        "format.number",
        serde_json::json!({"value": 12345, "style": "scientific"}),
    )
    .unwrap();
    assert_eq!(record_field(&scientific, "text"), "1.2345e4");
}

#[test]
fn csv_record_is_key_value_pair() {
    let outcome = call("format.to_csv", serde_json::json!({"value": {"a": 1}})).unwrap();
    assert_eq!(record_field(&outcome, "csv"), "key,value\r\na,1");
}

#[test]
fn csv_matrix_rows_are_crlf_separated() {
    let outcome = call(
        "format.to_csv",
        serde_json::json!({
            "value": {"kind": "matrix", "rows": 2, "cols": 2, "data": [1, 2, 3, 4]}
        }),
    )
    .unwrap();
    assert_eq!(record_field(&outcome, "csv"), "1,2\r\n3,4");
}

#[test]
fn text_matrix_is_aligned() {
    let outcome = call(
        "format.to_text",
        serde_json::json!({
            "value": {"kind": "matrix", "rows": 2, "cols": 2, "data": [1, 2, 3, 4]}
        }),
    )
    .unwrap();
    assert_eq!(record_field(&outcome, "text"), "1  2\n3  4");
}

#[test]
fn nested_markdown_falls_back_to_a_fenced_code_block() {
    let nested = Value::Array(vec![Value::Array(vec![integer(1)])]);
    let markdown = markdown_render(&nested, None).unwrap();
    assert!(markdown.starts_with("```json"));
    assert!(markdown.ends_with("```"));
}

#[test]
fn matrix_with_structured_entries_is_rejected() {
    let matrix = Value::Matrix {
        rows: 1,
        cols: 1,
        data: vec![Value::Array(vec![integer(1)])],
    };
    for error in [
        markdown_render(&matrix, None).unwrap_err(),
        latex_render(&matrix, None).unwrap_err(),
        csv_render(&matrix).unwrap_err(),
        text_render(&matrix).unwrap_err(),
    ] {
        assert_eq!(error.code, ErrorCode::DomainViolation);
        assert!(error.message.contains("matrix entry 0"));
    }
}

#[test]
fn heterogeneous_array_csv_is_rejected() {
    let mixed = Value::Array(vec![integer(1), Value::record([("a", integer(2))])]);
    assert_eq!(
        csv_render(&mixed).unwrap_err().code,
        ErrorCode::DomainViolation
    );
}

#[test]
fn every_function_declares_an_example_and_method_ref() {
    for function in crate::module().functions {
        let descriptor = function.descriptor();
        assert!(
            !descriptor.examples.is_empty(),
            "function {} has no examples",
            descriptor.id
        );
        assert!(
            descriptor.method_ref.starts_with("docs/methods/format.md#"),
            "function {} has method_ref {:?}",
            descriptor.id,
            descriptor.method_ref
        );
    }
}

#[test]
fn examples_match_their_expectations() {
    for function in crate::module().functions {
        let descriptor = function.descriptor();
        for example in &descriptor.examples {
            let raw = serde_json::to_value(&example.arguments).expect("examples serialize");
            let args_json = raw.as_object().expect("object args");
            let mut values = BTreeMap::new();
            for (name, value) in args_json {
                let param = descriptor.parameter(name).expect("parameter exists");
                values.insert(
                    name.clone(),
                    param
                        .schema
                        .coerce(value, name, &ctx().limits, true)
                        .expect("example coerces"),
                );
            }
            let outcome = function
                .invoke(&Args::new(values), &ctx())
                .unwrap_or_else(|error| panic!("{} example failed: {error}", descriptor.id));
            match &example.expected {
                Some(ExampleExpectation::Value(expected)) => assert_eq!(
                    &outcome.value, expected,
                    "{} example {:?} value mismatch",
                    descriptor.id, example.title
                ),
                Some(ExampleExpectation::Contains(needle)) => {
                    let rendered = serde_json::to_string(&outcome.value).unwrap();
                    assert!(
                        rendered.contains(needle),
                        "{} example {:?} does not contain {needle:?}",
                        descriptor.id,
                        example.title
                    );
                }
                Some(ExampleExpectation::Error(code)) => panic!(
                    "{} example {:?} expected error {code:?} but succeeded",
                    descriptor.id, example.title
                ),
                None => {}
            }
        }
    }
}

#[test]
fn module_descriptor_is_complete() {
    let module = crate::module();
    assert_eq!(module.descriptor.id, "format");
    assert_eq!(module.descriptor.version, "1.0.0");
    assert_eq!(module.functions.len(), 5);
    for function in &module.functions {
        let descriptor = function.descriptor();
        assert_eq!(descriptor.module, "format");
        assert_eq!(descriptor.version, "1.0.0");
        assert_eq!(descriptor.purity, Purity::Pure);
        assert!(!descriptor.parameters.is_empty());
        assert!(!descriptor.output_description.is_empty());
        assert!(!descriptor.units_rule.is_empty());
        assert!(!descriptor.tags.is_empty());
    }
}
