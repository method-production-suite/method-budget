//! The charges tab's live preview: a charge figured with a value still being
//! typed, without changing the budget.

use method_app::app::charge_preview_in;
use method_app::edit::{self, ChargeInput};
use method_app::Session;
use serde_json::{json, Value};

fn input(v: Value) -> ChargeInput {
    serde_json::from_value(v).unwrap()
}

fn report(s: &mut Session) -> Value {
    serde_json::to_value(s.full_view()).unwrap()["report"].clone()
}

#[test]
fn preview_matches_saving_the_edit_and_changes_nothing() {
    let mut s = Session::new();
    let before = s.budget().clone();
    let typed = input(json!({
        "name": "Contingency", "kind": "percent", "percent": "7.5",
        "excluded_amounts": [{"description": "Deposit", "amount": "2,500"}]
    }));
    let got = charge_preview_in(&mut s, Some(0), &typed).unwrap();
    assert_eq!(s.budget(), &before, "a preview never edits");
    assert!(!s.is_dirty());

    // Saving the same values gives the same figures.
    let mut b = before.clone();
    edit::set_charge(&mut b, Some(0), &typed).unwrap();
    let mut saved = Session::with_budget(b, None);
    let r = report(&mut saved);
    assert_eq!(r["charges"][0]["amount"], got.amount);
    assert_eq!(r["charges"][0]["excluded"], got.excluded);
    assert_eq!(r["charges"][0]["base"], got.base);
    assert_eq!(got.excluded, 250_000);
}

#[test]
fn a_value_that_is_not_a_number_yet_is_refused() {
    let mut s = Session::new();
    let half = input(json!({"name": "Contingency", "kind": "percent", "percent": "1x"}));
    assert!(charge_preview_in(&mut s, Some(0), &half).is_err());
    let gone = input(json!({"name": "Contingency", "kind": "percent", "percent": "1"}));
    assert!(charge_preview_in(&mut s, Some(42), &gone).is_err());
}
