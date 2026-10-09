//! Recalculation: a pure function from [`Budget`] to [`Report`].
//!
//! Same budget and options in, identical report out. Money is rounded to cents
//! at the line (and once per fringe, charge and credit; see docs/decisions.md).
//! Everything above that is integer addition, so rollups always agree.
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
use crate::model::{Budget, Detail, Formula, RowId};
use crate::report::{AccountTotals, CategoryTotals, Report};
use crate::Cents;

pub mod breakdown;
pub(crate) mod fringes;
pub(crate) mod lines;
pub(crate) mod topsheet;

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
    pub globals: BTreeMap<String, Dec>,
    pub opts: &'a RecalcOptions,
    /// When true, ignore group suppression and options: used for "full" group totals.
    pub unfiltered: bool,
}

impl Ctx<'_> {
    /// Evaluates a formula; a blank one is `blank`.
    pub fn eval(&self, f: &Formula, blank: Dec, at: &dyn Fn() -> String) -> Result<Dec, CalcError> {
        if f.is_blank() {
            return Ok(blank);
        }
        Expr::parse(&f.0)
            .map_err(|e| err(at(), ErrorKind::Parse(e)))?
            .eval(&|n| self.globals.get(n).copied())
            .map_err(|e| err(at(), ErrorKind::Eval(e)))
    }

    /// Whether a line counts toward totals: not suppressed by a group with
    /// "in budget total" off, and allowed by the options.
    pub fn includes(&self, d: &Detail, at: &dyn Fn() -> String) -> Result<bool, CalcError> {
        let mut suppressed = false;
        for g in &d.groups {
            let group = self
                .b
                .groups
                .get(g)
                .ok_or_else(|| err(at(), ErrorKind::UnknownGroup(g.clone())))?;
            suppressed |= !group.in_budget_total;
        }
        if self.unfiltered {
            return Ok(true);
        }
        if suppressed
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

/// Where a row lives, for error messages.
pub(crate) fn row_at(code: &str, idx: usize, d: &Detail, field: &str) -> String {
    format!(
        "account {code} line {} ({}): {field}",
        idx + 1,
        d.description
    )
}

pub(crate) fn eval_globals(b: &Budget) -> Result<BTreeMap<String, Dec>, CalcError> {
    let mut parsed = BTreeMap::new();
    for (name, f) in &b.globals {
        let at = || format!("global {name}");
        if !is_ident(name) {
            return Err(err(at(), ErrorKind::BadGlobalName));
        }
        let e = Expr::parse(&f.0).map_err(|e| err(at(), ErrorKind::Parse(e)))?;
        parsed.insert(name.as_str(), e);
    }
    let mut done = BTreeMap::new();
    let mut stack = Vec::new();
    for name in parsed.keys() {
        visit_global(name, &parsed, &mut done, &mut stack)?;
    }
    Ok(done)
}

/// Evaluates `name` after everything it depends on, depth-first.
fn visit_global<'a>(
    name: &'a str,
    parsed: &BTreeMap<&'a str, Expr>,
    done: &mut BTreeMap<String, Dec>,
    stack: &mut Vec<&'a str>,
) -> Result<(), CalcError> {
    if done.contains_key(name) {
        return Ok(());
    }
    if let Some(i) = stack.iter().position(|s| *s == name) {
        let mut cycle: Vec<String> = stack[i..].iter().map(|s| s.to_string()).collect();
        cycle.push(name.to_string());
        return Err(err(format!("global {name}"), ErrorKind::GlobalCycle(cycle)));
    }
    // Unknown names are reported by eval below, with this global as context.
    let Some((&key, expr)) = parsed.get_key_value(name) else {
        return Ok(());
    };
    let mut deps = BTreeSet::new();
    expr.vars(&mut deps);
    stack.push(key);
    for dep in &deps {
        if let Some((&dep, _)) = parsed.get_key_value(dep.as_str()) {
            visit_global(dep, parsed, done, stack)?;
        }
    }
    stack.pop();
    let v = expr
        .eval(&|n| done.get(n).copied())
        .map_err(|e| err(format!("global {name}"), ErrorKind::Eval(e)))?;
    done.insert(name.to_string(), v);
    Ok(())
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
pub(crate) fn eval_categories(ctx: &Ctx) -> Result<Vec<CategoryTotals>, CalcError> {
    let ovf = || overflow("budget totals");
    let mut categories = Vec::new();
    for cat in ctx.b.categories() {
        let mut accounts = Vec::with_capacity(cat.accounts.len());
        for acct in &cat.accounts {
            let mut rows = lines::eval_rows(ctx, acct)?;
            fringes::apply_fringes(ctx, acct, &mut rows)?;
            let details_total = sum(rows.iter().map(|r| r.total)).ok_or_else(ovf)?;
            let fringe_total = sum(rows.iter().map(|r| r.fringe_total)).ok_or_else(ovf)?;
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

/// Recalculates every total in `b`.
pub fn recalc(b: &Budget, opts: &RecalcOptions) -> Result<Report, CalcError> {
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
    let ctx = Ctx {
        b,
        globals: eval_globals(b)?,
        opts,
        unfiltered: false,
    };
    let categories = eval_categories(&ctx)?;
    let ts = topsheet::finish(&ctx, categories)?;

    let full_ctx = Ctx {
        unfiltered: true,
        ..ctx
    };
    let full = eval_categories(&full_ctx)?;
    let bd = breakdown::breakdowns(&full_ctx, &ts.categories, &full)?;

    Ok(Report {
        base_currency: b.base_currency.clone(),
        totals_currency: b
            .totals_currency
            .clone()
            .unwrap_or_else(|| b.base_currency.clone()),
        totals_rate,
        globals: full_ctx.globals,
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
    })
}
