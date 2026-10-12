//! What changed between two budgets, in plain words (roadmap 3.1;
//! decisions.md, entry 44).
//!
//! [`diff`] compares an older budget `a` with a newer one `b` by stable
//! identity, never by position:
//! - detail rows by row id (row ids survive edits and moves; docs/FORMAT.md);
//! - accounts and categories by code, falling back to the rows an account
//!   holds (or the accounts a category holds) when a code changed, then to
//!   the name, so a renumbered account reads as renumbered rather than as
//!   removed and added;
//! - production totals, setup items (globals, fringes, groups, units,
//!   currencies, sets, locations, budget info), charges and credits by name;
//!   saved versions by id.
//!
//! The result lists every change in budget order (budget settings, setup,
//! the topsheet, then account by account in `b`'s order, accounts that went
//! last, then versions), each with its fields' before and after values and a
//! sentence such as `21-01 line 3: rate 1,200.00 -> 1,350.00 (+1,500.00 to
//! 21-01)`, plus the money that moved per account, category and topsheet
//! line. Money comes from recalculating both budgets ([`diff`]) or from
//! reports the caller already has ([`diff_with`]; the application passes its
//! lenient totals, so a budget with a bad cell still shows money).
//!
//! Layout and tools are not content (as for versions: display settings,
//! report, tag and line presets, setup order and folders, the provenance),
//! so they are not listed.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::fmt::Write as _;

use crate::coding::AccountCoding;
use crate::model::{
    Account, Arithmetic, Baseline, Budget, Charge, ChargeKind, Credit, CreditCap, CreditSource,
    Currency, CutoffFactor, Detail, FlatBasis, Fringe, FringeKind, FringePosting, FringeRow,
    GlobalInfo, Group, GroupRules, Inclusion, InfoValue, Location, NewLineDefaults, Note, RowId,
    RowKind, SetDef, TopsheetRow, TrailingFringes, UnitDef, Version,
};
use crate::report::Report;
use crate::{recalc, Cents, Dec, RecalcOptions};

// ---------------------------------------------------------------------------
// What a diff is

/// What happened to the thing a [`DiffItem`] is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeKind {
    Added,
    Removed,
    /// Fields changed in place.
    Changed,
    /// Moved: a row to another line or account, an account to another place
    /// or category, a category on the topsheet. Field changes may come too.
    Moved,
    Renamed,
    Renumbered,
}

impl ChangeKind {
    /// `"added"`, `"removed"`...: for the application.
    pub fn key(self) -> &'static str {
        match self {
            ChangeKind::Added => "added",
            ChangeKind::Removed => "removed",
            ChangeKind::Changed => "changed",
            ChangeKind::Moved => "moved",
            ChangeKind::Renamed => "renamed",
            ChangeKind::Renumbered => "renumbered",
        }
    }
}

/// A setup table, in the order a diff lists them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Table {
    Info,
    Global,
    Fringe,
    Group,
    Unit,
    Currency,
    Set,
    Location,
    Charge,
    Credit,
}

impl Table {
    /// `"fringe"`: in sentences, and the application's key.
    pub fn singular(self) -> &'static str {
        match self {
            Table::Info => "info",
            Table::Global => "global",
            Table::Fringe => "fringe",
            Table::Group => "group",
            Table::Unit => "unit",
            Table::Currency => "currency",
            Table::Set => "set",
            Table::Location => "location",
            Table::Charge => "charge",
            Table::Credit => "credit",
        }
    }

    /// `"Fringes"`: a heading.
    pub fn title(self) -> &'static str {
        match self {
            Table::Info => "Budget info",
            Table::Global => "Globals",
            Table::Fringe => "Fringes",
            Table::Group => "Groups",
            Table::Unit => "Units",
            Table::Currency => "Currencies",
            Table::Set => "Sets",
            Table::Location => "Locations",
            Table::Charge => "Charges",
            Table::Credit => "Credits",
        }
    }

    /// `"Fringe"`: the start of a sentence.
    pub(crate) fn word(self) -> &'static str {
        match self {
            Table::Info => "Info",
            Table::Global => "Global",
            Table::Fringe => "Fringe",
            Table::Group => "Group",
            Table::Unit => "Unit",
            Table::Currency => "Currency",
            Table::Set => "Set",
            Table::Location => "Location",
            Table::Charge => "Charge",
            Table::Credit => "Credit",
        }
    }
}

/// Where a change is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Place {
    /// Budget-wide settings: title, currencies, account numbers, fringe
    /// posting, new-line defaults, the original (baseline) totals.
    Budget,
    /// An item of a setup table, by name (empty for the table's order).
    Setup { table: Table, name: String },
    /// A category (`code` set) or a production total (`code` none) on the
    /// topsheet; `ti` is its place in the newer budget's topsheet, when it
    /// is there.
    Topsheet {
        code: Option<String>,
        name: String,
        ti: Option<usize>,
    },
    /// An account, or a row in it (`row`, with its 1-based `line` there).
    /// The category, code and name are the newer budget's (the older one's
    /// for an account that went); `at` is (topsheet index, index in its
    /// category) in the newer budget, when it is there.
    Account {
        category: String,
        code: String,
        name: String,
        at: Option<(usize, usize)>,
        row: Option<RowId>,
        line: Option<usize>,
    },
    /// The saved versions.
    Versions,
}

impl Place {
    /// The heading a list groups this change under: "Budget", "Fringes",
    /// "Topsheet", "21-01 Camera", "Versions".
    pub fn group(&self) -> String {
        match self {
            Place::Budget => "Budget".into(),
            Place::Setup { table, .. } => table.title().into(),
            Place::Topsheet { .. } => "Topsheet".into(),
            Place::Account { code, name, .. } => {
                let name = name.trim();
                if name.is_empty() {
                    code.trim().to_string()
                } else {
                    format!("{} {name}", code.trim())
                }
            }
            Place::Versions => "Versions".into(),
        }
    }
}

/// One field's change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldChange {
    /// The file-format key (`"rate"`, `"in_budget_total"`, `"notes"`).
    pub field: &'static str,
    /// As a person says it (`"rate"`, `"4X"`, `"in budget total"`).
    pub label: &'static str,
    pub before: String,
    pub after: String,
    /// The phrase: `rate 1,200.00 -> 1,350.00`, `fringes +FICA -WGA`,
    /// `notes: 1 added`.
    pub text: String,
}

/// One change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffItem {
    pub kind: ChangeKind,
    pub place: Place,
    /// What it is about: a row's description, an account's, category's or
    /// setup item's name.
    pub subject: String,
    /// Field by field, for changed (and moved) things.
    pub fields: Vec<FieldChange>,
    /// The money this change moves by itself, when known: a row's own
    /// total (newer minus older), or an added or removed account's or
    /// category's total.
    pub money: Option<Cents>,
    /// The whole change in plain words.
    pub text: String,
}

/// A total before and after.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoneyLine {
    /// `"21-01 Camera"`, `"20 Production"`, `"Grand total"`.
    pub label: String,
    pub before: Option<Cents>,
    pub after: Option<Cents>,
    /// After minus before, a missing side counting as 0.
    pub delta: Cents,
    /// Where it is (accounts and categories), to go there.
    pub place: Option<Place>,
}

/// Everything that changed from one budget to another.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BudgetDiff {
    pub items: Vec<DiffItem>,
    /// Accounts (and category fringe accounts) whose total changed, in the
    /// newer budget's order, then those that went.
    pub accounts: Vec<MoneyLine>,
    /// Categories whose total changed, the same way.
    pub categories: Vec<MoneyLine>,
    /// Topsheet lines that changed: production totals, above and below the
    /// line, fringes, each charge, the grand total, each credit, the net.
    pub totals: Vec<MoneyLine>,
    /// Whether both budgets could be totalled. When not, no money is given.
    pub money_known: bool,
}

impl BudgetDiff {
    /// No change at all, and no money moved.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
            && self.accounts.is_empty()
            && self.categories.is_empty()
            && self.totals.is_empty()
    }

    /// The diff as text for a terminal: changes grouped under their
    /// headings, then the money.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        if self.items.is_empty() {
            out.push_str("No changes.\n");
        } else {
            let n = self.items.len();
            let _ = writeln!(out, "{n} change{}", if n == 1 { "" } else { "s" });
            let mut last = String::new();
            for item in &self.items {
                let g = item.place.group();
                if g != last {
                    let _ = writeln!(out, "\n{g}");
                    last = g;
                }
                let _ = writeln!(out, "  {}", item.text);
            }
        }
        let mut section = |title: &str, lines: &[MoneyLine]| {
            if lines.is_empty() {
                return;
            }
            let _ = writeln!(out, "\n{title}");
            for l in lines {
                let _ = writeln!(
                    out,
                    "  {:<36}{:>16} -> {:>16}  {:>15}",
                    l.label,
                    l.before.map(money).unwrap_or_else(|| "none".into()),
                    l.after.map(money).unwrap_or_else(|| "none".into()),
                    signed(l.delta)
                );
            }
        };
        section("Topsheet", &self.totals);
        section("Categories", &self.categories);
        section("Accounts", &self.accounts);
        if !self.money_known {
            out.push_str("\nTotals left out: a budget can't be totalled.\n");
        }
        out
    }
}

/// What changed from `a` (older) to `b` (newer). Both are recalculated for
/// the money; one that can't be totalled leaves the money out.
pub fn diff(a: &Budget, b: &Budget) -> BudgetDiff {
    let opts = RecalcOptions::default();
    let ra = recalc(a, &opts).ok();
    let rb = recalc(b, &opts).ok();
    diff_with(a, ra.as_ref(), b, rb.as_ref())
}

/// [`diff`] with totals the caller has: `ra` must be a report of `a` (or of
/// a copy with the same rows, such as one with bad cells blanked), `rb` of
/// `b`. `None` leaves the money out.
pub fn diff_with(a: &Budget, ra: Option<&Report>, b: &Budget, rb: Option<&Report>) -> BudgetDiff {
    let sa = Side::new(a, ra);
    let sb = Side::new(b, rb);
    let m = match_topsheets(&a.topsheet, &b.topsheet);
    let mut items = Vec::new();
    settings(a, b, &mut items);
    setup(a, b, &mut items);
    topsheet(&sa, &sb, &m, &mut items);
    accounts(&sa, &sb, &m, &mut items);
    versions(&a.versions, &b.versions, &mut items);
    let money_known = sa.totals.is_some() && sb.totals.is_some();
    let (accounts, categories, totals) = if money_known {
        money_lines(&sa, &sb, &m, ra, rb)
    } else {
        Default::default()
    };
    BudgetDiff {
        items,
        accounts,
        categories,
        totals,
        money_known,
    }
}

