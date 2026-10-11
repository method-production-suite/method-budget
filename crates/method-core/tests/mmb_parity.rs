//! Movie Magic arithmetic (decisions.md, entry 55): the figures the parity
//! run of 2026-10-10 read in Movie Magic Budgeting 10, reproduced through
//! the public engine, Explain and the cache, with exact arithmetic's
//! figures alongside to show it is unchanged.

mod common;

use common::*;
use method_core::calc::cache::RecalcCache;
use method_core::explain::{Explainer, Target};
use method_core::*;

/// `id` `desc`: `amount` x `x` x `x4` at `rate` (blank strings are blank).
fn line(id: u64, desc: &str, amount: &str, x: &str, rate: &str, x4: &str) -> Detail {
    Detail {
        x: x.into(),
        x4: x4.into(),
        ..Detail::line(id, desc, amount, "", rate)
    }
}

fn budget(arith: Arithmetic, topsheet: Vec<TopsheetRow>) -> Budget {
    let mut b = Budget::new("ZTEST", "USD");
    b.arithmetic = arith;
    b.topsheet = topsheet;
    b
}

/// Every row total of `b`, by row id, and the grand total; checked against
/// the cache and against Explain on the way.
fn totals(b: &Budget) -> (std::collections::BTreeMap<u64, Cents>, Report) {
    let opts = RecalcOptions::default();
    let r = recalc(b, &opts).unwrap();
    assert_eq!(RecalcCache::new().recalc(b, &opts).unwrap(), r);
    let ex = Explainer::new(b, &opts).unwrap();
    for t in ex.targets() {
        let e = ex.explain(&t).unwrap();
        assert_eq!(e.root.leaf_sum(), Some(i128::from(e.reported)), "{t:?}");
    }
    let rows = r
        .categories
        .iter()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.rows)
        .map(|row| (row.id, row.total))
        .collect();
    (rows, r)
}

/// ZTEST01 LINE MATH, as the parity plan lays it out (section 4.1).
fn ztest01(arith: Arithmetic) -> Budget {
    let l = line;
    let mut lm04 = l(4, "Units are labels", "2", "3", "100", "1");
    lm04.unit = "Day".into();
    lm04.unit2 = "Week".into();
    lm04.unit3 = "Allow".into();
    lm04.unit4 = "Month".into();
    budget(
        arith,
        vec![
            category(
                "1100",
                "Z line math",
                vec![
                    account(
                        "1101",
                        "Z factors",
                        vec![
                            l(1, "Two allow", "2", "", "100", ""),
                            l(2, "Two by three", "2", "3", "100", ""),
                            l(3, "Four factors", "2", "3", "100", "4"),
                            lm04,
                            l(5, "Blank amount", "", "", "50", ""),
                            l(6, "Blank rate", "1", "", "", ""),
                            l(7, "Zero amount", "0", "", "100", ""),
                            l(8, "Zero X", "1", "0", "100", ""),
                            l(9, "ZTITLE ONLY", "", "", "", ""),
                            l(10, "Negative amount", "-2", "", "100", ""),
                            l(11, "Negative rate", "2", "", "-100", ""),
                            l(12, "Both negative", "-2", "", "-100", ""),
                        ],
                    ),
                    account(
                        "1102",
                        "Z decimals",
                        vec![
                            l(13, "Half cent", "3", "", "432.105", ""),
                            l(14, "Float trap 1.005", "1", "", "1.005", ""),
                            l(15, "Float trap 2.675", "1", "", "2.675", ""),
                            l(16, "Even half 0.125", "1", "", "0.125", ""),
                            l(17, "Float trap 0.145", "1", "", "0.145", ""),
                            l(18, "Negative half", "-1", "", "0.125", ""),
                            l(19, "Product half", "3", "", "0.335", ""),
                            l(20, "Nine places", "1000", "", "0.123456789", ""),
                            l(21, "Below a cent", "0.5", "", "0.009999999", ""),
                            l(22, "Decimal factors", "2.5", "1.5", "33.33", ""),
                            l(23, "Fraction amount", "1/3", "", "30000000.015", ""),
                        ],
                    ),
                ],
            ),
            category(
                "1200",
                "Z formulas and size",
                vec![account(
                    "1201",
                    "Z formulas and size",
                    vec![
                        l(25, "Formula amount", "2*(3+4)", "", "10", ""),
                        l(26, "Division amount", "10/4", "", "10", ""),
                        l(27, "Big rate", "1", "", "999999999999.99", ""),
                        l(28, "Big half cent", "3", "", "3333333333333.335", ""),
                        l(29, "Big negative", "-1", "", "999999999999.99", ""),
                    ],
                )],
            ),
        ],
    )
}

