# Scientific module reference

Elementary transcendental functions, angle conversions, root finding, numerical integration and differentiation, ordinary differential equations, and special functions.

- Module id: `scientific`
- Version: 1.0.0
- Capabilities: elementary_functions, angle_conversions, root_finding, numerical_integration, numerical_differentiation, ode_integration, special_functions
- Supported modes: scientific
- Functions: 38

This file is generated from the live registry by `bicmath docs`; the function schemas, domains, and examples are the same ones the engine validates against at runtime.

<a id="acos"></a>

## acos

Inverse cosine in radians.

Defined for |x| <= 1; the result lies in [0, pi]. Values outside the domain are rejected with a domain violation.

- Module: `scientific` (version 1.0.0)
- Modes: scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/scientific.md#acos

## Parameters

- `x` — Value in [-1, 1]. (any value)

## Output

Function value as float64.

## Examples

### arccosine of one

```json
{
  "x": {
    "kind": "integer",
    "value": "1"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0"}}`

### outside the domain

```json
{
  "x": {
    "kind": "integer",
    "value": "2"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="acosh"></a>

## acosh

Inverse hyperbolic cosine of a real number.

Defined for x >= 1; values below one are rejected with a domain violation.

- Module: `scientific` (version 1.0.0)
- Modes: scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/scientific.md#acosh

## Parameters

- `x` — Value greater than or equal to one. (any value)

## Output

Function value as float64.

## Examples

### inverse hyperbolic cosine of one

```json
{
  "x": {
    "kind": "integer",
    "value": "1"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0"}}`

### below the domain

```json
{
  "x": {
    "kind": "decimal",
    "value": "0.5"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="asin"></a>

## asin

Inverse sine in radians.

Defined for |x| <= 1; the result lies in [-pi/2, pi/2]. Values outside the domain are rejected with a domain violation.

- Module: `scientific` (version 1.0.0)
- Modes: scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/scientific.md#asin

## Parameters

- `x` — Value in [-1, 1]. (any value)

## Output

Function value as float64.

## Examples

### arcsine of one

```json
{
  "x": {
    "kind": "integer",
    "value": "1"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"1.5707963267948966"}}`

### outside the domain

```json
{
  "x": {
    "kind": "integer",
    "value": "2"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="asinh"></a>

## asinh

Inverse hyperbolic sine of a real number.

Defined for every finite real number.

- Module: `scientific` (version 1.0.0)
- Modes: scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/scientific.md#asinh

## Parameters

- `x` — Real number. (any value)

## Output

Function value as float64.

## Examples

### inverse hyperbolic sine of zero

```json
{
  "x": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0"}}`


<a id="atan"></a>

## atan

Inverse tangent in radians.

Defined for every finite real number; the result lies in (-pi/2, pi/2).

- Module: `scientific` (version 1.0.0)
- Modes: scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/scientific.md#atan

## Parameters

- `x` — Real number. (any value)

## Output

Function value as float64.

## Examples

### arctangent of one

```json
{
  "x": {
    "kind": "integer",
    "value": "1"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0.7853981633974483"}}`


<a id="atan2"></a>

## atan2

Angle of the point (x, y) in radians.

Returns the angle in radians between the positive x-axis and the point (x, y), in (-pi, pi]. Both arguments must be plain numbers or quantities with the same dimension; the ratio is then dimensionless. Mixed dimensions are rejected.

- Module: `scientific` (version 1.0.0)
- Modes: scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/scientific.md#atan2

## Parameters

- `y` — Ordinate. (any value)
- `x` — Abscissa. (any value)

## Output

Angle in radians.

## Examples

### first quadrant diagonal

```json
{
  "x": {
    "kind": "integer",
    "value": "1"
  },
  "y": {
    "kind": "integer",
    "value": "1"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0.7853981633974483"}}`

### mismatched dimensions

```json
{
  "x": {
    "kind": "integer",
    "value": "1"
  },
  "y": {
    "kind": "quantity",
    "value": {
      "kind": "integer",
      "value": "1"
    },
    "dimension": {
      "length": 1
    }
  }
}
```

Expected: `{"type":"error","code":"incompatible_units"}`


<a id="atanh"></a>

## atanh

Inverse hyperbolic tangent of a real number.

Defined for |x| < 1; the endpoints are rejected with a domain violation.

- Module: `scientific` (version 1.0.0)
- Modes: scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/scientific.md#atanh

## Parameters

- `x` — Value strictly between -1 and 1. (any value)

## Output

Function value as float64.

## Examples

### inverse hyperbolic tangent of zero

```json
{
  "x": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0"}}`

### at the pole

```json
{
  "x": {
    "kind": "integer",
    "value": "1"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="bessel_j0"></a>

## bessel_j0

Bessel function of the first kind of order zero.

Computes J0(x). The power series is used for |x| < 12 and the asymptotic expansion for |x| >= 12. Relative error is a few ulps in the series region and at most about 1e-9 in the asymptotic region, degrading near the zeros of the function.

- Module: `scientific` (version 1.0.0)
- Modes: scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/scientific.md#bessel_j0

## Parameters

- `x` — Real argument. (any value)

## Output

Function value as float64.

## Examples

### Bessel J0 at zero

```json
{
  "x": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"1"}}`

### Bessel J0 at one

```json
{
  "x": {
    "kind": "integer",
    "value": "1"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0.7651976865579666"}}`


<a id="bessel_j1"></a>

## bessel_j1

Bessel function of the first kind of order one.

Computes J1(x). The power series is used for |x| < 12 and the asymptotic expansion for |x| >= 12; J1 is odd, so the sign is flipped for negative arguments. Relative error is a few ulps in the series region and at most about 1e-9 in the asymptotic region, degrading near the zeros of the function.

- Module: `scientific` (version 1.0.0)
- Modes: scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/scientific.md#bessel_j1

## Parameters

- `x` — Real argument. (any value)

## Output

Function value as float64.

## Examples

### Bessel J1 at zero

```json
{
  "x": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0"}}`

### Bessel J1 at one

```json
{
  "x": {
    "kind": "integer",
    "value": "1"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0.44005058574493355"}}`


<a id="beta"></a>

## beta

Euler beta function of two positive real arguments.

Computes beta(a, b) = Gamma(a) Gamma(b) / Gamma(a + b) as exp(lgamma(a) + lgamma(b) - lgamma(a + b)). Both arguments must be strictly positive; other values are rejected with a domain violation. Relative accuracy is near 1e-15.

- Module: `scientific` (version 1.0.0)
- Modes: scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/scientific.md#beta

## Parameters

- `a` — First positive real argument. (any value)
- `b` — Second positive real argument. (any value)

## Output

Beta function value as float64.

## Examples

### beta of two and three

```json
{
  "a": {
    "kind": "integer",
    "value": "2"
  },
  "b": {
    "kind": "integer",
    "value": "3"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0.08333333333333355"}}`

### non-positive argument

```json
{
  "a": {
    "kind": "integer",
    "value": "0"
  },
  "b": {
    "kind": "integer",
    "value": "3"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="cbrt"></a>

## cbrt

Real cube root of a number.

Defined for every finite real number, including negatives. The result is the binary64 cube root approximation.

- Module: `scientific` (version 1.0.0)
- Modes: scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/scientific.md#cbrt

## Parameters

- `x` — Real number. (any value)

## Output

Real cube root of x.

## Examples

### cube root of a perfect cube

```json
{
  "x": {
    "kind": "integer",
    "value": "27"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"3"}}`

### cube root of a negative number

```json
{
  "x": {
    "kind": "integer",
    "value": "-8"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"-2"}}`


<a id="constants"></a>

## constants

Correctly rounded binary64 mathematical constants.

Returns pi, e, tau = 2 * pi, ln 2, and ln 10 as the correctly rounded binary64 values of the real constants. No parameters.

- Module: `scientific` (version 1.0.0)
- Modes: scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/scientific.md#constants

## Parameters


## Output

Record with float64 fields pi, e, tau, ln2, and ln10.

## Examples

### binary64 constants

```json
{}
```

Expected: `{"type":"value","value":{"e":{"kind":"float64","value":"2.718281828459045"},"ln10":{"kind":"float64","value":"2.302585092994046"},"ln2":{"kind":"float64","value":"0.6931471805599453"},"pi":{"kind":"float64","value":"3.141592653589793"},"tau":{"kind":"float64","value":"6.283185307179586"}}}`


<a id="cos"></a>

## cos

Cosine of an angle in radians.

Accepts a plain number interpreted as radians or a quantity with the angle dimension. The result is the correctly rounded binary64 cosine approximation.

- Module: `scientific` (version 1.0.0)
- Modes: scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/scientific.md#cos

## Parameters

- `x` — Angle in radians, or an angle quantity. (any value)

## Output

Function value as float64.

## Examples

### cosine of zero

```json
{
  "x": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"1"}}`


<a id="cosh"></a>

## cosh

Hyperbolic cosine of a real number.

Defined for every finite real number; the result is always at least one.

- Module: `scientific` (version 1.0.0)
- Modes: scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/scientific.md#cosh

## Parameters

- `x` — Real number. (any value)

## Output

Function value as float64.

## Examples

### hyperbolic cosine of zero

```json
{
  "x": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"1"}}`


<a id="degrees_to_radians"></a>

## degrees_to_radians

Convert an angle in degrees to radians.

Returns an angle quantity whose float64 payload is x * (pi / 180). The input is a plain real number in degrees.

- Module: `scientific` (version 1.0.0)
- Modes: scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/scientific.md#degrees_to_radians

## Parameters

- `x` — Real number in the source unit. (any value)

## Output

Angle quantity with a float64 payload in the target unit.

## Examples

### zero

```json
{
  "x": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"value","value":{"kind":"quantity","value":{"kind":"float64","value":"0"},"dimension":{"angle":1}}}`

### straight angle

```json
{
  "x": {
    "kind": "integer",
    "value": "180"
  }
}
```

Expected: `{"type":"value","value":{"kind":"quantity","value":{"kind":"float64","value":"3.141592653589793"},"dimension":{"angle":1}}}`


<a id="differentiate"></a>

## differentiate

Numerical derivative of a restricted expression by central differences.

Central-difference estimates of the first through fourth derivative of a restricted expression at a point. The expression uses the same restricted grammar as scientific.root_find. With method "richardson" (the default) the estimates at h, h/2, and h/4 are combined by Richardson extrapolation, which cancels the leading O(h^2) error and returns the two-level estimate with |r2 - r1| as the error estimate. With method "central" the plain h/2 central difference is returned with the h-extrapolated difference as the error estimate. Orders above four are rejected.

- Module: `scientific` (version 1.0.0)
- Modes: scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/scientific.md#differentiate

## Parameters

- `expression` — Restricted expression in the variable. (restricted expression in x)
- `variable` — Name of the single variable used by the expression. (text)
- `at` — Point at which to differentiate. (any value)
- `order` (optional) — Derivative order between 1 and 4; default 1. (integer)
- `method` (optional) — Difference scheme: "richardson" (default) or "central". (one of ["richardson", "central"])

## Output

Derivative estimate, error estimate, step, method, and order.

## Examples

### second derivative of x squared at zero

```json
{
  "at": {
    "kind": "integer",
    "value": "0"
  },
  "expression": "x^2",
  "order": {
    "kind": "integer",
    "value": "2"
  },
  "variable": "x"
}
```

Expected: `{"type":"value","value":{"error_estimate":{"kind":"float64","value":"0"},"method":"richardson","order":{"kind":"integer","value":"2"},"step":{"kind":"float64","value":"0.0025"},"value":{"kind":"float64","value":"2"}}}`

### order above the supported maximum

```json
{
  "at": {
    "kind": "integer",
    "value": "0"
  },
  "expression": "x^2",
  "order": {
    "kind": "integer",
    "value": "5"
  },
  "variable": "x"
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="digamma"></a>

## digamma

Logarithmic derivative of the gamma function.

Computes psi(x) = d/dx ln Gamma(x) by raising the argument with the recurrence psi(x + 1) = psi(x) + 1/x to at least ten and evaluating the Bernoulli asymptotic expansion there, with reflection for x < 0.5. Defined for every real x except the poles at zero and the negative integers, which are rejected with a domain violation.

- Module: `scientific` (version 1.0.0)
- Modes: scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/scientific.md#digamma

## Parameters

- `x` — Real argument. (any value)

## Output

Function value as float64.

## Examples

### digamma of one

```json
{
  "x": {
    "kind": "integer",
    "value": "1"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"-0.5772156649015324"}}`

### pole at zero

```json
{
  "x": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="erf"></a>

## erf

Error function of a real argument.

Computes erf(x) = 2/sqrt(pi) * integral of exp(-t^2) from 0 to x through the regularized lower incomplete gamma function P(1/2, x^2), evaluated with a series for x^2 < 3/2 and a continued fraction otherwise. The absolute error is near 1e-15.

- Module: `scientific` (version 1.0.0)
- Modes: scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/scientific.md#erf

## Parameters

- `x` — Real argument. (any value)

## Output

Function value as float64.

## Examples

### error function of zero

```json
{
  "x": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0"}}`

### error function of one

```json
{
  "x": {
    "kind": "integer",
    "value": "1"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0.8427007929497154"}}`


<a id="erfc"></a>

## erfc

Complementary error function 1 - erf(x).

Computes erfc(x) = 1 - erf(x) directly from the regularized upper incomplete gamma function Q(1/2, x^2), so the positive tail keeps full relative accuracy instead of cancelling. The absolute error is near 1e-15 and the relative error stays small far into the tail.

- Module: `scientific` (version 1.0.0)
- Modes: scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/scientific.md#erfc

## Parameters

- `x` — Real argument. (any value)

## Output

Function value as float64.

## Examples

### complementary error function of zero

```json
{
  "x": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"1"}}`

### complementary error function of three

```json
{
  "x": {
    "kind": "integer",
    "value": "3"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0.000022090496998585465"}}`


<a id="exp"></a>

## exp

Natural exponential of a real number.

Defined for every finite real number; overflow to a non-finite value is reported as a domain violation.

- Module: `scientific` (version 1.0.0)
- Modes: scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/scientific.md#exp

## Parameters

- `x` — Real number. (any value)

## Output

Function value as float64.

## Examples

### exponential of zero

```json
{
  "x": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"1"}}`


<a id="expm1"></a>

## expm1

Accurate exp(x) - 1 for small x.

Computes exp(x) - 1 without the cancellation of computing exp(x) first. Defined for every finite real number.

- Module: `scientific` (version 1.0.0)
- Modes: scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/scientific.md#expm1

## Parameters

- `x` — Real number. (any value)

## Output

Function value as float64.

## Examples

### expm1 of zero

```json
{
  "x": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0"}}`


<a id="gamma"></a>

## gamma

Gamma function of a real argument.

Computes Gamma(x) with the Lanczos approximation (g = 7, n = 9) for x >= 0.5 and the reflection formula Gamma(x) Gamma(1 - x) = pi / sin(pi x) below. Defined for every real x except the poles at zero and the negative integers, which are rejected with a domain violation. Relative accuracy is near 1e-15.

- Module: `scientific` (version 1.0.0)
- Modes: scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/scientific.md#gamma

## Parameters

- `x` — Real argument. (any value)

## Output

Function value as float64.

## Examples

### gamma of five

```json
{
  "x": {
    "kind": "integer",
    "value": "5"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"23.999999999999996"}}`

### pole at zero

```json
{
  "x": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="integrate"></a>

## integrate

Definite integral of a restricted expression by adaptive Simpson quadrature.

Adaptive Simpson quadrature with Richardson extrapolation over [lower, upper]. The expression uses the same restricted grammar as scientific.root_find. The returned error_estimate is the accumulated Richardson estimate, not a rigorous bound. Non-convergence or an exhausted evaluation budget is reported as an error. Defaults: tolerance 1e-12 and 100000 evaluations, bounded by the execution context.

- Module: `scientific` (version 1.0.0)
- Modes: scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/scientific.md#integrate

## Parameters

- `expression` — Restricted expression in the variable. (restricted expression in x)
- `variable` — Name of the single variable used by the expression. (text)
- `lower` — Lower integration limit. (any value)
- `upper` — Upper integration limit. (any value)
- `tolerance` (optional) — Absolute error tolerance; default 1e-12. (any value)
- `max_evaluations` (optional) — Evaluation budget; default 100000, never above the context limit. (integer)

## Output

Integral estimate, Richardson error estimate, evaluation count, method, and convergence flag.

## Examples

### integral of x on [0, 1]

```json
{
  "expression": "x",
  "lower": {
    "kind": "integer",
    "value": "0"
  },
  "upper": {
    "kind": "integer",
    "value": "1"
  },
  "variable": "x"
}
```

Expected: `{"type":"value","value":{"converged":true,"error_estimate":{"kind":"float64","value":"0"},"evaluations":{"kind":"integer","value":"5"},"integral":{"kind":"float64","value":"0.5"},"method":"adaptive_simpson"}}`

### non-finite integrand

```json
{
  "expression": "ln(x)",
  "lower": {
    "kind": "integer",
    "value": "-1"
  },
  "upper": {
    "kind": "integer",
    "value": "1"
  },
  "variable": "x"
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="lgamma"></a>

## lgamma

Natural logarithm of the absolute value of the gamma function.

Computes ln|Gamma(x)| with the same Lanczos approximation and reflection as scientific.gamma. Defined for every real x except the poles at zero and the negative integers, which are rejected with a domain violation. Relative accuracy is near 1e-15.

- Module: `scientific` (version 1.0.0)
- Modes: scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/scientific.md#lgamma

## Parameters

- `x` — Real argument. (any value)

## Output

Function value as float64.

## Examples

### log-gamma of ten

```json
{
  "x": {
    "kind": "integer",
    "value": "10"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"12.801827480081474"}}`

### pole at zero

```json
{
  "x": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="ln"></a>

## ln

Natural logarithm of a positive real number.

Defined for x > 0; zero and negative values are rejected with a domain violation.

- Module: `scientific` (version 1.0.0)
- Modes: scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/scientific.md#ln

## Parameters

- `x` — Positive real number. (any value)

## Output

Function value as float64.

## Examples

### logarithm of one

```json
{
  "x": {
    "kind": "integer",
    "value": "1"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0"}}`

### negative input

```json
{
  "x": {
    "kind": "integer",
    "value": "-1"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="log"></a>

## log

Logarithm of x to a positive base other than one.

Returns log_base(x) = ln(x) / ln(base), defined for x > 0, base > 0, and base != 1.

- Module: `scientific` (version 1.0.0)
- Modes: scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/scientific.md#log

## Parameters

- `x` — Positive real number. (any value)
- `base` — Positive real number different from one. (any value)

## Output

Logarithm of x in the given base.

## Examples

### base two logarithm

```json
{
  "base": {
    "kind": "integer",
    "value": "2"
  },
  "x": {
    "kind": "integer",
    "value": "8"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"3"}}`

### base one is undefined

```json
{
  "base": {
    "kind": "integer",
    "value": "1"
  },
  "x": {
    "kind": "integer",
    "value": "8"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="log10"></a>

## log10

Base-10 logarithm of a positive real number.

Defined for x > 0; zero and negative values are rejected with a domain violation.

- Module: `scientific` (version 1.0.0)
- Modes: scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/scientific.md#log10

## Parameters

- `x` — Positive real number. (any value)

## Output

Function value as float64.

## Examples

### logarithm of one

```json
{
  "x": {
    "kind": "integer",
    "value": "1"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0"}}`

### zero input

```json
{
  "x": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="log1p"></a>

## log1p

Accurate ln(1 + x) for small x.

Computes ln(1 + x) without the cancellation of computing 1 + x first. Defined for x > -1; other values are rejected with a domain violation.

- Module: `scientific` (version 1.0.0)
- Modes: scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/scientific.md#log1p

## Parameters

- `x` — Real number greater than -1. (any value)

## Output

Function value as float64.

## Examples

### log1p of zero

```json
{
  "x": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0"}}`

### below the domain

```json
{
  "x": {
    "kind": "integer",
    "value": "-2"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="log2"></a>

## log2

Base-2 logarithm of a positive real number.

Defined for x > 0; zero and negative values are rejected with a domain violation.

- Module: `scientific` (version 1.0.0)
- Modes: scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/scientific.md#log2

## Parameters

- `x` — Positive real number. (any value)

## Output

Function value as float64.

## Examples

### logarithm of one

```json
{
  "x": {
    "kind": "integer",
    "value": "1"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0"}}`

### negative input

```json
{
  "x": {
    "kind": "integer",
    "value": "-1"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="nth_root"></a>

## nth_root

Real n-th root for a positive integer degree.

Computes the real n-th root of x. The degree n must be a positive integer. Odd degrees are defined for negative x; even degrees require x >= 0. For perfect powers the result is refined with Newton iterations so that exact roots are returned exactly.

- Module: `scientific` (version 1.0.0)
- Modes: scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/scientific.md#nth_root

## Parameters

- `x` — Real number. (any value)
- `n` — Positive integer degree. (integer)

## Output

Real n-th root of x.

## Examples

### odd root of a negative number

```json
{
  "n": {
    "kind": "integer",
    "value": "3"
  },
  "x": {
    "kind": "integer",
    "value": "-27"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"-3"}}`

### even root of a negative number

```json
{
  "n": {
    "kind": "integer",
    "value": "2"
  },
  "x": {
    "kind": "integer",
    "value": "-4"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="ode_rk45"></a>

## ode_rk45

Adaptive Dormand-Prince 5(4) integration of dy/dt = f(t, y).

Solves the scalar initial value problem dy/dt = f(t, y) with y(t0) = initial_value on the interval from t0 to t1 using the Dormand-Prince 5(4) embedded Runge-Kutta pair with adaptive step-size control. The expression may reference the independent variable t and the dependent variable named by `variable` (which must not be t); it uses the same restricted grammar as scientific.root_find. The returned solution contains the initial point and every accepted step. The step-size control uses the tolerance as both relative and absolute tolerance. If max_steps is exhausted before t1 the call fails with a non-convergence error instead of returning a partial solution as a success. Defaults: step (t1 - t0) / 100, tolerance 1e-8, max_steps 10000, bounded by the execution context.

- Module: `scientific` (version 1.0.0)
- Modes: scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/scientific.md#ode_rk45

## Parameters

- `expression` — Restricted expression for f(t, y) in the variables t and the dependent variable. (restricted expression in t, y)
- `variable` — Name of the dependent variable; t is reserved for the independent variable. (text)
- `initial_value` — Value of the dependent variable at t0. (any value)
- `t0` — Start of the integration interval. (any value)
- `t1` — End of the integration interval. (any value)
- `step` (optional) — Initial step size; default (t1 - t0) / 100. (any value)
- `tolerance` (optional) — Relative and absolute error tolerance; default 1e-8. (any value)
- `max_steps` (optional) — Attempted step cap; default 10000, never above the context limit. (integer)

## Output

Accepted solution points, final values, step counts, method, and convergence flag.

## Examples

### constant slope from zero to one

```json
{
  "expression": "1",
  "initial_value": {
    "kind": "integer",
    "value": "0"
  },
  "step": {
    "kind": "integer",
    "value": "1"
  },
  "t0": {
    "kind": "integer",
    "value": "0"
  },
  "t1": {
    "kind": "integer",
    "value": "1"
  },
  "variable": "y"
}
```

Expected: `{"type":"value","value":{"accepted_steps":{"kind":"integer","value":"1"},"converged":true,"final_t":{"kind":"float64","value":"1"},"final_y":{"kind":"float64","value":"1"},"method":"dormand_prince_45","rejected_steps":{"kind":"integer","value":"0"},"solution":[{"t":{"kind":"float64","value":"0"},"y":{"kind":"float64","value":"0"}},{"t":{"kind":"float64","value":"1"},"y":{"kind":"float64","value":"1"}}],"steps":{"kind":"integer","value":"1"}}}`

### step budget exhausted

```json
{
  "expression": "y",
  "initial_value": {
    "kind": "integer",
    "value": "1"
  },
  "max_steps": {
    "kind": "integer",
    "value": "1"
  },
  "step": {
    "kind": "decimal",
    "value": "0.1"
  },
  "t0": {
    "kind": "integer",
    "value": "0"
  },
  "t1": {
    "kind": "integer",
    "value": "1"
  },
  "variable": "y"
}
```

Expected: `{"type":"error","code":"non_convergence"}`


<a id="radians_to_degrees"></a>

## radians_to_degrees

Convert an angle in radians to degrees.

Returns an angle quantity whose float64 payload is x * (180 / pi). The input is a plain real number in radians.

- Module: `scientific` (version 1.0.0)
- Modes: scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/scientific.md#radians_to_degrees

## Parameters

- `x` — Real number in the source unit. (any value)

## Output

Angle quantity with a float64 payload in the target unit.

## Examples

### zero

```json
{
  "x": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"value","value":{"kind":"quantity","value":{"kind":"float64","value":"0"},"dimension":{"angle":1}}}`

### straight angle

```json
{
  "x": {
    "kind": "decimal",
    "value": "3.141592653589793"
  }
}
```

Expected: `{"type":"value","value":{"kind":"quantity","value":{"kind":"float64","value":"180"},"dimension":{"angle":1}}}`


<a id="root_find"></a>

## root_find

Find a root of a scalar expression on a sign-change bracket.

Brent's method on the required bracket [lower, upper], which must satisfy f(lower) * f(upper) <= 0. The expression is a restricted string using the named variable, numeric literals, parentheses, + - * / % ^, and the elementary functions sin, cos, tan, asin, acos, atan, atan2, sinh, cosh, tanh, exp, ln, log, log10, log2, sqrt, cbrt, abs, floor, ceil, round, min, max, and pow. Non-convergence is reported as an error; the last iterate is never returned as a success. Defaults: tolerance 1e-12 and 1000 iterations, bounded by the execution context.

- Module: `scientific` (version 1.0.0)
- Modes: scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/scientific.md#root_find

## Parameters

- `expression` — Restricted expression in the variable. (restricted expression in x)
- `variable` — Name of the single variable used by the expression. (text)
- `lower` — Lower bracket endpoint. (any value)
- `upper` — Upper bracket endpoint. (any value)
- `tolerance` (optional) — Absolute x tolerance; default 1e-12. (any value)
- `max_iterations` (optional) — Iteration cap; default 1000, never above the context limit. (integer)

## Output

Root, iteration and evaluation counts, residual, convergence flag, method, and bracket.

## Examples

### root of x - 1 on [0, 2]

```json
{
  "expression": "x - 1",
  "lower": {
    "kind": "integer",
    "value": "0"
  },
  "upper": {
    "kind": "integer",
    "value": "2"
  },
  "variable": "x"
}
```

Expected: `{"type":"value","value":{"bracket":{"lower":{"kind":"float64","value":"0"},"upper":{"kind":"float64","value":"2"}},"converged":true,"evaluations":{"kind":"integer","value":"3"},"iterations":{"kind":"integer","value":"2"},"method":"brent","residual":{"kind":"float64","value":"0"},"root":{"kind":"float64","value":"1"}}}`

### no sign change

```json
{
  "expression": "x^2 + 1",
  "lower": {
    "kind": "integer",
    "value": "-1"
  },
  "upper": {
    "kind": "integer",
    "value": "1"
  },
  "variable": "x"
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="sin"></a>

## sin

Sine of an angle in radians.

Accepts a plain number interpreted as radians or a quantity with the angle dimension. The result is the correctly rounded binary64 sine approximation.

- Module: `scientific` (version 1.0.0)
- Modes: scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/scientific.md#sin

## Parameters

- `x` — Angle in radians, or an angle quantity. (any value)

## Output

Function value as float64.

## Examples

### sine of zero

```json
{
  "x": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0"}}`

### length is not an angle

```json
{
  "x": {
    "kind": "quantity",
    "value": {
      "kind": "integer",
      "value": "1"
    },
    "dimension": {
      "length": 1
    }
  }
}
```

Expected: `{"type":"error","code":"incompatible_units"}`


<a id="sinh"></a>

## sinh

Hyperbolic sine of a real number.

Defined for every finite real number; overflow to a non-finite value is reported as a domain violation.

- Module: `scientific` (version 1.0.0)
- Modes: scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/scientific.md#sinh

## Parameters

- `x` — Real number. (any value)

## Output

Function value as float64.

## Examples

### hyperbolic sine of zero

```json
{
  "x": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0"}}`


<a id="tan"></a>

## tan

Tangent of an angle in radians.

Accepts a plain number interpreted as radians or a quantity with the angle dimension. Binary64 cannot represent pi/2 exactly, so the tangent is finite but very large near odd multiples of pi/2.

- Module: `scientific` (version 1.0.0)
- Modes: scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/scientific.md#tan

## Parameters

- `x` — Angle in radians, or an angle quantity. (any value)

## Output

Function value as float64.

## Examples

### tangent of zero

```json
{
  "x": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0"}}`


<a id="tanh"></a>

## tanh

Hyperbolic tangent of a real number.

Defined for every finite real number; the result lies in (-1, 1).

- Module: `scientific` (version 1.0.0)
- Modes: scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/scientific.md#tanh

## Parameters

- `x` — Real number. (any value)

## Output

Function value as float64.

## Examples

### hyperbolic tangent of zero

```json
{
  "x": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0"}}`


<a id="zeta"></a>

## zeta

Riemann zeta function for real arguments greater than one.

Computes zeta(s) for real s > 1 with Euler-Maclaurin summation: nineteen leading terms are summed directly and the tail is replaced by its expansion at N = 20 with ten Bernoulli correction terms. The domain is s > 1; s <= 1 is rejected with a domain violation. Absolute error is near 1e-15 for s >= 2.

- Module: `scientific` (version 1.0.0)
- Modes: scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/scientific.md#zeta

## Parameters

- `x` — Real argument. (any value)

## Output

Function value as float64.

## Examples

### zeta of two

```json
{
  "x": {
    "kind": "integer",
    "value": "2"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"1.6449340668482266"}}`

### outside the domain

```json
{
  "x": {
    "kind": "integer",
    "value": "1"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


