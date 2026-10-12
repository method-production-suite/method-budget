//! How an amount in a currency is written (decisions.md, entry 61): which
//! separators it uses (`1,234.56` or `1.234,56`) and whether its symbol
//! goes before the value (`$1,234.56`) or after it (`1.234,56 €`), as the
//! reference product's currency table keeps them. Display only: no total
//! depends on it. Where totals show in another currency, they are written
//! in that currency's format.

use crate::model::Currency;
use crate::Cents;

/// The thousands and decimal separators of a currency.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Separators {
    /// `1,234.56`: comma thousands, point decimals. The default.
    #[default]
    CommaPoint,
    /// `1.234,56`: point thousands, comma decimals.
    PointComma,
}

impl Separators {
    pub const ALL: [Separators; 2] = [Separators::CommaPoint, Separators::PointComma];

    /// The name files and commands use: `comma_point` or `point_comma`.
    pub fn name(self) -> &'static str {
        match self {
            Separators::CommaPoint => "comma_point",
            Separators::PointComma => "point_comma",
        }
    }

    /// A name, or the pattern itself (`1,234.56`, `1.234,56`), ignoring case
    /// and surrounding spaces.
    pub fn parse(text: &str) -> Option<Separators> {
        let t = text.trim().to_ascii_lowercase().replace('-', "_");
        match t.as_str() {
            "comma_point" | "1,234.56" | "1,000.00" => Some(Separators::CommaPoint),
            "point_comma" | "1.234,56" | "1.000,00" => Some(Separators::PointComma),
            _ => None,
        }
    }

    /// The thousands separator and the decimal mark.
    pub fn marks(self) -> (char, char) {
        match self {
            Separators::CommaPoint => (',', '.'),
            Separators::PointComma => ('.', ','),
        }
    }
}

/// How amounts in a currency are written.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CurrencyFormat {
    /// The symbol after the value (`1.234,56 €`) instead of before it
    /// (`$1,234.56`, the default).
    pub symbol_after: bool,
    pub separators: Separators,
}

impl CurrencyFormat {
    pub fn is_default(&self) -> bool {
        *self == CurrencyFormat::default()
    }

    /// `cents` with this format's separators and no symbol: `1,234.56` or
    /// `1.234,56`, a negative amount with a leading `-`.
    pub fn number(&self, cents: Cents) -> String {
        let (thousands, decimal) = self.separators.marks();
        let a = cents.unsigned_abs();
        let whole = (a / 100).to_string();
        let mut grouped = String::with_capacity(whole.len() + whole.len() / 3);
        for (i, ch) in whole.chars().enumerate() {
            if i > 0 && (whole.len() - i).is_multiple_of(3) {
                grouped.push(thousands);
            }
            grouped.push(ch);
        }
        let sign = if cents < 0 { "-" } else { "" };
        format!("{sign}{grouped}{decimal}{:02}", a % 100)
    }

    /// `cents` written with `symbol` where this format puts it: before the
    /// value with no space (`$1,234.56`, `-$5.00`), or after it with one
    /// (`1.234,56 €`, `-5,00 €`). A blank symbol writes the number alone.
    pub fn amount(&self, cents: Cents, symbol: &str) -> String {
        let symbol = symbol.trim();
        let n = self.number(cents);
        if symbol.is_empty() {
            n
        } else if self.symbol_after {
            format!("{n} {symbol}")
        } else if let Some(digits) = n.strip_prefix('-') {
            format!("-{symbol}{digits}")
        } else {
            format!("{symbol}{n}")
        }
    }
}

impl Currency {
    /// `cents` written in this currency's own format and symbol
    /// ([`CurrencyFormat::amount`]).
    pub fn amount_text(&self, cents: Cents) -> String {
        self.format.amount(cents, &self.symbol)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_take_the_separators() {
        let us = CurrencyFormat::default();
        let eu = CurrencyFormat {
            symbol_after: true,
            separators: Separators::PointComma,
        };
        assert_eq!(us.number(123_456_789), "1,234,567.89");
        assert_eq!(eu.number(123_456_789), "1.234.567,89");
        assert_eq!(us.number(5), "0.05");
        assert_eq!(eu.number(-100_000), "-1.000,00");
        assert_eq!(us.number(i64::MIN), "-92,233,720,368,547,758.08");
        assert_eq!(us.amount(4_925_340, "$"), "$49,253.40");
        assert_eq!(us.amount(-500, "$"), "-$5.00");
        assert_eq!(eu.amount(4_925_340, "\u{20ac}"), "49.253,40 \u{20ac}");
        assert_eq!(eu.amount(-500, " \u{20ac} "), "-5,00 \u{20ac}");
        assert_eq!(eu.amount(100, ""), "1,00");
    }

    #[test]
    fn separators_parse_by_name_or_pattern() {
        for s in Separators::ALL {
            assert_eq!(Separators::parse(s.name()), Some(s));
        }
        assert_eq!(
            Separators::parse(" 1.234,56 "),
            Some(Separators::PointComma)
        );
        assert_eq!(
            Separators::parse("Comma-Point"),
            Some(Separators::CommaPoint)
        );
        assert_eq!(Separators::parse("1 234,56"), None);
    }
}
