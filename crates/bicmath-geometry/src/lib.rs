//! Geometry module: planar geometry, triangle solving, spherical geodesy, and
//! 3D rotations.
//!
//! Algebraic results (sums of squares, shoelace areas, Hamilton products) are
//! computed with exact rational arithmetic; transcendental steps (non-perfect
//! square roots, trigonometry) use binary64 through `libm` fallbacks and are
//! labelled approximate. The module contains no transport, filesystem, clock,
//! or network code and is wasm-safe.

mod common;
mod f64math;
mod geodesy;
mod planar;
mod rotation;
mod scalar;
mod solids;
#[cfg(test)]
mod tests;
mod triangle;

use std::sync::Arc;

use bicmath_core::contract::{Function, Module, ModuleDescriptor};
use bicmath_core::number::NumericMode;

/// Build the geometry module with all of its registered functions.
pub fn module() -> Module {
    let mut functions: Vec<Arc<dyn Function>> = Vec::new();
    planar::register(&mut functions);
    triangle::register(&mut functions);
    geodesy::register(&mut functions);
    rotation::register(&mut functions);
    solids::register(&mut functions);
    let descriptor = ModuleDescriptor::new(
        "geometry",
        "Geometry",
        "1.0.0",
        "Planar geometry, triangle solving, spherical geodesy, and 3D quaternion rotations.",
    )
    .with_capabilities(vec![
        "exact_planar_geometry",
        "triangle_solving",
        "spherical_geodesy",
        "quaternion_rotation",
    ])
    .with_dependencies(vec!["core"])
    .with_modes(vec![
        NumericMode::Exact,
        NumericMode::Auto,
        NumericMode::Scientific,
    ])
    .with_source("crates/bicmath-geometry");
    Module::new(descriptor, functions)
}
