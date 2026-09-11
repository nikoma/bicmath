//! Units module: a versioned unit registry, dimensional algebra, and exact
//! conversions built on the shared quantity representation in `bicmath-core`.

mod check;
mod registry;

use std::collections::BTreeMap;
use std::sync::Arc;

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, Assumption, CostClass, Example, Function, FunctionDescriptor, Module, ModuleDescriptor,
    Outcome, ParamDescriptor, SimpleFunction,
};
use bicmath_core::envelope::Exactness;
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::limits::Limits;
use bicmath_core::number::{Decimal, Float64, Number, NumberResult, NumericContext, NumericMode};
use bicmath_core::schema::{FieldSchema, ValueSchema};
use bicmath_core::value::{DIM_TEMPERATURE, DIMENSION_NAMES, Dimension, Value};
use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{ToPrimitive, Zero};

pub use registry::REGISTRY_VERSION;

use registry::{Factor, Unit};

fn all_modes() -> Vec<NumericMode> {
    vec![
        NumericMode::Exact,
        NumericMode::Auto,
        NumericMode::Scientific,
    ]
}

fn temperature_dimension() -> Dimension {
    Dimension::new([0, 0, 0, 0, 1, 0, 0, 0])
}

fn length_dimension() -> Dimension {
    Dimension::new([1, 0, 0, 0, 0, 0, 0, 0])
}

fn area_dimension() -> Dimension {
    Dimension::new([2, 0, 0, 0, 0, 0, 0, 0])
}

fn volume_dimension() -> Dimension {
    Dimension::new([3, 0, 0, 0, 0, 0, 0, 0])
}

fn time_dimension() -> Dimension {
    Dimension::new([0, 0, 1, 0, 0, 0, 0, 0])
}

#[cfg(test)]
fn speed_dimension() -> Dimension {
    Dimension::new([1, 0, -1, 0, 0, 0, 0, 0])
}

#[cfg(test)]
fn pressure_dimension() -> Dimension {
    Dimension::new([-1, 1, -2, 0, 0, 0, 0, 0])
}

fn exact_number(value: BigRational, limits: &Limits) -> Number {
    if value.is_integer() {
        return Number::Integer(value.to_integer());
    }
    match Decimal::from_rational(&value, &NumericContext::exact(), limits) {
        Ok((decimal, false)) => Number::Decimal(decimal),
        _ => Number::Rational(value),
    }
}

fn dimension_value(dimension: Dimension) -> Value {
    let mut fields = BTreeMap::new();
    for (index, name) in DIMENSION_NAMES.iter().enumerate() {
        let exponent = dimension.get(index);
        if exponent != 0 {
            fields.insert(
                (*name).to_string(),
                Value::Number(Number::Integer(BigInt::from(exponent))),
            );
        }
    }
    Value::Record(fields)
}

fn factor_value(factor: &Factor, limits: &Limits) -> Value {
    match factor {
        Factor::Exact(value) => Value::Number(exact_number(value.clone(), limits)),
        Factor::Approx(value) => Value::Number(Number::Float64(
            Float64::new(*value).expect("registered unit factors are finite"),
        )),
    }
}

fn unit_record(unit: &Unit, with_notes: bool, limits: &Limits) -> Value {
    let mut fields = BTreeMap::new();
    fields.insert("id".to_string(), Value::text(unit.id));
    fields.insert("symbol".to_string(), Value::text(unit.symbol));
    fields.insert("name".to_string(), Value::text(unit.name));
    fields.insert("unit_kind".to_string(), Value::text(unit.kind));
    fields.insert("dimension".to_string(), dimension_value(unit.dimension));
    fields.insert("factor".to_string(), factor_value(&unit.factor, limits));
    fields.insert(
        "offset".to_string(),
        match &unit.offset {
            Some(value) => Value::Number(exact_number(value.clone(), limits)),
            None => Value::Null,
        },
    );
    fields.insert("exact_factor".to_string(), Value::Bool(unit.exact_factor));
    fields.insert(
        "aliases".to_string(),
        Value::Array(
            unit.aliases
                .iter()
                .map(|alias| Value::text(*alias))
                .collect(),
        ),
    );
    fields.insert("affine".to_string(), Value::Bool(unit.affine));
    if with_notes {
        fields.insert("notes".to_string(), Value::text(unit.notes));
    }
    Value::Record(fields)
}

fn unit_record_schema(with_notes: bool) -> ValueSchema {
    let mut fields = vec![
        FieldSchema::required("id", ValueSchema::text()),
        FieldSchema::required("symbol", ValueSchema::text()),
        FieldSchema::required("name", ValueSchema::text()),
        FieldSchema::required("unit_kind", ValueSchema::text()),
        FieldSchema::required("dimension", ValueSchema::Any),
        FieldSchema::required("factor", ValueSchema::Any),
        FieldSchema::required("offset", ValueSchema::Any),
        FieldSchema::required("exact_factor", ValueSchema::Bool),
        FieldSchema::required("aliases", ValueSchema::array(ValueSchema::text())),
        FieldSchema::required("affine", ValueSchema::Bool),
    ];
    if with_notes {
        fields.push(FieldSchema::required("notes", ValueSchema::text()));
    }
    ValueSchema::Record {
        fields,
        allow_extra: false,
    }
}

fn quantity_value(value: Number, dimension: Dimension) -> Value {
    Value::Quantity {
        value: Box::new(Value::Number(value)),
        dimension,
    }
}

