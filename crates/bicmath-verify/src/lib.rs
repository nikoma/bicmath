//! Verification module: independent checks for agent claims.
//!
//! Every exact check parses restricted arithmetic expressions with
//! [`bicmath_core::expr::parse_expression`] and evaluates them with
//! [`bicmath_core::eval::evaluate_number`] under an automatic numeric context,
//! so integer, rational, and decimal arithmetic stays exact and binary64 is
//! never used silently. Float64 bindings and operands are rejected for exact
//! checks; only `verify.probability` accepts float64 input, and it labels the
//! result approximate.
//!
//! `verify.inequality_grid` is a search, not a proof. It samples a finite,
//! evenly spaced grid and can refute a claim with a counterexample; a confirmed
//! result carries a warning because a finite grid cannot prove an interval
//! statement.
//!
//! The expression function table is closed: `abs`, `min`, `max`, `pow`
//! (integer exponent only), and `sign`. Unknown functions return a structured
//! [`ErrorCode::UnknownFunction`] error. There are no transcendental functions
//! in exact mode.

#![forbid(unsafe_code)]

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::Arc;

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{Signed, Zero};

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, CostClass, Example, Function, FunctionDescriptor, Module, ModuleDescriptor, Outcome,
    ParamDescriptor, SimpleFunction, Warning,
};
use bicmath_core::envelope::Exactness;
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::eval::evaluate_number;
use bicmath_core::expr::{Expr, parse_expression};
use bicmath_core::limits::Limits;
use bicmath_core::number::{Number, NumericContext, NumericMode};
use bicmath_core::schema::{FieldSchema, NumberKind, ValueSchema};
use bicmath_core::value::Value;

// ---------------------------------------------------------------------------
// Schemas and small helpers
// ---------------------------------------------------------------------------

fn all_modes() -> Vec<NumericMode> {
    vec![
        NumericMode::Exact,
        NumericMode::Auto,
        NumericMode::Scientific,
    ]
}

fn number_any() -> ValueSchema {
    ValueSchema::number(NumberKind::Any)
}

fn integer_schema() -> ValueSchema {
    ValueSchema::number(NumberKind::Integer)
}

fn text_schema() -> ValueSchema {
    ValueSchema::text()
}

fn enum_schema(variants: &[&str]) -> ValueSchema {
    ValueSchema::Enum {
        variants: variants.iter().map(|v| (*v).to_string()).collect(),
    }
}

fn expression_schema() -> ValueSchema {
    ValueSchema::Expression {
        variables: Vec::new(),
    }
}

fn bindings_schema() -> ValueSchema {
    ValueSchema::Record {
        fields: Vec::new(),
        allow_extra: true,
    }
}

fn field(name: &str, schema: ValueSchema) -> FieldSchema {
    FieldSchema::required(name, schema)
}

fn optional_field(name: &str, schema: ValueSchema) -> FieldSchema {
    FieldSchema::optional(name, schema)
}

fn record_schema(fields: Vec<FieldSchema>) -> ValueSchema {
    ValueSchema::Record {
        fields,
        allow_extra: false,
    }
}

fn status_value(confirmed: bool) -> Value {
    Value::text(if confirmed { "confirmed" } else { "refuted" })
}

const SCOPE_EXACT_AT_SUPPLIED_BINDINGS: &str = "exact_at_supplied_bindings";
const SCOPE_EXACT_TOTAL_OF_SUPPLIED_VALUES: &str = "exact_total_of_supplied_values";
const SCOPE_EXACT_RESIDUAL_AT_SUPPLIED_VALUES: &str = "exact_residual_at_supplied_values";
const SCOPE_FINITE_GRID_SEARCH: &str = "finite_grid_search";
const SCOPE_RANGE_CHECK_AT_SUPPLIED_VALUE: &str = "range_check_at_supplied_value";

const LIMITATION_EXACT_AT_SUPPLIED_BINDINGS: &str =
    "confirms the identity at the supplied bindings; it does not prove it for all inputs";
const LIMITATION_EXACT_TOTAL_OF_SUPPLIED_VALUES: &str =
    "confirms the arithmetic of the supplied values, not their provenance or completeness";
const LIMITATION_EXACT_RESIDUAL_AT_SUPPLIED_VALUES: &str = "confirms the residual at the supplied solution, not that the solution is unique or well-conditioned";
const LIMITATION_FINITE_GRID_SEARCH: &str = "a finite grid cannot establish that a statement holds everywhere; confirmed means only that no counterexample was found on the sampled points";
const LIMITATION_RANGE_CHECK_AT_SUPPLIED_VALUE: &str =
    "checks the supplied value against the bounds; it does not validate the model that produced it";

fn scope_fields(scope: &str, limitations: &[&str]) -> Vec<(&'static str, Value)> {
    vec![
        ("scope", Value::text(scope)),
        (
            "scope_limitations",
            Value::Array(limitations.iter().map(|text| Value::text(*text)).collect()),
        ),
    ]
}

fn example_args(pairs: &[(&str, serde_json::Value)]) -> BTreeMap<String, Value> {
    pairs
        .iter()
        .map(|(name, raw)| ((*name).to_string(), parse_value(raw.clone())))
        .collect()
}

fn parse_value(raw: serde_json::Value) -> Value {
    serde_json::from_value(raw).expect("example value must be valid")
}

// ---------------------------------------------------------------------------
// Relations
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Relation {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

impl Relation {
    fn parse(text: &str) -> Result<Relation, EngineError> {
        match text {
            "lt" => Ok(Relation::Lt),
            "le" => Ok(Relation::Le),
            "gt" => Ok(Relation::Gt),
            "ge" => Ok(Relation::Ge),
            "eq" => Ok(Relation::Eq),
            "ne" => Ok(Relation::Ne),
            other => Err(EngineError::domain(format!(
                "unknown relation {other:?}; expected one of lt, le, gt, ge, eq, ne"
            ))),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Relation::Lt => "lt",
            Relation::Le => "le",
            Relation::Gt => "gt",
            Relation::Ge => "ge",
            Relation::Eq => "eq",
            Relation::Ne => "ne",
        }
    }

    fn is_order(self) -> bool {
        matches!(
            self,
            Relation::Lt | Relation::Le | Relation::Gt | Relation::Ge
        )
    }
}

fn within_tolerance(
    left: &Number,
    right: &Number,
    tolerance: &Number,
) -> Result<bool, EngineError> {
    let difference = exact_difference(left, right)?.abs();
    Ok(difference.compare(tolerance)? != Ordering::Greater)
}

fn relation_holds(
    relation: Relation,
    left: &Number,
    right: &Number,
    tolerance: Option<&Number>,
) -> Result<bool, EngineError> {
    if relation.is_order() && tolerance.is_some() {
        return Err(EngineError::domain(
            "tolerance is only supported for the eq and ne relations",
        ));
    }
    let ordering = left.compare(right)?;
    Ok(match relation {
        Relation::Lt => ordering == Ordering::Less,
        Relation::Le => ordering != Ordering::Greater,
        Relation::Gt => ordering == Ordering::Greater,
        Relation::Ge => ordering != Ordering::Less,
        Relation::Eq => match tolerance {
            None => ordering == Ordering::Equal,
            Some(tolerance) => within_tolerance(left, right, tolerance)?,
        },
        Relation::Ne => match tolerance {
            None => ordering != Ordering::Equal,
            Some(tolerance) => !within_tolerance(left, right, tolerance)?,
        },
    })
}

// ---------------------------------------------------------------------------
// Exact number plumbing
// ---------------------------------------------------------------------------

fn float_rejected(path: &str) -> EngineError {
    EngineError::new(
        ErrorCode::UnsupportedNumericMode,
        format!(
            "{path} is a float64; exact checks accept only integer, rational, or decimal values"
        ),
    )
    .with_path(path.to_string())
}

fn ensure_exact(number: &Number, path: &str) -> Result<(), EngineError> {
    if number.is_float() {
        Err(float_rejected(path))
    } else {
        Ok(())
    }
}

fn exact_number_from_value(
    value: &Value,
    path: &str,
    limits: &Limits,
) -> Result<Number, EngineError> {
    match value {
        Value::Number(Number::Float64(_)) => Err(float_rejected(path)),
        Value::Number(number) => Ok(number.clone()),
        Value::Text(text) => {
            Number::parse_literal(text, limits).map_err(|error| error.with_path(path.to_string()))
        }
        other => Err(EngineError::malformed(format!(
            "{path} must be an exact number or a numeric shorthand string, found {}",
            other.kind_name()
        ))
        .with_path(path.to_string())),
    }
}

fn exact_vector(value: &Value, path: &str, limits: &Limits) -> Result<Vec<Number>, EngineError> {
    let items = value
        .as_array()
        .map_err(|error| error.with_path(path.to_string()))?;
    let mut out = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        out.push(exact_number_from_value(
            item,
            &format!("{path}[{index}]"),
            limits,
        )?);
    }
    Ok(out)
}

