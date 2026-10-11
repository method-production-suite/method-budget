//! Hand-written round trips of the shared fixtures.

mod common;

use common::*;
use method_core::*;
use serde_json::Value;

/// Asserts the round-trip and determinism contract and returns the text.
fn round_trip(b: &Budget) -> String {
    let text = method_format::to_string(b);
    let back = method_format::from_str(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));
    assert_eq!(&back, b);
    assert_eq!(
        method_format::to_string(&back),
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
    let text = method_format::to_string(&rich());
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
    assert!(text.starts_with("{\n  \"format\": \"method-budget\",\n  \"version\": 1,\n"));
}

#[test]
fn row_style_separators_and_group_colors_round_trip() {
    let mut b = Budget::new("T", "USD");
    b.units.clear();
    b.groups.insert(
        "Alt".into(),
        Group {
            short_code: None,
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
    styled.notes = vec![Note {
        id: 2,
        text: "Rate per UPM\nconfirm".into(),
        created: "2026-10-09T14:30:00Z".into(),
        edited: Some("2026-10-10T08:00:00Z".into()),
        author: Some("Alex".into()),
        done: true,
    }];
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
    for w in ["location", "bold", "italic", "indent", "color", "notes"].windows(2) {
        assert!(at(w[0]) < at(w[1]), "{} before {}", w[0], w[1]);
    }
    assert_eq!(keys.len(), 8);
    assert!(text.contains("\"indent\": 2"), "indent is a JSON integer");
}

#[test]
fn new_line_defaults_round_trip_and_are_left_out_at_default() {
    let mut b = rich();
    assert_eq!(b.new_line_defaults, NewLineDefaults::default());
    let text = round_trip(&b);
    assert!(!text.contains("new_line_defaults"), "default is left out");

    b.new_line_defaults = NewLineDefaults {
        fringes: false,
        groups: true,
        set: true,
        location: false,
        currency: true,
    };
    let text = round_trip(&b);
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        v["budget"]["new_line_defaults"],
        serde_json::json!({
            "fringes": false,
            "groups": true,
            "set": true,
            "location": false,
            "currency": true
        })
    );
    let pos = |k: &str| text.find(&format!("\"{k}\":")).unwrap();
    assert!(pos("fringe_posting") < pos("new_line_defaults"));
    assert!(pos("new_line_defaults") < pos("info"));

    // Versions keep their own copy of the setting.
    b.versions.push(Version {
        id: 1,
        name: "v1".into(),
        created: "2026-10-09T00:00:00Z".into(),
        note: None,
        budget: std::sync::Arc::new(b.snapshot()),
    });
    b.new_line_defaults = NewLineDefaults::default();
    let text = round_trip(&b);
    assert_eq!(
        text.matches("\"new_line_defaults\"").count(),
        1,
        "only the version's"
    );

    // When present, every switch is required, and nothing else is allowed.
    let file = |body: &str| {
        format!(
            "{{\"format\": \"method-budget\", \"version\": 1, \"budget\": \
             {{\"title\": \"T\", \"base_currency\": \"USD\", \"new_line_defaults\": {body}}}}}"
        )
    };
    let ok = file(
        r#"{"fringes": true, "groups": true, "set": false, "location": false, "currency": false}"#,
    );
    assert!(
        method_format::from_str(&ok)
            .unwrap()
            .new_line_defaults
            .groups
    );
    assert!(method_format::from_str(&file(r#"{"fringes": true}"#)).is_err());
    let extra = file(
        r#"{"fringes": true, "groups": true, "set": false, "location": false, "currency": false, "notes": true}"#,
    );
    assert!(method_format::from_str(&extra).is_err());
}

#[test]
fn files_without_new_fields_read_with_defaults() {
    let golden = include_str!("golden/small.json");
    let b = method_format::from_str(golden).unwrap();
    assert!(b.groups.values().all(|g| g.color.is_none()));
    for d in b
        .categories()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.details)
    {
        assert!(!d.bold && !d.italic && d.indent == 0);
        assert_eq!(&d.color, &None);
        assert!(d.notes.is_empty());
        assert_ne!(d.kind, RowKind::Separator);
    }
    assert_eq!(b.new_line_defaults, NewLineDefaults::default());
    assert_eq!(method_format::to_string(&b), golden, "golden is unchanged");
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
    let alone = method_format::to_string(&b.versions[1].budget.to_budget());
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
    assert_eq!(method_format::to_string(&b), text);
}

#[test]
fn a_single_note_from_an_older_file_reads_as_one_undated_note() {
    let text = r#"{"format": "method-budget", "version": 1, "budget": {
      "title": "Old", "base_currency": "USD",
      "topsheet": [{"kind": "category", "code": "1", "name": "C", "accounts": [
        {"code": "1-1", "name": "A", "details": [
          {"id": 1, "description": "Grip", "note": "Rate per UPM"},
          {"id": 2, "description": "Best boy", "note": "Old one",
           "notes": [{"id": 1, "created": "2026-10-09T14:30:00Z", "text": "New one"}]},
          {"id": 3, "description": "Dolly"}
        ]}
      ]}]
    }}"#;
    let b = method_format::from_str(text).unwrap();
    let rows: Vec<&Detail> = b
        .categories()
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.details)
        .collect();
    assert_eq!(rows[0].notes, vec![Note::plain(1, "Rate per UPM")]);
    // Both: the old note first, with an id after the list's.
    assert_eq!(rows[1].notes.len(), 2);
    assert_eq!(rows[1].notes[0], Note::plain(2, "Old one"));
    assert_eq!(rows[1].notes[1].text, "New one");
    assert_eq!(rows[1].notes[1].created, "2026-10-09T14:30:00Z");
    assert!(rows[2].notes.is_empty());
    // Written back as the new field only, and stable from then on.
    let out = method_format::to_string(&b);
    assert!(!out.contains("\"note\":"), "{out}");
    assert!(out.contains("\"notes\": ["));
    assert_eq!(method_format::from_str(&out).unwrap(), b);
    // A line block's row reads the old field the same way.
    let block = r#"{"format": "method-budget", "version": 1, "budget": {
      "title": "Old", "base_currency": "USD",
      "line_presets": [{"name": "B", "rows": [{"description": "x", "note": "kept"}]}]
    }}"#;
    let b = method_format::from_str(block).unwrap();
    assert_eq!(
        b.line_presets[0].rows[0].notes,
        vec![Note::plain(1, "kept")]
    );
}

