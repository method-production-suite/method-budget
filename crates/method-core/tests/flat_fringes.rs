//! How flat fringes count (decisions.md, entry 54), as checked in the
//! reference product on 2026-10-11 with units Day = 12 hours, Week = 60,
//! Hour = 1 and Allow with no hours, a 100.00 per Day fringe ZDAY and a
//! 1.00 per Hour fringe ZHR. Exact dollar figures, through the public
//! engine, Explain and the cache.

mod common;

use common::*;
use method_core::calc::cache::RecalcCache;
use method_core::explain::{explain, Explainer, Step, Target};
use method_core::*;

fn unit(plural: &str, hours: Option<&str>) -> UnitDef {
    UnitDef {
        plural: plural.into(),
        description: String::new(),
        hours: hours.map(d),
    }
}

fn flat(rate: &str, unit: &str, basis: FlatBasis, cap: Option<&str>) -> Fringe {
    Fringe {
        description: String::new(),
        short_code: None,
        rate: rate.into(),
        kind: FringeKind::Flat {
            unit: Some(unit.into()),
            basis,
            max_hours_per_day: cap.map(d),
            cutoff: None,
        },
    }
}

/// `amount unit [x X unit2] [x4 4X unit4]` at `rate`, carrying ZDAY and ZHR.
fn line(
    id: u64,
    what: &str,
    amount: &str,
    unit: &str,
    x: Option<(&str, &str)>,
    x4: Option<(&str, &str)>,
    rate: &str,
) -> Detail {
    let mut l = Detail::line(id, what, amount, unit, rate);
    if let Some((v, u)) = x {
        l.x = v.into();
        l.unit2 = u.into();
    }
    if let Some((v, u)) = x4 {
        l.x4 = v.into();
        l.unit4 = u.into();
    }
    l.fringes = set(&["ZDAY", "ZHR"]);
    l
}

/// The checked lines, each carrying both fringes counted by `basis` (ZHR
/// with `cap`).
fn checked(basis: FlatBasis, cap: Option<&str>) -> Budget {
    let mut b = Budget::new("Flat fringe check", "USD");
    b.units.clear();
    b.units.insert("Day".into(), unit("Days", Some("12")));
    b.units.insert("Week".into(), unit("Weeks", Some("60")));
    b.units.insert("Hour".into(), unit("Hours", Some("1")));
    b.units.insert("Allow".into(), unit("Allow", None));
    b.units.insert("Episode".into(), unit("Episodes", None));
    b.fringes
        .insert("ZDAY".into(), flat("100", "Day", basis, None));
    b.fringes
        .insert("ZHR".into(), flat("1", "Hour", basis, cap));
    b.topsheet = vec![category(
        "10",
        "Crew",
        vec![account(
            "10-01",
            "Grips",
            vec![
                line(1, "25 days", "25", "Day", None, None, "100"),
                line(2, "3 crew", "25", "Day", Some(("3", "")), None, "100"),
                line(
                    3,
                    "14-hour days",
                    "25",
                    "Day",
                    Some(("14", "Hour")),
                    None,
                    "100",
                ),
                line(
                    4,
                    "Two units",
                    "25",
                    "Day",
                    Some(("14", "Hour")),
                    Some(("2", "")),
                    "10",
                ),
                line(5, "Weeks", "2", "Week", None, None, "100"),
                line(6, "Hours", "300", "Hour", None, None, "100"),
                line(7, "Allowance", "2", "Allow", None, None, "100"),
                line(
                    8,
                    "Episodes",
                    "25",
                    "Day",
                    Some(("14", "Hour")),
                    Some(("2", "Episode")),
                    "1",
                ),
            ],
        )],
    )];
    b
}

/// Each line's (ZDAY, ZHR) in cents.
fn figures(b: &Budget) -> Vec<(Cents, Cents)> {
    let r = recalc(b, &RecalcOptions::default()).unwrap();
    r.categories[0].accounts[0]
        .rows
        .iter()
        .map(|row| (row.fringes["ZDAY"], row.fringes["ZHR"]))
        .collect()
}

