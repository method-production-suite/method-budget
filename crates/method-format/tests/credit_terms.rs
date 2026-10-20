//! A credit's `minimum_spend`, `broker_fee` and `shared_cap`, and the
//! budget's `shared_caps` (decisions.md, entry 88): written only when set,
//! in a fixed place, read back exactly, and a budget without them reads and
//! writes as before, byte for byte.

mod common;

use std::path::PathBuf;

use common::*;
use method_core::*;
use method_format::{from_str, to_string};
use serde_json::Value;

fn program(name: &str) -> Credit {
    Credit {
        name: name.into(),
        account: None,
        applied: true,
        cap: Some(5_000_000),
        discount_percent: d("8"),
        excluded_fringes: set(&["P"]),
        locations: set(&["Ridge"]),
        minimum_spend: Some(100_000),
        broker_fee: 2_500,
        shared_cap: Some("State pool".into()),
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

fn with_terms() -> Budget {
    let mut b = rich();
    b.credits.push(program("Ridge county program"));
    b.shared_caps.insert("State pool".into(), 750_000);
    b
}

fn field_station() -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/field-station/field-station.mthd");
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

#[test]
fn terms_are_written_after_locations_and_shared_caps_after_credits() {
    let b = with_terms();
    let text = to_string(&b);
    let back = from_str(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));
    assert_eq!(back, b);
    assert_eq!(to_string(&back), text, "rewrite is byte-identical");

    let v: Value = serde_json::from_str(&text).unwrap();
    let credit = v["budget"]["credits"].as_array().unwrap().last().unwrap();
    assert_eq!(credit["minimum_spend"], serde_json::json!(100_000));
    assert_eq!(credit["broker_fee"], serde_json::json!(2_500));
    assert_eq!(credit["shared_cap"], serde_json::json!("State pool"));
    assert_eq!(
        v["budget"]["shared_caps"],
        serde_json::json!({"State pool": {"amount": 750_000}})
    );
    let at = |key: &str| {
        let start = text.find("\"Ridge county program\"").unwrap();
        start + text[start..].find(key).unwrap()
    };
    assert!(at("\"locations\"") < at("\"minimum_spend\""));
    assert!(at("\"minimum_spend\"") < at("\"broker_fee\""));
    assert!(at("\"broker_fee\"") < at("\"shared_cap\""));
    assert!(at("\"shared_cap\"") < at("\"rows\""));
    let top = |key: &str| text.find(key).unwrap();
    assert!(top("\"credits\"") < top("\"shared_caps\""));
}

#[test]
fn a_credit_without_terms_and_a_budget_without_shared_caps_leave_them_out() {
    let mut b = rich();
    let mut p = program("Plain");
    p.minimum_spend = None;
    p.broker_fee = 0;
    p.shared_cap = None;
    b.credits.push(p);
    let text = to_string(&b);
    let v: Value = serde_json::from_str(&text).unwrap();
    for c in v["budget"]["credits"].as_array().unwrap() {
        for key in ["minimum_spend", "broker_fee", "shared_cap"] {
            assert!(c.get(key).is_none(), "{key}: {c}");
        }
    }
    assert!(v["budget"].get("shared_caps").is_none());
    assert_eq!(from_str(&text).unwrap(), b);
}

#[test]
fn the_documented_example_reads() {
    // The example in docs/FORMAT.md, "Credit".
    let text = r#"{
  "format": "method-budget",
  "version": 1,
  "budget": {
    "title": "Example",
    "base_currency": "USD",
    "credits": [
      {
        "name": "Ridge county program",
        "applied": false,
        "locations": ["Ridge"],
        "minimum_spend": 25000000,
        "broker_fee": 150000,
        "shared_cap": "State pool",
        "rows": [
          {
            "description": "Qualified spend",
            "source": { "kind": "category", "category": "2100" },
            "percent": "25"
          }
        ]
      }
    ],
    "shared_caps": {
      "State pool": { "amount": 500000000 }
    }
  }
}"#;
    let b = from_str(text).unwrap_or_else(|e| panic!("{e}"));
    let c = &b.credits[0];
    assert_eq!(c.minimum_spend, Some(25_000_000));
    assert_eq!(c.broker_fee, 150_000);
    assert_eq!(c.shared_cap.as_deref(), Some("State pool"));
    assert_eq!(b.shared_caps.get("State pool"), Some(&500_000_000));
}

