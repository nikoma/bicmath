//! Bond pricing, yield solving, and interest-rate risk measures.
//!
//! Coupons and yields follow the per-period convention: with frequency `f`, the
//! per-period coupon is `face_value * coupon_rate / f` and the per-period
//! discount rate is `yield_rate / f`. Bond prices and durations use exact
//! decimal arithmetic where the division terminates and are reported rounded
//! otherwise; the price is deliberately not quantized to settlement scale
//! because bond prices are quoted with fractions of a cent.

use num_bigint::BigInt;
use num_traits::ToPrimitive;

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, CostClass, Example, FunctionDescriptor, Outcome, ParamDescriptor,
};
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::number::Decimal;
use bicmath_core::schema::ValueSchema;
use bicmath_core::value::Value;

use crate::cashflow::brent;
use crate::util::{
    SETTLEMENT_SCALE, all_modes, arg_money, compounding_frequency, exact_schema, exactness,
    example_args, field, integer_schema, money_field, money_schema, money_value, parse_value,
    positive_periods, quantize, record_schema, require_currency, text_schema,
};

/// Per-period data shared by the bond price and duration computations.
struct BondTerms {
    face: Decimal,
    coupon: Decimal,
    base: Decimal,
    rounded: bool,
}

fn bond_terms(
    face: Decimal,
    coupon_rate: &Decimal,
    yield_rate: &Decimal,
    frequency: u32,
    ctx: &ExecContext,
) -> Result<BondTerms, EngineError> {
    let periods_per_year = Decimal::from_bigint(BigInt::from(frequency));
    let (coupon, coupon_inexact) = face.mul(coupon_rate, &ctx.numeric, &ctx.limits)?.div(
        &periods_per_year,
        &ctx.numeric,
        &ctx.limits,
    )?;
    let (per_yield, yield_inexact) =
        yield_rate.div(&periods_per_year, &ctx.numeric, &ctx.limits)?;
    let one = Decimal::from_bigint(BigInt::from(1));
    let base = one.add(&per_yield, &ctx.numeric, &ctx.limits)?;
    if !base.is_positive() {
        return Err(EngineError::domain(
            "the per-period discount base (1 + yield_rate / frequency) must be positive",
        ));
    }
    Ok(BondTerms {
        face,
        coupon,
        base,
        rounded: coupon_inexact || yield_inexact,
    })
}

fn discounted_price(
    terms: &BondTerms,
    periods: u32,
    ctx: &ExecContext,
) -> Result<(Decimal, Decimal, bool), EngineError> {
    let one = Decimal::from_bigint(BigInt::from(1));
    let mut discount = one;
    let mut price = Decimal::zero();
    let mut rounded = terms.rounded;
    for period in 1..=periods {
        if period % 1024 == 0 {
            ctx.check()?;
        }
        let (next, inexact) = discount.div(&terms.base, &ctx.numeric, &ctx.limits)?;
        discount = next;
        rounded |= inexact;
        let cash_flow = terms.coupon.mul(&discount, &ctx.numeric, &ctx.limits)?;
        price = price.add(&cash_flow, &ctx.numeric, &ctx.limits)?;
    }
    let face_present = terms.face.mul(&discount, &ctx.numeric, &ctx.limits)?;
    price = price.add(&face_present, &ctx.numeric, &ctx.limits)?;
    Ok((price, discount, rounded))
}

