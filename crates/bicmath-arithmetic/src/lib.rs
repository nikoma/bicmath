//! Arithmetic module: exact arithmetic, rounding, comparison, and integer and
//! rational operations.
//!
//! Every function delegates to the shared numeric contract in `bicmath-core`;
//! there are no parallel arithmetic implementations.

use std::collections::BTreeMap;
use std::sync::Arc;

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, CostClass, ErrorEstimate, Example, FunctionDescriptor, Module, ModuleDescriptor, Outcome,
    ParamDescriptor, SimpleFunction, Warning,
};
use bicmath_core::envelope::Exactness;
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::number::{Decimal, Number, NumberResult, NumericMode, RoundingMode};
use bicmath_core::schema::{NumberKind, ValueSchema};
use bicmath_core::value::Value;

fn number_schema() -> ValueSchema {
    ValueSchema::number(NumberKind::Any)
}

fn integer_schema() -> ValueSchema {
    ValueSchema::number(NumberKind::Integer)
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

fn outcome(result: NumberResult) -> Result<Outcome, EngineError> {
    let exactness = classify(&result);
    Ok(Outcome::new(Value::Number(result.value), exactness))
}

fn all_modes() -> Vec<NumericMode> {
    vec![
        NumericMode::Exact,
        NumericMode::Auto,
        NumericMode::Scientific,
    ]
}

fn parse_mode(text: &str) -> Result<RoundingMode, EngineError> {
    match text {
        "half_even" => Ok(RoundingMode::HalfEven),
        "half_away_from_zero" => Ok(RoundingMode::HalfAwayFromZero),
        "toward_zero" => Ok(RoundingMode::TowardZero),
        "floor" => Ok(RoundingMode::Floor),
        "ceiling" => Ok(RoundingMode::Ceiling),
        other => Err(EngineError::domain(format!(
            "unknown rounding mode {other:?}; expected half_even, half_away_from_zero, \
             toward_zero, floor, or ceiling"
        ))),
    }
}

fn number_at(args: &Args, name: &str) -> Result<Number, EngineError> {
    Ok(args.number(name)?.clone())
}

fn pow10(n: u64) -> num_bigint::BigInt {
    use num_traits::One;
    let mut value = num_bigint::BigInt::one();
    let mut base = num_bigint::BigInt::from(10u32);
    let mut exp = n;
    while exp > 0 {
        if exp & 1 == 1 {
            value *= &base;
        }
        base = &base * &base;
        exp >>= 1;
    }
    value
}

// ---------------------------------------------------------------------------
// Binary arithmetic
// ---------------------------------------------------------------------------

fn add_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "arithmetic.add",
        "arithmetic",
        "1.0.0",
        "Add",
        "Add two numbers under the shared promotion rules.",
    )
    .with_description(
        "Exact for integer, rational, and decimal operands. Float64 operands require \
         scientific mode and produce an approximate result.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("a", "Left operand.", number_schema()),
        ParamDescriptor::required("b", "Right operand.", number_schema()),
    ])
    .with_output(number_schema(), "Sum of a and b.")
    .with_modes(all_modes())
    .with_cost(CostClass::Constant)
    .with_method_ref("docs/methods/arithmetic.md#add")
    .with_examples(vec![
        Example::new(
            "exact decimal addition",
            example_args(&[
                ("a", serde_json::json!("0.1")),
                ("b", serde_json::json!("0.2")),
            ]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "decimal", "value": "0.3"}),
        )),
    ])
}

fn invoke_add(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let a = number_at(args, "a")?;
    let b = number_at(args, "b")?;
    outcome(a.add(&b, &ctx.numeric, &ctx.limits)?)
}

fn sub_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "arithmetic.sub",
        "arithmetic",
        "1.0.0",
        "Subtract",
        "Subtract one number from another.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("a", "Minuend.", number_schema()),
        ParamDescriptor::required("b", "Subtrahend.", number_schema()),
    ])
    .with_output(number_schema(), "Difference a - b.")
    .with_modes(all_modes())
    .with_method_ref("docs/methods/arithmetic.md#sub")
    .with_examples(vec![
        Example::new(
            "exact integer subtraction",
            example_args(&[("a", serde_json::json!(5)), ("b", serde_json::json!(8))]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "integer", "value": "-3"}),
        )),
    ])
}

fn invoke_sub(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let a = number_at(args, "a")?;
    let b = number_at(args, "b")?;
    outcome(a.sub(&b, &ctx.numeric, &ctx.limits)?)
}

fn mul_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "arithmetic.mul",
        "arithmetic",
        "1.0.0",
        "Multiply",
        "Multiply two numbers.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("a", "Left factor.", number_schema()),
        ParamDescriptor::required("b", "Right factor.", number_schema()),
    ])
    .with_output(number_schema(), "Product a * b.")
    .with_modes(all_modes())
    .with_method_ref("docs/methods/arithmetic.md#mul")
    .with_examples(vec![
        Example::new(
            "exact decimal multiplication",
            example_args(&[
                ("a", serde_json::json!("0.10")),
                ("b", serde_json::json!("3")),
            ]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "decimal", "value": "0.30"}),
        )),
    ])
}

fn invoke_mul(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let a = number_at(args, "a")?;
    let b = number_at(args, "b")?;
    outcome(a.mul(&b, &ctx.numeric, &ctx.limits)?)
}

