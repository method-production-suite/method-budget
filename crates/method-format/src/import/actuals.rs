//! Actuals from a spreadsheet (roadmap 3.7): the transactions in an
//! accounting system's export (CSV, tab-delimited or an Excel workbook,
//! read by the same readers as [`super::merge`]), mapped column by column,
//! matched to the budget's accounts and checked for duplicates, before they
//! go into the budget as one import ([`method_core::actuals::add_batch`]).
//!
//! How rows are read:
//!
//! - Rows after the header row; rows with nothing in any mapped column are
//!   passed over.
//! - **Amount**: the Amount column, else Debit minus Credit. Read exactly
//!   (no floats): number cells as written, text with currency signs,
//!   thousands separators and `(5)` for -5, rounded once to the cent. With
//!   [`Options::negate`], signs flip (for exports that write costs as
//!   negative numbers). A row without an amount, or with zero, is skipped.
//! - **Date**: a workbook date (a serial number, in the 1904 date system
//!   when the workbook uses it), ISO `2026-09-30`,
//!   `2026/09/30`, `20260930`, `9/30/2026` or `30/9/2026` (which one
//!   [`DateOrder`] says; found from the column when it can be), `30.9.2026`
//!   (day first), and dates with month names (`Sep 30, 2026`,
//!   `30-Sep-26`). Two-digit years 00 to 29 are 2000 to 2029, 30 to 99 are
//!   1930 to 1999. A time after the date is ignored. A date this can't read
//!   is left blank and the row says so.
//! - **Account**: read with the budget's account numbering (`1101`, `11-1`
//!   and `11.01` are all `11-01` under the default pattern) and matched to
//!   the chart's accounts, and its category fringe accounts when fringes
//!   post by category. A number the chart doesn't have (or a blank one) is
//!   listed for review: map it to an account, skip its rows, or keep them
//!   unassigned (the default; the number stays on them, so adding that
//!   account later picks them up). With [`Options::carry_account`] an empty
//!   account cell takes the account above (for reports that give the
//!   account once per block).
//! - **Total rows**: a row with no date and no account, or with no date and
//!   a vendor, description or reference that starts with "total", is
//!   skipped as a total row (when there is a date or account column).
//! - **Duplicates**: by date, account, amount and description (see
//!   [`method_core::actuals::DuplicateKey`]). Rows matching a transaction the
//!   budget already has are skipped unless [`Options::skip_existing`] is
//!   off (so importing the same file twice adds nothing); rows repeating an
//!   earlier row of the file are imported unless [`Options::skip_repeats`]
//!   is on (two equal purchases on one day are real). Both are listed.

use std::collections::{BTreeMap, BTreeSet};

use method_core::actuals::{
    self, duplicate_key, duplicate_keys, iso_date, normalize_code, Transaction, MAX_TEXT_CHARS,
};
use method_core::{Budget, Cents};

use super::mapping::{cell_list, cell_number, norm};
use super::merge::{fit_account, RowNote};
use super::{Table, Value};

/// A field an actuals column can hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Field {
    Date,
    Account,
    /// The transaction's amount (positive is a cost).
    Amount,
    /// A cost, with Credit: the amount is debit minus credit.
    Debit,
    /// A refund or credit, taken off.
    Credit,
    Vendor,
    Description,
    /// Invoice, purchase order or check number.
    Reference,
    /// Labels, separated by commas.
    Tags,
}

impl Field {
    pub const ALL: [Field; 9] = [
        Field::Date,
        Field::Account,
        Field::Amount,
        Field::Debit,
        Field::Credit,
        Field::Vendor,
        Field::Description,
        Field::Reference,
        Field::Tags,
    ];

    /// The field's name between the app and the page.
    pub fn key(self) -> &'static str {
        match self {
            Field::Date => "date",
            Field::Account => "account",
            Field::Amount => "amount",
            Field::Debit => "debit",
            Field::Credit => "credit",
            Field::Vendor => "vendor",
            Field::Description => "description",
            Field::Reference => "reference",
            Field::Tags => "tags",
        }
    }

    pub fn parse(key: &str) -> Option<Field> {
        Field::ALL.into_iter().find(|f| f.key() == key)
    }

    /// The field in a sentence.
    pub fn name(self) -> &'static str {
        match self {
            Field::Date => "Date",
            Field::Account => "Account number",
            Field::Amount => "Amount",
            Field::Debit => "Debit",
            Field::Credit => "Credit",
            Field::Vendor => "Vendor",
            Field::Description => "Description",
            Field::Reference => "Reference",
            Field::Tags => "Tags",
        }
    }

    /// Column titles that mean this field, normalized ([`norm`]), most
    /// telling first (as in [`super::mapping`]).
    fn synonyms(self) -> &'static [&'static str] {
        match self {
            Field::Date => &[
                "date",
                "transactiondate",
                "trandate",
                "postingdate",
                "postdate",
                "posted",
                "invoicedate",
                "documentdate",
                "docdate",
                "paiddate",
                "checkdate",
                "paymentdate",
                "gldate",
                "period",
            ],
            Field::Account => &[
                "account",
                "accountnumber",
                "accountno",
                "account#",
                "accountcode",
                "acct",
                "acctnumber",
                "acctno",
                "acct#",
                "acctcode",
                "glaccount",
                "glcode",
                "gl",
                "costcode",
                "coa",
                "budgetaccount",
                "budgetcode",
            ],
            Field::Amount => &[
                "amount",
                "amt",
                "netamount",
                "net",
                "total",
                "actual",
                "actualcost",
                "actualamount",
                "cost",
                "value",
                "sum",
                "extended",
            ],
            Field::Debit => &["debit", "debits", "dr", "debitamount", "charges"],
            Field::Credit => &["credit", "credits", "cr", "creditamount", "refunds"],
            Field::Vendor => &[
                "vendor",
                "vendorname",
                "payee",
                "payeename",
                "supplier",
                "suppliername",
                "paidto",
                "merchant",
                "name",
            ],
            Field::Description => &[
                "description",
                "desc",
                "memo",
                "linememo",
                "details",
                "detail",
                "narrative",
                "lineitem",
                "item",
                "comment",
                "comments",
                "notes",
            ],
            Field::Reference => &[
                "reference",
                "ref",
                "ref#",
                "refno",
                "invoice",
                "invoice#",
                "invoiceno",
                "invoicenumber",
                "po",
                "po#",
                "ponumber",
                "check",
                "check#",
                "checkno",
                "checknumber",
                "document",
                "document#",
                "docno",
                "transactionid",
                "transaction#",
                "num",
                "number",
                "no",
            ],
            Field::Tags => &["tags", "tag", "labels", "label", "class", "type"],
        }
    }
}

