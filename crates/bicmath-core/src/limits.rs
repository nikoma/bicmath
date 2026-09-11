//! Resource limits and cooperative cancellation.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::error::{EngineError, ErrorCode};

/// Server-policy resource limits. A caller may request smaller limits through
/// [`LimitsOverride`] but can never raise them above server policy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Limits {
    pub max_request_bytes: usize,
    pub max_expression_length: usize,
    pub max_expression_tokens: usize,
    pub max_ast_depth: usize,
    pub max_digits: usize,
    pub max_integer_bits: u32,
    pub max_decimal_scale: u32,
    pub max_array_len: usize,
    pub max_matrix_elements: usize,
    pub max_batch_nodes: usize,
    pub max_output_bytes: usize,
    pub max_trace_bytes: usize,
    pub max_exponent: u32,
    pub max_factorial: u32,
    pub max_iterations: u64,
    pub max_operations: u64,
    pub max_recursion_depth: usize,
    pub max_string_len: usize,
    pub max_series_terms: u64,
    pub request_timeout_ms: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Limits::conservative()
    }
}

impl Limits {
    /// Documented conservative defaults. See `docs/limits.md` for rationale.
    pub fn conservative() -> Limits {
        Limits {
            max_request_bytes: 1 << 20,
            max_expression_length: 16 << 10,
            max_expression_tokens: 4096,
            max_ast_depth: 64,
            max_digits: 4096,
            max_integer_bits: 8192,
            max_decimal_scale: 4096,
            max_array_len: 100_000,
            max_matrix_elements: 250_000,
            max_batch_nodes: 256,
            max_output_bytes: 4 << 20,
            max_trace_bytes: 256 << 10,
            max_exponent: 100_000,
            max_factorial: 10_000,
            max_iterations: 1_000_000,
            max_operations: 10_000_000,
            max_recursion_depth: 64,
            max_string_len: 1 << 20,
            max_series_terms: 1_000_000,
            request_timeout_ms: 30_000,
        }
    }

    /// Effective limits after applying caller-requested reductions.
    ///
    /// Returns an error when the caller asks for a limit above server policy.
    pub fn lowered_by(&self, overrides: &LimitsOverride) -> Result<Limits, EngineError> {
        let mut out = self.clone();
        macro_rules! lower {
            ($field:ident) => {
                if let Some(value) = overrides.$field {
                    if value > self.$field {
                        return Err(EngineError::new(
                            ErrorCode::ResourceLimit,
                            format!(
                                "requested {} ({}) exceeds server policy ({})",
                                stringify!($field),
                                value,
                                self.$field
                            ),
                        ));
                    }
                    out.$field = value;
                }
            };
        }
        lower!(max_request_bytes);
        lower!(max_expression_length);
        lower!(max_expression_tokens);
        lower!(max_ast_depth);
        lower!(max_digits);
        lower!(max_integer_bits);
        lower!(max_decimal_scale);
        lower!(max_array_len);
        lower!(max_matrix_elements);
        lower!(max_batch_nodes);
        lower!(max_output_bytes);
        lower!(max_trace_bytes);
        lower!(max_exponent);
        lower!(max_factorial);
        lower!(max_iterations);
        lower!(max_operations);
        lower!(max_recursion_depth);
        lower!(max_string_len);
        lower!(max_series_terms);
        lower!(request_timeout_ms);
        Ok(out)
    }

    /// Compute a wall-clock deadline from the timeout.
    ///
    /// WASM has no portable monotonic clock through `std`, so browser builds
    /// run without a wall-clock deadline; operation budgets and cooperative
    /// cancellation still bound work.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn deadline(&self) -> Option<Instant> {
        if self.request_timeout_ms == 0 {
            None
        } else {
            Some(Instant::now() + Duration::from_millis(self.request_timeout_ms))
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn deadline(&self) -> Option<Instant> {
        let _ = self.request_timeout_ms;
        None
    }
}

/// A caller's requested reductions of server limits. `None` means "use server
/// policy". Values above policy are rejected.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LimitsOverride {
    pub max_request_bytes: Option<usize>,
    pub max_expression_length: Option<usize>,
    pub max_expression_tokens: Option<usize>,
    pub max_ast_depth: Option<usize>,
    pub max_digits: Option<usize>,
    pub max_integer_bits: Option<u32>,
    pub max_decimal_scale: Option<u32>,
    pub max_array_len: Option<usize>,
    pub max_matrix_elements: Option<usize>,
    pub max_batch_nodes: Option<usize>,
    pub max_output_bytes: Option<usize>,
    pub max_trace_bytes: Option<usize>,
    pub max_exponent: Option<u32>,
    pub max_factorial: Option<u32>,
    pub max_iterations: Option<u64>,
    pub max_operations: Option<u64>,
    pub max_recursion_depth: Option<usize>,
    pub max_string_len: Option<usize>,
    pub max_series_terms: Option<u64>,
    pub request_timeout_ms: Option<u64>,
}

/// Cooperative cancellation token.
#[derive(Clone, Debug, Default)]
pub struct CancellationToken {
    flag: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn new() -> CancellationToken {
        CancellationToken {
            flag: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }

    /// Return an error if cancellation was requested.
    pub fn check(&self) -> Result<(), EngineError> {
        if self.is_cancelled() {
            Err(EngineError::cancelled())
        } else {
            Ok(())
        }
    }
}

/// A simple operation counter that enforces the configured operation budget.
#[derive(Debug)]
pub struct Budget {
    limit: u64,
    used: u64,
    token: CancellationToken,
}

impl Budget {
    pub fn new(limit: u64, token: CancellationToken) -> Budget {
        Budget {
            limit,
            used: 0,
            token,
        }
    }

    /// Account for one unit of work.
    pub fn tick(&mut self) -> Result<(), EngineError> {
        self.tick_by(1)
    }

    pub fn tick_by(&mut self, amount: u64) -> Result<(), EngineError> {
        self.token.check()?;
        self.used = self.used.saturating_add(amount);
        if self.used > self.limit {
            return Err(EngineError::new(
                ErrorCode::ResourceLimit,
                format!("operation budget of {} exceeded", self.limit),
            ));
        }
        Ok(())
    }

    pub fn used(&self) -> u64 {
        self.used
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overrides_cannot_raise_limits() {
        let policy = Limits::conservative();
        let overrides = LimitsOverride {
            max_array_len: Some(policy.max_array_len + 1),
            ..Default::default()
        };
        assert!(policy.lowered_by(&overrides).is_err());
        let overrides = LimitsOverride {
            max_array_len: Some(10),
            ..Default::default()
        };
        assert_eq!(policy.lowered_by(&overrides).unwrap().max_array_len, 10);
    }

    #[test]
    fn cancellation_is_cooperative() {
        let token = CancellationToken::new();
        assert!(token.check().is_ok());
        token.cancel();
        assert_eq!(token.check().unwrap_err().code, ErrorCode::Cancelled);
    }
}
