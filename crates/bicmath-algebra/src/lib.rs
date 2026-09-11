//! Algebra module: exact polynomial algebra, number theory, and combinatorics.
//!
//! Polynomial coefficient arrays are indexed by degree with the constant term
//! first; trailing zero coefficients are normalized away and the zero
//! polynomial is reported as `[0]`. All polynomial, number-theoretic, and
//! combinatorial results are exact over the integers and rationals. The only
//! approximate outputs are irrational real roots of polynomials of degree at
//! most four, which are reported as `float64` records together with an explicit
//! warning and are rejected in exact numeric mode.

use std::collections::BTreeMap;
use std::sync::Arc;

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, CostClass, Example, Function, FunctionDescriptor, Module, ModuleDescriptor, Outcome,
    ParamDescriptor, SimpleFunction, Warning,
};
use bicmath_core::envelope::Exactness;
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::number::{Number, NumericMode};
use bicmath_core::schema::{FieldSchema, NumberKind, ValueSchema};
use bicmath_core::value::Value;
use num_bigint::BigInt;
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::{One, Signed, ToPrimitive, Zero};

mod f64math {
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn sqrt(value: f64) -> f64 {
        libm::sqrt(value)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn sqrt(value: f64) -> f64 {
        value.sqrt()
    }
}

/// Trial division is performed up to this bound before Pollard rho is used.
const TRIAL_DIVISION_LIMIT: u32 = 100_000;
/// Pollard rho Brent batch size.
const POLLARD_BATCH: u64 = 128;
/// Maximum rho doubling factor before retrying with a different polynomial.
const POLLARD_MAX_R: u64 = 1 << 20;
/// Maximum number of deterministic Pollard rho polynomial constants.
const POLLARD_MAX_C: u32 = 100;
/// Largest Fibonacci/Lucas index accepted (larger indices exceed the bit cap).
const MAX_FIB_INDEX: u64 = 20_000;
/// Largest partition index accepted by the bounded pentagonal recurrence.
const MAX_PARTITION_N: u64 = 10_000;

/// Deterministic Miller-Rabin bases that are proven sufficient for n < 2^64.
const DETERMINISTIC_BASES: [u32; 12] = [2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37];

/// Fixed bases used for n >= 2^64, where the test is probabilistic.
const LARGE_INPUT_BASES: [u32; 64] = [
    2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59, 61, 67, 71, 73, 79, 83, 89, 97,
    101, 103, 107, 109, 113, 127, 131, 137, 139, 149, 151, 157, 163, 167, 173, 179, 181, 191, 193,
    197, 199, 211, 223, 227, 229, 233, 239, 241, 251, 257, 263, 269, 271, 277, 281, 283, 293, 307,
    311,
];

fn all_modes() -> Vec<NumericMode> {
    vec![
        NumericMode::Exact,
        NumericMode::Auto,
        NumericMode::Scientific,
    ]
}

fn exact_array_schema() -> ValueSchema {
    ValueSchema::array(ValueSchema::exact())
}

fn integer_schema() -> ValueSchema {
    ValueSchema::number(NumberKind::Integer)
}

fn integer_array_schema() -> ValueSchema {
    ValueSchema::array(integer_schema())
}

fn root_schema() -> ValueSchema {
    ValueSchema::Record {
        fields: vec![
            FieldSchema::required("exact", ValueSchema::Bool)
                .with_description("Whether the root is an exact rational number."),
            FieldSchema::optional("value", ValueSchema::exact())
                .with_description("Exact root, present when `exact` is true."),
            FieldSchema::optional("approximate", ValueSchema::float64())
                .with_description("float64 approximation, present when `exact` is false."),
        ],
        allow_extra: false,
    }
}

fn divmod_schema() -> ValueSchema {
    ValueSchema::Record {
        fields: vec![
            FieldSchema::required("quotient", exact_array_schema()),
            FieldSchema::required("remainder", exact_array_schema()),
        ],
        allow_extra: false,
    }
}

fn gcd_extended_schema() -> ValueSchema {
    ValueSchema::Record {
        fields: vec![
            FieldSchema::required("gcd", integer_schema())
                .with_description("Non-negative greatest common divisor."),
            FieldSchema::required("x", integer_schema())
                .with_description("Bezout coefficient of `a`."),
            FieldSchema::required("y", integer_schema())
                .with_description("Bezout coefficient of `b`."),
        ],
        allow_extra: false,
    }
}

fn prime_factors_schema() -> ValueSchema {
    ValueSchema::Record {
        fields: vec![
            FieldSchema::required(
                "factors",
                ValueSchema::array(ValueSchema::Record {
                    fields: vec![
                        FieldSchema::required("prime", integer_schema()),
                        FieldSchema::required("exponent", integer_schema()),
                    ],
                    allow_extra: false,
                }),
            )
            .with_description("Prime factors in ascending order with multiplicities."),
            FieldSchema::required(
                "method",
                ValueSchema::Enum {
                    variants: vec![
                        "trial_division".to_string(),
                        "trial_division+pollard_rho_brent".to_string(),
                    ],
                },
            )
            .with_description("Factorization method that produced the complete factorization."),
        ],
        allow_extra: false,
    }
}

/// Cooperative work guard enforcing the context operation and iteration
/// budgets and polling cancellation.
struct Guard<'a> {
    ctx: &'a ExecContext,
    operations: u64,
    iterations: u64,
}

impl Guard<'_> {
    fn new(ctx: &ExecContext) -> Guard<'_> {
        Guard {
            ctx,
            operations: 0,
            iterations: 0,
        }
    }

    fn tick(&mut self) -> Result<(), EngineError> {
        self.operations = self.operations.saturating_add(1);
        let limit = self.ctx.limits.max_operations;
        if self.operations > limit {
            return Err(EngineError::resource(format!(
                "operation budget of {limit} exceeded"
            )));
        }
        if self.operations.is_multiple_of(64) {
            self.ctx.check()?;
        }
        Ok(())
    }

    fn iterate(&mut self) -> Result<(), EngineError> {
        self.iterations = self.iterations.saturating_add(1);
        let limit = self.ctx.limits.max_iterations;
        if self.iterations > limit {
            return Err(EngineError::resource(format!(
                "iteration budget of {limit} exceeded"
            )));
        }
        self.tick()
    }
}

fn check_integer_bits(value: &BigInt, ctx: &ExecContext) -> Result<(), EngineError> {
    let cap = ctx
        .limits
        .max_integer_bits
        .min(ctx.numeric.max_integer_bits) as u64;
    let bits = value.bits();
    if bits > cap {
        return Err(EngineError::resource(format!(
            "integer result needs {bits} bits, exceeding the limit of {cap}"
        )));
    }
    Ok(())
}

fn check_rational_bits(value: &BigRational, ctx: &ExecContext) -> Result<(), EngineError> {
    let cap = ctx
        .limits
        .max_integer_bits
        .min(ctx.numeric.max_integer_bits) as u64;
    let numerator_bits = value.numer().bits();
    let denominator_bits = value.denom().bits();
    if numerator_bits > cap || denominator_bits > cap {
        return Err(EngineError::resource(format!(
            "rational result needs {numerator_bits}/{denominator_bits} bits, \
             exceeding the limit of {cap}"
        )));
    }
    Ok(())
}

fn rational_number(number: &Number, path: &str) -> Result<BigRational, EngineError> {
    match number {
        Number::Integer(value) => Ok(BigRational::from_integer(value.clone())),
        Number::Rational(value) => Ok(value.clone()),
        Number::Decimal(value) => Ok(value.to_rational()),
        Number::Float64(_) => Err(EngineError::new(
            ErrorCode::UnsupportedNumericMode,
            "algebra operates on exact integers and rationals; float64 is not accepted",
        )
        .with_path(path.to_string())),
    }
}

fn exact_number(args: &Args, name: &str) -> Result<BigRational, EngineError> {
    let number = args.number(name)?;
    rational_number(number, name)
}

fn rational_array(args: &Args, name: &str) -> Result<Vec<BigRational>, EngineError> {
    let items = args.array(name)?;
    let mut out = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        match item {
            Value::Number(number) => {
                out.push(rational_number(number, &format!("{name}[{index}]"))?)
            }
            other => {
                let kind = other.kind_name();
                return Err(EngineError::malformed(format!(
                    "expected a number at {name}[{index}], found {kind}"
                ))
                .with_path(format!("{name}[{index}]")));
            }
        }
    }
    Ok(out)
}

fn integer_array(args: &Args, name: &str) -> Result<Vec<BigInt>, EngineError> {
    let items = args.array(name)?;
    let mut out = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        match item {
            Value::Number(Number::Integer(value)) => out.push(value.clone()),
            other => {
                let kind = other.kind_name();
                return Err(EngineError::malformed(format!(
                    "expected an integer at {name}[{index}], found {kind}"
                ))
                .with_path(format!("{name}[{index}]")));
            }
        }
    }
    Ok(out)
}

fn rational_to_value(value: &BigRational) -> Value {
    if value.is_integer() {
        Value::Number(Number::Integer(value.to_integer()))
    } else {
        Value::Number(Number::Rational(value.clone()))
    }
}

fn integer_value(value: BigInt) -> Value {
    Value::Number(Number::Integer(value))
}

fn int_value(value: i64) -> Value {
    Value::Number(Number::Integer(BigInt::from(value)))
}

fn poly_value(coefficients: &[i64]) -> Value {
    Value::Array(
        coefficients
            .iter()
            .map(|coefficient| int_value(*coefficient))
            .collect(),
    )
}

fn integer_array_value(values: &[i64]) -> Value {
    Value::Array(values.iter().map(|value| int_value(*value)).collect())
}

fn factor_value(prime: i64, exponent: i64) -> Value {
    Value::record([
        ("prime", int_value(prime)),
        ("exponent", int_value(exponent)),
    ])
}

fn exact_root_value(value: i64) -> Value {
    Value::record([("exact", Value::Bool(true)), ("value", int_value(value))])
}

fn example_args(pairs: &[(&str, Value)]) -> BTreeMap<String, Value> {
    pairs
        .iter()
        .map(|(name, value)| ((*name).to_string(), value.clone()))
        .collect()
}

fn base_descriptor(id: &str, title: &str, summary: &str) -> FunctionDescriptor {
    FunctionDescriptor::new(format!("algebra.{id}"), "algebra", "1.0.0", title, summary)
        .with_modes(all_modes())
        .with_method_ref(format!("docs/methods/algebra.md#{id}"))
}

// ---------------------------------------------------------------------------
// Polynomial representation
// ---------------------------------------------------------------------------

/// Dense polynomial over the rationals, coefficients indexed by degree. The
/// zero polynomial is the empty coefficient vector; trailing zeros are always
/// stripped.
#[derive(Clone, Debug)]
struct Poly {
    coefficients: Vec<BigRational>,
}

impl Poly {
    fn new(coefficients: Vec<BigRational>) -> Poly {
        let mut out = coefficients;
        while out.last().is_some_and(BigRational::is_zero) {
            out.pop();
        }
        Poly { coefficients: out }
    }

    fn zero() -> Poly {
        Poly {
            coefficients: Vec::new(),
        }
    }

    fn is_zero(&self) -> bool {
        self.coefficients.is_empty()
    }

    fn degree(&self) -> Option<usize> {
        self.coefficients.len().checked_sub(1)
    }

    fn plus(&self, other: &Poly) -> Poly {
        let length = self.coefficients.len().max(other.coefficients.len());
        let mut out = Vec::with_capacity(length);
        let mut left = self.coefficients.iter();
        let mut right = other.coefficients.iter();
        for _ in 0..length {
            let a = left.next().cloned().unwrap_or_else(BigRational::zero);
            let b = right.next().cloned().unwrap_or_else(BigRational::zero);
            out.push(a + b);
        }
        Poly::new(out)
    }

    fn minus(&self, other: &Poly) -> Poly {
        let length = self.coefficients.len().max(other.coefficients.len());
        let mut out = Vec::with_capacity(length);
        let mut left = self.coefficients.iter();
        let mut right = other.coefficients.iter();
        for _ in 0..length {
            let a = left.next().cloned().unwrap_or_else(BigRational::zero);
            let b = right.next().cloned().unwrap_or_else(BigRational::zero);
            out.push(a - b);
        }
        Poly::new(out)
    }

