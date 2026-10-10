//! Global info (description, unit, decimals, show by name, folder) and the
//! budget's display settings: written only when set, read back exactly, and
//! never inside a version.

use fwb_core::*;

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

fn budget() -> Budget {
    let mut b = Budget::new("Globals", "USD");
    b.globals.insert("DAYS".into(), "30".into());
    b.globals.insert("WEEKS".into(), "DAYS / 5".into());
    b
}

#[test]
fn nothing_set_writes_nothing() {
    let text = round_trip(&budget());
    assert!(!text.contains("global_info") && !text.contains("\"display\""));
}

#[test]
fn global_info_round_trips_after_globals() {
    let mut b = budget();
    b.global_info.insert(
        "DAYS".into(),
        GlobalInfo {
            description: "Shoot days".into(),
            unit: "Day".into(),
            decimals: Some(0),
            show_by_name: true,
            folder: "Schedule".into(),
        },
    );
    b.global_info.insert(
        "WEEKS".into(),
        GlobalInfo {
            decimals: Some(8),
            ..GlobalInfo::default()
        },
    );
    let text = round_trip(&b);
    let globals = text.find("\"globals\"").unwrap();
    let info = text.find("\"global_info\"").unwrap();
    let fringes_or_end = text.find("\"topsheet\"").unwrap_or(text.len());
    assert!(globals < info && info < fringes_or_end, "{text}");
    // Defaults are left out field by field.
    let weeks = &text[text.find("\"WEEKS\": {").unwrap()..];
    let weeks = &weeks[..weeks.find('}').unwrap()];
    assert!(
        weeks.contains("\"decimals\": 8") && !weeks.contains("show_by_name"),
        "{weeks}"
    );
}

#[test]
fn info_without_a_global_or_with_only_defaults_is_not_written() {
    let mut b = budget();
    b.global_info.insert(
        "GONE".into(),
        GlobalInfo {
            show_by_name: true,
            ..GlobalInfo::default()
        },
    );
    b.global_info.insert("DAYS".into(), GlobalInfo::default());
    let text = fwb_format::to_string(&b);
    assert!(!text.contains("global_info"), "{text}");
    assert!(fwb_format::from_str(&text).unwrap().global_info.is_empty());
}

#[test]
fn info_for_a_name_that_is_not_a_global_is_ignored_on_read() {
    let text = r#"{
  "format": "film-wizard-budget",
  "version": 1,
  "budget": {
    "title": "T",
    "base_currency": "USD",
    "globals": { "DAYS": "5" },
    "global_info": {
      "DAYS": { "folder": "Schedule" },
      "NOPE": { "description": "left behind" }
    }
  }
}
"#;
    let b = fwb_format::from_str(text).unwrap();
    assert_eq!(b.global_info.len(), 1);
    assert_eq!(b.global_info["DAYS"].folder, "Schedule");
}

#[test]
fn too_many_decimal_places_are_refused() {
    let doc = |field: &str| {
        format!(
            r#"{{"format":"film-wizard-budget","version":1,"budget":{{"title":"T","base_currency":"USD","globals":{{"D":"1"}},{field}}}}}"#
        )
    };
    let e = fwb_format::from_str(&doc(r#""global_info":{"D":{"decimals":9}}"#)).unwrap_err();
    assert!(
        e.to_string().contains("decimal places must be 0 to 8"),
        "{e}"
    );
    let e = fwb_format::from_str(&doc(r#""display":{"rate_decimals_above":12}"#)).unwrap_err();
    assert!(
        e.to_string().contains("decimal places must be 0 to 8"),
        "{e}"
    );
    assert!(fwb_format::from_str(&doc(r#""display":{"rate_decimals_below":2.0}"#)).is_err());
    assert!(fwb_format::from_str(&doc(r#""global_info":{"D":{"color":"red"}}"#)).is_err());
}

#[test]
fn display_round_trips_and_leaves_defaults_out() {
    let mut b = budget();
    b.display = DisplayPrefs {
        rate_cutoff: Some("10.5".parse().unwrap()),
        rate_decimals_below: None,
        rate_decimals_above: Some(2),
        rate_global_decimals: true,
        summary_rows: false,
        net_total: true,
    };
    let text = round_trip(&b);
    let display = &text[text.find("\"display\": {").unwrap()..];
    let display = &display[..display.find('}').unwrap()];
    assert!(display.contains("\"rate_cutoff\": \"10.5\""), "{display}");
    assert!(display.contains("\"rate_decimals_above\": 2"), "{display}");
    assert!(display.contains("\"summary_rows\": false"), "{display}");
    for left_out in ["rate_decimals_below", "rate_global_decimals", "net_total"] {
        assert!(!display.contains(left_out), "{left_out} in {display}");
    }
    // An empty object reads as the defaults.
    let empty = text.replace(display, "\"display\": {");
    assert_eq!(
        fwb_format::from_str(&empty).unwrap().display,
        DisplayPrefs::default()
    );
}

#[test]
fn display_is_not_content() {
    let mut b = budget();
    b.display.net_total = false;
    let id = save_version(&mut b, "v1", "2026-10-09T10:00:00Z", None);
    // The version's copy is content only, so its budget has no display.
    let text = round_trip(&b);
    assert_eq!(text.matches("\"display\"").count(), 1, "{text}");
    // Restoring a version keeps the current display settings.
    b.display.summary_rows = false;
    restore_version(&mut b, id, "2026-10-09T11:00:00Z").unwrap();
    assert!(!b.display.summary_rows && !b.display.net_total);
    // A version's budget with display settings is refused.
    let bad = r#"{
  "format": "film-wizard-budget",
  "version": 1,
  "budget": {
    "title": "T",
    "base_currency": "USD",
    "versions": [
      {
        "id": 1,
        "name": "v",
        "created": "",
        "budget": { "title": "T", "base_currency": "USD", "display": { "net_total": false } }
      }
    ]
  }
}
"#;
    let e = fwb_format::from_str(bad).unwrap_err();
    assert!(
        e.to_string().contains("cannot have display settings"),
        "{e}"
    );
}

#[test]
fn global_info_is_content() {
    let mut b = budget();
    b.global_info.insert(
        "DAYS".into(),
        GlobalInfo {
            unit: "Day".into(),
            ..GlobalInfo::default()
        },
    );
    let id = save_version(&mut b, "v1", "2026-10-09T10:00:00Z", None);
    let text = round_trip(&b);
    assert_eq!(text.matches("\"global_info\"").count(), 2, "{text}");
    b.global_info.clear();
    restore_version(&mut b, id, "2026-10-09T11:00:00Z").unwrap();
    assert_eq!(b.global_info["DAYS"].unit, "Day");
}
