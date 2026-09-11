# Contributing

Thanks for considering a contribution. BicMath values evidence over breadth:
every registered function must have a real implementation, a documented domain,
executable examples, and tests that can detect incorrect mathematics.

## Getting started

```sh
git clone <repository>
cd bicmath
cargo build --workspace
cargo test --workspace
./scripts/check.sh          # fmt + clippy + tests + feature matrix + wasm smoke
```

The pinned toolchain is in `rust-toolchain.toml` (Rust 1.96.0, MSRV 1.88).

## Adding a module or function

Read [docs/architecture.md](docs/architecture.md#module-author-guide) for the
module contract. In short:

1. Implement functions as plain functions plus a `FunctionDescriptor`, wrapped
   by `SimpleFunction::arc`.
2. Return a `Module` from your crate.
3. Register it with `RegistryBuilder::add_module` in the application. No
   expression-parser or MCP changes are needed.
4. Every function needs at least one executable example with an expected value
   or error; `Engine::validate_examples` runs them and CI fails on any failure.

Checklist:

- Stable qualified id and version.
- Typed parameter schemas, required/optional flags, descriptions, domains.
- Output schema and description.
- Supported numeric modes, purity, determinism, cost class.
- Method reference, units/currency rule.
- Tests with hand-checkable or published reference values.
- Domain errors with the right `ErrorCode`; no panics or `unwrap` on user input.
- No filesystem, network, clock, or randomness; call `ctx.check()` in loops.

## Semantic changes are compatibility events

Changing a rounding default, quantile convention, promotion rule, or
standardization definition can alter downstream financial or statistical
results even when Rust types are unchanged. Bump
`bicmath_core::NUMERICAL_POLICY_VERSION` and document the change in
`CHANGELOG.md`.

## Tests

- Prefer tests that can detect incorrect mathematics, not tests that mirror the
  implementation.
- Property tests for algebraic identities (rational normalization, money
  conservation, dimensions, matrix residuals, parser round trips).
- Differential tests against independent references or published values, with
  provenance comments. Never derive the expected value from the same code under
  test.
- Fuzz number parsing, expressions, schemas, batch references, and malformed
  protocol requests.
- Record a regression test for every confirmed defect.

## Code style

- `cargo fmt` and `cargo clippy --workspace --all-targets -- -D warnings` must
  pass.
- No `unsafe` in project numerical/parser code unless a measured need is
  documented and isolated with tests.
- Keep transport, async, filesystem, clock, and network code out of the
  numerical core.
- Do not add a dependency without a rationale in `docs/dependencies.md`.

## Reporting issues

Include the request JSON, the observed envelope or error, and the fingerprint
when available. For numerical issues, include the method reference and a
hand-checkable or published reference value.