/// How well `title` (normalized) names `field`: 0 not at all.
fn score(field: Field, title: &str) -> u32 {
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

/// The header row and each mapped field's column.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Mapping {
    /// The row holding the column titles; data starts after it. `None`: no
    /// titles, data from the first row.
    pub header_row: Option<usize>,
    pub columns: BTreeMap<Field, usize>,
}

impl Mapping {
    pub fn column(&self, f: Field) -> Option<usize> {
        self.columns.get(&f).copied()
    }

    pub fn first_data_row(&self) -> usize {
        self.header_row.map_or(0, |h| h + 1)
    }
}

/// How many rows from the top are looked at for the header.
const HEADER_SCAN: usize = 30;

/// The row of column titles: among the first rows, the one whose text
/// cells best name actuals fields. `None` when no row names two.
pub fn detect_header(t: &Table) -> Option<usize> {
    let mut best: Option<(u32, usize)> = None;
    for r in 0..t.rows.len().min(HEADER_SCAN) {
        let titles: Vec<String> = t.rows[r]
            .iter()
            .filter(|v| matches!(v, Value::Text(_)))
            .map(|v| norm(v.text()))
            .filter(|s| !s.is_empty())
            .collect();
        let mut used = BTreeSet::new();
        let mut total = 0;
        let mut named = 0;
        for title in &titles {
            if let Some((s, f)) = Field::ALL
                .iter()
                .map(|&f| (score(f, title), f))
                .filter(|(s, f)| *s > 0 && !used.contains(f))
                .max_by_key(|(s, _)| *s)
            {
                used.insert(f);
                total += s;
                named += 1;
            }
        }
        if named >= 2 && best.is_none_or(|(b, _)| total > b) {
            best = Some((total, r));
        }
    }
    best.map(|(_, r)| r)
}

/// The mapping the titles suggest: each field to the column whose title
/// names it best, each column used once.
pub fn guess(t: &Table, header_row: Option<usize>) -> BTreeMap<Field, usize> {
    let titles: Vec<String> = super::mapping::titles(t, header_row)
        .iter()
        .map(|s| norm(s))
        .collect();
    let mut cands: Vec<(u32, usize, Field)> = Vec::new();
    for (c, title) in titles.iter().enumerate() {
        for f in Field::ALL {
            let s = score(f, title);
            if s > 0 {
                cands.push((s, c, f));
            }
        }
    }
    cands.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    let mut out = BTreeMap::new();
    let mut taken = BTreeSet::new();
    for (_, c, f) in cands {
        if out.contains_key(&f) || taken.contains(&c) {
            continue;
        }
        out.insert(f, c);
        taken.insert(c);
    }
    // An amount column makes debit and credit columns redundant guesses
    // only when they are the same column (they can't be): keep them all.
    out
}

// ---------------------------------------------------------------------------
// Dates

/// Which part of `1/2/2026` is the month.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DateOrder {
    /// `9/30/2026` (United States).
    #[default]
    MonthFirst,
    /// `30/9/2026`.
    DayFirst,
}

impl DateOrder {
    pub fn key(self) -> &'static str {
        match self {
            DateOrder::MonthFirst => "month_first",
            DateOrder::DayFirst => "day_first",
        }
    }

    pub fn parse(s: &str) -> Option<DateOrder> {
        match s {
            "month_first" => Some(DateOrder::MonthFirst),
            "day_first" => Some(DateOrder::DayFirst),
            _ => None,
        }
    }
}

/// The civil date `days` after 1970-01-01 (Howard Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// A workbook's date serial number (the 1900 date system) as `YYYY-MM-DD`:
/// serial 1 is 1900-01-01, and serial 60 is Lotus's 1900-02-29, which
/// doesn't exist (`None`). The fraction (a time of day) is ignored.
pub fn serial_date(serial: i64) -> Option<String> {
    if !(1..=2_958_465).contains(&serial) || serial == 60 {
        return None;
    }
    // From 61 on, serial n is n days after 1899-12-30; before the made-up
    // leap day, one day later.
    let offset = if serial < 60 { serial - 1 } else { serial - 2 };
    // 1900-01-01 is day -25567 from 1970-01-01.
    let (y, m, d) = civil_from_days(-25_567 + offset);
    iso_date(y as i32, m, d)
}

/// Days between the 1900 and 1904 date systems: 1904-01-01 is serial 1,462
/// in the first and serial 0 in the second.
const DAYS_1900_TO_1904: i64 = 1_462;

/// A date serial number of the 1904 date system (`date1904` workbooks) as
/// `YYYY-MM-DD`: serial 0 is 1904-01-01, up to 9999-12-31. The fraction (a
/// time of day) is ignored.
pub fn serial_date_1904(serial: i64) -> Option<String> {
    if !(0..=2_958_465 - DAYS_1900_TO_1904).contains(&serial) {
        return None;
    }
    // Every 1904 date is past the 1900 system's made-up leap day.
    serial_date(serial + DAYS_1900_TO_1904)
}

const MONTHS: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];

