//! Discrete distributions: Poisson and negative binomial.
//!
//! All functions produce float64 results and require auto or scientific mode.
//! Invalid parameters are rejected with a domain violation; the tail sums are
//! evaluated with the regularized incomplete gamma and beta functions from
//! [`crate::mathfn`] rather than by naive summation.

use std::sync::Arc;

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, CostClass, Example, FunctionDescriptor, Outcome, ParamDescriptor, SimpleFunction,
    require_mode,
};
use bicmath_core::error::{EngineError, ErrorCode};

use crate::common::*;
use crate::mathfn::{negative_binomial_cdf, negative_binomial_pmf, poisson_cdf, poisson_pmf};

fn poisson_descriptor(
    id: &str,
    title: &str,
    summary: &str,
    description: &str,
    examples: Vec<Example>,
) -> FunctionDescriptor {
    FunctionDescriptor::new(id, "statistics", "1.0.0", title, summary)
        .with_description(description)
        .with_parameters(vec![
            ParamDescriptor::required(
                "k",
                "Event count; integer >= 0 (negative values return 0).",
                integer_schema(),
            ),
            ParamDescriptor::required("lambda", "Mean rate; must be > 0.", any_number_schema()),
        ])
        .with_output(float64_schema(), "Poisson distribution value.")
        .with_modes(inferential_modes())
        .with_cost(CostClass::Constant)
        .with_method_ref(format!(
            "docs/methods/statistics.md#{}",
            id.rsplit('.').next().unwrap()
        ))
        .with_examples(examples)
}

fn poisson_pmf_descriptor() -> FunctionDescriptor {
    poisson_descriptor(
        "statistics.poisson_pmf",
        "Poisson probability mass",
        "Probability mass function of the Poisson distribution.",
        "pmf(k) = e^(-lambda) lambda^k / k!, evaluated in log space with a Lanczos \
         log-gamma so large k and lambda stay stable. lambda must be strictly positive; \
         k < 0 returns 0.",
        vec![
            Example::new(
                "negative count",
                example_args(&[
                    ("k", serde_json::json!(-1)),
                    ("lambda", serde_json::json!(3)),
                ]),
            )
            .with_value(parse_value(
                serde_json::json!({"kind": "float64", "value": "0"}),
            )),
            Example::new(
                "zero mean rate",
                example_args(&[
                    ("k", serde_json::json!(0)),
                    ("lambda", serde_json::json!(0)),
                ]),
            )
            .with_error(ErrorCode::DomainViolation),
        ],
    )
}

fn poisson_cdf_descriptor() -> FunctionDescriptor {
    poisson_descriptor(
        "statistics.poisson_cdf",
        "Poisson cumulative distribution",
        "Cumulative distribution function of the Poisson distribution.",
        "cdf(k) = P(X <= k) = Q(k + 1, lambda), the regularized upper incomplete gamma \
         function, computed directly for tail accuracy. lambda must be strictly positive; \
         k < 0 returns 0.",
        vec![
            Example::new(
                "negative count",
                example_args(&[
                    ("k", serde_json::json!(-1)),
                    ("lambda", serde_json::json!(3)),
                ]),
            )
            .with_value(parse_value(
                serde_json::json!({"kind": "float64", "value": "0"}),
            )),
            Example::new(
                "zero mean rate",
                example_args(&[
                    ("k", serde_json::json!(0)),
                    ("lambda", serde_json::json!(0)),
                ]),
            )
            .with_error(ErrorCode::DomainViolation),
        ],
    )
}

fn negative_binomial_descriptor(
    id: &str,
    title: &str,
    summary: &str,
    description: &str,
    examples: Vec<Example>,
) -> FunctionDescriptor {
    FunctionDescriptor::new(id, "statistics", "1.0.0", title, summary)
        .with_description(description)
        .with_parameters(vec![
            ParamDescriptor::required(
                "k",
                "Number of failures; integer >= 0 (negative values return 0).",
                integer_schema(),
            ),
            ParamDescriptor::required(
                "r",
                "Target number of successes; must be > 0.",
                any_number_schema(),
            ),
            ParamDescriptor::required("p", "Success probability in [0, 1].", any_number_schema()),
        ])
        .with_output(float64_schema(), "Negative binomial distribution value.")
        .with_modes(inferential_modes())
        .with_cost(CostClass::Constant)
        .with_method_ref(format!(
            "docs/methods/statistics.md#{}",
            id.rsplit('.').next().unwrap()
        ))
        .with_examples(examples)
}

fn negative_binomial_pmf_descriptor() -> FunctionDescriptor {
    negative_binomial_descriptor(
        "statistics.negative_binomial_pmf",
        "Negative binomial probability mass",
        "Probability mass function of the negative binomial distribution.",
        "pmf(k) = C(k + r - 1, k) p^r (1 - p)^k, evaluated in log space with a Lanczos \
         log-gamma. r must be strictly positive and p must lie in [0, 1]; k < 0 returns 0. \
         p = 0 and p = 1 are handled exactly.",
        vec![
            Example::new(
                "certain success on the first trial",
                example_args(&[
                    ("k", serde_json::json!(0)),
                    ("r", serde_json::json!(1)),
                    ("p", serde_json::json!(1)),
                ]),
            )
            .with_value(parse_value(
                serde_json::json!({"kind": "float64", "value": "1"}),
            )),
            Example::new(
                "probability above one",
                example_args(&[
                    ("k", serde_json::json!(1)),
                    ("r", serde_json::json!(2)),
                    ("p", serde_json::json!(1.5)),
                ]),
            )
            .with_error(ErrorCode::DomainViolation),
        ],
    )
}

