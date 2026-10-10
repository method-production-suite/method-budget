//! File > Import Setup From…: setup items picked from another `.fwb` file
//! (roadmap 1.20; decisions.md, entries 15, 23 and 29).
//!
//! - **The source.** Rust shows the Open dialog and reads the file (a file
//!   open in a tab is read from the tab, so its unsaved changes count). Paths
//!   never come from the page; it names the source by a token afterwards.
//! - **The review.** Every setup item in the source is listed by kind with
//!   how it stands here: new, the same, or different under the same name.
//!   Each item also lists what it needs (a fringe's flat unit and the globals
//!   in its rate, the globals a global names, a charge's excluded groups and
//!   currency, a credit's groups and excluded fringes, a tag preset's tags,
//!   what a line block's rows use, a report preset's groups), transitively,
//!   so the page can tick them along.
//! - **Importing** brings the picked items and everything they need. New
//!   items are added; items that differ keep this budget's version
//!   (decision 12) unless the user picks the imported one or "add it under a
//!   new name", which the imported items that refer to it then use. One undo
//!   step. The same dependency walk, conflict rule, renaming and definitions
//!   in words as pasting rows (src/clipboard.rs), which this reuses.
//!
//! Everything but the commands at the end is plain functions on budgets, so
//! it is tested without a window.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use fwb_core::coding::AccountCoding;
use fwb_core::{
    Budget, Charge, ChargeKind, Credit, CreditCap, CreditSource, FringeKind, FringePosting,
    NewLineDefaults, ReportPreset, MAX_PRESETS, MAX_PRESET_NAME_CHARS,
};
use serde::{Deserialize, Serialize};
use tauri::{Runtime, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::app::{finish, AppState, DocId};
use crate::clipboard::{self, global_names, Kind, Renames, Setup, Status};
use crate::docs::Docs;
use crate::edit::EditResult;
use crate::view::View;

// ---------------------------------------------------------------------------
// What can be imported

/// The kinds of thing the review lists, in its order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum What {
    Global,
    Fringe,
    Group,
    Unit,
    Set,
    Location,
    Currency,
    Charge,
    Credit,
    TagPreset,
    LinePreset,
    ReportPreset,
    AccountCoding,
    FringePosting,
    NewLineDefaults,
}

impl What {
    pub const ALL: [What; 15] = [
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
        What::LinePreset,
        What::ReportPreset,
        What::AccountCoding,
        What::FringePosting,
        What::NewLineDefaults,
    ];

    pub fn key(self) -> &'static str {
        match self {
            What::Global => "global",
            What::Fringe => "fringe",
            What::Group => "group",
            What::Unit => "unit",
            What::Set => "set",
            What::Location => "location",
            What::Currency => "currency",
            What::Charge => "charge",
            What::Credit => "credit",
            What::TagPreset => "tag_preset",
            What::LinePreset => "line_preset",
            What::ReportPreset => "report_preset",
            What::AccountCoding => "account_coding",
            What::FringePosting => "fringe_posting",
            What::NewLineDefaults => "new_line_defaults",
        }
    }

    pub fn parse(s: &str) -> EditResult<What> {
        What::ALL
            .into_iter()
            .find(|w| w.key() == s)
            .ok_or_else(|| format!("unknown kind of setup item `{s}`"))
    }

    /// The setup-table kind rows refer to, for the seven tables pasting
    /// also handles.
    fn table(self) -> Option<Kind> {
        Some(match self {
            What::Global => Kind::Global,
            What::Fringe => Kind::Fringe,
            What::Group => Kind::Group,
            What::Unit => Kind::Unit,
            What::Set => Kind::Set,
            What::Location => Kind::Location,
            What::Currency => Kind::Currency,
            _ => return None,
        })
    }

    /// A budget setting: one per budget, no name, never new.
    pub fn is_setting(self) -> bool {
        matches!(
            self,
            What::AccountCoding | What::FringePosting | What::NewLineDefaults
        )
    }

    /// Whether "add it under a new name" makes sense (not for currency
    /// codes, which name real currencies, nor for settings).
    pub fn renamable(self) -> bool {
        match self.table() {
            Some(k) => k.renamable(),
            None => !self.is_setting(),
        }
    }
}

/// One item, as the page names it. Settings have an empty name.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemRef {
    pub kind: String,
    pub name: String,
}

pub(crate) type Key = (What, String);

pub(crate) fn item_ref(k: &Key) -> ItemRef {
    ItemRef {
        kind: k.0.key().to_string(),
        name: k.1.clone(),
    }
}

/// Every setup item in `b`, by kind, in review order.
pub(crate) fn all_items(b: &Budget) -> Vec<Key> {
    let mut out = Vec::new();
    for w in What::ALL {
        let names: Vec<String> = match w {
            What::Global => b.globals.keys().cloned().collect(),
            What::Fringe => b.fringes.keys().cloned().collect(),
            What::Group => b.groups.keys().cloned().collect(),
            What::Unit => b.units.keys().cloned().collect(),
            What::Set => b.sets.keys().cloned().collect(),
            What::Location => b.locations.keys().cloned().collect(),
            What::Currency => b.currencies.keys().cloned().collect(),
            What::Charge => b.charges.iter().map(|c| c.name.clone()).collect(),
            What::Credit => b.credits.iter().map(|c| c.name.clone()).collect(),
            What::TagPreset => b.tag_presets.iter().map(|p| p.name.clone()).collect(),
            What::LinePreset => b.line_presets.iter().map(|p| p.name.clone()).collect(),
            What::ReportPreset => b.report_presets.iter().map(|p| p.name.clone()).collect(),
            _ => vec![String::new()],
        };
        out.extend(names.into_iter().map(|n| (w, n)));
    }
    out
}

/// Whether `b` has an item of kind `w` named `name` (settings: always).
fn has(b: &Budget, w: What, name: &str) -> bool {
    match w {
        What::Global => b.globals.contains_key(name),
        What::Fringe => b.fringes.contains_key(name),
        What::Group => b.groups.contains_key(name),
        What::Unit => b.units.contains_key(name),
        What::Set => b.sets.contains_key(name),
        What::Location => b.locations.contains_key(name),
        What::Currency => b.currencies.contains_key(name),
        What::Charge => b.charges.iter().any(|c| c.name == name),
        What::Credit => b.credits.iter().any(|c| c.name == name),
        What::ReportPreset | What::TagPreset | What::LinePreset => {
            preset_index(b, w, name).is_some()
        }
        _ => true,
    }
}

/// Whether `w` names its items ignoring case and surrounding spaces (the
/// presets, whose names are kept unique that way).
fn folds_case(w: What) -> bool {
    matches!(w, What::ReportPreset | What::TagPreset | What::LinePreset)
}

/// The report, tag or line preset of kind `w` with this name, ignoring case
/// and surrounding spaces.
fn preset_index(b: &Budget, w: What, name: &str) -> Option<usize> {
    let key = name.trim().to_lowercase();
    let same = |n: &str| n.trim().to_lowercase() == key;
    match w {
        What::ReportPreset => b.report_presets.iter().position(|p| same(&p.name)),
        What::TagPreset => b.tag_presets.iter().position(|p| same(&p.name)),
        What::LinePreset => b.line_presets.iter().position(|p| same(&p.name)),
        _ => None,
    }
}

