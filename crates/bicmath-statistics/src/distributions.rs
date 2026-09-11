//! Probability distributions: normal, Student's t, binomial, and chi-square.
//!
//! All functions produce float64 results and require auto or scientific mode.
//! The special functions themselves live in [`crate::mathfn`]; the upper tails
//! are computed from `erfc`/regularized incomplete gamma or beta functions so
//! they keep relative accuracy far from the center.

use std::sync::Arc;

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, CostClass, Example, FunctionDescriptor, Outcome, ParamDescriptor, SimpleFunction,
    require_mode,
};
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::number::Number;

use crate::common::*;
use crate::mathfn::{
    binomial_cdf, binomial_pmf, chi_square_cdf, chi_square_pdf, chi_square_quantile, chi_square_sf,
    normal_cdf, normal_pdf, normal_quantile, normal_sf, student_t_cdf, student_t_pdf,
    student_t_quantile, student_t_sf,
};

fn normal_parameters(args: &Args) -> Result<(f64, f64, f64), EngineError> {
    let x = scalar_f64(args, "x")?;
    let mean = optional_f64_param(args, "mean")?.unwrap_or(0.0);
    let sd = optional_f64_param(args, "sd")?.unwrap_or(1.0);
    Ok((x, mean, sd))
}

fn normal_params() -> Vec<ParamDescriptor> {
    vec![
        ParamDescriptor::required("x", "Quantile.", any_number_schema()),
        ParamDescriptor::optional("mean", "Mean; default 0.", any_number_schema()),
        ParamDescriptor::optional(
            "sd",
            "Standard deviation; must be > 0; default 1.",
            any_number_schema(),
        ),
    ]
}

fn normal_descriptor(
    id: &str,
    title: &str,
    summary: &str,
    description: &str,
) -> FunctionDescriptor {
    FunctionDescriptor::new(id, "statistics", "1.0.0", title, summary)
        .with_description(description)
        .with_parameters(normal_params())
        .with_output(float64_schema(), "Normal distribution value.")
        .with_modes(inferential_modes())
        .with_cost(CostClass::Constant)
        .with_method_ref(format!(
            "docs/methods/statistics.md#{}",
            id.rsplit('.').next().unwrap()
        ))
        .with_examples(vec![
            Example::new(
                "standard normal center",
                example_args(&[("x", serde_json::json!(0))]),
            )
            .with_value(parse_value(
                serde_json::json!({"kind": "float64", "value": "0.5"}),
            )),
        ])
}

fn normal_pdf_descriptor() -> FunctionDescriptor {
    let mut descriptor = normal_descriptor(
        "statistics.normal_pdf",
        "Normal probability density",
        "Probability density of the normal distribution.",
        "pdf(x) = exp(-0.5 * ((x - mean) / sd)^2) / (sd * sqrt(2 * pi)). The standard \
         deviation must be strictly positive.",
    );
    descriptor.examples = vec![
        Example::new(
            "zero standard deviation",
            example_args(&[("x", serde_json::json!(0)), ("sd", serde_json::json!(0))]),
        )
        .with_error(ErrorCode::DomainViolation),
    ];
    descriptor
}

fn normal_cdf_descriptor() -> FunctionDescriptor {
    normal_descriptor(
        "statistics.normal_cdf",
        "Normal cumulative distribution",
        "Cumulative distribution function of the normal distribution.",
        "cdf(x) = 0.5 * erfc(-(x - mean) / (sd * sqrt(2))), evaluated with a high-accuracy \
         complementary error function.",
    )
}

fn normal_sf_descriptor() -> FunctionDescriptor {
    normal_descriptor(
        "statistics.normal_sf",
        "Normal survival function",
        "Upper tail probability of the normal distribution.",
        "sf(x) = P(X > x) = 0.5 * erfc((x - mean) / (sd * sqrt(2))). The upper tail is \
         computed from erfc directly, never as 1 - cdf, so large positive x keeps relative \
         accuracy.",
    )
}

