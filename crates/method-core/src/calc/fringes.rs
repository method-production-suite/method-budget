//! Stage 2: fringe dollars per row.
//!
//! Only included `Line` rows earn fringes. A percent fringe is charged per
//! earner: a single line, or a fringe range (consecutive rows sharing
//! `fringe_range: Some(n)`; subtotal, separator and uncounted rows don't
//! break a run).
//! The earner's amount is rounded once, then split back over its lines so the
//! shares sum exactly. A flat fringe is charged per line, converted by unit
//! hours when both units have them.

use std::collections::{BTreeMap, BTreeSet};

use crate::calc::{err, overflow, row_at, sum, CalcError, Ctx, ErrorKind};
use crate::decimal::{percent_of, Dec, Exact};
use crate::model::{Account, Detail, Fringe, FringeKind, RowKind};
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
    }

    // Check every name (excluded lines too) and evaluate each rate once.
    let mut rates: BTreeMap<&str, (&Fringe, Dec)> = BTreeMap::new();
    for (i, d) in details.iter().enumerate() {
        if d.kind != RowKind::Line {
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
        .map(|(d, r)| d.kind == RowKind::Line && r.included)
        .collect();

    // Percent fringes, per earner. Only the fringes the earner's lines carry
    // can give it anything, so those are the ones visited, in name order (the
    // same order, and so the same first error, as visiting every fringe).
    let mut carried: BTreeSet<&str> = BTreeSet::new();
    let mut names: Vec<&str> = Vec::new();
    let mut lines: Vec<usize> = Vec::new();
    let mut weights: Vec<Cents> = Vec::new();
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
            let Some(&first) = lines.first() else {
                continue;
            };
            let ovf = || {
                overflow(row_at(
                    &acct.code,
                    first,
                    &details[first],
                    &format!("fringe {name}"),
                ))
            };
            weights.clear();
            weights.extend(lines.iter().map(|&i| rows[i].total));
            let mut base = sum(weights.iter().copied()).ok_or_else(ovf)?;
            if let Some(c) = cutoff {
                base = base.min(c);
            }
            let amount = percent_of(base, rate).ok_or_else(ovf)?;
            let shares = allocate(amount, &weights).ok_or_else(ovf)?;
            for (&i, share) in lines.iter().zip(shares) {
                rows[i].fringes.insert(name.to_string(), share);
            }
        }
    }

    // Flat fringes, per line.
    for (i, d) in details.iter().enumerate() {
        if !earns[i] {
            continue;
        }
        for name in &d.fringes {
            let Some(&(f, rate)) = rates.get(name.as_str()) else {
                continue;
            };
            let FringeKind::Flat { unit } = &f.kind else {
                continue;
            };
            let amount = flat_amount(ctx, unit.as_deref(), &d.unit, rate, rows[i].quantity)
                .ok_or_else(|| overflow(row_at(&acct.code, i, d, &format!("fringe {name}"))))?;
            rows[i].fringes.insert(name.clone(), amount);
        }
    }

    for (i, d) in details.iter().enumerate() {
        let r = &mut rows[i];
        r.fringe_total = sum(r.fringes.values().copied())
            .ok_or_else(|| overflow(row_at(&acct.code, i, d, "fringes")))?;
    }
    Ok(())
}

/// Percent-fringe earners as lists of `Line` row indexes, in row order. Lines
/// outside a range are their own earner. A range is a maximal run of rows with
/// the same `fringe_range: Some(n)`. Subtotal and separator rows, and rows
/// that don't count (suppressed or filtered out), are skipped, so they never
/// break a run; any other row with a different value does. Skipping uncounted rows keeps
/// "excluding a group equals deleting its lines" true (decisions.md 4).
pub(super) fn earners(details: &[Detail], rows: &[RowTotals]) -> Vec<Vec<usize>> {
    let mut out = Vec::new();
    let mut run: Option<(u32, Vec<usize>)> = None;
    for (i, d) in details.iter().enumerate() {
        if !d.kind.is_costed() || !rows[i].included {
            continue;
        }
        let is_line = d.kind == RowKind::Line;
        if let (Some((m, lines)), Some(n)) = (&mut run, d.fringe_range) {
            if *m == n {
                if is_line {
                    lines.push(i);
                }
                continue;
            }
        }
        out.extend(run.take().map(|(_, lines)| lines));
        match d.fringe_range {
            Some(n) => run = Some((n, if is_line { vec![i] } else { Vec::new() })),
            None if is_line => out.push(vec![i]),
            None => {}
        }
    }
    out.extend(run.map(|(_, lines)| lines));
    out.retain(|e| !e.is_empty());
    out
}

