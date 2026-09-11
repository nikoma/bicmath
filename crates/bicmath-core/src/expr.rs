//! The restricted expression grammar and parser.
//!
//! The parser produces an AST. Function resolution and evaluation live in the
//! engine crate and use exactly the same registry and validation as the typed
//! `calculate` path. Nothing here evaluates Rust, JavaScript, shell commands,
//! imports, or remote code.
//!
//! Grammar (whitespace-insensitive; `//` line comments and `/* */` block
//! comments are supported):
//!
//! ```text
//! expr        := or_expr
//! or_expr     := and_expr (('or' | '||') and_expr)*
//! and_expr    := not_expr (('and' | '&&') not_expr)*
//! not_expr    := ('not' | '!') not_expr | comparison
//! comparison  := additive (comp_op additive)+          // chained, e.g. 0 < x < 1
//! additive    := multiplicative (('+' | '-') multiplicative)*
//! multiplicative := unary (('*' | '/' | '%') unary)*
//! unary       := ('-' | '+') unary | power
//! power       := primary ('^' unary)?                  // right associative
//! primary     := NUMBER | STRING | 'true' | 'false' | IDENT
//!              | IDENT ('.' IDENT)+ '(' arguments? ')'
//!              | IDENT '(' arguments? ')'             // unqualified calls rejected
//!              | '(' expr ')' | '[' array ']' | '{' record '}'
//! arguments   := argument (',' argument)*
//! argument    := (IDENT '=')? expr
//! array       := expr (',' expr)* ','?
//! record      := (IDENT | STRING) ':' expr (',' ...)* ','?
//! ```
//!
//! Precedence (loosest to tightest): `or`, `and`, `not`, comparison,
//! `+ -`, `* / %`, unary sign, `^`. Exponentiation binds tighter than unary
//! minus, so `-2^2 == -4`, and is right associative, so `2^3^2 == 2^9`.

use std::collections::BTreeMap;

use serde_json::json;

use crate::error::{EngineError, ErrorCode};
use crate::limits::Limits;
use crate::number::Number;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Pos,
    Not,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Pow,
    And,
    Or,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompareOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CallArg {
    pub name: Option<String>,
    pub value: Expr,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Number(Number),
    Text(String),
    Bool(bool),
    Ident(String),
    Array(Vec<Expr>),
    Record(Vec<(String, Expr)>),
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Compare {
        operands: Vec<Expr>,
        ops: Vec<CompareOp>,
    },
    Call {
        name: String,
        args: Vec<CallArg>,
    },
}

impl Expr {
    /// Canonical JSON representation used for fingerprints and receipts.
    pub fn canonical_json(&self) -> serde_json::Value {
        match self {
            Expr::Number(number) => json!({
                "type": "number",
                "value": serde_json::to_value(number).unwrap_or(serde_json::Value::Null),
            }),
            Expr::Text(text) => json!({"type": "text", "value": text}),
            Expr::Bool(value) => json!({"type": "bool", "value": value}),
            Expr::Ident(name) => json!({"type": "binding", "name": name}),
            Expr::Array(items) => json!({
                "type": "array",
                "items": items.iter().map(Expr::canonical_json).collect::<Vec<_>>(),
            }),
            Expr::Record(fields) => json!({
                "type": "record",
                "fields": fields
                    .iter()
                    .map(|(k, v)| json!({"name": k, "value": v.canonical_json()}))
                    .collect::<Vec<_>>(),
            }),
            Expr::Unary { op, expr } => json!({
                "type": "unary",
                "op": format!("{op:?}").to_lowercase(),
                "expr": expr.canonical_json(),
            }),
            Expr::Binary { op, left, right } => json!({
                "type": "binary",
                "op": format!("{op:?}").to_lowercase(),
                "left": left.canonical_json(),
                "right": right.canonical_json(),
            }),
            Expr::Compare { operands, ops } => json!({
                "type": "compare",
                "ops": ops.iter().map(|op| format!("{op:?}").to_lowercase()).collect::<Vec<_>>(),
                "operands": operands.iter().map(Expr::canonical_json).collect::<Vec<_>>(),
            }),
            Expr::Call { name, args } => json!({
                "type": "call",
                "name": name,
                "args": args
                    .iter()
                    .map(|arg| json!({
                        "name": arg.name,
                        "value": arg.value.canonical_json(),
                    }))
                    .collect::<Vec<_>>(),
            }),
        }
    }

