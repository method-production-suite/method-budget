//! File > Import Schedule Days…: a schedule's day counts drive the budget's
//! globals (roadmap 3.8). The file is Method Schedule's days file
//! (`method-schedule-days`, read by `method_format::import::schedule`; spec
//! copy in docs/research/schedule-exchange.md).
//!
//! - **Figures.** Everything the file counts that a global could hold: the
//!   shoot days in all, the shoot days per unit, set, location and week, and
//!   each element's days (the day out of days total, as agreed with the
//!   scheduling side). Each figure has a key that stays the same between
//!   imports of one schedule (`total`, `location:<name>`, `element:<id>`...).
//! - **The rule** (decisions.md, entry 76). The shoot days in all always go
//!   to `SHOOT_DAYS`. Any other figure goes to a global only when picked:
//!   the person picks it and may name the global (an existing one to update
//!   it, such as `STATION_DAYS`, or a new one). Its default name is the
//!   figure's name in capitals with `_DAYS` (`COLD_SPRING_FIELD_STATION_DAYS`),
//!   never a global the budget already has: that gets `_2`, `_3`... (the
//!   Field Station budget's `RIDGE_DAYS` is the fire road location's, so the
//!   RIDGE set defaults to `RIDGE_DAYS_2`). A figure is picked to start with
//!   only when an earlier import set a global from it. Prep and wrap are
//!   never counted: those globals are the budget's own assumptions.
//! - **The link** is the global's setup source (`setup_sources`, already in
//!   the file format): its notes say which figure of which schedule set it,
//!   in words, then its key ("Shoot days at location X, from the schedule T
//!   (board, sub board) [key: location:X]"; an element's key is its id,
//!   category, board ID and name). A later import finds the figure again by
//!   that key (an element through the reader's identity rules: renamed or
//!   renumbered is the same element, both changed asks first), falling back
//!   to the words for a note without a key and saying so. It picks linked
//!   globals again and says which ones the schedule changed. A linked figure
//!   the file no longer has is offered at 0 days but starts unticked, so it
//!   is set to 0 only when picked. Remembering the file's `days_hash` would
//!   need a new field in the budget file; that is proposed in the roadmap's
//!   3.8 notes and not done here.
//! - **The review** lists every figure with its global, the global's value
//!   now, the new value and what changes; **applying** sets the picked
//!   globals (and their sources) as one undo step.
//!
//! Bytes in, no file system: the desktop, the browser build, the command
//! line and the MCP server all call these.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, MutexGuard};

use method_core::{recalc, Budget, RecalcOptions};
use method_format::import::schedule::{self, ScheduleDays, SHOOT_DAYS_GLOBAL};
use serde::{Deserialize, Serialize};

use crate::docs::{AppState, DocId};
use crate::history::count;
use crate::session::Session;

/// The setup source kind globals are kept under.
const GLOBAL_KIND: &str = "global";
/// The longest default global name made from a figure's name, before `_DAYS`.
const MAX_SLUG: usize = 40;
/// The unit a new global gets, when the budget has it.
const DAY_UNIT: &str = "Day";

// ---------------------------------------------------------------------------
// The file, kept by token between the dialog's calls

struct Source {
    token: u64,
    file: String,
    days: Arc<ScheduleDays>,
}

/// The last schedule days file picked (the app's state; one for every
/// budget).
#[derive(Default)]
pub struct ScheduleSources {
    inner: Mutex<(u64, Option<Source>)>,
}

impl ScheduleSources {
    fn lock(&self) -> MutexGuard<'_, (u64, Option<Source>)> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Keeps `days` (read from a file named `file`) and returns its token.
    pub fn keep(&self, file: String, days: ScheduleDays) -> u64 {
        let mut g = self.lock();
        g.0 += 1;
        let token = g.0;
        g.1 = Some(Source {
            token,
            file,
            days: Arc::new(days),
        });
        token
    }

    /// The file with `token`, or a message when another was picked since.
    pub fn get(&self, token: u64) -> Result<(String, Arc<ScheduleDays>), String> {
        let g = self.lock();
        g.1.as_ref()
            .filter(|s| s.token == token)
            .map(|s| (s.file.clone(), Arc::clone(&s.days)))
            .ok_or_else(|| "Another file was picked since. Import again.".to_string())
    }
}

// ---------------------------------------------------------------------------
// Figures

/// One thing the file counts that a global can hold.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Figure {
    /// Stays the same between imports of one schedule: `total`,
    /// `unit:<name>`, `set:<name>`, `location:<name>`, `week:<date>`,
    /// `element:<id>`, or `linked:<global>` for a global linked to a figure
    /// the file no longer has.
    pub key: String,
    /// `total`, `unit`, `set`, `location`, `week`, `element` or `missing`.
    pub kind: &'static str,
    /// The unit, set or location name, the week's Monday, the element's
    /// name; empty for the total.
    pub name: String,
    /// An element's category.
    pub category: String,
    /// An element's id and board ID in the schedule file.
    pub id: Option<u32>,
    pub board_id: Option<String>,
    pub days: u32,
    /// What it is, in words: "Shoot days at location Cold Spring field
    /// station".
    pub label: String,
}

/// Every figure in `days`, in the review's order: the total, then by unit,
/// set, location and week, then the elements in the file's order.
pub fn figures(days: &ScheduleDays) -> Vec<Figure> {
    let d = &days.shoot_days;
    let mut out = vec![Figure {
        key: "total".into(),
        kind: "total",
        name: String::new(),
        category: String::new(),
        id: None,
        board_id: None,
        days: d.total,
        label: "Shoot days in all".into(),
    }];
    for (kind, list, words) in [
        ("unit", &d.by_unit, "Shoot days of unit"),
        ("set", &d.by_set, "Shoot days on set"),
        ("location", &d.by_location, "Shoot days at location"),
        ("week", &d.by_week, "Shoot days in the week of"),
    ] {
        for (name, n) in list {
            out.push(Figure {
                key: format!("{kind}:{name}"),
                kind,
                name: name.clone(),
                category: String::new(),
                id: None,
                board_id: None,
                days: *n,
                label: format!("{words} {name}"),
            });
        }
    }
    for e in &days.elements {
        let who = [
            e.category.trim(),
            e.board_id.as_deref().unwrap_or("").trim(),
            e.name.trim(),
        ]
        .iter()
        .filter(|s| !s.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" ");
        out.push(Figure {
            key: format!("element:{}", e.id),
            kind: "element",
            name: e.name.clone(),
            category: e.category.clone(),
            id: Some(e.id),
            board_id: e.board_id.clone(),
            days: e.total,
            label: format!("Days of {who}"),
        });
    }
    out
}

