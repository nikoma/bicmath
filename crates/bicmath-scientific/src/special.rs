//! Special functions: the gamma family, the error functions, the Riemann zeta
//! function, and integer-order Bessel functions of the first kind.
//!
//! Every transcendental is routed through the wasm-safe wrappers in
//! [`crate::f64math`]. The implementations use only bounded series, continued
//! fractions, and asymptotic expansions, so no function here can iterate
//! without a fixed cap. All results are binary64 approximations and every
//! function requires [`bicmath_core::number::NumericMode::Scientific`].

use std::sync::Arc;

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, CostClass, Example, Function, FunctionDescriptor, Outcome, ParamDescriptor,
    SimpleFunction,
};
use bicmath_core::error::{EngineError, ErrorCode};

use crate::common::*;
use crate::f64math;

const PI: f64 = std::f64::consts::PI;
const LN_2_PI: f64 = 1.837_877_066_409_345_3;
const SQRT_2_PI: f64 = 2.506_628_274_631_000_2;
const MAX_SERIES_ITERATIONS: u64 = 100_000;
const SERIES_EPS: f64 = 1e-16;
const SERIES_FPMIN: f64 = f64::MIN_POSITIVE / SERIES_EPS;

fn non_convergence(what: &str) -> EngineError {
    EngineError::new(
        ErrorCode::NonConvergence,
        format!("{what} did not converge within {MAX_SERIES_ITERATIONS} iterations"),
    )
}

// ---------------------------------------------------------------------------
// Gamma, log-gamma, digamma, and the beta function
// ---------------------------------------------------------------------------

/// Lanczos coefficients for `g = 7`, `n = 9`.
const LANCZOS: [f64; 9] = [
    0.999_999_999_999_809_9,
    676.520_368_121_885_1,
    -1_259.139_216_722_402_8,
    771.323_428_777_653_1,
    -176.615_029_162_140_6,
    12.507_343_278_686_905,
    -0.138_571_095_265_720_12,
    9.984_369_578_019_572e-6,
    1.505_632_735_149_311_6e-7,
];

/// Lanczos approximation to `ln Gamma(x)`, valid for `x >= 0.5`.
fn lanczos_lgamma(x: f64) -> f64 {
    let z = x - 1.0;
    let mut series = LANCZOS[0];
    for (index, coefficient) in LANCZOS.iter().enumerate().skip(1) {
        series += coefficient / (z + index as f64);
    }
    let t = z + 7.5;
    0.5 * LN_2_PI + (z + 0.5) * f64math::ln(t) - t + f64math::ln(series)
}

/// Lanczos approximation to `Gamma(x)`, valid for `x >= 0.5`.
fn lanczos_gamma(x: f64) -> f64 {
    let z = x - 1.0;
    let mut series = LANCZOS[0];
    for (index, coefficient) in LANCZOS.iter().enumerate().skip(1) {
        series += coefficient / (z + index as f64);
    }
    let t = z + 7.5;
    SQRT_2_PI * f64math::pow(t, z + 0.5) * f64math::exp(-t) * series
}

/// `ln |Gamma(x)|` for every real `x` that is not a pole.
pub(crate) fn lgamma(x: f64) -> f64 {
    if x < 0.5 {
        // Reflection: |Gamma(x)| |Gamma(1 - x)| = pi / |sin(pi x)|.
        f64math::ln(PI / f64math::abs(f64math::sin(PI * x))) - lgamma(1.0 - x)
    } else {
        lanczos_lgamma(x)
    }
}

/// `Gamma(x)` for every real `x` that is not a pole.
pub(crate) fn gamma(x: f64) -> f64 {
    if x >= 0.5 {
        lanczos_gamma(x)
    } else {
        // Reflection: Gamma(x) Gamma(1 - x) = pi / sin(pi x).
        PI / (f64math::sin(PI * x) * lanczos_gamma(1.0 - x))
    }
}

/// Asymptotic coefficients of the digamma expansion:
/// `psi(x) ~ ln x - 1/(2x) - sum B(2n) / (2n x^(2n))`.
const DIGAMMA_COEFFICIENTS: [f64; 7] = [
    1.0 / 12.0,
    -1.0 / 120.0,
    1.0 / 252.0,
    -1.0 / 240.0,
    1.0 / 132.0,
    -691.0 / 32760.0,
    1.0 / 12.0,
];

