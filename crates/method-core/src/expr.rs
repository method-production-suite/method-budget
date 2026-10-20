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

/// A parsed formula.
///
/// A formula may be [`MAX_FORMULA_LEN`] long, and a chain like `1+1+...+1`
/// parses into a tree as deep as it has terms, so nothing here walks the
/// tree by recursion: every walk keeps its own stack on the heap, and
/// cloning, comparing, printing (`Display` and `Debug`) and dropping are
/// written out to do the same.
#[derive(Eq)]
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
/// size, whatever a file contains. A chain like `1+1+...` is as deep as it
/// is long, so parsing, evaluating, printing, comparing, cloning and dropping
/// all keep their own stack on the heap rather than recursing (see [`Expr`]).
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

    /// The grammar, as recursive descent would read it:
    ///
    /// ```text
    /// expr    = term (("+" | "-") term)*
    /// term    = unary (("*" | "/") unary)*
    /// unary   = "-" unary | "+" unary | primary
    /// primary = number | name | "(" expr ")"
    /// ```
    ///
    /// read with a stack of what is still to do with each operand instead
    /// of recursion, so a formula nested [`MAX_NESTING`] deep needs no more
    /// stack than a flat one. Errors, and where they are, are the ones
    /// recursive descent gives.
    fn expr(&mut self) -> Result<Expr, ParseError> {
        /// What to do with an operand once it is read.
        enum Then {
            /// Add it to a sum (to `lhs` with `op`, or start one).
            Sum(Option<(Expr, Op)>),
            /// Multiply it into a product.
            Product(Option<(Expr, Op)>),
            /// Negate it.
            Neg,
            /// Expect `)` after it.
            Close,
            /// It ends a unary: one level of nesting less.
            Leave,
        }
        let mut then = vec![Then::Sum(None), Then::Product(None)];
        // Each turn reads a unary up to its operand, then hands the operand
        // up until something wants another one.
        'unary: loop {
            if self.depth >= MAX_NESTING {
                return self.err("formula nested too deeply");
            }
            self.depth += 1;
            then.push(Then::Leave);
            let mut v = match self.peek().cloned() {
                Some(Tok::Op(Op::Sub)) => {
                    self.i += 1;
                    then.push(Then::Neg);
                    continue;
                }
                Some(Tok::Op(Op::Add)) => {
                    self.i += 1;
                    continue;
                }
                Some(Tok::Num(n)) => Expr::Num(n),
                Some(Tok::Ident(name)) => Expr::Var(name),
                Some(Tok::LParen) => {
                    self.i += 1;
                    then.extend([Then::Close, Then::Sum(None), Then::Product(None)]);
                    continue;
                }
                _ => return self.err("expected a number or name"),
            };
            self.i += 1;
            loop {
                match then.pop() {
                    None => return Ok(v),
                    Some(Then::Leave) => self.depth -= 1,
                    Some(Then::Neg) => v = Expr::Neg(Box::new(v)),
                    Some(Then::Close) => {
                        if self.peek() != Some(&Tok::RParen) {
                            return self.err("expected `)`");
                        }
                        self.i += 1;
                    }
                    Some(Then::Product(lhs)) => {
                        let lhs = match lhs {
                            Some((a, op)) => Expr::Bin(op, Box::new(a), Box::new(v)),
                            None => v,
                        };
                        if let Some(&Tok::Op(op @ (Op::Mul | Op::Div))) = self.peek() {
                            self.i += 1;
                            then.push(Then::Product(Some((lhs, op))));
                            continue 'unary;
                        }
                        v = lhs;
                    }
                    Some(Then::Sum(lhs)) => {
                        let lhs = match lhs {
                            Some((a, op)) => Expr::Bin(op, Box::new(a), Box::new(v)),
                            None => v,
                        };
                        if let Some(&Tok::Op(op @ (Op::Add | Op::Sub))) = self.peek() {
                            self.i += 1;
                            then.extend([Then::Sum(Some((lhs, op))), Then::Product(None)]);
                            continue 'unary;
                        }
                        v = lhs;
                    }
                }
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
        let mut todo = vec![self];
        while let Some(e) = todo.pop() {
            match e {
                Expr::Num(_) => {}
                Expr::Var(n) => {
                    out.insert(n.clone());
                }
                Expr::Neg(e) => todo.push(e),
                Expr::Bin(_, a, b) => todo.extend([&**b, &**a]),
            }
        }
    }

    /// The tree folded from the leaves up, left operand before right, as a
    /// recursive walk would go but with its own stack: `leaf` gives each
    /// number and name its value, `neg` and `bin` combine them. Stops at
    /// the first error, so the error is the one a recursive walk with `?`
    /// would give.
    fn fold<T, E>(
        &self,
        mut leaf: impl FnMut(&Expr) -> Result<T, E>,
        mut neg: impl FnMut(T) -> Result<T, E>,
        mut bin: impl FnMut(Op, T, T) -> Result<T, E>,
    ) -> Result<T, E> {
        enum Step<'a> {
            Visit(&'a Expr),
            Neg,
            Bin(Op),
        }
        let mut todo = vec![Step::Visit(self)];
        let mut done: Vec<T> = Vec::new();
        while let Some(step) = todo.pop() {
            let v = match step {
                Step::Visit(Expr::Neg(e)) => {
                    todo.extend([Step::Neg, Step::Visit(e)]);
                    continue;
                }
                Step::Visit(Expr::Bin(op, a, b)) => {
                    todo.extend([Step::Bin(*op), Step::Visit(b), Step::Visit(a)]);
                    continue;
                }
                Step::Visit(e) => leaf(e)?,
                Step::Neg => {
                    let a = done.pop().expect("an operand for each sign");
                    neg(a)?
                }
                Step::Bin(op) => {
                    let b = done.pop().expect("a right operand for each operator");
                    let a = done.pop().expect("a left operand for each operator");
                    bin(op, a, b)?
                }
            };
            done.push(v);
        }
        Ok(done.pop().expect("a value for the whole tree"))
    }

    /// The tree rebuilt from the leaves up, each number and name replaced by
    /// what `leaf` gives for it.
    fn map_leaves(&self, mut leaf: impl FnMut(&Expr) -> Expr) -> Expr {
        let rebuilt = self.fold(
            |e| Ok::<_, std::convert::Infallible>(leaf(e)),
            |a| Ok(Expr::Neg(Box::new(a))),
            |op, a, b| Ok(Expr::Bin(op, Box::new(a), Box::new(b))),
        );
        let Ok(e) = rebuilt;
        e
    }

    /// A leaf's own copy: a number or a name. Only called on leaves.
    fn leaf_clone(&self) -> Expr {
        match self {
            Expr::Num(n) => Expr::Num(*n),
            Expr::Var(n) => Expr::Var(n.clone()),
            Expr::Neg(_) | Expr::Bin(..) => unreachable!("not a leaf"),
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
        self.map_leaves(|e| match e {
            Expr::Var(n) if n == name => with.clone(),
            e => e.leaf_clone(),
        })
    }

    /// This expression with every reference to global `from` naming `to`
    /// instead: [`Expr::substitute`] with a name in place of a value.
    pub fn rename(&self, from: &str, to: &str) -> Expr {
        self.map_leaves(|e| match e {
            Expr::Var(n) if n == from => Expr::Var(to.to_string()),
            e => e.leaf_clone(),
        })
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

    /// Prints with the fewest parentheses that parse back to the same tree:
    /// each subtree in parentheses when it binds more loosely than `min`.
    fn fmt_prec(&self, f: &mut fmt::Formatter<'_>, min: u8) -> fmt::Result {
        enum Step<'a> {
            Show(&'a Expr, u8),
            Text(&'static str),
        }
        let mut todo = vec![Step::Show(self, min)];
        while let Some(step) = todo.pop() {
            let (e, min) = match step {
                Step::Text(t) => {
                    f.write_str(t)?;
                    continue;
                }
                Step::Show(e, min) => (e, min),
            };
            if e.prec() < min {
                f.write_str("(")?;
                todo.extend([Step::Text(")"), Step::Show(e, 0)]);
                continue;
            }
            match e {
                Expr::Num(n) => write!(f, "{n}")?,
                Expr::Var(v) => f.write_str(v)?,
                Expr::Neg(x) => {
                    f.write_str("-")?;
                    todo.push(Step::Show(x, 3));
                }
                Expr::Bin(op, a, b) => {
                    let p = e.prec();
                    let sym = match op {
                        Op::Add => " + ",
                        Op::Sub => " - ",
                        Op::Mul => " * ",
                        Op::Div => " / ",
                    };
                    // Left-associative: a right operand of equal precedence needs parens.
                    todo.extend([Step::Show(b, p + 1), Step::Text(sym), Step::Show(a, p)]);
                }
            }
        }
        Ok(())
    }

    pub fn eval(&self, lookup: &dyn Fn(&str) -> Option<Dec>) -> Result<Dec, EvalError> {
        self.fold(
            |e| match e {
                Expr::Num(n) => n.dec().ok_or(EvalError::TooPrecise),
                Expr::Var(n) => lookup(n).ok_or_else(|| EvalError::UnknownGlobal(n.clone())),
                Expr::Neg(_) | Expr::Bin(..) => unreachable!("not a leaf"),
            },
            |a| a.checked_neg().ok_or(EvalError::Overflow),
            |op, a, b| {
                match op {
                    Op::Add => a.checked_add(b),
                    Op::Sub => a.checked_sub(b),
                    Op::Mul => a.checked_mul(b),
                    Op::Div if b.is_zero() => return Err(EvalError::DivideByZero),
                    Op::Div => a.checked_div(b),
                }
                .ok_or(EvalError::Overflow)
            },
        )
    }

    /// The value in binary doubles, as Movie Magic arithmetic works it out
    /// (decisions.md, entry 55): each number is the double nearest what was
    /// written and each step rounds to a double. Dividing by zero, and a
    /// result past the doubles' range, are errors as in [`Expr::eval`].
    pub fn eval_f64(&self, lookup: &dyn Fn(&str) -> Option<f64>) -> Result<f64, EvalError> {
        // Every step's value, a leaf's included, must be finite.
        let finite = |v: f64| {
            if v.is_finite() {
                Ok(v)
            } else {
                Err(EvalError::Overflow)
            }
        };
        self.fold(
            |e| match e {
                Expr::Num(n) => finite(n.f64()),
                Expr::Var(n) => {
                    finite(lookup(n).ok_or_else(|| EvalError::UnknownGlobal(n.clone()))?)
                }
                Expr::Neg(_) | Expr::Bin(..) => unreachable!("not a leaf"),
            },
            |a| finite(-a),
            |op, a, b| {
                finite(match op {
                    Op::Add => a + b,
                    Op::Sub => a - b,
                    Op::Mul => a * b,
                    Op::Div if b == 0.0 => return Err(EvalError::DivideByZero),
                    Op::Div => a / b,
                })
            },
        )
    }
}

impl Clone for Expr {
    fn clone(&self) -> Expr {
        self.map_leaves(Expr::leaf_clone)
    }
}

impl PartialEq for Expr {
    fn eq(&self, other: &Expr) -> bool {
        let mut todo = vec![(self, other)];
        while let Some(pair) = todo.pop() {
            match pair {
                (Expr::Num(a), Expr::Num(b)) if a == b => {}
                (Expr::Var(a), Expr::Var(b)) if a == b => {}
                (Expr::Neg(a), Expr::Neg(b)) => todo.push((a, b)),
                (Expr::Bin(o, a, b), Expr::Bin(p, c, d)) if o == p => {
                    todo.extend([(&**b, &**d), (&**a, &**c)]);
                }
                _ => return false,
            }
        }
        true
    }
}

/// The formula as text, `Expr(1 + 2)`: the derived form would recurse as
/// deep as the tree.
impl fmt::Debug for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Expr")
            .field(&format_args!("{self}"))
            .finish()
    }
}

impl Drop for Expr {
    /// Takes each node's children out into a list before it is freed, so
    /// freeing a deep tree never recurses more than one level.
    fn drop(&mut self) {
        // What a node's taken child leaves behind: a leaf, freed without a walk.
        const PLACEHOLDER: Literal = Literal { dec: None, bits: 0 };
        fn take(e: &mut Expr, into: &mut Vec<Expr>) {
            let mut out = |b: &mut Box<Expr>| {
                if matches!(**b, Expr::Neg(_) | Expr::Bin(..)) {
                    into.push(std::mem::replace(&mut **b, Expr::Num(PLACEHOLDER)));
                }
            };
            match e {
                Expr::Num(_) | Expr::Var(_) => {}
                Expr::Neg(a) => out(a),
                Expr::Bin(_, a, b) => {
                    out(a);
                    out(b);
                }
            }
        }
        let mut todo = Vec::new();
        take(self, &mut todo);
        while let Some(mut e) = todo.pop() {
            take(&mut e, &mut todo);
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

    /// Stack each walk over the deepest trees may use: less than a debug
    /// build needed when the walks recursed (they overflowed it), and plenty
    /// for walks that keep their own stack on the heap.
    const SMALL_STACK: usize = 64 * 1024;

    /// The deepest trees a formula may give: the longest chain of each
    /// operator (`1+1+...`, as deep as it is long), with names in it so
    /// `vars` has work, and the deepest nesting of unary minus and
    /// parentheses.
    fn deepest_formulas() -> Vec<String> {
        let terms = MAX_FORMULA_LEN / 2;
        let mut out: Vec<String> = ["+", "-", "*", "/"]
            .iter()
            .map(|op| vec!["1"; terms].join(op))
            .collect();
        // Names and numbers mixed, every operator, still the longest allowed.
        let mixed: Vec<String> = (0..MAX_FORMULA_LEN / 2)
            .map(|i| {
                format!(
                    "{}{}",
                    ["A", "1", "B", "2"][i % 4],
                    ["+", "*", "-", "/"][i % 4]
                )
            })
            .collect();
        out.push(format!("{}1", &mixed.concat()[..MAX_FORMULA_LEN - 2]));
        // The most unary minus signs that parse, minus signs with parentheses,
        // and parentheses alone: each as deep as nesting may go.
        let negs = (0..)
            .find(|&n| Expr::parse(&format!("{}1", "-".repeat(n + 1))).is_err())
            .unwrap();
        assert!(negs >= MAX_NESTING - 1);
        out.push(format!("{}1", "-".repeat(negs)));
        out.push(format!(
            "{}1{}",
            "-(".repeat(MAX_NESTING / 2 - 1),
            ")".repeat(MAX_NESTING / 2 - 1)
        ));
        out.push(format!(
            "{}1{}",
            "(".repeat(MAX_NESTING - 1),
            ")".repeat(MAX_NESTING - 1)
        ));
        for f in &out {
            assert!(f.len() <= MAX_FORMULA_LEN, "{}", f.len());
        }
        out
    }

    #[test]
    fn deepest_formulas_need_little_stack() {
        let formulas = deepest_formulas();
        let lookup = |n: &str| match n {
            "A" => Some(d("3")),
            "B" => Some(d("0.5")),
            _ => None,
        };
        let lookup_f64 = move |n: &str| lookup(n).map(Dec::to_f64);
        // What the walks give, worked out on this thread's ordinary stack.
        let expect: Vec<_> = formulas
            .iter()
            .map(|f| {
                let e = Expr::parse(f).unwrap();
                (e.eval(&lookup), e.eval_f64(&lookup_f64), e.to_string())
            })
            .collect();
        assert_eq!(expect[0].0, Ok(Dec::from_int((MAX_FORMULA_LEN / 2) as i64)));
        assert_eq!(expect[3].0, Ok(d("1")));
        std::thread::Builder::new()
            .stack_size(SMALL_STACK)
            .spawn(move || {
                for (f, (value, value_f64, text)) in formulas.iter().zip(expect) {
                    let e = Expr::parse(f).unwrap();
                    assert_eq!(e.eval(&lookup), value);
                    assert_eq!(e.eval_f64(&lookup_f64), value_f64);
                    let mut vars = BTreeSet::new();
                    e.vars(&mut vars);
                    let shown = e.to_string();
                    assert_eq!(shown, text);
                    let again = Expr::parse(&shown).unwrap();
                    assert!(again == e);
                    let copy = e.clone();
                    assert!(copy == e);
                    assert_eq!(format!("{e:?}"), format!("Expr({text})"));
                    let renamed = e.rename("A", "C").substitute("B", d("-2"));
                    assert!(renamed != e || !vars.contains("A"));
                    drop(renamed);
                    drop(copy);
                    drop(again);
                    drop(e);
                }
                // A tree dropped after a parse error part way along the chain.
                let mut bad = vec!["1"; MAX_FORMULA_LEN / 2].join("+");
                bad.push(')');
                assert!(Expr::parse(&bad).is_err());
            })
            .unwrap()
            .join()
            .expect("the walks gave the same results on a small stack");
    }

    #[test]
    fn evaluation_reports_the_first_error_left_to_right() {
        let lookup = |n: &str| match n {
            "Z" => Some(d("0")),
            "BIG" => Some(d("99999999999")),
            _ => None,
        };
        let lookup_f64 = |n: &str| match n {
            "INF" => Some(f64::INFINITY),
            n => lookup(n).map(Dec::to_f64),
        };
        let ev = |src: &str| Expr::parse(src).unwrap().eval(&lookup);
        let ev_f64 = |src: &str| Expr::parse(src).unwrap().eval_f64(&lookup_f64);
        fn unknown<T>(n: &str) -> Result<T, EvalError> {
            Err(EvalError::UnknownGlobal(n.into()))
        }
        assert_eq!(ev("NOPE + 1 / Z"), unknown("NOPE"));
        assert_eq!(ev("1 / Z + NOPE"), Err(EvalError::DivideByZero));
        assert_eq!(ev("X / Z + Y"), unknown("X"));
        assert_eq!(ev("1 / (Z * NOPE)"), unknown("NOPE"));
        assert_eq!(ev("-(A) * B"), unknown("A"));
        assert_eq!(ev("BIG * BIG * BIG + NOPE"), Err(EvalError::Overflow));
        assert_eq!(ev("0.0000000001 + NOPE"), Err(EvalError::TooPrecise));
        assert_eq!(ev("NOPE + 0.0000000001"), unknown("NOPE"));
        assert_eq!(ev_f64("NOPE + 1 / Z"), unknown("NOPE"));
        assert_eq!(ev_f64("1 / Z + NOPE"), Err(EvalError::DivideByZero));
        assert_eq!(ev_f64("INF + NOPE"), Err(EvalError::Overflow));
        assert_eq!(ev_f64("NOPE + INF"), unknown("NOPE"));
        assert_eq!(ev_f64("-INF"), Err(EvalError::Overflow));
        assert_eq!(ev_f64("0.0000000001 * 3"), Ok(0.0000000001 * 3.0));
        // Along the longest chain, the leftmost failing term is the one reported.
        // (Shorter by the names' extra letters, to stay within the limit.)
        let mut terms = vec!["1"; MAX_FORMULA_LEN / 2 - 8];
        terms[1500] = "LATER";
        terms[700] = "FIRST";
        terms[900] = "1 / Z";
        let chain = terms.join("+");
        assert_eq!(ev(&chain), unknown("FIRST"));
        assert_eq!(ev_f64(&chain), unknown("FIRST"));
        terms[700] = "1";
        let chain = terms.join("+");
        assert_eq!(ev(&chain), Err(EvalError::DivideByZero));
        assert_eq!(ev_f64(&chain), Err(EvalError::DivideByZero));
    }

    /// The tree walks as they were written recursively, the reference the
    /// walks with their own stack must match exactly.
    mod recursive {
        use super::*;

        /// [`Expr::parse`] by recursive descent, as it was written.
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
            let e = expr(&mut p)?;
            if p.i != p.toks.len() {
                return p.err("unexpected token");
            }
            Ok(e)
        }
        fn expr(p: &mut Parser) -> Result<Expr, ParseError> {
            let mut lhs = term(p)?;
            while let Some(Tok::Op(op @ (Op::Add | Op::Sub))) = p.peek().cloned() {
                p.i += 1;
                lhs = Expr::Bin(op, Box::new(lhs), Box::new(term(p)?));
            }
            Ok(lhs)
        }

        fn term(p: &mut Parser) -> Result<Expr, ParseError> {
            let mut lhs = unary(p)?;
            while let Some(Tok::Op(op @ (Op::Mul | Op::Div))) = p.peek().cloned() {
                p.i += 1;
                lhs = Expr::Bin(op, Box::new(lhs), Box::new(unary(p)?));
            }
            Ok(lhs)
        }

        fn unary(p: &mut Parser) -> Result<Expr, ParseError> {
            if p.depth >= MAX_NESTING {
                return p.err("formula nested too deeply");
            }
            p.depth += 1;
            let e = unary_inner(p);
            p.depth -= 1;
            e
        }

        fn unary_inner(p: &mut Parser) -> Result<Expr, ParseError> {
            match p.peek() {
                Some(Tok::Op(Op::Sub)) => {
                    p.i += 1;
                    Ok(Expr::Neg(Box::new(unary(p)?)))
                }
                Some(Tok::Op(Op::Add)) => {
                    p.i += 1;
                    unary(p)
                }
                _ => primary(p),
            }
        }

        fn primary(p: &mut Parser) -> Result<Expr, ParseError> {
            let Some(tok) = p.peek().cloned() else {
                return p.err("expected a number or name");
            };
            p.i += 1;
            match tok {
                Tok::Num(n) => Ok(Expr::Num(n)),
                Tok::Ident(name) => Ok(Expr::Var(name)),
                Tok::LParen => {
                    let inner = expr(p)?;
                    if p.peek() != Some(&Tok::RParen) {
                        return p.err("expected `)`");
                    }
                    p.i += 1;
                    Ok(inner)
                }
                _ => {
                    p.i -= 1;
                    p.err("expected a number or name")
                }
            }
        }

        pub fn eval(e: &Expr, lookup: &dyn Fn(&str) -> Option<Dec>) -> Result<Dec, EvalError> {
            match e {
                Expr::Num(n) => n.dec().ok_or(EvalError::TooPrecise),
                Expr::Var(n) => lookup(n).ok_or_else(|| EvalError::UnknownGlobal(n.clone())),
                Expr::Neg(e) => eval(e, lookup)?.checked_neg().ok_or(EvalError::Overflow),
                Expr::Bin(op, a, b) => {
                    let (a, b) = (eval(a, lookup)?, eval(b, lookup)?);
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

        pub fn eval_f64(e: &Expr, lookup: &dyn Fn(&str) -> Option<f64>) -> Result<f64, EvalError> {
            let v = match e {
                Expr::Num(n) => n.f64(),
                Expr::Var(n) => lookup(n).ok_or_else(|| EvalError::UnknownGlobal(n.clone()))?,
                Expr::Neg(e) => -eval_f64(e, lookup)?,
                Expr::Bin(op, a, b) => {
                    let (a, b) = (eval_f64(a, lookup)?, eval_f64(b, lookup)?);
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

        pub fn show(e: &Expr, out: &mut String, min: u8) {
            if e.prec() < min {
                out.push('(');
                show(e, out, 0);
                out.push(')');
                return;
            }
            match e {
                Expr::Num(n) => out.push_str(&n.to_string()),
                Expr::Var(v) => out.push_str(v),
                Expr::Neg(x) => {
                    out.push('-');
                    show(x, out, 3);
                }
                Expr::Bin(op, a, b) => {
                    let p = e.prec();
                    show(a, out, p);
                    out.push_str(match op {
                        Op::Add => " + ",
                        Op::Sub => " - ",
                        Op::Mul => " * ",
                        Op::Div => " / ",
                    });
                    show(b, out, p + 1);
                }
            }
        }

        pub fn vars(e: &Expr, out: &mut BTreeSet<String>) {
            match e {
                Expr::Num(_) => {}
                Expr::Var(n) => {
                    out.insert(n.clone());
                }
                Expr::Neg(e) => vars(e, out),
                Expr::Bin(_, a, b) => {
                    vars(a, out);
                    vars(b, out);
                }
            }
        }

        pub fn same(x: &Expr, y: &Expr) -> bool {
            match (x, y) {
                (Expr::Num(a), Expr::Num(b)) => a == b,
                (Expr::Var(a), Expr::Var(b)) => a == b,
                (Expr::Neg(a), Expr::Neg(b)) => same(a, b),
                (Expr::Bin(o, a, b), Expr::Bin(p, c, d)) => o == p && same(a, c) && same(b, d),
                _ => false,
            }
        }
    }

    /// A small random formula from `seed`: numbers (some negative, some
    /// with more places than a decimal keeps), names, every operator, unary
    /// signs and parentheses.
    fn random_formula(seed: &mut u64, depth: u32) -> String {
        if depth == 0 || next(seed, 3) == 0 {
            return match next(seed, 9) {
                0 => "A".into(),
                1 => "B".into(),
                2 => "Z".into(),
                3 => "NOPE".into(),
                4 => "0".into(),
                5 => "0.1".into(),
                6 => "1.0000000000001".into(),
                7 => "99999999999".into(),
                _ => format!("{}.{}", next(seed, 1000), next(seed, 100)),
            };
        }
        match next(seed, 5) {
            0 => format!("-{}", random_formula(seed, depth - 1)),
            1 => format!("({})", random_formula(seed, depth - 1)),
            _ => {
                let a = random_formula(seed, depth - 1);
                let op = ["+", "-", "*", "/"][next(seed, 4) as usize];
                format!("{a} {op} {}", random_formula(seed, depth - 1))
            }
        }
    }

    /// A number below `n` from a small linear congruential generator.
    fn next(seed: &mut u64, n: u64) -> u64 {
        *seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (*seed >> 33) % n
    }

    #[test]
    fn walks_match_the_recursive_walks() {
        let lookup = |n: &str| match n {
            "A" => Some(d("3")),
            "B" => Some(d("-0.25")),
            "Z" => Some(Dec::ZERO),
            _ => None,
        };
        let lookup_f64 = |n: &str| lookup(n).map(Dec::to_f64);
        let mut seed = 7;
        let mut errors = 0;
        for _ in 0..5000 {
            let src = random_formula(&mut seed, 6);
            assert_eq!(Expr::parse(&src), recursive::parse(&src), "{src}");
            let Ok(e) = Expr::parse(&src) else { continue };
            let value = e.eval(&lookup);
            errors += usize::from(value.is_err());
            assert_eq!(value, recursive::eval(&e, &lookup), "{src}");
            let (a, b) = (
                e.eval_f64(&lookup_f64),
                recursive::eval_f64(&e, &lookup_f64),
            );
            assert_eq!(a.map(f64::to_bits), b.map(f64::to_bits), "{src}");
            let mut shown = String::new();
            recursive::show(&e, &mut shown, 0);
            assert_eq!(e.to_string(), shown, "{src}");
            let again = Expr::parse(&shown).unwrap();
            assert!(again == e && recursive::same(&again, &e), "{src}");
            let (mut v1, mut v2) = (BTreeSet::new(), BTreeSet::new());
            e.vars(&mut v1);
            recursive::vars(&e, &mut v2);
            assert_eq!(v1, v2, "{src}");
            let copy = e.clone();
            assert!(copy == e && recursive::same(&copy, &e), "{src}");
            let other = Expr::parse(&random_formula(&mut seed, 6)).ok();
            if let Some(o) = other {
                assert_eq!(o == e, recursive::same(&o, &e), "{src}");
            }
            for name in ["A", "NOPE"] {
                let r = e.rename(name, "Q");
                let mut v = BTreeSet::new();
                r.vars(&mut v);
                assert!(!v.contains(name));
                assert_eq!(
                    e.substitute(name, d("-1.5")).to_string(),
                    {
                        let mut s = String::new();
                        recursive::show(&e.substitute(name, d("-1.5")), &mut s, 0);
                        s
                    },
                    "{src}"
                );
            }
        }
        // The formulas reach every kind of error, not just values.
        assert!(errors > 100, "{errors}");
    }

    #[test]
    fn parser_matches_recursive_descent() {
        let mut seed = 11;
        let mut tried = Vec::new();
        // Token soup: mostly not formulas, so every kind of parse error.
        for _ in 0..20_000 {
            let len = next(&mut seed, 16);
            let src: String = (0..len)
                .map(|_| {
                    ["1", "A", "2.5", "+", "-", "*", "/", "(", ")", " ", "$"]
                        [next(&mut seed, 11) as usize]
                })
                .collect();
            tried.push(src);
        }
        // Nesting either side of the limit, each way it can be reached.
        for n in MAX_NESTING - 3..MAX_NESTING + 3 {
            let (open, close) = ("(".repeat(n), ")".repeat(n));
            tried.push(format!("{open}1{close}"));
            tried.push(format!("{open}1{}", ")".repeat(n.saturating_sub(1))));
            tried.push(format!("{}1", "-".repeat(n)));
            tried.push(format!("{}1", "+".repeat(n)));
            tried.push(format!("{}1{}", "-(".repeat(n / 2), ")".repeat(n / 2)));
            tried.push(format!("2 * {open}1 + -A{close} / 3"));
            tried.push(format!("{open}{close}"));
        }
        let longest = vec!["1"; MAX_FORMULA_LEN / 2].join("+");
        tried.push(longest.clone());
        tried.push(format!("{longest}1"));
        tried.push(format!("{longest}+"));
        // The recursive reference needs a deep stack for the deepest nesting
        // in a debug build, whatever stack test threads get.
        std::thread::Builder::new()
            .stack_size(8 << 20)
            .spawn(move || {
                let mut errors = 0;
                for src in &tried {
                    let parsed = Expr::parse(src);
                    errors += usize::from(parsed.is_err());
                    assert_eq!(parsed, recursive::parse(src), "{src}");
                }
                // Both kinds, plenty of each.
                assert!(errors > 1000 && tried.len() - errors > 300, "{errors}");
            })
            .unwrap()
            .join()
            .unwrap();
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
