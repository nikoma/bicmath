//! Portfolio return, risk, and drawdown measures.
//!
//! Return series and weights are exact decimals; sums, covariances, and
//! historical quantiles are computed in exact rational arithmetic and then
//! converted to decimals. The normal value-at-risk method uses a locally
//! implemented normal quantile and is reported rounded.

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{Signed, ToPrimitive, Zero};

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, CostClass, Example, FunctionDescriptor, Outcome, ParamDescriptor,
};
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::number::{Decimal, RoundingMode};
use bicmath_core::schema::ValueSchema;
use bicmath_core::value::Value;

use crate::mathfn::{normal_pdf, normal_quantile, sqrt};
use crate::util::{
    all_modes, enum_schema, exact_array, exact_array_schema, exact_schema, exactness, example_args,
    field, integer_schema, number_to_rational, parse_value, rational_sqrt, rational_to_decimal,
    record_schema, round_rational_to_bigint, text_schema,
};

fn mean(values: &[BigRational]) -> BigRational {
    let mut sum = BigRational::zero();
    for value in values {
        sum += value;
    }
    sum / BigRational::from_integer(BigInt::from(values.len()))
}

fn sample_variance(values: &[BigRational], mean: &BigRational) -> BigRational {
    let mut sum = BigRational::zero();
    for value in values {
        let deviation = value - mean;
        sum += &deviation * &deviation;
    }
    sum / BigRational::from_integer(BigInt::from(values.len() - 1))
}

fn require_matching_lengths(
    left: &[BigRational],
    right: &[BigRational],
    left_name: &str,
    right_name: &str,
) -> Result<(), EngineError> {
    if left.len() != right.len() {
        return Err(EngineError::malformed(format!(
            "{left_name} and {right_name} must have the same length; found {} and {}",
            left.len(),
            right.len()
        )));
    }
    Ok(())
}

fn risk_method(args: &Args) -> Result<&'static str, EngineError> {
    match args.optional_text("method")? {
        None | Some("historical") => Ok("historical"),
        Some("normal") => Ok("normal"),
        Some(other) => Err(EngineError::domain(format!(
            "unknown method {other:?}; expected historical or normal"
        ))),
    }
}

fn confidence_fraction(args: &Args) -> Result<BigRational, EngineError> {
    let confidence = args.decimal("confidence")?;
    let fraction = confidence.to_rational();
    if !fraction.is_positive() || fraction >= BigRational::from_integer(BigInt::from(1)) {
        return Err(EngineError::domain(
            "confidence must be strictly between 0 and 1",
        ));
    }
    Ok(fraction)
}

/// Index of the historical quantile at `1 - confidence`, clamped to the series.
fn historical_index(len: usize, confidence: &BigRational) -> usize {
    let one = BigRational::from_integer(BigInt::from(1));
    let position = (one - confidence) * BigRational::from_integer(BigInt::from(len));
    let index = round_rational_to_bigint(&position, RoundingMode::Floor);
    index.to_usize().unwrap_or(0).min(len - 1)
}

// ---------------------------------------------------------------------------
// Portfolio return and volatility
// ---------------------------------------------------------------------------

fn portfolio_return_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "finance.portfolio_return",
        "finance",
        "1.0.0",
        "Portfolio return",
        "Weighted arithmetic return of a portfolio.",
    )
    .with_description(
        "Computes sum(weights[i] * returns[i]). Weights are not required to sum to 1; \
         they are used exactly as supplied. The result is exact when the weighted sum \
         terminates as a decimal and rounded otherwise.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "weights",
            "Portfolio weights, one per asset.",
            exact_array_schema(1),
        ),
        ParamDescriptor::required(
            "returns",
            "Asset returns, one per asset.",
            exact_array_schema(1),
        ),
    ])
    .with_output(exact_schema(), "Weighted arithmetic return.")
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_units_rule("weights and returns are unitless decimals of the same length")
    .with_method_ref("docs/methods/finance.md#portfolio_return")
    .with_examples(vec![
        Example::new(
            "equal weights",
            example_args(&[
                ("weights", serde_json::json!([0.5, 0.5])),
                ("returns", serde_json::json!([0.1, 0.2])),
            ]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "decimal", "value": "0.15"}),
        )),
        Example::new(
            "length mismatch",
            example_args(&[
                ("weights", serde_json::json!([0.5, 0.5])),
                ("returns", serde_json::json!([0.1])),
            ]),
        )
        .with_error(ErrorCode::MalformedInput),
    ])
}

