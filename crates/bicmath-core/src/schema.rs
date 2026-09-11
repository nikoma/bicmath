//! Typed parameter and output schemas with shared validation semantics.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::{EngineError, ErrorCode};
use crate::limits::Limits;
use crate::number::Number;
use crate::value::{Bound, Dimension, RESERVED_KIND_KEY, Value};

/// Accepted numeric kinds for a parameter.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NumberKind {
    Integer,
    Rational,
    Decimal,
    Float64,
    /// Integer, rational, or decimal, but not float64.
    Exact,
    /// Any numeric representation, including float64.
    Any,
}

impl NumberKind {
    pub fn accepts(&self, number: &Number) -> bool {
        match self {
            NumberKind::Integer => matches!(number, Number::Integer(_)),
            NumberKind::Rational => matches!(number, Number::Rational(_)),
            NumberKind::Decimal => matches!(number, Number::Decimal(_)),
            NumberKind::Float64 => matches!(number, Number::Float64(_)),
            NumberKind::Exact => !matches!(number, Number::Float64(_)),
            NumberKind::Any => true,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            NumberKind::Integer => "integer",
            NumberKind::Rational => "rational",
            NumberKind::Decimal => "decimal",
            NumberKind::Float64 => "float64",
            NumberKind::Exact => "exact number (integer, rational, or decimal)",
            NumberKind::Any => "number",
        }
    }
}

/// A named field in a record schema.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FieldSchema {
    pub name: String,
    pub description: String,
    pub schema: ValueSchema,
    #[serde(default)]
    pub required: bool,
}

impl FieldSchema {
    pub fn required(name: impl Into<String>, schema: ValueSchema) -> FieldSchema {
        FieldSchema {
            name: name.into(),
            description: String::new(),
            schema,
            required: true,
        }
    }

    pub fn optional(name: impl Into<String>, schema: ValueSchema) -> FieldSchema {
        FieldSchema {
            name: name.into(),
            description: String::new(),
            schema,
            required: false,
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> FieldSchema {
        self.description = description.into();
        self
    }
}

/// The schema of a single value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ValueSchema {
    Number {
        kind: NumberKind,
    },
    Bool,
    Text {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_len: Option<usize>,
    },
    Enum {
        variants: Vec<String>,
    },
    Array {
        items: Box<ValueSchema>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min_len: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_len: Option<usize>,
    },
    Record {
        fields: Vec<FieldSchema>,
        #[serde(default)]
        allow_extra: bool,
    },
    Quantity {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dimension: Option<Dimension>,
        /// Whether a temperature-difference style quantity is accepted.
        #[serde(default)]
        allow_delta: bool,
    },
    Money,
    Matrix {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_elems: Option<usize>,
        #[serde(default)]
        square: bool,
    },
    /// A restricted expression string, parsed by the engine before invocation.
    Expression {
        variables: Vec<String>,
    },
    Bound,
    Any,
}

impl ValueSchema {
    pub fn number(kind: NumberKind) -> ValueSchema {
        ValueSchema::Number { kind }
    }

    pub fn exact() -> ValueSchema {
        ValueSchema::Number {
            kind: NumberKind::Exact,
        }
    }

    pub fn float64() -> ValueSchema {
        ValueSchema::Number {
            kind: NumberKind::Float64,
        }
    }

    pub fn text() -> ValueSchema {
        ValueSchema::Text { max_len: None }
    }

    pub fn array(items: ValueSchema) -> ValueSchema {
        ValueSchema::Array {
            items: Box::new(items),
            min_len: None,
            max_len: None,
        }
    }

    pub fn array_with_len(
        items: ValueSchema,
        min_len: usize,
        max_len: Option<usize>,
    ) -> ValueSchema {
        ValueSchema::Array {
            items: Box::new(items),
            min_len: Some(min_len),
            max_len,
        }
    }

    /// Validate an already-typed value.
    pub fn validate(&self, value: &Value, path: &str, limits: &Limits) -> Result<(), EngineError> {
        value.check_limits(limits, 0)?;
        self.validate_inner(value, path, limits)
    }

