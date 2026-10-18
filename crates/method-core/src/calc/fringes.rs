//! Stage 2: fringe dollars per row.
//!
//! Included lines and adjustment rows earn fringes (decisions.md, entry 56).
//! A percent fringe's cutoff, times the budget's cutoff factor
//! ([`factored_cutoff`]), applies per earner: a single row, or a fringe
//! range (consecutive rows sharing `fringe_range: Some(n)`; subtotal,
//! separator and uncounted rows don't break a run). Within an earner the
//! cutoff is used up in row order, as Movie Magic does: each row carrying
//! the fringe takes the percent of its total, or of the cutoff still left
//! when that is less, rounded on its own ([`percent_step`]). A flat fringe
//! is charged per row: its rate times the row's count, which [`flat_count`]
//! works out (decisions.md, entry 54). Rows with a person get their percent
//! fringes later, across accounts ([`super::persons`]).

use std::collections::{BTreeMap, BTreeSet};

use crate::calc::arith::{mmb_cents, percent_of, Money, Num};
use crate::calc::{err, overflow, row_at, sum, CalcError, Ctx, ErrorKind};
use crate::decimal::{Dec, Exact};
use crate::model::{
    Account, Arithmetic, CutoffFactor, Detail, FlatBasis, Fringe, FringeKind, UnitDef,
};
use crate::report::RowTotals;
use crate::Cents;

/// Fills `fringes` and `fringe_total` on each row of `acct` (`rows[i]` is
/// `acct.details[i]`, already evaluated by stage 1).
pub(crate) fn apply_fringes(
    ctx: &Ctx,
    acct: &Account,
    rows: &mut [RowTotals],
) -> Result<(), CalcError> {
    let details = &acct.details;
    for r in rows.iter_mut() {
        r.fringes.clear();
        r.fringe_total = 0;
        r.fringe_full_total = 0;
    }

    // Check every name (excluded rows too) and evaluate each rate once.
    let mut rates: BTreeMap<&str, (&Fringe, Num)> = BTreeMap::new();
    for (i, d) in details.iter().enumerate() {
        if !d.kind.is_costed() {
            continue;
        }
        for name in &d.fringes {
            if rates.contains_key(name.as_str()) {
                continue;
            }
            let (key, f) = ctx.b.fringes.get_key_value(name).ok_or_else(|| {
                err(
                    row_at(&acct.code, i, d, "fringes"),
                    ErrorKind::UnknownFringe(name.clone()),
                )
            })?;
            let rate = ctx.eval(&f.rate, Dec::ZERO, &|| format!("fringe {key}: rate"))?;
            rates.insert(key, (f, rate));
        }
    }
    let earns: Vec<bool> = details
        .iter()
        .zip(rows.iter())
        .map(|(d, r)| d.kind.is_costed() && r.included)
        .collect();

    // Percent fringes, per earner. Only the fringes the earner's lines carry
    // can give it anything, so those are the ones visited, in name order (the
    // same order, and so the same first error, as visiting every fringe).
    let mut carried: BTreeSet<&str> = BTreeSet::new();
    let mut names: Vec<&str> = Vec::new();
    let mut lines: Vec<usize> = Vec::new();
    for earner in earners(details, rows) {
        // A lone line (the usual earner) carries its own, already sorted, set.
        let single = match earner[..] {
            [i] if earns[i] => Some(i),
            _ => None,
        };
        names.clear();
        match single {
            Some(i) => names.extend(details[i].fringes.iter().map(String::as_str)),
            None => {
                carried.clear();
                for &i in &earner {
                    if earns[i] {
                        carried.extend(details[i].fringes.iter().map(String::as_str));
                    }
                }
                names.extend(carried.iter().copied());
            }
        }
        for &name in &names {
            let Some(&(f, rate)) = rates.get(name) else {
                continue;
            };
            let FringeKind::Percent { cutoff } = f.kind else {
                continue;
            };
            let cutoff = factored_cutoff(ctx.arith, cutoff, ctx.b.cutoff_factor)
                .ok_or_else(|| overflow(format!("fringe {name}: cutoff")))?;
            lines.clear();
            match single {
                Some(i) => lines.push(i),
                None => lines.extend(
                    earner
                        .iter()
                        .copied()
                        .filter(|&i| earns[i] && details[i].fringes.contains(name)),
                ),
            }
            // The cutoff is used up in row order; each row rounds on its own.
            let mut left = cutoff;
            for &i in &lines {
                let amount = percent_step(ctx.arith, rate, &mut left, rows[i].total)
                    .and_then(|(_, m)| m.to_cents())
                    .ok_or_else(|| {
                        overflow(row_at(
                            &acct.code,
                            i,
                            &details[i],
                            &format!("fringe {name}"),
                        ))
                    })?;
                rows[i].fringes.insert(name.to_string(), amount);
            }
        }
    }

    // Flat fringes, per row.
    for (i, d) in details.iter().enumerate() {
        if !earns[i] {
            continue;
        }
        let mut factors: Option<Factors> = None;
        for name in &d.fringes {
            let Some(&(f, rate)) = rates.get(name.as_str()) else {
                continue;
            };
            if !matches!(f.kind, FringeKind::Flat { .. }) {
                continue;
            }
            let v = match factors {
                Some(v) => v,
                None => *factors.insert(Factors::of(ctx, &acct.code, i, d)?),
            };
            let amount = flat_amount(ctx, &f.kind, d, v, rate)
                .ok_or_else(|| overflow(row_at(&acct.code, i, d, &format!("fringe {name}"))))?;
            rows[i].fringes.insert(name.clone(), amount);
        }
    }

    for (i, d) in details.iter().enumerate() {
        let r = &mut rows[i];
        r.fringe_total = sum(r.fringes.values().copied())
            .ok_or_else(|| overflow(row_at(&acct.code, i, d, "fringes")))?;
        if earns[i] {
            r.fringe_full_total = full_fringes(ctx.arith, d, r, &rates)
                .ok_or_else(|| overflow(row_at(&acct.code, i, d, "fringes")))?;
        }
    }
    Ok(())
}

/// What row `r` (of line `d`, which earns) would earn with no cutoff and no
/// range (decisions.md, entry 61): each percent fringe its percent of the
/// row's total, rounded on its own, plus the flat fringes it earned (they
/// never have a cutoff or a range). `None` on overflow.
fn full_fringes(
    arith: Arithmetic,
    d: &Detail,
    r: &RowTotals,
    rates: &BTreeMap<&str, (&Fringe, Num)>,
) -> Option<Cents> {
    let mut total: Cents = 0;
    for name in &d.fringes {
        let Some(&(f, rate)) = rates.get(name.as_str()) else {
            continue;
        };
        let amount = match f.kind {
            FringeKind::Percent { .. } => percent_of(arith, r.total, rate)?.to_cents()?,
            FringeKind::Flat { .. } => r.fringes.get(name).copied().unwrap_or(0),
        };
        total = total.checked_add(amount)?;
    }
    Some(total)
}

