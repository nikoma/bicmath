//! Numeric representations and the promotion/conversion contract.
//!
//! Four scalar representations are supported:
//!
//! | Kind | Rust type | Exactness |
//! |------|-----------|-----------|
//! | Integer | `num_bigint::BigInt` | exact |
//! | Rational | `num_rational::BigRational` (normalized, positive denominator) | exact |
//! | Decimal | [`Decimal`] (BigInt mantissa + decimal scale) | exact until a rounding context is applied |
//! | Float64 | [`Float64`] (finite IEEE-754 binary64) | approximate |
//!
//! Promotion rules (also documented in `docs/numerics.md`):
//!
//! 1. Float64 is never mixed into exact arithmetic silently. Any operation that
//!    involves a Float64 requires [`NumericMode::Scientific`]; otherwise the
//!    engine returns [`ErrorCode::UnsupportedNumericMode`].
//! 2. Integer op Integer is Integer for `+ - *` and integer powers; division is
//!    Integer when exact and Rational otherwise.
//! 3. Rational op Rational is Rational.
//! 4. Decimal op Decimal/Integer is Decimal and exact for `+ - *`.
//! 5. Decimal division (and mixed Rational/Decimal operations that cannot be
//!    represented exactly as decimals) rounds under the declared context and
//!    reports inexactness. In [`NumericMode::Exact`] an inexact operation is an
//!    error.
//! 6. Comparisons are exact across Integer/Rational/Decimal/Float64: every
//!    value is compared as a real number, with Float64 decomposed to its exact
//!    binary rational value.

use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

use num_bigint::{BigInt, Sign};
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::{One, Signed, ToPrimitive, Zero};
use serde::{Deserialize, Serialize};

use crate::error::{EngineError, ErrorCode};
use crate::limits::Limits;

/// Declared numeric mode for a calculation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NumericMode {
    /// Only exact operations are permitted. Inexact division, irrational roots,
    /// and any Float64 involvement are rejected.
    Exact,
    /// Exact where mathematically possible; decimal division rounds under the
    /// declared context and reports inexactness. Float64 is rejected.
    Auto,
    /// Float64 arithmetic is permitted and results are classified approximate.
    Scientific,
}

impl NumericMode {
    pub fn as_str(self) -> &'static str {
        match self {
            NumericMode::Exact => "exact",
            NumericMode::Auto => "auto",
            NumericMode::Scientific => "scientific",
        }
    }
}

impl fmt::Display for NumericMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Rounding modes for explicit quantization and context rounding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoundingMode {
    /// Round to nearest; ties to even last digit.
    HalfEven,
    /// Round to nearest; ties away from zero.
    HalfAwayFromZero,
    /// Truncate toward zero.
    TowardZero,
    /// Round toward negative infinity.
    Floor,
    /// Round toward positive infinity.
    Ceiling,
}

impl RoundingMode {
    pub fn as_str(self) -> &'static str {
        match self {
            RoundingMode::HalfEven => "half_even",
            RoundingMode::HalfAwayFromZero => "half_away_from_zero",
            RoundingMode::TowardZero => "toward_zero",
            RoundingMode::Floor => "floor",
            RoundingMode::Ceiling => "ceiling",
        }
    }
}

impl fmt::Display for RoundingMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Round `numerator / denominator` to an integer with the given mode.
///
/// `denominator` must be non-zero; the sign of the quotient is derived from both
/// operands, and rounding is defined on the real value of the fraction.
pub fn div_round(numerator: &BigInt, denominator: &BigInt, mode: RoundingMode) -> BigInt {
    debug_assert!(!denominator.is_zero());
    let (q, r) = numerator.div_rem(denominator);
    if r.is_zero() {
        return q;
    }
    let negative = (numerator.sign() == Sign::Minus) ^ (denominator.sign() == Sign::Minus);
    match mode {
        RoundingMode::TowardZero => q,
        RoundingMode::Floor => {
            if negative {
                q - 1
            } else {
                q
            }
        }
        RoundingMode::Ceiling => {
            if negative {
                q
            } else {
                q + 1
            }
        }
        RoundingMode::HalfAwayFromZero | RoundingMode::HalfEven => {
            let twice_remainder = r.abs() * 2u32;
            let divisor = denominator.abs();
            match twice_remainder.cmp(&divisor) {
                Ordering::Less => q,
                Ordering::Greater => {
                    if negative {
                        q - 1
                    } else {
                        q + 1
                    }
                }
                Ordering::Equal => match mode {
                    RoundingMode::HalfAwayFromZero => {
                        if negative {
                            q - 1
                        } else {
                            q + 1
                        }
                    }
                    RoundingMode::HalfEven => {
                        if (&q % 2u32).is_zero() {
                            q
                        } else if negative {
                            q - 1
                        } else {
                            q + 1
                        }
                    }
                    _ => unreachable!("outer match restricts mode"),
                },
            }
        }
    }
}

/// 10^n as a `BigInt`.
fn pow10(n: u64) -> BigInt {
    debug_assert!(n <= 10_000_000, "pow10 exponent out of supported range");
    BigInt::from(10u32).pow(n as u32)
}

/// An exact decimal: `mantissa * 10^-scale`.
///
/// The scale is preserved as given so that `0.10` remains distinguishable from
/// `0.1` for presentation and fingerprinting, while comparisons are numeric.
#[derive(Clone, Debug)]
pub struct Decimal {
    mantissa: BigInt,
    scale: u32,
}

impl Decimal {
    pub fn from_parts(mantissa: BigInt, scale: u32) -> Decimal {
        Decimal { mantissa, scale }
    }

    pub fn zero() -> Decimal {
        Decimal {
            mantissa: BigInt::zero(),
            scale: 0,
        }
    }

    pub fn from_bigint(value: BigInt) -> Decimal {
        Decimal {
            mantissa: value,
            scale: 0,
        }
    }

    pub fn mantissa(&self) -> &BigInt {
        &self.mantissa
    }

    pub fn scale(&self) -> u32 {
        self.scale
    }

    /// Parse a locale-independent decimal literal.
    ///
    /// Grammar: `[+-]? ( digits ('.' digits?)? | '.' digits ) ([eE] [+-]? digits)?`
    /// No whitespace, no thousands separators, no locale decimal comma.
    pub fn parse(source: &str, limits: &Limits) -> Result<Decimal, EngineError> {
        let (mantissa, scale) = parse_decimal_parts(source, limits)?;
        Ok(Decimal { mantissa, scale })
    }

    /// Parse using conservative default limits. Intended for tests and
    /// constants; public entry points must pass explicit limits.
    pub fn parse_default(source: &str) -> Result<Decimal, EngineError> {
        Self::parse(source, &Limits::conservative())
    }

    pub fn is_zero(&self) -> bool {
        self.mantissa.is_zero()
    }

    pub fn is_negative(&self) -> bool {
        self.mantissa.sign() == Sign::Minus
    }

    pub fn is_positive(&self) -> bool {
        self.mantissa.sign() == Sign::Plus
    }

    pub fn sign(&self) -> Sign {
        self.mantissa.sign()
    }

    pub fn neg(&self) -> Decimal {
        Decimal {
            mantissa: -&self.mantissa,
            scale: self.scale,
        }
    }

    pub fn abs(&self) -> Decimal {
        Decimal {
            mantissa: self.mantissa.abs(),
            scale: self.scale,
        }
    }

