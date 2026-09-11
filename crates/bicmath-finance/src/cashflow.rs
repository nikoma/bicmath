//! Discounted cash flows: NPV, XNPV, IRR, XIRR, day-count conventions, and a
//! bracketed Brent rate solver.

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{Signed, ToPrimitive};

use chrono::{Datelike, NaiveDate};

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, CostClass, Example, FunctionDescriptor, Outcome, ParamDescriptor, Warning,
};
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::number::Decimal;
use bicmath_core::schema::ValueSchema;
use bicmath_core::value::{Value, validate_currency};

use crate::util::{
    SETTLEMENT_SCALE, all_modes, exact_schema, example_args, field, integer_schema,
    money_from_value, number_to_rational, parse_date, parse_value, quantize, rational_to_decimal,
    record_schema, require_currency, text_schema,
};

// ---------------------------------------------------------------------------
// Day-count conventions
// ---------------------------------------------------------------------------

/// Supported day-count conventions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DayCount {
    Actual365,
    Actual360,
    ActualActual,
    Thirty360,
}

impl DayCount {
    pub(crate) fn parse(text: &str) -> Result<DayCount, EngineError> {
        match text {
            "actual_365" => Ok(DayCount::Actual365),
            "actual_360" => Ok(DayCount::Actual360),
            "actual_actual" => Ok(DayCount::ActualActual),
            "thirty_360" => Ok(DayCount::Thirty360),
            other => Err(EngineError::domain(format!(
                "unknown day_count {other:?}; expected actual_365, actual_360, \
                 actual_actual, or thirty_360"
            ))),
        }
    }

    pub(crate) fn from_optional(text: Option<&str>) -> Result<DayCount, EngineError> {
        match text {
            None => Ok(DayCount::Actual365),
            Some(text) => DayCount::parse(text),
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            DayCount::Actual365 => "actual_365",
            DayCount::Actual360 => "actual_360",
            DayCount::ActualActual => "actual_actual",
            DayCount::Thirty360 => "thirty_360",
        }
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_year(year: i32) -> i64 {
    if is_leap_year(year) { 366 } else { 365 }
}

fn actual_actual_fraction(start: NaiveDate, end: NaiveDate) -> BigRational {
    if start == end {
        return BigRational::from_integer(BigInt::from(0));
    }
    if start > end {
        return -actual_actual_fraction(end, start);
    }
    let mut total = BigRational::from_integer(BigInt::from(0));
    let mut cursor = start;
    while cursor.year() < end.year() {
        let next = NaiveDate::from_ymd_opt(cursor.year() + 1, 1, 1)
            .expect("first day of the next year is always valid");
        let days = (next - cursor).num_days();
        total += BigRational::new(
            BigInt::from(days),
            BigInt::from(days_in_year(cursor.year())),
        );
        cursor = next;
    }
    let days = (end - cursor).num_days();
    total + BigRational::new(BigInt::from(days), BigInt::from(days_in_year(end.year())))
}

fn thirty_360_fraction(start: NaiveDate, end: NaiveDate) -> BigRational {
    let start_day = start.day().min(30) as i64;
    let end_day = if end.day() == 31 { 30 } else { end.day() } as i64;
    let days = 360 * (end.year() as i64 - start.year() as i64)
        + 30 * (end.month() as i64 - start.month() as i64)
        + (end_day - start_day);
    BigRational::new(BigInt::from(days), BigInt::from(360))
}

/// Year fraction between two dates under a day-count convention.
///
/// `actual_365` and `actual_360` use the actual number of days over a fixed
/// 365/360 denominator. `actual_actual` splits the interval by calendar year
/// and uses each year's actual length, so leap years are handled explicitly
/// (2024 is 366 days). `thirty_360` is the European 30E/360 convention: day 31
/// becomes day 30 for both endpoints.
pub(crate) fn year_fraction(convention: DayCount, start: NaiveDate, end: NaiveDate) -> BigRational {
    match convention {
        DayCount::Actual365 => {
            BigRational::new(BigInt::from((end - start).num_days()), BigInt::from(365))
        }
        DayCount::Actual360 => {
            BigRational::new(BigInt::from((end - start).num_days()), BigInt::from(360))
        }
        DayCount::ActualActual => actual_actual_fraction(start, end),
        DayCount::Thirty360 => thirty_360_fraction(start, end),
    }
}

// ---------------------------------------------------------------------------
// Cash-flow collection
// ---------------------------------------------------------------------------

fn collect_periodic_cashflows(
    args: &Args,
    name: &str,
    currency: Option<&str>,
) -> Result<Vec<BigRational>, EngineError> {
    let items = args.array(name)?;
    if items.is_empty() {
        return Err(EngineError::new(
            ErrorCode::InsufficientObservations,
            format!("{name} must contain at least one cash flow"),
        ));
    }
    let mut out = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let path = format!("{name}[{index}]");
        out.push(cashflow_amount(item, &path, currency)?);
    }
    Ok(out)
}

/// Accept either a money value (currency checked against the declared currency)
/// or a plain number interpreted as an amount in the declared currency.
fn cashflow_amount(
    item: &Value,
    path: &str,
    currency: Option<&str>,
) -> Result<BigRational, EngineError> {
    match (item, currency) {
        (Value::Money { .. }, Some(expected)) => {
            let (amount, found) = money_from_value(item, path)?;
            require_currency(expected, &found)?;
            Ok(amount.to_rational())
        }
        (Value::Money { .. }, None) => Err(EngineError::malformed(
            "money cash flows require an explicit currency parameter",
        )
        .with_path(path.to_string())),
        (Value::Number(number), _) => number_to_rational(number, path),
        (other, _) => Err(EngineError::malformed(format!(
            "cash flow must be a number or money, found {}",
            other.kind_name()
        ))
        .with_path(path.to_string())),
    }
}

fn collect_dated_cashflows(
    args: &Args,
    name: &str,
    currency: Option<&str>,
) -> Result<Vec<(NaiveDate, BigRational)>, EngineError> {
    let items = args.array(name)?;
    if items.is_empty() {
        return Err(EngineError::new(
            ErrorCode::InsufficientObservations,
            format!("{name} must contain at least one cash flow"),
        ));
    }
    let mut out = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let path = format!("{name}[{index}]");
        let record = item
            .as_record()
            .map_err(|error| error.with_path(path.clone()))?;
        let date_path = format!("{path}.date");
        let date_value = record.get("date").ok_or_else(|| {
            EngineError::malformed(format!("missing field \"date\" at {path}"))
                .with_path(path.clone())
        })?;
        let date_text = date_value
            .as_text()
            .map_err(|error| error.with_path(date_path.clone()))?;
        let date = parse_date(date_text, &date_path)?;
        let amount_path = format!("{path}.amount");
        let amount_value = record.get("amount").ok_or_else(|| {
            EngineError::malformed(format!("missing field \"amount\" at {path}"))
                .with_path(path.clone())
        })?;
        let amount = cashflow_amount(amount_value, &amount_path, currency)?;
        out.push((date, amount));
    }
    Ok(out)
}