#[test]
fn notes_on_accounts_and_categories_short_codes_and_order_round_trip() {
    let mut b = rich();
    b.fringes.values_mut().next().unwrap().short_code = Some("FICA".into());
    let note = Note {
        id: 4,
        text: "Ask the studio".into(),
        created: "2026-10-09T10:00:00Z".into(),
        edited: None,
        author: None,
        done: false,
    };
    for c in b.categories_mut() {
        c.notes = vec![note.clone()];
        for a in &mut c.accounts {
            a.notes = vec![Note {
                done: true,
                ..note.clone()
            }];
        }
    }
    let names = method_core::order::names(&b, SetupKind::Fringe);
    if names.len() > 1 {
        let reversed: Vec<String> = names.iter().rev().cloned().collect();
        method_core::order::set_order(&mut b, SetupKind::Fringe, &reversed);
        assert!(!b.setup_order.is_empty());
    }
    let text = round_trip(&b);
    let v: Value = serde_json::from_str(&text).unwrap();
    let cat = &v["budget"]["topsheet"][0];
    assert_eq!(
        cat["notes"],
        serde_json::json!([{"id": 4, "created": "2026-10-09T10:00:00Z", "text": "Ask the studio"}])
    );
    assert_eq!(cat["accounts"][0]["notes"][0]["done"], true);
    assert!(text.contains("\"short_code\": \"FICA\""));
    // A production total has no notes.
    let bad = text.replacen(
        "\"kind\": \"production_total\",",
        "\"kind\": \"production_total\", \"notes\": [],",
        1,
    );
    if bad != text {
        assert!(method_format::from_str(&bad).is_err());
    }
}

#[test]
fn a_setup_order_in_byte_order_or_naming_nothing_is_left_out() {
    let mut b = rich();
    let names = method_core::order::names(&b, SetupKind::Fringe);
    b.setup_order.fringes = names.clone();
    b.setup_order.sets = vec!["no such set".into()];
    let text = method_format::to_string(&b);
    assert!(!text.contains("setup_order"), "{text}");
    // A version's budget may not have one.
    let doc = r#"{"format": "method-budget", "version": 1, "budget": {
      "title": "T", "base_currency": "USD",
      "versions": [{"id": 1, "name": "v", "created": "", "budget": {
        "title": "T", "base_currency": "USD", "setup_order": {"sets": ["a"]}}}]
    }}"#;
    assert!(method_format::from_str(doc).is_err());
}

#[test]
fn charge_label_switches_round_trip_and_are_left_out_when_off() {
    let mut b = sample();
    let text = method_format::to_string(&b);
    assert!(!text.contains("label_percent") && !text.contains("label_excluded"));

    b.charges[0].label_percent = true;
    b.charges[2].label_excluded = true; // a flat charge
    let text = round_trip(&b);
    let v: Value = serde_json::from_str(&text).unwrap();
    let charges = &v["budget"]["charges"];
    assert_eq!(charges[0]["label_percent"], true);
    assert!(charges[0].get("label_excluded").is_none());
    assert_eq!(charges[2]["label_excluded"], true);
    assert!(charges[2].get("label_percent").is_none());
    // Written last in its charge, after the percent.
    let at = |key: &str| text.find(key).unwrap();
    let label = at("\"label_percent\"");
    assert!(at("\"percent\": \"10\"") < label && label < at("\"Insurance\""));

    // A switch that isn't a boolean is refused.
    let bad = text.replacen("\"label_percent\": true", "\"label_percent\": \"yes\"", 1);
    assert!(method_format::from_str(&bad).is_err());
}