fn int_quantity(value: i64, dimension: Dimension) -> Value {
    quantity_value(Number::Integer(BigInt::from(value)), dimension)
}

fn decimal_quantity(value: &str, dimension: Dimension) -> Value {
    quantity_value(
        Number::Decimal(Decimal::parse_default(value).expect("example decimal literal")),
        dimension,
    )
}

fn example_args(pairs: &[(&str, Value)]) -> BTreeMap<String, Value> {
    pairs
        .iter()
        .map(|(name, value)| ((*name).to_string(), value.clone()))
        .collect()
}

fn describe_example(id: &str) -> Value {
    match registry::find_unit(id) {
        Ok(unit) => unit_record(unit, true, &Limits::conservative()),
        Err(_) => Value::Null,
    }
}

struct Magnitude {
    value: Number,
    dimension: Dimension,
    is_quantity: bool,
}

fn magnitude(value: &Value, name: &str) -> Result<Magnitude, EngineError> {
    match value {
        Value::Quantity { value, dimension } => Ok(Magnitude {
            value: value.as_number()?.clone(),
            dimension: *dimension,
            is_quantity: true,
        }),
        Value::Number(number) => Ok(Magnitude {
            value: number.clone(),
            dimension: Dimension::DIMENSIONLESS,
            is_quantity: false,
        }),
        Value::Money { .. } => Err(EngineError::domain(
            "money is not a physical quantity; currency is not a dimension",
        )
        .with_path(name.to_string())),
        other => Err(EngineError::malformed(format!(
            "{name} must be a quantity or a number, found {}",
            other.kind_name()
        ))
        .with_path(name.to_string())),
    }
}

fn extract_magnitude(
    raw: &Value,
    expected: Dimension,
    context: &str,
) -> Result<(Number, bool), EngineError> {
    match raw {
        Value::Quantity { value, dimension } => {
            if *dimension != expected {
                return Err(EngineError::new(
                    ErrorCode::IncompatibleUnits,
                    format!("{context} has dimension {dimension}, expected {expected}"),
                ));
            }
            Ok((value.as_number()?.clone(), false))
        }
        Value::Number(number) => Ok((number.clone(), true)),
        Value::Money { .. } => Err(EngineError::domain(
            "money is not a physical quantity; currency is not a dimension",
        )),
        other => Err(EngineError::malformed(format!(
            "{context} must be a quantity or a number, found {}",
            other.kind_name()
        ))),
    }
}

fn offset_exact(unit: &Unit, apply: bool) -> BigRational {
    if !apply {
        return BigRational::zero();
    }
    unit.offset.clone().unwrap_or_else(BigRational::zero)
}

fn offset_f64(unit: &Unit, apply: bool) -> f64 {
    if !apply {
        return 0.0;
    }
    unit.offset
        .as_ref()
        .and_then(|value| value.to_f64())
        .unwrap_or(0.0)
}

fn apply_conversion(
    input: &Number,
    plain: bool,
    from: &Unit,
    to: &Unit,
    apply_offsets: bool,
    ctx: &ExecContext,
) -> Result<(Number, Exactness), EngineError> {
    if let Number::Float64(value) = input {
        if ctx.numeric.mode != NumericMode::Scientific {
            return Err(EngineError::new(
                ErrorCode::UnsupportedNumericMode,
                "converting a float64 quantity requires scientific mode",
            ));
        }
        let base = if plain {
            value.get()
        } else {
            value.get() * from.factor.to_f64() + offset_f64(from, apply_offsets)
        };
        let converted = (base - offset_f64(to, apply_offsets)) / to.factor.to_f64();
        return Ok((Number::float(converted)?, Exactness::Approximate));
    }
    if let (Some(from_factor), Some(to_factor)) = (from.factor.as_exact(), to.factor.as_exact()) {
        let exact = input.to_exact_rational().ok_or_else(|| {
            EngineError::internal("exact unit operand is not representable as a rational")
        })?;
        let base = if plain {
            exact
        } else {
            exact * from_factor + offset_exact(from, apply_offsets)
        };
        let converted = (base - offset_exact(to, apply_offsets)) / to_factor;
        return Ok((exact_number(converted, &ctx.limits), Exactness::Exact));
    }
    let approximate = input
        .to_f64()
        .ok_or_else(|| EngineError::domain("value is not representable as float64"))?;
    let base = if plain {
        approximate
    } else {
        approximate * from.factor.to_f64() + offset_f64(from, apply_offsets)
    };
    let converted = (base - offset_f64(to, apply_offsets)) / to.factor.to_f64();
    Ok((Number::float(converted)?, Exactness::Approximate))
}

fn classify(result: &NumberResult) -> Exactness {
    if matches!(result.value, Number::Float64(_)) {
        Exactness::Approximate
    } else if result.rounded {
        Exactness::Rounded
    } else {
        Exactness::Exact
    }
}

fn arithmetic_outcome(result: NumberResult, dimension: Dimension, as_quantity: bool) -> Outcome {
    let exactness = classify(&result);
    let value = if as_quantity {
        quantity_value(result.value, dimension)
    } else {
        Value::Number(result.value)
    };
    Outcome::new(value, exactness)
}

fn reject_temperature(value: &Magnitude, operation: &str) -> Result<(), EngineError> {
    if value.dimension.get(DIM_TEMPERATURE) != 0 {
        return Err(EngineError::domain(format!(
            "cannot {operation} an absolute affine temperature; only temperature differences \
             may participate in dimensional algebra"
        )));
    }
    Ok(())
}

