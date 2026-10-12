//! Three-way merge of budgets (roadmap 3.2; decisions.md, entry 44).
//!
//! [`merge`] takes a common ancestor `base` and two budgets made from it,
//! `ours` and `theirs`, and returns one budget with both sets of changes and
//! a list of the [`Conflict`]s a person has to settle. Things are matched as
//! [`crate::diff`] matches them (rows by id, accounts and categories by code
//! with a fallback, setup items and charges by name, versions by id).
//!
//! - A change on one side only is taken. The same change on both sides is
//!   taken once.
//! - Changes to different rows, different fields of one row, different
//!   setup items or different fields of one item merge on their own. Tags
//!   (fringes, groups, a charge's exclusions) merge name by name, so one side
//!   adding FICA and the other adding WGA keeps both.
//! - The same field changed to different values on both sides is a
//!   conflict. So is something deleted on one side and changed on the other.
//!   An account or category deleted on one side and changed on the other
//!   (its own fields, or anything in it) is one conflict for the whole
//!   thing: keeping it keeps it as the side that changed it has it;
//!   deleting it takes everything in it (rows the deleting side moved out
//!   stay where it put them).
//! - Every conflict takes [`Side::Ours`] by default (or the side asked for);
//!   [`merge_with`] takes a pick per conflict, by its key. Keys are stable
//!   for the same three budgets.
//! - Rows added on both sides with the same id (each side numbers new rows
//!   from the highest it has) are two rows unless they are identical: the
//!   one from theirs gets a new id. Versions saved on both sides likewise.
//! - Order: the side that reordered a list (a topsheet, a category's
//!   accounts, an account's rows) wins; rows and accounts the other side
//!   added go after the row or account they followed there. Both sides
//!   reordering the same list differently is a conflict.
//! - Layout and tools (display settings, presets, setup order and folders)
//!   merge like anything else. The provenance is always ours: it describes
//!   the file being merged into.
//!
//! Money is never merged: totals are recalculated from the merged rows like
//! any budget's.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::hash::Hash;

use crate::diff::{
    account_list, basis_text, charge_fields, credit_fields, currency_fields, field_changes,
    formula_text, fringe_fields, global_fields, group_fields, info_fields, kind_name,
    location_fields, match_topsheets, max_hours_text, money, on_off, opt_text, pair_by_key, quoted,
    row_changes, row_name, set_fields, summary, top_key, unit_fields, Field, Place, Table,
};
use crate::model::{
    Account, Budget, Category, Charge, Credit, Detail, FlatBasis, FringeKind, Note,
    ProductionTotal, RowId, TopsheetRow, Version, VersionId,
};
use crate::{recalc, Dec, RecalcOptions};

// ---------------------------------------------------------------------------
// What a merge gives back

/// One side of a merge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    Ours,
    Theirs,
}

impl Side {
    /// `"ours"` or `"theirs"`.
    pub fn key(self) -> &'static str {
        match self {
            Side::Ours => "ours",
            Side::Theirs => "theirs",
        }
    }

    /// The side named `"ours"` or `"theirs"`.
    pub fn from_key(s: &str) -> Option<Side> {
        match s {
            "ours" => Some(Side::Ours),
            "theirs" => Some(Side::Theirs),
            _ => None,
        }
    }
}

/// Something both sides changed differently.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conflict {
    /// Stable for the same three budgets: what a pick names.
    pub key: String,
    pub place: Place,
    /// What it is about: `21-01 line 3 rate`, `Fringe FICA`, `Account 21-01`.
    pub what: String,
    /// The value in the base, when it had one.
    pub base: Option<String>,
    pub ours: String,
    pub theirs: String,
    /// The side the merged budget took.
    pub taken: Side,
    /// In plain words: `21-01 line 3 rate: ours 1,350.00, theirs 1,400.00,
    /// was 1,200.00`.
    pub text: String,
}

/// A merged budget.
#[derive(Clone, Debug)]
pub struct Merged {
    pub budget: Budget,
    /// In budget order: settings, setup, the topsheet and accounts, order,
    /// versions, tools.
    pub conflicts: Vec<Conflict>,
    /// Rows and versions from theirs given new ids because ours added
    /// different ones with the same ids.
    pub renumbered: usize,
    /// The merged budget can't be totalled although ours and theirs both
    /// can (a fringe one side deleted and the other side used, say): the
    /// first problem.
    pub problem: Option<String>,
}

/// Merges `ours` and `theirs`, both made from `base`. Conflicts keep ours.
pub fn merge(base: &Budget, ours: &Budget, theirs: &Budget) -> Merged {
    merge_with(base, ours, theirs, Side::Ours, &BTreeMap::new())
}

/// [`merge`], taking `picks` (conflict key to side) where given and
/// `default` for every other conflict.
pub fn merge_with(
    base: &Budget,
    ours: &Budget,
    theirs: &Budget,
    default: Side,
    picks: &BTreeMap<String, Side>,
) -> Merged {
    let mut cx = Cx {
        default,
        picks,
        conflicts: Vec::new(),
    };
    let (budget, renumbered) = merge_budgets(&mut cx, base, ours, theirs);
    let opts = RecalcOptions::default();
    let problem = match recalc(&budget, &opts) {
        Ok(_) => None,
        Err(e) if recalc(ours, &opts).is_ok() && recalc(theirs, &opts).is_ok() => {
            Some(e.to_string())
        }
        Err(_) => None,
    };
    Merged {
        budget,
        conflicts: cx.conflicts,
        renumbered,
        problem,
    }
}

/// The base to merge a returned shared copy against when the base is the
/// content it was made from (a saved version, or the budget itself): that
/// content as the copy was sent, so what the copy left out isn't read as
/// deleted by whoever returned it.
///
/// What a copy for sharing leaves out isn't recorded, so it is worked out
/// from the copy: no note of `base`'s in the copy means notes were left out,
/// no info field the same means info was, a missing baseline means the
/// baseline was, and setup items nothing uses all missing means unused setup
/// was. Versions are those the copy and `ours` (the original as it is now)
/// both have, and the tools (presets, display settings, setup order and
/// folders) are the copy's, as they were copied in when it was saved.
pub fn base_as_shared(base: &Budget, ours: &Budget, copy: &Budget) -> Budget {
    let mut out = base.clone();
    let notes = |b: &Budget| -> BTreeSet<(String, String)> {
        let mut s = BTreeSet::new();
        for c in b.categories() {
            s.extend(c.notes.iter().map(|n| (n.created.clone(), n.text.clone())));
            for a in &c.accounts {
                s.extend(a.notes.iter().map(|n| (n.created.clone(), n.text.clone())));
                for d in &a.details {
                    s.extend(d.notes.iter().map(|n| (n.created.clone(), n.text.clone())));
                }
            }
        }
        s
    };
    let base_notes = notes(base);
    if !base_notes.is_empty() && base_notes.is_disjoint(&notes(copy)) {
        crate::share::drop_notes(&mut out);
    }
    if !base.info.is_empty() && base.info.iter().all(|(k, v)| copy.info.get(k) != Some(v)) {
        out.info.clear();
    }
    if base.baseline.is_some() && copy.baseline.is_none() {
        out.baseline = None;
    }
    let mut pruned = out.clone();
    if crate::share::prune_unused_setup(&mut pruned) > 0 {
        let gone = |a: &BTreeSet<&String>, b: &BTreeSet<&String>| -> Vec<String> {
            a.difference(b).map(|s| (*s).clone()).collect()
        };
        let (ko, kp, kc) = (setup_names(&out), setup_names(&pruned), setup_names(copy));
        let all_gone_from_copy = ko
            .iter()
            .zip(&kp)
            .zip(&kc)
            .all(|((o, p), c)| gone(o, p).iter().all(|n| !c.contains(n)));
        if all_gone_from_copy {
            out = pruned;
        }
    }
    let sent = |v: &Version| {
        ours.versions
            .iter()
            .any(|o| o.id == v.id && o.created == v.created)
    };
    out.versions = copy.versions.iter().filter(|v| sent(v)).cloned().collect();
    out.report_presets = copy.report_presets.clone();
    out.tag_presets = copy.tag_presets.clone();
    out.line_presets = copy.line_presets.clone();
    out.display = copy.display.clone();
    out.setup_order = copy.setup_order.clone();
    out.setup_folders = copy.setup_folders.clone();
    out.ot_patterns = copy.ot_patterns.clone();
    out.setup_sources = copy.setup_sources.clone();
    out.compact_versions = copy.compact_versions;
    out.actuals = copy.actuals.clone();
    out
}

/// Every setup item's name, by kind.
fn setup_names(b: &Budget) -> [BTreeSet<&String>; 7] {
    [
        b.globals.keys().collect(),
        b.fringes.keys().collect(),
        b.groups.keys().collect(),
        b.units.keys().collect(),
        b.sets.keys().collect(),
        b.locations.keys().collect(),
        b.currencies.keys().collect(),
    ]
}

// ---------------------------------------------------------------------------
// Deciding

struct Cx<'p> {
    default: Side,
    picks: &'p BTreeMap<String, Side>,
    conflicts: Vec<Conflict>,
}

/// A conflict before it is decided.
struct Clash {
    key: String,
    place: Place,
    what: String,
    base: Option<String>,
    ours: String,
    theirs: String,
}

impl Cx<'_> {
    fn decide(&mut self, c: Clash) -> Side {
        let taken = self.picks.get(&c.key).copied().unwrap_or(self.default);
        let text = match &c.base {
            Some(b) => format!("{}: ours {}, theirs {}, was {b}", c.what, c.ours, c.theirs),
            None => format!("{}: ours {}, theirs {}", c.what, c.ours, c.theirs),
        };
        self.conflicts.push(Conflict {
            key: c.key,
            place: c.place,
            what: c.what,
            base: c.base,
            ours: c.ours,
            theirs: c.theirs,
            taken,
            text,
        });
        taken
    }
}