/// What item `name` of kind `w` in `b` needs directly: other items of `b`
/// it refers to by name. Names `b` doesn't define are left out.
fn needs(b: &Budget, w: What, name: &str) -> BTreeSet<Key> {
    let mut out: BTreeSet<Key> = BTreeSet::new();
    let mut add = |kind: What, n: &str| {
        if has(b, kind, n) {
            out.insert((kind, n.to_string()));
        }
    };
    match w {
        What::Global => {
            if let Some(f) = b.globals.get(name) {
                for g in global_names(&f.0) {
                    if g != name {
                        add(What::Global, &g);
                    }
                }
            }
        }
        What::Fringe => {
            if let Some(f) = b.fringes.get(name) {
                for g in global_names(&f.rate.0) {
                    add(What::Global, &g);
                }
                if let FringeKind::Flat { unit: Some(u) } = &f.kind {
                    add(What::Unit, u);
                }
            }
        }
        What::Charge => {
            if let Some(c) = b.charges.iter().find(|c| c.name == name) {
                for g in &c.excluded_groups {
                    add(What::Group, g);
                }
                if let ChargeKind::Flat {
                    currency: Some(cur),
                    ..
                } = &c.kind
                {
                    add(What::Currency, cur);
                }
            }
        }
        What::Credit => {
            if let Some(c) = b.credits.iter().find(|c| c.name == name) {
                for r in &c.rows {
                    if let CreditSource::Group {
                        group,
                        excluded_fringes,
                    } = &r.source
                    {
                        add(What::Group, group);
                        for f in excluded_fringes {
                            add(What::Fringe, f);
                        }
                    }
                }
            }
        }
        What::ReportPreset => {
            if let Some(i) = preset_index(b, w, name) {
                let spec = &b.report_presets[i].spec;
                for g in &spec.only_groups {
                    add(What::Group, g);
                }
                // A header or footer slot showing a global's value.
                for band in [&spec.header, &spec.footer].into_iter().flatten() {
                    for slot in [&band.left, &band.center, &band.right] {
                        if let Some(fwb_core::BandField::Global(g)) = slot {
                            add(What::Global, g);
                        }
                    }
                }
            }
        }
        What::TagPreset => {
            if let Some(i) = preset_index(b, w, name) {
                let t = &b.tag_presets[i];
                for f in &t.fringes {
                    add(What::Fringe, f);
                }
                for g in &t.groups {
                    add(What::Group, g);
                }
                if let Some(x) = &t.set {
                    add(What::Set, x);
                }
                if let Some(x) = &t.location {
                    add(What::Location, x);
                }
                if let Some(x) = &t.currency {
                    add(What::Currency, x);
                }
            }
        }
        What::LinePreset => {
            if let Some(i) = preset_index(b, w, name) {
                // What its rows use here, with what that uses in turn.
                let used = Setup::used_by(b, &b.line_presets[i].rows);
                for (kind, names) in [
                    (What::Fringe, used.fringes.keys().collect::<Vec<_>>()),
                    (What::Group, used.groups.keys().collect()),
                    (What::Global, used.globals.keys().collect()),
                    (What::Unit, used.units.keys().collect()),
                    (What::Set, used.sets.keys().collect()),
                    (What::Location, used.locations.keys().collect()),
                    (What::Currency, used.currencies.keys().collect()),
                ] {
                    for n in names {
                        add(kind, n);
                    }
                }
            }
        }
        _ => {}
    }
    out
}

/// `start` with everything it needs in `b`, however deep.
pub(crate) fn closure(b: &Budget, start: impl IntoIterator<Item = Key>) -> BTreeSet<Key> {
    let mut seen: BTreeSet<Key> = BTreeSet::new();
    let mut todo: Vec<Key> = start.into_iter().collect();
    while let Some(k) = todo.pop() {
        if !seen.insert(k.clone()) {
            continue;
        }
        todo.extend(needs(b, k.0, &k.1));
    }
    seen
}

// ---------------------------------------------------------------------------
// How each item stands here

/// A charge or credit compared without its place in the list.
fn status_of(src: &Budget, here: &Budget, w: What, name: &str) -> Status {
    match w {
        What::Global => clipboard::status(src.globals.get(name), here.globals.get(name)),
        What::Fringe => clipboard::status(src.fringes.get(name), here.fringes.get(name)),
        What::Group => clipboard::status(src.groups.get(name), here.groups.get(name)),
        What::Unit => clipboard::status(src.units.get(name), here.units.get(name)),
        What::Set => clipboard::status(src.sets.get(name), here.sets.get(name)),
        What::Location => clipboard::status(src.locations.get(name), here.locations.get(name)),
        What::Currency => clipboard::status(src.currencies.get(name), here.currencies.get(name)),
        What::Charge => clipboard::status(
            src.charges.iter().find(|c| c.name == name),
            here.charges.iter().find(|c| c.name == name),
        ),
        What::Credit => clipboard::status(
            src.credits.iter().find(|c| c.name == name),
            here.credits.iter().find(|c| c.name == name),
        ),
        // Presets compare by what they print, not their name's spelling or
        // which one is the default.
        What::ReportPreset => clipboard::status(
            preset_index(src, w, name).map(|i| &src.report_presets[i].spec),
            preset_index(here, w, name).map(|i| &here.report_presets[i].spec),
        ),
        // Tag presets and line blocks compare by what they hold.
        What::TagPreset => {
            fn tags(b: &Budget, name: &str) -> Option<fwb_core::TagPreset> {
                preset_index(b, What::TagPreset, name).map(|i| fwb_core::TagPreset {
                    name: String::new(),
                    ..b.tag_presets[i].clone()
                })
            }
            clipboard::status(tags(src, name).as_ref(), tags(here, name).as_ref())
        }
        What::LinePreset => {
            fn rows<'a>(b: &'a Budget, name: &str) -> Option<&'a Vec<fwb_core::Detail>> {
                preset_index(b, What::LinePreset, name).map(|i| &b.line_presets[i].rows)
            }
            clipboard::status(rows(src, name), rows(here, name))
        }
        What::AccountCoding => {
            clipboard::status(Some(&src.account_coding), Some(&here.account_coding))
        }
        What::FringePosting => {
            clipboard::status(Some(&src.fringe_posting), Some(&here.fringe_posting))
        }
        What::NewLineDefaults => {
            clipboard::status(Some(&src.new_line_defaults), Some(&here.new_line_defaults))
        }
    }
}

