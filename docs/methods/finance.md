# Finance module reference

Money operations, cash flows, interest, amortization, options, bonds, portfolio risk, and contribution analysis.

- Module id: `finance`
- Version: 1.0.0
- Capabilities: money, currency_safety, cash_flows, interest, amortization, rate_solving, day_count, options, fixed_income, portfolio_risk, capm
- Supported modes: exact, auto, scientific
- Functions: 34

This file is generated from the live registry by `bicmath docs`; the function schemas, domains, and examples are the same ones the engine validates against at runtime.

<a id="amortization"></a>

## amortization

Level-payment amortization schedule with exact principal reconciliation.

Builds a level-payment schedule for principal at the periodic rate annual_rate (a 6% nominal annual rate compounded monthly is passed as 0.005 with 12 periods). payment_timing selects ordinary (default) or annuity_due. rounding selects per_period (default), which rounds each period's interest and payment to the settlement scale of 2, or final_only, which keeps intermediate values at full precision and rounds only the final payment. In both modes the final row is adjusted so the principal column sums EXACTLY to the initial principal and the closing balance is exactly zero; reconciliation reports the totals, the final payment adjustment, and reconciles = true.

- Module: `finance` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/finance.md#amortization

## Parameters

- `principal` — Loan principal. (money)
- `annual_rate` — Periodic rate as an exact decimal. (exact number (integer, rational, or decimal))
- `periods` — Number of periods; at least 1. (integer)
- `currency` — Currency of the principal; must match. (text)
- `payment_timing` (optional) — ordinary (default) or annuity_due. (one of ["ordinary", "annuity_due"])
- `rounding` (optional) — per_period (default) or final_only. (one of ["per_period", "final_only"])

## Output

Schedule rows plus a reconciliation record.

## Examples

### zero-rate 300 over 3 periods

```json
{
  "annual_rate": "0",
  "currency": "USD",
  "periods": {
    "kind": "integer",
    "value": "3"
  },
  "principal": {
    "kind": "money",
    "amount": {
      "kind": "decimal",
      "value": "300.00"
    },
    "currency": "USD"
  }
}
```

Expected: `{"type":"value","value":{"annual_rate":{"kind":"decimal","value":"0"},"currency":"USD","payment_timing":"ordinary","reconciliation":{"final_payment_adjustment":{"kind":"money","amount":{"kind":"decimal","value":"0.00"},"currency":"USD"},"reconciles":true,"total_interest":{"kind":"money","amount":{"kind":"decimal","value":"0.00"},"currency":"USD"},"total_payments":{"kind":"money","amount":{"kind":"decimal","value":"300.00"},"currency":"USD"},"total_principal":{"kind":"money","amount":{"kind":"decimal","value":"300.00"},"currency":"USD"}},"rounding":"per_period","schedule":[{"balance":{"kind":"money","amount":{"kind":"decimal","value":"200.00"},"currency":"USD"},"interest":{"kind":"money","amount":{"kind":"decimal","value":"0.00"},"currency":"USD"},"payment":{"kind":"money","amount":{"kind":"decimal","value":"100.00"},"currency":"USD"},"period":{"kind":"integer","value":"1"},"principal":{"kind":"money","amount":{"kind":"decimal","value":"100.00"},"currency":"USD"}},{"balance":{"kind":"money","amount":{"kind":"decimal","value":"100.00"},"currency":"USD"},"interest":{"kind":"money","amount":{"kind":"decimal","value":"0.00"},"currency":"USD"},"payment":{"kind":"money","amount":{"kind":"decimal","value":"100.00"},"currency":"USD"},"period":{"kind":"integer","value":"2"},"principal":{"kind":"money","amount":{"kind":"decimal","value":"100.00"},"currency":"USD"}},{"balance":{"kind":"money","amount":{"kind":"decimal","value":"0.00"},"currency":"USD"},"interest":{"kind":"money","amount":{"kind":"decimal","value":"0.00"},"currency":"USD"},"payment":{"kind":"money","amount":{"kind":"decimal","value":"100.00"},"currency":"USD"},"period":{"kind":"integer","value":"3"},"principal":{"kind":"money","amount":{"kind":"decimal","value":"100.00"},"currency":"USD"}}]}}`


<a id="beta"></a>

## beta

Sample beta of an asset relative to a market return series.

beta = sample_covariance(asset_returns, market_returns) / sample_variance(market_returns), with the n - 1 denominator in both terms. The series must have the same length, at least two observations, and the market variance must be positive. The result is exact when the ratio terminates as a decimal and rounded otherwise; a series compared with itself has beta 1.

- Module: `finance` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/finance.md#beta
- Units/currency rule: both series are per-period return decimals

## Parameters

- `asset_returns` — Asset return series; at least two observations. (array of exact number (integer, rational, or decimal))
- `market_returns` — Market return series of the same length. (array of exact number (integer, rational, or decimal))

## Output

Sample beta.

## Examples

### a series against itself

```json
{
  "asset_returns": [
    {
      "kind": "decimal",
      "value": "0.02"
    },
    {
      "kind": "decimal",
      "value": "-0.01"
    },
    {
      "kind": "decimal",
      "value": "0.03"
    },
    {
      "kind": "decimal",
      "value": "0.05"
    },
    {
      "kind": "decimal",
      "value": "-0.02"
    }
  ],
  "market_returns": [
    {
      "kind": "decimal",
      "value": "0.02"
    },
    {
      "kind": "decimal",
      "value": "-0.01"
    },
    {
      "kind": "decimal",
      "value": "0.03"
    },
    {
      "kind": "decimal",
      "value": "0.05"
    },
    {
      "kind": "decimal",
      "value": "-0.02"
    }
  ]
}
```

Expected: `{"type":"value","value":{"kind":"decimal","value":"1"}}`

### constant market

