//! Hand-made display order for the setup tables (roadmap 2.12).
//!
//! Units, globals, fringes, groups, sets, locations and currencies are maps
//! sorted by name. A budget can also keep an explicit order per table in
//! [`SetupOrder`]: names, first to last. The order a table shows in is the
//! listed names that still exist, each once, in that order, then every other
//! name in byte order (so an item added later goes to the end). An empty list
//! means byte order. Currencies are the foreign ones: the base currency is not
//! in the table.
//!
//! Display only: no amount, name or assignment depends on it. Layout, not
//! content, like report presets: versions don't carry it, and restoring one
//! keeps the current order (names a version doesn't have are skipped).

use std::collections::BTreeSet;

use crate::model::Budget;
use crate::rename::SetupKind;

/// Each setup table's hand-made order. Empty lists are byte order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SetupOrder {
    pub units: Vec<String>,
    pub globals: Vec<String>,
    pub fringes: Vec<String>,
    pub groups: Vec<String>,
    pub sets: Vec<String>,
    pub locations: Vec<String>,
    pub currencies: Vec<String>,
}

impl SetupOrder {
    /// The list kept for `kind`'s table.
    pub fn get(&self, kind: SetupKind) -> &[String] {
        match kind {
            SetupKind::Unit => &self.units,
            SetupKind::Global => &self.globals,
            SetupKind::Fringe => &self.fringes,
            SetupKind::Group => &self.groups,
            SetupKind::Set => &self.sets,
            SetupKind::Location => &self.locations,
            SetupKind::Currency => &self.currencies,
        }
    }

    pub fn get_mut(&mut self, kind: SetupKind) -> &mut Vec<String> {
        match kind {
            SetupKind::Unit => &mut self.units,
            SetupKind::Global => &mut self.globals,
            SetupKind::Fringe => &mut self.fringes,
            SetupKind::Group => &mut self.groups,
            SetupKind::Set => &mut self.sets,
            SetupKind::Location => &mut self.locations,
            SetupKind::Currency => &mut self.currencies,
        }
    }

    /// True when every table is in byte order.
    pub fn is_empty(&self) -> bool {
        SetupKind::ALL.iter().all(|k| self.get(*k).is_empty())
    }
}

/// The names in `kind`'s table, in byte order.
pub fn names(b: &Budget, kind: SetupKind) -> Vec<String> {
    fn keys<V>(m: &std::collections::BTreeMap<String, V>) -> Vec<String> {
        m.keys().cloned().collect()
    }
    match kind {
        SetupKind::Unit => keys(&b.units),
        SetupKind::Global => keys(&b.globals),
        SetupKind::Fringe => keys(&b.fringes),
        SetupKind::Group => keys(&b.groups),
        SetupKind::Set => keys(&b.sets),
        SetupKind::Location => keys(&b.locations),
        SetupKind::Currency => keys(&b.currencies),
    }
}

/// `first` (names in `all` only, each once), then the rest of `all` in its
/// own order.
fn arrange(all: &[String], first: &[String]) -> Vec<String> {
    let have: BTreeSet<&str> = all.iter().map(String::as_str).collect();
    let mut seen = BTreeSet::new();
    let mut out: Vec<String> = first
        .iter()
        .filter(|n| have.contains(n.as_str()) && seen.insert(n.as_str()))
        .cloned()
        .collect();
    out.extend(all.iter().filter(|n| !seen.contains(n.as_str())).cloned());
    out
}

/// The order `kind`'s table shows in: every name, once.
pub fn ordered(b: &Budget, kind: SetupKind) -> Vec<String> {
    arrange(&names(b, kind), b.setup_order.get(kind))
}

/// What a file keeps for `kind`: the shown order when it isn't byte order,
/// else nothing.
pub fn normalized(b: &Budget, kind: SetupKind) -> Vec<String> {
    let all = names(b, kind);
    let shown = arrange(&all, b.setup_order.get(kind));
    if shown == all {
        Vec::new()
    } else {
        shown
    }
}

/// Every table's [`normalized`] list.
pub fn normalized_all(b: &Budget) -> SetupOrder {
    let mut out = SetupOrder::default();
    for &k in SetupKind::ALL {
        *out.get_mut(k) = normalized(b, k);
    }
    out
}

/// Sets `kind`'s order: `first` (the names that exist, each once), then the
/// rest in byte order. Kept empty when that is byte order.
pub fn set_order(b: &mut Budget, kind: SetupKind, first: &[String]) {
    let all = names(b, kind);
    let shown = arrange(&all, first);
    *b.setup_order.get_mut(kind) = if shown == all { Vec::new() } else { shown };
}