fn normal_quantile_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "statistics.normal_quantile",
        "statistics",
        "1.0.0",
        "Normal quantile",
        "Inverse normal CDF.",
    )
    .with_description(
        "Returns x such that cdf(x) = p, solved by bracket expansion, bisection, and Newton \
         polishing to a relative tolerance of 1e-15. p must be strictly between 0 and 1; \
         mean defaults to 0 and sd to 1 and must be positive.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("p", "Probability in (0, 1).", any_number_schema()),
        ParamDescriptor::optional("mean", "Mean; default 0.", any_number_schema()),
        ParamDescriptor::optional(
            "sd",
            "Standard deviation; must be > 0; default 1.",
            any_number_schema(),
        ),
    ])
    .with_output(float64_schema(), "Normal quantile.")
    .with_modes(inferential_modes())
    .with_cost(CostClass::Iterative)
    .with_method_ref("docs/methods/statistics.md#normal_quantile")
    .with_examples(vec![
        Example::new("median", example_args(&[("p", serde_json::json!(0.5))])).with_value(
            parse_value(serde_json::json!({"kind": "float64", "value": "0"})),
        ),
    ])
}

fn invoke_normal_pdf(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.normal_pdf")?;
    let (x, mean, sd) = normal_parameters(args)?;
    Ok(Outcome::approximate(float_value(normal_pdf(x, mean, sd)?)?))
}

fn invoke_normal_cdf(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.normal_cdf")?;
    let (x, mean, sd) = normal_parameters(args)?;
    Ok(Outcome::approximate(float_value(normal_cdf(x, mean, sd)?)?))
}

fn invoke_normal_sf(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.normal_sf")?;
    let (x, mean, sd) = normal_parameters(args)?;
    Ok(Outcome::approximate(float_value(normal_sf(x, mean, sd)?)?))
}

fn invoke_normal_quantile(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.normal_quantile")?;
    let p = scalar_f64(args, "p")?;
    let mean = optional_f64_param(args, "mean")?.unwrap_or(0.0);
    let sd = optional_f64_param(args, "sd")?.unwrap_or(1.0);
    Ok(Outcome::approximate(float_value(normal_quantile(
        p, mean, sd,
    )?)?))
}

// ---------------------------------------------------------------------------
// Student's t
// ---------------------------------------------------------------------------

fn student_t_params() -> Vec<ParamDescriptor> {
    vec![
        ParamDescriptor::required("x", "Quantile.", any_number_schema()),
        ParamDescriptor::required(
            "df",
            "Degrees of freedom; must be > 0.",
            any_number_schema(),
        ),
    ]
}

fn student_t_descriptor(
    id: &str,
    title: &str,
    summary: &str,
    description: &str,
    example: Example,
) -> FunctionDescriptor {
    FunctionDescriptor::new(id, "statistics", "1.0.0", title, summary)
        .with_description(description)
        .with_parameters(student_t_params())
        .with_output(float64_schema(), "Student-t distribution value.")
        .with_modes(inferential_modes())
        .with_cost(CostClass::Constant)
        .with_method_ref(format!(
            "docs/methods/statistics.md#{}",
            id.rsplit('.').next().unwrap()
        ))
        .with_examples(vec![example])
}

fn student_t_pdf_descriptor() -> FunctionDescriptor {
    student_t_descriptor(
        "statistics.student_t_pdf",
        "Student-t probability density",
        "Probability density of Student's t distribution.",
        "pdf(x) = Gamma((df + 1) / 2) / (sqrt(df * pi) Gamma(df / 2)) * \
         (1 + x^2 / df)^(-(df + 1) / 2), computed with a Lanczos log-gamma.",
        Example::new(
            "zero degrees of freedom",
            example_args(&[("x", serde_json::json!(0)), ("df", serde_json::json!(0))]),
        )
        .with_error(ErrorCode::DomainViolation),
    )
}

fn student_t_cdf_descriptor() -> FunctionDescriptor {
    student_t_descriptor(
        "statistics.student_t_cdf",
        "Student-t cumulative distribution",
        "Cumulative distribution function of Student's t distribution.",
        "For x > 0, cdf(x) = 1 - 0.5 * I_y(df/2, 1/2) with y = df / (df + x^2); for x < 0 the \
         mirrored expression is used. I_y is the regularized incomplete beta function.",
        Example::new(
            "symmetric center",
            example_args(&[("x", serde_json::json!(0)), ("df", serde_json::json!(5))]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "float64", "value": "0.5"}),
        )),
    )
}

fn student_t_sf_descriptor() -> FunctionDescriptor {
    student_t_descriptor(
        "statistics.student_t_sf",
        "Student-t survival function",
        "Upper tail probability of Student's t distribution.",
        "For x > 0, sf(x) = 0.5 * I_y(df/2, 1/2) with y = df / (df + x^2); the upper tail is \
         computed directly from the regularized incomplete beta function, not as 1 - cdf.",
        Example::new(
            "symmetric center",
            example_args(&[("x", serde_json::json!(0)), ("df", serde_json::json!(5))]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "float64", "value": "0.5"}),
        )),
    )
}

