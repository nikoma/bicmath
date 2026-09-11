# Interval module reference

Heuristically padded binary64 interval arithmetic with outward rounding; not a certified bound.

- Module id: `interval`
- Version: 1.0.0
- Capabilities: heuristic_padding_enclosure, outward_rounding, interval_expression_evaluation, domain_checking
- Supported modes: exact, auto, scientific
- Functions: 23

This file is generated from the live registry by `bicmath docs`; the function schemas, domains, and examples are the same ones the engine validates against at runtime.

<a id="abs"></a>

## abs

Range of the absolute value over an interval.

Exact piecewise handling: a non-negative interval is unchanged, a non-positive interval is reflected, and a zero-straddling interval maps to [0, max(-lo, hi)].

- Module: `interval` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/interval.md#abs
- Units/currency rule: All inputs and outputs are dimensionless real quantities represented as finite binary64 intervals.

## Parameters

- `a` — Interval to take the absolute value of. (record with fields lo, hi)

## Output

Certified enclosure of |a|.

## Examples

### interval straddling zero

```json
{
  "a": {
    "hi": {
      "kind": "integer",
      "value": "2"
    },
    "lo": {
      "kind": "integer",
      "value": "-1"
    }
  }
}
```

Expected: `{"type":"value","value":{"assurance":"heuristic_padding_not_certified","assurance_note":"endpoints are heuristically widened; this is not a verified error bound","hi":{"kind":"float64","value":"2"},"lo":{"kind":"float64","value":"0"},"midpoint":{"kind":"float64","value":"1"},"width":{"kind":"float64","value":"2"}}}`


<a id="add"></a>

## add

Minkowski sum of two intervals.

Computes [a.lo + b.lo, a.hi + b.hi] in round-to-nearest binary64 and widens each endpoint outward, so the result contains every exact sum a + b.

- Module: `interval` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/interval.md#add
- Units/currency rule: All inputs and outputs are dimensionless real quantities represented as finite binary64 intervals.

## Parameters

- `a` — Left operand interval. (record with fields lo, hi)
- `b` — Right operand interval. (record with fields lo, hi)

## Output

Certified enclosure of the result.

## Examples

### add point intervals

```json
{
  "a": {
    "hi": {
      "kind": "integer",
      "value": "2"
    },
    "lo": {
      "kind": "integer",
      "value": "1"
    }
  },
  "b": {
    "hi": {
      "kind": "integer",
      "value": "4"
    },
    "lo": {
      "kind": "integer",
      "value": "3"
    }
  }
}
```

Expected: `{"type":"value","value":{"assurance":"heuristic_padding_not_certified","assurance_note":"endpoints are heuristically widened; this is not a verified error bound","hi":{"kind":"float64","value":"6.0000000000000036"},"lo":{"kind":"float64","value":"3.9999999999999982"},"midpoint":{"kind":"float64","value":"5.000000000000001"},"width":{"kind":"float64","value":"2.0000000000000053"}}}`


<a id="contains"></a>

## contains

Whether one interval contains another.

True when every point of b lies inside a. The empty interval is contained in every interval.

- Module: `interval` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/interval.md#contains
- Units/currency rule: All inputs and outputs are dimensionless real quantities represented as finite binary64 intervals.

## Parameters

- `a` — Outer interval. (record with fields lo, hi)
- `b` — Inner interval. (record with fields lo, hi)

## Output

True when b is a subset of a.

## Examples

### inner interval is contained

```json
{
  "a": {
    "hi": {
      "kind": "integer",
      "value": "2"
    },
    "lo": {
      "kind": "integer",
      "value": "0"
    }
  },
  "b": {
    "hi": {
      "kind": "decimal",
      "value": "1.5"
    },
    "lo": {
      "kind": "integer",
      "value": "1"
    }
  }
}
```

Expected: `{"type":"value","value":true}`


<a id="contains_value"></a>

## contains_value

Whether an exact value lies inside an interval.

The comparison is exact: the binary64 endpoints are decomposed to their exact binary rational values, so a decimal such as 0.1 is tested against its exact real value.

- Module: `interval` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/interval.md#contains_value
- Units/currency rule: All inputs and outputs are dimensionless real quantities represented as finite binary64 intervals.

## Parameters

