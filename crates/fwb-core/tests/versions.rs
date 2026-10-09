//! Named versions: save, restore, delete, rename.

mod common;

use common::*;
use fwb_core::*;

const T1: &str = "2026-10-09T10:00:00Z";
const T2: &str = "2026-10-09T11:00:00Z";

fn writer_rate(b: &Budget) -> Formula {
    b.categories().next().unwrap().accounts[0].details[0]
        .rate
        .clone()
}

fn set_writer_rate(b: &mut Budget, rate: &str) {
    b.categories_mut().next().unwrap().accounts[0].details[0].rate = rate.into();
}

#[test]
fn save_copies_the_content_without_versions() {
    let mut b = sample();
    let id = save_version(&mut b, "Draft 1", T1, Some("first pass".into()));
    assert_eq!(id, 1);
    let v = b.version(id).unwrap();
    assert_eq!((v.name.as_str(), v.created.as_str()), ("Draft 1", T1));
    assert_eq!(v.note.as_deref(), Some("first pass"));
    assert_eq!(v.budget.to_budget(), sample(), "snapshot is the content");

    // A second version doesn't contain the first.
    let id2 = save_version(&mut b, "Draft 2", T2, None);
    assert_eq!(id2, 2);
    assert_eq!(b.versions.len(), 2);
    assert!(b
        .version(id2)
        .unwrap()
        .budget
        .to_budget()
        .versions
        .is_empty());
    // Saving doesn't change the content.
    assert_eq!(b.snapshot(), sample().snapshot());
}

#[test]
fn ids_are_unique_after_deletes() {
    let mut b = sample();
    let ids: Vec<_> = (0..5)
        .map(|i| save_version(&mut b, format!("v{i}"), T1, None))
        .collect();
    assert_eq!(ids, vec![1, 2, 3, 4, 5]);
    delete_version(&mut b, 3).unwrap();
    delete_version(&mut b, 5).unwrap();
    let next = save_version(&mut b, "after", T1, None);
    let mut all: Vec<_> = b.versions.iter().map(|v| v.id).collect();
    all.sort_unstable();
    all.dedup();
    assert_eq!(all.len(), b.versions.len(), "ids unique");
    assert_eq!(next, 5, "one past the highest id in use");

    // Even when the highest id is u64::MAX.
    b.versions.last_mut().unwrap().id = u64::MAX;
    let id = save_version(&mut b, "wrap", T1, None);
    assert!(id != u64::MAX && b.versions.iter().filter(|v| v.id == id).count() == 1);
}

#[test]
fn restore_replaces_content_and_is_reversible() {
    let mut b = sample();
    let v1 = save_version(&mut b, "Approved", T1, None);
    set_writer_rate(&mut b, "175000");
    b.title = "Changed".into();
    let edited = b.snapshot();

    let auto = restore_version(&mut b, v1, T2).unwrap();
    assert_eq!(
        b.snapshot(),
        sample().snapshot(),
        "content is the version's"
    );
    assert_eq!(writer_rate(&b), Formula::from(150_000));
    assert_eq!(b.versions.len(), 2, "versions kept, plus the automatic one");
    let a = b.version(auto).unwrap();
    assert_eq!(a.name, "Before restoring \"Approved\"");
    assert_eq!(a.created, T2);
    assert_eq!(*a.budget, edited, "the replaced content was saved first");

    // Undo the restore by restoring the automatic version.
    restore_version(&mut b, auto, T2).unwrap();
    assert_eq!(b.snapshot(), edited);
    assert_eq!(b.versions.len(), 3);
    assert_eq!(b.version(v1).unwrap().budget.to_budget(), sample());
}

#[test]
fn rename_and_delete() {
    let mut b = sample();
    let id = save_version(&mut b, "Old", T1, None);
    rename_version(&mut b, id, "New").unwrap();
    assert_eq!(b.version(id).unwrap().name, "New");
    let gone = delete_version(&mut b, id).unwrap();
    assert_eq!(gone.name, "New");
    assert!(b.versions.is_empty());
}

#[test]
fn missing_versions_are_errors_and_change_nothing() {
    let mut b = sample();
    save_version(&mut b, "Only", T1, None);
    let before = b.clone();
    assert_eq!(
        restore_version(&mut b, 9, T2),
        Err(VersionError::NotFound(9))
    );
    assert_eq!(delete_version(&mut b, 9), Err(VersionError::NotFound(9)));
    assert_eq!(
        rename_version(&mut b, 9, "x"),
        Err(VersionError::NotFound(9))
    );
    assert_eq!(b, before);
    assert_eq!(
        VersionError::NotFound(9).to_string(),
        "there is no version 9"
    );
}

#[test]
fn highest_row_id_sees_versions_and_baseline() {
    let mut b = sample();
    assert_eq!(b.highest_row_id(), Some(8));
    let mut big = sample();
    big.categories_mut().next().unwrap().accounts[0].details[0].id = 500;
    let id = save_version(&mut big, "big", T1, None);
    b.versions.push(big.version(id).unwrap().clone());
    assert_eq!(b.highest_row_id(), Some(500));
    b.baseline = Some(Baseline {
        rows: [(900, 1)].into(),
        ..Default::default()
    });
    assert_eq!(b.highest_row_id(), Some(900));
    assert_eq!(Budget::new("x", "USD").highest_row_id(), None);
}

#[test]
fn parts_round_trip() {
    let mut b = sample();
    save_version(&mut b, "v", T1, None);
    let (content, versions) = b.clone().into_parts();
    assert_eq!(content, b.snapshot());
    assert_eq!(content.into_budget(versions), b);
}
