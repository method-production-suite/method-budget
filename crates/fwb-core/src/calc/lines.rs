//! Stage 1: evaluate every row of an account.

use std::collections::BTreeMap;

use crate::calc::{err, overflow, row_at, CalcError, Ctx, ErrorKind};
use crate::decimal::{mul_cents, Dec};
use crate::model::{Account, RowKind};
use crate::report::RowTotals;
use crate::Cents;

/// One [`RowTotals`] per detail, in order, with `fringes` left empty.
pub(crate) fn eval_rows(ctx: &Ctx, acct: &Account) -> Result<Vec<RowTotals>, CalcError> {
    let base = &ctx.b.base_currency;
    let mut out = Vec::with_capacity(acct.details.len());
    // Lines since the previous adjustment row (subtotal rows don't reset it).
    let mut since_adjustment: Cents = 0;
    // Lines and adjustments since the previous subtotal row.
    let mut since_subtotal: Cents = 0;

    for (i, d) in acct.details.iter().enumerate() {
        let at = |field: &'static str| move || row_at(&acct.code, i, d, field);
        let ovf = || overflow(row_at(&acct.code, i, d, "total"));

        if d.kind == RowKind::Subtotal {
            out.push(RowTotals {
                id: d.id,
                kind: d.kind,
                included: true,
                quantity: Dec::ZERO,
                currency: base.clone(),
                local_total: 0,
                total: 0,
                subtotal: Some(since_subtotal),
                fringes: BTreeMap::new(),
                fringe_total: 0,
            });
            since_subtotal = 0;
            continue;
        }
        if d.kind == RowKind::Separator {
            // A drawn rule: no cost, and it resets neither window.
            out.push(RowTotals {
                id: d.id,
                kind: d.kind,
                included: true,
                quantity: Dec::ZERO,
                currency: base.clone(),
                local_total: 0,
                total: 0,
                subtotal: None,
                fringes: BTreeMap::new(),
                fringe_total: 0,
            });
            continue;
        }

        let amount = ctx.eval(&d.amount, Dec::ZERO, &at("amount"))?;
        let x = ctx.eval(&d.x, Dec::ONE, &at("X"))?;
        let x4 = ctx.eval(&d.x4, Dec::ONE, &at("4X"))?;
        let quantity = amount
            .checked_mul(x)
            .and_then(|q| q.checked_mul(x4))
            .ok_or_else(ovf)?;
        let included = ctx.includes(d, &at("groups"))?;

        let (currency, local_total, total) = match d.kind {
            RowKind::Adjustment => {
                let t = mul_cents(since_adjustment, quantity).ok_or_else(ovf)?;
                (base.clone(), t, t)
            }
            _ => {
                let rate = ctx.eval(&d.rate, Dec::ZERO, &at("rate"))?;
                let local = quantity
                    .checked_mul(rate)
                    .and_then(Dec::to_cents)
                    .ok_or_else(ovf)?;
                match &d.currency {
                    Some(c) if c != base => {
                        let cur = ctx.b.currencies.get(c).ok_or_else(|| {
                            err(at("currency")(), ErrorKind::UnknownCurrency(c.clone()))
                        })?;
                        (
                            c.clone(),
                            local,
                            mul_cents(local, cur.rate).ok_or_else(ovf)?,
                        )
                    }
                    _ => (base.clone(), local, local),
                }
            }
        };
        let (local_total, total) = if included {
            (local_total, total)
        } else {
            (0, 0)
        };

        // A left-out adjustment behaves as if deleted, so it doesn't reset the window.
        if d.kind == RowKind::Adjustment && included {
            since_adjustment = 0;
        } else {
            since_adjustment = since_adjustment.checked_add(total).ok_or_else(ovf)?;
        }
        since_subtotal = since_subtotal.checked_add(total).ok_or_else(ovf)?;

        out.push(RowTotals {
            id: d.id,
            kind: d.kind,
            included,
            quantity,
            currency,
            local_total,
            total,
            subtotal: None,
            fringes: BTreeMap::new(),
            fringe_total: 0,
        });
    }
    Ok(out)
}
