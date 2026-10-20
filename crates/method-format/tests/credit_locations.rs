//! A credit's `locations` (decisions.md, entry 80): written only when the
//! credit has some, between `excluded_fringes` and `rows`, read back
//! exactly, and a budget without them reads and writes as before.

mod common;

use std::path::PathBuf;

use common::*;
use method_core::*;
use method_format::{from_str, to_string};
use serde_json::Value;

fn program(locations: &[&str]) -> Credit {
    Credit {
        name: "Ridge county program".into(),
        account: None,
        applied: false,
        cap: Some(5_000_000),
        discount_percent: d("8"),
        excluded_fringes: set(&["P"]),
        locations: set(locations),
        minimum_spend: None,
        broker_fee: 0,
        shared_cap: None,
        rows: vec![CreditRow {
            description: "Qualified spend".into(),
            source: CreditSource::Category {
                category: "2100".into(),
            },
            percent: d("25"),
            cap: CreditCap::None,
        }],
    }
}

fn field_station() -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/field-station/field-station.mthd");
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

#[test]
fn locations_are_written_between_excluded_fringes_and_rows() {
    let mut b = rich();
    b.credits.push(program(&["Ridge", "Coast"]));
    let text = to_string(&b);
    let back = from_str(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));
    assert_eq!(back, b);
    assert_eq!(to_string(&back), text, "rewrite is byte-identical");

    let v: Value = serde_json::from_str(&text).unwrap();
    let credit = v["budget"]["credits"].as_array().unwrap().last().unwrap();
    assert_eq!(credit["locations"], serde_json::json!(["Coast", "Ridge"]));
    let at = |key: &str| {
        let start = text.find("\"Ridge county program\"").unwrap();
        start + text[start..].find(key).unwrap()
    };
    assert!(at("\"excluded_fringes\"") < at("\"locations\""));
    assert!(at("\"locations\"") < at("\"rows\""));
}

#[test]
fn a_credit_without_locations_leaves_the_field_out() {
    let mut b = rich();
    b.credits.push(program(&[]));
    let text = to_string(&b);
    let v: Value = serde_json::from_str(&text).unwrap();
    for c in v["budget"]["credits"].as_array().unwrap() {
        assert!(c.get("locations").is_none(), "{c}");
    }
    assert_eq!(from_str(&text).unwrap(), b);
}

#[test]
fn locations_read_as_a_name_set() {
    let mut b = rich();
    b.credits.push(program(&["Ridge"]));
    let mut v: Value = serde_json::from_str(&to_string(&b)).unwrap();
    let credit = v["budget"]["credits"]
        .as_array_mut()
        .unwrap()
        .last_mut()
        .unwrap();
    credit["locations"] = serde_json::json!(["Ridge", "Coast", "Ridge"]);
    let back = from_str(&v.to_string()).unwrap();
    assert_eq!(
        back.credits.last().unwrap().locations,
        set(&["Coast", "Ridge"])
    );

    // A misspelling is still caught.
    let credit = v["budget"]["credits"]
        .as_array_mut()
        .unwrap()
        .last_mut()
        .unwrap();
    let list = credit["locations"].take();
    credit.as_object_mut().unwrap().remove("locations");
    credit["location"] = list;
    assert!(from_str(&v.to_string()).is_err());
}

#[test]
fn field_station_reads_and_writes_byte_identical() {
    let text = field_station();
    let b = from_str(&text).unwrap();
    // The sample's county program counts spend at the field station only.
    assert_eq!(b.credits.len(), 1);
    assert_eq!(b.credits[0].locations, set(&["Cold Spring field station"]));
    assert_eq!(to_string(&b), text);

    // Without its locations the program writes no `locations` key, and
    // putting them back gives the same bytes.
    let mut anywhere = b.clone();
    anywhere.credits[0].locations = set(&[]);
    let plain = to_string(&anywhere);
    let v: Value = serde_json::from_str(&plain).unwrap();
    assert!(v["budget"]["credits"][0].get("locations").is_none());
    let mut back = from_str(&plain).unwrap();
    back.credits[0].locations = set(&["Cold Spring field station"]);
    assert_eq!(to_string(&back), text);

    // A program added and taken away again leaves the same bytes.
    let mut with = b.clone();
    with.credits.push(program(&["Cold Spring field station"]));
    let changed = to_string(&with);
    assert_ne!(changed, text);
    let mut back = from_str(&changed).unwrap();
    back.credits.pop();
    assert_eq!(to_string(&back), text);
}

/// The change log on disk writes no `locations` for a credit without them,
/// so shared logs that don't use them stay readable by older versions.
#[test]
fn the_change_log_writes_no_locations_for_a_credit_without_them() {
    let b = rich();
    assert!(!b.credits.is_empty() && b.credits.iter().all(|c| c.locations.is_empty()));
    let log = method_core::log::Log::adopt(
        method_core::log::ActorId([1; 32]),
        method_core::log::Ids::seeded(1),
        &b,
        "Adopted",
        "2026-10-18T09:00:00Z",
    );
    let text: String = log
        .changes()
        .map(method_format::log::change_to_json)
        .collect();
    assert!(text.contains("excluded_fringes"), "credits are in the log");
    assert!(!text.contains("\"locations\""), "{text}");

    let mut with = rich();
    with.credits.push(program(&["Ridge"]));
    let log = method_core::log::Log::adopt(
        method_core::log::ActorId([1; 32]),
        method_core::log::Ids::seeded(1),
        &with,
        "Adopted",
        "2026-10-18T09:00:00Z",
    );
    let text: String = log
        .changes()
        .map(method_format::log::change_to_json)
        .collect();
    assert!(text.contains("\"locations\""));
}
