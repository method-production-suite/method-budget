//! Side-by-side comparison of 2 to [`MAX_COMPARED`] budgets (or versions of one).
//!
//! # Matching
//!
//! Categories and accounts are matched **by code**. Each budget's codes are
//! parsed with that budget's own account coding into segments (category,
//! account, sub-account, set), and two codes match when their segments are
//! equal. So `1100`, `11-00` and `11 0` are the same account under the default
//! pattern, and budgets that use different separators still line up. A code
//! that does not fit its budget's pattern falls back to its trimmed text.
//! Segments are compared exactly: `11-01` with a 2-digit account segment does
//! not match `11-001` with a 3-digit one.
//!
//! Accounts are matched within their category. Category fringe accounts (under
//! [`FringePosting::Category`](crate::FringePosting)) are listed as accounts
//! flagged `fringe_account`, matched only with other fringe accounts.
//!
//! If one budget has two categories (or two accounts in a category) with the
//! same code, their totals are added together.
//!
//! # Ordering
//!
//! Deterministic: everything in the first budget's order, then anything it
//! doesn't have, in the order it first appears in the later budgets. Accounts
//! follow the same rule inside each category.
//!
//! # Figures
//!
//! Each budget is recalculated with default options. Every row carries each
//! budget's value (`None` when that budget doesn't have it) and its difference
//! from the first budget: amount (a missing value counts as 0) and percent of
//! the first budget's value (`None` when that value is 0 or missing). The
//! percent is taken against the base's size, so its sign always matches the
//! amount's, and it keeps the engine's 9 decimal places; display rounding is
//! the caller's job.
//!
//! Account figures are the account totals shown on the topsheet (they include
//! fringes only under account posting), category figures are category totals,
//! exactly as [`Baseline::capture`](crate::Baseline::capture) records them.
//!
//! # Detail level
//!
//! [`compare_account`] compares the rows of one account. Rows are matched by
//! [`RowMatch::Id`] (for versions of the same budget, where row ids are stable)
//! or by [`RowMatch::Description`] (for separate budgets): trimmed and
//! case-insensitive, and the n-th row with a description matches the n-th row
//! with the same description elsewhere. Only costed rows (lines and
//! adjustments) are compared.

use std::collections::BTreeMap;

use crate::calc::{err, overflow, recalc, CalcError, ErrorKind, RecalcOptions};
use crate::coding::AccountCoding;
use crate::decimal::Dec;
use crate::model::{Budget, RowId, RowKind};
use crate::report::Report;
use crate::Cents;

/// Fewest budgets [`compare`] takes.
pub const MIN_COMPARED: usize = 2;
/// Most budgets [`compare`] takes: a base and 15 others.
pub const MAX_COMPARED: usize = 16;

/// One budget's difference from the first budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Diff {
    /// This value minus the first budget's, a missing value counting as 0.
    pub amount: Cents,
    /// `amount` as a percent of the first budget's value (`12.5` is 12.5%),
    /// `None` when the first budget's value is 0 or missing.
    pub percent: Option<Dec>,
}

