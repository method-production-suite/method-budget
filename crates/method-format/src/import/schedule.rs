//! A schedule's day counts, from Method Scheduling's budget exchange file
//! (`format: "method-schedule-days"`, roadmap 3.8). The spec lives in that
//! project's docs/EXCHANGE.md, frozen at version 1 on 2026-10-15; our copy
//! is docs/research/schedule-exchange.md. One sub-board's shoot days (in
//! all, by unit, set, location and week, and every shoot date), its dates,
//! and each element's days with every count kept apart.
//!
//! What lands where in a budget ([`ScheduleDays::budget_fields`]): the shoot
//! days in the `SHOOT_DAYS` global, the dates in the budget information
//! fields New Budget fills (Prep start, Shoot start, Shoot end, Wrap end,
//! decisions.md entry 63). Prep and wrap dates are never turned into day
//! counts. Element days are looked up by identity ([`ScheduleDays::find`])
//! as agreed with the scheduling side: (category, board ID, name), falling
//! back to the element's id, asking before relinking when only the id still
//! matches, and a missing element counting 0. Neither key is permanent: the
//! names are unique within one file only (a rename or a delete frees a name
//! for another element), and an id is one element only within one line of
//! a schedule's history (two copies of a file edited apart can give one id
//! to different elements; a merge renumbers on a clash; a script import
//! gives new ids). So a budget keeps both, and asks when they disagree.
//!
//! Versions 0 and 1 are read; 1 only adds `days_hash` and
//! `shoot_days.dates`. A later version is read too (changes after 1 are
//! additive only; a breaking change gets a new format name), with a note.
//! Fields this build doesn't know are ignored at every level. `days_hash`,
//! when given, is checked: a mismatch is a note, not an error, since the
//! counts themselves are each checked on their own.
//! The file is untrusted input: it goes through the same hand-written JSON
//! reader as the budget exports, with the same size cap, and every field is
//! checked; a bad file is an error in words, never a panic.

use std::collections::BTreeSet;

use super::exchange::json::{self, J};
use super::{ReadError, MAX_CELL_CHARS, MAX_FILE_BYTES};

/// The `format` field of a schedule days file.
pub const FORMAT: &str = "method-schedule-days";
/// The newest version whose shape this build knows. Version 1 is version 0
/// frozen, so both read the same.
pub const KNOWN_VERSION: u64 = 1;
/// Most elements read from one file.
pub const MAX_ELEMENTS: usize = 100_000;
/// Most entries read in one of the `by_` counts.
pub const MAX_COUNTS: usize = 100_000;

/// The global the shoot days land in (the starter's, and New Budget's).
pub const SHOOT_DAYS_GLOBAL: &str = "SHOOT_DAYS";

/// One sub-board's days, as the file gives them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduleDays {
    pub version: u64,
    /// The schedule's title.
    pub schedule: String,
    pub board: String,
    pub sub_board: String,
    /// The day the file was written, `YYYY-MM-DD`.
    pub generated: Option<String>,
    /// Changes whenever the schedule file does (view settings included), so
    /// a linked budget can say the schedule changed. Compared, never parsed.
    pub source_hash: String,
    /// Changes only when something this file reports changes: what a
    /// linked budget compares to know its numbers are out of date. New in
    /// version 1; compared, never parsed.
    pub days_hash: Option<String>,
    pub shoot_days: ShootDays,
    pub elements: Vec<ElementDays>,
    /// What looked odd but didn't stop the file being read, in words.
    pub notes: Vec<String>,
}