/// Where a merged thing is: its conflict keys start with `key`, and its
/// conflicts say `what` (`21-01 line 3`, `Fringe FICA`).
#[derive(Clone)]
struct At {
    key: String,
    place: Place,
    what: String,
}

/// The three-way choice for one value: `Some(value)` (none for "not
/// there"), or `None` for a conflict.
fn pick3<'a, T: PartialEq + ?Sized>(
    b: Option<&'a T>,
    o: Option<&'a T>,
    t: Option<&'a T>,
) -> Option<Option<&'a T>> {
    if o == t {
        Some(o)
    } else if o == b {
        Some(t)
    } else if t == b {
        Some(o)
    } else {
        None
    }
}

/// Names merged one by one: kept unless a side removed it, added when a
/// side added it.
fn merge_set(
    b: Option<&BTreeSet<String>>,
    o: &BTreeSet<String>,
    t: &BTreeSet<String>,
) -> BTreeSet<String> {
    let empty = BTreeSet::new();
    let b = b.unwrap_or(&empty);
    o.union(t)
        .filter(|n| !b.contains(*n) || (o.contains(*n) && t.contains(*n)))
        .cloned()
        .collect()
}

/// The same thing in the base (if it had it), ours and theirs.
struct Three<'a, T> {
    b: Option<&'a T>,
    o: &'a T,
    t: &'a T,
}

impl<T> Three<'_, T> {
    /// One field merged: the side that changed it, or a conflict.
    fn field<V: PartialEq + Clone>(
        &self,
        cx: &mut Cx,
        at: &At,
        key: &str,
        label: &str,
        get: impl Fn(&T) -> &V,
        show: impl Fn(&V) -> String,
    ) -> V {
        let (b, o, t) = (self.b.map(&get), get(self.o), get(self.t));
        match pick3(b, Some(o), Some(t)) {
            Some(v) => v.unwrap_or(o).clone(),
            None => {
                let side = cx.decide(Clash {
                    key: format!("{}:{key}", at.key),
                    place: at.place.clone(),
                    what: format!("{} {label}", at.what),
                    base: b.map(&show),
                    ours: show(o),
                    theirs: show(t),
                });
                match side {
                    Side::Ours => o.clone(),
                    Side::Theirs => t.clone(),
                }
            }
        }
    }

    fn names(&self, get: impl Fn(&T) -> &BTreeSet<String>) -> BTreeSet<String> {
        merge_set(self.b.map(&get), get(self.o), get(self.t))
    }
}

/// `"changed: rate 6.2 -> 7.65"` for a thing one side changed.
fn changed_text(changes: Vec<String>) -> String {
    if changes.is_empty() {
        "changed".into()
    } else {
        format!("changed: {}", changes.join(", "))
    }
}

/// How a merged item came out of the three sides.
enum Pick<V> {
    Keep(V),
    Drop,
}

/// One keyed item (a setup item, a charge, a preset) merged: taken from the
/// side that changed it, merged field by field by `item` when both did,
/// or a conflict when one side deleted it and the other changed it.
fn merge_item<V: PartialEq + Clone>(
    cx: &mut Cx,
    at: &At,
    (b, o, t): (Option<&V>, Option<&V>, Option<&V>),
    fields: &dyn Fn(&V) -> Vec<Field>,
    item: &dyn Fn(&mut Cx, &At, Three<V>) -> V,
) -> Pick<V> {
    let changes = |x: &V, y: &V| -> Vec<String> {
        field_changes(&fields(x), &fields(y))
            .into_iter()
            .map(|f| f.text)
            .collect()
    };
    match (b, o, t) {
        (_, Some(x), Some(y)) if x == y => Pick::Keep(x.clone()),
        (Some(bb), Some(x), Some(y)) if x == bb => Pick::Keep(y.clone()),
        (Some(bb), Some(x), Some(y)) if y == bb => Pick::Keep(x.clone()),
        (bb, Some(x), Some(y)) => Pick::Keep(item(cx, at, Three { b: bb, o: x, t: y })),
        (Some(bb), Some(x), None) if x == bb => Pick::Drop,
        (Some(bb), None, Some(y)) if y == bb => Pick::Drop,
        (Some(bb), Some(x), None) => {
            let side = cx.decide(Clash {
                key: at.key.clone(),
                place: at.place.clone(),
                what: at.what.clone(),
                base: Some(summary(&fields(bb))),
                ours: changed_text(changes(bb, x)),
                theirs: "deleted".into(),
            });
            match side {
                Side::Ours => Pick::Keep(x.clone()),
                Side::Theirs => Pick::Drop,
            }
        }
        (Some(bb), None, Some(y)) => {
            let side = cx.decide(Clash {
                key: at.key.clone(),
                place: at.place.clone(),
                what: at.what.clone(),
                base: Some(summary(&fields(bb))),
                ours: "deleted".into(),
                theirs: changed_text(changes(bb, y)),
            });
            match side {
                Side::Ours => Pick::Drop,
                Side::Theirs => Pick::Keep(y.clone()),
            }
        }
        (None, Some(x), None) => Pick::Keep(x.clone()),
        (None, None, Some(y)) => Pick::Keep(y.clone()),
        (_, None, None) => Pick::Drop,
    }
}

/// What a keyed table is called: the start of its conflict keys, and how a
/// sentence names one of its items.
#[derive(Clone, Copy)]
struct Kind {
    key: &'static str,
    word: &'static str,
    table: Option<Table>,
}

impl Kind {
    fn of(table: Table) -> Kind {
        Kind {
            key: table.singular(),
            word: table.word(),
            table: Some(table),
        }
    }

    fn at(self, name: &str) -> At {
        At {
            key: format!("{}:{name}", self.key),
            place: match self.table {
                Some(table) => Place::Setup {
                    table,
                    name: name.to_string(),
                },
                None => Place::Budget,
            },
            what: format!("{} {name}", self.word),
        }
    }
}

/// The same map in the base, ours and theirs.
type Maps<'a, V> = (
    &'a BTreeMap<String, V>,
    &'a BTreeMap<String, V>,
    &'a BTreeMap<String, V>,
);

/// A map of named items merged item by item.
/// A fringe's kind merged field by field: percent or flat, then the
/// cutoff (both kinds keep one) and a flat fringe's unit, basis and most
/// hours a day, each from the side that changed it.
fn merge_fringe_kind(cx: &mut Cx, at: &At, th: &Three<crate::model::Fringe>) -> FringeKind {
    static NO_UNIT: Option<String> = None;
    static NO_HOURS: Option<Dec> = None;
    static STANDARD: FlatBasis = FlatBasis::Amount;
    let flat = th.field(
        cx,
        at,
        "kind",
        "kind",
        |f| match f.kind {
            FringeKind::Percent { .. } => &false,
            FringeKind::Flat { .. } => &true,
        },
        |v| if *v { "flat" } else { "percent" }.into(),
    );
    let cutoff = th.field(
        cx,
        at,
        "cutoff",
        "cutoff",
        |f| match &f.kind {
            FringeKind::Percent { cutoff } | FringeKind::Flat { cutoff, .. } => cutoff,
        },
        |v| v.map_or("none".into(), money),
    );
    if !flat {
        return FringeKind::Percent { cutoff };
    }
    FringeKind::Flat {
        unit: th.field(
            cx,
            at,
            "unit",
            "flat unit",
            |f| match &f.kind {
                FringeKind::Flat { unit, .. } => unit,
                FringeKind::Percent { .. } => &NO_UNIT,
            },
            opt_text,
        ),
        basis: th.field(
            cx,
            at,
            "basis",
            "basis",
            |f| match &f.kind {
                FringeKind::Flat { basis, .. } => basis,
                FringeKind::Percent { .. } => &STANDARD,
            },
            |v| basis_text(*v),
        ),
        max_hours_per_day: th.field(
            cx,
            at,
            "max_hours_per_day",
            "max hours a day",
            |f| match &f.kind {
                FringeKind::Flat {
                    max_hours_per_day, ..
                } => max_hours_per_day,
                FringeKind::Percent { .. } => &NO_HOURS,
            },
            max_hours_text,
        ),
        cutoff,
    }
}

fn merge_map<V: PartialEq + Clone>(
    cx: &mut Cx,
    kind: Kind,
    (b, o, t): Maps<'_, V>,
    fields: &dyn Fn(&V) -> Vec<Field>,
    item: &dyn Fn(&mut Cx, &At, Three<V>) -> V,
) -> BTreeMap<String, V> {
    let keys: BTreeSet<&String> = b.keys().chain(o.keys()).chain(t.keys()).collect();
    let mut out = BTreeMap::new();
    for k in keys {
        if let Pick::Keep(v) = merge_item(
            cx,
            &kind.at(k),
            (b.get(k), o.get(k), t.get(k)),
            fields,
            item,
        ) {
            out.insert(k.clone(), v);
        }
    }
    out
}