fn bond_price_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "finance.bond_price",
        "finance",
        "1.0.0",
        "Bond price",
        "Dirty price of a fixed-coupon bond from face value, coupon rate, periods, and yield.",
    )
    .with_description(
        "With frequency f, the coupon per period is face_value * coupon_rate / f and the \
         per-period discount rate is yield_rate / f, so the price is \
         sum(coupon / (1 + y/f)^t) + face_value / (1 + y/f)^periods for t = 1..=periods. \
         frequency defaults to 1. The price is a money amount in the currency of \
         face_value and keeps full decimal precision (bond prices are quoted with \
         fractions of a cent); total_coupons is the undiscounted coupon sum, quantized to \
         settlement scale 2. Arithmetic is exact decimal when every division terminates \
         and rounded otherwise.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("face_value", "Face (redemption) value.", money_schema()),
        ParamDescriptor::required(
            "coupon_rate",
            "Coupon rate per period as an exact decimal (0.05 means 5%).",
            exact_schema(),
        ),
        ParamDescriptor::required(
            "periods",
            "Number of coupon periods; at least 1.",
            integer_schema(),
        ),
        ParamDescriptor::required(
            "yield_rate",
            "Yield per period as an exact decimal.",
            exact_schema(),
        ),
        ParamDescriptor::optional(
            "frequency",
            "Coupon payments per year; default 1.",
            integer_schema(),
        ),
    ])
    .with_output(
        record_schema(vec![
            money_field("price"),
            money_field("total_coupons"),
            field("method", text_schema()),
        ]),
        "Bond price, total undiscounted coupons, and method.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_units_rule(
        "all money amounts share the currency of face_value; rates are per-period decimals",
    )
    .with_method_ref("docs/methods/finance.md#bond_price")
    .with_examples(vec![
        Example::new(
            "zero yield price is face plus all coupons",
            example_args(&[
                (
                    "face_value",
                    serde_json::json!({
                        "kind": "money",
                        "amount": {"kind": "decimal", "value": "1000.00"},
                        "currency": "USD"
                    }),
                ),
                ("coupon_rate", serde_json::json!("0.05")),
                ("periods", serde_json::json!(3)),
                ("yield_rate", serde_json::json!("0")),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "price": {
                "kind": "money",
                "amount": {"kind": "decimal", "value": "1150"},
                "currency": "USD"
            },
            "total_coupons": {
                "kind": "money",
                "amount": {"kind": "decimal", "value": "150.00"},
                "currency": "USD"
            },
            "method": "bond_price"
        }))),
        Example::new(
            "five percent coupon at four percent yield",
            example_args(&[
                (
                    "face_value",
                    serde_json::json!({
                        "kind": "money",
                        "amount": {"kind": "decimal", "value": "1000.00"},
                        "currency": "USD"
                    }),
                ),
                ("coupon_rate", serde_json::json!("0.05")),
                ("periods", serde_json::json!(3)),
                ("yield_rate", serde_json::json!("0.04")),
            ]),
        )
        .with_contains("bond_price"),
    ])
}

fn invoke_bond_price(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let (face, currency) = arg_money(args, "face_value")?;
    let coupon_rate = args.decimal("coupon_rate")?;
    let periods = positive_periods(args, "periods", ctx)?;
    let yield_rate = args.decimal("yield_rate")?;
    let frequency = compounding_frequency(args, "frequency")?;
    let terms = bond_terms(face, &coupon_rate, &yield_rate, frequency, ctx)?;
    let (price, _discount, mut rounded) = discounted_price(&terms, periods, ctx)?;
    let total = terms.coupon.mul(
        &Decimal::from_bigint(BigInt::from(periods)),
        &ctx.numeric,
        &ctx.limits,
    )?;
    let (total, total_changed) = quantize(&total, SETTLEMENT_SCALE);
    rounded |= total_changed;
    let value = Value::record([
        ("price", money_value(price, &currency)),
        ("total_coupons", money_value(total, &currency)),
        ("method", Value::text("bond_price")),
    ]);
    Ok(Outcome::new(value, exactness(rounded)))
}

fn price_at_yield(face: f64, coupon: f64, periods: u32, per_period_yield: f64) -> f64 {
    let base = 1.0 + per_period_yield;
    if base <= 0.0 {
        return f64::NAN;
    }
    let mut discount = 1.0f64;
    let mut price = 0.0f64;
    for _ in 0..periods {
        discount /= base;
        price += coupon * discount;
    }
    price += face * discount;
    if price.is_finite() { price } else { f64::NAN }
}

