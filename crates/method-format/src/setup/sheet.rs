//! Setup lists as spreadsheets (decisions.md, entry 50): each kind of setup
//! item as a table of its own, one row per item (per credit row for
//! credits), written as an .xlsx sheet per kind or a CSV file, and read
//! back with a column mapping into a setup-only budget that Import Setup's
//! review then brings into a budget or a pack (method_format::setup::import).
//!
//! - **Columns per kind** are each item's whole definition as the setup
//!   tables show it, plus where it came from (`Effective`, `Source Notes`,
//!   `Source Links`). Lists (a charge's excluded groups, a tag preset's
//!   fringes) are names joined with `, `, as the grid writes them.
//! - **Reading is checked, row by row.** Every problem is listed with its
//!   row number in the sheet (as the spreadsheet numbers rows) and nothing
//!   is imported while any is left: names unique and present, numbers
//!   numbers, formulas that parse and name only globals the sheet or the
//!   budget defines, dates dates, links web links.
//! - **Not in a sheet**: line blocks and report presets (rows and page
//!   layouts, not lists), a charge's excluded amounts (they belong to one
//!   budget's lines) and the budget settings. Export says how many charges
//!   lose excluded amounts.

use std::collections::{BTreeMap, BTreeSet};

use method_core::expr::{is_ident, Expr};
use method_core::overtime::{OtBand, OtPattern};
use method_core::pack::{self as core_pack, ItemSource};
use method_core::{
    Budget, Cents, Charge, ChargeKind, Credit, CreditCap, CreditRow, CreditSource, Currency, Dec,
    FlatBasis, Formula, Fringe, FringeKind, GlobalInfo, Group, Location, Separators, SetDef,
    TagPreset, UnitDef,
};
use serde::{Deserialize, Serialize};

use super::import::What;
use crate::export::{Cell, Sheet, Workbook};
use crate::import::mapping::{norm, parse_number_text};
use crate::import::{Table, Value};

/// The kinds a spreadsheet can hold, in workbook order.
pub const SHEET_KINDS: [What; 11] = [
    What::Global,
    What::Fringe,
    What::Group,
    What::Unit,
    What::Set,
    What::Location,
    What::Currency,
    What::Charge,
    What::Credit,
    What::TagPreset,
    What::OtPattern,
];

/// One column of a kind's table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Column {
    /// What the page and a saved mapping call it.
    pub key: &'static str,
    /// Its title in an export, and the title a guess looks for first.
    pub title: &'static str,
    /// Other titles that mean it, normalized ([`norm`]).
    pub also: &'static [&'static str],
}

const fn col(key: &'static str, title: &'static str, also: &'static [&'static str]) -> Column {
    Column { key, title, also }
}

const SOURCE: [Column; 3] = [
    col(
        "effective",
        "Effective",
        &["effectivedate", "effectivefrom", "asof"],
    ),
    col(
        "source_notes",
        "Source Notes",
        &["source", "sources", "notes"],
    ),
    col(
        "source_links",
        "Source Links",
        &["sourcelink", "links", "link", "url", "urls", "sourceurl"],
    ),
];

const GLOBAL: [Column; 4] = [
    col("name", "Name", &["global", "variable"]),
    col("formula", "Formula", &["value", "amount", "expression"]),
    col("description", "Description", &["desc"]),
    col("unit", "Unit", &["units"]),
];
const FRINGE: [Column; 9] = [
    col("name", "Name", &["fringe"]),
    col("description", "Description", &["desc"]),
    col("short_code", "Short Code", &["code", "short"]),
    col("kind", "Kind", &["type", "percentorflat"]),
    col(
        "rate",
        "Rate",
        &["percent", "amount", "value", "percentage"],
    ),
    col("cutoff", "Cutoff", &["cap", "ceiling", "limit", "wagecap"]),
    col("unit", "Unit", &["per", "perunit"]),
    col("basis", "Basis", &["counts", "counting", "countedon"]),
    col(
        "max_hours_per_day",
        "Max Hours Per Day",
        &["maxhours", "hourcap", "maxhoursaday", "dailyhourcap"],
    ),
];
const GROUP: [Column; 5] = [
    col("name", "Name", &["group"]),
    col("description", "Description", &["desc"]),
    col("short_code", "Short Code", &["code", "short"]),
    col(
        "in_total",
        "In Total",
        &["inbudgettotal", "counted", "included"],
    ),
    col("color", "Color", &["colour"]),
];
const UNIT: [Column; 4] = [
    col("name", "Name", &["unit", "singular"]),
    col("plural", "Plural", &[]),
    col("description", "Description", &["desc"]),
    col("hours", "Hours", &["hoursper", "equivalenthours"]),
];
const SET: [Column; 2] = [
    col("name", "Name", &["set"]),
    col("description", "Description", &["desc"]),
];
const LOCATION: [Column; 5] = [
    col("name", "Name", &["location"]),
    col("description", "Description", &["desc"]),
    col("city", "City", &[]),
    col("region", "Region", &["state", "province"]),
    col("country", "Country", &[]),
];
const CURRENCY: [Column; 6] = [
    col("name", "Code", &["currency", "name"]),
    col("description", "Description", &["desc"]),
    col("symbol", "Symbol", &["sign"]),
    col("rate", "Rate", &["exchangerate", "fx"]),
    col(
        "symbol_after",
        "Symbol After",
        &["symbolafteramount", "symbolaftervalue"],
    ),
    col("separators", "Separators", &["separator", "numberformat"]),
];
const CHARGE: [Column; 9] = [
    col("name", "Name", &["charge"]),
    col("kind", "Kind", &["type"]),
    col("rate", "Rate", &["percent", "amount", "value"]),
    col("currency", "Currency", &[]),
    col("account", "Account", &["acct", "account#"]),
    col("applied", "Applied", &["on", "active"]),
    col("excluded_groups", "Excluded Groups", &["excludegroups"]),
    col(
        "excluded_categories",
        "Excluded Categories",
        &["excludecategories"],
    ),
    col(
        "excluded_accounts",
        "Excluded Accounts",
        &["excludeaccounts"],
    ),
];
const CREDIT: [Column; 15] = [
    col("name", "Credit", &["name", "incentive"]),
    col("account", "Account", &["acct"]),
    col("applied", "Applied", &["on", "active"]),
    col("cap", "Cap", &["creditcap"]),
    col("discount", "Discount %", &["discount"]),
    col("row", "Row", &["rowdescription", "description"]),
    col("group", "Group", &["sourcegroup"]),
    col("category", "Category", &["sourcecategory"]),
    col(
        "row_account",
        "Row Account",
        &["sourceaccount", "spendaccount"],
    ),
    col("amount", "Flat Amount", &["flat", "grossamount"]),
    col("percent", "Percent", &["rate", "creditpercent"]),
    col("row_cap_on", "Row Cap On", &["capon", "capkind"]),
    col("row_cap", "Row Cap", &[]),
    col("excluded_fringes", "Excluded Fringes", &["excludefringes"]),
    col("locations", "Locations", &["location"]),
];
const TAG_PRESET: [Column; 6] = [
    col("name", "Name", &["preset", "tagpreset"]),
    col("fringes", "Fringes", &["fringe"]),
    col("groups", "Groups", &["group"]),
    col("set", "Set", &[]),
    col("location", "Location", &[]),
    col("currency", "Currency", &[]),
];
const OT_PATTERN: [Column; 5] = [
    col("name", "Name", &["pattern"]),
    col("straight", "Straight Hours", &["straight", "straighttime"]),
    col("bands", "Bands", &["overtime", "overtimebands"]),
    col("sixth", "6th Day", &["sixthday", "6thday"]),
    col("seventh", "7th Day", &["seventhday", "7thday"]),
];

