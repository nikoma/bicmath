//! Interval module: heuristically padded binary64 interval arithmetic.
//!
//! Every function returns an enclosure of the exact real result. Binary64
//! operations are performed in round-to-nearest and then widened outward with
//! [`f64::next_down`] and [`f64::next_up`] by a documented ulp pad, so the
//! returned interval contains the exact image under the documented binary64 and
//! libm accuracy assumption.
//!
//! # Representation
//!
//! An interval is a pair of finite binary64 endpoints `lo <= hi`. The empty
//! interval is represented by a pair of NaNs and is produced by `intersect`
//! when two intervals are disjoint. Inputs whose endpoints are NaN or infinite
//! are rejected with [`ErrorCode::DomainViolation`]; the same applies to
//! operations whose finite result would overflow binary64.
//!
//! # Rounding policy
//!
//! - Exact conversions of user inputs widen an inexact endpoint outward by two
//!   ulps (`f64::next_down`/`f64::next_up`), because decimal-to-binary and
//!   rational-to-binary conversion are correctly rounded to within one ulp.
//! - Every computed arithmetic or transcendental endpoint is widened outward by
//!   [`ULP_PAD`] ulps. One ulp covers the round-to-nearest error of a binary64
//!   operation; the remaining pad covers the documented assumption that the
//!   platform `libm` transcendentals are accurate to within a few ulps.
//! - Integer powers use interval exponentiation by squaring: each intermediate
//!   interval multiply is widened, so the composed enclosure remains valid.
//! - Trigonometric range analysis includes every critical point inside the
//!   interval. Intervals wider than one period return `[-1, 1]`. Arguments with
//!   magnitude above `2^53` are handled conservatively because floating-point
//!   argument reduction can no longer certify critical points.
//!
//! # Domain rules
//!
//! `div` and the `%` operator reject divisors that contain zero. `ln`, `log10`,
//! and `log2` require a strictly positive interval. `tan` rejects any interval
//! that crosses a pole. `pow_int` rejects a zero-containing base for negative
//! exponents. Unknown variables and functions in `evaluate` produce structured
//! [`ErrorCode::NotFound`] and [`ErrorCode::UnknownFunction`] errors.

#![forbid(unsafe_code)]

mod f64math;

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::Arc;

use num_bigint::BigInt;
use num_traits::ToPrimitive;

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, Assumption, CostClass, ErrorEstimate, Example, Function, FunctionDescriptor, Module,
    ModuleDescriptor, Outcome, ParamDescriptor, SimpleFunction, Warning,
};
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::expr::{BinaryOp, Expr, UnaryOp, parse_expression};
use bicmath_core::limits::Limits;
use bicmath_core::number::{Float64, Number, NumericMode};
use bicmath_core::schema::{FieldSchema, NumberKind, ValueSchema};
use bicmath_core::value::Value;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Outward widening, in ulps, applied to every computed endpoint.
///
/// One ulp covers the round-to-nearest error of a single binary64 operation;
/// the remaining three ulps cover the documented platform `libm` accuracy
/// assumption for transcendental functions.
pub const ULP_PAD: u32 = 4;

/// Outward widening, in ulps, applied when an exact input value is not exactly
/// representable as binary64. Decimal parsing is correctly rounded, and rational
/// conversion performs at most two roundings, so two ulps are sufficient.
const CONVERSION_PAD: u32 = 2;

/// Magnitude beyond which floating-point argument reduction is not trusted to
/// certify trigonometric critical points.
const TRIG_ARG_LIMIT: f64 = 9_007_199_254_740_992.0; // 2^53

// ---------------------------------------------------------------------------
// The interval type
// ---------------------------------------------------------------------------

/// A closed interval of real numbers with finite binary64 endpoints.
///
/// The canonical empty interval has NaN endpoints; use [`Interval::is_empty`]
/// rather than inspecting the fields directly.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Interval {
    /// Lower endpoint. NaN for the empty interval.
    pub lo: f64,
    /// Upper endpoint. NaN for the empty interval.
    pub hi: f64,
}

impl Interval {
    /// The canonical empty interval.
    pub const EMPTY: Interval = Interval {
        lo: f64::NAN,
        hi: f64::NAN,
    };

    /// A degenerate interval containing exactly one binary64 value.
    pub fn point(value: f64) -> Interval {
        Interval {
            lo: value,
            hi: value,
        }
    }

    /// Validate and construct an interval. NaN, infinite, or inverted endpoints
    /// are rejected.
    pub fn new(lo: f64, hi: f64) -> Result<Interval, EngineError> {
        if lo.is_nan() || hi.is_nan() {
            return Err(EngineError::domain("interval endpoints must not be NaN"));
        }
        if !lo.is_finite() || !hi.is_finite() {
            return Err(EngineError::domain("interval endpoints must be finite"));
        }
        if lo > hi {
            return Err(EngineError::domain(
                "interval lower bound must not exceed the upper bound",
            ));
        }
        Ok(Interval { lo, hi })
    }

    /// True for the canonical empty interval (and for any invalid NaN pair).
    pub fn is_empty(&self) -> bool {
        self.lo.is_nan() || self.hi.is_nan() || self.lo > self.hi
    }

    /// True when the interval is non-empty and contains zero.
    pub fn contains_zero(&self) -> bool {
        !self.is_empty() && self.lo <= 0.0 && self.hi >= 0.0
    }

    /// Approximate width `hi - lo`; zero for the empty interval. May be
    /// infinite when the mathematical width exceeds binary64.
    pub fn width(&self) -> f64 {
        if self.is_empty() {
            0.0
        } else {
            self.hi - self.lo
        }
    }

    /// Approximate midpoint, computed without overflow; `None` for the empty
    /// interval.
    pub fn midpoint(&self) -> Option<f64> {
        if self.is_empty() {
            None
        } else {
            Some(self.lo / 2.0 + self.hi / 2.0)
        }
    }
}

// ---------------------------------------------------------------------------
// Rounding helpers
// ---------------------------------------------------------------------------

fn widen_down(value: f64, steps: u32) -> f64 {
    let mut out = value;
    for _ in 0..steps {
        out = out.next_down();
    }
    out
}

fn widen_up(value: f64, steps: u32) -> f64 {
    let mut out = value;
    for _ in 0..steps {
        out = out.next_up();
    }
    out
}

fn require_finite(value: f64, what: &str) -> Result<f64, EngineError> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(EngineError::domain(format!(
            "{what} exceeds the finite binary64 range"
        )))
    }
}

fn pad_interval(lo: f64, hi: f64) -> Result<Interval, EngineError> {
    let padded_lo = widen_down(lo, ULP_PAD);
    let padded_hi = widen_up(hi, ULP_PAD);
    if !padded_lo.is_finite() || !padded_hi.is_finite() {
        return Err(EngineError::domain(
            "result exceeds the finite binary64 range",
        ));
    }
    Ok(Interval {
        lo: padded_lo,
        hi: padded_hi,
    })
}

// ---------------------------------------------------------------------------
// Exact-number conversion
// ---------------------------------------------------------------------------

fn number_to_f64(number: &Number, what: &str) -> Result<f64, EngineError> {
    let value = number.to_f64().ok_or_else(|| {
        EngineError::domain(format!("{what} is not representable as a finite float64"))
    })?;
    if !value.is_finite() {
        return Err(EngineError::domain(format!(
            "{what} is not a finite real number"
        )));
    }
    Ok(value)
}

fn is_exact_f64(number: &Number, value: f64) -> bool {
    let Some(exact) = number.to_exact_rational() else {
        return false;
    };
    let Ok(float) = Float64::new(value) else {
        return false;
    };
    float
        .to_rational_exact()
        .is_some_and(|converted| converted == exact)
}

fn exact_lower(number: &Number, what: &str) -> Result<f64, EngineError> {
    let value = number_to_f64(number, what)?;
    Ok(if is_exact_f64(number, value) {
        value
    } else {
        widen_down(value, CONVERSION_PAD)
    })
}

fn exact_upper(number: &Number, what: &str) -> Result<f64, EngineError> {
    let value = number_to_f64(number, what)?;
    Ok(if is_exact_f64(number, value) {
        value
    } else {
        widen_up(value, CONVERSION_PAD)
    })
}

/// Enclose an exact number: a point interval when the value is exactly
/// representable, otherwise the nearest binary64 widened outward.
pub fn enclose_number(number: &Number) -> Result<Interval, EngineError> {
    let lo = exact_lower(number, "value")?;
    let hi = exact_upper(number, "value")?;
    Interval::new(lo, hi)
}

// ---------------------------------------------------------------------------
// Interval operations
// ---------------------------------------------------------------------------

/// Minkowski sum `a + b`.
pub fn add(a: &Interval, b: &Interval) -> Result<Interval, EngineError> {
    if a.is_empty() || b.is_empty() {
        return Ok(Interval::EMPTY);
    }
    let lo = require_finite(a.lo + b.lo, "interval sum")?;
    let hi = require_finite(a.hi + b.hi, "interval sum")?;
    pad_interval(lo, hi)
}

/// Difference `a - b`.
pub fn sub(a: &Interval, b: &Interval) -> Result<Interval, EngineError> {
    if a.is_empty() || b.is_empty() {
        return Ok(Interval::EMPTY);
    }
    let lo = require_finite(a.lo - b.hi, "interval difference")?;
    let hi = require_finite(a.hi - b.lo, "interval difference")?;
    pad_interval(lo, hi)
}

/// Product `a * b` over all independent choices.
pub fn mul(a: &Interval, b: &Interval) -> Result<Interval, EngineError> {
    if a.is_empty() || b.is_empty() {
        return Ok(Interval::EMPTY);
    }
    let candidates = [a.lo * b.lo, a.lo * b.hi, a.hi * b.lo, a.hi * b.hi];
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for candidate in candidates {
        let value = require_finite(candidate, "interval product")?;
        lo = lo.min(value);
        hi = hi.max(value);
    }
    pad_interval(lo, hi)
}

/// Quotient `a / b`. The divisor must not contain zero.
pub fn div(a: &Interval, b: &Interval) -> Result<Interval, EngineError> {
    if a.is_empty() || b.is_empty() {
        return Ok(Interval::EMPTY);
    }
    if b.contains_zero() {
        return Err(EngineError::domain(
            "division by an interval that contains zero",
        ));
    }
    let candidates = [a.lo / b.lo, a.lo / b.hi, a.hi / b.lo, a.hi / b.hi];
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for candidate in candidates {
        let value = require_finite(candidate, "interval quotient")?;
        lo = lo.min(value);
        hi = hi.max(value);
    }
    pad_interval(lo, hi)
}