    fn times(&self, other: &Poly, guard: &mut Guard<'_>) -> Result<Poly, EngineError> {
        if self.is_zero() || other.is_zero() {
            return Ok(Poly::zero());
        }
        let mut out =
            vec![BigRational::zero(); self.coefficients.len() + other.coefficients.len() - 1];
        for (left_index, left) in self.coefficients.iter().enumerate() {
            if left.is_zero() {
                continue;
            }
            for (right_index, right) in other.coefficients.iter().enumerate() {
                guard.tick()?;
                out[left_index + right_index] += left * right;
            }
        }
        Ok(Poly::new(out))
    }

    fn derivative(&self) -> Poly {
        if self.coefficients.len() <= 1 {
            return Poly::zero();
        }
        let mut out = Vec::with_capacity(self.coefficients.len() - 1);
        for (index, coefficient) in self.coefficients.iter().enumerate().skip(1) {
            out.push(coefficient * BigRational::from_integer(BigInt::from(index)));
        }
        Poly::new(out)
    }

    fn evaluate(&self, x: &BigRational, guard: &mut Guard<'_>) -> Result<BigRational, EngineError> {
        let mut result = BigRational::zero();
        for coefficient in self.coefficients.iter().rev() {
            guard.tick()?;
            result = result * x + coefficient;
        }
        Ok(result)
    }

    /// Polynomial long division. Returns `(quotient, remainder)`.
    fn divmod(&self, divisor: &Poly, guard: &mut Guard<'_>) -> Result<(Poly, Poly), EngineError> {
        if divisor.is_zero() {
            return Err(EngineError::division_by_zero(
                "polynomial division by the zero polynomial",
            ));
        }
        let divisor_degree = divisor.coefficients.len() - 1;
        let divisor_lead = divisor
            .coefficients
            .last()
            .cloned()
            .unwrap_or_else(BigRational::zero);
        if self.coefficients.len() < divisor.coefficients.len() {
            return Ok((Poly::zero(), self.clone()));
        }
        let mut remainder = self.coefficients.clone();
        let mut quotient = vec![BigRational::zero(); self.coefficients.len() - divisor_degree];
        while remainder.len() >= divisor.coefficients.len() {
            guard.iterate()?;
            let degree_difference = remainder.len() - divisor.coefficients.len();
            let lead = remainder.last().cloned().unwrap_or_else(BigRational::zero);
            let factor = lead / &divisor_lead;
            quotient[degree_difference] = factor.clone();
            for (index, divisor_coefficient) in divisor.coefficients.iter().enumerate() {
                remainder[degree_difference + index] -= &factor * divisor_coefficient;
            }
            while remainder.last().is_some_and(BigRational::is_zero) {
                remainder.pop();
            }
        }
        Ok((Poly::new(quotient), Poly::new(remainder)))
    }

    fn monic(&self) -> Poly {
        if self.is_zero() {
            return Poly::zero();
        }
        let lead = self
            .coefficients
            .last()
            .cloned()
            .unwrap_or_else(BigRational::zero);
        Poly::new(
            self.coefficients
                .iter()
                .map(|coefficient| coefficient / &lead)
                .collect(),
        )
    }

    fn gcd(left: &Poly, right: &Poly, guard: &mut Guard<'_>) -> Result<Poly, EngineError> {
        let mut a = left.clone();
        let mut b = right.clone();
        while !b.is_zero() {
            guard.iterate()?;
            let (_quotient, remainder) = a.divmod(&b, guard)?;
            a = b;
            b = remainder;
        }
        Ok(a.monic())
    }

    fn from_roots(
        roots: &[BigRational],
        leading: &BigRational,
        guard: &mut Guard<'_>,
    ) -> Result<Poly, EngineError> {
        let mut out = Poly::new(vec![leading.clone()]);
        for root in roots {
            guard.iterate()?;
            let factor = Poly::new(vec![-root.clone(), BigRational::one()]);
            out = out.times(&factor, guard)?;
        }
        Ok(out)
    }

    /// Scale by the least common multiple of the denominators so that every
    /// coefficient is an integer.
    fn integer_coefficients(&self) -> Poly {
        let mut lcm = BigInt::one();
        for coefficient in &self.coefficients {
            lcm = lcm.lcm(coefficient.denom());
        }
        Poly::new(
            self.coefficients
                .iter()
                .map(|coefficient| {
                    BigRational::from_integer(coefficient.numer() * (&lcm / coefficient.denom()))
                })
                .collect(),
        )
    }
}

fn poly_value_of(poly: &Poly) -> Value {
    if poly.is_zero() {
        return Value::Array(vec![int_value(0)]);
    }
    Value::Array(poly.coefficients.iter().map(rational_to_value).collect())
}

fn checked_poly_value(
    poly: &Poly,
    ctx: &ExecContext,
    guard: &mut Guard<'_>,
) -> Result<Value, EngineError> {
    for coefficient in &poly.coefficients {
        guard.tick()?;
        check_rational_bits(coefficient, ctx)?;
    }
    Ok(poly_value_of(poly))
}

// ---------------------------------------------------------------------------
// Polynomial roots
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
enum Root {
    Exact(BigRational),
    Approximate(f64),
}

fn root_sort_key(root: &Root) -> f64 {
    match root {
        Root::Exact(value) => value.to_f64().unwrap_or(f64::NAN),
        Root::Approximate(value) => *value,
    }
}

fn root_to_value(root: &Root) -> Result<Value, EngineError> {
    match root {
        Root::Exact(value) => Ok(Value::record([
            ("exact", Value::Bool(true)),
            ("value", rational_to_value(value)),
        ])),
        Root::Approximate(value) => Ok(Value::record([
            ("exact", Value::Bool(false)),
            ("approximate", Value::Number(Number::float(*value)?)),
        ])),
    }
}

fn find_roots(poly: &Poly, guard: &mut Guard<'_>) -> Result<Vec<Root>, EngineError> {
    let mut roots = Vec::new();
    let mut current = poly.clone();
    while let Some(degree) = current.degree() {
        match degree {
            0 => break,
            1 => {
                let linear = current.coefficients[1].clone();
                let constant = current.coefficients[0].clone();
                roots.push(Root::Exact(-constant / linear));
                break;
            }
            2 => {
                solve_quadratic(&current, &mut roots, guard)?;
                break;
            }
            _ => match find_rational_root(&current, guard)? {
                Some(root) => {
                    let divisor = Poly::new(vec![-root.clone(), BigRational::one()]);
                    let (quotient, _remainder) = current.divmod(&divisor, guard)?;
                    roots.push(Root::Exact(root));
                    current = quotient;
                }
                None => {
                    let approximate = numeric_real_roots(&current, guard)?;
                    roots.extend(approximate.into_iter().map(Root::Approximate));
                    break;
                }
            },
        }
    }
    roots.sort_by(|left, right| root_sort_key(left).total_cmp(&root_sort_key(right)));
    Ok(roots)
}

fn solve_quadratic(
    poly: &Poly,
    roots: &mut Vec<Root>,
    guard: &mut Guard<'_>,
) -> Result<(), EngineError> {
    let a = poly.coefficients[2].clone();
    let b = poly.coefficients[1].clone();
    let c = poly.coefficients[0].clone();
    let discriminant = &b * &b - BigRational::from_integer(BigInt::from(4)) * &a * &c;
    if discriminant.is_negative() {
        return Ok(());
    }
    if discriminant.is_zero() {
        roots.push(Root::Exact(
            -b / (BigRational::from_integer(BigInt::from(2)) * a),
        ));
        return Ok(());
    }
    if let Some(square_root) = perfect_rational_square_root(&discriminant) {
        let denominator = BigRational::from_integer(BigInt::from(2)) * a;
        roots.push(Root::Exact((-&b - &square_root) / &denominator));
        roots.push(Root::Exact((-&b + &square_root) / &denominator));
        return Ok(());
    }
    guard.tick()?;
    let a_float = rational_to_f64(&a)?;
    let b_float = rational_to_f64(&b)?;
    let discriminant_float = rational_to_f64(&discriminant)?;
    let square_root = f64math::sqrt(discriminant_float);
    let denominator = 2.0 * a_float;
    roots.push(Root::Approximate((-b_float - square_root) / denominator));
    roots.push(Root::Approximate((-b_float + square_root) / denominator));
    Ok(())
}

fn perfect_rational_square_root(value: &BigRational) -> Option<BigRational> {
    if value.is_negative() {
        return None;
    }
    let numerator = value.numer().sqrt();
    let denominator = value.denom().sqrt();
    if &numerator * &numerator == *value.numer() && &denominator * &denominator == *value.denom() {
        Some(BigRational::new(numerator, denominator))
    } else {
        None
    }
}

fn rational_to_f64(value: &BigRational) -> Result<f64, EngineError> {
    let float = value
        .to_f64()
        .ok_or_else(|| EngineError::domain("value is too large for float64 root approximation"))?;
    if float.is_finite() {
        Ok(float)
    } else {
        Err(EngineError::domain(
            "value is too large for float64 root approximation",
        ))
    }
}