fn invoke_list_units(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let filter = args.optional_text("quantity_kind")?;
    if let Some(kind) = filter {
        let kinds = valid_kinds();
        if !kinds.contains(&kind) {
            return Err(EngineError::domain(format!(
                "unknown quantity kind {kind:?}; expected one of: {}",
                kinds.join(", ")
            )));
        }
    }
    let records: Vec<Value> = registry::registry()
        .iter()
        .filter(|unit| filter.is_none_or(|kind| unit.kind == kind))
        .map(|unit| unit_record(unit, false, &ctx.limits))
        .collect();
    Ok(Outcome::exact(Value::Array(records)))
}

fn valid_kinds() -> Vec<&'static str> {
    let mut kinds: Vec<&'static str> = registry::registry().iter().map(|unit| unit.kind).collect();
    kinds.sort_unstable();
    kinds.dedup();
    kinds
}

fn invoke_describe_unit(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let unit = registry::find_unit(args.text("unit")?)?;
    Ok(Outcome::exact(unit_record(unit, true, &ctx.limits)))
}

fn invoke_convert(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let from = registry::find_unit(args.text("from")?)?;
    let to = registry::find_unit(args.text("to")?)?;
    if from.dimension != to.dimension {
        return Err(EngineError::new(
            ErrorCode::IncompatibleUnits,
            format!(
                "cannot convert from {} ({}) to {} ({})",
                from.id, from.dimension, to.id, to.dimension
            ),
        ));
    }
    let raw = args.require("value")?;
    let (number, plain) =
        extract_magnitude(raw, from.dimension, &format!("value for unit {}", from.id))?;
    let (converted, exactness) = apply_conversion(&number, plain, from, to, true, ctx)?;
    Ok(Outcome::new(
        quantity_value(converted, to.dimension),
        exactness,
    ))
}

fn invoke_temperature_difference(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let from = registry::find_unit(args.text("from")?)?;
    let to = registry::find_unit(args.text("to")?)?;
    let temperature = temperature_dimension();
    if from.dimension != temperature || to.dimension != temperature {
        return Err(EngineError::domain(
            "temperature_difference requires temperature units for both from and to",
        ));
    }
    let raw = args.require("value")?;
    let (number, plain) =
        extract_magnitude(raw, temperature, &format!("value for unit {}", from.id))?;
    let (converted, exactness) = apply_conversion(&number, plain, from, to, false, ctx)?;
    let outcome = Outcome::new(quantity_value(converted, temperature), exactness).with_assumption(
        Assumption::checked(
            "temperature_delta",
            "the result is a temperature difference; affine offsets are not applied",
        ),
    );
    Ok(outcome)
}

fn invoke_multiply(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let a = magnitude(args.require("a")?, "a")?;
    let b = magnitude(args.require("b")?, "b")?;
    reject_temperature(&a, "multiply")?;
    reject_temperature(&b, "multiply")?;
    let dimension = a.dimension.multiply(&b.dimension)?;
    let result = a.value.mul(&b.value, &ctx.numeric, &ctx.limits)?;
    Ok(arithmetic_outcome(
        result,
        dimension,
        a.is_quantity || b.is_quantity,
    ))
}

fn invoke_divide(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let a = magnitude(args.require("a")?, "a")?;
    let b = magnitude(args.require("b")?, "b")?;
    reject_temperature(&a, "divide")?;
    reject_temperature(&b, "divide")?;
    let dimension = a.dimension.divide(&b.dimension)?;
    let result = a.value.div(&b.value, &ctx.numeric, &ctx.limits)?;
    Ok(arithmetic_outcome(
        result,
        dimension,
        a.is_quantity || b.is_quantity,
    ))
}

fn invoke_power(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let base = magnitude(args.require("a")?, "a")?;
    reject_temperature(&base, "raise to a power")?;
    let exponent = args.number("exponent")?;
    let exponent_rational = exponent.to_exact_rational();
    let dimension = match &exponent_rational {
        Some(value) if value.is_integer() => {
            let exponent = value.to_integer().to_i32().ok_or_else(|| {
                EngineError::domain("dimension exponent is out of the supported range")
            })?;
            base.dimension.pow(exponent)?
        }
        Some(_) if base.is_quantity => {
            return Err(EngineError::domain(
                "raising a quantity to a power requires an integer exponent",
            ));
        }
        _ => Dimension::DIMENSIONLESS,
    };
    let result = base.value.pow(exponent, &ctx.numeric, &ctx.limits)?;
    Ok(arithmetic_outcome(
        result,
        dimension,
        base.is_quantity || !dimension.is_dimensionless(),
    ))
}

fn invoke_add(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    sum_or_difference(args, ctx, false)
}

fn invoke_subtract(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    sum_or_difference(args, ctx, true)
}

fn sum_or_difference(
    args: &Args,
    ctx: &ExecContext,
    subtract: bool,
) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let a = magnitude(args.require("a")?, "a")?;
    let b = magnitude(args.require("b")?, "b")?;
    if a.dimension != b.dimension {
        return Err(EngineError::new(
            ErrorCode::IncompatibleUnits,
            format!(
                "cannot combine quantities with dimensions {} and {}",
                a.dimension, b.dimension
            ),
        ));
    }
    let is_temperature = a.dimension.get(DIM_TEMPERATURE) != 0;
    if is_temperature && !subtract {
        return Err(EngineError::domain(
            "adding absolute temperatures is not defined; subtract them to obtain a \
             temperature difference",
        ));
    }
    let result = if subtract {
        a.value.sub(&b.value, &ctx.numeric, &ctx.limits)?
    } else {
        a.value.add(&b.value, &ctx.numeric, &ctx.limits)?
    };
    let mut outcome = arithmetic_outcome(result, a.dimension, a.is_quantity || b.is_quantity);
    if is_temperature && subtract {
        outcome = outcome.with_assumption(Assumption::checked(
            "temperature_delta",
            "subtracting absolute temperatures yields a temperature difference",
        ));
    }
    Ok(outcome)
}