fn optional_currency(args: &Args) -> Result<Option<String>, EngineError> {
    match args.optional_text("currency")? {
        None => Ok(None),
        Some(text) => {
            validate_currency(text)?;
            Ok(Some(text.to_string()))
        }
    }
}

fn parse_first_at_t0(args: &Args) -> Result<bool, EngineError> {
    match args.optional_text("timing")? {
        None | Some("first_cashflow_at_t0") => Ok(true),
        Some("first_cashflow_at_t1") => Ok(false),
        Some(other) => Err(EngineError::domain(format!(
            "unknown timing {other:?}; expected first_cashflow_at_t0 or first_cashflow_at_t1"
        ))),
    }
}

// ---------------------------------------------------------------------------
// Rate solving: Brent with a documented default scan domain
// ---------------------------------------------------------------------------

pub(crate) const DEFAULT_LOWER: f64 = -0.9999;
pub(crate) const DEFAULT_UPPER: f64 = 10.0;
const SCAN_STEPS: u32 = 11_000;
const ROOT_DEDUPE_TOLERANCE: f64 = 1e-9;

pub(crate) struct BrentResult {
    pub(crate) root: f64,
    pub(crate) iterations: u32,
    pub(crate) residual: f64,
}

pub(crate) fn brent(
    f: &dyn Fn(f64) -> f64,
    mut a: f64,
    mut b: f64,
    tolerance: f64,
    max_iterations: u32,
) -> Option<BrentResult> {
    let mut fa = f(a);
    let mut fb = f(b);
    if !fa.is_finite() || !fb.is_finite() {
        return None;
    }
    if fa == 0.0 {
        return Some(BrentResult {
            root: a,
            iterations: 0,
            residual: fa,
        });
    }
    if fb == 0.0 {
        return Some(BrentResult {
            root: b,
            iterations: 0,
            residual: fb,
        });
    }
    if (fa < 0.0) == (fb < 0.0) {
        return None;
    }
    let mut c = a;
    let mut fc = fa;
    let mut d = b - a;
    let mut e = d;
    for iteration in 1..=max_iterations {
        if (fb < 0.0) == (fc < 0.0) {
            c = a;
            fc = fa;
            d = b - a;
            e = d;
        }
        if fc.abs() < fb.abs() {
            a = b;
            b = c;
            c = a;
            fa = fb;
            fb = fc;
            fc = fa;
        }
        let tol1 = 2.0 * f64::EPSILON * b.abs() + 0.5 * tolerance;
        let xm = 0.5 * (c - b);
        if xm.abs() <= tol1 || fb == 0.0 {
            return Some(BrentResult {
                root: b,
                iterations: iteration,
                residual: fb,
            });
        }
        if e.abs() >= tol1 && fa.abs() > fb.abs() {
            let s = fb / fa;
            let (p, q);
            if a == c {
                p = 2.0 * xm * s;
                q = 1.0 - s;
            } else {
                let q0 = fa / fc;
                let r = fb / fc;
                p = s * (2.0 * xm * q0 * (q0 - r) - (b - a) * (r - 1.0));
                q = (q0 - 1.0) * (r - 1.0) * (s - 1.0);
            }
            let (p, q) = if p > 0.0 { (p, -q) } else { (-p, q) };
            if q == 0.0 {
                d = xm;
                e = d;
            } else if 2.0 * p < (3.0 * xm * q - (tol1 * q).abs()).min((e * q).abs()) {
                e = d;
                d = p / q;
            } else {
                d = xm;
                e = d;
            }
        } else {
            d = xm;
            e = d;
        }
        a = b;
        fa = fb;
        b += if d.abs() > tol1 {
            d
        } else if xm > 0.0 {
            tol1
        } else {
            -tol1
        };
        fb = f(b);
        if !fb.is_finite() {
            return None;
        }
    }
    None
}