/// Negation. Exact sign change; no rounding is introduced.
pub fn neg(a: &Interval) -> Interval {
    if a.is_empty() {
        Interval::EMPTY
    } else {
        Interval {
            lo: -a.hi,
            hi: -a.lo,
        }
    }
}

/// Absolute value. Exact piecewise handling; a zero-straddling interval maps to
/// `[0, max(-lo, hi)]`.
pub fn abs(a: &Interval) -> Interval {
    if a.is_empty() {
        Interval::EMPTY
    } else if a.lo >= 0.0 {
        *a
    } else if a.hi <= 0.0 {
        Interval {
            lo: -a.hi,
            hi: -a.lo,
        }
    } else {
        Interval {
            lo: 0.0,
            hi: (-a.lo).max(a.hi),
        }
    }
}

/// Square root on the non-negative part of the interval.
pub fn sqrt(a: &Interval) -> Result<Interval, EngineError> {
    if a.is_empty() {
        return Ok(Interval::EMPTY);
    }
    if a.hi < 0.0 {
        return Err(EngineError::domain(
            "square root of a strictly negative interval",
        ));
    }
    let lo = a.lo.max(0.0);
    pad_interval(f64math::sqrt(lo), f64math::sqrt(a.hi))
}

/// Natural exponential. Monotone increasing.
pub fn exp(a: &Interval) -> Result<Interval, EngineError> {
    if a.is_empty() {
        return Ok(Interval::EMPTY);
    }
    pad_interval(f64math::exp(a.lo), f64math::exp(a.hi))
}

/// Natural logarithm. Requires a strictly positive interval.
pub fn ln(a: &Interval) -> Result<Interval, EngineError> {
    if a.is_empty() {
        return Ok(Interval::EMPTY);
    }
    if a.lo <= 0.0 {
        return Err(EngineError::domain(
            "logarithm of an interval that is not strictly positive",
        ));
    }
    pad_interval(f64math::ln(a.lo), f64math::ln(a.hi))
}

/// Base-10 logarithm. Requires a strictly positive interval.
pub fn log10(a: &Interval) -> Result<Interval, EngineError> {
    if a.is_empty() {
        return Ok(Interval::EMPTY);
    }
    if a.lo <= 0.0 {
        return Err(EngineError::domain(
            "base-10 logarithm of an interval that is not strictly positive",
        ));
    }
    pad_interval(f64math::log10(a.lo), f64math::log10(a.hi))
}

/// Base-2 logarithm. Requires a strictly positive interval.
pub fn log2(a: &Interval) -> Result<Interval, EngineError> {
    if a.is_empty() {
        return Ok(Interval::EMPTY);
    }
    if a.lo <= 0.0 {
        return Err(EngineError::domain(
            "base-2 logarithm of an interval that is not strictly positive",
        ));
    }
    pad_interval(f64math::log2(a.lo), f64math::log2(a.hi))
}

/// True when some point `offset + k * period` (integer `k`) lies in `[lo, hi]`.
fn has_critical_point(lo: f64, hi: f64, offset: f64, period: f64) -> bool {
    if !lo.is_finite() || !hi.is_finite() || period <= 0.0 {
        return false;
    }
    let start = ((lo - offset) / period).ceil();
    if !start.is_finite() {
        return false;
    }
    let point = offset + start * period;
    point >= lo && point <= hi
}

fn trig_conservative(a: &Interval) -> bool {
    a.width() >= 2.0 * std::f64::consts::PI
        || (a.lo != a.hi && (a.lo.abs() > TRIG_ARG_LIMIT || a.hi.abs() > TRIG_ARG_LIMIT))
}

/// Sine range over the interval, including interior extrema.
pub fn sin(a: &Interval) -> Result<Interval, EngineError> {
    if a.is_empty() {
        return Ok(Interval::EMPTY);
    }
    if trig_conservative(a) {
        return pad_interval(-1.0, 1.0);
    }
    let mut lo = f64math::sin(a.lo);
    let mut hi = f64math::sin(a.hi);
    if lo > hi {
        std::mem::swap(&mut lo, &mut hi);
    }
    let half_pi = std::f64::consts::FRAC_PI_2;
    if has_critical_point(a.lo, a.hi, half_pi, 2.0 * std::f64::consts::PI) {
        hi = 1.0;
    }
    if has_critical_point(a.lo, a.hi, -half_pi, 2.0 * std::f64::consts::PI) {
        lo = -1.0;
    }
    pad_interval(lo, hi)
}

/// Cosine range over the interval, including interior extrema.
pub fn cos(a: &Interval) -> Result<Interval, EngineError> {
    if a.is_empty() {
        return Ok(Interval::EMPTY);
    }
    if trig_conservative(a) {
        return pad_interval(-1.0, 1.0);
    }
    let mut lo = f64math::cos(a.lo);
    let mut hi = f64math::cos(a.hi);
    if lo > hi {
        std::mem::swap(&mut lo, &mut hi);
    }
    if has_critical_point(a.lo, a.hi, 0.0, 2.0 * std::f64::consts::PI) {
        hi = 1.0;
    }
    if has_critical_point(a.lo, a.hi, std::f64::consts::PI, 2.0 * std::f64::consts::PI) {
        lo = -1.0;
    }
    pad_interval(lo, hi)
}

/// Tangent. The interval must not cross a pole `pi/2 + k*pi`.
pub fn tan(a: &Interval) -> Result<Interval, EngineError> {
    if a.is_empty() {
        return Ok(Interval::EMPTY);
    }
    let pi = std::f64::consts::PI;
    if a.width() >= pi {
        return Err(EngineError::domain(
            "tangent of an interval that crosses a pole",
        ));
    }
    let huge = a.lo.abs() > TRIG_ARG_LIMIT || a.hi.abs() > TRIG_ARG_LIMIT;
    if huge {
        if a.lo != a.hi {
            return Err(EngineError::domain(
                "tangent argument is too large to exclude poles by argument reduction",
            ));
        }
    } else if has_critical_point(a.lo, a.hi, pi / 2.0, pi) {
        return Err(EngineError::domain(
            "tangent of an interval that crosses a pole",
        ));
    }
    pad_interval(f64math::tan(a.lo), f64math::tan(a.hi))
}

/// Set intersection of two enclosures; disjoint inputs produce the empty
/// interval.
pub fn intersect(a: &Interval, b: &Interval) -> Interval {
    if a.is_empty() || b.is_empty() {
        return Interval::EMPTY;
    }
    let lo = a.lo.max(b.lo);
    let hi = a.hi.min(b.hi);
    if lo > hi {
        Interval::EMPTY
    } else {
        Interval { lo, hi }
    }
}

/// Smallest interval containing both operands; empty operands are ignored.
pub fn hull(a: &Interval, b: &Interval) -> Interval {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => Interval::EMPTY,
        (true, false) => *b,
        (false, true) => *a,
        (false, false) => Interval {
            lo: a.lo.min(b.lo),
            hi: a.hi.max(b.hi),
        },
    }
}

fn trunc_interval(a: &Interval) -> Interval {
    if a.is_empty() {
        Interval::EMPTY
    } else {
        Interval {
            lo: a.lo.trunc(),
            hi: a.hi.trunc(),
        }
    }
}

/// Truncated remainder `a - b * trunc(a / b)`, matching the `%` operator of the
/// shared numeric contract. The divisor must not contain zero.
pub fn rem(a: &Interval, b: &Interval) -> Result<Interval, EngineError> {
    if a.is_empty() || b.is_empty() {
        return Ok(Interval::EMPTY);
    }
    if b.contains_zero() {
        return Err(EngineError::domain(
            "remainder by an interval that contains zero",
        ));
    }
    let quotient = div(a, b)?;
    let multiple = trunc_interval(&quotient);
    sub(a, &mul(b, &multiple)?)
}

fn point_power(value: f64, exponent: u32) -> Result<Interval, EngineError> {
    let mut result = Interval::point(1.0);
    let mut factor = Interval::point(value);
    let mut remaining = exponent;
    while remaining > 0 {
        if remaining & 1 == 1 {
            result = mul(&result, &factor)?;
        }
        remaining >>= 1;
        if remaining > 0 {
            factor = mul(&factor, &factor)?;
        }
    }
    Ok(result)
}

/// Integer power of an interval. Even powers clamp the lower bound at zero;
/// negative exponents require a base that excludes zero.
pub fn pow_int(base: &Interval, exponent: i64) -> Result<Interval, EngineError> {
    if base.is_empty() {
        return Ok(Interval::EMPTY);
    }
    if exponent == 0 {
        return Ok(Interval::point(1.0));
    }
    if exponent < 0 {
        if base.contains_zero() {
            return Err(EngineError::domain(
                "negative power of an interval that contains zero",
            ));
        }
        let magnitude = exponent
            .checked_neg()
            .ok_or_else(|| EngineError::resource("exponent is out of the supported range"))?;
        let positive = pow_int(base, magnitude)?;
        return div(&Interval::point(1.0), &positive);
    }
    let n = exponent as u32;
    if n.is_multiple_of(2) {
        if base.lo >= 0.0 {
            let lower = if base.lo == 0.0 {
                Interval::point(0.0)
            } else {
                point_power(base.lo, n)?
            };
            let upper = point_power(base.hi, n)?;
            pad_interval(lower.lo, upper.hi)
        } else if base.hi <= 0.0 {
            let lower = point_power(base.hi, n)?;
            let upper = point_power(base.lo, n)?;
            pad_interval(lower.lo, upper.hi)
        } else {
            let left = point_power(base.lo, n)?;
            let right = point_power(base.hi, n)?;
            pad_interval(0.0, left.hi.max(right.hi))
        }
    } else {
        let lower = point_power(base.lo, n)?;
        let upper = point_power(base.hi, n)?;
        pad_interval(lower.lo, upper.hi)
    }
}

// ---------------------------------------------------------------------------
// Value conversion
// ---------------------------------------------------------------------------