fn invoke_is_dimensionless(args: &Args, _ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let value = magnitude(args.require("value")?, "value")?;
    Ok(Outcome::exact(Value::Bool(
        value.dimension.is_dimensionless(),
    )))
}

fn list_units_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "units.list_units",
        "units",
        "1.0.0",
        "List units",
        "List the versioned unit registry, optionally filtered by quantity kind.",
    )
    .with_description(
        "Returns one record per registered unit with its id, symbol, name, quantity kind, \
         dimension, exact factor to SI base units, affine offset (temperature only), and \
         aliases. Factors are exact rationals except the pi-based angle units, which are \
         flagged with exact_factor=false.",
    )
    .with_parameters(vec![ParamDescriptor::optional(
        "quantity_kind",
        "Optional kind filter: length, mass, time, current, temperature_absolute, amount, \
         luminous, angle, area, volume, speed, pressure, energy, or power.",
        ValueSchema::text(),
    )])
    .with_output(
        ValueSchema::array(unit_record_schema(false)),
        "Array of unit records.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_method_ref("docs/methods/units.md#list_units")
    .with_examples(vec![Example::new(
        "list length units",
        example_args(&[("quantity_kind", Value::text("length"))]),
    )])
}

fn describe_unit_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "units.describe_unit",
        "units",
        "1.0.0",
        "Describe unit",
        "Describe one unit in the versioned registry, including notes.",
    )
    .with_description(
        "Ambiguous identifiers such as gallon, ton, and fluid_ounce are rejected with the \
         qualified alternatives.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "unit",
        "Registry unit id, symbol, or unambiguous alias, e.g. meter or us_gallon.",
        ValueSchema::text(),
    )])
    .with_output(unit_record_schema(true), "Unit record with notes.")
    .with_modes(all_modes())
    .with_method_ref("docs/methods/units.md#describe_unit")
    .with_examples(vec![
        Example::new(
            "describe the meter",
            example_args(&[("unit", Value::text("meter"))]),
        )
        .with_value(describe_example("meter")),
        Example::new(
            "ambiguous identifier",
            example_args(&[("unit", Value::text("gallon"))]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn convert_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "units.convert",
        "units",
        "1.0.0",
        "Convert units",
        "Convert a quantity between units of the same dimension.",
    )
    .with_description(
        "from and to are registry unit ids. A quantity carries its dimension and is checked \
         against the from unit; a plain number is interpreted in the SI base units of the \
         from unit's dimension. Absolute temperatures apply the affine offset; temperature \
         differences use units.temperature_difference. Results are exact when both factors \
         are exact and approximate when a factor is an approximation such as pi/180.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "value",
            "Quantity, or plain number in SI base units of the from unit's dimension.",
            ValueSchema::Any,
        ),
        ParamDescriptor::required("from", "Source unit id, e.g. mile.", ValueSchema::text()),
        ParamDescriptor::required("to", "Target unit id, e.g. kilometer.", ValueSchema::text()),
    ])
    .with_output(
        ValueSchema::Quantity {
            dimension: None,
            allow_delta: true,
        },
        "Converted quantity with the target unit's dimension.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Constant)
    .with_method_ref("docs/methods/units.md#convert")
    .with_examples(vec![
        Example::new(
            "one mile in kilometers",
            example_args(&[
                ("value", int_quantity(1, length_dimension())),
                ("from", Value::text("mile")),
                ("to", Value::text("kilometer")),
            ]),
        )
        .with_value(decimal_quantity("1.609344", length_dimension())),
        Example::new(
            "zero degrees Celsius in kelvin",
            example_args(&[
                ("value", int_quantity(0, temperature_dimension())),
                ("from", Value::text("degree_celsius")),
                ("to", Value::text("kelvin")),
            ]),
        )
        .with_value(decimal_quantity("273.15", temperature_dimension())),
        Example::new(
            "ambiguous unit identifier",
            example_args(&[
                ("value", int_quantity(1, volume_dimension())),
                ("from", Value::text("gallon")),
                ("to", Value::text("liter")),
            ]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn multiply_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "units.multiply",
        "units",
        "1.0.0",
        "Multiply quantities",
        "Multiply quantities, adding their dimension exponents.",
    )
    .with_description(
        "A plain number is treated as dimensionless. Absolute affine temperatures are \
         rejected; only temperature differences may participate in dimensional algebra.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("a", "Left quantity or number.", ValueSchema::Any),
        ParamDescriptor::required("b", "Right quantity or number.", ValueSchema::Any),
    ])
    .with_output(
        ValueSchema::Any,
        "Product as a quantity, or a plain number when both operands are plain numbers.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Constant)
    .with_method_ref("docs/methods/units.md#multiply")
    .with_examples(vec![
        Example::new(
            "area from two lengths",
            example_args(&[
                ("a", int_quantity(2, length_dimension())),
                ("b", int_quantity(3, length_dimension())),
            ]),
        )
        .with_value(int_quantity(6, area_dimension())),
    ])
}

fn divide_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "units.divide",
        "units",
        "1.0.0",
        "Divide quantities",
        "Divide quantities, subtracting their dimension exponents.",
    )
    .with_description(
        "A plain number is treated as dimensionless. Absolute affine temperatures are \
         rejected; only temperature differences may participate in dimensional algebra.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("a", "Dividend quantity or number.", ValueSchema::Any),
        ParamDescriptor::required(
            "b",
            "Divisor quantity or number; must not be zero.",
            ValueSchema::Any,
        ),
    ])
    .with_output(
        ValueSchema::Any,
        "Quotient as a quantity, or a plain number when both operands are plain numbers.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Constant)
    .with_method_ref("docs/methods/units.md#divide")
    .with_examples(vec![
        Example::new(
            "length from an area and a length",
            example_args(&[
                ("a", int_quantity(6, area_dimension())),
                ("b", int_quantity(2, length_dimension())),
            ]),
        )
        .with_value(int_quantity(3, length_dimension())),
    ])
}

fn power_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "units.power",
        "units",
        "1.0.0",
        "Power of a quantity",
        "Raise a quantity to an integer power, scaling its dimension exponents.",
    )
    .with_description(
        "The exponent must be an integer when the base is a quantity. A plain number accepts \
         any exponent supported by the arithmetic contract. Absolute affine temperatures are \
         rejected.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("a", "Base quantity or number.", ValueSchema::Any),
        ParamDescriptor::required("exponent", "Exponent.", ValueSchema::exact()),
    ])
    .with_output(
        ValueSchema::Any,
        "Power as a quantity, or a plain number for a plain-number base.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Constant)
    .with_method_ref("docs/methods/units.md#power")
    .with_examples(vec![
        Example::new(
            "volume from a length",
            example_args(&[
                ("a", int_quantity(2, length_dimension())),
                ("exponent", Value::integer(BigInt::from(3))),
            ]),
        )
        .with_value(int_quantity(8, volume_dimension())),
    ])
}