fn div_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "arithmetic.div",
        "arithmetic",
        "1.0.0",
        "Divide",
        "Divide one number by another.",
    )
    .with_description(
        "Integer / integer stays exact: an exact quotient is returned as an integer and a \
         non-terminating quotient as a normalized rational. Decimal division rounds to the \
         context precision and reports inexactness; in exact mode it is rejected.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("a", "Dividend.", number_schema()),
        ParamDescriptor::required("b", "Divisor; must not be zero.", number_schema()),
    ])
    .with_output(number_schema(), "Quotient a / b.")
    .with_modes(all_modes())
    .with_method_ref("docs/methods/arithmetic.md#div")
    .with_examples(vec![
        Example::new(
            "rational division",
            example_args(&[("a", serde_json::json!(1)), ("b", serde_json::json!(3))]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "rational", "numerator": "1", "denominator": "3"}),
        )),
        Example::new(
            "division by zero is an error",
            example_args(&[("a", serde_json::json!(1)), ("b", serde_json::json!(0))]),
        )
        .with_error(ErrorCode::DivisionByZero),
    ])
}

fn invoke_div(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let a = number_at(args, "a")?;
    let b = number_at(args, "b")?;
    outcome(a.div(&b, &ctx.numeric, &ctx.limits)?)
}

// ---------------------------------------------------------------------------
// Reductions
// ---------------------------------------------------------------------------

fn sum_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "arithmetic.sum",
        "arithmetic",
        "1.0.0",
        "Sum",
        "Sum an array of numbers.",
    )
    .with_description(
        "Exact inputs are summed exactly. If any element is float64 (scientific mode), \
         Neumaier compensated summation is used and the result is approximate. The sum of \
         an empty array is exact zero.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "values",
        "Numbers to sum.",
        ValueSchema::array(number_schema()),
    )])
    .with_output(number_schema(), "Sum of the values.")
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_method_ref("docs/methods/arithmetic.md#sum")
    .with_examples(vec![
        Example::new(
            "exact sum",
            example_args(&[("values", serde_json::json!([1, 2, 3, 4]))]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "integer", "value": "10"}),
        )),
    ])
}

fn collect_numbers(args: &Args, name: &str) -> Result<Vec<Number>, EngineError> {
    let items = args.array(name)?;
    let mut out = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        match item {
            Value::Number(number) => out.push(number.clone()),
            other => {
                return Err(EngineError::malformed(format!(
                    "expected a number at {name}[{index}], found {}",
                    other.kind_name()
                ))
                .with_path(format!("{name}[{index}]")));
            }
        }
    }
    Ok(out)
}

fn invoke_sum(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let values = collect_numbers(args, "values")?;
    if values.is_empty() {
        return Ok(Outcome::exact(Value::integer(num_bigint::BigInt::from(0))));
    }
    if values.iter().any(|v| v.is_float()) {
        if ctx.numeric.mode != NumericMode::Scientific {
            return Err(EngineError::new(
                ErrorCode::UnsupportedNumericMode,
                "float64 summation requires scientific mode",
            ));
        }
        let floats: Vec<f64> = values
            .iter()
            .map(|v| v.to_f64().unwrap_or(f64::NAN))
            .collect();
        let (sum, compensation) = neumaier_sum(&floats);
        let result = Number::float(sum)?;
        let estimate = ErrorEstimate::new(
            "compensated_summation",
            Value::decimal(
                Decimal::from_f64_display(compensation.abs()).unwrap_or_else(Decimal::zero),
            ),
            "Neumaier compensated summation; the compensation term is an error indicator, \
             not a rigorous bound",
        );
        let mut outcome = Outcome::approximate(Value::Number(result)).with_error_estimate(estimate);
        if compensation != 0.0 {
            outcome = outcome.with_warning(Warning::new(
                "float_reduction_compensated",
                "float64 sum used Neumaier compensated summation",
            ));
        }
        return Ok(outcome);
    }
    let mut total = Number::integer(num_bigint::BigInt::from(0));
    let mut rounded = false;
    for value in &values {
        ctx.check()?;
        let result = total.add(value, &ctx.numeric, &ctx.limits)?;
        total = result.value;
        rounded |= result.rounded;
    }
    outcome(NumberResult {
        value: total,
        rounded,
    })
}

fn neumaier_sum(values: &[f64]) -> (f64, f64) {
    let mut sum = 0.0f64;
    let mut compensation = 0.0f64;
    for &value in values {
        let next = sum + value;
        if sum.abs() >= value.abs() {
            compensation += (sum - next) + value;
        } else {
            compensation += (value - next) + sum;
        }
        sum = next;
    }
    (sum + compensation, compensation)
}

fn product_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "arithmetic.product",
        "arithmetic",
        "1.0.0",
        "Product",
        "Multiply an array of numbers.",
    )
    .with_description(
        "Exact for integer/rational/decimal inputs. The product of an empty array is \
         exact one.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "values",
        "Numbers to multiply.",
        ValueSchema::array(number_schema()),
    )])
    .with_output(number_schema(), "Product of the values.")
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_method_ref("docs/methods/arithmetic.md#product")
    .with_examples(vec![
        Example::new(
            "exact product",
            example_args(&[("values", serde_json::json!([2, 3, 4]))]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "integer", "value": "24"}),
        )),
    ])
}

fn invoke_product(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let values = collect_numbers(args, "values")?;
    if values.is_empty() {
        return Ok(Outcome::exact(Value::integer(num_bigint::BigInt::from(1))));
    }
    let mut total = Number::integer(num_bigint::BigInt::from(1));
    let mut rounded = false;
    for value in &values {
        ctx.check()?;
        let result = total.mul(value, &ctx.numeric, &ctx.limits)?;
        total = result.value;
        rounded |= result.rounded;
    }
    outcome(NumberResult {
        value: total,
        rounded,
    })
}

