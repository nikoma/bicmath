//! Simple and compound interest, present/future value, and level payments.

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{Signed, ToPrimitive, Zero};

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, Assumption, CostClass, Example, FunctionDescriptor, Outcome, ParamDescriptor,
};
use bicmath_core::error::EngineError;
use bicmath_core::number::Decimal;
use bicmath_core::value::Value;

use crate::util::{
    SETTLEMENT_SCALE, all_modes, arg_money, compounding_frequency, decimal_to_minor, enum_schema,
    exact_schema, exactness, example_args, integer_schema, money_field, money_schema, money_value,
    parse_value, positive_periods, quantize, rational_minor_to_decimal, rational_to_decimal,
    record_schema,
};

/// Compute a compounded amount, either growing (`invert = false`) or
/// discounting (`invert = true`).
///
/// When `compounds_per_year * years` is an integer the computation is exact in
/// rational arithmetic; otherwise binary64 is used explicitly and the result is
/// marked rounded by the caller.
fn compound_amount(
    principal: &Decimal,
    rate: &Decimal,
    years: &Decimal,
    compounds: u32,
    invert: bool,
    ctx: &ExecContext,
) -> Result<(Decimal, bool), EngineError> {
    let principal_rational = principal.to_rational();
    let rate_rational = rate.to_rational();
    let years_rational = years.to_rational();
    let n = BigRational::from_integer(BigInt::from(compounds));
    let base = BigRational::from_integer(BigInt::from(1)) + &rate_rational / &n;
    let exponent = &years_rational * &n;
    if exponent.is_integer()
        && let Some(exponent) = exponent.to_integer().to_i32()
        && exponent.unsigned_abs() <= 1_000_000
    {
        let factor = base.pow(exponent);
        if factor.is_zero() {
            return Err(EngineError::domain(
                "the compounding factor is zero; the amount cannot be discounted",
            ));
        }
        let result = if invert {
            principal_rational / factor
        } else {
            principal_rational * factor
        };
        let (decimal, inexact) = rational_to_decimal(&result, ctx)?;
        return Ok((decimal, inexact));
    }
    let base_f = base
        .to_f64()
        .ok_or_else(|| EngineError::domain("compounding base is not representable as float64"))?;
    let exponent_f = exponent.to_f64().ok_or_else(|| {
        EngineError::domain("compounding exponent is not representable as float64")
    })?;
    if base_f <= 0.0 {
        return Err(EngineError::domain(
            "the compounding base (1 + rate / compounds_per_year) must be positive",
        ));
    }
    let factor_f = base_f.powf(exponent_f);
    if !factor_f.is_finite() || factor_f == 0.0 {
        return Err(EngineError::domain(
            "the compounding factor is not finite for these inputs",
        ));
    }
    let principal_f = principal
        .to_f64()
        .ok_or_else(|| EngineError::domain("principal is not representable as float64"))?;
    let result_f = if invert {
        principal_f / factor_f
    } else {
        principal_f * factor_f
    };
    if !result_f.is_finite() {
        return Err(EngineError::domain(
            "the compounded value is not finite for these inputs",
        ));
    }
    let decimal = Decimal::from_f64_display(result_f).ok_or_else(|| {
        EngineError::domain("the compounded value is not representable as a decimal")
    })?;
    Ok((decimal, true))
}

fn simple_interest_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "finance.simple_interest",
        "finance",
        "1.0.0",
        "Simple interest",
        "Simple interest on a principal over a number of years.",
    )
    .with_description(
        "interest = principal * annual_rate * years. The interest is rounded to the \
         settlement scale of 2 decimal places (half-even) and the total is the quantized \
         principal plus that interest. No compounding is applied.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("principal", "Principal amount.", money_schema()),
        ParamDescriptor::required(
            "annual_rate",
            "Annual simple rate as an exact decimal (0.05 means 5%).",
            exact_schema(),
        ),
        ParamDescriptor::required(
            "years",
            "Number of years as an exact decimal.",
            exact_schema(),
        ),
    ])
    .with_output(
        record_schema(vec![money_field("interest"), money_field("total")]),
        "Interest and total (principal + interest).",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Constant)
    .with_method_ref("docs/methods/finance.md#simple_interest")
    .with_examples(vec![
        Example::new(
            "5% simple interest for 2 years",
            example_args(&[
                (
                    "principal",
                    serde_json::json!({
                        "kind": "money",
                        "amount": {"kind": "decimal", "value": "1000.00"},
                        "currency": "USD"
                    }),
                ),
                ("annual_rate", serde_json::json!("0.05")),
                ("years", serde_json::json!(2)),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "interest": {
                "kind": "money",
                "amount": {"kind": "decimal", "value": "100.00"},
                "currency": "USD"
            },
            "total": {
                "kind": "money",
                "amount": {"kind": "decimal", "value": "1100.00"},
                "currency": "USD"
            }
        }))),
    ])
}