- `a` — Interval to test. (record with fields lo, hi)
- `value` — Exact value to test for containment. (number)

## Output

True when the value lies in the closed interval.

## Examples

### value inside

```json
{
  "a": {
    "hi": {
      "kind": "integer",
      "value": "2"
    },
    "lo": {
      "kind": "integer",
      "value": "0"
    }
  },
  "value": {
    "kind": "integer",
    "value": "1"
  }
}
```

Expected: `{"type":"value","value":true}`

### value outside

```json
{
  "a": {
    "hi": {
      "kind": "integer",
      "value": "2"
    },
    "lo": {
      "kind": "integer",
      "value": "0"
    }
  },
  "value": {
    "kind": "integer",
    "value": "3"
  }
}
```

Expected: `{"type":"value","value":false}`


<a id="cos"></a>

## cos

Range of the cosine over an angle interval.

The range includes every critical point inside the interval and returns [-1, 1] for intervals wider than one period. Arguments with magnitude above 2^53 are handled conservatively.

- Module: `interval` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/interval.md#cos
- Units/currency rule: All inputs and outputs are dimensionless real quantities represented as finite binary64 intervals.

## Parameters

- `a` — Angle interval in radians. (record with fields lo, hi)

## Output

Certified enclosure of the trigonometric range.

## Examples

### unit interval

```json
{
  "a": {
    "hi": {
      "kind": "integer",
      "value": "1"
    },
    "lo": {
      "kind": "integer",
      "value": "0"
    }
  }
}
```

Expected: `{"type":"value","value":{"assurance":"heuristic_padding_not_certified","assurance_note":"endpoints are heuristically widened; this is not a verified error bound","hi":{"kind":"float64","value":"1.0000000000000009"},"lo":{"kind":"float64","value":"0.5403023058681393"},"midpoint":{"kind":"float64","value":"0.7701511529340701"},"width":{"kind":"float64","value":"0.45969769413186157"}}}`


<a id="div"></a>

## div

Range of the quotient of two intervals.

Evaluates all four endpoint quotients, takes the minimum and maximum, and widens the result outward. The divisor must not contain zero; otherwise the result is a DomainViolation.

- Module: `interval` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/interval.md#div
- Units/currency rule: All inputs and outputs are dimensionless real quantities represented as finite binary64 intervals.

## Parameters

- `a` — Left operand interval. (record with fields lo, hi)
- `b` — Right operand interval. (record with fields lo, hi)

## Output

Certified enclosure of the result.

## Examples

### divide point intervals

```json
{
  "a": {
    "hi": {
      "kind": "integer",
      "value": "2"
    },
    "lo": {
      "kind": "integer",
      "value": "1"
    }
  },
  "b": {
    "hi": {
      "kind": "integer",
      "value": "4"
    },
    "lo": {
      "kind": "integer",
      "value": "3"
    }
  }
}
```

Expected: `{"type":"value","value":{"assurance":"heuristic_padding_not_certified","assurance_note":"endpoints are heuristically widened; this is not a verified error bound","hi":{"kind":"float64","value":"0.6666666666666671"},"lo":{"kind":"float64","value":"0.2499999999999999"},"midpoint":{"kind":"float64","value":"0.4583333333333335"},"width":{"kind":"float64","value":"0.4166666666666672"}}}`

### divisor containing zero

