//! Incentive estimates (roadmap 3.6; decisions.md, entry 80): a credit
//! limited to locations counts only the lines there, every credit gets an
//! estimate whether it is applied or not, and spend is shown by location.
//! Every figure is invented and worked out by hand below.

mod common;

use common::*;
use method_core::incentives::{incentives, Incentives};
use method_core::*;

/// Category 2100, account 2101: a 1,000.00 line at Ridge earning a 10%
/// fringe (100.00), a 500.00 line at Coast, a 200.00 line with no location.
/// Account 2102: 300.00 at Ridge tagged LOCAL.
fn budget() -> Budget {
    let mut b = Budget::new("Incentives", "USD");
    b.fringe_posting = FringePosting::Budget;
    for name in ["Ridge", "Coast"] {
        b.locations.insert(name.into(), Location::default());
    }
    b.groups.insert("LOCAL".into(), group(true));
    b.fringes.insert(
        "PEN".into(),
        Fringe {
            short_code: None,
            description: String::new(),
            rate: "10".into(),
            kind: FringeKind::Percent { cutoff: None },
        },
    );
    let mut ridge = Detail::line(1, "Ridge crew", "1", "", "1000");
    ridge.location = Some("Ridge".into());
    ridge.fringes = set(&["PEN"]);
    let mut coast = Detail::line(2, "Coast crew", "1", "", "500");
    coast.location = Some("Coast".into());
    let nowhere = Detail::line(3, "Office", "1", "", "200");
    let mut local = Detail::line(4, "Local hire", "1", "", "300");
    local.location = Some("Ridge".into());
    local.groups = set(&["LOCAL"]);
    b.topsheet.push(category(
        "2100",
        "Crew",
        vec![
            account("2101", "Crew", vec![ridge, coast, nowhere]),
            account("2102", "Local", vec![local]),
        ],
    ));
    b
}

fn program(name: &str, locations: &[&str], source: CreditSource, percent: &str) -> Credit {
    Credit {
        name: name.into(),
        account: None,
        applied: false,
        cap: None,
        discount_percent: Dec::ZERO,
        excluded_fringes: Default::default(),
        locations: set(locations),
        minimum_spend: None,
        broker_fee: 0,
        shared_cap: None,
        rows: vec![CreditRow {
            description: "Qualified spend".into(),
            source,
            percent: d(percent),
            cap: CreditCap::None,
        }],
    }
}

fn cat(code: &str) -> CreditSource {
    CreditSource::Category {
        category: code.into(),
    }
}

fn estimate(b: &Budget) -> Incentives {
    let r = recalc(b, &RecalcOptions::default()).unwrap();
    incentives(b, &r).unwrap()
}

#[test]
fn spend_is_shown_by_location() {
    let got = estimate(&budget());
    let rows: Vec<(Option<&str>, Cents, Cents, Cents)> = got
        .locations
        .iter()
        .map(|l| (l.location.as_deref(), l.lines, l.fringes, l.total))
        .collect();
    assert_eq!(
        rows,
        vec![
            (Some("Coast"), 50_000, 0, 50_000),
            (Some("Ridge"), 130_000, 10_000, 140_000),
            (None, 20_000, 0, 20_000),
        ]
    );
    assert!(got.programs.is_empty());
}

#[test]
fn a_program_counts_only_lines_at_its_locations() {
    let mut b = budget();
    b.credits
        .push(program("Ridge program", &["Ridge"], cat("2100"), "25"));
    let got = estimate(&b);
    let p = &got.programs[0];
    assert!(!p.applied);
    // 1,000.00 + its 100.00 fringe + 300.00, at 25%.
    assert_eq!(p.qualified, 140_000);
    assert_eq!(p.amount(), Some(35_000));
    assert_eq!(p.by_location, vec![(Some("Ridge".to_string()), 140_000)]);

    // Not applied: the topsheet doesn't count it.
    let r = recalc(&b, &RecalcOptions::default()).unwrap();
    assert!(r.credits.is_empty());
    assert_eq!(r.net_total, r.grand_total);

    // Applied, the topsheet takes off what the estimate said.
    b.credits[0].applied = true;
    let r = recalc(&b, &RecalcOptions::default()).unwrap();
    assert_eq!(r.credits_total, 35_000);
    assert_eq!(estimate(&b).programs[0].amount(), Some(35_000));
}

