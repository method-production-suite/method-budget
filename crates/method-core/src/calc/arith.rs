//! The two arithmetics a budget can use (decisions.md, entry 55), in one
//! place. Every step where the engine (and Explain, which works the same
//! numbers out again) evaluates a formula or turns numbers into money goes
//! through [`Num`] and [`Money`], so the budget's [`Arithmetic`] decides it
//! here and nowhere else.
//!
//! - **Movie Magic** ([`Arithmetic::Mmb`], the default): numbers are binary
//!   doubles and every step rounds to a double, as Movie Magic Budgeting
//!   works; an amount of money is rounded to cents once, like JavaScript's
//!   `Math.round(v * 100) / 100` ([`mmb_cents`]), so `1 x 1.005`
//!   (1.00499999999999989... as a double) is 1.00 and `-0.125` is -0.12.
//! - **Exact** ([`Arithmetic::Exact`]): numbers are [`Dec`]s and an amount
//!   is the exact product, rounded to cents once, half away from zero
//!   (decisions.md, entry 2).
//!
//! In both, sums of rounded amounts (accounts, categories, the grand total)
//! are integer sums of cents.

use std::fmt;

use crate::decimal::{Dec, Exact};
use crate::model::Arithmetic;
use crate::Cents;

/// Rounds a double amount to cents as Movie Magic does, like JavaScript's
/// `Math.round(v * 100)`: `v * 100` in doubles, then the nearest whole
/// number, a half going toward plus infinity. `None` when it isn't finite
/// or doesn't fit in [`Cents`].
pub fn mmb_cents(v: f64) -> Option<Cents> {
    let x = v * 100.0;
    if !x.is_finite() {
        return None;
    }
    let f = x.floor();
    // Exact: x and its floor are within 1 of each other.
    let r = if x - f >= 0.5 { f + 1.0 } else { f };
    // 2^63, exactly.
    const LIMIT: f64 = 9_223_372_036_854_775_808.0;
    (-LIMIT..LIMIT).contains(&r).then_some(r as Cents)
}

/// `v` written with the shortest digits that read back as the same double
/// (never with an exponent).
pub fn float_text(v: f64) -> String {
    // -0 shows as 0.
    format!("{}", if v == 0.0 { 0.0 } else { v })
}

/// What the double `v` really holds, digit for digit, to 21 significant
/// digits with `...` when there are more: `1.005` holds
/// `1.00499999999999989341...`, `0.125` holds `0.125`.
pub fn float_digits(v: f64) -> String {
    const SIGNIFICANT: usize = 21;
    // A double's exact value ends within 1,074 places.
    let full = format!("{:.1074}", v.abs());
    let full = full.trim_end_matches('0').trim_end_matches('.');
    let mut out = String::new();
    let mut sig = 0;
    let mut cut = false;
    for ch in full.chars() {
        if ch.is_ascii_digit() && (sig > 0 || ch != '0') {
            if sig == SIGNIFICANT {
                cut = true;
                break;
            }
            sig += 1;
        }
        out.push(ch);
    }
    let sign = if v < 0.0 { "-" } else { "" };
    let more = if cut { "..." } else { "" };
    format!("{sign}{out}{more}")
}

/// A number as a budget's arithmetic holds it: a 9-place decimal (exact) or
/// a binary double (Movie Magic). Values of one budget are all one kind;
/// an operation on a mix works in doubles.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Num {
    Dec(Dec),
    F64(f64),
}

impl Num {
    /// `d` in `arith`'s kind: itself, or the double nearest it.
    pub fn of(arith: Arithmetic, d: Dec) -> Num {
        match arith {
            Arithmetic::Exact => Num::Dec(d),
            Arithmetic::Mmb => Num::F64(d.to_f64()),
        }
    }

    /// The value to 9 places, for showing (a double rounded to them).
    /// `None` when a double doesn't fit a [`Dec`].
    pub fn shown(self) -> Option<Dec> {
        match self {
            Num::Dec(d) => Some(d),
            Num::F64(v) => Dec::from_f64(v),
        }
    }

    /// The value as a double.
    pub fn f64(self) -> f64 {
        match self {
            Num::Dec(d) => d.to_f64(),
            Num::F64(v) => v,
        }
    }

    /// `self x o`: a [`Dec`] product rounded to 9 places, or a double
    /// product. `None` on overflow.
    pub fn checked_mul(self, o: Num) -> Option<Num> {
        match (self, o) {
            (Num::Dec(a), Num::Dec(b)) => a.checked_mul(b).map(Num::Dec),
            (a, b) => finite(a.f64() * b.f64()).map(Num::F64),
        }
    }

    pub fn is_zero(self) -> bool {
        match self {
            Num::Dec(d) => d.is_zero(),
            Num::F64(v) => v == 0.0,
        }
    }