    /// Strip trailing fractional zeros. `0.10` becomes `0.1`; `100` stays `100`.
    pub fn normalized(&self) -> Decimal {
        if self.mantissa.is_zero() {
            return Decimal {
                mantissa: BigInt::zero(),
                scale: 0,
            };
        }
        let mut mantissa = self.mantissa.clone();
        let mut scale = self.scale;
        while scale > 0 {
            let (q, r) = mantissa.div_rem(&pow10(1));
            if r.is_zero() {
                mantissa = q;
                scale -= 1;
            } else {
                break;
            }
        }
        Decimal { mantissa, scale }
    }

    /// Canonical string used for hashing and cross-representation equality:
    /// normalized, plain decimal notation, never exponent notation.
    pub fn canonical_string(&self) -> String {
        self.normalized().to_plain_string()
    }

    /// Display string that preserves the declared scale (`0.10` prints as `0.10`).
    pub fn to_plain_string(&self) -> String {
        let negative = self.mantissa.sign() == Sign::Minus;
        let digits = self.mantissa.abs().to_string();
        let scale = self.scale as usize;
        let mut out = String::new();
        if negative {
            out.push('-');
        }
        if scale == 0 {
            out.push_str(&digits);
        } else if digits.len() > scale {
            let split = digits.len() - scale;
            out.push_str(&digits[..split]);
            out.push('.');
            out.push_str(&digits[split..]);
        } else {
            out.push_str("0.");
            for _ in 0..(scale - digits.len()) {
                out.push('0');
            }
            out.push_str(&digits);
        }
        out
    }

    pub fn to_bigint_if_integral(&self) -> Option<BigInt> {
        let divisor = pow10(self.scale as u64);
        let (q, r) = self.mantissa.div_rem(&divisor);
        if r.is_zero() { Some(q) } else { None }
    }

    pub fn to_rational(&self) -> BigRational {
        BigRational::new(self.mantissa.clone(), pow10(self.scale as u64))
    }

    /// Approximate binary64 conversion. Returns `None` if the value overflows.
    pub fn to_f64(&self) -> Option<f64> {
        let v: f64 = self.to_plain_string().parse().ok()?;
        if v.is_finite() { Some(v) } else { None }
    }

    /// Decimal representation of the shortest round-tripping string of `value`.
    /// This is a *display* conversion, not an exact binary-to-decimal conversion.
    pub fn from_f64_display(value: f64) -> Option<Decimal> {
        if !value.is_finite() {
            return None;
        }
        let text = format!("{value}");
        Decimal::parse(&text, &Limits::conservative()).ok()
    }

    /// Exact conversion of a finite f64 to a rational (used for comparisons).
    pub fn from_f64_exact(value: f64) -> Option<BigRational> {
        float_to_rational(value)
    }

    /// Compare two decimals numerically, ignoring scale differences.
    pub fn numeric_cmp(&self, other: &Decimal) -> Ordering {
        let scale = self.scale.max(other.scale);
        let lhs = &self.mantissa * pow10((scale - self.scale) as u64);
        let rhs = &other.mantissa * pow10((scale - other.scale) as u64);
        lhs.cmp(&rhs)
    }

    fn check_bits(&self, limits: &Limits, ctx: &NumericContext) -> Result<(), EngineError> {
        let bits = self.mantissa.bits();
        let cap = limits.max_integer_bits.min(ctx.max_integer_bits) as u64;
        if bits > cap {
            return Err(EngineError::new(
                ErrorCode::ResourceLimit,
                format!(
                    "decimal result needs {bits} bits of mantissa, exceeding the limit of {cap}"
                ),
            ));
        }
        if self.scale > limits.max_decimal_scale.min(ctx.max_decimal_scale) {
            return Err(EngineError::new(
                ErrorCode::PrecisionLimit,
                format!(
                    "decimal scale {} exceeds the limit of {}",
                    self.scale,
                    limits.max_decimal_scale.min(ctx.max_decimal_scale)
                ),
            ));
        }
        Ok(())
    }

    pub fn add(
        &self,
        other: &Decimal,
        ctx: &NumericContext,
        limits: &Limits,
    ) -> Result<Decimal, EngineError> {
        let scale = self.scale.max(other.scale);
        let lhs = &self.mantissa * pow10((scale - self.scale) as u64);
        let rhs = &other.mantissa * pow10((scale - other.scale) as u64);
        let out = Decimal {
            mantissa: lhs + rhs,
            scale,
        };
        out.check_bits(limits, ctx)?;
        Ok(out)
    }

    pub fn sub(
        &self,
        other: &Decimal,
        ctx: &NumericContext,
        limits: &Limits,
    ) -> Result<Decimal, EngineError> {
        self.add(&other.neg(), ctx, limits)
    }

    pub fn mul(
        &self,
        other: &Decimal,
        ctx: &NumericContext,
        limits: &Limits,
    ) -> Result<Decimal, EngineError> {
        let out = Decimal {
            mantissa: &self.mantissa * &other.mantissa,
            scale: self.scale.saturating_add(other.scale),
        };
        out.check_bits(limits, ctx)?;
        Ok(out)
    }

    /// Divide under the declared context. Returns `(value, inexact)`.
    ///
    /// When the quotient terminates it is returned exactly with trailing zeros
    /// stripped. Otherwise the quotient is rounded to `ctx.precision`
    /// significant digits and `inexact` is `true`.
    pub fn div(
        &self,
        other: &Decimal,
        ctx: &NumericContext,
        limits: &Limits,
    ) -> Result<(Decimal, bool), EngineError> {
        if other.is_zero() {
            return Err(EngineError::division_by_zero("decimal division by zero"));
        }
        let precision = ctx.precision.max(1) as u64;
        let q_scale = precision + other.scale.saturating_sub(self.scale) as u64;
        let shift = q_scale + other.scale as u64 - self.scale as u64;
        let numerator = &self.mantissa * pow10(shift);
        let (q, r) = numerator.div_rem(&other.mantissa);
        let raw = Decimal {
            mantissa: q,
            scale: q_scale as u32,
        };
        if r.is_zero() {
            let value = raw.normalized();
            value.check_bits(limits, ctx)?;
            return Ok((value, false));
        }
        if ctx.mode == NumericMode::Exact {
            return Err(EngineError::new(
                ErrorCode::UnsupportedNumericMode,
                "division does not terminate in the selected representation; \
                 use auto or scientific mode, or an explicit rounding context",
            ));
        }
        let rounded = raw.round_significant(ctx.precision.max(1), ctx.rounding);
        rounded.check_bits(limits, ctx)?;
        Ok((rounded, true))
    }

    /// Round to a fixed number of fractional digits.
    pub fn round_to(&self, scale: u32, mode: RoundingMode) -> Decimal {
        self.round_to_scale(scale as i64, mode)
    }

    /// Round to a fixed number of significant digits.
    pub fn round_significant(&self, digits: u32, mode: RoundingMode) -> Decimal {
        if self.mantissa.is_zero() {
            return self.clone();
        }
        let nd = self.mantissa.abs().to_string().len() as i64;
        let target = self.scale as i64 - (nd - digits as i64);
        self.round_to_scale(target, mode)
    }

