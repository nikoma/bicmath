//! Representative benchmark harness.
//!
//! Run with: cargo run --release -p bicmath-engine --example bench
//!
//! The harness times end-to-end engine calls (validation, execution, envelope,
//! and fingerprint) so the numbers in docs/benchmarks.md are reproducible on
//! named hardware. `std::hint::black_box` and a printed checksum prevent the
//! optimiser from hoisting or eliminating the measured work.

use std::hint::black_box;
use std::time::Instant;

use bicmath_core::error::EngineError;
use bicmath_engine::{CallRequest, Engine, EngineConfig};

fn call(engine: &Engine, function: &str, arguments: serde_json::Value) -> Result<u64, EngineError> {
    let request: CallRequest =
        serde_json::from_value(serde_json::json!({"function": function, "arguments": arguments}))
            .expect("request");
    let envelope = engine.call(&request, &engine.base_context())?;
    Ok(envelope.fingerprint.len() as u64)
}

fn time<F: FnMut(u32) -> u64>(label: &str, iterations: u32, mut f: F) {
    // Warm up so first-allocation effects do not dominate.
    let mut checksum = 0u64;
    checksum ^= black_box(f(0));
    let start = Instant::now();
    for i in 0..iterations {
        checksum ^= black_box(f(i));
    }
    let elapsed = start.elapsed();
    let per_call = elapsed.as_secs_f64() * 1_000_000.0 / f64::from(iterations);
    println!(
        "{label:<42} {per_call:>12.3} us/call   ({iterations} calls, {:?} total, checksum {checksum})",
        elapsed
    );
}

fn main() {
    let engine = Engine::full(EngineConfig::default()).expect("engine builds");
    println!("BicMath engine benchmark");
    println!(
        "engine {} | functions {} | modules {}",
        bicmath_core::VERSION,
        engine.list_functions(&Default::default()).total,
        engine.list_modules().len()
    );
    println!();

    time("arithmetic.add decimals 0.1+0.2", 20_000, |i| {
        call(
            &engine,
            "arithmetic.add",
            serde_json::json!({"a": format!("0.{}", i % 100), "b": "0.2"}),
        )
        .unwrap()
    });

    let big = "9".repeat(2048 / 3 + 1);
    time("arithmetic.add 2048-bit integers", 5_000, |_| {
        call(
            &engine,
            "arithmetic.add",
            serde_json::json!({
                "a": {"kind": "integer", "value": big},
                "b": {"kind": "integer", "value": big}
            }),
        )
        .unwrap()
    });

    time("arithmetic.div 1/3 decimal (rounded)", 20_000, |i| {
        call(
            &engine,
            "arithmetic.div",
            serde_json::json!({
                "a": {"kind": "decimal", "value": format!("{}", i + 1)},
                "b": {"kind": "decimal", "value": "3"}
            }),
        )
        .unwrap()
    });

    time("statistics.median 1000 integers", 2_000, |i| {
        let values: Vec<i64> = (0..1_000).map(|k| ((k * 37) + i as i64) % 10_000).collect();
        call(
            &engine,
            "statistics.median",
            serde_json::json!({"values": values}),
        )
        .unwrap()
    });

    time("statistics.mean 1000 exact integers", 2_000, |i| {
        let values: Vec<i64> = (0..1_000)
            .map(|k| ((k * 41) + i as i64) % 100_000)
            .collect();
        call(
            &engine,
            "statistics.mean",
            serde_json::json!({"values": values}),
        )
        .unwrap()
    });

    time("finance.amortization 360 periods", 2_000, |i| {
        call(
            &engine,
            "finance.amortization",
            serde_json::json!({
                "principal": {"kind": "money", "amount": {"kind": "decimal", "value": format!("{}.00", 200_000 + i)}, "currency": "USD"},
                "annual_rate": "0.005",
                "periods": 360,
                "currency": "USD"
            }),
        )
        .unwrap()
    });

    time("linear_algebra.solve exact 8x8", 500, |i| {
        let mut data = Vec::new();
        for row in 0..8i64 {
            for col in 0..8i64 {
                data.push(if row == col { 4 + i as i64 % 3 } else { 1 });
            }
        }
        call(
            &engine,
            "linear_algebra.solve",
            serde_json::json!({
                "a": {"kind": "matrix", "rows": 8, "cols": 8, "data": data},
                "b": [1, 2, 3, 4, 5, 6, 7, 8]
            }),
        )
        .unwrap()
    });

    let scientific_ctx = serde_json::json!({"mode": "scientific"});
    time("scientific.integrate x^2 on [0,1]", 2_000, |_| {
        let request: CallRequest = serde_json::from_value(serde_json::json!({
            "function": "scientific.integrate",
            "arguments": {"expression": "x^2", "variable": "x", "lower": 0, "upper": 1},
            "context": scientific_ctx
        }))
        .unwrap();
        let envelope = engine.call(&request, &engine.base_context()).unwrap();
        envelope.fingerprint.len() as u64
    });

    time("statistics.stratified_experiment (campaign)", 2_000, |i| {
        call(
            &engine,
            "statistics.stratified_experiment",
            serde_json::json!({
                "strata": [
                    {"name": "beginners", "target_weight": 30000,
                     "treatment": {"assigned": 8000, "outcomes": [
                        {"value": 100, "count": 320 + i % 7}, {"value": -10, "count": 80}, {"value": 0, "count": 7600 - i % 7}]},
                     "control": {"assigned": 2000, "outcomes": [
                        {"value": 100, "count": 76}, {"value": -10, "count": 4}, {"value": 0, "count": 1920}]}},
                    {"name": "advanced", "target_weight": 10000,
                     "treatment": {"assigned": 2000, "outcomes": [
                        {"value": 100, "count": 320}, {"value": -10, "count": 80}, {"value": 0, "count": 1600}]},
                     "control": {"assigned": 8000, "outcomes": [
                        {"value": 100, "count": 1140}, {"value": -10, "count": 60}, {"value": 0, "count": 6800}]}}
                ],
                "fixed_cost": 12000,
                "variance_ddof": 1
            }),
        )
        .unwrap()
    });
}