/// Item `name` of kind `w` in `b`, in words (empty when `b` has none).
pub(crate) fn describe(b: &Budget, w: What, name: &str) -> String {
    match w {
        What::Global => b.globals.get(name).map(clipboard::describe_global),
        What::Fringe => b.fringes.get(name).map(clipboard::describe_fringe),
        What::Group => b.groups.get(name).map(clipboard::describe_group),
        What::Unit => b.units.get(name).map(clipboard::describe_unit),
        What::Set => b.sets.get(name).map(clipboard::describe_set),
        What::Location => b.locations.get(name).map(clipboard::describe_location),
        What::Currency => b.currencies.get(name).map(clipboard::describe_currency),
        What::Charge => b
            .charges
            .iter()
            .find(|c| c.name == name)
            .map(describe_charge),
        What::Credit => b
            .credits
            .iter()
            .find(|c| c.name == name)
            .map(describe_credit),
        What::TagPreset => preset_index(b, w, name).map(|i| describe_tags(&b.tag_presets[i])),
        What::LinePreset => preset_index(b, w, name).map(|i| {
            let n = b.line_presets[i].rows.len();
            count(n, "row", "rows")
        }),
        What::ReportPreset => {
            preset_index(b, w, name).map(|i| describe_preset(&b.report_presets[i]))
        }
        What::AccountCoding => Some(describe_coding(&b.account_coding)),
        What::FringePosting => Some(describe_posting(&b.fringe_posting)),
        What::NewLineDefaults => Some(describe_line_defaults(&b.new_line_defaults)),
    }
    .unwrap_or_default()
}

fn join(parts: Vec<String>, fallback: &str) -> String {
    let kept: Vec<String> = parts.into_iter().filter(|p| !p.trim().is_empty()).collect();
    if kept.is_empty() {
        fallback.to_string()
    } else {
        kept.join("; ")
    }
}

fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn describe_charge(c: &Charge) -> String {
    let how = match &c.kind {
        ChargeKind::Percent(p) => format!("{p}% of the budget"),
        ChargeKind::Flat { amount, currency } => match currency {
            Some(cur) => format!("{} {cur} flat", clipboard::money(*amount)),
            None => format!("{} flat", clipboard::money(*amount)),
        },
    };
    let applied = if c.applied { "applied" } else { "not applied" }.to_string();
    let excluded = c.excluded_groups.len()
        + c.excluded_categories.len()
        + c.excluded_accounts.len()
        + c.excluded_amounts.len();
    let ex = if excluded > 0 {
        count(excluded, "exclusion", "exclusions")
    } else {
        String::new()
    };
    let account = c
        .account
        .as_deref()
        .map(|a| format!("account {a}"))
        .unwrap_or_default();
    join(vec![how, applied, ex, account], "")
}

fn describe_credit(c: &Credit) -> String {
    let rows = count(c.rows.len(), "row", "rows");
    let cap = c
        .cap
        .map(|x| format!("capped at {}", clipboard::money(x)))
        .unwrap_or_default();
    let discount = if c.discount_percent == fwb_core::Dec::ZERO {
        String::new()
    } else {
        format!("{}% discount", c.discount_percent)
    };
    let caps = c.rows.iter().filter(|r| r.cap != CreditCap::None).count();
    let row_caps = if caps > 0 {
        count(caps, "row cap", "row caps")
    } else {
        String::new()
    };
    let applied = if c.applied { "applied" } else { "not applied" }.to_string();
    join(vec![rows, cap, discount, row_caps, applied], "")
}

fn describe_tags(t: &fwb_core::TagPreset) -> String {
    let list = |what: &str, names: &BTreeSet<String>| {
        if names.is_empty() {
            String::new()
        } else {
            format!(
                "{what} {}",
                names.iter().cloned().collect::<Vec<_>>().join(", ")
            )
        }
    };
    let one = |what: &str, x: &Option<String>| {
        x.as_deref()
            .map(|n| format!("{what} {n}"))
            .unwrap_or_default()
    };
    join(
        vec![
            list("fringes", &t.fringes),
            list("groups", &t.groups),
            one("set", &t.set),
            one("location", &t.location),
            one("currency", &t.currency),
        ],
        "no tags",
    )
}

fn describe_preset(p: &ReportPreset) -> String {
    let sections: Vec<&str> = p.spec.sections.iter().map(|s| s.name()).collect();
    let layout = if p.spec.landscape {
        "landscape"
    } else {
        "portrait"
    };
    join(
        vec![
            if sections.is_empty() {
                "no sections".to_string()
            } else {
                sections.join(", ")
            },
            layout.to_string(),
        ],
        "",
    )
}

fn describe_coding(c: &AccountCoding) -> String {
    let sep = match c.separator {
        Some(ch) => format!("separator {ch}"),
        None => "no separator".to_string(),
    };
    format!(
        "category {} digits, account {}, sub-account {}, set {}; {sep}",
        c.category_width, c.account_width, c.sub_width, c.set_width
    )
}

fn describe_posting(p: &FringePosting) -> String {
    match p {
        FringePosting::Budget => "one Total Fringes row for the budget".to_string(),
        FringePosting::ProductionLevel => "a fringe row above each production total".to_string(),
        FringePosting::Category { suffix } => {
            format!("a fringe account in each category, numbered {suffix}")
        }
        FringePosting::Account => "a fringe line in each account".to_string(),
    }
}

fn describe_line_defaults(d: &NewLineDefaults) -> String {
    let mut on = Vec::new();
    for (yes, what) in [
        (d.fringes, "fringes"),
        (d.groups, "groups"),
        (d.set, "set"),
        (d.location, "location"),
        (d.currency, "currency"),
    ] {
        if yes {
            on.push(what);
        }
    }
    if on.is_empty() {
        "a new line copies nothing from its neighbor".to_string()
    } else {
        format!("a new line copies {} from its neighbor", on.join(", "))
    }
}

/// The name "add it under a new name" gives each item that differs (and
/// can be renamed): `Name (2)` (`NAME_2` for globals, which must stay names a
/// formula can use), the first free here and in the source. Report preset
/// names are compared ignoring case, and a name too long for one is not
/// offered.
fn proposed_names(here: &Budget, src: &Budget, items: &[(Key, Status)]) -> BTreeMap<Key, String> {
    let mut out = BTreeMap::new();
    let mut chosen: BTreeSet<(What, String)> = BTreeSet::new();
    for ((w, name), status) in items {
        if *status != Status::Differs || !w.renamable() {
            continue;
        }
        let w = *w;
        let fold = |n: &str| {
            if folds_case(w) {
                n.trim().to_lowercase()
            } else {
                n.to_string()
            }
        };
        let taken = |n: &str| has(here, w, n) || has(src, w, n) || chosen.contains(&(w, fold(n)));
        let base = name.trim();
        let Some(new) = (2u64..)
            .map(|n| match w {
                What::Global => format!("{base}_{n}"),
                _ => format!("{base} ({n})"),
            })
            .find(|c| !taken(c))
        else {
            continue;
        };
        let longest = match w {
            What::ReportPreset => MAX_PRESET_NAME_CHARS,
            What::TagPreset | What::LinePreset => fwb_core::MAX_TOOL_PRESET_NAME_CHARS,
            _ => usize::MAX,
        };
        if new.chars().count() > longest {
            continue;
        }
        chosen.insert((w, fold(&new)));
        out.insert((w, name.clone()), new);
    }
    out
}