    /// Round to a (possibly negative) decimal scale.
    pub fn round_to_scale(&self, target_scale: i64, mode: RoundingMode) -> Decimal {
        if target_scale >= self.scale as i64 {
            let shift = target_scale as u64 - self.scale as u64;
            return Decimal {
                mantissa: &self.mantissa * pow10(shift),
                scale: target_scale as u32,
            };
        }
        let k = self.scale as u64 - target_scale as u64;
        let divisor = pow10(k);
        let q = div_round(&self.mantissa, &divisor, mode);
        if target_scale >= 0 {
            Decimal {
                mantissa: q,
                scale: target_scale as u32,
            }
        } else {
            Decimal {
                mantissa: q * pow10((-target_scale) as u64),
                scale: 0,
            }
        }
    }

    pub fn floor(&self) -> Decimal {
        self.round_to_scale(0, RoundingMode::Floor)
    }

    pub fn ceil(&self) -> Decimal {
        self.round_to_scale(0, RoundingMode::Ceiling)
    }

    pub fn trunc(&self) -> Decimal {
        self.round_to_scale(0, RoundingMode::TowardZero)
    }

    /// Exact square root when the decimal has an exact decimal square root.
    pub fn sqrt_exact(&self) -> Option<Decimal> {
        if self.mantissa.sign() == Sign::Minus {
            return None;
        }
        if self.mantissa.is_zero() {
            return Some(Decimal::zero());
        }
        let (mantissa, scale) = if self.scale.is_multiple_of(2) {
            (self.mantissa.clone(), self.scale)
        } else {
            (&self.mantissa * 10u32, self.scale + 1)
        };
        let root = mantissa.sqrt();
        if &root * &root == mantissa {
            Some(Decimal {
                mantissa: root,
                scale: scale / 2,
            })
        } else {
            None
        }
    }

    /// Integer power. Negative exponents are handled by the caller (division).
    pub fn pow_u32(
        &self,
        exponent: u32,
        ctx: &NumericContext,
        limits: &Limits,
    ) -> Result<Decimal, EngineError> {
        let predicted = self.mantissa.bits().saturating_mul(exponent as u64);
        let cap = limits.max_integer_bits.min(ctx.max_integer_bits) as u64;
        if predicted > cap {
            return Err(EngineError::new(
                ErrorCode::ResourceLimit,
                format!("power would need about {predicted} bits, exceeding the limit of {cap}"),
            ));
        }
        let out = Decimal {
            mantissa: self.mantissa.pow(exponent),
            scale: self.scale.saturating_mul(exponent),
        };
        out.check_bits(limits, ctx)?;
        Ok(out)
    }

    /// Exact conversion from a rational, or a rounded conversion when allowed.
    pub fn from_rational(
        value: &BigRational,
        ctx: &NumericContext,
        limits: &Limits,
    ) -> Result<(Decimal, bool), EngineError> {
        let numer = value.numer();
        let denom = value.denom();
        // Determine whether the rational terminates in base 10.
        let mut d = denom.clone();
        let two = BigInt::from(2u32);
        let five = BigInt::from(5u32);
        let mut twos = 0u32;
        let mut fives = 0u32;
        while d.is_even() {
            d /= &two;
            twos += 1;
        }
        while (&d % &five).is_zero() {
            d /= &five;
            fives += 1;
        }
        if d.is_one() {
            let scale = twos.max(fives);
            let mantissa = numer * pow10(scale as u64) / denom;
            let out = Decimal { mantissa, scale };
            out.check_bits(limits, ctx)?;
            return Ok((out, false));
        }
        if ctx.mode == NumericMode::Exact {
            return Err(EngineError::new(
                ErrorCode::UnsupportedNumericMode,
                "rational value has a non-terminating decimal expansion; \
                 use auto or scientific mode, or request a rational result",
            ));
        }
        let precision = ctx.precision.max(1) as u64;
        // Compute enough digits, then round to significant precision.
        let shift = precision + 4;
        let scaled = numer * pow10(shift);
        let (q, _r) = scaled.div_rem(denom);
        let raw = Decimal {
            mantissa: q,
            scale: shift as u32,
        };
        let rounded = raw.round_significant(ctx.precision.max(1), ctx.rounding);
        rounded.check_bits(limits, ctx)?;
        Ok((rounded, true))
    }
}

impl PartialEq for Decimal {
    fn eq(&self, other: &Self) -> bool {
        self.numeric_cmp(other) == Ordering::Equal
    }
}

impl Eq for Decimal {}

impl PartialOrd for Decimal {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Decimal {
    fn cmp(&self, other: &Self) -> Ordering {
        self.numeric_cmp(other)
    }
}

impl fmt::Display for Decimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_plain_string())
    }
}

/// Exact decomposition of a finite f64 into a rational number.
fn float_to_rational(value: f64) -> Option<BigRational> {
    if !value.is_finite() {
        return None;
    }
    if value == 0.0 {
        return Some(BigRational::zero());
    }
    let bits = value.to_bits();
    let negative = (bits >> 63) & 1 == 1;
    let exponent_bits = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & 0x000f_ffff_ffff_ffff;
    let (mantissa, exponent) = if exponent_bits == 0 {
        (fraction, -1074i32)
    } else {
        (fraction | (1u64 << 52), exponent_bits - 1075)
    };
    let mut numerator = BigInt::from(mantissa);
    if negative {
        numerator = -numerator;
    }
    let (numer, denom) = if exponent >= 0 {
        (numerator << exponent as u32, BigInt::one())
    } else {
        (numerator, BigInt::one() << (-exponent) as u32)
    };
    Some(BigRational::new(numer, denom))
}

/// Finite IEEE-754 binary64 wrapper. Non-finite values cannot be constructed
/// through the public API.
#[derive(Clone, Copy, Debug)]
pub struct Float64(f64);

impl Float64 {
    pub fn new(value: f64) -> Result<Float64, EngineError> {
        if value.is_finite() {
            Ok(Float64(value))
        } else {
            Err(EngineError::new(
                ErrorCode::DomainViolation,
                format!("non-finite float64 value {value} is not a valid public result"),
            ))
        }
    }

    pub fn get(self) -> f64 {
        self.0
    }

    /// Canonicalize negative zero to positive zero for hashing/equality.
    pub fn canonical(self) -> Float64 {
        if self.0 == 0.0 { Float64(0.0) } else { self }
    }

    pub fn to_rational_exact(self) -> Option<BigRational> {
        float_to_rational(self.0)
    }

    pub fn parse(source: &str) -> Result<Float64, EngineError> {
        let trimmed = source.trim();
        if trimmed.is_empty() {
            return Err(EngineError::malformed("empty float64 literal"));
        }
        let value: f64 = trimmed
            .parse()
            .map_err(|_| EngineError::malformed(format!("invalid float64 literal {source:?}")))?;
        Float64::new(value)
    }
}

impl PartialEq for Float64 {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for Float64 {}

impl Ord for Float64 {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.total_cmp(&other.0)
    }
}

impl PartialOrd for Float64 {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Float64 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Rust's default f64 formatting is shortest-round-trip.
        write!(f, "{}", self.0)
    }
}

/// The recursive scalar number type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Number {
    Integer(BigInt),
    Rational(BigRational),
    Decimal(Decimal),
    Float64(Float64),
}

/// Result of an arithmetic operation plus an explicit rounding flag.
#[derive(Clone, Debug)]
pub struct NumberResult {
    pub value: Number,
    pub rounded: bool,
}