fn invoke_portfolio_return(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let weights = exact_array(args, "weights")?;
    let returns = exact_array(args, "returns")?;
    require_matching_lengths(&weights, &returns, "weights", "returns")?;
    let mut sum = BigRational::zero();
    for (weight, value) in weights.iter().zip(returns.iter()) {
        sum += weight * value;
    }
    let (decimal, inexact) = rational_to_decimal(&sum, ctx)?;
    Ok(Outcome::new(Value::decimal(decimal), exactness(inexact)))
}

fn portfolio_volatility_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "finance.portfolio_volatility",
        "finance",
        "1.0.0",
        "Portfolio volatility",
        "Standard deviation of portfolio return from weights and a covariance matrix.",
    )
    .with_description(
        "Computes sqrt(w' Sigma w) where w is the weight vector and Sigma the covariance \
         matrix. The matrix must be square with one row per weight and at least one asset; \
         it must be positive semidefinite (a negative portfolio variance is a domain \
         error). The square root is exact when the variance is a perfect decimal square \
         and rounded otherwise.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "weights",
            "Portfolio weights, one per asset.",
            exact_array_schema(1),
        ),
        ParamDescriptor::required(
            "covariance_matrix",
            "Square covariance matrix in the same order as the weights.",
            ValueSchema::Matrix {
                max_elems: None,
                square: true,
            },
        ),
    ])
    .with_output(exact_schema(), "Portfolio volatility sqrt(w' Sigma w).")
    .with_modes(all_modes())
    .with_cost(CostClass::Quadratic)
    .with_units_rule("weights are unitless; the covariance matrix is in squared return units")
    .with_method_ref("docs/methods/finance.md#portfolio_volatility")
    .with_examples(vec![
        Example::new(
            "single asset",
            example_args(&[
                ("weights", serde_json::json!([1])),
                (
                    "covariance_matrix",
                    serde_json::json!({
                        "kind": "matrix",
                        "rows": 1,
                        "cols": 1,
                        "data": [0.04]
                    }),
                ),
            ]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "decimal", "value": "0.2"}),
        )),
        Example::new(
            "weight and matrix size mismatch",
            example_args(&[
                ("weights", serde_json::json!([1])),
                (
                    "covariance_matrix",
                    serde_json::json!({
                        "kind": "matrix",
                        "rows": 2,
                        "cols": 2,
                        "data": [0.04, 0, 0, 0.04]
                    }),
                ),
            ]),
        )
        .with_error(ErrorCode::MalformedInput),
    ])
}

fn invoke_portfolio_volatility(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let weights = exact_array(args, "weights")?;
    let value = args.require("covariance_matrix")?;
    let (rows, cols, data) = match value {
        Value::Matrix { rows, cols, data } => (*rows as usize, *cols as usize, data),
        other => {
            return Err(EngineError::malformed(format!(
                "expected a matrix, found {}",
                other.kind_name()
            )));
        }
    };
    if rows != cols {
        return Err(EngineError::malformed(format!(
            "covariance_matrix must be square, found {rows}x{cols}"
        )));
    }
    if rows != weights.len() {
        return Err(EngineError::malformed(format!(
            "covariance_matrix has {rows} rows but there are {} weights",
            weights.len()
        )));
    }
    if rows == 0 {
        return Err(EngineError::malformed(
            "covariance_matrix must have at least one row",
        ));
    }
    if rows.saturating_mul(cols) > ctx.limits.max_matrix_elements {
        return Err(EngineError::new(
            ErrorCode::ResourceLimit,
            format!(
                "covariance_matrix has {} elements, exceeding the limit of {}",
                rows.saturating_mul(cols),
                ctx.limits.max_matrix_elements
            ),
        ));
    }
    let mut covariance_values = Vec::with_capacity(data.len());
    for (index, item) in data.iter().enumerate() {
        let number = item
            .as_number()
            .map_err(|error| error.with_path(format!("covariance_matrix[{index}]")))?;
        covariance_values.push(number_to_rational(
            number,
            &format!("covariance_matrix[{index}]"),
        )?);
    }
    let mut variance = BigRational::zero();
    for (i, weight_i) in weights.iter().enumerate() {
        for (j, weight_j) in weights.iter().enumerate() {
            variance += weight_i * weight_j * &covariance_values[i * cols + j];
        }
    }
    if variance.is_negative() {
        return Err(EngineError::domain(
            "the covariance matrix produced a negative portfolio variance; it must be \
             positive semidefinite",
        ));
    }
    let (volatility, rounded) = rational_sqrt(&variance, ctx)?;
    Ok(Outcome::new(Value::decimal(volatility), exactness(rounded)))
}

