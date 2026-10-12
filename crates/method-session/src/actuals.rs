//! The cost figures every view of a budget with actuals carries (roadmap
//! 3.7; the engine is `method_core::actuals`): every account, category,
//! level and summary row, and the lines of the account on screen. The only
//! pass over every transaction (the rollup) is cached by the session until
//! the actuals or the account pattern change (`Session::cost`), so an edit
//! to a line costs one pass over the accounts, not over the transactions.
//! Importing and listing actuals is the desktop app's (method-app's
//! src/actuals.rs).

use std::collections::BTreeMap;
use std::sync::Arc;

use method_core::actuals::{self, CostAccount, CostFigures, CostReport, EtcMethod, Rollup, Sum};
use method_core::{Budget, Cents, Report, RowId};
use serde::Serialize;

// ---------------------------------------------------------------------------
// The cost figures every view carries

/// One row's cost figures, as the page draws them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FiguresDto {
    pub budget: Cents,
    pub baseline: Option<Cents>,
    pub actual: Cents,
    pub count: u64,
    pub etc: Cents,
    pub eac: Cents,
    pub variance: Cents,
    /// Percent spent with two decimals (`"12.5"`), or `null` for a zero budget.
    pub percent: Option<String>,
    /// Over budget (variance below zero).
    pub over: bool,
}

impl From<&CostFigures> for FiguresDto {
    fn from(f: &CostFigures) -> FiguresDto {
        FiguresDto {
            budget: f.budget,
            baseline: f.baseline,
            actual: f.actual,
            count: f.count,
            etc: f.etc,
            eac: f.eac,
            variance: f.variance,
            percent: f.percent_spent.map(|p| p.to_string()),
            over: f.is_over(),
        }
    }
}

/// An estimate-to-complete method as the page reads it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MethodDto {
    /// `manual` or `percent_complete` (budget minus actual has none).
    pub kind: &'static str,
    /// The manual amount, in cents.
    pub amount: Option<Cents>,
    /// The percent complete (`"40"`).
    pub percent: Option<String>,
}

