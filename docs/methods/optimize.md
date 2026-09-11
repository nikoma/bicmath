# Optimization module reference

Exact rational linear programming, bounded univariate minimization, nonlinear least squares, and exact linear assignment.

- Module id: `optimize`
- Version: 1.0.0
- Capabilities: linear_programming, univariate_minimization, nonlinear_least_squares, linear_assignment
- Supported modes: exact, auto, scientific
- Functions: 5

This file is generated from the live registry by `bicmath docs`; the function schemas, domains, and examples are the same ones the engine validates against at runtime.

<a id="golden_section"></a>

## golden_section

Minimize a scalar expression on a closed interval by golden-section search.

Golden-section search shrinks the bracket by the golden ratio until the interval is smaller than the tolerance, evaluating one new point per iteration. It is slower than Brent's method but has no interpolation assumptions and never fails to make progress. The same restricted expression grammar and function set as optimize.minimize_1d applies.

- Module: `optimize` (version 1.0.0)
- Modes: scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/optimize.md#golden_section

## Parameters

- `expression` — Restricted expression in the variable. (restricted expression in x)
- `variable` — Name of the single variable used by the expression. (text)
- `lower` — Lower bracket endpoint. (number)
- `upper` — Upper bracket endpoint. (number)
- `tolerance` (optional) — Absolute x tolerance; default 1e-10. (number)

## Output

Minimizer, value, iteration and evaluation counts, convergence flag, and method.

## Examples

### minimize x^2 - 4x + 7 on [0, 5]

```json
{
  "expression": "x^2 - 4*x + 7",
  "lower": {
    "kind": "integer",
    "value": "0"
  },
  "upper": {
    "kind": "integer",
    "value": "5"
  },
  "variable": "x"
}
```

Expected: `{"type":"value","value":{"converged":true,"evaluations":{"kind":"integer","value":"51"},"iterations":{"kind":"integer","value":"49"},"method":"golden_section","value":{"kind":"float64","value":"3"},"x":{"kind":"float64","value":"2.000000020991479"}}}`


<a id="linear_assignment"></a>

## linear_assignment

Solve the square linear assignment problem exactly with the Hungarian algorithm.

Finds the minimum-cost perfect matching of a square cost matrix using the exact rational Hungarian algorithm (shortest augmenting paths with potentials). The cost matrix must be square; a non-square matrix is rejected. Costs may be any exact numbers, including negative values, and the total cost is returned exactly.

- Module: `optimize` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Cubic
- Determinism: Deterministic
- Method reference: docs/methods/optimize.md#linear_assignment

## Parameters

- `cost_matrix` — Square matrix of exact costs. (array of array of exact number (integer, rational, or decimal))

## Output

Optimal row-to-column assignment, exact total cost, and method.

## Examples

### 3x3 assignment

```json
{
  "cost_matrix": [
    [
      {
        "kind": "integer",
        "value": "4"
      },
      {
        "kind": "integer",
        "value": "1"
      },
      {
        "kind": "integer",
        "value": "3"
      }
    ],
    [
      {
        "kind": "integer",
        "value": "2"
      },
      {
        "kind": "integer",
        "value": "0"
      },
      {
        "kind": "integer",
        "value": "5"
      }
    ],
    [
      {
        "kind": "integer",
        "value": "3"
      },
      {
        "kind": "integer",
        "value": "2"
      },
      {
        "kind": "integer",
        "value": "2"
      }
    ]
  ]
}
```

Expected: `{"type":"value","value":{"assignment":[{"kind":"integer","value":"1"},{"kind":"integer","value":"0"},{"kind":"integer","value":"2"}],"method":"hungarian","total_cost":{"kind":"integer","value":"5"}}}`


<a id="linear_program"></a>

## linear_program

Solve a linear program exactly with a two-phase rational simplex.

