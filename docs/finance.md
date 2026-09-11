# Finance conventions

## Money

- Money is a typed value: an exact decimal/integer amount plus an explicit
  currency identifier. `float64` amounts are rejected.
- Amounts in different currencies are never added, subtracted, or compared
  without an explicit conversion (`finance.convert_money`), which takes a
  caller-supplied rate and direction and reports the settlement rounding.
- No exchange-rate fetching, payments, tax filing, or account access exists.
  There is no network access in any numerical module.
- `finance.money_allocate` preserves the original total exactly after
  minor-unit rounding, distributing the rounding remainder deterministically
  (floor shares, then remaining minor units by descending fractional remainder,
  ties by ascending index).

## Rates, periods, and timing

- Rate units and compounding frequency must agree with the number of periods.
  The function descriptions state the expected convention.
- `finance.npv` makes the timing of the initial cash flow explicit:
  `first_cashflow_at_t0` (default) or `first_cashflow_at_t1`. Periodic NPV uses
  a per-period rate.
- `finance.xnpv` uses dated cash flows with an explicit day-count convention:
  `actual_365` (default), `actual_360`, `actual_actual`, or `thirty_360`.
  Dates are parsed as `YYYY-MM-DD` with no timezone or locale ambiguity;
  leap-year treatment follows the named convention and is tested.
- Settlement scale (default 2 minor units) is separate from internal precision.
  Rounding a display never rounds an intermediate computation.

## Interest and amortization

- Simple and compound interest return both interest and total, rounded at the
  settlement scale.
- Present value, future value, and level payment support ordinary annuities
  and annuity-due timing. A zero rate divides the principal evenly.
- `finance.amortization` handles zero rates, invalid terms, payment timing,
  per-period versus final-only rounding, and adjusts the final payment. The
  schedule is returned with a reconciliation record: total principal repayment
  equals the initial principal exactly.

## IRR and XIRR

- `finance.irr` and `finance.xirr` accept an optional search bracket and
  numerical tolerances, and report convergence, iterations, residual, and the
  bracket used.
- IRR/XIRR can have no root or multiple roots. A bracketed search returns a
  root within that bracket and says so; a scan cannot prove there are no roots
  outside its domain. Multiple roots are flagged with
  `multiple_roots_suspected` and `roots_found`.
- If no root is found, the engine returns `non_convergence`; it never returns
  the last iterate as an unqualified success. Rates near domain boundaries do
  not produce NaN or infinity.

## Scenario outputs

Scenario calculations are labelled and returned without unsolicited investment
advice. A mathematically positive estimate is not a claim that financial
success is guaranteed.