fn find_rational_root(
    poly: &Poly,
    guard: &mut Guard<'_>,
) -> Result<Option<BigRational>, EngineError> {
    let integer_poly = poly.integer_coefficients();
    if integer_poly.coefficients[0].is_zero() {
        return Ok(Some(BigRational::zero()));
    }
    let constant = integer_poly.coefficients[0].to_integer().abs();
    let leading = integer_poly
        .coefficients
        .last()
        .map(|coefficient| coefficient.to_integer().abs())
        .unwrap_or_else(BigInt::one);
    let (constant_factors, _) = factor_map(&constant, guard)?;
    let (leading_factors, _) = factor_map(&leading, guard)?;
    let constant_divisors = divisors_from_factors(&constant_factors, guard)?;
    let leading_divisors = divisors_from_factors(&leading_factors, guard)?;
    let mut candidates = Vec::new();
    for numerator in &constant_divisors {
        for denominator in &leading_divisors {
            let candidate = BigRational::new(numerator.clone(), denominator.clone());
            candidates.push(candidate.clone());
            candidates.push(-candidate);
        }
    }
    candidates.sort();
    candidates.dedup();
    for candidate in candidates {
        guard.iterate()?;
        if poly.evaluate(&candidate, guard)?.is_zero() {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

fn numeric_real_roots(poly: &Poly, guard: &mut Guard<'_>) -> Result<Vec<f64>, EngineError> {
    let mut coefficients = Vec::with_capacity(poly.coefficients.len());
    for coefficient in &poly.coefficients {
        coefficients.push(rational_to_f64(coefficient)?);
    }
    let mut roots = Vec::new();
    real_roots_f64(&coefficients, guard, &mut roots)?;
    roots.sort_by(|left, right| left.total_cmp(right));
    roots.dedup_by(|left, right| {
        (*left - *right).abs() <= 1e-12 * left.abs().max(right.abs()).max(1.0)
    });
    Ok(roots)
}

fn real_roots_f64(
    coefficients: &[f64],
    guard: &mut Guard<'_>,
    roots: &mut Vec<f64>,
) -> Result<(), EngineError> {
    let degree = coefficients.len() - 1;
    match degree {
        0 => Ok(()),
        1 => {
            roots.push(-coefficients[0] / coefficients[1]);
            Ok(())
        }
        2 => {
            let a = coefficients[2];
            let b = coefficients[1];
            let c = coefficients[0];
            let discriminant = b * b - 4.0 * a * c;
            if discriminant < 0.0 {
                return Ok(());
            }
            let square_root = f64math::sqrt(discriminant);
            roots.push((-b - square_root) / (2.0 * a));
            roots.push((-b + square_root) / (2.0 * a));
            Ok(())
        }
        _ => {
            let mut derivative = Vec::with_capacity(degree);
            for (index, coefficient) in coefficients.iter().enumerate().skip(1) {
                derivative.push(coefficient * index as f64);
            }
            let mut critical_points = Vec::new();
            real_roots_f64(&derivative, guard, &mut critical_points)?;
            critical_points.sort_by(|left, right| left.total_cmp(right));
            critical_points.dedup_by(|left, right| (*left - *right).abs() <= 1e-12);
            let bound = root_bound(coefficients)?;
            let mut samples = Vec::with_capacity(critical_points.len() + 2);
            samples.push(-bound);
            samples.extend(critical_points.iter().copied());
            samples.push(bound);
            samples.sort_by(|left, right| left.total_cmp(right));
            samples.dedup_by(|left, right| {
                (*left - *right).abs() <= 1e-12 * left.abs().max(right.abs()).max(1.0)
            });
            let mut at_root = vec![false; samples.len()];
            for (index, point) in samples.iter().enumerate() {
                guard.tick()?;
                if is_root_at(coefficients, *point) {
                    roots.push(*point);
                    at_root[index] = true;
                }
            }
            for (index, window) in samples.windows(2).enumerate() {
                guard.iterate()?;
                if at_root[index] || at_root[index + 1] {
                    continue;
                }
                let (left, right) = (window[0], window[1]);
                let left_value = evaluate_f64(coefficients, left);
                let right_value = evaluate_f64(coefficients, right);
                if left_value.signum() != right_value.signum()
                    && let Some(root) = bisect(coefficients, left, right, guard)?
                {
                    roots.push(root);
                }
            }
            Ok(())
        }
    }
}

fn root_bound(coefficients: &[f64]) -> Result<f64, EngineError> {
    let leading = coefficients.last().copied().unwrap_or(0.0).abs();
    if leading == 0.0 {
        return Err(EngineError::internal(
            "root bound requires a non-zero leading coefficient",
        ));
    }
    let mut maximum = 0.0f64;
    for coefficient in &coefficients[..coefficients.len() - 1] {
        maximum = maximum.max(coefficient.abs() / leading);
    }
    let bound = 1.0 + maximum;
    if bound.is_finite() {
        Ok(bound)
    } else {
        Err(EngineError::domain(
            "polynomial coefficients are too large for float64 root approximation",
        ))
    }
}

fn evaluate_f64(coefficients: &[f64], x: f64) -> f64 {
    let mut result = 0.0;
    for coefficient in coefficients.iter().rev() {
        result = result * x + coefficient;
    }
    result
}

fn evaluate_abs_f64(coefficients: &[f64], x: f64) -> f64 {
    let mut result = 0.0;
    for coefficient in coefficients.iter().rev() {
        result = result * x.abs() + coefficient.abs();
    }
    result
}

fn is_root_at(coefficients: &[f64], point: f64) -> bool {
    let value = evaluate_f64(coefficients, point);
    let scale = evaluate_abs_f64(coefficients, point);
    value.abs() <= 1e-12 * scale.max(1.0)
}

fn bisect(
    coefficients: &[f64],
    mut low: f64,
    mut high: f64,
    guard: &mut Guard<'_>,
) -> Result<Option<f64>, EngineError> {
    let mut low_value = evaluate_f64(coefficients, low);
    for _ in 0..200 {
        guard.iterate()?;
        let middle = (low + high) / 2.0;
        let middle_value = evaluate_f64(coefficients, middle);
        if middle_value == 0.0 || (high - low) <= 1e-15 * middle.abs().max(1.0) {
            return Ok(Some(middle));
        }
        if low_value.signum() != middle_value.signum() {
            high = middle;
        } else {
            low = middle;
            low_value = middle_value;
        }
    }
    Ok(Some((low + high) / 2.0))
}

// ---------------------------------------------------------------------------
// Number theory
// ---------------------------------------------------------------------------

fn miller_rabin_round(n: &BigInt, d: &BigInt, s: u64, base: &BigInt) -> bool {
    let a = base.mod_floor(n);
    if a.is_zero() {
        return true;
    }
    let mut x = a.modpow(d, n);
    if x.is_one() || x == n - 1u32 {
        return true;
    }
    for _ in 1..s {
        x = x.modpow(&BigInt::from(2u32), n);
        if x == n - 1u32 {
            return true;
        }
        if x.is_one() {
            return false;
        }
    }
    false
}

fn is_prime_bigint(n: &BigInt, guard: &mut Guard<'_>) -> Result<bool, EngineError> {
    if n < &BigInt::from(2u32) {
        return Ok(false);
    }
    for prime in DETERMINISTIC_BASES {
        let divisor = BigInt::from(prime);
        if n == &divisor {
            return Ok(true);
        }
        if (n % &divisor).is_zero() {
            return Ok(false);
        }
    }
    let n_minus_one = n - 1u32;
    let s = n_minus_one.trailing_zeros().unwrap_or(0);
    let d = &n_minus_one >> s;
    let bases: &[u32] = if n.bits() <= 64 {
        &DETERMINISTIC_BASES
    } else {
        &LARGE_INPUT_BASES
    };
    for base in bases {
        guard.iterate()?;
        if !miller_rabin_round(n, &d, s, &BigInt::from(*base)) {
            return Ok(false);
        }
    }
    Ok(true)
}

fn next_prime_bigint(n: &BigInt, guard: &mut Guard<'_>) -> Result<BigInt, EngineError> {
    if n < &BigInt::from(2u32) {
        return Ok(BigInt::from(2u32));
    }
    let mut candidate = n + 1u32;
    if candidate.is_even() {
        candidate += 1u32;
    }
    loop {
        guard.iterate()?;
        if is_prime_bigint(&candidate, guard)? {
            return Ok(candidate);
        }
        candidate += 2u32;
    }
}

fn pollard_brent(n: &BigInt, guard: &mut Guard<'_>) -> Result<BigInt, EngineError> {
    if n.is_even() {
        return Ok(BigInt::from(2u32));
    }
    let one = BigInt::one();
    let mut c = BigInt::one();
    loop {
        guard.iterate()?;
        let mut y = BigInt::from(2u32);
        let mut r: u64 = 1;
        let mut q = BigInt::one();
        let mut g = BigInt::one();
        let mut x = y.clone();
        let mut ys = y.clone();
        while g.is_one() {
            x = y.clone();
            for _ in 0..r {
                guard.iterate()?;
                y = (&y * &y + &c).mod_floor(n);
            }
            let mut k: u64 = 0;
            while k < r && g.is_one() {
                ys = y.clone();
                let batch = POLLARD_BATCH.min(r - k);
                for _ in 0..batch {
                    guard.iterate()?;
                    y = (&y * &y + &c).mod_floor(n);
                    let difference = if x > y { &x - &y } else { &y - &x };
                    if !difference.is_zero() {
                        q = (q * difference).mod_floor(n);
                    }
                }
                g = q.gcd(n);
                k += batch;
            }
            r = r.saturating_mul(2);
            if r > POLLARD_MAX_R {
                break;
            }
        }
        if g.is_one() {
            c += 1u32;
            if c > BigInt::from(POLLARD_MAX_C) {
                return Err(EngineError::new(
                    ErrorCode::NonConvergence,
                    "Pollard rho failed to split the composite number",
                ));
            }
            continue;
        }
        if &g == n {
            loop {
                guard.iterate()?;
                ys = (&ys * &ys + &c).mod_floor(n);
                let difference = if x > ys { &x - &ys } else { &ys - &x };
                g = difference.gcd(n);
                if !g.is_one() {
                    break;
                }
            }
        }
        if &g != n && g > one {
            return Ok(g);
        }
        c += 1u32;
        if c > BigInt::from(POLLARD_MAX_C) {
            return Err(EngineError::new(
                ErrorCode::NonConvergence,
                "Pollard rho failed to split the composite number",
            ));
        }
    }
}

/// Complete factorization of a positive integer. Returns the factor map and
/// whether Pollard rho was required in addition to trial division.
fn factor_map(
    value: &BigInt,
    guard: &mut Guard<'_>,
) -> Result<(BTreeMap<BigInt, u32>, bool), EngineError> {
    if value.is_negative() || value.is_zero() {
        return Err(EngineError::domain(
            "factorization requires a positive integer",
        ));
    }
    let mut remaining = value.clone();
    let mut factors: BTreeMap<BigInt, u32> = BTreeMap::new();
    let limit = BigInt::from(TRIAL_DIVISION_LIMIT);
    let mut candidate = BigInt::from(2u32);
    while candidate <= limit {
        if &candidate * &candidate > remaining {
            break;
        }
        guard.iterate()?;
        while (&remaining % &candidate).is_zero() {
            guard.iterate()?;
            remaining /= &candidate;
            *factors.entry(candidate.clone()).or_insert(0) += 1;
        }
        candidate = if candidate == BigInt::from(2u32) {
            BigInt::from(3u32)
        } else {
            candidate + 2u32
        };
    }
    let mut pollard_used = false;
    if remaining > BigInt::one() {
        let mut stack = vec![remaining];
        while let Some(current) = stack.pop() {
            guard.iterate()?;
            if current.is_one() {
                continue;
            }
            if is_prime_bigint(&current, guard)? {
                *factors.entry(current).or_insert(0) += 1;
                continue;
            }
            pollard_used = true;
            let divisor = pollard_brent(&current, guard)?;
            stack.push(divisor.clone());
            stack.push(current / divisor);
        }
    }
    Ok((factors, pollard_used))
}

fn divisors_from_factors(
    factors: &BTreeMap<BigInt, u32>,
    guard: &mut Guard<'_>,
) -> Result<Vec<BigInt>, EngineError> {
    let mut divisors = vec![BigInt::one()];
    for (prime, exponent) in factors {
        let base = divisors.clone();
        let mut power = BigInt::one();
        for _ in 0..*exponent {
            guard.iterate()?;
            power *= prime;
            for divisor in &base {
                divisors.push(divisor * &power);
            }
        }
    }
    Ok(divisors)
}

fn integer_nth_root(value: &BigInt, k: u32, guard: &mut Guard<'_>) -> Result<BigInt, EngineError> {
    if value.is_zero() {
        return Ok(BigInt::zero());
    }
    if k == 1 {
        return Ok(value.clone());
    }
    if value.is_one() {
        return Ok(BigInt::one());
    }
    let bits = value.bits();
    if u64::from(k) >= bits {
        return Ok(BigInt::one());
    }
    let mut x = BigInt::one() << bits.div_ceil(u64::from(k));
    loop {
        guard.iterate()?;
        let power = x.pow(k - 1);
        let quotient = value / &power;
        let next = ((k - 1) * &x + quotient) / k;
        if next >= x {
            break;
        }
        x = next;
    }
    Ok(x)
}

fn gcd_extended_bigint(
    a: &BigInt,
    b: &BigInt,
    guard: &mut Guard<'_>,
) -> Result<(BigInt, BigInt, BigInt), EngineError> {
    let mut old_r = a.clone();
    let mut r = b.clone();
    let mut old_s = BigInt::one();
    let mut s = BigInt::zero();
    let mut old_t = BigInt::zero();
    let mut t = BigInt::one();
    while !r.is_zero() {
        guard.iterate()?;
        let quotient = &old_r / &r;
        let next_r = &old_r - &quotient * &r;
        old_r = r;
        r = next_r;
        let next_s = &old_s - &quotient * &s;
        old_s = s;
        s = next_s;
        let next_t = &old_t - &quotient * &t;
        old_t = t;
        t = next_t;
    }
    if old_r.is_negative() {
        old_r = -old_r;
        old_s = -old_s;
        old_t = -old_t;
    }
    Ok((old_r, old_s, old_t))
}

fn crt_combine(
    residues: &[BigInt],
    moduli: &[BigInt],
    guard: &mut Guard<'_>,
) -> Result<BigInt, EngineError> {
    for modulus in moduli {
        if modulus.is_zero() {
            return Err(EngineError::domain("moduli must be non-zero"));
        }
    }
    let mut x = residues[0].mod_floor(&moduli[0].abs());
    let mut m = moduli[0].abs();
    for index in 1..residues.len() {
        guard.iterate()?;
        let residue = residues[index].mod_floor(&moduli[index].abs());
        let n = moduli[index].abs();
        let (g, p, _q) = gcd_extended_bigint(&m, &n, guard)?;
        let difference = &residue - &x;
        if !(&difference % &g).is_zero() {
            return Err(EngineError::domain(
                "the congruence system is inconsistent (moduli are not coprime and \
                 the residues disagree)",
            ));
        }
        let lcm = &m / &g * &n;
        let step = (&difference / &g * &p).mod_floor(&(&n / &g));
        x = (x + &m * step).mod_floor(&lcm);
        m = lcm;
    }
    Ok(x)
}

fn binomial_bigint(
    n: u64,
    k: u64,
    guard: &mut Guard<'_>,
    ctx: &ExecContext,
) -> Result<BigInt, EngineError> {
    let k = k.min(n - k);
    let mut result = BigInt::one();
    for index in 0..k {
        guard.tick()?;
        result = result * (n - index) / (index + 1);
        check_integer_bits(&result, ctx)?;
    }
    Ok(result)
}

fn factorial_bigint(
    n: u64,
    guard: &mut Guard<'_>,
    ctx: &ExecContext,
) -> Result<BigInt, EngineError> {
    let mut result = BigInt::one();
    for index in 2..=n {
        guard.tick()?;
        result *= index;
        check_integer_bits(&result, ctx)?;
    }
    Ok(result)
}

fn fib_pair(
    n: u64,
    guard: &mut Guard<'_>,
    ctx: &ExecContext,
) -> Result<(BigInt, BigInt), EngineError> {
    let mut a = BigInt::zero();
    let mut b = BigInt::one();
    let bits = 64 - n.leading_zeros();
    for bit in (0..bits).rev() {
        guard.iterate()?;
        let two_b = &b << 1u32;
        let c = &a * (&two_b - &a);
        let d = &a * &a + &b * &b;
        if (n >> bit) & 1 == 1 {
            b = &c + &d;
            a = d;
        } else {
            a = c;
            b = d;
        }
        check_integer_bits(&b, ctx)?;
    }
    Ok((a, b))
}

// ---------------------------------------------------------------------------
// Polynomial descriptors and invocations
// ---------------------------------------------------------------------------

fn polynomial_add_descriptor() -> FunctionDescriptor {
    base_descriptor(
        "polynomial_add",
        "Polynomial addition",
        "Add two exact polynomials.",
    )
    .with_description(
        "Coefficient arrays are indexed by degree with the constant term first and are \
         normalized by stripping trailing zeros. All arithmetic is exact over the integers \
         and rationals.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("a", "First coefficient array.", exact_array_schema()),
        ParamDescriptor::required("b", "Second coefficient array.", exact_array_schema()),
    ])
    .with_output(exact_array_schema(), "Sum a + b, normalized.")
    .with_cost(CostClass::Linear)
    .with_examples(vec![
        Example::new(
            "integer coefficients",
            example_args(&[("a", poly_value(&[1, 2])), ("b", poly_value(&[3, 4]))]),
        )
        .with_value(poly_value(&[4, 6])),
    ])
}

fn invoke_polynomial_add(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let a = Poly::new(rational_array(args, "a")?);
    let b = Poly::new(rational_array(args, "b")?);
    let mut guard = Guard::new(ctx);
    let sum = a.plus(&b);
    Ok(Outcome::exact(checked_poly_value(&sum, ctx, &mut guard)?))
}

fn polynomial_sub_descriptor() -> FunctionDescriptor {
    base_descriptor(
        "polynomial_sub",
        "Polynomial subtraction",
        "Subtract one exact polynomial from another.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("a", "Minuend coefficient array.", exact_array_schema()),
        ParamDescriptor::required("b", "Subtrahend coefficient array.", exact_array_schema()),
    ])
    .with_output(exact_array_schema(), "Difference a - b, normalized.")
    .with_cost(CostClass::Linear)
    .with_examples(vec![
        Example::new(
            "integer coefficients",
            example_args(&[("a", poly_value(&[1, 2])), ("b", poly_value(&[3, 4]))]),
        )
        .with_value(poly_value(&[-2, -2])),
    ])
}

fn invoke_polynomial_sub(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let a = Poly::new(rational_array(args, "a")?);
    let b = Poly::new(rational_array(args, "b")?);
    let mut guard = Guard::new(ctx);
    let difference = a.minus(&b);
    Ok(Outcome::exact(checked_poly_value(
        &difference,
        ctx,
        &mut guard,
    )?))
}

fn polynomial_mul_descriptor() -> FunctionDescriptor {
    base_descriptor(
        "polynomial_mul",
        "Polynomial multiplication",
        "Multiply two exact polynomials.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("a", "Left coefficient array.", exact_array_schema()),
        ParamDescriptor::required("b", "Right coefficient array.", exact_array_schema()),
    ])
    .with_output(exact_array_schema(), "Product a * b, normalized.")
    .with_cost(CostClass::Quadratic)
    .with_examples(vec![
        Example::new(
            "difference of squares",
            example_args(&[("a", poly_value(&[1, 1])), ("b", poly_value(&[1, -1]))]),
        )
        .with_value(poly_value(&[1, 0, -1])),
    ])
}