    /// The smaller of the two.
    pub fn min(self, o: Num) -> Num {
        match (self, o) {
            (Num::Dec(a), Num::Dec(b)) => Num::Dec(a.min(b)),
            (a, b) => Num::F64(a.f64().min(b.f64())),
        }
    }
}

fn finite(v: f64) -> Option<f64> {
    v.is_finite().then_some(v)
}

/// An amount of money before it is rounded to cents, in currency units: an
/// exact decimal, or a double (Movie Magic arithmetic).
#[derive(Clone, Copy, Debug)]
pub enum Money {
    Exact(Exact),
    F64(f64),
}

impl Money {
    /// `c` cents in `arith`'s kind.
    pub fn cents(arith: Arithmetic, c: Cents) -> Money {
        match arith {
            Arithmetic::Exact => Money::Exact(Exact::cents(c)),
            Arithmetic::Mmb => Money::F64(c as f64 / 100.0),
        }
    }

    /// A number taken as an amount of money.
    pub fn num(n: Num) -> Money {
        match n {
            Num::Dec(d) => Money::Exact(Exact::dec(d)),
            Num::F64(v) => Money::F64(v),
        }
    }

    fn f64(self) -> f64 {
        match self {
            Money::Exact(e) => e.to_string().parse().unwrap_or(f64::NAN),
            Money::F64(v) => v,
        }
    }

    /// `self x n`, exactly or in doubles. `None` on overflow.
    pub fn checked_mul(self, n: Num) -> Option<Money> {
        match (self, n) {
            (Money::Exact(e), Num::Dec(d)) => e.checked_mul(d).map(Money::Exact),
            (m, n) => finite(m.f64() * n.f64()).map(Money::F64),
        }
    }

    /// `self x o`, exactly or in doubles. `None` on overflow.
    pub fn checked_mul_money(self, o: Money) -> Option<Money> {
        match (self, o) {
            (Money::Exact(a), Money::Exact(b)) => a.checked_mul_exact(b).map(Money::Exact),
            (a, b) => finite(a.f64() * b.f64()).map(Money::F64),
        }
    }

    /// `self / 100`: a percentage of something.
    pub fn percent(self) -> Money {
        match self {
            Money::Exact(e) => Money::Exact(e.percent()),
            Money::F64(v) => Money::F64(v / 100.0),
        }
    }

    pub fn checked_add(self, o: Money) -> Option<Money> {
        match (self, o) {
            (Money::Exact(a), Money::Exact(b)) => a.checked_add(b).map(Money::Exact),
            (a, b) => finite(a.f64() + b.f64()).map(Money::F64),
        }
    }

    pub fn checked_sub(self, o: Money) -> Option<Money> {
        match (self, o) {
            (Money::Exact(a), Money::Exact(b)) => a.checked_sub(b).map(Money::Exact),
            (a, b) => finite(a.f64() - b.f64()).map(Money::F64),
        }
    }

    /// The smaller of the two.
    pub fn checked_min(self, o: Money) -> Option<Money> {
        match (self, o) {
            (Money::Exact(a), Money::Exact(b)) => a.checked_min(b).map(Money::Exact),
            (a, b) => Some(Money::F64(a.f64().min(b.f64()))),
        }
    }

    /// Rounded to cents once: half away from zero (exact), or half up on
    /// the double ([`mmb_cents`]). `None` when it doesn't fit.
    pub fn to_cents(self) -> Option<Cents> {
        match self {
            Money::Exact(e) => e.to_cents(),
            Money::F64(v) => mmb_cents(v),
        }
    }

    /// To 9 places, for showing.
    pub fn to_dec(self) -> Option<Dec> {
        match self {
            Money::Exact(e) => e.to_dec(),
            Money::F64(v) => Dec::from_f64(v),
        }
    }

    /// Whether this reads as exactly `c` cents: an exact value equal to it,
    /// or a double whose shortest digits are it (1.49 is a double just
    /// under 1.49, and reads as 1.49).
    pub fn is_cents(self, c: Cents) -> bool {
        match self {
            Money::Exact(e) => e.is_cents(c),
            Money::F64(v) => float_text(v)
                .parse::<Dec>()
                .is_ok_and(|d| d == Dec::from_cents(c)),
        }
    }

    /// The rounding rule this amount is rounded by, in words.
    pub fn rule(self) -> &'static str {
        match self {
            Money::Exact(_) => "half away from zero",
            Money::F64(_) => "half up on the binary value",
        }
    }
}

impl From<Exact> for Money {
    fn from(e: Exact) -> Money {
        Money::Exact(e)
    }
}

impl From<Dec> for Money {
    fn from(d: Dec) -> Money {
        Money::Exact(Exact::dec(d))
    }
}

impl fmt::Display for Money {
    /// Every digit of an exact value; a double's shortest digits.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Money::Exact(e) => write!(f, "{e}"),
            Money::F64(v) => f.write_str(&float_text(*v)),
        }
    }
}

