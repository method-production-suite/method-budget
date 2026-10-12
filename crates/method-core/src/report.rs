//! What recalc produces: every total in the budget, plus breakdowns.

use std::collections::BTreeMap;

use crate::decimal::{Dec, Exact};
use crate::model::{Budget, FringePosting, GroupRules, Inclusion, RowId, RowKind, TopsheetRow};
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
    /// The topsheet's figures in `totals_currency`, converted row by row
    /// (decisions.md, entry 60); `None` when totals show in the base
    /// currency (or a figure doesn't fit).
    pub display: Option<DisplayTotals>,
}

impl Report {
    /// Converts one base-currency amount for display in `totals_currency`,
    /// rounded once, exactly. `None` on overflow or a zero rate. The
    /// topsheet's totals in that currency are [`Report::display`], which
    /// converts each line, fringe, charge and credit before adding them up.
    pub fn in_totals_currency(&self, c: Cents) -> Option<Cents> {
        Exact::cents(c).div_to_cents(self.totals_rate)
    }
}

/// One row of the topsheet, in the order [`Report::topsheet_lines`] lists
/// them (decisions.md, entries 9 and 60). Indexes are into the report's
/// lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TopsheetLine {
    /// Category `n` of [`Report::categories`].
    Category(usize),
    /// The fringe row of level `n` of [`Report::levels`] (fringes posted by
    /// production level), just above its production total.
    LevelFringes(usize),
    /// The production total closing level `n` of [`Report::levels`].
    Level(usize),
    /// Total above-the-line.
    Atl,
    /// Total below-the-line.
    Btl,
    /// Total above and below-the-line.
    AtlBtl,
    /// The budget's one fringe row (fringes posted by budget), where
    /// [`crate::FringeRow::position`] puts it.
    BudgetFringes,
    /// Applied charge `n` of [`Report::charges`].
    Charge(usize),
    Grand,
    /// Applied credit `n` of [`Report::credits`].
    Credit(usize),
    /// The applied credits added up, when there are two or more.
    CreditsSubtotal,
    Net,
}

impl Report {
    /// Every row of the topsheet `b` (this report's budget) shows, in order,
    /// as the reference product lists them (decisions.md, entry 60): the
    /// categories and production totals, a level's fringe row above its
    /// production total; the summary rows; the budget's fringe row (in its
    /// default place, or above the topsheet row its position names); the
    /// applied charges newest first, which is the reverse of the budget's
    /// list; the grand total; the applied credits in reverse application
    /// order; their subtotal, with two or more; and the net total. Which of
    /// the summary rows and the net total to show is the caller's choice
    /// (display settings): they are all listed. The budget's fringe row is
    /// listed when fringes post by budget and come to something.
    pub fn topsheet_lines(&self, b: &Budget) -> Vec<TopsheetLine> {
        let fringe_row = b.fringe_posting == FringePosting::Budget && self.budget_fringes != 0;
        let above = if fringe_row {
            b.fringe_row.above(b.topsheet.len())
        } else {
            None
        };
        let mut out = Vec::new();
        let (mut ci, mut li) = (0, 0);
        for (ti, row) in b.topsheet.iter().enumerate() {
            if above == Some(ti) {
                out.push(TopsheetLine::BudgetFringes);
            }
            match row {
                TopsheetRow::Category(_) => {
                    if ci < self.categories.len() {
                        out.push(TopsheetLine::Category(ci));
                    }
                    ci += 1;
                }
                TopsheetRow::ProductionTotal(_) => {
                    if let Some(l) = self.levels.get(li) {
                        if l.posted_fringes != 0 {
                            out.push(TopsheetLine::LevelFringes(li));
                        }
                        out.push(TopsheetLine::Level(li));
                    }
                    li += 1;
                }
            }
        }
        // The unnamed level after the last production total keeps its
        // fringe row (`trailing_level_fringes` keep).
        if let Some(l) = self.levels.get(li) {
            if l.posted_fringes != 0 {
                out.push(TopsheetLine::LevelFringes(li));
            }
        }
        if above == Some(b.topsheet.len()) {
            out.push(TopsheetLine::BudgetFringes);
        }
        out.extend([TopsheetLine::Atl, TopsheetLine::Btl, TopsheetLine::AtlBtl]);
        if fringe_row && above.is_none() {
            out.push(TopsheetLine::BudgetFringes);
        }
        out.extend((0..self.charges.len()).rev().map(TopsheetLine::Charge));
        out.push(TopsheetLine::Grand);
        out.extend((0..self.credits.len()).rev().map(TopsheetLine::Credit));
        if self.credits.len() >= 2 {
            out.push(TopsheetLine::CreditsSubtotal);
        }
        out.push(TopsheetLine::Net);
        out
    }
}

/// The topsheet in the totals currency (decisions.md, entry 60), as the
/// reference product shows it: each line's base total converted and
/// rounded, an adjustment row's percent taken of the converted rows above
/// it, each fringe of a fringe row converted on its own, each charge and
/// credit converted on its own; everything above that adds those cents up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayTotals {
    /// One per category, in `Report::categories` order (with its fringe
    /// account or account fringe rows when fringes post there).
    pub categories: Vec<Cents>,
    /// One per level, in `Report::levels` order (with its fringe row when
    /// fringes post by production level).
    pub levels: Vec<Cents>,
    pub budget_fringes: Cents,
    pub total_atl: Cents,
    pub total_btl: Cents,
    pub atl_btl: Cents,
    /// One per applied charge, in `Report::charges` order.
    pub charges: Vec<Cents>,
    pub grand_total: Cents,
    /// One per applied credit, in `Report::credits` order.
    pub credits: Vec<Cents>,
    pub credits_total: Cents,
    pub net_total: Cents,
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
