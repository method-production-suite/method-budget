//! Seeded property tests for actuals and the cost report: on generated
//! budgets (every fringe posting mode) with random transactions and
//! estimate-to-complete overrides, every figure adds up exactly.

use std::collections::BTreeSet;

use fwb_core::actuals::{
    self, add_batch, cost_report_of, delete_transactions, edit_transactions, remove_batch, rollup,
    set_etc, CostFigures, CostReport, EtcMethod, Transaction, TransactionEdit,
};
use fwb_core::sample::{self, SampleSpec};
use fwb_core::{recalc, Budget, Dec, FringePosting, RecalcOptions};

const CASES: u64 = 60;

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
        self.next() % n.max(1)
    }
}

fn budget(seed: u64) -> Budget {
    let mut b = sample::budget(&SampleSpec {
        seed,
        rows: 150,
        accounts: 12,
        categories: 4,
        fringes: 6,
        groups: 4,
        baseline: seed.is_multiple_of(2),
    });
    b.fringe_posting = match seed % 4 {
        0 => FringePosting::Budget,
        1 => FringePosting::ProductionLevel,
        2 => FringePosting::Category {
            suffix: "99".into(),
        },
        _ => FringePosting::Account,
    };
    b
}

/// Account numbers to code transactions to: the chart's, other spellings of
/// them, numbers it doesn't have, and none.
fn codes(b: &Budget) -> Vec<String> {
    let mut out: Vec<String> = actuals::chart(b).values().map(|e| e.code.clone()).collect();
    let respelled: Vec<String> = out.iter().map(|c| c.replace('-', "")).collect();
    out.extend(respelled);
    out.extend(["98-01".to_string(), "ZZ".to_string(), String::new()]);
    out
}

fn random_transactions(rng: &mut Rng, b: &Budget, n: usize) -> Vec<Transaction> {
    let codes = codes(b);
    (0..n)
        .map(|i| Transaction {
            date: format!("2026-{:02}-{:02}", 1 + rng.below(12), 1 + rng.below(28)),
            account: codes[rng.below(codes.len() as u64) as usize].clone(),
            amount: rng.below(2_000_000) as i64 - 200_000,
            description: format!("Item {}", rng.below(20)),
            vendor: format!("Vendor {i}"),
            ..Transaction::default()
        })
        .collect()
}

fn random_overrides(rng: &mut Rng, b: &mut Budget) {
    let chart: Vec<String> = actuals::chart(b).values().map(|e| e.code.clone()).collect();
    for code in chart {
        let m = match rng.below(4) {
            0 => continue,
            1 => EtcMethod::Manual(rng.below(500_000) as i64),
            2 => EtcMethod::PercentComplete(Dec::from_int(1 + rng.below(100) as i64)),
            _ => EtcMethod::PercentComplete("37.5".parse().unwrap()),
        };
        set_etc(b, &code, Some(m)).unwrap();
    }
}

fn row_rules(f: &CostFigures, what: &str) {
    assert_eq!(f.eac, f.actual + f.etc, "{what}: EAC");
    assert_eq!(f.variance, f.budget - f.eac, "{what}: variance");
    assert_eq!(f.is_over(), f.variance < 0, "{what}: over");
    if f.budget == 0 {
        assert_eq!(f.percent_spent, None, "{what}: percent");
    }
}

fn sums_match(total: &CostFigures, parts: &[&CostFigures], what: &str) {
    let s = |g: fn(&CostFigures) -> i64| parts.iter().map(|p| g(p)).sum::<i64>();
    assert_eq!(total.budget, s(|f| f.budget), "{what}: budget");
    assert_eq!(total.actual, s(|f| f.actual), "{what}: actual");
    assert_eq!(total.etc, s(|f| f.etc), "{what}: etc");
    assert_eq!(total.eac, s(|f| f.eac), "{what}: eac");
    assert_eq!(total.variance, s(|f| f.variance), "{what}: variance");
    assert_eq!(
        total.count,
        parts.iter().map(|p| p.count).sum::<u64>(),
        "{what}: count"
    );
}

