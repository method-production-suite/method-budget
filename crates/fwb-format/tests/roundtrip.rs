//! Hand-written round trips of the shared fixtures.

mod common;

use common::*;
use fwb_core::*;
use serde_json::Value;

/// Asserts the round-trip and determinism contract and returns the text.
fn round_trip(b: &Budget) -> String {
    let text = fwb_format::to_string(b);
    let back = fwb_format::from_str(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));
    assert_eq!(&back, b);
    assert_eq!(
        fwb_format::to_string(&back),
        text,
        "rewrite is byte-identical"
    );
    text
}

/// Every JSON number in the file is an integer: decimals are strings.
fn assert_no_floats(v: &Value) {
    match v {
        Value::Number(n) => assert!(n.is_i64() || n.is_u64(), "float in output: {n}"),
        Value::Array(items) => items.iter().for_each(assert_no_floats),
        Value::Object(m) => m.values().for_each(assert_no_floats),
        _ => {}
    }
}

#[test]
fn new_budget_round_trips() {
    let text = round_trip(&Budget::new("Untitled", "USD"));
    assert!(text.ends_with("}\n") && !text.ends_with("\n\n"));
    // Starter units are data, so they are written out.
    assert!(text.contains("\"Week\""));
    assert!(!text.contains("fringe_posting") && !text.contains("account_coding"));
}

#[test]
fn sample_round_trips() {
    round_trip(&sample());
}

#[test]
fn rich_budget_round_trips() {
    let b = rich();
    let text = round_trip(&b);
    assert_no_floats(&serde_json::from_str(&text).unwrap());
    // Spot checks on the representation.
    assert!(text.contains("\"rate\": \"0.73\""), "Dec is a string");
    assert!(text.contains("\"cutoff\": 1000000"), "Cents is an integer");
    assert!(text.contains("\"separator\": \".\""));
    assert!(text.contains("\"kind\": \"production_level\""));
    assert!(text.contains("\"in_budget_total\": false"));
    assert!(text.contains("\"applied\": false"));
    assert!(
        text.contains("\"18446744073709551615\": 7"),
        "row ids key the baseline"
    );
    assert!(
        text.contains("\"x4\": \"  \""),
        "whitespace formulas are kept"
    );
}

#[test]
fn removing_coding_and_posting_round_trips_to_defaults() {
    let mut b = rich();
    b.account_coding = Default::default();
    b.fringe_posting = FringePosting::default();
    let text = round_trip(&b);
    assert!(!text.contains("account_coding") && !text.contains("fringe_posting"));

    b.account_coding.separator = None;
    b.fringe_posting = FringePosting::Category {
        suffix: String::new(),
    };
    let text = round_trip(&b);
    assert!(text.contains("\"separator\": \"\""));
    assert!(text.contains("\"suffix\": \"\""));
}

#[test]
fn empty_and_default_values_are_omitted() {
    let mut b = Budget::new("T", "USD");
    b.units.clear();
    b.topsheet.push(category(
        "1",
        "C",
        vec![account("11", "A", vec![Detail::subtotal(1, "")])],
    ));
    let text = round_trip(&b);
    let v: Value = serde_json::from_str(&text).unwrap();
    let budget = v["budget"].as_object().unwrap();
    let keys: Vec<&str> = budget.keys().map(String::as_str).collect();
    assert_eq!(keys, ["base_currency", "title", "topsheet"]);
    let detail = &v["budget"]["topsheet"][0]["accounts"][0]["details"][0];
    assert_eq!(
        detail,
        &serde_json::json!({"id": 1, "kind": "subtotal", "description": ""})
    );
}

#[test]
fn key_order_is_fixed() {
    let text = fwb_format::to_string(&rich());
    let pos = |k: &str| {
        text.find(&format!("\"{k}\":"))
            .unwrap_or_else(|| panic!("{k} missing"))
    };
    let order = [
        "format",
        "version",
        "budget",
        "title",
        "base_currency",
        "totals_currency",
        "account_coding",
        "fringe_posting",
        "info",
        "currencies",
        "units",
        "globals",
        "fringes",
        "groups",
        "sets",
        "locations",
        "topsheet",
        "charges",
        "credits",
        "baseline",
    ];
    for w in order.windows(2) {
        assert!(pos(w[0]) < pos(w[1]), "{} before {}", w[0], w[1]);
    }
    assert!(text.starts_with("{\n  \"format\": \"film-wizard-budget\",\n  \"version\": 1,\n"));
}
