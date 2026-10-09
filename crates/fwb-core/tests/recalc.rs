//! End-to-end recalc scenarios with hand-checked numbers.

mod common;

use std::collections::BTreeSet;

use common::*;
use fwb_core::*;

fn run(b: &Budget) -> Report {
    recalc(b, &RecalcOptions::default()).unwrap()
}

#[test]
fn sample_lines_and_rows() {
    let r = run(&sample());
    assert_eq!(r.globals["PREP_DAYS"], d("10"));

    let crew = &r.categories[1].accounts[0];
    let [grip, ot, sub, camera, adj, supp] = &crew.rows[..] else {
        panic!("6 rows")
    };

    assert_eq!(grip.quantity, d("120"));
    assert_eq!(grip.total, 5_400_000);
    assert_eq!(ot.total, 200_000);
    assert_eq!(sub.subtotal, Some(5_600_000));
    assert_eq!(sub.total, 0, "subtotal rows add nothing");

    assert_eq!(camera.currency, "CAD");
    assert_eq!(camera.local_total, 720_300);
    assert_eq!(camera.total, 525_819, "720,300 CAD cents x 0.73");

    // 10% of everything above it: 5,400,000 + 200,000 + 525,819 = 6,125,819.
    assert_eq!(adj.total, 612_582);

    assert!(!supp.included, "Suppress group has in-budget-total off");
    assert_eq!(supp.total, 0);

    assert_eq!(crew.details_total, 6_738_401);
}

#[test]
fn sample_fringes() {
    let r = run(&sample());
    let atl = &r.categories[0];
    assert_eq!(atl.fringe_total, 76_500, "FICA stops at the $10,000 cutoff");

    let rows = &r.categories[1].accounts[0].rows;
    // Grip and overtime are one earner: 5,600,000 caps to 1,000,000, so one
    // 76,500 FICA split 54:2 between them.
    assert_eq!(rows[0].fringes["FICA"], 73_768);
    assert_eq!(rows[1].fringes["FICA"], 2_732);
    // $50 per day x 120 person-days.
    assert_eq!(rows[0].fringes["PH"], 600_000);
    assert_eq!(r.categories[1].fringe_total, 676_500);

    assert_eq!(
        r.fringes["FICA"],
        FringeSummary {
            uses: 3,
            total: 153_000
        }
    );
    assert_eq!(
        r.fringes["PH"],
        FringeSummary {
            uses: 1,
            total: 600_000
        }
    );
}

#[test]
fn sample_topsheet() {
    let r = run(&sample());

    // Fringes post to a fringe account in each category.
    assert_eq!(
        r.categories[0].fringe_account,
        Some(("10-99".into(), 76_500))
    );
    assert_eq!(r.categories[0].total, 15_076_500);
    assert_eq!(r.categories[1].total, 7_414_901);
    assert_eq!(r.categories[2].total, 3_000_000);

    assert_eq!(r.levels.len(), 3);
    assert_eq!(r.total_atl, 15_076_500);
    assert_eq!(r.total_btl, 10_414_901);
    assert_eq!(r.atl_btl, 25_491_401);

    let c: Vec<_> = r
        .charges
        .iter()
        .map(|c| (c.name.as_str(), c.amount))
        .collect();
    assert_eq!(
        c,
        [
            ("Contingency", 2_549_140),
            // (25,491,401 - 15,076,500 writer and fringes - 100,000 deposit) x 2.5%
            ("Insurance", 257_873),
            // 500,000 CAD cents x 0.73
            ("Bond fee", 365_000),
        ]
    );
    assert_eq!(r.grand_total, 28_663_414);

    // (3,000,000 x 30% + min(250,000, 200,000)) less 10% discount.
    assert_eq!(r.credits[0].amount, 990_000);
    assert_eq!(r.credits_total, 990_000);
    assert_eq!(r.net_total, 27_673_414);
}

#[test]
fn sample_breakdowns() {
    let b = sample();
    let r = run(&b);
    let g = |name: &str| &r.groups[name];
    assert_eq!(g("OptionA").active.details_total, 525_819);
    assert_eq!(g("Suppress").full.details_total, 100_000);
    assert_eq!(g("Suppress").active.details_total, 0);
    assert_eq!(g("Incentive").active.details_total, 3_000_000);
    assert_eq!(r.sets["Cutting room"].details_total, 3_000_000);
    assert_eq!(r.locations["LA"].details_total, 3_000_000);

    let sel = selection_totals(&b, &r, &BTreeSet::from([2, 3])).unwrap();
    assert_eq!(
        sel.lines,
        Breakdown {
            details_total: 5_600_000,
            fringe_total: 676_500
        }
    );
    assert_eq!(sel.fringes["FICA"], 76_500);
}

