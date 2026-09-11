//! Continuous distributions: exponential, uniform, log-normal, gamma, beta,
//! and F.
//!
//! All functions produce float64 results and require auto or scientific mode.
//! Densities are evaluated in log space and survival functions are computed
//! directly from the regularized incomplete gamma and beta functions (never as
//! `1 - cdf`), so the upper tails keep relative accuracy. Invalid parameters are
//! rejected with a domain violation.

use std::sync::Arc;

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, CostClass, Example, FunctionDescriptor, Outcome, ParamDescriptor, SimpleFunction,
    require_mode,
};
use bicmath_core::error::{EngineError, ErrorCode};

use crate::common::*;
use crate::mathfn::{
    beta_cdf, beta_pdf, beta_quantile, exponential_cdf, exponential_pdf, exponential_quantile,
    exponential_sf, f_cdf, f_pdf, f_quantile, gamma_cdf, gamma_pdf, gamma_quantile, lognormal_cdf,
    lognormal_pdf, lognormal_quantile, uniform_cdf, uniform_pdf, uniform_quantile,
};

fn descriptor(
    id: &str,
    title: &str,
    summary: &str,
    description: &str,
    parameters: Vec<ParamDescriptor>,
    cost: CostClass,
    examples: Vec<Example>,
) -> FunctionDescriptor {
    FunctionDescriptor::new(id, "statistics", "1.0.0", title, summary)
        .with_description(description)
        .with_parameters(parameters)
        .with_output(float64_schema(), "Distribution value.")
        .with_modes(inferential_modes())
        .with_cost(cost)
        .with_method_ref(format!(
            "docs/methods/statistics.md#{}",
            id.rsplit('.').next().unwrap()
        ))
        .with_examples(examples)
}

fn x_rate_params(x_description: &str, rate_description: &str) -> Vec<ParamDescriptor> {
    vec![
        ParamDescriptor::required("x", x_description, any_number_schema()),
        ParamDescriptor::required("rate", rate_description, any_number_schema()),
    ]
}

fn x_lower_upper_params() -> Vec<ParamDescriptor> {
    vec![
        ParamDescriptor::required("x", "Quantile.", any_number_schema()),
        ParamDescriptor::required("lower", "Lower bound.", any_number_schema()),
        ParamDescriptor::required(
            "upper",
            "Upper bound; must be > lower.",
            any_number_schema(),
        ),
    ]
}

fn x_mu_sigma_params() -> Vec<ParamDescriptor> {
    vec![
        ParamDescriptor::required("x", "Quantile.", any_number_schema()),
        ParamDescriptor::optional(
            "mu",
            "Mean of the logarithm; default 0.",
            any_number_schema(),
        ),
        ParamDescriptor::optional(
            "sigma",
            "Standard deviation of the logarithm; must be > 0; default 1.",
            any_number_schema(),
        ),
    ]
}

fn x_shape_rate_params() -> Vec<ParamDescriptor> {
    vec![
        ParamDescriptor::required("x", "Quantile.", any_number_schema()),
        ParamDescriptor::required(
            "shape",
            "Shape parameter; must be > 0.",
            any_number_schema(),
        ),
        ParamDescriptor::optional(
            "rate",
            "Rate parameter; must be > 0; default 1.",
            any_number_schema(),
        ),
    ]
}

fn x_alpha_beta_params() -> Vec<ParamDescriptor> {
    vec![
        ParamDescriptor::required("x", "Quantile in [0, 1].", any_number_schema()),
        ParamDescriptor::required(
            "alpha",
            "First shape parameter; must be > 0.",
            any_number_schema(),
        ),
        ParamDescriptor::required(
            "beta",
            "Second shape parameter; must be > 0.",
            any_number_schema(),
        ),
    ]
}

fn x_df1_df2_params() -> Vec<ParamDescriptor> {
    vec![
        ParamDescriptor::required("x", "Quantile.", any_number_schema()),
        ParamDescriptor::required(
            "df1",
            "Numerator degrees of freedom; must be > 0.",
            any_number_schema(),
        ),
        ParamDescriptor::required(
            "df2",
            "Denominator degrees of freedom; must be > 0.",
            any_number_schema(),
        ),
    ]
}

// ---------------------------------------------------------------------------
// Exponential
// ---------------------------------------------------------------------------

fn exponential_pdf_descriptor() -> FunctionDescriptor {
    descriptor(
        "statistics.exponential_pdf",
        "Exponential probability density",
        "Probability density of the exponential distribution.",
        "pdf(x) = rate * e^(-rate * x) for x >= 0 and 0 for x < 0. rate must be strictly \
         positive.",
        x_rate_params("Quantile.", "Rate parameter; must be > 0."),
        CostClass::Constant,
        vec![
            Example::new(
                "density at zero",
                example_args(&[("x", serde_json::json!(0)), ("rate", serde_json::json!(1))]),
            )
            .with_value(parse_value(
                serde_json::json!({"kind": "float64", "value": "1"}),
            )),
            Example::new(
                "zero rate",
                example_args(&[("x", serde_json::json!(1)), ("rate", serde_json::json!(0))]),
            )
            .with_error(ErrorCode::DomainViolation),
        ],
    )
}

