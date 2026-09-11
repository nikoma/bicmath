//! The recursive wire value model.
//!
//! Canonical scalar encoding (used everywhere, including nested arrays and
//! records):
//!
//! ```json
//! {"kind":"integer","value":"9007199254740993"}
//! {"kind":"decimal","value":"0.10"}
//! {"kind":"rational","numerator":"1","denominator":"3"}
//! {"kind":"float64","value":"0.3333333333333333"}
//! ```
//!
//! Structured values:
//!
//! ```json
//! {"kind":"quantity","value":{"kind":"decimal","value":"9.81"},"dimension":{"length":1,"time":-2}}
//! {"kind":"money","amount":{"kind":"decimal","value":"10.00"},"currency":"USD"}
//! {"kind":"matrix","rows":2,"cols":2,"data":[...]}
//! {"kind":"bound","unbounded":true}
//! ```
//!
//! Plain JSON objects are records. To keep the encoding unambiguous, record
//! keys must not be the reserved key `"kind"`; validation rejects such records.
//! Plain JSON integers and decimal numbers are interpreted as exact integers and
//! decimals. `float64` is only produced by an explicit canonical value.

use std::collections::BTreeMap;
use std::fmt;

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::Zero;
use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::{EngineError, ErrorCode};
use crate::limits::Limits;
use crate::number::{Decimal, Float64, Number};

/// SI-style dimension vector. Angle is tracked separately from the seven SI
/// base dimensions so trigonometric functions can require angle/dimensionless
/// inputs explicitly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Dimension {
    /// length, mass, time, electric current, thermodynamic temperature,
    /// amount of substance, luminous intensity, plane angle.
    exponents: [i8; 8],
}

pub const DIM_LENGTH: usize = 0;
pub const DIM_MASS: usize = 1;
pub const DIM_TIME: usize = 2;
pub const DIM_CURRENT: usize = 3;
pub const DIM_TEMPERATURE: usize = 4;
pub const DIM_AMOUNT: usize = 5;
pub const DIM_LUMINOUS: usize = 6;
pub const DIM_ANGLE: usize = 7;

pub const DIMENSION_NAMES: [&str; 8] = [
    "length",
    "mass",
    "time",
    "current",
    "temperature",
    "amount",
    "luminous",
    "angle",
];

impl Dimension {
    pub const DIMENSIONLESS: Dimension = Dimension { exponents: [0; 8] };
    pub const ANGLE: Dimension = Dimension {
        exponents: [0, 0, 0, 0, 0, 0, 0, 1],
    };

    pub fn new(exponents: [i8; 8]) -> Dimension {
        Dimension { exponents }
    }

    pub fn exponents(&self) -> [i8; 8] {
        self.exponents
    }

    pub fn get(&self, index: usize) -> i8 {
        self.exponents[index]
    }

    pub fn is_dimensionless(&self) -> bool {
        self.exponents.iter().all(|e| *e == 0)
    }

    pub fn is_angle(&self) -> bool {
        self.exponents[..7].iter().all(|e| *e == 0) && self.exponents[DIM_ANGLE] != 0
    }

    pub fn is_angle_only(&self) -> bool {
        self.exponents[..7].iter().all(|e| *e == 0) && self.exponents[DIM_ANGLE] == 1
    }

    pub fn multiply(&self, other: &Dimension) -> Result<Dimension, EngineError> {
        let mut out = [0i8; 8];
        for (slot, (left, right)) in out
            .iter_mut()
            .zip(self.exponents.iter().zip(other.exponents.iter()))
        {
            let value = *left as i32 + *right as i32;
            if value > i8::MAX as i32 || value < i8::MIN as i32 {
                return Err(EngineError::new(
                    ErrorCode::DomainViolation,
                    "dimension exponent overflow",
                ));
            }
            *slot = value as i8;
        }
        Ok(Dimension { exponents: out })
    }