/// A list of named items (charges, credits, presets) merged item by item,
/// matched by name (the n-th of a name with the n-th), in merged order.
fn merge_list<V: PartialEq + Clone>(
    cx: &mut Cx,
    kind: Kind,
    lists: (&[V], &[V], &[V]),
    name: &dyn Fn(&V) -> &str,
    fields: &dyn Fn(&V) -> Vec<Field>,
    item: &dyn Fn(&mut Cx, &At, Three<V>) -> V,
) -> Vec<V> {
    let keyed = |l: &[V]| -> (Vec<String>, BTreeMap<String, V>) {
        let keys = crate::diff::list_keys(l.iter().map(name));
        let map = keys.iter().cloned().zip(l.iter().cloned()).collect();
        (keys, map)
    };
    let (kb, mb) = keyed(lists.0);
    let (ko, mo) = keyed(lists.1);
    let (kt, mt) = keyed(lists.2);
    let merged = merge_map(cx, kind, (&mb, &mo, &mt), fields, item);
    let members: BTreeSet<&String> = merged.keys().collect();
    let order = merge_order(
        cx,
        None,
        [&refs(&kb), &refs(&ko), &refs(&kt)],
        &members,
        |_| String::new(),
    );
    order
        .into_iter()
        .filter_map(|k| merged.get(k).cloned())
        .collect()
}

fn refs(l: &[String]) -> Vec<&String> {
    l.iter().collect()
}

/// A whole value (a budget setting) merged: the side that changed it, or a
/// conflict.
fn merge_value<V: PartialEq + Clone>(
    cx: &mut Cx,
    at: &At,
    (b, o, t): (&V, &V, &V),
    show: impl Fn(&V) -> String,
) -> V {
    match pick3(Some(b), Some(o), Some(t)) {
        Some(v) => v.unwrap_or(o).clone(),
        None => match cx.decide(Clash {
            key: at.key.clone(),
            place: at.place.clone(),
            what: at.what.clone(),
            base: Some(show(b)),
            ours: show(o),
            theirs: show(t),
        }) {
            Side::Ours => o.clone(),
            Side::Theirs => t.clone(),
        },
    }
}

fn setting_at(key: &str, what: &str) -> At {
    At {
        key: format!("setting:{key}"),
        place: Place::Budget,
        what: what.to_string(),
    }
}

/// Merges a list in which every element is unique: the side that
/// reordered it wins (both reordering differently is a conflict when
/// `clash` names the list, else ours wins); elements the other side (or
/// only the base) has go after the one they follow there; elements in no
/// list go last. Only `members` are kept.
fn merge_order<K: Copy + Ord + Hash>(
    cx: &mut Cx,
    clash: Option<&At>,
    lists: [&[K]; 3],
    members: &BTreeSet<K>,
    show: impl Fn(&[K]) -> String,
) -> Vec<K> {
    let sets: Vec<HashSet<K>> = lists.iter().map(|l| l.iter().copied().collect()).collect();
    let in_all = |k: &K| members.contains(k) && sets.iter().all(|s| s.contains(k));
    let common = |l: &[K]| -> Vec<K> { l.iter().copied().filter(|k| in_all(k)).collect() };
    let (fb, fo, ft) = (common(lists[0]), common(lists[1]), common(lists[2]));
    let (by_o, by_t) = (fo != fb, ft != fb);
    let primary = if by_o && by_t && fo != ft {
        match clash {
            Some(at) => cx.decide(Clash {
                key: format!("{}:order", at.key),
                place: at.place.clone(),
                what: format!("{} order", at.what),
                base: Some(show(&fb)),
                ours: show(&fo),
                theirs: show(&ft),
            }),
            None => Side::Ours,
        }
    } else if by_t {
        Side::Theirs
    } else {
        Side::Ours
    };
    let (first, second) = match primary {
        Side::Ours => (lists[1], lists[2]),
        Side::Theirs => (lists[2], lists[1]),
    };
    let mut out: Vec<K> = Vec::with_capacity(members.len());
    let mut placed: HashSet<K> = HashSet::new();
    for k in first {
        if members.contains(k) && placed.insert(*k) {
            out.push(*k);
        }
    }
    for l in [second, lists[0]] {
        for (i, k) in l.iter().enumerate() {
            if !members.contains(k) || placed.contains(k) {
                continue;
            }
            let at = l[..i]
                .iter()
                .rev()
                .find(|p| placed.contains(*p))
                .and_then(|p| out.iter().position(|x| x == p))
                .map_or(0, |i| i + 1);
            out.insert(at, *k);
            placed.insert(*k);
        }
    }
    for k in members {
        if placed.insert(*k) {
            out.push(*k);
        }
    }
    out
}

/// A short list for an order conflict: the first few names.
fn order_text(names: Vec<String>) -> String {
    const SHOWN: usize = 8;
    let more = names.len().saturating_sub(SHOWN);
    let mut s = names.into_iter().take(SHOWN).collect::<Vec<_>>().join(", ");
    if more > 0 {
        s.push_str(&format!(" and {more} more"));
    }
    if s.is_empty() {
        "nothing".into()
    } else {
        s
    }
}

/// Notes merged note by note (by id). Notes added on both sides with one
/// id are both kept, theirs with a new id.
fn merge_notes(cx: &mut Cx, at: &At, b: Option<&[Note]>, o: &[Note], t: &[Note]) -> Vec<Note> {
    let b = b.unwrap_or(&[]);
    if o == t {
        return o.to_vec();
    }
    if o == b {
        return t.to_vec();
    }
    if t == b {
        return o.to_vec();
    }
    let by_id =
        |l: &[Note]| -> BTreeMap<u64, Note> { l.iter().map(|n| (n.id, n.clone())).collect() };
    let (mb, mo) = (by_id(b), by_id(o));
    let mut mt = by_id(t);
    // Notes new on both sides with one id: theirs gets the next free id.
    let mut next = b.iter().chain(o).chain(t).map(|n| n.id).max().unwrap_or(0);
    let mut t_ids: Vec<u64> = t.iter().map(|n| n.id).collect();
    for id in t_ids.iter_mut() {
        let clash = !mb.contains_key(id) && mo.get(id).is_some_and(|x| Some(x) != mt.get(id));
        if clash {
            if let Some(mut n) = mt.remove(id) {
                next += 1;
                n.id = next;
                *id = next;
                mt.insert(next, n);
            }
        }
    }
    let note_text = |n: &Note| quoted(&n.text);
    let keys: BTreeSet<u64> = mb
        .keys()
        .chain(mo.keys())
        .chain(mt.keys())
        .copied()
        .collect();
    let mut kept = BTreeMap::new();
    for id in keys {
        let at = At {
            key: format!("{}:note:{id}", at.key),
            place: at.place.clone(),
            what: format!("{} note", at.what),
        };
        let fields = |n: &Note| {
            vec![Field {
                key: "text",
                label: "text",
                value: crate::diff::Value::Text(note_text(n)),
            }]
        };
        let item = |cx: &mut Cx, at: &At, th: Three<Note>| -> Note {
            match pick3(th.b, Some(th.o), Some(th.t)) {
                Some(v) => v.unwrap_or(th.o).clone(),
                None => match cx.decide(Clash {
                    key: at.key.clone(),
                    place: at.place.clone(),
                    what: at.what.clone(),
                    base: th.b.map(note_text),
                    ours: note_text(th.o),
                    theirs: note_text(th.t),
                }) {
                    Side::Ours => th.o.clone(),
                    Side::Theirs => th.t.clone(),
                },
            }
        };
        if let Pick::Keep(n) = merge_item(
            cx,
            &at,
            (mb.get(&id), mo.get(&id), mt.get(&id)),
            &fields,
            &item,
        ) {
            kept.insert(id, n);
        }
    }
    let members: BTreeSet<u64> = kept.keys().copied().collect();
    let (lb, lo): (Vec<u64>, Vec<u64>) = (
        b.iter().map(|n| n.id).collect(),
        o.iter().map(|n| n.id).collect(),
    );
    merge_order(cx, None, [&lb, &lo, &t_ids], &members, |_| String::new())
        .into_iter()
        .filter_map(|id| kept.remove(&id))
        .collect()
}

// ---------------------------------------------------------------------------
// The whole budget

const B: usize = 0;
const O: usize = 1;
const T: usize = 2;