fn bindings_from_value(
    value: &Value,
    path: &str,
    limits: &Limits,
) -> Result<BTreeMap<String, Number>, EngineError> {
    let record = value
        .as_record()
        .map_err(|error| error.with_path(path.to_string()))?;
    let mut out = BTreeMap::new();
    for (name, raw) in record {
        out.insert(
            name.clone(),
            exact_number_from_value(raw, &format!("{path}.{name}"), limits)?,
        );
    }
    Ok(out)
}

fn bindings_arg(args: &Args, limits: &Limits) -> Result<BTreeMap<String, Number>, EngineError> {
    match args.get("bindings") {
        None | Some(Value::Null) => Ok(BTreeMap::new()),
        Some(value) => bindings_from_value(value, "bindings", limits),
    }
}

fn bindings_to_value(bindings: &BTreeMap<String, Number>) -> Value {
    Value::Record(
        bindings
            .iter()
            .map(|(name, number)| (name.clone(), Value::Number(number.clone())))
            .collect(),
    )
}

fn rational_to_number(value: BigRational) -> Number {
    if value.is_integer() {
        Number::Integer(value.to_integer())
    } else {
        Number::Rational(value)
    }
}

fn exact_difference(left: &Number, right: &Number) -> Result<Number, EngineError> {
    let lhs = left
        .as_exact_rational()
        .ok_or_else(|| float_rejected("difference"))?;
    let rhs = right
        .as_exact_rational()
        .ok_or_else(|| float_rejected("difference"))?;
    Ok(rational_to_number(lhs - rhs))
}

fn tolerance_arg(args: &Args) -> Result<Option<Number>, EngineError> {
    match args.optional_number("tolerance")? {
        None => Ok(None),
        Some(number) => {
            ensure_exact(number, "tolerance")?;
            if number.is_negative() {
                return Err(EngineError::domain("tolerance must be non-negative"));
            }
            Ok(Some(number.clone()))
        }
    }
}

// ---------------------------------------------------------------------------
// Restricted expression evaluation
// ---------------------------------------------------------------------------

fn arity(name: &str, args: &[Number], expected: usize) -> Result<(), EngineError> {
    if args.len() == expected {
        Ok(())
    } else {
        Err(EngineError::malformed(format!(
            "function {name} expects {expected} argument(s), found {}",
            args.len()
        )))
    }
}

fn integer_exponent(number: &Number) -> Result<Number, EngineError> {
    match number {
        Number::Integer(_) => Ok(number.clone()),
        Number::Rational(value) if value.is_integer() => Ok(Number::Integer(value.to_integer())),
        Number::Decimal(value) => match value.to_bigint_if_integral() {
            Some(integer) => Ok(Number::Integer(integer)),
            None => Err(EngineError::domain(
                "pow requires an integer exponent in exact verification",
            )),
        },
        _ => Err(EngineError::domain(
            "pow requires an integer exponent in exact verification",
        )),
    }
}

fn sign_of(number: &Number) -> Result<Number, EngineError> {
    let zero = Number::integer(0);
    Ok(match number.compare(&zero)? {
        Ordering::Less => Number::integer(-1),
        Ordering::Equal => Number::integer(0),
        Ordering::Greater => Number::integer(1),
    })
}

fn dispatch_call(
    name: &str,
    args: &[Number],
    numeric: &NumericContext,
    limits: &Limits,
) -> Result<Number, EngineError> {
    match name {
        "abs" => {
            arity(name, args, 1)?;
            Ok(args[0].abs())
        }
        "sign" => {
            arity(name, args, 1)?;
            sign_of(&args[0])
        }
        "min" | "max" => {
            if args.is_empty() {
                return Err(EngineError::malformed(format!(
                    "function {name} expects at least one argument"
                )));
            }
            let mut best = args[0].clone();
            for candidate in &args[1..] {
                let ordering = candidate.compare(&best)?;
                let replace = if name == "min" {
                    ordering == Ordering::Less
                } else {
                    ordering == Ordering::Greater
                };
                if replace {
                    best = candidate.clone();
                }
            }
            Ok(best)
        }
        "pow" => {
            arity(name, args, 2)?;
            let exponent = integer_exponent(&args[1])?;
            Ok(args[0].pow(&exponent, numeric, limits)?.value)
        }
        other => Err(EngineError::new(
            ErrorCode::UnknownFunction,
            format!("unknown function {other:?} in a verification expression"),
        )
        .with_details(serde_json::json!({
            "function": other,
            "supported": ["abs", "min", "max", "pow", "sign"],
        }))),
    }
}

fn evaluate_parsed(
    expr: &Expr,
    bindings: &BTreeMap<String, Number>,
    ctx: &ExecContext,
) -> Result<Number, EngineError> {
    let numeric = NumericContext::default();
    let calls = |name: &str, args: &[Number]| dispatch_call(name, args, &numeric, &ctx.limits);
    evaluate_number(expr, bindings, &numeric, &ctx.limits, &calls)
}

fn evaluate_expression(
    source: &str,
    bindings: &BTreeMap<String, Number>,
    ctx: &ExecContext,
) -> Result<Number, EngineError> {
    ctx.check()?;
    let expr = parse_expression(source, &ctx.limits)?;
    evaluate_parsed(&expr, bindings, ctx)
}

// ---------------------------------------------------------------------------
// verify.equality
// ---------------------------------------------------------------------------

fn equality_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "verify.equality",
        "verify",
        "1.0.0",
        "Verify equality",
        "Check whether two restricted arithmetic expressions are exactly equal.",
    )
    .with_description(
        "Both expressions are parsed and evaluated with exact integer, rational, and decimal arithmetic; binary64 is never used. Without a tolerance the check is exact equality. With a non-negative exact tolerance the claim is confirmed when |left - right| is at most the tolerance. A refutation includes the witness bindings. Scope: the result is exact at the supplied bindings and does not prove the identity for all inputs.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "left",
            "Left-hand restricted arithmetic expression.",
            expression_schema(),
        ),
        ParamDescriptor::required(
            "right",
            "Right-hand restricted arithmetic expression.",
            expression_schema(),
        ),
        ParamDescriptor::optional(
            "bindings",
            "Record mapping free variables to exact numbers (canonical values or numeric shorthand strings). Float64 values are rejected.",
            bindings_schema(),
        ),
        ParamDescriptor::optional(
            "tolerance",
            "Non-negative exact tolerance for the equality comparison.",
            ValueSchema::exact(),
        ),
    ])
    .with_output(
        record_schema(vec![
            field("status", enum_schema(&["confirmed", "refuted"])),
            field("left", number_any()),
            field("right", number_any()),
            field("difference", number_any()),
            field("tolerance_used", ValueSchema::Any),
            field("method", text_schema()),
            optional_field("scope", text_schema()),
            optional_field("scope_limitations", ValueSchema::array(text_schema())),
            optional_field("scope", text_schema()),
            optional_field("scope_limitations", ValueSchema::array(text_schema())),
            optional_field("bindings", bindings_schema()),
        ]),
        "Verification record with status, evaluated operands, exact difference, and method.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Constant)
    .with_units_rule("Operands must be dimensionless exact numbers.")
    .with_method_ref("docs/methods/verify.md#equality")
    .with_tags(["verification", "exact", "equality"])
    .with_examples(vec![
        Example::new(
            "decimal addition is exact",
            example_args(&[
                ("left", serde_json::json!("0.1 + 0.2")),
                ("right", serde_json::json!("0.3")),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "status": "confirmed",
            "left": {"kind": "decimal", "value": "0.3"},
            "right": {"kind": "decimal", "value": "0.3"},
            "difference": {"kind": "integer", "value": "0"},
            "tolerance_used": null,
            "method": "exact_arithmetic",
            "scope": "exact_at_supplied_bindings",
            "scope_limitations": ["confirms the identity at the supplied bindings; it does not prove it for all inputs"]
        }))),
        Example::new(
            "repeating fraction differs from its truncation",
            example_args(&[
                ("left", serde_json::json!("1/3")),
                ("right", serde_json::json!("0.333")),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "status": "refuted",
            "left": {"kind": "rational", "numerator": "1", "denominator": "3"},
            "right": {"kind": "decimal", "value": "0.333"},
            "difference": {"kind": "rational", "numerator": "1", "denominator": "3000"},
            "tolerance_used": null,
            "method": "exact_arithmetic",
            "scope": "exact_at_supplied_bindings",
            "scope_limitations": ["confirms the identity at the supplied bindings; it does not prove it for all inputs"],
            "bindings": {}
        }))),
        Example::new(
            "tolerance confirms a rounded claim",
            example_args(&[
                ("left", serde_json::json!("1/3")),
                ("right", serde_json::json!("0.333")),
                ("tolerance", serde_json::json!("0.001")),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "status": "confirmed",
            "left": {"kind": "rational", "numerator": "1", "denominator": "3"},
            "right": {"kind": "decimal", "value": "0.333"},
            "difference": {"kind": "rational", "numerator": "1", "denominator": "3000"},
            "tolerance_used": {"kind": "decimal", "value": "0.001"},
            "method": "exact_arithmetic",
            "scope": "exact_at_supplied_bindings",
            "scope_limitations": ["confirms the identity at the supplied bindings; it does not prove it for all inputs"]
        }))),
        Example::new(
            "unknown function is rejected",
            example_args(&[
                ("left", serde_json::json!("nope(1)")),
                ("right", serde_json::json!("1")),
            ]),
        )
        .with_error(ErrorCode::UnknownFunction),
    ])
}

