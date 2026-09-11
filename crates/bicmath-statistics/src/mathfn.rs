//! Special functions and probability distributions implemented from scratch.
//!
//! The implementation follows the standard series and continued-fraction
//! methods for the regularized incomplete gamma and beta functions
//! (Numerical Recipes style, with explicit iteration bounds and a structured
//! `NonConvergence` error). No `statrs` or platform `erf` is used; on wasm32
//! every transcendental falls back to `libm`.

use bicmath_core::error::{EngineError, ErrorCode};

const EPS: f64 = 1e-16;
const FPMIN: f64 = f64::MIN_POSITIVE / EPS;
const MAX_ITER: u64 = 100_000;
const LN_2_PI: f64 = 1.837_877_066_409_345_3;
const SQRT_2PI: f64 = 2.506_628_274_631_000_7;
const SQRT_2: f64 = std::f64::consts::SQRT_2;

// ---------------------------------------------------------------------------
// libm fallbacks for wasm32
// ---------------------------------------------------------------------------

#[cfg(target_arch = "wasm32")]
pub fn exp(x: f64) -> f64 {
    libm::exp(x)
}
#[cfg(not(target_arch = "wasm32"))]
pub fn exp(x: f64) -> f64 {
    x.exp()
}

#[cfg(target_arch = "wasm32")]
pub fn ln(x: f64) -> f64 {
    libm::log(x)
}
#[cfg(not(target_arch = "wasm32"))]
pub fn ln(x: f64) -> f64 {
    x.ln()
}

#[cfg(target_arch = "wasm32")]
pub fn sqrt(x: f64) -> f64 {
    libm::sqrt(x)
}
#[cfg(not(target_arch = "wasm32"))]
pub fn sqrt(x: f64) -> f64 {
    x.sqrt()
}

#[cfg(target_arch = "wasm32")]
pub fn sin(x: f64) -> f64 {
    libm::sin(x)
}
#[cfg(not(target_arch = "wasm32"))]
pub fn sin(x: f64) -> f64 {
    x.sin()
}

#[cfg(target_arch = "wasm32")]
pub fn abs(x: f64) -> f64 {
    libm::fabs(x)
}
#[cfg(not(target_arch = "wasm32"))]
pub fn abs(x: f64) -> f64 {
    x.abs()
}

fn non_convergence(what: &str) -> EngineError {
    EngineError::new(
        ErrorCode::NonConvergence,
        format!("{what} did not converge within {MAX_ITER} iterations"),
    )
}

// ---------------------------------------------------------------------------
// Gamma and log-gamma
// ---------------------------------------------------------------------------

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

/// Lanczos approximation to ln|Gamma(x)|.
pub fn lgamma(x: f64) -> f64 {
    if x < 0.5 {
        // Reflection: Gamma(x) Gamma(1-x) = pi / sin(pi x).
        let pi = std::f64::consts::PI;
        return ln(pi / abs(sin(pi * x))) - lgamma(1.0 - x);
    }
    let z = x - 1.0;
    let mut series = LANCZOS[0];
    for (index, coefficient) in LANCZOS.iter().enumerate().skip(1) {
        series += coefficient / (z + index as f64);
    }
    let t = z + 7.5;
    0.5 * LN_2_PI + (z + 0.5) * ln(t) - t + ln(series)
}

// ---------------------------------------------------------------------------
// Regularized incomplete gamma
// ---------------------------------------------------------------------------

/// Regularized lower incomplete gamma `P(a, x)`.
pub fn gamma_p(a: f64, x: f64) -> Result<f64, EngineError> {
    if a <= 0.0 || !a.is_finite() {
        return Err(EngineError::domain("incomplete gamma requires a > 0"));
    }
    if x <= 0.0 {
        return Ok(0.0);
    }
    if x < a + 1.0 {
        gamma_p_series(a, x)
    } else {
        Ok(1.0 - gamma_q_cf(a, x)?)
    }
}

/// Regularized upper incomplete gamma `Q(a, x)`.
pub fn gamma_q(a: f64, x: f64) -> Result<f64, EngineError> {
    if a <= 0.0 || !a.is_finite() {
        return Err(EngineError::domain("incomplete gamma requires a > 0"));
    }
    if x <= 0.0 {
        return Ok(1.0);
    }
    if x < a + 1.0 {
        Ok(1.0 - gamma_p_series(a, x)?)
    } else {
        gamma_q_cf(a, x)
    }
}

fn gamma_p_series(a: f64, x: f64) -> Result<f64, EngineError> {
    let mut ap = a;
    let mut sum = 1.0 / a;
    let mut del = sum;
    let mut iterations = 0u64;
    loop {
        iterations += 1;
        if iterations > MAX_ITER {
            return Err(non_convergence("incomplete gamma series"));
        }
        ap += 1.0;
        del *= x / ap;
        sum += del;
        if abs(del) < abs(sum) * EPS {
            break;
        }
    }
    let prefactor = exp(-x + a * ln(x) - lgamma(a));
    Ok(sum * prefactor)
}

fn gamma_q_cf(a: f64, x: f64) -> Result<f64, EngineError> {
    let mut b = x + 1.0 - a;
    let mut c = 1.0 / FPMIN;
    let mut d = 1.0 / b;
    let mut h = d;
    let mut iterations = 0u64;
    loop {
        iterations += 1;
        if iterations > MAX_ITER {
            return Err(non_convergence("incomplete gamma continued fraction"));
        }
        let an = -(iterations as f64) * (iterations as f64 - a);
        b += 2.0;
        d = an * d + b;
        if abs(d) < FPMIN {
            d = FPMIN;
        }
        c = b + an / c;
        if abs(c) < FPMIN {
            c = FPMIN;
        }
        d = 1.0 / d;
        let delta = d * c;
        h *= delta;
        if abs(delta - 1.0) < EPS {
            break;
        }
    }
    let prefactor = exp(-x + a * ln(x) - lgamma(a));
    Ok(prefactor * h)
}

