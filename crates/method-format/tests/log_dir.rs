//! The change log on disk: a shared folder with two actors, segments,
//! outside edits, the keeper's snapshot, rolling and half-synced files
//! (docs/FORMAT.md, "The change log").

use std::fs;
use std::path::PathBuf;

use method_core::log::{ActorId, Ids};
use method_core::Budget;
use method_format::log::{self, LogDir, Scan};

const T: &str = "2026-10-12T14:03:22Z";

fn actor(n: u8) -> ActorId {
    let mut a = [0u8; 32];
    a[0] = n;
    ActorId(a)
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "method-log-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn sample() -> Budget {
    method_core::sample::budget(&method_core::sample::SampleSpec {
        rows: 40,
        accounts: 6,
        categories: 2,
        fringes: 4,
        groups: 3,
        ..Default::default()
    })
}

fn write_budget(path: &std::path::Path, b: &Budget) {
    fs::write(path, method_format::to_string(b)).unwrap();
}

fn row_rate(b: &mut Budget, index: usize, rate: i64) {
    let d = b
        .categories_mut()
        .flat_map(|c| &mut c.accounts)
        .flat_map(|a| &mut a.details)
        .nth(index)
        .unwrap();
    d.rate = rate.into();
}

#[test]
fn two_actors_share_a_folder() {
    let dir = temp_dir("share");
    let path = dir.join("Feature.mthd");
    write_budget(&path, &sample());

    // Alex starts sharing.
    let mut alex = log::init(&path, actor(1), Ids::seeded(1), T).unwrap();
    let log_dir = LogDir::for_budget(&path);
    assert!(log_dir.exists());
    assert_eq!(log_dir.root(), dir.join("Feature.mthd-log"));
    let info = log_dir.info().unwrap();
    assert_eq!(info.file, "Feature.mthd");
    assert_eq!(info.genesis.actor, actor(1));
    assert_eq!(log_dir.actors(), vec![actor(1)]);
    assert_eq!(log_dir.segments(actor(1)).len(), 1);

    // Jordan opens it.
    let (mut jordan, report) = log::open(&path, actor(2), Ids::seeded(2)).unwrap();
    assert_eq!(report.applied, 1);
    assert_eq!(jordan.log.frontier(), alex.log.frontier());
    assert_eq!(jordan.info.budget_id, info.budget_id);

    // Both edit apart and flush.
    let p = alex.log.project();
    let mut after = p.budget.clone();
    row_rate(&mut after, 0, 111);
    alex.log.record(&p, &after, &[], "Alex edits", T).unwrap();
    let p = jordan.log.project();
    let mut after = p.budget.clone();
    row_rate(&mut after, 1, 222);
    jordan
        .log
        .record(&p, &after, &[], "Jordan edits", T)
        .unwrap();
    let a_seg = log::flush(&alex.log, &alex.dir).unwrap().unwrap();
    assert!(a_seg.ends_with("000002-000002.chg"));
    assert!(log::flush(&alex.log, &alex.dir).unwrap().is_none());
    log::flush(&jordan.log, &jordan.dir).unwrap().unwrap();
    assert_eq!(log_dir.actors(), vec![actor(1), actor(2)]);
    // No temporary files are left behind.
    let leftovers: Vec<_> = fs::read_dir(log_dir.root().join("actors").join(actor(1).hex()))
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(".~"))
        .collect();
    assert!(leftovers.is_empty());

    // Each absorbs the other's segment through the scan hook.
    let mut a_scan = Scan::default();
    let r = log::absorb(&mut alex.log, &alex.dir, &mut a_scan);
    assert_eq!((r.applied, r.duplicates), (1, 2));
    let r = log::absorb(&mut alex.log, &alex.dir, &mut a_scan);
    assert_eq!((r.applied, r.duplicates), (0, 0));
    let mut j_scan = Scan::default();
    let r = log::absorb(&mut jordan.log, &jordan.dir, &mut j_scan);
    assert_eq!(r.applied, 1);
    assert_eq!(alex.log.frontier(), jordan.log.frontier());
    assert_eq!(alex.log.project().budget, jordan.log.project().budget);

    // The keeper writes the snapshot; it reads back as the merged state.
    let proj = log::write_snapshot(&alex).unwrap();
    let on_disk = method_format::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(on_disk, proj.budget);

    // An outside edit to the file is ingested as a change.
    let mut edited = on_disk.clone();
    edited.title = "Edited by hand".into();
    write_budget(&path, &edited);
    let id = log::ingest(&mut alex, T).unwrap().unwrap();
    assert_eq!(
        alex.log.change(id).unwrap().label,
        "Outside edit to Feature.mthd"
    );
    assert_eq!(alex.log.project().budget.title, "Edited by hand");
    assert_eq!(log::ingest(&mut alex, T).unwrap(), None);
    log::flush(&alex.log, &alex.dir).unwrap();

    // Rolling Alex's segments gives one file with the same changes.
    assert_eq!(log_dir.segments(actor(1)).len(), 3);
    let rolled = log_dir.roll(actor(1)).unwrap().unwrap();
    assert!(rolled.ends_with("000001-000003.chg"));
    assert_eq!(log_dir.segments(actor(1)).len(), 1);
    let (fresh, report) = log::open(&path, actor(3), Ids::seeded(3)).unwrap();
    assert_eq!(report.applied, 4);
    assert!(report.skipped.is_empty());
    assert_eq!(fresh.log.project().budget, alex.log.project().budget);

    // A half-synced segment is skipped until it is whole.
    let half = log_dir
        .root()
        .join("actors")
        .join(actor(2).hex())
        .join("000001-000001.chg");
    let bytes = fs::read(&half).unwrap();
    fs::write(&half, &bytes[..bytes.len() / 2]).unwrap();
    let (_, report) = log::open(&path, actor(4), Ids::seeded(4)).unwrap();
    assert_eq!(report.skipped.len(), 1);
    assert!(
        report.skipped[0].1.contains("hash mismatch") || report.skipped[0].1.contains("header")
    );
    fs::write(&half, &bytes).unwrap();
    let (_, report) = log::open(&path, actor(4), Ids::seeded(4)).unwrap();
    assert!(report.skipped.is_empty());

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn init_refuses_a_second_log_and_open_needs_one() {
    let dir = temp_dir("init");
    let path = dir.join("B.mthd");
    write_budget(&path, &Budget::new("B", "USD"));
    assert!(log::open(&path, actor(1), Ids::seeded(1)).is_err());
    log::init(&path, actor(1), Ids::seeded(1), T).unwrap();
    assert!(log::init(&path, actor(1), Ids::seeded(1), T).is_err());
    fs::remove_dir_all(&dir).unwrap();
}

