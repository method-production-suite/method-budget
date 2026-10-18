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
