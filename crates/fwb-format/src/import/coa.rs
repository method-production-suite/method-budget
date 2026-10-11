//! Reading a chart of accounts from a spreadsheet (any sheet of an Excel
//! workbook, CSV or tab-delimited text) or from another budget file.
//!
//! Charts come in a few shapes, all read here:
//!
//! - **A flat list**: one row per account (`1101  Story rights`), category
//!   rows mixed in or not (`1100  STORY & RIGHTS`, or `11`);
//! - **an outline**: category rows, then their accounts, in one number
//!   column or indented one column to the right;
//! - **separate columns**: category number and name, account number and
//!   name, the category left blank under its first row (merged cells read
//!   that way);
//! - **number and name in one cell** (`1101 Story rights`).
//!
//! Numbers may be written with or without a separator (`11-01`, `11.01`,
//! `1101`) and may have lost their leading zeros on the way through a
//! spreadsheet (`101` for `0101`): they are given back when the other
//! numbers of the same kind are one character longer. Rows above the
//! header, header rows repeated down the sheet, total rows and blank rows
//! are skipped; level headings (`ABOVE THE LINE`, `POST PRODUCTION`) and
//! level totals (`TOTAL BELOW-THE-LINE`) become production totals. The
//! pattern the numbers follow is inferred ([`fwb_core::coa::infer_coding`])
//! and can be overridden.
//!
//! Only numbers, names and levels are read (CLEAN-ROOM.md): every other
//! column of the sheet is ignored.

use std::collections::{BTreeMap, BTreeSet};

use fwb_core::coa::{
    infer_coding, infer_coding_lenient, join_code, level_name, split_code, total_for_heading,
    zero_suffix_width, Chart, ChartAccount, ChartCategory, ChartRow,
};
use fwb_core::coding::{AccountCoding, SEPARATORS};
use serde::Deserialize;

use super::mapping::norm;
use super::mapping_file::column_letters;
use super::merge::RowNote;
use super::{Book, Table, Value};

/// A column a chart-of-accounts sheet can map.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CoaField {
    CategoryCode,
    CategoryName,
    AccountCode,
    AccountName,
    /// A level for each row (`ATL`, `BTL`, `Post`): where it changes, a
    /// production total goes in.
    Level,
}

impl CoaField {
    pub const ALL: [CoaField; 5] = [
        CoaField::CategoryCode,
        CoaField::CategoryName,
        CoaField::AccountCode,
        CoaField::AccountName,
        CoaField::Level,
    ];

    pub fn key(self) -> &'static str {
        match self {
            CoaField::CategoryCode => "category_code",
            CoaField::CategoryName => "category_name",
            CoaField::AccountCode => "account_code",
            CoaField::AccountName => "account_name",
            CoaField::Level => "level",
        }
    }

    pub fn parse(key: &str) -> Option<CoaField> {
        CoaField::ALL.into_iter().find(|f| f.key() == key)
    }

    pub fn name(self) -> &'static str {
        match self {
            CoaField::CategoryCode => "Category number",
            CoaField::CategoryName => "Category name",
            CoaField::AccountCode => "Account number",
            CoaField::AccountName => "Account name",
            CoaField::Level => "Level",
        }
    }

    /// Column titles that mean this field, normalized, most telling first.
    fn synonyms(self) -> &'static [&'static str] {
        match self {
            CoaField::CategoryCode => &[
                "categorynumber",
                "categoryno",
                "category#",
                "categorycode",
                "category",
                "catnumber",
                "catno",
                "cat#",
                "catcode",
                "cat",
                "majoraccount",
            ],
            CoaField::CategoryName => &[
                "categoryname",
                "categorydescription",
                "categorytitle",
                "categorydesc",
                "catname",
                "catdescription",
                "catdesc",
                "department",
                "departmentname",
            ],
            CoaField::AccountCode => &[
                "accountnumber",
                "accountno",
                "account#",
                "accountcode",
                "account",
                "acct",
                "acctnumber",
                "acctno",
                "acct#",
                "acctcode",
                "glaccount",
                "glcode",
                "gl",
                "number",
                "no",
                "#",
                "code",
                "num",
            ],
            CoaField::AccountName => &[
                "accountname",
                "accountdescription",
                "accounttitle",
                "acctname",
                "acctdescription",
                "acctdesc",
                "accttitle",
                "description",
                "desc",
                "name",
                "title",
                "item",
            ],
            CoaField::Level => &[
                "level",
                "topsheetlevel",
                "section",
                "productiontotal",
                "atlbtl",
                "class",
                "type",
            ],
        }
    }
}

/// How well `title` (normalized) names `field`: 0 not at all.
fn score(field: CoaField, title: &str) -> u32 {
    if title.is_empty() {
        return 0;
    }
    let syn = field.synonyms();
    if let Some(i) = syn.iter().position(|s| *s == title) {
        return 200 - i as u32;
    }
    syn.iter()
        .enumerate()
        .filter(|(_, s)| s.len() >= 4 && title.contains(*s))
        .map(|(i, _)| 100 - i as u32)
        .max()
        .unwrap_or(0)
}

/// The header row and the mapped columns. Unlike a line import, the
/// category and account columns may be the same column (a one-column
/// outline); an unmapped name is the first text to the right of the number.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CoaMapping {
    pub header_row: Option<usize>,
    pub columns: BTreeMap<CoaField, usize>,
}

impl CoaMapping {
    pub fn column(&self, f: CoaField) -> Option<usize> {
        self.columns.get(&f).copied()
    }

    pub fn first_data_row(&self) -> usize {
        self.header_row.map_or(0, |h| h + 1)
    }
}

/// What to do with what is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoaOptions {
    /// Write the numbers in this pattern instead of the one inferred.
    pub coding: Option<AccountCoding>,
    /// Keep sub-accounts (numbers with a part after the account's) as
    /// accounts of their own; otherwise they are counted and left out.
    pub sub_accounts: bool,
    /// Add our default production totals when the sheet names none.
    pub default_levels: bool,
}

impl Default for CoaOptions {
    fn default() -> Self {
        CoaOptions {
            coding: None,
            sub_accounts: false,
            default_levels: true,
        }
    }
}

/// A chart as read, and what reading it found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoaRead {
    pub chart: Chart,
    /// The pattern the sheet's numbers follow (the chart is in it unless
    /// [`CoaOptions::coding`] asked for another).
    pub inferred: AccountCoding,
    /// Rows left out, and why (row numbers from 1).
    pub skipped: Vec<RowNote>,
    /// Rows read with something to check.
    pub warnings: Vec<RowNote>,
    /// Total rows skipped (counted, not listed).
    pub totals_skipped: usize,
    /// Level headings and level totals found.
    pub headings: usize,
    /// Sub-accounts found (kept only with [`CoaOptions::sub_accounts`]).
    pub sub_accounts: usize,
    /// Numbers given back their leading zeros.
    pub padded: usize,
    /// The sheet named its own levels.
    pub levels_from_source: bool,
    /// Our default levels were added.
    pub default_levels_added: bool,
    /// Read in the `kind,code,name,parent` layout (the column mapping
    /// isn't used).
    pub kinded: bool,
    /// Accounts named `.` (unused slots in the chart) left out.
    pub placeholders: usize,
    /// Accounts listed again under the same number and name, merged into
    /// the first.
    pub merged_repeats: usize,
    /// Accounts listed again under a number already used with another
    /// name, kept under a new number.
    pub renamed_repeats: Vec<RenamedAccount>,
}

// ---------------------------------------------------------------------------
// Cells

fn is_separator(c: char) -> bool {
    SEPARATORS.contains(&c)
}

/// Whether `s` reads as an account or category number: up to 20 letters,
/// digits and separators, with at least one digit, starting with a letter
/// or digit, in at most four parts, and not a sentence.
pub fn code_like(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() || s.len() > 20 {
        return false;
    }
    let first = s.chars().next().unwrap_or(' ');
    if !first.is_ascii_alphanumeric() {
        return false;
    }
    if !s
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || is_separator(c))
    {
        return false;
    }
    if !s.chars().any(|c| c.is_ascii_digit()) {
        return false;
    }
    let parts: Vec<&str> = s.split(is_separator).filter(|p| !p.is_empty()).collect();
    if parts.len() > 4 {
        return false;
    }
    // Letters are allowed (`A100`, `11-A1`) but a part that is a word is not.
    let letters = s.chars().filter(|c| c.is_ascii_alphabetic()).count();
    letters <= 2
}

/// `1101 Story rights`, `1101 - Story rights`, `1101: Story rights` as a
/// number and a name.
pub fn split_code_name(s: &str) -> Option<(String, String)> {
    let s = s.trim();
    let at = s.find(char::is_whitespace)?;
    let (code, rest) = s.split_at(at);
    let code = code.trim_end_matches([':', '.', '-']);
    if !code_like(code) {
        return None;
    }
    let name = clean_name(rest);
    if name.is_empty() || name.chars().all(|c| !c.is_alphabetic()) {
        return None;
    }
    Some((code.to_string(), name))
}