// ---------------------------------------------------------------------------
// Regularized incomplete beta
// ---------------------------------------------------------------------------

/// Regularized incomplete beta `I_x(a, b)`.
pub fn ibeta(a: f64, b: f64, x: f64) -> Result<f64, EngineError> {
    if a <= 0.0 || b <= 0.0 || !a.is_finite() || !b.is_finite() {
        return Err(EngineError::domain(
            "incomplete beta requires finite positive shape parameters",
        ));
    }
    if x <= 0.0 {
        return Ok(0.0);
    }
    if x >= 1.0 {
        return Ok(1.0);
    }
    let front = exp(lgamma(a + b) - lgamma(a) - lgamma(b) + a * ln(x) + b * ln(1.0 - x));
    if x < (a + 1.0) / (a + b + 2.0) {
        Ok(front * betacf(a, b, x)? / a)
    } else {
        Ok(1.0 - front * betacf(b, a, 1.0 - x)? / b)
    }
}

fn betacf(a: f64, b: f64, x: f64) -> Result<f64, EngineError> {
    let qab = a + b;
    let qap = a + 1.0;
    let qam = a - 1.0;
    let mut c = 1.0;
    let mut d = 1.0 - qab * x / qap;
    if abs(d) < FPMIN {
        d = FPMIN;
    }
    d = 1.0 / d;
    let mut h = d;
    let mut m = 0u64;
    loop {
        m += 1;
        if m > MAX_ITER {
            return Err(non_convergence("incomplete beta continued fraction"));
        }
        let mf = m as f64;
        let m2 = 2.0 * mf;
        let aa = mf * (b - mf) * x / ((qam + m2) * (a + m2));
        d = 1.0 + aa * d;
        if abs(d) < FPMIN {
            d = FPMIN;
        }
        c = 1.0 + aa / c;
        if abs(c) < FPMIN {
            c = FPMIN;
        }
        d = 1.0 / d;
        h *= d * c;
        let aa = -(a + mf) * (qab + mf) * x / ((a + m2) * (qap + m2));
        d = 1.0 + aa * d;
        if abs(d) < FPMIN {
            d = FPMIN;
        }
        c = 1.0 + aa / c;
        if abs(c) < FPMIN {
            c = FPMIN;
        }
        d = 1.0 / d;
        let delta = d * c;
        h *= delta;
        if abs(delta - 1.0) < EPS {
            return Ok(h);
        }
    }
}

// ---------------------------------------------------------------------------
// Error function
// ---------------------------------------------------------------------------

/// Error function, computed from the regularized incomplete gamma:
/// `erf(x) = P(1/2, x^2)` for `x >= 0`.
pub fn erf(x: f64) -> Result<f64, EngineError> {
    if x == 0.0 {
        return Ok(0.0);
    }
    let magnitude = gamma_p(0.5, x * x)?;
    Ok(if x > 0.0 { magnitude } else { -magnitude })
}

/// Complementary error function, computed from the regularized upper
/// incomplete gamma so that the positive tail keeps full relative accuracy.
pub fn erfc(x: f64) -> Result<f64, EngineError> {
    if x >= 0.0 {
        gamma_q(0.5, x * x)
    } else {
        Ok(2.0 - gamma_q(0.5, x * x)?)
    }
}

// ---------------------------------------------------------------------------
// Normal distribution
// ---------------------------------------------------------------------------

fn normal_pdf_std(x: f64) -> f64 {
    exp(-0.5 * x * x) / SQRT_2PI
}

pub fn normal_pdf(x: f64, mean: f64, sd: f64) -> Result<f64, EngineError> {
    if !x.is_finite() || !mean.is_finite() {
        return Err(EngineError::domain("normal_pdf requires finite inputs"));
    }
    if sd <= 0.0 || !sd.is_finite() {
        return Err(EngineError::domain("normal_pdf requires sd > 0"));
    }
    let z = (x - mean) / sd;
    Ok(normal_pdf_std(z) / sd)
}

pub fn normal_cdf(x: f64, mean: f64, sd: f64) -> Result<f64, EngineError> {
    if !x.is_finite() || !mean.is_finite() {
        return Err(EngineError::domain("normal_cdf requires finite inputs"));
    }
    if sd <= 0.0 || !sd.is_finite() {
        return Err(EngineError::domain("normal_cdf requires sd > 0"));
    }
    Ok(0.5 * erfc(-(x - mean) / (sd * SQRT_2))?)
}

/// Upper tail `P(X > x)`, computed with `erfc` so large positive `x` keeps
/// relative accuracy (never `1 - cdf`).
pub fn normal_sf(x: f64, mean: f64, sd: f64) -> Result<f64, EngineError> {
    if !x.is_finite() || !mean.is_finite() {
        return Err(EngineError::domain("normal_sf requires finite inputs"));
    }
    if sd <= 0.0 || !sd.is_finite() {
        return Err(EngineError::domain("normal_sf requires sd > 0"));
    }
    Ok(0.5 * erfc((x - mean) / (sd * SQRT_2))?)
}