/// How every item in `src` stands in `here`. A currency that is this
/// budget's base currency is left out: amounts in it count in the base
/// currency here.
fn statuses(here: &Budget, src: &Budget) -> Vec<(Key, Status)> {
    all_items(src)
        .into_iter()
        .filter(|(w, n)| !(*w == What::Currency && *n == here.base_currency))
        .map(|(w, n)| {
            let s = status_of(src, here, w, &n);
            ((w, n), s)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The review

/// One item in the review.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ImportItem {
    /// `global`, `fringe`, `group`, `unit`, `set`, `location`, `currency`,
    /// `charge`, `credit`, `tag_preset`, `line_preset`, `report_preset`,
    /// `account_coding`,
    /// `fringe_posting` or `new_line_defaults`.
    pub kind: &'static str,
    /// Empty for the three settings.
    pub name: String,
    /// `new` (not here: it will be added), `same` (nothing to do) or
    /// `differs` (here under the same name with another definition).
    pub status: &'static str,
    /// The imported definition, in words.
    pub imported: String,
    /// This budget's definition, in words (`differs` only).
    pub here: Option<String>,
    /// The name "add it under a new name" gives it, when offered.
    pub rename_to: Option<String>,
    /// Every other item in the source it needs, however deep: ticking it
    /// brings these too.
    pub needs: Vec<ItemRef>,
}

/// What importing from a budget would do, before anything is picked.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ImportPreview {
    /// Names the source; the import names it.
    pub token: u64,
    /// The source's file name without `.fwb`, or its tab's name.
    pub source: String,
    pub path: String,
    pub items: Vec<ImportItem>,
    /// Other things to know, as sentences.
    pub notes: Vec<String>,
}

fn status_key(s: Status) -> &'static str {
    match s {
        Status::Add => "new",
        Status::Same => "same",
        Status::Differs => "differs",
    }
}

/// The review for importing from `src` into `here`.
pub fn preview(here: &Budget, src: &Budget, token: u64, source: &str, path: &str) -> ImportPreview {
    let all = statuses(here, src);
    let proposed = proposed_names(here, src, &all);
    let items = all
        .iter()
        .map(|((w, name), status)| {
            let k = (*w, name.clone());
            let mut deps = closure(src, [k.clone()]);
            deps.remove(&k);
            deps.retain(|(dw, dn)| !(*dw == What::Currency && *dn == here.base_currency));
            ImportItem {
                kind: w.key(),
                name: name.clone(),
                status: status_key(*status),
                imported: describe(src, *w, name),
                here: (*status == Status::Differs).then(|| describe(here, *w, name)),
                rename_to: proposed.get(&k).cloned(),
                needs: deps.iter().map(item_ref).collect(),
            }
        })
        .collect();
    let mut notes = Vec::new();
    if src.base_currency != here.base_currency {
        notes.push(format!(
            "Flat charges in {from}, the base currency there, will count in {to}, this budget's base currency.",
            from = src.base_currency,
            to = here.base_currency,
        ));
    }
    ImportPreview {
        token,
        source: source.to_string(),
        path: path.to_string(),
        items,
        notes,
    }
}

// ---------------------------------------------------------------------------
// Importing

/// What the user chose for one item that differs.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resolution {
    pub kind: String,
    pub name: String,
    /// `keep` (this budget's version: the default), `imported` (the
    /// imported version replaces it) or `rename` (the imported version is
    /// added under the review's new name).
    pub choice: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Choice {
    Keep,
    Imported,
    Rename,
}

/// What an import did, counted.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Imported {
    /// Items added (new ones and ones added under a new name).
    pub added: usize,
    /// Items here replaced by the imported version (settings among them).
    pub replaced: usize,
    /// Items that differ and kept this budget's version.
    pub kept: usize,
}

impl Imported {
    pub fn message(&self) -> String {
        if self.added == 0 && self.replaced == 0 {
            return "Nothing to import: this budget already has what was picked.".to_string();
        }
        let mut parts = Vec::new();
        if self.added > 0 {
            parts.push(format!("{} added", count(self.added, "item", "items")));
        }
        if self.replaced > 0 {
            parts.push(format!(
                "{} replaced",
                count(self.replaced, "item", "items")
            ));
        }
        if self.kept > 0 {
            parts.push(format!(
                "{} kept as this budget had {}",
                count(self.kept, "item", "items"),
                if self.kept == 1 { "it" } else { "them" }
            ));
        }
        format!("Imported setup: {}.", parts.join(", "))
    }
}

