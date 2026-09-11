//! BicMath engine: registry assembly, validation, expression evaluation,
//! dependent batches, deterministic receipts, and the public API shared by
//! the CLI, MCP server, WASM bindings, and Rust embedders.
//!
//! The engine is synchronous and free of transport, async runtime, filesystem,
//! and network dependencies, so the same registry runs natively and in the
//! browser.

pub mod batch;
pub mod envelope;
pub mod evaluator;
pub mod recipes;
pub mod registry;
pub mod request;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use bicmath_core::context::{ExecContext, TraceLevel};
use bicmath_core::envelope::ResultEnvelope;
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::expr::parse_expression;
use bicmath_core::limits::Limits;
use bicmath_core::number::{NumericContext, NumericMode, RoundingMode};
use bicmath_core::value::Value;

pub use batch::{BatchEnvelope, BatchNode, BatchRequest, NodeResult, NodeStatus};
pub use registry::{Registry, RegistryBuilder};
pub use request::{
    CallOptions, CallRequest, ContextOverrides, EvaluateRequest, FunctionFilter, FunctionPage,
    FunctionSummary, ModuleSummary,
};

/// Engine configuration: compiled modules may be disabled at runtime; numeric
/// defaults and resource limits are server policy.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EngineConfig {
    /// Module ids disabled at runtime. Disabled modules are absent from
    /// discovery and unavailable through every execution path.
    pub disabled_modules: Vec<String>,
    pub default_mode: NumericMode,
    pub precision: u32,
    pub rounding: RoundingMode,
    pub limits: Limits,
}

impl Default for EngineConfig {
    fn default() -> Self {
        EngineConfig {
            disabled_modules: Vec::new(),
            default_mode: NumericMode::Auto,
            precision: 34,
            rounding: RoundingMode::HalfEven,
            limits: Limits::conservative(),
        }
    }
}

/// The assembled engine.
#[derive(Clone, Debug)]
pub struct Engine {
    registry: Registry,
    config: EngineConfig,
}

impl Engine {
    /// Build the engine with every module compiled in by Cargo features.
    pub fn full(config: EngineConfig) -> Result<Engine, Vec<EngineError>> {
        let mut builder = RegistryBuilder::new().with_limits(config.limits.clone());
        for id in &config.disabled_modules {
            builder = builder.disable_module(id.clone());
        }
        builder = registry::add_compiled_modules(builder);
        let registry = builder.build()?;
        Ok(Engine { registry, config })
    }

