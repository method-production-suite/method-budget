//! Properties of rate tables and filling a line's rate from one
//! (decisions.md, entry 90), on random tables, units and lines. Every name
//! and figure is invented.

mod common;

use common::*;
use method_core::rates::{self, RateCheck, RateRow, RateTable};
use method_core::*;

const CASES: u64 = 400;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }
    /// A figure with up to two decimal places.
    fn figure(&mut self) -> Dec {
        Dec::from_cents(self.below(1_000_000) as i64)
    }
}

/// The units a case uses: some with hours, some without.
const UNITS: [(&str, Option<i64>); 5] = [
    ("Hour", Some(1)),
    ("Day", Some(12)),
    ("Week", Some(60)),
    ("Flat", None),
    ("Shift", Some(10)),
];

fn case(rng: &mut Rng) -> Budget {
    let mut b = Budget::new("Invented", "USD");
    b.units.clear();
    for (u, h) in UNITS {
        // Sometimes a unit loses its hours.
        let hours = h.filter(|_| rng.chance(85)).map(Dec::from_int);
        b.units.insert(
            u.into(),
            UnitDef {
                plural: format!("{u}s"),
                description: String::new(),
                hours,
            },
        );
    }
    let mut rows = Vec::new();
    for i in 0..1 + rng.below(4) {
        let mut rates = std::collections::BTreeMap::new();
        for (u, _) in UNITS {
            if rng.chance(45) {
                rates.insert(u.to_string(), rng.figure());
            }
        }
        if !rates.is_empty() {
            rows.push(RateRow {
                classification: format!("Made Up {i}"),
                rates,
                from: None,
                to: None,
                notes: String::new(),
            });
        }
    }
    let rows = if rows.is_empty() {
        vec![RateRow {
            classification: "Made Up 0".into(),
            rates: [("Day".to_string(), rng.figure())].into(),
            ..RateRow::default()
        }]
    } else {
        rows
    };
    b.rate_tables.push(RateTable {
        name: "Invented Crew".into(),
        notes: String::new(),
        rows,
    });
    let unit = UNITS[rng.below(UNITS.len() as u64) as usize].0;
    b.topsheet.push(category(
        "10",
        "Crew",
        vec![account(
            "1000",
            "Crew",
            vec![
                Detail::line(1, "Invented line", "3", unit, "77"),
                Detail::line(2, "Another", "1", "", "5"),
            ],
        )],
    ));
    b
}

fn line(b: &Budget, id: RowId) -> &Detail {
    b.categories()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.details)
        .find(|d| d.id == id)
        .unwrap()
}

/// A fill either writes the table's figure (as is, or converted through
/// hours both units have, rounded to cents) with its source and nothing
/// else, or changes nothing at all.
#[test]
fn a_fill_writes_the_tables_figure_or_nothing() {
    for seed in 1..=CASES {
        let mut rng = Rng(seed);
        let b = case(&mut rng);
        let unit = line(&b, 1).unit.clone();
        let row = b.rate_tables[0].rows[0].clone();
        let mut after = b.clone();
        match rates::fill(
            &mut after,
            1,
            "Invented Crew",
            &row.classification,
            None,
            None,
        ) {
            Ok(f) => {
                let d = line(&after, 1);
                assert_eq!(
                    d.rate.0.parse::<Dec>().unwrap(),
                    f.found.rate,
                    "seed {seed}"
                );
                assert_eq!(d.unit3, unit, "seed {seed}");
                match row.rates.get(&unit) {
                    Some(v) => {
                        assert_eq!(f.found.rate, *v, "seed {seed}: the figure as it is");
                        assert_eq!(f.found.converted, None);
                    }
                    None => {
                        let (from_h, to_h) = f.found.converted.expect("converted");
                        assert_eq!(Some(to_h), b.units[&unit].hours, "seed {seed}");
                        assert_eq!(
                            Some(from_h),
                            b.units[&f.found.table_unit].hours,
                            "seed {seed}"
                        );
                        let exact = f
                            .found
                            .table_rate
                            .checked_mul(to_h)
                            .unwrap()
                            .checked_div(from_h)
                            .unwrap();
                        assert_eq!(
                            f.found.rate,
                            Dec::from_cents(exact.to_cents().unwrap()),
                            "seed {seed}"
                        );
                    }
                }
                // Only that line's rate, rate unit and source changed.
                let mut undo = after.clone();
                {
                    let d = undo
                        .categories_mut()
                        .flat_map(|c| &mut c.accounts)
                        .flat_map(|a| &mut a.details)
                        .find(|d| d.id == 1)
                        .unwrap();
                    d.rate = "77".into();
                    d.unit3 = String::new();
                    d.rate_source = None;
                }
                assert_eq!(undo, b, "seed {seed}");
                assert_eq!(
                    rates::check(&after, line(&after, 1)),
                    Some(RateCheck::Current)
                );
            }
            Err(_) => {
                assert_eq!(after, b, "seed {seed}: a refused fill changes nothing");
                // Refused only when no figure can be had for the unit.
                let has_hours = |u: &str| {
                    b.units
                        .get(u)
                        .and_then(|d| d.hours)
                        .is_some_and(|h| h > Dec::ZERO)
                };
                assert!(
                    !row.rates.contains_key(&unit)
                        && (!has_hours(&unit) || !row.rates.keys().any(|u| has_hours(u))),
                    "seed {seed}"
                );
            }
        }
    }
}

