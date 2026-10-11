//! A budget whose account numbers don't all follow its pattern, as a studio
//! chart's numbers are kept (mixed widths, letters, a trailing dot, a
//! repeated number given a new one): it recalculates, compares, matches
//! actuals and lookups by number, applies charts and renumbers without ever
//! reading one number as another.

use method_core::actuals::{add_batch, cost_report_of, Transaction};
use method_core::coa::{apply, ApplyOptions, Chart, ChartAccount, ChartCategory, ChartRow};
use method_core::coding::AccountCoding;
use method_core::comparison::compare;
use method_core::explain::Target;
use method_core::renumber::{renumber, Conflict, RenumberError};
use method_core::{
    recalc, Account, Budget, Category, Charge, ChargeKind, Dec, Detail, FringePosting,
    RecalcOptions, TopsheetRow,
};

fn two_plus_two() -> AccountCoding {
    AccountCoding {
        separator: None,
        ..AccountCoding::default()
    }
}

/// Categories and accounts, each account with one line of `amount`.
fn budget(chart: &[(&str, &[(&str, i64)])]) -> Budget {
    let mut b = Budget::new("Studio chart", "USD");
    b.account_coding = two_plus_two();
    b.fringe_posting = FringePosting::Account;
    let mut id = 0;
    for (cat, accounts) in chart {
        b.topsheet.push(TopsheetRow::Category(Category {
            code: cat.to_string(),
            name: format!("Category {cat}"),
            accounts: accounts
                .iter()
                .map(|(code, amount)| {
                    id += 1;
                    Account {
                        code: code.to_string(),
                        name: format!("Account {code}"),
                        details: vec![Detail::line(id, "Line", 1, "Flat", *amount)],
                        notes: Vec::new(),
                    }
                })
                .collect(),
            notes: Vec::new(),
        }));
    }
    b
}

fn studio() -> Budget {
    budget(&[
        ("0501", &[("050101", 100), ("050102", 200)]),
        ("0502", &[]),
        ("1100", &[("1101", 300), ("1101-2", 400), ("4AA.", 500)]),
        ("1A00", &[("1A10", 50)]),
    ])
}

/// Each account's total in the cost report's actual column, by number.
fn actuals_by_account(b: &Budget) -> Vec<(String, i64)> {
    let r = recalc(b, &RecalcOptions::default()).unwrap();
    let c = cost_report_of(b, &r).unwrap();
    c.categories
        .iter()
        .flat_map(|c| &c.accounts)
        .map(|a| (a.code.clone(), a.figures.actual))
        .collect()
}

#[test]
fn recalculates_with_an_empty_category() {
    let b = studio();
    let r = recalc(&b, &RecalcOptions::default()).unwrap();
    let totals: Vec<i64> = r.categories.iter().map(|c| c.total).collect();
    assert_eq!(totals, [30_000, 0, 120_000, 5_000]);
    assert_eq!(r.grand_total, 155_000);
}

#[test]
fn actuals_go_to_the_account_they_name_and_nowhere_else() {
    let mut b = studio();
    let tx = |account: &str, amount| Transaction {
        account: account.into(),
        amount,
        ..Transaction::default()
    };
    add_batch(
        &mut b,
        "ledger.csv",
        "",
        vec![
            tx("050102", 7),
            tx("1101-2", 11),
            tx("4AA.", 13),
            tx("11-01", 17), // another spelling of 1101
            tx("0501", 19),  // a category, not an account: unassigned
            tx("05-01", 23), // 0501 again: unassigned, never 050101
        ],
    )
    .unwrap();
    assert_eq!(
        actuals_by_account(&b),
        [
            ("050101".to_string(), 0),
            ("050102".to_string(), 7),
            ("1101".to_string(), 17),
            ("1101-2".to_string(), 11),
            ("4AA.".to_string(), 13),
            ("1A10".to_string(), 0),
        ]
    );
    let r = recalc(&b, &RecalcOptions::default()).unwrap();
    assert_eq!(cost_report_of(&b, &r).unwrap().unassigned.actual, 42);
}

#[test]
fn comparison_keeps_every_account_apart() {
    let a = studio();
    let mut b = studio();
    if let Some(TopsheetRow::Category(c)) = b.topsheet.get_mut(2) {
        c.accounts[1].details[0] = Detail::line(99, "Line", 1, "Flat", 450);
    }
    let cmp = compare(&[("a".into(), &a), ("b".into(), &b)]).unwrap();
    let rows: Vec<(String, Option<i64>, Option<i64>)> = cmp
        .categories
        .iter()
        .flat_map(|c| &c.accounts)
        .map(|x| (x.code.clone(), x.line.values[0], x.line.values[1]))
        .collect();
    assert_eq!(rows.len(), 6);
    assert!(rows.contains(&("1101-2".to_string(), Some(40_000), Some(45_000))));
    assert!(rows.contains(&("050101".to_string(), Some(10_000), Some(10_000))));
    assert!(rows.contains(&("050102".to_string(), Some(20_000), Some(20_000))));
    // A category written as its part (`11`) is the same category as `1100`.
    let mut parts = studio();
    if let Some(TopsheetRow::Category(c)) = parts.topsheet.get_mut(2) {
        c.code = "11".into();
    }
    let cmp = compare(&[("a".into(), &a), ("b".into(), &parts)]).unwrap();
    assert_eq!(cmp.categories.len(), 4);
}

