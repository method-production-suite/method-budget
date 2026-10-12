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

/// The percentage is a fraction first (entry 68, read from a real export's
/// account fringes): 3.05% of 5,250.00 is 5,250 x 0.0305 = 160.125 in
/// doubles, so 160.13, where 5,250 x 3.05 / 100 is 160.12499999999997;
/// exact arithmetic gives 160.13 too. 1.45000005% of 5,250.00 is 76.13.
#[test]
fn a_percent_fringe_takes_its_rate_as_a_fraction_first() {
    for arith in Arithmetic::ALL {
        let mut b = budget(
            arith,
            vec![category(
                "1100",
                "Z percent",
                vec![account(
                    "1101",
                    "Z crew",
                    vec![Detail {
                        fringes: set(&["ZWC", "ZMED"]),
                        ..Detail::line(1, "Prep", "25", "", "210")
                    }],
                )],
            )],
        );
        b.fringes.insert("ZWC".into(), percent("3.05", None));
        b.fringes.insert("ZMED".into(), percent("1.45000005", None));
        let (_, r) = totals(&b);
        let row = &r.categories[0].accounts[0].rows[0];
        assert_eq!(row.total, 525_000);
        assert_eq!(row.fringes["ZWC"], 16_013, "{arith:?}");
        assert_eq!(row.fringes["ZMED"], 7_613, "{arith:?}");
    }
}

