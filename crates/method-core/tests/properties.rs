//! Property tests over randomly generated budgets.
//!
//! Uses a small seeded generator instead of a property-testing crate, so the
//! engine stays dependency-free. Each failure prints its seed for replay.

mod common;

use common::*;
use method_core::*;

const CASES: u64 = 400;

/// xorshift64*: tiny, deterministic, good enough for test data.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn chance(&mut self, pct: u64) -> bool {
        self.below(100) < pct
    }

    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.below(items.len() as u64) as usize]
    }
}

fn gen_detail(rng: &mut Rng, id: RowId, range: Option<u32>) -> Detail {
    let mut det = match rng.below(11) {
        0 => return Detail::subtotal(id, "sub"),
        // Adjustment rows can carry fringes and sit in a range, as lines
        // can (decisions.md, entry 56).
        1 => {
            let mut adj = Detail::adjustment(id, "adj", format!("0.{}", rng.below(30)).as_str());
            for f in ["PCT", "CAPPED", "FLAT"] {
                if rng.chance(20) {
                    adj.fringes.insert(f.into());
                }
            }
            adj.fringe_range = range;
            adj
        }
        // Separators sometimes sit inside a fringe range, sometimes carry a
        // different range or stray tags: none of it may matter.
        2 => {
            let mut sep = Detail::separator(id);
            sep.fringe_range = match rng.below(3) {
                0 => range,
                1 => Some(u32::MAX),
                _ => None,
            };
            if rng.chance(30) {
                sep.fringes.insert("PCT".into());
            }
            sep
        }
        _ => {
            let amount = match rng.below(3) {
                0 => "DAYS".to_string(),
                1 => "WEEKS".to_string(),
                _ => rng.below(50).to_string(),
            };
            let rate = format!("{}.{:02}", rng.below(5_000), rng.below(100));
            let unit = rng.pick(&["Day", "Week", "Hour", "Flat", ""]);
            let mut det = Detail::line(id, "line", amount.as_str(), unit, rate.as_str());
            det.x = (rng.below(3) as i64 + 1).into();
            if rng.chance(20) {
                det.x4 = (rng.below(4) as i64 + 1).into();
            }
            det.currency = match rng.below(5) {
                0 => Some("CAD".into()),
                1 => Some("GBP".into()),
                _ => None,
            };
            for f in ["PCT", "CAPPED", "FLAT", "DAILY"] {
                if rng.chance(30) {
                    det.fringes.insert(f.into());
                }
            }
            det.fringe_range = range;
            det
        }
    };
    for g in ["A", "B", "OFF"] {
        if rng.chance(20) {
            det.groups.insert(g.into());
        }
    }
    if rng.chance(30) {
        det.set = Some(rng.pick(&["S1", "S2"]).into());
    }
    // Where it is shot (decisions.md, entry 80): L3 is not in the
    // budget's locations.
    if rng.chance(50) {
        det.location = Some(rng.pick(&["L1", "L2", "L3"]).into());
    }
    // Display-only styling, on any kind of row.
    det.bold = rng.chance(20);
    det.italic = rng.chance(20);
    det.indent = rng.below(5) as u8;
    if rng.chance(20) {
        det.color = Some("#c04000".into());
    }
    if rng.chance(10) {
        det.notes = vec![Note::plain(1, "check with UPM")];
    }
    det
}