fn interval_from_value(value: &Value, path: &str) -> Result<Interval, EngineError> {
    match value {
        Value::Number(number) => {
            enclose_number(number).map_err(|error| error.with_path(path.to_string()))
        }
        Value::Record(fields) => {
            let lo = fields.get("lo");
            let hi = fields.get("hi");
            if matches!(lo, Some(Value::Null)) && matches!(hi, Some(Value::Null)) {
                return Ok(Interval::EMPTY);
            }
            let lo = lo.ok_or_else(|| {
                EngineError::malformed(format!("interval at {path} is missing the lo field"))
                    .with_path(path.to_string())
            })?;
            let hi = hi.ok_or_else(|| {
                EngineError::malformed(format!("interval at {path} is missing the hi field"))
                    .with_path(path.to_string())
            })?;
            let lo = lo
                .as_number()
                .map_err(|error| error.with_path(format!("{path}.lo")))?;
            let hi = hi
                .as_number()
                .map_err(|error| error.with_path(format!("{path}.hi")))?;
            if lo.compare(hi)? == Ordering::Greater {
                return Err(EngineError::domain(format!(
                    "interval at {path} has lo greater than hi"
                ))
                .with_path(path.to_string()));
            }
            let lower = exact_lower(lo, &format!("{path}.lo"))?;
            let upper = exact_upper(hi, &format!("{path}.hi"))?;
            Interval::new(lower, upper).map_err(|error| error.with_path(path.to_string()))
        }
        other => Err(EngineError::malformed(format!(
            "{path} must be an interval record {{lo, hi}}, found {}",
            other.kind_name()
        ))
        .with_path(path.to_string())),
    }
}

fn interval_to_value(interval: &Interval) -> Result<Value, EngineError> {
    if interval.is_empty() {
        return Ok(Value::record([
            ("lo", Value::Null),
            ("hi", Value::Null),
            ("width", Value::Number(Number::float(0.0)?)),
            ("midpoint", Value::Null),
            ("assurance", Value::text("heuristic_padding_not_certified")),
            (
                "assurance_note",
                Value::text(
                    "endpoints are heuristically widened; this is not a verified error bound",
                ),
            ),
        ]));
    }
    let width = interval.width();
    let width_value = if width.is_finite() {
        Value::Number(Number::float(width)?)
    } else {
        Value::Null
    };
    let midpoint_value = match interval.midpoint() {
        Some(midpoint) if midpoint.is_finite() => Value::Number(Number::float(midpoint)?),
        _ => Value::Null,
    };
    Ok(Value::record([
        ("lo", Value::Number(Number::float(interval.lo)?)),
        ("hi", Value::Number(Number::float(interval.hi)?)),
        ("width", width_value),
        ("midpoint", midpoint_value),
        ("assurance", Value::text("heuristic_padding_not_certified")),
        (
            "assurance_note",
            Value::text("endpoints are heuristically widened; this is not a verified error bound"),
        ),
    ]))
}

// ---------------------------------------------------------------------------
// Outcomes
// ---------------------------------------------------------------------------

fn padding_assumption() -> Assumption {
    Assumption::unverified(
        "heuristic_ulp_padding",
        format!(
            "Every computed binary64 endpoint is widened outward by {ULP_PAD} ulps with \
             f64::next_down/f64::next_up. This is a heuristic padding, not a proven error \
             bound: enclosure is not guaranteed, particularly where the platform libm exceeds \
             the assumed accuracy. Do not use these intervals for safety-critical \
             certification."
        ),
    )
}

fn padding_estimate() -> ErrorEstimate {
    ErrorEstimate::new(
        "heuristic_ulp_padding",
        Value::integer(BigInt::from(ULP_PAD)),
        "outward widening of each computed endpoint; this is a heuristic padding, not a          proven error bound",
    )
    .with_notes(
        "estimated only: the enclosure is not certified and may fail to contain the true          result where the platform libm exceeds the assumed accuracy",
    )
}

fn not_certified_warning() -> Warning {
    Warning::new(
        "not_certified",
        "intervals are heuristically padded, not certified; enclosure is not guaranteed",
    )
}

fn interval_outcome(interval: Interval) -> Result<Outcome, EngineError> {
    let value = interval_to_value(&interval)?;
    Ok(Outcome::approximate(value)
        .with_assumption(padding_assumption())
        .with_warning(not_certified_warning())
        .with_error_estimate(padding_estimate()))
}

fn scalar_outcome(value: Value) -> Outcome {
    Outcome::approximate(value)
        .with_assumption(padding_assumption())
        .with_warning(not_certified_warning())
        .with_error_estimate(padding_estimate())
}

fn interval_arg(args: &Args, name: &str) -> Result<Interval, EngineError> {
    interval_from_value(args.require(name)?, name)
}

// ---------------------------------------------------------------------------
// Restricted expression evaluator
// ---------------------------------------------------------------------------

const SUPPORTED_CALLS: [&str; 14] = [
    "sin", "cos", "tan", "exp", "ln", "log10", "log2", "sqrt", "abs", "pow", "min", "max", "floor",
    "ceil",
];

struct EvalBudget {
    used: u64,
    max_operations: u64,
    max_depth: usize,
}

impl EvalBudget {
    fn new(limits: &Limits) -> EvalBudget {
        EvalBudget {
            used: 0,
            max_operations: limits.max_operations,
            max_depth: limits.max_ast_depth,
        }
    }

    fn tick(&mut self, depth: usize, ctx: &ExecContext) -> Result<(), EngineError> {
        if depth > self.max_depth {
            return Err(EngineError::resource(format!(
                "interval expression depth exceeds the limit of {}",
                self.max_depth
            )));
        }
        self.used = self.used.saturating_add(1);
        if self.used > self.max_operations {
            return Err(EngineError::resource(format!(
                "interval expression operation budget of {} exceeded",
                self.max_operations
            )));
        }
        if self.used.is_multiple_of(256) {
            ctx.check()?;
        }
        Ok(())
    }
}

fn expect_arity(name: &str, values: &[Interval], expected: usize) -> Result<(), EngineError> {
    if values.len() == expected {
        Ok(())
    } else {
        Err(EngineError::malformed(format!(
            "function {name} expects {expected} argument(s), found {}",
            values.len()
        )))
    }
}

fn integer_exponent(value: &Interval) -> Result<i64, EngineError> {
    if value.is_empty() {
        return Err(EngineError::domain("exponent interval is empty"));
    }
    if value.lo != value.hi {
        return Err(EngineError::domain(
            "exponent must be a single integer value",
        ));
    }
    let exponent = value.lo;
    if !exponent.is_finite() || exponent.fract() != 0.0 {
        return Err(EngineError::domain("exponent must be an integer"));
    }
    if exponent < i64::MIN as f64 || exponent > i64::MAX as f64 {
        return Err(EngineError::resource(
            "exponent is out of the supported range",
        ));
    }
    Ok(exponent as i64)
}

fn check_exponent(exponent: i64, ctx: &ExecContext) -> Result<(), EngineError> {
    if exponent.unsigned_abs() > ctx.limits.max_exponent as u64 {
        return Err(EngineError::resource(format!(
            "exponent magnitude exceeds the configured limit of {}",
            ctx.limits.max_exponent
        )));
    }
    Ok(())
}

fn interval_min(a: &Interval, b: &Interval) -> Interval {
    if a.is_empty() || b.is_empty() {
        return Interval::EMPTY;
    }
    Interval {
        lo: a.lo.min(b.lo),
        hi: a.hi.min(b.hi),
    }
}

fn interval_max(a: &Interval, b: &Interval) -> Interval {
    if a.is_empty() || b.is_empty() {
        return Interval::EMPTY;
    }
    Interval {
        lo: a.lo.max(b.lo),
        hi: a.hi.max(b.hi),
    }
}

fn floor_interval(a: &Interval) -> Interval {
    if a.is_empty() {
        Interval::EMPTY
    } else {
        Interval {
            lo: f64math::floor(a.lo),
            hi: f64math::floor(a.hi),
        }
    }
}

fn ceil_interval(a: &Interval) -> Interval {
    if a.is_empty() {
        Interval::EMPTY
    } else {
        Interval {
            lo: f64math::ceil(a.lo),
            hi: f64math::ceil(a.hi),
        }
    }
}

fn dispatch_call(
    name: &str,
    values: &[Interval],
    ctx: &ExecContext,
) -> Result<Interval, EngineError> {
    match name {
        "sin" => {
            expect_arity(name, values, 1)?;
            sin(&values[0])
        }
        "cos" => {
            expect_arity(name, values, 1)?;
            cos(&values[0])
        }
        "tan" => {
            expect_arity(name, values, 1)?;
            tan(&values[0])
        }
        "exp" => {
            expect_arity(name, values, 1)?;
            exp(&values[0])
        }
        "ln" => {
            expect_arity(name, values, 1)?;
            ln(&values[0])
        }
        "log10" => {
            expect_arity(name, values, 1)?;
            log10(&values[0])
        }
        "log2" => {
            expect_arity(name, values, 1)?;
            log2(&values[0])
        }
        "sqrt" => {
            expect_arity(name, values, 1)?;
            sqrt(&values[0])
        }
        "abs" => {
            expect_arity(name, values, 1)?;
            Ok(abs(&values[0]))
        }
        "floor" => {
            expect_arity(name, values, 1)?;
            Ok(floor_interval(&values[0]))
        }
        "ceil" => {
            expect_arity(name, values, 1)?;
            Ok(ceil_interval(&values[0]))
        }
        "pow" => {
            expect_arity(name, values, 2)?;
            let exponent = integer_exponent(&values[1])?;
            check_exponent(exponent, ctx)?;
            pow_int(&values[0], exponent)
        }
        "min" | "max" => {
            if values.is_empty() {
                return Err(EngineError::malformed(format!(
                    "function {name} expects at least one argument"
                )));
            }
            let mut result = values[0];
            for value in &values[1..] {
                result = if name == "min" {
                    interval_min(&result, value)
                } else {
                    interval_max(&result, value)
                };
            }
            Ok(result)
        }
        other => Err(EngineError::new(
            ErrorCode::UnknownFunction,
            format!("unknown function {other:?} in an interval expression"),
        )
        .with_details(serde_json::json!({
            "function": other,
            "supported": SUPPORTED_CALLS,
        }))),
    }
}