/// A name with its spaces tidied and leading dashes, dots and bullets gone.
fn clean_name(s: &str) -> String {
    let s = s.trim_start_matches(|c: char| {
        c.is_whitespace()
            || matches!(
                c,
                '-' | '\u{2013}' | '\u{2014}' | '.' | ':' | '*' | '\u{2022}'
            )
    });
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn text(t: &Table, r: usize, c: Option<usize>) -> String {
    c.map(|c| t.cell(r, c).text().trim().to_string())
        .unwrap_or_default()
}

/// A cell holding a number whose text reads as a word (not a number).
fn is_wordy(v: &Value) -> bool {
    matches!(v, Value::Text(s) if s.chars().any(char::is_alphabetic) && !code_like(s))
}

/// The first wordy cell right of column `from`.
fn name_right_of(t: &Table, r: usize, from: usize) -> String {
    let row = t.rows.get(r).map(Vec::as_slice).unwrap_or(&[]);
    row.iter()
        .skip(from + 1)
        .find(|v| is_wordy(v))
        .map(|v| clean_name(v.text()))
        .unwrap_or_default()
}

fn squash(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn is_total(s: &str) -> bool {
    let q = squash(s);
    q.starts_with("total") || q.starts_with("subtotal") || q.starts_with("grandtotal")
}

// ---------------------------------------------------------------------------
// Header and mapping

const HEADER_SCAN: usize = 30;

/// The row of column titles: among the first rows, the one whose titles
/// best name chart fields, including a number column. `None` when the sheet
/// has no titles (a bare list).
pub fn detect_header(t: &Table) -> Option<usize> {
    let mut best: Option<(u32, usize)> = None;
    for r in 0..t.rows.len().min(HEADER_SCAN) {
        let titles: Vec<String> = t.rows[r]
            .iter()
            .filter(|v| matches!(v, Value::Text(_)) && !code_like(v.text()))
            .map(|v| norm(v.text()))
            .filter(|s| !s.is_empty())
            .collect();
        if titles.len() < 2 {
            continue;
        }
        let mut used = BTreeSet::new();
        let mut total = 0;
        let mut has_code = false;
        for title in &titles {
            if let Some((s, f)) = CoaField::ALL
                .iter()
                .map(|&f| (score(f, title), f))
                .filter(|(s, f)| *s > 0 && !used.contains(f))
                .max_by_key(|(s, _)| *s)
            {
                used.insert(f);
                total += s;
                has_code |= matches!(f, CoaField::AccountCode | CoaField::CategoryCode);
            }
        }
        if has_code && total > 0 && best.is_none_or(|(b, _)| total > b) {
            best = Some((total, r));
        }
    }
    best.map(|(_, r)| r)
}

/// Each column's share of number-like cells below `first` (of its
/// non-empty cells), and how many non-empty cells it has.
fn column_stats(t: &Table, first: usize) -> Vec<(f64, usize)> {
    (0..t.width())
        .map(|c| {
            let mut filled = 0usize;
            let mut codes = 0usize;
            for r in first..t.rows.len() {
                let v = t.cell(r, c);
                if v.is_empty() {
                    continue;
                }
                filled += 1;
                if code_like(v.text()) || split_code_name(v.text()).is_some() {
                    codes += 1;
                }
            }
            if filled == 0 {
                (0.0, 0)
            } else {
                (codes as f64 / filled as f64, codes)
            }
        })
        .collect()
}

/// Strips separators: `11-01` as `1101`.
fn digits(s: &str) -> String {
    s.chars().filter(|c| !is_separator(*c)).collect()
}

/// The mapping the titles suggest, completed from the cells: without a
/// number column named, the leftmost column of numbers is the account
/// number, and a second one to its right is the account number with the
/// first as the category (an outline indented by a column, or category
/// and account columns side by side).
pub fn guess(t: &Table, header_row: Option<usize>) -> BTreeMap<CoaField, usize> {
    let mut out = BTreeMap::new();
    if let Some(h) = header_row {
        let titles: Vec<String> = (0..t.width()).map(|c| norm(t.cell(h, c).text())).collect();
        let mut cands: Vec<(u32, usize, CoaField)> = Vec::new();
        for (c, title) in titles.iter().enumerate() {
            for f in CoaField::ALL {
                let s = score(f, title);
                if s > 0 {
                    cands.push((s, c, f));
                }
            }
        }
        cands.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
        let mut taken = BTreeSet::new();
        for (_, c, f) in cands {
            if out.contains_key(&f) || taken.contains(&c) {
                continue;
            }
            out.insert(f, c);
            taken.insert(c);
        }
        // A lone `Category` column over account rows (no account column)
        // is the number column of an outline.
        if !out.contains_key(&CoaField::AccountCode) {
            if let Some(&c) = out.get(&CoaField::CategoryCode) {
                out.insert(CoaField::AccountCode, c);
            }
        }
        if !out.contains_key(&CoaField::AccountName) {
            if let Some(&c) = out.get(&CoaField::CategoryName) {
                if out.get(&CoaField::CategoryCode) == out.get(&CoaField::AccountCode) {
                    out.insert(CoaField::AccountName, c);
                }
            }
        }
    }
    if out.contains_key(&CoaField::AccountCode) {
        return out;
    }
    let first = header_row.map_or(0, |h| h + 1);
    let stats = column_stats(t, first);
    let code_cols: Vec<usize> = stats
        .iter()
        .enumerate()
        .filter(|(_, (share, n))| *share >= 0.4 && *n >= 2)
        .map(|(c, _)| c)
        .collect();
    let Some(&a) = code_cols.first() else {
        return out;
    };
    out.insert(CoaField::AccountCode, a);
    if let Some(&b) = code_cols.get(1) {
        // Side by side when the right column's numbers start with the left
        // one's; an outline when they are seldom on the same row.
        let (mut both, mut prefixed, mut either) = (0, 0, 0);
        for r in first..t.rows.len() {
            let (x, y) = (t.cell(r, a).text().trim(), t.cell(r, b).text().trim());
            let (xc, yc) = (code_like(x), code_like(y));
            if xc || yc {
                either += 1;
            }
            if xc && yc {
                both += 1;
                let (dx, dy) = (digits(x), digits(y));
                let stem = dx.trim_end_matches('0');
                if dy.starts_with(if stem.is_empty() { &dx } else { stem }) {
                    prefixed += 1;
                }
            }
        }
        let side_by_side = both > 0 && prefixed * 10 >= both * 8;
        let outline = either > 0 && both * 10 < either;
        if side_by_side || outline {
            out.insert(CoaField::CategoryCode, a);
            out.insert(CoaField::AccountCode, b);
        }
    }
    out
}

/// The sheet to start on: the visible one with the most rows of numbers.
pub fn best_sheet(book: &Book) -> usize {
    let mut best: Option<(usize, usize)> = None;
    for (i, t) in book.tables.iter().enumerate() {
        if t.hidden || t.rows.is_empty() {
            continue;
        }
        let n = column_stats(t, 0)
            .iter()
            .map(|(_, n)| *n)
            .max()
            .unwrap_or(0);
        if best.is_none_or(|(b, _)| n > b) {
            best = Some((n, i));
        }
    }
    best.map_or(0, |(_, i)| i)
}

// ---------------------------------------------------------------------------
// Reading

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Category,
    Account,
    Sub,
    /// From a column that holds both; worked out from all the numbers.
    Unknown,
}

#[derive(Clone, Debug)]
enum Ev {
    Entry {
        row: usize,
        code: String,
        name: String,
        kind: Kind,
    },
    Heading {
        text: String,
    },
    Total {
        name: String,
    },
    /// Text with no number: a category's name when a new category follows.
    Pending {
        row: usize,
        text: String,
    },
}

fn note(row: usize, message: impl Into<String>) -> RowNote {
    RowNote {
        row: row + 1,
        message: message.into(),
    }
}

/// Digit-only numbers one or more characters shorter than the longest of
/// their group, padded back with zeros (lengths are grouped where they
/// differ by one: `101` and `1101` together, `11` and `1101` apart).
/// Returns each number's group (0 shortest) and the padded numbers.
fn cluster(codes: &[String]) -> (Vec<usize>, Vec<String>, usize) {
    let lens: BTreeSet<usize> = codes.iter().map(|c| c.len()).collect();
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for l in lens {
        match groups.last_mut() {
            Some(g) if g.last().is_some_and(|&p| p + 1 == l) => g.push(l),
            _ => groups.push(vec![l]),
        }
    }
    let mut padded = 0;
    let mut which = Vec::with_capacity(codes.len());
    let mut out = Vec::with_capacity(codes.len());
    for c in codes {
        let g = groups
            .iter()
            .position(|g| g.contains(&c.len()))
            .unwrap_or(0);
        let max = *groups[g].last().unwrap_or(&c.len());
        which.push(g);
        if c.len() < max && c.bytes().all(|b| b.is_ascii_digit()) {
            padded += 1;
            out.push(format!("{c:0>max$}"));
        } else {
            out.push(c.clone());
        }
    }
    (which, out, padded)
}

/// Reads the chart in `t` with mapping `m`.
pub fn read_chart(t: &Table, m: &CoaMapping, o: &CoaOptions) -> Result<CoaRead, String> {
    if let Some((header, cols)) = kinded_columns(t, m.header_row) {
        return read_kinded(t, header, cols, o);
    }
    let cat_col = m.column(CoaField::CategoryCode);
    let acct_col = m.column(CoaField::AccountCode).or(cat_col);
    let cat_col = cat_col.filter(|c| Some(*c) != acct_col);
    let cat_name_col = m.column(CoaField::CategoryName);
    let acct_name_col = m.column(CoaField::AccountName);
    let level_col = m.column(CoaField::Level);
    let header: Vec<String> = match m.header_row {
        Some(h) => (0..t.width()).map(|c| norm(t.cell(h, c).text())).collect(),
        None => Vec::new(),
    };

    let mut skipped = Vec::new();
    let mut warnings = Vec::new();
    let mut totals_skipped = 0;
    let mut evs: Vec<Ev> = Vec::new();
    let mut level_value = String::new();
    let mut decimal_warned = false;

    for r in m.first_data_row()..t.rows.len() {
        let row = &t.rows[r];
        if row.iter().all(Value::is_empty) {
            continue;
        }
        // A header row repeated down the sheet (page breaks).
        if !header.is_empty() && (0..t.width()).all(|c| norm(t.cell(r, c).text()) == header[c]) {
            continue;
        }
        if level_col.is_some() {
            let lv = text(t, r, level_col);
            if !lv.is_empty() && squash(&lv) != squash(&level_value) {
                level_value = lv.clone();
                evs.push(Ev::Heading { text: lv });
            }
        }
        let acct_cell = text(t, r, acct_col);
        let cat_cell = text(t, r, cat_col);
        let mut wrote = false;
        // Excel keeps a number such as 11.10 as 11.1.
        for c in [acct_col, cat_col].into_iter().flatten() {
            if let Value::Number(n) = t.cell(r, c) {
                if n.contains('.') && !decimal_warned {
                    decimal_warned = true;
                    warnings.push(note(
                        r,
                        format!("{n} is a number cell with a decimal point: check no zero was dropped from its end (11.10 is kept as 11.1)"),
                    ));
                }
            }
        }

        // Side-by-side category columns: the category first.
        if cat_col.is_some() && code_like(&cat_cell) {
            let name = match cat_name_col {
                Some(_) => clean_name(&text(t, r, cat_name_col)),
                None if !code_like(&acct_cell) => name_right_of(t, r, cat_col.unwrap_or(0)),
                None => String::new(),
            };
            let name = if is_total(&name) { String::new() } else { name };
            evs.push(Ev::Entry {
                row: r,
                code: cat_cell.clone(),
                name,
                kind: Kind::Category,
            });
            wrote = true;
        }
        let (code, inline_name) = if code_like(&acct_cell) {
            (Some(acct_cell.clone()), None)
        } else if let Some((c, n)) = split_code_name(&acct_cell) {
            (Some(c), Some(n))
        } else {
            (None, None)
        };
        if let Some(code) = code {
            let name = match (inline_name, acct_name_col) {
                (Some(n), _) => n,
                (None, Some(_)) => {
                    let n = clean_name(&text(t, r, acct_name_col));
                    if n.is_empty() && cat_col.is_none() {
                        // A one-column outline may name categories elsewhere.
                        clean_name(&text(t, r, cat_name_col))
                    } else {
                        n
                    }
                }
                (None, None) => name_right_of(t, r, acct_col.unwrap_or(0)),
            };
            let name = if name.is_empty() && cat_col.is_none() && cat_name_col.is_some() {
                clean_name(&text(t, r, cat_name_col))
            } else {
                name
            };
            if is_total(&name) {
                totals_skipped += 1;
                continue;
            }
            let kind = if cat_col.is_some() {
                Kind::Account
            } else {
                Kind::Unknown
            };
            evs.push(Ev::Entry {
                row: r,
                code,
                name,
                kind,
            });
            continue;
        }
        if wrote {
            continue;
        }
        // No number: a heading, a total, or a name for what follows.
        let words = row
            .iter()
            .filter(|v| !v.is_empty())
            .map(|v| v.text().trim())
            .find(|s| s.chars().any(char::is_alphabetic));
        let Some(words) = words else {
            skipped.push(note(r, "no number or name"));
            continue;
        };
        let words = clean_name(words);
        if is_total(&words) {
            match level_name(&words) {
                Some(name) => evs.push(Ev::Total { name }),
                None => totals_skipped += 1,
            }
            continue;
        }
        if level_name(&words).is_some() {
            evs.push(Ev::Heading { text: words });
            continue;
        }
        evs.push(Ev::Pending {
            row: r,
            text: words,
        });
    }

    // --- what each number is, and padding back lost zeros ---
    let unknown: Vec<usize> = evs
        .iter()
        .enumerate()
        .filter(|(_, e)| {
            matches!(
                e,
                Ev::Entry {
                    kind: Kind::Unknown,
                    ..
                }
            )
        })
        .map(|(i, _)| i)
        .collect();
    let mut padded = 0;
    let code_of = |e: &Ev| match e {
        Ev::Entry { code, .. } => code.clone(),
        _ => String::new(),
    };
    let set = |evs: &mut Vec<Ev>, i: usize, new_code: String, new_kind: Kind| {
        if let Ev::Entry { code, kind, .. } = &mut evs[i] {
            *code = new_code;
            *kind = new_kind;
        }
    };
    if !unknown.is_empty() {
        let codes: Vec<String> = unknown.iter().map(|&i| code_of(&evs[i])).collect();
        if codes.iter().any(|c| c.chars().any(is_separator)) {
            for (&i, c) in unknown.iter().zip(&codes) {
                let parts: Vec<&str> = c.split(is_separator).filter(|p| !p.is_empty()).collect();
                // `101-00`: a category written with an all-zero account part.
                let zeros = |p: &&str| p.bytes().all(|b| b == b'0');
                let k = match parts.len() {
                    1 => Kind::Category,
                    _ if parts[1..].iter().all(zeros) => Kind::Category,
                    2 => Kind::Account,
                    _ => Kind::Sub,
                };
                set(&mut evs, i, c.clone(), k);
            }
        } else {
            let (groups, codes, p) = cluster(&codes);
            padded += p;
            let ngroups = groups.iter().max().map_or(0, |g| g + 1);
            if ngroups >= 2 {
                for ((&i, c), g) in unknown.iter().zip(codes).zip(groups) {
                    let k = match g {
                        0 => Kind::Category,
                        1 => Kind::Account,
                        _ => Kind::Sub,
                    };
                    set(&mut evs, i, c, k);
                }
            } else {
                let refs: Vec<&str> = codes.iter().map(String::as_str).collect();
                let w = zero_suffix_width(&refs);
                for (&i, c) in unknown.iter().zip(codes.iter()) {
                    let is_cat =
                        w.is_some_and(|w| c.len() > w && c[w..].bytes().all(|b| b == b'0'));
                    set(
                        &mut evs,
                        i,
                        c.clone(),
                        if is_cat {
                            Kind::Category
                        } else {
                            Kind::Account
                        },
                    );
                }
            }
        }
    }
    // Explicit columns: pad within each kind; long account numbers are subs.
    for want in [Kind::Category, Kind::Account] {
        let idx: Vec<usize> = evs
            .iter()
            .enumerate()
            .filter(|(i, e)| {
                matches!(e, Ev::Entry { kind, .. } if *kind == want) && !unknown.contains(i)
            })
            .map(|(i, _)| i)
            .collect();
        let codes: Vec<String> = idx.iter().map(|&i| code_of(&evs[i])).collect();
        if codes.is_empty() || codes.iter().any(|c| c.chars().any(is_separator)) {
            if want == Kind::Account {
                for (&i, c) in idx.iter().zip(&codes) {
                    if c.split(is_separator).filter(|p| !p.is_empty()).count() > 2 {
                        set(&mut evs, i, c.clone(), Kind::Sub);
                    }
                }
            }
            continue;
        }
        let (groups, codes, p) = cluster(&codes);
        padded += p;
        for ((&i, c), g) in idx.iter().zip(codes).zip(groups) {
            let k = if want == Kind::Account && g > 0 {
                Kind::Sub
            } else {
                want
            };
            set(&mut evs, i, c, k);
        }
    }
    if padded > 0 {
        warnings.push(RowNote {
            row: 0,
            message: format!(
                "{padded} {} had lost leading zeros (as spreadsheets drop them) and {} given them back",
                if padded == 1 { "number" } else { "numbers" },
                if padded == 1 { "was" } else { "were" }
            ),
        });
    }

    // --- the pattern ---
    let gather = |k: Kind| -> Vec<&str> {
        evs.iter()
            .filter_map(|e| match e {
                Ev::Entry { code, kind, .. } if *kind == k => Some(code.as_str()),
                _ => None,
            })
            .collect()
    };
    let (cats, accts, subs) = (
        gather(Kind::Category),
        gather(Kind::Account),
        gather(Kind::Sub),
    );
    if cats.is_empty() && accts.is_empty() {
        return Err(
            if m.column(CoaField::AccountCode).is_none() && cat_col.is_none() {
                "no column of account numbers was found: pick the column that holds them".into()
            } else {
                "no category or account numbers were found in the mapped columns".into()
            },
        );
    }
    let sub_count = subs.len();
    let inferred = infer_coding(&cats, &accts, if o.sub_accounts { &subs } else { &[] })?;

    // --- the chart, in sheet order ---
    let coding = inferred;
    let mut rows: Vec<ChartRow> = Vec::new();
    let mut cat_at: BTreeMap<String, usize> = BTreeMap::new();
    let mut cat_rows: BTreeMap<String, usize> = BTreeMap::new();
    let mut repeats = Repeats::new(
        &coding,
        evs.iter().filter_map(|e| match e {
            Ev::Entry { code, kind, .. } if *kind != Kind::Category => Some(code.as_str()),
            _ => None,
        }),
    );
    let mut current: Option<String> = None;
    let mut pending: Option<(usize, String)> = None;
    let mut section: Option<String> = None;
    let mut since = 0usize;
    let mut headings = 0usize;

    let drop_pending = |pending: &mut Option<(usize, String)>, skipped: &mut Vec<RowNote>| {
        if let Some((row, text)) = pending.take() {
            skipped.push(note(row, format!("\"{text}\" has no number")));
        }
    };

    for e in &evs {
        match e {
            Ev::Pending { row, text } => {
                drop_pending(&mut pending, &mut skipped);
                pending = Some((*row, text.clone()));
            }
            Ev::Heading { text, .. } => {
                drop_pending(&mut pending, &mut skipped);
                headings += 1;
                let total = total_for_heading(text);
                let pushed = match &section {
                    Some(prev) if since > 0 => {
                        rows.push(ChartRow::Level(total_for_heading(prev)));
                        true
                    }
                    None if since > 0 && total != fwb_core::coa::DEFAULT_LEVELS[0] => {
                        // Categories before the first heading are above the line.
                        rows.push(ChartRow::Level(
                            fwb_core::coa::DEFAULT_LEVELS[0].to_string(),
                        ));
                        true
                    }
                    _ => false,
                };
                if pushed {
                    since = 0;
                }
                section = Some(text.clone());
            }
            Ev::Total { name, .. } => {
                drop_pending(&mut pending, &mut skipped);
                headings += 1;
                if since > 0 {
                    rows.push(ChartRow::Level(name.clone()));
                }
                since = 0;
                section = None;
            }
            Ev::Entry {
                row,
                code,
                name,
                kind,
            } => {
                let parts = match split_code(&coding, code) {
                    Ok(p) => p,
                    Err(err) if *kind == Kind::Sub && !o.sub_accounts => {
                        let _ = err;
                        continue;
                    }
                    Err(err) => {
                        skipped.push(note(
                            *row,
                            format!("{code} doesn't follow the numbers' pattern: {err}"),
                        ));
                        continue;
                    }
                };
                let is_cat = *kind == Kind::Category || parts.len() == 1;
                if *kind == Kind::Sub && !o.sub_accounts {
                    continue;
                }
                let ccode = parts[0].clone();
                if is_cat {
                    if parts[1..].iter().any(|p| p.bytes().any(|b| b != b'0')) {
                        warnings.push(note(
                            *row,
                            format!("category number {code} is read as {ccode}"),
                        ));
                    }
                    let mut name = name.clone();
                    if name.is_empty() {
                        if let Some((_, text)) = pending.take() {
                            name = text;
                        }
                    } else {
                        drop_pending(&mut pending, &mut skipped);
                    }
                    match cat_at.get(&ccode) {
                        Some(&i) => {
                            if let ChartRow::Category(c) = &mut rows[i] {
                                if c.name.is_empty() {
                                    c.name = name;
                                } else if !name.is_empty()
                                    && squash(&name) != squash(&c.name)
                                    && current.as_deref() != Some(&ccode)
                                {
                                    warnings.push(note(*row, format!(
                                        "category {ccode} is listed again (row {}) as \"{name}\": the first name, \"{}\", is kept",
                                        cat_rows[&ccode] + 1, c.name
                                    )));
                                }
                            }
                        }
                        None => {
                            cat_at.insert(ccode.clone(), rows.len());
                            cat_rows.insert(ccode.clone(), *row);
                            rows.push(ChartRow::Category(ChartCategory {
                                code: ccode.clone(),
                                name,
                                accounts: Vec::new(),
                            }));
                            since += 1;
                        }
                    }
                    current = Some(ccode);
                    continue;
                }
                // An account (or a sub-account kept as one).
                let code_text = join_code(&coding, &parts);
                if let Some(cur) = &current {
                    if *cur != ccode && cat_col.is_none() && cat_at.contains_key(cur) {
                        warnings.push(note(*row, format!(
                            "account {code_text} is listed under category {cur} but its number puts it in category {ccode}"
                        )));
                    }
                }
                let i = match cat_at.get(&ccode) {
                    Some(&i) => {
                        drop_pending(&mut pending, &mut skipped);
                        i
                    }
                    None => {
                        let name = match pending.take() {
                            Some((_, text)) => text,
                            None => {
                                warnings.push(note(*row, format!(
                                    "there is no category row for {ccode}: it was added without a name"
                                )));
                                String::new()
                            }
                        };
                        cat_at.insert(ccode.clone(), rows.len());
                        cat_rows.insert(ccode.clone(), *row);
                        rows.push(ChartRow::Category(ChartCategory {
                            code: ccode.clone(),
                            name,
                            accounts: Vec::new(),
                        }));
                        since += 1;
                        current = Some(ccode.clone());
                        rows.len() - 1
                    }
                };
                let Some(code_text) = repeats.place(*row, &code_text, name, &mut warnings) else {
                    continue;
                };
                if name.is_empty() {
                    warnings.push(note(*row, format!("account {code_text} has no name")));
                }
                if let ChartRow::Category(c) = &mut rows[i] {
                    c.accounts.push(ChartAccount {
                        code: code_text,
                        name: name.clone(),
                    });
                }
            }
        }
    }
    drop_pending(&mut pending, &mut skipped);
    if let Some(prev) = &section {
        if since > 0 {
            rows.push(ChartRow::Level(total_for_heading(prev)));
        }
    }
    for c in rows.iter().filter_map(|r| match r {
        ChartRow::Category(c) => Some(c),
        _ => None,
    }) {
        if c.accounts.is_empty() {
            warnings.push(note(
                cat_rows[&c.code],
                format!("category {} has no accounts", c.code),
            ));
        }
    }
    if sub_count > 0 && !o.sub_accounts {
        warnings.push(RowNote {
            row: 0,
            message: format!(
                "{sub_count} {} left out",
                if sub_count == 1 {
                    "sub-account was"
                } else {
                    "sub-accounts were"
                }
            ),
        });
    }
    skipped.sort_by_key(|n| n.row);
    warnings.sort_by_key(|n| n.row);

    let mut chart = Chart { coding, rows };
    let levels_from_source = chart.level_count() > 0;
    let mut default_levels_added = false;
    if !levels_from_source && o.default_levels {
        chart = chart.with_default_levels();
        default_levels_added = chart.level_count() > 0;
    }
    if let Some(to) = o.coding {
        if to != chart.coding {
            chart = chart.recode(&to).map_err(|p| {
                format!(
                    "the numbers don't fit that pattern: {}{}",
                    p.iter().take(5).cloned().collect::<Vec<_>>().join("; "),
                    if p.len() > 5 {
                        format!(" (and {} more)", p.len() - 5)
                    } else {
                        String::new()
                    }
                )
            })?;
        }
    }
    Ok(CoaRead {
        chart,
        inferred,
        skipped,
        warnings,
        totals_skipped,
        headings,
        sub_accounts: sub_count,
        padded,
        levels_from_source,
        default_levels_added,
        kinded: false,
        placeholders: 0,
        merged_repeats: repeats.merged,
        renamed_repeats: repeats.renamed,
    })
}

// ---------------------------------------------------------------------------
// The `kind,code,name,parent` layout (docs/TEMPLATES.md)

/// The columns of the `kind,code,name,parent` layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KindedColumns {
    pub kind: usize,
    pub code: usize,
    pub name: usize,
    pub parent: Option<usize>,
}

