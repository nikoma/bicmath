# Verify module reference

Independent checks for agent claims using exact arithmetic.

- Module id: `verify`
- Version: 1.0.0
- Capabilities: exact_arithmetic, claim_verification, counterexample_search, matrix_residual
- Supported modes: exact, auto, scientific
- Functions: 7

This file is generated from the live registry by `bicmath docs`; the function schemas, domains, and examples are the same ones the engine validates against at runtime.

<a id="equality"></a>

## equality

Check whether two restricted arithmetic expressions are exactly equal.

Both expressions are parsed and evaluated with exact integer, rational, and decimal arithmetic; binary64 is never used. Without a tolerance the check is exact equality. With a non-negative exact tolerance the claim is confirmed when |left - right| is at most the tolerance. A refutation includes the witness bindings. Scope: the result is exact at the supplied bindings and does not prove the identity for all inputs.

- Module: `verify` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/verify.md#equality
- Units/currency rule: Operands must be dimensionless exact numbers.

## Parameters

- `left` — Left-hand restricted arithmetic expression. (restricted expression in )
- `right` — Right-hand restricted arithmetic expression. (restricted expression in )
- `bindings` (optional) — Record mapping free variables to exact numbers (canonical values or numeric shorthand strings). Float64 values are rejected. (record with fields )
- `tolerance` (optional) — Non-negative exact tolerance for the equality comparison. (exact number (integer, rational, or decimal))

## Output

Verification record with status, evaluated operands, exact difference, and method.

## Examples

### decimal addition is exact

```json
{
  "left": "0.1 + 0.2",
  "right": "0.3"
}
```

Expected: `{"type":"value","value":{"difference":{"kind":"integer","value":"0"},"left":{"kind":"decimal","value":"0.3"},"method":"exact_arithmetic","right":{"kind":"decimal","value":"0.3"},"scope":"exact_at_supplied_bindings","scope_limitations":["confirms the identity at the supplied bindings; it does not prove it for all inputs"],"status":"confirmed","tolerance_used":null}}`

### repeating fraction differs from its truncation

```json
{
  "left": "1/3",
  "right": "0.333"
}
```

Expected: `{"type":"value","value":{"bindings":{},"difference":{"kind":"rational","numerator":"1","denominator":"3000"},"left":{"kind":"rational","numerator":"1","denominator":"3"},"method":"exact_arithmetic","right":{"kind":"decimal","value":"0.333"},"scope":"exact_at_supplied_bindings","scope_limitations":["confirms the identity at the supplied bindings; it does not prove it for all inputs"],"status":"refuted","tolerance_used":null}}`

### tolerance confirms a rounded claim

```json
{
  "left": "1/3",
  "right": "0.333",
  "tolerance": "0.001"
}
```

Expected: `{"type":"value","value":{"difference":{"kind":"rational","numerator":"1","denominator":"3000"},"left":{"kind":"rational","numerator":"1","denominator":"3"},"method":"exact_arithmetic","right":{"kind":"decimal","value":"0.333"},"scope":"exact_at_supplied_bindings","scope_limitations":["confirms the identity at the supplied bindings; it does not prove it for all inputs"],"status":"confirmed","tolerance_used":{"kind":"decimal","value":"0.001"}}}`

### unknown function is rejected

```json
{
  "left": "nope(1)",
  "right": "1"
}
```

Expected: `{"type":"error","code":"unknown_function"}`


<a id="expression"></a>

## expression

Compare one restricted arithmetic expression to an exact value.

The expression is evaluated with exact integer, rational, and decimal arithmetic and compared to the claimed value under the requested relation. A refutation includes the witness bindings. Scope: the result is exact at the supplied bindings and does not prove the relation for all inputs.

- Module: `verify` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/verify.md#expression
- Units/currency rule: The expression must produce a dimensionless exact number.

## Parameters

- `expression` — Restricted arithmetic expression to evaluate. (restricted expression in )
- `relation` — Relation to check: lt, le, gt, ge, eq, or ne. (one of ["lt", "le", "gt", "ge", "eq", "ne"])
- `value` — Exact value to compare against. (exact number (integer, rational, or decimal))
- `bindings` (optional) — Record mapping free variables to exact numbers (canonical values or numeric shorthand strings). Float64 values are rejected. (record with fields )