// ---------------------------------------------------------------------------
// Min / max / abs / compare
// ---------------------------------------------------------------------------

fn minmax_descriptor(id: &str, title: &str, summary: &str, less: bool) -> FunctionDescriptor {
    FunctionDescriptor::new(id, "arithmetic", "1.0.0", title, summary)
        .with_parameters(vec![ParamDescriptor::required(
            "values",
            "Non-empty array of numbers.",
            ValueSchema::array_with_len(number_schema(), 1, None),
        )])
        .with_output(number_schema(), "Selected element.")
        .with_modes(all_modes())
        .with_cost(CostClass::Linear)
        .with_method_ref("docs/methods/arithmetic.md#min-max")
        .with_examples(vec![
            Example::new(
                "select element",
                example_args(&[("values", serde_json::json!([3, 1, 2]))]),
            )
            .with_value(parse_value(serde_json::json!({
                "kind": "integer",
                "value": if less { "1" } else { "3" }
            }))),
        ])
}

fn invoke_min(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    select_minmax(args, ctx, true)
}

fn invoke_max(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    select_minmax(args, ctx, false)
}

fn select_minmax(args: &Args, ctx: &ExecContext, less: bool) -> Result<Outcome, EngineError> {
    let values = collect_numbers(args, "values")?;
    if values.is_empty() {
        return Err(EngineError::new(
            ErrorCode::InsufficientObservations,
            "min/max require at least one value",
        ));
    }
    let mut best = values[0].clone();
    for value in &values[1..] {
        ctx.check()?;
        let ordering = value.compare(&best)?;
        if (less && ordering == std::cmp::Ordering::Less)
            || (!less && ordering == std::cmp::Ordering::Greater)
        {
            best = value.clone();
        }
    }
    Ok(Outcome::exact(Value::Number(best)))
}

fn abs_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "arithmetic.abs",
        "arithmetic",
        "1.0.0",
        "Absolute value",
        "Absolute value of a number.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "value",
        "Input number.",
        number_schema(),
    )])
    .with_output(number_schema(), "Absolute value.")
    .with_modes(all_modes())
    .with_method_ref("docs/methods/arithmetic.md#abs")
    .with_examples(vec![
        Example::new(
            "absolute value",
            example_args(&[("value", serde_json::json!(-7))]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "integer", "value": "7"}),
        )),
    ])
}

fn invoke_abs(args: &Args, _ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let value = number_at(args, "value")?;
    let exactness = if value.is_float() {
        Exactness::Approximate
    } else {
        Exactness::Exact
    };
    Ok(Outcome::new(Value::Number(value.abs()), exactness))
}

fn compare_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "arithmetic.compare",
        "arithmetic",
        "1.0.0",
        "Compare",
        "Compare two numbers exactly.",
    )
    .with_description(
        "Comparisons are exact across representations: float64 is decomposed to its exact \
         binary rational value, so 0.1 (decimal) is not equal to 0.1 (float64).",
    )
    .with_parameters(vec![
        ParamDescriptor::required("a", "Left operand.", number_schema()),
        ParamDescriptor::required("b", "Right operand.", number_schema()),
    ])
    .with_output(
        ValueSchema::Record {
            fields: vec![
                bicmath_core::schema::FieldSchema::required(
                    "ordering",
                    ValueSchema::Enum {
                        variants: vec![
                            "less".to_string(),
                            "equal".to_string(),
                            "greater".to_string(),
                        ],
                    },
                ),
                bicmath_core::schema::FieldSchema::required("sign", integer_schema()),
            ],
            allow_extra: false,
        },
        "Ordering label and integer sign.",
    )
    .with_modes(all_modes())
    .with_method_ref("docs/methods/arithmetic.md#compare")
    .with_examples(vec![
        Example::new(
            "compare integers",
            example_args(&[("a", serde_json::json!(1)), ("b", serde_json::json!(2))]),
        )
        .with_value(parse_value(serde_json::json!({
            "ordering": "less",
            "sign": {"kind": "integer", "value": "-1"}
        }))),
    ])
}

fn invoke_compare(args: &Args, _ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let a = number_at(args, "a")?;
    let b = number_at(args, "b")?;
    let ordering = a.compare(&b)?;
    let (label, sign) = match ordering {
        std::cmp::Ordering::Less => ("less", -1i64),
        std::cmp::Ordering::Equal => ("equal", 0),
        std::cmp::Ordering::Greater => ("greater", 1),
    };
    let value = Value::record([
        ("ordering", Value::text(label)),
        (
            "sign",
            Value::Number(Number::Integer(num_bigint::BigInt::from(sign))),
        ),
    ]);
    Ok(Outcome::exact(value))
}

// ---------------------------------------------------------------------------
// Powers and roots
// ---------------------------------------------------------------------------

fn pow_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "arithmetic.pow",
        "arithmetic",
        "1.0.0",
        "Power",
        "Raise a number to a power.",
    )
    .with_description(
        "Integer exponents are exact for exact bases (0^0 is defined as 1, consistently \
         with the empty-product convention). Non-integer exponents require scientific mode \
         and use binary64 powf.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("base", "Base.", number_schema()),
        ParamDescriptor::required("exponent", "Exponent.", number_schema()),
    ])
    .with_output(number_schema(), "base^exponent.")
    .with_modes(all_modes())
    .with_cost(CostClass::Iterative)
    .with_method_ref("docs/methods/arithmetic.md#pow")
    .with_examples(vec![
        Example::new(
            "integer power",
            example_args(&[
                ("base", serde_json::json!(2)),
                ("exponent", serde_json::json!(10)),
            ]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "integer", "value": "1024"}),
        )),
        Example::new(
            "zero to the zero",
            example_args(&[
                ("base", serde_json::json!(0)),
                ("exponent", serde_json::json!(0)),
            ]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "integer", "value": "1"}),
        )),
    ])
}

