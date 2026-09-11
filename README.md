# BicMath

BicMath is a standalone computational toolkit for mathematics, statistics,
science, and business finance. MCP is one interface to it: the same engine is
also a CLI, an embeddable Rust library, and a WASM build for offline browser
use. It ships 15 modules and 319 functions.

The engine executes defined methods on explicit inputs and returns inspectable
results: the method, assumptions, exactness classification, warnings, limits,
and a deterministic fingerprint. It is designed for AI agents and people who
need dependable computation without a cloud account, API key, or network
connection.

> A successful calculation does not certify that the right business question or
> statistical design was chosen. BicMath is a transparent computation engine,
> not a financial decision-maker.

BicMath™ is a trademark of Nikolai Manek. See [NOTICE](NOTICE) for copyright and
attribution details.

- **Exactness is explicit.** Integer, rational, and decimal inputs stay exact.
  Decimal division rounds only under a declared context and reports
  inexactness. Float64 results are always labelled approximate.
- **One implementation per operation.** CLI, MCP, expressions, batches, Rust
  callers, and WASM all use the same engine and registry.
- **No arbitrary code execution.** The expression grammar is restricted; it
  never evaluates Rust, JavaScript, shell commands, imports, or remote code.
- **Evidence before breadth.** Every registered function has a real
  implementation, documented domain, executable examples, and tests.
- **Claims match their scope.** Verification states exactly what it
  established; intervals are labelled heuristic padding, not certified;
  budgets select working precision only and never change method defaults.

## Table of contents