fn invoke_equality(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let bindings = bindings_arg(args, &ctx.limits)?;
    let left = evaluate_expression(args.text("left")?, &bindings, ctx)?;
    let right = evaluate_expression(args.text("right")?, &bindings, ctx)?;
    let difference = exact_difference(&left, &right)?;
    let tolerance = tolerance_arg(args)?;
    let confirmed = match &tolerance {
        None => left.compare(&right)? == Ordering::Equal,
        Some(tolerance) => difference.clone().abs().compare(tolerance)? != Ordering::Greater,
    };
    let mut fields = vec![
        ("status", status_value(confirmed)),
        ("left", Value::Number(left)),
        ("right", Value::Number(right)),
        ("difference", Value::Number(difference)),
        (
            "tolerance_used",
            tolerance.map_or(Value::Null, Value::Number),
        ),
        ("method", Value::text("exact_arithmetic")),
    ];
    if !confirmed {
        fields.push(("bindings", bindings_to_value(&bindings)));
    }
    fields.extend(scope_fields(
        SCOPE_EXACT_AT_SUPPLIED_BINDINGS,
        &[LIMITATION_EXACT_AT_SUPPLIED_BINDINGS],
    ));
    Ok(Outcome::exact(Value::record(fields)))
}

// ---------------------------------------------------------------------------
// verify.inequality
// ---------------------------------------------------------------------------

fn inequality_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "verify.inequality",
        "verify",
        "1.0.0",
        "Verify inequality",
        "Check a relation between two restricted arithmetic expressions exactly.",
    )
    .with_description(
        "Both expressions are evaluated with exact integer, rational, and decimal arithmetic. The relation is one of lt, le, gt, ge, eq, ne. A tolerance is accepted only for eq and ne, where it is interpreted as |left - right| <= tolerance (eq) or its negation (ne). A refutation includes the witness bindings. Scope: the result is exact at the supplied bindings and does not prove the relation for all inputs.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "left",
            "Left-hand restricted arithmetic expression.",
            expression_schema(),
        ),
        ParamDescriptor::required(
            "right",
            "Right-hand restricted arithmetic expression.",
            expression_schema(),
        ),
        ParamDescriptor::required(
            "relation",
            "Relation to check: lt, le, gt, ge, eq, or ne.",
            enum_schema(&["lt", "le", "gt", "ge", "eq", "ne"]),
        ),
        ParamDescriptor::optional(
            "bindings",
            "Record mapping free variables to exact numbers (canonical values or numeric shorthand strings). Float64 values are rejected.",
            bindings_schema(),
        ),
        ParamDescriptor::optional(
            "tolerance",
            "Non-negative exact tolerance; only supported for eq and ne.",
            ValueSchema::exact(),
        ),
    ])
    .with_output(
        record_schema(vec![
            field("status", enum_schema(&["confirmed", "refuted"])),
            field("left", number_any()),
            field("right", number_any()),
            field("relation", enum_schema(&["lt", "le", "gt", "ge", "eq", "ne"])),
            field("difference", number_any()),
            field("method", text_schema()),
            optional_field("scope", text_schema()),
            optional_field("scope_limitations", ValueSchema::array(text_schema())),
            optional_field("scope", text_schema()),
            optional_field("scope_limitations", ValueSchema::array(text_schema())),
            optional_field("bindings", bindings_schema()),
        ]),
        "Verification record with status, evaluated operands, relation, and exact difference.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Constant)
    .with_units_rule("Operands must be dimensionless exact numbers.")
    .with_method_ref("docs/methods/verify.md#inequality")
    .with_tags(["verification", "exact", "inequality"])
    .with_examples(vec![
        Example::new(
            "one half is less than two thirds",
            example_args(&[
                ("left", serde_json::json!("1/2")),
                ("right", serde_json::json!("2/3")),
                ("relation", serde_json::json!("lt")),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "status": "confirmed",
            "left": {"kind": "rational", "numerator": "1", "denominator": "2"},
            "right": {"kind": "rational", "numerator": "2", "denominator": "3"},
            "relation": "lt",
            "difference": {"kind": "rational", "numerator": "-1", "denominator": "6"},
            "method": "exact_arithmetic",
            "scope": "exact_at_supplied_bindings",
            "scope_limitations": ["confirms the identity at the supplied bindings; it does not prove it for all inputs"]
        }))),
        Example::new(
            "a negative tolerance is rejected",
            example_args(&[
                ("left", serde_json::json!("1")),
                ("right", serde_json::json!("1")),
                ("relation", serde_json::json!("eq")),
                ("tolerance", serde_json::json!("-1")),
            ]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_inequality(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let bindings = bindings_arg(args, &ctx.limits)?;
    let left = evaluate_expression(args.text("left")?, &bindings, ctx)?;
    let right = evaluate_expression(args.text("right")?, &bindings, ctx)?;
    let relation = Relation::parse(args.text("relation")?)?;
    let tolerance = tolerance_arg(args)?;
    let difference = exact_difference(&left, &right)?;
    let confirmed = relation_holds(relation, &left, &right, tolerance.as_ref())?;
    let mut fields = vec![
        ("status", status_value(confirmed)),
        ("left", Value::Number(left)),
        ("right", Value::Number(right)),
        ("relation", Value::text(relation.as_str())),
        ("difference", Value::Number(difference)),
        ("method", Value::text("exact_arithmetic")),
    ];
    if !confirmed {
        fields.push(("bindings", bindings_to_value(&bindings)));
    }
    fields.extend(scope_fields(
        SCOPE_EXACT_AT_SUPPLIED_BINDINGS,
        &[LIMITATION_EXACT_AT_SUPPLIED_BINDINGS],
    ));
    Ok(Outcome::exact(Value::record(fields)))
}

// ---------------------------------------------------------------------------
// verify.expression
// ---------------------------------------------------------------------------

fn expression_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "verify.expression",
        "verify",
        "1.0.0",
        "Verify expression",
        "Compare one restricted arithmetic expression to an exact value.",
    )
    .with_description(
        "The expression is evaluated with exact integer, rational, and decimal arithmetic and compared to the claimed value under the requested relation. A refutation includes the witness bindings. Scope: the result is exact at the supplied bindings and does not prove the relation for all inputs.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "expression",
            "Restricted arithmetic expression to evaluate.",
            expression_schema(),
        ),
        ParamDescriptor::required(
            "relation",
            "Relation to check: lt, le, gt, ge, eq, or ne.",
            enum_schema(&["lt", "le", "gt", "ge", "eq", "ne"]),
        ),
        ParamDescriptor::required(
            "value",
            "Exact value to compare against.",
            ValueSchema::exact(),
        ),
        ParamDescriptor::optional(
            "bindings",
            "Record mapping free variables to exact numbers (canonical values or numeric shorthand strings). Float64 values are rejected.",
            bindings_schema(),
        ),
    ])
    .with_output(
        record_schema(vec![
            field("status", enum_schema(&["confirmed", "refuted"])),
            field("left", number_any()),
            field("right", number_any()),
            field("relation", enum_schema(&["lt", "le", "gt", "ge", "eq", "ne"])),
            field("difference", number_any()),
            field("method", text_schema()),
            optional_field("scope", text_schema()),
            optional_field("scope_limitations", ValueSchema::array(text_schema())),
            optional_field("scope", text_schema()),
            optional_field("scope_limitations", ValueSchema::array(text_schema())),
            optional_field("bindings", bindings_schema()),
        ]),
        "Verification record with status, evaluated expression, claimed value, and exact difference.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Constant)
    .with_units_rule("The expression must produce a dimensionless exact number.")
    .with_method_ref("docs/methods/verify.md#expression")
    .with_tags(["verification", "exact", "expression"])
    .with_examples(vec![
        Example::new(
            "expression equals its claimed value",
            example_args(&[
                ("expression", serde_json::json!("2^10")),
                ("relation", serde_json::json!("eq")),
                ("value", serde_json::json!(1024)),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "status": "confirmed",
            "left": {"kind": "integer", "value": "1024"},
            "right": {"kind": "integer", "value": "1024"},
            "relation": "eq",
            "difference": {"kind": "integer", "value": "0"},
            "method": "exact_arithmetic"
        }))),
        Example::new(
            "bindings resolve free variables",
            example_args(&[
                ("expression", serde_json::json!("x + 1")),
                ("relation", serde_json::json!("eq")),
                ("value", serde_json::json!(3)),
                ("bindings", serde_json::json!({"x": 2})),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "status": "confirmed",
            "left": {"kind": "integer", "value": "3"},
            "right": {"kind": "integer", "value": "3"},
            "relation": "eq",
            "difference": {"kind": "integer", "value": "0"},
            "method": "exact_arithmetic"
        }))),
        Example::new(
            "float64 bindings are rejected",
            example_args(&[
                ("expression", serde_json::json!("x")),
                ("relation", serde_json::json!("eq")),
                ("value", serde_json::json!(1)),
                (
                    "bindings",
                    serde_json::json!({"x": {"kind": "float64", "value": "0.5"}}),
                ),
            ]),
        )
        .with_error(ErrorCode::UnsupportedNumericMode),
    ])
}

