//! Copying and pasting whole rows, accounts and categories with everything
//! attached (docs/APP.md, "Copy and paste rows"; decisions.md, entry 23).
//!
//! - **The clip.** Copying whole rows (detail lines, accounts on the account
//!   list, categories on the topsheet) keeps a copy of them in Rust, in
//!   [`ClipboardState`]: one clipboard for the whole app, shared by every
//!   budget open as a tab (and every window), holding every field
//!   of every row plus the definitions of every setup item they use, with
//!   what those use in turn. The page puts the rows' tab-separated text on
//!   the system clipboard for other apps; that exact text is the clip's key.
//!   A paste that finds other text (anything copied since, here or in
//!   another app) is a cell paste, and the clip is forgotten.
//! - **Paste** inserts above the target row (or account, or category) with
//!   new row ids and free account and category numbers, gives the pasted
//!   fringe ranges numbers new in the account, and adds or resolves the setup
//!   items, all as one undo step.
//! - **The review.** Each setup item the clip uses is missing here (it is
//!   added), the same here (nothing to do) or different here: the budget's
//!   own version is kept (decision 12) unless the user picks the copied one
//!   or "add it under a new name".
//!
//! Everything but the commands at the end is plain functions on a
//! [`Budget`], so it is tested without a window.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use fwb_core::coding::AccountCoding;
use fwb_core::{
    Account, Budget, Category, Cents, Currency, Detail, Formula, Fringe, FringeKind, Group,
    Location, RowId, RowKind, SetDef, TopsheetRow, UnitDef,
};
use serde::{Deserialize, Serialize};
use tauri::{Runtime, State, WebviewWindow};

use crate::app::{finish, AppState, DocId};
use crate::edit::{self, EditResult};
use crate::view::View;

// ---------------------------------------------------------------------------
// What a clip holds

/// The rows, accounts or categories copied, as they were (every field).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Content {
    Rows(Vec<Detail>),
    Accounts(Vec<Account>),
    Categories(Vec<Category>),
}

impl Content {
    /// `rows`, `accounts` or `categories`.
    pub fn what(&self) -> &'static str {
        match self {
            Content::Rows(_) => "rows",
            Content::Accounts(_) => "accounts",
            Content::Categories(_) => "categories",
        }
    }

    /// How many rows, accounts or categories.
    pub fn count(&self) -> usize {
        match self {
            Content::Rows(r) => r.len(),
            Content::Accounts(a) => a.len(),
            Content::Categories(c) => c.len(),
        }
    }

    /// Every detail row in it, in order.
    pub fn details(&self) -> Vec<&Detail> {
        match self {
            Content::Rows(r) => r.iter().collect(),
            Content::Accounts(a) => a.iter().flat_map(|a| &a.details).collect(),
            Content::Categories(c) => c
                .iter()
                .flat_map(|c| &c.accounts)
                .flat_map(|a| &a.details)
                .collect(),
        }
    }

    fn details_mut(&mut self) -> Vec<&mut Detail> {
        match self {
            Content::Rows(r) => r.iter_mut().collect(),
            Content::Accounts(a) => a.iter_mut().flat_map(|a| &mut a.details).collect(),
            Content::Categories(c) => c
                .iter_mut()
                .flat_map(|c| &mut c.accounts)
                .flat_map(|a| &mut a.details)
                .collect(),
        }
    }
}

/// The kinds of setup item rows refer to by name, in the order the review
/// lists them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Fringe,
    Group,
    Global,
    Unit,
    Set,
    Location,
    Currency,
}

impl Kind {
    pub const ALL: [Kind; 7] = [
        Kind::Fringe,
        Kind::Group,
        Kind::Global,
        Kind::Unit,
        Kind::Set,
        Kind::Location,
        Kind::Currency,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Kind::Fringe => "fringe",
            Kind::Group => "group",
            Kind::Global => "global",
            Kind::Unit => "unit",
            Kind::Set => "set",
            Kind::Location => "location",
            Kind::Currency => "currency",
        }
    }

    pub fn parse(s: &str) -> EditResult<Kind> {
        Kind::ALL
            .into_iter()
            .find(|k| k.key() == s)
            .ok_or_else(|| format!("unknown kind of setup item `{s}`"))
    }

    /// Whether "add it under a new name" makes sense. A currency code names
    /// a real currency, so a second one under another code would not.
    pub fn renamable(self) -> bool {
        self != Kind::Currency
    }
}

/// Setup items by kind and name: the same tables a [`Budget`] has, holding
/// only what a clip's rows use.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Setup {
    pub fringes: BTreeMap<String, Fringe>,
    pub groups: BTreeMap<String, Group>,
    pub globals: BTreeMap<String, Formula>,
    pub units: BTreeMap<String, UnitDef>,
    pub sets: BTreeMap<String, SetDef>,
    pub locations: BTreeMap<String, Location>,
    pub currencies: BTreeMap<String, Currency>,
}

/// Runs `$body` with each `$m` bound to the table of `$kind` in `$t` (a
/// [`Budget`] or a [`Setup`]: they name their tables the same).
macro_rules! with_tables {
    ($kind:expr, $($m:ident = $t:expr),+ => $body:expr) => {
        match $kind {
            Kind::Fringe => { $(let $m = &$t.fringes;)+ $body }
            Kind::Group => { $(let $m = &$t.groups;)+ $body }
            Kind::Global => { $(let $m = &$t.globals;)+ $body }
            Kind::Unit => { $(let $m = &$t.units;)+ $body }
            Kind::Set => { $(let $m = &$t.sets;)+ $body }
            Kind::Location => { $(let $m = &$t.locations;)+ $body }
            Kind::Currency => { $(let $m = &$t.currencies;)+ $body }
        }
    };
}

/// An item's definition in words, from `$t` (a [`Budget`] or a [`Setup`]).
macro_rules! describe_in {
    ($kind:expr, $name:expr, $t:expr) => {
        match $kind {
            Kind::Fringe => $t.fringes.get($name).map(describe_fringe),
            Kind::Group => $t.groups.get($name).map(describe_group),
            Kind::Global => $t.globals.get($name).map(describe_global),
            Kind::Unit => $t.units.get($name).map(describe_unit),
            Kind::Set => $t.sets.get($name).map(describe_set),
            Kind::Location => $t.locations.get($name).map(describe_location),
            Kind::Currency => $t.currencies.get($name).map(describe_currency),
        }
        .unwrap_or_default()
    };
}

impl Setup {
    /// What `rows` use in `b` (fringes, groups, units, globals named in
    /// formulas, sets, locations, currencies), with what those use in turn: a
    /// fringe's flat unit and the globals in its rate, the globals in a
    /// global's formula, however deep. Names `b` doesn't define are left out
    /// (the rows keep them as they are).
    pub fn used_by<'a>(b: &Budget, rows: impl IntoIterator<Item = &'a Detail>) -> Setup {
        fn take<T: Clone>(from: &BTreeMap<String, T>, name: &str, to: &mut BTreeMap<String, T>) {
            if let Some(def) = from.get(name) {
                to.insert(name.to_string(), def.clone());
            }
        }
        let mut s = Setup::default();
        let mut globals: Vec<String> = Vec::new();
        let mut units: BTreeSet<String> = BTreeSet::new();
        for d in rows {
            for f in &d.fringes {
                take(&b.fringes, f, &mut s.fringes);
            }
            for g in &d.groups {
                take(&b.groups, g, &mut s.groups);
            }
            if let Some(n) = &d.set {
                take(&b.sets, n, &mut s.sets);
            }
            if let Some(n) = &d.location {
                take(&b.locations, n, &mut s.locations);
            }
            if let Some(c) = &d.currency {
                take(&b.currencies, c, &mut s.currencies);
            }
            for u in [&d.unit, &d.unit2, &d.unit3, &d.unit4] {
                if !u.is_empty() {
                    units.insert(u.clone());
                }
            }
            for f in [&d.amount, &d.x, &d.rate, &d.x4] {
                globals.extend(global_names(&f.0));
            }
        }
        for f in s.fringes.values() {
            globals.extend(global_names(&f.rate.0));
            if let FringeKind::Flat { unit: Some(u) } = &f.kind {
                units.insert(u.clone());
            }
        }
        for u in units {
            take(&b.units, &u, &mut s.units);
        }
        while let Some(g) = globals.pop() {
            if s.globals.contains_key(&g) {
                continue;
            }
            if let Some(f) = b.globals.get(&g) {
                globals.extend(global_names(&f.0));
                s.globals.insert(g, f.clone());
            }
        }
        s
    }
}

/// Global names a formula mentions, read the way the formula lexer reads
/// them (letters, digits and `_`, not starting with a digit). Works on
/// formulas that don't parse too.
pub fn global_names(src: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    scan_names(src, |name| {
        out.insert(name.to_string());
        name.to_string()
    });
    out
}

/// `src` with every global named in `renames` renamed, name by name (never
/// inside a longer name); everything else stays as typed.
pub fn rename_globals(src: &str, renames: &BTreeMap<String, String>) -> String {
    if renames.is_empty() {
        return src.to_string();
    }
    scan_names(src, |name| {
        renames
            .get(name)
            .cloned()
            .unwrap_or_else(|| name.to_string())
    })
}

