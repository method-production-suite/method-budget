//! Stage 4: totals by fringe, group, set and location, and for any selection.

use std::collections::{BTreeMap, BTreeSet};

use crate::calc::{overflow, CalcError, Ctx};
use crate::model::{Budget, Detail, RowId, RowKind};
use crate::report::{Breakdown, CategoryTotals, FringeSummary, GroupTotals, Report, RowTotals};

pub(crate) struct Breakdowns {
    pub fringes: BTreeMap<String, FringeSummary>,
    pub groups: BTreeMap<String, GroupTotals>,
    pub sets: BTreeMap<String, Breakdown>,
    pub locations: BTreeMap<String, Breakdown>,
}

fn add(b: &mut Breakdown, r: &RowTotals) -> Option<()> {
    b.details_total = b.details_total.checked_add(r.total)?;
    b.fringe_total = b.fringe_total.checked_add(r.fringe_total)?;
    Some(())
}

/// Every detail with its row in `active` and in `full` (same budget, unfiltered).
fn rows<'a>(
    b: &'a Budget,
    active: &'a [CategoryTotals],
    full: &'a [CategoryTotals],
) -> impl Iterator<Item = (&'a Detail, &'a RowTotals, &'a RowTotals)> {
    b.categories()
        .zip(active.iter().zip(full))
        .flat_map(|(cat, (ac, fc))| {
            cat.accounts
                .iter()
                .zip(ac.accounts.iter().zip(&fc.accounts))
                .flat_map(|(acct, (aa, fa))| {
                    acct.details
                        .iter()
                        .zip(aa.rows.iter().zip(&fa.rows))
                        .map(|(d, (a, f))| (d, a, f))
                })
        })
}

pub(crate) fn breakdowns(
    ctx: &Ctx,
    active: &[CategoryTotals],
    full: &[CategoryTotals],
) -> Result<Breakdowns, CalcError> {
    let ovf = || overflow("breakdown totals");
    let mut fringes: BTreeMap<String, FringeSummary> = ctx
        .b
        .fringes
        .keys()
        .map(|k| (k.clone(), FringeSummary::default()))
        .collect();
    let mut groups: BTreeMap<String, GroupTotals> = ctx
        .b
        .groups
        .keys()
        .map(|k| (k.clone(), GroupTotals::default()))
        .collect();
    let mut sets: BTreeMap<String, Breakdown> = BTreeMap::new();
    let mut locations: BTreeMap<String, Breakdown> = BTreeMap::new();

    for (d, a, f) in rows(ctx.b, active, full) {
        if d.kind == RowKind::Subtotal {
            continue;
        }
        for name in &d.fringes {
            if let Some(s) = fringes.get_mut(name) {
                s.uses += 1;
                s.total = s
                    .total
                    .checked_add(a.fringes.get(name).copied().unwrap_or(0))
                    .ok_or_else(ovf)?;
            }
        }
        for g in &d.groups {
            if let Some(t) = groups.get_mut(g) {
                add(&mut t.full, f).ok_or_else(ovf)?;
                add(&mut t.active, a).ok_or_else(ovf)?;
            }
        }
        if let Some(s) = &d.set {
            add(sets.entry(s.clone()).or_default(), a).ok_or_else(ovf)?;
        }
        if let Some(l) = &d.location {
            add(locations.entry(l.clone()).or_default(), a).ok_or_else(ovf)?;
        }
    }
    Ok(Breakdowns {
        fringes,
        groups,
        sets,
        locations,
    })
}

/// Totals for a set of selected rows, as a side pane would show them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SelectionTotals {
    pub lines: Breakdown,
    pub fringes: BTreeMap<String, i64>,
    pub groups: BTreeMap<String, Breakdown>,
}

/// Sums the rows of `report` whose ids are in `ids`. `report` must come from
/// recalculating `budget`. Returns `None` only on overflow.
pub fn selection_totals(
    budget: &Budget,
    report: &Report,
    ids: &BTreeSet<RowId>,
) -> Option<SelectionTotals> {
    let mut out = SelectionTotals::default();
    let all = budget
        .categories()
        .zip(&report.categories)
        .flat_map(|(cat, ct)| {
            cat.accounts
                .iter()
                .zip(&ct.accounts)
                .flat_map(|(acct, at)| acct.details.iter().zip(&at.rows))
        });
    for (d, r) in all.filter(|(d, _)| ids.contains(&d.id)) {
        add(&mut out.lines, r)?;
        for (name, amt) in &r.fringes {
            let slot = out.fringes.entry(name.clone()).or_insert(0);
            *slot = slot.checked_add(*amt)?;
        }
        for g in &d.groups {
            add(out.groups.entry(g.clone()).or_default(), r)?;
        }
    }
    Some(out)
}
