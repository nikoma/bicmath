//! Envelope construction, fingerprints, and receipts.

use std::collections::BTreeMap;

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{Args, FunctionDescriptor, FunctionRef, Outcome, Trace};
use bicmath_core::envelope::{EngineInfo, Exactness, ResultEnvelope};
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::expr::Expr;
use bicmath_core::fingerprint::{Receipt, calculation_fingerprint};
use bicmath_core::value::Value;

use crate::registry::Registry;
use crate::request::{CallOptions, arguments_to_json};

/// What is being fingerprinted.
pub enum FingerprintInput<'a> {
    Call {
        function_id: &'a str,
        arguments: &'a Args,
    },
    Evaluate {
        expression: &'a Expr,
        bindings: &'a BTreeMap<String, Value>,
    },
    Batch {
        request: &'a serde_json::Value,
    },
    Raw {
        request: &'a serde_json::Value,
    },
}

impl FingerprintInput<'_> {
    fn request_json(&self) -> Result<serde_json::Value, EngineError> {
        match self {
            FingerprintInput::Call {
                function_id,
                arguments,
            } => Ok(serde_json::json!({
                "function": function_id,
                "arguments": arguments_to_json(arguments)?,
            })),
            FingerprintInput::Evaluate {
                expression,
                bindings,
            } => Ok(serde_json::json!({
                "expression": expression.canonical_json(),
                "bindings": bindings,
            })),
            FingerprintInput::Batch { request } | FingerprintInput::Raw { request } => {
                Ok((*request).clone())
            }
        }
    }

    fn kind(&self) -> &'static str {
        match self {
            FingerprintInput::Call { .. } => "calculate",
            FingerprintInput::Evaluate { .. } => "evaluate",
            FingerprintInput::Batch { .. } => "batch",
            FingerprintInput::Raw { .. } => "raw",
        }
    }
}

/// Build the versioned result envelope for a successful invocation.
pub fn build_envelope(
    registry: &Registry,
    descriptor: Option<&FunctionDescriptor>,
    outcome: Outcome,
    ctx: &ExecContext,
    options: &CallOptions,
    input: FingerprintInput<'_>,
) -> Result<ResultEnvelope, EngineError> {
    let request_json = input.request_json()?;
    let function_ref = descriptor.map(|descriptor| FunctionRef {
        id: descriptor.id.clone(),
        version: descriptor.version.clone(),
    });
    let mut effective = ctx.effective();
    effective.conversions = outcome.conversions.clone();
    let fingerprint = calculation_fingerprint(
        input.kind(),
        &request_json,
        function_ref.as_ref(),
        &registry.engine_info().modules,
        &effective,
        ctx.seed,
    )?;
    let engine = registry.engine_info().clone();
    let trace = if options.include_trace {
        bound_trace(outcome.trace, ctx.limits.max_trace_bytes)
    } else {
        None
    };
    let mut envelope = ResultEnvelope::new(
        function_ref.clone(),
        engine.clone(),
        outcome.value,
        outcome.exactness,
        effective.clone(),
        fingerprint.clone(),
    );
    envelope.error_estimate = outcome.error_estimate;
    envelope.warnings = outcome.warnings;
    envelope.assumptions = outcome.assumptions;
    envelope.trace = trace;
    if options.include_receipt {
        envelope.receipt = Some(Receipt::new(
            fingerprint,
            request_json,
            function_ref,
            engine,
            effective,
            ctx.seed,
        ));
    }
    Ok(envelope)
}

/// Bound a trace to the configured byte budget by dropping trailing steps.
fn bound_trace(trace: Option<Trace>, max_bytes: usize) -> Option<Trace> {
    let mut trace = trace?;
    let mut kept: Vec<bicmath_core::contract::TraceStep> = Vec::new();
    let mut bytes = 2usize; // "[]"
    for step in trace.steps.drain(..) {
        let size = serde_json::to_string(&step).map(|s| s.len()).unwrap_or(0) + 1;
        if bytes + size > max_bytes {
            break;
        }
        bytes += size;
        kept.push(step);
    }
    trace.steps = kept;
    Some(trace)
}

/// Build an error response.
pub fn error_response(
    registry: &Registry,
    error: EngineError,
) -> bicmath_core::envelope::ErrorResponse {
    bicmath_core::envelope::ErrorResponse::new(registry.engine_info().clone(), error)
}

/// Convenience: an internal error for a missing envelope invariant.
pub fn internal(message: impl Into<String>) -> EngineError {
    EngineError::new(ErrorCode::Internal, message)
}

/// Combine two exactness classifications.
pub fn combine(a: Exactness, b: Exactness) -> Exactness {
    a.combine(b)
}

/// Engine info with module versions from the registry.
pub fn engine_info(registry: &Registry) -> EngineInfo {
    registry.engine_info().clone()
}
