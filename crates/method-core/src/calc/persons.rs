//! Stage 2b: person cutoffs across accounts (decisions.md, entry 79).
//!
//! Opt-in. A line or adjustment row with a [`Detail::person`] pays that
//! person; every counted row with the same person (matched by
//! [`person_key`]: spacing and case don't count), in any account, is one
//! earner for each percent fringe, so the fringe's cutoff applies once to
//! everything the person is paid. Stage 2 ([`super::fringes`]) leaves
//! percent fringes off those rows (a person row leaves its fringe range);
//! this pass, run after every account's rows are worked out, puts them on.
//!
//! The rule ([`person_shares`]): with no cutoff, or when the person's pay
//! carrying the fringe is no more than the cutoff, each row earns its own
//! percent, rounded on its own, as a row with no cutoff does. Past the
//! cutoff, the person earns the percent of the cutoff, rounded once, and
//! that amount is split over the rows in proportion to their pay, in whole
//! cents by largest remainder (ties to the lower row id). Nothing depends
//! on where the rows sit, so moving lines or accounts never moves a cent,
//! and the person's pieces always add up to exactly one cutoff's worth.
//! A cutoff of 0 or less earns nothing.
//!
//! A budget with no person on any row skips this pass, so its figures are
//! exactly what they were before persons existed.

use std::collections::{BTreeMap, BTreeSet};

use crate::calc::arith::{percent_of, Money, Num};
use crate::calc::fringes::factored_cutoff;
use crate::calc::{overflow, row_at, sum, CalcError, Ctx};
use crate::decimal::Dec;
use crate::model::{Account, Arithmetic, Budget, Detail, FringeKind, RowId};
use crate::report::{Report, RowTotals};
use crate::Cents;

pub use crate::model::person_key;

/// Whether any line or adjustment row of `b` has a person: the person pass
/// runs only then.
pub fn any(b: &Budget) -> bool {
    b.categories()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.details)
        .any(is_person_row)
}

/// A costed row with a person.
pub(crate) fn is_person_row(d: &Detail) -> bool {
    d.kind.is_costed() && d.person_name().is_some()
}

/// How a person's percent fringe was worked out.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Split {
    /// No cutoff, or the pay is within it: each row its own percent.
    Own,
    /// The cutoff is 0 or less: nothing is earned.
    Nothing,
    /// Past the cutoff: `amount` (the percent of the cutoff, `exact` before
    /// rounding) split over the rows by their share of `pay`.
    Shared {
        pay: Cents,
        cutoff: Cents,
        exact: Money,
        amount: Cents,
    },
}

/// One row's piece of a person's percent fringe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Share {
    pub amount: Cents,
    /// Under [`Split::Shared`]: the row took one of the cents left over
    /// after every row's share was rounded down.
    pub extra_cent: bool,
}

/// A person's percent fringe at `rate` with `cutoff` (already factored) on
/// `rows` (each row's id and total, the rows that carry the fringe), in
/// any order: what each row earns, in the order given. `None` on overflow.
pub(crate) fn person_shares(
    arith: Arithmetic,
    rate: Num,
    cutoff: Option<Cents>,
    rows: &[(RowId, Cents)],
) -> Option<(Split, Vec<Share>)> {
    let pay = rows.iter().try_fold(0, |a: Cents, r| a.checked_add(r.1))?;
    let own = |rows: &[(RowId, Cents)]| -> Option<Vec<Share>> {
        rows.iter()
            .map(|&(_, total)| {
                Some(Share {
                    amount: percent_of(arith, total, rate)?.to_cents()?,
                    extra_cent: false,
                })
            })
            .collect()
    };
    let cutoff = match cutoff {
        None => return Some((Split::Own, own(rows)?)),
        Some(c) if c <= 0 => {
            let none = rows.iter().map(|_| Share {
                amount: 0,
                extra_cent: false,
            });
            return Some((Split::Nothing, none.collect()));
        }
        Some(c) => c,
    };
    if pay <= cutoff {
        return Some((Split::Own, own(rows)?));
    }
    // pay > cutoff > 0 from here.
    let exact = percent_of(arith, cutoff, rate)?;
    let amount = exact.to_cents()?;
    let (a, t) = (i128::from(amount), i128::from(pay));
    let mut shares = Vec::with_capacity(rows.len());
    let mut given: i128 = 0;
    // (remainder, row id, position) for the leftover cents.
    let mut order: Vec<(i128, RowId, usize)> = Vec::with_capacity(rows.len());
    for (k, &(id, total)) in rows.iter().enumerate() {
        let product = a.checked_mul(i128::from(total))?;
        let floor = product.div_euclid(t);
        given = given.checked_add(floor)?;
        shares.push(Share {
            amount: Cents::try_from(floor).ok()?,
            extra_cent: false,
        });
        order.push((product.rem_euclid(t), id, k));
    }
    // The floors fall short of `amount` by less than one cent a row.
    let left = usize::try_from(a.checked_sub(given)?).ok()?;
    order.sort_by(|x, y| y.0.cmp(&x.0).then(x.1.cmp(&y.1)));
    for &(_, _, k) in order.iter().take(left) {
        let s = shares.get_mut(k)?;
        s.amount = s.amount.checked_add(1)?;
        s.extra_cent = true;
    }
    Some((
        Split::Shared {
            pay,
            cutoff,
            exact,
            amount,
        },
        shares,
    ))
}

