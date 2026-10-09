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

/// Longest formula accepted, in bytes not counting spaces (so a formula's
/// printed form, which adds spaces, is always accepted too). Bounds the tree's
/// size and depth, so evaluating, printing or dropping it can't exhaust the
/// stack, whatever a file contains.
pub const MAX_FORMULA_LEN: usize = 4096;

/// Deepest nesting of parentheses and unary signs accepted.
pub const MAX_NESTING: usize = 64;

struct Parser {
    toks: Vec<(usize, Tok)>,
    i: usize,
    end: usize,
    depth: usize,
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
        if self.depth >= MAX_NESTING {
            return self.err("formula nested too deeply");
        }
        self.depth += 1;
        let e = self.unary_inner();
        self.depth -= 1;
        e
    }

    fn unary_inner(&mut self) -> Result<Expr, ParseError> {
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
        if src.bytes().filter(|b| !matches!(b, b' ' | b'\t')).count() > MAX_FORMULA_LEN {
            return Err(ParseError {
                pos: MAX_FORMULA_LEN,
                msg: format!("formula longer than {MAX_FORMULA_LEN} characters"),
            });
        }
        let mut p = Parser {
            toks: lex(src)?,
            i: 0,
            end: src.len(),
            depth: 0,
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

    /// This expression with every reference to `name` replaced by `value`.
    pub fn substitute(&self, name: &str, value: Dec) -> Expr {
        match self {
            // Negative as Neg(Num) so the printed text parses back to the same tree.
            Expr::Var(n) if n == name => match value.checked_neg() {
                Some(pos) if value < Dec::ZERO => Expr::Neg(Box::new(Expr::Num(pos))),
                _ => Expr::Num(value),
            },
            Expr::Num(_) | Expr::Var(_) => self.clone(),
            Expr::Neg(e) => Expr::Neg(Box::new(e.substitute(name, value))),
            Expr::Bin(op, a, b) => Expr::Bin(
                *op,
                Box::new(a.substitute(name, value)),
                Box::new(b.substitute(name, value)),
            ),
        }
    }

    fn prec(&self) -> u8 {
        match self {
            Expr::Bin(Op::Add | Op::Sub, ..) => 1,
            Expr::Bin(Op::Mul | Op::Div, ..) => 2,
            Expr::Neg(_) => 3,
            Expr::Num(n) if *n < Dec::ZERO => 3,
            Expr::Num(_) | Expr::Var(_) => 4,
        }
    }

    fn fmt_prec(&self, f: &mut fmt::Formatter<'_>, min: u8) -> fmt::Result {
        if self.prec() < min {
            f.write_str("(")?;
            self.fmt_prec(f, 0)?;
            return f.write_str(")");
        }
        match self {
            Expr::Num(n) => write!(f, "{n}"),
            Expr::Var(v) => f.write_str(v),
            Expr::Neg(e) => {
                f.write_str("-")?;
                e.fmt_prec(f, 3)
            }
            Expr::Bin(op, a, b) => {
                let p = self.prec();
                a.fmt_prec(f, p)?;
                let sym = match op {
                    Op::Add => " + ",
                    Op::Sub => " - ",
                    Op::Mul => " * ",
                    Op::Div => " / ",
                };
                f.write_str(sym)?;
                // Left-associative: a right operand of equal precedence needs parens.
                b.fmt_prec(f, p + 1)
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

/// Prints with the fewest parentheses that parse back to the same tree.
impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_prec(f, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Dec {
        s.parse().unwrap()
    }

    #[test]
    fn display_round_trips() {
        for src in [
            "1 + 2 * 3",
            "(1 + 2) * 3",
            "10 - (4 - 3)",
            "10 - 4 - 3",
            "12 / (4 / 3)",
            "-(A + B)",
            "--4",
            "A * -2",
            "2.5 * (SHOOT_DAYS + PREP) / 5",
        ] {
            let e = Expr::parse(src).unwrap();
            assert_eq!(e.to_string(), src);
            assert_eq!(Expr::parse(&e.to_string()).unwrap(), e);
        }
        assert_eq!(Expr::parse("((1))+(2*3)").unwrap().to_string(), "1 + 2 * 3");
    }

    #[test]
    fn substitute_replaces_only_that_name() {
        let e = Expr::parse("A * (B - A)").unwrap().substitute("A", d("-3"));
        assert_eq!(e.to_string(), "-3 * (B - -3)");
        assert_eq!(Expr::parse(&e.to_string()).unwrap(), e);
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
    fn hostile_input_is_rejected_not_a_crash() {
        let deep = format!("{}1{}", "(".repeat(100_000), ")".repeat(100_000));
        assert!(Expr::parse(&deep).unwrap_err().msg.contains("longer than"));
        let nested = format!("{}1{}", "(".repeat(MAX_NESTING), ")".repeat(MAX_NESTING));
        assert_eq!(
            Expr::parse(&nested).unwrap_err().msg,
            "formula nested too deeply"
        );
        let ok = format!(
            "{}1{}",
            "(".repeat(MAX_NESTING - 1),
            ")".repeat(MAX_NESTING - 1)
        );
        assert_eq!(eval(&ok), Ok(d("1")));
        assert!(Expr::parse(&"-".repeat(500)).is_err());
        // The longest allowed chain still evaluates and prints fine.
        let chain = vec!["1"; MAX_FORMULA_LEN / 2].join("+");
        let e = Expr::parse(&chain).unwrap();
        assert_eq!(
            e.eval(&|_| None),
            Ok(Dec::from_int((MAX_FORMULA_LEN / 2) as i64))
        );
        assert_eq!(Expr::parse(&e.to_string()).unwrap(), e);
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