/// Roles travel through the folder like any change, survive reopening,
/// and a change beyond its author's role is refused by everyone who reads
/// the folder, a newcomer included (decisions.md, entry 78).
#[test]
fn roles_travel_through_the_folder() {
    use method_core::log::{Change, LogError, Op, Role, Uid, Value};

    let dir = temp_dir("roles");
    let path = dir.join("Feature.mthd");
    write_budget(&path, &sample());
    let mut alex = log::init(&path, actor(1), Ids::seeded(1), T).unwrap();
    let (mut jordan, _) = log::open(&path, actor(2), Ids::seeded(2)).unwrap();

    alex.log
        .grant(actor(2), Role::ReadOnly, "Jordan reads", T)
        .unwrap();
    log::flush(&alex.log, &alex.dir).unwrap().unwrap();
    let mut j_scan = Scan::default();
    log::absorb(&mut jordan.log, &jordan.dir, &mut j_scan);
    assert_eq!(jordan.log.role_of(actor(2)), Role::ReadOnly);
    let p = jordan.log.project();
    let mut after = p.budget.clone();
    row_rate(&mut after, 0, 999);
    assert!(matches!(
        jordan.log.record(&p, &after, &[], "Jordan edits", T),
        Err(LogError::NotAllowed { .. })
    ));
    assert!(log::flush(&jordan.log, &jordan.dir).unwrap().is_none());

    // Another program writes a change in Jordan's name anyway.
    let forged = Change {
        actor: actor(2),
        seq: 1,
        deps: alex.log.frontier().clone(),
        lamport: alex.log.lamport() + 1,
        time: T.into(),
        label: "Forged".into(),
        ops: vec![Op::Set {
            entity: Uid::ROOT,
            field: method_core::log::Field::Title,
            value: Value::text("Forged"),
        }],
    };
    jordan.dir.write_segment(&[forged]).unwrap();
    let mut a_scan = Scan::default();
    let r = log::absorb(&mut alex.log, &alex.dir, &mut a_scan);
    assert_eq!(r.rejected.len(), 1, "{:?}", r.rejected);
    assert!(r.rejected[0].1.contains("not allowed"), "{:?}", r.rejected);
    assert_ne!(alex.log.project().budget.title, "Forged");

    // Someone opening the folder afresh reads the same roles and refuses
    // the same change.
    let (newcomer, report) = log::open(&path, actor(3), Ids::seeded(3)).unwrap();
    assert_eq!(report.rejected.len(), 1);
    assert_eq!(newcomer.log.role_of(actor(2)), Role::ReadOnly);
    assert_eq!(newcomer.log.role_of(actor(3)), Role::ReadOnly);
    assert_eq!(newcomer.log.owner(), Some(actor(1)));
    assert_eq!(newcomer.log.project().budget, alex.log.project().budget);
    let _ = fs::remove_dir_all(&dir);
}