/// A flat fringe on a line whose unit has no hours is charged once, but
/// not on a line whose Amount is 0 (entry 68: a stand-in's title line of
/// 0 with a 0.55 per Day fringe comes to nothing).
#[test]
fn a_flat_fringe_counts_nothing_on_an_amount_of_0() {
    for arith in Arithmetic::ALL {
        let mut b = budget(
            arith,
            vec![category(
                "1100",
                "Z flat",
                vec![account(
                    "1101",
                    "Z standins",
                    vec![
                        Detail {
                            fringes: set(&["ZPOST"]),
                            ..Detail::line(1, "Standin", "0", "", "0")
                        },
                        Detail {
                            fringes: set(&["ZPOST"]),
                            ..Detail::line(2, "Kit", "2", "Allow", "0")
                        },
                    ],
                )],
            )],
        );
        b.fringes.insert(
            "ZPOST".into(),
            Fringe {
                description: String::new(),
                short_code: None,
                rate: "0.55".into(),
                kind: FringeKind::flat(Some("Day".into())),
            },
        );
        let (_, r) = totals(&b);
        let rows = &r.categories[0].accounts[0].rows;
        assert_eq!(rows[0].fringes.get("ZPOST").copied().unwrap_or(0), 0);
        assert_eq!(rows[1].fringes["ZPOST"], 55, "{arith:?}");
        let text = explained(&b, Target::LineFringes(1));
        assert!(text.contains("the Amount is 0: nothing counted"), "{text}");
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
                format: Default::default(),
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
            excluded_fringes: set(&[]),
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

// ---------------------------------------------------------------------------
// Decisions.md, entry 56: ranges, charges, levels, posting, adjustment
// fringes and digit-led globals, as the parity run read them.

/// A percent fringe at `rate` with an optional cutoff in cents.
fn percent(rate: &str, cutoff: Option<Cents>) -> Fringe {
    Fringe {
        description: String::new(),
        short_code: None,
        rate: rate.into(),
        kind: FringeKind::Percent { cutoff },
    }
}

/// Line `id`: 1 x `rate`, carrying `fringes`, in range `range`.
fn fringed(id: u64, desc: &str, rate: &str, fringes: &[&str], range: Option<u32>) -> Detail {
    Detail {
        fringes: set(fringes),
        fringe_range: range,
        ..Detail::line(id, desc, "1", "", rate)
    }
}

/// Every line's fringes by row id.
fn line_fringes(
    r: &Report,
) -> std::collections::BTreeMap<u64, std::collections::BTreeMap<String, Cents>> {
    r.categories
        .iter()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.rows)
        .map(|row| (row.id, row.fringes.clone()))
        .collect()
}

/// ZTEST04's range accounts (parity plan 4.4; run log, ZTEST04 and its P1
/// range cases), with ZP10 at 10% (cutoff 1,000.00), ZP125 at 12.5%
/// (cutoff 2,222.22) and ZP321 at 3.21% (no cutoff).
fn ztest04_ranges(arith: Arithmetic) -> Budget {
    let mut prep = fringed(12, "Prep", "312.34", &["ZP125"], Some(1));
    prep.amount = "4".into();
    prep.unit = "Day".into();
    let mut shoot = fringed(13, "Shoot", "312.34", &["ZP125"], Some(1));
    shoot.amount = "17".into();
    shoot.unit = "Day".into();
    let mut wrap = fringed(14, "Wrap", "312.34", &["ZP125"], Some(1));
    wrap.amount = "2".into();
    wrap.unit = "Day".into();
    let mut middle = fringed(26, "Middle out", "1500", &["ZP10"], Some(1));
    middle.groups = set(&["ZOFF"]);
    let mut b = budget(
        arith,
        vec![
            category(
                "1100",
                "ZPERCENT A",
                vec![
                    account("1102", "Z range one earner", vec![prep, shoot, wrap]),
                    account(
                        "1104",
                        "Z range across subtotal",
                        vec![
                            fringed(18, "Before", "1000", &["ZP10"], Some(1)),
                            Detail {
                                fringe_range: Some(1),
                                ..Detail::subtotal(19, "ZSUB R")
                            },
                            fringed(20, "After", "1000", &["ZP10"], Some(1)),
                        ],
                    ),
                    account(
                        "1105",
                        "Z range mixed fringes",
                        vec![
                            fringed(21, "Both fringes", "1500", &["ZP125", "ZP321"], Some(1)),
                            fringed(22, "One fringe", "1500", &["ZP125"], Some(1)),
                        ],
                    ),
                    account(
                        "1106",
                        "Z range negative line",
                        vec![
                            fringed(23, "Pay", "3000", &["ZP10"], Some(1)),
                            fringed(24, "Refund", "-500", &["ZP10"], Some(1)),
                            // The same two lines before the range was made.
                            fringed(123, "Pay alone", "3000", &["ZP10"], None),
                            fringed(124, "Refund alone", "-500", &["ZP10"], None),
                        ],
                    ),
                ],
            ),
            level("ZTOTAL ATL"),
            category(
                "1200",
                "ZPERCENT B",
                vec![
                    account(
                        "1201",
                        "Z range suppressed middle",
                        vec![
                            fringed(25, "First", "1500", &["ZP10"], Some(1)),
                            middle,
                            fringed(27, "Last", "1500", &["ZP10"], Some(1)),
                        ],
                    ),
                    account(
                        "1202",
                        "Z two ranges",
                        vec![
                            fringed(28, "A one", "1500", &["ZP10"], Some(1)),
                            fringed(29, "A two", "1500", &["ZP10"], Some(1)),
                            fringed(30, "B one", "1500", &["ZP10"], Some(2)),
                            fringed(31, "B two", "1500", &["ZP10"], Some(2)),
                        ],
                    ),
                    account(
                        "1203",
                        "Z range partial fringe",
                        vec![
                            fringed(32, "Fringed", "600", &["ZP10"], Some(1)),
                            fringed(33, "Not fringed", "600", &[], Some(1)),
                            fringed(34, "Fringed again", "600", &["ZP10"], Some(1)),
                        ],
                    ),
                    account(
                        "1204",
                        "Z range rounding",
                        vec![
                            fringed(35, "Tiny one", "0.04", &["ZP125"], Some(1)),
                            fringed(36, "Tiny two", "0.04", &["ZP125"], Some(1)),
                            fringed(37, "Tiny three", "0.04", &["ZP125"], Some(1)),
                        ],
                    ),
                ],
            ),
            level("ZTOTAL BTL"),
        ],
    );
    b.groups.insert("ZOFF".into(), group(false));
    b.fringes
        .insert("ZP10".into(), percent("10", Some(100_000)));
    b.fringes
        .insert("ZP125".into(), percent("12.5", Some(222_222)));
    b.fringes.insert("ZP321".into(), percent("3.21", None));
    b
}

/// A range uses its cutoff up in line order, each line rounding on its own
/// (ZTEST04 1102, 1104, 1105, 1106, 1201 to 1204).
#[test]
fn ztest04_ranges_use_the_cutoff_in_line_order() {
    for arith in Arithmetic::ALL {
        let b = ztest04_ranges(arith);
        let (rows, r) = totals(&b);
        let f = line_fringes(&r);
        let got = |id: u64, name: &str| f[&id].get(name).copied();
        let cases: [(u64, &str, Option<Cents>); 23] = [
            // 1102: 12.5% of 1,249.36, then of the 972.86 left, then nothing.
            (12, "ZP125", Some(15_617)),
            (13, "ZP125", Some(12_161)),
            (14, "ZP125", Some(0)),
            // 1104: a subtotal row doesn't break the range.
            (18, "ZP10", Some(10_000)),
            (20, "ZP10", Some(0)),
            // 1105: ZP125 in line order; ZP321 has no cutoff.
            (21, "ZP125", Some(18_750)),
            (22, "ZP125", Some(9_028)),
            (21, "ZP321", Some(4_815)),
            // 1106: a credit after the cutoff is used earns nothing; on its
            // own it earns its percent.
            (23, "ZP10", Some(10_000)),
            (24, "ZP10", Some(0)),
            (123, "ZP10", Some(10_000)),
            (124, "ZP10", Some(-5_000)),
            // 1201: the suppressed middle line uses none of the cutoff.
            (25, "ZP10", Some(10_000)),
            (26, "ZP10", None),
            (27, "ZP10", Some(0)),
            // 1202: two ranges, each with its own cutoff.
            (28, "ZP10", Some(10_000)),
            (29, "ZP10", Some(0)),
            (30, "ZP10", Some(10_000)),
            (31, "ZP10", Some(0)),
            // 1203: a line without the fringe uses none of the cutoff.
            (32, "ZP10", Some(6_000)),
            (34, "ZP10", Some(4_000)),
            // 1204: 0.005 each, a cent each.
            (35, "ZP125", Some(1)),
            (37, "ZP125", Some(1)),
        ];
        for (id, name, want) in cases {
            assert_eq!(got(id, name), want, "{arith:?} P-{id} {name}");
        }
        assert_eq!(got(33, "ZP10"), None);
        assert_eq!(got(36, "ZP125"), Some(1));
        assert_eq!(
            (rows[&12], rows[&13], rows[&14]),
            (124_936, 530_978, 62_468)
        );
        let accounts: Vec<(&str, Cents)> = r
            .categories
            .iter()
            .flat_map(|c| &c.accounts)
            .map(|a| (a.code.as_str(), a.fringe_total))
            .collect();
        assert_eq!(
            &accounts[..3],
            [("1102", 27_778), ("1104", 10_000), ("1105", 32_593)],
            "{arith:?}: range totals as Movie Magic shows them"
        );
        // Fringe account 1299 = 400.03.
        assert_eq!(
            r.categories[1].fringe_account.as_ref().map(|f| f.1),
            Some(40_003),
            "{arith:?}"
        );
    }
}

/// ZTEST06 FRINGE POSTING (parity plan 4.6), posted by `posting`.
fn ztest06(posting: FringePosting) -> Budget {
    let mut assistant = fringed(6, "Assistant", "100", &["ZFDAY"], None);
    assistant.amount = "4".into();
    assistant.unit = "Day".into();
    let mut b = budget(
        Arithmetic::Mmb,
        vec![
            category(
                "1100",
                "ZATL",
                vec![
                    account(
                        "1101",
                        "Z atl one",
                        vec![fringed(1, "Star", "10000", &["ZP10N"], None)],
                    ),
                    account(
                        "1102",
                        "Z atl two",
                        vec![fringed(2, "Writer", "5000", &["ZP10N"], None)],
                    ),
                ],
            ),
            level("ZTOTAL ATL"),
            category(
                "2100",
                "ZPROD",
                vec![
                    account(
                        "2101",
                        "Z prod one",
                        vec![fringed(3, "Grip", "8000", &["ZP10N"], None)],
                    ),
                    account(
                        "2102",
                        "Z prod two",
                        vec![fringed(4, "Gear", "2000", &[], None)],
                    ),
                ],
            ),
            level("ZTOTAL PROD"),
            category(
                "3100",
                "ZPOST",
                vec![account(
                    "3101",
                    "Z post one",
                    vec![fringed(5, "Editor", "3000", &["ZP10N"], None), assistant],
                )],
            ),
            level("ZTOTAL POST"),
            category(
                "4100",
                "ZOTHER",
                vec![account(
                    "4101",
                    "Z other one",
                    vec![fringed(7, "Insurance", "1000", &["ZP10N"], None)],
                )],
            ),
        ],
    );
    b.fringe_posting = posting;
    b.fringes.insert("ZP10N".into(), percent("10", None));
    b.fringes.insert(
        "ZFDAY".into(),
        Fringe {
            description: String::new(),
            short_code: None,
            rate: "100".into(),
            kind: FringeKind::flat(Some("Day".into())),
        },
    );
    b.charges.push(percent_charge("ZPCT5", "5"));
    b
}

/// ATL is the first level; BTL every later level a production total
/// closes; 4100, after the last one, is in neither. A percent charge's base
/// is every category with its fringes, budget-posted fringes included
/// (ZTEST06 main, V06-1, V06-2, V06-3).
#[test]
fn ztest06_levels_and_the_charge_base_in_every_posting_mode() {
    let cat99 = FringePosting::Category {
        suffix: "99".into(),
    };
    for (posting, atl, btl, budget_fringes) in [
        (cat99, 1_650_000, 1_490_000, 0),
        (FringePosting::Account, 1_650_000, 1_490_000, 0),
        (FringePosting::ProductionLevel, 1_650_000, 1_490_000, 0),
        (FringePosting::Budget, 1_500_000, 1_340_000, 310_000),
    ] {
        let b = ztest06(posting.clone());
        let (_, r) = totals(&b);
        assert_eq!(
            (r.total_atl, r.total_btl, r.budget_fringes),
            (atl, btl, budget_fringes),
            "{posting:?}"
        );
        assert_eq!(r.charges[0].base, 3_250_000, "{posting:?}");
        assert_eq!(r.charges[0].amount, 162_500, "{posting:?}: ZPCT5 1,625.00");
        assert_eq!(r.grand_total, 3_412_500, "{posting:?}: grand 34,125.00");
        assert!(warnings(&b, &r).is_empty() || posting == FringePosting::ProductionLevel);
    }
}

/// Posting by production level, 4100's fringes have no production total to
/// post above. Kept by default, with a warning; dropped as Movie Magic
/// drops them when the budget says so (V06-2: ZPCT5 1,620.00, grand
/// 34,020.00). A closing production total brings them back.
#[test]
fn ztest06_trailing_level_fringes_are_kept_or_dropped() {
    let mut b = ztest06(FringePosting::ProductionLevel);
    let (_, kept) = totals(&b);
    assert_eq!(kept.levels[3].posted_fringes, 10_000);
    assert_eq!(kept.grand_total, 3_412_500);
    let w = warnings(&b, &kept);
    assert_eq!(w.len(), 1);
    assert!(
        w[0].starts_with(
            "fringes of categories after the last production total are not in any level (4100 100.00)"
        ),
        "{w:?}"
    );

    b.trailing_level_fringes = TrailingFringes::Drop;
    let (_, dropped) = totals(&b);
    assert_eq!(dropped.levels[3].posted_fringes, 0);
    assert_eq!(dropped.levels[3].total, 100_000);
    assert_eq!(dropped.charges[0].amount, 162_000, "ZPCT5 1,620.00");
    assert_eq!(dropped.grand_total, 3_402_000, "grand 34,020.00");
    assert_eq!(
        (dropped.total_atl, dropped.total_btl),
        (1_650_000, 1_490_000)
    );
    assert!(warnings(&b, &dropped)[0].contains("left out of every total"));

    // A production total after 4100 gives them a level: 34,125.00, BTL
    // 16,000.00, and nothing to warn about.
    b.topsheet.push(level("ZTOTAL OTHER"));
    let (_, closed) = totals(&b);
    assert_eq!(closed.grand_total, 3_412_500);
    assert_eq!(closed.total_btl, 1_600_000);
    assert!(warnings(&b, &closed).is_empty());
}

/// ZTEST11 LEVELS: four categories of 1,000.00 to 4,000.00, no fringes.
fn ztest11(rows: Vec<TopsheetRow>) -> Report {
    let b = budget(Arithmetic::Mmb, rows);
    totals(&b).1
}

fn z11_category(n: u64) -> TopsheetRow {
    let code = format!("{n}100");
    category(
        &code,
        "ZLEVEL",
        vec![account(
            &format!("{n}101"),
            "Z",
            vec![Detail::line(
                n,
                "Line",
                "1",
                "",
                (n * 1000).to_string().as_str(),
            )],
        )],
    )
}

/// With no production total nothing is above or below the line; an empty
/// first level is ATL 0, not skipped; categories after the last production
/// total count in neither (ZTEST11 T-01, T-02, T-04, V11-3b).
#[test]
fn ztest11_atl_and_btl_follow_the_production_totals() {
    let cats = || (1..=4).map(z11_category).collect::<Vec<_>>();
    let mut none = cats();
    let r = ztest11(none.clone());
    assert_eq!((r.total_atl, r.total_btl, r.atl_btl), (0, 0, 0), "T-01");
    assert_eq!(r.grand_total, 1_000_000);

    none.insert(1, level("ZTOTAL ATL"));
    let r = ztest11(none.clone());
    assert_eq!((r.total_atl, r.total_btl), (100_000, 0), "T-02");
    assert_eq!(r.grand_total, 1_000_000);

    none.insert(0, level("ZTOP"));
    let r = ztest11(none.clone());
    assert_eq!((r.total_atl, r.total_btl), (0, 100_000), "T-04");

    none.push(level("ZCLOSE"));
    let r = ztest11(none);
    assert_eq!((r.total_atl, r.total_btl), (0, 1_000_000), "V11-3b");
    assert_eq!(r.grand_total, 1_000_000);
}

/// ZTEST09 CONTRACTUAL CHARGES (parity plan 4.9, run log), posted by
/// `posting`, with group ZNOCHG on or off.
fn ztest09(posting: FringePosting, znochg_on: bool) -> Budget {
    let grouped = |id, desc: &str, rate: &str, groups: &[&str]| Detail {
        groups: set(groups),
        ..Detail::line(id, desc, "1", "", rate)
    };
    let mut b = budget(
        Arithmetic::Mmb,
        vec![
            category(
                "1100",
                "ZATL",
                vec![
                    account(
                        "1101",
                        "Z star",
                        vec![fringed(1, "Star", "10000", &["ZP10N"], None)],
                    ),
                    account(
                        "1102",
                        "Z rights",
                        vec![grouped(2, "Rights", "500", &["ZNOCHG"])],
                    ),
                ],
            ),
            level("ZTOTAL ATL"),
            category(
                "2100",
                "ZBTL A",
                vec![account(
                    "2101",
                    "Z crew",
                    vec![
                        fringed(3, "Grip", "8000", &["ZP10N"], None),
                        grouped(4, "Kit", "300", &["ZNOCHG", "ZX2"]),
                    ],
                )],
            ),
            category(
                "3100",
                "ZBTL B",
                vec![account(
                    "3101",
                    "Z gear",
                    vec![grouped(5, "Gear", "2000", &["ZX2"])],
                )],
            ),
            level("ZTOTAL BTL"),
        ],
    );
    b.fringe_posting = posting;
    b.fringes.insert("ZP10N".into(), percent("10", None));
    b.groups.insert("ZNOCHG".into(), group(znochg_on));
    b.groups.insert("ZX2".into(), group(true));
    b.currencies.insert(
        "EUR".into(),
        Currency {
            description: String::new(),
            symbol: String::new(),
            rate: d("3"),
            format: Default::default(),
        },
    );
    let flat = |name: &str, cents: Cents, currency: Option<&str>| Charge {
        kind: ChargeKind::Flat {
            amount: cents,
            currency: currency.map(String::from),
        },
        ..percent_charge(name, "0")
    };
    let mut zflat = flat("ZFLAT", 250_000, Some("USD"));
    zflat.excluded_amounts = vec![("ZEXCL B".into(), 22_200)];
    zflat.excluded_groups = set(&["ZNOCHG"]);
    let mut zpctx = percent_charge("ZPCTX", "10");
    zpctx.excluded_amounts = vec![("ZEXCL A".into(), 11_100)];
    zpctx.excluded_groups = set(&["ZNOCHG"]);
    let mut zpctcat = percent_charge("ZPCTCAT", "2");
    zpctcat.excluded_categories = set(&["2100"]);
    let mut zpctacc = percent_charge("ZPCTACC", "3");
    zpctacc.excluded_accounts = set(&["1102"]);
    let mut zover = percent_charge("ZOVER", "10");
    zover.excluded_groups = set(&["ZNOCHG", "ZX2"]);
    let mut zflatcat = flat("ZFLATCAT", 1_000_000, None);
    zflatcat.excluded_categories = set(&["3100"]);
    b.charges = vec![
        zflat,
        percent_charge("ZPCT5", "5"),
        zpctx,
        zpctcat,
        zpctacc,
        zover,
        flat("ZFLATEUR", 100_000, Some("EUR")),
        zflatcat,
        percent_charge("ZHALF", "0.4425"),
    ];
    b
}

/// Every charge as Movie Magic gave it: a flat fee honors category
/// exclusions (ZFLATCAT 8,000.00) as well as typed amounts and groups
/// (ZFLAT 2,500.00 - 222.00 - 800.00 = 1,478.00), and posting fringes by
/// budget changes no charge (V09-1).
#[test]
fn ztest09_charges_match_movie_magic() {
    let cat99 = FringePosting::Category {
        suffix: "99".into(),
    };
    let main = [
        ("ZFLAT", 147_800),
        ("ZPCT5", 113_000),
        ("ZPCTX", 216_890),
        ("ZPCTCAT", 27_000),
        ("ZPCTACC", 66_300),
        ("ZOVER", 198_000),
        ("ZFLATEUR", 300_000),
        ("ZFLATCAT", 800_000),
        ("ZHALF", 10_001),
    ];
    for posting in [cat99.clone(), FringePosting::Budget] {
        let (_, r) = totals(&ztest09(posting.clone(), true));
        let got: Vec<(&str, Cents)> = r
            .charges
            .iter()
            .map(|c| (c.name.as_str(), c.amount))
            .collect();
        assert_eq!(got, main, "{posting:?}");
        assert_eq!(r.grand_total, 4_138_991, "{posting:?}: grand 41,389.91");
    }
    // V09-3: ZNOCHG off.
    let (_, r) = totals(&ztest09(cat99, false));
    let got: Vec<(&str, Cents)> = r
        .charges
        .iter()
        .map(|c| (c.name.as_str(), c.amount))
        .collect();
    assert_eq!(
        got,
        [
            ("ZFLAT", 227_800),
            ("ZPCT5", 109_000),
            ("ZPCTX", 216_890),
            ("ZPCTCAT", 26_000),
            ("ZPCTACC", 65_400),
            ("ZOVER", 198_000),
            ("ZFLATEUR", 300_000),
            ("ZFLATCAT", 800_000),
            ("ZHALF", 9_647),
        ]
    );
    assert_eq!(r.grand_total, 4_132_737, "grand 41,327.37");
}

/// An adjustment row can carry a fringe: ZP10N on the 350.00 adjustment is
/// 35.00, so account 1101's fringes are 335.00 (ZTEST03 A-05, the
/// adjustment windows as Movie Magic showed them).
#[test]
fn ztest03_an_adjustment_row_earns_its_fringe() {
    for arith in Arithmetic::ALL {
        let mut a05 = Detail::adjustment(5, "ZADJ 10 percent", "0.1");
        a05.fringes = set(&["ZP10N"]);
        let mut a15 = Detail::adjustment(15, "ZADJ X", "0.1");
        a15.x = "2".into();
        let mut b = budget(
            arith,
            vec![category(
                "1100",
                "ZADJUST A",
                vec![account(
                    "1101",
                    "Z adjust windows",
                    vec![
                        fringed(1, "Line one", "1000", &["ZP10N"], None),
                        fringed(2, "Line two", "2000", &["ZP10N"], None),
                        Detail::subtotal(3, "ZSUB 1"),
                        fringed(4, "Line three", "500", &[], None),
                        a05,
                        fringed(6, "Line four", "400", &[], None),
                        Detail::adjustment(7, "ZADJ 5 percent", "0.05"),
                        fringed(71, "Line four b", "250", &[], None),
                        Detail::adjustment(9, "ZADJ restart", "0"),
                        fringed(10, "Line five", "1234.5", &[], None),
                        Detail::adjustment(11, "ZADJ half cent", "0.01"),
                        fringed(12, "Line six", "100", &[], None),
                        Detail::adjustment(13, "ZADJ discount", "-0.1"),
                        fringed(14, "Line seven", "300", &[], None),
                        a15,
                    ],
                )],
            )],
        );
        b.fringes.insert("ZP10N".into(), percent("10", None));
        let (rows, r) = totals(&b);
        for (id, want) in [
            (5, 35_000),
            (7, 2_000),
            (9, 0),
            (11, 1_235),
            (13, -1_000),
            (15, 6_000),
        ] {
            assert_eq!(rows[&id], want, "{arith:?} A-{id:02}");
        }
        let f = line_fringes(&r);
        assert_eq!(
            f[&5]["ZP10N"], 3_500,
            "{arith:?}: 10% of the 350.00 adjustment"
        );
        assert_eq!(
            r.categories[0].accounts[0].fringe_total, 33_500,
            "{arith:?}"
        );
        assert_eq!(r.fringes["ZP10N"].uses, 3);
    }
}

/// A global may be named with a leading digit: `1ZG` = 7, and `1ZG*1` in
/// Amt is 7.00 (V02-3).
#[test]
fn a_global_name_may_start_with_a_digit() {
    for arith in Arithmetic::ALL {
        let mut b = budget(
            arith,
            vec![category(
                "1100",
                "Z globals",
                vec![account(
                    "1101",
                    "Z globals",
                    vec![
                        Detail::line(1, "Digit global", "1ZG*1", "", "1"),
                        Detail::line(2, "Bare digit global", "1ZG", "", "1"),
                        Detail::line(3, "Plain number", "1.5", "", "2"),
                    ],
                )],
            )],
        );
        b.globals.insert("1ZG".into(), "7".into());
        let (rows, r) = totals(&b);
        assert_eq!((rows[&1], rows[&2], rows[&3]), (700, 700, 300), "{arith:?}");
        assert_eq!(r.globals["1ZG"], d("7"));
        assert!(method_core::rename::valid_name(SetupKind::Global, "1ZG"));
        assert!(!method_core::rename::valid_name(SetupKind::Global, "12"));
        // Renaming it rewrites the formula, and a name inside another
        // digit-led name is left alone.
        let mut renamed = b.clone();
        rename(&mut renamed, SetupKind::Global, "1ZG", "SEVEN").unwrap();
        let acct = &renamed.categories().next().unwrap().accounts[0];
        assert_eq!(acct.details[0].amount.0, "SEVEN*1");
        assert_eq!(
            recalc(&renamed, &RecalcOptions::default())
                .unwrap()
                .grand_total,
            r.grand_total
        );
    }
}

// ---------------------------------------------------------------------------
// Decisions.md, entry 57: credit rows on a topsheet category, and the group
// rules (lines with no group, conflicts, fringes in group totals).

/// Every step's label and text in `s`, one per line.
fn words(s: &method_core::explain::Step) -> String {
    let mut out = format!("{}: {}\n", s.label, s.text);
    for c in &s.children {
        out.push_str(&words(c));
    }
    out
}

/// What Explain says about `t` in `b`.
fn explained(b: &Budget, t: Target) -> String {
    let ex = Explainer::new(b, &RecalcOptions::default()).unwrap();
    words(&ex.explain(&t).unwrap().root)
}

/// A credit of `rows`, applied, with no cap, discount or excluded fringes.
fn credit(name: &str, rows: Vec<CreditRow>) -> Credit {
    credit_less(name, &[], rows)
}

/// A credit of `rows` that leaves `excluded` fringes out of every group
/// and category row (one list per credit, decisions.md, entry 57).
fn credit_less(name: &str, excluded: &[&str], rows: Vec<CreditRow>) -> Credit {
    Credit {
        name: name.into(),
        account: None,
        applied: true,
        cap: None,
        discount_percent: Dec::ZERO,
        excluded_fringes: set(excluded),
        rows,
    }
}

fn credit_row(source: CreditSource, percent: &str, cap: CreditCap) -> CreditRow {
    CreditRow {
        description: String::new(),
        source,
        percent: d(percent),
        cap,
    }
}

/// Credit row on topsheet category `code`.
fn on_category(code: &str) -> CreditSource {
    CreditSource::Category {
        category: code.into(),
    }
}

/// ZTEST10 CREDITS' lines (parity plan 4.10, run log): 1100 4,000.00 with
/// 500.00 of fringes (a suppressed line earns none), 2100 4,000.00 with
/// 300.00, grand 8,800.00 whatever the posting.
fn ztest10(arith: Arithmetic, posting: FringePosting) -> Budget {
    let mut grip = Detail::line(399, "Grip days", "4", "Day", "1000");
    grip.fringes = set(&["ZFDAY", "ZP10N"]);
    grip.groups = set(&["ZCRED"]);
    let mut off = Detail::line(400, "Suppressed grip", "1", "", "2000");
    off.fringes = set(&["ZP10N"]);
    off.groups = set(&["ZCRED", "ZOFF"]);
    let mut electric = Detail::line(401, "Electric", "1", "", "3000");
    electric.fringes = set(&["ZP10N"]);
    electric.groups = set(&["ZCRED2"]);
    let plain = Detail::line(402, "Plain", "1", "", "1000");
    let mut b = budget(
        arith,
        vec![
            category(
                "1100",
                "ZATL",
                vec![account("1101", "Z crew credit", vec![grip, off])],
            ),
            level("ZTOTAL ATL"),
            category(
                "2100",
                "ZBTL",
                vec![account("2101", "Z crew two", vec![electric, plain])],
            ),
            level("ZTOTAL BTL"),
        ],
    );
    b.fringe_posting = posting;
    b.fringes.insert("ZP10N".into(), percent("10", None));
    b.fringes.insert(
        "ZFDAY".into(),
        Fringe {
            description: String::new(),
            short_code: None,
            rate: "25".into(),
            kind: FringeKind::flat(Some("Day".into())),
        },
    );
    b.groups.insert("ZCRED".into(), group(true));
    b.groups.insert("ZCRED2".into(), group(true));
    b.groups.insert("ZOFF".into(), group(false));
    b
}

/// Every way fringes can post.
fn postings() -> [FringePosting; 4] {
    [
        FringePosting::Budget,
        FringePosting::ProductionLevel,
        FringePosting::Account,
        FringePosting::default(),
    ]
}

/// A category row's spend is the category with its own fringes, even when
/// fringes post by budget: 2100 is 4,000.00 + 300.00 = 4,300.00, and 10% of
/// it is 430.00 (V10-3).
#[test]
fn v10_3_a_category_credit_row_counts_the_category_with_its_fringes() {
    for arith in Arithmetic::ALL {
        for posting in postings() {
            let mut b = ztest10(arith, posting.clone());
            b.credits.push(credit(
                "ZCAT",
                vec![credit_row(on_category("2100"), "10", CreditCap::None)],
            ));
            let (_, r) = totals(&b);
            assert_eq!(r.grand_total, 880_000, "{arith:?} {posting:?}");
            let row = &r.credits[0].rows[0];
            assert_eq!(
                (row.gross, row.excluded_fringes, row.net),
                (430_000, 0, 430_000),
                "{arith:?} {posting:?}"
            );
            assert_eq!(r.credits[0].amount, 43_000, "{arith:?} {posting:?}");
            assert_eq!(r.net_total, 880_000 - 43_000);
            let text = explained(&b, Target::Credit(0));
            assert!(
                text.contains("category 2100 ZBTL with its fringes"),
                "{text}"
            );
        }
    }
}

/// Ticking ZCR3 on category 2100 adds a row at 0%: the credit's gross goes
/// from 1,234.57 to 5,534.57 and the credit stays 411.52 (V10-3). At 10%
/// the row adds 430.00, and the rows still round once together.
#[test]
fn v10_3_a_category_row_joins_a_credit_at_zero_percent() {
    for arith in Arithmetic::ALL {
        let mut b = ztest10(arith, FringePosting::Budget);
        b.credits.push(credit(
            "ZCR3",
            vec![
                credit_row(CreditSource::Flat(123_457), "33.333", CreditCap::None),
                credit_row(on_category("2100"), "0", CreditCap::None),
            ],
        ));
        let (_, r) = totals(&b);
        let c = &r.credits[0];
        let gross: Cents = c.rows.iter().map(|row| row.gross).sum();
        assert_eq!(gross, 553_457, "{arith:?}");
        assert_eq!(c.amount, 41_152, "{arith:?}");

        b.credits[0].rows[1].percent = d("10");
        let (_, r) = totals(&b);
        // 411.51849... + 430.00, rounded once.
        assert_eq!(r.credits[0].amount, 84_152, "{arith:?}");
    }
}

/// A category row leaves out the credit's excluded fringes like a group
/// row (decisions.md, entry 57), and takes either cap. 1100 with ZFDAY
/// left out is gross 4,500.00, excluded 100.00, net 4,400.00.
#[test]
fn a_category_credit_row_leaves_out_fringes_and_takes_caps() {
    for arith in Arithmetic::ALL {
        let mut b = ztest10(arith, FringePosting::Budget);
        b.credits.push(credit_less(
            "ZCAT",
            &["ZP10N"],
            vec![credit_row(on_category("2100"), "10", CreditCap::None)],
        ));
        b.credits.push(credit(
            "ZCAT2",
            vec![
                credit_row(
                    on_category("2100"),
                    "10",
                    CreditCap::QualifiedSpend(200_000),
                ),
                credit_row(on_category("1100"), "10", CreditCap::Credit(5_000)),
            ],
        ));
        b.credits.push(credit_less(
            "ZCAT3",
            &["ZFDAY"],
            vec![
                credit_row(on_category("1100"), "10", CreditCap::None),
                // A typed amount has no fringes to leave out.
                credit_row(CreditSource::Flat(100_000), "10", CreditCap::None),
            ],
        ));
        let (_, r) = totals(&b);
        let rows = &r.credits[0].rows;
        assert_eq!(
            (rows[0].gross, rows[0].excluded_fringes, rows[0].net),
            (430_000, 30_000, 400_000)
        );
        assert_eq!(rows[0].credit, d("400"));
        let rows = &r.credits[1].rows;
        assert_eq!(rows[0].excluded_fringes, 0);
        assert_eq!(rows[0].credit, d("200"));
        assert_eq!(rows[0].cap_variance, Some(d("-2300")));
        // 1100 is 4,000.00 + 500.00 (the suppressed line earns nothing).
        assert_eq!(rows[1].gross, 450_000);
        assert_eq!(rows[1].credit, d("50"));
        let rows = &r.credits[2].rows;
        assert_eq!(
            (rows[0].gross, rows[0].excluded_fringes, rows[0].net),
            (450_000, 10_000, 440_000)
        );
        assert_eq!(
            (rows[1].gross, rows[1].excluded_fringes, rows[1].net),
            (100_000, 0, 100_000)
        );
        let amounts: Vec<Cents> = r.credits.iter().map(|c| c.amount).collect();
        assert_eq!(amounts, vec![40_000, 25_000, 54_000]);
        let text = explained(&b, Target::Credit(0));
        assert!(text.contains("Excluded fringes: ZP10N"), "{text}");
    }
}

/// ZTEST10's credits ZCR1 to ZCR6 (parity plan 4.10, run log), in the
/// order they were made.
fn ztest10_credits() -> Vec<Credit> {
    let flat = |amount: Cents, percent: &str, cap: CreditCap| {
        credit_row(CreditSource::Flat(amount), percent, cap)
    };
    let zcred = CreditSource::Group {
        group: "ZCRED".into(),
    };
    vec![
        Credit {
            cap: Some(124_000),
            discount_percent: d("2"),
            ..credit_less(
                "ZCR1",
                &["ZFDAY"],
                vec![
                    credit_row(zcred, "20", CreditCap::None),
                    flat(100_000, "10", CreditCap::Credit(5_000)),
                    flat(300_000, "15", CreditCap::QualifiedSpend(200_000)),
                ],
            )
        },
        credit(
            "ZCR2",
            vec![credit_row(
                CreditSource::Group {
                    group: "ZCRED2".into(),
                },
                "25",
                CreditCap::None,
            )],
        ),
        credit("ZCR3", vec![flat(123_457, "33.333", CreditCap::None)]),
        credit(
            "ZCR4",
            vec![
                flat(5, "10", CreditCap::None),
                flat(5, "10", CreditCap::None),
            ],
        ),
        Credit {
            discount_percent: d("2.5"),
            ..credit("ZCR5", vec![flat(100_100, "10", CreditCap::None)])
        },
        Credit {
            cap: Some(90_000),
            discount_percent: d("10"),
            ..credit("ZCR6", vec![flat(1_000_000, "10", CreditCap::None)])
        },
    ]
}

/// ZTEST10 V10-1 (parity plan 4.10, run log): ZCR1 is 1,205.40 and the net
/// total 5,450.47, with ZFDAY left out of ZCR1 as a credit-wide list.
/// Switching ZCR1's group row to category 1100 keeps the credit's list: the
/// row is gross 4,500.00, excluded 100.00, net 4,400.00 either way.
#[test]
fn v10_1_credits_and_the_credit_wide_excluded_fringes() {
    for arith in Arithmetic::ALL {
        let mut b = ztest10(arith, FringePosting::Budget);
        b.credits = ztest10_credits();
        let (_, r) = totals(&b);
        let row = &r.credits[0].rows[0];
        assert_eq!(
            (row.gross, row.excluded_fringes, row.net),
            (450_000, 10_000, 440_000),
            "{arith:?}"
        );
        assert_eq!(r.credits[0].amount, 120_540, "{arith:?}");
        assert_eq!(r.credits[1].amount, 82_500, "{arith:?}");
        assert_eq!(r.grand_total, 880_000, "{arith:?}");
        assert_eq!(r.net_total, 545_047, "{arith:?}");

        b.credits[0].rows[0].source = on_category("1100");
        assert_eq!(b.credits[0].excluded_fringes, set(&["ZFDAY"]));
        let (_, r) = totals(&b);
        let row = &r.credits[0].rows[0];
        assert_eq!(
            (row.gross, row.excluded_fringes, row.net),
            (450_000, 10_000, 440_000),
            "{arith:?}"
        );
        assert_eq!(r.credits[0].amount, 120_540, "{arith:?}");
    }
}

/// A row on a category the budget doesn't have counts 0, and `warnings`
/// says so; renumbering carries the row's category along, and renaming a
/// fringe renames it in the credit's exclusions.
#[test]
fn a_category_credit_row_follows_the_chart() {
    let mut b = ztest10(Arithmetic::Mmb, FringePosting::Budget);
    b.credits.push(credit_less(
        "ZCAT",
        &["ZP10N"],
        vec![
            credit_row(on_category("9900"), "10", CreditCap::None),
            credit_row(on_category("2100"), "10", CreditCap::None),
        ],
    ));
    let (_, r) = totals(&b);
    assert_eq!(r.credits[0].rows[0].gross, 0);
    let w = warnings(&b, &r);
    assert!(
        w.iter()
            .any(|w| w.contains("credit ZCAT row 1") && w.contains("9900")),
        "{w:?}"
    );
    assert!(explained(&b, Target::Credit(0)).contains("category 9900 is not in the budget"));

    rename(&mut b, SetupKind::Fringe, "ZP10N", "ZPCT").unwrap();
    assert_eq!(b.credits[0].rows[1].source, on_category("2100"));
    assert_eq!(b.credits[0].excluded_fringes, set(&["ZPCT"]));
    let usage = method_core::usage::usage(&b, SetupKind::Fringe);
    assert!(usage["ZPCT"].other >= 1);

    let to = method_core::coding::AccountCoding {
        separator: Some('-'),
        ..b.account_coding
    };
    method_core::renumber::renumber(&mut b, to).unwrap();
    let cat = b.categories().nth(1).unwrap().code.clone();
    assert_ne!(cat, "2100");
    assert_eq!(b.credits[0].rows[1].source, on_category(&cat));
    let r2 = recalc(&b, &RecalcOptions::default()).unwrap();
    assert_eq!(r2.credits[0].amount, r.credits[0].amount);
}

/// ZTEST07 GROUPS (parity plan 4.7, run log): ZIN and ZIN2 count, ZOFF
/// doesn't; fringes post by budget.
fn ztest07(arith: Arithmetic) -> Budget {
    let tagged = |id: u64, desc: &str, rate: &str, groups: &[&str], fringes: &[&str]| Detail {
        groups: set(groups),
        fringes: set(fringes),
        ..Detail::line(id, desc, "1", "", rate)
    };
    let mut b = budget(
        arith,
        vec![
            category(
                "1100",
                "ZGROUPS A",
                vec![account(
                    "1101",
                    "Z groups",
                    vec![
                        tagged(306, "In", "1000", &["ZIN"], &["ZP10N"]),
                        tagged(307, "Out", "2000", &["ZOFF"], &["ZP10N"]),
                        tagged(308, "In and out", "3000", &["ZIN", "ZOFF"], &[]),
                        tagged(309, "In twice", "4000", &["ZIN", "ZIN2"], &["ZP10N"]),
                        tagged(310, "No group", "500", &[], &[]),
                        Detail::subtotal(311, "ZSUB G"),
                        Detail::adjustment(312, "ZADJ G", "0.1"),
                    ],
                )],
            ),
            level("ZTOTAL ATL"),
            category(
                "1200",
                "ZGROUPS B",
                vec![account(
                    "1201",
                    "Z groups B",
                    vec![tagged(313, "In two", "600", &["ZIN2"], &[])],
                )],
            ),
            level("ZTOTAL BTL"),
        ],
    );
    b.fringe_posting = FringePosting::Budget;
    b.fringes.insert("ZP10N".into(), percent("10", None));
    b.groups.insert("ZIN".into(), group(true));
    b.groups.insert("ZIN2".into(), group(true));
    b.groups.insert("ZOFF".into(), group(false));
    b
}

/// The row with id `id`.
fn row(r: &Report, id: u64) -> &RowTotals {
    r.categories
        .iter()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.rows)
        .find(|row| row.id == id)
        .unwrap()
}