/// Digamma function `psi(x) = d/dx ln Gamma(x)`.
///
/// The recurrence `psi(x + 1) = psi(x) + 1/x` raises the argument to at least
/// ten, where the Bernoulli asymptotic expansion is accurate to near machine
/// precision. Reflection covers `x < 0.5`.
pub(crate) fn digamma(x: f64) -> f64 {
    if x < 0.5 {
        return digamma(1.0 - x) - PI / f64math::tan(PI * x);
    }
    let mut z = x;
    let mut sum = 0.0;
    while z < 10.0 {
        sum -= 1.0 / z;
        z += 1.0;
    }
    let inverse = 1.0 / z;
    let inverse2 = inverse * inverse;
    let mut correction = 0.0;
    let mut power = inverse2;
    for coefficient in DIGAMMA_COEFFICIENTS {
        correction += coefficient * power;
        power *= inverse2;
    }
    sum + f64math::ln(z) - 0.5 * inverse - correction
}

/// `beta(a, b) = Gamma(a) Gamma(b) / Gamma(a + b)` through log-gamma.
pub(crate) fn beta(a: f64, b: f64) -> f64 {
    f64math::exp(lgamma(a) + lgamma(b) - lgamma(a + b))
}

// ---------------------------------------------------------------------------
// Error functions
// ---------------------------------------------------------------------------

/// Regularized lower incomplete gamma `P(a, x)`.
fn gamma_p(a: f64, x: f64) -> Result<f64, EngineError> {
    if a <= 0.0 || !a.is_finite() {
        return Err(EngineError::domain("incomplete gamma requires a > 0"));
    }
    if x <= 0.0 {
        return Ok(0.0);
    }
    if x < a + 1.0 {
        gamma_p_series(a, x)
    } else {
        Ok(1.0 - gamma_q_continued_fraction(a, x)?)
    }
}

/// Regularized upper incomplete gamma `Q(a, x)`.
fn gamma_q(a: f64, x: f64) -> Result<f64, EngineError> {
    if a <= 0.0 || !a.is_finite() {
        return Err(EngineError::domain("incomplete gamma requires a > 0"));
    }
    if x <= 0.0 {
        return Ok(1.0);
    }
    if x < a + 1.0 {
        Ok(1.0 - gamma_p_series(a, x)?)
    } else {
        gamma_q_continued_fraction(a, x)
    }
}

fn gamma_p_series(a: f64, x: f64) -> Result<f64, EngineError> {
    let mut ap = a;
    let mut sum = 1.0 / a;
    let mut term = sum;
    let mut iterations = 0u64;
    loop {
        iterations += 1;
        if iterations > MAX_SERIES_ITERATIONS {
            return Err(non_convergence("incomplete gamma series"));
        }
        ap += 1.0;
        term *= x / ap;
        sum += term;
        if f64math::abs(term) < f64math::abs(sum) * SERIES_EPS {
            break;
        }
    }
    let prefactor = f64math::exp(-x + a * f64math::ln(x) - lgamma(a));
    Ok(sum * prefactor)
}

fn gamma_q_continued_fraction(a: f64, x: f64) -> Result<f64, EngineError> {
    let mut b = x + 1.0 - a;
    let mut c = 1.0 / SERIES_FPMIN;
    let mut d = 1.0 / b;
    let mut h = d;
    let mut iterations = 0u64;
    loop {
        iterations += 1;
        if iterations > MAX_SERIES_ITERATIONS {
            return Err(non_convergence("incomplete gamma continued fraction"));
        }
        let index = iterations as f64;
        let an = -index * (index - a);
        b += 2.0;
        d = an * d + b;
        if f64math::abs(d) < SERIES_FPMIN {
            d = SERIES_FPMIN;
        }
        c = b + an / c;
        if f64math::abs(c) < SERIES_FPMIN {
            c = SERIES_FPMIN;
        }
        d = 1.0 / d;
        let delta = d * c;
        h *= delta;
        if f64math::abs(delta - 1.0) < SERIES_EPS {
            break;
        }
    }
    let prefactor = f64math::exp(-x + a * f64math::ln(x) - lgamma(a));
    Ok(prefactor * h)
}

