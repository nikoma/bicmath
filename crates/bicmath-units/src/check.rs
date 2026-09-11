//! Dimensional analysis of restricted expressions.
//!
//! `units.check` evaluates the *dimensions* of an expression whose identifiers
//! are bound to quantities, without evaluating numeric values. It catches
//! dimension errors before a calculation is run.

use std::collections::BTreeMap;

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, CostClass, Example, FunctionDescriptor, Outcome, ParamDescriptor, Warning,
};
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::expr::{BinaryOp, Expr, UnaryOp, parse_expression};
use bicmath_core::number::{NumericMode, RoundingMode};
use bicmath_core::schema::ValueSchema;
use bicmath_core::value::{Dimension, Value};

fn dimensionless() -> Dimension {
    Dimension::DIMENSIONLESS
}

fn number_of_exponent(expr: &Expr) -> Result<i32, EngineError> {
    match expr {
        Expr::Number(number) => {
            let rounded = number.round_to_scale(0, RoundingMode::TowardZero)?;
            match rounded {
                bicmath_core::number::Number::Integer(integer) => {
                    let value = num_traits::ToPrimitive::to_i32(&integer)
                        .ok_or_else(|| EngineError::domain("dimension exponent is out of range"))?;
                    Ok(value)
                }
                _ => Err(EngineError::domain("dimension exponents must be integers")),
            }
        }
        Expr::Unary {
            op: UnaryOp::Neg,
            expr,
        } => Ok(-number_of_exponent(expr)?),
        Expr::Unary {
            op: UnaryOp::Pos,
            expr,
        } => number_of_exponent(expr),
        Expr::Ident(name) => Err(EngineError::domain(format!(
            "dimension exponents must be integer literals, not binding {name:?}"
        ))),
        _ => Err(EngineError::domain(
            "dimension exponents must be integer literals",
        )),
    }
}

fn require_dimensionless(dimension: Dimension, function: &str) -> Result<(), EngineError> {
    if dimension.is_dimensionless() {
        Ok(())
    } else {
        Err(EngineError::new(
            ErrorCode::IncompatibleUnits,
            format!(
                "function {function} requires a dimensionless argument, found dimension {dimension}"
            ),
        ))
    }
}

fn dimension_of(
    expr: &Expr,
    bindings: &BTreeMap<String, Dimension>,
    depth: usize,
) -> Result<Dimension, EngineError> {
    if depth > 128 {
        return Err(EngineError::new(
            ErrorCode::ResourceLimit,
            "expression nesting depth exceeds the limit",
        ));
    }
    match expr {
        Expr::Number(_) => Ok(dimensionless()),
        Expr::Ident(name) => bindings.get(name).copied().ok_or_else(|| {
            EngineError::new(
                ErrorCode::NotFound,
                format!("unknown binding {name:?} in dimensional check"),
            )
        }),
        Expr::Unary { op, expr } => match op {
            UnaryOp::Neg | UnaryOp::Pos => dimension_of(expr, bindings, depth + 1),
            UnaryOp::Not => Err(EngineError::malformed(
                "logical negation is not valid in a dimensional expression",
            )),
        },
        Expr::Binary { op, left, right } => {
            let a = dimension_of(left, bindings, depth + 1)?;
            let b = dimension_of(right, bindings, depth + 1)?;
            match op {
                BinaryOp::Add | BinaryOp::Sub | BinaryOp::Rem => {
                    if a != b {
                        return Err(EngineError::new(
                            ErrorCode::IncompatibleUnits,
                            format!("cannot combine dimension {a} with dimension {b}"),
                        ));
                    }
                    Ok(a)
                }
                BinaryOp::Mul => a.multiply(&b),
                BinaryOp::Div => a.divide(&b),
                BinaryOp::Pow => {
                    let exponent = number_of_exponent(right)?;
                    a.pow(exponent)
                }
                BinaryOp::And | BinaryOp::Or => Err(EngineError::malformed(
                    "logical operators are not valid in a dimensional expression",
                )),
            }
        }
        Expr::Compare { operands, .. } => {
            let first = dimension_of(&operands[0], bindings, depth + 1)?;
            for operand in &operands[1..] {
                let other = dimension_of(operand, bindings, depth + 1)?;
                if first != other {
                    return Err(EngineError::new(
                        ErrorCode::IncompatibleUnits,
                        format!("cannot compare dimension {first} with dimension {other}"),
                    ));
                }
            }
            Ok(dimensionless())
        }
        Expr::Call { name, args } => {
            let mut dims = Vec::with_capacity(args.len());
            for arg in args {
                if arg.name.is_some() {
                    return Err(EngineError::malformed(
                        "named arguments are not supported in dimensional checks",
                    ));
                }
                dims.push(dimension_of(&arg.value, bindings, depth + 1)?);
            }
            match name.as_str() {
                "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "sinh" | "cosh" | "tanh"
                | "asinh" | "acosh" | "atanh" | "exp" | "expm1" | "ln" | "log" | "log10"
                | "log2" | "log1p" => {
                    for dimension in &dims {
                        require_dimensionless(*dimension, name)?;
                    }
                    Ok(dimensionless())
                }
                "atan2" | "min" | "max" => {
                    if dims.len() < 2 {
                        return Err(EngineError::malformed(format!(
                            "{name} requires at least two arguments"
                        )));
                    }
                    let first = dims[0];
                    for dimension in &dims[1..] {
                        if *dimension != first {
                            return Err(EngineError::new(
                                ErrorCode::IncompatibleUnits,
                                format!("{name} arguments have incompatible dimensions"),
                            ));
                        }
                    }
                    Ok(first)
                }
                "abs" | "floor" | "ceil" | "round" | "trunc" => {
                    if dims.len() != 1 {
                        return Err(EngineError::malformed(format!(
                            "{name} requires one argument"
                        )));
                    }
                    Ok(dims[0])
                }
                "sqrt" => {
                    if dims.len() != 1 {
                        return Err(EngineError::malformed("sqrt requires one argument"));
                    }
                    let dimension = dims[0];
                    let mut exponents = dimension.exponents();
                    for exponent in &mut exponents {
                        if *exponent % 2 != 0 {
                            return Err(EngineError::new(
                                ErrorCode::IncompatibleUnits,
                                format!(
                                    "sqrt of dimension {dimension} has odd exponents and is not \
                                     representable as an integer dimension"
                                ),
                            ));
                        }
                        *exponent /= 2;
                    }
                    Ok(Dimension::new(exponents))
                }
                "pow" => {
                    if dims.len() != 2 {
                        return Err(EngineError::malformed("pow requires two arguments"));
                    }
                    require_dimensionless(dims[1], "pow exponent")?;
                    Err(EngineError::domain(
                        "pow with a non-literal exponent is not supported in dimensional checks; \
                         use the ^ operator with an integer exponent",
                    ))
                }
                other => Err(EngineError::new(
                    ErrorCode::UnknownFunction,
                    format!("unknown function {other:?} in dimensional check"),
                )),
            }
        }
        Expr::Bool(_) | Expr::Text(_) | Expr::Array(_) | Expr::Record(_) => Err(
            EngineError::malformed("expression must be a numeric or quantity expression"),
        ),
    }
}

