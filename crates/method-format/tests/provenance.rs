//! `provenance`: where a shared copy or a sub-budget came from (roadmap 2.21
//! and 2.22).

use method_core::{starter, Budget, Provenance, ProvenanceKind};

fn stamped(kind: ProvenanceKind, version: Option<&str>, note: Option<&str>) -> Budget {
    let mut b = starter::budget();
    b.provenance = Some(Provenance {
        kind,
        source_title: "Big Show".into(),
        source_version: version.map(str::to_string),
        saved: "2026-10-09T14:30:00-07:00".into(),
        app_version: "0.0.1".into(),
        note: note.map(str::to_string),
        pack: None,
    });
    b
}

#[test]
fn provenance_round_trips_and_is_left_out_without_one() {
    let plain = starter::budget();
    assert!(!method_format::to_string(&plain).contains("provenance"));

    for (kind, word) in [
        (ProvenanceKind::SharedCopy, "shared_copy"),
        (ProvenanceKind::EditableCopy, "editable_copy"),
        (ProvenanceKind::SubBudget, "sub_budget"),
    ] {
        for (version, note) in [(None, None), (Some("Studio draft"), Some("For review"))] {
            let b = stamped(kind, version, note);
            let text = method_format::to_string(&b);
            assert!(text.contains(&format!("\"kind\": \"{word}\"")), "{text}");
            assert_eq!(text.contains("source_version"), version.is_some());
            assert_eq!(text.contains("\"note\": \"For review\""), note.is_some());
            let back = method_format::from_str(&text).unwrap();
            assert_eq!(back, b);
            assert_eq!(method_format::to_string(&back), text, "byte for byte");
        }
    }
}

#[test]
fn provenance_is_written_after_the_base_currency() {
    let text = method_format::to_string(&stamped(ProvenanceKind::SharedCopy, None, None));
    let at = |k: &str| text.find(&format!("\"{k}\"")).unwrap();
    assert!(at("base_currency") < at("provenance"));
    assert!(at("provenance") < at("units"));
}

#[test]
fn bad_provenance_is_refused() {
    let doc = |p: &str| {
        format!(
            r#"{{"format": "method-budget", "version": 1, "budget": {{
              "title": "T", "base_currency": "USD", "provenance": {p}}}}}"#
        )
    };
    let ok = r#"{"kind": "shared_copy", "source_title": "S", "saved": "2026-10-09T10:00Z", "app_version": "0.0.1"}"#;
    let b = method_format::from_str(&doc(ok)).unwrap();
    assert!(b.provenance.unwrap().is_shared_copy());
    // An unknown kind, an unknown field, a missing required field.
    for bad in [
        r#"{"kind": "stolen", "source_title": "S", "saved": "x", "app_version": "1"}"#,
        r#"{"kind": "sub_budget", "source_title": "S", "saved": "x", "app_version": "1", "path": "/Users/me"}"#,
        r#"{"kind": "sub_budget", "saved": "x", "app_version": "1"}"#,
    ] {
        assert!(method_format::from_str(&doc(bad)).is_err(), "{bad}");
    }
}

#[test]
fn a_versions_budget_may_not_have_a_provenance() {
    let doc = r#"{"format": "method-budget", "version": 1, "budget": {
      "title": "T", "base_currency": "USD",
      "versions": [{"id": 1, "name": "v", "created": "", "budget": {
        "title": "T", "base_currency": "USD",
        "provenance": {"kind": "sub_budget", "source_title": "S", "saved": "x", "app_version": "1"}}}]
    }}"#;
    assert!(method_format::from_str(doc).is_err());
}

#[test]
fn versions_dont_carry_it_and_restoring_keeps_it() {
    let mut b = stamped(ProvenanceKind::SubBudget, None, Some("Categories 10"));
    let id = method_core::save_version(&mut b, "v", "2026-10-09T10:00:00Z", None);
    let text = method_format::to_string(&b);
    assert_eq!(
        text.matches("\"provenance\"").count(),
        1,
        "only the budget's own"
    );
    method_core::restore_version(&mut b, id, "2026-10-09T11:00:00Z").unwrap();
    assert_eq!(
        b.provenance.as_ref().unwrap().note.as_deref(),
        Some("Categories 10")
    );
}
