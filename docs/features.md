# BicMath feature list

BicMath is a standalone computation engine and MCP server. This document lists
what ships in the release candidate. Function counts are from the live
registry: **15 modules, 319 functions**, all with executable examples.

## Interfaces

| Interface | What it provides |
|---|---|
| MCP server | stdio by default; optional Streamable HTTP behind `--features http`; compact (6 dispatcher tools) and expanded (one typed tool per function) profiles; protocol revisions 2026-07-28 and 2025-11-25 tested end-to-end |
| CLI (`bicmath`) | `serve`, `modules`, `functions`, `describe`, `calculate`, `evaluate`, `batch`, `replay`, `doctor`, `config`, `docs`, `sdk`; JSON by default, exit codes 0/1/2 |
| Rust API | synchronous, transport-free `Engine` with typed requests, envelopes, receipts, and static module registration |
| WASM | `wasm-bindgen` bindings with string-safe exact values, Node smoke test, and a Web Worker browser example |

## Core guarantees

- **Explicit exactness**: arbitrary-precision integers, normalized rationals,
  and exact decimals stay exact; decimal division rounds only under a declared
  context and reports inexactness; float64 is always labelled approximate.
- **One implementation per operation**: CLI, MCP, expressions, batches, Rust,
  and WASM share the same registry and arithmetic.
- **Versioned result envelope**: typed result, method/function version, engine
  build identity, effective context, exactness, warnings, assumptions,
  optional error estimate, deterministic SHA-256 fingerprint, optional replay
  receipt and bounded trace.
- **Structured errors**: stable codes for malformed input, unknown/disabled
  functions, domain violations, division by zero, incompatible units,
  currency mismatch, insufficient observations, singular/ill-conditioned
  matrices, unsupported modes, precision limits, non-convergence, resource
  limits, cancellation, and batch dependency failures.
- **No arbitrary code execution**: restricted grammar (arithmetic, comparisons,
  arrays, records, qualified calls); no Rust/JS/shell/imports/remote code.
- **Resource limits and cancellation**: request bytes, expression length/tokens,
  AST depth, digits, integer bits, decimal scale, array/matrix/batch/output/
  trace sizes, exponents, factorial, iterations, operations, timeouts, and
  in-flight concurrency; cooperative cancellation with tested worker release.
- **Privacy by default**: no network, filesystem, clock, or randomness in
  numerical modules; no logging of raw financial inputs.
- **Reproducibility**: exact operations are platform-stable; fingerprints cover
  inputs, graph, method/module versions, context, seed, and policy versions;
  `bicmath replay` re-executes a receipt and checks the fingerprint.
- **Budget presets**: `fast` (15 digits), `balanced` (34, default), `precise`
  (100), overridable by explicit precision.
- **Agent workflow**: batch dependency graphs with `$ref`; four built-in
  recipes (`ab_test_full`, `loan_compare`, `cohort_ltv`, `budget_scenario`).

## Modules and functions

### arithmetic (24)
Exact arithmetic, rounding, comparison, integer/rational operations.
`add`, `sub`, `mul`, `div`, `rem`, `modulo`, `pow`, `sqrt`, `abs`, `min`,
`max`, `sum`, `product`, `compare`, `floor`, `ceil`, `trunc`, `quantize`,
`gcd`, `lcm`, `factorial`, `binomial`, `to_rational`, `to_decimal`.
Rounding modes: half-even, half-away-from-zero, toward-zero, floor, ceiling.

### scientific (38)
Elementary functions and bounded numerical methods.
`sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`, `sinh`, `cosh`, `tanh`,
`asinh`, `acosh`, `atanh`, `exp`, `expm1`, `ln`, `log1p`, `log`, `log2`,
`log10`, `cbrt`, `nth_root`, `degrees_to_radians`, `radians_to_degrees`,
`constants`, `root_find` (Brent with bracket), `integrate` (adaptive Simpson),
`differentiate` (Richardson/central, orders 1–4), `ode_rk45`
(Dormand–Prince), `gamma`, `lgamma`, `digamma`, `beta`, `erf`, `erfc`,
`zeta`, `bessel_j0`, `bessel_j1`.

### statistics (77)
- Descriptive: `count`, `mean`, `weighted_mean`, `median`, `mode`, `min`,
  `max`, `quantile` (R-7 plus alternatives), `variance`, `stddev`,
  `covariance`, `correlation`, `summary`.
- Distributions: normal (pdf/cdf/sf/quantile), Student-t (pdf/cdf/sf/
  quantile), binomial, chi-square, Poisson, exponential, uniform, lognormal,
  gamma, beta, F, negative binomial — with tail-accurate survival functions.
- Inference: `ci_mean`, `ci_proportion`, `welch_ci`, `proportions_difference`,
  `chi_square_contingency`, `tost_two_means`, `p_adjust`
  (Bonferroni/Holm/Hochberg/BH), `anova_one_way`.