/// What a global's setup source says when this file set it: the figure,
/// the schedule, then the figure's key, so a later import finds the figure
/// again by the most stable thing the file has (an element's id, category,
/// board ID and name; a unit's, set's, location's or week's name), not by
/// its words.
fn source_note(days: &ScheduleDays, f: &Figure) -> String {
    format!("{}{}{KEY_OPEN}{}]", f.label, source_tail(days), key_spec(f))
}

/// Where a source note's key starts.
const KEY_OPEN: &str = " [key: ";
/// Between an element's key parts.
const KEY_SEP: &str = " | ";

/// A key part as written in a note: `|` and `]` would end it early.
fn key_part(s: &str) -> String {
    s.trim().replace('|', "/").replace(']', ")")
}

/// The key a figure is found again by: `location:NAME`..., and for an
/// element `element:ID | category | board ID | name`.
fn key_spec(f: &Figure) -> String {
    match (f.kind, f.id) {
        ("element", Some(id)) => [
            format!("element:{id}"),
            key_part(&f.category),
            key_part(f.board_id.as_deref().unwrap_or("")),
            key_part(&f.name),
        ]
        .join(KEY_SEP),
        _ => key_part(&f.key),
    }
}

/// The part of a source note naming the schedule and sub-board.
fn source_tail(days: &ScheduleDays) -> String {
    format!(
        ", from the schedule {} ({}, {})",
        days.schedule.trim(),
        days.board.trim(),
        days.sub_board.trim()
    )
}

/// A global an earlier import of this schedule's sub-board set, as its
/// source note records it.
struct Recorded {
    global: String,
    label: String,
    /// `None` for a note written before keys were recorded.
    key: Option<String>,
}

/// Every global whose source note says this file's schedule and sub-board
/// set it.
fn recorded(b: &Budget, days: &ScheduleDays) -> Vec<Recorded> {
    let tail = source_tail(days);
    let Some(by_name) = b.setup_sources.0.get(GLOBAL_KIND) else {
        return Vec::new();
    };
    by_name
        .iter()
        .filter(|(name, _)| b.globals.contains_key(*name))
        .filter_map(|(name, src)| {
            let note = src.notes.trim();
            let (head, key) = match note.strip_suffix(']').and_then(|n| n.rsplit_once(KEY_OPEN)) {
                Some((h, k)) => (h, Some(k.to_string())),
                None => (note, None),
            };
            let label = head.strip_suffix(&tail)?;
            (!label.is_empty()).then(|| Recorded {
                global: name.clone(),
                label: label.to_string(),
                key,
            })
        })
        .collect()
}

/// Where a recorded global's figure is in this file.
enum Found {
    /// The same figure; with a note when it was found by its words only.
    Same(usize, Option<String>),
    /// Only the element's id still matches: ask before relinking.
    Doubt(usize, String),
    /// Several figures match it and none by its id: ask which. Never gone,
    /// so never offered at 0.
    Ambiguous(Vec<usize>),
    /// Not in the file (or not one figure): why.
    Gone(String),
}

/// Finds a recorded global's figure in `figs`: by its key, else (an older
/// note) by its words.
fn find_recorded(days: &ScheduleDays, figs: &[Figure], r: &Recorded) -> Found {
    let at = |key: &str| figs.iter().position(|f| f.key == key);
    let gone = || Found::Gone("Not in this schedule file any more.".to_string());
    let Some(key) = &r.key else {
        return match figs.iter().position(|f| f.label == r.label) {
            Some(i) => Found::Same(
                i,
                Some("Found by its description only (set before keys were recorded); setting it again records its key.".into()),
            ),
            None => gone(),
        };
    };
    if !key.starts_with("element:") {
        return match figs.iter().position(|f| key_part(&f.key) == *key) {
            Some(i) => Found::Same(i, None),
            None => gone(),
        };
    }
    let parts: Vec<&str> = key.split(KEY_SEP).collect();
    let id = parts
        .first()
        .and_then(|p| p.strip_prefix("element:"))
        .and_then(|n| n.parse::<u32>().ok());
    let part = |i: usize| parts.get(i).map(|s| s.to_string()).unwrap_or_default();
    let board_id = Some(part(2)).filter(|s| !s.is_empty());
    let k = schedule::ElementKey {
        category: part(1),
        board_id: board_id.clone(),
        name: part(3),
        id,
    };
    let index = |e: &schedule::ElementDays| at(&format!("element:{}", e.id));
    match days.find(&k) {
        schedule::Found::Same(e) => index(e).map_or_else(gone, |i| Found::Same(i, None)),
        // No board ID on either side: only the name changed, so it is the
        // same element renamed.
        schedule::Found::IdOnly(e)
            if board_id.is_none()
                && e.board_id.is_none()
                && e.category.trim().eq_ignore_ascii_case(k.category.trim()) =>
        {
            index(e).map_or_else(gone, |i| Found::Same(i, None))
        }
        schedule::Found::IdOnly(e) => match index(e) {
            Some(i) => Found::Doubt(
                i,
                format!(
                    "Set from {} {} before; the schedule's element with that id is now named and numbered differently. Tick it to keep the link.",
                    k.category.trim(),
                    [k.board_id.as_deref().unwrap_or(""), k.name.trim()]
                        .iter()
                        .filter(|s| !s.is_empty())
                        .copied()
                        .collect::<Vec<_>>()
                        .join(" ")
                ),
            ),
            None => gone(),
        },
        schedule::Found::Ambiguous(list) => {
            let at: Vec<usize> = list.into_iter().filter_map(index).collect();
            if at.is_empty() {
                gone()
            } else {
                Found::Ambiguous(at)
            }
        }
        schedule::Found::Missing => gone(),
    }
}

/// A figure's name as part of a global's name: capitals, digits and `_`.
fn slug(name: &str) -> String {
    let mut s = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            s.push(c.to_ascii_uppercase());
        } else if !s.ends_with('_') {
            s.push('_');
        }
    }
    let s = s.trim_matches('_');
    let mut s: String = s.chars().take(MAX_SLUG).collect();
    while s.ends_with('_') {
        s.pop();
    }
    s
}

/// The global a figure goes to unless the person names another.
fn default_name(f: &Figure) -> String {
    match f.kind {
        "total" => SHOOT_DAYS_GLOBAL.to_string(),
        "week" => format!("WEEK_{}_DAYS", slug(&f.name)),
        kind => {
            let s = slug(&f.name);
            if s.is_empty() {
                format!("{}_DAYS", kind.to_ascii_uppercase())
            } else {
                format!("{s}_DAYS")
            }
        }
    }
}

// ---------------------------------------------------------------------------
// What the page sends and gets

/// A figure the person picked, and the global it goes to.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pick {
    pub key: String,
    /// The global's name; empty for the figure's default.
    #[serde(default)]
    pub global: String,
}

