//! Property tests over randomly generated budgets.
//!
//! Uses a small seeded generator instead of a property-testing crate, so the
//! engine stays dependency-free. Each failure prints its seed for replay.

use std::collections::BTreeSet;

use fwb_core::calc::Report;
use fwb_core::{
    recalc, Account, Budget, Category, ContractualCharge, Dec, Detail, Fringe, RecalcOptions, Unit,
};

const CASES: u64 = 500;

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
}

fn d(s: &str) -> Dec {
    s.parse().unwrap()
}

fn gen_budget(rng: &mut Rng) -> Budget {
    let mut b = Budget::new("random", "USD");
    b.exchange_rates.insert("CAD".into(), d("0.73"));
    b.exchange_rates.insert("GBP".into(), d("1.27"));
    b.globals
        .insert("DAYS".into(), (rng.below(60) as i64 + 1).into());
    b.globals.insert("WEEKS".into(), "DAYS / 5".into());
    b.fringes.insert(
        "PCT".into(),
        Fringe::Percent {
            percent: d("18.5"),
            wage_cap: Some(rng.below(5_000_000) as i64),
        },
    );
    b.fringes.insert(
        "FLAT".into(),
        Fringe::FlatPerUnit {
            per_unit: 2_500,
            unit_cap: None,
        },
    );

    let mut code = 1000;
    for c in 0..=rng.below(4) {
        let mut accounts = Vec::new();
        for _ in 0..=rng.below(5) {
            code += 10;
            let mut details = Vec::new();
            for i in 0..rng.below(7) {
                let amount = match rng.below(3) {
                    0 => "DAYS".to_string(),
                    1 => "WEEKS".to_string(),
                    _ => rng.below(50).to_string(),
                };
                let rate = format!("{}.{:02}", rng.below(5_000), rng.below(100));
                let mut det = Detail::new(
                    format!("line {i}"),
                    amount.as_str(),
                    Unit::Days,
                    rate.as_str(),
                );
                det.multiplier = (rng.below(3) as i64 + 1).into();
                det.currency = match rng.below(4) {
                    0 => Some("CAD".into()),
                    1 => Some("GBP".into()),
                    _ => None,
                };
                for f in ["PCT", "FLAT"] {
                    if rng.chance(40) {
                        det.fringes.insert(f.into());
                    }
                }
                for g in ["A", "B"] {
                    if rng.chance(25) {
                        det.groups.insert(g.into());
                    }
                }
                details.push(det);
            }
            accounts.push(Account {
                code: code.to_string(),
                name: format!("acct {code}"),
                details,
            });
        }
        b.categories.push(Category {
            name: format!("cat {c}"),
            accounts,
        });
    }
    b.contractual_charges.push(ContractualCharge {
        name: "Contingency".into(),
        percent: d("10"),
        excluded_accounts: BTreeSet::from(["1010".to_string()]),
        include_fringes: rng.chance(50),
    });
    b
}

fn check_rollups(r: &Report, seed: u64) {
    let mut all_fringes = 0;
    for cat in &r.categories {
        for acct in &cat.accounts {
            for det in &acct.details {
                assert_eq!(
                    det.fringe_total,
                    det.fringes.values().sum::<i64>(),
                    "seed {seed}"
                );
                if !det.included {
                    assert_eq!((det.total, det.fringe_total), (0, 0), "seed {seed}");
                }
                all_fringes += det.fringe_total;
            }
            assert_eq!(
                acct.details_total,
                acct.details.iter().map(|d| d.total).sum::<i64>(),
                "seed {seed}"
            );
            assert_eq!(
                acct.fringe_total,
                acct.details.iter().map(|d| d.fringe_total).sum::<i64>(),
                "seed {seed}"
            );
            assert_eq!(
                acct.total,
                acct.details_total + acct.fringe_total,
                "seed {seed}"
            );
        }
        assert_eq!(
            cat.details_total,
            cat.accounts.iter().map(|a| a.details_total).sum::<i64>(),
            "seed {seed}"
        );
        assert_eq!(
            cat.fringe_total,
            cat.accounts.iter().map(|a| a.fringe_total).sum::<i64>(),
            "seed {seed}"
        );
        assert_eq!(
            cat.total,
            cat.accounts.iter().map(|a| a.total).sum::<i64>(),
            "seed {seed}"
        );
    }
    assert_eq!(
        r.fringe_totals.values().sum::<i64>(),
        all_fringes,
        "seed {seed}"
    );
    assert_eq!(
        r.subtotal,
        r.categories.iter().map(|c| c.total).sum::<i64>(),
        "seed {seed}"
    );
    assert_eq!(
        r.grand_total,
        r.subtotal + r.contractual.iter().map(|c| c.amount).sum::<i64>(),
        "seed {seed}"
    );
}

#[test]
fn rollups_always_agree() {
    for seed in 1..=CASES {
        let mut rng = Rng(seed);
        let b = gen_budget(&mut rng);
        for opts in [
            RecalcOptions::default(),
            RecalcOptions {
                exclude_groups: BTreeSet::from(["A".to_string()]),
                ..Default::default()
            },
            RecalcOptions {
                only_groups: Some(BTreeSet::from(["B".to_string()])),
                ..Default::default()
            },
        ] {
            let r = recalc(&b, &opts).unwrap();
            check_rollups(&r, seed);
        }
    }
}

#[test]
fn excluding_a_group_equals_deleting_its_lines() {
    for seed in 1..=CASES {
        let mut rng = Rng(seed);
        let b = gen_budget(&mut rng);
        let opts = RecalcOptions {
            exclude_groups: BTreeSet::from(["A".to_string()]),
            ..Default::default()
        };
        let filtered = recalc(&b, &opts).unwrap();

        let mut pruned = b.clone();
        for acct in pruned.categories.iter_mut().flat_map(|c| &mut c.accounts) {
            acct.details.retain(|d| !d.groups.contains("A"));
        }
        let direct = recalc(&pruned, &RecalcOptions::default()).unwrap();

        assert_eq!(filtered.subtotal, direct.subtotal, "seed {seed}");
        assert_eq!(filtered.grand_total, direct.grand_total, "seed {seed}");
        assert_eq!(filtered.contractual, direct.contractual, "seed {seed}");
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