pub fn normal_quantile(p: f64, mean: f64, sd: f64) -> Result<f64, EngineError> {
    if !(0.0..1.0).contains(&p) {
        return Err(EngineError::domain(
            "normal_quantile requires p strictly between 0 and 1",
        ));
    }
    if !mean.is_finite() || sd <= 0.0 || !sd.is_finite() {
        return Err(EngineError::domain(
            "normal_quantile requires finite mean and sd > 0",
        ));
    }
    if p == 0.5 {
        return Ok(mean);
    }
    let z = invert_monotone(
        p,
        -1.0,
        1.0,
        &mut |x| normal_cdf(x, 0.0, 1.0),
        &mut normal_pdf_std,
    )?;
    Ok(mean + sd * z)
}

// ---------------------------------------------------------------------------
// Student's t distribution
// ---------------------------------------------------------------------------

fn student_t_pdf_raw(x: f64, df: f64) -> f64 {
    let half = 0.5 * df;
    let ln_pdf = lgamma(half + 0.5)
        - lgamma(half)
        - 0.5 * ln(df * std::f64::consts::PI)
        - (half + 0.5) * ln(1.0 + x * x / df);
    exp(ln_pdf)
}

pub fn student_t_pdf(x: f64, df: f64) -> Result<f64, EngineError> {
    if !x.is_finite() {
        return Err(EngineError::domain("student_t_pdf requires a finite x"));
    }
    if df <= 0.0 || !df.is_finite() {
        return Err(EngineError::domain("student_t_pdf requires df > 0"));
    }
    Ok(student_t_pdf_raw(x, df))
}

pub fn student_t_cdf(x: f64, df: f64) -> Result<f64, EngineError> {
    if !x.is_finite() {
        return Err(EngineError::domain("student_t_cdf requires a finite x"));
    }
    if df <= 0.0 || !df.is_finite() {
        return Err(EngineError::domain("student_t_cdf requires df > 0"));
    }
    if x == 0.0 {
        return Ok(0.5);
    }
    let y = df / (df + x * x);
    let tail = ibeta(0.5 * df, 0.5, y)?;
    if x > 0.0 {
        Ok(1.0 - 0.5 * tail)
    } else {
        Ok(0.5 * tail)
    }
}

pub fn student_t_sf(x: f64, df: f64) -> Result<f64, EngineError> {
    if !x.is_finite() {
        return Err(EngineError::domain("student_t_sf requires a finite x"));
    }
    if df <= 0.0 || !df.is_finite() {
        return Err(EngineError::domain("student_t_sf requires df > 0"));
    }
    if x == 0.0 {
        return Ok(0.5);
    }
    let y = df / (df + x * x);
    let tail = ibeta(0.5 * df, 0.5, y)?;
    if x > 0.0 {
        Ok(0.5 * tail)
    } else {
        Ok(1.0 - 0.5 * tail)
    }
}

pub fn student_t_quantile(p: f64, df: f64) -> Result<f64, EngineError> {
    if !(0.0..1.0).contains(&p) {
        return Err(EngineError::domain(
            "student_t_quantile requires p strictly between 0 and 1",
        ));
    }
    if df <= 0.0 || !df.is_finite() {
        return Err(EngineError::domain("student_t_quantile requires df > 0"));
    }
    if p == 0.5 {
        return Ok(0.0);
    }
    invert_monotone(p, -1.0, 1.0, &mut |x| student_t_cdf(x, df), &mut |x| {
        student_t_pdf_raw(x, df)
    })
}

// ---------------------------------------------------------------------------
// Binomial distribution
// ---------------------------------------------------------------------------

pub fn binomial_pmf(k: f64, n: f64, p: f64) -> Result<f64, EngineError> {
    if !k.is_finite() || !n.is_finite() || !p.is_finite() {
        return Err(EngineError::domain("binomial_pmf requires finite inputs"));
    }
    if !(0.0..=1.0).contains(&p) {
        return Err(EngineError::domain("binomial_pmf requires p in [0, 1]"));
    }
    if k < 0.0 || k > n {
        return Ok(0.0);
    }
    if p == 0.0 {
        return Ok(if k == 0.0 { 1.0 } else { 0.0 });
    }
    if p == 1.0 {
        return Ok(if k == n { 1.0 } else { 0.0 });
    }
    if k == 0.0 {
        return Ok(exp(n * ln(1.0 - p)));
    }
    if k == n {
        return Ok(exp(n * ln(p)));
    }
    let ln_pmf =
        lgamma(n + 1.0) - lgamma(k + 1.0) - lgamma(n - k + 1.0) + k * ln(p) + (n - k) * ln(1.0 - p);
    Ok(exp(ln_pmf))
}

pub fn binomial_cdf(k: f64, n: f64, p: f64) -> Result<f64, EngineError> {
    if !k.is_finite() || !n.is_finite() || !p.is_finite() {
        return Err(EngineError::domain("binomial_cdf requires finite inputs"));
    }
    if !(0.0..=1.0).contains(&p) {
        return Err(EngineError::domain("binomial_cdf requires p in [0, 1]"));
    }
    if k < 0.0 {
        return Ok(0.0);
    }
    if k >= n {
        return Ok(1.0);
    }
    ibeta(n - k, k + 1.0, 1.0 - p)
}

// ---------------------------------------------------------------------------
// Chi-square distribution
// ---------------------------------------------------------------------------

