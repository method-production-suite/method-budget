//! Comparing budgets and versions side by side.

mod common;

use common::*;
use fwb_core::coding::AccountCoding;
use fwb_core::*;

fn report(b: &Budget) -> Report {
    recalc(b, &RecalcOptions::default()).unwrap()
}

fn two<'a>(a: &'a Budget, b: &'a Budget) -> Vec<(String, &'a Budget)> {
    vec![("A".to_string(), a), ("B".to_string(), b)]
}

fn account_line<'a>(c: &'a Comparison, code: &str) -> &'a CompareLine {
    &c.categories
        .iter()
        .flat_map(|c| &c.accounts)
        .find(|a| a.code == code && !a.fringe_account)
        .unwrap_or_else(|| panic!("no account {code}"))
        .line
}

fn codes(c: &Comparison) -> Vec<(String, Vec<String>)> {
    c.categories
        .iter()
        .map(|cat| {
            (
                cat.code.clone(),
                cat.accounts.iter().map(|a| a.code.clone()).collect(),
            )
        })
        .collect()
}

fn acct_mut<'a>(b: &'a mut Budget, code: &str) -> &'a mut Account {
    b.categories_mut()
        .flat_map(|c| &mut c.accounts)
        .find(|a| a.code == code)
        .unwrap()
}

#[test]
fn matches_codes_through_each_budgets_coding() {
    let a = sample();
    // Same budget with every code spelled differently: separators, padding
    // and a different pattern separator all normalize to the same segments.
    let mut b = sample();
    b.account_coding = AccountCoding {
        separator: Some('.'),
        ..AccountCoding::default()
    };
    for cat in b.categories_mut() {
        cat.code = match cat.code.as_str() {
            "1000" => "10.00".into(),
            "2000" => "20 00".into(),
            other => other.into(),
        };
        for acct in &mut cat.accounts {
            acct.code = match acct.code.as_str() {
                "1100" => "11.00".into(),
                "2100" => "21-0".into(),
                other => other.into(),
            };
        }
    }
    let c = compare(&two(&a, &b)).unwrap();
    assert_eq!(c.labels, vec!["A", "B"]);
    // Codes and names are written as in the first budget; nothing unmatched.
    assert_eq!(
        codes(&c),
        vec![
            ("1000".into(), vec!["1100".into(), "10-99".into()]),
            ("2000".into(), vec!["2100".into(), "20-99".into()]),
            ("5000".into(), vec!["5100".into(), "50-99".into()]),
        ]
    );
    for cat in &c.categories {
        for line in std::iter::once(&cat.line).chain(cat.accounts.iter().map(|a| &a.line)) {
            assert_eq!(line.values[0], line.values[1]);
            assert!(line.diffs.iter().all(|d| d.amount == 0));
        }
    }
    assert!(c.categories[0].accounts[1].fringe_account);
    assert_eq!(c.categories[0].accounts[0].name, "Writing");
}

#[test]
fn raw_codes_are_the_fallback() {
    let mut a = sample();
    let mut b = sample();
    // `11?00` doesn't fit the pattern, so it only matches the same text.
    acct_mut(&mut a, "1100").code = " 11?00".into();
    acct_mut(&mut b, "1100").code = "11?00 ".into();
    let c = compare(&two(&a, &b)).unwrap();
    let line = account_line(&c, " 11?00");
    assert!(line.values.iter().all(Option::is_some));
}

#[test]
fn accounts_in_only_some_budgets_are_listed_in_order() {
    let a = sample();
    let mut b = sample();
    // B drops Writing, adds a new account in Production and a new category.
    b.categories_mut().next().unwrap().accounts.clear();
    b.categories_mut().nth(1).unwrap().accounts.insert(
        0,
        account(
            "2200",
            "Art",
            vec![Detail::line(20, "Painter", 5, "Day", 300)],
        ),
    );
    b.topsheet.push(category(
        "6000",
        "Other",
        vec![account(
            "6100",
            "Misc",
            vec![Detail::line(21, "Fee", 1, "Flat", 99)],
        )],
    ));
    let mut c3 = sample();
    c3.topsheet.push(category(
        "7000",
        "Later",
        vec![account("7100", "Late", vec![])],
    ));
    let budgets = vec![
        ("A".to_string(), &a),
        ("B".to_string(), &b),
        ("C".to_string(), &c3),
    ];
    let c = compare(&budgets).unwrap();
    assert_eq!(
        codes(&c),
        vec![
            ("1000".into(), vec!["1100".into(), "10-99".into()]),
            // First budget's order, then B's new account.
            (
                "2000".into(),
                vec!["2100".into(), "20-99".into(), "2200".into()]
            ),
            ("5000".into(), vec!["5100".into(), "50-99".into()]),
            ("6000".into(), vec!["6100".into(), "60-99".into()]),
            ("7000".into(), vec!["7100".into(), "70-99".into()]),
        ]
    );

    let writing = report(&a).categories[0].accounts[0].total;
    let w = account_line(&c, "1100");
    assert_eq!(w.values, vec![Some(writing), None, Some(writing)]);
    assert_eq!(w.diffs[1].amount, -writing);
    assert_eq!(w.diffs[1].percent, Some(d("-100")));
    assert_eq!(
        w.diffs[2],
        Diff {
            amount: 0,
            percent: Some(Dec::ZERO)
        }
    );

    let art = account_line(&c, "2200");
    assert_eq!(art.values, vec![None, Some(150_000), None]);
    assert_eq!(
        art.diffs[1],
        Diff {
            amount: 150_000,
            percent: None
        }
    );
    assert_eq!(
        art.diffs[2],
        Diff {
            amount: 0,
            percent: None
        }
    );

    let late = account_line(&c, "7100");
    assert_eq!(late.values, vec![None, None, Some(0)]);
}