fn invoke_polynomial_mul(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let a = Poly::new(rational_array(args, "a")?);
    let b = Poly::new(rational_array(args, "b")?);
    let mut guard = Guard::new(ctx);
    let product = a.times(&b, &mut guard)?;
    Ok(Outcome::exact(checked_poly_value(
        &product, ctx, &mut guard,
    )?))
}

fn polynomial_divmod_descriptor() -> FunctionDescriptor {
    base_descriptor(
        "polynomial_divmod",
        "Polynomial division",
        "Divide two exact polynomials, returning quotient and remainder.",
    )
    .with_description(
        "Long division over the rationals. The remainder has degree strictly smaller than \
         the divisor; dividing by the zero polynomial is an error.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("a", "Dividend coefficient array.", exact_array_schema()),
        ParamDescriptor::required("b", "Divisor coefficient array.", exact_array_schema()),
    ])
    .with_output(
        divmod_schema(),
        "Quotient and remainder coefficient arrays, normalized.",
    )
    .with_cost(CostClass::Quadratic)
    .with_examples(vec![
        Example::new(
            "difference of squares divided by a linear factor",
            example_args(&[("a", poly_value(&[-1, 0, 1])), ("b", poly_value(&[-1, 1]))]),
        )
        .with_value(Value::record([
            ("quotient", poly_value(&[1, 1])),
            ("remainder", poly_value(&[0])),
        ])),
        Example::new(
            "division by the zero polynomial",
            example_args(&[("a", poly_value(&[1, 1])), ("b", poly_value(&[0]))]),
        )
        .with_error(ErrorCode::DivisionByZero),
    ])
}

fn invoke_polynomial_divmod(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let a = Poly::new(rational_array(args, "a")?);
    let b = Poly::new(rational_array(args, "b")?);
    let mut guard = Guard::new(ctx);
    let (quotient, remainder) = a.divmod(&b, &mut guard)?;
    let quotient_value = checked_poly_value(&quotient, ctx, &mut guard)?;
    let remainder_value = checked_poly_value(&remainder, ctx, &mut guard)?;
    Ok(Outcome::exact(Value::record([
        ("quotient", quotient_value),
        ("remainder", remainder_value),
    ])))
}

fn polynomial_gcd_descriptor() -> FunctionDescriptor {
    base_descriptor(
        "polynomial_gcd",
        "Polynomial GCD",
        "Monic greatest common divisor of two exact polynomials.",
    )
    .with_description(
        "The Euclidean algorithm over the rationals, normalized to a monic polynomial. \
         The GCD of two zero polynomials is the zero polynomial.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("a", "First coefficient array.", exact_array_schema()),
        ParamDescriptor::required("b", "Second coefficient array.", exact_array_schema()),
    ])
    .with_output(exact_array_schema(), "Monic GCD, normalized.")
    .with_cost(CostClass::Quadratic)
    .with_examples(vec![
        Example::new(
            "common linear factor",
            example_args(&[
                ("a", poly_value(&[-1, 0, 1])),
                ("b", poly_value(&[1, 2, 1])),
            ]),
        )
        .with_value(poly_value(&[1, 1])),
    ])
}

fn invoke_polynomial_gcd(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let a = Poly::new(rational_array(args, "a")?);
    let b = Poly::new(rational_array(args, "b")?);
    let mut guard = Guard::new(ctx);
    let gcd = Poly::gcd(&a, &b, &mut guard)?;
    Ok(Outcome::exact(checked_poly_value(&gcd, ctx, &mut guard)?))
}

fn polynomial_derivative_descriptor() -> FunctionDescriptor {
    base_descriptor(
        "polynomial_derivative",
        "Polynomial derivative",
        "Formal derivative of an exact polynomial.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "coefficients",
        "Coefficient array.",
        exact_array_schema(),
    )])
    .with_output(exact_array_schema(), "Derivative coefficients, normalized.")
    .with_cost(CostClass::Linear)
    .with_examples(vec![
        Example::new(
            "quadratic derivative",
            example_args(&[("coefficients", poly_value(&[1, 2, 3]))]),
        )
        .with_value(poly_value(&[2, 6])),
    ])
}

fn invoke_polynomial_derivative(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let poly = Poly::new(rational_array(args, "coefficients")?);
    let mut guard = Guard::new(ctx);
    let derivative = poly.derivative();
    Ok(Outcome::exact(checked_poly_value(
        &derivative,
        ctx,
        &mut guard,
    )?))
}

fn polynomial_evaluate_descriptor() -> FunctionDescriptor {
    base_descriptor(
        "polynomial_evaluate",
        "Polynomial evaluation",
        "Evaluate an exact polynomial at an exact point.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("coefficients", "Coefficient array.", exact_array_schema()),
        ParamDescriptor::required("x", "Evaluation point.", ValueSchema::exact()),
    ])
    .with_output(ValueSchema::exact(), "Exact value p(x).")
    .with_cost(CostClass::Linear)
    .with_examples(vec![
        Example::new(
            "evaluate a quadratic",
            example_args(&[
                ("coefficients", poly_value(&[1, 2, 3])),
                ("x", int_value(2)),
            ]),
        )
        .with_value(int_value(17)),
    ])
}

fn invoke_polynomial_evaluate(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let poly = Poly::new(rational_array(args, "coefficients")?);
    let x = exact_number(args, "x")?;
    let mut guard = Guard::new(ctx);
    let value = poly.evaluate(&x, &mut guard)?;
    check_rational_bits(&value, ctx)?;
    Ok(Outcome::exact(rational_to_value(&value)))
}

fn polynomial_roots_descriptor() -> FunctionDescriptor {
    base_descriptor(
        "polynomial_roots",
        "Polynomial roots",
        "Real roots of an exact polynomial of degree at most four.",
    )
    .with_description(
        "Rational roots are found exactly with the rational-root theorem and deflation. \
         Quadratic roots are exact when the discriminant is a perfect rational square. \
         Remaining irrational real roots are isolated numerically and reported as float64 \
         records with a warning; exact mode rejects them. Polynomials of degree greater \
         than four are unsupported.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "coefficients",
        "Coefficient array of degree at most four.",
        exact_array_schema(),
    )])
    .with_output(
        ValueSchema::array(root_schema()),
        "Real roots in ascending order, exact where possible.",
    )
    .with_cost(CostClass::Iterative)
    .with_examples(vec![
        Example::new(
            "rational roots",
            example_args(&[("coefficients", poly_value(&[2, -3, 1]))]),
        )
        .with_value(Value::Array(vec![exact_root_value(1), exact_root_value(2)])),
        Example::new(
            "irrational roots are approximate",
            example_args(&[("coefficients", poly_value(&[-2, 0, 1]))]),
        )
        .with_contains("approximate"),
        Example::new(
            "degree above four",
            example_args(&[("coefficients", poly_value(&[0, 0, 0, 0, 0, 1]))]),
        )
        .with_error(ErrorCode::UnsupportedOperation),
    ])
}

