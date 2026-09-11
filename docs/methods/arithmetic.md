# Arithmetic module reference

Exact arithmetic, rounding, comparison, and integer/rational operations.

- Module id: `arithmetic`
- Version: 1.0.0
- Capabilities: exact_integer, exact_rational, exact_decimal, rounding_modes
- Supported modes: exact, auto, scientific
- Functions: 24

This file is generated from the live registry by `bicmath docs`; the function schemas, domains, and examples are the same ones the engine validates against at runtime.

<a id="abs"></a>

## abs

Absolute value of a number.

- Module: `arithmetic` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/arithmetic.md#abs

## Parameters

- `value` — Input number. (number)

## Output

Absolute value.

## Examples

### absolute value

```json
{
  "value": {
    "kind": "integer",
    "value": "-7"
  }
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"7"}}`


<a id="add"></a>

## add

Add two numbers under the shared promotion rules.

Exact for integer, rational, and decimal operands. Float64 operands require scientific mode and produce an approximate result.

- Module: `arithmetic` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/arithmetic.md#add

## Parameters

- `a` — Left operand. (number)
- `b` — Right operand. (number)

## Output

Sum of a and b.

## Examples

### exact decimal addition

```json
{
  "a": "0.1",
  "b": "0.2"
}
```

Expected: `{"type":"value","value":{"kind":"decimal","value":"0.3"}}`


<a id="binomial"></a>

## binomial

Exact binomial coefficient C(n, k).

Defined for integers n >= 0 and 0 <= k <= n; returns 0 when k < 0 or k > n. Bounded by the configured factorial and integer bit limits.

- Module: `arithmetic` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/arithmetic.md#binomial

## Parameters

- `n` — Non-negative integer. (integer)
- `k` — Integer number of selections. (integer)

## Output

C(n, k).

## Examples

### binomial coefficient

```json
{
  "k": {
    "kind": "integer",
    "value": "3"
  },
  "n": {
    "kind": "integer",
    "value": "10"
  }
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"120"}}`

### out of range k

```json
{
  "k": {
    "kind": "integer",
    "value": "9"
  },
  "n": {
    "kind": "integer",
    "value": "5"
  }
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"0"}}`


<a id="rounding"></a>

## ceil

Round toward positive infinity.

- Module: `arithmetic` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/arithmetic.md#rounding

## Parameters

- `value` — Input number. (number)

## Output

Rounded value.

## Examples

### round a decimal

```json
{
  "value": "1.5"
}
```

Expected: `{"type":"value","value":{"kind":"decimal","value":"2"}}`


<a id="compare"></a>

## compare

Compare two numbers exactly.

Comparisons are exact across representations: float64 is decomposed to its exact binary rational value, so 0.1 (decimal) is not equal to 0.1 (float64).

- Module: `arithmetic` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/arithmetic.md#compare

## Parameters

- `a` — Left operand. (number)
- `b` — Right operand. (number)

## Output

Ordering label and integer sign.

## Examples

### compare integers

```json
{
  "a": {
    "kind": "integer",
    "value": "1"
  },
  "b": {
    "kind": "integer",
    "value": "2"
  }
}
```

Expected: `{"type":"value","value":{"ordering":"less","sign":{"kind":"integer","value":"-1"}}}`


<a id="div"></a>

## div

Divide one number by another.

Integer / integer stays exact: an exact quotient is returned as an integer and a non-terminating quotient as a normalized rational. Decimal division rounds to the context precision and reports inexactness; in exact mode it is rejected.

- Module: `arithmetic` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/arithmetic.md#div

## Parameters

- `a` — Dividend. (number)
- `b` — Divisor; must not be zero. (number)

## Output

Quotient a / b.

## Examples

### rational division

```json
{
  "a": {
    "kind": "integer",
    "value": "1"
  },
  "b": {
    "kind": "integer",
    "value": "3"
  }
}
```

Expected: `{"type":"value","value":{"kind":"rational","numerator":"1","denominator":"3"}}`

### division by zero is an error

```json
{
  "a": {
    "kind": "integer",
    "value": "1"
  },
  "b": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"error","code":"division_by_zero"}`


<a id="factorial"></a>

## factorial

Exact factorial n!.

Defined for integers 0 <= n <= the configured factorial limit (default 10000). Results are bounded by the configured integer bit limit.

- Module: `arithmetic` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/arithmetic.md#factorial

## Parameters

- `n` — Non-negative integer. (integer)

## Output

n! as an exact integer.

## Examples

### factorial

```json
{
  "n": {
    "kind": "integer",
    "value": "5"
  }
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"120"}}`

### negative factorial