/// Imports the `picks` from `src` into `b`, with everything they need, as
/// `resolutions` say for items that differ (an item not named there keeps
/// this budget's version). Everything is worked out before anything
/// changes, so a refusal leaves `b` as it was.
pub fn import(
    b: &mut Budget,
    src: &Budget,
    picks: &[ItemRef],
    resolutions: &[Resolution],
) -> EditResult<Imported> {
    let mut start = Vec::with_capacity(picks.len());
    for p in picks {
        let w = What::parse(&p.kind)?;
        let name = if w.is_setting() {
            String::new()
        } else {
            p.name.clone()
        };
        if !has(src, w, &name) {
            return Err(format!(
                "{} {} is not in that budget",
                w.key().replace('_', " "),
                name
            ));
        }
        // A preset is named as the source spells it.
        let name = match (w, preset_index(src, w, &name)) {
            (What::ReportPreset, Some(i)) => src.report_presets[i].name.clone(),
            (What::TagPreset, Some(i)) => src.tag_presets[i].name.clone(),
            (What::LinePreset, Some(i)) => src.line_presets[i].name.clone(),
            _ => name,
        };
        start.push((w, name));
    }
    let mut choices: BTreeMap<Key, Choice> = BTreeMap::new();
    for r in resolutions {
        let w = What::parse(&r.kind)?;
        let choice = match r.choice.as_str() {
            "keep" => Choice::Keep,
            "imported" => Choice::Imported,
            "rename" if w.renamable() => Choice::Rename,
            "rename" => {
                return Err(format!(
                    "a {} can't be added under a new name",
                    w.key().replace('_', " ")
                ))
            }
            other => return Err(format!("unknown choice `{other}`")),
        };
        let name = if w.is_setting() {
            String::new()
        } else {
            r.name.clone()
        };
        choices.insert((w, name), choice);
    }

    let wanted = closure(src, start);
    let all: Vec<(Key, Status)> = statuses(b, src)
        .into_iter()
        .filter(|(k, _)| wanted.contains(k))
        .collect();
    let proposed = proposed_names(b, src, &all);

    // Which items go in, under which names.
    let mut done = Imported::default();
    let mut renames = Renames::default();
    let mut writes: Vec<(What, String, String, Status)> = Vec::new();
    for ((w, name), status) in &all {
        let choice = match status {
            Status::Same => continue,
            Status::Add => Choice::Imported,
            Status::Differs => choices
                .get(&(*w, name.clone()))
                .copied()
                .unwrap_or(Choice::Keep),
        };
        match choice {
            Choice::Keep => done.kept += 1,
            Choice::Imported => writes.push((*w, name.clone(), name.clone(), *status)),
            Choice::Rename => {
                let to = proposed.get(&(*w, name.clone())).cloned().ok_or_else(|| {
                    format!(
                        "{} {name} can't be added under a new name",
                        w.key().replace('_', " ")
                    )
                })?;
                if let Some(map) = w.table().and_then(|k| renames.of(k)) {
                    map.insert(name.clone(), to.clone());
                }
                writes.push((*w, name.clone(), to, Status::Add));
            }
        }
    }
    let added = |kind: What| {
        writes
            .iter()
            .filter(|(w, _, _, s)| *w == kind && *s == Status::Add)
            .count()
    };
    for (kind, have, most, what) in [
        (
            What::ReportPreset,
            b.report_presets.len(),
            MAX_PRESETS,
            "report presets",
        ),
        (
            What::TagPreset,
            b.tag_presets.len(),
            fwb_core::MAX_TAG_PRESETS,
            "tag presets",
        ),
        (
            What::LinePreset,
            b.line_presets.len(),
            fwb_core::MAX_LINE_PRESETS,
            "line blocks",
        ),
    ] {
        if have + added(kind) > most {
            return Err(format!("A budget can keep at most {most} {what}."));
        }
    }

    // Nothing can fail from here on.
    let setup = Setup {
        fringes: src.fringes.clone(),
        groups: src.groups.clone(),
        globals: src.globals.clone(),
        units: src.units.clone(),
        sets: src.sets.clone(),
        locations: src.locations.clone(),
        currencies: src.currencies.clone(),
    };
    let base = b.base_currency.clone();
    for (w, from, to, status) in writes {
        match status {
            Status::Add => done.added += 1,
            _ => done.replaced += 1,
        }
        if let Some(kind) = w.table() {
            clipboard::write(b, &setup, kind, &from, &to, &renames);
            continue;
        }
        match w {
            What::Charge => {
                let Some(c) = src.charges.iter().find(|c| c.name == from) else {
                    continue;
                };
                let mut c = c.clone();
                c.name = to.clone();
                c.excluded_groups = c
                    .excluded_groups
                    .iter()
                    .map(|g| clipboard::renamed(&renames.groups, g))
                    .collect();
                if let ChargeKind::Flat { currency, .. } = &mut c.kind {
                    if currency.as_deref() == Some(base.as_str()) {
                        *currency = None;
                    }
                }
                match b.charges.iter().position(|x| x.name == to) {
                    Some(i) => b.charges[i] = c,
                    None => b.charges.push(c),
                }
            }
            What::Credit => {
                let Some(c) = src.credits.iter().find(|c| c.name == from) else {
                    continue;
                };
                let mut c = c.clone();
                c.name = to.clone();
                for r in &mut c.rows {
                    if let CreditSource::Group {
                        group,
                        excluded_fringes,
                    } = &mut r.source
                    {
                        *group = clipboard::renamed(&renames.groups, group);
                        *excluded_fringes = excluded_fringes
                            .iter()
                            .map(|f| clipboard::renamed(&renames.fringes, f))
                            .collect();
                    }
                }
                match b.credits.iter().position(|x| x.name == to) {
                    Some(i) => b.credits[i] = c,
                    None => b.credits.push(c),
                }
            }
            What::TagPreset => {
                let Some(i) = preset_index(src, w, &from) else {
                    continue;
                };
                let mut t = src.tag_presets[i].clone();
                t.fringes = t
                    .fringes
                    .iter()
                    .map(|f| clipboard::renamed(&renames.fringes, f))
                    .collect();
                t.groups = t
                    .groups
                    .iter()
                    .map(|g| clipboard::renamed(&renames.groups, g))
                    .collect();
                t.set = t.set.map(|x| clipboard::renamed(&renames.sets, &x));
                t.location = t
                    .location
                    .map(|x| clipboard::renamed(&renames.locations, &x));
                if t.currency.as_deref() == Some(base.as_str()) {
                    t.currency = None;
                }
                match preset_index(b, w, &to) {
                    Some(j) => {
                        t.name = b.tag_presets[j].name.clone();
                        b.tag_presets[j] = t;
                    }
                    None => {
                        t.name = to.trim().to_string();
                        b.tag_presets.push(t);
                    }
                }
            }
            What::LinePreset => {
                let Some(i) = preset_index(src, w, &from) else {
                    continue;
                };
                let mut l = src.line_presets[i].clone();
                for d in &mut l.rows {
                    clipboard::rewrite(d, &renames, &base);
                }
                rename_setup_items(&mut l.setup, &renames);
                match preset_index(b, w, &to) {
                    Some(j) => {
                        l.name = b.line_presets[j].name.clone();
                        b.line_presets[j] = l;
                    }
                    None => {
                        l.name = to.trim().to_string();
                        b.line_presets.push(l);
                    }
                }
            }
            What::ReportPreset => {
                let Some(i) = preset_index(src, w, &from) else {
                    continue;
                };
                let mut p = src.report_presets[i].clone();
                p.spec.only_groups = p
                    .spec
                    .only_groups
                    .iter()
                    .map(|g| clipboard::renamed(&renames.groups, g))
                    .collect();
                let spec = &mut p.spec;
                for band in [&mut spec.header, &mut spec.footer].into_iter().flatten() {
                    for slot in [&mut band.left, &mut band.center, &mut band.right] {
                        if let Some(fwb_core::BandField::Global(g)) = slot {
                            *g = clipboard::renamed(&renames.globals, g);
                        }
                    }
                }
                match preset_index(b, w, &to) {
                    // Replacing keeps this budget's choice of default.
                    Some(j) => {
                        p.name = b.report_presets[j].name.clone();
                        p.default = b.report_presets[j].default;
                        b.report_presets[j] = p;
                    }
                    None => {
                        p.name = to.trim().to_string();
                        p.default = false;
                        b.report_presets.push(p);
                    }
                }
            }
            What::AccountCoding => b.account_coding = src.account_coding,
            What::FringePosting => b.fringe_posting = src.fringe_posting.clone(),
            What::NewLineDefaults => b.new_line_defaults = src.new_line_defaults,
            _ => {}
        }
    }
    Ok(done)
}

/// A line block's own copy of the setup its rows use, with the names the
/// import renamed (keys, and the names inside fringes and globals).
fn rename_setup_items(s: &mut fwb_core::SetupItems, r: &Renames) {
    fn keys<T>(m: &mut BTreeMap<String, T>, names: &BTreeMap<String, String>) {
        if names.is_empty() {
            return;
        }
        *m = std::mem::take(m)
            .into_iter()
            .map(|(k, v)| (clipboard::renamed(names, &k), v))
            .collect();
    }
    keys(&mut s.fringes, &r.fringes);
    keys(&mut s.groups, &r.groups);
    keys(&mut s.globals, &r.globals);
    keys(&mut s.units, &r.units);
    keys(&mut s.sets, &r.sets);
    keys(&mut s.locations, &r.locations);
    for f in s.fringes.values_mut() {
        f.rate.0 = clipboard::rename_globals(&f.rate.0, &r.globals);
        if let FringeKind::Flat { unit: Some(u) } = &mut f.kind {
            *u = clipboard::renamed(&r.units, u);
        }
    }
    for g in s.globals.values_mut() {
        g.0 = clipboard::rename_globals(&g.0, &r.globals);
    }
}

