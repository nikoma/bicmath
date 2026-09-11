//! Request types, context overrides, and shared argument validation.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use bicmath_core::context::{Budget, ExecContext};
use bicmath_core::contract::{Args, FunctionDescriptor};
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::limits::LimitsOverride;
use bicmath_core::number::{NumericMode, RoundingMode};
use bicmath_core::value::Value;

/// Server policy ceiling for requested decimal precision.
pub const MAX_PRECISION: u32 = 1000;

/// A single `calculate` request.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CallRequest {
    pub function: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
    #[serde(default)]
    pub context: Option<ContextOverrides>,
    #[serde(default)]
    pub receipt: bool,
}

/// An `evaluate` request.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvaluateRequest {
    pub expression: String,
    #[serde(default)]
    pub bindings: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    pub context: Option<ContextOverrides>,
    #[serde(default)]
    pub receipt: bool,
}

/// Caller-requested context overrides. A caller may lower resource limits but
/// never raise them above server policy.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextOverrides {
    #[serde(default)]
    pub mode: Option<NumericMode>,
    /// Precision/speed preset (see [`Budget`]); overridden by an explicit
    /// `precision`. Iterative methods use it to scale default tolerances and
    /// still report their achieved tolerance and convergence.
    #[serde(default)]
    pub budget: Option<Budget>,
    #[serde(default)]
    pub precision: Option<u32>,
    #[serde(default)]
    pub rounding: Option<RoundingMode>,
    #[serde(default)]
    pub seed: Option<u64>,
    #[serde(default)]
    pub trace: Option<bicmath_core::context::TraceLevel>,
    #[serde(default)]
    pub limits: Option<LimitsOverride>,
}

impl ContextOverrides {
    pub fn apply(&self, base: &ExecContext) -> Result<ExecContext, EngineError> {
        let mut ctx = base.clone();
        if let Some(mode) = self.mode {
            ctx.numeric.mode = mode;
        }
        if let Some(budget) = self.budget {
            ctx.budget = Some(budget);
            ctx.numeric.precision = budget.precision();
        }
        if let Some(precision) = self.precision {
            if precision == 0 || precision > MAX_PRECISION {
                return Err(EngineError::new(
                    ErrorCode::ResourceLimit,
                    format!("requested precision {precision} is outside 1..={MAX_PRECISION}"),
                ));
            }
            ctx.numeric.precision = precision;
        }
        if let Some(rounding) = self.rounding {
            ctx.numeric.rounding = rounding;
        }
        if let Some(seed) = self.seed {
            ctx.seed = Some(seed);
        }
        if let Some(trace) = self.trace {
            ctx.trace = trace;
        }
        if let Some(limits) = &self.limits {
            ctx.limits = base.limits.lowered_by(limits)?;
            ctx.deadline = ctx.limits.deadline();
        }
        Ok(ctx)
    }
}

/// Options controlling envelope construction.
#[derive(Clone, Debug, Default)]
pub struct CallOptions {
    pub include_receipt: bool,
    pub include_trace: bool,
}

/// Require that the context mode is supported by a function.
pub fn require_mode(
    ctx: &ExecContext,
    modes: &[NumericMode],
    function_id: &str,
) -> Result<(), EngineError> {
    if modes.contains(&ctx.numeric.mode) {
        Ok(())
    } else {
        Err(EngineError::new(
            ErrorCode::UnsupportedNumericMode,
            format!(
                "function {function_id} supports modes {:?}, not {}",
                modes, ctx.numeric.mode
            ),
        ))
    }
}

/// Coerce and validate raw JSON arguments against the function descriptor.
/// This is the single validation path shared by `calculate`, `evaluate`, and
/// `batch`.
pub fn coerce_arguments(
    descriptor: &FunctionDescriptor,
    raw: &serde_json::Value,
    limits: &bicmath_core::limits::Limits,
) -> Result<Args, EngineError> {
    let empty = serde_json::Map::new();
    let map = match raw {
        serde_json::Value::Null => &empty,
        serde_json::Value::Object(map) => map,
        other => {
            return Err(EngineError::malformed(format!(
                "arguments must be a JSON object, found {}",
                other
            )));
        }
    };
    let mut values = BTreeMap::new();
    for param in &descriptor.parameters {
        match map.get(&param.name) {
            Some(serde_json::Value::Null) | None => {
                if param.required {
                    return Err(EngineError::malformed(format!(
                        "missing required argument {:?}",
                        param.name
                    ))
                    .with_path(param.name.clone()));
                }
            }
            Some(value) => {
                let coerced =
                    param
                        .schema
                        .coerce(value, &param.name, limits, param.numeric_shorthand)?;
                values.insert(param.name.clone(), coerced);
            }
        }
    }
    for key in map.keys() {
        if descriptor.parameter(key).is_none() {
            return Err(
                EngineError::malformed(format!("unknown argument {key:?}")).with_path(key.clone())
            );
        }
    }
    Ok(Args::new(values))
}

/// Build the raw JSON object form of validated arguments.
pub fn arguments_to_json(args: &Args) -> Result<serde_json::Value, EngineError> {
    let mut map = serde_json::Map::new();
    for (name, value) in args.iter() {
        map.insert(
            name.clone(),
            serde_json::to_value(value)
                .map_err(|e| EngineError::internal(format!("argument is not serializable: {e}")))?,
        );
    }
    Ok(serde_json::Value::Object(map))
}

/// Validate an already-typed value against a descriptor parameter.
pub fn validate_argument(
    descriptor: &FunctionDescriptor,
    name: &str,
    value: &Value,
    limits: &bicmath_core::limits::Limits,
) -> Result<(), EngineError> {
    let Some(param) = descriptor.parameter(name) else {
        return Err(EngineError::malformed(format!("unknown argument {name:?}")));
    };
    param.schema.validate(value, name, limits)
}

/// Serialize a context's numeric settings for receipts.
pub fn context_json(ctx: &ExecContext) -> serde_json::Value {
    serde_json::json!({
        "numeric": ctx.numeric,
        "seed": ctx.seed,
        "trace": ctx.trace,
        "limits": {
            "request_timeout_ms": ctx.limits.request_timeout_ms,
            "max_operations": ctx.limits.max_operations,
            "max_iterations": ctx.limits.max_iterations,
        }
    })
}

/// A compact, serializable summary of a function.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FunctionSummary {
    pub id: String,
    pub module: String,
    pub title: String,
    pub summary: String,
    pub modes: Vec<NumericMode>,
    pub cost: bicmath_core::contract::CostClass,
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<bicmath_core::contract::Deprecation>,
}

/// A compact, serializable summary of a module.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModuleSummary {
    pub id: String,
    pub title: String,
    pub version: String,
    pub description: String,
    pub capabilities: Vec<String>,
    pub dependencies: Vec<String>,
    pub supported_modes: Vec<NumericMode>,
    pub function_count: usize,
    pub enabled: bool,
}

/// Filter and pagination for `list_functions`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FunctionFilter {
    #[serde(default)]
    pub module: Option<String>,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub offset: Option<usize>,
    #[serde(default)]
    pub limit: Option<usize>,
}

impl FunctionFilter {
    pub const DEFAULT_LIMIT: usize = 50;
    pub const MAX_LIMIT: usize = 200;

    pub fn effective_limit(&self) -> usize {
        self.limit
            .unwrap_or(Self::DEFAULT_LIMIT)
            .min(Self::MAX_LIMIT)
    }
}

/// A page of function summaries.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FunctionPage {
    pub functions: Vec<FunctionSummary>,
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_offset: Option<usize>,
}