/// One figure in the review, with the global it would set.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FigureRow {
    pub key: String,
    pub kind: &'static str,
    pub name: String,
    pub category: String,
    pub label: String,
    pub days: u32,
    /// The global it goes to: the person's, an earlier import's, or the
    /// default.
    pub global: String,
    pub picked: bool,
    /// An earlier import of this schedule set `global` from this figure.
    pub linked: bool,
    /// The global's formula now, `None` when there is no such global.
    pub current: Option<String>,
    /// What that formula comes to, when it isn't a plain number.
    pub current_value: Option<String>,
    /// The new formula: the days as a whole number.
    pub new: String,
    /// `add` (a new global), `update` (a new value) or `same`.
    pub action: &'static str,
    /// Why the picked figure can't be set as asked.
    pub problem: Option<String>,
    /// What to know about its link: found by its words only, an element
    /// that only matches by id, why a linked figure is gone.
    pub note: Option<String>,
    /// An earlier import set this global and the schedule has changed it
    /// since.
    pub changed: bool,
}

/// The review: the file, every figure, and what applying would do.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Review {
    /// Names the kept file in later calls (0 when nothing is kept).
    pub token: u64,
    pub file: String,
    pub schedule: String,
    pub board: String,
    pub sub_board: String,
    pub generated: Option<String>,
    pub version: u64,
    pub source_hash: String,
    pub days_hash: Option<String>,
    pub shoot_days: u32,
    pub start: String,
    pub end: Option<String>,
    /// What looked odd in the file, in words.
    pub notes: Vec<String>,
    pub figures: Vec<FigureRow>,
    /// Picked rows that add a global, change one, or already match.
    pub adds: usize,
    pub updates: usize,
    pub same: usize,
    /// Globals an earlier import set that the schedule has changed since.
    pub changed: Vec<String>,
    /// Why applying is refused, if it is.
    pub problems: Vec<String>,
    /// Whether applying would change anything.
    pub can_apply: bool,
}

/// The review of `days` against budget `b` with `picks` (`None`: the
/// defaults). Changes nothing.
pub fn review(b: &Budget, days: &ScheduleDays, picks: Option<&[Pick]>) -> Review {
    let mut figs = figures(days);
    // Each figure's linked global (and whether to ask first), and a note.
    let mut linked: Vec<Option<String>> = vec![None; figs.len()];
    let mut doubt: Vec<Option<String>> = vec![None; figs.len()];
    let mut notes: Vec<Option<String>> = vec![None; figs.len()];
    let mut gone = Vec::new();
    // Every global recorded for each figure, to say when there are two.
    let mut claims: Vec<Vec<String>> = vec![Vec::new(); figs.len()];
    let add_note = |notes: &mut Vec<Option<String>>, i: usize, n: String| {
        notes[i] = Some(match notes[i].take() {
            Some(old) => format!("{old} {n}"),
            None => n,
        });
    };
    for r in recorded(b, days) {
        match find_recorded(days, &figs, &r) {
            Found::Same(i, note) => {
                claims[i].push(r.global.clone());
                if linked[i].is_none() && doubt[i].is_none() {
                    linked[i] = Some(r.global);
                    if let Some(n) = note {
                        add_note(&mut notes, i, n);
                    }
                }
            }
            Found::Doubt(i, note) => {
                claims[i].push(r.global.clone());
                if linked[i].is_none() && doubt[i].is_none() {
                    doubt[i] = Some(r.global);
                    add_note(&mut notes, i, note);
                }
            }
            Found::Ambiguous(at) => {
                let keys: Vec<&str> = at.iter().map(|&i| figs[i].key.as_str()).collect();
                let note = format!(
                    "{} was set from a figure that now matches more than one figure in this file ({}): pick the right one to keep it linked.",
                    r.global,
                    keys.join(", ")
                );
                for &i in &at {
                    claims[i].push(r.global.clone());
                    if linked[i].is_none() && doubt[i].is_none() {
                        doubt[i] = Some(r.global.clone());
                    }
                    add_note(&mut notes, i, note.clone());
                }
            }
            Found::Gone(why) => gone.push((r, why)),
        }
    }
    for (i, globals) in claims.iter().enumerate() {
        if globals.len() > 1 {
            let kept = linked[i]
                .as_ref()
                .or(doubt[i].as_ref())
                .cloned()
                .unwrap_or_default();
            add_note(
                &mut notes,
                i,
                format!(
                    "{} were all set from this figure before; only {kept} is offered here, the others are left as they are.",
                    globals.join(", ")
                ),
            );
        }
    }
    for (r, why) in gone {
        figs.push(Figure {
            key: format!("linked:{}", r.global),
            kind: "missing",
            name: r.global.clone(),
            category: String::new(),
            id: None,
            board_id: None,
            days: 0,
            label: r.label,
        });
        linked.push(Some(r.global));
        doubt.push(None);
        notes.push(Some(format!("{why} Tick it to set the global to 0.")));
    }
    let values = recalc(b, &RecalcOptions::default())
        .ok()
        .map(|r| r.globals)
        .unwrap_or_default();

    // Each figure's default global: its linked one, the one it may be
    // linked to, else a free name. A default is never a global the budget
    // already has (decisions.md, entry 76: `set:RIDGE` must not land on the
    // fire road's `RIDGE_DAYS`); naming one in the row updates it.
    let mut taken: BTreeSet<String> = b.globals.keys().cloned().collect();
    let defaults: Vec<String> = figs
        .iter()
        .zip(linked.iter().zip(&doubt))
        .map(|(f, (l, d))| match l.as_ref().or(d.as_ref()) {
            Some(g) => g.clone(),
            None if f.kind == "total" => default_name(f),
            None => {
                let base = default_name(f);
                let mut name = base.clone();
                let mut n = 2;
                while taken.contains(&name) {
                    name = format!("{base}_{n}");
                    n += 1;
                }
                taken.insert(name.clone());
                name
            }
        })
        .collect();

    let chosen: BTreeMap<&str, &str> = match picks {
        Some(p) => p
            .iter()
            .map(|p| (p.key.as_str(), p.global.trim()))
            .collect(),
        None => figs
            .iter()
            .zip(&linked)
            // A figure gone from the file starts unticked: setting a global
            // to 0 is never done unasked.
            .filter(|(f, l)| f.kind == "total" || (l.is_some() && f.kind != "missing"))
            .map(|(f, _)| (f.key.as_str(), ""))
            .collect(),
    };

    let mut problems = Vec::new();
    for key in chosen.keys() {
        if !figs.iter().any(|f| f.key == *key) {
            problems.push(format!("The file has no figure {key}."));
        }
    }
    let mut rows = Vec::with_capacity(figs.len());
    let mut used: BTreeMap<String, String> = BTreeMap::new();
    for (((f, l), d), note) in figs.iter().zip(&linked).zip(&defaults).zip(notes) {
        let pick = chosen.get(f.key.as_str());
        let global = match pick {
            Some(g) if !g.is_empty() => g.to_string(),
            _ => d.clone(),
        };
        let new = f.days.to_string();
        let current = b.globals.get(&global).map(|v| v.0.clone());
        let action = match &current {
            None => "add",
            Some(c) if c.trim() == new => "same",
            Some(_) => "update",
        };
        let current_value = current.as_ref().and_then(|c| {
            let v = values.get(&global)?.to_string();
            (v != c.trim()).then_some(v)
        });
        let is_linked = l.as_deref() == Some(global.as_str());
        let mut problem = None;
        if pick.is_some() {
            if !method_core::expr::is_ident(&global) {
                problem = Some(format!(
                    "{global:?} can't be a global name: use letters, digits and _, with at least one letter or _."
                ));
            } else if let Some(other) = used.get(&global) {
                problem = Some(format!("{other} goes to {global} too."));
            }
            used.insert(global.clone(), f.label.clone());
            if let Some(p) = &problem {
                problems.push(format!("{}: {p}", f.label));
            }
        }
        rows.push(FigureRow {
            key: f.key.clone(),
            kind: f.kind,
            name: f.name.clone(),
            category: f.category.clone(),
            label: f.label.clone(),
            days: f.days,
            changed: is_linked && action != "same",
            global,
            picked: pick.is_some(),
            linked: is_linked,
            current,
            current_value,
            new,
            action,
            problem,
            note,
        });
    }
    let picked = || rows.iter().filter(|r| r.picked);
    let adds = picked().filter(|r| r.action == "add").count();
    let updates = picked().filter(|r| r.action == "update").count();
    let same = picked().filter(|r| r.action == "same").count();
    let changed = rows
        .iter()
        .filter(|r| r.changed)
        .map(|r| r.global.clone())
        .collect();
    let can_apply = problems.is_empty() && picked().any(|r| r.action != "same" || !r.linked);
    Review {
        token: 0,
        file: String::new(),
        schedule: days.schedule.clone(),
        board: days.board.clone(),
        sub_board: days.sub_board.clone(),
        generated: days.generated.clone(),
        version: days.version,
        source_hash: days.source_hash.clone(),
        days_hash: days.days_hash.clone(),
        shoot_days: days.shoot_days.total,
        start: days.shoot_days.start.clone(),
        end: days.shoot_days.end.clone(),
        notes: days.notes.clone(),
        figures: rows,
        adds,
        updates,
        same,
        changed,
        problems,
        can_apply,
    }
}

