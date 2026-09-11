//! Money operations: conversion, addition, subtraction, scaling, comparison,
//! and deterministic allocation.
//!
//! Every function enforces the shared money semantics: amounts are exact
//! decimals or integers, float64 is rejected, and different currencies are
//! never combined without an explicit conversion.

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{Signed, ToPrimitive};

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, Assumption, CostClass, Example, FunctionDescriptor, Outcome, ParamDescriptor, Warning,
};
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::number::Number;
use bicmath_core::schema::ValueSchema;
use bicmath_core::value::{Value, validate_currency};

use crate::util::{
    MAX_ALLOCATION_SCALE, SETTLEMENT_SCALE, all_modes, arg_money, decimal_to_minor, enum_schema,
    exact_schema, example_args, field, integer_schema, minor_to_decimal, money_field, money_schema,
    money_value, number_to_rational, parse_value, quantize, record_schema, require_currency,
    text_schema,
};

fn convert_money_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "finance.convert_money",
        "finance",
        "1.0.0",
        "Convert money",
        "Convert a money amount with a caller-supplied target-per-source rate.",
    )
    .with_description(
        "The rate is the number of target-currency units per one source-currency unit \
         (target per source). The converted amount is rounded to the settlement scale of \
         2 decimal places (half-even). No FX rate is looked up or inferred: the rate is \
         caller-supplied, its direction and any as-of date are recorded in warnings, and \
         the result is classified rounded.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("money", "Amount to convert.", money_schema()),
        ParamDescriptor::required("rate", "Target-per-source decimal rate.", exact_schema()),
        ParamDescriptor::required(
            "target_currency",
            "Target currency code (2-12 uppercase letters or digits).",
            text_schema(),
        ),
        ParamDescriptor::optional(
            "rate_as_of",
            "Optional YYYY-MM-DD date the supplied rate refers to.",
            text_schema(),
        ),
    ])
    .with_output(
        money_schema(),
        "Converted money in target_currency, rounded to settlement scale 2.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Constant)
    .with_method_ref("docs/methods/finance.md#convert_money")
    .with_examples(vec![
        Example::new(
            "USD to EUR at a supplied rate",
            example_args(&[
                (
                    "money",
                    serde_json::json!({
                        "kind": "money",
                        "amount": {"kind": "decimal", "value": "100.00"},
                        "currency": "USD"
                    }),
                ),
                ("rate", serde_json::json!("0.90")),
                ("target_currency", serde_json::json!("EUR")),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "kind": "money",
            "amount": {"kind": "decimal", "value": "90.00"},
            "currency": "EUR"
        }))),
        Example::new(
            "invalid target currency",
            example_args(&[
                (
                    "money",
                    serde_json::json!({
                        "kind": "money",
                        "amount": {"kind": "decimal", "value": "1.00"},
                        "currency": "USD"
                    }),
                ),
                ("rate", serde_json::json!("1.0")),
                ("target_currency", serde_json::json!("usd")),
            ]),
        )
        .with_error(ErrorCode::MalformedInput),
    ])
}

fn invoke_convert_money(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let (amount, source) = arg_money(args, "money")?;
    let rate = args.decimal("rate")?;
    let target = args.text("target_currency")?;
    validate_currency(target)?;
    let rate_as_of = match args.optional_text("rate_as_of")? {
        Some(text) => {
            crate::util::parse_date(text, "rate_as_of")?;
            Some(text.to_string())
        }
        None => None,
    };
    let product = amount.mul(&rate, &ctx.numeric, &ctx.limits)?;
    let (converted, _changed) = quantize(&product, SETTLEMENT_SCALE);
    let mut details = vec![
        ("rate_direction", Value::text("target_per_source")),
        ("rate", Value::Number(Number::Decimal(rate))),
        ("source_currency", Value::text(source)),
        ("target_currency", Value::text(target.to_string())),
        (
            "settlement_scale",
            Value::integer(BigInt::from(SETTLEMENT_SCALE)),
        ),
    ];
    if let Some(as_of) = &rate_as_of {
        details.push(("rate_as_of", Value::text(as_of.clone())));
    }
    Ok(Outcome::rounded(money_value(converted, target))
        .with_warning(
            Warning::new(
                "fx_rate_user_supplied",
                "conversion used a caller-supplied rate; no market rate was looked up",
            )
            .with_details(Value::record(details)),
        )
        .with_assumption(Assumption::user_supplied(
            "fx_rate_direction",
            "rate is interpreted as target-currency units per one source-currency unit",
        )))
}

