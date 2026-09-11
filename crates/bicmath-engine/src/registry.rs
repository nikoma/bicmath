//! Registry assembly and validation.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bicmath_core::contract::{Function, FunctionDescriptor, Module, ModuleDescriptor};
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::limits::Limits;

use bicmath_core::envelope::EngineInfo;

use crate::request::CallOptions;

/// A registered module plus its enablement state.
#[derive(Clone)]
pub struct ModuleEntry {
    pub descriptor: ModuleDescriptor,
    pub enabled: bool,
}

/// An immutable, validated registry of modules and functions.
#[derive(Clone)]
pub struct Registry {
    engine: EngineInfo,
    modules: BTreeMap<String, ModuleEntry>,
    functions: BTreeMap<String, Arc<dyn Function>>,
}

impl std::fmt::Debug for Registry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Registry")
            .field("modules", &self.modules.keys().collect::<Vec<_>>())
            .field("functions", &self.functions.keys().collect::<Vec<_>>())
            .finish()
    }
}

/// Builds a registry from statically linked modules.
pub struct RegistryBuilder {
    modules: Vec<Module>,
    disabled: BTreeSet<String>,
    limits: Limits,
}

impl Default for RegistryBuilder {
    fn default() -> Self {
        RegistryBuilder {
            modules: Vec::new(),
            disabled: BTreeSet::new(),
            limits: Limits::conservative(),
        }
    }
}

impl RegistryBuilder {
    pub fn new() -> RegistryBuilder {
        RegistryBuilder::default()
    }

    pub fn add_module(mut self, module: Module) -> RegistryBuilder {
        self.modules.push(module);
        self
    }

    pub fn disable_module(mut self, id: impl Into<String>) -> RegistryBuilder {
        self.disabled.insert(id.into());
        self
    }

    pub fn with_limits(mut self, limits: Limits) -> RegistryBuilder {
        self.limits = limits;
        self
    }

    /// Validate and assemble the registry.
    ///
    /// Rejects duplicate module ids, duplicate function ids, unresolved module
    /// dependencies, functions that declare modes outside their module's
    /// declared modes, and invalid documented examples (arguments that cannot
    /// be coerced against the function's own schema).
    pub fn build(self) -> Result<Registry, Vec<EngineError>> {
        let mut errors = Vec::new();
        let mut modules: BTreeMap<String, ModuleEntry> = BTreeMap::new();
        let mut functions: BTreeMap<String, Arc<dyn Function>> = BTreeMap::new();
        let mut module_versions = BTreeMap::new();

        for module in &self.modules {
            let id = &module.descriptor.id;
            if id.is_empty() {
                errors.push(EngineError::internal("module id must not be empty"));
                continue;
            }
            if modules.contains_key(id) {
                errors.push(EngineError::internal(format!("duplicate module id {id:?}")));
                continue;
            }
            modules.insert(
                id.clone(),
                ModuleEntry {
                    descriptor: module.descriptor.clone(),
                    enabled: !self.disabled.contains(id),
                },
            );
            module_versions.insert(id.clone(), module.descriptor.version.clone());
        }

        // Resolve module dependencies.
        for (id, entry) in &modules {
            for dependency in &entry.descriptor.dependencies {
                if dependency == "core" {
                    continue;
                }
                if !modules.contains_key(dependency) {
                    errors.push(EngineError::internal(format!(
                        "module {id:?} depends on unresolved module {dependency:?}"
                    )));
                }
            }
        }

        for module in &self.modules {
            let module_id = &module.descriptor.id;
            if !modules.contains_key(module_id) {
                continue;
            }
            for function in &module.functions {
                let descriptor = function.descriptor();
                let id = &descriptor.id;
                if descriptor.module != *module_id {
                    errors.push(EngineError::internal(format!(
                        "function {id:?} declares module {:?} but is registered under {module_id:?}",
                        descriptor.module
                    )));
                    continue;
                }
                if functions.contains_key(id) {
                    errors.push(EngineError::internal(format!(
                        "duplicate function id {id:?}"
                    )));
                    continue;
                }
                for mode in &descriptor.modes {
                    if !module.descriptor.supported_modes.contains(mode) {
                        errors.push(EngineError::internal(format!(
                            "function {id:?} declares mode {mode} outside module {module_id:?}'s supported modes"
                        )));
                    }
                }
                for example in &descriptor.examples {
                    if let Err(error) = validate_example(descriptor, example, &self.limits) {
                        errors.push(EngineError::internal(format!(
                            "function {id:?} example {:?} is invalid: {error}",
                            example.title
                        )));
                    }
                }
                functions.insert(id.clone(), function.clone());
            }
        }

        if !errors.is_empty() {
            return Err(errors);
        }

        let mut engine = EngineInfo::current();
        engine.modules = module_versions;
        Ok(Registry {
            engine,
            modules,
            functions,
        })
    }
}