fn add_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "units.add",
        "units",
        "1.0.0",
        "Add quantities",
        "Add quantities of identical dimension.",
    )
    .with_description(
        "Operands must have identical dimensions; a plain number is dimensionless. Adding \
         absolute temperatures is rejected because the result would not be an absolute \
         temperature.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("a", "Left quantity or number.", ValueSchema::Any),
        ParamDescriptor::required("b", "Right quantity or number.", ValueSchema::Any),
    ])
    .with_output(
        ValueSchema::Any,
        "Sum as a quantity, or a plain number when both operands are plain numbers.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Constant)
    .with_method_ref("docs/methods/units.md#add")
    .with_examples(vec![
        Example::new(
            "sum two durations",
            example_args(&[
                ("a", int_quantity(2, time_dimension())),
                ("b", int_quantity(3, time_dimension())),
            ]),
        )
        .with_value(int_quantity(5, time_dimension())),
    ])
}

fn subtract_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "units.subtract",
        "units",
        "1.0.0",
        "Subtract quantities",
        "Subtract quantities of identical dimension.",
    )
    .with_description(
        "Operands must have identical dimensions. Subtracting absolute temperatures yields \
         a temperature difference.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("a", "Minuend quantity or number.", ValueSchema::Any),
        ParamDescriptor::required("b", "Subtrahend quantity or number.", ValueSchema::Any),
    ])
    .with_output(
        ValueSchema::Any,
        "Difference as a quantity, or a plain number when both operands are plain numbers.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Constant)
    .with_method_ref("docs/methods/units.md#subtract")
    .with_examples(vec![
        Example::new(
            "difference of two lengths",
            example_args(&[
                ("a", int_quantity(5, length_dimension())),
                ("b", int_quantity(2, length_dimension())),
            ]),
        )
        .with_value(int_quantity(3, length_dimension())),
    ])
}

fn temperature_difference_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "units.temperature_difference",
        "units",
        "1.0.0",
        "Temperature difference",
        "Convert a temperature difference between scales without applying affine offsets.",
    )
    .with_description(
        "Only the scale factors are used, so 5 degrees Celsius equals 9 delta degrees \
         Fahrenheit. The result is a temperature-delta quantity, not an absolute \
         temperature.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "value",
            "Quantity, or plain number in kelvin.",
            ValueSchema::Any,
        ),
        ParamDescriptor::required("from", "Source temperature unit id.", ValueSchema::text()),
        ParamDescriptor::required("to", "Target temperature unit id.", ValueSchema::text()),
    ])
    .with_output(
        ValueSchema::Quantity {
            dimension: Some(temperature_dimension()),
            allow_delta: true,
        },
        "Temperature difference in the target scale.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Constant)
    .with_method_ref("docs/methods/units.md#temperature_difference")
    .with_examples(vec![
        Example::new(
            "celsius difference in fahrenheit",
            example_args(&[
                ("value", int_quantity(5, temperature_dimension())),
                ("from", Value::text("degree_celsius")),
                ("to", Value::text("degree_fahrenheit")),
            ]),
        )
        .with_value(int_quantity(9, temperature_dimension())),
    ])
}

