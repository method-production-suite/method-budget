//! Tiny arithmetic expressions for amount, multiplier and rate fields.
//!
//! Grammar: numbers, global names, `+ - * /`, unary minus and parentheses.
//! Parsed into an AST and walked; nothing is ever `eval`ed as code.
//!
//! A global name is letters, digits and `_` with at least one letter or `_`,
//! and may start with digits, as Movie Magic allows (`1ZG`; decisions.md,
//! entry 56). The lexer reads a run of digits that runs on into a letter or
//! `_` as a name, and a run of digits and dots otherwise as a number. A
//! number is never directly followed by a name in a formula that parses
//! (there is no implied multiplication or exponent), so no formula that
//! parsed before reads differently.

use std::collections::BTreeSet;
use std::fmt;

use crate::decimal::{Dec, ParseDecError, DEC_PLACES};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr {
    Num(Literal),
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
    /// A number written with more than [`DEC_PLACES`] places, under exact
    /// arithmetic (Movie Magic arithmetic keeps them).
    TooPrecise,
}

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EvalError::UnknownGlobal(n) => write!(f, "unknown global `{n}`"),
            EvalError::DivideByZero => f.write_str("divide by zero"),
            EvalError::Overflow => f.write_str("number too large"),
            EvalError::TooPrecise => write!(
                f,
                "more than {DEC_PLACES} decimal places (exact arithmetic keeps {DEC_PLACES})"
            ),
        }
    }
}

/// A number written in a formula: its value to [`DEC_PLACES`] places, for
/// exact arithmetic, and the binary double nearest it, for Movie Magic
/// arithmetic (decisions.md, entry 55). A number with more places than
/// [`DEC_PLACES`] has only the double.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Literal {
    dec: Option<Dec>,
    /// The double's bits, so a literal compares exactly.
    bits: u64,
}

impl Literal {
    /// A number token as the lexer reads it: digits and at most one point.
    /// More places than [`DEC_PLACES`] are kept (as a double only); a
    /// number too large for a [`Dec`] is still refused.
    fn parse(s: &str) -> Result<Literal, ParseDecError> {
        let dec = match s.parse::<Dec>() {
            Ok(d) => Some(d),
            Err(ParseDecError::TooPrecise) => {
                // Too large when its first places are.
                let cut = s.find('.').map_or(s.len(), |p| p + 1 + DEC_PLACES);
                s[..cut].parse::<Dec>()?;
                None
            }
            Err(e) => return Err(e),
        };
        let f: f64 = s.parse().map_err(|_| ParseDecError::Invalid)?;
        Ok(Literal {
            dec,
            bits: f.to_bits(),
        })
    }

    /// `v` written with the shortest digits that read back as it: how a
    /// global worked out in doubles is frozen into a formula.
    pub fn from_f64(v: f64) -> Literal {
        Literal {
            dec: format!("{v}").parse().ok(),
            bits: v.to_bits(),
        }
    }

    /// The value to [`DEC_PLACES`] places; `None` when written with more.
    pub fn dec(self) -> Option<Dec> {
        self.dec
    }

    /// The binary double nearest the value.
    pub fn f64(self) -> f64 {
        f64::from_bits(self.bits)
    }

    fn is_negative(self) -> bool {
        match self.dec {
            Some(d) => d < Dec::ZERO,
            None => self.f64() < 0.0,
        }
    }
}

impl From<Dec> for Literal {
    fn from(d: Dec) -> Literal {
        Literal {
            dec: Some(d),
            bits: d.to_f64().to_bits(),
        }
    }
}

impl fmt::Display for Literal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.dec {
            Some(d) => write!(f, "{d}"),
            // The shortest digits that read back as the same double, never
            // with an exponent, so the lexer reads them.
            None => write!(f, "{}", self.f64()),
        }
    }
}

impl std::error::Error for EvalError {}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(Literal),
    Ident(String),
    Op(Op),
    LParen,
    RParen,
}