/// Walks `src` like the formula lexer: numbers (digits and points) are kept,
/// each name is replaced by what `f` returns, other characters are kept.
fn scan_names(src: &str, mut f: impl FnMut(&str) -> String) -> String {
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        let start = i;
        if c.is_ascii_digit() || c == b'.' {
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            out.push_str(&src[start..i]);
        } else if c.is_ascii_alphabetic() || c == b'_' {
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            out.push_str(&f(&src[start..i]));
        } else {
            let ch = src[i..].chars().next().unwrap_or('?');
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

/// Clipboard text as a key: line endings as `\n`, no trailing newline (the
/// system clipboard may change both on the way back).
pub fn normalize(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .trim_end_matches('\n')
        .to_string()
}

static NEXT_TOKEN: AtomicU64 = AtomicU64::new(1);

/// One copy: what was copied, what it uses, where it came from.
#[derive(Clone, Debug)]
pub struct Clip {
    /// Names this copy; a paste names the copy it reviewed.
    pub token: u64,
    /// The text put on the system clipboard (normalized), or `None` when the
    /// page couldn't put it there (only Paste Rows finds the clip then).
    key: Option<String>,
    pub content: Content,
    pub setup: Setup,
    /// The base currency of the budget it came from.
    pub base_currency: String,
    /// The title of the budget it came from.
    pub source: String,
}

impl Clip {
    /// `content`, taken from `b`, with the setup it uses. `text` is the text
    /// the page put on the system clipboard (`None`: it couldn't).
    pub fn new(b: &Budget, mut content: Content, text: Option<&str>) -> Clip {
        // A line in the base currency counts in the base currency wherever
        // it lands, like a line with no currency.
        for d in content.details_mut() {
            if d.currency.as_deref() == Some(b.base_currency.as_str()) {
                d.currency = None;
            }
        }
        let setup = Setup::used_by(b, content.details());
        Clip {
            token: NEXT_TOKEN.fetch_add(1, Ordering::Relaxed),
            key: text.map(normalize).filter(|k| !k.is_empty()),
            content,
            setup,
            base_currency: b.base_currency.clone(),
            source: b.title.clone(),
        }
    }

    /// Whether `text` (what a paste found on the system clipboard) is this
    /// clip's text.
    pub fn matches(&self, text: &str) -> bool {
        self.key.as_deref() == Some(normalize(text).as_str())
    }

    /// Whether its text went on the system clipboard.
    pub fn on_clipboard(&self) -> bool {
        self.key.is_some()
    }

    pub fn info(&self) -> ClipInfo {
        ClipInfo {
            token: self.token,
            what: self.content.what(),
            count: self.content.count(),
            lines: self.content.details().len(),
        }
    }
}

/// What a copy took, for the page.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ClipInfo {
    pub token: u64,
    pub what: &'static str,
    pub count: usize,
    pub lines: usize,
}

/// The app's one row clipboard, shared by every budget.
#[derive(Debug, Default)]
pub struct ClipboardState {
    clip: Mutex<Option<Arc<Clip>>>,
}

impl ClipboardState {
    fn lock(&self) -> MutexGuard<'_, Option<Arc<Clip>>> {
        self.clip.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn set(&self, clip: Clip) {
        *self.lock() = Some(Arc::new(clip));
    }

    pub fn forget(&self) {
        *self.lock() = None;
    }

    pub fn current(&self) -> Option<Arc<Clip>> {
        self.lock().clone()
    }

    /// The clip a paste should use. With `text` (what Cmd+V found on the
    /// system clipboard), only when that is the clip's own text; text that
    /// isn't means something else was copied since, so a clip that was on
    /// the clipboard is forgotten. Without `text` (Paste Rows), the clip
    /// whatever the system clipboard holds.
    pub fn for_paste(&self, text: Option<&str>) -> Option<Arc<Clip>> {
        let mut g = self.lock();
        let Some(text) = text else {
            return g.clone();
        };
        match g.as_ref() {
            Some(c) if c.matches(text) => Some(Arc::clone(c)),
            Some(c) if c.on_clipboard() => {
                *g = None;
                None
            }
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Copying and cutting

/// What to copy, as the page names it.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Pick {
    /// `rows`, `accounts` or `categories`.
    pub what: String,
    /// Rows: detail row ids.
    #[serde(default)]
    pub row_ids: Vec<RowId>,
    /// Accounts: account numbers.
    #[serde(default)]
    pub accounts: Vec<String>,
    /// Categories: topsheet positions (production totals among them are
    /// passed over: they aren't copied, and a cut leaves them).
    #[serde(default)]
    pub categories: Vec<usize>,
}

/// Copies of the picked rows, accounts or categories, in budget order.
/// Refused when nothing is picked or something picked no longer exists.
pub fn take(b: &Budget, pick: &Pick) -> EditResult<Content> {
    match pick.what.as_str() {
        "rows" => {
            let want: BTreeSet<RowId> = pick.row_ids.iter().copied().collect();
            if want.is_empty() {
                return Err("select the rows to copy first".into());
            }
            let rows: Vec<Detail> = edit::details(b)
                .filter(|d| want.contains(&d.id))
                .cloned()
                .collect();
            let found: BTreeSet<RowId> = rows.iter().map(|d| d.id).collect();
            if let Some(id) = want.iter().find(|id| !found.contains(id)) {
                return Err(edit::row_gone(*id));
            }
            Ok(Content::Rows(rows))
        }
        "accounts" => {
            let want: BTreeSet<&str> = pick.accounts.iter().map(String::as_str).collect();
            if want.is_empty() {
                return Err("select the accounts to copy first".into());
            }
            let accounts: Vec<Account> = b
                .categories()
                .flat_map(|c| &c.accounts)
                .filter(|a| want.contains(a.code.as_str()))
                .cloned()
                .collect();
            let found: BTreeSet<&str> = accounts.iter().map(|a| a.code.as_str()).collect();
            if let Some(code) = want.iter().find(|c| !found.contains(*c)) {
                return Err(format!("account {code} no longer exists"));
            }
            Ok(Content::Accounts(accounts))
        }
        "categories" => {
            let want: BTreeSet<usize> = pick.categories.iter().copied().collect();
            let mut cats = Vec::new();
            for &i in &want {
                match b.topsheet.get(i) {
                    Some(TopsheetRow::Category(c)) => cats.push(c.clone()),
                    Some(TopsheetRow::ProductionTotal(_)) => {}
                    None => return Err("that topsheet row no longer exists".into()),
                }
            }
            if cats.is_empty() {
                return Err("select the categories to copy first".into());
            }
            Ok(Content::Categories(cats))
        }
        other => Err(format!("can't copy `{other}`")),
    }
}

/// Deletes what `pick` names (the second half of a cut; [`take`] first).
pub fn cut(b: &mut Budget, pick: &Pick) -> EditResult {
    take(b, pick)?;
    match pick.what.as_str() {
        "rows" => {
            edit::delete_details(b, &pick.row_ids)?;
        }
        "accounts" => {
            let codes: BTreeSet<&String> = pick.accounts.iter().collect();
            for code in codes {
                edit::delete_account(b, code)?;
            }
        }
        _ => {
            let at: BTreeSet<usize> = pick.categories.iter().copied().collect();
            for &i in at.iter().rev() {
                if matches!(b.topsheet.get(i), Some(TopsheetRow::Category(_))) {
                    b.topsheet.remove(i);
                }
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// The review: how each setup item the clip uses stands in the target

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Status {
    /// Not in the target: it will be added.
    Add,
    /// The same definition in the target: nothing to do.
    Same,
    /// A different definition in the target under the same name.
    Differs,
}

#[derive(Clone, Debug)]
struct Entry {
    kind: Kind,
    name: String,
    status: Status,
}

fn status<T: PartialEq>(copied: Option<&T>, here: Option<&T>) -> Status {
    match here {
        None => Status::Add,
        Some(h) if Some(h) == copied => Status::Same,
        Some(_) => Status::Differs,
    }
}

/// Every setup item the clip uses, with how it stands in `b`. The target's
/// base currency is left out: lines in it count in the base currency.
fn entries(b: &Budget, clip: &Clip) -> Vec<Entry> {
    let mut out = Vec::new();
    for kind in Kind::ALL {
        let names: Vec<String> = with_tables!(kind, c = clip.setup => c.keys().cloned().collect());
        for name in names {
            if kind == Kind::Currency && name == b.base_currency {
                continue;
            }
            let status =
                with_tables!(kind, c = clip.setup, h = b => status(c.get(&name), h.get(&name)));
            out.push(Entry { kind, name, status });
        }
    }
    out
}

/// For each item that differs (and can be renamed), the name "add it under
/// a new name" gives it: `Name (2)` (`NAME_2` for globals, which must stay
/// names a formula can use), the first free in the target and among the
/// clip's own items of that kind.
fn proposed_names(b: &Budget, clip: &Clip, entries: &[Entry]) -> BTreeMap<(Kind, String), String> {
    let mut out = BTreeMap::new();
    let mut chosen: BTreeSet<(Kind, String)> = BTreeSet::new();
    for e in entries
        .iter()
        .filter(|e| e.status == Status::Differs && e.kind.renamable())
    {
        let taken = |n: &str| {
            with_tables!(e.kind, h = b, c = clip.setup => h.contains_key(n) || c.contains_key(n))
                || chosen.contains(&(e.kind, n.to_string()))
        };
        let name = (2u64..)
            .map(|n| match e.kind {
                Kind::Global => format!("{}_{n}", e.name),
                _ => format!("{} ({n})", e.name),
            })
            .find(|c| !taken(c))
            .expect("free names never run out");
        chosen.insert((e.kind, name.clone()));
        out.insert((e.kind, e.name.clone()), name);
    }
    out
}

/// One setup item in the review.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ReviewItem {
    /// `fringe`, `group`, `global`, `unit`, `set`, `location` or `currency`.
    pub kind: &'static str,
    pub name: String,
    /// `add` (missing here: it will be added) or `differs` (here under the
    /// same name with another definition).
    pub status: &'static str,
    /// The copied definition, in words.
    pub copied: String,
    /// This budget's definition, in words (`differs` only).
    pub here: Option<String>,
    /// The name "add it under a new name" would give it, when that choice
    /// is offered (`differs` only, not for currencies).
    pub rename_to: Option<String>,
}

/// What pasting the clip into a budget would do, before it does it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Preview {
    /// The clip this describes; the paste names it.
    pub token: u64,
    /// `rows`, `accounts` or `categories`.
    pub what: &'static str,
    /// How many of them.
    pub count: usize,
    /// Detail rows in them.
    pub lines: usize,
    /// The title of the budget they came from.
    pub source: String,
    /// Setup items to add, then items that differ, by kind and name.
    pub items: Vec<ReviewItem>,
    /// Setup items the clip uses that are the same here.
    pub same: usize,
    /// Other things to know before pasting, as sentences.
    pub notes: Vec<String>,
    /// Whether to show the review first (anything added, different or
    /// noted); otherwise the paste goes straight in.
    pub review: bool,
}

/// The review for pasting `clip` into `b`.
pub fn preview(b: &Budget, clip: &Clip) -> Preview {
    let entries = entries(b, clip);
    let proposed = proposed_names(b, clip, &entries);
    let mut items = Vec::new();
    let mut same = 0;
    for e in &entries {
        let copied = describe_in!(e.kind, &e.name, clip.setup);
        match e.status {
            Status::Same => same += 1,
            Status::Add => items.push(ReviewItem {
                kind: e.kind.key(),
                name: e.name.clone(),
                status: "add",
                copied,
                here: None,
                rename_to: None,
            }),
            Status::Differs => items.push(ReviewItem {
                kind: e.kind.key(),
                name: e.name.clone(),
                status: "differs",
                copied,
                here: Some(describe_in!(e.kind, &e.name, b)),
                rename_to: proposed.get(&(e.kind, e.name.clone())).cloned(),
            }),
        }
    }
    // Additions first, then differences, each by kind and name.
    items.sort_by_key(|i| i.status != "add");
    let mut notes = Vec::new();
    if clip.base_currency != b.base_currency {
        let n = clip
            .content
            .details()
            .iter()
            .filter(|d| d.kind == RowKind::Line && d.currency.is_none())
            .count();
        if n > 0 {
            let lines = if n == 1 { "line" } else { "lines" };
            notes.push(format!(
                "{n} {lines} in {from}, the base currency where they were copied, will count in {to}, this budget's base currency.",
                from = clip.base_currency,
                to = b.base_currency,
            ));
        }
    }
    let review = !items.is_empty() || !notes.is_empty();
    Preview {
        token: clip.token,
        what: clip.content.what(),
        count: clip.content.count(),
        lines: clip.content.details().len(),
        source: clip.source.clone(),
        items,
        same,
        notes,
        review,
    }
}

fn money(c: Cents) -> String {
    let a = c.unsigned_abs();
    let digits = (a / 100).to_string();
    let mut grouped = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    let s = format!("{grouped}.{:02}", a % 100);
    if c < 0 {
        format!("-{s}")
    } else {
        s
    }
}

/// Parts in words, joined; `fallback` when every part is empty.
fn words(parts: &[&str], fallback: &str) -> String {
    let kept: Vec<&str> = parts
        .iter()
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect();
    if kept.is_empty() {
        fallback.to_string()
    } else {
        kept.join("; ")
    }
}

fn describe_fringe(f: &Fringe) -> String {
    let rate = f.rate.0.trim();
    let how = match &f.kind {
        FringeKind::Percent { cutoff: None } => format!("{rate}% with no cutoff"),
        FringeKind::Percent { cutoff: Some(c) } => format!("{rate}% up to {}", money(*c)),
        FringeKind::Flat { unit: None } => format!("{rate} flat per line"),
        FringeKind::Flat { unit: Some(u) } => format!("{rate} flat per {u}"),
    };
    words(&[how.as_str(), f.description.as_str()], "")
}

fn describe_group(g: &Group) -> String {
    let counted = if g.in_budget_total {
        "counted in the total"
    } else {
        "left out of the total"
    };
    let color = g
        .color
        .as_deref()
        .map(|c| format!("color {c}"))
        .unwrap_or_default();
    words(&[counted, color.as_str(), g.description.as_str()], "")
}

fn describe_global(f: &Formula) -> String {
    format!("= {}", f.0.trim())
}

fn describe_unit(u: &UnitDef) -> String {
    let hours = match u.hours {
        Some(h) => format!("{h} hours"),
        None => "not a time unit".to_string(),
    };
    let plural = if u.plural.trim().is_empty() {
        String::new()
    } else {
        format!("plural {}", u.plural.trim())
    };
    words(
        &[hours.as_str(), plural.as_str(), u.description.as_str()],
        "",
    )
}

fn describe_set(s: &SetDef) -> String {
    words(&[s.description.as_str()], "no description")
}

fn describe_location(l: &Location) -> String {
    words(
        &[
            l.description.as_str(),
            l.city.as_str(),
            l.region.as_str(),
            l.country.as_str(),
        ],
        "no description",
    )
}

fn describe_currency(c: &Currency) -> String {
    let rate = format!("rate {}", c.rate);
    let symbol = if c.symbol.trim().is_empty() {
        String::new()
    } else {
        format!("symbol {}", c.symbol.trim())
    };
    words(
        &[rate.as_str(), symbol.as_str(), c.description.as_str()],
        "",
    )
}

// ---------------------------------------------------------------------------
// Pasting

/// What the user chose for one setup item that differs.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resolution {
    /// `fringe`, `group`, `global`, `unit`, `set`, `location` or `currency`.
    pub kind: String,
    pub name: String,
    /// `keep` (this budget's version: the default), `copied` (the copied
    /// version replaces it, for every line here that uses it) or `rename`
    /// (the copied version is added under the review's new name, and the
    /// pasted rows use that).
    pub choice: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Choice {
    Keep,
    Copied,
    Rename,
}

fn choices(rs: &[Resolution]) -> EditResult<BTreeMap<(Kind, String), Choice>> {
    let mut out = BTreeMap::new();
    for r in rs {
        let kind = Kind::parse(&r.kind)?;
        let choice = match r.choice.as_str() {
            "keep" => Choice::Keep,
            "copied" => Choice::Copied,
            "rename" => Choice::Rename,
            other => return Err(format!("unknown choice `{other}`")),
        };
        if choice == Choice::Rename && !kind.renamable() {
            return Err(format!("a {} can't be added under a new name", kind.key()));
        }
        out.insert((kind, r.name.clone()), choice);
    }
    Ok(out)
}

/// Where a paste goes.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Target {
    /// Rows: the account to paste into.
    #[serde(default)]
    pub account: Option<String>,
    /// Rows: the row to paste above (`None`: at the end of the account).
    #[serde(default)]
    pub before_row: Option<RowId>,
    /// Accounts: the category to paste into.
    #[serde(default)]
    pub category: Option<String>,
    /// Accounts: the account to paste above (`None`: at the end).
    #[serde(default)]
    pub before_account: Option<String>,
    /// Categories: the topsheet position to paste at (`None`: the end).
    #[serde(default)]
    pub at: Option<usize>,
}

/// What a paste created.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pasted {
    /// Every new detail row, in order.
    pub rows: Vec<RowId>,
    /// The new accounts' numbers (accounts and categories).
    pub accounts: Vec<String>,
    /// The new categories' topsheet positions.
    pub categories: Vec<usize>,
}

/// Names the pasted content and setup use instead of the copied ones (the
/// "add it under a new name" choices).
#[derive(Debug, Default)]
struct Renames {
    fringes: BTreeMap<String, String>,
    groups: BTreeMap<String, String>,
    globals: BTreeMap<String, String>,
    units: BTreeMap<String, String>,
    sets: BTreeMap<String, String>,
    locations: BTreeMap<String, String>,
}

impl Renames {
    fn of(&mut self, kind: Kind) -> Option<&mut BTreeMap<String, String>> {
        match kind {
            Kind::Fringe => Some(&mut self.fringes),
            Kind::Group => Some(&mut self.groups),
            Kind::Global => Some(&mut self.globals),
            Kind::Unit => Some(&mut self.units),
            Kind::Set => Some(&mut self.sets),
            Kind::Location => Some(&mut self.locations),
            Kind::Currency => None,
        }
    }
}

fn renamed(map: &BTreeMap<String, String>, name: &str) -> String {
    map.get(name).cloned().unwrap_or_else(|| name.to_string())
}

/// A copied row as it lands: references renamed, the base currency as none.
fn rewrite(d: &mut Detail, r: &Renames, base: &str) {
    if !r.fringes.is_empty() {
        d.fringes = d.fringes.iter().map(|n| renamed(&r.fringes, n)).collect();
    }
    if !r.groups.is_empty() {
        d.groups = d.groups.iter().map(|n| renamed(&r.groups, n)).collect();
    }
    for u in [&mut d.unit, &mut d.unit2, &mut d.unit3, &mut d.unit4] {
        if let Some(to) = r.units.get(u.as_str()) {
            *u = to.clone();
        }
    }
    if let Some(s) = &mut d.set {
        *s = renamed(&r.sets, s);
    }
    if let Some(l) = &mut d.location {
        *l = renamed(&r.locations, l);
    }
    for f in [&mut d.amount, &mut d.x, &mut d.rate, &mut d.x4] {
        f.0 = rename_globals(&f.0, &r.globals);
    }
    if d.currency.as_deref() == Some(base) {
        d.currency = None;
    }
}

/// Puts the copied item `from` into `b` as `to`, with the names it uses renamed.
fn write(b: &mut Budget, s: &Setup, kind: Kind, from: &str, to: &str, r: &Renames) {
    let to = to.to_string();
    match kind {
        Kind::Fringe => {
            if let Some(f) = s.fringes.get(from) {
                let mut f = f.clone();
                f.rate.0 = rename_globals(&f.rate.0, &r.globals);
                if let FringeKind::Flat { unit: Some(u) } = &mut f.kind {
                    *u = renamed(&r.units, u);
                }
                b.fringes.insert(to, f);
            }
        }
        Kind::Global => {
            if let Some(f) = s.globals.get(from) {
                b.globals
                    .insert(to, Formula(rename_globals(&f.0, &r.globals)));
            }
        }
        Kind::Group => {
            if let Some(g) = s.groups.get(from) {
                b.groups.insert(to, g.clone());
            }
        }
        Kind::Unit => {
            if let Some(u) = s.units.get(from) {
                b.units.insert(to, u.clone());
            }
        }
        Kind::Set => {
            if let Some(x) = s.sets.get(from) {
                b.sets.insert(to, x.clone());
            }
        }
        Kind::Location => {
            if let Some(x) = s.locations.get(from) {
                b.locations.insert(to, x.clone());
            }
        }
        Kind::Currency => {
            if let Some(x) = s.currencies.get(from) {
                b.currencies.insert(to, x.clone());
            }
        }
    }
}

/// The content worked out and ready to go in.
enum Placed {
    Rows {
        ti: usize,
        ai: usize,
        at: usize,
        rows: Vec<Detail>,
    },
    Accounts {
        ti: usize,
        at: usize,
        accounts: Vec<Account>,
    },
    Categories {
        at: usize,
        categories: Vec<Category>,
    },
}

/// The account numbered `code`: (topsheet position, index in its category).
fn find_account(b: &Budget, code: &str) -> Option<(usize, usize)> {
    b.topsheet.iter().enumerate().find_map(|(ti, r)| match r {
        TopsheetRow::Category(c) => c
            .accounts
            .iter()
            .position(|a| a.code == code)
            .map(|ai| (ti, ai)),
        TopsheetRow::ProductionTotal(_) => None,
    })
}

/// A fringe range number not in `used`: one past the highest, or the first
/// gap when the highest is the last there is.
fn free_range(used: &BTreeSet<u32>) -> EditResult<u32> {
    match used.last() {
        None => Ok(1),
        Some(&top) => top
            .checked_add(1)
            .or_else(|| (1..=u32::MAX).find(|n| !used.contains(n)))
            .ok_or_else(|| "this account has no fringe range numbers left".into()),
    }
}

/// Every account number in the budget.
fn account_codes(b: &Budget) -> BTreeSet<String> {
    b.categories()
        .flat_map(|c| &c.accounts)
        .map(|a| a.code.clone())
        .collect()
}

/// `n` written with at least `width` digits.
fn padded(n: u64, width: u8) -> String {
    format!("{n:0w$}", w = width as usize)
}

/// The numbers a `width`-digit segment can take after `current` (1 to the
/// largest, wrapping round, `current` last).
fn segment_numbers(current: &str, width: u8) -> impl Iterator<Item = String> {
    let max = 10u64
        .saturating_pow(u32::from(width))
        .saturating_sub(1)
        .max(1);
    let start = current.parse::<u64>().unwrap_or(0).min(max);
    (1..=max).map(move |step| padded((start + step - 1) % max + 1, width))
}

/// The number an account numbered `code` gets in category `category`: the
/// same account (sub-account, set) segments under the category's number
/// when that is free, else the last of them counted on, wrapping round,
/// until one is. Falls back to counting on the code's trailing digits when
/// the budget's coding pattern can't read it.
fn free_account_code(
    coding: &AccountCoding,
    category: &str,
    code: &str,
    taken: &BTreeSet<String>,
) -> EditResult<String> {
    if coding.validate().is_ok() {
        if let Ok(mut parts) = coding.parse(code) {
            if let Ok(cat) = coding.parse(category) {
                parts.category = cat.category;
            }
            let first = coding.format(&parts);
            if !taken.contains(&first) {
                return Ok(first);
            }
            let widths = [coding.account_width, coding.sub_width, coding.set_width];
            let segs = [&parts.account, &parts.sub, &parts.set];
            // The last segment written after the category; the account
            // segment when none is.
            let slot = (0..3)
                .rev()
                .find(|&i| widths[i] > 0 && !segs[i].is_empty())
                .or((widths[0] > 0).then_some(0));
            if let Some(i) = slot {
                for n in segment_numbers(segs[i], widths[i]) {
                    let mut p = parts.clone();
                    match i {
                        0 => p.account = n,
                        1 => p.sub = n,
                        _ => p.set = n,
                    }
                    let c = coding.format(&p);
                    if !taken.contains(&c) {
                        return Ok(c);
                    }
                }
            }
            return Err(format!(
                "there is no free account number left in category {}",
                parts.category
            ));
        }
    }
    count_on(code, taken)
}

/// The number a category numbered `code` gets: its own when free, else
/// counted on (wrapping round) within the category width.
fn free_category_code(
    coding: &AccountCoding,
    code: &str,
    taken: &BTreeSet<String>,
) -> EditResult<String> {
    if coding.validate().is_ok() {
        if let Ok(parts) = coding.parse(code) {
            if !taken.contains(&parts.category) {
                return Ok(parts.category);
            }
            return segment_numbers(&parts.category, coding.category_width)
                .find(|c| !taken.contains(c))
                .ok_or_else(|| "there is no free category number left".to_string());
        }
    }
    count_on(code, taken)
}

/// `code` when free, else its trailing number counted on (`A9` to `A10`,
/// `X` to `X2`) until one is.
fn count_on(code: &str, taken: &BTreeSet<String>) -> EditResult<String> {
    let code = code.trim();
    if !code.is_empty() && !taken.contains(code) {
        return Ok(code.to_string());
    }
    let head = code.trim_end_matches(|c: char| c.is_ascii_digit());
    let tail = &code[head.len()..];
    let mut n: u64 = tail.parse().unwrap_or(1);
    for _ in 0..100_000 {
        n += 1;
        let c = format!("{head}{n:0w$}", w = tail.len());
        if !taken.contains(&c) {
            return Ok(c);
        }
    }
    Err(format!("there is no free number left after {code}"))
}

/// Pastes `clip` into `b` at `target`, with the setup items resolved as
/// `resolutions` say (an item that differs and isn't named there keeps
/// this budget's version). Missing setup items are added. Pasted rows get
/// new ids, pasted accounts and categories free numbers, and pasted fringe
/// ranges numbers new in their account. Everything is worked out before
/// anything changes, so a refusal leaves `b` as it was.
pub fn paste(
    b: &mut Budget,
    clip: &Clip,
    target: &Target,
    resolutions: &[Resolution],
) -> EditResult<Pasted> {
    let choices = choices(resolutions)?;
    let entries = entries(b, clip);
    let proposed = proposed_names(b, clip, &entries);

    // Which copied items go in, under which names.
    let mut renames = Renames::default();
    let mut writes: Vec<(Kind, String, String)> = Vec::new();
    for e in &entries {
        let choice = match e.status {
            Status::Same => continue,
            Status::Add => Choice::Copied,
            Status::Differs => choices
                .get(&(e.kind, e.name.clone()))
                .copied()
                .unwrap_or(Choice::Keep),
        };
        match choice {
            Choice::Keep => {}
            Choice::Copied => writes.push((e.kind, e.name.clone(), e.name.clone())),
            Choice::Rename => {
                let to = proposed
                    .get(&(e.kind, e.name.clone()))
                    .cloned()
                    .ok_or_else(|| format!("a {} can't be added under a new name", e.kind.key()))?;
                if let Some(map) = renames.of(e.kind) {
                    map.insert(e.name.clone(), to.clone());
                }
                writes.push((e.kind, e.name.clone(), to));
            }
        }
    }

    // The content as it will land.
    let base = b.base_currency.clone();
    let mut next_id = edit::next_row_id(b);
    let mut fresh = |d: &Detail| {
        let mut d = d.clone();
        d.id = next_id;
        next_id = next_id.saturating_add(1);
        rewrite(&mut d, &renames, &base);
        d
    };
    let placed = match &clip.content {
        Content::Rows(rows) => {
            let code = target
                .account
                .as_deref()
                .ok_or("lines are pasted into an account")?;
            let (ti, ai) =
                find_account(b, code).ok_or_else(|| format!("account {code} no longer exists"))?;
            let TopsheetRow::Category(cat) = &b.topsheet[ti] else {
                unreachable!("find_account only finds accounts in categories");
            };
            let acct = &cat.accounts[ai];
            let at = match target.before_row {
                None => acct.details.len(),
                Some(id) => acct
                    .details
                    .iter()
                    .position(|d| d.id == id)
                    .ok_or_else(|| format!("row {id} is not in account {code}"))?,
            };
            // Ranges get numbers new in the account, so they never join
            // (or take the number of) a range already there.
            let mut used: BTreeSet<u32> =
                acct.details.iter().filter_map(|d| d.fringe_range).collect();
            let mut numbers: BTreeMap<u32, u32> = BTreeMap::new();
            let mut out = Vec::with_capacity(rows.len());
            for d in rows {
                let mut nd = fresh(d);
                if let Some(n) = d.fringe_range {
                    let m = match numbers.get(&n) {
                        Some(&m) => m,
                        None => {
                            let m = free_range(&used)?;
                            used.insert(m);
                            numbers.insert(n, m);
                            m
                        }
                    };
                    nd.fringe_range = Some(m);
                }
                out.push(nd);
            }
            Placed::Rows {
                ti,
                ai,
                at,
                rows: out,
            }
        }
        Content::Accounts(accounts) => {
            let code = target
                .category
                .as_deref()
                .ok_or("accounts are pasted into a category")?;
            let ti = b
                .topsheet
                .iter()
                .position(|r| matches!(r, TopsheetRow::Category(c) if c.code == code))
                .ok_or_else(|| format!("category {code} no longer exists"))?;
            let TopsheetRow::Category(cat) = &b.topsheet[ti] else {
                unreachable!("position found a category");
            };
            let at = match target.before_account.as_deref() {
                None => cat.accounts.len(),
                Some(a) => cat
                    .accounts
                    .iter()
                    .position(|x| x.code == a)
                    .ok_or_else(|| format!("account {a} is not in category {code}"))?,
            };
            let mut taken = account_codes(b);
            let mut out = Vec::with_capacity(accounts.len());
            for a in accounts {
                let new_code = free_account_code(&b.account_coding, &cat.code, &a.code, &taken)?;
                taken.insert(new_code.clone());
                out.push(Account {
                    code: new_code,
                    name: a.name.clone(),
                    details: a.details.iter().map(&mut fresh).collect(),
                });
            }
            Placed::Accounts {
                ti,
                at,
                accounts: out,
            }
        }
        Content::Categories(categories) => {
            let at = target.at.unwrap_or(b.topsheet.len()).min(b.topsheet.len());
            let mut taken_cats: BTreeSet<String> = b.categories().map(|c| c.code.clone()).collect();
            let mut taken = account_codes(b);
            let mut out = Vec::with_capacity(categories.len());
            for c in categories {
                let cat_code = free_category_code(&b.account_coding, &c.code, &taken_cats)?;
                taken_cats.insert(cat_code.clone());
                let mut accounts = Vec::with_capacity(c.accounts.len());
                for a in &c.accounts {
                    let new_code =
                        free_account_code(&b.account_coding, &cat_code, &a.code, &taken)?;
                    taken.insert(new_code.clone());
                    accounts.push(Account {
                        code: new_code,
                        name: a.name.clone(),
                        details: a.details.iter().map(&mut fresh).collect(),
                    });
                }
                out.push(Category {
                    code: cat_code,
                    name: c.name.clone(),
                    accounts,
                });
            }
            Placed::Categories {
                at,
                categories: out,
            }
        }
    };

    // Nothing can fail from here on.
    for (kind, from, to) in &writes {
        write(b, &clip.setup, *kind, from, to, &renames);
    }
    let mut pasted = Pasted::default();
    match placed {
        Placed::Rows { ti, ai, at, rows } => {
            pasted.rows = rows.iter().map(|d| d.id).collect();
            if let TopsheetRow::Category(c) = &mut b.topsheet[ti] {
                c.accounts[ai].details.splice(at..at, rows);
            }
        }
        Placed::Accounts { ti, at, accounts } => {
            pasted.rows = accounts
                .iter()
                .flat_map(|a| &a.details)
                .map(|d| d.id)
                .collect();
            pasted.accounts = accounts.iter().map(|a| a.code.clone()).collect();
            if let TopsheetRow::Category(c) = &mut b.topsheet[ti] {
                c.accounts.splice(at..at, accounts);
            }
        }
        Placed::Categories { at, categories } => {
            pasted.rows = categories
                .iter()
                .flat_map(|c| &c.accounts)
                .flat_map(|a| &a.details)
                .map(|d| d.id)
                .collect();
            pasted.accounts = categories
                .iter()
                .flat_map(|c| &c.accounts)
                .map(|a| a.code.clone())
                .collect();
            pasted.categories = (at..at + categories.len()).collect();
            b.topsheet
                .splice(at..at, categories.into_iter().map(TopsheetRow::Category));
        }
    }
    Ok(pasted)
}

// ---------------------------------------------------------------------------
// Commands (registered in app.rs, build.rs and capabilities/main.json)

/// `{pick: {what, rowIds | accounts | categories}, text, onClipboard}`:
/// copies whole rows, accounts or categories into the app's clipboard.
/// `text` is what the page put on the system clipboard; `onClipboard` false
/// means it couldn't, so only Paste Rows finds the copy.
#[tauri::command]
pub fn clip_copy(
    state: State<'_, AppState>,
    clipboard: State<'_, ClipboardState>,
    doc: Option<DocId>,
    pick: Pick,
    text: String,
    on_clipboard: bool,
) -> Result<ClipInfo, String> {
    let clip = {
        let s = state.lock_doc(doc)?;
        let content = take(s.budget(), &pick)?;
        Clip::new(s.budget(), content, on_clipboard.then_some(text.as_str()))
    };
    let info = clip.info();
    clipboard.set(clip);
    Ok(info)
}

/// Like `clip_copy`, then deletes what was copied, as one undo step.
#[tauri::command]
pub fn clip_cut<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    clipboard: State<'_, ClipboardState>,
    doc: Option<DocId>,
    pick: Pick,
    text: String,
    on_clipboard: bool,
) -> Result<View, String> {
    let mut s = state.lock_doc(doc)?;
    let content = take(s.budget(), &pick)?;
    let clip = Clip::new(s.budget(), content, on_clipboard.then_some(text.as_str()));
    s.edit(|b| cut(b, &pick))?;
    clipboard.set(clip);
    Ok(finish(&window, &mut s, None))
}

/// `{text}`: the review for pasting the copied rows here, or `null` when a
/// paste of `text` isn't theirs (nothing copied, or something else copied
/// since: the page pastes `text` as cells). `text` `null` (Paste Rows):
/// whatever the system clipboard holds.
#[tauri::command]
pub fn clip_preview(
    state: State<'_, AppState>,
    clipboard: State<'_, ClipboardState>,
    doc: Option<DocId>,
    text: Option<String>,
) -> Result<Option<Preview>, String> {
    // Never hold both locks: take the clip, then the budget.
    let Some(clip) = clipboard.for_paste(text.as_deref()) else {
        return Ok(None);
    };
    let s = state.lock_doc(doc)?;
    Ok(Some(preview(s.budget(), &clip)))
}

/// `{token, target, resolutions}`: pastes the reviewed copy, as one undo
/// step. Refused when another copy has replaced it since the review.
#[tauri::command]
pub fn clip_paste<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    clipboard: State<'_, ClipboardState>,
    doc: Option<DocId>,
    token: u64,
    target: Target,
    resolutions: Vec<Resolution>,
) -> Result<View, String> {
    let clip = clipboard
        .current()
        .filter(|c| c.token == token)
        .ok_or("Something else was copied since. Paste again.")?;
    let mut s = state.lock_doc(doc)?;
    let pasted = s.edit(|b| paste(b, &clip, &target, &resolutions))?;
    let first = match clip.content {
        Content::Rows(_) => pasted.rows.first().copied(),
        _ => None,
    };
    Ok(finish(&window, &mut s, first))
}

/// Forgets the copied rows (cells were copied instead).
#[tauri::command]
pub fn clip_forget(clipboard: State<'_, ClipboardState>) {
    clipboard.forget();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Session;
    use fwb_core::{starter, Dec};

    // Starter rows: 6 Line producer and 7 1st AD in 20-01, 8 and 9 in 21-01.

    /// The starter budget with tags on 20-01's two lines, and a setup
    /// table entry of every kind.
    fn source() -> Budget {
        let mut b = starter::budget();
        b.title = "Source".into();
        for (n, f) in [
            ("DAYS", "20"),
            ("WEEKS", "DAYS / 5"),
            ("FICA_RATE", "7.65"),
            ("UNUSED", "1"),
        ] {
            b.globals.insert(n.into(), f.into());
        }
        b.fringes.insert(
            "FICA".into(),
            Fringe {
                description: "Social security".into(),
                rate: "FICA_RATE".into(),
                kind: FringeKind::Percent {
                    cutoff: Some(16_860_000),
                },
            },
        );
        b.fringes.insert(
            "KIT".into(),
            Fringe {
                description: String::new(),
                rate: "50".into(),
                kind: FringeKind::Flat {
                    unit: Some("Shift".into()),
                },
            },
        );
        b.units.insert(
            "Shift".into(),
            UnitDef {
                plural: "Shifts".into(),
                description: String::new(),
                hours: Some(Dec::from_int(12)),
            },
        );
        b.groups.insert(
            "OT".into(),
            Group {
                description: "Overtime".into(),
                in_budget_total: true,
                color: Some("#aa0000".into()),
            },
        );
        b.sets.insert(
            "Kitchen".into(),
            SetDef {
                description: "Int. kitchen".into(),
            },
        );
        b.locations.insert(
            "Atlanta".into(),
            Location {
                description: String::new(),
                city: "Atlanta".into(),
                region: "GA".into(),
                country: "US".into(),
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
        let rows = &mut account_mut(&mut b, "20-01").details;
        let lp = &mut rows[0];
        lp.fringes = ["PAYROLL".to_string(), "FICA".to_string()].into();
        lp.groups = ["OT".to_string()].into();
        lp.set = Some("Kitchen".into());
        lp.location = Some("Atlanta".into());
        lp.amount = "WEEKS + 1".into();
        lp.bold = true;
        lp.note = Some("Prep and shoot".into());
        lp.fringe_range = Some(1);
        let ad = &mut rows[1];
        ad.fringes = ["FICA".to_string(), "KIT".to_string()].into();
        ad.unit = "Shift".into();
        ad.x = "DAYS".into();
        ad.currency = Some("EUR".into());
        ad.fringe_range = Some(1);
        ad.indent = 2;
        ad.color = Some("#123456".into());
        b
    }

    fn account_mut<'a>(b: &'a mut Budget, code: &str) -> &'a mut Account {
        b.categories_mut()
            .flat_map(|c| &mut c.accounts)
            .find(|a| a.code == code)
            .unwrap()
    }

    fn account<'a>(b: &'a Budget, code: &str) -> &'a Account {
        b.categories()
            .flat_map(|c| &c.accounts)
            .find(|a| a.code == code)
            .unwrap()
    }

    fn ids(b: &Budget, code: &str) -> Vec<RowId> {
        account(b, code).details.iter().map(|d| d.id).collect()
    }

    fn rows(ids: &[RowId]) -> Pick {
        Pick {
            what: "rows".into(),
            row_ids: ids.to_vec(),
            ..Pick::default()
        }
    }

    fn copy(b: &Budget, pick: &Pick) -> Clip {
        Clip::new(b, take(b, pick).unwrap(), Some("copied text"))
    }

    fn into_account(code: &str, before: Option<RowId>) -> Target {
        Target {
            account: Some(code.into()),
            before_row: before,
            ..Target::default()
        }
    }

    fn res(kind: &str, name: &str, choice: &str) -> Resolution {
        Resolution {
            kind: kind.into(),
            name: name.into(),
            choice: choice.into(),
        }
    }

    /// Every row id in the budget is different.
    fn unique_ids(b: &Budget) -> bool {
        let all: Vec<RowId> = edit::details(b).map(|d| d.id).collect();
        all.len() == all.iter().collect::<BTreeSet<_>>().len()
    }

    /// A row as pasted, compared with the one copied: everything but the id
    /// and the fringe range number.
    fn same_but_id(a: &Detail, b: &Detail) -> bool {
        let mut a = a.clone();
        a.id = b.id;
        a.fringe_range = b.fringe_range;
        &a == b
    }

    #[test]
    fn a_copy_carries_everything_its_rows_use() {
        let b = source();
        let clip = copy(&b, &rows(&[6, 7]));
        let names = |m: Vec<&String>| m.into_iter().cloned().collect::<Vec<_>>();
        let s = &clip.setup;
        assert_eq!(
            names(s.fringes.keys().collect()),
            ["FICA", "KIT", "PAYROLL"]
        );
        assert_eq!(names(s.groups.keys().collect()), ["OT"]);
        // Units of the rows (Week, Shift) and of the flat fringe (Shift).
        assert_eq!(names(s.units.keys().collect()), ["Shift", "Week"]);
        // Globals in the formulas, those they use, and the fringe rate's,
        // but nothing unused.
        assert_eq!(
            names(s.globals.keys().collect()),
            [
                "DAYS",
                "FICA_RATE",
                "PREP_WEEKS",
                "SHOOT_DAYS",
                "SHOOT_WEEKS",
                "WEEKS"
            ]
        );
        assert_eq!(names(s.sets.keys().collect()), ["Kitchen"]);
        assert_eq!(names(s.locations.keys().collect()), ["Atlanta"]);
        assert_eq!(names(s.currencies.keys().collect()), ["EUR"]);
        assert_eq!(clip.info().count, 2);
        assert_eq!(clip.content.details()[0], &account(&b, "20-01").details[0]);
    }

    #[test]
    fn global_names_are_read_token_by_token() {
        let names: Vec<String> = global_names("A + A_1*2.5 - (B1/_c) 3x")
            .into_iter()
            .collect();
        assert_eq!(names, ["A", "A_1", "B1", "_c", "x"]);
        let r = BTreeMap::from([("A".to_string(), "A_2".to_string())]);
        assert_eq!(
            rename_globals("A + A_1 * AA - (A)", &r),
            "A_2 + A_1 * AA - (A_2)"
        );
        assert_eq!(rename_globals("  A  ", &r), "  A_2  ");
        assert_eq!(rename_globals("é + A", &r), "é + A_2");
    }

    #[test]
    fn paste_within_an_account_lands_above_the_cursor_as_one_undo_step() {
        let mut s = Session::with_budget(source(), None);
        let before = s.budget().clone();
        let clip = copy(s.budget(), &rows(&[6, 7]));
        let p = preview(s.budget(), &clip);
        assert!(!p.review, "nothing to review in the same budget: {p:?}");
        assert_eq!(p.same, clip.setup.fringes.len() + 1 + 6 + 2 + 1 + 1 + 1);

        let pasted = s
            .edit(|b| paste(b, &clip, &into_account("20-01", Some(7)), &[]))
            .unwrap();
        let acct = account(s.budget(), "20-01");
        // 6, then the two copies, then 7 (above the cursor row, 7).
        assert_eq!(acct.details.len(), 4);
        assert_eq!(acct.details[0].id, 6);
        assert_eq!(acct.details[3].id, 7);
        assert_eq!(pasted.rows, [acct.details[1].id, acct.details[2].id]);
        assert!(same_but_id(&before_row(&before, 6), &acct.details[1]));
        assert!(same_but_id(&before_row(&before, 7), &acct.details[2]));
        // The account already has range 1: the copies form range 2.
        assert_eq!(acct.details[1].fringe_range, Some(2));
        assert_eq!(acct.details[2].fringe_range, Some(2));
        assert!(unique_ids(s.budget()));
        // Setup untouched: everything was the same.
        assert_eq!(s.budget().fringes, before.fringes);

        // One undo takes it all back.
        assert!(s.undo());
        assert_eq!(s.budget(), &before);
        assert!(!s.can_undo());
    }

    fn before_row(b: &Budget, id: RowId) -> Detail {
        edit::details(b).find(|d| d.id == id).cloned().unwrap()
    }

    #[test]
    fn paste_into_another_account_goes_at_the_end_or_above_a_row() {
        let mut b = source();
        // 21-01 has its own range 1 and 2.
        for (d, n) in account_mut(&mut b, "21-01").details.iter_mut().zip([1, 2]) {
            d.fringe_range = Some(n);
        }
        let clip = copy(&b, &rows(&[6, 7]));
        let mut at_end = b.clone();
        paste(&mut at_end, &clip, &into_account("21-01", None), &[]).unwrap();
        let a = account(&at_end, "21-01");
        assert_eq!(a.details.len(), 4);
        assert_eq!(&ids(&at_end, "21-01")[..2], [8, 9]);
        assert_eq!(a.details[2].fringe_range, Some(3));
        assert_eq!(a.details[3].fringe_range, Some(3));

        let mut above = b.clone();
        paste(&mut above, &clip, &into_account("21-01", Some(8)), &[]).unwrap();
        assert_eq!(&ids(&above, "21-01")[2..], [8, 9]);
        // The source account is unchanged.
        assert_eq!(account(&above, "20-01"), account(&b, "20-01"));

        // A row that isn't in the account, or an account that's gone: refused.
        let mut c = b.clone();
        assert!(paste(&mut c, &clip, &into_account("21-01", Some(6)), &[]).is_err());
        assert!(paste(&mut c, &clip, &into_account("99-99", None), &[]).is_err());
        assert!(paste(&mut c, &clip, &Target::default(), &[]).is_err());
        assert_eq!(c, b);
    }

    #[test]
    fn paste_into_another_budget_adds_what_is_missing() {
        // Two sessions: two budgets (two tabs, later).
        let src = Session::with_budget(source(), None);
        let mut dst = Session::new();
        let original = dst.budget().clone();
        let clip = copy(src.budget(), &rows(&[6, 7]));
        let p = preview(dst.budget(), &clip);
        assert!(p.review);
        assert!(p.notes.is_empty());
        let added: Vec<(&str, &str)> = p.items.iter().map(|i| (i.kind, i.name.as_str())).collect();
        assert_eq!(
            added,
            [
                ("fringe", "FICA"),
                ("fringe", "KIT"),
                ("group", "OT"),
                ("global", "DAYS"),
                ("global", "FICA_RATE"),
                ("global", "WEEKS"),
                ("unit", "Shift"),
                ("set", "Kitchen"),
                ("location", "Atlanta"),
                ("currency", "EUR"),
            ]
        );
        assert!(p
            .items
            .iter()
            .all(|i| i.status == "add" && i.here.is_none()));
        // PAYROLL, Week, PREP_WEEKS, SHOOT_DAYS, SHOOT_WEEKS are the same.
        assert_eq!(p.same, 5);
        let fica = p.items.iter().find(|i| i.name == "FICA").unwrap();
        assert_eq!(fica.copied, "FICA_RATE% up to 168,600.00; Social security");

        let pasted = dst
            .edit(|b| paste(b, &clip, &into_account("20-01", None), &[]))
            .unwrap();
        let b = dst.budget();
        assert_eq!(b.fringes["FICA"], src.budget().fringes["FICA"]);
        assert_eq!(b.globals["WEEKS"], Formula::from("DAYS / 5"));
        assert_eq!(b.units["Shift"], src.budget().units["Shift"]);
        assert_eq!(b.currencies["EUR"], src.budget().currencies["EUR"]);
        assert!(!b.globals.contains_key("UNUSED"));
        // The pasted rows are the copied rows, under new ids.
        let src_rows = &account(src.budget(), "20-01").details;
        for (id, want) in pasted.rows.iter().zip(src_rows) {
            assert!(same_but_id(want, &before_row(b, *id)));
        }
        assert!(unique_ids(b));
        // And the budget totals with no problems.
        assert!(dst.outcome().error.is_none(), "{:?}", dst.outcome().error);
        assert!(dst.undo());
        assert_eq!(dst.budget(), &original);
        assert!(!dst.can_undo());
    }

    /// A target that has FICA, OT and DAYS, each defined differently.
    fn conflicted() -> Budget {
        let mut b = starter::budget();
        b.title = "Target".into();
        b.fringes.insert(
            "FICA".into(),
            Fringe {
                description: "Social security".into(),
                rate: "6.2".into(),
                kind: FringeKind::Percent { cutoff: None },
            },
        );
        b.groups.insert(
            "OT".into(),
            Group {
                description: "Overtime".into(),
                in_budget_total: false,
                color: None,
            },
        );
        b.globals.insert("DAYS".into(), "30".into());
        // Taken already: the new name for DAYS skips it.
        b.globals.insert("DAYS_2".into(), "1".into());
        // A line here already uses FICA.
        account_mut(&mut b, "50-01").details[0].fringes = ["FICA".to_string()].into();
        b
    }

    #[test]
    fn differences_are_found_and_the_target_wins_by_default() {
        let src = source();
        let mut b = conflicted();
        let clip = copy(&src, &rows(&[6, 7]));
        let p = preview(&b, &clip);
        let differs: Vec<(&str, &str, Option<&str>)> = p
            .items
            .iter()
            .filter(|i| i.status == "differs")
            .map(|i| (i.kind, i.name.as_str(), i.rename_to.as_deref()))
            .collect();
        assert_eq!(
            differs,
            [
                ("fringe", "FICA", Some("FICA (2)")),
                ("group", "OT", Some("OT (2)")),
                ("global", "DAYS", Some("DAYS_3")),
            ]
        );
        let fica = p.items.iter().find(|i| i.name == "FICA").unwrap();
        assert_eq!(
            fica.here.as_deref(),
            Some("6.2% with no cutoff; Social security")
        );
        // Additions come before differences.
        let first_differs = p.items.iter().position(|i| i.status == "differs").unwrap();
        assert!(p.items[..first_differs].iter().all(|i| i.status == "add"));
        assert!(p.items[first_differs..]
            .iter()
            .all(|i| i.status == "differs"));

        let before = b.clone();
        paste(&mut b, &clip, &into_account("20-01", None), &[]).unwrap();
        // The target's own definitions stay; the rows use them by name.
        assert_eq!(b.fringes["FICA"], before.fringes["FICA"]);
        assert_eq!(b.groups["OT"], before.groups["OT"]);
        assert_eq!(b.globals["DAYS"], Formula::from("30"));
        // WEEKS was missing: added as copied, so it reads this budget's DAYS.
        assert_eq!(b.globals["WEEKS"], Formula::from("DAYS / 5"));
        let pasted = &account(&b, "20-01").details[2..];
        assert!(pasted[0].fringes.contains("FICA"));
        assert_eq!(pasted[1].x, Formula::from("DAYS"));
        // Choosing "keep" says the same thing.
        let mut kept = before.clone();
        paste(
            &mut kept,
            &clip,
            &into_account("20-01", None),
            &[res("fringe", "FICA", "keep")],
        )
        .unwrap();
        assert_eq!(kept.fringes, b.fringes);
    }

    #[test]
    fn the_copied_version_can_replace_the_targets() {
        let src = source();
        let mut b = conflicted();
        let clip = copy(&src, &rows(&[6, 7]));
        paste(
            &mut b,
            &clip,
            &into_account("20-01", None),
            &[
                res("fringe", "FICA", "copied"),
                res("global", "DAYS", "copied"),
                res("group", "OT", "copied"),
            ],
        )
        .unwrap();
        assert_eq!(b.fringes["FICA"], src.fringes["FICA"]);
        assert_eq!(b.globals["DAYS"], Formula::from("20"));
        assert_eq!(b.groups["OT"], src.groups["OT"]);
        // No new names.
        assert!(!b.fringes.contains_key("FICA (2)"));
    }

    #[test]
    fn a_new_name_is_used_by_the_pasted_rows_and_what_they_bring() {
        let src = source();
        let mut b = conflicted();
        let clip = copy(&src, &rows(&[6, 7]));
        let before = b.clone();
        paste(
            &mut b,
            &clip,
            &into_account("20-01", None),
            &[
                res("fringe", "FICA", "rename"),
                res("global", "DAYS", "rename"),
                res("group", "OT", "rename"),
            ],
        )
        .unwrap();
        // The target's own items are untouched.
        assert_eq!(b.fringes["FICA"], before.fringes["FICA"]);
        assert_eq!(b.globals["DAYS"], Formula::from("30"));
        assert_eq!(b.groups["OT"], before.groups["OT"]);
        // The copied ones are here under new names.
        assert_eq!(b.fringes["FICA (2)"], src.fringes["FICA"]);
        assert_eq!(b.groups["OT (2)"], src.groups["OT"]);
        assert_eq!(b.globals["DAYS_3"], Formula::from("20"));
        // Formulas that named DAYS now name DAYS_3: a pasted cell, and a
        // global the paste added.
        assert_eq!(b.globals["WEEKS"], Formula::from("DAYS_3 / 5"));
        let pasted = &account(&b, "20-01").details[2..];
        assert_eq!(pasted[1].x, Formula::from("DAYS_3"));
        assert_eq!(pasted[0].amount, Formula::from("WEEKS + 1"));
        assert!(pasted[0].fringes.contains("FICA (2)") && !pasted[0].fringes.contains("FICA"));
        assert!(pasted[0].groups.contains("OT (2)"));
        // The line already here keeps the budget's own FICA.
        assert!(account(&b, "50-01").details[0].fringes.contains("FICA"));
    }

    #[test]
    fn bad_choices_are_refused_and_change_nothing() {
        let clip = copy(&source(), &rows(&[6, 7]));
        let b = conflicted();
        for bad in [
            res("fringes", "FICA", "keep"),
            res("fringe", "FICA", "both"),
            res("currency", "EUR", "rename"),
        ] {
            let mut c = b.clone();
            assert!(paste(&mut c, &clip, &into_account("20-01", None), &[bad]).is_err());
            assert_eq!(c, b);
        }
    }

    #[test]
    fn new_ids_are_past_every_id_the_budget_has_used() {
        let mut b = source();
        // A version holds row 500: new rows must not reuse it.
        let mut snap = b.snapshot();
        if let TopsheetRow::Category(c) = &mut snap.topsheet[0] {
            c.accounts[0].details[0].id = 500;
        }
        b.versions.push(fwb_core::Version {
            id: 1,
            name: "v1".into(),
            created: "2026-10-09T00:00:00Z".into(),
            note: None,
            budget: std::sync::Arc::new(snap),
        });
        let clip = copy(&b, &rows(&[6, 7]));
        let first = paste(&mut b, &clip, &into_account("20-01", None), &[]).unwrap();
        let second = paste(&mut b, &clip, &into_account("20-01", None), &[]).unwrap();
        assert_eq!(first.rows, [501, 502]);
        assert_eq!(second.rows, [503, 504]);
        assert!(unique_ids(&b));
        // Each paste is its own range.
        let ranges: Vec<Option<u32>> = account(&b, "20-01")
            .details
            .iter()
            .map(|d| d.fringe_range)
            .collect();
        assert_eq!(
            ranges,
            [Some(1), Some(1), Some(2), Some(2), Some(3), Some(3)]
        );
    }

    #[test]
    fn accounts_paste_with_their_rows_under_free_numbers() {
        let mut s = Session::with_budget(source(), None);
        let pick = Pick {
            what: "accounts".into(),
            accounts: vec!["20-01".into(), "21-01".into()],
            ..Pick::default()
        };
        let clip = copy(s.budget(), &pick);
        assert_eq!(clip.info().lines, 4);
        let before = s.budget().clone();
        // Into category 20, above 20-01: 20-01 is taken, so 20-02 and 20-03.
        let target = Target {
            category: Some("20".into()),
            before_account: Some("20-01".into()),
            ..Target::default()
        };
        let pasted = s.edit(|b| paste(b, &clip, &target, &[])).unwrap();
        assert_eq!(pasted.accounts, ["20-02", "20-03"]);
        let cat: Vec<&str> = s
            .budget()
            .categories()
            .nth(4)
            .unwrap()
            .accounts
            .iter()
            .map(|a| a.code.as_str())
            .collect();
        assert_eq!(cat, ["20-02", "20-03", "20-01"]);
        let copy_of = account(s.budget(), "20-02");
        assert_eq!(copy_of.name, "Production staff");
        assert_eq!(copy_of.details.len(), 2);
        assert!(same_but_id(&before_row(&before, 6), &copy_of.details[0]));
        // A whole account keeps its own range numbers.
        assert_eq!(copy_of.details[0].fringe_range, Some(1));
        assert!(unique_ids(s.budget()));
        assert!(s.undo());
        assert_eq!(s.budget(), &before);

        // Into category 40 (which has 40-01): the account segment is kept
        // under 40 when free.
        let mut b = before.clone();
        let into_40 = Target {
            category: Some("40".into()),
            ..Target::default()
        };
        let p = paste(&mut b, &clip, &into_40, &[]).unwrap();
        assert_eq!(p.accounts, ["40-02", "40-03"]);
        let mut b = before.clone();
        let into_50 = Target {
            category: Some("50".into()),
            ..Target::default()
        };
        // 50-01 is taken, so the first goes to 50-02; the second wants
        // 50-01 too, then 50-02 is taken by the first.
        assert_eq!(
            paste(&mut b, &clip, &into_50, &[]).unwrap().accounts,
            ["50-02", "50-03"]
        );
    }

    #[test]
    fn categories_paste_with_their_accounts_under_free_numbers() {
        let mut s = Session::with_budget(source(), None);
        let ti = s
            .budget()
            .topsheet
            .iter()
            .position(|r| matches!(r, TopsheetRow::Category(c) if c.code == "20"))
            .unwrap();
        let pick = Pick {
            what: "categories".into(),
            // A production total in the selection is passed over.
            categories: vec![ti, ti - 1],
            ..Pick::default()
        };
        let clip = copy(s.budget(), &pick);
        assert_eq!(clip.info().count, 1);
        let before = s.budget().clone();
        let end = before.topsheet.len();
        let pasted = s
            .edit(|b| paste(b, &clip, &Target::default(), &[]))
            .unwrap();
        // 20, 21 and 22 are taken: 23, with its account as 23-01.
        assert_eq!(pasted.categories, [end]);
        assert_eq!(pasted.accounts, ["23-01"]);
        let TopsheetRow::Category(c) = &s.budget().topsheet[end] else {
            panic!("a category was pasted");
        };
        assert_eq!(
            (c.code.as_str(), c.name.as_str()),
            ("23", "Production staff")
        );
        // Again, at the top: 24.
        let again = s
            .edit(|b| {
                paste(
                    b,
                    &clip,
                    &Target {
                        at: Some(0),
                        ..Target::default()
                    },
                    &[],
                )
            })
            .unwrap();
        assert_eq!(again.categories, [0]);
        assert_eq!(again.accounts, ["24-01"]);
        assert!(unique_ids(s.budget()));
        assert!(s.undo() && s.undo());
        assert_eq!(s.budget(), &before);

        // Only production totals: nothing to copy.
        let pt = Pick {
            what: "categories".into(),
            categories: vec![ti - 1],
            ..Pick::default()
        };
        assert!(take(s.budget(), &pt).is_err());
    }

    #[test]
    fn free_numbers() {
        let coding = AccountCoding::default();
        let taken: BTreeSet<String> = ["20-01", "20-02", "20-99"].map(String::from).into();
        let free = |cat: &str, code: &str| free_account_code(&coding, cat, code, &taken).unwrap();
        assert_eq!(free("20", "11-05"), "20-05");
        assert_eq!(free("20", "11-01"), "20-03");
        assert_eq!(free("20", "20-99"), "20-03", "wraps round");
        assert_eq!(free("30", "11-01"), "30-01");
        // A sub-account counts on in its own segment.
        let sub = AccountCoding {
            sub_width: 2,
            ..AccountCoding::default()
        };
        let t: BTreeSet<String> = ["20-01-01".to_string()].into();
        assert_eq!(
            free_account_code(&sub, "20", "20-01-01", &t).unwrap(),
            "20-01-02"
        );
        // Categories.
        let cats: BTreeSet<String> = ["10", "11", "99"].map(String::from).into();
        assert_eq!(free_category_code(&coding, "10", &cats).unwrap(), "12");
        assert_eq!(free_category_code(&coding, "99", &cats).unwrap(), "01");
        assert_eq!(free_category_code(&coding, "5", &cats).unwrap(), "05");
        // A full category has no number left.
        let full: BTreeSet<String> = (1..=99).map(|n| format!("20-{n:02}")).collect();
        assert!(free_account_code(&coding, "20", "20-01", &full).is_err());
        // Plain counting when the coding can't read the code.
        let bad = AccountCoding {
            category_width: 0,
            ..AccountCoding::default()
        };
        let t: BTreeSet<String> = ["A9".to_string(), "X".to_string()].into();
        assert_eq!(free_category_code(&bad, "A9", &t).unwrap(), "A10");
        assert_eq!(free_category_code(&bad, "X", &t).unwrap(), "X2");
        assert_eq!(free_category_code(&bad, "Q", &t).unwrap(), "Q");
    }

    #[test]
    fn cut_is_a_copy_then_one_undoable_delete() {
        let mut s = Session::with_budget(source(), None);
        let before = s.budget().clone();
        let pick = rows(&[6]);
        let clip = copy(s.budget(), &pick);
        s.edit(|b| cut(b, &pick)).unwrap();
        assert_eq!(ids(s.budget(), "20-01"), [7]);
        // The paste is a step of its own.
        s.edit(|b| paste(b, &clip, &into_account("21-01", None), &[]))
            .unwrap();
        assert_eq!(account(s.budget(), "21-01").details.len(), 3);
        assert!(s.undo());
        assert_eq!(ids(s.budget(), "20-01"), [7]);
        assert!(s.undo());
        assert_eq!(s.budget(), &before);

        // Accounts and categories cut whole; production totals stay.
        let mut b = before.clone();
        let accts = Pick {
            what: "accounts".into(),
            accounts: vec!["20-01".into()],
            ..Pick::default()
        };
        cut(&mut b, &accts).unwrap();
        assert!(find_account(&b, "20-01").is_none());
        let mut b = before.clone();
        let n = b.topsheet.len();
        let cats = Pick {
            what: "categories".into(),
            categories: vec![0, 4],
            ..Pick::default()
        };
        cut(&mut b, &cats).unwrap();
        assert_eq!(b.topsheet.len(), n - 1, "the production total at 4 stays");
        // Something gone: refused, nothing deleted.
        let mut c = before.clone();
        assert!(cut(&mut c, &rows(&[6, 9999])).is_err());
        assert_eq!(c, before);
    }

    #[test]
    fn the_clipboard_is_keyed_by_the_text_it_put_out() {
        let b = source();
        let state = ClipboardState::default();
        assert!(state.for_paste(None).is_none());
        let content = take(&b, &rows(&[6])).unwrap();
        state.set(Clip::new(&b, content.clone(), Some("Line producer\t1\n")));
        // The same text back, line endings changed: the rows.
        assert!(state.for_paste(Some("Line producer\t1\r\n")).is_some());
        assert!(state.for_paste(None).is_some());
        // Other text: something else was copied since. Forgotten.
        assert!(state.for_paste(Some("from a spreadsheet")).is_none());
        assert!(state.for_paste(None).is_none());

        // Text that never reached the clipboard: only Paste Rows finds it,
        // and a cell paste doesn't forget it.
        state.set(Clip::new(&b, content.clone(), None));
        assert!(state.for_paste(Some("anything")).is_none());
        assert!(state.for_paste(None).is_some());
        // Empty text is never a key.
        state.set(Clip::new(&b, content, Some("\n")));
        assert!(state.for_paste(Some("")).is_none());
        state.forget();
        assert!(state.current().is_none());
    }

    #[test]
    fn the_base_currency_counts_as_none() {
        let mut src = source();
        // A line marked with its own budget's base currency.
        account_mut(&mut src, "21-01").details[0].currency = Some("USD".into());
        let clip = copy(&src, &rows(&[7, 8]));
        assert_eq!(clip.content.details()[1].currency, None);

        // Pasted where EUR is the base: EUR lines become base lines.
        let mut euro = starter::budget();
        euro.base_currency = "EUR".into();
        let p = preview(&euro, &clip);
        assert!(p.items.iter().all(|i| i.kind != "currency"));
        assert_eq!(p.notes.len(), 1, "{:?}", p.notes);
        assert!(p.notes[0].starts_with("1 line in USD"), "{}", p.notes[0]);
        let pasted = paste(&mut euro, &clip, &into_account("20-01", None), &[]).unwrap();
        assert!(pasted
            .rows
            .iter()
            .all(|id| before_row(&euro, *id).currency.is_none()));
        assert!(!euro.currencies.contains_key("EUR"));
    }

    /// Budgets open as tabs share the one clipboard: rows copied in one tab
    /// paste into another, with the review against that tab's setup, and
    /// each tab keeps its own undo.
    #[test]
    fn one_clipboard_serves_every_tab() {
        let state = crate::docs::AppState::with(crate::docs::Docs::with(Session::with_budget(
            source(),
            None,
        )));
        let clipboard = ClipboardState::default();
        let a = state.docs().active();
        let b = state.docs().add(Session::with_budget(conflicted(), None));
        // Copied in A while B is in front (a command naming its tab).
        {
            let s = state.lock_doc(Some(a)).unwrap();
            let content = take(s.budget(), &rows(&[6, 7])).unwrap();
            clipboard.set(Clip::new(s.budget(), content, Some("Line producer\n")));
        }
        let before_a = state.docs().get(a).unwrap().budget().clone();
        // Pasted in B, as the commands do it: the clip first, then the budget.
        let clip = clipboard.for_paste(Some("Line producer\n")).unwrap();
        let mut s = state.lock();
        assert_eq!(s.id(), b);
        let review = preview(s.budget(), &clip);
        assert!(review.review, "B's FICA differs from A's");
        assert_eq!(review.source, "Source");
        let pasted = s
            .edit(|bud| paste(bud, &clip, &into_account("20-01", None), &[]))
            .unwrap();
        assert_eq!(pasted.rows.len(), 2);
        drop(s);
        assert_eq!(
            state.docs().get(a).unwrap().budget(),
            &before_a,
            "the source is untouched"
        );
        assert!(!state.docs().get(a).unwrap().can_undo());
        // Undo in B takes the paste back, and only there.
        assert!(state.lock().undo());
        assert!(!state.lock().can_undo());
        assert_eq!(state.lock().budget(), &conflicted());
        // The clip is still there for a third tab.
        let c = state.docs().add(Session::new());
        assert!(clipboard.for_paste(None).is_some());
        assert_eq!(state.lock().id(), c);
    }

    #[test]
    fn previews_are_plain_json() {
        let clip = copy(&source(), &rows(&[6, 7]));
        let v = serde_json::to_value(preview(&conflicted(), &clip)).unwrap();
        assert_eq!(v["what"], "rows");
        assert_eq!(v["count"], 2);
        assert_eq!(v["source"], "Source");
        assert_eq!(v["review"], true);
        let fica = v["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["name"] == "FICA")
            .unwrap();
        assert_eq!(fica["status"], "differs");
        assert_eq!(fica["rename_to"], "FICA (2)");
        let pick: Pick = serde_json::from_str(r#"{"what": "rows", "rowIds": [6, 7]}"#).unwrap();
        assert_eq!(pick.row_ids, [6, 7]);
        let t: Target = serde_json::from_str(r#"{"account": "20-01", "beforeRow": 7}"#).unwrap();
        assert_eq!(t.before_row, Some(7));
        assert!(serde_json::from_str::<Target>(r#"{"row": 7}"#).is_err());
    }
}
