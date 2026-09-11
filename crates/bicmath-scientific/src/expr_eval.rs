//! The local f64 evaluator for restricted expressions.
//!
//! Root finding and numerical integration accept an expression string that is
//! parsed by [`bicmath_core::expr::parse_expression`] and evaluated here over
//! plain `f64` values. The evaluator resolves no registry functions and no
//! engine bindings: only the supplied variable, numeric literals, parentheses,
//! the arithmetic operators `+ - * / % ^`, and a fixed set of elementary
//! functions are allowed. Unknown functions and variables are rejected.

use bicmath_core::error::EngineError;
use bicmath_core::expr::{BinaryOp, CallArg, Expr, UnaryOp};

use crate::f64math;

/// A parsed expression bound to one or two named variables.
pub(crate) struct Evaluator<'a> {
    expression: &'a Expr,
    variables: Vec<&'a str>,
    pub(crate) evaluations: u64,
}

impl<'a> Evaluator<'a> {
    pub(crate) fn new(expression: &'a Expr, variable: &'a str) -> Evaluator<'a> {
        Evaluator {
            expression,
            variables: vec![variable],
            evaluations: 0,
        }
    }

    /// Build an evaluator over two named variables, for example the independent
    /// variable `t` and the dependent variable of an ordinary differential
    /// equation.
    pub(crate) fn new_two(expression: &'a Expr, first: &'a str, second: &'a str) -> Evaluator<'a> {
        Evaluator {
            expression,
            variables: vec![first, second],
            evaluations: 0,
        }
    }

    /// Evaluate the expression at `x` for the single bound variable.
    pub(crate) fn evaluate(&mut self, x: f64) -> Result<f64, EngineError> {
        self.evaluate_bindings(&[x])
    }

    /// Evaluate the expression with one value per bound variable.
    pub(crate) fn evaluate_bindings(&mut self, values: &[f64]) -> Result<f64, EngineError> {
        self.evaluations += 1;
        eval_node(self.expression, &self.variables, values)
    }
}

/// Validate the caller-supplied variable name as an identifier.
pub(crate) fn validate_variable_name(name: &str) -> Result<(), EngineError> {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() || first == '_' => {}
        _ => {
            return Err(
                EngineError::domain("variable must be a non-empty ASCII identifier")
                    .with_path("variable".to_string()),
            );
        }
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(EngineError::domain(
            "variable may contain only ASCII letters, digits, and underscores",
        )
        .with_path("variable".to_string()));
    }
    Ok(())
}

