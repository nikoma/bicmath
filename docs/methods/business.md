# Business module reference

Unit economics, customer lifetime value, pricing elasticity, inventory, queueing, and cohort analysis.

- Module id: `business`
- Version: 1.0.0
- Capabilities: unit_economics, customer_lifetime_value, price_elasticity, inventory_management, queueing, cohort_analysis
- Supported modes: exact, auto, scientific
- Functions: 9

This file is generated from the live registry by `bicmath docs`; the function schemas, domains, and examples are the same ones the engine validates against at runtime.

<a id="arc_elasticity"></a>

## arc_elasticity

Midpoint arc price elasticity of demand with an elasticity classification.

elasticity = ((new_quantity - old_quantity) / midpoint_quantity) / ((new_price - old_price) / midpoint_price), where each midpoint is the average of the old and new values. A negative elasticity is expected for ordinary demand. classification is elastic when |elasticity| > 1, inelastic when |elasticity| < 1, and unit when |elasticity| = 1. The price must change and both midpoints must be non-zero.

- Module: `business` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/business.md#arc_elasticity
- Units/currency rule: price and quantity are dimensionless in the elasticity ratio.

## Parameters

- `old_price` — Price before the change. (exact number (integer, rational, or decimal))
- `new_price` — Price after the change. (exact number (integer, rational, or decimal))
- `old_quantity` — Quantity demanded before the change. (exact number (integer, rational, or decimal))
- `new_quantity` — Quantity demanded after the change. (exact number (integer, rational, or decimal))

## Output

Midpoint arc elasticity, its classification, and method.

## Examples

### elastic demand

```json
{
  "new_price": "12",
  "new_quantity": "60",
  "old_price": "10",
  "old_quantity": "100"
}
```

Expected: `{"type":"value","value":{"classification":"elastic","elasticity":{"kind":"decimal","value":"-2.75"},"method":"arc_elasticity"}}`

### unit elasticity

```json
{
  "new_price": "20",
  "new_quantity": "50",
  "old_price": "10",
  "old_quantity": "100"
}
```

Expected: `{"type":"value","value":{"classification":"unit","elasticity":{"kind":"decimal","value":"-1"},"method":"arc_elasticity"}}`


<a id="clv_geometric"></a>

## clv_geometric

Customer lifetime value as a geometric revenue series with optional discounting.

CLV = sum_{t=0}^{periods-1} revenue_per_period * retention_rate^t / (1 + discount_rate)^t. When periods is omitted the infinite series is summed, which requires retention_rate / (1 + discount_rate) < 1. The result is exact when the closed form terminates as a decimal and rounded to the context precision otherwise. The returned periods field is null for the infinite series.

- Module: `business` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/business.md#clv_geometric
- Units/currency rule: revenue_per_period and clv share one currency unit per period.

## Parameters

- `revenue_per_period` — Revenue recognized per period. (exact number (integer, rational, or decimal))
- `retention_rate` — Period-over-period retention rate, in [0, 1]. (exact number (integer, rational, or decimal))
- `discount_rate` — Per-period discount rate; must be greater than -1. (exact number (integer, rational, or decimal))
- `periods` (optional) — Number of periods to sum; when omitted the infinite series is used. (integer)

## Output

Geometric CLV, the number of periods summed (null for infinite), and method.

## Examples

### three periods at 50% retention

```json
{
  "discount_rate": "0",
  "periods": {
    "kind": "integer",
    "value": "3"
  },
  "retention_rate": "0.5",
  "revenue_per_period": "100"
}
```

Expected: `{"type":"value","value":{"clv":{"kind":"decimal","value":"175"},"method":"geometric_clv","periods":{"kind":"integer","value":"3"}}}`

### infinite series at 50% retention

```json
{
  "discount_rate": "0",
  "retention_rate": "0.5",
  "revenue_per_period": "100"
}
```

Expected: `{"type":"value","value":{"clv":{"kind":"decimal","value":"200"},"method":"geometric_clv","periods":null}}`

### non-convergent infinite series