fn invoke_polynomial_roots(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let poly = Poly::new(rational_array(args, "coefficients")?);
    if poly.is_zero() {
        return Err(EngineError::unsupported(
            "the zero polynomial has infinitely many roots",
        ));
    }
    let degree = poly.degree().unwrap_or(0);
    if degree > 4 {
        return Err(EngineError::unsupported(format!(
            "polynomial degree {degree} exceeds the supported maximum of 4"
        )));
    }
    let mut guard = Guard::new(ctx);
    let roots = find_roots(&poly, &mut guard)?;
    let approximate_count = roots
        .iter()
        .filter(|root| matches!(root, Root::Approximate(_)))
        .count();
    if approximate_count > 0 && ctx.numeric.mode == NumericMode::Exact {
        return Err(EngineError::new(
            ErrorCode::UnsupportedNumericMode,
            "irrational roots cannot be represented exactly; use auto or scientific mode",
        ));
    }
    let values = roots
        .iter()
        .map(root_to_value)
        .collect::<Result<Vec<Value>, EngineError>>()?;
    let mut outcome = if approximate_count > 0 {
        Outcome::new(Value::Array(values), Exactness::Approximate)
    } else {
        Outcome::exact(Value::Array(values))
    };
    if approximate_count > 0 {
        outcome = outcome.with_warning(Warning::new(
            "irrational_roots_approximated",
            format!(
                "{approximate_count} irrational real root(s) reported as float64 approximations"
            ),
        ));
    }
    Ok(outcome)
}

fn polynomial_from_roots_descriptor() -> FunctionDescriptor {
    base_descriptor(
        "polynomial_from_roots",
        "Polynomial from roots",
        "Build an exact polynomial from its roots.",
    )
    .with_description(
        "Multiplies the factors (x - r) for each root, optionally scaled by a leading \
         coefficient (default 1).",
    )
    .with_parameters(vec![
        ParamDescriptor::required("roots", "Root list.", exact_array_schema()),
        ParamDescriptor::optional(
            "leading",
            "Leading coefficient; defaults to 1.",
            ValueSchema::exact(),
        ),
    ])
    .with_output(exact_array_schema(), "Expanded coefficients, normalized.")
    .with_cost(CostClass::Quadratic)
    .with_examples(vec![
        Example::new(
            "two integer roots",
            example_args(&[("roots", integer_array_value(&[1, 2]))]),
        )
        .with_value(poly_value(&[2, -3, 1])),
        Example::new(
            "zero leading coefficient",
            example_args(&[
                ("roots", integer_array_value(&[1])),
                ("leading", int_value(0)),
            ]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_polynomial_from_roots(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let roots = rational_array(args, "roots")?;
    let leading = match args.optional_number("leading")? {
        Some(number) => rational_number(number, "leading")?,
        None => BigRational::one(),
    };
    if leading.is_zero() {
        return Err(EngineError::domain(
            "the leading coefficient must not be zero",
        ));
    }
    let mut guard = Guard::new(ctx);
    let poly = Poly::from_roots(&roots, &leading, &mut guard)?;
    Ok(Outcome::exact(checked_poly_value(&poly, ctx, &mut guard)?))
}

// ---------------------------------------------------------------------------
// Number theory descriptors and invocations
// ---------------------------------------------------------------------------

fn is_prime_descriptor() -> FunctionDescriptor {
    base_descriptor(
        "is_prime",
        "Primality test",
        "Deterministic Miller-Rabin primality test.",
    )
    .with_description(
        "For n < 2^64 the twelve documented bases 2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31 \
         and 37 are a proven deterministic test. For larger n the first 64 primes are used \
         and the result is probable rather than proven, which is reported as a warning.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "n",
        "Integer to test.",
        integer_schema(),
    )])
    .with_output(ValueSchema::Bool, "True when n is prime.")
    .with_cost(CostClass::Iterative)
    .with_examples(vec![
        Example::new(
            "Mersenne prime 2^61 - 1",
            example_args(&[("n", integer_value(BigInt::from(2305843009213693951u64)))]),
        )
        .with_value(Value::Bool(true)),
        Example::new(
            "Carmichael number 561",
            example_args(&[("n", int_value(561))]),
        )
        .with_value(Value::Bool(false)),
    ])
}

fn invoke_is_prime(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let n = args.integer("n")?;
    let mut guard = Guard::new(ctx);
    let prime = is_prime_bigint(&n, &mut guard)?;
    let mut outcome = Outcome::exact(Value::Bool(prime));
    if n.bits() > 64 {
        outcome = outcome.with_warning(Warning::new(
            "probable_prime_large_input",
            "for n >= 2^64 the fixed-base Miller-Rabin test is probabilistic, not a proof",
        ));
    }
    Ok(outcome)
}

fn next_prime_descriptor() -> FunctionDescriptor {
    base_descriptor(
        "next_prime",
        "Next prime",
        "Smallest prime strictly greater than the input.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "n",
        "Integer lower bound.",
        integer_schema(),
    )])
    .with_output(integer_schema(), "Next prime after n.")
    .with_cost(CostClass::Iterative)
    .with_examples(vec![
        Example::new(
            "next prime after 100",
            example_args(&[("n", int_value(100))]),
        )
        .with_value(int_value(101)),
    ])
}

fn invoke_next_prime(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let n = args.integer("n")?;
    let mut guard = Guard::new(ctx);
    let prime = next_prime_bigint(&n, &mut guard)?;
    check_integer_bits(&prime, ctx)?;
    Ok(Outcome::exact(integer_value(prime)))
}

fn prime_factors_descriptor() -> FunctionDescriptor {
    base_descriptor(
        "prime_factors",
        "Prime factorization",
        "Complete factorization of a positive integer.",
    )
    .with_description(
        "Trial division up to 100000 is followed by deterministic-seed Pollard rho \
         (Brent's variant) for any remaining composite cofactor. The result lists prime \
         factors in ascending order with multiplicities and reports the method used.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "n",
        "Positive integer to factor.",
        integer_schema(),
    )])
    .with_output(prime_factors_schema(), "Prime factors and the method used.")
    .with_cost(CostClass::Iterative)
    .with_examples(vec![
        Example::new(
            "factorization of 360",
            example_args(&[("n", int_value(360))]),
        )
        .with_value(Value::record([
            (
                "factors",
                Value::Array(vec![
                    factor_value(2, 3),
                    factor_value(3, 2),
                    factor_value(5, 1),
                ]),
            ),
            ("method", Value::text("trial_division")),
        ])),
    ])
}

fn invoke_prime_factors(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let n = args.integer("n")?;
    let mut guard = Guard::new(ctx);
    let (factors, pollard_used) = factor_map(&n, &mut guard)?;
    let mut records = Vec::with_capacity(factors.len());
    for (prime, exponent) in &factors {
        guard.tick()?;
        records.push(Value::record([
            ("prime", integer_value(prime.clone())),
            ("exponent", integer_value(BigInt::from(*exponent))),
        ]));
    }
    let method = if pollard_used {
        "trial_division+pollard_rho_brent"
    } else {
        "trial_division"
    };
    Ok(Outcome::exact(Value::record([
        ("factors", Value::Array(records)),
        ("method", Value::text(method)),
    ])))
}

fn divisors_descriptor() -> FunctionDescriptor {
    base_descriptor(
        "divisors",
        "Divisors",
        "All positive divisors of a positive integer in ascending order.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "n",
        "Positive integer.",
        integer_schema(),
    )])
    .with_output(integer_array_schema(), "Positive divisors.")
    .with_cost(CostClass::Iterative)
    .with_examples(vec![
        Example::new("divisors of 12", example_args(&[("n", int_value(12))]))
            .with_value(integer_array_value(&[1, 2, 3, 4, 6, 12])),
    ])
}

fn invoke_divisors(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let n = args.integer("n")?;
    let mut guard = Guard::new(ctx);
    let (factors, _) = factor_map(&n, &mut guard)?;
    let mut divisors = divisors_from_factors(&factors, &mut guard)?;
    if divisors.len() > ctx.limits.max_array_len {
        return Err(EngineError::resource(format!(
            "divisor list of length {} exceeds the array limit of {}",
            divisors.len(),
            ctx.limits.max_array_len
        )));
    }
    divisors.sort();
    let values = divisors
        .into_iter()
        .map(integer_value)
        .collect::<Vec<Value>>();
    Ok(Outcome::exact(Value::Array(values)))
}

fn totient_descriptor() -> FunctionDescriptor {
    base_descriptor(
        "totient",
        "Euler totient",
        "Euler's totient function phi(n).",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "n",
        "Positive integer.",
        integer_schema(),
    )])
    .with_output(integer_schema(), "phi(n), the count of totatives of n.")
    .with_cost(CostClass::Iterative)
    .with_examples(vec![
        Example::new("totient of 9", example_args(&[("n", int_value(9))])).with_value(int_value(6)),
    ])
}

fn invoke_totient(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let n = args.integer("n")?;
    if n < BigInt::one() {
        return Err(EngineError::domain("totient requires a positive integer"));
    }
    let mut guard = Guard::new(ctx);
    let (factors, _) = factor_map(&n, &mut guard)?;
    let mut phi = n;
    for prime in factors.keys() {
        guard.tick()?;
        phi = phi / prime * (prime - 1u32);
    }
    check_integer_bits(&phi, ctx)?;
    Ok(Outcome::exact(integer_value(phi)))
}

fn is_perfect_square_descriptor() -> FunctionDescriptor {
    base_descriptor(
        "is_perfect_square",
        "Perfect square test",
        "Whether an integer is the square of an integer.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "n",
        "Integer to test.",
        integer_schema(),
    )])
    .with_output(ValueSchema::Bool, "True when n is a perfect square.")
    .with_cost(CostClass::Iterative)
    .with_examples(vec![
        Example::new("perfect square", example_args(&[("n", int_value(144))]))
            .with_value(Value::Bool(true)),
        Example::new("non-square", example_args(&[("n", int_value(145))]))
            .with_value(Value::Bool(false)),
    ])
}

fn invoke_is_perfect_square(args: &Args, _ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let n = args.integer("n")?;
    let square = if n.is_negative() {
        false
    } else {
        let root = n.sqrt();
        &root * &root == n
    };
    Ok(Outcome::exact(Value::Bool(square)))
}