/// Result of a rate search.
pub(crate) struct RootSolve {
    pub(crate) root: f64,
    pub(crate) iterations: u32,
    pub(crate) residual: f64,
    pub(crate) bracket: (f64, f64),
    pub(crate) multiple_roots_suspected: bool,
    pub(crate) roots_found: u32,
}

/// Find a rate root.
///
/// When the caller supplies a bracket (`lower`/`upper`) it is used directly and
/// a bracketed search cannot prove uniqueness, so `multiple_roots_suspected`
/// stays false. Otherwise the documented default domain `[-0.9999, 10.0]` is
/// scanned in 11,000 steps for sign changes and each bracket is refined with
/// Brent's method. Every root found is reported; multiple roots or multiple
/// sign changes set `multiple_roots_suspected`. If no root is found the
/// function returns [`ErrorCode::NonConvergence`] and never a guess.
pub(crate) fn solve_rate(
    f: &dyn Fn(f64) -> f64,
    guess: Option<f64>,
    lower: Option<f64>,
    upper: Option<f64>,
    tolerance: Option<f64>,
    max_iterations: Option<u32>,
) -> Result<RootSolve, EngineError> {
    let tolerance = tolerance.unwrap_or(1e-12);
    if !tolerance.is_finite() || tolerance <= 0.0 {
        return Err(EngineError::domain(
            "tolerance must be a positive finite number",
        ));
    }
    let max_iterations = max_iterations.unwrap_or(200).max(1);
    if let Some(guess) = guess
        && !guess.is_finite()
    {
        return Err(EngineError::domain("guess must be finite"));
    }
    if lower.is_some() || upper.is_some() {
        let a = lower.unwrap_or(DEFAULT_LOWER);
        let b = upper.unwrap_or(DEFAULT_UPPER);
        if !a.is_finite() || !b.is_finite() {
            return Err(EngineError::domain("rate bounds must be finite"));
        }
        if a <= -1.0 {
            return Err(EngineError::domain(
                "lower rate bound must be greater than -1",
            ));
        }
        if b <= a {
            return Err(EngineError::domain(
                "upper rate bound must be greater than the lower bound",
            ));
        }
        let result = brent(f, a, b, tolerance, max_iterations).ok_or_else(|| {
            EngineError::new(
                ErrorCode::NonConvergence,
                format!("no root was found in the supplied bracket [{a}, {b}]"),
            )
        })?;
        return Ok(RootSolve {
            root: result.root,
            iterations: result.iterations,
            residual: result.residual,
            bracket: (a, b),
            multiple_roots_suspected: false,
            roots_found: 1,
        });
    }
    let mut brackets: Vec<(f64, f64)> = Vec::new();
    let mut previous_x = DEFAULT_LOWER;
    let mut previous_y = f(previous_x);
    if previous_y == 0.0 {
        brackets.push((previous_x, previous_x));
    }
    for step in 1..=SCAN_STEPS {
        let x = DEFAULT_LOWER
            + (DEFAULT_UPPER - DEFAULT_LOWER) * f64::from(step) / f64::from(SCAN_STEPS);
        let y = f(x);
        if previous_y.is_finite() && y.is_finite() {
            if y == 0.0 {
                brackets.push((x, x));
            } else if previous_y != 0.0 && (previous_y < 0.0) != (y < 0.0) {
                brackets.push((previous_x, x));
            }
        }
        previous_x = x;
        previous_y = y;
    }
    let mut roots: Vec<(f64, (f64, f64), u32)> = Vec::new();
    for (a, b) in &brackets {
        if a == b {
            roots.push((*a, (*a, *b), 0));
            continue;
        }
        if let Some(result) = brent(f, *a, *b, tolerance, max_iterations) {
            roots.push((result.root, (*a, *b), result.iterations));
        }
    }
    if roots.is_empty() {
        return Err(EngineError::new(
            ErrorCode::NonConvergence,
            format!(
                "no sign change was found while scanning [{DEFAULT_LOWER}, {DEFAULT_UPPER}]; \
                 no root exists in the default domain"
            ),
        ));
    }
    roots.sort_by(|left, right| left.0.total_cmp(&right.0));
    let mut distinct: Vec<(f64, (f64, f64), u32)> = Vec::new();
    for (root, bracket, iterations) in roots {
        let keep = distinct
            .last()
            .map(|(previous, _, _)| {
                (root - previous).abs() > ROOT_DEDUPE_TOLERANCE * (1.0 + previous.abs())
            })
            .unwrap_or(true);
        if keep {
            distinct.push((root, bracket, iterations));
        }
    }
    let chosen = match guess {
        Some(guess) => distinct
            .iter()
            .enumerate()
            .min_by(|(_, (left, _, _)), (_, (right, _, _))| {
                (left - guess).abs().total_cmp(&(right - guess).abs())
            })
            .map(|(index, _)| index)
            .unwrap_or(0),
        None => 0,
    };
    let (root, bracket, iterations) = distinct[chosen];
    let multiple_roots_suspected = brackets.len() > 1 || distinct.len() > 1;
    Ok(RootSolve {
        root,
        iterations,
        residual: f(root),
        bracket,
        multiple_roots_suspected,
        roots_found: distinct.len() as u32,
    })
}

