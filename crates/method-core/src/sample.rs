//! A large, deterministic sample budget for benchmarks and tests.
//!
//! Real budgets run to thousands of lines; the app has to stay quick at that
//! size, and the engine's optimizations have to give the same numbers. This
//! builds one from a seed: invented names and rates (not from any real
//! production or vendor template), but the shapes real budgets have: globals
//! in formulas, percent fringes with cutoffs, flat fringes per day, fringe
//! ranges, subtotal, adjustment and separator rows, groups (one out of the
//! total), sets, locations, foreign currencies, charges with exclusions,
//! credits on groups, and a locked baseline.

use std::collections::BTreeSet;

use crate::calc::{recalc, RecalcOptions};
use crate::decimal::Dec;
use crate::model::{
    Account, Baseline, Budget, Category, Charge, ChargeKind, Credit, CreditCap, CreditRow,
    CreditSource, Currency, Detail, Formula, Fringe, FringeKind, Group, InfoValue, Location, Note,
    ProductionTotal, SetDef, TopsheetRow, UnitDef,
};

/// How big a sample to make.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SampleSpec {
    /// Same seed, same budget.
    pub seed: u64,
    pub categories: usize,
    pub accounts: usize,
    /// Detail rows in all (lines, subtotals, adjustments and separators).
    pub rows: usize,
    pub fringes: usize,
    pub groups: usize,
    /// Lock a baseline from the generated totals.
    pub baseline: bool,
}

impl Default for SampleSpec {
    /// About the size of a big feature budget: 8 categories, 80 accounts,
    /// 5,000 rows, 40 fringes and 15 groups.
    fn default() -> SampleSpec {
        SampleSpec {
            seed: 1,
            categories: 8,
            accounts: 80,
            rows: 5_000,
            fringes: 40,
            groups: 15,
            baseline: true,
        }
    }
}

/// xorshift64*: tiny and deterministic.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: u64) -> u64 {
        if n == 0 {
            0
        } else {
            self.next() % n
        }
    }

    fn chance(&mut self, pct: u64) -> bool {
        self.below(100) < pct
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len() as u64) as usize]
    }
}

const CATEGORY_NAMES: [&str; 12] = [
    "Story and rights",
    "Producers and director",
    "Cast",
    "Production staff",
    "Camera and lighting",
    "Art and sets",
    "Locations and travel",
    "Wardrobe and makeup",
    "Editorial",
    "Sound and music",
    "Visual effects",
    "General expenses",
];

const ROLES: [&str; 16] = [
    "Coordinator",
    "Assistant",
    "Supervisor",
    "Operator",
    "Technician",
    "Driver",
    "Buyer",
    "Designer",
    "Lead",
    "Second",
    "Third",
    "Trainee",
    "Rental",
    "Purchases",
    "Kit fee",
    "Overtime",
];

const UNITS: [&str; 6] = ["Day", "Week", "Hour", "Flat", "Allow", "Shift"];

fn dec(s: &str) -> Dec {
    s.parse().expect("valid sample decimal")
}