    fn validate_inner(
        &self,
        value: &Value,
        path: &str,
        limits: &Limits,
    ) -> Result<(), EngineError> {
        let at = |error: EngineError| {
            if error.path.is_none() {
                error.with_path(path.to_string())
            } else {
                error
            }
        };
        match self {
            ValueSchema::Number { kind } => match value {
                Value::Number(number) if kind.accepts(number) => Ok(()),
                Value::Number(number) => Err(at(EngineError::malformed(format!(
                    "expected {}, found {}",
                    kind.label(),
                    number.kind_name()
                )))),
                other => Err(at(EngineError::malformed(format!(
                    "expected {}, found {}",
                    kind.label(),
                    other.kind_name()
                )))),
            },
            ValueSchema::Bool => match value {
                Value::Bool(_) => Ok(()),
                other => Err(at(EngineError::malformed(format!(
                    "expected a boolean, found {}",
                    other.kind_name()
                )))),
            },
            ValueSchema::Text { max_len } => match value {
                Value::Text(text) => {
                    if let Some(max_len) = max_len
                        && text.chars().count() > *max_len
                    {
                        return Err(at(EngineError::new(
                            ErrorCode::ResourceLimit,
                            format!(
                                "text length {} exceeds the limit of {max_len}",
                                text.chars().count()
                            ),
                        )));
                    }
                    Ok(())
                }
                other => Err(at(EngineError::malformed(format!(
                    "expected text, found {}",
                    other.kind_name()
                )))),
            },
            ValueSchema::Enum { variants } => match value {
                Value::Text(text) if variants.iter().any(|v| v == text) => Ok(()),
                Value::Text(text) => Err(at(EngineError::domain(format!(
                    "unknown variant {text:?}; expected one of {variants:?}"
                )))),
                other => Err(at(EngineError::malformed(format!(
                    "expected one of {variants:?}, found {}",
                    other.kind_name()
                )))),
            },
            ValueSchema::Array {
                items,
                min_len,
                max_len,
            } => match value {
                Value::Array(values) => {
                    if let Some(min_len) = min_len
                        && values.len() < *min_len
                    {
                        return Err(at(EngineError::new(
                            ErrorCode::InsufficientObservations,
                            format!(
                                "array has {} elements; at least {min_len} are required",
                                values.len()
                            ),
                        )));
                    }
                    let max = max_len.unwrap_or(limits.max_array_len);
                    if values.len() > max {
                        return Err(at(EngineError::new(
                            ErrorCode::ResourceLimit,
                            format!("array length {} exceeds the limit of {max}", values.len()),
                        )));
                    }
                    for (index, item) in values.iter().enumerate() {
                        items.validate_inner(item, &format!("{path}[{index}]"), limits)?;
                    }
                    Ok(())
                }
                other => Err(at(EngineError::malformed(format!(
                    "expected an array, found {}",
                    other.kind_name()
                )))),
            },
            ValueSchema::Record {
                fields,
                allow_extra,
            } => match value {
                Value::Record(record) => {
                    for field in fields {
                        match record.get(&field.name) {
                            Some(value) => {
                                field.schema.validate_inner(
                                    value,
                                    &format!("{path}.{}", field.name),
                                    limits,
                                )?;
                            }
                            None if field.required => {
                                return Err(at(EngineError::malformed(format!(
                                    "missing required field {:?}",
                                    field.name
                                ))));
                            }
                            None => {}
                        }
                    }
                    if !allow_extra {
                        for key in record.keys() {
                            if !fields.iter().any(|f| &f.name == key) {
                                return Err(at(EngineError::malformed(format!(
                                    "unknown field {key:?}"
                                ))));
                            }
                        }
                    }
                    Ok(())
                }
                other => Err(at(EngineError::malformed(format!(
                    "expected a record, found {}",
                    other.kind_name()
                )))),
            },
            ValueSchema::Quantity {
                dimension,
                allow_delta: _,
            } => match value {
                Value::Quantity {
                    value,
                    dimension: actual,
                } => {
                    if let Some(expected) = dimension
                        && expected != actual
                    {
                        return Err(at(EngineError::new(
                            ErrorCode::IncompatibleUnits,
                            format!("expected dimension {expected}, found {actual}"),
                        )));
                    }
                    value.check_limits(limits, 0)?;
                    Ok(())
                }
                other => Err(at(EngineError::malformed(format!(
                    "expected a quantity, found {}",
                    other.kind_name()
                )))),
            },
            ValueSchema::Money => match value {
                Value::Money { .. } => Ok(()),
                other => Err(at(EngineError::malformed(format!(
                    "expected money, found {}",
                    other.kind_name()
                )))),
            },
            ValueSchema::Matrix { max_elems, square } => match value {
                Value::Matrix { rows, cols, .. } => {
                    if *square && rows != cols {
                        return Err(at(EngineError::malformed(format!(
                            "expected a square matrix, found {rows}x{cols}"
                        ))));
                    }
                    let max = max_elems.unwrap_or(limits.max_matrix_elements);
                    if (*rows as usize).saturating_mul(*cols as usize) > max {
                        return Err(at(EngineError::new(
                            ErrorCode::ResourceLimit,
                            format!("matrix {rows}x{cols} exceeds the element limit of {max}"),
                        )));
                    }
                    Ok(())
                }
                other => Err(at(EngineError::malformed(format!(
                    "expected a matrix, found {}",
                    other.kind_name()
                )))),
            },
            ValueSchema::Expression { .. } => match value {
                Value::Text(_) => Ok(()),
                other => Err(at(EngineError::malformed(format!(
                    "expected an expression string, found {}",
                    other.kind_name()
                )))),
            },
            ValueSchema::Bound => match value {
                Value::Bound(_) => Ok(()),
                other => Err(at(EngineError::malformed(format!(
                    "expected a bound, found {}",
                    other.kind_name()
                )))),
            },
            ValueSchema::Any => Ok(()),
        }
    }