pub fn chi_square_pdf(x: f64, df: f64) -> Result<f64, EngineError> {
    if !x.is_finite() {
        return Err(EngineError::domain("chi_square_pdf requires a finite x"));
    }
    if df <= 0.0 || !df.is_finite() {
        return Err(EngineError::domain("chi_square_pdf requires df > 0"));
    }
    if x < 0.0 {
        return Ok(0.0);
    }
    if x == 0.0 {
        return match df.partial_cmp(&2.0) {
            Some(std::cmp::Ordering::Less) => Err(EngineError::domain(
                "chi_square_pdf is unbounded at x = 0 for df < 2",
            )),
            Some(std::cmp::Ordering::Equal) => Ok(0.5),
            _ => Ok(0.0),
        };
    }
    let half = 0.5 * df;
    let ln_pdf = (half - 1.0) * ln(x) - 0.5 * x - half * ln(2.0) - lgamma(half);
    Ok(exp(ln_pdf))
}

pub fn chi_square_cdf(x: f64, df: f64) -> Result<f64, EngineError> {
    if !x.is_finite() {
        return Err(EngineError::domain("chi_square_cdf requires a finite x"));
    }
    if df <= 0.0 || !df.is_finite() {
        return Err(EngineError::domain("chi_square_cdf requires df > 0"));
    }
    if x <= 0.0 {
        return Ok(0.0);
    }
    gamma_p(0.5 * df, 0.5 * x)
}

pub fn chi_square_sf(x: f64, df: f64) -> Result<f64, EngineError> {
    if !x.is_finite() {
        return Err(EngineError::domain("chi_square_sf requires a finite x"));
    }
    if df <= 0.0 || !df.is_finite() {
        return Err(EngineError::domain("chi_square_sf requires df > 0"));
    }
    if x <= 0.0 {
        return Ok(1.0);
    }
    gamma_q(0.5 * df, 0.5 * x)
}

pub fn chi_square_quantile(p: f64, df: f64) -> Result<f64, EngineError> {
    if !(0.0..1.0).contains(&p) {
        return Err(EngineError::domain(
            "chi_square_quantile requires p in [0, 1)",
        ));
    }
    if df <= 0.0 || !df.is_finite() {
        return Err(EngineError::domain("chi_square_quantile requires df > 0"));
    }
    if p == 0.0 {
        return Ok(0.0);
    }
    invert_monotone(p, 0.0, 1.0, &mut |x| chi_square_cdf(x, df), &mut |x| {
        chi_square_pdf_raw(x, df)
    })
}

fn chi_square_pdf_raw(x: f64, df: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    let half = 0.5 * df;
    exp((half - 1.0) * ln(x) - 0.5 * x - half * ln(2.0) - lgamma(half))
}

// ---------------------------------------------------------------------------
// Poisson distribution
// ---------------------------------------------------------------------------

pub fn poisson_pmf(k: f64, lambda: f64) -> Result<f64, EngineError> {
    if !k.is_finite() || !lambda.is_finite() {
        return Err(EngineError::domain("poisson_pmf requires finite inputs"));
    }
    if lambda <= 0.0 {
        return Err(EngineError::domain("poisson_pmf requires lambda > 0"));
    }
    if k < 0.0 {
        return Ok(0.0);
    }
    if k == 0.0 {
        return Ok(exp(-lambda));
    }
    let ln_pmf = -lambda + k * ln(lambda) - lgamma(k + 1.0);
    Ok(exp(ln_pmf))
}

pub fn poisson_cdf(k: f64, lambda: f64) -> Result<f64, EngineError> {
    if !k.is_finite() || !lambda.is_finite() {
        return Err(EngineError::domain("poisson_cdf requires finite inputs"));
    }
    if lambda <= 0.0 {
        return Err(EngineError::domain("poisson_cdf requires lambda > 0"));
    }
    if k < 0.0 {
        return Ok(0.0);
    }
    gamma_q(k + 1.0, lambda)
}

// ---------------------------------------------------------------------------
// Exponential distribution
// ---------------------------------------------------------------------------

pub fn exponential_pdf(x: f64, rate: f64) -> Result<f64, EngineError> {
    if !x.is_finite() || !rate.is_finite() {
        return Err(EngineError::domain(
            "exponential_pdf requires finite inputs",
        ));
    }
    if rate <= 0.0 {
        return Err(EngineError::domain("exponential_pdf requires rate > 0"));
    }
    if x < 0.0 {
        return Ok(0.0);
    }
    Ok(rate * exp(-rate * x))
}

pub fn exponential_cdf(x: f64, rate: f64) -> Result<f64, EngineError> {
    if !x.is_finite() || !rate.is_finite() {
        return Err(EngineError::domain(
            "exponential_cdf requires finite inputs",
        ));
    }
    if rate <= 0.0 {
        return Err(EngineError::domain("exponential_cdf requires rate > 0"));
    }
    if x <= 0.0 {
        return Ok(0.0);
    }
    Ok(1.0 - exp(-rate * x))
}

/// Upper tail `P(X > x)` computed from `exp` directly so large `x` keeps
/// relative accuracy (never `1 - cdf`).
pub fn exponential_sf(x: f64, rate: f64) -> Result<f64, EngineError> {
    if !x.is_finite() || !rate.is_finite() {
        return Err(EngineError::domain("exponential_sf requires finite inputs"));
    }
    if rate <= 0.0 {
        return Err(EngineError::domain("exponential_sf requires rate > 0"));
    }
    if x <= 0.0 {
        return Ok(1.0);
    }
    Ok(exp(-rate * x))
}

