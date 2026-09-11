# Symbolic module reference

An exact-first symbolic layer: differentiation, simplification, Taylor expansion, printing, substitution, and free-variable inspection over the restricted expression grammar.

- Module id: `symbolic`
- Version: 1.0.0
- Capabilities: differentiation, simplification, taylor_series, expression_printing, substitution, free_variables
- Supported modes: exact, auto, scientific
- Functions: 6

This file is generated from the live registry by `bicmath docs`; the function schemas, domains, and examples are the same ones the engine validates against at runtime.

<a id="derivative"></a>

## derivative

Differentiate a restricted expression with respect to one variable.

Applies the sum, product, quotient, power, and chain rules. Supported calls are sin, cos, tan, asin, acos, atan, sinh, cosh, tanh, exp, ln, log(value, base), log10, log2, sqrt, abs, and pow. `pi` and `e` are constants unless they are the differentiation variable. The result is simplified and printed in the restricted expression syntax.

- Module: `symbolic` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/symbolic.md#derivative
- Units/currency rule: The expression must be a dimensionless real-valued expression; variables carry no units.

## Parameters

- `expression` — Restricted numeric expression to differentiate. (restricted expression in x)
- `variable` — Variable to differentiate with respect to. (text)
- `order` (optional) — Derivative order; defaults to 1. Order 0 returns the simplified input. (integer)

## Output

The simplified derivative, printed with minimal parentheses.

## Examples

### first derivative of x squared

```json
{
  "expression": "x^2",
  "variable": "x"
}
```

Expected: `{"type":"value","value":"2 * x"}`

### unknown function is rejected

```json
{
  "expression": "nope(x)",
  "variable": "x"
}
```

Expected: `{"type":"error","code":"unknown_function"}`


<a id="free_variables"></a>

## free_variables

List the free variables of an expression.

Returns the sorted, unique variable names referenced by the expression. The constants `pi` and `e` are not variables and are excluded.

- Module: `symbolic` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/symbolic.md#free_variables
- Units/currency rule: The expression must be a dimensionless real-valued expression; variables carry no units.

## Parameters

- `expression` — Restricted expression to inspect. (restricted expression in x)

## Output

Sorted free variable names.

## Examples

### list variables

```json
{
  "expression": "x + y * 2 + pi"
}
```

Expected: `{"type":"value","value":["x","y"]}`


<a id="print"></a>

## print

Print a parsed expression with minimal parentheses.

The printer is precedence-aware: it emits exactly the parentheses required for the output to parse back to the same expression, including negative and rational literals.

- Module: `symbolic` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/symbolic.md#print
- Units/currency rule: The expression must be a dimensionless real-valued expression; variables carry no units.

## Parameters

- `expression` — Restricted expression to print. (restricted expression in x)

## Output

The expression printed with minimal parentheses.

## Examples

### minimal parentheses

```json
{
  "expression": "(x + 1) * 2"
}
```

Expected: `{"type":"value","value":"(x + 1) * 2"}`

### power binds tighter than unary minus

```json
{
  "expression": "-2^2"
}
```

Expected: `{"type":"value","value":"-2^2"}`


<a id="simplify"></a>

## simplify

Fold exact constants and eliminate arithmetic identities.

Only exact numeric operations are folded, so simplification never introduces a binary64 approximation. Identities include x + 0, x * 1, x * 0, x ^ 1, x ^ 0, x / 1, --x, x - x, and x / x for nonzero literals. Products and sums are normalized so that numeric coefficients are combined.

- Module: `symbolic` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/symbolic.md#simplify
- Units/currency rule: The expression must be a dimensionless real-valued expression; variables carry no units.

## Parameters

- `expression` — Restricted numeric expression to simplify. (restricted expression in x)

## Output

The simplified expression, printed with minimal parentheses.

## Examples

### identity elimination

```json
{
  "expression": "0*x + x*1 + 2 - 2"
}
```

Expected: `{"type":"value","value":"x"}`

### exact constant folding

```json
{
  "expression": "(2 + 3) * 4"
}
```

Expected: `{"type":"value","value":"20"}`


<a id="substitute"></a>

## substitute

Replace a variable with an expression.

Performs a structural substitution: every reference to the variable is replaced by the parsed replacement expression. The result is printed with minimal parentheses and is not simplified.

- Module: `symbolic` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/symbolic.md#substitute
- Units/currency rule: The expression must be a dimensionless real-valued expression; variables carry no units.

## Parameters

- `expression` — Restricted numeric expression. (restricted expression in x)
- `variable` — Variable to replace. (text)
- `replacement` — Expression substituted for the variable. (restricted expression in x)

## Output

The substituted expression, printed with minimal parentheses.

## Examples

### replace x with a + 1

```json
{
  "expression": "x^2 + y",
  "replacement": "a + 1",
  "variable": "x"
}
```

Expected: `{"type":"value","value":"(a + 1)^2 + y"}`


<a id="taylor"></a>

## taylor

Taylor coefficients f^(k)(at) / k! for k = 0..=order.

Each derivative is evaluated exactly first. When every coefficient has an exact rational value the result is exact; otherwise the expansion falls back to binary64 and is labelled approximate with a warning. In exact mode an approximate expansion is rejected.

- Module: `symbolic` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/symbolic.md#taylor
- Units/currency rule: The expression must be a dimensionless real-valued expression; variables carry no units.

## Parameters

- `expression` — Restricted numeric expression to expand. (restricted expression in x)
- `variable` — Variable the expansion is taken in. (text)
- `at` — Expansion point. (number)
- `order` — Highest derivative order; must be a non-negative integer. (integer)

## Output

Polynomial text, coefficient array, order, expansion point, method, and the approximate flag.

## Examples

### exponential series at zero

```json
{
  "at": {
    "kind": "integer",
    "value": "0"
  },
  "expression": "exp(x)",
  "order": {
    "kind": "integer",
    "value": "4"
  },
  "variable": "x"
}
```

Expected: `{"type":"value","value":{"approximate":false,"at":{"kind":"integer","value":"0"},"coefficients":[{"kind":"integer","value":"1"},{"kind":"integer","value":"1"},{"kind":"rational","numerator":"1","denominator":"2"},{"kind":"rational","numerator":"1","denominator":"6"},{"kind":"rational","numerator":"1","denominator":"24"}],"method":"taylor","order":{"kind":"integer","value":"4"},"polynomial":"1 + x + 1/2 * x^2 + 1/6 * x^3 + 1/24 * x^4"}}`


