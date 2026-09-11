# Statistics module reference

Descriptive statistics, probability distributions, inference, and stratified experiment analysis.

- Module id: `statistics`
- Version: 1.0.0
- Capabilities: exact_descriptive_statistics, probability_distributions, confidence_intervals, sample_size_planning, regression_models, hypothesis_testing, equivalence_testing, stratified_experiments, sequential_inference, confidence_sequences, bayesian_conjugate_updates, causal_estimators
- Supported modes: exact, auto, scientific
- Functions: 77

This file is generated from the live registry by `bicmath docs`; the function schemas, domains, and examples are the same ones the engine validates against at runtime.

<a id="anova_one_way"></a>

## anova_one_way

Classical one-way fixed-effects ANOVA F test.

groups must contain at least two non-empty arrays of observations. Computes the between-group and within-group sums of squares, the F statistic (mean square ratio), and the upper-tail p-value from the F distribution. At least one residual degree of freedom is required. Returns f_statistic, df_between, df_within, p_value, ss_between, ss_within, group_means, and method = "one_way_anova_f". A zero within-group sum of squares with a positive between-group sum of squares makes the F statistic unbounded and is rejected as a domain violation.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#anova_one_way

## Parameters

- `groups` — Groups of observations: an array of non-empty numeric arrays. (any value)

## Output

One-way ANOVA record.

## Examples

### two shifted groups

```json
{
  "groups": [
    [
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
    [
      {
        "kind": "integer",
        "value": "4"
      },
      {
        "kind": "integer",
        "value": "5"
      },
      {
        "kind": "integer",
        "value": "6"
      }
    ]
  ]
}
```

Expected: `{"type":"contains","text":"one_way_anova_f"}`

### a single group

