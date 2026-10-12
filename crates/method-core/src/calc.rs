//! Recalculation: a pure function from [`Budget`] to [`Report`].
//!
//! Same budget and options in, identical report out. Money is rounded to cents
//! at the line (and once per fringe, charge and credit; see docs/decisions.md).
//! Everything above that is integer addition, so rollups always agree. How
//! numbers are multiplied and rounded is the budget's [`Arithmetic`], all of
//! it in [`arith`].
//!
//! Stages, each in its own module:
//! 1. [`lines`]: evaluate every row of every account (formulas, currency,
//!    suppression, subtotal and adjustment rows).
//! 2. [`fringes`]: fringe dollars per row (cutoffs, ranges, unit conversion).
//! 3. [`topsheet`]: fringe posting, production levels, ATL/BTL, charges, credits.
//! 4. [`breakdown`]: per-fringe, group, set and location totals.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::decimal::Dec;
use crate::expr::{is_ident, EvalError, Expr, ParseError};
use crate::model::{Arithmetic, Budget, Detail, Formula, RowId};
use crate::report::{AccountTotals, CategoryTotals, Report, RowTotals};
use crate::Cents;
use arith::Num;

pub mod arith;
pub mod breakdown;
pub mod cache;
pub(crate) mod display;
pub mod explain;
pub(crate) mod fringes;
pub(crate) mod lines;
#[cfg(test)]
mod reference;
pub(crate) mod topsheet;

pub use fringes::day_unit;
pub use topsheet::charge_preview;

/// Which details count toward totals, on top of group suppression.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecalcOptions {
    /// Lines tagged with any of these groups are left out.
    pub exclude_groups: BTreeSet<String>,
    /// When set, only lines tagged with at least one of these groups count.
    pub only_groups: Option<BTreeSet<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CalcError {
    /// Where it happened, e.g. `account 1100 line 2 (Writer): rate`.
    pub at: String,
    pub kind: ErrorKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    Parse(ParseError),
    Eval(EvalError),
    BadGlobalName,
    /// The globals that refer to each other, ending where it started.
    GlobalCycle(Vec<String>),
    UnknownCurrency(String),
    UnknownFringe(String),
    UnknownGroup(String),
    DuplicateRowId(RowId),
    Overflow,
    /// A comparison was asked for with this many budgets; it takes 2 to 16.
    CompareCount(usize),
}

impl fmt::Display for CalcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: ", self.at)?;
        match &self.kind {
            ErrorKind::Parse(e) => write!(f, "{e}"),
            ErrorKind::Eval(e) => write!(f, "{e}"),
            ErrorKind::BadGlobalName => f.write_str("global names must be letters, digits and _"),
            ErrorKind::GlobalCycle(c) => {
                write!(f, "globals refer to each other: {}", c.join(" -> "))
            }
            ErrorKind::UnknownCurrency(c) => write!(f, "no exchange rate for `{c}`"),
            ErrorKind::UnknownFringe(n) => write!(f, "unknown fringe `{n}`"),
            ErrorKind::UnknownGroup(n) => write!(f, "unknown group `{n}`"),
            ErrorKind::DuplicateRowId(id) => write!(f, "row id {id} is used twice"),
            ErrorKind::Overflow => f.write_str("number too large"),
            ErrorKind::CompareCount(n) => write!(
                f,
                "a comparison takes {} to {} budgets, not {n}",
                crate::comparison::MIN_COMPARED,
                crate::comparison::MAX_COMPARED
            ),
        }
    }
}

impl std::error::Error for CalcError {}

pub(crate) fn err(at: impl Into<String>, kind: ErrorKind) -> CalcError {
    CalcError {
        at: at.into(),
        kind,
    }
}

pub(crate) fn overflow(at: impl Into<String>) -> CalcError {
    err(at, ErrorKind::Overflow)
}

/// Checked sum of cents.
pub(crate) fn sum(it: impl IntoIterator<Item = Cents>) -> Option<Cents> {
    it.into_iter().try_fold(0, Cents::checked_add)
}

