//! A charge previewed on an existing report matches what a full recalc gives.

mod common;

use common::*;
use fwb_core::*;

fn run(b: &Budget) -> Report {
    recalc(b, &RecalcOptions::default()).unwrap()
}

#[test]
fn preview_matches_the_recalc_for_every_applied_charge() {
    let b = sample();
    let r = run(&b);
    let applied: Vec<&Charge> = b.charges.iter().filter(|c| c.applied).collect();
    assert_eq!(applied.len(), r.charges.len());
    for (c, want) in applied.iter().zip(&r.charges) {
        assert_eq!(&charge_preview(&b, &r, c).unwrap(), want, "{}", c.name);
    }
}

#[test]
fn preview_of_an_edited_charge_matches_recalculating_with_the_edit() {
    let b = sample();
    let r = run(&b);
    // The insurance charge with a new percent and a new excluded amount, as
    // the editor has them while they are still being typed.
    let mut edited = b.charges[1].clone();
    edited.kind = ChargeKind::Percent(d("3.75"));
    edited.excluded_amounts.push(("Typed".into(), 12_345));
    edited.label_percent = true;
    let got = charge_preview(&b, &r, &edited).unwrap();

    let mut b2 = b.clone();
    b2.charges[1] = edited;
    let want = run(&b2);
    assert_eq!(got, want.charges[1]);
}

#[test]
fn an_unapplied_charge_previews_as_if_applied() {
    let b = sample();
    let r = run(&b);
    let unapplied = b.charges.iter().find(|c| !c.applied).unwrap().clone();
    let got = charge_preview(&b, &r, &unapplied).unwrap();

    let mut b2 = b.clone();
    for c in &mut b2.charges {
        c.applied = c.name == unapplied.name;
    }
    let want = run(&b2);
    assert_eq!(got, want.charges[0]);
}

#[test]
fn preview_reports_an_unknown_currency() {
    let b = sample();
    let r = run(&b);
    let fee = Charge {
        kind: ChargeKind::Flat {
            amount: 100,
            currency: Some("XXX".into()),
        },
        ..percent_charge("Fee", "0")
    };
    let e = charge_preview(&b, &r, &fee).unwrap_err();
    assert_eq!(e.at, "charge Fee");
}
