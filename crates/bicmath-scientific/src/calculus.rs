//! Numerical differentiation and ordinary differential equations over
//! restricted expressions.
//!
//! Both functions evaluate a restricted expression with [`Evaluator`] and
//! respect the execution context: every iteration checks cancellation and the
//! deadline, and every result is bounded by the configured iteration and
//! operation budgets.

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
use crate::f64math;

const MAX_DIFFERENTIATION_ORDER: u32 = 4;

// ---------------------------------------------------------------------------
// Shared parameter handling
// ---------------------------------------------------------------------------

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

/// Compensated (Kahan) linear combination `sum coefficient * value`.
fn compensated_combination(coefficients: &[(f64, f64)]) -> f64 {
    let mut sum = 0.0;
    let mut compensation = 0.0;
    for (coefficient, value) in coefficients {
        let term = coefficient * value - compensation;
        let next = sum + term;
        compensation = (next - sum) - term;
        sum = next;
    }
    sum
}

// ---------------------------------------------------------------------------
// Numerical differentiation
// ---------------------------------------------------------------------------

fn differentiate_output_schema() -> ValueSchema {
    ValueSchema::Record {
        fields: vec![
            FieldSchema::required("value", float64_schema())
                .with_description("Derivative estimate."),
            FieldSchema::required("error_estimate", float64_schema())
                .with_description("Difference between successive Richardson levels."),
            FieldSchema::required("step", float64_schema())
                .with_description("Smallest difference step used."),
            FieldSchema::required("method", ValueSchema::text())
                .with_description("Always \"richardson\" or \"central\"."),
            FieldSchema::required("order", integer_schema())
                .with_description("Derivative order between one and four."),
        ],
        allow_extra: false,
    }
}

