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

macro_rules! binary {
    ($name:ident, $method:ident, $libm:ident) => {
        #[cfg(target_arch = "wasm32")]
        #[inline]
        pub(crate) fn $name(x: f64, y: f64) -> f64 {
            libm::$libm(x, y)
        }

        #[cfg(not(target_arch = "wasm32"))]
        #[inline]
        pub(crate) fn $name(x: f64, y: f64) -> f64 {
            x.$method(y)
        }
    };
}

unary!(sin, sin, sin);
unary!(cos, cos, cos);
unary!(asin, asin, asin);
unary!(acos, acos, acos);
unary!(sqrt, sqrt, sqrt);
binary!(atan2, atan2, atan2);