fn root_record(solved: &RootSolve, day_count: Option<&str>) -> Value {
    let decimal = |value: f64| Decimal::from_f64_display(value).unwrap_or_else(Decimal::zero);
    let mut fields = vec![
        ("rate", Value::decimal(decimal(solved.root))),
        ("converged", Value::Bool(true)),
        ("method", Value::text("brent")),
        (
            "bracket",
            Value::Array(vec![
                Value::decimal(decimal(solved.bracket.0)),
                Value::decimal(decimal(solved.bracket.1)),
            ]),
        ),
        (
            "iterations",
            Value::integer(BigInt::from(solved.iterations)),
        ),
        ("residual", Value::decimal(decimal(solved.residual))),
        (
            "multiple_roots_suspected",
            Value::Bool(solved.multiple_roots_suspected),
        ),
        (
            "roots_found",
            Value::integer(BigInt::from(solved.roots_found)),
        ),
    ];
    if let Some(day_count) = day_count {
        fields.push(("day_count", Value::text(day_count)));
    }
    Value::record(fields)
}

// ---------------------------------------------------------------------------
// NPV
// ---------------------------------------------------------------------------

fn npv_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "finance.npv",
        "finance",
        "1.0.0",
        "Net present value",
        "Discounted sum of periodic cash flows at a constant per-period rate.",
    )
    .with_description(
        "NPV = sum(cashflow[i] / (1 + rate)^e) where e is i for \
         first_cashflow_at_t0 (default) and i + 1 for first_cashflow_at_t1. The rate is \
         a per-period decimal and must be greater than -1. Cash flows must all be money \
         in one currency when currency is supplied, or plain exact numbers when currency \
         is omitted. The result is exact when the discounted sum terminates as a decimal \
         and rounded otherwise.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "rate",
            "Per-period decimal discount rate; must be greater than -1.",
            exact_schema(),
        ),
        ParamDescriptor::required(
            "cashflows",
            "Periodic cash flows: all money in one currency, or exact numbers when currency is omitted.",
            ValueSchema::array(ValueSchema::Any),
        ),
        ParamDescriptor::optional(
            "currency",
            "Currency code when cash flows are money.",
            text_schema(),
        ),
        ParamDescriptor::optional(
            "timing",
            "first_cashflow_at_t0 (default) or first_cashflow_at_t1.",
            crate::util::enum_schema(&["first_cashflow_at_t0", "first_cashflow_at_t1"]),
        ),
    ])
    .with_output(
        record_schema(vec![
            field("npv", ValueSchema::Any),
            field("rate", exact_schema()),
            field("periods", integer_schema()),
            field(
                "timing",
                crate::util::enum_schema(&["first_cashflow_at_t0", "first_cashflow_at_t1"]),
            ),
            crate::util::optional_field("currency", text_schema()),
        ]),
        "NPV, rate, number of periods, timing, and optional currency.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_method_ref("docs/methods/finance.md#npv")
    .with_examples(vec![Example::new(
        "NPV at a zero rate",
        example_args(&[
            ("rate", serde_json::json!("0")),
            ("cashflows", serde_json::json!([-1000, 500, 600])),
        ]),
    )
    .with_value(parse_value(serde_json::json!({
        "npv": {"kind": "decimal", "value": "100"},
        "rate": {"kind": "decimal", "value": "0"},
        "periods": {"kind": "integer", "value": "3"},
        "timing": "first_cashflow_at_t0"
    })))])
}