fn merge_budgets(cx: &mut Cx, b: &Budget, o: &Budget, t: &Budget) -> (Budget, usize) {
    // Destructured completely, so a new Budget field is a compile error
    // here until the merge handles it.
    let Budget {
        title: _,
        base_currency: _,
        totals_currency: _,
        info: _,
        currencies: _,
        globals: _,
        global_info: _,
        units: _,
        fringes: _,
        fringe_posting: _,
        trailing_level_fringes: _,
        cutoff_factor: _,
        fringe_row: _,
        arithmetic: _,
        account_coding: _,
        new_line_defaults: _,
        groups: _,
        group_rules: _,
        sets: _,
        locations: _,
        topsheet: _,
        charges: _,
        credits: _,
        baseline: _,
        display: _,
        versions: _,
        compact_versions: _,
        report_presets: _,
        tag_presets: _,
        line_presets: _,
        setup_order: _,
        setup_folders: _,
        provenance: _,
        actuals: _,
        ot_patterns: _,
        setup_sources: _,
    } = o;

    // Budget settings.
    let title = merge_value(
        cx,
        &setting_at("title", "Title"),
        (&b.title, &o.title, &t.title),
        |v| quoted(v),
    );
    let base_currency = merge_value(
        cx,
        &setting_at("base_currency", "Base currency"),
        (&b.base_currency, &o.base_currency, &t.base_currency),
        |v| quoted(v),
    );
    let totals_currency = merge_value(
        cx,
        &setting_at("totals_currency", "Totals currency"),
        (&b.totals_currency, &o.totals_currency, &t.totals_currency),
        opt_text,
    );
    let account_coding = merge_value(
        cx,
        &setting_at("account_coding", "Account numbers"),
        (&b.account_coding, &o.account_coding, &t.account_coding),
        crate::diff::coding_text,
    );
    let fringe_posting = merge_value(
        cx,
        &setting_at("fringe_posting", "Fringe posting"),
        (&b.fringe_posting, &o.fringe_posting, &t.fringe_posting),
        crate::diff::posting_text,
    );
    let trailing_level_fringes = merge_value(
        cx,
        &setting_at("trailing_level_fringes", "Trailing level fringes"),
        (
            &b.trailing_level_fringes,
            &o.trailing_level_fringes,
            &t.trailing_level_fringes,
        ),
        crate::diff::trailing_text,
    );
    let cutoff_factor = merge_value(
        cx,
        &setting_at("cutoff_factor", "Cutoff factor"),
        (&b.cutoff_factor, &o.cutoff_factor, &t.cutoff_factor),
        crate::diff::cutoff_factor_text,
    );
    let fringe_row = merge_value(
        cx,
        &setting_at("fringe_row", "Fringe row"),
        (&b.fringe_row, &o.fringe_row, &t.fringe_row),
        crate::diff::fringe_row_text,
    );
    let arithmetic = merge_value(
        cx,
        &setting_at("arithmetic", "Arithmetic"),
        (&b.arithmetic, &o.arithmetic, &t.arithmetic),
        crate::diff::arithmetic_text,
    );
    let group_rules = merge_value(
        cx,
        &setting_at("group_rules", "Group rules"),
        (&b.group_rules, &o.group_rules, &t.group_rules),
        crate::diff::group_rules_text,
    );
    let new_line_defaults = merge_value(
        cx,
        &setting_at("new_line_defaults", "New lines copy"),
        (
            &b.new_line_defaults,
            &o.new_line_defaults,
            &t.new_line_defaults,
        ),
        crate::diff::defaults_text,
    );
    let baseline = merge_value(
        cx,
        &setting_at("baseline", "Original totals"),
        (&b.baseline, &o.baseline, &t.baseline),
        crate::diff::baseline_text,
    );

    // Setup.
    let info = merge_map(
        cx,
        Kind::of(Table::Info),
        (&b.info, &o.info, &t.info),
        &info_fields,
        &|cx, at, th| {
            th.field(
                cx,
                at,
                "value",
                "value",
                |v| v,
                |v| summary(&info_fields(v)),
            )
        },
    );
    let globals = merge_map(
        cx,
        Kind::of(Table::Global),
        (&b.globals, &o.globals, &t.globals),
        &|f| global_fields(Some(f), None),
        &|cx, at, th| {
            th.field(
                cx,
                at,
                "value",
                "value",
                |f| f,
                |f| formula_text(&f.0, false),
            )
        },
    );
    let global_info = merge_map(
        cx,
        Kind {
            key: "global_info",
            word: "Global",
            table: Some(Table::Global),
        },
        (&b.global_info, &o.global_info, &t.global_info),
        &|i| global_fields(None, Some(i)),
        &|cx, at, th| crate::model::GlobalInfo {
            description: th.field(
                cx,
                at,
                "description",
                "description",
                |i| &i.description,
                |v| quoted(v),
            ),
            unit: th.field(cx, at, "unit", "unit", |i| &i.unit, |v| quoted(v)),
            decimals: th.field(
                cx,
                at,
                "decimals",
                "decimals",
                |i| &i.decimals,
                |v| v.map_or("as calculated".into(), |d| d.to_string()),
            ),
            show_by_name: th.field(
                cx,
                at,
                "show_by_name",
                "show by name",
                |i| &i.show_by_name,
                |v| on_off(*v),
            ),
            folder: th.field(cx, at, "folder", "folder", |i| &i.folder, |v| quoted(v)),
        },
    );
    let fringes = merge_map(
        cx,
        Kind::of(Table::Fringe),
        (&b.fringes, &o.fringes, &t.fringes),
        &fringe_fields,
        &|cx, at, th| crate::model::Fringe {
            description: th.field(
                cx,
                at,
                "description",
                "description",
                |f| &f.description,
                |v| quoted(v),
            ),
            short_code: th.field(
                cx,
                at,
                "short_code",
                "short code",
                |f| &f.short_code,
                opt_text,
            ),
            rate: th.field(
                cx,
                at,
                "rate",
                "rate",
                |f| &f.rate,
                |v| formula_text(&v.0, false),
            ),
            kind: merge_fringe_kind(cx, at, &th),
        },
    );
    let groups = merge_map(
        cx,
        Kind::of(Table::Group),
        (&b.groups, &o.groups, &t.groups),
        &group_fields,
        &|cx, at, th| crate::model::Group {
            description: th.field(
                cx,
                at,
                "description",
                "description",
                |g| &g.description,
                |v| quoted(v),
            ),
            short_code: th.field(
                cx,
                at,
                "short_code",
                "short code",
                |g| &g.short_code,
                opt_text,
            ),
            in_budget_total: th.field(
                cx,
                at,
                "in_budget_total",
                "in budget total",
                |g| &g.in_budget_total,
                |v| if *v { "yes" } else { "no" }.into(),
            ),
            color: th.field(cx, at, "color", "color", |g| &g.color, opt_text),
        },
    );
    let units = merge_map(
        cx,
        Kind::of(Table::Unit),
        (&b.units, &o.units, &t.units),
        &unit_fields,
        &|cx, at, th| crate::model::UnitDef {
            plural: th.field(cx, at, "plural", "plural", |u| &u.plural, |v| quoted(v)),
            description: th.field(
                cx,
                at,
                "description",
                "description",
                |u| &u.description,
                |v| quoted(v),
            ),
            hours: th.field(
                cx,
                at,
                "hours",
                "hours",
                |u| &u.hours,
                |v| v.map_or("none".into(), |h| h.to_string()),
            ),
        },
    );
    let currencies = merge_map(
        cx,
        Kind::of(Table::Currency),
        (&b.currencies, &o.currencies, &t.currencies),
        &currency_fields,
        &|cx, at, th| crate::model::Currency {
            description: th.field(
                cx,
                at,
                "description",
                "description",
                |c| &c.description,
                |v| quoted(v),
            ),
            symbol: th.field(cx, at, "symbol", "symbol", |c| &c.symbol, |v| quoted(v)),
            rate: th.field(cx, at, "rate", "rate", |c| &c.rate, |v| v.to_string()),
            format: th.field(
                cx,
                at,
                "format",
                "format",
                |c| &c.format,
                crate::diff::currency_format_text,
            ),
        },
    );
    let sets = merge_map(
        cx,
        Kind::of(Table::Set),
        (&b.sets, &o.sets, &t.sets),
        &set_fields,
        &|cx, at, th| crate::model::SetDef {
            description: th.field(
                cx,
                at,
                "description",
                "description",
                |s| &s.description,
                |v| quoted(v),
            ),
        },
    );
    let locations = merge_map(
        cx,
        Kind::of(Table::Location),
        (&b.locations, &o.locations, &t.locations),
        &location_fields,
        &|cx, at, th| crate::model::Location {
            description: th.field(
                cx,
                at,
                "description",
                "description",
                |l| &l.description,
                |v| quoted(v),
            ),
            city: th.field(cx, at, "city", "city", |l| &l.city, |v| quoted(v)),
            region: th.field(cx, at, "region", "region", |l| &l.region, |v| quoted(v)),
            country: th.field(cx, at, "country", "country", |l| &l.country, |v| quoted(v)),
        },
    );
    let charges = merge_list(
        cx,
        Kind::of(Table::Charge),
        (&b.charges, &o.charges, &t.charges),
        &|c: &Charge| c.name.as_str(),
        &charge_fields,
        &merge_charge,
    );
    let credits = merge_list(
        cx,
        Kind::of(Table::Credit),
        (&b.credits, &o.credits, &t.credits),
        &|c: &Credit| c.name.as_str(),
        &credit_fields,
        &|cx, at, th| Credit {
            name: th.o.name.clone(),
            account: th.field(cx, at, "account", "account", |c| &c.account, opt_text),
            applied: th.field(cx, at, "applied", "applied", |c| &c.applied, |v| on_off(*v)),
            cap: th.field(
                cx,
                at,
                "cap",
                "cap",
                |c| &c.cap,
                |v| v.map_or("none".into(), crate::diff::money),
            ),
            discount_percent: th.field(
                cx,
                at,
                "discount_percent",
                "discount",
                |c| &c.discount_percent,
                |v| format!("{v}%"),
            ),
            excluded_fringes: th.names(|c| &c.excluded_fringes),
            rows: th.field(
                cx,
                at,
                "rows",
                "rows",
                |c| &c.rows,
                |rows| {
                    crate::diff::credit_rows_text(&Credit {
                        name: String::new(),
                        account: None,
                        applied: true,
                        cap: None,
                        discount_percent: crate::Dec::ZERO,
                        excluded_fringes: BTreeSet::new(),
                        rows: rows.clone(),
                    })
                },
            ),
        },
    );

    // Categories, accounts and rows.
    let (topsheet, renumbered_rows) = Structure::new(b, o, t).merge(cx);

    // Versions, then tools and layout.
    let (versions, renumbered_versions) = merge_versions(cx, &b.versions, &o.versions, &t.versions);
    let tool = |key: &'static str, word: &'static str| Kind {
        key,
        word,
        table: None,
    };
    let report_presets = merge_list(
        cx,
        tool("report_preset", "Report preset"),
        (&b.report_presets, &o.report_presets, &t.report_presets),
        &|p: &crate::ReportPreset| p.name.as_str(),
        &|_| Vec::new(),
        &|cx, at, th| whole_pick(cx, at, th),
    );
    let tag_presets = merge_list(
        cx,
        tool("tag_preset", "Tag preset"),
        (&b.tag_presets, &o.tag_presets, &t.tag_presets),
        &|p: &crate::TagPreset| p.name.as_str(),
        &|_| Vec::new(),
        &|cx, at, th| whole_pick(cx, at, th),
    );
    let line_presets = merge_list(
        cx,
        tool("line_preset", "Line block"),
        (&b.line_presets, &o.line_presets, &t.line_presets),
        &|p: &crate::LinePreset| p.name.as_str(),
        &|_| Vec::new(),
        &|cx, at, th| whole_pick(cx, at, th),
    );
    let display = merge_value(
        cx,
        &setting_at("display", "Display settings"),
        (&b.display, &o.display, &t.display),
        |_| "changed".into(),
    );
    let setup_order = merge_value(
        cx,
        &setting_at("setup_order", "Setup table order"),
        (&b.setup_order, &o.setup_order, &t.setup_order),
        |_| "changed".into(),
    );
    let setup_folders = merge_value(
        cx,
        &setting_at("setup_folders", "Setup folders"),
        (&b.setup_folders, &o.setup_folders, &t.setup_folders),
        |_| "changed".into(),
    );
    let setup_sources = merge_value(
        cx,
        &setting_at("setup_sources", "Where setup items came from"),
        (&b.setup_sources, &o.setup_sources, &t.setup_sources),
        |_| "changed".into(),
    );
    let ot_patterns = merge_list(
        cx,
        tool("ot_pattern", "Overtime pattern"),
        (&b.ot_patterns, &o.ot_patterns, &t.ot_patterns),
        &|p: &crate::overtime::OtPattern| p.name.as_str(),
        &|_| Vec::new(),
        &|cx, at, th| whole_pick(cx, at, th),
    );
    let compact_versions = merge_value(
        cx,
        &setting_at("compact_versions", "Compact versions"),
        (
            &b.compact_versions,
            &o.compact_versions,
            &t.compact_versions,
        ),
        |v| on_off(*v),
    );
    // Actuals as a whole: they come from accounting, so two copies rarely
    // both change them.
    let actuals = merge_value(
        cx,
        &setting_at("actuals", "Actuals"),
        (&b.actuals, &o.actuals, &t.actuals),
        |a| crate::diff::actuals_text(a),
    );

    let merged = Budget {
        title,
        base_currency,
        totals_currency,
        info,
        currencies,
        globals,
        global_info,
        units,
        fringes,
        fringe_posting,
        trailing_level_fringes,
        cutoff_factor,
        fringe_row,
        arithmetic,
        account_coding,
        new_line_defaults,
        groups,
        group_rules,
        sets,
        locations,
        topsheet,
        charges,
        credits,
        baseline,
        display,
        versions,
        compact_versions,
        report_presets,
        tag_presets,
        line_presets,
        setup_order,
        setup_folders,
        // About the file being merged into.
        provenance: o.provenance.clone(),
        actuals,
        ot_patterns,
        setup_sources,
    };
    (merged, renumbered_rows + renumbered_versions)
}