#[test]
fn budget_posting_keeps_fringes_out_of_levels() {
    let mut b = sample();
    b.fringe_posting = FringePosting::Budget;
    let r = run(&b);
    assert_eq!(r.categories[0].fringe_account, None);
    assert_eq!(r.total_atl, 15_000_000);
    assert_eq!(r.total_btl, 9_738_401);
    assert_eq!(r.budget_fringes, 753_000);
    let charges: i64 = r.charges.iter().map(|c| c.amount).sum();
    assert_eq!(r.grand_total, r.atl_btl + r.budget_fringes + charges);
}

#[test]
fn excluding_a_group_drops_its_lines() {
    let opts = RecalcOptions {
        exclude_groups: set(&["OptionA"]),
        ..Default::default()
    };
    let r = recalc(&sample(), &opts).unwrap();
    let rows = &r.categories[1].accounts[0].rows;
    assert!(!rows[3].included);
    // The adjustment now only sees grip and overtime.
    assert_eq!(rows[4].total, 560_000);
    assert_eq!(r.groups["OptionA"].active.details_total, 0);
    assert_eq!(r.groups["OptionA"].full.details_total, 525_819);
}

#[test]
fn changing_a_global_recalculates_dependents() {
    let mut b = sample();
    b.globals.insert("SHOOT_DAYS".into(), "35".into());
    let r = run(&b);
    let grip = &r.categories[1].accounts[0].rows[0];
    // 35 / 3 rounds to 11.666666667 at 9 places, so 3 x 46.666666667 = 140.000000001.
    assert_eq!(grip.quantity, d("140.000000001"));
    assert_eq!(grip.total, 6_300_000);
    assert_eq!(grip.fringes["PH"], 700_000);
}

#[test]
fn recalc_is_deterministic() {
    let b = sample();
    assert_eq!(format!("{:?}", run(&b)), format!("{:?}", run(&b.clone())));
}

#[test]
fn global_cycles_are_reported() {
    let mut b = sample();
    b.globals.insert("A".into(), "B + 1".into());
    b.globals.insert("B".into(), "A".into());
    let e = recalc(&b, &RecalcOptions::default()).unwrap_err();
    assert_eq!(
        e.kind,
        ErrorKind::GlobalCycle(vec!["A".into(), "B".into(), "A".into()])
    );
    assert_eq!(
        e.to_string(),
        "global A: globals refer to each other: A -> B -> A"
    );
}

fn editor(b: &mut Budget) -> &mut Detail {
    b.categories_mut().nth(2).unwrap().accounts[0]
        .details
        .get_mut(0)
        .unwrap()
}

#[test]
fn errors_name_the_line_and_field() {
    let check = |f: fn(&mut Budget), want: ErrorKind| {
        let mut b = sample();
        f(&mut b);
        assert_eq!(
            recalc(&b, &RecalcOptions::default()).unwrap_err().kind,
            want
        );
    };

    let mut b = sample();
    editor(&mut b).rate = "WEEKLY_RATE".into();
    let e = recalc(&b, &RecalcOptions::default()).unwrap_err();
    assert_eq!(
        e.to_string(),
        "account 5100 line 1 (Editor): rate: unknown global `WEEKLY_RATE`"
    );

    check(
        |b| editor(b).currency = Some("EUR".into()),
        ErrorKind::UnknownCurrency("EUR".into()),
    );
    check(
        |b| editor(b).fringes = set(&["PENSION"]),
        ErrorKind::UnknownFringe("PENSION".into()),
    );
    check(
        |b| editor(b).groups = set(&["Nope"]),
        ErrorKind::UnknownGroup("Nope".into()),
    );
    check(|b| editor(b).id = 1, ErrorKind::DuplicateRowId(1));
    check(
        |b| drop(b.globals.insert("bad-name".into(), "1".into())),
        ErrorKind::BadGlobalName,
    );
}

#[test]
fn blank_fields_make_title_lines() {
    let mut b = sample();
    let title = Detail::line(99, "CAST TRAVEL", "", "", "");
    b.categories_mut().nth(2).unwrap().accounts[0]
        .details
        .insert(0, title);
    let r = run(&b);
    assert_eq!(r.categories[2].accounts[0].rows[0].total, 0);
    assert_eq!(r.categories[2].total, 3_000_000);
}