fn exponential_cdf_descriptor() -> FunctionDescriptor {
    descriptor(
        "statistics.exponential_cdf",
        "Exponential cumulative distribution",
        "Cumulative distribution function of the exponential distribution.",
        "cdf(x) = 1 - e^(-rate * x) for x >= 0 and 0 for x < 0. rate must be strictly \
         positive.",
        x_rate_params("Quantile.", "Rate parameter; must be > 0."),
        CostClass::Constant,
        vec![
            Example::new(
                "cdf at zero",
                example_args(&[("x", serde_json::json!(0)), ("rate", serde_json::json!(1))]),
            )
            .with_value(parse_value(
                serde_json::json!({"kind": "float64", "value": "0"}),
            )),
            Example::new(
                "zero rate",
                example_args(&[("x", serde_json::json!(1)), ("rate", serde_json::json!(0))]),
            )
            .with_error(ErrorCode::DomainViolation),
        ],
    )
}

fn exponential_sf_descriptor() -> FunctionDescriptor {
    descriptor(
        "statistics.exponential_sf",
        "Exponential survival function",
        "Upper tail probability of the exponential distribution.",
        "sf(x) = P(X > x) = e^(-rate * x) for x >= 0 and 1 for x < 0. The tail is computed \
         from exp directly, never as 1 - cdf, so large x keeps relative accuracy.",
        x_rate_params("Quantile.", "Rate parameter; must be > 0."),
        CostClass::Constant,
        vec![
            Example::new(
                "survival at zero",
                example_args(&[("x", serde_json::json!(0)), ("rate", serde_json::json!(1))]),
            )
            .with_value(parse_value(
                serde_json::json!({"kind": "float64", "value": "1"}),
            )),
            Example::new(
                "zero rate",
                example_args(&[("x", serde_json::json!(1)), ("rate", serde_json::json!(0))]),
            )
            .with_error(ErrorCode::DomainViolation),
        ],
    )
}

fn exponential_quantile_descriptor() -> FunctionDescriptor {
    descriptor(
        "statistics.exponential_quantile",
        "Exponential quantile",
        "Inverse exponential CDF.",
        "Returns x such that cdf(x) = p as -ln(1 - p) / rate. p must be in [0, 1); p = 1 is \
         rejected because the quantile is unbounded. rate must be strictly positive.",
        vec![
            ParamDescriptor::required("p", "Probability in [0, 1).", any_number_schema()),
            ParamDescriptor::required("rate", "Rate parameter; must be > 0.", any_number_schema()),
        ],
        CostClass::Constant,
        vec![
            Example::new(
                "zero probability",
                example_args(&[("p", serde_json::json!(0)), ("rate", serde_json::json!(1))]),
            )
            .with_value(parse_value(
                serde_json::json!({"kind": "float64", "value": "0"}),
            )),
            Example::new(
                "unbounded probability",
                example_args(&[("p", serde_json::json!(1)), ("rate", serde_json::json!(1))]),
            )
            .with_error(ErrorCode::DomainViolation),
        ],
    )
}

fn invoke_exponential_pdf(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.exponential_pdf")?;
    let x = scalar_f64(args, "x")?;
    let rate = scalar_f64(args, "rate")?;
    Ok(Outcome::approximate(float_value(exponential_pdf(
        x, rate,
    )?)?))
}

fn invoke_exponential_cdf(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.exponential_cdf")?;
    let x = scalar_f64(args, "x")?;
    let rate = scalar_f64(args, "rate")?;
    Ok(Outcome::approximate(float_value(exponential_cdf(
        x, rate,
    )?)?))
}

fn invoke_exponential_sf(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.exponential_sf")?;
    let x = scalar_f64(args, "x")?;
    let rate = scalar_f64(args, "rate")?;
    Ok(Outcome::approximate(float_value(exponential_sf(x, rate)?)?))
}

fn invoke_exponential_quantile(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.exponential_quantile")?;
    let p = scalar_f64(args, "p")?;
    let rate = scalar_f64(args, "rate")?;
    Ok(Outcome::approximate(float_value(exponential_quantile(
        p, rate,
    )?)?))
}

// ---------------------------------------------------------------------------
// Uniform
// ---------------------------------------------------------------------------

fn uniform_pdf_descriptor() -> FunctionDescriptor {
    descriptor(
        "statistics.uniform_pdf",
        "Uniform probability density",
        "Probability density of the continuous uniform distribution.",
        "pdf(x) = 1 / (upper - lower) for lower <= x <= upper and 0 outside. upper must be \
         strictly greater than lower.",
        x_lower_upper_params(),
        CostClass::Constant,
        vec![
            Example::new(
                "unit interval",
                example_args(&[
                    ("x", serde_json::json!(0.5)),
                    ("lower", serde_json::json!(0)),
                    ("upper", serde_json::json!(1)),
                ]),
            )
            .with_value(parse_value(
                serde_json::json!({"kind": "float64", "value": "1"}),
            )),
            Example::new(
                "empty interval",
                example_args(&[
                    ("x", serde_json::json!(0.5)),
                    ("lower", serde_json::json!(1)),
                    ("upper", serde_json::json!(1)),
                ]),
            )
            .with_error(ErrorCode::DomainViolation),
        ],
    )
}

fn uniform_cdf_descriptor() -> FunctionDescriptor {
    descriptor(
        "statistics.uniform_cdf",
        "Uniform cumulative distribution",
        "Cumulative distribution function of the continuous uniform distribution.",
        "cdf(x) = clamp((x - lower) / (upper - lower), 0, 1). upper must be strictly greater \
         than lower.",
        x_lower_upper_params(),
        CostClass::Constant,
        vec![
            Example::new(
                "midpoint",
                example_args(&[
                    ("x", serde_json::json!(0.5)),
                    ("lower", serde_json::json!(0)),
                    ("upper", serde_json::json!(1)),
                ]),
            )
            .with_value(parse_value(
                serde_json::json!({"kind": "float64", "value": "0.5"}),
            )),
            Example::new(
                "empty interval",
                example_args(&[
                    ("x", serde_json::json!(0.5)),
                    ("lower", serde_json::json!(1)),
                    ("upper", serde_json::json!(0)),
                ]),
            )
            .with_error(ErrorCode::DomainViolation),
        ],
    )
}