pub fn exponential_quantile(p: f64, rate: f64) -> Result<f64, EngineError> {
    if !(0.0..1.0).contains(&p) {
        return Err(EngineError::domain(
            "exponential_quantile requires p in [0, 1)",
        ));
    }
    if rate <= 0.0 || !rate.is_finite() {
        return Err(EngineError::domain(
            "exponential_quantile requires rate > 0",
        ));
    }
    if p == 0.0 {
        return Ok(0.0);
    }
    Ok(-ln(1.0 - p) / rate)
}

// ---------------------------------------------------------------------------
// Uniform distribution
// ---------------------------------------------------------------------------

fn uniform_bounds(lower: f64, upper: f64) -> Result<(), EngineError> {
    if !lower.is_finite() || !upper.is_finite() {
        return Err(EngineError::domain("uniform bounds must be finite"));
    }
    if upper <= lower {
        return Err(EngineError::domain("uniform requires upper > lower"));
    }
    Ok(())
}

pub fn uniform_pdf(x: f64, lower: f64, upper: f64) -> Result<f64, EngineError> {
    if !x.is_finite() {
        return Err(EngineError::domain("uniform_pdf requires a finite x"));
    }
    uniform_bounds(lower, upper)?;
    if x < lower || x > upper {
        return Ok(0.0);
    }
    Ok(1.0 / (upper - lower))
}

pub fn uniform_cdf(x: f64, lower: f64, upper: f64) -> Result<f64, EngineError> {
    if !x.is_finite() {
        return Err(EngineError::domain("uniform_cdf requires a finite x"));
    }
    uniform_bounds(lower, upper)?;
    Ok(((x - lower) / (upper - lower)).clamp(0.0, 1.0))
}

pub fn uniform_quantile(p: f64, lower: f64, upper: f64) -> Result<f64, EngineError> {
    if !(0.0..=1.0).contains(&p) {
        return Err(EngineError::domain("uniform_quantile requires p in [0, 1]"));
    }
    uniform_bounds(lower, upper)?;
    Ok(lower + p * (upper - lower))
}

// ---------------------------------------------------------------------------
// Log-normal distribution
// ---------------------------------------------------------------------------

pub fn lognormal_pdf(x: f64, mu: f64, sigma: f64) -> Result<f64, EngineError> {
    if !x.is_finite() || !mu.is_finite() || !sigma.is_finite() {
        return Err(EngineError::domain("lognormal_pdf requires finite inputs"));
    }
    if sigma <= 0.0 {
        return Err(EngineError::domain("lognormal_pdf requires sigma > 0"));
    }
    if x <= 0.0 {
        return Ok(0.0);
    }
    let z = (ln(x) - mu) / sigma;
    Ok(exp(-0.5 * z * z) / (x * sigma * SQRT_2PI))
}

pub fn lognormal_cdf(x: f64, mu: f64, sigma: f64) -> Result<f64, EngineError> {
    if !x.is_finite() || !mu.is_finite() || !sigma.is_finite() {
        return Err(EngineError::domain("lognormal_cdf requires finite inputs"));
    }
    if sigma <= 0.0 {
        return Err(EngineError::domain("lognormal_cdf requires sigma > 0"));
    }
    if x <= 0.0 {
        return Ok(0.0);
    }
    normal_cdf((ln(x) - mu) / sigma, 0.0, 1.0)
}

pub fn lognormal_quantile(p: f64, mu: f64, sigma: f64) -> Result<f64, EngineError> {
    if !(0.0..1.0).contains(&p) {
        return Err(EngineError::domain(
            "lognormal_quantile requires p strictly between 0 and 1",
        ));
    }
    if !mu.is_finite() || sigma <= 0.0 || !sigma.is_finite() {
        return Err(EngineError::domain(
            "lognormal_quantile requires finite mu and sigma > 0",
        ));
    }
    let z = normal_quantile(p, 0.0, 1.0)?;
    Ok(exp(mu + sigma * z))
}

// ---------------------------------------------------------------------------
// Gamma distribution
// ---------------------------------------------------------------------------

fn gamma_pdf_raw(x: f64, shape: f64, rate: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    exp(shape * ln(rate) + (shape - 1.0) * ln(x) - rate * x - lgamma(shape))
}

pub fn gamma_pdf(x: f64, shape: f64, rate: f64) -> Result<f64, EngineError> {
    if !x.is_finite() || !shape.is_finite() || !rate.is_finite() {
        return Err(EngineError::domain("gamma_pdf requires finite inputs"));
    }
    if shape <= 0.0 {
        return Err(EngineError::domain("gamma_pdf requires shape > 0"));
    }
    if rate <= 0.0 {
        return Err(EngineError::domain("gamma_pdf requires rate > 0"));
    }
    if x < 0.0 {
        return Ok(0.0);
    }
    if x == 0.0 {
        return match shape.partial_cmp(&1.0) {
            Some(std::cmp::Ordering::Less) => Err(EngineError::domain(
                "gamma_pdf is unbounded at x = 0 for shape < 1",
            )),
            Some(std::cmp::Ordering::Equal) => Ok(rate),
            _ => Ok(0.0),
        };
    }
    Ok(gamma_pdf_raw(x, shape, rate))
}

pub fn gamma_cdf(x: f64, shape: f64, rate: f64) -> Result<f64, EngineError> {
    if !x.is_finite() || !shape.is_finite() || !rate.is_finite() {
        return Err(EngineError::domain("gamma_cdf requires finite inputs"));
    }
    if shape <= 0.0 {
        return Err(EngineError::domain("gamma_cdf requires shape > 0"));
    }
    if rate <= 0.0 {
        return Err(EngineError::domain("gamma_cdf requires rate > 0"));
    }
    if x <= 0.0 {
        return Ok(0.0);
    }
    gamma_p(shape, rate * x)
}

