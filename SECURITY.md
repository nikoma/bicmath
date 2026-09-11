# Security policy

## Scope

BicMath is a local computation engine. It has no outbound network access from
numerical modules, no access to host files, environment secrets, or user
directories from expressions, and no arbitrary code execution. The expression
grammar is restricted and evaluated by the engine itself.

## Reporting a vulnerability

Report suspected vulnerabilities privately to the maintainers (for example
through the repository's private security advisory feature). Include:

- affected version or commit,
- a minimal request or reproduction,
- impact assessment (for example resource exhaustion, panic, or incorrect
  result),
- whether the issue requires an untrusted caller.

Do not open a public issue for an unpatched vulnerability.

## Threat model

Every calculation is treated as untrusted input, including requests from an
otherwise authorized agent.

- **Resource exhaustion**: bounded by request bytes, expression length/tokens,
  AST depth, digit/bit/scale limits, array/matrix sizes, batch nodes, output and
  trace bytes, exponents, iteration and operation budgets, timeouts, and
  in-flight limits. See [docs/limits.md](docs/limits.md).
- **Cancellation**: MCP cancellation is bridged to a cooperative engine token;
  worker capacity is released, which is tested directly.
- **Panics**: user-input paths avoid panics and unchecked indexing. A panicking
  worker is reported as an internal error; the engine does not continue with
  potentially corrupted state.
- **Memory safety**: no `unsafe` in project numerical/parser code.
- **Remote deployment**: HTTP binds to loopback by default and validates
  Host/Origin headers. Remote deployments must sit behind a trusted gateway or
  implement MCP authorization for the negotiated revision. A static bearer
  token is not automatically interoperable MCP OAuth.
- **Privacy**: logging defaults to operation metadata and timings without raw
  financial inputs or results; there are no persistent audit sinks or shared
  caches in this release.

## Cryptography

SHA-256 is used for deterministic fingerprints and receipts. Fingerprints are
not a correctness proof, not a security boundary, and not protection against
guessing low-entropy sensitive inputs.
