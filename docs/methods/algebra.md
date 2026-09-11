# Algebra module reference

Exact polynomial algebra, number theory, and combinatorics over integers and rationals.

- Module id: `algebra`
- Version: 1.0.0
- Capabilities: exact_polynomials, number_theory, combinatorics, exact_roots
- Supported modes: exact, auto, scientific
- Functions: 28

This file is generated from the live registry by `bicmath docs`; the function schemas, domains, and examples are the same ones the engine validates against at runtime.

<a id="bell"></a>

## bell

The nth Bell number (all set partitions of n items).

Computed with the Bell triangle using exact integers. Work is bounded by the operation and iteration budgets and by the integer bit limit.

- Module: `algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Quadratic
- Determinism: Deterministic
- Method reference: docs/methods/algebra.md#bell

## Parameters

- `n` — Non-negative index. (integer)

## Output

B(n).

## Examples

### fifth Bell number

```json
{
  "n": {
    "kind": "integer",
    "value": "5"
  }
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"52"}}`


<a id="catalan"></a>

## catalan

The nth Catalan number C(n).

C(n) = binom(2n, n) / (n + 1), computed exactly. The index is bounded by half the configured factorial limit.

- Module: `algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/algebra.md#catalan

## Parameters

- `n` — Non-negative index. (integer)

## Output

C(n).

## Examples

### tenth Catalan number

```json
{
  "n": {
    "kind": "integer",
    "value": "10"
  }
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"16796"}}`


<a id="crt"></a>

## crt

Solve a system of simultaneous congruences.

Generalized CRT for non-necessarily-coprime moduli. Returns the least non-negative solution modulo the least common multiple of the moduli. An inconsistent system is a domain error.