/// The header row and columns when `t` is in the `kind,code,name,parent`
/// layout: a header row (`header_row`, else the first row) with `kind`,
/// `code` and `name` columns. Any other column (`notes`) is ignored.
pub fn kinded_columns(t: &Table, header_row: Option<usize>) -> Option<(usize, KindedColumns)> {
    let h = header_row.unwrap_or(0);
    let titles: Vec<String> = (0..t.width()).map(|c| norm(t.cell(h, c).text())).collect();
    let find = |name: &str| titles.iter().position(|x| x == name);
    Some((
        h,
        KindedColumns {
            kind: find("kind")?,
            code: find("code")?,
            name: find("name")?,
            parent: find("parent"),
        },
    ))
}

/// A number with its separators taken out (`11-00` as `1100`).
fn unseparated(s: &str) -> String {
    s.chars().filter(|c| !is_separator(*c)).collect()
}

/// Whether two names are the same, ignoring case and spacing.
fn same_name(a: &str, b: &str) -> bool {
    let tidy = |s: &str| {
        s.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    };
    tidy(a) == tidy(b)
}

/// An account listed again under a number already used with another name,
/// kept under a new number.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenamedAccount {
    /// The row it is on (from 1).
    pub row: usize,
    /// The number the chart gives it.
    pub old: String,
    /// The number it is imported as.
    pub new: String,
    pub name: String,
}

