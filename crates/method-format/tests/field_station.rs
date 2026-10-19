//! The Field Station fixtures (fixtures/field-station/, the suite's invented
//! production: the app's sample and the README and site pictures): the budget
//! is in canonical form, as `method-budget fmt` writes it, and it agrees with
//! the schedule days file it was made from (Method Schedule's export of its
//! own Field Station sample), read with our own reader.
//! `python3 scripts/make-field-station.py --check` makes the budget again
//! from that file and compares every byte.

use std::path::PathBuf;

use method_format::import::schedule;
use method_format::serde_json::{self, Value};

/// The schedule days file the budget is made from.
const DAYS: &str = "field-station.budget.json";

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
fn the_schedule_file_reads_without_notes() {
    let days = schedule::read(fixture(DAYS).as_bytes()).unwrap();
    assert!(days.notes.is_empty(), "{:?}", days.notes);
    assert_eq!(days.schedule, "Field Station");
    assert_eq!(days.shoot_days.total, 18);
}

#[test]
fn the_budget_agrees_with_its_schedule() {
    let days = schedule::read(fixture(DAYS).as_bytes()).unwrap();
    let doc: Value = serde_json::from_str(&fixture("field-station.mthd")).unwrap();
    let b = &doc["budget"];
    assert_eq!(b["title"], days.schedule.as_str());

    let global = |name: &str| b["globals"][name].as_str().unwrap_or("").to_string();
    assert_eq!(
        global(schedule::SHOOT_DAYS_GLOBAL),
        days.shoot_days.total.to_string()
    );
    let count = |list: &[(String, u32)], name: &str| {
        let (_, n) = list
            .iter()
            .find(|(l, _)| l == name)
            .unwrap_or_else(|| panic!("no {name}"));
        n.to_string()
    };
    let d = &days.shoot_days;
    assert_eq!(
        global("STATION_DAYS"),
        count(&d.by_location, "Cold Spring field station")
    );
    assert_eq!(
        global("RIDGE_DAYS"),
        count(&d.by_location, "Hollins Ridge fire road")
    );
    assert_eq!(
        global("LAKE_DAYS"),
        count(&d.by_location, "Kettle Lake boathouse")
    );
    assert_eq!(global("SECOND_UNIT_DAYS"), count(&d.by_unit, "Second Unit"));

    for f in days.budget_fields() {
        if let schedule::BudgetField::Date { label, date } = f {
            assert_eq!(b["info"][label]["value"], date.as_str(), "{label}");
        }
    }

    // Each cast member's line counts their days through a global set from
    // the schedule: the day out of days total.
    let lines: Vec<&Value> = b["topsheet"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r["accounts"].as_array())
        .flatten()
        .filter_map(|a| a["details"].as_array())
        .flatten()
        .collect();
    let cast: Vec<_> = days
        .elements
        .iter()
        .filter(|e| e.category == "Cast")
        .collect();
    assert_eq!(
        cast.len(),
        9,
        "four leads, four day players and a silent part"
    );
    for e in cast {
        let line = lines
            .iter()
            .find(|l| {
                l["description"]
                    .as_str()
                    .is_some_and(|d| d.split(',').next() == Some(e.name.as_str()))
            })
            .unwrap_or_else(|| panic!("no line for {}", e.name));
        let name = line["amount"].as_str().unwrap();
        assert!(name.ends_with("_DAYS"), "{}: {name}", e.name);
        assert_eq!(global(name), e.total.to_string(), "{}", e.name);
        let source = b["setup_sources"]["global"][name]["notes"]
            .as_str()
            .unwrap_or_else(|| panic!("{name} has no source"));
        assert!(
            source.contains(&format!("[key: element:{} | Cast", e.id)),
            "{source}"
        );
    }
}

/// The sample shows every feature group (SUITE-STANDARDS.md, section 8).
#[test]
fn the_sample_shows_the_features() {
    let doc: Value = serde_json::from_str(&fixture("field-station.mthd")).unwrap();
    let b = &doc["budget"];
    let has = |k: &str| b[k].as_object().is_some_and(|m| !m.is_empty());
    for k in [
        "globals",
        "fringes",
        "groups",
        "sets",
        "locations",
        "currencies",
    ] {
        assert!(has(k), "{k}");
    }
    assert!(b["groups"]
        .as_object()
        .unwrap()
        .values()
        .any(|g| g["in_budget_total"] == false));
    assert_eq!(b["charges"].as_array().unwrap().len(), 2);
    let credit = &b["credits"][0];
    assert_eq!(credit["applied"], false);
    assert_eq!(credit["locations"].as_array().unwrap().len(), 1);
    assert_eq!(b["baseline"]["frozen"], true);
    assert_eq!(b["versions"].as_array().unwrap().len(), 1);
    assert!(!b["actuals"]["transactions"].as_array().unwrap().is_empty());
    let text = fixture("field-station.mthd");
    for needle in [
        "\"person\"",
        "\"currency\": \"EUR\"",
        "\"notes\"",
        "\"cutoff\"",
    ] {
        assert!(text.contains(needle), "{needle}");
    }
}