/// The sample budget for `spec`. Row ids run from 1 and are unique; every
/// name a row uses is defined, so it recalculates without errors.
pub fn budget(spec: &SampleSpec) -> Budget {
    let mut rng = Rng::new(spec.seed);
    let mut b = Budget::new(format!("Sample feature {}", spec.seed), "USD");
    b.info
        .insert("Shoot days".into(), InfoValue::Number(Dec::from_int(42)));
    b.info
        .insert("Director".into(), InfoValue::Text("A. Director".into()));

    for (code, rate) in [("CAD", "0.73"), ("GBP", "1.27"), ("EUR", "1.08")] {
        b.currencies.insert(
            code.into(),
            Currency {
                description: format!("{code} (sample rate)"),
                symbol: String::new(),
                rate: dec(rate),
                format: Default::default(),
            },
        );
    }
    b.units.insert(
        "Shift".into(),
        UnitDef {
            plural: "Shifts".into(),
            description: "Twelve hours".into(),
            hours: Some(Dec::from_int(12)),
        },
    );

    let globals: [(&str, &str); 10] = [
        ("PREP_WEEKS", "6"),
        ("SHOOT_DAYS", "42"),
        ("WRAP_WEEKS", "3"),
        ("SHOOT_WEEKS", "SHOOT_DAYS / 5"),
        ("TOTAL_WEEKS", "PREP_WEEKS + SHOOT_WEEKS + WRAP_WEEKS"),
        ("CREW_SIZE", "60"),
        ("CAST_SIZE", "24"),
        ("OT_FACTOR", "1.5"),
        ("PER_DIEM", "75"),
        ("TRAVEL_DAYS", "4"),
    ];
    for (name, f) in globals {
        b.globals.insert(name.into(), Formula::from(f));
    }
    let amounts = [
        "SHOOT_DAYS",
        "SHOOT_WEEKS",
        "PREP_WEEKS + SHOOT_WEEKS",
        "TOTAL_WEEKS",
        "TRAVEL_DAYS",
        "CREW_SIZE / 4",
    ];

    // Fringes: mostly percents (some capped per earner), the rest flat, some
    // per day. The first few are the common payroll ones every crew line has.
    let mut fringe_names = Vec::new();
    for i in 0..spec.fringes {
        let name = format!("FR{i:02}");
        let kind = match i % 5 {
            0..=2 => FringeKind::Percent {
                cutoff: (i % 3 == 0).then(|| (rng.below(150_000) as i64 + 20_000) * 100),
            },
            3 => FringeKind::flat(Some("Day".into())),
            _ => FringeKind::flat(None),
        };
        let rate = match kind {
            FringeKind::Percent { .. } => format!("{}.{:02}", rng.below(12) + 1, rng.below(100)),
            FringeKind::Flat { .. } => format!("{}", rng.below(40) + 5),
        };
        b.fringes.insert(
            name.clone(),
            Fringe {
                description: format!("Sample fringe {i}"),
                short_code: (i % 3 == 0).then(|| format!("F{i}")),
                rate: rate.into(),
                kind,
            },
        );
        fringe_names.push(name);
    }

    let mut group_names = Vec::new();
    for i in 0..spec.groups {
        let name = format!("G{i:02}");
        b.groups.insert(
            name.clone(),
            Group {
                description: format!("Sample group {i}"),
                short_code: (i % 2 == 0).then(|| format!("G{i}")),
                // One group is out of the budget total, as a "hold" group would be.
                in_budget_total: i != 1,
                color: (i % 4 == 0).then(|| format!("#{:06x}", rng.below(0xFF_FFFF))),
            },
        );
        group_names.push(name);
    }
    let set_names: Vec<String> = (0..20).map(|i| format!("Set {i:02}")).collect();
    for s in &set_names {
        b.sets.insert(
            s.clone(),
            SetDef {
                description: String::new(),
            },
        );
    }
    let location_names: Vec<String> = (0..10).map(|i| format!("Location {i}")).collect();
    for l in &location_names {
        b.locations.insert(
            l.clone(),
            Location {
                description: String::new(),
                city: "Sample City".into(),
                region: String::new(),
                country: String::new(),
            },
        );
    }

    // Accounts per category and rows per account: uneven, as in real budgets,
    // with a few big accounts (a few hundred rows) and many small ones.
    let categories = spec.categories.max(1);
    let accounts = spec.accounts.max(categories);
    let weights: Vec<u64> = (0..accounts)
        .map(|i| if i % 17 == 3 { 40 } else { 1 + rng.below(6) })
        .collect();
    let total_weight: u64 = weights.iter().sum();
    let mut sizes: Vec<usize> = weights
        .iter()
        .map(|w| (spec.rows as u64 * w / total_weight) as usize)
        .collect();
    let mut short = spec.rows.saturating_sub(sizes.iter().sum());
    let mut k = 0;
    while short > 0 {
        sizes[k % accounts] += 1;
        short -= 1;
        k += 1;
    }

    let mut id = 0u64;
    let mut topsheet = Vec::new();
    let mut ai = 0;
    for ci in 0..categories {
        let code = format!("{}", 10 + ci * 5);
        let n_accts = accounts / categories + usize::from(ci < accounts % categories);
        let mut accts = Vec::with_capacity(n_accts);
        for a in 0..n_accts {
            let size = sizes[ai];
            ai += 1;
            let mut details = Vec::with_capacity(size);
            let mut range: Option<(u32, usize)> = None; // (number, rows left)
            let mut next_range = 1;
            for r in 0..size {
                id += 1;
                let left_in_range = range.map_or(0, |(_, n)| n);
                let d = match rng.below(40) {
                    0 if r > 0 => Detail::subtotal(id, format!("Subtotal {r}")),
                    1 if r > 2 => {
                        Detail::adjustment(id, "Kit and box", format!("0.{:02}", rng.below(15) + 1))
                    }
                    2 if r > 0 => Detail::separator(id),
                    _ => {
                        let role = *rng.pick(&ROLES);
                        let unit = *rng.pick(&UNITS);
                        let amount = if rng.chance(45) {
                            (*rng.pick(&amounts)).to_string()
                        } else {
                            format!("{}", rng.below(30) + 1)
                        };
                        let rate = format!("{}.{:02}", rng.below(4_000) + 50, rng.below(100));
                        let mut d =
                            Detail::line(id, format!("{role} {}", r + 1), amount, unit, rate);
                        if rng.chance(25) {
                            d.x = Formula::from(format!("{}", rng.below(3) + 1));
                        }
                        if rng.chance(5) {
                            d.x = "OT_FACTOR".into();
                        }
                        if rng.chance(8) {
                            d.currency = Some((*rng.pick(&["CAD", "GBP", "EUR"])).to_string());
                        }
                        // Payroll fringes on most labor, plus a few others.
                        if !fringe_names.is_empty() && rng.chance(70) {
                            for f in fringe_names.iter().take(3) {
                                d.fringes.insert(f.clone());
                            }
                            for _ in 0..rng.below(3) {
                                d.fringes.insert(rng.pick(&fringe_names).clone());
                            }
                        }
                        d
                    }
                };
                let mut d = d;
                // Fringe ranges: runs of 2 to 4 rows for one person's prep,
                // shoot and wrap.
                if left_in_range > 0 {
                    let (n, left) = range.expect("in a range");
                    d.fringe_range = Some(n);
                    range = (left > 1).then_some((n, left - 1));
                } else if rng.chance(6) {
                    let len = 1 + rng.below(3) as usize;
                    d.fringe_range = Some(next_range);
                    range = Some((next_range, len));
                    next_range += 1;
                }
                if d.kind.is_costed() {
                    if !group_names.is_empty() && rng.chance(30) {
                        d.groups.insert(rng.pick(&group_names).clone());
                        if rng.chance(20) {
                            d.groups.insert(rng.pick(&group_names).clone());
                        }
                    }
                    if rng.chance(40) {
                        d.set = Some(rng.pick(&set_names).clone());
                    }
                    if rng.chance(25) {
                        d.location = Some(rng.pick(&location_names).clone());
                    }
                }
                d.bold = rng.chance(5);
                d.italic = rng.chance(3);
                d.indent = if rng.chance(10) {
                    rng.below(3) as u8 + 1
                } else {
                    0
                };
                if rng.chance(4) {
                    d.notes = vec![Note::plain(1, "Check the deal memo")];
                }
                details.push(d);
            }
            accts.push(Account {
                code: format!("{code}-{:02}", a + 1),
                name: format!("{} {}", CATEGORY_NAMES[ci % CATEGORY_NAMES.len()], a + 1),
                details,
                notes: Vec::new(),
            });
        }
        topsheet.push(TopsheetRow::Category(Category {
            code,
            name: CATEGORY_NAMES[ci % CATEGORY_NAMES.len()].into(),
            accounts: accts,
            notes: Vec::new(),
        }));
        if ci == 1 {
            topsheet.push(TopsheetRow::ProductionTotal(ProductionTotal {
                name: "Total above-the-line".into(),
            }));
        } else if ci + 1 == categories / 2 + 2 {
            topsheet.push(TopsheetRow::ProductionTotal(ProductionTotal {
                name: "Total production".into(),
            }));
        }
    }
    topsheet.push(TopsheetRow::ProductionTotal(ProductionTotal {
        name: "Total other".into(),
    }));
    b.topsheet = topsheet;

    let first_cat = b.categories().next().map(|c| c.code.clone());
    let some_account = b
        .categories()
        .nth(2)
        .and_then(|c| c.accounts.first())
        .map(|a| a.code.clone());
    let group = |i: usize| group_names.get(i).cloned();
    b.charges = vec![
        Charge {
            name: "Contingency".into(),
            account: None,
            applied: true,
            kind: ChargeKind::Percent(Dec::from_int(10)),
            excluded_amounts: vec![("Insurance".into(), 2_500_000)],
            excluded_groups: group(2).into_iter().collect(),
            excluded_categories: first_cat.into_iter().collect(),
            excluded_accounts: some_account.into_iter().collect(),
            label_percent: false,
            label_excluded: false,
        },
        Charge {
            name: "Completion bond".into(),
            account: None,
            applied: true,
            kind: ChargeKind::Percent(dec("2.5")),
            excluded_amounts: Vec::new(),
            excluded_groups: group(3).into_iter().collect(),
            excluded_categories: BTreeSet::new(),
            excluded_accounts: BTreeSet::new(),
            label_percent: false,
            label_excluded: false,
        },
        Charge {
            name: "Overhead fee".into(),
            account: None,
            applied: true,
            kind: ChargeKind::Flat {
                amount: 15_000_000,
                currency: Some("CAD".into()),
            },
            excluded_amounts: Vec::new(),
            excluded_groups: group(4).into_iter().collect(),
            excluded_categories: BTreeSet::new(),
            excluded_accounts: BTreeSet::new(),
            label_percent: false,
            label_excluded: false,
        },
    ];
    let mut credit_rows = vec![CreditRow {
        description: "Other qualifying spend".into(),
        source: CreditSource::Flat(50_000_000),
        percent: Dec::from_int(15),
        cap: CreditCap::Credit(5_000_000),
    }];
    let mut credit_excluded = BTreeSet::new();
    if let Some(g) = group(5) {
        credit_excluded = fringe_names.iter().take(1).cloned().collect();
        credit_rows.insert(
            0,
            CreditRow {
                description: "Local labor".into(),
                source: CreditSource::Group { group: g },
                percent: Dec::from_int(25),
                cap: CreditCap::QualifiedSpend(400_000_000),
            },
        );
    }
    b.credits = vec![Credit {
        name: "Sample tax credit".into(),
        account: None,
        applied: true,
        cap: Some(100_000_000),
        discount_percent: Dec::from_int(5),
        excluded_fringes: credit_excluded,
        locations: BTreeSet::new(),
        rows: credit_rows,
    }];

    if spec.baseline {
        if let Some(base) = recalc(&b, &RecalcOptions::default())
            .ok()
            .as_ref()
            .and_then(Baseline::capture)
        {
            b.baseline = Some(base);
        }
    }
    b
}

