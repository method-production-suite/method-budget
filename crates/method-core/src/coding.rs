//! Account-number coding: the per-budget pattern that account numbers follow.
//!
//! An account number is made of up to four segments, in this order: category,
//! account, sub-account and set. Each segment has a fixed width (0 to 6
//! characters, 0 meaning the segment is not used) and all segments are joined by
//! one separator (or none).
//!
//! # Pattern rules ([`AccountCoding::validate`])
//!
//! - Every width is at most [`MAX_WIDTH`] (6).
//! - The category width is at least 1: every account number starts with a category.
//! - Used segments are contiguous, except that the set may follow the account
//!   when there is no sub-account: a segment with a width cannot follow an unused
//!   one (account 0 with sub-account 2, or account 0 with set 2, is rejected), but
//!   category 2, account 2, sub-account 0, set 3 is allowed (the reference
//!   product's default, decisions.md, entry 60).
//! - The separator is none, or one of space, `-`, `.`, `,`, `/`, `:`, `;`.
//!
//! # Parsing rules ([`AccountCoding::parse`])
//!
//! 1. Leading and trailing whitespace is trimmed. Nothing left (or only
//!    separators) is [`CodingError::Empty`].
//! 2. Allowed characters are ASCII letters, ASCII digits and the allowed separators
//!    (space `-` `.` `,` `/` `:` `;`). Anything else is
//!    [`CodingError::InvalidChar`]. Letter case is kept as typed.
//! 3. **Explicit mode**: if the entry contains any allowed separator (whichever
//!    one, not only the pattern's), it is split on separators. A run of several
//!    separators counts as one split, so `11 - 00` reads as `11`, `00`. Pieces fill
//!    the used segments in order. More pieces than used segments is
//!    [`CodingError::TooManySegments`] (we never silently drop a typed segment).
//! 4. **Width mode**: an entry with no separator at all is cut purely by widths:
//!    `1100` with widths 2/2 reads as `11`, `00`. Characters past the total width
//!    of the used segments are dropped (long entries are truncated).
//! 5. A piece longer than its segment is truncated, keeping its leftmost
//!    characters (`1100-01` with widths 2/2 reads as `11`, `01`).
//! 6. A non-empty piece shorter than its segment is **left-padded with zeros**:
//!    `11-5` reads as `11`, `05`, and `5` alone reads as category `05`. This keeps
//!    every stored segment at full width, so account numbers sort and match
//!    (comparison, import, accounting exports) regardless of how they were typed,
//!    and so [`AccountCoding::normalize`] is idempotent even with no separator.
//!    The same rule applies to letters: `A` in a width-2 segment becomes `0A`.
//! 7. Segments that were not typed stay empty (short entries leave trailing
//!    segments empty). Unused segments (width 0) are always empty.
//!
//! # Formatting rules ([`AccountCoding::format`])
//!
//! Used segments are written in order, joined by the separator (or nothing).
//! Trailing empty segments are omitted, so a category on its own formats as just
//! the category. Segments are written as stored; `format` does not pad or
//! truncate, that is `parse`'s job.
//!
//! # Numbers that don't follow the pattern
//!
//! The pattern is how new numbers are typed and how numbers are read for
//! matching; it never rewrites a number already in a budget. A budget made
//! from a studio chart keeps the chart's numbers exactly as written, so some
//! may not follow its pattern (mixed widths, `1A00`, `4AA.`). Anything that
//! matches numbers (actuals, comparison, imports, lookups by number) reads
//! a number with [`AccountCoding::read`] / [`AccountCoding::key`]: through
//! the pattern only when nothing would be cut off, else as its text, so two
//! different numbers never become one by being cut short.

use std::fmt;

/// Largest allowed segment width.
pub const MAX_WIDTH: u8 = 6;

/// Separators a pattern may use, and that typed entries may contain.
pub const SEPARATORS: [char; 7] = [' ', '-', '.', ',', '/', ':', ';'];

/// One of the four segments of an account number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Segment {
    Category,
    Account,
    Sub,
    Set,
}

