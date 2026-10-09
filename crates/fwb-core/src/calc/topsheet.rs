//! Stage 3: fringe posting, production levels, ATL/BTL, charges and credits.

use crate::calc::{err, overflow, sum, CalcError, Ctx, ErrorKind};
use crate::coding::AccountCoding;
use crate::decimal::{mul_cents, percent_of, Dec};
use crate::model::{
    Budget, Charge, ChargeKind, Credit, CreditCap, CreditSource, Detail, FringePosting, RowKind,
    TopsheetRow,
};
use crate::report::{
    CategoryTotals, ChargeTotals, CreditRowTotals, CreditTotals, LevelTotals, RowTotals,
};
use crate::Cents;

/// Everything above the account level.
pub(crate) struct Topsheet {
    /// The input categories with `AccountTotals::total`, `CategoryTotals::total`
    /// and `CategoryTotals::fringe_account` set for the posting mode.
    pub categories: Vec<CategoryTotals>,
    pub levels: Vec<LevelTotals>,
    pub budget_fringes: Cents,
    pub total_atl: Cents,
    pub total_btl: Cents,
    pub atl_btl: Cents,
    pub charges: Vec<ChargeTotals>,
    pub grand_total: Cents,
    pub credits: Vec<CreditTotals>,
    pub credits_total: Cents,
    pub net_total: Cents,
}

