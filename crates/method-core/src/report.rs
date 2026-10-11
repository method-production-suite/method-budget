//! What recalc produces: every total in the budget, plus breakdowns.

use std::collections::BTreeMap;

use crate::decimal::{Dec, Exact};
use crate::model::{GroupRules, Inclusion, RowId, RowKind};
use crate::Cents;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    pub base_currency: String,
    /// Currency for displaying totals; the base currency unless set.
    pub totals_currency: String,
    /// Base-currency units per one unit of `totals_currency`.
    pub totals_rate: Dec,
    pub globals: BTreeMap<String, Dec>,
    /// Categories in topsheet order (production-total rows are in `levels`).
    pub categories: Vec<CategoryTotals>,
    /// One per production-total row, in order, plus a trailing unnamed level if
    /// categories follow the last production total.
    pub levels: Vec<LevelTotals>,
    /// Fringes posted as one budget-wide row (`FringePosting::Budget`), else 0.
    pub budget_fringes: Cents,
    /// The first level's total when a production total closes it, else 0
    /// (decisions.md, entry 56).
    pub total_atl: Cents,
    /// The total of every later level a production total closes. The
    /// trailing unnamed level is in neither ATL nor BTL.
    pub total_btl: Cents,
    pub atl_btl: Cents,
    pub charges: Vec<ChargeTotals>,
    /// Every level (the trailing one too) + budget fringes + charges.
    pub grand_total: Cents,
    pub credits: Vec<CreditTotals>,
    pub credits_total: Cents,
    /// Grand total minus credits.
    pub net_total: Cents,
    /// Budget-wide figures per fringe name.
    pub fringes: BTreeMap<String, FringeSummary>,
    pub groups: BTreeMap<String, GroupTotals>,
    pub sets: BTreeMap<String, Breakdown>,
    pub locations: BTreeMap<String, Breakdown>,
}

impl Report {
    /// Converts a base-currency amount for display in `totals_currency`,
    /// rounded once. `None` on overflow or a zero rate.
    pub fn in_totals_currency(&self, c: Cents) -> Option<Cents> {
        Exact::cents(c).div_to_cents(self.totals_rate)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LevelTotals {
    /// Empty for the trailing unnamed level.
    pub name: String,
    /// Indexes into `Report::categories`: `start..end`.
    pub start: usize,
    pub end: usize,
    /// Fringes posted at this level (`FringePosting::ProductionLevel`), else 0
    /// (also 0 for the trailing level when `trailing_level_fringes` is
    /// `drop`).
    pub posted_fringes: Cents,
    /// Sum of its categories' totals plus `posted_fringes`.
    pub total: Cents,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CategoryTotals {
    pub code: String,
    pub name: String,
    pub accounts: Vec<AccountTotals>,
    /// Sum of accounts' `details_total`.
    pub details_total: Cents,
    /// Sum of accounts' `fringe_total` (all fringes earned in this category).
    pub fringe_total: Cents,
    /// Fringe account posted inside this category (`FringePosting::Category`):
    /// its number, and amount. `None` under other posting modes.
    pub fringe_account: Option<(String, Cents)>,
    /// Sum of accounts' `total`, plus the fringe account amount when present.
    pub total: Cents,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountTotals {
    pub code: String,
    pub name: String,
    pub rows: Vec<RowTotals>,
    /// Sum of rows' `total`.
    pub details_total: Cents,
    /// Sum of rows' `fringe_total`.
    pub fringe_total: Cents,
    /// `details_total`, plus `fringe_total` under `FringePosting::Account`.
    pub total: Cents,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowTotals {
    pub id: RowId,
    pub kind: RowKind,
    /// False when the row is suppressed by a group or filtered out by options.
    /// Its money fields are then 0. Subtotal and separator rows are always included.
    pub included: bool,
    /// amount x X x 4X.
    pub quantity: Dec,
    pub currency: String,
    /// In `currency`.
    pub local_total: Cents,
    /// In the base currency. Always 0 for subtotal and separator rows.
    pub total: Cents,
    /// Subtotal rows only: the running total they display.
    pub subtotal: Option<Cents>,
    /// Fringe dollars this row earned, by fringe name (lines and adjustment
    /// rows; a range uses its cutoff up in row order).
    pub fringes: BTreeMap<String, Cents>,
    pub fringe_total: Cents,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChargeTotals {
    pub name: String,
    /// What the percentage was applied to: every level plus budget fringes,
    /// the grand total before charges (for flat fees, the fee before
    /// exclusions).
    pub base: Cents,
    pub excluded: Cents,
    pub amount: Cents,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreditTotals {
    pub name: String,
    pub rows: Vec<CreditRowTotals>,
    /// Sum of row credits, unrounded, after the overall cap.
    pub capped: Dec,
    /// Final credit after the discount, rounded once.
    pub amount: Cents,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreditRowTotals {
    pub gross: Cents,
    pub excluded_fringes: Cents,
    pub net: Cents,
    /// Unrounded; rows are summed before rounding.
    pub credit: Dec,
    /// How far the row is under its cap (positive) or over it (negative),
    /// unrounded: the cap less the net spend for a cap on the spend, the cap
    /// less the uncapped credit for a cap on the credit. `None` with no cap.
    pub cap_variance: Option<Dec>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FringeSummary {
    /// Lines the fringe is applied to (included or not).
    pub uses: u32,
    /// Dollars, over included lines.
    pub total: Cents,
}

/// Totals over a set of lines.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Breakdown {
    pub details_total: Cents,
    pub fringe_total: Cents,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GroupTotals {
    /// The group's total, as Movie Magic shows it (decisions.md, entry 58):
    /// every tagged line at its base, suppressed ones included, and only the
    /// fringes the lines earn (a suppressed line earns none). The what-if
    /// filters don't change it.
    pub full: Breakdown,
    /// Only tagged lines that count toward the budget.
    pub active: Breakdown,
}

impl Breakdown {
    /// Lines and fringes as a group's dollars show them: without the fringes
    /// when the group rule `fringes_in_totals` is Exclude (display only;
    /// decisions.md, entry 57). `None` on overflow.
    pub fn group_shown(&self, rules: &GroupRules) -> Option<Cents> {
        match rules.fringes_in_totals {
            Inclusion::Include => self.details_total.checked_add(self.fringe_total),
            Inclusion::Exclude => Some(self.details_total),
        }
    }
}

impl GroupTotals {
    /// The group's total and active total as group lists, setup tables and
    /// reports show them, by the group rules (see [`Breakdown::group_shown`]).
    /// `None` on overflow.
    pub fn shown(&self, rules: &GroupRules) -> Option<(Cents, Cents)> {
        Some((
            self.full.group_shown(rules)?,
            self.active.group_shown(rules)?,
        ))
    }
}