/// Error function `erf(x) = P(1/2, x^2)`, odd by construction.
pub(crate) fn erf(x: f64) -> Result<f64, EngineError> {
    if x == 0.0 {
        return Ok(0.0);
    }
    let magnitude = gamma_p(0.5, x * x)?;
    Ok(if x > 0.0 { magnitude } else { -magnitude })
}

/// Complementary error function `erfc(x) = Q(1/2, x^2)` for `x >= 0`, so the
/// positive tail keeps full relative accuracy.
pub(crate) fn erfc(x: f64) -> Result<f64, EngineError> {
    if x >= 0.0 {
        gamma_q(0.5, x * x)
    } else {
        Ok(2.0 - gamma_q(0.5, x * x)?)
    }
}

// ---------------------------------------------------------------------------
// Riemann zeta function
// ---------------------------------------------------------------------------

/// `B(2k) / (2k)!` for `k = 1..=10`, the Euler-Maclaurin correction weights.
const ZETA_COEFFICIENTS: [f64; 10] = [
    0.083_333_333_333_333_33,
    -0.001_388_888_888_888_889,
    3.306_878_306_878_306_4e-5,
    -8.267_195_767_195_768e-7,
    2.087_675_698_786_81e-8,
    -5.284_190_138_687_493e-10,
    1.338_253_653_068_468e-11,
    -3.389_680_296_322_582_7e-13,
    8.586_062_056_277_845e-15,
    -2.174_868_698_558_062e-16,
];

/// Riemann zeta `zeta(s)` for real `s > 1` by Euler-Maclaurin summation.
///
/// The first nineteen terms are summed directly and the tail is replaced by
/// its Euler-Maclaurin expansion at `N = 20` with ten Bernoulli correction
/// terms, which keeps the absolute error near `1e-15` for `s >= 2`.
pub(crate) fn zeta(s: f64) -> f64 {
    const N: f64 = 20.0;
    let mut total = 0.0;
    let mut n = 1.0;
    while n < N {
        total += f64math::pow(n, -s);
        n += 1.0;
    }
    let n_power = f64math::pow(N, -s);
    total += 0.5 * n_power;
    total += f64math::pow(N, 1.0 - s) / (s - 1.0);
    let mut inverse_power = 1.0 / N;
    let inverse_n2 = 1.0 / (N * N);
    let mut rising = 1.0;
    for (index, coefficient) in ZETA_COEFFICIENTS.iter().enumerate() {
        let k = index as f64 + 1.0;
        if index == 0 {
            rising = s;
        } else {
            rising *= (s + (2.0 * k - 3.0)) * (s + (2.0 * k - 2.0));
        }
        total += coefficient * rising * n_power * inverse_power;
        inverse_power *= inverse_n2;
    }
    total
}

// ---------------------------------------------------------------------------
// Bessel functions of the first kind
// ---------------------------------------------------------------------------

/// Below this magnitude the power series is used; at and above it the
/// asymptotic expansion is used.
const BESSEL_SERIES_LIMIT: f64 = 12.0;
/// Number of terms kept in the asymptotic expansion.
const BESSEL_ASYMPTOTIC_TERMS: usize = 16;

fn bessel_series(x: f64, order: u32) -> f64 {
    let half = 0.5 * x;
    let y = half * half;
    let mut term = if order == 0 { 1.0 } else { half };
    let mut sum = term;
    let mut m = 1.0;
    while m <= 10_000.0 {
        let denominator = if order == 0 { m * m } else { m * (m + 1.0) };
        term = -term * y / denominator;
        sum += term;
        if term == 0.0 || f64math::abs(term) <= f64math::abs(sum) * 1e-17 {
            break;
        }
        m += 1.0;
    }
    sum
}

fn bessel_asymptotic(x: f64, order: u32) -> f64 {
    let z = f64math::abs(x);
    let mu = 4.0 * (order * order) as f64;
    let mut a = [0.0f64; BESSEL_ASYMPTOTIC_TERMS + 1];
    a[0] = 1.0;
    for m in 1..=BESSEL_ASYMPTOTIC_TERMS {
        let mf = m as f64;
        a[m] = a[m - 1] * (mu - (2.0 * mf - 1.0) * (2.0 * mf - 1.0)) / (8.0 * mf);
    }
    let omega = z - (0.5 * order as f64 + 0.25) * PI;
    let mut p = 0.0;
    let mut q = 0.0;
    let mut z2 = 1.0;
    for k in 0..=BESSEL_ASYMPTOTIC_TERMS / 2 {
        let sign = if k % 2 == 0 { 1.0 } else { -1.0 };
        p += sign * a[2 * k] / z2;
        if 2 * k < BESSEL_ASYMPTOTIC_TERMS {
            q += sign * a[2 * k + 1] / (z2 * z);
        }
        z2 *= z * z;
    }
    let value = f64math::sqrt(2.0 / (PI * z)) * (p * f64math::cos(omega) - q * f64math::sin(omega));
    if order == 1 && x < 0.0 { -value } else { value }
}