/// Splits `amount` over `weights` pro rata by largest remainder, so the shares
/// sum to exactly `amount`. Ties go to the earlier weight. The magnitude is
/// split and the sign restored, so a credit mirrors the matching debit. When
/// the weights sum to 0 the first one takes everything. `None` on overflow.
pub(super) fn allocate(amount: Cents, weights: &[Cents]) -> Option<Vec<Cents>> {
    let mut out = vec![0; weights.len()];
    let Some(first) = out.first_mut() else {
        return Some(out);
    };
    let total = weights
        .iter()
        .try_fold(0i128, |acc, &w| acc.checked_add(i128::from(w)))?;
    if total == 0 {
        *first = amount;
        return Some(out);
    }
    // Make the divisor positive; the ratios are unchanged.
    let (total, flip) = if total < 0 { (-total, -1) } else { (total, 1) };
    let mag = i128::from(amount).abs();

    let mut shares = Vec::with_capacity(weights.len());
    let mut rems = Vec::with_capacity(weights.len());
    let mut given = 0i128;
    for (k, &w) in weights.iter().enumerate() {
        let n = mag.checked_mul(i128::from(w).checked_mul(flip)?)?;
        let q = n.div_euclid(total);
        given = given.checked_add(q)?;
        shares.push(q);
        rems.push((n.rem_euclid(total), k));
    }
    // Each remainder is below `total` and they sum to `left * total`, so
    // `0 <= left < weights.len()`.
    let left = usize::try_from(mag.checked_sub(given)?).ok()?;
    rems.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    for &(_, k) in rems.iter().take(left) {
        shares[k] = shares[k].checked_add(1)?;
    }
    let sign = if amount < 0 { -1 } else { 1 };
    for (slot, q) in out.iter_mut().zip(shares) {
        *slot = Cents::try_from(q.checked_mul(sign)?).ok()?;
    }
    Some(out)
}

