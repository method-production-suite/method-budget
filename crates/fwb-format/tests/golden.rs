//! Golden snapshot: the exact bytes of a small budget's file.
//!
//! If the format changes on purpose, regenerate with
//! `FWB_UPDATE_GOLDEN=1 cargo test -p fwb-format --test golden`, review the diff,
//! and update docs/FORMAT.md to match.

mod common;

use std::path::PathBuf;

use common::*;
use fwb_core::*;

fn small() -> Budget {
    let mut b = Budget::new("Short Film", "USD");
    b.units
        .retain(|k, _| k == "Day" || k == "Flat" || k == "Week");
    b.info
        .insert("Shoot days".into(), InfoValue::Number(d("5")));
    b.globals.insert("DAYS".into(), "5".into());
    b.fringes.insert(
        "Payroll".into(),
        Fringe {
            description: "Payroll taxes".into(),
            rate: "18.5".into(),
            kind: FringeKind::Percent {
                cutoff: Some(15_000_000),
            },
        },
    );
    b.groups.insert("Alt".into(), group(false));

    let mut director = Detail::line(1, "Director", 1, "Flat", 5_000);
    director.fringes = set(&["Payroll"]);
    let mut dp = Detail::line(2, "Director of photography", "DAYS", "Day", "850.50");
    dp.fringes = set(&["Payroll"]);
    let mut drone = Detail::line(4, "Drone package", 1, "Week", 1_200);
    drone.groups = set(&["Alt"]);

    b.topsheet = vec![
        category(
            "10",
            "Above-the-line",
            vec![account("10-01", "Director", vec![director])],
        ),
        level("Total Above-the-Line"),
        category(
            "20",
            "Production",
            vec![account(
                "20-01",
                "Camera",
                vec![dp, Detail::adjustment(3, "Kit rental", "0.1"), drone],
            )],
        ),
        level("Total Below-the-Line"),
    ];
    b.charges.push(percent_charge("Contingency", "10"));
    b
}

#[test]
fn small_budget_matches_golden_file() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/small.json");
    let text = fwb_format::to_string(&small());
    if std::env::var_os("FWB_UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, &text).unwrap();
    }
    let golden = std::fs::read_to_string(&path).expect("golden file exists");
    assert_eq!(text, golden, "output differs from {}", path.display());
    assert_eq!(fwb_format::from_str(&golden).unwrap(), small());
}

#[test]
fn format_doc_example_matches_golden_file() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let doc = std::fs::read_to_string(root.join("../../docs/FORMAT.md")).unwrap();
    let golden = std::fs::read_to_string(root.join("tests/golden/small.json")).unwrap();
    assert!(
        doc.contains(&format!("```json\n{golden}```")),
        "docs/FORMAT.md's complete example must match tests/golden/small.json"
    );
}

#[test]
fn format_doc_versions_example_reads() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let doc = std::fs::read_to_string(root.join("../../docs/FORMAT.md")).unwrap();
    let start = doc
        .find("```json\n\"versions\": [")
        .expect("FORMAT.md has a versions example")
        + "```json\n".len();
    let example = &doc[start..start + doc[start..].find("```").unwrap()];
    let text = format!(
        "{{\"format\": \"film-wizard-budget\", \"version\": 1, \"budget\": \
         {{\"title\": \"Short Film\", \"base_currency\": \"USD\", {example}}}}}"
    );
    let b = fwb_format::from_str(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));
    let v = &b.versions[0];
    assert_eq!((v.id, v.name.as_str()), (1, "Studio draft"));
    assert_eq!(v.note.as_deref(), Some("Sent to the studio"));
    assert_eq!(v.budget.categories().count(), 1);
}
