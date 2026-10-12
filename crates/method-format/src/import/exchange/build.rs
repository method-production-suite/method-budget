//! An [`Export`] as a budget.
//!
//! What comes over as it is: categories, accounts and detail rows in order,
//! with descriptions, numbers (Amount, X, Rate, 4X as written), units,
//! currencies, fringes, groups, sets and locations by name, and the
//! budget's information fields.
//!
//! What is worked out from the export's figures, and said so in the notes:
//!
//! - **Account numbering**, from the numbers ([`infer_coding_lenient`]).
//! - **Fringe posting**: a Total Fringes row in an account's lines is by
//!   account; a fringe account in each category is by category, its suffix
//!   read from the account's number; Total Fringes rows between categories
//!   are by production level (a production total is added after each,
//!   since the export doesn't list them); one Total Fringes row after the
//!   categories is by budget.
//! - **Adjustment rows**: a row in the base (or totals) currency whose rate
//!   is the total of the rows above it since the previous adjustment row.
//! - **Fringes**: name, kind and rate from the fringe rows' figures; a
//!   percent fringe's cutoff from what it was worked out on (the capped
//!   total) against what its lines come to, and fringe ranges from lines
//!   that earned less than their rate.
//! - **Exchange rates**, from each foreign line's total against its own
//!   amount.
//! - **Units**: the usual starter hours (the export has none), custom
//!   units by their singular and plural.
//!
//! What can't come over: formulas and globals (lines keep the values;
//! each line that used globals gets a note naming them), production totals
//! (except under production-level posting), lines left out of the totals by
//! a group (the export omits them), and the rules of charges and credits
//! (they come in as fixed amounts that give the same totals).

use std::collections::{BTreeMap, BTreeSet};

use method_core::coa::infer_coding_lenient;
use method_core::coding::AccountCoding;
use method_core::{
    Account, Baseline, Budget, Category, Cents, Charge, ChargeKind, Credit, CreditCap, CreditRow,
    CreditSource, Currency, Dec, Detail, Formula, Fringe, FringeKind, FringePosting, Group,
    InfoValue, Note, ProductionTotal, RowId, RowKind, TopsheetRow, TrailingFringes, UnitDef,
};

use super::model::{Export, ExportFormat, Num, TopRow, XAccount, XCategory, XFringe};
use crate::setup::copy::money;

/// Choices for [`build`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BuildOptions {
    /// The title when the export names none (usually the file's name).
    pub title: String,
}

/// How sure a note is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NoteKind {
    /// What was done, for the record.
    Info,
    /// Worked out from the export's figures: check it.
    Check,
    /// In the original budget but not in the export, or not brought over.
    Lost,
}

impl NoteKind {
    pub fn key(self) -> &'static str {
        match self {
            NoteKind::Info => "info",
            NoteKind::Check => "check",
            NoteKind::Lost => "not_imported",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            NoteKind::Info => "Note",
            NoteKind::Check => "Check",
            NoteKind::Lost => "Not imported",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportNote {
    pub kind: NoteKind,
    pub text: String,
}

/// How much came over.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub categories: usize,
    pub accounts: usize,
    /// Lines and adjustment rows.
    pub lines: usize,
    pub adjustments: usize,
    /// Empty entry rows passed over.
    pub blank_rows: usize,
    pub fringes: usize,
    pub groups: usize,
    pub sets: usize,
    pub locations: usize,
    pub currencies: usize,
    pub charges: usize,
    pub credits: usize,
    /// Lines whose formulas used globals.
    pub lines_with_globals: usize,
}

/// A detail row and the export's total for it.
#[derive(Clone, Debug, PartialEq)]
pub struct LineLink {
    pub id: RowId,
    pub account: String,
    pub description: String,
    pub total: f64,
}

/// An account, category or topsheet row and the export's figures for it.
#[derive(Clone, Debug, PartialEq)]
pub struct TotalLink {
    pub code: String,
    pub name: String,
    pub total: f64,
    pub fringe: f64,
}

/// Where each of the export's figures is in the budget, for the
/// reconciliation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Links {
    pub lines: Vec<LineLink>,
    pub accounts: Vec<TotalLink>,
    pub categories: Vec<TotalLink>,
    /// Fringes posted by category: (category code, the export's fringe
    /// account number and total).
    pub fringe_accounts: Vec<(String, String, f64)>,
    /// Fringes posted by account: account code, its Total Fringes row.
    pub account_fringe_rows: Vec<(String, f64)>,
    /// Fringes posted by production level, one per level, in order.
    pub level_fringes: Vec<f64>,
    /// Fringes posted by budget.
    pub budget_fringes: Option<f64>,
    pub charges: Vec<(String, f64)>,
    /// The export's credit rows (negative).
    pub credits: Vec<(String, f64)>,
}

/// An export made into a budget.
#[derive(Clone, Debug, PartialEq)]
pub struct Imported {
    pub budget: Budget,
    pub format: ExportFormat,
    /// The currency the export's figures are in.
    pub currency: String,
    /// Decimal places the export's figures are given to (2, or 0 for whole
    /// units): the reconciliation compares at that precision.
    pub places: u8,
    pub notes: Vec<ImportNote>,
    pub counts: Counts,
    pub links: Links,
}

impl Imported {
    fn note(&mut self, kind: NoteKind, text: impl Into<String>) {
        self.notes.push(ImportNote {
            kind,
            text: text.into(),
        });
    }
}

/// Figures read from the export are doubles; cents, rounded half away from
/// zero (they are already whole cents, give or take the binary).
pub fn cents(v: f64) -> Cents {
    if !v.is_finite() || v.abs() >= 9.0e15 {
        return 0;
    }
    // Rounded as the digits read (`100.005` is 100.01, though the double
    // is a hair below it), when they are short enough to be exact.
    let text = format!("{v}");
    if let Ok(d) = text.parse::<Dec>() {
        if let Some(c) = d.to_cents() {
            return c;
        }
    }
    (v * 100.0).round() as Cents
}

/// `c` at the export's precision: whole units for `places` 0.
pub fn at_places(c: Cents, places: u8) -> Cents {
    if places >= 2 {
        return c;
    }
    let unit: Cents = if places == 1 { 10 } else { 100 };
    let q = c.abs() / unit + Cents::from(c.abs() % unit * 2 >= unit);
    q * unit * c.signum()
}

/// The hours the usual starter units have, the export carrying none.
pub(super) const STARTER_HOURS: [(&str, &str, i64); 4] = [
    ("Day", "Days", 12),
    ("Week", "Weeks", 60),
    ("Hour", "Hours", 1),
    ("Month", "Months", 240),
];

// ---------------------------------------------------------------------------
// Units

/// Maps the unit names an export writes (singular after an Amount of 1,
/// plural otherwise, plural in fringe figures) to the budget's units,
/// adding the ones it lacks. Returns the names added.
pub(super) fn resolve_units(
    seen: &BTreeMap<String, bool>,
    units: &mut BTreeMap<String, UnitDef>,
) -> (BTreeMap<String, String>, Vec<String>) {
    let mut map = BTreeMap::new();
    let mut custom: Vec<(&String, bool)> = Vec::new();
    for (name, &plural_seen) in seen {
        let found = units
            .iter()
            .find(|(k, u)| *k == name || u.plural == *name)
            .or_else(|| {
                units.iter().find(|(k, u)| {
                    k.eq_ignore_ascii_case(name) || u.plural.eq_ignore_ascii_case(name)
                })
            })
            .map(|(k, _)| k.clone());
        match found {
            Some(k) => {
                map.insert(name.clone(), k);
            }
            None => custom.push((name, plural_seen)),
        }
    }
    let names: BTreeSet<&str> = custom.iter().map(|(n, _)| n.as_str()).collect();
    let stem = |n: &str| -> Option<String> {
        let last = n.chars().last()?;
        (n.chars().count() > 1 && (last == 's' || last == 'S'))
            .then(|| n[..n.len() - last.len_utf8()].to_string())
    };
    let mut added = Vec::new();
    for (name, plural_seen) in &custom {
        let (singular, plural) = match stem(name) {
            Some(s) if names.contains(s.as_str()) || *plural_seen => (s, (*name).clone()),
            _ => {
                let up = name.chars().last().is_some_and(char::is_uppercase);
                (
                    (*name).clone(),
                    format!("{name}{}", if up { "S" } else { "s" }),
                )
            }
        };
        if !units.contains_key(&singular) {
            units.insert(
                singular.clone(),
                UnitDef {
                    plural,
                    description: String::new(),
                    hours: None,
                },
            );
            added.push(singular.clone());
        }
        map.insert((*name).clone(), singular);
    }
    (map, added)
}

