//! Rate tables the user keeps (roadmap 3.9; decisions.md, entry 90).
//!
//! A rate table is a named list of classifications ("Grip, Key"), each with
//! a rate per unit (Hour, Day, Week: the budget's own unit names) and, when
//! the figures change over time, the dates each row is in effect. Tables
//! are setup tools kept in the budget (`rate_tables`), like overtime
//! patterns: versions don't carry them and restoring a version keeps them.
//! They come and go through Import Setup, the setup library, setup packs and
//! setup sheets like any other setup item. Nothing about any union, guild
//! or agreement is built in: every figure is the user's.
//!
//! Picking a classification on a line ([`fill`]) writes the figure into the
//! line's rate once, as a plain number, and records where it came from in
//! the line's [`RateSource`]. The rate never follows the table on its own:
//! [`check`] compares the line with the table as it is now, and an update
//! ([`update`]) is always a separate, explicit edit.
//!
//! Units: a table's figure for the line's unit is used as it is. When the
//! table has no figure for that unit, it is converted from the figure whose
//! unit is closest in hours, only when both units have hours in this
//! budget, and rounded to cents (a 50.00 Hour rate is 600.00 for a 12-hour
//! Day).

use std::collections::BTreeMap;
use std::fmt;

use crate::actuals::is_iso_date;
use crate::decimal::Dec;
use crate::model::{person_key, Budget, Detail, RowId, RowKind, UnitDef};

/// Most rate tables a budget keeps.
pub const MAX_RATE_TABLES: usize = 200;
/// Most rows a table has.
pub const MAX_RATE_ROWS: usize = 5_000;
/// Most units a row has a figure for.
pub const MAX_ROW_UNITS: usize = 12;
/// Longest table name, in characters.
pub const MAX_TABLE_NAME_CHARS: usize = 80;
/// Longest classification, in characters.
pub const MAX_CLASSIFICATION_CHARS: usize = 120;
/// Longest unit name, in characters.
pub const MAX_UNIT_NAME_CHARS: usize = 40;
/// Longest notes on a table or a row, in characters.
pub const MAX_RATE_NOTES_CHARS: usize = 2_000;

/// A named table of classifications and their rates.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RateTable {
    /// 1 to [`MAX_TABLE_NAME_CHARS`] characters, no spaces at either end.
    /// Unique within the budget.
    pub name: String,
    /// Where the figures came from, what they cover: the user's words.
    pub notes: String,
    /// In the user's order.
    pub rows: Vec<RateRow>,
}

/// One classification's rates for one period.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RateRow {
    /// As the user wrote it ("Grip, Key"). Matched ignoring case and extra
    /// spaces.
    pub classification: String,
    /// Unit name to the rate per one of it. At least one.
    pub rates: BTreeMap<String, Dec>,
    /// First day in effect, `YYYY-MM-DD`; `None` for open.
    pub from: Option<String>,
    /// Last day in effect, `YYYY-MM-DD`; `None` for open.
    pub to: Option<String>,
    pub notes: String,
}

/// Where a line's rate came from: what was asked of which table, and the
/// figure written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RateSource {
    pub table: String,
    /// As the table spelled it when the rate was filled.
    pub classification: String,
    /// The line's unit the rate is for.
    pub unit: String,
    /// The date the row was picked for (`YYYY-MM-DD`), when the
    /// classification has more than one period.
    pub as_of: Option<String>,
    /// The figure written into the line.
    pub rate: Dec,
}

/// Why a table can't be kept or a rate can't be filled. The message says
/// what to do, in the app's words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RateError(pub String);

impl fmt::Display for RateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for RateError {}

fn err<T>(m: impl Into<String>) -> Result<T, RateError> {
    Err(RateError(m.into()))
}

/// The key two classifications match on: trimmed, inner spaces collapsed,
/// case ignored.
pub fn classification_key(s: &str) -> String {
    person_key(s)
}

impl RateRow {
    /// Whether the row is in effect on `date` (`YYYY-MM-DD`).
    pub fn covers(&self, date: &str) -> bool {
        self.from.as_deref().is_none_or(|f| f <= date)
            && self.to.as_deref().is_none_or(|t| date <= t)
    }

    /// The period in words: "", "from 2026-01-01", "2026-01-01 to 2026-06-30".
    pub fn period(&self) -> String {
        match (&self.from, &self.to) {
            (None, None) => String::new(),
            (Some(f), None) => format!("from {f}"),
            (None, Some(t)) => format!("until {t}"),
            (Some(f), Some(t)) => format!("{f} to {t}"),
        }
    }