/// Shared, read-only state for one recalc.
pub(crate) struct Ctx<'a> {
    pub b: &'a Budget,
    /// The budget's arithmetic (`b.arithmetic`).
    pub arith: Arithmetic,
    pub globals: BTreeMap<String, Num>,
    pub opts: &'a RecalcOptions,
    /// When true, ignore group suppression and options: used for "full" group totals.
    pub unfiltered: bool,
}

impl<'a> Ctx<'a> {
    /// The counted (filtered) context for `b`, with its globals evaluated.
    pub fn new(b: &'a Budget, opts: &'a RecalcOptions) -> Result<Ctx<'a>, CalcError> {
        Ok(Ctx {
            b,
            arith: b.arithmetic,
            globals: eval_globals(b)?,
            opts,
            unfiltered: false,
        })
    }

    /// `d` in the budget's arithmetic.
    pub fn num(&self, d: Dec) -> Num {
        Num::of(self.arith, d)
    }

    /// Evaluates a formula in the budget's arithmetic; a blank one is
    /// `blank`.
    pub fn eval(&self, f: &Formula, blank: Dec, at: &dyn Fn() -> String) -> Result<Num, CalcError> {
        if f.is_blank() {
            return Ok(self.num(blank));
        }
        eval_in(self.arith, &f.0, &|n| self.globals.get(n).copied()).map_err(|e| err(at(), e))
    }

    /// The globals as decimals, as the report shows them.
    pub fn shown_globals(&self) -> Result<BTreeMap<String, Dec>, CalcError> {
        self.globals
            .iter()
            .map(|(n, v)| {
                let d = v.shown().ok_or_else(|| overflow(format!("global {n}")))?;
                Ok((n.clone(), d))
            })
            .collect()
    }

    /// Whether a line counts toward totals: not left out by the group rules
    /// (a group with "in budget total" off, or no group when lines with no
    /// group are excluded; decisions.md, entry 57), and allowed by the
    /// options.
    pub fn includes(&self, d: &Detail, at: &dyn Fn() -> String) -> Result<bool, CalcError> {
        let mut counted = Vec::with_capacity(d.groups.len());
        for g in &d.groups {
            let group = self
                .b
                .groups
                .get(g)
                .ok_or_else(|| err(at(), ErrorKind::UnknownGroup(g.clone())))?;
            counted.push(group.in_budget_total);
        }
        if self.unfiltered {
            return Ok(true);
        }
        if self.b.group_rules.counts(counted).is_err()
            || d.groups
                .iter()
                .any(|g| self.opts.exclude_groups.contains(g))
        {
            return Ok(false);
        }
        Ok(match &self.opts.only_groups {
            Some(only) => d.groups.iter().any(|g| only.contains(g)),
            None => true,
        })
    }
}

/// Formula `src` in `arith`: in decimals ([`Expr::eval`]) or in doubles
/// ([`Expr::eval_f64`]), plain numbers and bare globals without a parse
/// tree (see [`crate::expr::quick_value`]).
pub(crate) fn eval_in(
    arith: Arithmetic,
    src: &str,
    lookup: &dyn Fn(&str) -> Option<Num>,
) -> Result<Num, ErrorKind> {
    let quick = match arith {
        Arithmetic::Exact => crate::expr::quick_value(src, &|n| lookup(n)?.shown()).map(Num::Dec),
        Arithmetic::Mmb => {
            crate::expr::quick_value_f64(src, &|n| Some(lookup(n)?.f64())).map(Num::F64)
        }
    };
    if let Some(v) = quick {
        return Ok(v);
    }
    let e = Expr::parse(src).map_err(ErrorKind::Parse)?;
    eval_expr(arith, &e, lookup).map_err(ErrorKind::Eval)
}

/// A parsed formula in `arith`.
fn eval_expr(
    arith: Arithmetic,
    e: &Expr,
    lookup: &dyn Fn(&str) -> Option<Num>,
) -> Result<Num, EvalError> {
    match arith {
        Arithmetic::Exact => e.eval(&|n| lookup(n)?.shown()).map(Num::Dec),
        Arithmetic::Mmb => e.eval_f64(&|n| Some(lookup(n)?.f64())).map(Num::F64),
    }
}