fn check(b: &Budget, c: &CostReport, seed: u64) {
    let r = recalc(b, &RecalcOptions::default()).unwrap();
    let all: i64 = b.actuals.transactions.iter().map(|t| t.amount).sum();
    assert_eq!(
        c.total.actual, all,
        "seed {seed}: every transaction counts once"
    );
    assert_eq!(
        c.total.count,
        b.actuals.transactions.len() as u64,
        "seed {seed}"
    );
    assert_eq!(
        c.total.budget, r.grand_total,
        "seed {seed}: the grand total"
    );
    // Unassigned: exactly the transactions whose number isn't in the chart.
    let chart = actuals::chart(b);
    let coding = b.account_coding;
    let loose: i64 = b
        .actuals
        .transactions
        .iter()
        .filter(|t| !chart.contains_key(&actuals::normalize_code(&coding, &t.account)))
        .map(|t| t.amount)
        .sum();
    assert_eq!(c.unassigned.actual, loose, "seed {seed}: unassigned");
    assert_eq!(
        (c.unassigned.budget, c.unassigned.etc),
        (0, 0),
        "seed {seed}"
    );
    for (ci, cat) in c.categories.iter().enumerate() {
        let what = format!("seed {seed} category {}", cat.code);
        assert_eq!(cat.figures.budget, r.categories[ci].total, "{what}");
        let mut parts: Vec<&CostFigures> = cat.accounts.iter().map(|a| &a.figures).collect();
        if let Some(fa) = &cat.fringe_account {
            parts.push(&fa.figures);
        }
        sums_match(&cat.figures, &parts, &what);
        row_rules(&cat.figures, &what);
        for (ai, a) in cat.accounts.iter().enumerate() {
            let what = format!("seed {seed} account {}", a.code);
            assert_eq!(
                a.figures.budget, r.categories[ci].accounts[ai].total,
                "{what}"
            );
            row_rules(&a.figures, &what);
            if a.method.is_none() {
                assert!(
                    if a.figures.budget >= 0 {
                        a.figures.etc >= 0
                    } else {
                        a.figures.etc <= 0
                    },
                    "{what}: budget minus actual never goes past zero"
                );
            }
        }
    }
    for (li, lv) in c.levels.iter().enumerate() {
        let what = format!("seed {seed} level {li}");
        assert_eq!(lv.figures.budget, r.levels[li].total, "{what}");
        let mut parts: Vec<&CostFigures> = c.categories[lv.start..lv.end]
            .iter()
            .map(|x| &x.figures)
            .collect();
        if let Some(p) = &lv.posted_fringes {
            parts.push(p);
        }
        sums_match(&lv.figures, &parts, &what);
    }
    assert_eq!(c.total_atl.budget, r.total_atl, "seed {seed}");
    assert_eq!(c.total_btl.budget, r.total_btl, "seed {seed}");
    let mut parts = vec![&c.atl_btl, &c.unassigned];
    if let Some(f) = &c.budget_fringes {
        parts.push(&f.figures);
    }
    parts.extend(c.charges.iter().map(|x| &x.figures));
    sums_match(&c.total, &parts, &format!("seed {seed} total"));
    row_rules(&c.total, &format!("seed {seed} total"));
}

#[test]
fn cost_reports_add_up_exactly() {
    for seed in 1..=CASES {
        let mut rng = Rng(seed);
        let mut b = budget(seed);
        let n = 1 + rng.below(80) as usize;
        let txs = random_transactions(&mut rng, &b, n);
        add_batch(&mut b, "ledger.csv", "2026-10-09T12:00:00Z", txs).unwrap();
        if !seed.is_multiple_of(3) {
            random_overrides(&mut rng, &mut b);
        }
        let r = recalc(&b, &RecalcOptions::default()).unwrap();
        let c = cost_report_of(&b, &r).unwrap();
        check(&b, &c, seed);
    }
}

#[test]
fn removing_an_import_puts_everything_back() {
    for seed in 1..=CASES / 2 {
        let mut rng = Rng(seed);
        let mut b = budget(seed);
        let first = random_transactions(&mut rng, &b, 10);
        add_batch(&mut b, "first.csv", "", first).unwrap();
        let before = b.clone();
        let r = recalc(&b, &RecalcOptions::default()).unwrap();
        let report_before = cost_report_of(&b, &r).unwrap();
        let second = random_transactions(&mut rng, &b, 25);
        let id = add_batch(&mut b, "second.csv", "", second).unwrap();
        assert_eq!(remove_batch(&mut b, id).unwrap(), 25, "seed {seed}");
        assert_eq!(b, before, "seed {seed}");
        assert_eq!(
            cost_report_of(&b, &r).unwrap(),
            report_before,
            "seed {seed}"
        );
    }
}