    /// A date inside the period, to pick this row again: its first day, or
    /// its last when it has no first.
    pub fn pick_date(&self) -> Option<&str> {
        self.from.as_deref().or(self.to.as_deref())
    }

    fn overlaps(&self, o: &RateRow) -> bool {
        let starts_before_other_ends = match (&self.from, &o.to) {
            (Some(f), Some(t)) => f <= t,
            _ => true,
        };
        let other_starts_before_this_ends = match (&o.from, &self.to) {
            (Some(f), Some(t)) => f <= t,
            _ => true,
        };
        starts_before_other_ends && other_starts_before_this_ends
    }
}

fn check_text(what: &str, s: &str, max: usize) -> Result<(), RateError> {
    if s.chars().count() > max {
        return err(format!("The {what} can be at most {max} characters."));
    }
    Ok(())
}

/// A table name as kept: trimmed, 1 to [`MAX_TABLE_NAME_CHARS`] characters.
pub fn valid_table_name(name: &str) -> Result<String, RateError> {
    let t = name.trim();
    if t.is_empty() {
        return err("A rate table needs a name.");
    }
    check_text("table name", t, MAX_TABLE_NAME_CHARS)?;
    if t.chars().any(char::is_control) {
        return err("A rate table's name must be one line.");
    }
    Ok(t.to_string())
}

impl RateTable {
    /// Checks the table as a file or an edit gives it: a name, rows with a
    /// classification and at least one rate each, rates not negative,
    /// periods as dates in order, and no two rows of one classification in
    /// effect on the same day (a row with no dates is in effect every day).
    pub fn validate(&self) -> Result<(), RateError> {
        if valid_table_name(&self.name)? != self.name {
            return err("A rate table's name can't start or end with a space.");
        }
        check_text("table notes", &self.notes, MAX_RATE_NOTES_CHARS)?;
        if self.rows.len() > MAX_RATE_ROWS {
            return err(format!(
                "A rate table can have at most {MAX_RATE_ROWS} rows."
            ));
        }
        let mut seen: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (i, r) in self.rows.iter().enumerate() {
            let n = i + 1;
            let c = r.classification.trim();
            if c.is_empty() {
                return err(format!("Row {n} needs a classification."));
            }
            if c != r.classification {
                return err(format!(
                    "Row {n}: a classification can't start or end with a space."
                ));
            }
            check_text("classification", c, MAX_CLASSIFICATION_CHARS)?;
            if c.chars().any(char::is_control) {
                return err(format!("Row {n}: a classification must be one line."));
            }
            if r.rates.is_empty() {
                return err(format!("Row {n} ({c}) needs a rate for at least one unit."));
            }
            if r.rates.len() > MAX_ROW_UNITS {
                return err(format!(
                    "Row {n} ({c}) can have rates for at most {MAX_ROW_UNITS} units."
                ));
            }
            for (u, v) in &r.rates {
                if u.trim().is_empty() || u.trim() != u {
                    return err(format!(
                        "Row {n} ({c}): a unit needs a name with no spaces at either end."
                    ));
                }
                check_text("unit name", u, MAX_UNIT_NAME_CHARS)?;
                if *v < Dec::ZERO {
                    return err(format!(
                        "Row {n} ({c}): the rate per {u} can't be negative."
                    ));
                }
            }
            for d in [&r.from, &r.to].into_iter().flatten() {
                if !is_iso_date(d) {
                    return err(format!(
                        "Row {n} ({c}): \u{201c}{d}\u{201d} is not a date written YYYY-MM-DD."
                    ));
                }
            }
            if let (Some(f), Some(t)) = (&r.from, &r.to) {
                if f > t {
                    return err(format!("Row {n} ({c}) ends before it starts."));
                }
            }
            check_text("row notes", &r.notes, MAX_RATE_NOTES_CHARS)?;
            let key = classification_key(c);
            for &j in seen.get(&key).into_iter().flatten() {
                if self.rows[j].overlaps(r) {
                    return err(format!(
                        "Rows {} and {n} are both {c} on the same days: give each its own dates, with no days in common.",
                        j + 1
                    ));
                }
            }
            seen.entry(key).or_default().push(i);
        }
        Ok(())
    }

