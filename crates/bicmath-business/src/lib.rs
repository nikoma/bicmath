//! Business module: unit economics, customer lifetime value, pricing elasticity,
//! inventory, queueing, and cohort analysis.
//!
//! Money-like quantities (prices, costs, ARPU, CAC, cohort revenue) are computed
//! with exact integer/rational/decimal arithmetic and only rounded when a
//! quotient does not terminate as a decimal. Probability and queueing models
//! use binary64 with `libm` fallbacks on wasm32 and report approximate results.

mod cohorts;
mod economics;
mod f64math;
mod operations;
mod util;

use std::sync::Arc;

use bicmath_core::contract::{Function, Module, ModuleDescriptor};
use bicmath_core::number::NumericMode;

/// Build the business module with all of its registered functions.
pub fn module() -> Module {
    let mut functions: Vec<Arc<dyn Function>> = Vec::new();
    functions.extend(economics::register());
    functions.extend(operations::register());
    functions.extend(cohorts::register());
    let descriptor = ModuleDescriptor::new(
        "business",
        "Business",
        "1.0.0",
        "Unit economics, customer lifetime value, pricing elasticity, inventory, queueing, \
         and cohort analysis.",
    )
    .with_capabilities(vec![
        "unit_economics",
        "customer_lifetime_value",
        "price_elasticity",
        "inventory_management",
        "queueing",
        "cohort_analysis",
    ])
    .with_dependencies(vec!["core"])
    .with_modes(vec![
        NumericMode::Exact,
        NumericMode::Auto,
        NumericMode::Scientific,
    ])
    .with_source("crates/bicmath-business");
    Module::new(descriptor, functions)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use num_bigint::BigInt;

    use bicmath_core::context::ExecContext;
    use bicmath_core::contract::{Args, ExampleExpectation, Outcome};
    use bicmath_core::envelope::Exactness;
    use bicmath_core::error::{EngineError, ErrorCode};
    use bicmath_core::number::{Decimal, Number};
    use bicmath_core::value::Value;

    use super::*;

    fn ctx() -> ExecContext {
        ExecContext::conservative()
    }

    fn call(id: &str, raw: serde_json::Value) -> Result<Outcome, EngineError> {
        let module = module();
        let function = module
            .functions
            .iter()
            .find(|f| f.descriptor().id == id)
            .expect("function exists");
        let args_json = raw.as_object().expect("object args");
        let mut values = BTreeMap::new();
        for (name, value) in args_json {
            let param = function
                .descriptor()
                .parameter(name)
                .expect("parameter exists");
            let coerced =
                param
                    .schema
                    .coerce(value, name, &ctx().limits, param.numeric_shorthand)?;
            values.insert(name.clone(), coerced);
        }
        function.invoke(&Args::new(values), &ctx())
    }

    fn record_field<'a>(value: &'a Value, name: &str) -> &'a Value {
        value
            .as_record()
            .expect("record")
            .get(name)
            .unwrap_or_else(|| panic!("missing field {name}"))
    }

    fn decimal_of(value: &Value) -> Decimal {
        match value {
            Value::Number(Number::Decimal(decimal)) => decimal.clone(),
            Value::Number(Number::Integer(integer)) => Decimal::from_bigint(integer.clone()),
            other => panic!("expected a decimal number, found {other:?}"),
        }
    }

    fn f64_of(value: &Value) -> f64 {
        value
            .as_number()
            .expect("number")
            .to_f64()
            .expect("representable as float64")
    }

    fn assert_close(actual: f64, expected: f64, tolerance: f64) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "expected {expected}, found {actual}"
        );
    }

    fn decimal(text: &str) -> Decimal {
        Decimal::parse_default(text).expect("valid decimal fixture")
    }

    // ------------------------------------------------------------------
    // Required hand-checkable tests
    // ------------------------------------------------------------------

    #[test]
    fn unit_economics_break_even_is_known() {
        let outcome = call(
            "business.unit_economics",
            serde_json::json!({
                "price": "10",
                "unit_variable_cost": "6",
                "fixed_costs": "1000",
                "volume": "300"
            }),
        )
        .unwrap();
        assert_eq!(outcome.exactness, Exactness::Exact);
        assert_eq!(
            decimal_of(record_field(&outcome.value, "unit_contribution")),
            decimal("4")
        );
        assert_eq!(
            decimal_of(record_field(&outcome.value, "contribution_margin_ratio")),
            decimal("0.4")
        );
        assert_eq!(
            decimal_of(record_field(&outcome.value, "gross_profit")),
            decimal("200")
        );
        assert_eq!(
            decimal_of(record_field(&outcome.value, "break_even_units")),
            decimal("250")
        );
        assert_eq!(
            decimal_of(record_field(&outcome.value, "break_even_revenue")),
            decimal("2500")
        );
        assert_eq!(
            decimal_of(record_field(&outcome.value, "margin_of_safety_units")),
            decimal("50")
        );
        assert_eq!(
            record_field(&outcome.value, "method"),
            &Value::text("unit_economics")
        );
        let error = call(
            "business.unit_economics",
            serde_json::json!({
                "price": "6",
                "unit_variable_cost": "6",
                "fixed_costs": "1000",
                "volume": "300"
            }),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::DomainViolation);
    }

    #[test]
    fn clv_geometric_matches_closed_form() {
        let finite = call(
            "business.clv_geometric",
            serde_json::json!({
                "revenue_per_period": "100",
                "retention_rate": "0.5",
                "discount_rate": "0",
                "periods": 3
            }),
        )
        .unwrap();
        assert_eq!(
            decimal_of(record_field(&finite.value, "clv")),
            decimal("175")
        );
        assert_eq!(
            record_field(&finite.value, "periods"),
            &Value::integer(BigInt::from(3))
        );

        let infinite = call(
            "business.clv_geometric",
            serde_json::json!({
                "revenue_per_period": "100",
                "retention_rate": "0.5",
                "discount_rate": "0"
            }),
        )
        .unwrap();
        assert_eq!(
            decimal_of(record_field(&infinite.value, "clv")),
            decimal("200")
        );
        assert_eq!(record_field(&infinite.value, "periods"), &Value::Null);

        // Manual closed form: revenue 120, retention 2/3, discount 0.2, 2 periods.
        // q = (2/3) / (6/5) = 5/9; clv = 120 * (1 + 5/9) = 560/3.
        let manual = call(
            "business.clv_geometric",
            serde_json::json!({
                "revenue_per_period": "120",
                "retention_rate": {
                    "kind": "rational",
                    "numerator": "2",
                    "denominator": "3"
                },
                "discount_rate": "0.2",
                "periods": 2
            }),
        )
        .unwrap();
        assert_close(
            f64_of(record_field(&manual.value, "clv")),
            560.0 / 3.0,
            1e-9,
        );

        let error = call(
            "business.clv_geometric",
            serde_json::json!({
                "revenue_per_period": "100",
                "retention_rate": "1",
                "discount_rate": "0"
            }),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::DomainViolation);
    }

    #[test]
    fn arc_elasticity_of_ten_to_twelve_is_negative_eleven_ninths() {
        let outcome = call(
            "business.arc_elasticity",
            serde_json::json!({
                "old_price": "10",
                "new_price": "12",
                "old_quantity": "100",
                "new_quantity": "80"
            }),
        )
        .unwrap();
        assert_close(
            f64_of(record_field(&outcome.value, "elasticity")),
            -11.0 / 9.0,
            1e-12,
        );
        assert_eq!(
            record_field(&outcome.value, "classification"),
            &Value::text("elastic")
        );

        let unit = call(
            "business.arc_elasticity",
            serde_json::json!({
                "old_price": "10",
                "new_price": "20",
                "old_quantity": "100",
                "new_quantity": "50"
            }),
        )
        .unwrap();
        assert_eq!(
            decimal_of(record_field(&unit.value, "elasticity")),
            decimal("-1")
        );
        assert_eq!(
            record_field(&unit.value, "classification"),
            &Value::text("unit")
        );

        let inelastic = call(
            "business.arc_elasticity",
            serde_json::json!({
                "old_price": "10",
                "new_price": "11",
                "old_quantity": "100",
                "new_quantity": "99"
            }),
        )
        .unwrap();
        assert_eq!(
            record_field(&inelastic.value, "classification"),
            &Value::text("inelastic")
        );
    }

    #[test]
    fn eoq_matches_known_value() {
        let outcome = call(
            "business.eoq",
            serde_json::json!({
                "annual_demand": "10000",
                "order_cost": "50",
                "holding_cost_per_unit": "2"
            }),
        )
        .unwrap();
        assert_eq!(outcome.exactness, Exactness::Approximate);
        assert_close(
            f64_of(record_field(&outcome.value, "eoq")),
            707.1067811865476,
            1e-9,
        );
        assert_close(
            f64_of(record_field(&outcome.value, "orders_per_year")),
            14.142135623730951,
            1e-9,
        );
        assert_close(
            f64_of(record_field(&outcome.value, "cycle_time_periods")),
            0.07071067811865476,
            1e-9,
        );
        let ordering = f64_of(record_field(&outcome.value, "total_ordering_cost"));
        let holding = f64_of(record_field(&outcome.value, "total_holding_cost"));
        assert_close(ordering, 707.1067811865476, 1e-6);
        assert_close(holding, 707.1067811865476, 1e-6);
        assert_close(
            f64_of(record_field(&outcome.value, "total_cost")),
            ordering + holding,
            1e-9,
        );
        let error = call(
            "business.eoq",
            serde_json::json!({
                "annual_demand": "0",
                "order_cost": "50",
                "holding_cost_per_unit": "2"
            }),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::DomainViolation);
    }

    #[test]
    fn mm1_matches_known_values() {
        let outcome = call(
            "business.queue_mm1",
            serde_json::json!({"arrival_rate": "4", "service_rate": "5"}),
        )
        .unwrap();
        assert_eq!(outcome.exactness, Exactness::Approximate);
        assert_close(
            f64_of(record_field(&outcome.value, "utilization")),
            0.8,
            1e-12,
        );
        assert_close(f64_of(record_field(&outcome.value, "p0")), 0.2, 1e-12);
        assert_close(f64_of(record_field(&outcome.value, "l")), 4.0, 1e-12);
        assert_close(f64_of(record_field(&outcome.value, "lq")), 3.2, 1e-12);
        assert_close(f64_of(record_field(&outcome.value, "w")), 1.0, 1e-12);
        assert_close(f64_of(record_field(&outcome.value, "wq")), 0.8, 1e-12);
        assert_close(f64_of(record_field(&outcome.value, "p_wait")), 0.8, 1e-12);
        assert_eq!(record_field(&outcome.value, "method"), &Value::text("mm1"));
        let error = call(
            "business.queue_mm1",
            serde_json::json!({"arrival_rate": "5", "service_rate": "5"}),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::DomainViolation);
    }

    #[test]
    fn erlang_c_matches_known_value() {
        let outcome = call(
            "business.erlang_c",
            serde_json::json!({"staff": 2, "arrival_rate": "3", "service_rate": "2"}),
        )
        .unwrap();
        assert_eq!(outcome.exactness, Exactness::Approximate);
        assert_close(
            f64_of(record_field(&outcome.value, "p_wait")),
            0.6428571428571428,
            1e-12,
        );
        assert_close(
            f64_of(record_field(&outcome.value, "utilization")),
            0.75,
            1e-12,
        );
        assert_close(
            f64_of(record_field(&outcome.value, "average_waiting_time")),
            0.6428571428571428,
            1e-12,
        );
        assert_close(
            f64_of(record_field(&outcome.value, "average_queue_length")),
            1.9285714285714284,
            1e-12,
        );
        assert_eq!(
            record_field(&outcome.value, "method"),
            &Value::text("erlang_c")
        );
        let error = call(
            "business.erlang_c",
            serde_json::json!({"staff": 2, "arrival_rate": "4", "service_rate": "2"}),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::DomainViolation);
    }

    #[test]
    fn retention_rates_between_cohorts() {
        let outcome = call(
            "business.retention_rates",
            serde_json::json!({"cohort_sizes": [1000, 800, 600, 300]}),
        )
        .unwrap();
        let rates = record_field(&outcome.value, "rates")
            .as_array()
            .expect("rates array");
        assert_eq!(rates.len(), 3);
        assert_eq!(decimal_of(&rates[0]), decimal("0.8"));
        assert_eq!(decimal_of(&rates[1]), decimal("0.75"));
        assert_eq!(decimal_of(&rates[2]), decimal("0.5"));
        assert_eq!(
            record_field(&outcome.value, "method"),
            &Value::text("retention_rates")
        );
        let error = call(
            "business.retention_rates",
            serde_json::json!({"cohort_sizes": [1000]}),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::InsufficientObservations);
    }

    #[test]
    fn cohort_revenue_projection() {
        let outcome = call(
            "business.cohort_revenue",
            serde_json::json!({
                "cohort_sizes": [1000, 800, 600],
                "revenue_per_user": "2.5"
            }),
        )
        .unwrap();
        assert_eq!(outcome.exactness, Exactness::Exact);
        let per_cohort = record_field(&outcome.value, "revenue_per_cohort")
            .as_array()
            .expect("revenue array");
        assert_eq!(per_cohort.len(), 3);
        assert_eq!(decimal_of(&per_cohort[0]), decimal("2500"));
        assert_eq!(decimal_of(&per_cohort[1]), decimal("2000"));
        assert_eq!(decimal_of(&per_cohort[2]), decimal("1500"));
        assert_eq!(
            decimal_of(record_field(&outcome.value, "total_revenue")),
            decimal("6000")
        );
        assert_eq!(
            record_field(&outcome.value, "method"),
            &Value::text("cohort_revenue")
        );
    }

    // ------------------------------------------------------------------
    // Contract tests
    // ------------------------------------------------------------------

    #[test]
    fn every_function_declares_identity_examples_and_method_ref() {
        let module = module();
        assert_eq!(module.descriptor.id, "business");
        assert_eq!(module.descriptor.version, "1.0.0");
        assert_eq!(module.functions.len(), 9);
        for function in module.functions {
            let descriptor = function.descriptor();
            assert!(
                descriptor.id.starts_with("business."),
                "unexpected id {}",
                descriptor.id
            );
            assert_eq!(descriptor.module, "business");
            assert_eq!(descriptor.version, "1.0.0");
            assert!(
                !descriptor.examples.is_empty(),
                "function {} has no examples",
                descriptor.id
            );
            assert!(
                descriptor
                    .method_ref
                    .starts_with("docs/methods/business.md#"),
                "function {} has no method ref",
                descriptor.id
            );
            assert!(
                !descriptor.parameters.is_empty(),
                "function {} has no parameters",
                descriptor.id
            );
            assert!(
                !descriptor.output_description.is_empty(),
                "function {} has no output description",
                descriptor.id
            );
        }
    }

    #[test]
    fn examples_match_their_expectations() {
        for function in module().functions {
            for example in &function.descriptor().examples {
                let mut values = BTreeMap::new();
                for (name, raw) in &example.arguments {
                    let param = function.descriptor().parameter(name).unwrap_or_else(|| {
                        panic!("{}: unknown parameter {name}", function.descriptor().id)
                    });
                    let value = param
                        .schema
                        .coerce(
                            &serde_json::to_value(raw).unwrap(),
                            name,
                            &ctx().limits,
                            param.numeric_shorthand,
                        )
                        .unwrap_or_else(|error| {
                            panic!(
                                "{}: {name} did not coerce: {error}",
                                function.descriptor().id
                            )
                        });
                    values.insert(name.clone(), value);
                }
                let outcome = function.invoke(&Args::new(values), &ctx());
                let label = format!("{}: {}", function.descriptor().id, example.title);
                match &example.expected {
                    Some(ExampleExpectation::Value(expected)) => {
                        let outcome = outcome.unwrap_or_else(|error| panic!("{label}: {error}"));
                        assert_eq!(&outcome.value, expected, "{label}");
                    }
                    Some(ExampleExpectation::Error(code)) => match outcome {
                        Ok(outcome) => {
                            panic!("{label}: expected error {code:?}, got {:?}", outcome.value)
                        }
                        Err(error) => assert_eq!(error.code, *code, "{label}"),
                    },
                    Some(ExampleExpectation::Contains(text)) => {
                        let outcome = outcome.unwrap_or_else(|error| panic!("{label}: {error}"));
                        let encoded = serde_json::to_string(&outcome.value).unwrap();
                        assert!(encoded.contains(text), "{label}: {encoded}");
                    }
                    None => {}
                }
            }
        }
    }
}