- Module: `algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/algebra.md#crt

## Parameters

- `residues` — Residues. (array of integer)
- `moduli` — Non-zero moduli. (array of integer)

## Output

Least non-negative solution modulo lcm(moduli).

## Examples

### classic three-modulus system

```json
{
  "moduli": [
    {
      "kind": "integer",
      "value": "3"
    },
    {
      "kind": "integer",
      "value": "5"
    },
    {
      "kind": "integer",
      "value": "7"
    }
  ],
  "residues": [
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
      "value": "2"
    }
  ]
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"23"}}`

### empty system

```json
{
  "moduli": [],
  "residues": []
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="divisors"></a>

## divisors

All positive divisors of a positive integer in ascending order.

- Module: `algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/algebra.md#divisors

## Parameters

- `n` — Positive integer. (integer)

## Output

Positive divisors.

## Examples

### divisors of 12

```json
{
  "n": {
    "kind": "integer",
    "value": "12"
  }
}
```

Expected: `{"type":"value","value":[{"kind":"integer","value":"1"},{"kind":"integer","value":"2"},{"kind":"integer","value":"3"},{"kind":"integer","value":"4"},{"kind":"integer","value":"6"},{"kind":"integer","value":"12"}]}`


<a id="fibonacci"></a>

## fibonacci

Fibonacci number with fast doubling and negative indices.

F(n) is computed by fast doubling in O(log |n|) exact integer operations. Negative indices use F(-n) = (-1)^(n+1) F(n). Indices are bounded by the integer bit limit.

- Module: `algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/algebra.md#fibonacci

## Parameters

- `n` — Integer index. (integer)

## Output

F(n).

## Examples

### hundredth Fibonacci number

```json
{
  "n": {
    "kind": "integer",
    "value": "100"
  }
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"354224848179261915075"}}`

### negative index

```json
{
  "n": {
    "kind": "integer",
    "value": "-7"
  }
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"13"}}`


<a id="gcd_extended"></a>

## gcd_extended

Greatest common divisor with Bezout coefficients.

Returns gcd, x and y such that a*x + b*y = gcd(a, b), with gcd always non-negative.

- Module: `algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/algebra.md#gcd_extended

## Parameters

- `a` — First integer. (integer)
- `b` — Second integer. (integer)

## Output

GCD and Bezout coefficients.

## Examples

### Bezout identity for 240 and 46

```json
{
  "a": {
    "kind": "integer",
    "value": "240"
  },
  "b": {
    "kind": "integer",
    "value": "46"
  }
}
```

Expected: `{"type":"value","value":{"gcd":{"kind":"integer","value":"2"},"x":{"kind":"integer","value":"-9"},"y":{"kind":"integer","value":"47"}}}`


<a id="integer_nth_root"></a>

## integer_nth_root

Floor of the nth root of an integer.

For negative inputs the root index must be odd. The result is the floor of the real nth root, computed by integer Newton iteration.

- Module: `algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/algebra.md#integer_nth_root

## Parameters

- `n` — Integer radicand. (integer)
- `k` — Root index, at least 1. (integer)

## Output

Floor(n^(1/k)).

## Examples

### cube root of 27

```json
{
  "k": {
    "kind": "integer",
    "value": "3"
  },
  "n": {
    "kind": "integer",
    "value": "27"
  }
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"3"}}`

### even root of a negative number

```json
{
  "k": {
    "kind": "integer",
    "value": "2"
  },
  "n": {
    "kind": "integer",
    "value": "-4"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="is_perfect_square"></a>

## is_perfect_square

Whether an integer is the square of an integer.

- Module: `algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/algebra.md#is_perfect_square

## Parameters

- `n` — Integer to test. (integer)

## Output

True when n is a perfect square.

## Examples

### perfect square

```json
{
  "n": {
    "kind": "integer",
    "value": "144"
  }
}
```

Expected: `{"type":"value","value":true}`

### non-square

```json
{
  "n": {
    "kind": "integer",
    "value": "145"
  }
}
```

Expected: `{"type":"value","value":false}`


<a id="is_prime"></a>

## is_prime

Deterministic Miller-Rabin primality test.

For n < 2^64 the twelve documented bases 2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31 and 37 are a proven deterministic test. For larger n the first 64 primes are used and the result is probable rather than proven, which is reported as a warning.

- Module: `algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/algebra.md#is_prime

## Parameters

- `n` — Integer to test. (integer)

## Output

True when n is prime.

## Examples

### Mersenne prime 2^61 - 1

```json
{
  "n": {
    "kind": "integer",
    "value": "2305843009213693951"
  }
}
```

Expected: `{"type":"value","value":true}`

### Carmichael number 561

```json
{
  "n": {
    "kind": "integer",
    "value": "561"
  }
}
```

Expected: `{"type":"value","value":false}`


<a id="lucas"></a>

## lucas

Lucas number with fast doubling and negative indices.

L(n) = 2F(n+1) - F(n), computed exactly. Negative indices use L(-n) = (-1)^n L(n). Indices are bounded by the integer bit limit.

- Module: `algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/algebra.md#lucas

## Parameters

- `n` — Integer index. (integer)

## Output

L(n).

## Examples

### tenth Lucas number

```json
{
  "n": {
    "kind": "integer",
    "value": "10"
  }
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"123"}}`


<a id="mod_inverse"></a>

## mod_inverse

Multiplicative inverse modulo m.

Returns the inverse of a modulo |m| in the range [0, |m|). A domain error is reported when gcd(a, m) != 1.

- Module: `algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/algebra.md#mod_inverse

## Parameters

- `a` — Value to invert. (integer)
- `m` — Non-zero modulus. (integer)

## Output

Inverse of a modulo |m|.

## Examples

### inverse of 3 modulo 11

```json
{
  "a": {
    "kind": "integer",
    "value": "3"
  },
  "m": {
    "kind": "integer",
    "value": "11"
  }
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"4"}}`

### non-invertible value

```json
{
  "a": {
    "kind": "integer",
    "value": "6"
  },
  "m": {
    "kind": "integer",
    "value": "9"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="mod_pow"></a>

## mod_pow

Exact modular power with square-and-multiply.

Returns base^exponent mod |modulus| in the range [0, |modulus|). Negative exponents invert the base first and require it to be invertible.

- Module: `algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/algebra.md#mod_pow

## Parameters

- `base` — Base. (integer)
- `exponent` — Exponent. (integer)
- `modulus` — Non-zero modulus. (integer)

## Output

base^exponent mod |modulus|.

## Examples

### 2^10 modulo 1000

```json
{
  "base": {
    "kind": "integer",
    "value": "2"
  },
  "exponent": {
    "kind": "integer",
    "value": "10"
  },
  "modulus": {
    "kind": "integer",
    "value": "1000"
  }
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"24"}}`


<a id="multinomial"></a>

## multinomial

Number of ways to partition items into labelled groups.

Computes (sum counts)! / product(counts!) for non-negative counts. The total is bounded by the configured factorial limit.

- Module: `algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/algebra.md#multinomial

## Parameters

- `counts` — Non-negative group sizes. (array of integer)

## Output

Multinomial coefficient.

## Examples

### group sizes 2, 1, 1

```json
{
  "counts": [
    {
      "kind": "integer",
      "value": "2"
    },
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

Expected: `{"type":"value","value":{"kind":"integer","value":"12"}}`


<a id="next_prime"></a>

## next_prime

Smallest prime strictly greater than the input.

- Module: `algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/algebra.md#next_prime

## Parameters

- `n` — Integer lower bound. (integer)

## Output

Next prime after n.

## Examples

### next prime after 100

```json
{
  "n": {
    "kind": "integer",
    "value": "100"
  }
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"101"}}`


<a id="partition_count"></a>

## partition_count

Number of integer partitions p(n).

Euler's pentagonal number recurrence with a bounded table. The index is limited to 10000 and results are bounded by the configured integer bit limit.

- Module: `algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Quadratic
- Determinism: Deterministic
- Method reference: docs/methods/algebra.md#partition_count

## Parameters

- `n` — Non-negative integer. (integer)

## Output

p(n).

## Examples

### partitions of 100

```json
{
  "n": {
    "kind": "integer",
    "value": "100"
  }
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"190569292"}}`


<a id="permutations"></a>

## permutations

Number of ordered arrangements P(n, k).

P(n, k) = n! / (n - k)! for 0 <= k <= n, and 0 when k > n. The result is exact and bounded by the configured factorial and integer bit limits.

- Module: `algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/algebra.md#permutations

