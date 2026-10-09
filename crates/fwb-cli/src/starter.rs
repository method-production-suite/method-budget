//! The starter budget `fwb new` writes: our own small chart of accounts with
//! placeholder rates to replace. Not derived from any vendor template.

use std::collections::BTreeSet;

use fwb_core::{
    Account, Budget, Category, Charge, ChargeKind, Detail, Fringe, FringeKind, ProductionTotal,
    TopsheetRow,
};

fn category(code: &str, name: &str, accounts: Vec<Account>) -> TopsheetRow {
    TopsheetRow::Category(Category {
        code: code.into(),
        name: name.into(),
        accounts,
    })
}

fn level(name: &str) -> TopsheetRow {
    TopsheetRow::ProductionTotal(ProductionTotal { name: name.into() })
}

fn account(code: &str, name: &str, details: Vec<Detail>) -> Account {
    Account {
        code: code.into(),
        name: name.into(),
        details,
    }
}

pub fn budget() -> Budget {
    let mut b = Budget::new("Untitled feature", "USD");
    for (name, value) in [
        ("PREP_WEEKS", "4"),
        ("SHOOT_DAYS", "25"),
        ("WRAP_WEEKS", "1"),
    ] {
        b.globals.insert(name.into(), value.into());
    }
    b.globals
        .insert("SHOOT_WEEKS".into(), "SHOOT_DAYS / 5".into());
    b.fringes.insert(
        "PAYROLL".into(),
        Fringe {
            description: "Payroll taxes (placeholder rate)".into(),
            rate: "10".into(),
            kind: FringeKind::Percent { cutoff: None },
        },
    );

    let mut id = 0;
    let mut line = |desc: &str, amount: &str, unit: &str, rate: &str, payroll: bool| {
        id += 1;
        let mut d = Detail::line(id, desc, amount, unit, rate);
        if payroll {
            d.fringes = BTreeSet::from(["PAYROLL".to_string()]);
        }
        d
    };

    let writing = account(
        "10-01",
        "Writing",
        vec![line("Screenplay", "1", "Flat", "50000", true)],
    );
    let producers = account(
        "11-01",
        "Producers",
        vec![line("Producer", "1", "Flat", "75000", true)],
    );
    let director = account(
        "12-01",
        "Director",
        vec![line("Director", "1", "Flat", "100000", true)],
    );
    let cast = account(
        "13-01",
        "Cast",
        vec![
            line("Lead", "SHOOT_WEEKS + 1", "Week", "5000", true),
            line("Supporting", "SHOOT_DAYS / 2", "Day", "1000", true),
        ],
    );
    let staff = account(
        "20-01",
        "Production staff",
        vec![
            line(
                "Line producer",
                "PREP_WEEKS + SHOOT_WEEKS + WRAP_WEEKS",
                "Week",
                "3000",
                true,
            ),
            line(
                "1st AD",
                "PREP_WEEKS / 2 + SHOOT_WEEKS",
                "Week",
                "3500",
                true,
            ),
        ],
    );
    let camera = account(
        "21-01",
        "Camera",
        vec![
            line(
                "Director of photography",
                "SHOOT_WEEKS + 1",
                "Week",
                "4500",
                true,
            ),
            line("Camera package", "SHOOT_WEEKS", "Week", "6000", false),
        ],
    );
    let locations = account(
        "22-01",
        "Locations",
        vec![line("Location fees", "SHOOT_DAYS", "Day", "1500", false)],
    );
    let editorial = account(
        "40-01",
        "Editorial",
        vec![line("Editor", "12", "Week", "3000", true)],
    );
    let sound = account(
        "41-01",
        "Post sound",
        vec![line("Mix", "1", "Allow", "20000", false)],
    );
    let insurance = account(
        "50-01",
        "Insurance",
        vec![line("Production package", "1", "Allow", "15000", false)],
    );

    b.topsheet = vec![
        category("10", "Story and rights", vec![writing]),
        category("11", "Producers", vec![producers]),
        category("12", "Director", vec![director]),
        category("13", "Cast", vec![cast]),
        level("Total above-the-line"),
        category("20", "Production staff", vec![staff]),
        category("21", "Camera", vec![camera]),
        category("22", "Locations", vec![locations]),
        level("Total production"),
        category("40", "Editorial", vec![editorial]),
        category("41", "Post sound", vec![sound]),
        level("Total post"),
        category("50", "Insurance", vec![insurance]),
        level("Total other"),
    ];
    b.charges.push(Charge {
        name: "Contingency".into(),
        account: None,
        applied: true,
        kind: ChargeKind::Percent("10".parse().expect("valid decimal")),
        excluded_amounts: Vec::new(),
        excluded_groups: BTreeSet::new(),
        excluded_categories: BTreeSet::new(),
        excluded_accounts: BTreeSet::new(),
    });
    b
}
