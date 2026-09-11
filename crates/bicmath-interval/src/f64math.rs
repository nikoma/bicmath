//! Platform-consistent wrappers for binary64 transcendental functions.
//!
//! On `wasm32-unknown-unknown` the standard library's floating-point
//! transcendentals are not available, so every call is routed through `libm`.
//! On every other target the intrinsic `f64` method is used. The wrappers are
//! pure, deterministic, and never panic.

macro_rules! unary {
    ($name:ident, $method:ident, $libm:ident) => {
        #[cfg(target_arch = "wasm32")]
        #[inline]
        pub(crate) fn $name(x: f64) -> f64 {
            libm::$libm(x)
        }

        #[cfg(not(target_arch = "wasm32"))]
        #[inline]
        pub(crate) fn $name(x: f64) -> f64 {
            x.$method()
        }
    };
}

unary!(sin, sin, sin);
unary!(cos, cos, cos);
unary!(tan, tan, tan);
unary!(exp, exp, exp);
unary!(ln, ln, log);
unary!(log10, log10, log10);
unary!(log2, log2, log2);
unary!(sqrt, sqrt, sqrt);
unary!(floor, floor, floor);
unary!(ceil, ceil, ceil);
