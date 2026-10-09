//! Tiny arithmetic expressions for amount, multiplier and rate fields.
//!
//! Grammar: numbers, global names, `+ - * /`, unary minus and parentheses.
//! Parsed into an AST and walked; nothing is ever `eval`ed as code.

use std::collections::BTreeSet;
use std::fmt;

use crate::decimal::{Dec, ParseDecError};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr {
    Num(Dec),
    Var(String),
    Neg(Box<Expr>),
    Bin(Op, Box<Expr>, Box<Expr>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
}

/// Why an expression failed to parse. `pos` is a byte offset into the source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub pos: usize,
    pub msg: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at position {}", self.msg, self.pos)
    }
}

impl std::error::Error for ParseError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EvalError {
    UnknownGlobal(String),
    DivideByZero,
    Overflow,
}

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EvalError::UnknownGlobal(n) => write!(f, "unknown global `{n}`"),
            EvalError::DivideByZero => f.write_str("divide by zero"),
            EvalError::Overflow => f.write_str("number too large"),
        }
    }
}

impl std::error::Error for EvalError {}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(Dec),
    Ident(String),
    Op(Op),
    LParen,
    RParen,
}

/// True for names usable as globals: `[A-Za-z_][A-Za-z0-9_]*`.
pub fn is_ident(s: &str) -> bool {
    let mut b = s.bytes();
    matches!(b.next(), Some(c) if c.is_ascii_alphabetic() || c == b'_')
        && b.all(|c| c.is_ascii_alphanumeric() || c == b'_')
}

fn lex(src: &str) -> Result<Vec<(usize, Tok)>, ParseError> {
    let bytes = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        let start = i;
        let tok = match c {
            b' ' | b'\t' => {
                i += 1;
                continue;
            }
            b'+' => Tok::Op(Op::Add),
            b'-' => Tok::Op(Op::Sub),
            b'*' => Tok::Op(Op::Mul),
            b'/' => Tok::Op(Op::Div),
            b'(' => Tok::LParen,
            b')' => Tok::RParen,
            b'0'..=b'9' | b'.' => {
                while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                    i += 1;
                }
                let n = src[start..i].parse::<Dec>().map_err(|e| ParseError {
                    pos: start,
                    msg: match e {
                        ParseDecError::Invalid => "malformed number".into(),
                        other => other.to_string(),
                    },
                })?;
                out.push((start, Tok::Num(n)));
                continue;
            }
            c if c.is_ascii_alphabetic() || c == b'_' => {
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                out.push((start, Tok::Ident(src[start..i].to_string())));
                continue;
            }
            _ => {
                let ch = src[i..].chars().next().unwrap_or('?');
                return Err(ParseError {
                    pos: i,
                    msg: format!("unexpected `{ch}`"),
                });
            }
        };
        out.push((start, tok));
        i += 1;
    }
    Ok(out)
}

struct Parser {
    toks: Vec<(usize, Tok)>,
    i: usize,
    end: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.i).map(|(_, t)| t)
    }

    fn pos(&self) -> usize {
        self.toks.get(self.i).map_or(self.end, |(p, _)| *p)
    }

    fn err<T>(&self, msg: &str) -> Result<T, ParseError> {
        Err(ParseError {
            pos: self.pos(),
            msg: msg.into(),
        })
    }

    fn expr(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.term()?;
        while let Some(Tok::Op(op @ (Op::Add | Op::Sub))) = self.peek().cloned() {
            self.i += 1;
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(self.term()?));
        }
        Ok(lhs)
    }

    fn term(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.unary()?;
        while let Some(Tok::Op(op @ (Op::Mul | Op::Div))) = self.peek().cloned() {
            self.i += 1;
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(self.unary()?));
        }
        Ok(lhs)
    }

    fn unary(&mut self) -> Result<Expr, ParseError> {
        match self.peek() {
            Some(Tok::Op(Op::Sub)) => {
                self.i += 1;
                Ok(Expr::Neg(Box::new(self.unary()?)))
            }
            Some(Tok::Op(Op::Add)) => {
                self.i += 1;
                self.unary()
            }
            _ => self.primary(),
        }
    }

    fn primary(&mut self) -> Result<Expr, ParseError> {
        let Some(tok) = self.peek().cloned() else {
            return self.err("expected a number or name");
        };
        self.i += 1;
        match tok {
            Tok::Num(n) => Ok(Expr::Num(n)),
            Tok::Ident(name) => Ok(Expr::Var(name)),
            Tok::LParen => {
                let inner = self.expr()?;
                if self.peek() != Some(&Tok::RParen) {
                    return self.err("expected `)`");
                }
                self.i += 1;
                Ok(inner)
            }
            _ => {
                self.i -= 1;
                self.err("expected a number or name")
            }
        }
    }
}

