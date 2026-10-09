//! Property tests over randomly generated budgets.
//!
//! Uses a small seeded generator instead of a property-testing crate, so the
//! engine stays dependency-free. Each failure prints its seed for replay.

mod common;

use common::*;
use fwb_core::*;

const CASES: u64 = 400;

/// xorshift64*: tiny, deterministic, good enough for test data.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn chance(&mut self, pct: u64) -> bool {
        self.below(100) < pct
    }

    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.below(items.len() as u64) as usize]
    }
}

fn gen_detail(rng: &mut Rng, id: RowId, range: Option<u32>) -> Detail {
    let mut det = match rng.below(11) {
        0 => return Detail::subtotal(id, "sub"),
        1 => Detail::adjustment(id, "adj", format!("0.{}", rng.below(30)).as_str()),
        // Separators sometimes sit inside a fringe range, sometimes carry a
        // different range or stray tags: none of it may matter.
        2 => {
            let mut sep = Detail::separator(id);
            sep.fringe_range = match rng.below(3) {
                0 => range,
                1 => Some(u32::MAX),
                _ => None,
            };
            if rng.chance(30) {
                sep.fringes.insert("PCT".into());
            }
            sep
        }
        _ => {
            let amount = match rng.below(3) {
                0 => "DAYS".to_string(),
                1 => "WEEKS".to_string(),
                _ => rng.below(50).to_string(),
            };
            let rate = format!("{}.{:02}", rng.below(5_000), rng.below(100));
            let unit = rng.pick(&["Day", "Week", "Hour", "Flat", ""]);
            let mut det = Detail::line(id, "line", amount.as_str(), unit, rate.as_str());
            det.x = (rng.below(3) as i64 + 1).into();
            if rng.chance(20) {
                det.x4 = (rng.below(4) as i64 + 1).into();
            }
            det.currency = match rng.below(5) {
                0 => Some("CAD".into()),
                1 => Some("GBP".into()),
                _ => None,
            };
            for f in ["PCT", "CAPPED", "FLAT", "DAILY"] {
                if rng.chance(30) {
                    det.fringes.insert(f.into());
                }
            }
            det.fringe_range = range;
            det
        }
    };
    for g in ["A", "B", "OFF"] {
        if rng.chance(20) {
            det.groups.insert(g.into());
        }
    }
    if rng.chance(30) {
        det.set = Some(rng.pick(&["S1", "S2"]).into());
    }
    // Display-only styling, on any kind of row.
    det.bold = rng.chance(20);
    det.italic = rng.chance(20);
    det.indent = rng.below(5) as u8;
    if rng.chance(20) {
        det.color = Some("#c04000".into());
    }
    if rng.chance(10) {
        det.note = Some("check with UPM".into());
    }
    det
}

fn gen_budget(rng: &mut Rng) -> Budget {
    let mut b = Budget::new("random", "USD");
    for (code, rate) in [("CAD", "0.73"), ("GBP", "1.27")] {
        b.currencies.insert(
            code.into(),
            Currency {
                description: String::new(),
                symbol: String::new(),
                rate: d(rate),
            },
        );
    }
    b.globals
        .insert("DAYS".into(), (rng.below(60) as i64 + 1).into());
    b.globals.insert("WEEKS".into(), "DAYS / 5".into());
    let fringe = |rate: &str, kind| Fringe {
        description: String::new(),
        rate: rate.into(),
        kind,
    };
    b.fringes.insert(
        "PCT".into(),
        fringe("18.5", FringeKind::Percent { cutoff: None }),
    );
    b.fringes.insert(
        "CAPPED".into(),
        fringe(
            "7.65",
            FringeKind::Percent {
                cutoff: Some(rng.below(3_000_000) as i64),
            },
        ),
    );
    b.fringes
        .insert("FLAT".into(), fringe("25", FringeKind::Flat { unit: None }));
    b.fringes.insert(
        "DAILY".into(),
        fringe(
            "12.5",
            FringeKind::Flat {
                unit: Some("Day".into()),
            },
        ),
    );
    b.fringe_posting = match rng.below(4) {
        0 => FringePosting::Budget,
        1 => FringePosting::ProductionLevel,
        2 => FringePosting::Account,
        _ => FringePosting::Category {
            suffix: "99".into(),
        },
    };
    b.groups.insert("A".into(), group(true));
    b.groups.insert("B".into(), group(true));
    b.groups.insert("OFF".into(), group(false));
    if rng.chance(50) {
        b.groups.get_mut("A").unwrap().color = Some("#2060a0".into());
    }

    let (mut id, mut code, mut range) = (0, 1000, 0);
    for c in 0..=rng.below(5) {
        let mut accounts = Vec::new();
        for _ in 0..=rng.below(4) {
            code += 10;
            let mut details = Vec::new();
            let mut current = None;
            for _ in 0..rng.below(8) {
                // Ranges are contiguous blocks, each with its own number.
                if rng.chance(25) {
                    range += 1;
                    current = if rng.chance(50) { Some(range) } else { None };
                }
                id += 1;
                details.push(gen_detail(rng, id, current));
            }
            accounts.push(account(&code.to_string(), "acct", details));
        }
        b.topsheet
            .push(category(&format!("{c}000"), "cat", accounts));
        if rng.chance(50) {
            b.topsheet.push(level("level"));
        }
    }

    let mut charge = percent_charge("Contingency", "10");
    charge.excluded_groups = set(&["B"]);
    charge.excluded_accounts = set(&["1010"]);
    b.charges.push(charge);
    b.charges.push(Charge {
        kind: ChargeKind::Flat {
            amount: 250_000,
            currency: Some("GBP".into()),
        },
        ..percent_charge("Fee", "0")
    });
    b.credits.push(Credit {
        name: "Credit".into(),
        account: None,
        applied: true,
        cap: Some(rng.below(2_000_000) as i64),
        discount_percent: d("5"),
        rows: vec![CreditRow {
            description: String::new(),
            source: CreditSource::Group {
                group: "A".into(),
                excluded_fringes: set(&["FLAT"]),
            },
            percent: d("25"),
            cap: CreditCap::QualifiedSpend(rng.below(5_000_000) as i64),
        }],
    });
    b
}