fn invoke_expression(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let bindings = bindings_arg(args, &ctx.limits)?;
    let left = evaluate_expression(args.text("expression")?, &bindings, ctx)?;
    let right = args.number("value")?.clone();
    ensure_exact(&right, "value")?;
    let relation = Relation::parse(args.text("relation")?)?;
    let difference = exact_difference(&left, &right)?;
    let confirmed = relation_holds(relation, &left, &right, None)?;
    let mut fields = vec![
        ("status", status_value(confirmed)),
        ("left", Value::Number(left)),
        ("right", Value::Number(right)),
        ("relation", Value::text(relation.as_str())),
        ("difference", Value::Number(difference)),
        ("method", Value::text("exact_arithmetic")),
    ];
    if !confirmed {
        fields.push(("bindings", bindings_to_value(&bindings)));
    }
    Ok(Outcome::exact(Value::record(fields)))
}

// ---------------------------------------------------------------------------
// verify.total
// ---------------------------------------------------------------------------

fn total_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "verify.total",
        "verify",
        "1.0.0",
        "Verify total",
        "Compare an exact sum of exact numbers to a claimed total.",
    )
    .with_description(
        "The values are summed exactly (decimal arithmetic when every value is an integer or decimal, rational arithmetic otherwise) and compared to the claimed total. A mismatch reports the exact difference.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "values",
            "Array of exact numbers to sum.",
            ValueSchema::array(ValueSchema::exact()),
        ),
        ParamDescriptor::required(
            "claimed_total",
            "Claimed exact total.",
            ValueSchema::exact(),
        ),
    ])
    .with_output(
        record_schema(vec![
            field("status", enum_schema(&["confirmed", "refuted"])),
            field("computed", number_any()),
            field("claimed", number_any()),
            field("difference", number_any()),
            field("method", text_schema()),
            optional_field("scope", text_schema()),
            optional_field("scope_limitations", ValueSchema::array(text_schema())),
        ]),
        "Verification record with status, computed sum, claimed total, and exact difference.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_units_rule("All values must be dimensionless exact numbers.")
    .with_method_ref("docs/methods/verify.md#total")
    .with_tags(["verification", "exact", "sum"])
    .with_examples(vec![
        Example::new(
            "exact total",
            example_args(&[
                ("values", serde_json::json!([0.1, 0.2])),
                ("claimed_total", serde_json::json!(0.3)),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "status": "confirmed",
            "computed": {"kind": "decimal", "value": "0.3"},
            "claimed": {"kind": "decimal", "value": "0.3"},
            "difference": {"kind": "integer", "value": "0"},
            "method": "exact_sum"
        }))),
        Example::new(
            "total mismatch",
            example_args(&[
                ("values", serde_json::json!([0.1, 0.2, 0.3])),
                ("claimed_total", serde_json::json!(0.7)),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "status": "refuted",
            "computed": {"kind": "decimal", "value": "0.6"},
            "claimed": {"kind": "decimal", "value": "0.7"},
            "difference": {"kind": "rational", "numerator": "-1", "denominator": "10"},
            "method": "exact_sum"
        }))),
    ])
}

fn exact_sum(values: &[Number], ctx: &ExecContext) -> Result<Number, EngineError> {
    let numeric = NumericContext::default();
    if values
        .iter()
        .all(|value| matches!(value, Number::Integer(_) | Number::Decimal(_)))
    {
        let mut total = Number::integer(0);
        for value in values {
            ctx.check()?;
            total = total.add(value, &numeric, &ctx.limits)?.value;
        }
        return Ok(total);
    }
    let mut total = BigRational::zero();
    for value in values {
        ctx.check()?;
        let rational = value
            .as_exact_rational()
            .ok_or_else(|| float_rejected("values"))?;
        total += rational;
    }
    Ok(rational_to_number(total))
}

fn invoke_total(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let values = exact_vector(args.require("values")?, "values", &ctx.limits)?;
    let claimed = args.number("claimed_total")?.clone();
    ensure_exact(&claimed, "claimed_total")?;
    let computed = exact_sum(&values, ctx)?;
    let difference = exact_difference(&computed, &claimed)?;
    let confirmed = difference.is_zero();
    let mut fields = vec![
        ("status", status_value(confirmed)),
        ("computed", Value::Number(computed)),
        ("claimed", Value::Number(claimed)),
        ("difference", Value::Number(difference)),
        ("method", Value::text("exact_sum")),
    ];
    fields.extend(scope_fields(
        SCOPE_EXACT_TOTAL_OF_SUPPLIED_VALUES,
        &[LIMITATION_EXACT_TOTAL_OF_SUPPLIED_VALUES],
    ));
    Ok(Outcome::exact(Value::record(fields)))
}

// ---------------------------------------------------------------------------
// verify.matrix_residual
// ---------------------------------------------------------------------------