pub fn gamma_quantile(p: f64, shape: f64, rate: f64) -> Result<f64, EngineError> {
    if !(0.0..1.0).contains(&p) {
        return Err(EngineError::domain("gamma_quantile requires p in [0, 1)"));
    }
    if shape <= 0.0 || !shape.is_finite() {
        return Err(EngineError::domain("gamma_quantile requires shape > 0"));
    }
    if rate <= 0.0 || !rate.is_finite() {
        return Err(EngineError::domain("gamma_quantile requires rate > 0"));
    }
    if p == 0.0 {
        return Ok(0.0);
    }
    invert_monotone(p, 0.0, 1.0, &mut |x| gamma_cdf(x, shape, rate), &mut |x| {
        gamma_pdf_raw(x, shape, rate)
    })
}

// ---------------------------------------------------------------------------
// Beta distribution
// ---------------------------------------------------------------------------

fn beta_pdf_raw(x: f64, alpha: f64, beta: f64) -> f64 {
    exp((alpha - 1.0) * ln(x) + (beta - 1.0) * ln(1.0 - x)
        - (lgamma(alpha) + lgamma(beta) - lgamma(alpha + beta)))
}

fn beta_shapes(alpha: f64, beta: f64) -> Result<(), EngineError> {
    if alpha <= 0.0 || beta <= 0.0 || !alpha.is_finite() || !beta.is_finite() {
        return Err(EngineError::domain(
            "beta distribution requires finite positive shape parameters",
        ));
    }
    Ok(())
}

pub fn beta_pdf(x: f64, alpha: f64, beta: f64) -> Result<f64, EngineError> {
    if !x.is_finite() {
        return Err(EngineError::domain("beta_pdf requires a finite x"));
    }
    beta_shapes(alpha, beta)?;
    if !(0.0..=1.0).contains(&x) {
        return Ok(0.0);
    }
    if x == 0.0 {
        return match alpha.partial_cmp(&1.0) {
            Some(std::cmp::Ordering::Less) => Err(EngineError::domain(
                "beta_pdf is unbounded at x = 0 for alpha < 1",
            )),
            Some(std::cmp::Ordering::Equal) => Ok(beta),
            _ => Ok(0.0),
        };
    }
    if x == 1.0 {
        return match beta.partial_cmp(&1.0) {
            Some(std::cmp::Ordering::Less) => Err(EngineError::domain(
                "beta_pdf is unbounded at x = 1 for beta < 1",
            )),
            Some(std::cmp::Ordering::Equal) => Ok(alpha),
            _ => Ok(0.0),
        };
    }
    Ok(beta_pdf_raw(x, alpha, beta))
}

pub fn beta_cdf(x: f64, alpha: f64, beta: f64) -> Result<f64, EngineError> {
    if !x.is_finite() {
        return Err(EngineError::domain("beta_cdf requires a finite x"));
    }
    beta_shapes(alpha, beta)?;
    if x <= 0.0 {
        return Ok(0.0);
    }
    if x >= 1.0 {
        return Ok(1.0);
    }
    ibeta(alpha, beta, x)
}

/// Upper tail `P(X > x)`, computed from the mirrored regularized incomplete
/// beta function rather than `1 - cdf`.
pub fn beta_sf(x: f64, alpha: f64, beta: f64) -> Result<f64, EngineError> {
    if !x.is_finite() {
        return Err(EngineError::domain("beta_sf requires a finite x"));
    }
    beta_shapes(alpha, beta)?;
    if x <= 0.0 {
        return Ok(1.0);
    }
    if x >= 1.0 {
        return Ok(0.0);
    }
    ibeta(beta, alpha, 1.0 - x)
}

pub fn beta_quantile(p: f64, alpha: f64, beta: f64) -> Result<f64, EngineError> {
    if !(0.0..=1.0).contains(&p) {
        return Err(EngineError::domain("beta_quantile requires p in [0, 1]"));
    }
    beta_shapes(alpha, beta)?;
    if p == 0.0 {
        return Ok(0.0);
    }
    if p == 1.0 {
        return Ok(1.0);
    }
    invert_monotone(p, 0.0, 1.0, &mut |x| beta_cdf(x, alpha, beta), &mut |x| {
        beta_pdf_raw(x, alpha, beta)
    })
}

// ---------------------------------------------------------------------------
// F distribution
// ---------------------------------------------------------------------------

fn f_pdf_raw(x: f64, df1: f64, df2: f64) -> f64 {
    let half1 = 0.5 * df1;
    let half2 = 0.5 * df2;
    exp(half1 * ln(df1) + half2 * ln(df2) + (half1 - 1.0) * ln(x)
        - (half1 + half2) * ln(df2 + df1 * x)
        - (lgamma(half1) + lgamma(half2) - lgamma(half1 + half2)))
}

fn f_degrees(df1: f64, df2: f64) -> Result<(), EngineError> {
    if df1 <= 0.0 || df2 <= 0.0 || !df1.is_finite() || !df2.is_finite() {
        return Err(EngineError::domain(
            "F distribution requires finite positive degrees of freedom",
        ));
    }
    Ok(())
}

