//! Bayesian conjugate updates and posterior tail probabilities.
//!
//! Every function in this module is explicitly Bayesian: the prior is supplied
//! by the caller, the posterior has the stated conjugate form, and the reported
//! interval is an equal-tailed posterior credible interval. No posterior
//! quantity is relabelled as a frequentist confidence interval, and no
//! posterior probability is relabelled as a p-value. Every record carries that
//! statement in its assumptions.

use std::sync::Arc;

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, Assumption, CostClass, Example, FunctionDescriptor, Outcome, ParamDescriptor,
    SimpleFunction, require_mode,
};
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::schema::ValueSchema;
use bicmath_core::value::Value;

use crate::common::*;
use crate::mathfn::{beta_quantile, beta_sf, gamma_quantile, normal_quantile, normal_sf, sqrt};

const POSTERIOR_NOT_P_VALUE: &str = "the reported probability is a posterior probability under \
     the supplied prior and model, not a p-value, and the reported interval is a credible \
     interval, not a frequentist confidence interval";

fn credible_interval_schema() -> ValueSchema {
    record_schema(
        vec![
            field("lower", float64_schema()),
            field("upper", float64_schema()),
        ],
        false,
    )
}

fn credible_interval_value(lower: f64, upper: f64) -> Result<Value, EngineError> {
    Ok(record(vec![
        ("lower", float_value(lower)?),
        ("upper", float_value(upper)?),
    ]))
}

fn attach_assumptions(mut outcome: Outcome, prefix: &str, statements: &[&str]) -> Outcome {
    for (index, statement) in statements.iter().enumerate() {
        outcome = outcome.with_assumption(Assumption::unverified(
            format!("{prefix}_{index}"),
            *statement,
        ));
    }
    outcome
}

// ---------------------------------------------------------------------------
// beta_binomial_update
// ---------------------------------------------------------------------------

