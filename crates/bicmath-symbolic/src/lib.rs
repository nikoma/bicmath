//! Symbolic module: an exact-first symbolic layer over the restricted
//! expression grammar.
//!
//! This is deliberately not a general computer algebra system. It reuses
//! [`bicmath_core::expr::Expr`] and the shared parser, and offers:
//!
//! - [`derivative`](docs/methods/symbolic.md#derivative): sum, product,
//!   quotient, power, and chain rules for `+ - * / ^`, unary sign, and the
//!   supported calls. `pi` and `e` are constants unless they are the
//!   differentiation variable.
//! - [`simplify`](docs/methods/symbolic.md#simplify): exact numeric constant
//!   folding and identity elimination (`x + 0`, `x * 1`, `x * 0`, `x ^ 1`,
//!   `x ^ 0`, `x / 1`, `--x`, `x - x`, and `x / x` for nonzero literals).
//! - [`taylor`](docs/methods/symbolic.md#taylor): coefficients
//!   `f^(k)(at) / k!`, exact rationals when possible and binary64 otherwise.
//! - [`print`](docs/methods/symbolic.md#print): precedence-aware minimal
//!   parentheses that round-trip through the parser.
//! - [`substitute`](docs/methods/symbolic.md#substitute) and
//!   [`free_variables`](docs/methods/symbolic.md#free_variables).
//!
//! Every function is pure and deterministic, respects the execution limits, and
//! returns structured errors for unknown functions and variables. No unsafe
//! code, filesystem, network, or clock access is used.

#![forbid(unsafe_code)]

mod ast;
mod differentiate;
mod evaluate;
mod print;
mod simplify;
mod taylor;

use std::collections::BTreeMap;
use std::sync::Arc;

use num_traits::ToPrimitive;
use serde_json::json;

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, CostClass, Example, FunctionDescriptor, Module, ModuleDescriptor, Outcome,
    ParamDescriptor, SimpleFunction, Warning,
};
use bicmath_core::envelope::Exactness;
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::expr::{Expr, parse_expression};
use bicmath_core::number::{Number, NumericMode};
use bicmath_core::schema::{FieldSchema, NumberKind, ValueSchema};
use bicmath_core::value::Value;

const MODULE: &str = "symbolic";
const VERSION: &str = "1.0.0";

const UNITS_RULE: &str =
    "The expression must be a dimensionless real-valued expression; variables carry no units.";

fn all_modes() -> Vec<NumericMode> {
    vec![
        NumericMode::Exact,
        NumericMode::Auto,
        NumericMode::Scientific,
    ]
}

fn number_schema() -> ValueSchema {
    ValueSchema::number(NumberKind::Any)
}

fn integer_schema() -> ValueSchema {
    ValueSchema::number(NumberKind::Integer)
}

fn text_schema() -> ValueSchema {
    ValueSchema::text()
}

fn expression_schema() -> ValueSchema {
    ValueSchema::Expression {
        variables: vec!["x".to_string()],
    }
}

fn record_schema(fields: Vec<FieldSchema>) -> ValueSchema {
    ValueSchema::Record {
        fields,
        allow_extra: false,
    }
}