/// Bessel function of the first kind `J0(x)`.
pub(crate) fn bessel_j0(x: f64) -> f64 {
    if f64math::abs(x) < BESSEL_SERIES_LIMIT {
        bessel_series(x, 0)
    } else {
        bessel_asymptotic(x, 0)
    }
}

/// Bessel function of the first kind `J1(x)`.
pub(crate) fn bessel_j1(x: f64) -> f64 {
    if f64math::abs(x) < BESSEL_SERIES_LIMIT {
        bessel_series(x, 1)
    } else {
        bessel_asymptotic(x, 1)
    }
}

// ---------------------------------------------------------------------------
// Descriptors and invocations
// ---------------------------------------------------------------------------

fn value_example(title: &str, input: serde_json::Value, expected: &str) -> Example {
    Example::new(title, example_args(&[("x", input)])).with_value(parse_value(
        serde_json::json!({"kind": "float64", "value": expected}),
    ))
}

fn unary_descriptor(
    name: &str,
    title: &str,
    summary: &str,
    description: &str,
    examples: Vec<Example>,
) -> FunctionDescriptor {
    FunctionDescriptor::new(
        format!("scientific.{name}"),
        MODULE,
        VERSION,
        title,
        summary,
    )
    .with_description(description)
    .with_parameters(vec![ParamDescriptor::required(
        "x",
        "Real argument.",
        any_number_schema(),
    )])
    .with_output(float64_schema(), "Function value as float64.")
    .with_modes(scientific_modes())
    .with_cost(CostClass::Constant)
    .with_method_ref(format!("docs/methods/scientific.md#{name}"))
    .with_examples(examples)
}

fn pole_check(x: f64, name: &str) -> Result<(), EngineError> {
    if x <= 0.0 && x.fract() == 0.0 {
        return Err(EngineError::domain(format!(
            "{name} has poles at zero and the negative integers"
        ))
        .with_path("x".to_string()));
    }
    Ok(())
}

fn invoke_scalar(
    args: &Args,
    ctx: &ExecContext,
    id: &str,
    evaluate: impl FnOnce(f64) -> Result<f64, EngineError>,
) -> Result<Outcome, EngineError> {
    require_scientific(ctx, id)?;
    ctx.check()?;
    let x = real_argument(args, "x")?;
    Ok(Outcome::approximate(float_value(evaluate(x)?)?))
}

pub(crate) fn gamma_descriptor() -> FunctionDescriptor {
    unary_descriptor(
        "gamma",
        "Gamma function",
        "Gamma function of a real argument.",
        "Computes Gamma(x) with the Lanczos approximation (g = 7, n = 9) for x >= 0.5 and \
         the reflection formula Gamma(x) Gamma(1 - x) = pi / sin(pi x) below. Defined for \
         every real x except the poles at zero and the negative integers, which are rejected \
         with a domain violation. Relative accuracy is near 1e-15.",
        vec![
            value_example("gamma of five", serde_json::json!(5), "23.999999999999996"),
            Example::new("pole at zero", example_args(&[("x", serde_json::json!(0))]))
                .with_error(ErrorCode::DomainViolation),
        ],
    )
}

pub(crate) fn invoke_gamma(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    invoke_scalar(args, ctx, "scientific.gamma", |x| {
        pole_check(x, "gamma")?;
        Ok(gamma(x))
    })
}

pub(crate) fn lgamma_descriptor() -> FunctionDescriptor {
    unary_descriptor(
        "lgamma",
        "Log-gamma function",
        "Natural logarithm of the absolute value of the gamma function.",
        "Computes ln|Gamma(x)| with the same Lanczos approximation and reflection as \
         scientific.gamma. Defined for every real x except the poles at zero and the negative \
         integers, which are rejected with a domain violation. Relative accuracy is near 1e-15.",
        vec![
            value_example(
                "log-gamma of ten",
                serde_json::json!(10),
                "12.801827480081474",
            ),
            Example::new("pole at zero", example_args(&[("x", serde_json::json!(0))]))
                .with_error(ErrorCode::DomainViolation),
        ],
    )
}