fn uniform_quantile_descriptor() -> FunctionDescriptor {
    descriptor(
        "statistics.uniform_quantile",
        "Uniform quantile",
        "Inverse continuous uniform CDF.",
        "Returns lower + p * (upper - lower). p must be in [0, 1] and upper must be strictly \
         greater than lower.",
        vec![
            ParamDescriptor::required("p", "Probability in [0, 1].", any_number_schema()),
            ParamDescriptor::required("lower", "Lower bound.", any_number_schema()),
            ParamDescriptor::required(
                "upper",
                "Upper bound; must be > lower.",
                any_number_schema(),
            ),
        ],
        CostClass::Constant,
        vec![
            Example::new(
                "median",
                example_args(&[
                    ("p", serde_json::json!(0.5)),
                    ("lower", serde_json::json!(0)),
                    ("upper", serde_json::json!(1)),
                ]),
            )
            .with_value(parse_value(
                serde_json::json!({"kind": "float64", "value": "0.5"}),
            )),
            Example::new(
                "probability above one",
                example_args(&[
                    ("p", serde_json::json!(1.5)),
                    ("lower", serde_json::json!(0)),
                    ("upper", serde_json::json!(1)),
                ]),
            )
            .with_error(ErrorCode::DomainViolation),
        ],
    )
}

fn invoke_uniform_pdf(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.uniform_pdf")?;
    let x = scalar_f64(args, "x")?;
    let lower = scalar_f64(args, "lower")?;
    let upper = scalar_f64(args, "upper")?;
    Ok(Outcome::approximate(float_value(uniform_pdf(
        x, lower, upper,
    )?)?))
}

fn invoke_uniform_cdf(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.uniform_cdf")?;
    let x = scalar_f64(args, "x")?;
    let lower = scalar_f64(args, "lower")?;
    let upper = scalar_f64(args, "upper")?;
    Ok(Outcome::approximate(float_value(uniform_cdf(
        x, lower, upper,
    )?)?))
}

fn invoke_uniform_quantile(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.uniform_quantile")?;
    let p = scalar_f64(args, "p")?;
    let lower = scalar_f64(args, "lower")?;
    let upper = scalar_f64(args, "upper")?;
    Ok(Outcome::approximate(float_value(uniform_quantile(
        p, lower, upper,
    )?)?))
}

// ---------------------------------------------------------------------------
// Log-normal
// ---------------------------------------------------------------------------

fn lognormal_pdf_descriptor() -> FunctionDescriptor {
    descriptor(
        "statistics.lognormal_pdf",
        "Log-normal probability density",
        "Probability density of the log-normal distribution.",
        "pdf(x) = e^(-0.5 z^2) / (x * sigma * sqrt(2 pi)) with z = (ln x - mu) / sigma, for \
         x > 0 and 0 for x <= 0. sigma must be strictly positive.",
        x_mu_sigma_params(),
        CostClass::Constant,
        vec![
            Example::new(
                "non-positive quantile",
                example_args(&[("x", serde_json::json!(0))]),
            )
            .with_value(parse_value(
                serde_json::json!({"kind": "float64", "value": "0"}),
            )),
            Example::new(
                "zero sigma",
                example_args(&[("x", serde_json::json!(1)), ("sigma", serde_json::json!(0))]),
            )
            .with_error(ErrorCode::DomainViolation),
        ],
    )
}

fn lognormal_cdf_descriptor() -> FunctionDescriptor {
    descriptor(
        "statistics.lognormal_cdf",
        "Log-normal cumulative distribution",
        "Cumulative distribution function of the log-normal distribution.",
        "cdf(x) = Phi((ln x - mu) / sigma) for x > 0 and 0 for x <= 0, where Phi is the \
         standard normal CDF evaluated with a high-accuracy erfc. sigma must be strictly \
         positive.",
        x_mu_sigma_params(),
        CostClass::Constant,
        vec![
            Example::new("median", example_args(&[("x", serde_json::json!(1))])).with_value(
                parse_value(serde_json::json!({"kind": "float64", "value": "0.5"})),
            ),
            Example::new(
                "zero sigma",
                example_args(&[("x", serde_json::json!(1)), ("sigma", serde_json::json!(0))]),
            )
            .with_error(ErrorCode::DomainViolation),
        ],
    )
}

fn lognormal_quantile_descriptor() -> FunctionDescriptor {
    descriptor(
        "statistics.lognormal_quantile",
        "Log-normal quantile",
        "Inverse log-normal CDF.",
        "Returns exp(mu + sigma * z_p) where z_p is the standard normal quantile. p must be \
         strictly between 0 and 1; mu defaults to 0 and sigma to 1 and must be positive.",
        vec![
            ParamDescriptor::required("p", "Probability in (0, 1).", any_number_schema()),
            ParamDescriptor::optional(
                "mu",
                "Mean of the logarithm; default 0.",
                any_number_schema(),
            ),
            ParamDescriptor::optional(
                "sigma",
                "Standard deviation of the logarithm; must be > 0; default 1.",
                any_number_schema(),
            ),
        ],
        CostClass::Iterative,
        vec![
            Example::new("median", example_args(&[("p", serde_json::json!(0.5))])).with_value(
                parse_value(serde_json::json!({"kind": "float64", "value": "1"})),
            ),
            Example::new(
                "zero sigma",
                example_args(&[
                    ("p", serde_json::json!(0.5)),
                    ("sigma", serde_json::json!(0)),
                ]),
            )
            .with_error(ErrorCode::DomainViolation),
        ],
    )
}