/// Line totals read in Movie Magic (run-log, ZTEST01), in cents.
const ZTEST01_MMB: [(u64, Cents); 28] = [
    (1, 20_000),
    (2, 60_000),
    (3, 240_000),
    (4, 60_000),
    (5, 0),
    (6, 0),
    (7, 0),
    (8, 0),
    (9, 0),
    (10, -20_000),
    (11, -20_000),
    (12, 20_000),
    (13, 129_632),
    (14, 100),
    (15, 268),
    (16, 13),
    (17, 14),
    (18, -12),
    (19, 101),
    (20, 12_346),
    (21, 0),
    (22, 12_499),
    (23, 1_000_000_000),
    (25, 14_000),
    (26, 2_500),
    (27, 99_999_999_999_999),
    (28, 1_000_000_000_000_000),
    (29, -99_999_999_999_999),
];

#[test]
fn ztest01_line_math_matches_movie_magic() {
    let b = ztest01(Arithmetic::Mmb);
    let (rows, r) = totals(&b);
    for (id, want) in ZTEST01_MMB {
        assert_eq!(rows[&id], want, "LM-{id:02}");
    }
    let accounts: Vec<Cents> = r
        .categories
        .iter()
        .flat_map(|c| &c.accounts)
        .map(|a| a.total)
        .collect();
    assert_eq!(
        accounts,
        [360_000, 1_000_154_961, 1_000_000_000_016_500],
        "1101, 1102, 1201"
    );
    // Grand 10,000,010,005,314.61.
    assert_eq!(r.grand_total, 1_000_001_000_531_461);
}

#[test]
fn ztest01_in_exact_arithmetic_is_unchanged() {
    let b = ztest01(Arithmetic::Exact);
    let (rows, r) = totals(&b);
    // Where exact decimals and Movie Magic part ways.
    let differ = [
        (14, 101),
        (17, 15),
        (18, -13),
        (23, 999_999_999),
        (28, 1_000_000_000_000_001),
    ];
    for (id, want) in ZTEST01_MMB {
        let want = differ
            .iter()
            .find(|(d, _)| *d == id)
            .map_or(want, |(_, w)| *w);
        assert_eq!(rows[&id], want, "LM-{id:02}");
    }
    assert_eq!(r.grand_total, 1_000_001_000_531_462);
}

/// A flat fringe of 0.06 per 12-hour Day on 1 Hour: 0.06 x (1 / 12) is
/// 0.004999... in doubles, so 0.00 (F-19); exact arithmetic makes it 0.01.
#[test]
fn a_tiny_flat_fringe_rounds_on_the_double() {
    for (arith, want) in [(Arithmetic::Mmb, 0), (Arithmetic::Exact, 1)] {
        let mut b = budget(
            arith,
            vec![category(
                "1100",
                "Z flat",
                vec![account(
                    "1101",
                    "Z flat",
                    vec![Detail {
                        fringes: set(&["ZFTINY"]),
                        ..Detail::line(1, "One hour", "1", "Hour", "100")
                    }],
                )],
            )],
        );
        b.units.get_mut("Day").unwrap().hours = Some(d("12"));
        b.fringes.insert(
            "ZFTINY".into(),
            Fringe {
                description: String::new(),
                short_code: None,
                rate: "0.06".into(),
                kind: FringeKind::flat(Some("Day".into())),
            },
        );
        let (_, r) = totals(&b);
        let row = &r.categories[0].accounts[0].rows[0];
        assert_eq!(row.fringes["ZFTINY"], want, "{arith:?}");
    }
}

