//! Bounded numerical methods: Brent root finding and adaptive Simpson
//! quadrature over restricted expressions.

use std::sync::Arc;

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, CostClass, Example, Function, FunctionDescriptor, Outcome, ParamDescriptor,
    SimpleFunction,
};
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::expr::parse_expression;
use bicmath_core::schema::{FieldSchema, ValueSchema};
use bicmath_core::value::Value;
use num_bigint::BigInt;
use num_traits::ToPrimitive;

use crate::common::*;
use crate::expr_eval::{Evaluator, validate_variable_name};

// ---------------------------------------------------------------------------
// Shared parameter handling
// ---------------------------------------------------------------------------

/// Resolve an optional tolerance: an explicit caller value always wins, and an
/// omitted one is the documented default. Budgets select working precision
/// only; they do not change method tolerances.
fn positive_tolerance(args: &Args, name: &str, default: f64) -> Result<f64, EngineError> {
    match args.optional_f64(name)? {
        None => Ok(default),
        Some(value) => {
            if value > 0.0 && value.is_finite() {
                Ok(value)
            } else {
                Err(
                    EngineError::domain(format!("{name} must be positive and finite"))
                        .with_path(name.to_string()),
                )
            }
        }
    }
}

fn optional_positive_u64(args: &Args, name: &str, default: u64) -> Result<u64, EngineError> {
    match args.optional_integer(name)? {
        None => Ok(default),
        Some(value) => value.to_u64().filter(|value| *value > 0).ok_or_else(|| {
            EngineError::domain(format!("{name} must be a positive integer"))
                .with_path(name.to_string())
        }),
    }
}

// ---------------------------------------------------------------------------
// Root finding: Brent's method
// ---------------------------------------------------------------------------

fn root_find_output_schema() -> ValueSchema {
    ValueSchema::Record {
        fields: vec![
            FieldSchema::required("root", float64_schema()).with_description("Located root."),
            FieldSchema::required("iterations", integer_schema())
                .with_description("Brent iterations performed."),
            FieldSchema::required("evaluations", integer_schema())
                .with_description("Expression evaluations performed."),
            FieldSchema::required("residual", float64_schema())
                .with_description("|f(root)| at the returned root."),
            FieldSchema::optional("tolerance", float64_schema())
                .with_description("Effective absolute x tolerance used (the caller's value or the documented default)."),
            FieldSchema::required("converged", ValueSchema::Bool)
                .with_description("Always true for a successful result."),
            FieldSchema::required("method", ValueSchema::text())
                .with_description("Always \"brent\"."),
            FieldSchema::required(
                "bracket",
                ValueSchema::Record {
                    fields: vec![
                        FieldSchema::required("lower", float64_schema()),
                        FieldSchema::required("upper", float64_schema()),
                    ],
                    allow_extra: false,
                },
            )
            .with_description("The supplied bracket endpoints."),
        ],
        allow_extra: false,
    }
}