/// The sub-board's shoot days and dates. Dates are `YYYY-MM-DD`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShootDays {
    pub start: String,
    /// The last shoot day; `None` when there are no days.
    pub end: Option<String>,
    pub total: u32,
    /// Name to the shoot days with at least one sheet of it, in the file's
    /// order. A day can count under several names, so these don't add up
    /// to `total`.
    pub by_unit: Vec<(String, u32)>,
    pub by_set: Vec<(String, u32)>,
    pub by_location: Vec<(String, u32)>,
    /// The Monday of each week with shoot days, to its shoot days. These do
    /// add up to `total`.
    pub by_week: Vec<(String, u32)>,
    /// Every shoot day's date, in order (new in version 1; `None` in a
    /// version 0 file).
    pub dates: Option<Vec<String>>,
    pub prep_start: Option<String>,
    pub wrap_end: Option<String>,
}

/// One element's days on the sub-board, each count apart.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ElementDays {
    /// The element's stable id in the schedule file.
    pub id: u32,
    /// Empty when the file leaves it out.
    pub category: String,
    /// Cast number or similar.
    pub board_id: Option<String>,
    pub name: String,
    /// First and last strip day.
    pub first: Option<String>,
    pub last: Option<String>,
    pub work: u32,
    pub hold: u32,
    pub travel: u32,
    pub fitting: u32,
    pub rehearsal: u32,
    pub holiday: u32,
    pub company_move: u32,
    /// The seven counts added up (checked when read): the day out of days
    /// total, and the days a budget uses unless told otherwise.
    pub total: u32,
    pub holds: bool,
    pub drops: bool,
    pub min_drop_gap: u32,
}

/// Something the file fills in a budget.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BudgetField {
    /// A global set to a whole number of days.
    Global { name: &'static str, days: u32 },
    /// A budget information date field.
    Date { label: &'static str, date: String },
}

/// How a budget names an element it linked to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ElementKey {
    pub category: String,
    pub board_id: Option<String>,
    pub name: String,
    /// The id it had when linked.
    pub id: Option<u32>,
}