fn invoke_npv(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let rate = args.decimal("rate")?;
    let first_at_t0 = parse_first_at_t0(args)?;
    let currency = optional_currency(args)?;
    let cashflows = collect_periodic_cashflows(args, "cashflows", currency.as_deref())?;
    let base = BigRational::from_integer(BigInt::from(1)) + rate.to_rational();
    if !base.is_positive() {
        return Err(EngineError::domain(
            "rate must be greater than -1 so the discount base stays positive",
        ));
    }
    let mut sum = BigRational::from_integer(BigInt::from(0));
    for (index, cashflow) in cashflows.iter().enumerate() {
        ctx.check()?;
        let exponent = if first_at_t0 { index } else { index + 1 };
        let exponent = i32::try_from(exponent).map_err(|_| {
            EngineError::resource("cash-flow count is too large to discount exactly")
        })?;
        let factor = base.pow(exponent);
        sum += cashflow / factor;
    }
    let (npv_decimal, inexact) = rational_to_decimal(&sum, ctx)?;
    let mut rounded = inexact;
    let npv = match &currency {
        Some(currency) => {
            let (quantized, changed) = quantize(&npv_decimal, SETTLEMENT_SCALE);
            rounded |= changed;
            crate::util::money_value(quantized, currency)
        }
        None => Value::decimal(npv_decimal),
    };
    let mut fields = vec![
        ("npv", npv),
        ("rate", Value::decimal(rate)),
        ("periods", Value::integer(BigInt::from(cashflows.len()))),
        (
            "timing",
            Value::text(if first_at_t0 {
                "first_cashflow_at_t0"
            } else {
                "first_cashflow_at_t1"
            }),
        ),
    ];
    if let Some(currency) = &currency {
        fields.push(("currency", Value::text(currency.clone())));
    }
    Ok(Outcome::new(
        Value::record(fields),
        crate::util::exactness(rounded),
    ))
}

// ---------------------------------------------------------------------------
// XNPV
// ---------------------------------------------------------------------------