fn gen_budget(rng: &mut Rng) -> Budget {
    let mut b = Budget::new("random", "USD");
    for (code, rate) in [("CAD", "0.73"), ("GBP", "1.27")] {
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
    b.globals
        .insert("DAYS".into(), (rng.below(60) as i64 + 1).into());
    b.globals.insert("WEEKS".into(), "DAYS / 5".into());
    let fringe = |rate: &str, kind| Fringe {
        short_code: None,
        description: String::new(),
        rate: rate.into(),
        kind,
    };
    b.fringes.insert(
        "PCT".into(),
        fringe("18.5", FringeKind::Percent { cutoff: None }),
    );
    b.fringes.insert(
        "CAPPED".into(),
        fringe(
            "7.65",
            FringeKind::Percent {
                cutoff: Some(rng.below(3_000_000) as i64),
            },
        ),
    );
    b.fringes
        .insert("FLAT".into(), fringe("25", FringeKind::flat(None)));
    b.fringes.insert(
        "DAILY".into(),
        fringe("12.5", FringeKind::flat(Some("Day".into()))),
    );
    b.fringe_posting = match rng.below(4) {
        0 => FringePosting::Budget,
        1 => FringePosting::ProductionLevel,
        2 => FringePosting::Account,
        _ => FringePosting::Category {
            suffix: "99".into(),
        },
    };
    if rng.chance(30) {
        b.trailing_level_fringes = TrailingFringes::Drop;
    }
    if rng.chance(30) {
        b.cutoff_factor = CutoffFactor::ALL[rng.below(5) as usize];
    }
    for l in ["L1", "L2"] {
        b.locations.insert(l.into(), Location::default());
    }
    b.groups.insert("A".into(), group(true));
    b.groups.insert("B".into(), group(true));
    b.groups.insert("OFF".into(), group(false));
    if rng.chance(50) {
        b.groups.get_mut("A").unwrap().color = Some("#2060a0".into());
    }

    let (mut id, mut code, mut range) = (0, 1000, 0);
    for c in 0..=rng.below(5) {
        let mut accounts = Vec::new();
        for _ in 0..=rng.below(4) {
            code += 10;
            let mut details = Vec::new();
            let mut current = None;
            for _ in 0..rng.below(8) {
                // Ranges are contiguous blocks, each with its own number.
                if rng.chance(25) {
                    range += 1;
                    current = if rng.chance(50) { Some(range) } else { None };
                }
                id += 1;
                details.push(gen_detail(rng, id, current));
            }
            accounts.push(account(&code.to_string(), "acct", details));
        }
        b.topsheet
            .push(category(&format!("{c}000"), "cat", accounts));
        if rng.chance(50) {
            b.topsheet.push(level("level"));
        }
    }

    let mut charge = percent_charge("Contingency", "10");
    charge.excluded_groups = set(&["B"]);
    charge.excluded_accounts = set(&["1010"]);
    b.charges.push(charge);
    b.charges.push(Charge {
        kind: ChargeKind::Flat {
            amount: 250_000,
            currency: Some("GBP".into()),
        },
        ..percent_charge("Fee", "0")
    });
    b.credits.push(Credit {
        name: "Credit".into(),
        account: None,
        applied: true,
        cap: Some(rng.below(2_000_000) as i64),
        discount_percent: d("5"),
        excluded_fringes: set(&["FLAT"]),
        locations: match rng.below(3) {
            0 => set(&["L1"]),
            1 => set(&["L1", "L2"]),
            _ => Default::default(),
        },
        minimum_spend: None,
        broker_fee: 0,
        shared_cap: None,
        rows: vec![
            CreditRow {
                description: String::new(),
                source: CreditSource::Group { group: "A".into() },
                percent: d("25"),
                cap: CreditCap::QualifiedSpend(rng.below(5_000_000) as i64),
            },
            CreditRow {
                description: String::new(),
                source: CreditSource::Category {
                    category: "0000".into(),
                },
                percent: d("10"),
                cap: CreditCap::Credit(rng.below(1_000_000) as i64),
            },
        ],
    });
    // Both arithmetics, so every property holds in each.
    if rng.below(2) == 0 {
        b.arithmetic = Arithmetic::Exact;
    }
    // Sometimes Movie Magic's other group rules (decisions.md, entry 57).
    if rng.chance(20) {
        b.group_rules.ungrouped_lines = Inclusion::Exclude;
    }
    if rng.chance(20) {
        b.group_rules.conflict = Inclusion::Include;
    }
    b
}

fn check_rollups(b: &Budget, r: &Report, seed: u64) {
    let msg = format!("seed {seed}");
    let posting = &b.fringe_posting;
    let mut all_fringes = 0;
    let mut posted = r.budget_fringes;
    for cat in &r.categories {
        for acct in &cat.accounts {
            for row in &acct.rows {
                assert_eq!(row.fringe_total, row.fringes.values().sum::<i64>(), "{msg}");
                if !row.included || !row.kind.is_costed() {
                    assert_eq!(row.fringe_total, 0, "{msg}");
                }
                if row.kind == RowKind::Separator {
                    assert!(row.included, "{msg}");
                    assert_eq!((row.total, row.subtotal), (0, None), "{msg}");
                }
                if !row.included {
                    assert_eq!(row.total, 0, "{msg}");
                }
                all_fringes += row.fringe_total;
            }
            assert_eq!(
                acct.details_total,
                acct.rows.iter().map(|r| r.total).sum::<i64>(),
                "{msg}"
            );
            assert_eq!(
                acct.fringe_total,
                acct.rows.iter().map(|r| r.fringe_total).sum::<i64>(),
                "{msg}"
            );
            let expect = match posting {
                FringePosting::Account => acct.details_total + acct.fringe_total,
                _ => acct.details_total,
            };
            assert_eq!(acct.total, expect, "{msg}");
            if *posting == FringePosting::Account {
                posted += acct.fringe_total;
            }
        }
        let fa = cat.fringe_account.as_ref().map_or(0, |f| f.1);
        posted += fa;
        assert_eq!(
            cat.total,
            cat.accounts.iter().map(|a| a.total).sum::<i64>() + fa,
            "{msg}"
        );
        assert_eq!(
            cat.fringe_total,
            cat.accounts.iter().map(|a| a.fringe_total).sum::<i64>(),
            "{msg}"
        );
    }
    // Where the unnamed level after the last production total starts, and
    // how many levels a production total closes.
    let closed = b
        .topsheet
        .iter()
        .filter(|row| matches!(row, TopsheetRow::ProductionTotal(_)))
        .count();
    let trailing = r.levels.get(closed).map_or(r.categories.len(), |l| l.start);
    let dropping = *posting == FringePosting::ProductionLevel
        && b.trailing_level_fringes == TrailingFringes::Drop;
    let dropped: i64 = if dropping {
        r.categories[trailing..]
            .iter()
            .map(|c| c.fringe_total)
            .sum()
    } else {
        0
    };
    for l in &r.levels {
        posted += l.posted_fringes;
        let cats: i64 = r.categories[l.start..l.end].iter().map(|c| c.total).sum();
        assert_eq!(l.total, cats + l.posted_fringes, "{msg}");
    }
    assert_eq!(
        posted + dropped,
        all_fringes,
        "every fringe dollar posts exactly once, unless it is dropped, {msg}"
    );
    assert_eq!(
        r.fringes.values().map(|f| f.total).sum::<i64>(),
        all_fringes,
        "{msg}"
    );
    // ATL and BTL are the levels a production total closes; the trailing
    // level is in neither, but in the grand total (decisions.md, entry 56).
    assert_eq!(
        r.total_atl + r.total_btl,
        r.levels[..closed].iter().map(|l| l.total).sum::<i64>(),
        "{msg}"
    );
    assert_eq!(r.atl_btl, r.total_atl + r.total_btl, "{msg}");
    let charges: i64 = r.charges.iter().map(|c| c.amount).sum();
    let levels: i64 = r.levels.iter().map(|l| l.total).sum();
    assert_eq!(r.grand_total, levels + r.budget_fringes + charges, "{msg}");
    assert_eq!(
        r.credits_total,
        r.credits.iter().map(|c| c.amount).sum::<i64>(),
        "{msg}"
    );
    assert_eq!(r.net_total, r.grand_total - r.credits_total, "{msg}");
}

#[test]
fn rollups_always_agree() {
    for seed in 1..=CASES {
        let b = gen_budget(&mut Rng(seed));
        for opts in [
            RecalcOptions::default(),
            RecalcOptions {
                exclude_groups: set(&["A"]),
                ..Default::default()
            },
            RecalcOptions {
                only_groups: Some(set(&["B"])),
                ..Default::default()
            },
        ] {
            let r = recalc(&b, &opts).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
            check_rollups(&b, &r, seed);
        }
    }
}

#[test]
fn excluding_a_group_equals_deleting_its_lines() {
    for seed in 1..=CASES {
        let b = gen_budget(&mut Rng(seed));
        let opts = RecalcOptions {
            exclude_groups: set(&["A"]),
            ..Default::default()
        };
        let filtered = recalc(&b, &opts).unwrap();

        let mut pruned = b.clone();
        for acct in pruned.categories_mut().flat_map(|c| &mut c.accounts) {
            acct.details
                .retain(|d| d.kind == RowKind::Subtotal || !d.groups.contains("A"));
        }
        let direct = recalc(&pruned, &RecalcOptions::default()).unwrap();

        assert_eq!(filtered.atl_btl, direct.atl_btl, "seed {seed}");
        assert_eq!(filtered.grand_total, direct.grand_total, "seed {seed}");
        assert_eq!(filtered.charges, direct.charges, "seed {seed}");
        assert_eq!(
            filtered.fringes,
            direct
                .fringes
                .iter()
                .map(|(k, v)| {
                    // Uses count every tagged line, filtered or not.
                    (
                        k.clone(),
                        FringeSummary {
                            uses: filtered.fringes[k].uses,
                            total: v.total,
                        },
                    )
                })
                .collect(),
            "seed {seed}"
        );
    }
}

#[test]
fn suppressed_group_equals_excluding_it() {
    for seed in 1..=CASES {
        let mut b = gen_budget(&mut Rng(seed));
        // Excluding a group leaves out a line it shares with a counted one;
        // suppressing it does only while conflicts are excluded.
        b.group_rules.conflict = Inclusion::Exclude;
        b.groups.insert("OFF".into(), group(true));
        let opts = RecalcOptions {
            exclude_groups: set(&["OFF"]),
            ..Default::default()
        };
        let excluded = recalc(&b, &opts).unwrap();
        b.groups.insert("OFF".into(), group(false));
        let suppressed = recalc(&b, &RecalcOptions::default()).unwrap();
        assert_eq!(excluded.categories, suppressed.categories, "seed {seed}");
        assert_eq!(excluded.net_total, suppressed.net_total, "seed {seed}");
    }
}

/// Leaving out lines with no group (decisions.md, entry 57) gives the same
/// totals as deleting them and the adjustment rows with no group.
#[test]
fn excluding_lines_with_no_group_equals_deleting_them() {
    for seed in 1..=CASES {
        let mut b = gen_budget(&mut Rng(seed));
        b.group_rules.ungrouped_lines = Inclusion::Exclude;
        let ruled = recalc(&b, &RecalcOptions::default()).unwrap();
        check_rollups(&b, &ruled, seed);

        let mut pruned = b.clone();
        pruned.group_rules.ungrouped_lines = Inclusion::Include;
        for acct in pruned.categories_mut().flat_map(|c| &mut c.accounts) {
            acct.details
                .retain(|d| !d.kind.is_costed() || !d.groups.is_empty());
        }
        let direct = recalc(&pruned, &RecalcOptions::default()).unwrap();
        assert_eq!(ruled.atl_btl, direct.atl_btl, "seed {seed}");
        assert_eq!(ruled.grand_total, direct.grand_total, "seed {seed}");
        assert_eq!(ruled.charges, direct.charges, "seed {seed}");
        assert_eq!(ruled.net_total, direct.net_total, "seed {seed}");
    }
}

/// Counting conflicts (decisions.md, entry 57) gives the same totals as
/// taking the uncounted group off every line that also has a counted one.
#[test]
fn including_conflicts_equals_untagging_the_uncounted_group() {
    for seed in 1..=CASES {
        let mut b = gen_budget(&mut Rng(seed));
        b.group_rules.conflict = Inclusion::Include;
        let ruled = recalc(&b, &RecalcOptions::default()).unwrap();
        check_rollups(&b, &ruled, seed);

        let mut untagged = b.clone();
        untagged.group_rules.conflict = Inclusion::Exclude;
        for d in untagged
            .categories_mut()
            .flat_map(|c| &mut c.accounts)
            .flat_map(|a| &mut a.details)
        {
            if d.groups.len() > 1 && d.groups.iter().any(|g| g != "OFF") {
                d.groups.remove("OFF");
            }
        }
        let direct = recalc(&untagged, &RecalcOptions::default()).unwrap();
        assert_eq!(ruled.categories, direct.categories, "seed {seed}");
        assert_eq!(ruled.net_total, direct.net_total, "seed {seed}");
    }
}

#[test]
fn full_group_totals_ignore_filters() {
    for seed in 1..=CASES {
        let b = gen_budget(&mut Rng(seed));
        let plain = recalc(&b, &RecalcOptions::default()).unwrap();
        let filtered = recalc(
            &b,
            &RecalcOptions {
                exclude_groups: set(&["A", "B"]),
                ..Default::default()
            },
        )
        .unwrap();
        for g in ["A", "B", "OFF"] {
            assert_eq!(plain.groups[g].full, filtered.groups[g].full, "seed {seed}");
        }
    }
}

#[test]
fn recalc_is_byte_identical() {
    for seed in 1..=CASES {
        let b = gen_budget(&mut Rng(seed));
        let a = format!("{:?}", recalc(&b, &RecalcOptions::default()));
        let c = format!("{:?}", recalc(&b.clone(), &RecalcOptions::default()));
        assert_eq!(a, c, "seed {seed}");
    }
}

#[test]
fn deleting_any_global_keeps_every_total() {
    for seed in 1..=CASES {
        let b = gen_budget(&mut Rng(seed));
        let before = recalc(&b, &RecalcOptions::default()).unwrap();
        for name in ["DAYS", "WEEKS"] {
            let mut edited = b.clone();
            delete_global(&mut edited, name).unwrap();
            let after = recalc(&edited, &RecalcOptions::default())
                .unwrap_or_else(|e| panic!("seed {seed} {name}: {e}"));
            assert_eq!(after.categories, before.categories, "seed {seed} {name}");
            assert_eq!(after.net_total, before.net_total, "seed {seed} {name}");
        }
    }
}

/// A copy of `r` with separator rows dropped from every account.
fn without_separator_rows(mut r: Report) -> Report {
    for cat in &mut r.categories {
        for acct in &mut cat.accounts {
            acct.rows.retain(|row| row.kind != RowKind::Separator);
        }
    }
    r
}

#[test]
fn separators_and_styling_change_nothing() {
    let mut separators = 0;
    for seed in 1..=CASES {
        let b = gen_budget(&mut Rng(seed));
        let mut plain = b.clone();
        for g in plain.groups.values_mut() {
            g.color = None;
        }
        for acct in plain.categories_mut().flat_map(|c| &mut c.accounts) {
            separators += acct.details.len();
            acct.details.retain(|d| d.kind != RowKind::Separator);
            separators -= acct.details.len();
            for d in &mut acct.details {
                d.bold = false;
                d.italic = false;
                d.indent = 0;
                d.color = None;
                d.notes.clear();
            }
        }
        for opts in [
            RecalcOptions::default(),
            RecalcOptions {
                exclude_groups: set(&["A"]),
                ..Default::default()
            },
        ] {
            let with = recalc(&b, &opts).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
            let without = recalc(&plain, &opts).unwrap();
            assert_eq!(without_separator_rows(with), without, "seed {seed}");
        }
    }
    assert!(separators > 100, "the generator makes separators");
}

/// Every line of a comparison, with where it is.
fn compare_lines(c: &Comparison) -> Vec<(String, &CompareLine)> {
    let mut out = Vec::new();
    for cat in &c.categories {
        out.push((format!("category {}", cat.code), &cat.line));
        for a in &cat.accounts {
            out.push((format!("account {}", a.code), &a.line));
        }
    }
    let t = &c.totals;
    for (name, line) in [
        ("atl", &t.total_atl),
        ("btl", &t.total_btl),
        ("fringes", &t.fringes),
        ("charges", &t.charges),
        ("grand", &t.grand_total),
        ("credits", &t.credits),
        ("net", &t.net_total),
    ] {
        out.push((name.to_string(), line));
    }
    out
}

#[test]
fn comparing_a_budget_with_itself_shows_no_differences() {
    for seed in 1..=CASES {
        let b = gen_budget(&mut Rng(seed));
        // The second copy spells every code in its normalized form, which
        // must not matter.
        let mut spelled = b.clone();
        let coding = spelled.account_coding;
        for cat in spelled.categories_mut() {
            cat.code = coding.normalize(&cat.code).unwrap();
            for a in &mut cat.accounts {
                a.code = coding.normalize(&a.code).unwrap();
            }
        }
        // Charge exclusions name codes by their exact text, so they follow.
        for ch in &mut spelled.charges {
            for codes in [&mut ch.excluded_accounts, &mut ch.excluded_categories] {
                *codes = codes.iter().map(|c| coding.normalize(c).unwrap()).collect();
            }
        }
        // So do credit rows on a category or an account.
        for r in spelled.credits.iter_mut().flat_map(|c| &mut c.rows) {
            match &mut r.source {
                CreditSource::Category { category: code }
                | CreditSource::Account { account: code } => {
                    *code = coding.normalize(code).unwrap();
                }
                _ => {}
            }
        }
        let c = compare(&[("a".to_string(), &b), ("b".to_string(), &spelled)])
            .unwrap_or_else(|e| panic!("seed {seed}: {e}"));
        let r = recalc(&b, &RecalcOptions::default()).unwrap();
        assert_eq!(c.categories.len(), r.categories.len(), "seed {seed}");
        for (at, line) in compare_lines(&c) {
            assert_eq!(line.values[0], line.values[1], "seed {seed} {at}");
            assert!(line.values[0].is_some(), "seed {seed} {at}");
            for d in &line.diffs {
                assert_eq!(d.amount, 0, "seed {seed} {at}");
                let zero_base = line.values[0] == Some(0);
                assert_eq!(
                    d.percent,
                    (!zero_base).then_some(Dec::ZERO),
                    "seed {seed} {at}"
                );
            }
        }
    }
}

/// Random edits: change rates, delete rows, add rows and accounts, drop an account.
fn edit_randomly(b: &mut Budget, rng: &mut Rng) {
    let mut next_id = b.highest_row_id().unwrap_or(0) + 1;
    for acct in b.categories_mut().flat_map(|c| &mut c.accounts) {
        for det in &mut acct.details {
            if det.kind == RowKind::Line && rng.chance(30) {
                det.rate = format!("{}", rng.below(3_000)).into();
            }
        }
        acct.details.retain(|_| !rng.chance(10));
        if rng.chance(20) {
            acct.details
                .push(Detail::line(next_id, "new", 2, "Day", "125.50"));
            next_id += 1;
        }
    }
    let n = b.categories().count() as u64;
    if rng.chance(30) {
        let cat = b.categories_mut().nth(rng.below(n) as usize).unwrap();
        if !cat.accounts.is_empty() {
            cat.accounts.remove(0);
        }
    }
    if rng.chance(30) {
        let cat = b.categories_mut().nth(rng.below(n) as usize).unwrap();
        cat.accounts.push(account(
            "9990",
            "added",
            vec![Detail::line(next_id, "added", 1, "Flat", 777)],
        ));
    }
}

#[test]
fn version_vs_current_equals_variance() {
    let (mut changed, mut missing) = (0, 0);
    for seed in 1..=CASES {
        let mut rng = Rng(seed);
        let mut b = gen_budget(&mut rng);
        let v = save_version(&mut b, "before", "2026-10-09T10:00:00Z", None);
        edit_randomly(&mut b, &mut rng);
        let old = b.version(v).unwrap().budget.to_budget();
        let opts = RecalcOptions::default();
        let (r_old, r_now) = (recalc(&old, &opts).unwrap(), recalc(&b, &opts).unwrap());
        let var = variance(&Baseline::capture(&r_old).unwrap(), &r_now).unwrap();

        let c = compare(&[("old".to_string(), &old), ("now".to_string(), &b)]).unwrap();
        let mut seen = 0;
        for cat in &c.categories {
            let ch = var.categories[&cat.code];
            assert_eq!(
                cat.line.values,
                vec![ch.original, ch.current],
                "seed {seed}"
            );
            assert_eq!(Some(cat.line.diffs[1].amount), ch.variance(), "seed {seed}");
            for a in cat.accounts.iter().filter(|a| !a.fringe_account) {
                let ch = var.accounts[&a.code];
                assert_eq!(a.line.values, vec![ch.original, ch.current], "seed {seed}");
                assert_eq!(Some(a.line.diffs[1].amount), ch.variance(), "seed {seed}");
                seen += 1;
                changed += usize::from(a.line.diffs[1].amount != 0);
                missing += a.line.values.iter().filter(|v| v.is_none()).count();

                // Rows of a version match the current rows by id.
                let rows = compare_account(
                    &[("old".to_string(), &old), ("now".to_string(), &b)],
                    &a.code,
                    RowMatch::Id,
                )
                .unwrap();
                for row in &rows.rows {
                    let ch = var.rows[&row.id];
                    assert_eq!(
                        row.line.values,
                        vec![ch.original, ch.current],
                        "seed {seed}"
                    );
                }
            }
        }
        // Accounts with the generator's unique codes, plus "9990" which may
        // be added to one category only: every variance account is listed.
        assert_eq!(seen, var.accounts.len(), "seed {seed}");
        assert_eq!(
            Some(c.totals.grand_total.diffs[1].amount),
            var.grand_total.variance(),
            "seed {seed}"
        );
        assert_eq!(
            Some(c.totals.net_total.diffs[1].amount),
            var.net_total.variance(),
            "seed {seed}"
        );
    }
    assert!(
        changed > 200 && missing > 50,
        "edits change totals and accounts"
    );
}

#[test]
fn restoring_a_version_brings_back_its_totals() {
    for seed in 1..=CASES / 4 {
        let mut rng = Rng(seed);
        let mut b = gen_budget(&mut rng);
        let original = recalc(&b, &RecalcOptions::default()).unwrap();
        let v = save_version(&mut b, "v", "t", None);
        edit_randomly(&mut b, &mut rng);
        let edited = b.snapshot();
        let auto = restore_version(&mut b, v, "t2").unwrap();
        assert_eq!(
            recalc(&b, &RecalcOptions::default()).unwrap(),
            original,
            "seed {seed}"
        );
        restore_version(&mut b, auto, "t3").unwrap();
        assert_eq!(b.snapshot(), edited, "seed {seed}");
        assert_eq!(b.versions.len(), 3, "seed {seed}");
    }
}

/// Every amount in a report, in order, with no names: what a rename keeps.
fn amounts(r: &Report) -> Vec<Cents> {
    let mut out = vec![
        r.grand_total,
        r.net_total,
        r.total_atl,
        r.total_btl,
        r.budget_fringes,
        r.credits_total,
    ];
    for c in &r.categories {
        out.extend([c.details_total, c.fringe_total, c.total]);
        for a in &c.accounts {
            out.extend([a.details_total, a.fringe_total, a.total]);
            for row in &a.rows {
                out.extend([row.local_total, row.total, row.fringe_total]);
                // Keyed by fringe name: the order may change with a name.
                let mut f: Vec<Cents> = row.fringes.values().copied().collect();
                f.sort_unstable();
                out.extend(f);
            }
        }
    }
    out.extend(r.charges.iter().map(|c| c.amount));
    out.extend(r.credits.iter().map(|c| c.amount));
    out
}

#[test]
fn renaming_any_setup_item_keeps_every_total_and_renaming_back_undoes_it() {
    use method_core::rename::references;
    let renames = [
        (SetupKind::Global, "DAYS", "PREP_DAYS"),
        (SetupKind::Global, "WEEKS", "W"),
        (SetupKind::Fringe, "PCT", "Pension"),
        (SetupKind::Fringe, "DAILY", "Per diem"),
        (SetupKind::Group, "A", "Crew"),
        (SetupKind::Group, "OFF", "Cut"),
        (SetupKind::Unit, "Day", "Shoot day"),
        (SetupKind::Set, "S1", "Set one"),
        (SetupKind::Location, "L1", "Ridge"),
        (SetupKind::Currency, "GBP", "UKP"),
        (SetupKind::Currency, "USD", "US$"),
    ];
    for seed in 1..=CASES {
        let mut b = gen_budget(&mut Rng(seed));
        b.sets.insert("S1".into(), SetDef::default());
        // Display info on a global, naming a unit: it follows both renames.
        if seed % 2 == 0 {
            b.global_info.insert(
                "DAYS".into(),
                GlobalInfo {
                    unit: "Day".into(),
                    decimals: Some(1),
                    ..GlobalInfo::default()
                },
            );
        }
        // A tag preset, and a line block of the first account's rows with
        // copies of the setup they use: both follow a rename.
        b.tag_presets.push(TagPreset {
            name: "tags".into(),
            fringes: set(&["PCT", "DAILY"]),
            groups: set(&["A"]),
            set: Some("S1".into()),
            location: None,
            currency: Some("GBP".into()),
        });
        let rows: Vec<Detail> = b
            .categories()
            .flat_map(|c| &c.accounts)
            .flat_map(|a| &a.details)
            .take(6)
            .cloned()
            .collect();
        let setup = SetupItems {
            globals: b.globals.clone(),
            fringes: b.fringes.clone(),
            groups: b.groups.clone(),
            units: b.units.clone(),
            sets: b.sets.clone(),
            currencies: b.currencies.clone(),
            ..SetupItems::default()
        };
        b.line_presets.push(LinePreset {
            name: "block".into(),
            rows,
            setup,
        });
        let before = recalc(&b, &RecalcOptions::default()).unwrap();
        for (kind, from, to) in renames {
            let mut e = b.clone();
            let n = rename(&mut e, kind, from, to)
                .unwrap_or_else(|err| panic!("seed {seed} {kind:?} {from}: {err}"));
            assert_eq!(n, references(&b, kind, from), "seed {seed} {kind:?}");
            let after = recalc(&e, &RecalcOptions::default())
                .unwrap_or_else(|err| panic!("seed {seed} {kind:?} {from}: {err}"));
            assert_eq!(amounts(&after), amounts(&before), "seed {seed} {kind:?}");
            if kind == SetupKind::Group {
                assert_eq!(after.groups[to], before.groups[from], "seed {seed}");
            }
            assert_eq!(
                rename(&mut e, kind, to, from),
                Ok(n),
                "seed {seed} {kind:?}"
            );
            assert_eq!(e, b, "seed {seed} {kind:?} back");
        }
    }
}

// ---------------------------------------------------------------------------
// Incentive programs (roadmap 3.6; decisions.md, entry 80)

/// A random program: rows on groups, categories, accounts and typed
/// amounts, random caps and a discount from 0 to 100%, maybe locations, a
/// minimum spend and a broker's fee (no shared cap: see
/// [`gen_shared_caps`]).
fn gen_program(rng: &mut Rng, b: &Budget, name: &str) -> Credit {
    let cats: Vec<String> = b.categories().map(|c| c.code.clone()).collect();
    let accts: Vec<String> = b
        .categories()
        .flat_map(|c| &c.accounts)
        .map(|a| a.code.clone())
        .collect();
    let rows = (0..=rng.below(3))
        .map(|_| CreditRow {
            description: String::new(),
            source: match rng.below(4) {
                0 => CreditSource::Flat(rng.below(3_000_000) as i64),
                1 => CreditSource::Group {
                    group: rng.pick(&["A", "B", "OFF"]).into(),
                },
                2 => CreditSource::Category {
                    category: cats[rng.below(cats.len() as u64) as usize].clone(),
                },
                _ => CreditSource::Account {
                    account: accts[rng.below(accts.len() as u64) as usize].clone(),
                },
            },
            percent: d(&format!("{}.{}", rng.below(40), rng.below(10))),
            cap: match rng.below(3) {
                0 => CreditCap::None,
                1 => CreditCap::QualifiedSpend(rng.below(4_000_000) as i64),
                _ => CreditCap::Credit(rng.below(800_000) as i64),
            },
        })
        .collect();
    Credit {
        name: name.into(),
        account: None,
        applied: rng.chance(50),
        cap: rng.chance(60).then(|| rng.below(1_500_000) as i64),
        discount_percent: d(&format!("{}.{}", rng.below(101), rng.below(10)))
            .min(Dec::from_int(100)),
        excluded_fringes: if rng.chance(30) {
            set(&["PCT"])
        } else {
            Default::default()
        },
        locations: match rng.below(4) {
            0 => set(&["L1"]),
            1 => set(&["L2", "L3"]),
            2 => set(&["L1", "L2", "L3"]),
            _ => Default::default(),
        },
        minimum_spend: rng.chance(30).then(|| rng.below(4_000_000) as i64),
        broker_fee: if rng.chance(30) {
            rng.below(200_000) as i64
        } else {
            0
        },
        shared_cap: None,
        rows,
    }
}

fn estimates(b: &Budget, seed: u64) -> incentives::Incentives {
    let r = recalc(b, &RecalcOptions::default()).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
    incentives::incentives(b, &r).unwrap_or_else(|e| panic!("seed {seed}: {e}"))
}

/// With no programs nothing changes: no credit, net equals grand, no
/// estimate. Programs that aren't applied change no figure either, and a
/// program's estimate is the same applied or not.
#[test]
fn no_programs_means_no_change() {
    for seed in 1..=CASES {
        let mut rng = Rng(seed);
        let mut b = gen_budget(&mut rng);
        let mut none = b.clone();
        none.credits.clear();
        let r0 = recalc(&none, &RecalcOptions::default()).unwrap();
        assert_eq!(r0.credits_total, 0, "seed {seed}");
        assert_eq!(r0.net_total, r0.grand_total, "seed {seed}");
        assert!(estimates(&none, seed).programs.is_empty(), "seed {seed}");

        for i in 0..3 {
            let p = gen_program(&mut rng, &b, &format!("P{i}"));
            b.credits.push(p);
        }
        let mut off = b.clone();
        for c in &mut off.credits {
            c.applied = false;
        }
        let r_off = recalc(&off, &RecalcOptions::default()).unwrap();
        assert_eq!(r_off, r0, "seed {seed}: unapplied programs change nothing");

        let on = estimates(&b, seed);
        let unapplied = estimates(&off, seed);
        assert_eq!(on.locations, unapplied.locations, "seed {seed}");
        for (a, u) in on.programs.iter().zip(&unapplied.programs) {
            assert_eq!(a.figures, u.figures, "seed {seed} {}", a.name);
        }
        // An applied program's estimate is what the topsheet takes off.
        let r = recalc(&b, &RecalcOptions::default()).unwrap();
        let applied: Vec<Cents> = on
            .programs
            .iter()
            .filter(|p| p.applied)
            .map(|p| p.amount().unwrap_or(-1))
            .collect();
        let taken: Vec<Cents> = r.credits.iter().map(|c| c.amount).collect();
        assert_eq!(applied, taken, "seed {seed}");
    }
}

/// No row's credit is over its cap on the credit, no row is a percent of
/// more than its cap on the spend, no program is over its own cap, and the
/// estimate (after the discount) is never more than the program's cap.
#[test]
fn caps_are_never_exceeded() {
    for seed in 1..=CASES {
        let mut rng = Rng(seed);
        let mut b = gen_budget(&mut rng);
        b.credits.clear();
        for i in 0..4 {
            let p = gen_program(&mut rng, &b, &format!("P{i}"));
            b.credits.push(p);
        }
        let got = estimates(&b, seed);
        for (c, p) in b.credits.iter().zip(&got.programs) {
            let f = p
                .figures
                .as_ref()
                .unwrap_or_else(|| panic!("seed {seed}: {:?}", p.problem));
            for (row, t) in c.rows.iter().zip(&f.rows) {
                match row.cap {
                    CreditCap::None => {}
                    CreditCap::Credit(cap) => {
                        assert!(
                            t.credit <= Dec::from_cents(cap),
                            "seed {seed}: row credit over its cap"
                        );
                    }
                    CreditCap::QualifiedSpend(cap) => {
                        let most = Dec::from_cents(cap)
                            .checked_mul(row.percent)
                            .and_then(|v| v.checked_div(Dec::from_int(100)))
                            .unwrap();
                        // Spend is never negative here, so the credit is at
                        // most the percent of the cap (to the 9th place).
                        assert!(
                            t.credit <= most.checked_add("0.000000001".parse().unwrap()).unwrap(),
                            "seed {seed}: row counts more spend than its cap"
                        );
                    }
                }
            }
            if let Some(cap) = c.cap {
                assert!(
                    f.capped <= Dec::from_cents(cap),
                    "seed {seed}: over the program cap"
                );
                assert!(
                    f.amount <= cap,
                    "seed {seed}: estimate over the program cap"
                );
            }
            assert!(f.capped <= p.rows_total, "seed {seed}");
            assert_eq!(
                p.capped,
                !f.below_minimum && f.capped < p.rows_total,
                "seed {seed}"
            );
            // Below the minimum the program pays nothing; at or above it,
            // the minimum changes nothing.
            assert_eq!(
                f.below_minimum,
                c.minimum_spend.is_some_and(|m| p.qualified < m),
                "seed {seed}"
            );
            if f.below_minimum {
                assert_eq!(f.amount, 0, "seed {seed}");
            }
            // The fee is never more than the credit had.
            assert!(f.fee >= 0 && f.fee <= c.broker_fee, "seed {seed}");
            assert!(f.amount >= 0, "seed {seed}");
        }
    }
}

/// Every figure is a sum over lines: the order of the programs changes no
/// program's estimate; a program at two locations counts what it counts at
/// each of them; and its spend by location adds up to its rows' spend.
#[test]
fn programs_are_order_independent_and_add_up_by_location() {
    for seed in 1..=CASES {
        let mut rng = Rng(seed);
        let mut b = gen_budget(&mut rng);
        b.credits.clear();
        for i in 0..4 {
            let p = gen_program(&mut rng, &b, &format!("P{i}"));
            b.credits.push(p);
        }
        let got = estimates(&b, seed);
        let mut reversed = b.clone();
        reversed.credits.reverse();
        let back = estimates(&reversed, seed);
        for p in &got.programs {
            let q = back.programs.iter().find(|q| q.name == p.name).unwrap();
            assert_eq!(p.figures, q.figures, "seed {seed} {}", p.name);
            assert_eq!(p.by_location, q.by_location, "seed {seed} {}", p.name);
        }

        for (c, p) in b.credits.iter().zip(&got.programs) {
            let f = p.figures.as_ref().unwrap();
            let typed: Cents = c
                .rows
                .iter()
                .zip(&f.rows)
                .filter(|(r, _)| !r.source.has_fringes())
                .map(|(_, t)| t.net)
                .sum();
            let located: Cents = p.by_location.iter().map(|(_, v)| v).sum();
            assert_eq!(located + typed, p.qualified, "seed {seed} {}", p.name);
            if !c.locations.is_empty() {
                assert!(
                    p.by_location
                        .iter()
                        .all(|(l, _)| l.as_ref().is_some_and(|l| c.locations.contains(l))),
                    "seed {seed} {}",
                    p.name
                );
            }

            // Split by location, row by row.
            let mut split = b.clone();
            split.credits = vec![c.clone(), c.clone()];
            split.credits[0].locations = set(&["L1"]);
            split.credits[1].locations = set(&["L2", "L3"]);
            let mut all = c.clone();
            all.locations = set(&["L1", "L2", "L3"]);
            split.credits.push(all);
            let s = estimates(&split, seed);
            let rows = |i: usize| s.programs[i].figures.clone().unwrap().rows;
            for ((a, z), w) in rows(0).iter().zip(rows(1)).zip(rows(2)) {
                if c.rows.iter().any(|r| !r.source.has_fringes()) {
                    continue;
                }
                assert_eq!(a.gross + z.gross, w.gross, "seed {seed}");
                assert_eq!(a.net + z.net, w.net, "seed {seed}");
            }
        }
    }
}

/// An applied program at its locations explains to exactly its amount.
#[test]
fn explaining_a_program_at_its_locations_adds_up() {
    for seed in 1..=CASES / 4 {
        let b = gen_budget(&mut Rng(seed));
        let r = recalc(&b, &RecalcOptions::default()).unwrap();
        for (i, c) in r.credits.iter().enumerate() {
            let e = explain::explain(&b, &RecalcOptions::default(), &explain::Target::Credit(i))
                .unwrap_or_else(|e| panic!("seed {seed}: {e}"));
            assert_eq!(e.root.amount, Some(c.amount), "seed {seed}");
        }
    }
}

// ---------------------------------------------------------------------------
// Bid layouts (roadmap 3.13; decisions.md, entry 92)

fn gen_bid(rng: &mut Rng, b: &Budget) -> BidLayout {
    let cats: Vec<String> = b.categories().map(|c| c.code.clone()).collect();
    let accts: Vec<String> = b
        .categories()
        .flat_map(|c| c.accounts.iter().map(|a| a.code.clone()))
        .collect();
    let any = |rng: &mut Rng, list: &[String], stray: &str| {
        if list.is_empty() || rng.chance(10) {
            stray.to_string()
        } else {
            list[rng.below(list.len() as u64) as usize].clone()
        }
    };
    let mut lines = Vec::new();
    let mut names = Vec::new();
    for i in 0..rng.below(7) {
        match rng.below(6) {
            0 => lines.push(BidLine::Subtotal {
                name: format!("Subtotal {i}"),
            }),
            1 => {
                let mut of: Vec<String> =
                    names.iter().filter(|_| rng.chance(50)).cloned().collect();
                if rng.chance(10) {
                    of.push("Nowhere".into());
                }
                lines.push(BidLine::Percent {
                    name: format!("Markup {i}"),
                    percent: format!("{}.{}", rng.below(30), rng.below(1000))
                        .parse()
                        .unwrap(),
                    of,
                });
            }
            2 => lines.push(BidLine::Flat {
                name: format!("Fee {i}"),
                amount: rng.below(1_000_000) as i64 - 100_000,
            }),
            _ => {
                let mut picks = Vec::new();
                for _ in 0..rng.below(4) {
                    picks.push(match rng.below(3) {
                        0 => BidPick::Category(any(rng, &cats, "9999")),
                        1 => BidPick::Account(any(rng, &accts, "1015")),
                        _ => {
                            let (x, y) = (any(rng, &accts, "1000"), any(rng, &accts, "9000"));
                            BidPick::Range { from: x, to: y }
                        }
                    });
                }
                let name = format!("Section {i}");
                names.push(name.clone());
                lines.push(BidLine::Section { name, picks });
            }
        }
    }
    BidLayout {
        fields: Vec::new(),
        lines,
        show_accounts: rng.chance(50),
    }
}

#[test]
fn a_bid_always_reconciles_with_the_budget() {
    for seed in 1..=CASES {
        let msg = format!("seed {seed}");
        let mut rng = Rng(seed);
        let b = gen_budget(&mut rng);
        let r = recalc(&b, &RecalcOptions::default()).unwrap();
        for _ in 0..3 {
            let layout = gen_bid(&mut rng, &b);
            let s = bid_sheet(&b, &r, &layout).unwrap();
            // Every account exactly once: in one section or Unassigned.
            let mut seen: Vec<&str> = s
                .lines
                .iter()
                .flat_map(|l| match l {
                    BidSheetLine::Section { accounts, .. } => accounts.as_slice(),
                    _ => &[],
                })
                .chain(&s.unassigned)
                .map(|a| a.code.as_str())
                .collect();
            let mut every: Vec<&str> = b
                .categories()
                .flat_map(|c| c.accounts.iter().map(|a| a.code.as_str()))
                .collect();
            seen.sort_unstable();
            every.sort_unstable();
            assert_eq!(seen, every, "{msg}");
            // Each section is its accounts; sections and Unassigned are
            // every account at its lines plus the fringes it earns.
            let mut sections = 0;
            let mut markups = 0;
            for l in &s.lines {
                match l {
                    BidSheetLine::Section {
                        accounts, total, ..
                    } => {
                        assert_eq!(*total, accounts.iter().map(|a| a.amount).sum(), "{msg}");
                        sections += total;
                    }
                    BidSheetLine::Markup { amount, .. } => markups += amount,
                    BidSheetLine::Subtotal { .. } => {}
                }
            }
            let earned: Cents = r
                .categories
                .iter()
                .flat_map(|c| &c.accounts)
                .map(|a| a.details_total + a.fringe_total)
                .sum();
            assert_eq!(sections + s.unassigned_total, earned, "{msg}");
            // Down to the budget's charges and credits, the bid is the
            // budget's net total; the markups are on top.
            let charges: Cents = s.charges.iter().map(|c| c.1).sum();
            let credits: Cents = s.credits.iter().map(|c| c.1).sum();
            assert_eq!(
                sections + s.unassigned_total + s.unposted_fringes + charges - credits,
                r.net_total,
                "{msg}"
            );
            assert_eq!(s.markups, markups, "{msg}");
            assert_eq!(s.total, r.net_total + markups, "{msg}");
            // Fringes go unposted only when the budget drops them.
            if !(b.fringe_posting == FringePosting::ProductionLevel
                && b.trailing_level_fringes == TrailingFringes::Drop)
            {
                assert_eq!(s.unposted_fringes, 0, "{msg}");
            }
            assert!(s.unposted_fringes <= 0, "{msg}");
        }
    }
}

// ---------------------------------------------------------------------------
// Caps shared by programs (decisions.md, entry 88)

/// Two shared caps, S1 and S2, of random amounts, and each program in one
/// of them, in a cap the budget doesn't have, or in none.
fn gen_shared_caps(rng: &mut Rng, b: &mut Budget) {
    b.shared_caps.clear();
    b.shared_caps
        .insert("S1".into(), rng.below(1_000_000) as i64);
    b.shared_caps.insert("S2".into(), rng.below(300_000) as i64);
    for c in &mut b.credits {
        c.shared_cap = match rng.below(5) {
            0 | 1 => Some("S1".into()),
            2 => Some("S2".into()),
            3 => Some("Missing".into()),
            _ => None,
        };
    }
}

/// Budgets with four random programs, some sharing caps.
fn shared_cap_budget(seed: u64) -> (Budget, Rng) {
    let mut rng = Rng(seed);
    let mut b = gen_budget(&mut rng);
    b.credits.clear();
    for i in 0..4 {
        let p = gen_program(&mut rng, &b, &format!("P{i}"));
        b.credits.push(p);
    }
    gen_shared_caps(&mut rng, &mut b);
    (b, rng)
}

/// What the topsheet takes off for each applied program, by name.
fn taken(b: &Budget, seed: u64) -> std::collections::BTreeMap<String, Cents> {
    let r = recalc(b, &RecalcOptions::default()).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
    r.credits
        .iter()
        .map(|c| (c.name.clone(), c.amount))
        .collect()
}

/// The applied programs under one shared cap never come to more than it;
/// with no discount and no fee they come to exactly the cap when they
/// would be more without it, and to what they would be when not; and no
/// program gets more from sharing a cap than it would alone.
#[test]
fn a_shared_cap_is_never_exceeded_and_adds_up_to_the_cent() {
    let mut held = 0;
    for seed in 1..=CASES {
        let (mut b, _) = shared_cap_budget(seed);
        let got = taken(&b, seed);
        let mut alone = b.clone();
        for c in &mut alone.credits {
            c.shared_cap = None;
        }
        let without = taken(&alone, seed);
        for (name, amount) in &got {
            assert!(*amount <= without[name], "seed {seed} {name}");
        }
        for (cap_name, cap) in &b.shared_caps {
            let members: Vec<&String> = b
                .credits
                .iter()
                .filter(|c| c.applied && c.shared_cap.as_deref() == Some(cap_name.as_str()))
                .map(|c| &c.name)
                .collect();
            let sum: Cents = members.iter().map(|n| got[*n]).sum();
            assert!(sum <= *cap, "seed {seed}: {cap_name} {sum} over {cap}");
        }

        // No discount, no fee: the shares add up to the cap exactly.
        for c in &mut b.credits {
            c.discount_percent = Dec::ZERO;
            c.broker_fee = 0;
        }
        let got = taken(&b, seed);
        let mut alone = b.clone();
        for c in &mut alone.credits {
            c.shared_cap = None;
        }
        let without = taken(&alone, seed);
        for (cap_name, cap) in &b.shared_caps {
            let members: Vec<&String> = b
                .credits
                .iter()
                .filter(|c| c.applied && c.shared_cap.as_deref() == Some(cap_name.as_str()))
                .map(|c| &c.name)
                .collect();
            let sum: Cents = members.iter().map(|n| got[*n]).sum();
            let before: Cents = members.iter().map(|n| without[*n]).sum();
            assert_eq!(sum, before.min(*cap), "seed {seed}: {cap_name}");
            held += usize::from(members.len() > 1 && before > *cap);
        }
        // A cap the budget doesn't have holds nothing down.
        for c in b.credits.iter().filter(|c| c.applied) {
            if c.shared_cap.as_deref() == Some("Missing") {
                assert_eq!(got[&c.name], without[&c.name], "seed {seed}");
            }
        }
    }
    // The generator does share caps between programs that are over them.
    assert!(held > CASES as usize / 10, "only {held} shared caps held");
}

/// Sharing a cap doesn't depend on the order of the programs, applied or
/// estimated.
#[test]
fn a_shared_cap_is_order_independent() {
    for seed in 1..=CASES {
        let (b, _) = shared_cap_budget(seed);
        let got = estimates(&b, seed);
        let mut reversed = b.clone();
        reversed.credits.reverse();
        let back = estimates(&reversed, seed);
        for p in &got.programs {
            let q = back.programs.iter().find(|q| q.name == p.name).unwrap();
            assert_eq!(p.figures, q.figures, "seed {seed} {}", p.name);
        }
        let mut rotated = b.clone();
        rotated.credits.rotate_left(1);
        assert_eq!(taken(&rotated, seed), taken(&b, seed), "seed {seed}");
    }
}

/// With shared caps: no programs change nothing, unapplied programs change
/// nothing, an applied program's estimate is what the topsheet takes off,
/// and an unapplied one's is what it would take off once applied.
#[test]
fn shared_caps_with_no_programs_or_unapplied_ones_change_nothing() {
    for seed in 1..=CASES {
        let (b, _) = shared_cap_budget(seed);
        let mut none = b.clone();
        none.credits.clear();
        let mut bare = none.clone();
        bare.shared_caps.clear();
        let r_none = recalc(&none, &RecalcOptions::default()).unwrap();
        assert_eq!(
            r_none,
            recalc(&bare, &RecalcOptions::default()).unwrap(),
            "seed {seed}: shared caps alone change nothing"
        );
        let mut off = b.clone();
        for c in &mut off.credits {
            c.applied = false;
        }
        assert_eq!(
            recalc(&off, &RecalcOptions::default()).unwrap(),
            r_none,
            "seed {seed}: unapplied programs change nothing"
        );

        let got = estimates(&b, seed);
        let now = taken(&b, seed);
        for (c, p) in b.credits.iter().zip(&got.programs) {
            let amount = p
                .amount()
                .unwrap_or_else(|| panic!("seed {seed}: {:?}", p.problem));
            if c.applied {
                assert_eq!(amount, now[&c.name], "seed {seed} {}", c.name);
            } else {
                let mut on = b.clone();
                on.credits[p.index].applied = true;
                assert_eq!(amount, taken(&on, seed)[&c.name], "seed {seed} {}", c.name);
            }
        }
    }
}

/// An applied program with a minimum, a fee or a shared cap explains to
/// exactly its amount.
#[test]
fn explaining_a_program_with_its_terms_adds_up() {
    for seed in 1..=CASES / 4 {
        let (b, _) = shared_cap_budget(seed);
        let r = recalc(&b, &RecalcOptions::default()).unwrap();
        for (i, c) in r.credits.iter().enumerate() {
            let e = explain::explain(&b, &RecalcOptions::default(), &explain::Target::Credit(i))
                .unwrap_or_else(|e| panic!("seed {seed}: {e}"));
            assert_eq!(e.root.amount, Some(c.amount), "seed {seed} {}", c.name);
        }
    }
}

// ---------------------------------------------------------------------------
// What-ifs and the per-location report (decisions.md, entry 93)

/// Every line id in `b`, in budget order.
fn line_ids(b: &Budget) -> Vec<RowId> {
    b.categories()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.details)
        .map(|d| d.id)
        .collect()
}