/// A whole item changed on both sides: one conflict for it.
fn whole_pick<V: PartialEq + Clone>(cx: &mut Cx, at: &At, th: Three<V>) -> V {
    match pick3(th.b, Some(th.o), Some(th.t)) {
        Some(v) => v.unwrap_or(th.o).clone(),
        None => match cx.decide(Clash {
            key: at.key.clone(),
            place: at.place.clone(),
            what: at.what.clone(),
            base: th.b.map(|_| "as it was".to_string()),
            ours: "changed".into(),
            theirs: "changed".into(),
        }) {
            Side::Ours => th.o.clone(),
            Side::Theirs => th.t.clone(),
        },
    }
}

fn merge_charge(cx: &mut Cx, at: &At, th: Three<Charge>) -> Charge {
    let kind_text = |k: &crate::model::ChargeKind| {
        let probe = Charge {
            kind: k.clone(),
            ..th.o.clone()
        };
        summary(&charge_fields(&probe)[1..5])
    };
    Charge {
        name: th.o.name.clone(),
        account: th.field(cx, at, "account", "account", |c| &c.account, opt_text),
        applied: th.field(cx, at, "applied", "applied", |c| &c.applied, |v| on_off(*v)),
        kind: th.field(cx, at, "kind", "amount", |c| &c.kind, kind_text),
        excluded_amounts: th.field(
            cx,
            at,
            "excluded_amounts",
            "excluded amounts",
            |c| &c.excluded_amounts,
            |v| {
                if v.is_empty() {
                    "none".into()
                } else {
                    v.iter()
                        .map(|(d, a)| format!("{} {}", quoted(d), crate::diff::money(*a)))
                        .collect::<Vec<_>>()
                        .join("; ")
                }
            },
        ),
        excluded_groups: th.names(|c| &c.excluded_groups),
        excluded_categories: th.names(|c| &c.excluded_categories),
        excluded_accounts: th.names(|c| &c.excluded_accounts),
        label_percent: th.field(
            cx,
            at,
            "label_percent",
            "percent in label",
            |c| &c.label_percent,
            |v| on_off(*v),
        ),
        label_excluded: th.field(
            cx,
            at,
            "label_excluded",
            "excluded in label",
            |c| &c.label_excluded,
            |v| on_off(*v),
        ),
    }
}

/// Versions merged by id. Versions saved on both sides with one id are both
/// kept, theirs with a new id.
fn merge_versions(
    cx: &mut Cx,
    b: &[Version],
    o: &[Version],
    t: &[Version],
) -> (Vec<Version>, usize) {
    let by_id = |l: &[Version]| -> BTreeMap<VersionId, Version> {
        l.iter().map(|v| (v.id, v.clone())).collect()
    };
    let (mb, mo) = (by_id(b), by_id(o));
    let mut mt = BTreeMap::new();
    let mut next = b.iter().chain(o).chain(t).map(|v| v.id).max().unwrap_or(0);
    let mut renumbered = 0;
    let mut t_ids = Vec::with_capacity(t.len());
    for v in t {
        let clash = !mb.contains_key(&v.id) && mo.get(&v.id).is_some_and(|x| x != v);
        let mut v = v.clone();
        if clash {
            next = next.saturating_add(1);
            v.id = next;
            renumbered += 1;
        }
        t_ids.push(v.id);
        mt.insert(v.id, v);
    }
    let fields = |v: &Version| {
        vec![
            Field {
                key: "name",
                label: "name",
                value: crate::diff::Value::Text(quoted(&v.name)),
            },
            Field {
                key: "note",
                label: "note",
                value: crate::diff::Value::Text(opt_text(&v.note)),
            },
        ]
    };
    let ids: BTreeSet<VersionId> = mb
        .keys()
        .chain(mo.keys())
        .chain(mt.keys())
        .copied()
        .collect();
    let mut kept = BTreeMap::new();
    for id in ids {
        let name = mo
            .get(&id)
            .or_else(|| mt.get(&id))
            .or_else(|| mb.get(&id))
            .map(|v| v.name.clone())
            .unwrap_or_default();
        let at = At {
            key: format!("version:{id}"),
            place: Place::Versions,
            what: format!("Version \"{name}\""),
        };
        let item = |cx: &mut Cx, at: &At, th: Three<Version>| -> Version {
            let mut v = th.o.clone();
            v.name = th.field(cx, at, "name", "name", |v| &v.name, |s| quoted(s));
            v.note = th.field(cx, at, "note", "note", |v| &v.note, opt_text);
            v
        };
        if let Pick::Keep(v) = merge_item(
            cx,
            &at,
            (mb.get(&id), mo.get(&id), mt.get(&id)),
            &fields,
            &item,
        ) {
            kept.insert(id, v);
        }
    }
    let members: BTreeSet<VersionId> = kept.keys().copied().collect();
    let lb: Vec<VersionId> = b.iter().map(|v| v.id).collect();
    let lo: Vec<VersionId> = o.iter().map(|v| v.id).collect();
    let order = merge_order(cx, None, [&lb, &lo, &t_ids], &members, |_| String::new());
    (
        order
            .into_iter()
            .filter_map(|id| kept.remove(&id))
            .collect(),
        renumbered,
    )
}

// ---------------------------------------------------------------------------
// Categories, accounts and rows

type Pos = (usize, usize);

/// A topsheet row's or account's identity across the three budgets: its
/// place in the base, or (new) in ours, or (new in theirs only) in theirs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Id<P> {
    B(P),
    O(P),
    T(P),
}

type TId = Id<usize>;
type AId = Id<Pos>;

/// What becomes of a category, an account or a row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fate {
    /// On both sides (or new): fields merged.
    Merge,
    /// Deleted by one side and left alone by the other: gone.
    Gone,
    /// Deleted by one side, changed by the other, and kept: as `Side` has it.
    Kept(Side),
    /// Deleted by one side (`Side`), changed by the other, and deleted with
    /// everything in it.
    Dropped(Side),
}

impl Fate {
    fn alive(self) -> bool {
        matches!(self, Fate::Merge | Fate::Kept(_))
    }
}

#[derive(Clone, Copy)]
struct Slot<'a> {
    aid: AId,
    di: usize,
    d: &'a Detail,
}

struct RowEntry<'a> {
    at: [Option<Slot<'a>>; 3],
}

struct Structure<'a> {
    bud: [&'a Budget; 3],
    top_id: [HashMap<usize, TId>; 3],
    top_at: BTreeMap<TId, [Option<usize>; 3]>,
    acct_id: [HashMap<Pos, AId>; 3],
    acct_at: BTreeMap<AId, [Option<Pos>; 3]>,
    /// By the id the row has in the merged budget.
    rows: BTreeMap<RowId, RowEntry<'a>>,
    /// Each side's rows per account, in order, by merged id.
    side_rows: [BTreeMap<Pos, Vec<RowId>>; 3],
    renumbered: usize,
}