/// Movie Magic's defaults: lines with no group count, a line in a counted
/// and an uncounted group doesn't. Subtotal 5,500.00, adjustment 550.00,
/// grand 7,150.00 (ZTEST07 main).
#[test]
fn ztest07_default_group_rules_match_movie_magic() {
    for arith in Arithmetic::ALL {
        let b = ztest07(arith);
        assert_eq!(b.group_rules, GroupRules::default());
        let (rows, r) = totals(&b);
        assert!(!row(&r, 307).included && !row(&r, 308).included);
        assert_eq!(row(&r, 311).subtotal, Some(550_000), "{arith:?}");
        assert_eq!(rows[&312], 55_000, "{arith:?}");
        assert_eq!(r.budget_fringes, 50_000);
        assert_eq!(r.grand_total, 715_000, "{arith:?}");
        let zin = &r.groups["ZIN"].active;
        assert_eq!(zin.details_total + zin.fringe_total, 550_000);
        let text = explained(&b, Target::Line(308));
        assert!(text.contains("leaves out such conflicts"), "{text}");
    }
}

/// Lines with no group excluded: No group and the adjustment row (it has
/// no group) are left out, the subtotal shows 5,000.00 and the grand total
/// is 6,100.00 (V07-3).
#[test]
fn ztest07_lines_with_no_group_excluded() {
    for arith in Arithmetic::ALL {
        let mut b = ztest07(arith);
        b.group_rules.ungrouped_lines = Inclusion::Exclude;
        let (rows, r) = totals(&b);
        assert!(!row(&r, 310).included, "No group");
        assert!(!row(&r, 312).included, "the adjustment row");
        assert!(row(&r, 306).included && row(&r, 313).included);
        assert_eq!(rows[&310], 0);
        assert_eq!(rows[&312], 0);
        assert_eq!(row(&r, 311).subtotal, Some(500_000), "{arith:?}");
        assert_eq!(r.grand_total, 610_000, "{arith:?}");
        assert!(explained(&b, Target::Line(310)).contains("the line has no group"));
        assert!(explained(&b, Target::Line(312)).contains("the adjustment row has no group"));
    }
}