/// A first change that didn't come from this budget's shared history (a
/// forged one, or a second computer that started sharing before the folder
/// synced) never takes the budget over: the owner is the one `budget.json`
/// names, and everyone opening the folder agrees (decisions.md, entry 78).
#[test]
fn a_forged_first_change_in_the_folder_never_takes_the_budget() {
    use method_core::log::{Change, Frontier, Op, Role};

    let dir = temp_dir("forged-genesis");
    let path = dir.join("Feature.mthd");
    write_budget(&path, &sample());
    // Actor 5 starts sharing; actor 1 sorts before it.
    let mut alex = log::init(&path, actor(5), Ids::seeded(5), T).unwrap();
    alex.log
        .grant(actor(2), Role::Editor, "Jordan edits", T)
        .unwrap();
    log::flush(&alex.log, &alex.dir).unwrap();
    let forged = Change {
        actor: actor(1),
        seq: 1,
        deps: Frontier::new(),
        lamport: 1,
        time: T.into(),
        label: "Forged".into(),
        ops: vec![Op::Grant {
            actor: actor(3),
            role: Role::Admin,
        }],
    };
    alex.dir.write_segment(&[forged]).unwrap();

    let mut a_scan = Scan::default();
    let r = log::absorb(&mut alex.log, &alex.dir, &mut a_scan);
    assert_eq!(r.rejected.len(), 1, "{r:?}");
    for n in [7, 8, 9] {
        let (fresh, report) = log::open(&path, actor(n), Ids::seeded(n as u64)).unwrap();
        assert_eq!(report.rejected.len(), 1, "{report:?}");
        assert_eq!(fresh.log.owner(), Some(actor(5)));
        assert_eq!(fresh.log.role_of(actor(3)), Role::ReadOnly);
        assert_eq!(fresh.log.role_of(actor(2)), Role::Editor);
        assert_eq!(fresh.log.frontier(), alex.log.frontier());
        assert_eq!(fresh.log.project().budget, alex.log.project().budget);
    }
    let _ = fs::remove_dir_all(&dir);
}