// ---------------------------------------------------------------------------
// Words and numbers

/// `123456` cents as `1,234.56`; negatives with a minus.
pub use crate::calc::explain::money;

/// `+1,234.56`, `-1,234.56`, `0.00`.
pub fn signed(c: Cents) -> String {
    if c > 0 {
        format!("+{}", money(c))
    } else {
        money(c)
    }
}

/// A formula as a person reads it: `blank`, the text as typed, or (for
/// money-like fields such as a rate) a plain number with thousands
/// separators and at least two decimals (`1,200.00`).
pub(crate) fn formula_text(f: &str, money_like: bool) -> String {
    let t = f.trim();
    if t.is_empty() {
        return "blank".into();
    }
    if money_like {
        if let Ok(d) = t.parse::<crate::Dec>() {
            return crate::calc::explain::dmoney(d);
        }
    }
    t.to_string()
}

pub(crate) fn quoted(s: &str) -> String {
    if s.trim().is_empty() {
        "blank".into()
    } else {
        format!("\"{s}\"")
    }
}

pub(crate) fn opt_text(s: &Option<String>) -> String {
    match s {
        Some(v) if !v.trim().is_empty() => v.clone(),
        _ => "none".into(),
    }
}

pub(crate) fn on_off(b: bool) -> String {
    if b { "on" } else { "off" }.into()
}

fn count_text(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

fn set_text(s: &BTreeSet<String>) -> String {
    if s.is_empty() {
        "none".into()
    } else {
        s.iter().cloned().collect::<Vec<_>>().join(", ")
    }
}

/// A field's value: plain text, or a set of names (shown as `+A -B`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Value {
    Text(String),
    Set(BTreeSet<String>),
}

/// One field of something: its key, how a person says it, its value.
pub(crate) struct Field {
    pub key: &'static str,
    pub label: &'static str,
    pub value: Value,
}

fn text(key: &'static str, label: &'static str, v: impl Into<String>) -> Field {
    Field {
        key,
        label,
        value: Value::Text(v.into()),
    }
}

fn names(key: &'static str, label: &'static str, v: &BTreeSet<String>) -> Field {
    Field {
        key,
        label,
        value: Value::Set(v.clone()),
    }
}

/// The phrase for one field going from `a` to `b`, or `None` when equal.
fn field_change(
    key: &'static str,
    label: &'static str,
    a: &Value,
    b: &Value,
) -> Option<FieldChange> {
    if a == b {
        return None;
    }
    Some(match (a, b) {
        (Value::Set(x), Value::Set(y)) => {
            let mut parts: Vec<String> = y.difference(x).map(|n| format!("+{n}")).collect();
            parts.extend(x.difference(y).map(|n| format!("-{n}")));
            FieldChange {
                field: key,
                label,
                before: set_text(x),
                after: set_text(y),
                text: format!("{label} {}", parts.join(" ")),
            }
        }
        _ => {
            let before = value_text(a);
            let after = value_text(b);
            FieldChange {
                field: key,
                label,
                text: format!("{label} {before} -> {after}"),
                before,
                after,
            }
        }
    })
}

fn value_text(v: &Value) -> String {
    match v {
        Value::Text(t) => t.clone(),
        Value::Set(s) => set_text(s),
    }
}

/// Field-by-field changes between two lists of the same fields.
pub(crate) fn field_changes(a: &[Field], b: &[Field]) -> Vec<FieldChange> {
    a.iter()
        .zip(b)
        .filter_map(|(x, y)| field_change(x.key, x.label, &x.value, &y.value))
        .collect()
}

