//! Tag presets and line blocks in the file: written only when there are
//! some, every part left out when empty, read back exactly, read strictly,
//! and never inside a version.

mod common;

use common::*;
use method_core::*;
use method_format::{from_str, tag_preset_from_value, tag_preset_to_value, to_string};
use serde_json::json;

fn crew() -> TagPreset {
    TagPreset {
        name: "Union crew".into(),
        fringes: set(&["PAYROLL", "PENSION"]),
        groups: set(&["Crew"]),
        set: Some("Stage A".into()),
        location: Some("Toronto".into()),
        currency: Some("CAD".into()),
    }
}

fn block() -> LinePreset {
    let mut first = Detail::line(0, "Prep", "5", "Day", "RATE");
    first.fringes = set(&["PAYROLL"]);
    first.fringe_range = Some(7);
    first.bold = true;
    first.notes = vec![Note::plain(1, "Five prep days")];
    let mut setup = SetupItems::default();
    setup.fringes.insert(
        "PAYROLL".into(),
        Fringe {
            short_code: None,
            description: String::new(),
            rate: "RATE_PCT".into(),
            kind: FringeKind::Percent { cutoff: None },
        },
    );
    setup.globals.insert("RATE".into(), "500".into());
    setup.globals.insert("RATE_PCT".into(), "18".into());
    setup.units.insert(
        "Day".into(),
        UnitDef {
            plural: "Days".into(),
            description: String::new(),
            hours: Some(d("10")),
        },
    );
    LinePreset {
        name: "Prep week".into(),
        rows: vec![first, Detail::subtotal(0, "Prep total")],
        setup,
    }
}

#[test]
fn no_presets_writes_nothing() {
    let text = to_string(&Budget::new("Film", "USD"));
    assert!(!text.contains("tag_presets"));
    assert!(!text.contains("line_presets"));
    // The full sample budget, with none, is unchanged too.
    let text = to_string(&sample());
    assert!(!text.contains("tag_presets") && !text.contains("line_presets"));
}

#[test]
fn presets_round_trip() {
    let mut b = sample();
    b.tag_presets = vec![
        crew(),
        TagPreset {
            name: "Nothing".into(),
            ..TagPreset::default()
        },
    ];
    b.line_presets = vec![
        block(),
        LinePreset {
            name: "Empty".into(),
            rows: vec![],
            setup: SetupItems::default(),
        },
    ];
    let text = to_string(&b);
    let back = from_str(&text).unwrap();
    assert_eq!(back, b);
    assert_eq!(to_string(&back), text, "stable bytes");
}

#[test]
fn tag_preset_shape() {
    let v = tag_preset_to_value(&crew());
    assert_eq!(
        v,
        json!({
            "name": "Union crew",
            "fringes": ["PAYROLL", "PENSION"],
            "groups": ["Crew"],
            "set": "Stage A",
            "location": "Toronto",
            "currency": "CAD"
        })
    );
    assert_eq!(tag_preset_from_value(v).unwrap(), crew());
    let bare = TagPreset {
        name: "Bare".into(),
        ..TagPreset::default()
    };
    assert_eq!(tag_preset_to_value(&bare), json!({"name": "Bare"}));
}

#[test]
fn block_shape_has_no_row_ids() {
    let mut b = Budget::new("Film", "USD");
    b.line_presets = vec![block()];
    let v: serde_json::Value = serde_json::from_str(&to_string(&b)).unwrap();
    let p = &v["budget"]["line_presets"][0];
    assert_eq!(p["name"], "Prep week");
    assert_eq!(
        p["rows"],
        json!([
            {
                "description": "Prep",
                "amount": "5",
                "unit": "Day",
                "rate": "RATE",
                "fringe_range": 7,
                "fringes": ["PAYROLL"],
                "bold": true,
                "notes": [{"id": 1, "text": "Five prep days"}]
            },
            {"kind": "subtotal", "description": "Prep total"}
        ])
    );
    assert_eq!(
        p["setup"],
        json!({
            "units": {"Day": {"plural": "Days", "hours": "10"}},
            "globals": {"RATE": "500", "RATE_PCT": "18"},
            "fringes": {"PAYROLL": {"kind": "percent", "rate": "RATE_PCT"}}
        })
    );
}

#[test]
fn presets_are_read_strictly() {
    for bad in [
        json!({"name": "x", "fringe": ["A"]}),
        json!({"name": "x", "fringes": "A"}),
        json!({"name": "x", "set": 3}),
        json!({"fringes": ["A"]}),
    ] {
        assert!(tag_preset_from_value(bad.clone()).is_err(), "{bad}");
    }
    let mut b = Budget::new("Film", "USD");
    b.line_presets = vec![block()];
    let text = to_string(&b);
    let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
    // A block row with an id, an unknown setup table, an unknown field.
    for (path, value) in [
        ("rows", json!([{"id": 4, "description": "x"}])),
        ("setup", json!({"charges": {}})),
        ("colour", json!("red")),
    ] {
        let mut w = v.clone();
        w["budget"]["line_presets"][0][path] = value;
        let err = from_str(&serde_json::to_string_pretty(&w).unwrap()).unwrap_err();
        assert!(err.to_string().contains("line_presets[0]"), "{path}: {err}");
    }
    v["budget"]["tag_presets"] = json!([{"name": "x", "zoom": 2}]);
    let err = from_str(&serde_json::to_string_pretty(&v).unwrap()).unwrap_err();
    assert!(err.to_string().contains("tag_presets[0]"), "{err}");
}

#[test]
fn a_version_cannot_carry_presets_and_restoring_keeps_them() {
    let mut b = Budget::new("Film", "USD");
    let id = save_version(&mut b, "empty", "2026-10-09T10:00:00Z", None);
    b.tag_presets = vec![crew()];
    b.line_presets = vec![block()];
    save_version(&mut b, "v2", "2026-10-09T10:30:00Z", None);
    let text = to_string(&b);
    assert_eq!(text.matches("tag_presets").count(), 1);
    assert_eq!(text.matches("line_presets").count(), 1);
    for key in ["tag_presets", "line_presets"] {
        let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
        v["budget"]["versions"][0]["budget"][key] = json!([{"name": "Sneaky"}]);
        let err = from_str(&serde_json::to_string_pretty(&v).unwrap())
            .unwrap_err()
            .to_string();
        assert!(err.contains("tag presets or line blocks"), "{err}");
    }
    restore_version(&mut b, id, "2026-10-09T11:00:00Z").unwrap();
    assert_eq!(b.tag_presets, vec![crew()]);
    assert_eq!(b.line_presets, vec![block()]);
}