/// Invented people for [`persons`].
pub const PERSONS: [&str; 4] = ["Dana Reyes", "Lee Park", "Ari Moss", "Jo Quinn"];

/// Gives about a fifth of `b`'s lines and adjustment rows a person from
/// [`PERSONS`] (decisions.md, entry 79), so each person is paid in rows of
/// several accounts, some of them in fringe ranges. For tests of person
/// cutoffs; nothing else changes.
pub fn persons(b: &mut Budget, seed: u64) {
    let mut rng = Rng::new(seed ^ 0x5eed_9e75);
    for c in b.categories_mut() {
        for a in &mut c.accounts {
            for d in &mut a.details {
                if d.kind.is_costed() && rng.chance(20) {
                    d.person = Some((*rng.pick(&PERSONS)).into());
                }
            }
        }
    }
}

/// Rewrites the numbers of `b` (from [`budget`]) with up to nine decimal
/// places, many of them on a rounding edge: products a hair under, at or over
/// half a cent, such as 0.5 x 0.009999999. Lines, fringe rates, unit hours,
/// currency rates and credits all change; the shape of the budget does not.
/// For tests that money is rounded to cents once, and the same way everywhere.
pub fn fine_decimals(b: &mut Budget, seed: u64) {
    const AMOUNTS: [&str; 8] = [
        "0.5",
        "1.5",
        "2.5",
        "0.333333333",
        "3",
        "0.000000001",
        "12.75",
        "0.999999999",
    ];
    const RATES: [&str; 9] = [
        "0.009999999",
        "0.010000001",
        "0.01",
        "0.019999999",
        "19.999999999",
        "1234.567890123",
        "0.003333333",
        "99.990000001",
        "0.005",
    ];
    let mut rng = Rng::new(seed);
    for c in b.categories_mut() {
        for a in &mut c.accounts {
            for d in &mut a.details {
                if d.kind == crate::model::RowKind::Line {
                    if rng.chance(70) {
                        d.amount = (*rng.pick(&AMOUNTS)).into();
                    }
                    if rng.chance(30) {
                        d.x = (*rng.pick(&AMOUNTS)).into();
                    }
                    if rng.chance(70) {
                        d.rate = (*rng.pick(&RATES)).into();
                    }
                }
            }
        }
    }
    for f in b.fringes.values_mut() {
        f.rate = match f.kind {
            FringeKind::Percent { .. } => *rng.pick(&["18.123456789", "0.05", "7.649999999"]),
            FringeKind::Flat { .. } => *rng.pick(&["0.009999999", "12.345678901", "0.015"]),
        }
        .into();
    }
    for u in b.units.values_mut() {
        if u.hours.is_some() {
            u.hours = Some(*rng.pick(&[
                Dec::from_int(7),
                Dec::from_int(12),
                "10.333333333".parse().unwrap_or(Dec::ONE),
            ]));
        }
    }
    for cur in b.currencies.values_mut() {
        cur.rate = (*rng.pick(&["0.733333333", "1.000000005", "0.499999999"]))
            .parse()
            .unwrap_or(Dec::ONE);
    }
    for cr in &mut b.credits {
        cr.discount_percent = "4.999999999".parse().unwrap_or(Dec::ZERO);
        for row in &mut cr.rows {
            row.percent = (*rng.pick(&["25.000000005", "12.5", "33.333333333"]))
                .parse()
                .unwrap_or(Dec::ONE);
        }
    }
}