/// The fields worth saying about a new item: those with a value.
pub(crate) fn summary(fields: &[Field]) -> String {
    fields
        .iter()
        .filter_map(|f| {
            let v = value_text(&f.value);
            (!matches!(v.as_str(), "" | "none" | "blank" | "off" | "as calculated"))
                .then(|| format!("{} {v}", f.label))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Notes going from `a` to `b`, summed up: `notes: 1 added, 2 edited`.
pub(crate) fn notes_change(a: &[Note], b: &[Note]) -> Option<FieldChange> {
    if a == b {
        return None;
    }
    let am: BTreeMap<u64, &Note> = a.iter().map(|n| (n.id, n)).collect();
    let bm: BTreeMap<u64, &Note> = b.iter().map(|n| (n.id, n)).collect();
    let added = bm.keys().filter(|k| !am.contains_key(k)).count();
    let removed = am.keys().filter(|k| !bm.contains_key(k)).count();
    let (mut edited, mut ticked, mut unticked) = (0, 0, 0);
    for (id, x) in &am {
        let Some(y) = bm.get(id) else { continue };
        if x == y {
            continue;
        }
        if x.done != y.done
            && (Note {
                done: x.done,
                ..(*y).clone()
            }) == **x
        {
            if y.done {
                ticked += 1;
            } else {
                unticked += 1;
            }
        } else {
            edited += 1;
        }
    }
    let mut parts = Vec::new();
    for (n, word) in [
        (added, "added"),
        (removed, "removed"),
        (edited, "edited"),
        (ticked, "ticked off"),
        (unticked, "unticked"),
    ] {
        if n > 0 {
            parts.push(format!("{n} {word}"));
        }
    }
    if parts.is_empty() {
        parts.push("reordered".into());
    }
    Some(FieldChange {
        field: "notes",
        label: "notes",
        before: count_text(a.len(), "note", "notes"),
        after: count_text(b.len(), "note", "notes"),
        text: format!("notes: {}", parts.join(", ")),
    })
}

// ---------------------------------------------------------------------------
// Fields of each kind of thing (shared with crate::merge for conflicts)

pub(crate) fn kind_name(k: RowKind) -> &'static str {
    match k {
        RowKind::Line => "line",
        RowKind::Subtotal => "subtotal",
        RowKind::Adjustment => "adjustment",
        RowKind::Separator => "separator",
    }
}

/// A detail row's fields in file order, notes left out (see
/// [`notes_change`]).
pub(crate) fn row_fields(d: &Detail) -> Vec<Field> {
    vec![
        text("kind", "kind", kind_name(d.kind)),
        text("description", "description", quoted(&d.description)),
        text("amount", "amount", formula_text(&d.amount.0, false)),
        text(
            "unit",
            "unit",
            if d.unit.is_empty() { "none" } else { &d.unit },
        ),
        text("x", "X", formula_text(&d.x.0, false)),
        text(
            "unit2",
            "X unit",
            if d.unit2.is_empty() { "none" } else { &d.unit2 },
        ),
        text("rate", "rate", formula_text(&d.rate.0, true)),
        text(
            "unit3",
            "rate unit",
            if d.unit3.is_empty() { "none" } else { &d.unit3 },
        ),
        text("x4", "4X", formula_text(&d.x4.0, false)),
        text(
            "unit4",
            "4X unit",
            if d.unit4.is_empty() { "none" } else { &d.unit4 },
        ),
        text(
            "currency",
            "currency",
            match &d.currency {
                None => "base".to_string(),
                Some(c) if c.is_empty() => "blank".to_string(),
                Some(c) => c.clone(),
            },
        ),
        text(
            "fringe_range",
            "fringe range",
            d.fringe_range.map_or("none".into(), |n| n.to_string()),
        ),
        names("fringes", "fringes", &d.fringes),
        names("groups", "groups", &d.groups),
        text("set", "set", opt_text(&d.set)),
        text("location", "location", opt_text(&d.location)),
        text("bold", "bold", on_off(d.bold)),
        text("italic", "italic", on_off(d.italic)),
        text("indent", "indent", d.indent.to_string()),
        text("color", "color", opt_text(&d.color)),
    ]
}

/// A row's field changes, notes last.
pub(crate) fn row_changes(a: &Detail, b: &Detail) -> Vec<FieldChange> {
    let mut out = field_changes(&row_fields(a), &row_fields(b));
    out.extend(notes_change(&a.notes, &b.notes));
    out
}

/// How a row is named when it is added or removed: `"Dolly grip"`, a
/// subtotal row, a separator.
pub(crate) fn row_name(d: &Detail) -> String {
    let desc = d.description.trim();
    let named = |what: &str| {
        if desc.is_empty() {
            what.to_string()
        } else {
            format!("{what} \"{desc}\"")
        }
    };
    match d.kind {
        RowKind::Line if desc.is_empty() => "a blank line".into(),
        RowKind::Line => format!("\"{desc}\""),
        RowKind::Subtotal => named("a subtotal row"),
        RowKind::Adjustment => named("an adjustment row"),
        RowKind::Separator => "a separator".into(),
    }
}

/// A flat fringe's basis as the diff says it.
pub(crate) fn basis_text(b: FlatBasis) -> String {
    match b {
        FlatBasis::Amount => "amount only",
        FlatBasis::AmountCounts => "amount plus counts in X and 4X",
        FlatBasis::Quantity => "full quantity",
    }
    .into()
}

/// Most hours a day, or none.
pub(crate) fn max_hours_text(h: &Option<Dec>) -> String {
    h.map_or("none".into(), |h| h.to_string())
}

pub(crate) fn fringe_fields(f: &Fringe) -> Vec<Field> {
    let none = || "none".to_string();
    let (kind, unit, basis, max_hours) = match &f.kind {
        FringeKind::Percent { .. } => ("percent", none(), none(), none()),
        FringeKind::Flat {
            unit,
            basis,
            max_hours_per_day,
            ..
        } => (
            "flat",
            opt_text(unit),
            basis_text(*basis),
            max_hours_text(max_hours_per_day),
        ),
    };
    // A flat fringe's cutoff is kept but never used; it still shows here.
    let cutoff = f.kind.cutoff().map_or_else(none, money);
    vec![
        text("description", "description", quoted(&f.description)),
        text("short_code", "short code", opt_text(&f.short_code)),
        text("kind", "kind", kind),
        text("rate", "rate", formula_text(&f.rate.0, false)),
        text("cutoff", "cutoff", cutoff),
        text("unit", "flat unit", unit),
        text("basis", "basis", basis),
        text("max_hours_per_day", "max hours a day", max_hours),
    ]
}

pub(crate) fn group_fields(g: &Group) -> Vec<Field> {
    vec![
        text("description", "description", quoted(&g.description)),
        text("short_code", "short code", opt_text(&g.short_code)),
        text(
            "in_budget_total",
            "in budget total",
            if g.in_budget_total { "yes" } else { "no" },
        ),
        text("color", "color", opt_text(&g.color)),
    ]
}

/// A global: its formula and its display info.
pub(crate) fn global_fields(f: Option<&crate::Formula>, i: Option<&GlobalInfo>) -> Vec<Field> {
    let default = GlobalInfo::default();
    let i = i.unwrap_or(&default);
    vec![
        text(
            "value",
            "value",
            f.map_or("none".into(), |f| formula_text(&f.0, false)),
        ),
        text("description", "description", quoted(&i.description)),
        text(
            "unit",
            "unit",
            if i.unit.is_empty() { "none" } else { &i.unit },
        ),
        text(
            "decimals",
            "decimals",
            i.decimals.map_or("as calculated".into(), |d| d.to_string()),
        ),
        text("show_by_name", "show by name", on_off(i.show_by_name)),
        text(
            "folder",
            "folder",
            if i.folder.is_empty() {
                "none"
            } else {
                &i.folder
            },
        ),
    ]
}

pub(crate) fn unit_fields(u: &UnitDef) -> Vec<Field> {
    vec![
        text("plural", "plural", quoted(&u.plural)),
        text("description", "description", quoted(&u.description)),
        text(
            "hours",
            "hours",
            u.hours.map_or("none".into(), |h| h.to_string()),
        ),
    ]
}

pub(crate) fn currency_fields(c: &Currency) -> Vec<Field> {
    vec![
        text("description", "description", quoted(&c.description)),
        text("symbol", "symbol", quoted(&c.symbol)),
        text("rate", "rate", c.rate.to_string()),
        text("format", "format", currency_format_text(&c.format)),
    ]
}

/// A currency's format in words: `1,234.56, symbol before` (decisions.md,
/// entry 61).
pub(crate) fn currency_format_text(f: &crate::CurrencyFormat) -> String {
    let pattern = match f.separators {
        crate::Separators::CommaPoint => "1,234.56",
        crate::Separators::PointComma => "1.234,56",
    };
    let place = if f.symbol_after {
        "symbol after"
    } else {
        "symbol before"
    };
    format!("{pattern}, {place}")
}

pub(crate) fn set_fields(s: &SetDef) -> Vec<Field> {
    vec![text("description", "description", quoted(&s.description))]
}

pub(crate) fn location_fields(l: &Location) -> Vec<Field> {
    vec![
        text("description", "description", quoted(&l.description)),
        text("city", "city", quoted(&l.city)),
        text("region", "region", quoted(&l.region)),
        text("country", "country", quoted(&l.country)),
    ]
}

pub(crate) fn info_fields(v: &InfoValue) -> Vec<Field> {
    vec![text(
        "value",
        "value",
        match v {
            InfoValue::Text(t) => quoted(t),
            InfoValue::Number(d) => d.to_string(),
            InfoValue::Date(s) => s.clone(),
        },
    )]
}

pub(crate) fn charge_fields(c: &Charge) -> Vec<Field> {
    let (kind, percent, amount, currency) = match &c.kind {
        ChargeKind::Percent(p) => ("percent", p.to_string(), "none".into(), "none".into()),
        ChargeKind::Flat { amount, currency } => {
            ("flat", "none".into(), money(*amount), opt_text(currency))
        }
    };
    let amounts = if c.excluded_amounts.is_empty() {
        "none".to_string()
    } else {
        c.excluded_amounts
            .iter()
            .map(|(d, a)| format!("{} {}", quoted(d), money(*a)))
            .collect::<Vec<_>>()
            .join("; ")
    };
    vec![
        text("applied", "applied", on_off(c.applied)),
        text("kind", "kind", kind),
        text("percent", "percent", percent),
        text("amount", "amount", amount),
        text("currency", "currency", currency),
        text("account", "account", opt_text(&c.account)),
        text("excluded_amounts", "excluded amounts", amounts),
        names("excluded_groups", "excluded groups", &c.excluded_groups),
        names(
            "excluded_categories",
            "excluded categories",
            &c.excluded_categories,
        ),
        names(
            "excluded_accounts",
            "excluded accounts",
            &c.excluded_accounts,
        ),
        text("label_percent", "percent in label", on_off(c.label_percent)),
        text(
            "label_excluded",
            "excluded in label",
            on_off(c.label_excluded),
        ),
    ]
}

pub(crate) fn credit_rows_text(c: &Credit) -> String {
    if c.rows.is_empty() {
        return "none".into();
    }
    c.rows
        .iter()
        .map(|r| {
            let source = match &r.source {
                CreditSource::Flat(a) => money(*a),
                CreditSource::Group { group } => format!("group {group}"),
                CreditSource::Category { category } => format!("category {category}"),
                CreditSource::Account { account } => format!("account {account}"),
            };
            let cap = match r.cap {
                CreditCap::None => String::new(),
                CreditCap::QualifiedSpend(a) => format!(", spend capped at {}", money(a)),
                CreditCap::Credit(a) => format!(", credit capped at {}", money(a)),
            };
            format!(
                "{}: {}% of {source}{cap}",
                quoted(&r.description),
                r.percent
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

pub(crate) fn credit_fields(c: &Credit) -> Vec<Field> {
    vec![
        text("applied", "applied", on_off(c.applied)),
        text("account", "account", opt_text(&c.account)),
        text("cap", "cap", c.cap.map_or("none".into(), money)),
        text(
            "discount_percent",
            "discount",
            format!("{}%", c.discount_percent),
        ),
        names("excluded_fringes", "excluded fringes", &c.excluded_fringes),
        text("rows", "rows", credit_rows_text(c)),
    ]
}

pub(crate) fn coding_text(c: &AccountCoding) -> String {
    let sep = c.separator.map(String::from).unwrap_or_default();
    [c.category_width, c.account_width, c.sub_width, c.set_width]
        .iter()
        .filter(|w| **w > 0)
        .map(|w| "0".repeat(usize::from(*w)))
        .collect::<Vec<_>>()
        .join(&sep)
}

pub(crate) fn posting_text(p: &FringePosting) -> String {
    match p {
        FringePosting::Budget => "one line for the budget".into(),
        FringePosting::ProductionLevel => "a line above each production total".into(),
        FringePosting::Category { suffix } => {
            format!("a fringe account per category (suffix {suffix})")
        }
        FringePosting::Account => "a line at the end of each account".into(),
    }
}

pub(crate) fn trailing_text(t: &TrailingFringes) -> String {
    match t {
        TrailingFringes::Keep => "kept after the last production total".into(),
        TrailingFringes::Drop => "dropped after the last production total".into(),
    }
}

pub(crate) fn arithmetic_text(a: &Arithmetic) -> String {
    match a {
        Arithmetic::Mmb => "spreadsheet (binary doubles)".into(),
        Arithmetic::Exact => "exact decimals".into(),
    }
}

pub(crate) fn cutoff_factor_text(f: &CutoffFactor) -> String {
    format!("percent fringe cutoffs x {}", f.name())
}

pub(crate) fn fringe_row_text(r: &FringeRow) -> String {
    let number = r.account().map(|a| format!("{a} ")).unwrap_or_default();
    let place = match r.position {
        None => "after the summary rows".to_string(),
        Some(n) => format!("above topsheet row {}", n + 1),
    };
    format!("{number}{}, {place}", quoted(r.description()))
}

pub(crate) fn group_rules_text(r: &GroupRules) -> String {
    let word = |i: Inclusion| match i {
        Inclusion::Include => "included",
        Inclusion::Exclude => "excluded",
    };
    format!(
        "lines with no group {}, conflicts {}, fringes in group totals {}",
        word(r.ungrouped_lines),
        word(r.conflict),
        word(r.fringes_in_totals)
    )
}

pub(crate) fn defaults_text(d: &NewLineDefaults) -> String {
    let on: Vec<&str> = [
        (d.fringes, "fringes"),
        (d.groups, "groups"),
        (d.set, "set"),
        (d.location, "location"),
        (d.currency, "currency"),
    ]
    .iter()
    .filter(|(on, _)| *on)
    .map(|(_, n)| *n)
    .collect();
    if on.is_empty() {
        "nothing".into()
    } else {
        on.join(", ")
    }
}

pub(crate) fn baseline_text(b: &Option<Baseline>) -> String {
    match b {
        None => "none".into(),
        Some(b) if b.frozen => format!("locked and frozen, grand total {}", money(b.grand_total)),
        Some(b) => format!("locked, grand total {}", money(b.grand_total)),
    }
}

/// Actuals in a few words: `12 transactions from 2 imports`.
pub(crate) fn actuals_text(a: &crate::actuals::Actuals) -> String {
    if a.is_empty() {
        return "none".into();
    }
    format!(
        "{} from {}",
        count_text(a.transactions.len(), "transaction", "transactions"),
        count_text(a.batches.len(), "import", "imports")
    )
}

/// The budget-wide settings as fields.
pub(crate) fn setting_fields(b: &Budget) -> Vec<Field> {
    vec![
        text("title", "title", quoted(&b.title)),
        text("base_currency", "base currency", quoted(&b.base_currency)),
        text(
            "totals_currency",
            "totals currency",
            opt_text(&b.totals_currency),
        ),
        text(
            "account_coding",
            "account numbers",
            coding_text(&b.account_coding),
        ),
        text(
            "fringe_posting",
            "fringe posting",
            posting_text(&b.fringe_posting),
        ),
        text(
            "trailing_level_fringes",
            "trailing level fringes",
            trailing_text(&b.trailing_level_fringes),
        ),
        text(
            "cutoff_factor",
            "cutoff factor",
            cutoff_factor_text(&b.cutoff_factor),
        ),
        text("fringe_row", "fringe row", fringe_row_text(&b.fringe_row)),
        text("arithmetic", "arithmetic", arithmetic_text(&b.arithmetic)),
        text(
            "group_rules",
            "group rules",
            group_rules_text(&b.group_rules),
        ),
        text(
            "new_line_defaults",
            "new lines copy",
            defaults_text(&b.new_line_defaults),
        ),
        text("baseline", "original totals", baseline_text(&b.baseline)),
        text("actuals", "actuals", actuals_text(&b.actuals)),
    ]
}

// ---------------------------------------------------------------------------
// Matching (shared with crate::merge)

/// Where a row is: topsheet index, index in its category, index in its
/// account.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RowAt<'a> {
    pub ti: usize,
    pub ai: usize,
    pub di: usize,
    pub d: &'a Detail,
}

/// Every row by id (the first, if an id is used twice).
pub(crate) fn row_index(ts: &[TopsheetRow]) -> HashMap<RowId, RowAt<'_>> {
    let mut out = HashMap::new();
    for (ti, row) in ts.iter().enumerate() {
        let TopsheetRow::Category(c) = row else {
            continue;
        };
        for (ai, a) in c.accounts.iter().enumerate() {
            for (di, d) in a.details.iter().enumerate() {
                out.entry(d.id).or_insert(RowAt { ti, ai, di, d });
            }
        }
    }
    out
}

/// Every account with where it is (topsheet index, index in its category).
pub(crate) fn account_list(ts: &[TopsheetRow]) -> Vec<((usize, usize), &Account)> {
    let mut out = Vec::new();
    for (ti, row) in ts.iter().enumerate() {
        if let TopsheetRow::Category(c) = row {
            out.extend(c.accounts.iter().enumerate().map(|(ai, a)| ((ti, ai), a)));
        }
    }
    out
}

/// Which accounts and topsheet rows of one budget are which of another's.
#[derive(Clone, Debug, Default)]
pub(crate) struct Matching {
    /// (ti, ai) in `a` to (ti, ai) in `b`.
    pub acct: BTreeMap<(usize, usize), (usize, usize)>,
    pub acct_rev: BTreeMap<(usize, usize), (usize, usize)>,
    /// Topsheet index in `a` to topsheet index in `b`.
    pub top: BTreeMap<usize, usize>,
    pub top_rev: BTreeMap<usize, usize>,
}

impl Matching {
    fn pair_acct(&mut self, a: (usize, usize), b: (usize, usize)) {
        self.acct.insert(a, b);
        self.acct_rev.insert(b, a);
    }

    fn pair_top(&mut self, a: usize, b: usize) {
        self.top.insert(a, b);
        self.top_rev.insert(b, a);
    }
}

/// The key a topsheet row is matched by first: a category's code, a
/// production total's name.
pub(crate) fn top_key(r: &TopsheetRow) -> String {
    match r {
        TopsheetRow::Category(c) => format!("c\u{0}{}", c.code.trim()),
        TopsheetRow::ProductionTotal(p) => format!("p\u{0}{}", p.name.trim()),
    }
}

/// Pairs `items_a` with `items_b` by key, the n-th of a key with the n-th.
pub(crate) fn pair_by_key<P: Copy, K: std::hash::Hash + Eq>(
    items_a: impl IntoIterator<Item = (P, K)>,
    items_b: impl IntoIterator<Item = (P, K)>,
) -> Vec<(P, P)> {
    let mut queue: HashMap<K, VecDeque<P>> = HashMap::new();
    for (p, k) in items_b {
        queue.entry(k).or_default().push_back(p);
    }
    items_a
        .into_iter()
        .filter_map(|(p, k)| {
            queue
                .get_mut(&k)
                .and_then(VecDeque::pop_front)
                .map(|q| (p, q))
        })
        .collect()
}

/// Matches the accounts and topsheet rows of `a` with those of `b`:
/// accounts by code, then by the rows they hold, then by name within
/// matched categories; categories by code, then by the accounts they hold,
/// then by name; production totals by name.
pub(crate) fn match_topsheets(a: &[TopsheetRow], b: &[TopsheetRow]) -> Matching {
    let mut m = Matching::default();
    let al = account_list(a);
    let bl = account_list(b);

    // Accounts by code.
    for (x, y) in pair_by_key(
        al.iter().map(|(p, acc)| (*p, acc.code.trim())),
        bl.iter().map(|(p, acc)| (*p, acc.code.trim())),
    ) {
        m.pair_acct(x, y);
    }
    // Accounts by the rows they hold (a renumbered account).
    let mut home: HashMap<RowId, (usize, usize)> = HashMap::new();
    for (p, acc) in &bl {
        if m.acct_rev.contains_key(p) {
            continue;
        }
        for d in &acc.details {
            home.entry(d.id).or_insert(*p);
        }
    }
    for (p, acc) in &al {
        if m.acct.contains_key(p) {
            continue;
        }
        let mut counts: BTreeMap<(usize, usize), usize> = BTreeMap::new();
        for d in &acc.details {
            if let Some(h) = home.get(&d.id) {
                if !m.acct_rev.contains_key(h) {
                    *counts.entry(*h).or_default() += 1;
                }
            }
        }
        let best = counts
            .iter()
            .max_by(|x, y| x.1.cmp(y.1).then(y.0.cmp(x.0)))
            .map(|(q, _)| *q);
        if let Some(q) = best {
            m.pair_acct(*p, q);
        }
    }

    // Topsheet rows by code or name.
    for (x, y) in pair_by_key(
        a.iter().enumerate().map(|(i, r)| (i, top_key(r))),
        b.iter().enumerate().map(|(i, r)| (i, top_key(r))),
    ) {
        m.pair_top(x, y);
    }
    // Categories by the accounts they hold (a renumbered category).
    for (ti, r) in a.iter().enumerate() {
        if m.top.contains_key(&ti) || !matches!(r, TopsheetRow::Category(_)) {
            continue;
        }
        let mut counts: BTreeMap<usize, usize> = BTreeMap::new();
        for ((ati, _), (bti, _)) in &m.acct {
            if *ati == ti
                && !m.top_rev.contains_key(bti)
                && matches!(b.get(*bti), Some(TopsheetRow::Category(_)))
            {
                *counts.entry(*bti).or_default() += 1;
            }
        }
        let best = counts
            .iter()
            .max_by(|x, y| x.1.cmp(y.1).then(y.0.cmp(x.0)))
            .map(|(q, _)| *q);
        if let Some(q) = best {
            m.pair_top(ti, q);
        }
    }
    // Categories left over, by name.
    let left_a: Vec<(usize, String)> = a
        .iter()
        .enumerate()
        .filter(|(i, _)| !m.top.contains_key(i))
        .filter_map(|(i, r)| match r {
            TopsheetRow::Category(c) => Some((i, c.name.trim().to_lowercase())),
            TopsheetRow::ProductionTotal(_) => None,
        })
        .collect();
    let left_b: Vec<(usize, String)> = b
        .iter()
        .enumerate()
        .filter(|(i, _)| !m.top_rev.contains_key(i))
        .filter_map(|(i, r)| match r {
            TopsheetRow::Category(c) => Some((i, c.name.trim().to_lowercase())),
            TopsheetRow::ProductionTotal(_) => None,
        })
        .collect();
    for (x, y) in pair_by_key(left_a, left_b) {
        m.pair_top(x, y);
    }
    // Accounts left over, by name within matched categories.
    let left_a: Vec<((usize, usize), (usize, String))> = al
        .iter()
        .filter(|(p, _)| !m.acct.contains_key(p))
        .filter_map(|(p, acc)| {
            m.top
                .get(&p.0)
                .map(|bti| (*p, (*bti, acc.name.trim().to_lowercase())))
        })
        .collect();
    let left_b: Vec<((usize, usize), (usize, String))> = bl
        .iter()
        .filter(|(p, _)| !m.acct_rev.contains_key(p))
        .map(|(p, acc)| (*p, (p.0, acc.name.trim().to_lowercase())))
        .collect();
    for (x, y) in pair_by_key(left_a, left_b) {
        m.pair_acct(x, y);
    }
    m
}

/// The elements of `b` that moved relative to `a`: those outside a longest
/// common subsequence of the two (only elements in both count). Elements
/// are unique, so this is a longest increasing subsequence of positions.
pub(crate) fn moved_in<K: Copy + Eq + std::hash::Hash + Ord>(a: &[K], b: &[K]) -> BTreeSet<K> {
    let pos: HashMap<K, usize> = a.iter().enumerate().map(|(i, k)| (*k, i)).collect();
    let seq: Vec<(K, usize)> = b
        .iter()
        .filter_map(|k| pos.get(k).map(|p| (*k, *p)))
        .collect();
    // Patience sorting with back links.
    let mut tails: Vec<usize> = Vec::new(); // index into seq
    let mut prev: Vec<Option<usize>> = vec![None; seq.len()];
    for (i, (_, p)) in seq.iter().enumerate() {
        let at = tails.partition_point(|&t| seq[t].1 < *p);
        if at > 0 {
            prev[i] = Some(tails[at - 1]);
        }
        if at == tails.len() {
            tails.push(i);
        } else {
            tails[at] = i;
        }
    }
    let mut keep = BTreeSet::new();
    let mut cur = tails.last().copied();
    while let Some(i) = cur {
        keep.insert(seq[i].0);
        cur = prev[i];
    }
    seq.iter()
        .map(|(k, _)| *k)
        .filter(|k| !keep.contains(k))
        .collect()
}

// ---------------------------------------------------------------------------
// One side of a diff

struct Totals {
    rows: HashMap<RowId, Cents>,
    accounts: HashMap<(usize, usize), Cents>,
    categories: HashMap<usize, Cents>,
    /// Category fringe accounts: topsheet index to (number, total).
    fringe_accounts: HashMap<usize, (String, Cents)>,
}

struct Side<'a> {
    b: &'a Budget,
    rows: HashMap<RowId, RowAt<'a>>,
    totals: Option<Totals>,
}

impl<'a> Side<'a> {
    fn new(b: &'a Budget, r: Option<&Report>) -> Side<'a> {
        Side {
            b,
            rows: row_index(&b.topsheet),
            totals: r.map(|r| totals_of(b, r)),
        }
    }

    fn account(&self, at: (usize, usize)) -> Option<&'a Account> {
        match self.b.topsheet.get(at.0)? {
            TopsheetRow::Category(c) => c.accounts.get(at.1),
            TopsheetRow::ProductionTotal(_) => None,
        }
    }

    fn category_code(&self, ti: usize) -> String {
        match self.b.topsheet.get(ti) {
            Some(TopsheetRow::Category(c)) => c.code.clone(),
            _ => String::new(),
        }
    }

    fn row_total(&self, id: RowId) -> Option<Cents> {
        let t = self.totals.as_ref()?;
        Some(t.rows.get(&id).copied().unwrap_or(0))
    }

    fn account_total(&self, at: (usize, usize)) -> Option<Cents> {
        self.totals.as_ref()?.accounts.get(&at).copied()
    }
}

fn totals_of(b: &Budget, r: &Report) -> Totals {
    let mut t = Totals {
        rows: HashMap::new(),
        accounts: HashMap::new(),
        categories: HashMap::new(),
        fringe_accounts: HashMap::new(),
    };
    let mut ci = 0;
    for (ti, row) in b.topsheet.iter().enumerate() {
        let TopsheetRow::Category(_) = row else {
            continue;
        };
        let Some(cat) = r.categories.get(ci) else {
            break;
        };
        ci += 1;
        t.categories.insert(ti, cat.total);
        if let Some(fa) = &cat.fringe_account {
            t.fringe_accounts.insert(ti, fa.clone());
        }
        for (ai, acct) in cat.accounts.iter().enumerate() {
            t.accounts.insert((ti, ai), acct.total);
            for row in acct.rows.iter().filter(|r| r.kind.is_costed()) {
                t.rows.insert(row.id, row.total);
            }
        }
    }
    t
}

fn account_place(s: &Side, at: (usize, usize), present: bool) -> Place {
    let acct = s.account(at);
    Place::Account {
        category: s.category_code(at.0),
        code: acct.map(|a| a.code.clone()).unwrap_or_default(),
        name: acct.map(|a| a.name.clone()).unwrap_or_default(),
        at: present.then_some(at),
        row: None,
        line: None,
    }
}

fn row_place(s: &Side, at: (usize, usize), present: bool, id: RowId, line: usize) -> Place {
    match account_place(s, at, present) {
        Place::Account {
            category,
            code,
            name,
            at,
            ..
        } => Place::Account {
            category,
            code,
            name,
            at,
            row: Some(id),
            line: Some(line),
        },
        p => p,
    }
}

// ---------------------------------------------------------------------------
// Budget settings, setup tables and versions

fn settings(a: &Budget, b: &Budget, out: &mut Vec<DiffItem>) {
    let mut fcs = field_changes(&setting_fields(a), &setting_fields(b));
    // Figures that changed without changing how they read: say so.
    let mut same_words = |key: &'static str, label: &'static str, words: String| {
        if !fcs.iter().any(|f| f.field == key) {
            let after = format!("{words} (other figures changed)");
            fcs.push(FieldChange {
                field: key,
                label,
                text: format!("{label} {words} -> {after}"),
                before: words,
                after,
            });
        }
    };
    if a.baseline != b.baseline {
        same_words("baseline", "original totals", baseline_text(&a.baseline));
    }
    if a.actuals != b.actuals {
        same_words("actuals", "actuals", actuals_text(&a.actuals));
    }
    for fc in fcs {
        let mut sentence = fc.text.clone();
        if let Some(first) = sentence.get(..1) {
            sentence = format!("{}{}", first.to_uppercase(), &sentence[1..]);
        }
        out.push(DiffItem {
            kind: ChangeKind::Changed,
            place: Place::Budget,
            subject: fc.label.to_string(),
            fields: vec![fc],
            money: None,
            text: sentence,
        });
    }
}

/// Items of one setup table, by name: added, removed, changed.
fn named_table<V>(
    table: Table,
    a: &BTreeMap<String, V>,
    b: &BTreeMap<String, V>,
    fields: impl Fn(&V) -> Vec<Field>,
    out: &mut Vec<DiffItem>,
) {
    let keys: BTreeSet<&String> = a.keys().chain(b.keys()).collect();
    for k in keys {
        let place = Place::Setup {
            table,
            name: k.clone(),
        };
        let item = match (a.get(k), b.get(k)) {
            (None, Some(v)) => {
                let s = summary(&fields(v));
                DiffItem {
                    kind: ChangeKind::Added,
                    place,
                    subject: k.clone(),
                    fields: Vec::new(),
                    money: None,
                    text: if s.is_empty() {
                        format!("{} {k} added", table.word())
                    } else {
                        format!("{} {k} added: {s}", table.word())
                    },
                }
            }
            (Some(_), None) => DiffItem {
                kind: ChangeKind::Removed,
                place,
                subject: k.clone(),
                fields: Vec::new(),
                money: None,
                text: format!("{} {k} removed", table.word()),
            },
            (Some(x), Some(y)) => {
                let fcs = field_changes(&fields(x), &fields(y));
                if fcs.is_empty() {
                    continue;
                }
                DiffItem {
                    kind: ChangeKind::Changed,
                    text: format!(
                        "{} {k}: {}",
                        table.word(),
                        fcs.iter()
                            .map(|f| f.text.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    place,
                    subject: k.clone(),
                    fields: fcs,
                    money: None,
                }
            }
            (None, None) => continue,
        };
        out.push(item);
    }
}

/// Names for the items of a list matched by name: the name, then `name (2)`,
/// `name (3)` for repeats.
pub(crate) fn list_keys<'a>(names: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    names
        .map(|n| {
            let n = n.trim().to_string();
            let c = seen.entry(n.clone()).or_default();
            *c += 1;
            if *c == 1 {
                n
            } else {
                format!("{n} ({c})")
            }
        })
        .collect()
}

/// A list matched by name (charges, credits), with a change of order.
fn list_table<V>(
    table: Table,
    a: &[V],
    b: &[V],
    name: impl Fn(&V) -> &str,
    fields: impl Fn(&V) -> Vec<Field>,
    out: &mut Vec<DiffItem>,
) {
    let ka = list_keys(a.iter().map(&name));
    let kb = list_keys(b.iter().map(&name));
    let ma: BTreeMap<String, &V> = ka.iter().cloned().zip(a).collect();
    let mb: BTreeMap<String, &V> = kb.iter().cloned().zip(b).collect();
    named_table(table, &ma, &mb, |v: &&V| fields(v), out);
    let common_a: Vec<&String> = ka.iter().filter(|k| mb.contains_key(*k)).collect();
    let common_b: Vec<&String> = kb.iter().filter(|k| ma.contains_key(*k)).collect();
    if common_a != common_b {
        out.push(DiffItem {
            kind: ChangeKind::Moved,
            place: Place::Setup {
                table,
                name: String::new(),
            },
            subject: table.title().to_string(),
            fields: Vec::new(),
            money: None,
            text: format!("{} reordered", table.title()),
        });
    }
}

fn setup(a: &Budget, b: &Budget, out: &mut Vec<DiffItem>) {
    named_table(Table::Info, &a.info, &b.info, info_fields, out);
    // Globals with their display info, as one item each.
    let globals = |x: &Budget| -> BTreeMap<String, (Option<crate::Formula>, Option<GlobalInfo>)> {
        x.globals
            .keys()
            .chain(x.global_info.keys().filter(|k| x.globals.contains_key(*k)))
            .map(|k| {
                (
                    k.clone(),
                    (x.globals.get(k).cloned(), x.global_info.get(k).cloned()),
                )
            })
            .collect()
    };
    named_table(
        Table::Global,
        &globals(a),
        &globals(b),
        |(f, i)| global_fields(f.as_ref(), i.as_ref()),
        out,
    );
    named_table(Table::Fringe, &a.fringes, &b.fringes, fringe_fields, out);
    named_table(Table::Group, &a.groups, &b.groups, group_fields, out);
    named_table(Table::Unit, &a.units, &b.units, unit_fields, out);
    named_table(
        Table::Currency,
        &a.currencies,
        &b.currencies,
        currency_fields,
        out,
    );
    named_table(Table::Set, &a.sets, &b.sets, set_fields, out);
    named_table(
        Table::Location,
        &a.locations,
        &b.locations,
        location_fields,
        out,
    );
    list_table(
        Table::Charge,
        &a.charges,
        &b.charges,
        |c| &c.name,
        charge_fields,
        out,
    );
    list_table(
        Table::Credit,
        &a.credits,
        &b.credits,
        |c| &c.name,
        credit_fields,
        out,
    );
}

fn versions(a: &[Version], b: &[Version], out: &mut Vec<DiffItem>) {
    // A version is the same one when its id and time match.
    let key = |v: &Version| (v.id, v.created.clone());
    let ma: BTreeMap<(u64, String), &Version> = a.iter().map(|v| (key(v), v)).collect();
    let mb: BTreeMap<(u64, String), &Version> = b.iter().map(|v| (key(v), v)).collect();
    let item = |kind, subject: &str, fields, text| DiffItem {
        kind,
        place: Place::Versions,
        subject: subject.to_string(),
        fields,
        money: None,
        text,
    };
    for v in a {
        if !mb.contains_key(&key(v)) {
            out.push(item(
                ChangeKind::Removed,
                &v.name,
                Vec::new(),
                format!("Version \"{}\" deleted", v.name),
            ));
        }
    }
    for v in b {
        match ma.get(&key(v)) {
            None => out.push(item(
                ChangeKind::Added,
                &v.name,
                Vec::new(),
                format!("Version \"{}\" saved", v.name),
            )),
            Some(old) => {
                let fa = [
                    text("name", "name", quoted(&old.name)),
                    text("note", "note", opt_text(&old.note)),
                ];
                let fb = [
                    text("name", "name", quoted(&v.name)),
                    text("note", "note", opt_text(&v.note)),
                ];
                let fcs = field_changes(&fa, &fb);
                if fcs.is_empty() {
                    continue;
                }
                let kind = if old.name != v.name {
                    ChangeKind::Renamed
                } else {
                    ChangeKind::Changed
                };
                let t = format!(
                    "Version \"{}\": {}",
                    old.name,
                    fcs.iter()
                        .map(|f| f.text.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                out.push(item(kind, &v.name, fcs, t));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The topsheet

fn topsheet(sa: &Side, sb: &Side, m: &Matching, out: &mut Vec<DiffItem>) {
    let (a, b) = (&sa.b.topsheet, &sb.b.topsheet);
    let order_a: Vec<usize> = (0..a.len()).filter(|i| m.top.contains_key(i)).collect();
    let order_b: Vec<usize> = (0..b.len())
        .filter_map(|i| m.top_rev.get(&i).copied())
        .collect();
    let moved = moved_in(&order_a, &order_b);
    let cat_money = |s: &Side, ti: usize| {
        s.totals
            .as_ref()
            .and_then(|t| t.categories.get(&ti).copied())
    };

    for (bti, row) in b.iter().enumerate() {
        let place = |r: &TopsheetRow| match r {
            TopsheetRow::Category(c) => Place::Topsheet {
                code: Some(c.code.clone()),
                name: c.name.clone(),
                ti: Some(bti),
            },
            TopsheetRow::ProductionTotal(p) => Place::Topsheet {
                code: None,
                name: p.name.clone(),
                ti: Some(bti),
            },
        };
        let Some(&ati) = m.top_rev.get(&bti) else {
            // Added.
            let (subject, text, money) = match row {
                TopsheetRow::Category(c) => {
                    let money = cat_money(sb, bti);
                    let tail = money
                        .filter(|v| *v != 0)
                        .map(|v| format!(" ({})", signed(v)))
                        .unwrap_or_default();
                    (
                        c.name.clone(),
                        format!("Category {} {} added{tail}", c.code, c.name),
                        money,
                    )
                }
                TopsheetRow::ProductionTotal(p) => (
                    p.name.clone(),
                    format!("Production total {} added", quoted(&p.name)),
                    None,
                ),
            };
            out.push(DiffItem {
                kind: ChangeKind::Added,
                place: place(row),
                subject,
                fields: Vec::new(),
                money,
                text,
            });
            continue;
        };
        if let (TopsheetRow::Category(x), TopsheetRow::Category(y)) = (&a[ati], row) {
            if x.code.trim() != y.code.trim() {
                out.push(DiffItem {
                    kind: ChangeKind::Renumbered,
                    place: place(row),
                    subject: y.name.clone(),
                    fields: vec![FieldChange {
                        field: "code",
                        label: "number",
                        before: x.code.clone(),
                        after: y.code.clone(),
                        text: format!("number {} -> {}", x.code, y.code),
                    }],
                    money: None,
                    text: format!("Category {} renumbered to {}", x.code, y.code),
                });
            }
            if x.name != y.name {
                out.push(DiffItem {
                    kind: ChangeKind::Renamed,
                    place: place(row),
                    subject: y.name.clone(),
                    fields: vec![FieldChange {
                        field: "name",
                        label: "name",
                        before: quoted(&x.name),
                        after: quoted(&y.name),
                        text: format!("name {} -> {}", quoted(&x.name), quoted(&y.name)),
                    }],
                    money: None,
                    text: format!(
                        "Category {} renamed: {} -> {}",
                        y.code,
                        quoted(&x.name),
                        quoted(&y.name)
                    ),
                });
            }
            if let Some(fc) = notes_change(&x.notes, &y.notes) {
                out.push(DiffItem {
                    kind: ChangeKind::Changed,
                    place: place(row),
                    subject: y.name.clone(),
                    text: format!("Category {}: {}", y.code, fc.text),
                    fields: vec![fc],
                    money: None,
                });
            }
        }
        if moved.contains(&ati) {
            let (subject, text) = match row {
                TopsheetRow::Category(c) => (
                    c.name.clone(),
                    format!("Category {} {} moved on the topsheet", c.code, c.name),
                ),
                TopsheetRow::ProductionTotal(p) => (
                    p.name.clone(),
                    format!("Production total {} moved on the topsheet", quoted(&p.name)),
                ),
            };
            out.push(DiffItem {
                kind: ChangeKind::Moved,
                place: place(row),
                subject,
                fields: Vec::new(),
                money: None,
                text,
            });
        }
    }
    for (ati, row) in a.iter().enumerate() {
        if m.top.contains_key(&ati) {
            continue;
        }
        let (place, subject, text, money) = match row {
            TopsheetRow::Category(c) => {
                let money = cat_money(sa, ati).map(|v| -v);
                let tail = money
                    .filter(|v| *v != 0)
                    .map(|v| format!(" ({})", signed(v)))
                    .unwrap_or_default();
                (
                    Place::Topsheet {
                        code: Some(c.code.clone()),
                        name: c.name.clone(),
                        ti: None,
                    },
                    c.name.clone(),
                    format!("Category {} {} removed{tail}", c.code, c.name),
                    money,
                )
            }
            TopsheetRow::ProductionTotal(p) => (
                Place::Topsheet {
                    code: None,
                    name: p.name.clone(),
                    ti: None,
                },
                p.name.clone(),
                format!("Production total {} removed", quoted(&p.name)),
                None,
            ),
        };
        out.push(DiffItem {
            kind: ChangeKind::Removed,
            place,
            subject,
            fields: Vec::new(),
            money,
            text,
        });
    }
}

// ---------------------------------------------------------------------------
// Accounts and rows

/// `" (+1,500.00 to 21-01)"`, or nothing for no money.
fn money_tail(delta: Option<Cents>, word: &str, code: &str) -> String {
    match delta {
        Some(v) if v != 0 => format!(" ({} {word} {})", signed(v), code.trim()),
        _ => String::new(),
    }
}

fn accounts(sa: &Side, sb: &Side, m: &Matching, out: &mut Vec<DiffItem>) {
    let al = account_list(&sa.b.topsheet);
    let bl = account_list(&sb.b.topsheet);

    // Accounts that moved within their (matched) category.
    let mut moved_accounts = BTreeSet::new();
    for (bti, row) in sb.b.topsheet.iter().enumerate() {
        let TopsheetRow::Category(c) = row else {
            continue;
        };
        let Some(&ati) = m.top_rev.get(&bti) else {
            continue;
        };
        let in_b: Vec<(usize, usize)> = (0..c.accounts.len())
            .filter_map(|ai| m.acct_rev.get(&(bti, ai)).copied())
            .filter(|p| p.0 == ati)
            .collect();
        let in_a: Vec<(usize, usize)> = al
            .iter()
            .map(|(p, _)| *p)
            .filter(|p| p.0 == ati && m.acct.get(p).is_some_and(|q| q.0 == bti))
            .collect();
        moved_accounts.extend(moved_in(&in_a, &in_b));
    }

    for (bp, bacc) in &bl {
        let code = bacc.code.trim().to_string();
        let ap = m.acct_rev.get(bp).copied();
        let here = account_place(sb, *bp, true);
        match ap {
            None => {
                let money = sb.account_total(*bp);
                let tail = money
                    .filter(|v| *v != 0)
                    .map(|v| format!(" ({})", signed(v)))
                    .unwrap_or_default();
                out.push(DiffItem {
                    kind: ChangeKind::Added,
                    place: here.clone(),
                    subject: bacc.name.clone(),
                    fields: Vec::new(),
                    money,
                    text: format!("Account {code} {} added{tail}", bacc.name),
                });
            }
            Some(ap) => {
                let Some(aacc) = sa.account(ap) else { continue };
                if aacc.code.trim() != bacc.code.trim() {
                    out.push(DiffItem {
                        kind: ChangeKind::Renumbered,
                        place: here.clone(),
                        subject: bacc.name.clone(),
                        fields: vec![FieldChange {
                            field: "code",
                            label: "number",
                            before: aacc.code.clone(),
                            after: bacc.code.clone(),
                            text: format!("number {} -> {}", aacc.code, bacc.code),
                        }],
                        money: None,
                        text: format!("Account {} renumbered to {code}", aacc.code.trim()),
                    });
                }
                if aacc.name != bacc.name {
                    out.push(DiffItem {
                        kind: ChangeKind::Renamed,
                        place: here.clone(),
                        subject: bacc.name.clone(),
                        fields: vec![FieldChange {
                            field: "name",
                            label: "name",
                            before: quoted(&aacc.name),
                            after: quoted(&bacc.name),
                            text: format!("name {} -> {}", quoted(&aacc.name), quoted(&bacc.name)),
                        }],
                        money: None,
                        text: format!(
                            "Account {code} renamed: {} -> {}",
                            quoted(&aacc.name),
                            quoted(&bacc.name)
                        ),
                    });
                }
                let same_cat = m.top.get(&ap.0) == Some(&bp.0);
                if !same_cat {
                    let from = sa.category_code(ap.0);
                    let to = sb.category_code(bp.0);
                    out.push(DiffItem {
                        kind: ChangeKind::Moved,
                        place: here.clone(),
                        subject: bacc.name.clone(),
                        fields: Vec::new(),
                        money: None,
                        text: format!("Account {code} moved from category {from} to {to}"),
                    });
                } else if moved_accounts.contains(&ap) {
                    out.push(DiffItem {
                        kind: ChangeKind::Moved,
                        place: here.clone(),
                        subject: bacc.name.clone(),
                        fields: Vec::new(),
                        money: None,
                        text: format!(
                            "Account {code} moved within category {}",
                            sb.category_code(bp.0)
                        ),
                    });
                }
                if let Some(fc) = notes_change(&aacc.notes, &bacc.notes) {
                    out.push(DiffItem {
                        kind: ChangeKind::Changed,
                        place: here.clone(),
                        subject: bacc.name.clone(),
                        text: format!("Account {code}: {}", fc.text),
                        fields: vec![fc],
                        money: None,
                    });
                }
            }
        }
        rows_of(sa, sb, m, *bp, ap, out);
    }

    // Accounts that went, with their rows that went.
    for (ap, aacc) in &al {
        if m.acct.contains_key(ap) {
            continue;
        }
        let money = sa.account_total(*ap).map(|v| -v);
        let tail = money
            .filter(|v| *v != 0)
            .map(|v| format!(" ({})", signed(v)))
            .unwrap_or_default();
        out.push(DiffItem {
            kind: ChangeKind::Removed,
            place: account_place(sa, *ap, false),
            subject: aacc.name.clone(),
            fields: Vec::new(),
            money,
            text: format!("Account {} {} removed{tail}", aacc.code.trim(), aacc.name),
        });
        removed_rows(sa, sb, m, *ap, aacc, out);
    }
}

/// The rows of account `bp` in `b` (matched with `ap` in `a`, if it was
/// there): added, moved and changed in `b`'s order, then those that went.
fn rows_of(
    sa: &Side,
    sb: &Side,
    m: &Matching,
    bp: (usize, usize),
    ap: Option<(usize, usize)>,
    out: &mut Vec<DiffItem>,
) {
    let Some(bacc) = sb.account(bp) else { return };
    let code = bacc.code.trim();
    // Rows that stayed in this account but changed places in it.
    let moved = match ap.and_then(|p| sa.account(p).map(|a| (p, a))) {
        Some((p, aacc)) => {
            let in_a: Vec<RowId> = aacc
                .details
                .iter()
                .filter(|d| sb.rows.get(&d.id).is_some_and(|r| (r.ti, r.ai) == bp))
                .map(|d| d.id)
                .collect();
            let in_b: Vec<RowId> = bacc
                .details
                .iter()
                .filter(|d| sa.rows.get(&d.id).is_some_and(|r| (r.ti, r.ai) == p))
                .map(|d| d.id)
                .collect();
            moved_in(&in_a, &in_b)
        }
        None => BTreeSet::new(),
    };
    for (di, d) in bacc.details.iter().enumerate() {
        let line = di + 1;
        let place = row_place(sb, bp, true, d.id, line);
        // A row whose id appears twice is listed once, at its first place.
        if sb
            .rows
            .get(&d.id)
            .is_some_and(|r| (r.ti, r.ai, r.di) != (bp.0, bp.1, di))
        {
            continue;
        }
        let Some(old) = sa.rows.get(&d.id) else {
            let money = sb.row_total(d.id);
            out.push(DiffItem {
                kind: ChangeKind::Added,
                place,
                subject: d.description.clone(),
                fields: Vec::new(),
                money,
                text: format!(
                    "{code} line {line} added: {}{}",
                    row_name(d),
                    money_tail(money, "to", code)
                ),
            });
            continue;
        };
        let fields = row_changes(old.d, d);
        let same_account = m.acct.get(&(old.ti, old.ai)) == Some(&bp);
        let delta = match (sa.row_total(d.id), sb.row_total(d.id)) {
            (Some(x), Some(y)) => Some(y.saturating_sub(x)),
            _ => None,
        };
        let mut parts: Vec<String> = Vec::new();
        let kind = if !same_account {
            let from = sa
                .account(old.ti_ai())
                .map(|a| a.code.trim().to_string())
                .unwrap_or_default();
            parts.push(format!("moved from {from} line {}", old.di + 1));
            ChangeKind::Moved
        } else if moved.contains(&d.id) {
            parts.push(format!("moved from line {}", old.di + 1));
            ChangeKind::Moved
        } else {
            ChangeKind::Changed
        };
        if fields.is_empty() && kind == ChangeKind::Changed {
            continue;
        }
        parts.extend(fields.iter().map(|f| f.text.clone()));
        let tail = if same_account {
            money_tail(delta, "to", code)
        } else {
            let from = sa
                .account(old.ti_ai())
                .map(|a| a.code.trim().to_string())
                .unwrap_or_default();
            match (sa.row_total(d.id), sb.row_total(d.id)) {
                (Some(x), Some(y)) if x != 0 || y != 0 => {
                    format!(" ({} to {code}, {} from {from})", signed(y), signed(-x))
                }
                _ => String::new(),
            }
        };
        out.push(DiffItem {
            kind,
            place,
            subject: d.description.clone(),
            fields,
            money: delta,
            text: format!("{code} line {line}: {}{tail}", parts.join(", ")),
        });
    }
    if let Some((p, aacc)) = ap.and_then(|p| sa.account(p).map(|a| (p, a))) {
        removed_rows(sa, sb, m, p, aacc, out);
    }
}

impl RowAt<'_> {
    fn ti_ai(&self) -> (usize, usize) {
        (self.ti, self.ai)
    }
}

/// The rows of account `ap` in `a` that are nowhere in `b`.
fn removed_rows(
    sa: &Side,
    sb: &Side,
    m: &Matching,
    ap: (usize, usize),
    aacc: &Account,
    out: &mut Vec<DiffItem>,
) {
    let code = aacc.code.trim();
    // Removed rows sit under the account they were in: its place in `b`
    // when it is still there.
    for (di, d) in aacc.details.iter().enumerate() {
        if sb.rows.contains_key(&d.id) {
            continue;
        }
        if sa
            .rows
            .get(&d.id)
            .is_some_and(|r| (r.ti, r.ai, r.di) != (ap.0, ap.1, di))
        {
            continue;
        }
        let money = sa.row_total(d.id).map(|v| -v);
        out.push(DiffItem {
            kind: ChangeKind::Removed,
            place: removed_row_place(sa, sb, m, ap, d.id, di + 1),
            subject: d.description.clone(),
            fields: Vec::new(),
            money,
            text: format!(
                "{code} line {} removed: {}{}",
                di + 1,
                row_name(d),
                money_tail(money, "from", code)
            ),
        });
    }
}

fn removed_row_place(
    sa: &Side,
    sb: &Side,
    m: &Matching,
    ap: (usize, usize),
    id: RowId,
    line: usize,
) -> Place {
    // Grouped under the account as `b` has it, when it is still there, so
    // removed rows sit with the account's other changes.
    match m.acct.get(&ap) {
        Some(bp) => match row_place(sb, *bp, true, id, line) {
            Place::Account {
                category,
                code,
                name,
                at,
                ..
            } => Place::Account {
                category,
                code,
                name,
                at,
                row: Some(id),
                line: Some(line),
            },
            p => p,
        },
        None => row_place(sa, ap, false, id, line),
    }
}

// ---------------------------------------------------------------------------
// Money

type MoneyLists = (Vec<MoneyLine>, Vec<MoneyLine>, Vec<MoneyLine>);

fn line(
    label: String,
    before: Option<Cents>,
    after: Option<Cents>,
    place: Option<Place>,
) -> Option<MoneyLine> {
    let delta = after.unwrap_or(0).saturating_sub(before.unwrap_or(0));
    (delta != 0).then_some(MoneyLine {
        label,
        before,
        after,
        delta,
        place,
    })
}

fn label_of(code: &str, name: &str) -> String {
    let name = name.trim();
    if name.is_empty() {
        code.trim().to_string()
    } else {
        format!("{} {name}", code.trim())
    }
}

fn money_lines(
    sa: &Side,
    sb: &Side,
    m: &Matching,
    ra: Option<&Report>,
    rb: Option<&Report>,
) -> MoneyLists {
    let (Some(ta), Some(tb), Some(ra), Some(rb)) = (&sa.totals, &sb.totals, ra, rb) else {
        return Default::default();
    };
    // Accounts.
    let mut accounts = Vec::new();
    for (bp, bacc) in account_list(&sb.b.topsheet) {
        let before = m
            .acct_rev
            .get(&bp)
            .and_then(|ap| ta.accounts.get(ap).copied());
        let after = tb.accounts.get(&bp).copied();
        accounts.extend(line(
            label_of(&bacc.code, &bacc.name),
            before,
            after,
            Some(account_place(sb, bp, true)),
        ));
    }
    for (ap, aacc) in account_list(&sa.b.topsheet) {
        if m.acct.contains_key(&ap) {
            continue;
        }
        accounts.extend(line(
            label_of(&aacc.code, &aacc.name),
            ta.accounts.get(&ap).copied(),
            None,
            Some(account_place(sa, ap, false)),
        ));
    }
    // Category fringe accounts, with the categories they close.
    let fringe_label = |n: &str| format!("{} Fringes", n.trim());
    for (bti, _) in sb.b.topsheet.iter().enumerate() {
        let after = tb.fringe_accounts.get(&bti);
        let before = m
            .top_rev
            .get(&bti)
            .and_then(|ati| ta.fringe_accounts.get(ati));
        let Some(number) = after.or(before).map(|(n, _)| n.clone()) else {
            continue;
        };
        let place = match sb.b.topsheet.get(bti) {
            Some(TopsheetRow::Category(c)) => Some(Place::Topsheet {
                code: Some(c.code.clone()),
                name: c.name.clone(),
                ti: Some(bti),
            }),
            _ => None,
        };
        accounts.extend(line(
            fringe_label(&number),
            before.map(|x| x.1),
            after.map(|x| x.1),
            place,
        ));
    }
    for (ati, _) in sa.b.topsheet.iter().enumerate() {
        if m.top.contains_key(&ati) {
            continue;
        }
        if let Some((n, v)) = ta.fringe_accounts.get(&ati) {
            accounts.extend(line(fringe_label(n), Some(*v), None, None));
        }
    }

    // Categories.
    let mut categories = Vec::new();
    for (bti, row) in sb.b.topsheet.iter().enumerate() {
        let TopsheetRow::Category(c) = row else {
            continue;
        };
        let before = m
            .top_rev
            .get(&bti)
            .and_then(|ati| ta.categories.get(ati).copied());
        categories.extend(line(
            label_of(&c.code, &c.name),
            before,
            tb.categories.get(&bti).copied(),
            Some(Place::Topsheet {
                code: Some(c.code.clone()),
                name: c.name.clone(),
                ti: Some(bti),
            }),
        ));
    }
    for (ati, row) in sa.b.topsheet.iter().enumerate() {
        let TopsheetRow::Category(c) = row else {
            continue;
        };
        if m.top.contains_key(&ati) {
            continue;
        }
        categories.extend(line(
            label_of(&c.code, &c.name),
            ta.categories.get(&ati).copied(),
            None,
            None,
        ));
    }

    // The topsheet's own lines.
    let mut totals = Vec::new();
    let named = |items: Vec<(String, Cents)>| -> BTreeMap<String, Cents> {
        let keys = list_keys(items.iter().map(|(n, _)| n.as_str()));
        keys.into_iter()
            .zip(items.into_iter().map(|(_, v)| v))
            .collect()
    };
    let levels = |r: &Report| {
        named(
            r.levels
                .iter()
                .filter(|l| !l.name.trim().is_empty())
                .map(|l| (l.name.clone(), l.total))
                .collect(),
        )
    };
    let charges = |r: &Report| {
        named(
            r.charges
                .iter()
                .map(|c| (c.name.clone(), c.amount))
                .collect(),
        )
    };
    let credits = |r: &Report| {
        named(
            r.credits
                .iter()
                .map(|c| (c.name.clone(), c.amount))
                .collect(),
        )
    };
    let both = |label: &str,
                out: &mut Vec<MoneyLine>,
                x: &BTreeMap<String, Cents>,
                y: &BTreeMap<String, Cents>| {
        let keys: Vec<&String> = y
            .keys()
            .chain(x.keys().filter(|k| !y.contains_key(*k)))
            .collect();
        for k in keys {
            out.extend(line(
                if label.is_empty() {
                    k.clone()
                } else {
                    format!("{label} {k}")
                },
                x.get(k).copied(),
                y.get(k).copied(),
                None,
            ));
        }
    };
    both("", &mut totals, &levels(ra), &levels(rb));
    for (label, x, y) in [
        ("Total above-the-line", ra.total_atl, rb.total_atl),
        ("Total below-the-line", ra.total_btl, rb.total_btl),
        ("Total above and below-the-line", ra.atl_btl, rb.atl_btl),
        ("Total fringes", ra.budget_fringes, rb.budget_fringes),
    ] {
        // A production total named like this line already said it.
        let said = totals.iter().any(|l| {
            l.label.eq_ignore_ascii_case(label) && l.before == Some(x) && l.after == Some(y)
        });
        if !said {
            totals.extend(line(label.into(), Some(x), Some(y), None));
        }
    }
    both("", &mut totals, &charges(ra), &charges(rb));
    totals.extend(line(
        "Grand total".into(),
        Some(ra.grand_total),
        Some(rb.grand_total),
        None,
    ));
    both("Less", &mut totals, &credits(ra), &credits(rb));
    totals.extend(line(
        "Net total".into(),
        Some(ra.net_total),
        Some(rb.net_total),
        None,
    ));
    (accounts, categories, totals)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::starter;

    fn first_line(b: &mut Budget) -> &mut Detail {
        b.categories_mut()
            .flat_map(|c| c.accounts.iter_mut())
            .flat_map(|a| a.details.iter_mut())
            .find(|d| d.kind == RowKind::Line && !d.rate.is_blank())
            .unwrap()
    }

    #[test]
    fn money_and_numbers_read_well() {
        assert_eq!(money(150_000), "1,500.00");
        assert_eq!(money(-5), "-0.05");
        assert_eq!(signed(150_000), "+1,500.00");
        assert_eq!(signed(-150_000), "-1,500.00");
        assert_eq!(formula_text("1200", true), "1,200.00");
        assert_eq!(formula_text("1350.5", true), "1,350.50");
        assert_eq!(formula_text("0.125", true), "0.125");
        assert_eq!(formula_text("-1200", true), "-1,200.00");
        assert_eq!(formula_text("WEEKS * 5", true), "WEEKS * 5");
        assert_eq!(formula_text("10", false), "10");
        assert_eq!(formula_text("  ", false), "blank");
    }

    #[test]
    fn same_budget_has_no_changes() {
        let b = starter::budget();
        let d = diff(&b, &b);
        assert!(d.is_empty(), "{d:?}");
        assert!(d.money_known);
        assert_eq!(d.to_text(), "No changes.\n");
    }

    #[test]
    fn a_rate_change_reads_in_plain_words() {
        let a = starter::budget();
        let mut b = a.clone();
        let (code, line, id) = {
            let (code, line, id) = a
                .categories()
                .flat_map(|c| &c.accounts)
                .find_map(|acc| {
                    acc.details
                        .iter()
                        .position(|d| d.kind == RowKind::Line && !d.rate.is_blank())
                        .map(|i| (acc.code.clone(), i + 1, acc.details[i].id))
                })
                .unwrap();
            (code, line, id)
        };
        let d = first_line(&mut b);
        d.amount = "10".into();
        d.x = "".into();
        d.x4 = "".into();
        d.rate = "1350".into();
        let mut a2 = a.clone();
        let d2 = first_line(&mut a2);
        d2.amount = "10".into();
        d2.x = "".into();
        d2.x4 = "".into();
        d2.rate = "1200".into();
        let diff = diff(&a2, &b);
        let row: Vec<&DiffItem> = diff
            .items
            .iter()
            .filter(|i| matches!(i.place, Place::Account { row: Some(r), .. } if r == id))
            .collect();
        assert_eq!(row.len(), 1, "{:?}", diff.items);
        assert_eq!(
            row[0].text,
            format!("{code} line {line}: rate 1,200.00 -> 1,350.00 (+1,500.00 to {code})")
        );
        assert_eq!(row[0].money, Some(150_000));
        assert_eq!(row[0].fields[0].field, "rate");
        // The account's total moved by at least the line (fringes on top).
        let acct = diff
            .accounts
            .iter()
            .find(|l| l.label.starts_with(&code))
            .unwrap();
        assert!(acct.delta >= 150_000);
        assert!(diff.totals.iter().any(|l| l.label == "Grand total"));
    }

    #[test]
    fn added_removed_and_moved_rows() {
        let a = starter::budget();
        let mut b = a.clone();
        let next = b.highest_row_id().unwrap() + 1;
        let accts: Vec<(usize, usize)> =
            account_list(&b.topsheet).iter().map(|(p, _)| *p).collect();
        let (p0, p1) = (accts[0], accts[1]);
        let TopsheetRow::Category(c) = &mut b.topsheet[p0.0] else {
            panic!()
        };
        c.accounts[p0.1]
            .details
            .push(Detail::line(next, "Dolly grip", "10", "Day", "450"));
        let gone = c.accounts[p0.1].details.remove(0);
        let gone_id = gone.id;
        // Move the first row of the second account to the first.
        let moved = {
            let TopsheetRow::Category(c) = &mut b.topsheet[p1.0] else {
                panic!()
            };
            c.accounts[p1.1].details.remove(0)
        };
        let moved_id = moved.id;
        let TopsheetRow::Category(c) = &mut b.topsheet[p0.0] else {
            panic!()
        };
        c.accounts[p0.1].details.insert(0, moved);
        let d = diff(&a, &b);
        let find = |id: RowId| {
            d.items
                .iter()
                .find(|i| matches!(i.place, Place::Account { row: Some(r), .. } if r == id))
                .unwrap()
        };
        assert_eq!(find(next).kind, ChangeKind::Added);
        assert!(
            find(next)
                .text
                .contains("added: \"Dolly grip\" (+4,500.00 to"),
            "{}",
            find(next).text
        );
        assert_eq!(find(gone_id).kind, ChangeKind::Removed);
        assert_eq!(find(moved_id).kind, ChangeKind::Moved);
        assert!(
            find(moved_id).text.contains("moved from"),
            "{}",
            find(moved_id).text
        );
    }

    #[test]
    fn renumbered_and_renamed_accounts_are_matched() {
        let a = starter::budget();
        let mut b = a.clone();
        let (old_code, new_name) = {
            let acc = b
                .categories_mut()
                .next()
                .unwrap()
                .accounts
                .first_mut()
                .unwrap();
            let old = acc.code.clone();
            acc.code = format!("{old}X");
            acc.name = "Brand New Name".into();
            (old, acc.name.clone())
        };
        let d = diff(&a, &b);
        assert!(d.items.iter().any(|i| i.kind == ChangeKind::Renumbered
            && i.text == format!("Account {old_code} renumbered to {old_code}X")));
        assert!(d.items.iter().any(
            |i| i.kind == ChangeKind::Renamed && i.text.contains(&format!("-> \"{new_name}\""))
        ));
        assert!(!d
            .items
            .iter()
            .any(|i| matches!(i.kind, ChangeKind::Added | ChangeKind::Removed)));
        // No money moved.
        assert!(d.totals.is_empty() && d.categories.is_empty());
    }

    #[test]
    fn setup_and_settings_changes() {
        let a = starter::budget();
        let mut b = a.clone();
        b.title = "New Title".into();
        let (name, _) = b
            .fringes
            .iter()
            .next()
            .map(|(k, v)| (k.clone(), v.clone()))
            .unwrap();
        b.fringes.get_mut(&name).unwrap().rate = "99".into();
        b.globals.insert("NEW_ONE".into(), "3".into());
        b.groups.insert(
            "NQ".into(),
            Group {
                description: "Not quoted".into(),
                short_code: None,
                in_budget_total: false,
                color: None,
            },
        );
        b.currencies.insert(
            "EUR".into(),
            Currency {
                description: String::new(),
                symbol: "€".into(),
                rate: Dec::from_int(1),
                format: Default::default(),
            },
        );
        let d = diff(&a, &b);
        let texts: Vec<&str> = d.items.iter().map(|i| i.text.as_str()).collect();
        assert!(
            texts.contains(&"Title \"Starter Budget\" -> \"New Title\"")
                || texts.iter().any(|t| t.starts_with("Title ")),
            "{texts:?}"
        );
        assert!(
            texts
                .iter()
                .any(|t| t.starts_with(&format!("Fringe {name}: rate ")) && t.ends_with("-> 99")),
            "{texts:?}"
        );
        assert!(
            texts.contains(&"Global NEW_ONE added: value 3"),
            "{texts:?}"
        );
        assert!(
            texts
                .iter()
                .any(|t| t
                    .starts_with("Group NQ added: description \"Not quoted\", in budget total no")),
            "{texts:?}"
        );
        assert!(
            texts.iter().any(|t| t.starts_with("Currency EUR added")),
            "{texts:?}"
        );
        // Grouped under their headings.
        let text = d.to_text();
        assert!(text.contains("\nFringes\n"));
        assert!(text.contains("\nGlobals\n"));
    }

    #[test]
    fn moved_in_finds_what_moved() {
        assert!(moved_in(&[1, 2, 3, 4], &[1, 2, 3, 4]).is_empty());
        assert_eq!(moved_in(&[1, 2, 3, 4], &[1, 3, 4, 2]), BTreeSet::from([2]));
        assert_eq!(moved_in(&[1, 2, 3], &[3, 1, 2]), BTreeSet::from([3]));
        // Elements only on one side don't count.
        assert!(moved_in(&[1, 2, 9], &[7, 1, 2]).is_empty());
    }

    #[test]
    fn notes_are_summed_up() {
        let a = vec![Note::plain(1, "one"), Note::plain(2, "two")];
        let mut b = a.clone();
        b[0].done = true;
        b[1].text = "TWO".into();
        b.push(Note::plain(3, "three"));
        let fc = notes_change(&a, &b).unwrap();
        assert_eq!(fc.text, "notes: 1 added, 1 edited, 1 ticked off");
    }

    #[test]
    fn versions_are_listed_by_name() {
        let a = starter::budget();
        let mut b = a.clone();
        crate::save_version(&mut b, "Studio draft", "2026-10-09T10:00:00Z", None);
        let d = diff(&a, &b);
        assert!(d
            .items
            .iter()
            .any(|i| i.text == "Version \"Studio draft\" saved"));
        let mut c = b.clone();
        let id = c.versions[0].id;
        crate::rename_version(&mut c, id, "Final").unwrap();
        let d = diff(&b, &c);
        assert_eq!(d.items.len(), 1);
        assert_eq!(d.items[0].kind, ChangeKind::Renamed);
    }

    #[test]
    fn money_is_left_out_when_a_budget_cant_be_totalled() {
        let a = starter::budget();
        let mut b = a.clone();
        first_line(&mut b).rate = "NO_SUCH_GLOBAL".into();
        let d = diff(&a, &b);
        assert!(!d.money_known);
        assert!(d.totals.is_empty());
        assert!(d
            .items
            .iter()
            .any(|i| i.text.contains("rate") && i.money.is_none()));
        assert!(d.to_text().contains("can't be totalled"));
    }
}
