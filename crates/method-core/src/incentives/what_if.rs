//! What if lines were shot at another location (roadmap 3.6; decisions.md,
//! entry 93).
//!
//! A what-if moves the picked lines to a location on a copy of the budget,
//! works the copy out with the same code the topsheet and [`incentives`]
//! use, and sets every program's estimate before and after side by side.
//! The budget itself never changes here: applying a what-if is
//! [`move_lines`] on the real budget, which gives exactly the "after"
//! figures.

use std::collections::{BTreeMap, BTreeSet};

use super::{incentives, Incentives};
use crate::calc::{recalc, CalcError, RecalcOptions};
use crate::model::{Budget, RowId};
use crate::report::Report;
use crate::Cents;

/// Which lines a what-if moves: rows by id, every line of some accounts
/// and categories, maybe only those at one place now, less some rows.
/// Only lines and adjustment rows are picked (their location is what a
/// program counts).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LinePick {
    pub rows: Vec<RowId>,
    /// Account codes.
    pub accounts: Vec<String>,
    /// Category codes.
    pub categories: Vec<String>,
    /// `None`: wherever they are; `Some(None)`: only lines with no
    /// location; `Some(Some(name))`: only lines at `name`.
    pub from: Option<Option<String>>,
    /// Rows left out of what the rest picks.
    pub except: Vec<RowId>,
}

/// Why a what-if can't be worked out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WhatIfError {
    NoRow(RowId),
    NoAccount(String),
    NoCategory(String),
    /// The location to move to isn't in the budget's locations.
    NoLocation(String),
    Calc(CalcError),
}

impl std::fmt::Display for WhatIfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WhatIfError::NoRow(id) => write!(f, "there is no row #{id}"),
            WhatIfError::NoAccount(c) => write!(f, "there is no account {c}"),
            WhatIfError::NoCategory(c) => write!(f, "there is no category {c}"),
            WhatIfError::NoLocation(n) => {
                write!(f, "there is no location named {n} in this budget")
            }
            WhatIfError::Calc(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for WhatIfError {}

impl From<CalcError> for WhatIfError {
    fn from(e: CalcError) -> WhatIfError {
        WhatIfError::Calc(e)
    }
}

/// The lines `pick` names, in budget order, each once.
pub fn pick_lines(b: &Budget, pick: &LinePick) -> Result<Vec<RowId>, WhatIfError> {
    let mut wanted: BTreeSet<RowId> = BTreeSet::new();
    let all: BTreeSet<RowId> = b
        .categories()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.details)
        .map(|d| d.id)
        .collect();
    for &id in &pick.rows {
        if !all.contains(&id) {
            return Err(WhatIfError::NoRow(id));
        }
        wanted.insert(id);
    }
    for code in &pick.accounts {
        let a = b
            .categories()
            .flat_map(|c| &c.accounts)
            .find(|a| a.code == *code)
            .ok_or_else(|| WhatIfError::NoAccount(code.clone()))?;
        wanted.extend(a.details.iter().map(|d| d.id));
    }
    for code in &pick.categories {
        let c = b
            .categories()
            .find(|c| c.code == *code)
            .ok_or_else(|| WhatIfError::NoCategory(code.clone()))?;
        wanted.extend(c.accounts.iter().flat_map(|a| &a.details).map(|d| d.id));
    }
    let except: BTreeSet<RowId> = pick.except.iter().copied().collect();
    Ok(b.categories()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.details)
        .filter(|d| d.kind.is_costed() && wanted.contains(&d.id) && !except.contains(&d.id))
        .filter(|d| match &pick.from {
            None => true,
            Some(place) => d.location == *place,
        })
        .map(|d| d.id)
        .collect())
}

/// Moves rows `rows` to location `to` (`None`: no location), as one change.
/// Gives how many rows' location changed. A location the budget doesn't
/// have, or a row it doesn't have, changes nothing.
pub fn move_lines(b: &mut Budget, rows: &[RowId], to: Option<&str>) -> Result<usize, WhatIfError> {
    if let Some(name) = to {
        if !b.locations.contains_key(name) {
            return Err(WhatIfError::NoLocation(name.to_string()));
        }
    }
    let wanted: BTreeSet<RowId> = rows.iter().copied().collect();
    let found: BTreeSet<RowId> = b
        .categories()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.details)
        .map(|d| d.id)
        .filter(|id| wanted.contains(id))
        .collect();
    if let Some(&missing) = wanted.difference(&found).next() {
        return Err(WhatIfError::NoRow(missing));
    }
    let to = to.map(str::to_string);
    let mut changed = 0;
    for d in b
        .categories_mut()
        .flat_map(|c| c.accounts.iter_mut())
        .flat_map(|a| a.details.iter_mut())
        .filter(|d| wanted.contains(&d.id))
    {
        if d.location != to {
            d.location = to.clone();
            changed += 1;
        }
    }
    Ok(changed)
}