/// 12.5% of a 5,000.00 line with a 2,222.22 cutoff is 277.78 (P-02); at
/// the cutoff doubled, 4,444.44 x 12.5% is 555.555 exactly but 555.55499...
/// in doubles, so 555.55 (V04-2), where exact arithmetic gives 555.56.
#[test]
fn a_percent_fringe_at_a_cutoff_rounds_on_the_double() {
    for (arith, cutoff, want) in [
        (Arithmetic::Mmb, 222_222, 27_778),
        (Arithmetic::Mmb, 444_444, 55_555),
        (Arithmetic::Exact, 222_222, 27_778),
        (Arithmetic::Exact, 444_444, 55_556),
    ] {
        let mut b = budget(
            arith,
            vec![category(
                "1100",
                "Z percent",
                vec![account(
                    "1101",
                    "Z single earners",
                    vec![
                        Detail {
                            fringes: set(&["ZP125"]),
                            ..Detail::line(1, "Above cutoff", "1", "", "5000")
                        },
                        // P-06: 0.04 x 12.5% is 0.005, a cent either way.
                        Detail {
                            fringes: set(&["ZP125"]),
                            ..Detail::line(2, "Half cent fringe", "1", "", "0.04")
                        },
                    ],
                )],
            )],
        );
        b.fringes.insert(
            "ZP125".into(),
            Fringe {
                description: String::new(),
                short_code: None,
                rate: "12.5".into(),
                kind: FringeKind::Percent {
                    cutoff: Some(cutoff),
                },
            },
        );
        let (_, r) = totals(&b);
        let rows = &r.categories[0].accounts[0].rows;
        assert_eq!(rows[0].fringes["ZP125"], want, "{arith:?} {cutoff}");
        assert_eq!(rows[1].fringes["ZP125"], 1, "{arith:?}");
    }
}

/// 0.4425% of 22,600.00 is 100.005 and of 21,800.00 96.465: both land
/// above the half in doubles, so 100.01 and 96.47, as in exact arithmetic
/// (ZTEST09 ZHALF).
#[test]
fn a_half_cent_charge_rounds_up() {
    for arith in Arithmetic::ALL {
        for (base, want) in [("22600", 10_001), ("21800", 9_647)] {
            let mut b = budget(
                arith,
                vec![category(
                    "1100",
                    "Z charges",
                    vec![account(
                        "1101",
                        "Z base",
                        vec![Detail::line(1, "Base", "1", "", base)],
                    )],
                )],
            );
            b.charges.push(percent_charge("ZHALF", "0.4425"));
            let (_, r) = totals(&b);
            assert_eq!(r.charges[0].amount, want, "{arith:?} {base}");
        }
    }
}

/// 0.335 EUR at 3 USD per EUR: Movie Magic converts the unrounded amount,
/// 1.005 in doubles (1.00500000000000011...), so 1.01 (CU-04); exact
/// arithmetic rounds 0.34 EUR first, so 1.02.
#[test]
fn a_foreign_line_converts_before_it_rounds() {
    for (arith, want) in [(Arithmetic::Mmb, 101), (Arithmetic::Exact, 102)] {
        let mut b = budget(
            arith,
            vec![category(
                "1100",
                "Z currencies",
                vec![account(
                    "1101",
                    "Z currencies",
                    vec![Detail {
                        currency: Some("EUR".into()),
                        ..Detail::line(1, "Euro half cent", "1", "", "0.335")
                    }],
                )],
            )],
        );
        b.currencies.insert(
            "EUR".into(),
            Currency {
                description: String::new(),
                symbol: String::new(),
                rate: d("3"),
            },
        );
        let (_, r) = totals(&b);
        let row = &r.categories[0].accounts[0].rows[0];
        assert_eq!(row.local_total, 34, "{arith:?}: shown in euros");
        assert_eq!(row.total, want, "{arith:?}");
        let text = method_core::explain::explain(&b, &RecalcOptions::default(), &Target::Line(1))
            .unwrap()
            .to_text(None);
        let step = match arith {
            Arithmetic::Mmb => {
                "0.335 EUR (not rounded yet) x 3 USD per EUR = 1.0050000000000001 USD"
            }
            Arithmetic::Exact => "0.34 EUR x 3 USD per EUR = 1.02 USD",
        };
        assert!(text.contains(step), "{text}");
    }
}

/// A global of 17 / 3 is the double 5.666666666666667, and x 15,000,000
/// that is 85,000,000.00 (G-02); exact arithmetic keeps 5.666666667, so
/// 85,000,000.01.
#[test]
fn globals_are_doubles() {
    for (arith, want) in [
        (Arithmetic::Mmb, 8_500_000_000),
        (Arithmetic::Exact, 8_500_000_001),
    ] {
        let mut b = budget(
            arith,
            vec![category(
                "1100",
                "Z globals",
                vec![account(
                    "1101",
                    "Z globals",
                    vec![Detail::line(1, "Global division", "ZG_C", "", "15000000")],
                )],
            )],
        );
        b.globals.insert("ZG_B".into(), "17".into());
        b.globals.insert("ZG_C".into(), "ZG_B / 3".into());
        let (rows, r) = totals(&b);
        assert_eq!(rows[&1], want, "{arith:?}");
        assert_eq!(r.globals["ZG_C"], d("5.666666667"), "{arith:?}: shown");

        // Deleting the global freezes its value without changing a total.
        let mut frozen = b.clone();
        delete_global(&mut frozen, "ZG_C").unwrap();
        let after = recalc(&frozen, &RecalcOptions::default()).unwrap();
        assert_eq!(after.grand_total, r.grand_total, "{arith:?}");
    }
}

