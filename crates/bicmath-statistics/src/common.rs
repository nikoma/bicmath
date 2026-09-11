//! Shared helpers for the statistics module.
//!
//! All numeric conversions are explicit: float64 never silently enters exact
//! arithmetic, non-finite values are rejected, and non-numeric array entries are
//! reported with their path.

use std::collections::BTreeMap;

use bicmath_core::context::ExecContext;
use bicmath_core::contract::Args;
use bicmath_core::envelope::Exactness;
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::number::{Decimal, Number, NumericMode};
use bicmath_core::schema::{FieldSchema, NumberKind, ValueSchema};
use bicmath_core::value::Value;
use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{One, Signed, ToPrimitive, Zero};

use crate::mathfn::sqrt;

// ---------------------------------------------------------------------------
// Modes and schemas
// ---------------------------------------------------------------------------

/// Descriptive statistics support all three numeric modes.
pub fn all_modes() -> Vec<NumericMode> {
    vec![
        NumericMode::Exact,
        NumericMode::Auto,
        NumericMode::Scientific,
    ]
}

/// Distributions and inference always produce float64 results and therefore
/// require auto or scientific mode.
pub fn inferential_modes() -> Vec<NumericMode> {
    vec![NumericMode::Auto, NumericMode::Scientific]
}

pub fn any_number_schema() -> ValueSchema {
    ValueSchema::number(NumberKind::Any)
}

pub fn float64_schema() -> ValueSchema {
    ValueSchema::number(NumberKind::Float64)
}

pub fn integer_schema() -> ValueSchema {
    ValueSchema::number(NumberKind::Integer)
}

pub fn text_schema() -> ValueSchema {
    ValueSchema::text()
}

pub fn bool_schema() -> ValueSchema {
    ValueSchema::Bool
}

pub fn array_schema(items: ValueSchema) -> ValueSchema {
    ValueSchema::array(items)
}

pub fn record_schema(fields: Vec<FieldSchema>, allow_extra: bool) -> ValueSchema {
    ValueSchema::Record {
        fields,
        allow_extra,
    }
}

pub fn field(name: &str, schema: ValueSchema) -> FieldSchema {
    FieldSchema::required(name, schema)
}

pub fn optional_field(name: &str, schema: ValueSchema) -> FieldSchema {
    FieldSchema::optional(name, schema)
}

pub fn number_field(name: &str) -> FieldSchema {
    FieldSchema::required(name, any_number_schema())
}

// ---------------------------------------------------------------------------
// Example helpers
// ---------------------------------------------------------------------------

pub fn parse_value(raw: serde_json::Value) -> Value {
    serde_json::from_value(raw).expect("example value must be valid")
}

pub fn example_args(pairs: &[(&str, serde_json::Value)]) -> BTreeMap<String, Value> {
    pairs
        .iter()
        .map(|(name, raw)| (name.to_string(), parse_value(raw.clone())))
        .collect()
}

// ---------------------------------------------------------------------------
// Value construction
// ---------------------------------------------------------------------------

pub fn record(entries: Vec<(&str, Value)>) -> Value {
    Value::Record(
        entries
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect(),
    )
}

pub fn text(value: impl Into<String>) -> Value {
    Value::Text(value.into())
}

pub fn integer_value(value: u64) -> Value {
    Value::integer(BigInt::from(value))
}

pub fn bool_value(value: bool) -> Value {
    Value::Bool(value)
}

pub fn array_value(values: Vec<Value>) -> Value {
    Value::Array(values)
}

pub fn number_value(value: Number) -> Value {
    Value::Number(value)
}

pub fn float_value(value: f64) -> Result<Value, EngineError> {
    Ok(Value::Number(float_number(value)?))
}

pub fn rational_value(value: BigRational) -> Value {
    Value::Number(rational_to_number(value))
}

pub fn rational_to_number(value: BigRational) -> Number {
    if value.is_integer() {
        Number::Integer(value.to_integer())
    } else {
        Number::Rational(value)
    }
}

