//! European Black-Scholes and Cox-Ross-Rubinstein binomial option pricing.
//!
//! Option prices and greeks are explicit binary64 approximations. The zero
//! volatility limit is handled deterministically: the option is priced at its
//! discounted forward intrinsic value and `d1`/`d2` are reported as null
//! because the standardized moneyness has no unique limit.

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, CostClass, Example, FunctionDescriptor, Outcome, ParamDescriptor, require_mode,
};
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::number::NumericMode;
use bicmath_core::schema::ValueSchema;
use bicmath_core::value::Value;

use crate::mathfn::{exp, ln, normal_cdf, normal_pdf, sqrt};
use crate::util::{
    enum_schema, exact_schema, example_args, field, float_value, float64_schema, integer_schema,
    number_to_f64, parse_value, record_schema, text_schema,
};

/// Upper bound on binomial steps accepted by `finance.binomial_option`.
const MAX_BINOMIAL_STEPS: usize = 5_000;

#[derive(Clone, Copy, PartialEq, Eq)]
enum OptionKind {
    Call,
    Put,
}

fn float_modes() -> Vec<NumericMode> {
    vec![NumericMode::Auto, NumericMode::Scientific]
}

fn option_kind(args: &Args) -> Result<OptionKind, EngineError> {
    match args.text("option_type")? {
        "call" => Ok(OptionKind::Call),
        "put" => Ok(OptionKind::Put),
        other => Err(EngineError::domain(format!(
            "unknown option_type {other:?}; expected call or put"
        ))),
    }
}

fn intrinsic(kind: OptionKind, spot: f64, strike: f64) -> f64 {
    match kind {
        OptionKind::Call => (spot - strike).max(0.0),
        OptionKind::Put => (strike - spot).max(0.0),
    }
}

struct Greeks {
    price: f64,
    delta: f64,
    gamma: f64,
    vega: f64,
    theta: f64,
    rho: f64,
    d1: Option<f64>,
    d2: Option<f64>,
}

fn black_scholes_values(
    kind: OptionKind,
    spot: f64,
    strike: f64,
    rate: f64,
    dividend_yield: f64,
    volatility: f64,
    time: f64,
) -> Result<Greeks, EngineError> {
    let spot_pv = spot * exp(-dividend_yield * time);
    let strike_pv = strike * exp(-rate * time);
    if volatility == 0.0 {
        let forward = spot_pv - strike_pv;
        let (price, moneyness) = match kind {
            OptionKind::Call => (forward.max(0.0), forward),
            OptionKind::Put => ((-forward).max(0.0), -forward),
        };
        let indicator = if moneyness > 0.0 {
            1.0
        } else if moneyness < 0.0 {
            0.0
        } else {
            0.5
        };
        let delta = match kind {
            OptionKind::Call => indicator * exp(-dividend_yield * time),
            OptionKind::Put => -indicator * exp(-dividend_yield * time),
        };
        let theta = match kind {
            OptionKind::Call => dividend_yield * spot_pv * indicator - rate * strike_pv * indicator,
            OptionKind::Put => -dividend_yield * spot_pv * indicator + rate * strike_pv * indicator,
        };
        let rho = match kind {
            OptionKind::Call => strike_pv * time * indicator,
            OptionKind::Put => -strike_pv * time * indicator,
        };
        return Ok(Greeks {
            price,
            delta,
            gamma: 0.0,
            vega: 0.0,
            theta,
            rho,
            d1: None,
            d2: None,
        });
    }
    let sqrt_time = sqrt(time);
    let d1 = (ln(spot / strike) + (rate - dividend_yield + 0.5 * volatility * volatility) * time)
        / (volatility * sqrt_time);
    let d2 = d1 - volatility * sqrt_time;
    let nd1 = normal_cdf(d1)?;
    let nd2 = normal_cdf(d2)?;
    let density = normal_pdf(d1);
    let disc_spot = exp(-dividend_yield * time);
    let disc_strike = exp(-rate * time);
    let gamma = disc_spot * density / (spot * volatility * sqrt_time);
    let vega = spot * disc_spot * density * sqrt_time;
    let (price, delta, theta, rho) = match kind {
        OptionKind::Call => (
            spot * disc_spot * nd1 - strike * disc_strike * nd2,
            disc_spot * nd1,
            -spot * disc_spot * density * volatility / (2.0 * sqrt_time)
                + dividend_yield * spot * disc_spot * nd1
                - rate * strike * disc_strike * nd2,
            strike * time * disc_strike * nd2,
        ),
        OptionKind::Put => {
            let nd1_neg = normal_cdf(-d1)?;
            let nd2_neg = normal_cdf(-d2)?;
            (
                strike * disc_strike * nd2_neg - spot * disc_spot * nd1_neg,
                -disc_spot * nd1_neg,
                -spot * disc_spot * density * volatility / (2.0 * sqrt_time)
                    - dividend_yield * spot * disc_spot * nd1_neg
                    + rate * strike * disc_strike * nd2_neg,
                -strike * time * disc_strike * nd2_neg,
            )
        }
    };
    Ok(Greeks {
        price,
        delta,
        gamma,
        vega,
        theta,
        rho,
        d1: Some(d1),
        d2: Some(d2),
    })
}

