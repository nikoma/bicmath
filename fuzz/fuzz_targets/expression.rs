#![no_main]
//! Fuzz the restricted expression parser and evaluator. The only acceptable
//! outcomes are a valid envelope or a structured error: never a panic.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(source) = std::str::from_utf8(data) {
        let limits = bicmath_core::limits::Limits::conservative();
        if let Ok(expression) = bicmath_core::expr::parse_expression(source, &limits) {
            let engine = bicmath_engine::Engine::full(Default::default()).expect("engine");
            let request = bicmath_engine::request::EvaluateRequest {
                expression: source.to_string(),
                bindings: Default::default(),
                context: None,
                receipt: false,
            };
            let _ = (expression, engine.evaluate(&request, &engine.base_context()));
        }
    }
});