/// The picks a review starts with, plus `add` (a figure's key, and the
/// global to use or `None` for its default) and less `skip` (keys): what
/// the command line's `--pick` and `--skip` and the MCP tool's `pick` and
/// `skip` mean. A key the file doesn't have is refused.
pub fn picks_with(
    b: &Budget,
    days: &ScheduleDays,
    add: &[(String, Option<String>)],
    skip: &[String],
) -> Result<Vec<Pick>, String> {
    let start = review(b, days, None);
    let has = |k: &str| start.figures.iter().any(|f| f.key == k);
    for k in add.iter().map(|(k, _)| k).chain(skip) {
        if !has(k) {
            return Err(format!(
                "the file has no figure {k:?} (keys are total, unit:NAME, set:NAME, location:NAME, week:YYYY-MM-DD and element:ID, as the review lists them)"
            ));
        }
    }
    let mut out: Vec<Pick> = start
        .figures
        .iter()
        .filter(|f| f.picked && !skip.contains(&f.key))
        .map(|f| Pick {
            key: f.key.clone(),
            global: String::new(),
        })
        .collect();
    for (key, global) in add {
        let global = global.clone().unwrap_or_default();
        match out.iter_mut().find(|p| &p.key == key) {
            Some(p) => p.global = global,
            None => out.push(Pick {
                key: key.clone(),
                global,
            }),
        }
    }
    Ok(out)
}

/// What [`apply`] did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Applied {
    /// Globals made.
    pub added: Vec<String>,
    /// Globals given a new value.
    pub updated: Vec<String>,
    /// Globals already at their value (now linked to the figure).
    pub same: Vec<String>,
}

impl Applied {
    /// The sentence an import ends with.
    pub fn message(&self) -> String {
        let set = self.added.len() + self.updated.len();
        if set == 0 {
            return format!(
                "The globals already match the schedule; {} now linked to it.",
                count(self.same.len(), "global is", "globals are")
            );
        }
        let mut s = format!("Set {} from the schedule", count(set, "global", "globals"));
        if !self.added.is_empty() {
            s.push_str(&format!(" ({} new)", self.added.len()));
        }
        s.push('.');
        if !self.same.is_empty() {
            s.push_str(&format!(
                " {} already matched.",
                count(self.same.len(), "global", "globals")
            ));
        }
        s
    }
}

/// Sets the picked globals in `b` as the review says, with each one's setup
/// source naming the figure. A new global gets the figure as its
/// description (and the Day unit when the budget has one). Refused, with
/// `b` unchanged, when the review has a problem or a global can't take the
/// value.
pub fn apply(
    b: &mut Budget,
    days: &ScheduleDays,
    picks: Option<&[Pick]>,
) -> Result<Applied, String> {
    let r = review(b, days, picks);
    if let Some(p) = r.problems.first() {
        return Err(p.clone());
    }
    let mut next = b.clone();
    let mut done = Applied::default();
    let figs = figures(days);
    for row in r.figures.iter().filter(|r| r.picked) {
        if row.action != "same" {
            crate::edit::set_global(&mut next, &row.global, &row.new)
                .map_err(|e| format!("{}: {e}", row.label))?;
        }
        if row.action == "add" {
            let info = next.global_info.entry(row.global.clone()).or_default();
            info.description = row.label.clone();
            if next.units.contains_key(DAY_UNIT) {
                info.unit = DAY_UNIT.to_string();
            }
        }
        let mut src = next
            .setup_sources
            .get(GLOBAL_KIND, &row.global)
            .cloned()
            .unwrap_or_default();
        src.notes = match figs.iter().find(|f| f.key == row.key) {
            Some(f) => source_note(days, f),
            // A gone figure keeps the words it was set by.
            None => next
                .setup_sources
                .get(GLOBAL_KIND, &row.global)
                .map(|s| s.notes.clone())
                .unwrap_or_default(),
        };
        // A figure from a schedule isn't a pack's: what a pack said about
        // this global no longer holds.
        src.pack.clear();
        src.pack_version.clear();
        src.effective.clear();
        next.setup_sources.set(GLOBAL_KIND, &row.global, src);
        match row.action {
            "add" => done.added.push(row.global.clone()),
            "update" => done.updated.push(row.global.clone()),
            _ => done.same.push(row.global.clone()),
        }
    }
    if done.added.is_empty() && done.updated.is_empty() && done.same.is_empty() {
        return Err("Nothing is picked to set.".into());
    }
    *b = next;
    Ok(done)
}

