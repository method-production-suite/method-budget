//! The Credits tab's incentive programs, headless: a credit limited to
//! locations through `set_credit`, and every program's estimate side by side
//! through `incentives`, which changes nothing.

use method_app::app::incentives_in;
use method_app::edit::{self, CreditInput, LocationInput};
use method_app::Session;
use method_core::{Budget, Detail};
use serde_json::{json, Value};

fn credit(v: Value) -> CreditInput {
    serde_json::from_value(v).unwrap()
}

/// The new budget's first account with a 1,000.00 line at Ridge and a
/// 500.00 line at Coast, and both locations in the setup.
fn session() -> Session {
    let mut b: Budget = Session::new().budget().clone();
    for name in ["Ridge", "Coast"] {
        let l = LocationInput {
            description: String::new(),
            city: String::new(),
            region: String::new(),
            country: String::new(),
        };
        edit::set_location(&mut b, name, &l).unwrap();
    }
    let next = b
        .categories()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.details)
        .map(|d| d.id)
        .max()
        .unwrap_or(0);
    let acct = b
        .categories_mut()
        .flat_map(|c| &mut c.accounts)
        .next()
        .unwrap();
    let mut ridge = Detail::line(next + 1, "Ridge crew", "1", "", "1000");
    ridge.location = Some("Ridge".into());
    let mut coast = Detail::line(next + 2, "Coast crew", "1", "", "500");
    coast.location = Some("Coast".into());
    acct.details = vec![ridge, coast];
    let code = acct.code.clone();
    let program = credit(json!({
        "name": "Ridge program", "applied": false, "cap": "200", "discount_percent": "10",
        "locations": ["Ridge"],
        "rows": [{"description": "Spend", "source": {"kind": "account", "account": code},
                  "percent": "25", "cap": {"kind": "none"}}]
    }));
    edit::set_credit(&mut b, None, &program).unwrap();
    Session::with_budget(b, None)
}

