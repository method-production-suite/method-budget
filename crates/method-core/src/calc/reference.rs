//! The engine as it was before the speed work, kept for tests only: percent
//! fringes visit every fringe the account uses for every earner, and the
//! "full" group totals come from a second pass over every account. The
//! optimized engine must give exactly the same reports and errors.

use std::collections::{BTreeMap, BTreeSet};

use crate::calc::arith::{percent_of, Money, Num};
use crate::calc::{
    breakdown, err, lines, overflow, row_at, sum, topsheet, CalcError, Ctx, ErrorKind,
    RecalcOptions,
};
use crate::decimal::Dec;
use crate::model::{Account, Budget, Detail, Fringe, FringeKind};
use crate::report::{AccountTotals, CategoryTotals, Report, RowTotals};
use crate::Cents;

/// The old stage 2.
fn apply_fringes(ctx: &Ctx, acct: &Account, rows: &mut [RowTotals]) -> Result<(), CalcError> {
    let details = &acct.details;
    for r in rows.iter_mut() {
        r.fringes.clear();
        r.fringe_total = 0;
        r.fringe_full_total = 0;
    }
    let mut rates: BTreeMap<&str, (&Fringe, Num)> = BTreeMap::new();
    for (i, d) in details.iter().enumerate() {
        if !d.kind.is_costed() {
            continue;
        }
        for name in &d.fringes {
            let (key, f) = ctx.b.fringes.get_key_value(name).ok_or_else(|| {
                err(
                    row_at(&acct.code, i, d, "fringes"),
                    ErrorKind::UnknownFringe(name.clone()),
                )
            })?;
            if !rates.contains_key(key.as_str()) {
                let rate = ctx.eval(&f.rate, Dec::ZERO, &|| format!("fringe {key}: rate"))?;
                rates.insert(key, (f, rate));
            }
        }
    }
    let earns: Vec<bool> = details
        .iter()
        .zip(rows.iter())
        .map(|(d, r)| d.kind.is_costed() && r.included)
        .collect();
    for earner in earners(details, rows) {
        for (&name, &(f, rate)) in &rates {
            let FringeKind::Percent { cutoff } = f.kind else {
                continue;
            };
            // Times the budget's cutoff factor, rounded to cents.
            let cutoff = match cutoff {
                Some(c) => Some(
                    Money::cents(ctx.arith, c)
                        .checked_mul(Num::of(ctx.arith, ctx.b.cutoff_factor.value()))
                        .and_then(Money::to_cents)
                        .ok_or_else(|| overflow("cutoff"))?,
                ),
                None => None,
            };
            let lines: Vec<usize> = earner
                .iter()
                .copied()
                .filter(|&i| earns[i] && details[i].fringes.contains(name))
                .collect();
            // The cutoff, used up in row order: each row takes the percent
            // of its total, or of what is left of the cutoff when that is
            // less (nothing once it is used), rounded on its own.
            let mut left: Option<Cents> = cutoff;
            for &i in &lines {
                let total = rows[i].total;
                let taken = match left {
                    None => total,
                    Some(l) => {
                        left = Some(l.saturating_sub(total));
                        if l > 0 {
                            total.min(l)
                        } else {
                            0
                        }
                    }
                };
                let amount = percent_of(ctx.arith, taken, rate)
                    .and_then(Money::to_cents)
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
    for (i, d) in details.iter().enumerate() {
        if !earns[i] {
            continue;
        }
        for name in &d.fringes {
            let Some(&(f, rate)) = rates.get(name.as_str()) else {
                continue;
            };
            if !matches!(f.kind, FringeKind::Flat { .. }) {
                continue;
            }
            let v = super::fringes::Factors::of(ctx, &acct.code, i, d)?;
            let amount = super::fringes::flat_amount(ctx, &f.kind, d, v, rate)
                .ok_or_else(|| overflow(row_at(&acct.code, i, d, &format!("fringe {name}"))))?;
            rows[i].fringes.insert(name.clone(), amount);
        }
    }
    for (i, d) in details.iter().enumerate() {
        let r = &mut rows[i];
        r.fringe_total = sum(r.fringes.values().copied())
            .ok_or_else(|| overflow(row_at(&acct.code, i, d, "fringes")))?;
        // With no cutoff and no range: every percent fringe on the whole
        // total, the flat ones as earned.
        if earns[i] {
            let mut full: Cents = 0;
            for name in &d.fringes {
                let (f, rate) = rates[name.as_str()];
                let amount = match f.kind {
                    FringeKind::Percent { .. } => percent_of(ctx.arith, r.total, rate)
                        .and_then(Money::to_cents)
                        .ok_or_else(|| overflow("fringes"))?,
                    FringeKind::Flat { .. } => r.fringes[name],
                };
                full = full
                    .checked_add(amount)
                    .ok_or_else(|| overflow("fringes"))?;
            }
            r.fringe_full_total = full;
        }
    }
    Ok(())
}

fn earners(details: &[Detail], rows: &[RowTotals]) -> Vec<Vec<usize>> {
    super::fringes::earners(details, rows)
}

/// The old stages 1 and 2 over every account.
fn eval_categories(ctx: &Ctx) -> Result<Vec<CategoryTotals>, CalcError> {
    let ovf = || overflow("budget totals");
    let mut categories = Vec::new();
    for cat in ctx.b.categories() {
        let mut accounts = Vec::with_capacity(cat.accounts.len());
        for acct in &cat.accounts {
            let mut rows = lines::eval_rows(ctx, acct)?;
            apply_fringes(ctx, acct, &mut rows)?;
            let details_total = sum(rows.iter().map(|r| r.total)).ok_or_else(ovf)?;
            let details_total = ctx.account_total(details_total).ok_or_else(ovf)?;
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
    // Persons across accounts, over every account (decisions.md, entry 79).
    if super::persons::any(ctx.b) {
        super::apply_persons(ctx, &mut categories)?;
    }
    Ok(categories)
}

/// The old recalc: a counted pass, then a full unfiltered pass over every account.
pub(crate) fn recalc(b: &Budget, opts: &RecalcOptions) -> Result<Report, CalcError> {
    super::check_row_ids(b)?;
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
    let categories = eval_categories(&ctx)?;
    let ts = topsheet::finish(&ctx, categories)?;
    let display = b
        .totals_currency
        .as_ref()
        .is_some_and(|c| *c != b.base_currency)
        .then(|| super::display::display_totals(&ctx, &ts, totals_rate).ok())
        .flatten();
    let full_ctx = Ctx {
        unfiltered: true,
        ..ctx
    };
    let full: Vec<Vec<Option<Vec<RowTotals>>>> = eval_categories(&full_ctx)?
        .into_iter()
        .map(|c| c.accounts.into_iter().map(|a| Some(a.rows)).collect())
        .collect();
    // Rows under the group rules alone, for the fringes in group totals.
    let plain = RecalcOptions::default();
    let earned_ctx = Ctx {
        b: full_ctx.b,
        arith: full_ctx.arith,
        globals: full_ctx.globals.clone(),
        opts: &plain,
        unfiltered: false,
    };
    let earned: Option<Vec<Vec<Option<Vec<RowTotals>>>>> =
        if opts.exclude_groups.is_empty() && opts.only_groups.is_none() {
            None
        } else {
            Some(
                eval_categories(&earned_ctx)?
                    .into_iter()
                    .map(|c| c.accounts.into_iter().map(|a| Some(a.rows)).collect())
                    .collect(),
            )
        };
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::Expr;
    use crate::model::{FringePosting, Group};
    use crate::sample::{self, SampleSpec};

    /// A small deterministic generator for mutations.
    struct Rng(u64);

    impl Rng {
        fn below(&mut self, n: u64) -> u64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) % n.max(1)
        }
    }

    fn same(b: &Budget, opts: &RecalcOptions, what: &str) {
        let new = super::super::recalc(b, opts);
        let old = recalc(b, opts);
        assert_eq!(new, old, "{what}");
    }

    fn all_details(b: &mut Budget) -> Vec<&mut Detail> {
        b.categories_mut()
            .flat_map(|c| &mut c.accounts)
            .flat_map(|a| &mut a.details)
            .collect()
    }

    /// Generated budgets of several shapes, with every posting mode, group
    /// filters, suppressed groups and random edits (including ones that
    /// break the budget), give exactly the old reports and errors.
    #[test]
    fn optimized_recalc_equals_the_old_one() {
        for seed in 1..=12u64 {
            let spec = SampleSpec {
                seed,
                rows: 300 + (seed as usize) * 97,
                accounts: 12 + seed as usize,
                fringes: 6 + (seed as usize % 7) * 5,
                groups: 3 + (seed as usize % 5),
                ..SampleSpec::default()
            };
            let mut b = sample::budget(&spec);
            if seed % 3 == 0 {
                b.arithmetic = crate::model::Arithmetic::Exact;
            }
            // Half the budgets pay people across accounts (entry 79).
            if seed % 2 == 0 {
                sample::persons(&mut b, seed);
            }
            b.fringe_posting = match seed % 4 {
                0 => FringePosting::Budget,
                1 => FringePosting::ProductionLevel,
                2 => FringePosting::Account,
                _ => FringePosting::Category {
                    suffix: "99".into(),
                },
            };
            same(&b, &RecalcOptions::default(), &format!("seed {seed}"));

            let groups: Vec<String> = b.groups.keys().cloned().collect();
            let exclude = RecalcOptions {
                exclude_groups: groups.iter().take(2).cloned().collect(),
                only_groups: None,
            };
            same(&b, &exclude, &format!("seed {seed} exclude"));
            let only = RecalcOptions {
                exclude_groups: BTreeSet::new(),
                only_groups: Some(groups.iter().skip(1).take(3).cloned().collect()),
            };
            same(&b, &only, &format!("seed {seed} only"));

            // No group left out at all: every account takes the shortcut.
            let mut all_in = b.clone();
            for g in all_in.groups.values_mut() {
                g.in_budget_total = true;
            }
            same(
                &all_in,
                &RecalcOptions::default(),
                &format!("seed {seed} all in"),
            );

            // Random edits, some of which break the budget.
            let mut rng = Rng(seed.wrapping_mul(7919) | 1);
            for step in 0..25 {
                let mut e = b.clone();
                let n = all_details(&mut e).len() as u64;
                for _ in 0..1 + rng.below(4) {
                    let k = rng.below(n) as usize;
                    let pick = rng.below(11);
                    let fringes: Vec<String> = e.fringes.keys().cloned().collect();
                    let d = &mut all_details(&mut e)[k];
                    match pick {
                        0 => d.rate = format!("{}", rng.below(90_000)).into(),
                        1 => d.amount = "SHOOT_DAYS * 2".into(),
                        2 => d.amount = "NOPE".into(),
                        3 => d.rate = "1 +".into(),
                        4 => {
                            d.groups.insert("GHOST".into());
                        }
                        5 => {
                            d.fringes.insert("NOFRINGE".into());
                        }
                        6 => d.fringe_range = Some(rng.below(3) as u32 + 1),
                        9 | 10 => {
                            d.person = Some(
                                ["Dana Reyes", "Lee Park", " Dana Reyes"][rng.below(3) as usize]
                                    .into(),
                            );
                        }
                        7 => {
                            if let Some(f) = fringes.get(rng.below(fringes.len() as u64) as usize) {
                                d.fringes.insert(f.clone());
                            }
                        }
                        _ => d.rate = "99999999999999999".into(),
                    }
                }
                if step % 5 == 0 {
                    e.groups.insert(
                        "HOLD".into(),
                        Group {
                            short_code: None,
                            description: String::new(),
                            in_budget_total: false,
                            color: None,
                        },
                    );
                    let k = rng.below(n) as usize;
                    all_details(&mut e)[k].groups.insert("HOLD".into());
                }
                if step % 7 == 3 {
                    if let Some(f) = e.fringes.values_mut().next() {
                        f.rate = "MISSING_GLOBAL".into();
                    }
                }
                same(
                    &e,
                    &RecalcOptions::default(),
                    &format!("seed {seed} step {step}"),
                );
                same(&e, &exclude, &format!("seed {seed} step {step} exclude"));
            }
        }
    }

    /// Numbers on rounding edges, under every posting mode and filter: the
    /// optimized engine matches the old one to the cent, and the review's
    /// case (0.5 x 0.009999999) totals 0.00.
    #[test]
    fn rounding_edges_match_the_old_engine() {
        for seed in 1..=8u64 {
            let mut b = sample::budget(&SampleSpec {
                seed,
                rows: 250 + (seed as usize) * 41,
                accounts: 9 + seed as usize,
                fringes: 8,
                groups: 4,
                ..SampleSpec::default()
            });
            sample::fine_decimals(&mut b, seed);
            b.fringe_posting = match seed % 4 {
                0 => FringePosting::Budget,
                1 => FringePosting::ProductionLevel,
                2 => FringePosting::Account,
                _ => FringePosting::Category {
                    suffix: "99".into(),
                },
            };
            let groups: Vec<String> = b.groups.keys().cloned().collect();
            same(&b, &RecalcOptions::default(), &format!("seed {seed}"));
            let exclude = RecalcOptions {
                exclude_groups: groups.iter().take(1).cloned().collect(),
                only_groups: None,
            };
            same(&b, &exclude, &format!("seed {seed} exclude"));
        }

        let mut b = sample::budget(&SampleSpec {
            rows: 10,
            accounts: 1,
            categories: 1,
            ..SampleSpec::default()
        });
        let d = &mut all_details(&mut b)[0];
        **d = Detail::line(d.id, "edge", "0.5", "Allow", "0.009999999");
        let r = recalc(&b, &RecalcOptions::default()).unwrap();
        let row = &r.categories[0].accounts[0].rows[0];
        assert_eq!((row.local_total, row.total), (0, 0));
    }

    /// Flat fringes counted every way (each basis, caps, X and 4X in every
    /// kind of unit, stored cutoffs), under every posting mode: the
    /// optimized engine matches the old one to the cent.
    #[test]
    fn every_flat_basis_matches_the_old_engine() {
        for seed in 1..=8u64 {
            let mut b = sample::budget(&SampleSpec {
                seed,
                rows: 250 + (seed as usize) * 37,
                accounts: 9 + seed as usize,
                fringes: 15,
                groups: 4,
                ..SampleSpec::default()
            });
            sample::flat_bases(&mut b, seed);
            if seed % 3 == 0 {
                sample::fine_decimals(&mut b, seed);
            }
            b.fringe_posting = match seed % 4 {
                0 => FringePosting::Budget,
                1 => FringePosting::ProductionLevel,
                2 => FringePosting::Account,
                _ => FringePosting::Category {
                    suffix: "99".into(),
                },
            };
            same(&b, &RecalcOptions::default(), &format!("seed {seed}"));
            let groups: Vec<String> = b.groups.keys().cloned().collect();
            let exclude = RecalcOptions {
                exclude_groups: groups.iter().take(1).cloned().collect(),
                only_groups: None,
            };
            same(&b, &exclude, &format!("seed {seed} exclude"));
        }
    }

    /// The shortcut for plain numbers and bare globals agrees with parsing
    /// and evaluating, and leaves everything else to the parser.
    #[test]
    fn simple_formulas_match_the_parser() {
        let b = sample::budget(&SampleSpec {
            rows: 10,
            accounts: 2,
            categories: 1,
            ..SampleSpec::default()
        });
        let opts = RecalcOptions::default();
        let cases = [
            "0",
            "12",
            " 12 ",
            "\t1250.50\t",
            "007",
            ".5",
            "5.",
            "1.2.3",
            ".",
            "..",
            "1e5",
            "99999999999999999999999999999999",
            "123456789.1234567891",
            "SHOOT_DAYS",
            " SHOOT_DAYS ",
            "_X",
            "NOPE",
            "shoot_days",
            "SHOOT_DAYS2",
            "2SHOOT",
            "-5",
            "+5",
            "1 2",
            "12\n",
            "SHOOT DAYS",
            "(5)",
            "١٢",
            "5\u{a0}",
        ];
        for arith in crate::model::Arithmetic::ALL {
            let mut b = b.clone();
            b.arithmetic = arith;
            let ctx = Ctx::new(&b, &opts).unwrap();
            let dec = |n: &str| ctx.globals.get(n)?.shown();
            let float = |n: &str| Some(ctx.globals.get(n)?.f64());
            for src in cases {
                let full = Expr::parse(src).ok().and_then(|e| match arith {
                    crate::model::Arithmetic::Exact => e.eval(&dec).ok().map(Num::Dec),
                    crate::model::Arithmetic::Mmb => e.eval_f64(&float).ok().map(Num::F64),
                });
                let quick = match arith {
                    crate::model::Arithmetic::Exact => {
                        crate::expr::quick_value(src, &dec).map(Num::Dec)
                    }
                    crate::model::Arithmetic::Mmb => {
                        crate::expr::quick_value_f64(src, &float).map(Num::F64)
                    }
                };
                if let Some(v) = quick {
                    assert_eq!(Some(v), full, "{arith:?} {src:?}");
                }
                let via_eval = ctx.eval(&src.into(), Dec::ZERO, &|| String::new()).ok();
                if !src.trim().is_empty() {
                    assert_eq!(via_eval, full, "{arith:?} {src:?}");
                }
            }
            for (src, quick) in [("12", true), ("SHOOT_DAYS", true), ("NOPE", false)] {
                assert_eq!(
                    crate::expr::quick_value(src, &dec).is_some(),
                    quick,
                    "{src}"
                );
                assert_eq!(
                    crate::expr::quick_value_f64(src, &float).is_some(),
                    quick,
                    "{src}"
                );
            }
            assert!(crate::expr::quick_value("1.2.3", &dec).is_none());
            // More than 9 places: only Movie Magic arithmetic reads it.
            let precise = ctx.eval(&"0.0000000149".into(), Dec::ZERO, &|| String::new());
            match arith {
                crate::model::Arithmetic::Exact => assert!(precise.is_err()),
                crate::model::Arithmetic::Mmb => assert_eq!(precise, Ok(Num::F64(1.49e-8))),
            }
        }
    }

    #[test]
    fn row_kinds_are_still_handled() {
        // Sanity check that the reference covers every row kind the sample has.
        let b = sample::budget(&SampleSpec::default());
        let kinds: BTreeSet<_> = b
            .categories()
            .flat_map(|c| &c.accounts)
            .flat_map(|a| &a.details)
            .map(|d| format!("{:?}", d.kind))
            .collect();
        assert_eq!(kinds.len(), 4);
    }
}