    /// Collect all binding identifiers referenced by the expression.
    pub fn collect_bindings(&self, out: &mut BTreeMap<String, ()>) {
        match self {
            Expr::Ident(name) => {
                out.insert(name.clone(), ());
            }
            Expr::Array(items) => {
                for item in items {
                    item.collect_bindings(out);
                }
            }
            Expr::Record(fields) => {
                for (_, value) in fields {
                    value.collect_bindings(out);
                }
            }
            Expr::Unary { expr, .. } => expr.collect_bindings(out),
            Expr::Binary { left, right, .. } => {
                left.collect_bindings(out);
                right.collect_bindings(out);
            }
            Expr::Compare { operands, .. } => {
                for operand in operands {
                    operand.collect_bindings(out);
                }
            }
            Expr::Call { args, .. } => {
                for arg in args {
                    arg.value.collect_bindings(out);
                }
            }
            Expr::Number(_) | Expr::Text(_) | Expr::Bool(_) => {}
        }
    }

    pub fn calls(&self, out: &mut Vec<String>) {
        match self {
            Expr::Call { name, args } => {
                out.push(name.clone());
                for arg in args {
                    arg.value.calls(out);
                }
            }
            Expr::Array(items) => {
                for item in items {
                    item.calls(out);
                }
            }
            Expr::Record(fields) => {
                for (_, value) in fields {
                    value.calls(out);
                }
            }
            Expr::Unary { expr, .. } => expr.calls(out),
            Expr::Binary { left, right, .. } => {
                left.calls(out);
                right.calls(out);
            }
            Expr::Compare { operands, .. } => {
                for operand in operands {
                    operand.calls(out);
                }
            }
            Expr::Number(_) | Expr::Text(_) | Expr::Bool(_) | Expr::Ident(_) => {}
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Number(String),
    Text(String),
    Ident(String),
    True,
    False,
    And,
    Or,
    Not,
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Comma,
    Colon,
    Assign,
    Dot,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Caret,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Bang,
}

struct Lexer<'a> {
    source: &'a str,
    bytes: &'a [u8],
    pos: usize,
    tokens: Vec<Token>,
    limits: &'a Limits,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str, limits: &'a Limits) -> Result<Lexer<'a>, EngineError> {
        if source.len() > limits.max_expression_length {
            return Err(EngineError::new(
                ErrorCode::ResourceLimit,
                format!(
                    "expression length {} exceeds the limit of {}",
                    source.len(),
                    limits.max_expression_length
                ),
            ));
        }
        Ok(Lexer {
            source,
            bytes: source.as_bytes(),
            pos: 0,
            tokens: Vec::new(),
            limits,
        })
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn peek2(&self) -> Option<u8> {
        self.bytes.get(self.pos + 1).copied()
    }

    fn skip_trivia(&mut self) -> Result<(), EngineError> {
        loop {
            match self.peek() {
                Some(b' ' | b'\t' | b'\n' | b'\r') => self.pos += 1,
                Some(b'/') if self.peek2() == Some(b'/') => {
                    while let Some(c) = self.peek() {
                        if c == b'\n' {
                            break;
                        }
                        self.pos += 1;
                    }
                }
                Some(b'/') if self.peek2() == Some(b'*') => {
                    self.pos += 2;
                    loop {
                        match (self.peek(), self.peek2()) {
                            (Some(b'*'), Some(b'/')) => {
                                self.pos += 2;
                                break;
                            }
                            (Some(_), _) => self.pos += 1,
                            (None, _) => {
                                return Err(EngineError::malformed(
                                    "unterminated block comment in expression",
                                ));
                            }
                        }
                    }
                }
                _ => return Ok(()),
            }
        }
    }