fn validate_example(
    descriptor: &FunctionDescriptor,
    example: &bicmath_core::contract::Example,
    limits: &Limits,
) -> Result<(), EngineError> {
    // Error examples intentionally violate the schema (bad enums, empty arrays,
    // out-of-domain values), so only parameter names are validated for them.
    let expect_error = matches!(
        example.expected,
        Some(bicmath_core::contract::ExampleExpectation::Error(_))
    );
    let mut provided = BTreeSet::new();
    for (name, value) in &example.arguments {
        provided.insert(name.clone());
        let Some(param) = descriptor.parameter(name) else {
            return Err(EngineError::malformed(format!(
                "unknown parameter {name:?}"
            )));
        };
        if expect_error {
            continue;
        }
        let raw = serde_json::to_value(value).map_err(|e| {
            EngineError::internal(format!("example value is not serializable: {e}"))
        })?;
        param
            .schema
            .coerce(&raw, name, limits, param.numeric_shorthand)?;
    }
    if !expect_error {
        for param in &descriptor.parameters {
            if param.required && !provided.contains(&param.name) {
                return Err(EngineError::malformed(format!(
                    "missing required parameter {:?}",
                    param.name
                )));
            }
        }
    }
    if let Some(bicmath_core::contract::ExampleExpectation::Value(expected)) = &example.expected {
        descriptor.output.validate(expected, "expected", limits)?;
    }
    Ok(())
}

impl Registry {
    pub fn engine_info(&self) -> &EngineInfo {
        &self.engine
    }

    pub fn modules(&self) -> impl Iterator<Item = &ModuleEntry> {
        self.modules.values()
    }

    pub fn enabled_modules(&self) -> impl Iterator<Item = &ModuleEntry> {
        self.modules.values().filter(|entry| entry.enabled)
    }

    pub fn module(&self, id: &str) -> Option<&ModuleEntry> {
        self.modules.get(id)
    }

    /// Look up an enabled function by qualified id.
    pub fn function(&self, id: &str) -> Result<&Arc<dyn Function>, EngineError> {
        let Some(module_id) = id.split('.').next() else {
            return Err(EngineError::new(
                ErrorCode::UnknownFunction,
                format!("invalid function id {id:?}"),
            ));
        };
        match self.modules.get(module_id) {
            None => Err(EngineError::new(
                ErrorCode::UnknownModule,
                format!("module {module_id:?} is not registered"),
            )),
            Some(entry) if !entry.enabled => Err(EngineError::new(
                ErrorCode::DisabledModule,
                format!("module {module_id:?} is disabled by configuration"),
            )),
            Some(_) => match self.functions.get(id) {
                Some(function) => Ok(function),
                None => Err(EngineError::new(
                    ErrorCode::UnknownFunction,
                    format!("function {id:?} is not registered"),
                )),
            },
        }
    }