fn descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "units.check",
        "units",
        "1.0.0",
        "Check dimensions",
        "Evaluate the physical dimensions of a restricted expression without computing values.",
    )
    .with_description(
        "Bindings map identifiers to quantities. Literals are dimensionless. Addition, \
         subtraction, remainder, and comparison require equal dimensions; multiplication, \
         division, and integer powers combine dimensions; transcendental functions require \
         dimensionless arguments; sqrt halves even dimension exponents. Returns the resulting \
         dimension, a symbol, and whether it is dimensionless. This catches unit errors before \
         a calculation runs.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "expression",
            "Restricted expression in the bound identifiers, e.g. \"distance / time\".",
            ValueSchema::text(),
        ),
        ParamDescriptor::required(
            "bindings",
            "Record mapping identifiers to quantities or dimensionless numbers.",
            ValueSchema::Record {
                fields: Vec::new(),
                allow_extra: true,
            },
        ),
    ])
    .with_output(
        ValueSchema::Any,
        "Record with dimension, symbol, and dimensionless flag.",
    )
    .with_modes(vec![
        NumericMode::Exact,
        NumericMode::Auto,
        NumericMode::Scientific,
    ])
    .with_cost(CostClass::Linear)
    .with_method_ref("docs/methods/units.md#check")
    .with_examples(vec![
        Example::new("speed has length over time", {
            let mut args = BTreeMap::new();
            args.insert("expression".to_string(), Value::text("distance / time"));
            args.insert(
                "bindings".to_string(),
                Value::record([
                    (
                        "distance",
                        Value::Quantity {
                            value: Box::new(Value::Number(bicmath_core::number::Number::integer(
                                100,
                            ))),
                            dimension: Dimension::new([1, 0, 0, 0, 0, 0, 0, 0]),
                        },
                    ),
                    (
                        "time",
                        Value::Quantity {
                            value: Box::new(Value::Number(bicmath_core::number::Number::integer(
                                10,
                            ))),
                            dimension: Dimension::new([0, 0, 1, 0, 0, 0, 0, 0]),
                        },
                    ),
                ]),
            );
            args
        })
        .with_contains("m*s^-1"),
    ])
}

fn invoke_check(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let source = args.text("expression")?;
    let expression = parse_expression(source, &ctx.limits)?;
    let raw_bindings = args.record("bindings")?;
    let mut bindings: BTreeMap<String, Dimension> = BTreeMap::new();
    for (name, value) in raw_bindings {
        let dimension = match value {
            Value::Quantity { dimension, .. } => *dimension,
            Value::Number(_) => dimensionless(),
            other => {
                return Err(EngineError::malformed(format!(
                    "binding {name:?} must be a quantity or number, found {}",
                    other.kind_name()
                ))
                .with_path(name.clone()));
            }
        };
        bindings.insert(name.clone(), dimension);
    }
    let dimension = dimension_of(&expression, &bindings, 0)?;
    let dimension_value = Value::from_json(&dimension.to_json(), &ctx.limits)?;
    let mut outcome = Outcome::exact(Value::record([
        ("dimension", dimension_value),
        ("symbol", Value::text(dimension.symbol())),
        ("dimensionless", Value::Bool(dimension.is_dimensionless())),
        ("method", Value::text("dimensional_analysis")),
    ]));
    if dimension.is_angle_only() {
        outcome = outcome.with_warning(Warning::new(
            "angle_dimension",
            "the result has angle dimension; trig functions accept angle quantities and convert \
             them to radians",
        ));
    }
    Ok(outcome)
}

/// Build the `units.check` function for registration.
pub fn function() -> std::sync::Arc<dyn bicmath_core::contract::Function> {
    bicmath_core::contract::SimpleFunction::arc(descriptor(), invoke_check)
}