fn student_t_quantile_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "statistics.student_t_quantile",
        "statistics",
        "1.0.0",
        "Student-t quantile",
        "Inverse Student-t CDF.",
    )
    .with_description(
        "Returns x such that cdf(x) = p, solved by bracket expansion, bisection, and Newton \
         polishing to a relative tolerance of 1e-15. p must be strictly between 0 and 1 and df \
         must be positive.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("p", "Probability in (0, 1).", any_number_schema()),
        ParamDescriptor::required(
            "df",
            "Degrees of freedom; must be > 0.",
            any_number_schema(),
        ),
    ])
    .with_output(float64_schema(), "Student-t quantile.")
    .with_modes(inferential_modes())
    .with_cost(CostClass::Iterative)
    .with_method_ref("docs/methods/statistics.md#student_t_quantile")
    .with_examples(vec![
        Example::new(
            "median",
            example_args(&[("p", serde_json::json!(0.5)), ("df", serde_json::json!(5))]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "float64", "value": "0"}),
        )),
    ])
}

fn invoke_student_t_pdf(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.student_t_pdf")?;
    let x = scalar_f64(args, "x")?;
    let df = scalar_f64(args, "df")?;
    Ok(Outcome::approximate(float_value(student_t_pdf(x, df)?)?))
}

fn invoke_student_t_cdf(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.student_t_cdf")?;
    let x = scalar_f64(args, "x")?;
    let df = scalar_f64(args, "df")?;
    Ok(Outcome::approximate(float_value(student_t_cdf(x, df)?)?))
}

fn invoke_student_t_sf(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.student_t_sf")?;
    let x = scalar_f64(args, "x")?;
    let df = scalar_f64(args, "df")?;
    Ok(Outcome::approximate(float_value(student_t_sf(x, df)?)?))
}

fn invoke_student_t_quantile(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.student_t_quantile")?;
    let p = scalar_f64(args, "p")?;
    let df = scalar_f64(args, "df")?;
    Ok(Outcome::approximate(float_value(student_t_quantile(
        p, df,
    )?)?))
}

// ---------------------------------------------------------------------------
// Binomial
// ---------------------------------------------------------------------------

fn binomial_parameters(args: &Args) -> Result<(f64, f64, f64), EngineError> {
    let k = args.integer("k")?;
    let n = args.integer("n")?;
    if n.sign() == num_bigint::Sign::Minus {
        return Err(
            EngineError::domain("n must be a non-negative integer").with_path("n".to_string())
        );
    }
    let p = scalar_f64(args, "p")?;
    if !(0.0..=1.0).contains(&p) {
        return Err(EngineError::domain("p must be in [0, 1]").with_path("p".to_string()));
    }
    let k = number_to_f64(&Number::Integer(k))?;
    let n = number_to_f64(&Number::Integer(n))?;
    Ok((k, n, p))
}

fn binomial_descriptor(
    id: &str,
    title: &str,
    summary: &str,
    description: &str,
    example: Example,
) -> FunctionDescriptor {
    FunctionDescriptor::new(id, "statistics", "1.0.0", title, summary)
        .with_description(description)
        .with_parameters(vec![
            ParamDescriptor::required("k", "Number of successes; integer.", integer_schema()),
            ParamDescriptor::required("n", "Number of trials; integer >= 0.", integer_schema()),
            ParamDescriptor::required("p", "Success probability in [0, 1].", any_number_schema()),
        ])
        .with_output(float64_schema(), "Binomial distribution value.")
        .with_modes(inferential_modes())
        .with_cost(CostClass::Constant)
        .with_method_ref(format!(
            "docs/methods/statistics.md#{}",
            id.rsplit('.').next().unwrap()
        ))
        .with_examples(vec![example])
}

fn binomial_pmf_descriptor() -> FunctionDescriptor {
    binomial_descriptor(
        "statistics.binomial_pmf",
        "Binomial probability mass",
        "Probability mass function of the binomial distribution.",
        "pmf(k) = C(n, k) p^k (1 - p)^(n - k), evaluated with a lgamma-based log-pmf. \
         k < 0 or k > n gives 0; p = 0 or p = 1 are handled exactly.",
        Example::new(
            "empty trial set",
            example_args(&[
                ("k", serde_json::json!(0)),
                ("n", serde_json::json!(0)),
                ("p", serde_json::json!(0.5)),
            ]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "float64", "value": "1"}),
        )),
    )
}

