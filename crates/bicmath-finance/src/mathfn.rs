//! Local special functions for option pricing and risk measures.
//!
//! The error function is implemented from the regularized incomplete gamma
//! series and continued fraction (Numerical Recipes style, with explicit
//! iteration bounds and a structured `NonConvergence` error). No external
//! statistics crate is used. On wasm32 every transcendental falls back to
//! `libm`; on other targets the standard library is used.

use bicmath_core::error::{EngineError, ErrorCode};

const EPS: f64 = 1e-16;
const FPMIN: f64 = f64::MIN_POSITIVE / EPS;
const MAX_ITER: u64 = 100_000;
const LN_2_PI: f64 = 1.837_877_066_409_345_3;
const SQRT_2PI: f64 = 2.506_628_274_631_000_7;

// ---------------------------------------------------------------------------
// wasm-safe transcendental wrappers
// ---------------------------------------------------------------------------

#[cfg(target_arch = "wasm32")]
pub(crate) fn exp(x: f64) -> f64 {
    libm::exp(x)
}
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn exp(x: f64) -> f64 {
    x.exp()
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn ln(x: f64) -> f64 {
    libm::log(x)
}
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn ln(x: f64) -> f64 {
    x.ln()
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn sqrt(x: f64) -> f64 {
    libm::sqrt(x)
}
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn sqrt(x: f64) -> f64 {
    x.sqrt()
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn abs(x: f64) -> f64 {
    libm::fabs(x)
}
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn abs(x: f64) -> f64 {
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

/// Lanczos approximation to `ln|Gamma(x)|`.
fn lgamma(x: f64) -> f64 {
    if x < 0.5 {
        let pi = std::f64::consts::PI;
        return ln(pi / abs(sin_pi(x))) - lgamma(1.0 - x);
    }
    let z = x - 1.0;
    let mut series = LANCZOS[0];
    for (index, coefficient) in LANCZOS.iter().enumerate().skip(1) {
        series += coefficient / (z + index as f64);
    }
    let t = z + 7.5;
    0.5 * LN_2_PI + (z + 0.5) * ln(t) - t + ln(series)
}

#[cfg(target_arch = "wasm32")]
fn sin_pi(x: f64) -> f64 {
    libm::sin(std::f64::consts::PI * x)
}
#[cfg(not(target_arch = "wasm32"))]
fn sin_pi(x: f64) -> f64 {
    (std::f64::consts::PI * x).sin()
}

// ---------------------------------------------------------------------------
// Regularized incomplete gamma
// ---------------------------------------------------------------------------

/// Regularized lower incomplete gamma `P(a, x)`.
fn gamma_p(a: f64, x: f64) -> Result<f64, EngineError> {
    if x <= 0.0 {
        return Ok(0.0);
    }
    if x < a + 1.0 {
        gamma_p_series(a, x)
    } else {
        Ok(1.0 - gamma_q_cf(a, x)?)
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
// Error function and standard normal distribution
// ---------------------------------------------------------------------------

/// Error function, computed from the regularized incomplete gamma:
/// `erf(x) = P(1/2, x^2)` for `x >= 0`.
pub(crate) fn erf(x: f64) -> Result<f64, EngineError> {
    if x == 0.0 {
        return Ok(0.0);
    }
    let magnitude = gamma_p(0.5, x * x)?;
    Ok(if x > 0.0 { magnitude } else { -magnitude })
}

/// Standard normal cumulative distribution `Phi(x)`.
pub(crate) fn normal_cdf(x: f64) -> Result<f64, EngineError> {
    Ok(0.5 * (1.0 + erf(x / std::f64::consts::SQRT_2)?))
}

/// Standard normal probability density `phi(x)`.
pub(crate) fn normal_pdf(x: f64) -> f64 {
    exp(-0.5 * x * x) / SQRT_2PI
}

/// Standard normal quantile `Phi^-1(p)` for `p` strictly inside `(0, 1)`.
///
/// Uses bracket expansion, bisection, and Newton polishing, mirroring the
/// documented method in `docs/methods/finance.md`.
pub(crate) fn normal_quantile(p: f64) -> Result<f64, EngineError> {
    if !p.is_finite() || p <= 0.0 || p >= 1.0 {
        return Err(EngineError::domain(
            "normal_quantile requires p strictly between 0 and 1",
        ));
    }
    if p == 0.5 {
        return Ok(0.0);
    }
    invert_monotone(p)
}

fn invert_monotone(p: f64) -> Result<f64, EngineError> {
    let mut lo = -1.0f64;
    let mut hi = 1.0f64;
    let mut expansions = 0u32;
    while normal_cdf(lo)? > p {
        hi = lo;
        lo = if lo < 0.0 { lo * 2.0 } else { -1.0 };
        expansions += 1;
        if expansions > 4_000 || !lo.is_finite() {
            return Err(non_convergence("quantile bracket expansion"));
        }
    }
    while normal_cdf(hi)? < p {
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
        if normal_cdf(mid)? < p {
            lo = mid;
        } else {
            hi = mid;
        }
        if (hi - lo) <= 1e-15 * (1.0 + abs(mid)) {
            break;
        }
    }
    for _ in 0..5 {
        let value = normal_cdf(x)?;
        let density = normal_pdf(x);
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
    fn error_function_reference_values() {
        // Python 3 math.erf fixtures.
        close(erf(1.0).unwrap(), 0.842_700_792_949_714_8, 1e-14);
        close(erf(-1.0).unwrap(), -0.842_700_792_949_714_8, 1e-14);
        assert_eq!(erf(0.0).unwrap(), 0.0);
    }

    #[test]
    fn normal_quantile_matches_python() {
        // Python 3 statistics.NormalDist().inv_cdf fixtures.
        close(
            normal_quantile(0.95).unwrap(),
            1.644_853_626_951_472_2,
            1e-12,
        );
        close(
            normal_quantile(0.975).unwrap(),
            1.959_963_984_540_054,
            1e-12,
        );
        close(normal_quantile(0.5).unwrap(), 0.0, 0.0);
        close(
            normal_quantile(0.01).unwrap(),
            -2.326_347_874_040_841,
            1e-12,
        );
    }

    #[test]
    fn normal_quantile_rejects_out_of_range_probabilities() {
        assert!(normal_quantile(0.0).is_err());
        assert!(normal_quantile(1.0).is_err());
        assert!(normal_quantile(-0.1).is_err());
    }
}