fn matrix_residual_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "verify.matrix_residual",
        "verify",
        "1.0.0",
        "Verify matrix residual",
        "Compute the exact infinity norm of A*x - b for a claimed solution.",
    )
    .with_description(
        "All matrix and vector entries must be exact numbers. The residual is computed with rational arithmetic and the infinity norm is returned exactly. A zero norm confirms the claimed solution exactly; a refutation also reports the residual vector.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "matrix",
            "Coefficient matrix A.",
            ValueSchema::Matrix {
                max_elems: None,
                square: false,
            },
        ),
        ParamDescriptor::required(
            "vector",
            "Right-hand side b as an array of exact numbers.",
            ValueSchema::array(ValueSchema::exact()),
        ),
        ParamDescriptor::required(
            "claimed_solution",
            "Claimed solution x as an array of exact numbers.",
            ValueSchema::array(ValueSchema::exact()),
        ),
    ])
    .with_output(
        record_schema(vec![
            field("status", enum_schema(&["confirmed", "refuted"])),
            field("residual_norm", number_any()),
            field("method", text_schema()),
            optional_field("scope", text_schema()),
            optional_field("scope_limitations", ValueSchema::array(text_schema())),
            optional_field("residual", ValueSchema::array(number_any())),
        ]),
        "Verification record with status, exact residual infinity norm, and method.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Quadratic)
    .with_units_rule("All matrix and vector entries must be dimensionless exact numbers.")
    .with_method_ref("docs/methods/verify.md#matrix_residual")
    .with_tags(["verification", "exact", "matrix"])
    .with_examples(vec![
        Example::new(
            "identity system has zero residual",
            example_args(&[
                (
                    "matrix",
                    serde_json::json!({"kind": "matrix", "rows": 2, "cols": 2, "data": [1, 0, 0, 1]}),
                ),
                ("vector", serde_json::json!([1, 2])),
                ("claimed_solution", serde_json::json!([1, 2])),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "status": "confirmed",
            "residual_norm": {"kind": "integer", "value": "0"},
            "method": "exact_matrix_residual"
        }))),
        Example::new(
            "a wrong solution has a nonzero residual",
            example_args(&[
                (
                    "matrix",
                    serde_json::json!({"kind": "matrix", "rows": 2, "cols": 2, "data": [1, 0, 0, 1]}),
                ),
                ("vector", serde_json::json!([1, 2])),
                ("claimed_solution", serde_json::json!([1, 3])),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "status": "refuted",
            "residual_norm": {"kind": "integer", "value": "1"},
            "method": "exact_matrix_residual",
            "residual": [
                {"kind": "integer", "value": "0"},
                {"kind": "integer", "value": "1"}
            ]
        }))),
    ])
}

fn invoke_matrix_residual(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let matrix = args.require("matrix")?;
    let Value::Matrix { rows, cols, data } = matrix else {
        return Err(EngineError::malformed(format!(
            "matrix must be a matrix, found {}",
            matrix.kind_name()
        ))
        .with_path("matrix".to_string()));
    };
    let (rows, cols) = (*rows as usize, *cols as usize);
    let mut coefficients = Vec::with_capacity(data.len());
    for (index, item) in data.iter().enumerate() {
        coefficients.push(exact_number_from_value(
            item,
            &format!("matrix[{index}]"),
            &ctx.limits,
        )?);
    }
    let vector = exact_vector(args.require("vector")?, "vector", &ctx.limits)?;
    let solution = exact_vector(
        args.require("claimed_solution")?,
        "claimed_solution",
        &ctx.limits,
    )?;
    if vector.len() != rows {
        return Err(EngineError::malformed(format!(
            "vector length {} does not match matrix rows {rows}",
            vector.len()
        ))
        .with_path("vector".to_string()));
    }
    if solution.len() != cols {
        return Err(EngineError::malformed(format!(
            "claimed_solution length {} does not match matrix columns {cols}",
            solution.len()
        ))
        .with_path("claimed_solution".to_string()));
    }
    let mut residual = Vec::with_capacity(rows);
    for (row, rhs) in vector.iter().enumerate() {
        ctx.check()?;
        let mut accumulator = BigRational::zero();
        for (col, x) in solution.iter().enumerate() {
            let coefficient = coefficients[row * cols + col]
                .as_exact_rational()
                .ok_or_else(|| float_rejected("matrix"))?;
            let value = x
                .as_exact_rational()
                .ok_or_else(|| float_rejected("claimed_solution"))?;
            accumulator += coefficient * value;
        }
        let rhs = rhs
            .as_exact_rational()
            .ok_or_else(|| float_rejected("vector"))?;
        residual.push(accumulator - rhs);
    }
    let norm = residual.iter().fold(BigRational::zero(), |best, entry| {
        let magnitude = entry.abs();
        if magnitude > best { magnitude } else { best }
    });
    let residual_norm = rational_to_number(norm);
    let confirmed = residual_norm.is_zero();
    let mut fields = vec![
        ("status", status_value(confirmed)),
        ("residual_norm", Value::Number(residual_norm)),
        ("method", Value::text("exact_matrix_residual")),
    ];
    fields.extend(scope_fields(
        SCOPE_EXACT_RESIDUAL_AT_SUPPLIED_VALUES,
        &[LIMITATION_EXACT_RESIDUAL_AT_SUPPLIED_VALUES],
    ));
    if !confirmed {
        fields.push((
            "residual",
            Value::Array(
                residual
                    .iter()
                    .map(|entry| Value::Number(rational_to_number(entry.clone())))
                    .collect(),
            ),
        ));
    }
    Ok(Outcome::exact(Value::record(fields)))
}

// ---------------------------------------------------------------------------
// verify.inequality_grid
// ---------------------------------------------------------------------------

fn grid_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "verify.inequality_grid",
        "verify",
        "1.0.0",
        "Verify inequality on a grid",
        "Search a finite, evenly spaced grid for a counterexample to a relation between two expressions.",
    )
    .with_description(
        "This is a search, not a proof. Both expressions are evaluated at up to 10000 evenly spaced exact points in [lower, upper]. A refutation reports the first counterexample; a confirmed result means only that the relation held at every sampled point and carries a warning.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "left",
            "Left-hand restricted arithmetic expression.",
            expression_schema(),
        ),
        ParamDescriptor::required(
            "right",
            "Right-hand restricted arithmetic expression.",
            expression_schema(),
        ),
        ParamDescriptor::required(
            "relation",
            "Relation to check: lt, le, gt, ge, eq, or ne.",
            enum_schema(&["lt", "le", "gt", "ge", "eq", "ne"]),
        ),
        ParamDescriptor::required(
            "variable",
            "Name of the single variable sampled over the interval.",
            text_schema(),
        ),
        ParamDescriptor::required("lower", "Inclusive lower endpoint.", ValueSchema::exact()),
        ParamDescriptor::required("upper", "Inclusive upper endpoint.", ValueSchema::exact()),
        ParamDescriptor::required(
            "samples",
            "Number of evenly spaced sample points, at least 2 and at most 10000.",
            integer_schema(),
        ),
    ])
    .with_output(
        record_schema(vec![
            field("status", enum_schema(&["confirmed", "refuted"])),
            field("checked_points", integer_schema()),
            field("counterexample", ValueSchema::Any),
            field("method", text_schema()),
            optional_field("scope", text_schema()),
            optional_field("scope_limitations", ValueSchema::array(text_schema())),
        ]),
        "Search record with status, number of points checked, and the first counterexample if any.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_units_rule("Both expressions must produce dimensionless exact numbers.")
    .with_method_ref("docs/methods/verify.md#inequality_grid")
    .with_tags(["verification", "exact", "search", "grid"])
    .with_examples(vec![
        Example::new(
            "grid confirms a positive quadratic",
            example_args(&[
                ("left", serde_json::json!("x^2 + 1")),
                ("right", serde_json::json!("0")),
                ("relation", serde_json::json!("gt")),
                ("variable", serde_json::json!("x")),
                ("lower", serde_json::json!(-2)),
                ("upper", serde_json::json!(2)),
                ("samples", serde_json::json!(5)),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "status": "confirmed",
            "checked_points": {"kind": "integer", "value": "5"},
            "counterexample": null,
            "method": "grid_search",
            "scope": "finite_grid_search",
            "scope_limitations": [
                "a finite grid cannot establish that a statement holds everywhere; confirmed means only that no counterexample was found on the sampled points"
            ]
        }))),
        Example::new(
            "grid finds a counterexample",
            example_args(&[
                ("left", serde_json::json!("x^2")),
                ("right", serde_json::json!("x")),
                ("relation", serde_json::json!("ge")),
                ("variable", serde_json::json!("x")),
                ("lower", serde_json::json!(0)),
                ("upper", serde_json::json!(1)),
                ("samples", serde_json::json!(3)),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "status": "refuted",
            "checked_points": {"kind": "integer", "value": "2"},
            "counterexample": {
                "value": {"kind": "rational", "numerator": "1", "denominator": "2"},
                "left": {"kind": "rational", "numerator": "1", "denominator": "4"},
                "right": {"kind": "rational", "numerator": "1", "denominator": "2"}
            },
            "method": "grid_search",
            "scope": "finite_grid_search",
            "scope_limitations": [
                "a finite grid cannot establish that a statement holds everywhere; confirmed means only that no counterexample was found on the sampled points"
            ]
        }))),
    ])
}

