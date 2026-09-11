//! Execution and effective numeric contexts.

use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::error::{EngineError, ErrorCode};
use crate::limits::{CancellationToken, Limits};
pub use crate::number::NumericContext;

/// Working-precision preset requested by the caller.
///
/// A budget selects only the working precision. It deliberately does **not**
/// change method defaults, tolerances, or convergence criteria: different
/// algorithms have different conditioning, and a tighter requested tolerance
/// does not by itself produce a more accurate answer. Iterative methods report
/// their own requested tolerance, error estimate, residual, and convergence
/// status as separate fields, and decimal calculations that round still report
/// `rounded`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Budget {
    /// 15 significant digits.
    Fast,
    /// 34 significant digits (default).
    Balanced,
    /// 100 significant digits.
    Precise,
}

impl Budget {
    /// Working precision in significant digits.
    pub fn precision(self) -> u32 {
        match self {
            Budget::Fast => 15,
            Budget::Balanced => 34,
            Budget::Precise => 100,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Budget::Fast => "fast",
            Budget::Balanced => "balanced",
            Budget::Precise => "precise",
        }
    }
}

/// Trace verbosity requested by the caller.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceLevel {
    /// No trace is recorded.
    #[default]
    Off,
    /// Only top-level steps are recorded.
    Summary,
    /// All bounded intermediate steps are recorded, subject to `max_trace_bytes`.
    Full,
}

/// A recorded numeric conversion or promotion, for provenance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conversion {
    pub from: String,
    pub to: String,
    pub reason: String,
    pub exact: bool,
}

/// The subset of applied limits that can affect an algorithm or its result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppliedLimits {
    pub request_timeout_ms: u64,
    pub max_operations: u64,
    pub max_iterations: u64,
    pub max_array_len: usize,
    pub max_matrix_elements: usize,
    pub max_batch_nodes: usize,
    pub max_output_bytes: usize,
}

impl From<&Limits> for AppliedLimits {
    fn from(limits: &Limits) -> Self {
        AppliedLimits {
            request_timeout_ms: limits.request_timeout_ms,
            max_operations: limits.max_operations,
            max_iterations: limits.max_iterations,
            max_array_len: limits.max_array_len,
            max_matrix_elements: limits.max_matrix_elements,
            max_batch_nodes: limits.max_batch_nodes,
            max_output_bytes: limits.max_output_bytes,
        }
    }
}

/// The effective context recorded in a result envelope.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectiveContext {
    pub numeric: NumericContext,
    /// The requested budget preset, when one was supplied. The numeric
    /// context records the effective precision; methods report their own
    /// achieved tolerances and convergence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget: Option<Budget>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    pub limits: AppliedLimits,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conversions: Vec<Conversion>,
}

/// The context passed to every function invocation.
#[derive(Clone, Debug)]
pub struct ExecContext {
    pub numeric: NumericContext,
    /// Budget preset, used by iterative methods for default tolerances.
    pub budget: Option<Budget>,
    pub limits: Limits,
    pub cancellation: CancellationToken,
    pub deadline: Option<Instant>,
    pub seed: Option<u64>,
    pub trace: TraceLevel,
}

impl Default for ExecContext {
    fn default() -> Self {
        ExecContext::conservative()
    }
}

impl ExecContext {
    pub fn conservative() -> ExecContext {
        let limits = Limits::conservative();
        ExecContext {
            deadline: limits.deadline(),
            numeric: NumericContext::default(),
            budget: None,
            limits,
            cancellation: CancellationToken::new(),
            seed: None,
            trace: TraceLevel::Off,
        }
    }

    pub fn with_numeric(mut self, numeric: NumericContext) -> ExecContext {
        self.numeric = numeric;
        self
    }

    pub fn with_limits(mut self, limits: Limits) -> ExecContext {
        self.deadline = limits.deadline();
        self.limits = limits;
        self
    }

    pub fn with_seed(mut self, seed: Option<u64>) -> ExecContext {
        self.seed = seed;
        self
    }

    pub fn with_trace(mut self, trace: TraceLevel) -> ExecContext {
        self.trace = trace;
        self
    }

    pub fn with_budget(mut self, budget: Option<Budget>) -> ExecContext {
        self.budget = budget;
        if let Some(budget) = budget {
            self.numeric.precision = budget.precision();
        }
        self
    }

    pub fn exact() -> ExecContext {
        ExecContext {
            numeric: NumericContext::exact(),
            ..ExecContext::conservative()
        }
    }

    pub fn scientific() -> ExecContext {
        ExecContext {
            numeric: NumericContext::scientific(),
            ..ExecContext::conservative()
        }
    }

    /// Check cancellation and deadline.
    pub fn check(&self) -> Result<(), EngineError> {
        self.cancellation.check()?;
        // `deadline` is only ever `Some` on platforms with a monotonic clock;
        // WASM leaves it `None` and relies on budgets and cancellation.
        if let Some(deadline) = self.deadline
            && Instant::now() > deadline
        {
            return Err(EngineError::new(
                ErrorCode::ResourceLimit,
                format!(
                    "request exceeded the {} ms time budget",
                    self.limits.request_timeout_ms
                ),
            ));
        }
        Ok(())
    }

    pub fn effective(&self) -> EffectiveContext {
        EffectiveContext {
            numeric: self.numeric.clone(),
            budget: self.budget,
            seed: self.seed,
            limits: AppliedLimits::from(&self.limits),
            conversions: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_context_serializes() {
        let ctx = ExecContext::conservative();
        let effective = ctx.effective();
        let json = serde_json::to_string(&effective).unwrap();
        assert!(json.contains("\"mode\":\"auto\""));
    }
}
