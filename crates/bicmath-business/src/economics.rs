//! Unit economics, customer lifetime value, and pricing elasticity.
//!
//! All money-like values are computed with exact rational arithmetic and only
//! rounded when a quotient does not terminate as a decimal.

use std::cmp::Ordering;

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{Signed, ToPrimitive, Zero};

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, CostClass, Example, FunctionDescriptor, Outcome, ParamDescriptor,
};
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::schema::ValueSchema;
use bicmath_core::value::Value;

use crate::util::{
    all_modes, decimal_schema, exact_rational, exact_schema, exactness, example_args, field,
    integer_schema, optional_exact_rational, parse_value, rational_value, record_schema,
    text_schema,
};

const UNIT_ECONOMICS_METHOD: &str = "unit_economics";
const LTV_CAC_UNDISCOUNTED_METHOD: &str = "ltv_cac_undiscounted";
const LTV_CAC_DISCOUNTED_METHOD: &str = "ltv_cac_discounted";
const GEOMETRIC_CLV_METHOD: &str = "geometric_clv";
const ARC_ELASTICITY_METHOD: &str = "arc_elasticity";

fn rational(value: i64) -> BigRational {
    BigRational::from_integer(BigInt::from(value))
}

// ---------------------------------------------------------------------------
// business.unit_economics
// ---------------------------------------------------------------------------

