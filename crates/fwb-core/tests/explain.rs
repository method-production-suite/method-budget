//! "Explain this number": every total's explanation adds back up to it.

mod common;

use std::collections::BTreeSet;

use common::*;
use fwb_core::explain::{explain, Explainer, Link, Role, Step, Target};
use fwb_core::sample::{self, SampleSpec};
use fwb_core::*;

/// Every total of `b` explained, each tree's leaves summing to the number
/// the report shows. Returns how many totals were checked.
fn check_all(b: &Budget, opts: &RecalcOptions) -> usize {
    let ex = Explainer::new(b, opts).unwrap();
    let targets = ex.targets();
    let mut n = 0;
    for t in &targets {
        let e = match ex.explain(t) {
            Ok(e) => e,
            Err(err) => panic!("{t:?}: {err}"),
        };
        assert_eq!(e.reported, ex.reported(t).unwrap(), "{t:?}");
        assert_eq!(e.root.amount, Some(e.reported), "{t:?}: {}", e.title);
        assert_eq!(
            e.root.leaf_sum(),
            Some(i128::from(e.reported)),
            "{t:?}: {}\n{}",
            e.title,
            e.to_text(Some(4))
        );
        n += 1;
    }
    n
}

fn postings() -> Vec<FringePosting> {
    vec![
        FringePosting::Budget,
        FringePosting::ProductionLevel,
        FringePosting::Category {
            suffix: "99".into(),
        },
        FringePosting::Account,
    ]
}

#[test]
fn every_total_of_the_big_sample_adds_up() {
    let b = sample::budget(&SampleSpec::default());
    let n = check_all(&b, &RecalcOptions::default());
    // Every line, its fringes, every account, category and summary row.
    assert!(n > 10_000, "only {n} totals");
}

#[test]
fn every_total_adds_up_under_every_posting_mode_and_filter() {
    for seed in 1..=4 {
        let spec = SampleSpec {
            seed,
            categories: 5,
            accounts: 20,
            rows: 600,
            fringes: 12,
            groups: 6,
            baseline: false,
        };
        let base = sample::budget(&spec);
        let groups: Vec<String> = base.groups.keys().cloned().collect();
        for posting in postings() {
            let mut b = base.clone();
            b.fringe_posting = posting.clone();
            check_all(&b, &RecalcOptions::default());
            let exclude = RecalcOptions {
                exclude_groups: groups.iter().take(2).cloned().collect(),
                only_groups: None,
            };
            check_all(&b, &exclude);
            let only = RecalcOptions {
                exclude_groups: BTreeSet::new(),
                only_groups: Some(groups.iter().skip(1).take(3).cloned().collect()),
            };
            check_all(&b, &only);
        }
    }
}

#[test]
fn every_total_of_the_hand_checked_sample_adds_up() {
    for posting in postings() {
        let mut b = sample();
        b.fringe_posting = posting;
        check_all(&b, &RecalcOptions::default());
        check_all(
            &b,
            &RecalcOptions {
                exclude_groups: set(&["OptionA"]),
                only_groups: None,
            },
        );
    }
}

/// The first step (depth first) whose label is `label`.
fn find<'a>(s: &'a Step, label: &str) -> Option<&'a Step> {
    if s.label == label {
        return Some(s);
    }
    s.children.iter().find_map(|c| find(c, label))
}

fn all_text(s: &Step) -> String {
    let mut out = format!("{}: {}\n", s.label, s.text);
    for c in &s.children {
        out.push_str(&all_text(c));
    }
    out
}

#[test]
fn a_line_shows_globals_quantity_and_rounding() {
    let b = sample();
    let e = explain(&b, &RecalcOptions::default(), &Target::Line(2)).unwrap();
    assert_eq!(e.reported, 5_400_000);
    assert_eq!(e.root.role, Role::Value);
    let text = all_text(&e.root);
    assert!(
        text.contains("SHOOT_DAYS + PREP_DAYS = 40 (PREP_DAYS = 10, SHOOT_DAYS = 30) Days"),
        "{text}"
    );
    assert!(text.contains("Quantity: 40 x 3 = 120"), "{text}");
    assert!(text.contains("Line: 120 x 450 = 54,000.00"), "{text}");
    // The line's fringes are pointed to, not added in.
    let f = find(&e.root, "Fringes").unwrap();
    assert_eq!(f.role, Role::Note);
    assert_eq!(f.amount, Some(673_768));
    assert_eq!(f.expand, Some(Target::LineFringes(2)));
    assert!(matches!(
        e.root.link,
        Some(Link::Row {
            ci: 1,
            ai: 0,
            row: 2
        })
    ));
}