fn eval_interval(
    expr: &Expr,
    variables: &BTreeMap<String, Interval>,
    ctx: &ExecContext,
    budget: &mut EvalBudget,
    depth: usize,
) -> Result<Interval, EngineError> {
    budget.tick(depth, ctx)?;
    match expr {
        Expr::Number(number) => enclose_number(number),
        Expr::Ident(name) => variables.get(name).copied().ok_or_else(|| {
            let available: Vec<&String> = variables.keys().collect();
            EngineError::new(
                ErrorCode::NotFound,
                format!("unknown interval variable {name:?}; available: {available:?}"),
            )
            .with_details(serde_json::json!({
                "name": name,
                "available": available,
            }))
        }),
        Expr::Unary { op, expr } => {
            let value = eval_interval(expr, variables, ctx, budget, depth + 1)?;
            match op {
                UnaryOp::Neg => Ok(neg(&value)),
                UnaryOp::Pos => Ok(value),
                UnaryOp::Not => Err(EngineError::malformed(
                    "logical negation is not valid in an interval expression",
                )),
            }
        }
        Expr::Binary { op, left, right } => {
            let a = eval_interval(left, variables, ctx, budget, depth + 1)?;
            let b = eval_interval(right, variables, ctx, budget, depth + 1)?;
            match op {
                BinaryOp::Add => add(&a, &b),
                BinaryOp::Sub => sub(&a, &b),
                BinaryOp::Mul => mul(&a, &b),
                BinaryOp::Div => div(&a, &b),
                BinaryOp::Rem => rem(&a, &b),
                BinaryOp::Pow => {
                    let exponent = integer_exponent(&b)?;
                    check_exponent(exponent, ctx)?;
                    pow_int(&a, exponent)
                }
                BinaryOp::And | BinaryOp::Or => Err(EngineError::malformed(
                    "logical operators are not valid in an interval expression",
                )),
            }
        }
        Expr::Call { name, args } => {
            let mut values = Vec::with_capacity(args.len());
            for arg in args {
                if arg.name.is_some() {
                    return Err(EngineError::malformed(
                        "named arguments are not supported in an interval expression",
                    ));
                }
                values.push(eval_interval(
                    &arg.value,
                    variables,
                    ctx,
                    budget,
                    depth + 1,
                )?);
            }
            dispatch_call(name, &values, ctx)
        }
        Expr::Compare { .. } => Err(EngineError::malformed(
            "comparisons are not valid in an interval expression",
        )),
        Expr::Bool(_) | Expr::Text(_) | Expr::Array(_) | Expr::Record(_) => Err(
            EngineError::malformed("interval expression must produce a single interval value"),
        ),
    }
}

fn evaluate_source(
    source: &str,
    variables: &BTreeMap<String, Interval>,
    ctx: &ExecContext,
) -> Result<Interval, EngineError> {
    let expr = parse_expression(source, &ctx.limits)?;
    let mut budget = EvalBudget::new(&ctx.limits);
    eval_interval(&expr, variables, ctx, &mut budget, 0)
}