/// Changing a table never changes a line: the line says what the table
/// says now, and only an update takes it.
#[test]
fn a_changed_table_changes_no_figure_until_updated() {
    for seed in 1..=CASES {
        let mut rng = Rng(seed);
        let mut b = case(&mut rng);
        let class = b.rate_tables[0].rows[0].classification.clone();
        let Ok(_) = rates::fill(&mut b, 1, "Invented Crew", &class, None, None) else {
            continue;
        };
        let filled = line(&b, 1).clone();
        let report = recalc(&b, &RecalcOptions::default()).unwrap();
        // Every figure in the row moves.
        for v in b.rate_tables[0].rows[0].rates.values_mut() {
            *v = v.checked_add(Dec::from_int(1)).unwrap();
        }
        assert_eq!(line(&b, 1), &filled, "seed {seed}");
        assert_eq!(
            recalc(&b, &RecalcOptions::default()).unwrap(),
            report,
            "seed {seed}"
        );
        let Some(RateCheck::Changed { now }) = rates::check(&b, line(&b, 1)) else {
            panic!("seed {seed}: the line should say the table changed");
        };
        assert_eq!(
            rates::changed_lines(&b),
            vec![(1, filled.rate_source.clone().unwrap().rate, now)]
        );
        assert_eq!(rates::update(&mut b, 1).unwrap(), now);
        assert_eq!(line(&b, 1).rate.0.parse::<Dec>().unwrap(), now);
        assert_eq!(
            rates::check(&b, line(&b, 1)),
            Some(RateCheck::Current),
            "seed {seed}"
        );
        // Typed over: never updated.
        let mut typed = b.clone();
        for d in typed
            .categories_mut()
            .flat_map(|c| &mut c.accounts)
            .flat_map(|a| &mut a.details)
            .filter(|d| d.id == 1)
        {
            d.rate = "1".into();
        }
        assert_eq!(
            rates::check(&typed, line(&typed, 1)),
            Some(RateCheck::Edited)
        );
        assert!(rates::update(&mut typed, 1).is_err());
    }
}

/// With dated rows, a date picks exactly the row in effect then, and a
/// table whose periods share a day is refused.
#[test]
fn a_date_picks_the_row_in_effect() {
    for seed in 1..=CASES {
        let mut rng = Rng(seed);
        let years = 1 + rng.below(4) as i32;
        let rows: Vec<RateRow> = (0..years)
            .map(|y| RateRow {
                classification: "Made Up".into(),
                rates: [("Day".to_string(), Dec::from_int(100 + i64::from(y)))].into(),
                from: Some(format!("{}-01-01", 2030 + y)),
                to: (y + 1 < years || rng.chance(50)).then(|| format!("{}-12-31", 2030 + y)),
                notes: String::new(),
            })
            .collect();
        let t = RateTable {
            name: "Invented".into(),
            notes: String::new(),
            rows: rows.clone(),
        };
        t.validate().unwrap();
        for y in 0..years {
            let day = format!(
                "{}-{:02}-{:02}",
                2030 + y,
                1 + rng.below(12),
                1 + rng.below(28)
            );
            let r = rates::pick_row(&t, "made up", Some(&day)).unwrap();
            assert_eq!(
                r.rates["Day"],
                Dec::from_int(100 + i64::from(y)),
                "seed {seed} {day}"
            );
        }
        assert!(rates::pick_row(&t, "Made Up", Some("2029-12-31")).is_err());
        if years > 1 {
            assert!(
                rates::pick_row(&t, "Made Up", None).is_err(),
                "more than one period needs a date"
            );
            let mut overlap = t.clone();
            overlap.rows[1].from = Some(format!("{}-12-31", 2030));
            assert!(overlap.validate().is_err(), "seed {seed}");
        }
    }
}