/// Where a row lives, for error messages.
pub(crate) fn row_at(code: &str, idx: usize, d: &Detail, field: &str) -> String {
    format!(
        "account {code} line {} ({}): {field}",
        idx + 1,
        d.description
    )
}

/// Every global's value in the budget's arithmetic: Movie Magic keeps a
/// global as a double (`17/3` is 5.666666666666667), exact arithmetic as a
/// 9-place decimal.
pub(crate) fn eval_globals(b: &Budget) -> Result<BTreeMap<String, Num>, CalcError> {
    // Each global with the other globals it refers to (unknown names are left for
    // eval to report, with the global as context).
    let mut parsed: BTreeMap<&str, (Expr, Vec<&str>)> = BTreeMap::new();
    for (name, f) in &b.globals {
        let at = || format!("global {name}");
        if !is_ident(name) {
            return Err(err(at(), ErrorKind::BadGlobalName));
        }
        let e = Expr::parse(&f.0).map_err(|e| err(at(), ErrorKind::Parse(e)))?;
        let mut vars = BTreeSet::new();
        e.vars(&mut vars);
        let deps = vars
            .iter()
            .filter_map(|v| b.globals.get_key_value(v).map(|(k, _)| k.as_str()))
            .collect();
        parsed.insert(name.as_str(), (e, deps));
    }

    // Depth-first in name order, with an explicit stack so a long chain of
    // globals can't overflow the call stack.
    let mut done: BTreeMap<String, Num> = BTreeMap::new();
    for &root in parsed.keys() {
        if done.contains_key(root) {
            continue;
        }
        // (global, index of the next dependency to visit)
        let mut stack: Vec<(&str, usize)> = vec![(root, 0)];
        let mut on_stack: BTreeSet<&str> = BTreeSet::from([root]);
        while let Some(&(name, next)) = stack.last() {
            let (expr, deps) = &parsed[name];
            if let Some(&dep) = deps.get(next) {
                if let Some(top) = stack.last_mut() {
                    top.1 += 1;
                }
                if done.contains_key(dep) {
                    continue;
                }
                if on_stack.contains(dep) {
                    let start = stack.iter().position(|(n, _)| *n == dep).unwrap_or(0);
                    let mut cycle: Vec<String> =
                        stack[start..].iter().map(|(n, _)| n.to_string()).collect();
                    cycle.push(dep.to_string());
                    return Err(err(format!("global {dep}"), ErrorKind::GlobalCycle(cycle)));
                }
                stack.push((dep, 0));
                on_stack.insert(dep);
                continue;
            }
            let v = eval_expr(b.arithmetic, expr, &|n| done.get(n).copied())
                .map_err(|e| err(format!("global {name}"), ErrorKind::Eval(e)))?;
            done.insert(name.to_string(), v);
            stack.pop();
            on_stack.remove(name);
        }
    }
    Ok(done)
}

fn check_row_ids(b: &Budget) -> Result<(), CalcError> {
    let mut seen = BTreeSet::new();
    for acct in b.categories().flat_map(|c| &c.accounts) {
        for (i, d) in acct.details.iter().enumerate() {
            if !seen.insert(d.id) {
                return Err(err(
                    row_at(&acct.code, i, d, "id"),
                    ErrorKind::DuplicateRowId(d.id),
                ));
            }
        }
    }
    Ok(())
}

/// Stages 1 and 2 for every account, with category and account sums filled in.
/// `total` fields are left equal to `details_total`; posting adjusts them.
#[cfg(test)]
pub(crate) fn eval_categories(ctx: &Ctx) -> Result<Vec<CategoryTotals>, CalcError> {
    eval_categories_with(ctx, None)
}