fn side_of(s: usize) -> Side {
    if s == T {
        Side::Theirs
    } else {
        Side::Ours
    }
}

fn index_of(s: Side) -> usize {
    match s {
        Side::Ours => O,
        Side::Theirs => T,
    }
}

fn other(s: usize) -> usize {
    if s == O {
        T
    } else {
        O
    }
}

impl<'a> Structure<'a> {
    fn new(b: &'a Budget, o: &'a Budget, t: &'a Budget) -> Structure<'a> {
        let mut s = Structure {
            bud: [b, o, t],
            top_id: Default::default(),
            top_at: BTreeMap::new(),
            acct_id: Default::default(),
            acct_at: BTreeMap::new(),
            rows: BTreeMap::new(),
            side_rows: Default::default(),
            renumbered: 0,
        };
        let mbo = match_topsheets(&b.topsheet, &o.topsheet);
        let mbt = match_topsheets(&b.topsheet, &t.topsheet);

        // Topsheet rows.
        for ti in 0..b.topsheet.len() {
            s.put_top(B, ti, Id::B(ti));
        }
        for ti in 0..o.topsheet.len() {
            s.put_top(O, ti, mbo.top_rev.get(&ti).map_or(Id::O(ti), |&x| Id::B(x)));
        }
        let new_o: Vec<(usize, String)> = (0..o.topsheet.len())
            .filter(|i| !mbo.top_rev.contains_key(i))
            .map(|i| (i, top_key(&o.topsheet[i])))
            .collect();
        let new_t: Vec<(usize, String)> = (0..t.topsheet.len())
            .filter(|i| !mbt.top_rev.contains_key(i))
            .map(|i| (i, top_key(&t.topsheet[i])))
            .collect();
        let same: HashMap<usize, usize> = pair_by_key(new_t, new_o).into_iter().collect();
        for ti in 0..t.topsheet.len() {
            let id = match (mbt.top_rev.get(&ti), same.get(&ti)) {
                (Some(&x), _) => Id::B(x),
                (None, Some(&x)) => Id::O(x),
                (None, None) => Id::T(ti),
            };
            s.put_top(T, ti, id);
        }

        // Accounts.
        let (al, ol, tl) = (
            account_list(&b.topsheet),
            account_list(&o.topsheet),
            account_list(&t.topsheet),
        );
        for (p, _) in &al {
            s.put_acct(B, *p, Id::B(*p));
        }
        for (p, _) in &ol {
            s.put_acct(O, *p, mbo.acct_rev.get(p).map_or(Id::O(*p), |&x| Id::B(x)));
        }
        let new_o: Vec<(Pos, String)> = ol
            .iter()
            .filter(|(p, _)| !mbo.acct_rev.contains_key(p))
            .map(|(p, a)| (*p, a.code.trim().to_string()))
            .collect();
        let new_t: Vec<(Pos, String)> = tl
            .iter()
            .filter(|(p, _)| !mbt.acct_rev.contains_key(p))
            .map(|(p, a)| (*p, a.code.trim().to_string()))
            .collect();
        let same: HashMap<Pos, Pos> = pair_by_key(new_t, new_o).into_iter().collect();
        for (p, _) in &tl {
            let id = match (mbt.acct_rev.get(p), same.get(p)) {
                (Some(&x), _) => Id::B(x),
                (None, Some(&x)) => Id::O(x),
                (None, None) => Id::T(*p),
            };
            s.put_acct(T, *p, id);
        }

        // Rows.
        let mut next = [b, o, t]
            .iter()
            .filter_map(|x| x.highest_row_id())
            .max()
            .unwrap_or(0);
        let mut fresh = || {
            next = next.saturating_add(1);
            next
        };
        for side in [B, O, T] {
            let mut seen = HashSet::new();
            for (p, acct) in account_list(&s.bud[side].topsheet) {
                let aid = s.acct_id[side][&p];
                for (di, d) in acct.details.iter().enumerate() {
                    let slot = Slot { aid, di, d };
                    let id = if !seen.insert(d.id) {
                        if side == B {
                            continue;
                        }
                        // An id used twice in one budget: the repeat is a
                        // row of its own.
                        fresh()
                    } else if side == T {
                        match s.rows.get(&d.id) {
                            Some(e) if e.at[B].is_some() => d.id,
                            Some(e) => match e.at[O] {
                                Some(x) if x.aid == aid && x.d == d => d.id,
                                Some(_) => {
                                    s.renumbered += 1;
                                    fresh()
                                }
                                None => d.id,
                            },
                            None => d.id,
                        }
                    } else {
                        d.id
                    };
                    s.rows
                        .entry(id)
                        .or_insert_with(|| RowEntry { at: [None; 3] })
                        .at[side] = Some(slot);
                    s.side_rows[side].entry(p).or_default().push(id);
                }
            }
        }
        s
    }

    fn put_top(&mut self, side: usize, ti: usize, id: TId) {
        self.top_id[side].insert(ti, id);
        self.top_at.entry(id).or_insert([None; 3])[side] = Some(ti);
    }

    fn put_acct(&mut self, side: usize, p: Pos, id: AId) {
        self.acct_id[side].insert(p, id);
        self.acct_at.entry(id).or_insert([None; 3])[side] = Some(p);
    }

    fn top_row(&self, side: usize, ti: usize) -> &'a TopsheetRow {
        &self.bud[side].topsheet[ti]
    }

