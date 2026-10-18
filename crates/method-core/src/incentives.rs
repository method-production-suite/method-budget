//! Incentive estimates (roadmap 3.6; decisions.md, entry 80).
//!
//! An incentive program is a [`Credit`]: rows of qualifying groups,
//! categories, accounts or typed amounts, each a percent of its spend with an
//! optional cap, then the program's own cap and a broker's discount. A
//! credit's `locations` limit its spend to lines at those locations. Here
//! every credit is worked out whether it is applied or not, so programs can
//! be set side by side before one is taken off the net total, and spend is
//! shown by location.
//!
//! Every program and rate is the user's: nothing about any real program is
//! built in. Figures are the engine's own (exact integer cents, rounded once
//! per credit as the topsheet rounds them), so an applied program's estimate
//! is what the topsheet takes off.

use std::collections::{BTreeMap, BTreeSet};

use crate::calc::topsheet::{credit_figures, location_spend};
use crate::calc::{overflow, CalcError};
use crate::decimal::Dec;
use crate::model::{Budget, Credit};
use crate::report::{CreditTotals, Report};
use crate::Cents;

/// Spend by location and an estimate for every credit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Incentives {
    /// Spend by the location lines are at: locations in name order (every
    /// location the budget has, and any a line names that it doesn't), then
    /// lines with no location when there are any.
    pub locations: Vec<LocationSpend>,
    /// Every credit, applied or not, in the budget's order.
    pub programs: Vec<Program>,
}

/// What the lines at one location come to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocationSpend {
    /// `None`: lines with no location.
    pub location: Option<String>,
    /// Whether the budget's location list has it (always true for `None`).
    pub known: bool,
    /// The lines' and adjustment rows' totals.
    pub lines: Cents,
    /// The fringes they earn, wherever fringes post.
    pub fringes: Cents,
    pub total: Cents,
}

/// One credit worked out as an estimate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Program {
    /// Its place in the budget's credit list.
    pub index: usize,
    pub name: String,
    /// Whether the topsheet takes it off the net total.
    pub applied: bool,
    /// Where its spend has to be; empty means anywhere.
    pub locations: BTreeSet<String>,
    pub cap: Option<Cents>,
    pub discount_percent: Dec,
    /// The credit's figures, as the topsheet would list it applied. `None`
    /// when it can't be worked out (`problem` says why).
    pub figures: Option<CreditTotals>,
    pub problem: Option<String>,
    /// The rows' net spend added up (typed amounts included).
    pub qualified: Cents,
    /// The net spend of its group, category and account rows by the
    /// location of the lines it came from (`None`: no location), in name
    /// order with `None` last. Typed amounts have no location.
    pub by_location: Vec<(Option<String>, Cents)>,
    /// The row credits added up, unrounded, before the program's cap.
    pub rows_total: Dec,
    /// Whether the program's own cap holds the credit down.
    pub capped: bool,
}

impl Program {
    /// The estimate: the credit after its caps and discount, in cents.
    pub fn amount(&self) -> Option<Cents> {
        self.figures.as_ref().map(|f| f.amount)
    }
}

/// Spend by location and every credit's estimate, on the budget `report`
/// was computed from. `report` must come from recalculating `b`. A credit
/// that can't be worked out says why in its [`Program::problem`]; only a
/// spend too large to add up is an error.
pub fn incentives(b: &Budget, report: &Report) -> Result<Incentives, CalcError> {
    let cats = &report.categories;
    let spend = location_spend(b, cats).ok_or_else(|| overflow("spend by location"))?;
    let mut locations: Vec<LocationSpend> = Vec::new();
    let names: BTreeSet<&String> = b.locations.keys().chain(spend.keys().flatten()).collect();
    for name in names {
        let (lines, fringes, total) = spend.get(&Some(name.clone())).copied().unwrap_or((0, 0, 0));
        locations.push(LocationSpend {
            location: Some(name.clone()),
            known: b.locations.contains_key(name),
            lines,
            fringes,
            total,
        });
    }
    if let Some(&(lines, fringes, total)) = spend.get(&None) {
        locations.push(LocationSpend {
            location: None,
            known: true,
            lines,
            fringes,
            total,
        });
    }

    let programs = b
        .credits
        .iter()
        .enumerate()
        .map(|(index, c)| program(b, report, index, c))
        .collect();
    Ok(Incentives {
        locations,
        programs,
    })
}

fn program(b: &Budget, report: &Report, index: usize, c: &Credit) -> Program {
    let mut p = Program {
        index,
        name: c.name.clone(),
        applied: c.applied,
        locations: c.locations.clone(),
        cap: c.cap,
        discount_percent: c.discount_percent,
        figures: None,
        problem: None,
        qualified: 0,
        by_location: Vec::new(),
        rows_total: Dec::ZERO,
        capped: false,
    };
    match credit_figures(b, &report.categories, c) {
        Ok((figures, by_location)) => {
            let qualified = figures
                .rows
                .iter()
                .try_fold(0, |a: Cents, r| a.checked_add(r.net));
            let rows_total = figures
                .rows
                .iter()
                .try_fold(Dec::ZERO, |a, r| a.checked_add(r.credit));
            match (qualified, rows_total) {
                (Some(q), Some(t)) => {
                    p.qualified = q;
                    p.rows_total = t;
                    p.capped = figures.capped < t;
                    p.by_location = in_order(by_location);
                    p.figures = Some(figures);
                }
                _ => p.problem = Some(format!("credit {}: number too large", c.name)),
            }
        }
        Err(e) => p.problem = Some(e.to_string()),
    }
    p
}

/// Named locations in name order, then `None`.
fn in_order(m: BTreeMap<Option<String>, Cents>) -> Vec<(Option<String>, Cents)> {
    let (none, named): (Vec<_>, Vec<_>) = m.into_iter().partition(|(k, _)| k.is_none());
    named.into_iter().chain(none).collect()
}