fn money_add_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "finance.money_add",
        "finance",
        "1.0.0",
        "Add money",
        "Add two money amounts in the same currency.",
    )
    .with_description(
        "Exact decimal addition. Both amounts must share one currency; different \
         currencies are rejected with a currency mismatch error and are never combined \
         implicitly.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("a", "Left amount.", money_schema()),
        ParamDescriptor::required("b", "Right amount.", money_schema()),
    ])
    .with_output(money_schema(), "Sum a + b in the shared currency.")
    .with_modes(all_modes())
    .with_cost(CostClass::Constant)
    .with_method_ref("docs/methods/finance.md#money_add")
    .with_examples(vec![
        Example::new(
            "add USD amounts",
            example_args(&[
                (
                    "a",
                    serde_json::json!({
                        "kind": "money",
                        "amount": {"kind": "decimal", "value": "1.10"},
                        "currency": "USD"
                    }),
                ),
                (
                    "b",
                    serde_json::json!({
                        "kind": "money",
                        "amount": {"kind": "decimal", "value": "2.20"},
                        "currency": "USD"
                    }),
                ),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "kind": "money",
            "amount": {"kind": "decimal", "value": "3.30"},
            "currency": "USD"
        }))),
        Example::new(
            "currency mismatch",
            example_args(&[
                (
                    "a",
                    serde_json::json!({
                        "kind": "money",
                        "amount": {"kind": "decimal", "value": "1.00"},
                        "currency": "USD"
                    }),
                ),
                (
                    "b",
                    serde_json::json!({
                        "kind": "money",
                        "amount": {"kind": "decimal", "value": "1.00"},
                        "currency": "INR"
                    }),
                ),
            ]),
        )
        .with_error(ErrorCode::CurrencyMismatch),
    ])
}

fn invoke_money_add(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let (a, currency) = arg_money(args, "a")?;
    let (b, other) = arg_money(args, "b")?;
    require_currency(&currency, &other)?;
    let sum = a.add(&b, &ctx.numeric, &ctx.limits)?;
    Ok(Outcome::exact(money_value(sum, &currency)))
}

fn money_sub_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "finance.money_sub",
        "finance",
        "1.0.0",
        "Subtract money",
        "Subtract one money amount from another in the same currency.",
    )
    .with_description(
        "Exact decimal subtraction. Both amounts must share one currency; different \
         currencies are rejected with a currency mismatch error.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("a", "Minuend.", money_schema()),
        ParamDescriptor::required("b", "Subtrahend.", money_schema()),
    ])
    .with_output(money_schema(), "Difference a - b in the shared currency.")
    .with_modes(all_modes())
    .with_cost(CostClass::Constant)
    .with_method_ref("docs/methods/finance.md#money_sub")
    .with_examples(vec![
        Example::new(
            "subtract USD amounts",
            example_args(&[
                (
                    "a",
                    serde_json::json!({
                        "kind": "money",
                        "amount": {"kind": "decimal", "value": "5.00"},
                        "currency": "USD"
                    }),
                ),
                (
                    "b",
                    serde_json::json!({
                        "kind": "money",
                        "amount": {"kind": "decimal", "value": "1.25"},
                        "currency": "USD"
                    }),
                ),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "kind": "money",
            "amount": {"kind": "decimal", "value": "3.75"},
            "currency": "USD"
        }))),
    ])
}

fn invoke_money_sub(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let (a, currency) = arg_money(args, "a")?;
    let (b, other) = arg_money(args, "b")?;
    require_currency(&currency, &other)?;
    let difference = a.sub(&b, &ctx.numeric, &ctx.limits)?;
    Ok(Outcome::exact(money_value(difference, &currency)))
}

fn money_scale_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "finance.money_scale",
        "finance",
        "1.0.0",
        "Scale money",
        "Multiply a money amount by an exact decimal factor.",
    )
    .with_description(
        "Exact decimal multiplication: the amount is scaled by a caller-supplied exact \
         decimal or integer factor. The currency is unchanged.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("money", "Amount to scale.", money_schema()),
        ParamDescriptor::required("factor", "Exact decimal factor.", exact_schema()),
    ])
    .with_output(money_schema(), "Scaled amount in the same currency.")
    .with_modes(all_modes())
    .with_cost(CostClass::Constant)
    .with_method_ref("docs/methods/finance.md#money_scale")
    .with_examples(vec![
        Example::new(
            "scale by an integer factor",
            example_args(&[
                (
                    "money",
                    serde_json::json!({
                        "kind": "money",
                        "amount": {"kind": "decimal", "value": "2.50"},
                        "currency": "USD"
                    }),
                ),
                ("factor", serde_json::json!(3)),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "kind": "money",
            "amount": {"kind": "decimal", "value": "7.50"},
            "currency": "USD"
        }))),
    ])
}