/// Repeated account numbers (docs/TEMPLATES.md, "Repeated account
/// numbers"). A number listed again with the same name is the same account:
/// it is merged into the first. A number listed again with another name is
/// another account: it keeps the chart's number with `-2`, `-3`... after it
/// (the chart's separator when it has one other than a space), the first
/// such number not used anywhere in the chart. Matching by number (actuals,
/// comparison, imports) can't tell two accounts with one number apart, so
/// both can't keep it.
struct Repeats {
    sep: char,
    /// Every number in the chart and every number given.
    taken: BTreeSet<String>,
    /// Each number given: its account's name and row.
    seen: BTreeMap<String, Vec<(String, usize)>>,
    merged: usize,
    renamed: Vec<RenamedAccount>,
}

impl Repeats {
    fn new<'a>(coding: &AccountCoding, codes: impl Iterator<Item = &'a str>) -> Repeats {
        Repeats {
            sep: coding.separator.filter(|c| *c != ' ').unwrap_or('-'),
            taken: codes.map(str::to_string).collect(),
            seen: BTreeMap::new(),
            merged: 0,
            renamed: Vec::new(),
        }
    }

    /// The number the account `code` named `name` on `row` (from 0) gets,
    /// or `None` when it is merged into an earlier one.
    fn place(
        &mut self,
        row: usize,
        code: &str,
        name: &str,
        warnings: &mut Vec<RowNote>,
    ) -> Option<String> {
        let Some(before) = self.seen.get(code) else {
            self.seen
                .insert(code.to_string(), vec![(name.to_string(), row)]);
            self.taken.insert(code.to_string());
            return Some(code.to_string());
        };
        if let Some((_, first)) = before.iter().find(|(n, _)| same_name(n, name)) {
            warnings.push(note(
                row,
                format!(
                    "account {code} \"{name}\" is listed again (first on row {}): the two are merged into one",
                    first + 1
                ),
            ));
            self.merged += 1;
            return None;
        }
        let first = before[0].1;
        let new = (2u32..)
            .map(|n| format!("{code}{}{n}", self.sep))
            .find(|c| !self.taken.contains(c))
            .expect("a free number");
        warnings.push(note(
            row,
            format!(
                "account {code} is listed again as \"{name}\" (first on row {}, as \"{}\"): imported as {new}",
                first + 1,
                before[0].0
            ),
        ));
        self.seen
            .get_mut(code)
            .expect("seen")
            .push((name.to_string(), row));
        self.seen.insert(new.clone(), vec![(name.to_string(), row)]);
        self.taken.insert(new.clone());
        self.renamed.push(RenamedAccount {
            row: row + 1,
            old: code.to_string(),
            new: new.clone(),
            name: name.to_string(),
        });
        Some(new)
    }
}