pub fn float_number(value: f64) -> Result<Number, EngineError> {
    if !value.is_finite() {
        return Err(EngineError::domain(
            "calculation produced a non-finite float64 result",
        ));
    }
    Number::float(value)
}

pub fn float_array(values: &[f64]) -> Result<Value, EngineError> {
    values
        .iter()
        .map(|value| float_value(*value))
        .collect::<Result<Vec<_>, _>>()
        .map(array_value)
}

/// A text array of method assumptions, shared by the inference records.
pub fn assumptions_value(statements: &[&str]) -> Value {
    array_value(
        statements
            .iter()
            .map(|statement| text(*statement))
            .collect(),
    )
}

// ---------------------------------------------------------------------------
// Parameter parsing
// ---------------------------------------------------------------------------

pub fn ensure_finite(number: &Number) -> Result<(), EngineError> {
    if let Number::Float64(value) = number
        && !value.get().is_finite()
    {
        return Err(EngineError::domain(
            "non-finite float64 values are not accepted",
        ));
    }
    Ok(())
}

pub fn number_to_f64(number: &Number) -> Result<f64, EngineError> {
    ensure_finite(number)?;
    let value = number.to_f64().ok_or_else(|| {
        EngineError::domain(format!(
            "{} value is not representable as float64",
            number.kind_name()
        ))
    })?;
    if !value.is_finite() {
        return Err(EngineError::domain("value overflows float64"));
    }
    Ok(value)
}

pub fn scalar_f64(args: &Args, name: &str) -> Result<f64, EngineError> {
    number_to_f64(args.number(name)?).map_err(|e| e.with_path(name.to_string()))
}

pub fn optional_f64_param(args: &Args, name: &str) -> Result<Option<f64>, EngineError> {
    match args.optional_number(name)? {
        None => Ok(None),
        Some(number) => number_to_f64(number)
            .map(Some)
            .map_err(|e| e.with_path(name.to_string())),
    }
}

pub fn confidence_param(args: &Args) -> Result<f64, EngineError> {
    let value = optional_f64_param(args, "confidence")?.unwrap_or(0.95);
    if !(0.0..1.0).contains(&value) {
        return Err(
            EngineError::domain("confidence must be strictly between 0 and 1")
                .with_path("confidence".to_string()),
        );
    }
    Ok(value)
}

pub fn alpha_param(args: &Args) -> Result<f64, EngineError> {
    let value = optional_f64_param(args, "alpha")?.unwrap_or(0.05);
    if !(0.0..1.0).contains(&value) {
        return Err(
            EngineError::domain("alpha must be strictly between 0 and 1")
                .with_path("alpha".to_string()),
        );
    }
    Ok(value)
}

pub fn power_param(args: &Args) -> Result<f64, EngineError> {
    let value = optional_f64_param(args, "power")?.unwrap_or(0.8);
    if !(0.0..1.0).contains(&value) {
        return Err(
            EngineError::domain("power must be strictly between 0 and 1")
                .with_path("power".to_string()),
        );
    }
    Ok(value)
}

pub fn allocation_ratio_param(args: &Args) -> Result<f64, EngineError> {
    let value = optional_f64_param(args, "allocation_ratio")?.unwrap_or(1.0);
    if value <= 0.0 {
        return Err(EngineError::domain("allocation_ratio must be positive")
            .with_path("allocation_ratio".to_string()));
    }
    Ok(value)
}

pub fn sided_param(args: &Args) -> Result<String, EngineError> {
    parse_choice(args, "sided", "two", &["two", "one"])
}

pub fn parse_choice(
    args: &Args,
    name: &str,
    default: &str,
    variants: &[&str],
) -> Result<String, EngineError> {
    match args.optional_text(name)? {
        None => Ok(default.to_string()),
        Some(text) => {
            if variants.contains(&text) {
                Ok(text.to_string())
            } else {
                Err(EngineError::domain(format!(
                    "unknown {name} {text:?}; expected one of {variants:?}"
                ))
                .with_path(name.to_string()))
            }
        }
    }
}

