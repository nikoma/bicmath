//! Format module: pure text rendering of values.
//!
//! The module renders the recursive wire [`Value`](bicmath_core::value::Value)
//! model as Markdown, LaTeX, RFC 4180 CSV, and aligned plain text, plus a
//! consistent scalar number renderer. Rendering is deterministic and
//! side-effect free: there is no filesystem, network, clock, randomness, or
//! unsafe code, so the module is safe to compile to WebAssembly.
//!
//! Shape rules shared by the renderers:
//!
//! | Input shape | Markdown | LaTeX | CSV | Text |
//! |---|---|---|---|---|
//! | scalar | inline | equation | single cell | inline |
//! | matrix | table | tabular | rows | aligned table |
//! | array of records | table | tabular | header + rows | aligned table |
//! | array of scalars | bullet list | itemize | one column | one per line |
//! | record | key/value table | tabular | key,value | aligned table |
//! | nested | fenced JSON | verbatim | error | pretty JSON |
//!
//! Only values that genuinely cannot be tabulated (for example a matrix whose
//! entries are themselves structured) are rejected; everything else falls back
//! to a code block or an inline JSON cell. Decimal values always keep their
//! declared scale, so `0.10` renders as `0.10`, never `0.1`.

#![forbid(unsafe_code)]

mod functions;
mod render;
#[cfg(test)]
mod tests;

use bicmath_core::contract::{Module, ModuleDescriptor};

/// Build the format module with all of its registered functions.
pub fn module() -> Module {
    let descriptor: ModuleDescriptor = functions::module_descriptor();
    Module::new(descriptor, functions::functions())
}