#[test]
fn currency_and_adjustment_rows_show_their_working() {
    let b = sample();
    let opts = RecalcOptions::default();
    let cam = explain(&b, &opts, &Target::Line(5)).unwrap();
    let text = all_text(&cam.root);
    assert!(text.contains("6 Weeks"), "{text}");
    assert!(text.contains("6 x 1,200.5 = 7,203.00 CAD"), "{text}");
    assert!(
        text.contains("7,203.00 CAD x 0.73 USD per CAD = 5,258.19 USD"),
        "{text}"
    );
    let adj = explain(&b, &opts, &Target::Line(6)).unwrap();
    assert_eq!(adj.reported, 612_582);
    let text = all_text(&adj.root);
    assert!(text.contains("61,258.19 x 0.1 (10%) = 6,125.819"), "{text}");
    assert!(
        text.contains("6,125.819 rounds half away from zero to 6,125.82"),
        "{text}"
    );
    let base = find(&adj.root, "Base").unwrap();
    assert_eq!(base.amount, Some(6_125_819));
    assert_eq!(base.children.len(), 3, "grip, overtime and the camera");
}

#[test]
fn a_range_shares_its_cutoff_by_largest_remainder() {
    let b = sample();
    let e = explain(
        &b,
        &RecalcOptions::default(),
        &Target::LineFringe(2, "FICA".into()),
    )
    .unwrap();
    assert_eq!(e.reported, 73_768);
    let text = all_text(&e.root);
    assert!(
        text.contains("range 1: lines 1, 2 earn as one person"),
        "{text}"
    );
    assert!(text.contains("54,000.00 + 2,000.00"), "{text}");
    assert!(
        text.contains("FICA stops at 10,000.00 per earner; this line uses 9,642.86 of it"),
        "{text}"
    );
    assert!(text.contains("7.65% of 10,000.00 = 765.00"), "{text}");
    assert!(
        text.contains("which is 737.68 once the shares are split by largest remainder"),
        "{text}"
    );
    let ph = explain(
        &b,
        &RecalcOptions::default(),
        &Target::LineFringe(2, "PH".into()),
    )
    .unwrap();
    assert!(
        all_text(&ph.root).contains(
            "50.00 per Day x 120 (quantity) x 10 hours per Day / 10 hours per Day = 6,000.00"
        ),
        "{}",
        all_text(&ph.root)
    );
}

#[test]
fn left_out_lines_say_why() {
    let b = sample();
    let e = explain(&b, &RecalcOptions::default(), &Target::Line(7)).unwrap();
    assert_eq!(e.reported, 0);
    let out = find(&e.root, "Left out").unwrap();
    assert!(
        out.text.contains("group Suppress has In budget total off")
            && out.text.contains("counted, it would be 1,000.00"),
        "{}",
        out.text
    );
    let opts = RecalcOptions {
        exclude_groups: set(&["OptionA"]),
        only_groups: None,
    };
    let e = explain(&b, &opts, &Target::Line(5)).unwrap();
    assert!(find(&e.root, "Left out")
        .unwrap()
        .text
        .contains("the report leaves out group OptionA"));
}

#[test]
fn charges_list_their_exclusions_and_credits_their_caps() {
    let b = sample();
    let opts = RecalcOptions::default();
    let ins = explain(&b, &opts, &Target::Charge(1)).unwrap();
    assert_eq!(ins.reported, 257_873);
    let ex = find(&ins.root, "Excluded").unwrap();
    assert_eq!(ex.amount, Some(15_176_500));
    assert_eq!(ex.children.len(), 2, "the deposit and account 1100");
    let text = all_text(&ins.root);
    assert!(text.contains("left out by account 1100"), "{text}");
    assert!(text.contains("Deposit"), "{text}");
    assert!(
        text.contains("2.5% of (254,914.01 - 151,765.00) = 2,578.72525"),
        "{text}"
    );
    let bond = explain(&b, &opts, &Target::Charge(2)).unwrap();
    assert!(all_text(&bond.root).contains("5,000.00 CAD x 0.73 USD per CAD = 3,650.00"));

    let cr = explain(&b, &opts, &Target::Credit(0)).unwrap();
    assert_eq!(cr.reported, 990_000);
    let text = all_text(&cr.root);
    assert!(text.contains("30% of 30,000.00 = 9,000.00"), "{text}");
    assert!(
        text.contains("25% of 10,000.00 = 2,500.00, capped at 2,000.00: 2,000.00"),
        "{text}"
    );
    assert!(
        text.contains("less 10%: 11,000.00 x 0.9 = 9,900.00"),
        "{text}"
    );

    let net = explain(&b, &opts, &Target::NetTotal).unwrap();
    assert_eq!(net.reported, 27_673_414);
    let less = find(&net.root, "Less credits").unwrap();
    assert!(less.minus);
}