    /// Build an engine around an existing registry.
    pub fn from_registry(registry: Registry, config: EngineConfig) -> Engine {
        Engine { registry, config }
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    pub fn config(&self) -> &EngineConfig {
        &self.config
    }

    /// The base execution context from engine policy.
    pub fn base_context(&self) -> ExecContext {
        ExecContext {
            numeric: NumericContext {
                mode: self.config.default_mode,
                precision: self.config.precision,
                rounding: self.config.rounding,
                ..NumericContext::default()
            },
            budget: None,
            limits: self.config.limits.clone(),
            cancellation: bicmath_core::limits::CancellationToken::new(),
            deadline: self.config.limits.deadline(),
            seed: None,
            trace: TraceLevel::Off,
        }
    }

    fn resolve_context(
        &self,
        base: &ExecContext,
        overrides: Option<&ContextOverrides>,
    ) -> Result<ExecContext, EngineError> {
        match overrides {
            Some(overrides) => overrides.apply(base),
            None => Ok(base.clone()),
        }
    }

    fn options_for(&self, ctx: &ExecContext, receipt: bool) -> CallOptions {
        CallOptions {
            include_receipt: receipt,
            include_trace: ctx.trace != TraceLevel::Off,
        }
    }

    /// Execute one function with typed arguments.
    pub fn call(
        &self,
        request: &CallRequest,
        base: &ExecContext,
    ) -> Result<ResultEnvelope, EngineError> {
        let ctx = self.resolve_context(base, request.context.as_ref())?;
        let options = self.options_for(&ctx, request.receipt);
        self.registry
            .call(&request.function, &request.arguments, &ctx, &options)
    }

    /// Evaluate a restricted expression with named bindings.
    pub fn evaluate(
        &self,
        request: &EvaluateRequest,
        base: &ExecContext,
    ) -> Result<ResultEnvelope, EngineError> {
        let ctx = self.resolve_context(base, request.context.as_ref())?;
        if request.bindings.len() > 256 {
            return Err(EngineError::new(
                ErrorCode::ResourceLimit,
                "too many named bindings (limit 256)",
            ));
        }
        let expression = parse_expression(&request.expression, &ctx.limits)?;
        let mut bindings = BTreeMap::new();
        for (name, raw) in &request.bindings {
            let value = Value::from_json(raw, &ctx.limits)
                .map_err(|error| error.with_path(name.clone()))?;
            value.check_limits(&ctx.limits, 0)?;
            bindings.insert(name.clone(), value);
        }
        let output = evaluator::evaluate(&self.registry, &expression, &bindings, &ctx)?;
        let outcome = bicmath_core::contract::Outcome {
            value: output.value,
            exactness: output.exactness,
            warnings: output.warnings,
            assumptions: output.assumptions,
            error_estimate: output.error_estimate,
            trace: output.trace,
            conversions: output.conversions,
        };
        let options = self.options_for(&ctx, request.receipt);
        envelope::build_envelope(
            &self.registry,
            None,
            outcome,
            &ctx,
            &options,
            envelope::FingerprintInput::Evaluate {
                expression: &expression,
                bindings: &bindings,
            },
        )
    }

    /// Execute a bounded dependency graph of calculations.
    pub fn batch(
        &self,
        request: &BatchRequest,
        base: &ExecContext,
    ) -> Result<BatchEnvelope, EngineError> {
        let ctx = self.resolve_context(base, request.context.as_ref())?;
        let options = self.options_for(&ctx, request.receipt);
        let request_json = serde_json::to_value(request).map_err(|e| {
            EngineError::internal(format!("batch request is not serializable: {e}"))
        })?;
        batch::run_batch(&self.registry, request, &ctx, &options, &request_json)
    }

    /// Available multi-step workflow recipes.
    pub fn list_recipes(&self) -> Vec<recipes::RecipeDescriptor> {
        recipes::list()
    }

    /// Module catalog for discovery.
    pub fn list_modules(&self) -> Vec<ModuleSummary> {
        self.registry
            .modules()
            .map(|entry| {
                let function_count = self
                    .registry
                    .function_descriptors()
                    .filter(|descriptor| descriptor.module == entry.descriptor.id)
                    .count();
                ModuleSummary {
                    id: entry.descriptor.id.clone(),
                    title: entry.descriptor.title.clone(),
                    version: entry.descriptor.version.clone(),
                    description: entry.descriptor.description.clone(),
                    capabilities: entry.descriptor.capabilities.clone(),
                    dependencies: entry.descriptor.dependencies.clone(),
                    supported_modes: entry.descriptor.supported_modes.clone(),
                    function_count,
                    enabled: entry.enabled,
                }
            })
            .collect()
    }

    /// Filtered, paginated function summaries.
    pub fn list_functions(&self, filter: &FunctionFilter) -> FunctionPage {
        let query = filter.query.as_ref().map(|query| query.to_lowercase());
        let mut matching: Vec<FunctionSummary> = self
            .registry
            .function_descriptors()
            .filter(|descriptor| {
                if let Some(module) = &filter.module
                    && &descriptor.module != module
                {
                    return false;
                }
                if let Some(query) = &query {
                    let haystack = format!(
                        "{} {} {} {}",
                        descriptor.id, descriptor.title, descriptor.summary, descriptor.description
                    )
                    .to_lowercase();
                    if !haystack.contains(query) {
                        return false;
                    }
                }
                true
            })
            .map(|descriptor| FunctionSummary {
                id: descriptor.id.clone(),
                module: descriptor.module.clone(),
                title: descriptor.title.clone(),
                summary: descriptor.summary.clone(),
                modes: descriptor.modes.clone(),
                cost: descriptor.cost,
                tags: descriptor.tags.clone(),
                deprecated: descriptor.deprecated.clone(),
            })
            .collect();
        matching.sort_by(|a, b| a.id.cmp(&b.id));
        let total = matching.len();
        let offset = filter.offset.unwrap_or(0).min(total);
        let limit = filter.effective_limit();
        let page: Vec<FunctionSummary> = matching.into_iter().skip(offset).take(limit).collect();
        let next_offset = if offset + page.len() < total {
            Some(offset + page.len())
        } else {
            None
        };
        FunctionPage {
            functions: page,
            total,
            offset,
            limit,
            next_offset,
        }
    }

    /// Full function description as JSON, including generated documentation.
    pub fn describe(&self, id: &str) -> Result<serde_json::Value, EngineError> {
        let function = self.registry.function(id)?;
        let descriptor = function.descriptor();
        let markdown = render_function_markdown(descriptor);
        Ok(serde_json::json!({
            "schema_version": bicmath_core::WIRE_SCHEMA_VERSION,
            "function": descriptor,
            "markdown": markdown,
        }))
    }

    /// Execute every documented example. Intended for tests and `doctor`.
    pub fn validate_examples(&self) -> Vec<EngineError> {
        registry::validate_examples(&self.registry)
    }
}

/// Render a function descriptor as Markdown documentation.
pub fn render_function_markdown(descriptor: &bicmath_core::contract::FunctionDescriptor) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", descriptor.id));
    out.push_str(&format!("{}\n\n", descriptor.summary));
    if !descriptor.description.is_empty() {
        out.push_str(&format!("{}\n\n", descriptor.description));
    }
    out.push_str(&format!(
        "- Module: `{}` (version {})\n- Modes: {}\n- Cost: {:?}\n- Determinism: {:?}\n",
        descriptor.module,
        descriptor.version,
        descriptor
            .modes
            .iter()
            .map(|mode| mode.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        descriptor.cost,
        descriptor.determinism
    ));
    if !descriptor.method_ref.is_empty() {
        out.push_str(&format!("- Method reference: {}\n", descriptor.method_ref));
    }
    if !descriptor.units_rule.is_empty() {
        out.push_str(&format!(
            "- Units/currency rule: {}\n",
            descriptor.units_rule
        ));
    }
    out.push_str("\n## Parameters\n\n");
    for param in &descriptor.parameters {
        out.push_str(&format!(
            "- `{}`{} — {} ({})\n",
            param.name,
            if param.required { "" } else { " (optional)" },
            param.description,
            param.schema.summary()
        ));
    }
    out.push_str(&format!(
        "\n## Output\n\n{}\n",
        descriptor.output_description
    ));
    if !descriptor.examples.is_empty() {
        out.push_str("\n## Examples\n\n");
        for example in &descriptor.examples {
            out.push_str(&format!("### {}\n\n", example.title));
            let arguments = serde_json::to_string_pretty(&example.arguments).unwrap_or_default();
            out.push_str(&format!("```json\n{arguments}\n```\n\n"));
            if let Some(expected) = &example.expected {
                out.push_str(&format!(
                    "Expected: `{}`\n\n",
                    serde_json::to_string(expected).unwrap_or_default()
                ));
            }
        }
    }
    out
}