/// The columns of kind `w`'s table, where each item came from last (empty
/// for a kind no sheet holds).
pub fn columns(w: What) -> Vec<Column> {
    let own: &[Column] = match w {
        What::Global => &GLOBAL,
        What::Fringe => &FRINGE,
        What::Group => &GROUP,
        What::Unit => &UNIT,
        What::Set => &SET,
        What::Location => &LOCATION,
        What::Currency => &CURRENCY,
        What::Charge => &CHARGE,
        What::Credit => &CREDIT,
        What::TagPreset => &TAG_PRESET,
        What::OtPattern => &OT_PATTERN,
        _ => return Vec::new(),
    };
    own.iter().chain(SOURCE.iter()).copied().collect()
}

/// Kind `w`'s sheet name ("Fringes", "Tag Presets").
pub fn sheet_name(w: What) -> &'static str {
    match w {
        What::Global => "Globals",
        What::Fringe => "Fringes",
        What::Group => "Groups",
        What::Unit => "Units",
        What::Set => "Sets",
        What::Location => "Locations",
        What::Currency => "Currencies",
        What::Charge => "Charges",
        What::Credit => "Credits",
        What::TagPreset => "Tag Presets",
        What::OtPattern => "Overtime Patterns",
        _ => "Setup",
    }
}

/// The kind a sheet or file name names (`"Fringes"`, `"Show - Fringes"`,
/// `"fringes.csv"`), if any.
pub fn kind_named(name: &str) -> Option<What> {
    let n = norm(name);
    SHEET_KINDS
        .into_iter()
        .filter(|w| n.ends_with(&norm(sheet_name(*w))) || n.contains(&norm(sheet_name(*w))))
        .max_by_key(|w| norm(sheet_name(*w)).len())
}

// ---------------------------------------------------------------------------
// Export

fn yes_no(b: bool) -> Cell {
    Cell::text(if b { "Yes" } else { "No" })
}

fn list<'a>(names: impl IntoIterator<Item = &'a String>) -> Cell {
    Cell::text(names.into_iter().cloned().collect::<Vec<_>>().join(", "))
}

/// A flat fringe's basis as a sheet may write it: the application's labels
/// (`Amount only (standard)`), the file's names (`amount_counts`) or short
/// forms (`Full quantity`, `quantity`), ignoring case, spaces and marks.
pub fn basis_named(text: &str) -> Option<FlatBasis> {
    let key: String = text
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase();
    match key.as_str() {
        "amount" | "amountonly" | "amountonlystandard" | "standard" => Some(FlatBasis::Amount),
        "amountcounts" | "amountpluscounts" | "amountpluscountsinxand4x" | "counts" => {
            Some(FlatBasis::AmountCounts)
        }
        "quantity" | "fullquantity" => Some(FlatBasis::Quantity),
        _ => None,
    }
}

fn opt(s: &Option<String>) -> Cell {
    Cell::text(s.clone().unwrap_or_default())
}

fn source_cells(b: &Budget, w: What, name: &str) -> Vec<Cell> {
    let s = b
        .setup_sources
        .get(w.key(), name)
        .cloned()
        .unwrap_or_default();
    vec![
        Cell::text(s.effective),
        Cell::text(s.notes),
        Cell::text(s.urls.join(" ")),
    ]
}