pub(crate) fn root_find_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "scientific.root_find",
        MODULE,
        VERSION,
        "Root find",
        "Find a root of a scalar expression on a sign-change bracket.",
    )
    .with_description(
        "Brent's method on the required bracket [lower, upper], which must satisfy \
         f(lower) * f(upper) <= 0. The expression is a restricted string using the named \
         variable, numeric literals, parentheses, + - * / % ^, and the elementary functions \
         sin, cos, tan, asin, acos, atan, atan2, sinh, cosh, tanh, exp, ln, log, log10, \
         log2, sqrt, cbrt, abs, floor, ceil, round, min, max, and pow. Non-convergence is \
         reported as an error; the last iterate is never returned as a success. Defaults: \
         tolerance 1e-12 and 1000 iterations, bounded by the execution context.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "expression",
            "Restricted expression in the variable.",
            expression_schema(),
        ),
        ParamDescriptor::required(
            "variable",
            "Name of the single variable used by the expression.",
            ValueSchema::text(),
        ),
        ParamDescriptor::required("lower", "Lower bracket endpoint.", any_number_schema()),
        ParamDescriptor::required("upper", "Upper bracket endpoint.", any_number_schema()),
        ParamDescriptor::optional(
            "tolerance",
            "Absolute x tolerance; default 1e-12.",
            any_number_schema(),
        ),
        ParamDescriptor::optional(
            "max_iterations",
            "Iteration cap; default 1000, never above the context limit.",
            integer_schema(),
        ),
    ])
    .with_output(
        root_find_output_schema(),
        "Root, iteration and evaluation counts, residual, convergence flag, method, and bracket.",
    )
    .with_modes(scientific_modes())
    .with_cost(CostClass::Iterative)
    .with_method_ref("docs/methods/scientific.md#root_find")
    .with_examples(vec![
        Example::new(
            "root of x - 1 on [0, 2]",
            example_args(&[
                ("expression", serde_json::json!("x - 1")),
                ("variable", serde_json::json!("x")),
                ("lower", serde_json::json!(0)),
                ("upper", serde_json::json!(2)),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "root": {"kind": "float64", "value": "1"},
            "iterations": {"kind": "integer", "value": "2"},
            "evaluations": {"kind": "integer", "value": "3"},
            "residual": {"kind": "float64", "value": "0"},
            "converged": true,
            "method": "brent",
            "bracket": {
                "lower": {"kind": "float64", "value": "0"},
                "upper": {"kind": "float64", "value": "2"}
            }
        }))),
        Example::new(
            "no sign change",
            example_args(&[
                ("expression", serde_json::json!("x^2 + 1")),
                ("variable", serde_json::json!("x")),
                ("lower", serde_json::json!(-1)),
                ("upper", serde_json::json!(1)),
            ]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

struct BrentOutcome {
    root: f64,
    iterations: u64,
    evaluations: u64,
    residual: f64,
}

pub(crate) fn invoke_root_find(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_scientific(ctx, "scientific.root_find")?;
    let source = args.text("expression")?;
    let variable = args.text("variable")?;
    validate_variable_name(variable)?;
    let lower = angle_argument(args, "lower")?;
    let upper = angle_argument(args, "upper")?;
    let tolerance = positive_tolerance(args, "tolerance", 1e-12)?;
    let max_iterations = optional_positive_u64(args, "max_iterations", 1000)?;
    let max_iterations = max_iterations.min(ctx.limits.max_iterations);
    let expression = parse_expression(source, &ctx.limits)?;
    let mut evaluator = Evaluator::new(&expression, variable);
    let result = brent(&mut evaluator, lower, upper, tolerance, max_iterations, ctx)?;
    let value = Value::record([
        ("root", float_value(result.root)?),
        (
            "iterations",
            Value::integer(BigInt::from(result.iterations)),
        ),
        (
            "evaluations",
            Value::integer(BigInt::from(result.evaluations)),
        ),
        ("residual", float_value(result.residual)?),
        ("converged", Value::Bool(true)),
        ("method", Value::text("brent")),
        (
            "bracket",
            Value::record([
                ("lower", float_value(lower)?),
                ("upper", float_value(upper)?),
            ]),
        ),
    ]);
    Ok(Outcome::approximate(value))
}

#[allow(clippy::float_cmp)]
fn brent(
    evaluator: &mut Evaluator<'_>,
    lower: f64,
    upper: f64,
    tolerance: f64,
    max_iterations: u64,
    ctx: &ExecContext,
) -> Result<BrentOutcome, EngineError> {
    let mut a = lower;
    let mut b = upper;
    let mut fa = evaluator.evaluate(a)?;
    let mut fb = evaluator.evaluate(b)?;
    if fa == 0.0 {
        return Ok(BrentOutcome {
            root: a,
            iterations: 0,
            evaluations: evaluator.evaluations,
            residual: 0.0,
        });
    }
    if fb == 0.0 {
        return Ok(BrentOutcome {
            root: b,
            iterations: 0,
            evaluations: evaluator.evaluations,
            residual: 0.0,
        });
    }
    if (fa > 0.0) == (fb > 0.0) {
        return Err(EngineError::domain(format!(
            "root_find requires a sign-change bracket: f({lower}) = {fa} and f({upper}) = {fb} \
             have the same sign"
        )));
    }
    let mut c = a;
    let mut fc = fa;
    let mut d = b - a;
    let mut e = d;
    let mut iterations = 0u64;
    while iterations < max_iterations {
        iterations += 1;
        if (fb > 0.0 && fc > 0.0) || (fb < 0.0 && fc < 0.0) {
            c = a;
            fc = fa;
            d = b - a;
            e = d;
        }
        if fc.abs() < fb.abs() {
            a = b;
            b = c;
            c = a;
            fa = fb;
            fb = fc;
            fc = fa;
        }
        let tol1 = 2.0 * f64::EPSILON * b.abs() + 0.5 * tolerance;
        let xm = 0.5 * (c - b);
        if xm.abs() <= tol1 || fb == 0.0 {
            return Ok(BrentOutcome {
                root: b,
                iterations,
                evaluations: evaluator.evaluations,
                residual: fb.abs(),
            });
        }
        if e.abs() >= tol1 && fa.abs() > fb.abs() {
            let s = fb / fa;
            let (mut p, mut q) = if a == c {
                (2.0 * xm * s, 1.0 - s)
            } else {
                let q = fa / fc;
                let r = fb / fc;
                (
                    s * (2.0 * xm * q * (q - r) - (b - a) * (r - 1.0)),
                    (q - 1.0) * (r - 1.0) * (s - 1.0),
                )
            };
            if p > 0.0 {
                q = -q;
            }
            p = p.abs();
            let min1 = 3.0 * xm * q - (tol1 * q).abs();
            let min2 = (e * q).abs();
            if q != 0.0 && 2.0 * p < min1.min(min2) {
                e = d;
                d = p / q;
            } else {
                d = xm;
                e = d;
            }
        } else {
            d = xm;
            e = d;
        }
        a = b;
        fa = fb;
        if d.abs() > tol1 {
            b += d;
        } else if xm > 0.0 {
            b += tol1;
        } else {
            b -= tol1;
        }
        ctx.check()?;
        fb = evaluator.evaluate(b)?;
        if evaluator.evaluations > ctx.limits.max_operations {
            return Err(EngineError::new(
                ErrorCode::ResourceLimit,
                format!(
                    "root_find exceeded the operation budget of {}",
                    ctx.limits.max_operations
                ),
            ));
        }
    }
    Err(EngineError::new(
        ErrorCode::NonConvergence,
        format!("root_find did not converge within {max_iterations} iterations"),
    ))
}

// ---------------------------------------------------------------------------
// Numerical integration: adaptive Simpson with Richardson extrapolation
// ---------------------------------------------------------------------------

const MAX_SUBDIVISION_DEPTH: u32 = 50;

fn integrate_output_schema() -> ValueSchema {
    ValueSchema::Record {
        fields: vec![
            FieldSchema::required("integral", float64_schema())
                .with_description("Approximate value of the integral."),
            FieldSchema::required("error_estimate", float64_schema())
                .with_description("Sum of the Richardson error estimates over accepted panels."),
            FieldSchema::required("evaluations", integer_schema())
                .with_description("Expression evaluations performed."),
            FieldSchema::required("method", ValueSchema::text())
                .with_description("Always \"adaptive_simpson\"."),
            FieldSchema::required("converged", ValueSchema::Bool)
                .with_description("Always true for a successful result."),
        ],
        allow_extra: false,
    }
}

pub(crate) fn integrate_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "scientific.integrate",
        MODULE,
        VERSION,
        "Numerical integration",
        "Definite integral of a restricted expression by adaptive Simpson quadrature.",
    )
    .with_description(
        "Adaptive Simpson quadrature with Richardson extrapolation over [lower, upper]. The \
         expression uses the same restricted grammar as scientific.root_find. The returned \
         error_estimate is the accumulated Richardson estimate, not a rigorous bound. \
         Non-convergence or an exhausted evaluation budget is reported as an error. \
         Defaults: tolerance 1e-12 and 100000 evaluations, bounded by the execution context.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "expression",
            "Restricted expression in the variable.",
            expression_schema(),
        ),
        ParamDescriptor::required(
            "variable",
            "Name of the single variable used by the expression.",
            ValueSchema::text(),
        ),
        ParamDescriptor::required("lower", "Lower integration limit.", any_number_schema()),
        ParamDescriptor::required("upper", "Upper integration limit.", any_number_schema()),
        ParamDescriptor::optional(
            "tolerance",
            "Absolute error tolerance; default 1e-12.",
            any_number_schema(),
        ),
        ParamDescriptor::optional(
            "max_evaluations",
            "Evaluation budget; default 100000, never above the context limit.",
            integer_schema(),
        ),
    ])
    .with_output(
        integrate_output_schema(),
        "Integral estimate, Richardson error estimate, evaluation count, method, and \
         convergence flag.",
    )
    .with_modes(scientific_modes())
    .with_cost(CostClass::Iterative)
    .with_method_ref("docs/methods/scientific.md#integrate")
    .with_examples(vec![
        Example::new(
            "integral of x on [0, 1]",
            example_args(&[
                ("expression", serde_json::json!("x")),
                ("variable", serde_json::json!("x")),
                ("lower", serde_json::json!(0)),
                ("upper", serde_json::json!(1)),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "integral": {"kind": "float64", "value": "0.5"},
            "error_estimate": {"kind": "float64", "value": "0"},
            "evaluations": {"kind": "integer", "value": "5"},
            "method": "adaptive_simpson",
            "converged": true
        }))),
        Example::new(
            "non-finite integrand",
            example_args(&[
                ("expression", serde_json::json!("ln(x)")),
                ("variable", serde_json::json!("x")),
                ("lower", serde_json::json!(-1)),
                ("upper", serde_json::json!(1)),
            ]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

struct SimpsonOutcome {
    integral: f64,
    error_estimate: f64,
    evaluations: u64,
}

pub(crate) fn invoke_integrate(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_scientific(ctx, "scientific.integrate")?;
    let source = args.text("expression")?;
    let variable = args.text("variable")?;
    validate_variable_name(variable)?;
    let lower = angle_argument(args, "lower")?;
    let upper = angle_argument(args, "upper")?;
    let tolerance = positive_tolerance(args, "tolerance", 1e-12)?;
    let max_evaluations = optional_positive_u64(args, "max_evaluations", 100_000)?;
    let max_evaluations = max_evaluations.min(ctx.limits.max_operations);
    let expression = parse_expression(source, &ctx.limits)?;
    let mut evaluator = Evaluator::new(&expression, variable);
    let result = adaptive_simpson(
        &mut evaluator,
        lower,
        upper,
        tolerance,
        max_evaluations,
        ctx,
    )?;
    let value = Value::record([
        ("integral", float_value(result.integral)?),
        ("error_estimate", float_value(result.error_estimate)?),
        (
            "evaluations",
            Value::integer(BigInt::from(result.evaluations)),
        ),
        ("method", Value::text("adaptive_simpson")),
        ("converged", Value::Bool(true)),
    ]);
    Ok(Outcome::approximate(value))
}

struct Integrator<'a, 'e, 'c> {
    evaluator: &'a mut Evaluator<'e>,
    ctx: &'c ExecContext,
    max_evaluations: u64,
    error_estimate: f64,
}

impl Integrator<'_, '_, '_> {
    fn evaluate(&mut self, x: f64) -> Result<f64, EngineError> {
        if self.evaluator.evaluations >= self.max_evaluations {
            return Err(EngineError::new(
                ErrorCode::NonConvergence,
                format!(
                    "integrate exhausted the evaluation budget of {} before meeting the tolerance",
                    self.max_evaluations
                ),
            ));
        }
        self.ctx.check()?;
        self.evaluator.evaluate(x)
    }

    #[allow(clippy::too_many_arguments)]
    fn recurse(
        &mut self,
        a: f64,
        b: f64,
        fa: f64,
        fm: f64,
        fb: f64,
        whole: f64,
        eps: f64,
        depth: u32,
    ) -> Result<f64, EngineError> {
        let middle = 0.5 * (a + b);
        let left_mid = 0.5 * (a + middle);
        let right_mid = 0.5 * (middle + b);
        let flm = self.evaluate(left_mid)?;
        let frm = self.evaluate(right_mid)?;
        let left = (middle - a) / 6.0 * (fa + 4.0 * flm + fm);
        let right = (b - middle) / 6.0 * (fm + 4.0 * frm + fb);
        let delta = left + right - whole;
        if delta.abs() <= 15.0 * eps {
            self.error_estimate += delta.abs() / 15.0;
            return Ok(left + right + delta / 15.0);
        }
        if depth >= MAX_SUBDIVISION_DEPTH {
            return Err(EngineError::new(
                ErrorCode::NonConvergence,
                format!(
                    "integrate reached the maximum subdivision depth of {MAX_SUBDIVISION_DEPTH} \
                     without meeting the tolerance"
                ),
            ));
        }
        let left_value = self.recurse(a, middle, fa, flm, fm, left, 0.5 * eps, depth + 1)?;
        let right_value = self.recurse(middle, b, fm, frm, fb, right, 0.5 * eps, depth + 1)?;
        Ok(left_value + right_value)
    }
}

fn adaptive_simpson(
    evaluator: &mut Evaluator<'_>,
    lower: f64,
    upper: f64,
    tolerance: f64,
    max_evaluations: u64,
    ctx: &ExecContext,
) -> Result<SimpsonOutcome, EngineError> {
    let mut integrator = Integrator {
        evaluator,
        ctx,
        max_evaluations,
        error_estimate: 0.0,
    };
    let middle = 0.5 * (lower + upper);
    let fa = integrator.evaluate(lower)?;
    let fb = integrator.evaluate(upper)?;
    let fm = integrator.evaluate(middle)?;
    let whole = (upper - lower) / 6.0 * (fa + 4.0 * fm + fb);
    let integral = integrator.recurse(lower, upper, fa, fm, fb, whole, tolerance, 0)?;
    Ok(SimpsonOutcome {
        integral,
        error_estimate: integrator.error_estimate,
        evaluations: integrator.evaluator.evaluations,
    })
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

pub(crate) fn functions() -> Vec<Arc<dyn Function>> {
    vec![
        SimpleFunction::arc(root_find_descriptor(), invoke_root_find),
        SimpleFunction::arc(integrate_descriptor(), invoke_integrate),
    ]
}