/// Conflicts included: In and out counts, ZOFF's active total is
/// 3,000.00 and the grand total is 10,450.00 (V07-4).
#[test]
fn ztest07_conflicts_included() {
    for arith in Arithmetic::ALL {
        let mut b = ztest07(arith);
        b.group_rules.conflict = Inclusion::Include;
        let (rows, r) = totals(&b);
        assert!(row(&r, 308).included);
        assert!(!row(&r, 307).included, "only uncounted groups: still out");
        assert_eq!(row(&r, 311).subtotal, Some(850_000), "{arith:?}");
        assert_eq!(rows[&312], 85_000, "{arith:?}");
        assert_eq!(r.grand_total, 1_045_000, "{arith:?}");
        assert_eq!(r.groups["ZOFF"].active.details_total, 300_000);
    }
}

/// "Fringe amounts in group totals" is display only: no total changes.
/// The rules round through a version, and the cache starts over when one
/// changes.
#[test]
fn group_rules_switch_cleanly() {
    let mut b = ztest07(Arithmetic::Mmb);
    let opts = RecalcOptions::default();
    let mut cache = RecalcCache::new();
    let base = cache.recalc(&b, &opts).unwrap();
    b.group_rules.fringes_in_totals = Inclusion::Exclude;
    assert_eq!(cache.recalc(&b, &opts).unwrap(), base);
    b.group_rules.ungrouped_lines = Inclusion::Exclude;
    let ex = cache.recalc(&b, &opts).unwrap();
    assert_eq!(ex, recalc(&b, &opts).unwrap());
    assert_eq!(ex.grand_total, 610_000);
    assert_eq!(b.snapshot().group_rules, b.group_rules);
    assert_eq!(b.snapshot().to_budget().group_rules, b.group_rules);

    let rules = GroupRules::default();
    assert_eq!(rules.counts([]), Ok(()));
    assert_eq!(rules.counts([true, false]), Err(LeftOut::GroupOff));
    let open = GroupRules {
        ungrouped_lines: Inclusion::Exclude,
        conflict: Inclusion::Include,
        ..rules
    };
    assert_eq!(open.counts([]), Err(LeftOut::Ungrouped));
    assert_eq!(open.counts([true, false]), Ok(()));
    assert_eq!(open.counts([false]), Err(LeftOut::GroupOff));
    for i in Inclusion::ALL {
        assert_eq!(Inclusion::parse(i.name()), Some(i));
    }
}