fn check_rollups(b: &Budget, r: &Report, seed: u64) {
    let msg = format!("seed {seed}");
    let posting = &b.fringe_posting;
    let mut all_fringes = 0;
    let mut posted = r.budget_fringes;
    for cat in &r.categories {
        for acct in &cat.accounts {
            for row in &acct.rows {
                assert_eq!(row.fringe_total, row.fringes.values().sum::<i64>(), "{msg}");
                if !row.included || row.kind != RowKind::Line {
                    assert_eq!(row.fringe_total, 0, "{msg}");
                }
                if row.kind == RowKind::Separator {
                    assert!(row.included, "{msg}");
                    assert_eq!((row.total, row.subtotal), (0, None), "{msg}");
                }
                if !row.included {
                    assert_eq!(row.total, 0, "{msg}");
                }
                all_fringes += row.fringe_total;
            }
            assert_eq!(
                acct.details_total,
                acct.rows.iter().map(|r| r.total).sum::<i64>(),
                "{msg}"
            );
            assert_eq!(
                acct.fringe_total,
                acct.rows.iter().map(|r| r.fringe_total).sum::<i64>(),
                "{msg}"
            );
            let expect = match posting {
                FringePosting::Account => acct.details_total + acct.fringe_total,
                _ => acct.details_total,
            };
            assert_eq!(acct.total, expect, "{msg}");
            if *posting == FringePosting::Account {
                posted += acct.fringe_total;
            }
        }
        let fa = cat.fringe_account.as_ref().map_or(0, |f| f.1);
        posted += fa;
        assert_eq!(
            cat.total,
            cat.accounts.iter().map(|a| a.total).sum::<i64>() + fa,
            "{msg}"
        );
        assert_eq!(
            cat.fringe_total,
            cat.accounts.iter().map(|a| a.fringe_total).sum::<i64>(),
            "{msg}"
        );
    }
    for l in &r.levels {
        posted += l.posted_fringes;
        let cats: i64 = r.categories[l.start..l.end].iter().map(|c| c.total).sum();
        assert_eq!(l.total, cats + l.posted_fringes, "{msg}");
    }
    assert_eq!(
        posted, all_fringes,
        "every fringe dollar posts exactly once, {msg}"
    );
    assert_eq!(
        r.fringes.values().map(|f| f.total).sum::<i64>(),
        all_fringes,
        "{msg}"
    );
    assert_eq!(
        r.total_atl + r.total_btl,
        r.levels.iter().map(|l| l.total).sum::<i64>(),
        "{msg}"
    );
    assert_eq!(r.atl_btl, r.total_atl + r.total_btl, "{msg}");
    let charges: i64 = r.charges.iter().map(|c| c.amount).sum();
    assert_eq!(
        r.grand_total,
        r.atl_btl + r.budget_fringes + charges,
        "{msg}"
    );
    assert_eq!(
        r.credits_total,
        r.credits.iter().map(|c| c.amount).sum::<i64>(),
        "{msg}"
    );
    assert_eq!(r.net_total, r.grand_total - r.credits_total, "{msg}");
}

#[test]
fn rollups_always_agree() {
    for seed in 1..=CASES {
        let b = gen_budget(&mut Rng(seed));
        for opts in [
            RecalcOptions::default(),
            RecalcOptions {
                exclude_groups: set(&["A"]),
                ..Default::default()
            },
            RecalcOptions {
                only_groups: Some(set(&["B"])),
                ..Default::default()
            },
        ] {
            let r = recalc(&b, &opts).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
            check_rollups(&b, &r, seed);
        }
    }
}