pub(crate) fn differentiate_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "scientific.differentiate",
        MODULE,
        VERSION,
        "Numerical derivative",
        "Numerical derivative of a restricted expression by central differences.",
    )
    .with_description(
        "Central-difference estimates of the first through fourth derivative of a restricted \
         expression at a point. The expression uses the same restricted grammar as \
         scientific.root_find. With method \"richardson\" (the default) the estimates at \
         h, h/2, and h/4 are combined by Richardson extrapolation, which cancels the leading \
         O(h^2) error and returns the two-level estimate with |r2 - r1| as the error estimate. \
         With method \"central\" the plain h/2 central difference is returned with the \
         h-extrapolated difference as the error estimate. Orders above four are rejected.",
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
        ParamDescriptor::required(
            "at",
            "Point at which to differentiate.",
            any_number_schema(),
        ),
        ParamDescriptor::optional(
            "order",
            "Derivative order between 1 and 4; default 1.",
            integer_schema(),
        ),
        ParamDescriptor::optional(
            "method",
            "Difference scheme: \"richardson\" (default) or \"central\".",
            ValueSchema::Enum {
                variants: vec!["richardson".to_string(), "central".to_string()],
            },
        ),
    ])
    .with_output(
        differentiate_output_schema(),
        "Derivative estimate, error estimate, step, method, and order.",
    )
    .with_modes(scientific_modes())
    .with_cost(CostClass::Constant)
    .with_method_ref("docs/methods/scientific.md#differentiate")
    .with_examples(vec![
        Example::new(
            "second derivative of x squared at zero",
            example_args(&[
                ("expression", serde_json::json!("x^2")),
                ("variable", serde_json::json!("x")),
                ("at", serde_json::json!(0)),
                ("order", serde_json::json!(2)),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "value": {"kind": "float64", "value": "2"},
            "error_estimate": {"kind": "float64", "value": "0"},
            "step": {"kind": "float64", "value": "0.0025"},
            "method": "richardson",
            "order": {"kind": "integer", "value": "2"}
        }))),
        Example::new(
            "order above the supported maximum",
            example_args(&[
                ("expression", serde_json::json!("x^2")),
                ("variable", serde_json::json!("x")),
                ("at", serde_json::json!(0)),
                ("order", serde_json::json!(5)),
            ]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

pub(crate) fn invoke_differentiate(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_scientific(ctx, "scientific.differentiate")?;
    ctx.check()?;
    let source = args.text("expression")?;
    let variable = args.text("variable")?;
    validate_variable_name(variable)?;
    let at = real_argument(args, "at")?;
    let order = match args.optional_integer("order")? {
        None => 1,
        Some(value) => value
            .to_u32()
            .filter(|order| (1..=MAX_DIFFERENTIATION_ORDER).contains(order))
            .ok_or_else(|| {
                EngineError::domain(format!(
                    "differentiate supports orders 1 through {MAX_DIFFERENTIATION_ORDER}; \
                 received {value}"
                ))
                .with_path("order".to_string())
            })?,
    };
    let method = args.optional_text("method")?.unwrap_or("richardson");
    let expression = parse_expression(source, &ctx.limits)?;
    let mut evaluator = Evaluator::new(&expression, variable);
    let scale = at.abs().max(1.0);
    let base = scale * 1e-2;
    let (value, error_estimate, step) = match method {
        "richardson" => {
            let coarse = central_difference(&mut evaluator, at, order, base)?;
            let medium = central_difference(&mut evaluator, at, order, base * 0.5)?;
            let fine = central_difference(&mut evaluator, at, order, base * 0.25)?;
            let first = (4.0 * medium - coarse) / 3.0;
            let second = (4.0 * fine - medium) / 3.0;
            (second, (second - first).abs(), base * 0.25)
        }
        "central" => {
            let coarse = central_difference(&mut evaluator, at, order, base)?;
            let fine = central_difference(&mut evaluator, at, order, base * 0.5)?;
            (fine, ((fine - coarse) / 3.0).abs(), base * 0.5)
        }
        other => {
            return Err(EngineError::domain(format!(
                "unknown differentiation method {other:?}; expected \"richardson\" or \"central\""
            ))
            .with_path("method".to_string()));
        }
    };
    if evaluator.evaluations > ctx.limits.max_operations {
        return Err(EngineError::new(
            ErrorCode::ResourceLimit,
            format!(
                "differentiate exceeded the operation budget of {}",
                ctx.limits.max_operations
            ),
        ));
    }
    let value = Value::record([
        ("value", float_value(value)?),
        ("error_estimate", float_value(error_estimate)?),
        ("step", float_value(step)?),
        ("method", Value::text(method)),
        ("order", Value::integer(BigInt::from(order))),
    ]);
    Ok(Outcome::approximate(value))
}

fn central_difference(
    evaluator: &mut Evaluator<'_>,
    at: f64,
    order: u32,
    step: f64,
) -> Result<f64, EngineError> {
    let two_h = 2.0 * step;
    Ok(match order {
        1 => (evaluator.evaluate(at + step)? - evaluator.evaluate(at - step)?) / two_h,
        2 => {
            (evaluator.evaluate(at + step)? - 2.0 * evaluator.evaluate(at)?
                + evaluator.evaluate(at - step)?)
                / (step * step)
        }
        3 => {
            (evaluator.evaluate(at + two_h)? - 2.0 * evaluator.evaluate(at + step)?
                + 2.0 * evaluator.evaluate(at - step)?
                - evaluator.evaluate(at - two_h)?)
                / (two_h * step * step)
        }
        4 => {
            (evaluator.evaluate(at + two_h)? - 4.0 * evaluator.evaluate(at + step)?
                + 6.0 * evaluator.evaluate(at)?
                - 4.0 * evaluator.evaluate(at - step)?
                + evaluator.evaluate(at - two_h)?)
                / (step * step * step * step)
        }
        other => {
            return Err(EngineError::domain(format!(
                "differentiate supports orders 1 through {MAX_DIFFERENTIATION_ORDER}; \
                 received {other}"
            ))
            .with_path("order".to_string()));
        }
    })
}

// ---------------------------------------------------------------------------
// Ordinary differential equations: Dormand-Prince 5(4)
// ---------------------------------------------------------------------------

const C2: f64 = 1.0 / 5.0;
const C3: f64 = 3.0 / 10.0;
const C4: f64 = 4.0 / 5.0;
const C5: f64 = 8.0 / 9.0;
const A21: f64 = 1.0 / 5.0;
const A31: f64 = 3.0 / 40.0;
const A32: f64 = 9.0 / 40.0;
const A41: f64 = 44.0 / 45.0;
const A42: f64 = -56.0 / 15.0;
const A43: f64 = 32.0 / 9.0;
const A51: f64 = 19372.0 / 6561.0;
const A52: f64 = -25360.0 / 2187.0;
const A53: f64 = 64448.0 / 6561.0;
const A54: f64 = -212.0 / 729.0;
const A61: f64 = 9017.0 / 3168.0;
const A62: f64 = -355.0 / 33.0;
const A63: f64 = 46732.0 / 5247.0;
const A64: f64 = 49.0 / 176.0;
const A65: f64 = -5103.0 / 18656.0;
const B1: f64 = 35.0 / 384.0;
const B3: f64 = 500.0 / 1113.0;
const B4: f64 = 125.0 / 192.0;
const B5: f64 = -2187.0 / 6784.0;
const B6: f64 = 11.0 / 84.0;
const E1: f64 = 71.0 / 57600.0;
const E3: f64 = -71.0 / 16695.0;
const E4: f64 = 71.0 / 1920.0;
const E5: f64 = -17253.0 / 339200.0;
const E6: f64 = 22.0 / 525.0;
const E7: f64 = -1.0 / 40.0;

fn ode_output_schema() -> ValueSchema {
    ValueSchema::Record {
        fields: vec![
            FieldSchema::required(
                "solution",
                ValueSchema::Array {
                    items: Box::new(ValueSchema::Record {
                        fields: vec![
                            FieldSchema::required("t", float64_schema()),
                            FieldSchema::required("y", float64_schema()),
                        ],
                        allow_extra: false,
                    }),
                    min_len: Some(1),
                    max_len: None,
                },
            )
            .with_description("Accepted points, starting at (t0, y0)."),
            FieldSchema::required("final_t", float64_schema())
                .with_description("Independent variable at the final accepted point."),
            FieldSchema::required("final_y", float64_schema())
                .with_description("Dependent variable at the final accepted point."),
            FieldSchema::required("steps", integer_schema())
                .with_description("Total attempted steps, accepted plus rejected."),
            FieldSchema::required("accepted_steps", integer_schema())
                .with_description("Accepted steps."),
            FieldSchema::required("rejected_steps", integer_schema())
                .with_description("Rejected steps."),
            FieldSchema::required("method", ValueSchema::text())
                .with_description("Always \"dormand_prince_45\"."),
            FieldSchema::required("converged", ValueSchema::Bool)
                .with_description("Always true for a successful result."),
        ],
        allow_extra: false,
    }
}

pub(crate) fn ode_rk45_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "scientific.ode_rk45",
        MODULE,
        VERSION,
        "Dormand-Prince RK45",
        "Adaptive Dormand-Prince 5(4) integration of dy/dt = f(t, y).",
    )
    .with_description(
        "Solves the scalar initial value problem dy/dt = f(t, y) with y(t0) = initial_value \
         on the interval from t0 to t1 using the Dormand-Prince 5(4) embedded Runge-Kutta \
         pair with adaptive step-size control. The expression may reference the independent \
         variable t and the dependent variable named by `variable` (which must not be t); it \
         uses the same restricted grammar as scientific.root_find. The returned solution \
         contains the initial point and every accepted step. The step-size control uses the \
         tolerance as both relative and absolute tolerance. If max_steps is exhausted before \
         t1 the call fails with a non-convergence error instead of returning a partial \
         solution as a success. Defaults: step (t1 - t0) / 100, tolerance 1e-8, max_steps \
         10000, bounded by the execution context.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "expression",
            "Restricted expression for f(t, y) in the variables t and the dependent variable.",
            ValueSchema::Expression {
                variables: vec!["t".to_string(), "y".to_string()],
            },
        ),
        ParamDescriptor::required(
            "variable",
            "Name of the dependent variable; t is reserved for the independent variable.",
            ValueSchema::text(),
        ),
        ParamDescriptor::required(
            "initial_value",
            "Value of the dependent variable at t0.",
            any_number_schema(),
        ),
        ParamDescriptor::required(
            "t0",
            "Start of the integration interval.",
            any_number_schema(),
        ),
        ParamDescriptor::required(
            "t1",
            "End of the integration interval.",
            any_number_schema(),
        ),
        ParamDescriptor::optional(
            "step",
            "Initial step size; default (t1 - t0) / 100.",
            any_number_schema(),
        ),
        ParamDescriptor::optional(
            "tolerance",
            "Relative and absolute error tolerance; default 1e-8.",
            any_number_schema(),
        ),
        ParamDescriptor::optional(
            "max_steps",
            "Attempted step cap; default 10000, never above the context limit.",
            integer_schema(),
        ),
    ])
    .with_output(
        ode_output_schema(),
        "Accepted solution points, final values, step counts, method, and convergence flag.",
    )
    .with_modes(scientific_modes())
    .with_cost(CostClass::Iterative)
    .with_method_ref("docs/methods/scientific.md#ode_rk45")
    .with_examples(vec![
        Example::new(
            "constant slope from zero to one",
            example_args(&[
                ("expression", serde_json::json!("1")),
                ("variable", serde_json::json!("y")),
                ("initial_value", serde_json::json!(0)),
                ("t0", serde_json::json!(0)),
                ("t1", serde_json::json!(1)),
                ("step", serde_json::json!(1)),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "solution": [
                {"t": {"kind": "float64", "value": "0"}, "y": {"kind": "float64", "value": "0"}},
                {"t": {"kind": "float64", "value": "1"}, "y": {"kind": "float64", "value": "1"}}
            ],
            "final_t": {"kind": "float64", "value": "1"},
            "final_y": {"kind": "float64", "value": "1"},
            "steps": {"kind": "integer", "value": "1"},
            "accepted_steps": {"kind": "integer", "value": "1"},
            "rejected_steps": {"kind": "integer", "value": "0"},
            "method": "dormand_prince_45",
            "converged": true
        }))),
        Example::new(
            "step budget exhausted",
            example_args(&[
                ("expression", serde_json::json!("y")),
                ("variable", serde_json::json!("y")),
                ("initial_value", serde_json::json!(1)),
                ("t0", serde_json::json!(0)),
                ("t1", serde_json::json!(1)),
                ("step", serde_json::json!(0.1)),
                ("max_steps", serde_json::json!(1)),
            ]),
        )
        .with_error(ErrorCode::NonConvergence),
    ])
}

