//! Module-extension contract test.
//!
//! A third-party crate implements the `Function` contract, describes itself,
//! and is statically registered. No engine, expression-parser, or MCP code
//! changes are required.

use std::sync::Arc;

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, CostClass, Example, FunctionDescriptor, FunctionRef, Module, ModuleDescriptor, Outcome,
    ParamDescriptor, SimpleFunction,
};
use bicmath_core::envelope::Exactness;
use bicmath_core::error::EngineError;
use bicmath_core::number::NumericMode;
use bicmath_core::schema::{NumberKind, ValueSchema};
use bicmath_core::value::Value;
use bicmath_engine::request::{CallRequest, EvaluateRequest, FunctionFilter};
use bicmath_engine::{Engine, EngineConfig, RegistryBuilder};

fn double_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "example.double",
        "example",
        "1.0.0",
        "Double",
        "Multiply an exact number by two.",
    )
    .with_description("A minimal third-party module function.")
    .with_parameters(vec![ParamDescriptor::required(
        "value",
        "Number to double.",
        ValueSchema::number(NumberKind::Any),
    )])
    .with_output(ValueSchema::number(NumberKind::Any), "Twice the input.")
    .with_modes(vec![
        NumericMode::Exact,
        NumericMode::Auto,
        NumericMode::Scientific,
    ])
    .with_cost(CostClass::Constant)
    .with_method_ref("docs/architecture.md#module-author-guide")
    .with_examples(vec![
        Example::new(
            "double three",
            [(
                "value".to_string(),
                serde_json::from_value(serde_json::json!(3)).unwrap(),
            )]
            .into_iter()
            .collect(),
        )
        .with_value(
            serde_json::from_value(serde_json::json!({"kind": "integer", "value": "6"})).unwrap(),
        ),
    ])
}

fn invoke_double(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let value = args.number("value")?;
    let two = bicmath_core::number::Number::integer(2);
    let result = value.mul(&two, &ctx.numeric, &ctx.limits)?;
    let exactness = if result.value.is_float() {
        Exactness::Approximate
    } else if result.rounded {
        Exactness::Rounded
    } else {
        Exactness::Exact
    };
    Ok(Outcome::new(Value::Number(result.value), exactness))
}

fn example_module() -> Module {
    let functions: Vec<Arc<dyn bicmath_core::contract::Function>> =
        vec![SimpleFunction::arc(double_descriptor(), invoke_double)];
    Module::new(
        ModuleDescriptor::new(
            "example",
            "Example",
            "1.0.0",
            "A third-party extension module used to verify the module contract.",
        )
        .with_capabilities(vec!["extension_example"])
        .with_modes(vec![
            NumericMode::Exact,
            NumericMode::Auto,
            NumericMode::Scientific,
        ])
        .with_source("crates/bicmath-engine/tests/extension.rs"),
        functions,
    )
}

fn engine() -> Engine {
    let registry = RegistryBuilder::new()
        .add_module(example_module())
        .build()
        .expect("extension module validates");
    Engine::from_registry(registry, EngineConfig::default())
}

#[test]
fn extension_module_is_discovered_and_callable_everywhere() {
    let engine = engine();

    // Discovery.
    let page = engine.list_functions(&FunctionFilter {
        module: Some("example".to_string()),
        ..FunctionFilter::default()
    });
    assert_eq!(page.total, 1);
    assert_eq!(page.functions[0].id, "example.double");

    // Typed calculate.
    let request: CallRequest = serde_json::from_value(serde_json::json!({
        "function": "example.double",
        "arguments": {"value": "0.25"}
    }))
    .unwrap();
    let envelope = engine.call(&request, &engine.base_context()).unwrap();
    assert_eq!(
        serde_json::to_value(&envelope.result).unwrap(),
        serde_json::json!({"kind": "decimal", "value": "0.50"})
    );
    assert_eq!(
        envelope.function,
        Some(FunctionRef {
            id: "example.double".to_string(),
            version: "1.0.0".to_string()
        })
    );

    // Expression evaluation uses the same registry without parser changes.
    let request: EvaluateRequest = serde_json::from_value(serde_json::json!({
        "expression": "example.double(value = 21)"
    }))
    .unwrap();
    let envelope = engine.evaluate(&request, &engine.base_context()).unwrap();
    assert_eq!(
        serde_json::to_value(&envelope.result).unwrap(),
        serde_json::json!({"kind": "integer", "value": "42"})
    );

    // Documented examples execute for extension modules too.
    assert!(engine.validate_examples().is_empty());
}

#[test]
fn extension_module_rejects_duplicate_ids() {
    let result = RegistryBuilder::new()
        .add_module(example_module())
        .add_module(example_module())
        .build();
    assert!(result.is_err());
}