#[test]
fn locations_combine_with_groups_accounts_and_excluded_fringes() {
    let mut b = budget();
    let group = CreditSource::Group {
        group: "LOCAL".into(),
    };
    b.credits
        .push(program("Local", &["Ridge"], group.clone(), "10"));
    b.credits
        .push(program("Local at Coast", &["Coast"], group, "10"));
    let mut acct = program(
        "Account",
        &["Ridge", "Coast"],
        CreditSource::Account {
            account: "2101".into(),
        },
        "20",
    );
    acct.excluded_fringes = set(&["PEN"]);
    b.credits.push(acct);
    let got = estimate(&b);
    assert_eq!(got.programs[0].qualified, 30_000);
    assert_eq!(got.programs[0].amount(), Some(3_000));
    assert_eq!(got.programs[1].qualified, 0);
    assert_eq!(got.programs[1].amount(), Some(0));
    // 1,000.00 at Ridge (its fringe left out) and 500.00 at Coast; the
    // office line has no location.
    let a = &got.programs[2];
    let f = a.figures.as_ref().unwrap();
    assert_eq!(
        (f.rows[0].gross, f.rows[0].excluded_fringes),
        (160_000, 10_000)
    );
    assert_eq!(a.qualified, 150_000);
    assert_eq!(
        a.by_location,
        vec![
            (Some("Coast".to_string()), 50_000),
            (Some("Ridge".to_string()), 100_000),
        ]
    );
    assert_eq!(a.amount(), Some(30_000));
}

#[test]
fn caps_and_the_broker_discount_apply_to_the_estimate() {
    let mut b = budget();
    let mut p = program("Capped", &["Ridge"], cat("2100"), "25");
    p.cap = Some(20_000);
    p.discount_percent = d("10");
    b.credits.push(p);
    let mut q = program("Spend cap", &[], cat("2100"), "25");
    q.rows[0].cap = CreditCap::QualifiedSpend(100_000);
    b.credits.push(q);
    let got = estimate(&b);
    // 350.00 of credit, capped at 200.00, less 10%: 180.00.
    let p = &got.programs[0];
    assert_eq!(p.rows_total, d("350"));
    assert!(p.capped);
    assert_eq!(p.amount(), Some(18_000));
    // Every line, 2,100.00 of spend, counted up to 1,000.00.
    let q = &got.programs[1];
    assert_eq!(q.qualified, 210_000);
    assert!(!q.capped);
    assert_eq!(q.amount(), Some(25_000));
    assert_eq!(
        q.by_location,
        vec![
            (Some("Coast".to_string()), 50_000),
            (Some("Ridge".to_string()), 140_000),
            (None, 20_000),
        ]
    );
}

#[test]
fn a_typed_amount_ignores_locations() {
    let mut b = budget();
    b.credits.push(program(
        "Typed",
        &["Coast"],
        CreditSource::Flat(1_000_000),
        "30",
    ));
    let got = estimate(&b);
    assert_eq!(got.programs[0].qualified, 1_000_000);
    assert!(got.programs[0].by_location.is_empty());
    assert_eq!(got.programs[0].amount(), Some(300_000));
}

#[test]
fn a_program_that_cant_be_worked_out_says_why() {
    let mut b = budget();
    b.credits.push(program(
        "Bad",
        &["Ridge"],
        CreditSource::Group {
            group: "NOPE".into(),
        },
        "10",
    ));
    b.credits
        .push(program("Good", &["Ridge"], cat("2100"), "10"));
    let got = estimate(&b);
    assert!(got.programs[0].figures.is_none());
    assert!(got.programs[0]
        .problem
        .as_deref()
        .unwrap_or("")
        .contains("NOPE"));
    assert_eq!(got.programs[1].amount(), Some(14_000));
}

