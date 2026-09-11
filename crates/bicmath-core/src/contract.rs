//! Module and function contracts, invocation arguments, and outcomes.

use std::collections::BTreeMap;
use std::sync::Arc;

use num_bigint::BigInt;
use num_traits::ToPrimitive;
use serde::{Deserialize, Serialize};

use crate::context::{Conversion, ExecContext};
use crate::envelope::Exactness;
use crate::error::{EngineError, ErrorCode};
use crate::number::{Decimal, Number, NumericMode};
use crate::schema::ValueSchema;
use crate::value::Value;

/// A statically registered module.
#[derive(Clone)]
pub struct Module {
    pub descriptor: ModuleDescriptor,
    pub functions: Vec<Arc<dyn Function>>,
}

impl Module {
    pub fn new(descriptor: ModuleDescriptor, functions: Vec<Arc<dyn Function>>) -> Module {
        Module {
            descriptor,
            functions,
        }
    }
}

/// Metadata describing a module.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModuleDescriptor {
    pub id: String,
    pub title: String,
    pub version: String,
    pub description: String,
    pub capabilities: Vec<String>,
    pub dependencies: Vec<String>,
    pub supported_modes: Vec<NumericMode>,
    pub license: String,
    pub source: String,
}

impl ModuleDescriptor {
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        version: impl Into<String>,
        description: impl Into<String>,
    ) -> ModuleDescriptor {
        ModuleDescriptor {
            id: id.into(),
            title: title.into(),
            version: version.into(),
            description: description.into(),
            capabilities: Vec::new(),
            dependencies: Vec::new(),
            supported_modes: vec![NumericMode::Auto],
            license: "MIT".to_string(),
            source: String::new(),
        }
    }

    pub fn with_capabilities(
        mut self,
        capabilities: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.capabilities = capabilities.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_dependencies(
        mut self,
        dependencies: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.dependencies = dependencies.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_modes(mut self, modes: impl IntoIterator<Item = NumericMode>) -> Self {
        self.supported_modes = modes.into_iter().collect();
        self
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = source.into();
        self
    }
}

/// Whether a function has side effects.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Purity {
    Pure,
}

/// Determinism class of a function.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Determinism {
    Deterministic,
    SeededRandom,
}

/// Rough cost class, used for documentation and batch budgeting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CostClass {
    Constant,
    Linear,
    Quadratic,
    Cubic,
    Iterative,
}

/// Deprecation metadata.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Deprecation {
    pub since: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replacement: Option<String>,
}

/// A function parameter.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParamDescriptor {
    pub name: String,
    pub description: String,
    pub schema: ValueSchema,
    pub required: bool,
    /// Whether a string may be used as a numeric shorthand for this parameter.
    pub numeric_shorthand: bool,
    /// Whether the parameter may be supplied positionally in expressions.
    pub positional: bool,
}

impl ParamDescriptor {
    pub fn required(
        name: impl Into<String>,
        description: impl Into<String>,
        schema: ValueSchema,
    ) -> ParamDescriptor {
        let numeric = schema.is_numeric();
        ParamDescriptor {
            name: name.into(),
            description: description.into(),
            schema,
            required: true,
            numeric_shorthand: numeric,
            positional: true,
        }
    }

    pub fn optional(
        name: impl Into<String>,
        description: impl Into<String>,
        schema: ValueSchema,
    ) -> ParamDescriptor {
        let numeric = schema.is_numeric();
        ParamDescriptor {
            name: name.into(),
            description: description.into(),
            schema,
            required: false,
            numeric_shorthand: numeric,
            positional: true,
        }
    }

    pub fn with_shorthand(mut self, allowed: bool) -> Self {
        self.numeric_shorthand = allowed;
        self
    }

    pub fn with_positional(mut self, allowed: bool) -> Self {
        self.positional = allowed;
        self
    }
}

/// Expected outcome of a documented example.
///
/// Serialized explicitly because an internally tagged enum cannot represent
/// newtype variants whose payload is not a map (booleans, arrays, strings).
#[derive(Clone, Debug, PartialEq)]
pub enum ExampleExpectation {
    Value(Value),
    Error(ErrorCode),
    Contains(String),
}