    /// All enabled function descriptors, ordered by id.
    pub fn function_descriptors(&self) -> impl Iterator<Item = &FunctionDescriptor> {
        self.functions
            .values()
            .map(|function| function.descriptor())
            .filter(|descriptor| {
                self.modules
                    .get(&descriptor.module)
                    .map(|entry| entry.enabled)
                    .unwrap_or(false)
            })
    }

    pub fn function_count(&self) -> usize {
        self.functions.len()
    }

    /// Execute a validated call and build a result envelope.
    pub fn call(
        &self,
        function_id: &str,
        arguments: &serde_json::Value,
        ctx: &bicmath_core::context::ExecContext,
        options: &CallOptions,
    ) -> Result<bicmath_core::envelope::ResultEnvelope, EngineError> {
        let function = self.function(function_id)?;
        let descriptor = function.descriptor();
        crate::request::require_mode(ctx, &descriptor.modes, function_id)?;
        let args = crate::request::coerce_arguments(descriptor, arguments, &ctx.limits)?;
        let outcome = function.invoke(&args, ctx)?;
        crate::envelope::build_envelope(
            self,
            Some(descriptor),
            outcome,
            ctx,
            options,
            crate::envelope::FingerprintInput::Call {
                function_id,
                arguments: &args,
            },
        )
    }
}

/// Build the full registry from all modules compiled into the engine.
pub fn full_registry() -> Result<Registry, Vec<EngineError>> {
    let builder = add_compiled_modules(RegistryBuilder::new());
    builder.build()
}

/// Add every module enabled by Cargo features to the builder.
///
/// `mut` is required only when at least one module feature is enabled; builds
/// with no module features still compile cleanly.
#[allow(unused_mut)]
pub fn add_compiled_modules(mut builder: RegistryBuilder) -> RegistryBuilder {
    #[cfg(feature = "arithmetic")]
    {
        builder = builder.add_module(bicmath_arithmetic::module());
    }
    #[cfg(feature = "scientific")]
    {
        builder = builder.add_module(bicmath_scientific::module());
    }
    #[cfg(feature = "statistics")]
    {
        builder = builder.add_module(bicmath_statistics::module());
    }
    #[cfg(feature = "finance")]
    {
        builder = builder.add_module(bicmath_finance::module());
    }
    #[cfg(feature = "units")]
    {
        builder = builder.add_module(bicmath_units::module());
    }
    #[cfg(feature = "linear-algebra")]
    {
        builder = builder.add_module(bicmath_linear_algebra::module());
    }
    #[cfg(feature = "interval")]
    {
        builder = builder.add_module(bicmath_interval::module());
    }
    #[cfg(feature = "verify")]
    {
        builder = builder.add_module(bicmath_verify::module());
    }
    #[cfg(feature = "symbolic")]
    {
        builder = builder.add_module(bicmath_symbolic::module());
    }
    #[cfg(feature = "optimize")]
    {
        builder = builder.add_module(bicmath_optimize::module());
    }
    #[cfg(feature = "algebra")]
    {
        builder = builder.add_module(bicmath_algebra::module());
    }
    #[cfg(feature = "geometry")]
    {
        builder = builder.add_module(bicmath_geometry::module());
    }
    #[cfg(feature = "business")]
    {
        builder = builder.add_module(bicmath_business::module());
    }
    #[cfg(feature = "format")]
    {
        builder = builder.add_module(bicmath_format::module());
    }
    #[cfg(feature = "plan")]
    {
        builder = builder.add_module(bicmath_plan::module());
    }
    builder
}