fn invoke_simple_interest(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let (principal, currency) = arg_money(args, "principal")?;
    let rate = args.decimal("annual_rate")?;
    let years = args.decimal("years")?;
    let interest_rational = principal.to_rational() * rate.to_rational() * years.to_rational();
    let (interest_decimal, inexact) = rational_to_decimal(&interest_rational, ctx)?;
    let (interest, changed) = quantize(&interest_decimal, SETTLEMENT_SCALE);
    let (principal, principal_changed) = quantize(&principal, SETTLEMENT_SCALE);
    let total = principal.add(&interest, &ctx.numeric, &ctx.limits)?;
    let value = Value::record([
        ("interest", money_value(interest, &currency)),
        ("total", money_value(total, &currency)),
    ]);
    Ok(Outcome::new(
        value,
        exactness(inexact || changed || principal_changed),
    ))
}

fn compound_interest_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "finance.compound_interest",
        "finance",
        "1.0.0",
        "Compound interest",
        "Compound interest on a principal with a nominal annual rate.",
    )
    .with_description(
        "total = principal * (1 + annual_rate / compounds_per_year) ^ \
         (compounds_per_year * years). The annual_rate is a nominal annual rate whose \
         units must agree with compounds_per_year: a 6% nominal annual rate compounded \
         monthly is passed as 0.06 with compounds_per_year = 12, giving a per-period rate \
         of 0.005. The total is rounded to settlement scale 2 (half-even); when the \
         exponent is not a whole number the computation uses binary64 explicitly and is \
         reported rounded.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("principal", "Principal amount.", money_schema()),
        ParamDescriptor::required(
            "annual_rate",
            "Nominal annual rate as an exact decimal (0.06 means 6%).",
            exact_schema(),
        ),
        ParamDescriptor::required(
            "years",
            "Number of years as an exact decimal.",
            exact_schema(),
        ),
        ParamDescriptor::required(
            "compounds_per_year",
            "Compounding periods per year; at least 1.",
            integer_schema(),
        ),
    ])
    .with_output(
        record_schema(vec![money_field("interest"), money_field("total")]),
        "Interest and total (principal + interest).",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Iterative)
    .with_method_ref("docs/methods/finance.md#compound_interest")
    .with_examples(vec![
        Example::new(
            "5% compounded annually for 2 years",
            example_args(&[
                (
                    "principal",
                    serde_json::json!({
                        "kind": "money",
                        "amount": {"kind": "decimal", "value": "1000.00"},
                        "currency": "USD"
                    }),
                ),
                ("annual_rate", serde_json::json!("0.05")),
                ("years", serde_json::json!(2)),
                ("compounds_per_year", serde_json::json!(1)),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "interest": {
                "kind": "money",
                "amount": {"kind": "decimal", "value": "102.50"},
                "currency": "USD"
            },
            "total": {
                "kind": "money",
                "amount": {"kind": "decimal", "value": "1102.50"},
                "currency": "USD"
            }
        }))),
    ])
}

fn invoke_compound_interest(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let (principal, currency) = arg_money(args, "principal")?;
    let rate = args.decimal("annual_rate")?;
    let years = args.decimal("years")?;
    let compounds = compounding_frequency(args, "compounds_per_year")?;
    let (total_decimal, inexact) =
        compound_amount(&principal, &rate, &years, compounds, false, ctx)?;
    let (total, changed) = quantize(&total_decimal, SETTLEMENT_SCALE);
    let (principal, principal_changed) = quantize(&principal, SETTLEMENT_SCALE);
    let interest = total.sub(&principal, &ctx.numeric, &ctx.limits)?;
    let value = Value::record([
        ("interest", money_value(interest, &currency)),
        ("total", money_value(total, &currency)),
    ]);
    Ok(Outcome::new(
        value,
        exactness(inexact || changed || principal_changed),
    ))
}