/// Random lines (about a third) and a random place: L1, L2 or none.
fn gen_move(rng: &mut Rng, b: &Budget) -> (Vec<RowId>, Option<String>) {
    let rows: Vec<RowId> = line_ids(b).into_iter().filter(|_| rng.chance(35)).collect();
    let to = match rng.below(3) {
        0 => None,
        1 => Some("L1".to_string()),
        _ => Some("L2".to_string()),
    };
    (rows, to)
}

/// A what-if's "after" is exactly what applying it gives: the estimates,
/// the topsheet's credits and the net total. Its "before" is the budget as
/// it is, and the budget doesn't change.
#[test]
fn a_what_if_is_what_applying_it_gives() {
    use incentives::what_if::{move_lines, what_if};
    for seed in 1..=CASES {
        let (b, mut rng) = shared_cap_budget(seed);
        let (rows, to) = gen_move(&mut rng, &b);
        let kept = b.clone();
        let w = what_if(&b, &rows, to.as_deref()).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
        assert_eq!(b, kept, "seed {seed}: the what-if changed the budget");
        assert_eq!(w.before, estimates(&b, seed), "seed {seed}");
        let mut moved = b.clone();
        let n = move_lines(&mut moved, &rows, to.as_deref()).unwrap();
        assert_eq!(n, w.changed, "seed {seed}");
        assert_eq!(w.after, estimates(&moved, seed), "seed {seed}");
        let r = recalc(&moved, &RecalcOptions::default()).unwrap();
        assert_eq!(w.after_totals.credits_total, r.credits_total, "seed {seed}");
        assert_eq!(w.after_totals.net_total, r.net_total, "seed {seed}");
        // Moving lines never changes the grand total.
        assert_eq!(
            w.before_totals.grand_total, w.after_totals.grand_total,
            "seed {seed}"
        );
        for p in &w.programs {
            if let (Some(a), Some(z)) = (p.before, p.after) {
                assert_eq!(p.difference, Some(z - a), "seed {seed}");
            }
        }
    }
}