/// The topsheet's figures a what-if compares.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Totals {
    pub grand_total: Cents,
    /// The applied credits taken off.
    pub credits_total: Cents,
    /// The grand total less the applied credits.
    pub net_total: Cents,
    /// The unapplied programs' estimates added up (each as if applied
    /// alongside the applied ones).
    pub estimates_total: Cents,
}

/// One program before and after.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgramChange {
    pub index: usize,
    pub name: String,
    pub applied: bool,
    pub qualified_before: Cents,
    pub qualified_after: Cents,
    /// Its estimate; `None` when it can't be worked out.
    pub before: Option<Cents>,
    pub after: Option<Cents>,
    /// After less before; `None` when either is.
    pub difference: Option<Cents>,
    /// Why it can't be worked out, after the move (or before).
    pub problem: Option<String>,
}

/// The spend at one location before and after (`None`: no location).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocationChange {
    pub location: Option<String>,
    pub before: Cents,
    pub after: Cents,
}

/// What moving lines would do to every program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WhatIf {
    /// The rows moved, as given.
    pub rows: Vec<RowId>,
    /// How many of them would change location.
    pub changed: usize,
    pub to: Option<String>,
    pub before: Incentives,
    pub after: Incentives,
    /// Every credit, in the budget's order.
    pub programs: Vec<ProgramChange>,
    /// Every location either side has, in name order, then no location.
    pub locations: Vec<LocationChange>,
    pub before_totals: Totals,
    pub after_totals: Totals,
}

/// What moving `rows` to `to` would do, worked out with the default
/// options. `b` doesn't change.
pub fn what_if(b: &Budget, rows: &[RowId], to: Option<&str>) -> Result<WhatIf, WhatIfError> {
    let mut moved = b.clone();
    let changed = move_lines(&mut moved, rows, to)?;
    let opts = RecalcOptions::default();
    let before = recalc(b, &opts)?;
    let after = recalc(&moved, &opts)?;
    let mut w = compare((b, &before), (&moved, &after))?;
    w.rows = rows.to_vec();
    w.changed = changed;
    w.to = to.map(str::to_string);
    Ok(w)
}

/// Every program's estimate and the topsheet's totals in `before` and
/// `after` (each a budget and the report recalculated from it), side by
/// side. The caller fills in `rows`, `changed` and `to`.
pub fn compare(before: (&Budget, &Report), after: (&Budget, &Report)) -> Result<WhatIf, CalcError> {
    let inc_before = incentives(before.0, before.1)?;
    let inc_after = incentives(after.0, after.1)?;
    let programs = inc_before
        .programs
        .iter()
        .zip(&inc_after.programs)
        .map(|(a, z)| {
            let (pa, pz) = (a.amount(), z.amount());
            ProgramChange {
                index: a.index,
                name: a.name.clone(),
                applied: a.applied,
                qualified_before: a.qualified,
                qualified_after: z.qualified,
                before: pa,
                after: pz,
                difference: pa.zip(pz).and_then(|(x, y)| y.checked_sub(x)),
                problem: z.problem.clone().or_else(|| a.problem.clone()),
            }
        })
        .collect();
    let mut spend: BTreeMap<Option<String>, (Cents, Cents)> = BTreeMap::new();
    for l in &inc_before.locations {
        spend.entry(l.location.clone()).or_default().0 = l.total;
    }
    for l in &inc_after.locations {
        spend.entry(l.location.clone()).or_default().1 = l.total;
    }
    let (none, named): (Vec<_>, Vec<_>) = spend.into_iter().partition(|(k, _)| k.is_none());
    let locations = named
        .into_iter()
        .chain(none)
        .map(|(location, (before, after))| LocationChange {
            location,
            before,
            after,
        })
        .collect();
    Ok(WhatIf {
        rows: Vec::new(),
        changed: 0,
        to: None,
        before_totals: totals(before.1, &inc_before)?,
        after_totals: totals(after.1, &inc_after)?,
        before: inc_before,
        after: inc_after,
        programs,
        locations,
    })
}

fn totals(r: &Report, inc: &Incentives) -> Result<Totals, CalcError> {
    let estimates_total = inc
        .programs
        .iter()
        .filter(|p| !p.applied)
        .filter_map(|p| p.amount())
        .try_fold(0, |a: Cents, c| a.checked_add(c))
        .ok_or_else(|| crate::calc::overflow("estimates"))?;
    Ok(Totals {
        grand_total: r.grand_total,
        credits_total: r.credits_total,
        net_total: r.net_total,
        estimates_total,
    })
}