fn lognormal_parameters(args: &Args) -> Result<(f64, f64, f64), EngineError> {
    let x = scalar_f64(args, "x")?;
    let mu = optional_f64_param(args, "mu")?.unwrap_or(0.0);
    let sigma = optional_f64_param(args, "sigma")?.unwrap_or(1.0);
    Ok((x, mu, sigma))
}

fn invoke_lognormal_pdf(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.lognormal_pdf")?;
    let (x, mu, sigma) = lognormal_parameters(args)?;
    Ok(Outcome::approximate(float_value(lognormal_pdf(
        x, mu, sigma,
    )?)?))
}

fn invoke_lognormal_cdf(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.lognormal_cdf")?;
    let (x, mu, sigma) = lognormal_parameters(args)?;
    Ok(Outcome::approximate(float_value(lognormal_cdf(
        x, mu, sigma,
    )?)?))
}

fn invoke_lognormal_quantile(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.lognormal_quantile")?;
    let p = scalar_f64(args, "p")?;
    let mu = optional_f64_param(args, "mu")?.unwrap_or(0.0);
    let sigma = optional_f64_param(args, "sigma")?.unwrap_or(1.0);
    Ok(Outcome::approximate(float_value(lognormal_quantile(
        p, mu, sigma,
    )?)?))
}

// ---------------------------------------------------------------------------
// Gamma
// ---------------------------------------------------------------------------

fn gamma_pdf_descriptor() -> FunctionDescriptor {
    descriptor(
        "statistics.gamma_pdf",
        "Gamma probability density",
        "Probability density of the gamma distribution.",
        "pdf(x) = rate^shape x^(shape - 1) e^(-rate x) / Gamma(shape) for x > 0, evaluated in \
         log space with a Lanczos log-gamma. For x < 0 the density is 0; at x = 0 it is \
         unbounded for shape < 1, equals rate for shape = 1, and is 0 for shape > 1.",
        x_shape_rate_params(),
        CostClass::Constant,
        vec![
            Example::new(
                "density at zero for shape one",
                example_args(&[
                    ("x", serde_json::json!(0)),
                    ("shape", serde_json::json!(1)),
                    ("rate", serde_json::json!(2)),
                ]),
            )
            .with_value(parse_value(
                serde_json::json!({"kind": "float64", "value": "2"}),
            )),
            Example::new(
                "zero shape",
                example_args(&[("x", serde_json::json!(1)), ("shape", serde_json::json!(0))]),
            )
            .with_error(ErrorCode::DomainViolation),
        ],
    )
}

fn gamma_cdf_descriptor() -> FunctionDescriptor {
    descriptor(
        "statistics.gamma_cdf",
        "Gamma cumulative distribution",
        "Cumulative distribution function of the gamma distribution.",
        "cdf(x) = P(shape, rate * x), the regularized lower incomplete gamma function, \
         evaluated with the series for small arguments and the continued fraction otherwise. \
         x <= 0 returns 0. shape and rate must be strictly positive.",
        x_shape_rate_params(),
        CostClass::Constant,
        vec![
            Example::new(
                "zero quantile",
                example_args(&[("x", serde_json::json!(0)), ("shape", serde_json::json!(2))]),
            )
            .with_value(parse_value(
                serde_json::json!({"kind": "float64", "value": "0"}),
            )),
            Example::new(
                "zero shape",
                example_args(&[("x", serde_json::json!(1)), ("shape", serde_json::json!(0))]),
            )
            .with_error(ErrorCode::DomainViolation),
        ],
    )
}

fn gamma_quantile_descriptor() -> FunctionDescriptor {
    descriptor(
        "statistics.gamma_quantile",
        "Gamma quantile",
        "Inverse gamma CDF.",
        "Returns x such that cdf(x) = p, solved by bracket expansion, bisection, and Newton \
         polishing. p must be in [0, 1); p = 0 returns 0 and p = 1 is rejected because the \
         quantile is unbounded. shape and rate must be strictly positive.",
        vec![
            ParamDescriptor::required("p", "Probability in [0, 1).", any_number_schema()),
            ParamDescriptor::required(
                "shape",
                "Shape parameter; must be > 0.",
                any_number_schema(),
            ),
            ParamDescriptor::optional(
                "rate",
                "Rate parameter; must be > 0; default 1.",
                any_number_schema(),
            ),
        ],
        CostClass::Iterative,
        vec![
            Example::new(
                "zero probability",
                example_args(&[("p", serde_json::json!(0)), ("shape", serde_json::json!(2))]),
            )
            .with_value(parse_value(
                serde_json::json!({"kind": "float64", "value": "0"}),
            )),
            Example::new(
                "zero rate",
                example_args(&[
                    ("p", serde_json::json!(0.5)),
                    ("shape", serde_json::json!(2)),
                    ("rate", serde_json::json!(0)),
                ]),
            )
            .with_error(ErrorCode::DomainViolation),
        ],
    )
}

fn gamma_parameters(args: &Args) -> Result<(f64, f64, f64), EngineError> {
    let x = scalar_f64(args, "x")?;
    let shape = scalar_f64(args, "shape")?;
    let rate = optional_f64_param(args, "rate")?.unwrap_or(1.0);
    Ok((x, shape, rate))
}