    pub fn divide(&self, other: &Dimension) -> Result<Dimension, EngineError> {
        let mut out = [0i8; 8];
        for (slot, (left, right)) in out
            .iter_mut()
            .zip(self.exponents.iter().zip(other.exponents.iter()))
        {
            let value = *left as i32 - *right as i32;
            if value > i8::MAX as i32 || value < i8::MIN as i32 {
                return Err(EngineError::new(
                    ErrorCode::DomainViolation,
                    "dimension exponent overflow",
                ));
            }
            *slot = value as i8;
        }
        Ok(Dimension { exponents: out })
    }

    pub fn pow(&self, exponent: i32) -> Result<Dimension, EngineError> {
        let mut out = [0i8; 8];
        for (slot, source) in out.iter_mut().zip(self.exponents.iter()) {
            let value = *source as i32 * exponent;
            if value > i8::MAX as i32 || value < i8::MIN as i32 {
                return Err(EngineError::new(
                    ErrorCode::DomainViolation,
                    "dimension exponent overflow",
                ));
            }
            *slot = value as i8;
        }
        Ok(Dimension { exponents: out })
    }

    pub fn to_json(&self) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        for (i, name) in DIMENSION_NAMES.iter().enumerate() {
            if self.exponents[i] != 0 {
                map.insert(
                    (*name).to_string(),
                    serde_json::Value::from(self.exponents[i]),
                );
            }
        }
        serde_json::Value::Object(map)
    }

    pub fn from_json(value: &serde_json::Value) -> Result<Dimension, EngineError> {
        let object = value
            .as_object()
            .ok_or_else(|| EngineError::malformed("dimension must be a JSON object"))?;
        let mut exponents = [0i8; 8];
        for (key, raw) in object {
            let index = DIMENSION_NAMES
                .iter()
                .position(|name| name == key)
                .ok_or_else(|| {
                    EngineError::malformed(format!("unknown dimension component {key:?}"))
                })?;
            let value = raw.as_i64().ok_or_else(|| {
                EngineError::malformed(format!("dimension component {key:?} must be an integer"))
            })?;
            if !(-128..=127).contains(&value) {
                return Err(EngineError::malformed(format!(
                    "dimension component {key:?} out of range"
                )));
            }
            exponents[index] = value as i8;
        }
        Ok(Dimension { exponents })
    }

    /// Human-readable dimension string such as `m*s^-2`.
    pub fn symbol(&self) -> String {
        const SYMBOLS: [&str; 8] = ["m", "kg", "s", "A", "K", "mol", "cd", "rad"];
        if self.is_dimensionless() {
            return "1".to_string();
        }
        let mut parts = Vec::new();
        for (i, symbol) in SYMBOLS.iter().enumerate() {
            match self.exponents[i] {
                0 => {}
                1 => parts.push((*symbol).to_string()),
                e => parts.push(format!("{symbol}^{e}")),
            }
        }
        parts.join("*")
    }
}

impl Serialize for Dimension {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.to_json().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Dimension {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = serde_json::Value::deserialize(deserializer)?;
        Dimension::from_json(&raw).map_err(D::Error::custom)
    }
}

impl fmt::Display for Dimension {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.symbol())
    }
}

/// A tagged confidence-bound endpoint. A mathematically unbounded endpoint is
/// represented as [`Bound::Unbounded`], never as a float infinity.
#[derive(Clone, Debug, PartialEq)]
pub enum Bound {
    Unbounded,
    Finite(Number),
}

/// The recursive wire value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Text(String),
    Number(Number),
    Array(Vec<Value>),
    Record(BTreeMap<String, Value>),
    Quantity {
        value: Box<Value>,
        dimension: Dimension,
    },
    Money {
        amount: Box<Value>,
        currency: String,
    },
    Matrix {
        rows: u32,
        cols: u32,
        data: Vec<Value>,
    },
    Bound(Bound),
}