struct OdeOutcome {
    solution: Vec<(f64, f64)>,
    steps: u64,
    accepted: u64,
    rejected: u64,
}

pub(crate) fn invoke_ode_rk45(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_scientific(ctx, "scientific.ode_rk45")?;
    ctx.check()?;
    let source = args.text("expression")?;
    let variable = args.text("variable")?;
    validate_variable_name(variable)?;
    if variable == "t" {
        return Err(EngineError::domain(
            "ode_rk45 reserves the name t for the independent variable; choose a different \
             dependent variable name",
        )
        .with_path("variable".to_string()));
    }
    let initial_value = real_argument(args, "initial_value")?;
    let t0 = real_argument(args, "t0")?;
    let t1 = real_argument(args, "t1")?;
    if t1 == t0 {
        return Err(
            EngineError::domain("ode_rk45 requires t1 to differ from t0")
                .with_path("t1".to_string()),
        );
    }
    let default_step = (t1 - t0).abs() / 100.0;
    let initial_step = positive_tolerance(args, "step", default_step)?;
    let tolerance = positive_tolerance(args, "tolerance", 1e-8)?;
    let default_max_steps = 10_000u64;
    let requested_max_steps = optional_positive_u64(args, "max_steps", default_max_steps)?;
    let solution_cap = ctx.limits.max_array_len.saturating_sub(1) as u64;
    let max_steps = requested_max_steps
        .min(ctx.limits.max_iterations)
        .min(solution_cap.max(1));
    let expression = parse_expression(source, &ctx.limits)?;
    let mut evaluator = Evaluator::new_two(&expression, "t", variable);
    let result = rk45(
        &mut evaluator,
        t0,
        t1,
        initial_value,
        initial_step,
        tolerance,
        max_steps,
        ctx,
    )?;
    let mut points = Vec::with_capacity(result.solution.len());
    for (t, y) in &result.solution {
        points.push(Value::record([
            ("t", float_value(*t)?),
            ("y", float_value(*y)?),
        ]));
    }
    let final_point = result
        .solution
        .last()
        .copied()
        .unwrap_or((t0, initial_value));
    let value = Value::record([
        ("solution", Value::Array(points)),
        ("final_t", float_value(final_point.0)?),
        ("final_y", float_value(final_point.1)?),
        ("steps", Value::integer(BigInt::from(result.steps))),
        (
            "accepted_steps",
            Value::integer(BigInt::from(result.accepted)),
        ),
        (
            "rejected_steps",
            Value::integer(BigInt::from(result.rejected)),
        ),
        ("method", Value::text("dormand_prince_45")),
        ("converged", Value::Bool(true)),
    ]);
    Ok(Outcome::approximate(value))
}