/// A group's total and active total, lines plus fringes.
fn group_figures(r: &Report, name: &str) -> (Cents, Cents) {
    r.groups[name].shown(&GroupRules::default()).unwrap()
}

/// A group's total is Movie Magic's (decisions.md, entry 58): every tagged
/// line at its base, suppressed ones included, plus only the fringes
/// actually earned. ZTEST07: ZOFF 5,000.00 total (Out 2,000.00 earns no
/// fringe while suppressed) and 0.00 active; ZIN 8,500.00 and 5,500.00;
/// ZIN2 5,000.00 and 5,000.00. With ZOFF on: 5,200.00 and 5,200.00.
#[test]
fn ztest07_group_totals_count_only_earned_fringes() {
    for arith in Arithmetic::ALL {
        let mut b = ztest07(arith);
        let (_, r) = totals(&b);
        assert_eq!(group_figures(&r, "ZOFF"), (500_000, 0), "{arith:?}");
        assert_eq!(group_figures(&r, "ZIN"), (850_000, 550_000), "{arith:?}");
        assert_eq!(group_figures(&r, "ZIN2"), (500_000, 500_000), "{arith:?}");
        assert_eq!(r.groups["ZOFF"].full.fringe_total, 0);

        // The what-if filters don't move a group's total.
        let filtered = recalc(
            &b,
            &RecalcOptions {
                exclude_groups: set(&["ZIN2"]),
                ..Default::default()
            },
        )
        .unwrap();
        let only = recalc(
            &b,
            &RecalcOptions {
                only_groups: Some(set(&["ZIN2"])),
                ..Default::default()
            },
        )
        .unwrap();
        for g in ["ZIN", "ZIN2", "ZOFF"] {
            assert_eq!(filtered.groups[g].full, r.groups[g].full, "{g} {arith:?}");
            assert_eq!(only.groups[g].full, r.groups[g].full, "{g} {arith:?}");
        }

        // "Fringes in group totals" Exclude: lines alone.
        let rules = GroupRules {
            fringes_in_totals: Inclusion::Exclude,
            ..GroupRules::default()
        };
        assert_eq!(r.groups["ZIN"].shown(&rules), Some((800_000, 500_000)));

        b.groups.insert("ZOFF".into(), group(true));
        let (_, on) = totals(&b);
        assert_eq!(group_figures(&on, "ZOFF"), (520_000, 520_000), "{arith:?}");
    }
}

/// ZTEST10: ZCRED's total is 6,500.00 (Grip days 4,000.00 with 500.00 of
/// fringes, Suppressed grip 2,000.00 at its base alone), active 4,500.00;
/// ZOFF 2,000.00 and 0.00, whatever the posting.
#[test]
fn ztest10_group_totals_count_only_earned_fringes() {
    for arith in Arithmetic::ALL {
        for posting in postings() {
            let (_, r) = totals(&ztest10(arith, posting.clone()));
            let at = format!("{arith:?} {posting:?}");
            assert_eq!(group_figures(&r, "ZCRED"), (650_000, 450_000), "{at}");
            assert_eq!(group_figures(&r, "ZOFF"), (200_000, 0), "{at}");
        }
    }
}

// ---------------------------------------------------------------------------
// Decisions.md, entry 60: new-budget defaults, the cutoff factor, totals in
// another currency, the topsheet's order and the budget's fringe row.

/// ZTEST04's single earners P-01 (1,000.00), P-02 (5,000.00) and P-10
/// (3,703.68), each with ZP125 (12.5%, cutoff 2,222.22), at cutoff factor
/// `factor`; plus a flat fringe with a stored cutoff, which no factor
/// touches.
fn ztest04_factor(arith: Arithmetic, factor: CutoffFactor) -> Budget {
    let mut flat_line = Detail::line(4, "Flat with a cutoff", "2", "Day", "100");
    flat_line.fringes = set(&["ZFCUT"]);
    let mut b = budget(
        arith,
        vec![category(
            "1100",
            "Z percent",
            vec![account(
                "1101",
                "Z single earners",
                vec![
                    fringed(1, "P-01", "1000", &["ZP125"], None),
                    fringed(2, "P-02", "5000", &["ZP125"], None),
                    fringed(3, "P-10", "3703.68", &["ZP125"], None),
                    flat_line,
                ],
            )],
        )],
    );
    b.fringes
        .insert("ZP125".into(), percent("12.5", Some(222_222)));
    b.fringes.insert(
        "ZFCUT".into(),
        Fringe {
            description: String::new(),
            short_code: None,
            rate: "100".into(),
            kind: FringeKind::Flat {
                unit: Some("Day".into()),
                basis: FlatBasis::Amount,
                max_hours_per_day: None,
                cutoff: Some(15_000),
            },
        },
    );
    b.cutoff_factor = factor;
    b
}

/// The cutoff factor multiplies every percent fringe's cutoff (V04-1,
/// V04-2, V04-3): at 1.5, 2.0 and 3.0, P-02 is 416.67, 555.55 and 625.00,
/// P-10 416.67, 462.96 and 462.96, and P-01 125.00 throughout. 4,444.44 x
/// 12.5% is 555.555, which doubles hold as 555.55499..., so 555.55 (exact
/// arithmetic: 555.56). 2.5 was not read in the reference product; it
/// follows the same rule. A flat fringe's stored cutoff stays unused (F-31:
/// 2 Days at 100.00 is 200.00 at every factor).
#[test]
fn v04_the_cutoff_factor_multiplies_every_percent_cutoff() {
    use CutoffFactor::*;
    let cases = [
        (Arithmetic::Mmb, One, [12_500, 27_778, 27_778]),
        (Arithmetic::Mmb, OneAndAHalf, [12_500, 41_667, 41_667]),
        (Arithmetic::Mmb, Two, [12_500, 55_555, 46_296]),
        (Arithmetic::Mmb, TwoAndAHalf, [12_500, 62_500, 46_296]),
        (Arithmetic::Mmb, Three, [12_500, 62_500, 46_296]),
        (Arithmetic::Exact, Two, [12_500, 55_556, 46_296]),
    ];
    for (arith, factor, want) in cases {
        let (_, r) = totals(&ztest04_factor(arith, factor));
        let f = line_fringes(&r);
        let got = [f[&1]["ZP125"], f[&2]["ZP125"], f[&3]["ZP125"]];
        assert_eq!(got, want, "{arith:?} {factor:?}");
        assert_eq!(f[&4]["ZFCUT"], 20_000, "{arith:?} {factor:?}");
    }
    // Explain says where a factored cutoff comes from.
    let b = ztest04_factor(Arithmetic::Mmb, OneAndAHalf);
    let text = explained(&b, Target::LineFringe(2, "ZP125".into()));
    assert!(text.contains("2,222.22 x 1.5 = 3,333.33"), "{text}");
    // Part of a version; read from text as the five factors only.
    assert_eq!(b.snapshot().to_budget().cutoff_factor, OneAndAHalf);
    assert_eq!(CutoffFactor::parse("2"), Some(Two));
    assert_eq!(CutoffFactor::parse("1.75"), None);
}