fn invoke_money_scale(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let (amount, currency) = arg_money(args, "money")?;
    let factor = args.decimal("factor")?;
    let product = amount.mul(&factor, &ctx.numeric, &ctx.limits)?;
    Ok(Outcome::exact(money_value(product, &currency)))
}

fn money_compare_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "finance.money_compare",
        "finance",
        "1.0.0",
        "Compare money",
        "Compare two money amounts in the same currency.",
    )
    .with_description(
        "Exact numeric comparison. Both amounts must share one currency; different \
         currencies are rejected with a currency mismatch error.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("a", "Left amount.", money_schema()),
        ParamDescriptor::required("b", "Right amount.", money_schema()),
    ])
    .with_output(
        record_schema(vec![
            field("ordering", enum_schema(&["less", "equal", "greater"])),
            field("sign", integer_schema()),
        ]),
        "Ordering label and integer sign (-1, 0, 1).",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Constant)
    .with_method_ref("docs/methods/finance.md#money_compare")
    .with_examples(vec![
        Example::new(
            "compare USD amounts",
            example_args(&[
                (
                    "a",
                    serde_json::json!({
                        "kind": "money",
                        "amount": {"kind": "decimal", "value": "1.00"},
                        "currency": "USD"
                    }),
                ),
                (
                    "b",
                    serde_json::json!({
                        "kind": "money",
                        "amount": {"kind": "decimal", "value": "2.00"},
                        "currency": "USD"
                    }),
                ),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "ordering": "less",
            "sign": {"kind": "integer", "value": "-1"}
        }))),
    ])
}

fn invoke_money_compare(args: &Args, _ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let (a, currency) = arg_money(args, "a")?;
    let (b, other) = arg_money(args, "b")?;
    require_currency(&currency, &other)?;
    let ordering = a.numeric_cmp(&b);
    let (label, sign) = match ordering {
        std::cmp::Ordering::Less => ("less", -1i64),
        std::cmp::Ordering::Equal => ("equal", 0),
        std::cmp::Ordering::Greater => ("greater", 1),
    };
    Ok(Outcome::exact(Value::record([
        ("ordering", Value::text(label)),
        ("sign", Value::integer(BigInt::from(sign))),
    ])))
}

fn money_allocate_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "finance.money_allocate",
        "finance",
        "1.0.0",
        "Allocate money",
        "Split a money amount by non-negative weights with an exact sum.",
    )
    .with_description(
        "Splits the amount into shares at the requested settlement scale (default 2). \
         Each exact share is floored to minor units; the remaining minor units are \
         distributed one at a time to recipients ordered by descending fractional \
         remainder, with ties broken by ascending index. This largest-remainder rule is \
         deterministic and makes the shares sum EXACTLY to the original total. Weights \
         must be non-negative exact numbers with a positive sum; the amount must be \
         exactly representable at the requested scale.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("money", "Amount to allocate.", money_schema()),
        ParamDescriptor::required(
            "weights",
            "Non-negative exact weights; at least one must be positive.",
            ValueSchema::array_with_len(exact_schema(), 1, None),
        ),
        ParamDescriptor::optional(
            "scale",
            "Settlement scale for the shares, 0-18; default 2.",
            integer_schema(),
        ),
    ])
    .with_output(
        record_schema(vec![
            field("shares", ValueSchema::array(money_schema())),
            money_field("total"),
            field("scale", integer_schema()),
            field("currency", text_schema()),
        ]),
        "Shares, exact total, settlement scale, and currency.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_method_ref("docs/methods/finance.md#money_allocate")
    .with_examples(vec![Example::new(
        "allocate USD 10.00 by equal weights",
        example_args(&[
            (
                "money",
                serde_json::json!({
                    "kind": "money",
                    "amount": {"kind": "decimal", "value": "10.00"},
                    "currency": "USD"
                }),
            ),
            ("weights", serde_json::json!([1, 1, 1])),
        ]),
    )
    .with_value(parse_value(serde_json::json!({
        "shares": [
            {"kind": "money", "amount": {"kind": "decimal", "value": "3.34"}, "currency": "USD"},
            {"kind": "money", "amount": {"kind": "decimal", "value": "3.33"}, "currency": "USD"},
            {"kind": "money", "amount": {"kind": "decimal", "value": "3.33"}, "currency": "USD"}
        ],
        "total": {"kind": "money", "amount": {"kind": "decimal", "value": "10.00"}, "currency": "USD"},
        "scale": {"kind": "integer", "value": "2"},
        "currency": "USD"
    })))])
}