impl NumberResult {
    pub fn exact(value: Number) -> NumberResult {
        NumberResult {
            value,
            rounded: false,
        }
    }

    pub fn rounded(value: Number) -> NumberResult {
        NumberResult {
            value,
            rounded: true,
        }
    }
}

impl Number {
    pub fn integer<T: Into<BigInt>>(value: T) -> Number {
        Number::Integer(value.into())
    }

    pub fn decimal(value: Decimal) -> Number {
        Number::Decimal(value)
    }

    pub fn rational(numer: BigInt, denom: BigInt) -> Result<Number, EngineError> {
        if denom.is_zero() {
            return Err(EngineError::division_by_zero(
                "rational with zero denominator",
            ));
        }
        Ok(Number::Rational(BigRational::new(numer, denom)))
    }

    pub fn float(value: f64) -> Result<Number, EngineError> {
        Ok(Number::Float64(Float64::new(value)?))
    }

    pub fn is_zero(&self) -> bool {
        match self {
            Number::Integer(v) => v.is_zero(),
            Number::Rational(v) => v.is_zero(),
            Number::Decimal(v) => v.is_zero(),
            Number::Float64(v) => v.0 == 0.0,
        }
    }

    pub fn is_negative(&self) -> bool {
        match self {
            Number::Integer(v) => v.sign() == Sign::Minus,
            Number::Rational(v) => v.is_negative(),
            Number::Decimal(v) => v.is_negative(),
            Number::Float64(v) => v.0 < 0.0,
        }
    }

    pub fn is_float(&self) -> bool {
        matches!(self, Number::Float64(_))
    }