fn integer_nth_root_descriptor() -> FunctionDescriptor {
    base_descriptor(
        "integer_nth_root",
        "Integer nth root",
        "Floor of the nth root of an integer.",
    )
    .with_description(
        "For negative inputs the root index must be odd. The result is the floor of the \
         real nth root, computed by integer Newton iteration.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("n", "Integer radicand.", integer_schema()),
        ParamDescriptor::required("k", "Root index, at least 1.", integer_schema()),
    ])
    .with_output(integer_schema(), "Floor(n^(1/k)).")
    .with_cost(CostClass::Iterative)
    .with_examples(vec![
        Example::new(
            "cube root of 27",
            example_args(&[("n", int_value(27)), ("k", int_value(3))]),
        )
        .with_value(int_value(3)),
        Example::new(
            "even root of a negative number",
            example_args(&[("n", int_value(-4)), ("k", int_value(2))]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_integer_nth_root(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let n = args.integer("n")?;
    let k = args.integer("k")?;
    let k = k
        .to_u32()
        .ok_or_else(|| EngineError::domain("root index must be a positive 32-bit integer"))?;
    if k == 0 {
        return Err(EngineError::domain("root index must be at least 1"));
    }
    if k > ctx.limits.max_exponent {
        return Err(EngineError::resource(format!(
            "root index {k} exceeds the configured exponent limit of {}",
            ctx.limits.max_exponent
        )));
    }
    if n.is_negative() && k.is_multiple_of(2) {
        return Err(EngineError::domain(
            "an even root of a negative number is not real",
        ));
    }
    let mut guard = Guard::new(ctx);
    let magnitude = n.abs();
    let root = integer_nth_root(&magnitude, k, &mut guard)?;
    let result = if n.is_negative() { -root } else { root };
    Ok(Outcome::exact(integer_value(result)))
}

fn gcd_extended_descriptor() -> FunctionDescriptor {
    base_descriptor(
        "gcd_extended",
        "Extended GCD",
        "Greatest common divisor with Bezout coefficients.",
    )
    .with_description(
        "Returns gcd, x and y such that a*x + b*y = gcd(a, b), with gcd always \
         non-negative.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("a", "First integer.", integer_schema()),
        ParamDescriptor::required("b", "Second integer.", integer_schema()),
    ])
    .with_output(gcd_extended_schema(), "GCD and Bezout coefficients.")
    .with_cost(CostClass::Iterative)
    .with_examples(vec![
        Example::new(
            "Bezout identity for 240 and 46",
            example_args(&[("a", int_value(240)), ("b", int_value(46))]),
        )
        .with_value(Value::record([
            ("gcd", int_value(2)),
            ("x", int_value(-9)),
            ("y", int_value(47)),
        ])),
    ])
}

fn invoke_gcd_extended(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let a = args.integer("a")?;
    let b = args.integer("b")?;
    let mut guard = Guard::new(ctx);
    let (gcd, x, y) = gcd_extended_bigint(&a, &b, &mut guard)?;
    Ok(Outcome::exact(Value::record([
        ("gcd", integer_value(gcd)),
        ("x", integer_value(x)),
        ("y", integer_value(y)),
    ])))
}

fn mod_inverse_descriptor() -> FunctionDescriptor {
    base_descriptor(
        "mod_inverse",
        "Modular inverse",
        "Multiplicative inverse modulo m.",
    )
    .with_description(
        "Returns the inverse of a modulo |m| in the range [0, |m|). A domain error is \
         reported when gcd(a, m) != 1.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("a", "Value to invert.", integer_schema()),
        ParamDescriptor::required("m", "Non-zero modulus.", integer_schema()),
    ])
    .with_output(integer_schema(), "Inverse of a modulo |m|.")
    .with_cost(CostClass::Iterative)
    .with_examples(vec![
        Example::new(
            "inverse of 3 modulo 11",
            example_args(&[("a", int_value(3)), ("m", int_value(11))]),
        )
        .with_value(int_value(4)),
        Example::new(
            "non-invertible value",
            example_args(&[("a", int_value(6)), ("m", int_value(9))]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_mod_inverse(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let a = args.integer("a")?;
    let m = args.integer("m")?;
    if m.is_zero() {
        return Err(EngineError::domain("modulus must be non-zero"));
    }
    let modulus = m.abs();
    let mut guard = Guard::new(ctx);
    let (gcd, x, _y) = gcd_extended_bigint(&a, &modulus, &mut guard)?;
    if !gcd.is_one() {
        return Err(EngineError::domain(format!(
            "{a} has no inverse modulo {modulus}: gcd is {gcd}"
        )));
    }
    Ok(Outcome::exact(integer_value(x.mod_floor(&modulus))))
}

fn mod_pow_descriptor() -> FunctionDescriptor {
    base_descriptor(
        "mod_pow",
        "Modular exponentiation",
        "Exact modular power with square-and-multiply.",
    )
    .with_description(
        "Returns base^exponent mod |modulus| in the range [0, |modulus|). Negative \
         exponents invert the base first and require it to be invertible.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("base", "Base.", integer_schema()),
        ParamDescriptor::required("exponent", "Exponent.", integer_schema()),
        ParamDescriptor::required("modulus", "Non-zero modulus.", integer_schema()),
    ])
    .with_output(integer_schema(), "base^exponent mod |modulus|.")
    .with_cost(CostClass::Iterative)
    .with_examples(vec![
        Example::new(
            "2^10 modulo 1000",
            example_args(&[
                ("base", int_value(2)),
                ("exponent", int_value(10)),
                ("modulus", int_value(1000)),
            ]),
        )
        .with_value(int_value(24)),
    ])
}

fn invoke_mod_pow(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let base = args.integer("base")?;
    let exponent = args.integer("exponent")?;
    let modulus = args.integer("modulus")?;
    if modulus.is_zero() {
        return Err(EngineError::domain("modulus must be non-zero"));
    }
    let modulus = modulus.abs();
    if modulus.is_one() {
        return Ok(Outcome::exact(integer_value(BigInt::zero())));
    }
    let mut guard = Guard::new(ctx);
    let mut base = base.mod_floor(&modulus);
    let mut exponent = exponent;
    if exponent.is_negative() {
        let (gcd, inverse, _y) = gcd_extended_bigint(&base, &modulus, &mut guard)?;
        if !gcd.is_one() {
            return Err(EngineError::domain(
                "negative exponent requires an invertible base",
            ));
        }
        base = inverse.mod_floor(&modulus);
        exponent = -exponent;
    }
    guard.tick()?;
    Ok(Outcome::exact(integer_value(
        base.modpow(&exponent, &modulus),
    )))
}

fn crt_descriptor() -> FunctionDescriptor {
    base_descriptor(
        "crt",
        "Chinese remainder theorem",
        "Solve a system of simultaneous congruences.",
    )
    .with_description(
        "Generalized CRT for non-necessarily-coprime moduli. Returns the least \
         non-negative solution modulo the least common multiple of the moduli. An \
         inconsistent system is a domain error.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("residues", "Residues.", integer_array_schema()),
        ParamDescriptor::required("moduli", "Non-zero moduli.", integer_array_schema()),
    ])
    .with_output(
        integer_schema(),
        "Least non-negative solution modulo lcm(moduli).",
    )
    .with_cost(CostClass::Iterative)
    .with_examples(vec![
        Example::new(
            "classic three-modulus system",
            example_args(&[
                ("residues", integer_array_value(&[2, 3, 2])),
                ("moduli", integer_array_value(&[3, 5, 7])),
            ]),
        )
        .with_value(int_value(23)),
        Example::new(
            "empty system",
            example_args(&[
                ("residues", Value::Array(Vec::new())),
                ("moduli", Value::Array(Vec::new())),
            ]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_crt(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let residues = integer_array(args, "residues")?;
    let moduli = integer_array(args, "moduli")?;
    if residues.is_empty() {
        return Err(EngineError::domain(
            "the congruence system must contain at least one congruence",
        ));
    }
    if residues.len() != moduli.len() {
        return Err(EngineError::malformed(format!(
            "residues and moduli must have the same length ({} vs {})",
            residues.len(),
            moduli.len()
        )));
    }
    let mut guard = Guard::new(ctx);
    let result = crt_combine(&residues, &moduli, &mut guard)?;
    Ok(Outcome::exact(integer_value(result)))
}

// ---------------------------------------------------------------------------
// Combinatorics descriptors and invocations
// ---------------------------------------------------------------------------

fn permutations_descriptor() -> FunctionDescriptor {
    base_descriptor(
        "permutations",
        "Permutations",
        "Number of ordered arrangements P(n, k).",
    )
    .with_description(
        "P(n, k) = n! / (n - k)! for 0 <= k <= n, and 0 when k > n. The result is exact \
         and bounded by the configured factorial and integer bit limits.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("n", "Number of items, n >= 0.", integer_schema()),
        ParamDescriptor::required("k", "Number selected, k >= 0.", integer_schema()),
    ])
    .with_output(integer_schema(), "P(n, k).")
    .with_cost(CostClass::Linear)
    .with_examples(vec![
        Example::new(
            "arrangements of 5 items taken 3 at a time",
            example_args(&[("n", int_value(5)), ("k", int_value(3))]),
        )
        .with_value(int_value(60)),
        Example::new(
            "k greater than n",
            example_args(&[("n", int_value(3)), ("k", int_value(5))]),
        )
        .with_value(int_value(0)),
    ])
}

fn invoke_permutations(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let n = args.integer("n")?;
    let k = args.integer("k")?;
    if n.is_negative() || k.is_negative() {
        return Err(EngineError::domain(
            "permutations require non-negative arguments",
        ));
    }
    if k > n {
        return Ok(Outcome::exact(integer_value(BigInt::zero())));
    }
    let n = n
        .to_u64()
        .ok_or_else(|| EngineError::resource("n is too large for permutations"))?;
    let k = k
        .to_u64()
        .ok_or_else(|| EngineError::resource("k is too large for permutations"))?;
    if n > u64::from(ctx.limits.max_factorial) {
        return Err(EngineError::resource(format!(
            "n={n} exceeds the configured factorial limit of {}",
            ctx.limits.max_factorial
        )));
    }
    let mut guard = Guard::new(ctx);
    let mut result = BigInt::one();
    for index in 0..k {
        guard.tick()?;
        result *= n - index;
        check_integer_bits(&result, ctx)?;
    }
    Ok(Outcome::exact(integer_value(result)))
}

fn multinomial_descriptor() -> FunctionDescriptor {
    base_descriptor(
        "multinomial",
        "Multinomial coefficient",
        "Number of ways to partition items into labelled groups.",
    )
    .with_description(
        "Computes (sum counts)! / product(counts!) for non-negative counts. The total is \
         bounded by the configured factorial limit.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "counts",
        "Non-negative group sizes.",
        integer_array_schema(),
    )])
    .with_output(integer_schema(), "Multinomial coefficient.")
    .with_cost(CostClass::Linear)
    .with_examples(vec![
        Example::new(
            "group sizes 2, 1, 1",
            example_args(&[("counts", integer_array_value(&[2, 1, 1]))]),
        )
        .with_value(int_value(12)),
    ])
}

fn invoke_multinomial(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let counts = integer_array(args, "counts")?;
    let mut total = BigInt::zero();
    for count in &counts {
        if count.is_negative() {
            return Err(EngineError::domain(
                "multinomial counts must be non-negative",
            ));
        }
        total += count;
    }
    let total = total
        .to_u64()
        .ok_or_else(|| EngineError::resource("multinomial total is too large"))?;
    if total > u64::from(ctx.limits.max_factorial) {
        return Err(EngineError::resource(format!(
            "multinomial total {total} exceeds the configured factorial limit of {}",
            ctx.limits.max_factorial
        )));
    }
    let mut guard = Guard::new(ctx);
    let mut result = factorial_bigint(total, &mut guard, ctx)?;
    for count in &counts {
        let count = count
            .to_u64()
            .ok_or_else(|| EngineError::resource("multinomial count is too large"))?;
        result /= factorial_bigint(count, &mut guard, ctx)?;
    }
    check_integer_bits(&result, ctx)?;
    Ok(Outcome::exact(integer_value(result)))
}

fn catalan_descriptor() -> FunctionDescriptor {
    base_descriptor("catalan", "Catalan number", "The nth Catalan number C(n).")
        .with_description(
            "C(n) = binom(2n, n) / (n + 1), computed exactly. The index is bounded by half \
         the configured factorial limit.",
        )
        .with_parameters(vec![ParamDescriptor::required(
            "n",
            "Non-negative index.",
            integer_schema(),
        )])
        .with_output(integer_schema(), "C(n).")
        .with_cost(CostClass::Linear)
        .with_examples(vec![
            Example::new(
                "tenth Catalan number",
                example_args(&[("n", int_value(10))]),
            )
            .with_value(int_value(16796)),
        ])
}

fn invoke_catalan(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let n = args.integer("n")?;
    if n.is_negative() {
        return Err(EngineError::domain("catalan requires a non-negative index"));
    }
    let n = n
        .to_u64()
        .ok_or_else(|| EngineError::resource("catalan index is too large"))?;
    let maximum = u64::from(ctx.limits.max_factorial) / 2;
    if n > maximum {
        return Err(EngineError::resource(format!(
            "catalan index {n} exceeds the configured maximum of {maximum}"
        )));
    }
    let mut guard = Guard::new(ctx);
    let central = binomial_bigint(2 * n, n, &mut guard, ctx)?;
    let result = central / (n + 1);
    check_integer_bits(&result, ctx)?;
    Ok(Outcome::exact(integer_value(result)))
}

fn partition_count_descriptor() -> FunctionDescriptor {
    base_descriptor(
        "partition_count",
        "Partition count",
        "Number of integer partitions p(n).",
    )
    .with_description(
        "Euler's pentagonal number recurrence with a bounded table. The index is limited \
         to 10000 and results are bounded by the configured integer bit limit.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "n",
        "Non-negative integer.",
        integer_schema(),
    )])
    .with_output(integer_schema(), "p(n).")
    .with_cost(CostClass::Quadratic)
    .with_examples(vec![
        Example::new("partitions of 100", example_args(&[("n", int_value(100))]))
            .with_value(int_value(190569292)),
    ])
}

fn invoke_partition_count(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let n = args.integer("n")?;
    if n.is_negative() {
        return Err(EngineError::domain(
            "partition_count requires a non-negative integer",
        ));
    }
    let n = n
        .to_u64()
        .ok_or_else(|| EngineError::resource("partition index is too large"))?;
    let maximum = MAX_PARTITION_N.min(u64::from(ctx.limits.max_factorial));
    if n > maximum {
        return Err(EngineError::resource(format!(
            "partition index {n} exceeds the supported maximum of {maximum}"
        )));
    }
    let n = n as usize;
    let mut guard = Guard::new(ctx);
    let mut partitions = vec![BigInt::zero(); n + 1];
    partitions[0] = BigInt::one();
    for index in 1..=n {
        let mut total = BigInt::zero();
        let mut k: u64 = 1;
        loop {
            guard.iterate()?;
            let first = k * (3 * k - 1) / 2;
            if first > index as u64 {
                break;
            }
            let positive = k % 2 == 1;
            let term = partitions[index - first as usize].clone();
            if positive {
                total += term;
            } else {
                total -= term;
            }
            let second = k * (3 * k + 1) / 2;
            if second <= index as u64 {
                let term = partitions[index - second as usize].clone();
                if positive {
                    total += term;
                } else {
                    total -= term;
                }
            }
            k += 1;
        }
        check_integer_bits(&total, ctx)?;
        partitions[index] = total;
    }
    Ok(Outcome::exact(integer_value(partitions[n].clone())))
}

fn stirling_second_descriptor() -> FunctionDescriptor {
    base_descriptor(
        "stirling_second",
        "Stirling number of the second kind",
        "Number of partitions of n items into k non-empty subsets.",
    )
    .with_description(
        "Computed with the recurrence S(n, k) = k*S(n-1, k) + S(n-1, k-1) using exact \
         integers. Work is bounded by the operation and iteration budgets.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("n", "Number of items, n >= 0.", integer_schema()),
        ParamDescriptor::required("k", "Number of subsets, k >= 0.", integer_schema()),
    ])
    .with_output(integer_schema(), "S(n, k).")
    .with_cost(CostClass::Quadratic)
    .with_examples(vec![
        Example::new(
            "five items in three subsets",
            example_args(&[("n", int_value(5)), ("k", int_value(3))]),
        )
        .with_value(int_value(25)),
    ])
}

fn invoke_stirling_second(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let n = args.integer("n")?;
    let k = args.integer("k")?;
    if n.is_negative() || k.is_negative() {
        return Err(EngineError::domain(
            "stirling_second requires non-negative arguments",
        ));
    }
    if k > n {
        return Ok(Outcome::exact(integer_value(BigInt::zero())));
    }
    let n = n
        .to_usize()
        .ok_or_else(|| EngineError::resource("n is too large for stirling_second"))?;
    let k = k
        .to_usize()
        .ok_or_else(|| EngineError::resource("k is too large for stirling_second"))?;
    let mut guard = Guard::new(ctx);
    let mut row = vec![BigInt::zero(); k + 1];
    row[0] = BigInt::one();
    for item in 1..=n {
        let upper = item.min(k);
        for subset in (1..=upper).rev() {
            guard.tick()?;
            let previous = row[subset].clone();
            row[subset] = previous * subset + &row[subset - 1];
        }
        row[0] = BigInt::zero();
        check_integer_bits(&row[upper], ctx)?;
    }
    Ok(Outcome::exact(integer_value(row[k].clone())))
}

fn bell_descriptor() -> FunctionDescriptor {
    base_descriptor(
        "bell",
        "Bell number",
        "The nth Bell number (all set partitions of n items).",
    )
    .with_description(
        "Computed with the Bell triangle using exact integers. Work is bounded by the \
         operation and iteration budgets and by the integer bit limit.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "n",
        "Non-negative index.",
        integer_schema(),
    )])
    .with_output(integer_schema(), "B(n).")
    .with_cost(CostClass::Quadratic)
    .with_examples(vec![
        Example::new("fifth Bell number", example_args(&[("n", int_value(5))]))
            .with_value(int_value(52)),
    ])
}

fn invoke_bell(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let n = args.integer("n")?;
    if n.is_negative() {
        return Err(EngineError::domain("bell requires a non-negative index"));
    }
    let n = n
        .to_usize()
        .ok_or_else(|| EngineError::resource("bell index is too large"))?;
    let mut guard = Guard::new(ctx);
    let mut row = vec![BigInt::one()];
    for size in 1..=n {
        let mut next = Vec::with_capacity(size + 1);
        next.push(row[size - 1].clone());
        for index in 1..=size {
            guard.tick()?;
            let value = &next[index - 1] + &row[index - 1];
            next.push(value);
        }
        check_integer_bits(next.last().expect("bell row is non-empty"), ctx)?;
        row = next;
    }
    Ok(Outcome::exact(integer_value(row[0].clone())))
}

fn fibonacci_descriptor() -> FunctionDescriptor {
    base_descriptor(
        "fibonacci",
        "Fibonacci number",
        "Fibonacci number with fast doubling and negative indices.",
    )
    .with_description(
        "F(n) is computed by fast doubling in O(log |n|) exact integer operations. \
         Negative indices use F(-n) = (-1)^(n+1) F(n). Indices are bounded by the integer \
         bit limit.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "n",
        "Integer index.",
        integer_schema(),
    )])
    .with_output(integer_schema(), "F(n).")
    .with_cost(CostClass::Iterative)
    .with_examples(vec![
        Example::new(
            "hundredth Fibonacci number",
            example_args(&[("n", int_value(100))]),
        )
        .with_value(integer_value(BigInt::from(354224848179261915075u128))),
        Example::new("negative index", example_args(&[("n", int_value(-7))]))
            .with_value(int_value(13)),
    ])
}