fn month_of(word: &str) -> Option<u32> {
    let w = word.to_ascii_lowercase();
    if w.len() < 3 {
        return None;
    }
    let full = [
        "january",
        "february",
        "march",
        "april",
        "may",
        "june",
        "july",
        "august",
        "september",
        "october",
        "november",
        "december",
    ];
    for (i, (short, long)) in MONTHS.iter().zip(full).enumerate() {
        if long.starts_with(&w) && w.starts_with(short) || (w == "sept" && i == 8) {
            return Some(i as u32 + 1);
        }
    }
    None
}

fn year_of(digits: &str) -> Option<i32> {
    let n: i32 = digits.parse().ok()?;
    match digits.len() {
        4 => Some(n),
        2 if n < 30 => Some(2000 + n),
        2 => Some(1900 + n),
        _ => None,
    }
}

/// The date part of a cell's text: up to a `T` or space followed by a time.
fn date_part(s: &str) -> &str {
    let s = s.trim();
    let cut = s
        .char_indices()
        .find(|&(i, c)| {
            (c == 'T' || c == ' ')
                && s[i + 1..].contains(':')
                && s[..i].chars().last().is_some_and(|p| p.is_ascii_digit())
        })
        .map_or(s.len(), |(i, _)| i);
    s[..cut].trim()
}

/// Reads typed date text (see the module docs), or `None`.
pub fn text_date(s: &str, order: DateOrder) -> Option<String> {
    let s = date_part(s);
    if s.is_empty() {
        return None;
    }
    let digits_only = s.bytes().all(|b| b.is_ascii_digit());
    if digits_only && s.len() == 8 {
        return iso_date(
            s[..4].parse().ok()?,
            s[4..6].parse().ok()?,
            s[6..].parse().ok()?,
        );
    }
    if digits_only && s.len() == 5 {
        return serial_date(s.parse().ok()?);
    }
    // Pieces of digits and of letters; anything else separates them.
    let mut pieces: Vec<&str> = Vec::new();
    let mut start: Option<usize> = None;
    let mut seps: Vec<char> = Vec::new();
    for (i, c) in s.char_indices() {
        let kind = |c: char| {
            if c.is_ascii_digit() {
                1
            } else if c.is_alphabetic() {
                2
            } else {
                0
            }
        };
        match start {
            Some(st) if kind(c) == kind(s[st..].chars().next().unwrap_or(' ')) => {}
            Some(st) => {
                pieces.push(&s[st..i]);
                start = None;
                if kind(c) != 0 {
                    start = Some(i);
                } else {
                    seps.push(c);
                }
            }
            None if kind(c) != 0 => start = Some(i),
            None => seps.push(c),
        }
    }
    if let Some(st) = start {
        pieces.push(&s[st..]);
    }
    // Ordinal suffixes ("30th") and weekday names are noise.
    let pieces: Vec<&str> = pieces
        .into_iter()
        .filter(|p| !matches!(p.to_ascii_lowercase().as_str(), "st" | "nd" | "rd" | "th"))
        .filter(|p| {
            let l = p.to_ascii_lowercase();
            !["mon", "tue", "wed", "thu", "fri", "sat", "sun"]
                .iter()
                .any(|d| l.starts_with(d) && month_of(&l).is_none())
        })
        .collect();
    if pieces.len() != 3 {
        return None;
    }
    let numeric = |p: &str| p.bytes().all(|b| b.is_ascii_digit());
    if let Some(mi) = pieces.iter().position(|p| !numeric(p)) {
        // A month name, a day and a year.
        let month = month_of(pieces[mi])?;
        let nums: Vec<&str> = pieces.iter().copied().filter(|p| numeric(p)).collect();
        if nums.len() != 2 {
            return None;
        }
        let (day, year) = if nums[0].len() == 4 {
            (nums[1], nums[0])
        } else {
            (nums[0], nums[1])
        };
        if day.len() > 2 {
            return None;
        }
        return iso_date(year_of(year)?, month, day.parse().ok()?);
    }
    let [a, b, c] = [pieces[0], pieces[1], pieces[2]];
    if a.len() == 4 {
        // Year first: always year, month, day.
        if b.len() > 2 || c.len() > 2 {
            return None;
        }
        return iso_date(a.parse().ok()?, b.parse().ok()?, c.parse().ok()?);
    }
    if a.len() > 2 || b.len() > 2 {
        return None;
    }
    let year = year_of(c)?;
    let (x, y): (u32, u32) = (a.parse().ok()?, b.parse().ok()?);
    let day_first = seps.first() == Some(&'.') || order == DateOrder::DayFirst;
    if day_first {
        iso_date(year, y, x)
    } else {
        iso_date(year, x, y)
    }
}

/// A date cell as `YYYY-MM-DD`: `Ok(None)` when empty, `Err` when it isn't
/// a date this reads. A number is a serial of the 1900 date system; see
/// [`cell_date_in`] for a workbook in the 1904 system.
pub fn cell_date(v: &Value, order: DateOrder) -> Result<Option<String>, String> {
    cell_date_in(v, order, false)
}

/// As [`cell_date`], with numbers read as serials of the 1904 date system
/// when `date1904` (the table's [`Table::date1904`]). Text is read the same
/// either way.
pub fn cell_date_in(v: &Value, order: DateOrder, date1904: bool) -> Result<Option<String>, String> {
    let bad = || format!("{:?} isn't a date this import reads", v.text().trim());
    match v {
        Value::Empty => Ok(None),
        Value::Text(s) if s.trim().is_empty() => Ok(None),
        Value::Number(s) => {
            let d = super::mapping::parse_decimal(s).map_err(|_| bad())?;
            let whole = d.to_string();
            let int = whole.split('.').next().unwrap_or("");
            let serial: i64 = int.parse().map_err(|_| bad())?;
            // An 8-digit number is a compact date (20260930).
            if (19_000_101..=29_991_231).contains(&serial) {
                return text_date(int, order).map(Some).ok_or_else(bad);
            }
            let date = if date1904 {
                serial_date_1904(serial)
            } else {
                serial_date(serial)
            };
            date.map(Some).ok_or_else(bad)
        }
        Value::Text(s) => text_date(s, order).map(Some).ok_or_else(bad),
        Value::Bool(_) => Err(bad()),
    }
}