- [Install and build](#install-and-build)
- [Quick start](#quick-start)
- [CLI reference](#cli-reference)
- [MCP server](#mcp-server)
- [Embedding in Rust](#embedding-in-rust)
- [WASM in the browser](#wasm-in-the-browser)
- [Exact versus approximate](#exact-versus-approximate)
- [Modules](#modules)
- [Configuration](#configuration)
- [Resource limits and privacy](#resource-limits-and-privacy)
- [Testing and verification](#testing-and-verification)
- [Project layout](#project-layout)
- [Documentation](#documentation)
- [Feature profiles](#feature-profiles)
- [Status and limitations](#status-and-limitations)
- [Contributing](#contributing)
- [Security](#security)
- [License](#license)

## Install and build

Requirements:

- Rust 1.88 or newer (the repository pins 1.96.0 in `rust-toolchain.toml`; MSRV
  is 1.88).
- No database, cloud account, API key, or network access is required to build or
  run calculations.

```sh
git clone <repository>
cd bicmath

# Debug build
cargo build --workspace

# Optimized release build
cargo build --release

# Install the `bicmath` command from this checkout
cargo install --path crates/bicmath

# Or, once the crate is published to crates.io
cargo install bicmath
```

The native binary is `target/release/bicmath`. `bicmath doctor` verifies the
installation and executes every documented example.

## Quick start

```sh
./target/release/bicmath doctor
./target/release/bicmath modules
./target/release/bicmath functions --module statistics
./target/release/bicmath describe statistics.median
```

Calculate exact decimals from stdin (JSON is the default output):

```sh
echo '{"function":"arithmetic.add","arguments":{"a":"0.1","b":"0.2"}}' \
  | ./target/release/bicmath calculate
```

```json
{
  "result": { "kind": "decimal", "value": "0.3" },
  "exactness": "exact",
  "fingerprint": "…"
}
```

Evaluate a restricted expression with named arguments:

```sh
echo '{"expression":"finance.npv(rate = 0.08, cashflows = [-10000, 4000, 4000, 4000], currency = \"USD\")"}' \
  | ./target/release/bicmath evaluate
```

Execute a dependency graph in one request:

```sh
echo '{
  "nodes": [
    {"type":"call","name":"total","function":"arithmetic.add","arguments":{"a":2,"b":3}},
    {"type":"call","name":"double","function":"arithmetic.mul","arguments":{"a":{"$ref":"total"},"b":2}},
    {"type":"evaluate","name":"check","expression":"double > 9"}
  ],
  "outputs": ["total","double","check"]
}' | ./target/release/bicmath batch
```

## CLI reference

| Command | Purpose |
|---|---|
| `serve --transport stdio` | Run the MCP server over stdio (default) |
| `serve --transport http --bind 127.0.0.1:8080` | Run the MCP Streamable HTTP server (needs `--features http`) |
| `modules` | List enabled modules and capabilities |
| `functions --module <id> [--query <text>] [--limit N]` | List functions with filtering and pagination |
| `describe <function>` | Full schema, domain, conventions, and examples |
| `calculate --request <file>` | Execute one function (file, or stdin with `-`) |
| `evaluate --request <file>` | Evaluate an expression |
| `batch --request <file>` | Execute a bounded dependency graph |
| `replay --receipt <file>` | Re-run a stored receipt and check its fingerprint |
| `doctor` | Validate the install and execute every documented example |
| `config` | Print the effective configuration (never credentials) |
| `docs --out docs/methods` | Regenerate the function reference from the registry |
| `sdk --language typescript\|python` | Generate a typed client SDK from the registry |

Behavior:

- Output is machine-readable JSON by default; `--format text` renders a concise
  human view.
- Exit codes: `0` success, `1` calculation/engine error (a structured error is
  printed), `2` configuration, transport, or usage error.
- Complex or sensitive inputs are read from files or stdin, never forced onto
  the command line.
- Configuration precedence, lowest to highest: built-in defaults, JSON config
  file (`--config` or `BICMATH_CONFIG`), environment variables
  (`BICMATH_PROFILE`, `BICMATH_DISABLED_MODULES`, `BICMATH_MAX_IN_FLIGHT`,
  `BICMATH_LOG`), then command-line flags. Unknown configuration keys are
  rejected so misspellings cannot silently change behavior.

## MCP server

The server uses the official Rust SDK (`rmcp 3.2.0`) and supports protocol
revisions `2026-07-28` and `2025-11-25`. Both lifecycles are covered by
end-to-end tests.

### stdio (recommended for desktop clients)

The client spawns the process; nothing needs to be left running.

```json
{
  "mcpServers": {
    "bicmath": {
      "command": "/absolute/path/to/bicmath",
      "args": ["serve", "--transport", "stdio"],
      "env": { "BICMATH_LOG": "warn" }
    }
  }
}
```

### Streamable HTTP (optional)

```sh
cargo build --release --features bicmath/http
./target/release/bicmath serve --transport http --bind 127.0.0.1:8080
# MCP endpoint: http://127.0.0.1:8080/mcp
```

HTTP binds to loopback by default and validates Host and Origin headers. A
static bearer token is not automatically interoperable MCP OAuth; remote
deployments must sit behind a properly configured trusted gateway or implement
MCP authorization for the negotiated revision.

### Tools

The default compact profile exposes six tools:

| Tool | Purpose |
|---|---|
| `list_modules` | Catalog of enabled modules and capabilities |
| `list_functions` | Filtered, paginated function summaries |
| `describe_function` | Full schema, conventions, domains, examples |
| `calculate` | Execute one qualified function with typed arguments |
| `evaluate` | Execute a restricted expression with named bindings |
| `batch` | Execute a bounded dependency graph |

`serve --profile expanded` additionally exposes one typed tool per enabled
function (for example `statistics__median`), generated from the same registry.
See [docs/mcp.md](docs/mcp.md).

## Embedding in Rust

```rust
use bicmath_engine::{CallRequest, Engine, EngineConfig};

let engine = Engine::full(EngineConfig::default()).expect("modules validate");
let request: CallRequest = serde_json::from_value(serde_json::json!({
    "function": "statistics.variance",
    "arguments": { "values": [1, 2, 3], "ddof": 0 }
}))?;
let envelope = engine.call(&request, &engine.base_context())?;

println!("{}", serde_json::to_string(&envelope.result)?); // {"kind":"rational","numerator":"2","denominator":"3"}
println!("exactness: {}", envelope.exactness.as_str());
```

The engine is synchronous and has no transport, async runtime, filesystem, or
network dependencies, so it can be embedded anywhere. A runnable example lives
in `crates/bicmath-engine/examples/embed.rs`; the module-author guide is in
[docs/architecture.md](docs/architecture.md#module-author-guide).

## WASM in the browser

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.126   # must match the pinned crate
./scripts/build-wasm.sh
node examples/wasm/smoke.cjs                        # Node smoke test (all modules)
python3 -m http.server 8000 --directory examples/wasm
# open http://localhost:8000/ and click the buttons
```

Exact integer, rational, and decimal payloads are strings in the canonical wire
format, so they survive JavaScript round trips unchanged. The browser example
includes a Web Worker path so heavy requests do not block the main thread.

## Exact versus approximate

| Input | Operation | Result | Classification |
|---|---|---|---|
| `0.1` + `0.2` | decimal addition | `0.3` | exact |
| `9007199254740993` + `1` | integer addition | `9007199254740994` | exact |
| `1/3` + `1/6` | rational addition | `1/2` | exact |
| `1` / `3` (decimals) | decimal division | rounded to context precision | rounded |
| `sqrt(0.25)` | square root | `0.5` | exact |
| `sqrt(2)` (auto) | square root | decimal approximation | approximate |
| `sin(1)` | scientific function | float64 | approximate |
| `1/0`, `0/0`, `asin(2)`, `ln(-1)` | — | structured domain error | — |

A mathematically unbounded confidence endpoint is returned as a tagged `bound`
value, never as a float infinity presented as a computed scalar. See
[docs/numerics.md](docs/numerics.md) for the promotion table, rounding modes,
and wire format.

## Modules

| Module | Scope | Example |
|---|---|---|
| `arithmetic` | Exact arithmetic, rounding modes, comparison, integer/rational operations | `arithmetic.quantize` |
| `scientific` | Trigonometry, exponentials/logarithms, roots, bracketed root finding, adaptive integration | `scientific.root_find` |
| `statistics` | Descriptive statistics, distributions, inference, power, stratified experiments | `statistics.stratified_experiment` |
| `finance` | Money, allocation, interest, amortization, NPV/XNPV, IRR/XIRR | `finance.xirr` |
| `units` | Versioned unit registry, dimensional algebra, conversions | `units.convert` |
| `linear_algebra` | Checked vectors/matrices, determinant, LU/QR, solving, least squares, eigen/SVD/PCA | `linear_algebra.solve` |
| `interval` | Heuristically padded binary64 intervals (not certified) | `interval.evaluate` |
| `optimize` | Exact rational LP with duals, Brent/golden minimization, Levenberg–Marquardt, assignment | `optimize.linear_program` |
| `algebra` | Polynomials, number theory, combinatorics | `algebra.factor` |
| `symbolic` | Differentiation, simplification, Taylor series | `symbolic.derivative` |
| `geometry` | Triangle solver, polygons, geodesy, quaternions | `geometry.triangle_solve` |
| `business` | Unit economics, CLV, elasticity, EOQ, queueing, cohorts | `business.eoq` |
| `verify` | Scope-stated claim checking with counterexamples | `verify.equality` |
| `format` | Markdown, LaTeX, CSV, and text rendering | `format.to_markdown` |
| `plan` | Deterministic method recommendations, checklists, unknowns | `plan.recommend` |

Generated reference documentation for every registered function (schemas,
domains, examples) lives in [`docs/methods/`](docs/methods/) and is produced
from the live registry by `bicmath docs`. Nothing in those files is
handwritten, so it cannot drift from runtime behavior.

### Workflow recipes

Four recipes compose plan, calculation, verification, and presentation into one
request (usable as `batch` nodes of type `recipe`):

| Recipe | What it returns |
|---|---|
| `ab_test_full` | Standardized experiment analysis, input-count reconciliation, interpretation notes, Markdown summary |
| `loan_compare` | Amortized offers, principal reconciliation, comparison table, rate/rounding conventions |
| `cohort_ltv` | Cohort retention and revenue with total reconciliation and a table |
| `budget_scenario` | Unit economics per volume plus exact whole-unit break-even coverage and minimality checks |

Each verification entry states the check, its status, what it **established**,
and what it did **not**. There is deliberately no generic `verified: true`.

### Verification, planning, and presentation

- `verify.*` checks exact identities, totals, inequalities, and matrix
  residuals, and every result carries a `scope` plus `scope_limitations`
  (for example, a finite grid search cannot establish a statement everywhere).
- `interval.*` returns heuristically padded enclosures labelled
  `assurance: "heuristic_padding_not_certified"` with an explicit warning; it
  is **not** a verified error bound. Operations that cannot be bounded
  (division by a zero-containing interval, `ln` at zero, `tan` across a pole)
  return structured errors.
- `plan.*` gives deterministic eligibility reasons and keeps missing evidence
  missing: randomization, independence, and comparability appear under
  `unknowns` unless explicitly attested.
- Budget presets (`fast`, `balanced`, `precise`) change working precision only.
  Iterative methods report their own requested tolerance, error estimate,
  residual, and convergence as separate fields.

## Configuration

Configuration files are JSON and reject unknown keys. Example:

```json
{
  "engine": {
    "disabled_modules": ["linear_algebra"],
    "default_mode": "auto",
    "precision": 34,
    "rounding": "half_even",
    "limits": {
      "max_array_len": 10000,
      "max_batch_nodes": 64,
      "request_timeout_ms": 10000
    }
  },
  "profile": "compact",
  "transport": {
    "bind": "127.0.0.1:8080",
    "max_in_flight": 4,
    "allowed_hosts": ["localhost", "127.0.0.1"],
    "allowed_origins": ["http://localhost:8080"]
  }
}
```

```sh
bicmath --config bicmath.json serve --transport stdio
bicmath config   # print the effective configuration
```

## Resource limits and privacy

Every calculation is treated as untrusted input. Defaults include 1 MiB request
bytes, 4096-digit literals, 8192-bit integers, 250,000 matrix elements, 256
batch nodes, 10M operations, and a 30s timeout. A caller may lower limits but
never raise them above server policy. CPU-heavy work runs on bounded blocking
workers, cancellation is cooperative and tested to release capacity, and there
is no outbound network access, host file access, or logging of raw financial
inputs by default. See [docs/limits.md](docs/limits.md) and
[SECURITY.md](SECURITY.md).

## Testing and verification

```sh
./scripts/check.sh          # fmt, clippy -D warnings, tests, feature matrix, docs, wasm smoke
./scripts/smoke-offline.sh  # release CLI offline smoke test
cargo test --workspace
cargo test -p bicmath --features http   # MCP stdio + Streamable HTTP interop
```

The suite includes hand-checkable and published reference values, property
tests (rational normalization, money conservation, matrix residuals, parser
round trips), a robustness corpus for malformed input, real MCP client sessions
over stdio and HTTP for both protocol lifecycles, native/WASM parity checks, and
a generic end-to-end business/statistics fixture. Every documented example is
executed by `bicmath doctor` and CI.

## Project layout

```
crates/bicmath-core            numeric contract, values, errors, limits, contracts, envelope
crates/bicmath-arithmetic      exact arithmetic module
crates/bicmath-scientific      elementary functions and numerical methods
crates/bicmath-statistics      descriptive statistics, distributions, inference, experiments
crates/bicmath-finance         money, cash flows, interest, amortization
crates/bicmath-units           unit catalog and dimensional algebra
crates/bicmath-linear-algebra  vectors, matrices, decompositions, solving
crates/bicmath-engine          registry, validation, expressions, batches, receipts
crates/bicmath-mcp             MCP transport and schema adapters
crates/bicmath                 native executable and MCP server
crates/bicmath-wasm            browser bindings
docs/                          architecture, numerics, limits, MCP, reproducibility, methods
examples/                      MCP config, WASM page/worker/smoke, Rust embedding
fuzz/                          libFuzzer targets (separate workspace)
scripts/                       check, offline smoke, wasm build, license report
```

## Documentation

- [Architecture and module-author guide](docs/architecture.md)
- [Expression grammar and batch references](docs/expression.md)
- [Numeric types, conversion, precision, and wire format](docs/numerics.md)
- [Resource limits, cancellation, and privacy](docs/limits.md)
- [MCP protocol/SDK compatibility and tool profiles](docs/mcp.md)
- [Reproducibility, receipts, and portability](docs/reproducibility.md)
- [Statistical interpretation guidance](docs/statistics.md)
- [Finance conventions](docs/finance.md)
- [Dependency rationale and licenses](docs/dependencies.md)
- [Benchmark methodology and measured results](docs/benchmarks.md)
- [Versioning and deprecation policy](docs/versioning.md)
- [Generated function reference](docs/methods/)
- [Notice and attribution](NOTICE)
- [Third-party license report](THIRD_PARTY_LICENSES.md)

## Feature profiles

Module selection is compile-time (Cargo features) and can be narrowed further
at runtime through configuration. Uncompiled or disabled functions are absent
from discovery and unavailable through every execution path.

```sh
# Minimal arithmetic only
cargo build -p bicmath-engine --no-default-features --features arithmetic

# Arithmetic, scientific, units
cargo build -p bicmath-engine --no-default-features --features arithmetic,scientific,units

# Business: statistics and finance
cargo build -p bicmath-engine --no-default-features --features arithmetic,statistics,finance

# Full build (default)
cargo build --workspace
```

## Status and limitations

This is a release candidate. See [CHANGELOG.md](CHANGELOG.md) for scope and
[docs/reproducibility.md](docs/reproducibility.md) for portability limits.

Known limits:

- Transcendental float64 results are not promised to agree bit-for-bit across
  platforms.
- Symbolic algebra, proofs, general equation solving, eigenvalue/SVD solvers,
  sparse solvers, GPU support, Bayesian inference, and persistent notebooks are
  out of scope for this release.
- Exact descriptive statistics are slower than float paths; measured numbers
  are in [docs/benchmarks.md](docs/benchmarks.md).
- The HTTP transport is behind the `http` Cargo feature.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Every new function needs a descriptor,
an executable example, and tests that can detect incorrect mathematics. Changes
to rounding, quantile, or standardization conventions are versioned
compatibility events.

## Security

See [SECURITY.md](SECURITY.md) for the threat model and reporting process.
Report suspected vulnerabilities privately.

## License

MIT. See [LICENSE](LICENSE). BicMath™ is a trademark of Nikolai Manek; the MIT
License does not grant trademark rights. Copyright and attribution details are
in [NOTICE](NOTICE), and third-party notices are in
[THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md).