#[test]
fn a_location_the_budget_doesnt_have_is_warned_about() {
    let mut b = budget();
    b.credits
        .push(program("Elsewhere", &["Harbor"], cat("2100"), "25"));
    let r = recalc(&b, &RecalcOptions::default()).unwrap();
    let w = warnings(&b, &r);
    assert!(
        w.iter()
            .any(|w| w.contains("Elsewhere") && w.contains("Harbor")),
        "{w:?}"
    );
    assert_eq!(estimate(&b).programs[0].qualified, 0);
}

#[test]
fn explaining_an_applied_program_names_its_locations_and_adds_up() {
    let mut b = budget();
    let mut p = program("Ridge program", &["Ridge"], cat("2100"), "25");
    p.applied = true;
    b.credits.push(p);
    let e = explain::explain(&b, &RecalcOptions::default(), &explain::Target::Credit(0)).unwrap();
    assert_eq!(e.reported, 35_000);
    assert_eq!(e.root.amount, Some(35_000));
    let text = format!("{:?}", e.root);
    assert!(text.contains("Ridge"), "{text}");
    assert!(text.contains("1,400.00"), "{text}");
}

#[test]
fn diff_merge_rename_and_usage_follow_a_programs_locations() {
    let mut a = budget();
    a.credits
        .push(program("Ridge program", &[], cat("2100"), "25"));
    let mut b = a.clone();
    b.credits[0].locations = set(&["Ridge"]);
    let text = diff::diff(&a, &b).to_text();
    assert!(text.contains("locations"), "{text}");
    assert!(text.contains("Ridge"), "{text}");

    // Theirs limits the program; ours changes something else: both land.
    let mut ours = a.clone();
    ours.credits[0].discount_percent = d("5");
    let m = merge::merge(&a, &ours, &b);
    assert!(m.conflicts.is_empty(), "{:?}", m.conflicts);
    assert_eq!(m.budget.credits[0].locations, set(&["Ridge"]));
    assert_eq!(m.budget.credits[0].discount_percent, d("5"));

    // Renaming the location renames it in the program and keeps the figure.
    let before = estimate(&b).programs[0].amount();
    let mut renamed = b.clone();
    assert_eq!(rename::references(&b, SetupKind::Location, "Ridge"), 3);
    rename(&mut renamed, SetupKind::Location, "Ridge", "Hollins Ridge").unwrap();
    assert_eq!(renamed.credits[0].locations, set(&["Hollins Ridge"]));
    assert_eq!(estimate(&renamed).programs[0].amount(), before);

    // A location a program names counts as used.
    let u = usage::usage(&b, SetupKind::Location);
    assert_eq!(u["Ridge"].other, 1);
    assert_eq!(u["Coast"].other, 0);
}

