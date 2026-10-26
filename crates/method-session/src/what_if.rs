//! What if lines were shot at another location (roadmap 3.6; decisions.md,
//! entry 93): the Credits tab's What if… panel.
//!
//! [`what_if`] changes nothing: it moves the picked lines on a copy of the
//! budget, works the copy out the way the session works out the budget
//! itself (a budget with problems from its cleaned copy), and sets every
//! program before and after side by side with
//! [`method_core::incentives::what_if::compare`]. [`apply_what_if`] makes
//! the same move on the budget as one undo step, so the figures it gives
//! are the what-if's "after".

use method_core::incentives::what_if::{compare, move_lines, pick_lines, LinePick, Totals, WhatIf};
use method_core::{Budget, Cents, RowId};
use serde::{Deserialize, Serialize};

use crate::diagnose::lenient_recalc;
use crate::edit::EditResult;
use crate::session::Session;

/// Which lines to move and where (`to`: a location the budget has, or
/// `null` for no location).
#[derive(Clone, Debug, Default, Deserialize)]
pub struct WhatIfInput {
    #[serde(default)]
    pub row_ids: Vec<RowId>,
    /// Account codes: every line in them.
    #[serde(default)]
    pub accounts: Vec<String>,
    /// Category codes: every line in them.
    #[serde(default)]
    pub categories: Vec<String>,
    /// Only the lines now at a place: left out for wherever they are,
    /// `{"location": null}` for lines with no location.
    #[serde(default)]
    pub from: Option<FromPlace>,
    /// Rows left out of what the rest picks.
    #[serde(default)]
    pub except: Vec<RowId>,
    pub to: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct FromPlace {
    pub location: Option<String>,
}

impl WhatIfInput {
    fn pick(&self) -> LinePick {
        LinePick {
            rows: self.row_ids.clone(),
            accounts: trimmed(&self.accounts),
            categories: trimmed(&self.categories),
            from: self.from.as_ref().map(|f| f.location.clone()),
            except: self.except.clone(),
        }
    }

