#![no_main]
//! Fuzz batch planning: duplicate names, cycles, missing references, and
//! malformed nodes must be rejected before execution.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(request) = serde_json::from_slice::<bicmath_engine::BatchRequest>(data) {
        let engine = bicmath_engine::Engine::full(Default::default()).expect("engine");
        let _ = engine.batch(&request, &engine.base_context());
    }
});