/// What [`ScheduleDays::find`] found for a linked element.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Found<'a> {
    /// The same element.
    Same(&'a ElementDays),
    /// Only the id still matches (board ID and name both changed, or the
    /// category did): ask before relinking.
    IdOnly(&'a ElementDays),
    /// Several elements have this category, board ID and name, and none has
    /// the linked id: ask which.
    Ambiguous(Vec<&'a ElementDays>),
    /// Not in the file: 0 days on this sub-board. Warn rather than keep an
    /// old number.
    Missing,
}

impl ElementDays {
    /// The key a budget keeps to find this element again.
    pub fn key(&self) -> ElementKey {
        ElementKey {
            category: self.category.clone(),
            board_id: self.board_id.clone(),
            name: self.name.clone(),
            id: Some(self.id),
        }
    }
}

/// Names compared as people type them: trimmed, any letter case.
fn same(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

fn same_board_id(a: Option<&str>, b: Option<&str>) -> bool {
    same(a.unwrap_or(""), b.unwrap_or(""))
}

impl ScheduleDays {
    /// What the file fills in a budget: the shoot days global and the dates
    /// it has. Prep and wrap are dates only; their day counts stay the
    /// budget's own assumptions.
    pub fn budget_fields(&self) -> Vec<BudgetField> {
        let d = &self.shoot_days;
        let mut out = vec![BudgetField::Global {
            name: SHOOT_DAYS_GLOBAL,
            days: d.total,
        }];
        for (label, date) in [
            ("Prep start", &d.prep_start),
            ("Shoot start", &Some(d.start.clone())),
            ("Shoot end", &d.end),
            ("Wrap end", &d.wrap_end),
        ] {
            if let Some(date) = date {
                out.push(BudgetField::Date {
                    label,
                    date: date.clone(),
                });
            }
        }
        out
    }

    /// Finds a linked element: by category, board ID and name, then by id.
    pub fn find(&self, key: &ElementKey) -> Found<'_> {
        let by_id = key
            .id
            .and_then(|id| self.elements.iter().find(|e| e.id == id));
        let named: Vec<&ElementDays> = self
            .elements
            .iter()
            .filter(|e| {
                same(&e.category, &key.category)
                    && same(&e.name, &key.name)
                    && same_board_id(e.board_id.as_deref(), key.board_id.as_deref())
            })
            .collect();
        match named.as_slice() {
            [one] => return Found::Same(one),
            [] => {}
            several => {
                return match by_id.filter(|e| several.iter().any(|n| n.id == e.id)) {
                    Some(e) => Found::Same(e),
                    None => Found::Ambiguous(named),
                }
            }
        }
        match by_id {
            // Renamed or renumbered, not both: still the same element.
            Some(e)
                if same(&e.category, &key.category)
                    && (same(&e.name, &key.name)
                        || (key.board_id.is_some()
                            && same_board_id(e.board_id.as_deref(), key.board_id.as_deref()))) =>
            {
                Found::Same(e)
            }
            Some(e) => Found::IdOnly(e),
            None => Found::Missing,
        }
    }
}

/// Whether `bytes` look like a schedule days file: a JSON object whose
/// `format` is [`FORMAT`] near the start.
pub fn looks_like(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(4096)];
    let s = String::from_utf8_lossy(head);
    let t = s.trim_start_matches('\u{feff}').trim_start();
    t.starts_with('{') && t.contains(&format!("\"{FORMAT}\""))
}

/// Reads a schedule days file.
pub fn read(bytes: &[u8]) -> Result<ScheduleDays, ReadError> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(ReadError::new(format!(
            "the file is too large to import (more than {} MB)",
            MAX_FILE_BYTES >> 20
        )));
    }
    let src = std::str::from_utf8(bytes)
        .map_err(|_| ReadError::new("the file isn't UTF-8 text"))?
        .trim_start_matches('\u{feff}');
    let root = json::parse(src)?;
    let top = Obj::new(&root, "the file")?;
    match top.get("format") {
        Some(J::Str(f)) if f == FORMAT => {}
        _ => {
            return Err(ReadError::new(
                "this isn't a schedule days file (its format isn't \"method-schedule-days\")",
            ))
        }
    }
    let mut notes = Vec::new();
    let version = top.whole("version")?;
    if version > KNOWN_VERSION {
        notes.push(format!(
            "The file is version {version}, newer than this build knows ({KNOWN_VERSION}); what it added is left out."
        ));
    }

    let sd = Obj::new(top.need("shoot_days")?, "shoot_days")?;
    let total = sd.days("total")?;
    let shoot_days = ShootDays {
        start: sd.date("start")?,
        end: sd.opt_date("end")?,
        total,
        by_unit: sd.counts("by_unit", false)?,
        by_set: sd.counts("by_set", false)?,
        by_location: sd.counts("by_location", false)?,
        by_week: sd.counts("by_week", true)?,
        dates: sd.opt_dates("dates")?,
        prep_start: sd.opt_date("prep_start")?,
        wrap_end: sd.opt_date("wrap_end")?,
    };
    check_shoot_days(&shoot_days, &mut notes);

    let list = match top.need("elements")? {
        J::Arr(a) => a,
        _ => return Err(ReadError::new("elements isn't a list")),
    };
    if list.len() > MAX_ELEMENTS {
        return Err(ReadError::new(format!(
            "the file lists more than {MAX_ELEMENTS} elements"
        )));
    }
    let mut elements = Vec::with_capacity(list.len());
    let mut ids = BTreeSet::new();
    for (i, v) in list.iter().enumerate() {
        let e = element(v, i + 1)?;
        if !ids.insert(e.id) {
            return Err(ReadError::new(format!(
                "element {} ({}) has the same id as an earlier one, {}",
                i + 1,
                e.name,
                e.id
            )));
        }
        elements.push(e);
    }

    let days_hash_given = top.opt_text("days_hash")?;
    if let Some(given) = &days_hash_given {
        let worked = days_hash(top.need("shoot_days")?, top.need("elements")?);
        if !given.eq_ignore_ascii_case(&worked) {
            notes.push(format!(
                "The file's days_hash ({given}) isn't what its days come to ({worked}): it may have been edited by hand."
            ));
        }
    }
    for (i, e) in elements.iter().enumerate() {
        if e.first.is_some() != e.last.is_some() {
            notes.push(format!(
                "Element {} ({}) gives only one of its first and last strip days.",
                i + 1,
                e.name
            ));
        }
    }

    Ok(ScheduleDays {
        version,
        schedule: top.text("schedule")?,
        board: top.text("board")?,
        sub_board: top.text("sub_board")?,
        generated: top.opt_date("generated")?,
        days_hash: days_hash_given,
        source_hash: top.text("source_hash").and_then(|h| {
            if h.trim().is_empty() {
                Err(ReadError::new("source_hash is empty"))
            } else {
                Ok(h)
            }
        })?,
        shoot_days,
        elements,
        notes,
    })
}

