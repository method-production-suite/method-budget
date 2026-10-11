//! Side-by-side comparison of 2 to [`MAX_COMPARED`] budgets (or versions of one).
//!
//! # Matching
//!
//! Categories and accounts are matched **by code**. Each budget's codes are
//! parsed with that budget's own account coding into segments (category,
//! account, sub-account, set), and two codes match when their segments are
//! equal. So `1100`, `11-00` and `11 0` are the same account under the default
//! pattern, and budgets that use different separators still line up. A code
//! that does not fit its budget's pattern (one the pattern could only read by
//! cutting it short, such as `050101` under 2 + 2) falls back to its trimmed
//! text, so two different accounts are never added together by being cut.
//! A category is matched by the category it names
//! ([`AccountCoding::category_key`]): `11`, `1100` and `11-00` are category
//! `11` under the default pattern.
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
//! Production levels (each production-total row) are matched by name, trimmed
//! and case-insensitive; the unnamed level after the last production total is
//! not listed (the topsheet doesn't show it either).
//!
//! Charges and credits are compared one by one as well, matched by name the
//! same way (trimmed, case-insensitive); one only some budgets have is
//! `None` in the others, so differently named items list per budget. Their
//! totals stay in [`CompareTotals`].
//!
//! Each matched category, account and row also says where it is in each
//! budget (`at`, `ids`), so a caller can take the user to it.
//!
//! [`CompareLine::aggregate`] gives the other way of reading a comparison:
//! the sum and average of every budget but the first, and their variance
//! (that sum minus the first budget's value) as a percent of the average.
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

/// Every budget but the first, taken together (see [`CompareLine::aggregate`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Aggregate {
    /// Sum of the other budgets' values, a missing value counting as 0.
    pub sum: Cents,
    /// `sum` divided by how many other budgets there are, rounded once.
    pub average: Cents,
    /// `sum` minus the first budget's value (missing counts as 0).
    pub variance: Cents,
    /// `variance` as a percent of the average's size (exact, not of the
    /// rounded `average`), so its sign follows `variance`. `None` when the
    /// sum is 0.
    pub percent: Option<Dec>,
}