## Parameters

- `n` — Number of items, n >= 0. (integer)
- `k` — Number selected, k >= 0. (integer)

## Output

P(n, k).

## Examples

### arrangements of 5 items taken 3 at a time

```json
{
  "k": {
    "kind": "integer",
    "value": "3"
  },
  "n": {
    "kind": "integer",
    "value": "5"
  }
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"60"}}`

### k greater than n

```json
{
  "k": {
    "kind": "integer",
    "value": "5"
  },
  "n": {
    "kind": "integer",
    "value": "3"
  }
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"0"}}`


<a id="polynomial_add"></a>

## polynomial_add

Add two exact polynomials.

Coefficient arrays are indexed by degree with the constant term first and are normalized by stripping trailing zeros. All arithmetic is exact over the integers and rationals.

- Module: `algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/algebra.md#polynomial_add

## Parameters

- `a` — First coefficient array. (array of exact number (integer, rational, or decimal))
- `b` — Second coefficient array. (array of exact number (integer, rational, or decimal))

## Output

Sum a + b, normalized.

## Examples

### integer coefficients

```json
{
  "a": [
    {
      "kind": "integer",
      "value": "1"
    },
    {
      "kind": "integer",
      "value": "2"
    }
  ],
  "b": [
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

Expected: `{"type":"value","value":[{"kind":"integer","value":"4"},{"kind":"integer","value":"6"}]}`


<a id="polynomial_derivative"></a>

## polynomial_derivative

Formal derivative of an exact polynomial.

- Module: `algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/algebra.md#polynomial_derivative

## Parameters

- `coefficients` — Coefficient array. (array of exact number (integer, rational, or decimal))

## Output

Derivative coefficients, normalized.

## Examples

### quadratic derivative

```json
{
  "coefficients": [
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

Expected: `{"type":"value","value":[{"kind":"integer","value":"2"},{"kind":"integer","value":"6"}]}`


<a id="polynomial_divmod"></a>

## polynomial_divmod

Divide two exact polynomials, returning quotient and remainder.

Long division over the rationals. The remainder has degree strictly smaller than the divisor; dividing by the zero polynomial is an error.

- Module: `algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Quadratic
- Determinism: Deterministic
- Method reference: docs/methods/algebra.md#polynomial_divmod

## Parameters

- `a` — Dividend coefficient array. (array of exact number (integer, rational, or decimal))
- `b` — Divisor coefficient array. (array of exact number (integer, rational, or decimal))

## Output

Quotient and remainder coefficient arrays, normalized.

## Examples

### difference of squares divided by a linear factor

```json
{
  "a": [
    {
      "kind": "integer",
      "value": "-1"
    },
    {
      "kind": "integer",
      "value": "0"
    },
    {
      "kind": "integer",
      "value": "1"
    }
  ],
  "b": [
    {
      "kind": "integer",
      "value": "-1"
    },
    {
      "kind": "integer",
      "value": "1"
    }
  ]
}
```

Expected: `{"type":"value","value":{"quotient":[{"kind":"integer","value":"1"},{"kind":"integer","value":"1"}],"remainder":[{"kind":"integer","value":"0"}]}}`

### division by the zero polynomial

```json
{
  "a": [
    {
      "kind": "integer",
      "value": "1"
    },
    {
      "kind": "integer",
      "value": "1"
    }
  ],
  "b": [
    {
      "kind": "integer",
      "value": "0"
    }
  ]
}
```

Expected: `{"type":"error","code":"division_by_zero"}`


<a id="polynomial_evaluate"></a>

## polynomial_evaluate

Evaluate an exact polynomial at an exact point.

- Module: `algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/algebra.md#polynomial_evaluate

## Parameters

- `coefficients` — Coefficient array. (array of exact number (integer, rational, or decimal))
- `x` — Evaluation point. (exact number (integer, rational, or decimal))

## Output

Exact value p(x).

## Examples

### evaluate a quadratic

```json
{
  "coefficients": [
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
  ],
  "x": {
    "kind": "integer",
    "value": "2"
  }
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"17"}}`


<a id="polynomial_from_roots"></a>

## polynomial_from_roots

Build an exact polynomial from its roots.

Multiplies the factors (x - r) for each root, optionally scaled by a leading coefficient (default 1).

- Module: `algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Quadratic
- Determinism: Deterministic
- Method reference: docs/methods/algebra.md#polynomial_from_roots

## Parameters

- `roots` — Root list. (array of exact number (integer, rational, or decimal))
- `leading` (optional) — Leading coefficient; defaults to 1. (exact number (integer, rational, or decimal))

## Output

Expanded coefficients, normalized.

## Examples

### two integer roots

```json
{
  "roots": [
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

Expected: `{"type":"value","value":[{"kind":"integer","value":"2"},{"kind":"integer","value":"-3"},{"kind":"integer","value":"1"}]}`

### zero leading coefficient

```json
{
  "leading": {
    "kind": "integer",
    "value": "0"
  },
  "roots": [
    {
      "kind": "integer",
      "value": "1"
    }
  ]
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="polynomial_gcd"></a>

## polynomial_gcd

Monic greatest common divisor of two exact polynomials.

The Euclidean algorithm over the rationals, normalized to a monic polynomial. The GCD of two zero polynomials is the zero polynomial.

- Module: `algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Quadratic
- Determinism: Deterministic
- Method reference: docs/methods/algebra.md#polynomial_gcd

## Parameters

- `a` — First coefficient array. (array of exact number (integer, rational, or decimal))
- `b` — Second coefficient array. (array of exact number (integer, rational, or decimal))

## Output

Monic GCD, normalized.

## Examples

### common linear factor

```json
{
  "a": [
    {
      "kind": "integer",
      "value": "-1"
    },
    {
      "kind": "integer",
      "value": "0"
    },
    {
      "kind": "integer",
      "value": "1"
    }
  ],
  "b": [
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
      "value": "1"
    }
  ]
}
```

Expected: `{"type":"value","value":[{"kind":"integer","value":"1"},{"kind":"integer","value":"1"}]}`


<a id="polynomial_mul"></a>

## polynomial_mul

Multiply two exact polynomials.

- Module: `algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Quadratic
- Determinism: Deterministic
- Method reference: docs/methods/algebra.md#polynomial_mul

## Parameters

- `a` — Left coefficient array. (array of exact number (integer, rational, or decimal))
- `b` — Right coefficient array. (array of exact number (integer, rational, or decimal))

## Output

Product a * b, normalized.

## Examples

### difference of squares

```json
{
  "a": [
    {
      "kind": "integer",
      "value": "1"
    },
    {
      "kind": "integer",
      "value": "1"
    }
  ],
  "b": [
    {
      "kind": "integer",
      "value": "1"
    },
    {
      "kind": "integer",
      "value": "-1"
    }
  ]
}
```

Expected: `{"type":"value","value":[{"kind":"integer","value":"1"},{"kind":"integer","value":"0"},{"kind":"integer","value":"-1"}]}`


<a id="polynomial_roots"></a>

## polynomial_roots

Real roots of an exact polynomial of degree at most four.

Rational roots are found exactly with the rational-root theorem and deflation. Quadratic roots are exact when the discriminant is a perfect rational square. Remaining irrational real roots are isolated numerically and reported as float64 records with a warning; exact mode rejects them. Polynomials of degree greater than four are unsupported.

- Module: `algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/algebra.md#polynomial_roots

## Parameters

- `coefficients` — Coefficient array of degree at most four. (array of exact number (integer, rational, or decimal))

## Output

Real roots in ascending order, exact where possible.

## Examples

### rational roots

```json
{
  "coefficients": [
    {
      "kind": "integer",
      "value": "2"
    },
    {
      "kind": "integer",
      "value": "-3"
    },
    {
      "kind": "integer",
      "value": "1"
    }
  ]
}
```

Expected: `{"type":"value","value":[{"exact":true,"value":{"kind":"integer","value":"1"}},{"exact":true,"value":{"kind":"integer","value":"2"}}]}`

### irrational roots are approximate

```json
{
  "coefficients": [
    {
      "kind": "integer",
      "value": "-2"
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
}
```

Expected: `{"type":"contains","text":"approximate"}`

### degree above four

```json
{
  "coefficients": [
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
      "value": "0"
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
}
```

Expected: `{"type":"error","code":"unsupported_operation"}`


<a id="polynomial_sub"></a>

## polynomial_sub

Subtract one exact polynomial from another.

- Module: `algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/algebra.md#polynomial_sub

## Parameters

- `a` — Minuend coefficient array. (array of exact number (integer, rational, or decimal))
- `b` — Subtrahend coefficient array. (array of exact number (integer, rational, or decimal))

## Output

Difference a - b, normalized.

## Examples

### integer coefficients

```json
{
  "a": [
    {
      "kind": "integer",
      "value": "1"
    },
    {
      "kind": "integer",
      "value": "2"
    }
  ],
  "b": [
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

Expected: `{"type":"value","value":[{"kind":"integer","value":"-2"},{"kind":"integer","value":"-2"}]}`


<a id="prime_factors"></a>

## prime_factors

Complete factorization of a positive integer.

Trial division up to 100000 is followed by deterministic-seed Pollard rho (Brent's variant) for any remaining composite cofactor. The result lists prime factors in ascending order with multiplicities and reports the method used.

- Module: `algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/algebra.md#prime_factors

## Parameters

- `n` — Positive integer to factor. (integer)

## Output

Prime factors and the method used.

## Examples

### factorization of 360

```json
{
  "n": {
    "kind": "integer",
    "value": "360"
  }
}
```

Expected: `{"type":"value","value":{"factors":[{"exponent":{"kind":"integer","value":"3"},"prime":{"kind":"integer","value":"2"}},{"exponent":{"kind":"integer","value":"2"},"prime":{"kind":"integer","value":"3"}},{"exponent":{"kind":"integer","value":"1"},"prime":{"kind":"integer","value":"5"}}],"method":"trial_division"}}`


<a id="stirling_second"></a>

## stirling_second

Number of partitions of n items into k non-empty subsets.

Computed with the recurrence S(n, k) = k*S(n-1, k) + S(n-1, k-1) using exact integers. Work is bounded by the operation and iteration budgets.

- Module: `algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Quadratic
- Determinism: Deterministic
- Method reference: docs/methods/algebra.md#stirling_second

## Parameters

- `n` — Number of items, n >= 0. (integer)
- `k` — Number of subsets, k >= 0. (integer)

## Output

S(n, k).

## Examples

### five items in three subsets

```json
{
  "k": {
    "kind": "integer",
    "value": "3"
  },
  "n": {
    "kind": "integer",
    "value": "5"
  }
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"25"}}`


<a id="totient"></a>

## totient

Euler's totient function phi(n).

- Module: `algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/algebra.md#totient

## Parameters

- `n` — Positive integer. (integer)

## Output

phi(n), the count of totatives of n.

## Examples

### totient of 9

```json
{
  "n": {
    "kind": "integer",
    "value": "9"
  }
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"6"}}`