fn invoke_inequality_grid(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let left_source = args.text("left")?;
    let right_source = args.text("right")?;
    let relation = Relation::parse(args.text("relation")?)?;
    let variable = args.text("variable")?.to_string();
    let lower = args.number("lower")?.clone();
    let upper = args.number("upper")?.clone();
    ensure_exact(&lower, "lower")?;
    ensure_exact(&upper, "upper")?;
    let samples = args.usize_param("samples")?;
    if samples < 2 {
        return Err(EngineError::domain("samples must be at least 2"));
    }
    if samples > 10_000 {
        return Err(EngineError::resource(
            "samples must not exceed 10000 for a grid search",
        ));
    }
    let left_expr = parse_expression(left_source, &ctx.limits)?;
    let right_expr = parse_expression(right_source, &ctx.limits)?;
    let lower_rational = lower
        .as_exact_rational()
        .ok_or_else(|| float_rejected("lower"))?;
    let upper_rational = upper
        .as_exact_rational()
        .ok_or_else(|| float_rejected("upper"))?;
    let span = &upper_rational - &lower_rational;
    let step_denominator = BigRational::from_integer(BigInt::from(samples - 1));
    let mut bindings = BTreeMap::new();
    let mut checked_points = 0usize;
    let mut counterexample = None;
    for index in 0..samples {
        ctx.check()?;
        let fraction = BigRational::from_integer(BigInt::from(index)) / &step_denominator;
        let point = &lower_rational + &span * fraction;
        let point_number = rational_to_number(point);
        bindings.insert(variable.clone(), point_number.clone());
        let left = evaluate_parsed(&left_expr, &bindings, ctx)?;
        let right = evaluate_parsed(&right_expr, &bindings, ctx)?;
        checked_points += 1;
        if !relation_holds(relation, &left, &right, None)? {
            counterexample = Some((point_number, left, right));
            break;
        }
    }
    let confirmed = counterexample.is_none();
    let counterexample_value = match counterexample {
        Some((point, left, right)) => Value::record([
            ("value", Value::Number(point)),
            ("left", Value::Number(left)),
            ("right", Value::Number(right)),
        ]),
        None => Value::Null,
    };
    let mut fields = vec![
        ("status", status_value(confirmed)),
        (
            "checked_points",
            Value::Number(Number::Integer(BigInt::from(checked_points))),
        ),
        ("counterexample", counterexample_value),
        ("method", Value::text("grid_search")),
    ];
    fields.extend(scope_fields(
        SCOPE_FINITE_GRID_SEARCH,
        &[LIMITATION_FINITE_GRID_SEARCH],
    ));
    let value = Value::record(fields);
    let mut outcome = Outcome::exact(value);
    if confirmed {
        outcome = outcome.with_warning(Warning::new(
            "grid_search_is_not_a_proof",
            "A finite grid can refute a claim but cannot prove it; the relation held only at the sampled points.",
        ));
    }
    Ok(outcome)
}

// ---------------------------------------------------------------------------
// verify.probability
// ---------------------------------------------------------------------------

fn probability_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "verify.probability",
        "verify",
        "1.0.0",
        "Verify probability",
        "Check that a probability lies within [0, 1] or supplied bounds.",
    )
    .with_description(
        "Exact inputs are compared exactly. Float64 inputs are accepted here and the result is labelled approximate; the comparison still uses the exact binary value. Bounds are inclusive and default to [0, 1].",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "value",
            "Probability value to check; may be exact or float64.",
            number_any(),
        ),
        ParamDescriptor::optional(
            "lower",
            "Inclusive lower bound; defaults to 0.",
            number_any(),
        ),
        ParamDescriptor::optional(
            "upper",
            "Inclusive upper bound; defaults to 1.",
            number_any(),
        ),
    ])
    .with_output(
        record_schema(vec![
            field("status", enum_schema(&["confirmed", "refuted"])),
            field("value", number_any()),
            field("lower", number_any()),
            field("upper", number_any()),
            field("method", text_schema()),
            optional_field("scope", text_schema()),
            optional_field("scope_limitations", ValueSchema::array(text_schema())),
            field("approximate", ValueSchema::Bool),
        ]),
        "Range-check record with the checked value, bounds, and approximation flag.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Constant)
    .with_units_rule("Bounds and value must be dimensionless numbers.")
    .with_method_ref("docs/methods/verify.md#probability")
    .with_tags(["verification", "probability"])
    .with_examples(vec![
        Example::new(
            "a decimal probability is in range",
            example_args(&[("value", serde_json::json!("0.5"))]),
        )
        .with_value(parse_value(serde_json::json!({
            "status": "confirmed",
            "value": {"kind": "decimal", "value": "0.5"},
            "lower": {"kind": "integer", "value": "0"},
            "upper": {"kind": "integer", "value": "1"},
            "method": "probability_bounds",
            "approximate": false
        }))),
        Example::new(
            "an out-of-range probability is refuted",
            example_args(&[("value", serde_json::json!("1.5"))]),
        )
        .with_value(parse_value(serde_json::json!({
            "status": "refuted",
            "value": {"kind": "decimal", "value": "1.5"},
            "lower": {"kind": "integer", "value": "0"},
            "upper": {"kind": "integer", "value": "1"},
            "method": "probability_bounds",
            "approximate": false
        }))),
        Example::new(
            "a float64 probability is labelled approximate",
            example_args(&[(
                "value",
                serde_json::json!({"kind": "float64", "value": "0.5"}),
            )]),
        )
        .with_value(parse_value(serde_json::json!({
            "status": "confirmed",
            "value": {"kind": "float64", "value": "0.5"},
            "lower": {"kind": "integer", "value": "0"},
            "upper": {"kind": "integer", "value": "1"},
            "method": "probability_bounds",
            "approximate": true
        }))),
    ])
}