fn unit_economics_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "business.unit_economics",
        "business",
        "1.0.0",
        "Unit economics",
        "Unit contribution, contribution margin, gross profit, break-even, and margin of safety.",
    )
    .with_description(
        "unit_contribution = price - unit_variable_cost; contribution_margin_ratio = \
         unit_contribution / price; gross_profit = unit_contribution * volume - fixed_costs; \
         break_even_units = fixed_costs / unit_contribution; break_even_revenue = \
         break_even_units * price; margin_of_safety_units = volume - break_even_units. \
         price must be strictly greater than unit_variable_cost, otherwise the unit \
         contribution is not positive and the break-even point does not exist. Values are \
         computed exactly and rounded to the context precision only when a quotient does \
         not terminate as a decimal.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("price", "Selling price per unit.", exact_schema()),
        ParamDescriptor::required(
            "unit_variable_cost",
            "Variable cost per unit.",
            exact_schema(),
        ),
        ParamDescriptor::required(
            "fixed_costs",
            "Total fixed costs over the period.",
            exact_schema(),
        ),
        ParamDescriptor::required("volume", "Units sold over the period.", exact_schema()),
    ])
    .with_output(
        record_schema(vec![
            field("unit_contribution", decimal_schema()),
            field("contribution_margin_ratio", decimal_schema()),
            field("gross_profit", decimal_schema()),
            field("break_even_units", decimal_schema()),
            field("break_even_revenue", decimal_schema()),
            field("margin_of_safety_units", decimal_schema()),
            field("method", text_schema()),
        ]),
        "Unit contribution, contribution margin ratio, gross profit, break-even units and \
         revenue, margin of safety in units, and method.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Constant)
    .with_units_rule(
        "price, unit_variable_cost, and fixed_costs share one currency unit; volume and the \
         break-even and margin-of-safety outputs are in units.",
    )
    .with_method_ref("docs/methods/business.md#unit_economics")
    .with_tags(["unit-economics", "break-even", "contribution-margin"])
    .with_examples(vec![
        Example::new(
            "break even at 250 units",
            example_args(&[
                ("price", serde_json::json!("10")),
                ("unit_variable_cost", serde_json::json!("6")),
                ("fixed_costs", serde_json::json!("1000")),
                ("volume", serde_json::json!("300")),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "unit_contribution": {"kind": "decimal", "value": "4"},
            "contribution_margin_ratio": {"kind": "decimal", "value": "0.4"},
            "gross_profit": {"kind": "decimal", "value": "200"},
            "break_even_units": {"kind": "decimal", "value": "250"},
            "break_even_revenue": {"kind": "decimal", "value": "2500"},
            "margin_of_safety_units": {"kind": "decimal", "value": "50"},
            "method": UNIT_ECONOMICS_METHOD
        }))),
        Example::new(
            "price below variable cost",
            example_args(&[
                ("price", serde_json::json!("6")),
                ("unit_variable_cost", serde_json::json!("10")),
                ("fixed_costs", serde_json::json!("1000")),
                ("volume", serde_json::json!("300")),
            ]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_unit_economics(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let price = exact_rational(args, "price")?;
    let unit_variable_cost = exact_rational(args, "unit_variable_cost")?;
    let fixed_costs = exact_rational(args, "fixed_costs")?;
    let volume = exact_rational(args, "volume")?;
    if price <= unit_variable_cost {
        return Err(EngineError::domain(
            "price must be strictly greater than unit_variable_cost",
        ));
    }
    let unit_contribution = &price - &unit_variable_cost;
    let contribution_margin_ratio = &unit_contribution / &price;
    let gross_profit = &unit_contribution * &volume - &fixed_costs;
    let break_even_units = &fixed_costs / &unit_contribution;
    let break_even_revenue = &break_even_units * &price;
    let margin_of_safety_units = &volume - &break_even_units;
    let mut rounded = false;
    let value = Value::record([
        (
            "unit_contribution",
            rational_value(&unit_contribution, ctx, &mut rounded)?,
        ),
        (
            "contribution_margin_ratio",
            rational_value(&contribution_margin_ratio, ctx, &mut rounded)?,
        ),
        (
            "gross_profit",
            rational_value(&gross_profit, ctx, &mut rounded)?,
        ),
        (
            "break_even_units",
            rational_value(&break_even_units, ctx, &mut rounded)?,
        ),
        (
            "break_even_revenue",
            rational_value(&break_even_revenue, ctx, &mut rounded)?,
        ),
        (
            "margin_of_safety_units",
            rational_value(&margin_of_safety_units, ctx, &mut rounded)?,
        ),
        ("method", Value::text(UNIT_ECONOMICS_METHOD)),
    ]);
    Ok(Outcome::new(value, exactness(rounded)))
}

// ---------------------------------------------------------------------------
// business.ltv_cac
// ---------------------------------------------------------------------------

fn ltv_cac_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "business.ltv_cac",
        "business",
        "1.0.0",
        "LTV to CAC",
        "Customer lifetime value, LTV/CAC ratio, and CAC payback from ARPU and churn.",
    )
    .with_description(
        "The undiscounted CLV is ARPU * gross_margin_rate / monthly_churn_rate. When \
         discount_rate is supplied the CLV is the geometric discounted series \
         sum_{t>=0} ARPU * gross_margin_rate * (1 - churn)^t / (1 + discount_rate)^t, which \
         converges to ARPU * gross_margin_rate * (1 + discount_rate) / \
         (churn + discount_rate). ltv_cac_ratio = clv / cac and payback_months = \
         cac / (arpu * gross_margin_rate). monthly_churn_rate must be strictly positive; \
         cac, arpu, and gross_margin_rate must be positive.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "arpu",
            "Average revenue per user per month.",
            exact_schema(),
        ),
        ParamDescriptor::required(
            "gross_margin_rate",
            "Gross margin as a fraction of ARPU, in (0, 1].",
            exact_schema(),
        ),
        ParamDescriptor::required(
            "monthly_churn_rate",
            "Monthly churn as a fraction, in (0, 1].",
            exact_schema(),
        ),
        ParamDescriptor::required("cac", "Customer acquisition cost.", exact_schema()),
        ParamDescriptor::optional(
            "discount_rate",
            "Monthly discount rate; when present the CLV series is discounted.",
            exact_schema(),
        ),
    ])
    .with_output(
        record_schema(vec![
            field("clv", decimal_schema()),
            field("ltv_cac_ratio", decimal_schema()),
            field("payback_months", decimal_schema()),
            field("method", text_schema()),
        ]),
        "Customer lifetime value, LTV/CAC ratio, CAC payback in months, and method.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Constant)
    .with_units_rule(
        "arpu, cac, and clv share one currency unit per month; payback_months is in months.",
    )
    .with_method_ref("docs/methods/business.md#ltv_cac")
    .with_tags(["ltv", "cac", "churn", "retention"])
    .with_examples(vec![
        Example::new(
            "undiscounted LTV/CAC",
            example_args(&[
                ("arpu", serde_json::json!("100")),
                ("gross_margin_rate", serde_json::json!("0.8")),
                ("monthly_churn_rate", serde_json::json!("0.05")),
                ("cac", serde_json::json!("500")),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "clv": {"kind": "decimal", "value": "1600"},
            "ltv_cac_ratio": {"kind": "decimal", "value": "3.2"},
            "payback_months": {"kind": "decimal", "value": "6.25"},
            "method": LTV_CAC_UNDISCOUNTED_METHOD
        }))),
        Example::new(
            "discounted LTV/CAC",
            example_args(&[
                ("arpu", serde_json::json!("100")),
                ("gross_margin_rate", serde_json::json!("0.8")),
                ("monthly_churn_rate", serde_json::json!("0.05")),
                ("cac", serde_json::json!("500")),
                ("discount_rate", serde_json::json!("0.2")),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "clv": {"kind": "decimal", "value": "384"},
            "ltv_cac_ratio": {"kind": "decimal", "value": "0.768"},
            "payback_months": {"kind": "decimal", "value": "6.25"},
            "method": LTV_CAC_DISCOUNTED_METHOD
        }))),
        Example::new(
            "zero churn is rejected",
            example_args(&[
                ("arpu", serde_json::json!("100")),
                ("gross_margin_rate", serde_json::json!("0.8")),
                ("monthly_churn_rate", serde_json::json!("0")),
                ("cac", serde_json::json!("500")),
            ]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_ltv_cac(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let arpu = exact_rational(args, "arpu")?;
    let gross_margin_rate = exact_rational(args, "gross_margin_rate")?;
    let monthly_churn_rate = exact_rational(args, "monthly_churn_rate")?;
    let cac = exact_rational(args, "cac")?;
    let discount_rate = optional_exact_rational(args, "discount_rate")?;
    let zero = rational(0);
    let one = rational(1);
    if arpu <= zero {
        return Err(EngineError::domain("arpu must be positive"));
    }
    if gross_margin_rate <= zero || gross_margin_rate > one {
        return Err(EngineError::domain(
            "gross_margin_rate must be a fraction in (0, 1]",
        ));
    }
    if monthly_churn_rate <= zero || monthly_churn_rate > one {
        return Err(EngineError::domain(
            "monthly_churn_rate must be a fraction in (0, 1]",
        ));
    }
    if cac <= zero {
        return Err(EngineError::domain("cac must be positive"));
    }
    let margin_per_month = &arpu * &gross_margin_rate;
    let (clv, method) = match &discount_rate {
        None => (
            margin_per_month.clone() / &monthly_churn_rate,
            LTV_CAC_UNDISCOUNTED_METHOD,
        ),
        Some(discount) => {
            if *discount <= rational(-1) {
                return Err(EngineError::domain("discount_rate must be greater than -1"));
            }
            let growth = &one + discount;
            let retention = &one - &monthly_churn_rate;
            if retention / &growth >= one {
                return Err(EngineError::domain(
                    "the discounted CLV series does not converge; use a discount_rate \
                     greater than -monthly_churn_rate",
                ));
            }
            let clv = &margin_per_month * &growth / (&monthly_churn_rate + discount);
            (clv, LTV_CAC_DISCOUNTED_METHOD)
        }
    };
    let ltv_cac_ratio = &clv / &cac;
    let payback_months = &cac / &margin_per_month;
    let mut rounded = false;
    let value = Value::record([
        ("clv", rational_value(&clv, ctx, &mut rounded)?),
        (
            "ltv_cac_ratio",
            rational_value(&ltv_cac_ratio, ctx, &mut rounded)?,
        ),
        (
            "payback_months",
            rational_value(&payback_months, ctx, &mut rounded)?,
        ),
        ("method", Value::text(method)),
    ]);
    Ok(Outcome::new(value, exactness(rounded)))
}

// ---------------------------------------------------------------------------
// business.clv_geometric
// ---------------------------------------------------------------------------

fn clv_geometric_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "business.clv_geometric",
        "business",
        "1.0.0",
        "Geometric CLV",
        "Customer lifetime value as a geometric revenue series with optional discounting.",
    )
    .with_description(
        "CLV = sum_{t=0}^{periods-1} revenue_per_period * retention_rate^t / \
         (1 + discount_rate)^t. When periods is omitted the infinite series is summed, \
         which requires retention_rate / (1 + discount_rate) < 1. The result is exact when \
         the closed form terminates as a decimal and rounded to the context precision \
         otherwise. The returned periods field is null for the infinite series.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "revenue_per_period",
            "Revenue recognized per period.",
            exact_schema(),
        ),
        ParamDescriptor::required(
            "retention_rate",
            "Period-over-period retention rate, in [0, 1].",
            exact_schema(),
        ),
        ParamDescriptor::required(
            "discount_rate",
            "Per-period discount rate; must be greater than -1.",
            exact_schema(),
        ),
        ParamDescriptor::optional(
            "periods",
            "Number of periods to sum; when omitted the infinite series is used.",
            integer_schema(),
        ),
    ])
    .with_output(
        record_schema(vec![
            field("clv", decimal_schema()),
            field("periods", ValueSchema::Any),
            field("method", text_schema()),
        ]),
        "Geometric CLV, the number of periods summed (null for infinite), and method.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Iterative)
    .with_units_rule("revenue_per_period and clv share one currency unit per period.")
    .with_method_ref("docs/methods/business.md#clv_geometric")
    .with_tags(["clv", "retention", "discounting"])
    .with_examples(vec![
        Example::new(
            "three periods at 50% retention",
            example_args(&[
                ("revenue_per_period", serde_json::json!("100")),
                ("retention_rate", serde_json::json!("0.5")),
                ("discount_rate", serde_json::json!("0")),
                ("periods", serde_json::json!(3)),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "clv": {"kind": "decimal", "value": "175"},
            "periods": {"kind": "integer", "value": "3"},
            "method": GEOMETRIC_CLV_METHOD
        }))),
        Example::new(
            "infinite series at 50% retention",
            example_args(&[
                ("revenue_per_period", serde_json::json!("100")),
                ("retention_rate", serde_json::json!("0.5")),
                ("discount_rate", serde_json::json!("0")),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "clv": {"kind": "decimal", "value": "200"},
            "periods": null,
            "method": GEOMETRIC_CLV_METHOD
        }))),
        Example::new(
            "non-convergent infinite series",
            example_args(&[
                ("revenue_per_period", serde_json::json!("100")),
                ("retention_rate", serde_json::json!("1")),
                ("discount_rate", serde_json::json!("0")),
            ]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_clv_geometric(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let revenue_per_period = exact_rational(args, "revenue_per_period")?;
    let retention_rate = exact_rational(args, "retention_rate")?;
    let discount_rate = exact_rational(args, "discount_rate")?;
    let zero = rational(0);
    let one = rational(1);
    if retention_rate < zero || retention_rate > one {
        return Err(EngineError::domain("retention_rate must be in [0, 1]"));
    }
    if discount_rate <= rational(-1) {
        return Err(EngineError::domain("discount_rate must be greater than -1"));
    }
    let growth = &one + &discount_rate;
    let ratio = &retention_rate / &growth;
    let periods = match args.optional_integer("periods")? {
        None => None,
        Some(value) => {
            let n = value
                .to_i32()
                .ok_or_else(|| EngineError::domain("periods is out of the supported range"))?;
            if n < 1 {
                return Err(EngineError::domain("periods must be at least 1"));
            }
            if n as u64 > ctx.limits.max_series_terms {
                return Err(EngineError::new(
                    ErrorCode::ResourceLimit,
                    format!(
                        "periods={n} exceeds the series term limit of {}",
                        ctx.limits.max_series_terms
                    ),
                ));
            }
            Some(n)
        }
    };
    let clv = match periods {
        Some(n) => {
            if ratio == one {
                &revenue_per_period * BigRational::from_integer(BigInt::from(n))
            } else {
                let numerator = &one - ratio.pow(n);
                let denominator = &one - &ratio;
                &revenue_per_period * numerator / denominator
            }
        }
        None => {
            if ratio >= one {
                return Err(EngineError::domain(
                    "the infinite geometric CLV series does not converge; supply periods or \
                     a positive discount_rate",
                ));
            }
            &revenue_per_period / (&one - &ratio)
        }
    };
    let mut rounded = false;
    let clv_value = rational_value(&clv, ctx, &mut rounded)?;
    let periods_value = match periods {
        Some(n) => Value::integer(BigInt::from(n)),
        None => Value::Null,
    };
    let value = Value::record([
        ("clv", clv_value),
        ("periods", periods_value),
        ("method", Value::text(GEOMETRIC_CLV_METHOD)),
    ]);
    Ok(Outcome::new(value, exactness(rounded)))
}

// ---------------------------------------------------------------------------
// business.arc_elasticity
// ---------------------------------------------------------------------------

fn arc_elasticity_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "business.arc_elasticity",
        "business",
        "1.0.0",
        "Arc elasticity",
        "Midpoint arc price elasticity of demand with an elasticity classification.",
    )
    .with_description(
        "elasticity = ((new_quantity - old_quantity) / midpoint_quantity) / \
         ((new_price - old_price) / midpoint_price), where each midpoint is the average of \
         the old and new values. A negative elasticity is expected for ordinary demand. \
         classification is elastic when |elasticity| > 1, inelastic when |elasticity| < 1, \
         and unit when |elasticity| = 1. The price must change and both midpoints must be \
         non-zero.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("old_price", "Price before the change.", exact_schema()),
        ParamDescriptor::required("new_price", "Price after the change.", exact_schema()),
        ParamDescriptor::required(
            "old_quantity",
            "Quantity demanded before the change.",
            exact_schema(),
        ),
        ParamDescriptor::required(
            "new_quantity",
            "Quantity demanded after the change.",
            exact_schema(),
        ),
    ])
    .with_output(
        record_schema(vec![
            field("elasticity", decimal_schema()),
            field("classification", text_schema()),
            field("method", text_schema()),
        ]),
        "Midpoint arc elasticity, its classification, and method.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Constant)
    .with_units_rule("price and quantity are dimensionless in the elasticity ratio.")
    .with_method_ref("docs/methods/business.md#arc_elasticity")
    .with_tags(["elasticity", "pricing", "demand"])
    .with_examples(vec![
        Example::new(
            "elastic demand",
            example_args(&[
                ("old_price", serde_json::json!("10")),
                ("new_price", serde_json::json!("12")),
                ("old_quantity", serde_json::json!("100")),
                ("new_quantity", serde_json::json!("60")),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "elasticity": {"kind": "decimal", "value": "-2.75"},
            "classification": "elastic",
            "method": ARC_ELASTICITY_METHOD
        }))),
        Example::new(
            "unit elasticity",
            example_args(&[
                ("old_price", serde_json::json!("10")),
                ("new_price", serde_json::json!("20")),
                ("old_quantity", serde_json::json!("100")),
                ("new_quantity", serde_json::json!("50")),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "elasticity": {"kind": "decimal", "value": "-1"},
            "classification": "unit",
            "method": ARC_ELASTICITY_METHOD
        }))),
    ])
}

