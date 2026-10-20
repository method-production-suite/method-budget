//! The printed per-location incentive report (roadmap 3.6; decisions.md,
//! entry 93): location by location, the spend there and the programs that
//! count it, then every program with its terms, and totals that reconcile
//! with the topsheet's credits.
//!
//! Every figure comes from [`super::incentives`]: nothing is worked out
//! twice. A program's estimate is not split by location (a program is
//! worked out on its whole qualified spend, so a share per location would
//! be a new figure); a location lists what each program counts there.

use std::collections::BTreeSet;

use super::{Incentives, LocationSpend};
use crate::calc::{overflow, CalcError};
use crate::decimal::Dec;
use crate::report::Report;
use crate::Cents;

/// The report's figures.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocationReport {
    /// As [`Incentives::locations`] orders them: named locations, then
    /// lines with no location.
    pub locations: Vec<LocationBlock>,
    /// Every credit, in the budget's order.
    pub programs: Vec<ProgramSummary>,
    /// The locations' spend added up.
    pub spend_total: Cents,
    /// The applied programs' credits: the topsheet's credits total.
    pub applied_total: Cents,
    /// The unapplied programs' estimates added up.
    pub estimates_total: Cents,
    pub grand_total: Cents,
    /// The grand total less the applied credits.
    pub net_total: Cents,
}

/// One location's spend and the programs that count it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocationBlock {
    pub spend: LocationSpend,
    pub programs: Vec<LocationProgram>,
}

/// A program counting a location: its net spend there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocationProgram {
    pub index: usize,
    pub name: String,
    pub applied: bool,
    pub qualified_here: Cents,
}

/// A program and the terms that made its figure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgramSummary {
    pub index: usize,
    pub name: String,
    pub applied: bool,
    /// Where its spend has to be; empty means anywhere.
    pub locations: BTreeSet<String>,
    /// The rows' net spend added up.
    pub qualified: Cents,
    /// Of that, the amounts typed in (no location).
    pub typed: Cents,
    /// The rows' credits, unrounded, before the program's own cap.
    pub rows_total: Dec,
    pub minimum_spend: Option<Cents>,
    pub below_minimum: bool,
    pub cap: Option<Cents>,
    /// Whether its own cap held it down.
    pub capped: bool,
    pub shared_cap: Option<String>,
    /// Its share when the shared cap held it down.
    pub shared: Option<Cents>,
    pub discount_percent: Dec,
    /// What the discount took off, in cents.
    pub discount: Cents,
    /// The broker's fee taken off.
    pub fee: Cents,
    /// The credit (applied) or the estimate; `None` with a problem.
    pub amount: Option<Cents>,
    pub problem: Option<String>,
}

/// The report from `inc`, the estimates worked out with report `r` (both
/// from the same budget).
pub fn location_report(r: &Report, inc: &Incentives) -> Result<LocationReport, CalcError> {
    let ovf = || overflow("incentives by location");
    let locations = inc
        .locations
        .iter()
        .map(|l| LocationBlock {
            spend: l.clone(),
            programs: inc
                .programs
                .iter()
                .filter_map(|p| {
                    let here = p
                        .by_location
                        .iter()
                        .find(|(at, _)| *at == l.location)
                        .map(|(_, v)| *v);
                    let named = l.location.as_ref().is_some_and(|n| p.locations.contains(n));
                    (here.is_some() || named).then(|| LocationProgram {
                        index: p.index,
                        name: p.name.clone(),
                        applied: p.applied,
                        qualified_here: here.unwrap_or(0),
                    })
                })
                .collect(),
        })
        .collect();
    let mut programs = Vec::with_capacity(inc.programs.len());
    for p in &inc.programs {
        let located = sum(p.by_location.iter().map(|(_, v)| *v)).ok_or_else(ovf)?;
        let f = p.figures.as_ref();
        let fee = f.map_or(0, |f| f.fee);
        // Before the discount: its share of a shared cap, else the capped
        // figure rounded; after it, the credit with the fee put back.
        let discount = match f {
            Some(f) => {
                let before = match f.shared {
                    Some(s) => Some(s),
                    None => f.capped.to_cents(),
                };
                before
                    .and_then(|x| x.checked_sub(f.amount.checked_add(f.fee)?))
                    .ok_or_else(ovf)?
            }
            None => 0,
        };
        programs.push(ProgramSummary {
            index: p.index,
            name: p.name.clone(),
            applied: p.applied,
            locations: p.locations.clone(),
            qualified: p.qualified,
            typed: p.qualified.checked_sub(located).ok_or_else(ovf)?,
            rows_total: p.rows_total,
            minimum_spend: p.minimum_spend,
            below_minimum: f.is_some_and(|f| f.below_minimum),
            cap: p.cap,
            capped: p.capped,
            shared_cap: p.shared_cap.clone(),
            shared: f.and_then(|f| f.shared),
            discount_percent: p.discount_percent,
            discount,
            fee,
            amount: p.amount(),
            problem: p.problem.clone(),
        });
    }
    let total = |applied: bool| {
        sum(inc
            .programs
            .iter()
            .filter(|p| p.applied == applied)
            .filter_map(|p| p.amount()))
        .ok_or_else(ovf)
    };
    Ok(LocationReport {
        spend_total: sum(inc.locations.iter().map(|l| l.total)).ok_or_else(ovf)?,
        applied_total: total(true)?,
        estimates_total: total(false)?,
        grand_total: r.grand_total,
        net_total: r.net_total,
        locations,
        programs,
    })
}

fn sum(it: impl Iterator<Item = Cents>) -> Option<Cents> {
    it.into_iter().try_fold(0, |a: Cents, c| a.checked_add(c))
}