fn element(v: &J, n: usize) -> Result<ElementDays, ReadError> {
    let o = Obj::new(v, &format!("element {n}"))?;
    let e = ElementDays {
        id: o.days("id")?,
        category: o.opt_text("category")?.unwrap_or_default(),
        board_id: o.opt_text("board_id")?,
        name: o.text("name")?,
        first: o.opt_date("first")?,
        last: o.opt_date("last")?,
        work: o.days("work")?,
        hold: o.days("hold")?,
        travel: o.days("travel")?,
        fitting: o.days("fitting")?,
        rehearsal: o.days("rehearsal")?,
        holiday: o.days("holiday")?,
        company_move: o.days("company_move")?,
        total: o.days("total")?,
        holds: o.flag("holds")?,
        drops: o.flag("drops")?,
        min_drop_gap: o.days("min_drop_gap")?,
    };
    let sum = [
        e.work,
        e.hold,
        e.travel,
        e.fitting,
        e.rehearsal,
        e.holiday,
        e.company_move,
    ]
    .iter()
    .map(|&d| u64::from(d))
    .sum::<u64>();
    if sum != u64::from(e.total) {
        return Err(ReadError::new(format!(
            "element {n} ({}) has a total of {} but its counts add up to {sum}",
            e.name, e.total
        )));
    }
    if let (Some(a), Some(b)) = (&e.first, &e.last) {
        if a > b {
            return Err(ReadError::new(format!(
                "element {n} ({}) has its first day ({a}) after its last ({b})",
                e.name
            )));
        }
    }
    Ok(e)
}

/// The `shoot_days` keys `days_hash` covers (its coverage is frozen at
/// version 1).
const HASHED_SHOOT_DAYS: [&str; 10] = [
    "start",
    "end",
    "total",
    "by_unit",
    "by_set",
    "by_location",
    "by_week",
    "dates",
    "prep_start",
    "wrap_end",
];

/// The element keys `days_hash` covers.
const HASHED_ELEMENT: [&str; 17] = [
    "id",
    "category",
    "board_id",
    "name",
    "first",
    "last",
    "work",
    "hold",
    "travel",
    "fitting",
    "rehearsal",
    "holiday",
    "company_move",
    "total",
    "holds",
    "drops",
    "min_drop_gap",
];

/// `days_hash` worked out from the file's own `shoot_days` and `elements`:
/// FNV-1a 64 over `{"shoot_days":…,"elements":[…]}` as compact JSON, keys
/// and items in the file's order, leaving out keys added after version 1.
fn days_hash(shoot_days: &J, elements: &J) -> String {
    let mut out = String::from("{\"shoot_days\":");
    compact(shoot_days, Some(&HASHED_SHOOT_DAYS), &mut out);
    out.push_str(",\"elements\":[");
    for (i, e) in elements.arr().iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        compact(e, Some(&HASHED_ELEMENT), &mut out);
    }
    out.push_str("]}");
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in out.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// `v` as compact JSON, as the writer's JSON library writes it (numbers as
/// the file wrote them). An object keeps only the `keep` keys when given.
fn compact(v: &J, keep: Option<&[&str]>, out: &mut String) {
    match v {
        J::Null => out.push_str("null"),
        J::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        J::Num(n) => out.push_str(n),
        J::Str(s) => quote(s, out),
        J::Arr(items) => {
            out.push('[');
            for (i, x) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                compact(x, None, out);
            }
            out.push(']');
        }
        J::Obj(fields) => {
            out.push('{');
            let mut first = true;
            for (k, x) in fields {
                if keep.is_some_and(|keep| !keep.contains(&k.as_str())) {
                    continue;
                }
                if !first {
                    out.push(',');
                }
                first = false;
                quote(k, out);
                out.push(':');
                compact(x, None, out);
            }
            out.push('}');
        }
    }
}