    /// The rows of classification `c` (matched ignoring case and spaces).
    pub fn rows_of<'a>(&'a self, c: &str) -> impl Iterator<Item = &'a RateRow> + 'a {
        let key = classification_key(c);
        self.rows
            .iter()
            .filter(move |r| classification_key(&r.classification) == key)
    }

    /// Every unit any row has a figure for, in name order.
    pub fn units(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .rows
            .iter()
            .flat_map(|r| r.rates.keys().cloned())
            .collect();
        out.sort();
        out.dedup();
        out
    }
}

// ---------------------------------------------------------------------------
// Keeping tables

/// Saves `t` under its name: a new table at the end, or (with `replacing`)
/// in place of the table of that name, which may rename it; lines filled
/// from the old name then follow the new one.
pub fn save_table(b: &mut Budget, t: RateTable, replacing: Option<&str>) -> Result<(), RateError> {
    t.validate()?;
    let clash = b.rate_tables.iter().position(|x| x.name == t.name);
    match replacing {
        Some(old) => {
            let i = b
                .rate_tables
                .iter()
                .position(|x| x.name == old)
                .ok_or_else(|| {
                    RateError(format!("There is no rate table {old} in this budget."))
                })?;
            if clash.is_some_and(|c| c != i) {
                return err(format!("There is already a rate table named {}.", t.name));
            }
            if old != t.name {
                let new = t.name.clone();
                for d in details_mut(b) {
                    if let Some(s) = &mut d.rate_source {
                        if s.table == old {
                            s.table = new.clone();
                        }
                    }
                }
            }
            b.rate_tables[i] = t;
        }
        None => {
            if clash.is_some() {
                return err(format!("There is already a rate table named {}.", t.name));
            }
            if b.rate_tables.len() >= MAX_RATE_TABLES {
                return err(format!(
                    "A budget can keep at most {MAX_RATE_TABLES} rate tables."
                ));
            }
            b.rate_tables.push(t);
        }
    }
    Ok(())
}

/// Removes the table named `name`. Lines filled from it keep their figures
/// and their source (which then says the table is gone).
pub fn delete_table(b: &mut Budget, name: &str) -> Result<RateTable, RateError> {
    let i = b
        .rate_tables
        .iter()
        .position(|x| x.name == name)
        .ok_or_else(|| RateError(format!("There is no rate table {name} in this budget.")))?;
    Ok(b.rate_tables.remove(i))
}

/// The table named `name`.
pub fn table<'a>(b: &'a Budget, name: &str) -> Option<&'a RateTable> {
    b.rate_tables.iter().find(|t| t.name == name)
}

/// After unit `old` is renamed `new`: the tables' figures and the lines'
/// sources follow it.
pub fn rename_unit(b: &mut Budget, old: &str, new: &str) {
    for t in &mut b.rate_tables {
        for r in &mut t.rows {
            if let Some(v) = r.rates.remove(old) {
                r.rates.insert(new.to_string(), v);
            }
        }
    }
    for d in details_mut(b) {
        if let Some(s) = &mut d.rate_source {
            if s.unit == old {
                s.unit = new.to_string();
            }
        }
    }
}

fn details_mut(b: &mut Budget) -> impl Iterator<Item = &mut Detail> {
    b.categories_mut()
        .flat_map(|c| &mut c.accounts)
        .flat_map(|a| &mut a.details)
}

// ---------------------------------------------------------------------------
// Looking up a rate

/// A rate found in a table for one unit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    /// The classification as the table spells it.
    pub classification: String,
    /// The row's period in words ("" when it has no dates).
    pub period: String,
    /// The unit the table's figure is per.
    pub table_unit: String,
    /// The table's figure.
    pub table_rate: Dec,
    /// The rate for the unit asked for (the table's figure, or converted
    /// from it through unit hours and rounded to cents).
    pub rate: Dec,
    /// `(hours in the table's unit, hours in the unit asked for)` when
    /// converted.
    pub converted: Option<(Dec, Dec)>,
}

impl Found {
    /// How the figure was reached, in words: "50 per Hour x 12 hours a Day
    /// / 1 hour an Hour = 600".
    pub fn conversion_words(&self, unit: &str) -> Option<String> {
        self.converted.map(|(from_h, to_h)| {
            format!(
                "converted from {} per {}: {} x {to_h} hours a {unit} / {from_h} hours a {} = {}",
                self.table_rate, self.table_unit, self.table_rate, self.table_unit, self.rate
            )
        })
    }
}