impl Expr {
    pub fn parse(src: &str) -> Result<Expr, ParseError> {
        let mut p = Parser {
            toks: lex(src)?,
            i: 0,
            end: src.len(),
        };
        let e = p.expr()?;
        if p.i != p.toks.len() {
            return p.err("unexpected token");
        }
        Ok(e)
    }

    /// Global names this expression refers to.
    pub fn vars(&self, out: &mut BTreeSet<String>) {
        match self {
            Expr::Num(_) => {}
            Expr::Var(n) => {
                out.insert(n.clone());
            }
            Expr::Neg(e) => e.vars(out),
            Expr::Bin(_, a, b) => {
                a.vars(out);
                b.vars(out);
            }
        }
    }

    pub fn eval(&self, lookup: &dyn Fn(&str) -> Option<Dec>) -> Result<Dec, EvalError> {
        match self {
            Expr::Num(n) => Ok(*n),
            Expr::Var(n) => lookup(n).ok_or_else(|| EvalError::UnknownGlobal(n.clone())),
            Expr::Neg(e) => e.eval(lookup)?.checked_neg().ok_or(EvalError::Overflow),
            Expr::Bin(op, a, b) => {
                let (a, b) = (a.eval(lookup)?, b.eval(lookup)?);
                match op {
                    Op::Add => a.checked_add(b),
                    Op::Sub => a.checked_sub(b),
                    Op::Mul => a.checked_mul(b),
                    Op::Div if b.is_zero() => return Err(EvalError::DivideByZero),
                    Op::Div => a.checked_div(b),
                }
                .ok_or(EvalError::Overflow)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Dec {
        s.parse().unwrap()
    }

    fn eval(src: &str) -> Result<Dec, EvalError> {
        let lookup = |n: &str| match n {
            "SHOOT_DAYS" => Some(d("30")),
            "PREP" => Some(d("2.5")),
            _ => None,
        };
        Expr::parse(src).unwrap().eval(&lookup)
    }

    #[test]
    fn precedence_and_parens() {
        assert_eq!(eval("1 + 2 * 3"), Ok(d("7")));
        assert_eq!(eval("(1 + 2) * 3"), Ok(d("9")));
        assert_eq!(eval("10 - 4 - 3"), Ok(d("3")));
        assert_eq!(eval("12 / 4 / 3"), Ok(d("1")));
        assert_eq!(eval("-2 * -3"), Ok(d("6")));
        assert_eq!(eval("--4"), Ok(d("4")));
    }

    #[test]
    fn globals() {
        assert_eq!(eval("SHOOT_DAYS + PREP"), Ok(d("32.5")));
        assert_eq!(eval("SHOOT_DAYS / 5"), Ok(d("6")));
        assert_eq!(eval("NOPE"), Err(EvalError::UnknownGlobal("NOPE".into())));
    }

    #[test]
    fn divide_by_zero() {
        assert_eq!(eval("1 / (PREP - 2.5)"), Err(EvalError::DivideByZero));
    }

    #[test]
    fn parse_errors_have_positions() {
        assert_eq!(Expr::parse("").unwrap_err().pos, 0);
        assert_eq!(Expr::parse("1 +").unwrap_err().pos, 3);
        assert_eq!(Expr::parse("(1 + 2").unwrap_err().msg, "expected `)`");
        assert_eq!(Expr::parse("2 $ 3").unwrap_err().pos, 2);
        assert_eq!(Expr::parse("1 2").unwrap_err().pos, 2);
        assert_eq!(Expr::parse("1..2").unwrap_err().msg, "malformed number");
    }

    #[test]
    fn collects_vars() {
        let mut v = BTreeSet::new();
        Expr::parse("A * (B + A) - 3").unwrap().vars(&mut v);
        assert_eq!(v.into_iter().collect::<Vec<_>>(), ["A", "B"]);
    }

    #[test]
    fn ident_rule() {
        assert!(is_ident("SHOOT_DAYS"));
        assert!(is_ident("_x1"));
        assert!(!is_ident("1x"));
        assert!(!is_ident(""));
        assert!(!is_ident("a-b"));
    }
}