impl Serialize for ExampleExpectation {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        match self {
            ExampleExpectation::Value(value) => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "value")?;
                map.serialize_entry("value", value)?;
                map.end()
            }
            ExampleExpectation::Error(code) => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "error")?;
                map.serialize_entry("code", code)?;
                map.end()
            }
            ExampleExpectation::Contains(text) => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "contains")?;
                map.serialize_entry("text", text)?;
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for ExampleExpectation {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error as DeError;
        let raw = serde_json::Value::deserialize(deserializer)?;
        let kind = raw
            .get("type")
            .and_then(|value| value.as_str())
            .ok_or_else(|| D::Error::custom("example expectation is missing \"type\""))?;
        match kind {
            "value" => {
                let value = raw
                    .get("value")
                    .ok_or_else(|| D::Error::custom("value expectation is missing \"value\""))?;
                Ok(ExampleExpectation::Value(
                    serde_json::from_value(value.clone()).map_err(D::Error::custom)?,
                ))
            }
            "error" => {
                let code = raw
                    .get("code")
                    .ok_or_else(|| D::Error::custom("error expectation is missing \"code\""))?;
                Ok(ExampleExpectation::Error(
                    serde_json::from_value(code.clone()).map_err(D::Error::custom)?,
                ))
            }
            "contains" => {
                let text = raw
                    .get("text")
                    .and_then(|value| value.as_str())
                    .ok_or_else(|| D::Error::custom("contains expectation is missing \"text\""))?;
                Ok(ExampleExpectation::Contains(text.to_string()))
            }
            other => Err(D::Error::custom(format!(
                "unknown example expectation type {other:?}"
            ))),
        }
    }
}

/// A documented, executable example. CI runs every example and checks the
/// expectation so documentation cannot drift from behaviour.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Example {
    pub title: String,
    pub arguments: BTreeMap<String, Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<ExampleExpectation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl Example {
    pub fn new(title: impl Into<String>, arguments: BTreeMap<String, Value>) -> Example {
        Example {
            title: title.into(),
            arguments,
            expected: None,
            note: None,
        }
    }

    pub fn with_value(mut self, value: Value) -> Example {
        self.expected = Some(ExampleExpectation::Value(value));
        self
    }

    pub fn with_error(mut self, code: ErrorCode) -> Example {
        self.expected = Some(ExampleExpectation::Error(code));
        self
    }

    pub fn with_contains(mut self, text: impl Into<String>) -> Example {
        self.expected = Some(ExampleExpectation::Contains(text.into()));
        self
    }
}

/// The authoritative function declaration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FunctionDescriptor {
    pub id: String,
    pub module: String,
    pub version: String,
    pub title: String,
    pub summary: String,
    pub description: String,
    pub parameters: Vec<ParamDescriptor>,
    pub output: ValueSchema,
    pub output_description: String,
    pub modes: Vec<NumericMode>,
    pub purity: Purity,
    pub determinism: Determinism,
    pub cost: CostClass,
    pub units_rule: String,
    pub method_ref: String,
    pub examples: Vec<Example>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<Deprecation>,
    pub tags: Vec<String>,
}

