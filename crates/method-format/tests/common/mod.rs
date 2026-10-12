//! Shared test fixtures, copied from crates/method-core/tests/common (test crates
//! cannot import each other). Numbers are invented, not from any real budget.

#![allow(dead_code)]

use std::collections::BTreeSet;

use method_core::*;

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
            kind: FringeKind::flat(Some("Day".into())),
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
        excluded_fringes: BTreeSet::new(),
        rows: vec![
            CreditRow {
                description: "Qualified labor".into(),
                source: CreditSource::Group {
                    group: "Incentive".into(),
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

/// `sample()` plus every field the format stores that `sample()` leaves at its
/// default: info, totals currency, coding, posting, units, sets, locations,
/// baseline, every enum variant, and awkward strings.
pub fn rich() -> Budget {
    use method_core::coding::AccountCoding;

    let mut b = sample();
    b.totals_currency = Some("CAD".into());
    b.account_coding = AccountCoding {
        category_width: 2,
        account_width: 2,
        sub_width: 3,
        set_width: 0,
        separator: Some('.'),
    };
    b.fringe_posting = FringePosting::ProductionLevel;
    b.info
        .insert("Director".into(), InfoValue::Text("A. Person".into()));
    b.info
        .insert("Shoot days".into(), InfoValue::Number(d("30")));
    b.info
        .insert("Ratio".into(), InfoValue::Number(d("-0.000000001")));
    b.info
        .insert("Start".into(), InfoValue::Date("2027-03-01".into()));
    b.units.insert(
        "Episode".into(),
        UnitDef {
            plural: "Episodes".into(),
            description: "One episode of a series".into(),
            hours: None,
        },
    );
    b.units.get_mut("Day").unwrap().hours = Some(d("12.5"));
    b.globals.insert("BLANK".into(), "".into());
    b.globals.insert("SPACES".into(), "  ".into());
    b.fringes.get_mut("FICA").unwrap().description = "Social security".into();
    b.fringes.insert(
        "Pension".into(),
        Fringe {
            short_code: None,
            description: String::new(),
            rate: "PENSION_RATE".into(),
            kind: FringeKind::Percent { cutoff: None },
        },
    );
    b.fringes.insert(
        "Meal".into(),
        Fringe {
            short_code: None,
            description: "Per line".into(),
            rate: "".into(),
            kind: FringeKind::flat(None),
        },
    );
    b.groups.get_mut("OptionA").unwrap().description = "Option A \"deluxe\"".into();
    b.sets.insert("Cutting room".into(), SetDef::default());
    b.sets.insert(
        "Diner".into(),
        SetDef {
            description: "Int. diner, night".into(),
        },
    );
    b.locations.insert(
        "LA".into(),
        Location {
            description: "Los Angeles".into(),
            city: "Los Angeles".into(),
            region: "CA".into(),
            country: "US".into(),
        },
    );
    b.locations.insert(
        "Studio \\ B".into(),
        Location {
            description: String::new(),
            city: "Montr\u{e9}al".into(),
            region: String::new(),
            country: String::new(),
        },
    );

    // A category with no accounts, an account with no details, and a row that
    // fills every detail field.
    let mut full = Detail::line(100, "Everything \u{2603}\n\ttabbed", "2", "Week", "-1.5");
    full.x = "X_FACTOR".into();
    full.unit2 = "Day".into();
    full.unit3 = "Hour".into();
    full.x4 = "  ".into();
    full.unit4 = "Flat".into();
    full.currency = Some(String::new());
    full.fringes = set(&["FICA", "Meal"]);
    full.groups = set(&["OptionA", "Incentive"]);
    full.location = Some("Studio \\ B".into());
    full.set = Some("Diner".into());
    full.fringe_range = Some(u32::MAX);
    let mut adj = Detail::adjustment(101, "", "0.05");
    adj.currency = Some("CAD".into());
    b.topsheet.push(category("6000", "Empty category", vec![]));
    b.topsheet.push(category(
        "7000",
        "Other",
        vec![
            account("7100", "No rows", vec![]),
            account("7200", "", vec![full, Detail::subtotal(102, ""), adj]),
        ],
    ));

    let mut flat = percent_charge("Completion bond", "0");
    flat.kind = ChargeKind::Flat {
        amount: -12_345,
        currency: None,
    };
    flat.account = Some("8100".into());
    flat.applied = false;
    flat.excluded_groups = set(&["Suppress"]);
    flat.excluded_categories = set(&["1000", "6000"]);
    b.charges.push(flat);
    b.charges[0].account = Some("8000".into());
    b.charges[0].excluded_amounts = vec![
        ("Twice".into(), 1),
        ("Twice".into(), 1),
        ("Negative".into(), i64::MIN),
    ];

    b.credits.push(Credit {
        name: "Capped".into(),
        account: Some("9000".into()),
        applied: false,
        cap: Some(i64::MAX),
        discount_percent: Dec::ZERO,
        excluded_fringes: set(&["FICA", "PH"]),
        rows: vec![
            CreditRow {
                description: "Spend cap".into(),
                source: CreditSource::Group {
                    group: "OptionA".into(),
                },
                percent: d("12.345678901"),
                cap: CreditCap::QualifiedSpend(5_000_000),
            },
            CreditRow {
                description: String::new(),
                source: CreditSource::Flat(0),
                percent: Dec::ZERO,
                cap: CreditCap::None,
            },
        ],
    });
    b.credits.push(Credit {
        name: "No rows".into(),
        account: None,
        applied: true,
        cap: None,
        discount_percent: d("2.5"),
        excluded_fringes: BTreeSet::new(),
        rows: vec![],
    });

    b.baseline = Some(Baseline {
        rows: [(1, 15_000_000), (2, 1), (10, -5), (u64::MAX, 7)].into(),
        accounts: [("1100".to_string(), 15_000_000), ("2100".to_string(), 42)].into(),
        categories: [("1000".to_string(), 15_000_000)].into(),
        grand_total: 987_654_321,
        net_total: -1,
    });
    b
}
