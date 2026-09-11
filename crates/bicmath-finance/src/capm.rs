//! Capital Asset Pricing Model: sample beta and expected return.

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::Zero;

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, CostClass, Example, FunctionDescriptor, Outcome, ParamDescriptor,
};
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::value::Value;

use crate::util::{
    all_modes, exact_array, exact_array_schema, exact_schema, exactness, example_args, field,
    parse_value, rational_to_decimal, record_schema, text_schema,
};

fn mean(values: &[BigRational]) -> BigRational {
    let mut sum = BigRational::zero();
    for value in values {
        sum += value;
    }
    sum / BigRational::from_integer(BigInt::from(values.len()))
}

fn beta_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "finance.beta",
        "finance",
        "1.0.0",
        "Market beta",
        "Sample beta of an asset relative to a market return series.",
    )
    .with_description(
        "beta = sample_covariance(asset_returns, market_returns) / \
         sample_variance(market_returns), with the n - 1 denominator in both terms. The \
         series must have the same length, at least two observations, and the market \
         variance must be positive. The result is exact when the ratio terminates as a \
         decimal and rounded otherwise; a series compared with itself has beta 1.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "asset_returns",
            "Asset return series; at least two observations.",
            exact_array_schema(2),
        ),
        ParamDescriptor::required(
            "market_returns",
            "Market return series of the same length.",
            exact_array_schema(2),
        ),
    ])
    .with_output(exact_schema(), "Sample beta.")
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_units_rule("both series are per-period return decimals")
    .with_method_ref("docs/methods/finance.md#beta")
    .with_examples(vec![
        Example::new(
            "a series against itself",
            example_args(&[
                (
                    "asset_returns",
                    serde_json::json!([0.02, -0.01, 0.03, 0.05, -0.02]),
                ),
                (
                    "market_returns",
                    serde_json::json!([0.02, -0.01, 0.03, 0.05, -0.02]),
                ),
            ]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "decimal", "value": "1"}),
        )),
        Example::new(
            "constant market",
            example_args(&[
                ("asset_returns", serde_json::json!([0.01, 0.02])),
                ("market_returns", serde_json::json!([0.01, 0.01])),
            ]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_beta(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let asset = exact_array(args, "asset_returns")?;
    let market = exact_array(args, "market_returns")?;
    if asset.len() != market.len() {
        return Err(EngineError::malformed(format!(
            "asset_returns and market_returns must have the same length; found {} and {}",
            asset.len(),
            market.len()
        )));
    }
    let asset_mean = mean(&asset);
    let market_mean = mean(&market);
    let mut covariance = BigRational::zero();
    let mut variance = BigRational::zero();
    for (a, m) in asset.iter().zip(market.iter()) {
        covariance += (a - &asset_mean) * (m - &market_mean);
        variance += (m - &market_mean) * (m - &market_mean);
    }
    if variance.is_zero() {
        return Err(EngineError::domain(
            "the market return series has zero variance; beta is undefined",
        ));
    }
    let beta = covariance / variance;
    let (decimal, inexact) = rational_to_decimal(&beta, ctx)?;
    Ok(Outcome::new(Value::decimal(decimal), exactness(inexact)))
}

fn capm_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "finance.capm",
        "finance",
        "1.0.0",
        "CAPM expected return",
        "Expected return and risk premium from the Capital Asset Pricing Model.",
    )
    .with_description(
        "expected_return = risk_free_rate + beta * (market_return - risk_free_rate) and \
         risk_premium = beta * (market_return - risk_free_rate). All three inputs are \
         exact decimals and the arithmetic is exact.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "risk_free_rate",
            "Per-period risk-free rate as an exact decimal.",
            exact_schema(),
        ),
        ParamDescriptor::required("beta", "Asset beta as an exact decimal.", exact_schema()),
        ParamDescriptor::required(
            "market_return",
            "Expected market return as an exact decimal.",
            exact_schema(),
        ),
    ])
    .with_output(
        record_schema(vec![
            field("expected_return", exact_schema()),
            field("risk_premium", exact_schema()),
            field("method", text_schema()),
        ]),
        "CAPM expected return, risk premium, and method.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Constant)
    .with_units_rule("all rates are per-period decimals")
    .with_method_ref("docs/methods/finance.md#capm")
    .with_examples(vec![
        Example::new(
            "beta 1.2 with a five percent market premium",
            example_args(&[
                ("risk_free_rate", serde_json::json!("0.03")),
                ("beta", serde_json::json!("1.2")),
                ("market_return", serde_json::json!("0.08")),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "expected_return": {"kind": "decimal", "value": "0.09"},
            "risk_premium": {"kind": "decimal", "value": "0.06"},
            "method": "capm"
        }))),
    ])
}

fn invoke_capm(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let risk_free = args.decimal("risk_free_rate")?;
    let beta = args.decimal("beta")?;
    let market = args.decimal("market_return")?;
    let excess = market.sub(&risk_free, &ctx.numeric, &ctx.limits)?;
    let risk_premium = beta.mul(&excess, &ctx.numeric, &ctx.limits)?;
    let expected_return = risk_free.add(&risk_premium, &ctx.numeric, &ctx.limits)?;
    let value = Value::record([
        ("expected_return", Value::decimal(expected_return)),
        ("risk_premium", Value::decimal(risk_premium)),
        ("method", Value::text("capm")),
    ]);
    Ok(Outcome::exact(value))
}

pub(crate) fn register() -> Vec<std::sync::Arc<dyn bicmath_core::contract::Function>> {
    use bicmath_core::contract::SimpleFunction;
    vec![
        SimpleFunction::arc(beta_descriptor(), invoke_beta),
        SimpleFunction::arc(capm_descriptor(), invoke_capm),
    ]
}