```json
{
  "asset_returns": [
    {
      "kind": "decimal",
      "value": "0.01"
    },
    {
      "kind": "decimal",
      "value": "0.02"
    }
  ],
  "market_returns": [
    {
      "kind": "decimal",
      "value": "0.01"
    },
    {
      "kind": "decimal",
      "value": "0.01"
    }
  ]
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="binomial_option"></a>

## binomial_option

European or American call or put price on a CRR binomial tree.

Builds a Cox-Ross-Rubinstein recombining tree with dt = T / steps, u = exp(sigma sqrt(dt)), d = 1 / u, and risk-neutral probability p = (exp(r dt) - d) / (u - d). European options take the discounted expected value; American options allow early exercise at each node. Zero volatility is allowed and collapses the tree to the deterministic discounted forward intrinsic. steps must be at least 1 and at most 5000 and must fit the execution context's array and operation budgets.

- Module: `finance` (version 1.0.0)
- Modes: auto, scientific
- Cost: Quadratic
- Determinism: Deterministic
- Method reference: docs/methods/finance.md#binomial_option
- Units/currency rule: spot and strike share one price unit; rate and volatility are annual decimals

## Parameters

- `option_type` — call or put. (one of ["call", "put"])
- `spot` — Spot price; must be positive. (exact number (integer, rational, or decimal))
- `strike` — Strike price; must be positive. (exact number (integer, rational, or decimal))
- `rate` — Continuously compounded risk-free rate as an exact decimal. (exact number (integer, rational, or decimal))
- `volatility` — Annualized volatility as an exact decimal; zero is allowed. (exact number (integer, rational, or decimal))
- `time` — Time to expiry in years; must be positive. (exact number (integer, rational, or decimal))
- `steps` — Number of tree steps; 1..=5000 and within the context limits. (integer)
- `american` (optional) — Allow early exercise; default false. (boolean)

## Output

Option price, step count, early-exercise flag, and method.

## Examples

### zero volatility one-step call

```json
{
  "option_type": "call",
  "rate": "0",
  "spot": {
    "kind": "integer",
    "value": "100"
  },
  "steps": {
    "kind": "integer",
    "value": "1"
  },
  "strike": {
    "kind": "integer",
    "value": "90"
  },
  "time": {
    "kind": "integer",
    "value": "1"
  },
  "volatility": "0"
}
```

Expected: `{"type":"value","value":{"american":false,"method":"crr_binomial","price":{"kind":"float64","value":"10"},"steps":{"kind":"integer","value":"1"}}}`

### one hundred step call

```json
{
  "option_type": "call",
  "rate": "0.05",
  "spot": {
    "kind": "integer",
    "value": "100"
  },
  "steps": {
    "kind": "integer",
    "value": "100"
  },
  "strike": {
    "kind": "integer",
    "value": "100"
  },
  "time": {
    "kind": "integer",
    "value": "1"
  },
  "volatility": "0.2"
}
```

Expected: `{"type":"contains","text":"crr_binomial"}`

### zero steps

```json
{
  "option_type": "call",
  "rate": "0",
  "spot": {
    "kind": "integer",
    "value": "100"
  },
  "steps": {
    "kind": "integer",
    "value": "0"
  },
  "strike": {
    "kind": "integer",
    "value": "90"
  },
  "time": {
    "kind": "integer",
    "value": "1"
  },
  "volatility": "0.2"
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="black_scholes"></a>

## black_scholes

European call or put price and greeks with a continuous dividend yield.

Computes the Black-Scholes-Merton price of a European option and its delta, gamma, vega, theta (per year), and rho. d1 = (ln(S/K) + (r - q + sigma^2/2) T) / (sigma sqrt(T)) and d2 = d1 - sigma sqrt(T). The volatility may be zero: the option is then priced at its discounted forward intrinsic value, the greeks take their zero-volatility limits, and d1/d2 are null because the standardized moneyness has no unique limit. Option pricing is binary64 and reported approximate; spot and strike must be positive and time must be positive.

- Module: `finance` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/finance.md#black_scholes
- Units/currency rule: spot and strike share one price unit; rate, volatility, and dividend_yield are annual decimals

## Parameters

- `option_type` — call or put. (one of ["call", "put"])
- `spot` — Spot price; must be positive. (exact number (integer, rational, or decimal))
- `strike` — Strike price; must be positive. (exact number (integer, rational, or decimal))
- `rate` — Continuously compounded risk-free rate as an exact decimal. (exact number (integer, rational, or decimal))
- `volatility` — Annualized volatility as an exact decimal; zero is allowed. (exact number (integer, rational, or decimal))
- `time` — Time to expiry in years; must be positive. (exact number (integer, rational, or decimal))
- `dividend_yield` (optional) — Continuous dividend yield; default 0. (exact number (integer, rational, or decimal))

## Output

Price, delta, gamma, vega, theta, rho, d1, d2, and method.

## Examples

### zero volatility prices the discounted forward intrinsic

```json
{
  "option_type": "call",
  "rate": "0",
  "spot": {
    "kind": "integer",
    "value": "100"
  },
  "strike": {
    "kind": "integer",
    "value": "90"
  },
  "time": {
    "kind": "integer",
    "value": "1"
  },
  "volatility": "0"
}
```

Expected: `{"type":"value","value":{"d1":null,"d2":null,"delta":{"kind":"float64","value":"1"},"gamma":{"kind":"float64","value":"0"},"method":"black_scholes","price":{"kind":"float64","value":"10"},"rho":{"kind":"float64","value":"90"},"theta":{"kind":"float64","value":"0"},"vega":{"kind":"float64","value":"0"}}}`

### at-the-money call

```json
{
  "option_type": "call",
  "rate": "0.05",
  "spot": {
    "kind": "integer",
    "value": "100"
  },
  "strike": {
    "kind": "integer",
    "value": "100"
  },
  "time": {
    "kind": "integer",
    "value": "1"
  },
  "volatility": "0.2"
}
```

Expected: `{"type":"contains","text":"black_scholes"}`

### negative volatility

```json
{
  "option_type": "call",
  "rate": "0.05",
  "spot": {
    "kind": "integer",
    "value": "100"
  },
  "strike": {
    "kind": "integer",
    "value": "100"
  },
  "time": {
    "kind": "integer",
    "value": "1"
  },
  "volatility": "-0.2"
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="bond_duration"></a>

## bond_duration

Macaulay duration, modified duration, and convexity of a fixed-coupon bond.

Durations are expressed in years. Macaulay duration is the present-value weighted average time to each cash flow: sum((t/f) * PV_t) / price. Modified duration is Macaulay / (1 + y/f) and convexity is sum((t/f) * ((t/f) + 1/f) * PV_t) / (price * (1 + y/f)^2). Coupons follow the per-period convention coupon = face_value * coupon_rate / f and y = yield_rate / f. Arithmetic is exact decimal when every division terminates and rounded otherwise.

- Module: `finance` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/finance.md#bond_duration
- Units/currency rule: durations are in years; rates are per-period decimals

## Parameters

- `face_value` — Face (redemption) value. (money)
- `coupon_rate` — Coupon rate per period as an exact decimal. (exact number (integer, rational, or decimal))
- `periods` — Number of coupon periods; at least 1. (integer)
- `yield_rate` — Yield per period as an exact decimal. (exact number (integer, rational, or decimal))
- `frequency` (optional) — Coupon payments per year; default 1. (integer)

## Output

Macaulay duration (years), modified duration (years), convexity (years^2), and method.

## Examples

### two-period zero coupon bond

```json
{
  "coupon_rate": "0",
  "face_value": {
    "kind": "money",
    "amount": {
      "kind": "decimal",
      "value": "1000.00"
    },
    "currency": "USD"
  },
  "periods": {
    "kind": "integer",
    "value": "2"
  },
  "yield_rate": "0"
}
```

Expected: `{"type":"value","value":{"convexity":{"kind":"decimal","value":"6"},"macaulay_duration":{"kind":"decimal","value":"2"},"method":"bond_duration","modified_duration":{"kind":"decimal","value":"2"}}}`

### five percent coupon at four percent yield

```json
{
  "coupon_rate": "0.05",
  "face_value": {
    "kind": "money",
    "amount": {
      "kind": "decimal",
      "value": "1000.00"
    },
    "currency": "USD"
  },
  "periods": {
    "kind": "integer",
    "value": "3"
  },
  "yield_rate": "0.04"
}
```

Expected: `{"type":"contains","text":"bond_duration"}`


<a id="bond_price"></a>

## bond_price

Dirty price of a fixed-coupon bond from face value, coupon rate, periods, and yield.

With frequency f, the coupon per period is face_value * coupon_rate / f and the per-period discount rate is yield_rate / f, so the price is sum(coupon / (1 + y/f)^t) + face_value / (1 + y/f)^periods for t = 1..=periods. frequency defaults to 1. The price is a money amount in the currency of face_value and keeps full decimal precision (bond prices are quoted with fractions of a cent); total_coupons is the undiscounted coupon sum, quantized to settlement scale 2. Arithmetic is exact decimal when every division terminates and rounded otherwise.

- Module: `finance` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/finance.md#bond_price
- Units/currency rule: all money amounts share the currency of face_value; rates are per-period decimals

## Parameters

- `face_value` — Face (redemption) value. (money)
- `coupon_rate` — Coupon rate per period as an exact decimal (0.05 means 5%). (exact number (integer, rational, or decimal))
- `periods` — Number of coupon periods; at least 1. (integer)
- `yield_rate` — Yield per period as an exact decimal. (exact number (integer, rational, or decimal))
- `frequency` (optional) — Coupon payments per year; default 1. (integer)

## Output

Bond price, total undiscounted coupons, and method.

## Examples

### zero yield price is face plus all coupons

```json
{
  "coupon_rate": "0.05",
  "face_value": {
    "kind": "money",
    "amount": {
      "kind": "decimal",
      "value": "1000.00"
    },
    "currency": "USD"
  },
  "periods": {
    "kind": "integer",
    "value": "3"
  },
  "yield_rate": "0"
}
```

Expected: `{"type":"value","value":{"method":"bond_price","price":{"kind":"money","amount":{"kind":"decimal","value":"1150"},"currency":"USD"},"total_coupons":{"kind":"money","amount":{"kind":"decimal","value":"150.00"},"currency":"USD"}}}`

### five percent coupon at four percent yield

```json
{
  "coupon_rate": "0.05",
  "face_value": {
    "kind": "money",
    "amount": {
      "kind": "decimal",
      "value": "1000.00"
    },
    "currency": "USD"
  },
  "periods": {
    "kind": "integer",
    "value": "3"
  },
  "yield_rate": "0.04"
}
```

Expected: `{"type":"contains","text":"bond_price"}`


<a id="bond_yield"></a>

## bond_yield

Yield to maturity implied by a bond price, solved with a bracketed method.

Solves for the per-period yield y such that the bond price equals the supplied money price. The price function is strictly decreasing in y, so a bracket is found by scanning and then refined with bisection and Brent's method. The returned yield is per period and must be multiplied by frequency to obtain the annual yield. tolerance defaults to 1e-12 and max_iterations to 200 (capped by the execution context). If no bracket exists or the iteration budget is exhausted, the function returns non_convergence rather than a guess.

- Module: `finance` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/finance.md#bond_yield
- Units/currency rule: face_value and price must share one currency; yield is per period

## Parameters

- `face_value` — Face (redemption) value. (money)
- `coupon_rate` — Coupon rate per period as an exact decimal. (exact number (integer, rational, or decimal))
- `periods` — Number of coupon periods; at least 1. (integer)
- `price` — Observed dirty price as money in the currency of face_value. (money)
- `frequency` (optional) — Coupon payments per year; default 1. (integer)
- `tolerance` (optional) — Absolute yield tolerance; default 1e-12. (exact number (integer, rational, or decimal))
- `max_iterations` (optional) — Maximum solver iterations; default 200. (integer)

## Output

Per-period yield, iterations used, convergence flag, and method.

## Examples

### at par yield equals the coupon rate

```json
{
  "coupon_rate": "0.05",
  "face_value": {
    "kind": "money",
    "amount": {
      "kind": "decimal",
      "value": "1000.00"
    },
    "currency": "USD"
  },
  "periods": {
    "kind": "integer",
    "value": "3"
  },
  "price": {
    "kind": "money",
    "amount": {
      "kind": "decimal",
      "value": "1000.00"
    },
    "currency": "USD"
  }
}
```

Expected: `{"type":"contains","text":"bond_yield_bisection_brent"}`

### zero price has no yield

```json
{
  "coupon_rate": "0.05",
  "face_value": {
    "kind": "money",
    "amount": {
      "kind": "decimal",
      "value": "1000.00"
    },
    "currency": "USD"
  },
  "periods": {
    "kind": "integer",
    "value": "3"
  },
  "price": {
    "kind": "money",
    "amount": {
      "kind": "decimal",
      "value": "0"
    },
    "currency": "USD"
  }
}
```

Expected: `{"type":"error","code":"non_convergence"}`


<a id="break_even"></a>

## break_even

Break-even units and revenue from fixed costs and unit economics.

All three amounts must share one currency. The unit contribution must be strictly positive; otherwise the break-even point does not exist and a domain error is returned. Break-even units are rounded up to whole units and the break-even revenue is that whole number of units times the unit price.

- Module: `finance` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/finance.md#break_even

## Parameters

- `fixed_costs` — Total fixed costs. (money)
- `unit_price` — Selling price per unit. (money)
- `unit_variable_cost` — Variable cost per unit. (money)

## Output

Unit contribution, whole break-even units, break-even revenue, and method.

## Examples

### break even at 250 units

```json
{
  "fixed_costs": {
    "kind": "money",
    "amount": {
      "kind": "decimal",
      "value": "1000.00"
    },
    "currency": "USD"
  },
  "unit_price": {
    "kind": "money",
    "amount": {
      "kind": "decimal",
      "value": "10.00"
    },
    "currency": "USD"
  },
  "unit_variable_cost": {
    "kind": "money",
    "amount": {
      "kind": "decimal",
      "value": "6.00"
    },
    "currency": "USD"
  }
}
```

Expected: `{"type":"value","value":{"break_even_revenue":{"kind":"money","amount":{"kind":"decimal","value":"2500.00"},"currency":"USD"},"break_even_units":{"kind":"integer","value":"250"},"method":"unit_contribution = unit_price - unit_variable_cost; break_even_units = ceil(fixed_costs / unit_contribution); break_even_revenue = break_even_units * unit_price","unit_contribution":{"kind":"money","amount":{"kind":"decimal","value":"4.00"},"currency":"USD"}}}`

### negative unit contribution

```json
{
  "fixed_costs": {
    "kind": "money",
    "amount": {
      "kind": "decimal",
      "value": "1000.00"
    },
    "currency": "USD"
  },
  "unit_price": {
    "kind": "money",
    "amount": {
      "kind": "decimal",
      "value": "6.00"
    },
    "currency": "USD"
  },
  "unit_variable_cost": {
    "kind": "money",
    "amount": {
      "kind": "decimal",
      "value": "10.00"
    },
    "currency": "USD"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="capm"></a>

## capm

Expected return and risk premium from the Capital Asset Pricing Model.

expected_return = risk_free_rate + beta * (market_return - risk_free_rate) and risk_premium = beta * (market_return - risk_free_rate). All three inputs are exact decimals and the arithmetic is exact.

- Module: `finance` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/finance.md#capm
- Units/currency rule: all rates are per-period decimals

## Parameters

- `risk_free_rate` — Per-period risk-free rate as an exact decimal. (exact number (integer, rational, or decimal))
- `beta` — Asset beta as an exact decimal. (exact number (integer, rational, or decimal))
- `market_return` — Expected market return as an exact decimal. (exact number (integer, rational, or decimal))

## Output

CAPM expected return, risk premium, and method.

## Examples

### beta 1.2 with a five percent market premium

```json
{
  "beta": "1.2",
  "market_return": "0.08",
  "risk_free_rate": "0.03"
}
```

Expected: `{"type":"value","value":{"expected_return":{"kind":"decimal","value":"0.09"},"method":"capm","risk_premium":{"kind":"decimal","value":"0.06"}}}`


<a id="compound_interest"></a>

## compound_interest

Compound interest on a principal with a nominal annual rate.

total = principal * (1 + annual_rate / compounds_per_year) ^ (compounds_per_year * years). The annual_rate is a nominal annual rate whose units must agree with compounds_per_year: a 6% nominal annual rate compounded monthly is passed as 0.06 with compounds_per_year = 12, giving a per-period rate of 0.005. The total is rounded to settlement scale 2 (half-even); when the exponent is not a whole number the computation uses binary64 explicitly and is reported rounded.

- Module: `finance` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/finance.md#compound_interest

## Parameters

- `principal` — Principal amount. (money)
- `annual_rate` — Nominal annual rate as an exact decimal (0.06 means 6%). (exact number (integer, rational, or decimal))
- `years` — Number of years as an exact decimal. (exact number (integer, rational, or decimal))
- `compounds_per_year` — Compounding periods per year; at least 1. (integer)

## Output

Interest and total (principal + interest).

## Examples

### 5% compounded annually for 2 years

```json
{
  "annual_rate": "0.05",
  "compounds_per_year": {
    "kind": "integer",
    "value": "1"
  },
  "principal": {
    "kind": "money",
    "amount": {
      "kind": "decimal",
      "value": "1000.00"
    },
    "currency": "USD"
  },
  "years": {
    "kind": "integer",
    "value": "2"
  }
}
```

Expected: `{"type":"value","value":{"interest":{"kind":"money","amount":{"kind":"decimal","value":"102.50"},"currency":"USD"},"total":{"kind":"money","amount":{"kind":"decimal","value":"1102.50"},"currency":"USD"}}}`


<a id="conditional_value_at_risk"></a>

## conditional_value_at_risk

Expected shortfall beyond the value-at-risk threshold.

Expected loss conditional on exceeding the value-at-risk threshold. The historical method averages every return at or below the historical quantile; the normal method returns sigma * phi(Phi^-1(confidence)) / (1 - confidence) - mean, where sigma is the sample standard deviation. The normal method is rounded; the historical method is exact when the average terminates as a decimal.

- Module: `finance` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/finance.md#conditional_value_at_risk
- Units/currency rule: returns and conditional value at risk are per-period decimals

## Parameters

- `returns` — Return series; at least one observation (two for the normal method). (array of exact number (integer, rational, or decimal))
- `confidence` — Confidence level strictly between 0 and 1. (exact number (integer, rational, or decimal))
- `method` (optional) — historical (default) or normal. (one of ["historical", "normal"])

## Output

Conditional value at risk, method, and the confidence level used.

## Examples

### historical expected shortfall

```json
{
  "confidence": "0.8",
  "returns": [
    {
      "kind": "decimal",
      "value": "-0.3"
    },
    {
      "kind": "decimal",
      "value": "-0.2"
    },
    {
      "kind": "decimal",
      "value": "-0.1"
    },
    {
      "kind": "integer",
      "value": "0"
    },
    {
      "kind": "decimal",
      "value": "0.4"
    }
  ]
}
```

Expected: `{"type":"value","value":{"confidence":{"kind":"decimal","value":"0.8"},"cvar":{"kind":"decimal","value":"0.25"},"method":"historical"}}`

### normal expected shortfall

```json
{
  "confidence": "0.95",
  "method": "normal",
  "returns": [
    {
      "kind": "decimal",
      "value": "-0.3"
    },
    {
      "kind": "decimal",
      "value": "-0.2"
    },
    {
      "kind": "decimal",
      "value": "-0.1"
    },
    {
      "kind": "integer",
      "value": "0"
    },
    {
      "kind": "decimal",
      "value": "0.4"
    }
  ]
}
```

Expected: `{"type":"contains","text":"normal"}`


<a id="contribution"></a>

## contribution

Gross and net contribution with optional per-unit contribution margin.

gross_contribution = revenue - refunds - variable_costs; net_contribution = gross_contribution - fixed_costs. Fixed costs are kept separate from variable costs. All money amounts must be in the currency named by the currency parameter. When units is supplied, margin_per_unit is the gross contribution per unit, rounded to settlement scale 2.

- Module: `finance` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/finance.md#contribution

## Parameters

- `revenue` — Gross revenue. (money)
- `refunds` (optional) — Refunds or returns. (money)
- `variable_costs` (optional) — Total variable costs. (money)
- `fixed_costs` (optional) — Total fixed costs. (money)
- `currency` — Currency all amounts must share. (text)
- `units` (optional) — Optional positive unit count for margin_per_unit. (exact number (integer, rational, or decimal))

## Output

Gross contribution, net contribution, and optional per-unit margin.

## Examples

### contribution with fixed costs

```json
{
  "currency": "USD",
  "fixed_costs": {
    "kind": "money",
    "amount": {
      "kind": "decimal",
      "value": "200.00"
    },
    "currency": "USD"
  },
  "revenue": {
    "kind": "money",
    "amount": {
      "kind": "decimal",
      "value": "1000.00"
    },
    "currency": "USD"
  },
  "units": {
    "kind": "integer",
    "value": "100"
  },
  "variable_costs": {
    "kind": "money",
    "amount": {
      "kind": "decimal",
      "value": "400.00"
    },
    "currency": "USD"
  }
}
```

Expected: `{"type":"value","value":{"gross_contribution":{"kind":"money","amount":{"kind":"decimal","value":"600.00"},"currency":"USD"},"margin_per_unit":{"kind":"money","amount":{"kind":"decimal","value":"6.00"},"currency":"USD"},"net_contribution":{"kind":"money","amount":{"kind":"decimal","value":"400.00"},"currency":"USD"}}}`


<a id="convert_money"></a>

## convert_money

Convert a money amount with a caller-supplied target-per-source rate.

The rate is the number of target-currency units per one source-currency unit (target per source). The converted amount is rounded to the settlement scale of 2 decimal places (half-even). No FX rate is looked up or inferred: the rate is caller-supplied, its direction and any as-of date are recorded in warnings, and the result is classified rounded.

- Module: `finance` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/finance.md#convert_money

## Parameters

- `money` — Amount to convert. (money)
- `rate` — Target-per-source decimal rate. (exact number (integer, rational, or decimal))
- `target_currency` — Target currency code (2-12 uppercase letters or digits). (text)
- `rate_as_of` (optional) — Optional YYYY-MM-DD date the supplied rate refers to. (text)

## Output

Converted money in target_currency, rounded to settlement scale 2.

## Examples

### USD to EUR at a supplied rate

```json
{
  "money": {
    "kind": "money",
    "amount": {
      "kind": "decimal",
      "value": "100.00"
    },
    "currency": "USD"
  },
  "rate": "0.90",
  "target_currency": "EUR"
}
```

Expected: `{"type":"value","value":{"kind":"money","amount":{"kind":"decimal","value":"90.00"},"currency":"EUR"}}`

### invalid target currency

```json
{
  "money": {
    "kind": "money",
    "amount": {
      "kind": "decimal",
      "value": "1.00"
    },
    "currency": "USD"
  },
  "rate": "1.0",
  "target_currency": "usd"
}
```

Expected: `{"type":"error","code":"malformed_input"}`


<a id="future_value"></a>

## future_value

Compound a present money amount forward.

future_value = present_value * (1 + annual_rate / compounds_per_year) ^ (compounds_per_year * years). The annual_rate is a nominal annual rate whose units must agree with compounds_per_year. The result is rounded to settlement scale 2 (half-even).

- Module: `finance` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/finance.md#future_value

## Parameters

- `present_value` — Present amount. (money)
- `annual_rate` — Nominal annual rate as an exact decimal. (exact number (integer, rational, or decimal))
- `years` — Number of years as an exact decimal. (exact number (integer, rational, or decimal))
- `compounds_per_year` (optional) — Compounding periods per year; default 1. (integer)

## Output

Future value rounded to settlement scale 2.

## Examples

### grow 1000.00 at 5% for 2 years

```json
{
  "annual_rate": "0.05",
  "present_value": {
    "kind": "money",
    "amount": {
      "kind": "decimal",
      "value": "1000.00"
    },
    "currency": "USD"
  },
  "years": {
    "kind": "integer",
    "value": "2"
  }
}
```

Expected: `{"type":"value","value":{"kind":"money","amount":{"kind":"decimal","value":"1102.50"},"currency":"USD"}}`


<a id="irr"></a>

## irr

Periodic internal rate of return of a cash-flow series.

Finds the per-period rate where the NPV of the cash flows is zero, with the first cash flow at t = 0. When lower/upper are supplied they are used as the bracket; otherwise the default domain [-0.9999, 10] is scanned in 11,000 steps for sign changes and each bracket is refined with Brent's method. A bracketed search cannot prove uniqueness: multiple roots or multiple sign changes set multiple_roots_suspected. If no root is found the function returns non_convergence and never a guess. Rates near the domain boundary never produce NaN or infinity: evaluations outside the domain are discarded.

- Module: `finance` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/finance.md#irr

## Parameters

- `cashflows` — Periodic cash flows: all money in one currency, or exact numbers when currency is omitted. (array of any value)
- `guess` (optional) — Optional initial rate hint. (exact number (integer, rational, or decimal))
- `lower` (optional) — Optional lower bracket bound (> -1). (exact number (integer, rational, or decimal))
- `upper` (optional) — Optional upper bracket bound. (exact number (integer, rational, or decimal))
- `tolerance` (optional) — Root tolerance; default 1e-12. (exact number (integer, rational, or decimal))
- `max_iterations` (optional) — Brent iteration cap; default 200. (integer)
- `currency` (optional) — Currency code when cash flows are money. (text)

## Output

Rate, convergence, method, bracket, iterations, residual, and root diagnostics.

## Examples

### no sign change

```json
{
  "cashflows": [
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

Expected: `{"type":"error","code":"non_convergence"}`


<a id="margin"></a>

## margin

Convert a markup to the equivalent margin.

margin = markup / (1 + markup). The markup must be greater than -1 so the resulting margin is below 1; markup <= -1 is a domain error. The result is exact when it terminates as a decimal and rounded otherwise.

- Module: `finance` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/finance.md#margin

## Parameters

- `markup` — Markup as a decimal fraction (0.25 means 25%). (exact number (integer, rational, or decimal))

## Output

Margin markup / (1 + markup).

## Examples

### markup 25%

```json
{
  "markup": "0.25"
}
```

Expected: `{"type":"value","value":{"kind":"decimal","value":"0.2"}}`

### markup at -100%

```json
{
  "markup": "-1"
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="markup"></a>

## markup

Convert a margin to the equivalent markup.

markup = margin / (1 - margin). The margin is a decimal in (-infinity, 1); a margin of 1 or more is a domain error. The result is exact when it terminates as a decimal and rounded otherwise.

- Module: `finance` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/finance.md#markup

## Parameters

- `margin` — Margin as a decimal fraction (0.25 means 25%). (exact number (integer, rational, or decimal))

## Output

Markup margin / (1 - margin).

## Examples

### margin 20%

```json
{
  "margin": "0.2"
}
```

Expected: `{"type":"value","value":{"kind":"decimal","value":"0.25"}}`

### margin at or above 100%

```json
{
  "margin": "1"
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="max_drawdown"></a>

## max_drawdown

Largest peak-to-trough decline of a compounded equity curve.

Builds the equity curve equity[0] = 1 and equity[i] = equity[i-1] * (1 + returns[i-1]), then reports the largest fractional decline (peak - equity[i]) / peak observed at any point. peak_index and trough_index are equity-curve indices, so peak_index 0 is the starting value. Every return must be greater than -1 so the equity curve stays positive. If the curve never declines, the drawdown is 0 and both indices are 0.

- Module: `finance` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/finance.md#max_drawdown
- Units/currency rule: returns are per-period decimals; the drawdown is a unitless fraction

## Parameters

- `returns` — Return series; at least one observation. (array of exact number (integer, rational, or decimal))

## Output

Maximum drawdown, peak index, trough index, and method.

## Examples

### peak then trough

```json
{
  "returns": [
    {
      "kind": "decimal",
      "value": "0.1"
    },
    {
      "kind": "decimal",
      "value": "-0.2"
    },
    {
      "kind": "decimal",
      "value": "0.05"
    },
    {
      "kind": "decimal",
      "value": "-0.3"
    },
    {
      "kind": "decimal",
      "value": "0.4"
    }
  ]
}
```

Expected: `{"type":"value","value":{"max_drawdown":{"kind":"decimal","value":"0.412"},"method":"max_drawdown","peak_index":{"kind":"integer","value":"1"},"trough_index":{"kind":"integer","value":"4"}}}`

### monotonic gains

```json
{
  "returns": [
    {
      "kind": "decimal",
      "value": "0.1"
    },
    {
      "kind": "decimal",
      "value": "0.2"
    }
  ]
}
```

Expected: `{"type":"value","value":{"max_drawdown":{"kind":"decimal","value":"0"},"method":"max_drawdown","peak_index":{"kind":"integer","value":"0"},"trough_index":{"kind":"integer","value":"0"}}}`


<a id="money_add"></a>

## money_add

Add two money amounts in the same currency.

Exact decimal addition. Both amounts must share one currency; different currencies are rejected with a currency mismatch error and are never combined implicitly.

- Module: `finance` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/finance.md#money_add

## Parameters

- `a` — Left amount. (money)
- `b` — Right amount. (money)

## Output

Sum a + b in the shared currency.

## Examples

### add USD amounts

```json
{
  "a": {
    "kind": "money",
    "amount": {
      "kind": "decimal",
      "value": "1.10"
    },
    "currency": "USD"
  },
  "b": {
    "kind": "money",
    "amount": {
      "kind": "decimal",
      "value": "2.20"
    },
    "currency": "USD"
  }
}
```

Expected: `{"type":"value","value":{"kind":"money","amount":{"kind":"decimal","value":"3.30"},"currency":"USD"}}`

### currency mismatch

```json
{
  "a": {
    "kind": "money",
    "amount": {
      "kind": "decimal",
      "value": "1.00"
    },
    "currency": "USD"
  },
  "b": {
    "kind": "money",
    "amount": {
      "kind": "decimal",
      "value": "1.00"
    },
    "currency": "INR"
  }
}
```

Expected: `{"type":"error","code":"currency_mismatch"}`


<a id="money_allocate"></a>

## money_allocate

Split a money amount by non-negative weights with an exact sum.

Splits the amount into shares at the requested settlement scale (default 2). Each exact share is floored to minor units; the remaining minor units are distributed one at a time to recipients ordered by descending fractional remainder, with ties broken by ascending index. This largest-remainder rule is deterministic and makes the shares sum EXACTLY to the original total. Weights must be non-negative exact numbers with a positive sum; the amount must be exactly representable at the requested scale.

- Module: `finance` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/finance.md#money_allocate

## Parameters

- `money` — Amount to allocate. (money)
- `weights` — Non-negative exact weights; at least one must be positive. (array of exact number (integer, rational, or decimal))
- `scale` (optional) — Settlement scale for the shares, 0-18; default 2. (integer)

## Output

Shares, exact total, settlement scale, and currency.

## Examples

### allocate USD 10.00 by equal weights

```json
{
  "money": {
    "kind": "money",
    "amount": {
      "kind": "decimal",
      "value": "10.00"
    },
    "currency": "USD"
  },
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
  ]
}
```

Expected: `{"type":"value","value":{"currency":"USD","scale":{"kind":"integer","value":"2"},"shares":[{"kind":"money","amount":{"kind":"decimal","value":"3.34"},"currency":"USD"},{"kind":"money","amount":{"kind":"decimal","value":"3.33"},"currency":"USD"},{"kind":"money","amount":{"kind":"decimal","value":"3.33"},"currency":"USD"}],"total":{"kind":"money","amount":{"kind":"decimal","value":"10.00"},"currency":"USD"}}}`


<a id="money_compare"></a>

## money_compare

Compare two money amounts in the same currency.

Exact numeric comparison. Both amounts must share one currency; different currencies are rejected with a currency mismatch error.

- Module: `finance` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/finance.md#money_compare

## Parameters

- `a` — Left amount. (money)
- `b` — Right amount. (money)

## Output

Ordering label and integer sign (-1, 0, 1).

## Examples

### compare USD amounts

```json
{
  "a": {
    "kind": "money",
    "amount": {
      "kind": "decimal",
      "value": "1.00"
    },
    "currency": "USD"
  },
  "b": {
    "kind": "money",
    "amount": {
      "kind": "decimal",
      "value": "2.00"
    },
    "currency": "USD"
  }
}
```

Expected: `{"type":"value","value":{"ordering":"less","sign":{"kind":"integer","value":"-1"}}}`


<a id="money_scale"></a>

## money_scale

Multiply a money amount by an exact decimal factor.

Exact decimal multiplication: the amount is scaled by a caller-supplied exact decimal or integer factor. The currency is unchanged.

- Module: `finance` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/finance.md#money_scale

## Parameters

- `money` — Amount to scale. (money)
- `factor` — Exact decimal factor. (exact number (integer, rational, or decimal))

## Output

Scaled amount in the same currency.

## Examples

### scale by an integer factor

```json
{
  "factor": {
    "kind": "integer",
    "value": "3"
  },
  "money": {
    "kind": "money",
    "amount": {
      "kind": "decimal",
      "value": "2.50"
    },
    "currency": "USD"
  }
}
```

Expected: `{"type":"value","value":{"kind":"money","amount":{"kind":"decimal","value":"7.50"},"currency":"USD"}}`


<a id="money_sub"></a>

## money_sub

Subtract one money amount from another in the same currency.

Exact decimal subtraction. Both amounts must share one currency; different currencies are rejected with a currency mismatch error.

- Module: `finance` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/finance.md#money_sub

## Parameters

- `a` — Minuend. (money)
- `b` — Subtrahend. (money)

## Output

Difference a - b in the shared currency.

## Examples

### subtract USD amounts

```json
{
  "a": {
    "kind": "money",
    "amount": {
      "kind": "decimal",
      "value": "5.00"
    },
    "currency": "USD"
  },
  "b": {
    "kind": "money",
    "amount": {
      "kind": "decimal",
      "value": "1.25"
    },
    "currency": "USD"
  }
}
```

Expected: `{"type":"value","value":{"kind":"money","amount":{"kind":"decimal","value":"3.75"},"currency":"USD"}}`


<a id="npv"></a>

## npv

Discounted sum of periodic cash flows at a constant per-period rate.

NPV = sum(cashflow[i] / (1 + rate)^e) where e is i for first_cashflow_at_t0 (default) and i + 1 for first_cashflow_at_t1. The rate is a per-period decimal and must be greater than -1. Cash flows must all be money in one currency when currency is supplied, or plain exact numbers when currency is omitted. The result is exact when the discounted sum terminates as a decimal and rounded otherwise.

- Module: `finance` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/finance.md#npv

## Parameters

- `rate` — Per-period decimal discount rate; must be greater than -1. (exact number (integer, rational, or decimal))
- `cashflows` — Periodic cash flows: all money in one currency, or exact numbers when currency is omitted. (array of any value)
- `currency` (optional) — Currency code when cash flows are money. (text)
- `timing` (optional) — first_cashflow_at_t0 (default) or first_cashflow_at_t1. (one of ["first_cashflow_at_t0", "first_cashflow_at_t1"])

## Output

NPV, rate, number of periods, timing, and optional currency.

## Examples

### NPV at a zero rate

```json
{
  "cashflows": [
    {
      "kind": "integer",
      "value": "-1000"
    },
    {
      "kind": "integer",
      "value": "500"
    },
    {
      "kind": "integer",
      "value": "600"
    }
  ],
  "rate": "0"
}
```

Expected: `{"type":"value","value":{"npv":{"kind":"decimal","value":"100"},"periods":{"kind":"integer","value":"3"},"rate":{"kind":"decimal","value":"0"},"timing":"first_cashflow_at_t0"}}`


<a id="payment"></a>

## payment

Level payment for a present value over a number of periods.

Computes the level payment that amortizes present_value over periods at the periodic rate annual_rate: payment = PV * r / (1 - (1 + r)^-n). annual_rate is the rate per period (a 6% nominal annual rate compounded monthly is passed as 0.005 with 12 periods). timing selects an ordinary annuity (payment at period end, default) or an annuity due (payment at period start, payment divided by (1 + r)). At a zero rate the payment is present_value / periods, rounded to settlement scale 2 when it does not divide evenly.

- Module: `finance` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/finance.md#payment

## Parameters

- `present_value` — Present value. (money)
- `annual_rate` — Periodic rate as an exact decimal. (exact number (integer, rational, or decimal))
- `periods` — Number of payment periods; at least 1. (integer)
- `timing` (optional) — ordinary (default) or annuity_due. (one of ["ordinary", "annuity_due"])

## Output

Level payment rounded to settlement scale 2.

## Examples

### monthly payment on 1200 at 6%/12 for 12 months

```json
{
  "annual_rate": "0.005",
  "periods": {
    "kind": "integer",
    "value": "12"
  },
  "present_value": {
    "kind": "money",
    "amount": {
      "kind": "decimal",
      "value": "1200.00"
    },
    "currency": "USD"
  }
}
```

Expected: `{"type":"value","value":{"kind":"money","amount":{"kind":"decimal","value":"103.28"},"currency":"USD"}}`

### zero-rate payment divides evenly

```json
{
  "annual_rate": "0",
  "periods": {
    "kind": "integer",
    "value": "12"
  },
  "present_value": {
    "kind": "money",
    "amount": {
      "kind": "decimal",
      "value": "1200.00"
    },
    "currency": "USD"
  }
}
```

Expected: `{"type":"value","value":{"kind":"money","amount":{"kind":"decimal","value":"100.00"},"currency":"USD"}}`


<a id="percentage_change"></a>

## percentage_change

Relative change (new - old) / old as an exact decimal when possible.

Returns the fractional change, not the percentage: 0.25 means +25%. The result is exact when (new - old) / old terminates as a decimal and rounded otherwise. old_value must be non-zero.

- Module: `finance` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/finance.md#percentage_change

## Parameters

- `old_value` — Original value. (exact number (integer, rational, or decimal))
- `new_value` — New value. (exact number (integer, rational, or decimal))

## Output

Fractional change (new - old) / old.

## Examples

### increase from 100 to 125

```json
{
  "new_value": {
    "kind": "integer",
    "value": "125"
  },
  "old_value": {
    "kind": "integer",
    "value": "100"
  }
}
```

Expected: `{"type":"value","value":{"kind":"decimal","value":"0.25"}}`

### zero baseline

```json
{
  "new_value": {
    "kind": "integer",
    "value": "10"
  },
  "old_value": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="portfolio_return"></a>

## portfolio_return

Weighted arithmetic return of a portfolio.

Computes sum(weights[i] * returns[i]). Weights are not required to sum to 1; they are used exactly as supplied. The result is exact when the weighted sum terminates as a decimal and rounded otherwise.

- Module: `finance` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/finance.md#portfolio_return
- Units/currency rule: weights and returns are unitless decimals of the same length

## Parameters

- `weights` — Portfolio weights, one per asset. (array of exact number (integer, rational, or decimal))
- `returns` — Asset returns, one per asset. (array of exact number (integer, rational, or decimal))

## Output

Weighted arithmetic return.

## Examples

### equal weights

```json
{
  "returns": [
    {
      "kind": "decimal",
      "value": "0.1"
    },
    {
      "kind": "decimal",
      "value": "0.2"
    }
  ],
  "weights": [
    {
      "kind": "decimal",
      "value": "0.5"
    },
    {
      "kind": "decimal",
      "value": "0.5"
    }
  ]
}
```

Expected: `{"type":"value","value":{"kind":"decimal","value":"0.15"}}`

### length mismatch

```json
{
  "returns": [
    {
      "kind": "decimal",
      "value": "0.1"
    }
  ],
  "weights": [
    {
      "kind": "decimal",
      "value": "0.5"
    },
    {
      "kind": "decimal",
      "value": "0.5"
    }
  ]
}
```

Expected: `{"type":"error","code":"malformed_input"}`


<a id="portfolio_volatility"></a>

## portfolio_volatility

Standard deviation of portfolio return from weights and a covariance matrix.

Computes sqrt(w' Sigma w) where w is the weight vector and Sigma the covariance matrix. The matrix must be square with one row per weight and at least one asset; it must be positive semidefinite (a negative portfolio variance is a domain error). The square root is exact when the variance is a perfect decimal square and rounded otherwise.

- Module: `finance` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Quadratic
- Determinism: Deterministic
- Method reference: docs/methods/finance.md#portfolio_volatility
- Units/currency rule: weights are unitless; the covariance matrix is in squared return units

## Parameters

- `weights` — Portfolio weights, one per asset. (array of exact number (integer, rational, or decimal))
- `covariance_matrix` — Square covariance matrix in the same order as the weights. (matrix)

## Output

Portfolio volatility sqrt(w' Sigma w).

## Examples

### single asset

```json
{
  "covariance_matrix": {
    "kind": "matrix",
    "rows": 1,
    "cols": 1,
    "data": [
      {
        "kind": "decimal",
        "value": "0.04"
      }
    ]
  },
  "weights": [
    {
      "kind": "integer",
      "value": "1"
    }
  ]
}
```

Expected: `{"type":"value","value":{"kind":"decimal","value":"0.2"}}`

### weight and matrix size mismatch

```json
{
  "covariance_matrix": {
    "kind": "matrix",
    "rows": 2,
    "cols": 2,
    "data": [
      {
        "kind": "decimal",
        "value": "0.04"
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
        "kind": "decimal",
        "value": "0.04"
      }
    ]
  },
  "weights": [
    {
      "kind": "integer",
      "value": "1"
    }
  ]
}
```

Expected: `{"type":"error","code":"malformed_input"}`


<a id="present_value"></a>

## present_value

Discount a future money amount to today.

present_value = future_value / (1 + annual_rate / compounds_per_year) ^ (compounds_per_year * years). The annual_rate is a nominal annual rate whose units must agree with compounds_per_year. The result is rounded to settlement scale 2 (half-even).

- Module: `finance` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/finance.md#present_value

## Parameters

- `future_value` — Future amount. (money)
- `annual_rate` — Nominal annual rate as an exact decimal. (exact number (integer, rational, or decimal))
- `years` — Number of years as an exact decimal. (exact number (integer, rational, or decimal))
- `compounds_per_year` (optional) — Compounding periods per year; default 1. (integer)

## Output

Present value rounded to settlement scale 2.

## Examples

### discount 1102.50 at 5% for 2 years

```json
{
  "annual_rate": "0.05",
  "future_value": {
    "kind": "money",
    "amount": {
      "kind": "decimal",
      "value": "1102.50"
    },
    "currency": "USD"
  },
  "years": {
    "kind": "integer",
    "value": "2"
  }
}
```

Expected: `{"type":"value","value":{"kind":"money","amount":{"kind":"decimal","value":"1000.00"},"currency":"USD"}}`


<a id="sharpe_ratio"></a>

## sharpe_ratio

Excess mean return per unit of sample standard deviation.

(mean(returns) - risk_free_rate) / sample_standard_deviation(returns), where the standard deviation uses the n-1 denominator. risk_free_rate defaults to 0. At least two observations are required and the sample standard deviation must be positive. The result is exact when the square root and ratio terminate as decimals and rounded otherwise.

- Module: `finance` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/finance.md#sharpe_ratio
- Units/currency rule: returns and risk_free_rate are per-period decimals

## Parameters

- `returns` — Return series; at least two observations. (array of exact number (integer, rational, or decimal))
- `risk_free_rate` (optional) — Per-period risk-free rate; default 0. (exact number (integer, rational, or decimal))

## Output

Sharpe ratio.

## Examples

### zero risk-free rate

```json
{
  "returns": [
    {
      "kind": "decimal",
      "value": "0.1"
    },
    {
      "kind": "decimal",
      "value": "0.2"
    },
    {
      "kind": "decimal",
      "value": "0.3"
    }
  ]
}
```

Expected: `{"type":"value","value":{"kind":"decimal","value":"2"}}`

### constant returns have no risk

```json
{
  "returns": [
    {
      "kind": "decimal",
      "value": "0.1"
    },
    {
      "kind": "decimal",
      "value": "0.1"
    }
  ]
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="simple_interest"></a>

## simple_interest

Simple interest on a principal over a number of years.

interest = principal * annual_rate * years. The interest is rounded to the settlement scale of 2 decimal places (half-even) and the total is the quantized principal plus that interest. No compounding is applied.

- Module: `finance` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/finance.md#simple_interest

## Parameters

- `principal` — Principal amount. (money)
- `annual_rate` — Annual simple rate as an exact decimal (0.05 means 5%). (exact number (integer, rational, or decimal))
- `years` — Number of years as an exact decimal. (exact number (integer, rational, or decimal))

## Output

Interest and total (principal + interest).

## Examples

### 5% simple interest for 2 years

```json
{
  "annual_rate": "0.05",
  "principal": {
    "kind": "money",
    "amount": {
      "kind": "decimal",
      "value": "1000.00"
    },
    "currency": "USD"
  },
  "years": {
    "kind": "integer",
    "value": "2"
  }
}
```

Expected: `{"type":"value","value":{"interest":{"kind":"money","amount":{"kind":"decimal","value":"100.00"},"currency":"USD"},"total":{"kind":"money","amount":{"kind":"decimal","value":"1100.00"},"currency":"USD"}}}`


<a id="value_at_risk"></a>

## value_at_risk

Historical or normal value at risk of a return series.

Returns the loss threshold exceeded with probability 1 - confidence. The historical method uses the floor((1 - confidence) * n)-th smallest return, clamped to the series, and reports its negative. The normal method assumes a normal return distribution and returns sigma * Phi^-1(confidence) - mean, where sigma is the sample standard deviation (n - 1 denominator, at least two observations required). The normal method is rounded; the historical method is exact when the negation terminates as a decimal.

- Module: `finance` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/finance.md#value_at_risk
- Units/currency rule: returns and value at risk are per-period decimals

## Parameters

- `returns` — Return series; at least one observation (two for the normal method). (array of exact number (integer, rational, or decimal))
- `confidence` — Confidence level strictly between 0 and 1. (exact number (integer, rational, or decimal))
- `method` (optional) — historical (default) or normal. (one of ["historical", "normal"])

## Output

Value at risk, method, and the confidence level used.

## Examples

### historical at eighty percent

```json
{
  "confidence": "0.8",
  "returns": [
    {
      "kind": "decimal",
      "value": "-0.3"
    },
    {
      "kind": "decimal",
      "value": "-0.2"
    },
    {
      "kind": "decimal",
      "value": "-0.1"
    },
    {
      "kind": "integer",
      "value": "0"
    },
    {
      "kind": "decimal",
      "value": "0.4"
    }
  ]
}
```

Expected: `{"type":"value","value":{"confidence":{"kind":"decimal","value":"0.8"},"method":"historical","var":{"kind":"decimal","value":"0.2"}}}`

### normal at the median

```json
{
  "confidence": "0.5",
  "method": "normal",
  "returns": [
    {
      "kind": "decimal",
      "value": "0.1"
    },
    {
      "kind": "decimal",
      "value": "-0.1"
    }
  ]
}
```

Expected: `{"type":"value","value":{"confidence":{"kind":"decimal","value":"0.5"},"method":"normal","var":{"kind":"decimal","value":"0"}}}`


<a id="xirr"></a>

## xirr

Annualized internal rate of return of dated cash flows.

Dated version of irr: finds the annual rate where the XNPV of the dated cash flows is zero. The earliest date is the reference date; the same day-count conventions as xnpv apply. Bracket handling, multiple-root detection, and non-convergence behaviour match irr.

- Module: `finance` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/finance.md#xirr

## Parameters

- `cashflows` — Dated cash flows: records with date and amount. (array of any value)
- `day_count` (optional) — actual_365 (default), actual_360, actual_actual, or thirty_360. (one of ["actual_365", "actual_360", "actual_actual", "thirty_360"])
- `guess` (optional) — Optional initial rate hint. (exact number (integer, rational, or decimal))
- `lower` (optional) — Optional lower bracket bound (> -1). (exact number (integer, rational, or decimal))
- `upper` (optional) — Optional upper bracket bound. (exact number (integer, rational, or decimal))
- `tolerance` (optional) — Root tolerance; default 1e-12. (exact number (integer, rational, or decimal))
- `max_iterations` (optional) — Brent iteration cap; default 200. (integer)
- `currency` (optional) — Currency code when cash-flow amounts are money. (text)

## Output

Rate, convergence, method, bracket, iterations, residual, day count, and root diagnostics.

## Examples

### no sign change

```json
{
  "cashflows": [
    {
      "amount": {
        "kind": "integer",
        "value": "100"
      },
      "date": "2024-01-01"
    },
    {
      "amount": {
        "kind": "integer",
        "value": "200"
      },
      "date": "2025-01-01"
    }
  ]
}
```

Expected: `{"type":"error","code":"non_convergence"}`


<a id="xnpv"></a>

## xnpv

Discounted sum of dated cash flows using an explicit day-count convention.

Each cash flow is {date: "YYYY-MM-DD", amount: money-or-number}. Dates are parsed as unambiguous calendar dates with no timezone or locale involved. The earliest date is the reference date (year fraction 0) and every amount is discounted as amount / (1 + rate)^year_fraction. Day counts: actual_365 (default), actual_360, actual_actual (split by calendar year, leap years handled explicitly), and thirty_360 (European 30E/360). The rate must be greater than -1. Discounting uses binary64 exponentiation and is reported rounded.

- Module: `finance` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/finance.md#xnpv

## Parameters

- `rate` — Per-period decimal discount rate; must be greater than -1. (exact number (integer, rational, or decimal))
- `cashflows` — Dated cash flows: records with date and amount. (array of any value)
- `day_count` (optional) — actual_365 (default), actual_360, actual_actual, or thirty_360. (one of ["actual_365", "actual_360", "actual_actual", "thirty_360"])
- `currency` (optional) — Currency code when cash-flow amounts are money. (text)

## Output

XNPV, rate, number of periods, day count, and optional currency.

## Examples

### XNPV at a zero rate

```json
{
  "cashflows": [
    {
      "amount": {
        "kind": "integer",
        "value": "100"
      },
      "date": "2024-01-01"
    },
    {
      "amount": {
        "kind": "integer",
        "value": "100"
      },
      "date": "2025-01-01"
    }
  ],
  "rate": "0"
}
```

Expected: `{"type":"value","value":{"day_count":"actual_365","npv":{"kind":"decimal","value":"200"},"periods":{"kind":"integer","value":"2"},"rate":{"kind":"decimal","value":"0"}}}`