/// A rate with ten places: 100,000,000 x 0.0000000149 is 1.49 (V01-1).
/// Exact arithmetic keeps nine places, so it refuses the rate.
#[test]
fn rates_keep_more_than_nine_places() {
    let lines = vec![category(
        "4100",
        "Z precision",
        vec![account(
            "4101",
            "Z precision",
            vec![Detail::line(
                1,
                "Ten places",
                "100000000",
                "",
                "0.0000000149",
            )],
        )],
    )];
    let (rows, _) = totals(&budget(Arithmetic::Mmb, lines.clone()));
    assert_eq!(rows[&1], 149);
    let e = recalc(&budget(Arithmetic::Exact, lines), &RecalcOptions::default()).unwrap_err();
    assert!(e.to_string().contains("more than 9 decimal places"), "{e}");
}

/// Credit rows are added up before the one rounding, in both arithmetics:
/// two rows of 50% of 0.01 are 0.005 each, 0.01 together.
#[test]
fn credit_rows_round_once_together() {
    for arith in Arithmetic::ALL {
        let mut b = budget(
            arith,
            vec![category(
                "1100",
                "Z credits",
                vec![account(
                    "1101",
                    "Z credits",
                    vec![Detail::line(1, "Base", "1", "", "100")],
                )],
            )],
        );
        let row = CreditRow {
            description: String::new(),
            source: CreditSource::Flat(1),
            percent: d("50"),
            cap: CreditCap::None,
        };
        b.credits.push(Credit {
            name: "ZCR".into(),
            account: None,
            applied: true,
            cap: None,
            discount_percent: Dec::ZERO,
            rows: vec![row.clone(), row],
        });
        let (_, r) = totals(&b);
        assert_eq!(r.credits[0].amount, 1, "{arith:?}");
    }
}

/// An adjustment row's base is the rows above added up in doubles, as
/// Movie Magic adds them (CU-10: 10% of 132,347.67 is 13,234.77).
#[test]
fn an_adjustment_takes_its_percent_of_the_rows_above() {
    for arith in Arithmetic::ALL {
        let b = budget(
            arith,
            vec![category(
                "1100",
                "Z adjustments",
                vec![account(
                    "1101",
                    "Z adjustments",
                    vec![
                        Detail::line(1, "One", "1", "", "132347.12"),
                        Detail::line(2, "Two", "1", "", "0.55"),
                        Detail::adjustment(3, "Ten percent", "0.1"),
                    ],
                )],
            )],
        );
        let (rows, _) = totals(&b);
        assert_eq!(rows[&3], 1_323_477, "{arith:?}");
    }
}

/// Files that don't say are Movie Magic arithmetic; the setting is part of
/// a version, and the cache starts over when it changes.
#[test]
fn the_setting_defaults_to_movie_magic_and_switches_cleanly() {
    let mut b = ztest01(Arithmetic::Mmb);
    assert_eq!(Budget::new("New", "USD").arithmetic, Arithmetic::Mmb);
    assert_eq!(Arithmetic::default(), Arithmetic::Mmb);
    let opts = RecalcOptions::default();
    let mut cache = RecalcCache::new();
    let mmb = cache.recalc(&b, &opts).unwrap();
    b.arithmetic = Arithmetic::Exact;
    let exact = cache.recalc(&b, &opts).unwrap();
    assert_eq!(exact, recalc(&b, &opts).unwrap());
    assert_ne!(mmb.grand_total, exact.grand_total);
    assert_eq!(b.snapshot().arithmetic, Arithmetic::Exact);
    for a in Arithmetic::ALL {
        assert_eq!(Arithmetic::parse(a.name()), Some(a));
    }
    assert_eq!(Arithmetic::parse("float"), None);
    let ex = Explainer::new(&b, &opts).unwrap();
    let e = ex.explain(&Target::GrandTotal).unwrap();
    assert_eq!(e.arithmetic, Arithmetic::Exact);
}
