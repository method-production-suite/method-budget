//! The starter budget for new files (`method new` and the app's New): our own small
//! chart of accounts with placeholder rates to replace. Not derived from any
//! vendor template.
//!
//! Every new budget, the starter or an empty one ([`blank`]), starts with
//! the reference product's new-budget settings (decisions.md, entry 60):
//! fringes posted by budget, account numbers of category 2, account 2 and
//! set 3, the starter units (a Day of 12 hours, a Week of 60, a Month of
//! 240, an Hour; Allow and Flat with no hours), a new line copying its
//! fringes from the line above, and a cutoff factor of 1.0. A budget file
//! keeps its own settings: these are only where a new one starts.

use std::collections::BTreeSet;

use crate::coding::AccountCoding;
use crate::model::{
    default_units, Account, Budget, Category, Charge, ChargeKind, CutoffFactor, Detail, Fringe,
    FringeKind, FringePosting, NewLineDefaults, ProductionTotal, TopsheetRow,
};

/// Gives `b` the settings a new budget starts with (the module docs).
pub fn new_budget_settings(b: &mut Budget) {
    b.fringe_posting = FringePosting::Budget;
    b.account_coding = AccountCoding::new_budget();
    b.units = default_units();
    b.new_line_defaults = NewLineDefaults::default();
    b.cutoff_factor = CutoffFactor::One;
}

/// A new, empty budget titled `title` in `base_currency`, with the
/// new-budget settings (the module docs) and nothing else.
pub fn blank(title: impl Into<String>, base_currency: impl Into<String>) -> Budget {
    let mut b = Budget::new(title, base_currency);
    new_budget_settings(&mut b);
    b
}

fn category(code: &str, name: &str, accounts: Vec<Account>) -> TopsheetRow {
    TopsheetRow::Category(Category {
        code: code.into(),
        name: name.into(),
        accounts,
        notes: Vec::new(),
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
        notes: Vec::new(),
    }
}

/// A fresh starter budget. Row ids start at 1 and are unique.
pub fn budget() -> Budget {
    let mut b = blank("Untitled feature", "USD");
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
            short_code: None,
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
        label_percent: false,
        label_excluded: false,
    });
    b
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use crate::coding::AccountCoding;
    use crate::{recalc, CutoffFactor, Dec, FringePosting, RecalcOptions};

    /// A new budget starts as the reference product starts one
    /// (decisions.md, entry 60).
    #[test]
    fn new_budgets_start_with_the_reference_settings() {
        for b in [super::budget(), super::blank("New", "USD")] {
            assert_eq!(b.fringe_posting, FringePosting::Budget);
            assert_eq!(b.account_coding, AccountCoding::new_budget());
            assert_eq!(
                (
                    b.account_coding.category_width,
                    b.account_coding.account_width,
                    b.account_coding.sub_width,
                    b.account_coding.set_width
                ),
                (2, 2, 0, 3)
            );
            assert_eq!(b.cutoff_factor, CutoffFactor::One);
            let n = b.new_line_defaults;
            assert!(n.fringes && !n.groups && !n.location && !n.set && !n.currency);
            let hours: Vec<(&str, Option<Dec>)> = b
                .units
                .iter()
                .map(|(name, u)| (name.as_str(), u.hours))
                .collect();
            assert_eq!(
                hours,
                vec![
                    ("Allow", None),
                    ("Day", Some(Dec::from_int(12))),
                    ("Flat", None),
                    ("Hour", Some(Dec::from_int(1))),
                    ("Month", Some(Dec::from_int(240))),
                    ("Week", Some(Dec::from_int(60))),
                ]
            );
        }
    }

    #[test]
    fn recalcs_with_unique_ids() {
        let b = super::budget();
        let r = recalc(&b, &RecalcOptions::default()).unwrap();
        assert!(r.total_atl > 0 && r.total_btl > 0);
        assert!(r.grand_total > r.atl_btl);
        let ids: Vec<_> = b
            .categories()
            .flat_map(|c| &c.accounts)
            .flat_map(|a| &a.details)
            .map(|d| d.id)
            .collect();
        assert_eq!(ids.len(), ids.iter().collect::<BTreeSet<_>>().len());
    }
}