fn collect_weights(args: &Args) -> Result<Vec<BigRational>, EngineError> {
    let items = args.array("weights")?;
    if items.is_empty() {
        return Err(EngineError::new(
            ErrorCode::InsufficientObservations,
            "weights must contain at least one entry",
        ));
    }
    let mut out = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let path = format!("weights[{index}]");
        let number = item
            .as_number()
            .map_err(|error| error.with_path(path.clone()))?;
        let weight = number_to_rational(number, &path)?;
        if weight.is_negative() {
            return Err(
                EngineError::domain(format!("weight at {path} must be non-negative"))
                    .with_path(path),
            );
        }
        out.push(weight);
    }
    Ok(out)
}

fn invoke_money_allocate(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let (amount, currency) = arg_money(args, "money")?;
    let scale = match args.optional_integer("scale")? {
        None => SETTLEMENT_SCALE,
        Some(value) => value
            .to_u32()
            .filter(|scale| *scale <= MAX_ALLOCATION_SCALE)
            .ok_or_else(|| {
                EngineError::domain(format!(
                    "scale must be an integer between 0 and {MAX_ALLOCATION_SCALE}"
                ))
            })?,
    };
    let total_minor = decimal_to_minor(&amount, scale)?;
    let weights = collect_weights(args)?;
    let weight_sum: BigRational = weights.iter().cloned().sum();
    if !weight_sum.is_positive() {
        return Err(EngineError::domain(
            "weights must have a positive sum to allocate an amount",
        ));
    }
    let total_rational = BigRational::from_integer(total_minor.clone());
    let mut floors = Vec::with_capacity(weights.len());
    let mut remainders = Vec::with_capacity(weights.len());
    for weight in &weights {
        ctx.check()?;
        let share = &total_rational * weight / &weight_sum;
        let floor = share.floor().to_integer();
        let remainder = &share - BigRational::from_integer(floor.clone());
        floors.push(floor);
        remainders.push(remainder);
    }
    let floor_sum: BigInt = floors.iter().cloned().sum();
    let remaining = &total_minor - &floor_sum;
    let remaining_count = remaining.to_usize().ok_or_else(|| {
        EngineError::internal("allocation remainder is not representable as an index")
    })?;
    if remaining_count > weights.len() {
        return Err(EngineError::internal(
            "allocation remainder exceeds the number of recipients",
        ));
    }
    let mut order: Vec<usize> = (0..weights.len()).collect();
    order.sort_by(|&i, &j| remainders[j].cmp(&remainders[i]).then_with(|| i.cmp(&j)));
    for &index in order.iter().take(remaining_count) {
        floors[index] += 1u32;
    }
    let shares: Vec<Value> = floors
        .iter()
        .map(|minor| money_value(minor_to_decimal(minor, scale), &currency))
        .collect();
    let total = money_value(minor_to_decimal(&total_minor, scale), &currency);
    let value = Value::record([
        ("shares", Value::Array(shares)),
        ("total", total),
        ("scale", Value::integer(BigInt::from(scale))),
        ("currency", Value::text(currency)),
    ]);
    Ok(Outcome::new(
        value,
        crate::util::exactness(remaining_count > 0),
    ))
}

pub(crate) fn register() -> Vec<std::sync::Arc<dyn bicmath_core::contract::Function>> {
    use bicmath_core::contract::SimpleFunction;
    vec![
        SimpleFunction::arc(convert_money_descriptor(), invoke_convert_money),
        SimpleFunction::arc(money_add_descriptor(), invoke_money_add),
        SimpleFunction::arc(money_sub_descriptor(), invoke_money_sub),
        SimpleFunction::arc(money_scale_descriptor(), invoke_money_scale),
        SimpleFunction::arc(money_compare_descriptor(), invoke_money_compare),
        SimpleFunction::arc(money_allocate_descriptor(), invoke_money_allocate),
    ]
}