/// Percent-fringe earners as lists of row indexes (lines and adjustment
/// rows), in row order. Rows outside a range are their own earner. A range
/// is a maximal run of rows with the same `fringe_range: Some(n)`. Subtotal
/// and separator rows, and rows that don't count (suppressed or filtered
/// out), are skipped, so they never break a run; any other row with a
/// different value does. Skipping uncounted rows keeps "excluding a group
/// equals deleting its lines" true (decisions.md 4). Rows with a person are
/// skipped the same way: they earn their percent fringes with the person,
/// across accounts ([`super::persons`], decisions.md, entry 79).
pub(super) fn earners(details: &[Detail], rows: &[RowTotals]) -> Vec<Vec<usize>> {
    let mut out = Vec::new();
    let mut run: Option<(u32, Vec<usize>)> = None;
    for (i, d) in details.iter().enumerate() {
        if !d.kind.is_costed() || !rows[i].included || super::persons::is_person_row(d) {
            continue;
        }
        if let (Some((m, members)), Some(n)) = (&mut run, d.fringe_range) {
            if *m == n {
                members.push(i);
                continue;
            }
        }
        out.extend(run.take().map(|(_, members)| members));
        match d.fringe_range {
            Some(n) => run = Some((n, vec![i])),
            None => out.push(vec![i]),
        }
    }
    out.extend(run.map(|(_, members)| members));
    out
}

/// A percent fringe's `cutoff` as the budget applies it: times the
/// budget's cutoff factor (decisions.md, entry 60), rounded to cents by the
/// budget's arithmetic (2,222.22 x 1.5 is 3,333.33). At 1.0, or with no
/// cutoff, it is the cutoff as typed. `None` on overflow.
pub(crate) fn factored_cutoff(
    arith: Arithmetic,
    cutoff: Option<Cents>,
    factor: CutoffFactor,
) -> Option<Option<Cents>> {
    match cutoff {
        Some(c) if factor != CutoffFactor::One => Money::cents(arith, c)
            .checked_mul(Num::of(arith, factor.value()))?
            .to_cents()
            .map(Some),
        _ => Some(cutoff),
    }
}

/// One row of a percent fringe's earner, taken in row order (decisions.md,
/// entry 56, as Movie Magic works a range): the part of the row's `total`
/// the fringe applies to, and the fringe on it before rounding. `left` is
/// the earner's cutoff not used yet (`None`: no cutoff); this row uses its
/// whole total of it. Once nothing is left a row takes nothing, so a credit
/// after the cutoff is used earns 0, while a credit on its own (with the
/// cutoff all there) earns its percent. `None` on overflow.
pub(crate) fn percent_step(
    arith: Arithmetic,
    rate: Num,
    left: &mut Option<Cents>,
    total: Cents,
) -> Option<(Cents, Money)> {
    let taken = match *left {
        None => total,
        Some(l) => {
            *left = Some(l.saturating_sub(total));
            if l <= 0 {
                0
            } else {
                total.min(l)
            }
        }
    };
    Some((taken, percent_of(arith, taken, rate)?))
}

/// A line's evaluated Amount, X and 4X, and its quantity (`amount x X x
/// 4X`, as stage 1 works it out), in the budget's arithmetic.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Factors {
    pub amount: Num,
    pub x: Num,
    pub x4: Num,
    pub quantity: Num,
}

impl Factors {
    /// Evaluated again for the line at `i` of account `code`; stage 1 has
    /// already evaluated them, so this doesn't fail in practice.
    pub(crate) fn of(ctx: &Ctx, code: &str, i: usize, d: &Detail) -> Result<Factors, CalcError> {
        let at = |field: &'static str| move || row_at(code, i, d, field);
        let amount = ctx.eval(&d.amount, Dec::ZERO, &at("amount"))?;
        let x = ctx.eval(&d.x, Dec::ONE, &at("X"))?;
        let x4 = ctx.eval(&d.x4, Dec::ONE, &at("4X"))?;
        Ok(Factors {
            amount,
            x,
            x4,
            quantity: Factors::quantity(amount, x, x4).ok_or_else(|| overflow(at("total")()))?,
        })
    }

    /// `amount x X x 4X`, as stage 1 multiplies them. `None` on overflow.
    pub(crate) fn quantity(amount: Num, x: Num, x4: Num) -> Option<Num> {
        amount.checked_mul(x)?.checked_mul(x4)
    }
}

/// A flat fringe (`kind`) at `rate` on one line: `rate` times the line's
/// count, rounded to cents once. `None` on overflow, or when `kind` is not
/// flat.
pub(crate) fn flat_amount(
    ctx: &Ctx,
    kind: &FringeKind,
    d: &Detail,
    v: Factors,
    rate: Num,
) -> Option<Cents> {
    flat_count(ctx.arith, &ctx.b.units, kind, d, v)?.amount(rate)
}

/// X or 4X.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Factor {
    X,
    X4,
}

/// Why a flat fringe's count starts where it does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CountFrom {
    /// The fringe has no unit: once per line.
    PerLine,
    /// The fringe's unit has no hours (or 0): charged once.
    FringeNoHours(String),
    /// The line's unit (blank for none) has no hours (or 0): charged once.
    LineNoHours(String),
    /// It would be charged once, but the line's Amount is 0: nothing is
    /// counted (decisions.md, entry 68).
    NoAmount,
    /// The line is in the fringe's own unit: the count is the Amount
    /// (`base`).
    SameUnit,
    /// `base` in the line's unit, converted: `base x line_hours /
    /// fringe_hours`.
    Hours { line_hours: Dec, fringe_hours: Dec },
    /// A per-hour fringe with a most-hours-a-day cap: `days x min(per_day,
    /// cap)` hours, where `days` is the Amount converted to days through
    /// the Day unit (`day_unit`, `day_hours` hours) and `per_day` is X when
    /// X is filled in and in hours (`from_x`), else the Day unit's hours.
    Capped {
        line_hours: Dec,
        fringe_hours: Dec,
        day_unit: String,
        day_hours: Dec,
        per_day: Dec,
        cap: Dec,
        from_x: bool,
    },
}

/// How many of its units a flat fringe counts on one line, and why. The
/// count is (product of `num`) / (product of `den`). Exact arithmetic
/// multiplies `rate` by it as an exact fraction, in that order, and rounds
/// to cents once (decisions.md, entries 2 and 54); Movie Magic arithmetic
/// works the count out in doubles, left to right, then multiplies it by
/// `rate` and rounds (entry 55): `0.06` per 12-hour Day on 1 Hour is
/// `0.06 x (1 x 1 / 12)`, 0.004999... and so 0.00.
#[derive(Clone, Debug)]
pub(crate) struct FlatCount {
    pub arith: Arithmetic,
    pub basis: FlatBasis,
    /// What the count starts from, as shown: the Amount, or under
    /// `Quantity` without a cap the line's quantity.
    pub base: Dec,
    pub from: CountFrom,
    /// X or 4X multiplied in, with its value as shown (blank ones, which
    /// are 1, are left off).
    pub counted: Vec<(Factor, Dec)>,
    /// X or 4X filled in but not counted.
    pub ignored: Vec<(Factor, Dec)>,
    pub num: Vec<Num>,
    pub den: Vec<Num>,
}

impl FlatCount {
    /// The decimals of `v` (exact arithmetic's values are all decimals).
    fn decs(v: &[Num]) -> Option<Vec<Dec>> {
        v.iter().map(|n| n.shown()).collect()
    }

    /// `start` times every factor of `num`, exactly.
    fn times(&self, start: Dec) -> Option<Exact> {
        Self::decs(&self.num)?
            .into_iter()
            .try_fold(Exact::dec(start), |acc, f| acc.checked_mul(f))
    }