## Output

Verification record with status, evaluated expression, claimed value, and exact difference.

## Examples

### expression equals its claimed value

```json
{
  "expression": "2^10",
  "relation": "eq",
  "value": {
    "kind": "integer",
    "value": "1024"
  }
}
```

Expected: `{"type":"value","value":{"difference":{"kind":"integer","value":"0"},"left":{"kind":"integer","value":"1024"},"method":"exact_arithmetic","relation":"eq","right":{"kind":"integer","value":"1024"},"status":"confirmed"}}`

### bindings resolve free variables

```json
{
  "bindings": {
    "x": {
      "kind": "integer",
      "value": "2"
    }
  },
  "expression": "x + 1",
  "relation": "eq",
  "value": {
    "kind": "integer",
    "value": "3"
  }
}
```

Expected: `{"type":"value","value":{"difference":{"kind":"integer","value":"0"},"left":{"kind":"integer","value":"3"},"method":"exact_arithmetic","relation":"eq","right":{"kind":"integer","value":"3"},"status":"confirmed"}}`

### float64 bindings are rejected

```json
{
  "bindings": {
    "x": {
      "kind": "float64",
      "value": "0.5"
    }
  },
  "expression": "x",
  "relation": "eq",
  "value": {
    "kind": "integer",
    "value": "1"
  }
}
```

Expected: `{"type":"error","code":"unsupported_numeric_mode"}`


<a id="inequality"></a>

## inequality

Check a relation between two restricted arithmetic expressions exactly.

Both expressions are evaluated with exact integer, rational, and decimal arithmetic. The relation is one of lt, le, gt, ge, eq, ne. A tolerance is accepted only for eq and ne, where it is interpreted as |left - right| <= tolerance (eq) or its negation (ne). A refutation includes the witness bindings. Scope: the result is exact at the supplied bindings and does not prove the relation for all inputs.

- Module: `verify` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/verify.md#inequality
- Units/currency rule: Operands must be dimensionless exact numbers.

## Parameters

- `left` — Left-hand restricted arithmetic expression. (restricted expression in )
- `right` — Right-hand restricted arithmetic expression. (restricted expression in )
- `relation` — Relation to check: lt, le, gt, ge, eq, or ne. (one of ["lt", "le", "gt", "ge", "eq", "ne"])
- `bindings` (optional) — Record mapping free variables to exact numbers (canonical values or numeric shorthand strings). Float64 values are rejected. (record with fields )
- `tolerance` (optional) — Non-negative exact tolerance; only supported for eq and ne. (exact number (integer, rational, or decimal))

## Output

Verification record with status, evaluated operands, relation, and exact difference.

## Examples

### one half is less than two thirds

```json
{
  "left": "1/2",
  "relation": "lt",
  "right": "2/3"
}
```

Expected: `{"type":"value","value":{"difference":{"kind":"rational","numerator":"-1","denominator":"6"},"left":{"kind":"rational","numerator":"1","denominator":"2"},"method":"exact_arithmetic","relation":"lt","right":{"kind":"rational","numerator":"2","denominator":"3"},"scope":"exact_at_supplied_bindings","scope_limitations":["confirms the identity at the supplied bindings; it does not prove it for all inputs"],"status":"confirmed"}}`

### a negative tolerance is rejected