    fn to(&self) -> Option<&str> {
        self.to.as_deref()
    }
}

fn trimmed(codes: &[String]) -> Vec<String> {
    codes.iter().map(|c| c.trim().to_string()).collect()
}

/// A what-if's figures (money in integer cents).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct WhatIfDto {
    /// From a cleaned copy of a budget with problems: figures may be short.
    pub partial: bool,
    /// The lines it moves, in budget order.
    pub lines: Vec<WhatIfLineDto>,
    /// How many of them would change location.
    pub changed: usize,
    pub to: Option<String>,
    /// Every credit, in the budget's order.
    pub programs: Vec<ProgramChangeDto>,
    pub locations: Vec<LocationChangeDto>,
    pub before: TotalsDto,
    pub after: TotalsDto,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct WhatIfLineDto {
    pub id: RowId,
    pub account: String,
    /// Its place in the account, from 1.
    pub line: usize,
    pub description: String,
    /// Where it is now (`null`: no location).
    pub location: Option<String>,
    /// The line's total and the fringes it earns.
    pub total: Cents,
    pub fringes: Cents,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ProgramChangeDto {
    pub index: usize,
    pub name: String,
    pub applied: bool,
    pub qualified_before: Cents,
    pub qualified_after: Cents,
    pub before: Option<Cents>,
    pub after: Option<Cents>,
    pub difference: Option<Cents>,
    pub problem: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LocationChangeDto {
    pub location: Option<String>,
    pub before: Cents,
    pub after: Cents,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct TotalsDto {
    pub grand_total: Cents,
    /// The applied credits taken off.
    pub credits_total: Cents,
    /// The grand total less the applied credits.
    pub net_total: Cents,
    /// The unapplied programs' estimates added up.
    pub estimates_total: Cents,
}

impl From<&Totals> for TotalsDto {
    fn from(t: &Totals) -> TotalsDto {
        TotalsDto {
            grand_total: t.grand_total,
            credits_total: t.credits_total,
            net_total: t.net_total,
            estimates_total: t.estimates_total,
        }
    }
}

/// What moving the lines `input` picks would do to every program; nothing
/// changes. Figured as the session figures the budget.
pub fn what_if(s: &mut Session, input: &WhatIfInput) -> Result<WhatIfDto, String> {
    let to = input.to();
    let (budget, outcome) = s.totals();
    let rows = pick_lines(budget, &input.pick()).map_err(|e| e.to_string())?;
    let mut moved = budget.clone();
    let changed = move_lines(&mut moved, &rows, to).map_err(|e| e.to_string())?;
    let no_totals = "the totals can't be worked out until the budget's problems are fixed";
    let before_r = outcome.report.as_ref().ok_or(no_totals)?;
    let before_b = outcome.sanitized.as_ref().unwrap_or(budget);
    let after = lenient_recalc(&moved);
    let after_r = after.report.as_ref().ok_or(no_totals)?;
    let after_b = after.sanitized.as_ref().unwrap_or(&moved);
    let mut w = compare((before_b, before_r), (after_b, after_r)).map_err(|e| e.to_string())?;
    w.rows = rows;
    w.changed = changed;
    w.to = to.map(str::to_string);
    let partial = outcome.sanitized.is_some() || after.sanitized.is_some();
    Ok(dto(before_b, before_r, &w, partial))
}

/// A what-if as JSON, its lines' totals from `r` (recalculated from `b`,
/// the budget before the move).
pub fn dto(b: &Budget, r: &method_core::Report, w: &WhatIf, partial: bool) -> WhatIfDto {
    WhatIfDto {
        partial,
        lines: lines(b, r, &w.rows),
        changed: w.changed,
        to: w.to.clone(),
        programs: w
            .programs
            .iter()
            .map(|p| ProgramChangeDto {
                index: p.index,
                name: p.name.clone(),
                applied: p.applied,
                qualified_before: p.qualified_before,
                qualified_after: p.qualified_after,
                before: p.before,
                after: p.after,
                difference: p.difference,
                problem: p.problem.clone(),
            })
            .collect(),
        locations: w
            .locations
            .iter()
            .map(|l| LocationChangeDto {
                location: l.location.clone(),
                before: l.before,
                after: l.after,
            })
            .collect(),
        before: (&w.before_totals).into(),
        after: (&w.after_totals).into(),
    }
}

/// The lines `rows` names, with their totals from `r` (recalculated from
/// `b`).
fn lines(b: &Budget, r: &method_core::Report, rows: &[RowId]) -> Vec<WhatIfLineDto> {
    let wanted: std::collections::BTreeSet<RowId> = rows.iter().copied().collect();
    let mut out = Vec::new();
    for (c, ct) in b.categories().zip(&r.categories) {
        for (a, at) in c.accounts.iter().zip(&ct.accounts) {
            for (n, d) in a.details.iter().enumerate() {
                if !wanted.contains(&d.id) {
                    continue;
                }
                let t = at.rows.iter().find(|t| t.id == d.id);
                out.push(WhatIfLineDto {
                    id: d.id,
                    account: a.code.clone(),
                    line: n + 1,
                    description: d.description.clone(),
                    location: d.location.clone(),
                    total: t.map_or(0, |t| t.total),
                    fringes: t.map_or(0, |t| t.fringe_total),
                });
            }
        }
    }
    out
}

/// Moves the lines `input` picks to its location, as one undo step: the
/// what-if applied. Gives how many lines moved.
pub fn apply_what_if(s: &mut Session, input: &WhatIfInput) -> EditResult<usize> {
    let rows = pick_lines(s.budget(), &input.pick()).map_err(|e| e.to_string())?;
    let to = input.to();
    let place = to.map_or("no location".to_string(), str::to_string);
    if rows.is_empty() {
        return Err("No lines to move: pick the lines first.".into());
    }
    let mut check = s.budget().clone();
    let changed = move_lines(&mut check, &rows, to).map_err(|e| e.to_string())?;
    if changed == 0 {
        return Err(format!("The lines are already at {place}."));
    }
    let moved = crate::history::rows_phrase(s.budget(), &rows);
    let label = match to {
        Some(to) => crate::messages::msg(
            "engine.undo.move_rows_to",
            &[("rows", &moved), ("place", &to)],
        ),
        None => crate::messages::msg("engine.undo.move_rows_to_no_location", &[("rows", &moved)]),
    };
    s.edit(label, |b| {
        move_lines(b, &rows, to).map_err(|e| e.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use method_core::*;

    /// One account: a 1,000.00 line at Ridge, a 500.00 line at Coast; a
    /// program at Ridge (applied, 25%) and one at Coast (20%, not applied).
    fn session() -> Session {
        let mut b = Budget::new("What if", "USD");
        for n in ["Ridge", "Coast"] {
            b.locations.insert(n.into(), Location::default());
        }
        let mut one = Detail::line(1, "Crew", "1", "", "1000");
        one.location = Some("Ridge".into());
        let mut two = Detail::line(2, "Van", "1", "", "500");
        two.location = Some("Coast".into());
        b.topsheet.push(TopsheetRow::Category(Category {
            code: "21".into(),
            name: "Crew".into(),
            notes: Vec::new(),
            accounts: vec![Account {
                code: "2101".into(),
                name: "Crew".into(),
                notes: Vec::new(),
                details: vec![one, two],
            }],
        }));
        for (name, at, pct, applied) in [
            ("Ridge", "Ridge", "25", true),
            ("Coast", "Coast", "20", false),
        ] {
            b.credits.push(Credit {
                name: name.into(),
                account: None,
                applied,
                cap: None,
                discount_percent: Dec::ZERO,
                excluded_fringes: Default::default(),
                locations: [at.to_string()].into_iter().collect(),
                minimum_spend: None,
                broker_fee: 0,
                shared_cap: None,
                rows: vec![CreditRow {
                    description: String::new(),
                    source: CreditSource::Category {
                        category: "21".into(),
                    },
                    percent: pct.parse().unwrap(),
                    cap: CreditCap::None,
                }],
            });
        }
        Session::with_budget(b, None)
    }

    fn crew_to_coast() -> WhatIfInput {
        WhatIfInput {
            accounts: vec!["2101".into()],
            from: Some(FromPlace {
                location: Some("Ridge".into()),
            }),
            to: Some("Coast".into()),
            ..WhatIfInput::default()
        }
    }

    #[test]
    fn a_what_if_changes_nothing_and_applying_it_gives_its_figures() {
        let mut s = session();
        let before = s.budget().clone();
        let w = what_if(&mut s, &crew_to_coast()).unwrap();
        assert_eq!(s.budget(), &before);
        assert!(!s.can_undo());
        assert_eq!(w.lines.len(), 1);
        assert_eq!(w.lines[0].description, "Crew");
        assert_eq!(w.lines[0].total, 100_000);
        assert_eq!(w.changed, 1);
        let p: Vec<_> = w
            .programs
            .iter()
            .map(|p| (p.before, p.after, p.difference))
            .collect();
        assert_eq!(
            p,
            vec![
                (Some(25_000), Some(0), Some(-25_000)),
                (Some(10_000), Some(30_000), Some(20_000)),
            ]
        );
        assert_eq!(w.before.net_total, 125_000);
        assert_eq!(w.after.net_total, 150_000);

        // Applying it is one step and gives the "after" figures.
        assert_eq!(apply_what_if(&mut s, &crew_to_coast()), Ok(1));
        let now = crate::commands::incentives(&mut s).unwrap();
        let amounts: Vec<_> = now.programs.iter().map(|p| p.amount).collect();
        assert_eq!(amounts, vec![Some(0), Some(30_000)]);
        let (_, outcome) = s.totals();
        assert_eq!(
            outcome.report.as_ref().unwrap().net_total,
            w.after.net_total
        );
        s.undo();
        assert_eq!(s.budget(), &before);
        assert!(!s.can_undo(), "one undo step");
    }

    #[test]
    fn nothing_to_move_is_refused() {
        let mut s = session();
        let mut none = crew_to_coast();
        none.from = Some(FromPlace { location: None });
        assert!(apply_what_if(&mut s, &none)
            .unwrap_err()
            .contains("No lines"));
        let mut there = crew_to_coast();
        there.from = Some(FromPlace {
            location: Some("Coast".into()),
        });
        assert_eq!(
            apply_what_if(&mut s, &there),
            Err("The lines are already at Coast.".to_string())
        );
        let mut nowhere = crew_to_coast();
        nowhere.to = Some("Desert".into());
        assert!(what_if(&mut s, &nowhere).unwrap_err().contains("Desert"));
        assert!(apply_what_if(&mut s, &nowhere).is_err());
        assert!(!s.can_undo());
        // An empty what-if shows every program unchanged.
        let w = what_if(&mut s, &none).unwrap();
        assert!(w.lines.is_empty());
        assert!(w.programs.iter().all(|p| p.difference == Some(0)));
    }
}