fn method_dto(m: Option<EtcMethod>) -> Option<MethodDto> {
    match m? {
        EtcMethod::BudgetMinusActual => None,
        EtcMethod::Manual(c) => Some(MethodDto {
            kind: "manual",
            amount: Some(c),
            percent: None,
        }),
        EtcMethod::PercentComplete(p) => Some(MethodDto {
            kind: "percent_complete",
            amount: None,
            percent: Some(p.to_string()),
        }),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CostAccountDto {
    pub code: String,
    pub figures: FiguresDto,
    pub method: Option<MethodDto>,
}

impl From<&CostAccount> for CostAccountDto {
    fn from(a: &CostAccount) -> CostAccountDto {
        CostAccountDto {
            code: a.code.clone(),
            figures: FiguresDto::from(&a.figures),
            method: method_dto(a.method),
        }
    }
}

/// A category: its accounts in the chart's order (as the report lists them).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CostCategoryDto {
    pub code: String,
    pub name: String,
    pub figures: FiguresDto,
    pub accounts: Vec<CostAccountDto>,
    pub fringe_account: Option<CostAccountDto>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CostLevelDto {
    pub name: String,
    pub start: usize,
    pub end: usize,
    pub figures: FiguresDto,
    pub posted_fringes: Option<FiguresDto>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CostItemDto {
    pub name: String,
    pub figures: FiguresDto,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SumDto {
    pub code: String,
    pub amount: Cents,
    pub count: u64,
}

/// The cost report for the page: in step with the view's `report`
/// (categories and levels by position).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CostDto {
    /// Transactions in the budget, and imports.
    pub transactions: usize,
    pub batches: usize,
    pub categories: Vec<CostCategoryDto>,
    pub levels: Vec<CostLevelDto>,
    pub total_atl: FiguresDto,
    pub total_btl: FiguresDto,
    pub atl_btl: FiguresDto,
    pub budget_fringes: Option<CostItemDto>,
    pub charges: Vec<CostItemDto>,
    pub unassigned: FiguresDto,
    /// The numbers unassigned transactions carry (`""`: none).
    pub unassigned_codes: Vec<SumDto>,
    pub total: FiguresDto,
    /// Transactions that name a detail row, by row id: the lean view's
    /// account on screen only, every row in the full view.
    pub lines: BTreeMap<RowId, SumDto>,
    /// Problems working it out (a sum too large), if any.
    pub error: Option<String>,
}

/// The cost figures of `b` (whose report is `r`) with its actuals rolled up
/// as `roll` (`None`: the sums overflowed); `lines` keeps the line sums
/// whose rows it lets through.
pub fn cost_dto(
    b: &Budget,
    r: &Report,
    roll: Option<&Rollup>,
    lines: impl Fn(RowId) -> bool,
) -> CostDto {
    match roll.and_then(|roll| actuals::cost_report(b, r, roll)) {
        Some(c) => cost_dto_of(b, &c, lines),
        None => CostDto {
            transactions: b.actuals.transactions.len(),
            batches: b.actuals.batches.len(),
            categories: Vec::new(),
            levels: Vec::new(),
            total_atl: FiguresDto::from(&CostFigures::default()),
            total_btl: FiguresDto::from(&CostFigures::default()),
            atl_btl: FiguresDto::from(&CostFigures::default()),
            budget_fringes: None,
            charges: Vec::new(),
            unassigned: FiguresDto::from(&CostFigures::default()),
            unassigned_codes: Vec::new(),
            total: FiguresDto::from(&CostFigures::default()),
            lines: BTreeMap::new(),
            error: Some("The actuals add up to more than the app can hold.".into()),
        },
    }
}

fn cost_dto_of(b: &Budget, c: &CostReport, lines: impl Fn(RowId) -> bool) -> CostDto {
    let sum = |code: &str, s: &Sum| SumDto {
        code: code.to_string(),
        amount: s.amount,
        count: s.count,
    };
    CostDto {
        transactions: b.actuals.transactions.len(),
        batches: b.actuals.batches.len(),
        categories: c
            .categories
            .iter()
            .map(|cat| CostCategoryDto {
                code: cat.code.clone(),
                name: cat.name.clone(),
                figures: FiguresDto::from(&cat.figures),
                accounts: cat.accounts.iter().map(CostAccountDto::from).collect(),
                fringe_account: cat.fringe_account.as_ref().map(CostAccountDto::from),
            })
            .collect(),
        levels: c
            .levels
            .iter()
            .map(|l| CostLevelDto {
                name: l.name.clone(),
                start: l.start,
                end: l.end,
                figures: FiguresDto::from(&l.figures),
                posted_fringes: l.posted_fringes.as_ref().map(FiguresDto::from),
            })
            .collect(),
        total_atl: FiguresDto::from(&c.total_atl),
        total_btl: FiguresDto::from(&c.total_btl),
        atl_btl: FiguresDto::from(&c.atl_btl),
        budget_fringes: c.budget_fringes.as_ref().map(|f| CostItemDto {
            name: f.name.clone(),
            figures: FiguresDto::from(&f.figures),
        }),
        charges: c
            .charges
            .iter()
            .map(|x| CostItemDto {
                name: x.name.clone(),
                figures: FiguresDto::from(&x.figures),
            })
            .collect(),
        unassigned: FiguresDto::from(&c.unassigned),
        unassigned_codes: c.unassigned_codes.iter().map(|(k, s)| sum(k, s)).collect(),
        total: FiguresDto::from(&c.total),
        lines: c
            .lines
            .iter()
            .filter(|(id, _)| lines(**id))
            .map(|(id, s)| (*id, sum("", s)))
            .collect(),
        error: None,
    }
}

/// The rollup cache a session keeps: the actuals it was made from (kept
/// alive here, so a pointer match means the very same actuals) and the
/// account pattern.
#[derive(Debug, Default)]
pub struct RollupCache {
    entry: Option<(
        Arc<method_core::actuals::Actuals>,
        method_core::coding::AccountCoding,
        Option<Arc<Rollup>>,
    )>,
    /// How many times the rollup was worked out (for tests).
    pub computed: u64,
}

impl RollupCache {
    /// `b`'s actuals rolled up, worked out again only when they or the
    /// account pattern changed. `None` on overflow.
    pub fn get(&mut self, b: &Budget) -> Option<Arc<Rollup>> {
        if let Some((a, coding, roll)) = &self.entry {
            if Arc::ptr_eq(a, &b.actuals) && *coding == b.account_coding {
                return roll.clone();
            }
        }
        self.computed += 1;
        let roll = actuals::rollup(&b.actuals, &b.account_coding).map(Arc::new);
        self.entry = Some((Arc::clone(&b.actuals), b.account_coding, roll.clone()));
        roll
    }
}
