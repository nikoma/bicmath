//! Canonical encoding, deterministic fingerprints, and replay receipts.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::WIRE_SCHEMA_VERSION;
use crate::context::EffectiveContext;
use crate::contract::FunctionRef;
use crate::envelope::EngineInfo;
use crate::error::EngineError;
use crate::value::Value;

/// Canonical JSON bytes: sorted object keys (serde_json maps are ordered),
/// no insignificant whitespace, no volatile fields. The caller is responsible
/// for excluding timestamps, elapsed time, request ids, and trace noise before
/// calling this.
pub fn canonical_json_bytes(value: &serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(value).unwrap_or_default()
}

/// SHA-256 fingerprint of canonical JSON, hex encoded.
pub fn fingerprint_json(value: &serde_json::Value) -> String {
    let bytes = canonical_json_bytes(value);
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// SHA-256 fingerprint of a typed value.
pub fn fingerprint_value(value: &Value) -> Result<String, EngineError> {
    let json = serde_json::to_value(value)
        .map_err(|e| EngineError::internal(format!("value is not serializable: {e}")))?;
    Ok(fingerprint_json(&json))
}

/// A full replay receipt: effective inputs and method choices, not just a hash.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Receipt {
    pub schema_version: u32,
    pub fingerprint: String,
    /// The effective request, including resolved arguments and method choices.
    pub request: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function: Option<FunctionRef>,
    pub engine: EngineInfo,
    pub context: EffectiveContext,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    pub policy_version: u32,
}

impl Receipt {
    pub fn new(
        fingerprint: String,
        request: serde_json::Value,
        function: Option<FunctionRef>,
        engine: EngineInfo,
        context: EffectiveContext,
        seed: Option<u64>,
    ) -> Receipt {
        Receipt {
            schema_version: WIRE_SCHEMA_VERSION,
            fingerprint,
            request,
            function,
            engine,
            context,
            seed,
            policy_version: crate::NUMERICAL_POLICY_VERSION,
        }
    }
}

/// Build the canonical fingerprint input for a calculation.
///
/// Covers canonical inputs, the expression/batch graph, method and module
/// versions, the effective context, the explicitly supplied seed, and semantic
/// policy versions. It deliberately excludes wall-clock time, elapsed time,
/// request ids, and tracing noise.
#[allow(clippy::too_many_arguments)]
pub fn calculation_fingerprint(
    kind: &str,
    request: &serde_json::Value,
    function: Option<&FunctionRef>,
    module_versions: &BTreeMap<String, String>,
    context: &EffectiveContext,
    seed: Option<u64>,
) -> Result<String, EngineError> {
    let context_json = serde_json::to_value(context)
        .map_err(|e| EngineError::internal(format!("context is not serializable: {e}")))?;
    let input = serde_json::json!({
        "kind": kind,
        "request": request,
        "function": function,
        "module_versions": module_versions,
        "context": context_json,
        "seed": seed,
        "policy_version": crate::NUMERICAL_POLICY_VERSION,
        "wire_schema_version": WIRE_SCHEMA_VERSION,
    });
    Ok(fingerprint_json(&input))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn fingerprint_is_order_independent_for_objects() {
        let a = json!({"b": 1, "a": {"y": 2, "x": 3}});
        let b = json!({"a": {"x": 3, "y": 2}, "b": 1});
        assert_eq!(fingerprint_json(&a), fingerprint_json(&b));
    }

    #[test]
    fn fingerprint_is_stable_vector() {
        // Fixed test vectors: guard against accidental encoding changes.
        // Regenerate deliberately when the canonical encoding is versioned.
        assert_eq!(
            fingerprint_json(&json!({"kind": "decimal", "value": "0.10"})),
            "3d1dd0467829e705dba121199a1cf47dcb07ea61b04eb9132c32ad4f36fa8b53"
        );
        assert_eq!(
            fingerprint_json(&json!({"kind": "integer", "value": "9007199254740993"})),
            "01efc5513dd6e1b5bba1bea7ef0141b4421f4a0fe5cc29352baa52af8b2bf6cf"
        );
    }
}