fn is_dimensionless_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "units.is_dimensionless",
        "units",
        "1.0.0",
        "Is dimensionless",
        "True when a value is a plain number or a dimensionless quantity.",
    )
    .with_description("Currency is not a physical dimension and is never accepted here.")
    .with_parameters(vec![ParamDescriptor::required(
        "value",
        "Quantity or number.",
        ValueSchema::Any,
    )])
    .with_output(
        ValueSchema::Bool,
        "True when all dimension exponents are zero.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Constant)
    .with_method_ref("docs/methods/units.md#is_dimensionless")
    .with_examples(vec![
        Example::new(
            "plain number",
            example_args(&[("value", Value::integer(BigInt::from(5)))]),
        )
        .with_value(Value::Bool(true)),
        Example::new(
            "length quantity",
            example_args(&[("value", int_quantity(5, length_dimension()))]),
        )
        .with_value(Value::Bool(false)),
    ])
}

/// Build the units module with all of its registered functions.
pub fn module() -> Module {
    let functions: Vec<Arc<dyn Function>> = vec![
        SimpleFunction::arc(list_units_descriptor(), invoke_list_units),
        SimpleFunction::arc(describe_unit_descriptor(), invoke_describe_unit),
        SimpleFunction::arc(convert_descriptor(), invoke_convert),
        SimpleFunction::arc(multiply_descriptor(), invoke_multiply),
        SimpleFunction::arc(divide_descriptor(), invoke_divide),
        SimpleFunction::arc(power_descriptor(), invoke_power),
        SimpleFunction::arc(add_descriptor(), invoke_add),
        SimpleFunction::arc(subtract_descriptor(), invoke_subtract),
        SimpleFunction::arc(
            temperature_difference_descriptor(),
            invoke_temperature_difference,
        ),
        SimpleFunction::arc(is_dimensionless_descriptor(), invoke_is_dimensionless),
        check::function(),
    ];
    let descriptor = ModuleDescriptor::new(
        "units",
        "Units",
        "1.0.0",
        "Versioned unit registry, dimensional algebra, and exact conversions.",
    )
    .with_capabilities(vec![
        "unit_registry",
        "exact_conversions",
        "affine_temperature",
        "dimensional_algebra",
    ])
    .with_dependencies(vec!["core"])
    .with_modes(all_modes())
    .with_source("crates/bicmath-units");
    Module::new(descriptor, functions)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> ExecContext {
        ExecContext::conservative()
    }

    fn args_json(pairs: &[(&str, Value)]) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        for (name, value) in pairs {
            map.insert(
                (*name).to_string(),
                serde_json::to_value(value).expect("value serializes"),
            );
        }
        serde_json::Value::Object(map)
    }

    fn call(id: &str, raw: serde_json::Value) -> Result<Outcome, EngineError> {
        let module = module();
        let function = module
            .functions
            .iter()
            .find(|function| function.descriptor().id == id)
            .expect("function exists");
        let args = raw.as_object().expect("object args");
        let mut values = BTreeMap::new();
        for (name, value) in args {
            let parameter = function
                .descriptor()
                .parameter(name)
                .expect("parameter exists");
            values.insert(
                name.clone(),
                parameter
                    .schema
                    .coerce(value, name, &ctx().limits, true)
                    .expect("argument coerces"),
            );
        }
        function.invoke(&Args::new(values), &ctx())
    }

    fn convert_quantity(value: Value, from: &str, to: &str) -> Result<Outcome, EngineError> {
        call(
            "units.convert",
            args_json(&[
                ("value", value),
                ("from", Value::text(from)),
                ("to", Value::text(to)),
            ]),
        )
    }

    fn quantity_of(outcome: &Outcome) -> (Number, Dimension) {
        match &outcome.value {
            Value::Quantity { value, dimension } => {
                (value.as_number().expect("number").clone(), *dimension)
            }
            other => panic!("expected a quantity, found {}", other.kind_name()),
        }
    }

    fn rational(numer: i64, denom: i64) -> BigRational {
        BigRational::new(BigInt::from(numer), BigInt::from(denom))
    }

    fn exact_rational(number: &Number) -> BigRational {
        number.to_exact_rational().expect("exact number")
    }

    #[test]
    fn zero_celsius_to_kelvin_is_exact() {
        let outcome = convert_quantity(
            int_quantity(0, temperature_dimension()),
            "degree_celsius",
            "kelvin",
        )
        .unwrap();
        assert_eq!(outcome.exactness, Exactness::Exact);
        let (number, dimension) = quantity_of(&outcome);
        assert_eq!(exact_rational(&number), rational(27315, 100));
        assert_eq!(dimension, temperature_dimension());
    }

    #[test]
    fn zero_celsius_to_fahrenheit_is_32() {
        let outcome = convert_quantity(
            int_quantity(0, temperature_dimension()),
            "degree_celsius",
            "degree_fahrenheit",
        )
        .unwrap();
        let (number, _) = quantity_of(&outcome);
        assert_eq!(exact_rational(&number), rational(32, 1));
    }

    #[test]
    fn minus_forty_celsius_equals_minus_forty_fahrenheit() {
        let celsius = convert_quantity(
            int_quantity(-40, temperature_dimension()),
            "degree_celsius",
            "kelvin",
        )
        .unwrap();
        let fahrenheit = convert_quantity(
            int_quantity(-40, temperature_dimension()),
            "degree_fahrenheit",
            "kelvin",
        )
        .unwrap();
        let (celsius, _) = quantity_of(&celsius);
        let (fahrenheit, _) = quantity_of(&fahrenheit);
        assert_eq!(exact_rational(&celsius), exact_rational(&fahrenheit));
    }

    #[test]
    fn mile_to_meter_is_exact() {
        let outcome =
            convert_quantity(int_quantity(1, length_dimension()), "mile", "meter").unwrap();
        let (number, dimension) = quantity_of(&outcome);
        assert_eq!(exact_rational(&number), rational(1609344, 1000));
        assert_eq!(dimension, length_dimension());
    }

    #[test]
    fn inch_to_centimeter() {
        let outcome =
            convert_quantity(int_quantity(1, length_dimension()), "inch", "centimeter").unwrap();
        let (number, _) = quantity_of(&outcome);
        assert_eq!(exact_rational(&number), rational(254, 100));
    }

    #[test]
    fn us_and_imperial_gallons_differ() {
        let us =
            convert_quantity(int_quantity(1, volume_dimension()), "us_gallon", "liter").unwrap();
        let imperial = convert_quantity(
            int_quantity(1, volume_dimension()),
            "imperial_gallon",
            "liter",
        )
        .unwrap();
        let (us, _) = quantity_of(&us);
        let (imperial, _) = quantity_of(&imperial);
        assert_eq!(exact_rational(&us), rational(3785411784, 1_000_000_000));
        assert_eq!(exact_rational(&imperial), rational(454609, 100_000));
        assert_ne!(exact_rational(&us), exact_rational(&imperial));
    }

    #[test]
    fn psi_to_pascal() {
        let outcome =
            convert_quantity(int_quantity(1, pressure_dimension()), "psi", "pascal").unwrap();
        let (number, _) = quantity_of(&outcome);
        assert_eq!(
            exact_rational(&number),
            rational(6894757293168, 1_000_000_000)
        );
    }

    #[test]
    fn kmh_to_ms_is_rational() {
        let outcome = convert_quantity(
            int_quantity(100, speed_dimension()),
            "kilometer_per_hour",
            "meter_per_second",
        )
        .unwrap();
        let (number, dimension) = quantity_of(&outcome);
        assert!(matches!(number, Number::Rational(_)));
        assert_eq!(exact_rational(&number), rational(250, 9));
        assert_eq!(dimension, speed_dimension());
    }

    #[test]
    fn degrees_to_radians() {
        let outcome =
            convert_quantity(int_quantity(180, Dimension::ANGLE), "degree", "radian").unwrap();
        assert_eq!(outcome.exactness, Exactness::Approximate);
        let (number, _) = quantity_of(&outcome);
        let value = number.to_f64().expect("float");
        assert!((value - std::f64::consts::PI).abs() < 1e-15);
    }

    #[test]
    fn turn_to_degrees() {
        let outcome =
            convert_quantity(int_quantity(1, Dimension::ANGLE), "turn", "degree").unwrap();
        let (number, _) = quantity_of(&outcome);
        let value = number.to_f64().expect("float");
        assert!((value - 360.0).abs() < 1e-12);
    }

    #[test]
    fn hour_plus_thirty_minutes_is_5400_seconds() {
        let hour = convert_quantity(int_quantity(1, time_dimension()), "hour", "second").unwrap();
        let minute =
            convert_quantity(int_quantity(30, time_dimension()), "minute", "second").unwrap();
        let sum = call(
            "units.add",
            args_json(&[("a", hour.value), ("b", minute.value)]),
        )
        .unwrap();
        let (number, dimension) = quantity_of(&sum);
        assert_eq!(exact_rational(&number), rational(5400, 1));
        assert_eq!(dimension, time_dimension());
    }

    #[test]
    fn multiply_two_lengths_is_an_area() {
        let outcome = call(
            "units.multiply",
            args_json(&[
                ("a", int_quantity(2, length_dimension())),
                ("b", int_quantity(3, length_dimension())),
            ]),
        )
        .unwrap();
        let (number, dimension) = quantity_of(&outcome);
        assert_eq!(exact_rational(&number), rational(6, 1));
        assert_eq!(dimension, area_dimension());
    }

    #[test]
    fn divide_area_by_length_is_a_length() {
        let outcome = call(
            "units.divide",
            args_json(&[
                ("a", int_quantity(6, area_dimension())),
                ("b", int_quantity(2, length_dimension())),
            ]),
        )
        .unwrap();
        let (number, dimension) = quantity_of(&outcome);
        assert_eq!(exact_rational(&number), rational(3, 1));
        assert_eq!(dimension, length_dimension());
    }

    #[test]
    fn add_meter_and_second_is_incompatible() {
        let error = call(
            "units.add",
            args_json(&[
                ("a", int_quantity(1, length_dimension())),
                ("b", int_quantity(1, time_dimension())),
            ]),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::IncompatibleUnits);
    }

    #[test]
    fn multiply_absolute_temperature_is_rejected() {
        let error = call(
            "units.multiply",
            args_json(&[
                ("a", int_quantity(20, temperature_dimension())),
                ("b", int_quantity(2, Dimension::DIMENSIONLESS)),
            ]),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::DomainViolation);
        assert!(error.message.contains("temperature differences"));
    }

    #[test]
    fn temperature_difference_celsius_to_fahrenheit() {
        let outcome = call(
            "units.temperature_difference",
            args_json(&[
                ("value", int_quantity(5, temperature_dimension())),
                ("from", Value::text("degree_celsius")),
                ("to", Value::text("degree_fahrenheit")),
            ]),
        )
        .unwrap();
        let (number, dimension) = quantity_of(&outcome);
        assert_eq!(exact_rational(&number), rational(9, 1));
        assert_eq!(dimension, temperature_dimension());
    }

    #[test]
    fn subtract_absolute_temperatures_yields_a_delta() {
        let outcome = call(
            "units.subtract",
            args_json(&[
                ("a", int_quantity(20, temperature_dimension())),
                ("b", int_quantity(5, temperature_dimension())),
            ]),
        )
        .unwrap();
        let (number, dimension) = quantity_of(&outcome);
        assert_eq!(exact_rational(&number), rational(15, 1));
        assert_eq!(dimension, temperature_dimension());
        assert!(!outcome.assumptions.is_empty());
    }

    #[test]
    fn add_absolute_temperatures_is_rejected() {
        let error = call(
            "units.add",
            args_json(&[
                ("a", int_quantity(20, temperature_dimension())),
                ("b", int_quantity(5, temperature_dimension())),
            ]),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::DomainViolation);
    }

    #[test]
    fn power_is_dimensional() {
        let outcome = call(
            "units.power",
            args_json(&[
                ("a", int_quantity(2, length_dimension())),
                ("exponent", Value::integer(BigInt::from(3))),
            ]),
        )
        .unwrap();
        let (number, dimension) = quantity_of(&outcome);
        assert_eq!(exact_rational(&number), rational(8, 1));
        assert_eq!(dimension, volume_dimension());
    }

    #[test]
    fn plain_number_is_si_base() {
        let outcome =
            convert_quantity(Value::integer(BigInt::from(1)), "meter", "centimeter").unwrap();
        let (number, dimension) = quantity_of(&outcome);
        assert_eq!(exact_rational(&number), rational(100, 1));
        assert_eq!(dimension, length_dimension());
    }

    #[test]
    fn ambiguous_unit_is_rejected_with_alternatives() {
        let error = call(
            "units.describe_unit",
            args_json(&[("unit", Value::text("gallon"))]),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::DomainViolation);
        assert!(error.message.contains("us_gallon"));
        assert!(error.message.contains("imperial_gallon"));
    }

    #[test]
    fn unknown_unit_is_rejected() {
        let error =
            convert_quantity(int_quantity(1, length_dimension()), "furlong", "meter").unwrap_err();
        assert_eq!(error.code, ErrorCode::DomainViolation);
    }

    #[test]
    fn convert_incompatible_dimensions_is_rejected() {
        let error =
            convert_quantity(int_quantity(1, length_dimension()), "meter", "second").unwrap_err();
        assert_eq!(error.code, ErrorCode::IncompatibleUnits);
    }

    #[test]
    fn money_is_not_a_dimension() {
        let money = Value::Money {
            amount: Box::new(Value::integer(BigInt::from(5))),
            currency: "USD".to_string(),
        };
        let error = call("units.is_dimensionless", args_json(&[("value", money)])).unwrap_err();
        assert_eq!(error.code, ErrorCode::DomainViolation);
    }

    #[test]
    fn list_units_filters_by_kind() {
        let outcome = call(
            "units.list_units",
            args_json(&[("quantity_kind", Value::text("length"))]),
        )
        .unwrap();
        let records = outcome.value.as_array().expect("array");
        assert_eq!(records.len(), 12);
        for record in records {
            let fields = record.as_record().expect("record");
            assert_eq!(fields["unit_kind"].as_text().expect("text"), "length");
        }
    }

    #[test]
    fn describe_unit_reports_metadata() {
        let outcome = call(
            "units.describe_unit",
            args_json(&[("unit", Value::text("degree_celsius"))]),
        )
        .unwrap();
        let fields = outcome.value.as_record().expect("record");
        assert_eq!(
            fields["unit_kind"].as_text().expect("text"),
            "temperature_absolute"
        );
        assert!(fields["affine"].as_bool().expect("bool"));
        assert!(fields["exact_factor"].as_bool().expect("bool"));
        assert!(fields.contains_key("notes"));

        let degree = call(
            "units.describe_unit",
            args_json(&[("unit", Value::text("degree"))]),
        )
        .unwrap();
        let fields = degree.value.as_record().expect("record");
        assert!(!fields["exact_factor"].as_bool().expect("bool"));
        assert_eq!(fields["unit_kind"].as_text().expect("text"), "angle");
    }

    #[test]
    fn is_dimensionless_checks_dimension() {
        let length = call(
            "units.is_dimensionless",
            args_json(&[("value", int_quantity(5, length_dimension()))]),
        )
        .unwrap();
        assert_eq!(length.value, Value::Bool(false));
        let plain = call(
            "units.is_dimensionless",
            args_json(&[("value", Value::integer(BigInt::from(5)))]),
        )
        .unwrap();
        assert_eq!(plain.value, Value::Bool(true));
    }

    #[test]
    fn examples_are_declared_for_every_function() {
        for function in module().functions {
            assert!(
                !function.descriptor().examples.is_empty(),
                "function {} has no examples",
                function.descriptor().id
            );
        }
    }

    #[test]
    fn module_identity_is_stable() {
        let module = module();
        assert_eq!(module.descriptor.id, "units");
        assert_eq!(module.descriptor.version, "1.0.0");
        assert_eq!(REGISTRY_VERSION, "1.0.0");
        for function in &module.functions {
            assert_eq!(function.descriptor().module, "units");
            assert_eq!(function.descriptor().version, "1.0.0");
            assert!(function.descriptor().id.starts_with("units."));
            assert!(
                function
                    .descriptor()
                    .method_ref
                    .starts_with("docs/methods/units.md#")
            );
        }
    }
}