fn bond_yield_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "finance.bond_yield",
        "finance",
        "1.0.0",
        "Bond yield",
        "Yield to maturity implied by a bond price, solved with a bracketed method.",
    )
    .with_description(
        "Solves for the per-period yield y such that the bond price equals the supplied \
         money price. The price function is strictly decreasing in y, so a bracket is \
         found by scanning and then refined with bisection and Brent's method. The \
         returned yield is per period and must be multiplied by frequency to obtain the \
         annual yield. tolerance defaults to 1e-12 and max_iterations to 200 (capped by \
         the execution context). If no bracket exists or the iteration budget is \
         exhausted, the function returns non_convergence rather than a guess.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("face_value", "Face (redemption) value.", money_schema()),
        ParamDescriptor::required(
            "coupon_rate",
            "Coupon rate per period as an exact decimal.",
            exact_schema(),
        ),
        ParamDescriptor::required(
            "periods",
            "Number of coupon periods; at least 1.",
            integer_schema(),
        ),
        ParamDescriptor::required(
            "price",
            "Observed dirty price as money in the currency of face_value.",
            money_schema(),
        ),
        ParamDescriptor::optional(
            "frequency",
            "Coupon payments per year; default 1.",
            integer_schema(),
        ),
        ParamDescriptor::optional(
            "tolerance",
            "Absolute yield tolerance; default 1e-12.",
            exact_schema(),
        ),
        ParamDescriptor::optional(
            "max_iterations",
            "Maximum solver iterations; default 200.",
            integer_schema(),
        ),
    ])
    .with_output(
        record_schema(vec![
            field("yield", exact_schema()),
            field("iterations", integer_schema()),
            field("converged", ValueSchema::Bool),
            field("method", text_schema()),
        ]),
        "Per-period yield, iterations used, convergence flag, and method.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Iterative)
    .with_units_rule("face_value and price must share one currency; yield is per period")
    .with_method_ref("docs/methods/finance.md#bond_yield")
    .with_examples(vec![
        Example::new(
            "at par yield equals the coupon rate",
            example_args(&[
                (
                    "face_value",
                    serde_json::json!({
                        "kind": "money",
                        "amount": {"kind": "decimal", "value": "1000.00"},
                        "currency": "USD"
                    }),
                ),
                ("coupon_rate", serde_json::json!("0.05")),
                ("periods", serde_json::json!(3)),
                (
                    "price",
                    serde_json::json!({
                        "kind": "money",
                        "amount": {"kind": "decimal", "value": "1000.00"},
                        "currency": "USD"
                    }),
                ),
            ]),
        )
        .with_contains("bond_yield_bisection_brent"),
        Example::new(
            "zero price has no yield",
            example_args(&[
                (
                    "face_value",
                    serde_json::json!({
                        "kind": "money",
                        "amount": {"kind": "decimal", "value": "1000.00"},
                        "currency": "USD"
                    }),
                ),
                ("coupon_rate", serde_json::json!("0.05")),
                ("periods", serde_json::json!(3)),
                (
                    "price",
                    serde_json::json!({
                        "kind": "money",
                        "amount": {"kind": "decimal", "value": "0"},
                        "currency": "USD"
                    }),
                ),
            ]),
        )
        .with_error(ErrorCode::NonConvergence),
    ])
}

fn invoke_bond_yield(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let (face, face_currency) = arg_money(args, "face_value")?;
    let coupon_rate = args.decimal("coupon_rate")?;
    let periods = positive_periods(args, "periods", ctx)?;
    let (price, price_currency) = arg_money(args, "price")?;
    require_currency(&face_currency, &price_currency)?;
    let frequency = compounding_frequency(args, "frequency")?;
    let tolerance = match args.optional_decimal("tolerance")? {
        None => 1e-12,
        Some(value) => value
            .to_f64()
            .ok_or_else(|| EngineError::domain("tolerance is not representable as float64"))?,
    };
    if !tolerance.is_finite() || tolerance <= 0.0 {
        return Err(EngineError::domain(
            "tolerance must be a positive finite number",
        ));
    }
    let requested = match args.optional_integer("max_iterations")? {
        None => 200u32,
        Some(value) => value.to_u32().ok_or_else(|| {
            EngineError::domain("max_iterations must be a positive 32-bit integer")
        })?,
    };
    if requested == 0 {
        return Err(EngineError::domain("max_iterations must be at least 1"));
    }
    let cap = ctx.limits.max_iterations.min(u32::MAX as u64) as u32;
    let max_iterations = requested.min(cap).max(1);
    let work = (periods as u64).saturating_mul(max_iterations as u64);
    if work > ctx.limits.max_operations {
        return Err(EngineError::new(
            ErrorCode::ResourceLimit,
            format!(
                "solving {periods} periods for up to {max_iterations} iterations needs \
                 about {work} price evaluations, exceeding the operation budget of {}",
                ctx.limits.max_operations
            ),
        ));
    }
    let face_f = face
        .to_f64()
        .ok_or_else(|| EngineError::domain("face_value is not representable as float64"))?;
    let coupon_f = coupon_rate
        .to_f64()
        .ok_or_else(|| EngineError::domain("coupon_rate is not representable as float64"))?;
    let price_f = price
        .to_f64()
        .ok_or_else(|| EngineError::domain("price is not representable as float64"))?;
    let per_period_coupon = face_f * coupon_f / f64::from(frequency);
    let target = price_f;
    let objective =
        |yield_rate: f64| price_at_yield(face_f, per_period_coupon, periods, yield_rate) - target;
    let lower = -0.9999f64;
    let lower_value = objective(lower);
    if !lower_value.is_finite() || lower_value <= 0.0 {
        return Err(EngineError::new(
            ErrorCode::NonConvergence,
            "the target price is above the price at the lower yield bound; no yield exists \
             in the searched domain",
        ));
    }
    let mut upper = 0.0f64;
    let mut upper_value = objective(upper);
    if upper_value > 0.0 {
        let mut step = 0.5f64;
        upper = step;
        upper_value = objective(upper);
        while upper_value > 0.0 {
            step *= 2.0;
            upper += step;
            if !upper.is_finite() || upper > 1e12 {
                return Err(EngineError::new(
                    ErrorCode::NonConvergence,
                    "no upper yield bound produced a price at or below the target",
                ));
            }
            upper_value = objective(upper);
        }
    }
    let mut lo = lower;
    let mut hi = upper;
    let mut bisection_iterations = 0u32;
    for _ in 0..64 {
        let mid = 0.5 * (lo + hi);
        if (hi - lo) <= 1e-9 * (1.0 + mid.abs()) {
            break;
        }
        if objective(mid) > 0.0 {
            lo = mid;
        } else {
            hi = mid;
        }
        bisection_iterations += 1;
    }
    let solved = brent(&objective, lo, hi, tolerance, max_iterations).ok_or_else(|| {
        EngineError::new(
            ErrorCode::NonConvergence,
            format!("Brent's method did not converge in the bracket [{lo}, {hi}]"),
        )
    })?;
    let yield_decimal = Decimal::from_f64_display(solved.root)
        .ok_or_else(|| EngineError::domain("the solved yield is not representable as a decimal"))?;
    let value = Value::record([
        ("yield", Value::decimal(yield_decimal)),
        (
            "iterations",
            Value::integer(BigInt::from(bisection_iterations + solved.iterations)),
        ),
        ("converged", Value::Bool(true)),
        ("method", Value::text("bond_yield_bisection_brent")),
    ]);
    Ok(Outcome::rounded(value))
}

