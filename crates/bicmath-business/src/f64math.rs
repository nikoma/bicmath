//! Platform-consistent binary64 wrappers for the business module.
//!
//! On `wasm32-unknown-unknown` the standard library's floating-point `sqrt` is
//! not available, so calls are routed through `libm`. Every wrapper is pure,
//! deterministic, and never panics.

#[cfg(target_arch = "wasm32")]
#[inline]
pub(crate) fn sqrt(x: f64) -> f64 {
    libm::sqrt(x)
}

#[cfg(not(target_arch = "wasm32"))]
#[inline]
pub(crate) fn sqrt(x: f64) -> f64 {
    x.sqrt()
}