    fn push(&mut self, token: Token) -> Result<(), EngineError> {
        self.tokens.push(token);
        if self.tokens.len() > self.limits.max_expression_tokens {
            return Err(EngineError::new(
                ErrorCode::ResourceLimit,
                format!(
                    "expression token count exceeds the limit of {}",
                    self.limits.max_expression_tokens
                ),
            ));
        }
        Ok(())
    }

    fn lex_number(&mut self) -> Result<(), EngineError> {
        let start = self.pos;
        let mut seen_digit = false;
        while let Some(c) = self.peek() {
            match c {
                b'0'..=b'9' => {
                    seen_digit = true;
                    self.pos += 1;
                }
                b'.' => {
                    self.pos += 1;
                }
                b'e' | b'E' => {
                    self.pos += 1;
                    if matches!(self.peek(), Some(b'+' | b'-')) {
                        self.pos += 1;
                    }
                }
                _ => break,
            }
        }
        if !seen_digit {
            return Err(EngineError::malformed(format!(
                "invalid number literal at byte {start}"
            )));
        }
        let text = &self.source[start..self.pos];
        self.push(Token::Number(text.to_string()))
    }

    fn lex_ident(&mut self) -> Result<(), EngineError> {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == b'_' {
                self.pos += 1;
            } else {
                break;
            }
        }
        let text = &self.source[start..self.pos];
        let token = match text {
            "true" => Token::True,
            "false" => Token::False,
            "and" => Token::And,
            "or" => Token::Or,
            "not" => Token::Not,
            _ => Token::Ident(text.to_string()),
        };
        self.push(token)
    }

    fn lex_string(&mut self) -> Result<(), EngineError> {
        self.pos += 1; // opening quote
        let mut out = String::new();
        loop {
            match self.peek() {
                None => {
                    return Err(EngineError::malformed(
                        "unterminated string literal in expression",
                    ));
                }
                Some(b'"') => {
                    self.pos += 1;
                    break;
                }
                Some(b'\\') => {
                    self.pos += 1;
                    let escaped = self.peek().ok_or_else(|| {
                        EngineError::malformed("unterminated escape in string literal")
                    })?;
                    self.pos += 1;
                    match escaped {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'n' => out.push('\n'),
                        b't' => out.push('\t'),
                        b'r' => out.push('\r'),
                        b'b' => out.push('\u{0008}'),
                        b'f' => out.push('\u{000C}'),
                        b'u' => {
                            let hex_start = self.pos;
                            if hex_start + 4 > self.bytes.len() {
                                return Err(EngineError::malformed(
                                    "invalid unicode escape in string literal",
                                ));
                            }
                            let hex = &self.source[hex_start..hex_start + 4];
                            self.pos += 4;
                            let code = u32::from_str_radix(hex, 16).map_err(|_| {
                                EngineError::malformed("invalid unicode escape in string literal")
                            })?;
                            let ch = char::from_u32(code).ok_or_else(|| {
                                EngineError::malformed("invalid unicode scalar in string literal")
                            })?;
                            out.push(ch);
                        }
                        _ => {
                            return Err(EngineError::malformed(format!(
                                "invalid escape sequence \\{} in string literal",
                                escaped as char
                            )));
                        }
                    }
                }
                Some(_) => {
                    let ch = self.source[self.pos..].chars().next().unwrap();
                    self.pos += ch.len_utf8();
                    out.push(ch);
                }
            }
            if out.len() > self.limits.max_string_len {
                return Err(EngineError::new(
                    ErrorCode::ResourceLimit,
                    "string literal exceeds the string length limit",
                ));
            }
        }
        self.push(Token::Text(out))
    }

    fn run(mut self) -> Result<Vec<Token>, EngineError> {
        loop {
            self.skip_trivia()?;
            let Some(c) = self.peek() else { break };
            match c {
                b'0'..=b'9' | b'.' => {
                    if c == b'.' && !matches!(self.peek2(), Some(b'0'..=b'9')) {
                        self.pos += 1;
                        self.push(Token::Dot)?;
                    } else {
                        self.lex_number()?;
                    }
                }
                b'a'..=b'z' | b'A'..=b'Z' | b'_' => self.lex_ident()?,
                b'"' => self.lex_string()?,
                b'(' => {
                    self.pos += 1;
                    self.push(Token::LParen)?;
                }
                b')' => {
                    self.pos += 1;
                    self.push(Token::RParen)?;
                }
                b'[' => {
                    self.pos += 1;
                    self.push(Token::LBracket)?;
                }
                b']' => {
                    self.pos += 1;
                    self.push(Token::RBracket)?;
                }
                b'{' => {
                    self.pos += 1;
                    self.push(Token::LBrace)?;
                }
                b'}' => {
                    self.pos += 1;
                    self.push(Token::RBrace)?;
                }
                b',' => {
                    self.pos += 1;
                    self.push(Token::Comma)?;
                }
                b':' => {
                    self.pos += 1;
                    self.push(Token::Colon)?;
                }
                b'+' => {
                    self.pos += 1;
                    self.push(Token::Plus)?;
                }
                b'-' => {
                    self.pos += 1;
                    self.push(Token::Minus)?;
                }
                b'*' => {
                    self.pos += 1;
                    self.push(Token::Star)?;
                }
                b'/' => {
                    self.pos += 1;
                    self.push(Token::Slash)?;
                }
                b'%' => {
                    self.pos += 1;
                    self.push(Token::Percent)?;
                }
                b'^' => {
                    self.pos += 1;
                    self.push(Token::Caret)?;
                }
                b'=' => {
                    if self.peek2() == Some(b'=') {
                        self.pos += 2;
                        self.push(Token::Eq)?;
                    } else {
                        self.pos += 1;
                        self.push(Token::Assign)?;
                    }
                }
                b'!' => {
                    if self.peek2() == Some(b'=') {
                        self.pos += 2;
                        self.push(Token::Ne)?;
                    } else {
                        self.pos += 1;
                        self.push(Token::Bang)?;
                    }
                }
                b'<' => {
                    if self.peek2() == Some(b'=') {
                        self.pos += 2;
                        self.push(Token::Le)?;
                    } else {
                        self.pos += 1;
                        self.push(Token::Lt)?;
                    }
                }
                b'>' => {
                    if self.peek2() == Some(b'=') {
                        self.pos += 2;
                        self.push(Token::Ge)?;
                    } else {
                        self.pos += 1;
                        self.push(Token::Gt)?;
                    }
                }
                b'&' if self.peek2() == Some(b'&') => {
                    self.pos += 2;
                    self.push(Token::And)?;
                }
                b'|' if self.peek2() == Some(b'|') => {
                    self.pos += 2;
                    self.push(Token::Or)?;
                }
                other => {
                    return Err(EngineError::malformed(format!(
                        "unexpected character {:?} in expression at byte {}",
                        other as char, self.pos
                    )));
                }
            }
        }
        Ok(self.tokens)
    }
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
    limits: &'a Limits,
    depth: usize,
}