/// One compared figure across every budget.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompareLine {
    /// Per budget, in input order. `None` when that budget doesn't have it.
    pub values: Vec<Option<Cents>>,
    /// Per budget, in input order. The first entry is always a zero difference.
    pub diffs: Vec<Diff>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Comparison {
    /// The labels passed in, in order.
    pub labels: Vec<String>,
    pub categories: Vec<CompareCategory>,
    pub totals: CompareTotals,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompareCategory {
    /// The code and name as written in the first budget that has the category.
    pub code: String,
    pub name: String,
    pub line: CompareLine,
    pub accounts: Vec<CompareAccount>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompareAccount {
    /// The code and name as written in the first budget that has the account.
    /// A fringe account's name is empty.
    pub code: String,
    pub name: String,
    /// A category fringe account rather than a real account.
    pub fringe_account: bool,
    pub line: CompareLine,
}

/// Budget-level figures. Every budget has all of them, so no value is `None`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompareTotals {
    pub total_atl: CompareLine,
    pub total_btl: CompareLine,
    /// Every fringe dollar, wherever it posts.
    pub fringes: CompareLine,
    /// Sum of the charges.
    pub charges: CompareLine,
    pub grand_total: CompareLine,
    pub credits: CompareLine,
    pub net_total: CompareLine,
}

/// How [`compare_account`] matches detail rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowMatch {
    /// By row id. For versions of the same budget, where ids are stable.
    Id,
    /// By description, trimmed and case-insensitive; the n-th row with a
    /// description matches the n-th row with the same description.
    Description,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountComparison {
    pub labels: Vec<String>,
    /// As written in the first budget that has the account; the code asked
    /// for (and an empty name) when no budget has it.
    pub code: String,
    pub name: String,
    pub rows: Vec<CompareRow>,
    /// Sum of the rows' totals (`None` where the account is missing).
    pub details_total: CompareLine,
    /// Fringes earned by the account's rows.
    pub fringe_total: CompareLine,
    /// The account total as on the topsheet.
    pub total: CompareLine,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompareRow {
    /// The row's id in the first budget that has it.
    pub id: RowId,
    /// Kind and description as in the first budget that has the row.
    pub kind: RowKind,
    pub description: String,
    /// Each budget's row total (0 for a row left out of the total).
    pub line: CompareLine,
}

// ---------------------------------------------------------------------------
// Matching machinery

/// A code split by a budget's coding, or its trimmed text when it doesn't fit.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum CodeKey {
    Coded([String; 4]),
    Raw(String),
}

fn code_key(coding: &AccountCoding, code: &str) -> CodeKey {
    match coding.parse(code) {
        Ok(c) => CodeKey::Coded([c.category, c.account, c.sub, c.set]),
        Err(_) => CodeKey::Raw(code.trim().to_string()),
    }
}

/// Items keyed by `K`, in first-seen order, each with a value per budget.
struct Matched<K, T> {
    index: BTreeMap<K, usize>,
    items: Vec<(T, Vec<Option<Cents>>)>,
}

impl<K: Ord, T> Matched<K, T> {
    fn new() -> Self {
        Matched {
            index: BTreeMap::new(),
            items: Vec::new(),
        }
    }

    /// Adds `value` for budget `i` of `n` under `key`, creating the item with
    /// `make` the first time the key is seen. `None` on overflow.
    fn add(
        &mut self,
        key: K,
        n: usize,
        i: usize,
        value: Cents,
        make: impl FnOnce() -> T,
    ) -> Option<&mut T> {
        let items = &mut self.items;
        let at = *self.index.entry(key).or_insert_with(|| {
            items.push((make(), vec![None; n]));
            items.len() - 1
        });
        let (item, values) = &mut self.items[at];
        values[i] = Some(values[i].unwrap_or(0).checked_add(value)?);
        Some(item)
    }
}

fn diff(base: Option<Cents>, v: Option<Cents>) -> Option<Diff> {
    let base = base.unwrap_or(0);
    let amount = v.unwrap_or(0).checked_sub(base)?;
    let percent = if base == 0 {
        None
    } else {
        Some(
            Dec::from_int(amount)
                .checked_mul(Dec::from_int(100))?
                .checked_div(Dec::from_int(base.checked_abs()?))?,
        )
    };
    Some(Diff { amount, percent })
}

fn line(values: Vec<Option<Cents>>) -> Result<CompareLine, CalcError> {
    let base = values[0];
    let diffs = values
        .iter()
        .map(|v| diff(base, *v))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| overflow("comparison"))?;
    Ok(CompareLine { values, diffs })
}

fn all(values: impl Iterator<Item = Cents>) -> Result<CompareLine, CalcError> {
    line(values.map(Some).collect())
}