#[test]
fn a_program_is_limited_to_its_locations_and_estimated_unapplied() {
    let mut s = session();
    let c = s.budget().credits.last().unwrap().clone();
    assert_eq!(c.locations.iter().collect::<Vec<_>>(), ["Ridge"]);
    let before = s.budget().clone();

    let got = serde_json::to_value(incentives_in(&mut s).unwrap()).unwrap();
    assert_eq!(s.budget(), &before, "estimating never edits");
    let p = got["programs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "Ridge program")
        .unwrap();
    assert_eq!(p["applied"], false);
    assert_eq!(p["locations"], json!(["Ridge"]));
    // 25% of 1,000.00 is 250.00, capped at 200.00, less 10%.
    assert_eq!(p["qualified"], 100_000);
    assert_eq!(p["capped"], true);
    assert_eq!(p["amount"], 18_000);
    assert_eq!(
        p["by_location"],
        json!([{"location": "Ridge", "net": 100_000}])
    );
    let spend: Vec<(Value, Value)> = got["locations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| (l["location"].clone(), l["total"].clone()))
        .collect();
    assert!(
        spend.contains(&(json!("Ridge"), json!(100_000))),
        "{spend:?}"
    );
    assert!(
        spend.contains(&(json!("Coast"), json!(50_000))),
        "{spend:?}"
    );
}

#[test]
fn set_credit_refuses_a_location_the_budget_doesnt_have() {
    let mut b = session().budget().clone();
    let bad = credit(json!({"name": "Harbor program", "locations": ["Harbor"]}));
    let e = edit::set_credit(&mut b, None, &bad).unwrap_err();
    assert!(e.contains("Harbor"), "{e}");
}

/// A budget with problems is figured from a cleaned copy, which counts a
/// credit row on a missing group as 0: the reply says the figures are
/// partial and names the program's problem, so a short estimate is never
/// shown as if it were whole.
#[test]
fn estimates_from_a_cleaned_copy_say_so() {
    let mut s = session();
    let clean = serde_json::to_value(incentives_in(&mut s).unwrap()).unwrap();
    assert_eq!(clean["partial"], false);
    assert!(clean["programs"][0]["problem"].is_null());

    let mut b = s.budget().clone();
    let c = b.credits.last_mut().unwrap();
    let mut gone = c.rows[0].clone();
    gone.source = method_core::CreditSource::Group {
        group: "Gone".into(),
    };
    c.rows.push(gone);

    // Not applied, the totals don't need it: the engine says it can't be
    // worked out and gives no figure.
    let mut s = Session::with_budget(b.clone(), None);
    let got = serde_json::to_value(incentives_in(&mut s).unwrap()).unwrap();
    assert_eq!(got["partial"], false);
    let p = &got["programs"][0];
    assert!(p["amount"].is_null());
    assert!(p["problem"].as_str().unwrap_or("").contains("Gone"));

    // Applied, the totals come from the cleaned copy.
    b.credits.last_mut().unwrap().applied = true;
    let mut s = Session::with_budget(b, None);
    let got = serde_json::to_value(incentives_in(&mut s).unwrap()).unwrap();
    assert_eq!(got["partial"], true);
    let p = got["programs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "Ridge program")
        .unwrap();
    let problem = p["problem"].as_str().unwrap_or("");
    assert!(problem.contains("Gone"), "{problem}");
    assert!(problem.contains("short"), "{problem}");
    // The figure from the cleaned copy is still there, flagged.
    assert_eq!(p["amount"], 18_000);
}

/// A minimum spend, a flat broker's fee and a cap shared with another
/// program (decisions.md, entry 88), set through `set_credit` and shown
/// side by side.
#[test]
fn set_credit_takes_a_minimum_a_fee_and_a_shared_cap() {
    let mut b = session().budget().clone();
    let code = b
        .categories()
        .flat_map(|c| &c.accounts)
        .next()
        .unwrap()
        .code
        .clone();
    let row = json!([{"description": "Spend", "source": {"kind": "account", "account": code},
                      "percent": "25", "cap": {"kind": "none"}}]);
    // A shared cap that is new needs an amount.
    let e = edit::set_credit(
        &mut b,
        Some(0),
        &credit(json!({"name": "Ridge program", "shared_cap": "State", "rows": row})),
    )
    .unwrap_err();
    assert!(e.contains("needs an amount"), "{e}");
    // An amount needs a name.
    let e = edit::set_credit(
        &mut b,
        Some(0),
        &credit(json!({"name": "Ridge program", "shared_cap_amount": "300", "rows": row})),
    )
    .unwrap_err();
    assert!(e.contains("needs a shared cap name"), "{e}");
    // A fee can't be less than zero.
    assert!(edit::set_credit(
        &mut b,
        Some(0),
        &credit(json!({"name": "Ridge program", "broker_fee": "-1", "rows": row})),
    )
    .is_err());

    edit::set_credit(
        &mut b,
        Some(0),
        &credit(json!({
            "name": "Ridge program", "applied": true, "locations": ["Ridge"],
            "minimum_spend": "500", "broker_fee": "10",
            "shared_cap": "State", "shared_cap_amount": "300", "rows": row
        })),
    )
    .unwrap();
    // The second program joins the cap without restating its amount.
    edit::set_credit(
        &mut b,
        None,
        &credit(json!({
            "name": "Coast program", "applied": true, "locations": ["Coast"],
            "shared_cap": "State", "rows": row
        })),
    )
    .unwrap();
    let c = &b.credits[0];
    assert_eq!(
        (c.minimum_spend, c.broker_fee, c.shared_cap.as_deref()),
        (Some(50_000), 1_000, Some("State"))
    );
    assert_eq!(b.shared_caps.get("State"), Some(&30_000));

    let mut s = Session::with_budget(b.clone(), None);
    let got = serde_json::to_value(incentives_in(&mut s).unwrap()).unwrap();
    let p = &got["programs"][0];
    assert_eq!(p["minimum_spend"], 50_000);
    assert_eq!(p["broker_fee"], 1_000);
    assert_eq!(p["shared_cap"], "State");
    assert_eq!(p["shared_cap_amount"], 30_000);
    // 250.00 and 125.00 share 300.00: 200.00 and 100.00; less the 10.00 fee.
    assert_eq!(p["amount"], 19_000);
    assert_eq!(p["figures"]["shared"], 20_000);
    assert_eq!(p["figures"]["fee"], 1_000);
    assert_eq!(got["programs"][1]["amount"], 10_000);

    // Below its minimum the estimate is 0 and the figures say why.
    b.credits[0].minimum_spend = Some(1_000_000);
    let mut s = Session::with_budget(b.clone(), None);
    let got = serde_json::to_value(incentives_in(&mut s).unwrap()).unwrap();
    assert_eq!(got["programs"][0]["amount"], 0);
    assert_eq!(got["programs"][0]["figures"]["below_minimum"], true);
    assert_eq!(got["programs"][1]["amount"], 12_500);

    // Taking every credit off a shared cap takes the cap off too.
    edit::delete_credit(&mut b, 1).unwrap();
    assert!(b.shared_caps.contains_key("State"));
    edit::set_credit(
        &mut b,
        Some(0),
        &credit(json!({"name": "Ridge program", "rows": row})),
    )
    .unwrap();
    assert!(b.shared_caps.is_empty());
}