impl<'a> Parser<'a> {
    fn new(tokens: &'a [Token], limits: &'a Limits) -> Parser<'a> {
        Parser {
            tokens,
            pos: 0,
            limits,
            depth: 0,
        }
    }

    fn enter(&mut self) -> Result<(), EngineError> {
        self.depth += 1;
        if self.depth > self.limits.max_ast_depth {
            return Err(EngineError::new(
                ErrorCode::ResourceLimit,
                format!(
                    "expression nesting depth exceeds the limit of {}",
                    self.limits.max_ast_depth
                ),
            ));
        }
        Ok(())
    }

    fn exit(&mut self) {
        self.depth -= 1;
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.pos).cloned();
        if token.is_some() {
            self.pos += 1;
        }
        token
    }

    fn expect(&mut self, token: &Token, what: &str) -> Result<(), EngineError> {
        match self.next() {
            Some(found) if &found == token => Ok(()),
            Some(found) => Err(EngineError::malformed(format!(
                "expected {what}, found {found:?}"
            ))),
            None => Err(EngineError::malformed(format!(
                "expected {what}, found end of expression"
            ))),
        }
    }

    fn parse(&mut self) -> Result<Expr, EngineError> {
        let expr = self.parse_or()?;
        if self.pos != self.tokens.len() {
            return Err(EngineError::malformed(format!(
                "unexpected token {:?} after expression",
                self.tokens[self.pos]
            )));
        }
        Ok(expr)
    }

    fn parse_or(&mut self) -> Result<Expr, EngineError> {
        let mut left = self.parse_and()?;
        while matches!(self.peek(), Some(Token::Or)) {
            self.next();
            let right = self.parse_and()?;
            left = Expr::Binary {
                op: BinaryOp::Or,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, EngineError> {
        let mut left = self.parse_not()?;
        while matches!(self.peek(), Some(Token::And)) {
            self.next();
            let right = self.parse_not()?;
            left = Expr::Binary {
                op: BinaryOp::And,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_not(&mut self) -> Result<Expr, EngineError> {
        if matches!(self.peek(), Some(Token::Not | Token::Bang)) {
            self.next();
            self.enter()?;
            let expr = self.parse_not();
            self.exit();
            return Ok(Expr::Unary {
                op: UnaryOp::Not,
                expr: Box::new(expr?),
            });
        }
        self.parse_comparison()
    }

    fn parse_comparison(&mut self) -> Result<Expr, EngineError> {
        let first = self.parse_additive()?;
        let mut operands = vec![first];
        let mut ops = Vec::new();
        loop {
            let op = match self.peek() {
                Some(Token::Eq) => CompareOp::Eq,
                Some(Token::Ne) => CompareOp::Ne,
                Some(Token::Lt) => CompareOp::Lt,
                Some(Token::Le) => CompareOp::Le,
                Some(Token::Gt) => CompareOp::Gt,
                Some(Token::Ge) => CompareOp::Ge,
                _ => break,
            };
            self.next();
            let right = self.parse_additive()?;
            operands.push(right);
            ops.push(op);
        }
        if ops.is_empty() {
            Ok(operands.pop().unwrap())
        } else {
            Ok(Expr::Compare { operands, ops })
        }
    }

    fn parse_additive(&mut self) -> Result<Expr, EngineError> {
        let mut left = self.parse_multiplicative()?;
        loop {
            let op = match self.peek() {
                Some(Token::Plus) => BinaryOp::Add,
                Some(Token::Minus) => BinaryOp::Sub,
                _ => break,
            };
            self.next();
            let right = self.parse_multiplicative()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<Expr, EngineError> {
        let mut left = self.parse_unary()?;
        loop {
            let op = match self.peek() {
                Some(Token::Star) => BinaryOp::Mul,
                Some(Token::Slash) => BinaryOp::Div,
                Some(Token::Percent) => BinaryOp::Rem,
                _ => break,
            };
            self.next();
            let right = self.parse_unary()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, EngineError> {
        match self.peek() {
            Some(Token::Minus) => {
                self.next();
                self.enter()?;
                let expr = self.parse_unary();
                self.exit();
                Ok(Expr::Unary {
                    op: UnaryOp::Neg,
                    expr: Box::new(expr?),
                })
            }
            Some(Token::Plus) => {
                self.next();
                self.enter()?;
                let expr = self.parse_unary();
                self.exit();
                Ok(Expr::Unary {
                    op: UnaryOp::Pos,
                    expr: Box::new(expr?),
                })
            }
            _ => self.parse_power(),
        }
    }

    fn parse_power(&mut self) -> Result<Expr, EngineError> {
        let base = self.parse_primary()?;
        if matches!(self.peek(), Some(Token::Caret)) {
            self.next();
            self.enter()?;
            let exponent = self.parse_unary();
            self.exit();
            Ok(Expr::Binary {
                op: BinaryOp::Pow,
                left: Box::new(base),
                right: Box::new(exponent?),
            })
        } else {
            Ok(base)
        }
    }

    fn parse_primary(&mut self) -> Result<Expr, EngineError> {
        let token = self
            .next()
            .ok_or_else(|| EngineError::malformed("unexpected end of expression"))?;
        match token {
            Token::Number(text) => {
                let number = Number::parse_literal(&text, self.limits)?;
                Ok(Expr::Number(number))
            }
            Token::Text(text) => Ok(Expr::Text(text)),
            Token::True => Ok(Expr::Bool(true)),
            Token::False => Ok(Expr::Bool(false)),
            Token::Ident(name) => {
                // Qualified call: ident ('.' ident)+ '(' ...
                let mut qualified = name;
                let mut saw_dot = false;
                while matches!(self.peek(), Some(Token::Dot)) {
                    self.next();
                    match self.next() {
                        Some(Token::Ident(part)) => {
                            qualified.push('.');
                            qualified.push_str(&part);
                            saw_dot = true;
                        }
                        other => {
                            return Err(EngineError::malformed(format!(
                                "expected identifier after '.', found {other:?}"
                            )));
                        }
                    }
                }
                if matches!(self.peek(), Some(Token::LParen)) {
                    self.next();
                    let args = self.parse_call_args()?;
                    Ok(Expr::Call {
                        name: qualified,
                        args,
                    })
                } else if saw_dot {
                    Err(EngineError::malformed(format!(
                        "qualified name {qualified:?} must be called as a function"
                    )))
                } else {
                    Ok(Expr::Ident(qualified))
                }
            }
            Token::LParen => {
                self.enter()?;
                let expr = self.parse_or();
                self.exit();
                let expr = expr?;
                self.expect(&Token::RParen, "')'")?;
                Ok(expr)
            }
            Token::LBracket => {
                self.enter()?;
                let mut items = Vec::new();
                if !matches!(self.peek(), Some(Token::RBracket)) {
                    loop {
                        items.push(self.parse_or()?);
                        if matches!(self.peek(), Some(Token::Comma)) {
                            self.next();
                            if matches!(self.peek(), Some(Token::RBracket)) {
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                }
                self.exit();
                self.expect(&Token::RBracket, "']'")?;
                if items.len() > self.limits.max_array_len {
                    return Err(EngineError::new(
                        ErrorCode::ResourceLimit,
                        format!(
                            "array literal length {} exceeds the limit of {}",
                            items.len(),
                            self.limits.max_array_len
                        ),
                    ));
                }
                Ok(Expr::Array(items))
            }
            Token::LBrace => {
                self.enter()?;
                let mut fields = Vec::new();
                if !matches!(self.peek(), Some(Token::RBrace)) {
                    loop {
                        let key = match self.next() {
                            Some(Token::Ident(name)) => name,
                            Some(Token::Text(text)) => text,
                            other => {
                                return Err(EngineError::malformed(format!(
                                    "expected a record key, found {other:?}"
                                )));
                            }
                        };
                        self.expect(&Token::Colon, "':'")?;
                        let value = self.parse_or()?;
                        fields.push((key, value));
                        if matches!(self.peek(), Some(Token::Comma)) {
                            self.next();
                            if matches!(self.peek(), Some(Token::RBrace)) {
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                }
                self.exit();
                self.expect(&Token::RBrace, "'}'")?;
                Ok(Expr::Record(fields))
            }
            other => Err(EngineError::malformed(format!(
                "unexpected token {other:?} in expression"
            ))),
        }
    }

    fn parse_call_args(&mut self) -> Result<Vec<CallArg>, EngineError> {
        let mut args = Vec::new();
        let mut seen_named = false;
        if matches!(self.peek(), Some(Token::RParen)) {
            self.next();
            return Ok(args);
        }
        loop {
            self.enter()?;
            let parsed = (|| -> Result<CallArg, EngineError> {
                // Look ahead for `name =`.
                if let (Some(Token::Ident(name)), Some(Token::Assign)) =
                    (self.tokens.get(self.pos), self.tokens.get(self.pos + 1))
                {
                    let name = name.clone();
                    self.pos += 2;
                    seen_named = true;
                    let value = self.parse_or()?;
                    Ok(CallArg {
                        name: Some(name),
                        value,
                    })
                } else {
                    if seen_named {
                        return Err(EngineError::malformed(
                            "positional arguments must precede named arguments",
                        ));
                    }
                    let value = self.parse_or()?;
                    Ok(CallArg { name: None, value })
                }
            })();
            self.exit();
            args.push(parsed?);
            if args.len() > 256 {
                return Err(EngineError::new(
                    ErrorCode::ResourceLimit,
                    "too many call arguments",
                ));
            }
            match self.next() {
                Some(Token::Comma) => continue,
                Some(Token::RParen) => break,
                other => {
                    return Err(EngineError::malformed(format!(
                        "expected ',' or ')' in argument list, found {other:?}"
                    )));
                }
            }
        }
        Ok(args)
    }
}

/// Parse a restricted expression. This performs no function resolution.
pub fn parse_expression(source: &str, limits: &Limits) -> Result<Expr, EngineError> {
    let tokens = Lexer::new(source, limits)?.run()?;
    if tokens.is_empty() {
        return Err(EngineError::malformed("empty expression"));
    }
    Parser::new(&tokens, limits).parse()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(source: &str) -> Expr {
        parse_expression(source, &Limits::conservative()).unwrap()
    }

    #[test]
    fn precedence_of_unary_minus_and_power() {
        let expr = parse("-2^2");
        match expr {
            Expr::Unary {
                op: UnaryOp::Neg,
                expr,
            } => match *expr {
                Expr::Binary {
                    op: BinaryOp::Pow, ..
                } => {}
                other => panic!("expected power, got {other:?}"),
            },
            other => panic!("expected negation, got {other:?}"),
        }
    }

    #[test]
    fn power_is_right_associative() {
        let expr = parse("2^3^2");
        match expr {
            Expr::Binary {
                op: BinaryOp::Pow,
                right,
                ..
            } => match *right {
                Expr::Binary {
                    op: BinaryOp::Pow, ..
                } => {}
                other => panic!("expected nested power, got {other:?}"),
            },
            other => panic!("expected power, got {other:?}"),
        }
    }

    #[test]
    fn chained_comparison_parses() {
        let expr = parse("0 < x < 1");
        match expr {
            Expr::Compare { operands, ops } => {
                assert_eq!(operands.len(), 3);
                assert_eq!(ops, vec![CompareOp::Lt, CompareOp::Lt]);
            }
            other => panic!("expected comparison, got {other:?}"),
        }
    }

    #[test]
    fn qualified_call_with_named_arguments() {
        let expr = parse("finance.npv(rate = 0.08, cashflows = [-10000, 4000])");
        match expr {
            Expr::Call { name, args } => {
                assert_eq!(name, "finance.npv");
                assert_eq!(args.len(), 2);
                assert_eq!(args[0].name.as_deref(), Some("rate"));
            }
            other => panic!("expected call, got {other:?}"),
        }
    }

    #[test]
    fn unqualified_call_is_parsed_and_rejected_later() {
        let expr = parse("sin(1)");
        match expr {
            Expr::Call { name, .. } => assert_eq!(name, "sin"),
            other => panic!("expected call, got {other:?}"),
        }
    }

    #[test]
    fn depth_limit_is_enforced() {
        let mut limits = Limits::conservative();
        limits.max_ast_depth = 8;
        let source = "((((((((((1))))))))))";
        assert!(parse_expression(source, &limits).is_err());
    }

    #[test]
    fn record_and_array_literals() {
        let expr = parse("{a: 1, b: [2, 3,]}");
        match expr {
            Expr::Record(fields) => assert_eq!(fields.len(), 2),
            other => panic!("expected record, got {other:?}"),
        }
    }

    #[test]
    fn canonical_json_is_stable() {
        let a = parse("1 + 2 * 3");
        let b = parse("1+2*3");
        assert_eq!(a.canonical_json(), b.canonical_json());
    }
}