fn negative_binomial_cdf_descriptor() -> FunctionDescriptor {
    negative_binomial_descriptor(
        "statistics.negative_binomial_cdf",
        "Negative binomial cumulative distribution",
        "Cumulative distribution function of the negative binomial distribution.",
        "cdf(k) = P(X <= k) = I_p(r, k + 1), the regularized incomplete beta function. \
         r must be strictly positive and p must lie in [0, 1]; k < 0 returns 0.",
        vec![
            Example::new(
                "certain success on the first trial",
                example_args(&[
                    ("k", serde_json::json!(5)),
                    ("r", serde_json::json!(1)),
                    ("p", serde_json::json!(1)),
                ]),
            )
            .with_value(parse_value(
                serde_json::json!({"kind": "float64", "value": "1"}),
            )),
            Example::new(
                "zero target successes",
                example_args(&[
                    ("k", serde_json::json!(1)),
                    ("r", serde_json::json!(0)),
                    ("p", serde_json::json!(0.5)),
                ]),
            )
            .with_error(ErrorCode::DomainViolation),
        ],
    )
}

fn invoke_poisson_pmf(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.poisson_pmf")?;
    let k = scalar_f64(args, "k")?;
    let lambda = scalar_f64(args, "lambda")?;
    Ok(Outcome::approximate(float_value(poisson_pmf(k, lambda)?)?))
}

fn invoke_poisson_cdf(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.poisson_cdf")?;
    let k = scalar_f64(args, "k")?;
    let lambda = scalar_f64(args, "lambda")?;
    Ok(Outcome::approximate(float_value(poisson_cdf(k, lambda)?)?))
}

fn invoke_negative_binomial_pmf(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(
        ctx,
        &inferential_modes(),
        "statistics.negative_binomial_pmf",
    )?;
    let k = scalar_f64(args, "k")?;
    let r = scalar_f64(args, "r")?;
    let p = scalar_f64(args, "p")?;
    Ok(Outcome::approximate(float_value(negative_binomial_pmf(
        k, r, p,
    )?)?))
}

fn invoke_negative_binomial_cdf(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(
        ctx,
        &inferential_modes(),
        "statistics.negative_binomial_cdf",
    )?;
    let k = scalar_f64(args, "k")?;
    let r = scalar_f64(args, "r")?;
    let p = scalar_f64(args, "p")?;
    Ok(Outcome::approximate(float_value(negative_binomial_cdf(
        k, r, p,
    )?)?))
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

pub fn functions() -> Vec<Arc<dyn bicmath_core::contract::Function>> {
    vec![
        SimpleFunction::arc(poisson_pmf_descriptor(), invoke_poisson_pmf),
        SimpleFunction::arc(poisson_cdf_descriptor(), invoke_poisson_cdf),
        SimpleFunction::arc(
            negative_binomial_pmf_descriptor(),
            invoke_negative_binomial_pmf,
        ),
        SimpleFunction::arc(
            negative_binomial_cdf_descriptor(),
            invoke_negative_binomial_cdf,
        ),
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
    fn poisson_reference_values() {
        // Provenance: Python 3.14 stdlib, exp(-3) * 3**2 / factorial(2).
        close(
            as_f64(
                &call(
                    "statistics.poisson_pmf",
                    serde_json::json!({"k": 2, "lambda": 3}),
                )
                .unwrap(),
            ),
            0.224_041_807_655_387_75,
            1e-12,
        );
        // Provenance: Python 3.14 stdlib sum of the pmf terms up to k.
        close(
            as_f64(
                &call(
                    "statistics.poisson_cdf",
                    serde_json::json!({"k": 2, "lambda": 3}),
                )
                .unwrap(),
            ),
            0.423_190_081_126_843_64,
            1e-12,
        );
        close(
            as_f64(
                &call(
                    "statistics.poisson_cdf",
                    serde_json::json!({"k": -1, "lambda": 3}),
                )
                .unwrap(),
            ),
            0.0,
            0.0,
        );
    }

    #[test]
    fn negative_binomial_reference_values() {
        // Provenance: Python 3.14 stdlib, comb(k + r - 1, k) * p^r * (1 - p)^k.
        close(
            as_f64(
                &call(
                    "statistics.negative_binomial_pmf",
                    serde_json::json!({"k": 1, "r": 2, "p": 0.5}),
                )
                .unwrap(),
            ),
            0.25,
            1e-15,
        );
        // Provenance: Python 3.14 stdlib, pmf(0) + pmf(1) = 0.25 + 0.25.
        close(
            as_f64(
                &call(
                    "statistics.negative_binomial_cdf",
                    serde_json::json!({"k": 1, "r": 2, "p": 0.5}),
                )
                .unwrap(),
            ),
            0.5,
            1e-15,
        );
        close(
            as_f64(
                &call(
                    "statistics.negative_binomial_cdf",
                    serde_json::json!({"k": 0, "r": 2, "p": 0.5}),
                )
                .unwrap(),
            ),
            0.25,
            1e-15,
        );
    }

    #[test]
    fn invalid_parameters_are_domain_violations() {
        for (id, raw) in [
            (
                "statistics.poisson_pmf",
                serde_json::json!({"k": 1, "lambda": 0}),
            ),
            (
                "statistics.poisson_cdf",
                serde_json::json!({"k": 1, "lambda": -1}),
            ),
            (
                "statistics.negative_binomial_pmf",
                serde_json::json!({"k": 1, "r": 0, "p": 0.5}),
            ),
            (
                "statistics.negative_binomial_cdf",
                serde_json::json!({"k": 1, "r": 2, "p": 2}),
            ),
        ] {
            let error = call(id, raw).unwrap_err();
            assert_eq!(error.code, ErrorCode::DomainViolation, "{id}");
        }
    }
}
