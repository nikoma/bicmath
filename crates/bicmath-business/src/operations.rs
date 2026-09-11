//! Inventory and queueing models.
//!
//! These models produce irrational quantities and therefore use binary64 with
//! `libm` fallbacks on wasm32. They require auto or scientific mode, and every
//! result is reported as approximate.

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, CostClass, Example, FunctionDescriptor, Outcome, ParamDescriptor, require_mode,
};
use bicmath_core::envelope::Exactness;
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::number::NumericMode;
use bicmath_core::value::Value;

use crate::f64math;
use crate::util::{
    exact_schema, example_args, field, float_schema, float_value, integer_schema, parse_value,
    record_schema, text_schema,
};

const EOQ_METHOD: &str = "eoq";
const MM1_METHOD: &str = "mm1";
const ERLANG_C_METHOD: &str = "erlang_c";

/// Distributions, queueing models, and other binary64 models require auto or
/// scientific mode because their results are approximate.
fn inferential_modes() -> Vec<NumericMode> {
    vec![NumericMode::Auto, NumericMode::Scientific]
}

// ---------------------------------------------------------------------------
// business.eoq
// ---------------------------------------------------------------------------

fn eoq_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "business.eoq",
        "business",
        "1.0.0",
        "Economic order quantity",
        "Economic order quantity, order frequency, and inventory cost split.",
    )
    .with_description(
        "eoq = sqrt(2 * annual_demand * order_cost / holding_cost_per_unit); \
         orders_per_year = annual_demand / eoq; cycle_time_periods = eoq / annual_demand; \
         total_ordering_cost = orders_per_year * order_cost; total_holding_cost = \
         (eoq / 2) * holding_cost_per_unit; total_cost = total_ordering_cost + \
         total_holding_cost. The result is an approximation computed in binary64 and is \
         reported approximate. All three inputs must be strictly positive.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("annual_demand", "Annual demand in units.", exact_schema()),
        ParamDescriptor::required("order_cost", "Fixed cost per order.", exact_schema()),
        ParamDescriptor::required(
            "holding_cost_per_unit",
            "Annual holding cost per unit.",
            exact_schema(),
        ),
    ])
    .with_output(
        record_schema(vec![
            field("eoq", float_schema()),
            field("orders_per_year", float_schema()),
            field("cycle_time_periods", float_schema()),
            field("total_ordering_cost", float_schema()),
            field("total_holding_cost", float_schema()),
            field("total_cost", float_schema()),
            field("method", text_schema()),
        ]),
        "Economic order quantity, orders per year, cycle time, ordering and holding costs, \
         total cost, and method.",
    )
    .with_modes(inferential_modes())
    .with_cost(CostClass::Constant)
    .with_units_rule(
        "annual_demand and eoq are in units; cycle_time_periods is a fraction of one year; \
         costs are in one currency unit.",
    )
    .with_method_ref("docs/methods/business.md#eoq")
    .with_tags(["inventory", "eoq", "ordering"])
    .with_examples(vec![
        Example::new(
            "known EOQ",
            example_args(&[
                ("annual_demand", serde_json::json!("10000")),
                ("order_cost", serde_json::json!("50")),
                ("holding_cost_per_unit", serde_json::json!("2")),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "eoq": {"kind": "float64", "value": "707.1067811865476"},
            "orders_per_year": {"kind": "float64", "value": "14.14213562373095"},
            "cycle_time_periods": {"kind": "float64", "value": "0.07071067811865475"},
            "total_ordering_cost": {"kind": "float64", "value": "707.1067811865474"},
            "total_holding_cost": {"kind": "float64", "value": "707.1067811865476"},
            "total_cost": {"kind": "float64", "value": "1414.213562373095"},
            "method": EOQ_METHOD
        }))),
        Example::new(
            "zero demand is rejected",
            example_args(&[
                ("annual_demand", serde_json::json!("0")),
                ("order_cost", serde_json::json!("50")),
                ("holding_cost_per_unit", serde_json::json!("2")),
            ]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_eoq(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "business.eoq")?;
    let annual_demand = args.f64_param("annual_demand")?;
    let order_cost = args.f64_param("order_cost")?;
    let holding_cost_per_unit = args.f64_param("holding_cost_per_unit")?;
    if annual_demand <= 0.0 {
        return Err(EngineError::domain("annual_demand must be positive"));
    }
    if order_cost <= 0.0 {
        return Err(EngineError::domain("order_cost must be positive"));
    }
    if holding_cost_per_unit <= 0.0 {
        return Err(EngineError::domain(
            "holding_cost_per_unit must be positive",
        ));
    }
    let eoq = f64math::sqrt(2.0 * annual_demand * order_cost / holding_cost_per_unit);
    if !eoq.is_finite() || eoq <= 0.0 {
        return Err(EngineError::domain(
            "the economic order quantity is not finite for these inputs",
        ));
    }
    let orders_per_year = annual_demand / eoq;
    let cycle_time_periods = eoq / annual_demand;
    let total_ordering_cost = orders_per_year * order_cost;
    let total_holding_cost = eoq / 2.0 * holding_cost_per_unit;
    let total_cost = total_ordering_cost + total_holding_cost;
    let value = Value::record([
        ("eoq", float_value(eoq)?),
        ("orders_per_year", float_value(orders_per_year)?),
        ("cycle_time_periods", float_value(cycle_time_periods)?),
        ("total_ordering_cost", float_value(total_ordering_cost)?),
        ("total_holding_cost", float_value(total_holding_cost)?),
        ("total_cost", float_value(total_cost)?),
        ("method", Value::text(EOQ_METHOD)),
    ]);
    Ok(Outcome::new(value, Exactness::Approximate))
}

// ---------------------------------------------------------------------------
// business.queue_mm1
// ---------------------------------------------------------------------------

fn queue_mm1_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "business.queue_mm1",
        "business",
        "1.0.0",
        "M/M/1 queue",
        "M/M/1 steady-state performance measures.",
    )
    .with_description(
        "Single-server Markovian queue with Poisson arrivals and exponential service. \
         utilization = arrival_rate / service_rate; p0 = 1 - utilization; \
         l = arrival_rate / (service_rate - arrival_rate); lq = utilization * l; \
         w = 1 / (service_rate - arrival_rate); wq = utilization / \
         (service_rate - arrival_rate); p_wait = utilization. Stability requires \
         arrival_rate < service_rate; otherwise the queue grows without bound and a \
         domain error is returned. Results are binary64 approximations.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "arrival_rate",
            "Mean arrivals per period; non-negative.",
            exact_schema(),
        ),
        ParamDescriptor::required(
            "service_rate",
            "Mean services per period; strictly positive.",
            exact_schema(),
        ),
    ])
    .with_output(
        record_schema(vec![
            field("utilization", float_schema()),
            field("p0", float_schema()),
            field("l", float_schema()),
            field("lq", float_schema()),
            field("w", float_schema()),
            field("wq", float_schema()),
            field("p_wait", float_schema()),
            field("method", text_schema()),
        ]),
        "Utilization, empty-system probability, mean number in system and queue, mean wait \
         in system and queue, probability of waiting, and method.",
    )
    .with_modes(inferential_modes())
    .with_cost(CostClass::Constant)
    .with_units_rule("rates are per period; w and wq are in periods; l and lq are in customers.")
    .with_method_ref("docs/methods/business.md#queue_mm1")
    .with_tags(["queueing", "mm1", "operations"])
    .with_examples(vec![
        Example::new(
            "stable M/M/1 queue",
            example_args(&[
                ("arrival_rate", serde_json::json!("4")),
                ("service_rate", serde_json::json!("5")),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "utilization": {"kind": "float64", "value": "0.8"},
            "p0": {"kind": "float64", "value": "0.2"},
            "l": {"kind": "float64", "value": "4"},
            "lq": {"kind": "float64", "value": "3.2"},
            "w": {"kind": "float64", "value": "1"},
            "wq": {"kind": "float64", "value": "0.8"},
            "p_wait": {"kind": "float64", "value": "0.8"},
            "method": MM1_METHOD
        }))),
        Example::new(
            "unstable queue",
            example_args(&[
                ("arrival_rate", serde_json::json!("5")),
                ("service_rate", serde_json::json!("5")),
            ]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_queue_mm1(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "business.queue_mm1")?;
    let arrival_rate = args.f64_param("arrival_rate")?;
    let service_rate = args.f64_param("service_rate")?;
    if arrival_rate < 0.0 {
        return Err(EngineError::domain("arrival_rate must be non-negative"));
    }
    if service_rate <= 0.0 {
        return Err(EngineError::domain("service_rate must be positive"));
    }
    if arrival_rate >= service_rate {
        return Err(EngineError::domain(
            "stability requires arrival_rate < service_rate",
        ));
    }
    let utilization = arrival_rate / service_rate;
    let p0 = (service_rate - arrival_rate) / service_rate;
    let l = arrival_rate / (service_rate - arrival_rate);
    let lq = utilization * l;
    let w = 1.0 / (service_rate - arrival_rate);
    let wq = utilization / (service_rate - arrival_rate);
    let p_wait = utilization;
    let value = Value::record([
        ("utilization", float_value(utilization)?),
        ("p0", float_value(p0)?),
        ("l", float_value(l)?),
        ("lq", float_value(lq)?),
        ("w", float_value(w)?),
        ("wq", float_value(wq)?),
        ("p_wait", float_value(p_wait)?),
        ("method", Value::text(MM1_METHOD)),
    ]);
    Ok(Outcome::new(value, Exactness::Approximate))
}

// ---------------------------------------------------------------------------
// business.erlang_c
// ---------------------------------------------------------------------------

fn erlang_c_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "business.erlang_c",
        "business",
        "1.0.0",
        "Erlang C",
        "M/M/c Erlang C waiting probability and queue performance measures.",
    )
    .with_description(
        "Multi-server Markovian queue with c identical servers. The offered load is \
         a = arrival_rate / service_rate and utilization = a / staff. The Erlang C \
         probability is evaluated with the numerically stable Erlang B recursion \
         B(0) = 1, B(k) = a * B(k-1) / (k + a * B(k-1)) and \
         C = B(c) / (1 - utilization + utilization * B(c)). \
         average_waiting_time = C / (staff * service_rate - arrival_rate) and \
         average_queue_length = arrival_rate * average_waiting_time. Stability requires \
         arrival_rate < staff * service_rate; results are binary64 approximations.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "staff",
            "Number of identical servers; at least 1.",
            integer_schema(),
        ),
        ParamDescriptor::required(
            "arrival_rate",
            "Mean arrivals per period; non-negative.",
            exact_schema(),
        ),
        ParamDescriptor::required(
            "service_rate",
            "Mean services per period per server; strictly positive.",
            exact_schema(),
        ),
    ])
    .with_output(
        record_schema(vec![
            field("p_wait", float_schema()),
            field("utilization", float_schema()),
            field("average_waiting_time", float_schema()),
            field("average_queue_length", float_schema()),
            field("method", text_schema()),
        ]),
        "Erlang C waiting probability, server utilization, mean queue wait, mean queue \
         length, and method.",
    )
    .with_modes(inferential_modes())
    .with_cost(CostClass::Iterative)
    .with_units_rule(
        "rates are per period; average_waiting_time is in periods; average_queue_length is \
         in customers.",
    )
    .with_method_ref("docs/methods/business.md#erlang_c")
    .with_tags(["queueing", "erlang-c", "staffing"])
    .with_examples(vec![
        Example::new(
            "two servers at 75% utilization",
            example_args(&[
                ("staff", serde_json::json!(2)),
                ("arrival_rate", serde_json::json!("3")),
                ("service_rate", serde_json::json!("2")),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "p_wait": {"kind": "float64", "value": "0.6428571428571428"},
            "utilization": {"kind": "float64", "value": "0.75"},
            "average_waiting_time": {"kind": "float64", "value": "0.6428571428571428"},
            "average_queue_length": {"kind": "float64", "value": "1.9285714285714284"},
            "method": ERLANG_C_METHOD
        }))),
        Example::new(
            "unstable staffing",
            example_args(&[
                ("staff", serde_json::json!(2)),
                ("arrival_rate", serde_json::json!("4")),
                ("service_rate", serde_json::json!("2")),
            ]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_erlang_c(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "business.erlang_c")?;
    let staff = args.usize_param("staff")?;
    if staff == 0 {
        return Err(EngineError::domain("staff must be at least 1"));
    }
    if staff as u64 > ctx.limits.max_iterations {
        return Err(EngineError::new(
            ErrorCode::ResourceLimit,
            format!(
                "staff={staff} exceeds the iteration limit of {}",
                ctx.limits.max_iterations
            ),
        ));
    }
    let arrival_rate = args.f64_param("arrival_rate")?;
    let service_rate = args.f64_param("service_rate")?;
    if arrival_rate < 0.0 {
        return Err(EngineError::domain("arrival_rate must be non-negative"));
    }
    if service_rate <= 0.0 {
        return Err(EngineError::domain("service_rate must be positive"));
    }
    let staff_f = staff as f64;
    let capacity = staff_f * service_rate;
    if arrival_rate >= capacity {
        return Err(EngineError::domain(
            "stability requires arrival_rate < staff * service_rate",
        ));
    }
    let offered_load = arrival_rate / service_rate;
    let utilization = offered_load / staff_f;
    let mut erlang_b = 1.0;
    for k in 1..=staff {
        if k % 4096 == 0 {
            ctx.check()?;
        }
        let k_f = k as f64;
        erlang_b = offered_load * erlang_b / (k_f + offered_load * erlang_b);
    }
    let p_wait = erlang_b / (1.0 - utilization + utilization * erlang_b);
    let average_waiting_time = p_wait / (capacity - arrival_rate);
    let average_queue_length = arrival_rate * average_waiting_time;
    let value = Value::record([
        ("p_wait", float_value(p_wait)?),
        ("utilization", float_value(utilization)?),
        ("average_waiting_time", float_value(average_waiting_time)?),
        ("average_queue_length", float_value(average_queue_length)?),
        ("method", Value::text(ERLANG_C_METHOD)),
    ]);
    Ok(Outcome::new(value, Exactness::Approximate))
}

pub(crate) fn register() -> Vec<std::sync::Arc<dyn bicmath_core::contract::Function>> {
    use bicmath_core::contract::SimpleFunction;
    vec![
        SimpleFunction::arc(eoq_descriptor(), invoke_eoq),
        SimpleFunction::arc(queue_mm1_descriptor(), invoke_queue_mm1),
        SimpleFunction::arc(erlang_c_descriptor(), invoke_erlang_c),
    ]
}
