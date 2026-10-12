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
        rows: vec![CreditRow {
            description: String::new(),
            source: CreditSource::Group { group: "A".into() },
            percent: d("25"),
            cap: CreditCap::QualifiedSpend(rng.below(5_000_000) as i64),
        }],
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