// ---------------------------------------------------------------------------
// The source, kept by token between the review and the import

struct Source {
    token: u64,
    label: String,
    budget: Arc<Budget>,
}

/// The last budget picked to import from (the app's state; one for every
/// budget).
#[derive(Default)]
pub struct ImportSources {
    inner: Mutex<(u64, Option<Source>)>,
}

impl ImportSources {
    fn lock(&self) -> MutexGuard<'_, (u64, Option<Source>)> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Keeps `budget` and returns its token.
    pub fn keep(&self, label: String, budget: Budget) -> u64 {
        let mut g = self.lock();
        g.0 += 1;
        let token = g.0;
        g.1 = Some(Source {
            token,
            label,
            budget: Arc::new(budget),
        });
        token
    }

    fn get(&self, token: u64) -> Option<(String, Arc<Budget>)> {
        let g = self.lock();
        g.1.as_ref()
            .filter(|s| s.token == token)
            .map(|s| (s.label.clone(), Arc::clone(&s.budget)))
    }
}

/// The budget in `path` and its name: from its tab when it is open (unsaved
/// changes count), else read from the file. Refused when it is budget
/// `target` itself.
pub fn read_source(docs: &Docs, target: DocId, path: &Path) -> Result<(String, Budget), String> {
    if let Some(doc) = docs.find_file(path) {
        if doc == target {
            return Err("That is this budget. Pick another file to import from.".into());
        }
        let s = docs.get(doc).ok_or("That budget has been closed.")?;
        return Ok((s.tab_label(), s.budget().clone()));
    }
    let (budget, _) = crate::session::read_budget(path)?;
    let label = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    Ok((label, budget))
}

// ---------------------------------------------------------------------------
// Commands (registered in app.rs, build.rs and capabilities/main.json; the
// tools window may call none of them)

/// Shows an Open dialog for the budget to import setup from, reads it and
/// returns the review for budget `doc` (`None`: the one in front). `None`
/// when cancelled.
#[tauri::command]
pub async fn import_setup_pick<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    sources: State<'_, ImportSources>,
    doc: Option<DocId>,
) -> Result<Option<ImportPreview>, String> {
    let (target, start) = {
        let s = state.lock_doc(doc)?;
        (s.id(), s.path().and_then(|p| p.parent()).map(PathBuf::from))
    };
    let mut dialog = window
        .dialog()
        .file()
        .set_title("Import setup from")
        .add_filter("Film Wizard budget", &["fwb"])
        .set_parent(&window);
    if let Some(dir) = start {
        dialog = dialog.set_directory(dir);
    }
    let Some(picked) = dialog.blocking_pick_file() else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(|e| e.to_string())?;
    let (label, budget) = read_source(&state.docs(), target, &path)?;
    let s = state.lock_doc(Some(target))?;
    let p = preview(s.budget(), &budget, 0, &label, &path.display().to_string());
    let token = sources.keep(label, budget);
    Ok(Some(ImportPreview { token, ..p }))
}

/// What `import_setup` did: the view, and a sentence saying so.
#[derive(Debug, Serialize)]
pub struct ImportDone {
    pub view: View,
    pub message: String,
    #[serde(flatten)]
    pub counts: Imported,
}