```json
{
  "a": {
    "hi": {
      "kind": "integer",
      "value": "2"
    },
    "lo": {
      "kind": "integer",
      "value": "1"
    }
  },
  "b": {
    "hi": {
      "kind": "integer",
      "value": "1"
    },
    "lo": {
      "kind": "integer",
      "value": "-1"
    }
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="enclose"></a>

## enclose

Narrowest padded interval containing an exact number (heuristic, not certified).

Integers and exactly representable binary64 values enclose to a point interval. An exact decimal or rational that is not exactly representable is converted to the nearest binary64 and widened outward by two ulps, so the exact real value is contained under the documented binary64 conversion assumption.

- Module: `interval` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/interval.md#enclose
- Units/currency rule: All inputs and outputs are dimensionless real quantities represented as finite binary64 intervals.

## Parameters

- `value` — Exact number to enclose. (number)

## Output

Certified enclosure of the input.

## Examples

### enclose 0.1

```json
{
  "value": "0.1"
}
```

Expected: `{"type":"value","value":{"assurance":"heuristic_padding_not_certified","assurance_note":"endpoints are heuristically widened; this is not a verified error bound","hi":{"kind":"float64","value":"0.10000000000000003"},"lo":{"kind":"float64","value":"0.09999999999999998"},"midpoint":{"kind":"float64","value":"0.1"},"width":{"kind":"float64","value":"0.00000000000000005551115123125783"}}}`

### enclose an integer

```json
{
  "value": {
    "kind": "integer",
    "value": "2"
  }
}
```

Expected: `{"type":"value","value":{"assurance":"heuristic_padding_not_certified","assurance_note":"endpoints are heuristically widened; this is not a verified error bound","hi":{"kind":"float64","value":"2"},"lo":{"kind":"float64","value":"2"},"midpoint":{"kind":"float64","value":"2"},"width":{"kind":"float64","value":"0"}}}`


<a id="evaluate"></a>

## evaluate

Evaluate a restricted expression with interval variables.

Parses the expression with bicmath_core::expr::parse_expression and evaluates it with a local interval evaluator supporting + - * / % ^, unary sign, identifiers, and calls to sin, cos, tan, exp, ln, log10, log2, sqrt, abs, pow, min, max, floor, and ceil. Unknown variables produce NotFound and unknown functions produce UnknownFunction. The configured operation budget, AST depth, and cancellation checks are enforced.

- Module: `interval` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/interval.md#evaluate
- Units/currency rule: All inputs and outputs are dimensionless real quantities represented as finite binary64 intervals.

## Parameters

- `expression` — Restricted expression using interval variables. (restricted expression in )
- `variables` — Record mapping each free variable to an interval. (record with fields )

## Output

Certified enclosure of the expression range.

## Examples

### quadratic range contains the square root of two

```json
{
  "expression": "x^2 - 2",
  "variables": {
    "x": {
      "hi": {
        "kind": "integer",
        "value": "2"
      },
      "lo": {
        "kind": "integer",
        "value": "1"
      }
    }
  }
}
```

Expected: `{"type":"value","value":{"assurance":"heuristic_padding_not_certified","assurance_note":"endpoints are heuristically widened; this is not a verified error bound","hi":{"kind":"float64","value":"2.0000000000000124"},"lo":{"kind":"float64","value":"-1.0000000000000022"},"midpoint":{"kind":"float64","value":"0.5000000000000051"},"width":{"kind":"float64","value":"3.0000000000000147"}}}`

### unknown function

```json
{
  "expression": "nope(x)",
  "variables": {
    "x": {
      "hi": {
        "kind": "integer",
        "value": "2"
      },
      "lo": {
        "kind": "integer",
        "value": "1"
      }
    }
  }
}
```

Expected: `{"type":"error","code":"unknown_function"}`


<a id="exp"></a>

## exp

Range of the natural exponential over an interval.

The exponential is monotone increasing, so the endpoints map directly. A result beyond the finite binary64 range is a domain error.

- Module: `interval` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/interval.md#exp
- Units/currency rule: All inputs and outputs are dimensionless real quantities represented as finite binary64 intervals.

## Parameters

- `a` — Interval to exponentiate. (record with fields lo, hi)

## Output

Certified enclosure of exp(a).

## Examples

### exponential of a unit interval

```json
{
  "a": {
    "hi": {
      "kind": "integer",
      "value": "1"
    },
    "lo": {
      "kind": "integer",
      "value": "0"
    }
  }
}
```

Expected: `{"type":"value","value":{"assurance":"heuristic_padding_not_certified","assurance_note":"endpoints are heuristically widened; this is not a verified error bound","hi":{"kind":"float64","value":"2.718281828459047"},"lo":{"kind":"float64","value":"0.9999999999999996"},"midpoint":{"kind":"float64","value":"1.8591409142295232"},"width":{"kind":"float64","value":"1.7182818284590473"}}}`


<a id="from_bounds"></a>

## from_bounds

Build a padded interval from exact lower and upper bounds (heuristic, not certified).

The exact real bounds are converted to finite binary64 endpoints and widened outward when a bound is not exactly representable. A lower bound greater than the upper bound is a domain error.

- Module: `interval` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/interval.md#from_bounds
- Units/currency rule: All inputs and outputs are dimensionless real quantities represented as finite binary64 intervals.

## Parameters

- `lo` — Exact lower bound. (number)
- `hi` — Exact upper bound; must not be less than lo. (number)

## Output

Certified interval [lo, hi].

## Examples

### unit interval

```json
{
  "hi": {
    "kind": "integer",
    "value": "2"
  },
  "lo": {
    "kind": "integer",
    "value": "1"
  }
}
```

Expected: `{"type":"value","value":{"assurance":"heuristic_padding_not_certified","assurance_note":"endpoints are heuristically widened; this is not a verified error bound","hi":{"kind":"float64","value":"2"},"lo":{"kind":"float64","value":"1"},"midpoint":{"kind":"float64","value":"1.5"},"width":{"kind":"float64","value":"1"}}}`

### inverted bounds

```json
{
  "hi": {
    "kind": "integer",
    "value": "1"
  },
  "lo": {
    "kind": "integer",
    "value": "2"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="hull"></a>

## hull

Smallest interval containing both operands.

Empty operands are ignored; the hull of two empty intervals is empty.

- Module: `interval` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/interval.md#hull
- Units/currency rule: All inputs and outputs are dimensionless real quantities represented as finite binary64 intervals.

## Parameters

- `a` — Left operand interval. (record with fields lo, hi)
- `b` — Right operand interval. (record with fields lo, hi)

## Output

Certified enclosure of the result.

## Examples

### hull of disjoint intervals

```json
{
  "a": {
    "hi": {
      "kind": "integer",
      "value": "1"
    },
    "lo": {
      "kind": "integer",
      "value": "0"
    }
  },
  "b": {
    "hi": {
      "kind": "integer",
      "value": "3"
    },
    "lo": {
      "kind": "integer",
      "value": "2"
    }
  }
}
```

Expected: `{"type":"value","value":{"assurance":"heuristic_padding_not_certified","assurance_note":"endpoints are heuristically widened; this is not a verified error bound","hi":{"kind":"float64","value":"3"},"lo":{"kind":"float64","value":"0"},"midpoint":{"kind":"float64","value":"1.5"},"width":{"kind":"float64","value":"3"}}}`


<a id="intersect"></a>

## intersect

Set intersection of two intervals.

Disjoint intervals produce the canonical empty interval, whose record has null lo and hi endpoints and zero width.

- Module: `interval` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/interval.md#intersect
- Units/currency rule: All inputs and outputs are dimensionless real quantities represented as finite binary64 intervals.

## Parameters

- `a` — Left operand interval. (record with fields lo, hi)
- `b` — Right operand interval. (record with fields lo, hi)

## Output

Certified enclosure of the result.

## Examples

### overlapping intervals

```json
{
  "a": {
    "hi": {
      "kind": "integer",
      "value": "2"
    },
    "lo": {
      "kind": "integer",
      "value": "0"
    }
  },
  "b": {
    "hi": {
      "kind": "integer",
      "value": "3"
    },
    "lo": {
      "kind": "integer",
      "value": "1"
    }
  }
}
```

Expected: `{"type":"value","value":{"assurance":"heuristic_padding_not_certified","assurance_note":"endpoints are heuristically widened; this is not a verified error bound","hi":{"kind":"float64","value":"2"},"lo":{"kind":"float64","value":"1"},"midpoint":{"kind":"float64","value":"1.5"},"width":{"kind":"float64","value":"1"}}}`

### disjoint intervals

```json
{
  "a": {
    "hi": {
      "kind": "integer",
      "value": "1"
    },
    "lo": {
      "kind": "integer",
      "value": "0"
    }
  },
  "b": {
    "hi": {
      "kind": "integer",
      "value": "3"
    },
    "lo": {
      "kind": "integer",
      "value": "2"
    }
  }
}
```

Expected: `{"type":"value","value":{"assurance":"heuristic_padding_not_certified","assurance_note":"endpoints are heuristically widened; this is not a verified error bound","hi":null,"lo":null,"midpoint":null,"width":{"kind":"float64","value":"0"}}}`


<a id="is_empty"></a>

## is_empty

Whether an interval is the canonical empty interval.

The canonical empty interval is written as {"lo": null, "hi": null}; it is produced by intersecting disjoint intervals.

- Module: `interval` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/interval.md#is_empty
- Units/currency rule: All inputs and outputs are dimensionless real quantities represented as finite binary64 intervals.

## Parameters

- `a` — Interval to test. (record with fields lo, hi)

## Output

True for the empty interval.

## Examples

### empty interval

```json
{
  "a": {
    "hi": null,
    "lo": null
  }
}
```

Expected: `{"type":"value","value":true}`

### non-empty interval

```json
{
  "a": {
    "hi": {
      "kind": "integer",
      "value": "1"
    },
    "lo": {
      "kind": "integer",
      "value": "0"
    }
  }
}
```

Expected: `{"type":"value","value":false}`


<a id="ln"></a>

## ln

Range of the natural logarithm over an interval.

The natural logarithm is monotone increasing on the positive reals. An interval whose lower bound is not strictly positive, including any interval containing zero, is a domain error.

- Module: `interval` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/interval.md#ln
- Units/currency rule: All inputs and outputs are dimensionless real quantities represented as finite binary64 intervals.

## Parameters

- `a` — Interval to take the logarithm of. (record with fields lo, hi)

## Output

Certified enclosure of ln(a).

## Examples

### logarithm of a positive interval

```json
{
  "a": {
    "hi": {
      "kind": "integer",
      "value": "2"
    },
    "lo": {
      "kind": "integer",
      "value": "1"
    }
  }
}
```

Expected: `{"type":"value","value":{"assurance":"heuristic_padding_not_certified","assurance_note":"endpoints are heuristically widened; this is not a verified error bound","hi":{"kind":"float64","value":"0.6931471805599457"},"lo":{"kind":"float64","value":"-0.00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000002"},"midpoint":{"kind":"float64","value":"0.34657359027997287"},"width":{"kind":"float64","value":"0.6931471805599457"}}}`

### interval containing zero

```json
{
  "a": {
    "hi": {
      "kind": "integer",
      "value": "1"
    },
    "lo": {
      "kind": "integer",
      "value": "0"
    }
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="midpoint"></a>

## midpoint

Approximate midpoint of an interval.

Computed as lo/2 + hi/2 to avoid overflow. The result is a point estimate derived from the padded endpoints and is labelled approximate; the empty interval has no midpoint and returns null.

- Module: `interval` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/interval.md#midpoint
- Units/currency rule: All inputs and outputs are dimensionless real quantities represented as finite binary64 intervals.

## Parameters

- `a` — Interval whose midpoint is requested. (record with fields lo, hi)

## Output

Approximate midpoint, or null for the empty interval.

## Examples

### midpoint of [1, 2]

```json
{
  "a": {
    "hi": {
      "kind": "integer",
      "value": "2"
    },
    "lo": {
      "kind": "integer",
      "value": "1"
    }
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"1.5"}}`


<a id="mul"></a>

## mul

Range of the product of two intervals.

Evaluates all four endpoint products, takes the minimum and maximum, and widens the result outward, so it contains every exact product a * b.

- Module: `interval` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/interval.md#mul
- Units/currency rule: All inputs and outputs are dimensionless real quantities represented as finite binary64 intervals.

## Parameters

- `a` — Left operand interval. (record with fields lo, hi)
- `b` — Right operand interval. (record with fields lo, hi)

## Output

Certified enclosure of the result.

## Examples

### multiply point intervals

```json
{
  "a": {
    "hi": {
      "kind": "integer",
      "value": "2"
    },
    "lo": {
      "kind": "integer",
      "value": "1"
    }
  },
  "b": {
    "hi": {
      "kind": "integer",
      "value": "4"
    },
    "lo": {
      "kind": "integer",
      "value": "3"
    }
  }
}
```

Expected: `{"type":"value","value":{"assurance":"heuristic_padding_not_certified","assurance_note":"endpoints are heuristically widened; this is not a verified error bound","hi":{"kind":"float64","value":"8.000000000000007"},"lo":{"kind":"float64","value":"2.9999999999999982"},"midpoint":{"kind":"float64","value":"5.500000000000003"},"width":{"kind":"float64","value":"5.000000000000009"}}}`


<a id="neg"></a>

## neg

Sign change of an interval.

Maps [lo, hi] to [-hi, -lo]. Negation is exact in binary64, so no ulp padding is introduced.

- Module: `interval` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/interval.md#neg
- Units/currency rule: All inputs and outputs are dimensionless real quantities represented as finite binary64 intervals.

## Parameters

- `a` — Interval to negate. (record with fields lo, hi)

## Output

Certified enclosure of -a.

## Examples

### negate an interval

```json
{
  "a": {
    "hi": {
      "kind": "integer",
      "value": "2"
    },
    "lo": {
      "kind": "integer",
      "value": "1"
    }
  }
}
```

Expected: `{"type":"value","value":{"assurance":"heuristic_padding_not_certified","assurance_note":"endpoints are heuristically widened; this is not a verified error bound","hi":{"kind":"float64","value":"-1"},"lo":{"kind":"float64","value":"-2"},"midpoint":{"kind":"float64","value":"-1.5"},"width":{"kind":"float64","value":"1"}}}`


<a id="pow_int"></a>

## pow_int

Range of an interval raised to an integer power.

Uses interval exponentiation by squaring with outward widening at every multiply. Even powers clamp the lower bound at zero. Negative exponents compute the reciprocal of the positive power and require a base that excludes zero. The exponent magnitude is bounded by the configured exponent limit.

- Module: `interval` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/interval.md#pow_int
- Units/currency rule: All inputs and outputs are dimensionless real quantities represented as finite binary64 intervals.

## Parameters

- `base` — Base interval. (record with fields lo, hi)
- `exponent` — Integer exponent. (integer)

## Output

Certified enclosure of base^exponent.

## Examples

### square an interval

```json
{
  "base": {
    "hi": {
      "kind": "integer",
      "value": "3"
    },
    "lo": {
      "kind": "integer",
      "value": "-2"
    }
  },
  "exponent": {
    "kind": "integer",
    "value": "2"
  }
}
```

Expected: `{"type":"value","value":{"assurance":"heuristic_padding_not_certified","assurance_note":"endpoints are heuristically widened; this is not a verified error bound","hi":{"kind":"float64","value":"9.000000000000021"},"lo":{"kind":"float64","value":"-0.00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000002"},"midpoint":{"kind":"float64","value":"4.500000000000011"},"width":{"kind":"float64","value":"9.000000000000021"}}}`

### negative power of a zero-containing interval

```json
{
  "base": {
    "hi": {
      "kind": "integer",
      "value": "1"
    },
    "lo": {
      "kind": "integer",
      "value": "0"
    }
  },
  "exponent": {
    "kind": "integer",
    "value": "-1"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="sin"></a>

## sin

Range of the sine over an angle interval.

The range includes every critical point inside the interval and returns [-1, 1] for intervals wider than one period. Arguments with magnitude above 2^53 are handled conservatively.

- Module: `interval` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/interval.md#sin
- Units/currency rule: All inputs and outputs are dimensionless real quantities represented as finite binary64 intervals.

## Parameters

- `a` — Angle interval in radians. (record with fields lo, hi)

## Output

Certified enclosure of the trigonometric range.

## Examples

### unit interval

```json
{
  "a": {
    "hi": {
      "kind": "integer",
      "value": "1"
    },
    "lo": {
      "kind": "integer",
      "value": "0"
    }
  }
}
```

Expected: `{"type":"value","value":{"assurance":"heuristic_padding_not_certified","assurance_note":"endpoints are heuristically widened; this is not a verified error bound","hi":{"kind":"float64","value":"0.841470984807897"},"lo":{"kind":"float64","value":"-0.00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000002"},"midpoint":{"kind":"float64","value":"0.4207354924039485"},"width":{"kind":"float64","value":"0.841470984807897"}}}`


<a id="sqrt"></a>

## sqrt

Range of the square root over an interval.

Defined on the non-negative part of the interval: a lower endpoint below zero is clamped to zero. A strictly negative interval is a domain error.

- Module: `interval` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/interval.md#sqrt
- Units/currency rule: All inputs and outputs are dimensionless real quantities represented as finite binary64 intervals.

## Parameters

- `a` — Interval to take the square root of. (record with fields lo, hi)

## Output

Certified enclosure of sqrt(a).

## Examples

### square root of a positive interval

```json
{
  "a": {
    "hi": {
      "kind": "integer",
      "value": "9"
    },
    "lo": {
      "kind": "integer",
      "value": "4"
    }
  }
}
```

Expected: `{"type":"value","value":{"assurance":"heuristic_padding_not_certified","assurance_note":"endpoints are heuristically widened; this is not a verified error bound","hi":{"kind":"float64","value":"3.0000000000000018"},"lo":{"kind":"float64","value":"1.9999999999999991"},"midpoint":{"kind":"float64","value":"2.5000000000000004"},"width":{"kind":"float64","value":"1.0000000000000027"}}}`

### strictly negative interval

```json
{
  "a": {
    "hi": {
      "kind": "integer",
      "value": "-1"
    },
    "lo": {
      "kind": "integer",
      "value": "-4"
    }
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="sub"></a>

## sub

Difference of two intervals.

Computes [a.lo - b.hi, a.hi - b.lo] in round-to-nearest binary64 and widens each endpoint outward, so the result contains every exact difference a - b.

- Module: `interval` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/interval.md#sub
- Units/currency rule: All inputs and outputs are dimensionless real quantities represented as finite binary64 intervals.

## Parameters

- `a` — Left operand interval. (record with fields lo, hi)
- `b` — Right operand interval. (record with fields lo, hi)

## Output

Certified enclosure of the result.

## Examples

### subtract point intervals

```json
{
  "a": {
    "hi": {
      "kind": "integer",
      "value": "2"
    },
    "lo": {
      "kind": "integer",
      "value": "1"
    }
  },
  "b": {
    "hi": {
      "kind": "integer",
      "value": "4"
    },
    "lo": {
      "kind": "integer",
      "value": "3"
    }
  }
}
```

Expected: `{"type":"value","value":{"assurance":"heuristic_padding_not_certified","assurance_note":"endpoints are heuristically widened; this is not a verified error bound","hi":{"kind":"float64","value":"-0.9999999999999996"},"lo":{"kind":"float64","value":"-3.0000000000000018"},"midpoint":{"kind":"float64","value":"-2.000000000000001"},"width":{"kind":"float64","value":"2.000000000000002"}}}`


<a id="tan"></a>

## tan

Range of the tangent over an angle interval.

The tangent is increasing on each branch between poles. An interval that crosses a pole pi/2 + k*pi, or whose argument is too large to exclude poles by argument reduction, is a domain error.

- Module: `interval` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/interval.md#tan
- Units/currency rule: All inputs and outputs are dimensionless real quantities represented as finite binary64 intervals.

## Parameters

- `a` — Angle interval in radians. (record with fields lo, hi)

## Output

Certified enclosure of the trigonometric range.

## Examples

### unit interval

```json
{
  "a": {
    "hi": {
      "kind": "integer",
      "value": "1"
    },
    "lo": {
      "kind": "integer",
      "value": "0"
    }
  }
}
```

Expected: `{"type":"value","value":{"assurance":"heuristic_padding_not_certified","assurance_note":"endpoints are heuristically widened; this is not a verified error bound","hi":{"kind":"float64","value":"1.557407724654903"},"lo":{"kind":"float64","value":"-0.00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000002"},"midpoint":{"kind":"float64","value":"0.7787038623274515"},"width":{"kind":"float64","value":"1.557407724654903"}}}`

### interval crossing pi/2

```json
{
  "a": {
    "hi": {
      "kind": "decimal",
      "value": "1.6"
    },
    "lo": {
      "kind": "decimal",
      "value": "1.5"
    }
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="width"></a>

## width

Approximate width of an interval.

Computed as hi - lo and labelled approximate. The empty interval has width zero; a width beyond the finite binary64 range is reported as null.

- Module: `interval` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/interval.md#width
- Units/currency rule: All inputs and outputs are dimensionless real quantities represented as finite binary64 intervals.

## Parameters

- `a` — Interval whose width is requested. (record with fields lo, hi)

## Output

Approximate width, or null when it exceeds binary64.

## Examples

### width of [1, 2]

```json
{
  "a": {
    "hi": {
      "kind": "integer",
      "value": "2"
    },
    "lo": {
      "kind": "integer",
      "value": "1"
    }
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"1"}}`