    /// The count in doubles, as Movie Magic works it out: the factors of
    /// `num` multiplied, then divided by each of `den`, left to right.
    fn float_count(&self) -> f64 {
        let up = self.num.iter().fold(1.0, |acc, n| acc * n.f64());
        self.den.iter().fold(up, |acc, d| acc / d.f64())
    }

    /// `rate x count`, rounded to cents once. `None` on overflow. With one
    /// divisor, exact arithmetic is the same, step for step, as before
    /// decisions.md entry 54.
    pub fn amount(&self, rate: Num) -> Option<Cents> {
        match self.arith {
            Arithmetic::Mmb => mmb_cents(rate.f64() * self.float_count()),
            Arithmetic::Exact => {
                let e = self.times(rate.shown()?)?;
                let den = Self::decs(&self.den)?;
                match den[..] {
                    [] => e.to_cents(),
                    [d] => e.div_to_cents(d),
                    _ => e.div_exact_to_cents(product(&den)?),
                }
            }
        }
    }

    /// `start x count` to 9 places, for showing. `None` on overflow.
    fn shown(&self, start: Dec) -> Option<Dec> {
        if self.arith == Arithmetic::Mmb {
            return Dec::from_f64(start.to_f64() * self.float_count());
        }
        let e = self.times(start)?;
        let den = Self::decs(&self.den)?;
        match den[..] {
            [] => e.to_dec(),
            [d] => e.div_to_dec(d),
            _ => e.div_exact_to_dec(product(&den)?),
        }
    }

    /// `rate x count` before rounding, for showing (exactly, or to 9
    /// places when the division need not end; a double as it is).
    pub fn exact_amount(&self, rate: Num) -> Option<Money> {
        match self.arith {
            Arithmetic::Mmb => {
                let v = rate.f64() * self.float_count();
                v.is_finite().then_some(Money::F64(v))
            }
            Arithmetic::Exact if self.den.is_empty() => self.times(rate.shown()?).map(Money::Exact),
            Arithmetic::Exact => self.shown(rate.shown()?).map(Money::from),
        }
    }

    /// The count to 9 places, for showing. `None` on overflow.
    pub fn count(&self) -> Option<Dec> {
        self.shown(Dec::ONE)
    }
}

/// The exact product of `factors` (at least one).
fn product(factors: &[Dec]) -> Option<Exact> {
    let (&first, rest) = factors.split_first()?;
    rest.iter()
        .try_fold(Exact::dec(first), |acc, &f| acc.checked_mul(f))
}

/// Hours in one `unit`, when the budget defines it with hours.
fn unit_hours(units: &BTreeMap<String, UnitDef>, unit: &str) -> Option<Dec> {
    units.get(unit).and_then(|u| u.hours)
}

/// Hours in one `unit` when it is a time unit: defined, with hours that
/// aren't 0.
pub(crate) fn time_hours(units: &BTreeMap<String, UnitDef>, unit: &str) -> Option<Dec> {
    unit_hours(units, unit).filter(|h| !h.is_zero())
}

/// Whether `unit` is an hour: one hour long, or named Hour (any case).
pub(crate) fn is_hour_unit(units: &BTreeMap<String, UnitDef>, unit: &str) -> bool {
    unit_hours(units, unit) == Some(Dec::ONE)
        || ["hour", "hours"]
            .iter()
            .any(|h| unit.trim().eq_ignore_ascii_case(h))
}

/// The budget's Day unit (named Day or Days, any case; `Day` first) and its
/// hours, when it is a time unit: what a flat fringe's most hours a day
/// count days by.
pub fn day_unit(units: &BTreeMap<String, UnitDef>) -> Option<(&str, Dec)> {
    let named = |n: &str| {
        ["day", "days"]
            .iter()
            .any(|d| n.trim().eq_ignore_ascii_case(d))
    };
    units
        .get_key_value("Day")
        .into_iter()
        .chain(units.iter().filter(|(n, _)| named(n)))
        .find_map(|(n, u)| Some((n.as_str(), u.hours.filter(|h| !h.is_zero())?)))
}

/// How many units flat fringe `kind` counts on line `d` (with evaluated
/// factors `v`). `None` when `kind` is not flat, or on overflow.
///
/// - `quantity` with no cap that applies: the rule before decisions.md entry
///   54, unchanged: the line's quantity converted through hours when the
///   fringe's unit has hours (not 0) and the line's unit has hours, else 1.
/// - Otherwise the Amount converted into the fringe's unit through hours
///   (the Amount itself when the line is in the fringe's unit), or 1 when
///   the fringe has no unit or either unit has no hours (or 0). For a
///   fringe charged per hour with `max_hours_per_day`, and a Day unit with
///   hours, the converted hours are `days x min(per_day, cap)`
///   ([`CountFrom::Capped`]). Then `amount_counts` multiplies X and 4X
///   whose units are not time units, and `quantity` multiplies both (X not
///   again when it was read as the hours a day).
pub(crate) fn flat_count(
    arith: Arithmetic,
    units: &BTreeMap<String, UnitDef>,
    kind: &FringeKind,
    d: &Detail,
    v: Factors,
) -> Option<FlatCount> {
    let FringeKind::Flat {
        unit,
        basis,
        max_hours_per_day,
        ..
    } = kind
    else {
        return None;
    };
    let basis = *basis;
    let n = |d: Dec| Num::of(arith, d);
    let fu = unit.as_deref();
    let fh = fu.and_then(|u| time_hours(units, u));
    let lh = time_hours(units, &d.unit);
    // The cap: a fringe per hour, both units with hours, and a Day unit.
    let cap = match (max_hours_per_day, fu, fh, lh) {
        (Some(cap), Some(u), Some(fh), Some(lh)) if is_hour_unit(units, u) => {
            day_unit(units).map(|(day, dh)| (*cap, fh, lh, day.to_string(), dh))
        }
        _ => None,
    };

    if basis == FlatBasis::Quantity && cap.is_none() {
        // The old rule, exactly: zero hours on the line's unit count 0.
        let q = v.quantity;
        let fh_old = fu.and_then(|u| unit_hours(units, u));
        let (from, num, den) = match (fh_old, unit_hours(units, &d.unit)) {
            (Some(fh), Some(lh)) if !fh.is_zero() => (
                if fu == Some(d.unit.as_str()) {
                    CountFrom::SameUnit
                } else {
                    CountFrom::Hours {
                        line_hours: lh,
                        fringe_hours: fh,
                    }
                },
                vec![q, n(lh)],
                vec![n(fh)],
            ),
            _ => (once_from(fu, fh_old, &d.unit), Vec::new(), Vec::new()),
        };
        return Some(FlatCount {
            arith,
            basis,
            base: q.shown()?,
            from,
            counted: Vec::new(),
            ignored: Vec::new(),
            num,
            den,
        });
    }

    let a = v.amount;
    let mut x_is_hours = false;
    let (from, mut num, den) = match (fu, fh, lh, cap) {
        (Some(_), Some(_), Some(_), Some((cap, fh, lh, day_unit, dh))) => {
            x_is_hours = !d.x.is_blank() && is_hour_unit(units, &d.unit2);
            let per_day = if x_is_hours { v.x } else { n(dh) };
            let used = per_day.min(n(cap));
            (
                CountFrom::Capped {
                    line_hours: lh,
                    fringe_hours: fh,
                    day_unit,
                    day_hours: dh,
                    per_day: per_day.shown()?,
                    cap,
                    from_x: x_is_hours,
                },
                vec![a, n(lh), used],
                vec![n(dh), n(fh)],
            )
        }
        (Some(u), Some(_), Some(_), None) if u == d.unit => {
            (CountFrom::SameUnit, vec![a], Vec::new())
        }
        (Some(_), Some(fh), Some(lh), None) => (
            CountFrom::Hours {
                line_hours: lh,
                fringe_hours: fh,
            },
            vec![a, n(lh)],
            vec![n(fh)],
        ),
        // Charged once, unless the Amount is 0: the reference product
        // charges a stand-in line of 0 with no unit nothing.
        _ if a.is_zero() => (CountFrom::NoAmount, vec![a], Vec::new()),
        _ => (once_from(fu, fh, &d.unit), Vec::new(), Vec::new()),
    };

    let mut counted = Vec::new();
    let mut ignored = Vec::new();
    for (factor, value, unit, blank) in [
        (Factor::X, v.x, &d.unit2, d.x.is_blank()),
        (Factor::X4, v.x4, &d.unit4, d.x4.is_blank()),
    ] {
        if factor == Factor::X && x_is_hours {
            continue;
        }
        let counts = match basis {
            FlatBasis::Amount => false,
            FlatBasis::AmountCounts => time_hours(units, unit).is_none(),
            FlatBasis::Quantity => true,
        };
        if counts {
            num.push(value);
        }
        if !blank {
            let shown = value.shown()?;
            if counts {
                counted.push((factor, shown));
            } else {
                ignored.push((factor, shown));
            }
        }
    }
    Some(FlatCount {
        arith,
        basis,
        base: a.shown()?,
        from,
        counted,
        ignored,
        num,
        den,
    })
}

