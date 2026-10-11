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
