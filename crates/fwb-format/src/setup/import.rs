//! Importing setup items from another budget (File > Import Setup From…,
//! the setup library and setup packs; decisions.md, entries 15, 23, 29, 37
//! and 50): every item listed by kind with how it stands in the target and
//! what it needs, transitively; importing the picked items and everything
//! they need, keeping the target's version of what differs unless the
//! caller picks the imported one or a new name. Plain functions on budgets,
//! shared by the application (src/import_setup.rs) and the command line.

use std::collections::{BTreeMap, BTreeSet};

use fwb_core::coding::AccountCoding;
use fwb_core::{
    Budget, Charge, ChargeKind, Credit, CreditCap, CreditSource, FringeKind, FringePosting,
    NewLineDefaults, ReportPreset, MAX_PRESETS, MAX_PRESET_NAME_CHARS,
};
use serde::{Deserialize, Serialize};

use super::copy::{self as clipboard, global_names, EditResult, Kind, Renames, Setup, Status};

/// "1 item", "3 items".
pub fn count(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

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
    OtPattern,
    AccountCoding,
    FringePosting,
    NewLineDefaults,
}

impl What {
    pub const ALL: [What; 16] = [
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
        What::OtPattern,
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
            What::OtPattern => "ot_pattern",
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

pub type Key = (What, String);

pub fn item_ref(k: &Key) -> ItemRef {
    ItemRef {
        kind: k.0.key().to_string(),
        name: k.1.clone(),
    }
}

/// Every setup item in `b`, by kind, in review order.
pub fn all_items(b: &Budget) -> Vec<Key> {
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
            What::OtPattern => b.ot_patterns.iter().map(|p| p.name.clone()).collect(),
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
        What::OtPattern => b.ot_patterns.iter().any(|p| p.name == name),
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
pub fn closure(b: &Budget, start: impl IntoIterator<Item = Key>) -> BTreeSet<Key> {
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
        What::OtPattern => clipboard::status(
            src.ot_patterns.iter().find(|p| p.name == name),
            here.ot_patterns.iter().find(|p| p.name == name),
        ),
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
pub fn describe(b: &Budget, w: What, name: &str) -> String {
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
        What::OtPattern => b
            .ot_patterns
            .iter()
            .find(|p| p.name == name)
            .map(describe_ot),
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

fn describe_ot(p: &fwb_core::overtime::OtPattern) -> String {
    let mut parts = vec![format!("{} straight hours", p.straight_hours)];
    for band in &p.bands {
        parts.push(match band.hours {
            Some(h) => format!("{h} hours at {}x", band.multiplier),
            None => format!("then {}x", band.multiplier),
        });
    }
    if let Some(m) = p.sixth_day {
        parts.push(format!("6th day {m}x"));
    }
    if let Some(m) = p.seventh_day {
        parts.push(format!("7th day {m}x"));
    }
    parts.join(", ")
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
            What::OtPattern => fwb_core::overtime::MAX_OT_NAME_CHARS,
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
    /// The source's file name without its extension, or its tab's name.
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
    /// Every item written, but the settings: its kind, its name in the
    /// source and its name here (setup packs stamp these).
    #[serde(skip)]
    pub written: Vec<(What, String, String)>,
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
        (
            What::OtPattern,
            b.ot_patterns.len(),
            fwb_core::overtime::MAX_OT_PATTERNS,
            "overtime patterns",
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
            if w == What::Global {
                // A global's description, unit and display come with it.
                match src.global_info.get(&from).filter(|g| !g.is_default()) {
                    Some(g) => {
                        let mut g = g.clone();
                        g.unit = clipboard::renamed(&renames.units, &g.unit);
                        b.global_info.insert(to.clone(), g);
                    }
                    None => {
                        b.global_info.remove(&to);
                    }
                }
            }
            carry_source(b, src, w, &from, &to, &mut done);
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
            What::OtPattern => {
                let Some(p) = src.ot_patterns.iter().find(|p| p.name == from) else {
                    continue;
                };
                let mut p = p.clone();
                p.name = to.clone();
                match b.ot_patterns.iter().position(|x| x.name == to) {
                    Some(i) => b.ot_patterns[i] = p,
                    None => b.ot_patterns.push(p),
                }
            }
            What::AccountCoding => b.account_coding = src.account_coding,
            What::FringePosting => b.fringe_posting = src.fringe_posting.clone(),
            What::NewLineDefaults => b.new_line_defaults = src.new_line_defaults,
            _ => {}
        }
        if !w.is_setting() {
            carry_source(b, src, w, &from, &to, &mut done);
        }
    }
    Ok(done)
}

/// Item `from` of `src` was just written into `b` as `to`: where it came
/// from comes with it (the source's own entry, or none, replacing what this
/// budget said about the item it replaced), and it is listed as written.
fn carry_source(b: &mut Budget, src: &Budget, w: What, from: &str, to: &str, done: &mut Imported) {
    // A preset keeps this budget's spelling when it replaced one.
    let here = match (w, preset_index(b, w, to)) {
        (What::ReportPreset, Some(i)) => b.report_presets[i].name.clone(),
        (What::TagPreset, Some(i)) => b.tag_presets[i].name.clone(),
        (What::LinePreset, Some(i)) => b.line_presets[i].name.clone(),
        _ => to.trim().to_string(),
    };
    let kind = w.key();
    match src.setup_sources.get(kind, from) {
        Some(s) => {
            let s = s.clone();
            b.setup_sources.set(kind, &here, s);
        }
        None => b.setup_sources.remove(kind, &here),
    }
    done.written.push((w, from.to_string(), here));
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