fn invoke_probability(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let value = args.number("value")?.clone();
    let lower = args
        .optional_number("lower")?
        .cloned()
        .unwrap_or_else(|| Number::integer(0));
    let upper = args
        .optional_number("upper")?
        .cloned()
        .unwrap_or_else(|| Number::integer(1));
    if lower.compare(&upper)? == Ordering::Greater {
        return Err(EngineError::domain(
            "probability lower bound must not exceed the upper bound",
        ));
    }
    let confirmed =
        lower.compare(&value)? != Ordering::Greater && value.compare(&upper)? != Ordering::Greater;
    let approximate = value.is_float() || lower.is_float() || upper.is_float();
    let mut fields = vec![
        ("status", status_value(confirmed)),
        ("value", Value::Number(value)),
        ("lower", Value::Number(lower)),
        ("upper", Value::Number(upper)),
        ("method", Value::text("probability_bounds")),
        ("approximate", Value::Bool(approximate)),
    ];
    fields.extend(scope_fields(
        SCOPE_RANGE_CHECK_AT_SUPPLIED_VALUE,
        &[LIMITATION_RANGE_CHECK_AT_SUPPLIED_VALUE],
    ));
    let value = Value::record(fields);
    let mut outcome = Outcome::new(
        value,
        if approximate {
            Exactness::Approximate
        } else {
            Exactness::Exact
        },
    );
    if approximate {
        outcome = outcome.with_warning(Warning::new(
            "approximate_probability_input",
            "The probability input is float64; the range check is labelled approximate.",
        ));
    }
    Ok(outcome)
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

/// Build the verify module with all of its registered functions.
pub fn module() -> Module {
    let functions: Vec<Arc<dyn Function>> = vec![
        SimpleFunction::arc(equality_descriptor(), invoke_equality),
        SimpleFunction::arc(inequality_descriptor(), invoke_inequality),
        SimpleFunction::arc(expression_descriptor(), invoke_expression),
        SimpleFunction::arc(total_descriptor(), invoke_total),
        SimpleFunction::arc(matrix_residual_descriptor(), invoke_matrix_residual),
        SimpleFunction::arc(grid_descriptor(), invoke_inequality_grid),
        SimpleFunction::arc(probability_descriptor(), invoke_probability),
    ];
    let descriptor = ModuleDescriptor::new(
        "verify",
        "Verify",
        "1.0.0",
        "Independent checks for agent claims using exact arithmetic.",
    )
    .with_capabilities(vec![
        "exact_arithmetic",
        "claim_verification",
        "counterexample_search",
        "matrix_residual",
    ])
    .with_dependencies(vec!["core"])
    .with_modes(all_modes())
    .with_source("crates/bicmath-verify");
    Module::new(descriptor, functions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bicmath_core::contract::ExampleExpectation;

    fn ctx() -> ExecContext {
        ExecContext::conservative()
    }

    fn call(id: &str, raw: serde_json::Value) -> Result<Outcome, EngineError> {
        let module = module();
        let function = module
            .functions
            .iter()
            .find(|f| f.descriptor().id == id)
            .expect("function exists");
        let args_json = raw.as_object().expect("object args");
        let mut values = BTreeMap::new();
        for (name, value) in args_json {
            let param = function
                .descriptor()
                .parameter(name)
                .expect("parameter exists");
            values.insert(
                name.clone(),
                param
                    .schema
                    .coerce(value, name, &ctx().limits, param.numeric_shorthand)
                    .expect("argument coerces"),
            );
        }
        function.invoke(&Args::new(values), &ctx())
    }

    fn field<'a>(value: &'a Value, name: &str) -> &'a Value {
        value
            .as_record()
            .expect("record")
            .get(name)
            .unwrap_or_else(|| panic!("missing field {name}"))
    }

    fn status(value: &Value) -> String {
        field(value, "status")
            .as_text()
            .expect("status text")
            .to_string()
    }

    fn number_of(value: &Value) -> Number {
        value.as_number().expect("number").clone()
    }

    #[test]
    fn module_metadata_matches_contract() {
        let module = module();
        assert_eq!(module.descriptor.id, "verify");
        assert_eq!(module.descriptor.version, "1.0.0");
        let ids: Vec<&str> = module
            .functions
            .iter()
            .map(|function| function.descriptor().id.as_str())
            .collect();
        assert_eq!(
            ids,
            vec![
                "verify.equality",
                "verify.inequality",
                "verify.expression",
                "verify.total",
                "verify.matrix_residual",
                "verify.inequality_grid",
                "verify.probability",
            ]
        );
    }

    #[test]
    fn equality_confirms_exact_decimal_addition() {
        let outcome = call(
            "verify.equality",
            serde_json::json!({"left": "0.1 + 0.2", "right": "0.3"}),
        )
        .unwrap();
        assert_eq!(status(&outcome.value), "confirmed");
        assert_eq!(
            number_of(field(&outcome.value, "difference")).to_string(),
            "0"
        );
        assert!(matches!(
            field(&outcome.value, "tolerance_used"),
            Value::Null
        ));
    }

    #[test]
    fn equality_refutes_truncated_third_with_exact_difference() {
        let outcome = call(
            "verify.equality",
            serde_json::json!({"left": "1/3", "right": "0.333"}),
        )
        .unwrap();
        assert_eq!(status(&outcome.value), "refuted");
        assert_eq!(
            number_of(field(&outcome.value, "difference")).to_string(),
            "1/3000"
        );
        assert!(matches!(
            field(&outcome.value, "bindings"),
            Value::Record(_)
        ));
    }

    #[test]
    fn equality_confirms_rational_sum() {
        let outcome = call(
            "verify.equality",
            serde_json::json!({"left": "1/3 + 1/6", "right": "1/2"}),
        )
        .unwrap();
        assert_eq!(status(&outcome.value), "confirmed");
    }

    #[test]
    fn equality_tolerance_decides_rounded_claims() {
        let confirmed = call(
            "verify.equality",
            serde_json::json!({"left": "1/3", "right": "0.333", "tolerance": "0.001"}),
        )
        .unwrap();
        assert_eq!(status(&confirmed.value), "confirmed");
        let refuted = call(
            "verify.equality",
            serde_json::json!({"left": "1/3", "right": "0.333", "tolerance": "0.0001"}),
        )
        .unwrap();
        assert_eq!(status(&refuted.value), "refuted");
    }

    #[test]
    fn equality_rejects_negative_tolerance() {
        let error = call(
            "verify.equality",
            serde_json::json!({"left": "1", "right": "1", "tolerance": "-1"}),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::DomainViolation);
    }

    #[test]
    fn inequality_relations_hold_exactly() {
        let lt = call(
            "verify.inequality",
            serde_json::json!({"left": "1/2", "right": "2/3", "relation": "lt"}),
        )
        .unwrap();
        assert_eq!(status(&lt.value), "confirmed");
        let ge = call(
            "verify.inequality",
            serde_json::json!({"left": "1/2", "right": "1/2", "relation": "ge"}),
        )
        .unwrap();
        assert_eq!(status(&ge.value), "confirmed");
        let ne = call(
            "verify.inequality",
            serde_json::json!({"left": "1/3", "right": "0.333", "relation": "ne"}),
        )
        .unwrap();
        assert_eq!(status(&ne.value), "confirmed");
    }

    #[test]
    fn inequality_rejects_tolerance_for_ordering() {
        let error = call(
            "verify.inequality",
            serde_json::json!({"left": "1", "right": "2", "relation": "lt", "tolerance": "1"}),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::DomainViolation);
    }

    #[test]
    fn expression_uses_exact_bindings() {
        let outcome = call(
            "verify.expression",
            serde_json::json!({
                "expression": "x + 1",
                "relation": "eq",
                "value": 3,
                "bindings": {"x": 2}
            }),
        )
        .unwrap();
        assert_eq!(status(&outcome.value), "confirmed");
        let rational = call(
            "verify.expression",
            serde_json::json!({
                "expression": "x",
                "relation": "eq",
                "value": {"kind": "rational", "numerator": "1", "denominator": "3"},
                "bindings": {"x": {"kind": "rational", "numerator": "1", "denominator": "3"}}
            }),
        )
        .unwrap();
        assert_eq!(status(&rational.value), "confirmed");
    }

    #[test]
    fn float64_bindings_are_rejected() {
        let error = call(
            "verify.expression",
            serde_json::json!({
                "expression": "x",
                "relation": "eq",
                "value": 1,
                "bindings": {"x": {"kind": "float64", "value": "0.5"}}
            }),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::UnsupportedNumericMode);
    }

    #[test]
    fn expression_function_table_is_closed() {
        let outcome = call(
            "verify.expression",
            serde_json::json!({
                "expression": "abs(-3) + min(2, 5) + max(1, 4) + sign(-9) + pow(2, 3)",
                "relation": "eq",
                "value": 16
            }),
        )
        .unwrap();
        assert_eq!(status(&outcome.value), "confirmed");
        let error = call(
            "verify.expression",
            serde_json::json!({
                "expression": "sin(1)",
                "relation": "eq",
                "value": 1
            }),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::UnknownFunction);
    }

    #[test]
    fn unknown_function_is_a_structured_error() {
        let error = call(
            "verify.equality",
            serde_json::json!({"left": "nope(1)", "right": "1"}),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::UnknownFunction);
    }

    #[test]
    fn total_confirms_and_detects_mismatch() {
        let confirmed = call(
            "verify.total",
            serde_json::json!({"values": [0.1, 0.2, 0.3], "claimed_total": 0.6}),
        )
        .unwrap();
        assert_eq!(status(&confirmed.value), "confirmed");
        assert_eq!(
            number_of(field(&confirmed.value, "computed")).to_string(),
            "0.6"
        );
        let refuted = call(
            "verify.total",
            serde_json::json!({"values": [0.1, 0.2, 0.3], "claimed_total": 0.7}),
        )
        .unwrap();
        assert_eq!(status(&refuted.value), "refuted");
        assert_eq!(
            number_of(field(&refuted.value, "difference")).to_string(),
            "-1/10"
        );
    }

    #[test]
    fn total_sums_rationals_exactly() {
        let outcome = call(
            "verify.total",
            serde_json::json!({
                "values": [
                    {"kind": "rational", "numerator": "1", "denominator": "3"},
                    {"kind": "rational", "numerator": "1", "denominator": "6"}
                ],
                "claimed_total": {"kind": "rational", "numerator": "1", "denominator": "2"}
            }),
        )
        .unwrap();
        assert_eq!(status(&outcome.value), "confirmed");
    }

    #[test]
    fn matrix_residual_zero_and_nonzero() {
        let confirmed = call(
            "verify.matrix_residual",
            serde_json::json!({
                "matrix": {"kind": "matrix", "rows": 2, "cols": 2, "data": [1, 0, 0, 1]},
                "vector": [1, 2],
                "claimed_solution": [1, 2]
            }),
        )
        .unwrap();
        assert_eq!(status(&confirmed.value), "confirmed");
        assert_eq!(
            number_of(field(&confirmed.value, "residual_norm")).to_string(),
            "0"
        );
        let refuted = call(
            "verify.matrix_residual",
            serde_json::json!({
                "matrix": {"kind": "matrix", "rows": 2, "cols": 2, "data": [1, 0, 0, 1]},
                "vector": [1, 2],
                "claimed_solution": [1, 3]
            }),
        )
        .unwrap();
        assert_eq!(status(&refuted.value), "refuted");
        assert_eq!(
            number_of(field(&refuted.value, "residual_norm")).to_string(),
            "1"
        );
        assert!(matches!(field(&refuted.value, "residual"), Value::Array(_)));
    }

    #[test]
    fn matrix_residual_uses_exact_fractions() {
        let outcome = call(
            "verify.matrix_residual",
            serde_json::json!({
                "matrix": {"kind": "matrix", "rows": 1, "cols": 1, "data": [3]},
                "vector": [1],
                "claimed_solution": [
                    {"kind": "rational", "numerator": "1", "denominator": "3"}
                ]
            }),
        )
        .unwrap();
        assert_eq!(status(&outcome.value), "confirmed");
    }

    #[test]
    fn grid_refutes_x_squared_ge_x_with_counterexample() {
        let outcome = call(
            "verify.inequality_grid",
            serde_json::json!({
                "left": "x^2",
                "right": "x",
                "relation": "ge",
                "variable": "x",
                "lower": 0,
                "upper": 1,
                "samples": 3
            }),
        )
        .unwrap();
        assert_eq!(status(&outcome.value), "refuted");
        let counterexample = field(&outcome.value, "counterexample");
        assert_eq!(number_of(field(counterexample, "value")).to_string(), "1/2");
        assert_eq!(number_of(field(counterexample, "left")).to_string(), "1/4");
        assert_eq!(number_of(field(counterexample, "right")).to_string(), "1/2");
        assert_eq!(
            number_of(field(&outcome.value, "checked_points")).to_string(),
            "2"
        );
        assert!(outcome.warnings.is_empty());
    }

    #[test]
    fn grid_confirms_positive_quadratic_with_warning() {
        let outcome = call(
            "verify.inequality_grid",
            serde_json::json!({
                "left": "x^2 + 1",
                "right": "0",
                "relation": "gt",
                "variable": "x",
                "lower": -10,
                "upper": 10,
                "samples": 101
            }),
        )
        .unwrap();
        assert_eq!(status(&outcome.value), "confirmed");
        assert_eq!(
            number_of(field(&outcome.value, "checked_points")).to_string(),
            "101"
        );
        assert!(matches!(
            field(&outcome.value, "counterexample"),
            Value::Null
        ));
        assert!(
            outcome
                .warnings
                .iter()
                .any(|warning| warning.code == "grid_search_is_not_a_proof")
        );
    }

    #[test]
    fn grid_rejects_too_few_samples() {
        let error = call(
            "verify.inequality_grid",
            serde_json::json!({
                "left": "x",
                "right": "0",
                "relation": "gt",
                "variable": "x",
                "lower": 0,
                "upper": 1,
                "samples": 1
            }),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::DomainViolation);
    }

    #[test]
    fn probability_checks_bounds() {
        let confirmed = call("verify.probability", serde_json::json!({"value": 0.5})).unwrap();
        assert_eq!(status(&confirmed.value), "confirmed");
        assert_eq!(confirmed.exactness, Exactness::Exact);
        let refuted = call("verify.probability", serde_json::json!({"value": 1.5})).unwrap();
        assert_eq!(status(&refuted.value), "refuted");
        let custom = call(
            "verify.probability",
            serde_json::json!({"value": 0.5, "lower": 0.6, "upper": 1}),
        )
        .unwrap();
        assert_eq!(status(&custom.value), "refuted");
        let boundary = call(
            "verify.probability",
            serde_json::json!({"value": 2, "lower": 0, "upper": 2}),
        )
        .unwrap();
        assert_eq!(status(&boundary.value), "confirmed");
    }

    #[test]
    fn probability_labels_float64_input_approximate() {
        let outcome = call(
            "verify.probability",
            serde_json::json!({"value": {"kind": "float64", "value": "0.5"}}),
        )
        .unwrap();
        assert_eq!(status(&outcome.value), "confirmed");
        assert_eq!(outcome.exactness, Exactness::Approximate);
        assert!(
            field(&outcome.value, "approximate")
                .as_bool()
                .expect("bool")
        );
        assert!(
            outcome
                .warnings
                .iter()
                .any(|warning| warning.code == "approximate_probability_input")
        );
    }

    #[test]
    fn probability_rejects_inverted_bounds() {
        let error = call(
            "verify.probability",
            serde_json::json!({"value": 0.5, "lower": 1, "upper": 0}),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::DomainViolation);
    }

    /// Field-wise example comparison: every listed field must match, extra
    /// result fields are allowed.
    fn example_value_matches(expected: &Value, actual: &Value) -> bool {
        match (expected, actual) {
            (Value::Record(expected_fields), Value::Record(actual_fields)) => {
                expected_fields.iter().all(|(key, expected_value)| {
                    actual_fields.get(key).is_some_and(|actual_value| {
                        example_value_matches(expected_value, actual_value)
                    })
                })
            }
            (Value::Array(expected_items), Value::Array(actual_items)) => {
                expected_items.len() == actual_items.len()
                    && expected_items.iter().zip(actual_items).all(
                        |(expected_item, actual_item)| {
                            example_value_matches(expected_item, actual_item)
                        },
                    )
            }
            _ => expected == actual,
        }
    }

    #[test]
    fn every_documented_example_executes() {
        let module = module();
        for function in &module.functions {
            let descriptor = function.descriptor();
            assert!(
                !descriptor.examples.is_empty(),
                "{} must document at least one executable example",
                descriptor.id
            );
            for example in &descriptor.examples {
                let raw = serde_json::to_value(&example.arguments).expect("arguments serialize");
                let object = raw.as_object().expect("arguments are a record");
                let mut values = BTreeMap::new();
                for (name, value) in object {
                    let param = descriptor.parameter(name).expect("parameter exists");
                    let coerced = param
                        .schema
                        .coerce(value, name, &ctx().limits, param.numeric_shorthand)
                        .unwrap_or_else(|error| {
                            panic!("{} example {:?}: {error}", descriptor.id, example.title)
                        });
                    values.insert(name.clone(), coerced);
                }
                let result = function.invoke(&Args::new(values), &ctx());
                match &example.expected {
                    Some(ExampleExpectation::Value(expected)) => {
                        let outcome = result.unwrap_or_else(|error| {
                            panic!("{} example {:?}: {error}", descriptor.id, example.title)
                        });
                        assert!(
                            example_value_matches(expected, &outcome.value),
                            "{} example {:?}: expected {expected:?}, got {:?}",
                            descriptor.id,
                            example.title,
                            outcome.value
                        );
                    }
                    Some(ExampleExpectation::Error(code)) => {
                        let error = result.expect_err("example must fail");
                        assert_eq!(
                            error.code, *code,
                            "{} example {:?}",
                            descriptor.id, example.title
                        );
                    }
                    Some(ExampleExpectation::Contains(needle)) => {
                        let outcome = result.expect("example must succeed");
                        let rendered = serde_json::to_string(&outcome.value).expect("serializes");
                        assert!(
                            rendered.contains(needle),
                            "{} example {:?}",
                            descriptor.id,
                            example.title
                        );
                    }
                    None => {}
                }
            }
        }
    }
}