fn xnpv_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "finance.xnpv",
        "finance",
        "1.0.0",
        "Net present value of dated cash flows",
        "Discounted sum of dated cash flows using an explicit day-count convention.",
    )
    .with_description(
        "Each cash flow is {date: \"YYYY-MM-DD\", amount: money-or-number}. Dates are \
         parsed as unambiguous calendar dates with no timezone or locale involved. The \
         earliest date is the reference date (year fraction 0) and every amount is \
         discounted as amount / (1 + rate)^year_fraction. Day counts: actual_365 \
         (default), actual_360, actual_actual (split by calendar year, leap years handled \
         explicitly), and thirty_360 (European 30E/360). The rate must be greater than \
         -1. Discounting uses binary64 exponentiation and is reported rounded.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "rate",
            "Per-period decimal discount rate; must be greater than -1.",
            exact_schema(),
        ),
        ParamDescriptor::required(
            "cashflows",
            "Dated cash flows: records with date and amount.",
            ValueSchema::array_with_len(ValueSchema::Any, 1, None),
        ),
        ParamDescriptor::optional(
            "day_count",
            "actual_365 (default), actual_360, actual_actual, or thirty_360.",
            crate::util::enum_schema(&["actual_365", "actual_360", "actual_actual", "thirty_360"]),
        ),
        ParamDescriptor::optional(
            "currency",
            "Currency code when cash-flow amounts are money.",
            text_schema(),
        ),
    ])
    .with_output(
        record_schema(vec![
            field("npv", ValueSchema::Any),
            field("rate", exact_schema()),
            field("periods", integer_schema()),
            field(
                "day_count",
                crate::util::enum_schema(&[
                    "actual_365",
                    "actual_360",
                    "actual_actual",
                    "thirty_360",
                ]),
            ),
            crate::util::optional_field("currency", text_schema()),
        ]),
        "XNPV, rate, number of periods, day count, and optional currency.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_method_ref("docs/methods/finance.md#xnpv")
    .with_examples(vec![
        Example::new(
            "XNPV at a zero rate",
            example_args(&[
                ("rate", serde_json::json!("0")),
                (
                    "cashflows",
                    serde_json::json!([
                        {"date": "2024-01-01", "amount": 100},
                        {"date": "2025-01-01", "amount": 100}
                    ]),
                ),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "npv": {"kind": "decimal", "value": "200"},
            "rate": {"kind": "decimal", "value": "0"},
            "periods": {"kind": "integer", "value": "2"},
            "day_count": "actual_365"
        }))),
    ])
}

fn invoke_xnpv(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let rate = args.decimal("rate")?;
    let day_count = DayCount::from_optional(args.optional_text("day_count")?)?;
    let currency = optional_currency(args)?;
    let cashflows = collect_dated_cashflows(args, "cashflows", currency.as_deref())?;
    let rate_f = rate
        .to_f64()
        .ok_or_else(|| EngineError::domain("rate is not representable as float64"))?;
    if rate_f <= -1.0 {
        return Err(EngineError::domain(
            "rate must be greater than -1 so the discount base stays positive",
        ));
    }
    let reference = cashflows
        .iter()
        .map(|(date, _)| *date)
        .min()
        .ok_or_else(|| EngineError::internal("cash-flow list cannot be empty"))?;
    let mut sum = 0.0f64;
    for (date, amount) in &cashflows {
        ctx.check()?;
        let year_fraction = year_fraction(day_count, reference, *date)
            .to_f64()
            .ok_or_else(|| EngineError::domain("year fraction is not representable as float64"))?;
        let amount = amount.to_f64().ok_or_else(|| {
            EngineError::domain("cash-flow amount is not representable as float64")
        })?;
        sum += amount / (1.0 + rate_f).powf(year_fraction);
    }
    if !sum.is_finite() {
        return Err(EngineError::domain(
            "discounted sum is not finite for these inputs",
        ));
    }
    let decimal = Decimal::from_f64_display(sum)
        .ok_or_else(|| EngineError::domain("discounted sum is not representable as a decimal"))?;
    let npv = match &currency {
        Some(currency) => {
            let (quantized, _) = quantize(&decimal, SETTLEMENT_SCALE);
            crate::util::money_value(quantized, currency)
        }
        None => Value::decimal(decimal),
    };
    let mut fields = vec![
        ("npv", npv),
        ("rate", Value::decimal(rate)),
        ("periods", Value::integer(BigInt::from(cashflows.len()))),
        ("day_count", Value::text(day_count.as_str())),
    ];
    if let Some(currency) = &currency {
        fields.push(("currency", Value::text(currency.clone())));
    }
    Ok(
        Outcome::rounded(Value::record(fields)).with_warning(Warning::new(
            "float64_discounting",
            "XNPV discounting used binary64 exponentiation; the result is rounded",
        )),
    )
}

// ---------------------------------------------------------------------------
// IRR / XIRR
// ---------------------------------------------------------------------------

fn has_sign_change(values: &[f64]) -> bool {
    values.iter().any(|value| *value > 0.0) && values.iter().any(|value| *value < 0.0)
}

fn periodic_npv(rate: f64, values: &[f64], first_at_t0: bool) -> f64 {
    if rate <= -1.0 {
        return f64::NAN;
    }
    let base = 1.0 + rate;
    let mut sum = 0.0f64;
    for (index, value) in values.iter().enumerate() {
        let exponent = if first_at_t0 { index } else { index + 1 };
        sum += value / base.powi(exponent as i32);
    }
    if sum.is_finite() { sum } else { f64::NAN }
}