/// The person rows of `accts` (budget order; `rows[k]` are `accts[k]`'s
/// evaluated rows) that earn, by person ([`person_key`]), as (account, row)
/// positions in budget order.
pub(crate) fn members(
    accts: &[&Account],
    rows: &[&mut [RowTotals]],
) -> BTreeMap<String, Vec<(usize, usize)>> {
    let mut people: BTreeMap<String, Vec<(usize, usize)>> = BTreeMap::new();
    for (k, (acct, rs)) in accts.iter().zip(rows).enumerate() {
        for (i, d) in acct.details.iter().enumerate() {
            let earns = rs.get(i).is_some_and(|r| r.included);
            if let (true, true, Some(p)) = (d.kind.is_costed(), earns, d.person_key()) {
                people.entry(p).or_default().push((k, i));
            }
        }
    }
    people
}

/// Puts each person's percent fringes on their rows across `accts`, and
/// works those rows' fringe totals out again. `rows[k]` are `accts[k]`'s
/// rows after stage 2; accounts without person rows may be left out of
/// both lists. A row's percent fringes from an earlier pass are replaced,
/// so this may run again on rows it already ran on.
pub(crate) fn apply(
    ctx: &Ctx,
    accts: &[&Account],
    rows: &mut [&mut [RowTotals]],
) -> Result<(), CalcError> {
    let people = members(accts, rows);
    for (person, at) in &people {
        let names: BTreeSet<&str> = at
            .iter()
            .flat_map(|&(k, i)| accts[k].details[i].fringes.iter().map(String::as_str))
            .collect();
        for name in names {
            let Some((key, f)) = ctx.b.fringes.get_key_value(name) else {
                continue;
            };
            let FringeKind::Percent { cutoff } = f.kind else {
                continue;
            };
            let rate = ctx.eval(&f.rate, Dec::ZERO, &|| format!("fringe {key}: rate"))?;
            let ovf = || overflow(format!("person {person}: fringe {name}"));
            let cutoff = factored_cutoff(ctx.arith, cutoff, ctx.b.cutoff_factor).ok_or_else(ovf)?;
            let lines: Vec<(usize, usize)> = at
                .iter()
                .copied()
                .filter(|&(k, i)| accts[k].details[i].fringes.contains(name))
                .collect();
            let input: Vec<(RowId, Cents)> = lines
                .iter()
                .map(|&(k, i)| (accts[k].details[i].id, rows[k][i].total))
                .collect();
            let (_, shares) = person_shares(ctx.arith, rate, cutoff, &input).ok_or_else(ovf)?;
            for (&(k, i), s) in lines.iter().zip(shares) {
                rows[k][i].fringes.insert(name.to_string(), s.amount);
            }
        }
        for &(k, i) in at {
            let r = &mut rows[k][i];
            r.fringe_total = sum(r.fringes.values().copied()).ok_or_else(|| {
                overflow(row_at(&accts[k].code, i, &accts[k].details[i], "fringes"))
            })?;
        }
    }
    Ok(())
}

/// One percent fringe of a person, as [`person_totals`] reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersonFringe {
    pub fringe: String,
    /// The cutoff as applied (times the budget's cutoff factor); `None`
    /// for a fringe without one.
    pub cutoff: Option<Cents>,
    /// What the person is paid on the counted rows carrying the fringe.
    pub pay: Cents,
    /// The fringe the person earns across those rows.
    pub amount: Cents,
    /// Whether the pay is past the cutoff (so the person earns the percent
    /// of the cutoff, not of the pay).
    pub reached: bool,
}

/// A person's running totals across the budget.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersonTotals {
    /// The name as the first of the person's rows in budget order spells
    /// it (trimmed).
    pub name: String,
    /// Every line and adjustment row with the person, counted or not, in
    /// budget order.
    pub rows: Vec<RowId>,
    /// The accounts those rows are in, in budget order, each once.
    pub accounts: Vec<String>,
    /// What the counted rows come to.
    pub pay: Cents,
    /// Every fringe the counted rows earn (percent and flat).
    pub fringe_total: Cents,
    /// Each percent fringe the rows carry, by name.
    pub fringes: Vec<PersonFringe>,
}