/// Reserved key that distinguishes tagged values from records.
pub const RESERVED_KIND_KEY: &str = "kind";

impl Value {
    pub fn number(number: Number) -> Value {
        Value::Number(number)
    }

    pub fn decimal(decimal: Decimal) -> Value {
        Value::Number(Number::Decimal(decimal))
    }

    pub fn integer(value: BigInt) -> Value {
        Value::Number(Number::Integer(value))
    }

    pub fn text(value: impl Into<String>) -> Value {
        Value::Text(value.into())
    }

    pub fn record(entries: impl IntoIterator<Item = (impl Into<String>, Value)>) -> Value {
        Value::Record(entries.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }

    pub fn kind_name(&self) -> &'static str {
        match self {
            Value::Null => "null",
            Value::Bool(_) => "boolean",
            Value::Text(_) => "text",
            Value::Number(n) => n.kind_name(),
            Value::Array(_) => "array",
            Value::Record(_) => "record",
            Value::Quantity { .. } => "quantity",
            Value::Money { .. } => "money",
            Value::Matrix { .. } => "matrix",
            Value::Bound(_) => "bound",
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    pub fn as_number(&self) -> Result<&Number, EngineError> {
        match self {
            Value::Number(n) => Ok(n),
            other => Err(EngineError::malformed(format!(
                "expected a number, found {}",
                other.kind_name()
            ))),
        }
    }

    pub fn as_array(&self) -> Result<&[Value], EngineError> {
        match self {
            Value::Array(items) => Ok(items),
            other => Err(EngineError::malformed(format!(
                "expected an array, found {}",
                other.kind_name()
            ))),
        }
    }

    pub fn as_record(&self) -> Result<&BTreeMap<String, Value>, EngineError> {
        match self {
            Value::Record(fields) => Ok(fields),
            other => Err(EngineError::malformed(format!(
                "expected a record, found {}",
                other.kind_name()
            ))),
        }
    }

    pub fn as_text(&self) -> Result<&str, EngineError> {
        match self {
            Value::Text(text) => Ok(text),
            other => Err(EngineError::malformed(format!(
                "expected text, found {}",
                other.kind_name()
            ))),
        }
    }

    pub fn as_bool(&self) -> Result<bool, EngineError> {
        match self {
            Value::Bool(value) => Ok(*value),
            other => Err(EngineError::malformed(format!(
                "expected a boolean, found {}",
                other.kind_name()
            ))),
        }
    }

    pub fn as_money(&self) -> Result<(&Number, &str), EngineError> {
        match self {
            Value::Money { amount, currency } => Ok((amount.as_number()?, currency)),
            other => Err(EngineError::malformed(format!(
                "expected money, found {}",
                other.kind_name()
            ))),
        }
    }

    pub fn as_quantity(&self) -> Result<(&Number, Dimension), EngineError> {
        match self {
            Value::Quantity { value, dimension } => Ok((value.as_number()?, *dimension)),
            other => Err(EngineError::malformed(format!(
                "expected a quantity, found {}",
                other.kind_name()
            ))),
        }
    }

    /// Check structural depth and basic size limits.
    pub fn check_limits(&self, limits: &Limits, depth: usize) -> Result<(), EngineError> {
        if depth > limits.max_recursion_depth {
            return Err(EngineError::new(
                ErrorCode::ResourceLimit,
                format!(
                    "value nesting depth exceeds the limit of {}",
                    limits.max_recursion_depth
                ),
            ));
        }
        match self {
            Value::Array(items) => {
                if items.len() > limits.max_array_len {
                    return Err(EngineError::new(
                        ErrorCode::ResourceLimit,
                        format!(
                            "array length {} exceeds the limit of {}",
                            items.len(),
                            limits.max_array_len
                        ),
                    ));
                }
                for item in items {
                    item.check_limits(limits, depth + 1)?;
                }
            }
            Value::Record(fields) => {
                for value in fields.values() {
                    value.check_limits(limits, depth + 1)?;
                }
            }
            Value::Quantity { value, .. } => value.check_limits(limits, depth + 1)?,
            Value::Money { amount, .. } => amount.check_limits(limits, depth + 1)?,
            Value::Matrix { rows, cols, data } => {
                if (*rows as usize).saturating_mul(*cols as usize) > limits.max_matrix_elements {
                    return Err(EngineError::new(
                        ErrorCode::ResourceLimit,
                        format!(
                            "matrix {}x{} exceeds the element limit of {}",
                            rows, cols, limits.max_matrix_elements
                        ),
                    ));
                }
                for item in data {
                    item.check_limits(limits, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Convert plain JSON into a typed value. JSON numbers are interpreted as
    /// exact integers or decimals; `float64` requires the canonical form.
    pub fn from_json(raw: &serde_json::Value, limits: &Limits) -> Result<Value, EngineError> {
        Value::from_json_at(raw, limits, 0)
    }

    fn from_json_at(
        raw: &serde_json::Value,
        limits: &Limits,
        depth: usize,
    ) -> Result<Value, EngineError> {
        if depth > limits.max_recursion_depth {
            return Err(EngineError::new(
                ErrorCode::ResourceLimit,
                format!(
                    "value nesting depth exceeds the limit of {}",
                    limits.max_recursion_depth
                ),
            ));
        }
        match raw {
            serde_json::Value::Null => Ok(Value::Null),
            serde_json::Value::Bool(value) => Ok(Value::Bool(*value)),
            serde_json::Value::String(text) => {
                if text.len() > limits.max_string_len {
                    return Err(EngineError::new(
                        ErrorCode::ResourceLimit,
                        format!(
                            "string length {} exceeds the limit of {}",
                            text.len(),
                            limits.max_string_len
                        ),
                    ));
                }
                Ok(Value::Text(text.clone()))
            }
            serde_json::Value::Number(number) => {
                let text = number.to_string();
                if text.contains('.') || text.contains('e') || text.contains('E') {
                    Ok(Value::Number(Number::Decimal(Decimal::parse(
                        &text, limits,
                    )?)))
                } else {
                    let value = text.parse::<BigInt>().map_err(|_| {
                        EngineError::malformed(format!("invalid JSON integer {text:?}"))
                    })?;
                    if value.bits() > limits.max_integer_bits as u64 {
                        return Err(EngineError::new(
                            ErrorCode::ResourceLimit,
                            format!("integer needs {} bits, exceeding the limit", value.bits()),
                        ));
                    }
                    Ok(Value::Number(Number::Integer(value)))
                }
            }
            serde_json::Value::Array(items) => {
                if items.len() > limits.max_array_len {
                    return Err(EngineError::new(
                        ErrorCode::ResourceLimit,
                        format!(
                            "array length {} exceeds the limit of {}",
                            items.len(),
                            limits.max_array_len
                        ),
                    ));
                }
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    out.push(Value::from_json_at(item, limits, depth + 1)?);
                }
                Ok(Value::Array(out))
            }
            serde_json::Value::Object(object) => {
                if let Some(tagged) = parse_tagged(object, limits, depth)? {
                    return Ok(tagged);
                }
                let mut fields = BTreeMap::new();
                for (key, value) in object {
                    if key == RESERVED_KIND_KEY {
                        return Err(EngineError::malformed(
                            "record key \"kind\" is reserved for tagged values",
                        ));
                    }
                    fields.insert(key.clone(), Value::from_json_at(value, limits, depth + 1)?);
                }
                Ok(Value::Record(fields))
            }
        }
    }
}

fn parse_tagged(
    object: &serde_json::Map<String, serde_json::Value>,
    limits: &Limits,
    depth: usize,
) -> Result<Option<Value>, EngineError> {
    let Some(kind) = object.get(RESERVED_KIND_KEY).and_then(|v| v.as_str()) else {
        return Ok(None);
    };
    let number_from = |raw: &serde_json::Value| -> Result<Number, EngineError> {
        Value::from_json_at(raw, limits, depth + 1)?
            .as_number()
            .cloned()
    };
    match kind {
        "integer" => {
            let raw = object
                .get("value")
                .ok_or_else(|| EngineError::malformed("integer value is missing"))?;
            let text = match raw {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            if !text
                .strip_prefix(['+', '-'])
                .unwrap_or(&text)
                .bytes()
                .all(|b| b.is_ascii_digit())
                || text.is_empty()
                || text.strip_prefix(['+', '-']).unwrap_or(&text).is_empty()
            {
                return Err(EngineError::malformed(format!(
                    "invalid integer payload {text:?}"
                )));
            }
            let value = text
                .parse::<BigInt>()
                .map_err(|_| EngineError::malformed(format!("invalid integer payload {text:?}")))?;
            if value.bits() > limits.max_integer_bits as u64 {
                return Err(EngineError::new(
                    ErrorCode::ResourceLimit,
                    "integer payload exceeds the bit limit",
                ));
            }
            Ok(Some(Value::Number(Number::Integer(value))))
        }
        "decimal" => {
            let raw = object
                .get("value")
                .ok_or_else(|| EngineError::malformed("decimal value is missing"))?;
            let text = match raw {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            Ok(Some(Value::Number(Number::Decimal(Decimal::parse(
                &text, limits,
            )?))))
        }
        "rational" => {
            let numer = object
                .get("numerator")
                .and_then(|v| v.as_str())
                .ok_or_else(|| EngineError::malformed("rational numerator must be a string"))?;
            let denom = object
                .get("denominator")
                .and_then(|v| v.as_str())
                .ok_or_else(|| EngineError::malformed("rational denominator must be a string"))?;
            let numer = numer
                .parse::<BigInt>()
                .map_err(|_| EngineError::malformed("invalid rational numerator"))?;
            let denom = denom
                .parse::<BigInt>()
                .map_err(|_| EngineError::malformed("invalid rational denominator"))?;
            if denom.is_zero() {
                return Err(EngineError::division_by_zero(
                    "rational denominator must not be zero",
                ));
            }
            Ok(Some(Value::Number(Number::Rational(BigRational::new(
                numer, denom,
            )))))
        }
        "float64" => {
            let raw = object
                .get("value")
                .ok_or_else(|| EngineError::malformed("float64 value is missing"))?;
            let text = match raw {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            let value: f64 = text
                .parse()
                .map_err(|_| EngineError::malformed(format!("invalid float64 payload {text:?}")))?;
            Ok(Some(Value::Number(Number::Float64(Float64::new(value)?))))
        }
        "quantity" => {
            let raw = object
                .get("value")
                .ok_or_else(|| EngineError::malformed("quantity value is missing"))?;
            let value = Value::from_json_at(raw, limits, depth + 1)?;
            if !matches!(value, Value::Number(_)) {
                return Err(EngineError::malformed(
                    "quantity value must be a scalar number",
                ));
            }
            let dimension = match object.get("dimension") {
                Some(raw) => Dimension::from_json(raw)?,
                None => Dimension::DIMENSIONLESS,
            };
            Ok(Some(Value::Quantity {
                value: Box::new(value),
                dimension,
            }))
        }
        "money" => {
            let raw = object
                .get("amount")
                .ok_or_else(|| EngineError::malformed("money amount is missing"))?;
            let amount = number_from(raw)?;
            let currency = object
                .get("currency")
                .and_then(|v| v.as_str())
                .ok_or_else(|| EngineError::malformed("money currency is missing"))?
                .to_string();
            validate_currency(&currency)?;
            Ok(Some(Value::Money {
                amount: Box::new(Value::Number(amount)),
                currency,
            }))
        }
        "matrix" => {
            let rows = object
                .get("rows")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| EngineError::malformed("matrix rows is missing"))?;
            let cols = object
                .get("cols")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| EngineError::malformed("matrix cols is missing"))?;
            if rows == 0 || cols == 0 {
                return Err(EngineError::malformed("matrix dimensions must be positive"));
            }
            if (rows as usize).saturating_mul(cols as usize) > limits.max_matrix_elements {
                return Err(EngineError::new(
                    ErrorCode::ResourceLimit,
                    format!(
                        "matrix {rows}x{cols} exceeds the element limit of {}",
                        limits.max_matrix_elements
                    ),
                ));
            }
            let raw = object
                .get("data")
                .and_then(|v| v.as_array())
                .ok_or_else(|| EngineError::malformed("matrix data must be an array"))?;
            if raw.len() != (rows as usize) * (cols as usize) {
                return Err(EngineError::malformed(format!(
                    "matrix data length {} does not match {}x{}",
                    raw.len(),
                    rows,
                    cols
                )));
            }
            let mut data = Vec::with_capacity(raw.len());
            for item in raw {
                let value = Value::from_json_at(item, limits, depth + 1)?;
                if !matches!(value, Value::Number(_)) {
                    return Err(EngineError::malformed(
                        "matrix elements must be scalar numbers",
                    ));
                }
                data.push(value);
            }
            Ok(Some(Value::Matrix {
                rows: rows as u32,
                cols: cols as u32,
                data,
            }))
        }
        "bound" => {
            if object.get("unbounded").and_then(|v| v.as_bool()) == Some(true) {
                Ok(Some(Value::Bound(Bound::Unbounded)))
            } else if let Some(raw) = object.get("value") {
                Ok(Some(Value::Bound(Bound::Finite(number_from(raw)?))))
            } else {
                Err(EngineError::malformed(
                    "bound must carry either \"unbounded\": true or a finite value",
                ))
            }
        }
        other => Err(EngineError::malformed(format!(
            "unknown tagged value kind {other:?}"
        ))),
    }
}

/// Validate a currency identifier: 2-12 uppercase letters/digits, starting with
/// a letter. Nonstandard units may be used but must be explicit.
pub fn validate_currency(currency: &str) -> Result<(), EngineError> {
    let mut chars = currency.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() => {}
        _ => {
            return Err(EngineError::malformed(format!(
                "invalid currency identifier {currency:?}"
            )));
        }
    }
    if currency.len() < 2 || currency.len() > 12 {
        return Err(EngineError::malformed(format!(
            "invalid currency identifier {currency:?}: length must be 2-12"
        )));
    }
    if !currency
        .chars()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    {
        return Err(EngineError::malformed(format!(
            "invalid currency identifier {currency:?}: expected uppercase letters and digits"
        )));
    }
    Ok(())
}

impl Serialize for Value {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        match self {
            Value::Null => serializer.serialize_none(),
            Value::Bool(value) => serializer.serialize_bool(*value),
            Value::Text(text) => serializer.serialize_str(text),
            Value::Number(number) => number.serialize(serializer),
            Value::Array(items) => items.serialize(serializer),
            Value::Record(fields) => fields.serialize(serializer),
            Value::Quantity { value, dimension } => {
                let mut map = serializer.serialize_map(Some(3))?;
                map.serialize_entry("kind", "quantity")?;
                map.serialize_entry("value", value)?;
                map.serialize_entry("dimension", dimension)?;
                map.end()
            }
            Value::Money { amount, currency } => {
                let mut map = serializer.serialize_map(Some(3))?;
                map.serialize_entry("kind", "money")?;
                map.serialize_entry("amount", amount)?;
                map.serialize_entry("currency", currency)?;
                map.end()
            }
            Value::Matrix { rows, cols, data } => {
                let mut map = serializer.serialize_map(Some(4))?;
                map.serialize_entry("kind", "matrix")?;
                map.serialize_entry("rows", rows)?;
                map.serialize_entry("cols", cols)?;
                map.serialize_entry("data", data)?;
                map.end()
            }
            Value::Bound(bound) => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("kind", "bound")?;
                match bound {
                    Bound::Unbounded => map.serialize_entry("unbounded", &true)?,
                    Bound::Finite(number) => map.serialize_entry("value", number)?,
                }
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for Value {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = serde_json::Value::deserialize(deserializer)?;
        Value::from_json(&raw, &Limits::conservative()).map_err(D::Error::custom)
    }
}

impl Serialize for Number {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        match self {
            Number::Integer(value) => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("kind", "integer")?;
                map.serialize_entry("value", &value.to_string())?;
                map.end()
            }
            Number::Decimal(value) => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("kind", "decimal")?;
                map.serialize_entry("value", &value.to_plain_string())?;
                map.end()
            }
            Number::Rational(value) => {
                let mut map = serializer.serialize_map(Some(3))?;
                map.serialize_entry("kind", "rational")?;
                map.serialize_entry("numerator", &value.numer().to_string())?;
                map.serialize_entry("denominator", &value.denom().to_string())?;
                map.end()
            }
            Number::Float64(value) => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("kind", "float64")?;
                map.serialize_entry("value", &value.to_string())?;
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for Number {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        match value {
            Value::Number(number) => Ok(number),
            other => Err(D::Error::custom(format!(
                "expected a number, found {}",
                other.kind_name()
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalar_round_trip_preserves_exact_payloads() {
        let raw = serde_json::json!({"kind": "integer", "value": "9007199254740993"});
        let value: Value = serde_json::from_value(raw.clone()).unwrap();
        let encoded = serde_json::to_value(&value).unwrap();
        assert_eq!(encoded, raw);
    }

    #[test]
    fn decimal_scale_survives_round_trip() {
        let raw = serde_json::json!({"kind": "decimal", "value": "0.10"});
        let value: Value = serde_json::from_value(raw.clone()).unwrap();
        let encoded = serde_json::to_value(&value).unwrap();
        assert_eq!(encoded, raw);
    }

    #[test]
    fn plain_json_integers_and_decimals_are_exact() {
        let value: Value = serde_json::from_value(serde_json::json!(9007199254740993u64)).unwrap();
        assert_eq!(
            serde_json::to_value(&value).unwrap(),
            serde_json::json!({"kind": "integer", "value": "9007199254740993"})
        );
        let value: Value = serde_json::from_value(serde_json::json!(0.10)).unwrap();
        assert_eq!(
            serde_json::to_value(&value).unwrap(),
            serde_json::json!({"kind": "decimal", "value": "0.1"})
        );
    }

    #[test]
    fn quantity_round_trip() {
        let raw = serde_json::json!({
            "kind": "quantity",
            "value": {"kind": "decimal", "value": "9.81"},
            "dimension": {"length": 1, "time": -2}
        });
        let value: Value = serde_json::from_value(raw.clone()).unwrap();
        assert_eq!(serde_json::to_value(&value).unwrap(), raw);
    }

    #[test]
    fn money_requires_valid_currency() {
        let raw = serde_json::json!({
            "kind": "money",
            "amount": {"kind": "decimal", "value": "10.00"},
            "currency": "usd"
        });
        assert!(serde_json::from_value::<Value>(raw).is_err());
    }

    #[test]
    fn record_key_kind_is_rejected() {
        let raw = serde_json::json!({"kind": "not-a-real-tag", "value": 1});
        assert!(serde_json::from_value::<Value>(raw).is_err());
    }

    #[test]
    fn matrix_shape_is_checked() {
        let raw = serde_json::json!({
            "kind": "matrix", "rows": 2, "cols": 2,
            "data": [1, 2, 3]
        });
        assert!(serde_json::from_value::<Value>(raw).is_err());
    }
}