/// Reads a chart in the `kind,code,name,parent` layout: one row per
/// category, account, sub-account or production total, in chart order.
///
/// The sheet says what each row is, so nothing is guessed from the numbers:
/// every category and account keeps its number exactly as written (mixed
/// widths, letters, separators), each account goes in the category its
/// `parent` names (else the category above it), and the pattern is only
/// worked out for typing new numbers ([`infer_coding_lenient`]). A
/// sub-account becomes an account of its own right after its account, its
/// number as written, since a budget has no sub-account level. An account
/// named `.` is an unused slot in the chart and is left out (counted);
/// repeated numbers are merged or renumbered (see [`Repeats`]).
pub fn read_kinded(
    t: &Table,
    header: usize,
    cols: KindedColumns,
    o: &CoaOptions,
) -> Result<CoaRead, String> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum K {
        Category,
        Account,
        Sub,
        Total,
    }
    struct Row {
        row: usize,
        kind: K,
        code: String,
        name: String,
        parent: String,
    }
    let mut skipped = Vec::new();
    let mut warnings = Vec::new();
    let mut rows: Vec<Row> = Vec::new();
    let mut placeholders = 0usize;
    for r in header + 1..t.rows.len() {
        if t.rows[r].iter().all(Value::is_empty) {
            continue;
        }
        let cell = |c: usize| t.cell(r, c).text().trim().to_string();
        let kind = match squash(&cell(cols.kind)).as_str() {
            "category" => K::Category,
            "account" => K::Account,
            "subaccount" => K::Sub,
            "total" => K::Total,
            other => {
                skipped.push(note(
                    r,
                    format!("kind \"{other}\" isn't category, account, subaccount or total"),
                ));
                continue;
            }
        };
        let code = cell(cols.code);
        let raw_name = cell(cols.name);
        if kind != K::Total && code.is_empty() {
            skipped.push(note(r, "no number"));
            continue;
        }
        if matches!(kind, K::Account | K::Sub) && raw_name == "." {
            placeholders += 1;
            continue;
        }
        let name = clean_name(&raw_name);
        if kind == K::Total && name.is_empty() {
            skipped.push(note(r, "a total row with no name"));
            continue;
        }
        rows.push(Row {
            row: r,
            kind,
            code,
            name,
            parent: cols.parent.map(cell).unwrap_or_default(),
        });
    }
    let of = |k: K| -> Vec<&str> {
        rows.iter()
            .filter(|x| x.kind == k)
            .map(|x| x.code.as_str())
            .collect()
    };
    let (cats, accts, subs) = (of(K::Category), of(K::Account), of(K::Sub));
    if cats.is_empty() && accts.is_empty() {
        return Err("no category or account rows were found".into());
    }

    // Categories and totals, in order; each row's category above it.
    let mut out: Vec<ChartRow> = Vec::new();
    let mut cat_at: BTreeMap<String, usize> = BTreeMap::new();
    let mut cat_row: BTreeMap<usize, usize> = BTreeMap::new();
    let mut above: Vec<Option<usize>> = Vec::with_capacity(rows.len());
    let mut current: Option<usize> = None;
    let mut since = 0usize;
    let mut headings = 0usize;
    for x in &rows {
        match x.kind {
            K::Total => {
                headings += 1;
                if since == 0 {
                    warnings.push(note(x.row, format!(
                        "\"{}\" totals other totals: a topsheet works out its above- and below-the-line totals itself, so it was left out",
                        x.name
                    )));
                } else {
                    out.push(ChartRow::Level(total_for_heading(&x.name)));
                    since = 0;
                }
            }
            K::Category => match cat_at.get(&x.code) {
                Some(&i) => {
                    warnings.push(note(
                        x.row,
                        format!(
                            "category {} is listed again: its accounts join the first",
                            x.code
                        ),
                    ));
                    current = Some(i);
                }
                None => {
                    cat_at.insert(x.code.clone(), out.len());
                    cat_row.insert(out.len(), x.row);
                    current = Some(out.len());
                    out.push(ChartRow::Category(ChartCategory {
                        code: x.code.clone(),
                        name: x.name.clone(),
                        accounts: Vec::new(),
                    }));
                    since += 1;
                }
            },
            K::Account | K::Sub => {}
        }
        above.push(current);
    }
    // A parent written another way (`11-00` for `1100`) when only one
    // category is written that way.
    let mut loose: BTreeMap<String, Option<usize>> = BTreeMap::new();
    for (code, &i) in &cat_at {
        loose
            .entry(unseparated(code))
            .and_modify(|v| *v = None)
            .or_insert(Some(i));
    }

    // Each account's category.
    let mut acct_cat: BTreeMap<&str, usize> = BTreeMap::new();
    let mut placed: Vec<(&Row, Option<usize>)> = Vec::new();
    for (x, above) in rows.iter().zip(&above) {
        if !matches!(x.kind, K::Account | K::Sub) {
            continue;
        }
        let cat = if x.kind == K::Sub {
            acct_cat.get(x.parent.as_str()).copied().or(*above)
        } else if x.parent.is_empty() {
            *above
        } else {
            cat_at
                .get(&x.parent)
                .copied()
                .or_else(|| loose.get(&unseparated(&x.parent)).copied().flatten())
        };
        if let (K::Account, Some(c)) = (x.kind, cat) {
            acct_cat.entry(x.code.as_str()).or_insert(c);
        }
        placed.push((x, cat));
    }
    // The pattern, for typing new numbers: the one most numbers follow.
    let cat_code = |i: usize| match out.get(i) {
        Some(ChartRow::Category(c)) => c.code.as_str(),
        _ => "",
    };
    let pairs: Vec<(&str, &str)> = placed
        .iter()
        .filter(|(x, _)| x.kind == K::Account)
        .map(|(x, c)| (x.code.as_str(), c.map_or("", cat_code)))
        .collect();
    let coding = infer_coding_lenient(&cats, &pairs, &subs);

    // Accounts, each in its category.
    let mut repeats = Repeats::new(&coding, accts.iter().chain(&subs).copied());
    let mut folded = 0usize;
    for (x, cat) in placed {
        let Some(cat) = cat else {
            skipped.push(note(
                x.row,
                if x.parent.is_empty() {
                    format!("account {} has no category above it", x.code)
                } else {
                    format!(
                        "account {} names category \"{}\", which isn't in the chart",
                        x.code, x.parent
                    )
                },
            ));
            continue;
        };
        let Some(code) = repeats.place(x.row, &x.code, &x.name, &mut warnings) else {
            continue;
        };
        if x.name.is_empty() {
            warnings.push(note(x.row, format!("account {code} has no name")));
        }
        if x.kind == K::Sub {
            folded += 1;
        }
        if let Some(ChartRow::Category(c)) = out.get_mut(cat) {
            c.accounts.push(ChartAccount {
                code,
                name: x.name.clone(),
            });
        }
    }
    if folded > 0 {
        warnings.push(RowNote {
            row: 0,
            message: if folded == 1 {
                "1 sub-account was kept as an account of its own, after its account (a budget has no sub-account level)".to_string()
            } else {
                format!("{folded} sub-accounts were kept as accounts of their own, after their accounts (a budget has no sub-account level)")
            },
        });
    }
    if placeholders > 0 {
        warnings.push(RowNote {
            row: 0,
            message: format!(
                "{placeholders} {} named \".\" (unused slots in the chart) {} left out",
                if placeholders == 1 {
                    "account"
                } else {
                    "accounts"
                },
                if placeholders == 1 { "was" } else { "were" }
            ),
        });
    }
    for (i, r) in out.iter().enumerate() {
        if let ChartRow::Category(c) = r {
            if c.accounts.is_empty() {
                warnings.push(note(
                    cat_row[&i],
                    format!("category {} has no accounts", c.code),
                ));
            }
        }
    }
    skipped.sort_by_key(|n| n.row);
    warnings.sort_by_key(|n| n.row);
    let mut chart = Chart { coding, rows: out };
    let levels_from_source = chart.level_count() > 0;
    let mut default_levels_added = false;
    if !levels_from_source && o.default_levels {
        chart = chart.with_default_levels();
        default_levels_added = chart.level_count() > 0;
    }
    if let Some(to) = o.coding {
        if to != chart.coding {
            chart = chart.recode(&to).map_err(|p| {
                format!(
                    "the numbers don't fit that pattern: {}",
                    p.into_iter().take(5).collect::<Vec<_>>().join("; ")
                )
            })?;
        }
    }
    Ok(CoaRead {
        chart,
        inferred: coding,
        skipped,
        warnings,
        totals_skipped: 0,
        headings,
        sub_accounts: folded,
        padded: 0,
        levels_from_source,
        default_levels_added,
        kinded: true,
        placeholders,
        merged_repeats: repeats.merged,
        renamed_repeats: repeats.renamed,
    })
}

/// Whether `bytes` (a file named `name`) is a budget file rather than a
/// spreadsheet.
pub fn is_budget_file(bytes: &[u8], name: &str) -> bool {
    crate::has_budget_extension(std::path::Path::new(name))
        || bytes
            .iter()
            .find(|b| !b.is_ascii_whitespace())
            .is_some_and(|&b| b == b'{')
}

/// The chart of a budget file's text.
pub fn chart_from_budget_text(text: &str) -> Result<Chart, String> {
    let b = crate::from_str(text).map_err(|e| e.to_string())?;
    Ok(Chart::from_budget(&b))
}