/// Why a flat fringe counts once on a line: it has no unit, its unit has no
/// hours (`fringe_hours`), or the line's unit has none.
fn once_from(fringe_unit: Option<&str>, fringe_hours: Option<Dec>, line_unit: &str) -> CountFrom {
    match fringe_unit {
        None => CountFrom::PerLine,
        Some(u) if fringe_hours.is_none_or(Dec::is_zero) => CountFrom::FringeNoHours(u.to_string()),
        Some(_) => CountFrom::LineNoHours(line_unit.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::calc::{lines, RecalcOptions};
    use crate::model::{Budget, Category, Formula, Group, TopsheetRow, UnitDef};

    fn budget(details: Vec<Detail>) -> Budget {
        let mut b = Budget::new("Test", "USD");
        b.topsheet.push(TopsheetRow::Category(Category {
            notes: Vec::new(),
            code: "1000".into(),
            name: "Crew".into(),
            accounts: vec![Account {
                notes: Vec::new(),
                code: "1100".into(),
                name: "Camera".into(),
                details,
            }],
        }));
        b
    }

    /// Stage 1 on the first account, then stage 2 when `fringed`.
    fn rows_of(b: &Budget, fringed: bool) -> Result<Vec<RowTotals>, CalcError> {
        let opts = RecalcOptions::default();
        let ctx = Ctx::new(b, &opts)?;
        let acct = &b.categories().next().unwrap().accounts[0];
        let mut rows = lines::eval_rows(&ctx, acct)?;
        if fringed {
            apply_fringes(&ctx, acct, &mut rows)?;
        }
        Ok(rows)
    }

    fn run(b: &Budget) -> Result<Vec<RowTotals>, CalcError> {
        rows_of(b, true)
    }

    fn hold() -> (BTreeSet<String>, Group) {
        let g = Group {
            short_code: None,
            description: String::new(),
            in_budget_total: false,
            color: None,
        };
        (BTreeSet::from(["Hold".to_string()]), g)
    }

    fn pct(rate: &str, cutoff: Option<Cents>) -> Fringe {
        Fringe {
            short_code: None,
            description: String::new(),
            rate: rate.into(),
            kind: FringeKind::Percent { cutoff },
        }
    }

    fn flat(rate: &str, unit: Option<&str>) -> Fringe {
        Fringe {
            short_code: None,
            description: String::new(),
            rate: rate.into(),
            kind: FringeKind::flat(unit.map(String::from)),
        }
    }

    fn flat_with(rate: &str, unit: Option<&str>, basis: FlatBasis, cap: Option<&str>) -> Fringe {
        Fringe {
            kind: FringeKind::Flat {
                unit: unit.map(String::from),
                basis,
                max_hours_per_day: cap.map(|c| c.parse().unwrap()),
                cutoff: None,
            },
            ..flat(rate, unit)
        }
    }

    fn unit(hours: Option<&str>) -> UnitDef {
        UnitDef {
            plural: String::new(),
            description: String::new(),
            hours: hours.map(|h| h.parse().unwrap()),
        }
    }

    /// The units the reference product was checked with (2026-10-11): Day
    /// 12 hours, Week 60, Hour 1, Allow none, plus a non-time Episode.
    fn checked_units(b: &mut Budget) {
        b.units.clear();
        b.units.insert("Day".into(), unit(Some("12")));
        b.units.insert("Week".into(), unit(Some("60")));
        b.units.insert("Hour".into(), unit(Some("1")));
        b.units.insert("Allow".into(), unit(None));
        b.units.insert("Episode".into(), unit(None));
    }

    /// `amount unit [x X unit2] [4X x4 unit4]` at `rate`.
    fn shaped(
        id: u64,
        amount: &str,
        unit: &str,
        x: Option<(&str, &str)>,
        x4: Option<(&str, &str)>,
        rate: &str,
        fringes: &[&str],
    ) -> Detail {
        let mut d = line(id, amount, unit, rate, fringes, None);
        if let Some((v, u)) = x {
            d.x = v.into();
            d.unit2 = u.into();
        }
        if let Some((v, u)) = x4 {
            d.x4 = v.into();
            d.unit4 = u.into();
        }
        d
    }

    /// Lines in the shapes checked in the reference product, each with
    /// `fringes`.
    fn checked_lines(fringes: &[&str]) -> Vec<Detail> {
        vec![
            shaped(1, "25", "Day", None, None, "100", fringes),
            shaped(2, "25", "Day", Some(("3", "")), None, "100", fringes),
            shaped(3, "25", "Day", Some(("14", "Hour")), None, "100", fringes),
            shaped(
                4,
                "25",
                "Day",
                Some(("14", "Hour")),
                Some(("2", "")),
                "10",
                fringes,
            ),
            shaped(5, "2", "Week", None, None, "100", fringes),
            shaped(6, "300", "Hour", None, None, "100", fringes),
            shaped(7, "2", "Allow", None, None, "100", fringes),
        ]
    }

    /// The figures read off the reference product on 2026-10-11 (units Day
    /// 12h, Week 60h, Hour 1h, Allow none; ZDAY 100.00 per Day, ZHR 1.00
    /// per Hour), by the standard basis.
    #[test]
    fn flat_fringes_count_the_amount_as_checked_in_the_reference() {
        let mut b = budget(checked_lines(&["ZDAY", "ZHR"]));
        checked_units(&mut b);
        b.fringes.insert("ZDAY".into(), flat("100", Some("Day")));
        b.fringes.insert("ZHR".into(), flat("1", Some("Hour")));
        let rows = run(&b).unwrap();
        // 25 Days; x 3; x 14 Hours; x 14 Hours x 4X 2; 2 Weeks; 300 Hours; 2 Allow.
        assert_eq!(
            amounts(&rows, "ZDAY"),
            [
                Some(250_000),
                Some(250_000),
                Some(250_000),
                Some(250_000),
                Some(100_000),
                Some(250_000),
                Some(10_000),
            ]
        );
        assert_eq!(
            amounts(&rows, "ZHR"),
            [
                Some(30_000),
                Some(30_000),
                Some(30_000),
                Some(30_000),
                Some(12_000),
                Some(30_000),
                Some(100),
            ]
        );
        assert_eq!(rows[3].total, 700_000, "25 x 14 x 2 x 10.00");
        assert_totals_match(&rows);
    }

    #[test]
    fn amount_counts_multiplies_counts_not_time() {
        let mut b = budget(vec![
            // 3 crew x 25 days: 75 days.
            shaped(1, "25", "Day", Some(("3", "")), None, "100", &["ZDAY"]),
            // 14 Hours in X is time: ignored.
            shaped(2, "25", "Day", Some(("14", "Hour")), None, "100", &["ZDAY"]),
            // 2 Episodes in 4X (no hours) count; 14 Hours don't.
            shaped(
                3,
                "25",
                "Day",
                Some(("14", "Hour")),
                Some(("2", "Episode")),
                "1",
                &["ZDAY"],
            ),
            // A 4X in Weeks is time: ignored; a blank-unit X counts.
            shaped(
                4,
                "25",
                "Day",
                Some(("2", "")),
                Some(("3", "Week")),
                "1",
                &["ZDAY"],
            ),
            // No hours on the line: once, times the counts.
            shaped(5, "2", "Allow", Some(("3", "")), None, "1", &["ZDAY"]),
        ]);
        checked_units(&mut b);
        b.fringes.insert(
            "ZDAY".into(),
            flat_with("100", Some("Day"), FlatBasis::AmountCounts, None),
        );
        let rows = run(&b).unwrap();
        assert_eq!(
            amounts(&rows, "ZDAY"),
            [
                Some(750_000),
                Some(250_000),
                Some(500_000),
                Some(500_000),
                Some(30_000)
            ]
        );
    }

    /// The full quantity is the rule from before decisions.md 54: worked out
    /// here the old way, independently, for every checked shape.
    #[test]
    fn quantity_reproduces_the_old_figures() {
        let mut b = budget(checked_lines(&["ZDAY", "ZHR"]));
        checked_units(&mut b);
        b.fringes.insert(
            "ZDAY".into(),
            flat_with("100", Some("Day"), FlatBasis::Quantity, None),
        );
        b.fringes.insert(
            "ZHR".into(),
            flat_with("1", Some("Hour"), FlatBasis::Quantity, None),
        );
        let rows = run(&b).unwrap();
        let old = |rate: &str, fh: Option<&str>, r: &RowTotals, unit: &str| -> Cents {
            let hours = |u: &str| b.units.get(u).and_then(|d| d.hours);
            let rate: Dec = rate.parse().unwrap();
            match (fh.and_then(hours), hours(unit)) {
                (Some(fh), Some(lh)) if !fh.is_zero() => Exact::dec(rate)
                    .checked_mul(r.quantity)
                    .unwrap()
                    .checked_mul(lh)
                    .unwrap()
                    .div_to_cents(fh)
                    .unwrap(),
                _ => rate.to_cents().unwrap(),
            }
        };
        let units = ["Day", "Day", "Day", "Day", "Week", "Hour", "Allow"];
        for (k, r) in rows.iter().enumerate() {
            assert_eq!(
                r.fringes["ZDAY"],
                old("100", Some("Day"), r, units[k]),
                "line {k}"
            );
            assert_eq!(
                r.fringes["ZHR"],
                old("1", Some("Hour"), r, units[k]),
                "line {k}"
            );
        }
        // 25 Days x 3 counts 75 days; 25 x 14 Hours x 2 counts 700 "days".
        assert_eq!(rows[1].fringes["ZDAY"], 750_000);
        assert_eq!(rows[3].fringes["ZDAY"], 7_000_000);
        assert_eq!(rows[6].fringes["ZDAY"], 10_000, "still once on Allow");
    }

    #[test]
    fn max_hours_per_day_caps_hours_in_both_shapes() {
        let lines = vec![
            // Shape B: hours a day from the Day unit (12).
            shaped(1, "25", "Day", None, None, "100", &["ZHR"]),
            shaped(2, "2", "Week", None, None, "100", &["ZHR"]),
            shaped(3, "300", "Hour", None, None, "100", &["ZHR"]),
            // Shape A: "N Days x H Hours", hours a day from X.
            shaped(4, "25", "Day", Some(("14", "Hour")), None, "100", &["ZHR"]),
            shaped(
                5,
                "25",
                "Day",
                Some(("8", "Hour")),
                Some(("2", "")),
                "100",
                &["ZHR"],
            ),
            // No hours on the line: still once.
            shaped(6, "2", "Allow", Some(("14", "Hour")), None, "100", &["ZHR"]),
        ];
        let mut b = budget(lines);
        checked_units(&mut b);
        let with_cap = |b: &mut Budget, basis: FlatBasis, cap: Option<&str>| {
            b.fringes
                .insert("ZHR".into(), flat_with("1", Some("Hour"), basis, cap));
            amounts(&run(b).unwrap(), "ZHR")
        };
        // Cap 12 with a 12-hour Day: 25 x 12 = 300; X's 14 hours capped to 12.
        assert_eq!(
            with_cap(&mut b, FlatBasis::Amount, Some("12")),
            [
                Some(30_000),
                Some(12_000),
                Some(30_000),
                Some(30_000),
                Some(20_000),
                Some(100)
            ]
        );
        // Cap 10: 25 x 10 = 250; 2 Weeks = 10 days x 10 = 100; 300 Hours =
        // 25 days x 10; 25 x min(14, 10); 25 x min(8, 10) = 200.
        assert_eq!(
            with_cap(&mut b, FlatBasis::Amount, Some("10")),
            [
                Some(25_000),
                Some(10_000),
                Some(25_000),
                Some(25_000),
                Some(20_000),
                Some(100)
            ]
        );
        // A cap above the Day's hours: the Day lines as without a cap, but
        // X's hours a day are counted (25 x 14).
        assert_eq!(
            with_cap(&mut b, FlatBasis::Amount, Some("16")),
            [
                Some(30_000),
                Some(12_000),
                Some(30_000),
                Some(35_000),
                Some(20_000),
                Some(100)
            ]
        );
        // Counts in X and 4X multiply on top: the 2 in 4X on line 5.
        assert_eq!(
            with_cap(&mut b, FlatBasis::AmountCounts, Some("10"))[4],
            Some(40_000)
        );
        // Under the full quantity the cap reads X as hours a day, 4X counts.
        assert_eq!(
            with_cap(&mut b, FlatBasis::Quantity, Some("10"))[3..5],
            [Some(25_000), Some(40_000)]
        );
        // Hours in X's unit but no X typed: hours a day are the Day's.
        let mut blank_x = budget(vec![shaped(
            1,
            "25",
            "Day",
            Some(("", "Hour")),
            None,
            "100",
            &["ZHR"],
        )]);
        checked_units(&mut blank_x);
        assert_eq!(
            with_cap(&mut blank_x, FlatBasis::Amount, Some("10")),
            [Some(25_000)]
        );
        // A cap on a fringe charged per Day is ignored.
        b.fringes.insert(
            "ZHR".into(),
            flat_with("100", Some("Day"), FlatBasis::Amount, Some("10")),
        );
        assert_eq!(amounts(&run(&b).unwrap(), "ZHR")[0], Some(250_000));
        // With no Day unit the cap can't apply.
        b.units.remove("Day");
        b.units.insert("Shift".into(), unit(Some("12")));
        for d in all_lines(&mut b) {
            if d.unit == "Day" {
                d.unit = "Shift".into();
            }
        }
        assert_eq!(
            with_cap(&mut b, FlatBasis::Amount, Some("10"))[0],
            Some(30_000)
        );
    }

    fn all_lines(b: &mut Budget) -> Vec<&mut Detail> {
        b.categories_mut()
            .flat_map(|c| &mut c.accounts)
            .flat_map(|a| &mut a.details)
            .collect()
    }

    #[test]
    fn a_cutoff_kept_on_a_flat_fringe_is_ignored() {
        let mut b = budget(vec![
            line(1, "25", "Day", "1000", &["ZDAY"], Some(1)),
            line(2, "25", "Day", "1000", &["ZDAY"], Some(1)),
        ]);
        b.fringes.insert(
            "ZDAY".into(),
            Fringe {
                kind: FringeKind::Flat {
                    unit: Some("Day".into()),
                    basis: FlatBasis::Amount,
                    max_hours_per_day: None,
                    cutoff: Some(100),
                },
                ..flat("100", Some("Day"))
            },
        );
        // Per line, whole: the range and the 1.00 cutoff change nothing.
        let rows = run(&b).unwrap();
        assert_eq!(amounts(&rows, "ZDAY"), [Some(250_000), Some(250_000)]);
        assert_eq!(b.fringes["ZDAY"].kind.cutoff(), Some(100));
    }

    #[test]
    fn hour_and_day_units_are_found_by_hours_or_name() {
        let mut b = budget(Vec::new());
        checked_units(&mut b);
        b.units.insert("Hr".into(), unit(Some("1")));
        b.units.insert("HOURS".into(), unit(Some("2")));
        assert!(is_hour_unit(&b.units, "Hr"));
        assert!(is_hour_unit(&b.units, "HOURS"));
        assert!(is_hour_unit(&b.units, "hour"));
        assert!(!is_hour_unit(&b.units, "Day"));
        assert_eq!(day_unit(&b.units), Some(("Day", "12".parse().unwrap())));
        b.units.remove("Day");
        b.units.insert("days".into(), unit(Some("8")));
        assert_eq!(day_unit(&b.units), Some(("days", "8".parse().unwrap())));
        b.units.insert("days".into(), unit(Some("0")));
        assert_eq!(day_unit(&b.units), None);
        assert_eq!(time_hours(&b.units, "Allow"), None);
        assert_eq!(time_hours(&b.units, "days"), None, "0 hours is no hours");
    }

    /// `amount` `unit` at `rate`, carrying `fringes`, in `range`.
    fn line(
        id: u64,
        amount: &str,
        unit: &str,
        rate: &str,
        fringes: &[&str],
        range: Option<u32>,
    ) -> Detail {
        Detail {
            fringes: fringes.iter().map(|s| s.to_string()).collect(),
            fringe_range: range,
            ..Detail::line(id, format!("line {id}"), amount, unit, rate)
        }
    }

    fn amounts(rows: &[RowTotals], name: &str) -> Vec<Option<Cents>> {
        rows.iter().map(|r| r.fringes.get(name).copied()).collect()
    }

    fn assert_totals_match(rows: &[RowTotals]) {
        for r in rows {
            assert_eq!(r.fringe_total, r.fringes.values().sum::<Cents>());
        }
    }

    #[test]
    fn plain_percent() {
        let mut b = budget(vec![line(1, "10", "Day", "100", &["FICA"], None)]);
        b.fringes.insert("FICA".into(), pct("7.65", None));
        let rows = run(&b).unwrap();
        assert_eq!(rows[0].total, 100_000);
        assert_eq!(amounts(&rows, "FICA"), [Some(7_650)]);
        assert_eq!(rows[0].fringe_total, 7_650);
    }

    #[test]
    fn percent_rounds_once_half_away_from_zero() {
        let mut b = budget(vec![
            line(1, "1", "Flat", "0.25", &["P"], None),
            line(2, "1", "Flat", "-0.25", &["P"], None),
        ]);
        b.fringes.insert("P".into(), pct("50", None));
        b.arithmetic = Arithmetic::Exact;
        let rows = run(&b).unwrap();
        assert_eq!(amounts(&rows, "P"), [Some(13), Some(-13)]);
        // Movie Magic rounds a half up, toward plus infinity: -0.125 is
        // -0.12 (decisions.md, entry 55).
        b.arithmetic = Arithmetic::Mmb;
        let rows = run(&b).unwrap();
        assert_eq!(amounts(&rows, "P"), [Some(13), Some(-12)]);
    }

    #[test]
    fn cutoff_on_single_line() {
        let mut b = budget(vec![
            line(1, "1", "Flat", "2000", &["PEN"], None),
            line(2, "1", "Flat", "1000", &["PEN"], None),
        ]);
        b.fringes.insert("PEN".into(), pct("10", Some(150_000)));
        let rows = run(&b).unwrap();
        // Each line outside a range is its own earner with its own cutoff.
        assert_eq!(amounts(&rows, "PEN"), [Some(15_000), Some(10_000)]);
    }

    #[test]
    fn range_uses_its_cutoff_in_row_order() {
        let mut b = budget(vec![
            line(1, "1", "Flat", "1000", &["PEN"], Some(1)),
            line(2, "1", "Flat", "1000", &["PEN"], Some(1)),
            line(3, "1", "Flat", "1000", &["PEN"], Some(1)),
            // In the range but without the fringe: uses none of the cutoff.
            line(4, "1", "Flat", "5000", &[], Some(1)),
        ]);
        b.fringes.insert("PEN".into(), pct("10", Some(200_000)));
        let rows = run(&b).unwrap();
        // The first two lines use the 2,000.00 cutoff up; the third earns 0.
        assert_eq!(
            amounts(&rows, "PEN"),
            [Some(10_000), Some(10_000), Some(0), None]
        );
        assert_eq!(rows.iter().map(|r| r.fringe_total).sum::<Cents>(), 20_000);
        assert_totals_match(&rows);
    }

    #[test]
    fn each_row_of_a_range_rounds_on_its_own() {
        let mut b = budget(vec![
            line(1, "1", "Flat", "1000", &["FICA"], Some(7)),
            line(2, "1", "Flat", "0.04", &["FICA"], Some(7)),
            line(3, "1", "Flat", "0.04", &["FICA"], Some(7)),
            line(4, "1", "Flat", "0.04", &["FICA"], Some(7)),
        ]);
        b.fringes.insert("FICA".into(), pct("12.5", Some(222_222)));
        let rows = run(&b).unwrap();
        // 0.005 each, a cent each (0.03), not 0.015 rounded once.
        assert_eq!(
            amounts(&rows, "FICA"),
            [Some(12_500), Some(1), Some(1), Some(1)]
        );
    }

    #[test]
    fn a_credit_after_the_cutoff_is_used_earns_nothing() {
        let mut b = budget(vec![
            line(1, "1", "Flat", "3000", &["P"], Some(1)),
            line(2, "1", "Flat", "-500", &["P"], Some(1)),
            line(3, "1", "Flat", "-500", &["P"], None),
        ]);
        b.fringes.insert("P".into(), pct("10", Some(100_000)));
        let rows = run(&b).unwrap();
        // In the range: 100.00 then 0.00; on its own the credit earns -50.00.
        assert_eq!(amounts(&rows, "P"), [Some(10_000), Some(0), Some(-5_000)]);
    }

    #[test]
    fn different_range_number_splits_runs() {
        let mut b = budget(vec![
            line(1, "1", "Flat", "1000", &["PEN"], Some(1)),
            line(2, "1", "Flat", "1000", &["PEN"], Some(2)),
            line(3, "1", "Flat", "1000", &["PEN"], Some(1)),
            line(4, "1", "Flat", "1000", &["PEN"], None),
            line(5, "1", "Flat", "1000", &["PEN"], Some(1)),
        ]);
        b.fringes.insert("PEN".into(), pct("10", Some(50_000)));
        let rows = run(&b).unwrap();
        // Five separate earners, each capped at 500.00.
        assert_eq!(amounts(&rows, "PEN"), [Some(5_000); 5]);
    }

    #[test]
    fn subtotal_inside_range_does_not_break_it() {
        let mut b = budget(vec![
            line(1, "1", "Flat", "1000", &["PEN"], Some(1)),
            Detail::subtotal(2, "Prep"),
            line(3, "1", "Flat", "1000", &["PEN"], Some(1)),
        ]);
        b.fringes.insert("PEN".into(), pct("10", Some(150_000)));
        let rows = run(&b).unwrap();
        // 1,000.00 of the cutoff, then the 500.00 left.
        assert_eq!(amounts(&rows, "PEN"), [Some(10_000), None, Some(5_000)]);
        assert_eq!(rows[1].fringe_total, 0);
    }

    #[test]
    fn adjustment_row_breaks_range_and_earns_its_own_fringes() {
        let mut adj = Detail::adjustment(2, "Kit", "0.1");
        adj.fringes.insert("PEN".into());
        let mut b = budget(vec![
            line(1, "1", "Flat", "1000", &["PEN"], Some(1)),
            adj,
            line(3, "1", "Flat", "1000", &["PEN"], Some(1)),
        ]);
        b.fringes.insert("PEN".into(), pct("10", Some(150_000)));
        let rows = run(&b).unwrap();
        assert_eq!(rows[1].total, 10_000);
        // 10% of the 100.00 adjustment, as Movie Magic charges it.
        assert_eq!(
            amounts(&rows, "PEN"),
            [Some(10_000), Some(1_000), Some(10_000)]
        );
        assert_eq!(rows[1].fringe_total, 1_000);
    }

    #[test]
    fn adjustment_rows_take_part_in_a_range() {
        let mut adj = Detail::adjustment(2, "Kit", "0.5");
        adj.fringes.insert("PEN".into());
        adj.fringe_range = Some(1);
        let mut b = budget(vec![
            line(1, "1", "Flat", "1000", &["PEN"], Some(1)),
            adj,
            line(3, "1", "Flat", "1000", &["PEN"], Some(1)),
        ]);
        b.fringes.insert("PEN".into(), pct("10", Some(150_000)));
        let rows = run(&b).unwrap();
        // 1,000.00, then the 500.00 adjustment uses the rest.
        assert_eq!(amounts(&rows, "PEN"), [Some(10_000), Some(5_000), Some(0)]);
    }

    #[test]
    fn excluded_line_in_range_does_not_contribute() {
        let hold = BTreeSet::from(["Hold".to_string()]);
        let mut b = budget(vec![
            line(1, "1", "Flat", "1000", &["PEN", "MEAL"], Some(1)),
            Detail {
                groups: hold,
                ..line(2, "1", "Flat", "1000", &["PEN", "MEAL"], Some(1))
            },
            line(3, "1", "Flat", "1000", &["PEN", "MEAL"], Some(1)),
        ]);
        b.groups.insert(
            "Hold".into(),
            Group {
                short_code: None,
                description: String::new(),
                in_budget_total: false,
                color: None,
            },
        );
        b.fringes.insert("PEN".into(), pct("10", Some(150_000)));
        b.fringes.insert("MEAL".into(), flat("12", None));
        let rows = run(&b).unwrap();
        assert!(!rows[1].included);
        // The left-out line uses none of the cutoff: 1,000.00, then 500.00.
        assert_eq!(amounts(&rows, "PEN"), [Some(10_000), None, Some(5_000)]);
        assert_eq!(amounts(&rows, "MEAL"), [Some(1_200), None, Some(1_200)]);
        assert!(rows[1].fringes.is_empty());
        assert_eq!(rows[1].fringe_total, 0);
        assert_eq!(rows[0].fringe_total, 11_200);
    }

    #[test]
    fn flat_once_per_line() {
        let mut b = budget(vec![
            line(1, "10", "Day", "100", &["MEAL"], Some(1)),
            line(2, "3", "Week", "500", &["MEAL"], Some(1)),
        ]);
        b.fringes.insert("MEAL".into(), flat("25", None));
        let rows = run(&b).unwrap();
        assert_eq!(amounts(&rows, "MEAL"), [Some(2_500), Some(2_500)]);
    }

    #[test]
    fn flat_per_day_on_week_line_converts_by_hours() {
        let mut b = budget(vec![
            line(1, "2", "Week", "1000", &["HW"], None),
            Detail {
                x: "3".into(),
                ..line(2, "2", "Week", "1000", &["HW"], None)
            },
            line(3, "18", "Hour", "50", &["HW"], None),
            line(4, "1", "Day", "300", &["HW"], None),
        ]);
        b.fringes.insert("HW".into(), flat("20", Some("Day")));
        let rows = run(&b).unwrap();
        // 20/day x Amount x line hours / 12; the 3 in X doesn't count.
        assert_eq!(
            amounts(&rows, "HW"),
            [Some(20_000), Some(20_000), Some(3_000), Some(2_000)]
        );
        // Under the full quantity it does, as before decisions.md 54.
        b.fringes.insert(
            "HW".into(),
            flat_with("20", Some("Day"), FlatBasis::Quantity, None),
        );
        let rows = run(&b).unwrap();
        assert_eq!(
            amounts(&rows, "HW"),
            [Some(20_000), Some(60_000), Some(3_000), Some(2_000)]
        );
    }

    #[test]
    fn flat_rounds_once_after_conversion() {
        let mut b = budget(vec![line(1, "1", "Hour", "50", &["HW"], None)]);
        b.units.insert(
            "Shift".into(),
            UnitDef {
                plural: "Shifts".into(),
                description: String::new(),
                hours: Some("3".parse().unwrap()),
            },
        );
        // 10.01 per 3-hour shift on one hour: 3.336666... -> 3.34.
        b.fringes.insert("HW".into(), flat("10.01", Some("Shift")));
        let rows = run(&b).unwrap();
        assert_eq!(amounts(&rows, "HW"), [Some(334)]);
    }

    #[test]
    fn flat_without_hours_falls_back_to_once_per_line() {
        let mut b = budget(vec![
            line(1, "2", "Week", "1000", &["SHIFT"], None),
            line(2, "2", "Allow", "1000", &["HW"], None),
            line(3, "2", "", "1000", &["HW"], None),
            line(4, "2", "Week", "1000", &["ZERO"], None),
        ]);
        b.units.insert(
            "Zero".into(),
            UnitDef {
                plural: "Zeros".into(),
                description: String::new(),
                hours: Some(Dec::ZERO),
            },
        );
        b.fringes.insert("SHIFT".into(), flat("20", Some("Shift")));
        b.fringes.insert("HW".into(), flat("20", Some("Day")));
        b.fringes.insert("ZERO".into(), flat("20", Some("Zero")));
        let rows = run(&b).unwrap();
        let got: Vec<Cents> = rows.iter().map(|r| r.fringe_total).collect();
        assert_eq!(got, [2_000; 4]);
    }

    #[test]
    fn percent_and_flat_on_one_line() {
        let mut b = budget(vec![line(1, "10", "Day", "100", &["FICA", "HW"], None)]);
        b.fringes.insert("FICA".into(), pct("7.65", None));
        b.fringes.insert("HW".into(), flat("20", Some("Day")));
        let rows = run(&b).unwrap();
        assert_eq!(
            rows[0].fringes,
            BTreeMap::from([("FICA".to_string(), 7_650), ("HW".to_string(), 20_000)])
        );
        assert_eq!(rows[0].fringe_total, 27_650);
    }

    #[test]
    fn fringe_rate_via_global() {
        let mut b = budget(vec![line(1, "10", "Day", "100", &["FICA"], None)]);
        b.globals
            .insert("FICA_RATE".into(), Formula::from("6.2 + 1.45"));
        b.fringes.insert("FICA".into(), pct("FICA_RATE", None));
        let rows = run(&b).unwrap();
        assert_eq!(amounts(&rows, "FICA"), [Some(7_650)]);
    }

    #[test]
    fn bad_rate_formula_is_reported_with_fringe_name() {
        let mut b = budget(vec![line(1, "10", "Day", "100", &["FICA"], None)]);
        b.fringes.insert("FICA".into(), pct("NOPE", None));
        let e = run(&b).unwrap_err();
        assert_eq!(e.at, "fringe FICA: rate");
        assert!(matches!(e.kind, ErrorKind::Eval(_)));
    }

    #[test]
    fn unknown_fringe_is_an_error() {
        let b = budget(vec![line(1, "10", "Day", "100", &["Ghost"], None)]);
        let e = run(&b).unwrap_err();
        assert_eq!(e.kind, ErrorKind::UnknownFringe("Ghost".into()));
        assert_eq!(e.at, "account 1100 line 1 (line 1): fringes");
    }

    #[test]
    fn unknown_fringe_on_excluded_line_is_still_an_error() {
        let mut b = budget(vec![
            line(1, "10", "Day", "100", &[], None),
            Detail {
                groups: BTreeSet::from(["Hold".to_string()]),
                ..line(2, "10", "Day", "100", &["Ghost"], None)
            },
        ]);
        b.groups.insert(
            "Hold".into(),
            Group {
                short_code: None,
                description: String::new(),
                in_budget_total: false,
                color: None,
            },
        );
        let e = run(&b).unwrap_err();
        assert_eq!(e.kind, ErrorKind::UnknownFringe("Ghost".into()));
        assert_eq!(e.at, "account 1100 line 2 (line 2): fringes");
    }

    #[test]
    fn negative_line() {
        let mut b = budget(vec![
            line(1, "1", "Flat", "-1000", &["PEN"], None),
            line(2, "1", "Flat", "-1000", &["PEN"], Some(1)),
            line(3, "1", "Flat", "-1000", &["PEN"], Some(1)),
            line(4, "1", "Flat", "-1000", &["PEN"], Some(1)),
        ]);
        b.fringes.insert("PEN".into(), pct("10", Some(50_000)));
        let rows = run(&b).unwrap();
        // min(-1000, 500) keeps the credit; in the range each credit adds
        // to the cutoff left, so each earns its own -100.00.
        assert_eq!(
            amounts(&rows, "PEN"),
            [Some(-10_000), Some(-10_000), Some(-10_000), Some(-10_000)]
        );
    }

    #[test]
    fn without_a_cutoff_each_row_earns_its_own_percent() {
        let mut b = budget(vec![
            line(1, "1", "Flat", "1000", &["PEN"], Some(1)),
            line(2, "1", "Flat", "-1000", &["PEN"], Some(1)),
        ]);
        b.fringes.insert("PEN".into(), pct("10", None));
        let rows = run(&b).unwrap();
        assert_eq!(amounts(&rows, "PEN"), [Some(10_000), Some(-10_000)]);
    }

    #[test]
    fn overflow_is_an_error_not_a_panic() {
        let mut b = budget(vec![line(1, "1", "Flat", "1", &["BIG"], None)]);
        b.fringes
            .insert("BIG".into(), flat("100000000000000000", None));
        let e = run(&b).unwrap_err();
        assert_eq!(e.kind, ErrorKind::Overflow);
        assert_eq!(e.at, "account 1100 line 1 (line 1): fringe BIG");

        let mut b = budget(vec![line(
            1,
            "1",
            "Flat",
            "90000000000000000",
            &["P"],
            None,
        )]);
        b.fringes.insert("P".into(), pct("1000", None));
        assert_eq!(run(&b).unwrap_err().kind, ErrorKind::Overflow);
    }

    #[test]
    fn earners_group_runs() {
        let mut ds = vec![
            line(0, "1", "Flat", "1", &[], None),
            line(1, "1", "Flat", "1", &[], Some(1)),
            Detail::subtotal(2, "sub"),
            line(3, "1", "Flat", "1", &[], Some(1)),
            line(4, "1", "Flat", "1", &[], Some(2)),
            Detail {
                fringe_range: Some(2),
                ..Detail::adjustment(5, "adj", "0.1")
            },
            line(6, "1", "Flat", "1", &[], Some(2)),
            line(7, "1", "Flat", "1", &[], None),
        ];
        let b = budget(ds.clone());
        let rows = rows_of(&b, false).unwrap();
        assert_eq!(
            earners(&ds, &rows),
            vec![vec![0], vec![1, 3], vec![4, 5, 6], vec![7]]
        );
        ds.clear();
        assert!(earners(&ds, &[]).is_empty());
    }

    #[test]
    fn uncounted_rows_do_not_break_a_range() {
        let (tags, group) = hold();
        let mut adj = Detail::adjustment(3, "adj", "0.1");
        adj.groups = tags.clone();
        let mut b = budget(vec![
            line(1, "1", "Flat", "1000", &["PEN"], Some(1)),
            Detail {
                groups: tags,
                ..line(2, "1", "Flat", "1000", &["PEN"], None)
            },
            adj,
            line(4, "1", "Flat", "1000", &["PEN"], Some(1)),
        ]);
        b.groups.insert("Hold".into(), group);
        b.fringes.insert("PEN".into(), pct("10", Some(150_000)));
        let rows = run(&b).unwrap();
        // Same as if rows 2 and 3 were deleted: one earner, 1,500.00 cutoff.
        assert_eq!(
            amounts(&rows, "PEN"),
            [Some(10_000), None, None, Some(5_000)]
        );
    }
}