/// The order a column's slash or dash dates are in, when some date in it
/// can only be read one way: a first part over 12 means day first, a
/// second part over 12 month first. `None` when none tells (or they
/// disagree).
pub fn detect_date_order(t: &Table, col: usize, from: usize) -> Option<DateOrder> {
    let mut day_first = false;
    let mut month_first = false;
    for r in from..t.rows.len() {
        let Value::Text(s) = t.cell(r, col) else {
            continue;
        };
        let s = date_part(s);
        let parts: Vec<&str> = s.split(['/', '-']).collect();
        if parts.len() != 3 || parts[0].len() > 2 || parts[1].len() > 2 {
            continue;
        }
        let (Ok(a), Ok(b)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) else {
            continue;
        };
        if a > 12 && a <= 31 {
            day_first = true;
        }
        if b > 12 && b <= 31 {
            month_first = true;
        }
    }
    match (day_first, month_first) {
        (true, false) => Some(DateOrder::DayFirst),
        (false, true) => Some(DateOrder::MonthFirst),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Planning

/// What to do with the rows of an account number the chart doesn't have.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum CodeChoice {
    /// Import them unassigned, keeping the number (the default).
    #[default]
    Keep,
    /// Leave them out.
    Skip,
    /// Import them into this account (any spelling of a number in the chart).
    Map(String),
}

/// Choices beyond the mapping.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    /// `None`: the order found from the date column, else month first.
    pub date_order: Option<DateOrder>,
    /// Flip every amount's sign.
    pub negate: bool,
    /// An empty account cell takes the account above.
    pub carry_account: bool,
    /// What to do with each account number the chart doesn't have, by its
    /// key in [`Plan::unmatched`] (`""` for rows without one).
    pub codes: BTreeMap<String, CodeChoice>,
    /// Skip rows that match a transaction the budget already has.
    pub skip_existing: bool,
    /// Skip rows that repeat an earlier row of the file.
    pub skip_repeats: bool,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            date_order: None,
            negate: false,
            carry_account: false,
            codes: BTreeMap::new(),
            skip_existing: true,
            skip_repeats: false,
        }
    }
}

/// An account number the chart doesn't have, with what its rows add up to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unmatched {
    /// The number as the budget's pattern writes it when it fits, else as
    /// written (`""`: rows with no account).
    pub key: String,
    /// How the file first wrote it.
    pub text: String,
    pub rows: usize,
    pub amount: Cents,
    /// The choice in effect.
    pub choice: CodeChoice,
}

/// A row that matches a transaction the budget has, or an earlier row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Duplicate {
    pub row: usize,
    /// The earlier row of the file it repeats; `None`: it matches a
    /// transaction already in the budget.
    pub repeats: Option<usize>,
    /// Left out of the import.
    pub skipped: bool,
    pub date: String,
    pub account: String,
    pub amount: Cents,
    pub description: String,
}

/// A row that becomes a transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Planned {
    /// Its row number in the sheet (from 1).
    pub row: usize,
    /// Its id and batch are set when it is imported.
    pub transaction: Transaction,
    /// Its account is in the chart.
    pub assigned: bool,
}

/// What importing a mapped table would do.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub transactions: Vec<Planned>,
    /// Rows that won't be imported, and why.
    pub skipped: Vec<RowNote>,
    /// Rows imported with something left out or changed.
    pub warnings: Vec<RowNote>,
    /// Rows that only name the account for the rows below.
    pub headings: usize,
    /// Account numbers the chart doesn't have, by key.
    pub unmatched: Vec<Unmatched>,
    pub duplicates: Vec<Duplicate>,
    /// The date order in effect, and whether it was found from the column.
    pub date_order: DateOrder,
    pub date_order_found: bool,
    /// What the imported transactions add up to.
    pub total: Cents,
    /// Accounts in the chart that get transactions.
    pub accounts: BTreeSet<String>,
}

fn clip_text(s: &str) -> (String, bool) {
    let t = s.trim();
    match t.char_indices().nth(MAX_TEXT_CHARS) {
        Some((at, _)) => (t[..at].to_string(), true),
        None => (t.to_string(), false),
    }
}

fn is_total_text(s: &str) -> bool {
    s.trim().to_lowercase().starts_with("total")
}