fn invoke_pow(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let base = number_at(args, "base")?;
    let exponent = number_at(args, "exponent")?;
    outcome(base.pow(&exponent, &ctx.numeric, &ctx.limits)?)
}

fn sqrt_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "arithmetic.sqrt",
        "arithmetic",
        "1.0.0",
        "Square root",
        "Square root with explicit exactness handling.",
    )
    .with_description(
        "Returns an exact integer, rational, or decimal when the input has an exact square \
         root in that representation. Otherwise: exact mode returns a domain error; auto \
         mode returns a decimal approximation to the context precision and marks it \
         approximate; scientific mode returns float64. Negative inputs are always a domain \
         error in real mode.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "value",
        "Non-negative input.",
        number_schema(),
    )])
    .with_output(number_schema(), "Square root of value.")
    .with_modes(all_modes())
    .with_cost(CostClass::Iterative)
    .with_method_ref("docs/methods/arithmetic.md#sqrt")
    .with_examples(vec![
        Example::new(
            "exact square root",
            example_args(&[("value", serde_json::json!("0.25"))]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "decimal", "value": "0.5"}),
        )),
        Example::new(
            "negative input",
            example_args(&[("value", serde_json::json!(-1))]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_sqrt(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let value = number_at(args, "value")?;
    if value.is_negative() {
        return Err(EngineError::domain("square root of a negative number"));
    }
    match &value {
        Number::Integer(integer) => {
            let root = integer.sqrt();
            if &root * &root == *integer {
                return Ok(Outcome::exact(Value::Number(Number::Integer(root))));
            }
            approximate_sqrt(&value, ctx)
        }
        Number::Rational(rational) => {
            let numer = rational.numer().sqrt();
            let denom = rational.denom().sqrt();
            if &numer * &numer == *rational.numer() && &denom * &denom == *rational.denom() {
                return Ok(Outcome::exact(Value::Number(Number::Rational(
                    num_rational::BigRational::new(numer, denom),
                ))));
            }
            approximate_sqrt(&value, ctx)
        }
        Number::Decimal(decimal) => {
            if let Some(exact) = decimal.sqrt_exact() {
                return Ok(Outcome::exact(Value::Number(Number::Decimal(exact))));
            }
            approximate_sqrt(&value, ctx)
        }
        Number::Float64(float) => {
            if ctx.numeric.mode != NumericMode::Scientific {
                return Err(EngineError::new(
                    ErrorCode::UnsupportedNumericMode,
                    "sqrt of float64 requires scientific mode",
                ));
            }
            Ok(Outcome::approximate(Value::Number(Number::float(
                float.get().sqrt(),
            )?)))
        }
    }
}

fn approximate_sqrt(value: &Number, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    match ctx.numeric.mode {
        NumericMode::Exact => Err(EngineError::new(
            ErrorCode::UnsupportedNumericMode,
            "input has no exact square root in the selected representation; \
             use auto or scientific mode",
        )),
        NumericMode::Auto => {
            let decimal = to_decimal_value(value, ctx)?;
            let root = decimal_sqrt(&decimal, ctx)?;
            Ok(Outcome::approximate(Value::Number(Number::Decimal(root))))
        }
        NumericMode::Scientific => {
            let input = value
                .to_f64()
                .ok_or_else(|| EngineError::domain("value is not representable as float64"))?;
            Ok(Outcome::approximate(Value::Number(Number::float(
                input.sqrt(),
            )?)))
        }
    }
}

fn decimal_sqrt(value: &Decimal, ctx: &ExecContext) -> Result<Decimal, EngineError> {
    let target_scale = ctx.numeric.precision.max(1) as i64;
    let scale = value.scale() as i64;
    let exponent = 2 * target_scale - scale;
    let mantissa = value.mantissa().clone();
    let root = if exponent >= 0 {
        let scaled = mantissa * pow10(exponent as u64);
        scaled.sqrt()
    } else {
        let extra_scale = (scale + 1) / 2;
        let scaled = mantissa * pow10((2 * extra_scale - scale) as u64);
        let root = scaled.sqrt();
        // Root is at scale `extra_scale`; round down to the target scale.
        return Ok(Decimal::from_parts(root, extra_scale as u32)
            .round_to_scale(target_scale, ctx.numeric.rounding));
    };
    let raw = Decimal::from_parts(root, target_scale as u32);
    Ok(raw.round_to_scale(target_scale, ctx.numeric.rounding))
}

fn to_decimal_value(value: &Number, ctx: &ExecContext) -> Result<Decimal, EngineError> {
    match value {
        Number::Decimal(decimal) => Ok(decimal.clone()),
        Number::Integer(integer) => Ok(Decimal::from_bigint(integer.clone())),
        Number::Rational(rational) => {
            let (decimal, _inexact) = Decimal::from_rational(rational, &ctx.numeric, &ctx.limits)?;
            Ok(decimal)
        }
        Number::Float64(_) => Err(EngineError::new(
            ErrorCode::UnsupportedNumericMode,
            "cannot silently convert float64 to an exact decimal",
        )),
    }
}

// ---------------------------------------------------------------------------
// Rounding family
// ---------------------------------------------------------------------------

fn rounding_descriptor(id: &str, title: &str, summary: &str) -> FunctionDescriptor {
    FunctionDescriptor::new(id, "arithmetic", "1.0.0", title, summary)
        .with_parameters(vec![ParamDescriptor::required(
            "value",
            "Input number.",
            number_schema(),
        )])
        .with_output(number_schema(), "Rounded value.")
        .with_modes(all_modes())
        .with_method_ref("docs/methods/arithmetic.md#rounding")
        .with_examples(vec![Example::new(
            "round a decimal",
            example_args(&[("value", serde_json::json!("1.5"))]),
        )
        .with_value(parse_value(serde_json::json!({
            "kind": "decimal",
            "value": if id.ends_with("floor") { "1" } else if id.ends_with("ceil") { "2" } else { "1" }
        })))])
}

fn invoke_floor(args: &Args, _ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let value = number_at(args, "value")?;
    let exactness = if value.is_float() {
        Exactness::Approximate
    } else {
        Exactness::Exact
    };
    Ok(Outcome::new(
        Value::Number(value.round_to_scale(0, RoundingMode::Floor)?),
        exactness,
    ))
}

fn invoke_ceil(args: &Args, _ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let value = number_at(args, "value")?;
    let exactness = if value.is_float() {
        Exactness::Approximate
    } else {
        Exactness::Exact
    };
    Ok(Outcome::new(
        Value::Number(value.round_to_scale(0, RoundingMode::Ceiling)?),
        exactness,
    ))
}

fn invoke_trunc(args: &Args, _ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let value = number_at(args, "value")?;
    let exactness = if value.is_float() {
        Exactness::Approximate
    } else {
        Exactness::Exact
    };
    Ok(Outcome::new(
        Value::Number(value.round_to_scale(0, RoundingMode::TowardZero)?),
        exactness,
    ))
}

fn quantize_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "arithmetic.quantize",
        "arithmetic",
        "1.0.0",
        "Quantize",
        "Round a number to a fixed decimal scale with an explicit rounding mode.",
    )
    .with_description(
        "Display scale and settlement scale are caller-controlled; this function rounds \
         exactly at the requested scale. The default mode is half_even.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("value", "Input number.", number_schema()),
        ParamDescriptor::required(
            "scale",
            "Number of fractional decimal digits; may be negative.",
            integer_schema(),
        ),
        ParamDescriptor::optional(
            "mode",
            "Rounding mode: half_even, half_away_from_zero, toward_zero, floor, ceiling.",
            ValueSchema::Enum {
                variants: vec![
                    "half_even".into(),
                    "half_away_from_zero".into(),
                    "toward_zero".into(),
                    "floor".into(),
                    "ceiling".into(),
                ],
            },
        ),
    ])
    .with_output(number_schema(), "Value rounded to the requested scale.")
    .with_modes(all_modes())
    .with_method_ref("docs/methods/arithmetic.md#quantize")
    .with_examples(vec![
        Example::new(
            "half-even to cents",
            example_args(&[
                ("value", serde_json::json!("1.005")),
                ("scale", serde_json::json!(2)),
            ]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "decimal", "value": "1.00"}),
        )),
        Example::new(
            "half-away-from-zero to cents",
            example_args(&[
                ("value", serde_json::json!("1.005")),
                ("scale", serde_json::json!(2)),
                ("mode", serde_json::json!("half_away_from_zero")),
            ]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "decimal", "value": "1.01"}),
        )),
    ])
}

