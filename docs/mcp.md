# MCP protocol, SDK compatibility, and tool profiles

## SDK and protocol revisions

BicMath uses the official Rust SDK (`rmcp`) pinned to a stable release:
**rmcp 3.2.0**. The server advertises and tests these protocol revisions:

| Revision | Status | Lifecycle |
|---|---|---|
| `2026-07-28` | supported and tested | `server/discover` negotiation; per-request metadata |
| `2025-11-25` | supported and tested (legacy path) | `initialize` handshake + `notifications/initialized` |

`Cargo.lock` is committed. The end-to-end tests spawn the real binary and
exercise both lifecycles through a real MCP client
(`crates/bicmath/tests/mcp_stdio.rs`).

Other SDK revisions may work through the SDK's compatibility layer, but only
the two above are promised. Do not assume every revision uses the same startup
lifecycle or request metadata.

## Tool profiles

Both profiles are generated from the same registry and chosen at startup.

### Compact (default)

| Tool | Purpose |
|---|---|
| `list_modules` | Catalog of enabled modules and capabilities |
| `list_functions` | Filtered, paginated function summaries |
| `describe_function` | Full schema, conventions, domains, examples |
| `calculate` | Execute one qualified function with typed arguments |
| `evaluate` | Execute a restricted expression with named bindings |
| `batch` | Execute a bounded dependency graph |

The compact dispatcher validates arguments against the selected function's real
schema; it does not accept arbitrary JSON merely because its outer schema is
generic. A stable tool list plus function discovery is the product design: it
does not pretend to force clients to load schemas lazily.

### Expanded

`serve --profile expanded` exposes one individually typed MCP tool per enabled
function, named `module__function` (for example `statistics__median`), with the
input and output schemas generated from the descriptor. The tool list is built
once at startup and never changes as a side effect of a request.

## Results and errors

- Successful tools return structured content: the versioned result envelope
  with the typed result, exactness classification, effective context, warnings,
  assumptions, error estimates when a method can provide one, fingerprint, and
  optional receipt/trace.
- Tool-level failures (bad input, domain errors, disabled modules) return
  `isError: true` with a structured error body containing a stable `code`,
  message, input path, and details.
- Unknown tool names are protocol-level method-not-found errors.
- Pure calculation tools are annotated `readOnlyHint: true`,
  `destructiveHint: false`, `idempotentHint: true`, `openWorldHint: false`.
  Annotations are hints, not security enforcement.

## Transports

- **stdio** is the default. stdout carries protocol messages only; logs go to
  stderr.
- **Streamable HTTP** is optional behind the `http` Cargo feature. It binds to
  loopback by default, validates Host and Origin headers, enforces request-body
  limits, supports JSON responses for simple tools, and shuts down cleanly. A
  generic JSON HTTP endpoint is not marketed as MCP.

The server does not require sampling, an LLM connection, elicitation, cloud
credentials, or write access for pure calculations. Only implemented and tested
capabilities are advertised.

## Authorization boundary

MCP authorization, when needed, follows the negotiated revision's
specification. A static bearer token is not automatically interoperable MCP
OAuth. Remote deployments must sit behind a properly configured trusted gateway
that strips spoofed identity headers, and the gateway (not the engine) owns
authentication and authorization.