impl FunctionDescriptor {
    pub fn new(
        id: impl Into<String>,
        module: impl Into<String>,
        version: impl Into<String>,
        title: impl Into<String>,
        summary: impl Into<String>,
    ) -> FunctionDescriptor {
        FunctionDescriptor {
            id: id.into(),
            module: module.into(),
            version: version.into(),
            title: title.into(),
            summary: summary.into(),
            description: String::new(),
            parameters: Vec::new(),
            output: ValueSchema::Any,
            output_description: String::new(),
            modes: vec![NumericMode::Auto],
            purity: Purity::Pure,
            determinism: Determinism::Deterministic,
            cost: CostClass::Constant,
            units_rule: String::new(),
            method_ref: String::new(),
            examples: Vec::new(),
            deprecated: None,
            tags: Vec::new(),
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn with_parameters(mut self, parameters: Vec<ParamDescriptor>) -> Self {
        self.parameters = parameters;
        self
    }

    pub fn with_output(mut self, output: ValueSchema, description: impl Into<String>) -> Self {
        self.output = output;
        self.output_description = description.into();
        self
    }

    pub fn with_modes(mut self, modes: impl IntoIterator<Item = NumericMode>) -> Self {
        self.modes = modes.into_iter().collect();
        self
    }

    pub fn with_cost(mut self, cost: CostClass) -> Self {
        self.cost = cost;
        self
    }

    pub fn with_units_rule(mut self, rule: impl Into<String>) -> Self {
        self.units_rule = rule.into();
        self
    }

    pub fn with_method_ref(mut self, method_ref: impl Into<String>) -> Self {
        self.method_ref = method_ref.into();
        self
    }

    pub fn with_examples(mut self, examples: Vec<Example>) -> Self {
        self.examples = examples;
        self
    }

    pub fn with_tags(mut self, tags: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.tags = tags.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_deprecation(mut self, deprecation: Deprecation) -> Self {
        self.deprecated = Some(deprecation);
        self
    }

    pub fn parameter(&self, name: &str) -> Option<&ParamDescriptor> {
        self.parameters.iter().find(|p| p.name == name)
    }
}

/// A reference to a function and its version.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionRef {
    pub id: String,
    pub version: String,
}

/// Warning attached to a result.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Warning {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

impl Warning {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Warning {
        Warning {
            code: code.into(),
            message: message.into(),
            details: None,
        }
    }

    pub fn with_details(mut self, details: Value) -> Warning {
        self.details = Some(details);
        self
    }
}

/// Where an assumption came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssumptionSource {
    /// Supplied by the caller and not verified by the engine.
    UserSupplied,
    /// Checked by the engine for the supplied data.
    Checked,
    /// Required by the method but not verifiable from the supplied inputs.
    Unverified,
}

/// An explicit method assumption.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Assumption {
    pub id: String,
    pub statement: String,
    pub source: AssumptionSource,
}

impl Assumption {
    pub fn user_supplied(id: impl Into<String>, statement: impl Into<String>) -> Assumption {
        Assumption {
            id: id.into(),
            statement: statement.into(),
            source: AssumptionSource::UserSupplied,
        }
    }

    pub fn checked(id: impl Into<String>, statement: impl Into<String>) -> Assumption {
        Assumption {
            id: id.into(),
            statement: statement.into(),
            source: AssumptionSource::Checked,
        }
    }

    pub fn unverified(id: impl Into<String>, statement: impl Into<String>) -> Assumption {
        Assumption {
            id: id.into(),
            statement: statement.into(),
            source: AssumptionSource::Unverified,
        }
    }
}

/// A validated numerical error estimate, only present when the method can
/// actually provide one.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ErrorEstimate {
    pub kind: String,
    pub bound: Value,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

impl ErrorEstimate {
    pub fn new(kind: impl Into<String>, bound: Value, method: impl Into<String>) -> ErrorEstimate {
        ErrorEstimate {
            kind: kind.into(),
            bound,
            method: method.into(),
            notes: None,
        }
    }

    pub fn with_notes(mut self, notes: impl Into<String>) -> ErrorEstimate {
        self.notes = Some(notes.into());
        self
    }
}

/// One bounded trace step.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TraceStep {
    pub op: String,
    pub detail: BTreeMap<String, Value>,
}

/// A bounded execution trace of computational steps. This is not a narrative
/// explanation and contains no hidden reasoning.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Trace {
    pub steps: Vec<TraceStep>,
}

impl Trace {
    pub fn new() -> Trace {
        Trace { steps: Vec::new() }
    }

    pub fn push(&mut self, step: TraceStep) {
        self.steps.push(step);
    }
}

/// A successful invocation outcome.
#[derive(Clone, Debug)]
pub struct Outcome {
    pub value: Value,
    pub exactness: Exactness,
    pub warnings: Vec<Warning>,
    pub assumptions: Vec<Assumption>,
    pub error_estimate: Option<ErrorEstimate>,
    pub trace: Option<Trace>,
    pub conversions: Vec<Conversion>,
}

impl Outcome {
    pub fn new(value: Value, exactness: Exactness) -> Outcome {
        Outcome {
            value,
            exactness,
            warnings: Vec::new(),
            assumptions: Vec::new(),
            error_estimate: None,
            trace: None,
            conversions: Vec::new(),
        }
    }

    pub fn exact(value: Value) -> Outcome {
        Outcome::new(value, Exactness::Exact)
    }

