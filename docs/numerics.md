# Numeric types, conversion, precision, and wire format

## Scalar representations

| Kind | Storage | Exactness | Notes |
|---|---|---|---|
| `integer` | arbitrary precision `BigInt` | exact | bounded by configured bit limits |
| `rational` | normalized `BigInt` numerator / positive denominator | exact | always reduced, denominator positive |
| `decimal` | `BigInt` mantissa × 10^-scale | exact until rounded | scale is preserved for display |
| `float64` | finite IEEE-754 binary64 | approximate | only in scientific mode |

## Wire format

Canonical scalar encoding, used everywhere including nested arrays and records:

```json
{"kind":"integer","value":"9007199254740993"}
{"kind":"decimal","value":"0.10"}
{"kind":"rational","numerator":"1","denominator":"3"}
{"kind":"float64","value":"0.3333333333333333"}
```

Exact numerical payloads are strings; they never pass through a floating-point
parser. Plain JSON integers and decimals are interpreted as exact integers and
decimals. A decimal-string shorthand (for example `"0.10"`) is accepted only
for parameters explicitly declared numeric, and only outside nested records.

Structured values:

```json
{"kind":"quantity","value":{"kind":"decimal","value":"9.81"},"dimension":{"length":1,"time":-2}}
{"kind":"money","amount":{"kind":"decimal","value":"10.00"},"currency":"USD"}
{"kind":"matrix","rows":2,"cols":2,"data":[1,2,3,4]}
{"kind":"bound","unbounded":true}
{"kind":"bound","value":{"kind":"decimal","value":"0"}}
```

Plain JSON objects are records. Record keys must not be the reserved key
`"kind"`; validation rejects such records. Currency identifiers are 2–12
uppercase letters/digits starting with a letter.

Lexical rules for numeric literals: optional sign, decimal point, optional
`e`/`E` exponent, no whitespace, no thousands separators, and no locale
decimal comma. Locale-dependent input must be normalized by a separate import
layer before it reaches the engine.

## Promotion and conversion

1. Float64 is never mixed into exact arithmetic silently. Any operation
   involving a Float64 requires `scientific` mode; otherwise the engine
   returns `unsupported_numeric_mode`.
2. Integer `+ - *` and integer powers are integers; integer division is an
   integer when exact and a normalized rational otherwise.
3. Rational operations stay rational.
4. Decimal `+ - *` with decimals/integers is exact.
5. Decimal division rounds to the context precision, reports `rounded`, and
   sets the inexact flag. In `exact` mode it is rejected. Mixed
   rational/decimal operations that do not terminate behave the same way.
6. Comparisons are exact across representations. Float64 is decomposed to its
   exact binary rational value, so `0.1` (decimal) is **not** equal to `0.1`
   (float64), while `0.5` equals `0.5`.
7. `0^0` is defined as `1`, consistently with the empty-product convention.

A float conversion must be explicitly requested or part of an explicitly
chosen scientific mode, and is recorded in provenance.

## Precision, display scale, settlement scale

- **Computational precision** (`precision`, default 34 significant digits) is
  used only when a context rounding is required.
- **Output display scale** is the decimal scale attached to a value; `0.10`
  prints as `0.10`.
- **Currency settlement scale** is chosen per finance operation (default 2)
  and is separate from internal precision. Rounding a display never rounds an
  intermediate computation.

## Rounding modes

`half_even`, `half_away_from_zero`, `toward_zero`, `floor`, `ceiling`. Tests
cover positive and negative ties, for example `1.005 → 1.00` and
`1.015 → 1.02` under half-even.

## Errors instead of NaN/Infinity

Non-finite results are rejected at public boundaries. Division by zero, invalid
logarithms, invalid inverse-trig inputs, overflow, non-convergence, and
precision exhaustion produce structured errors. A mathematically unbounded
confidence endpoint is a tagged `bound`, not an infinity presented as a scalar.

## Canonicalization

`Decimal::canonical_string` strips trailing zeros for hashing and comparison
while the value keeps its declared scale for presentation. Signed zero is
canonicalized to positive zero for hashing. Rationals are normalized. Float64
comparison uses numeric equality; ordering uses `total_cmp`. No arbitrary
epsilon is ever used as equality for financial values.
