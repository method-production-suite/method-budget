//! Reading Method Scheduling's budget exchange file, version 0 (roadmap 3.8;
//! the spec is that project's docs/EXCHANGE.md). The fixtures in
//! tests/fixtures/schedule are invented schedules: `lantern-days.json` is
//! the file the scheduling side sent as its sample, `long-haul-days.json`
//! is its writer's output for its built-in generated sample (board IDs,
//! holds, holidays, company moves), and `every-field-days.json` is written
//! by hand from the spec's example with every optional field.

use std::path::PathBuf;

use method_format::import::exchange;
use method_format::import::schedule::{self, BudgetField, ElementKey, Found, ScheduleDays};

fn fixture(name: &str) -> Vec<u8> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/schedule")
        .join(name);
    std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn read(name: &str) -> ScheduleDays {
    schedule::read(&fixture(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn text(name: &str) -> String {
    String::from_utf8(fixture(name)).unwrap()
}

/// `name`'s text with `from` replaced by `to` (once, and it must be there).
fn edited(name: &str, from: &str, to: &str) -> Vec<u8> {
    let t = text(name);
    assert!(t.contains(from), "{from:?} isn't in {name}");
    t.replacen(from, to, 1).into_bytes()
}

fn refused(bytes: &[u8]) -> String {
    match schedule::read(bytes) {
        Ok(d) => panic!("read when it should not have been: {d:?}"),
        Err(e) => e.0,
    }
}

#[test]
fn the_sample_reads() {
    let d = read("lantern-days.json");
    assert_eq!(d.version, 0);
    assert_eq!(
        (d.schedule.as_str(), d.board.as_str(), d.sub_board.as_str()),
        ("Untitled", "Board 1", "Main")
    );
    assert_eq!(d.generated.as_deref(), Some("2026-10-12"));
    assert_eq!(d.source_hash, "302513897fb5f2a5");
    assert!(d.notes.is_empty(), "{:?}", d.notes);

    let s = &d.shoot_days;
    assert_eq!(
        (s.start.as_str(), s.end.as_deref()),
        ("2027-03-01", Some("2027-04-02"))
    );
    assert_eq!(s.total, 25);
    assert!(s.by_unit.is_empty() && s.by_location.is_empty());
    assert_eq!(s.by_set.len(), 25);
    assert_eq!(s.by_set[0], ("ROOM 1".to_string(), 1));
    assert_eq!(s.by_week.len(), 5);
    assert_eq!(s.by_week[4], ("2027-03-29".to_string(), 5));
    assert_eq!(
        (s.prep_start.as_deref(), s.wrap_end.as_deref()),
        (None, None)
    );

    let names: Vec<&str> = d.elements.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["ADA", "BRUNO", "CLEO", "DARIO", "EVE", "FINN"]);
    let ada = &d.elements[0];
    assert_eq!(
        (ada.id, ada.category.as_str(), ada.board_id.as_deref()),
        (6, "Cast", None)
    );
    assert_eq!(
        (ada.first.as_deref(), ada.last.as_deref()),
        (Some("2027-03-01"), Some("2027-03-29"))
    );
    assert_eq!((ada.work, ada.hold, ada.total), (3, 0, 3));
    assert!(!ada.holds && !ada.drops);
    assert_eq!(ada.min_drop_gap, 1);

    // Shoot days into SHOOT_DAYS, the shoot dates into budget info.
    assert_eq!(
        d.budget_fields(),
        [
            BudgetField::Global {
                name: "SHOOT_DAYS",
                days: 25
            },
            BudgetField::Date {
                label: "Shoot start",
                date: "2027-03-01".into()
            },
            BudgetField::Date {
                label: "Shoot end",
                date: "2027-04-02".into()
            },
        ]
    );
}

#[test]
fn every_optional_field_reads() {
    let d = read("every-field-days.json");
    assert!(d.notes.is_empty(), "{:?}", d.notes);
    let s = &d.shoot_days;
    assert_eq!(s.by_unit, [("SECOND".to_string(), 6)]);
    assert_eq!(s.by_location.len(), 2);
    assert_eq!(s.prep_start.as_deref(), Some("2027-01-18"));
    assert_eq!(s.wrap_end.as_deref(), Some("2027-03-26"));
    let kit = &d.elements[0];
    assert_eq!(kit.board_id.as_deref(), Some("1"));
    assert_eq!(
        [
            kit.work,
            kit.hold,
            kit.travel,
            kit.fitting,
            kit.rehearsal,
            kit.holiday,
            kit.company_move,
            kit.total
        ],
        [5, 1, 1, 1, 1, 1, 1, 11]
    );
    assert!(kit.holds && !kit.drops);
    assert_eq!(d.elements[1].min_drop_gap, 10);
    assert_eq!(d.elements[2].board_id, None);
    let labels: Vec<String> = d
        .budget_fields()
        .iter()
        .map(|f| match f {
            BudgetField::Global { name, days } => format!("{name} = {days}"),
            BudgetField::Date { label, date } => format!("{label}: {date}"),
        })
        .collect();
    assert_eq!(
        labels,
        [
            "SHOOT_DAYS = 6",
            "Prep start: 2027-01-18",
            "Shoot start: 2027-03-01",
            "Shoot end: 2027-03-09",
            "Wrap end: 2027-03-26",
        ]
    );
}

#[test]
fn a_long_generated_schedule_reads() {
    let d = read("long-haul-days.json");
    assert!(d.notes.is_empty(), "{:?}", d.notes);
    assert_eq!(d.shoot_days.total, 292);
    assert_eq!(d.elements.len(), 200);
    let weeks: u32 = d.shoot_days.by_week.iter().map(|(_, n)| n).sum();
    assert_eq!(weeks, 292);
    // A held performer's total runs past the shoot days: holidays and
    // company moves count too.
    let lead = &d.elements[0];
    assert_eq!(
        (lead.board_id.as_deref(), lead.name.as_str()),
        (Some("1"), "PLAYER 00")
    );
    assert_eq!(
        (lead.work, lead.hold, lead.holiday, lead.company_move),
        (249, 43, 3, 1)
    );
    assert_eq!(lead.total, 296);
    assert!(lead.total > d.shoot_days.total);
}

#[test]
fn linked_elements_are_found_again() {
    let d = read("every-field-days.json");
    let kit = d.elements[0].key();
    assert!(matches!(d.find(&kit), Found::Same(e) if e.id == 7));
    // The next export renames KIT: the id carries the link.
    let renamed = schedule::read(&edited("every-field-days.json", "\"KIT\"", "\"KITTY\"")).unwrap();
    assert!(matches!(renamed.find(&kit), Found::Same(e) if e.name == "KITTY"));
    // Renamed and renumbered: ask first.
    let both = edited("every-field-days.json", "\"KIT\"", "\"KITTY\"");
    let both =
        String::from_utf8(both)
            .unwrap()
            .replacen("\"board_id\": \"1\"", "\"board_id\": \"9\"", 1);
    let both = schedule::read(both.as_bytes()).unwrap();
    assert!(matches!(both.find(&kit), Found::IdOnly(e) if e.id == 7));
    // Gone from the sub-board: 0 days.
    let gone = ElementKey {
        category: "Cast".into(),
        board_id: Some("3".into()),
        name: "WREN".into(),
        id: Some(99),
    };
    assert_eq!(d.find(&gone), Found::Missing);
}

#[test]
fn later_versions_read_with_a_note_and_unknown_fields_are_ignored() {
    let v1 = schedule::read(&edited(
        "lantern-days.json",
        "\"version\": 0",
        "\"version\": 1",
    ))
    .unwrap();
    assert_eq!(v1.version, 1);
    assert!(v1.notes.is_empty());
    let v2 = edited("lantern-days.json", "\"version\": 0", "\"version\": 2");
    let v2 = String::from_utf8(v2).unwrap().replacen(
        "\"source_hash\"",
        "\"added_later\": {\"x\": [1, 2]},\n  \"source_hash\"",
        1,
    );
    let v2 = schedule::read(v2.as_bytes()).unwrap();
    assert_eq!(v2.version, 2);
    assert_eq!(v2.notes.len(), 1, "{:?}", v2.notes);
    assert!(v2.notes[0].contains("version 2"));
    assert_eq!(v2.elements.len(), 6);
}

#[test]
fn odd_but_readable_files_say_why() {
    let weeks = schedule::read(&edited(
        "lantern-days.json",
        "\"2027-03-29\": 5",
        "\"2027-03-29\": 4",
    ))
    .unwrap();
    assert!(
        weeks
            .notes
            .iter()
            .any(|n| n.contains("by week add up to 24")),
        "{:?}",
        weeks.notes
    );
    let no_end =
        schedule::read(&edited("lantern-days.json", "\"end\": \"2027-04-02\",", "")).unwrap();
    assert!(no_end.notes.iter().any(|n| n.contains("no last shoot day")));
    assert!(!no_end.budget_fields().iter().any(|f| matches!(
        f,
        BudgetField::Date {
            label: "Shoot end",
            ..
        }
    )));
    let prep = schedule::read(&edited(
        "every-field-days.json",
        "\"prep_start\": \"2027-01-18\"",
        "\"prep_start\": \"2027-04-18\"",
    ))
    .unwrap();
    assert!(prep.notes.iter().any(|n| n.contains("Prep starts")));
    // null is a left-out optional field.
    let null = schedule::read(&edited(
        "every-field-days.json",
        "\"board_id\": \"1\"",
        "\"board_id\": null",
    ))
    .unwrap();
    assert_eq!(null.elements[0].board_id, None);
}

#[test]
fn bad_files_are_refused_in_words() {
    let lantern = "lantern-days.json";
    for (from, to, why) in [
        (
            "\"method-schedule-days\"",
            "\"method-budget\"",
            "isn't a schedule days file",
        ),
        ("\"version\": 0", "\"version\": -1", "version"),
        ("\"version\": 0", "\"version\": 0.5", "version"),
        ("\"version\": 0,", "", "no version"),
        ("\"total\": 25", "\"total\": \"25\"", "total"),
        (
            "\"total\": 25",
            "\"total\": 4294967296",
            "smaller whole number",
        ),
        (
            "\"start\": \"2027-03-01\"",
            "\"start\": \"2027-02-30\"",
            "YYYY-MM-DD",
        ),
        (
            "\"generated\": \"2026-10-12\"",
            "\"generated\": \"12/10/2026\"",
            "generated",
        ),
        ("\"2027-03-01\": 5", "\"week 1\": 5", "isn't a date"),
        ("\"ROOM 1\": 1", "\"ROOM 1\": true", "ROOM 1"),
        (
            "\"source_hash\": \"302513897fb5f2a5\"",
            "\"source_hash\": \"\"",
            "source_hash",
        ),
        ("\"total\": 3,", "\"total\": 4,", "add up to 3"),
        ("\"holds\": false", "\"holds\": 0", "true or false"),
        ("\"id\": 16", "\"id\": 6", "same id"),
        ("\"name\": \"ADA\",", "", "no name"),
        (
            "\"first\": \"2027-03-01\"",
            "\"first\": \"2027-04-01\"",
            "after its last",
        ),
        (
            "\"schedule\": \"Untitled\"",
            "\"schedule\": \"Untitled\", \"schedule\": \"Other\"",
            "twice",
        ),
        (
            "\"elements\": [",
            "\"elements\": {\"a\": 1}, \"x\": [",
            "elements isn't a list",
        ),
    ] {
        let e = refused(&edited(lantern, from, to));
        assert!(e.contains(why), "{from} -> {to}: {e}");
    }
    for junk in [
        &b""[..],
        b"[]",
        b"{",
        b"\xff\xfe{}",
        b"{\"format\": \"method-schedule-days\"}",
    ] {
        assert!(schedule::read(junk).is_err());
    }
    let deep = format!("{}{}", "[".repeat(100_000), "]".repeat(100_000));
    assert!(schedule::read(deep.as_bytes()).is_err());
    let huge = vec![b' '; method_format::import::MAX_FILE_BYTES + 1];
    assert!(refused(&huge).contains("too large"));
}

#[test]
fn every_cut_of_the_sample_is_refused_without_panicking() {
    let bytes = fixture("every-field-days.json");
    let end = bytes.trim_ascii_end().len();
    for n in 0..end {
        assert!(schedule::read(&bytes[..n]).is_err(), "cut at {n}");
    }
}

#[test]
fn the_budget_export_import_points_schedule_files_elsewhere() {
    let bytes = fixture("lantern-days.json");
    assert!(schedule::looks_like(&bytes));
    let e = exchange::read(&bytes, "ZTEST-Lantern - Budget Days.json", None).unwrap_err();
    assert!(e.0.contains("link it as a schedule"), "{e}");
    assert!(!schedule::looks_like(b"{\"categories\": []}"));
}