fn binomial_cdf_descriptor() -> FunctionDescriptor {
    binomial_descriptor(
        "statistics.binomial_cdf",
        "Binomial cumulative distribution",
        "Cumulative distribution function of the binomial distribution.",
        "cdf(k) = P(X <= k) = I_(1-p)(n - k, k + 1), the regularized incomplete beta \
         function. k < 0 gives 0 and k >= n gives 1.",
        Example::new(
            "all trials succeed",
            example_args(&[
                ("k", serde_json::json!(10)),
                ("n", serde_json::json!(10)),
                ("p", serde_json::json!(0.5)),
            ]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "float64", "value": "1"}),
        )),
    )
}

fn invoke_binomial_pmf(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.binomial_pmf")?;
    let (k, n, p) = binomial_parameters(args)?;
    Ok(Outcome::approximate(float_value(binomial_pmf(k, n, p)?)?))
}

fn invoke_binomial_cdf(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.binomial_cdf")?;
    let (k, n, p) = binomial_parameters(args)?;
    Ok(Outcome::approximate(float_value(binomial_cdf(k, n, p)?)?))
}

// ---------------------------------------------------------------------------
// Chi-square
// ---------------------------------------------------------------------------

fn chi_square_descriptor(
    id: &str,
    title: &str,
    summary: &str,
    description: &str,
    example: Example,
) -> FunctionDescriptor {
    FunctionDescriptor::new(id, "statistics", "1.0.0", title, summary)
        .with_description(description)
        .with_parameters(vec![
            ParamDescriptor::required("x", "Quantile.", any_number_schema()),
            ParamDescriptor::required(
                "df",
                "Degrees of freedom; must be > 0.",
                any_number_schema(),
            ),
        ])
        .with_output(float64_schema(), "Chi-square distribution value.")
        .with_modes(inferential_modes())
        .with_cost(CostClass::Constant)
        .with_method_ref(format!(
            "docs/methods/statistics.md#{}",
            id.rsplit('.').next().unwrap()
        ))
        .with_examples(vec![example])
}

fn chi_square_pdf_descriptor() -> FunctionDescriptor {
    chi_square_descriptor(
        "statistics.chi_square_pdf",
        "Chi-square probability density",
        "Probability density of the chi-square distribution.",
        "pdf(x) = x^(df/2 - 1) e^(-x/2) / (2^(df/2) Gamma(df/2)) for x >= 0, evaluated in log \
         space with a Lanczos log-gamma. For x < 0 the density is 0; at x = 0 the density is \
         unbounded for df < 2 and 0.5 for df = 2.",
        Example::new(
            "density at zero for df = 2",
            example_args(&[("x", serde_json::json!(0)), ("df", serde_json::json!(2))]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "float64", "value": "0.5"}),
        )),
    )
}

fn chi_square_cdf_descriptor() -> FunctionDescriptor {
    chi_square_descriptor(
        "statistics.chi_square_cdf",
        "Chi-square cumulative distribution",
        "Cumulative distribution function of the chi-square distribution.",
        "cdf(x) = P(df/2, x/2), the regularized lower incomplete gamma function, evaluated \
         with the series for x < a + 1 and the continued fraction otherwise.",
        Example::new(
            "zero quantile",
            example_args(&[("x", serde_json::json!(0)), ("df", serde_json::json!(3))]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "float64", "value": "0"}),
        )),
    )
}

fn chi_square_sf_descriptor() -> FunctionDescriptor {
    chi_square_descriptor(
        "statistics.chi_square_sf",
        "Chi-square survival function",
        "Upper tail probability of the chi-square distribution.",
        "sf(x) = Q(df/2, x/2), the regularized upper incomplete gamma function, computed \
         directly so the upper tail keeps relative accuracy.",
        Example::new(
            "zero quantile",
            example_args(&[("x", serde_json::json!(0)), ("df", serde_json::json!(3))]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "float64", "value": "1"}),
        )),
    )
}

fn chi_square_quantile_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "statistics.chi_square_quantile",
        "statistics",
        "1.0.0",
        "Chi-square quantile",
        "Inverse chi-square CDF.",
    )
    .with_description(
        "Returns x such that cdf(x) = p, solved by bracket expansion, bisection, and Newton \
         polishing to a relative tolerance of 1e-15. p must be in [0, 1); p = 0 returns 0 and \
         p = 1 is rejected because the quantile is unbounded. df must be positive.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("p", "Probability in [0, 1).", any_number_schema()),
        ParamDescriptor::required(
            "df",
            "Degrees of freedom; must be > 0.",
            any_number_schema(),
        ),
    ])
    .with_output(float64_schema(), "Chi-square quantile.")
    .with_modes(inferential_modes())
    .with_cost(CostClass::Iterative)
    .with_method_ref("docs/methods/statistics.md#chi_square_quantile")
    .with_examples(vec![
        Example::new(
            "zero probability",
            example_args(&[("p", serde_json::json!(0)), ("df", serde_json::json!(3))]),
        )
        .with_value(parse_value(
            serde_json::json!({"kind": "float64", "value": "0"}),
        )),
    ])
}