    fn account(&self, side: usize, p: Pos) -> &'a Account {
        match &self.bud[side].topsheet[p.0] {
            TopsheetRow::Category(c) => &c.accounts[p.1],
            TopsheetRow::ProductionTotal(_) => unreachable!("accounts are in categories"),
        }
    }

    fn parent(&self, side: usize, p: Pos) -> TId {
        self.top_id[side][&p.0]
    }

    fn row_changed(e: &RowEntry, side: usize) -> bool {
        match (&e.at[B], &e.at[side]) {
            (_, None) => false,
            (None, Some(_)) => true,
            (Some(x), Some(y)) => x.aid != y.aid || x.d != y.d,
        }
    }

    /// Whether `side` changed account `aid` or anything in it.
    fn acct_touched(&self, side: usize, aid: AId, rows_touched: &BTreeSet<AId>) -> bool {
        let at = self.acct_at[&aid];
        let Some(p) = at[side] else { return false };
        let Some(bp) = at[B] else { return true };
        let (x, y) = (self.account(B, bp), self.account(side, p));
        x.code != y.code
            || x.name != y.name
            || x.notes != y.notes
            || self.parent(B, bp) != self.parent(side, p)
            || rows_touched.contains(&aid)
    }

    /// Words for a category or production total.
    fn top_what(&self, tid: TId) -> (String, Place) {
        let at = self.top_at[&tid];
        let side = [O, T, B]
            .into_iter()
            .find(|s| at[*s].is_some())
            .unwrap_or(B);
        let ti = at[side].unwrap_or(0);
        match self.top_row(side, ti) {
            TopsheetRow::Category(c) => (
                format!("Category {}", c.code.trim()),
                Place::Topsheet {
                    code: Some(c.code.clone()),
                    name: c.name.clone(),
                    ti: at[O],
                },
            ),
            TopsheetRow::ProductionTotal(p) => (
                format!("Production total {}", quoted(&p.name)),
                Place::Topsheet {
                    code: None,
                    name: p.name.clone(),
                    ti: at[O],
                },
            ),
        }
    }

    fn acct_at_words(&self, aid: AId) -> At {
        let at = self.acct_at[&aid];
        let side = [O, T, B]
            .into_iter()
            .find(|s| at[*s].is_some())
            .unwrap_or(B);
        let p = at[side].unwrap_or((0, 0));
        let a = self.account(side, p);
        let category = match self.top_row(side, p.0) {
            TopsheetRow::Category(c) => c.code.clone(),
            TopsheetRow::ProductionTotal(_) => String::new(),
        };
        At {
            key: format!("account:{}", id_key(aid)),
            place: Place::Account {
                category,
                code: a.code.clone(),
                name: a.name.clone(),
                at: at[O],
                row: None,
                line: None,
            },
            what: format!("Account {}", a.code.trim()),
        }
    }

    fn row_at_words(&self, id: RowId, e: &RowEntry) -> At {
        let side = [O, T, B]
            .into_iter()
            .find(|s| e.at[*s].is_some())
            .unwrap_or(B);
        let Some(slot) = e.at[side] else {
            return At {
                key: format!("row:{id}"),
                place: Place::Budget,
                what: format!("Row {id}"),
            };
        };
        let p = self.acct_at[&slot.aid][side].unwrap_or((0, 0));
        let a = self.account(side, p);
        let category = match self.top_row(side, p.0) {
            TopsheetRow::Category(c) => c.code.clone(),
            TopsheetRow::ProductionTotal(_) => String::new(),
        };
        At {
            key: format!("row:{id}"),
            place: Place::Account {
                category,
                code: a.code.clone(),
                name: a.name.clone(),
                at: if side == O { Some(p) } else { None },
                row: Some(id),
                line: Some(slot.di + 1),
            },
            what: format!("{} line {}", a.code.trim(), slot.di + 1),
        }
    }

    fn merge(self, cx: &mut Cx) -> (Vec<TopsheetRow>, usize) {
        // What each side changed, bottom up.
        let mut rows_touched: [BTreeSet<AId>; 3] = Default::default();
        for e in self.rows.values() {
            for side in [O, T] {
                if Self::row_changed(e, side) {
                    if let Some(s) = e.at[side] {
                        rows_touched[side].insert(s.aid);
                    }
                }
            }
        }
        let mut accts_touched_in: [BTreeSet<TId>; 3] = Default::default();
        for (aid, at) in &self.acct_at {
            for side in [O, T] {
                if let Some(p) = at[side] {
                    if self.acct_touched(side, *aid, &rows_touched[side]) {
                        accts_touched_in[side].insert(self.parent(side, p));
                    }
                }
            }
        }
        let top_touched = |side: usize, tid: TId| -> bool {
            let at = self.top_at[&tid];
            let Some(ti) = at[side] else { return false };
            let Some(bti) = at[B] else { return true };
            let differs = match (self.top_row(B, bti), self.top_row(side, ti)) {
                (TopsheetRow::Category(x), TopsheetRow::Category(y)) => {
                    x.code != y.code || x.name != y.name || x.notes != y.notes
                }
                (x, y) => x != y,
            };
            differs || accts_touched_in[side].contains(&tid)
        };

        // Categories and production totals.
        let mut top_fate: BTreeMap<TId, Fate> = BTreeMap::new();
        for (&tid, at) in &self.top_at {
            let fate = match (at[B], at[O], at[T]) {
                (Some(_), Some(_), Some(_)) | (None, _, _) => Fate::Merge,
                (Some(_), None, None) => Fate::Gone,
                (Some(_), o, _) => {
                    let keeper = if o.is_some() { O } else { T };
                    if !top_touched(keeper, tid) {
                        Fate::Gone
                    } else {
                        let (what, place) = self.top_what(tid);
                        self.container_clash(
                            cx,
                            format!("topsheet:{}", id_key(tid)),
                            place,
                            what,
                            keeper,
                        )
                    }
                }
            };
            top_fate.insert(tid, fate);
        }

        // Accounts: their fate, then where they go.
        let mut acct_fate: BTreeMap<AId, Fate> = BTreeMap::new();
        for (&aid, at) in &self.acct_at {
            let fate = match (at[B], at[O], at[T]) {
                (Some(_), Some(_), Some(_)) | (None, _, _) => Fate::Merge,
                (Some(_), None, None) => Fate::Gone,
                (Some(bp), o, _) => {
                    let keeper = if o.is_some() { O } else { T };
                    let deleter = other(keeper);
                    let kp = at[keeper].unwrap_or(bp);
                    let base_parent = self.parent(B, bp);
                    if self.top_at[&base_parent][deleter].is_none()
                        && self.parent(keeper, kp) == base_parent
                    {
                        // Gone with its category: it goes the category's way.
                        top_fate[&base_parent]
                    } else if !self.acct_touched(keeper, aid, &rows_touched[keeper]) {
                        Fate::Gone
                    } else {
                        let w = self.acct_at_words(aid);
                        self.container_clash(cx, w.key, w.place, w.what, keeper)
                    }
                }
            };
            acct_fate.insert(aid, fate);
        }
        let mut acct_parent: BTreeMap<AId, TId> = BTreeMap::new();
        for (&aid, at) in &self.acct_at {
            let fate = acct_fate[&aid];
            if !fate.alive() {
                continue;
            }
            let par = |side: usize| at[side].map(|p| self.parent(side, p));
            let wanted = match fate {
                Fate::Kept(s) => par(index_of(s)),
                _ => match (par(O), par(T)) {
                    (Some(x), Some(y)) => {
                        let w = self.acct_at_words(aid);
                        let words = |tid: &TId| self.top_what(*tid).0;
                        Some(match pick3(par(B).as_ref(), Some(&x), Some(&y)) {
                            Some(v) => *v.unwrap_or(&x),
                            None => match cx.decide(Clash {
                                key: format!("{}:category", w.key),
                                place: w.place.clone(),
                                what: format!("{} category", w.what),
                                base: par(B).as_ref().map(words),
                                ours: words(&x),
                                theirs: words(&y),
                            }) {
                                Side::Ours => x,
                                Side::Theirs => y,
                            },
                        })
                    }
                    (x, y) => x.or(y),
                },
            };
            let placed = wanted.and_then(|w| self.settle(w, &top_fate, aid_parents(&self, at)));
            match placed {
                Some(p) => {
                    acct_parent.insert(aid, p);
                }
                None => {
                    acct_fate.insert(aid, Fate::Gone);
                }
            }
        }

        // Rows: their fate, fields and account.
        let mut row_acct: BTreeMap<RowId, AId> = BTreeMap::new();
        let mut row_detail: BTreeMap<RowId, Detail> = BTreeMap::new();
        for (&id, e) in &self.rows {
            let w = self.row_at_words(id, e);
            let kept: Option<(Detail, Option<AId>)> = match (e.at[B], e.at[O], e.at[T]) {
                (b, Some(x), Some(y)) => {
                    let d = merge_detail(
                        cx,
                        &w,
                        Three {
                            b: b.map(|s| s.d),
                            o: x.d,
                            t: y.d,
                        },
                        id,
                    );
                    let target = match pick3(b.map(|s| s.aid).as_ref(), Some(&x.aid), Some(&y.aid))
                    {
                        Some(v) => *v.unwrap_or(&x.aid),
                        None => {
                            let words = |aid: &AId| self.acct_at_words(*aid).what;
                            match cx.decide(Clash {
                                key: format!("{}:account", w.key),
                                place: w.place.clone(),
                                what: format!("{} account", w.what),
                                base: b.map(|s| words(&s.aid)),
                                ours: words(&x.aid),
                                theirs: words(&y.aid),
                            }) {
                                Side::Ours => x.aid,
                                Side::Theirs => y.aid,
                            }
                        }
                    };
                    Some((d, Some(target)))
                }
                (None, x, y) => x.or(y).map(|s| (with_id(s.d, id), Some(s.aid))),
                (Some(_), None, None) => None,
                (Some(bs), x, y) => {
                    let keeper = if x.is_some() { O } else { T };
                    let deleter = other(keeper);
                    let ks = x.or(y).unwrap_or(bs);
                    let gone_with_account =
                        self.acct_at[&bs.aid][deleter].is_none() && ks.aid == bs.aid;
                    let keep = if gone_with_account {
                        acct_fate[&bs.aid] == Fate::Kept(side_of(keeper))
                    } else if !Self::row_changed(e, keeper) {
                        false
                    } else {
                        let changes: Vec<String> = row_changes(bs.d, ks.d)
                            .into_iter()
                            .map(|f| f.text)
                            .collect();
                        let mut changes = changes;
                        if ks.aid != bs.aid {
                            changes
                                .insert(0, format!("moved to {}", self.acct_at_words(ks.aid).what));
                        }
                        let kept_side = side_of(keeper);
                        let side = cx.decide(Clash {
                            key: w.key.clone(),
                            place: w.place.clone(),
                            what: format!("{} ({})", w.what, row_name(bs.d)),
                            base: Some(row_name(bs.d)),
                            ours: if keeper == O {
                                changed_text(changes.clone())
                            } else {
                                "deleted".into()
                            },
                            theirs: if keeper == T {
                                changed_text(changes)
                            } else {
                                "deleted".into()
                            },
                        });
                        side == kept_side
                    };
                    keep.then(|| (with_id(ks.d, id), Some(ks.aid)))
                }
            };
            let Some((d, Some(target))) = kept else {
                continue;
            };
            let placed = self.settle_row(target, &acct_fate, e);
            if let Some(a) = placed {
                row_acct.insert(id, a);
                row_detail.insert(id, d);
            }
        }

        // Order and assembly.
        let live_tops: BTreeSet<TId> = top_fate
            .iter()
            .filter(|(_, f)| f.alive())
            .map(|(t, _)| *t)
            .collect();
        let top_lists: [Vec<TId>; 3] = [B, O, T].map(|s| {
            (0..self.bud[s].topsheet.len())
                .map(|ti| self.top_id[s][&ti])
                .collect()
        });
        let top_order = merge_order(
            cx,
            Some(&At {
                key: "topsheet".into(),
                place: Place::Budget,
                what: "Topsheet".into(),
            }),
            [&top_lists[0], &top_lists[1], &top_lists[2]],
            &live_tops,
            |l| order_text(l.iter().map(|t| self.top_what(*t).0).collect()),
        );
        let mut topsheet = Vec::with_capacity(top_order.len());
        for tid in top_order {
            let at = self.top_at[&tid];
            let fate = top_fate[&tid];
            let side_row = |s: usize| at[s].map(|ti| self.top_row(s, ti));
            let row = match fate {
                Fate::Kept(s) => side_row(index_of(s)),
                _ => side_row(O).or(side_row(T)),
            };
            match row {
                Some(TopsheetRow::ProductionTotal(p)) => {
                    topsheet.push(TopsheetRow::ProductionTotal(ProductionTotal {
                        name: p.name.clone(),
                    }));
                }
                Some(TopsheetRow::Category(_)) => {
                    let cat = |s: usize| match side_row(s) {
                        Some(TopsheetRow::Category(c)) => Some(c),
                        _ => None,
                    };
                    let (what, place) = self.top_what(tid);
                    let w = At {
                        key: format!("topsheet:{}", id_key(tid)),
                        place,
                        what,
                    };
                    let (code, name, notes) = match (fate, cat(O), cat(T)) {
                        (Fate::Kept(s), _, _) => {
                            let c = cat(index_of(s)).expect("kept from a side that has it");
                            (c.code.clone(), c.name.clone(), c.notes.clone())
                        }
                        (_, Some(x), Some(y)) => {
                            let th = Three {
                                b: cat(B),
                                o: x,
                                t: y,
                            };
                            (
                                th.field(
                                    cx,
                                    &w,
                                    "code",
                                    "number",
                                    |c: &Category| &c.code,
                                    |v| v.clone(),
                                ),
                                th.field(
                                    cx,
                                    &w,
                                    "name",
                                    "name",
                                    |c: &Category| &c.name,
                                    |v| quoted(v),
                                ),
                                merge_notes(
                                    cx,
                                    &w,
                                    cat(B).map(|c| c.notes.as_slice()),
                                    &x.notes,
                                    &y.notes,
                                ),
                            )
                        }
                        (_, x, y) => {
                            let c = x.or(y).expect("on one side at least");
                            (c.code.clone(), c.name.clone(), c.notes.clone())
                        }
                    };
                    let members: BTreeSet<AId> = acct_parent
                        .iter()
                        .filter(|(a, p)| **p == tid && acct_fate[*a].alive())
                        .map(|(a, _)| *a)
                        .collect();
                    let lists: [Vec<AId>; 3] = [B, O, T].map(|s| match cat(s) {
                        Some(c) => {
                            let ti = at[s].unwrap_or(0);
                            (0..c.accounts.len())
                                .map(|ai| self.acct_id[s][&(ti, ai)])
                                .collect()
                        }
                        None => Vec::new(),
                    });
                    let order = merge_order(
                        cx,
                        Some(&w),
                        [&lists[0], &lists[1], &lists[2]],
                        &members,
                        |l| order_text(l.iter().map(|a| self.acct_at_words(*a).what).collect()),
                    );
                    let accounts = order
                        .into_iter()
                        .map(|aid| {
                            self.build_account(cx, aid, acct_fate[&aid], &row_acct, &mut row_detail)
                        })
                        .collect();
                    topsheet.push(TopsheetRow::Category(Category {
                        code,
                        name,
                        accounts,
                        notes,
                    }));
                }
                None => {}
            }
        }
        (topsheet, self.renumbered)
    }

    /// One conflict for a container one side deleted and the other changed.
    fn container_clash(
        &self,
        cx: &mut Cx,
        key: String,
        place: Place,
        what: String,
        keeper: usize,
    ) -> Fate {
        let side = cx.decide(Clash {
            key,
            place,
            what,
            base: None,
            ours: if keeper == O {
                "changed".into()
            } else {
                "deleted".into()
            },
            theirs: if keeper == T {
                "changed".into()
            } else {
                "deleted".into()
            },
        });
        if side == side_of(keeper) {
            Fate::Kept(side)
        } else {
            Fate::Dropped(side_of(other(keeper)))
        }
    }

    /// Where an account whose category is `wanted` goes: there when that
    /// category stays; where the deleting side put it when that category
    /// was deleted with what was in it; else the first of its categories
    /// (ours, theirs, the base's) that stays. `None`: nowhere (it goes).
    fn settle(
        &self,
        wanted: TId,
        top_fate: &BTreeMap<TId, Fate>,
        parents: [Option<TId>; 3],
    ) -> Option<TId> {
        let fate = |t: &TId| top_fate.get(t).copied().unwrap_or(Fate::Gone);
        if fate(&wanted).alive() {
            return Some(wanted);
        }
        if let Fate::Dropped(d) = fate(&wanted) {
            return parents[index_of(d)].filter(|p| fate(p).alive());
        }
        [parents[O], parents[T], parents[B]]
            .into_iter()
            .flatten()
            .find(|p| fate(p).alive())
    }

    /// The same for a row whose account is `target`.
    fn settle_row(
        &self,
        target: AId,
        acct_fate: &BTreeMap<AId, Fate>,
        e: &RowEntry,
    ) -> Option<AId> {
        let fate = |a: &AId| acct_fate.get(a).copied().unwrap_or(Fate::Gone);
        if fate(&target).alive() {
            return Some(target);
        }
        let at = |s: usize| e.at[s].map(|x| x.aid);
        if let Fate::Dropped(d) = fate(&target) {
            return at(index_of(d)).filter(|a| fate(a).alive());
        }
        [at(O), at(T), at(B)]
            .into_iter()
            .flatten()
            .find(|a| fate(a).alive())
    }

    fn build_account(
        &self,
        cx: &mut Cx,
        aid: AId,
        fate: Fate,
        row_acct: &BTreeMap<RowId, AId>,
        row_detail: &mut BTreeMap<RowId, Detail>,
    ) -> Account {
        let at = self.acct_at[&aid];
        let acct = |s: usize| at[s].map(|p| self.account(s, p));
        let w = self.acct_at_words(aid);
        let (code, name, notes) = match (fate, acct(O), acct(T)) {
            (Fate::Kept(s), _, _) => {
                let a = acct(index_of(s)).expect("kept from a side that has it");
                (a.code.clone(), a.name.clone(), a.notes.clone())
            }
            (_, Some(x), Some(y)) => {
                let th = Three {
                    b: acct(B),
                    o: x,
                    t: y,
                };
                (
                    th.field(
                        cx,
                        &w,
                        "code",
                        "number",
                        |a: &Account| &a.code,
                        |v| v.clone(),
                    ),
                    th.field(cx, &w, "name", "name", |a: &Account| &a.name, |v| quoted(v)),
                    merge_notes(
                        cx,
                        &w,
                        acct(B).map(|a| a.notes.as_slice()),
                        &x.notes,
                        &y.notes,
                    ),
                )
            }
            (_, x, y) => {
                let a = x.or(y).expect("on one side at least");
                (a.code.clone(), a.name.clone(), a.notes.clone())
            }
        };
        let members: BTreeSet<RowId> = row_acct
            .iter()
            .filter(|(_, a)| **a == aid)
            .map(|(id, _)| *id)
            .collect();
        let empty = Vec::new();
        let lists: [&Vec<RowId>; 3] = [B, O, T].map(|s| match at[s] {
            Some(p) => self.side_rows[s].get(&p).unwrap_or(&empty),
            None => &empty,
        });
        let order = merge_order(
            cx,
            Some(&w),
            [lists[0], lists[1], lists[2]],
            &members,
            |l| {
                order_text(
                    l.iter()
                        .map(|id| {
                            row_detail
                                .get(id)
                                .map(|d| d.description.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .unwrap_or_else(|| format!("row {id}"))
                        })
                        .collect(),
                )
            },
        );
        Account {
            code,
            name,
            details: order
                .into_iter()
                .filter_map(|id| row_detail.remove(&id))
                .collect(),
            notes,
        }
    }
}