    pub fn kind_name(&self) -> &'static str {
        match self {
            Number::Integer(_) => "integer",
            Number::Rational(_) => "rational",
            Number::Decimal(_) => "decimal",
            Number::Float64(_) => "float64",
        }
    }

    /// Convert to an exact rational, failing for Float64 (use
    /// [`Number::to_exact_rational`] to include floats via their exact bits).
    pub fn as_exact_rational(&self) -> Option<BigRational> {
        match self {
            Number::Integer(v) => Some(BigRational::from_integer(v.clone())),
            Number::Rational(v) => Some(v.clone()),
            Number::Decimal(v) => Some(v.to_rational()),
            Number::Float64(_) => None,
        }
    }

    /// Convert to an exact rational, including Float64 via bit decomposition.
    pub fn to_exact_rational(&self) -> Option<BigRational> {
        match self {
            Number::Float64(v) => v.to_rational_exact(),
            other => other.as_exact_rational(),
        }
    }

    pub fn to_f64(&self) -> Option<f64> {
        match self {
            Number::Integer(v) => v.to_f64(),
            Number::Rational(v) => v.to_f64(),
            Number::Decimal(v) => v.to_f64(),
            Number::Float64(v) => Some(v.0),
        }
    }

    pub fn neg(&self) -> Number {
        match self {
            Number::Integer(v) => Number::Integer(-v),
            Number::Rational(v) => Number::Rational(-v),
            Number::Decimal(v) => Number::Decimal(v.neg()),
            Number::Float64(v) => Number::Float64(Float64(-v.0)),
        }
    }

    pub fn abs(&self) -> Number {
        match self {
            Number::Integer(v) => Number::Integer(v.abs()),
            Number::Rational(v) => Number::Rational(v.abs()),
            Number::Decimal(v) => Number::Decimal(v.abs()),
            Number::Float64(v) => Number::Float64(Float64(v.0.abs())),
        }
    }

    /// Exact comparison across all representations.
    pub fn compare(&self, other: &Number) -> Result<Ordering, EngineError> {
        if let (Number::Float64(a), Number::Float64(b)) = (self, other) {
            return Ok(a.get().total_cmp(&b.get()));
        }
        let lhs = self
            .to_exact_rational()
            .ok_or_else(|| EngineError::internal("unrepresentable comparison operand"))?;
        let rhs = other
            .to_exact_rational()
            .ok_or_else(|| EngineError::internal("unrepresentable comparison operand"))?;
        Ok(lhs.cmp(&rhs))
    }

    /// Numeric equality across representations (exact semantics).
    pub fn numeric_eq(&self, other: &Number) -> Result<bool, EngineError> {
        Ok(self.compare(other)? == Ordering::Equal)
    }

    fn require_float_allowed(
        &self,
        other: &Number,
        ctx: &NumericContext,
    ) -> Result<(), EngineError> {
        if (self.is_float() || other.is_float()) && ctx.mode != NumericMode::Scientific {
            return Err(EngineError::new(
                ErrorCode::UnsupportedNumericMode,
                format!(
                    "float64 arithmetic requires scientific mode; current mode is {}",
                    ctx.mode
                ),
            ));
        }
        Ok(())
    }

    pub fn add(
        &self,
        other: &Number,
        ctx: &NumericContext,
        limits: &Limits,
    ) -> Result<NumberResult, EngineError> {
        self.require_float_allowed(other, ctx)?;
        if self.is_float() || other.is_float() {
            let a = self.to_f64().unwrap_or(f64::NAN);
            let b = other.to_f64().unwrap_or(f64::NAN);
            return Ok(NumberResult::rounded(Number::float(a + b)?));
        }
        match (self, other) {
            (Number::Integer(a), Number::Integer(b)) => {
                let out = a + b;
                check_bigint(&out, ctx, limits)?;
                Ok(NumberResult::exact(Number::Integer(out)))
            }
            (Number::Rational(a), Number::Rational(b)) => {
                let out = a + b;
                check_rational(&out, ctx, limits)?;
                Ok(NumberResult::exact(Number::Rational(out)))
            }
            (Number::Rational(a), Number::Integer(b)) => {
                let out = a + BigRational::from_integer(b.clone());
                check_rational(&out, ctx, limits)?;
                Ok(NumberResult::exact(Number::Rational(out)))
            }
            (Number::Integer(a), Number::Rational(b)) => {
                let out = BigRational::from_integer(a.clone()) + b;
                check_rational(&out, ctx, limits)?;
                Ok(NumberResult::exact(Number::Rational(out)))
            }
            (Number::Decimal(a), Number::Decimal(b)) => {
                Ok(NumberResult::exact(Number::Decimal(a.add(b, ctx, limits)?)))
            }
            (Number::Decimal(a), Number::Integer(b)) => {
                let b = Decimal::from_bigint(b.clone());
                Ok(NumberResult::exact(Number::Decimal(
                    a.add(&b, ctx, limits)?,
                )))
            }
            (Number::Integer(a), Number::Decimal(b)) => {
                let a = Decimal::from_bigint(a.clone());
                Ok(NumberResult::exact(Number::Decimal(a.add(b, ctx, limits)?)))
            }
            (Number::Decimal(a), Number::Rational(b)) => {
                let (b, inexact) = Decimal::from_rational(b, ctx, limits)?;
                Ok(NumberResult {
                    value: Number::Decimal(a.add(&b, ctx, limits)?),
                    rounded: inexact,
                })
            }
            (Number::Rational(a), Number::Decimal(b)) => {
                let (a, inexact) = Decimal::from_rational(a, ctx, limits)?;
                Ok(NumberResult {
                    value: Number::Decimal(a.add(b, ctx, limits)?),
                    rounded: inexact,
                })
            }
            (Number::Float64(_), _) | (_, Number::Float64(_)) => unreachable!("handled above"),
        }
    }

    pub fn sub(
        &self,
        other: &Number,
        ctx: &NumericContext,
        limits: &Limits,
    ) -> Result<NumberResult, EngineError> {
        self.add(&other.neg(), ctx, limits)
    }

    pub fn mul(
        &self,
        other: &Number,
        ctx: &NumericContext,
        limits: &Limits,
    ) -> Result<NumberResult, EngineError> {
        self.require_float_allowed(other, ctx)?;
        if self.is_float() || other.is_float() {
            let a = self.to_f64().unwrap_or(f64::NAN);
            let b = other.to_f64().unwrap_or(f64::NAN);
            return Ok(NumberResult::rounded(Number::float(a * b)?));
        }
        match (self, other) {
            (Number::Integer(a), Number::Integer(b)) => {
                let out = a * b;
                check_bigint(&out, ctx, limits)?;
                Ok(NumberResult::exact(Number::Integer(out)))
            }
            (Number::Rational(a), Number::Rational(b)) => {
                let out = a * b;
                check_rational(&out, ctx, limits)?;
                Ok(NumberResult::exact(Number::Rational(out)))
            }
            (Number::Rational(a), Number::Integer(b)) => {
                let out = a * BigRational::from_integer(b.clone());
                check_rational(&out, ctx, limits)?;
                Ok(NumberResult::exact(Number::Rational(out)))
            }
            (Number::Integer(a), Number::Rational(b)) => {
                let out = BigRational::from_integer(a.clone()) * b;
                check_rational(&out, ctx, limits)?;
                Ok(NumberResult::exact(Number::Rational(out)))
            }
            (Number::Decimal(a), Number::Decimal(b)) => {
                Ok(NumberResult::exact(Number::Decimal(a.mul(b, ctx, limits)?)))
            }
            (Number::Decimal(a), Number::Integer(b)) => {
                let b = Decimal::from_bigint(b.clone());
                Ok(NumberResult::exact(Number::Decimal(
                    a.mul(&b, ctx, limits)?,
                )))
            }
            (Number::Integer(a), Number::Decimal(b)) => {
                let a = Decimal::from_bigint(a.clone());
                Ok(NumberResult::exact(Number::Decimal(a.mul(b, ctx, limits)?)))
            }
            (Number::Decimal(a), Number::Rational(b)) => {
                let (b, inexact) = Decimal::from_rational(b, ctx, limits)?;
                Ok(NumberResult {
                    value: Number::Decimal(a.mul(&b, ctx, limits)?),
                    rounded: inexact,
                })
            }
            (Number::Rational(a), Number::Decimal(b)) => {
                let (a, inexact) = Decimal::from_rational(a, ctx, limits)?;
                Ok(NumberResult {
                    value: Number::Decimal(a.mul(b, ctx, limits)?),
                    rounded: inexact,
                })
            }
            (Number::Float64(_), _) | (_, Number::Float64(_)) => unreachable!("handled above"),
        }
    }

    pub fn div(
        &self,
        other: &Number,
        ctx: &NumericContext,
        limits: &Limits,
    ) -> Result<NumberResult, EngineError> {
        self.require_float_allowed(other, ctx)?;
        if other.is_zero() {
            return Err(EngineError::division_by_zero(format!(
                "division by zero ({} / {})",
                self.kind_name(),
                other.kind_name()
            )));
        }
        if self.is_float() || other.is_float() {
            let a = self.to_f64().unwrap_or(f64::NAN);
            let b = other.to_f64().unwrap_or(f64::NAN);
            return Ok(NumberResult::rounded(Number::float(a / b)?));
        }
        match (self, other) {
            (Number::Integer(a), Number::Integer(b)) => {
                let (q, r) = a.div_rem(b);
                if r.is_zero() {
                    check_bigint(&q, ctx, limits)?;
                    Ok(NumberResult::exact(Number::Integer(q)))
                } else {
                    let out = BigRational::new(a.clone(), b.clone());
                    Ok(NumberResult::exact(Number::Rational(out)))
                }
            }
            (Number::Rational(a), Number::Rational(b)) => {
                let out = a / b;
                check_rational(&out, ctx, limits)?;
                Ok(NumberResult::exact(Number::Rational(out)))
            }
            (Number::Rational(a), Number::Integer(b)) => {
                let out = a / BigRational::from_integer(b.clone());
                check_rational(&out, ctx, limits)?;
                Ok(NumberResult::exact(Number::Rational(out)))
            }
            (Number::Integer(a), Number::Rational(b)) => {
                let out = BigRational::from_integer(a.clone()) / b;
                check_rational(&out, ctx, limits)?;
                Ok(NumberResult::exact(Number::Rational(out)))
            }
            (Number::Decimal(a), Number::Decimal(b)) => {
                let (value, inexact) = a.div(b, ctx, limits)?;
                Ok(NumberResult {
                    value: Number::Decimal(value),
                    rounded: inexact,
                })
            }
            (Number::Decimal(a), Number::Integer(b)) => {
                let b = Decimal::from_bigint(b.clone());
                let (value, inexact) = a.div(&b, ctx, limits)?;
                Ok(NumberResult {
                    value: Number::Decimal(value),
                    rounded: inexact,
                })
            }
            (Number::Integer(a), Number::Decimal(b)) => {
                let a = Decimal::from_bigint(a.clone());
                let (value, inexact) = a.div(b, ctx, limits)?;
                Ok(NumberResult {
                    value: Number::Decimal(value),
                    rounded: inexact,
                })
            }
            (Number::Decimal(a), Number::Rational(b)) => {
                let (b, b_inexact) = Decimal::from_rational(b, ctx, limits)?;
                let (value, d_inexact) = a.div(&b, ctx, limits)?;
                Ok(NumberResult {
                    value: Number::Decimal(value),
                    rounded: b_inexact || d_inexact,
                })
            }
            (Number::Rational(a), Number::Decimal(b)) => {
                let (a, a_inexact) = Decimal::from_rational(a, ctx, limits)?;
                let (value, d_inexact) = a.div(b, ctx, limits)?;
                Ok(NumberResult {
                    value: Number::Decimal(value),
                    rounded: a_inexact || d_inexact,
                })
            }
            (Number::Float64(_), _) | (_, Number::Float64(_)) => unreachable!("handled above"),
        }
    }

    /// Integer power. A non-integer exponent requires scientific mode.
    pub fn pow(
        &self,
        exponent: &Number,
        ctx: &NumericContext,
        limits: &Limits,
    ) -> Result<NumberResult, EngineError> {
        if self.is_float() || exponent.is_float() {
            if ctx.mode != NumericMode::Scientific {
                return Err(EngineError::new(
                    ErrorCode::UnsupportedNumericMode,
                    "exponentiation involving float64 requires scientific mode",
                ));
            }
            let a = self.to_f64().unwrap_or(f64::NAN);
            let b = exponent.to_f64().unwrap_or(f64::NAN);
            return Ok(NumberResult::rounded(Number::float(a.powf(b))?));
        }
        let exp_int = match exponent {
            Number::Integer(v) => v.clone(),
            Number::Rational(v) if v.is_integer() => v.to_integer(),
            Number::Decimal(v) => match v.to_bigint_if_integral() {
                Some(i) => i,
                None => {
                    if ctx.mode != NumericMode::Scientific {
                        return Err(EngineError::new(
                            ErrorCode::UnsupportedNumericMode,
                            "non-integer exponent requires scientific mode",
                        ));
                    }
                    let a = self.to_f64().unwrap_or(f64::NAN);
                    let b = exponent.to_f64().unwrap_or(f64::NAN);
                    return Ok(NumberResult::rounded(Number::float(a.powf(b))?));
                }
            },
            Number::Rational(_) => {
                if ctx.mode != NumericMode::Scientific {
                    return Err(EngineError::new(
                        ErrorCode::UnsupportedNumericMode,
                        "non-integer exponent requires scientific mode",
                    ));
                }
                let a = self.to_f64().unwrap_or(f64::NAN);
                let b = exponent.to_f64().unwrap_or(f64::NAN);
                return Ok(NumberResult::rounded(Number::float(a.powf(b))?));
            }
            Number::Float64(_) => unreachable!("handled above"),
        };
        let exp_u32 = match exp_int.to_u32() {
            Some(v) if v <= limits.max_exponent => v,
            _ => {
                return Err(EngineError::new(
                    ErrorCode::ResourceLimit,
                    format!(
                        "exponent {} exceeds the supported range 0..={}",
                        exp_int, limits.max_exponent
                    ),
                ));
            }
        };
        let negative_exponent = exp_int.sign() == Sign::Minus;
        match self {
            Number::Integer(base) => {
                if negative_exponent {
                    let power = base.pow(exp_u32);
                    if power.is_zero() {
                        return Err(EngineError::division_by_zero("zero to a negative power"));
                    }
                    let out = BigRational::new(BigInt::one(), power);
                    check_rational(&out, ctx, limits)?;
                    Ok(NumberResult::exact(Number::Rational(out)))
                } else {
                    let predicted = base.bits().saturating_mul(exp_u32 as u64);
                    let cap = limits.max_integer_bits.min(ctx.max_integer_bits) as u64;
                    if predicted > cap {
                        return Err(EngineError::new(
                            ErrorCode::ResourceLimit,
                            format!(
                                "integer power would need about {predicted} bits, exceeding the limit of {cap}"
                            ),
                        ));
                    }
                    let out = base.pow(exp_u32);
                    check_bigint(&out, ctx, limits)?;
                    Ok(NumberResult::exact(Number::Integer(out)))
                }
            }
            Number::Rational(base) => {
                if negative_exponent {
                    let flipped = BigRational::new(base.denom().clone(), base.numer().clone());
                    let out = flipped.pow(exp_u32 as i32);
                    check_rational(&out, ctx, limits)?;
                    Ok(NumberResult::exact(Number::Rational(out)))
                } else {
                    let out = base.pow(exp_u32 as i32);
                    check_rational(&out, ctx, limits)?;
                    Ok(NumberResult::exact(Number::Rational(out)))
                }
            }
            Number::Decimal(base) => {
                if negative_exponent {
                    let positive = base.pow_u32(exp_u32, ctx, limits)?;
                    let one = Decimal::from_bigint(BigInt::one());
                    let (value, inexact) = one.div(&positive, ctx, limits)?;
                    Ok(NumberResult {
                        value: Number::Decimal(value),
                        rounded: inexact,
                    })
                } else {
                    Ok(NumberResult::exact(Number::Decimal(
                        base.pow_u32(exp_u32, ctx, limits)?,
                    )))
                }
            }
            Number::Float64(_) => unreachable!("handled above"),
        }
    }

    /// Truncated remainder: sign follows the dividend (like Rust's `%`).
    pub fn rem(
        &self,
        other: &Number,
        ctx: &NumericContext,
        limits: &Limits,
    ) -> Result<NumberResult, EngineError> {
        if other.is_zero() {
            return Err(EngineError::division_by_zero("remainder by zero"));
        }
        if self.is_float() || other.is_float() {
            if ctx.mode != NumericMode::Scientific {
                return Err(EngineError::new(
                    ErrorCode::UnsupportedNumericMode,
                    "float64 remainder requires scientific mode",
                ));
            }
            let a = self.to_f64().unwrap_or(f64::NAN);
            let b = other.to_f64().unwrap_or(f64::NAN);
            return Ok(NumberResult::rounded(Number::float(a % b)?));
        }
        let a = self
            .to_exact_rational()
            .ok_or_else(|| EngineError::internal("remainder operand not representable"))?;
        let b = other
            .to_exact_rational()
            .ok_or_else(|| EngineError::internal("remainder operand not representable"))?;
        let q = &a / &b;
        let truncated = q.trunc();
        let r = a - truncated * b;
        rational_result(r, self, other, ctx, limits)
    }

    /// Euclidean modulo: result is non-negative for a positive modulus.
    pub fn modulo(
        &self,
        other: &Number,
        ctx: &NumericContext,
        limits: &Limits,
    ) -> Result<NumberResult, EngineError> {
        if other.is_zero() {
            return Err(EngineError::division_by_zero("modulo by zero"));
        }
        if self.is_float() || other.is_float() {
            if ctx.mode != NumericMode::Scientific {
                return Err(EngineError::new(
                    ErrorCode::UnsupportedNumericMode,
                    "float64 modulo requires scientific mode",
                ));
            }
            let a = self.to_f64().unwrap_or(f64::NAN);
            let b = other.to_f64().unwrap_or(f64::NAN);
            let mut r = a % b;
            if r != 0.0 && (r < 0.0) != (b < 0.0) {
                r += b;
            }
            return Ok(NumberResult::rounded(Number::float(r)?));
        }
        let a = self
            .to_exact_rational()
            .ok_or_else(|| EngineError::internal("modulo operand not representable"))?;
        let b = other
            .to_exact_rational()
            .ok_or_else(|| EngineError::internal("modulo operand not representable"))?;
        let q = &a / &b;
        let floored = q.floor();
        let r = a - floored * b;
        rational_result(r, self, other, ctx, limits)
    }

    pub fn round_to_scale(&self, scale: i64, mode: RoundingMode) -> Result<Number, EngineError> {
        match self {
            Number::Integer(v) => {
                if scale >= 0 {
                    Ok(Number::Decimal(
                        Decimal::from_bigint(v.clone()).round_to(scale as u32, mode),
                    ))
                } else {
                    let k = pow10((-scale) as u64);
                    Ok(Number::Integer(div_round(v, &k, mode) * k))
                }
            }
            Number::Decimal(v) => Ok(Number::Decimal(v.round_to_scale(scale, mode))),
            Number::Rational(v) => {
                let scale = scale.clamp(-1_000_000, 1_000_000);
                if scale >= 0 {
                    let factor = pow10(scale as u64);
                    let scaled = v * BigRational::from_integer(factor);
                    let rounded = div_round(scaled.numer(), scaled.denom(), mode);
                    Ok(Number::Decimal(Decimal::from_parts(rounded, scale as u32)))
                } else {
                    let factor = pow10((-scale) as u64);
                    let scaled = v / BigRational::from_integer(factor.clone());
                    let rounded = div_round(scaled.numer(), scaled.denom(), mode);
                    Ok(Number::Integer(rounded * factor))
                }
            }
            Number::Float64(v) => {
                let factor = 10f64.powi(scale.clamp(-308, 308) as i32);
                let rounded = match mode {
                    RoundingMode::HalfEven => (v.0 * factor).round_ties_even() / factor,
                    RoundingMode::HalfAwayFromZero => (v.0 * factor).round() / factor,
                    RoundingMode::TowardZero => (v.0 * factor).trunc() / factor,
                    RoundingMode::Floor => (v.0 * factor).floor() / factor,
                    RoundingMode::Ceiling => (v.0 * factor).ceil() / factor,
                };
                Ok(Number::Float64(Float64::new(rounded)?))
            }
        }
    }
}

