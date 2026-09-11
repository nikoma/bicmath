//! Amortization schedules with exact reconciliation.

use std::cmp::Ordering;

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::Signed;

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, Assumption, CostClass, Example, FunctionDescriptor, Outcome, ParamDescriptor,
};
use bicmath_core::error::EngineError;
use bicmath_core::number::{Decimal, Number, RoundingMode};
use bicmath_core::schema::ValueSchema;
use bicmath_core::value::Value;

use crate::interest::level_payment_minor;
use crate::util::{
    SETTLEMENT_SCALE, all_modes, arg_money, decimal_to_minor, enum_schema, exact_schema, exactness,
    example_args, field, integer_schema, minor_to_decimal, money_field, money_schema, money_value,
    parse_value, positive_periods, quantize, rational_minor_to_decimal, record_schema,
    require_currency, round_rational_to_bigint, text_schema,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum RoundingKind {
    PerPeriod,
    FinalOnly,
}

struct ScheduleResult {
    rows: Vec<Value>,
    total_payments: Decimal,
    total_principal: Decimal,
    total_interest: Decimal,
    final_adjustment: Decimal,
    rounded: bool,
}

fn schedule_row(
    period: u32,
    payment: Decimal,
    interest: Decimal,
    principal: Decimal,
    balance: Decimal,
    currency: &str,
) -> Value {
    Value::record([
        ("period", Value::integer(BigInt::from(period))),
        ("payment", money_value(payment, currency)),
        ("interest", money_value(interest, currency)),
        ("principal", money_value(principal, currency)),
        ("balance", money_value(balance, currency)),
    ])
}

fn per_period_schedule(
    principal_minor: &BigInt,
    rate: &BigRational,
    periods: u32,
    due: bool,
    currency: &str,
    ctx: &ExecContext,
) -> Result<ScheduleResult, EngineError> {
    let payment_exact = level_payment_minor(principal_minor, rate, periods, due)?;
    let payment_minor = round_rational_to_bigint(&payment_exact, RoundingMode::HalfEven);
    let mut rounded = BigRational::from_integer(payment_minor.clone()) != payment_exact;
    let mut balance = principal_minor.clone();
    let mut rows = Vec::with_capacity(periods as usize);
    let mut total_payments = Decimal::zero();
    let mut total_principal = Decimal::zero();
    let mut last_payment_minor = payment_minor.clone();
    for index in 0..periods {
        ctx.check()?;
        let interest_rational = BigRational::from_integer(balance.clone()) * rate;
        let interest_minor = round_rational_to_bigint(&interest_rational, RoundingMode::HalfEven);
        if BigRational::from_integer(interest_minor.clone()) != interest_rational {
            rounded = true;
        }
        let (payment_minor_i, principal_minor_i) = if index + 1 == periods {
            let payment = &balance + &interest_minor;
            (payment, balance.clone())
        } else {
            let principal = &payment_minor - &interest_minor;
            (payment_minor.clone(), principal)
        };
        balance = &balance - &principal_minor_i;
        last_payment_minor = payment_minor_i.clone();
        let payment = minor_to_decimal(&payment_minor_i, SETTLEMENT_SCALE);
        let interest = minor_to_decimal(&interest_minor, SETTLEMENT_SCALE);
        let principal = minor_to_decimal(&principal_minor_i, SETTLEMENT_SCALE);
        let balance_decimal = minor_to_decimal(&balance, SETTLEMENT_SCALE);
        total_payments = total_payments.add(&payment, &ctx.numeric, &ctx.limits)?;
        total_principal = total_principal.add(&principal, &ctx.numeric, &ctx.limits)?;
        rows.push(schedule_row(
            index + 1,
            payment,
            interest,
            principal,
            balance_decimal,
            currency,
        ));
    }
    let adjustment_minor = &last_payment_minor - &payment_minor;
    let final_adjustment = minor_to_decimal(&adjustment_minor, SETTLEMENT_SCALE);
    let total_interest = total_payments.sub(&total_principal, &ctx.numeric, &ctx.limits)?;
    Ok(ScheduleResult {
        rows,
        total_payments,
        total_principal,
        total_interest,
        final_adjustment,
        rounded,
    })
}

fn final_only_schedule(
    principal_minor: &BigInt,
    rate: &BigRational,
    periods: u32,
    due: bool,
    currency: &str,
    ctx: &ExecContext,
) -> Result<ScheduleResult, EngineError> {
    let payment_exact = level_payment_minor(principal_minor, rate, periods, due)?;
    let mut balance = BigRational::from_integer(principal_minor.clone());
    let mut rows = Vec::with_capacity(periods as usize);
    let mut total_payments = Decimal::zero();
    let mut total_principal = Decimal::zero();
    let mut rounded = false;
    for index in 0..periods.saturating_sub(1) {
        ctx.check()?;
        let interest = &balance * rate;
        let principal = &payment_exact - &interest;
        balance = &balance - &principal;
        let (payment, payment_inexact) =
            rational_minor_to_decimal(&payment_exact, SETTLEMENT_SCALE, ctx)?;
        let (interest, interest_inexact) =
            rational_minor_to_decimal(&interest, SETTLEMENT_SCALE, ctx)?;
        let (principal, principal_inexact) =
            rational_minor_to_decimal(&principal, SETTLEMENT_SCALE, ctx)?;
        let (balance_decimal, balance_inexact) =
            rational_minor_to_decimal(&balance, SETTLEMENT_SCALE, ctx)?;
        rounded |= payment_inexact || interest_inexact || principal_inexact || balance_inexact;
        total_payments = total_payments.add(&payment, &ctx.numeric, &ctx.limits)?;
        total_principal = total_principal.add(&principal, &ctx.numeric, &ctx.limits)?;
        rows.push(schedule_row(
            index + 1,
            payment,
            interest,
            principal,
            balance_decimal,
            currency,
        ));
    }
    let interest_last = &balance * rate;
    let (interest_last, interest_inexact) =
        rational_minor_to_decimal(&interest_last, SETTLEMENT_SCALE, ctx)?;
    rounded |= interest_inexact;
    let principal_total = minor_to_decimal(principal_minor, SETTLEMENT_SCALE);
    let final_principal = principal_total.sub(&total_principal, &ctx.numeric, &ctx.limits)?;
    let final_payment_exact = final_principal.add(&interest_last, &ctx.numeric, &ctx.limits)?;
    let (final_payment, changed) = quantize(&final_payment_exact, SETTLEMENT_SCALE);
    rounded |= changed;
    let final_adjustment = final_payment.sub(&final_payment_exact, &ctx.numeric, &ctx.limits)?;
    total_payments = total_payments.add(&final_payment, &ctx.numeric, &ctx.limits)?;
    total_principal = total_principal.add(&final_principal, &ctx.numeric, &ctx.limits)?;
    rows.push(schedule_row(
        periods,
        final_payment,
        interest_last,
        final_principal,
        Decimal::zero(),
        currency,
    ));
    let total_interest = total_payments.sub(&total_principal, &ctx.numeric, &ctx.limits)?;
    Ok(ScheduleResult {
        rows,
        total_payments,
        total_principal,
        total_interest,
        final_adjustment,
        rounded,
    })
}

fn row_schema() -> ValueSchema {
    record_schema(vec![
        field("period", integer_schema()),
        money_field("payment"),
        money_field("interest"),
        money_field("principal"),
        money_field("balance"),
    ])
}

fn reconciliation_schema() -> ValueSchema {
    record_schema(vec![
        money_field("total_payments"),
        money_field("total_interest"),
        money_field("total_principal"),
        money_field("final_payment_adjustment"),
        field("reconciles", ValueSchema::Bool),
    ])
}

fn amortization_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "finance.amortization",
        "finance",
        "1.0.0",
        "Amortization schedule",
        "Level-payment amortization schedule with exact principal reconciliation.",
    )
    .with_description(
        "Builds a level-payment schedule for principal at the periodic rate annual_rate \
         (a 6% nominal annual rate compounded monthly is passed as 0.005 with 12 \
         periods). payment_timing selects ordinary (default) or annuity_due. rounding \
         selects per_period (default), which rounds each period's interest and payment to \
         the settlement scale of 2, or final_only, which keeps intermediate values at \
         full precision and rounds only the final payment. In both modes the final row is \
         adjusted so the principal column sums EXACTLY to the initial principal and the \
         closing balance is exactly zero; reconciliation reports the totals, the final \
         payment adjustment, and reconciles = true.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("principal", "Loan principal.", money_schema()),
        ParamDescriptor::required(
            "annual_rate",
            "Periodic rate as an exact decimal.",
            exact_schema(),
        ),
        ParamDescriptor::required(
            "periods",
            "Number of periods; at least 1.",
            integer_schema(),
        ),
        ParamDescriptor::required(
            "currency",
            "Currency of the principal; must match.",
            text_schema(),
        ),
        ParamDescriptor::optional(
            "payment_timing",
            "ordinary (default) or annuity_due.",
            enum_schema(&["ordinary", "annuity_due"]),
        ),
        ParamDescriptor::optional(
            "rounding",
            "per_period (default) or final_only.",
            enum_schema(&["per_period", "final_only"]),
        ),
    ])
    .with_output(
        record_schema(vec![
            field("schedule", ValueSchema::array(row_schema())),
            field("reconciliation", reconciliation_schema()),
            field("currency", text_schema()),
            field("annual_rate", exact_schema()),
            field("payment_timing", enum_schema(&["ordinary", "annuity_due"])),
            field("rounding", enum_schema(&["per_period", "final_only"])),
        ]),
        "Schedule rows plus a reconciliation record.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_method_ref("docs/methods/finance.md#amortization")
    .with_examples(vec![Example::new(
        "zero-rate 300 over 3 periods",
        example_args(&[
            (
                "principal",
                serde_json::json!({
                    "kind": "money",
                    "amount": {"kind": "decimal", "value": "300.00"},
                    "currency": "USD"
                }),
            ),
            ("annual_rate", serde_json::json!("0")),
            ("periods", serde_json::json!(3)),
            ("currency", serde_json::json!("USD")),
        ]),
    )
    .with_value(parse_value(serde_json::json!({
        "schedule": [
            {
                "period": {"kind": "integer", "value": "1"},
                "payment": {"kind": "money", "amount": {"kind": "decimal", "value": "100.00"}, "currency": "USD"},
                "interest": {"kind": "money", "amount": {"kind": "decimal", "value": "0.00"}, "currency": "USD"},
                "principal": {"kind": "money", "amount": {"kind": "decimal", "value": "100.00"}, "currency": "USD"},
                "balance": {"kind": "money", "amount": {"kind": "decimal", "value": "200.00"}, "currency": "USD"}
            },
            {
                "period": {"kind": "integer", "value": "2"},
                "payment": {"kind": "money", "amount": {"kind": "decimal", "value": "100.00"}, "currency": "USD"},
                "interest": {"kind": "money", "amount": {"kind": "decimal", "value": "0.00"}, "currency": "USD"},
                "principal": {"kind": "money", "amount": {"kind": "decimal", "value": "100.00"}, "currency": "USD"},
                "balance": {"kind": "money", "amount": {"kind": "decimal", "value": "100.00"}, "currency": "USD"}
            },
            {
                "period": {"kind": "integer", "value": "3"},
                "payment": {"kind": "money", "amount": {"kind": "decimal", "value": "100.00"}, "currency": "USD"},
                "interest": {"kind": "money", "amount": {"kind": "decimal", "value": "0.00"}, "currency": "USD"},
                "principal": {"kind": "money", "amount": {"kind": "decimal", "value": "100.00"}, "currency": "USD"},
                "balance": {"kind": "money", "amount": {"kind": "decimal", "value": "0.00"}, "currency": "USD"}
            }
        ],
        "reconciliation": {
            "total_payments": {"kind": "money", "amount": {"kind": "decimal", "value": "300.00"}, "currency": "USD"},
            "total_interest": {"kind": "money", "amount": {"kind": "decimal", "value": "0.00"}, "currency": "USD"},
            "total_principal": {"kind": "money", "amount": {"kind": "decimal", "value": "300.00"}, "currency": "USD"},
            "final_payment_adjustment": {"kind": "money", "amount": {"kind": "decimal", "value": "0.00"}, "currency": "USD"},
            "reconciles": true
        },
        "currency": "USD",
        "annual_rate": {"kind": "decimal", "value": "0"},
        "payment_timing": "ordinary",
        "rounding": "per_period"
    })))])
}

