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
