//! Finance module: money operations, interest, cash flows, amortization, and
//! contribution analysis.
//!
//! Every function is pure and deterministic, rejects float64 money amounts, and
//! never combines different currencies without an explicit conversion. Rate
//! solving may use binary64 internally, but every result is converted back to an
//! explicit decimal and reported as rounded.

mod amortization;
mod bonds;
mod capm;
mod cashflow;
mod interest;
mod mathfn;
mod money;
mod options;
mod portfolio;
mod rates;
mod util;

use std::sync::Arc;

use bicmath_core::contract::{Function, Module, ModuleDescriptor};
use bicmath_core::number::NumericMode;

/// Build the finance module with all of its registered functions.
pub fn module() -> Module {
    let mut functions: Vec<Arc<dyn Function>> = Vec::new();
    functions.extend(money::register());
    functions.extend(rates::register());
    functions.extend(interest::register());
    functions.extend(amortization::register());
    functions.extend(cashflow::register());
    functions.extend(options::register());
    functions.extend(bonds::register());
    functions.extend(portfolio::register());
    functions.extend(capm::register());
    let descriptor = ModuleDescriptor::new(
        "finance",
        "Finance",
        "1.0.0",
        "Money operations, cash flows, interest, amortization, options, bonds, portfolio risk, and contribution analysis.",
    )
    .with_capabilities(vec![
        "money",
        "currency_safety",
        "cash_flows",
        "interest",
        "amortization",
        "rate_solving",
        "day_count",
        "options",
        "fixed_income",
        "portfolio_risk",
        "capm",
    ])
    .with_dependencies(vec!["core"])
    .with_modes(vec![
        NumericMode::Exact,
        NumericMode::Auto,
        NumericMode::Scientific,
    ])
    .with_source("crates/bicmath-finance");
    Module::new(descriptor, functions)
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;
    use std::collections::BTreeMap;

    use chrono::NaiveDate;
    use num_bigint::BigInt;
    use num_rational::BigRational;

    use bicmath_core::contract::{Args, ExampleExpectation};
    use bicmath_core::envelope::Exactness;
    use bicmath_core::error::ErrorCode;
    use bicmath_core::number::{Decimal, Number};
    use bicmath_core::value::Value;

    use super::*;
    use crate::cashflow::{DayCount, year_fraction};

    fn ctx() -> bicmath_core::context::ExecContext {
        bicmath_core::context::ExecContext::conservative()
    }

    fn call(
        id: &str,
        raw: serde_json::Value,
    ) -> Result<bicmath_core::contract::Outcome, bicmath_core::error::EngineError> {
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
            values.insert(
                name.clone(),
                param
                    .schema
                    .coerce(value, name, &ctx().limits, true)
                    .expect("argument coerces"),
            );
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
            .expect("representable")
    }

    fn money_amount(value: &Value) -> Decimal {
        let (amount, _) = value.as_money().expect("money");
        decimal_of(&Value::Number(amount.clone()))
    }

    fn add_decimals(left: &Decimal, right: &Decimal) -> Decimal {
        left.add(right, &ctx().numeric, &ctx().limits)
            .expect("decimal addition")
    }

    // ------------------------------------------------------------------
    // Required hand-checkable tests
    // ------------------------------------------------------------------

    #[test]
    fn npv_first_cashflow_at_t0_and_t1_differ() {
        // Fixtures computed with Python 3 decimal at 50 digits:
        //   npv_t0 = 308.3879489915155210079764263577706650408982370573
        //   npv_t1 = 285.5443972143662231555337281090469120749057750530
        let t0 = call(
            "finance.npv",
            serde_json::json!({"rate": "0.08", "cashflows": [-10000, 4000, 4000, 4000]}),
        )
        .unwrap();
        let t1 = call(
            "finance.npv",
            serde_json::json!({
                "rate": "0.08",
                "cashflows": [-10000, 4000, 4000, 4000],
                "timing": "first_cashflow_at_t1"
            }),
        )
        .unwrap();
        let npv_t0 = f64_of(record_field(&t0.value, "npv"));
        let npv_t1 = f64_of(record_field(&t1.value, "npv"));
        assert!(
            (npv_t0 - 308.3879489915155).abs() < 1e-6,
            "t0 npv was {npv_t0}"
        );
        assert!(
            (npv_t1 - 285.5443972143662).abs() < 1e-6,
            "t1 npv was {npv_t1}"
        );
        assert!((npv_t0 - npv_t1).abs() > 1.0);
    }

    #[test]
    fn irr_matches_high_precision_fixture() {
        // Fixture computed with Python 3 decimal bisection at 60 digits:
        //   irr = 0.23375192852825878819094337767939303519...
        let outcome = call(
            "finance.irr",
            serde_json::json!({"cashflows": [-1000, 500, 500, 500]}),
        )
        .unwrap();
        let rate = f64_of(record_field(&outcome.value, "rate"));
        assert!(
            (rate - 0.233_751_928_528_258_8).abs() < 1e-9,
            "irr was {rate}"
        );
    }

    #[test]
    fn xnpv_actual_365_uses_leap_year_days() {
        let start = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let end = NaiveDate::from_ymd_opt(2025, 1, 1).unwrap();
        assert_eq!(
            year_fraction(DayCount::Actual365, start, end),
            BigRational::new(BigInt::from(366), BigInt::from(365)),
            "2024 is a leap year, so the actual_365 year fraction is exactly 366/365"
        );
        assert_eq!(
            year_fraction(DayCount::Actual360, start, end),
            BigRational::new(BigInt::from(366), BigInt::from(360))
        );
        assert_eq!(
            year_fraction(DayCount::ActualActual, start, end),
            BigRational::from_integer(BigInt::from(1)),
            "a full leap year is exactly one actual_actual year"
        );
        assert_eq!(
            year_fraction(DayCount::Thirty360, start, end),
            BigRational::from_integer(BigInt::from(1))
        );
        // Python fixture: actual_actual(2023-07-01, 2024-07-01) = 66887/66795.
        assert_eq!(
            year_fraction(
                DayCount::ActualActual,
                NaiveDate::from_ymd_opt(2023, 7, 1).unwrap(),
                NaiveDate::from_ymd_opt(2024, 7, 1).unwrap()
            ),
            BigRational::new(BigInt::from(66887), BigInt::from(66795))
        );
    }

    #[test]
    fn amortization_zero_rate_pays_one_hundred_each() {
        let outcome = call(
            "finance.amortization",
            serde_json::json!({
                "principal": {
                    "kind": "money",
                    "amount": {"kind": "decimal", "value": "1200.00"},
                    "currency": "USD"
                },
                "annual_rate": "0",
                "periods": 12,
                "currency": "USD"
            }),
        )
        .unwrap();
        let schedule = record_field(&outcome.value, "schedule")
            .as_array()
            .unwrap()
            .to_vec();
        assert_eq!(schedule.len(), 12);
        let mut principal_sum = Decimal::zero();
        for row in &schedule {
            assert_eq!(
                money_amount(record_field(row, "payment")),
                Decimal::parse_default("100.00").unwrap()
            );
            assert!(money_amount(record_field(row, "interest")).is_zero());
            principal_sum = add_decimals(
                &principal_sum,
                &money_amount(record_field(row, "principal")),
            );
        }
        assert_eq!(
            principal_sum.numeric_cmp(&Decimal::parse_default("1200.00").unwrap()),
            Ordering::Equal
        );
        assert!(money_amount(record_field(schedule.last().unwrap(), "balance")).is_zero());
        assert_eq!(
            record_field(record_field(&outcome.value, "reconciliation"), "reconciles"),
            &Value::Bool(true)
        );
    }

    #[test]
    fn amortization_six_percent_over_twelve_reconciles() {
        // 6% nominal annual compounded monthly: per-period rate 0.005.
        // Python fixture for the per-period schedule:
        //   payment = 103.28 (rounded from 103.2797...), final payment = 103.24
        //   total principal = 1200.00 exactly, closing balance 0.
        let outcome = call(
            "finance.amortization",
            serde_json::json!({
                "principal": {
                    "kind": "money",
                    "amount": {"kind": "decimal", "value": "1200.00"},
                    "currency": "USD"
                },
                "annual_rate": "0.005",
                "periods": 12,
                "currency": "USD"
            }),
        )
        .unwrap();
        let schedule = record_field(&outcome.value, "schedule")
            .as_array()
            .unwrap()
            .to_vec();
        assert_eq!(schedule.len(), 12);
        assert_eq!(
            money_amount(record_field(&schedule[0], "payment")),
            Decimal::parse_default("103.28").unwrap()
        );
        assert_eq!(
            money_amount(record_field(&schedule[0], "interest")),
            Decimal::parse_default("6.00").unwrap()
        );
        assert_eq!(
            money_amount(record_field(&schedule[11], "payment")),
            Decimal::parse_default("103.24").unwrap()
        );
        let mut principal_sum = Decimal::zero();
        for row in &schedule {
            principal_sum = add_decimals(
                &principal_sum,
                &money_amount(record_field(row, "principal")),
            );
        }
        assert_eq!(
            principal_sum.numeric_cmp(&Decimal::parse_default("1200.00").unwrap()),
            Ordering::Equal,
            "sum of principal column must equal the initial principal exactly"
        );
        assert!(money_amount(record_field(&schedule[11], "balance")).is_zero());
        assert_eq!(
            record_field(record_field(&outcome.value, "reconciliation"), "reconciles"),
            &Value::Bool(true)
        );
    }

    #[test]
    fn amortization_final_only_rounding_also_reconciles() {
        let outcome = call(
            "finance.amortization",
            serde_json::json!({
                "principal": {
                    "kind": "money",
                    "amount": {"kind": "decimal", "value": "1200.00"},
                    "currency": "USD"
                },
                "annual_rate": "0.005",
                "periods": 12,
                "currency": "USD",
                "rounding": "final_only"
            }),
        )
        .unwrap();
        let schedule = record_field(&outcome.value, "schedule")
            .as_array()
            .unwrap()
            .to_vec();
        let mut principal_sum = Decimal::zero();
        for row in &schedule {
            principal_sum = add_decimals(
                &principal_sum,
                &money_amount(record_field(row, "principal")),
            );
        }
        assert_eq!(
            principal_sum.numeric_cmp(&Decimal::parse_default("1200.00").unwrap()),
            Ordering::Equal
        );
        assert!(money_amount(record_field(&schedule[11], "balance")).is_zero());
        assert_eq!(
            record_field(record_field(&outcome.value, "reconciliation"), "reconciles"),
            &Value::Bool(true)
        );
    }

    #[test]
    fn money_allocate_sums_exactly() {
        let cent = call(
            "finance.money_allocate",
            serde_json::json!({
                "money": {
                    "kind": "money",
                    "amount": {"kind": "decimal", "value": "0.01"},
                    "currency": "USD"
                },
                "weights": [1, 1, 1]
            }),
        )
        .unwrap();
        let shares = record_field(&cent.value, "shares").as_array().unwrap();
        let mut sum = Decimal::zero();
        for share in shares {
            sum = add_decimals(&sum, &money_amount(share));
        }
        assert_eq!(
            sum.numeric_cmp(&Decimal::parse_default("0.01").unwrap()),
            Ordering::Equal
        );
        assert_eq!(
            money_amount(&shares[0]),
            Decimal::parse_default("0.01").unwrap()
        );
        assert!(money_amount(&shares[1]).is_zero());
        assert!(money_amount(&shares[2]).is_zero());

        let ten = call(
            "finance.money_allocate",
            serde_json::json!({
                "money": {
                    "kind": "money",
                    "amount": {"kind": "decimal", "value": "10.00"},
                    "currency": "USD"
                },
                "weights": [1, 1, 1]
            }),
        )
        .unwrap();
        let shares = record_field(&ten.value, "shares").as_array().unwrap();
        assert_eq!(
            money_amount(&shares[0]),
            Decimal::parse_default("3.34").unwrap()
        );
        assert_eq!(
            money_amount(&shares[1]),
            Decimal::parse_default("3.33").unwrap()
        );
        assert_eq!(
            money_amount(&shares[2]),
            Decimal::parse_default("3.33").unwrap()
        );
        let mut sum = Decimal::zero();
        for share in shares {
            sum = add_decimals(&sum, &money_amount(share));
        }
        assert_eq!(
            sum.numeric_cmp(&Decimal::parse_default("10.00").unwrap()),
            Ordering::Equal
        );

        // Determinism: the same inputs produce byte-for-byte identical values.
        let again = call(
            "finance.money_allocate",
            serde_json::json!({
                "money": {
                    "kind": "money",
                    "amount": {"kind": "decimal", "value": "10.00"},
                    "currency": "USD"
                },
                "weights": [1, 1, 1]
            }),
        )
        .unwrap();
        assert_eq!(ten.value, again.value);
    }

    #[test]
    fn money_add_rejects_currency_mismatch() {
        let error = call(
            "finance.money_add",
            serde_json::json!({
                "a": {
                    "kind": "money",
                    "amount": {"kind": "decimal", "value": "1.00"},
                    "currency": "USD"
                },
                "b": {
                    "kind": "money",
                    "amount": {"kind": "decimal", "value": "1.00"},
                    "currency": "INR"
                }
            }),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::CurrencyMismatch);
    }

    #[test]
    fn break_even_is_250_units() {
        let outcome = call(
            "finance.break_even",
            serde_json::json!({
                "fixed_costs": {
                    "kind": "money",
                    "amount": {"kind": "decimal", "value": "1000.00"},
                    "currency": "USD"
                },
                "unit_price": {
                    "kind": "money",
                    "amount": {"kind": "decimal", "value": "10.00"},
                    "currency": "USD"
                },
                "unit_variable_cost": {
                    "kind": "money",
                    "amount": {"kind": "decimal", "value": "6.00"},
                    "currency": "USD"
                }
            }),
        )
        .unwrap();
        assert_eq!(
            record_field(&outcome.value, "break_even_units"),
            &Value::integer(BigInt::from(250))
        );
        assert_eq!(
            money_amount(record_field(&outcome.value, "unit_contribution")),
            Decimal::parse_default("4.00").unwrap()
        );
        assert_eq!(
            money_amount(record_field(&outcome.value, "break_even_revenue")),
            Decimal::parse_default("2500.00").unwrap()
        );
    }

    // ------------------------------------------------------------------
    // Additional semantics tests
    // ------------------------------------------------------------------

    #[test]
    fn float64_money_amounts_are_rejected() {
        let error = call(
            "finance.money_add",
            serde_json::json!({
                "a": {
                    "kind": "money",
                    "amount": {"kind": "float64", "value": "1.0"},
                    "currency": "USD"
                },
                "b": {
                    "kind": "money",
                    "amount": {"kind": "decimal", "value": "1.00"},
                    "currency": "USD"
                }
            }),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::MalformedInput);
        assert!(error.message.contains("float64"));
    }

    #[test]
    fn convert_money_records_direction_and_as_of() {
        let outcome = call(
            "finance.convert_money",
            serde_json::json!({
                "money": {
                    "kind": "money",
                    "amount": {"kind": "decimal", "value": "100.00"},
                    "currency": "USD"
                },
                "rate": "0.90",
                "target_currency": "EUR",
                "rate_as_of": "2024-06-30"
            }),
        )
        .unwrap();
        assert_eq!(outcome.exactness, Exactness::Rounded);
        assert_eq!(
            money_amount(&outcome.value),
            Decimal::parse_default("90.00").unwrap()
        );
        assert_eq!(
            outcome.warnings[0]
                .details
                .as_ref()
                .unwrap()
                .as_record()
                .unwrap()["rate_direction"],
            Value::text("target_per_source")
        );
        assert_eq!(
            outcome.warnings[0]
                .details
                .as_ref()
                .unwrap()
                .as_record()
                .unwrap()["rate_as_of"],
            Value::text("2024-06-30")
        );
    }

    #[test]
    fn percentage_change_is_exact_when_it_terminates() {
        let outcome = call(
            "finance.percentage_change",
            serde_json::json!({"old_value": 100, "new_value": 125}),
        )
        .unwrap();
        assert_eq!(outcome.exactness, Exactness::Exact);
        assert_eq!(
            decimal_of(&outcome.value),
            Decimal::parse_default("0.25").unwrap()
        );
    }

    #[test]
    fn markup_and_margin_are_inverses() {
        let markup = call("finance.markup", serde_json::json!({"margin": "0.2"})).unwrap();
        let margin = call("finance.margin", serde_json::json!({"markup": "0.25"})).unwrap();
        assert_eq!(
            decimal_of(&markup.value),
            Decimal::parse_default("0.25").unwrap()
        );
        assert_eq!(
            decimal_of(&margin.value),
            Decimal::parse_default("0.2").unwrap()
        );
        assert_eq!(
            call("finance.markup", serde_json::json!({"margin": "1"}))
                .unwrap_err()
                .code,
            ErrorCode::DomainViolation
        );
        assert_eq!(
            call("finance.margin", serde_json::json!({"markup": "-1"}))
                .unwrap_err()
                .code,
            ErrorCode::DomainViolation
        );
    }

    #[test]
    fn xnpv_zero_rate_matches_sum_of_amounts() {
        let outcome = call(
            "finance.xnpv",
            serde_json::json!({
                "rate": "0",
                "cashflows": [
                    {"date": "2024-01-01", "amount": 100},
                    {"date": "2025-01-01", "amount": 100}
                ]
            }),
        )
        .unwrap();
        assert_eq!(f64_of(record_field(&outcome.value, "npv")), 200.0);
        assert_eq!(
            record_field(&outcome.value, "day_count"),
            &Value::text("actual_365")
        );
    }

    #[test]
    fn xirr_requires_a_sign_change() {
        let error = call(
            "finance.xirr",
            serde_json::json!({
                "cashflows": [
                    {"date": "2024-01-01", "amount": 100},
                    {"date": "2025-01-01", "amount": 200}
                ]
            }),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::NonConvergence);
    }

    #[test]
    fn irr_detects_multiple_roots() {
        // -100 + 230/(1+r) - 132/(1+r)^2 has roots at exactly 10% and 20%.
        let outcome = call(
            "finance.irr",
            serde_json::json!({"cashflows": [-100, 230, -132]}),
        )
        .unwrap();
        assert_eq!(
            record_field(&outcome.value, "multiple_roots_suspected"),
            &Value::Bool(true)
        );
        assert_eq!(
            record_field(&outcome.value, "roots_found"),
            &Value::integer(BigInt::from(2))
        );
    }

    #[test]
    fn irr_with_supplied_bracket_converges() {
        let outcome = call(
            "finance.irr",
            serde_json::json!({
                "cashflows": [-1000, 500, 500, 500],
                "lower": "0.0",
                "upper": "1.0"
            }),
        )
        .unwrap();
        let rate = f64_of(record_field(&outcome.value, "rate"));
        assert!((rate - 0.233_751_928_528_258_8).abs() < 1e-9);
        assert_eq!(
            record_field(&outcome.value, "roots_found"),
            &Value::integer(BigInt::from(1))
        );
    }

    // ------------------------------------------------------------------
    // Options, bonds, portfolio, and CAPM reference tests
    // ------------------------------------------------------------------

    #[test]
    fn black_scholes_matches_python_reference() {
        // Fixtures computed independently with Python 3.11 (math.erf):
        //   call S=100 K=100 r=0.05 sigma=0.2 T=1:
        //     price 10.450583572185565, delta 0.6368306511756191,
        //     gamma 0.018762017345846895, vega 37.52403469169379,
        //     theta -6.414027546438197, rho 53.232481545376345,
        //     d1 0.35000000000000003, d2 0.15000000000000002
        //   put price 5.573526022256971 (put-call parity).
        let call_outcome = call(
            "finance.black_scholes",
            serde_json::json!({
                "option_type": "call",
                "spot": 100,
                "strike": 100,
                "rate": "0.05",
                "volatility": "0.2",
                "time": 1
            }),
        )
        .unwrap();
        let price = f64_of(record_field(&call_outcome.value, "price"));
        assert!(
            (price - 10.450_583_572_185_565).abs() < 1e-10,
            "price {price}"
        );
        let delta = f64_of(record_field(&call_outcome.value, "delta"));
        assert!(
            (delta - 0.636_830_651_175_619_1).abs() < 1e-10,
            "delta {delta}"
        );
        let gamma = f64_of(record_field(&call_outcome.value, "gamma"));
        assert!(
            (gamma - 0.018_762_017_345_846_895).abs() < 1e-12,
            "gamma {gamma}"
        );
        let vega = f64_of(record_field(&call_outcome.value, "vega"));
        assert!((vega - 37.524_034_691_693_79).abs() < 1e-9, "vega {vega}");
        let theta = f64_of(record_field(&call_outcome.value, "theta"));
        assert!(
            (theta - -6.414_027_546_438_197).abs() < 1e-9,
            "theta {theta}"
        );
        let rho = f64_of(record_field(&call_outcome.value, "rho"));
        assert!((rho - 53.232_481_545_376_345).abs() < 1e-9, "rho {rho}");
        let d1 = f64_of(record_field(&call_outcome.value, "d1"));
        assert!((d1 - 0.35).abs() < 1e-12, "d1 {d1}");

        let put_outcome = call(
            "finance.black_scholes",
            serde_json::json!({
                "option_type": "put",
                "spot": 100,
                "strike": 100,
                "rate": "0.05",
                "volatility": "0.2",
                "time": 1
            }),
        )
        .unwrap();
        let put_price = f64_of(record_field(&put_outcome.value, "price"));
        assert!(
            (put_price - 5.573_526_022_256_971).abs() < 1e-10,
            "put price {put_price}"
        );
        // Put-call parity: C - P = S - K exp(-rT).
        let parity = price - put_price;
        let expected = 100.0 - 100.0 * (-0.05f64).exp();
        assert!((parity - expected).abs() < 1e-9, "parity {parity}");
    }

    #[test]
    fn black_scholes_zero_volatility_is_discounted_intrinsic() {
        let outcome = call(
            "finance.black_scholes",
            serde_json::json!({
                "option_type": "call",
                "spot": 100,
                "strike": 90,
                "rate": "0",
                "volatility": "0",
                "time": 1
            }),
        )
        .unwrap();
        assert_eq!(f64_of(record_field(&outcome.value, "price")), 10.0);
        assert_eq!(f64_of(record_field(&outcome.value, "delta")), 1.0);
        assert_eq!(record_field(&outcome.value, "d1"), &Value::Null);
        assert_eq!(record_field(&outcome.value, "d2"), &Value::Null);
    }

    #[test]
    fn binomial_500_steps_tracks_black_scholes() {
        // Python 3 CRR fixture: 500-step European call = 10.44658513644654,
        // within 0.02 of the Black-Scholes value 10.450583572185565.
        let outcome = call(
            "finance.binomial_option",
            serde_json::json!({
                "option_type": "call",
                "spot": 100,
                "strike": 100,
                "rate": "0.05",
                "volatility": "0.2",
                "time": 1,
                "steps": 500
            }),
        )
        .unwrap();
        let price = f64_of(record_field(&outcome.value, "price"));
        assert!(
            (price - 10.450_583_572_185_565).abs() < 0.02,
            "binomial price {price}"
        );
        assert_eq!(
            record_field(&outcome.value, "steps"),
            &Value::integer(BigInt::from(500))
        );
        assert_eq!(
            record_field(&outcome.value, "american"),
            &Value::Bool(false)
        );
    }

    #[test]
    fn bond_price_matches_python_fixture() {
        // Python 3.11 decimal fixture (50 digits):
        //   price = 9031875/8788 = 1027.750910332271279016841147018661...
        //   total coupons = 150.00.
        let outcome = call(
            "finance.bond_price",
            serde_json::json!({
                "face_value": {
                    "kind": "money",
                    "amount": {"kind": "decimal", "value": "1000.00"},
                    "currency": "USD"
                },
                "coupon_rate": "0.05",
                "periods": 3,
                "yield_rate": "0.04"
            }),
        )
        .unwrap();
        let price = money_amount(record_field(&outcome.value, "price"));
        let price = price.to_f64().unwrap();
        assert!(
            (price - 1_027.750_910_332_271_3).abs() < 1e-9,
            "bond price {price}"
        );
        assert_eq!(
            money_amount(record_field(&outcome.value, "total_coupons")),
            Decimal::parse_default("150.00").unwrap()
        );

        let zero = call(
            "finance.bond_price",
            serde_json::json!({
                "face_value": {
                    "kind": "money",
                    "amount": {"kind": "decimal", "value": "1000.00"},
                    "currency": "USD"
                },
                "coupon_rate": "0.05",
                "periods": 3,
                "yield_rate": "0"
            }),
        )
        .unwrap();
        assert_eq!(
            money_amount(record_field(&zero.value, "price")),
            Decimal::parse_default("1150.00").unwrap()
        );
    }

    #[test]
    fn bond_yield_recovers_the_python_fixture() {
        let outcome = call(
            "finance.bond_yield",
            serde_json::json!({
                "face_value": {
                    "kind": "money",
                    "amount": {"kind": "decimal", "value": "1000.00"},
                    "currency": "USD"
                },
                "coupon_rate": "0.05",
                "periods": 3,
                "price": {
                    "kind": "money",
                    "amount": {"kind": "decimal", "value": "1027.750910332271279016841147018662"},
                    "currency": "USD"
                }
            }),
        )
        .unwrap();
        let solved = f64_of(record_field(&outcome.value, "yield"));
        assert!((solved - 0.04).abs() < 1e-9, "yield {solved}");
        assert_eq!(
            record_field(&outcome.value, "converged"),
            &Value::Bool(true)
        );
        assert_eq!(
            record_field(&outcome.value, "method"),
            &Value::text("bond_yield_bisection_brent")
        );

        let par = call(
            "finance.bond_yield",
            serde_json::json!({
                "face_value": {
                    "kind": "money",
                    "amount": {"kind": "decimal", "value": "1000.00"},
                    "currency": "USD"
                },
                "coupon_rate": "0.05",
                "periods": 3,
                "price": {
                    "kind": "money",
                    "amount": {"kind": "decimal", "value": "1000.00"},
                    "currency": "USD"
                }
            }),
        )
        .unwrap();
        let par_yield = f64_of(record_field(&par.value, "yield"));
        assert!((par_yield - 0.05).abs() < 1e-9, "par yield {par_yield}");
    }

    #[test]
    fn bond_yield_reports_non_convergence() {
        let error = call(
            "finance.bond_yield",
            serde_json::json!({
                "face_value": {
                    "kind": "money",
                    "amount": {"kind": "decimal", "value": "1000.00"},
                    "currency": "USD"
                },
                "coupon_rate": "0.05",
                "periods": 3,
                "price": {
                    "kind": "money",
                    "amount": {"kind": "decimal", "value": "0"},
                    "currency": "USD"
                }
            }),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::NonConvergence);
    }

    #[test]
    fn bond_duration_matches_python_fixture() {
        // Python 3.11 decimal fixtures for face 1000, coupon 5%, 3 periods,
        // yield 4%, frequency 1:
        //   macaulay 2.861462874541554217701197148986229
        //   modified 2.751406610136109824712689566332912
        //   convexity 10.41266159996298448255459481725431
        let outcome = call(
            "finance.bond_duration",
            serde_json::json!({
                "face_value": {
                    "kind": "money",
                    "amount": {"kind": "decimal", "value": "1000.00"},
                    "currency": "USD"
                },
                "coupon_rate": "0.05",
                "periods": 3,
                "yield_rate": "0.04"
            }),
        )
        .unwrap();
        let macaulay = decimal_of(record_field(&outcome.value, "macaulay_duration"))
            .to_f64()
            .unwrap();
        assert!(
            (macaulay - 2.861_462_874_541_554).abs() < 1e-9,
            "macaulay {macaulay}"
        );
        let modified = decimal_of(record_field(&outcome.value, "modified_duration"))
            .to_f64()
            .unwrap();
        assert!(
            (modified - 2.751_406_610_136_11).abs() < 1e-9,
            "modified {modified}"
        );
        let convexity = decimal_of(record_field(&outcome.value, "convexity"))
            .to_f64()
            .unwrap();
        assert!(
            (convexity - 10.412_661_599_962_984).abs() < 1e-9,
            "convexity {convexity}"
        );

        let zero = call(
            "finance.bond_duration",
            serde_json::json!({
                "face_value": {
                    "kind": "money",
                    "amount": {"kind": "decimal", "value": "1000.00"},
                    "currency": "USD"
                },
                "coupon_rate": "0",
                "periods": 2,
                "yield_rate": "0"
            }),
        )
        .unwrap();
        assert_eq!(
            decimal_of(record_field(&zero.value, "macaulay_duration")),
            Decimal::parse_default("2").unwrap()
        );
        assert_eq!(
            decimal_of(record_field(&zero.value, "modified_duration")),
            Decimal::parse_default("2").unwrap()
        );
        assert_eq!(
            decimal_of(record_field(&zero.value, "convexity")),
            Decimal::parse_default("6").unwrap()
        );
    }

    #[test]
    fn portfolio_return_and_volatility_reference() {
        let outcome = call(
            "finance.portfolio_return",
            serde_json::json!({
                "weights": ["0.5", "0.5"],
                "returns": ["0.1", "0.2"]
            }),
        )
        .unwrap();
        assert_eq!(
            decimal_of(&outcome.value),
            Decimal::parse_default("0.15").unwrap()
        );

        let volatility = call(
            "finance.portfolio_volatility",
            serde_json::json!({
                "weights": ["0.5", "0.5"],
                "covariance_matrix": {
                    "kind": "matrix",
                    "rows": 2,
                    "cols": 2,
                    "data": [0.04, 0, 0, 0.09]
                }
            }),
        )
        .unwrap();
        // Python 3 fixture: sqrt(0.0325) = 0.18027756377319946.
        let value = decimal_of(&volatility.value).to_f64().unwrap();
        assert!(
            (value - 0.180_277_563_773_199_46).abs() < 1e-12,
            "volatility {value}"
        );

        let single = call(
            "finance.portfolio_volatility",
            serde_json::json!({
                "weights": ["1"],
                "covariance_matrix": {
                    "kind": "matrix",
                    "rows": 1,
                    "cols": 1,
                    "data": [0.04]
                }
            }),
        )
        .unwrap();
        assert_eq!(
            decimal_of(&single.value),
            Decimal::parse_default("0.2").unwrap()
        );
    }

    #[test]
    fn sharpe_ratio_is_exact_when_it_terminates() {
        let outcome = call(
            "finance.sharpe_ratio",
            serde_json::json!({"returns": ["0.1", "0.2", "0.3"]}),
        )
        .unwrap();
        assert_eq!(
            decimal_of(&outcome.value),
            Decimal::parse_default("2").unwrap()
        );
    }

    #[test]
    fn value_at_risk_matches_python_reference() {
        // Historical Python 3 fixture: returns [-0.3,-0.2,-0.1,0,0.4],
        // confidence 0.8 -> index floor(0.2*5) = 1 -> VaR 0.2.
        let historical = call(
            "finance.value_at_risk",
            serde_json::json!({
                "returns": ["-0.3", "-0.2", "-0.1", "0", "0.4"],
                "confidence": "0.8"
            }),
        )
        .unwrap();
        assert_eq!(
            decimal_of(record_field(&historical.value, "var")),
            Decimal::parse_default("0.2").unwrap()
        );
        assert_eq!(
            record_field(&historical.value, "method"),
            &Value::text("historical")
        );

        // Normal Python fixture: statistics.NormalDist().inv_cdf(0.95) =
        // 1.6448536269514722 for mean 0 and sample sd 1 (series [-1, 0, 1]).
        let normal = call(
            "finance.value_at_risk",
            serde_json::json!({
                "returns": [-1, 0, 1],
                "confidence": "0.95",
                "method": "normal"
            }),
        )
        .unwrap();
        let var = decimal_of(record_field(&normal.value, "var"))
            .to_f64()
            .unwrap();
        assert!(
            (var - 1.644_853_626_951_472_2).abs() < 1e-9,
            "normal VaR {var}"
        );
    }

    #[test]
    fn conditional_value_at_risk_reference() {
        // Historical Python fixture: mean of the returns at or below -0.2 is
        // -0.25, so the expected shortfall is 0.25.
        let historical = call(
            "finance.conditional_value_at_risk",
            serde_json::json!({
                "returns": ["-0.3", "-0.2", "-0.1", "0", "0.4"],
                "confidence": "0.8"
            }),
        )
        .unwrap();
        assert_eq!(
            decimal_of(record_field(&historical.value, "cvar")),
            Decimal::parse_default("0.25").unwrap()
        );

        // Normal Python fixture for mean 0, sample sd 1, confidence 0.95:
        // phi(z) / (1 - c) = 2.062712807507429.
        let normal = call(
            "finance.conditional_value_at_risk",
            serde_json::json!({
                "returns": [-1, 0, 1],
                "confidence": "0.95",
                "method": "normal"
            }),
        )
        .unwrap();
        let cvar = decimal_of(record_field(&normal.value, "cvar"))
            .to_f64()
            .unwrap();
        assert!(
            (cvar - 2.062_712_807_507_429).abs() < 1e-9,
            "normal CVaR {cvar}"
        );
    }

    #[test]
    fn max_drawdown_matches_python_fixture() {
        // Python 3 exact fixture for [0.1, -0.2, 0.05, -0.3, 0.4]:
        // equity 1.1 -> 0.88 -> 0.924 -> 0.6468, so
        // (1.1 - 0.6468) / 1.1 = 0.412 with peak index 1 and trough index 4.
        let outcome = call(
            "finance.max_drawdown",
            serde_json::json!({"returns": ["0.1", "-0.2", "0.05", "-0.3", "0.4"]}),
        )
        .unwrap();
        assert_eq!(
            decimal_of(record_field(&outcome.value, "max_drawdown")),
            Decimal::parse_default("0.412").unwrap()
        );
        assert_eq!(
            record_field(&outcome.value, "peak_index"),
            &Value::integer(BigInt::from(1))
        );
        assert_eq!(
            record_field(&outcome.value, "trough_index"),
            &Value::integer(BigInt::from(4))
        );
    }

    #[test]
    fn beta_of_identical_and_scaled_series() {
        let identical = call(
            "finance.beta",
            serde_json::json!({
                "asset_returns": ["0.02", "-0.01", "0.03", "0.05", "-0.02"],
                "market_returns": ["0.02", "-0.01", "0.03", "0.05", "-0.02"]
            }),
        )
        .unwrap();
        assert_eq!(
            decimal_of(&identical.value),
            Decimal::parse_default("1").unwrap()
        );

        // Python 3 fixture: a series equal to twice the market has beta 2.
        let doubled = call(
            "finance.beta",
            serde_json::json!({
                "asset_returns": ["0.02", "-0.04", "0.08", "0.06", "-0.02"],
                "market_returns": ["0.01", "-0.02", "0.04", "0.03", "-0.01"]
            }),
        )
        .unwrap();
        assert_eq!(
            decimal_of(&doubled.value),
            Decimal::parse_default("2").unwrap()
        );
    }

    #[test]
    fn capm_matches_python_fixture() {
        let outcome = call(
            "finance.capm",
            serde_json::json!({
                "risk_free_rate": "0.03",
                "beta": "1.2",
                "market_return": "0.08"
            }),
        )
        .unwrap();
        assert_eq!(
            decimal_of(record_field(&outcome.value, "expected_return")),
            Decimal::parse_default("0.09").unwrap()
        );
        assert_eq!(
            decimal_of(record_field(&outcome.value, "risk_premium")),
            Decimal::parse_default("0.06").unwrap()
        );
    }

    // ------------------------------------------------------------------
    // Contract tests
    // ------------------------------------------------------------------

    #[test]
    fn every_function_declares_identity_examples_and_method_ref() {
        for function in module().functions {
            let descriptor = function.descriptor();
            assert!(
                descriptor.id.starts_with("finance."),
                "unexpected id {}",
                descriptor.id
            );
            assert_eq!(descriptor.module, "finance");
            assert_eq!(descriptor.version, "1.0.0");
            assert!(
                !descriptor.examples.is_empty(),
                "function {} has no examples",
                descriptor.id
            );
            assert!(
                descriptor
                    .method_ref
                    .starts_with("docs/methods/finance.md#"),
                "function {} has no method ref",
                descriptor.id
            );
            assert!(
                !descriptor.parameters.is_empty(),
                "function {} has no parameters",
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
                            true,
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