// ---------------------------------------------------------------------------
// Sharpe ratio
// ---------------------------------------------------------------------------

fn sharpe_ratio_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "finance.sharpe_ratio",
        "finance",
        "1.0.0",
        "Sharpe ratio",
        "Excess mean return per unit of sample standard deviation.",
    )
    .with_description(
        "(mean(returns) - risk_free_rate) / sample_standard_deviation(returns), where the \
         standard deviation uses the n-1 denominator. risk_free_rate defaults to 0. At \
         least two observations are required and the sample standard deviation must be \
         positive. The result is exact when the square root and ratio terminate as \
         decimals and rounded otherwise.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "returns",
            "Return series; at least two observations.",
            exact_array_schema(2),
        ),
        ParamDescriptor::optional(
            "risk_free_rate",
            "Per-period risk-free rate; default 0.",
            exact_schema(),
        ),
    ])
    .with_output(exact_schema(), "Sharpe ratio.")
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_units_rule("returns and risk_free_rate are per-period decimals")
    .with_method_ref("docs/methods/finance.md#sharpe_ratio")
    .with_examples(vec![
        Example::new(
            "zero risk-free rate",
            example_args(&[("returns", serde_json::json!([0.1, 0.2, 0.3]))]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "decimal", "value": "2"}),
        )),
        Example::new(
            "constant returns have no risk",
            example_args(&[("returns", serde_json::json!([0.1, 0.1]))]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_sharpe_ratio(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let returns = exact_array(args, "returns")?;
    let risk_free = match args.optional_decimal("risk_free_rate")? {
        Some(value) => value.to_rational(),
        None => BigRational::zero(),
    };
    let returns_mean = mean(&returns);
    let variance = sample_variance(&returns, &returns_mean);
    let (deviation, deviation_rounded) = rational_sqrt(&variance, ctx)?;
    if deviation.is_zero() {
        return Err(EngineError::domain(
            "the sample standard deviation is zero; the Sharpe ratio is undefined",
        ));
    }
    let excess = returns_mean - risk_free;
    if deviation_rounded {
        let excess_f = excess
            .to_f64()
            .ok_or_else(|| EngineError::domain("mean excess return is not representable"))?;
        let deviation_f = deviation
            .to_f64()
            .ok_or_else(|| EngineError::domain("standard deviation is not representable"))?;
        let ratio = Decimal::from_f64_display(excess_f / deviation_f)
            .ok_or_else(|| EngineError::domain("Sharpe ratio is not representable"))?;
        return Ok(Outcome::rounded(Value::decimal(ratio)));
    }
    let ratio = excess / deviation.to_rational();
    let (decimal, inexact) = rational_to_decimal(&ratio, ctx)?;
    Ok(Outcome::new(Value::decimal(decimal), exactness(inexact)))
}

// ---------------------------------------------------------------------------
// Value at risk
// ---------------------------------------------------------------------------

fn value_at_risk_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "finance.value_at_risk",
        "finance",
        "1.0.0",
        "Value at risk",
        "Historical or normal value at risk of a return series.",
    )
    .with_description(
        "Returns the loss threshold exceeded with probability 1 - confidence. The \
         historical method uses the floor((1 - confidence) * n)-th smallest return, \
         clamped to the series, and reports its negative. The normal method assumes a \
         normal return distribution and returns sigma * Phi^-1(confidence) - mean, where \
         sigma is the sample standard deviation (n - 1 denominator, at least two \
         observations required). The normal method is rounded; the historical method is \
         exact when the negation terminates as a decimal.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "returns",
            "Return series; at least one observation (two for the normal method).",
            exact_array_schema(1),
        ),
        ParamDescriptor::required(
            "confidence",
            "Confidence level strictly between 0 and 1.",
            exact_schema(),
        ),
        ParamDescriptor::optional(
            "method",
            "historical (default) or normal.",
            enum_schema(&["historical", "normal"]),
        ),
    ])
    .with_output(
        record_schema(vec![
            field("var", exact_schema()),
            field("method", text_schema()),
            field("confidence", exact_schema()),
        ]),
        "Value at risk, method, and the confidence level used.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_units_rule("returns and value at risk are per-period decimals")
    .with_method_ref("docs/methods/finance.md#value_at_risk")
    .with_examples(vec![
        Example::new(
            "historical at eighty percent",
            example_args(&[
                ("returns", serde_json::json!([-0.3, -0.2, -0.1, 0, 0.4])),
                ("confidence", serde_json::json!("0.8")),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "var": {"kind": "decimal", "value": "0.2"},
            "method": "historical",
            "confidence": {"kind": "decimal", "value": "0.8"}
        }))),
        Example::new(
            "normal at the median",
            example_args(&[
                ("returns", serde_json::json!([0.1, -0.1])),
                ("confidence", serde_json::json!("0.5")),
                ("method", serde_json::json!("normal")),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "var": {"kind": "decimal", "value": "0"},
            "method": "normal",
            "confidence": {"kind": "decimal", "value": "0.5"}
        }))),
    ])
}