/// Moves `name` `delta` places in `kind`'s shown order (negative is up),
/// stopping at either end. False when it isn't there or can't move.
pub fn move_by(b: &mut Budget, kind: SetupKind, name: &str, delta: isize) -> bool {
    let mut shown = ordered(b, kind);
    let Some(i) = shown.iter().position(|n| n == name) else {
        return false;
    };
    let to = i
        .saturating_add_signed(delta)
        .min(shown.len().saturating_sub(1));
    if to == i {
        return false;
    }
    let item = shown.remove(i);
    shown.insert(to, item);
    set_order(b, kind, &shown);
    true
}

/// Drops `name` from `kind`'s order (it was deleted), keeping the rest.
pub fn forget(b: &mut Budget, kind: SetupKind, name: &str) {
    let list = b.setup_order.get(kind);
    if list.is_empty() {
        return;
    }
    let rest: Vec<String> = list.iter().filter(|n| *n != name).cloned().collect();
    set_order(b, kind, &rest);
}

/// `from` becomes `to` in `kind`'s order (it was renamed), keeping its place.
pub fn rename_in(b: &mut Budget, kind: SetupKind, from: &str, to: &str) {
    for n in b.setup_order.get_mut(kind).iter_mut() {
        if n == from {
            *n = to.to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{SetDef, TopsheetRow};

    fn budget(sets: &[&str]) -> Budget {
        let mut b = Budget::new("T", "USD");
        for s in sets {
            b.sets.insert((*s).into(), SetDef::default());
        }
        b.topsheet = Vec::<TopsheetRow>::new();
        b
    }

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn empty_is_byte_order_and_new_items_go_last() {
        let mut b = budget(&["b", "a", "c"]);
        assert_eq!(ordered(&b, SetupKind::Set), s(&["a", "b", "c"]));
        set_order(&mut b, SetupKind::Set, &s(&["c", "a"]));
        assert_eq!(b.setup_order.sets, s(&["c", "a", "b"]));
        b.sets.insert("0".into(), SetDef::default());
        assert_eq!(ordered(&b, SetupKind::Set), s(&["c", "a", "b", "0"]));
        assert_eq!(normalized(&b, SetupKind::Set), s(&["c", "a", "b", "0"]));
    }

    #[test]
    fn byte_order_is_kept_as_nothing() {
        let mut b = budget(&["a", "b"]);
        set_order(&mut b, SetupKind::Set, &s(&["a", "b", "zz", "a"]));
        assert!(b.setup_order.is_empty());
        set_order(&mut b, SetupKind::Set, &s(&["b"]));
        assert_eq!(b.setup_order.sets, s(&["b", "a"]));
        // Stale names are skipped when shown and written.
        b.setup_order.sets = s(&["gone", "b", "b", "a"]);
        assert_eq!(ordered(&b, SetupKind::Set), s(&["b", "a"]));
        assert_eq!(normalized(&b, SetupKind::Set), s(&["b", "a"]));
        b.setup_order.sets = s(&["gone", "a"]);
        assert!(normalized(&b, SetupKind::Set).is_empty());
    }

    #[test]
    fn moving_forgetting_and_renaming() {
        let mut b = budget(&["a", "b", "c"]);
        assert!(move_by(&mut b, SetupKind::Set, "c", -1));
        assert_eq!(ordered(&b, SetupKind::Set), s(&["a", "c", "b"]));
        assert!(!move_by(&mut b, SetupKind::Set, "a", -1), "already first");
        assert!(move_by(&mut b, SetupKind::Set, "a", 5), "stops at the end");
        assert_eq!(ordered(&b, SetupKind::Set), s(&["c", "b", "a"]));
        assert!(!move_by(&mut b, SetupKind::Set, "nope", 1));
        let def = b.sets.remove("b").unwrap();
        b.sets.insert("bb".into(), def);
        rename_in(&mut b, SetupKind::Set, "b", "bb");
        assert_eq!(b.setup_order.sets, s(&["c", "bb", "a"]));
        b.sets.remove("c");
        forget(&mut b, SetupKind::Set, "c");
        // bb, a is not byte order ("a" < "bb"), so it stays.
        assert_eq!(b.setup_order.sets, s(&["bb", "a"]));
        b.sets.remove("bb");
        forget(&mut b, SetupKind::Set, "bb");
        assert!(b.setup_order.is_empty());
    }
}