/// What importing `t` with mapping `m` into `b` would do. `b` is not
/// changed.
pub fn plan(b: &Budget, t: &Table, m: &Mapping, o: &Options) -> Plan {
    let coding = b.account_coding;
    let chart = actuals::chart(b);
    let existing = duplicate_keys(&b.actuals, &coding);
    let first = m.first_data_row();
    let found = m
        .column(Field::Date)
        .and_then(|c| detect_date_order(t, c, first));
    let order = o.date_order.or(found).unwrap_or_default();
    let mut p = Plan {
        date_order: order,
        date_order_found: o.date_order.is_none() && found.is_some(),
        ..Plan::default()
    };
    let cell = |r: usize, f: Field| -> &Value {
        match m.column(f) {
            Some(c) => t.cell(r, c),
            None => &Value::Empty,
        }
    };
    let text = |r: usize, f: Field| cell(r, f).text().trim().to_string();
    let mapped_columns: Vec<usize> = m.columns.values().copied().collect();
    let has_date = m.column(Field::Date).is_some();
    let has_account = m.column(Field::Account).is_some();
    let mut unmatched: BTreeMap<String, Unmatched> = BTreeMap::new();
    let mut seen: BTreeMap<actuals::DuplicateKey, usize> = BTreeMap::new();
    let mut carried = String::new();
    for r in first..t.rows.len() {
        let row = r + 1;
        if mapped_columns.iter().all(|&c| t.cell(r, c).is_empty()) {
            continue;
        }
        let mut account_text = text(r, Field::Account);
        if account_text.is_empty() && o.carry_account {
            account_text = carried.clone();
        } else if !account_text.is_empty() {
            carried = account_text.clone();
        }
        // The amount: Amount, else debit minus credit.
        let money = |f: Field| -> Result<Option<Cents>, String> {
            match cell_number(cell(r, f), t.decimal_comma)? {
                None => Ok(None),
                Some(d) => d
                    .to_cents()
                    .map(Some)
                    .ok_or_else(|| "is too large".to_string()),
            }
        };
        let amount = match money(Field::Amount) {
            Err(e) => Err(format!("the amount {e}")),
            Ok(Some(a)) => Ok(Some(a)),
            Ok(None) => match (money(Field::Debit), money(Field::Credit)) {
                (Err(e), _) => Err(format!("the debit {e}")),
                (_, Err(e)) => Err(format!("the credit {e}")),
                (Ok(None), Ok(None)) => Ok(None),
                (Ok(d), Ok(c)) => d
                    .unwrap_or(0)
                    .checked_sub(c.unwrap_or(0))
                    .map(Some)
                    .ok_or_else(|| "the amount is too large".to_string()),
            },
        };
        let amount = match amount {
            Err(e) => {
                p.skipped.push(RowNote { row, message: e });
                continue;
            }
            Ok(None) => {
                let only_account = !text(r, Field::Account).is_empty()
                    && [
                        Field::Date,
                        Field::Vendor,
                        Field::Description,
                        Field::Reference,
                    ]
                    .iter()
                    .all(|f| cell(r, *f).is_empty());
                if only_account && o.carry_account {
                    p.headings += 1;
                } else {
                    p.skipped.push(RowNote {
                        row,
                        message: "no amount".into(),
                    });
                }
                continue;
            }
            Ok(Some(0)) => {
                p.skipped.push(RowNote {
                    row,
                    message: "the amount is zero".into(),
                });
                continue;
            }
            Ok(Some(a)) => a,
        };
        let amount = if o.negate {
            match amount.checked_neg() {
                Some(a) => a,
                None => {
                    p.skipped.push(RowNote {
                        row,
                        message: "the amount is too large".into(),
                    });
                    continue;
                }
            }
        } else {
            amount
        };
        let date = match cell_date_in(cell(r, Field::Date), order, t.date1904) {
            Ok(d) => d.unwrap_or_default(),
            Err(e) => {
                p.warnings.push(RowNote {
                    row,
                    message: format!("{e}; imported without a date"),
                });
                String::new()
            }
        };
        let (vendor, cut_v) = clip_text(&text(r, Field::Vendor));
        let (description, cut_d) = clip_text(&text(r, Field::Description));
        let (reference, cut_r) = clip_text(&text(r, Field::Reference));
        // Total rows: no date, and no account or a "Total..." label.
        let cell_text_empty = cell(r, Field::Date).is_empty();
        if (has_date || has_account) && cell_text_empty {
            let no_account = account_text.is_empty();
            let labelled = [&vendor, &description, &reference, &text(r, Field::Account)]
                .iter()
                .any(|s| is_total_text(s));
            if (no_account && has_date) || labelled {
                p.skipped.push(RowNote {
                    row,
                    message: "a total row (no date, and no account or a Total label)".into(),
                });
                continue;
            }
        }
        if cut_v || cut_d || cut_r {
            p.warnings.push(RowNote {
                row,
                message: format!("text cut to {MAX_TEXT_CHARS} characters"),
            });
        }
        let tags: BTreeSet<String> = cell_list(cell(r, Field::Tags))
            .into_iter()
            .filter(|s| {
                s.chars().count() <= actuals::MAX_TAG_CHARS && !s.chars().any(char::is_control)
            })
            .take(actuals::MAX_TAGS)
            .collect();
        // The account: in the chart, or reviewed.
        let key = if account_text.is_empty() {
            String::new()
        } else {
            fit_account(&coding, &account_text).unwrap_or_else(|_| account_text.clone())
        };
        let (account, assigned) = match chart.get(&key).filter(|_| !key.is_empty()) {
            Some(e) => (e.code.clone(), true),
            None => {
                let choice = o.codes.get(&key).cloned().unwrap_or_default();
                let u = unmatched.entry(key.clone()).or_insert_with(|| Unmatched {
                    key: key.clone(),
                    text: account_text.clone(),
                    rows: 0,
                    amount: 0,
                    choice: choice.clone(),
                });
                u.rows += 1;
                u.amount = u.amount.saturating_add(amount);
                match choice {
                    CodeChoice::Keep => (key.clone(), false),
                    CodeChoice::Skip => {
                        let what = if key.is_empty() {
                            "rows without an account are skipped".to_string()
                        } else {
                            format!("account {key} is skipped")
                        };
                        p.skipped.push(RowNote { row, message: what });
                        continue;
                    }
                    CodeChoice::Map(to) => {
                        match chart
                            .get(&normalize_code(&coding, &to))
                            .filter(|_| !to.trim().is_empty())
                        {
                            Some(e) => (e.code.clone(), true),
                            None => {
                                p.skipped.push(RowNote {
                                    row,
                                    message: format!("this budget has no account {}", to.trim()),
                                });
                                continue;
                            }
                        }
                    }
                }
            }
        };
        let transaction = Transaction {
            date,
            account,
            tags,
            vendor,
            description,
            reference,
            amount,
            ..Transaction::default()
        };
        // Anything the budget would refuse (an account number too long to
        // keep) leaves out the row, not the whole import.
        if let Err(e) = actuals::check_transaction(&transaction) {
            p.skipped.push(RowNote {
                row,
                message: e.to_string(),
            });
            continue;
        }
        // Duplicates.
        let dk = duplicate_key(
            &coding,
            &transaction.date,
            &transaction.account,
            amount,
            &transaction.description,
        );
        let dup = if existing.contains_key(&dk) {
            Some((None, o.skip_existing))
        } else {
            seen.get(&dk).map(|&first| (Some(first), o.skip_repeats))
        };
        seen.entry(dk).or_insert(row);
        if let Some((repeats, skipped)) = dup {
            p.duplicates.push(Duplicate {
                row,
                repeats,
                skipped,
                date: transaction.date.clone(),
                account: transaction.account.clone(),
                amount,
                description: transaction.description.clone(),
            });
            if skipped {
                continue;
            }
        }
        p.total = p.total.saturating_add(amount);
        if assigned {
            p.accounts.insert(transaction.account.clone());
        }
        p.transactions.push(Planned {
            row,
            transaction,
            assigned,
        });
    }
    p.unmatched = unmatched.into_values().collect();
    p
}

