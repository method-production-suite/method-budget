//! Separator rows and display-only row styling (bold, italic, indent, colors,
//! notes): none of them may change a single total.

mod common;

use common::*;
use fwb_core::*;

fn run(b: &Budget) -> Report {
    recalc(b, &RecalcOptions::default()).unwrap()
}

fn one_account(details: Vec<Detail>) -> Budget {
    let mut b = Budget::new("t", "USD");
    b.topsheet = vec![category(
        "10",
        "Cat",
        vec![account("10-01", "Acct", details)],
    )];
    b
}

fn rows(r: &Report) -> &[RowTotals] {
    &r.categories[0].accounts[0].rows
}

#[test]
fn separators_add_nothing_and_reset_no_window() {
    let b = one_account(vec![
        Detail::line(1, "A", 1, "Flat", 1_000),
        Detail::separator(2),
        Detail::line(3, "B", 1, "Flat", 2_000),
        Detail::subtotal(4, "Sub"),
        Detail::adjustment(5, "Adj", "0.1"),
        Detail::separator(6),
        Detail::line(7, "C", 1, "Flat", 500),
        Detail::separator(8),
        Detail::adjustment(9, "Adj 2", "0.1"),
        Detail::subtotal(10, "Sub 2"),
    ]);
    let r = run(&b);
    let rows = rows(&r);
    // The separator between A and B resets neither the subtotal nor the
    // adjustment window.
    assert_eq!(rows[3].subtotal, Some(300_000));
    assert_eq!(rows[4].total, 30_000);
    // Separators after an adjustment don't reset the next window either.
    assert_eq!(rows[8].total, 5_000);
    assert_eq!(rows[9].subtotal, Some(30_000 + 50_000 + 5_000));
    for i in [1, 5, 7] {
        let s = &rows[i];
        assert_eq!(s.kind, RowKind::Separator);
        assert!(s.included);
        assert_eq!((s.total, s.local_total, s.fringe_total), (0, 0, 0));
        assert_eq!(s.subtotal, None);
        assert!(s.fringes.is_empty());
    }
    assert_eq!(r.categories[0].accounts[0].details_total, 385_000);
}

#[test]
fn separators_do_not_break_a_fringe_range() {
    let line = |id, range| {
        let mut d = Detail::line(id, "Grip", 1, "Flat", 1_000);
        d.fringes = set(&["PEN"]);
        d.fringe_range = range;
        d
    };
    let mut sep = Detail::separator(2);
    sep.fringe_range = Some(9);
    let mut b = one_account(vec![
        line(1, Some(1)),
        sep,
        Detail::separator(3),
        line(4, Some(1)),
    ]);
    b.fringes.insert(
        "PEN".into(),
        Fringe {
            short_code: None,
            description: String::new(),
            rate: "10".into(),
            kind: FringeKind::Percent {
                cutoff: Some(150_000),
            },
        },
    );
    let r = run(&b);
    let rows = rows(&r);
    // One earner: 2,000.00 caps at 1,500.00, so 150.00 split evenly. As two
    // earners it would be 100.00 each.
    assert_eq!(rows[0].fringes["PEN"], 7_500);
    assert_eq!(rows[3].fringes["PEN"], 7_500);
    assert_eq!(rows[1].fringe_total + rows[2].fringe_total, 0);
}

#[test]
fn separators_are_never_counted_fringed_or_reported() {
    // A separator carrying fringe, group, set and location tags (even unknown
    // ones) is still ignored: no error, no fringe, no breakdown entry.
    let mut sep = Detail::separator(2);
    sep.fringes = set(&["PCT", "NOPE"]);
    sep.groups = set(&["A", "MISSING"]);
    sep.set = Some("S1".into());
    sep.location = Some("L1".into());
    sep.amount = "5".into();
    sep.rate = "100".into();
    let mut line = Detail::line(1, "Line", 1, "Flat", 1_000);
    line.fringes = set(&["PCT"]);
    line.groups = set(&["A"]);
    let mut b = one_account(vec![line, sep]);
    b.groups.insert("A".into(), group(true));
    b.fringes.insert(
        "PCT".into(),
        Fringe {
            short_code: None,
            description: String::new(),
            rate: "10".into(),
            kind: FringeKind::Percent { cutoff: None },
        },
    );
    b.charges.push(percent_charge("Contingency", "10"));
    let r = run(&b);
    assert_eq!(rows(&r)[1].total, 0);
    assert_eq!(rows(&r)[1].fringe_total, 0);
    assert_eq!(r.fringes["PCT"].uses, 1);
    assert_eq!(r.fringes["PCT"].total, 10_000);
    assert_eq!(r.groups["A"].full.details_total, 100_000);
    assert!(r.sets.is_empty());
    assert!(r.locations.is_empty());
    assert_eq!(r.atl_btl, 110_000);

    let base = Baseline::capture(&r).unwrap();
    assert!(base.rows.contains_key(&1));
    assert!(
        !base.rows.contains_key(&2),
        "separators have no baseline row"
    );

    let ids = [1, 2].into_iter().collect();
    let sel = selection_totals(&b, &r, &ids).unwrap();
    assert_eq!(sel.lines.details_total, 100_000);
}

#[test]
fn removing_separators_changes_no_total() {
    let mut b = sample();
    let before = run(&b);
    // A separator at the start, the end and between every other pair of rows
    // (inside the grip fringe range and around the adjustment row too).
    let mut next = 1_000;
    for acct in b.categories_mut().flat_map(|c| &mut c.accounts) {
        for at in (0..=acct.details.len()).rev().step_by(2) {
            acct.details.insert(at, Detail::separator(next));
            next += 1;
        }
    }
    let mut after = run(&b);
    for cat in &mut after.categories {
        for acct in &mut cat.accounts {
            acct.rows.retain(|r| r.kind != RowKind::Separator);
        }
    }
    assert_eq!(after, before);
}

#[test]
fn styling_changes_no_total() {
    let mut b = sample();
    let before = run(&b);
    for g in b.groups.values_mut() {
        g.color = Some("#336699".into());
    }
    for (i, d) in b
        .categories_mut()
        .flat_map(|c| &mut c.accounts)
        .flat_map(|a| &mut a.details)
        .enumerate()
    {
        d.bold = i % 2 == 0;
        d.italic = i % 3 == 0;
        d.indent = (i % 5) as u8;
        d.color = Some("#aa0000".into());
        d.notes = vec![Note::plain(1, format!("note {i}"))];
    }
    assert_eq!(run(&b), before);
}

#[test]
fn only_lines_and_adjustments_are_costed() {
    assert!(RowKind::Line.is_costed());
    assert!(RowKind::Adjustment.is_costed());
    assert!(!RowKind::Subtotal.is_costed());
    assert!(!RowKind::Separator.is_costed());
    let s = Detail::separator(7);
    assert_eq!(s.kind, RowKind::Separator);
    assert!(s.description.is_empty());
    assert!(!s.bold && !s.italic && s.indent == 0);
    assert_eq!(s.color, None);
    assert!(s.notes.is_empty());
}