impl fmt::Display for Segment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Segment::Category => "category",
            Segment::Account => "account",
            Segment::Sub => "sub-account",
            Segment::Set => "set",
        })
    }
}

/// The per-budget account-number pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccountCoding {
    pub category_width: u8,
    pub account_width: u8,
    pub sub_width: u8,
    pub set_width: u8,
    pub separator: Option<char>,
}

impl AccountCoding {
    /// The pattern a new budget starts with (decisions.md, entry 60), as the
    /// reference product starts one: category 2, account 2, no
    /// sub-account, set 3, separator `-`. A file that doesn't say has
    /// [`AccountCoding::default`], so budgets already saved keep theirs.
    pub fn new_budget() -> AccountCoding {
        AccountCoding {
            set_width: 3,
            ..AccountCoding::default()
        }
    }
}

impl Default for AccountCoding {
    /// Category 2, account 2, no sub-account, no set, separator `-`: what a
    /// file that doesn't say has. New budgets start from
    /// [`AccountCoding::new_budget`].
    ///
    /// Two plus two characters is the common four-digit film chart of accounts
    /// (category `11`, account `11-01`), and it leaves room for 100 categories with
    /// 100 accounts each. Sub-accounts and sets are opt-in because most budgets
    /// never use them. The hyphen makes the category boundary visible and is the
    /// separator least likely to be read as a decimal point or a list.
    fn default() -> Self {
        AccountCoding {
            category_width: 2,
            account_width: 2,
            sub_width: 0,
            set_width: 0,
            separator: Some('-'),
        }
    }
}

/// An account number split into its segments. Unused or untyped segments are empty.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AccountCode {
    pub category: String,
    pub account: String,
    pub sub: String,
    pub set: String,
}

impl AccountCode {
    /// The text of one segment.
    pub fn segment(&self, segment: Segment) -> &str {
        match segment {
            Segment::Category => &self.category,
            Segment::Account => &self.account,
            Segment::Sub => &self.sub,
            Segment::Set => &self.set,
        }
    }

    fn segment_mut(&mut self, segment: Segment) -> &mut String {
        match segment {
            Segment::Category => &mut self.category,
            Segment::Account => &mut self.account,
            Segment::Sub => &mut self.sub,
            Segment::Set => &mut self.set,
        }
    }
}

/// Why a pattern or an account number was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodingError {
    /// A segment width is over [`MAX_WIDTH`].
    WidthTooLarge { segment: Segment, width: u8 },
    /// The category width is 0.
    CategoryRequired,
    /// A used segment follows an unused one.
    SegmentGap { segment: Segment },
    /// The pattern's separator is not one of [`SEPARATORS`].
    BadSeparator(char),
    /// Nothing was typed (or only separators).
    Empty,
    /// A character that is not an ASCII letter, digit or allowed separator.
    InvalidChar(char),
    /// More separated pieces were typed than the pattern has segments.
    TooManySegments { max: usize, found: usize },
    /// A fringe account needs an account segment, but its width is 0.
    NoAccountSegment,
    /// A fringe account suffix was empty.
    EmptySuffix,
}

impl fmt::Display for CodingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CodingError::WidthTooLarge { segment, width } => write!(
                f,
                "{segment} width {width} is too large (at most {MAX_WIDTH})"
            ),
            CodingError::CategoryRequired => f.write_str("category width must be at least 1"),
            CodingError::SegmentGap { segment } => write!(
                f,
                "{segment} segment is used but the segment before it is not"
            ),
            CodingError::BadSeparator(c) => write!(
                f,
                "separator {c:?} is not allowed (use none, space, - . , / : or ;)"
            ),
            CodingError::Empty => f.write_str("account number is empty"),
            CodingError::InvalidChar(c) => {
                write!(f, "character {c:?} is not allowed in an account number")
            }
            CodingError::TooManySegments { max, found } => write!(
                f,
                "account number has {found} segments but the pattern has only {max}"
            ),
            CodingError::NoAccountSegment => {
                f.write_str("fringe accounts need an account segment (account width is 0)")
            }
            CodingError::EmptySuffix => f.write_str("fringe account suffix is empty"),
        }
    }
}