fn rational_result(
    value: BigRational,
    lhs: &Number,
    rhs: &Number,
    ctx: &NumericContext,
    limits: &Limits,
) -> Result<NumberResult, EngineError> {
    match (lhs, rhs) {
        (Number::Integer(_), Number::Integer(_)) => {
            if value.is_integer() {
                Ok(NumberResult::exact(Number::Integer(value.to_integer())))
            } else {
                check_rational(&value, ctx, limits)?;
                Ok(NumberResult::exact(Number::Rational(value)))
            }
        }
        (Number::Decimal(_), _) | (_, Number::Decimal(_)) => {
            let (decimal, inexact) = Decimal::from_rational(&value, ctx, limits)?;
            Ok(NumberResult {
                value: Number::Decimal(decimal),
                rounded: inexact,
            })
        }
        _ => {
            check_rational(&value, ctx, limits)?;
            Ok(NumberResult::exact(Number::Rational(value)))
        }
    }
}

fn check_bigint(value: &BigInt, ctx: &NumericContext, limits: &Limits) -> Result<(), EngineError> {
    let cap = limits.max_integer_bits.min(ctx.max_integer_bits) as u64;
    if value.bits() > cap {
        return Err(EngineError::new(
            ErrorCode::ResourceLimit,
            format!(
                "integer result needs {} bits, exceeding the limit of {cap}",
                value.bits()
            ),
        ));
    }
    Ok(())
}