#[test]
fn percent_edge_cases() {
    let mut a = Budget::new("a", "USD");
    a.topsheet = vec![category(
        "10",
        "Cat",
        vec![
            account("10-01", "Eight", vec![Detail::line(1, "x", 1, "Flat", 8)]),
            account("10-02", "Zero", vec![]),
            account("10-03", "Neg", vec![Detail::line(2, "x", 1, "Flat", -8)]),
            account("10-04", "Third", vec![Detail::line(3, "x", 1, "Flat", 3)]),
        ],
    )];
    let mut b = a.clone();
    let rates = [("10-01", 9), ("10-02", 5), ("10-03", -10), ("10-04", 4)];
    for (i, (code, rate)) in rates.into_iter().enumerate() {
        let acct = acct_mut(&mut b, code);
        acct.details = vec![Detail::line(10 + i as RowId, "x", 1, "Flat", rate)];
    }
    let c = compare(&two(&a, &b)).unwrap();
    // 8 -> 9 is +12.5%.
    assert_eq!(
        account_line(&c, "10-01").diffs[1],
        Diff {
            amount: 100,
            percent: Some(d("12.5"))
        }
    );
    // From zero: no percent.
    assert_eq!(
        account_line(&c, "10-02").diffs[1],
        Diff {
            amount: 500,
            percent: None
        }
    );
    // -8 -> -10 is -2 against a base of size 8: -25%, same sign as the amount.
    assert_eq!(
        account_line(&c, "10-03").diffs[1],
        Diff {
            amount: -200,
            percent: Some(d("-25"))
        }
    );
    // 3 -> 4 is 33.333333333% (9 places, rounded half away from zero).
    assert_eq!(
        account_line(&c, "10-04").diffs[1].percent,
        Some(d("33.333333333"))
    );
    // The first budget's own diffs are zero, with percent when its value isn't 0.
    assert_eq!(
        account_line(&c, "10-01").diffs[0],
        Diff {
            amount: 0,
            percent: Some(Dec::ZERO)
        }
    );
    assert_eq!(account_line(&c, "10-02").diffs[0].percent, None);
}

#[test]
fn budget_totals_are_compared() {
    let a = sample();
    let mut b = sample();
    b.charges.clear();
    b.credits.clear();
    let (ra, rb) = (report(&a), report(&b));
    let c = compare(&two(&a, &b)).unwrap();
    let t = &c.totals;
    assert_eq!(
        t.grand_total.values,
        vec![Some(ra.grand_total), Some(rb.grand_total)]
    );
    assert_eq!(
        t.net_total.values,
        vec![Some(ra.net_total), Some(rb.net_total)]
    );
    assert_eq!(
        t.total_atl.values,
        vec![Some(ra.total_atl), Some(rb.total_atl)]
    );
    assert_eq!(
        t.total_btl.values,
        vec![Some(ra.total_btl), Some(rb.total_btl)]
    );
    let charges: i64 = ra.charges.iter().map(|c| c.amount).sum();
    assert!(charges > 0);
    assert_eq!(t.charges.values, vec![Some(charges), Some(0)]);
    assert_eq!(t.charges.diffs[1].percent, Some(d("-100")));
    assert_eq!(t.credits.values, vec![Some(ra.credits_total), Some(0)]);
    let fringes: i64 = ra.fringes.values().map(|f| f.total).sum();
    assert!(fringes > 0);
    assert_eq!(t.fringes.values, vec![Some(fringes), Some(fringes)]);
    assert_eq!(
        t.grand_total.diffs[1].amount,
        rb.grand_total - ra.grand_total
    );
}