/// `percent` percent of `c` cents (`12.5` is 12.5%), before rounding:
/// `c x percent / 100`. `None` on overflow.
pub fn percent_of(arith: Arithmetic, c: Cents, percent: Num) -> Option<Money> {
    Some(Money::cents(arith, c).checked_mul(percent)?.percent())
}

/// A line converted from its own currency at `rate` (base units per one
/// unit of it), before rounding: exact arithmetic converts the line's
/// rounded total `local` (decisions.md, entry 2, step 3); Movie Magic
/// converts the unrounded amount `line`, so the line rounds once. `None`
/// on overflow.
pub fn converted(arith: Arithmetic, line: Money, local: Cents, rate: Dec) -> Option<Money> {
    match arith {
        Arithmetic::Exact => Money::cents(arith, local).checked_mul(Num::Dec(rate)),
        Arithmetic::Mmb => line.checked_mul(Num::F64(rate.to_f64())),
    }
}

/// The rows an adjustment row takes its percentage of: the rounded totals
/// of the rows since the previous adjustment, as integer cents and, for
/// Movie Magic arithmetic, as Movie Magic adds them up, in doubles row by
/// row.
#[derive(Clone, Copy, Debug, Default)]
pub struct Window {
    pub cents: Cents,
    float: f64,
}

impl Window {
    /// Adds a row's rounded total. `None` on overflow.
    pub fn add(&mut self, c: Cents) -> Option<()> {
        self.cents = self.cents.checked_add(c)?;
        self.float += c as f64 / 100.0;
        Some(())
    }

    pub fn clear(&mut self) {
        *self = Window::default();
    }

    /// The window's amount in `arith`'s kind.
    pub fn money(&self, arith: Arithmetic) -> Money {
        match arith {
            Arithmetic::Exact => Money::cents(arith, self.cents),
            Arithmetic::Mmb => Money::F64(self.float),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(amount: f64, rate: f64) -> Option<Cents> {
        mmb_cents(amount * rate)
    }

    /// Movie Magic's line totals (parity run 2026-10-10, ZTEST01), rounded
    /// like `Math.round(v * 100) / 100` on the double.
    #[test]
    fn rounds_like_movie_magic() {
        assert_eq!(line(1.0, 1.005), Some(100));
        assert_eq!(line(1.0, 0.145), Some(14));
        assert_eq!(line(-1.0, 0.125), Some(-12));
        assert_eq!(line(1.0, 2.675), Some(268));
        assert_eq!(line(1.0, 0.125), Some(13));
        assert_eq!(line(3.0, 0.335), Some(101));
        assert_eq!(line(3.0, 432.105), Some(129_632));
        assert_eq!(line(1.0 / 3.0, 30_000_000.015), Some(1_000_000_000));
        assert_eq!(
            line(3.0, 3_333_333_333_333.335),
            Some(1_000_000_000_000_000)
        );
        // The fringe that needs Math.round's own rule: 0.49999999999999994
        // cents, which floor(x + 0.5) would make 1.
        assert_eq!(mmb_cents(0.06 * (1.0 / 12.0)), Some(0));
        assert_eq!(mmb_cents(-0.005), Some(0));
        assert_eq!(mmb_cents(f64::INFINITY), None);
        assert_eq!(mmb_cents(1e20), None);
    }

    #[test]
    fn a_double_shows_what_it_holds() {
        // Cut, not rounded: the digits that follow are the "...".
        assert_eq!(float_digits(1.005), "1.00499999999999989341...");
        assert_eq!(float_digits(0.125), "0.125");
        assert_eq!(float_digits(-0.125), "-0.125");
        assert_eq!(float_digits(3.0), "3");
        assert_eq!(float_digits(0.1 + 0.2), "0.300000000000000044408...");
        assert_eq!(float_text(1.0 / 3.0), "0.3333333333333333");
    }

    #[test]
    fn money_reads_and_rounds() {
        let a = Money::F64(1.49);
        assert!(a.is_cents(149));
        assert!(!Money::F64(1.005).is_cents(100));
        assert_eq!(Money::F64(1.005).to_string(), "1.005");
        assert_eq!(Money::F64(-0.0).to_string(), "0");
        let e = Money::cents(Arithmetic::Exact, 100).checked_mul(Num::Dec(Dec::ONE));
        assert!(e.is_some_and(|m| m.is_cents(100)));
        let mut w = Window::default();
        for c in [10, 20] {
            w.add(c).unwrap();
        }
        // 0.1 + 0.2 in doubles.
        assert_eq!(w.money(Arithmetic::Mmb).to_string(), "0.30000000000000004");
        assert_eq!(w.money(Arithmetic::Exact).to_string(), "0.3");
        assert_eq!(w.cents, 30);
    }
}