/// The transactions a plan imports, in sheet order.
pub fn transactions(p: &Plan) -> Vec<Transaction> {
    p.transactions
        .iter()
        .map(|t| t.transaction.clone())
        .collect()
}

/// Plans and imports in one go (what the app runs, inside one undo step):
/// the transactions go into `b` as one import named `file` at `imported`.
pub fn import(
    b: &mut Budget,
    t: &Table,
    m: &Mapping,
    o: &Options,
    file: &str,
    imported: &str,
) -> Result<(Plan, method_core::actuals::BatchId), String> {
    let p = plan(b, t, m, o);
    if p.transactions.is_empty() {
        return Err(
            "No transactions to import: map the columns, or check the skipped rows.".into(),
        );
    }
    let batch =
        actuals::add_batch(b, file, imported, transactions(&p)).map_err(|e| e.to_string())?;
    Ok((p, batch))
}

#[cfg(test)]
mod tests {
    use super::*;
    use method_core::starter;

    fn table(rows: &[&[&str]]) -> Table {
        let mut t = Table::default();
        for (r, row) in rows.iter().enumerate() {
            for (c, s) in row.iter().enumerate() {
                if !s.is_empty() {
                    t.put(r, c, Value::Text(s.to_string()));
                }
            }
        }
        t
    }

    fn mapping(fields: &[Field]) -> Mapping {
        Mapping {
            header_row: Some(0),
            columns: fields.iter().enumerate().map(|(i, f)| (*f, i)).collect(),
        }
    }

    #[test]
    fn serial_dates_in_the_1904_system() {
        assert_eq!(serial_date_1904(0).as_deref(), Some("1904-01-01"));
        assert_eq!(serial_date_1904(59).as_deref(), Some("1904-02-29"));
        assert_eq!(serial_date_1904(44_833).as_deref(), Some("2026-09-30"));
        assert_eq!(serial_date_1904(2_957_003).as_deref(), Some("9999-12-31"));
        assert_eq!(serial_date_1904(-1), None);
        assert_eq!(serial_date_1904(2_957_004), None);
        let v = Value::Number("44833.5".into());
        assert_eq!(
            cell_date_in(&v, DateOrder::MonthFirst, true).unwrap(),
            Some("2026-09-30".into())
        );
        // The same number in the 1900 system is 1,462 days earlier.
        assert_eq!(
            cell_date_in(&v, DateOrder::MonthFirst, false).unwrap(),
            Some("2022-09-29".into())
        );
        // Text and compact dates don't depend on the system.
        let t = Value::Text("9/30/2026".into());
        assert_eq!(
            cell_date_in(&t, DateOrder::MonthFirst, true).unwrap(),
            Some("2026-09-30".into())
        );
        let c = Value::Number("20260930".into());
        assert_eq!(
            cell_date_in(&c, DateOrder::MonthFirst, true).unwrap(),
            Some("2026-09-30".into())
        );
    }

    #[test]
    fn serial_dates_follow_the_1900_system() {
        assert_eq!(serial_date(1).as_deref(), Some("1900-01-01"));
        assert_eq!(serial_date(59).as_deref(), Some("1900-02-28"));
        assert_eq!(serial_date(60), None, "the made-up leap day");
        assert_eq!(serial_date(61).as_deref(), Some("1900-03-01"));
        assert_eq!(serial_date(25_569).as_deref(), Some("1970-01-01"));
        assert_eq!(serial_date(46_295).as_deref(), Some("2026-09-30"));
        assert_eq!(serial_date(2_958_465).as_deref(), Some("9999-12-31"));
        assert_eq!(serial_date(0), None);
        assert_eq!(serial_date(2_958_466), None);
    }

