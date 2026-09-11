//! Browser bindings around the BicMath engine.
//!
//! The WASM interface calls the same numerical engine and registry as the
//! native server. It does not run an MCP server inside the browser: the host
//! page supplies UI, optional storage, and scheduling (for example a Web
//! Worker for heavier requests).
//!
//! Exactness over JavaScript:
//! - Exact integer, rational, and decimal payloads are strings in the canonical
//!   wire format, so they survive JS round trips unchanged.
//! - The `*_json` functions take and return JSON text and are the safest API.
//! - The object API uses `serde-wasm-bindgen`; plain JS numbers are IEEE-754
//!   doubles and must not be used for exact values that exceed 2^53.

use wasm_bindgen::prelude::*;

use bicmath_core::error::EngineError;
use bicmath_engine::{
    BatchRequest, CallRequest, Engine, EngineConfig, EvaluateRequest, FunctionFilter,
};

fn engine_error(error: EngineError) -> JsValue {
    match serde_wasm_bindgen::to_value(&error) {
        Ok(value) => value,
        Err(_) => JsValue::from_str(&format!("{}: {}", error.code, error.message)),
    }
}

fn to_js<T: serde::Serialize>(value: &T) -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(value).map_err(|error| JsValue::from_str(&error.to_string()))
}

/// A BicMath engine instance for the browser.
#[wasm_bindgen]
pub struct BicMath {
    engine: Engine,
}

#[wasm_bindgen]
impl BicMath {
    /// Create an engine with every module compiled into this WASM build.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<BicMath, JsValue> {
        let engine = Engine::full(EngineConfig::default()).map_err(|errors| {
            JsValue::from_str(
                &errors
                    .iter()
                    .map(|error| error.to_string())
                    .collect::<Vec<_>>()
                    .join("; "),
            )
        })?;
        Ok(BicMath { engine })
    }

    /// Engine version.
    #[wasm_bindgen(getter)]
    pub fn version(&self) -> String {
        bicmath_core::VERSION.to_string()
    }

    /// Wire schema version.
    #[wasm_bindgen(getter, js_name = wireSchemaVersion)]
    pub fn wire_schema_version(&self) -> u32 {
        bicmath_core::WIRE_SCHEMA_VERSION
    }

    /// Execute one function from JSON text; returns the envelope as JSON text.
    #[wasm_bindgen(js_name = calculateJson)]
    pub fn calculate_json(&self, request_json: &str) -> Result<String, JsValue> {
        let request: CallRequest = serde_json::from_str(request_json)
            .map_err(|error| JsValue::from_str(&format!("invalid request JSON: {error}")))?;
        let envelope = self
            .engine
            .call(&request, &self.engine.base_context())
            .map_err(engine_error)?;
        serde_json::to_string(&envelope).map_err(|error| JsValue::from_str(&error.to_string()))
    }

    /// Evaluate an expression from JSON text; returns the envelope as JSON text.
    #[wasm_bindgen(js_name = evaluateJson)]
    pub fn evaluate_json(&self, request_json: &str) -> Result<String, JsValue> {
        let request: EvaluateRequest = serde_json::from_str(request_json)
            .map_err(|error| JsValue::from_str(&format!("invalid request JSON: {error}")))?;
        let envelope = self
            .engine
            .evaluate(&request, &self.engine.base_context())
            .map_err(engine_error)?;
        serde_json::to_string(&envelope).map_err(|error| JsValue::from_str(&error.to_string()))
    }

    /// Execute a batch from JSON text; returns the batch envelope as JSON text.
    #[wasm_bindgen(js_name = batchJson)]
    pub fn batch_json(&self, request_json: &str) -> Result<String, JsValue> {
        let request: BatchRequest = serde_json::from_str(request_json)
            .map_err(|error| JsValue::from_str(&format!("invalid request JSON: {error}")))?;
        let envelope = self
            .engine
            .batch(&request, &self.engine.base_context())
            .map_err(engine_error)?;
        serde_json::to_string(&envelope).map_err(|error| JsValue::from_str(&error.to_string()))
    }

    /// Execute one function from a JS object; returns a JS object.
    #[wasm_bindgen]
    pub fn calculate(&self, request: JsValue) -> Result<JsValue, JsValue> {
        let request: CallRequest = serde_wasm_bindgen::from_value(request)
            .map_err(|error| JsValue::from_str(&format!("invalid request: {error}")))?;
        let envelope = self
            .engine
            .call(&request, &self.engine.base_context())
            .map_err(engine_error)?;
        to_js(&envelope)
    }

    /// Evaluate an expression from a JS object; returns a JS object.
    #[wasm_bindgen]
    pub fn evaluate(&self, request: JsValue) -> Result<JsValue, JsValue> {
        let request: EvaluateRequest = serde_wasm_bindgen::from_value(request)
            .map_err(|error| JsValue::from_str(&format!("invalid request: {error}")))?;
        let envelope = self
            .engine
            .evaluate(&request, &self.engine.base_context())
            .map_err(engine_error)?;
        to_js(&envelope)
    }

    /// Execute a batch from a JS object; returns a JS object.
    #[wasm_bindgen]
    pub fn batch(&self, request: JsValue) -> Result<JsValue, JsValue> {
        let request: BatchRequest = serde_wasm_bindgen::from_value(request)
            .map_err(|error| JsValue::from_str(&format!("invalid request: {error}")))?;
        let envelope = self
            .engine
            .batch(&request, &self.engine.base_context())
            .map_err(engine_error)?;
        to_js(&envelope)
    }

    /// List enabled modules.
    #[wasm_bindgen(js_name = listModules)]
    pub fn list_modules(&self) -> Result<JsValue, JsValue> {
        to_js(&self.engine.list_modules())
    }

    /// List functions, optionally filtered by module/query/pagination.
    #[wasm_bindgen(js_name = listFunctions)]
    pub fn list_functions(&self, filter: JsValue) -> Result<JsValue, JsValue> {
        let filter: FunctionFilter = if filter.is_undefined() || filter.is_null() {
            FunctionFilter::default()
        } else {
            serde_wasm_bindgen::from_value(filter)
                .map_err(|error| JsValue::from_str(&format!("invalid filter: {error}")))?
        };
        to_js(&self.engine.list_functions(&filter))
    }

    /// Full description for one function, including generated Markdown.
    #[wasm_bindgen(js_name = describeFunction)]
    pub fn describe_function(&self, function: &str) -> Result<JsValue, JsValue> {
        self.engine
            .describe(function)
            .map_err(engine_error)
            .and_then(|value| to_js(&value))
    }

    /// Run every documented example and report module compatibility. This is
    /// the browser-side equivalent of `bicmath doctor`.
    #[wasm_bindgen]
    pub fn doctor(&self) -> Result<JsValue, JsValue> {
        to_js(&self.doctor_report())
    }

    /// The same doctor report as JSON text (usable from any JS runtime).
    #[wasm_bindgen(js_name = doctorJson)]
    pub fn doctor_json(&self) -> String {
        serde_json::to_string(&self.doctor_report()).unwrap_or_else(|_| "{}".to_string())
    }

    fn doctor_report(&self) -> serde_json::Value {
        let failures = self.engine.validate_examples();
        serde_json::json!({
            "version": bicmath_core::VERSION,
            "wire_schema_version": bicmath_core::WIRE_SCHEMA_VERSION,
            "target": "wasm32",
            "modules": self.engine.list_modules(),
            "examples_ok": failures.is_empty(),
            "example_failures": failures,
        })
    }
}