```json
{
  "discount_rate": "0",
  "retention_rate": "1",
  "revenue_per_period": "100"
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="cohort_revenue"></a>

## cohort_revenue

Simple cohort revenue projection from cohort sizes and revenue per user.

Each cohort contributes cohort_size * revenue_per_user, and total_revenue is the sum over cohorts. The projection is a simple per-user revenue model: it does not model retention decay or discounting. Multiplication is exact, so the result is exact whenever the inputs are exact.

- Module: `business` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/business.md#cohort_revenue
- Units/currency rule: cohort_sizes are counts; revenue_per_user, per-cohort revenue, and total_revenue share one currency unit.

## Parameters

- `cohort_sizes` — Cohort sizes in chronological order. (array of exact number (integer, rational, or decimal))
- `revenue_per_user` — Revenue per user over the projection horizon. (exact number (integer, rational, or decimal))

## Output

Revenue for each cohort, total revenue, and method.

## Examples

### three cohorts at 2.50 per user

```json
{
  "cohort_sizes": [
    {
      "kind": "integer",
      "value": "1000"
    },
    {
      "kind": "integer",
      "value": "800"
    },
    {
      "kind": "integer",
      "value": "600"
    }
  ],
  "revenue_per_user": "2.5"
}
```

Expected: `{"type":"value","value":{"method":"cohort_revenue","revenue_per_cohort":[{"kind":"decimal","value":"2500"},{"kind":"decimal","value":"2000"},{"kind":"decimal","value":"1500"}],"total_revenue":{"kind":"decimal","value":"6000"}}}`


<a id="eoq"></a>

## eoq

Economic order quantity, order frequency, and inventory cost split.

eoq = sqrt(2 * annual_demand * order_cost / holding_cost_per_unit); orders_per_year = annual_demand / eoq; cycle_time_periods = eoq / annual_demand; total_ordering_cost = orders_per_year * order_cost; total_holding_cost = (eoq / 2) * holding_cost_per_unit; total_cost = total_ordering_cost + total_holding_cost. The result is an approximation computed in binary64 and is reported approximate. All three inputs must be strictly positive.

- Module: `business` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/business.md#eoq
- Units/currency rule: annual_demand and eoq are in units; cycle_time_periods is a fraction of one year; costs are in one currency unit.

## Parameters

- `annual_demand` — Annual demand in units. (exact number (integer, rational, or decimal))
- `order_cost` — Fixed cost per order. (exact number (integer, rational, or decimal))
- `holding_cost_per_unit` — Annual holding cost per unit. (exact number (integer, rational, or decimal))

## Output

Economic order quantity, orders per year, cycle time, ordering and holding costs, total cost, and method.

## Examples

### known EOQ

```json
{
  "annual_demand": "10000",
  "holding_cost_per_unit": "2",
  "order_cost": "50"
}
```

Expected: `{"type":"value","value":{"cycle_time_periods":{"kind":"float64","value":"0.07071067811865475"},"eoq":{"kind":"float64","value":"707.1067811865476"},"method":"eoq","orders_per_year":{"kind":"float64","value":"14.14213562373095"},"total_cost":{"kind":"float64","value":"1414.213562373095"},"total_holding_cost":{"kind":"float64","value":"707.1067811865476"},"total_ordering_cost":{"kind":"float64","value":"707.1067811865474"}}}`

### zero demand is rejected

```json
{
  "annual_demand": "0",
  "holding_cost_per_unit": "2",
  "order_cost": "50"
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="erlang_c"></a>

## erlang_c

M/M/c Erlang C waiting probability and queue performance measures.

Multi-server Markovian queue with c identical servers. The offered load is a = arrival_rate / service_rate and utilization = a / staff. The Erlang C probability is evaluated with the numerically stable Erlang B recursion B(0) = 1, B(k) = a * B(k-1) / (k + a * B(k-1)) and C = B(c) / (1 - utilization + utilization * B(c)). average_waiting_time = C / (staff * service_rate - arrival_rate) and average_queue_length = arrival_rate * average_waiting_time. Stability requires arrival_rate < staff * service_rate; results are binary64 approximations.

- Module: `business` (version 1.0.0)
- Modes: auto, scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/business.md#erlang_c
- Units/currency rule: rates are per period; average_waiting_time is in periods; average_queue_length is in customers.

## Parameters

- `staff` — Number of identical servers; at least 1. (integer)
- `arrival_rate` — Mean arrivals per period; non-negative. (exact number (integer, rational, or decimal))
- `service_rate` — Mean services per period per server; strictly positive. (exact number (integer, rational, or decimal))

## Output

Erlang C waiting probability, server utilization, mean queue wait, mean queue length, and method.

## Examples

### two servers at 75% utilization

```json
{
  "arrival_rate": "3",
  "service_rate": "2",
  "staff": {
    "kind": "integer",
    "value": "2"
  }
}
```

Expected: `{"type":"value","value":{"average_queue_length":{"kind":"float64","value":"1.9285714285714284"},"average_waiting_time":{"kind":"float64","value":"0.6428571428571428"},"method":"erlang_c","p_wait":{"kind":"float64","value":"0.6428571428571428"},"utilization":{"kind":"float64","value":"0.75"}}}`

### unstable staffing

```json
{
  "arrival_rate": "4",
  "service_rate": "2",
  "staff": {
    "kind": "integer",
    "value": "2"
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="ltv_cac"></a>

## ltv_cac

Customer lifetime value, LTV/CAC ratio, and CAC payback from ARPU and churn.

The undiscounted CLV is ARPU * gross_margin_rate / monthly_churn_rate. When discount_rate is supplied the CLV is the geometric discounted series sum_{t>=0} ARPU * gross_margin_rate * (1 - churn)^t / (1 + discount_rate)^t, which converges to ARPU * gross_margin_rate * (1 + discount_rate) / (churn + discount_rate). ltv_cac_ratio = clv / cac and payback_months = cac / (arpu * gross_margin_rate). monthly_churn_rate must be strictly positive; cac, arpu, and gross_margin_rate must be positive.

- Module: `business` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/business.md#ltv_cac
- Units/currency rule: arpu, cac, and clv share one currency unit per month; payback_months is in months.

## Parameters

- `arpu` — Average revenue per user per month. (exact number (integer, rational, or decimal))
- `gross_margin_rate` — Gross margin as a fraction of ARPU, in (0, 1]. (exact number (integer, rational, or decimal))
- `monthly_churn_rate` — Monthly churn as a fraction, in (0, 1]. (exact number (integer, rational, or decimal))
- `cac` — Customer acquisition cost. (exact number (integer, rational, or decimal))
- `discount_rate` (optional) — Monthly discount rate; when present the CLV series is discounted. (exact number (integer, rational, or decimal))

## Output

Customer lifetime value, LTV/CAC ratio, CAC payback in months, and method.

## Examples

### undiscounted LTV/CAC

```json
{
  "arpu": "100",
  "cac": "500",
  "gross_margin_rate": "0.8",
  "monthly_churn_rate": "0.05"
}
```

Expected: `{"type":"value","value":{"clv":{"kind":"decimal","value":"1600"},"ltv_cac_ratio":{"kind":"decimal","value":"3.2"},"method":"ltv_cac_undiscounted","payback_months":{"kind":"decimal","value":"6.25"}}}`

### discounted LTV/CAC

```json
{
  "arpu": "100",
  "cac": "500",
  "discount_rate": "0.2",
  "gross_margin_rate": "0.8",
  "monthly_churn_rate": "0.05"
}
```

Expected: `{"type":"value","value":{"clv":{"kind":"decimal","value":"384"},"ltv_cac_ratio":{"kind":"decimal","value":"0.768"},"method":"ltv_cac_discounted","payback_months":{"kind":"decimal","value":"6.25"}}}`

### zero churn is rejected

```json
{
  "arpu": "100",
  "cac": "500",
  "gross_margin_rate": "0.8",
  "monthly_churn_rate": "0"
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="queue_mm1"></a>

## queue_mm1

M/M/1 steady-state performance measures.

Single-server Markovian queue with Poisson arrivals and exponential service. utilization = arrival_rate / service_rate; p0 = 1 - utilization; l = arrival_rate / (service_rate - arrival_rate); lq = utilization * l; w = 1 / (service_rate - arrival_rate); wq = utilization / (service_rate - arrival_rate); p_wait = utilization. Stability requires arrival_rate < service_rate; otherwise the queue grows without bound and a domain error is returned. Results are binary64 approximations.

- Module: `business` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/business.md#queue_mm1
- Units/currency rule: rates are per period; w and wq are in periods; l and lq are in customers.

## Parameters

- `arrival_rate` — Mean arrivals per period; non-negative. (exact number (integer, rational, or decimal))
- `service_rate` — Mean services per period; strictly positive. (exact number (integer, rational, or decimal))

## Output

Utilization, empty-system probability, mean number in system and queue, mean wait in system and queue, probability of waiting, and method.

## Examples

### stable M/M/1 queue

```json
{
  "arrival_rate": "4",
  "service_rate": "5"
}
```

Expected: `{"type":"value","value":{"l":{"kind":"float64","value":"4"},"lq":{"kind":"float64","value":"3.2"},"method":"mm1","p0":{"kind":"float64","value":"0.2"},"p_wait":{"kind":"float64","value":"0.8"},"utilization":{"kind":"float64","value":"0.8"},"w":{"kind":"float64","value":"1"},"wq":{"kind":"float64","value":"0.8"}}}`

### unstable queue

```json
{
  "arrival_rate": "5",
  "service_rate": "5"
}
```

Expected: `{"type":"error","code":"domain_violation"}`


<a id="retention_rates"></a>

## retention_rates

Period-over-period retention rates between consecutive cohort sizes.

For a sequence of cohort sizes s0, s1, ..., the returned rates are s1/s0, s2/s1, ... in order. At least two cohort sizes are required, and every size must be strictly positive. Rates are exact when they terminate as decimals and rounded to the context precision otherwise.

- Module: `business` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/business.md#retention_rates
- Units/currency rule: cohort sizes and rates are counts and dimensionless fractions.

## Parameters

- `cohort_sizes` — Cohort sizes in chronological order; at least two positive values. (array of exact number (integer, rational, or decimal))

## Output

Retention rate between each consecutive pair of cohorts, and method.

## Examples

### three shrinking cohorts

```json
{
  "cohort_sizes": [
    {
      "kind": "integer",
      "value": "1000"
    },
    {
      "kind": "integer",
      "value": "800"
    },
    {
      "kind": "integer",
      "value": "600"
    }
  ]
}
```

Expected: `{"type":"value","value":{"method":"retention_rates","rates":[{"kind":"decimal","value":"0.8"},{"kind":"decimal","value":"0.75"}]}}`

### a single cohort has no rates

```json
{
  "cohort_sizes": [
    {
      "kind": "integer",
      "value": "1000"
    }
  ]
}
```

Expected: `{"type":"error","code":"insufficient_observations"}`


<a id="unit_economics"></a>

## unit_economics

Unit contribution, contribution margin, gross profit, break-even, and margin of safety.

unit_contribution = price - unit_variable_cost; contribution_margin_ratio = unit_contribution / price; gross_profit = unit_contribution * volume - fixed_costs; break_even_units = fixed_costs / unit_contribution; break_even_revenue = break_even_units * price; margin_of_safety_units = volume - break_even_units. price must be strictly greater than unit_variable_cost, otherwise the unit contribution is not positive and the break-even point does not exist. Values are computed exactly and rounded to the context precision only when a quotient does not terminate as a decimal.

- Module: `business` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/business.md#unit_economics
- Units/currency rule: price, unit_variable_cost, and fixed_costs share one currency unit; volume and the break-even and margin-of-safety outputs are in units.

## Parameters

- `price` — Selling price per unit. (exact number (integer, rational, or decimal))
- `unit_variable_cost` — Variable cost per unit. (exact number (integer, rational, or decimal))
- `fixed_costs` — Total fixed costs over the period. (exact number (integer, rational, or decimal))
- `volume` — Units sold over the period. (exact number (integer, rational, or decimal))

## Output

Unit contribution, contribution margin ratio, gross profit, break-even units and revenue, margin of safety in units, and method.

## Examples

### break even at 250 units

```json
{
  "fixed_costs": "1000",
  "price": "10",
  "unit_variable_cost": "6",
  "volume": "300"
}
```

Expected: `{"type":"value","value":{"break_even_revenue":{"kind":"decimal","value":"2500"},"break_even_units":{"kind":"decimal","value":"250"},"contribution_margin_ratio":{"kind":"decimal","value":"0.4"},"gross_profit":{"kind":"decimal","value":"200"},"margin_of_safety_units":{"kind":"decimal","value":"50"},"method":"unit_economics","unit_contribution":{"kind":"decimal","value":"4"}}}`

### price below variable cost

```json
{
  "fixed_costs": "1000",
  "price": "6",
  "unit_variable_cost": "10",
  "volume": "300"
}
```

Expected: `{"type":"error","code":"domain_violation"}`