fn bond_duration_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "finance.bond_duration",
        "finance",
        "1.0.0",
        "Bond duration and convexity",
        "Macaulay duration, modified duration, and convexity of a fixed-coupon bond.",
    )
    .with_description(
        "Durations are expressed in years. Macaulay duration is the present-value weighted \
         average time to each cash flow: sum((t/f) * PV_t) / price. Modified duration is \
         Macaulay / (1 + y/f) and convexity is \
         sum((t/f) * ((t/f) + 1/f) * PV_t) / (price * (1 + y/f)^2). Coupons follow the \
         per-period convention coupon = face_value * coupon_rate / f and y = yield_rate / f. \
         Arithmetic is exact decimal when every division terminates and rounded otherwise.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("face_value", "Face (redemption) value.", money_schema()),
        ParamDescriptor::required(
            "coupon_rate",
            "Coupon rate per period as an exact decimal.",
            exact_schema(),
        ),
        ParamDescriptor::required(
            "periods",
            "Number of coupon periods; at least 1.",
            integer_schema(),
        ),
        ParamDescriptor::required(
            "yield_rate",
            "Yield per period as an exact decimal.",
            exact_schema(),
        ),
        ParamDescriptor::optional(
            "frequency",
            "Coupon payments per year; default 1.",
            integer_schema(),
        ),
    ])
    .with_output(
        record_schema(vec![
            field("macaulay_duration", exact_schema()),
            field("modified_duration", exact_schema()),
            field("convexity", exact_schema()),
            field("method", text_schema()),
        ]),
        "Macaulay duration (years), modified duration (years), convexity (years^2), and method.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_units_rule("durations are in years; rates are per-period decimals")
    .with_method_ref("docs/methods/finance.md#bond_duration")
    .with_examples(vec![
        Example::new(
            "two-period zero coupon bond",
            example_args(&[
                (
                    "face_value",
                    serde_json::json!({
                        "kind": "money",
                        "amount": {"kind": "decimal", "value": "1000.00"},
                        "currency": "USD"
                    }),
                ),
                ("coupon_rate", serde_json::json!("0")),
                ("periods", serde_json::json!(2)),
                ("yield_rate", serde_json::json!("0")),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "macaulay_duration": {"kind": "decimal", "value": "2"},
            "modified_duration": {"kind": "decimal", "value": "2"},
            "convexity": {"kind": "decimal", "value": "6"},
            "method": "bond_duration"
        }))),
        Example::new(
            "five percent coupon at four percent yield",
            example_args(&[
                (
                    "face_value",
                    serde_json::json!({
                        "kind": "money",
                        "amount": {"kind": "decimal", "value": "1000.00"},
                        "currency": "USD"
                    }),
                ),
                ("coupon_rate", serde_json::json!("0.05")),
                ("periods", serde_json::json!(3)),
                ("yield_rate", serde_json::json!("0.04")),
            ]),
        )
        .with_contains("bond_duration"),
    ])
}