/// A JSON string as serde_json writes one: `"` and `\` escaped, control
/// characters as their short escape or `\u00xx`, everything else as is.
fn quote(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Odd but readable: said in the notes.
fn check_shoot_days(d: &ShootDays, notes: &mut Vec<String>) {
    match &d.end {
        Some(end) if d.total == 0 => notes.push(format!(
            "The file has no shoot days but gives a last shoot day ({end})."
        )),
        None if d.total > 0 => notes.push(format!(
            "The file has {} shoot days but no last shoot day.",
            d.total
        )),
        Some(end) if end < &d.start => notes.push(format!(
            "The last shoot day ({end}) is before the start ({}).",
            d.start
        )),
        _ => {}
    }
    if let Some(dates) = &d.dates {
        if dates.len() != d.total as usize {
            notes.push(format!(
                "The file lists {} shoot dates for {} shoot days.",
                dates.len(),
                d.total
            ));
        }
        if dates.windows(2).any(|w| w[0] >= w[1]) {
            notes.push("The shoot dates aren't in order, one per day.".into());
        }
        if dates.first().is_some_and(|f| f < &d.start) || dates.last() != d.end.as_ref() {
            notes.push("The shoot dates don't run from the start to the last shoot day.".into());
        }
    }
    let weeks: u64 = d.by_week.iter().map(|(_, n)| u64::from(*n)).sum();
    if weeks != u64::from(d.total) {
        notes.push(format!(
            "The shoot days by week add up to {weeks}, not the {} in all.",
            d.total
        ));
    }
    for (what, counts) in [
        ("unit", &d.by_unit),
        ("set", &d.by_set),
        ("location", &d.by_location),
        ("week", &d.by_week),
    ] {
        for (name, n) in counts {
            if *n > d.total {
                notes.push(format!(
                    "The {what} {name} has {n} shoot days, more than the {} in all.",
                    d.total
                ));
            }
        }
    }
    if let Some(p) = &d.prep_start {
        if p > &d.start {
            notes.push(format!(
                "Prep starts ({p}) after the shoot does ({}).",
                d.start
            ));
        }
    }
    if let Some(w) = &d.wrap_end {
        let last = d.end.as_ref().unwrap_or(&d.start);
        if w < last {
            notes.push(format!(
                "Wrap ends ({w}) before the last shoot day ({last})."
            ));
        }
    }
}

/// A JSON object read field by field, refusing a key given twice.
struct Obj<'a> {
    fields: &'a [(String, J)],
    what: String,
}

impl<'a> Obj<'a> {
    fn new(v: &'a J, what: &str) -> Result<Obj<'a>, ReadError> {
        let J::Obj(fields) = v else {
            return Err(ReadError::new(format!("{what} isn't a JSON object")));
        };
        let mut seen = BTreeSet::new();
        for (k, _) in fields {
            if !seen.insert(k.as_str()) {
                return Err(ReadError::new(format!("{what} gives {k} twice")));
            }
        }
        Ok(Obj {
            fields,
            what: what.to_string(),
        })
    }

    /// A field; `null` counts as left out.
    fn get(&self, key: &str) -> Option<&'a J> {
        self.fields
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
            .filter(|v| !matches!(v, J::Null))
    }

