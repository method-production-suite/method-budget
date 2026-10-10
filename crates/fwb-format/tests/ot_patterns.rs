//! Overtime patterns in the file: written only when there are some, read
//! back exactly, checked when read, and never inside a version.

mod common;

use common::*;
use fwb_core::overtime::{OtBand, OtPattern};
use fwb_core::*;
use fwb_format::{from_str, to_string};
use serde_json::json;

fn crew() -> OtPattern {
    OtPattern {
        name: "Crew 12s".into(),
        straight_hours: d("8"),
        bands: vec![
            OtBand {
                multiplier: d("1.5"),
                hours: Some(d("4")),
            },
            OtBand {
                multiplier: d("2"),
                hours: None,
            },
        ],
        sixth_day: Some(d("1.5")),
        seventh_day: None,
    }
}

#[test]
fn none_writes_nothing() {
    assert!(!to_string(&Budget::new("Film", "USD")).contains("ot_patterns"));
    assert!(!to_string(&sample()).contains("ot_patterns"));
}

#[test]
fn patterns_round_trip_exactly() {
    let mut b = sample();
    let plain = OtPattern {
        name: "Straight".into(),
        straight_hours: d("10"),
        bands: Vec::new(),
        sixth_day: None,
        seventh_day: Some(d("2")),
    };
    b.ot_patterns = vec![crew(), plain];
    let text = to_string(&b);
    let back = from_str(&text).unwrap();
    assert_eq!(back, b);
    assert_eq!(to_string(&back), text, "stable");
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        v["budget"]["ot_patterns"][0],
        json!({
            "name": "Crew 12s",
            "straight_hours": "8",
            "bands": [{"multiplier": "1.5", "hours": "4"}, {"multiplier": "2"}],
            "sixth_day": "1.5"
        })
    );
    assert_eq!(
        v["budget"]["ot_patterns"][1],
        json!({"name": "Straight", "straight_hours": "10", "seventh_day": "2"})
    );
}

#[test]
fn patterns_are_read_strictly() {
    let mut b = Budget::new("Film", "USD");
    b.ot_patterns = vec![crew()];
    let v: serde_json::Value = serde_json::from_str(&to_string(&b)).unwrap();
    for (path, value) in [
        // An unknown field, a float, a middle band without hours, a zero
        // multiplier, a blank name.
        ("speed", json!(2)),
        ("straight_hours", json!(8)),
        ("bands", json!([{"multiplier": "1.5"}, {"multiplier": "2"}])),
        ("bands", json!([{"multiplier": "0"}])),
        ("name", json!(" ")),
        ("straight_hours", json!("25")),
    ] {
        let mut w = v.clone();
        w["budget"]["ot_patterns"][0][path] = value.clone();
        let err = from_str(&serde_json::to_string_pretty(&w).unwrap());
        assert!(err.is_err(), "{path} = {value}");
        assert!(
            err.unwrap_err().to_string().contains("ot_patterns[0]"),
            "{path} = {value}"
        );
    }
}

#[test]
fn a_version_cannot_carry_patterns_and_restoring_keeps_them() {
    let mut b = Budget::new("Film", "USD");
    let id = save_version(&mut b, "empty", "2026-10-09T10:00:00Z", None);
    b.ot_patterns = vec![crew()];
    save_version(&mut b, "v2", "2026-10-09T10:30:00Z", None);
    let text = to_string(&b);
    assert_eq!(text.matches("ot_patterns").count(), 1);
    let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
    v["budget"]["versions"][0]["budget"]["ot_patterns"] =
        json!([{"name": "Sneaky", "straight_hours": "8"}]);
    let err = from_str(&serde_json::to_string_pretty(&v).unwrap())
        .unwrap_err()
        .to_string();
    assert!(err.contains("overtime patterns"), "{err}");
    restore_version(&mut b, id, "2026-10-09T11:00:00Z").unwrap();
    assert_eq!(b.ot_patterns, vec![crew()]);
}