fn check_rational(
    value: &BigRational,
    ctx: &NumericContext,
    limits: &Limits,
) -> Result<(), EngineError> {
    let cap = limits.max_integer_bits.min(ctx.max_integer_bits) as u64;
    if value.numer().bits() > cap || value.denom().bits() > cap {
        return Err(EngineError::new(
            ErrorCode::ResourceLimit,
            format!(
                "rational result needs {}/{} bits, exceeding the limit of {cap}",
                value.numer().bits(),
                value.denom().bits()
            ),
        ));
    }
    Ok(())
}

/// Parse a decimal literal into `(mantissa, scale)`.
fn parse_decimal_parts(source: &str, limits: &Limits) -> Result<(BigInt, u32), EngineError> {
    let bytes = source.as_bytes();
    if bytes.is_empty() {
        return Err(EngineError::malformed("empty numeric literal"));
    }
    let mut i = 0usize;
    let mut negative = false;
    match bytes[i] {
        b'+' => i += 1,
        b'-' => {
            negative = true;
            i += 1;
        }
        _ => {}
    }
    let mut digits = String::new();
    let mut frac_digits: i64 = 0;
    let mut seen_digit = false;
    let mut seen_point = false;
    while i < bytes.len() {
        match bytes[i] {
            b'0'..=b'9' => {
                digits.push(bytes[i] as char);
                if seen_point {
                    frac_digits += 1;
                }
                seen_digit = true;
                i += 1;
            }
            b'.' => {
                if seen_point {
                    return Err(EngineError::malformed(format!(
                        "invalid numeric literal {source:?}: multiple decimal points"
                    )));
                }
                seen_point = true;
                i += 1;
            }
            b'e' | b'E' => break,
            _ => {
                return Err(EngineError::malformed(format!(
                    "invalid numeric literal {source:?}: unexpected character {:?}",
                    bytes[i] as char
                )));
            }
        }
    }
    if !seen_digit {
        return Err(EngineError::malformed(format!(
            "invalid numeric literal {source:?}: no digits"
        )));
    }
    let mut exponent: i64 = 0;
    if i < bytes.len() {
        // Consume exponent.
        i += 1;
        let mut exp_negative = false;
        if i < bytes.len() {
            match bytes[i] {
                b'+' => i += 1,
                b'-' => {
                    exp_negative = true;
                    i += 1;
                }
                _ => {}
            }
        }
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if start == i {
            return Err(EngineError::malformed(format!(
                "invalid numeric literal {source:?}: missing exponent digits"
            )));
        }
        if i != bytes.len() {
            return Err(EngineError::malformed(format!(
                "invalid numeric literal {source:?}: trailing characters"
            )));
        }
        let exp_text = &source[start..i];
        if exp_text.len() > 7 {
            return Err(EngineError::new(
                ErrorCode::PrecisionLimit,
                format!("exponent {exp_text} exceeds the supported range"),
            ));
        }
        exponent = exp_text.parse::<i64>().map_err(|_| {
            EngineError::malformed(format!("invalid numeric literal {source:?}: bad exponent"))
        })?;
        if exp_negative {
            exponent = -exponent;
        }
    }
    if digits.len() > limits.max_digits {
        return Err(EngineError::new(
            ErrorCode::ResourceLimit,
            format!(
                "numeric literal has {} digits, exceeding the limit of {}",
                digits.len(),
                limits.max_digits
            ),
        ));
    }
    let mut mantissa = BigInt::from_str(&digits)
        .map_err(|_| EngineError::malformed(format!("invalid numeric literal {source:?}")))?;
    if negative {
        mantissa = -mantissa;
    }
    let mut scale = frac_digits - exponent;
    if scale < 0 {
        mantissa *= pow10((-scale) as u64);
        scale = 0;
    }
    if scale > limits.max_decimal_scale as i64 {
        return Err(EngineError::new(
            ErrorCode::PrecisionLimit,
            format!(
                "numeric literal scale {scale} exceeds the limit of {}",
                limits.max_decimal_scale
            ),
        ));
    }
    if mantissa.bits() > limits.max_integer_bits as u64 {
        return Err(EngineError::new(
            ErrorCode::ResourceLimit,
            format!(
                "numeric literal needs {} bits, exceeding the limit of {}",
                mantissa.bits(),
                limits.max_integer_bits
            ),
        ));
    }
    Ok((mantissa, scale as u32))
}

impl Number {
    /// Parse a literal from expression text or a numeric shorthand string.
    ///
    /// A literal containing `.` or an exponent becomes a [`Decimal`]; otherwise
    /// an [`Integer`](Number::Integer). Rationals and Float64 are only produced
    /// by explicit typed construction, never by guessing at text.
    pub fn parse_literal(source: &str, limits: &Limits) -> Result<Number, EngineError> {
        let trimmed = source.trim();
        if trimmed.is_empty() {
            return Err(EngineError::malformed("empty numeric literal"));
        }
        if trimmed.contains('.') || trimmed.contains('e') || trimmed.contains('E') {
            Ok(Number::Decimal(Decimal::parse(trimmed, limits)?))
        } else {
            let digits = trimmed.strip_prefix(['+', '-']).unwrap_or(trimmed);
            if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                return Err(EngineError::malformed(format!(
                    "invalid numeric literal {source:?}"
                )));
            }
            if digits.len() > limits.max_digits {
                return Err(EngineError::new(
                    ErrorCode::ResourceLimit,
                    format!(
                        "numeric literal has {} digits, exceeding the limit of {}",
                        digits.len(),
                        limits.max_digits
                    ),
                ));
            }
            let value = BigInt::from_str(trimmed).map_err(|_| {
                EngineError::malformed(format!("invalid integer literal {source:?}"))
            })?;
            if value.bits() > limits.max_integer_bits as u64 {
                return Err(EngineError::new(
                    ErrorCode::ResourceLimit,
                    format!(
                        "integer literal needs {} bits, exceeding the limit of {}",
                        value.bits(),
                        limits.max_integer_bits
                    ),
                ));
            }
            Ok(Number::Integer(value))
        }
    }
}

