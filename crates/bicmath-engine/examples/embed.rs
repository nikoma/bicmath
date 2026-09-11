//! Rust embedding example: no Tokio, no MCP, no transport.
//!
//! Run with: cargo run -p bicmath-engine --example embed

use bicmath_core::context::{ExecContext, TraceLevel};
use bicmath_core::number::{NumericContext, NumericMode, RoundingMode};
use bicmath_engine::request::CallRequest;
use bicmath_engine::{Engine, EngineConfig, RegistryBuilder};

fn main() {
    // 1. Assemble a registry. `Engine::full` uses every module compiled in;
    //    `RegistryBuilder` lets an application choose exactly what to link.
    let registry = bicmath_engine::registry::add_compiled_modules(RegistryBuilder::new())
        .build()
        .expect("modules validate");
    println!("registered {} functions", registry.function_count());

    // 2. Configure engine policy: modules can be disabled, numeric defaults and
    //    limits are server policy.
    let config = EngineConfig {
        disabled_modules: vec![],
        default_mode: NumericMode::Auto,
        precision: 34,
        rounding: RoundingMode::HalfEven,
        ..EngineConfig::default()
    };
    let engine = Engine::from_registry(registry, config);

    // 3. Supply an execution context. A caller may lower limits here but never
    //    raise them above policy.
    let mut context = engine.base_context();
    context.trace = TraceLevel::Summary;

    // 4. Invoke a function and inspect success metadata.
    let request: CallRequest = serde_json::from_value(serde_json::json!({
        "function": "statistics.variance",
        "arguments": {"values": [1, 2, 3], "ddof": 0}
    }))
    .unwrap();
    match engine.call(&request, &context) {
        Ok(envelope) => {
            println!(
                "result: {}",
                serde_json::to_string(&envelope.result).unwrap()
            );
            println!("exactness: {}", envelope.exactness.as_str());
            println!("fingerprint: {}", envelope.fingerprint);
            for warning in &envelope.warnings {
                println!("warning: {}", warning.message);
            }
        }
        Err(error) => eprintln!("calculation failed: {error}"),
    }

    // 5. Errors are structured and never carry a result.
    let request: CallRequest = serde_json::from_value(serde_json::json!({
        "function": "arithmetic.div",
        "arguments": {"a": 1, "b": 0}
    }))
    .unwrap();
    match engine.call(&request, &context) {
        Ok(_) => unreachable!("division by zero must fail"),
        Err(error) => println!(
            "structured error: code={} path={:?} message={}",
            error.code, error.path, error.message
        ),
    }

    // 6. Exact values stay exact through the typed API.
    let request: CallRequest = serde_json::from_value(serde_json::json!({
        "function": "arithmetic.add",
        "arguments": {"a": "0.1", "b": "0.2"}
    }))
    .unwrap();
    let envelope = engine.call(&request, &context).unwrap();
    assert_eq!(
        serde_json::to_value(&envelope.result).unwrap(),
        serde_json::json!({"kind": "decimal", "value": "0.3"})
    );
    println!("exact decimal addition verified: 0.1 + 0.2 = 0.3");

    // 7. Numeric mode is explicit; scientific functions require it.
    let scientific = ExecContext {
        numeric: NumericContext::scientific(),
        ..engine.base_context()
    };
    let request: CallRequest = serde_json::from_value(serde_json::json!({
        "function": "scientific.sin",
        "arguments": {"x": 0}
    }))
    .unwrap();
    let envelope = engine.call(&request, &scientific).unwrap();
    println!(
        "sin(0) = {}",
        serde_json::to_string(&envelope.result).unwrap()
    );
}