fn irr_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "finance.irr",
        "finance",
        "1.0.0",
        "Internal rate of return",
        "Periodic internal rate of return of a cash-flow series.",
    )
    .with_description(
        "Finds the per-period rate where the NPV of the cash flows is zero, with the \
         first cash flow at t = 0. When lower/upper are supplied they are used as the \
         bracket; otherwise the default domain [-0.9999, 10] is scanned in 11,000 steps \
         for sign changes and each bracket is refined with Brent's method. A bracketed \
         search cannot prove uniqueness: multiple roots or multiple sign changes set \
         multiple_roots_suspected. If no root is found the function returns \
         non_convergence and never a guess. Rates near the domain boundary never produce \
         NaN or infinity: evaluations outside the domain are discarded.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "cashflows",
            "Periodic cash flows: all money in one currency, or exact numbers when currency is omitted.",
            ValueSchema::array_with_len(ValueSchema::Any, 2, None),
        ),
        ParamDescriptor::optional("guess", "Optional initial rate hint.", exact_schema()),
        ParamDescriptor::optional("lower", "Optional lower bracket bound (> -1).", exact_schema()),
        ParamDescriptor::optional("upper", "Optional upper bracket bound.", exact_schema()),
        ParamDescriptor::optional(
            "tolerance",
            "Root tolerance; default 1e-12.",
            exact_schema(),
        ),
        ParamDescriptor::optional(
            "max_iterations",
            "Brent iteration cap; default 200.",
            integer_schema(),
        ),
        ParamDescriptor::optional(
            "currency",
            "Currency code when cash flows are money.",
            text_schema(),
        ),
    ])
    .with_output(
        root_schema(),
        "Rate, convergence, method, bracket, iterations, residual, and root diagnostics.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Iterative)
    .with_method_ref("docs/methods/finance.md#irr")
    .with_examples(vec![Example::new(
        "no sign change",
        example_args(&[("cashflows", serde_json::json!([1, 2, 3]))]),
    )
    .with_error(ErrorCode::NonConvergence)])
}

fn xirr_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "finance.xirr",
        "finance",
        "1.0.0",
        "Internal rate of return of dated cash flows",
        "Annualized internal rate of return of dated cash flows.",
    )
    .with_description(
        "Dated version of irr: finds the annual rate where the XNPV of the dated cash \
         flows is zero. The earliest date is the reference date; the same day-count \
         conventions as xnpv apply. Bracket handling, multiple-root detection, and \
         non-convergence behaviour match irr.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "cashflows",
            "Dated cash flows: records with date and amount.",
            ValueSchema::array_with_len(ValueSchema::Any, 2, None),
        ),
        ParamDescriptor::optional(
            "day_count",
            "actual_365 (default), actual_360, actual_actual, or thirty_360.",
            crate::util::enum_schema(&["actual_365", "actual_360", "actual_actual", "thirty_360"]),
        ),
        ParamDescriptor::optional("guess", "Optional initial rate hint.", exact_schema()),
        ParamDescriptor::optional("lower", "Optional lower bracket bound (> -1).", exact_schema()),
        ParamDescriptor::optional("upper", "Optional upper bracket bound.", exact_schema()),
        ParamDescriptor::optional(
            "tolerance",
            "Root tolerance; default 1e-12.",
            exact_schema(),
        ),
        ParamDescriptor::optional(
            "max_iterations",
            "Brent iteration cap; default 200.",
            integer_schema(),
        ),
        ParamDescriptor::optional(
            "currency",
            "Currency code when cash-flow amounts are money.",
            text_schema(),
        ),
    ])
    .with_output(
        root_schema_with_day_count(),
        "Rate, convergence, method, bracket, iterations, residual, day count, and root diagnostics.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Iterative)
    .with_method_ref("docs/methods/finance.md#xirr")
    .with_examples(vec![Example::new(
        "no sign change",
        example_args(&[(
            "cashflows",
            serde_json::json!([
                {"date": "2024-01-01", "amount": 100},
                {"date": "2025-01-01", "amount": 200}
            ]),
        )]),
    )
    .with_error(ErrorCode::NonConvergence)])
}

fn root_schema() -> ValueSchema {
    record_schema(vec![
        field("rate", exact_schema()),
        field("converged", ValueSchema::Bool),
        field("method", text_schema()),
        field(
            "bracket",
            ValueSchema::array_with_len(exact_schema(), 2, Some(2)),
        ),
        field("iterations", integer_schema()),
        field("residual", exact_schema()),
        field("multiple_roots_suspected", ValueSchema::Bool),
        field("roots_found", integer_schema()),
    ])
}