fn invoke_value_at_risk(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let returns = exact_array(args, "returns")?;
    let confidence = confidence_fraction(args)?;
    let method = risk_method(args)?;
    let var = match method {
        "historical" => {
            let mut sorted = returns.clone();
            sorted.sort();
            let index = historical_index(sorted.len(), &confidence);
            let value = -sorted[index].clone();
            let (decimal, inexact) = rational_to_decimal(&value, ctx)?;
            let outcome = Outcome::new(Value::decimal(decimal), exactness(inexact));
            return finish_risk(outcome, "var", method, args);
        }
        _ => {
            if returns.len() < 2 {
                return Err(EngineError::new(
                    ErrorCode::InsufficientObservations,
                    "the normal method requires at least two observations",
                ));
            }
            let returns_mean = mean(&returns);
            let variance = sample_variance(&returns, &returns_mean);
            let deviation = sqrt(
                variance
                    .to_f64()
                    .ok_or_else(|| EngineError::domain("variance is not representable"))?,
            );
            let confidence_f = confidence
                .to_f64()
                .ok_or_else(|| EngineError::domain("confidence is not representable"))?;
            let mean_f = returns_mean
                .to_f64()
                .ok_or_else(|| EngineError::domain("mean is not representable"))?;
            let z = normal_quantile(confidence_f)?;
            let var_f = deviation * z - mean_f;
            Decimal::from_f64_display(var_f)
                .ok_or_else(|| EngineError::domain("value at risk is not representable"))?
        }
    };
    let outcome = Outcome::rounded(Value::decimal(var));
    finish_risk(outcome, "var", method, args)
}

