//! The Field Station fixtures (fixtures/field-station/, the suite's invented
//! production the README and site pictures show): the budget is in
//! canonical form, as `method fmt` writes it, and it agrees with the
//! schedule days file Method Schedule wrote for it, read with our own
//! reader. `python3 scripts/make-field-station.py --check` makes the budget
//! again from that file and compares every byte.

use std::path::PathBuf;

use method_format::import::schedule;
use method_format::serde_json::{self, Value};

fn fixture(name: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/field-station")
        .join(name);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

#[test]
fn the_budget_is_canonical() {
    let text = fixture("field-station.mthd");
    let budget = method_format::from_str(&text).unwrap();
    assert_eq!(method_format::to_string(&budget), text);
}

#[test]
fn the_budget_agrees_with_its_schedule() {
    let days = schedule::read(fixture("field-station.budget.json").as_bytes()).unwrap();
    assert!(days.notes.is_empty(), "{:?}", days.notes);
    let doc: Value = serde_json::from_str(&fixture("field-station.mthd")).unwrap();
    let b = &doc["budget"];
    assert_eq!(b["title"], days.schedule.as_str());

    let global = |name: &str| b["globals"][name].as_str().unwrap_or("").to_string();
    assert_eq!(
        global(schedule::SHOOT_DAYS_GLOBAL),
        days.shoot_days.total.to_string()
    );
    let at = |name: &str| {
        let (_, n) = days
            .shoot_days
            .by_location
            .iter()
            .find(|(l, _)| l == name)
            .unwrap_or_else(|| panic!("no location {name}"));
        n.to_string()
    };
    assert_eq!(global("STATION_DAYS"), at("Cold Spring field station"));
    assert_eq!(global("RIDGE_DAYS"), at("Hollins Ridge fire road"));

    for f in days.budget_fields() {
        if let schedule::BudgetField::Date { label, date } = f {
            assert_eq!(b["info"][label]["value"], date.as_str(), "{label}");
        }
    }

    // Each cast member's line counts their work days.
    let lines: Vec<&Value> = b["topsheet"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r["accounts"].as_array())
        .flatten()
        .filter_map(|a| a["details"].as_array())
        .flatten()
        .collect();
    for e in days.elements.iter().filter(|e| e.category == "Cast") {
        let line = lines
            .iter()
            .find(|l| {
                l["description"]
                    .as_str()
                    .is_some_and(|d| d.split(',').next() == Some(e.name.as_str()))
            })
            .unwrap_or_else(|| panic!("no line for {}", e.name));
        assert_eq!(line["amount"], e.work.to_string().as_str(), "{}", e.name);
    }
}