/// Reads a picked schedule days file, `bytes` from a file named `file`.
pub fn read(file: &str, bytes: &[u8]) -> Result<ScheduleDays, String> {
    if !schedule::looks_like(bytes) {
        return Err(format!(
            "Couldn't read {file}: it isn't a schedule days file (Method Schedule writes one with File > Export for Budget)."
        ));
    }
    schedule::read(bytes).map_err(|e| format!("Couldn't read {file}: {e}."))
}

/// The undo step's name.
fn step_label(days: &ScheduleDays) -> String {
    format!("Set globals from the schedule {}", days.schedule.trim())
        .trim_end()
        .to_string()
}

/// Applies to the budget in `s`, as one undo step. The message saying what
/// changed.
pub fn apply_to(
    s: &mut Session,
    days: &ScheduleDays,
    picks: Option<&[Pick]>,
) -> Result<String, String> {
    let done = s.edit(step_label(days), |b| apply(b, days, picks))?;
    Ok(done.message())
}

// ---------------------------------------------------------------------------
// Commands (the desktop's src/schedule_days.rs and the host's
// src/host/schedule_days.rs call these)

/// What `schedule_days_apply` returns.
#[derive(Debug, Serialize)]
pub struct Imported {
    pub view: crate::view::View,
    pub message: String,
}

/// Reads a picked schedule days file, keeps it and returns the review
/// against budget `b` with the default picks.
pub fn schedule_days_pick(
    b: &Budget,
    sources: &ScheduleSources,
    file: String,
    bytes: &[u8],
) -> Result<Review, String> {
    let days = read(&file, bytes)?;
    let token = sources.keep(file.clone(), days);
    schedule_days_review(b, sources, token, None)
}

/// The review of the kept file against budget `b` with `picks` (`None`:
/// the defaults). Changes nothing.
pub fn schedule_days_review(
    b: &Budget,
    sources: &ScheduleSources,
    token: u64,
    picks: Option<&[Pick]>,
) -> Result<Review, String> {
    let (file, days) = sources.get(token)?;
    let mut r = review(b, &days, picks);
    r.token = token;
    r.file = file;
    Ok(r)
}

/// Sets the picked globals in budget `doc` as one undo step. The budget,
/// and the message saying what changed.
pub fn schedule_days_apply(
    state: &AppState,
    sources: &ScheduleSources,
    doc: Option<DocId>,
    token: u64,
    picks: Option<&[Pick]>,
) -> Result<(DocId, String), String> {
    let (_, days) = sources.get(token)?;
    let mut s = state.lock_doc(doc)?;
    let message = apply_to(&mut s, &days, picks)?;
    Ok((s.id(), message))
}

#[cfg(test)]
mod tests {
    use super::*;
    use method_core::Formula;

