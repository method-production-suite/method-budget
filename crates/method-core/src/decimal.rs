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

    /// The binary double nearest this value, for Movie Magic arithmetic
    /// (decisions.md, entry 55).
    pub fn to_f64(self) -> f64 {
        // Below 2^53 the scaled value and 10^9 are both exact doubles, and
        // one division rounds correctly; past that, parse the digits.
        const EXACT: u128 = 1 << 53;
        if self.0.unsigned_abs() <= EXACT {
            self.0 as f64 / SCALE as f64
        } else {
            self.to_string().parse().unwrap_or(f64::NAN)
        }
    }

    /// `v` to 9 places, for showing a double. `None` when it isn't a finite
    /// number or doesn't fit.
    pub fn from_f64(v: f64) -> Option<Dec> {
        if !v.is_finite() {
            return None;
        }
        // Formatting a double to fixed places rounds its exact value.
        let d: Dec = format!("{v:.9}").parse().ok()?;
        Some(if d.is_zero() { Dec::ZERO } else { d })
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

// ---------------------------------------------------------------------------
// Exact products, rounded to cents once

/// 64-bit limbs in a [`Wide`]: room for the product of four `Dec`s and more.
const LIMBS: usize = 8;

/// An unsigned integer of `LIMBS * 64` bits, least significant limb first.
/// Only what exact money products need: multiply, divide, print.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Wide([u64; LIMBS]);

impl Wide {
    const fn from_u128(v: u128) -> Wide {
        let mut l = [0u64; LIMBS];
        l[0] = v as u64;
        l[1] = (v >> 64) as u64;
        Wide(l)
    }

    fn to_u128(self) -> Option<u128> {
        if self.0[2..].iter().any(|&l| l != 0) {
            return None;
        }
        Some(u128::from(self.0[0]) | (u128::from(self.0[1]) << 64))
    }

    fn is_zero(self) -> bool {
        self.0.iter().all(|&l| l == 0)
    }

    /// `self * m`, `None` when it doesn't fit.
    fn mul_u64(self, m: u64) -> Option<Wide> {
        let mut out = [0u64; LIMBS];
        let mut carry = 0u128;
        for (o, &l) in out.iter_mut().zip(&self.0) {
            let v = u128::from(l) * u128::from(m) + carry;
            *o = v as u64;
            carry = v >> 64;
        }
        (carry == 0).then_some(Wide(out))
    }

    fn add(self, o: Wide) -> Option<Wide> {
        let mut out = [0u64; LIMBS];
        let mut carry = false;
        for (k, slot) in out.iter_mut().enumerate() {
            let (s1, c1) = self.0[k].overflowing_add(o.0[k]);
            let (s2, c2) = s1.overflowing_add(u64::from(carry));
            *slot = s2;
            carry = c1 || c2;
        }
        (!carry).then_some(Wide(out))
    }

    /// `self * m`, `None` when it doesn't fit.
    fn mul_u128(self, m: u128) -> Option<Wide> {
        let lo = self.mul_u64(m as u64)?;
        let hi = (m >> 64) as u64;
        if hi == 0 {
            return Some(lo);
        }
        let h = self.mul_u64(hi)?;
        if h.0[LIMBS - 1] != 0 {
            return None;
        }
        let mut shifted = [0u64; LIMBS];
        shifted[1..].copy_from_slice(&h.0[..LIMBS - 1]);
        lo.add(Wide(shifted))
    }

    /// `self * 10^n`, `None` when it doesn't fit.
    fn mul_pow10(mut self, mut n: u32) -> Option<Wide> {
        while n > 0 {
            let step = n.min(19);
            self = self.mul_u64(10u64.pow(step))?;
            n -= step;
        }
        Some(self)
    }

    /// Floor of `self / d` and the remainder. `d` is not zero.
    fn div_u64(self, d: u64) -> (Wide, u64) {
        let mut out = [0u64; LIMBS];
        let mut rem = 0u128;
        let top = self.0.iter().rposition(|&l| l != 0).map_or(0, |k| k + 1);
        for k in (0..top).rev() {
            let cur = (rem << 64) | u128::from(self.0[k]);
            out[k] = (cur / u128::from(d)) as u64;
            rem = cur % u128::from(d);
        }
        (Wide(out), rem as u64)
    }

    /// Floor of `self / 10^n`.
    fn div_pow10(mut self, mut n: u32) -> Wide {
        while n > 0 && !self.is_zero() {
            let step = n.min(19);
            self = self.div_u64(10u64.pow(step)).0;
            n -= step;
        }
        self
    }

    /// Floor of `self / d`. `d` is not zero.
    fn div_u128(self, d: u128) -> Wide {
        if let Ok(small) = u64::try_from(d) {
            return self.div_u64(small).0;
        }
        // Shift and subtract, one bit at a time; only for divisors past 64 bits.
        let mut out = [0u64; LIMBS];
        let mut rem = 0u128;
        for i in (0..LIMBS * 64).rev() {
            let top = rem >> 127;
            rem = (rem << 1) | u128::from((self.0[i / 64] >> (i % 64)) & 1);
            // With the bit shifted out, the true remainder is past 2^128 > d.
            if top == 1 || rem >= d {
                rem = rem.wrapping_sub(d);
                out[i / 64] |= 1 << (i % 64);
            }
        }
        Wide(out)
    }

    fn cmp(&self, o: &Wide) -> std::cmp::Ordering {
        self.0.iter().rev().cmp(o.0.iter().rev())
    }

    /// `self - o`; `o` is at most `self`.
    fn sub(self, o: Wide) -> Wide {
        let mut out = [0u64; LIMBS];
        let mut borrow = false;
        for (k, slot) in out.iter_mut().enumerate() {
            let (d1, b1) = self.0[k].overflowing_sub(o.0[k]);
            let (d2, b2) = d1.overflowing_sub(u64::from(borrow));
            *slot = d2;
            borrow = b1 || b2;
        }
        Wide(out)
    }

    /// Decimal digits, no sign, at least one digit.
    fn digits(self) -> String {
        const CHUNK: u64 = 10_000_000_000_000_000_000; // 10^19
        let mut chunks = Vec::new();
        let mut v = self;
        while !v.is_zero() {
            let (q, r) = v.div_u64(CHUNK);
            chunks.push(r);
            v = q;
        }
        let Some((last, rest)) = chunks.split_last() else {
            return "0".into();
        };
        let mut s = last.to_string();
        for c in rest.iter().rev() {
            s.push_str(&format!("{c:019}"));
        }
        s
    }
}

/// Rounds `n / 10^places` (`n` at least 0) to a whole number, half up.
fn round_pow10(n: Wide, places: u32) -> Option<Wide> {
    if places == 0 {
        return Some(n);
    }
    // The common case fits in 128 bits.
    if let (Some(m), Some(d)) = (n.to_u128(), 10u128.checked_pow(places)) {
        let (q, r) = (m / d, m % d);
        return Some(Wide::from_u128(if r >= d - r { q + 1 } else { q }));
    }
    // round(n / D) = floor((floor(2n / D) + 1) / 2).
    let twice = n.mul_u64(2)?.div_pow10(places);
    Some(twice.add(Wide::from_u128(1))?.div_u64(2).0)
}

/// Gives a rounded magnitude its sign as cents, `None` when it doesn't fit.
fn signed_cents(neg: bool, mag: Wide) -> Option<Cents> {
    let m = i128::try_from(mag.to_u128()?).ok()?;
    Cents::try_from(if neg { -m } else { m }).ok()
}

/// An exact decimal, `mantissa / 10^places`, for a product of money and
/// decimals that is rounded to cents only once, at the end (see
/// docs/decisions.md, entry 2). Adding and multiplying never round; they
/// fail (`None`) only past the width of the mantissa, far beyond any budget.
/// Compare values with [`Exact::checked_cmp`]; there is no `==`, since the
/// same value can be held at different scales.
#[derive(Clone, Copy, Debug)]
pub struct Exact {
    neg: bool,
    mag: Wide,
    places: u32,
}

impl Exact {
    /// `raw / 10^places`.
    pub fn new(raw: i128, places: u32) -> Exact {
        Exact {
            neg: raw < 0,
            mag: Wide::from_u128(raw.unsigned_abs()),
            places,
        }
    }

    pub fn dec(d: Dec) -> Exact {
        Exact::new(d.0, DEC_PLACES as u32)
    }

    pub fn cents(c: Cents) -> Exact {
        Exact::new(i128::from(c), 2)
    }

    /// `self * d`, exactly.
    pub fn checked_mul(self, d: Dec) -> Option<Exact> {
        self.checked_mul_exact(Exact::dec(d))
    }

    /// `self * o`, exactly.
    pub fn checked_mul_exact(self, o: Exact) -> Option<Exact> {
        let m = o.mag.to_u128()?;
        Some(Exact {
            neg: (self.neg != o.neg) && !self.mag.is_zero() && m != 0,
            mag: self.mag.mul_u128(m)?,
            places: self.places.checked_add(o.places)?,
        })
    }

    /// Both magnitudes at the larger scale.
    fn aligned(self, o: Exact) -> Option<(Wide, Wide, u32)> {
        let places = self.places.max(o.places);
        Some((
            self.mag.mul_pow10(places - self.places)?,
            o.mag.mul_pow10(places - o.places)?,
            places,
        ))
    }

    /// `self + o`, exactly.
    pub fn checked_add(self, o: Exact) -> Option<Exact> {
        let (a, b, places) = self.aligned(o)?;
        let (neg, mag) = if self.neg == o.neg {
            (self.neg, a.add(b)?)
        } else if a.cmp(&b).is_ge() {
            (self.neg, a.sub(b))
        } else {
            (o.neg, b.sub(a))
        };
        Some(Exact {
            neg: neg && !mag.is_zero(),
            mag,
            places,
        })
    }

    /// `self - o`, exactly.
    pub fn checked_sub(self, o: Exact) -> Option<Exact> {
        self.checked_add(Exact {
            neg: !o.neg && !o.mag.is_zero(),
            ..o
        })
    }

    /// How the two values compare.
    pub fn checked_cmp(self, o: Exact) -> Option<std::cmp::Ordering> {
        use std::cmp::Ordering;
        let (a, b, _) = self.aligned(o)?;
        let (sa, sb) = (self.neg && !a.is_zero(), o.neg && !b.is_zero());
        Some(match (sa, sb) {
            (false, true) => Ordering::Greater,
            (true, false) => Ordering::Less,
            (false, _) => a.cmp(&b),
            (true, true) => b.cmp(&a),
        })
    }

    /// The smaller of the two.
    pub fn checked_min(self, o: Exact) -> Option<Exact> {
        Some(if self.checked_cmp(o)?.is_le() {
            self
        } else {
            o
        })
    }

    /// `self / 100`, exactly: a percentage of something.
    pub fn percent(self) -> Exact {
        Exact {
            places: self.places + 2,
            ..self
        }
    }

    /// Rounded once to cents, half away from zero. `None` when it doesn't fit.
    pub fn to_cents(self) -> Option<Cents> {
        let mag = if self.places <= 2 {
            self.mag.mul_pow10(2 - self.places)?
        } else {
            round_pow10(self.mag, self.places - 2)?
        };
        signed_cents(self.neg, mag)
    }

    /// `self / d`, rounded once to cents, half away from zero. `None` when
    /// `d` is zero or the result doesn't fit.
    pub fn div_to_cents(self, d: Dec) -> Option<Cents> {
        let (neg, mag) = self.div_round(d, 2)?;
        signed_cents(neg, mag)
    }

    /// `self / d`, rounded once to [`DEC_PLACES`], half away from zero, for
    /// showing a quotient that need not end. `None` when `d` is zero or the
    /// result doesn't fit.
    pub fn div_to_dec(self, d: Dec) -> Option<Dec> {
        let (neg, mag) = self.div_round(d, DEC_PLACES as u32)?;
        let m = i128::try_from(mag.to_u128()?).ok()?;
        Some(Dec(if neg { -m } else { m }))
    }

    /// The sign and magnitude of `self / d` rounded to `places`.
    fn div_round(self, d: Dec, places: u32) -> Option<(bool, Wide)> {
        if d.0 == 0 {
            return None;
        }
        // round(mag * 10^(9 + places) / (10^self.places * |d|)), using
        // round(n / D) = floor((floor(2n / D) + 1) / 2) and nested floors.
        let n = self.mag.mul_pow10(DEC_PLACES as u32 + places)?.mul_u64(2)?;
        let twice = n.div_pow10(self.places).div_u128(d.0.unsigned_abs());
        let mag = twice.add(Wide::from_u128(1))?.div_u64(2).0;
        Some((self.neg != (d.0 < 0) && !mag.is_zero(), mag))
    }

    /// `self / d` for an exact divisor (a product of decimals), rounded once
    /// to cents, half away from zero. `None` when `d` is zero, its mantissa
    /// is wider than 128 bits, or the result doesn't fit.
    pub fn div_exact_to_cents(self, d: Exact) -> Option<Cents> {
        let (neg, mag) = self.div_round_exact(d, 2)?;
        signed_cents(neg, mag)
    }

    /// `self / d` for an exact divisor, rounded once to [`DEC_PLACES`], half
    /// away from zero, for showing. `None` as for
    /// [`Exact::div_exact_to_cents`].
    pub fn div_exact_to_dec(self, d: Exact) -> Option<Dec> {
        let (neg, mag) = self.div_round_exact(d, DEC_PLACES as u32)?;
        let m = i128::try_from(mag.to_u128()?).ok()?;
        Some(Dec(if neg { -m } else { m }))
    }

    /// The sign and magnitude of `self / d` rounded to `places`: the same
    /// nested floors as [`Exact::div_round`], so dividing by `Exact::dec(x)`
    /// gives exactly what dividing by `x` does.
    fn div_round_exact(self, d: Exact, places: u32) -> Option<(bool, Wide)> {
        let dm = d.mag.to_u128()?;
        if dm == 0 {
            return None;
        }
        // self / d = (mag / 10^self.places) / (dm / 10^d.places), so
        // round(mag * 10^(d.places + places) / (10^self.places * dm)).
        let n = self
            .mag
            .mul_pow10(d.places.checked_add(places)?)?
            .mul_u64(2)?;
        let twice = n.div_pow10(self.places).div_u128(dm);
        let mag = twice.add(Wide::from_u128(1))?.div_u64(2).0;
        Some((self.neg != d.neg && !mag.is_zero(), mag))
    }

    /// Rounded to [`DEC_PLACES`], half away from zero, for showing. `None`
    /// when it doesn't fit.
    pub fn to_dec(self) -> Option<Dec> {
        let p = DEC_PLACES as u32;
        let mag = if self.places <= p {
            self.mag.mul_pow10(p - self.places)?
        } else {
            round_pow10(self.mag, self.places - p)?
        };
        let m = i128::try_from(mag.to_u128()?).ok()?;
        Some(Dec(if self.neg { -m } else { m }))
    }

    /// Whether this is exactly `c` cents.
    pub fn is_cents(self, c: Cents) -> bool {
        let cents = Exact::cents(c);
        // Compare at the same scale: a * 10^p2 == b * 10^p1.
        let lhs = self.mag.mul_pow10(cents.places);
        let rhs = cents.mag.mul_pow10(self.places);
        let sign_ok = (self.mag.is_zero() && c == 0) || self.neg == (c < 0);
        sign_ok && lhs.is_some() && lhs == rhs
    }
}

impl From<Dec> for Exact {
    fn from(d: Dec) -> Exact {
        Exact::dec(d)
    }
}

impl fmt::Display for Exact {
    /// Every digit, trailing zeros after the point dropped (like [`Dec`]).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let digits = self.mag.digits();
        let p = self.places as usize;
        let (int, frac) = if digits.len() > p {
            let (i, fr) = digits.split_at(digits.len() - p);
            (i.to_string(), fr.to_string())
        } else {
            ("0".to_string(), format!("{digits:0>p$}"))
        };
        let frac = frac.trim_end_matches('0');
        let sign = if self.neg && !self.mag.is_zero() {
            "-"
        } else {
            ""
        };
        if frac.is_empty() {
            write!(f, "{sign}{int}")
        } else {
            write!(f, "{sign}{int}.{frac}")
        }
    }
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

    /// The review's case: 0.5 x 0.009999999 is 0.0049999995, which is 0.00.
    /// Rounding the product to 9 places first made it 0.005, then 0.01.
    #[test]
    fn a_product_rounds_to_cents_once() {
        let exact = Exact::dec(d("0.5")).checked_mul(d("0.009999999")).unwrap();
        assert_eq!(exact.to_string(), "0.0049999995");
        assert_eq!(exact.to_cents(), Some(0));
        assert_eq!(d("0.5").checked_mul(d("0.009999999")), Some(d("0.005")));
        let neg = Exact::dec(d("-0.5")).checked_mul(d("0.009999999")).unwrap();
        assert_eq!(neg.to_cents(), Some(0));
        assert_eq!(neg.to_string(), "-0.0049999995");
        // Exactly half a cent still goes away from zero.
        let half = Exact::dec(d("0.5")).checked_mul(d("0.01")).unwrap();
        assert_eq!(half.to_cents(), Some(1));
        let half = Exact::dec(d("-0.5")).checked_mul(d("0.01")).unwrap();
        assert_eq!(half.to_cents(), Some(-1));
        // A quotient: 0.0049999995 / 1 and 1 / 3 of a cent.
        assert_eq!(exact.div_to_cents(Dec::ONE), Some(0));
        assert_eq!(Exact::cents(1).div_to_cents(d("3")), Some(0));
        assert_eq!(Exact::cents(2).div_to_cents(d("4")), Some(1));
        assert_eq!(Exact::cents(-2).div_to_cents(d("4")), Some(-1));
        assert_eq!(Exact::cents(100).div_to_cents(d("-0.73")), Some(-137));
        assert_eq!(Exact::cents(5).div_to_cents(Dec::ZERO), None);
        assert_eq!(Exact::cents(1).div_to_dec(d("3")), Some(d("0.003333333")));
    }

    #[test]
    fn exact_adds_compares_and_shows() {
        use std::cmp::Ordering;
        let a = Exact::new(15, 1); // 1.5
        let b = Exact::cents(-150); // -1.50
        assert_eq!(a.checked_add(b).unwrap().to_string(), "0");
        assert_eq!(a.checked_sub(b).unwrap().to_string(), "3");
        assert_eq!(b.checked_sub(a).unwrap().to_string(), "-3");
        assert_eq!(a.checked_cmp(b), Some(Ordering::Greater));
        assert_eq!(b.checked_cmp(a), Some(Ordering::Less));
        assert_eq!(Exact::new(150, 2).checked_cmp(a), Some(Ordering::Equal));
        assert_eq!(
            Exact::new(0, 3).checked_cmp(Exact::new(-0, 9)),
            Some(Ordering::Equal)
        );
        assert_eq!(
            Exact::new(-2, 0).checked_cmp(Exact::new(-1, 0)),
            Some(Ordering::Less)
        );
        assert_eq!(a.checked_min(b).unwrap().to_string(), "-1.5");
        assert!(Exact::new(150, 2).is_cents(150));
        assert!(Exact::new(-1_500_000, 6).is_cents(-150));
        assert!(!Exact::new(1_500_001, 6).is_cents(150));
        assert!(!Exact::new(150, 2).is_cents(-150));
        assert!(Exact::new(0, 7).is_cents(0));
        assert_eq!(Exact::new(5, 12).to_string(), "0.000000000005");
        assert_eq!(Exact::new(5, 12).to_dec(), Some(d("0.000000000")));
        assert_eq!(Exact::new(500, 12).to_dec(), Some(d("0.000000001")));
        assert_eq!(Exact::new(-123_000, 3).to_string(), "-123");
        assert_eq!(Exact::cents(7).percent().to_string(), "0.0007");
    }

    /// Products far past 128 bits are still exact, and a result too large
    /// for cents is `None`, never a panic.
    #[test]
    fn wide_products_are_exact_and_overflow_is_none() {
        let big = Dec(i128::MAX);
        let p = Exact::dec(big).checked_mul(big).unwrap();
        assert_eq!(p.to_cents(), None);
        assert_eq!(p.to_dec(), None);
        let p4 = p.checked_mul(big).unwrap().checked_mul(big).unwrap();
        assert_eq!(p4.to_cents(), None);
        // A fifth factor no longer fits the mantissa.
        assert!(p4.checked_mul(big).is_none());
        // 10^20 x 10^20 / 10^26 = 10^14, through a divisor past 64 bits.
        let e20 = d("100000000000000000000");
        let n = Exact::dec(e20).checked_mul(e20).unwrap();
        let e26 = d("100000000000000000000000000");
        assert_eq!(n.div_to_cents(e26), Some(10_000_000_000_000_000));
        assert_eq!(Exact::cents(i64::MAX).to_cents(), Some(i64::MAX));
        assert_eq!(Exact::cents(i64::MIN).to_cents(), Some(i64::MIN));
    }

    // -- An independent reference: schoolbook arithmetic on decimal digits.

    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }

        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }

        /// A raw `Dec` of a random number of digits, either sign, often
        /// ending in nines or a five to sit near a rounding edge.
        fn raw(&mut self, max_digits: u64) -> i128 {
            let n = 1 + self.below(max_digits);
            let mut v: i128 = 0;
            for k in 0..n {
                let digit = match self.below(4) {
                    0 if k + 1 == n => 5,
                    1 => 9,
                    _ => self.below(10),
                };
                v = v * 10 + digit as i128;
            }
            if self.below(3) == 0 {
                -v
            } else {
                v
            }
        }
    }

    /// Most significant digit first.
    fn digits_of(v: u128) -> Vec<u8> {
        v.to_string().bytes().map(|b| b - b'0').collect()
    }

    fn mul_digits(a: &[u8], b: &[u8]) -> Vec<u8> {
        let mut acc = vec![0u32; a.len() + b.len()];
        for (i, &x) in a.iter().rev().enumerate() {
            for (j, &y) in b.iter().rev().enumerate() {
                acc[i + j] += u32::from(x) * u32::from(y);
            }
        }
        let mut carry = 0;
        for slot in acc.iter_mut() {
            let v = *slot + carry;
            *slot = v % 10;
            carry = v / 10;
        }
        assert_eq!(carry, 0);
        acc.iter().rev().map(|&v| v as u8).collect()
    }

    /// Drops `places` digits, rounding half up on the magnitude (the first
    /// dropped digit decides), and reads the rest as cents if it fits.
    fn round_digits(d: &[u8], places: usize, neg: bool) -> Option<Cents> {
        let keep = d.len().saturating_sub(places);
        let mut v: i128 = 0;
        for &x in &d[..keep] {
            v = v.checked_mul(10)?.checked_add(i128::from(x))?;
        }
        if d.get(keep).is_some_and(|&x| x >= 5) {
            v += 1;
        }
        Cents::try_from(if neg { -v } else { v }).ok()
    }

    /// Long division of digits by `div`, keeping `extra` more digits.
    fn div_digits(d: &[u8], div: u128) -> (Vec<u8>, u128) {
        let mut out = Vec::with_capacity(d.len());
        let mut rem: u128 = 0;
        for &x in d {
            rem = rem * 10 + u128::from(x);
            out.push((rem / div) as u8);
            rem %= div;
        }
        (out, rem)
    }

    fn reference_product_cents(raws: &[i128]) -> Option<Cents> {
        let neg = raws.iter().filter(|&&r| r < 0).count() % 2 == 1 && raws.iter().all(|&r| r != 0);
        let mut acc = vec![1u8];
        for &r in raws {
            acc = mul_digits(&acc, &digits_of(r.unsigned_abs()));
        }
        let places = DEC_PLACES * raws.len() - 2;
        round_digits(&acc, places, neg)
    }

    /// (a) The exact product rounded once equals schoolbook arithmetic on
    /// the decimal digits, for products of two and three random `Dec`s,
    /// small and huge, and for a product divided by a fourth.
    #[test]
    fn exact_products_match_a_big_number_reference() {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        for case in 0..20_000 {
            let n = 2 + (case % 2);
            let max_digits = if case % 5 == 0 { 30 } else { 14 };
            let raws: Vec<i128> = (0..n).map(|_| rng.raw(max_digits)).collect();
            let mut e = Exact::dec(Dec(raws[0]));
            for &r in &raws[1..] {
                e = e.checked_mul(Dec(r)).unwrap();
            }
            assert_eq!(
                e.to_cents(),
                reference_product_cents(&raws),
                "case {case}: {raws:?}"
            );
            // The same product shown in full reads back digit for digit.
            let mut acc = vec![1u8];
            for &r in &raws {
                acc = mul_digits(&acc, &digits_of(r.unsigned_abs()));
            }
            let shown = e.to_string();
            let plain: String = acc.iter().map(|&x| char::from(b'0' + x)).collect();
            let digits: String = shown.chars().filter(char::is_ascii_digit).collect();
            assert_eq!(
                digits.trim_start_matches('0').trim_end_matches('0'),
                plain.trim_start_matches('0').trim_end_matches('0'),
                "case {case}: {shown}"
            );

            // Divided by a positive divisor below 10^30: round(2 x N x 10^11
            // / (10^(9n) x div)) by long division, then halved up.
            let div = rng.raw(30).unsigned_abs().max(1);
            let neg_div = rng.below(2) == 0;
            let divisor = Dec(if neg_div { -(div as i128) } else { div as i128 });
            let mut num = mul_digits(&acc, &[2]);
            num.extend(std::iter::repeat_n(0, DEC_PLACES + 2));
            let keep = num.len().saturating_sub(DEC_PLACES * n);
            let (q, _) = div_digits(&num[..keep.max(1).min(num.len())], div);
            let q = if keep == 0 { vec![0] } else { q };
            let mut twice: i128 = 0;
            let mut fits = true;
            for &x in &q {
                match twice
                    .checked_mul(10)
                    .and_then(|v| v.checked_add(i128::from(x)))
                {
                    Some(v) => twice = v,
                    None => fits = false,
                }
            }
            let neg = (raws.iter().filter(|&&r| r < 0).count() % 2 == 1) != neg_div;
            let want = if fits {
                let m = (twice + 1) / 2;
                Cents::try_from(if neg && m != 0 { -m } else { m }).ok()
            } else {
                None
            };
            assert_eq!(
                e.div_to_cents(divisor),
                want,
                "case {case}: {raws:?} / {div}"
            );
            // An exact divisor gives the same: as a Dec, and at 18 places
            // (the scale of a product of two decimals).
            assert_eq!(
                e.div_exact_to_cents(Exact::dec(divisor)),
                want,
                "case {case}"
            );
            if let Some(wide) = divisor.0.checked_mul(SCALE) {
                assert_eq!(
                    e.div_exact_to_cents(Exact::new(wide, 18)),
                    want,
                    "case {case}"
                );
            }
        }
    }

    #[test]
    fn exact_divisors() {
        let third = Exact::dec(Dec::ONE).div_exact_to_dec(Exact::dec(d("3")));
        assert_eq!(third, Some(d("0.333333333")));
        // 100 x 25 x 12 x 10 / (12 x 1): a capped count through two divisors.
        let n = Exact::dec(d("100"))
            .checked_mul(d("25"))
            .and_then(|v| v.checked_mul(d("12")))
            .and_then(|v| v.checked_mul(d("10")))
            .unwrap();
        let den = Exact::dec(d("12")).checked_mul(Dec::ONE).unwrap();
        assert_eq!(n.div_exact_to_cents(den), Some(2_500_000));
        // 10.01 / (1.5 x 2): 3.336666... rounds to 3.34; negatives mirror.
        let den = Exact::dec(d("1.5")).checked_mul(d("2")).unwrap();
        assert_eq!(Exact::dec(d("10.01")).div_exact_to_cents(den), Some(334));
        assert_eq!(Exact::dec(d("-10.01")).div_exact_to_cents(den), Some(-334));
        let neg = Exact::dec(d("-3"));
        assert_eq!(Exact::dec(d("10.01")).div_exact_to_cents(neg), Some(-334));
        assert_eq!(
            Exact::cents(5).div_exact_to_cents(Exact::dec(Dec::ZERO)),
            None
        );
        // A divisor past 128 bits is refused, not a panic.
        let huge = Exact::dec(Dec(i128::MAX))
            .checked_mul(Dec(i128::MAX))
            .unwrap();
        assert_eq!(Exact::cents(5).div_exact_to_cents(huge), None);
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

    #[test]
    fn doubles_nearest_and_back() {
        for s in ["0", "1.005", "-0.125", "0.145", "30000000.015", "1.234565"] {
            assert_eq!(d(s).to_f64(), s.parse::<f64>().unwrap(), "{s}");
        }
        // Past 2^53 scaled, through the digits.
        let big = d("3333333333333.335");
        assert_eq!(big.to_f64(), 3333333333333.335_f64);
        assert_eq!(Dec::from_f64(1.0 / 3.0), Some(d("0.333333333")));
        assert_eq!(Dec::from_f64(17.0 / 3.0), Some(d("5.666666667")));
        assert_eq!(Dec::from_f64(-0.0), Some(Dec::ZERO));
        assert_eq!(Dec::from_f64(1.005), Some(d("1.005")));
        assert_eq!(Dec::from_f64(f64::INFINITY), None);
        assert_eq!(Dec::from_f64(f64::NAN), None);
        assert_eq!(Dec::from_f64(1e30), None);
    }
}