/// [`eval_categories`], taking each unchanged account's rows from `cache`
/// (and keeping the ones evaluated there).
fn eval_categories_with(
    ctx: &Ctx,
    mut cache: Option<&mut cache::Accounts>,
) -> Result<Vec<CategoryTotals>, CalcError> {
    let ovf = || overflow("budget totals");
    let mut categories = Vec::new();
    let mut k = 0;
    for cat in ctx.b.categories() {
        let mut accounts = Vec::with_capacity(cat.accounts.len());
        for acct in &cat.accounts {
            let reused = cache.as_deref().and_then(|c| c.counted(k, acct));
            let (rows, details_total, fringe_total) = match reused {
                Some(hit) => hit,
                None => {
                    let mut rows = lines::eval_rows(ctx, acct)?;
                    fringes::apply_fringes(ctx, acct, &mut rows)?;
                    let details_total = sum(rows.iter().map(|r| r.total)).ok_or_else(ovf)?;
                    let fringe_total = sum(rows.iter().map(|r| r.fringe_total)).ok_or_else(ovf)?;
                    if let Some(c) = cache.as_deref_mut() {
                        c.keep(k, acct, &rows, details_total, fringe_total);
                    }
                    (rows, details_total, fringe_total)
                }
            };
            k += 1;
            accounts.push(AccountTotals {
                code: acct.code.clone(),
                name: acct.name.clone(),
                rows,
                details_total,
                fringe_total,
                total: details_total,
            });
        }
        let details_total = sum(accounts.iter().map(|a| a.details_total)).ok_or_else(ovf)?;
        let fringe_total = sum(accounts.iter().map(|a| a.fringe_total)).ok_or_else(ovf)?;
        categories.push(CategoryTotals {
            code: cat.code.clone(),
            name: cat.name.clone(),
            accounts,
            details_total,
            fringe_total,
            fringe_account: None,
            total: details_total,
        });
    }
    Ok(categories)
}

/// Per category, per account: the account's unfiltered rows, or `None` when
/// they are the same as its counted rows.
pub(crate) type FullRows = Vec<Vec<Option<Vec<RowTotals>>>>;

/// Unfiltered rows for each account (as `ctx.unfiltered` evaluates them),
/// for the "full" group totals. `None` for an account whose rows all count:
/// evaluating it again unfiltered would give exactly the same rows, so the
/// counted ones stand in. Only accounts with a row left out are evaluated
/// again. Errors are the ones a full unfiltered pass would raise, in the
/// same order (accounts that are skipped can't raise any).
fn eval_full(
    ctx: &Ctx,
    active: &[CategoryTotals],
    mut cache: Option<&mut cache::Accounts>,
) -> Result<FullRows, CalcError> {
    let ovf = || overflow("budget totals");
    let mut out = Vec::with_capacity(active.len());
    let mut k = 0;
    for (cat, ac) in ctx.b.categories().zip(active) {
        let mut accounts = Vec::with_capacity(cat.accounts.len());
        let mut details_totals = Vec::with_capacity(cat.accounts.len());
        let mut fringe_totals = Vec::with_capacity(cat.accounts.len());
        for (acct, aa) in cat.accounts.iter().zip(&ac.accounts) {
            let at = k;
            k += 1;
            if aa.rows.iter().all(|r| r.included) {
                details_totals.push(aa.details_total);
                fringe_totals.push(aa.fringe_total);
                accounts.push(None);
                continue;
            }
            if let Some(hit) = cache.as_deref().and_then(|c| c.full(at)) {
                details_totals.push(hit.1);
                fringe_totals.push(hit.2);
                accounts.push(Some(hit.0));
                continue;
            }
            let mut rows = lines::eval_rows(ctx, acct)?;
            fringes::apply_fringes(ctx, acct, &mut rows)?;
            let dt = sum(rows.iter().map(|r| r.total)).ok_or_else(ovf)?;
            let ft = sum(rows.iter().map(|r| r.fringe_total)).ok_or_else(ovf)?;
            if let Some(c) = cache.as_deref_mut() {
                c.keep_full(at, &rows, dt, ft);
            }
            details_totals.push(dt);
            fringe_totals.push(ft);
            accounts.push(Some(rows));
        }
        // The category sums a full pass adds up, for the same overflow errors.
        sum(details_totals).ok_or_else(ovf)?;
        sum(fringe_totals).ok_or_else(ovf)?;
        out.push(accounts);
    }
    Ok(out)
}

