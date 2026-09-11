//! Stable error codes and structured errors.

use serde::{Deserialize, Serialize};

/// Stable machine-readable error codes. These are part of the wire contract:
/// callers may branch on them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// Input was not valid JSON or did not match the declared schema shape.
    MalformedInput,
    /// The requested function id does not exist.
    UnknownFunction,
    /// The requested module id does not exist.
    UnknownModule,
    /// The module exists but is disabled by configuration.
    DisabledModule,
    /// The input was well-formed but outside the function's mathematical domain.
    DomainViolation,
    /// Division or remainder by zero.
    DivisionByZero,
    /// Quantity dimensions are incompatible.
    IncompatibleUnits,
    /// Two monetary amounts with different currencies were combined without an
    /// explicit conversion.
    CurrencyMismatch,
    /// Not enough observations for the requested estimator.
    InsufficientObservations,
    /// Matrix is singular to working precision.
    SingularMatrix,
    /// Matrix is numerically ill-conditioned; results may be unreliable.
    IllConditioned,
    /// The operation is not supported in the selected numeric mode.
    UnsupportedNumericMode,
    /// The result would exceed configured precision.
    PrecisionLimit,
    /// An iterative method failed to converge.
    NonConvergence,
    /// A configured resource limit was reached.
    ResourceLimit,
    /// The caller cancelled the request.
    Cancelled,
    /// A batch node failed and a dependent node could not run.
    BatchDependencyFailed,
    /// The operation is recognised but not implemented in this release.
    UnsupportedOperation,
    /// A referenced batch node, binding, or resource was not found.
    NotFound,
    /// Internal invariant violation. Never returned for ordinary user input.
    Internal,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::MalformedInput => "malformed_input",
            ErrorCode::UnknownFunction => "unknown_function",
            ErrorCode::UnknownModule => "unknown_module",
            ErrorCode::DisabledModule => "disabled_module",
            ErrorCode::DomainViolation => "domain_violation",
            ErrorCode::DivisionByZero => "division_by_zero",
            ErrorCode::IncompatibleUnits => "incompatible_units",
            ErrorCode::CurrencyMismatch => "currency_mismatch",
            ErrorCode::InsufficientObservations => "insufficient_observations",
            ErrorCode::SingularMatrix => "singular_matrix",
            ErrorCode::IllConditioned => "ill_conditioned",
            ErrorCode::UnsupportedNumericMode => "unsupported_numeric_mode",
            ErrorCode::PrecisionLimit => "precision_limit",
            ErrorCode::NonConvergence => "non_convergence",
            ErrorCode::ResourceLimit => "resource_limit",
            ErrorCode::Cancelled => "cancelled",
            ErrorCode::BatchDependencyFailed => "batch_dependency_failed",
            ErrorCode::UnsupportedOperation => "unsupported_operation",
            ErrorCode::NotFound => "not_found",
            ErrorCode::Internal => "internal",
        }
    }
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A structured engine error. Errors never carry a fabricated successful result.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EngineError {
    pub code: ErrorCode,
    pub message: String,
    /// JSON-pointer-like path to the offending input, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Additional structured detail. Always a JSON object.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub details: serde_json::Value,
}

impl EngineError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> EngineError {
        EngineError {
            code,
            message: message.into(),
            path: None,
            details: serde_json::Value::Null,
        }
    }

    pub fn malformed(message: impl Into<String>) -> EngineError {
        Self::new(ErrorCode::MalformedInput, message)
    }

    pub fn domain(message: impl Into<String>) -> EngineError {
        Self::new(ErrorCode::DomainViolation, message)
    }

    pub fn division_by_zero(message: impl Into<String>) -> EngineError {
        Self::new(ErrorCode::DivisionByZero, message)
    }

    pub fn unsupported(message: impl Into<String>) -> EngineError {
        Self::new(ErrorCode::UnsupportedOperation, message)
    }

    pub fn resource(message: impl Into<String>) -> EngineError {
        Self::new(ErrorCode::ResourceLimit, message)
    }

    pub fn internal(message: impl Into<String>) -> EngineError {
        Self::new(ErrorCode::Internal, message)
    }

    pub fn cancelled() -> EngineError {
        Self::new(ErrorCode::Cancelled, "calculation was cancelled")
    }

    pub fn with_path(mut self, path: impl Into<String>) -> EngineError {
        self.path = Some(path.into());
        self
    }

    pub fn with_details(mut self, details: serde_json::Value) -> EngineError {
        self.details = details;
        self
    }

    /// Wrap another error as a batch dependency failure.
    pub fn batch_dependency(node: &str, source: &EngineError) -> EngineError {
        EngineError::new(
            ErrorCode::BatchDependencyFailed,
            format!("node {node:?} failed: {}", source.message),
        )
        .with_details(serde_json::json!({
            "node": node,
            "source_code": source.code.as_str(),
        }))
    }
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)?;
        if let Some(path) = &self.path {
            write!(f, " (at {path})")?;
        }
        Ok(())
    }
}

impl std::error::Error for EngineError {}