fn invoke_gamma_pdf(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.gamma_pdf")?;
    let (x, shape, rate) = gamma_parameters(args)?;
    Ok(Outcome::approximate(float_value(gamma_pdf(
        x, shape, rate,
    )?)?))
}

fn invoke_gamma_cdf(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.gamma_cdf")?;
    let (x, shape, rate) = gamma_parameters(args)?;
    Ok(Outcome::approximate(float_value(gamma_cdf(
        x, shape, rate,
    )?)?))
}

fn invoke_gamma_quantile(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.gamma_quantile")?;
    let p = scalar_f64(args, "p")?;
    let shape = scalar_f64(args, "shape")?;
    let rate = optional_f64_param(args, "rate")?.unwrap_or(1.0);
    Ok(Outcome::approximate(float_value(gamma_quantile(
        p, shape, rate,
    )?)?))
}

// ---------------------------------------------------------------------------
// Beta
// ---------------------------------------------------------------------------

fn beta_pdf_descriptor() -> FunctionDescriptor {
    descriptor(
        "statistics.beta_pdf",
        "Beta probability density",
        "Probability density of the beta distribution.",
        "pdf(x) = x^(alpha - 1) (1 - x)^(beta - 1) / B(alpha, beta) for x in [0, 1], evaluated \
         in log space with a Lanczos log-gamma. Outside [0, 1] the density is 0; at the \
         endpoints it is unbounded when the corresponding shape is below 1, equals the other \
         shape when it is 1, and is 0 when it is above 1.",
        x_alpha_beta_params(),
        CostClass::Constant,
        vec![
            Example::new(
                "finite endpoint",
                example_args(&[
                    ("x", serde_json::json!(0)),
                    ("alpha", serde_json::json!(1)),
                    ("beta", serde_json::json!(2)),
                ]),
            )
            .with_value(parse_value(
                serde_json::json!({"kind": "float64", "value": "2"}),
            )),
            Example::new(
                "zero alpha",
                example_args(&[
                    ("x", serde_json::json!(0.5)),
                    ("alpha", serde_json::json!(0)),
                    ("beta", serde_json::json!(2)),
                ]),
            )
            .with_error(ErrorCode::DomainViolation),
        ],
    )
}

fn beta_cdf_descriptor() -> FunctionDescriptor {
    descriptor(
        "statistics.beta_cdf",
        "Beta cumulative distribution",
        "Cumulative distribution function of the beta distribution.",
        "cdf(x) = I_x(alpha, beta), the regularized incomplete beta function. x <= 0 returns 0 \
         and x >= 1 returns 1. alpha and beta must be strictly positive.",
        x_alpha_beta_params(),
        CostClass::Constant,
        vec![
            Example::new(
                "zero quantile",
                example_args(&[
                    ("x", serde_json::json!(0)),
                    ("alpha", serde_json::json!(2)),
                    ("beta", serde_json::json!(2)),
                ]),
            )
            .with_value(parse_value(
                serde_json::json!({"kind": "float64", "value": "0"}),
            )),
            Example::new(
                "zero beta",
                example_args(&[
                    ("x", serde_json::json!(0.5)),
                    ("alpha", serde_json::json!(2)),
                    ("beta", serde_json::json!(0)),
                ]),
            )
            .with_error(ErrorCode::DomainViolation),
        ],
    )
}

fn beta_quantile_descriptor() -> FunctionDescriptor {
    descriptor(
        "statistics.beta_quantile",
        "Beta quantile",
        "Inverse beta CDF.",
        "Returns x such that cdf(x) = p, solved by bracket expansion, bisection, and Newton \
         polishing. p must be in [0, 1]; p = 0 returns 0 and p = 1 returns 1. alpha and beta \
         must be strictly positive.",
        vec![
            ParamDescriptor::required("p", "Probability in [0, 1].", any_number_schema()),
            ParamDescriptor::required(
                "alpha",
                "First shape parameter; must be > 0.",
                any_number_schema(),
            ),
            ParamDescriptor::required(
                "beta",
                "Second shape parameter; must be > 0.",
                any_number_schema(),
            ),
        ],
        CostClass::Iterative,
        vec![
            Example::new(
                "zero probability",
                example_args(&[
                    ("p", serde_json::json!(0)),
                    ("alpha", serde_json::json!(2)),
                    ("beta", serde_json::json!(2)),
                ]),
            )
            .with_value(parse_value(
                serde_json::json!({"kind": "float64", "value": "0"}),
            )),
            Example::new(
                "zero alpha",
                example_args(&[
                    ("p", serde_json::json!(0.5)),
                    ("alpha", serde_json::json!(0)),
                    ("beta", serde_json::json!(2)),
                ]),
            )
            .with_error(ErrorCode::DomainViolation),
        ],
    )
}

fn invoke_beta_pdf(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.beta_pdf")?;
    let x = scalar_f64(args, "x")?;
    let alpha = scalar_f64(args, "alpha")?;
    let beta = scalar_f64(args, "beta")?;
    Ok(Outcome::approximate(float_value(beta_pdf(
        x, alpha, beta,
    )?)?))
}

fn invoke_beta_cdf(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.beta_cdf")?;
    let x = scalar_f64(args, "x")?;
    let alpha = scalar_f64(args, "alpha")?;
    let beta = scalar_f64(args, "beta")?;
    Ok(Outcome::approximate(float_value(beta_cdf(
        x, alpha, beta,
    )?)?))
}