    #[test]
    fn typed_dates() {
        let m = |s: &str| text_date(s, DateOrder::MonthFirst);
        let dd = |s: &str| text_date(s, DateOrder::DayFirst);
        assert_eq!(m("2026-09-30").as_deref(), Some("2026-09-30"));
        assert_eq!(m("2026-9-3").as_deref(), Some("2026-09-03"));
        assert_eq!(m("2026-09-30T14:30:00Z").as_deref(), Some("2026-09-30"));
        assert_eq!(m("2026/09/30 14:30").as_deref(), Some("2026-09-30"));
        assert_eq!(m("20260930").as_deref(), Some("2026-09-30"));
        assert_eq!(m("9/30/2026").as_deref(), Some("2026-09-30"));
        assert_eq!(m("9/3/26").as_deref(), Some("2026-09-03"));
        assert_eq!(dd("9/3/26").as_deref(), Some("2026-03-09"));
        assert_eq!(dd("30/9/2026").as_deref(), Some("2026-09-30"));
        assert_eq!(
            m("30.9.2026").as_deref(),
            Some("2026-09-30"),
            "dots are day first"
        );
        assert_eq!(m("Sep 30, 2026").as_deref(), Some("2026-09-30"));
        assert_eq!(m("September 30th, 2026").as_deref(), Some("2026-09-30"));
        assert_eq!(m("30 Sept 2026").as_deref(), Some("2026-09-30"));
        assert_eq!(m("30-Sep-26").as_deref(), Some("2026-09-30"));
        assert_eq!(m("Wed, 30 Sep 2026").as_deref(), Some("2026-09-30"));
        assert_eq!(m("1/1/99").as_deref(), Some("1999-01-01"));
        assert_eq!(m("46295").as_deref(), Some("2026-09-30"));
        for bad in [
            "",
            "TBD",
            "13/13/2026",
            "2/30/2026",
            "9/30",
            "Sept",
            "2026-09",
            "1/2/3/4",
            "Mayday 3 4",
        ] {
            assert_eq!(m(bad), None, "{bad}");
        }
        assert_eq!(
            cell_date(&Value::Number("46295.75".into()), DateOrder::MonthFirst).unwrap(),
            Some("2026-09-30".into())
        );
        assert_eq!(
            cell_date(&Value::Number("20260930".into()), DateOrder::MonthFirst).unwrap(),
            Some("2026-09-30".into())
        );
        assert_eq!(
            cell_date(&Value::Empty, DateOrder::MonthFirst).unwrap(),
            None
        );
        assert!(cell_date(&Value::Bool(true), DateOrder::MonthFirst).is_err());
    }

    #[test]
    fn date_order_is_found_from_the_column() {
        let t = table(&[&["Date"], &["1/2/2026"], &["25/2/2026"]]);
        assert_eq!(detect_date_order(&t, 0, 1), Some(DateOrder::DayFirst));
        let t = table(&[&["Date"], &["1/2/2026"], &["2/25/2026"]]);
        assert_eq!(detect_date_order(&t, 0, 1), Some(DateOrder::MonthFirst));
        let t = table(&[&["Date"], &["1/2/2026"]]);
        assert_eq!(detect_date_order(&t, 0, 1), None);
    }

    #[test]
    fn headers_and_columns_are_guessed() {
        let t = table(&[
            &["Acme Productions general ledger"],
            &[],
            &[
                "Trans Date",
                "GL Account",
                "Payee",
                "Memo",
                "Invoice #",
                "Debit",
                "Credit",
            ],
            &["9/1/2026", "1101", "Writer", "Draft", "INV-1", "2500", ""],
        ]);
        assert_eq!(detect_header(&t), Some(2));
        let g = guess(&t, Some(2));
        assert_eq!(g.get(&Field::Date), Some(&0));
        assert_eq!(g.get(&Field::Account), Some(&1));
        assert_eq!(g.get(&Field::Vendor), Some(&2));
        assert_eq!(g.get(&Field::Description), Some(&3));
        assert_eq!(g.get(&Field::Reference), Some(&4));
        assert_eq!(g.get(&Field::Debit), Some(&5));
        assert_eq!(g.get(&Field::Credit), Some(&6));
        assert_eq!(g.get(&Field::Amount), None);
    }

    #[test]
    fn rows_become_transactions_with_review() {
        let b = starter::budget_with_separator('-');
        let t = table(&[
            &["Date", "Account", "Vendor", "Description", "Amount"],
            &["9/1/2026", "1001", "Writer", "Draft", "$2,500.00"],
            &["9/2/2026", "10-01", "Writer", "Polish", "(100)"],
            &["9/3/2026", "9999", "Mystery", "Unknown account", "50"],
            &["9/4/2026", "", "Bank", "Fee", "5"],
            &["", "", "", "Total", "2455"],
            &["9/5/2026", "10-01", "Writer", "Nothing", "0"],
            &["9/6/2026", "10-01", "Writer", "Bad", "TBD"],
            &["Sept 31", "10-01", "Writer", "Odd date", "7"],
        ]);
        use Field::*;
        let m = mapping(&[Date, Account, Vendor, Description, Amount]);
        let p = plan(&b, &t, &m, &Options::default());
        let got: Vec<(usize, &str, Cents, bool)> = p
            .transactions
            .iter()
            .map(|x| {
                (
                    x.row,
                    x.transaction.account.as_str(),
                    x.transaction.amount,
                    x.assigned,
                )
            })
            .collect();
        assert_eq!(
            got,
            [
                (2, "10-01", 250_000, true),
                (3, "10-01", -10_000, true),
                (4, "99-99", 5_000, false),
                (5, "", 500, false),
                (9, "10-01", 700, true),
            ]
        );
        assert_eq!(p.transactions[0].transaction.date, "2026-09-01");
        assert_eq!(
            p.transactions[4].transaction.date, "",
            "unreadable date left blank"
        );
        assert_eq!(p.warnings.len(), 1);
        let skipped: Vec<usize> = p.skipped.iter().map(|n| n.row).collect();
        assert_eq!(skipped, [6, 7, 8]);
        assert!(p.skipped[0].message.contains("total"));
        assert_eq!(p.unmatched.len(), 2);
        assert_eq!((p.unmatched[0].key.as_str(), p.unmatched[0].rows), ("", 1));
        assert_eq!(
            (p.unmatched[1].key.as_str(), p.unmatched[1].text.as_str()),
            ("99-99", "9999")
        );
        assert_eq!(p.total, 250_000 - 10_000 + 5_000 + 500 + 700);
        assert_eq!(p.accounts.len(), 1);

        // The review: map one number, skip the blank ones.
        let o = Options {
            codes: BTreeMap::from([
                ("99-99".to_string(), CodeChoice::Map("1101".into())),
                (String::new(), CodeChoice::Skip),
            ]),
            ..Options::default()
        };
        let p = plan(&b, &t, &m, &o);
        assert_eq!(p.transactions[2].transaction.account, "11-01");
        assert!(p.transactions[2].assigned);
        assert_eq!(p.transactions.len(), 4);
        assert!(p
            .skipped
            .iter()
            .any(|n| n.row == 5 && n.message.contains("without an account")));
        // Mapping to an account the budget lacks skips the rows, saying so.
        let o = Options {
            codes: BTreeMap::from([("99-99".to_string(), CodeChoice::Map("88-88".into()))]),
            ..Options::default()
        };
        let p = plan(&b, &t, &m, &o);
        assert!(p
            .skipped
            .iter()
            .any(|n| n.row == 4 && n.message.contains("88-88")));
    }