fn invoke_chi_square_pdf(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.chi_square_pdf")?;
    let x = scalar_f64(args, "x")?;
    let df = scalar_f64(args, "df")?;
    Ok(Outcome::approximate(float_value(chi_square_pdf(x, df)?)?))
}

fn invoke_chi_square_cdf(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.chi_square_cdf")?;
    let x = scalar_f64(args, "x")?;
    let df = scalar_f64(args, "df")?;
    Ok(Outcome::approximate(float_value(chi_square_cdf(x, df)?)?))
}

fn invoke_chi_square_sf(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.chi_square_sf")?;
    let x = scalar_f64(args, "x")?;
    let df = scalar_f64(args, "df")?;
    Ok(Outcome::approximate(float_value(chi_square_sf(x, df)?)?))
}

fn invoke_chi_square_quantile(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.chi_square_quantile")?;
    let p = scalar_f64(args, "p")?;
    let df = scalar_f64(args, "df")?;
    Ok(Outcome::approximate(float_value(chi_square_quantile(
        p, df,
    )?)?))
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

pub fn functions() -> Vec<Arc<dyn bicmath_core::contract::Function>> {
    vec![
        SimpleFunction::arc(normal_pdf_descriptor(), invoke_normal_pdf),
        SimpleFunction::arc(normal_cdf_descriptor(), invoke_normal_cdf),
        SimpleFunction::arc(normal_sf_descriptor(), invoke_normal_sf),
        SimpleFunction::arc(normal_quantile_descriptor(), invoke_normal_quantile),
        SimpleFunction::arc(student_t_pdf_descriptor(), invoke_student_t_pdf),
        SimpleFunction::arc(student_t_cdf_descriptor(), invoke_student_t_cdf),
        SimpleFunction::arc(student_t_sf_descriptor(), invoke_student_t_sf),
        SimpleFunction::arc(student_t_quantile_descriptor(), invoke_student_t_quantile),
        SimpleFunction::arc(binomial_pmf_descriptor(), invoke_binomial_pmf),
        SimpleFunction::arc(binomial_cdf_descriptor(), invoke_binomial_cdf),
        SimpleFunction::arc(chi_square_pdf_descriptor(), invoke_chi_square_pdf),
        SimpleFunction::arc(chi_square_cdf_descriptor(), invoke_chi_square_cdf),
        SimpleFunction::arc(chi_square_sf_descriptor(), invoke_chi_square_sf),
        SimpleFunction::arc(chi_square_quantile_descriptor(), invoke_chi_square_quantile),
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
    fn published_reference_values() {
        close(
            as_f64(&call("statistics.normal_cdf", serde_json::json!({"x": 1.96})).unwrap()),
            0.975_002_104_851_779_5,
            1e-12,
        );
        close(
            as_f64(
                &call(
                    "statistics.normal_quantile",
                    serde_json::json!({"p": 0.975}),
                )
                .unwrap(),
            ),
            1.959_963_984_540_054,
            1e-9,
        );
        close(
            as_f64(
                &call(
                    "statistics.student_t_cdf",
                    serde_json::json!({"x": 2, "df": 10}),
                )
                .unwrap(),
            ),
            0.963_305_982_614_629_8,
            1e-12,
        );
        close(
            as_f64(
                &call(
                    "statistics.chi_square_cdf",
                    serde_json::json!({"x": 3.841458820694124, "df": 1}),
                )
                .unwrap(),
            ),
            0.95,
            1e-12,
        );
        close(
            as_f64(
                &call(
                    "statistics.binomial_cdf",
                    serde_json::json!({"k": 5, "n": 10, "p": 0.5}),
                )
                .unwrap(),
            ),
            0.623_046_875,
            1e-15,
        );
        let tail = as_f64(&call("statistics.normal_sf", serde_json::json!({"x": 10})).unwrap());
        close(tail / 7.619_853_024_160_593e-24, 1.0, 1e-9);
    }
}
