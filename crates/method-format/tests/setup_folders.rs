//! `setup_folders`: the personal library's folders per kind (roadmap 2.15).

use method_core::{folders, Budget, Fringe, FringeKind, SetDef};

fn library() -> Budget {
    let mut b = Budget::new("My Method Budget Library", "USD");
    b.units.clear();
    b.fringes.insert(
        "FICA".into(),
        Fringe {
            short_code: None,
            description: String::new(),
            rate: "7.65".into(),
            kind: FringeKind::Percent { cutoff: None },
        },
    );
    b.sets.insert("Stage".into(), SetDef::default());
    b
}

#[test]
fn folders_round_trip_and_are_left_out_when_there_are_none() {
    let mut b = library();
    let text = method_format::to_string(&b);
    assert!(!text.contains("setup_folders"), "{text}");
    assert!(!text.contains("topsheet"), "a library has no lines: {text}");

    folders::add_folder(&mut b, "fringe", "Payroll").unwrap();
    folders::add_folder(&mut b, "fringe", "Empty").unwrap();
    folders::move_items(&mut b, "fringe", &["FICA".into()], Some("Payroll")).unwrap();
    let text = method_format::to_string(&b);
    assert!(text.contains("\"setup_folders\""), "{text}");
    let back = method_format::from_str(&text).unwrap();
    assert_eq!(back, b);
    assert_eq!(folders::folder_of(&back, "fringe", "FICA"), Some("Payroll"));
    assert!(back.setup_folders.0["fringe"]["Empty"].is_empty());
    // Written again, byte for byte the same.
    assert_eq!(method_format::to_string(&back), text);
}

#[test]
fn names_that_are_gone_are_dropped_on_write() {
    let mut b = library();
    folders::add_folder(&mut b, "set", "Sets").unwrap();
    folders::move_items(&mut b, "set", &["Stage".into()], Some("Sets")).unwrap();
    b.sets.clear();
    let text = method_format::to_string(&b);
    let back = method_format::from_str(&text).unwrap();
    assert!(back.setup_folders.0["set"]["Sets"].is_empty());
}

#[test]
fn a_versions_budget_may_not_have_folders() {
    let doc = r#"{"format": "method-budget", "version": 1, "budget": {
      "title": "T", "base_currency": "USD",
      "versions": [{"id": 1, "name": "v", "created": "", "budget": {
        "title": "T", "base_currency": "USD", "setup_folders": {"set": {"A": []}}}}]
    }}"#;
    assert!(method_format::from_str(doc).is_err());
    let ok = r#"{"format": "method-budget", "version": 1, "budget": {
      "title": "T", "base_currency": "USD", "setup_folders": {"set": {"A": []}}}}"#;
    let b = method_format::from_str(ok).unwrap();
    assert!(b.setup_folders.0["set"]["A"].is_empty());
}

#[test]
fn saving_and_restoring_a_version_keeps_the_folders() {
    let mut b = library();
    folders::add_folder(&mut b, "set", "Sets").unwrap();
    let id = method_core::save_version(&mut b, "v", "2026-10-09T00:00:00Z", None);
    let text = method_format::to_string(&b);
    assert_eq!(text.matches("setup_folders").count(), 1, "{text}");
    method_core::restore_version(&mut b, id, "2026-10-09T00:00:01Z").unwrap();
    assert!(b.setup_folders.0["set"].contains_key("Sets"));
}