/// A person-cutoff line in a credit's location (decisions.md, entries 79
/// and 80): Dana is paid 1,000.00 at Ridge and 1,000.00 at Coast under a
/// 10% fringe with a 1,500.00 cutoff, so as one earner she earns 150.00,
/// 75.00 on each line (100.00 each on their own). A Ridge program counts
/// the Ridge line's person share like any other fringe, and leaves it out
/// like any other excluded fringe.
#[test]
fn a_person_cutoff_fringe_counts_in_a_programs_location_like_any_fringe() {
    let mut b = budget();
    b.fringes.insert(
        "CAP".into(),
        Fringe {
            short_code: None,
            description: String::new(),
            rate: "10".into(),
            kind: FringeKind::Percent {
                cutoff: Some(150_000),
            },
        },
    );
    let mut coast = Detail::line(5, "Dana, Coast", "1", "", "1000");
    coast.location = Some("Coast".into());
    coast.fringes = set(&["CAP"]);
    coast.person = Some("Dana".into());
    for cat in b.categories_mut() {
        for a in &mut cat.accounts {
            for d in &mut a.details {
                if d.id == 1 {
                    d.fringes.insert("CAP".into());
                    d.person = Some("Dana".into());
                }
            }
        }
        cat.accounts
            .push(account("2103", "Dana", vec![coast.clone()]));
    }
    let r = recalc(&b, &RecalcOptions::default()).unwrap();
    let row = |id: RowId| {
        r.categories
            .iter()
            .flat_map(|c| &c.accounts)
            .flat_map(|a| &a.rows)
            .find(|x| x.id == id)
            .unwrap()
            .clone()
    };
    assert_eq!(row(1).fringes["CAP"], 7_500);
    assert_eq!(row(5).fringes["CAP"], 7_500);

    b.credits
        .push(program("Ridge program", &["Ridge"], cat("2100"), "100"));
    let mut ex = program("Ridge less CAP", &["Ridge"], cat("2100"), "100");
    ex.excluded_fringes = set(&["CAP"]);
    b.credits.push(ex);
    let got = estimate(&b);
    // 1,000.00 + PEN 100.00 + Dana's share 75.00, and the 300.00 local hire.
    assert_eq!(got.programs[0].qualified, 147_500);
    assert_eq!(got.programs[0].amount(), Some(147_500));
    assert_eq!(got.programs[1].qualified, 140_000);
    let ridge = got
        .locations
        .iter()
        .find(|l| l.location.as_deref() == Some("Ridge"))
        .unwrap();
    assert_eq!(ridge.total, 147_500);

    // Applied, Explain adds up to the same.
    b.credits[0].applied = true;
    let e = explain::explain(&b, &RecalcOptions::default(), &explain::Target::Credit(0)).unwrap();
    assert_eq!(e.reported, 147_500);
    assert_eq!(e.root.amount, Some(147_500));
}

// ---------------------------------------------------------------------------
// A minimum spend, a flat broker's fee and a cap shared by programs
// (decisions.md, entry 88)

/// A program paying `percent` of a typed amount of `spend`.
fn typed(name: &str, spend: Cents, percent: &str) -> Credit {
    program(name, &[], CreditSource::Flat(spend), percent)
}

fn amounts(got: &Incentives) -> Vec<Option<Cents>> {
    got.programs.iter().map(|p| p.amount()).collect()
}

#[test]
fn below_the_minimum_spend_a_program_pays_nothing_and_says_so() {
    // At Ridge: 1,400.00 of spend at 25% is 350.00.
    let ridge = |min: Option<Cents>| {
        let mut p = program("Ridge program", &["Ridge"], cat("2100"), "25");
        p.minimum_spend = min;
        p
    };
    let mut b = budget();
    b.credits = vec![
        ridge(Some(140_001)),
        ridge(Some(140_000)),
        ridge(Some(100_000)),
        ridge(None),
    ];
    for (i, c) in b.credits.iter_mut().enumerate() {
        c.name = format!("P{i}");
    }
    let got = estimate(&b);
    assert_eq!(
        amounts(&got),
        vec![Some(0), Some(35_000), Some(35_000), Some(35_000)]
    );
    let below = &got.programs[0];
    let f = below.figures.as_ref().unwrap();
    assert!(f.below_minimum);
    assert_eq!(f.capped, Dec::ZERO);
    assert_eq!(below.qualified, 140_000);
    assert!(!below.capped, "the cap isn't what holds it down");
    assert!(!got.programs[1].figures.as_ref().unwrap().below_minimum);

    // Applied, the topsheet takes nothing off for it.
    for c in &mut b.credits {
        c.applied = true;
    }
    let r = recalc(&b, &RecalcOptions::default()).unwrap();
    assert_eq!(r.credits[0].amount, 0);
    assert_eq!(r.credits_total, 3 * 35_000);
    let e = explain::explain(&b, &RecalcOptions::default(), &explain::Target::Credit(0)).unwrap();
    assert_eq!(e.root.amount, Some(0));
    let text = format!("{:?}", e.root);
    assert!(text.contains("below the minimum of 1,400.01"), "{text}");
}