fn conditional_value_at_risk_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "finance.conditional_value_at_risk",
        "finance",
        "1.0.0",
        "Conditional value at risk",
        "Expected shortfall beyond the value-at-risk threshold.",
    )
    .with_description(
        "Expected loss conditional on exceeding the value-at-risk threshold. The \
         historical method averages every return at or below the historical quantile; the \
         normal method returns sigma * phi(Phi^-1(confidence)) / (1 - confidence) - mean, \
         where sigma is the sample standard deviation. The normal method is rounded; the \
         historical method is exact when the average terminates as a decimal.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "returns",
            "Return series; at least one observation (two for the normal method).",
            exact_array_schema(1),
        ),
        ParamDescriptor::required(
            "confidence",
            "Confidence level strictly between 0 and 1.",
            exact_schema(),
        ),
        ParamDescriptor::optional(
            "method",
            "historical (default) or normal.",
            enum_schema(&["historical", "normal"]),
        ),
    ])
    .with_output(
        record_schema(vec![
            field("cvar", exact_schema()),
            field("method", text_schema()),
            field("confidence", exact_schema()),
        ]),
        "Conditional value at risk, method, and the confidence level used.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_units_rule("returns and conditional value at risk are per-period decimals")
    .with_method_ref("docs/methods/finance.md#conditional_value_at_risk")
    .with_examples(vec![
        Example::new(
            "historical expected shortfall",
            example_args(&[
                ("returns", serde_json::json!([-0.3, -0.2, -0.1, 0, 0.4])),
                ("confidence", serde_json::json!("0.8")),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "cvar": {"kind": "decimal", "value": "0.25"},
            "method": "historical",
            "confidence": {"kind": "decimal", "value": "0.8"}
        }))),
        Example::new(
            "normal expected shortfall",
            example_args(&[
                ("returns", serde_json::json!([-0.3, -0.2, -0.1, 0, 0.4])),
                ("confidence", serde_json::json!("0.95")),
                ("method", serde_json::json!("normal")),
            ]),
        )
        .with_contains("normal"),
    ])
}

fn invoke_conditional_value_at_risk(
    args: &Args,
    ctx: &ExecContext,
) -> Result<Outcome, EngineError> {
    let returns = exact_array(args, "returns")?;
    let confidence = confidence_fraction(args)?;
    let method = risk_method(args)?;
    let cvar = match method {
        "historical" => {
            let mut sorted = returns.clone();
            sorted.sort();
            let index = historical_index(sorted.len(), &confidence);
            let threshold = sorted[index].clone();
            let mut sum = BigRational::zero();
            let mut count = 0usize;
            for value in &sorted {
                if value <= &threshold {
                    sum += value;
                    count += 1;
                }
            }
            let value = -(sum / BigRational::from_integer(BigInt::from(count)));
            let (decimal, inexact) = rational_to_decimal(&value, ctx)?;
            let outcome = Outcome::new(Value::decimal(decimal), exactness(inexact));
            return finish_risk(outcome, "cvar", method, args);
        }
        _ => {
            if returns.len() < 2 {
                return Err(EngineError::new(
                    ErrorCode::InsufficientObservations,
                    "the normal method requires at least two observations",
                ));
            }
            let returns_mean = mean(&returns);
            let variance = sample_variance(&returns, &returns_mean);
            let deviation = sqrt(
                variance
                    .to_f64()
                    .ok_or_else(|| EngineError::domain("variance is not representable"))?,
            );
            let confidence_f = confidence
                .to_f64()
                .ok_or_else(|| EngineError::domain("confidence is not representable"))?;
            let mean_f = returns_mean
                .to_f64()
                .ok_or_else(|| EngineError::domain("mean is not representable"))?;
            let z = normal_quantile(confidence_f)?;
            let cvar_f = deviation * normal_pdf(z) / (1.0 - confidence_f) - mean_f;
            Decimal::from_f64_display(cvar_f).ok_or_else(|| {
                EngineError::domain("conditional value at risk is not representable")
            })?
        }
    };
    let outcome = Outcome::rounded(Value::decimal(cvar));
    finish_risk(outcome, "cvar", method, args)
}

fn finish_risk(
    outcome: Outcome,
    field_name: &str,
    method: &str,
    args: &Args,
) -> Result<Outcome, EngineError> {
    let confidence = args.decimal("confidence")?;
    let value = match outcome.value {
        Value::Number(number) => Value::record([
            (field_name, Value::Number(number)),
            ("method", Value::text(method)),
            ("confidence", Value::decimal(confidence)),
        ]),
        other => {
            return Err(EngineError::internal(format!(
                "unexpected risk result {}",
                other.kind_name()
            )));
        }
    };
    Ok(Outcome::new(value, outcome.exactness))
}

// ---------------------------------------------------------------------------
// Maximum drawdown
// ---------------------------------------------------------------------------