/// True for names usable as globals: letters, digits and `_`, with at least
/// one letter or `_` (so it may start with digits, `1ZG`, but `12` is a
/// number).
pub fn is_ident(s: &str) -> bool {
    !s.is_empty()
        && s.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
        && s.bytes().any(|c| c.is_ascii_alphabetic() || c == b'_')
}

/// Where the token starting at `i` (a digit, a dot, a letter or `_`) ends,
/// and whether it is a name, as the lexer splits it: digits running on into
/// a letter or `_` start a name, which runs on through letters, digits and
/// `_`; otherwise a run of digits and dots is a number.
fn token_end(bytes: &[u8], mut i: usize) -> (usize, bool) {
    let word = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    if bytes[i].is_ascii_digit() {
        let mut j = i;
        while j < bytes.len() && bytes[j].is_ascii_digit() {
            j += 1;
        }
        if j < bytes.len() && (bytes[j].is_ascii_alphabetic() || bytes[j] == b'_') {
            while j < bytes.len() && word(bytes[j]) {
                j += 1;
            }
            return (j, true);
        }
    }
    if bytes[i].is_ascii_digit() || bytes[i] == b'.' {
        while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
            i += 1;
        }
        return (i, false);
    }
    while i < bytes.len() && word(bytes[i]) {
        i += 1;
    }
    (i, true)
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
            c if c.is_ascii_alphanumeric() || c == b'_' || c == b'.' => {
                let (end, name) = token_end(bytes, i);
                i = end;
                if name {
                    out.push((start, Tok::Ident(src[start..i].to_string())));
                    continue;
                }
                let n = Literal::parse(&src[start..i]).map_err(|e| ParseError {
                    pos: start,
                    msg: match e {
                        ParseDecError::Invalid => "malformed number".into(),
                        other => other.to_string(),
                    },
                })?;
                out.push((start, Tok::Num(n)));
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

/// `src` with every global-name token equal to `from` replaced by `to`, and
/// how many were replaced. Tokens are split as the lexer splits them (a run
/// of digits and dots is a number, unless its digits run on into a letter or
/// `_`; a letter or `_` starts a name that runs on through letters, digits
/// and `_`), so `DAYS` is never touched inside `SHOOT_DAYS`, `DAYS2` or
/// `2DAYS`. Everything else is kept byte for byte, spacing
/// included, and formulas that don't parse are rewritten the same way: what
/// they name is still a reference. Parsing the result gives what
/// [`Expr::rename`] gives on the parsed original.
pub fn rename_var(src: &str, from: &str, to: &str) -> (String, usize) {
    let mut out = String::with_capacity(src.len());
    let mut n = 0;
    let mut kept = 0;
    for r in name_spans(src).filter(|r| &src[r.clone()] == from) {
        out.push_str(&src[kept..r.start]);
        out.push_str(to);
        kept = r.end;
        n += 1;
    }
    out.push_str(&src[kept..]);
    (out, n)
}

/// Where each global-name token of `src` is, split as the lexer splits them
/// (see [`rename_var`]). Anything that is neither a number nor a name is
/// stepped over a character at a time, so a formula that doesn't parse
/// still gives every name in it.
pub fn name_spans(src: &str) -> impl Iterator<Item = std::ops::Range<usize>> + '_ {
    let bytes = src.as_bytes();
    let mut i = 0;
    std::iter::from_fn(move || {
        while i < bytes.len() {
            let c = bytes[i];
            let start = i;
            if c.is_ascii_alphanumeric() || c == b'_' || c == b'.' {
                let (end, name) = token_end(bytes, i);
                i = end;
                if name {
                    return Some(start..i);
                }
            } else {
                i += src[i..].chars().next().map_or(1, char::len_utf8);
            }
        }
        None
    })
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

/// The value of a formula that is just a number (`1250.50`) or just a
/// global `lookup` knows (`SHOOT_DAYS`), the most common cells, without
/// building a parse tree. `None` for anything else, and for anything the
/// parser would reject or report, so errors always come from
/// [`Expr::parse`] and [`Expr::eval`] with their usual messages. When it
/// gives a value, it is exactly what parsing and evaluating would give.
pub fn quick_value(src: &str, lookup: &dyn Fn(&str) -> Option<Dec>) -> Option<Dec> {
    match quick_token(src)? {
        // Parsed the way the lexer parses it.
        Quick::Num(t) => Literal::parse(t).ok()?.dec(),
        Quick::Name(n) => lookup(n),
    }
}

/// [`quick_value`] for Movie Magic arithmetic: exactly what
/// [`Expr::eval_f64`] would give.
pub fn quick_value_f64(src: &str, lookup: &dyn Fn(&str) -> Option<f64>) -> Option<f64> {
    match quick_token(src)? {
        Quick::Num(t) => Literal::parse(t).ok().map(Literal::f64),
        Quick::Name(n) => lookup(n),
    }
}

/// A formula that is one number token or one name.
enum Quick<'a> {
    Num(&'a str),
    Name(&'a str),
}

fn quick_token(src: &str) -> Option<Quick<'_>> {
    // The lexer skips only spaces and tabs.
    let t = src.trim_matches([' ', '\t']);
    // Short enough that the length limit can't apply.
    if t.is_empty() || t.len() > 64 {
        return None;
    }
    if t.bytes().all(|c| c.is_ascii_digit() || c == b'.') {
        return Some(Quick::Num(t));
    }
    if is_ident(t) {
        return Some(Quick::Name(t));
    }
    None
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
        // Negative as Neg(Num) so the printed text parses back to the same tree.
        let num = match value.checked_neg() {
            Some(pos) if value < Dec::ZERO => Expr::Neg(Box::new(Expr::Num(pos.into()))),
            _ => Expr::Num(value.into()),
        };
        self.put(name, &num)
    }

    /// [`Expr::substitute`] with a double (Movie Magic arithmetic), written
    /// with the shortest digits that read back as the same double.
    pub fn substitute_f64(&self, name: &str, value: f64) -> Expr {
        let num = if value < 0.0 {
            Expr::Neg(Box::new(Expr::Num(Literal::from_f64(-value))))
        } else {
            Expr::Num(Literal::from_f64(value))
        };
        self.put(name, &num)
    }

    /// This expression with every reference to `name` replaced by `with`.
    fn put(&self, name: &str, with: &Expr) -> Expr {
        match self {
            Expr::Var(n) if n == name => with.clone(),
            Expr::Num(_) | Expr::Var(_) => self.clone(),
            Expr::Neg(e) => Expr::Neg(Box::new(e.put(name, with))),
            Expr::Bin(op, a, b) => Expr::Bin(
                *op,
                Box::new(a.put(name, with)),
                Box::new(b.put(name, with)),
            ),
        }
    }

    /// This expression with every reference to global `from` naming `to`
    /// instead: [`Expr::substitute`] with a name in place of a value.
    pub fn rename(&self, from: &str, to: &str) -> Expr {
        match self {
            Expr::Var(n) if n == from => Expr::Var(to.to_string()),
            Expr::Num(_) | Expr::Var(_) => self.clone(),
            Expr::Neg(e) => Expr::Neg(Box::new(e.rename(from, to))),
            Expr::Bin(op, a, b) => Expr::Bin(
                *op,
                Box::new(a.rename(from, to)),
                Box::new(b.rename(from, to)),
            ),
        }
    }

    fn prec(&self) -> u8 {
        match self {
            Expr::Bin(Op::Add | Op::Sub, ..) => 1,
            Expr::Bin(Op::Mul | Op::Div, ..) => 2,
            Expr::Neg(_) => 3,
            Expr::Num(n) if n.is_negative() => 3,
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
            Expr::Num(n) => n.dec().ok_or(EvalError::TooPrecise),
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

    /// The value in binary doubles, as Movie Magic arithmetic works it out
    /// (decisions.md, entry 55): each number is the double nearest what was
    /// written and each step rounds to a double. Dividing by zero, and a
    /// result past the doubles' range, are errors as in [`Expr::eval`].
    pub fn eval_f64(&self, lookup: &dyn Fn(&str) -> Option<f64>) -> Result<f64, EvalError> {
        let v = match self {
            Expr::Num(n) => n.f64(),
            Expr::Var(n) => lookup(n).ok_or_else(|| EvalError::UnknownGlobal(n.clone()))?,
            Expr::Neg(e) => -e.eval_f64(lookup)?,
            Expr::Bin(op, a, b) => {
                let (a, b) = (a.eval_f64(lookup)?, b.eval_f64(lookup)?);
                match op {
                    Op::Add => a + b,
                    Op::Sub => a - b,
                    Op::Mul => a * b,
                    Op::Div if b == 0.0 => return Err(EvalError::DivideByZero),
                    Op::Div => a / b,
                }
            }
        };
        if v.is_finite() {
            Ok(v)
        } else {
            Err(EvalError::Overflow)
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
    fn renaming_a_name_keeps_everything_else_as_typed() {
        assert_eq!(rename_var("DAYS*2", "DAYS", "SHOOT"), ("SHOOT*2".into(), 1));
        assert_eq!(
            rename_var("(DAYS+1)/ DAYS", "DAYS", "D"),
            ("(D+1)/ D".into(), 2)
        );
        // Never inside a longer name, and numbers stay numbers.
        assert_eq!(
            rename_var("SHOOT_DAYS + DAYS2 + 1.5", "DAYS", "X"),
            ("SHOOT_DAYS + DAYS2 + 1.5".into(), 0)
        );
        // A digit-led name is a name of its own; digits after a point are
        // a number, so the name after them stands alone.
        assert_eq!(rename_var("2DAYS", "DAYS", "X"), ("2DAYS".into(), 0));
        assert_eq!(
            rename_var("2DAYS * 1.5DAYS", "DAYS", "X"),
            ("2DAYS * 1.5X".into(), 1)
        );
        assert_eq!(rename_var("1ZG*1", "1ZG", "ONE"), ("ONE*1".into(), 1));
        // Formulas that don't parse are still rewritten; other text is kept.
        assert_eq!(rename_var("DAYS * é", "DAYS", "X"), ("X * é".into(), 1));
        for (src, from, to) in [
            ("A * (B - A)", "A", "LONGER_NAME"),
            ("-(A+B)/A", "B", "C"),
            ("WEEKS * 5 + WEEKS", "WEEKS", "W"),
        ] {
            let (out, _) = rename_var(src, from, to);
            assert_eq!(
                Expr::parse(&out).unwrap(),
                Expr::parse(src).unwrap().rename(from, to),
                "{src}"
            );
            // And back again, byte for byte.
            assert_eq!(rename_var(&out, to, from).0, src);
        }
        // Renaming a name to itself finds exactly the names usage counts.
        for src in ["A*(B+A)", "2A.5B__c é d1", "", "((", "x1.2.3y"] {
            for name in crate::usage::formula_names(src) {
                let count = crate::usage::formula_names(src)
                    .filter(|n| *n == name)
                    .count();
                assert_eq!(rename_var(src, name, name), (src.to_string(), count));
            }
        }
    }

    #[test]
    fn ident_rule() {
        assert!(is_ident("SHOOT_DAYS"));
        assert!(is_ident("_x1"));
        // Digit-led names, as Movie Magic allows (decisions.md, entry 56).
        assert!(is_ident("1x"));
        assert!(is_ident("1ZG"));
        assert!(is_ident("12_"));
        assert!(!is_ident("12"));
        assert!(!is_ident("1.5x"));
        assert!(!is_ident(""));
        assert!(!is_ident("a-b"));
    }
}