#[test]
fn edits_move_money_between_rows_exactly() {
    for seed in 1..=CASES / 2 {
        let mut rng = Rng(seed);
        let mut b = budget(seed);
        let txs = random_transactions(&mut rng, &b, 30);
        add_batch(&mut b, "ledger.csv", "", txs).unwrap();
        let r = recalc(&b, &RecalcOptions::default()).unwrap();
        let total = cost_report_of(&b, &r).unwrap().total.actual;
        // Reassigning never changes the total actual.
        let chart: Vec<String> = actuals::chart(&b)
            .values()
            .map(|e| e.code.clone())
            .collect();
        let ids: Vec<u64> = b
            .actuals
            .transactions
            .iter()
            .map(|t| t.id)
            .step_by(3)
            .collect();
        let to = chart[rng.below(chart.len() as u64) as usize].clone();
        let e = TransactionEdit {
            account: Some(to.clone()),
            ..TransactionEdit::default()
        };
        edit_transactions(&mut b, &ids, &e).unwrap();
        let c = cost_report_of(&b, &r).unwrap();
        assert_eq!(c.total.actual, total, "seed {seed}");
        check(&b, &c, seed);
        // Deleting takes exactly the deleted amounts off.
        let gone: BTreeSet<u64> = ids.iter().copied().take(2).collect();
        let off: i64 = b
            .actuals
            .transactions
            .iter()
            .filter(|t| gone.contains(&t.id))
            .map(|t| t.amount)
            .sum();
        delete_transactions(&mut b, &gone.iter().copied().collect::<Vec<_>>()).unwrap();
        let c = cost_report_of(&b, &r).unwrap();
        assert_eq!(c.total.actual, total - off, "seed {seed}");
        check(&b, &c, seed);
    }
}

#[test]
fn rollups_sum_to_the_total() {
    for seed in 1..=CASES {
        let mut rng = Rng(seed);
        let mut b = budget(seed);
        let txs = random_transactions(&mut rng, &b, 40);
        add_batch(&mut b, "ledger.csv", "", txs).unwrap();
        let roll = rollup(&b.actuals, &b.account_coding).unwrap();
        let by_account: i64 = roll.accounts.values().map(|s| s.amount).sum();
        assert_eq!(by_account, roll.total.amount, "seed {seed}");
        assert_eq!(roll.accounts.values().map(|s| s.count).sum::<u64>(), 40);
        // Spellings of one number land on one key.
        for key in roll.accounts.keys() {
            assert_eq!(&actuals::normalize_code(&b.account_coding, key), key);
        }
    }
}

#[test]
fn renumbering_carries_actuals() {
    let mut b = budget(3);
    let chart: Vec<String> = actuals::chart(&b)
        .values()
        .filter(|e| !e.fringe)
        .map(|e| e.code.clone())
        .collect();
    let txs = vec![
        Transaction {
            account: chart[0].clone(),
            amount: 500,
            ..Transaction::default()
        },
        Transaction {
            account: "98-01".into(),
            amount: 7,
            ..Transaction::default()
        },
    ];
    add_batch(&mut b, "ledger.csv", "", txs).unwrap();
    set_etc(&mut b, &chart[1], Some(EtcMethod::Manual(42))).unwrap();
    let r = recalc(&b, &RecalcOptions::default()).unwrap();
    let before = cost_report_of(&b, &r).unwrap();
    let to = fwb_core::coding::AccountCoding {
        category_width: 3,
        account_width: 3,
        ..b.account_coding
    };
    fwb_core::renumber::renumber(&mut b, to).unwrap();
    let r = recalc(&b, &RecalcOptions::default()).unwrap();
    let after = cost_report_of(&b, &r).unwrap();
    assert_eq!(
        b.actuals.transactions[1].account, "098-001",
        "carried by the same rule"
    );
    let first = &after.categories[0].accounts[0];
    assert_eq!(
        first.figures.actual,
        before.categories[0].accounts[0].figures.actual
    );
    assert_eq!(after.total.actual, before.total.actual);
    assert_eq!(after.total.etc, before.total.etc);
    assert_eq!(
        b.actuals.etc.values().copied().collect::<Vec<_>>(),
        [EtcMethod::Manual(42)]
    );
}

#[test]
fn versions_never_hold_actuals_and_restoring_keeps_them() {
    let mut b = budget(1);
    let txs = random_transactions(&mut Rng(9), &b, 5);
    add_batch(&mut b, "ledger.csv", "", txs).unwrap();
    let id = fwb_core::save_version(&mut b, "v1", "2026-10-09T12:00:00Z", None);
    let actuals = b.actuals.clone();
    fwb_core::restore_version(&mut b, id, "2026-10-09T12:01:00Z").unwrap();
    assert_eq!(b.actuals, actuals);
    assert!(b.version(id).unwrap().budget.to_budget().actuals.is_empty());
}
