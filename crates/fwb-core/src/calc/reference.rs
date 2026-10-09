//! The engine as it was before the speed work, kept for tests only: percent
//! fringes visit every fringe the account uses for every earner, and the
//! "full" group totals come from a second pass over every account. The
//! optimized engine must give exactly the same reports and errors.

use std::collections::{BTreeMap, BTreeSet};

use crate::calc::{
    breakdown, err, eval_globals, lines, overflow, row_at, sum, topsheet, CalcError, Ctx,
    ErrorKind, RecalcOptions,
};
use crate::decimal::{percent_of, Dec};
use crate::model::{Account, Budget, Detail, Fringe, FringeKind, RowKind};
use crate::report::{AccountTotals, CategoryTotals, Report, RowTotals};
use crate::Cents;

/// The old stage 2.
fn apply_fringes(ctx: &Ctx, acct: &Account, rows: &mut [RowTotals]) -> Result<(), CalcError> {
    let details = &acct.details;
    for r in rows.iter_mut() {
        r.fringes.clear();
        r.fringe_total = 0;
    }
    let mut rates: BTreeMap<&str, (&Fringe, Dec)> = BTreeMap::new();
    for (i, d) in details.iter().enumerate() {
        if d.kind != RowKind::Line {
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
        .map(|(d, r)| d.kind == RowKind::Line && r.included)
        .collect();
    for earner in earners(details, rows) {
        for (&name, &(f, rate)) in &rates {
            let FringeKind::Percent { cutoff } = f.kind else {
                continue;
            };
            let lines: Vec<usize> = earner
                .iter()
                .copied()
                .filter(|&i| earns[i] && details[i].fringes.contains(name))
                .collect();
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
            let weights: Vec<Cents> = lines.iter().map(|&i| rows[i].total).collect();
            let mut base = sum(weights.iter().copied()).ok_or_else(ovf)?;
            if let Some(c) = cutoff {
                base = base.min(c);
            }
            let amount = percent_of(base, rate).ok_or_else(ovf)?;
            let shares = super::fringes::allocate(amount, &weights).ok_or_else(ovf)?;
            for (&i, share) in lines.iter().zip(shares) {
                rows[i].fringes.insert(name.to_string(), share);
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
            let FringeKind::Flat { unit } = &f.kind else {
                continue;
            };
            let amount =
                super::fringes::flat_amount(ctx, unit.as_deref(), &d.unit, rate, rows[i].quantity)
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
    let full: Vec<Vec<Option<Vec<RowTotals>>>> = eval_categories(&full_ctx)?
        .into_iter()
        .map(|c| c.accounts.into_iter().map(|a| Some(a.rows)).collect())
        .collect();
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
                    let pick = rng.below(9);
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
        let ctx = Ctx {
            b: &b,
            globals: eval_globals(&b).unwrap(),
            opts: &opts,
            unfiltered: false,
        };
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
        for src in cases {
            let full = Expr::parse(src)
                .ok()
                .and_then(|e| e.eval(&|n| ctx.globals.get(n).copied()).ok());
            if let Some(v) = ctx.eval_simple(src) {
                assert_eq!(Some(v), full, "{src:?}");
            }
            let via_eval = ctx.eval(&src.into(), Dec::ZERO, &|| String::new()).ok();
            if !src.trim().is_empty() {
                assert_eq!(via_eval, full, "{src:?}");
            }
        }
        assert!(ctx.eval_simple("12").is_some());
        assert!(ctx.eval_simple("SHOOT_DAYS").is_some());
        assert!(ctx.eval_simple("NOPE").is_none());
        assert!(ctx.eval_simple("1.2.3").is_none());
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