/// The first role raises the log's format, so an app too old to read a
/// role refuses the folder (its "newer than this program reads") instead
/// of skipping the role's file and editing a stale budget.
#[test]
fn the_first_role_raises_the_log_format() {
    use method_core::log::Role;

    let dir = temp_dir("format");
    let path = dir.join("Feature.mthd");
    write_budget(&path, &sample());
    let mut alex = log::init(&path, actor(1), Ids::seeded(1), T).unwrap();
    let info_path = alex.dir.root().join("budget.json");
    let version = || {
        let text = fs::read_to_string(&info_path).unwrap();
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        v["version"].as_u64().unwrap()
    };
    let header_format = |seg: &std::path::Path| {
        let text = fs::read_to_string(seg).unwrap();
        let first = text.lines().next().unwrap().to_string();
        let v: serde_json::Value = serde_json::from_str(&first).unwrap();
        v["segment"]["format"].as_u64().unwrap()
    };
    assert_eq!(version(), 1);
    let p = alex.log.project();
    let mut after = p.budget.clone();
    row_rate(&mut after, 0, 111);
    alex.log.record(&p, &after, &[], "Alex edits", T).unwrap();
    let seg = log::flush(&alex.log, &alex.dir).unwrap().unwrap();
    assert_eq!(version(), 1, "edits alone keep the format");
    assert_eq!(header_format(&seg), 1);

    alex.log
        .grant(actor(2), Role::ReadOnly, "Jordan reads", T)
        .unwrap();
    let seg = log::flush(&alex.log, &alex.dir).unwrap().unwrap();
    assert_eq!(version(), 2);
    assert_eq!(header_format(&seg), 2);
    // This version reads it back, roles and all.
    let (jordan, _) = log::open(&path, actor(2), Ids::seeded(2)).unwrap();
    assert_eq!(jordan.log.role_of(actor(2)), Role::ReadOnly);
    // A folder newer than this program reads is refused in words.
    let text = fs::read_to_string(&info_path)
        .unwrap()
        .replace("\"version\": 2", "\"version\": 99");
    fs::write(&info_path, text).unwrap();
    let err = log::open(&path, actor(2), Ids::seeded(2)).err().unwrap();
    assert!(err.0.contains("newer than this program reads"), "{}", err.0);
    let _ = fs::remove_dir_all(&dir);
}

fn actor_file(dir: &std::path::Path, n: u8) -> PathBuf {
    let f = dir.join(format!("actor-{n}"));
    fs::write(&f, format!("{}\n", actor(n).hex())).unwrap();
    f
}

/// The command line and the MCP server change a shared budget as this
/// computer's person, through the log and within their role; they never
/// write the `.mthd` unless they keep it (decisions.md, entry 78).
#[test]
fn a_program_without_a_window_changes_a_shared_budget_as_its_person() {
    use method_core::log::Role;
    use method_format::log::SharedEdit;

    let dir = temp_dir("shared-edit");
    let path = dir.join("Feature.mthd");
    write_budget(&path, &sample());
    let mut alex = log::init(&path, actor(1), Ids::seeded(1), T).unwrap();

    // Before any role, a computer with no person yet gets one and edits.
    let fresh = dir.join("new-person").join("actor");
    let mut e = SharedEdit::open(&path, Some(&fresh)).unwrap();
    assert!(fresh.is_file());
    let mut after = e.budget();
    after.title = "From the command line".into();
    assert!(e.record(&after, "Command line: set title", T).unwrap());
    let file_before = fs::read(&path).unwrap();

    alex.log
        .grant(actor(2), Role::ReadOnly, "Jordan reads", T)
        .unwrap();
    alex.log
        .grant(actor(3), Role::Editor, "Sam edits", T)
        .unwrap();
    log::flush(&alex.log, &alex.dir).unwrap();

    // A reader is refused, with nothing written.
    let jordan_file = actor_file(&dir, 2);
    let mut e = SharedEdit::open(&path, Some(&jordan_file)).unwrap();
    assert_eq!(e.role(), Role::ReadOnly);
    let mut after = e.budget();
    after.title = "Jordan's".into();
    let err = e.record(&after, "Command line: set title", T).unwrap_err();
    assert!(err.0.contains("role"), "{}", err.0);
    assert!(LogDir::for_budget(&path).segments(actor(2)).is_empty());

    // An editor's change goes into their own folder; the file is the
    // keeper's to write, so it stays as it was.
    let sam_file = actor_file(&dir, 3);
    let mut e = SharedEdit::open(&path, Some(&sam_file)).unwrap();
    let mut after = e.budget();
    after.title = "Sam's".into();
    assert!(e.record(&after, "Command line: set title", T).unwrap());
    assert!(!e.record(&after, "again", T).unwrap(), "nothing new");
    assert_eq!(LogDir::for_budget(&path).segments(actor(3)).len(), 1);
    assert_eq!(fs::read(&path).unwrap(), file_before);
    let mut scan = Scan::default();
    log::absorb(&mut alex.log, &alex.dir, &mut scan);
    assert_eq!(alex.log.project().budget.title, "Sam's");
    assert_eq!(log::read_shared(&path).unwrap().title, "Sam's");

    // The keeper's own change rewrites the file too.
    let alex_file = actor_file(&dir, 1);
    let mut e = SharedEdit::open(&path, Some(&alex_file)).unwrap();
    let mut after = e.budget();
    after.title = "Alex's".into();
    assert!(e.record(&after, "Command line: set title", T).unwrap());
    let on_disk = method_format::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(on_disk.title, "Alex's");

    // Once roles are set, a computer with no person is refused.
    let none = dir.join("nobody").join("actor");
    let err = SharedEdit::open(&path, Some(&none)).err().unwrap();
    assert_eq!(err.0, log::NO_PERSON);
    assert!(!none.exists());
    let _ = fs::remove_dir_all(&dir);
}