pub fn f_pdf(x: f64, df1: f64, df2: f64) -> Result<f64, EngineError> {
    if !x.is_finite() {
        return Err(EngineError::domain("f_pdf requires a finite x"));
    }
    f_degrees(df1, df2)?;
    if x < 0.0 {
        return Ok(0.0);
    }
    if x == 0.0 {
        return match df1.partial_cmp(&2.0) {
            Some(std::cmp::Ordering::Less) => Err(EngineError::domain(
                "f_pdf is unbounded at x = 0 for df1 < 2",
            )),
            Some(std::cmp::Ordering::Equal) => Ok(1.0),
            _ => Ok(0.0),
        };
    }
    Ok(f_pdf_raw(x, df1, df2))
}

pub fn f_cdf(x: f64, df1: f64, df2: f64) -> Result<f64, EngineError> {
    if !x.is_finite() {
        return Err(EngineError::domain("f_cdf requires a finite x"));
    }
    f_degrees(df1, df2)?;
    if x <= 0.0 {
        return Ok(0.0);
    }
    let y = df1 * x / (df1 * x + df2);
    ibeta(0.5 * df1, 0.5 * df2, y)
}

/// Upper tail `P(X > x)`, computed directly from the regularized incomplete
/// beta function so large `x` keeps relative accuracy.
pub fn f_sf(x: f64, df1: f64, df2: f64) -> Result<f64, EngineError> {
    if !x.is_finite() {
        return Err(EngineError::domain("f_sf requires a finite x"));
    }
    f_degrees(df1, df2)?;
    if x <= 0.0 {
        return Ok(1.0);
    }
    let y = df2 / (df2 + df1 * x);
    ibeta(0.5 * df2, 0.5 * df1, y)
}

pub fn f_quantile(p: f64, df1: f64, df2: f64) -> Result<f64, EngineError> {
    if !(0.0..1.0).contains(&p) {
        return Err(EngineError::domain("f_quantile requires p in [0, 1)"));
    }
    f_degrees(df1, df2)?;
    if p == 0.0 {
        return Ok(0.0);
    }
    let q = beta_quantile(p, 0.5 * df1, 0.5 * df2)?;
    if q >= 1.0 {
        return Err(EngineError::domain(
            "f_quantile could not resolve the incomplete beta quantile",
        ));
    }
    Ok(df2 * q / (df1 * (1.0 - q)))
}

// ---------------------------------------------------------------------------
// Negative binomial distribution
// ---------------------------------------------------------------------------

fn negative_binomial_parameters(r: f64, p: f64) -> Result<(), EngineError> {
    if !r.is_finite() || !p.is_finite() {
        return Err(EngineError::domain(
            "negative binomial requires finite inputs",
        ));
    }
    if r <= 0.0 {
        return Err(EngineError::domain("negative binomial requires r > 0"));
    }
    if !(0.0..=1.0).contains(&p) {
        return Err(EngineError::domain(
            "negative binomial requires p in [0, 1]",
        ));
    }
    Ok(())
}

pub fn negative_binomial_pmf(k: f64, r: f64, p: f64) -> Result<f64, EngineError> {
    if !k.is_finite() {
        return Err(EngineError::domain(
            "negative_binomial_pmf requires a finite k",
        ));
    }
    negative_binomial_parameters(r, p)?;
    if k < 0.0 {
        return Ok(0.0);
    }
    if p == 0.0 {
        return Ok(0.0);
    }
    if p == 1.0 {
        return Ok(if k == 0.0 { 1.0 } else { 0.0 });
    }
    if k == 0.0 {
        return Ok(exp(r * ln(p)));
    }
    let ln_pmf = lgamma(k + r) - lgamma(k + 1.0) - lgamma(r) + r * ln(p) + k * ln(1.0 - p);
    Ok(exp(ln_pmf))
}

pub fn negative_binomial_cdf(k: f64, r: f64, p: f64) -> Result<f64, EngineError> {
    if !k.is_finite() {
        return Err(EngineError::domain(
            "negative_binomial_cdf requires a finite k",
        ));
    }
    negative_binomial_parameters(r, p)?;
    if k < 0.0 {
        return Ok(0.0);
    }
    if p == 0.0 {
        return Ok(0.0);
    }
    if p == 1.0 {
        return Ok(1.0);
    }
    ibeta(r, k + 1.0, p)
}

// ---------------------------------------------------------------------------
// Generic monotone CDF inversion
// ---------------------------------------------------------------------------