fn field(name: &str, schema: ValueSchema) -> FieldSchema {
    FieldSchema::required(name, schema)
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

fn expression_argument(args: &Args, name: &str, ctx: &ExecContext) -> Result<Expr, EngineError> {
    let source = args.text(name)?;
    parse_expression(source, &ctx.limits).map_err(|error| error.with_path(name.to_string()))
}

fn variable_argument(args: &Args, name: &str) -> Result<String, EngineError> {
    let variable = args.text(name)?;
    ast::validate_variable_name(variable)?;
    Ok(variable.to_string())
}

fn order_argument(
    args: &Args,
    name: &str,
    default: u32,
    ctx: &ExecContext,
) -> Result<u32, EngineError> {
    match args.optional_integer(name)? {
        None => Ok(default),
        Some(value) => {
            let order = value.to_u32().ok_or_else(|| {
                EngineError::domain(format!("{name} must be a non-negative integer"))
            })?;
            if u64::from(order) > ctx.limits.max_series_terms {
                return Err(EngineError::new(
                    ErrorCode::ResourceLimit,
                    format!(
                        "{name} {order} exceeds the series term limit of {}",
                        ctx.limits.max_series_terms
                    ),
                ));
            }
            Ok(order)
        }
    }
}

fn validate_free_variables(
    expression: &Expr,
    variable: &str,
    ctx: &ExecContext,
) -> Result<(), EngineError> {
    for name in ast::collect_free_variables(expression, ctx)? {
        if name != variable {
            return Err(EngineError::new(
                ErrorCode::NotFound,
                format!(
                    "unknown variable {name:?}; the expression may only use {variable:?}, pi, and e"
                ),
            )
            .with_path("expression".to_string()));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Invocations
// ---------------------------------------------------------------------------

fn invoke_derivative(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let expression = expression_argument(args, "expression", ctx)?;
    let variable = variable_argument(args, "variable")?;
    let order = order_argument(args, "order", 1, ctx)?;
    ast::validate_numeric_expression(&expression, ctx)?;
    ast::validate_calls(&expression, ctx)?;
    let mut current = expression;
    for _ in 0..order {
        ctx.check()?;
        current = differentiate::differentiate(&current, &variable, ctx)?;
    }
    let simplified = simplify::simplify(&current, ctx)?;
    Ok(Outcome::exact(Value::text(print::print_expression(
        &simplified,
        ctx,
    )?)))
}

fn invoke_simplify(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let expression = expression_argument(args, "expression", ctx)?;
    let simplified = simplify::simplify(&expression, ctx)?;
    Ok(Outcome::exact(Value::text(print::print_expression(
        &simplified,
        ctx,
    )?)))
}

fn invoke_taylor(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let expression = expression_argument(args, "expression", ctx)?;
    let variable = variable_argument(args, "variable")?;
    let at = args.number("at")?.clone();
    let order = args.u32_param("order")?;
    if u64::from(order) > ctx.limits.max_series_terms {
        return Err(EngineError::new(
            ErrorCode::ResourceLimit,
            format!(
                "order {order} exceeds the series term limit of {}",
                ctx.limits.max_series_terms
            ),
        ));
    }
    ast::validate_numeric_expression(&expression, ctx)?;
    ast::validate_calls(&expression, ctx)?;
    validate_free_variables(&expression, &variable, ctx)?;

    let expansion = taylor::taylor(&expression, &variable, &at, order, ctx)?;
    let coefficients = Value::Array(
        expansion
            .coefficients
            .iter()
            .cloned()
            .map(Value::Number)
            .collect(),
    );
    let polynomial = print::print_expression(&expansion.polynomial, ctx)?;
    let value = Value::record([
        ("polynomial", Value::text(polynomial)),
        ("coefficients", coefficients),
        ("order", Value::Number(Number::integer(order))),
        ("at", Value::Number(at)),
        ("method", Value::text("taylor")),
        ("approximate", Value::Bool(expansion.approximate)),
    ]);
    let exactness = if expansion.approximate {
        Exactness::Approximate
    } else {
        Exactness::Exact
    };
    let mut outcome = Outcome::new(value, exactness);
    if expansion.approximate {
        outcome = outcome.with_warning(Warning::new(
            "taylor_approximate",
            "the expansion point has no exact rational value at this order; \
             coefficients are float64 approximations",
        ));
    }
    Ok(outcome)
}

fn invoke_print(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let expression = expression_argument(args, "expression", ctx)?;
    Ok(Outcome::exact(Value::text(print::print_expression(
        &expression,
        ctx,
    )?)))
}

fn invoke_substitute(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let expression = expression_argument(args, "expression", ctx)?;
    let variable = variable_argument(args, "variable")?;
    let replacement = expression_argument(args, "replacement", ctx)?;
    let substituted = ast::substitute(&expression, &variable, &replacement, ctx)?;
    Ok(Outcome::exact(Value::text(print::print_expression(
        &substituted,
        ctx,
    )?)))
}

fn invoke_free_variables(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let expression = expression_argument(args, "expression", ctx)?;
    let names = ast::collect_free_variables(&expression, ctx)?;
    Ok(Outcome::exact(Value::Array(
        names.into_iter().map(Value::text).collect(),
    )))
}

// ---------------------------------------------------------------------------
// Descriptors
// ---------------------------------------------------------------------------

fn derivative_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "symbolic.derivative",
        MODULE,
        VERSION,
        "Derivative",
        "Differentiate a restricted expression with respect to one variable.",
    )
    .with_description(
        "Applies the sum, product, quotient, power, and chain rules. Supported calls are \
         sin, cos, tan, asin, acos, atan, sinh, cosh, tanh, exp, ln, log(value, base), \
         log10, log2, sqrt, abs, and pow. `pi` and `e` are constants unless they are the \
         differentiation variable. The result is simplified and printed in the restricted \
         expression syntax.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "expression",
            "Restricted numeric expression to differentiate.",
            expression_schema(),
        ),
        ParamDescriptor::required(
            "variable",
            "Variable to differentiate with respect to.",
            text_schema(),
        ),
        ParamDescriptor::optional(
            "order",
            "Derivative order; defaults to 1. Order 0 returns the simplified input.",
            integer_schema(),
        ),
    ])
    .with_output(
        text_schema(),
        "The simplified derivative, printed with minimal parentheses.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Iterative)
    .with_units_rule(UNITS_RULE)
    .with_method_ref("docs/methods/symbolic.md#derivative")
    .with_examples(vec![
        Example::new(
            "first derivative of x squared",
            example_args(&[("expression", json!("x^2")), ("variable", json!("x"))]),
        )
        .with_value(Value::text("2 * x")),
        Example::new(
            "unknown function is rejected",
            example_args(&[("expression", json!("nope(x)")), ("variable", json!("x"))]),
        )
        .with_error(ErrorCode::UnknownFunction),
    ])
    .with_tags(["calculus", "differentiation"])
}

fn simplify_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "symbolic.simplify",
        MODULE,
        VERSION,
        "Simplify",
        "Fold exact constants and eliminate arithmetic identities.",
    )
    .with_description(
        "Only exact numeric operations are folded, so simplification never introduces a \
         binary64 approximation. Identities include x + 0, x * 1, x * 0, x ^ 1, x ^ 0, \
         x / 1, --x, x - x, and x / x for nonzero literals. Products and sums are \
         normalized so that numeric coefficients are combined.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "expression",
        "Restricted numeric expression to simplify.",
        expression_schema(),
    )])
    .with_output(
        text_schema(),
        "The simplified expression, printed with minimal parentheses.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_units_rule(UNITS_RULE)
    .with_method_ref("docs/methods/symbolic.md#simplify")
    .with_examples(vec![
        Example::new(
            "identity elimination",
            example_args(&[("expression", json!("0*x + x*1 + 2 - 2"))]),
        )
        .with_value(Value::text("x")),
        Example::new(
            "exact constant folding",
            example_args(&[("expression", json!("(2 + 3) * 4"))]),
        )
        .with_value(Value::text("20")),
    ])
    .with_tags(["simplification", "algebra"])
}