/// The bands of an overtime pattern in words: `2 @ 1.5; rest @ 2`.
pub fn bands_text(bands: &[OtBand]) -> String {
    bands
        .iter()
        .map(|b| match b.hours {
            Some(h) => format!("{h} @ {}", b.multiplier),
            None => format!("rest @ {}", b.multiplier),
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// Kind `w` of budget `b` as a table (in the budget's order).
pub fn sheet(b: &Budget, w: What) -> Sheet {
    let cols = columns(w);
    let mut rows: Vec<Vec<Cell>> = Vec::new();
    let mut push = |name: &str, mut own: Vec<Cell>| {
        own.extend(source_cells(b, w, name));
        rows.push(own);
    };
    let ordered = |kind: method_core::SetupKind| method_core::order::ordered(b, kind);
    match w {
        What::Global => {
            for n in ordered(method_core::SetupKind::Global) {
                let Some(f) = b.globals.get(&n) else { continue };
                let info = b.global_info.get(&n).cloned().unwrap_or_default();
                push(
                    &n,
                    vec![
                        Cell::text(n.clone()),
                        Cell::text(f.0.clone()),
                        Cell::text(info.description),
                        Cell::text(info.unit),
                    ],
                );
            }
        }
        What::Fringe => {
            for n in ordered(method_core::SetupKind::Fringe) {
                let Some(f) = b.fringes.get(&n) else { continue };
                let (kind, unit, basis, max_hours) = match &f.kind {
                    FringeKind::Percent { .. } => {
                        ("Percent", Cell::Empty, Cell::Empty, Cell::Empty)
                    }
                    FringeKind::Flat {
                        unit,
                        basis,
                        max_hours_per_day,
                        ..
                    } => (
                        "Flat",
                        opt(unit),
                        Cell::text(basis.label()),
                        max_hours_per_day.map_or(Cell::Empty, Cell::Number),
                    ),
                };
                // A flat fringe's cutoff is kept (never used), so it is written too.
                let cutoff = f.kind.cutoff().map_or(Cell::Empty, Cell::Money);
                push(
                    &n,
                    vec![
                        Cell::text(n.clone()),
                        Cell::text(f.description.clone()),
                        opt(&f.short_code),
                        Cell::text(kind),
                        Cell::text(f.rate.0.clone()),
                        cutoff,
                        unit,
                        basis,
                        max_hours,
                    ],
                );
            }
        }
        What::Group => {
            for n in ordered(method_core::SetupKind::Group) {
                let Some(g) = b.groups.get(&n) else { continue };
                push(
                    &n,
                    vec![
                        Cell::text(n.clone()),
                        Cell::text(g.description.clone()),
                        opt(&g.short_code),
                        yes_no(g.in_budget_total),
                        opt(&g.color),
                    ],
                );
            }
        }
        What::Unit => {
            for n in ordered(method_core::SetupKind::Unit) {
                let Some(u) = b.units.get(&n) else { continue };
                push(
                    &n,
                    vec![
                        Cell::text(n.clone()),
                        Cell::text(u.plural.clone()),
                        Cell::text(u.description.clone()),
                        u.hours.map_or(Cell::Empty, Cell::Number),
                    ],
                );
            }
        }
        What::Set => {
            for n in ordered(method_core::SetupKind::Set) {
                let Some(s) = b.sets.get(&n) else { continue };
                push(
                    &n,
                    vec![Cell::text(n.clone()), Cell::text(s.description.clone())],
                );
            }
        }
        What::Location => {
            for n in ordered(method_core::SetupKind::Location) {
                let Some(l) = b.locations.get(&n) else {
                    continue;
                };
                push(
                    &n,
                    vec![
                        Cell::text(n.clone()),
                        Cell::text(l.description.clone()),
                        Cell::text(l.city.clone()),
                        Cell::text(l.region.clone()),
                        Cell::text(l.country.clone()),
                    ],
                );
            }
        }
        What::Currency => {
            for n in ordered(method_core::SetupKind::Currency) {
                let Some(c) = b.currencies.get(&n) else {
                    continue;
                };
                push(
                    &n,
                    vec![
                        Cell::text(n.clone()),
                        Cell::text(c.description.clone()),
                        Cell::text(c.symbol.clone()),
                        Cell::Number(c.rate),
                        yes_no(c.format.symbol_after),
                        Cell::text(match c.format.separators {
                            Separators::CommaPoint => "1,234.56",
                            Separators::PointComma => "1.234,56",
                        }),
                    ],
                );
            }
        }
        What::Charge => {
            for c in &b.charges {
                let (kind, rate, cur) = match &c.kind {
                    ChargeKind::Percent(p) => ("Percent", Cell::Number(*p), Cell::Empty),
                    ChargeKind::Flat { amount, currency } => {
                        ("Flat", Cell::Money(*amount), opt(currency))
                    }
                };
                push(
                    &c.name,
                    vec![
                        Cell::text(c.name.clone()),
                        Cell::text(kind),
                        rate,
                        cur,
                        opt(&c.account),
                        yes_no(c.applied),
                        list(&c.excluded_groups),
                        list(&c.excluded_categories),
                        list(&c.excluded_accounts),
                    ],
                );
            }
        }
        What::Credit => {
            for c in &b.credits {
                let head = |cells: &mut Vec<Cell>| {
                    cells.extend([
                        Cell::text(c.name.clone()),
                        opt(&c.account),
                        yes_no(c.applied),
                        c.cap.map_or(Cell::Empty, Cell::Money),
                        if c.discount_percent == Dec::ZERO {
                            Cell::Empty
                        } else {
                            Cell::Number(c.discount_percent)
                        },
                    ])
                };
                if c.rows.is_empty() {
                    let mut cells = Vec::new();
                    head(&mut cells);
                    cells.extend(std::iter::repeat_n(Cell::Empty, 8));
                    cells.push(list(&c.excluded_fringes));
                    cells.push(list(&c.locations));
                    push(&c.name, cells);
                }
                for r in &c.rows {
                    let mut cells = Vec::new();
                    head(&mut cells);
                    let (group, category, account, amount) = match &r.source {
                        CreditSource::Flat(a) => {
                            (Cell::Empty, Cell::Empty, Cell::Empty, Cell::Money(*a))
                        }
                        CreditSource::Group { group } => (
                            Cell::text(group.clone()),
                            Cell::Empty,
                            Cell::Empty,
                            Cell::Empty,
                        ),
                        CreditSource::Category { category } => (
                            Cell::Empty,
                            Cell::text(category.clone()),
                            Cell::Empty,
                            Cell::Empty,
                        ),
                        CreditSource::Account { account } => (
                            Cell::Empty,
                            Cell::Empty,
                            Cell::text(account.clone()),
                            Cell::Empty,
                        ),
                    };
                    // The credit's one list, repeated on each of its rows.
                    let excluded = list(&c.excluded_fringes);
                    let (on, cap) = match r.cap {
                        CreditCap::None => (Cell::Empty, Cell::Empty),
                        CreditCap::QualifiedSpend(x) => (Cell::text("Spend"), Cell::Money(x)),
                        CreditCap::Credit(x) => (Cell::text("Credit"), Cell::Money(x)),
                    };
                    cells.extend([
                        Cell::text(r.description.clone()),
                        group,
                        category,
                        account,
                        amount,
                        Cell::Number(r.percent),
                        on,
                        cap,
                        excluded,
                        list(&c.locations),
                    ]);
                    push(&c.name, cells);
                }
            }
        }
        What::TagPreset => {
            for t in &b.tag_presets {
                push(
                    &t.name,
                    vec![
                        Cell::text(t.name.clone()),
                        list(&t.fringes),
                        list(&t.groups),
                        opt(&t.set),
                        opt(&t.location),
                        opt(&t.currency),
                    ],
                );
            }
        }
        What::OtPattern => {
            for p in &b.ot_patterns {
                push(
                    &p.name,
                    vec![
                        Cell::text(p.name.clone()),
                        Cell::Number(p.straight_hours),
                        Cell::text(bands_text(&p.bands)),
                        p.sixth_day.map_or(Cell::Empty, Cell::Number),
                        p.seventh_day.map_or(Cell::Empty, Cell::Number),
                    ],
                );
            }
        }
        _ => {}
    }
    Sheet {
        name: sheet_name(w),
        columns: cols.iter().map(|c| c.title.to_string()).collect(),
        rows,
    }
}

/// The workbook of kinds `kinds` (every sheet kind when empty), a sheet per
/// kind, in [`SHEET_KINDS`] order.
pub fn workbook(b: &Budget, kinds: &[What]) -> Workbook {
    Workbook {
        title: b.title.clone(),
        sheets: SHEET_KINDS
            .into_iter()
            .filter(|w| kinds.is_empty() || kinds.contains(w))
            .map(|w| sheet(b, w))
            .collect(),
    }
}

/// How many charges have excluded amounts, which a sheet doesn't keep.
pub fn charges_losing_amounts(b: &Budget) -> usize {
    b.charges
        .iter()
        .filter(|c| !c.excluded_amounts.is_empty())
        .count()
}

// ---------------------------------------------------------------------------
// The mapping

/// Which row holds the titles and which column holds each field (by
/// [`Column::key`]), as the page shows and sends it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SheetMapping {
    /// The row of titles (0-based); data starts after it. `None`: no
    /// titles, data from the first row.
    pub header_row: Option<usize>,
    pub columns: BTreeMap<String, usize>,
}

impl SheetMapping {
    fn first_data_row(&self) -> usize {
        self.header_row.map_or(0, |h| h + 1)
    }
}

fn score(c: &Column, title: &str) -> u32 {
    if title.is_empty() {
        return 0;
    }
    if norm(c.title) == title {
        return 300;
    }
    if let Some(i) = c.also.iter().position(|a| *a == title) {
        return 200 - i as u32;
    }
    if title.contains(&norm(c.title)) && norm(c.title).len() >= 4 {
        return 100;
    }
    0
}

/// The guessed mapping of table `t` as kind `w`: the row among the first
/// thirty whose titles best name the kind's columns, each column used once
/// (an exact title beats a synonym, which beats a title containing it).
pub fn guess(t: &Table, w: What) -> SheetMapping {
    let cols = columns(w);
    let mut best: Option<(u32, usize, BTreeMap<String, usize>)> = None;
    for r in 0..t.rows.len().min(30) {
        let titles: Vec<String> = t.rows[r].iter().map(|v| norm(v.text())).collect();
        let mut pairs: Vec<(u32, usize, usize)> = Vec::new(); // score, column, field
        for (ci, title) in titles.iter().enumerate() {
            for (fi, c) in cols.iter().enumerate() {
                let s = score(c, title);
                if s > 0 {
                    pairs.push((s, ci, fi));
                }
            }
        }
        pairs.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
        let mut used_cols = BTreeSet::new();
        let mut map = BTreeMap::new();
        let mut total = 0;
        for (s, ci, fi) in pairs {
            let key = cols[fi].key.to_string();
            if used_cols.contains(&ci) || map.contains_key(&key) {
                continue;
            }
            used_cols.insert(ci);
            map.insert(key, ci);
            total += s;
        }
        if map.contains_key("name") && best.as_ref().is_none_or(|b| total > b.0) {
            best = Some((total, r, map));
        }
    }
    match best {
        Some((_, r, columns)) => SheetMapping {
            header_row: Some(r),
            columns,
        },
        // No titles: the export's order.
        None => SheetMapping {
            header_row: None,
            columns: cols
                .iter()
                .enumerate()
                .map(|(i, c)| (c.key.to_string(), i))
                .collect(),
        },
    }
}

/// The column titles of table `t` under `m` (or "Column A" ... without).
pub fn titles(t: &Table, m: &SheetMapping) -> Vec<String> {
    let width = t.width();
    (0..width)
        .map(|c| match m.header_row {
            Some(r) => {
                let s = t.cell(r, c).text().trim().to_string();
                if s.is_empty() {
                    format!("Column {}", crate::export::xlsx::column_name(c))
                } else {
                    s
                }
            }
            None => format!("Column {}", crate::export::xlsx::column_name(c)),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Reading

/// A problem on one row of a sheet.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RowError {
    /// The sheet's name.
    pub sheet: String,
    /// The row as the spreadsheet numbers it (the first row is 1).
    pub row: usize,
    pub message: String,
}

/// One row's cells by field key.
struct Row<'a> {
    t: &'a Table,
    m: &'a SheetMapping,
    r: usize,
    decimal_comma: bool,
}

impl Row<'_> {
    fn get(&self, key: &str) -> String {
        match self.m.columns.get(key) {
            Some(&c) => self.t.cell(self.r, c).text().trim().to_string(),
            None => String::new(),
        }
    }

    fn number(&self, key: &str, what: &str) -> Result<Option<Dec>, String> {
        let s = self.get(key);
        let s = s.trim_end_matches('%');
        let v = match self.m.columns.get(key).map(|&c| self.t.cell(self.r, c)) {
            Some(Value::Number(n)) => crate::import::mapping::parse_decimal(n).map(Some),
            _ => parse_number_text(s, self.decimal_comma),
        };
        v.map_err(|_| format!("{what} \u{201c}{}\u{201d} isn't a number", self.get(key)))
    }

    fn money(&self, key: &str, what: &str) -> Result<Option<Cents>, String> {
        match self.number(key, what)? {
            None => Ok(None),
            Some(d) => d
                .to_cents()
                .map(Some)
                .ok_or_else(|| format!("{what} is too large")),
        }
    }

    fn yes(&self, key: &str, what: &str, default: bool) -> Result<bool, String> {
        let s = self.get(key).to_ascii_lowercase();
        match s.as_str() {
            "" => Ok(default),
            "yes" | "y" | "true" | "1" | "on" | "x" => Ok(true),
            "no" | "n" | "false" | "0" | "off" => Ok(false),
            _ => Err(format!(
                "{what} should be Yes or No, not \u{201c}{}\u{201d}",
                self.get(key)
            )),
        }
    }

    fn names(&self, key: &str) -> BTreeSet<String> {
        self.get(key)
            .split(',')
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty())
            .collect()
    }

    fn opt(&self, key: &str) -> Option<String> {
        Some(self.get(key)).filter(|s| !s.is_empty())
    }

    fn blank(&self) -> bool {
        self.m
            .columns
            .values()
            .all(|&c| self.t.cell(self.r, c).is_empty())
    }

    fn source(&self) -> Result<ItemSource, String> {
        let urls: Vec<String> = self
            .get("source_links")
            .split(|c: char| c.is_whitespace() || c == ';')
            .map(str::to_string)
            .collect();
        core_pack::check_source(&ItemSource {
            pack: String::new(),
            pack_version: String::new(),
            effective: self.get("effective"),
            notes: self.get("source_notes"),
            urls,
        })
    }
}

fn check_formula(
    f: &str,
    what: &str,
    row: usize,
    uses: &mut Vec<(usize, String, BTreeSet<String>)>,
) -> Result<(), String> {
    let e =
        Expr::parse(f).map_err(|e| format!("{what} \u{201c}{f}\u{201d} isn't a formula: {e}"))?;
    let mut vars = BTreeSet::new();
    e.vars(&mut vars);
    uses.push((row, what.to_string(), vars));
    Ok(())
}

fn ot_bands(s: &str) -> Result<Vec<OtBand>, String> {
    let mut out = Vec::new();
    for part in s.split(';').map(str::trim).filter(|p| !p.is_empty()) {
        let (h, m) = part.split_once('@').ok_or_else(|| {
            format!("the band \u{201c}{part}\u{201d} should read like 2 @ 1.5 or rest @ 2")
        })?;
        let num = |t: &str| {
            crate::import::mapping::parse_decimal(
                t.trim().trim_end_matches(['x', 'X', 'h', 'H']).trim(),
            )
            .map_err(|_| {
                format!("the band \u{201c}{part}\u{201d} should read like 2 @ 1.5 or rest @ 2")
            })
        };
        let hours = if h.trim().eq_ignore_ascii_case("rest") {
            None
        } else {
            Some(num(h)?)
        };
        out.push(OtBand {
            multiplier: num(m)?,
            hours,
        });
    }
    Ok(out)
}

/// Reads table `t` as kind `w` under mapping `m` into `into` (a setup-only
/// budget), with where each item came from. Every problem is returned with
/// its row; `known_globals` are the names a formula may use besides the
/// globals read (the target budget's). Credits are read from consecutive
/// or scattered rows of the same credit name, in order.
pub fn read(
    t: &Table,
    w: What,
    m: &SheetMapping,
    known_globals: &BTreeSet<String>,
    into: &mut Budget,
) -> Vec<RowError> {
    let mut errors = Vec::new();
    let sheet = t.name.clone();
    if !m.columns.contains_key("name") {
        errors.push(RowError {
            sheet,
            row: m.header_row.map_or(1, |r| r + 1),
            message: "Pick the column that holds the names.".into(),
        });
        return errors;
    }
    let mut first_row: BTreeMap<String, usize> = BTreeMap::new();
    let mut uses: Vec<(usize, String, BTreeSet<String>)> = Vec::new();
    for r in m.first_data_row()..t.rows.len() {
        let row = Row {
            t,
            m,
            r,
            decimal_comma: t.decimal_comma,
        };
        if row.blank() {
            continue;
        }
        let at = r + 1;
        let name = row.get("name");
        let result = (|| -> Result<(), String> {
            if name.is_empty() {
                return Err("no name".into());
            }
            if name.contains(',') && !matches!(w, What::Charge | What::Credit) {
                return Err(format!(
                    "\u{201c}{name}\u{201d}: names can't contain commas"
                ));
            }
            if w != What::Credit {
                if let Some(first) = first_row.get(&name) {
                    return Err(format!(
                        "\u{201c}{name}\u{201d} is listed twice (first on row {first})"
                    ));
                }
            }
            let source = row.source()?;
            match w {
                What::Global => {
                    if !is_ident(&name) {
                        return Err(format!(
                            "\u{201c}{name}\u{201d} can't be a global name: use letters, digits and _, with at least one letter or _"
                        ));
                    }
                    let f = row.get("formula");
                    if f.is_empty() {
                        return Err(format!("global {name} needs a value"));
                    }
                    check_formula(&f, &format!("global {name}'s formula"), at, &mut uses)?;
                    into.globals.insert(name.clone(), Formula(f));
                    let info = GlobalInfo {
                        description: row.get("description"),
                        unit: row.get("unit"),
                        ..GlobalInfo::default()
                    };
                    if !info.is_default() {
                        into.global_info.insert(name.clone(), info);
                    }
                }
                What::Fringe => {
                    let kind = row.get("kind").to_ascii_lowercase();
                    let rate = row.get("rate");
                    let rate = rate.trim_end_matches('%').trim().to_string();
                    if rate.is_empty() {
                        return Err(format!("fringe {name} needs a rate"));
                    }
                    check_formula(&rate, &format!("fringe {name}'s rate"), at, &mut uses)?;
                    let cutoff = row.money("cutoff", "the cutoff")?;
                    let unit = row.opt("unit");
                    let basis = row.opt("basis");
                    let max_hours = row.number("max_hours_per_day", "the max hours per day")?;
                    let kind = match kind.as_str() {
                        "" | "percent" | "%" | "pct" | "percentage" => {
                            for (set, what) in [
                                (unit.is_some(), "a unit"),
                                (basis.is_some(), "a basis"),
                                (max_hours.is_some(), "max hours per day"),
                            ] {
                                if set {
                                    return Err(format!("fringe {name}: only a flat fringe has {what}"));
                                }
                            }
                            FringeKind::Percent { cutoff }
                        }
                        "flat" | "amount" | "fixed" => {
                            let basis = match basis.as_deref() {
                                None => FlatBasis::Amount,
                                Some(text) => basis_named(text).ok_or_else(|| format!(
                                    "fringe {name}: the basis should be Amount only, Amount plus counts or Full quantity, not \u{201c}{text}\u{201d}"
                                ))?,
                            };
                            if max_hours.is_some_and(|h| h <= Dec::ZERO) {
                                return Err(format!("fringe {name}: max hours per day should be more than 0"));
                            }
                            // A cutoff is kept as it is, but never used:
                            // cutoffs apply to percent fringes only.
                            FringeKind::Flat {
                                unit,
                                basis,
                                max_hours_per_day: max_hours,
                                cutoff,
                            }
                        }
                        other => return Err(format!("fringe {name}: the kind should be Percent or Flat, not \u{201c}{other}\u{201d}")),
                    };
                    into.fringes.insert(
                        name.clone(),
                        Fringe {
                            description: row.get("description"),
                            short_code: row.opt("short_code"),
                            rate: Formula(rate),
                            kind,
                        },
                    );
                }
                What::Group => {
                    let color = match row.opt("color") {
                        None => None,
                        Some(c) => {
                            let hex = c.strip_prefix('#').unwrap_or("");
                            if hex.len() != 6 || !hex.bytes().all(|x| x.is_ascii_hexdigit()) {
                                return Err(format!("\u{201c}{c}\u{201d} is not a color: use # and 6 hex digits, like #1a2b3c"));
                            }
                            Some(c.to_ascii_lowercase())
                        }
                    };
                    into.groups.insert(
                        name.clone(),
                        Group {
                            description: row.get("description"),
                            short_code: row.opt("short_code"),
                            in_budget_total: row.yes("in_total", "In Total", true)?,
                            color,
                        },
                    );
                }
                What::Unit => {
                    let hours = row.number("hours", "the hours")?;
                    if hours.is_some_and(|h| h < Dec::ZERO) {
                        return Err("the hours can't be negative".into());
                    }
                    into.units.insert(
                        name.clone(),
                        UnitDef {
                            plural: row.get("plural"),
                            description: row.get("description"),
                            hours,
                        },
                    );
                }
                What::Set => {
                    into.sets.insert(
                        name.clone(),
                        SetDef {
                            description: row.get("description"),
                        },
                    );
                }
                What::Location => {
                    into.locations.insert(
                        name.clone(),
                        Location {
                            description: row.get("description"),
                            city: row.get("city"),
                            region: row.get("region"),
                            country: row.get("country"),
                        },
                    );
                }
                What::Currency => {
                    let rate = row
                        .number("rate", "the rate")?
                        .ok_or_else(|| format!("currency {name} needs an exchange rate"))?;
                    if rate <= Dec::ZERO {
                        return Err("an exchange rate must be more than 0".into());
                    }
                    let separators = match row.get("separators").trim() {
                        "" => Separators::default(),
                        s => Separators::parse(s).ok_or_else(|| {
                            format!(
                                "separators should be 1,234.56 or 1.234,56, not \u{201c}{s}\u{201d}"
                            )
                        })?,
                    };
                    into.currencies.insert(
                        name.clone(),
                        Currency {
                            description: row.get("description"),
                            symbol: row.get("symbol"),
                            rate,
                            format: method_core::CurrencyFormat {
                                symbol_after: row.yes("symbol_after", "Symbol After", false)?,
                                separators,
                            },
                        },
                    );
                }
                What::Charge => {
                    let kind = row.get("kind").to_ascii_lowercase();
                    let kind = match kind.as_str() {
                        "" | "percent" | "%" | "pct" | "percentage" => ChargeKind::Percent(
                            row.number("rate", "the rate")?
                                .ok_or_else(|| format!("charge {name} needs a rate"))?,
                        ),
                        "flat" | "amount" | "fixed" => ChargeKind::Flat {
                            amount: row
                                .money("rate", "the amount")?
                                .ok_or_else(|| format!("charge {name} needs an amount"))?,
                            currency: row.opt("currency"),
                        },
                        other => return Err(format!("charge {name}: the kind should be Percent or Flat, not \u{201c}{other}\u{201d}")),
                    };
                    into.charges.push(Charge {
                        name: name.clone(),
                        account: row.opt("account"),
                        applied: row.yes("applied", "Applied", true)?,
                        kind,
                        excluded_amounts: Vec::new(),
                        excluded_groups: row.names("excluded_groups"),
                        excluded_categories: row.names("excluded_categories"),
                        excluded_accounts: row.names("excluded_accounts"),
                        label_percent: false,
                        label_excluded: false,
                    });
                }
                What::Credit => {
                    let has_row = [
                        "row",
                        "group",
                        "category",
                        "row_account",
                        "amount",
                        "percent",
                        "row_cap",
                        "row_cap_on",
                    ]
                    .iter()
                    .any(|k| !row.get(k).is_empty());
                    let i = match into.credits.iter().position(|c| c.name == name) {
                        Some(i) => i,
                        None => {
                            into.credits.push(Credit {
                                name: name.clone(),
                                account: row.opt("account"),
                                applied: row.yes("applied", "Applied", true)?,
                                cap: row.money("cap", "the cap")?,
                                discount_percent: row
                                    .number("discount", "the discount")?
                                    .unwrap_or(Dec::ZERO),
                                excluded_fringes: BTreeSet::new(),
                                locations: BTreeSet::new(),
                                rows: Vec::new(),
                            });
                            into.credits.len() - 1
                        }
                    };
                    // One list per credit: every sheet row of it adds to it.
                    into.credits[i]
                        .excluded_fringes
                        .extend(row.names("excluded_fringes"));
                    into.credits[i].locations.extend(row.names("locations"));
                    if has_row {
                        let group = row.opt("group");
                        let category = row.opt("category");
                        let account = row.opt("row_account");
                        let amount = row.money("amount", "the flat amount")?;
                        let source = match (group, category, account, amount) {
                            (Some(g), None, None, None) => CreditSource::Group { group: g },
                            (None, Some(c), None, None) => CreditSource::Category { category: c },
                            (None, None, Some(a), None) => CreditSource::Account { account: a },
                            (None, None, None, Some(a)) => CreditSource::Flat(a),
                            (None, None, None, None) => {
                                return Err(
                                    "a credit row needs a group, a category, an account or a flat amount"
                                        .into(),
                                )
                            }
                            _ => return Err(
                                "a credit row takes one of a group, a category, an account or a flat amount"
                                    .into(),
                            ),
                        };
                        let percent = row
                            .number("percent", "the percent")?
                            .ok_or("a credit row needs a percent")?;
                        let cap_amount = row.money("row_cap", "the row cap")?;
                        let cap = match (
                            row.get("row_cap_on").to_ascii_lowercase().as_str(),
                            cap_amount,
                        ) {
                            (_, None) => CreditCap::None,
                            ("" | "spend" | "qualified spend" | "qualifiedspend", Some(x)) => {
                                CreditCap::QualifiedSpend(x)
                            }
                            ("credit", Some(x)) => CreditCap::Credit(x),
                            (other, Some(_)) => {
                                return Err(format!(
                                "Row Cap On should be Spend or Credit, not \u{201c}{other}\u{201d}"
                            ))
                            }
                        };
                        into.credits[i].rows.push(CreditRow {
                            description: row.get("row"),
                            source,
                            percent,
                            cap,
                        });
                    }
                }
                What::TagPreset => {
                    if name.chars().count() > method_core::MAX_TOOL_PRESET_NAME_CHARS {
                        return Err(format!(
                            "a tag preset's name can be at most {} characters",
                            method_core::MAX_TOOL_PRESET_NAME_CHARS
                        ));
                    }
                    let key = name.trim().to_lowercase();
                    if into
                        .tag_presets
                        .iter()
                        .any(|p| p.name.trim().to_lowercase() == key)
                    {
                        return Err(format!(
                            "\u{201c}{name}\u{201d} is listed twice (tag preset names ignore case)"
                        ));
                    }
                    into.tag_presets.push(TagPreset {
                        name: name.clone(),
                        fringes: row.names("fringes"),
                        groups: row.names("groups"),
                        set: row.opt("set"),
                        location: row.opt("location"),
                        currency: row.opt("currency"),
                    });
                }
                What::OtPattern => {
                    let p = OtPattern {
                        name: name.clone(),
                        straight_hours: row
                            .number("straight", "the straight hours")?
                            .ok_or("a pattern needs its straight hours")?,
                        bands: ot_bands(&row.get("bands"))?,
                        sixth_day: row.number("sixth", "the 6th day")?,
                        seventh_day: row.number("seventh", "the 7th day")?,
                    };
                    p.validate_all().map_err(|e| e.to_string())?;
                    into.ot_patterns.push(p);
                }
                _ => return Err("that kind of item can't come from a spreadsheet".into()),
            }
            if !source.is_empty() {
                into.setup_sources.set(w.key(), &name, source);
            }
            first_row.entry(name.clone()).or_insert(at);
            Ok(())
        })();
        if let Err(message) = result {
            errors.push(RowError {
                sheet: sheet.clone(),
                row: at,
                message,
            });
        }
    }
    // Formulas may name only globals the sheet or the budget defines.
    for (row, what, vars) in uses {
        for v in vars {
            if !into.globals.contains_key(&v) && !known_globals.contains(&v) {
                errors.push(RowError {
                    sheet: sheet.clone(),
                    row,
                    message: format!(
                        "{what} names {v}, which is not a global here or in the sheet"
                    ),
                });
            }
        }
    }
    errors.sort_by_key(|e| e.row);
    errors
}

/// The budget a sheet import builds before Import Setup's review: setup
/// only, no units of its own, in `base_currency`.
pub fn empty_source(title: &str, base_currency: &str) -> Budget {
    let mut b = Budget::new(title, base_currency);
    b.units.clear();
    b
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::read_file;

    /// A budget with every kind a sheet holds, sources on some.
    fn budget() -> Budget {
        let mut b = method_core::starter::budget();
        b.globals.insert("DAYS".into(), "20".into());
        b.globals.insert("WEEKS".into(), "DAYS / 5".into());
        b.global_info.insert(
            "DAYS".into(),
            GlobalInfo {
                description: "Shoot days".into(),
                unit: "Day".into(),
                ..GlobalInfo::default()
            },
        );
        b.fringes.insert(
            "FICA".into(),
            Fringe {
                description: "Social security, \"OASDI\"".into(),
                short_code: Some("SS".into()),
                rate: "6.2".into(),
                kind: FringeKind::Percent {
                    cutoff: Some(17_610_000),
                },
            },
        );
        b.fringes.insert(
            "KIT".into(),
            Fringe {
                description: String::new(),
                short_code: None,
                rate: "WEEKS * 10".into(),
                kind: FringeKind::flat(Some("Week".into())),
            },
        );
        b.fringes.insert(
            "PENHR".into(),
            Fringe {
                description: "Pension per hour".into(),
                short_code: None,
                rate: "1.25".into(),
                kind: FringeKind::Flat {
                    unit: Some("Hour".into()),
                    basis: FlatBasis::AmountCounts,
                    max_hours_per_day: Some("12.5".parse().unwrap()),
                    cutoff: Some(5_000_000),
                },
            },
        );
        b.fringes.insert(
            "OLD".into(),
            Fringe {
                description: String::new(),
                short_code: None,
                rate: "3".into(),
                kind: FringeKind::Flat {
                    unit: None,
                    basis: FlatBasis::Quantity,
                    max_hours_per_day: None,
                    cutoff: None,
                },
            },
        );
        b.groups.insert(
            "OT".into(),
            Group {
                description: "Overtime".into(),
                short_code: Some("O".into()),
                in_budget_total: false,
                color: Some("#1a2b3c".into()),
            },
        );
        b.sets.insert(
            "Stage 1".into(),
            SetDef {
                description: "Main".into(),
            },
        );
        b.locations.insert(
            "Ranch".into(),
            Location {
                description: "Desert".into(),
                city: "Lancaster".into(),
                region: "CA".into(),
                country: "US".into(),
            },
        );
        b.currencies.insert(
            "EUR".into(),
            Currency {
                description: "Euro".into(),
                symbol: "\u{20ac}".into(),
                rate: "1.08".parse().unwrap(),
                format: method_core::CurrencyFormat {
                    symbol_after: true,
                    separators: Separators::PointComma,
                },
            },
        );
        b.charges.push(Charge {
            name: "Fee, all in".into(),
            account: Some("9900".into()),
            applied: false,
            kind: ChargeKind::Flat {
                amount: 125_050,
                currency: Some("EUR".into()),
            },
            excluded_amounts: Vec::new(),
            excluded_groups: ["OT".to_string()].into(),
            excluded_categories: ["10".to_string()].into(),
            excluded_accounts: BTreeSet::new(),
            label_percent: false,
            label_excluded: false,
        });
        b.credits.push(Credit {
            name: "State".into(),
            account: None,
            applied: true,
            cap: Some(100_000_000),
            discount_percent: Dec::from_int(5),
            excluded_fringes: ["KIT".to_string()].into(),
            locations: ["Stage 4".to_string()].into(),
            rows: vec![
                CreditRow {
                    description: "Labor".into(),
                    source: CreditSource::Group { group: "OT".into() },
                    percent: Dec::from_int(25),
                    cap: CreditCap::Credit(500_000),
                },
                CreditRow {
                    description: "Other".into(),
                    source: CreditSource::Flat(1_000_000),
                    percent: Dec::from_int(20),
                    cap: CreditCap::None,
                },
                CreditRow {
                    description: "Electric".into(),
                    source: CreditSource::Category {
                        category: "10".into(),
                    },
                    percent: Dec::from_int(10),
                    cap: CreditCap::None,
                },
                CreditRow {
                    description: "Writers".into(),
                    source: CreditSource::Account {
                        account: "1001".into(),
                    },
                    percent: Dec::from_int(5),
                    cap: CreditCap::None,
                },
            ],
        });
        b.credits.push(Credit {
            name: "Empty".into(),
            account: Some("9800".into()),
            applied: false,
            cap: None,
            discount_percent: Dec::ZERO,
            excluded_fringes: BTreeSet::new(),
            locations: BTreeSet::new(),
            rows: Vec::new(),
        });
        b.tag_presets.push(TagPreset {
            name: "Crew".into(),
            fringes: ["FICA".to_string(), "KIT".to_string()].into(),
            groups: ["OT".to_string()].into(),
            set: Some("Stage 1".into()),
            location: None,
            currency: Some("EUR".into()),
        });
        b.ot_patterns.push(OtPattern {
            name: "Crew 12s".into(),
            straight_hours: Dec::from_int(8),
            bands: vec![
                OtBand {
                    multiplier: "1.5".parse().unwrap(),
                    hours: Some(Dec::from_int(2)),
                },
                OtBand {
                    multiplier: Dec::from_int(2),
                    hours: None,
                },
            ],
            sixth_day: Some("1.5".parse().unwrap()),
            seventh_day: None,
        });
        b.setup_sources.set(
            "fringe",
            "FICA",
            ItemSource {
                effective: "2026-01-01".into(),
                notes: "Federal rate".into(),
                urls: vec![
                    "https://example.org/a".into(),
                    "https://example.org/b".into(),
                ],
                ..ItemSource::default()
            },
        );
        b
    }

    /// Reads every sheet of `bytes` back into a setup-only budget.
    fn read_back(bytes: &[u8], file: &str) -> Budget {
        let book = read_file(bytes, file).unwrap();
        let mut out = empty_source("Back", "USD");
        for t in &book.tables {
            let w = kind_named(&t.name).or_else(|| kind_named(file)).unwrap();
            let m = guess(t, w);
            let errs = read(t, w, &m, &BTreeSet::new(), &mut out);
            assert!(errs.is_empty(), "{}: {errs:?}", t.name);
        }
        out
    }

    fn same_setup(a: &Budget, b: &Budget) {
        assert_eq!(a.globals, b.globals);
        assert_eq!(a.global_info, b.global_info);
        assert_eq!(a.fringes, b.fringes);
        assert_eq!(a.groups, b.groups);
        assert_eq!(a.units, b.units);
        assert_eq!(a.sets, b.sets);
        assert_eq!(a.locations, b.locations);
        assert_eq!(a.currencies, b.currencies);
        assert_eq!(a.charges, b.charges);
        assert_eq!(a.credits, b.credits);
        assert_eq!(a.tag_presets, b.tag_presets);
        assert_eq!(a.ot_patterns, b.ot_patterns);
        assert_eq!(
            method_core::pack::normalized(a),
            method_core::pack::normalized(b)
        );
    }

    #[test]
    fn every_kind_round_trips_through_a_workbook() {
        let b = budget();
        let wb = workbook(&b, &[]);
        assert_eq!(wb.sheets.len(), SHEET_KINDS.len());
        let bytes = crate::export::xlsx::write(&wb).unwrap();
        let back = read_back(&bytes, "Setup.xlsx");
        same_setup(&b, &back);
    }

    #[test]
    fn each_kind_round_trips_through_csv() {
        let b = budget();
        let mut back = empty_source("Back", "USD");
        for w in SHEET_KINDS {
            let s = sheet(&b, w);
            let bytes = crate::export::csv::write(&s, crate::export::csv::Delimiter::Comma, true);
            let file = format!("Show - {}.csv", sheet_name(w));
            let book = read_file(&bytes, &file).unwrap();
            let t = &book.tables[0];
            assert_eq!(kind_named(&file), Some(w), "{file}");
            let errs = read(t, w, &guess(t, w), &BTreeSet::new(), &mut back);
            assert!(errs.is_empty(), "{w:?}: {errs:?}");
        }
        same_setup(&b, &back);
    }

    fn csv_table(text: &str) -> Table {
        read_file(text.as_bytes(), "x.csv")
            .unwrap()
            .tables
            .remove(0)
    }

    #[test]
    fn problems_are_listed_with_their_rows() {
        let t = csv_table(
            "Fringe,Rate,Kind,Cutoff\n\
             FICA,6.2,Percent,176100\n\
             ,7,Percent,\n\
             FICA,7,,\n\
             BAD,7 +,,\n\
             MISSING,NOPE * 2,,\n\
             FLATCUT,10,Flat,5000\n\
             CHEAP,abc,,\n\
             ODD,1,Sometimes,\n\
             \n\
             LATE,1,,lots\n",
        );
        let mut into = empty_source("x", "USD");
        let m = guess(&t, What::Fringe);
        assert_eq!(m.header_row, Some(0));
        let errs = read(&t, What::Fringe, &m, &BTreeSet::new(), &mut into);
        let rows: Vec<usize> = errs.iter().map(|e| e.row).collect();
        assert_eq!(rows, [3, 4, 5, 6, 8, 9, 11], "{errs:?}");
        assert!(errs[0].message.contains("no name"));
        assert!(
            errs[1].message.contains("listed twice (first on row 2)"),
            "{}",
            errs[1].message
        );
        assert!(errs[2].message.contains("isn't a formula"));
        assert!(errs[3].message.contains("NOPE"), "{}", errs[3].message);
        // A cutoff on a flat fringe is kept (and never used), not refused.
        assert!(errs[4].message.contains("abc"), "{}", errs[4].message);
        assert!(errs[5].message.contains("Percent or Flat"));
        assert!(errs[6].message.contains("isn't a number"));
        // A global the budget has is fine.
        let mut into = empty_source("x", "USD");
        let t = csv_table("Name,Rate\nX,NOPE * 2\n");
        let known: BTreeSet<String> = ["NOPE".to_string()].into();
        assert!(read(
            &t,
            What::Fringe,
            &guess(&t, What::Fringe),
            &known,
            &mut into
        )
        .is_empty());

        // Globals: names must be names, formulas must parse.
        let t = csv_table("Name,Formula,Effective,Source Links\nGOOD,1,,\n2.5BAD,1,,\nX,1,someday,\nY,1,,ftp://nope\nZ,,,\n");
        let mut into = empty_source("x", "USD");
        let errs = read(
            &t,
            What::Global,
            &guess(&t, What::Global),
            &BTreeSet::new(),
            &mut into,
        );
        let rows: Vec<usize> = errs.iter().map(|e| e.row).collect();
        assert_eq!(rows, [3, 4, 5, 6], "{errs:?}");
        assert!(errs[1].message.contains("YYYY-MM-DD"));
        assert!(errs[2].message.contains("https://"));

        // No name column at all.
        let t = csv_table("Rate,Kind\n1,Flat\n");
        let mut m = guess(&t, What::Fringe);
        m.columns.remove("name");
        let errs = read(
            &t,
            What::Fringe,
            &m,
            &BTreeSet::new(),
            &mut empty_source("x", "USD"),
        );
        assert_eq!(errs.len(), 1);
    }

    #[test]
    fn flat_fringe_basis_and_max_hours_columns() {
        let t = csv_table(
            "Name,Kind,Rate,Unit,Basis,Max Hours Per Day,Cutoff\n\
             STD,Flat,100,Day,,,\n\
             LABEL,Flat,100,Day,\"Amount, plus counts in X and 4X\",,\n\
             FULL,Flat,1,Hour,Full quantity,12,\n\
             CUT,Flat,1,Hour,amount_counts,10.5,1000\n\
             BADB,Flat,1,Hour,sometimes,,\n\
             ZERO,Flat,1,Hour,,0,\n\
             PCTB,Percent,7,,Full quantity,,\n\
             PCTH,Percent,7,,,12,\n",
        );
        let mut into = empty_source("x", "USD");
        let errs = read(
            &t,
            What::Fringe,
            &guess(&t, What::Fringe),
            &BTreeSet::new(),
            &mut into,
        );
        let rows: Vec<usize> = errs.iter().map(|e| e.row).collect();
        assert_eq!(rows, [6, 7, 8, 9], "{errs:?}");
        assert!(errs[0].message.contains("the basis should be"), "{errs:?}");
        assert!(errs[1].message.contains("more than 0"), "{errs:?}");
        assert!(
            errs[2].message.contains("only a flat fringe has a basis"),
            "{errs:?}"
        );
        assert!(
            errs[3].message.contains("only a flat fringe has max hours"),
            "{errs:?}"
        );
        let kind = |n: &str| into.fringes[n].kind.clone();
        assert_eq!(kind("STD"), FringeKind::flat(Some("Day".into())));
        assert!(matches!(
            kind("LABEL"),
            FringeKind::Flat {
                basis: FlatBasis::AmountCounts,
                ..
            }
        ));
        assert_eq!(
            kind("FULL"),
            FringeKind::Flat {
                unit: Some("Hour".into()),
                basis: FlatBasis::Quantity,
                max_hours_per_day: Some(Dec::from_int(12)),
                cutoff: None,
            }
        );
        assert_eq!(
            kind("CUT"),
            FringeKind::Flat {
                unit: Some("Hour".into()),
                basis: FlatBasis::AmountCounts,
                max_hours_per_day: Some("10.5".parse().unwrap()),
                cutoff: Some(100_000),
            }
        );
        // Every label the application shows reads back as its basis.
        for b in FlatBasis::ALL {
            assert_eq!(basis_named(b.label()), Some(b));
            assert_eq!(basis_named(b.name()), Some(b));
        }
    }

    #[test]
    fn a_department_sheet_maps_by_its_own_titles() {
        let t = csv_table(
            "Union rates 2026,,\n\
             ,,\n\
             Fringe Name,Percentage,Wage Cap,Notes\n\
             PENSION,8.5%,,From the scale sheet\n",
        );
        let m = guess(&t, What::Fringe);
        assert_eq!(m.header_row, Some(2));
        assert_eq!(m.columns.get("rate"), Some(&1));
        assert_eq!(m.columns.get("cutoff"), Some(&2));
        assert_eq!(m.columns.get("source_notes"), Some(&3));
        let mut into = empty_source("x", "USD");
        assert!(read(&t, What::Fringe, &m, &BTreeSet::new(), &mut into).is_empty());
        assert_eq!(into.fringes["PENSION"].rate.0, "8.5");
        assert_eq!(
            into.setup_sources.get("fringe", "PENSION").unwrap().notes,
            "From the scale sheet"
        );
        assert_eq!(kind_named("Fringes"), Some(What::Fringe));
        assert_eq!(kind_named("Tag Presets"), Some(What::TagPreset));
        assert_eq!(kind_named("whatever"), None);
    }
}