fn invoke_quantize(args: &Args, _ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let value = number_at(args, "value")?;
    let scale = args.integer("scale")?;
    let scale = num_traits::ToPrimitive::to_i64(&scale)
        .ok_or_else(|| EngineError::domain("scale is out of the supported range"))?;
    let mode = match args.optional_text("mode")? {
        Some(text) => parse_mode(text)?,
        None => RoundingMode::HalfEven,
    };
    let exactness = if value.is_float() {
        Exactness::Approximate
    } else {
        Exactness::Rounded
    };
    Ok(Outcome::new(
        Value::Number(value.round_to_scale(scale, mode)?),
        exactness,
    ))
}

// ---------------------------------------------------------------------------
// Integer functions
// ---------------------------------------------------------------------------

fn gcd_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "arithmetic.gcd",
        "arithmetic",
        "1.0.0",
        "Greatest common divisor",
        "Non-negative greatest common divisor of two integers.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("a", "First integer.", integer_schema()),
        ParamDescriptor::required("b", "Second integer.", integer_schema()),
    ])
    .with_output(integer_schema(), "gcd(a, b), always non-negative.")
    .with_modes(all_modes())
    .with_cost(CostClass::Iterative)
    .with_method_ref("docs/methods/arithmetic.md#gcd")
    .with_examples(vec![
        Example::new(
            "gcd",
            example_args(&[("a", serde_json::json!(12)), ("b", serde_json::json!(18))]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "integer", "value": "6"}),
        )),
    ])
}

fn invoke_gcd(args: &Args, _ctx: &ExecContext) -> Result<Outcome, EngineError> {
    use num_integer::Integer;
    let a = args.integer("a")?;
    let b = args.integer("b")?;
    Ok(Outcome::exact(Value::Number(Number::Integer(a.gcd(&b)))))
}

fn lcm_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "arithmetic.lcm",
        "arithmetic",
        "1.0.0",
        "Least common multiple",
        "Non-negative least common multiple of two integers.",
    )
    .with_description("lcm(a, 0) is defined as 0.")
    .with_parameters(vec![
        ParamDescriptor::required("a", "First integer.", integer_schema()),
        ParamDescriptor::required("b", "Second integer.", integer_schema()),
    ])
    .with_output(integer_schema(), "lcm(a, b), always non-negative.")
    .with_modes(all_modes())
    .with_cost(CostClass::Iterative)
    .with_method_ref("docs/methods/arithmetic.md#lcm")
    .with_examples(vec![
        Example::new(
            "lcm",
            example_args(&[("a", serde_json::json!(4)), ("b", serde_json::json!(6))]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "integer", "value": "12"}),
        )),
    ])
}

