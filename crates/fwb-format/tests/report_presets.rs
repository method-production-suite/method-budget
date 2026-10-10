//! Report presets in the file: written only when there are some, every field
//! left out at its default, read back exactly, and read strictly.

use fwb_core::*;
use fwb_format::{from_str, report_preset_from_value, report_preset_to_value, to_string};
use serde_json::json;

fn full_preset() -> ReportPreset {
    ReportPreset {
        name: "Studio pack".into(),
        default: true,
        spec: ReportSpec {
            sections: vec![
                ReportSection::Info,
                ReportSection::Topsheet,
                ReportSection::Accounts,
                ReportSection::Detail,
                ReportSection::Fringes,
                ReportSection::Globals,
                ReportSection::Groups,
                ReportSection::Notes,
            ],
            excluded_categories: ["20".to_string()].into(),
            excluded_accounts: ["11-02".to_string(), "10-01".to_string()].into(),
            skip_empty_accounts: true,
            skip_zero_accounts: true,
            skip_suppressed_lines: true,
            skip_notes: true,
            only_groups: ["Alt".to_string()].into(),
            columns: Some(vec![
                ReportColumn::Rate,
                ReportColumn::Amount,
                ReportColumn::FringePercent,
                ReportColumn::Original,
            ]),
            landscape: true,
            font_size: FontSize::Small,
            shading: true,
            group_colors: true,
            header: Some(Band {
                left: Some(BandField::Info("Shoot days".into())),
                center: None,
                right: Some(BandField::Text("Confidential".into())),
            }),
            footer: Some(Band {
                left: Some(BandField::File),
                center: Some(BandField::Printed),
                right: Some(BandField::Page),
            }),
        },
    }
}

fn plain(name: &str) -> ReportPreset {
    ReportPreset {
        name: name.into(),
        default: false,
        spec: ReportSpec::default(),
    }
}

#[test]
fn no_presets_writes_nothing() {
    let b = Budget::new("Film", "USD");
    assert!(!to_string(&b).contains("report_presets"));
}

#[test]
fn presets_round_trip() {
    let mut b = Budget::new("Film", "USD");
    b.report_presets = vec![full_preset(), plain("Topsheet only")];
    let mut empty_bands = plain("Bare");
    empty_bands.spec.header = Some(Band::default());
    empty_bands.spec.footer = Some(Band::default());
    empty_bands.spec.columns = Some(vec![]);
    empty_bands.spec.sections = vec![];
    b.report_presets.push(empty_bands);
    let text = to_string(&b);
    let back = from_str(&text).unwrap();
    assert_eq!(back, b);
    assert_eq!(to_string(&back), text, "stable bytes");
}

#[test]
fn defaults_are_left_out() {
    let v = report_preset_to_value(&plain("Plain"));
    assert_eq!(
        v,
        json!({"name": "Plain", "sections": ["topsheet", "detail"]})
    );
    // No columns or bands means the standard ones; an empty list or band
    // object is a real choice and is written.
    let mut p = plain("Bare");
    p.spec.columns = Some(vec![]);
    p.spec.header = Some(Band::default());
    let v = report_preset_to_value(&p);
    assert_eq!(v["columns"], json!([]));
    assert_eq!(v["header"], json!({}));
    assert!(v.get("footer").is_none());
}

#[test]
fn full_preset_shape() {
    let v = report_preset_to_value(&full_preset());
    assert_eq!(
        v,
        json!({
            "name": "Studio pack",
            "default": true,
            "sections": ["info", "topsheet", "accounts", "detail", "fringes", "globals", "groups", "notes"],
            "excluded_categories": ["20"],
            "excluded_accounts": ["10-01", "11-02"],
            "skip_empty_accounts": true,
            "skip_zero_accounts": true,
            "skip_suppressed_lines": true,
            "skip_notes": true,
            "only_groups": ["Alt"],
            "columns": ["rate", "amount", "fringe_pct", "original"],
            "landscape": true,
            "font_size": "small",
            "shading": true,
            "group_colors": true,
            "header": {
                "left": {"kind": "info", "label": "Shoot days"},
                "right": {"kind": "text", "text": "Confidential"}
            },
            "footer": {
                "left": {"kind": "file"},
                "center": {"kind": "printed"},
                "right": {"kind": "page"}
            }
        })
    );
    assert_eq!(report_preset_from_value(v).unwrap(), full_preset());
}

#[test]
fn presets_are_read_strictly() {
    for bad in [
        json!({"name": "x", "sections": ["topsheet", "budget"]}),
        json!({"name": "x", "columns": ["Rate"]}),
        json!({"name": "x", "font_size": "huge"}),
        json!({"name": "x", "colour": true}),
        json!({"name": "x", "header": {"middle": {"kind": "title"}}}),
        json!({"name": "x", "header": {"left": {"kind": "title", "text": "y"}}}),
        json!({"name": "x", "footer": {"left": {"kind": "clock"}}}),
        json!({"sections": ["topsheet"]}),
        json!({"name": "x", "landscape": "yes"}),
    ] {
        assert!(report_preset_from_value(bad.clone()).is_err(), "{bad}");
    }
}

#[test]
fn unknown_preset_field_in_a_file_names_its_path() {
    let mut b = Budget::new("Film", "USD");
    b.report_presets = vec![plain("A")];
    let text = to_string(&b).replace("\"name\": \"A\"", "\"name\": \"A\",\n        \"zoom\": 2");
    let err = from_str(&text).unwrap_err().to_string();
    assert!(err.contains("report_presets[0]"), "{err}");
    assert!(err.contains("zoom"), "{err}");
}

#[test]
fn a_version_cannot_carry_presets() {
    let mut b = Budget::new("Film", "USD");
    b.report_presets = vec![plain("Kept")];
    save_version(&mut b, "v1", "2026-10-09T10:00:00Z", None);
    let text = to_string(&b);
    // Versions never write presets of their own.
    assert_eq!(text.matches("report_presets").count(), 1);
    // A file that puts some in a version is rejected.
    let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
    v["budget"]["versions"][0]["budget"]["report_presets"] = json!([{"name": "Sneaky"}]);
    let err = from_str(&serde_json::to_string_pretty(&v).unwrap())
        .unwrap_err()
        .to_string();
    assert!(err.contains("report presets"), "{err}");
}

#[test]
fn restoring_a_version_keeps_the_presets() {
    let mut b = Budget::new("Film", "USD");
    let id = save_version(&mut b, "empty", "2026-10-09T10:00:00Z", None);
    b.report_presets = vec![plain("Made later")];
    restore_version(&mut b, id, "2026-10-09T11:00:00Z").unwrap();
    assert_eq!(b.report_presets, vec![plain("Made later")]);
}