fn invoke_amortization(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let (principal, principal_currency) = arg_money(args, "principal")?;
    let currency = args.text("currency")?;
    require_currency(currency, &principal_currency)?;
    let rate = args.decimal("annual_rate")?;
    let periods = positive_periods(args, "periods", ctx)?;
    let due = match args.optional_text("payment_timing")? {
        None | Some("ordinary") => false,
        Some("annuity_due") => true,
        Some(other) => {
            return Err(EngineError::domain(format!(
                "unknown payment_timing {other:?}; expected ordinary or annuity_due"
            )));
        }
    };
    let rounding = match args.optional_text("rounding")? {
        None | Some("per_period") => RoundingKind::PerPeriod,
        Some("final_only") => RoundingKind::FinalOnly,
        Some(other) => {
            return Err(EngineError::domain(format!(
                "unknown rounding {other:?}; expected per_period or final_only"
            )));
        }
    };
    let principal_minor = decimal_to_minor(&principal, SETTLEMENT_SCALE)?;
    if !principal_minor.is_positive() {
        return Err(EngineError::domain("principal must be positive"));
    }
    let rate_rational = rate.to_rational();
    if rate_rational <= BigRational::from_integer(BigInt::from(-1)) {
        return Err(EngineError::domain(
            "annual_rate must be greater than -1 for a level-payment schedule",
        ));
    }
    let schedule = match rounding {
        RoundingKind::PerPeriod => per_period_schedule(
            &principal_minor,
            &rate_rational,
            periods,
            due,
            currency,
            ctx,
        )?,
        RoundingKind::FinalOnly => final_only_schedule(
            &principal_minor,
            &rate_rational,
            periods,
            due,
            currency,
            ctx,
        )?,
    };
    let reconciles = schedule.total_principal.numeric_cmp(&principal) == Ordering::Equal;
    let reconciliation = Value::record([
        (
            "total_payments",
            money_value(schedule.total_payments, currency),
        ),
        (
            "total_interest",
            money_value(schedule.total_interest, currency),
        ),
        (
            "total_principal",
            money_value(schedule.total_principal, currency),
        ),
        (
            "final_payment_adjustment",
            money_value(schedule.final_adjustment, currency),
        ),
        ("reconciles", Value::Bool(reconciles)),
    ]);
    let value = Value::record([
        ("schedule", Value::Array(schedule.rows)),
        ("reconciliation", reconciliation),
        ("currency", Value::text(currency)),
        ("annual_rate", Value::Number(Number::Decimal(rate))),
        (
            "payment_timing",
            Value::text(if due { "annuity_due" } else { "ordinary" }),
        ),
        (
            "rounding",
            Value::text(match rounding {
                RoundingKind::PerPeriod => "per_period",
                RoundingKind::FinalOnly => "final_only",
            }),
        ),
    ]);
    Ok(
        Outcome::new(value, exactness(schedule.rounded)).with_assumption(
            Assumption::user_supplied(
                "annual_rate_is_periodic",
                "annual_rate is the rate per period, not an annual rate divided by the schedule",
            ),
        ),
    )
}

pub(crate) fn register() -> Vec<std::sync::Arc<dyn bicmath_core::contract::Function>> {
    use bicmath_core::contract::SimpleFunction;
    vec![SimpleFunction::arc(
        amortization_descriptor(),
        invoke_amortization,
    )]
}
