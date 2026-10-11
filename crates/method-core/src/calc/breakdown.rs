//! Stage 4: totals by fringe, group, set and location, and for any selection.

use std::collections::{BTreeMap, BTreeSet};

use crate::calc::{overflow, CalcError, Ctx, FullRows};
use crate::model::{Budget, Detail, RowId};
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

/// One detail with its rows: counted (`active`), unfiltered (`full`) and
/// under the group rules alone (`earned`).
struct Rows<'a> {
    d: &'a Detail,
    active: &'a RowTotals,
    full: &'a RowTotals,
    earned: &'a RowTotals,
}

/// Every detail with its rows. `full` and `earned` hold an account's rows
/// where they differ from the counted ones (see `calc::eval_full` and
/// `calc::eval_earned`); `earned` is `None` when no account's do.
fn rows<'a>(
    b: &'a Budget,
    active: &'a [CategoryTotals],
    full: &'a FullRows,
    earned: Option<&'a FullRows>,
) -> impl Iterator<Item = Rows<'a>> {
    b.categories()
        .zip(active.iter().zip(full))
        .enumerate()
        .flat_map(move |(ci, (cat, (ac, fc)))| {
            cat.accounts
                .iter()
                .zip(ac.accounts.iter().zip(fc))
                .enumerate()
                .flat_map(move |(ai, (acct, (aa, fa)))| {
                    let fa = fa.as_deref().unwrap_or(&aa.rows);
                    let ea = earned
                        .and_then(|e| e[ci][ai].as_deref())
                        .unwrap_or(&aa.rows);
                    acct.details
                        .iter()
                        .zip(aa.rows.iter().zip(fa.iter().zip(ea)))
                        .map(|(d, (a, (f, e)))| Rows {
                            d,
                            active: a,
                            full: f,
                            earned: e,
                        })
                })
        })
}

/// A group's total, as Movie Magic shows it next to the active total
/// (decisions.md, entry 58): every tagged line at its base, suppressed ones
/// included, plus only the fringes the lines actually earn. A suppressed
/// line earns none, so it adds its base alone. `full` is the line's
/// unfiltered row (for the base), `earned` its row under the group rules
/// alone (for the fringes), so the figure ignores the what-if filters.
fn add_group_total(t: &mut Breakdown, full: &RowTotals, earned: &RowTotals) -> Option<()> {
    t.details_total = t.details_total.checked_add(full.total)?;
    t.fringe_total = t.fringe_total.checked_add(earned.fringe_total)?;
    Some(())
}

pub(crate) fn breakdowns(
    ctx: &Ctx,
    active: &[CategoryTotals],
    full: &FullRows,
    earned: Option<&FullRows>,
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

    for r in rows(ctx.b, active, full, earned) {
        let (d, a) = (r.d, r.active);
        if !d.kind.is_costed() {
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
                add_group_total(&mut t.full, r.full, r.earned).ok_or_else(ovf)?;
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