// ---------------------------------------------------------------------------
// Exchange rates

/// What `ratio` (a total over an amount) can be, given the total is
/// rounded to cents: `(lo, hi)`. `None` when the amount is 0.
fn ratio_bounds(total: f64, amount: f64, half: f64) -> Option<(f64, f64)> {
    if amount == 0.0 || !amount.is_finite() {
        return None;
    }
    let a = (total - half) / amount;
    let b = (total + half) / amount;
    Some((a.min(b), a.max(b)))
}

/// The number with the fewest decimal places (up to 6) between `lo` and
/// `hi`, as text.
pub(super) fn simplest(lo: f64, hi: f64) -> String {
    let mid = (lo + hi) / 2.0;
    for p in 0..=6 {
        let s = format!("{mid:.p$}");
        if let Ok(v) = s.parse::<f64>() {
            if v >= lo - 1e-12 && v <= hi + 1e-12 {
                return trim_zeros(&s);
            }
        }
    }
    trim_zeros(&format!("{mid:.6}"))
}

/// A double as plain decimal text, at most 6 places.
pub(super) fn trim_number(v: f64) -> String {
    trim_zeros(&format!("{v:.6}"))
}

fn trim_zeros(s: &str) -> String {
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s.to_string()
    }
}

/// Intersects bounds, keeping the first when they don't overlap.
fn narrow(acc: Option<(f64, f64)>, b: (f64, f64)) -> (Option<(f64, f64)>, bool) {
    match acc {
        None => (Some(b), true),
        Some((lo, hi)) => {
            let (l, h) = (lo.max(b.0), hi.min(b.1));
            if l <= h + 1e-12 {
                (Some((l, h.max(l))), true)
            } else {
                (Some((lo, hi)), false)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Cutoffs

/// What a percent fringe is worked out on over one earner (a row, or the
/// rows of a range carrying it, in order) with a cutoff of `c`, as the
/// export's fringe figures count it: each row takes its total, or what is
/// left of the cutoff when that is less (nothing once it is used up), and
/// a negative row its whole total.
fn capped(rows: &[f64], c: f64) -> f64 {
    let mut left = c;
    let mut sum = 0.0;
    for &e in rows {
        if e < 0.0 {
            sum += e;
        } else {
            sum += e.min(left.max(0.0));
        }
        left -= e;
    }
    sum
}

/// The cutoff, in cents, under which `earners` come to `target`; `None`
/// when no cutoff in cents gives it (within a cent).
fn solve_cutoff(earners: &[Vec<f64>], target: f64, slack: f64) -> Option<Cents> {
    let f = |c: f64| earners.iter().map(|e| capped(e, c)).sum::<f64>();
    let hi0: f64 = earners
        .iter()
        .map(|e| e.iter().filter(|v| **v > 0.0).sum::<f64>())
        .fold(0.0, f64::max);
    if hi0 <= 0.0 || f(0.0) > target + 0.005 + slack {
        return None;
    }
    let (mut lo, mut hi) = (0.0, hi0);
    for _ in 0..200 {
        let mid = (lo + hi) / 2.0;
        if f(mid) < target {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let base = (hi * 100.0).round() as Cents;
    // A cent more cutoff moves the figure by a cent for every earner it
    // caps.
    let capping = earners
        .iter()
        .filter(|e| capped(e, hi + 0.01) - capped(e, hi) > 0.001)
        .count()
        .max(1);
    [base, base - 1, base + 1]
        .into_iter()
        .filter(|c| *c > 0)
        .map(|c| (c, (f(c as f64 / 100.0) - target).abs()))
        .filter(|(_, d)| *d <= 0.0051 * capping as f64 + 0.0001 + slack)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(c, _)| c)
}

/// One imported row, for the inference passes.
struct LineInfo {
    /// Which account it is in (an index over every account built).
    account: usize,
    /// Where it is: topsheet row, account, detail.
    at: (usize, usize, usize),
    /// The export's total for it.
    total: f64,
    percent: BTreeSet<String>,
    flat: bool,
    any_fringe: bool,
    earned: Option<f64>,
    nominal: Option<f64>,
    /// Its fringes post nowhere in the export (after the last production
    /// level's fringe row), so the fringe figures leave it out.
    unposted: bool,
    /// Its Amount and unit (the budget's unit name), for flat fringes.
    amount: f64,
    unit: String,
    /// Every fringe on it, by name.
    names: BTreeSet<String>,
}

/// Puts line `t` in a fringe range with the lines above it that carry one
/// of its percent fringes (passing over lines with no fringes): joins the
/// range of the line just above when it is in one, else starts a range
/// back to the first of the run. Returns whether it did.
fn join_back(lines: &[LineInfo], ranges: &mut [Option<u32>], t: usize, next: &mut u32) -> bool {
    if ranges[t].is_some() {
        return false;
    }
    let mut first = None;
    let mut j = t;
    while j > 0 {
        j -= 1;
        let l = &lines[j];
        if l.account != lines[t].account {
            break;
        }
        if !l.any_fringe {
            continue;
        }
        if l.percent.is_disjoint(&lines[t].percent) {
            break;
        }
        if let Some(r) = ranges[j] {
            if first.is_none() {
                for slot in &mut ranges[j + 1..=t] {
                    *slot = Some(r);
                }
                return true;
            }
            break;
        }
        first = Some(j);
    }
    match first {
        Some(f) => {
            *next += 1;
            for slot in &mut ranges[f..=t] {
                *slot = Some(*next);
            }
            true
        }
        None => false,
    }
}

/// The earners of fringe `name`: each line carrying it on its own, the
/// lines of a range together, in order.
fn earners(lines: &[LineInfo], ranges: &[Option<u32>], name: &str) -> Vec<Vec<f64>> {
    let mut out: Vec<Vec<f64>> = Vec::new();
    let mut current: Option<(usize, u32)> = None;
    for (i, l) in lines.iter().enumerate() {
        if l.unposted || !l.percent.contains(name) {
            continue;
        }
        match (ranges[i], current) {
            (Some(r), Some((a, cr))) if cr == r && a == l.account => {
                out.last_mut().expect("a range has started").push(l.total);
            }
            (Some(r), _) => {
                current = Some((l.account, r));
                out.push(vec![l.total]);
            }
            (None, _) => {
                current = None;
                out.push(vec![l.total]);
            }
        }
    }
    out
}

/// What a flat fringe counted: its unit, the units it counted in all
/// (from the export's figures), and each line's Amount and unit.
type FlatEquation = (Option<String>, f64, Vec<(f64, String)>);

// ---------------------------------------------------------------------------
// Posting

enum Posting {
    Account,
    Category(String),
    Level,
    Budget,
}

fn infer_posting(x: &Export, coding: &AccountCoding) -> (Posting, Vec<ImportNote>) {
    let mut notes = Vec::new();
    let cats = || {
        x.rows.iter().filter_map(|r| match r {
            TopRow::Category(c) => Some(c),
            _ => None,
        })
    };
    if cats()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.details)
        .any(|d| d.fringe_row.is_some())
    {
        return (Posting::Account, notes);
    }
    let mut suffixes: BTreeMap<String, usize> = BTreeMap::new();
    let mut any_fringe_account = false;
    for c in cats() {
        for a in c.accounts.iter().filter(|a| a.fringe_account.is_some()) {
            any_fringe_account = true;
            let n = a.number.trim();
            if n.is_empty() || c.number.trim().is_empty() {
                continue;
            }
            let found = (1..n.len())
                .filter(|k| n.is_char_boundary(*k))
                .map(|k| &n[k..])
                .find(|s| coding.category_fringe_account(&c.number, s) == n);
            if let Some(s) = found {
                *suffixes.entry(s.to_string()).or_default() += 1;
            }
        }
    }
    if any_fringe_account {
        let suffix = suffixes
            .iter()
            .max_by_key(|(_, n)| **n)
            .map(|(s, _)| s.clone())
            .unwrap_or_else(|| {
                notes.push(ImportNote {
                    kind: NoteKind::Check,
                    text: "the fringe accounts' numbers don't show their suffix: 99 was used"
                        .into(),
                });
                "99".into()
            });
        if suffixes.len() > 1 {
            notes.push(ImportNote {
                kind: NoteKind::Check,
                text: format!(
                    "the fringe accounts are numbered with different suffixes; {suffix} was used"
                ),
            });
        }
        return (Posting::Category(suffix), notes);
    }
    // Total Fringes rows on the topsheet.
    let fringe_rows: Vec<usize> = x
        .rows
        .iter()
        .enumerate()
        .filter(|(_, r)| matches!(r, TopRow::Fringes(_)))
        .map(|(i, _)| i)
        .collect();
    let last_cat = x
        .rows
        .iter()
        .rposition(|r| matches!(r, TopRow::Category(_)));
    let first_blank = x.rows.iter().position(|r| matches!(r, TopRow::Blank));
    match fringe_rows.as_slice() {
        [] => (Posting::Budget, notes),
        [one] => {
            let among = last_cat.is_some_and(|l| *one < l) || first_blank.is_some_and(|b| *one < b);
            if !among {
                return (Posting::Budget, notes);
            }
            // One row among the categories: a level's fringes, unless it
            // holds every category's (a budget row that was moved).
            let TopRow::Fringes(f) = &x.rows[*one] else {
                unreachable!()
            };
            let all: f64 = cats().map(|c| c.fringe).sum();
            let before: f64 = x.rows[..*one]
                .iter()
                .filter_map(|r| match r {
                    TopRow::Category(c) => Some(c.fringe),
                    _ => None,
                })
                .sum();
            if cents(f.total) == cents(all) && cents(all) != cents(before) {
                (Posting::Budget, notes)
            } else {
                (Posting::Level, notes)
            }
        }
        _ => (Posting::Level, notes),
    }
}

// ---------------------------------------------------------------------------
// Information fields

fn info_value(label: &str, text: &str) -> InfoValue {
    let t = text.trim();
    let numeric = ["shoot days", "year", "episodes"]
        .iter()
        .any(|k| label.to_ascii_lowercase().contains(k));
    if numeric {
        if let Ok(d) = t.replace(',', "").parse::<Dec>() {
            return InfoValue::Number(d);
        }
    }
    if let Some(d) = iso_date(t) {
        return InfoValue::Date(d);
    }
    InfoValue::Text(t.to_string())
}

/// `YYYY-MM-DD`, or a month/day/year date, as ISO 8601.
fn iso_date(t: &str) -> Option<String> {
    let valid =
        |y: u32, m: u32, d: u32| (1..=12).contains(&m) && (1..=31).contains(&d) && y >= 1000;
    let b = t.as_bytes();
    if b.len() == 10 && b[4] == b'-' && b[7] == b'-' {
        let y: u32 = t[..4].parse().ok()?;
        let m: u32 = t[5..7].parse().ok()?;
        let d: u32 = t[8..].parse().ok()?;
        return valid(y, m, d).then(|| format!("{y:04}-{m:02}-{d:02}"));
    }
    let parts: Vec<&str> = t.split('/').collect();
    if let [m, d, y] = parts.as_slice() {
        let (m, d, y): (u32, u32, u32) = (m.parse().ok()?, d.parse().ok()?, y.parse().ok()?);
        return valid(y, m, d).then(|| format!("{y:04}-{m:02}-{d:02}"));
    }
    None
}

// ---------------------------------------------------------------------------
// The budget

fn formula(n: &Option<Num>, blank_if_one: bool) -> Formula {
    match n {
        None => Formula::default(),
        Some(n) if blank_if_one && n.value == 1.0 => Formula::default(),
        Some(n) => Formula(n.text.clone()),
    }
}

fn is_real_account(a: &XAccount) -> bool {
    a.fringe_account.is_none()
        && (!a.number.trim().is_empty()
            || !a.name.trim().is_empty()
            || a.details
                .iter()
                .any(|d| !d.is_blank() && d.fringe_row.is_none()))
}

fn is_real_category(c: &XCategory) -> bool {
    !c.number.trim().is_empty()
        || !c.name.trim().is_empty()
        || c.accounts.iter().any(is_real_account)
}

/// Makes the budget an export describes. Never fails: what can't be read
/// is left out and said so in the notes.
pub fn build(given: &Export, o: &BuildOptions) -> Imported {
    // Layouts that leave out part of the fringes: work it out first.
    let mut prepared = given.clone();
    let mut pre_notes: Vec<ImportNote> = Vec::new();
    let mut synthetic: Vec<XFringe> = Vec::new();
    let mut pre_note = |kind, text: String| pre_notes.push(ImportNote { kind, text });
    if !given.fringe_figures && !given.totals_only {
        let (figs, unknown, single) = super::infer::fringe_figures(given);
        if !figs.is_empty() {
            pre_note(
                NoteKind::Check,
                format!(
                    "This layout has no fringe rates: worked out from the lines' fringe percentages: {}.",
                    figs.iter()
                        .map(|f| if f.percent {
                            format!("{} {}%", f.name, f.rate)
                        } else if f.unit.is_empty() {
                            format!("{} {} per line", f.name, f.rate)
                        } else {
                            format!("{} {} per {}", f.name, f.rate, f.unit)
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            );
        }
        if !single.is_empty() {
            pre_note(
                NoteKind::Check,
                format!(
                    "Worked out from one line each, so a flat amount may have been read as a percentage: {}.",
                    single.join(", ")
                ),
            );
        }
        if !unknown.is_empty() {
            pre_note(
                NoteKind::Lost,
                format!(
                    "No line shows the rate of {}; added at 0%.",
                    unknown.join(", ")
                ),
            );
        }
        synthetic = figs;
    }
    if !given.line_fringes && !given.totals_only {
        let mut figs: Vec<XFringe> = synthetic.clone();
        for r in &given.rows {
            match r {
                TopRow::Fringes(t) => figs.extend(t.fringes.iter().cloned()),
                TopRow::Category(c) => {
                    for a in &c.accounts {
                        figs.extend(a.fringe_account.iter().flatten().cloned());
                    }
                }
                _ => {}
            }
        }
        let (done, undecided) = super::infer::line_fringes(&mut prepared, &figs);
        pre_note(
            NoteKind::Check,
            format!(
                "This layout doesn't name the lines' fringes: {done} line{} got the fringes whose percentages add up to the line's{}.",
                if done == 1 { "" } else { "s" },
                if undecided > 0 {
                    format!("; {undecided} couldn't be decided and have none")
                } else {
                    String::new()
                }
            ),
        );
    }
    let x = &prepared;
    let half = 0.5 * 10f64.powi(-i32::from(x.places));
    let title = [x.name.trim(), o.title.trim()]
        .into_iter()
        .find(|s| !s.is_empty())
        .unwrap_or("Imported budget")
        .to_string();
    let base = x.base_currency.trim().to_string();
    let conv = x.totals_currency.trim().to_string();
    let converted = !conv.is_empty() && conv != base;
    let mut b = Budget::new(title, base.clone());
    for (k, plural, h) in STARTER_HOURS {
        b.units.insert(
            k.to_string(),
            UnitDef {
                plural: plural.to_string(),
                description: String::new(),
                hours: Some(Dec::from_int(h)),
            },
        );
    }
    let mut im = Imported {
        budget: Budget::new("", ""),
        format: x.format,
        places: x.places,
        currency: if conv.is_empty() {
            base.clone()
        } else {
            conv.clone()
        },
        notes: Vec::new(),
        counts: Counts::default(),
        links: Links::default(),
    };
    for n in &x.notes {
        // A default the reader chose is to check; the rest was left out.
        let kind = if n.contains(" was used") {
            NoteKind::Check
        } else {
            NoteKind::Lost
        };
        im.note(kind, n.clone());
    }
    im.notes.extend(pre_notes);

    let cats: Vec<&XCategory> = x
        .rows
        .iter()
        .filter_map(|r| match r {
            TopRow::Category(c) if is_real_category(c) => Some(c),
            _ => None,
        })
        .collect();

    // Numbering, from the numbers.
    let cat_numbers: Vec<&str> = cats
        .iter()
        .map(|c| c.number.trim())
        .filter(|n| !n.is_empty())
        .collect();
    let acct_numbers: Vec<(&str, &str)> = cats
        .iter()
        .flat_map(|c| {
            c.accounts
                .iter()
                .filter(|a| is_real_account(a) && !a.number.trim().is_empty())
                .map(move |a| (a.number.trim(), c.number.trim()))
        })
        .collect();
    b.account_coding = infer_coding_lenient(&cat_numbers, &acct_numbers, &[]);
    let coding = b.account_coding;

    // Fringe posting.
    let (posting, posting_notes) = infer_posting(x, &coding);
    im.notes.extend(posting_notes);
    b.fringe_posting = match &posting {
        Posting::Account => FringePosting::Account,
        Posting::Category(s) => FringePosting::Category { suffix: s.clone() },
        Posting::Level => FringePosting::ProductionLevel,
        Posting::Budget => FringePosting::Budget,
    };

    // Every fringe's figures, wherever they are.
    let mut figures: Vec<&XFringe> = Vec::new();
    for r in &x.rows {
        match r {
            TopRow::Fringes(t) => figures.extend(&t.fringes),
            TopRow::Category(c) => {
                for a in &c.accounts {
                    figures.extend(a.fringe_account.iter().flatten());
                    for d in &a.details {
                        figures.extend(d.fringe_row.iter().flatten());
                    }
                }
            }
            _ => {}
        }
    }
    figures.extend(&synthetic);

    // Units, by every name the export writes.
    let mut unit_names: BTreeMap<String, bool> = BTreeMap::new();
    let mut see_unit = |name: &str, plural: bool| {
        let n = name.trim();
        if !n.is_empty() {
            *unit_names.entry(n.to_string()).or_default() |= plural;
        }
    };
    for c in &cats {
        for a in &c.accounts {
            for d in a.details.iter().filter(|d| d.fringe_row.is_none()) {
                let plural = |n: &Option<Num>| n.as_ref().is_some_and(|n| n.value != 1.0);
                see_unit(&d.unit, plural(&d.amount));
                see_unit(&d.unit2, plural(&d.x));
                see_unit(&d.unit3, false);
                see_unit(&d.unit4, plural(&d.x4));
            }
        }
    }
    for f in figures.iter().filter(|f| !f.percent) {
        see_unit(&f.unit, true);
    }
    let (unit_map, units_added) = resolve_units(&unit_names, &mut b.units);
    let unit = |n: &str| -> String {
        let t = n.trim();
        unit_map.get(t).cloned().unwrap_or_else(|| t.to_string())
    };

    // The topsheet.
    let fringe_row_count = x
        .rows
        .iter()
        .filter(|r| matches!(r, TopRow::Fringes(_)))
        .count();
    let mut fringe_rows_seen = 0usize;
    let mut lines: Vec<LineInfo> = Vec::new();
    let mut next_id: RowId = 0;
    let mut account_index = 0usize;
    let mut seen_codes: BTreeSet<String> = BTreeSet::new();
    let mut renamed_codes: Vec<(String, String)> = Vec::new();
    let mut fringe_names: BTreeSet<String> = BTreeSet::new();
    let mut globals_used: BTreeMap<String, usize> = BTreeMap::new();
    for r in &x.rows {
        match r {
            TopRow::Category(c) if is_real_category(c) => {
                let unposted =
                    matches!(posting, Posting::Level) && fringe_rows_seen == fringe_row_count;
                let mut cat = Category {
                    code: c.number.trim().to_string(),
                    name: c.name.trim().to_string(),
                    accounts: Vec::new(),
                    notes: Vec::new(),
                };
                let ti = b.topsheet.len();
                for a in &c.accounts {
                    if a.fringe_account.is_some() {
                        im.links.fringe_accounts.push((
                            cat.code.clone(),
                            a.number.trim().to_string(),
                            a.total,
                        ));
                        continue;
                    }
                    if !is_real_account(a) {
                        im.counts.blank_rows += a.details.len();
                        continue;
                    }
                    let mut code = a.number.trim().to_string();
                    if !seen_codes.insert(code.clone()) {
                        let mut k = 2;
                        while seen_codes.contains(&format!("{code}-{k}")) {
                            k += 1;
                        }
                        let new = format!("{code}-{k}");
                        renamed_codes.push((code.clone(), new.clone()));
                        seen_codes.insert(new.clone());
                        code = new;
                    }
                    let mut acct = Account {
                        code: code.clone(),
                        name: a.name.trim().to_string(),
                        details: Vec::new(),
                        notes: Vec::new(),
                    };
                    let ai = cat.accounts.len();
                    // The last row that isn't an empty entry row.
                    let last_real = a
                        .details
                        .iter()
                        .rposition(|d| !d.is_blank() && d.fringe_row.is_none());
                    // Totals since the last adjustment row, for spotting the next.
                    let mut window: Vec<f64> = Vec::new();
                    for (di, d) in a.details.iter().enumerate() {
                        if d.fringe_row.is_some() {
                            im.links
                                .account_fringe_rows
                                .push((code.clone(), d.subtotal));
                            continue;
                        }
                        if d.is_blank() && last_real.is_none_or(|l| di > l) {
                            im.counts.blank_rows += 1;
                            continue;
                        }
                        next_id += 1;
                        let cur = d.currency.trim();
                        let in_totals_currency = !cur.is_empty() && (cur == base || cur == conv);
                        let rate = d.rate.as_ref().map_or(0.0, |n| n.value);
                        let window_sum: f64 = window.iter().sum();
                        let adjustment = in_totals_currency
                            && !window.is_empty()
                            && (rate - window_sum).abs()
                                <= half * (window.len() as f64 + 1.0) + window_sum.abs() * 1e-12;
                        let mut det = if d.is_blank() {
                            Detail::line(next_id, "", Formula::default(), "", Formula::default())
                        } else if adjustment {
                            let mut det = Detail::adjustment(
                                next_id,
                                d.description.clone(),
                                formula(&d.amount, false),
                            );
                            det.x = formula(&d.x, true);
                            det.x4 = formula(&d.x4, true);
                            det.unit = unit(&d.unit);
                            det.unit2 = unit(&d.unit2);
                            det.unit4 = unit(&d.unit4);
                            im.counts.adjustments += 1;
                            det
                        } else {
                            let mut det = Detail::line(
                                next_id,
                                d.description.clone(),
                                formula(&d.amount, false),
                                unit(&d.unit),
                                formula(&d.rate, false),
                            );
                            det.x = formula(&d.x, true);
                            det.unit2 = unit(&d.unit2);
                            det.unit3 = unit(&d.unit3);
                            det.x4 = formula(&d.x4, true);
                            det.unit4 = unit(&d.unit4);
                            if !cur.is_empty() && cur != base {
                                det.currency = Some(cur.to_string());
                            }
                            det
                        };
                        if adjustment {
                            window.clear();
                        } else if !d.is_blank() {
                            window.push(d.subtotal);
                        }
                        det.fringes = d.fringes.iter().cloned().collect();
                        det.groups = d.groups.iter().cloned().collect();
                        if !d.location.trim().is_empty() {
                            det.location = Some(d.location.trim().to_string());
                        }
                        if !d.set.trim().is_empty() {
                            det.set = Some(d.set.trim().to_string());
                        }
                        if !d.globals.is_empty() {
                            im.counts.lines_with_globals += 1;
                            for g in &d.globals {
                                *globals_used.entry(g.clone()).or_default() += 1;
                            }
                            det.notes.push(Note::plain(
                                1,
                                format!(
                                    "Imported: this line used {} {}; the export keeps only the values.",
                                    if d.globals.len() == 1 { "the global" } else { "the globals" },
                                    d.globals.join(", ")
                                ),
                            ));
                        }
                        fringe_names.extend(d.fringes.iter().cloned());
                        lines.push(LineInfo {
                            account: account_index,
                            at: (ti, ai, acct.details.len()),
                            total: if x.places >= 2 || adjustment {
                                d.subtotal
                            } else {
                                super::infer::line_total(d)
                            },
                            percent: BTreeSet::new(),
                            flat: false,
                            any_fringe: !d.fringes.is_empty(),
                            earned: d.fringe_percent,
                            nominal: d.fringe_percent_sum,
                            unposted,
                            amount: d.amount.as_ref().map_or(0.0, |n| n.value),
                            unit: unit(&d.unit),
                            names: d.fringes.iter().cloned().collect(),
                        });
                        // Fringe kinds are filled in once known.
                        lines.last_mut().expect("just pushed").percent =
                            d.fringes.iter().cloned().collect();
                        im.links.lines.push(LineLink {
                            id: next_id,
                            account: code.clone(),
                            description: d.description.clone(),
                            total: d.subtotal,
                        });
                        acct.details.push(det);
                    }
                    im.links.accounts.push(TotalLink {
                        code: code.clone(),
                        name: acct.name.clone(),
                        total: a.total,
                        fringe: a.fringe,
                    });
                    cat.accounts.push(acct);
                    account_index += 1;
                }
                im.links.categories.push(TotalLink {
                    code: cat.code.clone(),
                    name: cat.name.clone(),
                    total: c.total,
                    fringe: c.fringe,
                });
                b.topsheet.push(TopsheetRow::Category(cat));
            }
            TopRow::Category(_) | TopRow::Blank => {}
            TopRow::Fringes(f) => match posting {
                Posting::Level => {
                    fringe_rows_seen += 1;
                    im.links.level_fringes.push(f.total);
                    b.topsheet
                        .push(TopsheetRow::ProductionTotal(ProductionTotal {
                            name: format!("Production total {fringe_rows_seen}"),
                        }));
                }
                _ => {
                    im.links.budget_fringes =
                        Some(im.links.budget_fringes.unwrap_or(0.0) + f.total);
                }
            },
            TopRow::ProductionTotal(name) => {
                b.topsheet
                    .push(TopsheetRow::ProductionTotal(ProductionTotal {
                        name: name.trim().to_string(),
                    }));
            }
            TopRow::Charge(t) => {
                let amount = cents(t.total);
                b.charges.push(Charge {
                    name: t.name.clone(),
                    account: None,
                    applied: true,
                    kind: ChargeKind::Flat {
                        amount,
                        currency: converted.then(|| conv.clone()),
                    },
                    excluded_amounts: Vec::new(),
                    excluded_groups: BTreeSet::new(),
                    excluded_categories: BTreeSet::new(),
                    excluded_accounts: BTreeSet::new(),
                    label_percent: false,
                    label_excluded: false,
                });
                im.links.charges.push((t.name.clone(), t.total));
            }
            TopRow::Credit(t) => {
                b.credits.push(Credit {
                    name: t.name.clone(),
                    account: None,
                    applied: true,
                    cap: None,
                    discount_percent: Dec::ZERO,
                    excluded_fringes: BTreeSet::new(),
                    rows: vec![CreditRow {
                        description: t.name.clone(),
                        // Filled in base currency below, once rates are known.
                        source: CreditSource::Flat(-cents(t.total)),
                        percent: Dec::from_int(100),
                        cap: CreditCap::None,
                    }],
                });
                im.links.credits.push((t.name.clone(), t.total));
            }
        }
    }
    im.counts.categories = b.categories().count();
    im.counts.accounts = account_index;
    im.counts.lines = im.links.lines.len();
    im.counts.charges = b.charges.len();
    im.counts.credits = b.credits.len();

    // Exchange rates, from the lines.
    let local = |d: &Detail| -> f64 {
        let v = |f: &Formula, blank: f64| f.0.trim().parse::<f64>().unwrap_or(blank);
        v(&d.amount, 0.0) * v(&d.x, 1.0) * v(&d.rate, 0.0) * v(&d.x4, 1.0)
    };
    let mut rows_by_id: BTreeMap<RowId, (&Detail, f64)> = BTreeMap::new();
    for c in b.topsheet.iter().filter_map(|r| match r {
        TopsheetRow::Category(c) => Some(c),
        _ => None,
    }) {
        for d in c.accounts.iter().flat_map(|a| &a.details) {
            if d.kind == RowKind::Line {
                rows_by_id.insert(d.id, (d, 0.0));
            }
        }
    }
    for l in &im.links.lines {
        if let Some(e) = rows_by_id.get_mut(&l.id) {
            e.1 = l.total;
        }
    }
    // The totals currency's rate, from base-currency lines.
    let mut conv_rate = 1.0f64;
    let mut conv_rate_text = "1".to_string();
    let mut conv_known = !converted;
    if converted {
        let mut acc = None;
        for (d, total) in rows_by_id.values() {
            if d.currency.is_some() {
                continue;
            }
            // total = local / R, so total / local bounds 1 / R.
            if let Some(bd) = ratio_bounds(*total, local(d), half) {
                if bd.0 > 0.0 {
                    acc = narrow(acc, bd).0;
                }
            }
        }
        if let Some((lo, hi)) = acc {
            conv_rate_text = simplest(1.0 / hi, 1.0 / lo);
            conv_rate = conv_rate_text.parse().unwrap_or(1.0);
            conv_known = true;
        }
    }
    let mut foreign: BTreeMap<String, Option<(f64, f64)>> = BTreeMap::new();
    let mut inconsistent: BTreeSet<String> = BTreeSet::new();
    for (d, total) in rows_by_id.values() {
        let Some(code) = &d.currency else { continue };
        let entry = foreign.entry(code.clone()).or_insert(None);
        let bounds = if converted {
            // The export converts the line's rounded base total.
            let l = local(d);
            let a = ((*total - half) * conv_rate - 0.005) / l;
            let b = ((*total + half) * conv_rate + 0.005) / l;
            (l != 0.0 && l.is_finite()).then(|| (a.min(b), a.max(b)))
        } else {
            ratio_bounds(*total, local(d), half)
        };
        if let Some(bd) = bounds {
            let (n, ok) = narrow(*entry, bd);
            *entry = n;
            if !ok {
                inconsistent.insert(code.clone());
            }
        }
    }
    let mut rate_words = Vec::new();
    let mut rateless_currencies = Vec::new();
    for (code, bounds) in &foreign {
        let text = if *code == conv && converted {
            conv_rate_text.clone()
        } else {
            match bounds {
                Some((lo, hi)) if *lo > 0.0 => simplest(*lo, *hi),
                _ => {
                    rateless_currencies.push(code.clone());
                    "1".into()
                }
            }
        };
        let rate: Dec = text.parse().unwrap_or(Dec::ONE);
        rate_words.push(format!("{code} {text}"));
        b.currencies.insert(
            code.clone(),
            Currency {
                description: String::new(),
                symbol: String::new(),
                rate,
            },
        );
    }
    if converted && !b.currencies.contains_key(&conv) {
        b.currencies.insert(
            conv.clone(),
            Currency {
                description: String::new(),
                symbol: String::new(),
                rate: conv_rate_text.parse().unwrap_or(Dec::ONE),
            },
        );
        rate_words.push(format!("{conv} {conv_rate_text}"));
    }
    if converted {
        b.totals_currency = Some(conv.clone());
    }
    im.counts.currencies = b.currencies.len();
    // Credits are typed in the base currency.
    if converted {
        for cr in &mut b.credits {
            for row in &mut cr.rows {
                if let CreditSource::Flat(c) = &mut row.source {
                    *c = ((*c as f64) * conv_rate).round() as Cents;
                }
            }
        }
    }

    // Fringes: definitions from the figures, by name.
    let mut defs: BTreeMap<String, &XFringe> = BTreeMap::new();
    let mut conflicting: BTreeSet<String> = BTreeSet::new();
    for f in &figures {
        match defs.get(&f.name) {
            Some(d) if d.percent != f.percent || d.rate != f.rate || d.unit != f.unit => {
                conflicting.insert(f.name.clone());
            }
            Some(_) => {}
            None => {
                defs.insert(f.name.clone(), f);
            }
        }
    }
    for l in &mut lines {
        let names = std::mem::take(&mut l.percent);
        for n in names {
            match defs.get(&n) {
                Some(d) if !d.percent => l.flat = true,
                _ => {
                    l.percent.insert(n);
                }
            }
        }
    }

    // Ranges and cutoffs, in the export's currency.
    let mut ranges: Vec<Option<u32>> = vec![None; lines.len()];
    let mut next_range = 0u32;
    for t in 0..lines.len() {
        let l = &lines[t];
        if !l.percent.is_empty() && l.earned.is_none() && l.nominal.is_some() && l.total != 0.0 {
            join_back(&lines, &mut ranges, t, &mut next_range);
        }
    }
    let mut cutoffs: BTreeMap<String, Option<Cents>> = BTreeMap::new();
    let mut unsolved: Vec<String> = Vec::new();
    let mut no_target: BTreeSet<String> = BTreeSet::new();
    let percent_defs: Vec<(String, f64)> = defs
        .iter()
        .filter(|(_, d)| d.percent)
        .map(|(n, d)| (n.clone(), d.rate.trim().parse::<f64>().unwrap_or(0.0)))
        .collect();
    // A layout with no fringe figures: what the lines were worked out on
    // comes from their fringe percentages, shown to two places.
    let slack_of = |name: &str| -> f64 {
        if given.fringe_figures {
            return 0.0;
        }
        let rate = percent_defs
            .iter()
            .find(|(k, _)| k == name)
            .map_or(0.0, |(_, r)| *r);
        if rate <= 0.0 {
            return 0.0;
        }
        lines
            .iter()
            .filter(|l| l.percent.contains(name))
            .map(|l| 0.005 * l.total.abs() / rate)
            .sum()
    };
    let target = |name: &str| -> f64 {
        figures
            .iter()
            .filter(|f| f.name == name && f.percent)
            .map(|f| f.amount)
            .sum()
    };
    {
        for pass in 0..4 {
            cutoffs.clear();
            unsolved.clear();
            for (name, _) in &percent_defs {
                let ea = earners(&lines, &ranges, name);
                let full: f64 = ea.iter().map(|e| capped(e, f64::INFINITY)).sum();
                let t = target(name);
                if t.is_nan() {
                    // No figures to work a cutoff out from.
                    cutoffs.insert(name.clone(), None);
                    no_target.insert(name.clone());
                    continue;
                }
                let slack = slack_of(name);
                if t >= full - 0.005 - slack {
                    cutoffs.insert(name.clone(), None);
                    continue;
                }
                match solve_cutoff(&ea, t, slack) {
                    Some(c) => {
                        cutoffs.insert(name.clone(), Some(c));
                    }
                    None => {
                        cutoffs.insert(name.clone(), None);
                        unsolved.push(name.clone());
                    }
                }
            }
            if pass == 3 {
                break;
            }
            // Lines that earned less than their rate on their own: in a
            // range with the lines above.
            let rate_of = |n: &str| {
                percent_defs
                    .iter()
                    .find(|(k, _)| k == n)
                    .map_or(0.0, |(_, r)| *r)
            };
            let mut changed = false;
            for t in 0..lines.len() {
                let l = &lines[t];
                if ranges[t].is_some()
                    || l.flat
                    || l.percent.is_empty()
                    || l.total <= 0.0
                    || l.percent
                        .iter()
                        .any(|n| unsolved.contains(n) || no_target.contains(n))
                {
                    continue;
                }
                let expected: f64 = l
                    .percent
                    .iter()
                    .map(|n| {
                        let base = match cutoffs.get(n).copied().flatten() {
                            Some(c) => l.total.min(c as f64 / 100.0),
                            None => l.total,
                        };
                        rate_of(n) * base / l.total
                    })
                    .sum();
                let tol = 0.006 + 100.0 * 0.0051 * l.percent.len() as f64 / l.total;
                if l.earned.unwrap_or(0.0) < expected - tol
                    && join_back(&lines, &mut ranges, t, &mut next_range)
                {
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
    }
    for (i, l) in lines.iter().enumerate() {
        if let Some(r) = ranges[i] {
            if let TopsheetRow::Category(c) = &mut b.topsheet[l.at.0] {
                c.accounts[l.at.1].details[l.at.2].fringe_range = Some(r);
            }
        }
    }
    let range_count = ranges.iter().flatten().collect::<BTreeSet<_>>().len();

    // Fringe definitions.
    for (name, d) in &defs {
        let kind = if d.percent {
            FringeKind::Percent {
                cutoff: cutoffs.get(name).copied().flatten().map(|c| {
                    if converted {
                        cents(c as f64 / 100.0 * conv_rate)
                    } else {
                        c
                    }
                }),
            }
        } else {
            let u = unit(&d.unit);
            FringeKind::flat((!u.is_empty()).then_some(u))
        };
        b.fringes.insert(
            name.clone(),
            Fringe {
                description: String::new(),
                short_code: None,
                rate: Formula(d.rate.trim().to_string()),
                kind,
            },
        );
    }
    // A layout with no fringe figures: the cutoffs worked out from the
    // lines' rounded percentages are near; the one that gives the
    // accounts' fringe totals exactly is the one.
    if !given.fringe_figures {
        let wanted: BTreeMap<String, Cents> = im
            .links
            .accounts
            .iter()
            .map(|a| (a.code.clone(), cents(a.fringe)))
            .collect();
        let score = |b: &Budget| -> Option<Cents> {
            let r = method_core::recalc(b, &method_core::RecalcOptions::default()).ok()?;
            Some(
                r.categories
                    .iter()
                    .flat_map(|c| &c.accounts)
                    .filter_map(|a| wanted.get(&a.code).map(|w| (a.fringe_total - w).abs()))
                    .sum(),
            )
        };
        let solved_names: Vec<(String, Cents)> = cutoffs
            .iter()
            .filter_map(|(n, c)| c.map(|c| (n.clone(), c)))
            .collect();
        for (name, c0) in solved_names {
            let w = ((slack_of(&name) * 100.0).ceil() as Cents).clamp(1, 1000);
            let set = |b: &mut Budget, c: Cents| {
                if let Some(f) = b.fringes.get_mut(&name) {
                    f.kind = FringeKind::Percent { cutoff: Some(c) };
                }
            };
            let scored: Vec<(Cents, Cents)> = ((c0 - w).max(1)..=(c0 + w))
                .filter_map(|c| {
                    set(&mut b, c);
                    score(&b).map(|s| (c, s))
                })
                .collect();
            // Of the cutoffs that fit best, the middle of the run nearest
            // the estimate (every one of them gives the same totals).
            let pick = scored.iter().map(|(_, s)| *s).min().and_then(|min| {
                let mut runs: Vec<(Cents, Cents)> = Vec::new();
                for (c, _) in scored.iter().filter(|(_, s)| *s == min) {
                    match runs.last_mut() {
                        Some(r) if r.1 + 1 == *c => r.1 = *c,
                        _ => runs.push((*c, *c)),
                    }
                }
                runs.into_iter()
                    .min_by_key(|(lo, hi)| {
                        if (*lo..=*hi).contains(&c0) {
                            0
                        } else {
                            (lo - c0).abs().min((hi - c0).abs())
                        }
                    })
                    .map(|(lo, hi)| lo + (hi - lo) / 2)
            });
            let c = pick.unwrap_or(c0);
            set(&mut b, c);
            cutoffs.insert(name, Some(c));
        }
    }

    // Hours of the units added, from what the flat fringes counted.
    let mut unit_hours: BTreeMap<String, Option<f64>> = b
        .units
        .iter()
        .map(|(k, u)| (k.clone(), u.hours.map(|h| h.to_f64())))
        .collect();
    let mut unknown: BTreeSet<String> = units_added.iter().cloned().collect();
    let flat_eqs: Vec<FlatEquation> = defs
        .iter()
        .filter(|(_, d)| !d.percent)
        .map(|(name, d)| {
            let u = unit(&d.unit);
            let target: f64 = figures
                .iter()
                .filter(|f| f.name == *name && !f.percent)
                .map(|f| f.amount)
                .sum();
            let rows = lines
                .iter()
                .filter(|l| !l.unposted && l.names.contains(name))
                .map(|l| (l.amount, l.unit.clone()))
                .collect();
            ((!u.is_empty()).then_some(u), target, rows)
        })
        .collect();
    let counted = |hours: &BTreeMap<String, Option<f64>>,
                   fu: &Option<String>,
                   rows: &[(f64, String)]|
     -> f64 {
        let h = |u: &str| hours.get(u).copied().flatten().filter(|h| *h > 0.0);
        rows.iter()
            .map(|(amount, lu)| match (fu.as_deref().and_then(h), h(lu)) {
                (Some(f), Some(l)) => amount * l / f,
                _ => 1.0,
            })
            .sum()
    };
    let close = |a: f64, b: f64| (a - b).abs() <= 1e-6 * a.abs().max(b.abs()).max(1.0);
    let mut worked_hours: Vec<String> = Vec::new();
    loop {
        let mut progress = false;
        for (fu, target, rows) in &flat_eqs {
            let involved: BTreeSet<&String> = fu
                .iter()
                .chain(rows.iter().map(|(_, u)| u))
                .filter(|u| unknown.contains(*u))
                .collect();
            let [u] = involved.into_iter().collect::<Vec<_>>()[..] else {
                continue;
            };
            let u = u.clone();
            let found = (0..=2000).find(|h| {
                unit_hours.insert(u.clone(), (*h > 0).then_some(*h as f64));
                close(counted(&unit_hours, fu, rows), *target)
            });
            match found {
                Some(h) => {
                    unit_hours.insert(u.clone(), (h > 0).then_some(h as f64));
                    if h > 0 {
                        worked_hours.push(format!("{u} {h}"));
                    }
                    unknown.remove(&u);
                    progress = true;
                }
                None => {
                    unit_hours.insert(u.clone(), None);
                }
            }
        }
        if !progress {
            break;
        }
    }
    for (u, h) in &unit_hours {
        if let (Some(h), Some(def)) = (h, b.units.get_mut(u)) {
            if def.hours.is_none() {
                def.hours = Some(Dec::from_int(*h as i64));
            }
        }
    }
    let mut rateless: Vec<String> = Vec::new();
    for n in &fringe_names {
        if !b.fringes.contains_key(n) {
            rateless.push(n.clone());
            b.fringes.insert(
                n.clone(),
                Fringe {
                    description: String::new(),
                    short_code: None,
                    rate: Formula::from("0"),
                    kind: FringeKind::Percent { cutoff: None },
                },
            );
        }
    }
    im.counts.fringes = b.fringes.len();

    // Groups, sets, locations.
    for c in b.topsheet.iter().filter_map(|r| match r {
        TopsheetRow::Category(c) => Some(c),
        _ => None,
    }) {
        for d in c.accounts.iter().flat_map(|a| &a.details) {
            for g in &d.groups {
                b.groups.entry(g.clone()).or_insert_with(|| Group {
                    description: String::new(),
                    short_code: None,
                    in_budget_total: true,
                    color: None,
                });
            }
            if let Some(s) = &d.set {
                b.sets.entry(s.clone()).or_default();
            }
            if let Some(l) = &d.location {
                b.locations.entry(l.clone()).or_default();
            }
        }
    }
    im.counts.groups = b.groups.len();
    im.counts.sets = b.sets.len();
    im.counts.locations = b.locations.len();

    // Originals, as the baseline.
    let any_original = x.rows.iter().any(|r| match r {
        TopRow::Category(c) => c.original != 0.0 || c.accounts.iter().any(|a| a.original != 0.0),
        TopRow::Fringes(t) | TopRow::Charge(t) | TopRow::Credit(t) => t.original != 0.0,
        _ => false,
    });
    if any_original {
        let to_base = |v: f64| -> Cents {
            if converted {
                cents(v * conv_rate)
            } else {
                cents(v)
            }
        };
        let mut bl = Baseline::default();
        let mut grand: Cents = 0;
        let mut credits: Cents = 0;
        for r in &x.rows {
            match r {
                TopRow::Category(c) if is_real_category(c) => {
                    bl.categories
                        .insert(c.number.trim().to_string(), to_base(c.original));
                    grand += to_base(c.original);
                    for a in c.accounts.iter().filter(|a| is_real_account(a)) {
                        bl.accounts
                            .insert(a.number.trim().to_string(), to_base(a.original));
                    }
                }
                TopRow::Fringes(t) | TopRow::Charge(t) => grand += to_base(t.original),
                TopRow::Credit(t) => credits += to_base(t.original),
                _ => {}
            }
        }
        bl.grand_total = grand;
        bl.net_total = grand + credits;
        b.baseline = Some(bl);
    }

    // Information fields.
    for (label, text) in &x.info {
        b.info.insert(label.clone(), info_value(label, text));
    }

    // Production-level posting: what the export leaves out after the last
    // level is left out here too.
    if matches!(posting, Posting::Level) && lines.iter().any(|l| l.unposted && l.any_fringe) {
        b.trailing_level_fringes = TrailingFringes::Drop;
    }

    // ---- Notes ----
    let posting_words = match &posting {
        Posting::Account => "by account (a Total Fringes row at the end of each account)".to_string(),
        Posting::Category(s) => format!("by category (a fringe account numbered with {s} in each)"),
        Posting::Level => format!(
            "by production level ({} level{}; the export doesn't name the production totals, so they are called Production total 1, 2...)",
            fringe_row_count,
            if fringe_row_count == 1 { "" } else { "s" }
        ),
        Posting::Budget => "by budget (one Total Fringes row)".to_string(),
    };
    if x.totals_only {
        im.note(
            NoteKind::Lost,
            "This layout holds only the topsheet and the accounts' totals: each account comes in as one line holding its total (fringe accounts too), and Total Fringes rows as fixed charges; lines, fringes, groups and other setup aren't in it.",
        );
    } else {
        im.note(NoteKind::Check, format!("Fringes post {posting_words}."));
    }
    if !x.totals_only {
        let missing: Vec<&str> = [(!x.line_groups, "groups"), (!x.line_globals, "globals")]
            .into_iter()
            .filter_map(|(m, w)| m.then_some(w))
            .collect();
        if !missing.is_empty() {
            im.note(
                NoteKind::Lost,
                format!(
                    "This layout doesn't name the lines' {}: none came over.",
                    missing.join(" or ")
                ),
            );
        }
    }
    if x.places == 0 {
        im.note(
            NoteKind::Info,
            "This layout gives its figures in whole units: they are compared rounded to whole units, and charges and credits come in at the whole unit.",
        );
    }
    if b.trailing_level_fringes == TrailingFringes::Drop {
        im.note(
            NoteKind::Check,
            "The categories after the last production total have fringes the export leaves out of every total; the budget is set to leave them out too (trailing level fringes: drop).",
        );
    }
    let has_levels = b
        .topsheet
        .iter()
        .any(|r| matches!(r, TopsheetRow::ProductionTotal(_)));
    if !has_levels {
        im.note(
            NoteKind::Lost,
            "Production totals aren't in the export, so none were added: above- and below-the-line are 0 until they are (Budget Setup).",
        );
    }
    im.note(
        NoteKind::Check,
        format!(
            "Account numbers read as category {} and account {} characters{}.",
            coding.category_width,
            coding.account_width,
            match coding.separator {
                Some(c) => format!(", separated by '{c}'"),
                None => ", with no separator".into(),
            }
        ),
    );
    for (from, to) in &renamed_codes {
        im.note(
            NoteKind::Check,
            format!("Account number {from} is used twice; the second is {to}."),
        );
    }
    if !x.totals_only {
        im.note(
            NoteKind::Check,
            "Unit hours aren't in the export: Day 12, Week 60, Hour 1 and Month 240 were used.",
        );
    }
    if !worked_hours.is_empty() {
        im.note(
            NoteKind::Check,
            format!(
                "Hours of units worked out from what the flat fringes counted: {}.",
                worked_hours.join(", ")
            ),
        );
    }
    let hourless: Vec<&String> = units_added
        .iter()
        .filter(|u| b.units.get(*u).is_some_and(|d| d.hours.is_none()))
        .collect();
    if !hourless.is_empty() {
        im.note(
            NoteKind::Check,
            format!(
                "Units added with no hours (set them if flat fringes count them): {}.",
                hourless.into_iter().cloned().collect::<Vec<_>>().join(", ")
            ),
        );
    }
    let solved: Vec<String> = cutoffs
        .iter()
        .filter(|(_, c)| c.is_some())
        .filter_map(|(n, _)| b.fringes.get(n).map(|f| (n, f)))
        .filter_map(|(n, f)| f.kind.cutoff().map(|c| format!("{n} {}", money(c))))
        .collect();
    if !solved.is_empty() {
        im.note(
            NoteKind::Check,
            format!(
                "Fringe cutoffs aren't in the export; worked out from the fringe figures: {}.",
                solved.join(", ")
            ),
        );
    }
    if range_count > 0 {
        im.note(
            NoteKind::Check,
            format!(
                "{range_count} fringe range{} worked out from lines that earned less than their rate.",
                if range_count == 1 { "" } else { "s" }
            ),
        );
    }
    if !unsolved.is_empty() {
        im.note(
            NoteKind::Check,
            format!(
                "The cutoff of {} couldn't be worked out (set it in Fringes).",
                unsolved.join(", ")
            ),
        );
    }
    if converted && !solved.is_empty() {
        im.note(
            NoteKind::Check,
            format!("The cutoffs were worked out in {conv} and converted to {base} at the worked-out rate, so they may be a cent out."),
        );
    }
    if !conflicting.is_empty() {
        im.note(
            NoteKind::Check,
            format!(
                "Fringes with different rates in different places (the first was used): {}.",
                conflicting.into_iter().collect::<Vec<_>>().join(", ")
            ),
        );
    }
    if !rateless.is_empty() {
        im.note(
            NoteKind::Lost,
            format!(
                "Fringes with no figures in the export, added at 0%: {}.",
                rateless.join(", ")
            ),
        );
    }
    if !rate_words.is_empty() {
        im.note(
            NoteKind::Check,
            format!(
                "Exchange rates aren't in the export; worked out from the lines: {}.",
                rate_words.join(", ")
            ),
        );
    }
    if !inconsistent.is_empty() {
        im.note(
            NoteKind::Check,
            format!(
                "Lines in {} don't agree on one rate.",
                inconsistent.into_iter().collect::<Vec<_>>().join(", ")
            ),
        );
    }
    if !rateless_currencies.is_empty() || !conv_known {
        let mut v = rateless_currencies;
        if !conv_known {
            v.push(conv.clone());
        }
        im.note(
            NoteKind::Check,
            format!("No line shows the rate of {}: set to 1.", v.join(", ")),
        );
    }
    if converted {
        im.note(
            NoteKind::Info,
            format!(
                "The export's figures are in {conv}; the budget's base currency is {base} and it shows its totals in {conv}."
            ),
        );
    }
    if im.counts.adjustments > 0 {
        im.note(
            NoteKind::Info,
            format!(
                "{} adjustment row{} (a rate equal to the rows above it).",
                im.counts.adjustments,
                if im.counts.adjustments == 1 { "" } else { "s" }
            ),
        );
    }
    // A charge that is exactly a round percentage of the total before
    // charges may well be a percent charge: say so.
    let before_charges: f64 = im.links.categories.iter().map(|c| c.total).sum::<f64>()
        + im.links.level_fringes.iter().sum::<f64>()
        + im.links.budget_fringes.unwrap_or(0.0);
    let base_cents = cents(before_charges);
    let hints: Vec<String> = im
        .links
        .charges
        .iter()
        .filter_map(|(name, total)| {
            let t = cents(*total);
            if t == 0 || base_cents == 0 {
                return None;
            }
            let p = t as f64 / base_cents as f64 * 100.0;
            (0..=3).find_map(|dp| {
                let cand: f64 = format!("{p:.dp$}").parse().ok()?;
                ((base_cents as f64 * cand / 100.0).round() as Cents == t && cand > 0.0)
                    .then(|| format!("{name} is {}%", trim_zeros(&format!("{cand:.dp$}"))))
            })
        })
        .collect();
    if !hints.is_empty() {
        im.note(
            NoteKind::Check,
            format!(
                "Of the total before charges, {}: re-enter {} as a percent charge if that is what {}.",
                hints.join(", "),
                if hints.len() == 1 { "it" } else { "them" },
                if hints.len() == 1 { "it is" } else { "they are" }
            ),
        );
    }
    if !b.charges.is_empty() {
        im.note(
            NoteKind::Lost,
            format!(
                "Charges come in as fixed amounts (their percentages and exclusions aren't in the export): {}.",
                b.charges
                    .iter()
                    .map(|c| c.name.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        );
    }
    if !b.credits.is_empty() {
        im.note(
            NoteKind::Lost,
            format!(
                "Credits come in as fixed amounts (their rows, caps and discounts aren't in the export): {}.",
                b.credits
                    .iter()
                    .map(|c| c.name.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        );
    }
    if !globals_used.is_empty() {
        im.note(
            NoteKind::Lost,
            format!(
                "Formulas aren't in the export: {} line{} used globals ({}); they keep the values, with a note naming the globals.",
                im.counts.lines_with_globals,
                if im.counts.lines_with_globals == 1 { "" } else { "s" },
                globals_used.keys().cloned().collect::<Vec<_>>().join(", ")
            ),
        );
    }
    let count_of = |key: &str| x.counts.iter().find(|(k, _)| k == key).map(|(_, n)| *n);
    if let Some(n) = count_of("GlobalCount") {
        if n > 0 {
            im.note(
                NoteKind::Lost,
                format!(
                    "The budget has {n} global{}; globals aren't in the export, so none were created.",
                    if n == 1 { "" } else { "s" }
                ),
            );
        }
    }
    for (key, what, have) in [
        ("FringeCount", "fringes", defs.len() + rateless.len()),
        ("LocationCount", "locations", b.locations.len()),
        ("SetCount", "sets", b.sets.len()),
    ] {
        if let Some(n) = count_of(key) {
            if n as usize > have {
                im.note(
                    NoteKind::Lost,
                    format!(
                        "The budget has {n} {what}; {} not used on any line in the export and couldn't come over.",
                        n as usize - have
                    ),
                );
            }
        }
    }
    im.note(
        NoteKind::Lost,
        "Lines left out of the totals by a group aren't in the export, nor are their groups unless other lines use them.",
    );
    if b.baseline.is_some() {
        im.note(
            NoteKind::Info,
            "The export's originals are the budget's baseline (by category and account).",
        );
    }
    if im.counts.blank_rows > 0 {
        im.note(
            NoteKind::Info,
            format!(
                "{} empty entry row{} passed over.",
                im.counts.blank_rows,
                if im.counts.blank_rows == 1 { "" } else { "s" }
            ),
        );
    }
    im.budget = b;
    im
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simplest_rates() {
        assert_eq!(simplest(2.99999, 3.00001), "3");
        assert_eq!(simplest(1.23455, 1.23465), "1.2346");
        assert_eq!(simplest(1.2345649, 1.2345651), "1.234565");
    }

    #[test]
    fn capped_uses_the_cutoff_in_order() {
        assert_eq!(capped(&[3000.0, -500.0], 1000.0), 500.0);
        assert_eq!(capped(&[1249.36, 5309.78, 624.68], 2222.22), 2222.22);
        assert_eq!(capped(&[-1000.0], 2222.22), -1000.0);
        let c = solve_cutoff(
            &[vec![1000.0], vec![5000.0], vec![1500.0, 1500.0]],
            1000.0 + 2222.22 * 2.0,
            0.0,
        )
        .unwrap();
        assert_eq!(c, 222222);
        assert_eq!(solve_cutoff(&[vec![-5.0]], 10.0, 0.0), None);
    }

    #[test]
    fn units_by_singular_and_plural() {
        let mut units = method_core::default_units();
        let seen: BTreeMap<String, bool> = [
            ("Days".to_string(), true),
            ("ZSHIFT".to_string(), false),
            ("ZSHIFTS".to_string(), true),
            ("ZHALFS".to_string(), true),
            ("ZZERO".to_string(), false),
        ]
        .into_iter()
        .collect();
        let (map, added) = resolve_units(&seen, &mut units);
        assert_eq!(map["Days"], "Day");
        assert_eq!(map["ZSHIFTS"], "ZSHIFT");
        assert_eq!(map["ZSHIFT"], "ZSHIFT");
        assert_eq!(map["ZHALFS"], "ZHALF");
        assert_eq!(units["ZZERO"].plural, "ZZEROS");
        assert_eq!(added, vec!["ZHALF", "ZSHIFT", "ZZERO"]);
    }

    #[test]
    fn dates_and_numbers_in_info() {
        assert_eq!(iso_date("2026-10-11"), Some("2026-10-11".into()));
        assert_eq!(iso_date("10/11/2026"), Some("2026-10-11".into()));
        assert_eq!(iso_date("soon"), None);
        assert!(matches!(
            info_value("Stage shoot days", "12"),
            InfoValue::Number(_)
        ));
        assert!(matches!(info_value("Status", "12"), InfoValue::Text(_)));
    }
}