/// Gives the flat fringes of `b` (from [`budget`]) every basis, some a
/// most-hours-a-day cap (with some charged per Hour so it applies) and a
/// stored cutoff (which must change nothing), and gives lines X and 4X in
/// every kind of unit: hours, days, weeks, a non-time Episode, none. For
/// tests that the engine, the cache, the reference and Explain agree on
/// every way flat fringes count (decisions.md, entry 54).
pub fn flat_bases(b: &mut Budget, seed: u64) {
    let mut rng = Rng::new(seed ^ 0x5EED_F1A7);
    b.units.insert(
        "Episode".into(),
        UnitDef {
            plural: "Episodes".into(),
            description: String::new(),
            hours: None,
        },
    );
    let caps: [Option<&str>; 5] = [None, Some("8"), Some("10"), Some("12"), Some("14.5")];
    for f in b.fringes.values_mut() {
        if let FringeKind::Flat {
            unit,
            basis,
            max_hours_per_day,
            cutoff,
        } = &mut f.kind
        {
            *basis = *rng.pick(&crate::model::FlatBasis::ALL);
            *max_hours_per_day = rng.pick(&caps).map(dec);
            if unit.is_some() && rng.chance(50) {
                *unit = Some("Hour".into());
            }
            if rng.chance(20) {
                *cutoff = Some(rng.below(100_000) as i64 * 100);
            }
        }
    }
    const SECOND: [&str; 6] = ["", "Hour", "Day", "Week", "Episode", "Allow"];
    const FOURTH: [&str; 4] = ["", "Episode", "Week", "Hour"];
    for c in b.categories_mut() {
        for a in &mut c.accounts {
            for d in &mut a.details {
                if d.kind != crate::model::RowKind::Line {
                    continue;
                }
                if rng.chance(40) {
                    d.x = (*rng.pick(&["2", "3", "8", "14", "0.5"])).into();
                    d.unit2 = (*rng.pick(&SECOND)).into();
                }
                if rng.chance(30) {
                    d.x4 = (*rng.pick(&["2", "3", "1.5"])).into();
                    d.unit4 = (*rng.pick(&FOURTH)).into();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_sample_is_big_valid_and_deterministic() {
        let spec = SampleSpec::default();
        let b = budget(&spec);
        let rows: usize = b
            .categories()
            .flat_map(|c| &c.accounts)
            .map(|a| a.details.len())
            .sum();
        assert_eq!(rows, 5_000);
        assert_eq!(b.categories().count(), 8);
        assert_eq!(b.categories().map(|c| c.accounts.len()).sum::<usize>(), 80);
        assert_eq!(b.fringes.len(), 40);
        assert_eq!(b.groups.len(), 15);
        let r = recalc(&b, &RecalcOptions::default()).expect("the sample totals");
        assert!(r.grand_total > 0 && r.net_total > 0);
        assert!(b.baseline.is_some());
        let biggest = b
            .categories()
            .flat_map(|c| &c.accounts)
            .map(|a| a.details.len())
            .max()
            .unwrap();
        assert!(biggest >= 200, "a few accounts are big: {biggest}");
        assert_eq!(budget(&spec), b);
    }
}