- Regression: `ols`, `wls`, `logistic_regression` with diagnostics.
- Power: `sample_size_two_means`, `sample_size_two_proportions`,
  `power_two_means`, `power_two_proportions`.
- Sequential: `sprt_bernoulli`, `confidence_sequence_mean`,
  `confidence_sequence_proportion` (time-uniform).
- Bayesian conjugates: `beta_binomial_update`, `normal_normal_update`,
  `gamma_poisson_update`, `beta_posterior_probability_gt`,
  `normal_posterior_probability_gt`.
- Causal/experiments: `difference_in_differences`,
  `propensity_score_weighting`, `stratified_experiment`, `prospective_pool`.

### finance (34)
Money: `money_add`, `money_sub`, `money_scale`, `money_compare`,
`money_allocate` (largest-remainder, exact total), `convert_money`.
Rates: `percentage_change`, `markup`, `margin`, `break_even`, `contribution`.
Interest: `simple_interest`, `compound_interest`, `present_value`,
`future_value`, `payment`, `amortization` (reconciled schedules).
Cash flows: `npv` (explicit t0/t1), `xnpv` (four day-count conventions),
`irr`, `xirr` (bracketed, convergence reported, multiple roots flagged).
Derivatives: `black_scholes`, `binomial_option` (CRR).
Bonds: `bond_price`, `bond_yield`, `bond_duration`.
Portfolio: `portfolio_return`, `portfolio_volatility`, `sharpe_ratio`,
`value_at_risk`, `conditional_value_at_risk`, `max_drawdown`.
CAPM: `beta`, `capm`.

### units (11)
Versioned unit registry (length, mass, time, current, temperature, amount,
luminous, angle, area, volume, speed, pressure, energy, power) with exact
factors and qualified ids. `list_units`, `describe_unit`, `convert`,
`multiply`, `divide`, `power`, `add`, `subtract`, `temperature_difference`
(affine-aware), `is_dimensionless`, `check` (dimensional analysis of an
expression without evaluating it).

### linear_algebra (25)
Vectors: `vector_add`, `vector_sub`, `vector_scale`, `vector_dot`,
`vector_norm`. Matrices: `matrix_add`, `matrix_sub`, `matrix_scale`,
`matrix_transpose`, `matrix_multiply`, `matrix_vector_multiply`, `trace`,
`identity`, `zeros`, `ones`, `determinant`, `lu`, `qr`, `solve`,
`least_squares`, `condition_number_estimate`, `eigen_symmetric`, `svd`,
`power_iteration`, `pca`. Exact for integer/rational inputs where applicable;
scientific float64 for decompositions.

### interval (23)
Heuristically padded binary64 intervals with outward rounding. **Not
certified**: each computed endpoint is widened by a fixed number of ulps, which
is a heuristic padding rather than a proven error bound, and results are
labelled `assurance: "heuristic_padding_not_certified"` with an explicit
warning. Interior extrema are handled for `sin`/`cos`; pole-crossing `tan`,
zero-containing division, and non-positive `ln` return structured errors.
Functions: `enclose`, `from_bounds`, `add`, `sub`, `mul`, `div`, `neg`, `abs`,
`pow_int`, `sqrt`, `exp`, `ln`, `sin`, `cos`, `tan`, `intersect`, `hull`,
`contains`, `contains_value`, `midpoint`, `width`, `is_empty`, `evaluate`.

### optimize (5)
`linear_program` (exact rational two-phase simplex, Bland's rule, duals and
reduced costs), `minimize_1d` (Brent), `golden_section`,
`nonlinear_least_squares` (Levenberg–Marquardt), `linear_assignment`
(Hungarian, exact).

### algebra (28)
Polynomials: `polynomial_add`, `polynomial_sub`, `polynomial_mul`,
`polynomial_divmod`, `polynomial_gcd`, `polynomial_derivative`,
`polynomial_evaluate`, `polynomial_roots`, `polynomial_from_roots`.
Number theory: `is_prime`, `next_prime`, `prime_factors`, `divisors`,
`totient`, `is_perfect_square`, `integer_nth_root`, `gcd_extended`,
`mod_inverse`, `mod_pow`, `crt`.
Combinatorics: `permutations`, `multinomial`, `catalan`, `partition_count`,
`stirling_second`, `bell`, `fibonacci`, `lucas`.

### symbolic (6)
Exact-first symbolic layer over the restricted grammar: `derivative`
(orders 1–10), `simplify`, `taylor`, `print` (round-trippable),
`substitute`, `free_variables`.