    #[test]
    fn debits_credits_signs_and_carried_accounts() {
        let b = starter::budget_with_separator('-');
        let t = table(&[
            &["Account", "Date", "Memo", "Debit", "Credit"],
            &["10-01", "", "", "", ""],
            &["", "2026-09-01", "Draft", "1,000.00", ""],
            &["", "2026-09-02", "Refund", "", "250"],
            &["", "", "Total 10-01", "1000", "250"],
            &["10-02", "2026-09-03", "Rights", "300", ""],
        ]);
        use Field::*;
        let m = mapping(&[Account, Date, Description, Debit, Credit]);
        let o = Options {
            carry_account: true,
            ..Options::default()
        };
        let p = plan(&b, &t, &m, &o);
        let got: Vec<(&str, Cents)> = p
            .transactions
            .iter()
            .map(|x| (x.transaction.account.as_str(), x.transaction.amount))
            .collect();
        assert_eq!(
            got,
            [("10-01", 100_000), ("10-01", -25_000), ("10-02", 30_000)]
        );
        assert_eq!(p.headings, 1);
        assert_eq!(p.skipped.len(), 1, "{:?}", p.skipped);
        let o = Options {
            carry_account: true,
            negate: true,
            ..Options::default()
        };
        let p = plan(&b, &t, &m, &o);
        assert_eq!(p.transactions[0].transaction.amount, -100_000);
    }

    #[test]
    fn duplicates_are_found_and_reviewed() {
        let mut b = starter::budget_with_separator('-');
        let t = table(&[
            &["Date", "Account", "Description", "Amount"],
            &["2026-09-01", "10-01", "Draft", "100"],
            &["2026-09-01", "10-01", "Draft", "100"],
            &["2026-09-02", "10-01", "Polish", "50"],
        ]);
        use Field::*;
        let m = mapping(&[Date, Account, Description, Amount]);
        let p = plan(&b, &t, &m, &Options::default());
        assert_eq!(p.transactions.len(), 3, "repeats in the file import");
        assert_eq!(p.duplicates.len(), 1);
        assert_eq!(
            (
                p.duplicates[0].row,
                p.duplicates[0].repeats,
                p.duplicates[0].skipped
            ),
            (3, Some(2), false)
        );
        let o = Options {
            skip_repeats: true,
            ..Options::default()
        };
        assert_eq!(plan(&b, &t, &m, &o).transactions.len(), 2);
        // Importing it, then again: the second time every row matches.
        let (p, batch) = import(
            &mut b,
            &t,
            &m,
            &Options::default(),
            "ledger.csv",
            "2026-10-09T12:00:00Z",
        )
        .unwrap();
        assert_eq!((p.transactions.len(), batch), (3, 1));
        let again = plan(&b, &t, &m, &Options::default());
        assert!(again.transactions.is_empty());
        assert_eq!(again.duplicates.len(), 3);
        assert!(again
            .duplicates
            .iter()
            .all(|d| d.skipped && d.repeats.is_none()));
        let e = import(&mut b, &t, &m, &Options::default(), "ledger.csv", "now").unwrap_err();
        assert!(e.contains("No transactions"), "{e}");
        // With duplicates of the budget's let through, they import.
        let o = Options {
            skip_existing: false,
            ..Options::default()
        };
        assert_eq!(plan(&b, &t, &m, &o).transactions.len(), 3);
    }

    #[test]
    fn long_text_is_cut_and_said() {
        let b = starter::budget_with_separator('-');
        let long = "x".repeat(MAX_TEXT_CHARS + 5);
        let t = table(&[
            &["Account", "Description", "Amount"],
            &["10-01", &long, "1"],
        ]);
        use Field::*;
        let p = plan(
            &b,
            &t,
            &mapping(&[Account, Description, Amount]),
            &Options::default(),
        );
        assert_eq!(
            p.transactions[0].transaction.description.chars().count(),
            MAX_TEXT_CHARS
        );
        assert_eq!(p.warnings.len(), 1);
    }

    #[test]
    fn what_a_plan_keeps_the_budget_takes() {
        // Whatever a plan lists as imported must pass the engine's checks,
        // or one odd row refuses the whole import.
        let mut b = starter::budget_with_separator('-');
        let long_account = "9".repeat(80);
        let t = table(&[
            &["Account", "Description", "Amount", "Tags"],
            &["10-01", "Fine", "1", "PO"],
            &[&long_account, "Odd account", "2", ""],
            &["10-01", "Odd tag", "3", "PO, a\tb"],
        ]);
        use Field::*;
        let m = mapping(&[Account, Description, Amount, Tags]);
        let (p, _) = import(
            &mut b,
            &t,
            &m,
            &Options::default(),
            "ledger.csv",
            "2026-10-09T12:00:00Z",
        )
        .unwrap();
        assert_eq!(p.transactions.len(), 2);
        assert_eq!(p.skipped.len(), 1);
        assert_eq!(p.skipped[0].row, 3);
        let tags = &p.transactions[1].transaction.tags;
        assert_eq!(tags.iter().collect::<Vec<_>>(), ["PO"]);
        assert_eq!(b.actuals.transactions.len(), 2);
    }
}