/// The row of classification `c` in table `t` in effect on `as_of` (or the
/// only one, when no date is given).
pub fn pick_row<'a>(
    t: &'a RateTable,
    c: &str,
    as_of: Option<&str>,
) -> Result<&'a RateRow, RateError> {
    let rows: Vec<&RateRow> = t.rows_of(c).collect();
    if rows.is_empty() {
        return err(format!(
            "The rate table {} has no classification {}.",
            t.name,
            c.trim()
        ));
    }
    match as_of {
        Some(d) => {
            if !is_iso_date(d) {
                return err(format!(
                    "\u{201c}{d}\u{201d} is not a date written YYYY-MM-DD."
                ));
            }
            let mut on = rows.iter().filter(|r| r.covers(d));
            match (on.next(), on.next()) {
                (Some(r), None) => Ok(r),
                (None, _) => err(format!(
                    "The rate table {} has no rate for {} in effect on {d} ({}).",
                    t.name,
                    rows[0].classification,
                    periods(&rows)
                )),
                (Some(_), Some(_)) => err(format!(
                    "The rate table {} has more than one row for {} on {d}.",
                    t.name, rows[0].classification
                )),
            }
        }
        None => match rows.as_slice() {
            [one] => Ok(one),
            _ => err(format!(
                "{} has {} periods in the rate table {} ({}): give the date the rate is for.",
                rows[0].classification,
                rows.len(),
                t.name,
                periods(&rows)
            )),
        },
    }
}

