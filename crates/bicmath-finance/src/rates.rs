//! Business ratios: percentage change, margin/markup, break-even, and
//! contribution analysis.

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::Zero;

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, CostClass, Example, FunctionDescriptor, Outcome, ParamDescriptor,
};
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::number::{Decimal, RoundingMode};
use bicmath_core::schema::ValueSchema;
use bicmath_core::value::Value;

use crate::util::{
    SETTLEMENT_SCALE, all_modes, arg_money, decimal_from_number, exact_schema, exactness,
    example_args, field, integer_schema, money_field, money_schema, money_value,
    number_to_rational, optional_arg_money, optional_field, parse_value, quantize,
    rational_to_decimal, record_schema, require_currency, round_rational_to_bigint, text_schema,
};

fn exact_number(args: &Args, name: &str) -> Result<BigRational, EngineError> {
    let number = args.number(name)?;
    number_to_rational(number, name)
}

fn percentage_change_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "finance.percentage_change",
        "finance",
        "1.0.0",
        "Percentage change",
        "Relative change (new - old) / old as an exact decimal when possible.",
    )
    .with_description(
        "Returns the fractional change, not the percentage: 0.25 means +25%. The result \
         is exact when (new - old) / old terminates as a decimal and rounded otherwise. \
         old_value must be non-zero.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("old_value", "Original value.", exact_schema()),
        ParamDescriptor::required("new_value", "New value.", exact_schema()),
    ])
    .with_output(exact_schema(), "Fractional change (new - old) / old.")
    .with_modes(all_modes())
    .with_cost(CostClass::Constant)
    .with_method_ref("docs/methods/finance.md#percentage_change")
    .with_examples(vec![
        Example::new(
            "increase from 100 to 125",
            example_args(&[
                ("old_value", serde_json::json!(100)),
                ("new_value", serde_json::json!(125)),
            ]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "decimal", "value": "0.25"}),
        )),
        Example::new(
            "zero baseline",
            example_args(&[
                ("old_value", serde_json::json!(0)),
                ("new_value", serde_json::json!(10)),
            ]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_percentage_change(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let old = exact_number(args, "old_value")?;
    let new = exact_number(args, "new_value")?;
    if old.is_zero() {
        return Err(EngineError::domain(
            "percentage change is undefined when old_value is zero",
        ));
    }
    let change = (&new - &old) / &old;
    let (decimal, inexact) = rational_to_decimal(&change, ctx)?;
    Ok(Outcome::new(Value::decimal(decimal), exactness(inexact)))
}

fn markup_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "finance.markup",
        "finance",
        "1.0.0",
        "Markup from margin",
        "Convert a margin to the equivalent markup.",
    )
    .with_description(
        "markup = margin / (1 - margin). The margin is a decimal in (-infinity, 1); a \
         margin of 1 or more is a domain error. The result is exact when it terminates \
         as a decimal and rounded otherwise.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "margin",
        "Margin as a decimal fraction (0.25 means 25%).",
        exact_schema(),
    )])
    .with_output(exact_schema(), "Markup margin / (1 - margin).")
    .with_modes(all_modes())
    .with_cost(CostClass::Constant)
    .with_method_ref("docs/methods/finance.md#markup")
    .with_examples(vec![
        Example::new(
            "margin 20%",
            example_args(&[("margin", serde_json::json!("0.2"))]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "decimal", "value": "0.25"}),
        )),
        Example::new(
            "margin at or above 100%",
            example_args(&[("margin", serde_json::json!("1"))]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_markup(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let margin = exact_number(args, "margin")?;
    let one = BigRational::from_integer(BigInt::from(1));
    if margin >= one {
        return Err(EngineError::domain(
            "margin must be less than 1 (use -infinity..1)",
        ));
    }
    let denominator = &one - &margin;
    if denominator.is_zero() {
        return Err(EngineError::domain("margin must be less than 1"));
    }
    let markup = &margin / &denominator;
    let (decimal, inexact) = rational_to_decimal(&markup, ctx)?;
    Ok(Outcome::new(Value::decimal(decimal), exactness(inexact)))
}

fn margin_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "finance.margin",
        "finance",
        "1.0.0",
        "Margin from markup",
        "Convert a markup to the equivalent margin.",
    )
    .with_description(
        "margin = markup / (1 + markup). The markup must be greater than -1 so the \
         resulting margin is below 1; markup <= -1 is a domain error. The result is \
         exact when it terminates as a decimal and rounded otherwise.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "markup",
        "Markup as a decimal fraction (0.25 means 25%).",
        exact_schema(),
    )])
    .with_output(exact_schema(), "Margin markup / (1 + markup).")
    .with_modes(all_modes())
    .with_cost(CostClass::Constant)
    .with_method_ref("docs/methods/finance.md#margin")
    .with_examples(vec![
        Example::new(
            "markup 25%",
            example_args(&[("markup", serde_json::json!("0.25"))]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "decimal", "value": "0.2"}),
        )),
        Example::new(
            "markup at -100%",
            example_args(&[("markup", serde_json::json!("-1"))]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_margin(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let markup = exact_number(args, "markup")?;
    let minus_one = BigRational::from_integer(BigInt::from(-1));
    if markup <= minus_one {
        return Err(EngineError::domain(
            "markup must be greater than -1 so the margin stays below 1",
        ));
    }
    let denominator = BigRational::from_integer(BigInt::from(1)) + &markup;
    let margin = &markup / &denominator;
    let (decimal, inexact) = rational_to_decimal(&margin, ctx)?;
    Ok(Outcome::new(Value::decimal(decimal), exactness(inexact)))
}

const BREAK_EVEN_METHOD: &str = "unit_contribution = unit_price - unit_variable_cost; \
     break_even_units = ceil(fixed_costs / unit_contribution); \
     break_even_revenue = break_even_units * unit_price";

fn break_even_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "finance.break_even",
        "finance",
        "1.0.0",
        "Break-even",
        "Break-even units and revenue from fixed costs and unit economics.",
    )
    .with_description(
        "All three amounts must share one currency. The unit contribution must be \
         strictly positive; otherwise the break-even point does not exist and a domain \
         error is returned. Break-even units are rounded up to whole units and the \
         break-even revenue is that whole number of units times the unit price.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("fixed_costs", "Total fixed costs.", money_schema()),
        ParamDescriptor::required("unit_price", "Selling price per unit.", money_schema()),
        ParamDescriptor::required(
            "unit_variable_cost",
            "Variable cost per unit.",
            money_schema(),
        ),
    ])
    .with_output(
        record_schema(vec![
            money_field("unit_contribution"),
            field("break_even_units", integer_schema()),
            money_field("break_even_revenue"),
            field("method", text_schema()),
        ]),
        "Unit contribution, whole break-even units, break-even revenue, and method.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Constant)
    .with_method_ref("docs/methods/finance.md#break_even")
    .with_examples(vec![
        Example::new(
            "break even at 250 units",
            example_args(&[
                (
                    "fixed_costs",
                    serde_json::json!({
                        "kind": "money",
                        "amount": {"kind": "decimal", "value": "1000.00"},
                        "currency": "USD"
                    }),
                ),
                (
                    "unit_price",
                    serde_json::json!({
                        "kind": "money",
                        "amount": {"kind": "decimal", "value": "10.00"},
                        "currency": "USD"
                    }),
                ),
                (
                    "unit_variable_cost",
                    serde_json::json!({
                        "kind": "money",
                        "amount": {"kind": "decimal", "value": "6.00"},
                        "currency": "USD"
                    }),
                ),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "unit_contribution": {
                "kind": "money",
                "amount": {"kind": "decimal", "value": "4.00"},
                "currency": "USD"
            },
            "break_even_units": {"kind": "integer", "value": "250"},
            "break_even_revenue": {
                "kind": "money",
                "amount": {"kind": "decimal", "value": "2500.00"},
                "currency": "USD"
            },
            "method": BREAK_EVEN_METHOD
        }))),
        Example::new(
            "negative unit contribution",
            example_args(&[
                (
                    "fixed_costs",
                    serde_json::json!({
                        "kind": "money",
                        "amount": {"kind": "decimal", "value": "1000.00"},
                        "currency": "USD"
                    }),
                ),
                (
                    "unit_price",
                    serde_json::json!({
                        "kind": "money",
                        "amount": {"kind": "decimal", "value": "6.00"},
                        "currency": "USD"
                    }),
                ),
                (
                    "unit_variable_cost",
                    serde_json::json!({
                        "kind": "money",
                        "amount": {"kind": "decimal", "value": "10.00"},
                        "currency": "USD"
                    }),
                ),
            ]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_break_even(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let (fixed_costs, fixed_currency) = arg_money(args, "fixed_costs")?;
    let (unit_price, price_currency) = arg_money(args, "unit_price")?;
    let (unit_variable_cost, cost_currency) = arg_money(args, "unit_variable_cost")?;
    require_currency(&fixed_currency, &price_currency)?;
    require_currency(&fixed_currency, &cost_currency)?;
    let unit_contribution = unit_price.sub(&unit_variable_cost, &ctx.numeric, &ctx.limits)?;
    if !unit_contribution.is_positive() {
        return Err(EngineError::domain(
            "unit contribution (unit_price - unit_variable_cost) must be positive",
        ));
    }
    let units_exact = fixed_costs.to_rational() / unit_contribution.to_rational();
    let break_even_units = round_rational_to_bigint(&units_exact, RoundingMode::Ceiling);
    let break_even_revenue = unit_price.mul(
        &Decimal::from_bigint(break_even_units.clone()),
        &ctx.numeric,
        &ctx.limits,
    )?;
    let value = Value::record([
        (
            "unit_contribution",
            money_value(unit_contribution, &fixed_currency),
        ),
        ("break_even_units", Value::integer(break_even_units)),
        (
            "break_even_revenue",
            money_value(break_even_revenue, &fixed_currency),
        ),
        ("method", Value::text(BREAK_EVEN_METHOD)),
    ]);
    Ok(Outcome::exact(value))
}

fn contribution_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "finance.contribution",
        "finance",
        "1.0.0",
        "Contribution analysis",
        "Gross and net contribution with optional per-unit contribution margin.",
    )
    .with_description(
        "gross_contribution = revenue - refunds - variable_costs; \
         net_contribution = gross_contribution - fixed_costs. Fixed costs are kept \
         separate from variable costs. All money amounts must be in the currency named \
         by the currency parameter. When units is supplied, margin_per_unit is the gross \
         contribution per unit, rounded to settlement scale 2.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("revenue", "Gross revenue.", money_schema()),
        ParamDescriptor::optional("refunds", "Refunds or returns.", money_schema()),
        ParamDescriptor::optional("variable_costs", "Total variable costs.", money_schema()),
        ParamDescriptor::optional("fixed_costs", "Total fixed costs.", money_schema()),
        ParamDescriptor::required(
            "currency",
            "Currency all amounts must share.",
            text_schema(),
        ),
        ParamDescriptor::optional(
            "units",
            "Optional positive unit count for margin_per_unit.",
            exact_schema(),
        ),
    ])
    .with_output(
        record_schema(vec![
            money_field("gross_contribution"),
            money_field("net_contribution"),
            optional_field("margin_per_unit", ValueSchema::Money),
        ]),
        "Gross contribution, net contribution, and optional per-unit margin.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Constant)
    .with_method_ref("docs/methods/finance.md#contribution")
    .with_examples(vec![
        Example::new(
            "contribution with fixed costs",
            example_args(&[
                (
                    "revenue",
                    serde_json::json!({
                        "kind": "money",
                        "amount": {"kind": "decimal", "value": "1000.00"},
                        "currency": "USD"
                    }),
                ),
                (
                    "variable_costs",
                    serde_json::json!({
                        "kind": "money",
                        "amount": {"kind": "decimal", "value": "400.00"},
                        "currency": "USD"
                    }),
                ),
                (
                    "fixed_costs",
                    serde_json::json!({
                        "kind": "money",
                        "amount": {"kind": "decimal", "value": "200.00"},
                        "currency": "USD"
                    }),
                ),
                ("currency", serde_json::json!("USD")),
                ("units", serde_json::json!(100)),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "gross_contribution": {
                "kind": "money",
                "amount": {"kind": "decimal", "value": "600.00"},
                "currency": "USD"
            },
            "net_contribution": {
                "kind": "money",
                "amount": {"kind": "decimal", "value": "400.00"},
                "currency": "USD"
            },
            "margin_per_unit": {
                "kind": "money",
                "amount": {"kind": "decimal", "value": "6.00"},
                "currency": "USD"
            }
        }))),
    ])
}

fn invoke_contribution(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let currency = args.text("currency")?;
    let (revenue, revenue_currency) = arg_money(args, "revenue")?;
    require_currency(currency, &revenue_currency)?;
    let refunds = match optional_arg_money(args, "refunds")? {
        Some((amount, found)) => {
            require_currency(currency, &found)?;
            amount
        }
        None => Decimal::zero(),
    };
    let variable_costs = match optional_arg_money(args, "variable_costs")? {
        Some((amount, found)) => {
            require_currency(currency, &found)?;
            amount
        }
        None => Decimal::zero(),
    };
    let fixed_costs = match optional_arg_money(args, "fixed_costs")? {
        Some((amount, found)) => {
            require_currency(currency, &found)?;
            amount
        }
        None => Decimal::zero(),
    };
    let gross = revenue.sub(&refunds, &ctx.numeric, &ctx.limits)?.sub(
        &variable_costs,
        &ctx.numeric,
        &ctx.limits,
    )?;
    let net = gross.sub(&fixed_costs, &ctx.numeric, &ctx.limits)?;
    let mut rounded = false;
    let mut fields = vec![
        ("gross_contribution", money_value(gross.clone(), currency)),
        ("net_contribution", money_value(net, currency)),
    ];
    if let Some(units) = args.optional_number("units")? {
        let units = decimal_from_number(units, "units")?;
        if !units.is_positive() {
            return Err(EngineError::domain("units must be positive"));
        }
        let per_unit = gross.to_rational() / units.to_rational();
        let (decimal, inexact) = rational_to_decimal(&per_unit, ctx)?;
        let (decimal, changed) = quantize(&decimal, SETTLEMENT_SCALE);
        rounded |= inexact || changed;
        fields.push(("margin_per_unit", money_value(decimal, currency)));
    }
    Ok(Outcome::new(Value::record(fields), exactness(rounded)))
}

pub(crate) fn register() -> Vec<std::sync::Arc<dyn bicmath_core::contract::Function>> {
    use bicmath_core::contract::SimpleFunction;
    vec![
        SimpleFunction::arc(percentage_change_descriptor(), invoke_percentage_change),
        SimpleFunction::arc(markup_descriptor(), invoke_markup),
        SimpleFunction::arc(margin_descriptor(), invoke_margin),
        SimpleFunction::arc(break_even_descriptor(), invoke_break_even),
        SimpleFunction::arc(contribution_descriptor(), invoke_contribution),
    ]
}