fn invoke_lcm(args: &Args, _ctx: &ExecContext) -> Result<Outcome, EngineError> {
    use num_integer::Integer;
    let a = args.integer("a")?;
    let b = args.integer("b")?;
    Ok(Outcome::exact(Value::Number(Number::Integer(a.lcm(&b)))))
}

fn factorial_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "arithmetic.factorial",
        "arithmetic",
        "1.0.0",
        "Factorial",
        "Exact factorial n!.",
    )
    .with_description(
        "Defined for integers 0 <= n <= the configured factorial limit (default 10000). \
         Results are bounded by the configured integer bit limit.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "n",
        "Non-negative integer.",
        integer_schema(),
    )])
    .with_output(integer_schema(), "n! as an exact integer.")
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_method_ref("docs/methods/arithmetic.md#factorial")
    .with_examples(vec![
        Example::new("factorial", example_args(&[("n", serde_json::json!(5))])).with_value(
            parse_value(serde_json::json!({"kind": "integer", "value": "120"})),
        ),
        Example::new(
            "negative factorial",
            example_args(&[("n", serde_json::json!(-1))]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_factorial(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    use num_traits::{One, ToPrimitive};
    let n = args.integer("n")?;
    let n = n
        .to_u32()
        .ok_or_else(|| EngineError::domain("factorial requires a non-negative integer"))?;
    if n > ctx.limits.max_factorial {
        return Err(EngineError::new(
            ErrorCode::ResourceLimit,
            format!(
                "factorial argument {n} exceeds the configured limit of {}",
                ctx.limits.max_factorial
            ),
        ));
    }
    let mut result = num_bigint::BigInt::one();
    for i in 2..=n {
        ctx.check()?;
        result *= i;
    }
    if result.bits()
        > ctx
            .limits
            .max_integer_bits
            .min(ctx.numeric.max_integer_bits) as u64
    {
        return Err(EngineError::new(
            ErrorCode::ResourceLimit,
            "factorial result exceeds the integer bit limit",
        ));
    }
    Ok(Outcome::exact(Value::Number(Number::Integer(result))))
}

fn binomial_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "arithmetic.binomial",
        "arithmetic",
        "1.0.0",
        "Binomial coefficient",
        "Exact binomial coefficient C(n, k).",
    )
    .with_description(
        "Defined for integers n >= 0 and 0 <= k <= n; returns 0 when k < 0 or k > n. \
         Bounded by the configured factorial and integer bit limits.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("n", "Non-negative integer.", integer_schema()),
        ParamDescriptor::required("k", "Integer number of selections.", integer_schema()),
    ])
    .with_output(integer_schema(), "C(n, k).")
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_method_ref("docs/methods/arithmetic.md#binomial")
    .with_examples(vec![
        Example::new(
            "binomial coefficient",
            example_args(&[("n", serde_json::json!(10)), ("k", serde_json::json!(3))]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "integer", "value": "120"}),
        )),
        Example::new(
            "out of range k",
            example_args(&[("n", serde_json::json!(5)), ("k", serde_json::json!(9))]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "integer", "value": "0"}),
        )),
    ])
}

fn invoke_binomial(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    use num_traits::{One, ToPrimitive, Zero};
    let n = args.integer("n")?;
    let k = args.integer("k")?;
    if n.sign() == num_bigint::Sign::Minus {
        return Err(EngineError::domain("binomial coefficient requires n >= 0"));
    }
    let n_u32 = n
        .to_u32()
        .ok_or_else(|| EngineError::domain("binomial coefficient requires a 32-bit n"))?;
    if n_u32 > ctx.limits.max_factorial {
        return Err(EngineError::new(
            ErrorCode::ResourceLimit,
            format!(
                "binomial n={n_u32} exceeds the configured limit of {}",
                ctx.limits.max_factorial
            ),
        ));
    }
    if k.sign() == num_bigint::Sign::Minus || k > n {
        return Ok(Outcome::exact(Value::Number(Number::Integer(
            num_bigint::BigInt::zero(),
        ))));
    }
    let k_u32 = k
        .to_u32()
        .ok_or_else(|| EngineError::domain("binomial coefficient requires a 32-bit k"))?;
    let k = k_u32.min(n_u32 - k_u32);
    let mut result = num_bigint::BigInt::one();
    for i in 0..k {
        ctx.check()?;
        result = result * (n_u32 - i) / (i + 1);
    }
    Ok(Outcome::exact(Value::Number(Number::Integer(result))))
}

// ---------------------------------------------------------------------------
// Remainder family and conversions
// ---------------------------------------------------------------------------

fn rem_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "arithmetic.rem",
        "arithmetic",
        "1.0.0",
        "Truncated remainder",
        "Remainder with the sign of the dividend (like Rust's %).",
    )
    .with_parameters(vec![
        ParamDescriptor::required("a", "Dividend.", number_schema()),
        ParamDescriptor::required("b", "Divisor; must not be zero.", number_schema()),
    ])
    .with_output(number_schema(), "a - trunc(a / b) * b.")
    .with_modes(all_modes())
    .with_method_ref("docs/methods/arithmetic.md#rem")
    .with_examples(vec![
        Example::new(
            "negative dividend",
            example_args(&[("a", serde_json::json!(-7)), ("b", serde_json::json!(3))]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "integer", "value": "-1"}),
        )),
    ])
}