pub(crate) fn invoke_lgamma(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    invoke_scalar(args, ctx, "scientific.lgamma", |x| {
        pole_check(x, "lgamma")?;
        Ok(lgamma(x))
    })
}

pub(crate) fn digamma_descriptor() -> FunctionDescriptor {
    unary_descriptor(
        "digamma",
        "Digamma function",
        "Logarithmic derivative of the gamma function.",
        "Computes psi(x) = d/dx ln Gamma(x) by raising the argument with the recurrence \
         psi(x + 1) = psi(x) + 1/x to at least ten and evaluating the Bernoulli asymptotic \
         expansion there, with reflection for x < 0.5. Defined for every real x except the \
         poles at zero and the negative integers, which are rejected with a domain violation.",
        vec![
            value_example(
                "digamma of one",
                serde_json::json!(1),
                "-0.5772156649015324",
            ),
            Example::new("pole at zero", example_args(&[("x", serde_json::json!(0))]))
                .with_error(ErrorCode::DomainViolation),
        ],
    )
}

pub(crate) fn invoke_digamma(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    invoke_scalar(args, ctx, "scientific.digamma", |x| {
        pole_check(x, "digamma")?;
        Ok(digamma(x))
    })
}

pub(crate) fn beta_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "scientific.beta",
        MODULE,
        VERSION,
        "Beta function",
        "Euler beta function of two positive real arguments.",
    )
    .with_description(
        "Computes beta(a, b) = Gamma(a) Gamma(b) / Gamma(a + b) as exp(lgamma(a) + \
         lgamma(b) - lgamma(a + b)). Both arguments must be strictly positive; other values \
         are rejected with a domain violation. Relative accuracy is near 1e-15.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("a", "First positive real argument.", any_number_schema()),
        ParamDescriptor::required("b", "Second positive real argument.", any_number_schema()),
    ])
    .with_output(float64_schema(), "Beta function value as float64.")
    .with_modes(scientific_modes())
    .with_cost(CostClass::Constant)
    .with_method_ref("docs/methods/scientific.md#beta")
    .with_examples(vec![
        Example::new(
            "beta of two and three",
            example_args(&[("a", serde_json::json!(2)), ("b", serde_json::json!(3))]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "float64", "value": "0.08333333333333355"}),
        )),
        Example::new(
            "non-positive argument",
            example_args(&[("a", serde_json::json!(0)), ("b", serde_json::json!(3))]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

pub(crate) fn invoke_beta(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_scientific(ctx, "scientific.beta")?;
    ctx.check()?;
    let a = real_argument(args, "a")?;
    let b = real_argument(args, "b")?;
    if a <= 0.0 {
        return Err(EngineError::domain("beta requires a > 0").with_path("a".to_string()));
    }
    if b <= 0.0 {
        return Err(EngineError::domain("beta requires b > 0").with_path("b".to_string()));
    }
    Ok(Outcome::approximate(float_value(beta(a, b))?))
}

pub(crate) fn erf_descriptor() -> FunctionDescriptor {
    unary_descriptor(
        "erf",
        "Error function",
        "Error function of a real argument.",
        "Computes erf(x) = 2/sqrt(pi) * integral of exp(-t^2) from 0 to x through the \
         regularized lower incomplete gamma function P(1/2, x^2), evaluated with a series \
         for x^2 < 3/2 and a continued fraction otherwise. The absolute error is near 1e-15.",
        vec![
            value_example("error function of zero", serde_json::json!(0), "0"),
            value_example(
                "error function of one",
                serde_json::json!(1),
                "0.8427007929497154",
            ),
        ],
    )
}

pub(crate) fn invoke_erf(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    invoke_scalar(args, ctx, "scientific.erf", erf)
}

pub(crate) fn erfc_descriptor() -> FunctionDescriptor {
    unary_descriptor(
        "erfc",
        "Complementary error function",
        "Complementary error function 1 - erf(x).",
        "Computes erfc(x) = 1 - erf(x) directly from the regularized upper incomplete gamma \
         function Q(1/2, x^2), so the positive tail keeps full relative accuracy instead of \
         cancelling. The absolute error is near 1e-15 and the relative error stays small far \
         into the tail.",
        vec![
            value_example(
                "complementary error function of zero",
                serde_json::json!(0),
                "1",
            ),
            value_example(
                "complementary error function of three",
                serde_json::json!(3),
                "2.2090496998585465e-5",
            ),
        ],
    )
}

pub(crate) fn invoke_erfc(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    invoke_scalar(args, ctx, "scientific.erfc", erfc)
}

pub(crate) fn zeta_descriptor() -> FunctionDescriptor {
    unary_descriptor(
        "zeta",
        "Riemann zeta function",
        "Riemann zeta function for real arguments greater than one.",
        "Computes zeta(s) for real s > 1 with Euler-Maclaurin summation: nineteen leading \
         terms are summed directly and the tail is replaced by its expansion at N = 20 with \
         ten Bernoulli correction terms. The domain is s > 1; s <= 1 is rejected with a \
         domain violation. Absolute error is near 1e-15 for s >= 2.",
        vec![
            value_example("zeta of two", serde_json::json!(2), "1.6449340668482266"),
            Example::new(
                "outside the domain",
                example_args(&[("x", serde_json::json!(1))]),
            )
            .with_error(ErrorCode::DomainViolation),
        ],
    )
}

pub(crate) fn invoke_zeta(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    invoke_scalar(args, ctx, "scientific.zeta", |x| {
        if x <= 1.0 {
            return Err(EngineError::domain(
                "zeta requires s > 1; the Euler-Maclaurin representation used here does not \
                 extend to the critical strip",
            )
            .with_path("x".to_string()));
        }
        Ok(zeta(x))
    })
}

pub(crate) fn bessel_j0_descriptor() -> FunctionDescriptor {
    unary_descriptor(
        "bessel_j0",
        "Bessel J0",
        "Bessel function of the first kind of order zero.",
        "Computes J0(x). The power series is used for |x| < 12 and the asymptotic expansion \
         for |x| >= 12. Relative error is a few ulps in the series region and at most about \
         1e-9 in the asymptotic region, degrading near the zeros of the function.",
        vec![
            value_example("Bessel J0 at zero", serde_json::json!(0), "1"),
            value_example(
                "Bessel J0 at one",
                serde_json::json!(1),
                "0.7651976865579666",
            ),
        ],
    )
}

pub(crate) fn invoke_bessel_j0(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    invoke_scalar(args, ctx, "scientific.bessel_j0", |x| Ok(bessel_j0(x)))
}

pub(crate) fn bessel_j1_descriptor() -> FunctionDescriptor {
    unary_descriptor(
        "bessel_j1",
        "Bessel J1",
        "Bessel function of the first kind of order one.",
        "Computes J1(x). The power series is used for |x| < 12 and the asymptotic expansion \
         for |x| >= 12; J1 is odd, so the sign is flipped for negative arguments. Relative \
         error is a few ulps in the series region and at most about 1e-9 in the asymptotic \
         region, degrading near the zeros of the function.",
        vec![
            value_example("Bessel J1 at zero", serde_json::json!(0), "0"),
            value_example(
                "Bessel J1 at one",
                serde_json::json!(1),
                "0.44005058574493355",
            ),
        ],
    )
}

pub(crate) fn invoke_bessel_j1(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    invoke_scalar(args, ctx, "scientific.bessel_j1", |x| Ok(bessel_j1(x)))
}

pub(crate) fn functions() -> Vec<Arc<dyn Function>> {
    vec![
        SimpleFunction::arc(gamma_descriptor(), invoke_gamma),
        SimpleFunction::arc(lgamma_descriptor(), invoke_lgamma),
        SimpleFunction::arc(digamma_descriptor(), invoke_digamma),
        SimpleFunction::arc(beta_descriptor(), invoke_beta),
        SimpleFunction::arc(erf_descriptor(), invoke_erf),
        SimpleFunction::arc(erfc_descriptor(), invoke_erfc),
        SimpleFunction::arc(zeta_descriptor(), invoke_zeta),
        SimpleFunction::arc(bessel_j0_descriptor(), invoke_bessel_j0),
        SimpleFunction::arc(bessel_j1_descriptor(), invoke_bessel_j1),
    ]
}