/// The order the lines are listed in changes nothing, and moving two sets
/// of lines to one place one after the other is moving them together.
#[test]
fn a_what_if_is_order_independent() {
    use incentives::what_if::{move_lines, what_if};
    for seed in 1..=CASES {
        let (b, mut rng) = shared_cap_budget(seed);
        let (rows, to) = gen_move(&mut rng, &b);
        let w = what_if(&b, &rows, to.as_deref()).unwrap();
        let mut back = rows.clone();
        back.reverse();
        let mut wb = what_if(&b, &back, to.as_deref()).unwrap();
        wb.rows = w.rows.clone();
        assert_eq!(w, wb, "seed {seed}");

        let (first, second) = rows.split_at(rows.len() / 2);
        let mut step = b.clone();
        move_lines(&mut step, second, to.as_deref()).unwrap();
        move_lines(&mut step, first, to.as_deref()).unwrap();
        assert_eq!(estimates(&step, seed), w.after, "seed {seed}");
    }
}

/// A what-if with no lines, or with lines already where it moves them,
/// changes nothing: every program the same, every total the same.
#[test]
fn an_empty_what_if_changes_nothing() {
    use incentives::what_if::what_if;
    for seed in 1..=CASES {
        let (b, mut rng) = shared_cap_budget(seed);
        let (_, to) = gen_move(&mut rng, &b);
        let w = what_if(&b, &[], to.as_deref()).unwrap();
        assert_eq!(w.changed, 0, "seed {seed}");
        assert_eq!(w.before, w.after, "seed {seed}");
        assert_eq!(w.before_totals, w.after_totals, "seed {seed}");
        assert!(
            w.programs
                .iter()
                .all(|p| p.difference == Some(0) || p.before.is_none()),
            "seed {seed}"
        );
        assert!(
            w.locations.iter().all(|l| l.before == l.after),
            "seed {seed}"
        );
        // Lines already there.
        let here: Vec<RowId> = b
            .categories()
            .flat_map(|c| &c.accounts)
            .flat_map(|a| &a.details)
            .filter(|d| d.location == to)
            .map(|d| d.id)
            .collect();
        let w = what_if(&b, &here, to.as_deref()).unwrap();
        assert_eq!(w.changed, 0, "seed {seed}");
        assert_eq!(w.before, w.after, "seed {seed}");
    }
}

