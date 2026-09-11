# Dependency rationale

BicMath prefers permissive, maintained dependencies and keeps the numerical
core free of transport and runtime crates. The full resolved list, with
versions and licenses, is generated into
[`THIRD_PARTY_LICENSES.md`](../THIRD_PARTY_LICENSES.md) by
`scripts/license-report.sh`.

## Direct dependencies

| Crate | Purpose | License | WASM | Why needed |
|---|---|---|---|---|
| `serde`, `serde_json` | serialization and the wire format | MIT OR Apache-2.0 | yes | standard, permissive serialization; `arbitrary_precision` keeps JSON numbers exact |
| `num-bigint` | arbitrary-precision integers | MIT OR Apache-2.0 | yes | foundational exact integer storage; no need to reimplement |
| `num-rational` | normalized exact rationals | MIT OR Apache-2.0 | yes | normalized numerator/denominator with positive denominator |
| `num-integer`, `num-traits` | integer/trait helpers used by the above | MIT OR Apache-2.0 | yes | companion crates of `num-bigint` |
| `thiserror` | structured error boilerplate | MIT OR Apache-2.0 | yes | no runtime cost, widely used |
| `sha2` | fingerprints and receipts | MIT OR Apache-2.0 | yes | standard SHA-256; fingerprints are not a security boundary |
| `libm` | wasm fallback for transcendental functions | MIT | yes | deterministic fallback where the platform libm is unavailable |
| `chrono` | calendar dates for dated cash flows | MIT OR Apache-2.0 | yes | leap-year/date arithmetic without a timezone database |
| `rmcp` (pinned 3.2.0) | official MCP Rust SDK | Apache-2.0 | no | required to implement MCP correctly rather than hand-writing the protocol |
| `tokio` | async runtime for MCP transports | MIT | no | required by `rmcp`; never used by numerical crates |
| `clap` | CLI argument parsing | MIT OR Apache-2.0 | no | standard, permissive CLI |
| `tracing`, `tracing-subscriber` | stderr diagnostics | MIT | no | structured logging without touching stdout |
| `axum`, `tower` (optional) | Streamable HTTP serving | MIT | no | serving the SDK's tower service |
| `wasm-bindgen`, `js-sys`, `serde-wasm-bindgen` | browser bindings | MIT OR Apache-2.0 | wasm only | standard WASM interop |

## Deliberate exclusions

- No statistics application, calculator, Python process, hosted service, or LLM
  is wrapped. Algorithms are implemented and tested in this repository.
- No database dependency exists for calculations.
- No runtime plugin loading, shared-library loading, or hot installation.
- External reference tools (Python stdlib, published tables) generated test
  fixtures only; they are not runtime dependencies and CI does not need network
  access for normal tests.
- `wasm-bindgen` is pinned to the version matching the installed CLI so the
  WASM build is reproducible.

## License policy

The project is MIT licensed. Dependencies are permissively licensed
(MIT/Apache-2.0/Unlicense); the resolved graph contains no copyleft runtime
dependency. Required notices are preserved in `THIRD_PARTY_LICENSES.md`. No
custom commercial exceptions are introduced.