Maximizes (or minimizes, with sense "min") c^T x subject to linear constraints with relations "le", "ge", or "eq" and x >= 0. All arithmetic is exact over rationals and pivots follow Bland's rule, so the method is deterministic and cannot cycle. The result reports the exact objective, primal solution, constraint duals, and reduced costs. Infeasible and unbounded problems return the corresponding status without fabricating a solution; dual and reduced-cost sign conventions follow the declared sense.

- Module: `optimize` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/optimize.md#linear_program

## Parameters

- `objective` — Objective coefficients, one per decision variable. (array of exact number (integer, rational, or decimal))
- `constraints` — Constraint rows. (array of record with fields coefficients, relation, rhs)
- `sense` (optional) — Optimization sense: "max" (default) or "min". (one of ["max", "min"])
- `variable_names` (optional) — Optional names; when supplied, x is a record keyed by name. (array of text)

## Output

Status, exact objective and solution, duals, reduced costs, pivot count, and method.

## Examples

### maximize 3x + 5y

```json
{
  "constraints": [
    {
      "coefficients": [
        {
          "kind": "integer",
          "value": "1"
        },
        {
          "kind": "integer",
          "value": "0"
        }
      ],
      "relation": "le",
      "rhs": {
        "kind": "integer",
        "value": "4"
      }
    },
    {
      "coefficients": [
        {
          "kind": "integer",
          "value": "0"
        },
        {
          "kind": "integer",
          "value": "2"
        }
      ],
      "relation": "le",
      "rhs": {
        "kind": "integer",
        "value": "12"
      }
    },
    {
      "coefficients": [
        {
          "kind": "integer",
          "value": "3"
        },
        {
          "kind": "integer",
          "value": "2"
        }
      ],
      "relation": "le",
      "rhs": {
        "kind": "integer",
        "value": "18"
      }
    }
  ],
  "objective": [
    {
      "kind": "integer",
      "value": "3"
    },
    {
      "kind": "integer",
      "value": "5"
    }
  ]
}
```

Expected: `{"type":"value","value":{"dual":[{"kind":"integer","value":"0"},{"kind":"rational","numerator":"3","denominator":"2"},{"kind":"integer","value":"1"}],"iterations":{"kind":"integer","value":"3"},"method":"rational_simplex_bland","objective":{"kind":"integer","value":"36"},"reduced_costs":[{"kind":"integer","value":"0"},{"kind":"integer","value":"0"}],"status":"optimal","x":[{"kind":"integer","value":"2"},{"kind":"integer","value":"6"}]}}`

### infeasible system

```json
{
  "constraints": [
    {
      "coefficients": [
        {
          "kind": "integer",
          "value": "1"
        },
        {
          "kind": "integer",
          "value": "1"
        }
      ],
      "relation": "le",
      "rhs": {
        "kind": "integer",
        "value": "1"
      }
    },
    {
      "coefficients": [
        {
          "kind": "integer",
          "value": "1"
        },
        {
          "kind": "integer",
          "value": "1"
        }
      ],
      "relation": "ge",
      "rhs": {
        "kind": "integer",
        "value": "3"
      }
    }
  ],
  "objective": [
    {
      "kind": "integer",
      "value": "1"
    },
    {
      "kind": "integer",
      "value": "1"
    }
  ]
}
```

Expected: `{"type":"value","value":{"iterations":{"kind":"integer","value":"1"},"method":"rational_simplex_bland","status":"infeasible"}}`

### unbounded direction

```json
{
  "constraints": [
    {
      "coefficients": [
        {
          "kind": "integer",
          "value": "1"
        },
        {
          "kind": "integer",
          "value": "0"
        }
      ],
      "relation": "le",
      "rhs": {
        "kind": "integer",
        "value": "1"
      }
    }
  ],
  "objective": [
    {
      "kind": "integer",
      "value": "1"
    },
    {
      "kind": "integer",
      "value": "1"
    }
  ]
}
```

Expected: `{"type":"value","value":{"iterations":{"kind":"integer","value":"1"},"method":"rational_simplex_bland","status":"unbounded"}}`