fn present_value_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "finance.present_value",
        "finance",
        "1.0.0",
        "Present value",
        "Discount a future money amount to today.",
    )
    .with_description(
        "present_value = future_value / (1 + annual_rate / compounds_per_year) ^ \
         (compounds_per_year * years). The annual_rate is a nominal annual rate whose \
         units must agree with compounds_per_year. The result is rounded to settlement \
         scale 2 (half-even).",
    )
    .with_parameters(vec![
        ParamDescriptor::required("future_value", "Future amount.", money_schema()),
        ParamDescriptor::required(
            "annual_rate",
            "Nominal annual rate as an exact decimal.",
            exact_schema(),
        ),
        ParamDescriptor::required(
            "years",
            "Number of years as an exact decimal.",
            exact_schema(),
        ),
        ParamDescriptor::optional(
            "compounds_per_year",
            "Compounding periods per year; default 1.",
            integer_schema(),
        ),
    ])
    .with_output(
        money_schema(),
        "Present value rounded to settlement scale 2.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Iterative)
    .with_method_ref("docs/methods/finance.md#present_value")
    .with_examples(vec![
        Example::new(
            "discount 1102.50 at 5% for 2 years",
            example_args(&[
                (
                    "future_value",
                    serde_json::json!({
                        "kind": "money",
                        "amount": {"kind": "decimal", "value": "1102.50"},
                        "currency": "USD"
                    }),
                ),
                ("annual_rate", serde_json::json!("0.05")),
                ("years", serde_json::json!(2)),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "kind": "money",
            "amount": {"kind": "decimal", "value": "1000.00"},
            "currency": "USD"
        }))),
    ])
}

fn invoke_present_value(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let (future_value, currency) = arg_money(args, "future_value")?;
    let rate = args.decimal("annual_rate")?;
    let years = args.decimal("years")?;
    let compounds = compounding_frequency(args, "compounds_per_year")?;
    let (present_decimal, inexact) =
        compound_amount(&future_value, &rate, &years, compounds, true, ctx)?;
    let (present, changed) = quantize(&present_decimal, SETTLEMENT_SCALE);
    Ok(Outcome::new(
        money_value(present, &currency),
        exactness(inexact || changed),
    )
    .with_assumption(Assumption::checked(
        "rate_frequency_agreement",
        "annual_rate is nominal and is divided by compounds_per_year to obtain the per-period rate",
    )))
}

fn future_value_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "finance.future_value",
        "finance",
        "1.0.0",
        "Future value",
        "Compound a present money amount forward.",
    )
    .with_description(
        "future_value = present_value * (1 + annual_rate / compounds_per_year) ^ \
         (compounds_per_year * years). The annual_rate is a nominal annual rate whose \
         units must agree with compounds_per_year. The result is rounded to settlement \
         scale 2 (half-even).",
    )
    .with_parameters(vec![
        ParamDescriptor::required("present_value", "Present amount.", money_schema()),
        ParamDescriptor::required(
            "annual_rate",
            "Nominal annual rate as an exact decimal.",
            exact_schema(),
        ),
        ParamDescriptor::required(
            "years",
            "Number of years as an exact decimal.",
            exact_schema(),
        ),
        ParamDescriptor::optional(
            "compounds_per_year",
            "Compounding periods per year; default 1.",
            integer_schema(),
        ),
    ])
    .with_output(
        money_schema(),
        "Future value rounded to settlement scale 2.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Iterative)
    .with_method_ref("docs/methods/finance.md#future_value")
    .with_examples(vec![
        Example::new(
            "grow 1000.00 at 5% for 2 years",
            example_args(&[
                (
                    "present_value",
                    serde_json::json!({
                        "kind": "money",
                        "amount": {"kind": "decimal", "value": "1000.00"},
                        "currency": "USD"
                    }),
                ),
                ("annual_rate", serde_json::json!("0.05")),
                ("years", serde_json::json!(2)),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "kind": "money",
            "amount": {"kind": "decimal", "value": "1102.50"},
            "currency": "USD"
        }))),
    ])
}

fn invoke_future_value(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let (present_value, currency) = arg_money(args, "present_value")?;
    let rate = args.decimal("annual_rate")?;
    let years = args.decimal("years")?;
    let compounds = compounding_frequency(args, "compounds_per_year")?;
    let (future_decimal, inexact) =
        compound_amount(&present_value, &rate, &years, compounds, false, ctx)?;
    let (future, changed) = quantize(&future_decimal, SETTLEMENT_SCALE);
    Ok(Outcome::new(
        money_value(future, &currency),
        exactness(inexact || changed),
    )
    .with_assumption(Assumption::checked(
        "rate_frequency_agreement",
        "annual_rate is nominal and is divided by compounds_per_year to obtain the per-period rate",
    )))
}