```json
{
  "left": "1",
  "relation": "eq",
  "right": "1",
  "tolerance": "-1"
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="inequality_grid"></a>

## inequality_grid

Search a finite, evenly spaced grid for a counterexample to a relation between two expressions.

This is a search, not a proof. Both expressions are evaluated at up to 10000 evenly spaced exact points in [lower, upper]. A refutation reports the first counterexample; a confirmed result means only that the relation held at every sampled point and carries a warning.

- Module: `verify` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/verify.md#inequality_grid
- Units/currency rule: Both expressions must produce dimensionless exact numbers.

## Parameters

- `left` — Left-hand restricted arithmetic expression. (restricted expression in )
- `right` — Right-hand restricted arithmetic expression. (restricted expression in )
- `relation` — Relation to check: lt, le, gt, ge, eq, or ne. (one of ["lt", "le", "gt", "ge", "eq", "ne"])
- `variable` — Name of the single variable sampled over the interval. (text)
- `lower` — Inclusive lower endpoint. (exact number (integer, rational, or decimal))
- `upper` — Inclusive upper endpoint. (exact number (integer, rational, or decimal))
- `samples` — Number of evenly spaced sample points, at least 2 and at most 10000. (integer)

## Output

Search record with status, number of points checked, and the first counterexample if any.

## Examples

### grid confirms a positive quadratic

```json
{
  "left": "x^2 + 1",
  "lower": {
    "kind": "integer",
    "value": "-2"
  },
  "relation": "gt",
  "right": "0",
  "samples": {
    "kind": "integer",
    "value": "5"
  },
  "upper": {
    "kind": "integer",
    "value": "2"
  },
  "variable": "x"
}
```

Expected: `{"type":"value","value":{"checked_points":{"kind":"integer","value":"5"},"counterexample":null,"method":"grid_search","scope":"finite_grid_search","scope_limitations":["a finite grid cannot establish that a statement holds everywhere; confirmed means only that no counterexample was found on the sampled points"],"status":"confirmed"}}`

### grid finds a counterexample

```json
{
  "left": "x^2",
  "lower": {
    "kind": "integer",
    "value": "0"
  },
  "relation": "ge",
  "right": "x",
  "samples": {
    "kind": "integer",
    "value": "3"
  },
  "upper": {
    "kind": "integer",
    "value": "1"
  },
  "variable": "x"
}
```

Expected: `{"type":"value","value":{"checked_points":{"kind":"integer","value":"2"},"counterexample":{"left":{"kind":"rational","numerator":"1","denominator":"4"},"right":{"kind":"rational","numerator":"1","denominator":"2"},"value":{"kind":"rational","numerator":"1","denominator":"2"}},"method":"grid_search","scope":"finite_grid_search","scope_limitations":["a finite grid cannot establish that a statement holds everywhere; confirmed means only that no counterexample was found on the sampled points"],"status":"refuted"}}`


<a id="matrix_residual"></a>

## matrix_residual

Compute the exact infinity norm of A*x - b for a claimed solution.

All matrix and vector entries must be exact numbers. The residual is computed with rational arithmetic and the infinity norm is returned exactly. A zero norm confirms the claimed solution exactly; a refutation also reports the residual vector.

- Module: `verify` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Quadratic
- Determinism: Deterministic
- Method reference: docs/methods/verify.md#matrix_residual
- Units/currency rule: All matrix and vector entries must be dimensionless exact numbers.

## Parameters

- `matrix` — Coefficient matrix A. (matrix)
- `vector` — Right-hand side b as an array of exact numbers. (array of exact number (integer, rational, or decimal))
- `claimed_solution` — Claimed solution x as an array of exact numbers. (array of exact number (integer, rational, or decimal))

## Output

Verification record with status, exact residual infinity norm, and method.

## Examples

### identity system has zero residual

```json
{
  "claimed_solution": [
    {
      "kind": "integer",
      "value": "1"
    },
    {
      "kind": "integer",
      "value": "2"
    }
  ],
  "matrix": {
    "kind": "matrix",
    "rows": 2,
    "cols": 2,
    "data": [
      {
        "kind": "integer",
        "value": "1"
      },
      {
        "kind": "integer",
        "value": "0"
      },
      {
        "kind": "integer",
        "value": "0"
      },
      {
        "kind": "integer",
        "value": "1"
      }
    ]
  },
  "vector": [
    {
      "kind": "integer",
      "value": "1"
    },
    {
      "kind": "integer",
      "value": "2"
    }
  ]
}
```

Expected: `{"type":"value","value":{"method":"exact_matrix_residual","residual_norm":{"kind":"integer","value":"0"},"status":"confirmed"}}`

### a wrong solution has a nonzero residual

```json
{
  "claimed_solution": [
    {
      "kind": "integer",
      "value": "1"
    },
    {
      "kind": "integer",
      "value": "3"
    }
  ],
  "matrix": {
    "kind": "matrix",
    "rows": 2,
    "cols": 2,
    "data": [
      {
        "kind": "integer",
        "value": "1"
      },
      {
        "kind": "integer",
        "value": "0"
      },
      {
        "kind": "integer",
        "value": "0"
      },
      {
        "kind": "integer",
        "value": "1"
      }
    ]
  },
  "vector": [
    {
      "kind": "integer",
      "value": "1"
    },
    {
      "kind": "integer",
      "value": "2"
    }
  ]
}
```

Expected: `{"type":"value","value":{"method":"exact_matrix_residual","residual":[{"kind":"integer","value":"0"},{"kind":"integer","value":"1"}],"residual_norm":{"kind":"integer","value":"1"},"status":"refuted"}}`


<a id="probability"></a>

## probability

Check that a probability lies within [0, 1] or supplied bounds.

Exact inputs are compared exactly. Float64 inputs are accepted here and the result is labelled approximate; the comparison still uses the exact binary value. Bounds are inclusive and default to [0, 1].

- Module: `verify` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/verify.md#probability
- Units/currency rule: Bounds and value must be dimensionless numbers.

## Parameters

- `value` — Probability value to check; may be exact or float64. (number)
- `lower` (optional) — Inclusive lower bound; defaults to 0. (number)
- `upper` (optional) — Inclusive upper bound; defaults to 1. (number)

## Output

Range-check record with the checked value, bounds, and approximation flag.

## Examples

### a decimal probability is in range

```json
{
  "value": "0.5"
}
```

Expected: `{"type":"value","value":{"approximate":false,"lower":{"kind":"integer","value":"0"},"method":"probability_bounds","status":"confirmed","upper":{"kind":"integer","value":"1"},"value":{"kind":"decimal","value":"0.5"}}}`

### an out-of-range probability is refuted

```json
{
  "value": "1.5"
}
```

Expected: `{"type":"value","value":{"approximate":false,"lower":{"kind":"integer","value":"0"},"method":"probability_bounds","status":"refuted","upper":{"kind":"integer","value":"1"},"value":{"kind":"decimal","value":"1.5"}}}`

### a float64 probability is labelled approximate

```json
{
  "value": {
    "kind": "float64",
    "value": "0.5"
  }
}
```

Expected: `{"type":"value","value":{"approximate":true,"lower":{"kind":"integer","value":"0"},"method":"probability_bounds","status":"confirmed","upper":{"kind":"integer","value":"1"},"value":{"kind":"float64","value":"0.5"}}}`


<a id="total"></a>

## total

Compare an exact sum of exact numbers to a claimed total.

The values are summed exactly (decimal arithmetic when every value is an integer or decimal, rational arithmetic otherwise) and compared to the claimed total. A mismatch reports the exact difference.

- Module: `verify` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/verify.md#total
- Units/currency rule: All values must be dimensionless exact numbers.

## Parameters

- `values` — Array of exact numbers to sum. (array of exact number (integer, rational, or decimal))
- `claimed_total` — Claimed exact total. (exact number (integer, rational, or decimal))

## Output

Verification record with status, computed sum, claimed total, and exact difference.

## Examples

### exact total

```json
{
  "claimed_total": {
    "kind": "decimal",
    "value": "0.3"
  },
  "values": [
    {
      "kind": "decimal",
      "value": "0.1"
    },
    {
      "kind": "decimal",
      "value": "0.2"
    }
  ]
}
```

Expected: `{"type":"value","value":{"claimed":{"kind":"decimal","value":"0.3"},"computed":{"kind":"decimal","value":"0.3"},"difference":{"kind":"integer","value":"0"},"method":"exact_sum","status":"confirmed"}}`

### total mismatch

```json
{
  "claimed_total": {
    "kind": "decimal",
    "value": "0.7"
  },
  "values": [
    {
      "kind": "decimal",
      "value": "0.1"
    },
    {
      "kind": "decimal",
      "value": "0.2"
    },
    {
      "kind": "decimal",
      "value": "0.3"
    }
  ]
}
```

Expected: `{"type":"value","value":{"claimed":{"kind":"decimal","value":"0.7"},"computed":{"kind":"decimal","value":"0.6"},"difference":{"kind":"rational","numerator":"-1","denominator":"10"},"method":"exact_sum","status":"refuted"}}`