#[test]
fn lookups_by_number_find_the_account_named() {
    let b = studio();
    let r = recalc(&b, &RecalcOptions::default()).unwrap();
    let at = |text: &str| Target::parse(&b, &r, text).ok();
    assert_eq!(at("050102"), Some(Target::Account { ci: 0, ai: 1 }));
    assert_eq!(at("1101-2"), Some(Target::Account { ci: 2, ai: 1 }));
    assert_eq!(at("4AA."), Some(Target::Account { ci: 2, ai: 2 }));
    assert_eq!(at("11-01"), Some(Target::Account { ci: 2, ai: 0 }));
    // `0501` is a category here, never account 050101 cut short.
    assert_eq!(at("0501"), Some(Target::Category(0)));
}

#[test]
fn charges_exclude_exactly_the_accounts_named() {
    let mut b = studio();
    let base = method_core::starter::budget().charges[0].clone();
    let charge = Charge {
        name: "Fee".into(),
        account: None,
        applied: true,
        kind: ChargeKind::Percent(Dec::from_int(10)),
        excluded_amounts: Vec::new(),
        excluded_groups: Default::default(),
        excluded_categories: Default::default(),
        excluded_accounts: ["050101".to_string(), "1101-2".to_string()].into(),
        ..base
    };
    b.charges = vec![charge];
    let r = recalc(&b, &RecalcOptions::default()).unwrap();
    assert_eq!(r.charges[0].excluded, 10_000 + 40_000);
}

#[test]
fn applying_a_chart_matches_categories_by_the_category_they_name() {
    let mut b = studio();
    let chart = Chart {
        coding: AccountCoding::default(),
        rows: vec![ChartRow::Category(ChartCategory {
            code: "11".into(),
            name: "Writing".into(),
            accounts: vec![
                ChartAccount {
                    code: "11-01".into(),
                    name: "Writer".into(),
                },
                ChartAccount {
                    code: "11-02".into(),
                    name: "Researcher".into(),
                },
            ],
        })],
    };
    let rep = apply(&mut b, &chart, &ApplyOptions::default()).unwrap();
    assert!(rep.added_categories.is_empty(), "{rep:?}");
    assert_eq!(rep.renamed_categories[0].code, "1100");
    let codes: Vec<&str> = b
        .categories()
        .nth(2)
        .unwrap()
        .accounts
        .iter()
        .map(|a| a.code.as_str())
        .collect();
    assert_eq!(codes, ["1101", "1101-2", "1102", "4AA."]);
    assert_eq!(rep.renamed_accounts[0].code, "1101");
}

#[test]
fn renumbering_refuses_numbers_it_cannot_carry() {
    let mut b = studio();
    let before = b.clone();
    let to = AccountCoding {
        category_width: 3,
        account_width: 3,
        ..two_plus_two()
    };
    let err = renumber(&mut b, to).unwrap_err();
    let RenumberError::Conflicts(plan) = &err else {
        panic!("{err}");
    };
    let unfit: Vec<&str> = plan
        .conflicts
        .iter()
        .filter_map(|c| match c {
            Conflict::DoesNotFit { code, .. } => Some(code.as_str()),
            _ => None,
        })
        .collect();
    for code in ["0501", "050101", "050102", "1101-2"] {
        assert!(unfit.contains(&code), "{code} in {unfit:?}");
    }
    assert!(err.to_string().contains("doesn't fit"), "{err}");
    assert_eq!(b, before, "nothing changes");
}

#[test]
fn renumbering_keeps_full_width_category_numbers_in_their_form() {
    let mut b = budget(&[
        ("1100", &[("1101", 1), ("1102", 2)]),
        ("1200", &[("1201", 3)]),
    ]);
    let start = b.clone();
    let to = AccountCoding {
        category_width: 3,
        account_width: 3,
        ..AccountCoding::default()
    };
    renumber(&mut b, to).unwrap();
    let codes: Vec<String> = b
        .categories()
        .flat_map(|c| {
            std::iter::once(c.code.clone()).chain(c.accounts.iter().map(|a| a.code.clone()))
        })
        .collect();
    assert_eq!(
        codes,
        ["011-000", "011-001", "011-002", "012-000", "012-001"]
    );
    renumber(&mut b, two_plus_two()).unwrap();
    assert_eq!(b, start, "and back");
}