    /// Invented: a small schedule with two locations, two sets, a week and
    /// two elements.
    const DAYS: &str = r#"{
  "format": "method-schedule-days",
  "version": 1,
  "schedule": "Kettle Lake",
  "board": "Board 1",
  "sub_board": "Main",
  "source_hash": "0123456789abcdef",
  "shoot_days": {
    "start": "2027-03-01",
    "end": "2027-03-05",
    "total": 5,
    "by_unit": {},
    "by_set": {"BOATHOUSE": 3, "DOCK": 2},
    "by_location": {"North shore": 3, "Pine Hollow camp": 2},
    "by_week": {"2027-03-01": 5}
  },
  "elements": [
    {"id": 7, "category": "Cast", "board_id": "1", "name": "WREN",
     "work": 4, "hold": 1, "travel": 0, "fitting": 0, "rehearsal": 0, "holiday": 0,
     "company_move": 0, "total": 5, "holds": true, "drops": false, "min_drop_gap": 1},
    {"id": 9, "category": "Props", "name": "CANOE",
     "work": 2, "hold": 0, "travel": 0, "fitting": 0, "rehearsal": 0, "holiday": 0,
     "company_move": 0, "total": 2, "holds": false, "drops": false, "min_drop_gap": 1}
  ]
}"#;

    fn days() -> ScheduleDays {
        read("kettle.json", DAYS.as_bytes()).unwrap()
    }

    fn budget() -> Budget {
        let mut b = method_core::starter::budget();
        b.globals.insert("PREP_DAYS".into(), Formula("10".into()));
        b.globals
            .insert("NORTH_SHORE_DAYS".into(), Formula("2".into()));
        b
    }

    fn row<'a>(r: &'a Review, key: &str) -> &'a FigureRow {
        r.figures.iter().find(|f| f.key == key).unwrap()
    }

    fn pick(key: &str, global: &str) -> Pick {
        Pick {
            key: key.into(),
            global: global.into(),
        }
    }

    #[test]
    fn every_figure_is_offered_with_a_default_global() {
        let r = review(&budget(), &days(), None);
        let keys: Vec<&str> = r.figures.iter().map(|f| f.key.as_str()).collect();
        assert_eq!(
            keys,
            [
                "total",
                "set:BOATHOUSE",
                "set:DOCK",
                "location:North shore",
                "location:Pine Hollow camp",
                "week:2027-03-01",
                "element:7",
                "element:9"
            ]
        );
        let names: Vec<&str> = r.figures.iter().map(|f| f.global.as_str()).collect();
        assert_eq!(
            names,
            [
                "SHOOT_DAYS",
                "BOATHOUSE_DAYS",
                "DOCK_DAYS",
                // NORTH_SHORE_DAYS is the budget's own: never a default.
                "NORTH_SHORE_DAYS_2",
                "PINE_HOLLOW_CAMP_DAYS",
                "WEEK_2027_03_01_DAYS",
                "WREN_DAYS",
                "CANOE_DAYS"
            ]
        );
        assert_eq!(row(&r, "element:7").label, "Days of Cast 1 WREN");
        // A performer's days are the day out of days total, holds included.
        assert_eq!(row(&r, "element:7").days, 5);
        assert_eq!(
            row(&r, "location:North shore").label,
            "Shoot days at location North shore"
        );
    }

    #[test]
    fn only_the_total_is_picked_to_start() {
        let r = review(&budget(), &days(), None);
        let picked: Vec<&str> = r
            .figures
            .iter()
            .filter(|f| f.picked)
            .map(|f| f.global.as_str())
            .collect();
        // NORTH_SHORE_DAYS has the default name but no link: not picked.
        assert_eq!(picked, ["SHOOT_DAYS"]);
        let total = row(&r, "total");
        assert_eq!(
            (total.current.as_deref(), total.new.as_str(), total.action),
            (Some("25"), "5", "update")
        );
        let north = row(&r, "location:North shore");
        assert_eq!(
            (
                north.global.as_str(),
                north.action,
                north.days,
                north.picked
            ),
            ("NORTH_SHORE_DAYS_2", "add", 3, false)
        );
        // Prep and wrap are never touched.
        assert!(r.figures.iter().all(|f| !f.global.starts_with("PREP")));
        assert_eq!((r.adds, r.updates, r.same), (0, 1, 0));
        assert!(r.can_apply);
        assert!(r.problems.is_empty());
    }

    #[test]
    fn a_picked_figure_can_go_to_any_global() {
        let b = budget();
        let picks = [
            pick("total", ""),
            pick("location:Pine Hollow camp", "CAMP_DAYS"),
            pick("element:7", ""),
        ];
        let r = review(&b, &days(), Some(&picks));
        let camp = row(&r, "location:Pine Hollow camp");
        assert_eq!(
            (camp.global.as_str(), camp.picked, camp.action),
            ("CAMP_DAYS", true, "add")
        );
        assert!(!row(&r, "location:North shore").picked);
        assert_eq!(row(&r, "element:7").global, "WREN_DAYS");
        assert_eq!((r.adds, r.updates), (2, 1));
    }

    #[test]
    fn bad_names_and_two_figures_in_one_global_are_refused() {
        let b = budget();
        let picks = [
            pick("total", ""),
            pick("set:DOCK", "SHOOT_DAYS"),
            pick("set:BOATHOUSE", "12"),
        ];
        let r = review(&b, &days(), Some(&picks));
        assert_eq!(r.problems.len(), 2, "{:?}", r.problems);
        assert!(r.problems[0].contains("can't be a global name"));
        assert!(r.problems[1].contains("goes to SHOOT_DAYS too"));
        assert!(!r.can_apply);
        let mut b2 = b.clone();
        assert!(apply(&mut b2, &days(), Some(&picks)).is_err());
        assert_eq!(b2, b);
        let r = review(&b, &days(), Some(&[pick("nope", "")]));
        assert!(r.problems[0].contains("no figure nope"));
    }

    #[test]
    fn applying_sets_values_descriptions_and_sources() {
        let mut b = budget();
        let picks = [
            pick("total", ""),
            pick("location:Pine Hollow camp", "CAMP_DAYS"),
        ];
        let done = apply(&mut b, &days(), Some(&picks)).unwrap();
        assert_eq!(done.added, ["CAMP_DAYS"]);
        assert_eq!(done.updated, ["SHOOT_DAYS"]);
        assert_eq!(done.message(), "Set 2 globals from the schedule (1 new).");
        assert_eq!(b.globals["SHOOT_DAYS"].0, "5");
        assert_eq!(b.globals["CAMP_DAYS"].0, "2");
        // Untouched: prep, and a figure not picked.
        assert_eq!(b.globals["PREP_DAYS"].0, "10");
        assert_eq!(b.globals["NORTH_SHORE_DAYS"].0, "2");
        let info = &b.global_info["CAMP_DAYS"];
        assert_eq!(info.description, "Shoot days at location Pine Hollow camp");
        assert_eq!(info.unit, "Day");
        assert_eq!(
            b.setup_sources.get("global", "CAMP_DAYS").unwrap().notes,
            "Shoot days at location Pine Hollow camp, from the schedule Kettle Lake (Board 1, Main) [key: location:Pine Hollow camp]"
        );
        let picks = [pick("total", ""), pick("element:7", "")];
        apply(&mut b, &days(), Some(&picks)).unwrap();
        assert_eq!(
            b.setup_sources.get("global", "WREN_DAYS").unwrap().notes,
            "Days of Cast 1 WREN, from the schedule Kettle Lake (Board 1, Main) [key: element:7 | Cast | 1 | WREN]"
        );
        // An existing global keeps its description.
        assert!(!b
            .global_info
            .get("SHOOT_DAYS")
            .is_some_and(|i| i.description.starts_with("Shoot days in all")));
        // Totals still work out.
        assert!(recalc(&b, &RecalcOptions::default()).is_ok());
    }

    #[test]
    fn a_later_import_finds_its_globals_and_says_what_changed() {
        let mut b = budget();
        let picks = [
            pick("total", ""),
            pick("location:Pine Hollow camp", "CAMP_DAYS"),
        ];
        apply(&mut b, &days(), Some(&picks)).unwrap();
        // The same file again: picked again by the link, nothing to change.
        let r = review(&b, &days(), None);
        let camp = row(&r, "location:Pine Hollow camp");
        assert!(camp.picked && camp.linked);
        assert_eq!((camp.global.as_str(), camp.action), ("CAMP_DAYS", "same"));
        assert_eq!(row(&r, "total").action, "same");
        assert!(!r.can_apply);
        assert!(r.changed.is_empty());
        // The schedule moved a day to the camp.
        let moved = DAYS.replace("\"Pine Hollow camp\": 2", "\"Pine Hollow camp\": 3");
        let d2 = read("kettle.json", moved.as_bytes()).unwrap();
        let r = review(&b, &d2, None);
        assert_eq!(r.changed, ["CAMP_DAYS"]);
        assert!(row(&r, "location:Pine Hollow camp").changed);
        assert!(r.can_apply);
    }

    #[test]
    fn a_figure_gone_from_the_file_starts_unticked() {
        let mut b = budget();
        let picks = [
            pick("total", ""),
            pick("element:9", ""),
            pick("location:Pine Hollow camp", "CAMP_DAYS"),
        ];
        apply(&mut b, &days(), Some(&picks)).unwrap();
        assert_eq!(b.globals["CANOE_DAYS"].0, "2");
        // The canoe is cut (another id and name) and the camp dropped.
        let cut = DAYS
            .replace(
                "\"id\": 9, \"category\": \"Props\", \"name\": \"CANOE\"",
                "\"id\": 12, \"category\": \"Props\", \"name\": \"KAYAK\"",
            )
            .replace("\"Pine Hollow camp\": 2", "\"South shore\": 2");
        let d2 = read("kettle.json", cut.as_bytes()).unwrap();
        let r = review(&b, &d2, None);
        let gone = row(&r, "linked:CANOE_DAYS");
        assert_eq!(
            (
                gone.kind,
                gone.days,
                gone.picked,
                gone.action,
                gone.label.as_str()
            ),
            ("missing", 0, false, "update", "Days of Props CANOE")
        );
        assert!(gone
            .note
            .as_deref()
            .unwrap()
            .contains("Not in this schedule file"));
        assert!(!row(&r, "linked:CAMP_DAYS").picked);
        // Applying unattended leaves both alone.
        apply(&mut b, &d2, None).unwrap();
        assert_eq!(b.globals["CANOE_DAYS"].0, "2");
        assert_eq!(b.globals["CAMP_DAYS"].0, "2");
        // Only an explicit pick sets one to 0.
        let p = picks_with(&b, &d2, &[("linked:CANOE_DAYS".into(), None)], &[]).unwrap();
        apply(&mut b, &d2, Some(&p)).unwrap();
        assert_eq!(b.globals["CANOE_DAYS"].0, "0");
        assert_eq!(b.globals["CAMP_DAYS"].0, "2");
        // Another sub-board's links are not this file's to clear.
        let other = DAYS.replace("\"Main\"", "\"Second unit\"");
        let d3 = read("kettle-2.json", other.as_bytes()).unwrap();
        let r = review(&b, &d3, None);
        assert!(r.figures.iter().all(|f| f.kind != "missing"));
    }

    #[test]
    fn a_renamed_or_renumbered_element_keeps_its_link() {
        let mut b = budget();
        let picks = [
            pick("total", ""),
            pick("element:7", "WREN_DAYS"),
            pick("element:9", ""),
        ];
        apply(&mut b, &days(), Some(&picks)).unwrap();
        let linked = |text: &str| {
            let d = read("kettle.json", text.as_bytes()).unwrap();
            let r = review(&b, &d, None);
            assert!(r.figures.iter().all(|f| f.kind != "missing"), "{text}");
            let w = row(&r, "element:7").clone();
            let c = row(&r, "element:9").clone();
            (w.global, w.picked, w.linked, c.global, c.picked, w.note)
        };
        // Renamed (cast and a prop without a board ID), and renumbered.
        let renamed = DAYS
            .replace("\"WREN\"", "\"WRENNA\"")
            .replace("\"CANOE\"", "\"KAYAK\"");
        assert_eq!(
            linked(&renamed),
            (
                "WREN_DAYS".into(),
                true,
                true,
                "CANOE_DAYS".into(),
                true,
                None
            )
        );
        let renumbered = DAYS.replace("\"board_id\": \"1\"", "\"board_id\": \"4\"");
        assert_eq!(linked(&renumbered).0, "WREN_DAYS");
        assert!(linked(&renumbered).1);
        // Both changed: the id alone, so ask (unticked, with a note).
        let both = renumbered.replace("\"WREN\"", "\"HERON\"");
        let (global, picked, is_linked, _, _, note) = linked(&both);
        assert_eq!(
            (global.as_str(), picked, is_linked),
            ("WREN_DAYS", false, false)
        );
        assert!(note.unwrap().contains("Tick it to keep the link"));
        // A renamed set has no id: its link goes, and it starts unticked.
        let set = DAYS.replace("\"DOCK\": 2", "\"JETTY\": 2");
        let picks = [pick("total", ""), pick("set:DOCK", "")];
        let mut b2 = budget();
        apply(&mut b2, &days(), Some(&picks)).unwrap();
        let r = review(&b2, &read("k.json", set.as_bytes()).unwrap(), None);
        assert!(!row(&r, "linked:DOCK_DAYS").picked);
    }

    #[test]
    fn a_link_recorded_by_its_words_only_is_found_and_said() {
        let mut b = budget();
        b.globals.insert("CAMP".into(), Formula("1".into()));
        b.setup_sources.set(
            "global",
            "CAMP",
            method_core::pack::ItemSource {
                notes: "Shoot days at location Pine Hollow camp, from the schedule Kettle Lake (Board 1, Main)".into(),
                ..Default::default()
            },
        );
        let r = review(&b, &days(), None);
        let camp = row(&r, "location:Pine Hollow camp");
        assert_eq!(
            (camp.global.as_str(), camp.picked, camp.linked),
            ("CAMP", true, true)
        );
        assert!(camp.note.as_deref().unwrap().contains("description only"));
        // Setting it again records its key.
        apply(&mut b, &days(), None).unwrap();
        assert!(b
            .setup_sources
            .get("global", "CAMP")
            .unwrap()
            .notes
            .ends_with("[key: location:Pine Hollow camp]"));
        assert!(row(&review(&b, &days(), None), "location:Pine Hollow camp")
            .note
            .is_none());
    }

    #[test]
    fn default_names_are_tidy_and_never_clash() {
        assert_eq!(
            slug("Cold Spring field station"),
            "COLD_SPRING_FIELD_STATION"
        );
        assert_eq!(slug("  INT. KITCHEN / NIGHT "), "INT_KITCHEN_NIGHT");
        assert_eq!(slug("Café"), "CAF");
        assert_eq!(slug("***"), "");
        assert!(slug(&"x".repeat(100)).len() <= MAX_SLUG);
        let clash = DAYS.replace("\"DOCK\": 2", "\"North shore\": 2");
        let d = read("kettle.json", clash.as_bytes()).unwrap();
        let r = review(&budget(), &d, None);
        // NORTH_SHORE_DAYS is the budget's own, so both get free names.
        assert_eq!(row(&r, "set:North shore").global, "NORTH_SHORE_DAYS_2");
        assert_eq!(row(&r, "location:North shore").global, "NORTH_SHORE_DAYS_3");
    }

    #[test]
    fn picks_from_the_command_line() {
        let b = budget();
        let p = picks_with(
            &b,
            &days(),
            &[
                ("location:Pine Hollow camp".into(), Some("CAMP_DAYS".into())),
                ("total".into(), Some("DAYS".into())),
            ],
            &["location:North shore".into()],
        )
        .unwrap();
        assert_eq!(
            p,
            [
                pick("total", "DAYS"),
                pick("location:Pine Hollow camp", "CAMP_DAYS")
            ]
        );
        let p = picks_with(&b, &days(), &[], &["total".into()]).unwrap();
        assert!(p.is_empty());
        assert!(picks_with(&b, &days(), &[("set:NOPE".into(), None)], &[]).is_err());
    }

    #[test]
    fn files_that_arent_schedule_days_are_refused_in_words() {
        let e = read("notes.json", b"{\"hello\": 1}").unwrap_err();
        assert!(e.contains("isn't a schedule days file"), "{e}");
        let e = read(
            "bad.json",
            DAYS.replace("\"total\": 5,", "\"total\": -5,").as_bytes(),
        )
        .unwrap_err();
        assert!(e.starts_with("Couldn't read bad.json"), "{e}");
    }

    #[test]
    fn applying_is_one_undo_step_through_the_kept_file() {
        let state = AppState::default();
        state.docs().add(Session::with_budget(budget(), None));
        let sources = ScheduleSources::default();
        let b = state.lock_doc(None).unwrap().budget().clone();
        let r = schedule_days_pick(&b, &sources, "kettle.json".into(), DAYS.as_bytes()).unwrap();
        assert_eq!(
            (r.token, r.file.as_str(), r.shoot_days),
            (1, "kettle.json", 5)
        );
        let (_, msg) = schedule_days_apply(&state, &sources, None, r.token, None).unwrap();
        assert_eq!(msg, "Set 1 global from the schedule.");
        let mut s = state.lock_doc(None).unwrap();
        assert_eq!(s.budget().globals["SHOOT_DAYS"].0, "5");
        s.undo();
        assert_eq!(s.budget().globals["SHOOT_DAYS"].0, "25");
        assert_eq!(s.budget(), &budget());
        drop(s);
        sources.keep("other.json".into(), days());
        assert!(schedule_days_review(&b, &sources, r.token, None).is_err());
    }

    #[test]
    fn the_field_station_budget_already_agrees_with_its_schedule() {
        // The suite's invented production: its budget's day globals were
        // made from this file (scripts/make-field-station.py), under names
        // of its own for the two locations.
        let d = read(
            "field-station.budget.json",
            include_bytes!("../../../fixtures/field-station/field-station.budget.json"),
        )
        .unwrap();
        let b = method_format::from_str(include_str!(
            "../../../fixtures/field-station/field-station.mthd"
        ))
        .unwrap();
        let r = review(&b, &d, None);
        let picked: Vec<&str> = r
            .figures
            .iter()
            .filter(|f| f.picked)
            .map(|f| f.global.as_str())
            .collect();
        assert_eq!(picked, ["SHOOT_DAYS"]);
        assert_eq!(row(&r, "total").action, "same");
        let picks = [
            pick("total", ""),
            pick("location:Cold Spring field station", "STATION_DAYS"),
            pick("location:Hollins Ridge fire road", "RIDGE_DAYS"),
        ];
        let r = review(&b, &d, Some(&picks));
        assert!(r
            .figures
            .iter()
            .filter(|f| f.picked)
            .all(|f| f.action == "same"));
        assert_eq!((r.adds, r.updates, r.same), (0, 0, 3));
        // Applying links them; nothing else changes.
        let mut after = b.clone();
        let done = apply(&mut after, &d, Some(&picks)).unwrap();
        assert_eq!(
            done.message(),
            "The globals already match the schedule; 3 globals are now linked to it."
        );
        assert_eq!(after.globals, b.globals);
        let again = review(&after, &d, None);
        let picked: Vec<&str> = again
            .figures
            .iter()
            .filter(|f| f.picked)
            .map(|f| f.global.as_str())
            .collect();
        assert_eq!(picked, ["SHOOT_DAYS", "STATION_DAYS", "RIDGE_DAYS"]);
        assert!(!again.can_apply);
    }

    #[test]
    fn default_names_never_land_on_an_existing_global() {
        // Field Station: RIDGE_DAYS is the fire road location's 2 days. The
        // RIDGE set's default name would be RIDGE_DAYS too; it gets a free
        // one, and ticking the set leaves the fire road's global alone.
        let d = read(
            "field-station.budget.json",
            include_bytes!("../../../fixtures/field-station/field-station.budget.json"),
        )
        .unwrap();
        let mut b = method_format::from_str(include_str!(
            "../../../fixtures/field-station/field-station.mthd"
        ))
        .unwrap();
        let r = review(&b, &d, None);
        let ridge = row(&r, "set:RIDGE");
        assert_eq!(
            (ridge.global.as_str(), ridge.action),
            ("RIDGE_DAYS_2", "add")
        );
        let p = picks_with(&b, &d, &[("set:RIDGE".into(), None)], &[]).unwrap();
        apply(&mut b, &d, Some(&p)).unwrap();
        assert_eq!(b.globals["RIDGE_DAYS"].0, "2");
        assert_eq!(b.globals["RIDGE_DAYS_2"].0, "1");
        // Naming the existing global in the row is what updates it.
        let p = picks_with(
            &b,
            &d,
            &[("set:RIDGE ROAD".into(), Some("RIDGE_DAYS".into()))],
            &[],
        )
        .unwrap();
        let r = review(&b, &d, Some(&p));
        assert_eq!(row(&r, "set:RIDGE ROAD").action, "update");
    }

    #[test]
    fn a_link_matching_several_elements_asks_and_is_never_offered_at_zero() {
        let mut b = budget();
        apply(
            &mut b,
            &days(),
            Some(&[pick("total", ""), pick("element:9", "")]),
        )
        .unwrap();
        // The canoe's id is gone and two props are now named CANOE.
        let two = DAYS.replace("\"id\": 9,", "\"id\": 12,").replace(
            "\n  ]\n}",
            ",\n    {\"id\": 13, \"category\": \"Props\", \"name\": \"CANOE\",\n     \"work\": 1, \"hold\": 0, \"travel\": 0, \"fitting\": 0, \"rehearsal\": 0, \"holiday\": 0,\n     \"company_move\": 0, \"total\": 1, \"holds\": false, \"drops\": false, \"min_drop_gap\": 1}\n  ]\n}",
        );
        let d = read("kettle.json", two.as_bytes()).unwrap();
        let r = review(&b, &d, None);
        assert!(r.figures.iter().all(|f| f.kind != "missing"));
        for key in ["element:12", "element:13"] {
            let f = row(&r, key);
            assert_eq!(
                (f.global.as_str(), f.picked, f.linked),
                ("CANOE_DAYS", false, false),
                "{key}"
            );
            let note = f.note.as_deref().unwrap();
            assert!(note.contains("more than one figure"), "{note}");
            assert!(note.contains("pick"), "{note}");
            assert!(!note.contains(" 0"), "{note}");
        }
        // Applying unattended leaves it; picking one relinks it.
        apply(&mut b, &d, None).unwrap();
        assert_eq!(b.globals["CANOE_DAYS"].0, "2");
        let p = picks_with(&b, &d, &[("element:13".into(), None)], &[]).unwrap();
        apply(&mut b, &d, Some(&p)).unwrap();
        assert_eq!(b.globals["CANOE_DAYS"].0, "1");
        let r = review(&b, &d, None);
        assert!(row(&r, "element:13").linked && row(&r, "element:13").note.is_none());
    }

    #[test]
    fn two_globals_linked_to_one_figure_are_noted() {
        let mut b = budget();
        apply(
            &mut b,
            &days(),
            Some(&[pick("total", ""), pick("element:9", "")]),
        )
        .unwrap();
        let src = b.setup_sources.get("global", "CANOE_DAYS").unwrap().clone();
        b.globals.insert("BOATS".into(), Formula("2".into()));
        b.setup_sources.set("global", "BOATS", src);
        let r = review(&b, &days(), None);
        let canoe = row(&r, "element:9");
        let note = canoe.note.as_deref().unwrap();
        assert!(
            note.contains("BOATS") && note.contains("CANOE_DAYS"),
            "{note}"
        );
        // One global is ticked for it; the other is left as it is.
        assert!(canoe.picked);
        assert!(r.figures.iter().all(|f| f.kind != "missing"));
    }
}