/// After roles are set, an edit to the `.mthd` by a program that doesn't
/// write the log is nobody's to take in: it is kept beside the file and
/// the file is written again from everyone's changes.
#[test]
fn an_outside_edit_after_roles_is_kept_aside() {
    use method_core::log::Role;

    let dir = temp_dir("set-aside");
    let path = dir.join("Feature.mthd");
    write_budget(&path, &sample());
    let mut alex = log::init(&path, actor(1), Ids::seeded(1), T).unwrap();
    alex.log
        .grant(actor(2), Role::ReadOnly, "Jordan reads", T)
        .unwrap();
    log::flush(&alex.log, &alex.dir).unwrap();
    log::write_snapshot(&alex).unwrap();
    assert_eq!(log::set_aside(&alex, T).unwrap(), None, "file matches");

    let mut edited = method_format::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    edited.title = "Edited elsewhere".into();
    write_budget(&path, &edited);
    let changes = alex.log.len();
    let kept = log::set_aside(&alex, T).unwrap().unwrap();
    assert_eq!(
        kept.file_name().unwrap().to_string_lossy(),
        "Feature (outside edit 2026-10-12 140322).mthd"
    );
    let kept_budget = method_format::from_str(&fs::read_to_string(&kept).unwrap()).unwrap();
    assert_eq!(kept_budget.title, "Edited elsewhere");
    let on_disk = method_format::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(on_disk.title, alex.log.project().budget.title);
    assert_eq!(alex.log.len(), changes, "nothing taken in");
    // A second outside edit never overwrites the first kept copy.
    write_budget(&path, &edited);
    let second = log::set_aside(&alex, T).unwrap().unwrap();
    assert_ne!(second, kept);
    assert!(kept.exists() && second.exists());
    let _ = fs::remove_dir_all(&dir);
}

/// A file that is only behind the log (the keeper hasn't written it since
/// someone else's change) is not an outside edit: taking it in would undo
/// that change. Every file the log wrote, or started from, is known by its
/// hash.
#[test]
fn a_file_behind_the_log_is_not_an_outside_edit() {
    use method_format::log::SharedEdit;

    let dir = temp_dir("behind");
    let path = dir.join("Feature.mthd");
    write_budget(&path, &sample());
    let alex = log::init(&path, actor(1), Ids::seeded(1), T).unwrap();
    drop(alex);
    let jordan_file = actor_file(&dir, 2);
    let mut e = SharedEdit::open(&path, Some(&jordan_file)).unwrap();
    let mut after = e.budget();
    after.title = "Jordan's".into();
    e.record(&after, "Command line: set title", T).unwrap();

    // Jordan syncs while the file is still the one Alex started from.
    let (mut jordan, _) = log::open(&path, actor(2), Ids::seeded(2)).unwrap();
    assert!(!log::has_outside_edit(&jordan, T).unwrap());
    assert_eq!(log::ingest(&mut jordan, T).unwrap(), None);
    assert_eq!(jordan.log.project().budget.title, "Jordan's");
    // The file as a snapshot wrote it is known too.
    log::write_snapshot(&jordan).unwrap();
    assert!(!log::has_outside_edit(&jordan, T).unwrap());
    // A real edit is still seen.
    let mut b = method_format::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    b.title = "By hand".into();
    write_budget(&path, &b);
    assert!(log::has_outside_edit(&jordan, T).unwrap());
    assert!(log::ingest(&mut jordan, T).unwrap().is_some());
    let _ = fs::remove_dir_all(&dir);
}