/// Invert a strictly increasing CDF by bracket expansion, bisection, and
/// Newton polishing. The tolerance is relative (`1e-15`), comfortably inside
/// the documented `1e-12` requirement.
pub fn invert_monotone(
    p: f64,
    mut lo: f64,
    mut hi: f64,
    cdf: &mut dyn FnMut(f64) -> Result<f64, EngineError>,
    pdf: &mut dyn FnMut(f64) -> f64,
) -> Result<f64, EngineError> {
    let mut expansions = 0u32;
    while cdf(lo)? > p {
        hi = lo;
        lo = if lo < 0.0 { lo * 2.0 } else { -1.0 };
        expansions += 1;
        if expansions > 4_000 || !lo.is_finite() {
            return Err(non_convergence("quantile bracket expansion"));
        }
    }
    while cdf(hi)? < p {
        lo = hi;
        hi = if hi > 0.0 { hi * 2.0 } else { 1.0 };
        expansions += 1;
        if expansions > 4_000 || !hi.is_finite() {
            return Err(non_convergence("quantile bracket expansion"));
        }
    }
    let mut x = 0.5 * (lo + hi);
    for _ in 0..400 {
        let mid = 0.5 * (lo + hi);
        x = mid;
        if cdf(mid)? < p {
            lo = mid;
        } else {
            hi = mid;
        }
        if (hi - lo) <= 1e-15 * (1.0 + abs(mid)) {
            break;
        }
    }
    for _ in 0..5 {
        let value = cdf(x)?;
        let density = pdf(x);
        if density <= 0.0 || !density.is_finite() {
            break;
        }
        let step = (value - p) / density;
        if !step.is_finite() {
            break;
        }
        let mut next = x - step;
        if !next.is_finite() {
            break;
        }
        if next < lo {
            next = 0.5 * (lo + x);
        } else if next > hi {
            next = 0.5 * (hi + x);
        }
        if abs(next - x) <= 1e-16 * (1.0 + abs(next)) {
            x = next;
            break;
        }
        x = next;
    }
    if !x.is_finite() {
        return Err(non_convergence("quantile inversion"));
    }
    Ok(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(actual: f64, expected: f64, tolerance: f64) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "expected {expected}, got {actual} (tolerance {tolerance})"
        );
    }

    #[test]
    fn normal_reference_values() {
        close(
            normal_cdf(1.96, 0.0, 1.0).unwrap(),
            0.975_002_104_851_779_5,
            1e-12,
        );
        close(
            normal_quantile(0.975, 0.0, 1.0).unwrap(),
            1.959_963_984_540_054,
            1e-9,
        );
        // The commonly published 7.619853e-24 is the 7-digit rounded value;
        // compare against the full-precision reference within 1e-9 relative.
        let tail = normal_sf(10.0, 0.0, 1.0).unwrap();
        close(tail / 7.619_853_024_160_593e-24, 1.0, 1e-9);
        close(normal_cdf(0.0, 0.0, 1.0).unwrap(), 0.5, 0.0);
        close(normal_quantile(0.5, 0.0, 1.0).unwrap(), 0.0, 0.0);
    }

    #[test]
    fn student_t_reference_value() {
        // Provenance: for even df the Student-t CDF has the closed form
        // P = 1 - 0.5 * I_x(df/2, 1/2) with x = df/(df + t^2), and for integer
        // df/2 the incomplete beta reduces to a finite polynomial in
        // sqrt(1 - x). The reference below was computed independently in Python
        // with 60-digit Decimal arithmetic from that closed form
        // (student_t_cdf(2, 10) = 0.96330598261462981719...), not with this
        // crate's continued-fraction implementation.
        close(
            student_t_cdf(2.0, 10.0).unwrap(),
            0.963_305_982_614_629_8,
            1e-12,
        );
        close(
            student_t_sf(2.0, 10.0).unwrap(),
            0.036_694_017_385_370_18,
            1e-12,
        );
        let q = student_t_quantile(0.975, 10.0).unwrap();
        close(q, 2.228_138_851_964_938_5, 1e-8);
        close(student_t_cdf(q, 10.0).unwrap(), 0.975, 1e-10);
    }

    #[test]
    fn chi_square_reference_values() {
        close(
            chi_square_cdf(3.841_458_820_694_124, 1.0).unwrap(),
            0.95,
            1e-12,
        );
        let q = chi_square_quantile(0.95, 1.0).unwrap();
        close(q, 3.841_458_820_694_124, 1e-9);
        close(chi_square_cdf(q, 1.0).unwrap(), 0.95, 1e-10);
        close(chi_square_pdf(0.0, 2.0).unwrap(), 0.5, 0.0);
    }

    #[test]
    fn binomial_reference_value() {
        close(binomial_cdf(5.0, 10.0, 0.5).unwrap(), 0.623_046_875, 1e-15);
        close(binomial_pmf(0.0, 0.0, 0.5).unwrap(), 1.0, 0.0);
        close(binomial_pmf(5.0, 10.0, 0.5).unwrap(), 252.0 / 1024.0, 1e-15);
    }

    #[test]
    fn erfc_tail_is_relative_accurate() {
        // erfc(5) = 1.5374597944280351e-12
        close(erfc(5.0).unwrap(), 1.537_459_794_428_035_1e-12, 1e-24);
        close(erf(1.0).unwrap(), 0.842_700_792_949_715, 1e-15);
    }

    #[test]
    fn binomial_edge_cases_are_exact() {
        close(binomial_pmf(0.0, 0.0, 0.5).unwrap(), 1.0, 0.0);
        close(
            binomial_pmf(0.0, 10.0, 0.3).unwrap(),
            0.7f64.powi(10),
            1e-18,
        );
        close(
            binomial_pmf(10.0, 10.0, 0.3).unwrap(),
            0.3f64.powi(10),
            1e-18,
        );
        close(binomial_cdf(-1.0, 10.0, 0.5).unwrap(), 0.0, 0.0);
        close(binomial_cdf(10.0, 10.0, 0.5).unwrap(), 1.0, 0.0);
    }

    #[test]
    fn round_trips_are_tight() {
        for p in [1e-6, 0.01, 0.25, 0.5, 0.75, 0.99, 1.0 - 1e-6] {
            let q = normal_quantile(p, 0.0, 1.0).unwrap();
            close(normal_cdf(q, 0.0, 1.0).unwrap(), p, 1e-10);
            let t = student_t_quantile(p, 7.0).unwrap();
            close(student_t_cdf(t, 7.0).unwrap(), p, 1e-10);
            let c = chi_square_quantile(p, 3.0).unwrap();
            close(chi_square_cdf(c, 3.0).unwrap(), p, 1e-10);
        }
    }
}