    fn need(&self, key: &str) -> Result<&'a J, ReadError> {
        self.get(key)
            .ok_or_else(|| ReadError::new(format!("{} has no {key}", self.what)))
    }

    fn bad(&self, key: &str, should: &str) -> ReadError {
        ReadError::new(format!("{key} in {} should be {should}", self.what))
    }

    fn opt_text(&self, key: &str) -> Result<Option<String>, ReadError> {
        match self.get(key) {
            None => Ok(None),
            Some(J::Str(s)) if s.chars().count() <= MAX_CELL_CHARS => Ok(Some(s.clone())),
            Some(J::Str(_)) => Err(self.bad(key, &format!("at most {MAX_CELL_CHARS} characters"))),
            Some(_) => Err(self.bad(key, "text")),
        }
    }

    fn text(&self, key: &str) -> Result<String, ReadError> {
        self.need(key)?;
        Ok(self.opt_text(key)?.unwrap_or_default())
    }

    fn opt_date(&self, key: &str) -> Result<Option<String>, ReadError> {
        match self.get(key) {
            None => Ok(None),
            Some(J::Str(s)) if method_core::actuals::is_iso_date(s) => Ok(Some(s.clone())),
            Some(_) => Err(self.bad(key, "a date written YYYY-MM-DD")),
        }
    }

    fn date(&self, key: &str) -> Result<String, ReadError> {
        self.need(key)?;
        Ok(self.opt_date(key)?.unwrap_or_default())
    }

    /// A list of dates; left out is `None`.
    fn opt_dates(&self, key: &str) -> Result<Option<Vec<String>>, ReadError> {
        let Some(v) = self.get(key) else {
            return Ok(None);
        };
        let J::Arr(items) = v else {
            return Err(self.bad(key, "a list of dates"));
        };
        if items.len() > MAX_COUNTS {
            return Err(ReadError::new(format!(
                "{key} has more than {MAX_COUNTS} entries"
            )));
        }
        items
            .iter()
            .map(|d| match d {
                J::Str(s) if method_core::actuals::is_iso_date(s) => Ok(s.clone()),
                _ => Err(self.bad(key, "a list of dates written YYYY-MM-DD")),
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Some)
    }

    fn whole(&self, key: &str) -> Result<u64, ReadError> {
        whole(self.need(key)?).ok_or_else(|| self.bad(key, "a whole number, 0 or more"))
    }

    fn days(&self, key: &str) -> Result<u32, ReadError> {
        u32::try_from(self.whole(key)?).map_err(|_| self.bad(key, "a smaller whole number"))
    }

    fn flag(&self, key: &str) -> Result<bool, ReadError> {
        match self.need(key)? {
            J::Bool(b) => Ok(*b),
            _ => Err(self.bad(key, "true or false")),
        }
    }

    /// One of the `by_` objects, name to days. Left out means none. Week
    /// keys must be dates.
    fn counts(&self, key: &str, dates: bool) -> Result<Vec<(String, u32)>, ReadError> {
        let Some(v) = self.get(key) else {
            return Ok(Vec::new());
        };
        let o = Obj::new(v, key)?;
        if o.fields.len() > MAX_COUNTS {
            return Err(ReadError::new(format!(
                "{key} has more than {MAX_COUNTS} entries"
            )));
        }
        let mut out = Vec::with_capacity(o.fields.len());
        for (name, _) in o.fields {
            if dates && !method_core::actuals::is_iso_date(name) {
                return Err(ReadError::new(format!(
                    "{key} has {name:?}, which isn't a date written YYYY-MM-DD"
                )));
            }
            if name.chars().count() > MAX_CELL_CHARS {
                return Err(ReadError::new(format!(
                    "{key} has a name longer than {MAX_CELL_CHARS} characters"
                )));
            }
            out.push((name.clone(), o.days(name)?));
        }
        Ok(out)
    }
}