fn invoke_rem(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let a = number_at(args, "a")?;
    let b = number_at(args, "b")?;
    outcome(a.rem(&b, &ctx.numeric, &ctx.limits)?)
}

fn modulo_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "arithmetic.modulo",
        "arithmetic",
        "1.0.0",
        "Euclidean modulo",
        "Remainder with the sign of the divisor; non-negative for a positive divisor.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("a", "Dividend.", number_schema()),
        ParamDescriptor::required("b", "Divisor; must not be zero.", number_schema()),
    ])
    .with_output(number_schema(), "a mod b, in [0, |b|) for b > 0.")
    .with_modes(all_modes())
    .with_method_ref("docs/methods/arithmetic.md#modulo")
    .with_examples(vec![
        Example::new(
            "negative dividend",
            example_args(&[("a", serde_json::json!(-7)), ("b", serde_json::json!(3))]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "integer", "value": "2"}),
        )),
    ])
}

fn invoke_modulo(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let a = number_at(args, "a")?;
    let b = number_at(args, "b")?;
    outcome(a.modulo(&b, &ctx.numeric, &ctx.limits)?)
}

fn to_rational_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "arithmetic.to_rational",
        "arithmetic",
        "1.0.0",
        "Convert to rational",
        "Convert an exact number to a normalized rational; float64 converts via its exact bits.",
    )
    .with_description(
        "The rational is normalized with a positive denominator and no common factors. \
         float64 conversion is exact in the sense that it recovers the binary value.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "value",
        "Input number.",
        number_schema(),
    )])
    .with_output(
        ValueSchema::number(NumberKind::Rational),
        "Normalized rational value.",
    )
    .with_modes(all_modes())
    .with_method_ref("docs/methods/arithmetic.md#to_rational")
    .with_examples(vec![
        Example::new(
            "decimal to rational",
            example_args(&[("value", serde_json::json!("0.25"))]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "rational", "numerator": "1", "denominator": "4"}),
        )),
    ])
}

fn invoke_to_rational(args: &Args, _ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let value = number_at(args, "value")?;
    let rational = value
        .to_exact_rational()
        .ok_or_else(|| EngineError::domain("value cannot be represented as a rational"))?;
    Ok(Outcome::exact(Value::Number(Number::Rational(rational))))
}

fn to_decimal_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "arithmetic.to_decimal",
        "arithmetic",
        "1.0.0",
        "Convert to decimal",
        "Convert an exact number to a decimal under the declared rounding context.",
    )
    .with_description(
        "A terminating rational converts exactly. A non-terminating rational rounds to the \
         context precision and is marked rounded. float64 is not silently converted.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "value",
        "Input number.",
        number_schema(),
    )])
    .with_output(ValueSchema::number(NumberKind::Decimal), "Decimal value.")
    .with_modes(all_modes())
    .with_method_ref("docs/methods/arithmetic.md#to_decimal")
    .with_examples(vec![
        Example::new(
            "terminating rational",
            example_args(&[(
                "value",
                serde_json::json!({"kind": "rational", "numerator": "1", "denominator": "8"}),
            )]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "decimal", "value": "0.125"}),
        )),
        Example::new(
            "non-terminating rational in exact mode",
            example_args(&[(
                "value",
                serde_json::json!({"kind": "rational", "numerator": "1", "denominator": "3"}),
            )]),
        )
        .with_error(ErrorCode::UnsupportedNumericMode),
    ])
}

