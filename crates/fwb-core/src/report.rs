//! What recalc produces: every total in the budget, plus breakdowns.

use std::collections::BTreeMap;

use crate::decimal::Dec;
use crate::model::{RowId, RowKind};
use crate::Cents;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    pub base_currency: String,
    pub globals: BTreeMap<String, Dec>,
    /// Categories in topsheet order (production-total rows are in `levels`).
    pub categories: Vec<CategoryTotals>,
    /// One per production-total row, in order, plus a trailing unnamed level if
    /// categories follow the last production total.
    pub levels: Vec<LevelTotals>,
    /// Fringes posted as one budget-wide row (`FringePosting::Budget`), else 0.
    pub budget_fringes: Cents,
    /// The first level's total, or 0 when there are no production totals.
    pub total_atl: Cents,
    /// Every other level's total.
    pub total_btl: Cents,
    pub atl_btl: Cents,
    pub charges: Vec<ChargeTotals>,
    /// ATL + BTL + budget fringes + charges.
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LevelTotals {
    /// Empty for the trailing unnamed level.
    pub name: String,
    /// Indexes into `Report::categories`: `start..end`.
    pub start: usize,
    pub end: usize,
    /// Fringes posted at this level (`FringePosting::ProductionLevel`), else 0.
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
    /// Its money fields are then 0. Subtotal rows are always included.
    pub included: bool,
    /// amount x X x 4X.
    pub quantity: Dec,
    pub currency: String,
    /// In `currency`.
    pub local_total: Cents,
    /// In the base currency. Always 0 for subtotal rows.
    pub total: Cents,
    /// Subtotal rows only: the running total they display.
    pub subtotal: Option<Cents>,
    /// Fringe dollars this row earned, by fringe name (range fringes allocated).
    pub fringes: BTreeMap<String, Cents>,
    pub fringe_total: Cents,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChargeTotals {
    pub name: String,
    /// What the percentage was applied to (for flat fees, the fee before exclusions).
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
    /// Every tagged line, as if nothing were suppressed or filtered.
    pub full: Breakdown,
    /// Only tagged lines that count toward the budget.
    pub active: Breakdown,
}