```json
{
  "n": {
    "kind": "integer",
    "value": "-1"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="rounding"></a>

## floor

Round toward negative infinity.

- Module: `arithmetic` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/arithmetic.md#rounding

## Parameters

- `value` — Input number. (number)

## Output

Rounded value.

## Examples

### round a decimal

```json
{
  "value": "1.5"
}
```

Expected: `{"type":"value","value":{"kind":"decimal","value":"1"}}`


<a id="gcd"></a>

## gcd

Non-negative greatest common divisor of two integers.

- Module: `arithmetic` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/arithmetic.md#gcd

## Parameters

- `a` — First integer. (integer)
- `b` — Second integer. (integer)

## Output

gcd(a, b), always non-negative.

## Examples

### gcd

```json
{
  "a": {
    "kind": "integer",
    "value": "12"
  },
  "b": {
    "kind": "integer",
    "value": "18"
  }
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"6"}}`


<a id="lcm"></a>

## lcm

Non-negative least common multiple of two integers.

lcm(a, 0) is defined as 0.

- Module: `arithmetic` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/arithmetic.md#lcm

## Parameters

- `a` — First integer. (integer)
- `b` — Second integer. (integer)

## Output

lcm(a, b), always non-negative.

## Examples

### lcm

```json
{
  "a": {
    "kind": "integer",
    "value": "4"
  },
  "b": {
    "kind": "integer",
    "value": "6"
  }
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"12"}}`


<a id="min-max"></a>

## max

Largest element of a non-empty array.

- Module: `arithmetic` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/arithmetic.md#min-max

## Parameters

- `values` — Non-empty array of numbers. (array of number)

## Output

Selected element.

## Examples

### select element

```json
{
  "values": [
    {
      "kind": "integer",
      "value": "3"
    },
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

Expected: `{"type":"value","value":{"kind":"integer","value":"3"}}`


<a id="min-max"></a>

## min

Smallest element of a non-empty array.

- Module: `arithmetic` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/arithmetic.md#min-max

## Parameters

- `values` — Non-empty array of numbers. (array of number)

## Output

Selected element.

## Examples

### select element

```json
{
  "values": [
    {
      "kind": "integer",
      "value": "3"
    },
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

Expected: `{"type":"value","value":{"kind":"integer","value":"1"}}`


<a id="modulo"></a>

## modulo

Remainder with the sign of the divisor; non-negative for a positive divisor.

- Module: `arithmetic` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/arithmetic.md#modulo

## Parameters

- `a` — Dividend. (number)
- `b` — Divisor; must not be zero. (number)

## Output

a mod b, in [0, |b|) for b > 0.

## Examples

### negative dividend

```json
{
  "a": {
    "kind": "integer",
    "value": "-7"
  },
  "b": {
    "kind": "integer",
    "value": "3"
  }
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"2"}}`


<a id="mul"></a>

## mul

Multiply two numbers.

- Module: `arithmetic` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/arithmetic.md#mul

## Parameters

- `a` — Left factor. (number)
- `b` — Right factor. (number)

## Output

Product a * b.

## Examples

### exact decimal multiplication

```json
{
  "a": "0.10",
  "b": "3"
}
```

Expected: `{"type":"value","value":{"kind":"decimal","value":"0.30"}}`


<a id="pow"></a>

## pow

Raise a number to a power.

Integer exponents are exact for exact bases (0^0 is defined as 1, consistently with the empty-product convention). Non-integer exponents require scientific mode and use binary64 powf.

- Module: `arithmetic` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/arithmetic.md#pow

## Parameters

- `base` — Base. (number)
- `exponent` — Exponent. (number)

## Output

base^exponent.

## Examples

### integer power

```json
{
  "base": {
    "kind": "integer",
    "value": "2"
  },
  "exponent": {
    "kind": "integer",
    "value": "10"
  }
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"1024"}}`

### zero to the zero

```json
{
  "base": {
    "kind": "integer",
    "value": "0"
  },
  "exponent": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"1"}}`


<a id="product"></a>

## product

Multiply an array of numbers.

Exact for integer/rational/decimal inputs. The product of an empty array is exact one.

- Module: `arithmetic` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/arithmetic.md#product

## Parameters

- `values` — Numbers to multiply. (array of number)

## Output

Product of the values.

## Examples

### exact product

```json
{
  "values": [
    {
      "kind": "integer",
      "value": "2"
    },
    {
      "kind": "integer",
      "value": "3"
    },
    {
      "kind": "integer",
      "value": "4"
    }
  ]
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"24"}}`


<a id="quantize"></a>

## quantize

Round a number to a fixed decimal scale with an explicit rounding mode.

Display scale and settlement scale are caller-controlled; this function rounds exactly at the requested scale. The default mode is half_even.

- Module: `arithmetic` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/arithmetic.md#quantize

## Parameters

- `value` — Input number. (number)
- `scale` — Number of fractional decimal digits; may be negative. (integer)
- `mode` (optional) — Rounding mode: half_even, half_away_from_zero, toward_zero, floor, ceiling. (one of ["half_even", "half_away_from_zero", "toward_zero", "floor", "ceiling"])

## Output

Value rounded to the requested scale.

## Examples

### half-even to cents

```json
{
  "scale": {
    "kind": "integer",
    "value": "2"
  },
  "value": "1.005"
}
```

Expected: `{"type":"value","value":{"kind":"decimal","value":"1.00"}}`

### half-away-from-zero to cents

```json
{
  "mode": "half_away_from_zero",
  "scale": {
    "kind": "integer",
    "value": "2"
  },
  "value": "1.005"
}
```

Expected: `{"type":"value","value":{"kind":"decimal","value":"1.01"}}`


<a id="rem"></a>

## rem

Remainder with the sign of the dividend (like Rust's %).

- Module: `arithmetic` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/arithmetic.md#rem

## Parameters

- `a` — Dividend. (number)
- `b` — Divisor; must not be zero. (number)

## Output

a - trunc(a / b) * b.

## Examples

### negative dividend

```json
{
  "a": {
    "kind": "integer",
    "value": "-7"
  },
  "b": {
    "kind": "integer",
    "value": "3"
  }
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"-1"}}`


<a id="sqrt"></a>

## sqrt

Square root with explicit exactness handling.

Returns an exact integer, rational, or decimal when the input has an exact square root in that representation. Otherwise: exact mode returns a domain error; auto mode returns a decimal approximation to the context precision and marks it approximate; scientific mode returns float64. Negative inputs are always a domain error in real mode.

- Module: `arithmetic` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/arithmetic.md#sqrt

## Parameters

- `value` — Non-negative input. (number)

## Output

Square root of value.

## Examples

### exact square root

```json
{
  "value": "0.25"
}
```

Expected: `{"type":"value","value":{"kind":"decimal","value":"0.5"}}`

### negative input

```json
{
  "value": {
    "kind": "integer",
    "value": "-1"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="sub"></a>

## sub

Subtract one number from another.

- Module: `arithmetic` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/arithmetic.md#sub

## Parameters

- `a` — Minuend. (number)
- `b` — Subtrahend. (number)

## Output

Difference a - b.

## Examples

### exact integer subtraction

```json
{
  "a": {
    "kind": "integer",
    "value": "5"
  },
  "b": {
    "kind": "integer",
    "value": "8"
  }
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"-3"}}`


<a id="sum"></a>

## sum

Sum an array of numbers.

Exact inputs are summed exactly. If any element is float64 (scientific mode), Neumaier compensated summation is used and the result is approximate. The sum of an empty array is exact zero.

- Module: `arithmetic` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/arithmetic.md#sum

## Parameters

- `values` — Numbers to sum. (array of number)

## Output

Sum of the values.

## Examples

### exact sum

```json
{
  "values": [
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
    },
    {
      "kind": "integer",
      "value": "4"
    }
  ]
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"10"}}`


<a id="to_decimal"></a>

## to_decimal

Convert an exact number to a decimal under the declared rounding context.

A terminating rational converts exactly. A non-terminating rational rounds to the context precision and is marked rounded. float64 is not silently converted.

- Module: `arithmetic` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/arithmetic.md#to_decimal

## Parameters

- `value` — Input number. (number)

## Output

Decimal value.

## Examples

### terminating rational

```json
{
  "value": {
    "kind": "rational",
    "numerator": "1",
    "denominator": "8"
  }
}
```

Expected: `{"type":"value","value":{"kind":"decimal","value":"0.125"}}`

### non-terminating rational in exact mode

```json
{
  "value": {
    "kind": "rational",
    "numerator": "1",
    "denominator": "3"
  }
}
```

Expected: `{"type":"error","code":"unsupported_numeric_mode"}`


<a id="to_rational"></a>

## to_rational

Convert an exact number to a normalized rational; float64 converts via its exact bits.

The rational is normalized with a positive denominator and no common factors. float64 conversion is exact in the sense that it recovers the binary value.

- Module: `arithmetic` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/arithmetic.md#to_rational

## Parameters

- `value` — Input number. (number)

## Output

Normalized rational value.

## Examples

### decimal to rational

```json
{
  "value": "0.25"
}
```

Expected: `{"type":"value","value":{"kind":"rational","numerator":"1","denominator":"4"}}`


<a id="rounding"></a>

## trunc

Round toward zero.

- Module: `arithmetic` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/arithmetic.md#rounding

## Parameters

- `value` — Input number. (number)

## Output

Rounded value.

## Examples

### round a decimal

```json
{
  "value": "1.5"
}
```

Expected: `{"type":"value","value":{"kind":"decimal","value":"1"}}`