    /// Coerce raw JSON into a typed value, applying the documented shorthand
    /// rules: JSON integers/decimals are exact, and when `numeric_shorthand` is
    /// true a string is parsed as a numeric literal for numeric schemas.
    pub fn coerce(
        &self,
        raw: &serde_json::Value,
        path: &str,
        limits: &Limits,
        numeric_shorthand: bool,
    ) -> Result<Value, EngineError> {
        match (self, raw) {
            (ValueSchema::Number { kind }, serde_json::Value::String(text)) => {
                if !numeric_shorthand {
                    return Err(EngineError::malformed(
                        "string shorthand is not allowed for this parameter",
                    )
                    .with_path(path.to_string()));
                }
                let number = Number::parse_literal(text, limits)
                    .map_err(|e| e.with_path(path.to_string()))?;
                if !kind.accepts(&number) {
                    return Err(EngineError::malformed(format!(
                        "expected {}, parsed shorthand is {}",
                        kind.label(),
                        number.kind_name()
                    ))
                    .with_path(path.to_string()));
                }
                Ok(Value::Number(number))
            }
            (ValueSchema::Array { items, .. }, serde_json::Value::Array(raw_items)) => {
                let mut out = Vec::with_capacity(raw_items.len());
                for (index, item) in raw_items.iter().enumerate() {
                    out.push(items.coerce(
                        item,
                        &format!("{path}[{index}]"),
                        limits,
                        numeric_shorthand,
                    )?);
                }
                let value = Value::Array(out);
                self.validate(&value, path, limits)?;
                Ok(value)
            }
            (
                ValueSchema::Record {
                    fields,
                    allow_extra,
                },
                serde_json::Value::Object(object),
            ) => {
                let mut out = BTreeMap::new();
                for (key, raw_value) in object {
                    if key == RESERVED_KIND_KEY {
                        return Err(EngineError::malformed(
                            "record key \"kind\" is reserved for tagged values",
                        )
                        .with_path(path.to_string()));
                    }
                    let field_schema = fields
                        .iter()
                        .find(|f| &f.name == key)
                        .map(|f| &f.schema)
                        .or(if *allow_extra {
                            Some(&ValueSchema::Any)
                        } else {
                            None
                        });
                    let Some(field_schema) = field_schema else {
                        return Err(EngineError::malformed(format!("unknown field {key:?}"))
                            .with_path(path.to_string()));
                    };
                    out.insert(
                        key.clone(),
                        field_schema.coerce(
                            raw_value,
                            &format!("{path}.{key}"),
                            limits,
                            numeric_shorthand,
                        )?,
                    );
                }
                let value = Value::Record(out);
                self.validate(&value, path, limits)?;
                Ok(value)
            }
            _ => {
                let value =
                    Value::from_json(raw, limits).map_err(|e| e.with_path(path.to_string()))?;
                self.validate(&value, path, limits)?;
                Ok(value)
            }
        }
    }

