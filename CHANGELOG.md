# Changelog

All notable changes are recorded here. The public wire schema, module
contracts, numerical policies, and methods are versioned; a change to a
rounding default or statistical convention is a compatibility event even when
Rust types do not change.

## [0.1.0] — 2026-09-10

Initial release candidate.

### Added

- `bicmath-core`: the shared numerical contract. Arbitrary-precision integers,
  normalized rationals, exact decimals with explicit scale, and finite `f64`.
  Recursive wire values (quantities, money, matrices, tagged bounds). Stable
  error codes. Resource limits and cooperative cancellation. Typed schemas and
  module/function contracts. Versioned result envelope with exactness,
  warnings, assumptions, error estimates, traces, fingerprints, and replay
  receipts. Restricted expression AST and parser.
- `bicmath-arithmetic`: add/subtract/multiply/divide, sum, product, min, max,
  absolute value, exact comparison, integer powers, square roots with exactness
  handling, floor/ceiling/truncate, quantization with five rounding modes,
  gcd/lcm, factorial, binomial, truncated remainder, Euclidean modulo, and
  rational/decimal conversion.
- `bicmath-scientific`: trigonometric, inverse trigonometric, `atan2`,
  hyperbolic, exponential, logarithmic, `log1p`/`expm1`, roots, angle
  conversions, constants, Brent bracketed root finding, and adaptive Simpson
  integration over restricted expressions.
- `bicmath-statistics`: descriptive statistics (count, mean, weighted mean,
  median, mode, min, max, quantiles, variance, standard deviation, covariance,
  correlation, summary), normal/Student-t/binomial/chi-square distributions,
  confidence intervals, Welch and proportion comparisons, chi-square
  contingency analysis, power/sample-size planning, stratified experiment
  standardization, and prospective pooling.
- `bicmath-finance`: money arithmetic and deterministic allocation, currency
  conversion with explicit rates, percentage change, markup/margin,
  break-even, contribution analysis, simple/compound interest, present/future
  value, payments, amortization with reconciliation, periodic NPV, dated XNPV
  with four day-count conventions, and bracketed IRR/XIRR with convergence
  reporting.
- `bicmath-units`: versioned unit registry, dimensional algebra, affine
  temperature handling, temperature differences, and exact conversions.
- `bicmath-linear-algebra`: checked vectors and matrices, exact and scientific
  operations, determinant, pivoted LU, Householder QR, solving, least squares,
  and condition estimation.
- `bicmath-engine`: registry assembly and validation, shared argument
  coercion, expression evaluation, dependent batches with `$ref` references,
  and deterministic fingerprints/receipts.
- `bicmath-mcp`: official SDK (`rmcp` 3.2.0) adapter with compact and expanded
  tool profiles, stdio by default, optional Streamable HTTP, structured
  results and errors, annotations, cancellation bridging, and bounded workers.
- `bicmath`: `serve`, `modules`, `functions`, `describe`, `calculate`,
  `evaluate`, `batch`, `replay`, `doctor`, `config`, and `docs`.
- `bicmath-wasm`: browser bindings with string-safe exact values, a Node smoke
  test, and a Web Worker browser example.
- `bicmath-interval`: heuristically padded binary64 intervals with outward
  rounding, explicit `heuristic_padding_not_certified` labelling, and
  structured errors for operations that cannot be bounded.
- `bicmath-optimize`: exact rational two-phase simplex with duals and reduced
  costs, Brent and golden-section minimization, Levenberg–Marquardt least
  squares, and exact Hungarian assignment.
- `bicmath-algebra`: exact polynomial arithmetic and roots, number theory
  (primality, factorization, modular arithmetic, CRT), and combinatorics.
- `bicmath-symbolic`: differentiation (orders 1–10), simplification, Taylor
  series, round-trippable printing, substitution, and free variables.
- `bicmath-geometry`: triangle solving, polygon tools, spherical geodesy, and
  quaternion/Euler rotation.
- `bicmath-business`: unit economics, LTV/CAC, geometric CLV, arc elasticity,
  EOQ, M/M/1, Erlang C, and cohort retention/revenue.
- `bicmath-verify`: exact claim checking (equality, inequality, totals, matrix
  residuals, grid counterexample search, probability bounds) with explicit
  `scope` and `scope_limitations` on every result.
- `bicmath-format`: Markdown, LaTeX, CSV, and aligned text rendering.
- `bicmath-plan`: deterministic method recommendations with eligibility
  reasons, experiment checklists, field plans, and interpretation notes;
  missing evidence (randomization, independence, comparability) stays listed
  as unknown.
- Workflow recipes (`ab_test_full`, `loan_compare`, `cohort_ltv`,
  `budget_scenario`) composing plan, calculation, verification, and
  presentation, with per-check `established`/`not_established` fields and no
  generic `verified: true`.
- Budget presets (`fast`, `balanced`, `precise`) selecting working precision
  only, never changing method defaults or tolerances; iterative methods report
  requested tolerance, error estimate, residual, and convergence separately.
- `bicmath sdk --language typescript|python` generates typed clients from the
  registry; `units.check` performs dimensional analysis of an expression
  without evaluating it.
- Documentation: architecture/module-author guide, expression grammar, numeric
  contract, limits/privacy, MCP compatibility, reproducibility, statistical and
  finance conventions, dependency rationale, benchmark methodology, generated
  function reference, and mandatory regression tests including the end-to-end
  campaign fixture.

### Known limitations

- Transcendental float64 results are not promised to agree bit-for-bit across
  platforms.
- No symbolic algebra, proofs, general equation solving, eigenvalues, SVD,
  sparse solvers, GPU support, or Bayesian inference in this release.
- `bicmath` HTTP transport is behind the `http` Cargo feature.