### geometry (23)
Planar: `distance_2d`, `squared_distance_2d`, `distance_3d`,
`squared_distance_3d`, `point_line_distance_2d`, `line_intersection_2d`,
`polygon_area`, `polygon_perimeter`, `polygon_centroid`, `point_in_polygon`,
`triangle_solve` (SSS/SAS/ASA/AAS, ambiguous SSA rejected), `circle`,
`sphere`.
Geodesy: `haversine`, `initial_bearing`, `destination_point`.
Rotation: `quaternion_multiply`, `quaternion_conjugate`, `quaternion_norm`,
`quaternion_normalize`, `quaternion_rotate`, `euler_to_quaternion`,
`quaternion_to_euler`.

### business (9)
`unit_economics`, `ltv_cac`, `clv_geometric`, `arc_elasticity`, `eoq`,
`queue_mm1`, `erlang_c`, `retention_rates`, `cohort_revenue`.

### verify (7)
Independent exact-arithmetic claim checking: `equality`, `inequality`,
`expression`, `total`, `matrix_residual`, `inequality_grid`, `probability`.
Every result carries a `scope` and `scope_limitations` stating exactly what the
check established: exact checks confirm an identity at the supplied bindings,
a grid search cannot establish that a statement holds everywhere, a total
confirms arithmetic rather than provenance, and a residual confirms the
supplied solution rather than its uniqueness.

### format (5)
`to_markdown`, `to_latex`, `to_csv`, `to_text`, `number` — pure rendering of
results for reports; exact scales preserved.

### plan (4)
Rule-based guidance (no LLM): `recommend` (method selection per task/data
shape/goal), `experiment_checklist`, `describe_fields`,
`interpretation_notes`.

## Workflow recipes

Four recipes compose plan, calculation, verification, and presentation into a
single request (as `batch` nodes of type `recipe`):

| Recipe | Returns |
|---|---|
| `ab_test_full` | Standardized analysis, input-count reconciliation, interpretation notes, Markdown summary |
| `loan_compare` | Amortized offers, principal reconciliation, comparison table, rate/rounding conventions |
| `cohort_ltv` | Cohort retention and revenue, total reconciliation, table |
| `budget_scenario` | Unit economics per volume, exact whole-unit break-even coverage and minimality checks |

Each verification entry has `check`, `status`, `established`, `not_established`,
and `evidence`. There is no generic `verified: true`.

## Budget semantics

`fast` (15 digits), `balanced` (34, default), and `precise` (100) change the
working precision only. They do not alter method defaults, tolerances, or
convergence criteria: a tighter requested tolerance does not by itself produce
a more accurate answer. Iterative methods report their requested tolerance,
error estimate, residual, and convergence as separate fields, and the envelope
records the budget preset.

## Tooling and packaging

- `bicmath docs` generates the per-module function reference from the live
  registry (`docs/methods/`), so documentation cannot drift.
- `bicmath sdk --language typescript|python` generates typed client wrappers
  and the full function-id/schema map.
- `bicmath doctor` validates the install and executes every documented
  example; `bicmath replay` re-runs a receipt and checks the fingerprint.
- `scripts/check.sh` (fmt, clippy `-D warnings`, tests, feature matrix, docs,
  WASM smoke), `scripts/smoke-offline.sh`, `scripts/build-wasm.sh`,
  `scripts/license-report.sh`.
- Feature profiles: arithmetic-only; arithmetic/scientific/units; business
  (statistics/finance); full build. Runtime module disablement via config.
- CI: Linux/macOS/Windows build matrix, MCP stdio + HTTP interop, WASM build
  and Node smoke test, cargo-deny license/advisory policy.
- Packaging: Dockerfile (non-root), release workflow with checksums,
  `THIRD_PARTY_LICENSES.md`, `SECURITY.md`, `CONTRIBUTING.md`, `CHANGELOG.md`,
  versioning/deprecation policy, and `NOTICE`.

## Verification

483 tests across the workspace (unit, integration, doc), including:
hand-checkable and published reference values; property tests (rational
normalization, money conservation, matrix residuals, parser round trips);
malformed-input robustness corpus; fuzz targets for expressions, wire values,
and batches; real MCP client sessions over stdio and HTTP for both protocol
lifecycles; native/WASM parity; and the generic stratified experiment fixture
with the 85% holdout economics and original-study uncertainty.

## Known limitations

- Transcendental float64 results are not promised bit-for-bit across
  platforms; interval enclosures use documented padding rather than a fully
  verified directed-rounding implementation.
- No general CAS, proofs, arbitrary equation solving, sparse/GPU linear
  algebra, or persistent notebooks.
- `polynomial_roots` is exact for rational roots and quadratics, approximate
  otherwise; degree > 4 is unsupported.
- Exact descriptive statistics are slower than float paths (measured in
  `docs/benchmarks.md`).
- HTTP transport requires the `http` Cargo feature and is loopback-first;
  remote deployments need a trusted gateway or MCP authorization.