pub fn probability_param(args: &Args, name: &str, default: f64) -> Result<f64, EngineError> {
    let value = optional_f64_param(args, name)?.unwrap_or(default);
    if !(0.0..=1.0).contains(&value) {
        return Err(
            EngineError::domain(format!("{name} must be in [0, 1]")).with_path(name.to_string())
        );
    }
    Ok(value)
}

pub fn non_negative_u64(value: &BigInt, name: &str) -> Result<u64, EngineError> {
    if value.is_negative() {
        return Err(
            EngineError::domain(format!("{name} must be a non-negative integer"))
                .with_path(name.to_string()),
        );
    }
    value.to_u64().ok_or_else(|| {
        EngineError::domain(format!("{name} is too large for the supported range"))
            .with_path(name.to_string())
    })
}

pub fn insufficient(message: impl Into<String>) -> EngineError {
    EngineError::new(ErrorCode::InsufficientObservations, message)
}

// ---------------------------------------------------------------------------
// Input collection and exact/float dispatch
// ---------------------------------------------------------------------------

pub fn collect_numbers(args: &Args, name: &str) -> Result<Vec<Number>, EngineError> {
    let items = args.array(name)?;
    let mut out = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        match item {
            Value::Number(number) => {
                ensure_finite(number).map_err(|e| e.with_path(format!("{name}[{index}]")))?;
                out.push(number.clone());
            }
            other => {
                return Err(EngineError::malformed(format!(
                    "expected a number at {name}[{index}], found {}",
                    other.kind_name()
                ))
                .with_path(format!("{name}[{index}]")));
            }
        }
    }
    Ok(out)
}

/// Collect a non-empty numeric array as float64 values, preserving the path of
/// the offending entry in any conversion error.
pub fn float_vector(args: &Args, name: &str) -> Result<Vec<f64>, EngineError> {
    let numbers = collect_numbers(args, name)?;
    if numbers.is_empty() {
        return Err(insufficient(format!("{name} must not be empty")));
    }
    numbers
        .iter()
        .enumerate()
        .map(|(index, number)| {
            number_to_f64(number).map_err(|error| error.with_path(format!("{name}[{index}]")))
        })
        .collect()
}

/// A numeric series in the representation chosen for the calculation.
#[derive(Clone, Debug)]
pub enum Series {
    Exact(Vec<BigRational>),
    Float(Vec<f64>),
}