/// ZTEST08 CURRENCIES as the reference product ran it (CU-03 left out, its
/// 4X column was hidden): base USD; EUR at 3, GBP at 1.23457, JPY at
/// 0.00667, CAD at 1.234565; fringes ZP10N (10%), ZP125 (12.5%, cutoff
/// 2,222.22) and ZFDAY (25.00 a Day), posted by budget.
fn ztest08(arith: Arithmetic) -> Budget {
    let cur = |id: u64, desc: &str, amount: &str, unit: &str, code: &str, rate: &str| Detail {
        currency: Some(code.into()),
        ..Detail::line(id, desc, amount, unit, rate)
    };
    let with = |d: Detail, f: &str| Detail {
        fringes: set(&[f]),
        ..d
    };
    let mut b = budget(
        arith,
        vec![
            category(
                "1100",
                "ZCURRENCY",
                vec![account(
                    "1101",
                    "Z foreign lines",
                    vec![
                        cur(1, "Euro lens", "5", "", "EUR", "100"),
                        cur(2, "Pound line", "1", "", "GBP", "100"),
                        cur(4, "Euro rounding", "1", "", "EUR", "0.335"),
                        cur(5, "Yen line", "1", "", "JPY", "10000"),
                        cur(6, "Six place rate", "1", "", "CAD", "100000"),
                        with(
                            cur(7, "Euro percent fringe", "1", "", "EUR", "1000"),
                            "ZP10N",
                        ),
                        with(
                            cur(8, "Euro cutoff fringe", "1", "", "EUR", "1000"),
                            "ZP125",
                        ),
                        with(
                            cur(9, "Euro flat fringe", "4", "Day", "EUR", "100"),
                            "ZFDAY",
                        ),
                        Detail::adjustment(10, "ZADJ foreign", "0.1"),
                        Detail::line(11, "Base line", "1", "", "1000"),
                    ],
                )],
            ),
            level("ZTOTAL ATL"),
            category(
                "1200",
                "ZCURRENCY B",
                vec![account(
                    "1201",
                    "Z base only",
                    vec![Detail::line(12, "Base line two", "1", "", "500")],
                )],
            ),
            level("ZTOTAL BTL"),
        ],
    );
    for (code, rate) in [
        ("EUR", "3"),
        ("GBP", "1.23457"),
        ("JPY", "0.00667"),
        ("CAD", "1.234565"),
    ] {
        b.currencies.insert(
            code.into(),
            Currency {
                description: String::new(),
                symbol: String::new(),
                rate: d(rate),
                format: Default::default(),
            },
        );
    }
    b.fringes.insert("ZP10N".into(), percent("10", None));
    b.fringes
        .insert("ZP125".into(), percent("12.5", Some(222_222)));
    b.fringes.insert(
        "ZFDAY".into(),
        Fringe {
            description: String::new(),
            short_code: None,
            rate: "25".into(),
            kind: FringeKind::flat(Some("Day".into())),
        },
    );
    b.fringe_posting = FringePosting::Budget;
    b
}

/// Totals shown in another currency convert each account's total and
/// round it, convert a fringe row fringe by fringe, then add up (V08-1,
/// V08-2). In USD: 1100 146,582.44, 1200 500.00, fringes 677.78, grand
/// 147,760.22. In EUR (3): 48,860.81, 166.67, fringes 225.92 (100.00 +
/// 92.59 + 33.33, not 677.78 / 3 = 225.93), grand 49,253.40 (not 147,760.22
/// / 3 = 49,253.41). In GBP (1.23457): 118,731.57 (146,582.44 / 1.23457;
/// converting line by line would make it 118,731.60), 405.00, fringes
/// 549.00 (243.00 + 225.00 + 81.00, not 549.01), grand 119,685.57.
#[test]
fn v08_totals_in_another_currency_convert_account_by_account() {
    let b = ztest08(Arithmetic::Mmb);
    let (rows, r) = totals(&b);
    assert_eq!(rows[&10], 1_323_477);
    assert_eq!(
        (r.categories[0].total, r.categories[1].total),
        (14_658_244, 50_000)
    );
    assert_eq!(r.budget_fringes, 67_778);
    assert_eq!(r.grand_total, 14_776_022);
    assert!(r.display.is_none(), "totals in the base currency");

    for (code, cats, fringes, grand) in [
        ("EUR", [4_886_081, 16_667], 22_592, 4_925_340),
        ("GBP", [11_873_157, 40_500], 54_900, 11_968_557),
    ] {
        let mut b = ztest08(Arithmetic::Mmb);
        b.totals_currency = Some(code.into());
        let (_, r) = totals(&b);
        let shown = r.display.clone().expect("shown in another currency");
        assert_eq!(shown.categories, cats.to_vec(), "{code}");
        assert_eq!(shown.budget_fringes, fringes, "{code}");
        assert_eq!(shown.grand_total, grand, "{code}");
        assert_eq!(shown.net_total, grand, "{code}");
        assert_eq!(
            (shown.total_atl, shown.total_btl, shown.atl_btl),
            (cats[0], cats[1], cats[0] + cats[1]),
            "{code}"
        );
        if code == "EUR" {
            // The grand total divided as one amount is a cent off.
            assert_eq!(r.in_totals_currency(r.grand_total), Some(4_925_341));
        }
    }
}

/// The names the topsheet lists after its production totals, in order.
fn summary_names(b: &Budget, r: &Report) -> Vec<String> {
    r.topsheet_lines(b)
        .into_iter()
        .filter_map(|l| match l {
            TopsheetLine::BudgetFringes => Some(b.fringe_row.description().to_string()),
            TopsheetLine::Charge(i) => Some(r.charges[i].name.clone()),
            TopsheetLine::Grand => Some("Grand total".into()),
            TopsheetLine::Credit(i) => Some(r.credits[i].name.clone()),
            TopsheetLine::CreditsSubtotal => Some("Sub Total".into()),
            TopsheetLine::Net => Some("Net total".into()),
            _ => None,
        })
        .collect()
}

/// After the grand total, credits list in reverse application order, then
/// their Sub Total and the net total (ZTEST10: ZCR3, ZCR5, ZCR6, ZCR4,
/// ZCR2, ZCR1, Sub Total 3,349.53, net 5,450.47). Applied in the order 1,
/// 2, 4, 6, 5, 3, as the run showed them.
#[test]
fn ztest10_credits_list_in_reverse_application_order() {
    for arith in Arithmetic::ALL {
        let mut b = ztest10(arith, FringePosting::Budget);
        let made = ztest10_credits();
        b.credits = [0, 1, 3, 5, 4, 2]
            .iter()
            .map(|&i| made[i].clone())
            .collect();
        let (_, r) = totals(&b);
        assert_eq!(
            summary_names(&b, &r),
            [
                "Total Fringes",
                "Grand total",
                "ZCR3",
                "ZCR5",
                "ZCR6",
                "ZCR4",
                "ZCR2",
                "ZCR1",
                "Sub Total",
                "Net total"
            ],
            "{arith:?}"
        );
        assert_eq!(r.credits_total, 334_953, "{arith:?}");
        assert_eq!(r.net_total, 545_047, "{arith:?}");
        // One credit has no Sub Total.
        b.credits.truncate(1);
        let (_, r) = totals(&b);
        assert!(!r
            .topsheet_lines(&b)
            .contains(&TopsheetLine::CreditsSubtotal));
    }
}

/// Charges list newest first, after the summary rows and the budget's
/// fringe row (V09-2: the topsheet keeps creation order, newest first, and
/// un-applying one changes no other charge).
#[test]
fn ztest09_charges_list_newest_first() {
    let b = ztest09(FringePosting::Budget, true);
    let (_, r) = totals(&b);
    let mut want: Vec<String> = vec!["Total Fringes".into()];
    want.extend(b.charges.iter().rev().map(|c| c.name.clone()));
    want.extend(["Grand total".into(), "Net total".into()]);
    assert_eq!(summary_names(&b, &r), want);
    let lines = r.topsheet_lines(&b);
    let at = |l: TopsheetLine| lines.iter().position(|x| *x == l).unwrap();
    assert!(at(TopsheetLine::AtlBtl) < at(TopsheetLine::BudgetFringes));
    assert!(at(TopsheetLine::BudgetFringes) < at(TopsheetLine::Charge(r.charges.len() - 1)));
}

/// The budget's fringe row can carry an account number and its own
/// description, and move above any topsheet row; moving it changes no
/// total, ATL or BTL (ZTEST06 V06-3: ATL 15,000.00, BTL 13,400.00, fringes
/// 3,100.00, ZPCT5 1,625.00, grand 34,125.00 wherever the row shows).
#[test]
fn the_budget_fringe_row_moves_without_changing_a_total() {
    let mut b = ztest06(FringePosting::Budget);
    let (_, before) = totals(&b);
    assert_eq!(
        (before.total_atl, before.total_btl, before.budget_fringes),
        (1_500_000, 1_340_000, 310_000)
    );
    assert_eq!(before.charges[0].amount, 162_500);
    assert_eq!(before.grand_total, 3_412_500);
    // Its default: no number, "Total Fringes", after the summary rows.
    assert_eq!(b.fringe_row.account(), None);
    assert_eq!(b.fringe_row.description(), "Total Fringes");
    let lines = before.topsheet_lines(&b);
    let fr = lines
        .iter()
        .position(|l| *l == TopsheetLine::BudgetFringes)
        .unwrap();
    assert_eq!(lines[fr - 1], TopsheetLine::AtlBtl);

    for position in 0..=b.topsheet.len() + 1 {
        b.fringe_row = FringeRow {
            account: Some("9900".into()),
            description: Some("Fringe benefits".into()),
            position: Some(position),
        };
        let (_, r) = totals(&b);
        assert_eq!(r, before, "position {position}");
        let lines = r.topsheet_lines(&b);
        let fr = lines
            .iter()
            .position(|l| *l == TopsheetLine::BudgetFringes)
            .unwrap();
        // Just above topsheet row `position` (after the last one past the end).
        let rows_before = lines[..fr]
            .iter()
            .filter(|l| matches!(l, TopsheetLine::Category(_) | TopsheetLine::Level(_)))
            .count();
        assert_eq!(
            rows_before,
            position.min(b.topsheet.len()),
            "position {position}"
        );
        assert_eq!(
            lines
                .iter()
                .filter(|l| **l == TopsheetLine::BudgetFringes)
                .count(),
            1
        );
        assert_eq!(b.fringe_row.account(), Some("9900"));
        assert_eq!(b.fringe_row.description(), "Fringe benefits");
    }
    // Other postings have no budget fringe row to place.
    let mut c = ztest06(FringePosting::default());
    c.fringe_row.position = Some(0);
    let (_, r) = totals(&c);
    assert!(!r.topsheet_lines(&c).contains(&TopsheetLine::BudgetFringes));
    // Inserting and removing topsheet rows keeps it above the same row.
    let mut row = FringeRow {
        position: Some(2),
        ..FringeRow::default()
    };
    row.row_inserted(0);
    assert_eq!(row.position, Some(3));
    row.row_removed(1);
    assert_eq!(row.position, Some(2));
    row.rows_reordered(&[2, 0, 1]);
    assert_eq!(row.position, Some(0));
}

