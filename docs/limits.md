# Resource limits, cancellation, and privacy

Every calculation is treated as untrusted input, including requests from an
otherwise authorized agent. Limits are applied before parsing/allocating where
possible and throughout execution.

## Default limits

| Limit | Default |
|---|---|
| Request bytes | 1 MiB |
| Expression length / tokens / AST depth | 16 KiB / 4096 / 64 |
| Digits per literal | 4096 |
| Integer bits | 8192 |
| Decimal scale | 4096 |
| Array length | 100,000 |
| Matrix elements | 250,000 |
| Batch nodes | 256 |
| Output bytes | 4 MiB |
| Trace bytes | 256 KiB |
| Exponent | 100,000 |
| Factorial argument | 10,000 |
| Iterations | 1,000,000 |
| Operations | 10,000,000 |
| Nested value depth | 64 |
| String length | 1 MiB |
| Request timeout | 30,000 ms |

Operators can lower these in configuration. A caller may request smaller limits
through the request `context.limits` object; requests above server policy are
rejected with `resource_limit`. Requested `precision` must be in `1..=1000`.

Budget presets (`fast`, `balanced`, `precise`) select working precision only.
They never change method defaults or tolerances; iterative methods expose their
requested tolerance, error estimate, residual, and convergence separately.

## Where limits are checked

- Before parsing: request bytes, expression length, token count.
- During parsing: AST depth, array literal length, literal digits.
- Before allocating: array/matrix shapes, predicted result sizes, operation
  counts.
- During execution: operation budgets, iteration budgets, cancellation and
  deadline checks inside loops.
- After execution: output bytes and trace bytes.

A small expression such as a huge power is rejected by a predicted-size check,
not by attempting the allocation.

## Cancellation and concurrency

- CPU-heavy work runs on bounded blocking workers, not on async I/O threads.
- MCP cancellation is bridged to the engine's cooperative token; long loops
  call `ctx.check()` and stop promptly. Worker capacity is released after
  cancellation, and this is tested directly.
- A timeout on an awaited blocking worker does not terminate the worker;
  therefore correctness relies on cooperative checks plus strict prechecks for
  non-interruptible operations. The engine never claims a hard CPU or memory
  cap that is not enforced.
- The server enforces a maximum number of in-flight calculations; excess
  requests wait for a bounded period and then receive `resource_limit`.

## Privacy defaults

- No outbound network access from numerical modules.
- No access to host files, environment secrets, or user directories from
  expressions.
- Logging defaults to operation metadata and timings, never raw financial
  inputs or results.
- Receipts are returned to the caller only when requested; there is no
  persistent audit sink or dataset store.
- There are no shared caches of sensitive results in this release.

## Memory safety

Project numerical and parser code contains no `unsafe`. User-input paths avoid
panics and unchecked indexing. A worker that panics is reported as an internal
error; the engine does not catch a panic and continue with potentially
corrupted logical state.