/// A chart read from a file: a budget file's own chart, or a spreadsheet's
/// with `mapping` (`None`: the best sheet, its header row found and its
/// columns guessed).
pub struct ReadSource {
    pub chart: Chart,
    /// What reading a spreadsheet found (`None` for a budget file).
    pub read: Option<CoaRead>,
    /// The sheet read, for a spreadsheet.
    pub sheet: Option<String>,
}

/// Reads the chart in `bytes`, a file named `name`.
pub fn read_source(
    bytes: &[u8],
    name: &str,
    mapping: Option<&CoaMappingFile>,
) -> Result<ReadSource, String> {
    if is_budget_file(bytes, name) {
        let text = std::str::from_utf8(bytes)
            .map_err(|_| "the budget file isn't UTF-8 text".to_string())?;
        let mut chart = chart_from_budget_text(text)?;
        if let Some(to) = mapping.and_then(|m| m.options.coding) {
            chart = chart.recode(&to).map_err(|p| p.join("; "))?;
        }
        return Ok(ReadSource {
            chart,
            read: None,
            sheet: None,
        });
    }
    let book = super::read_file(bytes, name).map_err(|e| e.to_string())?;
    let default = CoaMappingFile::default();
    let m = mapping.unwrap_or(&default);
    let (t, cm) = m.resolve(&book)?;
    let read = read_chart(t, &cm, &m.options)?;
    Ok(ReadSource {
        chart: read.chart.clone(),
        read: Some(read),
        sheet: Some(t.name.clone()),
    })
}

/// A chart template built from a source file: a new budget titled `title`
/// in `currency` with the chart and nothing else. The same file always
/// gives the same budget (and so the same bytes once written).
pub fn build_template(
    bytes: &[u8],
    name: &str,
    mapping: Option<&CoaMappingFile>,
    title: &str,
    currency: &str,
) -> Result<(fwb_core::Budget, ReadSource), String> {
    let src = read_source(bytes, name, mapping)?;
    if src.chart.category_count() == 0 {
        return Err("no categories were found".into());
    }
    Ok((src.chart.to_budget(title, currency), src))
}

// ---------------------------------------------------------------------------
// A mapping file (`method coa import --mapping`)