#[test]
fn posting_mode_decides_where_fringes_count() {
    let mut b = sample();
    let opts = RecalcOptions::default();
    let acct = Target::Account { ci: 1, ai: 0 };
    let e = explain(&b, &opts, &acct).unwrap();
    let note = find(&e.root, "Fringes earned here").unwrap();
    assert_eq!(note.role, Role::Note);
    assert!(note.text.contains("fringe account"), "{}", note.text);
    let cat = explain(&b, &opts, &Target::Category(1)).unwrap();
    assert!(
        find(&cat.root, "20-99 Fringes").is_some(),
        "{}",
        cat.to_text(Some(2))
    );

    b.fringe_posting = FringePosting::Account;
    let e = explain(&b, &opts, &acct).unwrap();
    let f = find(&e.root, "Fringes").unwrap();
    assert_eq!(f.role, Role::Sum);
    assert_eq!(f.amount, Some(676_500));
}

#[test]
fn subtotal_rows_and_text_output() {
    let b = sample();
    let e = explain(&b, &RecalcOptions::default(), &Target::Line(4)).unwrap();
    assert_eq!(e.reported, 5_600_000);
    assert_eq!(e.root.role, Role::Sum);
    assert_eq!(e.root.children.len(), 2);
    let text = e.to_text(None);
    assert!(
        text.starts_with("2100 line 3 (Grip subtotal): total: 56,000.00\n"),
        "{text}"
    );
    let short = explain(&b, &RecalcOptions::default(), &Target::GrandTotal)
        .unwrap()
        .to_text(Some(2));
    assert!(short.contains("  ..."), "{short}");
    // A separator has nothing to explain; an unknown row is gone.
    let mut b2 = sample();
    if let TopsheetRow::Category(c) = &mut b2.topsheet[0] {
        c.accounts[0].details.push(Detail::separator(99));
    }
    assert!(explain(&b2, &RecalcOptions::default(), &Target::Line(99)).is_err());
    assert!(explain(&b, &RecalcOptions::default(), &Target::Line(12345)).is_err());
}

#[test]
fn targets_parse_from_text() {
    let b = sample();
    let r = recalc(&b, &RecalcOptions::default()).unwrap();
    let p = |s: &str| Target::parse(&b, &r, s);
    assert_eq!(p("grand").unwrap(), Target::GrandTotal);
    assert_eq!(p("topsheet").unwrap(), Target::GrandTotal);
    assert_eq!(p("net").unwrap(), Target::NetTotal);
    assert_eq!(p("ATL").unwrap(), Target::Atl);
    assert_eq!(p("charge:insurance").unwrap(), Target::Charge(1));
    assert_eq!(p("charge:3").unwrap(), Target::Charge(2));
    assert_eq!(p("credit:1").unwrap(), Target::Credit(0));
    assert_eq!(p("level:Total Production").unwrap(), Target::Level(1));
    assert_eq!(p("fringe:fica").unwrap(), Target::Fringe("FICA".into()));
    assert_eq!(p("#5").unwrap(), Target::Line(5));
    assert_eq!(p("line:6").unwrap(), Target::Line(6));
    assert_eq!(p("2100:2").unwrap(), Target::Line(3));
    assert_eq!(p("2100").unwrap(), Target::Account { ci: 1, ai: 0 });
    assert_eq!(
        p("2100/fringes").unwrap(),
        Target::AccountFringes { ci: 1, ai: 0 }
    );
    assert_eq!(p("2000").unwrap(), Target::Category(1));
    assert_eq!(p("category:5000").unwrap(), Target::Category(2));
    for bad in ["nope", "charge:9", "2100:99", "#x", "2000/lines"] {
        assert!(p(bad).is_err(), "{bad}");
    }
    // Spellings are normalized (`10-1` is `10-01`), but a number is never
    // cut short to name an account it doesn't (`10-012` is not `10-01`).
    let s = starter::budget();
    let rs = recalc(&s, &RecalcOptions::default()).unwrap();
    let ps = |t: &str| Target::parse(&s, &rs, t);
    assert_eq!(ps("10-1").unwrap(), Target::Account { ci: 0, ai: 0 });
    assert_eq!(ps("1001").unwrap(), Target::Account { ci: 0, ai: 0 });
    for bad in ["10-012", "100123"] {
        assert!(ps(bad).is_err(), "{bad}");
    }
}

#[test]
fn explaining_changes_nothing() {
    let b = sample();
    let before = b.clone();
    let r = recalc(&b, &RecalcOptions::default()).unwrap();
    check_all(&b, &RecalcOptions::default());
    assert_eq!(b, before);
    assert_eq!(recalc(&b, &RecalcOptions::default()).unwrap(), r);
}
