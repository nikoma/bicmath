//! The versioned result envelope shared by every adapter.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::WIRE_SCHEMA_VERSION;
use crate::context::EffectiveContext;
use crate::contract::{Assumption, ErrorEstimate, FunctionRef, Trace, Warning};
use crate::error::EngineError;
use crate::fingerprint::Receipt;
use crate::value::Value;

/// How exact a result is. These classifications are mutually exclusive and are
/// never inferred from mere repeatability.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Exactness {
    /// The result is mathematically exact under the selected representation.
    Exact,
    /// The result was rounded under a declared context (e.g. decimal division,
    /// money quantization). The context and inexactness are reported.
    Rounded,
    /// The result is a floating-point approximation. Accuracy is bounded by the
    /// method, not by the representation alone.
    Approximate,
}

impl Exactness {
    pub fn as_str(self) -> &'static str {
        match self {
            Exactness::Exact => "exact",
            Exactness::Rounded => "rounded",
            Exactness::Approximate => "approximate",
        }
    }

    /// Combine classifications, taking the least exact.
    pub fn combine(self, other: Exactness) -> Exactness {
        match (self, other) {
            (Exactness::Approximate, _) | (_, Exactness::Approximate) => Exactness::Approximate,
            (Exactness::Rounded, _) | (_, Exactness::Rounded) => Exactness::Rounded,
            _ => Exactness::Exact,
        }
    }
}

/// Engine build identity. Timestamps and request ids are deliberately excluded
/// from the fingerprint.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineInfo {
    pub name: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build: Option<String>,
    pub modules: BTreeMap<String, String>,
}

impl EngineInfo {
    pub fn current() -> EngineInfo {
        EngineInfo {
            name: "bicmath".to_string(),
            version: crate::VERSION.to_string(),
            build: option_env!("BICMATH_BUILD_ID").map(|s| s.to_string()),
            modules: BTreeMap::new(),
        }
    }
}

/// A successful, inspectable result.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResultEnvelope {
    pub schema_version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function: Option<FunctionRef>,
    pub engine: EngineInfo,
    pub result: Value,
    pub exactness: Exactness,
    pub context: EffectiveContext,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_estimate: Option<ErrorEstimate>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<Warning>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assumptions: Vec<Assumption>,
    pub fingerprint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt: Option<Receipt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace: Option<Trace>,
}

impl ResultEnvelope {
    pub fn new(
        function: Option<FunctionRef>,
        engine: EngineInfo,
        result: Value,
        exactness: Exactness,
        context: EffectiveContext,
        fingerprint: String,
    ) -> ResultEnvelope {
        ResultEnvelope {
            schema_version: WIRE_SCHEMA_VERSION,
            function,
            engine,
            result,
            exactness,
            context,
            error_estimate: None,
            warnings: Vec::new(),
            assumptions: Vec::new(),
            fingerprint,
            receipt: None,
            trace: None,
        }
    }
}

/// A structured error response. Errors never carry a fabricated result.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub schema_version: u32,
    pub engine: EngineInfo,
    pub error: EngineError,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
}

impl ErrorResponse {
    pub fn new(engine: EngineInfo, error: EngineError) -> ErrorResponse {
        ErrorResponse {
            schema_version: WIRE_SCHEMA_VERSION,
            engine,
            error,
            fingerprint: None,
        }
    }
}