/// `{token, picks, resolutions}`: imports the picked items (and what they
/// need) from the reviewed source into budget `doc`, as one undo step.
#[tauri::command]
pub fn import_setup<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    sources: State<'_, ImportSources>,
    doc: Option<DocId>,
    token: u64,
    picks: Vec<ItemRef>,
    resolutions: Vec<Resolution>,
) -> Result<ImportDone, String> {
    let (_, src) = sources
        .get(token)
        .ok_or("Another file was picked since. Import again.")?;
    let mut s = state.lock_doc(doc)?;
    let label = format!(
        "Import {} of setup",
        crate::history::count(picks.len(), "item", "items")
    );
    let counts = s.edit(label, |b| import(b, &src, &picks, &resolutions))?;
    Ok(ImportDone {
        view: finish(&window, &mut s, None),
        message: counts.message(),
        counts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Session;
    use fwb_core::{starter, Currency, Dec, Fringe, Group, ReportSpec, UnitDef};

    /// A source with every kind of item, some referring to others.
    fn source() -> Budget {
        let mut b = starter::budget();
        b.title = "Source".into();
        for (n, f) in [("DAYS", "20"), ("WEEKS", "DAYS / 5"), ("FICA_RATE", "7.65")] {
            b.globals.insert(n.into(), f.into());
        }
        b.units.insert(
            "Shift".into(),
            UnitDef {
                plural: "Shifts".into(),
                description: String::new(),
                hours: Some(Dec::from_int(12)),
            },
        );
        b.fringes.insert(
            "FICA".into(),
            Fringe {
                short_code: None,
                description: "Social security".into(),
                rate: "FICA_RATE".into(),
                kind: FringeKind::Percent { cutoff: None },
            },
        );
        b.fringes.insert(
            "KIT".into(),
            Fringe {
                short_code: None,
                description: String::new(),
                rate: "WEEKS * 10".into(),
                kind: FringeKind::Flat {
                    unit: Some("Shift".into()),
                },
            },
        );
        b.groups.insert(
            "OT".into(),
            Group {
                short_code: None,
                description: "Overtime".into(),
                in_budget_total: true,
                color: None,
            },
        );
        b.currencies.insert(
            "EUR".into(),
            Currency {
                description: "Euro".into(),
                symbol: "€".into(),
                rate: "1.1".parse().unwrap(),
            },
        );
        b.charges.push(Charge {
            name: "Fee".into(),
            account: None,
            applied: true,
            kind: ChargeKind::Flat {
                amount: 500_000,
                currency: Some("EUR".into()),
            },
            excluded_amounts: Vec::new(),
            excluded_groups: ["OT".to_string()].into(),
            excluded_categories: BTreeSet::new(),
            excluded_accounts: BTreeSet::new(),
            label_percent: false,
            label_excluded: false,
        });
        b.credits.push(Credit {
            name: "State".into(),
            account: None,
            applied: true,
            cap: None,
            discount_percent: Dec::ZERO,
            rows: vec![fwb_core::CreditRow {
                description: "Labor".into(),
                source: CreditSource::Group {
                    group: "OT".into(),
                    excluded_fringes: ["KIT".to_string()].into(),
                },
                percent: Dec::from_int(25),
                cap: CreditCap::None,
            }],
        });
        b.report_presets.push(ReportPreset {
            name: "Crew".into(),
            default: true,
            spec: ReportSpec {
                only_groups: ["OT".to_string()].into(),
                footer: Some(fwb_core::Band {
                    left: Some(fwb_core::BandField::Global("WEEKS".into())),
                    ..fwb_core::Band::default()
                }),
                ..ReportSpec::default()
            },
        });
        b.fringe_posting = FringePosting::Account;
        b
    }

    fn pick(kind: &str, name: &str) -> ItemRef {
        ItemRef {
            kind: kind.into(),
            name: name.into(),
        }
    }

    fn res(kind: &str, name: &str, choice: &str) -> Resolution {
        Resolution {
            kind: kind.into(),
            name: name.into(),
            choice: choice.into(),
        }
    }

    fn item<'a>(p: &'a ImportPreview, kind: &str, name: &str) -> &'a ImportItem {
        p.items
            .iter()
            .find(|i| i.kind == kind && i.name == name)
            .unwrap_or_else(|| panic!("{kind} {name} not listed"))
    }

    #[test]
    fn the_review_lists_every_item_with_its_status_and_needs() {
        let src = source();
        let mut here = starter::budget();
        here.globals.insert("DAYS".into(), "30".into());
        let p = preview(&here, &src, 7, "Source", "/x/Source.fwb");
        assert_eq!(p.token, 7);
        // Globals first, settings last, as the review lists them.
        assert_eq!(p.items.first().unwrap().kind, "global");
        assert_eq!(p.items.last().unwrap().kind, "new_line_defaults");
        let days = item(&p, "global", "DAYS");
        assert_eq!(days.status, "differs");
        assert_eq!(days.here.as_deref(), Some("= 30"));
        assert_eq!(days.imported, "= 20");
        assert_eq!(days.rename_to.as_deref(), Some("DAYS_2"));
        assert_eq!(item(&p, "fringe", "PAYROLL").status, "same");
        let kit = item(&p, "fringe", "KIT");
        assert_eq!(kit.status, "new");
        assert_eq!(
            kit.needs,
            [
                pick("global", "DAYS"),
                pick("global", "WEEKS"),
                pick("unit", "Shift")
            ],
            "a flat fringe needs its unit and its rate's globals, however deep"
        );
        let credit = item(&p, "credit", "State");
        assert!(credit.needs.contains(&pick("fringe", "KIT")));
        assert!(credit.needs.contains(&pick("unit", "Shift")));
        assert!(credit.needs.contains(&pick("group", "OT")));
        assert_eq!(
            item(&p, "charge", "Fee").needs,
            [pick("group", "OT"), pick("currency", "EUR")]
        );
        assert_eq!(
            item(&p, "report_preset", "Crew").needs,
            [
                pick("global", "DAYS"),
                pick("global", "WEEKS"),
                pick("group", "OT")
            ],
            "a footer showing a global needs it, and what its formula names"
        );
        assert_eq!(item(&p, "fringe_posting", "").status, "differs");
        assert_eq!(item(&p, "fringe_posting", "").rename_to, None);
        assert_eq!(item(&p, "account_coding", "").status, "same");
        assert!(p.notes.is_empty());
        let json = serde_json::to_value(&p).unwrap();
        assert_eq!(json["items"][0]["kind"], "global");
    }

    #[test]
    fn import_brings_dependencies_and_is_one_undo_step() {
        let src = source();
        let mut s = Session::new();
        let before = s.budget().clone();
        let done = s
            .edit("Test edit", |b| {
                import(b, &src, &[pick("credit", "State")], &[])
            })
            .unwrap();
        let b = s.budget();
        assert_eq!(b.credits.len(), 1);
        assert!(
            b.fringes.contains_key("KIT"),
            "the credit's excluded fringe"
        );
        assert!(b.units.contains_key("Shift"), "the fringe's unit");
        assert!(b.groups.contains_key("OT"), "the credit's group");
        assert_eq!(b.globals.get("WEEKS").unwrap().0, "DAYS / 5");
        assert_eq!(b.globals.get("DAYS").unwrap().0, "20");
        assert!(!b.fringes.contains_key("FICA"), "not picked, not needed");
        assert!(b.charges.iter().all(|c| c.name != "Fee"));
        assert_eq!(done.added, 6, "{done:?}"); // credit, KIT, Shift, OT, DAYS, WEEKS
        assert_eq!(done.replaced, 0);
        assert!(s.undo(), "one undo step");
        assert_eq!(s.budget(), &before);
        assert!(!s.can_undo());
    }

    fn conflicted() -> Budget {
        let mut here = starter::budget();
        here.globals.insert("DAYS".into(), "30".into());
        here.groups.insert(
            "OT".into(),
            Group {
                short_code: None,
                description: "Other".into(),
                in_budget_total: false,
                color: None,
            },
        );
        here
    }

    #[test]
    fn conflicts_keep_this_budgets_version_by_default() {
        let src = source();
        let mut b = conflicted();
        let done = import(
            &mut b,
            &src,
            &[pick("fringe", "KIT"), pick("charge", "Fee")],
            &[],
        )
        .unwrap();
        assert_eq!(b.globals["DAYS"].0, "30", "kept");
        assert!(!b.groups["OT"].in_budget_total, "kept");
        assert_eq!(b.fringes["KIT"].rate.0, "WEEKS * 10");
        assert_eq!(done.kept, 2);
        let fee = b.charges.iter().find(|c| c.name == "Fee").unwrap();
        assert!(fee.excluded_groups.contains("OT"));
        assert!(b.currencies.contains_key("EUR"));
    }

    #[test]
    fn conflicts_can_use_the_imported_version_or_a_new_name() {
        let src = source();
        let mut b = conflicted();
        import(
            &mut b,
            &src,
            &[
                pick("fringe", "KIT"),
                pick("charge", "Fee"),
                pick("report_preset", "crew"),
            ],
            &[
                res("global", "DAYS", "imported"),
                res("group", "OT", "rename"),
            ],
        )
        .unwrap();
        assert_eq!(b.globals["DAYS"].0, "20", "imported replaces");
        assert!(!b.groups["OT"].in_budget_total, "this budget's OT stays");
        assert!(
            b.groups["OT (2)"].in_budget_total,
            "the imported one, renamed"
        );
        let fee = b.charges.iter().find(|c| c.name == "Fee").unwrap();
        assert!(fee.excluded_groups.contains("OT (2)"), "uses the new name");
        let crew = b.report_presets.iter().find(|p| p.name == "Crew").unwrap();
        assert!(crew.spec.only_groups.contains("OT (2)"));
        assert!(!crew.default, "an added preset is never the default");

        // A renamed global is renamed in what the import brings.
        let mut b = conflicted();
        import(
            &mut b,
            &src,
            &[pick("fringe", "KIT")],
            &[res("global", "DAYS", "rename")],
        )
        .unwrap();
        assert_eq!(b.globals["DAYS"].0, "30");
        assert_eq!(b.globals["DAYS_2"].0, "20");
        assert_eq!(b.globals["WEEKS"].0, "DAYS_2 / 5");

        // Settings: keep by default, or the imported one.
        let mut b = conflicted();
        import(&mut b, &src, &[pick("fringe_posting", "")], &[]).unwrap();
        assert_eq!(b.fringe_posting, FringePosting::default());
        import(
            &mut b,
            &src,
            &[pick("fringe_posting", "")],
            &[res("fringe_posting", "", "imported")],
        )
        .unwrap();
        assert_eq!(b.fringe_posting, FringePosting::Account);
    }

    #[test]
    fn replacing_a_charge_or_preset_keeps_its_place_and_default() {
        let src = source();
        let mut b = starter::budget();
        b.charges.insert(
            0,
            Charge {
                name: "Fee".into(),
                ..b.charges[0].clone()
            },
        );
        b.report_presets.push(ReportPreset {
            name: "CREW".into(),
            default: true,
            spec: ReportSpec::default(),
        });
        let done = import(
            &mut b,
            &src,
            &[pick("charge", "Fee"), pick("report_preset", "Crew")],
            &[
                res("charge", "Fee", "imported"),
                res("report_preset", "Crew", "imported"),
            ],
        )
        .unwrap();
        assert_eq!(b.charges[0].name, "Fee");
        assert!(matches!(b.charges[0].kind, ChargeKind::Flat { .. }));
        assert_eq!(b.report_presets.len(), 1);
        assert_eq!(b.report_presets[0].name, "CREW");
        assert!(b.report_presets[0].default);
        assert!(b.report_presets[0].spec.only_groups.contains("OT"));
        assert_eq!(done.replaced, 2);
    }

    #[test]
    fn tag_presets_and_line_blocks_bring_their_tags_under_new_names() {
        let mut src = source();
        src.tag_presets.push(fwb_core::TagPreset {
            name: "Crew OT".into(),
            fringes: ["FICA".to_string()].into(),
            groups: ["OT".to_string()].into(),
            ..fwb_core::TagPreset::default()
        });
        let mut row = fwb_core::Detail::line(0, "Grip", "DAYS", "Shift", "400");
        row.groups = ["OT".to_string()].into();
        let mut setup = fwb_core::SetupItems::default();
        setup.groups.insert("OT".into(), src.groups["OT"].clone());
        src.line_presets.push(fwb_core::LinePreset {
            name: "Grips".into(),
            rows: vec![row],
            setup,
        });
        let p = preview(&starter::budget(), &src, 1, "Source", "");
        assert_eq!(
            item(&p, "tag_preset", "Crew OT").needs,
            [
                pick("global", "FICA_RATE"),
                pick("fringe", "FICA"),
                pick("group", "OT")
            ]
        );
        assert_eq!(
            item(&p, "line_preset", "Grips").needs,
            [
                pick("global", "DAYS"),
                pick("group", "OT"),
                pick("unit", "Shift")
            ]
        );

        let mut b = conflicted();
        import(
            &mut b,
            &src,
            &[pick("tag_preset", "crew ot"), pick("line_preset", "Grips")],
            &[res("group", "OT", "rename")],
        )
        .unwrap();
        assert_eq!(b.tag_presets[0].name, "Crew OT");
        assert!(b.tag_presets[0].groups.contains("OT (2)"));
        assert!(b.fringes.contains_key("FICA"));
        let block = &b.line_presets[0];
        assert!(block.rows[0].groups.contains("OT (2)"));
        assert!(block.setup.groups.contains_key("OT (2)"));
        assert!(!block.setup.groups.contains_key("OT"));
    }

    #[test]
    fn bad_picks_and_choices_change_nothing() {
        let src = source();
        let mut b = conflicted();
        let before = b.clone();
        assert!(import(&mut b, &src, &[pick("fringe", "NOPE")], &[]).is_err());
        assert!(import(&mut b, &src, &[pick("bogus", "x")], &[]).is_err());
        assert!(import(
            &mut b,
            &src,
            &[pick("fringe", "KIT")],
            &[res("global", "DAYS", "maybe")]
        )
        .is_err());
        assert!(import(
            &mut b,
            &src,
            &[pick("fringe_posting", "")],
            &[res("fringe_posting", "", "rename")]
        )
        .is_err());
        assert_eq!(b, before);
    }

    #[test]
    fn the_base_currency_is_not_imported_and_flat_charges_in_it_count_as_base() {
        let mut src = source();
        src.base_currency = "CAD".into();
        let mut b = starter::budget();
        b.base_currency = "EUR".into();
        let p = preview(&b, &src, 1, "Source", "");
        assert!(p
            .items
            .iter()
            .all(|i| !(i.kind == "currency" && i.name == "EUR")));
        assert_eq!(p.notes.len(), 1);
        assert!(!item(&p, "charge", "Fee")
            .needs
            .contains(&pick("currency", "EUR")));
        import(&mut b, &src, &[pick("charge", "Fee")], &[]).unwrap();
        assert!(!b.currencies.contains_key("EUR"));
        let fee = b.charges.iter().find(|c| c.name == "Fee").unwrap();
        assert_eq!(
            fee.kind,
            ChargeKind::Flat {
                amount: 500_000,
                currency: None
            }
        );
    }

    #[test]
    fn a_source_open_in_a_tab_is_read_from_the_tab_and_never_itself() {
        let dir = crate::safety::tests::temp_dir("import-source");
        let path = dir.join("other.fwb");
        let mut other = Session::new();
        other.save_to(&path).unwrap();
        other
            .edit("Test edit", |b| crate::edit::set_global(b, "UNSAVED", "1"))
            .unwrap();
        let mut docs = Docs::default();
        let first = docs.active();
        let tab = docs.add(other);
        let (label, b) = read_source(&docs, first, &path).unwrap();
        assert_eq!(label, "other");
        assert!(b.globals.contains_key("UNSAVED"), "unsaved changes count");
        assert!(read_source(&docs, tab, &path).is_err(), "not from itself");
        docs.remove(tab).unwrap();
        let (_, b) = read_source(&docs, first, &path).unwrap();
        assert!(!b.globals.contains_key("UNSAVED"), "read from the file");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn commands_are_wired_for_the_main_window_only() {
        const APP_RS: &str = include_str!("app.rs");
        const BUILD_RS: &str = include_str!("../build.rs");
        const MAIN_CAPS: &str = include_str!("../capabilities/main.json");
        const TOOLS_CAPS: &str = include_str!("../capabilities/tools.json");
        for (module, c) in [
            ("import_setup", "import_setup_pick"),
            ("import_setup", "import_setup"),
            ("templates", "template_new"),
            ("templates", "template_save"),
        ] {
            assert!(
                APP_RS.contains(&format!("crate::{module}::{c},")),
                "{c} not in generate_handler!"
            );
            assert!(
                BUILD_RS.contains(&format!("\"{c}\",")),
                "{c} not in build.rs"
            );
            let allow = format!("\"allow-{}\"", c.replace('_', "-"));
            assert!(
                MAIN_CAPS.contains(&allow),
                "{c} not allowed for the main window"
            );
            assert!(
                !TOOLS_CAPS.contains(&allow),
                "{c} allowed for the tools window"
            );
        }
    }
}
