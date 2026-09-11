# Expression grammar and batch references

The expression engine evaluates its own restricted grammar. It never evaluates
Rust, JavaScript, Python, shell commands, imports, or remote code. Function
calls resolve through the same registry, schemas, and validation used by the
typed `calculate` path.

## Grammar

```text
expr        := or_expr
or_expr     := and_expr (('or' | '||') and_expr)*
and_expr    := not_expr (('and' | '&&') not_expr)*
not_expr    := ('not' | '!') not_expr | comparison
comparison  := additive (comp_op additive)+          // chained: 0 < x < 1
additive    := multiplicative (('+' | '-') multiplicative)*
multiplicative := unary (('*' | '/' | '%') unary)*
unary       := ('-' | '+') unary | power
power       := primary ('^' unary)?                  // right associative
primary     := NUMBER | STRING | 'true' | 'false' | IDENT
             | IDENT ('.' IDENT)+ '(' arguments? ')'
             | IDENT '(' arguments? ')'             // unqualified: parsed, rejected later
             | '(' expr ')' | '[' array ']' | '{' record '}'
arguments   := argument (',' argument)*
argument    := (IDENT '=')? expr
array       := expr (',' expr)* ','?
record      := (IDENT | STRING) ':' expr (',' ...)* ','?
```

Precedence, loosest to tightest: `or`, `and`, `not`, comparison, `+ -`,
`* / %`, unary sign, `^`. Exponentiation binds tighter than unary minus, so
`-2^2 == -4`; it is right associative, so `2^3^2 == 2^9`. Line comments (`//`)
and block comments (`/* */`) are supported.

Numeric literals parse directly into exact integers or decimals, never through
a floating-point parser. A literal with `.` or an exponent is a decimal; other
digits are integers. Rationals and float64 are only produced by explicit typed
values or functions.

## Bindings

`evaluate` accepts named bindings. Each binding is a typed wire value; plain
JSON integers and decimals are exact. Identifiers resolve only to those
bindings. There is no variable assignment, mutation, reflection, or
server-side state.

```json
{
  "expression": "a + b * 2",
  "bindings": { "a": "0.1", "b": { "kind": "integer", "value": "9007199254740993" } }
}
```

## Function calls

```text
finance.npv(
    rate = 0.08,
    cashflows = [-10000, 4000, 4000, 4000],
    currency = "USD",
    timing = "first_cashflow_at_t0"
)
```

Positional arguments are allowed before named arguments and are mapped to
parameters in declaration order. Duplicate, unknown, missing-required, and
out-of-domain arguments are rejected with structured errors. The engine
validates each argument against the function's real schema, not a generic
"anything goes" shape.

## Batch requests

A batch is a bounded dependency graph with uniquely named nodes:

```json
{
  "nodes": [
    { "type": "call", "name": "total",
      "function": "arithmetic.add", "arguments": { "a": 2, "b": 3 } },
    { "type": "call", "name": "double",
      "function": "arithmetic.mul", "arguments": { "a": { "$ref": "total" }, "b": 2 } },
    { "type": "evaluate", "name": "check", "expression": "double > 9" }
  ],
  "outputs": ["total", "double", "check"],
  "fail_fast": true
}
```

- References to earlier results use a dedicated representation:
  `{"$ref": "node_name"}` inside call arguments, or a bare identifier in an
  expression node.
- Duplicate names, invalid identifiers, missing references, dependency cycles,
  unknown outputs, and over-limit node counts are rejected before execution.
- Resolved values are validated again against the receiving function's schema.
- Execution order is a stable topological order. With `fail_fast` (the
  default), the first failing node stops the batch and remaining nodes are
  reported as `skipped`; set `"fail_fast": false` to run independent nodes and
  mark dependents skipped.
- Requested output ordering is preserved. All batch state is request-local:
  there is no hidden `last_result` and no cross-request variable store.
- Node results carry the same envelopes as `calculate`/`evaluate`; errors are
  structured and never carry a fabricated result.

Batches are bounded as a whole (`max_batch_nodes`, operation budgets, timeout),
not only per node.

## Replay

A receipt includes the effective request and method choices, not just a hash.
`bicmath replay --receipt receipt.json` re-executes the request and reports
whether the fingerprint matches. See
[reproducibility.md](reproducibility.md).