#[allow(clippy::too_many_arguments)]
fn rk45(
    evaluator: &mut Evaluator<'_>,
    t0: f64,
    t1: f64,
    y0: f64,
    initial_step: f64,
    tolerance: f64,
    max_steps: u64,
    ctx: &ExecContext,
) -> Result<OdeOutcome, EngineError> {
    let direction = if t1 >= t0 { 1.0 } else { -1.0 };
    let mut t = t0;
    let mut y = y0;
    let mut h = initial_step.abs() * direction;
    let mut k1 = evaluator.evaluate_bindings(&[t, y])?;
    let mut solution = vec![(t, y)];
    let mut steps = 0u64;
    let mut accepted = 0u64;
    let mut rejected = 0u64;
    while direction * (t1 - t) > 0.0 {
        if steps >= max_steps {
            return Err(EngineError::new(
                ErrorCode::NonConvergence,
                format!("ode_rk45 reached the {max_steps}-step limit at t = {t} before t1 = {t1}"),
            ));
        }
        steps += 1;
        ctx.check()?;
        let clamped = direction * (t + h - t1) > 0.0;
        if clamped {
            h = t1 - t;
        }
        let k2 = evaluator.evaluate_bindings(&[t + C2 * h, y + h * A21 * k1])?;
        let k3 = evaluator.evaluate_bindings(&[t + C3 * h, y + h * (A31 * k1 + A32 * k2)])?;
        let k4 =
            evaluator.evaluate_bindings(&[t + C4 * h, y + h * (A41 * k1 + A42 * k2 + A43 * k3)])?;
        let k5 = evaluator.evaluate_bindings(&[
            t + C5 * h,
            y + h * (A51 * k1 + A52 * k2 + A53 * k3 + A54 * k4),
        ])?;
        let k6 = evaluator.evaluate_bindings(&[
            t + h,
            y + h * (A61 * k1 + A62 * k2 + A63 * k3 + A64 * k4 + A65 * k5),
        ])?;
        let y_new =
            y + h * compensated_combination(&[(B1, k1), (B3, k3), (B4, k4), (B5, k5), (B6, k6)]);
        let k7 = evaluator.evaluate_bindings(&[t + h, y_new])?;
        let error = h * compensated_combination(&[
            (E1, k1),
            (E3, k3),
            (E4, k4),
            (E5, k5),
            (E6, k6),
            (E7, k7),
        ]);
        let scale = tolerance + tolerance * y.abs().max(y_new.abs());
        let err = (error / scale).abs();
        if err <= 1.0 {
            accepted += 1;
            let previous_t = t;
            t = if clamped { t1 } else { previous_t + h };
            y = y_new;
            k1 = k7;
            solution.push((t, y));
            if t == previous_t {
                // The remaining interval is below the resolution of `t`.
                if let Some(point) = solution.last_mut() {
                    point.0 = t1;
                }
                break;
            }
            let factor = if err > 0.0 {
                0.9 * f64math::pow(err, -0.2)
            } else {
                5.0
            };
            h *= factor.clamp(0.2, 5.0);
        } else {
            rejected += 1;
            let factor = if err.is_finite() && err > 0.0 {
                0.9 * f64math::pow(err, -0.2)
            } else {
                0.1
            };
            h *= factor.clamp(0.1, 1.0);
        }
        if h == 0.0 || !h.is_finite() {
            return Err(EngineError::new(
                ErrorCode::NonConvergence,
                format!("ode_rk45 step size collapsed to {h} at t = {t}"),
            ));
        }
        if evaluator.evaluations > ctx.limits.max_operations {
            return Err(EngineError::new(
                ErrorCode::ResourceLimit,
                format!(
                    "ode_rk45 exceeded the operation budget of {}",
                    ctx.limits.max_operations
                ),
            ));
        }
    }
    if let Some(last) = solution.last_mut() {
        // Report the exact requested endpoint when the final step reached or
        // rounded past t1.
        if direction * (t1 - last.0) <= 0.0 {
            last.0 = t1;
        }
    }
    Ok(OdeOutcome {
        solution,
        steps,
        accepted,
        rejected,
    })
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

pub(crate) fn functions() -> Vec<Arc<dyn Function>> {
    vec![
        SimpleFunction::arc(differentiate_descriptor(), invoke_differentiate),
        SimpleFunction::arc(ode_rk45_descriptor(), invoke_ode_rk45),
    ]
}