/// The per-location report reconciles with the topsheet: the applied
/// programs' credits are its credits total, the net total is the grand
/// total less them, each program's spend location by location (plus its
/// typed amounts) is its qualified spend, and the locations' spend adds
/// up.
#[test]
fn the_location_report_reconciles() {
    use incentives::by_location::location_report;
    for seed in 1..=CASES {
        let (b, _) = shared_cap_budget(seed);
        let r = recalc(&b, &RecalcOptions::default()).unwrap();
        let inc = incentives::incentives(&b, &r).unwrap();
        let rep = location_report(&r, &inc).unwrap();
        assert_eq!(rep.applied_total, r.credits_total, "seed {seed}");
        assert_eq!(
            rep.net_total,
            rep.grand_total - rep.applied_total,
            "seed {seed}"
        );
        assert_eq!(
            rep.spend_total,
            rep.locations.iter().map(|l| l.spend.total).sum::<Cents>(),
            "seed {seed}"
        );
        for p in &rep.programs {
            let here: Cents = rep
                .locations
                .iter()
                .flat_map(|l| &l.programs)
                .filter(|q| q.index == p.index)
                .map(|q| q.qualified_here)
                .sum();
            assert_eq!(here + p.typed, p.qualified, "seed {seed} {}", p.name);
            let amount = p.amount.unwrap();
            if !p.below_minimum && p.shared.is_none() {
                // The discount and the fee take the capped figure to the
                // credit, to the cent.
                let c = inc.programs[p.index].figures.as_ref().unwrap();
                assert_eq!(
                    c.capped.to_cents().unwrap() - p.discount - p.fee,
                    amount,
                    "seed {seed} {}",
                    p.name
                );
            }
        }
        let estimated: Cents = rep
            .programs
            .iter()
            .filter(|p| !p.applied)
            .filter_map(|p| p.amount)
            .sum();
        assert_eq!(rep.estimates_total, estimated, "seed {seed}");
    }
}