fn invoke_to_decimal(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let value = number_at(args, "value")?;
    match &value {
        Number::Decimal(decimal) => Ok(Outcome::exact(Value::Number(Number::Decimal(
            decimal.clone(),
        )))),
        Number::Integer(integer) => Ok(Outcome::exact(Value::Number(Number::Decimal(
            Decimal::from_bigint(integer.clone()),
        )))),
        Number::Rational(rational) => {
            let (decimal, inexact) = Decimal::from_rational(rational, &ctx.numeric, &ctx.limits)?;
            Ok(Outcome::new(
                Value::Number(Number::Decimal(decimal)),
                if inexact {
                    Exactness::Rounded
                } else {
                    Exactness::Exact
                },
            ))
        }
        Number::Float64(_) => Err(EngineError::new(
            ErrorCode::UnsupportedNumericMode,
            "float64 is not converted to an exact decimal implicitly; use a decimal input \
             or request a scientific result",
        )),
    }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

fn example_args(pairs: &[(&str, serde_json::Value)]) -> BTreeMap<String, Value> {
    pairs
        .iter()
        .map(|(name, raw)| (name.to_string(), parse_value(raw.clone())))
        .collect()
}

fn parse_value(raw: serde_json::Value) -> Value {
    serde_json::from_value(raw).expect("example value must be valid")
}

/// Build the arithmetic module with all of its registered functions.
pub fn module() -> Module {
    let functions: Vec<Arc<dyn bicmath_core::contract::Function>> = vec![
        SimpleFunction::arc(add_descriptor(), invoke_add),
        SimpleFunction::arc(sub_descriptor(), invoke_sub),
        SimpleFunction::arc(mul_descriptor(), invoke_mul),
        SimpleFunction::arc(div_descriptor(), invoke_div),
        SimpleFunction::arc(sum_descriptor(), invoke_sum),
        SimpleFunction::arc(product_descriptor(), invoke_product),
        SimpleFunction::arc(
            minmax_descriptor(
                "arithmetic.min",
                "Minimum",
                "Smallest element of a non-empty array.",
                true,
            ),
            invoke_min,
        ),
        SimpleFunction::arc(
            minmax_descriptor(
                "arithmetic.max",
                "Maximum",
                "Largest element of a non-empty array.",
                false,
            ),
            invoke_max,
        ),
        SimpleFunction::arc(abs_descriptor(), invoke_abs),
        SimpleFunction::arc(compare_descriptor(), invoke_compare),
        SimpleFunction::arc(pow_descriptor(), invoke_pow),
        SimpleFunction::arc(sqrt_descriptor(), invoke_sqrt),
        SimpleFunction::arc(
            rounding_descriptor(
                "arithmetic.floor",
                "Floor",
                "Round toward negative infinity.",
            ),
            invoke_floor,
        ),
        SimpleFunction::arc(
            rounding_descriptor(
                "arithmetic.ceil",
                "Ceiling",
                "Round toward positive infinity.",
            ),
            invoke_ceil,
        ),
        SimpleFunction::arc(
            rounding_descriptor("arithmetic.trunc", "Truncate", "Round toward zero."),
            invoke_trunc,
        ),
        SimpleFunction::arc(quantize_descriptor(), invoke_quantize),
        SimpleFunction::arc(gcd_descriptor(), invoke_gcd),
        SimpleFunction::arc(lcm_descriptor(), invoke_lcm),
        SimpleFunction::arc(factorial_descriptor(), invoke_factorial),
        SimpleFunction::arc(binomial_descriptor(), invoke_binomial),
        SimpleFunction::arc(rem_descriptor(), invoke_rem),
        SimpleFunction::arc(modulo_descriptor(), invoke_modulo),
        SimpleFunction::arc(to_rational_descriptor(), invoke_to_rational),
        SimpleFunction::arc(to_decimal_descriptor(), invoke_to_decimal),
    ];
    let descriptor = ModuleDescriptor::new(
        "arithmetic",
        "Arithmetic",
        "1.0.0",
        "Exact arithmetic, rounding, comparison, and integer/rational operations.",
    )
    .with_capabilities(vec![
        "exact_integer",
        "exact_rational",
        "exact_decimal",
        "rounding_modes",
    ])
    .with_dependencies(vec!["core"])
    .with_modes(all_modes())
    .with_source("crates/bicmath-arithmetic");
    Module::new(descriptor, functions)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> ExecContext {
        ExecContext::conservative()
    }

    fn call(id: &str, raw: serde_json::Value) -> Result<Outcome, EngineError> {
        let module = module();
        let function = module
            .functions
            .iter()
            .find(|f| f.descriptor().id == id)
            .expect("function exists");
        let args_json = raw.as_object().expect("object args");
        let mut values = BTreeMap::new();
        for (name, value) in args_json {
            let param = function
                .descriptor()
                .parameter(name)
                .expect("parameter exists");
            values.insert(
                name.clone(),
                param
                    .schema
                    .coerce(value, name, &ctx().limits, true)
                    .expect("argument coerces"),
            );
        }
        function.invoke(&Args::new(values), &ctx())
    }

    #[test]
    fn exact_decimal_addition_is_not_binary() {
        let outcome = call(
            "arithmetic.add",
            serde_json::json!({"a": "0.1", "b": "0.2"}),
        )
        .unwrap();
        assert_eq!(outcome.exactness, Exactness::Exact);
        match outcome.value {
            Value::Number(Number::Decimal(decimal)) => {
                assert_eq!(decimal.to_plain_string(), "0.3")
            }
            other => panic!("expected decimal, got {other:?}"),
        }
    }

    #[test]
    fn big_integer_addition_is_exact() {
        let outcome = call(
            "arithmetic.add",
            serde_json::json!({"a": {"kind": "integer", "value": "9007199254740993"}, "b": 1}),
        )
        .unwrap();
        match outcome.value {
            Value::Number(Number::Integer(value)) => {
                assert_eq!(value.to_string(), "9007199254740994")
            }
            other => panic!("expected integer, got {other:?}"),
        }
    }

    #[test]
    fn compensated_float_sum_beats_naive_fold() {
        let mut limits = ctx();
        limits.numeric.mode = NumericMode::Scientific;
        let module = module();
        let function = module
            .functions
            .iter()
            .find(|f| f.descriptor().id == "arithmetic.sum")
            .unwrap();
        let raw = serde_json::json!({"values": [
            {"kind": "float64", "value": "1e16"},
            {"kind": "float64", "value": "1"},
            {"kind": "float64", "value": "-1e16"}
        ]});
        let mut values = BTreeMap::new();
        let schema = function
            .descriptor()
            .parameter("values")
            .unwrap()
            .schema
            .clone();
        values.insert(
            "values".to_string(),
            schema
                .coerce(&raw["values"], "values", &limits.limits, true)
                .unwrap(),
        );
        let outcome = function.invoke(&Args::new(values), &limits).unwrap();
        match outcome.value {
            Value::Number(Number::Float64(value)) => assert_eq!(value.get(), 1.0),
            other => panic!("expected float, got {other:?}"),
        }
    }

    #[test]
    fn quantize_half_even_and_half_away() {
        let a = call(
            "arithmetic.quantize",
            serde_json::json!({"value": "1.005", "scale": 2}),
        )
        .unwrap();
        let b = call(
            "arithmetic.quantize",
            serde_json::json!({"value": "1.005", "scale": 2, "mode": "half_away_from_zero"}),
        )
        .unwrap();
        let text = |outcome: Outcome| match outcome.value {
            Value::Number(Number::Decimal(d)) => d.to_plain_string(),
            other => panic!("expected decimal, got {other:?}"),
        };
        assert_eq!(text(a), "1.00");
        assert_eq!(text(b), "1.01");
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
}
