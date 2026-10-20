//! Report presets in the file: written only when there are some, every field
//! left out at its default, read back exactly, and read strictly.

use method_core::*;
use method_format::{from_str, report_preset_from_value, report_preset_to_value, to_string};
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
                ReportSection::Chart,
                ReportSection::Units,
                ReportSection::Sets,
                ReportSection::Locations,
                ReportSection::Currencies,
            ],
            excluded_categories: ["20".to_string()].into(),
            excluded_accounts: ["11-02".to_string(), "10-01".to_string()].into(),
            skip_empty_accounts: true,
            skip_zero_accounts: true,
            skip_suppressed_lines: true,
            skip_notes: true,
            skip_done_notes: true,
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
            vertical_rules: true,
            collapse_details: true,
            topsheet_fringes: true,
            topsheet_pages: true,
            header: Some(Band {
                left: Some(BandField::Info("Shoot days".into())),
                center: Some(BandField::Global("WEEKS".into())),
                right: Some(BandField::Text("Confidential".into())),
                first_page_only: true,
            }),
            footer: Some(Band {
                left: Some(BandField::File),
                center: Some(BandField::Printed),
                right: Some(BandField::Page),
                first_page_only: false,
            }),
            bid: None,
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
            "sections": ["info", "topsheet", "accounts", "detail", "fringes", "globals", "groups", "notes", "chart", "units", "sets", "locations", "currencies"],
            "excluded_categories": ["20"],
            "excluded_accounts": ["10-01", "11-02"],
            "skip_empty_accounts": true,
            "skip_zero_accounts": true,
            "skip_suppressed_lines": true,
            "skip_notes": true,
            "skip_done_notes": true,
            "only_groups": ["Alt"],
            "columns": ["rate", "amount", "fringe_pct", "original"],
            "landscape": true,
            "font_size": "small",
            "shading": true,
            "group_colors": true,
            "vertical_rules": true,
            "collapse_details": true,
            "topsheet_fringes": true,
            "topsheet_pages": true,
            "header": {
                "left": {"kind": "info", "label": "Shoot days"},
                "center": {"kind": "global", "name": "WEEKS"},
                "right": {"kind": "text", "text": "Confidential"},
                "first_page_only": true
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
        json!({"name": "x", "footer": {"left": {"kind": "global"}}}),
        json!({"name": "x", "footer": {"left": {"kind": "date", "name": "y"}}}),
        json!({"name": "x", "footer": {"first_page_only": "yes"}}),
        json!({"name": "x", "sections": ["chart_of_accounts"]}),
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

#[test]
fn date_and_time_slots_round_trip() {
    let v = json!({
        "name": "Clock",
        "header": {"left": {"kind": "date"}, "right": {"kind": "time"}}
    });
    let p = report_preset_from_value(v.clone()).unwrap();
    let h = p.spec.header.as_ref().unwrap();
    assert_eq!(h.left, Some(BandField::Date));
    assert_eq!(h.right, Some(BandField::Time));
    assert!(!h.first_page_only);
    assert_eq!(report_preset_to_value(&p), v);
}

// ---------------------------------------------------------------------------
// Bid layouts (roadmap 3.13; decisions.md, entry 92)

fn bid_preset() -> ReportPreset {
    let mut p = plain("Spot bid");
    p.spec.sections = vec![ReportSection::Bid];
    p.spec.bid = Some(BidLayout {
        fields: vec![
            BidField {
                label: "Client".into(),
                value: "Field Station Films".into(),
            },
            BidField {
                label: "Job number".into(),
                value: String::new(),
            },
        ],
        lines: vec![
            BidLine::Section {
                name: "Shoot".into(),
                picks: vec![
                    BidPick::Category("20".into()),
                    BidPick::Account("3100".into()),
                    BidPick::Range {
                        from: "4000".into(),
                        to: "4999".into(),
                    },
                ],
            },
            BidLine::Section {
                name: "Post".into(),
                picks: Vec::new(),
            },
            BidLine::Subtotal {
                name: "Subtotal".into(),
            },
            BidLine::Percent {
                name: "Markup".into(),
                percent: "12.5".parse().unwrap(),
                of: vec!["Shoot".into()],
            },
            BidLine::Percent {
                name: "On everything".into(),
                percent: "1".parse().unwrap(),
                of: Vec::new(),
            },
            BidLine::Flat {
                name: "Handling".into(),
                amount: 25_000,
            },
        ],
        show_accounts: true,
    });
    p
}

#[test]
fn bid_layout_shape() {
    let v = report_preset_to_value(&bid_preset());
    assert_eq!(
        v,
        json!({
            "name": "Spot bid",
            "sections": ["bid"],
            "bid": {
                "fields": [
                    {"label": "Client", "value": "Field Station Films"},
                    {"label": "Job number"}
                ],
                "lines": [
                    {"kind": "section", "name": "Shoot", "picks": [
                        {"kind": "category", "code": "20"},
                        {"kind": "account", "code": "3100"},
                        {"kind": "range", "from": "4000", "to": "4999"}
                    ]},
                    {"kind": "section", "name": "Post"},
                    {"kind": "subtotal", "name": "Subtotal"},
                    {"kind": "percent", "name": "Markup", "percent": "12.5", "of": ["Shoot"]},
                    {"kind": "percent", "name": "On everything", "percent": "1"},
                    {"kind": "flat", "name": "Handling", "amount": 25000}
                ],
                "show_accounts": true
            }
        })
    );
    assert_eq!(report_preset_from_value(v).unwrap(), bid_preset());
    // An empty layout is a real choice and is written as {}.
    let mut p = plain("Empty bid");
    p.spec.bid = Some(BidLayout::default());
    let v = report_preset_to_value(&p);
    assert_eq!(v["bid"], json!({}));
    assert_eq!(report_preset_from_value(v).unwrap(), p);
}

#[test]
fn bid_layouts_round_trip_and_leave_other_files_alone() {
    let mut b = Budget::new("Film", "USD");
    b.report_presets = vec![full_preset(), plain("Topsheet only")];
    // Without a bid layout the file has no trace of one.
    let before = to_string(&b);
    assert!(!before.contains("\"bid\""));
    let back = from_str(&before).unwrap();
    assert_eq!(to_string(&back), before, "byte-identical when unused");
    let mut example = plain("Example bid");
    example.spec.sections = vec![ReportSection::Topsheet, ReportSection::Bid];
    example.spec.bid = Some(BidLayout::example());
    b.report_presets.push(bid_preset());
    b.report_presets.push(example);
    let text = to_string(&b);
    let back = from_str(&text).unwrap();
    assert_eq!(back, b);
    assert_eq!(to_string(&back), text, "stable bytes");
}

#[test]
fn bid_layouts_are_read_strictly() {
    for bad in [
        json!({"name": "x", "bid": {"rows": []}}),
        json!({"name": "x", "bid": {"lines": [{"kind": "heading", "name": "A"}]}}),
        json!({"name": "x", "bid": {"lines": [{"kind": "section"}]}}),
        json!({"name": "x", "bid": {"lines": [{"kind": "section", "name": "A", "picks": [{"kind": "group", "code": "X"}]}]}}),
        json!({"name": "x", "bid": {"lines": [{"kind": "section", "name": "A", "picks": [{"kind": "range", "from": "1"}]}]}}),
        json!({"name": "x", "bid": {"lines": [{"kind": "percent", "name": "M", "percent": 10}]}}),
        json!({"name": "x", "bid": {"lines": [{"kind": "flat", "name": "F", "amount": "10.00"}]}}),
        json!({"name": "x", "bid": {"lines": [{"kind": "flat", "name": "F", "amount": 1, "of": []}]}}),
        json!({"name": "x", "bid": {"lines": [{"kind": "subtotal", "name": "S", "picks": []}]}}),
        json!({"name": "x", "bid": {"fields": [{"value": "v"}]}}),
        json!({"name": "x", "bid": {"show_accounts": "yes"}}),
    ] {
        assert!(report_preset_from_value(bad.clone()).is_err(), "{bad}");
    }
}