/// A whole number as JSON writes one: digits only, no sign, point or
/// exponent.
fn whole(v: &J) -> Option<u64> {
    match v {
        J::Num(s) if !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) => s.parse().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn element(id: u32, category: &str, board_id: Option<&str>, name: &str) -> ElementDays {
        ElementDays {
            id,
            category: category.into(),
            board_id: board_id.map(Into::into),
            name: name.into(),
            first: None,
            last: None,
            work: 1,
            hold: 0,
            travel: 0,
            fitting: 0,
            rehearsal: 0,
            holiday: 0,
            company_move: 0,
            total: 1,
            holds: false,
            drops: false,
            min_drop_gap: 1,
        }
    }

    fn days(elements: Vec<ElementDays>) -> ScheduleDays {
        ScheduleDays {
            version: 0,
            schedule: String::new(),
            board: String::new(),
            sub_board: String::new(),
            generated: None,
            source_hash: "0".into(),
            days_hash: None,
            shoot_days: ShootDays {
                start: "2027-03-01".into(),
                end: None,
                total: 0,
                by_unit: vec![],
                by_set: vec![],
                by_location: vec![],
                by_week: vec![],
                dates: None,
                prep_start: None,
                wrap_end: None,
            },
            elements,
            notes: vec![],
        }
    }

    fn key(category: &str, board_id: Option<&str>, name: &str, id: Option<u32>) -> ElementKey {
        ElementKey {
            category: category.into(),
            board_id: board_id.map(Into::into),
            name: name.into(),
            id,
        }
    }

    #[test]
    fn elements_are_found_by_name_then_id() {
        let s = days(vec![
            element(7, "Cast", Some("1"), "KIT"),
            element(8, "Cast", None, "EXTRA"),
            element(9, "Cast", None, "EXTRA"),
            element(10, "Props", None, "LANTERN"),
        ]);
        let id = |f: Found<'_>| match f {
            Found::Same(e) => format!("same {}", e.id),
            Found::IdOnly(e) => format!("id {}", e.id),
            Found::Ambiguous(v) => format!("ambiguous {}", v.len()),
            Found::Missing => "missing".into(),
        };
        // By name, in any letter case, whatever the id.
        assert_eq!(id(s.find(&key(" cast", Some("1"), "kit ", None))), "same 7");
        assert_eq!(
            id(s.find(&key("Cast", Some("1"), "KIT", Some(99)))),
            "same 7"
        );
        // Renamed, or renumbered: the id carries it.
        assert_eq!(
            id(s.find(&key("Cast", Some("1"), "KITTY", Some(7)))),
            "same 7"
        );
        assert_eq!(
            id(s.find(&key("Cast", Some("2"), "KIT", Some(7)))),
            "same 7"
        );
        // Both changed, or the category: ask.
        assert_eq!(
            id(s.find(&key("Cast", Some("2"), "KITTY", Some(7)))),
            "id 7"
        );
        assert_eq!(id(s.find(&key("Props", Some("1"), "KIT", Some(7)))), "id 7");
        // Two with the same name: the id picks, else ask.
        assert_eq!(id(s.find(&key("Cast", None, "EXTRA", Some(9)))), "same 9");
        assert_eq!(id(s.find(&key("Cast", None, "EXTRA", None))), "ambiguous 2");
        // No board ID on either side is a match; one side only is not.
        assert_eq!(id(s.find(&key("Props", None, "LANTERN", None))), "same 10");
        assert_eq!(
            id(s.find(&key("Props", Some("4"), "LANTERN", None))),
            "missing"
        );
        assert_eq!(
            id(s.find(&key("Cast", None, "NOBODY", Some(42)))),
            "missing"
        );
    }

    #[test]
    fn whole_numbers_only() {
        assert_eq!(whole(&J::Num("25".into())), Some(25));
        for bad in ["-1", "2.0", "1e3", ""] {
            assert_eq!(whole(&J::Num(bad.into())), None, "{bad}");
        }
        assert_eq!(whole(&J::Str("3".into())), None);
    }
}
