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

#[test]
fn row_style_separators_and_group_colors_round_trip() {
    let mut b = Budget::new("T", "USD");
    b.units.clear();
    b.groups.insert(
        "Alt".into(),
        Group {
            description: String::new(),
            in_budget_total: false,
            color: Some("#336699".into()),
        },
    );
    let mut styled = Detail::line(1, "Grip", "", "", "");
    styled.location = Some("LA".into());
    styled.bold = true;
    styled.italic = true;
    styled.indent = 2;
    styled.color = Some("#aa0000".into());
    styled.note = Some("Rate per UPM\nconfirm".into());
    let mut italic_only = Detail::line(3, "", "", "", "");
    italic_only.italic = true;
    b.topsheet.push(category(
        "1",
        "C",
        vec![account(
            "11",
            "A",
            vec![styled, Detail::separator(2), italic_only],
        )],
    ));
    let text = round_trip(&b);
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        v["budget"]["groups"]["Alt"],
        serde_json::json!({"in_budget_total": false, "color": "#336699"})
    );
    let details = &v["budget"]["topsheet"][0]["accounts"][0]["details"];
    assert_eq!(
        details[1],
        serde_json::json!({"id": 2, "kind": "separator", "description": ""})
    );
    assert_eq!(
        details[2],
        serde_json::json!({"id": 3, "description": "", "italic": true}),
        "style fields at their default are left out"
    );
    // Style fields come last, in documented order.
    let keys: Vec<&str> = details[0]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let row = &text[text.find("\"id\": 1,").unwrap()..];
    let at = |k: &str| row.find(&format!("\"{k}\":")).unwrap();
    for w in ["location", "bold", "italic", "indent", "color", "note"].windows(2) {
        assert!(at(w[0]) < at(w[1]), "{} before {}", w[0], w[1]);
    }
    assert_eq!(keys.len(), 8);
    assert!(text.contains("\"indent\": 2"), "indent is a JSON integer");
}

#[test]
fn files_without_new_fields_read_with_defaults() {
    let golden = include_str!("golden/small.json");
    let b = fwb_format::from_str(golden).unwrap();
    assert!(b.groups.values().all(|g| g.color.is_none()));
    for d in b
        .categories()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.details)
    {
        assert!(!d.bold && !d.italic && d.indent == 0);
        assert_eq!((&d.color, &d.note), (&None, &None));
        assert_ne!(d.kind, RowKind::Separator);
    }
    assert_eq!(fwb_format::to_string(&b), golden, "golden is unchanged");
}

#[test]
fn versions_round_trip_after_the_baseline() {
    let mut b = rich();
    assert!(b.baseline.is_some());
    let first = save_version(&mut b, "Studio draft", "2026-10-09T14:30:00Z", None);
    b.title = "Renamed".into();
    save_version(
        &mut b,
        "Line \"two\"\n",
        "2026-10-10T09:00:00-07:00",
        Some("after notes".into()),
    );
    let auto = restore_version(&mut b, first, "2026-10-11T08:00:00Z").unwrap();
    assert_eq!(b.versions.len(), 3);
    let text = round_trip(&b);
    assert_no_floats(&serde_json::from_str(&text).unwrap());

    // `versions` is the budget's last field, after `baseline`, and each
    // version's fields come in documented order.
    let v: Value = serde_json::from_str(&text).unwrap();
    let keys: Vec<&String> = v["budget"].as_object().unwrap().keys().collect();
    assert_eq!(keys.last().unwrap().as_str(), "versions");
    let pos = |k: &str| text.find(k).unwrap_or_else(|| panic!("{k} missing"));
    assert!(pos("\n    \"baseline\":") < pos("\n    \"versions\":"));
    let version = &v["budget"]["versions"][2];
    assert_eq!(
        version.as_object().unwrap().keys().collect::<Vec<_>>(),
        vec!["budget", "created", "id", "name"],
        "note left out when not set"
    );
    let second = &text[pos("\"name\": \"Line")..];
    let at = |k: &str| second.find(&format!("\"{k}\":")).unwrap();
    assert!(at("created") < at("note") && at("note") < at("budget"));
    let first = &text[pos("\n    \"versions\":")..];
    let at = |k: &str| first.find(&format!("\"{k}\":")).unwrap();
    assert!(at("id") < at("name") && at("name") < at("created") && at("created") < at("budget"));
    assert_eq!(version["id"], auto);
    assert!(version["budget"].get("versions").is_none());
    assert_eq!(v["budget"]["versions"][1]["note"], "after notes");
    // A version's budget is written exactly like a top-level budget.
    let alone = fwb_format::to_string(&b.versions[1].budget.to_budget());
    let alone: Value = serde_json::from_str(&alone).unwrap();
    assert_eq!(v["budget"]["versions"][1]["budget"], alone["budget"]);
}

#[test]
fn no_versions_writes_no_versions_field() {
    let text = round_trip(&sample());
    assert!(!text.contains("\"versions\""));
    let mut b = sample();
    let id = save_version(&mut b, "v", "t", None);
    delete_version(&mut b, id).unwrap();
    assert_eq!(fwb_format::to_string(&b), text);
}