fn invoke_bond_duration(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let (face, _currency) = arg_money(args, "face_value")?;
    let coupon_rate = args.decimal("coupon_rate")?;
    let periods = positive_periods(args, "periods", ctx)?;
    let yield_rate = args.decimal("yield_rate")?;
    let frequency = compounding_frequency(args, "frequency")?;
    let terms = bond_terms(face, &coupon_rate, &yield_rate, frequency, ctx)?;
    let one = Decimal::from_bigint(BigInt::from(1));
    let periods_per_year = Decimal::from_bigint(BigInt::from(frequency));
    let (inverse_frequency, inverse_inexact) =
        one.div(&periods_per_year, &ctx.numeric, &ctx.limits)?;
    let mut rounded = terms.rounded || inverse_inexact;
    let mut discount = one;
    let mut price = Decimal::zero();
    let mut weighted = Decimal::zero();
    let mut convex_weighted = Decimal::zero();
    for period in 1..=periods {
        if period % 1024 == 0 {
            ctx.check()?;
        }
        let (next, inexact) = discount.div(&terms.base, &ctx.numeric, &ctx.limits)?;
        discount = next;
        rounded |= inexact;
        let cash_flow = if period == periods {
            terms.coupon.add(&terms.face, &ctx.numeric, &ctx.limits)?
        } else {
            terms.coupon.clone()
        };
        let present = cash_flow.mul(&discount, &ctx.numeric, &ctx.limits)?;
        price = price.add(&present, &ctx.numeric, &ctx.limits)?;
        let time = Decimal::from_bigint(BigInt::from(period)).mul(
            &inverse_frequency,
            &ctx.numeric,
            &ctx.limits,
        )?;
        let time_plus = time.add(&inverse_frequency, &ctx.numeric, &ctx.limits)?;
        weighted = weighted.add(
            &present.mul(&time, &ctx.numeric, &ctx.limits)?,
            &ctx.numeric,
            &ctx.limits,
        )?;
        convex_weighted = convex_weighted.add(
            &present.mul(&time, &ctx.numeric, &ctx.limits)?.mul(
                &time_plus,
                &ctx.numeric,
                &ctx.limits,
            )?,
            &ctx.numeric,
            &ctx.limits,
        )?;
    }
    if price.is_zero() {
        return Err(EngineError::domain(
            "the bond price is zero; duration and convexity are undefined",
        ));
    }
    let (macaulay, macaulay_inexact) = weighted.div(&price, &ctx.numeric, &ctx.limits)?;
    rounded |= macaulay_inexact;
    let (modified, modified_inexact) = macaulay.div(&terms.base, &ctx.numeric, &ctx.limits)?;
    rounded |= modified_inexact;
    let base_squared = terms.base.mul(&terms.base, &ctx.numeric, &ctx.limits)?;
    let denominator = price.mul(&base_squared, &ctx.numeric, &ctx.limits)?;
    let (convexity, convexity_inexact) =
        convex_weighted.div(&denominator, &ctx.numeric, &ctx.limits)?;
    rounded |= convexity_inexact;
    let value = Value::record([
        ("macaulay_duration", Value::decimal(macaulay)),
        ("modified_duration", Value::decimal(modified)),
        ("convexity", Value::decimal(convexity)),
        ("method", Value::text("bond_duration")),
    ]);
    Ok(Outcome::new(value, exactness(rounded)))
}

pub(crate) fn register() -> Vec<std::sync::Arc<dyn bicmath_core::contract::Function>> {
    use bicmath_core::contract::SimpleFunction;
    vec![
        SimpleFunction::arc(bond_price_descriptor(), invoke_bond_price),
        SimpleFunction::arc(bond_yield_descriptor(), invoke_bond_yield),
        SimpleFunction::arc(bond_duration_descriptor(), invoke_bond_duration),
    ]
}