fn taylor_output_schema() -> ValueSchema {
    record_schema(vec![
        field("polynomial", text_schema()),
        field("coefficients", ValueSchema::array(number_schema())),
        field("order", integer_schema()),
        field("at", number_schema()),
        field("method", text_schema()),
        field("approximate", ValueSchema::Bool),
    ])
}

fn taylor_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "symbolic.taylor",
        MODULE,
        VERSION,
        "Taylor expansion",
        "Taylor coefficients f^(k)(at) / k! for k = 0..=order.",
    )
    .with_description(
        "Each derivative is evaluated exactly first. When every coefficient has an exact \
         rational value the result is exact; otherwise the expansion falls back to binary64 \
         and is labelled approximate with a warning. In exact mode an approximate expansion \
         is rejected.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "expression",
            "Restricted numeric expression to expand.",
            expression_schema(),
        ),
        ParamDescriptor::required(
            "variable",
            "Variable the expansion is taken in.",
            text_schema(),
        ),
        ParamDescriptor::required("at", "Expansion point.", number_schema()),
        ParamDescriptor::required(
            "order",
            "Highest derivative order; must be a non-negative integer.",
            integer_schema(),
        ),
    ])
    .with_output(
        taylor_output_schema(),
        "Polynomial text, coefficient array, order, expansion point, method, and the \
         approximate flag.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Iterative)
    .with_units_rule(UNITS_RULE)
    .with_method_ref("docs/methods/symbolic.md#taylor")
    .with_examples(vec![
        Example::new(
            "exponential series at zero",
            example_args(&[
                ("expression", json!("exp(x)")),
                ("variable", json!("x")),
                ("at", json!(0)),
                ("order", json!(4)),
            ]),
        )
        .with_value(parse_value(json!({
            "polynomial": "1 + x + 1/2 * x^2 + 1/6 * x^3 + 1/24 * x^4",
            "coefficients": [
                {"kind": "integer", "value": "1"},
                {"kind": "integer", "value": "1"},
                {"kind": "rational", "numerator": "1", "denominator": "2"},
                {"kind": "rational", "numerator": "1", "denominator": "6"},
                {"kind": "rational", "numerator": "1", "denominator": "24"}
            ],
            "order": 4,
            "at": 0,
            "method": "taylor",
            "approximate": false
        }))),
    ])
    .with_tags(["calculus", "taylor", "series"])
}