fn root_schema_with_day_count() -> ValueSchema {
    record_schema(vec![
        field("rate", exact_schema()),
        field("converged", ValueSchema::Bool),
        field("method", text_schema()),
        field(
            "bracket",
            ValueSchema::array_with_len(exact_schema(), 2, Some(2)),
        ),
        field("iterations", integer_schema()),
        field("residual", exact_schema()),
        field("multiple_roots_suspected", ValueSchema::Bool),
        field("roots_found", integer_schema()),
        field(
            "day_count",
            crate::util::enum_schema(&["actual_365", "actual_360", "actual_actual", "thirty_360"]),
        ),
    ])
}

struct SolverOptions {
    guess: Option<f64>,
    lower: Option<f64>,
    upper: Option<f64>,
    tolerance: Option<f64>,
    max_iterations: Option<u32>,
}

fn solver_options(args: &Args) -> Result<SolverOptions, EngineError> {
    let guess = args.optional_f64("guess")?;
    let lower = args.optional_f64("lower")?;
    let upper = args.optional_f64("upper")?;
    let tolerance = args.optional_f64("tolerance")?;
    let max_iterations = match args.optional_integer("max_iterations")? {
        None => None,
        Some(value) => Some(value.to_u32().ok_or_else(|| {
            EngineError::domain("max_iterations must be a positive 32-bit integer")
        })?),
    };
    Ok(SolverOptions {
        guess,
        lower,
        upper,
        tolerance,
        max_iterations,
    })
}

fn invoke_irr(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let currency = optional_currency(args)?;
    let cashflows = collect_periodic_cashflows(args, "cashflows", currency.as_deref())?;
    let values: Vec<f64> = cashflows
        .iter()
        .map(|cashflow| {
            cashflow
                .to_f64()
                .ok_or_else(|| EngineError::domain("cash flow is not representable as float64"))
        })
        .collect::<Result<_, _>>()?;
    if !has_sign_change(&values) {
        return Err(EngineError::new(
            ErrorCode::NonConvergence,
            "cash flows must contain at least one positive and one negative value for a root to exist",
        ));
    }
    let options = solver_options(args)?;
    let objective = |rate: f64| periodic_npv(rate, &values, true);
    let solved = solve_rate(
        &objective,
        options.guess,
        options.lower,
        options.upper,
        options.tolerance,
        options.max_iterations,
    )?;
    Ok(Outcome::rounded(root_record(&solved, None)))
}

fn invoke_xirr(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let currency = optional_currency(args)?;
    let day_count = DayCount::from_optional(args.optional_text("day_count")?)?;
    let cashflows = collect_dated_cashflows(args, "cashflows", currency.as_deref())?;
    let values: Vec<(NaiveDate, f64)> = cashflows
        .iter()
        .map(|(date, amount)| {
            let amount = amount
                .to_f64()
                .ok_or_else(|| EngineError::domain("cash flow is not representable as float64"))?;
            Ok((*date, amount))
        })
        .collect::<Result<_, EngineError>>()?;
    if !has_sign_change(&values.iter().map(|(_, amount)| *amount).collect::<Vec<_>>()) {
        return Err(EngineError::new(
            ErrorCode::NonConvergence,
            "cash flows must contain at least one positive and one negative value for a root to exist",
        ));
    }
    let reference = values
        .iter()
        .map(|(date, _)| *date)
        .min()
        .ok_or_else(|| EngineError::internal("cash-flow list cannot be empty"))?;
    let objective = |rate: f64| -> f64 {
        if rate <= -1.0 {
            return f64::NAN;
        }
        let base = 1.0 + rate;
        let mut sum = 0.0f64;
        for (date, amount) in &values {
            let year_fraction = year_fraction(day_count, reference, *date).to_f64();
            let Some(year_fraction) = year_fraction else {
                return f64::NAN;
            };
            sum += amount / base.powf(year_fraction);
        }
        if sum.is_finite() { sum } else { f64::NAN }
    };
    let options = solver_options(args)?;
    let solved = solve_rate(
        &objective,
        options.guess,
        options.lower,
        options.upper,
        options.tolerance,
        options.max_iterations,
    )?;
    Ok(Outcome::rounded(root_record(
        &solved,
        Some(day_count.as_str()),
    )))
}

pub(crate) fn register() -> Vec<std::sync::Arc<dyn bicmath_core::contract::Function>> {
    use bicmath_core::contract::SimpleFunction;
    vec![
        SimpleFunction::arc(npv_descriptor(), invoke_npv),
        SimpleFunction::arc(xnpv_descriptor(), invoke_xnpv),
        SimpleFunction::arc(irr_descriptor(), invoke_irr),
        SimpleFunction::arc(xirr_descriptor(), invoke_xirr),
    ]
}