#[test]
fn the_minimum_counts_spend_at_the_locations_before_any_cap() {
    let mut b = budget();
    // Everywhere it is 2,100.00; at Ridge only 1,400.00, under 1,500.00.
    let mut anywhere = program("Anywhere", &[], cat("2100"), "25");
    anywhere.minimum_spend = Some(150_000);
    let mut ridge = program("Ridge", &["Ridge"], cat("2100"), "25");
    ridge.minimum_spend = Some(150_000);
    // A cap on the spend doesn't lower what the minimum compares with:
    // 1,400.00 qualifies for a 1,000.00 minimum though only 500.00 counts.
    let mut spend_cap = program("Spend cap", &["Ridge"], cat("2100"), "25");
    spend_cap.minimum_spend = Some(100_000);
    spend_cap.rows[0].cap = CreditCap::QualifiedSpend(50_000);
    // Nor does the program's own cap.
    let mut own_cap = program("Own cap", &["Ridge"], cat("2100"), "25");
    own_cap.minimum_spend = Some(100_000);
    own_cap.cap = Some(10_000);
    b.credits = vec![anywhere, ridge, spend_cap, own_cap];
    let got = estimate(&b);
    assert_eq!(
        amounts(&got),
        vec![Some(52_500), Some(0), Some(12_500), Some(10_000)]
    );
    assert!(got.programs[3].capped);
}

#[test]
fn the_brokers_fee_comes_off_after_the_cap_and_discount_never_below_zero() {
    let mut b = budget();
    // 350.00, less 10%: 315.00, less a 15.00 fee: 300.00.
    let mut p = program("Fee", &["Ridge"], cat("2100"), "25");
    p.discount_percent = d("10");
    p.broker_fee = 1_500;
    // Capped at 200.00, less 10%: 180.00, less 5.00: 175.00.
    let mut capped = p.clone();
    capped.name = "Capped".into();
    capped.cap = Some(20_000);
    capped.broker_fee = 500;
    // A fee larger than the credit leaves 0, not less.
    let mut large = p.clone();
    large.name = "Large fee".into();
    large.broker_fee = 40_000;
    // Below the minimum there is nothing to take a fee from.
    let mut below = p.clone();
    below.name = "Below".into();
    below.minimum_spend = Some(1_000_000);
    b.credits = vec![p, capped, large, below];
    let got = estimate(&b);
    assert_eq!(
        amounts(&got),
        vec![Some(30_000), Some(17_500), Some(0), Some(0)]
    );
    let fees: Vec<Cents> = got
        .programs
        .iter()
        .map(|p| p.figures.as_ref().unwrap().fee)
        .collect();
    assert_eq!(fees, vec![1_500, 500, 31_500, 0]);

    for c in &mut b.credits {
        c.applied = true;
    }
    let r = recalc(&b, &RecalcOptions::default()).unwrap();
    assert_eq!(r.credits_total, 30_000 + 17_500);
    for i in 0..4 {
        let e =
            explain::explain(&b, &RecalcOptions::default(), &explain::Target::Credit(i)).unwrap();
        assert_eq!(e.root.amount, Some(r.credits[i].amount), "credit {i}");
    }
    let e = explain::explain(&b, &RecalcOptions::default(), &explain::Target::Credit(2)).unwrap();
    let text = format!("{:?}", e.root);
    assert!(text.contains("never below zero"), "{text}");
}

#[test]
fn one_program_under_a_shared_cap_is_held_to_it() {
    let mut b = budget();
    b.shared_caps.insert("State".into(), 30_000);
    let mut p = program("Ridge program", &["Ridge"], cat("2100"), "25");
    p.shared_cap = Some("State".into());
    p.applied = true;
    b.credits.push(p);
    let got = estimate(&b);
    assert_eq!(got.programs[0].amount(), Some(30_000));
    assert_eq!(
        got.programs[0].figures.as_ref().unwrap().shared,
        Some(30_000)
    );
    assert_eq!(got.programs[0].shared_cap_amount, Some(30_000));
    let r = recalc(&b, &RecalcOptions::default()).unwrap();
    assert_eq!(r.credits_total, 30_000);

    // Under a larger shared cap nothing changes.
    b.shared_caps.insert("State".into(), 50_000);
    let got = estimate(&b);
    assert_eq!(got.programs[0].amount(), Some(35_000));
    assert_eq!(got.programs[0].figures.as_ref().unwrap().shared, None);
}