fn invoke_fibonacci(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let n = args.integer("n")?;
    let negative = n.is_negative();
    let magnitude = n.abs();
    let magnitude = magnitude
        .to_u64()
        .ok_or_else(|| EngineError::resource("fibonacci index is too large"))?;
    if magnitude > MAX_FIB_INDEX {
        return Err(EngineError::resource(format!(
            "fibonacci index {magnitude} exceeds the supported maximum of {MAX_FIB_INDEX}"
        )));
    }
    let mut guard = Guard::new(ctx);
    let (value, _next) = fib_pair(magnitude, &mut guard, ctx)?;
    let result = if negative && magnitude.is_multiple_of(2) {
        -value
    } else {
        value
    };
    check_integer_bits(&result, ctx)?;
    Ok(Outcome::exact(integer_value(result)))
}

fn lucas_descriptor() -> FunctionDescriptor {
    base_descriptor(
        "lucas",
        "Lucas number",
        "Lucas number with fast doubling and negative indices.",
    )
    .with_description(
        "L(n) = 2F(n+1) - F(n), computed exactly. Negative indices use \
         L(-n) = (-1)^n L(n). Indices are bounded by the integer bit limit.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "n",
        "Integer index.",
        integer_schema(),
    )])
    .with_output(integer_schema(), "L(n).")
    .with_cost(CostClass::Iterative)
    .with_examples(vec![
        Example::new("tenth Lucas number", example_args(&[("n", int_value(10))]))
            .with_value(int_value(123)),
    ])
}