/// The rows a group's total takes its fringes from (decisions.md, entry 58):
/// the group rules apply but not the recalc options' what-if filters, so a
/// suppressed line earns no fringes and the figure doesn't move with the
/// filters. `None` when there are no filters: the counted rows are those
/// rows. `ctx` is any context for the budget (its globals are reused).
pub(crate) fn eval_earned(
    ctx: &Ctx,
    active: &[CategoryTotals],
) -> Result<Option<FullRows>, CalcError> {
    if ctx.opts.exclude_groups.is_empty() && ctx.opts.only_groups.is_none() {
        return Ok(None);
    }
    let plain = RecalcOptions::default();
    let earned_ctx = Ctx {
        b: ctx.b,
        arith: ctx.arith,
        globals: ctx.globals.clone(),
        opts: &plain,
        unfiltered: false,
    };
    eval_full(&earned_ctx, active, None).map(Some)
}

/// What `b`'s totals (`r`, from recalculating it) leave out or count in a
/// way worth a look, in plain words: fringes of categories after the last
/// production total under production-level posting, which no level holds
/// (decisions.md, entry 56), and credit rows on a category the budget
/// doesn't have (entry 57). `method check` and the MCP `validate` tool list
/// them; they never stop a recalc.
pub fn warnings(b: &Budget, r: &Report) -> Vec<String> {
    topsheet::trailing_fringes_warning(b, r)
        .into_iter()
        .chain(topsheet::credit_category_warnings(b))
        .collect()
}

/// Recalculates every total in `b`.
pub fn recalc(b: &Budget, opts: &RecalcOptions) -> Result<Report, CalcError> {
    recalc_with(b, opts, None)
}

/// [`recalc`], reusing account rows from `cache` when given (see
/// [`cache::RecalcCache`]).
fn recalc_with(
    b: &Budget,
    opts: &RecalcOptions,
    mut cache: Option<&mut cache::Accounts>,
) -> Result<Report, CalcError> {
    check_row_ids(b)?;
    let totals_rate = match &b.totals_currency {
        Some(c) if *c != b.base_currency => {
            b.currencies
                .get(c)
                .ok_or_else(|| err("totals currency", ErrorKind::UnknownCurrency(c.clone())))?
                .rate
        }
        _ => Dec::ONE,
    };
    let ctx = Ctx::new(b, opts)?;
    let globals = ctx.shown_globals()?;
    let categories = eval_categories_with(&ctx, cache.as_deref_mut())?;
    let ts = topsheet::finish(&ctx, categories)?;
    // The topsheet in the totals currency, converted row by row.
    let display = b
        .totals_currency
        .as_ref()
        .is_some_and(|c| *c != b.base_currency)
        .then(|| display::display_totals(&ctx, &ts, totals_rate).ok())
        .flatten();

    let full_ctx = Ctx {
        unfiltered: true,
        ..ctx
    };
    let full = eval_full(&full_ctx, &ts.categories, cache)?;
    let earned = eval_earned(&full_ctx, &ts.categories)?;
    let bd = breakdown::breakdowns(&full_ctx, &ts.categories, &full, earned.as_ref())?;

    Ok(Report {
        base_currency: b.base_currency.clone(),
        totals_currency: b
            .totals_currency
            .clone()
            .unwrap_or_else(|| b.base_currency.clone()),
        totals_rate,
        globals,
        categories: ts.categories,
        levels: ts.levels,
        budget_fringes: ts.budget_fringes,
        total_atl: ts.total_atl,
        total_btl: ts.total_btl,
        atl_btl: ts.atl_btl,
        charges: ts.charges,
        grand_total: ts.grand_total,
        credits: ts.credits,
        credits_total: ts.credits_total,
        net_total: ts.net_total,
        fringes: bd.fringes,
        groups: bd.groups,
        sets: bd.sets,
        locations: bd.locations,
        display,
    })
}