impl Series {
    pub fn len(&self) -> usize {
        match self {
            Series::Exact(values) => values.len(),
            Series::Float(values) => values.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn is_float(&self) -> bool {
        matches!(self, Series::Float(_))
    }

    /// Explicit conversion of the series to float64 for inferential methods
    /// whose output is always float64.
    pub fn to_f64_vec(&self) -> Result<Vec<f64>, EngineError> {
        match self {
            Series::Float(values) => Ok(values.clone()),
            Series::Exact(values) => values
                .iter()
                .map(|value| {
                    value
                        .to_f64()
                        .filter(|v| v.is_finite())
                        .ok_or_else(|| EngineError::domain("value overflows float64"))
                })
                .collect(),
        }
    }
}

/// Convert an input array to either exact rationals or float64 values.
///
/// If any element is float64 the whole series is computed in float64 and the
/// result is approximate. Exact mode rejects float64 input.
pub fn classify_series(args: &Args, name: &str, ctx: &ExecContext) -> Result<Series, EngineError> {
    let values = collect_numbers(args, name)?;
    let has_float = values.iter().any(Number::is_float);
    if has_float {
        if ctx.numeric.mode == NumericMode::Exact {
            return Err(EngineError::new(
                ErrorCode::UnsupportedNumericMode,
                format!("{name} contains float64 values, which require auto or scientific mode"),
            )
            .with_path(name.to_string()));
        }
        let mut out = Vec::with_capacity(values.len());
        for (index, value) in values.iter().enumerate() {
            let float =
                number_to_f64(value).map_err(|e| e.with_path(format!("{name}[{index}]")))?;
            out.push(float);
        }
        Ok(Series::Float(out))
    } else {
        let mut out = Vec::with_capacity(values.len());
        for (index, value) in values.iter().enumerate() {
            let rational = value.as_exact_rational().ok_or_else(|| {
                EngineError::internal("exact value could not be converted to a rational")
                    .with_path(format!("{name}[{index}]"))
            })?;
            out.push(rational);
        }
        Ok(Series::Exact(out))
    }
}

// ---------------------------------------------------------------------------
// Exact statistics
// ---------------------------------------------------------------------------

pub fn mean_exact(values: &[BigRational]) -> Result<BigRational, EngineError> {
    if values.is_empty() {
        return Err(insufficient("mean requires at least one observation"));
    }
    let n = BigInt::from(values.len());
    let sum: BigRational = values.iter().cloned().sum();
    Ok(sum / BigRational::from_integer(n))
}

pub fn variance_exact(values: &[BigRational], ddof: u64) -> Result<BigRational, EngineError> {
    let n = values.len() as u64;
    if values.is_empty() {
        return Err(insufficient("variance requires at least one observation"));
    }
    if n <= ddof {
        return Err(insufficient(format!(
            "variance with ddof={ddof} requires more than {ddof} observations"
        )));
    }
    let mean = mean_exact(values)?;
    let mut sum_squares = BigRational::zero();
    for value in values {
        let delta = value - &mean;
        sum_squares += &delta * &delta;
    }
    let denominator = BigRational::from_integer(BigInt::from(n - ddof));
    Ok(sum_squares / denominator)
}

pub fn covariance_exact(
    xs: &[BigRational],
    ys: &[BigRational],
    ddof: u64,
) -> Result<BigRational, EngineError> {
    let n = xs.len() as u64;
    if xs.is_empty() {
        return Err(insufficient("covariance requires at least one observation"));
    }
    if n <= ddof {
        return Err(insufficient(format!(
            "covariance with ddof={ddof} requires more than {ddof} observations"
        )));
    }
    let mean_x = mean_exact(xs)?;
    let mean_y = mean_exact(ys)?;
    let mut sum = BigRational::zero();
    for (x, y) in xs.iter().zip(ys.iter()) {
        sum += (x - &mean_x) * (y - &mean_y);
    }
    let denominator = BigRational::from_integer(BigInt::from(n - ddof));
    Ok(sum / denominator)
}

/// Exact square root of a non-negative rational, when one exists.
pub fn rational_sqrt_exact(value: &BigRational) -> Option<BigRational> {
    if value.is_negative() {
        return None;
    }
    let numer = value.numer().sqrt();
    let denom = value.denom().sqrt();
    if &numer * &numer == *value.numer() && &denom * &denom == *value.denom() {
        Some(BigRational::new(numer, denom))
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Float statistics (stable algorithms)
// ---------------------------------------------------------------------------

/// Neumaier compensated summation.
pub fn compensated_sum(values: &[f64]) -> f64 {
    let mut sum = 0.0f64;
    let mut compensation = 0.0f64;
    for &value in values {
        let next = sum + value;
        if sum.abs() >= value.abs() {
            compensation += (sum - next) + value;
        } else {
            compensation += (value - next) + sum;
        }
        sum = next;
    }
    sum + compensation
}

pub fn mean_f64(values: &[f64]) -> f64 {
    if values.is_empty() {
        return f64::NAN;
    }
    compensated_sum(values) / values.len() as f64
}

/// Welford's online algorithm for the sample variance.
pub fn variance_f64(values: &[f64], ddof: u64) -> Result<f64, EngineError> {
    let n = values.len() as u64;
    if values.is_empty() {
        return Err(insufficient("variance requires at least one observation"));
    }
    if n <= ddof {
        return Err(insufficient(format!(
            "variance with ddof={ddof} requires more than {ddof} observations"
        )));
    }
    let mut count = 0u64;
    let mut mean = 0.0f64;
    let mut m2 = 0.0f64;
    for &value in values {
        count += 1;
        let delta = value - mean;
        mean += delta / count as f64;
        let delta2 = value - mean;
        m2 += delta * delta2;
    }
    Ok(m2 / (n - ddof) as f64)
}

/// Two-pass centered covariance with compensated summation.
pub fn covariance_f64(xs: &[f64], ys: &[f64], ddof: u64) -> Result<f64, EngineError> {
    let n = xs.len() as u64;
    if xs.is_empty() {
        return Err(insufficient("covariance requires at least one observation"));
    }
    if n <= ddof {
        return Err(insufficient(format!(
            "covariance with ddof={ddof} requires more than {ddof} observations"
        )));
    }
    let mean_x = mean_f64(xs);
    let mean_y = mean_f64(ys);
    let mut sum = 0.0f64;
    let mut compensation = 0.0f64;
    for (x, y) in xs.iter().zip(ys.iter()) {
        let term = (x - mean_x) * (y - mean_y);
        let adjusted = term - compensation;
        let next = sum + adjusted;
        compensation = (next - sum) - adjusted;
        sum = next;
    }
    Ok(sum / (n - ddof) as f64)
}

// ---------------------------------------------------------------------------
// Moment dispatch
// ---------------------------------------------------------------------------

/// An exact or approximate statistical moment.
#[derive(Clone, Debug)]
pub enum Moment {
    Exact(BigRational),
    Float(f64),
}

pub fn mean_moment(series: &Series) -> Result<Moment, EngineError> {
    match series {
        Series::Exact(values) => Ok(Moment::Exact(mean_exact(values)?)),
        Series::Float(values) => {
            if values.is_empty() {
                return Err(insufficient("mean requires at least one observation"));
            }
            Ok(Moment::Float(mean_f64(values)))
        }
    }
}

pub fn variance_moment(series: &Series, ddof: u64) -> Result<Moment, EngineError> {
    match series {
        Series::Exact(values) => Ok(Moment::Exact(variance_exact(values, ddof)?)),
        Series::Float(values) => Ok(Moment::Float(variance_f64(values, ddof)?)),
    }
}

pub fn covariance_moment(xs: &Series, ys: &Series, ddof: u64) -> Result<Moment, EngineError> {
    match (xs, ys) {
        (Series::Exact(xs), Series::Exact(ys)) => {
            Ok(Moment::Exact(covariance_exact(xs, ys, ddof)?))
        }
        (Series::Float(xs), Series::Float(ys)) => Ok(Moment::Float(covariance_f64(xs, ys, ddof)?)),
        _ => Err(EngineError::internal(
            "mixed exact/float series reached covariance",
        )),
    }
}

/// Convert a variance moment to a standard deviation under the mode rules:
/// an exact perfect square stays exact, otherwise exact mode is rejected, auto
/// mode returns a decimal approximation, and scientific mode returns float64.
pub fn stddev_moment(
    moment: &Moment,
    mode: NumericMode,
) -> Result<(Number, Exactness), EngineError> {
    match moment {
        Moment::Exact(value) => {
            if value.is_negative() {
                return Err(EngineError::internal("negative exact variance"));
            }
            if let Some(root) = rational_sqrt_exact(value) {
                return Ok((rational_to_number(root), Exactness::Exact));
            }
            match mode {
                NumericMode::Exact => Err(EngineError::new(
                    ErrorCode::UnsupportedNumericMode,
                    "variance is not a perfect square; exact mode cannot represent the \
                     standard deviation; use auto or scientific mode",
                )),
                NumericMode::Auto => {
                    let approximate = value.to_f64().ok_or_else(|| {
                        EngineError::domain("variance is not representable as float64")
                    })?;
                    let root = sqrt(approximate);
                    let decimal = Decimal::from_f64_display(root).ok_or_else(|| {
                        EngineError::internal("could not format decimal approximation")
                    })?;
                    Ok((Number::Decimal(decimal), Exactness::Approximate))
                }
                NumericMode::Scientific => {
                    let approximate = value.to_f64().ok_or_else(|| {
                        EngineError::domain("variance is not representable as float64")
                    })?;
                    Ok((float_number(sqrt(approximate))?, Exactness::Approximate))
                }
            }
        }
        Moment::Float(value) => {
            if value.is_nan() || *value < 0.0 {
                return Err(EngineError::domain("negative float64 variance"));
            }
            Ok((float_number(sqrt(*value))?, Exactness::Approximate))
        }
    }
}

// ---------------------------------------------------------------------------
// Order statistics
// ---------------------------------------------------------------------------

pub fn extreme_number(series: &Series, minimum: bool) -> Result<(Number, Exactness), EngineError> {
    if series.is_empty() {
        return Err(insufficient(
            "minimum and maximum require at least one observation",
        ));
    }
    match series {
        Series::Exact(values) => {
            let mut best = values[0].clone();
            for value in &values[1..] {
                if (minimum && value < &best) || (!minimum && value > &best) {
                    best = value.clone();
                }
            }
            Ok((rational_to_number(best), Exactness::Exact))
        }
        Series::Float(values) => {
            let mut best = values[0];
            for &value in &values[1..] {
                if (minimum && value < best) || (!minimum && value > best) {
                    best = value;
                }
            }
            Ok((float_number(best)?, Exactness::Approximate))
        }
    }
}

pub fn median_number(series: &Series) -> Result<(Number, Exactness), EngineError> {
    if series.is_empty() {
        return Err(insufficient("median requires at least one observation"));
    }
    match series {
        Series::Exact(values) => {
            let mut sorted = values.clone();
            sorted.sort();
            let n = sorted.len();
            let median = if n % 2 == 1 {
                sorted[n / 2].clone()
            } else {
                (sorted[n / 2 - 1].clone() + sorted[n / 2].clone())
                    / BigRational::from_integer(BigInt::from(2))
            };
            Ok((rational_to_number(median), Exactness::Exact))
        }
        Series::Float(values) => {
            let mut sorted = values.clone();
            sorted.sort_by(|a, b| a.total_cmp(b));
            let n = sorted.len();
            let median = if n % 2 == 1 {
                sorted[n / 2]
            } else {
                0.5 * (sorted[n / 2 - 1] + sorted[n / 2])
            };
            Ok((float_number(median)?, Exactness::Approximate))
        }
    }
}

/// R-7 style quantiles with alternative order-statistic conventions.
///
/// With `h = (n - 1) * q`:
/// - `linear` interpolates `x[floor(h)] + (h - floor(h)) * (x[floor(h)+1] - x[floor(h)])`
/// - `lower` returns `x[floor(h)]`
/// - `higher` returns `x[ceil(h)]`
/// - `midpoint` returns `(x[floor(h)] + x[ceil(h)]) / 2`
/// - `nearest` returns `x[round_half_to_even(h)]`
pub fn quantile_number(
    series: &Series,
    q: &Number,
    method: &str,
) -> Result<(Number, Exactness), EngineError> {
    if series.is_empty() {
        return Err(insufficient("quantile requires at least one observation"));
    }
    let use_float = series.is_float() || q.is_float();
    if use_float {
        let p = number_to_f64(q)?;
        if !(0.0..=1.0).contains(&p) {
            return Err(EngineError::domain("q must be in [0, 1]"));
        }
        let mut sorted: Vec<f64> = match series {
            Series::Float(values) => values.clone(),
            Series::Exact(values) => values
                .iter()
                .map(|value| {
                    value
                        .to_f64()
                        .filter(|v| v.is_finite())
                        .ok_or_else(|| EngineError::domain("value overflows float64"))
                })
                .collect::<Result<_, _>>()?,
        };
        sorted.sort_by(|a, b| a.total_cmp(b));
        let n = sorted.len();
        let h = (n - 1) as f64 * p;
        let lower = h.floor() as usize;
        let fraction = h - lower as f64;
        let upper = if fraction == 0.0 {
            lower
        } else {
            (lower + 1).min(n - 1)
        };
        let result = match method {
            "linear" => sorted[lower] + fraction * (sorted[upper] - sorted[lower]),
            "lower" => sorted[lower],
            "higher" => sorted[upper],
            "midpoint" => 0.5 * (sorted[lower] + sorted[upper]),
            "nearest" => {
                let index = h.round_ties_even() as usize;
                sorted[index.min(n - 1)]
            }
            other => {
                return Err(EngineError::domain(format!(
                    "unknown quantile method {other:?}"
                )));
            }
        };
        return Ok((float_number(result)?, Exactness::Approximate));
    }
    let p = q
        .as_exact_rational()
        .ok_or_else(|| EngineError::internal("exact q could not be converted to a rational"))?;
    if p.is_negative() || p > BigRational::one() {
        return Err(EngineError::domain("q must be in [0, 1]"));
    }
    let values = match series {
        Series::Exact(values) => values,
        Series::Float(_) => {
            return Err(EngineError::internal(
                "float series reached the exact quantile path",
            ));
        }
    };
    let mut sorted = values.clone();
    sorted.sort();
    let n = sorted.len();
    let h = BigRational::from_integer(BigInt::from(n - 1)) * &p;
    let lower = h
        .floor()
        .to_integer()
        .to_usize()
        .ok_or_else(|| EngineError::internal("quantile index out of range"))?;
    let fraction = &h - BigRational::from_integer(h.floor().to_integer());
    let upper = if fraction.is_zero() {
        lower
    } else {
        (lower + 1).min(n - 1)
    };
    let result = match method {
        "linear" => &sorted[lower] + &fraction * (&sorted[upper] - &sorted[lower]),
        "lower" => sorted[lower].clone(),
        "higher" => sorted[upper].clone(),
        "midpoint" => {
            (&sorted[lower] + &sorted[upper]) / BigRational::from_integer(BigInt::from(2))
        }
        "nearest" => {
            let twice = &fraction * BigRational::from_integer(BigInt::from(2));
            let index = match twice.cmp(&BigRational::one()) {
                std::cmp::Ordering::Less => lower,
                std::cmp::Ordering::Greater => upper,
                std::cmp::Ordering::Equal => {
                    if lower % 2 == 0 {
                        lower
                    } else {
                        upper
                    }
                }
            };
            sorted[index.min(n - 1)].clone()
        }
        other => {
            return Err(EngineError::domain(format!(
                "unknown quantile method {other:?}"
            )));
        }
    };
    Ok((rational_to_number(result), Exactness::Exact))
}

/// All tied modes and their counts, sorted ascending.
pub fn mode_numbers(series: &Series) -> Result<(Vec<Number>, Vec<u64>), EngineError> {
    if series.is_empty() {
        return Err(insufficient("mode requires at least one observation"));
    }
    match series {
        Series::Exact(values) => {
            let mut counts: BTreeMap<BigRational, u64> = BTreeMap::new();
            for value in values {
                *counts.entry(value.clone()).or_insert(0) += 1;
            }
            let max = counts.values().copied().max().unwrap_or(0);
            let mut modes = Vec::new();
            let mut mode_counts = Vec::new();
            for (value, count) in counts {
                if count == max {
                    modes.push(rational_to_number(value));
                    mode_counts.push(count);
                }
            }
            Ok((modes, mode_counts))
        }
        Series::Float(values) => {
            let mut sorted: Vec<f64> = values
                .iter()
                .map(|value| if *value == 0.0 { 0.0 } else { *value })
                .collect();
            sorted.sort_by(|a, b| a.total_cmp(b));
            let mut groups: Vec<(f64, u64)> = Vec::new();
            for value in sorted {
                match groups.last_mut() {
                    Some(last) if last.0 == value => last.1 += 1,
                    _ => groups.push((value, 1)),
                }
            }
            let max = groups.iter().map(|group| group.1).max().unwrap_or(0);
            let mut modes = Vec::new();
            let mut mode_counts = Vec::new();
            for (value, count) in groups {
                if count == max {
                    modes.push(float_number(value)?);
                    mode_counts.push(count);
                }
            }
            Ok((modes, mode_counts))
        }
    }
}