/// Every person in `b`, by [`person_key`], with their pay and each percent fringe's
/// pay against its cutoff, read from `r` (which must come from
/// recalculating `b`). Empty when no row has a person. `None` on overflow
/// or when `r` doesn't match `b`.
pub fn person_totals(b: &Budget, r: &Report) -> Option<BTreeMap<String, PersonTotals>> {
    let mut out: BTreeMap<String, PersonTotals> = BTreeMap::new();
    let mut carried: BTreeMap<String, BTreeMap<String, (Cents, Cents)>> = BTreeMap::new();
    for (cat, ct) in b.categories().zip(&r.categories) {
        for (acct, at) in cat.accounts.iter().zip(&ct.accounts) {
            for (d, row) in acct.details.iter().zip(&at.rows) {
                let (true, Some(shown)) = (d.kind.is_costed(), d.person_name()) else {
                    continue;
                };
                let p = person_key(shown);
                let t = out.entry(p.clone()).or_insert_with(|| PersonTotals {
                    name: shown.to_string(),
                    rows: Vec::new(),
                    accounts: Vec::new(),
                    pay: 0,
                    fringe_total: 0,
                    fringes: Vec::new(),
                });
                t.rows.push(d.id);
                if !t.accounts.contains(&acct.code) {
                    t.accounts.push(acct.code.clone());
                }
                if !row.included {
                    continue;
                }
                t.pay = t.pay.checked_add(row.total)?;
                t.fringe_total = t.fringe_total.checked_add(row.fringe_total)?;
                let fr = carried.entry(p).or_default();
                for name in &d.fringes {
                    let Some(f) = b.fringes.get(name) else {
                        continue;
                    };
                    if !matches!(f.kind, FringeKind::Percent { .. }) {
                        continue;
                    }
                    let slot = fr.entry(name.clone()).or_insert((0, 0));
                    slot.0 = slot.0.checked_add(row.total)?;
                    slot.1 = slot
                        .1
                        .checked_add(row.fringes.get(name).copied().unwrap_or(0))?;
                }
            }
        }
    }
    for (p, fr) in carried {
        let Some(t) = out.get_mut(&p) else {
            continue;
        };
        for (name, (pay, amount)) in fr {
            let cutoff = match b.fringes.get(&name).map(|f| &f.kind) {
                Some(FringeKind::Percent { cutoff }) => {
                    factored_cutoff(b.arithmetic, *cutoff, b.cutoff_factor)?
                }
                _ => None,
            };
            t.fringes.push(PersonFringe {
                fringe: name,
                cutoff,
                pay,
                amount,
                reached: cutoff.is_some_and(|c| pay > c),
            });
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cents(rows: &[(RowId, Cents)], cutoff: Option<Cents>, rate: &str) -> Vec<Cents> {
        let rate = Num::of(Arithmetic::Exact, rate.parse().unwrap());
        person_shares(Arithmetic::Exact, rate, cutoff, rows)
            .unwrap()
            .1
            .into_iter()
            .map(|s| s.amount)
            .collect()
    }

    #[test]
    fn within_the_cutoff_each_row_earns_its_own_percent() {
        assert_eq!(
            cents(&[(1, 100_000), (2, 200_000)], Some(700_000), "10"),
            [10_000, 20_000]
        );
        assert_eq!(
            cents(&[(1, 100_000), (2, 200_000)], None, "10"),
            [10_000, 20_000]
        );
    }

    #[test]
    fn past_the_cutoff_the_cutoff_is_split_by_pay() {
        // 700.00 of SUTA on 3,000 / 5,000 / 2,000.
        assert_eq!(
            cents(
                &[(1, 300_000), (2, 500_000), (3, 200_000)],
                Some(700_000),
                "10"
            ),
            [21_000, 35_000, 14_000]
        );
        // 250.00 three ways: the leftover cent goes to the lowest row id,
        // wherever it is listed.
        assert_eq!(
            cents(
                &[(9, 100_000), (4, 100_000), (7, 100_000)],
                Some(200_000),
                "12.5"
            ),
            [8_333, 8_334, 8_333]
        );
    }

    #[test]
    fn a_cutoff_of_nothing_earns_nothing() {
        assert_eq!(cents(&[(1, 100), (2, -50)], Some(0), "10"), [0, 0]);
    }

    #[test]
    fn a_credit_shares_the_cutoff_too() {
        // Pay 2,500.00 past a 1,000.00 cutoff: 100.00 split 3,000 / -500.
        assert_eq!(
            cents(&[(1, 300_000), (2, -50_000)], Some(100_000), "10"),
            [12_000, -2_000]
        );
    }
}