fn periods(rows: &[&RateRow]) -> String {
    rows.iter()
        .map(|r| match r.period() {
            p if p.is_empty() => "no dates".to_string(),
            p => p,
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// How far apart two hour counts are (`None` only on overflow, sorted last).
fn distance(a: Dec, b: Dec) -> (bool, Dec) {
    let d = if a > b {
        a.checked_sub(b)
    } else {
        b.checked_sub(a)
    };
    (d.is_none(), d.unwrap_or(Dec::ZERO))
}

fn hours(units: &BTreeMap<String, UnitDef>, u: &str) -> Option<Dec> {
    units
        .get(u)
        .and_then(|d| d.hours)
        .filter(|h| *h > Dec::ZERO)
}

/// The rate for `unit` from row `r`, with `units` (the budget's) for
/// conversions.
pub fn rate_for(
    r: &RateRow,
    unit: &str,
    units: &BTreeMap<String, UnitDef>,
    table: &str,
) -> Result<Found, RateError> {
    let found = |table_unit: &str, table_rate: Dec, rate: Dec, converted| Found {
        classification: r.classification.clone(),
        period: r.period(),
        table_unit: table_unit.to_string(),
        table_rate,
        rate,
        converted,
    };
    if let Some(v) = r.rates.get(unit) {
        return Ok(found(unit, *v, *v, None));
    }
    let has = r.rates.keys().cloned().collect::<Vec<_>>().join(", ");
    let Some(to_h) = hours(units, unit) else {
        return err(format!(
            "{} has no rate per {unit} in the rate table {table} (it has {has}), and {unit} has no hours in this budget to convert with.",
            r.classification
        ));
    };
    // The figure whose unit is closest in hours; the smaller one on a tie.
    let best = r
        .rates
        .iter()
        .filter_map(|(u, v)| hours(units, u).map(|h| (u, *v, h)))
        .min_by(|a, b| {
            distance(a.2, to_h)
                .cmp(&distance(b.2, to_h))
                .then(a.2.cmp(&b.2))
        });
    let Some((u, v, from_h)) = best else {
        return err(format!(
            "{} has no rate per {unit} in the rate table {table} (it has {has}), and none of those units has hours in this budget to convert from.",
            r.classification
        ));
    };
    let exact = v
        .checked_mul(to_h)
        .and_then(|x| x.checked_div(from_h))
        .ok_or_else(|| RateError("That rate is too large to convert.".into()))?;
    let cents = exact
        .to_cents()
        .ok_or_else(|| RateError("That rate is too large to convert.".into()))?;
    Ok(found(u, v, Dec::from_cents(cents), Some((from_h, to_h))))
}

/// The rate table `table` gives classification `c` per `unit` on `as_of`,
/// in budget `b`.
pub fn lookup(
    b: &Budget,
    table: &str,
    c: &str,
    unit: &str,
    as_of: Option<&str>,
) -> Result<Found, RateError> {
    let t = self::table(b, table)
        .ok_or_else(|| RateError(format!("There is no rate table {table} in this budget.")))?;
    let r = pick_row(t, c, as_of)?;
    rate_for(r, unit, &b.units, &t.name)
}

// ---------------------------------------------------------------------------
// Filling a line

/// What a fill wrote.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Filled {
    pub found: Found,
    pub unit: String,
    /// The line's rate before, as typed.
    pub before: String,
}

fn find_detail(b: &mut Budget, row: RowId) -> Result<&mut Detail, RateError> {
    details_mut(b)
        .find(|d| d.id == row)
        .ok_or_else(|| RateError("That line is no longer in the budget.".into()))
}

/// The unit a fill uses: the one asked for, else the line's rate unit, else
/// its amount unit, else the row's only unit.
fn unit_for(
    d: &Detail,
    asked: Option<&str>,
    t: &RateTable,
    c: &str,
    as_of: Option<&str>,
) -> Result<String, RateError> {
    if let Some(u) = asked.map(str::trim).filter(|u| !u.is_empty()) {
        return Ok(u.to_string());
    }
    for u in [&d.unit3, &d.unit] {
        if !u.trim().is_empty() {
            return Ok(u.trim().to_string());
        }
    }
    let r = pick_row(t, c, as_of)?;
    match r.rates.keys().collect::<Vec<_>>().as_slice() {
        [one] => Ok((*one).clone()),
        many => err(format!(
            "The line has no unit: pick one of {} for the rate.",
            many.iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// Fills line `row`'s rate from classification `c` of table `table` (per
/// `unit`, on `as_of`): the figure as a plain number, the unit as the
/// line's rate unit, and where it came from. A pure budget edit; the
/// caller makes it one undo step.
pub fn fill(
    b: &mut Budget,
    row: RowId,
    table: &str,
    c: &str,
    unit: Option<&str>,
    as_of: Option<&str>,
) -> Result<Filled, RateError> {
    let t = self::table(b, table)
        .ok_or_else(|| RateError(format!("There is no rate table {table} in this budget.")))?
        .clone();
    let as_of = as_of.map(str::trim).filter(|s| !s.is_empty());
    let d = details_mut(b)
        .find(|d| d.id == row)
        .ok_or_else(|| RateError("That line is no longer in the budget.".into()))?;
    if d.kind != RowKind::Line {
        return err("Only a line has a rate to fill.");
    }
    let unit = unit_for(d, unit, &t, c, as_of)?;
    let units = b.units.clone();
    let r = pick_row(&t, c, as_of)?;
    let found = rate_for(r, &unit, &units, &t.name)?;
    let d = find_detail(b, row)?;
    let before = d.rate.0.clone();
    d.rate = found.rate.into();
    d.unit3 = unit.clone();
    d.rate_source = Some(RateSource {
        table: t.name.clone(),
        classification: r.classification.clone(),
        unit: unit.clone(),
        as_of: as_of.map(str::to_string),
        rate: found.rate,
    });
    Ok(Filled {
        found,
        unit,
        before,
    })
}

/// How a line filled from a table stands against the table now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RateCheck {
    /// The line has the table's figure.
    Current,
    /// The table now says something else; the line still has the figure it
    /// was filled with. An update is offered, never made on its own.
    Changed { now: Dec },
    /// The line's rate or rate unit was changed by hand since.
    Edited,
    /// The table no longer gives a rate for it (gone, renamed elsewhere, no
    /// such classification or unit, or no row on that date).
    Missing(String),
}

impl RateCheck {
    /// The key the page and the command line use.
    pub fn key(&self) -> &'static str {
        match self {
            RateCheck::Current => "current",
            RateCheck::Changed { .. } => "changed",
            RateCheck::Edited => "edited",
            RateCheck::Missing(_) => "missing",
        }
    }
}

/// Whether line `d`'s rate is still the figure it was filled with.
fn still_filled(d: &Detail, s: &RateSource) -> bool {
    let rate_ok = d.rate.0.trim().parse::<Dec>().is_ok_and(|v| v == s.rate);
    let unit = if d.unit3.trim().is_empty() {
        d.unit.trim()
    } else {
        d.unit3.trim()
    };
    rate_ok && unit == s.unit
}

/// How line `d` stands against the table it was filled from; `None` when
/// it wasn't filled from one.
pub fn check(b: &Budget, d: &Detail) -> Option<RateCheck> {
    let s = d.rate_source.as_ref()?;
    if !still_filled(d, s) {
        return Some(RateCheck::Edited);
    }
    Some(
        match lookup(b, &s.table, &s.classification, &s.unit, s.as_of.as_deref()) {
            Err(e) => RateCheck::Missing(e.0),
            Ok(f) if f.rate != s.rate => RateCheck::Changed { now: f.rate },
            Ok(_) => RateCheck::Current,
        },
    )
}

/// Where line `d`'s rate came from and how it stands, in words, for
/// Explain; `None` when it wasn't filled from a table.
pub fn source_words(b: &Budget, d: &Detail) -> Option<String> {
    let s = d.rate_source.as_ref()?;
    let on = s
        .as_of
        .as_deref()
        .map(|a| format!(" in effect on {a}"))
        .unwrap_or_default();
    let mut out = format!(
        "filled from the rate table {}, {}{on}, at {} per {}",
        s.table, s.classification, s.rate, s.unit
    );
    if let Ok(f) = lookup(b, &s.table, &s.classification, &s.unit, s.as_of.as_deref()) {
        if f.rate == s.rate {
            if let Some(w) = f.conversion_words(&s.unit) {
                out.push_str(&format!(" ({w})"));
            }
        }
    }
    match check(b, d)? {
        RateCheck::Current => {}
        RateCheck::Changed { now } => out.push_str(&format!(
            "; the table now says {now}, and the line keeps {} until it is updated",
            s.rate
        )),
        RateCheck::Edited => out.push_str("; the rate has been changed by hand since"),
        RateCheck::Missing(m) => out.push_str(&format!("; now: {m}")),
    }
    Some(out)
}

/// Fills line `row` again from its source, when the table now says
/// something else. Returns the new figure; refused when the line wasn't
/// filled from a table, was changed by hand, or its table no longer gives
/// a rate.
pub fn update(b: &mut Budget, row: RowId) -> Result<Dec, RateError> {
    let d = find_detail(b, row)?.clone();
    let s = d
        .rate_source
        .clone()
        .ok_or_else(|| RateError("That line's rate wasn't filled from a rate table.".into()))?;
    match check(b, &d) {
        Some(RateCheck::Changed { .. }) => {}
        Some(RateCheck::Current) => return Ok(s.rate),
        Some(RateCheck::Edited) => {
            return err("That line's rate was changed by hand since it was filled: pick a classification again to fill it.")
        }
        Some(RateCheck::Missing(m)) => return Err(RateError(m)),
        None => return err("That line's rate wasn't filled from a rate table."),
    }
    let f = fill(
        b,
        row,
        &s.table,
        &s.classification,
        Some(&s.unit),
        s.as_of.as_deref(),
    )?;
    Ok(f.found.rate)
}

/// Every line filled from a table whose table now says something else.
pub fn changed_lines(b: &Budget) -> Vec<(RowId, Dec, Dec)> {
    b.categories()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.details)
        .filter_map(|d| match check(b, d)? {
            RateCheck::Changed { now } => Some((d.id, d.rate_source.as_ref()?.rate, now)),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Account, Category, TopsheetRow};

    fn d(s: &str) -> Dec {
        s.parse().unwrap()
    }

    fn row(c: &str, rates: &[(&str, &str)], from: Option<&str>, to: Option<&str>) -> RateRow {
        RateRow {
            classification: c.into(),
            rates: rates.iter().map(|(u, v)| (u.to_string(), d(v))).collect(),
            from: from.map(String::from),
            to: to.map(String::from),
            notes: String::new(),
        }
    }

    /// An invented table: made-up classifications and figures.
    fn crew() -> RateTable {
        RateTable {
            name: "Invented Crew".into(),
            notes: "Made-up figures for tests.".into(),
            rows: vec![
                row("Grip, Key", &[("Hour", "50"), ("Week", "3000")], None, None),
                row("Grip, Best Boy", &[("Day", "410.5")], None, None),
                row(
                    "Gaffer",
                    &[("Hour", "40")],
                    Some("2030-01-01"),
                    Some("2030-06-30"),
                ),
                row("Gaffer", &[("Hour", "42")], Some("2030-07-01"), None),
            ],
        }
    }

    fn budget() -> Budget {
        let mut b = Budget::new("Test", "USD");
        b.topsheet.push(TopsheetRow::Category(Category {
            code: "10".into(),
            name: "Crew".into(),
            accounts: vec![Account {
                code: "1000".into(),
                name: "Grip".into(),
                details: vec![
                    Detail::line(1, "Key grip", "10", "Day", ""),
                    Detail::line(2, "Best boy", "10", "", ""),
                    Detail::subtotal(3, "Sub"),
                ],
                notes: Vec::new(),
            }],
            notes: Vec::new(),
        }));
        b.rate_tables.push(crew());
        b
    }

    fn line(b: &Budget, id: RowId) -> &Detail {
        b.categories()
            .flat_map(|c| &c.accounts)
            .flat_map(|a| &a.details)
            .find(|x| x.id == id)
            .unwrap()
    }

    #[test]
    fn a_table_is_checked() {
        crew().validate().unwrap();
        let mut t = crew();
        t.name = " Padded".into();
        assert!(t.validate().is_err());
        let mut t = crew();
        t.rows[0].rates.clear();
        assert!(t.validate().unwrap_err().0.contains("at least one unit"));
        let mut t = crew();
        t.rows[0].rates.insert("Day".into(), d("-1"));
        assert!(t.validate().unwrap_err().0.contains("negative"));
        let mut t = crew();
        t.rows[2].from = Some("2030-13-01".into());
        assert!(t.validate().unwrap_err().0.contains("YYYY-MM-DD"));
        let mut t = crew();
        t.rows[3].from = Some("2030-06-30".into());
        assert!(
            t.validate().unwrap_err().0.contains("same days"),
            "overlap by one day"
        );
        let mut t = crew();
        t.rows
            .push(row("grip,  KEY", &[("Day", "1")], Some("2030-01-01"), None));
        assert!(
            t.validate().unwrap_err().0.contains("same days"),
            "a row with no dates covers every day"
        );
        let mut t = crew();
        t.rows[2].to = Some("2029-01-01".into());
        assert!(t.validate().unwrap_err().0.contains("ends before"));
    }

    #[test]
    fn filling_writes_the_figure_once_with_its_source() {
        let mut b = budget();
        let f = fill(&mut b, 2, "Invented Crew", "grip, best boy", None, None).unwrap();
        assert_eq!(f.unit, "Day", "the row's only unit when the line has none");
        let l = line(&b, 2);
        assert_eq!(l.rate.0, "410.5");
        assert_eq!(l.unit3, "Day");
        let s = l.rate_source.as_ref().unwrap();
        assert_eq!(s.classification, "Grip, Best Boy", "as the table spells it");
        assert_eq!(check(&b, line(&b, 2)), Some(RateCheck::Current));
        // A line with a unit uses it, converting where both units have hours.
        let f = fill(&mut b, 1, "Invented Crew", "Grip, Key", None, None).unwrap();
        assert_eq!(
            f.found.table_unit, "Hour",
            "12 hours is closer to 1 than to 60"
        );
        assert_eq!(line(&b, 1).rate.0, "600");
        assert!(fill(
            &mut b,
            3,
            "Invented Crew",
            "Gaffer",
            None,
            Some("2030-02-01")
        )
        .is_err());
        assert!(fill(&mut b, 9, "Invented Crew", "Gaffer", None, None).is_err());
        assert!(fill(&mut b, 1, "Nope", "Gaffer", None, None).is_err());
    }

    #[test]
    fn conversion_needs_hours_on_both_units() {
        let mut b = budget();
        // Week to Day: 3000 x 12 / 60 when Hour is gone.
        b.rate_tables[0].rows[0].rates.remove("Hour");
        let f = lookup(&b, "Invented Crew", "Grip, Key", "Day", None).unwrap();
        assert_eq!(f.rate, d("600"));
        assert_eq!(f.converted, Some((d("60"), d("12"))));
        // Rounded to cents.
        b.rate_tables[0].rows[0]
            .rates
            .insert("Week".into(), d("1000"));
        assert_eq!(
            lookup(&b, "Invented Crew", "Grip, Key", "Day", None)
                .unwrap()
                .rate,
            d("200")
        );
        b.units.get_mut("Week").unwrap().hours = Some(d("7"));
        assert_eq!(
            lookup(&b, "Invented Crew", "Grip, Key", "Hour", None)
                .unwrap()
                .rate,
            d("142.86")
        );
        // No hours, no conversion.
        assert!(lookup(&b, "Invented Crew", "Grip, Key", "Flat", None)
            .unwrap_err()
            .0
            .contains("no hours"));
        b.units.get_mut("Week").unwrap().hours = None;
        assert!(lookup(&b, "Invented Crew", "Grip, Key", "Day", None)
            .unwrap_err()
            .0
            .contains("none of those units"));
    }

    #[test]
    fn effective_dates_pick_the_row() {
        let b = budget();
        let at = |date| lookup(&b, "Invented Crew", "Gaffer", "Hour", date).map(|f| f.rate);
        assert_eq!(at(Some("2030-03-01")).unwrap(), d("40"));
        assert_eq!(at(Some("2030-06-30")).unwrap(), d("40"));
        assert_eq!(at(Some("2030-07-01")).unwrap(), d("42"));
        assert_eq!(at(Some("2099-01-01")).unwrap(), d("42"));
        assert!(at(Some("2029-12-31"))
            .unwrap_err()
            .0
            .contains("in effect on"));
        assert!(at(None).unwrap_err().0.contains("2 periods"));
        assert!(at(Some("June")).is_err());
    }

    #[test]
    fn a_changed_table_offers_an_update_and_changes_nothing() {
        let mut b = budget();
        fill(&mut b, 1, "Invented Crew", "Grip, Key", Some("Hour"), None).unwrap();
        let before = b.clone();
        b.rate_tables[0].rows[0]
            .rates
            .insert("Hour".into(), d("55"));
        assert_eq!(
            check(&b, line(&b, 1)),
            Some(RateCheck::Changed { now: d("55") })
        );
        assert_eq!(line(&b, 1).rate, line(&before, 1).rate, "the figure stays");
        assert_eq!(changed_lines(&b), vec![(1, d("50"), d("55"))]);
        let words = source_words(&b, line(&b, 1)).unwrap();
        assert!(words.contains("the table now says 55"), "{words}");
        assert_eq!(update(&mut b, 1).unwrap(), d("55"));
        assert_eq!(line(&b, 1).rate.0, "55");
        assert_eq!(check(&b, line(&b, 1)), Some(RateCheck::Current));
        // Typed over: no update, the check says so.
        b.rate_tables[0].rows[0]
            .rates
            .insert("Hour".into(), d("60"));
        set_rate(&mut b, 1, "70");
        assert_eq!(check(&b, line(&b, 1)), Some(RateCheck::Edited));
        assert!(update(&mut b, 1).is_err());
        // A deleted table: the figure and its source stay.
        let mut b = budget();
        fill(&mut b, 1, "Invented Crew", "Grip, Key", Some("Hour"), None).unwrap();
        delete_table(&mut b, "Invented Crew").unwrap();
        assert_eq!(line(&b, 1).rate.0, "50");
        assert!(matches!(
            check(&b, line(&b, 1)),
            Some(RateCheck::Missing(_))
        ));
    }

    fn set_rate(b: &mut Budget, id: RowId, v: &str) {
        details_mut(b).find(|x| x.id == id).unwrap().rate = v.into();
    }

    #[test]
    fn renaming_a_table_or_a_unit_keeps_lines_with_it() {
        let mut b = budget();
        fill(&mut b, 1, "Invented Crew", "Grip, Key", Some("Hour"), None).unwrap();
        let mut t = crew();
        t.name = "Crew Two".into();
        save_table(&mut b, t, Some("Invented Crew")).unwrap();
        assert_eq!(line(&b, 1).rate_source.as_ref().unwrap().table, "Crew Two");
        assert_eq!(check(&b, line(&b, 1)), Some(RateCheck::Current));
        rename_unit(&mut b, "Hour", "Hr");
        assert_eq!(line(&b, 1).rate_source.as_ref().unwrap().unit, "Hr");
        assert!(b.rate_tables[0].rows[0].rates.contains_key("Hr"));
        // Names stay unique.
        assert!(save_table(&mut b, crew(), None).is_ok());
        assert!(save_table(&mut b, crew(), None).is_err());
        let mut t = crew();
        t.name = "Crew Two".into();
        assert!(save_table(&mut b, t, Some("Invented Crew")).is_err());
    }

    #[test]
    fn explain_says_where_the_rate_came_from() {
        use crate::explain::{explain, Target};
        let mut b = budget();
        fill(&mut b, 1, "Invented Crew", "Grip, Key", None, None).unwrap();
        let text = |b: &Budget| {
            explain(b, &crate::RecalcOptions::default(), &Target::Line(1))
                .unwrap()
                .to_text(None)
        };
        let t = text(&b);
        assert!(
            t.contains("filled from the rate table Invented Crew, Grip, Key"),
            "{t}"
        );
        assert!(t.contains("converted from 50 per Hour"), "{t}");
        b.rate_tables[0].rows[0]
            .rates
            .insert("Hour".into(), d("51"));
        let t = text(&b);
        assert!(t.contains("the table now says 612"), "{t}");
        // A line never filled from a table says nothing about one.
        assert!(
            !explain(&b, &crate::RecalcOptions::default(), &Target::Line(2))
                .unwrap()
                .to_text(None)
                .contains("rate table")
        );
    }
}