fn invoke_beta_quantile(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.beta_quantile")?;
    let p = scalar_f64(args, "p")?;
    let alpha = scalar_f64(args, "alpha")?;
    let beta = scalar_f64(args, "beta")?;
    Ok(Outcome::approximate(float_value(beta_quantile(
        p, alpha, beta,
    )?)?))
}

// ---------------------------------------------------------------------------
// F
// ---------------------------------------------------------------------------

fn f_pdf_descriptor() -> FunctionDescriptor {
    descriptor(
        "statistics.f_pdf",
        "F probability density",
        "Probability density of the F distribution.",
        "pdf(x) = (df1/df2)^(df1/2) x^(df1/2 - 1) / (B(df1/2, df2/2) (1 + df1 x / df2)^((df1 \
         + df2)/2)) for x > 0, evaluated in log space with a Lanczos log-gamma. For x < 0 the \
         density is 0; at x = 0 it is unbounded for df1 < 2, equals 1 for df1 = 2, and is 0 \
         for df1 > 2.",
        x_df1_df2_params(),
        CostClass::Constant,
        vec![
            Example::new(
                "density at zero for df1 three",
                example_args(&[
                    ("x", serde_json::json!(0)),
                    ("df1", serde_json::json!(3)),
                    ("df2", serde_json::json!(3)),
                ]),
            )
            .with_value(parse_value(
                serde_json::json!({"kind": "float64", "value": "0"}),
            )),
            Example::new(
                "zero degrees of freedom",
                example_args(&[
                    ("x", serde_json::json!(1)),
                    ("df1", serde_json::json!(0)),
                    ("df2", serde_json::json!(1)),
                ]),
            )
            .with_error(ErrorCode::DomainViolation),
        ],
    )
}

fn f_cdf_descriptor() -> FunctionDescriptor {
    descriptor(
        "statistics.f_cdf",
        "F cumulative distribution",
        "Cumulative distribution function of the F distribution.",
        "cdf(x) = I_y(df1/2, df2/2) with y = df1 x / (df1 x + df2), the regularized \
         incomplete beta function. x <= 0 returns 0. df1 and df2 must be strictly positive.",
        x_df1_df2_params(),
        CostClass::Constant,
        vec![
            Example::new(
                "zero quantile",
                example_args(&[
                    ("x", serde_json::json!(0)),
                    ("df1", serde_json::json!(1)),
                    ("df2", serde_json::json!(1)),
                ]),
            )
            .with_value(parse_value(
                serde_json::json!({"kind": "float64", "value": "0"}),
            )),
            Example::new(
                "zero denominator degrees of freedom",
                example_args(&[
                    ("x", serde_json::json!(1)),
                    ("df1", serde_json::json!(1)),
                    ("df2", serde_json::json!(0)),
                ]),
            )
            .with_error(ErrorCode::DomainViolation),
        ],
    )
}

fn f_quantile_descriptor() -> FunctionDescriptor {
    descriptor(
        "statistics.f_quantile",
        "F quantile",
        "Inverse F CDF.",
        "Returns x such that cdf(x) = p, computed from the beta quantile q = \
         I^(-1)_p(df1/2, df2/2) as x = df2 q / (df1 (1 - q)). p must be in [0, 1); p = 0 \
         returns 0 and p = 1 is rejected because the quantile is unbounded. df1 and df2 must \
         be strictly positive.",
        vec![
            ParamDescriptor::required("p", "Probability in [0, 1).", any_number_schema()),
            ParamDescriptor::required(
                "df1",
                "Numerator degrees of freedom; must be > 0.",
                any_number_schema(),
            ),
            ParamDescriptor::required(
                "df2",
                "Denominator degrees of freedom; must be > 0.",
                any_number_schema(),
            ),
        ],
        CostClass::Iterative,
        vec![
            Example::new(
                "zero probability",
                example_args(&[
                    ("p", serde_json::json!(0)),
                    ("df1", serde_json::json!(1)),
                    ("df2", serde_json::json!(1)),
                ]),
            )
            .with_value(parse_value(
                serde_json::json!({"kind": "float64", "value": "0"}),
            )),
            Example::new(
                "zero numerator degrees of freedom",
                example_args(&[
                    ("p", serde_json::json!(0.5)),
                    ("df1", serde_json::json!(0)),
                    ("df2", serde_json::json!(1)),
                ]),
            )
            .with_error(ErrorCode::DomainViolation),
        ],
    )
}

fn invoke_f_pdf(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.f_pdf")?;
    let x = scalar_f64(args, "x")?;
    let df1 = scalar_f64(args, "df1")?;
    let df2 = scalar_f64(args, "df2")?;
    Ok(Outcome::approximate(float_value(f_pdf(x, df1, df2)?)?))
}

fn invoke_f_cdf(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.f_cdf")?;
    let x = scalar_f64(args, "x")?;
    let df1 = scalar_f64(args, "df1")?;
    let df2 = scalar_f64(args, "df2")?;
    Ok(Outcome::approximate(float_value(f_cdf(x, df1, df2)?)?))
}