    pub fn rounded(value: Value) -> Outcome {
        Outcome::new(value, Exactness::Rounded)
    }

    pub fn approximate(value: Value) -> Outcome {
        Outcome::new(value, Exactness::Approximate)
    }

    pub fn with_warning(mut self, warning: Warning) -> Outcome {
        self.warnings.push(warning);
        self
    }

    pub fn with_assumption(mut self, assumption: Assumption) -> Outcome {
        self.assumptions.push(assumption);
        self
    }

    pub fn with_error_estimate(mut self, estimate: ErrorEstimate) -> Outcome {
        self.error_estimate = Some(estimate);
        self
    }

    pub fn with_trace(mut self, trace: Trace) -> Outcome {
        self.trace = Some(trace);
        self
    }

    pub fn with_conversion(mut self, conversion: Conversion) -> Outcome {
        self.conversions.push(conversion);
        self
    }
}

/// Validated arguments for a function invocation.
#[derive(Clone, Debug, Default)]
pub struct Args {
    values: BTreeMap<String, Value>,
}

impl Args {
    pub fn new(values: BTreeMap<String, Value>) -> Args {
        Args { values }
    }

    pub fn get(&self, name: &str) -> Option<&Value> {
        self.values.get(name)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.values.contains_key(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &Value)> {
        self.values.iter()
    }

    pub fn into_map(self) -> BTreeMap<String, Value> {
        self.values
    }

    fn expected(name: &str, kind: &str) -> EngineError {
        EngineError::malformed(format!(
            "missing or invalid argument {name:?}: expected {kind}"
        ))
        .with_path(name.to_string())
    }

    pub fn require(&self, name: &str) -> Result<&Value, EngineError> {
        self.values
            .get(name)
            .ok_or_else(|| Self::expected(name, "a value"))
    }

    pub fn number(&self, name: &str) -> Result<&Number, EngineError> {
        self.require(name)?
            .as_number()
            .map_err(|e| e.with_path(name.to_string()))
    }

    pub fn optional_number(&self, name: &str) -> Result<Option<&Number>, EngineError> {
        match self.values.get(name) {
            None | Some(Value::Null) => Ok(None),
            Some(value) => Ok(Some(
                value
                    .as_number()
                    .map_err(|e| e.with_path(name.to_string()))?,
            )),
        }
    }

    pub fn text(&self, name: &str) -> Result<&str, EngineError> {
        self.require(name)?
            .as_text()
            .map_err(|e| e.with_path(name.to_string()))
    }

    pub fn optional_text(&self, name: &str) -> Result<Option<&str>, EngineError> {
        match self.values.get(name) {
            None | Some(Value::Null) => Ok(None),
            Some(value) => Ok(Some(
                value.as_text().map_err(|e| e.with_path(name.to_string()))?,
            )),
        }
    }

    pub fn bool(&self, name: &str) -> Result<bool, EngineError> {
        self.require(name)?
            .as_bool()
            .map_err(|e| e.with_path(name.to_string()))
    }

    pub fn optional_bool(&self, name: &str) -> Result<Option<bool>, EngineError> {
        match self.values.get(name) {
            None | Some(Value::Null) => Ok(None),
            Some(value) => Ok(Some(
                value.as_bool().map_err(|e| e.with_path(name.to_string()))?,
            )),
        }
    }

    pub fn array(&self, name: &str) -> Result<&[Value], EngineError> {
        self.require(name)?
            .as_array()
            .map_err(|e| e.with_path(name.to_string()))
    }

    pub fn record(&self, name: &str) -> Result<&BTreeMap<String, Value>, EngineError> {
        self.require(name)?
            .as_record()
            .map_err(|e| e.with_path(name.to_string()))
    }

    pub fn money(&self, name: &str) -> Result<(&Number, &str), EngineError> {
        self.require(name)?
            .as_money()
            .map_err(|e| e.with_path(name.to_string()))
    }

    pub fn quantity(&self, name: &str) -> Result<(&Number, crate::value::Dimension), EngineError> {
        self.require(name)?
            .as_quantity()
            .map_err(|e| e.with_path(name.to_string()))
    }

    pub fn integer(&self, name: &str) -> Result<BigInt, EngineError> {
        match self.number(name)? {
            Number::Integer(value) => Ok(value.clone()),
            other => Err(Self::expected(
                name,
                &format!("an integer, found {}", other.kind_name()),
            )),
        }
    }

    pub fn optional_integer(&self, name: &str) -> Result<Option<BigInt>, EngineError> {
        match self.optional_number(name)? {
            None => Ok(None),
            Some(Number::Integer(value)) => Ok(Some(value.clone())),
            Some(other) => Err(Self::expected(
                name,
                &format!("an integer, found {}", other.kind_name()),
            )),
        }
    }

    /// Convert an integer argument to `usize`, rejecting negatives and overflow.
    pub fn usize_param(&self, name: &str) -> Result<usize, EngineError> {
        let value = self.integer(name)?;
        value
            .to_usize()
            .ok_or_else(|| Self::expected(name, "a non-negative machine-sized integer"))
    }

    pub fn u32_param(&self, name: &str) -> Result<u32, EngineError> {
        let value = self.integer(name)?;
        value
            .to_u32()
            .ok_or_else(|| Self::expected(name, "a non-negative 32-bit integer"))
    }

    pub fn u64_param(&self, name: &str) -> Result<u64, EngineError> {
        let value = self.integer(name)?;
        value
            .to_u64()
            .ok_or_else(|| Self::expected(name, "a non-negative 64-bit integer"))
    }

    /// Convert a numeric argument to f64. This is an explicit conversion and is
    /// recorded by callers when it matters; it never happens for exact results.
    pub fn f64_param(&self, name: &str) -> Result<f64, EngineError> {
        let value = self.number(name)?;
        value
            .to_f64()
            .ok_or_else(|| Self::expected(name, "a value representable as float64"))
    }

    pub fn optional_f64(&self, name: &str) -> Result<Option<f64>, EngineError> {
        match self.optional_number(name)? {
            None => Ok(None),
            Some(value) => value
                .to_f64()
                .map(Some)
                .ok_or_else(|| Self::expected(name, "a value representable as float64")),
        }
    }

    pub fn decimal(&self, name: &str) -> Result<Decimal, EngineError> {
        match self.number(name)? {
            Number::Decimal(value) => Ok(value.clone()),
            Number::Integer(value) => Ok(Decimal::from_bigint(value.clone())),
            other => Err(Self::expected(
                name,
                &format!("a decimal, found {}", other.kind_name()),
            )),
        }
    }

    pub fn optional_decimal(&self, name: &str) -> Result<Option<Decimal>, EngineError> {
        match self.optional_number(name)? {
            None => Ok(None),
            Some(number) => match number {
                Number::Decimal(value) => Ok(Some(value.clone())),
                Number::Integer(value) => Ok(Some(Decimal::from_bigint(value.clone()))),
                other => Err(Self::expected(
                    name,
                    &format!("a decimal, found {}", other.kind_name()),
                )),
            },
        }
    }
}

/// The callable interface implemented by every function in every module.
pub trait Function: Send + Sync {
    fn descriptor(&self) -> &FunctionDescriptor;
    fn invoke(&self, args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError>;
}

/// A function implemented by a plain function pointer plus its descriptor.
///
/// This is the explicit registration mechanism used by the numerical modules:
/// no macro framework, no hidden behaviour. The descriptor is the
/// authoritative schema; the engine validates arguments against it before
/// [`Function::invoke`] is called.
pub struct SimpleFunction {
    descriptor: FunctionDescriptor,
    invoke: fn(&Args, &ExecContext) -> Result<Outcome, EngineError>,
}

impl SimpleFunction {
    pub fn new(
        descriptor: FunctionDescriptor,
        invoke: fn(&Args, &ExecContext) -> Result<Outcome, EngineError>,
    ) -> SimpleFunction {
        SimpleFunction { descriptor, invoke }
    }

    pub fn arc(
        descriptor: FunctionDescriptor,
        invoke: fn(&Args, &ExecContext) -> Result<Outcome, EngineError>,
    ) -> Arc<dyn Function> {
        Arc::new(SimpleFunction::new(descriptor, invoke))
    }
}

impl Function for SimpleFunction {
    fn descriptor(&self) -> &FunctionDescriptor {
        &self.descriptor
    }

    fn invoke(&self, args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
        (self.invoke)(args, ctx)
    }
}

/// Helper to require that the context mode is supported by a function.
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