fn print_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "symbolic.print",
        MODULE,
        VERSION,
        "Print expression",
        "Print a parsed expression with minimal parentheses.",
    )
    .with_description(
        "The printer is precedence-aware: it emits exactly the parentheses required for the \
         output to parse back to the same expression, including negative and rational \
         literals.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "expression",
        "Restricted expression to print.",
        expression_schema(),
    )])
    .with_output(
        text_schema(),
        "The expression printed with minimal parentheses.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_units_rule(UNITS_RULE)
    .with_method_ref("docs/methods/symbolic.md#print")
    .with_examples(vec![
        Example::new(
            "minimal parentheses",
            example_args(&[("expression", json!("(x + 1) * 2"))]),
        )
        .with_value(Value::text("(x + 1) * 2")),
        Example::new(
            "power binds tighter than unary minus",
            example_args(&[("expression", json!("-2^2"))]),
        )
        .with_value(Value::text("-2^2")),
    ])
    .with_tags(["printing", "expression"])
}

fn substitute_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "symbolic.substitute",
        MODULE,
        VERSION,
        "Substitute",
        "Replace a variable with an expression.",
    )
    .with_description(
        "Performs a structural substitution: every reference to the variable is replaced by \
         the parsed replacement expression. The result is printed with minimal parentheses \
         and is not simplified.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "expression",
            "Restricted numeric expression.",
            expression_schema(),
        ),
        ParamDescriptor::required("variable", "Variable to replace.", text_schema()),
        ParamDescriptor::required(
            "replacement",
            "Expression substituted for the variable.",
            expression_schema(),
        ),
    ])
    .with_output(
        text_schema(),
        "The substituted expression, printed with minimal parentheses.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_units_rule(UNITS_RULE)
    .with_method_ref("docs/methods/symbolic.md#substitute")
    .with_examples(vec![
        Example::new(
            "replace x with a + 1",
            example_args(&[
                ("expression", json!("x^2 + y")),
                ("variable", json!("x")),
                ("replacement", json!("a + 1")),
            ]),
        )
        .with_value(Value::text("(a + 1)^2 + y")),
    ])
    .with_tags(["substitution", "algebra"])
}

