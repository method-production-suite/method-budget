//! Fixed-point decimal for quantities, rates, percentages and exchange rates.
//!
//! `Dec` holds a value scaled by 10^9 in an `i128`. Every operation that can lose
//! precision rounds **half away from zero** (see docs/decisions.md). Money itself
//! stays in [`Cents`]; `Dec` only turns into money through the explicit helpers
//! here, each of which rounds exactly once.

use std::fmt;
use std::str::FromStr;

use crate::Cents;

/// Decimal places `Dec` keeps.
pub const DEC_PLACES: usize = 9;
const SCALE: i128 = 1_000_000_000;
const CENTS_SCALE: i128 = SCALE / 100;

/// A signed fixed-point decimal with 9 places.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Dec(i128);

/// `n / d`, rounded half away from zero. `None` on divide by zero or overflow.
fn div_round(n: i128, d: i128) -> Option<i128> {
    let q = n.checked_div(d)?;
    let r = n.checked_rem(d)?;
    if r == 0 {
        return Some(q);
    }
    let (ra, da) = (r.unsigned_abs(), d.unsigned_abs());
    if ra >= da - ra {
        if (n < 0) == (d < 0) {
            q.checked_add(1)
        } else {
            q.checked_sub(1)
        }
    } else {
        Some(q)
    }
}

impl Dec {
    pub const ZERO: Dec = Dec(0);
    pub const ONE: Dec = Dec(SCALE);

    pub const fn from_int(n: i64) -> Dec {
        Dec(n as i128 * SCALE)
    }

    pub const fn from_cents(c: Cents) -> Dec {
        Dec(c as i128 * CENTS_SCALE)
    }

    pub fn checked_add(self, o: Dec) -> Option<Dec> {
        self.0.checked_add(o.0).map(Dec)
    }

    pub fn checked_sub(self, o: Dec) -> Option<Dec> {
        self.0.checked_sub(o.0).map(Dec)
    }

    pub fn checked_neg(self) -> Option<Dec> {
        self.0.checked_neg().map(Dec)
    }

    pub fn checked_mul(self, o: Dec) -> Option<Dec> {
        div_round(self.0.checked_mul(o.0)?, SCALE).map(Dec)
    }

    /// `None` on overflow or when `o` is zero.
    pub fn checked_div(self, o: Dec) -> Option<Dec> {
        div_round(self.0.checked_mul(SCALE)?, o.0).map(Dec)
    }

    pub fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// Treats `self` as currency units and rounds to cents.
    pub fn to_cents(self) -> Option<Cents> {
        Cents::try_from(div_round(self.0, CENTS_SCALE)?).ok()
    }
}

/// `c * f`, rounded once to cents.
pub fn mul_cents(c: Cents, f: Dec) -> Option<Cents> {
    let n = (c as i128).checked_mul(f.0)?;
    Cents::try_from(div_round(n, SCALE)?).ok()
}

/// `percent` percent of `c` (so `18.5` means 18.5%), rounded once to cents.
pub fn percent_of(c: Cents, percent: Dec) -> Option<Cents> {
    let n = (c as i128).checked_mul(percent.0)?;
    Cents::try_from(div_round(n, SCALE * 100)?).ok()
}

/// The whole that `c` is `percent` percent of (`c * 100 / percent`), rounded
/// once to cents. `None` when `percent` is zero or on overflow.
pub fn whole_from_percent(c: Cents, percent: Dec) -> Option<Cents> {
    let n = (c as i128).checked_mul(100)?.checked_mul(SCALE)?;
    Cents::try_from(div_round(n, percent.0)?).ok()
}

/// `part` as a percentage of `whole`, rounded once to two decimal places
/// (`1, 3` is `33.33`). `None` when `whole` is zero.
pub fn percent_ratio(part: Cents, whole: Cents) -> Option<Dec> {
    let hundredths = div_round((part as i128).checked_mul(10_000)?, whole as i128)?;
    hundredths.checked_mul(SCALE / 100).map(Dec)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseDecError {
    Invalid,
    /// More than [`DEC_PLACES`] digits after the point.
    TooPrecise,
    Overflow,
}

impl fmt::Display for ParseDecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseDecError::Invalid => f.write_str("not a number"),
            ParseDecError::TooPrecise => write!(f, "more than {DEC_PLACES} decimal places"),
            ParseDecError::Overflow => f.write_str("number too large"),
        }
    }
}

impl std::error::Error for ParseDecError {}

impl FromStr for Dec {
    type Err = ParseDecError;