impl std::error::Error for CodingError {}

fn is_separator(c: char) -> bool {
    SEPARATORS.contains(&c)
}

/// Rule 5 and 6: truncate to `width` keeping the leftmost characters, then
/// left-pad a non-empty piece with zeros. Input must be ASCII.
fn fit(piece: &str, width: u8) -> String {
    let width = width as usize;
    if piece.is_empty() {
        return String::new();
    }
    let cut = &piece[..piece.len().min(width)];
    format!("{cut:0>width$}")
}

impl AccountCoding {
    /// Widths of category, account, sub and set, in order.
    fn widths(&self) -> [(Segment, u8); 4] {
        [
            (Segment::Category, self.category_width),
            (Segment::Account, self.account_width),
            (Segment::Sub, self.sub_width),
            (Segment::Set, self.set_width),
        ]
    }

    /// The used segments with their widths, in order (a valid pattern has
    /// them contiguous from the start, but for a set after the account with
    /// no sub-account).
    fn used_segments(&self) -> Vec<(Segment, u8)> {
        self.widths().into_iter().filter(|(_, w)| *w > 0).collect()
    }

    /// Checks the pattern rules listed in the module docs.
    pub fn validate(&self) -> Result<(), CodingError> {
        for (segment, width) in self.widths() {
            if width > MAX_WIDTH {
                return Err(CodingError::WidthTooLarge { segment, width });
            }
        }
        if self.category_width == 0 {
            return Err(CodingError::CategoryRequired);
        }
        // The segment each one needs before it: the set may skip an unused
        // sub-account, never an unused account.
        let needs = [
            (Segment::Sub, self.sub_width, self.account_width),
            (Segment::Set, self.set_width, self.account_width),
        ];
        for (segment, width, before) in needs {
            if width > 0 && before == 0 {
                return Err(CodingError::SegmentGap { segment });
            }
        }
        if let Some(c) = self.separator {
            if !is_separator(c) {
                return Err(CodingError::BadSeparator(c));
            }
        }
        Ok(())
    }

    /// Parses what a user typed into segments, following the parsing rules in the
    /// module docs. The pattern is validated first.
    pub fn parse(&self, typed: &str) -> Result<AccountCode, CodingError> {
        self.validate()?;
        let typed = typed.trim();
        if let Some(c) = typed
            .chars()
            .find(|c| !c.is_ascii_alphanumeric() && !is_separator(*c))
        {
            return Err(CodingError::InvalidChar(c));
        }
        let segments = self.used_segments();
        let used = segments.len();
        let mut pieces: Vec<String> = Vec::with_capacity(used);

        if typed.chars().any(is_separator) {
            // Explicit mode: split on separator runs.
            let parts: Vec<&str> = typed
                .split(is_separator)
                .filter(|p| !p.is_empty())
                .collect();
            if parts.is_empty() {
                return Err(CodingError::Empty);
            }
            if parts.len() > used {
                return Err(CodingError::TooManySegments {
                    max: used,
                    found: parts.len(),
                });
            }
            for (part, (_, width)) in parts.iter().zip(&segments) {
                pieces.push(fit(part, *width));
            }
        } else {
            // Width mode: cut by widths, drop anything past the total.
            if typed.is_empty() {
                return Err(CodingError::Empty);
            }
            let mut rest = typed;
            for (_, width) in &segments {
                let n = rest.len().min(*width as usize);
                let (head, tail) = rest.split_at(n);
                pieces.push(fit(head, *width));
                rest = tail;
            }
        }

        let mut code = AccountCode::default();
        for ((segment, _), piece) in segments.into_iter().zip(pieces) {
            *code.segment_mut(segment) = piece;
        }
        Ok(code)
    }

