//! Shared helpers for geometry descriptors, schemas, and example values.

use std::collections::BTreeMap;

use bicmath_core::contract::{Example, FunctionDescriptor};
use bicmath_core::number::NumericMode;
use bicmath_core::schema::{FieldSchema, NumberKind, ValueSchema};
use bicmath_core::value::Value;

pub(crate) fn number_schema() -> ValueSchema {
    ValueSchema::number(NumberKind::Any)
}

pub(crate) fn all_modes() -> Vec<NumericMode> {
    vec![
        NumericMode::Exact,
        NumericMode::Auto,
        NumericMode::Scientific,
    ]
}

pub(crate) fn approximate_modes() -> Vec<NumericMode> {
    vec![NumericMode::Auto, NumericMode::Scientific]
}

/// A descriptor for a geometry function with the module and version preset.
pub(crate) fn descriptor(id: &str, title: &str, summary: &str, method: &str) -> FunctionDescriptor {
    FunctionDescriptor::new(id, "geometry", "1.0.0", title, summary)
        .with_method_ref(format!("docs/methods/geometry.md#{method}"))
}

/// A fixed-length array of numbers.
pub(crate) fn vector_schema(len: usize) -> ValueSchema {
    ValueSchema::Array {
        items: Box::new(number_schema()),
        min_len: Some(len),
        max_len: Some(len),
    }
}

/// A polygon vertex list: at least three `[x, y]` points.
pub(crate) fn vertices_schema() -> ValueSchema {
    ValueSchema::Array {
        items: Box::new(vector_schema(2)),
        min_len: Some(3),
        max_len: None,
    }
}

pub(crate) fn field(name: &str, schema: ValueSchema) -> FieldSchema {
    FieldSchema::required(name, schema)
}

pub(crate) fn optional_field(name: &str, schema: ValueSchema) -> FieldSchema {
    FieldSchema::optional(name, schema)
}

pub(crate) fn record_schema(fields: Vec<FieldSchema>) -> ValueSchema {
    ValueSchema::Record {
        fields,
        allow_extra: false,
    }
}

pub(crate) fn enum_schema(variants: &[&str]) -> ValueSchema {
    ValueSchema::Enum {
        variants: variants.iter().map(|value| (*value).to_string()).collect(),
    }
}

pub(crate) fn example_args(pairs: &[(&str, serde_json::Value)]) -> BTreeMap<String, Value> {
    pairs
        .iter()
        .map(|(name, raw)| (name.to_string(), parse_value(raw.clone())))
        .collect()
}

pub(crate) fn parse_value(raw: serde_json::Value) -> Value {
    serde_json::from_value(raw).expect("example value must be valid")
}

/// Convenience constructor for an executable value example.
pub(crate) fn value_example(
    title: &str,
    pairs: &[(&str, serde_json::Value)],
    expected: serde_json::Value,
) -> Example {
    Example::new(title, example_args(pairs)).with_value(parse_value(expected))
}