    fn from_str(s: &str) -> Result<Dec, ParseDecError> {
        let s = s.trim();
        let (neg, body) = match s.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, s.strip_prefix('+').unwrap_or(s)),
        };
        let (int, frac) = body.split_once('.').unwrap_or((body, ""));
        let digits = |p: &str| p.bytes().all(|b| b.is_ascii_digit());
        if (int.is_empty() && frac.is_empty()) || !digits(int) || !digits(frac) {
            return Err(ParseDecError::Invalid);
        }
        if frac.len() > DEC_PLACES {
            return Err(ParseDecError::TooPrecise);
        }
        let push = |acc: i128, b: u8| acc.checked_mul(10)?.checked_add(i128::from(b - b'0'));
        let mut raw = 0i128;
        for b in int.bytes() {
            raw = push(raw, b).ok_or(ParseDecError::Overflow)?;
        }
        raw = raw.checked_mul(SCALE).ok_or(ParseDecError::Overflow)?;
        let mut f = 0i128;
        for b in frac.bytes() {
            f = f * 10 + i128::from(b - b'0');
        }
        f *= 10i128.pow((DEC_PLACES - frac.len()) as u32);
        raw = raw.checked_add(f).ok_or(ParseDecError::Overflow)?;
        Ok(Dec(if neg { -raw } else { raw }))
    }
}

impl fmt::Display for Dec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.0 < 0 { "-" } else { "" };
        let a = self.0.unsigned_abs();
        let (int, frac) = (a / SCALE as u128, a % SCALE as u128);
        if frac == 0 {
            return write!(f, "{sign}{int}");
        }
        let frac = format!("{frac:09}");
        write!(f, "{sign}{int}.{}", frac.trim_end_matches('0'))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Dec {
        s.parse().unwrap()
    }

    #[test]
    fn parse_and_display_round_trip() {
        for s in ["0", "1", "-1", "12.5", "0.000000001", "-3.25", "1000000"] {
            assert_eq!(d(s).to_string(), s);
        }
        assert_eq!(d("+4.50").to_string(), "4.5");
        assert_eq!(d("7.").to_string(), "7");
        assert_eq!(d(".5").to_string(), "0.5");
    }

    #[test]
    fn parse_rejects_junk() {
        assert_eq!("".parse::<Dec>(), Err(ParseDecError::Invalid));
        assert_eq!(".".parse::<Dec>(), Err(ParseDecError::Invalid));
        assert_eq!("1.2.3".parse::<Dec>(), Err(ParseDecError::Invalid));
        assert_eq!("1e5".parse::<Dec>(), Err(ParseDecError::Invalid));
        assert_eq!(
            "0.0000000001".parse::<Dec>(),
            Err(ParseDecError::TooPrecise)
        );
    }

    #[test]
    fn rounds_half_away_from_zero() {
        assert_eq!(d("0.005").to_cents(), Some(1));
        assert_eq!(d("-0.005").to_cents(), Some(-1));
        assert_eq!(d("0.0049").to_cents(), Some(0));
        assert_eq!(d("2.675").to_cents(), Some(268));
        assert_eq!(d("1").checked_div(d("3")), Some(d("0.333333333")));
        assert_eq!(d("2").checked_div(d("3")), Some(d("0.666666667")));
        assert_eq!(d("-2").checked_div(d("3")), Some(d("-0.666666667")));
    }

    #[test]
    fn money_helpers_round_once() {
        assert_eq!(mul_cents(1001, d("1.5")), Some(1502)); // 1501.5 -> 1502
        assert_eq!(percent_of(10_000, d("18.5")), Some(1850));
        assert_eq!(percent_of(333, d("10")), Some(33)); // 33.3
        assert_eq!(percent_of(335, d("10")), Some(34)); // 33.5
        assert_eq!(Dec::from_cents(12_345), d("123.45"));
    }

    #[test]
    fn divide_by_zero_is_none() {
        assert_eq!(Dec::ONE.checked_div(Dec::ZERO), None);
    }

    #[test]
    fn percent_helpers_round_once() {
        // 40% complete with 1,000.00 spent: 2,500.00 at completion.
        assert_eq!(whole_from_percent(100_000, d("40")), Some(250_000));
        // 1.00 is 30% of 3.333..., rounded once to 3.33.
        assert_eq!(whole_from_percent(100, d("30")), Some(333));
        // 0.01 is 0.3% of 3.333...: 3.33.
        assert_eq!(whole_from_percent(1, d("0.3")), Some(333));
        // Exactly half a cent rounds away from zero: 0.05 / 20% = 0.25,
        // 0.01 / 40% = 0.025 -> 0.03, and -0.025 -> -0.03.
        assert_eq!(whole_from_percent(1, d("40")), Some(3));
        assert_eq!(whole_from_percent(-1, d("40")), Some(-3));
        assert_eq!(whole_from_percent(5, Dec::ZERO), None);
        assert_eq!(percent_ratio(1, 3), Some(d("33.33")));
        assert_eq!(percent_ratio(2, 3), Some(d("66.67")));
        assert_eq!(percent_ratio(-1, 8), Some(d("-12.5")));
        assert_eq!(percent_ratio(15, 10), Some(d("150")));
        assert_eq!(percent_ratio(1, 0), None);
        // Half a hundredth of a percent rounds away from zero.
        assert_eq!(percent_ratio(1, 80_000), Some(d("0")));
        assert_eq!(percent_ratio(1, 20_000), Some(d("0.01")));
    }
}