fn describe_bindings(variables: &[&str], values: &[f64]) -> String {
    variables
        .iter()
        .zip(values.iter())
        .map(|(name, value)| format!("{name} = {value}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn eval_node(expression: &Expr, variables: &[&str], values: &[f64]) -> Result<f64, EngineError> {
    let value = match expression {
        Expr::Number(number) => number.to_f64().ok_or_else(|| {
            EngineError::domain(format!(
                "numeric literal {number} is not representable as float64"
            ))
        })?,
        Expr::Ident(name) => match variables.iter().position(|variable| variable == name) {
            Some(index) => values[index],
            None => {
                return Err(EngineError::domain(format!(
                    "unknown variable {name:?}; the bound variables are {}",
                    variables.join(", ")
                )));
            }
        },
        Expr::Unary { op, expr } => match op {
            UnaryOp::Neg => -eval_node(expr, variables, values)?,
            UnaryOp::Pos => eval_node(expr, variables, values)?,
            UnaryOp::Not => {
                return Err(EngineError::domain(
                    "boolean operators are not allowed in scientific expressions",
                ));
            }
        },
        Expr::Binary { op, left, right } => {
            let a = eval_node(left, variables, values)?;
            let b = eval_node(right, variables, values)?;
            match op {
                BinaryOp::Add => a + b,
                BinaryOp::Sub => a - b,
                BinaryOp::Mul => a * b,
                BinaryOp::Div => {
                    if b == 0.0 {
                        return Err(EngineError::division_by_zero(format!(
                            "division by zero in the expression at {}",
                            describe_bindings(variables, values)
                        )));
                    }
                    a / b
                }
                BinaryOp::Rem => {
                    if b == 0.0 {
                        return Err(EngineError::division_by_zero(format!(
                            "remainder by zero in the expression at {}",
                            describe_bindings(variables, values)
                        )));
                    }
                    a % b
                }
                BinaryOp::Pow => f64math::pow(a, b),
                BinaryOp::And | BinaryOp::Or => {
                    return Err(EngineError::domain(
                        "boolean operators are not allowed in scientific expressions",
                    ));
                }
            }
        }
        Expr::Call { name, args } => eval_call(name, args, variables, values)?,
        Expr::Compare { .. } => {
            return Err(EngineError::domain(
                "comparisons are not allowed in scientific expressions",
            ));
        }
        Expr::Bool(_) | Expr::Text(_) | Expr::Array(_) | Expr::Record(_) => {
            return Err(EngineError::domain(
                "only numeric expressions are allowed in scientific expressions",
            ));
        }
    };
    if !value.is_finite() {
        return Err(EngineError::domain(format!(
            "expression evaluated to a non-finite value at {}",
            describe_bindings(variables, values)
        )));
    }
    Ok(value)
}

fn arity(name: &str, values: &[f64], expected: usize) -> Result<(), EngineError> {
    if values.len() == expected {
        Ok(())
    } else {
        Err(EngineError::domain(format!(
            "{name} expects {expected} argument(s), found {}",
            values.len()
        )))
    }
}

fn eval_call(
    name: &str,
    args: &[CallArg],
    variables: &[&str],
    values: &[f64],
) -> Result<f64, EngineError> {
    let mut arguments = Vec::with_capacity(args.len());
    for arg in args {
        if arg.name.is_some() {
            return Err(EngineError::domain(format!(
                "named arguments are not supported in a call to {name:?}"
            )));
        }
        arguments.push(eval_node(&arg.value, variables, values)?);
    }
    let values = arguments;
    let result = match name {
        "sin" => {
            arity(name, &values, 1)?;
            f64math::sin(values[0])
        }
        "cos" => {
            arity(name, &values, 1)?;
            f64math::cos(values[0])
        }
        "tan" => {
            arity(name, &values, 1)?;
            f64math::tan(values[0])
        }
        "asin" => {
            arity(name, &values, 1)?;
            if values[0].abs() > 1.0 {
                return Err(EngineError::domain("asin requires |x| <= 1"));
            }
            f64math::asin(values[0])
        }
        "acos" => {
            arity(name, &values, 1)?;
            if values[0].abs() > 1.0 {
                return Err(EngineError::domain("acos requires |x| <= 1"));
            }
            f64math::acos(values[0])
        }
        "atan" => {
            arity(name, &values, 1)?;
            f64math::atan(values[0])
        }
        "atan2" => {
            arity(name, &values, 2)?;
            f64math::atan2(values[0], values[1])
        }
        "sinh" => {
            arity(name, &values, 1)?;
            f64math::sinh(values[0])
        }
        "cosh" => {
            arity(name, &values, 1)?;
            f64math::cosh(values[0])
        }
        "tanh" => {
            arity(name, &values, 1)?;
            f64math::tanh(values[0])
        }
        "exp" => {
            arity(name, &values, 1)?;
            f64math::exp(values[0])
        }
        "ln" => {
            arity(name, &values, 1)?;
            if values[0] <= 0.0 {
                return Err(EngineError::domain("ln requires x > 0"));
            }
            f64math::ln(values[0])
        }
        "log" => {
            arity(name, &values, 2)?;
            let (value, base) = (values[0], values[1]);
            if value <= 0.0 {
                return Err(EngineError::domain("log requires x > 0"));
            }
            if base <= 0.0 || base == 1.0 {
                return Err(EngineError::domain("log requires base > 0 and base != 1"));
            }
            f64math::ln(value) / f64math::ln(base)
        }
        "log10" => {
            arity(name, &values, 1)?;
            if values[0] <= 0.0 {
                return Err(EngineError::domain("log10 requires x > 0"));
            }
            f64math::log10(values[0])
        }
        "log2" => {
            arity(name, &values, 1)?;
            if values[0] <= 0.0 {
                return Err(EngineError::domain("log2 requires x > 0"));
            }
            f64math::log2(values[0])
        }
        "sqrt" => {
            arity(name, &values, 1)?;
            if values[0] < 0.0 {
                return Err(EngineError::domain("sqrt requires x >= 0"));
            }
            f64math::sqrt(values[0])
        }
        "cbrt" => {
            arity(name, &values, 1)?;
            f64math::cbrt(values[0])
        }
        "abs" => {
            arity(name, &values, 1)?;
            f64math::abs(values[0])
        }
        "floor" => {
            arity(name, &values, 1)?;
            f64math::floor(values[0])
        }
        "ceil" => {
            arity(name, &values, 1)?;
            f64math::ceil(values[0])
        }
        "round" => {
            arity(name, &values, 1)?;
            f64math::round(values[0])
        }
        "min" => {
            arity(name, &values, 2)?;
            if values[0] < values[1] {
                values[0]
            } else {
                values[1]
            }
        }
        "max" => {
            arity(name, &values, 2)?;
            if values[0] > values[1] {
                values[0]
            } else {
                values[1]
            }
        }
        "pow" => {
            arity(name, &values, 2)?;
            f64math::pow(values[0], values[1])
        }
        other => {
            return Err(EngineError::domain(format!(
                "unknown function {other:?} in scientific expression"
            )));
        }
    };
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bicmath_core::error::ErrorCode;
    use bicmath_core::expr::parse_expression;
    use bicmath_core::limits::Limits;

    fn evaluate(source: &str, variable: &str, x: f64) -> Result<f64, EngineError> {
        let limits = Limits::conservative();
        let expression = parse_expression(source, &limits)?;
        let mut evaluator = Evaluator::new(&expression, variable);
        evaluator.evaluate(x)
    }

    #[test]
    fn arithmetic_and_precedence() {
        assert_eq!(evaluate("2 + 3 * 4", "x", 0.0).unwrap(), 14.0);
        assert_eq!(evaluate("2^3^2", "x", 0.0).unwrap(), 512.0);
        assert_eq!(evaluate("-2^2", "x", 0.0).unwrap(), -4.0);
        assert_eq!(evaluate("x^2 - 2", "x", 3.0).unwrap(), 7.0);
    }

    #[test]
    fn elementary_calls_are_supported() {
        let value = evaluate("sin(x)^2 + cos(x)^2", "x", 1.2345).unwrap();
        assert!((value - 1.0).abs() < 1e-15);
        assert_eq!(evaluate("min(x, 2)", "x", 3.0).unwrap(), 2.0);
        assert_eq!(evaluate("pow(x, 2)", "x", 4.0).unwrap(), 16.0);
        assert_eq!(evaluate("log(x, 2)", "x", 8.0).unwrap(), 3.0);
    }

    #[test]
    fn unknown_names_are_rejected() {
        assert_eq!(
            evaluate("frobnicate(x)", "x", 1.0).unwrap_err().code,
            ErrorCode::DomainViolation
        );
        assert_eq!(
            evaluate("y + 1", "x", 1.0).unwrap_err().code,
            ErrorCode::DomainViolation
        );
    }

    #[test]
    fn two_variables_are_bound_by_name() {
        let limits = Limits::conservative();
        let expression = parse_expression("t + 2*y", &limits).unwrap();
        let mut evaluator = Evaluator::new_two(&expression, "t", "y");
        assert_eq!(evaluator.evaluate_bindings(&[1.0, 3.0]).unwrap(), 7.0);
        assert_eq!(evaluator.evaluations, 1);
        let expression = parse_expression("z", &limits).unwrap();
        let mut evaluator = Evaluator::new_two(&expression, "t", "y");
        assert_eq!(
            evaluator.evaluate_bindings(&[0.0, 0.0]).unwrap_err().code,
            ErrorCode::DomainViolation
        );
    }

    #[test]
    fn domain_violations_are_structured() {
        assert_eq!(
            evaluate("sqrt(x)", "x", -1.0).unwrap_err().code,
            ErrorCode::DomainViolation
        );
        assert_eq!(
            evaluate("1 / x", "x", 0.0).unwrap_err().code,
            ErrorCode::DivisionByZero
        );
    }

    #[test]
    fn variable_names_are_validated() {
        assert!(validate_variable_name("x").is_ok());
        assert!(validate_variable_name("theta_1").is_ok());
        assert!(validate_variable_name("").is_err());
        assert!(validate_variable_name("1x").is_err());
        assert!(validate_variable_name("a b").is_err());
    }
}