/// Level payment in minor units for a present value in minor units.
pub(crate) fn level_payment_minor(
    present_minor: &BigInt,
    rate: &BigRational,
    periods: u32,
    due: bool,
) -> Result<BigRational, EngineError> {
    if periods == 0 {
        return Err(EngineError::domain("periods must be at least 1"));
    }
    if rate.is_zero() {
        return Ok(BigRational::new(
            present_minor.clone(),
            BigInt::from(periods),
        ));
    }
    let one = BigRational::from_integer(BigInt::from(1));
    let one_plus_rate = &one + rate;
    if !one_plus_rate.is_positive() {
        return Err(EngineError::domain(
            "annual_rate must be greater than -1 for a level payment",
        ));
    }
    let periods_i32 = i32::try_from(periods)
        .map_err(|_| EngineError::domain("periods is too large to discount exactly"))?;
    let growth = one_plus_rate.pow(periods_i32);
    let annuity_factor = &one - &one / &growth;
    let mut payment = BigRational::from_integer(present_minor.clone()) * rate / annuity_factor;
    if due {
        payment /= one_plus_rate;
    }
    Ok(payment)
}

fn payment_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "finance.payment",
        "finance",
        "1.0.0",
        "Level payment",
        "Level payment for a present value over a number of periods.",
    )
    .with_description(
        "Computes the level payment that amortizes present_value over periods at the \
         periodic rate annual_rate: payment = PV * r / (1 - (1 + r)^-n). \
         annual_rate is the rate per period (a 6% nominal annual rate compounded monthly \
         is passed as 0.005 with 12 periods). timing selects an ordinary annuity \
         (payment at period end, default) or an annuity due (payment at period start, \
         payment divided by (1 + r)). At a zero rate the payment is present_value / \
         periods, rounded to settlement scale 2 when it does not divide evenly.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("present_value", "Present value.", money_schema()),
        ParamDescriptor::required(
            "annual_rate",
            "Periodic rate as an exact decimal.",
            exact_schema(),
        ),
        ParamDescriptor::required(
            "periods",
            "Number of payment periods; at least 1.",
            integer_schema(),
        ),
        ParamDescriptor::optional(
            "timing",
            "ordinary (default) or annuity_due.",
            enum_schema(&["ordinary", "annuity_due"]),
        ),
    ])
    .with_output(
        money_schema(),
        "Level payment rounded to settlement scale 2.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Iterative)
    .with_method_ref("docs/methods/finance.md#payment")
    .with_examples(vec![
        Example::new(
            "monthly payment on 1200 at 6%/12 for 12 months",
            example_args(&[
                (
                    "present_value",
                    serde_json::json!({
                        "kind": "money",
                        "amount": {"kind": "decimal", "value": "1200.00"},
                        "currency": "USD"
                    }),
                ),
                ("annual_rate", serde_json::json!("0.005")),
                ("periods", serde_json::json!(12)),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "kind": "money",
            "amount": {"kind": "decimal", "value": "103.28"},
            "currency": "USD"
        }))),
        Example::new(
            "zero-rate payment divides evenly",
            example_args(&[
                (
                    "present_value",
                    serde_json::json!({
                        "kind": "money",
                        "amount": {"kind": "decimal", "value": "1200.00"},
                        "currency": "USD"
                    }),
                ),
                ("annual_rate", serde_json::json!("0")),
                ("periods", serde_json::json!(12)),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "kind": "money",
            "amount": {"kind": "decimal", "value": "100.00"},
            "currency": "USD"
        }))),
    ])
}

fn invoke_payment(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let (present_value, currency) = arg_money(args, "present_value")?;
    let rate = args.decimal("annual_rate")?;
    let periods = positive_periods(args, "periods", ctx)?;
    let due = match args.optional_text("timing")? {
        None | Some("ordinary") => false,
        Some("annuity_due") => true,
        Some(other) => {
            return Err(EngineError::domain(format!(
                "unknown timing {other:?}; expected ordinary or annuity_due"
            )));
        }
    };
    let present_minor = decimal_to_minor(&present_value, SETTLEMENT_SCALE)?;
    let payment_rational = level_payment_minor(&present_minor, &rate.to_rational(), periods, due)?;
    let (payment_decimal, inexact) =
        rational_minor_to_decimal(&payment_rational, SETTLEMENT_SCALE, ctx)?;
    let (payment, changed) = quantize(&payment_decimal, SETTLEMENT_SCALE);
    Ok(Outcome::new(
        money_value(payment, &currency),
        exactness(inexact || changed),
    ))
}

pub(crate) fn register() -> Vec<std::sync::Arc<dyn bicmath_core::contract::Function>> {
    use bicmath_core::contract::SimpleFunction;
    vec![
        SimpleFunction::arc(simple_interest_descriptor(), invoke_simple_interest),
        SimpleFunction::arc(compound_interest_descriptor(), invoke_compound_interest),
        SimpleFunction::arc(present_value_descriptor(), invoke_present_value),
        SimpleFunction::arc(future_value_descriptor(), invoke_future_value),
        SimpleFunction::arc(payment_descriptor(), invoke_payment),
    ]
}