#[test]
fn takes_two_to_sixteen_budgets() {
    let b = sample();
    let many =
        |n: usize| -> Vec<(String, &Budget)> { (0..n).map(|i| (format!("B{i}"), &b)).collect() };
    for n in [0, 1, 17] {
        let e = compare(&many(n)).unwrap_err();
        assert_eq!(e.kind, ErrorKind::CompareCount(n));
        assert!(compare_account(&many(n), "1100", RowMatch::Id).is_err());
    }
    assert_eq!(
        compare(&many(1)).unwrap_err().to_string(),
        "comparison: a comparison takes 2 to 16 budgets, not 1"
    );
    let c = compare(&many(16)).unwrap();
    assert_eq!(c.labels.len(), 16);
    assert!(c
        .categories
        .iter()
        .all(|cat| cat.line.values.len() == 16 && cat.line.diffs.len() == 16));
    assert!(compare(&many(2)).is_ok());
}

#[test]
fn errors_name_the_budget() {
    let a = sample();
    let mut b = sample();
    acct_mut(&mut b, "1100").details[0].rate = "NOPE".into();
    let e = compare(&two(&a, &b)).unwrap_err();
    assert!(e.at.starts_with("B: account 1100"), "{}", e.at);
}

#[test]
fn account_rows_match_by_id_or_description() {
    let a = sample();
    let mut b = sample();
    {
        let crew = acct_mut(&mut b, "2100");
        // Key grip gets a new id and a new rate; a new row is added; the
        // overtime row is removed.
        crew.details[0].id = 40;
        crew.details[0].rate = "500".into();
        crew.details.retain(|d| d.id != 3);
        crew.details
            .push(Detail::line(41, "  KEY GRIP ", 1, "Flat", 10));
    }
    let (ra, rb) = (report(&a), report(&b));
    let crew_a = &ra.categories[1].accounts[0];
    let crew_b = &rb.categories[1].accounts[0];
    let row_total =
        |acct: &AccountTotals, id: RowId| acct.rows.iter().find(|r| r.id == id).unwrap().total;

    let by_id = compare_account(&two(&a, &b), "21-00", RowMatch::Id).unwrap();
    assert_eq!((by_id.code.as_str(), by_id.name.as_str()), ("2100", "Crew"));
    let ids: Vec<RowId> = by_id.rows.iter().map(|r| r.id).collect();
    // Costed rows only (no subtotal), A's order then B's new rows.
    assert_eq!(ids, vec![2, 3, 5, 6, 7, 40, 41]);
    let grip = &by_id.rows[0];
    assert_eq!(grip.description, "Key grip");
    assert_eq!(grip.line.values, vec![Some(row_total(crew_a, 2)), None]);
    assert_eq!(
        by_id.rows[5].line.values,
        vec![None, Some(row_total(crew_b, 40))]
    );
    let suppressed = by_id.rows.iter().find(|r| r.id == 7).unwrap();
    assert_eq!(suppressed.line.values, vec![Some(0), Some(0)]);
    assert_eq!(
        by_id.total.values,
        vec![Some(crew_a.total), Some(crew_b.total)]
    );
    assert_eq!(
        by_id.details_total.values,
        vec![Some(crew_a.details_total), Some(crew_b.details_total)]
    );
    assert_eq!(
        by_id.fringe_total.values,
        vec![Some(crew_a.fringe_total), Some(crew_b.fringe_total)]
    );

    let by_desc = compare_account(&two(&a, &b), "2100", RowMatch::Description).unwrap();
    let descs: Vec<&str> = by_desc
        .rows
        .iter()
        .map(|r| r.description.as_str())
        .collect();
    assert_eq!(
        descs,
        vec![
            "Key grip",
            "Key grip overtime",
            "Camera package",
            "Crew contingency",
            "Second unit grip",
            // The second "key grip" in B has no partner in A.
            "  KEY GRIP ",
        ]
    );
    assert_eq!(
        by_desc.rows[0].line.values,
        vec![Some(row_total(crew_a, 2)), Some(row_total(crew_b, 40))]
    );
    assert_eq!(by_desc.rows[1].line.values[1], None);
    assert_eq!(by_desc.rows[5].line.values, vec![None, Some(1_000)]);
}

#[test]
fn account_missing_everywhere_is_empty() {
    let a = sample();
    let c = compare_account(&two(&a, &a), "99-99", RowMatch::Id).unwrap();
    assert_eq!((c.code.as_str(), c.name.as_str()), ("99-99", ""));
    assert!(c.rows.is_empty());
    assert_eq!(c.total.values, vec![None, None]);
    assert_eq!(
        c.total.diffs[1],
        Diff {
            amount: 0,
            percent: None
        }
    );
}

#[test]
fn versions_compare_with_the_current_budget() {
    let mut b = sample();
    let v = save_version(&mut b, "Approved", "2026-10-09T10:00:00Z", None);
    acct_mut(&mut b, "1100").details[0].rate = "160000".into();
    let old = b.version(v).unwrap().budget.to_budget();
    let c = compare(&[("Approved".to_string(), &old), ("Current".to_string(), &b)]).unwrap();
    let w = account_line(&c, "1100");
    assert_eq!(w.diffs[1].amount, 1_000_000);
    assert_eq!(w.diffs[1].percent, Some(d("6.666666667")));
}
