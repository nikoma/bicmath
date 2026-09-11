//! Binary64 mathematical constants.

use std::collections::BTreeMap;
use std::sync::Arc;

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, CostClass, Example, Function, FunctionDescriptor, Outcome, SimpleFunction,
};
use bicmath_core::error::EngineError;
use bicmath_core::schema::{FieldSchema, ValueSchema};
use bicmath_core::value::Value;

use crate::common::*;

fn constants_output_schema() -> ValueSchema {
    ValueSchema::Record {
        fields: vec![
            FieldSchema::required("pi", float64_schema()),
            FieldSchema::required("e", float64_schema()),
            FieldSchema::required("tau", float64_schema()),
            FieldSchema::required("ln2", float64_schema()),
            FieldSchema::required("ln10", float64_schema()),
        ],
        allow_extra: false,
    }
}

pub(crate) fn constants_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "scientific.constants",
        MODULE,
        VERSION,
        "Mathematical constants",
        "Correctly rounded binary64 mathematical constants.",
    )
    .with_description(
        "Returns pi, e, tau = 2 * pi, ln 2, and ln 10 as the correctly rounded binary64 \
         values of the real constants. No parameters.",
    )
    .with_parameters(Vec::new())
    .with_output(
        constants_output_schema(),
        "Record with float64 fields pi, e, tau, ln2, and ln10.",
    )
    .with_modes(scientific_modes())
    .with_cost(CostClass::Constant)
    .with_method_ref("docs/methods/scientific.md#constants")
    .with_examples(vec![
        Example::new("binary64 constants", BTreeMap::new()).with_value(parse_value(
            serde_json::json!({
                "pi": {"kind": "float64", "value": "3.141592653589793"},
                "e": {"kind": "float64", "value": "2.718281828459045"},
                "tau": {"kind": "float64", "value": "6.283185307179586"},
                "ln2": {"kind": "float64", "value": "0.6931471805599453"},
                "ln10": {"kind": "float64", "value": "2.302585092994046"}
            }),
        )),
    ])
}

pub(crate) fn invoke_constants(_args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_scientific(ctx, "scientific.constants")?;
    let value = Value::record([
        ("pi", float_value(std::f64::consts::PI)?),
        ("e", float_value(std::f64::consts::E)?),
        ("tau", float_value(std::f64::consts::TAU)?),
        ("ln2", float_value(std::f64::consts::LN_2)?),
        ("ln10", float_value(std::f64::consts::LN_10)?),
    ]);
    Ok(Outcome::approximate(value))
}

pub(crate) fn functions() -> Vec<Arc<dyn Function>> {
    vec![SimpleFunction::arc(
        constants_descriptor(),
        invoke_constants,
    )]
}