fn invoke_lucas(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let n = args.integer("n")?;
    let negative = n.is_negative();
    let magnitude = n.abs();
    let magnitude = magnitude
        .to_u64()
        .ok_or_else(|| EngineError::resource("lucas index is too large"))?;
    if magnitude > MAX_FIB_INDEX {
        return Err(EngineError::resource(format!(
            "lucas index {magnitude} exceeds the supported maximum of {MAX_FIB_INDEX}"
        )));
    }
    let mut guard = Guard::new(ctx);
    let (value, next) = fib_pair(magnitude, &mut guard, ctx)?;
    let mut result = &next * 2u32 - &value;
    if negative && !magnitude.is_multiple_of(2) {
        result = -result;
    }
    check_integer_bits(&result, ctx)?;
    Ok(Outcome::exact(integer_value(result)))
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

/// Build the algebra module with all of its registered functions.
pub fn module() -> Module {
    let functions: Vec<Arc<dyn Function>> = vec![
        SimpleFunction::arc(polynomial_add_descriptor(), invoke_polynomial_add),
        SimpleFunction::arc(polynomial_sub_descriptor(), invoke_polynomial_sub),
        SimpleFunction::arc(polynomial_mul_descriptor(), invoke_polynomial_mul),
        SimpleFunction::arc(polynomial_divmod_descriptor(), invoke_polynomial_divmod),
        SimpleFunction::arc(polynomial_gcd_descriptor(), invoke_polynomial_gcd),
        SimpleFunction::arc(
            polynomial_derivative_descriptor(),
            invoke_polynomial_derivative,
        ),
        SimpleFunction::arc(polynomial_evaluate_descriptor(), invoke_polynomial_evaluate),
        SimpleFunction::arc(polynomial_roots_descriptor(), invoke_polynomial_roots),
        SimpleFunction::arc(
            polynomial_from_roots_descriptor(),
            invoke_polynomial_from_roots,
        ),
        SimpleFunction::arc(is_prime_descriptor(), invoke_is_prime),
        SimpleFunction::arc(next_prime_descriptor(), invoke_next_prime),
        SimpleFunction::arc(prime_factors_descriptor(), invoke_prime_factors),
        SimpleFunction::arc(divisors_descriptor(), invoke_divisors),
        SimpleFunction::arc(totient_descriptor(), invoke_totient),
        SimpleFunction::arc(is_perfect_square_descriptor(), invoke_is_perfect_square),
        SimpleFunction::arc(integer_nth_root_descriptor(), invoke_integer_nth_root),
        SimpleFunction::arc(gcd_extended_descriptor(), invoke_gcd_extended),
        SimpleFunction::arc(mod_inverse_descriptor(), invoke_mod_inverse),
        SimpleFunction::arc(mod_pow_descriptor(), invoke_mod_pow),
        SimpleFunction::arc(crt_descriptor(), invoke_crt),
        SimpleFunction::arc(permutations_descriptor(), invoke_permutations),
        SimpleFunction::arc(multinomial_descriptor(), invoke_multinomial),
        SimpleFunction::arc(catalan_descriptor(), invoke_catalan),
        SimpleFunction::arc(partition_count_descriptor(), invoke_partition_count),
        SimpleFunction::arc(stirling_second_descriptor(), invoke_stirling_second),
        SimpleFunction::arc(bell_descriptor(), invoke_bell),
        SimpleFunction::arc(fibonacci_descriptor(), invoke_fibonacci),
        SimpleFunction::arc(lucas_descriptor(), invoke_lucas),
    ];
    let descriptor = ModuleDescriptor::new(
        "algebra",
        "Algebra",
        "1.0.0",
        "Exact polynomial algebra, number theory, and combinatorics over integers and \
         rationals.",
    )
    .with_capabilities(vec![
        "exact_polynomials",
        "number_theory",
        "combinatorics",
        "exact_roots",
    ])
    .with_dependencies(vec!["core"])
    .with_modes(all_modes())
    .with_source("crates/bicmath-algebra");
    Module::new(descriptor, functions)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    fn ctx() -> ExecContext {
        ExecContext::conservative()
    }

    fn call(id: &str, raw: serde_json::Value) -> Result<Outcome, EngineError> {
        let module = module();
        let function = module
            .functions
            .iter()
            .find(|function| function.descriptor().id == id)
            .unwrap_or_else(|| panic!("function {id} is not registered"));
        let arguments = raw.as_object().expect("arguments must be an object");
        let mut values = BTreeMap::new();
        for (name, raw_value) in arguments {
            let parameter = function
                .descriptor()
                .parameter(name)
                .unwrap_or_else(|| panic!("parameter {name} is not declared"));
            let value = parameter
                .schema
                .coerce(raw_value, name, &ctx().limits, true)
                .unwrap_or_else(|error| panic!("argument {name} does not coerce: {error}"));
            values.insert(name.clone(), value);
        }
        function.invoke(&Args::new(values), &ctx())
    }

    fn parse_value(raw: serde_json::Value) -> Value {
        serde_json::from_value(raw).expect("value parses")
    }

    fn integer_result(outcome: &Outcome) -> BigInt {
        match &outcome.value {
            Value::Number(Number::Integer(value)) => value.clone(),
            other => panic!("expected an integer result, got {other:?}"),
        }
    }

    fn bool_result(outcome: &Outcome) -> bool {
        match &outcome.value {
            Value::Bool(value) => *value,
            other => panic!("expected a boolean result, got {other:?}"),
        }
    }

    fn array_result(outcome: &Outcome) -> Vec<Value> {
        match &outcome.value {
            Value::Array(items) => items.clone(),
            other => panic!("expected an array result, got {other:?}"),
        }
    }

    fn record_result(outcome: &Outcome) -> BTreeMap<String, Value> {
        match &outcome.value {
            Value::Record(fields) => fields.clone(),
            other => panic!("expected a record result, got {other:?}"),
        }
    }

    fn integer_field(fields: &BTreeMap<String, Value>, name: &str) -> BigInt {
        match fields.get(name) {
            Some(Value::Number(Number::Integer(value))) => value.clone(),
            other => panic!("expected integer field {name}, got {other:?}"),
        }
    }

    fn root_exact_rational(value: &Value) -> BigRational {
        match value {
            Value::Record(fields) => {
                assert_eq!(fields.get("exact"), Some(&Value::Bool(true)));
                match fields.get("value") {
                    Some(Value::Number(Number::Integer(number))) => {
                        BigRational::from_integer(number.clone())
                    }
                    Some(Value::Number(Number::Rational(number))) => number.clone(),
                    other => panic!("expected an exact root, got {other:?}"),
                }
            }
            other => panic!("expected a root record, got {other:?}"),
        }
    }

    fn factor_pairs(outcome: &Outcome) -> Vec<(BigInt, u32)> {
        let fields = record_result(outcome);
        let Some(Value::Array(items)) = fields.get("factors") else {
            panic!("expected a factors array");
        };
        items
            .iter()
            .map(|item| match item {
                Value::Record(record) => {
                    let prime = match record.get("prime") {
                        Some(Value::Number(Number::Integer(value))) => value.clone(),
                        other => panic!("expected prime, got {other:?}"),
                    };
                    let exponent = match record.get("exponent") {
                        Some(Value::Number(Number::Integer(value))) => {
                            value.to_u32().expect("exponent fits")
                        }
                        other => panic!("expected exponent, got {other:?}"),
                    };
                    (prime, exponent)
                }
                other => panic!("expected a factor record, got {other:?}"),
            })
            .collect()
    }

    #[test]
    fn polynomial_multiplication_then_division_is_identity() {
        let a = serde_json::json!([1, 2, 3]);
        let b = serde_json::json!([2, -1]);
        let product = call(
            "algebra.polynomial_mul",
            serde_json::json!({"a": a.clone(), "b": b.clone()}),
        )
        .unwrap();
        let product_json = serde_json::to_value(&product.value).unwrap();
        let divided = call(
            "algebra.polynomial_divmod",
            serde_json::json!({"a": product_json, "b": b}),
        )
        .unwrap();
        let fields = record_result(&divided);
        assert_eq!(fields.get("quotient"), Some(&parse_value(a)));
        assert_eq!(fields.get("remainder"), Some(&poly_value(&[0])));
    }

    #[test]
    fn polynomial_gcd_of_square_minus_one_and_square() {
        let outcome = call(
            "algebra.polynomial_gcd",
            serde_json::json!({"a": [-1, 0, 1], "b": [1, 2, 1]}),
        )
        .unwrap();
        assert_eq!(outcome.value, poly_value(&[1, 1]));
    }

    #[test]
    fn quadratic_roots_are_exact() {
        let outcome = call(
            "algebra.polynomial_roots",
            serde_json::json!({"coefficients": [2, -3, 1]}),
        )
        .unwrap();
        let roots: Vec<BigRational> = array_result(&outcome)
            .iter()
            .map(root_exact_rational)
            .collect();
        assert_eq!(
            roots,
            vec![
                BigRational::from_integer(BigInt::from(1)),
                BigRational::from_integer(BigInt::from(2))
            ]
        );
    }

    #[test]
    fn cubic_roots_are_exact() {
        let outcome = call(
            "algebra.polynomial_roots",
            serde_json::json!({"coefficients": [0, -1, 0, 1]}),
        )
        .unwrap();
        let roots: Vec<BigRational> = array_result(&outcome)
            .iter()
            .map(root_exact_rational)
            .collect();
        assert_eq!(
            roots,
            vec![
                BigRational::from_integer(BigInt::from(-1)),
                BigRational::zero(),
                BigRational::from_integer(BigInt::from(1))
            ]
        );
    }

    #[test]
    fn mersenne_prime_and_carmichael_number() {
        let prime = call(
            "algebra.is_prime",
            serde_json::json!({"n": "2305843009213693951"}),
        )
        .unwrap();
        assert!(bool_result(&prime));
        let composite = call("algebra.is_prime", serde_json::json!({"n": 561})).unwrap();
        assert!(!bool_result(&composite));
    }

    #[test]
    fn factorization_of_360() {
        let outcome = call("algebra.prime_factors", serde_json::json!({"n": 360})).unwrap();
        assert_eq!(
            factor_pairs(&outcome),
            vec![
                (BigInt::from(2), 3),
                (BigInt::from(3), 2),
                (BigInt::from(5), 1)
            ]
        );
        let fields = record_result(&outcome);
        assert_eq!(fields.get("method"), Some(&Value::text("trial_division")));
    }

    #[test]
    fn extended_gcd_identity() {
        let outcome = call(
            "algebra.gcd_extended",
            serde_json::json!({"a": 240, "b": 46}),
        )
        .unwrap();
        let fields = record_result(&outcome);
        let gcd = integer_field(&fields, "gcd");
        let x = integer_field(&fields, "x");
        let y = integer_field(&fields, "y");
        assert_eq!(gcd, BigInt::from(2));
        assert_eq!(
            BigInt::from(240) * &x + BigInt::from(46) * &y,
            BigInt::from(2)
        );
    }

    #[test]
    fn modular_inverse_power_and_crt() {
        let inverse = call("algebra.mod_inverse", serde_json::json!({"a": 3, "m": 11})).unwrap();
        assert_eq!(integer_result(&inverse), BigInt::from(4));
        let power = call(
            "algebra.mod_pow",
            serde_json::json!({"base": 2, "exponent": 10, "modulus": 1000}),
        )
        .unwrap();
        assert_eq!(integer_result(&power), BigInt::from(24));
        let crt = call(
            "algebra.crt",
            serde_json::json!({"residues": [2, 3, 2], "moduli": [3, 5, 7]}),
        )
        .unwrap();
        assert_eq!(integer_result(&crt), BigInt::from(23));
    }

    #[test]
    fn combinatorics_reference_values() {
        let catalan = call("algebra.catalan", serde_json::json!({"n": 10})).unwrap();
        assert_eq!(integer_result(&catalan), BigInt::from(16796));
        let partitions = call("algebra.partition_count", serde_json::json!({"n": 100})).unwrap();
        assert_eq!(integer_result(&partitions), BigInt::from(190569292));
        let fibonacci = call("algebra.fibonacci", serde_json::json!({"n": 100})).unwrap();
        assert_eq!(
            integer_result(&fibonacci),
            BigInt::from(354224848179261915075u128)
        );
        let stirling = call(
            "algebra.stirling_second",
            serde_json::json!({"n": 5, "k": 3}),
        )
        .unwrap();
        assert_eq!(integer_result(&stirling), BigInt::from(25));
        let bell = call("algebra.bell", serde_json::json!({"n": 5})).unwrap();
        assert_eq!(integer_result(&bell), BigInt::from(52));
        let multinomial = call(
            "algebra.multinomial",
            serde_json::json!({"counts": [2, 1, 1]}),
        )
        .unwrap();
        assert_eq!(integer_result(&multinomial), BigInt::from(12));
    }

    #[test]
    fn large_semiprime_uses_pollard_rho() {
        let n = BigInt::from(1_000_003u64) * BigInt::from(1_000_033u64);
        let outcome = call(
            "algebra.prime_factors",
            serde_json::json!({"n": n.to_string()}),
        )
        .unwrap();
        assert_eq!(
            factor_pairs(&outcome),
            vec![
                (BigInt::from(1_000_003u64), 1),
                (BigInt::from(1_000_033u64), 1)
            ]
        );
        let fields = record_result(&outcome);
        assert_eq!(
            fields.get("method"),
            Some(&Value::text("trial_division+pollard_rho_brent"))
        );
    }

    #[test]
    fn irrational_cubic_roots_are_approximate() {
        let outcome = call(
            "algebra.polynomial_roots",
            serde_json::json!({"coefficients": [-2, 0, 0, 1]}),
        )
        .unwrap();
        assert_eq!(outcome.exactness, Exactness::Approximate);
        assert!(!outcome.warnings.is_empty());
        let roots = array_result(&outcome);
        assert_eq!(roots.len(), 1);
        match &roots[0] {
            Value::Record(fields) => match fields.get("approximate") {
                Some(Value::Number(Number::Float64(value))) => {
                    assert!((value.get() - 1.259_921_049_894_873_2).abs() <= 1e-12);
                }
                other => panic!("expected a float64 approximation, got {other:?}"),
            },
            other => panic!("expected a root record, got {other:?}"),
        }
    }

    #[test]
    fn descriptor_contract_is_complete() {
        let module = module();
        assert_eq!(module.descriptor.id, "algebra");
        assert_eq!(module.descriptor.version, "1.0.0");
        assert_eq!(module.functions.len(), 28);
        let mut ids = BTreeSet::new();
        for function in &module.functions {
            let descriptor = function.descriptor();
            assert!(descriptor.id.starts_with("algebra."));
            assert_eq!(descriptor.module, "algebra");
            assert_eq!(descriptor.version, "1.0.0");
            assert!(
                descriptor
                    .method_ref
                    .starts_with("docs/methods/algebra.md#")
            );
            assert!(
                !descriptor.examples.is_empty(),
                "function {} has no examples",
                descriptor.id
            );
            assert!(ids.insert(descriptor.id.clone()), "duplicate id");
        }
    }

    #[test]
    fn every_example_executes_and_matches() {
        let module = module();
        let ctx = ExecContext::conservative();
        for function in &module.functions {
            for example in &function.descriptor().examples {
                let mut values = BTreeMap::new();
                for (name, raw) in &example.arguments {
                    let parameter = function
                        .descriptor()
                        .parameter(name)
                        .unwrap_or_else(|| panic!("example parameter {name} is not declared"));
                    parameter
                        .schema
                        .validate(raw, name, &ctx.limits)
                        .unwrap_or_else(|error| {
                            panic!(
                                "example {:?} of {} has an invalid {name}: {error}",
                                example.title,
                                function.descriptor().id
                            )
                        });
                    values.insert(name.clone(), raw.clone());
                }
                let result = function.invoke(&Args::new(values), &ctx);
                match &example.expected {
                    Some(bicmath_core::contract::ExampleExpectation::Value(expected)) => {
                        let outcome = result.unwrap_or_else(|error| {
                            panic!(
                                "example {:?} of {} failed: {error}",
                                example.title,
                                function.descriptor().id
                            )
                        });
                        assert_eq!(
                            &outcome.value,
                            expected,
                            "example {:?} of {} produced an unexpected value",
                            example.title,
                            function.descriptor().id
                        );
                    }
                    Some(bicmath_core::contract::ExampleExpectation::Error(code)) => {
                        let error = match result {
                            Err(error) => error,
                            Ok(outcome) => panic!(
                                "example {:?} of {} expected {code:?}, produced {:?}",
                                example.title,
                                function.descriptor().id,
                                outcome.value
                            ),
                        };
                        assert_eq!(error.code, *code);
                    }
                    Some(bicmath_core::contract::ExampleExpectation::Contains(text)) => {
                        let outcome = result.unwrap_or_else(|error| {
                            panic!(
                                "example {:?} of {} failed: {error}",
                                example.title,
                                function.descriptor().id
                            )
                        });
                        let rendered = serde_json::to_string(&outcome.value).unwrap_or_default();
                        assert!(
                            rendered.contains(text),
                            "example {:?} of {} does not contain {text:?}",
                            example.title,
                            function.descriptor().id
                        );
                    }
                    None => {}
                }
            }
        }
    }
}
