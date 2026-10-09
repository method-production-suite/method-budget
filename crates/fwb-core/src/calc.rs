//! Recalculation: a pure function from [`Budget`] to [`Report`].
//!
//! Same budget and options in, identical report out. Every money value is
//! rounded to cents exactly once, at the detail line (see docs/decisions.md);
//! everything above a line is plain integer addition, so rollups always agree.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::decimal::{mul_cents, percent_of, Dec};
use crate::expr::{is_ident, EvalError, Expr, ParseError};
use crate::model::{Budget, Detail, Formula, Fringe};
use crate::Cents;

/// Which details count toward totals.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecalcOptions {
    /// Lines tagged with any of these groups are left out.
    pub exclude_groups: BTreeSet<String>,
    /// When set, only lines tagged with at least one of these groups count.
    pub only_groups: Option<BTreeSet<String>>,
}

impl RecalcOptions {
    fn includes(&self, d: &Detail) -> bool {
        if d.groups.iter().any(|g| self.exclude_groups.contains(g)) {
            return false;
        }
        match &self.only_groups {
            Some(only) => d.groups.iter().any(|g| only.contains(g)),
            None => true,
        }
    }
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
            ErrorKind::Overflow => f.write_str("number too large"),
        }
    }
}

impl std::error::Error for CalcError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    pub base_currency: String,
    pub globals: BTreeMap<String, Dec>,
    pub categories: Vec<CategoryTotals>,
    /// Budget-wide total per fringe name.
    pub fringe_totals: BTreeMap<String, Cents>,
    /// All categories: lines plus fringes.
    pub subtotal: Cents,
    pub contractual: Vec<ChargeTotal>,
    pub grand_total: Cents,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CategoryTotals {
    pub name: String,
    pub accounts: Vec<AccountTotals>,
    pub details_total: Cents,
    pub fringe_total: Cents,
    pub total: Cents,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountTotals {
    pub code: String,
    pub name: String,
    pub details: Vec<DetailTotals>,
    pub details_total: Cents,
    pub fringe_total: Cents,
    pub total: Cents,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DetailTotals {
    pub description: String,
    /// False when group filtering left this line out; its money fields are then 0.
    pub included: bool,
    /// amount x multiplier.
    pub quantity: Dec,
    pub currency: String,
    /// In `currency`.
    pub local_total: Cents,
    /// In the base currency.
    pub total: Cents,
    pub fringes: BTreeMap<String, Cents>,
    pub fringe_total: Cents,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChargeTotal {
    pub name: String,
    /// What the percentage was applied to.
    pub base: Cents,
    pub amount: Cents,
}

fn err(at: impl Into<String>, kind: ErrorKind) -> CalcError {
    CalcError {
        at: at.into(),
        kind,
    }
}

fn sum(it: impl IntoIterator<Item = Cents>) -> Option<Cents> {
    it.into_iter().try_fold(0, Cents::checked_add)
}

fn eval_globals(b: &Budget) -> Result<BTreeMap<String, Dec>, CalcError> {
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

fn calc_detail(
    b: &Budget,
    globals: &BTreeMap<String, Dec>,
    opts: &RecalcOptions,
    code: &str,
    idx: usize,
    d: &Detail,
) -> Result<DetailTotals, CalcError> {
    let at = |field: &str| {
        format!(
            "account {code} line {} ({}): {field}",
            idx + 1,
            d.description
        )
    };
    let ev = |f: &Formula, field: &str| -> Result<Dec, CalcError> {
        Expr::parse(&f.0)
            .map_err(|e| err(at(field), ErrorKind::Parse(e)))?
            .eval(&|n| globals.get(n).copied())
            .map_err(|e| err(at(field), ErrorKind::Eval(e)))
    };
    let overflow = || err(at("total"), ErrorKind::Overflow);

    let amount = ev(&d.amount, "amount")?;
    let multiplier = ev(&d.multiplier, "multiplier")?;
    let rate = ev(&d.rate, "rate")?;
    let quantity = amount.checked_mul(multiplier).ok_or_else(overflow)?;
    let local_total = quantity
        .checked_mul(rate)
        .and_then(Dec::to_cents)
        .ok_or_else(overflow)?;

    let currency = d
        .currency
        .clone()
        .unwrap_or_else(|| b.base_currency.clone());
    let total = if currency == b.base_currency {
        local_total
    } else {
        let fx = b
            .exchange_rates
            .get(&currency)
            .ok_or_else(|| err(at("currency"), ErrorKind::UnknownCurrency(currency.clone())))?;
        mul_cents(local_total, *fx).ok_or_else(overflow)?
    };

    let mut fringes = BTreeMap::new();
    for name in &d.fringes {
        let f = b
            .fringes
            .get(name)
            .ok_or_else(|| err(at("fringes"), ErrorKind::UnknownFringe(name.clone())))?;
        let amt = match *f {
            Fringe::Percent { percent, wage_cap } => {
                percent_of(wage_cap.map_or(total, |cap| total.min(cap)), percent)
            }
            Fringe::FlatPerUnit { per_unit, unit_cap } => {
                mul_cents(per_unit, unit_cap.map_or(quantity, |cap| quantity.min(cap)))
            }
        };
        fringes.insert(name.clone(), amt.ok_or_else(overflow)?);
    }

    if !opts.includes(d) {
        return Ok(DetailTotals {
            description: d.description.clone(),
            included: false,
            quantity,
            currency,
            local_total: 0,
            total: 0,
            fringes: BTreeMap::new(),
            fringe_total: 0,
        });
    }
    Ok(DetailTotals {
        description: d.description.clone(),
        included: true,
        quantity,
        currency,
        local_total,
        total,
        fringe_total: sum(fringes.values().copied()).ok_or_else(overflow)?,
        fringes,
    })
}

/// Recalculates every total in `b`.
pub fn recalc(b: &Budget, opts: &RecalcOptions) -> Result<Report, CalcError> {
    let globals = eval_globals(b)?;
    let overflow = || err("budget totals", ErrorKind::Overflow);

    let mut categories = Vec::with_capacity(b.categories.len());
    for cat in &b.categories {
        let mut accounts = Vec::with_capacity(cat.accounts.len());
        for acct in &cat.accounts {
            let details = acct
                .details
                .iter()
                .enumerate()
                .map(|(i, d)| calc_detail(b, &globals, opts, &acct.code, i, d))
                .collect::<Result<Vec<_>, _>>()?;
            let details_total = sum(details.iter().map(|d| d.total)).ok_or_else(overflow)?;
            let fringe_total = sum(details.iter().map(|d| d.fringe_total)).ok_or_else(overflow)?;
            accounts.push(AccountTotals {
                code: acct.code.clone(),
                name: acct.name.clone(),
                details,
                details_total,
                fringe_total,
                total: details_total
                    .checked_add(fringe_total)
                    .ok_or_else(overflow)?,
            });
        }
        let details_total = sum(accounts.iter().map(|a| a.details_total)).ok_or_else(overflow)?;
        let fringe_total = sum(accounts.iter().map(|a| a.fringe_total)).ok_or_else(overflow)?;
        categories.push(CategoryTotals {
            name: cat.name.clone(),
            accounts,
            details_total,
            fringe_total,
            total: details_total
                .checked_add(fringe_total)
                .ok_or_else(overflow)?,
        });
    }

    let all_accounts = || categories.iter().flat_map(|c| &c.accounts);

    let mut fringe_totals: BTreeMap<String, Cents> = BTreeMap::new();
    for d in all_accounts().flat_map(|a| &a.details) {
        for (name, amt) in &d.fringes {
            let slot = fringe_totals.entry(name.clone()).or_insert(0);
            *slot = slot.checked_add(*amt).ok_or_else(overflow)?;
        }
    }

    let subtotal = sum(categories.iter().map(|c| c.total)).ok_or_else(overflow)?;

    let mut contractual = Vec::with_capacity(b.contractual_charges.len());
    for c in &b.contractual_charges {
        let base = sum(all_accounts()
            .filter(|a| !c.excluded_accounts.contains(&a.code))
            .map(|a| {
                if c.include_fringes {
                    a.total
                } else {
                    a.details_total
                }
            }))
        .ok_or_else(overflow)?;
        let amount = percent_of(base, c.percent).ok_or_else(|| {
            err(
                format!("contractual charge {}", c.name),
                ErrorKind::Overflow,
            )
        })?;
        contractual.push(ChargeTotal {
            name: c.name.clone(),
            base,
            amount,
        });
    }

    let charges = sum(contractual.iter().map(|c| c.amount)).ok_or_else(overflow)?;
    Ok(Report {
        base_currency: b.base_currency.clone(),
        globals,
        categories,
        fringe_totals,
        subtotal,
        contractual,
        grand_total: subtotal.checked_add(charges).ok_or_else(overflow)?,
    })
}
