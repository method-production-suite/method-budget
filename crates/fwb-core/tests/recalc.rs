//! End-to-end recalc scenarios with hand-checked numbers.

use std::collections::BTreeSet;

use fwb_core::{
    recalc, Account, Budget, Category, ContractualCharge, Dec, Detail, ErrorKind, Fringe,
    RecalcOptions, Unit,
};

fn d(s: &str) -> Dec {
    s.parse().unwrap()
}

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}

fn account(code: &str, name: &str, details: Vec<Detail>) -> Account {
    Account {
        code: code.into(),
        name: name.into(),
        details,
    }
}

fn sample() -> Budget {
    let mut b = Budget::new("Sample Feature", "USD");
    b.exchange_rates.insert("CAD".into(), d("0.73"));
    b.globals.insert("SHOOT_DAYS".into(), "30".into());
    b.globals
        .insert("PREP_DAYS".into(), "SHOOT_DAYS / 3".into());
    b.fringes.insert(
        "FICA".into(),
        Fringe::Percent {
            percent: d("7.65"),
            wage_cap: Some(1_000_000),
        },
    );
    b.fringes.insert(
        "PH".into(),
        Fringe::FlatPerUnit {
            per_unit: 5_000,
            unit_cap: None,
        },
    );

    let mut writer = Detail::new("Writer", 1, Unit::Flat, 150_000);
    writer.fringes = set(&["FICA"]);

    let mut grip = Detail::new("Key grip", "SHOOT_DAYS + PREP_DAYS", Unit::Days, 450);
    grip.multiplier = 3.into();
    grip.fringes = set(&["FICA", "PH"]);

    let mut camera = Detail::new("Camera package", 6, Unit::Weeks, "1200.50");
    camera.currency = Some("CAD".into());
    camera.groups = set(&["OptionA"]);

    b.categories = vec![
        Category {
            name: "Above-the-line".into(),
            accounts: vec![account("1100", "Writing", vec![writer])],
        },
        Category {
            name: "Production".into(),
            accounts: vec![account("2000", "Crew and equipment", vec![grip, camera])],
        },
        Category {
            name: "Post".into(),
            accounts: vec![account(
                "5000",
                "Editorial",
                vec![Detail::new("Editor", 10, Unit::Weeks, 3_000)],
            )],
        },
    ];
    b.contractual_charges = vec![
        ContractualCharge {
            name: "Contingency".into(),
            percent: d("10"),
            excluded_accounts: BTreeSet::new(),
            include_fringes: true,
        },
        ContractualCharge {
            name: "Insurance".into(),
            percent: d("2.5"),
            excluded_accounts: set(&["1100"]),
            include_fringes: false,
        },
    ];
    b
}

#[test]
fn sample_budget_totals() {
    let r = recalc(&sample(), &RecalcOptions::default()).unwrap();

    assert_eq!(r.globals["PREP_DAYS"], d("10"));

    let atl = &r.categories[0];
    assert_eq!(atl.details_total, 15_000_000);
    assert_eq!(
        atl.fringe_total, 76_500,
        "FICA stops at the $10,000 wage cap"
    );

    let crew = &r.categories[1].accounts[0];
    let grip = &crew.details[0];
    assert_eq!(grip.quantity, d("120"));
    assert_eq!(grip.total, 5_400_000);
    assert_eq!(grip.fringes["FICA"], 76_500);
    assert_eq!(grip.fringes["PH"], 600_000, "$50 x 120 person-days");

    let camera = &crew.details[1];
    assert_eq!(camera.currency, "CAD");
    assert_eq!(camera.local_total, 720_300);
    assert_eq!(camera.total, 525_819, "720,300 CAD cents x 0.73");

    assert_eq!(r.categories[1].total, 6_602_319);
    assert_eq!(r.categories[2].total, 3_000_000);
    assert_eq!(r.subtotal, 24_678_819);

    assert_eq!(r.fringe_totals["FICA"], 153_000);
    assert_eq!(r.fringe_totals["PH"], 600_000);

    assert_eq!(r.contractual[0].base, 24_678_819);
    assert_eq!(
        r.contractual[0].amount, 2_467_882,
        "10% of 24,678,819 rounds up"
    );
    assert_eq!(r.contractual[1].base, 8_925_819, "no 1100, no fringes");
    assert_eq!(r.contractual[1].amount, 223_145);
    assert_eq!(r.grand_total, 27_369_846);
}

#[test]
fn excluding_a_group_drops_its_lines() {
    let opts = RecalcOptions {
        exclude_groups: set(&["OptionA"]),
        ..Default::default()
    };
    let r = recalc(&sample(), &opts).unwrap();
    let camera = &r.categories[1].accounts[0].details[1];
    assert!(!camera.included);
    assert_eq!(camera.total, 0);
    assert_eq!(r.subtotal, 24_153_000);
}

#[test]
fn only_groups_keeps_tagged_lines() {
    let opts = RecalcOptions {
        only_groups: Some(set(&["OptionA"])),
        ..Default::default()
    };
    let r = recalc(&sample(), &opts).unwrap();
    assert_eq!(r.subtotal, 525_819);
}

#[test]
fn changing_a_global_recalculates_dependents() {
    let mut b = sample();
    b.globals.insert("SHOOT_DAYS".into(), "35".into());
    let r = recalc(&b, &RecalcOptions::default()).unwrap();
    let grip = &r.categories[1].accounts[0].details[0];
    // 35 / 3 rounds to 11.666666667 at 9 places, so 3 x 46.666666667 = 140.000000001.
    assert_eq!(grip.quantity, d("140.000000001"));
    assert_eq!(grip.total, 6_300_000);
    assert_eq!(grip.fringes["PH"], 700_000);
}

#[test]
fn recalc_is_deterministic() {
    let b = sample();
    let a = recalc(&b, &RecalcOptions::default()).unwrap();
    let c = recalc(&b, &RecalcOptions::default()).unwrap();
    assert_eq!(format!("{a:?}"), format!("{c:?}"));
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

#[test]
fn errors_name_the_line_and_field() {
    let mut b = sample();
    b.categories[2].accounts[0].details[0].rate = "WEEKLY_RATE".into();
    let e = recalc(&b, &RecalcOptions::default()).unwrap_err();
    assert_eq!(
        e.to_string(),
        "account 5000 line 1 (Editor): rate: unknown global `WEEKLY_RATE`"
    );

    let mut b = sample();
    b.categories[2].accounts[0].details[0].currency = Some("EUR".into());
    let e = recalc(&b, &RecalcOptions::default()).unwrap_err();
    assert_eq!(e.kind, ErrorKind::UnknownCurrency("EUR".into()));

    let mut b = sample();
    b.categories[2].accounts[0].details[0].fringes = set(&["PENSION"]);
    let e = recalc(&b, &RecalcOptions::default()).unwrap_err();
    assert_eq!(e.kind, ErrorKind::UnknownFringe("PENSION".into()));

    let mut b = sample();
    b.globals.insert("bad-name".into(), "1".into());
    let e = recalc(&b, &RecalcOptions::default()).unwrap_err();
    assert_eq!(e.kind, ErrorKind::BadGlobalName);
}

#[test]
fn flat_fringe_unit_cap() {
    let mut b = sample();
    b.fringes.insert(
        "PH".into(),
        Fringe::FlatPerUnit {
            per_unit: 5_000,
            unit_cap: Some(d("100")),
        },
    );
    let r = recalc(&b, &RecalcOptions::default()).unwrap();
    assert_eq!(r.fringe_totals["PH"], 500_000);
}