    /// Whether `typed` reads without anything cut off: no separated piece
    /// longer than its segment (rule 5) and, with no separator, no more
    /// characters than the used segments hold (rule 4). It says nothing
    /// about whether `typed` parses.
    pub fn fits(&self, typed: &str) -> bool {
        let typed = typed.trim();
        let used = self.used_segments().into_iter();
        if typed.chars().any(is_separator) {
            typed
                .split(is_separator)
                .filter(|p| !p.is_empty())
                .zip(used)
                .all(|(p, (_, w))| p.len() <= usize::from(w))
        } else {
            typed.len() <= used.map(|(_, w)| usize::from(w)).sum()
        }
    }

    /// Formats segments with the pattern's separator, omitting unused segments and
    /// trailing empty ones. Segments are written as stored.
    pub fn format(&self, code: &AccountCode) -> String {
        let segs: Vec<&str> = self
            .used_segments()
            .into_iter()
            .map(|(segment, _)| code.segment(segment))
            .collect();
        let last = segs
            .iter()
            .rposition(|s| !s.is_empty())
            .map_or(0, |i| i + 1);
        let sep = self.separator.map(String::from).unwrap_or_default();
        segs[..last].join(&sep)
    }

    /// Parses then formats: the canonical spelling of a typed account number.
    /// Idempotent: normalizing a normalized number returns it unchanged.
    pub fn normalize(&self, typed: &str) -> Result<String, CodingError> {
        Ok(self.format(&self.parse(typed)?))
    }

    /// `code` split into segments when the pattern reads it without
    /// cutting anything off ([`AccountCoding::fits`] and
    /// [`AccountCoding::parse`] both agree); `None` for a number that
    /// doesn't follow the pattern (see "Numbers that don't follow the
    /// pattern" in the module docs).
    pub fn read(&self, code: &str) -> Option<AccountCode> {
        let t = code.trim();
        if t.is_empty() || !self.fits(t) {
            return None;
        }
        self.parse(t).ok()
    }

    /// The number as matching compares it: the pattern's spelling when the
    /// pattern reads it without cutting ([`AccountCoding::read`]), else its
    /// trimmed text. `1101` and `11-01` are one number under a 2 + 2
    /// pattern; `050101` stays `050101` under it, never `05-01`.
    pub fn key(&self, code: &str) -> String {
        match self.read(code) {
            Some(c) => self.format(&c),
            None => code.trim().to_string(),
        }
    }

    /// The category a category's own number names, as matching compares
    /// it: its category segment when the pattern reads it without cutting
    /// and every later segment is empty or zeros (`11`, `1100` and `11-00`
    /// are category `11`), else its trimmed text (`1A00-5`, `050101`).
    pub fn category_key(&self, code: &str) -> String {
        match self.read(code) {
            Some(c)
                if [&c.account, &c.sub, &c.set]
                    .iter()
                    .all(|s| s.bytes().all(|b| b == b'0')) =>
            {
                c.category
            }
            _ => code.trim().to_string(),
        }
    }

    /// The fringe account number for a category under category posting.
    ///
    /// `category` is parsed like any typed account number and only its category
    /// segment is used (so `11`, `1100` and `11-00` all mean category `11`).
    /// `suffix` must be non-empty ASCII letters and digits; it is fitted to the
    /// account width with the same rules as a typed segment: truncated keeping its
    /// leftmost characters, or left-padded with zeros. Letters are kept, in both
    /// the category and the suffix. The result is formatted with the pattern, for
    /// example category `11` with suffix `99` gives `11-99` under the default
    /// pattern, and `11-0099` with an account width of 4.
    pub fn fringe_account(&self, category: &str, suffix: &str) -> Result<String, CodingError> {
        let parsed = self.parse(category)?;
        if self.account_width == 0 {
            return Err(CodingError::NoAccountSegment);
        }
        let suffix = suffix.trim();
        if suffix.is_empty() {
            return Err(CodingError::EmptySuffix);
        }
        if let Some(c) = suffix.chars().find(|c| !c.is_ascii_alphanumeric()) {
            return Err(CodingError::InvalidChar(c));
        }
        let code = AccountCode {
            category: parsed.category,
            account: fit(suffix, self.account_width),
            ..AccountCode::default()
        };
        Ok(self.format(&code))
    }