<a id="minimize_1d"></a>

## minimize_1d

Minimize a scalar expression on a closed interval with Brent's method.

Brent's method combines inverse parabolic interpolation with golden-section safeguarding on the closed interval [lower, upper]. The expression is a restricted string using the named variable, numeric literals, parentheses, + - * / % ^, and the functions sin, cos, tan, asin, acos, atan, sinh, cosh, tanh, exp, ln, log10, log2, sqrt, abs, pow, floor, ceil, min, and max. Non-convergence within the context iteration budget is reported as an error; the last iterate is never returned as a success.

- Module: `optimize` (version 1.0.0)
- Modes: scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/optimize.md#minimize_1d

## Parameters

- `expression` — Restricted expression in the variable. (restricted expression in x)
- `variable` — Name of the single variable used by the expression. (text)
- `lower` — Lower bracket endpoint. (number)
- `upper` — Upper bracket endpoint. (number)
- `tolerance` (optional) — Absolute x tolerance; default 1e-10. (number)

## Output

Minimizer, value, iteration and evaluation counts, convergence flag, and method.

## Examples

### minimize (x - 3)^2 on [0, 10]

```json
{
  "expression": "(x - 3)^2",
  "lower": {
    "kind": "integer",
    "value": "0"
  },
  "upper": {
    "kind": "integer",
    "value": "10"
  },
  "variable": "x"
}
```

Expected: `{"type":"value","value":{"converged":true,"evaluations":{"kind":"integer","value":"6"},"iterations":{"kind":"integer","value":"6"},"method":"brent_minimization","value":{"kind":"float64","value":"0"},"x":{"kind":"float64","value":"3"}}}`


<a id="nonlinear_least_squares"></a>

## nonlinear_least_squares

Fit a nonlinear model to data with the Levenberg-Marquardt method.

Minimizes the sum of squared residuals between the model expression and the supplied targets. The model is a restricted expression in the declared parameter names plus the data variable x; the same restricted grammar and function set as optimize.minimize_1d applies. Derivatives are computed with central differences and the normal equations are damped and solved with partial-pivoted Gaussian elimination. Non-convergence is reported as an error; partial fits are never returned as success. The default initial guess is zero for every parameter.

- Module: `optimize` (version 1.0.0)
- Modes: scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/optimize.md#nonlinear_least_squares

## Parameters

- `model` — Restricted model expression in the parameter names and x. (restricted expression in x)
- `parameters` — Parameter names used by the model. (array of text)
- `points` — Data abscissae. (array of number)
- `targets` — Observed targets, one per point. (array of number)
- `initial` (optional) — Initial parameter guesses; default zeros. (array of number)
- `tolerance` (optional) — Convergence tolerance; default 1e-10. (number)
- `max_iterations` (optional) — Iteration cap; default 100, never above the context limit. (integer)

## Output

Fitted parameters, residual sum of squares, iteration count, convergence flag, and method.

## Examples

### fit a*x + b to (0,1), (1,2), (2,3)

```json
{
  "model": "a*x + b",
  "parameters": [
    "a",
    "b"
  ],
  "points": [
    {
      "kind": "integer",
      "value": "0"
    },
    {
      "kind": "integer",
      "value": "1"
    },
    {
      "kind": "integer",
      "value": "2"
    }
  ],
  "targets": [
    {
      "kind": "integer",
      "value": "1"
    },
    {
      "kind": "integer",
      "value": "2"
    },
    {
      "kind": "integer",
      "value": "3"
    }
  ]
}
```

Expected: `{"type":"value","value":{"converged":true,"iterations":{"kind":"integer","value":"4"},"method":"levenberg_marquardt","parameters":{"a":{"kind":"float64","value":"0.9999999999900482"},"b":{"kind":"float64","value":"1.0000000000124378"}},"residual_sum_squares":{"kind":"float64","value":"0.00000000000000000000021662140262708086"}}}`