fn free_variables_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "symbolic.free_variables",
        MODULE,
        VERSION,
        "Free variables",
        "List the free variables of an expression.",
    )
    .with_description(
        "Returns the sorted, unique variable names referenced by the expression. The \
         constants `pi` and `e` are not variables and are excluded.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "expression",
        "Restricted expression to inspect.",
        expression_schema(),
    )])
    .with_output(
        ValueSchema::array(text_schema()),
        "Sorted free variable names.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_units_rule(UNITS_RULE)
    .with_method_ref("docs/methods/symbolic.md#free_variables")
    .with_examples(vec![
        Example::new(
            "list variables",
            example_args(&[("expression", json!("x + y * 2 + pi"))]),
        )
        .with_value(Value::Array(vec![Value::text("x"), Value::text("y")])),
    ])
    .with_tags(["expression", "variables"])
}

/// Build the symbolic module with all of its registered functions.
pub fn module() -> Module {
    let functions: Vec<Arc<dyn bicmath_core::contract::Function>> = vec![
        SimpleFunction::arc(derivative_descriptor(), invoke_derivative),
        SimpleFunction::arc(simplify_descriptor(), invoke_simplify),
        SimpleFunction::arc(taylor_descriptor(), invoke_taylor),
        SimpleFunction::arc(print_descriptor(), invoke_print),
        SimpleFunction::arc(substitute_descriptor(), invoke_substitute),
        SimpleFunction::arc(free_variables_descriptor(), invoke_free_variables),
    ];
    let descriptor = ModuleDescriptor::new(
        MODULE,
        "Symbolic",
        VERSION,
        "An exact-first symbolic layer: differentiation, simplification, Taylor \
         expansion, printing, substitution, and free-variable inspection over the \
         restricted expression grammar.",
    )
    .with_capabilities(vec![
        "differentiation",
        "simplification",
        "taylor_series",
        "expression_printing",
        "substitution",
        "free_variables",
    ])
    .with_dependencies(vec!["core"])
    .with_modes(all_modes())
    .with_source("crates/bicmath-symbolic");
    Module::new(descriptor, functions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bicmath_core::contract::ExampleExpectation;
    use bicmath_core::limits::Limits;
    use num_bigint::BigInt;

    fn ctx() -> ExecContext {
        ExecContext::conservative()
    }

    fn call_with(
        id: &str,
        raw: serde_json::Value,
        ctx: &ExecContext,
    ) -> Result<Outcome, EngineError> {
        let module = module();
        let function = module
            .functions
            .iter()
            .find(|function| function.descriptor().id == id)
            .expect("function exists");
        let object = raw.as_object().expect("object arguments");
        let mut values = BTreeMap::new();
        for (name, value) in object {
            let param = function
                .descriptor()
                .parameter(name)
                .expect("parameter exists");
            values.insert(
                name.clone(),
                param
                    .schema
                    .coerce(value, name, &ctx.limits, param.numeric_shorthand)
                    .expect("argument coerces"),
            );
        }
        function.invoke(&Args::new(values), ctx)
    }

    fn call(id: &str, raw: serde_json::Value) -> Result<Outcome, EngineError> {
        call_with(id, raw, &ctx())
    }

    fn text(outcome: Outcome) -> String {
        match outcome.value {
            Value::Text(value) => value,
            other => panic!("expected text, got {other:?}"),
        }
    }

    fn integer(value: i64) -> Number {
        Number::integer(value)
    }

    fn rational(numerator: i64, denominator: i64) -> Number {
        Number::rational(BigInt::from(numerator), BigInt::from(denominator)).unwrap()
    }

    fn record(outcome: Outcome) -> BTreeMap<String, Value> {
        match outcome.value {
            Value::Record(record) => record,
            other => panic!("expected record, got {other:?}"),
        }
    }

    #[test]
    fn derivative_of_x_squared_is_two_x() {
        let outcome = call(
            "symbolic.derivative",
            json!({"expression": "x^2", "variable": "x"}),
        )
        .unwrap();
        assert_eq!(text(outcome), "2 * x");
    }

    #[test]
    fn derivative_of_sin_of_x_squared() {
        let outcome = call(
            "symbolic.derivative",
            json!({"expression": "sin(x^2)", "variable": "x"}),
        )
        .unwrap();
        assert_eq!(text(outcome), "2 * x * cos(x^2)");
    }

    #[test]
    fn derivative_of_natural_log() {
        let outcome = call(
            "symbolic.derivative",
            json!({"expression": "ln(x)", "variable": "x"}),
        )
        .unwrap();
        assert_eq!(text(outcome), "1 / x");
    }

    #[test]
    fn second_derivative_of_x_cubed_is_six_x() {
        let outcome = call(
            "symbolic.derivative",
            json!({"expression": "x^3", "variable": "x", "order": 2}),
        )
        .unwrap();
        assert_eq!(text(outcome), "6 * x");
    }

    #[test]
    fn simplify_eliminates_identities() {
        let outcome = call(
            "symbolic.simplify",
            json!({"expression": "0*x + x*1 + 2 - 2"}),
        )
        .unwrap();
        assert_eq!(text(outcome), "x");
    }

    #[test]
    fn simplify_folds_exact_constants() {
        let outcome = call("symbolic.simplify", json!({"expression": "2^10 / 4"})).unwrap();
        assert_eq!(text(outcome), "256");
    }

    #[test]
    fn taylor_exp_at_zero_is_exact() {
        let outcome = call(
            "symbolic.taylor",
            json!({"expression": "exp(x)", "variable": "x", "at": 0, "order": 4}),
        )
        .unwrap();
        assert_eq!(outcome.exactness, Exactness::Exact);
        let fields = record(outcome);
        assert_eq!(
            fields.get("polynomial"),
            Some(&Value::text("1 + x + 1/2 * x^2 + 1/6 * x^3 + 1/24 * x^4"))
        );
        assert_eq!(
            fields.get("coefficients"),
            Some(&Value::Array(vec![
                Value::Number(integer(1)),
                Value::Number(integer(1)),
                Value::Number(rational(1, 2)),
                Value::Number(rational(1, 6)),
                Value::Number(rational(1, 24)),
            ]))
        );
        assert_eq!(fields.get("approximate"), Some(&Value::Bool(false)));
        assert_eq!(fields.get("method"), Some(&Value::text("taylor")));
    }

    #[test]
    fn taylor_sin_at_zero_is_exact() {
        let outcome = call(
            "symbolic.taylor",
            json!({"expression": "sin(x)", "variable": "x", "at": 0, "order": 5}),
        )
        .unwrap();
        assert_eq!(outcome.exactness, Exactness::Exact);
        let fields = record(outcome);
        assert_eq!(
            fields.get("coefficients"),
            Some(&Value::Array(vec![
                Value::Number(integer(0)),
                Value::Number(integer(1)),
                Value::Number(integer(0)),
                Value::Number(rational(-1, 6)),
                Value::Number(integer(0)),
                Value::Number(rational(1, 120)),
            ]))
        );
        assert_eq!(
            fields.get("polynomial"),
            Some(&Value::text("x - 1/6 * x^3 + 1/120 * x^5"))
        );
    }

    #[test]
    fn taylor_without_exact_values_is_approximate() {
        let outcome = call(
            "symbolic.taylor",
            json!({"expression": "sin(x)", "variable": "x", "at": 1, "order": 3}),
        )
        .unwrap();
        assert_eq!(outcome.exactness, Exactness::Approximate);
        assert!(!outcome.warnings.is_empty());
    }

    #[test]
    fn taylor_rejects_unknown_variables() {
        let error = call(
            "symbolic.taylor",
            json!({"expression": "x + z", "variable": "x", "at": 0, "order": 1}),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::NotFound);
    }

    #[test]
    fn approximate_taylor_is_rejected_in_exact_mode() {
        let error = call_with(
            "symbolic.taylor",
            json!({"expression": "sin(x)", "variable": "x", "at": 1, "order": 3}),
            &ExecContext::exact(),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::UnsupportedNumericMode);
    }

    #[test]
    fn substitute_replaces_only_the_named_variable() {
        let outcome = call(
            "symbolic.substitute",
            json!({"expression": "x^2 + y", "variable": "x", "replacement": "a + 1"}),
        )
        .unwrap();
        assert_eq!(text(outcome), "(a + 1)^2 + y");
    }

    #[test]
    fn free_variables_excludes_constants() {
        let outcome = call(
            "symbolic.free_variables",
            json!({"expression": "x + y * 2 + pi"}),
        )
        .unwrap();
        assert_eq!(
            outcome.value,
            Value::Array(vec![Value::text("x"), Value::text("y")])
        );
    }

    #[test]
    fn print_round_trips_through_parse() {
        let limits = Limits::conservative();
        let ctx = ctx();
        let sources = [
            "x^2 + 2*x + 1",
            "(x + 1) * (x - 1)",
            "-(x + y)",
            "2^(x + 1)",
            "x^(1/2)",
            "sin(x)^2 + cos(x)^2",
            "1 / (x - 1)",
            "-2^2",
            "2^3^2",
            "x - (y - z)",
            "not (a or b)",
            "0 < x == y",
            "[1, 2, 3]",
            "{a: 1, \"b c\": x}",
            "x % 3",
            "a and b or c",
        ];
        for source in sources {
            let parsed = parse_expression(source, &limits).unwrap();
            let printed = print::print_expression(&parsed, &ctx).unwrap();
            let reparsed = parse_expression(&printed, &limits)
                .unwrap_or_else(|error| panic!("{source:?} printed as {printed:?}: {error}"));
            assert_eq!(reparsed, parsed, "{source:?} printed as {printed:?}");
            assert_eq!(
                print::print_expression(&reparsed, &ctx).unwrap(),
                printed,
                "printing is not stable for {source:?}"
            );
        }
    }

    #[test]
    fn every_function_has_an_executable_example_and_method_ref() {
        let ctx = ctx();
        for function in module().functions {
            let descriptor = function.descriptor();
            assert!(
                !descriptor.examples.is_empty(),
                "{} must document at least one executable example",
                descriptor.id
            );
            assert!(
                descriptor
                    .method_ref
                    .starts_with("docs/methods/symbolic.md#"),
                "{} has method_ref {:?}",
                descriptor.id,
                descriptor.method_ref
            );
            for example in &descriptor.examples {
                let raw = serde_json::to_value(&example.arguments).expect("arguments serialize");
                let object = raw.as_object().expect("arguments are a record");
                let mut values = BTreeMap::new();
                for (name, value) in object {
                    let param = descriptor.parameter(name).expect("parameter exists");
                    let coerced = param
                        .schema
                        .coerce(value, name, &ctx.limits, param.numeric_shorthand)
                        .unwrap_or_else(|error| {
                            panic!("{} example {:?}: {error}", descriptor.id, example.title)
                        });
                    values.insert(name.clone(), coerced);
                }
                let result = function.invoke(&Args::new(values), &ctx);
                match &example.expected {
                    Some(ExampleExpectation::Value(expected)) => {
                        let outcome = result.unwrap_or_else(|error| {
                            panic!("{} example {:?}: {error}", descriptor.id, example.title)
                        });
                        assert_eq!(
                            &outcome.value, expected,
                            "{} example {:?}",
                            descriptor.id, example.title
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

    #[test]
    fn module_identity_is_declared() {
        let module = module();
        assert_eq!(module.descriptor.id, "symbolic");
        assert_eq!(module.descriptor.version, "1.0.0");
        let ids: Vec<String> = module
            .functions
            .iter()
            .map(|function| function.descriptor().id.clone())
            .collect();
        assert_eq!(
            ids,
            vec![
                "symbolic.derivative",
                "symbolic.simplify",
                "symbolic.taylor",
                "symbolic.print",
                "symbolic.substitute",
                "symbolic.free_variables",
            ]
        );
    }
}