/// `categories` arrive with every row evaluated and fringed, sums filled in,
/// and every `total` equal to its `details_total`.
pub(crate) fn finish(
    ctx: &Ctx,
    mut categories: Vec<CategoryTotals>,
) -> Result<Topsheet, CalcError> {
    let ovf = || overflow("topsheet totals");
    let posting = &ctx.b.fringe_posting;
    let budget_fringes =
        post_fringes(posting, &ctx.b.account_coding, &mut categories).ok_or_else(ovf)?;

    let (levels, closed) = levels(ctx.b, posting, &categories).ok_or_else(ovf)?;
    // The first level is above-the-line only when a real production total ends it.
    let split = usize::from(closed > 0);
    let total_atl = sum(levels[..split].iter().map(|l| l.total)).ok_or_else(ovf)?;
    let total_btl = sum(levels[split..].iter().map(|l| l.total)).ok_or_else(ovf)?;
    let atl_btl = total_atl.checked_add(total_btl).ok_or_else(ovf)?;

    // Charges never stack: each one sees ATL + BTL only.
    let charges = ctx
        .b
        .charges
        .iter()
        .filter(|c| c.applied)
        .map(|c| charge(ctx, &categories, atl_btl, c))
        .collect::<Result<Vec<_>, _>>()?;
    let grand_total = sum(charges.iter().map(|c| c.amount))
        .and_then(|c| c.checked_add(atl_btl))
        .and_then(|t| t.checked_add(budget_fringes))
        .ok_or_else(ovf)?;

    let credits = ctx
        .b
        .credits
        .iter()
        .filter(|c| c.applied)
        .map(|c| credit(ctx, &categories, c))
        .collect::<Result<Vec<_>, _>>()?;
    let credits_total = sum(credits.iter().map(|c| c.amount)).ok_or_else(ovf)?;
    let net_total = grand_total.checked_sub(credits_total).ok_or_else(ovf)?;

    Ok(Topsheet {
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

/// Sets account and category totals for the posting mode. Returns the
/// budget-wide fringe row (`FringePosting::Budget`), else 0. `None` on overflow.
fn post_fringes(
    posting: &FringePosting,
    coding: &AccountCoding,
    cats: &mut [CategoryTotals],
) -> Option<Cents> {
    let mut budget_fringes: Cents = 0;
    for cat in cats.iter_mut() {
        match posting {
            FringePosting::Account => {
                for a in &mut cat.accounts {
                    a.total = a.details_total.checked_add(a.fringe_total)?;
                }
                cat.total = sum(cat.accounts.iter().map(|a| a.total))?;
            }
            FringePosting::Category { suffix } => {
                // Codes that don't fit the coding pattern fall back to plain concatenation.
                let number = coding
                    .fringe_account(&cat.code, suffix)
                    .unwrap_or_else(|_| format!("{}{suffix}", cat.code));
                cat.fringe_account = Some((number, cat.fringe_total));
                cat.total = cat.details_total.checked_add(cat.fringe_total)?;
            }
            FringePosting::ProductionLevel => {}
            FringePosting::Budget => {
                budget_fringes = budget_fringes.checked_add(cat.fringe_total)?;
            }
        }
    }
    Some(budget_fringes)
}

/// One level per production-total row, plus a trailing unnamed level when
/// categories follow the last one. Also returns how many levels were closed by
/// a real production total. `None` on overflow.
fn levels(
    b: &Budget,
    posting: &FringePosting,
    cats: &[CategoryTotals],
) -> Option<(Vec<LevelTotals>, usize)> {
    let posted = *posting == FringePosting::ProductionLevel;
    let level = |name: &str, start: usize, end: usize| -> Option<LevelTotals> {
        let slice = &cats[start..end];
        let posted_fringes = if posted {
            sum(slice.iter().map(|c| c.fringe_total))?
        } else {
            0
        };
        let total = sum(slice.iter().map(|c| c.total))?.checked_add(posted_fringes)?;
        Some(LevelTotals {
            name: name.to_string(),
            start,
            end,
            posted_fringes,
            total,
        })
    };

    let mut out = Vec::new();
    let mut start = 0;
    let mut idx = 0;
    for row in &b.topsheet {
        match row {
            // Clamped so a mismatched input can't index out of bounds.
            TopsheetRow::Category(_) => idx = (idx + 1).min(cats.len()),
            TopsheetRow::ProductionTotal(pt) => {
                out.push(level(&pt.name, start, idx)?);
                start = idx;
            }
        }
    }
    let closed = out.len();
    if start < cats.len() {
        out.push(level("", start, cats.len())?);
    }
    Some((out, closed))
}

/// A costed row with where it lives. Subtotal rows are skipped.
struct Line<'a> {
    category: &'a str,
    account: &'a str,
    detail: &'a Detail,
    row: &'a RowTotals,
}

/// Every non-subtotal detail of `b` with its evaluated row.
fn lines<'a>(b: &'a Budget, cats: &'a [CategoryTotals]) -> impl Iterator<Item = Line<'a>> {
    b.categories().zip(cats).flat_map(|(cat, ct)| {
        cat.accounts
            .iter()
            .zip(&ct.accounts)
            .flat_map(move |(acct, at)| {
                acct.details
                    .iter()
                    .zip(&at.rows)
                    .filter(|(d, _)| d.kind != RowKind::Subtotal)
                    .map(move |(detail, row)| Line {
                        category: &cat.code,
                        account: &acct.code,
                        detail,
                        row,
                    })
            })
    })
}

/// What a line contributes to ATL + BTL: its total, plus its fringes unless
/// they're posted budget-wide (outside ATL and BTL).
fn counted(posting: &FringePosting, r: &RowTotals) -> Option<Cents> {
    match posting {
        FringePosting::Budget => Some(r.total),
        _ => r.total.checked_add(r.fringe_total),
    }
}

fn charge(
    ctx: &Ctx,
    cats: &[CategoryTotals],
    atl_btl: Cents,
    c: &Charge,
) -> Result<ChargeTotals, CalcError> {
    let at = || format!("charge {}", c.name);
    let ovf = || overflow(at());

    // Flat fees exclude only groups (and manual amounts), per the reference.
    let by_code = matches!(c.kind, ChargeKind::Percent(_));
    let mut excluded = sum(c.excluded_amounts.iter().map(|(_, a)| *a)).ok_or_else(ovf)?;
    for l in lines(ctx.b, cats) {
        let hit = l
            .detail
            .groups
            .iter()
            .any(|g| c.excluded_groups.contains(g))
            || (by_code
                && (c.excluded_accounts.contains(l.account)
                    || c.excluded_categories.contains(l.category)));
        if hit {
            let v = counted(&ctx.b.fringe_posting, l.row).ok_or_else(ovf)?;
            excluded = excluded.checked_add(v).ok_or_else(ovf)?;
        }
    }

    let (base, amount) = match &c.kind {
        ChargeKind::Percent(p) => {
            let amount = atl_btl
                .checked_sub(excluded)
                .and_then(|n| percent_of(n, *p))
                .ok_or_else(ovf)?;
            (atl_btl, amount)
        }
        ChargeKind::Flat { amount, currency } => {
            let base = match currency {
                Some(cur) if *cur != ctx.b.base_currency => {
                    let rate = ctx
                        .b
                        .currencies
                        .get(cur)
                        .ok_or_else(|| err(at(), ErrorKind::UnknownCurrency(cur.clone())))?
                        .rate;
                    mul_cents(*amount, rate).ok_or_else(ovf)?
                }
                _ => *amount,
            };
            (base, base.checked_sub(excluded).ok_or_else(ovf)?)
        }
    };
    Ok(ChargeTotals {
        name: c.name.clone(),
        base,
        excluded,
        amount,
    })
}

fn credit(ctx: &Ctx, cats: &[CategoryTotals], c: &Credit) -> Result<CreditTotals, CalcError> {
    let ovf = || overflow(format!("credit {}", c.name));
    let hundred = Dec::from_int(100);

    let mut rows = Vec::with_capacity(c.rows.len());
    let mut total = Dec::ZERO;
    for (i, cr) in c.rows.iter().enumerate() {
        let at = || format!("credit {} row {} ({})", c.name, i + 1, cr.description);
        let row_ovf = || overflow(at());

        let (gross, excluded_fringes) = match &cr.source {
            CreditSource::Flat(g) => (*g, 0),
            CreditSource::Group {
                group,
                excluded_fringes,
            } => {
                if !ctx.b.groups.contains_key(group) {
                    return Err(err(at(), ErrorKind::UnknownGroup(group.clone())));
                }
                let mut gross: Cents = 0;
                let mut ex: Cents = 0;
                let tagged = lines(ctx.b, cats)
                    .filter(|l| l.row.included && l.detail.groups.contains(group));
                for l in tagged {
                    gross = gross
                        .checked_add(l.row.total)
                        .and_then(|g| g.checked_add(l.row.fringe_total))
                        .ok_or_else(row_ovf)?;
                    for (name, amt) in &l.row.fringes {
                        if excluded_fringes.contains(name) {
                            ex = ex.checked_add(*amt).ok_or_else(row_ovf)?;
                        }
                    }
                }
                (gross, ex)
            }
        };
        let net = gross.checked_sub(excluded_fringes).ok_or_else(row_ovf)?;

        // Unrounded: rows are summed, capped and discounted before rounding.
        let pct = |v: Dec| v.checked_mul(cr.percent)?.checked_div(hundred);
        let net_dec = Dec::from_cents(net);
        let credit = match cr.cap {
            CreditCap::None => pct(net_dec),
            CreditCap::QualifiedSpend(cap) => pct(net_dec.min(Dec::from_cents(cap))),
            CreditCap::Credit(cap) => pct(net_dec).map(|v| v.min(Dec::from_cents(cap))),
        }
        .ok_or_else(row_ovf)?;
        total = total.checked_add(credit).ok_or_else(ovf)?;
        rows.push(CreditRowTotals {
            gross,
            excluded_fringes,
            net,
            credit,
        });
    }

    let capped = match c.cap {
        Some(cap) => total.min(Dec::from_cents(cap)),
        None => total,
    };
    let amount = c
        .discount_percent
        .checked_div(hundred)
        .and_then(|d| Dec::ONE.checked_sub(d))
        .and_then(|keep| capped.checked_mul(keep))
        .and_then(Dec::to_cents)
        .ok_or_else(ovf)?;
    Ok(CreditTotals {
        name: c.name.clone(),
        rows,
        capped,
        amount,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::*;
    use crate::calc::{eval_categories, RecalcOptions};
    use crate::model::{Account, Category, CreditRow, Currency, Group, ProductionTotal};

    fn d(s: &str) -> Dec {
        s.parse().unwrap()
    }

    fn set<const N: usize>(items: [&str; N]) -> BTreeSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    fn line(id: u64, rate: i64, groups: &[&str]) -> Detail {
        let mut l = Detail::line(id, format!("line {id}"), 1, "Flat", rate);
        l.groups = groups.iter().map(|g| g.to_string()).collect();
        l
    }

    fn cat(code: &str, accounts: Vec<(&str, Vec<Detail>)>) -> TopsheetRow {
        TopsheetRow::Category(Category {
            code: code.into(),
            name: format!("Category {code}"),
            accounts: accounts
                .into_iter()
                .map(|(code, details)| Account {
                    code: code.into(),
                    name: format!("Account {code}"),
                    details,
                })
                .collect(),
        })
    }

    fn pt(name: &str) -> TopsheetRow {
        TopsheetRow::ProductionTotal(ProductionTotal { name: name.into() })
    }

    fn group(in_budget_total: bool) -> Group {
        Group {
            description: String::new(),
            in_budget_total,
        }
    }

    /// Details (cents):
    /// - 1000: 1100 row 1 = 1,000,000 (WGA 100,000); 1200 row 2 = 500,000, row 7 subtotal
    /// - Total Above-the-Line
    /// - 2000: 2100 row 3 = 300,000 TAX (FICA 22,950, UNION 30,000),
    ///   row 4 = 100,000 VFX (FICA 7,650); 2200 row 5 = 200,000 TAX
    /// - Total Production
    /// - 5000: 5100 row 6 = 50,000 VFX (FICA 5,000)
    fn fixture(posting: FringePosting) -> Budget {
        let mut b = Budget::new("Test", "USD");
        b.fringe_posting = posting;
        b.groups.insert("VFX".into(), group(true));
        b.groups.insert("TAX".into(), group(true));
        b.currencies.insert(
            "EUR".into(),
            Currency {
                description: "Euro".into(),
                symbol: "E".into(),
                rate: d("1.1"),
            },
        );
        b.topsheet = vec![
            cat(
                "1000",
                vec![
                    ("1100", vec![line(1, 10_000, &[])]),
                    (
                        "1200",
                        vec![line(2, 5_000, &[]), Detail::subtotal(7, "sub")],
                    ),
                ],
            ),
            pt("Total Above-the-Line"),
            cat(
                "2000",
                vec![
                    (
                        "2100",
                        vec![line(3, 3_000, &["TAX"]), line(4, 1_000, &["VFX"])],
                    ),
                    ("2200", vec![line(5, 2_000, &["TAX"])]),
                ],
            ),
            pt("Total Production"),
            cat("5000", vec![("5100", vec![line(6, 500, &["VFX"])])]),
        ];
        b
    }

    fn fixture_fringes() -> Vec<(u64, &'static str, Cents)> {
        vec![
            (1, "WGA", 100_000),
            (3, "FICA", 22_950),
            (3, "UNION", 30_000),
            (4, "FICA", 7_650),
            (6, "FICA", 5_000),
        ]
    }

    /// Simulates stage 2: puts `fringes` on rows by id, then re-sums accounts and categories.
    fn with_fringes(
        mut cats: Vec<CategoryTotals>,
        fringes: &[(u64, &str, Cents)],
    ) -> Vec<CategoryTotals> {
        for c in &mut cats {
            for a in &mut c.accounts {
                for r in &mut a.rows {
                    for (id, name, amt) in fringes {
                        if r.id == *id {
                            r.fringes.insert(name.to_string(), *amt);
                        }
                    }
                    r.fringe_total = r.fringes.values().sum();
                }
                a.fringe_total = a.rows.iter().map(|r| r.fringe_total).sum();
            }
            c.fringe_total = c.accounts.iter().map(|a| a.fringe_total).sum();
        }
        cats
    }

    fn run_with(
        b: &Budget,
        opts: &RecalcOptions,
        fringes: &[(u64, &str, Cents)],
    ) -> Result<Topsheet, CalcError> {
        let ctx = Ctx {
            b,
            globals: BTreeMap::new(),
            opts,
            unfiltered: false,
        };
        let cats = with_fringes(eval_categories(&ctx)?, fringes);
        finish(&ctx, cats)
    }

    fn run(b: &Budget) -> Topsheet {
        run_with(b, &RecalcOptions::default(), &fixture_fringes()).unwrap()
    }

    fn level_totals(ts: &Topsheet) -> Vec<(&str, usize, usize, Cents, Cents)> {
        ts.levels
            .iter()
            .map(|l| (l.name.as_str(), l.start, l.end, l.posted_fringes, l.total))
            .collect()
    }

    fn account_totals(ts: &Topsheet) -> Vec<Cents> {
        ts.categories
            .iter()
            .flat_map(|c| c.accounts.iter().map(|a| a.total))
            .collect()
    }

    fn charge(name: &str, kind: ChargeKind) -> Charge {
        Charge {
            name: name.into(),
            account: None,
            applied: true,
            kind,
            excluded_amounts: Vec::new(),
            excluded_groups: BTreeSet::new(),
            excluded_categories: BTreeSet::new(),
            excluded_accounts: BTreeSet::new(),
        }
    }

    fn credit(name: &str, rows: Vec<CreditRow>) -> Credit {
        Credit {
            name: name.into(),
            account: None,
            applied: true,
            cap: None,
            discount_percent: Dec::ZERO,
            rows,
        }
    }

    fn credit_row(source: CreditSource, percent: &str, cap: CreditCap) -> CreditRow {
        CreditRow {
            description: "row".into(),
            source,
            percent: d(percent),
            cap,
        }
    }

    #[test]
    fn account_posting_puts_fringes_in_accounts() {
        let ts = run(&fixture(FringePosting::Account));
        assert_eq!(
            account_totals(&ts),
            vec![1_100_000, 500_000, 460_600, 200_000, 55_000]
        );
        let cats: Vec<_> = ts
            .categories
            .iter()
            .map(|c| (c.total, c.fringe_account.clone()))
            .collect();
        assert_eq!(
            cats,
            vec![(1_600_000, None), (660_600, None), (55_000, None)]
        );
        assert_eq!(
            level_totals(&ts),
            vec![
                ("Total Above-the-Line", 0, 1, 0, 1_600_000),
                ("Total Production", 1, 2, 0, 660_600),
                ("", 2, 3, 0, 55_000),
            ]
        );
        assert_eq!(
            (ts.total_atl, ts.total_btl, ts.atl_btl),
            (1_600_000, 715_600, 2_315_600)
        );
        assert_eq!(ts.budget_fringes, 0);
        assert_eq!(ts.grand_total, 2_315_600);
        assert_eq!(ts.net_total, 2_315_600);
    }

    #[test]
    fn category_posting_adds_a_fringe_account() {
        let ts = run(&fixture(FringePosting::Category {
            suffix: "99".into(),
        }));
        assert_eq!(
            account_totals(&ts),
            vec![1_000_000, 500_000, 400_000, 200_000, 50_000]
        );
        let cats: Vec<_> = ts
            .categories
            .iter()
            .map(|c| (c.total, c.fringe_account.clone()))
            .collect();
        assert_eq!(
            cats,
            vec![
                (1_600_000, Some(("10-99".to_string(), 100_000))),
                (660_600, Some(("20-99".to_string(), 60_600))),
                (55_000, Some(("50-99".to_string(), 5_000))),
            ]
        );
        assert_eq!(
            (ts.total_atl, ts.total_btl, ts.atl_btl),
            (1_600_000, 715_600, 2_315_600)
        );
        assert_eq!((ts.budget_fringes, ts.grand_total), (0, 2_315_600));
    }

    #[test]
    fn category_posting_creates_zero_fringe_account() {
        let mut b = fixture(FringePosting::Category {
            suffix: "99".into(),
        });
        b.topsheet.push(cat("6000", vec![]));
        let ts = run(&b);
        assert_eq!(
            ts.categories[3].fringe_account,
            Some(("60-99".to_string(), 0))
        );
        assert_eq!(ts.categories[3].total, 0);
    }

    #[test]
    fn production_level_posting_adds_fringes_per_level() {
        let ts = run(&fixture(FringePosting::ProductionLevel));
        assert_eq!(
            account_totals(&ts),
            vec![1_000_000, 500_000, 400_000, 200_000, 50_000]
        );
        let cats: Vec<_> = ts
            .categories
            .iter()
            .map(|c| (c.total, c.fringe_account.clone()))
            .collect();
        assert_eq!(
            cats,
            vec![(1_500_000, None), (600_000, None), (50_000, None)]
        );
        assert_eq!(
            level_totals(&ts),
            vec![
                ("Total Above-the-Line", 0, 1, 100_000, 1_600_000),
                ("Total Production", 1, 2, 60_600, 660_600),
                ("", 2, 3, 5_000, 55_000),
            ]
        );
        assert_eq!(
            (ts.total_atl, ts.total_btl, ts.atl_btl),
            (1_600_000, 715_600, 2_315_600)
        );
        assert_eq!((ts.budget_fringes, ts.grand_total), (0, 2_315_600));
    }

    #[test]
    fn budget_posting_keeps_fringes_outside_atl_btl() {
        let ts = run(&fixture(FringePosting::Budget));
        assert_eq!(
            account_totals(&ts),
            vec![1_000_000, 500_000, 400_000, 200_000, 50_000]
        );
        let cats: Vec<_> = ts
            .categories
            .iter()
            .map(|c| (c.total, c.fringe_account.clone()))
            .collect();
        assert_eq!(
            cats,
            vec![(1_500_000, None), (600_000, None), (50_000, None)]
        );
        assert!(ts.levels.iter().all(|l| l.posted_fringes == 0));
        assert_eq!(
            (ts.total_atl, ts.total_btl, ts.atl_btl),
            (1_500_000, 650_000, 2_150_000)
        );
        assert_eq!(ts.budget_fringes, 165_600);
        assert_eq!(ts.grand_total, 2_315_600);
    }

    #[test]
    fn no_production_totals_is_all_btl() {
        let mut b = fixture(FringePosting::Account);
        b.topsheet.retain(|r| matches!(r, TopsheetRow::Category(_)));
        let ts = run(&b);
        assert_eq!(level_totals(&ts), vec![("", 0, 3, 0, 2_315_600)]);
        assert_eq!(
            (ts.total_atl, ts.total_btl, ts.atl_btl),
            (0, 2_315_600, 2_315_600)
        );
    }

    #[test]
    fn no_trailing_level_when_topsheet_ends_with_a_production_total() {
        let mut b = fixture(FringePosting::Account);
        b.topsheet.push(pt("Total Other"));
        let ts = run(&b);
        assert_eq!(
            level_totals(&ts),
            vec![
                ("Total Above-the-Line", 0, 1, 0, 1_600_000),
                ("Total Production", 1, 2, 0, 660_600),
                ("Total Other", 2, 3, 0, 55_000),
            ]
        );
        assert_eq!((ts.total_atl, ts.total_btl), (1_600_000, 715_600));
    }

    #[test]
    fn leading_and_back_to_back_production_totals_make_empty_levels() {
        let mut b = fixture(FringePosting::Account);
        b.topsheet.insert(0, pt("Nothing"));
        b.topsheet.insert(3, pt("Also nothing"));
        let ts = run(&b);
        assert_eq!(
            level_totals(&ts),
            vec![
                ("Nothing", 0, 0, 0, 0),
                ("Total Above-the-Line", 0, 1, 0, 1_600_000),
                ("Also nothing", 1, 1, 0, 0),
                ("Total Production", 1, 2, 0, 660_600),
                ("", 2, 3, 0, 55_000),
            ]
        );
        // The empty first level is ATL; everything else is BTL.
        assert_eq!(
            (ts.total_atl, ts.total_btl, ts.atl_btl),
            (0, 2_315_600, 2_315_600)
        );
    }

    #[test]
    fn empty_topsheet() {
        let ts = run(&Budget::new("Empty", "USD"));
        assert!(ts.levels.is_empty());
        assert_eq!(
            (ts.total_atl, ts.total_btl, ts.grand_total, ts.net_total),
            (0, 0, 0, 0)
        );
    }

    fn percent_charge() -> Charge {
        let mut c = charge("Contingency", ChargeKind::Percent(d("10")));
        c.excluded_amounts = vec![("Deposit".into(), 15_600)];
        c.excluded_accounts = set(["1200"]);
        // Row 6 is in category 5000 and group VFX: counted once.
        c.excluded_categories = set(["5000"]);
        c.excluded_groups = set(["VFX"]);
        c
    }

    #[test]
    fn percent_charge_excludes_each_line_once() {
        let mut b = fixture(FringePosting::Account);
        b.charges = vec![percent_charge()];
        let ts = run(&b);
        // 15,600 + row 2 500,000 + row 4 (100,000 + 7,650) + row 6 (50,000 + 5,000)
        assert_eq!(
            ts.charges,
            vec![ChargeTotals {
                name: "Contingency".into(),
                base: 2_315_600,
                excluded: 678_250,
                amount: 163_735
            }]
        );
        assert_eq!(ts.grand_total, 2_315_600 + 163_735);
        assert_eq!(ts.net_total, ts.grand_total);
    }

    #[test]
    fn percent_charge_counts_fringes_only_inside_atl_btl() {
        for posting in [
            FringePosting::Category {
                suffix: "99".into(),
            },
            FringePosting::ProductionLevel,
        ] {
            let mut b = fixture(posting);
            b.charges = vec![percent_charge()];
            let ts = run(&b);
            assert_eq!(
                (ts.charges[0].excluded, ts.charges[0].amount),
                (678_250, 163_735)
            );
        }
        let mut b = fixture(FringePosting::Budget);
        b.charges = vec![percent_charge()];
        let ts = run(&b);
        // Fringes sit outside ATL + BTL, so excluded lines count without them.
        assert_eq!(
            ts.charges,
            vec![ChargeTotals {
                name: "Contingency".into(),
                base: 2_150_000,
                excluded: 665_600,
                amount: 148_440
            }]
        );
        assert_eq!(ts.grand_total, 2_150_000 + 165_600 + 148_440);
    }

    #[test]
    fn charges_do_not_stack_and_unapplied_ones_are_ignored() {
        let mut b = fixture(FringePosting::Account);
        let mut unapplied = charge("Unused", ChargeKind::Percent(d("50")));
        unapplied.applied = false;
        b.charges = vec![
            percent_charge(),
            unapplied,
            charge("Fee", ChargeKind::Percent(d("5"))),
        ];
        let ts = run(&b);
        let names: Vec<_> = ts.charges.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["Contingency", "Fee"]);
        assert_eq!(
            ts.charges[1],
            ChargeTotals {
                name: "Fee".into(),
                base: 2_315_600,
                excluded: 0,
                amount: 115_780
            }
        );
        assert_eq!(ts.grand_total, 2_315_600 + 163_735 + 115_780);
    }

    #[test]
    fn flat_charge_in_foreign_currency_excludes_amounts_and_groups_only() {
        let mut b = fixture(FringePosting::Account);
        let mut c = charge(
            "Completion bond",
            ChargeKind::Flat {
                amount: 1_000_000,
                currency: Some("EUR".into()),
            },
        );
        c.excluded_amounts = vec![("Waived".into(), 1_000)];
        c.excluded_groups = set(["VFX"]);
        // Ignored for flat fees.
        c.excluded_accounts = set(["1100"]);
        c.excluded_categories = set(["2000"]);
        b.charges = vec![c];
        let ts = run(&b);
        // 1,000,000 EUR cents x 1.1; excluded 1,000 + row 4 107,650 + row 6 55,000
        assert_eq!(
            ts.charges,
            vec![ChargeTotals {
                name: "Completion bond".into(),
                base: 1_100_000,
                excluded: 163_650,
                amount: 936_350
            }]
        );
        assert_eq!(ts.grand_total, 2_315_600 + 936_350);
    }

    #[test]
    fn flat_charge_in_base_currency_is_not_converted() {
        let mut b = fixture(FringePosting::Account);
        b.charges = vec![
            charge(
                "A",
                ChargeKind::Flat {
                    amount: 12_345,
                    currency: Some("USD".into()),
                },
            ),
            charge(
                "B",
                ChargeKind::Flat {
                    amount: 500,
                    currency: None,
                },
            ),
        ];
        let ts = run(&b);
        let got: Vec<_> = ts
            .charges
            .iter()
            .map(|c| (c.base, c.excluded, c.amount))
            .collect();
        assert_eq!(got, vec![(12_345, 0, 12_345), (500, 0, 500)]);
    }

    #[test]
    fn flat_charge_in_unknown_currency_is_an_error() {
        let mut b = fixture(FringePosting::Account);
        b.charges = vec![charge(
            "Bond",
            ChargeKind::Flat {
                amount: 100,
                currency: Some("XYZ".into()),
            },
        )];
        let e = run_with(&b, &RecalcOptions::default(), &[]).err().unwrap();
        assert_eq!(e.at, "charge Bond");
        assert_eq!(e.kind, ErrorKind::UnknownCurrency("XYZ".into()));
    }

    #[test]
    fn charge_overflow_is_an_error() {
        let mut b = fixture(FringePosting::Account);
        b.charges = vec![charge(
            "Huge",
            ChargeKind::Flat {
                amount: Cents::MAX,
                currency: Some("EUR".into()),
            },
        )];
        let e = run_with(&b, &RecalcOptions::default(), &[]).err().unwrap();
        assert_eq!(
            (e.at.as_str(), e.kind),
            ("charge Huge", ErrorKind::Overflow)
        );

        b.charges = vec![
            charge(
                "Max",
                ChargeKind::Flat {
                    amount: Cents::MAX,
                    currency: None,
                },
            ),
            charge(
                "More",
                ChargeKind::Flat {
                    amount: Cents::MAX,
                    currency: None,
                },
            ),
        ];
        let e = run_with(&b, &RecalcOptions::default(), &[]).err().unwrap();
        assert_eq!(
            (e.at.as_str(), e.kind),
            ("topsheet totals", ErrorKind::Overflow)
        );
    }

    #[test]
    fn credits_with_each_cap_type() {
        let mut b = fixture(FringePosting::Account);
        let tax = CreditSource::Group {
            group: "TAX".into(),
            excluded_fringes: set(["UNION"]),
        };
        b.credits = vec![credit(
            "State incentive",
            vec![
                // TAX: rows 3 and 5 = 300,000 + 52,950 + 200,000; less UNION 30,000
                credit_row(tax, "25", CreditCap::None),
                credit_row(
                    CreditSource::Flat(1_000_000),
                    "20",
                    CreditCap::QualifiedSpend(400_000),
                ),
                credit_row(
                    CreditSource::Flat(1_000_000),
                    "30",
                    CreditCap::Credit(250_000),
                ),
            ],
        )];
        let ts = run(&b);
        let c = &ts.credits[0];
        assert_eq!(
            c.rows,
            vec![
                CreditRowTotals {
                    gross: 552_950,
                    excluded_fringes: 30_000,
                    net: 522_950,
                    credit: d("1307.375")
                },
                CreditRowTotals {
                    gross: 1_000_000,
                    excluded_fringes: 0,
                    net: 1_000_000,
                    credit: d("800")
                },
                CreditRowTotals {
                    gross: 1_000_000,
                    excluded_fringes: 0,
                    net: 1_000_000,
                    credit: d("2500")
                },
            ]
        );
        assert_eq!(c.capped, d("4607.375"));
        assert_eq!(c.amount, 460_738); // rounded once, half away from zero
        assert_eq!(ts.credits_total, 460_738);
        assert_eq!(ts.net_total, 2_315_600 - 460_738);
    }

    #[test]
    fn credit_cap_cuts_the_sum_then_discount_applies() {
        let mut b = fixture(FringePosting::Account);
        let mut c = credit(
            "Capped",
            vec![
                credit_row(CreditSource::Flat(500_000), "40", CreditCap::None),
                credit_row(CreditSource::Flat(500_000), "40", CreditCap::None),
            ],
        );
        c.cap = Some(300_000);
        c.discount_percent = d("5");
        let mut unapplied = credit(
            "Unused",
            vec![credit_row(
                CreditSource::Flat(1_000),
                "100",
                CreditCap::None,
            )],
        );
        unapplied.applied = false;
        let mut discounted = credit(
            "Brokered",
            vec![credit_row(
                CreditSource::Flat(1_843_475),
                "25",
                CreditCap::None,
            )],
        );
        discounted.discount_percent = d("10");
        b.credits = vec![c, unapplied, discounted];
        let ts = run(&b);
        let got: Vec<_> = ts
            .credits
            .iter()
            .map(|c| (c.name.as_str(), c.capped, c.amount))
            .collect();
        // 4,608.6875 x 0.9 = 4,147.81875
        assert_eq!(
            got,
            vec![
                ("Capped", d("3000"), 285_000),
                ("Brokered", d("4608.6875"), 414_782)
            ]
        );
        assert_eq!(ts.credits_total, 285_000 + 414_782);
        assert_eq!(ts.net_total, ts.grand_total - 285_000 - 414_782);
    }

    #[test]
    fn credit_rows_are_summed_unrounded() {
        let mut b = fixture(FringePosting::Account);
        b.credits = vec![credit(
            "Halves",
            vec![
                credit_row(CreditSource::Flat(1_001), "50", CreditCap::None),
                credit_row(CreditSource::Flat(1_001), "50", CreditCap::None),
            ],
        )];
        let ts = run(&b);
        let c = &ts.credits[0];
        assert_eq!(c.rows[0].credit, d("5.005"));
        assert_eq!(c.rows[1].credit, d("5.005"));
        // Rounding each row first would give 501 + 501 = 1,002.
        assert_eq!(c.amount, 1_001);
    }

    #[test]
    fn credit_group_skips_lines_that_dont_count() {
        let mut b = fixture(FringePosting::Account);
        b.credits = vec![credit(
            "Tax",
            vec![credit_row(
                CreditSource::Group {
                    group: "TAX".into(),
                    excluded_fringes: BTreeSet::new(),
                },
                "100",
                CreditCap::None,
            )],
        )];
        // Row 3 is TAX and VFX-free; tag row 5 with VFX too and filter VFX out.
        for c in b.categories_mut() {
            for a in &mut c.accounts {
                for dl in &mut a.details {
                    if dl.id == 5 {
                        dl.groups.insert("VFX".into());
                    }
                }
            }
        }
        let opts = RecalcOptions {
            exclude_groups: set(["VFX"]),
            only_groups: None,
        };
        // A stray fringe on the excluded row must not count either.
        let mut fringes = fixture_fringes();
        fringes.push((5, "FICA", 999));
        let ts = run_with(&b, &opts, &fringes).unwrap();
        assert_eq!(ts.credits[0].rows[0].gross, 300_000 + 52_950);
    }

    #[test]
    fn credit_with_unknown_group_is_an_error() {
        let mut b = fixture(FringePosting::Account);
        b.credits = vec![credit(
            "Tax",
            vec![credit_row(
                CreditSource::Group {
                    group: "NOPE".into(),
                    excluded_fringes: BTreeSet::new(),
                },
                "25",
                CreditCap::None,
            )],
        )];
        let e = run_with(&b, &RecalcOptions::default(), &[]).err().unwrap();
        assert_eq!(e.at, "credit Tax row 1 (row)");
        assert_eq!(e.kind, ErrorKind::UnknownGroup("NOPE".into()));
    }

    #[test]
    fn grand_total_is_the_same_in_every_posting_mode() {
        for posting in [
            FringePosting::Account,
            FringePosting::Category {
                suffix: "99".into(),
            },
            FringePosting::ProductionLevel,
            FringePosting::Budget,
        ] {
            let ts = run(&fixture(posting));
            assert_eq!(ts.grand_total, 2_315_600);
            let levels = sum(ts.levels.iter().map(|l| l.total)).unwrap();
            assert_eq!(levels + ts.budget_fringes, ts.grand_total);
        }
    }
}