fn black_scholes_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "finance.black_scholes",
        "finance",
        "1.0.0",
        "Black-Scholes option price",
        "European call or put price and greeks with a continuous dividend yield.",
    )
    .with_description(
        "Computes the Black-Scholes-Merton price of a European option and its delta, gamma, \
         vega, theta (per year), and rho. d1 = (ln(S/K) + (r - q + sigma^2/2) T) / \
         (sigma sqrt(T)) and d2 = d1 - sigma sqrt(T). The volatility may be zero: the \
         option is then priced at its discounted forward intrinsic value, the greeks take \
         their zero-volatility limits, and d1/d2 are null because the standardized \
         moneyness has no unique limit. Option pricing is binary64 and reported \
         approximate; spot and strike must be positive and time must be positive.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "option_type",
            "call or put.",
            enum_schema(&["call", "put"]),
        ),
        ParamDescriptor::required("spot", "Spot price; must be positive.", exact_schema()),
        ParamDescriptor::required("strike", "Strike price; must be positive.", exact_schema()),
        ParamDescriptor::required(
            "rate",
            "Continuously compounded risk-free rate as an exact decimal.",
            exact_schema(),
        ),
        ParamDescriptor::required(
            "volatility",
            "Annualized volatility as an exact decimal; zero is allowed.",
            exact_schema(),
        ),
        ParamDescriptor::required("time", "Time to expiry in years; must be positive.", exact_schema()),
        ParamDescriptor::optional(
            "dividend_yield",
            "Continuous dividend yield; default 0.",
            exact_schema(),
        ),
    ])
    .with_output(
        record_schema(vec![
            field("price", float64_schema()),
            field("delta", float64_schema()),
            field("gamma", float64_schema()),
            field("vega", float64_schema()),
            field("theta", float64_schema()),
            field("rho", float64_schema()),
            field("d1", ValueSchema::Any),
            field("d2", ValueSchema::Any),
            field("method", text_schema()),
        ]),
        "Price, delta, gamma, vega, theta, rho, d1, d2, and method.",
    )
    .with_modes(float_modes())
    .with_cost(CostClass::Constant)
    .with_units_rule("spot and strike share one price unit; rate, volatility, and dividend_yield are annual decimals")
    .with_method_ref("docs/methods/finance.md#black_scholes")
    .with_examples(vec![
        Example::new(
            "zero volatility prices the discounted forward intrinsic",
            example_args(&[
                ("option_type", serde_json::json!("call")),
                ("spot", serde_json::json!(100)),
                ("strike", serde_json::json!(90)),
                ("rate", serde_json::json!("0")),
                ("volatility", serde_json::json!("0")),
                ("time", serde_json::json!(1)),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "price": {"kind": "float64", "value": "10"},
            "delta": {"kind": "float64", "value": "1"},
            "gamma": {"kind": "float64", "value": "0"},
            "vega": {"kind": "float64", "value": "0"},
            "theta": {"kind": "float64", "value": "0"},
            "rho": {"kind": "float64", "value": "90"},
            "d1": null,
            "d2": null,
            "method": "black_scholes"
        }))),
        Example::new(
            "at-the-money call",
            example_args(&[
                ("option_type", serde_json::json!("call")),
                ("spot", serde_json::json!(100)),
                ("strike", serde_json::json!(100)),
                ("rate", serde_json::json!("0.05")),
                ("volatility", serde_json::json!("0.2")),
                ("time", serde_json::json!(1)),
            ]),
        )
        .with_contains("black_scholes"),
        Example::new(
            "negative volatility",
            example_args(&[
                ("option_type", serde_json::json!("call")),
                ("spot", serde_json::json!(100)),
                ("strike", serde_json::json!(100)),
                ("rate", serde_json::json!("0.05")),
                ("volatility", serde_json::json!("-0.2")),
                ("time", serde_json::json!(1)),
            ]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_black_scholes(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &float_modes(), "finance.black_scholes")?;
    let kind = option_kind(args)?;
    let spot = number_to_f64(args.number("spot")?, "spot")?;
    let strike = number_to_f64(args.number("strike")?, "strike")?;
    let rate = number_to_f64(args.number("rate")?, "rate")?;
    let volatility = number_to_f64(args.number("volatility")?, "volatility")?;
    let time = number_to_f64(args.number("time")?, "time")?;
    let dividend_yield = match args.optional_number("dividend_yield")? {
        Some(number) => number_to_f64(number, "dividend_yield")?,
        None => 0.0,
    };
    if spot <= 0.0 {
        return Err(EngineError::domain("spot must be positive"));
    }
    if strike <= 0.0 {
        return Err(EngineError::domain("strike must be positive"));
    }
    if volatility < 0.0 {
        return Err(EngineError::domain("volatility must be non-negative"));
    }
    if time <= 0.0 {
        return Err(EngineError::domain("time must be positive"));
    }
    let greeks = black_scholes_values(kind, spot, strike, rate, dividend_yield, volatility, time)?;
    let d1 = match greeks.d1 {
        Some(value) => float_value(value)?,
        None => Value::Null,
    };
    let d2 = match greeks.d2 {
        Some(value) => float_value(value)?,
        None => Value::Null,
    };
    let value = Value::record([
        ("price", float_value(greeks.price)?),
        ("delta", float_value(greeks.delta)?),
        ("gamma", float_value(greeks.gamma)?),
        ("vega", float_value(greeks.vega)?),
        ("theta", float_value(greeks.theta)?),
        ("rho", float_value(greeks.rho)?),
        ("d1", d1),
        ("d2", d2),
        ("method", Value::text("black_scholes")),
    ]);
    Ok(Outcome::approximate(value))
}

fn binomial_option_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "finance.binomial_option",
        "finance",
        "1.0.0",
        "Cox-Ross-Rubinstein binomial option",
        "European or American call or put price on a CRR binomial tree.",
    )
    .with_description(
        "Builds a Cox-Ross-Rubinstein recombining tree with dt = T / steps, \
         u = exp(sigma sqrt(dt)), d = 1 / u, and risk-neutral probability \
         p = (exp(r dt) - d) / (u - d). European options take the discounted expected \
         value; American options allow early exercise at each node. Zero volatility is \
         allowed and collapses the tree to the deterministic discounted forward \
         intrinsic. steps must be at least 1 and at most 5000 and must fit the \
         execution context's array and operation budgets.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("option_type", "call or put.", enum_schema(&["call", "put"])),
        ParamDescriptor::required("spot", "Spot price; must be positive.", exact_schema()),
        ParamDescriptor::required("strike", "Strike price; must be positive.", exact_schema()),
        ParamDescriptor::required(
            "rate",
            "Continuously compounded risk-free rate as an exact decimal.",
            exact_schema(),
        ),
        ParamDescriptor::required(
            "volatility",
            "Annualized volatility as an exact decimal; zero is allowed.",
            exact_schema(),
        ),
        ParamDescriptor::required(
            "time",
            "Time to expiry in years; must be positive.",
            exact_schema(),
        ),
        ParamDescriptor::required(
            "steps",
            "Number of tree steps; 1..=5000 and within the context limits.",
            integer_schema(),
        ),
        ParamDescriptor::optional(
            "american",
            "Allow early exercise; default false.",
            ValueSchema::Bool,
        ),
    ])
    .with_output(
        record_schema(vec![
            field("price", float64_schema()),
            field("steps", integer_schema()),
            field("american", ValueSchema::Bool),
            field("method", text_schema()),
        ]),
        "Option price, step count, early-exercise flag, and method.",
    )
    .with_modes(float_modes())
    .with_cost(CostClass::Quadratic)
    .with_units_rule(
        "spot and strike share one price unit; rate and volatility are annual decimals",
    )
    .with_method_ref("docs/methods/finance.md#binomial_option")
    .with_examples(vec![
        Example::new(
            "zero volatility one-step call",
            example_args(&[
                ("option_type", serde_json::json!("call")),
                ("spot", serde_json::json!(100)),
                ("strike", serde_json::json!(90)),
                ("rate", serde_json::json!("0")),
                ("volatility", serde_json::json!("0")),
                ("time", serde_json::json!(1)),
                ("steps", serde_json::json!(1)),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "price": {"kind": "float64", "value": "10"},
            "steps": {"kind": "integer", "value": "1"},
            "american": false,
            "method": "crr_binomial"
        }))),
        Example::new(
            "one hundred step call",
            example_args(&[
                ("option_type", serde_json::json!("call")),
                ("spot", serde_json::json!(100)),
                ("strike", serde_json::json!(100)),
                ("rate", serde_json::json!("0.05")),
                ("volatility", serde_json::json!("0.2")),
                ("time", serde_json::json!(1)),
                ("steps", serde_json::json!(100)),
            ]),
        )
        .with_contains("crr_binomial"),
        Example::new(
            "zero steps",
            example_args(&[
                ("option_type", serde_json::json!("call")),
                ("spot", serde_json::json!(100)),
                ("strike", serde_json::json!(90)),
                ("rate", serde_json::json!("0")),
                ("volatility", serde_json::json!("0.2")),
                ("time", serde_json::json!(1)),
                ("steps", serde_json::json!(0)),
            ]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_binomial_option(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &float_modes(), "finance.binomial_option")?;
    let kind = option_kind(args)?;
    let spot = number_to_f64(args.number("spot")?, "spot")?;
    let strike = number_to_f64(args.number("strike")?, "strike")?;
    let rate = number_to_f64(args.number("rate")?, "rate")?;
    let volatility = number_to_f64(args.number("volatility")?, "volatility")?;
    let time = number_to_f64(args.number("time")?, "time")?;
    if spot <= 0.0 {
        return Err(EngineError::domain("spot must be positive"));
    }
    if strike <= 0.0 {
        return Err(EngineError::domain("strike must be positive"));
    }
    if volatility < 0.0 {
        return Err(EngineError::domain("volatility must be non-negative"));
    }
    if time <= 0.0 {
        return Err(EngineError::domain("time must be positive"));
    }
    let steps = args.usize_param("steps")?;
    if steps == 0 {
        return Err(EngineError::domain("steps must be at least 1"));
    }
    if steps > MAX_BINOMIAL_STEPS {
        return Err(EngineError::new(
            ErrorCode::ResourceLimit,
            format!("steps={steps} exceeds the binomial limit of {MAX_BINOMIAL_STEPS}"),
        ));
    }
    if steps + 1 > ctx.limits.max_array_len {
        return Err(EngineError::new(
            ErrorCode::ResourceLimit,
            format!(
                "steps={steps} needs {} tree nodes, exceeding the array limit of {}",
                steps + 1,
                ctx.limits.max_array_len
            ),
        ));
    }
    let work = ((steps + 1) as u128) * ((steps + 2) as u128) / 2;
    if work > ctx.limits.max_operations as u128 {
        return Err(EngineError::new(
            ErrorCode::ResourceLimit,
            format!(
                "steps={steps} needs about {work} node updates, exceeding the operation \
                 budget of {}",
                ctx.limits.max_operations
            ),
        ));
    }
    let american = args.optional_bool("american")?.unwrap_or(false);
    let price = crr_price(
        kind, spot, strike, rate, volatility, time, steps, american, ctx,
    )?;
    let value = Value::record([
        ("price", float_value(price)?),
        ("steps", Value::integer(num_bigint::BigInt::from(steps))),
        ("american", Value::Bool(american)),
        ("method", Value::text("crr_binomial")),
    ]);
    Ok(Outcome::approximate(value))
}

#[allow(clippy::too_many_arguments)]
fn crr_price(
    kind: OptionKind,
    spot: f64,
    strike: f64,
    rate: f64,
    volatility: f64,
    time: f64,
    steps: usize,
    american: bool,
    ctx: &ExecContext,
) -> Result<f64, EngineError> {
    let dt = time / steps as f64;
    let discount = exp(-rate * dt);
    let (up, down, probability) = if volatility == 0.0 {
        (1.0, 1.0, 0.5)
    } else {
        let up = exp(volatility * sqrt(dt));
        let down = 1.0 / up;
        let probability = (exp(rate * dt) - down) / (up - down);
        if !(0.0..=1.0).contains(&probability) {
            return Err(EngineError::domain(
                "the risk-neutral probability fell outside [0, 1]; check rate, volatility, \
                 and time",
            ));
        }
        (up, down, probability)
    };
    let ratio = up / down;
    let mut values: Vec<f64> = Vec::with_capacity(steps + 1);
    let mut node_spot = spot * down.powi(steps as i32);
    for _ in 0..=steps {
        values.push(intrinsic(kind, node_spot, strike));
        node_spot *= ratio;
    }
    for level in (0..steps).rev() {
        if level % 256 == 0 {
            ctx.check()?;
        }
        let mut node_spot = spot * down.powi(level as i32);
        for node in 0..=level {
            let continuation =
                discount * (probability * values[node + 1] + (1.0 - probability) * values[node]);
            values[node] = if american {
                continuation.max(intrinsic(kind, node_spot, strike))
            } else {
                continuation
            };
            node_spot *= ratio;
        }
    }
    let price = values[0];
    if !price.is_finite() {
        return Err(EngineError::domain(
            "the binomial tree produced a non-finite price",
        ));
    }
    Ok(price)
}

pub(crate) fn register() -> Vec<std::sync::Arc<dyn bicmath_core::contract::Function>> {
    use bicmath_core::contract::SimpleFunction;
    vec![
        SimpleFunction::arc(black_scholes_descriptor(), invoke_black_scholes),
        SimpleFunction::arc(binomial_option_descriptor(), invoke_binomial_option),
    ]
}