fn max_drawdown_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "finance.max_drawdown",
        "finance",
        "1.0.0",
        "Maximum drawdown",
        "Largest peak-to-trough decline of a compounded equity curve.",
    )
    .with_description(
        "Builds the equity curve equity[0] = 1 and equity[i] = equity[i-1] * \
         (1 + returns[i-1]), then reports the largest fractional decline \
         (peak - equity[i]) / peak observed at any point. peak_index and trough_index are \
         equity-curve indices, so peak_index 0 is the starting value. Every return must be \
         greater than -1 so the equity curve stays positive. If the curve never declines, \
         the drawdown is 0 and both indices are 0.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "returns",
        "Return series; at least one observation.",
        exact_array_schema(1),
    )])
    .with_output(
        record_schema(vec![
            field("max_drawdown", exact_schema()),
            field("peak_index", integer_schema()),
            field("trough_index", integer_schema()),
            field("method", text_schema()),
        ]),
        "Maximum drawdown, peak index, trough index, and method.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_units_rule("returns are per-period decimals; the drawdown is a unitless fraction")
    .with_method_ref("docs/methods/finance.md#max_drawdown")
    .with_examples(vec![
        Example::new(
            "peak then trough",
            example_args(&[("returns", serde_json::json!([0.1, -0.2, 0.05, -0.3, 0.4]))]),
        )
        .with_value(parse_value(serde_json::json!({
            "max_drawdown": {"kind": "decimal", "value": "0.412"},
            "peak_index": {"kind": "integer", "value": "1"},
            "trough_index": {"kind": "integer", "value": "4"},
            "method": "max_drawdown"
        }))),
        Example::new(
            "monotonic gains",
            example_args(&[("returns", serde_json::json!([0.1, 0.2]))]),
        )
        .with_value(parse_value(serde_json::json!({
            "max_drawdown": {"kind": "decimal", "value": "0"},
            "peak_index": {"kind": "integer", "value": "0"},
            "trough_index": {"kind": "integer", "value": "0"},
            "method": "max_drawdown"
        }))),
    ])
}

fn invoke_max_drawdown(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let returns = exact_array(args, "returns")?;
    let one = BigRational::from_integer(BigInt::from(1));
    let mut equity = Vec::with_capacity(returns.len() + 1);
    equity.push(one.clone());
    for value in &returns {
        let growth = &one + value;
        if !growth.is_positive() {
            return Err(EngineError::domain(
                "every return must be greater than -1 so the equity curve stays positive",
            ));
        }
        let next = equity.last().expect("equity curve has at least one point") * growth;
        equity.push(next);
    }
    let mut peak = equity[0].clone();
    let mut peak_index = 0usize;
    let mut max_drawdown = BigRational::zero();
    let mut drawdown_peak = 0usize;
    let mut drawdown_trough = 0usize;
    for (index, value) in equity.iter().enumerate() {
        if value > &peak {
            peak = value.clone();
            peak_index = index;
        }
        let drawdown = (&peak - value) / &peak;
        if drawdown > max_drawdown {
            max_drawdown = drawdown;
            drawdown_peak = peak_index;
            drawdown_trough = index;
        }
    }
    let (decimal, inexact) = rational_to_decimal(&max_drawdown, ctx)?;
    let value = Value::record([
        ("max_drawdown", Value::decimal(decimal)),
        ("peak_index", Value::integer(BigInt::from(drawdown_peak))),
        (
            "trough_index",
            Value::integer(BigInt::from(drawdown_trough)),
        ),
        ("method", Value::text("max_drawdown")),
    ]);
    Ok(Outcome::new(value, exactness(inexact)))
}

pub(crate) fn register() -> Vec<std::sync::Arc<dyn bicmath_core::contract::Function>> {
    use bicmath_core::contract::SimpleFunction;
    vec![
        SimpleFunction::arc(portfolio_return_descriptor(), invoke_portfolio_return),
        SimpleFunction::arc(
            portfolio_volatility_descriptor(),
            invoke_portfolio_volatility,
        ),
        SimpleFunction::arc(sharpe_ratio_descriptor(), invoke_sharpe_ratio),
        SimpleFunction::arc(value_at_risk_descriptor(), invoke_value_at_risk),
        SimpleFunction::arc(
            conditional_value_at_risk_descriptor(),
            invoke_conditional_value_at_risk,
        ),
        SimpleFunction::arc(max_drawdown_descriptor(), invoke_max_drawdown),
    ]
}