fn beta_binomial_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "statistics.beta_binomial_update",
        "statistics",
        "1.0.0",
        "Beta-binomial conjugate update",
        "Posterior summary for a Bernoulli proportion under a Beta prior.",
    )
    .with_description(
        "Given successes in trials and a Beta(prior_alpha, prior_beta) prior, the posterior is \
         Beta(prior_alpha + successes, prior_beta + trials - successes). Returns the posterior \
         alpha and beta, the posterior mean, the posterior mode (null when both posterior \
         shape parameters are at most 1 and the mode is not unique), and the equal-tailed \
         credible interval at the requested confidence level, computed from the beta quantile \
         function. method = \"beta_binomial_conjugate\". successes must be an integer in \
         0..=trials, trials must be at least 1, and the prior shape parameters must be \
         strictly positive. The record states that the interval and any probability are \
         posterior statements under the supplied prior and binomial model, not p-values.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "successes",
            "Observed successes; integer in 0..=trials.",
            integer_schema(),
        ),
        ParamDescriptor::required("trials", "Observed trials; integer >= 1.", integer_schema()),
        ParamDescriptor::required(
            "prior_alpha",
            "Prior Beta shape alpha; strictly positive.",
            any_number_schema(),
        ),
        ParamDescriptor::required(
            "prior_beta",
            "Prior Beta shape beta; strictly positive.",
            any_number_schema(),
        ),
        ParamDescriptor::optional(
            "confidence",
            "Credible level in (0, 1); default 0.95.",
            any_number_schema(),
        ),
    ])
    .with_output(
        record_schema(
            vec![
                field("posterior_alpha", float64_schema()),
                field("posterior_beta", float64_schema()),
                field("posterior_mean", float64_schema()),
                field("posterior_mode", ValueSchema::Any),
                field("credible_interval", credible_interval_schema()),
                field("confidence", float64_schema()),
                field("method", text_schema()),
                field("assumptions", array_schema(text_schema())),
            ],
            false,
        ),
        "Beta-binomial posterior record with an equal-tailed credible interval.",
    )
    .with_modes(inferential_modes())
    .with_cost(CostClass::Constant)
    .with_method_ref("docs/methods/statistics.md#beta_binomial_update")
    .with_examples(vec![
        Example::new(
            "uniform prior after three successes in ten trials",
            example_args(&[
                ("successes", serde_json::json!(3)),
                ("trials", serde_json::json!(10)),
                ("prior_alpha", serde_json::json!(1)),
                ("prior_beta", serde_json::json!(1)),
            ]),
        )
        .with_contains("beta_binomial_conjugate"),
        Example::new(
            "successes exceed trials",
            example_args(&[
                ("successes", serde_json::json!(11)),
                ("trials", serde_json::json!(10)),
                ("prior_alpha", serde_json::json!(1)),
                ("prior_beta", serde_json::json!(1)),
            ]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_beta_binomial(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.beta_binomial_update")?;
    let successes = non_negative_u64(&args.integer("successes")?, "successes")?;
    let trials = non_negative_u64(&args.integer("trials")?, "trials")?;
    if trials == 0 {
        return Err(
            EngineError::domain("trials must be at least 1").with_path("trials".to_string())
        );
    }
    if successes > trials {
        return Err(
            EngineError::domain("successes must be between 0 and trials inclusive")
                .with_path("successes".to_string()),
        );
    }
    let prior_alpha = scalar_f64(args, "prior_alpha")?;
    let prior_beta = scalar_f64(args, "prior_beta")?;
    if prior_alpha <= 0.0 || prior_beta <= 0.0 {
        return Err(EngineError::domain(
            "prior_alpha and prior_beta must be strictly positive",
        ));
    }
    let confidence = confidence_param(args)?;
    let posterior_alpha = prior_alpha + successes as f64;
    let posterior_beta = prior_beta + (trials - successes) as f64;
    let posterior_mean = posterior_alpha / (posterior_alpha + posterior_beta);
    let posterior_mode = if posterior_alpha > 1.0 && posterior_beta > 1.0 {
        float_value((posterior_alpha - 1.0) / (posterior_alpha + posterior_beta - 2.0))?
    } else if posterior_alpha <= 1.0 && posterior_beta > 1.0 {
        float_value(0.0)?
    } else if posterior_alpha > 1.0 && posterior_beta <= 1.0 {
        float_value(1.0)?
    } else {
        Value::Null
    };
    let tail = (1.0 - confidence) / 2.0;
    let lower = beta_quantile(tail, posterior_alpha, posterior_beta)?;
    let upper = beta_quantile(1.0 - tail, posterior_alpha, posterior_beta)?;
    let assumptions = [
        "the observations are independent Bernoulli draws with a constant success probability",
        "the prior is Beta(prior_alpha, prior_beta) and the posterior is Beta(prior_alpha + successes, prior_beta + trials - successes)",
        "the interval is an equal-tailed posterior credible interval at the requested confidence level",
        POSTERIOR_NOT_P_VALUE,
    ];
    let value = record(vec![
        ("posterior_alpha", float_value(posterior_alpha)?),
        ("posterior_beta", float_value(posterior_beta)?),
        ("posterior_mean", float_value(posterior_mean)?),
        ("posterior_mode", posterior_mode),
        ("credible_interval", credible_interval_value(lower, upper)?),
        ("confidence", float_value(confidence)?),
        ("method", text("beta_binomial_conjugate")),
        ("assumptions", assumptions_value(&assumptions)),
    ]);
    Ok(attach_assumptions(
        Outcome::approximate(value),
        "beta_binomial",
        &assumptions,
    ))
}

// ---------------------------------------------------------------------------
// normal_normal_update
// ---------------------------------------------------------------------------

fn normal_normal_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "statistics.normal_normal_update",
        "statistics",
        "1.0.0",
        "Normal-normal conjugate update",
        "Posterior summary for a normal mean with known sampling variance.",
    )
    .with_description(
        "Given a sample mean from sample_n observations with known standard deviation \
         known_sigma and a N(prior_mean, prior_sigma^2) prior, the posterior precision is \
         1 / prior_sigma^2 + sample_n / known_sigma^2, the posterior mean is the \
         precision-weighted average of the prior mean and the sample mean, and the posterior \
         variance is the reciprocal of the posterior precision. Returns posterior_mean, \
         posterior_variance, posterior_sd, the equal-tailed credible interval at the \
         requested confidence level, method = \"normal_normal_conjugate\", and the \
         assumptions. sample_n must be an integer at least 1 and both standard deviations \
         must be strictly positive. The interval is a posterior credible interval under the \
         supplied prior and model, not a confidence interval.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("sample_mean", "Observed sample mean.", any_number_schema()),
        ParamDescriptor::required(
            "sample_n",
            "Number of observations in the sample; integer >= 1.",
            integer_schema(),
        ),
        ParamDescriptor::required(
            "known_sigma",
            "Known sampling standard deviation; strictly positive.",
            any_number_schema(),
        ),
        ParamDescriptor::required("prior_mean", "Prior mean.", any_number_schema()),
        ParamDescriptor::required(
            "prior_sigma",
            "Prior standard deviation; strictly positive.",
            any_number_schema(),
        ),
        ParamDescriptor::optional(
            "confidence",
            "Credible level in (0, 1); default 0.95.",
            any_number_schema(),
        ),
    ])
    .with_output(
        record_schema(
            vec![
                field("posterior_mean", float64_schema()),
                field("posterior_variance", float64_schema()),
                field("posterior_sd", float64_schema()),
                field("credible_interval", credible_interval_schema()),
                field("confidence", float64_schema()),
                field("method", text_schema()),
                field("assumptions", array_schema(text_schema())),
            ],
            false,
        ),
        "Normal-normal posterior record with an equal-tailed credible interval.",
    )
    .with_modes(inferential_modes())
    .with_cost(CostClass::Constant)
    .with_method_ref("docs/methods/statistics.md#normal_normal_update")
    .with_examples(vec![
        Example::new(
            "precision-weighted update",
            example_args(&[
                ("sample_mean", serde_json::json!(2.0)),
                ("sample_n", serde_json::json!(4)),
                ("known_sigma", serde_json::json!(1.0)),
                ("prior_mean", serde_json::json!(0.0)),
                ("prior_sigma", serde_json::json!(1.0)),
            ]),
        )
        .with_contains("normal_normal_conjugate"),
        Example::new(
            "zero observations",
            example_args(&[
                ("sample_mean", serde_json::json!(2.0)),
                ("sample_n", serde_json::json!(0)),
                ("known_sigma", serde_json::json!(1.0)),
                ("prior_mean", serde_json::json!(0.0)),
                ("prior_sigma", serde_json::json!(1.0)),
            ]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_normal_normal(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.normal_normal_update")?;
    let sample_mean = scalar_f64(args, "sample_mean")?;
    let sample_n = non_negative_u64(&args.integer("sample_n")?, "sample_n")?;
    if sample_n == 0 {
        return Err(
            EngineError::domain("sample_n must be at least 1").with_path("sample_n".to_string())
        );
    }
    let known_sigma = scalar_f64(args, "known_sigma")?;
    let prior_mean = scalar_f64(args, "prior_mean")?;
    let prior_sigma = scalar_f64(args, "prior_sigma")?;
    if known_sigma <= 0.0 || prior_sigma <= 0.0 {
        return Err(EngineError::domain(
            "known_sigma and prior_sigma must be strictly positive",
        ));
    }
    let confidence = confidence_param(args)?;
    let prior_precision = 1.0 / (prior_sigma * prior_sigma);
    let data_precision = sample_n as f64 / (known_sigma * known_sigma);
    let posterior_precision = prior_precision + data_precision;
    let posterior_mean =
        (prior_mean * prior_precision + sample_mean * data_precision) / posterior_precision;
    let posterior_variance = 1.0 / posterior_precision;
    let posterior_sd = sqrt(posterior_variance);
    let critical = normal_quantile(1.0 - (1.0 - confidence) / 2.0, 0.0, 1.0)?;
    let lower = posterior_mean - critical * posterior_sd;
    let upper = posterior_mean + critical * posterior_sd;
    let assumptions = [
        "the sample mean is normal with known sampling standard deviation known_sigma, so the sampling variance of the mean is known_sigma^2 / sample_n",
        "the prior is normal with mean prior_mean and standard deviation prior_sigma",
        "the posterior is normal with the precision-weighted mean and reciprocal-precision variance reported here",
        POSTERIOR_NOT_P_VALUE,
    ];
    let value = record(vec![
        ("posterior_mean", float_value(posterior_mean)?),
        ("posterior_variance", float_value(posterior_variance)?),
        ("posterior_sd", float_value(posterior_sd)?),
        ("credible_interval", credible_interval_value(lower, upper)?),
        ("confidence", float_value(confidence)?),
        ("method", text("normal_normal_conjugate")),
        ("assumptions", assumptions_value(&assumptions)),
    ]);
    Ok(attach_assumptions(
        Outcome::approximate(value),
        "normal_normal",
        &assumptions,
    ))
}

// ---------------------------------------------------------------------------
// gamma_poisson_update
// ---------------------------------------------------------------------------

fn gamma_poisson_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "statistics.gamma_poisson_update",
        "statistics",
        "1.0.0",
        "Gamma-Poisson conjugate update",
        "Posterior summary for a Poisson rate under a Gamma prior.",
    )
    .with_description(
        "Given total_count events observed over exposure units and a Gamma(prior_shape, \
         prior_rate) prior in the rate parameterization, the posterior is \
         Gamma(prior_shape + total_count, prior_rate + exposure). Returns the posterior shape \
         and rate, the posterior mean, the equal-tailed credible interval at the requested \
         confidence level, method = \"gamma_poisson_conjugate\", and the assumptions. \
         total_count must be a non-negative integer, exposure must be strictly positive, and \
         the prior shape and rate must be strictly positive. The interval is a posterior \
         credible interval under the supplied prior and Poisson model, not a confidence \
         interval.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "total_count",
            "Observed event count; integer >= 0.",
            integer_schema(),
        ),
        ParamDescriptor::required(
            "exposure",
            "Total exposure; strictly positive.",
            any_number_schema(),
        ),
        ParamDescriptor::required(
            "prior_shape",
            "Prior Gamma shape; strictly positive.",
            any_number_schema(),
        ),
        ParamDescriptor::required(
            "prior_rate",
            "Prior Gamma rate; strictly positive.",
            any_number_schema(),
        ),
        ParamDescriptor::optional(
            "confidence",
            "Credible level in (0, 1); default 0.95.",
            any_number_schema(),
        ),
    ])
    .with_output(
        record_schema(
            vec![
                field("posterior_shape", float64_schema()),
                field("posterior_rate", float64_schema()),
                field("posterior_mean", float64_schema()),
                field("credible_interval", credible_interval_schema()),
                field("confidence", float64_schema()),
                field("method", text_schema()),
                field("assumptions", array_schema(text_schema())),
            ],
            false,
        ),
        "Gamma-Poisson posterior record with an equal-tailed credible interval.",
    )
    .with_modes(inferential_modes())
    .with_cost(CostClass::Constant)
    .with_method_ref("docs/methods/statistics.md#gamma_poisson_update")
    .with_examples(vec![
        Example::new(
            "five events over two units of exposure",
            example_args(&[
                ("total_count", serde_json::json!(5)),
                ("exposure", serde_json::json!(2)),
                ("prior_shape", serde_json::json!(2)),
                ("prior_rate", serde_json::json!(1)),
            ]),
        )
        .with_contains("gamma_poisson_conjugate"),
        Example::new(
            "zero exposure",
            example_args(&[
                ("total_count", serde_json::json!(5)),
                ("exposure", serde_json::json!(0)),
                ("prior_shape", serde_json::json!(2)),
                ("prior_rate", serde_json::json!(1)),
            ]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_gamma_poisson(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.gamma_poisson_update")?;
    let total_count = non_negative_u64(&args.integer("total_count")?, "total_count")?;
    let exposure = scalar_f64(args, "exposure")?;
    let prior_shape = scalar_f64(args, "prior_shape")?;
    let prior_rate = scalar_f64(args, "prior_rate")?;
    if exposure <= 0.0 {
        return Err(EngineError::domain("exposure must be strictly positive")
            .with_path("exposure".to_string()));
    }
    if prior_shape <= 0.0 || prior_rate <= 0.0 {
        return Err(EngineError::domain(
            "prior_shape and prior_rate must be strictly positive",
        ));
    }
    let confidence = confidence_param(args)?;
    let posterior_shape = prior_shape + total_count as f64;
    let posterior_rate = prior_rate + exposure;
    let posterior_mean = posterior_shape / posterior_rate;
    let tail = (1.0 - confidence) / 2.0;
    let lower = gamma_quantile(tail, posterior_shape, posterior_rate)?;
    let upper = gamma_quantile(1.0 - tail, posterior_shape, posterior_rate)?;
    let assumptions = [
        "the total count is Poisson with rate lambda * exposure for a constant rate lambda",
        "the prior is Gamma(prior_shape, prior_rate) in the rate parameterization",
        "the posterior is Gamma(prior_shape + total_count, prior_rate + exposure)",
        POSTERIOR_NOT_P_VALUE,
    ];
    let value = record(vec![
        ("posterior_shape", float_value(posterior_shape)?),
        ("posterior_rate", float_value(posterior_rate)?),
        ("posterior_mean", float_value(posterior_mean)?),
        ("credible_interval", credible_interval_value(lower, upper)?),
        ("confidence", float_value(confidence)?),
        ("method", text("gamma_poisson_conjugate")),
        ("assumptions", assumptions_value(&assumptions)),
    ]);
    Ok(attach_assumptions(
        Outcome::approximate(value),
        "gamma_poisson",
        &assumptions,
    ))
}

// ---------------------------------------------------------------------------
// beta_posterior_probability_gt
// ---------------------------------------------------------------------------

fn beta_probability_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "statistics.beta_posterior_probability_gt",
        "statistics",
        "1.0.0",
        "Beta posterior tail probability",
        "Posterior probability that a Beta-distributed parameter exceeds a threshold.",
    )
    .with_description(
        "Returns P(parameter > threshold) for a Beta(alpha, beta) posterior, computed from \
         the regularized incomplete beta function as I_{1 - threshold}(beta, alpha). The \
         threshold may be any finite number; probabilities outside [0, 1] are handled by the \
         support of the distribution. method = \"beta_posterior_probability_gt\". The result \
         states that the probability is a posterior probability under the supplied prior and \
         model, not a p-value.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "alpha",
            "Posterior Beta shape alpha; strictly positive.",
            any_number_schema(),
        ),
        ParamDescriptor::required(
            "beta",
            "Posterior Beta shape beta; strictly positive.",
            any_number_schema(),
        ),
        ParamDescriptor::required(
            "threshold",
            "Threshold; the returned probability is P(parameter > threshold).",
            any_number_schema(),
        ),
    ])
    .with_output(
        record_schema(
            vec![
                field("probability", float64_schema()),
                field("alpha", float64_schema()),
                field("beta", float64_schema()),
                field("threshold", float64_schema()),
                field("method", text_schema()),
                field("assumptions", array_schema(text_schema())),
            ],
            false,
        ),
        "Beta posterior tail probability record.",
    )
    .with_modes(inferential_modes())
    .with_cost(CostClass::Constant)
    .with_method_ref("docs/methods/statistics.md#beta_posterior_probability_gt")
    .with_examples(vec![
        Example::new(
            "uniform posterior above one half",
            example_args(&[
                ("alpha", serde_json::json!(1)),
                ("beta", serde_json::json!(1)),
                ("threshold", serde_json::json!(0.5)),
            ]),
        )
        .with_contains("beta_posterior_probability_gt"),
        Example::new(
            "non-positive shape",
            example_args(&[
                ("alpha", serde_json::json!(0)),
                ("beta", serde_json::json!(1)),
                ("threshold", serde_json::json!(0.5)),
            ]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_beta_probability(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(
        ctx,
        &inferential_modes(),
        "statistics.beta_posterior_probability_gt",
    )?;
    let alpha = scalar_f64(args, "alpha")?;
    let beta = scalar_f64(args, "beta")?;
    let threshold = scalar_f64(args, "threshold")?;
    if alpha <= 0.0 || beta <= 0.0 {
        return Err(EngineError::domain(
            "alpha and beta must be strictly positive",
        ));
    }
    let probability = beta_sf(threshold, alpha, beta)?;
    let assumptions = [
        "the parameter follows the Beta(alpha, beta) posterior distribution",
        "the threshold is compared with the parameter itself, not with a test statistic",
        POSTERIOR_NOT_P_VALUE,
    ];
    let value = record(vec![
        ("probability", float_value(probability)?),
        ("alpha", float_value(alpha)?),
        ("beta", float_value(beta)?),
        ("threshold", float_value(threshold)?),
        ("method", text("beta_posterior_probability_gt")),
        ("assumptions", assumptions_value(&assumptions)),
    ]);
    Ok(attach_assumptions(
        Outcome::approximate(value),
        "beta_probability",
        &assumptions,
    ))
}

// ---------------------------------------------------------------------------
// normal_posterior_probability_gt
// ---------------------------------------------------------------------------

fn normal_probability_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "statistics.normal_posterior_probability_gt",
        "statistics",
        "1.0.0",
        "Normal posterior tail probability",
        "Posterior probability that a normal parameter exceeds a threshold.",
    )
    .with_description(
        "Returns P(parameter > threshold) for a N(mean, sd^2) posterior, computed from the \
         complementary error function as 0.5 * erfc((threshold - mean) / (sd * sqrt(2))). \
         sd must be strictly positive. method = \"normal_posterior_probability_gt\". The \
         result states that the probability is a posterior probability under the supplied \
         prior and model, not a p-value.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("mean", "Posterior mean.", any_number_schema()),
        ParamDescriptor::required(
            "sd",
            "Posterior standard deviation; strictly positive.",
            any_number_schema(),
        ),
        ParamDescriptor::required(
            "threshold",
            "Threshold; the returned probability is P(parameter > threshold).",
            any_number_schema(),
        ),
    ])
    .with_output(
        record_schema(
            vec![
                field("probability", float64_schema()),
                field("mean", float64_schema()),
                field("sd", float64_schema()),
                field("threshold", float64_schema()),
                field("method", text_schema()),
                field("assumptions", array_schema(text_schema())),
            ],
            false,
        ),
        "Normal posterior tail probability record.",
    )
    .with_modes(inferential_modes())
    .with_cost(CostClass::Constant)
    .with_method_ref("docs/methods/statistics.md#normal_posterior_probability_gt")
    .with_examples(vec![
        Example::new(
            "one point nine six standard deviations above the mean",
            example_args(&[
                ("mean", serde_json::json!(0)),
                ("sd", serde_json::json!(1)),
                ("threshold", serde_json::json!(1.96)),
            ]),
        )
        .with_contains("normal_posterior_probability_gt"),
        Example::new(
            "non-positive standard deviation",
            example_args(&[
                ("mean", serde_json::json!(0)),
                ("sd", serde_json::json!(0)),
                ("threshold", serde_json::json!(1.96)),
            ]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_normal_probability(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(
        ctx,
        &inferential_modes(),
        "statistics.normal_posterior_probability_gt",
    )?;
    let mean = scalar_f64(args, "mean")?;
    let sd = scalar_f64(args, "sd")?;
    let threshold = scalar_f64(args, "threshold")?;
    if sd <= 0.0 {
        return Err(EngineError::domain("sd must be strictly positive").with_path("sd".to_string()));
    }
    let probability = normal_sf(threshold, mean, sd)?;
    let assumptions = [
        "the parameter follows the N(mean, sd^2) posterior distribution",
        "the threshold is compared with the parameter itself, not with a test statistic",
        POSTERIOR_NOT_P_VALUE,
    ];
    let value = record(vec![
        ("probability", float_value(probability)?),
        ("mean", float_value(mean)?),
        ("sd", float_value(sd)?),
        ("threshold", float_value(threshold)?),
        ("method", text("normal_posterior_probability_gt")),
        ("assumptions", assumptions_value(&assumptions)),
    ]);
    Ok(attach_assumptions(
        Outcome::approximate(value),
        "normal_probability",
        &assumptions,
    ))
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

pub fn functions() -> Vec<Arc<dyn bicmath_core::contract::Function>> {
    vec![
        SimpleFunction::arc(beta_binomial_descriptor(), invoke_beta_binomial),
        SimpleFunction::arc(normal_normal_descriptor(), invoke_normal_normal),
        SimpleFunction::arc(gamma_poisson_descriptor(), invoke_gamma_poisson),
        SimpleFunction::arc(beta_probability_descriptor(), invoke_beta_probability),
        SimpleFunction::arc(normal_probability_descriptor(), invoke_normal_probability),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
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

    fn record_of(outcome: &Outcome) -> &BTreeMap<String, Value> {
        match &outcome.value {
            Value::Record(fields) => fields,
            other => panic!("expected record, got {other:?}"),
        }
    }

    fn field_f64(fields: &BTreeMap<String, Value>, name: &str) -> f64 {
        match fields.get(name) {
            Some(Value::Number(number)) => number.to_f64().expect("number"),
            other => panic!("expected numeric field {name}, got {other:?}"),
        }
    }

    fn field_record<'a>(
        fields: &'a BTreeMap<String, Value>,
        name: &str,
    ) -> &'a BTreeMap<String, Value> {
        match fields.get(name) {
            Some(Value::Record(record)) => record,
            other => panic!("expected record field {name}, got {other:?}"),
        }
    }

    fn assumptions_of(outcome: &Outcome) -> Vec<String> {
        outcome
            .assumptions
            .iter()
            .map(|assumption| assumption.statement.clone())
            .collect()
    }

    fn assert_posterior_statement(outcome: &Outcome) {
        let statements = assumptions_of(outcome);
        assert!(
            statements
                .iter()
                .any(|statement| statement.contains("not a p-value")),
            "the record must state that the probability is not a p-value: {statements:?}"
        );
    }

    fn close(actual: f64, expected: f64, tolerance: f64) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "expected {expected}, got {actual} (tolerance {tolerance})"
        );
    }

    #[test]
    fn beta_binomial_posterior_matches_the_conjugate_form() {
        // Provenance: Python 3.14 stdlib evaluation of the Beta(4, 8)
        // posterior after 3 successes in 10 trials with a Beta(1, 1) prior:
        // mean 4 / 12 = 0.3333333333333333, mode (4 - 1) / (4 + 8 - 2) = 0.3,
        // and equal-tailed 95% interval [0.10926344381909811,
        // 0.6097425595724211].
        let outcome = call(
            "statistics.beta_binomial_update",
            serde_json::json!({
                "successes": 3,
                "trials": 10,
                "prior_alpha": 1,
                "prior_beta": 1
            }),
        )
        .unwrap();
        let fields = record_of(&outcome);
        close(field_f64(fields, "posterior_alpha"), 4.0, 0.0);
        close(field_f64(fields, "posterior_beta"), 8.0, 0.0);
        close(field_f64(fields, "posterior_mean"), 4.0 / 12.0, 1e-15);
        close(field_f64(fields, "posterior_mode"), 0.3, 1e-12);
        let interval = field_record(fields, "credible_interval");
        close(field_f64(interval, "lower"), 0.109_263_443_819_098_11, 1e-9);
        close(field_f64(interval, "upper"), 0.609_742_559_572_421_1, 1e-9);
        assert_posterior_statement(&outcome);
    }

    #[test]
    fn normal_normal_posterior_mean_lies_between_prior_and_sample() {
        // Provenance: Python 3.14 stdlib evaluation of the conjugate update
        // with prior N(0, 1), sample mean 2, n = 4, known sigma = 1:
        // posterior precision 5, mean 1.6, variance 0.2, and equal-tailed
        // 95% interval [0.7234774594234186, 2.476522540576582].
        let outcome = call(
            "statistics.normal_normal_update",
            serde_json::json!({
                "sample_mean": 2.0,
                "sample_n": 4,
                "known_sigma": 1.0,
                "prior_mean": 0.0,
                "prior_sigma": 1.0
            }),
        )
        .unwrap();
        let fields = record_of(&outcome);
        let posterior_mean = field_f64(fields, "posterior_mean");
        assert!(
            posterior_mean > 0.0 && posterior_mean < 2.0,
            "the posterior mean must lie between the prior mean and the sample mean"
        );
        close(posterior_mean, 1.6, 1e-15);
        close(field_f64(fields, "posterior_variance"), 0.2, 1e-15);
        let interval = field_record(fields, "credible_interval");
        close(field_f64(interval, "lower"), 0.723_477_459_423_418_6, 1e-9);
        close(field_f64(interval, "upper"), 2.476_522_540_576_582, 1e-9);
        assert_posterior_statement(&outcome);
    }

    #[test]
    fn gamma_poisson_posterior_shape_adds_the_count() {
        // Provenance: Python 3.14 stdlib evaluation of the Gamma(7, 3)
        // posterior after 5 events over 2 units of exposure with a Gamma(2, 1)
        // prior: mean 7 / 3 = 2.3333333333333335 and equal-tailed 95%
        // interval [0.9381210171732888, 4.353158007506227].
        let outcome = call(
            "statistics.gamma_poisson_update",
            serde_json::json!({
                "total_count": 5,
                "exposure": 2,
                "prior_shape": 2,
                "prior_rate": 1
            }),
        )
        .unwrap();
        let fields = record_of(&outcome);
        close(field_f64(fields, "posterior_shape"), 7.0, 1e-15);
        close(field_f64(fields, "posterior_rate"), 3.0, 1e-15);
        close(field_f64(fields, "posterior_mean"), 7.0 / 3.0, 1e-15);
        let interval = field_record(fields, "credible_interval");
        close(field_f64(interval, "lower"), 0.938_121_017_173_288_8, 1e-9);
        close(field_f64(interval, "upper"), 4.353_158_007_506_227, 1e-9);
        assert_posterior_statement(&outcome);
    }

    #[test]
    fn beta_posterior_probability_above_one_half_is_one_half() {
        let outcome = call(
            "statistics.beta_posterior_probability_gt",
            serde_json::json!({"alpha": 1, "beta": 1, "threshold": 0.5}),
        )
        .unwrap();
        let fields = record_of(&outcome);
        close(field_f64(fields, "probability"), 0.5, 1e-12);
        assert_posterior_statement(&outcome);
    }

    #[test]
    fn normal_posterior_probability_matches_the_erfc_tail() {
        // Provenance: Python 3.14 stdlib math.erfc: P(Z > 1.96) =
        // 0.5 * erfc(1.96 / sqrt(2)) = 0.02499789514822043.
        let outcome = call(
            "statistics.normal_posterior_probability_gt",
            serde_json::json!({"mean": 0, "sd": 1, "threshold": 1.96}),
        )
        .unwrap();
        let fields = record_of(&outcome);
        close(
            field_f64(fields, "probability"),
            0.024_997_895_148_220_43,
            1e-15,
        );
        assert_posterior_statement(&outcome);
    }

    #[test]
    fn bayesian_updates_reject_invalid_inputs() {
        let error = call(
            "statistics.beta_binomial_update",
            serde_json::json!({
                "successes": 11, "trials": 10,
                "prior_alpha": 1, "prior_beta": 1
            }),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::DomainViolation);
        let error = call(
            "statistics.gamma_poisson_update",
            serde_json::json!({
                "total_count": 5, "exposure": 0,
                "prior_shape": 2, "prior_rate": 1
            }),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::DomainViolation);
        let error = call(
            "statistics.normal_posterior_probability_gt",
            serde_json::json!({"mean": 0, "sd": -1, "threshold": 0}),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::DomainViolation);
    }
}