#[test]
fn two_programs_share_a_cap_in_proportion() {
    let mut b = budget();
    b.shared_caps.insert("State".into(), 40_000);
    // 350.00 and 210.00 (10% of 2,100.00), 560.00 together: 400.00 is
    // shared 250.00 and 150.00.
    let mut a = program("A", &["Ridge"], cat("2100"), "25");
    let mut z = program("B", &[], cat("2100"), "10");
    for c in [&mut a, &mut z] {
        c.shared_cap = Some("State".into());
        c.applied = true;
    }
    // A program sharing no cap is unaffected.
    let mut other = program("Other", &[], cat("2100"), "10");
    other.applied = true;
    b.credits = vec![a, z, other];
    let got = estimate(&b);
    assert_eq!(
        amounts(&got),
        vec![Some(25_000), Some(15_000), Some(21_000)]
    );
    let r = recalc(&b, &RecalcOptions::default()).unwrap();
    assert_eq!(r.credits_total, 40_000 + 21_000);

    // The discount comes off each share: 250.00 less 10% is 225.00.
    b.credits[0].discount_percent = d("10");
    assert_eq!(estimate(&b).programs[0].amount(), Some(22_500));

    // Explain says what the share is and still adds up.
    let e = explain::explain(&b, &RecalcOptions::default(), &explain::Target::Credit(0)).unwrap();
    assert_eq!(e.root.amount, Some(22_500));
    let text = format!("{:?}", e.root);
    assert!(text.contains("Shared cap"), "{text}");
    assert!(text.contains("250.00"), "{text}");
}

#[test]
fn three_programs_share_a_cap_to_the_cent_whatever_their_order() {
    let mut b = budget();
    b.shared_caps.insert("Pool".into(), 10_000);
    // Three of 100.00 under a 100.00 cap: 33.33 each and a cent left,
    // which goes to the first by name, Alpha.
    b.credits = ["Gamma", "Alpha", "Beta"]
        .iter()
        .map(|n| {
            let mut c = typed(n, 100_000, "10");
            c.shared_cap = Some("Pool".into());
            c.applied = true;
            c
        })
        .collect();
    let got = estimate(&b);
    assert_eq!(amounts(&got), vec![Some(3_333), Some(3_334), Some(3_333)]);
    let r = recalc(&b, &RecalcOptions::default()).unwrap();
    assert_eq!(r.credits_total, 10_000);
    b.credits.reverse();
    let back = estimate(&b);
    assert_eq!(amounts(&back), vec![Some(3_333), Some(3_334), Some(3_333)]);

    // 100.00, 200.00 and 400.00 under 100.00: 14.2857, 28.5714 and
    // 57.1428 rounded down make 99.99; the largest remainder (14.2857's)
    // gets the cent.
    for (c, spend) in b.credits.iter_mut().zip([100_000, 200_000, 400_000]) {
        c.rows[0].source = CreditSource::Flat(spend);
    }
    let got = estimate(&b);
    assert_eq!(amounts(&got), vec![Some(1_429), Some(2_857), Some(5_714)]);
    let total: Cents = amounts(&got).into_iter().flatten().sum();
    assert_eq!(total, 10_000);
}

#[test]
fn a_programs_own_cap_comes_before_the_shared_cap() {
    let mut b = budget();
    b.shared_caps.insert("State".into(), 30_000);
    // A is 350.00 held to its own 200.00; B is 210.00. 410.00 together
    // over 300.00: 146.34 and 153.65 rounded down, and B's larger
    // remainder takes the cent.
    let mut a = program("A", &["Ridge"], cat("2100"), "25");
    a.cap = Some(20_000);
    let mut z = program("B", &[], cat("2100"), "10");
    for c in [&mut a, &mut z] {
        c.shared_cap = Some("State".into());
        c.applied = true;
    }
    b.credits = vec![a, z];
    let got = estimate(&b);
    assert_eq!(amounts(&got), vec![Some(14_634), Some(15_366)]);
    assert!(got.programs[0].capped);
    // The fee comes off after the share.
    b.credits[1].broker_fee = 366;
    assert_eq!(estimate(&b).programs[1].amount(), Some(15_000));
}