/// Checks the count and recalculates each budget, labelling errors.
fn reports(budgets: &[(String, &Budget)]) -> Result<Vec<Report>, CalcError> {
    if !(MIN_COMPARED..=MAX_COMPARED).contains(&budgets.len()) {
        return Err(err("comparison", ErrorKind::CompareCount(budgets.len())));
    }
    budgets
        .iter()
        .map(|(label, b)| {
            recalc(b, &RecalcOptions::default()).map_err(|e| CalcError {
                at: format!("{label}: {}", e.at),
                kind: e.kind,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Topsheet and account level

struct CatItem {
    code: String,
    name: String,
    accounts: Matched<(bool, CodeKey), (String, String, bool)>,
}

/// Compares 2 to [`MAX_COMPARED`] budgets by category and account code. The
/// first budget is the base every difference is measured from. See the module
/// docs for matching and ordering.
pub fn compare(budgets: &[(String, &Budget)]) -> Result<Comparison, CalcError> {
    let reports = reports(budgets)?;
    let n = budgets.len();
    let ovf = || overflow("comparison");

    let mut cats: Matched<CodeKey, CatItem> = Matched::new();
    for (i, ((_, b), r)) in budgets.iter().zip(&reports).enumerate() {
        let coding = &b.account_coding;
        for cat in &r.categories {
            let item = cats
                .add(code_key(coding, &cat.code), n, i, cat.total, || CatItem {
                    code: cat.code.clone(),
                    name: cat.name.clone(),
                    accounts: Matched::new(),
                })
                .ok_or_else(ovf)?;
            for acct in &cat.accounts {
                let key = (false, code_key(coding, &acct.code));
                item.accounts
                    .add(key, n, i, acct.total, || {
                        (acct.code.clone(), acct.name.clone(), false)
                    })
                    .ok_or_else(ovf)?;
            }
            if let Some((code, amount)) = &cat.fringe_account {
                let key = (true, code_key(coding, code));
                item.accounts
                    .add(key, n, i, *amount, || (code.clone(), String::new(), true))
                    .ok_or_else(ovf)?;
            }
        }
    }

    let mut categories = Vec::with_capacity(cats.items.len());
    for (cat, values) in cats.items {
        let mut accounts = Vec::with_capacity(cat.accounts.items.len());
        for ((code, name, fringe_account), values) in cat.accounts.items {
            accounts.push(CompareAccount {
                code,
                name,
                fringe_account,
                line: line(values)?,
            });
        }
        categories.push(CompareCategory {
            code: cat.code,
            name: cat.name,
            line: line(values)?,
            accounts,
        });
    }

    let fringes = reports
        .iter()
        .map(|r| crate::calc::sum(r.categories.iter().map(|c| c.fringe_total)))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(ovf)?;
    let charges = reports
        .iter()
        .map(|r| crate::calc::sum(r.charges.iter().map(|c| c.amount)))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(ovf)?;

    Ok(Comparison {
        labels: budgets.iter().map(|(l, _)| l.clone()).collect(),
        categories,
        totals: CompareTotals {
            total_atl: all(reports.iter().map(|r| r.total_atl))?,
            total_btl: all(reports.iter().map(|r| r.total_btl))?,
            fringes: all(fringes.into_iter())?,
            charges: all(charges.into_iter())?,
            grand_total: all(reports.iter().map(|r| r.grand_total))?,
            credits: all(reports.iter().map(|r| r.credits_total))?,
            net_total: all(reports.iter().map(|r| r.net_total))?,
        },
    })
}

// ---------------------------------------------------------------------------
// Detail level

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum RowKey {
    Id(RowId),
    Description(String, usize),
}

/// Compares the rows of the account with `code` across 2 to [`MAX_COMPARED`]
/// budgets. `code` is matched in each budget through that budget's coding (the
/// first account with that code, in topsheet order). Budgets without the
/// account get `None` throughout. See the module docs for row matching.
pub fn compare_account(
    budgets: &[(String, &Budget)],
    code: &str,
    by: RowMatch,
) -> Result<AccountComparison, CalcError> {
    let reports = reports(budgets)?;
    let n = budgets.len();
    let ovf = || overflow("comparison");

    let mut found: Option<(String, String)> = None;
    let mut rows: Matched<RowKey, (RowId, RowKind, String)> = Matched::new();
    let (mut details, mut fringes, mut totals) = (vec![None; n], vec![None; n], vec![None; n]);
    for (i, ((_, b), r)) in budgets.iter().zip(&reports).enumerate() {
        let want = code_key(&b.account_coding, code);
        let Some(acct) = r
            .categories
            .iter()
            .flat_map(|c| &c.accounts)
            .find(|a| code_key(&b.account_coding, &a.code) == want)
        else {
            continue;
        };
        found.get_or_insert_with(|| (acct.code.clone(), acct.name.clone()));
        details[i] = Some(acct.details_total);
        fringes[i] = Some(acct.fringe_total);
        totals[i] = Some(acct.total);

        // Descriptions come from the budget itself; the report has only ids.
        let descriptions: BTreeMap<RowId, &str> = b
            .categories()
            .flat_map(|c| &c.accounts)
            .flat_map(|a| &a.details)
            .map(|d| (d.id, d.description.as_str()))
            .collect();
        let mut seen: BTreeMap<String, usize> = BTreeMap::new();
        for row in acct.rows.iter().filter(|row| row.kind.is_costed()) {
            let description = descriptions.get(&row.id).copied().unwrap_or("");
            let key = match by {
                RowMatch::Id => RowKey::Id(row.id),
                RowMatch::Description => {
                    let text = description.trim().to_lowercase();
                    let nth = seen.entry(text.clone()).or_insert(0);
                    *nth += 1;
                    RowKey::Description(text, *nth)
                }
            };
            rows.add(key, n, i, row.total, || {
                (row.id, row.kind, description.to_string())
            })
            .ok_or_else(ovf)?;
        }
    }

    let (code, name) = found.unwrap_or_else(|| (code.to_string(), String::new()));
    Ok(AccountComparison {
        labels: budgets.iter().map(|(l, _)| l.clone()).collect(),
        code,
        name,
        rows: rows
            .items
            .into_iter()
            .map(|((id, kind, description), values)| {
                Ok(CompareRow {
                    id,
                    kind,
                    description,
                    line: line(values)?,
                })
            })
            .collect::<Result<_, CalcError>>()?,
        details_total: line(details)?,
        fringe_total: line(fringes)?,
        total: line(totals)?,
    })
}
