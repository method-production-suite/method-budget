//! Totals shown in the totals currency (decisions.md, entry 59).
//!
//! When a budget shows its totals in another currency, each account's
//! total is divided by the totals currency's rate and rounded to cents (a
//! category's fringe account is one more account), a fringe row is
//! converted fringe by fringe, and each charge and credit on its own.
//! Categories, levels, ATL, BTL and the grand and net totals are integer
//! sums of those cents, as the reference product adds them, so a total
//! shown in the other currency can differ by a cent from the base-currency
//! total divided by the rate (ZTEST08 V08-1: fringes 225.92, not 677.78 / 3
//! = 225.93; grand 49,253.40, not 49,253.41). Accounts are where the
//! conversion starts, not lines: line by line, V08-2's category 1100 would
//! be 118,731.60 in GBP, where the reference shows 118,731.57 (its total
//! divided by 1.23457). Rounding follows the budget's arithmetic. Display
//! only: nothing here changes a base-currency figure.

use std::collections::BTreeMap;

use crate::calc::arith::mmb_cents;
use crate::calc::topsheet::{dropped_from, Topsheet};
use crate::calc::{overflow, sum, CalcError, Ctx};
use crate::decimal::{Dec, Exact};
use crate::model::{Arithmetic, FringePosting, TopsheetRow};
use crate::report::{DisplayTotals, RowTotals};
use crate::Cents;

/// `c` base-currency cents in a currency of `rate` base units per unit,
/// rounded to cents once by `arith`. `None` on overflow or a zero rate.
pub(crate) fn convert(arith: Arithmetic, c: Cents, rate: Dec) -> Option<Cents> {
    match arith {
        Arithmetic::Exact => Exact::cents(c).div_to_cents(rate),
        Arithmetic::Mmb => {
            let r = rate.to_f64();
            if r == 0.0 {
                return None;
            }
            mmb_cents(c as f64 / 100.0 / r)
        }
    }
}

/// Fringe dollars by fringe name over `rows`.
fn add_fringes<'a>(into: &mut BTreeMap<&'a str, Cents>, rows: &'a [RowTotals]) -> Option<()> {
    for r in rows {
        for (name, c) in &r.fringes {
            let e = into.entry(name.as_str()).or_insert(0);
            *e = e.checked_add(*c)?;
        }
    }
    Some(())
}

/// A fringe row in the totals currency: each fringe converted on its own,
/// then added up.
fn fringe_row(arith: Arithmetic, by_name: &BTreeMap<&str, Cents>, rate: Dec) -> Option<Cents> {
    let mut total: Cents = 0;
    for c in by_name.values() {
        total = total.checked_add(convert(arith, *c, rate)?)?;
    }
    Some(total)
}

/// Every topsheet figure of `ts` in the totals currency of `rate` (base
/// units per unit of it). `ctx` is the counted context `ts` was worked out
/// in.
pub(crate) fn display_totals(
    ctx: &Ctx,
    ts: &Topsheet,
    rate: Dec,
) -> Result<DisplayTotals, CalcError> {
    let arith = ctx.arith;
    let b = ctx.b;
    let ovf = || overflow("totals in the totals currency");
    let conv = |c: Cents| convert(arith, c, rate).ok_or_else(ovf);
    let posting = &b.fringe_posting;

    // Each account (with its fringe line, when fringes post by account),
    // and a category's fringe account as one more account.
    let mut categories = Vec::with_capacity(ts.categories.len());
    for ct in &ts.categories {
        let mut cat_total: Cents = 0;
        for at in &ct.accounts {
            cat_total = cat_total.checked_add(conv(at.total)?).ok_or_else(ovf)?;
        }
        if let Some((_, f)) = &ct.fringe_account {
            cat_total = cat_total.checked_add(conv(*f)?).ok_or_else(ovf)?;
        }
        categories.push(cat_total);
    }

    // Fringes by name per category, for level and budget fringe rows.
    let fringes_of = |range: std::ops::Range<usize>| -> Result<BTreeMap<&str, Cents>, CalcError> {
        let mut by_name = BTreeMap::new();
        for ct in &ts.categories[range] {
            for at in &ct.accounts {
                add_fringes(&mut by_name, &at.rows).ok_or_else(ovf)?;
            }
        }
        Ok(by_name)
    };

    let dropped = dropped_from(b);
    let mut levels = Vec::with_capacity(ts.levels.len());
    for l in &ts.levels {
        let mut total = sum(categories[l.start..l.end].iter().copied()).ok_or_else(ovf)?;
        let posted =
            *posting == FringePosting::ProductionLevel && dropped.is_none_or(|d| l.start < d);
        if posted {
            let by_name = fringes_of(l.start..l.end)?;
            let f = fringe_row(arith, &by_name, rate).ok_or_else(ovf)?;
            total = total.checked_add(f).ok_or_else(ovf)?;
        }
        levels.push(total);
    }
    let budget_fringes = if *posting == FringePosting::Budget {
        let by_name = fringes_of(0..ts.categories.len())?;
        fringe_row(arith, &by_name, rate).ok_or_else(ovf)?
    } else {
        0
    };

    // As the base-currency totals count them (decisions.md, entry 56).
    let closed = b
        .topsheet
        .iter()
        .filter(|r| matches!(r, TopsheetRow::ProductionTotal(_)))
        .count()
        .min(levels.len());
    let split = usize::from(closed > 0);
    let total_atl = sum(levels[..split].iter().copied()).ok_or_else(ovf)?;
    let total_btl = sum(levels[split..closed].iter().copied()).ok_or_else(ovf)?;
    let atl_btl = total_atl.checked_add(total_btl).ok_or_else(ovf)?;

    let charges = ts
        .charges
        .iter()
        .map(|c| conv(c.amount))
        .collect::<Result<Vec<_>, _>>()?;
    let grand_total = sum(levels.iter().copied())
        .and_then(|t| t.checked_add(budget_fringes))
        .and_then(|t| t.checked_add(sum(charges.iter().copied())?))
        .ok_or_else(ovf)?;
    let credits = ts
        .credits
        .iter()
        .map(|c| conv(c.amount))
        .collect::<Result<Vec<_>, _>>()?;
    let credits_total = sum(credits.iter().copied()).ok_or_else(ovf)?;
    let net_total = grand_total.checked_sub(credits_total).ok_or_else(ovf)?;

    Ok(DisplayTotals {
        categories,
        levels,
        budget_fringes,
        total_atl,
        total_btl,
        atl_btl,
        charges,
        grand_total,
        credits,
        credits_total,
        net_total,
    })
}