#[test]
fn an_unapplied_program_is_estimated_alongside_the_applied_ones() {
    let mut b = budget();
    b.shared_caps.insert("State".into(), 40_000);
    let mut a = program("A", &["Ridge"], cat("2100"), "25");
    let mut z = program("B", &[], cat("2100"), "10");
    for c in [&mut a, &mut z] {
        c.shared_cap = Some("State".into());
    }
    a.applied = true;
    b.credits = vec![a, z];
    // A alone is within the cap; B's estimate is its share were it applied
    // too, and A's figure stays what the topsheet takes off.
    let got = estimate(&b);
    assert_eq!(amounts(&got), vec![Some(35_000), Some(15_000)]);
    let r = recalc(&b, &RecalcOptions::default()).unwrap();
    assert_eq!(r.credits_total, 35_000);

    // Applying B takes off exactly its estimate.
    b.credits[1].applied = true;
    let r = recalc(&b, &RecalcOptions::default()).unwrap();
    assert_eq!(r.credits[1].amount, 15_000);
    assert_eq!(r.credits_total, 40_000);
}

#[test]
fn a_program_below_its_minimum_takes_no_share() {
    let mut b = budget();
    b.shared_caps.insert("State".into(), 30_000);
    let mut a = program("A", &["Ridge"], cat("2100"), "25");
    a.minimum_spend = Some(1_000_000);
    let mut z = program("B", &[], cat("2100"), "10");
    for c in [&mut a, &mut z] {
        c.shared_cap = Some("State".into());
        c.applied = true;
    }
    b.credits = vec![a, z];
    assert_eq!(amounts(&estimate(&b)), vec![Some(0), Some(21_000)]);
}

#[test]
fn a_shared_cap_the_budget_doesnt_have_holds_nothing_down_and_is_warned_about() {
    let mut b = budget();
    let mut p = program("Ridge program", &["Ridge"], cat("2100"), "25");
    p.shared_cap = Some("Nowhere".into());
    p.applied = true;
    b.credits.push(p);
    let r = recalc(&b, &RecalcOptions::default()).unwrap();
    assert_eq!(r.credits_total, 35_000);
    let w = warnings(&b, &r);
    assert!(
        w.iter()
            .any(|w| w.contains("Ridge program") && w.contains("Nowhere")),
        "{w:?}"
    );
}

#[test]
fn diff_and_merge_follow_the_minimum_fee_and_shared_cap() {
    let mut a = budget();
    a.credits
        .push(program("Ridge program", &[], cat("2100"), "25"));
    let mut b = a.clone();
    b.credits[0].minimum_spend = Some(100_000);
    b.credits[0].broker_fee = 2_500;
    b.credits[0].shared_cap = Some("State".into());
    b.shared_caps.insert("State".into(), 50_000);
    let text = diff::diff(&a, &b).to_text();
    for word in [
        "minimum spend",
        "broker's fee",
        "shared cap",
        "State",
        "500.00",
    ] {
        assert!(text.contains(word), "{word}: {text}");
    }

    // Theirs adds the terms and the shared cap; ours changes the discount.
    let mut ours = a.clone();
    ours.credits[0].discount_percent = d("5");
    let m = merge::merge(&a, &ours, &b);
    assert!(m.conflicts.is_empty(), "{:?}", m.conflicts);
    let c = &m.budget.credits[0];
    assert_eq!(
        (c.minimum_spend, c.broker_fee, c.shared_cap.as_deref()),
        (Some(100_000), 2_500, Some("State"))
    );
    assert_eq!(c.discount_percent, d("5"));
    assert_eq!(m.budget.shared_caps.get("State"), Some(&50_000));

    // Both change one shared cap's amount differently: a conflict.
    let mut ours = b.clone();
    ours.shared_caps.insert("State".into(), 60_000);
    let mut theirs = b.clone();
    theirs.shared_caps.insert("State".into(), 70_000);
    let m = merge::merge(&b, &ours, &theirs);
    assert_eq!(m.conflicts.len(), 1, "{:?}", m.conflicts);
}