/// Execute every documented example and report failures. Used by tests and
/// `bicmath doctor`; never run implicitly at request time.
/// Field-wise comparison for documented examples: every field listed in the
/// expected value must match exactly, while additional result fields are
/// allowed. This keeps examples precise about the values they assert without
/// making every additive result field a breaking change.
fn example_value_matches(
    expected: &bicmath_core::value::Value,
    actual: &bicmath_core::value::Value,
) -> bool {
    use bicmath_core::value::Value;
    match (expected, actual) {
        (Value::Record(expected_fields), Value::Record(actual_fields)) => {
            expected_fields.iter().all(|(key, expected_value)| {
                actual_fields
                    .get(key)
                    .is_some_and(|actual_value| example_value_matches(expected_value, actual_value))
            })
        }
        (Value::Array(expected_items), Value::Array(actual_items)) => {
            expected_items.len() == actual_items.len()
                && expected_items
                    .iter()
                    .zip(actual_items)
                    .all(|(expected_item, actual_item)| {
                        example_value_matches(expected_item, actual_item)
                    })
        }
        _ => expected == actual,
    }
}

pub fn validate_examples(registry: &Registry) -> Vec<EngineError> {
    let mut failures = Vec::new();
    for function in registry.functions.values() {
        let descriptor = function.descriptor();
        for example in &descriptor.examples {
            let raw = match serde_json::to_value(&example.arguments) {
                Ok(value) => value,
                Err(error) => {
                    failures.push(EngineError::internal(format!(
                        "example {:?} is not serializable: {error}",
                        example.title
                    )));
                    continue;
                }
            };
            // Error examples document behaviour in a specific mode, so they run
            // under the first declared mode. Value examples run under the first
            // mode that accepts them.
            let is_error_example = matches!(
                example.expected,
                Some(bicmath_core::contract::ExampleExpectation::Error(_))
            );
            let modes: Vec<bicmath_core::number::NumericMode> = if is_error_example {
                descriptor.modes.first().copied().into_iter().collect()
            } else {
                descriptor.modes.clone()
            };
            let mut outcome = None;
            let mut last_error = None;
            for mode in &modes {
                let mut ctx = bicmath_core::context::ExecContext::conservative();
                ctx.numeric.mode = *mode;
                match crate::request::coerce_arguments(descriptor, &raw, &ctx.limits)
                    .and_then(|args| function.invoke(&args, &ctx))
                {
                    Ok(value) => {
                        outcome = Some(value);
                        break;
                    }
                    Err(error) => last_error = Some(error),
                }
            }
            let outcome = match outcome {
                Some(outcome) => outcome,
                None => {
                    let error = last_error.unwrap_or_else(|| EngineError::internal("no modes"));
                    {
                        match &example.expected {
                            Some(bicmath_core::contract::ExampleExpectation::Error(expected)) => {
                                if error.code != *expected {
                                    failures.push(EngineError::internal(format!(
                                        "{} example {:?}: expected error {:?}, got {:?} ({})",
                                        descriptor.id,
                                        example.title,
                                        expected,
                                        error.code,
                                        error.message
                                    )));
                                }
                            }
                            _ => failures.push(EngineError::internal(format!(
                                "{} example {:?} failed: {error}",
                                descriptor.id, example.title
                            ))),
                        }
                        continue;
                    }
                }
            };
            match &example.expected {
                Some(bicmath_core::contract::ExampleExpectation::Value(expected)) => {
                    if !example_value_matches(expected, &outcome.value) {
                        failures.push(EngineError::internal(format!(
                            "{} example {:?}: expected {expected:?}, got {:?}",
                            descriptor.id, example.title, outcome.value
                        )));
                    }
                }
                Some(bicmath_core::contract::ExampleExpectation::Error(expected)) => {
                    failures.push(EngineError::internal(format!(
                        "{} example {:?}: expected error {:?}, but the call succeeded",
                        descriptor.id, example.title, expected
                    )));
                }
                Some(bicmath_core::contract::ExampleExpectation::Contains(needle)) => {
                    let rendered = serde_json::to_string(&outcome.value).unwrap_or_default();
                    if !rendered.contains(needle) {
                        failures.push(EngineError::internal(format!(
                            "{} example {:?}: result does not contain {needle:?}",
                            descriptor.id, example.title
                        )));
                    }
                }
                None => {}
            }
        }
    }
    failures
}