fn variables_arg(args: &Args) -> Result<BTreeMap<String, Interval>, EngineError> {
    let record = args.record("variables")?;
    let mut out = BTreeMap::new();
    for (name, raw) in record {
        out.insert(name.clone(), interval_from_value(raw, name)?);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Descriptor helpers
// ---------------------------------------------------------------------------

fn all_modes() -> Vec<NumericMode> {
    vec![
        NumericMode::Exact,
        NumericMode::Auto,
        NumericMode::Scientific,
    ]
}

fn interval_schema() -> ValueSchema {
    ValueSchema::Record {
        fields: vec![
            FieldSchema::required("lo", ValueSchema::Any)
                .with_description("Lower endpoint; null when the interval is empty."),
            FieldSchema::required("hi", ValueSchema::Any)
                .with_description("Upper endpoint; null when the interval is empty."),
            FieldSchema::required("width", ValueSchema::Any)
                .with_description("Approximate width; null when it exceeds binary64."),
            FieldSchema::required("midpoint", ValueSchema::Any)
                .with_description("Approximate midpoint; null when the interval is empty."),
            FieldSchema::optional("assurance", ValueSchema::text()).with_description(
                "Always \"heuristic_padding_not_certified\": intervals are not a verified bound.",
            ),
            FieldSchema::optional("assurance_note", ValueSchema::text())
                .with_description("Explicit limitation of the padding policy."),
        ],
        allow_extra: false,
    }
}

fn interval_input_schema() -> ValueSchema {
    ValueSchema::Record {
        fields: vec![
            FieldSchema::required("lo", ValueSchema::Any)
                .with_description("Lower endpoint; null when the interval is empty."),
            FieldSchema::required("hi", ValueSchema::Any)
                .with_description("Upper endpoint; null when the interval is empty."),
        ],
        allow_extra: true,
    }
}

fn number_schema() -> ValueSchema {
    ValueSchema::number(NumberKind::Any)
}

fn integer_schema() -> ValueSchema {
    ValueSchema::number(NumberKind::Integer)
}

fn iv_param(name: &str, description: &str) -> ParamDescriptor {
    ParamDescriptor::required(name, description, interval_input_schema()).with_shorthand(true)
}

fn num_param(name: &str, description: &str) -> ParamDescriptor {
    ParamDescriptor::required(name, description, number_schema())
}

fn int_param(name: &str, description: &str) -> ParamDescriptor {
    ParamDescriptor::required(name, description, integer_schema())
}

fn base_descriptor(
    id: &str,
    title: &str,
    summary: &str,
    parameters: Vec<ParamDescriptor>,
) -> FunctionDescriptor {
    let anchor = id.strip_prefix("interval.").unwrap_or(id);
    FunctionDescriptor::new(id, "interval", "1.0.0", title, summary)
        .with_parameters(parameters)
        .with_modes(all_modes())
        .with_units_rule(
            "All inputs and outputs are dimensionless real quantities represented as finite \
             binary64 intervals.",
        )
        .with_method_ref(format!("docs/methods/interval.md#{anchor}"))
        .with_tags(["interval", "heuristic_padding", "outward_rounding"])
}

fn example_args(pairs: &[(&str, serde_json::Value)]) -> BTreeMap<String, Value> {
    pairs
        .iter()
        .map(|(name, raw)| ((*name).to_string(), parse_value(raw.clone())))
        .collect()
}

fn parse_value(raw: serde_json::Value) -> Value {
    serde_json::from_value(raw).expect("example value must be valid")
}

fn parse_number(text: &str) -> Number {
    Number::parse_literal(text, &Limits::conservative()).expect("example literal must parse")
}

fn example_interval_value(interval: Interval) -> Value {
    interval_to_value(&interval).expect("interval example value must serialize")
}

fn evaluate_for_example(source: &str, variables: &[(&str, Interval)]) -> Interval {
    let ctx = ExecContext::conservative();
    let variables: BTreeMap<String, Interval> = variables
        .iter()
        .map(|(name, interval)| ((*name).to_string(), *interval))
        .collect();
    evaluate_source(source, &variables, &ctx).expect("example expression must evaluate")
}

// ---------------------------------------------------------------------------
// Function invocations
// ---------------------------------------------------------------------------

fn invoke_enclose(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    interval_outcome(enclose_number(args.number("value")?)?)
}

fn invoke_from_bounds(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let lo = args.number("lo")?;
    let hi = args.number("hi")?;
    if lo.compare(hi)? == Ordering::Greater {
        return Err(EngineError::domain("lo must not exceed hi"));
    }
    let lower = exact_lower(lo, "lo")?;
    let upper = exact_upper(hi, "hi")?;
    interval_outcome(Interval::new(lower, upper)?)
}

fn invoke_add(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let a = interval_arg(args, "a")?;
    let b = interval_arg(args, "b")?;
    interval_outcome(add(&a, &b)?)
}

fn invoke_sub(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let a = interval_arg(args, "a")?;
    let b = interval_arg(args, "b")?;
    interval_outcome(sub(&a, &b)?)
}

fn invoke_mul(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let a = interval_arg(args, "a")?;
    let b = interval_arg(args, "b")?;
    interval_outcome(mul(&a, &b)?)
}

fn invoke_div(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let a = interval_arg(args, "a")?;
    let b = interval_arg(args, "b")?;
    interval_outcome(div(&a, &b)?)
}

fn invoke_neg(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let a = interval_arg(args, "a")?;
    interval_outcome(neg(&a))
}

fn invoke_abs(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let a = interval_arg(args, "a")?;
    interval_outcome(abs(&a))
}

fn invoke_pow_int(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let base = interval_arg(args, "base")?;
    let exponent = args.integer("exponent")?;
    let exponent = exponent
        .to_i64()
        .ok_or_else(|| EngineError::resource("exponent is out of the supported range"))?;
    check_exponent(exponent, ctx)?;
    interval_outcome(pow_int(&base, exponent)?)
}

fn invoke_sqrt(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let a = interval_arg(args, "a")?;
    interval_outcome(sqrt(&a)?)
}

fn invoke_exp(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let a = interval_arg(args, "a")?;
    interval_outcome(exp(&a)?)
}

fn invoke_ln(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let a = interval_arg(args, "a")?;
    interval_outcome(ln(&a)?)
}

fn invoke_sin(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let a = interval_arg(args, "a")?;
    interval_outcome(sin(&a)?)
}

fn invoke_cos(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let a = interval_arg(args, "a")?;
    interval_outcome(cos(&a)?)
}

fn invoke_tan(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let a = interval_arg(args, "a")?;
    interval_outcome(tan(&a)?)
}

fn invoke_intersect(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let a = interval_arg(args, "a")?;
    let b = interval_arg(args, "b")?;
    interval_outcome(intersect(&a, &b))
}

fn invoke_hull(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let a = interval_arg(args, "a")?;
    let b = interval_arg(args, "b")?;
    interval_outcome(hull(&a, &b))
}

fn invoke_contains(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let a = interval_arg(args, "a")?;
    let b = interval_arg(args, "b")?;
    let contained = if b.is_empty() {
        true
    } else if a.is_empty() {
        false
    } else {
        a.lo <= b.lo && b.hi <= a.hi
    };
    Ok(Outcome::exact(Value::Bool(contained)))
}

fn invoke_midpoint(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let a = interval_arg(args, "a")?;
    let value = match a.midpoint() {
        Some(midpoint) => Value::Number(Number::float(midpoint)?),
        None => Value::Null,
    };
    Ok(scalar_outcome(value))
}

fn invoke_width(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let a = interval_arg(args, "a")?;
    let width = a.width();
    let value = if width.is_finite() {
        Value::Number(Number::float(width)?)
    } else {
        Value::Null
    };
    Ok(scalar_outcome(value))
}

fn invoke_is_empty(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let a = interval_arg(args, "a")?;
    Ok(Outcome::exact(Value::Bool(a.is_empty())))
}

fn invoke_contains_value(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let a = interval_arg(args, "a")?;
    let value = args.number("value")?;
    let contained = if a.is_empty() {
        false
    } else {
        let lo = Number::Float64(Float64::new(a.lo)?);
        let hi = Number::Float64(Float64::new(a.hi)?);
        lo.compare(value)? != Ordering::Greater && value.compare(&hi)? != Ordering::Greater
    };
    Ok(Outcome::exact(Value::Bool(contained)))
}

fn invoke_evaluate(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let source = args.text("expression")?;
    let variables = variables_arg(args)?;
    interval_outcome(evaluate_source(source, &variables, ctx)?)
}

// ---------------------------------------------------------------------------
// Descriptors
// ---------------------------------------------------------------------------

fn enclose_descriptor() -> FunctionDescriptor {
    let expected =
        example_interval_value(enclose_number(&parse_number("0.1")).expect("0.1 must enclose"));
    base_descriptor(
        "interval.enclose",
        "Enclose a number",
        "Narrowest padded interval containing an exact number (heuristic, not certified).",
        vec![num_param("value", "Exact number to enclose.")],
    )
    .with_description(
        "Integers and exactly representable binary64 values enclose to a point interval. An \
         exact decimal or rational that is not exactly representable is converted to the \
         nearest binary64 and widened outward by two ulps, so the exact real value is \
         contained under the documented binary64 conversion assumption.",
    )
    .with_output(interval_schema(), "Certified enclosure of the input.")
    .with_examples(vec![
        Example::new(
            "enclose 0.1",
            example_args(&[("value", serde_json::json!("0.1"))]),
        )
        .with_value(expected),
        Example::new(
            "enclose an integer",
            example_args(&[("value", serde_json::json!(2))]),
        )
        .with_value(example_interval_value(Interval::point(2.0))),
    ])
}

fn from_bounds_descriptor() -> FunctionDescriptor {
    base_descriptor(
        "interval.from_bounds",
        "Interval from bounds",
        "Build a padded interval from exact lower and upper bounds (heuristic, not certified).",
        vec![
            num_param("lo", "Exact lower bound."),
            num_param("hi", "Exact upper bound; must not be less than lo."),
        ],
    )
    .with_description(
        "The exact real bounds are converted to finite binary64 endpoints and widened outward \
         when a bound is not exactly representable. A lower bound greater than the upper bound \
         is a domain error.",
    )
    .with_output(interval_schema(), "Certified interval [lo, hi].")
    .with_examples(vec![
        Example::new(
            "unit interval",
            example_args(&[("lo", serde_json::json!(1)), ("hi", serde_json::json!(2))]),
        )
        .with_value(example_interval_value(Interval { lo: 1.0, hi: 2.0 })),
        Example::new(
            "inverted bounds",
            example_args(&[("lo", serde_json::json!(2)), ("hi", serde_json::json!(1))]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn binary_descriptor(
    id: &str,
    title: &str,
    summary: &str,
    description: &str,
    examples: Vec<Example>,
) -> FunctionDescriptor {
    base_descriptor(
        id,
        title,
        summary,
        vec![
            iv_param("a", "Left operand interval."),
            iv_param("b", "Right operand interval."),
        ],
    )
    .with_description(description)
    .with_output(interval_schema(), "Certified enclosure of the result.")
    .with_examples(examples)
}

fn add_descriptor() -> FunctionDescriptor {
    binary_descriptor(
        "interval.add",
        "Add intervals",
        "Minkowski sum of two intervals.",
        "Computes [a.lo + b.lo, a.hi + b.hi] in round-to-nearest binary64 and widens each \
         endpoint outward, so the result contains every exact sum a + b.",
        vec![
            Example::new(
                "add point intervals",
                example_args(&[
                    ("a", serde_json::json!({"lo": 1, "hi": 2})),
                    ("b", serde_json::json!({"lo": 3, "hi": 4})),
                ]),
            )
            .with_value(example_interval_value(
                add(
                    &Interval { lo: 1.0, hi: 2.0 },
                    &Interval { lo: 3.0, hi: 4.0 },
                )
                .expect("sum"),
            )),
        ],
    )
}

fn sub_descriptor() -> FunctionDescriptor {
    binary_descriptor(
        "interval.sub",
        "Subtract intervals",
        "Difference of two intervals.",
        "Computes [a.lo - b.hi, a.hi - b.lo] in round-to-nearest binary64 and widens each \
         endpoint outward, so the result contains every exact difference a - b.",
        vec![
            Example::new(
                "subtract point intervals",
                example_args(&[
                    ("a", serde_json::json!({"lo": 1, "hi": 2})),
                    ("b", serde_json::json!({"lo": 3, "hi": 4})),
                ]),
            )
            .with_value(example_interval_value(
                sub(
                    &Interval { lo: 1.0, hi: 2.0 },
                    &Interval { lo: 3.0, hi: 4.0 },
                )
                .expect("difference"),
            )),
        ],
    )
}

fn mul_descriptor() -> FunctionDescriptor {
    binary_descriptor(
        "interval.mul",
        "Multiply intervals",
        "Range of the product of two intervals.",
        "Evaluates all four endpoint products, takes the minimum and maximum, and widens the \
         result outward, so it contains every exact product a * b.",
        vec![
            Example::new(
                "multiply point intervals",
                example_args(&[
                    ("a", serde_json::json!({"lo": 1, "hi": 2})),
                    ("b", serde_json::json!({"lo": 3, "hi": 4})),
                ]),
            )
            .with_value(example_interval_value(
                mul(
                    &Interval { lo: 1.0, hi: 2.0 },
                    &Interval { lo: 3.0, hi: 4.0 },
                )
                .expect("product"),
            )),
        ],
    )
}

fn div_descriptor() -> FunctionDescriptor {
    binary_descriptor(
        "interval.div",
        "Divide intervals",
        "Range of the quotient of two intervals.",
        "Evaluates all four endpoint quotients, takes the minimum and maximum, and widens the \
         result outward. The divisor must not contain zero; otherwise the result is a \
         DomainViolation.",
        vec![
            Example::new(
                "divide point intervals",
                example_args(&[
                    ("a", serde_json::json!({"lo": 1, "hi": 2})),
                    ("b", serde_json::json!({"lo": 3, "hi": 4})),
                ]),
            )
            .with_value(example_interval_value(
                div(
                    &Interval { lo: 1.0, hi: 2.0 },
                    &Interval { lo: 3.0, hi: 4.0 },
                )
                .expect("quotient"),
            )),
            Example::new(
                "divisor containing zero",
                example_args(&[
                    ("a", serde_json::json!({"lo": 1, "hi": 2})),
                    ("b", serde_json::json!({"lo": -1, "hi": 1})),
                ]),
            )
            .with_error(ErrorCode::DomainViolation),
        ],
    )
}

fn neg_descriptor() -> FunctionDescriptor {
    base_descriptor(
        "interval.neg",
        "Negate an interval",
        "Sign change of an interval.",
        vec![iv_param("a", "Interval to negate.")],
    )
    .with_description(
        "Maps [lo, hi] to [-hi, -lo]. Negation is exact in binary64, so no ulp padding is \
         introduced.",
    )
    .with_output(interval_schema(), "Certified enclosure of -a.")
    .with_examples(vec![
        Example::new(
            "negate an interval",
            example_args(&[("a", serde_json::json!({"lo": 1, "hi": 2}))]),
        )
        .with_value(example_interval_value(neg(&Interval { lo: 1.0, hi: 2.0 }))),
    ])
}

fn abs_descriptor() -> FunctionDescriptor {
    base_descriptor(
        "interval.abs",
        "Absolute value of an interval",
        "Range of the absolute value over an interval.",
        vec![iv_param("a", "Interval to take the absolute value of.")],
    )
    .with_description(
        "Exact piecewise handling: a non-negative interval is unchanged, a non-positive \
         interval is reflected, and a zero-straddling interval maps to [0, max(-lo, hi)].",
    )
    .with_output(interval_schema(), "Certified enclosure of |a|.")
    .with_examples(vec![
        Example::new(
            "interval straddling zero",
            example_args(&[("a", serde_json::json!({"lo": -1, "hi": 2}))]),
        )
        .with_value(example_interval_value(abs(&Interval { lo: -1.0, hi: 2.0 }))),
    ])
}

fn pow_int_descriptor() -> FunctionDescriptor {
    base_descriptor(
        "interval.pow_int",
        "Integer power of an interval",
        "Range of an interval raised to an integer power.",
        vec![
            iv_param("base", "Base interval."),
            int_param("exponent", "Integer exponent."),
        ],
    )
    .with_description(
        "Uses interval exponentiation by squaring with outward widening at every multiply. \
         Even powers clamp the lower bound at zero. Negative exponents compute the reciprocal \
         of the positive power and require a base that excludes zero. The exponent magnitude \
         is bounded by the configured exponent limit.",
    )
    .with_output(interval_schema(), "Certified enclosure of base^exponent.")
    .with_cost(CostClass::Iterative)
    .with_examples(vec![
        Example::new(
            "square an interval",
            example_args(&[
                ("base", serde_json::json!({"lo": -2, "hi": 3})),
                ("exponent", serde_json::json!(2)),
            ]),
        )
        .with_value(example_interval_value(
            pow_int(&Interval { lo: -2.0, hi: 3.0 }, 2).expect("power"),
        )),
        Example::new(
            "negative power of a zero-containing interval",
            example_args(&[
                ("base", serde_json::json!({"lo": 0, "hi": 1})),
                ("exponent", serde_json::json!(-1)),
            ]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn sqrt_descriptor() -> FunctionDescriptor {
    base_descriptor(
        "interval.sqrt",
        "Square root of an interval",
        "Range of the square root over an interval.",
        vec![iv_param("a", "Interval to take the square root of.")],
    )
    .with_description(
        "Defined on the non-negative part of the interval: a lower endpoint below zero is \
         clamped to zero. A strictly negative interval is a domain error.",
    )
    .with_output(interval_schema(), "Certified enclosure of sqrt(a).")
    .with_cost(CostClass::Constant)
    .with_examples(vec![
        Example::new(
            "square root of a positive interval",
            example_args(&[("a", serde_json::json!({"lo": 4, "hi": 9}))]),
        )
        .with_value(example_interval_value(
            sqrt(&Interval { lo: 4.0, hi: 9.0 }).expect("root"),
        )),
        Example::new(
            "strictly negative interval",
            example_args(&[("a", serde_json::json!({"lo": -4, "hi": -1}))]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn exp_descriptor() -> FunctionDescriptor {
    base_descriptor(
        "interval.exp",
        "Exponential of an interval",
        "Range of the natural exponential over an interval.",
        vec![iv_param("a", "Interval to exponentiate.")],
    )
    .with_description(
        "The exponential is monotone increasing, so the endpoints map directly. A result \
         beyond the finite binary64 range is a domain error.",
    )
    .with_output(interval_schema(), "Certified enclosure of exp(a).")
    .with_cost(CostClass::Constant)
    .with_examples(vec![
        Example::new(
            "exponential of a unit interval",
            example_args(&[("a", serde_json::json!({"lo": 0, "hi": 1}))]),
        )
        .with_value(example_interval_value(
            exp(&Interval { lo: 0.0, hi: 1.0 }).expect("exponential"),
        )),
    ])
}

fn ln_descriptor() -> FunctionDescriptor {
    base_descriptor(
        "interval.ln",
        "Natural logarithm of an interval",
        "Range of the natural logarithm over an interval.",
        vec![iv_param("a", "Interval to take the logarithm of.")],
    )
    .with_description(
        "The natural logarithm is monotone increasing on the positive reals. An interval \
         whose lower bound is not strictly positive, including any interval containing zero, \
         is a domain error.",
    )
    .with_output(interval_schema(), "Certified enclosure of ln(a).")
    .with_cost(CostClass::Constant)
    .with_examples(vec![
        Example::new(
            "logarithm of a positive interval",
            example_args(&[("a", serde_json::json!({"lo": 1, "hi": 2}))]),
        )
        .with_value(example_interval_value(
            ln(&Interval { lo: 1.0, hi: 2.0 }).expect("logarithm"),
        )),
        Example::new(
            "interval containing zero",
            example_args(&[("a", serde_json::json!({"lo": 0, "hi": 1}))]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn trig_descriptor(
    id: &str,
    title: &str,
    summary: &str,
    description: &str,
    expected: Interval,
    error_example: Option<(&str, serde_json::Value)>,
) -> FunctionDescriptor {
    let mut examples = vec![
        Example::new(
            "unit interval",
            example_args(&[("a", serde_json::json!({"lo": 0, "hi": 1}))]),
        )
        .with_value(example_interval_value(expected)),
    ];
    if let Some((title, interval)) = error_example {
        examples.push(
            Example::new(title, example_args(&[("a", interval)]))
                .with_error(ErrorCode::DomainViolation),
        );
    }
    base_descriptor(
        id,
        title,
        summary,
        vec![iv_param("a", "Angle interval in radians.")],
    )
    .with_description(description)
    .with_output(
        interval_schema(),
        "Certified enclosure of the trigonometric range.",
    )
    .with_cost(CostClass::Constant)
    .with_examples(examples)
}

fn sin_descriptor() -> FunctionDescriptor {
    trig_descriptor(
        "interval.sin",
        "Sine of an interval",
        "Range of the sine over an angle interval.",
        "The range includes every critical point inside the interval and returns [-1, 1] for \
         intervals wider than one period. Arguments with magnitude above 2^53 are handled \
         conservatively.",
        sin(&Interval { lo: 0.0, hi: 1.0 }).expect("sine"),
        None,
    )
}

fn cos_descriptor() -> FunctionDescriptor {
    trig_descriptor(
        "interval.cos",
        "Cosine of an interval",
        "Range of the cosine over an angle interval.",
        "The range includes every critical point inside the interval and returns [-1, 1] for \
         intervals wider than one period. Arguments with magnitude above 2^53 are handled \
         conservatively.",
        cos(&Interval { lo: 0.0, hi: 1.0 }).expect("cosine"),
        None,
    )
}

fn tan_descriptor() -> FunctionDescriptor {
    trig_descriptor(
        "interval.tan",
        "Tangent of an interval",
        "Range of the tangent over an angle interval.",
        "The tangent is increasing on each branch between poles. An interval that crosses a \
         pole pi/2 + k*pi, or whose argument is too large to exclude poles by argument reduction, is a \
         domain error.",
        tan(&Interval { lo: 0.0, hi: 1.0 }).expect("tangent"),
        Some((
            "interval crossing pi/2",
            serde_json::json!({"lo": 1.5, "hi": 1.6}),
        )),
    )
}

fn intersect_descriptor() -> FunctionDescriptor {
    binary_descriptor(
        "interval.intersect",
        "Intersect intervals",
        "Set intersection of two intervals.",
        "Disjoint intervals produce the canonical empty interval, whose record has null lo and \
         hi endpoints and zero width.",
        vec![
            Example::new(
                "overlapping intervals",
                example_args(&[
                    ("a", serde_json::json!({"lo": 0, "hi": 2})),
                    ("b", serde_json::json!({"lo": 1, "hi": 3})),
                ]),
            )
            .with_value(example_interval_value(intersect(
                &Interval { lo: 0.0, hi: 2.0 },
                &Interval { lo: 1.0, hi: 3.0 },
            ))),
            Example::new(
                "disjoint intervals",
                example_args(&[
                    ("a", serde_json::json!({"lo": 0, "hi": 1})),
                    ("b", serde_json::json!({"lo": 2, "hi": 3})),
                ]),
            )
            .with_value(example_interval_value(Interval::EMPTY)),
        ],
    )
}

fn hull_descriptor() -> FunctionDescriptor {
    binary_descriptor(
        "interval.hull",
        "Hull of two intervals",
        "Smallest interval containing both operands.",
        "Empty operands are ignored; the hull of two empty intervals is empty.",
        vec![
            Example::new(
                "hull of disjoint intervals",
                example_args(&[
                    ("a", serde_json::json!({"lo": 0, "hi": 1})),
                    ("b", serde_json::json!({"lo": 2, "hi": 3})),
                ]),
            )
            .with_value(example_interval_value(hull(
                &Interval { lo: 0.0, hi: 1.0 },
                &Interval { lo: 2.0, hi: 3.0 },
            ))),
        ],
    )
}

fn contains_descriptor() -> FunctionDescriptor {
    base_descriptor(
        "interval.contains",
        "Interval containment",
        "Whether one interval contains another.",
        vec![
            iv_param("a", "Outer interval."),
            iv_param("b", "Inner interval."),
        ],
    )
    .with_description(
        "True when every point of b lies inside a. The empty interval is contained in every \
         interval.",
    )
    .with_output(ValueSchema::Bool, "True when b is a subset of a.")
    .with_examples(vec![
        Example::new(
            "inner interval is contained",
            example_args(&[
                ("a", serde_json::json!({"lo": 0, "hi": 2})),
                ("b", serde_json::json!({"lo": 1, "hi": 1.5})),
            ]),
        )
        .with_value(Value::Bool(true)),
    ])
}

fn midpoint_descriptor() -> FunctionDescriptor {
    base_descriptor(
        "interval.midpoint",
        "Midpoint of an interval",
        "Approximate midpoint of an interval.",
        vec![iv_param("a", "Interval whose midpoint is requested.")],
    )
    .with_description(
        "Computed as lo/2 + hi/2 to avoid overflow. The result is a point estimate derived \
         from the padded endpoints and is labelled approximate; the empty interval has no \
         midpoint and returns null.",
    )
    .with_output(
        ValueSchema::Any,
        "Approximate midpoint, or null for the empty interval.",
    )
    .with_examples(vec![
        Example::new(
            "midpoint of [1, 2]",
            example_args(&[("a", serde_json::json!({"lo": 1, "hi": 2}))]),
        )
        .with_value(Value::Number(Number::float(1.5).expect("finite"))),
    ])
}

fn width_descriptor() -> FunctionDescriptor {
    base_descriptor(
        "interval.width",
        "Width of an interval",
        "Approximate width of an interval.",
        vec![iv_param("a", "Interval whose width is requested.")],
    )
    .with_description(
        "Computed as hi - lo and labelled approximate. The empty interval has width zero; a \
         width beyond the finite binary64 range is reported as null.",
    )
    .with_output(
        ValueSchema::Any,
        "Approximate width, or null when it exceeds binary64.",
    )
    .with_examples(vec![
        Example::new(
            "width of [1, 2]",
            example_args(&[("a", serde_json::json!({"lo": 1, "hi": 2}))]),
        )
        .with_value(Value::Number(Number::float(1.0).expect("finite"))),
    ])
}

fn is_empty_descriptor() -> FunctionDescriptor {
    base_descriptor(
        "interval.is_empty",
        "Empty interval test",
        "Whether an interval is the canonical empty interval.",
        vec![iv_param("a", "Interval to test.")],
    )
    .with_description(
        "The canonical empty interval is written as {\"lo\": null, \"hi\": null}; it is \
         produced by intersecting disjoint intervals.",
    )
    .with_output(ValueSchema::Bool, "True for the empty interval.")
    .with_examples(vec![
        Example::new(
            "empty interval",
            example_args(&[("a", serde_json::json!({"lo": null, "hi": null}))]),
        )
        .with_value(Value::Bool(true)),
        Example::new(
            "non-empty interval",
            example_args(&[("a", serde_json::json!({"lo": 0, "hi": 1}))]),
        )
        .with_value(Value::Bool(false)),
    ])
}

fn contains_value_descriptor() -> FunctionDescriptor {
    base_descriptor(
        "interval.contains_value",
        "Value containment",
        "Whether an exact value lies inside an interval.",
        vec![
            iv_param("a", "Interval to test."),
            num_param("value", "Exact value to test for containment."),
        ],
    )
    .with_description(
        "The comparison is exact: the binary64 endpoints are decomposed to their exact binary \
         rational values, so a decimal such as 0.1 is tested against its exact real value.",
    )
    .with_output(
        ValueSchema::Bool,
        "True when the value lies in the closed interval.",
    )
    .with_examples(vec![
        Example::new(
            "value inside",
            example_args(&[
                ("a", serde_json::json!({"lo": 0, "hi": 2})),
                ("value", serde_json::json!(1)),
            ]),
        )
        .with_value(Value::Bool(true)),
        Example::new(
            "value outside",
            example_args(&[
                ("a", serde_json::json!({"lo": 0, "hi": 2})),
                ("value", serde_json::json!(3)),
            ]),
        )
        .with_value(Value::Bool(false)),
    ])
}

fn evaluate_descriptor() -> FunctionDescriptor {
    let expected = example_interval_value(evaluate_for_example(
        "x^2 - 2",
        &[("x", Interval { lo: 1.0, hi: 2.0 })],
    ));
    base_descriptor(
        "interval.evaluate",
        "Evaluate an expression over intervals",
        "Evaluate a restricted expression with interval variables.",
        vec![
            ParamDescriptor::required(
                "expression",
                "Restricted expression using interval variables.",
                ValueSchema::Expression {
                    variables: Vec::new(),
                },
            ),
            ParamDescriptor::required(
                "variables",
                "Record mapping each free variable to an interval.",
                ValueSchema::Record {
                    fields: Vec::new(),
                    allow_extra: true,
                },
            )
            .with_shorthand(true),
        ],
    )
    .with_description(
        "Parses the expression with bicmath_core::expr::parse_expression and evaluates it with \
         a local interval evaluator supporting + - * / % ^, unary sign, identifiers, and calls \
         to sin, cos, tan, exp, ln, log10, log2, sqrt, abs, pow, min, max, floor, and ceil. \
         Unknown variables produce NotFound and unknown functions produce UnknownFunction. \
         The configured operation budget, AST depth, and cancellation checks are enforced.",
    )
    .with_output(
        interval_schema(),
        "Certified enclosure of the expression range.",
    )
    .with_cost(CostClass::Iterative)
    .with_examples(vec![
        Example::new(
            "quadratic range contains the square root of two",
            example_args(&[
                ("expression", serde_json::json!("x^2 - 2")),
                ("variables", serde_json::json!({"x": {"lo": 1, "hi": 2}})),
            ]),
        )
        .with_value(expected),
        Example::new(
            "unknown function",
            example_args(&[
                ("expression", serde_json::json!("nope(x)")),
                ("variables", serde_json::json!({"x": {"lo": 1, "hi": 2}})),
            ]),
        )
        .with_error(ErrorCode::UnknownFunction),
    ])
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

/// Build the interval module with all of its registered functions.
pub fn module() -> Module {
    let functions: Vec<Arc<dyn Function>> = vec![
        SimpleFunction::arc(enclose_descriptor(), invoke_enclose),
        SimpleFunction::arc(from_bounds_descriptor(), invoke_from_bounds),
        SimpleFunction::arc(add_descriptor(), invoke_add),
        SimpleFunction::arc(sub_descriptor(), invoke_sub),
        SimpleFunction::arc(mul_descriptor(), invoke_mul),
        SimpleFunction::arc(div_descriptor(), invoke_div),
        SimpleFunction::arc(neg_descriptor(), invoke_neg),
        SimpleFunction::arc(abs_descriptor(), invoke_abs),
        SimpleFunction::arc(pow_int_descriptor(), invoke_pow_int),
        SimpleFunction::arc(sqrt_descriptor(), invoke_sqrt),
        SimpleFunction::arc(exp_descriptor(), invoke_exp),
        SimpleFunction::arc(ln_descriptor(), invoke_ln),
        SimpleFunction::arc(sin_descriptor(), invoke_sin),
        SimpleFunction::arc(cos_descriptor(), invoke_cos),
        SimpleFunction::arc(tan_descriptor(), invoke_tan),
        SimpleFunction::arc(intersect_descriptor(), invoke_intersect),
        SimpleFunction::arc(hull_descriptor(), invoke_hull),
        SimpleFunction::arc(contains_descriptor(), invoke_contains),
        SimpleFunction::arc(midpoint_descriptor(), invoke_midpoint),
        SimpleFunction::arc(width_descriptor(), invoke_width),
        SimpleFunction::arc(is_empty_descriptor(), invoke_is_empty),
        SimpleFunction::arc(contains_value_descriptor(), invoke_contains_value),
        SimpleFunction::arc(evaluate_descriptor(), invoke_evaluate),
    ];
    let descriptor = ModuleDescriptor::new(
        "interval",
        "Interval",
        "1.0.0",
        "Heuristically padded binary64 interval arithmetic with outward rounding; not a certified bound.",
    )
    .with_capabilities(vec![
        "heuristic_padding_enclosure",
        "outward_rounding",
        "interval_expression_evaluation",
        "domain_checking",
    ])
    .with_dependencies(vec!["core"])
    .with_modes(all_modes())
    .with_source("crates/bicmath-interval");
    Module::new(descriptor, functions)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use bicmath_core::contract::ExampleExpectation;
    use bicmath_core::envelope::Exactness;
    use num_rational::BigRational;

    fn ctx() -> ExecContext {
        ExecContext::conservative()
    }

    fn call(id: &str, raw: serde_json::Value) -> Result<Outcome, EngineError> {
        let module = module();
        let function = module
            .functions
            .iter()
            .find(|function| function.descriptor().id == id)
            .expect("function exists");
        let args_json = raw.as_object().expect("object args");
        let mut values = BTreeMap::new();
        for (name, value) in args_json {
            let param = function
                .descriptor()
                .parameter(name)
                .expect("parameter exists");
            values.insert(
                name.clone(),
                param
                    .schema
                    .coerce(value, name, &ctx().limits, param.numeric_shorthand)
                    .expect("argument coerces"),
            );
        }
        function.invoke(&Args::new(values), &ctx())
    }

    fn outcome_interval(outcome: &Outcome) -> Interval {
        interval_from_value(&outcome.value, "result").expect("outcome is an interval record")
    }

    fn float_rational(value: f64) -> BigRational {
        Float64::new(value)
            .expect("finite float")
            .to_rational_exact()
            .expect("rational")
    }

    fn interval_contains_rational(interval: &Interval, value: &BigRational) -> bool {
        if interval.is_empty() {
            return false;
        }
        float_rational(interval.lo) <= *value && *value <= float_rational(interval.hi)
    }

    fn rational(numerator: i64, denominator: i64) -> BigRational {
        BigRational::new(BigInt::from(numerator), BigInt::from(denominator))
    }

    #[test]
    fn module_metadata_matches_contract() {
        let module = module();
        assert_eq!(module.descriptor.id, "interval");
        assert_eq!(module.descriptor.version, "1.0.0");
        let ids: Vec<&str> = module
            .functions
            .iter()
            .map(|function| function.descriptor().id.as_str())
            .collect();
        assert_eq!(
            ids,
            vec![
                "interval.enclose",
                "interval.from_bounds",
                "interval.add",
                "interval.sub",
                "interval.mul",
                "interval.div",
                "interval.neg",
                "interval.abs",
                "interval.pow_int",
                "interval.sqrt",
                "interval.exp",
                "interval.ln",
                "interval.sin",
                "interval.cos",
                "interval.tan",
                "interval.intersect",
                "interval.hull",
                "interval.contains",
                "interval.midpoint",
                "interval.width",
                "interval.is_empty",
                "interval.contains_value",
                "interval.evaluate",
            ]
        );
    }

    #[test]
    fn enclose_contains_the_exact_decimal() {
        let outcome = call("interval.enclose", serde_json::json!({"value": "0.1"})).unwrap();
        assert_eq!(outcome.exactness, Exactness::Approximate);
        let interval = outcome_interval(&outcome);
        assert!(interval_contains_rational(&interval, &rational(1, 10)));
        assert!(interval.lo <= 0.1 && interval.hi >= 0.1);
    }

    #[test]
    fn sum_of_thirds_contains_one_half() {
        let one_third = call(
            "interval.enclose",
            serde_json::json!({"value": {"kind": "rational", "numerator": "1", "denominator": "3"}}),
        )
        .unwrap();
        let one_sixth = call(
            "interval.enclose",
            serde_json::json!({"value": {"kind": "rational", "numerator": "1", "denominator": "6"}}),
        )
        .unwrap();
        let sum = add(&outcome_interval(&one_third), &outcome_interval(&one_sixth)).unwrap();
        assert!(interval_contains_rational(&sum, &rational(1, 2)));
    }

    #[test]
    fn transcendental_ranges_contain_sampled_values() {
        let mut samples = 0usize;
        for index in 0..1000 {
            let x = -10.0 + 20.0 * (index as f64) / 999.0;
            let point = Interval::point(x);
            let sine = sin(&point).unwrap();
            let cosine = cos(&point).unwrap();
            assert!(
                interval_contains_rational(&sine, &float_rational(f64math::sin(x))),
                "sin at {x}"
            );
            assert!(
                interval_contains_rational(&cosine, &float_rational(f64math::cos(x))),
                "cos at {x}"
            );
            samples += 1;
        }
        assert_eq!(samples, 1000);
        for index in 0..1000 {
            let x = -700.0 + 1400.0 * (index as f64) / 999.0;
            let exponential = exp(&Interval::point(x)).unwrap();
            assert!(
                interval_contains_rational(&exponential, &float_rational(f64math::exp(x))),
                "exp at {x}"
            );
            let positive = 1e-6 + (1e6 - 1e-6) * (index as f64) / 999.0;
            let logarithm = ln(&Interval::point(positive)).unwrap();
            assert!(
                interval_contains_rational(&logarithm, &float_rational(f64math::ln(positive))),
                "ln at {positive}"
            );
        }
    }

    #[test]
    fn ln_rejects_intervals_containing_zero() {
        let error = call("interval.ln", serde_json::json!({"a": {"lo": -1, "hi": 1}})).unwrap_err();
        assert_eq!(error.code, ErrorCode::DomainViolation);
        let error = call("interval.ln", serde_json::json!({"a": {"lo": 0, "hi": 1}})).unwrap_err();
        assert_eq!(error.code, ErrorCode::DomainViolation);
    }

    #[test]
    fn tan_rejects_poles() {
        let error = call(
            "interval.tan",
            serde_json::json!({"a": {"lo": 1.5, "hi": 1.6}}),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::DomainViolation);
        let error = call("interval.tan", serde_json::json!({"a": {"lo": 0, "hi": 4}})).unwrap_err();
        assert_eq!(error.code, ErrorCode::DomainViolation);
    }

    #[test]
    fn abs_straddling_zero_is_clamped() {
        let outcome = call(
            "interval.abs",
            serde_json::json!({"a": {"lo": -1, "hi": 2}}),
        )
        .unwrap();
        let interval = outcome_interval(&outcome);
        assert_eq!(interval.lo, 0.0);
        assert_eq!(interval.hi, 2.0);
    }

    #[test]
    fn evaluate_quadratic_contains_square_root_of_two() {
        let outcome = call(
            "interval.evaluate",
            serde_json::json!({
                "expression": "x^2 - 2",
                "variables": {"x": {"lo": 1, "hi": 2}}
            }),
        )
        .unwrap();
        let interval = outcome_interval(&outcome);
        let sqrt_two = float_rational(std::f64::consts::SQRT_2);
        assert!(interval_contains_rational(&interval, &sqrt_two));
        let upper_squared = float_rational(interval.hi) * float_rational(interval.hi);
        assert!(upper_squared >= BigRational::from(BigInt::from(2)));
        assert!(interval.lo <= 0.0);
    }

    #[test]
    fn div_rejects_zero_containing_divisor() {
        let error = call(
            "interval.div",
            serde_json::json!({"a": {"lo": 1, "hi": 2}, "b": {"lo": -1, "hi": 1}}),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::DomainViolation);
        let error = call(
            "interval.div",
            serde_json::json!({"a": {"lo": 1, "hi": 2}, "b": {"lo": 0, "hi": 0}}),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::DomainViolation);
    }

    #[test]
    fn from_bounds_rejects_invalid_and_non_finite_endpoints() {
        let error = call(
            "interval.from_bounds",
            serde_json::json!({"lo": 2, "hi": 1}),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::DomainViolation);
        assert!(Interval::new(f64::NAN, 1.0).is_err());
        assert!(Interval::new(0.0, f64::INFINITY).is_err());
    }

    #[test]
    fn intersect_hull_and_empty_round_trip() {
        let overlapping = call(
            "interval.intersect",
            serde_json::json!({"a": {"lo": 0, "hi": 2}, "b": {"lo": 1, "hi": 3}}),
        )
        .unwrap();
        let overlapping = outcome_interval(&overlapping);
        assert_eq!(overlapping.lo, 1.0);
        assert_eq!(overlapping.hi, 2.0);

        let disjoint = call(
            "interval.intersect",
            serde_json::json!({"a": {"lo": 0, "hi": 1}, "b": {"lo": 2, "hi": 3}}),
        )
        .unwrap();
        assert!(outcome_interval(&disjoint).is_empty());

        let empty = call(
            "interval.is_empty",
            serde_json::json!({"a": {"lo": null, "hi": null}}),
        )
        .unwrap();
        assert_eq!(empty.value, Value::Bool(true));

        let hull = call(
            "interval.hull",
            serde_json::json!({"a": {"lo": 0, "hi": 1}, "b": {"lo": 2, "hi": 3}}),
        )
        .unwrap();
        let hull = outcome_interval(&hull);
        assert_eq!(hull.lo, 0.0);
        assert_eq!(hull.hi, 3.0);
    }

    #[test]
    fn contains_and_contains_value_are_exact() {
        let inside = call(
            "interval.contains",
            serde_json::json!({"a": {"lo": 0, "hi": 2}, "b": {"lo": 1, "hi": 1.5}}),
        )
        .unwrap();
        assert_eq!(inside.value, Value::Bool(true));
        let outside = call(
            "interval.contains",
            serde_json::json!({"a": {"lo": 0, "hi": 2}, "b": {"lo": 1, "hi": 3}}),
        )
        .unwrap();
        assert_eq!(outside.value, Value::Bool(false));

        let enclosed = call("interval.enclose", serde_json::json!({"value": "0.1"})).unwrap();
        let enclosed = outcome_interval(&enclosed);
        let decimal =
            Number::Decimal(bicmath_core::number::Decimal::parse_default("0.1").expect("decimal"));
        let lo = Number::Float64(Float64::new(enclosed.lo).unwrap());
        let hi = Number::Float64(Float64::new(enclosed.hi).unwrap());
        assert!(lo.compare(&decimal).unwrap() != Ordering::Greater);
        assert!(decimal.compare(&hi).unwrap() != Ordering::Greater);
    }

    #[test]
    fn pow_int_handles_even_odd_and_negative_exponents() {
        let even = pow_int(&Interval { lo: -2.0, hi: 3.0 }, 2).unwrap();
        assert!(even.lo <= 0.0 && even.hi >= 9.0);
        let odd = pow_int(&Interval { lo: -2.0, hi: 3.0 }, 3).unwrap();
        assert!(odd.lo <= -8.0 && odd.hi >= 27.0);
        let negative = pow_int(&Interval { lo: 1.0, hi: 2.0 }, -1).unwrap();
        assert!(interval_contains_rational(&negative, &rational(1, 2)));
        assert!(negative.lo <= 0.5 && negative.hi >= 1.0);
        assert!(pow_int(&Interval { lo: 0.0, hi: 1.0 }, -1).is_err());
        let zero = pow_int(&Interval { lo: 0.0, hi: 5.0 }, 0).unwrap();
        assert_eq!(zero.lo, 1.0);
        assert_eq!(zero.hi, 1.0);
    }

    #[test]
    fn sqrt_handles_domain_and_partial_intervals() {
        let error = call(
            "interval.sqrt",
            serde_json::json!({"a": {"lo": -4, "hi": -1}}),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::DomainViolation);
        let partial = sqrt(&Interval { lo: -1.0, hi: 4.0 }).unwrap();
        assert!(partial.lo <= 0.0);
        assert!(partial.hi >= 2.0);
    }

    #[test]
    fn evaluate_reports_structured_unknown_names() {
        let unknown_function = call(
            "interval.evaluate",
            serde_json::json!({
                "expression": "nope(x)",
                "variables": {"x": {"lo": 1, "hi": 2}}
            }),
        )
        .unwrap_err();
        assert_eq!(unknown_function.code, ErrorCode::UnknownFunction);
        let unknown_variable = call(
            "interval.evaluate",
            serde_json::json!({
                "expression": "y + 1",
                "variables": {"x": {"lo": 1, "hi": 2}}
            }),
        )
        .unwrap_err();
        assert_eq!(unknown_variable.code, ErrorCode::NotFound);
    }

    #[test]
    fn evaluate_supports_percent_and_functions() {
        let outcome = call(
            "interval.evaluate",
            serde_json::json!({
                "expression": "abs(x) % 2 + sin(0) + floor(1.5)",
                "variables": {"x": {"lo": -3, "hi": 3}}
            }),
        )
        .unwrap();
        let interval = outcome_interval(&outcome);
        assert!(interval.lo <= 1.0 && interval.hi >= 3.0);
    }

    #[test]
    fn midpoint_and_width_are_reported() {
        let midpoint = call(
            "interval.midpoint",
            serde_json::json!({"a": {"lo": 1, "hi": 2}}),
        )
        .unwrap();
        assert_eq!(
            midpoint.value,
            Value::Number(Number::float(1.5).expect("finite"))
        );
        assert_eq!(midpoint.exactness, Exactness::Approximate);
        let width = call(
            "interval.width",
            serde_json::json!({"a": {"lo": 1, "hi": 2}}),
        )
        .unwrap();
        assert_eq!(
            width.value,
            Value::Number(Number::float(1.0).expect("finite"))
        );
        let empty_midpoint = call(
            "interval.midpoint",
            serde_json::json!({"a": {"lo": null, "hi": null}}),
        )
        .unwrap();
        assert_eq!(empty_midpoint.value, Value::Null);
    }

    #[test]
    fn every_documented_example_executes() {
        let module = module();
        for function in &module.functions {
            let descriptor = function.descriptor();
            assert!(
                !descriptor.examples.is_empty(),
                "{} must document at least one executable example",
                descriptor.id
            );
            assert!(
                descriptor
                    .method_ref
                    .starts_with("docs/methods/interval.md#"),
                "{} has an invalid method reference",
                descriptor.id
            );
            assert!(
                !descriptor.parameters.is_empty(),
                "{} must declare parameters",
                descriptor.id
            );
            for example in &descriptor.examples {
                let raw = serde_json::to_value(&example.arguments).expect("arguments serialize");
                let object = raw.as_object().expect("arguments are a record");
                let mut values = BTreeMap::new();
                for (name, value) in object {
                    let param = descriptor.parameter(name).expect("parameter exists");
                    let coerced = param
                        .schema
                        .coerce(value, name, &ctx().limits, param.numeric_shorthand)
                        .unwrap_or_else(|error| {
                            panic!("{} example {:?}: {error}", descriptor.id, example.title)
                        });
                    values.insert(name.clone(), coerced);
                }
                let result = function.invoke(&Args::new(values), &ctx());
                match &example.expected {
                    Some(ExampleExpectation::Value(expected)) => {
                        let outcome = result.unwrap_or_else(|error| {
                            panic!("{} example {:?}: {error}", descriptor.id, example.title)
                        });
                        assert_eq!(
                            &outcome.value, expected,
                            "{} example {:?}",
                            descriptor.id, example.title
                        );
                        if !matches!(outcome.value, Value::Bool(_)) {
                            assert_eq!(
                                outcome.exactness,
                                Exactness::Approximate,
                                "{} example {:?}",
                                descriptor.id,
                                example.title
                            );
                            assert!(
                                outcome.error_estimate.is_some(),
                                "{} example {:?} lacks an error estimate",
                                descriptor.id,
                                example.title
                            );
                            assert!(
                                !outcome.assumptions.is_empty(),
                                "{} example {:?} lacks an assumption",
                                descriptor.id,
                                example.title
                            );
                        }
                    }
                    Some(ExampleExpectation::Error(code)) => {
                        let error = result.expect_err("example must fail");
                        assert_eq!(
                            error.code, *code,
                            "{} example {:?}",
                            descriptor.id, example.title
                        );
                    }
                    Some(ExampleExpectation::Contains(needle)) => {
                        let outcome = result.expect("example must succeed");
                        let rendered = serde_json::to_string(&outcome.value).expect("serializes");
                        assert!(
                            rendered.contains(needle),
                            "{} example {:?}",
                            descriptor.id,
                            example.title
                        );
                    }
                    None => panic!(
                        "{} example {:?} has no expectation",
                        descriptor.id, example.title
                    ),
                }
            }
        }
    }
}