    /// True when this schema accepts exact numeric payloads and therefore may
    /// receive string shorthand in expressions/requests.
    pub fn is_numeric(&self) -> bool {
        matches!(self, ValueSchema::Number { .. })
    }

    /// A short human-readable description, used in generated documentation.
    pub fn summary(&self) -> String {
        match self {
            ValueSchema::Number { kind } => kind.label().to_string(),
            ValueSchema::Bool => "boolean".to_string(),
            ValueSchema::Text { .. } => "text".to_string(),
            ValueSchema::Enum { variants } => format!("one of {variants:?}"),
            ValueSchema::Array { items, .. } => format!("array of {}", items.summary()),
            ValueSchema::Record { fields, .. } => {
                let names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
                format!("record with fields {}", names.join(", "))
            }
            ValueSchema::Quantity { dimension, .. } => match dimension {
                Some(dimension) => format!("quantity [{dimension}]"),
                None => "quantity".to_string(),
            },
            ValueSchema::Money => "money".to_string(),
            ValueSchema::Matrix { .. } => "matrix".to_string(),
            ValueSchema::Expression { variables } => {
                format!("restricted expression in {}", variables.join(", "))
            }
            ValueSchema::Bound => "bound".to_string(),
            ValueSchema::Any => "any value".to_string(),
        }
    }