#[derive(Deserialize)]
#[serde(untagged)]
enum SheetRef {
    Number(usize),
    Name(String),
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RowRef {
    Number(usize),
    Word(String),
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ColumnRef {
    Number(usize),
    Letters(String),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCoding {
    category: u8,
    account: u8,
    #[serde(default)]
    sub: u8,
    #[serde(default)]
    separator: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFile {
    #[serde(default)]
    sheet: Option<SheetRef>,
    #[serde(default)]
    header_row: Option<RowRef>,
    #[serde(default)]
    columns: Option<BTreeMap<String, ColumnRef>>,
    #[serde(default)]
    coding: Option<RawCoding>,
    #[serde(default)]
    sub_accounts: bool,
    #[serde(default = "yes")]
    default_levels: bool,
}

fn yes() -> bool {
    true
}

/// A chart mapping file, read:
///
/// ```json
/// {
///   "sheet": 1,
///   "header_row": 3,
///   "columns": { "account_code": "A", "account_name": "B" },
///   "coding": { "category": 2, "account": 2, "separator": "-" },
///   "sub_accounts": false,
///   "default_levels": true
/// }
/// ```
///
/// Every field may be left out: the best sheet, the header row found, the
/// columns guessed, the pattern inferred.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CoaMappingFile {
    /// From 0; `None` the best sheet.
    pub sheet: Option<usize>,
    pub sheet_name: Option<String>,
    /// `None`: find it; `Some(None)`: no header row.
    pub header_row: Option<Option<usize>>,
    pub columns: Option<BTreeMap<CoaField, usize>>,
    pub options: CoaOptions,
}

impl CoaMappingFile {
    pub fn parse(text: &str) -> Result<CoaMappingFile, String> {
        let raw: RawFile = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let (sheet, sheet_name) = match raw.sheet {
            None => (None, None),
            Some(SheetRef::Number(0)) => return Err("sheets count from 1".into()),
            Some(SheetRef::Number(n)) => (Some(n - 1), None),
            Some(SheetRef::Name(s)) => (None, Some(s)),
        };
        let header_row = match raw.header_row {
            None => None,
            Some(RowRef::Number(0)) => return Err("rows count from 1".into()),
            Some(RowRef::Number(n)) => Some(Some(n - 1)),
            Some(RowRef::Word(w)) if w == "none" => Some(None),
            Some(RowRef::Word(w)) if w == "detect" => None,
            Some(RowRef::Word(w)) => {
                return Err(format!(
                    "header_row `{w}`: use a row number, \"none\" or \"detect\""
                ))
            }
        };
        let columns = match raw.columns {
            None => None,
            Some(map) => {
                let mut out = BTreeMap::new();
                for (key, c) in map {
                    let f = CoaField::parse(&key).ok_or_else(|| {
                        let keys: Vec<&str> = CoaField::ALL.iter().map(|f| f.key()).collect();
                        format!("unknown field `{key}`: use {}", keys.join(", "))
                    })?;
                    let col = match c {
                        ColumnRef::Number(0) => return Err("columns count from 1".into()),
                        ColumnRef::Number(n) => n - 1,
                        ColumnRef::Letters(s) => column_letters(&s).ok_or_else(|| {
                            format!("`{s}` is not a column (use A, B... or 1, 2...)")
                        })?,
                    };
                    out.insert(f, col);
                }
                Some(out)
            }
        };
        let coding = match raw.coding {
            None => None,
            Some(c) => Some(parse_coding(c.category, c.account, c.sub, &c.separator)?),
        };
        Ok(CoaMappingFile {
            sheet,
            sheet_name,
            header_row,
            columns,
            options: CoaOptions {
                coding,
                sub_accounts: raw.sub_accounts,
                default_levels: raw.default_levels,
            },
        })
    }

    /// The table and mapping this file means for `book`.
    pub fn resolve<'b>(&self, book: &'b Book) -> Result<(&'b Table, CoaMapping), String> {
        let t = if let Some(n) = &self.sheet_name {
            book.tables
                .iter()
                .find(|t| t.name.eq_ignore_ascii_case(n.trim()))
        } else {
            book.tables
                .get(self.sheet.unwrap_or_else(|| best_sheet(book)))
        }
        .ok_or_else(|| {
            let names: Vec<String> = book
                .tables
                .iter()
                .map(|t| format!("\"{}\"", t.name))
                .collect();
            format!(
                "that sheet isn't in the file (sheets: {})",
                names.join(", ")
            )
        })?;
        let header_row = match self.header_row {
            None => detect_header(t),
            Some(Some(r)) if r >= t.rows.len().max(1) => {
                return Err("the header row is past the end of the sheet".into())
            }
            Some(h) => h,
        };
        let columns = match &self.columns {
            None => guess(t, header_row),
            Some(cols) => {
                for (f, &c) in cols {
                    if c >= t.width() {
                        return Err(format!(
                            "the {} column is past the sheet's last column",
                            f.name()
                        ));
                    }
                }
                cols.clone()
            }
        };
        Ok((
            t,
            CoaMapping {
                header_row,
                columns,
            },
        ))
    }
}

/// A pattern from widths and a separator (`""` for none, or one of
/// space `-` `.` `,` `/` `:` `;`).
pub fn parse_coding(
    category: u8,
    account: u8,
    sub: u8,
    separator: &str,
) -> Result<AccountCoding, String> {
    let mut chars = separator.chars();
    let sep = match (chars.next(), chars.next()) {
        (None, _) => None,
        (Some(c), None) => Some(c),
        _ => return Err(format!("separator `{separator}` should be one character")),
    };
    let k = AccountCoding {
        category_width: category,
        account_width: account,
        sub_width: sub,
        set_width: 0,
        separator: sep,
    };
    k.validate().map_err(|e| e.to_string())?;
    if account == 0 {
        return Err("a chart needs an account part".into());
    }
    Ok(k)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(rows: &[&[&str]]) -> Table {
        Table {
            name: "Sheet1".into(),
            rows: rows
                .iter()
                .map(|r| {
                    r.iter()
                        .map(|s| {
                            if s.is_empty() {
                                Value::Empty
                            } else if s.bytes().all(|b| b.is_ascii_digit()) {
                                Value::Number(s.to_string())
                            } else {
                                Value::Text(s.to_string())
                            }
                        })
                        .collect()
                })
                .collect(),
            ..Table::default()
        }
    }

    fn read(rows: &[&[&str]]) -> CoaRead {
        read_with(rows, &CoaOptions::default())
    }

    fn read_with(rows: &[&[&str]], o: &CoaOptions) -> CoaRead {
        let t = table(rows);
        let h = detect_header(&t);
        let m = CoaMapping {
            header_row: h,
            columns: guess(&t, h),
        };
        read_chart(&t, &m, o).unwrap()
    }

    /// The chart as `code name` lines, levels as `= name`.
    fn lines(r: &CoaRead) -> Vec<String> {
        let mut out = Vec::new();
        for row in &r.chart.rows {
            match row {
                ChartRow::Level(n) => out.push(format!("= {n}")),
                ChartRow::Category(c) => {
                    out.push(format!("{} {}", c.code, c.name));
                    for a in &c.accounts {
                        out.push(format!("  {} {}", a.code, a.name));
                    }
                }
            }
        }
        out
    }

    fn coding(r: &CoaRead) -> (u8, u8, u8, Option<char>) {
        let k = r.chart.coding;
        (k.category_width, k.account_width, k.sub_width, k.separator)
    }

    #[test]
    fn a_flat_list_with_category_rows() {
        let r = read(&[
            &["Acct #", "Description"],
            &["1100", "STORY & RIGHTS"],
            &["1101", "Rights purchase"],
            &["1102", "Writers"],
            &["1200", "PRODUCERS"],
            &["1201", "Producer"],
        ]);
        assert_eq!(
            lines(&r),
            [
                "11 STORY & RIGHTS",
                "  1101 Rights purchase",
                "  1102 Writers",
                "12 PRODUCERS",
                "  1201 Producer"
            ]
        );
        assert_eq!(coding(&r), (2, 2, 0, None));
        assert!(
            r.skipped.is_empty() && r.warnings.is_empty(),
            "{:?} {:?}",
            r.skipped,
            r.warnings
        );
    }

    #[test]
    fn a_flat_list_without_category_rows_takes_headings_as_names() {
        let r = read(&[
            &["Story and rights", ""],
            &["1101", "Rights purchase"],
            &["Producers", ""],
            &["1201", "Producer"],
        ]);
        assert_eq!(
            lines(&r),
            [
                "11 Story and rights",
                "  1101 Rights purchase",
                "12 Producers",
                "  1201 Producer"
            ]
        );
    }

    #[test]
    fn an_outline_indented_one_column() {
        let r = read(&[
            &["10", "Story", ""],
            &["", "10-01", "Rights"],
            &["", "10-02", "Writers"],
            &["20", "Staff", ""],
            &["", "20-01", "Line producer"],
        ]);
        assert_eq!(
            lines(&r),
            [
                "10 Story",
                "  10-01 Rights",
                "  10-02 Writers",
                "= Total above-the-line",
                "20 Staff",
                "  20-01 Line producer",
                "= Total production"
            ]
        );
        assert_eq!(coding(&r), (2, 2, 0, Some('-')));
    }

    #[test]
    fn separators_and_one_column_outlines() {
        let r = read(&[
            &["Number", "Name"],
            &["11", "Producers"],
            &["11.01", "Producer"],
            &["11.02", "Associate producer"],
            &["12", "Director"],
            &["12.01", "Director"],
        ]);
        assert_eq!(coding(&r), (2, 2, 0, Some('.')));
        assert_eq!(r.chart.account_count(), 3);
    }

    #[test]
    fn side_by_side_columns_with_merged_category_cells() {
        let r = read(&[
            &["Category", "Category Name", "Account", "Account Name"],
            &["11", "Producers", "1101", "Producer"],
            &["", "", "1102", "Line producer"],
            &["12", "Director", "1201", "Director"],
        ]);
        assert_eq!(
            lines(&r),
            [
                "11 Producers",
                "  1101 Producer",
                "  1102 Line producer",
                "12 Director",
                "  1201 Director"
            ]
        );
    }

    #[test]
    fn leading_zeros_are_given_back() {
        let r = read(&[
            &["Account", "Name"],
            &["100", "DEVELOPMENT"],
            &["101", "Script"],
            &["1100", "STORY"],
            &["1101", "Rights"],
        ]);
        assert_eq!(
            lines(&r),
            [
                "01 DEVELOPMENT",
                "  0101 Script",
                "11 STORY",
                "  1101 Rights"
            ]
        );
        assert_eq!(r.padded, 2);
        assert!(r
            .warnings
            .iter()
            .any(|w| w.message.contains("leading zeros")));
    }

    #[test]
    fn totals_rows_are_skipped_and_level_headings_become_levels() {
        let r = read(&[
            &["My Studio Chart of Accounts", ""],
            &["", ""],
            &["Account", "Description"],
            &["ABOVE THE LINE", ""],
            &["1100", "STORY"],
            &["1101", "Rights"],
            &["Total Story", ""],
            &["1300", "CAST"],
            &["1301", "Principal cast"],
            &["TOTAL ABOVE-THE-LINE", ""],
            &["BELOW THE LINE", ""],
            &["2000", "STAFF"],
            &["2001", "Production manager"],
            &["POST PRODUCTION", ""],
            &["4500", "EDITORIAL"],
            &["4501", "Editor"],
            &["Account", "Description"],
            &["OTHER", ""],
            &["6500", "INSURANCE"],
            &["6501", "Package"],
            &["GRAND TOTAL", ""],
        ]);
        assert_eq!(
            lines(&r),
            [
                "11 STORY",
                "  1101 Rights",
                "13 CAST",
                "  1301 Principal cast",
                "= Total above-the-line",
                "20 STAFF",
                "  2001 Production manager",
                "= Total below-the-line",
                "45 EDITORIAL",
                "  4501 Editor",
                "= Total post",
                "65 INSURANCE",
                "  6501 Package",
                "= Total other",
            ]
        );
        assert!(r.levels_from_source && !r.default_levels_added);
        assert_eq!(r.totals_skipped, 2);
    }

    #[test]
    fn duplicates_and_gaps_are_warned_about() {
        let r = read(&[
            &["Account", "Description"],
            &["1100", "STORY"],
            &["1101", "Rights"],
            &["1101", "Rights again"],
            &["1101", "RIGHTS"],
            &["1200", "EMPTY"],
            &["1301", "Orphan"],
        ]);
        let msgs: Vec<&str> = r.warnings.iter().map(|w| w.message.as_str()).collect();
        assert!(
            msgs.iter().any(|m| m.contains("imported as 1101-2")),
            "{msgs:?}"
        );
        assert!(
            msgs.iter().any(|m| m.contains("merged into one")),
            "{msgs:?}"
        );
        assert!(msgs.iter().any(|m| m.contains("no accounts")), "{msgs:?}");
        assert!(
            msgs.iter().any(|m| m.contains("no category row for 13")),
            "{msgs:?}"
        );
        // Another name is another account, under a new number; the same
        // name is the same account.
        assert_eq!(
            lines(&r),
            [
                "11 STORY",
                "  1101 Rights",
                "  1101-2 Rights again",
                "12 EMPTY",
                "13 ",
                "  1301 Orphan"
            ]
        );
        assert_eq!((r.merged_repeats, r.renamed_repeats.len()), (1, 1));
    }

    #[test]
    fn codes_and_names_in_one_cell() {
        let r = read(&[
            &["1100 STORY & RIGHTS"],
            &["1101 Rights purchase"],
            &["1102 - Writers"],
        ]);
        assert_eq!(
            lines(&r),
            [
                "11 STORY & RIGHTS",
                "  1101 Rights purchase",
                "  1102 Writers"
            ]
        );
    }

    #[test]
    fn default_levels_and_sub_accounts() {
        let rows: &[&[&str]] = &[
            &["Account", "Description"],
            &["11", "Producers"],
            &["11-01", "Producer"],
            &["11-01-01", "Producer fee"],
            &["20", "Staff"],
            &["20-01", "Manager"],
        ];
        let r = read(rows);
        assert_eq!(r.sub_accounts, 1);
        assert_eq!(r.chart.account_count(), 2);
        assert!(r.default_levels_added);
        assert_eq!(r.chart.level_count(), 2);
        let kept = read_with(
            rows,
            &CoaOptions {
                sub_accounts: true,
                default_levels: false,
                coding: None,
            },
        );
        assert_eq!(kept.chart.account_count(), 3);
        assert_eq!(coding(&kept), (2, 2, 2, Some('-')));
        assert_eq!(kept.chart.level_count(), 0);
    }

    #[test]
    fn a_pattern_can_be_asked_for() {
        let r = read_with(
            &[
                &["Account", "Description"],
                &["1100", "STORY"],
                &["1101", "Rights"],
            ],
            &CoaOptions {
                coding: Some(AccountCoding::default()),
                ..CoaOptions::default()
            },
        );
        assert_eq!(lines(&r)[1], "  11-01 Rights");
        assert_eq!(r.inferred.separator, None);
    }

    #[test]
    fn categories_written_with_zero_accounts_and_continued_headings() {
        let r = read(&[
            &["Account #", "Description", "Qualified"],
            &["101-00", "STORY & RIGHTS", ""],
            &["101-01", "Story rights", "NO"],
            &["101-02", "Acquisition", "NO"],
            &["101-00 STORY & RIGHTS (cont.)", "", ""],
            &["101-08", "Copyright", "NO"],
            &["PRODUCTION", "", ""],
            &["112-00", "PRODUCTION STAFF", ""],
            &["112-01", "Production manager", "YES"],
        ]);
        assert_eq!(
            lines(&r),
            [
                "101 STORY & RIGHTS",
                "  101-01 Story rights",
                "  101-02 Acquisition",
                "  101-08 Copyright",
                "= Total above-the-line",
                "112 PRODUCTION STAFF",
                "  112-01 Production manager",
                "= Total production",
            ]
        );
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    }

    #[test]
    fn templates_build_the_same_bytes_every_time() {
        let csv =
            b",ABOVE THE LINE\n10,Story\n10-01,Rights\n,PRODUCTION\n20,Staff\n20-01,Manager\n";
        let a = build_template(csv, "chart.csv", None, "Chart", "USD")
            .unwrap()
            .0;
        let b = build_template(csv, "chart.csv", None, "Chart", "USD")
            .unwrap()
            .0;
        assert_eq!(crate::to_string(&a), crate::to_string(&b));
        assert_eq!(a.topsheet.len(), 4);
        // A budget file is read as its own chart.
        let text = crate::to_string(&a);
        let again = build_template(text.as_bytes(), "x.mthd", None, "Chart", "USD")
            .unwrap()
            .0;
        assert_eq!(crate::to_string(&again), text);
    }

    #[test]
    fn the_kind_code_name_parent_layout() {
        let csv = "kind,code,name,parent,notes\r\n\
category,1100,STORY & RIGHTS,,\r\n\
account,1101,Story rights,1100,ignored\r\n\
subaccount,1101.05,Option payments,1101,\r\n\
account,1102,\"Writers, staff\",1100,\r\n\
category,1300,DIRECTOR,,\r\n\
account,1301,Director,1300,\r\n\
total,,TOTAL ABOVE-THE-LINE,,\r\n\
category,2000,PRODUCTION STAFF,,\r\n\
account,2001,Production manager,2000,\r\n\
total,,TOTAL PRODUCTION,,\r\n\
total,,TOTAL BELOW-THE-LINE,,\r\n";
        let book = crate::import::read_file(csv.as_bytes(), "Studio Features.csv").unwrap();
        let t = &book.tables[0];
        let h = detect_header(t);
        assert_eq!(h, Some(0));
        let m = CoaMapping {
            header_row: h,
            columns: guess(t, h),
        };
        let r = read_chart(t, &m, &CoaOptions::default()).unwrap();
        assert_eq!(
            lines(&r),
            [
                "1100 STORY & RIGHTS",
                "  1101 Story rights",
                "  1101.05 Option payments",
                "  1102 Writers, staff",
                "1300 DIRECTOR",
                "  1301 Director",
                "= Total above-the-line",
                "2000 PRODUCTION STAFF",
                "  2001 Production manager",
                "= Total production",
            ]
        );
        assert_eq!(coding(&r), (2, 2, 0, None));
        assert_eq!(r.sub_accounts, 1);
        let msgs: Vec<&str> = r.warnings.iter().map(|w| w.message.as_str()).collect();
        assert!(
            msgs.iter()
                .any(|m| m.contains("sub-account was kept as an account")
                    || m.contains("sub-account was kept")),
            "{msgs:?}"
        );
        assert!(
            msgs.iter().any(|m| m.contains("totals other totals")),
            "{msgs:?}"
        );
        assert!(r.skipped.is_empty(), "{:?}", r.skipped);
        // Separators kept as written.
        let csv = "kind,code,name,parent\ncategory,11,Producers,\naccount,11-01,Producer,11\nsubaccount,11-01-05,Fee,11-01\n";
        let (b, _) = build_template(csv.as_bytes(), "Other.csv", None, "Other", "USD").unwrap();
        let codes: Vec<&str> = b
            .categories()
            .flat_map(|c| &c.accounts)
            .map(|a| a.code.as_str())
            .collect();
        assert_eq!(codes, ["11-01", "11-01-05"]);
    }

    fn kinded(csv: &str) -> CoaRead {
        let book = crate::import::read_file(csv.as_bytes(), "Chart.csv").unwrap();
        let t = &book.tables[0];
        let (h, cols) = kinded_columns(t, None).unwrap();
        read_kinded(
            t,
            h,
            cols,
            &CoaOptions {
                default_levels: false,
                ..CoaOptions::default()
            },
        )
        .unwrap()
    }

    #[test]
    fn named_parents_keep_every_number_as_written() {
        // Mixed widths (`050101` and `1101` under 4-wide categories),
        // letters, trailing dots, categories and accounts of one width,
        // and a category with a separator inside.
        let r = kinded(
            "kind,code,name,parent\n\
category,0501,BONUSES,\n\
account,050101,.,0501\n\
account,050199,SIGNING BONUS,0501\n\
category,0502,RELOCATION,\n\
category,1100,WRITING,\n\
account,1101,STUDIO WRITER,1100\n\
account,110201,FREELANCE,1100\n\
account,1150,RESEARCH,1100\n\
category,1A00,REVENUE,\n\
account,1A10,BOX OFFICE,1A00\n\
account,1A20,DEDUCTIONS,1A00\n\
category,4AA0,CAMERA,\n\
account,4AA.,,4AA0\n\
account,4AA1,OPERATOR,4AA0\n\
category,5600-100,POST,\n\
account,5600-101,EDITOR,5600-100\n\
total,,TOTAL BELOW-THE-LINE,\n",
        );
        assert_eq!(
            lines(&r),
            [
                "0501 BONUSES",
                "  050199 SIGNING BONUS",
                "0502 RELOCATION",
                "1100 WRITING",
                "  1101 STUDIO WRITER",
                "  110201 FREELANCE",
                "  1150 RESEARCH",
                "1A00 REVENUE",
                "  1A10 BOX OFFICE",
                "  1A20 DEDUCTIONS",
                "4AA0 CAMERA",
                "  4AA. ",
                "  4AA1 OPERATOR",
                "5600-100 POST",
                "  5600-101 EDITOR",
                "= Total below-the-line",
            ]
        );
        assert!(r.skipped.is_empty(), "{:?}", r.skipped);
        assert_eq!(r.placeholders, 1);
        let msgs: Vec<&str> = r.warnings.iter().map(|w| w.message.as_str()).collect();
        assert!(
            msgs.iter()
                .any(|m| m.contains("category 0502 has no accounts")),
            "{msgs:?}"
        );
        assert!(
            msgs.iter().any(|m| m.contains("account 4AA. has no name")),
            "{msgs:?}"
        );
        assert!(msgs.iter().any(|m| m.contains("named \".\"")), "{msgs:?}");
        // The pattern most numbers follow, for typing new ones.
        assert_eq!(coding(&r), (2, 2, 0, None));
        // The chart becomes a budget that recalculates and matches by number.
        let b = r.chart.to_budget("Chart", "USD");
        assert_eq!(
            fwb_core::recalc(&b, &fwb_core::RecalcOptions::default())
                .unwrap()
                .grand_total,
            0
        );
        let chart = fwb_core::actuals::chart(&b);
        assert_eq!(
            chart.values().filter(|e| !e.fringe).count(),
            r.chart.account_count()
        );
    }

    #[test]
    fn without_parents_accounts_go_in_the_category_above() {
        let r = kinded(
            "kind,code,name\n\
category,1100,SCRIPT,\n\
account,1101,SCRIPT\n\
account,1102,RESEARCH\n\
category,1200,PRODUCERS\n\
account,1201,PRODUCER\n",
        );
        assert_eq!(
            lines(&r),
            [
                "1100 SCRIPT",
                "  1101 SCRIPT",
                "  1102 RESEARCH",
                "1200 PRODUCERS",
                "  1201 PRODUCER"
            ]
        );
        // An account before any category, or naming one that isn't there.
        let r = kinded(
            "kind,code,name,parent\naccount,0101,Early,\ncategory,1100,SCRIPT,\naccount,1101,Writer,9900\naccount,1102,Editor,11-00\n",
        );
        assert_eq!(lines(&r), ["1100 SCRIPT", "  1102 Editor"]);
        let skipped: Vec<&str> = r.skipped.iter().map(|w| w.message.as_str()).collect();
        assert_eq!(skipped.len(), 2, "{skipped:?}");
    }

    #[test]
    fn repeated_numbers_merge_or_get_a_new_number() {
        let r = kinded(
            "kind,code,name,parent\n\
category,10900,LOCATIONS,\n\
account,10950,RENTALS,10900\n\
account,10950,REHEARSAL HALL RENTALS,10900\n\
account,10950,VIDEO EQUIPMENT RENTALS,10900\n\
account,10950-3,TAKEN,10900\n\
account,10950,Rentals,10900\n\
category,11-00,STAFF,\n\
account,11-01,MANAGER,11-00\n\
account,11-01,COORDINATOR,11-00\n",
        );
        assert_eq!(
            lines(&r),
            [
                "10900 LOCATIONS",
                "  10950 RENTALS",
                "  10950-2 REHEARSAL HALL RENTALS",
                "  10950-4 VIDEO EQUIPMENT RENTALS",
                "  10950-3 TAKEN",
                "11-00 STAFF",
                "  11-01 MANAGER",
                "  11-01-2 COORDINATOR",
            ]
        );
        assert_eq!(r.merged_repeats, 1);
        let renamed: Vec<(&str, &str)> = r
            .renamed_repeats
            .iter()
            .map(|x| (x.old.as_str(), x.new.as_str()))
            .collect();
        assert_eq!(
            renamed,
            [
                ("10950", "10950-2"),
                ("10950", "10950-4"),
                ("11-01", "11-01-2")
            ]
        );
        // Each is its own account to everything that matches by number.
        let b = r.chart.to_budget("Chart", "USD");
        let chart = fwb_core::actuals::chart(&b);
        assert_eq!(chart.values().filter(|e| !e.fringe).count(), 6);
    }

    #[test]
    fn a_level_column() {
        let r = read(&[
            &["Level", "Account", "Description"],
            &["ATL", "1100", "STORY"],
            &["ATL", "1101", "Rights"],
            &["BTL", "2000", "STAFF"],
            &["BTL", "2001", "Manager"],
        ]);
        assert_eq!(
            lines(&r),
            [
                "11 STORY",
                "  1101 Rights",
                "= Total above-the-line",
                "20 STAFF",
                "  2001 Manager",
                "= Total below-the-line"
            ]
        );
    }

    #[test]
    fn mapping_files_parse() {
        let f = CoaMappingFile::parse(
            r#"{"sheet": 2, "header_row": 3, "columns": {"account_code": "B", "account_name": 3},
                "coding": {"category": 2, "account": 2, "separator": "-"}, "sub_accounts": true}"#,
        )
        .unwrap();
        assert_eq!(f.sheet, Some(1));
        assert_eq!(f.header_row, Some(Some(2)));
        assert_eq!(f.columns.as_ref().unwrap()[&CoaField::AccountCode], 1);
        assert_eq!(f.options.coding, Some(AccountCoding::default()));
        assert!(f.options.sub_accounts && f.options.default_levels);
        for bad in [
            r#"{"columns": {"rate": 1}}"#,
            r#"{"coding": {"category": 9, "account": 2}}"#,
            r#"{"sheet": 0}"#,
            r#"{"other": 1}"#,
        ] {
            assert!(CoaMappingFile::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn code_like_cells() {
        for yes in ["1101", "11-01", "011.001", "A100", "11 01"] {
            assert!(code_like(yes), "{yes}");
        }
        for no in ["Story", "", "Total 1100", "1101 Story rights", "-5", "$100"] {
            assert!(!code_like(no), "{no}");
        }
    }
}
