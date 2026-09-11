# Benchmark methodology and measured results

No speed claims are made beyond the measurements below.

## Method

- Harness: `crates/bicmath-engine/examples/bench.rs`, dependency-free, using
  `std::time::Instant` with a warm-up call and `std::hint::black_box` so the
  optimiser cannot hoist or eliminate measured work. A checksum is printed to
  confirm the work ran.
- What is measured: the full public engine call — request parsing, argument
  coercion/validation, execution, envelope construction, and fingerprinting.
  This is the cost an MCP/CLI/WASM caller actually pays.
- Inputs vary slightly per iteration so results are not loop-invariant.

Reproduce:

```sh
cargo run --release -p bicmath-engine --example bench
```

## Environment

| | |
|---|---|
| Machine | Apple M4 |
| OS | macOS 26.5.1 (build 25F80) |
| Toolchain | rustc 1.96.0 (2026-05-25) |
| Profile | release (thin LTO, codegen-units 1) |

## Measured results

| Operation | µs/call | Notes |
|---|---:|---|
| `arithmetic.add` decimals | 17.9 | `0.1 + 0.2`, exact decimal path |
| `arithmetic.add` 2048-bit integers | 70.0 | BigInt addition with limit checks |
| `arithmetic.div` decimal (rounded) | 22.6 | `1/3` to context precision |
| `statistics.median` 1000 integers | 2300.0 | exact sort and middle selection |
| `statistics.mean` 1000 exact integers | 2174.3 | exact rational accumulation |
| `finance.amortization` 360 periods | 1476.1 | schedule with per-period rounding |
| `linear_algebra.solve` exact 8×8 | 871.8 | exact rational elimination |
| `scientific.integrate` `x^2` on [0,1] | 27.5 | adaptive Simpson |
| `statistics.stratified_experiment` (campaign) | 212.5 | two strata, categorical outcomes |

## Observations and limits

- Descriptive statistics on exact integer inputs use exact rational
  arithmetic, which is much slower than a float pass. This is a deliberate
  exactness trade-off; scientific/float paths are faster where exactness is not
  required.
- Numbers were measured on one machine and one toolchain. They are indicative,
  not a guarantee. Re-run the harness on your own hardware before making
  capacity decisions.
- The engine enforces operation budgets, so worst-case latency is bounded by
  policy even when a single call is expensive.