#[test]
fn the_reference_products_figures() {
    let b = checked(FlatBasis::Amount, None);
    assert_eq!(
        figures(&b),
        [
            (250_000, 30_000), // 25 Days: 2,500.00; 25 x 12 hours = 300.00
            (250_000, 30_000), // x 3 (no unit): still 2,500.00
            (250_000, 30_000), // x 14 Hours: still 2,500.00
            (250_000, 30_000), // x 14 Hours x 4X 2: 2,500.00; ZHR 300.00
            (100_000, 12_000), // 2 Weeks: 10 days, 1,000.00; 120 hours, 120.00
            (250_000, 30_000), // 300 Hours: 25 days, 2,500.00; 300.00
            (10_000, 100),     // 2 Allow: charged once, 100.00 and 1.00
            (250_000, 30_000), // x 14 Hours x 2 Episodes: amount only
        ]
    );
    let r = recalc(&b, &RecalcOptions::default()).unwrap();
    assert_eq!(r.categories[0].accounts[0].rows[3].total, 700_000);
    // ZDAY over the eight lines: six of 2,500.00, 1,000.00 and 100.00.
    assert_eq!(r.fringes["ZDAY"].total, 1_610_000);
}

#[test]
fn amount_plus_counts_and_full_quantity() {
    let counts = figures(&checked(FlatBasis::AmountCounts, None));
    // 3 crew x 25 days = 75 days; 14 Hours ignored; 2 Episodes counted.
    assert_eq!(counts[1].0, 750_000);
    assert_eq!(counts[2].0, 250_000);
    assert_eq!(counts[3].0, 500_000, "a 4X with no unit counts too");
    assert_eq!(counts[7].0, 500_000);
    assert_eq!(counts[6], (10_000, 100), "still once on Allow");

    // The rule before decisions.md 54: Amount x X x 4X through hours.
    let quantity = figures(&checked(FlatBasis::Quantity, None));
    assert_eq!(quantity[1].0, 750_000);
    assert_eq!(quantity[2].0, 3_500_000, "25 x 14 x 100.00");
    assert_eq!(quantity[3].0, 7_000_000);
    assert_eq!(quantity[4], (100_000, 12_000));
    assert_eq!(quantity[6], (10_000, 100));
}

#[test]
fn max_hours_per_day_in_both_shapes() {
    let zhr = |cap: &str| -> Vec<Cents> {
        figures(&checked(FlatBasis::Amount, Some(cap)))
            .into_iter()
            .map(|(_, h)| h)
            .collect()
    };
    // "25 Days": hours a day are the Day's 12; "25 Days x 14 Hours": X's 14.
    assert_eq!(zhr("12")[0], 30_000);
    assert_eq!(zhr("10")[0], 25_000);
    assert_eq!(zhr("12")[2], 30_000);
    assert_eq!(zhr("10")[2], 25_000);
    assert_eq!(zhr("16")[2], 35_000, "below the cap X's 14 hours all count");
    assert_eq!(zhr("10")[4], 10_000, "2 Weeks = 10 days x 10 hours");
    assert_eq!(zhr("10")[5], 25_000, "300 Hours = 25 days x 10 hours");
    assert_eq!(zhr("10")[6], 100, "no hours on the line: once");
    // ZDAY is per Day: a cap there would change nothing.
    let mut b = checked(FlatBasis::Amount, None);
    b.fringes.insert(
        "ZDAY".into(),
        flat("100", "Day", FlatBasis::Amount, Some("10")),
    );
    assert_eq!(figures(&b)[0].0, 250_000);
}

fn text_of(s: &Step) -> String {
    let mut out = format!("{}: {}\n", s.label, s.text);
    for c in &s.children {
        out.push_str(&text_of(c));
    }
    out
}

fn fringe_text(b: &Budget, row: RowId, name: &str) -> (Cents, String) {
    let e = explain(
        b,
        &RecalcOptions::default(),
        &Target::LineFringe(row, name.into()),
    )
    .unwrap();
    assert_eq!(e.root.leaf_sum(), Some(i128::from(e.reported)));
    (e.reported, text_of(&e.root))
}