fn invoke_arc_elasticity(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let old_price = exact_rational(args, "old_price")?;
    let new_price = exact_rational(args, "new_price")?;
    let old_quantity = exact_rational(args, "old_quantity")?;
    let new_quantity = exact_rational(args, "new_quantity")?;
    if new_price == old_price {
        return Err(EngineError::domain(
            "new_price must differ from old_price to define an elasticity",
        ));
    }
    let price_sum = &old_price + &new_price;
    let quantity_sum = &old_quantity + &new_quantity;
    if price_sum.is_zero() {
        return Err(EngineError::domain("the midpoint price must be non-zero"));
    }
    if quantity_sum.is_zero() {
        return Err(EngineError::domain(
            "the midpoint quantity must be non-zero",
        ));
    }
    let numerator = (&new_quantity - &old_quantity) * &price_sum;
    let denominator = (&new_price - &old_price) * &quantity_sum;
    let elasticity = numerator / denominator;
    let classification = match elasticity.abs().cmp(&rational(1)) {
        Ordering::Greater => "elastic",
        Ordering::Less => "inelastic",
        Ordering::Equal => "unit",
    };
    let mut rounded = false;
    let value = Value::record([
        (
            "elasticity",
            rational_value(&elasticity, ctx, &mut rounded)?,
        ),
        ("classification", Value::text(classification)),
        ("method", Value::text(ARC_ELASTICITY_METHOD)),
    ]);
    Ok(Outcome::new(value, exactness(rounded)))
}

pub(crate) fn register() -> Vec<std::sync::Arc<dyn bicmath_core::contract::Function>> {
    use bicmath_core::contract::SimpleFunction;
    vec![
        SimpleFunction::arc(unit_economics_descriptor(), invoke_unit_economics),
        SimpleFunction::arc(ltv_cac_descriptor(), invoke_ltv_cac),
        SimpleFunction::arc(clv_geometric_descriptor(), invoke_clv_geometric),
        SimpleFunction::arc(arc_elasticity_descriptor(), invoke_arc_elasticity),
    ]
}