```json
{
  "groups": [
    [
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
  ]
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="beta_binomial_update"></a>

## beta_binomial_update

Posterior summary for a Bernoulli proportion under a Beta prior.

Given successes in trials and a Beta(prior_alpha, prior_beta) prior, the posterior is Beta(prior_alpha + successes, prior_beta + trials - successes). Returns the posterior alpha and beta, the posterior mean, the posterior mode (null when both posterior shape parameters are at most 1 and the mode is not unique), and the equal-tailed credible interval at the requested confidence level, computed from the beta quantile function. method = "beta_binomial_conjugate". successes must be an integer in 0..=trials, trials must be at least 1, and the prior shape parameters must be strictly positive. The record states that the interval and any probability are posterior statements under the supplied prior and binomial model, not p-values.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#beta_binomial_update

## Parameters

- `successes` — Observed successes; integer in 0..=trials. (integer)
- `trials` — Observed trials; integer >= 1. (integer)
- `prior_alpha` — Prior Beta shape alpha; strictly positive. (number)
- `prior_beta` — Prior Beta shape beta; strictly positive. (number)
- `confidence` (optional) — Credible level in (0, 1); default 0.95. (number)

## Output

Beta-binomial posterior record with an equal-tailed credible interval.

## Examples

### uniform prior after three successes in ten trials

```json
{
  "prior_alpha": {
    "kind": "integer",
    "value": "1"
  },
  "prior_beta": {
    "kind": "integer",
    "value": "1"
  },
  "successes": {
    "kind": "integer",
    "value": "3"
  },
  "trials": {
    "kind": "integer",
    "value": "10"
  }
}
```

Expected: `{"type":"contains","text":"beta_binomial_conjugate"}`

### successes exceed trials

```json
{
  "prior_alpha": {
    "kind": "integer",
    "value": "1"
  },
  "prior_beta": {
    "kind": "integer",
    "value": "1"
  },
  "successes": {
    "kind": "integer",
    "value": "11"
  },
  "trials": {
    "kind": "integer",
    "value": "10"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="beta_cdf"></a>

## beta_cdf

Cumulative distribution function of the beta distribution.

cdf(x) = I_x(alpha, beta), the regularized incomplete beta function. x <= 0 returns 0 and x >= 1 returns 1. alpha and beta must be strictly positive.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#beta_cdf

## Parameters

- `x` — Quantile in [0, 1]. (number)
- `alpha` — First shape parameter; must be > 0. (number)
- `beta` — Second shape parameter; must be > 0. (number)

## Output

Distribution value.

## Examples

### zero quantile

```json
{
  "alpha": {
    "kind": "integer",
    "value": "2"
  },
  "beta": {
    "kind": "integer",
    "value": "2"
  },
  "x": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0"}}`

### zero beta

```json
{
  "alpha": {
    "kind": "integer",
    "value": "2"
  },
  "beta": {
    "kind": "integer",
    "value": "0"
  },
  "x": {
    "kind": "decimal",
    "value": "0.5"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="beta_pdf"></a>

## beta_pdf

Probability density of the beta distribution.

pdf(x) = x^(alpha - 1) (1 - x)^(beta - 1) / B(alpha, beta) for x in [0, 1], evaluated in log space with a Lanczos log-gamma. Outside [0, 1] the density is 0; at the endpoints it is unbounded when the corresponding shape is below 1, equals the other shape when it is 1, and is 0 when it is above 1.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#beta_pdf

## Parameters

- `x` — Quantile in [0, 1]. (number)
- `alpha` — First shape parameter; must be > 0. (number)
- `beta` — Second shape parameter; must be > 0. (number)

## Output

Distribution value.

## Examples

### finite endpoint

```json
{
  "alpha": {
    "kind": "integer",
    "value": "1"
  },
  "beta": {
    "kind": "integer",
    "value": "2"
  },
  "x": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"2"}}`

### zero alpha

```json
{
  "alpha": {
    "kind": "integer",
    "value": "0"
  },
  "beta": {
    "kind": "integer",
    "value": "2"
  },
  "x": {
    "kind": "decimal",
    "value": "0.5"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="beta_posterior_probability_gt"></a>

## beta_posterior_probability_gt

Posterior probability that a Beta-distributed parameter exceeds a threshold.

Returns P(parameter > threshold) for a Beta(alpha, beta) posterior, computed from the regularized incomplete beta function as I_{1 - threshold}(beta, alpha). The threshold may be any finite number; probabilities outside [0, 1] are handled by the support of the distribution. method = "beta_posterior_probability_gt". The result states that the probability is a posterior probability under the supplied prior and model, not a p-value.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#beta_posterior_probability_gt

## Parameters

- `alpha` — Posterior Beta shape alpha; strictly positive. (number)
- `beta` — Posterior Beta shape beta; strictly positive. (number)
- `threshold` — Threshold; the returned probability is P(parameter > threshold). (number)

## Output

Beta posterior tail probability record.

## Examples

### uniform posterior above one half

```json
{
  "alpha": {
    "kind": "integer",
    "value": "1"
  },
  "beta": {
    "kind": "integer",
    "value": "1"
  },
  "threshold": {
    "kind": "decimal",
    "value": "0.5"
  }
}
```

Expected: `{"type":"contains","text":"beta_posterior_probability_gt"}`

### non-positive shape

```json
{
  "alpha": {
    "kind": "integer",
    "value": "0"
  },
  "beta": {
    "kind": "integer",
    "value": "1"
  },
  "threshold": {
    "kind": "decimal",
    "value": "0.5"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="beta_quantile"></a>

## beta_quantile

Inverse beta CDF.

Returns x such that cdf(x) = p, solved by bracket expansion, bisection, and Newton polishing. p must be in [0, 1]; p = 0 returns 0 and p = 1 returns 1. alpha and beta must be strictly positive.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#beta_quantile

## Parameters

- `p` — Probability in [0, 1]. (number)
- `alpha` — First shape parameter; must be > 0. (number)
- `beta` — Second shape parameter; must be > 0. (number)

## Output

Distribution value.

## Examples

### zero probability

```json
{
  "alpha": {
    "kind": "integer",
    "value": "2"
  },
  "beta": {
    "kind": "integer",
    "value": "2"
  },
  "p": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0"}}`

### zero alpha

```json
{
  "alpha": {
    "kind": "integer",
    "value": "0"
  },
  "beta": {
    "kind": "integer",
    "value": "2"
  },
  "p": {
    "kind": "decimal",
    "value": "0.5"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="binomial_cdf"></a>

## binomial_cdf

Cumulative distribution function of the binomial distribution.

cdf(k) = P(X <= k) = I_(1-p)(n - k, k + 1), the regularized incomplete beta function. k < 0 gives 0 and k >= n gives 1.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#binomial_cdf

## Parameters

- `k` — Number of successes; integer. (integer)
- `n` — Number of trials; integer >= 0. (integer)
- `p` — Success probability in [0, 1]. (number)

## Output

Binomial distribution value.

## Examples

### all trials succeed

```json
{
  "k": {
    "kind": "integer",
    "value": "10"
  },
  "n": {
    "kind": "integer",
    "value": "10"
  },
  "p": {
    "kind": "decimal",
    "value": "0.5"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"1"}}`


<a id="binomial_pmf"></a>

## binomial_pmf

Probability mass function of the binomial distribution.

pmf(k) = C(n, k) p^k (1 - p)^(n - k), evaluated with a lgamma-based log-pmf. k < 0 or k > n gives 0; p = 0 or p = 1 are handled exactly.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#binomial_pmf

## Parameters

- `k` — Number of successes; integer. (integer)
- `n` — Number of trials; integer >= 0. (integer)
- `p` — Success probability in [0, 1]. (number)

## Output

Binomial distribution value.

## Examples

### empty trial set

```json
{
  "k": {
    "kind": "integer",
    "value": "0"
  },
  "n": {
    "kind": "integer",
    "value": "0"
  },
  "p": {
    "kind": "decimal",
    "value": "0.5"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"1"}}`


<a id="chi_square_cdf"></a>

## chi_square_cdf

Cumulative distribution function of the chi-square distribution.

cdf(x) = P(df/2, x/2), the regularized lower incomplete gamma function, evaluated with the series for x < a + 1 and the continued fraction otherwise.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#chi_square_cdf

## Parameters

- `x` — Quantile. (number)
- `df` — Degrees of freedom; must be > 0. (number)

## Output

Chi-square distribution value.

## Examples

### zero quantile

```json
{
  "df": {
    "kind": "integer",
    "value": "3"
  },
  "x": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0"}}`


<a id="chi_square_contingency"></a>

## chi_square_contingency

Pearson chi-square test for a two-way contingency table.

Accepts a matrix or an array of equal-length arrays of non-negative counts. Returns the Pearson statistic, degrees of freedom (r - 1)(c - 1), p-value, the expected count matrix, and diagnostics (minimum expected count, number of cells below 5, and a warning when expected counts are small). Ragged or empty tables and tables with a zero margin are rejected.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Quadratic
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#chi_square_contingency

## Parameters

- `table` — Two-way table of non-negative counts: a matrix or an array of equal-length arrays. (any value)

## Output

Chi-square test record with the expected-count matrix and diagnostics.

## Examples

### zero margins

```json
{
  "table": [
    [
      {
        "kind": "integer",
        "value": "0"
      },
      {
        "kind": "integer",
        "value": "0"
      }
    ],
    [
      {
        "kind": "integer",
        "value": "0"
      },
      {
        "kind": "integer",
        "value": "0"
      }
    ]
  ]
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="chi_square_pdf"></a>

## chi_square_pdf

Probability density of the chi-square distribution.

pdf(x) = x^(df/2 - 1) e^(-x/2) / (2^(df/2) Gamma(df/2)) for x >= 0, evaluated in log space with a Lanczos log-gamma. For x < 0 the density is 0; at x = 0 the density is unbounded for df < 2 and 0.5 for df = 2.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#chi_square_pdf

## Parameters

- `x` — Quantile. (number)
- `df` — Degrees of freedom; must be > 0. (number)

## Output

Chi-square distribution value.

## Examples

### density at zero for df = 2

```json
{
  "df": {
    "kind": "integer",
    "value": "2"
  },
  "x": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0.5"}}`


<a id="chi_square_quantile"></a>

## chi_square_quantile

Inverse chi-square CDF.

Returns x such that cdf(x) = p, solved by bracket expansion, bisection, and Newton polishing to a relative tolerance of 1e-15. p must be in [0, 1); p = 0 returns 0 and p = 1 is rejected because the quantile is unbounded. df must be positive.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#chi_square_quantile

## Parameters

- `p` — Probability in [0, 1). (number)
- `df` — Degrees of freedom; must be > 0. (number)

## Output

Chi-square quantile.

## Examples

### zero probability

```json
{
  "df": {
    "kind": "integer",
    "value": "3"
  },
  "p": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0"}}`


<a id="chi_square_sf"></a>

## chi_square_sf

Upper tail probability of the chi-square distribution.

sf(x) = Q(df/2, x/2), the regularized upper incomplete gamma function, computed directly so the upper tail keeps relative accuracy.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#chi_square_sf

## Parameters

- `x` — Quantile. (number)
- `df` — Degrees of freedom; must be > 0. (number)

## Output

Chi-square distribution value.

## Examples

### zero quantile

```json
{
  "df": {
    "kind": "integer",
    "value": "3"
  },
  "x": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"1"}}`


<a id="ci_mean"></a>

## ci_mean

Confidence interval for the mean of one sample.

method = "t" (default) uses the Student-t critical value with n - 1 degrees of freedom; method = "z" uses the normal critical value with the sample standard deviation (a large-sample approximation). At least two observations are required. The output reports estimate, lower, upper, standard_error, method, df, and the assumptions that were applied; no normality or independence assumption is silently added.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#ci_mean

## Parameters

- `values` — Observations. (array of number)
- `confidence` (optional) — Confidence level in (0, 1); default 0.95. (number)
- `method` (optional) — Critical value method: t (default) or z. (one of ["t", "z"])

## Output

Confidence interval record.

## Examples

### single observation is not enough

```json
{
  "values": [
    {
      "kind": "integer",
      "value": "1"
    }
  ]
}
```

Expected: `{"type":"error","code":"insufficient_observations"}`


<a id="ci_proportion"></a>

## ci_proportion

Confidence interval for a binomial proportion.

method = "wilson" (default) uses the Wilson score interval; method = "wald" uses the normal approximation p_hat +/- z * sqrt(p_hat (1 - p_hat) / n) with endpoints clipped to [0, 1]. Wald intervals emit a warning when n * p_hat < 5 or n * (1 - p_hat) < 5. successes must be an integer in 0..=n and n must be at least 1.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#ci_proportion

## Parameters

- `successes` — Number of successes; integer in 0..=n. (integer)
- `n` — Number of trials; integer >= 1. (integer)
- `confidence` (optional) — Confidence level in (0, 1); default 0.95. (number)
- `method` (optional) — Interval method: wilson (default) or wald. (one of ["wilson", "wald"])

## Output

Proportion confidence interval record.

## Examples

### zero trials

```json
{
  "n": {
    "kind": "integer",
    "value": "0"
  },
  "successes": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="confidence_sequence_mean"></a>

## confidence_sequence_mean

Always-valid confidence sequence for a mean using a time-uniform concentration bound.

Computes a confidence sequence for the mean that is valid simultaneously for every observation time t = 1, ..., n. At each t the bound inverts a pointwise concentration inequality at level alpha_t = alpha * 6 / (pi^2 t^2), where alpha = 1 - confidence; the schedule sums to alpha over t = 1, 2, ..., so the union bound gives coverage at least confidence for all t. method = "hoeffding" (default) requires either the known sub-Gaussian standard deviation sigma, or a known bounded range supplied as lower and upper (every observation must lie inside it). method = "empirical_bernstein" uses the observed range and the empirical (biased) variance and ignores sigma. The output reports estimate, lower, upper, half_width, n, method, the variance_source actually used, confidence, and the assumptions. The Howard-Ramdas-McAuliffe-Sekhon time-uniform construction is used with this discrete schedule; widths shrink at a root-logarithmic rate rather than the 1/sqrt(n) rate of a fixed-time interval.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#confidence_sequence_mean

## Parameters

- `values` — Observations in observation order. (array of number)
- `confidence` (optional) — Confidence level in (0, 1); default 0.95. (number)
- `sigma` (optional) — Known sub-Gaussian standard deviation (variance proxy sigma^2); when supplied the sub-Gaussian bound is used instead of a bounded range. Only valid with method = "hoeffding". (number)
- `method` (optional) — Bound method: hoeffding (default) or empirical_bernstein. (one of ["hoeffding", "empirical_bernstein"])
- `lower` (optional) — Known lower bound of every observation; required for the hoeffding method without sigma. (number)
- `upper` (optional) — Known upper bound of every observation; required for the hoeffding method without sigma. (number)

## Output

Time-uniform confidence sequence record for the mean.

## Examples

### bounded Hoeffding sequence

```json
{
  "lower": {
    "kind": "integer",
    "value": "0"
  },
  "upper": {
    "kind": "integer",
    "value": "4"
  },
  "values": [
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

Expected: `{"type":"contains","text":"hoeffding"}`

### empirical Bernstein sequence

```json
{
  "method": "empirical_bernstein",
  "values": [
    {
      "kind": "decimal",
      "value": "1.0"
    },
    {
      "kind": "decimal",
      "value": "2.0"
    },
    {
      "kind": "decimal",
      "value": "3.0"
    }
  ]
}
```

Expected: `{"type":"contains","text":"empirical_bernstein"}`

### hoeffding without bounds

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
    }
  ]
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="confidence_sequence_proportion"></a>

## confidence_sequence_proportion

Always-valid confidence sequence for a proportion using the time-uniform Hoeffding bound on [0, 1].

Computes a time-uniform confidence sequence for a Bernoulli success probability from successes successes in n trials. The pointwise Hoeffding bound on [0, 1] is inverted at level alpha_t = alpha * 6 / (pi^2 t^2) with alpha = 1 - confidence and t = n, so the interval covers the true proportion for every n simultaneously with probability at least confidence. The endpoints are clipped to [0, 1]. The output reports estimate, lower, upper, half_width, n, method = "hoeffding", confidence, and the assumptions.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#confidence_sequence_proportion

## Parameters

- `successes` — Number of successes; integer in 0..=n. (integer)
- `n` — Number of trials; integer >= 1. (integer)
- `confidence` (optional) — Confidence level in (0, 1); default 0.95. (number)
- `method` (optional) — Bound method; only hoeffding is supported. (one of ["hoeffding"])

## Output

Time-uniform confidence sequence record for a proportion.

## Examples

### three successes in ten trials

```json
{
  "n": {
    "kind": "integer",
    "value": "10"
  },
  "successes": {
    "kind": "integer",
    "value": "3"
  }
}
```

Expected: `{"type":"contains","text":"hoeffding"}`

### successes exceed trials

```json
{
  "n": {
    "kind": "integer",
    "value": "10"
  },
  "successes": {
    "kind": "integer",
    "value": "12"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="correlation"></a>

## correlation

Pearson product-moment correlation coefficient.

Returns the Pearson correlation r = cov(x, y) / (sd(x) * sd(y)) as float64. If either series has zero variance the result is undefined and a domain_violation is returned instead of NaN. At least two paired observations are required. The output is always approximate, so exact mode is not supported.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#correlation

## Parameters

- `xs` — First series. (array of number)
- `ys` — Second series, paired with xs. (array of number)

## Output

Pearson correlation coefficient.

## Examples

### perfect positive correlation

```json
{
  "xs": [
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
  "ys": [
    {
      "kind": "integer",
      "value": "2"
    },
    {
      "kind": "integer",
      "value": "4"
    },
    {
      "kind": "integer",
      "value": "6"
    }
  ]
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"1"}}`

### zero variance

```json
{
  "xs": [
    {
      "kind": "integer",
      "value": "1"
    },
    {
      "kind": "integer",
      "value": "1"
    },
    {
      "kind": "integer",
      "value": "1"
    }
  ],
  "ys": [
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

Expected: `{"type":"error","code":"domain_violation"}`


<a id="count"></a>

## count

Count the observations in an array.

Returns the number of entries. Every entry must be a finite number; non-numeric or non-finite entries are rejected rather than silently skipped.

- Module: `statistics` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#count

## Parameters

- `values` — Observations to count. (array of number)

## Output

Number of observations.

## Examples

### count three values

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
    }
  ]
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"3"}}`


<a id="covariance"></a>

## covariance

Sample or population covariance of two paired series.

Computes sum((x - mean_x) * (y - mean_y)) / (n - ddof) for paired observations. The two arrays must have equal length; n must be strictly greater than ddof. Exact inputs use exact rational arithmetic; float64 inputs use a stable two-pass centered algorithm and require auto or scientific mode.

- Module: `statistics` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#covariance

## Parameters

- `xs` — First series. (array of number)
- `ys` — Second series, paired with xs. (array of number)
- `ddof` — Delta degrees of freedom. (integer)

## Output

Covariance.

## Examples

### covariance of identical series

```json
{
  "ddof": {
    "kind": "integer",
    "value": "0"
  },
  "xs": [
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
  "ys": [
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

Expected: `{"type":"value","value":{"kind":"rational","numerator":"2","denominator":"3"}}`

### length mismatch

```json
{
  "ddof": {
    "kind": "integer",
    "value": "0"
  },
  "xs": [
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
  "ys": [
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

Expected: `{"type":"error","code":"malformed_input"}`


<a id="difference_in_differences"></a>

## difference_in_differences

Means-based difference-in-differences estimator with a Welch-style standard error.

Estimates (treatment_post - treatment_pre) - (control_post - control_pre) from the four group-period samples. The standard error combines the four unbiased sample variances as var(control_pre) / n_control_pre + var(control_post) / n_control_post + var(treatment_pre) / n_treatment_pre + var(treatment_post) / n_treatment_post, and the interval is estimate +/- z * standard_error with the normal critical value at the requested confidence level. Every sample needs at least two observations. The output reports estimate, standard_error, the confidence interval, method = "difference_in_differences_2x2", and the parallel-trends and no-interference assumptions that the estimator requires.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#difference_in_differences

## Parameters

- `control_pre` — Control-group observations before the intervention. (array of number)
- `control_post` — Control-group observations after the intervention. (array of number)
- `treatment_pre` — Treatment-group observations before the intervention. (array of number)
- `treatment_post` — Treatment-group observations after the intervention. (array of number)
- `confidence` (optional) — Confidence level in (0, 1); default 0.95. (number)

## Output

Difference-in-differences record with its parallel-trends assumption.

## Examples

### treatment change exceeds control change

```json
{
  "control_post": [
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
  ],
  "control_pre": [
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
  "treatment_post": [
    {
      "kind": "integer",
      "value": "4"
    },
    {
      "kind": "integer",
      "value": "5"
    },
    {
      "kind": "integer",
      "value": "6"
    }
  ],
  "treatment_pre": [
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

Expected: `{"type":"contains","text":"difference_in_differences_2x2"}`

### one observation per group period

```json
{
  "control_post": [
    {
      "kind": "integer",
      "value": "2"
    }
  ],
  "control_pre": [
    {
      "kind": "integer",
      "value": "1"
    }
  ],
  "treatment_post": [
    {
      "kind": "integer",
      "value": "4"
    }
  ],
  "treatment_pre": [
    {
      "kind": "integer",
      "value": "1"
    }
  ]
}
```

Expected: `{"type":"error","code":"insufficient_observations"}`


<a id="exponential_cdf"></a>

## exponential_cdf

Cumulative distribution function of the exponential distribution.

cdf(x) = 1 - e^(-rate * x) for x >= 0 and 0 for x < 0. rate must be strictly positive.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#exponential_cdf

## Parameters

- `x` — Quantile. (number)
- `rate` — Rate parameter; must be > 0. (number)

## Output

Distribution value.

## Examples

### cdf at zero

```json
{
  "rate": {
    "kind": "integer",
    "value": "1"
  },
  "x": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0"}}`

### zero rate

```json
{
  "rate": {
    "kind": "integer",
    "value": "0"
  },
  "x": {
    "kind": "integer",
    "value": "1"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="exponential_pdf"></a>

## exponential_pdf

Probability density of the exponential distribution.

pdf(x) = rate * e^(-rate * x) for x >= 0 and 0 for x < 0. rate must be strictly positive.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#exponential_pdf

## Parameters

- `x` — Quantile. (number)
- `rate` — Rate parameter; must be > 0. (number)

## Output

Distribution value.

## Examples

### density at zero

```json
{
  "rate": {
    "kind": "integer",
    "value": "1"
  },
  "x": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"1"}}`

### zero rate

```json
{
  "rate": {
    "kind": "integer",
    "value": "0"
  },
  "x": {
    "kind": "integer",
    "value": "1"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="exponential_quantile"></a>

## exponential_quantile

Inverse exponential CDF.

Returns x such that cdf(x) = p as -ln(1 - p) / rate. p must be in [0, 1); p = 1 is rejected because the quantile is unbounded. rate must be strictly positive.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#exponential_quantile

## Parameters

- `p` — Probability in [0, 1). (number)
- `rate` — Rate parameter; must be > 0. (number)

## Output

Distribution value.

## Examples

### zero probability

```json
{
  "p": {
    "kind": "integer",
    "value": "0"
  },
  "rate": {
    "kind": "integer",
    "value": "1"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0"}}`

### unbounded probability

```json
{
  "p": {
    "kind": "integer",
    "value": "1"
  },
  "rate": {
    "kind": "integer",
    "value": "1"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="exponential_sf"></a>

## exponential_sf

Upper tail probability of the exponential distribution.

sf(x) = P(X > x) = e^(-rate * x) for x >= 0 and 1 for x < 0. The tail is computed from exp directly, never as 1 - cdf, so large x keeps relative accuracy.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#exponential_sf

## Parameters

- `x` — Quantile. (number)
- `rate` — Rate parameter; must be > 0. (number)

## Output

Distribution value.

## Examples

### survival at zero

```json
{
  "rate": {
    "kind": "integer",
    "value": "1"
  },
  "x": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"1"}}`

### zero rate

```json
{
  "rate": {
    "kind": "integer",
    "value": "0"
  },
  "x": {
    "kind": "integer",
    "value": "1"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="f_cdf"></a>

## f_cdf

Cumulative distribution function of the F distribution.

cdf(x) = I_y(df1/2, df2/2) with y = df1 x / (df1 x + df2), the regularized incomplete beta function. x <= 0 returns 0. df1 and df2 must be strictly positive.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#f_cdf

## Parameters

- `x` — Quantile. (number)
- `df1` — Numerator degrees of freedom; must be > 0. (number)
- `df2` — Denominator degrees of freedom; must be > 0. (number)

## Output

Distribution value.

## Examples

### zero quantile

```json
{
  "df1": {
    "kind": "integer",
    "value": "1"
  },
  "df2": {
    "kind": "integer",
    "value": "1"
  },
  "x": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0"}}`

### zero denominator degrees of freedom

```json
{
  "df1": {
    "kind": "integer",
    "value": "1"
  },
  "df2": {
    "kind": "integer",
    "value": "0"
  },
  "x": {
    "kind": "integer",
    "value": "1"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="f_pdf"></a>

## f_pdf

Probability density of the F distribution.

pdf(x) = (df1/df2)^(df1/2) x^(df1/2 - 1) / (B(df1/2, df2/2) (1 + df1 x / df2)^((df1 + df2)/2)) for x > 0, evaluated in log space with a Lanczos log-gamma. For x < 0 the density is 0; at x = 0 it is unbounded for df1 < 2, equals 1 for df1 = 2, and is 0 for df1 > 2.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#f_pdf

## Parameters

- `x` — Quantile. (number)
- `df1` — Numerator degrees of freedom; must be > 0. (number)
- `df2` — Denominator degrees of freedom; must be > 0. (number)

## Output

Distribution value.

## Examples

### density at zero for df1 three

```json
{
  "df1": {
    "kind": "integer",
    "value": "3"
  },
  "df2": {
    "kind": "integer",
    "value": "3"
  },
  "x": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0"}}`

### zero degrees of freedom

```json
{
  "df1": {
    "kind": "integer",
    "value": "0"
  },
  "df2": {
    "kind": "integer",
    "value": "1"
  },
  "x": {
    "kind": "integer",
    "value": "1"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="f_quantile"></a>

## f_quantile

Inverse F CDF.

Returns x such that cdf(x) = p, computed from the beta quantile q = I^(-1)_p(df1/2, df2/2) as x = df2 q / (df1 (1 - q)). p must be in [0, 1); p = 0 returns 0 and p = 1 is rejected because the quantile is unbounded. df1 and df2 must be strictly positive.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#f_quantile

## Parameters

- `p` — Probability in [0, 1). (number)
- `df1` — Numerator degrees of freedom; must be > 0. (number)
- `df2` — Denominator degrees of freedom; must be > 0. (number)

## Output

Distribution value.

## Examples

### zero probability

```json
{
  "df1": {
    "kind": "integer",
    "value": "1"
  },
  "df2": {
    "kind": "integer",
    "value": "1"
  },
  "p": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0"}}`

### zero numerator degrees of freedom

```json
{
  "df1": {
    "kind": "integer",
    "value": "0"
  },
  "df2": {
    "kind": "integer",
    "value": "1"
  },
  "p": {
    "kind": "decimal",
    "value": "0.5"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="gamma_cdf"></a>

## gamma_cdf

Cumulative distribution function of the gamma distribution.

cdf(x) = P(shape, rate * x), the regularized lower incomplete gamma function, evaluated with the series for small arguments and the continued fraction otherwise. x <= 0 returns 0. shape and rate must be strictly positive.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#gamma_cdf

## Parameters

- `x` — Quantile. (number)
- `shape` — Shape parameter; must be > 0. (number)
- `rate` (optional) — Rate parameter; must be > 0; default 1. (number)

## Output

Distribution value.

## Examples

### zero quantile

```json
{
  "shape": {
    "kind": "integer",
    "value": "2"
  },
  "x": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0"}}`

### zero shape

```json
{
  "shape": {
    "kind": "integer",
    "value": "0"
  },
  "x": {
    "kind": "integer",
    "value": "1"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="gamma_pdf"></a>

## gamma_pdf

Probability density of the gamma distribution.

pdf(x) = rate^shape x^(shape - 1) e^(-rate x) / Gamma(shape) for x > 0, evaluated in log space with a Lanczos log-gamma. For x < 0 the density is 0; at x = 0 it is unbounded for shape < 1, equals rate for shape = 1, and is 0 for shape > 1.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#gamma_pdf

## Parameters

- `x` — Quantile. (number)
- `shape` — Shape parameter; must be > 0. (number)
- `rate` (optional) — Rate parameter; must be > 0; default 1. (number)

## Output

Distribution value.

## Examples

### density at zero for shape one

```json
{
  "rate": {
    "kind": "integer",
    "value": "2"
  },
  "shape": {
    "kind": "integer",
    "value": "1"
  },
  "x": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"2"}}`

### zero shape

```json
{
  "shape": {
    "kind": "integer",
    "value": "0"
  },
  "x": {
    "kind": "integer",
    "value": "1"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="gamma_poisson_update"></a>

## gamma_poisson_update

Posterior summary for a Poisson rate under a Gamma prior.

Given total_count events observed over exposure units and a Gamma(prior_shape, prior_rate) prior in the rate parameterization, the posterior is Gamma(prior_shape + total_count, prior_rate + exposure). Returns the posterior shape and rate, the posterior mean, the equal-tailed credible interval at the requested confidence level, method = "gamma_poisson_conjugate", and the assumptions. total_count must be a non-negative integer, exposure must be strictly positive, and the prior shape and rate must be strictly positive. The interval is a posterior credible interval under the supplied prior and Poisson model, not a confidence interval.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#gamma_poisson_update

## Parameters

- `total_count` — Observed event count; integer >= 0. (integer)
- `exposure` — Total exposure; strictly positive. (number)
- `prior_shape` — Prior Gamma shape; strictly positive. (number)
- `prior_rate` — Prior Gamma rate; strictly positive. (number)
- `confidence` (optional) — Credible level in (0, 1); default 0.95. (number)

## Output

Gamma-Poisson posterior record with an equal-tailed credible interval.

## Examples

### five events over two units of exposure

```json
{
  "exposure": {
    "kind": "integer",
    "value": "2"
  },
  "prior_rate": {
    "kind": "integer",
    "value": "1"
  },
  "prior_shape": {
    "kind": "integer",
    "value": "2"
  },
  "total_count": {
    "kind": "integer",
    "value": "5"
  }
}
```

Expected: `{"type":"contains","text":"gamma_poisson_conjugate"}`

### zero exposure

```json
{
  "exposure": {
    "kind": "integer",
    "value": "0"
  },
  "prior_rate": {
    "kind": "integer",
    "value": "1"
  },
  "prior_shape": {
    "kind": "integer",
    "value": "2"
  },
  "total_count": {
    "kind": "integer",
    "value": "5"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="gamma_quantile"></a>

## gamma_quantile

Inverse gamma CDF.

Returns x such that cdf(x) = p, solved by bracket expansion, bisection, and Newton polishing. p must be in [0, 1); p = 0 returns 0 and p = 1 is rejected because the quantile is unbounded. shape and rate must be strictly positive.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#gamma_quantile

## Parameters

- `p` — Probability in [0, 1). (number)
- `shape` — Shape parameter; must be > 0. (number)
- `rate` (optional) — Rate parameter; must be > 0; default 1. (number)

## Output

Distribution value.

## Examples

### zero probability

```json
{
  "p": {
    "kind": "integer",
    "value": "0"
  },
  "shape": {
    "kind": "integer",
    "value": "2"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0"}}`

### zero rate

```json
{
  "p": {
    "kind": "decimal",
    "value": "0.5"
  },
  "rate": {
    "kind": "integer",
    "value": "0"
  },
  "shape": {
    "kind": "integer",
    "value": "2"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="logistic_regression"></a>

## logistic_regression

Binary logistic regression fitted with iteratively reweighted least squares.

y must contain only 0 and 1 values. x may be a flat numeric array (simple regression) or an array of equal-length rows; intercept defaults to true. Fitting uses iteratively reweighted least squares with tolerance (default 1e-8) on the maximum coefficient change and max_iterations (default 100). Complete or quasi-complete separation and failure to converge within the iteration limit are reported as non_convergence. Returns coefficients, standard errors, z statistics, two-sided normal p-values, the log-likelihood, AIC, the iteration count, the convergence flag, and method = "irls_logistic".

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#logistic_regression

## Parameters

- `y` — Binary response values (0 or 1). (array of number)
- `x` — Flat predictor array or array of predictor rows. (any value)
- `intercept` (optional) — Include an intercept column; default true. (boolean)
- `tolerance` (optional) — Convergence tolerance on the maximum coefficient change; default 1e-8. (number)
- `max_iterations` (optional) — Maximum IRLS iterations; integer >= 1; default 100. (integer)

## Output

Logistic regression fit record.

## Examples

### one dimensional separation

```json
{
  "x": [
    {
      "kind": "integer",
      "value": "-2"
    },
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
    },
    {
      "kind": "integer",
      "value": "2"
    }
  ],
  "y": [
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
      "value": "0"
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

Expected: `{"type":"contains","text":"irls_logistic"}`

### non-binary response

```json
{
  "x": [
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
  "y": [
    {
      "kind": "integer",
      "value": "0"
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

Expected: `{"type":"error","code":"domain_violation"}`


<a id="lognormal_cdf"></a>

## lognormal_cdf

Cumulative distribution function of the log-normal distribution.

cdf(x) = Phi((ln x - mu) / sigma) for x > 0 and 0 for x <= 0, where Phi is the standard normal CDF evaluated with a high-accuracy erfc. sigma must be strictly positive.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#lognormal_cdf

## Parameters

- `x` — Quantile. (number)
- `mu` (optional) — Mean of the logarithm; default 0. (number)
- `sigma` (optional) — Standard deviation of the logarithm; must be > 0; default 1. (number)

## Output

Distribution value.

## Examples

### median

```json
{
  "x": {
    "kind": "integer",
    "value": "1"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0.5"}}`

### zero sigma

```json
{
  "sigma": {
    "kind": "integer",
    "value": "0"
  },
  "x": {
    "kind": "integer",
    "value": "1"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="lognormal_pdf"></a>

## lognormal_pdf

Probability density of the log-normal distribution.

pdf(x) = e^(-0.5 z^2) / (x * sigma * sqrt(2 pi)) with z = (ln x - mu) / sigma, for x > 0 and 0 for x <= 0. sigma must be strictly positive.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#lognormal_pdf

## Parameters

- `x` — Quantile. (number)
- `mu` (optional) — Mean of the logarithm; default 0. (number)
- `sigma` (optional) — Standard deviation of the logarithm; must be > 0; default 1. (number)

## Output

Distribution value.

## Examples

### non-positive quantile

```json
{
  "x": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0"}}`

### zero sigma

```json
{
  "sigma": {
    "kind": "integer",
    "value": "0"
  },
  "x": {
    "kind": "integer",
    "value": "1"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="lognormal_quantile"></a>

## lognormal_quantile

Inverse log-normal CDF.

Returns exp(mu + sigma * z_p) where z_p is the standard normal quantile. p must be strictly between 0 and 1; mu defaults to 0 and sigma to 1 and must be positive.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#lognormal_quantile

## Parameters

- `p` — Probability in (0, 1). (number)
- `mu` (optional) — Mean of the logarithm; default 0. (number)
- `sigma` (optional) — Standard deviation of the logarithm; must be > 0; default 1. (number)

## Output

Distribution value.

## Examples

### median

```json
{
  "p": {
    "kind": "decimal",
    "value": "0.5"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"1"}}`

### zero sigma

```json
{
  "p": {
    "kind": "decimal",
    "value": "0.5"
  },
  "sigma": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="min-max"></a>

## max

Largest observation in a non-empty array.

Returns the smallest/largest observation without modifying the input. An empty array is rejected with insufficient_observations. Exact inputs return the exact selected value; float64 inputs require auto or scientific mode.

- Module: `statistics` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#min-max

## Parameters

- `values` — Observations. (array of number)

## Output

Selected extreme value.

## Examples

### extreme value

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


<a id="mean"></a>

## mean

Exact arithmetic mean of a non-empty array.

Integer, rational, and decimal inputs produce an exact integer or rational mean. Float64 inputs require auto or scientific mode, are computed in binary64, and are marked approximate. An empty array is rejected with insufficient_observations.

- Module: `statistics` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#mean

## Parameters

- `values` — Observations. (array of number)

## Output

Arithmetic mean.

## Examples

### exact integer mean

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
    }
  ]
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"2"}}`

### exact rational mean

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
    }
  ]
}
```

Expected: `{"type":"value","value":{"kind":"rational","numerator":"3","denominator":"2"}}`

### empty array

```json
{
  "values": []
}
```

Expected: `{"type":"error","code":"insufficient_observations"}`


<a id="median"></a>

## median

Median of a non-empty array.

The values are sorted numerically without modifying the input. An odd count returns the middle value; an even count returns the exact mean of the two middle values. Exact inputs stay exact (median([1, 2, 10, 100]) = 6); float64 inputs require auto or scientific mode.

- Module: `statistics` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#median

## Parameters

- `values` — Observations. (array of number)

## Output

Median value.

## Examples

### even count

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
      "value": "10"
    },
    {
      "kind": "integer",
      "value": "100"
    }
  ]
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"6"}}`

### odd count

```json
{
  "values": [
    {
      "kind": "integer",
      "value": "2"
    },
    {
      "kind": "integer",
      "value": "10"
    },
    {
      "kind": "integer",
      "value": "30"
    }
  ]
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"10"}}`


<a id="min-max"></a>

## min

Smallest observation in a non-empty array.

Returns the smallest/largest observation without modifying the input. An empty array is rejected with insufficient_observations. Exact inputs return the exact selected value; float64 inputs require auto or scientific mode.

- Module: `statistics` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#min-max

## Parameters

- `values` — Observations. (array of number)

## Output

Selected extreme value.

## Examples

### extreme value

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


<a id="mode"></a>

## mode

All tied modes with their counts.

Returns every value that occurs most often, sorted ascending, with the matching counts. Ties are reported in full; there is no arbitrary winner. The method label is always all_modes.

- Module: `statistics` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#mode

## Parameters

- `values` — Observations. (array of number)

## Output

Record with modes, counts, and the method label all_modes.

## Examples

### single mode

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
      "value": "2"
    },
    {
      "kind": "integer",
      "value": "3"
    }
  ]
}
```

Expected: `{"type":"value","value":{"counts":[{"kind":"integer","value":"2"}],"method":"all_modes","modes":[{"kind":"integer","value":"2"}]}}`


<a id="negative_binomial_cdf"></a>

## negative_binomial_cdf

Cumulative distribution function of the negative binomial distribution.

cdf(k) = P(X <= k) = I_p(r, k + 1), the regularized incomplete beta function. r must be strictly positive and p must lie in [0, 1]; k < 0 returns 0.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#negative_binomial_cdf

## Parameters

- `k` — Number of failures; integer >= 0 (negative values return 0). (integer)
- `r` — Target number of successes; must be > 0. (number)
- `p` — Success probability in [0, 1]. (number)

## Output

Negative binomial distribution value.

## Examples

### certain success on the first trial

```json
{
  "k": {
    "kind": "integer",
    "value": "5"
  },
  "p": {
    "kind": "integer",
    "value": "1"
  },
  "r": {
    "kind": "integer",
    "value": "1"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"1"}}`

### zero target successes

```json
{
  "k": {
    "kind": "integer",
    "value": "1"
  },
  "p": {
    "kind": "decimal",
    "value": "0.5"
  },
  "r": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="negative_binomial_pmf"></a>

## negative_binomial_pmf

Probability mass function of the negative binomial distribution.

pmf(k) = C(k + r - 1, k) p^r (1 - p)^k, evaluated in log space with a Lanczos log-gamma. r must be strictly positive and p must lie in [0, 1]; k < 0 returns 0. p = 0 and p = 1 are handled exactly.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#negative_binomial_pmf

## Parameters

- `k` — Number of failures; integer >= 0 (negative values return 0). (integer)
- `r` — Target number of successes; must be > 0. (number)
- `p` — Success probability in [0, 1]. (number)

## Output

Negative binomial distribution value.

## Examples

### certain success on the first trial

```json
{
  "k": {
    "kind": "integer",
    "value": "0"
  },
  "p": {
    "kind": "integer",
    "value": "1"
  },
  "r": {
    "kind": "integer",
    "value": "1"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"1"}}`

### probability above one

```json
{
  "k": {
    "kind": "integer",
    "value": "1"
  },
  "p": {
    "kind": "decimal",
    "value": "1.5"
  },
  "r": {
    "kind": "integer",
    "value": "2"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="normal_cdf"></a>

## normal_cdf

Cumulative distribution function of the normal distribution.

cdf(x) = 0.5 * erfc(-(x - mean) / (sd * sqrt(2))), evaluated with a high-accuracy complementary error function.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#normal_cdf

## Parameters

- `x` — Quantile. (number)
- `mean` (optional) — Mean; default 0. (number)
- `sd` (optional) — Standard deviation; must be > 0; default 1. (number)

## Output

Normal distribution value.

## Examples

### standard normal center

```json
{
  "x": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0.5"}}`


<a id="normal_normal_update"></a>

## normal_normal_update

Posterior summary for a normal mean with known sampling variance.

Given a sample mean from sample_n observations with known standard deviation known_sigma and a N(prior_mean, prior_sigma^2) prior, the posterior precision is 1 / prior_sigma^2 + sample_n / known_sigma^2, the posterior mean is the precision-weighted average of the prior mean and the sample mean, and the posterior variance is the reciprocal of the posterior precision. Returns posterior_mean, posterior_variance, posterior_sd, the equal-tailed credible interval at the requested confidence level, method = "normal_normal_conjugate", and the assumptions. sample_n must be an integer at least 1 and both standard deviations must be strictly positive. The interval is a posterior credible interval under the supplied prior and model, not a confidence interval.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#normal_normal_update

## Parameters

- `sample_mean` — Observed sample mean. (number)
- `sample_n` — Number of observations in the sample; integer >= 1. (integer)
- `known_sigma` — Known sampling standard deviation; strictly positive. (number)
- `prior_mean` — Prior mean. (number)
- `prior_sigma` — Prior standard deviation; strictly positive. (number)
- `confidence` (optional) — Credible level in (0, 1); default 0.95. (number)

## Output

Normal-normal posterior record with an equal-tailed credible interval.

## Examples

### precision-weighted update

```json
{
  "known_sigma": {
    "kind": "decimal",
    "value": "1.0"
  },
  "prior_mean": {
    "kind": "decimal",
    "value": "0.0"
  },
  "prior_sigma": {
    "kind": "decimal",
    "value": "1.0"
  },
  "sample_mean": {
    "kind": "decimal",
    "value": "2.0"
  },
  "sample_n": {
    "kind": "integer",
    "value": "4"
  }
}
```

Expected: `{"type":"contains","text":"normal_normal_conjugate"}`

### zero observations

```json
{
  "known_sigma": {
    "kind": "decimal",
    "value": "1.0"
  },
  "prior_mean": {
    "kind": "decimal",
    "value": "0.0"
  },
  "prior_sigma": {
    "kind": "decimal",
    "value": "1.0"
  },
  "sample_mean": {
    "kind": "decimal",
    "value": "2.0"
  },
  "sample_n": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="normal_pdf"></a>

## normal_pdf

Probability density of the normal distribution.

pdf(x) = exp(-0.5 * ((x - mean) / sd)^2) / (sd * sqrt(2 * pi)). The standard deviation must be strictly positive.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#normal_pdf

## Parameters

- `x` — Quantile. (number)
- `mean` (optional) — Mean; default 0. (number)
- `sd` (optional) — Standard deviation; must be > 0; default 1. (number)

## Output

Normal distribution value.

## Examples

### zero standard deviation

```json
{
  "sd": {
    "kind": "integer",
    "value": "0"
  },
  "x": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="normal_posterior_probability_gt"></a>

## normal_posterior_probability_gt

Posterior probability that a normal parameter exceeds a threshold.

Returns P(parameter > threshold) for a N(mean, sd^2) posterior, computed from the complementary error function as 0.5 * erfc((threshold - mean) / (sd * sqrt(2))). sd must be strictly positive. method = "normal_posterior_probability_gt". The result states that the probability is a posterior probability under the supplied prior and model, not a p-value.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#normal_posterior_probability_gt

## Parameters

- `mean` — Posterior mean. (number)
- `sd` — Posterior standard deviation; strictly positive. (number)
- `threshold` — Threshold; the returned probability is P(parameter > threshold). (number)

## Output

Normal posterior tail probability record.

## Examples

### one point nine six standard deviations above the mean

```json
{
  "mean": {
    "kind": "integer",
    "value": "0"
  },
  "sd": {
    "kind": "integer",
    "value": "1"
  },
  "threshold": {
    "kind": "decimal",
    "value": "1.96"
  }
}
```

Expected: `{"type":"contains","text":"normal_posterior_probability_gt"}`

### non-positive standard deviation

```json
{
  "mean": {
    "kind": "integer",
    "value": "0"
  },
  "sd": {
    "kind": "integer",
    "value": "0"
  },
  "threshold": {
    "kind": "decimal",
    "value": "1.96"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="normal_quantile"></a>

## normal_quantile

Inverse normal CDF.

Returns x such that cdf(x) = p, solved by bracket expansion, bisection, and Newton polishing to a relative tolerance of 1e-15. p must be strictly between 0 and 1; mean defaults to 0 and sd to 1 and must be positive.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#normal_quantile

## Parameters

- `p` — Probability in (0, 1). (number)
- `mean` (optional) — Mean; default 0. (number)
- `sd` (optional) — Standard deviation; must be > 0; default 1. (number)

## Output

Normal quantile.

## Examples

### median

```json
{
  "p": {
    "kind": "decimal",
    "value": "0.5"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0"}}`


<a id="normal_sf"></a>

## normal_sf

Upper tail probability of the normal distribution.

sf(x) = P(X > x) = 0.5 * erfc((x - mean) / (sd * sqrt(2))). The upper tail is computed from erfc directly, never as 1 - cdf, so large positive x keeps relative accuracy.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#normal_sf

## Parameters

- `x` — Quantile. (number)
- `mean` (optional) — Mean; default 0. (number)
- `sd` (optional) — Standard deviation; must be > 0; default 1. (number)

## Output

Normal distribution value.

## Examples

### standard normal center

```json
{
  "x": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0.5"}}`


<a id="ols"></a>

## ols

Ordinary least squares regression with pivoted normal equations.

x may be a flat numeric array (simple regression) or an array of equal-length rows (one row per observation, one column per predictor). intercept defaults to true. The normal equations are solved with partial pivoting; a rank-deficient or numerically singular design is rejected with ill_conditioned. More observations than coefficients are required. Returns coefficients, standard errors, t statistics, two-sided t p-values, R^2, adjusted R^2, the residual standard error, the residual degrees of freedom, fitted values, residuals, and method = "ols".

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Cubic
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#ols

## Parameters

- `y` — Response values. (array of number)
- `x` — Flat predictor array or array of predictor rows. (any value)
- `intercept` (optional) — Include an intercept column; default true. (boolean)

## Output

Least-squares fit record.

## Examples

### simple regression

```json
{
  "x": [
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
  "y": [
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

Expected: `{"type":"contains","text":"ols"}`

### rank deficient design

```json
{
  "x": [
    [
      {
        "kind": "integer",
        "value": "1"
      }
    ],
    [
      {
        "kind": "integer",
        "value": "1"
      }
    ],
    [
      {
        "kind": "integer",
        "value": "1"
      }
    ]
  ],
  "y": [
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

Expected: `{"type":"error","code":"ill_conditioned"}`


<a id="p_adjust"></a>

## p_adjust

Adjust p-values for multiple testing.

Returns the adjusted p-values in the same order as the input. bonferroni multiplies each p-value by the number of tests m and caps at 1. holm is the step-down Bonferroni-Holm method. hochberg is the step-up Hochberg method. bh is the Benjamini-Hochberg false discovery rate procedure. All p-values must lie in [0, 1] and the array must not be empty.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#p_adjust

## Parameters

- `p_values` — Raw p-values in [0, 1]. (array of number)
- `method` — Adjustment method: bonferroni, holm, hochberg, or bh. (one of ["bonferroni", "holm", "hochberg", "bh"])

## Output

Adjusted p-values in input order plus the method label.

## Examples

### holm adjustment

```json
{
  "method": "holm",
  "p_values": [
    {
      "kind": "decimal",
      "value": "0.01"
    },
    {
      "kind": "decimal",
      "value": "0.02"
    },
    {
      "kind": "decimal",
      "value": "0.03"
    },
    {
      "kind": "decimal",
      "value": "0.04"
    }
  ]
}
```

Expected: `{"type":"contains","text":"holm"}`

### p-value above one

```json
{
  "method": "bonferroni",
  "p_values": [
    {
      "kind": "decimal",
      "value": "0.5"
    },
    {
      "kind": "decimal",
      "value": "1.5"
    }
  ]
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="poisson_cdf"></a>

## poisson_cdf

Cumulative distribution function of the Poisson distribution.

cdf(k) = P(X <= k) = Q(k + 1, lambda), the regularized upper incomplete gamma function, computed directly for tail accuracy. lambda must be strictly positive; k < 0 returns 0.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#poisson_cdf

## Parameters

- `k` — Event count; integer >= 0 (negative values return 0). (integer)
- `lambda` — Mean rate; must be > 0. (number)

## Output

Poisson distribution value.

## Examples

### negative count

```json
{
  "k": {
    "kind": "integer",
    "value": "-1"
  },
  "lambda": {
    "kind": "integer",
    "value": "3"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0"}}`

### zero mean rate

```json
{
  "k": {
    "kind": "integer",
    "value": "0"
  },
  "lambda": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="poisson_pmf"></a>

## poisson_pmf

Probability mass function of the Poisson distribution.

pmf(k) = e^(-lambda) lambda^k / k!, evaluated in log space with a Lanczos log-gamma so large k and lambda stay stable. lambda must be strictly positive; k < 0 returns 0.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#poisson_pmf

## Parameters

- `k` — Event count; integer >= 0 (negative values return 0). (integer)
- `lambda` — Mean rate; must be > 0. (number)

## Output

Poisson distribution value.

## Examples

### negative count

```json
{
  "k": {
    "kind": "integer",
    "value": "-1"
  },
  "lambda": {
    "kind": "integer",
    "value": "3"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0"}}`

### zero mean rate

```json
{
  "k": {
    "kind": "integer",
    "value": "0"
  },
  "lambda": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="power_two_means"></a>

## power_two_means

Achieved power for a two-sample mean comparison (normal approximation).

Computes power = Phi(d * sqrt(n / 2) - z_(1-alpha')) for equal groups, where d is the standardized effect size and alpha' is alpha / 2 for a two-sided test or alpha for a one-sided test. The result is a probability under the planning assumptions, not a guarantee.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#power_two_means

## Parameters

- `n_per_group` — Observations per group; integer >= 2. (integer)
- `effect_size` — Standardized effect size d >= 0. (number)
- `alpha` (optional) — Type I error rate; default 0.05. (number)
- `sided` (optional) — two (default) or one. (one of ["two", "one"])

## Output

Achieved power record.

## Examples

### one observation per group

```json
{
  "effect_size": {
    "kind": "decimal",
    "value": "0.5"
  },
  "n_per_group": {
    "kind": "integer",
    "value": "1"
  }
}
```

Expected: `{"type":"error","code":"insufficient_observations"}`


<a id="power_two_proportions"></a>

## power_two_proportions

Achieved power for a two-proportion comparison (normal approximation).

Computes power = Phi((|p1 - p2| * sqrt(n) - z_(1-alpha') * sqrt(2 * pbar * (1 - pbar))) / sqrt(p1 (1 - p1) + p2 (1 - p2))) for equal groups, where pbar = (p1 + p2) / 2. The result is a probability under the planning assumptions, not a guarantee.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#power_two_proportions

## Parameters

- `n_per_group` — Observations per group; integer >= 1. (integer)
- `p1` — Proportion in group 1, in [0, 1]. (number)
- `p2` — Proportion in group 2, in [0, 1]. (number)
- `alpha` (optional) — Type I error rate; default 0.05. (number)
- `sided` (optional) — two (default) or one. (one of ["two", "one"])

## Output

Achieved power record.

## Examples

### zero observations

```json
{
  "n_per_group": {
    "kind": "integer",
    "value": "0"
  },
  "p1": {
    "kind": "decimal",
    "value": "0.4"
  },
  "p2": {
    "kind": "decimal",
    "value": "0.5"
  }
}
```

Expected: `{"type":"error","code":"insufficient_observations"}`


<a id="propensity_score_weighting"></a>

## propensity_score_weighting

Inverse-probability-weighted average treatment effect on the treated.

treatment must contain only 0 and 1 values, covariates is an array of covariate rows (one row per unit), and outcomes holds one outcome per unit. A logistic regression of treatment on the covariates (with an intercept, fitted by the shared IRLS helper) gives the propensity score e(x) for each unit. The ATT is mean(outcome | treated) minus the inverse-probability-weighted control mean. method = "hajek" (default) normalizes the control weights e / (1 - e) by their sum; method = "horvitz_thompson" divides the weighted control total by the number of treated units. The standard error combines the treated sample variance with the weighted control variance and treats the fitted propensities as fixed. Positivity is checked: a propensity outside (0, 1) is rejected as an empty overlap, and propensities within 1e-6 of 0 or 1 raise a warning. The output reports att, standard_error, weights_summary, propensity_summary, diagnostics {overlap_min, overlap_max, extreme_weights}, method = "ipw_att", the estimator actually used, and the no-unmeasured-confounding and model assumptions.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#propensity_score_weighting

## Parameters

- `treatment` — Binary treatment indicator per unit: 0 or 1. (array of number)
- `covariates` — Covariate rows, one row per unit (an array of equal-length numeric arrays). (any value)
- `outcomes` — Outcome per unit, aligned with treatment and covariates. (array of number)
- `method` (optional) — Weighting estimator: hajek (default) or horvitz_thompson. (one of ["hajek", "horvitz_thompson"])

## Output

Inverse-probability-weighted ATT record with propensity and weight diagnostics.

## Examples

### small overlapping dataset

```json
{
  "covariates": [
    [
      {
        "kind": "integer",
        "value": "0"
      }
    ],
    [
      {
        "kind": "integer",
        "value": "1"
      }
    ],
    [
      {
        "kind": "integer",
        "value": "2"
      }
    ],
    [
      {
        "kind": "integer",
        "value": "3"
      }
    ],
    [
      {
        "kind": "integer",
        "value": "4"
      }
    ],
    [
      {
        "kind": "integer",
        "value": "5"
      }
    ]
  ],
  "outcomes": [
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
    },
    {
      "kind": "integer",
      "value": "5"
    },
    {
      "kind": "integer",
      "value": "6"
    }
  ],
  "treatment": [
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
    },
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
      "value": "1"
    }
  ]
}
```

Expected: `{"type":"contains","text":"ipw_att"}`

### non-binary treatment

```json
{
  "covariates": [
    [
      {
        "kind": "integer",
        "value": "0"
      }
    ],
    [
      {
        "kind": "integer",
        "value": "1"
      }
    ],
    [
      {
        "kind": "integer",
        "value": "2"
      }
    ]
  ],
  "outcomes": [
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
  "treatment": [
    {
      "kind": "integer",
      "value": "0"
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

Expected: `{"type":"error","code":"domain_violation"}`


<a id="proportions_difference"></a>

## proportions_difference

Newcombe or Wald interval for p_a - p_b.

method = "newcombe" (default) combines the two Wilson score intervals with the square-and-add hybrid; method = "wald" uses the normal approximation. Each group needs n >= 1 and 0 <= successes <= n. The Wald interval emits a small-sample warning when any expected count is below 5.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#proportions_difference

## Parameters

- `successes_a` — Successes in group A; integer in 0..=n_a. (integer)
- `n_a` — Trials in group A; integer >= 1. (integer)
- `successes_b` — Successes in group B; integer in 0..=n_b. (integer)
- `n_b` — Trials in group B; integer >= 1. (integer)
- `confidence` (optional) — Confidence level in (0, 1); default 0.95. (number)
- `method` (optional) — Interval method: newcombe (default) or wald. (one of ["newcombe", "wald"])

## Output

Difference of proportions confidence interval record.

## Examples

### successes exceed trials

```json
{
  "n_a": {
    "kind": "integer",
    "value": "2"
  },
  "n_b": {
    "kind": "integer",
    "value": "2"
  },
  "successes_a": {
    "kind": "integer",
    "value": "5"
  },
  "successes_b": {
    "kind": "integer",
    "value": "1"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="prospective_pool"></a>

## prospective_pool

Pool observed and hypothetical additional strata and standardize the combination.

Combines two strata specifications with the same structure into pooled per-stratum arm summaries (assigned counts and outcome counts summed by value), then applies the same standardization as stratified_experiment. target_weights may override the pooled target weights as either an array aligned with the original strata order or a record keyed by stratum name. The output is labelled prospective: true and carries a warning that the result uses hypothetical additional data and is not an observed result.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#prospective_pool

## Parameters

- `original` — Observed strata, same structure as stratified_experiment. (array of any value)
- `additional` — Hypothetical additional strata with the same names and structure. (array of any value)
- `target_weights` (optional) — Optional override: an array aligned with original strata order, or a record keyed by name. (any value)
- `confidence` (optional) — Confidence level in (0, 1); default 0.95. (number)

## Output

Prospective pooled analysis, labelled with prospective: true.

## Examples

### stratum names must match

```json
{
  "additional": [
    {
      "control": {
        "assigned": {
          "kind": "integer",
          "value": "1"
        },
        "outcomes": [
          {
            "count": {
              "kind": "integer",
              "value": "1"
            },
            "value": {
              "kind": "integer",
              "value": "0"
            }
          }
        ]
      },
      "name": "b",
      "target_weight": {
        "kind": "integer",
        "value": "1"
      },
      "treatment": {
        "assigned": {
          "kind": "integer",
          "value": "1"
        },
        "outcomes": [
          {
            "count": {
              "kind": "integer",
              "value": "1"
            },
            "value": {
              "kind": "integer",
              "value": "1"
            }
          }
        ]
      }
    }
  ],
  "original": [
    {
      "control": {
        "assigned": {
          "kind": "integer",
          "value": "1"
        },
        "outcomes": [
          {
            "count": {
              "kind": "integer",
              "value": "1"
            },
            "value": {
              "kind": "integer",
              "value": "0"
            }
          }
        ]
      },
      "name": "a",
      "target_weight": {
        "kind": "integer",
        "value": "1"
      },
      "treatment": {
        "assigned": {
          "kind": "integer",
          "value": "1"
        },
        "outcomes": [
          {
            "count": {
              "kind": "integer",
              "value": "1"
            },
            "value": {
              "kind": "integer",
              "value": "1"
            }
          }
        ]
      }
    }
  ]
}
```

Expected: `{"type":"error","code":"malformed_input"}`


<a id="quantile"></a>

## quantile

Quantile with an explicit interpolation convention.

Let h = (n - 1) * q. linear (default, R-7) returns x[floor(h)] + (h - floor(h)) * (x[floor(h)+1] - x[floor(h)]); lower returns x[floor(h)]; higher returns x[ceil(h)]; midpoint returns the mean of those two order statistics; nearest returns x[round_half_to_even(h)]. q must be in [0, 1]. Exact inputs stay exact; float64 inputs require auto or scientific mode.

- Module: `statistics` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#quantile

## Parameters

- `values` — Observations. (array of number)
- `q` — Quantile level in [0, 1]. (number)
- `method` (optional) — Interpolation method: linear (default), lower, higher, midpoint, nearest. (one of ["linear", "lower", "higher", "midpoint", "nearest"])

## Output

Quantile value.

## Examples

### linear interpolation

```json
{
  "q": {
    "kind": "decimal",
    "value": "0.5"
  },
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

Expected: `{"type":"value","value":{"kind":"rational","numerator":"5","denominator":"2"}}`

### out of range q

```json
{
  "q": {
    "kind": "decimal",
    "value": "1.5"
  },
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
    }
  ]
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="sample_size_two_means"></a>

## sample_size_two_means

Per-group sample size for a two-sample mean comparison (normal approximation).

Uses the normal approximation: n1 = (z_(1-alpha') + z_(1-power))^2 * (1 + 1/r) / d^2 and n2 = ceil(r * n1), where d is the standardized effect size, r is allocation_ratio = n2 / n1, and alpha' is alpha / 2 for a two-sided test or alpha for a one-sided test. Sizes are rounded up. A power figure is a probability under the planning assumptions, not a guarantee.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#sample_size_two_means

## Parameters

- `effect_size` — Standardized effect size d = (mean1 - mean2) / sd; must be > 0. (number)
- `alpha` (optional) — Type I error rate; default 0.05. (number)
- `power` (optional) — Target power; default 0.8. (number)
- `allocation_ratio` (optional) — n2 / n1; default 1. (number)
- `sided` (optional) — two (default) or one. (one of ["two", "one"])

## Output

Sample-size plan record.

## Examples

### zero effect size

```json
{
  "effect_size": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="sample_size_two_proportions"></a>

## sample_size_two_proportions

Per-group sample size for a two-proportion comparison (normal approximation).

Uses the normal approximation with pooled variance under the null and unpooled variance under the alternative: n1 = (z_(1-alpha') * sqrt((1 + 1/r) * pbar * (1 - pbar)) + z_(1-power) * sqrt(p1 (1 - p1) + p2 (1 - p2) / r))^2 / (p1 - p2)^2, where pbar = (p1 + r * p2) / (1 + r) and r = allocation_ratio = n2 / n1. p1 and p2 must differ. A power figure is a probability under the planning assumptions, not a guarantee.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#sample_size_two_proportions

## Parameters

- `p1` — Proportion in group 1, in [0, 1]. (number)
- `p2` — Proportion in group 2, in [0, 1]. (number)
- `alpha` (optional) — Type I error rate; default 0.05. (number)
- `power` (optional) — Target power; default 0.8. (number)
- `allocation_ratio` (optional) — n2 / n1; default 1. (number)
- `sided` (optional) — two (default) or one. (one of ["two", "one"])

## Output

Sample-size plan record.

## Examples

### equal proportions

```json
{
  "p1": {
    "kind": "decimal",
    "value": "0.5"
  },
  "p2": {
    "kind": "decimal",
    "value": "0.5"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="sprt_bernoulli"></a>

## sprt_bernoulli

Sequential log-likelihood ratio test between two simple Bernoulli hypotheses.

Tests H0: p = p0 against H1: p = p1 with Wald's sequential probability ratio test, where 0 < p0 < p1 < 1. successes_a and successes_b are the successes in two successive looks (or two independent batches) with n_a and n_b trials; the batches are pooled into total successes S and total trials N, and the Bernoulli log-likelihood ratio LLR = S ln(p1 / p0) + (N - S) ln((1 - p1) / (1 - p0)) is compared with the Wald boundaries ln((1 - beta) / alpha) and ln(beta / (1 - alpha)). The decision is accept_h1 when LLR >= the upper boundary, accept_h0 when LLR <= the lower boundary, and continue otherwise. alpha (default 0.05) and beta (default 0.1) are the type I and type II error probabilities, each strictly between 0 and 1. The output reports method = "wald_sprt", the pooled counts, the thresholds, and the assumptions; the error rates hold under optional stopping because the test is sequential by construction.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#sprt_bernoulli

## Parameters

- `successes_a` — Successes in the first batch; integer in 0..=n_a. (integer)
- `n_a` — Trials in the first batch; integer >= 1. (integer)
- `successes_b` — Successes in the second batch; integer in 0..=n_b. (integer)
- `n_b` — Trials in the second batch; integer >= 1. (integer)
- `p0` — Null success probability; strictly between 0 and p1. (number)
- `p1` — Alternative success probability; strictly between p0 and 1. (number)
- `alpha` (optional) — Type I error probability in (0, 1); default 0.05. (number)
- `beta` (optional) — Type II error probability in (0, 1); default 0.1. (number)

## Output

Wald SPRT record with the pooled counts and decision boundaries.

## Examples

### clear evidence for the alternative

```json
{
  "n_a": {
    "kind": "integer",
    "value": "10"
  },
  "n_b": {
    "kind": "integer",
    "value": "10"
  },
  "p0": {
    "kind": "decimal",
    "value": "0.2"
  },
  "p1": {
    "kind": "decimal",
    "value": "0.4"
  },
  "successes_a": {
    "kind": "integer",
    "value": "9"
  },
  "successes_b": {
    "kind": "integer",
    "value": "9"
  }
}
```

Expected: `{"type":"contains","text":"accept_h1"}`

### the null is not below the alternative

```json
{
  "n_a": {
    "kind": "integer",
    "value": "10"
  },
  "n_b": {
    "kind": "integer",
    "value": "10"
  },
  "p0": {
    "kind": "decimal",
    "value": "0.4"
  },
  "p1": {
    "kind": "decimal",
    "value": "0.2"
  },
  "successes_a": {
    "kind": "integer",
    "value": "1"
  },
  "successes_b": {
    "kind": "integer",
    "value": "1"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="stddev"></a>

## stddev

Square root of the variance with explicit exactness handling.

Computes sqrt(variance(values, ddof)). When the variance is a perfect square in the selected representation the result is exact. Otherwise exact mode returns unsupported_numeric_mode, auto mode returns a decimal approximation marked approximate, and scientific mode returns float64. Float64 inputs require auto or scientific mode.

- Module: `statistics` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#stddev

## Parameters

- `values` — Observations. (array of number)
- `ddof` — Delta degrees of freedom: 0 for the population standard deviation, 1 for the sample. (integer)

## Output

Standard deviation.

## Examples

### exact perfect square

```json
{
  "ddof": {
    "kind": "integer",
    "value": "1"
  },
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
    }
  ]
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"1"}}`

### non-square variance in exact mode

```json
{
  "ddof": {
    "kind": "integer",
    "value": "0"
  },
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
    }
  ]
}
```

Expected: `{"type":"error","code":"unsupported_numeric_mode"}`


<a id="stratified_experiment"></a>

## stratified_experiment

Standardize per-stratum treatment-minus-control differences to a target mix.

strata is an array of records {name, target_weight, treatment, control}, where each arm is {assigned, outcomes: [{value, count}]}. Outcome counts must sum exactly to assigned. For each arm the mean of the per-person outcome and the unbiased sample variance (sum(value^2 * count) - n * mean^2) / (n - variance_ddof) are computed, together with the positive, negative, and net-positive outcome rates. Strata are standardized with weight = target_weight / sum(target_weight); the standardized treatment-minus-control variance is sum(weight^2 * (var_t / n_t + var_c / n_c)) and the interval is estimate +/- z * se with z = normal_quantile(1 - (1 - confidence)/2) (method normal_approximation_z_interval). Rate differences use the per-arm binomial variance p (1 - p) / n, and the net-positive rate uses the per-person variance p_positive + p_negative - net^2. Contribution, positive-rate, negative-rate, and net-positive-rate differences are reported in separate blocks; a positive outcome rate difference is not a positive contribution difference. The sum of target weights is also the represented population scale: per-person standardized quantities are multiplied by it to report totals. fixed_cost is subtracted from the scaled total contribution after the interval and adds no sampling variance. rollout_fraction scales the expected difference and its variance by fraction^2; the full and scaled results are both reported.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#stratified_experiment

## Parameters

- `strata` — Array of stratum records {name, target_weight, treatment, control}. (array of any value)
- `confidence` (optional) — Confidence level in (0, 1); default 0.95. (number)
- `fixed_cost` (optional) — Known fixed cost subtracted from the scaled contribution difference; default 0. (number)
- `rollout_fraction` (optional) — Fraction of the population receiving the treatment; in [0, 1], default 1. (number)
- `variance_ddof` (optional) — Delta degrees of freedom for the per-arm variance; default 1. (integer)

## Output

Standardized experiment analysis with separate contribution and rate blocks.

## Examples

### outcome counts must sum to assigned

```json
{
  "strata": [
    {
      "control": {
        "assigned": {
          "kind": "integer",
          "value": "2"
        },
        "outcomes": [
          {
            "count": {
              "kind": "integer",
              "value": "2"
            },
            "value": {
              "kind": "integer",
              "value": "10"
            }
          }
        ]
      },
      "name": "beginners",
      "target_weight": {
        "kind": "integer",
        "value": "1"
      },
      "treatment": {
        "assigned": {
          "kind": "integer",
          "value": "3"
        },
        "outcomes": [
          {
            "count": {
              "kind": "integer",
              "value": "2"
            },
            "value": {
              "kind": "integer",
              "value": "10"
            }
          }
        ]
      }
    }
  ]
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="student_t_cdf"></a>

## student_t_cdf

Cumulative distribution function of Student's t distribution.

For x > 0, cdf(x) = 1 - 0.5 * I_y(df/2, 1/2) with y = df / (df + x^2); for x < 0 the mirrored expression is used. I_y is the regularized incomplete beta function.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#student_t_cdf

## Parameters

- `x` — Quantile. (number)
- `df` — Degrees of freedom; must be > 0. (number)

## Output

Student-t distribution value.

## Examples

### symmetric center

```json
{
  "df": {
    "kind": "integer",
    "value": "5"
  },
  "x": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0.5"}}`


<a id="student_t_pdf"></a>

## student_t_pdf

Probability density of Student's t distribution.

pdf(x) = Gamma((df + 1) / 2) / (sqrt(df * pi) Gamma(df / 2)) * (1 + x^2 / df)^(-(df + 1) / 2), computed with a Lanczos log-gamma.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#student_t_pdf

## Parameters

- `x` — Quantile. (number)
- `df` — Degrees of freedom; must be > 0. (number)

## Output

Student-t distribution value.

## Examples

### zero degrees of freedom

```json
{
  "df": {
    "kind": "integer",
    "value": "0"
  },
  "x": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="student_t_quantile"></a>

## student_t_quantile

Inverse Student-t CDF.

Returns x such that cdf(x) = p, solved by bracket expansion, bisection, and Newton polishing to a relative tolerance of 1e-15. p must be strictly between 0 and 1 and df must be positive.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#student_t_quantile

## Parameters

- `p` — Probability in (0, 1). (number)
- `df` — Degrees of freedom; must be > 0. (number)

## Output

Student-t quantile.

## Examples

### median

```json
{
  "df": {
    "kind": "integer",
    "value": "5"
  },
  "p": {
    "kind": "decimal",
    "value": "0.5"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0"}}`


<a id="student_t_sf"></a>

## student_t_sf

Upper tail probability of Student's t distribution.

For x > 0, sf(x) = 0.5 * I_y(df/2, 1/2) with y = df / (df + x^2); the upper tail is computed directly from the regularized incomplete beta function, not as 1 - cdf.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#student_t_sf

## Parameters

- `x` — Quantile. (number)
- `df` — Degrees of freedom; must be > 0. (number)

## Output

Student-t distribution value.

## Examples

### symmetric center

```json
{
  "df": {
    "kind": "integer",
    "value": "5"
  },
  "x": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0.5"}}`


<a id="summary"></a>

## summary

One-call descriptive summary with quantiles.

Returns count, mean, min, max, median, sample variance (ddof = 1), sample standard deviation, and the linear quantiles 0.25, 0.5, and 0.75. At least two observations are required. Exact inputs stay exact wherever the representation allows; the standard deviation follows the stddev exactness rules.

- Module: `statistics` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#summary

## Parameters

- `values` — Observations. (array of number)

## Output

Record with count, mean, min, max, median, variance, stddev, and quantiles.

## Examples

### too few observations

```json
{
  "values": [
    {
      "kind": "integer",
      "value": "1"
    }
  ]
}
```

Expected: `{"type":"error","code":"insufficient_observations"}`


<a id="tost_two_means"></a>

## tost_two_means

TOST equivalence test for two independent means (Welch).

Tests H0: |mean_a - mean_b| >= margin against the two one-sided alternatives at alpha = 1 - confidence, using the Welch standard error and Welch-Satterthwaite degrees of freedom. The reported p_value is the larger of the two one-sided p-values; equivalence holds when p_value < 1 - confidence. ci_lower and ci_upper are the two-sided confidence interval for the difference with the same alpha. Each sample needs at least two observations, margin must be strictly positive, and the Welch standard error must be positive.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#tost_two_means

## Parameters

- `sample_a` — First sample. (array of number)
- `sample_b` — Second sample. (array of number)
- `margin` — Equivalence margin; must be > 0. (number)
- `confidence` (optional) — Confidence level in (0, 1); default 0.95. (number)

## Output

TOST equivalence record.

## Examples

### equivalent samples

```json
{
  "margin": {
    "kind": "integer",
    "value": "2"
  },
  "sample_a": [
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
  ],
  "sample_b": [
    {
      "kind": "decimal",
      "value": "1.1"
    },
    {
      "kind": "decimal",
      "value": "2.1"
    },
    {
      "kind": "decimal",
      "value": "3.1"
    },
    {
      "kind": "decimal",
      "value": "4.1"
    }
  ]
}
```

Expected: `{"type":"contains","text":"tost_welch"}`

### zero margin

```json
{
  "margin": {
    "kind": "integer",
    "value": "0"
  },
  "sample_a": [
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
  "sample_b": [
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

Expected: `{"type":"error","code":"domain_violation"}`


<a id="uniform_cdf"></a>

## uniform_cdf

Cumulative distribution function of the continuous uniform distribution.

cdf(x) = clamp((x - lower) / (upper - lower), 0, 1). upper must be strictly greater than lower.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#uniform_cdf

## Parameters

- `x` — Quantile. (number)
- `lower` — Lower bound. (number)
- `upper` — Upper bound; must be > lower. (number)

## Output

Distribution value.

## Examples

### midpoint

```json
{
  "lower": {
    "kind": "integer",
    "value": "0"
  },
  "upper": {
    "kind": "integer",
    "value": "1"
  },
  "x": {
    "kind": "decimal",
    "value": "0.5"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0.5"}}`

### empty interval

```json
{
  "lower": {
    "kind": "integer",
    "value": "1"
  },
  "upper": {
    "kind": "integer",
    "value": "0"
  },
  "x": {
    "kind": "decimal",
    "value": "0.5"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="uniform_pdf"></a>

## uniform_pdf

Probability density of the continuous uniform distribution.

pdf(x) = 1 / (upper - lower) for lower <= x <= upper and 0 outside. upper must be strictly greater than lower.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#uniform_pdf

## Parameters

- `x` — Quantile. (number)
- `lower` — Lower bound. (number)
- `upper` — Upper bound; must be > lower. (number)

## Output

Distribution value.

## Examples

### unit interval

```json
{
  "lower": {
    "kind": "integer",
    "value": "0"
  },
  "upper": {
    "kind": "integer",
    "value": "1"
  },
  "x": {
    "kind": "decimal",
    "value": "0.5"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"1"}}`

### empty interval

```json
{
  "lower": {
    "kind": "integer",
    "value": "1"
  },
  "upper": {
    "kind": "integer",
    "value": "1"
  },
  "x": {
    "kind": "decimal",
    "value": "0.5"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="uniform_quantile"></a>

## uniform_quantile

Inverse continuous uniform CDF.

Returns lower + p * (upper - lower). p must be in [0, 1] and upper must be strictly greater than lower.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#uniform_quantile

## Parameters

- `p` — Probability in [0, 1]. (number)
- `lower` — Lower bound. (number)
- `upper` — Upper bound; must be > lower. (number)

## Output

Distribution value.

## Examples

### median

```json
{
  "lower": {
    "kind": "integer",
    "value": "0"
  },
  "p": {
    "kind": "decimal",
    "value": "0.5"
  },
  "upper": {
    "kind": "integer",
    "value": "1"
  }
}
```

Expected: `{"type":"value","value":{"kind":"float64","value":"0.5"}}`

### probability above one

```json
{
  "lower": {
    "kind": "integer",
    "value": "0"
  },
  "p": {
    "kind": "decimal",
    "value": "1.5"
  },
  "upper": {
    "kind": "integer",
    "value": "1"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="variance"></a>

## variance

Sample or population variance with explicit delta degrees of freedom.

Computes sum((x - mean)^2) / (n - ddof). ddof must be a non-negative integer and n must be strictly greater than ddof, otherwise insufficient_observations is returned. Exact inputs use exact rational arithmetic (variance([1, 2, 3], ddof=0) = 2/3); float64 inputs use Welford's stable one-pass algorithm and require auto or scientific mode.

- Module: `statistics` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#variance

## Parameters

- `values` — Observations. (array of number)
- `ddof` — Delta degrees of freedom: 0 for the population variance, 1 for the sample variance. (integer)

## Output

Variance.

## Examples

### population variance

```json
{
  "ddof": {
    "kind": "integer",
    "value": "0"
  },
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
    }
  ]
}
```

Expected: `{"type":"value","value":{"kind":"rational","numerator":"2","denominator":"3"}}`

### sample variance

```json
{
  "ddof": {
    "kind": "integer",
    "value": "1"
  },
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
    }
  ]
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"1"}}`

### too few observations

```json
{
  "ddof": {
    "kind": "integer",
    "value": "1"
  },
  "values": [
    {
      "kind": "integer",
      "value": "1"
    }
  ]
}
```

Expected: `{"type":"error","code":"insufficient_observations"}`


<a id="weighted_mean"></a>

## weighted_mean

Weighted arithmetic mean with frequency or reliability weights.

Frequency weights must be non-negative integers and count repeated observations; reliability weights must be non-negative numbers. Weights may be zero but the total weight must be positive. Negative weights, length mismatches, and a zero total weight are rejected. Exact inputs produce an exact rational result; float64 inputs require auto or scientific mode and are marked approximate.

- Module: `statistics` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#weighted_mean

## Parameters

- `values` — Observations. (array of number)
- `weights` — Non-negative weights, one per observation. (array of number)
- `weight_type` (optional) — Weight semantics: frequency (default) or reliability. (one of ["frequency", "reliability"])

## Output

Weighted mean.

## Examples

### frequency weights

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
    }
  ],
  "weights": [
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

Expected: `{"type":"value","value":{"kind":"rational","numerator":"3","denominator":"2"}}`

### negative weight

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
    }
  ],
  "weights": [
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

Expected: `{"type":"error","code":"domain_violation"}`


<a id="welch_ci"></a>

## welch_ci

Welch interval for mean_a - mean_b without an equal-variance assumption.

Returns the difference of means with the Welch-Satterthwaite degrees of freedom and a Student-t interval. The equal-variance assumption of the pooled two-sample interval is deliberately not applied. Each sample must contain at least two observations and the standard error must be positive (two constant samples have an undefined interval).

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#welch_ci

## Parameters

- `sample_a` — First sample. (array of number)
- `sample_b` — Second sample. (array of number)
- `confidence` (optional) — Confidence level in (0, 1); default 0.95. (number)

## Output

Welch confidence interval record.

## Examples

### one observation per sample

```json
{
  "sample_a": [
    {
      "kind": "integer",
      "value": "1"
    }
  ],
  "sample_b": [
    {
      "kind": "integer",
      "value": "2"
    }
  ]
}
```

Expected: `{"type":"error","code":"insufficient_observations"}`


<a id="wls"></a>

## wls

Weighted least squares regression with pivoted normal equations.

Minimizes sum(w_i (y_i - x_i b)^2) by solving the weighted normal equations with partial pivoting. weights must be non-negative with a strictly positive total; a zero weight drops the observation. The output shape matches statistics.ols with method = "wls". A rank-deficient or numerically singular design is rejected with ill_conditioned.

- Module: `statistics` (version 1.0.0)
- Modes: auto, scientific
- Cost: Cubic
- Determinism: Deterministic
- Method reference: docs/methods/statistics.md#wls

## Parameters

- `y` — Response values. (array of number)
- `x` — Flat predictor array or array of predictor rows. (any value)
- `weights` — Non-negative observation weights, one per observation. (array of number)
- `intercept` (optional) — Include an intercept column; default true. (boolean)

## Output

Weighted least-squares fit record.

## Examples

### equal weights match ordinary least squares

```json
{
  "weights": [
    {
      "kind": "integer",
      "value": "1"
    },
    {
      "kind": "integer",
      "value": "1"
    },
    {
      "kind": "integer",
      "value": "1"
    }
  ],
  "x": [
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
  "y": [
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

Expected: `{"type":"contains","text":"wls"}`

### negative weight

```json
{
  "weights": [
    {
      "kind": "integer",
      "value": "1"
    },
    {
      "kind": "integer",
      "value": "-1"
    },
    {
      "kind": "integer",
      "value": "1"
    }
  ],
  "x": [
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
  "y": [
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

Expected: `{"type":"error","code":"domain_violation"}`