/// An account's category on each side.
fn aid_parents(s: &Structure, at: &[Option<Pos>; 3]) -> [Option<TId>; 3] {
    [B, O, T].map(|side| at[side].map(|p| s.parent(side, p)))
}

fn id_key<P: std::fmt::Debug>(id: Id<P>) -> String {
    let s = match id {
        Id::B(p) => format!("b{p:?}"),
        Id::O(p) => format!("o{p:?}"),
        Id::T(p) => format!("t{p:?}"),
    };
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == ',')
        .collect()
}

fn with_id(d: &Detail, id: RowId) -> Detail {
    let mut d = d.clone();
    d.id = id;
    d
}

/// A row on both sides, field by field. Tags merge name by name; notes note
/// by note.
fn merge_detail(cx: &mut Cx, at: &At, th: Three<Detail>, id: RowId) -> Detail {
    let text = |v: &String| quoted(v);
    let unit = |v: &String| {
        if v.is_empty() {
            "none".to_string()
        } else {
            v.clone()
        }
    };
    let num = |v: &crate::Formula| formula_text(&v.0, false);
    Detail {
        id,
        kind: th.field(
            cx,
            at,
            "kind",
            "kind",
            |d| &d.kind,
            |k| kind_name(*k).into(),
        ),
        description: th.field(
            cx,
            at,
            "description",
            "description",
            |d| &d.description,
            text,
        ),
        amount: th.field(cx, at, "amount", "amount", |d| &d.amount, num),
        unit: th.field(cx, at, "unit", "unit", |d| &d.unit, unit),
        x: th.field(cx, at, "x", "X", |d| &d.x, num),
        unit2: th.field(cx, at, "unit2", "X unit", |d| &d.unit2, unit),
        rate: th.field(
            cx,
            at,
            "rate",
            "rate",
            |d| &d.rate,
            |v| formula_text(&v.0, true),
        ),
        unit3: th.field(cx, at, "unit3", "rate unit", |d| &d.unit3, unit),
        x4: th.field(cx, at, "x4", "4X", |d| &d.x4, num),
        unit4: th.field(cx, at, "unit4", "4X unit", |d| &d.unit4, unit),
        currency: th.field(
            cx,
            at,
            "currency",
            "currency",
            |d| &d.currency,
            |c| c.clone().unwrap_or_else(|| "base".into()),
        ),
        fringes: th.names(|d| &d.fringes),
        groups: th.names(|d| &d.groups),
        location: th.field(cx, at, "location", "location", |d| &d.location, opt_text),
        set: th.field(cx, at, "set", "set", |d| &d.set, opt_text),
        fringe_range: th.field(
            cx,
            at,
            "fringe_range",
            "fringe range",
            |d| &d.fringe_range,
            |r| r.map_or("none".into(), |n| n.to_string()),
        ),
        bold: th.field(cx, at, "bold", "bold", |d| &d.bold, |v| on_off(*v)),
        italic: th.field(cx, at, "italic", "italic", |d| &d.italic, |v| on_off(*v)),
        indent: th.field(cx, at, "indent", "indent", |d| &d.indent, |v| v.to_string()),
        color: th.field(cx, at, "color", "color", |d| &d.color, opt_text),
        notes: merge_notes(
            cx,
            at,
            th.b.map(|d| d.notes.as_slice()),
            &th.o.notes,
            &th.t.notes,
        ),
    }
}
