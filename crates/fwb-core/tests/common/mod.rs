//! Shared test fixtures. Numbers are invented, not from any real budget.

#![allow(dead_code)]

use std::collections::BTreeSet;

use fwb_core::*;

pub fn d(s: &str) -> Dec {
    s.parse().unwrap()
}

pub fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}

pub fn category(code: &str, name: &str, accounts: Vec<Account>) -> TopsheetRow {
    TopsheetRow::Category(Category {
        notes: Vec::new(),
        code: code.into(),
        name: name.into(),
        accounts,
    })
}

pub fn level(name: &str) -> TopsheetRow {
    TopsheetRow::ProductionTotal(ProductionTotal { name: name.into() })
}

pub fn account(code: &str, name: &str, details: Vec<Detail>) -> Account {
    Account {
        notes: Vec::new(),
        code: code.into(),
        name: name.into(),
        details,
    }
}

pub fn group(in_budget_total: bool) -> Group {
    Group {
        short_code: None,
        description: String::new(),
        in_budget_total,
        color: None,
    }
}

pub fn percent_charge(name: &str, p: &str) -> Charge {
    Charge {
        name: name.into(),
        account: None,
        applied: true,
        kind: ChargeKind::Percent(d(p)),
        excluded_amounts: Vec::new(),
        excluded_groups: BTreeSet::new(),
        excluded_categories: BTreeSet::new(),
        excluded_accounts: BTreeSet::new(),
        label_percent: false,
        label_excluded: false,
    }
}

/// A small feature budget exercising most of the engine. Expected figures are
/// worked out by hand in tests/recalc.rs.
pub fn sample() -> Budget {
    let mut b = Budget::new("Sample Feature", "USD");
    b.currencies.insert(
        "CAD".into(),
        Currency {
            description: "Canadian dollar".into(),
            symbol: "$".into(),
            rate: d("0.73"),
        },
    );
    b.globals.insert("SHOOT_DAYS".into(), "30".into());
    b.globals
        .insert("PREP_DAYS".into(), "SHOOT_DAYS / 3".into());
    b.fringes.insert(
        "FICA".into(),
        Fringe {
            short_code: None,
            description: String::new(),
            rate: "7.65".into(),
            kind: FringeKind::Percent {
                cutoff: Some(1_000_000),
            },
        },
    );
    b.fringes.insert(
        "PH".into(),
        Fringe {
            short_code: None,
            description: String::new(),
            rate: "50".into(),
            kind: FringeKind::Flat {
                unit: Some("Day".into()),
            },
        },
    );
    b.groups.insert("OptionA".into(), group(true));
    b.groups.insert("Suppress".into(), group(false));
    b.groups.insert("Incentive".into(), group(true));

    let mut writer = Detail::line(1, "Writer", 1, "Flat", 150_000);
    writer.fringes = set(&["FICA"]);

    let mut grip = Detail::line(2, "Key grip", "SHOOT_DAYS + PREP_DAYS", "Day", 450);
    grip.x = 3.into();
    grip.fringes = set(&["FICA", "PH"]);
    grip.fringe_range = Some(1);

    let mut grip_ot = Detail::line(3, "Key grip overtime", 1, "Flat", 2_000);
    grip_ot.fringes = set(&["FICA"]);
    grip_ot.fringe_range = Some(1);

    let mut camera = Detail::line(5, "Camera package", 6, "Week", "1200.50");
    camera.currency = Some("CAD".into());
    camera.groups = set(&["OptionA"]);

    let mut suppressed = Detail::line(7, "Second unit grip", 10, "Day", 100);
    suppressed.groups = set(&["Suppress"]);

    let mut editor = Detail::line(8, "Editor", 10, "Week", 3_000);
    editor.groups = set(&["Incentive"]);
    editor.set = Some("Cutting room".into());
    editor.location = Some("LA".into());

    b.topsheet = vec![
        category(
            "1000",
            "Above-the-line",
            vec![account("1100", "Writing", vec![writer])],
        ),
        level("Total Above-the-Line"),
        category(
            "2000",
            "Production",
            vec![account(
                "2100",
                "Crew",
                vec![
                    grip,
                    grip_ot,
                    Detail::subtotal(4, "Grip subtotal"),
                    camera,
                    Detail::adjustment(6, "Crew contingency", "0.1"),
                    suppressed,
                ],
            )],
        ),
        level("Total Production"),
        category(
            "5000",
            "Post",
            vec![account("5100", "Editorial", vec![editor])],
        ),
        level("Total Post"),
    ];

    let mut insurance = percent_charge("Insurance", "2.5");
    insurance.excluded_accounts = set(&["1100"]);
    insurance.excluded_amounts = vec![("Deposit".into(), 100_000)];
    let mut unapplied = percent_charge("Unused fee", "50");
    unapplied.applied = false;
    b.charges = vec![
        percent_charge("Contingency", "10"),
        insurance,
        Charge {
            kind: ChargeKind::Flat {
                amount: 500_000,
                currency: Some("CAD".into()),
            },
            ..percent_charge("Bond fee", "0")
        },
        unapplied,
    ];

    b.credits = vec![Credit {
        name: "State incentive".into(),
        account: None,
        applied: true,
        cap: None,
        discount_percent: d("10"),
        rows: vec![
            CreditRow {
                description: "Qualified labor".into(),
                source: CreditSource::Group {
                    group: "Incentive".into(),
                    excluded_fringes: set(&[]),
                },
                percent: d("30"),
                cap: CreditCap::None,
            },
            CreditRow {
                description: "Other spend".into(),
                source: CreditSource::Flat(1_000_000),
                percent: d("25"),
                cap: CreditCap::Credit(200_000),
            },
        ],
    }];
    b
}