impl fmt::Display for Number {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Number::Integer(v) => write!(f, "{v}"),
            Number::Rational(v) => {
                if v.is_integer() {
                    write!(f, "{}", v.to_integer())
                } else {
                    write!(f, "{}/{}", v.numer(), v.denom())
                }
            }
            Number::Decimal(v) => write!(f, "{v}"),
            Number::Float64(v) => write!(f, "{v}"),
        }
    }
}

/// The numeric context shared by all operations.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NumericContext {
    pub mode: NumericMode,
    /// Significant digits used when a context rounding is required.
    pub precision: u32,
    /// Maximum bits for exact integer/rational results.
    pub max_integer_bits: u32,
    /// Maximum decimal scale for exact decimal results.
    pub max_decimal_scale: u32,
    /// Rounding mode used when the context must round.
    pub rounding: RoundingMode,
}

impl Default for NumericContext {
    fn default() -> Self {
        NumericContext {
            mode: NumericMode::Auto,
            precision: 34,
            max_integer_bits: 8192,
            max_decimal_scale: 4096,
            rounding: RoundingMode::HalfEven,
        }
    }
}

impl NumericContext {
    pub fn exact() -> Self {
        NumericContext {
            mode: NumericMode::Exact,
            ..Default::default()
        }
    }

    pub fn scientific() -> Self {
        NumericContext {
            mode: NumericMode::Scientific,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> NumericContext {
        NumericContext::default()
    }

    fn limits() -> Limits {
        Limits::conservative()
    }

    #[test]
    fn decimal_parse_and_display_preserves_scale() {
        let d = Decimal::parse_default("0.10").unwrap();
        assert_eq!(d.mantissa(), &BigInt::from(10));
        assert_eq!(d.scale(), 2);
        assert_eq!(d.to_plain_string(), "0.10");
        assert_eq!(d.canonical_string(), "0.1");
    }

    #[test]
    fn decimal_add_is_exact() {
        let a = Decimal::parse_default("0.1").unwrap();
        let b = Decimal::parse_default("0.2").unwrap();
        let sum = a.add(&b, &ctx(), &limits()).unwrap();
        assert_eq!(sum.to_plain_string(), "0.3");
        assert!(sum.to_plain_string() != "0.30000000000000004");
    }

    #[test]
    fn rounding_modes_positive_and_negative_ties() {
        let cases = [
            ("1.005", RoundingMode::HalfEven, "1.00"),
            ("1.015", RoundingMode::HalfEven, "1.02"),
            ("1.005", RoundingMode::HalfAwayFromZero, "1.01"),
            ("-1.005", RoundingMode::HalfEven, "-1.00"),
            ("-1.015", RoundingMode::HalfEven, "-1.02"),
            ("-1.005", RoundingMode::HalfAwayFromZero, "-1.01"),
            ("1.009", RoundingMode::TowardZero, "1.00"),
            ("-1.009", RoundingMode::TowardZero, "-1.00"),
            ("-1.001", RoundingMode::Floor, "-1.01"),
            ("1.001", RoundingMode::Ceiling, "1.01"),
        ];
        for (input, mode, expected) in cases {
            let d = Decimal::parse_default(input).unwrap();
            let rounded = d.round_to(2, mode);
            assert_eq!(
                rounded.to_plain_string(),
                expected,
                "round({input}, {mode:?})"
            );
        }
    }

    #[test]
    fn exact_division_terminates() {
        let a = Decimal::parse_default("1").unwrap();
        let b = Decimal::parse_default("8").unwrap();
        let (q, inexact) = a.div(&b, &ctx(), &limits()).unwrap();
        assert!(!inexact);
        assert_eq!(q.to_plain_string(), "0.125");
    }

    #[test]
    fn inexact_division_reports_flag() {
        let a = Decimal::parse_default("1").unwrap();
        let b = Decimal::parse_default("3").unwrap();
        let (q, inexact) = a.div(&b, &ctx(), &limits()).unwrap();
        assert!(inexact);
        assert!(q.to_plain_string().starts_with("0.3333"));
    }

    #[test]
    fn exact_mode_rejects_inexact_division() {
        let a = Decimal::parse_default("1").unwrap();
        let b = Decimal::parse_default("3").unwrap();
        let err = a.div(&b, &NumericContext::exact(), &limits()).unwrap_err();
        assert_eq!(err.code, ErrorCode::UnsupportedNumericMode);
    }

    #[test]
    fn exact_sqrt_detection() {
        assert_eq!(
            Decimal::parse_default("0.25")
                .unwrap()
                .sqrt_exact()
                .unwrap()
                .to_plain_string(),
            "0.5"
        );
        assert!(Decimal::parse_default("2").unwrap().sqrt_exact().is_none());
    }

    #[test]
    fn number_promotion_int_div() {
        let one = Number::integer(1);
        let three = Number::integer(3);
        let result = one.div(&three, &ctx(), &limits()).unwrap();
        assert!(!result.rounded);
        assert_eq!(result.value.to_string(), "1/3");
    }

    #[test]
    fn float_arithmetic_requires_scientific_mode() {
        let a = Number::float(0.1).unwrap();
        let b = Number::integer(1);
        let err = a.add(&b, &ctx(), &limits()).unwrap_err();
        assert_eq!(err.code, ErrorCode::UnsupportedNumericMode);
        let ok = a.add(&b, &NumericContext::scientific(), &limits()).unwrap();
        assert!(ok.rounded);
    }

    #[test]
    fn comparisons_are_exact_across_representations() {
        let dec = Number::Decimal(Decimal::parse_default("0.5").unwrap());
        let float = Number::float(0.5).unwrap();
        assert!(dec.numeric_eq(&float).unwrap());
        let dec_tenth = Number::Decimal(Decimal::parse_default("0.1").unwrap());
        let float_tenth = Number::float(0.1).unwrap();
        assert!(!dec_tenth.numeric_eq(&float_tenth).unwrap());
    }

    #[test]
    fn pow_zero_zero_is_one() {
        let zero = Number::integer(0);
        let result = zero.pow(&zero, &ctx(), &limits()).unwrap();
        assert_eq!(result.value.to_string(), "1");
    }

    #[test]
    fn division_by_zero_is_structured() {
        let one = Number::integer(1);
        let zero = Number::integer(0);
        let err = one.div(&zero, &ctx(), &limits()).unwrap_err();
        assert_eq!(err.code, ErrorCode::DivisionByZero);
    }

    #[test]
    fn remainder_vs_modulo_negative_operands() {
        let a = Number::integer(-7);
        let b = Number::integer(3);
        let rem = a.rem(&b, &ctx(), &limits()).unwrap();
        let modulo = a.modulo(&b, &ctx(), &limits()).unwrap();
        assert_eq!(rem.value.to_string(), "-1");
        assert_eq!(modulo.value.to_string(), "2");
    }
}