/// A flat fringe on one line. Per unit, scaled by the hours ratio, when the
/// fringe's unit and the line's primary unit both have hours (and the
/// fringe's are not zero); otherwise `rate` once. The exact amount is
/// rounded to cents once.
pub(super) fn flat_amount(
    ctx: &Ctx,
    fringe_unit: Option<&str>,
    line_unit: &str,
    rate: Dec,
    quantity: Dec,
) -> Option<Cents> {
    let hours = |u: &str| ctx.b.units.get(u).and_then(|def| def.hours);
    match (fringe_unit.and_then(hours), hours(line_unit)) {
        (Some(fh), Some(lh)) if !fh.is_zero() => Exact::dec(rate)
            .checked_mul(quantity)?
            .checked_mul(lh)?
            .div_to_cents(fh),
        _ => rate.to_cents(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::calc::{eval_globals, lines, RecalcOptions};
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
        let ctx = Ctx {
            b,
            globals: eval_globals(b)?,
            opts: &opts,
            unfiltered: false,
        };
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
            kind: FringeKind::Flat {
                unit: unit.map(String::from),
            },
        }
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
            line(1, "1", "Flat", "33.35", &["P"], None),
            line(2, "1", "Flat", "-33.35", &["P"], None),
        ]);
        b.fringes.insert("P".into(), pct("10", None));
        let rows = run(&b).unwrap();
        assert_eq!(amounts(&rows, "P"), [Some(334), Some(-334)]);
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
    fn range_shares_one_cutoff_and_allocates_exactly() {
        let mut b = budget(vec![
            line(1, "1", "Flat", "1000", &["PEN"], Some(1)),
            line(2, "1", "Flat", "1000", &["PEN"], Some(1)),
            line(3, "1", "Flat", "1000", &["PEN"], Some(1)),
            // In the range but without the fringe: adds nothing to its base.
            line(4, "1", "Flat", "5000", &[], Some(1)),
        ]);
        b.fringes.insert("PEN".into(), pct("10", Some(200_000)));
        let rows = run(&b).unwrap();
        // 10% of min(3000, 2000) = 200.00, split 66.666... each, ties by row order.
        assert_eq!(
            amounts(&rows, "PEN"),
            [Some(6_667), Some(6_667), Some(6_666), None]
        );
        assert_eq!(rows.iter().map(|r| r.fringe_total).sum::<Cents>(), 20_000);
        assert_totals_match(&rows);
    }

    #[test]
    fn range_allocation_by_largest_remainder() {
        let mut b = budget(vec![
            line(1, "1", "Flat", "1000", &["FICA"], Some(7)),
            line(2, "1", "Flat", "500", &["FICA"], Some(7)),
            line(3, "1", "Flat", "250", &["FICA"], Some(7)),
        ]);
        b.fringes.insert("FICA".into(), pct("7.65", Some(100_000)));
        let rows = run(&b).unwrap();
        // 76.50 over 4371.43 / 2185.71 / 1092.86: the two larger remainders win.
        assert_eq!(
            amounts(&rows, "FICA"),
            [Some(4_371), Some(2_186), Some(1_093)]
        );
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
        assert_eq!(amounts(&rows, "PEN"), [Some(7_500), None, Some(7_500)]);
        assert_eq!(rows[1].fringe_total, 0);
    }

    #[test]
    fn adjustment_row_breaks_range_and_earns_nothing() {
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
        assert_eq!(amounts(&rows, "PEN"), [Some(10_000), None, Some(10_000)]);
        assert_eq!(rows[1].fringe_total, 0);
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
        // Base is 2000 (not 3000), capped to 1500.
        assert_eq!(amounts(&rows, "PEN"), [Some(7_500), None, Some(7_500)]);
        assert_eq!(amounts(&rows, "MEAL"), [Some(1_200), None, Some(1_200)]);
        assert!(rows[1].fringes.is_empty());
        assert_eq!(rows[1].fringe_total, 0);
        assert_eq!(rows[0].fringe_total, 8_700);
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
            line(3, "12", "Hour", "50", &["HW"], None),
            line(4, "1", "Day", "300", &["HW"], None),
        ]);
        b.fringes.insert("HW".into(), flat("20", Some("Day")));
        let rows = run(&b).unwrap();
        // 20/day x quantity x line hours / 10.
        assert_eq!(
            amounts(&rows, "HW"),
            [Some(20_000), Some(60_000), Some(2_400), Some(2_000)]
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
        // min(-1000, 500) keeps the credit; the range splits -300.00 evenly.
        assert_eq!(
            amounts(&rows, "PEN"),
            [Some(-10_000), Some(-10_000), Some(-10_000), Some(-10_000)]
        );
    }

    #[test]
    fn range_netting_to_zero_earns_zero() {
        let mut b = budget(vec![
            line(1, "1", "Flat", "1000", &["PEN"], Some(1)),
            line(2, "1", "Flat", "-1000", &["PEN"], Some(1)),
        ]);
        b.fringes.insert("PEN".into(), pct("10", None));
        let rows = run(&b).unwrap();
        assert_eq!(amounts(&rows, "PEN"), [Some(0), Some(0)]);
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
            vec![vec![0], vec![1, 3], vec![4, 6], vec![7]]
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
        // Same as if rows 2 and 3 were deleted: one earner capped at 1500.
        assert_eq!(
            amounts(&rows, "PEN"),
            [Some(7_500), None, None, Some(7_500)]
        );
    }

    #[test]
    fn allocate_cases() {
        assert_eq!(allocate(10, &[1, 1, 1]), Some(vec![4, 3, 3]));
        assert_eq!(allocate(-10, &[1, 1, 1]), Some(vec![-4, -3, -3]));
        assert_eq!(allocate(10, &[-1, -1, -1]), Some(vec![4, 3, 3]));
        assert_eq!(allocate(100, &[300, -100]), Some(vec![150, -50]));
        assert_eq!(allocate(7, &[5, -5]), Some(vec![7, 0]));
        assert_eq!(allocate(0, &[5, 5]), Some(vec![0, 0]));
        assert_eq!(allocate(5, &[0, 3, 0]), Some(vec![0, 5, 0]));
        assert_eq!(allocate(5, &[]), Some(vec![]));
        assert_eq!(allocate(i64::MAX, &[i64::MAX, -1]), None);
        let w = [123_457, 98_765, 1, 55_555, 77_777];
        for amount in [-9_999, -1, 0, 1, 2, 3, 12_345, 1_000_001] {
            let shares = allocate(amount, &w).unwrap();
            assert_eq!(shares.iter().sum::<Cents>(), amount);
        }
    }
}