impl CompareLine {
    /// The other budgets taken together: their sum and average, and the sum
    /// minus the first budget's value as a percent of the average. With one
    /// other budget the variance is simply its difference from the first and
    /// the percent is against that other budget. `None` with fewer than two
    /// budgets, or on overflow.
    pub fn aggregate(&self) -> Option<Aggregate> {
        let others = self.values.get(1..).filter(|o| !o.is_empty())?;
        let n = i64::try_from(others.len()).ok()?;
        let sum = others
            .iter()
            .try_fold(0 as Cents, |acc, v| acc.checked_add(v.unwrap_or(0)))?;
        let average = Dec::from_cents(sum)
            .checked_div(Dec::from_int(n))?
            .to_cents()?;
        let base = self.values.first().copied().flatten().unwrap_or(0);
        let variance = sum.checked_sub(base)?;
        // variance / (sum / n) * 100 = variance * n * 100 / |sum|
        let percent = if sum == 0 {
            None
        } else {
            Some(
                Dec::from_int(variance)
                    .checked_mul(Dec::from_int(n.checked_mul(100)?))?
                    .checked_div(Dec::from_int(sum.checked_abs()?))?,
            )
        };
        Some(Aggregate {
            sum,
            average,
            variance,
            percent,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Comparison {
    /// The labels passed in, in order.
    pub labels: Vec<String>,
    pub categories: Vec<CompareCategory>,
    /// Each production total, matched by name (see the module docs).
    pub levels: Vec<CompareLevel>,
    /// Each charge, matched by name (see the module docs).
    pub charges: Vec<CompareItem>,
    /// Each credit, matched by name.
    pub credits: Vec<CompareItem>,
    pub totals: CompareTotals,
}

/// One charge or credit across the budgets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompareItem {
    /// The name as written in the first budget that has it.
    pub name: String,
    pub line: CompareLine,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompareLevel {
    /// The name as written in the first budget that has it.
    pub name: String,
    pub line: CompareLine,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompareCategory {
    /// The code and name as written in the first budget that has the category.
    pub code: String,
    pub name: String,
    pub line: CompareLine,
    /// Per budget: where the category is in that budget's report
    /// (`Report::categories` index; the first one when the code repeats).
    pub at: Vec<Option<usize>>,
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
    /// Per budget: (report category index, account index in it). A fringe
    /// account's index is one past the category's last account, where
    /// account lists show it.
    pub at: Vec<Option<(usize, usize)>>,
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
    /// Per budget: (report category index, account index in it), `None`
    /// where the account is missing.
    pub at: Vec<Option<(usize, usize)>>,
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
    /// Per budget: the row's id there (the first match), `None` where it is missing.
    pub ids: Vec<Option<RowId>>,
    /// Each budget's row total (0 for a row left out of the total).
    pub line: CompareLine,
}

// ---------------------------------------------------------------------------
// Matching machinery

/// A code split by a budget's coding, or its trimmed text when the coding
/// can't read it without cutting something off ([`AccountCoding::read`]).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum CodeKey {
    Coded([String; 4]),
    Raw(String),
}

fn code_key(coding: &AccountCoding, code: &str) -> CodeKey {
    match coding.read(code) {
        Some(c) => CodeKey::Coded([c.category, c.account, c.sub, c.set]),
        None => CodeKey::Raw(code.trim().to_string()),
    }
}

/// One matched item: what describes it, a value per budget, and where it
/// first turned up in each budget.
struct Entry<T, P> {
    item: T,
    values: Vec<Option<Cents>>,
    places: Vec<Option<P>>,
}

/// Items keyed by `K`, in first-seen order, each with a value per budget.
struct Matched<K, T, P> {
    index: BTreeMap<K, usize>,
    items: Vec<Entry<T, P>>,
}

impl<K: Ord, T, P: Clone> Matched<K, T, P> {
    fn new() -> Self {
        Matched {
            index: BTreeMap::new(),
            items: Vec::new(),
        }
    }

    /// Adds `value` for budget `i` of `n` under `key`, found at `place` in
    /// that budget, creating the item with `make` the first time the key is
    /// seen. `None` on overflow.
    fn add(
        &mut self,
        key: K,
        (n, i): (usize, usize),
        place: P,
        value: Cents,
        make: impl FnOnce() -> T,
    ) -> Option<&mut T> {
        let items = &mut self.items;
        let at = *self.index.entry(key).or_insert_with(|| {
            items.push(Entry {
                item: make(),
                values: vec![None; n],
                places: vec![None; n],
            });
            items.len() - 1
        });
        let e = &mut self.items[at];
        e.values[i] = Some(e.values[i].unwrap_or(0).checked_add(value)?);
        e.places[i].get_or_insert(place);
        Some(&mut e.item)
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

/// An account's key: (is a fringe account, its code).
type AccountKey = (bool, CodeKey);
/// What describes an account: code and name as written, and whether it is a
/// fringe account.
type AccountItem = (String, String, bool);

struct CatItem {
    code: String,
    name: String,
    accounts: Matched<AccountKey, AccountItem, (usize, usize)>,
}

/// Compares 2 to [`MAX_COMPARED`] budgets by category and account code. The
/// first budget is the base every difference is measured from. See the module
/// docs for matching and ordering.
pub fn compare(budgets: &[(String, &Budget)]) -> Result<Comparison, CalcError> {
    let reports = reports(budgets)?;
    let n = budgets.len();
    let ovf = || overflow("comparison");

    // Categories by the category they name (`11`, `1100` and `11-00` are
    // one category), so a chart written with full-width category numbers
    // lines up with one written with category parts.
    let mut cats: Matched<String, CatItem, usize> = Matched::new();
    let mut levels: Matched<String, String, ()> = Matched::new();
    let mut charges: Matched<String, String, ()> = Matched::new();
    let mut credits: Matched<String, String, ()> = Matched::new();
    for (i, ((_, b), r)) in budgets.iter().zip(&reports).enumerate() {
        let coding = &b.account_coding;
        for (ci, cat) in r.categories.iter().enumerate() {
            let key = coding.category_key(&cat.code);
            let item = cats
                .add(key, (n, i), ci, cat.total, || CatItem {
                    code: cat.code.clone(),
                    name: cat.name.clone(),
                    accounts: Matched::new(),
                })
                .ok_or_else(ovf)?;
            for (ai, acct) in cat.accounts.iter().enumerate() {
                let key = (false, code_key(coding, &acct.code));
                item.accounts
                    .add(key, (n, i), (ci, ai), acct.total, || {
                        (acct.code.clone(), acct.name.clone(), false)
                    })
                    .ok_or_else(ovf)?;
            }
            if let Some((code, amount)) = &cat.fringe_account {
                let key = (true, code_key(coding, code));
                let place = (ci, cat.accounts.len());
                item.accounts
                    .add(key, (n, i), place, *amount, || {
                        (code.clone(), String::new(), true)
                    })
                    .ok_or_else(ovf)?;
            }
        }
        for lv in r.levels.iter().filter(|l| !l.name.trim().is_empty()) {
            let key = lv.name.trim().to_lowercase();
            levels
                .add(key, (n, i), (), lv.total, || lv.name.clone())
                .ok_or_else(ovf)?;
        }
        for c in &r.charges {
            let key = c.name.trim().to_lowercase();
            charges
                .add(key, (n, i), (), c.amount, || c.name.clone())
                .ok_or_else(ovf)?;
        }
        for c in &r.credits {
            let key = c.name.trim().to_lowercase();
            credits
                .add(key, (n, i), (), c.amount, || c.name.clone())
                .ok_or_else(ovf)?;
        }
    }

    let mut categories = Vec::with_capacity(cats.items.len());
    for cat in cats.items {
        let mut accounts = Vec::with_capacity(cat.item.accounts.items.len());
        for a in cat.item.accounts.items {
            let (code, name, fringe_account) = a.item;
            accounts.push(CompareAccount {
                code,
                name,
                fringe_account,
                line: line(a.values)?,
                at: a.places,
            });
        }
        categories.push(CompareCategory {
            code: cat.item.code,
            name: cat.item.name,
            line: line(cat.values)?,
            at: cat.places,
            accounts,
        });
    }
    let levels = levels
        .items
        .into_iter()
        .map(|l| {
            Ok(CompareLevel {
                name: l.item,
                line: line(l.values)?,
            })
        })
        .collect::<Result<Vec<_>, CalcError>>()?;

    let items = |m: Matched<String, String, ()>| {
        m.items
            .into_iter()
            .map(|e| {
                Ok(CompareItem {
                    name: e.item,
                    line: line(e.values)?,
                })
            })
            .collect::<Result<Vec<_>, CalcError>>()
    };
    let charge_items = items(charges)?;
    let credit_items = items(credits)?;

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
        levels,
        charges: charge_items,
        credits: credit_items,
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
    let mut rows: Matched<RowKey, (RowId, RowKind, String), RowId> = Matched::new();
    let (mut details, mut fringes, mut totals) = (vec![None; n], vec![None; n], vec![None; n]);
    let mut at = vec![None; n];
    for (i, ((_, b), r)) in budgets.iter().zip(&reports).enumerate() {
        let want = code_key(&b.account_coding, code);
        let Some((place, acct)) = r
            .categories
            .iter()
            .enumerate()
            .flat_map(|(ci, c)| {
                c.accounts
                    .iter()
                    .enumerate()
                    .map(move |(ai, a)| ((ci, ai), a))
            })
            .find(|(_, a)| code_key(&b.account_coding, &a.code) == want)
        else {
            continue;
        };
        at[i] = Some(place);
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
            rows.add(key, (n, i), row.id, row.total, || {
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
        at,
        rows: rows
            .items
            .into_iter()
            .map(|e| {
                let (id, kind, description) = e.item;
                Ok(CompareRow {
                    id,
                    kind,
                    description,
                    ids: e.places,
                    line: line(e.values)?,
                })
            })
            .collect::<Result<_, CalcError>>()?,
        details_total: line(details)?,
        fringe_total: line(fringes)?,
        total: line(totals)?,
    })
}