impl Default for BicMath {
    fn default() -> Self {
        BicMath::new().expect("built-in engine must be valid")
    }
}

/// One-shot exact addition helper, convenient for smoke tests.
#[wasm_bindgen(js_name = exactAdd)]
pub fn exact_add(a: &str, b: &str) -> Result<String, JsValue> {
    let engine = Engine::full(EngineConfig::default()).map_err(|errors| {
        JsValue::from_str(
            &errors
                .iter()
                .map(|error| error.to_string())
                .collect::<Vec<_>>()
                .join("; "),
        )
    })?;
    let request: CallRequest = serde_json::from_value(serde_json::json!({
        "function": "arithmetic.add",
        "arguments": {"a": a, "b": b}
    }))
    .map_err(|error| JsValue::from_str(&error.to_string()))?;
    let envelope = engine
        .call(&request, &engine.base_context())
        .map_err(engine_error)?;
    serde_json::to_string(&envelope.result).map_err(|error| JsValue::from_str(&error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_values_survive_json_round_trip() {
        let engine = BicMath::new().unwrap();
        let output = engine
            .calculate_json(r#"{"function":"arithmetic.add","arguments":{"a":"0.1","b":"0.2"}}"#)
            .unwrap();
        let value: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(
            value["result"],
            serde_json::json!({"kind": "decimal", "value": "0.3"})
        );
        let output = engine
            .calculate_json(
                r#"{"function":"arithmetic.add","arguments":{"a":{"kind":"integer","value":"9007199254740993"},"b":1}}"#,
            )
            .unwrap();
        let value: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(
            value["result"],
            serde_json::json!({"kind": "integer", "value": "9007199254740994"})
        );
    }

    #[test]
    fn statistics_works_offline() {
        let engine = BicMath::new().unwrap();
        let output = engine
            .calculate_json(
                r#"{"function":"statistics.median","arguments":{"values":[1,2,10,100]}}"#,
            )
            .unwrap();
        let value: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(
            value["result"],
            serde_json::json!({"kind": "integer", "value": "6"})
        );
    }

    #[test]
    fn all_six_modules_report_and_run() {
        let engine = BicMath::new().unwrap();
        let report: serde_json::Value = serde_json::from_str(&engine.doctor_json()).unwrap();
        assert_eq!(report["examples_ok"], true);
        assert!(report["modules"].as_array().unwrap().len() >= 6);
    }
}
