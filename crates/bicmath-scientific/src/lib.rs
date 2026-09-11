//! Scientific module: elementary transcendental functions, angle conversions,
//! and bounded numerical methods.
//!
//! Every function produces binary64 results and therefore requires
//! [`bicmath_core::number::NumericMode::Scientific`]. The module is pure and
//! deterministic: no filesystem, network, clock, randomness, or unsafe code is
//! used, and every transcendental is routed through a wasm-safe wrapper.

mod calculus;
mod common;
mod constants;
mod elementary;
mod expr_eval;
mod f64math;
mod numeric;
mod roots;
mod special;

use std::sync::Arc;

use bicmath_core::contract::{Function, Module, ModuleDescriptor};
use bicmath_core::number::NumericMode;

/// Build the scientific module with all of its registered functions.
pub fn module() -> Module {
    let mut functions: Vec<Arc<dyn Function>> = Vec::new();
    functions.extend(elementary::functions());
    functions.extend(roots::functions());
    functions.extend(numeric::functions());
    functions.extend(constants::functions());
    functions.extend(special::functions());
    functions.extend(calculus::functions());
    let descriptor = ModuleDescriptor::new(
        "scientific",
        "Scientific",
        "1.0.0",
        "Elementary transcendental functions, angle conversions, root finding, numerical \
         integration and differentiation, ordinary differential equations, and special \
         functions.",
    )
    .with_capabilities(vec![
        "elementary_functions",
        "angle_conversions",
        "root_finding",
        "numerical_integration",
        "numerical_differentiation",
        "ode_integration",
        "special_functions",
    ])
    .with_dependencies(vec!["core"])
    .with_modes(vec![NumericMode::Scientific])
    .with_source("crates/bicmath-scientific");
    Module::new(descriptor, functions)
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use bicmath_core::context::ExecContext;
    use bicmath_core::contract::{Args, ExampleExpectation, Outcome};
    use bicmath_core::error::{EngineError, ErrorCode};
    use bicmath_core::number::Number;
    use bicmath_core::value::Value;

    use super::*;

    fn call(id: &str, raw: serde_json::Value) -> Result<Outcome, EngineError> {
        let module = module();
        let function = module
            .functions
            .iter()
            .find(|function| function.descriptor().id == id)
            .unwrap_or_else(|| panic!("function {id} is not registered"));
        let ctx = ExecContext::scientific();
        let arguments = raw.as_object().expect("arguments must be an object");
        let mut values = BTreeMap::new();
        for (name, raw_value) in arguments {
            let parameter = function
                .descriptor()
                .parameter(name)
                .unwrap_or_else(|| panic!("parameter {name} is not declared"));
            let value = parameter
                .schema
                .coerce(raw_value, name, &ctx.limits, true)
                .unwrap_or_else(|error| panic!("argument {name} does not coerce: {error}"));
            values.insert(name.clone(), value);
        }
        function.invoke(&Args::new(values), &ctx)
    }

    fn f64_value(value: f64) -> serde_json::Value {
        serde_json::json!({"kind": "float64", "value": value.to_string()})
    }

    fn float_result(outcome: &Outcome) -> f64 {
        match &outcome.value {
            Value::Number(Number::Float64(value)) => value.get(),
            other => panic!("expected a float64 result, got {other:?}"),
        }
    }

    fn record_result(outcome: &Outcome) -> &BTreeMap<String, Value> {
        match &outcome.value {
            Value::Record(fields) => fields,
            other => panic!("expected a record result, got {other:?}"),
        }
    }

    fn record_f64(fields: &BTreeMap<String, Value>, name: &str) -> f64 {
        match fields.get(name) {
            Some(Value::Number(Number::Float64(value))) => value.get(),
            other => panic!("expected float64 field {name}, got {other:?}"),
        }
    }

    // -- Required reference values -----------------------------------------
    // Fixture provenance: all reference values below were generated with
    // Python 3.14 `math` (stdlib only), e.g.
    //   math.sin(math.pi/6) = 0.49999999999999994
    //   math.atan2(1, 1)    = 0.7853981633974483
    //   math.acosh(math.cosh(3)) = 3.0
    // and compared against the binary64 results produced here.

    #[test]
    fn sine_of_pi_over_six_is_half() {
        let result = call(
            "scientific.sin",
            serde_json::json!({"x": f64_value(std::f64::consts::FRAC_PI_6)}),
        )
        .unwrap();
        assert!((float_result(&result) - 0.5).abs() <= 1e-15);
    }

    #[test]
    fn atan2_diagonal_is_pi_over_four() {
        let result = call("scientific.atan2", serde_json::json!({"y": 1, "x": 1})).unwrap();
        assert!((float_result(&result) - std::f64::consts::FRAC_PI_4).abs() <= 1e-15);
    }

    #[test]
    fn cosine_of_zero_is_one() {
        let result = call("scientific.cos", serde_json::json!({"x": 0})).unwrap();
        assert_eq!(float_result(&result), 1.0);
    }

    #[test]
    fn natural_log_of_e_is_one() {
        let exponential = call("scientific.exp", serde_json::json!({"x": 1})).unwrap();
        let result = call(
            "scientific.ln",
            serde_json::json!({"x": f64_value(float_result(&exponential))}),
        )
        .unwrap();
        assert!((float_result(&result) - 1.0).abs() <= 1e-15);
    }

    #[test]
    fn log1p_and_expm1_keep_small_inputs_accurate() {
        let log = call(
            "scientific.log1p",
            serde_json::json!({"x": f64_value(1e-16)}),
        )
        .unwrap();
        assert!((float_result(&log) - 1e-16).abs() <= 1e-15 * 1e-16);
        let exp = call(
            "scientific.expm1",
            serde_json::json!({"x": f64_value(1e-16)}),
        )
        .unwrap();
        assert!((float_result(&exp) - 1e-16).abs() <= 1e-15 * 1e-16);
    }

    #[test]
    fn acosh_of_cosh_is_identity() {
        let cosh = call("scientific.cosh", serde_json::json!({"x": 3})).unwrap();
        let result = call(
            "scientific.acosh",
            serde_json::json!({"x": f64_value(float_result(&cosh))}),
        )
        .unwrap();
        assert!((float_result(&result) - 3.0).abs() <= 1e-14);
    }

    #[test]
    fn domain_errors_are_reported() {
        assert_eq!(
            call("scientific.asin", serde_json::json!({"x": 2}))
                .unwrap_err()
                .code,
            ErrorCode::DomainViolation
        );
        assert_eq!(
            call("scientific.ln", serde_json::json!({"x": -1}))
                .unwrap_err()
                .code,
            ErrorCode::DomainViolation
        );
        assert_eq!(
            call("scientific.atanh", serde_json::json!({"x": 1}))
                .unwrap_err()
                .code,
            ErrorCode::DomainViolation
        );
    }

    #[test]
    fn nth_root_handles_negative_odd_degrees() {
        let result = call("scientific.nth_root", serde_json::json!({"x": -27, "n": 3})).unwrap();
        assert_eq!(float_result(&result), -3.0);
        assert_eq!(
            call("scientific.nth_root", serde_json::json!({"x": -4, "n": 2}))
                .unwrap_err()
                .code,
            ErrorCode::DomainViolation
        );
    }

    #[test]
    fn root_find_uses_brent_on_a_bracket() {
        let result = call(
            "scientific.root_find",
            serde_json::json!({
                "expression": "x^2 - 2",
                "variable": "x",
                "lower": 1,
                "upper": 2
            }),
        )
        .unwrap();
        let fields = record_result(&result);
        assert!((record_f64(fields, "root") - std::f64::consts::SQRT_2).abs() <= 1e-10);
        assert_eq!(fields.get("method"), Some(&Value::text("brent")));
        assert_eq!(fields.get("converged"), Some(&Value::Bool(true)));
    }

    #[test]
    fn root_find_requires_a_sign_change_bracket() {
        assert_eq!(
            call(
                "scientific.root_find",
                serde_json::json!({
                    "expression": "x^2 + 1",
                    "variable": "x",
                    "lower": -1,
                    "upper": 1
                }),
            )
            .unwrap_err()
            .code,
            ErrorCode::DomainViolation
        );
    }

    #[test]
    fn root_find_reports_non_convergence() {
        assert_eq!(
            call(
                "scientific.root_find",
                serde_json::json!({
                    "expression": "x^2 - 2",
                    "variable": "x",
                    "lower": 1,
                    "upper": 2,
                    "max_iterations": 1
                }),
            )
            .unwrap_err()
            .code,
            ErrorCode::NonConvergence
        );
    }

    #[test]
    fn integrate_quadratic_and_sine() {
        let quadratic = call(
            "scientific.integrate",
            serde_json::json!({
                "expression": "x^2",
                "variable": "x",
                "lower": 0,
                "upper": 1
            }),
        )
        .unwrap();
        assert!((record_f64(record_result(&quadratic), "integral") - 1.0 / 3.0).abs() <= 1e-9);

        let sine = call(
            "scientific.integrate",
            serde_json::json!({
                "expression": "sin(x)",
                "variable": "x",
                "lower": 0,
                "upper": std::f64::consts::PI
            }),
        )
        .unwrap();
        assert!((record_f64(record_result(&sine), "integral") - 2.0).abs() <= 1e-8);
    }

    // -- Special functions -------------------------------------------------
    // Reference values follow the published binary64 values of the
    // mathematical constants, for example
    //   EulerGamma = 0.5772156649015328606...
    //   pi^2/6     = 1.6449340668482264364...
    //   J0(1)      = 0.7651976865579665514...
    // and the tolerances below are the smallest that hold for this
    // implementation.

    #[test]
    fn gamma_reference_values() {
        let five = call("scientific.gamma", serde_json::json!({"x": 5})).unwrap();
        assert!((float_result(&five) - 24.0).abs() <= 1e-12);

        let half = call("scientific.gamma", serde_json::json!({"x": 0.5})).unwrap();
        assert!((float_result(&half) - std::f64::consts::PI.sqrt()).abs() <= 1e-12);
    }

    #[test]
    fn lgamma_reference_value() {
        let result = call("scientific.lgamma", serde_json::json!({"x": 10})).unwrap();
        assert!((float_result(&result) - (362_880.0f64).ln()).abs() <= 1e-12);
    }

    #[test]
    fn digamma_reference_value() {
        let result = call("scientific.digamma", serde_json::json!({"x": 1})).unwrap();
        assert!((float_result(&result) + 0.577_215_664_901_532_9).abs() <= 1e-12);
    }

    #[test]
    fn beta_reference_value() {
        let result = call("scientific.beta", serde_json::json!({"a": 2, "b": 3})).unwrap();
        assert!((float_result(&result) - 1.0 / 12.0).abs() <= 1e-12);
    }

    #[test]
    fn erf_and_erfc_reference_values() {
        let erf = call("scientific.erf", serde_json::json!({"x": 1})).unwrap();
        assert!((float_result(&erf) - 0.842_700_792_949_714_9).abs() <= 1e-15);

        let erfc = call("scientific.erfc", serde_json::json!({"x": 3})).unwrap();
        let reference = 2.209_049_699_858_544e-5;
        assert!((float_result(&erfc) - reference).abs() <= 1e-14 * reference);
    }

    #[test]
    fn zeta_reference_values() {
        let two = call("scientific.zeta", serde_json::json!({"x": 2})).unwrap();
        assert!((float_result(&two) - std::f64::consts::PI.powi(2) / 6.0).abs() <= 1e-15);

        let four = call("scientific.zeta", serde_json::json!({"x": 4})).unwrap();
        assert!((float_result(&four) - std::f64::consts::PI.powi(4) / 90.0).abs() <= 1e-15);
    }

    #[test]
    fn bessel_reference_values() {
        let j0 = call("scientific.bessel_j0", serde_json::json!({"x": 1})).unwrap();
        assert!((float_result(&j0) - 0.765_197_686_557_966_6).abs() <= 1e-15);

        let j1 = call("scientific.bessel_j1", serde_json::json!({"x": 1})).unwrap();
        assert!((float_result(&j1) - 0.440_050_585_744_933_5).abs() <= 1e-15);
    }

    #[test]
    fn special_function_domains_are_rejected() {
        for (id, raw) in [
            ("scientific.gamma", serde_json::json!({"x": 0})),
            ("scientific.lgamma", serde_json::json!({"x": -1})),
            ("scientific.digamma", serde_json::json!({"x": 0})),
            ("scientific.beta", serde_json::json!({"a": -1, "b": 2})),
            ("scientific.zeta", serde_json::json!({"x": 1})),
        ] {
            assert_eq!(call(id, raw).unwrap_err().code, ErrorCode::DomainViolation);
        }
    }

    // -- Numerical differentiation and ODEs --------------------------------

    #[test]
    fn differentiate_cubic_reference_values() {
        for order in [1, 2] {
            let result = call(
                "scientific.differentiate",
                serde_json::json!({
                    "expression": "x^3",
                    "variable": "x",
                    "at": 2,
                    "order": order
                }),
            )
            .unwrap();
            let fields = record_result(&result);
            assert!(
                (record_f64(fields, "value") - 12.0).abs() <= 1e-8,
                "order {order}: {:?}",
                fields.get("value")
            );
            assert_eq!(
                fields.get("order"),
                Some(&Value::integer(num_bigint::BigInt::from(order)))
            );
            assert_eq!(fields.get("method"), Some(&Value::text("richardson")));
        }
    }

    #[test]
    fn differentiate_sine_at_zero() {
        let result = call(
            "scientific.differentiate",
            serde_json::json!({
                "expression": "sin(x)",
                "variable": "x",
                "at": 0
            }),
        )
        .unwrap();
        assert!((record_f64(record_result(&result), "value") - 1.0).abs() <= 1e-8);
    }

    #[test]
    fn differentiate_rejects_orders_above_four() {
        let error = call(
            "scientific.differentiate",
            serde_json::json!({
                "expression": "x^2",
                "variable": "x",
                "at": 0,
                "order": 5
            }),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::DomainViolation);
        assert!(
            error.message.contains("orders 1 through 4"),
            "{}",
            error.message
        );
    }

    fn ode_call(
        expression: &str,
        initial_value: f64,
        t0: f64,
        t1: f64,
        step: Option<f64>,
    ) -> Result<Outcome, EngineError> {
        let mut raw = serde_json::json!({
            "expression": expression,
            "variable": "y",
            "initial_value": f64_value(initial_value),
            "t0": f64_value(t0),
            "t1": f64_value(t1)
        });
        if let Some(step) = step {
            raw["step"] = f64_value(step);
        }
        call("scientific.ode_rk45", raw)
    }

    #[test]
    fn ode_rk45_solves_exponential_growth() {
        let result = ode_call("y", 1.0, 0.0, 1.0, None).unwrap();
        let fields = record_result(&result);
        assert!((record_f64(fields, "final_y") - std::f64::consts::E).abs() <= 1e-6);
        assert_eq!(
            fields.get("method"),
            Some(&Value::text("dormand_prince_45"))
        );
        assert_eq!(fields.get("converged"), Some(&Value::Bool(true)));
        assert!(fields.get("solution").is_some());
    }

    #[test]
    fn ode_rk45_solves_gaussian_decay() {
        let result = ode_call("-2*t*y", 1.0, 0.0, 1.0, None).unwrap();
        let fields = record_result(&result);
        assert!(
            (record_f64(fields, "final_y") - (-1.0f64).exp()).abs() <= 1e-6,
            "final_y = {:?}",
            fields.get("final_y")
        );
    }

    #[test]
    fn ode_rk45_reports_non_convergence() {
        let raw = serde_json::json!({
            "expression": "y",
            "variable": "y",
            "initial_value": 1,
            "t0": 0,
            "t1": 1,
            "step": 0.1,
            "max_steps": 1
        });
        assert_eq!(
            call("scientific.ode_rk45", raw).unwrap_err().code,
            ErrorCode::NonConvergence
        );
    }

    #[test]
    fn ode_rk45_rejects_the_reserved_independent_variable() {
        let raw = serde_json::json!({
            "expression": "t + y",
            "variable": "t",
            "initial_value": 0,
            "t0": 0,
            "t1": 1
        });
        assert_eq!(
            call("scientific.ode_rk45", raw).unwrap_err().code,
            ErrorCode::DomainViolation
        );
    }

    // -- Contract and documentation tests ----------------------------------

    #[test]
    fn descriptor_contract_is_complete() {
        let module = module();
        assert_eq!(module.descriptor.id, "scientific");
        assert_eq!(module.descriptor.version, "1.0.0");
        assert_eq!(module.functions.len(), 38);
        let mut ids = BTreeSet::new();
        for function in &module.functions {
            let descriptor = function.descriptor();
            assert!(
                descriptor.id.starts_with("scientific."),
                "bad id {}",
                descriptor.id
            );
            assert_eq!(descriptor.module, "scientific");
            assert_eq!(descriptor.version, "1.0.0");
            assert!(
                descriptor
                    .method_ref
                    .starts_with("docs/methods/scientific.md#"),
                "bad method_ref {}",
                descriptor.method_ref
            );
            assert!(
                !descriptor.examples.is_empty(),
                "function {} has no executable example",
                descriptor.id
            );
            assert!(ids.insert(descriptor.id.clone()), "duplicate id");
        }
    }

    #[test]
    fn every_example_executes_and_matches() {
        let module = module();
        let ctx = ExecContext::scientific();
        for function in &module.functions {
            for example in &function.descriptor().examples {
                let mut values = BTreeMap::new();
                for (name, raw) in &example.arguments {
                    let parameter = function
                        .descriptor()
                        .parameter(name)
                        .unwrap_or_else(|| panic!("example parameter {name} is not declared"));
                    parameter
                        .schema
                        .validate(raw, name, &ctx.limits)
                        .unwrap_or_else(|error| {
                            panic!(
                                "example {:?} of {} has an invalid {name}: {error}",
                                example.title,
                                function.descriptor().id
                            )
                        });
                    values.insert(name.clone(), raw.clone());
                }
                let result = function.invoke(&Args::new(values), &ctx);
                match &example.expected {
                    Some(ExampleExpectation::Value(expected)) => {
                        let outcome = result.unwrap_or_else(|error| {
                            panic!(
                                "example {:?} of {} failed: {error}",
                                example.title,
                                function.descriptor().id
                            )
                        });
                        assert_eq!(
                            &outcome.value,
                            expected,
                            "example {:?} of {} produced an unexpected value",
                            example.title,
                            function.descriptor().id
                        );
                    }
                    Some(ExampleExpectation::Error(code)) => {
                        let error = match result {
                            Err(error) => error,
                            Ok(outcome) => panic!(
                                "example {:?} of {} expected {code:?}, produced {:?}",
                                example.title,
                                function.descriptor().id,
                                outcome.value
                            ),
                        };
                        assert_eq!(
                            error.code,
                            *code,
                            "example {:?} of {} produced the wrong error: {}",
                            example.title,
                            function.descriptor().id,
                            error.message
                        );
                    }
                    Some(ExampleExpectation::Contains(text)) => {
                        let outcome = result.unwrap_or_else(|error| {
                            panic!(
                                "example {:?} of {} failed: {error}",
                                example.title,
                                function.descriptor().id
                            )
                        });
                        assert!(
                            format!("{:?}", outcome.value).contains(text),
                            "example {:?} of {} does not contain {text:?}",
                            example.title,
                            function.descriptor().id
                        );
                    }
                    None => {
                        let _ = result;
                    }
                }
            }
        }
    }
}
