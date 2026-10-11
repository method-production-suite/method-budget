//! How much each setup item is used (roadmap 2.9): the lines that name it,
//! and the other setup that refers to it, counted in one pass over the rows.

use std::collections::{BTreeMap, BTreeSet};

use crate::model::{Budget, ChargeKind, CreditSource, Detail, Formula, FringeKind};
use crate::rename::SetupKind;

/// One item's use.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ItemUse {
    /// Detail rows that name it (a row counts once, however many of its
    /// fields do). Only the budget's own rows: not line blocks or versions.
    pub lines: usize,
    /// Other setup that names it: globals' formulas and fringe rates (a
    /// global), flat fringes and globals' units (a unit), credit rows (a
    /// fringe or group), charges (a group or currency), the totals currency.
    pub other: usize,
}

impl ItemUse {
    pub fn is_used(&self) -> bool {
        self.lines > 0 || self.other > 0
    }
}

/// Every global-name token in a formula, as the formula reader splits them
/// (a run of digits and dots is a number, a letter or `_` starts a name).
pub fn formula_names(src: &str) -> impl Iterator<Item = &str> {
    crate::expr::name_spans(src).map(move |r| &src[r])
}

/// The names of `kind` one row refers to, each once.
fn row_names<'a>(d: &'a Detail, kind: SetupKind, out: &mut BTreeSet<&'a str>) {
    out.clear();
    let opt = |s: &'a Option<String>| s.as_deref();
    match kind {
        SetupKind::Global => {
            for f in [&d.amount, &d.x, &d.rate, &d.x4] {
                out.extend(formula_names(&f.0));
            }
        }
        SetupKind::Fringe => out.extend(d.fringes.iter().map(String::as_str)),
        SetupKind::Group => out.extend(d.groups.iter().map(String::as_str)),
        SetupKind::Unit => out.extend(
            [&d.unit, &d.unit2, &d.unit3, &d.unit4]
                .into_iter()
                .map(String::as_str)
                .filter(|u| !u.is_empty()),
        ),
        SetupKind::Set => out.extend(opt(&d.set)),
        SetupKind::Location => out.extend(opt(&d.location)),
        SetupKind::Currency => out.extend(opt(&d.currency)),
    }
}

/// Use of every item in `kind`'s table (items nothing uses included, at 0).
pub fn usage(b: &Budget, kind: SetupKind) -> BTreeMap<String, ItemUse> {
    let mut out: BTreeMap<String, ItemUse> = crate::order::names(b, kind)
        .into_iter()
        .map(|n| (n, ItemUse::default()))
        .collect();
    let mut bump = |name: &str, line: bool| {
        if let Some(u) = out.get_mut(name) {
            if line {
                u.lines += 1;
            } else {
                u.other += 1;
            }
        }
    };
    let mut seen = BTreeSet::new();
    for d in b
        .categories()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.details)
    {
        row_names(d, kind, &mut seen);
        for n in &seen {
            bump(n, true);
        }
    }
    fn once(f: &Formula) -> BTreeSet<&str> {
        formula_names(&f.0).collect()
    }
    match kind {
        SetupKind::Global => {
            for (name, f) in &b.globals {
                for n in once(f) {
                    if n != name {
                        bump(n, false);
                    }
                }
            }
            for fr in b.fringes.values() {
                for n in once(&fr.rate) {
                    bump(n, false);
                }
            }
        }
        SetupKind::Unit => {
            for fr in b.fringes.values() {
                if let FringeKind::Flat { unit: Some(u), .. } = &fr.kind {
                    bump(u, false);
                }
            }
            for g in b.global_info.values() {
                if !g.unit.is_empty() {
                    bump(&g.unit, false);
                }
            }
        }
        SetupKind::Fringe | SetupKind::Group => {
            for r in b.credits.iter().flat_map(|c| &c.rows) {
                if let CreditSource::Group {
                    group,
                    excluded_fringes,
                } = &r.source
                {
                    if kind == SetupKind::Group {
                        bump(group, false);
                    } else {
                        for f in excluded_fringes {
                            bump(f, false);
                        }
                    }
                }
            }
            if kind == SetupKind::Group {
                for c in &b.charges {
                    for g in &c.excluded_groups {
                        bump(g, false);
                    }
                }
            }
        }
        SetupKind::Currency => {
            for c in &b.charges {
                if let ChargeKind::Flat {
                    currency: Some(cur),
                    ..
                } = &c.kind
                {
                    bump(cur, false);
                }
            }
            if let Some(t) = &b.totals_currency {
                bump(t, false);
            }
        }
        SetupKind::Set | SetupKind::Location => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formula_names_split_like_the_reader() {
        let v: Vec<&str> = formula_names("DAYS*2 + SHOOT_DAYS/(x1 - 3.5) + _a").collect();
        assert_eq!(v, ["DAYS", "SHOOT_DAYS", "x1", "_a"]);
        assert_eq!(formula_names("1.5 é 2").count(), 0);
        assert_eq!(formula_names("").count(), 0);
    }

    #[test]
    fn rows_count_once_and_setup_counts_apart() {
        let mut b = crate::starter::budget();
        b.globals.insert("A".into(), "B * 2".into());
        b.globals.insert("B".into(), "3".into());
        let id = b
            .categories()
            .flat_map(|c| &c.accounts)
            .flat_map(|a| &a.details)
            .next()
            .unwrap()
            .id;
        for d in b
            .categories_mut()
            .flat_map(|c| &mut c.accounts)
            .flat_map(|a| &mut a.details)
        {
            if d.id == id {
                d.amount = "A + A".into();
                d.rate = "A".into();
                d.unit = "Week".into();
                d.unit2 = "Week".into();
            }
        }
        let g = usage(&b, SetupKind::Global);
        assert_eq!(g["A"], ItemUse { lines: 1, other: 0 });
        assert_eq!(g["B"], ItemUse { lines: 0, other: 1 });
        assert!(g["B"].is_used());
        let u = usage(&b, SetupKind::Unit);
        assert!(u["Week"].lines >= 1);
        // Names lines use without a table entry are not listed.
        assert!(!g.contains_key("NOPE"));
    }
}
