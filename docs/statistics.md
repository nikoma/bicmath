# Statistical interpretation guidance

This document explains the conventions BicMath uses and what its outputs do and
do not mean. The engine reports method, assumptions, and diagnostics with every
result; it does not automatically identify causal validity in arbitrary
datasets.

## Descriptive statistics

- **Variance** is `sum((x - mean)^2) / (n - ddof)`. `ddof = 0` is the
  population variance, `ddof = 1` the unbiased sample variance. The engine
  requires `n > ddof`; one observation with `ddof = 1` is an
  `insufficient_observations` error.
- **Quantiles** use linear interpolation (the R-7 definition) by default:
  `h = (n - 1) q`, then interpolate between the two neighbouring order
  statistics. Alternative methods (`lower`, `higher`, `midpoint`, `nearest`)
  are explicit parameters. Numeric sorting is mandatory; input arrays are never
  mutated.
- **Weights** are either `frequency` (non-negative integers) or `reliability`
  (non-negative numbers). Negative weights and zero total weight are rejected.
  Missing or non-finite observations are never silently dropped.
- **Zero-variance inputs**: Pearson correlation returns a domain error rather
  than NaN. Covariance is still defined.
- Exact inputs produce exact results where mathematically possible (for
  example variance of `[1,2,3]` is the rational `2/3` for `ddof = 0` and the
  integer `1` for `ddof = 1`).

## Distributions

Normal, Student-t, binomial, and chi-square evaluations and quantiles are
implemented from scratch with documented algorithms (erf/erfc, Lanczos
`lgamma`, regularized incomplete beta and gamma with iteration bounds).
Inverse functions use bracketing plus refinement, and round trips are tested.
Survival functions are computed directly (via `erfc` or the complementary
regularized functions), not as `1 - cdf`, so small tail probabilities remain
accurate.

## Confidence intervals and comparisons

Methods are always named and assumptions always reported:

- `statistics.ci_mean`: `t` (default) or `z`.
- `statistics.ci_proportion`: `wilson` (default) or `wald`. Wald warns when
  `n·p < 5` or `n·(1-p) < 5`.
- `statistics.welch_ci`: independent two-sample difference of means with
  Welch–Satterthwaite degrees of freedom. Equal-variance and paired designs are
  not silently substituted.
- `statistics.proportions_difference`: `newcombe` (default) or `wald`.
- `statistics.chi_square_contingency`: Pearson chi-square with expected counts
  and assumption diagnostics (minimum expected count, cells below 5).

A frequentist interval or p-value is never relabelled as
`probability_true_effect_positive`. Bayesian posterior probabilities require an
explicitly implemented Bayesian model and prior; that is outside this release.

## Power and sample size

`statistics.sample_size_two_means`, `sample_size_two_proportions`,
`power_two_means`, and `power_two_proportions` use normal approximations and
label the method. Alpha, target power, effect size, allocation ratio, and
sidedness are explicit. A power estimate is a probability under planning
assumptions; it is not a promise that a future confidence interval excludes
zero. Observed or post-hoc power is not presented as independent evidence of an
established effect.

## Stratified experiments and contribution outcomes

`statistics.stratified_experiment` accepts per-stratum treatment/control
observations or sufficient statistics, target-population weights, and an
optional known fixed cost. It:

- computes per-arm means and **unbiased sample variances of the complete
  per-assigned-person outcome** from categorical counts,
- standardizes to the supplied target mix (`w_s = target_weight_s / Σ
  target_weight`), never letting pooled unequal segment composition silently
  replace the specified mix,
- reports the standardized difference, its variance, standard error, and a
  normal-approximation confidence interval,
- reports positive-outcome, negative-outcome, and net-positive rate
  differences in separate blocks, because a positive purchase effect is not the
  same as a positive contribution effect,
- scales expected differences and variance by `rollout_fraction`
  (`variance × fraction²`), while a known fixed cost shifts the estimate and
  interval without adding sampling variance,
- emits the pooled rates for diagnostics only and warns when the observed mix
  differs from the target weights.

For monetary categorical outcomes such as retained (+100), refunded (−10), and
no purchase (0), the mean and variance are computed over the full assigned
population. Retained and refunded purchases are mutually exclusive outcomes,
not independent samples; their dependence is preserved by using the complete
categorical distribution.

Refunds are a subset of purchasers. Excluding refunders from the analysis
changes the analysis population using a post-assignment outcome; that is not
the original intention-to-treat comparison, and the engine does not do it
silently.

`statistics.prospective_pool` combines original and hypothetical additional
data. Its output is labelled `prospective: true` and carries a warning that it
is not an observed result; original-study uncertainty, hypothetical pooled
uncertainty, and future realized-profit variability are kept distinct.

## What the engine cannot conclude

- Whether a holdout is economically worthwhile without the future decision,
  timing, and cost assumptions.
- That randomization, absence of interference, or future comparability hold.
- That a positive point estimate guarantees financial success.

These remain the caller's responsibility, and the diagnostics say so.