/// A global's unit fills a line's empty Unit when the line's Amt is that
/// global (G-08: ZG_W in Weeks, Amt ZG_W x 100 = 200.00, Unit Weeks).
#[test]
fn g08_a_global_fills_the_unit_of_a_line_that_names_it() {
    let mut b = budget(Arithmetic::Mmb, Vec::new());
    b.globals.insert("ZG_W".into(), "2".into());
    b.global_info.insert(
        "ZG_W".into(),
        GlobalInfo {
            unit: "Week".into(),
            ..GlobalInfo::default()
        },
    );
    b.globals.insert("ZG_A".into(), "3".into());
    assert_eq!(edit::unit_from_global(&b, "ZG_W"), Some("Week"));
    assert_eq!(edit::unit_from_global(&b, " ZG_W "), Some("Week"));
    // Only the global alone, and only one with a unit.
    assert_eq!(edit::unit_from_global(&b, "ZG_W * 2"), None);
    assert_eq!(edit::unit_from_global(&b, "ZG_A"), None);
    assert_eq!(edit::unit_from_global(&b, "NOPE"), None);
    b.topsheet = vec![category(
        "1100",
        "ZGLOBALS",
        vec![account(
            "1101",
            "Z globals",
            vec![Detail::line(1, "G-08", "ZG_W", "Week", "100")],
        )],
    )];
    let (rows, _) = totals(&b);
    assert_eq!(rows[&1], 20_000);
}

/// What a formula typed in may not be (V02-4): Amt `ZG_A/0` is refused,
/// and so is a global that refers back to itself, which shows no value
/// (counted as 0). Other problems are kept as typed and flagged.
#[test]
fn v02_4_dividing_by_zero_and_circular_globals_are_refused_at_entry() {
    let mut b = budget(Arithmetic::Mmb, Vec::new());
    b.globals.insert("ZG_A".into(), "3".into());
    assert!(edit::refused_formula(&b, "ZG_A/0").is_some());
    assert!(edit::refused_formula(&b, "ZG_A / (ZG_A - 3)").is_some());
    assert_eq!(edit::refused_formula(&b, "ZG_A / 2"), None);
    assert_eq!(edit::refused_formula(&b, "NOPE"), None);
    assert_eq!(edit::refused_formula(&b, "3 +"), None);

    // ZG_X = ZG_Y + 1 and ZG_Y = ZG_X + 1: the second is refused.
    assert_eq!(edit::refused_global(&b, "ZG_X", "ZG_Y + 1"), None);
    b.globals.insert("ZG_X".into(), "ZG_Y + 1".into());
    assert!(edit::refused_global(&b, "ZG_Y", "ZG_X + 1").is_some());
    assert!(edit::refused_global(&b, "ZG_Y", "ZG_Y * 2").is_some());
    assert!(edit::refused_global(&b, "ZG_Y", "1 / 0").is_some());
    assert_eq!(edit::refused_global(&b, "ZG_Y", "4"), None);

    // One from a file: both are circular and can't be used in a cell; the
    // engine still refuses to total the budget (`method check` says so).
    b.globals.insert("ZG_Y".into(), "ZG_X + 1".into());
    assert_eq!(
        edit::circular_globals(&b),
        ["ZG_X".to_string(), "ZG_Y".to_string()].into()
    );
    assert!(edit::refused_formula(&b, "ZG_X").is_some());
    assert_eq!(edit::refused_formula(&b, "ZG_A * 2"), None);
    assert!(matches!(
        recalc(&b, &RecalcOptions::default()).unwrap_err().kind,
        ErrorKind::GlobalCycle(_)
    ));
}

/// A new budget starts with the reference product's settings (run log C2,
/// ZTEST05, ZTEST07): fringes posted by budget, account numbers of
/// category 2, account 2 and set 3, starter units Day 12 hours, Week 60,
/// Month 240, Hour 1, Allow and Flat with none, Fringe copied from the
/// line above (Group, Location, Set and Currency not), cutoff factor 1.0.
/// A file that doesn't say keeps the old defaults.
#[test]
fn new_budgets_start_as_the_reference_product_starts_them() {
    for b in [starter::budget(), starter::blank("New", "USD")] {
        assert_eq!(b.fringe_posting, FringePosting::Budget);
        let c = b.account_coding;
        assert_eq!(
            (c.category_width, c.account_width, c.sub_width, c.set_width),
            (2, 2, 0, 3)
        );
        // No separator: numbers read `1101` (decisions.md, entry 61).
        assert_eq!(c.separator, None);
        assert_eq!(c.validate(), Ok(()));
        let hours = |u: &str| b.units[u].hours;
        assert_eq!(hours("Day"), Some(d("12")));
        assert_eq!(hours("Week"), Some(d("60")));
        assert_eq!(hours("Month"), Some(d("240")));
        assert_eq!(hours("Hour"), Some(d("1")));
        assert_eq!((hours("Allow"), hours("Flat")), (None, None));
        assert_eq!(b.units.len(), 6);
        let n = b.new_line_defaults;
        assert!(n.fringes && !n.groups && !n.location && !n.set && !n.currency);
        assert_eq!(b.cutoff_factor, CutoffFactor::One);
        assert!(b.fringe_row.is_default());
    }
    // A flat fringe per Day on a Week line counts 5 Days (60 / 12).
    let mut b = starter::blank("New", "USD");
    b.fringes.insert(
        "ZFDAY".into(),
        Fringe {
            description: String::new(),
            short_code: None,
            rate: "25".into(),
            kind: FringeKind::flat(Some("Day".into())),
        },
    );
    let mut week = Detail::line(1, "A week", "1", "Week", "1000");
    week.fringes = set(&["ZFDAY"]);
    b.topsheet = vec![category("11", "Z", vec![account("11-01", "Z", vec![week])])];
    let (_, r) = totals(&b);
    assert_eq!(r.budget_fringes, 12_500);
}

// ---------------------------------------------------------------------------
// Parity run 2 (2026-10-12; imports/mmb-parity/2026-10-12/run-log.md, cases
// R2-01 to R2-08; decisions.md, entry 61)

/// Credit row on account `code`.
fn on_account(code: &str) -> CreditSource {
    CreditSource::Account {
        account: code.into(),
    }
}

/// R2-04: an account row's spend is the account's lines with the fringes
/// they earn, wherever fringes post: 1101 is 4,000.00 + 500.00 = 4,500.00
/// even posting by budget. Ticked onto ZCR3 it starts at 0%, so the credit
/// stays 411.52; at 10% it adds 450.00, and the rows round once together.
#[test]
fn r2_04_an_account_credit_row_counts_the_account_with_its_fringes() {
    for arith in Arithmetic::ALL {
        for posting in postings() {
            let at = format!("{arith:?} {posting:?}");
            let mut b = ztest10(arith, posting.clone());
            b.credits.push(credit(
                "ZCR3",
                vec![
                    credit_row(CreditSource::Flat(123_457), "33.333", CreditCap::None),
                    credit_row(on_account("1101"), "0", CreditCap::None),
                ],
            ));
            let (_, r) = totals(&b);
            let row = &r.credits[0].rows[1];
            assert_eq!(
                (row.gross, row.excluded_fringes, row.net),
                (450_000, 0, 450_000),
                "{at}"
            );
            assert_eq!(r.credits[0].amount, 41_152, "{at}");
            assert_eq!(r.grand_total, 880_000, "{at}");

            b.credits[0].rows[1].percent = d("10");
            let (_, r) = totals(&b);
            // 411.51824... + 450.00, rounded once.
            assert_eq!(r.credits[0].amount, 86_152, "{at}");
            let text = explained(&b, Target::Credit(0));
            assert!(
                text.contains("account 1101 Z crew credit with its fringes"),
                "{text}"
            );
        }
    }
}

/// An account row leaves out the credit's excluded fringes (one list per
/// credit), takes a cap, follows a renumbering, and one on an account the
/// budget doesn't have is 0 with a warning.
#[test]
fn r2_04_an_account_credit_row_excludes_fringes_and_follows_the_chart() {
    for arith in Arithmetic::ALL {
        let mut b = ztest10(arith, FringePosting::Budget);
        b.credits.push(credit_less(
            "ZACC",
            &["ZFDAY"],
            vec![
                credit_row(on_account("1101"), "10", CreditCap::None),
                credit_row(on_account("2101"), "10", CreditCap::QualifiedSpend(200_000)),
            ],
        ));
        b.credits.push(credit(
            "ZMISS",
            vec![credit_row(on_account("9901"), "10", CreditCap::None)],
        ));
        let (_, r) = totals(&b);
        let rows = &r.credits[0].rows;
        assert_eq!(
            (rows[0].gross, rows[0].excluded_fringes, rows[0].net),
            (450_000, 10_000, 440_000)
        );
        // 2101 is 4,000.00 + 300.00, its spend capped at 2,000.00.
        assert_eq!(rows[1].gross, 430_000);
        assert_eq!(rows[1].credit, d("200"));
        assert_eq!(r.credits[0].amount, 64_000, "{arith:?}");
        assert_eq!(r.credits[1].rows[0].gross, 0);
        assert_eq!(r.credits[1].amount, 0);
        assert_eq!(
            warnings(&b, &r),
            vec![
                "credit ZMISS row 1 is on account 9901, which is not in the budget, so its spend is 0"
                    .to_string()
            ]
        );
        let text = explained(&b, Target::Credit(1));
        assert!(text.contains("account 9901 is not in the budget"), "{text}");

        let to = method_core::coding::AccountCoding {
            separator: Some('.'),
            ..b.account_coding
        };
        method_core::renumber::renumber(&mut b, to).unwrap();
        assert_eq!(b.credits[0].rows[0].source, on_account("11.01"));
        let (_, again) = totals(&b);
        assert_eq!(again.credits[0].amount, r.credits[0].amount);
    }
}

