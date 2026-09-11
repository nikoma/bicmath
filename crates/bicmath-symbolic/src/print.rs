//! Precedence-aware printing of the restricted expression grammar.
//!
//! The printer emits the minimal parentheses required by the parser's
//! precedence and associativity, so `parse(print(e))` denotes the same
//! expression as `e`. Negative literals and rational literals are treated as
//! operators for precedence purposes because their printed form contains `-`
//! or `/`.

use bicmath_core::context::ExecContext;
use bicmath_core::error::EngineError;
use bicmath_core::expr::{BinaryOp, CompareOp, Expr, UnaryOp};
use bicmath_core::number::Number;

use crate::ast::Budget;

const PREC_OR: u8 = 1;
const PREC_AND: u8 = 2;
const PREC_NOT: u8 = 3;
const PREC_COMPARE: u8 = 4;
const PREC_ADD: u8 = 5;
const PREC_MUL: u8 = 6;
const PREC_UNARY: u8 = 7;
const PREC_POW: u8 = 8;
const PREC_ATOM: u8 = 9;

/// Print an expression as a minimal-parenthesis string.
pub(crate) fn print_expression(expr: &Expr, ctx: &ExecContext) -> Result<String, EngineError> {
    let mut budget = Budget::new(&ctx.limits);
    let mut out = String::new();
    write_expr(expr, 0, &mut out, ctx, &mut budget)?;
    Ok(out)
}

fn precedence(expr: &Expr) -> u8 {
    match expr {
        Expr::Binary { op, .. } => match op {
            BinaryOp::Or => PREC_OR,
            BinaryOp::And => PREC_AND,
            BinaryOp::Add | BinaryOp::Sub => PREC_ADD,
            BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem => PREC_MUL,
            BinaryOp::Pow => PREC_POW,
        },
        Expr::Unary { op, .. } => match op {
            UnaryOp::Not => PREC_NOT,
            UnaryOp::Neg | UnaryOp::Pos => PREC_UNARY,
        },
        Expr::Compare { .. } => PREC_COMPARE,
        Expr::Number(number) => number_precedence(number),
        _ => PREC_ATOM,
    }
}

fn number_precedence(number: &Number) -> u8 {
    match number {
        Number::Rational(rational) if !rational.is_integer() => PREC_MUL,
        other if other.is_negative() => PREC_UNARY,
        _ => PREC_ATOM,
    }
}

fn binary_symbol(op: BinaryOp) -> (u8, &'static str) {
    match op {
        BinaryOp::Add => (PREC_ADD, "+"),
        BinaryOp::Sub => (PREC_ADD, "-"),
        BinaryOp::Mul => (PREC_MUL, "*"),
        BinaryOp::Div => (PREC_MUL, "/"),
        BinaryOp::Rem => (PREC_MUL, "%"),
        BinaryOp::Pow => (PREC_POW, "^"),
        BinaryOp::And => (PREC_AND, "and"),
        BinaryOp::Or => (PREC_OR, "or"),
    }
}

fn compare_symbol(op: CompareOp) -> &'static str {
    match op {
        CompareOp::Eq => "==",
        CompareOp::Ne => "!=",
        CompareOp::Lt => "<",
        CompareOp::Le => "<=",
        CompareOp::Gt => ">",
        CompareOp::Ge => ">=",
    }
}

fn quote(text: &str) -> Result<String, EngineError> {
    serde_json::to_string(text)
        .map_err(|error| EngineError::internal(format!("failed to quote string: {error}")))
}

fn write_expr(
    expr: &Expr,
    parent_precedence: u8,
    out: &mut String,
    ctx: &ExecContext,
    budget: &mut Budget,
) -> Result<(), EngineError> {
    budget.tick(ctx)?;
    let precedence = precedence(expr);
    let parenthesize = precedence < parent_precedence;
    if parenthesize {
        out.push('(');
    }
    match expr {
        Expr::Number(number) => out.push_str(&number.to_string()),
        Expr::Text(text) => out.push_str(&quote(text)?),
        Expr::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
        Expr::Ident(name) => out.push_str(name),
        Expr::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                write_expr(item, 0, out, ctx, budget)?;
            }
            out.push(']');
        }
        Expr::Record(fields) => {
            out.push('{');
            for (index, (key, value)) in fields.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                out.push_str(&quote(key)?);
                out.push_str(": ");
                write_expr(value, 0, out, ctx, budget)?;
            }
            out.push('}');
        }
        Expr::Unary { op, expr } => {
            let operand_precedence = match op {
                UnaryOp::Not => PREC_NOT,
                UnaryOp::Neg | UnaryOp::Pos => PREC_UNARY,
            };
            match op {
                UnaryOp::Neg => out.push('-'),
                UnaryOp::Pos => out.push('+'),
                UnaryOp::Not => out.push_str("not "),
            }
            write_expr(expr, operand_precedence, out, ctx, budget)?;
        }
        Expr::Binary { op, left, right } => {
            let (precedence, symbol) = binary_symbol(*op);
            match op {
                BinaryOp::Pow => {
                    write_expr(left, precedence + 1, out, ctx, budget)?;
                    out.push('^');
                    write_expr(right, precedence, out, ctx, budget)?;
                }
                _ => {
                    write_expr(left, precedence, out, ctx, budget)?;
                    out.push(' ');
                    out.push_str(symbol);
                    out.push(' ');
                    write_expr(right, precedence + 1, out, ctx, budget)?;
                }
            }
        }
        Expr::Compare { operands, ops } => {
            for (index, operand) in operands.iter().enumerate() {
                if index > 0 {
                    out.push(' ');
                    out.push_str(compare_symbol(ops[index - 1]));
                    out.push(' ');
                }
                write_expr(operand, PREC_COMPARE, out, ctx, budget)?;
            }
        }
        Expr::Call { name, args } => {
            out.push_str(name);
            out.push('(');
            for (index, arg) in args.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                if let Some(arg_name) = &arg.name {
                    out.push_str(arg_name);
                    out.push_str(" = ");
                }
                write_expr(&arg.value, 0, out, ctx, budget)?;
            }
            out.push(')');
        }
    }
    if parenthesize {
        out.push(')');
    }
    Ok(())
}
