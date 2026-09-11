//! BicMath core: the shared numerical contract.
//!
//! This crate owns:
//! - [`number`]: exact integers, rationals, decimals, and explicit `f64`.
//! - [`value`]: the recursive wire value model (quantities, money, matrices).
//! - [`error`]: stable error codes and structured errors.
//! - [`limits`]: resource limits and cooperative cancellation.
//! - [`context`]: execution and numeric contexts.
//! - [`schema`]: typed parameter/output schemas and validation.
//! - [`contract`]: module and function descriptors, the [`contract::Function`] trait.
//! - [`envelope`]: the versioned result envelope.
//! - [`fingerprint`]: canonical encoding, fingerprints, and replay receipts.
//! - [`expr`]: the restricted expression grammar and parser (no function resolution).
//!
//! No transport, async runtime, filesystem, clock, or network code lives here.

pub mod context;
pub mod contract;
pub mod envelope;
pub mod error;
pub mod eval;
pub mod expr;
pub mod fingerprint;
pub mod limits;
pub mod number;
pub mod schema;
pub mod value;

pub use context::{Budget, EffectiveContext, ExecContext, TraceLevel};
pub use contract::{
    Args, Assumption, CostClass, Determinism, ErrorEstimate, Example, ExampleExpectation, Function,
    FunctionDescriptor, FunctionRef, Module, ModuleDescriptor, Outcome, ParamDescriptor, Purity,
    Trace, TraceStep, Warning,
};
pub use envelope::{EngineInfo, ErrorResponse, Exactness, ResultEnvelope};
pub use error::{EngineError, ErrorCode};
pub use eval::{FloatCalls, NumberCalls, SimpleCalls, evaluate_f64, evaluate_number};
pub use expr::{Expr, parse_expression};
pub use fingerprint::{Receipt, canonical_json_bytes, fingerprint_json};
pub use limits::{CancellationToken, Limits, LimitsOverride};
pub use number::NumericContext;
pub use number::{Decimal, Float64, Number, NumberResult, NumericMode, RoundingMode};
pub use schema::{FieldSchema, ValueSchema};
pub use value::{Bound, Dimension, Value};

/// Version of the public wire schema produced by this engine.
pub const WIRE_SCHEMA_VERSION: u32 = 1;

/// Version of the numerical policy semantics (rounding defaults, promotion rules,
/// quantile conventions). Bump this whenever a semantic change could alter a
/// downstream financial or statistical result, even if Rust types are unchanged.
pub const NUMERICAL_POLICY_VERSION: u32 = 1;

/// Crate version, as compiled.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