    /// The fringe account number of the category numbered `category` (as
    /// stored) under category posting: [`AccountCoding::fringe_account`]
    /// when the number names a plain category ([`AccountCoding::category_key`]
    /// reads it as a category part: `11`, `1100`, `11-00`), else the number
    /// as written followed by the suffix (`0501` with `99` is `050199`).
    /// Two categories with different numbers never share a fringe account
    /// because the pattern cut one short (`0501` and `0502` are not both
    /// `0599`).
    pub fn category_fringe_account(&self, category: &str, suffix: &str) -> String {
        let plain = self.read(category).is_some_and(|c| {
            [&c.account, &c.sub, &c.set]
                .iter()
                .all(|s| s.bytes().all(|b| b == b'0'))
        });
        if plain {
            if let Ok(n) = self.fringe_account(category, suffix) {
                return n;
            }
        }
        format!("{}{}", category.trim(), suffix.trim())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pat(c: u8, a: u8, s: u8, t: u8, sep: Option<char>) -> AccountCoding {
        AccountCoding {
            category_width: c,
            account_width: a,
            sub_width: s,
            set_width: t,
            separator: sep,
        }
    }

    fn code(c: &str, a: &str, s: &str, t: &str) -> AccountCode {
        AccountCode {
            category: c.into(),
            account: a.into(),
            sub: s.into(),
            set: t.into(),
        }
    }

    fn norm(p: &AccountCoding, typed: &str) -> String {
        p.normalize(typed).unwrap()
    }

    #[test]
    fn default_is_valid() {
        let d = AccountCoding::default();
        assert_eq!(d, pat(2, 2, 0, 0, Some('-')));
        assert_eq!(d.validate(), Ok(()));
    }

    #[test]
    fn validate_widths() {
        assert_eq!(pat(6, 6, 6, 6, None).validate(), Ok(()));
        assert_eq!(
            pat(7, 2, 0, 0, None).validate(),
            Err(CodingError::WidthTooLarge {
                segment: Segment::Category,
                width: 7
            })
        );
        assert_eq!(
            pat(2, 2, 2, 9, None).validate(),
            Err(CodingError::WidthTooLarge {
                segment: Segment::Set,
                width: 9
            })
        );
        assert_eq!(
            pat(0, 2, 0, 0, None).validate(),
            Err(CodingError::CategoryRequired)
        );
        assert_eq!(pat(1, 0, 0, 0, None).validate(), Ok(()));
    }

    #[test]
    fn validate_gaps() {
        assert_eq!(
            pat(2, 0, 2, 0, None).validate(),
            Err(CodingError::SegmentGap {
                segment: Segment::Sub
            })
        );
        assert_eq!(
            pat(2, 0, 0, 2, None).validate(),
            Err(CodingError::SegmentGap {
                segment: Segment::Set
            })
        );
        // The set may follow the account when there is no sub-account.
        assert_eq!(pat(2, 2, 0, 2, None).validate(), Ok(()));
    }

    /// The new-budget pattern (category 2, account 2, set 3): the set is
    /// the third part, read and written after the account.
    #[test]
    fn a_set_after_the_account() {
        let p = AccountCoding::new_budget();
        assert_eq!(p, pat(2, 2, 0, 3, Some('-')));
        assert_eq!(p.validate(), Ok(()));
        assert_eq!(p.parse("11-01-5"), Ok(code("11", "01", "", "005")));
        assert_eq!(p.parse("1101005"), Ok(code("11", "01", "", "005")));
        assert_eq!(norm(&p, "1101"), "11-01");
        assert_eq!(norm(&p, "11-01-ABC"), "11-01-ABC");
        assert_eq!(p.format(&code("11", "01", "", "005")), "11-01-005");
        assert!(p.fits("11-01-005") && !p.fits("11-01-0005"));
        assert_eq!(
            p.parse("1-2-3-4"),
            Err(CodingError::TooManySegments { max: 3, found: 4 })
        );
        assert_eq!(p.fringe_account("11", "99"), Ok("11-99".into()));
    }

    #[test]
    fn validate_separators() {
        for c in SEPARATORS {
            assert_eq!(pat(2, 2, 0, 0, Some(c)).validate(), Ok(()), "{c:?}");
        }
        for c in ['_', '|', 'a', '1', '\t', '\u{2013}'] {
            assert_eq!(
                pat(2, 2, 0, 0, Some(c)).validate(),
                Err(CodingError::BadSeparator(c))
            );
        }
    }

    #[test]
    fn parse_rejects_invalid_pattern() {
        assert_eq!(
            pat(0, 2, 0, 0, None).parse("11"),
            Err(CodingError::CategoryRequired)
        );
    }

    #[test]
    fn fits_says_whether_parsing_would_cut() {
        let d = AccountCoding::default();
        for ok in ["11-01", "1101", "11 - 5", "5", " 11.01 ", ""] {
            assert!(d.fits(ok), "{ok:?}");
        }
        for cut in ["11-012", "011-01", "110123", "11010"] {
            assert!(!d.fits(cut), "{cut:?}");
            // Parsing reads them, by cutting.
            assert!(d.parse(cut).is_ok(), "{cut:?}");
        }
        assert!(pat(2, 2, 2, 0, None).fits("11-01-55"));
        assert!(!pat(2, 2, 2, 0, None).fits("1101555"));
    }

    #[test]
    fn reading_never_cuts_and_keys_fall_back_to_the_text() {
        let p = pat(2, 2, 0, 0, None);
        assert_eq!(p.read("1101"), Some(code("11", "01", "", "")));
        assert_eq!(p.key("11-01"), "1101");
        assert_eq!(p.key(" 1101 "), "1101");
        // Too long, a part too long, too many parts, odd characters: as written.
        for (typed, key) in [
            ("050101", "050101"),
            ("4AA.", "4AA."),
            ("10950-2", "10950-2"),
            ("11-01-2", "11-01-2"),
            ("79-03`", "79-03`"),
        ] {
            assert_eq!(p.read(typed), None, "{typed}");
            assert_eq!(p.key(typed), key, "{typed}");
        }
        assert_eq!(p.key("1A00"), "1A00");
        assert_eq!(p.key(""), "");
        // Category keys: the category part when the rest is zeros.
        for (typed, key) in [
            ("11", "11"),
            ("1100", "11"),
            ("11-00", "11"),
            ("1A00", "1A"),
            ("1101", "1101"),
            ("050101", "050101"),
            ("5600-100", "5600-100"),
        ] {
            assert_eq!(p.category_key(typed), key, "{typed}");
        }
    }

    #[test]
    fn width_mode_splits_by_widths() {
        let d = AccountCoding::default();
        assert_eq!(d.parse("1100").unwrap(), code("11", "00", "", ""));
        assert_eq!(norm(&d, "1100"), "11-00");
        assert_eq!(norm(&d, "1101"), "11-01");
    }

    #[test]
    fn typed_separators_split_explicitly() {
        let d = AccountCoding::default();
        for typed in [
            "11.00", "11-00", "11 00", "11,00", "11/00", "11:00", "11;00",
        ] {
            assert_eq!(norm(&d, typed), "11-00", "{typed}");
        }
    }

    #[test]
    fn separator_runs_and_outer_whitespace() {
        let d = AccountCoding::default();
        assert_eq!(norm(&d, "  11 - 00  "), "11-00");
        assert_eq!(norm(&d, "11..00"), "11-00");
        assert_eq!(norm(&d, "-11-00-"), "11-00");
        assert_eq!(norm(&d, "11-"), "11");
    }

    #[test]
    fn short_segments_are_zero_padded() {
        let d = AccountCoding::default();
        assert_eq!(norm(&d, "11-5"), "11-05");
        assert_eq!(norm(&d, "1-5"), "01-05");
        assert_eq!(norm(&d, "5"), "05");
        // Width mode: partial last chunk is padded too.
        assert_eq!(norm(&d, "115"), "11-05");
        let p = pat(4, 4, 0, 0, Some('.'));
        assert_eq!(norm(&p, "11.1"), "0011.0001");
    }

    #[test]
    fn long_segments_are_truncated() {
        let d = AccountCoding::default();
        assert_eq!(norm(&d, "1100-01"), "11-01");
        assert_eq!(norm(&d, "11-0123"), "11-01");
        // Width mode drops anything past the total width.
        assert_eq!(norm(&d, "110012"), "11-00");
    }

    #[test]
    fn short_entries_leave_trailing_segments_empty() {
        let p = pat(2, 2, 2, 2, Some('.'));
        assert_eq!(p.parse("11").unwrap(), code("11", "", "", ""));
        assert_eq!(p.parse("1100").unwrap(), code("11", "00", "", ""));
        assert_eq!(p.parse("11-00-5").unwrap(), code("11", "00", "05", ""));
        assert_eq!(norm(&p, "11-00-5"), "11.00.05");
        assert_eq!(norm(&p, "11000502"), "11.00.05.02");
        assert_eq!(norm(&p, "1100050"), "11.00.05.00");
    }

    #[test]
    fn too_many_segments_rejected() {
        let d = AccountCoding::default();
        assert_eq!(
            d.parse("11-00-01"),
            Err(CodingError::TooManySegments { max: 2, found: 3 })
        );
        let one = pat(3, 0, 0, 0, None);
        assert_eq!(
            one.parse("1 2"),
            Err(CodingError::TooManySegments { max: 1, found: 2 })
        );
        assert_eq!(norm(&one, "12345"), "123");
    }

    #[test]
    fn empty_rejected() {
        let d = AccountCoding::default();
        for typed in ["", "   ", "-", " - . "] {
            assert_eq!(d.parse(typed), Err(CodingError::Empty), "{typed:?}");
        }
    }

    #[test]
    fn invalid_chars_rejected() {
        let d = AccountCoding::default();
        assert_eq!(d.parse("11_00"), Err(CodingError::InvalidChar('_')));
        assert_eq!(d.parse("11\t00"), Err(CodingError::InvalidChar('\t')));
        assert_eq!(d.parse("1é00"), Err(CodingError::InvalidChar('é')));
        assert_eq!(d.parse("１１"), Err(CodingError::InvalidChar('１')));
        assert_eq!(d.parse("11+00"), Err(CodingError::InvalidChar('+')));
    }

    #[test]
    fn alphanumeric_keeps_case() {
        let d = AccountCoding::default();
        assert_eq!(norm(&d, "1a-0B"), "1a-0B");
        assert_eq!(norm(&d, "A1b2"), "A1-b2");
        assert_eq!(norm(&d, "A-1"), "0A-01");
    }

    #[test]
    fn no_separator_pattern() {
        let p = pat(2, 2, 0, 0, None);
        assert_eq!(norm(&p, "11-5"), "1105");
        assert_eq!(norm(&p, "1105"), "1105");
        assert_eq!(norm(&p, "11"), "11");
    }

    #[test]
    fn space_separator_pattern() {
        let p = pat(2, 3, 0, 0, Some(' '));
        assert_eq!(norm(&p, "11-5"), "11 005");
        assert_eq!(norm(&p, "11 005"), "11 005");
    }

    #[test]
    fn normalize_is_idempotent() {
        let patterns = [
            AccountCoding::default(),
            pat(2, 2, 0, 0, None),
            pat(1, 3, 2, 1, Some('/')),
            pat(4, 4, 0, 0, Some(' ')),
            pat(6, 6, 6, 6, None),
        ];
        let inputs = [
            "1",
            "11",
            "115",
            "1100",
            "11-5",
            "a-b-c-d",
            "1.2.3",
            "123456789012",
            "Ab 9",
            "x",
        ];
        for p in &patterns {
            for typed in inputs {
                if let Ok(once) = p.normalize(typed) {
                    assert_eq!(p.normalize(&once).as_ref(), Ok(&once), "{p:?} {typed}");
                }
            }
        }
    }

    #[test]
    fn format_omits_trailing_empty_and_unused() {
        let p = pat(2, 2, 2, 0, Some('.'));
        assert_eq!(p.format(&code("11", "", "", "")), "11");
        assert_eq!(p.format(&code("11", "00", "", "")), "11.00");
        assert_eq!(p.format(&code("11", "00", "01", "")), "11.00.01");
        // Set is unused in this pattern, so it is never written.
        assert_eq!(p.format(&code("11", "00", "", "ZZ")), "11.00");
        assert_eq!(p.format(&AccountCode::default()), "");
    }

    #[test]
    fn fringe_account_default() {
        let d = AccountCoding::default();
        assert_eq!(d.fringe_account("11", "99").unwrap(), "11-99");
        assert_eq!(d.fringe_account("1100", "99").unwrap(), "11-99");
        assert_eq!(d.fringe_account("11-00", "99").unwrap(), "11-99");
        assert_eq!(d.fringe_account("1", "9").unwrap(), "01-09");
        assert_eq!(d.fringe_account("11", "999").unwrap(), "11-99");
    }

    #[test]
    fn fringe_account_alphanumeric_and_widths() {
        let d = AccountCoding::default();
        assert_eq!(d.fringe_account("1A", "99").unwrap(), "1A-99");
        assert_eq!(d.fringe_account("1A", "F").unwrap(), "1A-0F");
        let wide = pat(4, 4, 2, 0, None);
        assert_eq!(wide.fringe_account("2000", "99").unwrap(), "20000099");
        let narrow = pat(2, 1, 0, 0, Some('.'));
        assert_eq!(narrow.fringe_account("11", "99").unwrap(), "11.9");
    }

    #[test]
    fn category_fringe_accounts_never_cut_a_category_short() {
        let p = pat(2, 2, 0, 0, None);
        assert_eq!(p.category_fringe_account("11", "99"), "1199");
        assert_eq!(p.category_fringe_account("1100", "99"), "1199");
        assert_eq!(p.category_fringe_account("5", "99"), "0599");
        // Not a plain category under the pattern: the number as written.
        assert_eq!(p.category_fringe_account("0501", "99"), "050199");
        assert_eq!(p.category_fringe_account("0502", "99"), "050299");
        assert_eq!(p.category_fringe_account("5600-100", "99"), "5600-10099");
        assert_eq!(p.category_fringe_account("4AA.", "99"), "4AA.99");
    }

    #[test]
    fn fringe_account_errors() {
        let d = AccountCoding::default();
        assert_eq!(d.fringe_account("11", ""), Err(CodingError::EmptySuffix));
        assert_eq!(
            d.fringe_account("11", "9-9"),
            Err(CodingError::InvalidChar('-'))
        );
        assert_eq!(d.fringe_account("", "99"), Err(CodingError::Empty));
        assert_eq!(
            pat(2, 0, 0, 0, None).fringe_account("11", "99"),
            Err(CodingError::NoAccountSegment)
        );
    }

    #[test]
    fn errors_display() {
        let errs = [
            CodingError::WidthTooLarge {
                segment: Segment::Sub,
                width: 8,
            },
            CodingError::CategoryRequired,
            CodingError::SegmentGap {
                segment: Segment::Set,
            },
            CodingError::BadSeparator('_'),
            CodingError::Empty,
            CodingError::InvalidChar('#'),
            CodingError::TooManySegments { max: 2, found: 3 },
            CodingError::NoAccountSegment,
            CodingError::EmptySuffix,
        ];
        for e in errs {
            assert!(!e.to_string().is_empty());
        }
        assert_eq!(
            CodingError::WidthTooLarge {
                segment: Segment::Sub,
                width: 8
            }
            .to_string(),
            "sub-account width 8 is too large (at most 6)"
        );
    }
}