#[test]
fn excluding_a_group_equals_deleting_its_lines() {
    for seed in 1..=CASES {
        let b = gen_budget(&mut Rng(seed));
        let opts = RecalcOptions {
            exclude_groups: set(&["A"]),
            ..Default::default()
        };
        let filtered = recalc(&b, &opts).unwrap();

        let mut pruned = b.clone();
        for acct in pruned.categories_mut().flat_map(|c| &mut c.accounts) {
            acct.details
                .retain(|d| d.kind == RowKind::Subtotal || !d.groups.contains("A"));
        }
        let direct = recalc(&pruned, &RecalcOptions::default()).unwrap();

        assert_eq!(filtered.atl_btl, direct.atl_btl, "seed {seed}");
        assert_eq!(filtered.grand_total, direct.grand_total, "seed {seed}");
        assert_eq!(filtered.charges, direct.charges, "seed {seed}");
        assert_eq!(
            filtered.fringes,
            direct
                .fringes
                .iter()
                .map(|(k, v)| {
                    // Uses count every tagged line, filtered or not.
                    (
                        k.clone(),
                        FringeSummary {
                            uses: filtered.fringes[k].uses,
                            total: v.total,
                        },
                    )
                })
                .collect(),
            "seed {seed}"
        );
    }
}

#[test]
fn suppressed_group_equals_excluding_it() {
    for seed in 1..=CASES {
        let mut b = gen_budget(&mut Rng(seed));
        b.groups.insert("OFF".into(), group(true));
        let opts = RecalcOptions {
            exclude_groups: set(&["OFF"]),
            ..Default::default()
        };
        let excluded = recalc(&b, &opts).unwrap();
        b.groups.insert("OFF".into(), group(false));
        let suppressed = recalc(&b, &RecalcOptions::default()).unwrap();
        assert_eq!(excluded.categories, suppressed.categories, "seed {seed}");
        assert_eq!(excluded.net_total, suppressed.net_total, "seed {seed}");
    }
}

#[test]
fn full_group_totals_ignore_filters() {
    for seed in 1..=CASES {
        let b = gen_budget(&mut Rng(seed));
        let plain = recalc(&b, &RecalcOptions::default()).unwrap();
        let filtered = recalc(
            &b,
            &RecalcOptions {
                exclude_groups: set(&["A", "B"]),
                ..Default::default()
            },
        )
        .unwrap();
        for g in ["A", "B", "OFF"] {
            assert_eq!(plain.groups[g].full, filtered.groups[g].full, "seed {seed}");
        }
    }
}

#[test]
fn recalc_is_byte_identical() {
    for seed in 1..=CASES {
        let b = gen_budget(&mut Rng(seed));
        let a = format!("{:?}", recalc(&b, &RecalcOptions::default()));
        let c = format!("{:?}", recalc(&b.clone(), &RecalcOptions::default()));
        assert_eq!(a, c, "seed {seed}");
    }
}

#[test]
fn deleting_any_global_keeps_every_total() {
    for seed in 1..=CASES {
        let b = gen_budget(&mut Rng(seed));
        let before = recalc(&b, &RecalcOptions::default()).unwrap();
        for name in ["DAYS", "WEEKS"] {
            let mut edited = b.clone();
            delete_global(&mut edited, name).unwrap();
            let after = recalc(&edited, &RecalcOptions::default())
                .unwrap_or_else(|e| panic!("seed {seed} {name}: {e}"));
            assert_eq!(after.categories, before.categories, "seed {seed} {name}");
            assert_eq!(after.net_total, before.net_total, "seed {seed} {name}");
        }
    }
}

/// A copy of `r` with separator rows dropped from every account.
fn without_separator_rows(mut r: Report) -> Report {
    for cat in &mut r.categories {
        for acct in &mut cat.accounts {
            acct.rows.retain(|row| row.kind != RowKind::Separator);
        }
    }
    r
}

#[test]
fn separators_and_styling_change_nothing() {
    let mut separators = 0;
    for seed in 1..=CASES {
        let b = gen_budget(&mut Rng(seed));
        let mut plain = b.clone();
        for g in plain.groups.values_mut() {
            g.color = None;
        }
        for acct in plain.categories_mut().flat_map(|c| &mut c.accounts) {
            separators += acct.details.len();
            acct.details.retain(|d| d.kind != RowKind::Separator);
            separators -= acct.details.len();
            for d in &mut acct.details {
                d.bold = false;
                d.italic = false;
                d.indent = 0;
                d.color = None;
                d.note = None;
            }
        }
        for opts in [
            RecalcOptions::default(),
            RecalcOptions {
                exclude_groups: set(&["A"]),
                ..Default::default()
            },
        ] {
            let with = recalc(&b, &opts).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
            let without = recalc(&plain, &opts).unwrap();
            assert_eq!(without_separator_rows(with), without, "seed {seed}");
        }
    }
    assert!(separators > 100, "the generator makes separators");
}