#[test]
fn explain_says_the_rule_and_the_count() {
    let b = checked(FlatBasis::Amount, None);
    let (amount, text) = fringe_text(&b, 3, "ZDAY");
    assert_eq!(amount, 250_000);
    assert!(
        text.contains(
            "ZDAY 100.00 per Day x 25 Days (amount only; 14 Hours in X ignored) = 2,500.00"
        ),
        "{text}"
    );
    let (_, text) = fringe_text(&b, 7, "ZDAY");
    assert!(
        text.contains("ZDAY 100.00 per Day x 1 (Allow has no hours: charged once) = 100.00"),
        "{text}"
    );
    let (_, text) = fringe_text(&b, 5, "ZDAY");
    assert!(
        text.contains("2 Weeks x 60 hours per Week / 12 hours per Day = 10 Days"),
        "{text}"
    );
    let (_, text) = fringe_text(&b, 4, "ZHR");
    assert!(
        text.contains("25 Days x 12 hours per Day / 1 hour per Hour = 300 Hours"),
        "{text}"
    );
    assert!(text.contains("14 Hours in X and 2 in 4X ignored"), "{text}");

    let counts = checked(FlatBasis::AmountCounts, None);
    let (amount, text) = fringe_text(&counts, 2, "ZDAY");
    assert_eq!(amount, 750_000);
    assert!(text.contains("25 Days x 3 (X) = 75 Days"), "{text}");
    assert!(
        text.contains("(amount plus counts in X and 4X) = 7,500.00"),
        "{text}"
    );

    let capped = checked(FlatBasis::Amount, Some("12"));
    let (amount, text) = fringe_text(&capped, 3, "ZHR");
    assert_eq!(amount, 30_000);
    assert!(text.contains("capped at 12 hours a day"), "{text}");
    assert!(
        text.contains(
            "14 hours a day from X, capped at 12 hours a day: 25 days x 12 hours = 300 Hours"
        ),
        "{text}"
    );
    let (amount, text) = fringe_text(&checked(FlatBasis::Amount, Some("10")), 1, "ZHR");
    assert_eq!(amount, 25_000);
    assert!(
        text.contains(
            "12 hours a day (one Day), capped at 10 hours a day: 25 days x 10 hours = 250 Hours"
        ),
        "{text}"
    );

    let quantity = checked(FlatBasis::Quantity, None);
    let (_, text) = fringe_text(&quantity, 2, "ZDAY");
    assert!(
        text.contains("the line's quantity (amount x X x 4X) is 75 Days"),
        "{text}"
    );
    assert!(text.contains("(full quantity) = 7,500.00"), "{text}");
}

/// Every total explained adds up, the cache agrees, for every basis and cap.
#[test]
fn explain_and_the_cache_agree_for_every_basis() {
    let mut cache = RecalcCache::new();
    for basis in FlatBasis::ALL {
        for cap in [None, Some("8"), Some("12"), Some("16")] {
            let b = checked(basis, cap);
            let opts = RecalcOptions::default();
            let ex = Explainer::new(&b, &opts).unwrap();
            for t in ex.targets() {
                let e = ex.explain(&t).unwrap();
                assert_eq!(e.reported, ex.reported(&t).unwrap(), "{t:?}");
                assert_eq!(e.root.leaf_sum(), Some(i128::from(e.reported)), "{t:?}");
            }
            assert_eq!(cache.recalc(&b, &opts), recalc(&b, &opts));
        }
    }
}

/// A cutoff kept on a flat fringe (from a file or an import) changes nothing.
#[test]
fn a_cutoff_on_a_flat_fringe_is_ignored() {
    let mut b = checked(FlatBasis::Amount, None);
    let before = figures(&b);
    if let FringeKind::Flat { cutoff, .. } = &mut b.fringes.get_mut("ZDAY").unwrap().kind {
        *cutoff = Some(100);
    }
    assert_eq!(figures(&b), before);
    assert_eq!(b.fringes["ZDAY"].kind.cutoff(), Some(100));
    let (_, text) = fringe_text(&b, 1, "ZDAY");
    assert!(
        text.contains("1.00 is kept on ZDAY but not used: cutoffs apply to percent fringes only"),
        "{text}"
    );
}