    /// Convert to a JSON-schema-like description used in MCP tool definitions.
    pub fn to_json_schema(&self) -> serde_json::Value {
        use serde_json::json;
        match self {
            ValueSchema::Number { kind } => json!({
                "description": kind.label(),
                "oneOf": [
                    {"type": "object", "properties": {"kind": {"const": "integer"}, "value": {"type": "string"}}, "required": ["kind", "value"]},
                    {"type": "object", "properties": {"kind": {"const": "rational"}, "numerator": {"type": "string"}, "denominator": {"type": "string"}}, "required": ["kind", "numerator", "denominator"]},
                    {"type": "object", "properties": {"kind": {"const": "decimal"}, "value": {"type": "string"}}, "required": ["kind", "value"]},
                    {"type": "object", "properties": {"kind": {"const": "float64"}, "value": {"type": "string"}}, "required": ["kind", "value"]},
                    {"type": "string", "description": "numeric shorthand string (for declared numeric parameters)"}
                ]
            }),
            ValueSchema::Bool => json!({"type": "boolean"}),
            ValueSchema::Text { .. } => json!({"type": "string"}),
            ValueSchema::Enum { variants } => json!({"type": "string", "enum": variants}),
            ValueSchema::Array {
                items,
                min_len,
                max_len,
            } => {
                let mut schema = json!({"type": "array", "items": items.to_json_schema()});
                if let Some(min_len) = min_len {
                    schema["minItems"] = json!(min_len);
                }
                if let Some(max_len) = max_len {
                    schema["maxItems"] = json!(max_len);
                }
                schema
            }
            ValueSchema::Record {
                fields,
                allow_extra,
            } => {
                let mut properties = serde_json::Map::new();
                let mut required = Vec::new();
                for field in fields {
                    let mut schema = field.schema.to_json_schema();
                    if !field.description.is_empty() {
                        schema["description"] = json!(field.description);
                    }
                    properties.insert(field.name.clone(), schema);
                    if field.required {
                        required.push(serde_json::Value::String(field.name.clone()));
                    }
                }
                json!({
                    "type": "object",
                    "properties": properties,
                    "required": required,
                    "additionalProperties": *allow_extra,
                })
            }
            ValueSchema::Quantity { .. } => json!({
                "type": "object",
                "description": "quantity: {kind: quantity, value: number, dimension: {...}}",
            }),
            ValueSchema::Money => json!({
                "type": "object",
                "description": "money: {kind: money, amount: number, currency: \"USD\"}",
            }),
            ValueSchema::Matrix { .. } => json!({
                "type": "object",
                "description": "matrix: {kind: matrix, rows: n, cols: m, data: [numbers]}",
            }),
            ValueSchema::Expression { variables } => json!({
                "type": "string",
                "description": format!("restricted expression using variables {}", variables.join(", ")),
            }),
            ValueSchema::Bound => json!({
                "type": "object",
                "description": "bound: {kind: bound, unbounded: true} or {kind: bound, value: number}",
            }),
            ValueSchema::Any => json!({}),
        }
    }
}

/// Convenience: an exact-number array schema.
pub fn exact_number_array() -> ValueSchema {
    ValueSchema::array(ValueSchema::exact())
}

/// Convenience: a finite float64 array schema.
pub fn float64_array() -> ValueSchema {
    ValueSchema::array(ValueSchema::float64())
}

/// Convenience: a positive integer schema (validated by the function).
pub fn integer() -> ValueSchema {
    ValueSchema::number(NumberKind::Integer)
}

/// Convenience: a finite scalar schema.
pub fn finite_scalar() -> ValueSchema {
    ValueSchema::number(NumberKind::Any)
}

/// Convenience: an unbounded finite bound value.
pub fn bound() -> ValueSchema {
    ValueSchema::Bound
}

/// Extract a required dimension from a value, for unit-aware functions.
pub fn expect_dimension(value: &Value, expected: Dimension) -> Result<(), EngineError> {
    match value {
        Value::Quantity { dimension, .. } if *dimension == expected => Ok(()),
        Value::Quantity { dimension, .. } => Err(EngineError::new(
            ErrorCode::IncompatibleUnits,
            format!("expected dimension {expected}, found {dimension}"),
        )),
        other => Err(EngineError::malformed(format!(
            "expected a quantity, found {}",
            other.kind_name()
        ))),
    }
}

/// Extract a finite bound as a concrete endpoint, or `None` when unbounded.
pub fn bound_endpoint(value: &Value) -> Result<Option<&Number>, EngineError> {
    match value {
        Value::Bound(Bound::Unbounded) => Ok(None),
        Value::Bound(Bound::Finite(number)) => Ok(Some(number)),
        other => Err(EngineError::malformed(format!(
            "expected a bound, found {}",
            other.kind_name()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> Limits {
        Limits::conservative()
    }

    #[test]
    fn shorthand_is_only_allowed_when_declared() {
        let schema = ValueSchema::exact();
        let coerced = schema
            .coerce(&serde_json::json!("0.10"), "x", &limits(), true)
            .unwrap();
        assert_eq!(
            serde_json::to_value(&coerced).unwrap(),
            serde_json::json!({"kind": "decimal", "value": "0.10"})
        );
        assert!(
            schema
                .coerce(&serde_json::json!("0.10"), "x", &limits(), false)
                .is_err()
        );
    }

    #[test]
    fn float_is_rejected_for_exact_schema() {
        let schema = ValueSchema::exact();
        let raw = serde_json::json!({"kind": "float64", "value": "0.1"});
        let err = schema.coerce(&raw, "x", &limits(), true).unwrap_err();
        assert_eq!(err.code, ErrorCode::MalformedInput);
    }

    #[test]
    fn unknown_record_fields_are_rejected() {
        let schema = ValueSchema::Record {
            fields: vec![FieldSchema::required("a", ValueSchema::exact())],
            allow_extra: false,
        };
        let err = schema
            .coerce(&serde_json::json!({"a": 1, "b": 2}), "r", &limits(), true)
            .unwrap_err();
        assert!(err.message.contains("unknown field"));
    }
}
