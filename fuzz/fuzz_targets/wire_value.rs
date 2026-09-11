#![no_main]
//! Fuzz the wire value deserializer: malformed tagged values, huge payloads,
//! deep nesting, and reserved keys must be rejected, never panic.
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let limits = bicmath_core::limits::Limits::conservative();
    if let Ok(raw) = serde_json::from_slice::<serde_json::Value>(data) {
        let _ = bicmath_core::value::Value::from_json(&raw, &limits);
    }
});
