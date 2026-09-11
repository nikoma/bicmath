//! Cohort retention and revenue projection.

use num_bigint::BigInt;
use num_rational::BigRational;

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, CostClass, Example, FunctionDescriptor, Outcome, ParamDescriptor,
};
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::schema::ValueSchema;
use bicmath_core::value::Value;

use crate::util::{
    all_modes, decimal_schema, exact_rational, exact_schema, exactness, example_args, field,
    number_to_rational, parse_value, rational_value, record_schema, text_schema,
};

const RETENTION_RATES_METHOD: &str = "retention_rates";
const COHORT_REVENUE_METHOD: &str = "cohort_revenue";

fn zero() -> BigRational {
    BigRational::from_integer(BigInt::from(0))
}

fn cohort_sizes_schema() -> ValueSchema {
    ValueSchema::array_with_len(exact_schema(), 1, None)
}

// ---------------------------------------------------------------------------
// business.retention_rates
// ---------------------------------------------------------------------------

fn retention_rates_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "business.retention_rates",
        "business",
        "1.0.0",
        "Retention rates",
        "Period-over-period retention rates between consecutive cohort sizes.",
    )
    .with_description(
        "For a sequence of cohort sizes s0, s1, ..., the returned rates are s1/s0, s2/s1, \
         ... in order. At least two cohort sizes are required, and every size must be \
         strictly positive. Rates are exact when they terminate as decimals and rounded to \
         the context precision otherwise.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "cohort_sizes",
        "Cohort sizes in chronological order; at least two positive values.",
        cohort_sizes_schema(),
    )])
    .with_output(
        record_schema(vec![
            field("rates", ValueSchema::array(decimal_schema())),
            field("method", text_schema()),
        ]),
        "Retention rate between each consecutive pair of cohorts, and method.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_units_rule("cohort sizes and rates are counts and dimensionless fractions.")
    .with_method_ref("docs/methods/business.md#retention_rates")
    .with_tags(["cohorts", "retention"])
    .with_examples(vec![
        Example::new(
            "three shrinking cohorts",
            example_args(&[("cohort_sizes", serde_json::json!([1000, 800, 600]))]),
        )
        .with_value(parse_value(serde_json::json!({
            "rates": [
                {"kind": "decimal", "value": "0.8"},
                {"kind": "decimal", "value": "0.75"}
            ],
            "method": RETENTION_RATES_METHOD
        }))),
        Example::new(
            "a single cohort has no rates",
            example_args(&[("cohort_sizes", serde_json::json!([1000]))]),
        )
        .with_error(ErrorCode::InsufficientObservations),
    ])
}

fn invoke_retention_rates(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let sizes = args.array("cohort_sizes")?;
    if sizes.len() < 2 {
        return Err(EngineError::new(
            ErrorCode::InsufficientObservations,
            "at least two cohort sizes are required to compute retention rates",
        ));
    }
    let mut rounded = false;
    let mut rates = Vec::with_capacity(sizes.len() - 1);
    let mut previous: Option<BigRational> = None;
    for (index, item) in sizes.iter().enumerate() {
        let path = format!("cohort_sizes[{index}]");
        let number = item.as_number().map_err(|e| e.with_path(path.clone()))?;
        let size = number_to_rational(number, &path)?;
        if size <= zero() {
            return Err(EngineError::domain(format!(
                "{path} must be strictly positive"
            )));
        }
        if let Some(prior) = &previous {
            rates.push(rational_value(&(&size / prior), ctx, &mut rounded)?);
        }
        previous = Some(size);
    }
    let value = Value::record([
        ("rates", Value::Array(rates)),
        ("method", Value::text(RETENTION_RATES_METHOD)),
    ]);
    Ok(Outcome::new(value, exactness(rounded)))
}

// ---------------------------------------------------------------------------
// business.cohort_revenue
// ---------------------------------------------------------------------------

fn cohort_revenue_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "business.cohort_revenue",
        "business",
        "1.0.0",
        "Cohort revenue",
        "Simple cohort revenue projection from cohort sizes and revenue per user.",
    )
    .with_description(
        "Each cohort contributes cohort_size * revenue_per_user, and total_revenue is the \
         sum over cohorts. The projection is a simple per-user revenue model: it does not \
         model retention decay or discounting. Multiplication is exact, so the result is \
         exact whenever the inputs are exact.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "cohort_sizes",
            "Cohort sizes in chronological order.",
            ValueSchema::array_with_len(exact_schema(), 1, None),
        ),
        ParamDescriptor::required(
            "revenue_per_user",
            "Revenue per user over the projection horizon.",
            exact_schema(),
        ),
    ])
    .with_output(
        record_schema(vec![
            field("revenue_per_cohort", ValueSchema::array(decimal_schema())),
            field("total_revenue", decimal_schema()),
            field("method", text_schema()),
        ]),
        "Revenue for each cohort, total revenue, and method.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_units_rule(
        "cohort_sizes are counts; revenue_per_user, per-cohort revenue, and total_revenue \
         share one currency unit.",
    )
    .with_method_ref("docs/methods/business.md#cohort_revenue")
    .with_tags(["cohorts", "revenue", "projection"])
    .with_examples(vec![
        Example::new(
            "three cohorts at 2.50 per user",
            example_args(&[
                ("cohort_sizes", serde_json::json!([1000, 800, 600])),
                ("revenue_per_user", serde_json::json!("2.5")),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "revenue_per_cohort": [
                {"kind": "decimal", "value": "2500"},
                {"kind": "decimal", "value": "2000"},
                {"kind": "decimal", "value": "1500"}
            ],
            "total_revenue": {"kind": "decimal", "value": "6000"},
            "method": COHORT_REVENUE_METHOD
        }))),
    ])
}

fn invoke_cohort_revenue(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let sizes = args.array("cohort_sizes")?;
    let revenue_per_user = exact_rational(args, "revenue_per_user")?;
    let mut rounded = false;
    let mut total = zero();
    let mut per_cohort = Vec::with_capacity(sizes.len());
    for (index, item) in sizes.iter().enumerate() {
        let path = format!("cohort_sizes[{index}]");
        let number = item.as_number().map_err(|e| e.with_path(path.clone()))?;
        let size = number_to_rational(number, &path)?;
        let revenue = &size * &revenue_per_user;
        total += &revenue;
        per_cohort.push(rational_value(&revenue, ctx, &mut rounded)?);
    }
    let value = Value::record([
        ("revenue_per_cohort", Value::Array(per_cohort)),
        ("total_revenue", rational_value(&total, ctx, &mut rounded)?),
        ("method", Value::text(COHORT_REVENUE_METHOD)),
    ]);
    Ok(Outcome::new(value, exactness(rounded)))
}

pub(crate) fn register() -> Vec<std::sync::Arc<dyn bicmath_core::contract::Function>> {
    use bicmath_core::contract::SimpleFunction;
    vec![
        SimpleFunction::arc(retention_rates_descriptor(), invoke_retention_rates),
        SimpleFunction::arc(cohort_revenue_descriptor(), invoke_cohort_revenue),
    ]
}