fn invoke_f_quantile(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.f_quantile")?;
    let p = scalar_f64(args, "p")?;
    let df1 = scalar_f64(args, "df1")?;
    let df2 = scalar_f64(args, "df2")?;
    Ok(Outcome::approximate(float_value(f_quantile(p, df1, df2)?)?))
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

pub fn functions() -> Vec<Arc<dyn bicmath_core::contract::Function>> {
    vec![
        SimpleFunction::arc(exponential_pdf_descriptor(), invoke_exponential_pdf),
        SimpleFunction::arc(exponential_cdf_descriptor(), invoke_exponential_cdf),
        SimpleFunction::arc(exponential_sf_descriptor(), invoke_exponential_sf),
        SimpleFunction::arc(
            exponential_quantile_descriptor(),
            invoke_exponential_quantile,
        ),
        SimpleFunction::arc(uniform_pdf_descriptor(), invoke_uniform_pdf),
        SimpleFunction::arc(uniform_cdf_descriptor(), invoke_uniform_cdf),
        SimpleFunction::arc(uniform_quantile_descriptor(), invoke_uniform_quantile),
        SimpleFunction::arc(lognormal_pdf_descriptor(), invoke_lognormal_pdf),
        SimpleFunction::arc(lognormal_cdf_descriptor(), invoke_lognormal_cdf),
        SimpleFunction::arc(lognormal_quantile_descriptor(), invoke_lognormal_quantile),
        SimpleFunction::arc(gamma_pdf_descriptor(), invoke_gamma_pdf),
        SimpleFunction::arc(gamma_cdf_descriptor(), invoke_gamma_cdf),
        SimpleFunction::arc(gamma_quantile_descriptor(), invoke_gamma_quantile),
        SimpleFunction::arc(beta_pdf_descriptor(), invoke_beta_pdf),
        SimpleFunction::arc(beta_cdf_descriptor(), invoke_beta_cdf),
        SimpleFunction::arc(beta_quantile_descriptor(), invoke_beta_quantile),
        SimpleFunction::arc(f_pdf_descriptor(), invoke_f_pdf),
        SimpleFunction::arc(f_cdf_descriptor(), invoke_f_cdf),
        SimpleFunction::arc(f_quantile_descriptor(), invoke_f_quantile),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use bicmath_core::value::Value;
    use std::collections::BTreeMap;

    fn call(id: &str, raw: serde_json::Value) -> Result<Outcome, EngineError> {
        let module = crate::module();
        let function = module
            .functions
            .iter()
            .find(|f| f.descriptor().id == id)
            .expect("function exists");
        let ctx = ExecContext::scientific();
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
                    .coerce(value, name, &ctx.limits, true)
                    .expect("argument coerces"),
            );
        }
        function.invoke(&Args::new(values), &ctx)
    }

    fn as_f64(outcome: &Outcome) -> f64 {
        match &outcome.value {
            Value::Number(number) => number.to_f64().expect("number"),
            other => panic!("expected number, got {other:?}"),
        }
    }

    fn close(actual: f64, expected: f64, tolerance: f64) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "expected {expected}, got {actual} (tolerance {tolerance})"
        );
    }

    #[test]
    fn exponential_reference_values() {
        // Provenance: Python 3.14 stdlib, 1 - exp(-1), exp(-1), and -log(1 - 0.5)
        // (ln 2, the f64 constant).
        close(
            as_f64(
                &call(
                    "statistics.exponential_cdf",
                    serde_json::json!({"x": 1, "rate": 1}),
                )
                .unwrap(),
            ),
            0.632_120_558_828_557_7,
            1e-15,
        );
        close(
            as_f64(
                &call(
                    "statistics.exponential_sf",
                    serde_json::json!({"x": 1, "rate": 1}),
                )
                .unwrap(),
            ),
            0.367_879_441_171_442_33,
            1e-15,
        );
        close(
            as_f64(
                &call(
                    "statistics.exponential_quantile",
                    serde_json::json!({"p": 0.5, "rate": 1}),
                )
                .unwrap(),
            ),
            std::f64::consts::LN_2,
            1e-15,
        );
        close(
            as_f64(
                &call(
                    "statistics.exponential_pdf",
                    serde_json::json!({"x": 1, "rate": 1}),
                )
                .unwrap(),
            ),
            0.367_879_441_171_442_33,
            1e-15,
        );
    }

    #[test]
    fn uniform_reference_values() {
        close(
            as_f64(
                &call(
                    "statistics.uniform_cdf",
                    serde_json::json!({"x": 0.5, "lower": 0, "upper": 1}),
                )
                .unwrap(),
            ),
            0.5,
            0.0,
        );
        close(
            as_f64(
                &call(
                    "statistics.uniform_pdf",
                    serde_json::json!({"x": 0.5, "lower": 0, "upper": 2}),
                )
                .unwrap(),
            ),
            0.5,
            0.0,
        );
        close(
            as_f64(
                &call(
                    "statistics.uniform_quantile",
                    serde_json::json!({"p": 0.25, "lower": -1, "upper": 3}),
                )
                .unwrap(),
            ),
            0.0,
            0.0,
        );
    }

    #[test]
    fn lognormal_reference_values() {
        close(
            as_f64(&call("statistics.lognormal_cdf", serde_json::json!({"x": 1})).unwrap()),
            0.5,
            0.0,
        );
        close(
            as_f64(
                &call(
                    "statistics.lognormal_quantile",
                    serde_json::json!({"p": 0.5}),
                )
                .unwrap(),
            ),
            1.0,
            0.0,
        );
        // Provenance: Python 3.14 stdlib, 1 / sqrt(2 * pi).
        close(
            as_f64(&call("statistics.lognormal_pdf", serde_json::json!({"x": 1})).unwrap()),
            0.398_942_280_401_432_7,
            1e-15,
        );
        close(
            as_f64(&call("statistics.lognormal_cdf", serde_json::json!({"x": 0})).unwrap()),
            0.0,
            0.0,
        );
    }

    #[test]
    fn gamma_reference_values() {
        // Provenance: Python 3.14 stdlib, 1 - 3 * exp(-2) (the Gamma(2, 1) CDF).
        close(
            as_f64(
                &call(
                    "statistics.gamma_cdf",
                    serde_json::json!({"x": 2, "shape": 2}),
                )
                .unwrap(),
            ),
            0.593_994_150_290_161_6,
            1e-12,
        );
        // Provenance: Python 3.14 stdlib, 2 * exp(-2) (the Gamma(2, 1) density).
        close(
            as_f64(
                &call(
                    "statistics.gamma_pdf",
                    serde_json::json!({"x": 2, "shape": 2}),
                )
                .unwrap(),
            ),
            0.270_670_566_473_225_4,
            1e-12,
        );
        // The shape-1 gamma is the exponential; its median is ln 2.
        close(
            as_f64(
                &call(
                    "statistics.gamma_quantile",
                    serde_json::json!({"p": 0.5, "shape": 1, "rate": 1}),
                )
                .unwrap(),
            ),
            std::f64::consts::LN_2,
            1e-9,
        );
    }

    #[test]
    fn beta_reference_values() {
        close(
            as_f64(
                &call(
                    "statistics.beta_cdf",
                    serde_json::json!({"x": 0.5, "alpha": 2, "beta": 2}),
                )
                .unwrap(),
            ),
            0.5,
            1e-12,
        );
        // Provenance: Python 3.14 stdlib, 0.5 * 0.5 / B(2, 2) with B(2, 2) = 1/6.
        close(
            as_f64(
                &call(
                    "statistics.beta_pdf",
                    serde_json::json!({"x": 0.5, "alpha": 2, "beta": 2}),
                )
                .unwrap(),
            ),
            1.5,
            1e-12,
        );
        close(
            as_f64(
                &call(
                    "statistics.beta_quantile",
                    serde_json::json!({"p": 0.5, "alpha": 2, "beta": 2}),
                )
                .unwrap(),
            ),
            0.5,
            1e-9,
        );
    }

    #[test]
    fn f_reference_values() {
        close(
            as_f64(
                &call(
                    "statistics.f_cdf",
                    serde_json::json!({"x": 1, "df1": 1, "df2": 1}),
                )
                .unwrap(),
            ),
            0.5,
            1e-12,
        );
        // Provenance: Python 3.14 stdlib, 1 - (4 / (4 + 2 * 1))^2 (the F(2, 4) CDF at 1).
        close(
            as_f64(
                &call(
                    "statistics.f_cdf",
                    serde_json::json!({"x": 1, "df1": 2, "df2": 4}),
                )
                .unwrap(),
            ),
            5.0 / 9.0,
            1e-12,
        );
        // Provenance: Python 3.14 stdlib, 1 / (pi * sqrt(x) * (1 + x)) at x = 1.
        close(
            as_f64(
                &call(
                    "statistics.f_pdf",
                    serde_json::json!({"x": 1, "df1": 1, "df2": 1}),
                )
                .unwrap(),
            ),
            0.159_154_943_091_895_2,
            1e-12,
        );
        close(
            as_f64(
                &call(
                    "statistics.f_quantile",
                    serde_json::json!({"p": 0.5, "df1": 1, "df2": 1}),
                )
                .unwrap(),
            ),
            1.0,
            1e-8,
        );
    }

    #[test]
    fn quantile_round_trips() {
        for p in [0.01, 0.25, 0.5, 0.75, 0.99] {
            let q = as_f64(
                &call(
                    "statistics.gamma_quantile",
                    serde_json::json!({"p": p, "shape": 2.5, "rate": 1.5}),
                )
                .unwrap(),
            );
            close(
                as_f64(
                    &call(
                        "statistics.gamma_cdf",
                        serde_json::json!({"x": q, "shape": 2.5, "rate": 1.5}),
                    )
                    .unwrap(),
                ),
                p,
                1e-9,
            );
            let q = as_f64(
                &call(
                    "statistics.beta_quantile",
                    serde_json::json!({"p": p, "alpha": 2.5, "beta": 3.5}),
                )
                .unwrap(),
            );
            close(
                as_f64(
                    &call(
                        "statistics.beta_cdf",
                        serde_json::json!({"x": q, "alpha": 2.5, "beta": 3.5}),
                    )
                    .unwrap(),
                ),
                p,
                1e-9,
            );
            let q = as_f64(
                &call(
                    "statistics.f_quantile",
                    serde_json::json!({"p": p, "df1": 3, "df2": 7}),
                )
                .unwrap(),
            );
            close(
                as_f64(
                    &call(
                        "statistics.f_cdf",
                        serde_json::json!({"x": q, "df1": 3, "df2": 7}),
                    )
                    .unwrap(),
                ),
                p,
                1e-9,
            );
        }
    }

    #[test]
    fn invalid_parameters_are_domain_violations() {
        for (id, raw) in [
            (
                "statistics.exponential_pdf",
                serde_json::json!({"x": 1, "rate": 0}),
            ),
            (
                "statistics.uniform_cdf",
                serde_json::json!({"x": 1, "lower": 2, "upper": 1}),
            ),
            (
                "statistics.lognormal_pdf",
                serde_json::json!({"x": 1, "sigma": -1}),
            ),
            (
                "statistics.gamma_cdf",
                serde_json::json!({"x": 1, "shape": -1}),
            ),
            (
                "statistics.beta_cdf",
                serde_json::json!({"x": 0.5, "alpha": 1, "beta": 0}),
            ),
            (
                "statistics.f_pdf",
                serde_json::json!({"x": 1, "df1": 1, "df2": -1}),
            ),
        ] {
            let error = call(id, raw).unwrap_err();
            assert_eq!(error.code, ErrorCode::DomainViolation, "{id}");
        }
    }
}