#[test]
fn misspellings_are_still_caught() {
    let b = with_terms();
    let v: Value = serde_json::from_str(&to_string(&b)).unwrap();
    for (path, from, to) in [
        ("credit", "broker_fee", "brokers_fee"),
        ("credit", "minimum_spend", "minimum"),
        ("cap", "amount", "cap"),
    ] {
        let mut v = v.clone();
        let obj = match path {
            "credit" => v["budget"]["credits"]
                .as_array_mut()
                .unwrap()
                .last_mut()
                .unwrap(),
            _ => &mut v["budget"]["shared_caps"]["State pool"],
        };
        let value = obj[from].take();
        obj.as_object_mut().unwrap().remove(from);
        obj[to] = value;
        assert!(from_str(&v.to_string()).is_err(), "{to} accepted");
    }
}

#[test]
fn versions_carry_shared_caps_full_and_compact() {
    let mut b = with_terms();
    versions::save_version(&mut b, "Draft", "2026-10-20T09:00:00Z", None);
    b.shared_caps.insert("State pool".into(), 900_000);
    b.credits.last_mut().unwrap().broker_fee = 0;
    for compact in [false, true] {
        b.compact_versions = compact;
        let text = to_string(&b);
        let back = from_str(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));
        assert_eq!(back, b, "compact {compact}");
        assert_eq!(
            back.versions[0].budget.shared_caps.get("State pool"),
            Some(&750_000)
        );
        assert_eq!(to_string(&back), text);
    }
}

#[test]
fn field_station_is_unchanged_byte_for_byte() {
    let text = field_station();
    let b = from_str(&text).unwrap();
    assert!(b.shared_caps.is_empty());
    assert!(b
        .credits
        .iter()
        .all(|c| c.minimum_spend.is_none() && c.broker_fee == 0 && c.shared_cap.is_none()));
    assert_eq!(to_string(&b), text);

    // Terms and a shared cap added and taken away again leave the same
    // bytes.
    let mut with = b.clone();
    with.credits[0].minimum_spend = Some(1);
    with.credits[0].broker_fee = 1;
    with.credits[0].shared_cap = Some("Pool".into());
    with.shared_caps.insert("Pool".into(), 1);
    let changed = to_string(&with);
    assert_ne!(changed, text);
    let mut back = from_str(&changed).unwrap();
    back.credits[0].minimum_spend = None;
    back.credits[0].broker_fee = 0;
    back.credits[0].shared_cap = None;
    back.shared_caps.clear();
    assert_eq!(to_string(&back), text);
}

/// The change log on disk writes the terms and the shared caps only when a
/// budget uses them, so logs that don't stay readable by older versions.
#[test]
fn the_change_log_writes_terms_only_when_set() {
    let adopt = |b: &Budget| {
        let log = method_core::log::Log::adopt(
            method_core::log::ActorId([1; 32]),
            method_core::log::Ids::seeded(1),
            b,
            "Adopted",
            "2026-10-20T09:00:00Z",
        );
        log.changes()
            .map(method_format::log::change_to_json)
            .collect::<String>()
    };
    let text = adopt(&rich());
    assert!(text.contains("excluded_fringes"), "credits are in the log");
    for key in ["minimum_spend", "broker_fee", "shared_cap"] {
        assert!(!text.contains(key), "{key}: {text}");
    }
    let text = adopt(&with_terms());
    for key in [
        "\"minimum_spend\"",
        "\"broker_fee\"",
        "\"shared_cap\"",
        "\"shared_caps\"",
    ] {
        assert!(text.contains(key), "{key}");
    }
}
