# Reproducibility, receipts, and portability

## Deterministic fingerprints

Every result envelope carries a SHA-256 fingerprint over canonical JSON that
covers:

- canonical inputs and the expression or batch graph,
- method and module versions,
- the effective numeric context and any conversions,
- an explicitly supplied seed (when one exists),
- semantic policy versions (wire schema and numerical policy).

It deliberately excludes wall-clock timestamps, elapsed time, request ids, and
tracing noise. If a resource policy changes an algorithm or its result, that
policy is included. Test vectors for canonical encoding and fingerprints live
in `bicmath-core`.

A hash is not a correctness proof and is not protection against guessing
low-entropy sensitive inputs.

## Replay receipts

A receipt contains the effective request and method choices, not just the hash:

```json
{
  "schema_version": 1,
  "fingerprint": "…",
  "request": { "function": "arithmetic.add", "arguments": { "a": "0.1", "b": "0.2" } },
  "function": { "id": "arithmetic.add", "version": "1.0.0" },
  "engine": { "name": "bicmath", "version": "0.1.0", "modules": { "…": "1.0.0" } },
  "context": { "numeric": { "mode": "auto", "precision": 34, "rounding": "half_even" }, "…": "…" },
  "policy_version": 1
}
```

Request a receipt with `"receipt": true` on `calculate`, `evaluate`, or
`batch`, then replay it:

```sh
bicmath replay --receipt receipt.json
```

The command re-executes the request and reports whether the fingerprint
matches. A mismatch means the request, effective context, or module versions
changed, and the warning says so.

## Reproducibility levels

| Level | Meaning | Examples |
|---|---|---|
| Exact | reproducible across supported platforms for the same module versions | integer, rational, and decimal arithmetic and rounding |
| Bounded | declared tolerances and tested method bounds | bracketed root finding, adaptive integration, iterative solvers |
| Platform-dependent | binary64 transcendental results may differ in the last bits across libm implementations | `scientific.sin`, `exp`, `ln`, distribution functions |

Exact calculations are reproducible across supported platforms. Scientific
results are not promised to agree bit-for-bit for transcendental functions;
native/WASM parity tests use declared tolerances for approximate methods and
exact equality for exact operations.

## Platform support

- **Native**: Linux, macOS, and Windows are supported by the chosen
  dependencies and are exercised by the CI build matrix. A platform is only
  claimed as *tested* when the matrix actually runs its tests.
- **WASM**: `wasm32-unknown-unknown` with `wasm-bindgen`. All modules are
  compiled into the browser build and verified by the Node smoke test and the
  browser example. WASM has no portable monotonic clock through `std`, so
  wall-clock deadlines are disabled there; operation budgets and cooperative
  cancellation still bound work.
- Published binaries identify their source revision and engine version.