/// R2-06: a flat fringe ignores ranges and cutoffs. ZFDAYR, 100.00 a Day
/// with a 500.00 cutoff, on two lines of 3 Days at 100.00 in one range is
/// 300.00 + 300.00, as without the range.
#[test]
fn r2_06_a_flat_fringe_in_a_range_ignores_the_range_and_the_cutoff() {
    for arith in Arithmetic::ALL {
        let day_line = |id: u64, desc: &str, range: Option<u32>| Detail {
            fringes: set(&["ZFDAYR"]),
            fringe_range: range,
            ..Detail::line(id, desc, "3", "Day", "100")
        };
        let mut b = budget(
            arith,
            vec![category(
                "1200",
                "ZPERCENT B",
                vec![
                    account(
                        "1205",
                        "Z flat range",
                        vec![day_line(1, "A", Some(1)), day_line(2, "B", Some(1))],
                    ),
                    account(
                        "1206",
                        "Z flat no range",
                        vec![day_line(3, "A", None), day_line(4, "B", None)],
                    ),
                ],
            )],
        );
        b.fringes.insert(
            "ZFDAYR".into(),
            Fringe {
                description: String::new(),
                short_code: None,
                rate: "100".into(),
                kind: FringeKind::Flat {
                    unit: Some("Day".into()),
                    basis: FlatBasis::Amount,
                    max_hours_per_day: None,
                    cutoff: Some(50_000),
                },
            },
        );
        let (_, r) = totals(&b);
        let fringe = |id: u64| row(&r, id).fringe_total;
        assert_eq!(
            [fringe(1), fringe(2), fringe(3), fringe(4)],
            [30_000; 4],
            "{arith:?}"
        );
        assert_eq!(r.categories[0].fringe_total, 120_000);
        // Before cutoffs the same: a flat fringe has none.
        assert_eq!(row(&r, 2).fringe_full_total, 30_000);
    }
}

/// R2-05: a suppressed line's effective fringe percent and its percent
/// before cutoffs are both nothing: it earns nothing either way. A line
/// that earns has both (ZP10N: 10.00%); a line or adjustment row without
/// a fringe has nothing.
#[test]
fn r2_05_fringe_percents_of_a_suppressed_line_are_zero() {
    for arith in Arithmetic::ALL {
        let (_, r) = totals(&ztest07(arith));
        let out = row(&r, 307);
        assert!(!out.included);
        assert_eq!((out.fringe_total, out.fringe_full_total), (0, 0));
        let both = row(&r, 308);
        assert_eq!((both.fringe_total, both.fringe_full_total), (0, 0));
        let in_line = row(&r, 306);
        assert_eq!(
            (
                in_line.total,
                in_line.fringe_total,
                in_line.fringe_full_total
            ),
            (100_000, 10_000, 10_000)
        );
        assert_eq!(row(&r, 310).fringe_full_total, 0);
        assert_eq!(row(&r, 312).fringe_full_total, 0);
    }
}

/// The full fringe percent (the reference's Agg Sum in the 2026-10-11
/// exports): a range's lines past the cutoff earn less, but before cutoffs
/// each is its own percent (Wrap earns 0.00 and is 12.5% before cutoffs);
/// a 0.04 line at 12.5% is 0.01 (25%), rounded on its own; a flat fringe
/// counts as earned (Grip days: 100.00 + 400.00 on 4,000.00 is 12.5%).
#[test]
fn the_full_fringe_percent_counts_every_fringe_before_cutoffs() {
    for arith in Arithmetic::ALL {
        let prep = fringed(1, "Prep", "1249.36", &["ZP125"], Some(1));
        let shoot = fringed(2, "Shoot", "5309.78", &["ZP125"], Some(1));
        let wrap = fringed(3, "Wrap", "624.68", &["ZP125"], Some(1));
        let tiny = fringed(4, "Tiny", "0.04", &["ZP125"], None);
        let mut b = budget(
            arith,
            vec![category(
                "1100",
                "Z",
                vec![account("1102", "Z range", vec![prep, shoot, wrap, tiny])],
            )],
        );
        b.fringes
            .insert("ZP125".into(), percent("12.5", Some(222_222)));
        let (_, r) = totals(&b);
        let got = |id: u64| (row(&r, id).fringe_total, row(&r, id).fringe_full_total);
        assert_eq!(got(1), (15_617, 15_617), "{arith:?}");
        assert_eq!(got(2), (12_161, 66_372), "{arith:?}");
        let wrap_full = match arith {
            // 624.68 is 624.67999... as a double, so 78.08.
            Arithmetic::Mmb => 7_808,
            Arithmetic::Exact => 7_809,
        };
        assert_eq!(got(3), (0, wrap_full), "{arith:?}");
        assert_eq!(got(4), (1, 1), "{arith:?}");

        let (_, r) = totals(&ztest10(arith, FringePosting::Budget));
        assert_eq!(row(&r, 399).fringe_full_total, 50_000, "{arith:?}");
        assert_eq!(row(&r, 400).fringe_full_total, 0, "suppressed");
    }
}

/// R2-02: "fringe amounts in group totals" set to Exclude shows the lines
/// alone: ZIN 8,000.00 / 5,000.00, ZIN2 4,600.00 / 4,600.00, ZOFF 5,000.00 /
/// 0.00; display only, so the grand total stays 7,150.00.
#[test]
fn r2_02_group_totals_without_fringes() {
    for arith in Arithmetic::ALL {
        let mut b = ztest07(arith);
        let (_, before) = totals(&b);
        b.group_rules.fringes_in_totals = Inclusion::Exclude;
        let (_, r) = totals(&b);
        let shown = |name: &str| r.groups[name].shown(&b.group_rules).unwrap();
        assert_eq!(shown("ZIN"), (800_000, 500_000), "{arith:?}");
        assert_eq!(shown("ZIN2"), (460_000, 460_000), "{arith:?}");
        assert_eq!(shown("ZOFF"), (500_000, 0), "{arith:?}");
        assert_eq!(r.grand_total, 715_000);
        assert_eq!(r, before, "display only");
    }
}

/// R2-07: a category's fringe account under category posting is the
/// category's first two characters and the suffix (coding 2 + 2, suffix
/// 98): 510 is 5198, 63 is 6398, 7000 is 7098; 1100 with 99 is 1199.
#[test]
fn r2_07_category_fringe_accounts_take_the_category_part() {
    let one = |code: &str, acct: &str, id: u64| {
        category(
            code,
            "Z",
            vec![account(
                acct,
                "Z",
                vec![fringed(id, "Pay", "1000", &["ZP10N"], None)],
            )],
        )
    };
    for arith in Arithmetic::ALL {
        let mut b = budget(
            arith,
            vec![
                one("510", "511", 1),
                one("63", "6301", 2),
                one("7000", "7001", 3),
            ],
        );
        b.account_coding = method_core::coding::AccountCoding {
            separator: None,
            ..method_core::coding::AccountCoding::default()
        };
        b.fringe_posting = FringePosting::Category {
            suffix: "98".into(),
        };
        b.fringes.insert("ZP10N".into(), percent("10", None));
        let (_, r) = totals(&b);
        let numbers: Vec<&str> = r
            .categories
            .iter()
            .map(|c| c.fringe_account.as_ref().unwrap().0.as_str())
            .collect();
        assert_eq!(numbers, ["5198", "6398", "7098"], "{arith:?}");
        assert!(r.categories.iter().all(|c| c.fringe_total == 10_000));

        b.topsheet = vec![one("1100", "1101", 4)];
        b.fringe_posting = FringePosting::Category {
            suffix: "99".into(),
        };
        let (_, r) = totals(&b);
        assert_eq!(r.categories[0].fringe_account.as_ref().unwrap().0, "1199");
    }
}

/// Account totals in whole units (decisions.md, entry 69; the reference's
/// "Decimal Places in Totals" at 0, read on a feature budget's exports at
/// 0 and 2 places, 2026-10-12): each account's lines total is rounded to
/// the nearest whole unit (3,426.50 is 3,427.00, as the export shows), and
/// categories, the total before charges, a percent charge and the grand
/// and net totals are worked out from the rounded accounts; lines, fringes
/// and a credit on a category row keep their cents. Exact arithmetic
/// rounds a negative half away from zero, binary toward plus infinity.
#[test]
fn account_totals_in_whole_units() {
    fn whole(arith: Arithmetic, round: bool, posting: FringePosting) -> Budget {
        let mut lead = Detail::line(1, "Lead", "1", "", "3426.50");
        lead.fringes = set(&["ZFICA"]);
        let mut b = budget(
            arith,
            vec![category(
                "1100",
                "Z whole",
                vec![
                    account("1101", "Z half", vec![lead]),
                    account(
                        "1102",
                        "Z up",
                        vec![Detail::line(2, "Kit", "1", "", "1975.70")],
                    ),
                    account(
                        "1103",
                        "Z down",
                        vec![Detail::line(3, "Fee", "1", "", "100.25")],
                    ),
                    account(
                        "1104",
                        "Z refund",
                        vec![Detail::line(4, "Back", "-1", "", "50.50")],
                    ),
                ],
            )],
        );
        b.fringe_posting = posting;
        b.round_account_totals = round;
        b.fringes.insert("ZFICA".into(), percent("6.2", None));
        b.charges.push(percent_charge("ZCONT", "10"));
        b.credits.push(credit(
            "ZCREDIT",
            vec![credit_row(on_category("1100"), "10", CreditCap::None)],
        ));
        b
    }
    for arith in Arithmetic::ALL {
        let (rows, r) = totals(&whole(arith, true, FringePosting::Budget));
        // Lines and fringes keep their cents.
        assert_eq!(rows[&1], 342_650);
        assert_eq!(r.budget_fringes, 21_244);
        let accts: Vec<Cents> = r.categories[0].accounts.iter().map(|a| a.total).collect();
        let refund = match arith {
            Arithmetic::Mmb => -5_000,
            Arithmetic::Exact => -5_100,
        };
        assert_eq!(accts, [342_700, 197_600, 10_000, refund], "{arith:?}");
        let cat = 342_700 + 197_600 + 10_000 + refund;
        assert_eq!(r.categories[0].total, cat);
        // 10% of the rounded total before charges.
        let base = cat + 21_244;
        let charge = match arith {
            Arithmetic::Mmb => 56_654,
            Arithmetic::Exact => 56_644,
        };
        assert_eq!(r.charges[0].base, base);
        assert_eq!(r.charges[0].amount, charge, "{arith:?}");
        assert_eq!(r.grand_total, base + charge);
        // The credit's category row counts the lines as they are:
        // 5,451.95 + 212.44, 10% of it 566.439.
        assert_eq!(r.credits[0].rows[0].gross, 566_439);
        assert_eq!(r.credits[0].amount, 56_644);
        assert_eq!(r.net_total, base + charge - 56_644);

        // Off, nothing is rounded: 5,451.95 + 212.44 = 5,664.39, 566.44.
        let (_, off) = totals(&whole(arith, false, FringePosting::Budget));
        assert_eq!(off.categories[0].total, 545_195);
        assert_eq!(off.grand_total, 566_439 + 56_644);

        // Posted by account, the fringes add to the rounded lines.
        let (_, by_account) = totals(&whole(arith, true, FringePosting::Account));
        assert_eq!(by_account.categories[0].accounts[0].total, 342_700 + 21_244);

        // Explain shows the step.
        let b = whole(arith, true, FringePosting::Budget);
        let e = Explainer::new(&b, &RecalcOptions::default())
            .unwrap()
            .explain(&Target::AccountLines { ci: 0, ai: 0 })
            .unwrap();
        let text = e.to_text(None);
        assert!(
            text.contains(
                "the budget rounds account totals to whole units: the lines come to 3,426.50"
            ),
            "{text}"
        );
        assert_eq!(e.reported, 342_700);
    }
}
