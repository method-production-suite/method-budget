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
        sig: None,
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
        sig: None,
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
        .grant(actor(3), Role::Editor, "Riley edits", T)
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
    assert!(LogDir::for_budget(&path).segments(e.actor()).is_empty());
    assert!(LogDir::for_budget(&path).segments(actor(2)).is_empty());
    drop(e);

    // An editor's change goes into their own folder; the file is the
    // keeper's to write, so it stays as it was.
    let riley_file = actor_file(&dir, 3);
    let mut e = SharedEdit::open(&path, Some(&riley_file)).unwrap();
    let mut after = e.budget();
    after.title = "Riley's".into();
    assert!(e.record(&after, "Command line: set title", T).unwrap());
    assert!(!e.record(&after, "again", T).unwrap(), "nothing new");
    // In the program's own folder, as Riley's.
    assert_ne!(e.actor(), actor(3));
    assert_eq!(e.person(), actor(3));
    assert_eq!(LogDir::for_budget(&path).segments(e.actor()).len(), 1);
    assert!(LogDir::for_budget(&path).segments(actor(3)).is_empty());
    drop(e);
    assert_eq!(fs::read(&path).unwrap(), file_before);
    let mut scan = Scan::default();
    log::absorb(&mut alex.log, &alex.dir, &mut scan);
    assert_eq!(alex.log.project().budget.title, "Riley's");
    assert_eq!(log::read_shared(&path).unwrap().title, "Riley's");

    // The keeper's own change rewrites the file too.
    let alex_file = actor_file(&dir, 1);
    let mut e = SharedEdit::open(&path, Some(&alex_file)).unwrap();
    let mut after = e.budget();
    after.title = "Alex's".into();
    assert!(e.record(&after, "Command line: set title", T).unwrap());
    let on_disk = method_format::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(on_disk.title, "Alex's");

    // A person with no role is refused, with nothing written.
    let kim_file = actor_file(&dir, 4);
    let mut e = SharedEdit::open(&path, Some(&kim_file)).unwrap();
    let mut after = e.budget();
    after.title = "Kim's".into();
    let err = e.record(&after, "Command line: set title", T).unwrap_err();
    assert_eq!(err.0, log::NO_ROLE);
    assert!(LogDir::for_budget(&path).segments(e.actor()).is_empty());
    drop(e);

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

/// The app and a command line on one computer write at the same moment:
/// the app has an edit not yet in the folder when the command line records
/// one. Each has its own actor, so neither takes the other's place in a
/// sequence: both edits reach everyone, nothing is refused, and every
/// computer ends the same (MReview, pull request #27).
#[test]
fn the_app_and_a_command_line_on_one_computer_never_share_a_sequence() {
    use method_core::log::Role;
    use method_format::log::SharedEdit;

    let dir = temp_dir("same-computer");
    let path = dir.join("Feature.mthd");
    write_budget(&path, &sample());
    let mut app = log::init(&path, actor(1), Ids::seeded(1), T).unwrap();
    app.log
        .grant(actor(2), Role::Editor, "Jordan edits", T)
        .unwrap();
    log::flush(&app.log, &app.dir).unwrap();
    let alex_file = actor_file(&dir, 1);

    // The app records a rate, not yet written to the folder.
    let p = app.log.project();
    let mut after = p.budget.clone();
    row_rate(&mut after, 0, 4321);
    app.log.record(&p, &after, &[], "Edit rate", T).unwrap();
    // Meanwhile a command line (and a second one, as an agent would be)
    // changes the title as the same person.
    let mut cli = SharedEdit::open(&path, Some(&alex_file)).unwrap();
    let agent = SharedEdit::open(&path, Some(&alex_file)).unwrap();
    assert_ne!(cli.actor(), app.log.actor());
    assert_ne!(cli.actor(), agent.actor());
    let mut b = cli.budget();
    b.title = "From the command line".into();
    assert!(cli.record(&b, "Command line: set title", T).unwrap());
    drop(agent);
    drop(cli);
    // The app writes its edit afterwards and reads the folder.
    log::flush(&app.log, &app.dir).unwrap();
    let mut scan = Scan::default();
    let r = log::absorb(&mut app.log, &app.dir, &mut scan);
    assert!(r.rejected.is_empty(), "{r:?}");
    let budget = app.log.project().budget;
    assert_eq!(budget.title, "From the command line");
    // Everyone opening the folder agrees, the edit of the app included.
    for n in [5, 6] {
        let (fresh, report) = log::open(&path, actor(n), Ids::seeded(n as u64)).unwrap();
        assert!(report.rejected.is_empty(), "{report:?}");
        assert_eq!(fresh.log.frontier(), app.log.frontier());
        assert_eq!(fresh.log.project().budget, budget);
    }
    let _ = fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// Signed changes (decisions.md, entry 89)

fn keyed(log: &mut method_core::log::Log, seed: u8) -> method_core::log::PublicKey {
    use method_core::log::Signer;
    let k = log::sign::KeyPair::from_seed([seed; 32]);
    let public = k.public();
    log.set_signer(std::sync::Arc::new(k));
    public
}

fn info_version(dir: &LogDir) -> u64 {
    let text = fs::read_to_string(dir.root().join("budget.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    v["version"].as_u64().unwrap()
}

/// Rewrites one segment with its last line replaced, header hash and all,
/// so the file reads as whole: what someone with access to the folder could
/// do.
fn rewrite_last_line(seg: &std::path::Path, f: impl FnOnce(&str) -> String) {
    let text = fs::read_to_string(seg).unwrap();
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    let last = lines.pop().unwrap();
    lines.push(f(&last));
    let changes: Vec<method_core::log::Change> = lines[1..]
        .iter()
        .map(|l| log::change_from_json(l).unwrap())
        .collect();
    // Re-made from the changes so the header matches; the altered line is
    // written as the reader will rebuild it.
    fs::write(seg, log::segment_bytes(&changes).unwrap()).unwrap();
}

#[test]
fn the_first_role_starts_signing_and_raises_the_log_format_to_3() {
    use method_core::log::Role;

    let dir = temp_dir("signed");
    let path = dir.join("Feature.mthd");
    write_budget(&path, &sample());
    let mut alex = log::init(&path, actor(1), Ids::seeded(1), T).unwrap();
    let alex_key = keyed(&mut alex.log, 1);
    // Edits before any role are not signed, and the format stays 1.
    let p = alex.log.project();
    let mut after = p.budget.clone();
    row_rate(&mut after, 0, 111);
    alex.log.record(&p, &after, &[], "Alex edits", T).unwrap();
    log::flush(&alex.log, &alex.dir).unwrap();
    assert_eq!(info_version(&alex.dir), 1);
    assert!(alex.log.changes().all(|c| c.sig.is_none()));

    alex.log
        .grant(actor(2), Role::Editor, "Jordan edits", T)
        .unwrap();
    let seg = log::flush(&alex.log, &alex.dir).unwrap().unwrap();
    assert_eq!(info_version(&alex.dir), 3);
    let text = fs::read_to_string(&seg).unwrap();
    assert!(
        text.lines().next().unwrap().contains("\"format\":3"),
        "{text}"
    );
    let line = text.lines().nth(1).unwrap();
    assert!(line.contains(r#""op":"key""#), "{line}");
    assert!(line.contains(&alex_key.hex()), "{line}");
    assert!(line.contains(r#""sig":""#), "{line}");

    // Jordan joins: the key goes in with Jordan's first change, signed.
    let (mut jordan, report) = log::open(&path, actor(2), Ids::seeded(2)).unwrap();
    assert!(report.rejected.is_empty(), "{:?}", report.rejected);
    keyed(&mut jordan.log, 2);
    let p = jordan.log.project();
    let mut after = p.budget.clone();
    after.title = "Jordan's".into();
    jordan
        .log
        .record(&p, &after, &[], "Jordan edits", T)
        .unwrap();
    log::flush(&jordan.log, &jordan.dir).unwrap();

    // A reader with no key of its own checks every signature.
    let (reader, report) = log::open(&path, actor(9), Ids::seeded(9)).unwrap();
    assert!(report.rejected.is_empty(), "{:?}", report.rejected);
    assert_eq!(reader.log.project().budget.title, "Jordan's");
    assert!(reader.log.signing_started());
    assert_eq!(reader.log.key_of(actor(1)), Some(alex_key));

    // No key file is ever written into the shared folder.
    let mut stack = vec![alex.dir.root().to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d).unwrap().flatten() {
            if e.path().is_dir() {
                stack.push(e.path());
            }
            assert_ne!(e.file_name(), log::sign::KEY_FILE);
        }
    }

    // A program that reads only format 2 refuses a folder at 3 as newer
    // than it reads, rather than taking in changes it can't check; this one
    // reads 3 (and 4, for invites).
    assert!(log::LOG_FORMAT_SIGNED <= log::LOG_FORMAT_VERSION);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn changes_altered_or_unsigned_in_the_folder_are_kept_aside() {
    use method_core::log::{Change, Field, Op, Role, Uid, Value};

    let dir = temp_dir("tampered");
    let path = dir.join("Feature.mthd");
    write_budget(&path, &sample());
    let mut alex = log::init(&path, actor(1), Ids::seeded(1), T).unwrap();
    keyed(&mut alex.log, 1);
    alex.log
        .grant(actor(2), Role::Editor, "Jordan edits", T)
        .unwrap();
    log::flush(&alex.log, &alex.dir).unwrap();
    let (mut jordan, _) = log::open(&path, actor(2), Ids::seeded(2)).unwrap();
    keyed(&mut jordan.log, 2);
    let p = jordan.log.project();
    let mut after = p.budget.clone();
    after.title = "Jordan's".into();
    jordan
        .log
        .record(&p, &after, &[], "Jordan edits", T)
        .unwrap();
    let mut after = jordan.log.project().budget.clone();
    after.title = "Jordan's second".into();
    let p = jordan.log.project();
    jordan
        .log
        .record(&p, &after, &[], "Jordan edits again", T)
        .unwrap();
    // Two segments: the key's, then the second edit's.
    let first = jordan.log.own_after(0)[0].clone();
    let second = jordan.log.own_after(0)[1].clone();
    jordan.dir.write_segment(&[first]).unwrap();
    let seg = jordan.dir.write_segment(&[second]).unwrap();

    // Someone with the folder changes the second edit's title.
    rewrite_last_line(&seg, |l| l.replace("Jordan's second", "Someone else's"));
    let (reader, report) = log::open(&path, actor(9), Ids::seeded(9)).unwrap();
    assert_eq!(report.rejected.len(), 1, "{:?}", report.rejected);
    assert!(
        report.rejected[0].1.contains("doesn't match"),
        "{:?}",
        report.rejected
    );
    assert_eq!(reader.log.project().budget.title, "Jordan's");
    let (_, why) = &reader.log.rejected()[0];
    assert!(
        why.starts_with(method_core::log::refused::SIGNATURE),
        "{why}"
    );

    // An unsigned change in Jordan's name, after Jordan's key.
    let unsigned = Change {
        actor: actor(2),
        seq: 3,
        deps: reader.log.frontier().clone(),
        lamport: reader.log.lamport() + 1,
        time: T.into(),
        label: "Unsigned".into(),
        ops: vec![Op::Set {
            entity: Uid::ROOT,
            field: Field::Title,
            value: Value::text("Unsigned"),
        }],
        sig: None,
    };
    jordan.dir.write_segment(&[unsigned]).unwrap();
    let (reader, report) = log::open(&path, actor(9), Ids::seeded(9)).unwrap();
    assert_eq!(report.rejected.len(), 2, "{:?}", report.rejected);
    assert!(
        report.rejected[1].1.contains("isn't signed"),
        "{:?}",
        report.rejected
    );
    assert_eq!(reader.log.project().budget.title, "Jordan's");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_removed_persons_later_changes_are_kept_aside_through_the_folder() {
    use method_core::log::Role;

    let dir = temp_dir("removed");
    let path = dir.join("Feature.mthd");
    write_budget(&path, &sample());
    let mut alex = log::init(&path, actor(1), Ids::seeded(1), T).unwrap();
    keyed(&mut alex.log, 1);
    alex.log
        .grant(actor(2), Role::Editor, "Jordan edits", T)
        .unwrap();
    log::flush(&alex.log, &alex.dir).unwrap();
    let (mut jordan, _) = log::open(&path, actor(2), Ids::seeded(2)).unwrap();
    keyed(&mut jordan.log, 2);
    let edit = |s: &mut log::Shared, title: &str| {
        let p = s.log.project();
        let mut after = p.budget.clone();
        after.title = title.into();
        s.log.record(&p, &after, &[], title, T).unwrap();
        log::flush(&s.log, &s.dir).unwrap();
    };
    edit(&mut jordan, "Seen before the removal");
    let mut scan = Scan::default();
    log::absorb(&mut alex.log, &alex.dir, &mut scan);
    edit(&mut jordan, "Made after, unseen");
    alex.log.revoke(actor(2), "Remove Jordan", T).unwrap();
    log::flush(&alex.log, &alex.dir).unwrap();

    let (reader, report) = log::open(&path, actor(9), Ids::seeded(9)).unwrap();
    assert_eq!(report.rejected.len(), 1, "{:?}", report.rejected);
    assert!(
        report.rejected[0].1.contains("removed"),
        "{:?}",
        report.rejected
    );
    assert_eq!(reader.log.project().budget.title, "Seen before the removal");
    assert!(reader.log.removed(actor(2)));
    // The line in the file names who, and the cutoff is the change's own deps.
    let seg = alex.dir.segments(actor(1)).last().unwrap().path.clone();
    let text = fs::read_to_string(seg).unwrap();
    assert!(text.contains(r#""op":"revoke""#), "{text}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_program_signs_with_its_persons_key_or_is_refused_in_words() {
    use method_core::log::Role;
    use method_format::log::SharedEdit;

    let dir = temp_dir("program-signs");
    let path = dir.join("Feature.mthd");
    write_budget(&path, &sample());
    let mut alex = log::init(&path, actor(1), Ids::seeded(1), T).unwrap();
    keyed(&mut alex.log, 1);
    alex.log
        .grant(actor(3), Role::Editor, "Riley edits", T)
        .unwrap();
    log::flush(&alex.log, &alex.dir).unwrap();

    // Riley's key file beside Riley's actor id; Riley's app hasn't published it.
    let riley_file = actor_file(&dir, 3);
    let riley_key = log::sign::load_or_make_key(&log::sign::key_path(&riley_file)).unwrap();
    let mut e = SharedEdit::open(&path, Some(&riley_file)).unwrap();
    let mut after = e.budget();
    after.title = "Riley's".into();
    let err = e.record(&after, "Command line: set title", T).unwrap_err();
    assert_eq!(err.0, log::NO_KEY_YET);
    assert!(LogDir::for_budget(&path).segments(e.actor()).is_empty());
    drop(e);

    // Riley's app publishes the key (as Riley's own actor).
    let (mut riley, _) = log::open(&path, actor(3), Ids::seeded(3)).unwrap();
    riley.log.set_signer(std::sync::Arc::new(riley_key));
    riley.log.publish_key("Key", T).unwrap().unwrap();
    log::flush(&riley.log, &riley.dir).unwrap();

    let mut e = SharedEdit::open(&path, Some(&riley_file)).unwrap();
    let mut after = e.budget();
    after.title = "Riley's".into();
    assert!(e.record(&after, "Command line: set title", T).unwrap());
    let tool = e.actor();
    drop(e);
    let seg = LogDir::for_budget(&path).segments(tool)[0].path.clone();
    let text = fs::read_to_string(seg).unwrap();
    assert!(
        text.contains(r#""op":"acts_for""#) && text.contains(r#""sig":""#),
        "{text}"
    );
    let (reader, report) = log::open(&path, actor(9), Ids::seeded(9)).unwrap();
    assert!(report.rejected.is_empty(), "{:?}", report.rejected);
    assert_eq!(reader.log.project().budget.title, "Riley's");
    assert_eq!(reader.log.person_of(tool), actor(3));

    // Removed, the program is told so.
    alex.log.revoke(actor(3), "Remove Riley", T).unwrap();
    log::flush(&alex.log, &alex.dir).unwrap();
    let mut e = SharedEdit::open(&path, Some(&riley_file)).unwrap();
    let mut after = e.budget();
    after.title = "Riley again".into();
    let err = e.record(&after, "Command line: set title", T).unwrap_err();
    assert_eq!(err.0, log::REMOVED);
    let _ = fs::remove_dir_all(&dir);
}

/// A key put into the folder in someone's name by someone else (decisions.md,
/// entry 89; MReview, pull request #73): the person's own programs compare it
/// with this computer's key, say so, and write nothing.
#[test]
fn a_key_in_a_persons_name_that_isnt_theirs_stops_their_programs() {
    use method_core::log::{Change, Op, Role, Signer};
    use method_format::log::SharedEdit;

    let dir = temp_dir("key-not-yours");
    let path = dir.join("Feature.mthd");
    write_budget(&path, &sample());
    let mut alex = log::init(&path, actor(1), Ids::seeded(1), T).unwrap();
    keyed(&mut alex.log, 1);
    alex.log
        .grant(actor(3), Role::Editor, "Riley edits", T)
        .unwrap();
    log::flush(&alex.log, &alex.dir).unwrap();

    // Someone with the folder publishes a key for Riley first.
    let theirs = log::sign::KeyPair::from_seed([77; 32]);
    let mut taken = Change {
        actor: actor(3),
        seq: 1,
        deps: alex.log.frontier().clone(),
        lamport: alex.log.lamport() + 1,
        time: T.into(),
        label: "Added this computer's key".into(),
        ops: vec![Op::Key {
            key: theirs.public(),
        }],
        sig: None,
    };
    taken.sig = Some(theirs.sign(&taken));
    alex.dir.write_segment(&[taken]).unwrap();

    let riley_file = actor_file(&dir, 3);
    log::sign::load_or_make_key(&log::sign::key_path(&riley_file)).unwrap();
    let mut e = SharedEdit::open(&path, Some(&riley_file)).unwrap();
    assert!(e.key_not_ours());
    let mut after = e.budget();
    after.title = "Riley's".into();
    let err = e.record(&after, "Command line: set title", T).unwrap_err();
    assert_eq!(err.0, log::KEY_NOT_YOURS);
    assert!(LogDir::for_budget(&path).segments(e.actor()).is_empty());
    // Riley's own app, as Riley's actor, signs nothing either.
    let (mut riley, _) = log::open(&path, actor(3), Ids::seeded(3)).unwrap();
    riley.log.set_signer(std::sync::Arc::new(
        log::sign::read_key(&log::sign::key_path(&riley_file)).unwrap(),
    ));
    assert_eq!(riley.log.key_conflict(), Some(theirs.public()));
    assert!(riley.log.publish_key("Key", T).unwrap().is_none());
    let p = riley.log.project();
    let mut after = p.budget.clone();
    after.title = "Riley's".into();
    assert!(riley.log.record(&p, &after, &[], "edit", T).is_err());
    let _ = fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// The owner's key (docs/research/owner-key.md)

/// A budget whose roles were set unsigned (by a version before signing) and a key put in
/// the owner's name by someone else before the owner's app did: the owner's
/// computer starts it again. The new folder takes the old one's place with
/// the budget as it stood, the owner's real key and everyone's roles; the
/// old folder is kept aside and ignored; members' apps notice; a computer
/// reading the folder afresh takes it all in; the owner's key is
/// remembered; and a stop between the two moves is finished on the next
/// open.
#[test]
fn the_owner_starts_a_budget_again_after_a_key_is_planted() {
    use method_core::log::{Change, Op, Role, Signer};

    let dir = temp_dir("start-again");
    let keys = dir.join("owner-keys");
    let path = dir.join("Feature.mthd");
    write_budget(&path, &sample());
    // Roles set by a version that didn't sign.
    let mut alex = log::init(&path, actor(1), Ids::seeded(1), T).unwrap();
    let cat = alex
        .log
        .project()
        .budget
        .categories()
        .next()
        .unwrap()
        .code
        .clone();
    let cat_uid = alex.log.project().binding.categories[&cat];
    alex.log
        .grant(actor(3), Role::Editor, "Riley edits", T)
        .unwrap();
    alex.log
        .grant(
            actor(4),
            Role::Department([cat_uid].into_iter().collect()),
            "Morgan edits a category",
            T,
        )
        .unwrap();
    let p = alex.log.project();
    let mut after = p.budget.clone();
    after.title = "Before the key".into();
    alex.log.record(&p, &after, &[], "Alex edits", T).unwrap();
    log::flush(&alex.log, &alex.dir).unwrap();
    let old_id = alex.info.budget_id.clone();

    // Someone with the folder puts a key in Alex's name first.
    let theirs = log::sign::KeyPair::from_seed([77; 32]);
    let mut planted = Change {
        actor: actor(1),
        seq: alex.log.own_seq() + 1,
        deps: alex.log.frontier().clone(),
        lamport: alex.log.lamport() + 1,
        time: T.into(),
        label: "Added this computer's key".into(),
        ops: vec![Op::Key {
            key: theirs.public(),
        }],
        sig: None,
    };
    planted.sig = Some(theirs.sign(&planted));
    alex.dir.write_segment(&[planted]).unwrap();

    // Riley's app has the budget open.
    let (riley, _) = log::open_in(&path, actor(3), Ids::seeded(3), Some(&keys)).unwrap();

    // Alex's computer finds the key isn't its own, and starts again.
    let (mut owner, _) = log::open_in(&path, actor(1), Ids::seeded(11), Some(&keys)).unwrap();
    let mine = keyed(&mut owner.log, 1);
    assert_eq!(owner.log.key_conflict(), Some(theirs.public()));
    let before = owner.log.project();
    let signer = std::sync::Arc::new(log::sign::KeyPair::from_seed([1; 32]));
    let other = std::sync::Arc::new(log::sign::KeyPair::from_seed([77; 32]));
    assert!(
        log::start_again(&owner, other, Some(&keys), Ids::seeded(12), T).is_err(),
        "nothing to start again when the key is this computer's"
    );
    let new = log::start_again(&owner, signer, Some(&keys), Ids::seeded(12), T).unwrap();

    assert_ne!(new.info.budget_id, old_id);
    assert_eq!(
        LogDir::for_budget(&path).info().unwrap().budget_id,
        new.info.budget_id
    );
    assert_eq!(new.log.key_of(actor(1)), Some(mine));
    assert!(new.log.signing_started());
    assert_eq!(new.log.role_of(actor(3)), Role::Editor);
    let morgan = new.log.role_of(actor(4));
    let new_cat = new.log.project().binding.categories[&cat];
    assert_eq!(morgan, Role::Department([new_cat].into_iter().collect()));
    let mut was = before.budget.clone();
    let mut now = new.log.project().budget;
    was.provenance = None;
    now.provenance = None;
    assert_eq!(
        method_format::to_string(&now),
        method_format::to_string(&was)
    );
    assert_eq!(now.title, "Before the key");
    assert_eq!(log::remembered_owner_key(&keys, &new.info), Some(mine));

    // The old folder is kept aside, marked and ignored.
    let aside: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("Feature.mthd-log-replaced-")
        })
        .collect();
    assert_eq!(aside.len(), 1, "{aside:?}");
    let marker = fs::read_to_string(aside[0].join(log::REPLACED_FILE)).unwrap();
    assert_eq!(marker.trim(), new.info.budget_id);
    assert!(!LogDir::at(&aside[0]).exists());

    // Riley's app notices; reading the folder afresh takes everything in.
    assert!(log::replaced(&riley));
    let (fresh, report) = log::open_in(&path, actor(3), Ids::seeded(13), Some(&keys)).unwrap();
    assert!(report.rejected.is_empty(), "{:?}", report.rejected);
    assert_eq!(fresh.log.key_of(actor(1)), Some(mine));
    assert_eq!(fresh.log.project().budget.title, "Before the key");

    // A stop between moving the old folder aside and moving the new one in
    // is finished by the next open.
    let staging = dir.join(".~Feature.mthd-log-new");
    fs::rename(LogDir::for_budget(&path).root(), &staging).unwrap();
    assert!(log::is_shared(&path));
    let (again, _) = log::open(&path, actor(9), Ids::seeded(14)).unwrap();
    assert_eq!(again.info.budget_id, new.info.budget_id);
    assert!(!staging.exists());
    // So is a stop after the old folder was marked and before it moved.
    let final_root = LogDir::for_budget(&path).root().to_path_buf();
    fs::rename(&final_root, &staging).unwrap();
    fs::create_dir(&final_root).unwrap();
    fs::write(final_root.join("budget.json"), "{}").unwrap();
    fs::write(final_root.join(log::REPLACED_FILE), "marked\n").unwrap();
    assert!(log::is_shared(&path));
    let (again, _) = log::open(&path, actor(9), Ids::seeded(15)).unwrap();
    assert_eq!(again.info.budget_id, new.info.budget_id);
    assert!(dir.join("Feature.mthd-log-replaced-unfinished").is_dir());
    let _ = fs::remove_dir_all(&dir);
}

/// Each computer remembers the owner key it first sees, outside the shared
/// folder; on the owner's own computer only its own key is remembered, so a
/// key planted before it looked is never taken for its own; and a key the
/// owner's program can't vouch for is never written.
#[test]
fn the_first_owner_key_seen_is_remembered_outside_the_folder() {
    use method_core::log::Role;

    let dir = temp_dir("owner-key-seen");
    let keys = dir.join("owner-keys");
    let path = dir.join("Feature.mthd");
    write_budget(&path, &sample());
    let mut alex = log::init(&path, actor(1), Ids::seeded(1), T).unwrap();
    let alex_key = keyed(&mut alex.log, 1);
    alex.log
        .grant(actor(3), Role::Editor, "Riley edits", T)
        .unwrap();
    log::flush(&alex.log, &alex.dir).unwrap();

    // Riley's computer remembers Alex's key once it has seen it.
    let (mut riley, _) = log::open_in(&path, actor(3), Ids::seeded(3), Some(&keys)).unwrap();
    assert!(log::remember_owner_key(&mut riley, &keys, actor(3), T).unwrap());
    assert_eq!(riley.log.owner_key_pin(), Some(alex_key));
    assert!(!log::remember_owner_key(&mut riley, &keys, actor(3), T).unwrap());
    assert_eq!(
        log::remembered_owner_key(&keys, &riley.info),
        Some(alex_key)
    );
    // Opened again, the key is checked from the first change read.
    let (again, _) = log::open_in(&path, actor(3), Ids::seeded(4), Some(&keys)).unwrap();
    assert_eq!(again.log.owner_key_pin(), Some(alex_key));
    // Nothing of it is in the shared folder.
    let mut stack = vec![alex.dir.root().to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d).unwrap().flatten() {
            if e.path().is_dir() {
                stack.push(e.path());
            }
            assert_ne!(
                e.file_name().to_string_lossy(),
                format!("{}.json", riley.info.budget_id)
            );
        }
    }

    // On the owner's computer with a different key (the one in the folder
    // isn't its own), nothing is remembered.
    let elsewhere = dir.join("elsewhere-keys");
    let (mut other, _) = log::open_in(&path, actor(1), Ids::seeded(5), Some(&elsewhere)).unwrap();
    keyed(&mut other.log, 2);
    assert!(!log::remember_owner_key(&mut other, &elsewhere, actor(1), T).unwrap());
    assert_eq!(log::remembered_owner_key(&elsewhere, &other.info), None);
    let _ = fs::remove_dir_all(&dir);
}

/// Someone with the shared folder swaps in a log of their own, as if the
/// owner had started again (MReview on pull request #73). A computer that
/// knew the budget refuses a log with another owner, and holds a log in the
/// owner's name with another key until the person trusts it, whether or not
/// the old folder is still there.
#[test]
fn a_forged_replacement_is_refused_or_held() {
    use method_core::log::Role;

    let dir = temp_dir("forged-replacement");
    let keys = dir.join("owner-keys");
    let path = dir.join("Feature.mthd");
    write_budget(&path, &sample());
    let mut alex = log::init(&path, actor(1), Ids::seeded(1), T).unwrap();
    let alex_key = keyed(&mut alex.log, 1);
    alex.log
        .grant(actor(3), Role::Editor, "Riley edits", T)
        .unwrap();
    log::flush(&alex.log, &alex.dir).unwrap();
    let (mut riley, _) = log::open_in(&path, actor(3), Ids::seeded(3), Some(&keys)).unwrap();
    keyed(&mut riley.log, 3);
    assert!(log::remember_owner_key(&mut riley, &keys, actor(3), T).unwrap());

    // A log by another actor put in the folder's place.
    let real = LogDir::for_budget(&path).root().to_path_buf();
    let kept = dir.join("kept-aside");
    fs::rename(&real, &kept).unwrap();
    let mut forged = log::init(&path, actor(5), Ids::seeded(5), T).unwrap();
    keyed(&mut forged.log, 5);
    forged
        .log
        .grant(actor(3), Role::Editor, "Riley edits", T)
        .unwrap();
    log::flush(&forged.log, &forged.dir).unwrap();
    match log::open_in(&path, actor(3), Ids::seeded(6), Some(&keys)) {
        Err(e) => assert_eq!(e.0, log::NOT_SAME_OWNER),
        Ok(_) => panic!("a log by another owner was opened"),
    }

    // A log in Alex's name, with someone else's key; the old folder gone.
    fs::remove_dir_all(&real).unwrap();
    fs::remove_dir_all(&kept).unwrap();
    let mut forged = log::init(&path, actor(1), Ids::seeded(7), T).unwrap();
    let theirs = keyed(&mut forged.log, 77);
    forged
        .log
        .grant(actor(3), Role::Editor, "Riley edits", T)
        .unwrap();
    log::flush(&forged.log, &forged.dir).unwrap();
    let (held, _) = log::open_in(&path, actor(3), Ids::seeded(8), Some(&keys)).unwrap();
    assert_eq!(held.log.owner_key_pin(), Some(alex_key));
    assert_eq!(held.log.owner_key_changed(), Some(theirs));
    assert_eq!(held.log.key_of(actor(1)), None);
    // A computer that never knew the budget can't tell.
    let (fresh, _) =
        log::open_in(&path, actor(4), Ids::seeded(9), Some(&dir.join("new-keys"))).unwrap();
    assert_eq!(fresh.log.key_of(actor(1)), Some(theirs));
    let _ = fs::remove_dir_all(&dir);
}

/// The owner's genuine Start Again: a computer that remembered the owner's
/// real key joins the new budget without a word, and one that had seen the
/// planted key is asked to trust the real one.
#[test]
fn a_genuine_start_again_joins_silently_where_the_real_key_was_known() {
    use method_core::log::{Change, Op, Signer};

    let dir = temp_dir("genuine-replacement");
    let path = dir.join("Feature.mthd");
    write_budget(&path, &sample());
    let alex = log::init(&path, actor(1), Ids::seeded(1), T).unwrap();
    let theirs = log::sign::KeyPair::from_seed([77; 32]);
    let mut planted = Change {
        actor: actor(1),
        seq: alex.log.own_seq() + 1,
        deps: alex.log.frontier().clone(),
        lamport: alex.log.lamport() + 1,
        time: T.into(),
        label: "Added this computer's key".into(),
        ops: vec![Op::Key {
            key: theirs.public(),
        }],
        sig: None,
    };
    planted.sig = Some(theirs.sign(&planted));
    alex.dir.write_segment(&[planted]).unwrap();
    let real = log::sign::KeyPair::from_seed([1; 32]).public();

    // Riley's computer saw the planted key; Morgan's was told the real one.
    let riley_keys = dir.join("riley-keys");
    let (mut riley, _) = log::open_in(&path, actor(3), Ids::seeded(3), Some(&riley_keys)).unwrap();
    keyed(&mut riley.log, 3);
    assert!(log::remember_owner_key(&mut riley, &riley_keys, actor(3), T).unwrap());
    let morgan_keys = dir.join("morgan-keys");
    let (morgan, _) = log::open_in(&path, actor(4), Ids::seeded(4), Some(&morgan_keys)).unwrap();
    log::trust_owner_key(&morgan_keys, &path, &morgan.info, real, T).unwrap();

    let (mut owner, _) = log::open_in(&path, actor(1), Ids::seeded(11), None).unwrap();
    keyed(&mut owner.log, 1);
    let signer = std::sync::Arc::new(log::sign::KeyPair::from_seed([1; 32]));
    let new = log::start_again(&owner, signer, None, Ids::seeded(12), T).unwrap();

    let (morgan, report) =
        log::open_in(&path, actor(4), Ids::seeded(5), Some(&morgan_keys)).unwrap();
    assert!(report.rejected.is_empty(), "{:?}", report.rejected);
    assert_eq!(morgan.info.budget_id, new.info.budget_id);
    assert_eq!(morgan.log.owner_key_changed(), None);
    assert_eq!(morgan.log.key_of(actor(1)), Some(real));

    let (riley, _) = log::open_in(&path, actor(3), Ids::seeded(6), Some(&riley_keys)).unwrap();
    assert_eq!(riley.log.owner_key_changed(), Some(real));
    log::trust_owner_key(&riley_keys, &path, &riley.info, real, T).unwrap();
    let (riley, report) = log::open_in(&path, actor(3), Ids::seeded(7), Some(&riley_keys)).unwrap();
    assert!(report.rejected.is_empty(), "{:?}", report.rejected);
    assert_eq!(riley.log.key_of(actor(1)), Some(real));
    let _ = fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// Invites (decisions.md, entry 97)

fn header_format(seg: &std::path::Path) -> u64 {
    let text = fs::read_to_string(seg).unwrap();
    let v: serde_json::Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();
    v["segment"]["format"].as_u64().unwrap()
}

#[test]
fn an_invite_raises_the_log_format_to_4_and_a_newcomer_joins_with_it() {
    use method_core::log::{invite::rfc3339_secs, Role, Uid};

    let dir = temp_dir("invite");
    let path = dir.join("Feature.mthd");
    write_budget(&path, &sample());
    let mut alex = log::init(&path, actor(1), Ids::seeded(1), T).unwrap();
    keyed(&mut alex.log, 1);
    alex.log
        .grant(actor(2), Role::Editor, "Jordan edits", T)
        .unwrap();
    let signed = log::flush(&alex.log, &alex.dir).unwrap().unwrap();
    assert_eq!(info_version(&alex.dir), 3, "no invite yet: 3, as before");
    let signed_bytes = fs::read(&signed).unwrap();

    // The invite: its secret never reaches the folder, only its key.
    let secret = [0x5a; 32];
    let id = Uid(0x1234);
    let expires = "2026-10-19T14:03:22Z";
    assert!(rfc3339_secs(expires).is_some());
    alex.log
        .invite(
            id,
            Role::Comment,
            expires,
            log::sign::invite_key(&secret),
            "Invite",
            T,
        )
        .unwrap();
    let seg = log::flush(&alex.log, &alex.dir).unwrap().unwrap();
    assert_eq!(info_version(&alex.dir), 4);
    assert_eq!(header_format(&seg), 4);
    let text = fs::read_to_string(&seg).unwrap();
    assert!(text.contains(r#""op":"invite""#), "{text}");
    assert!(!text.contains(&method_core::log::ids::hex(&secret)));
    // The segment written before it is unchanged, byte for byte.
    assert_eq!(fs::read(&signed).unwrap(), signed_bytes);
    assert_eq!(header_format(&signed), 3);

    // Riley's computer joins the folder and takes up the invite.
    let (mut riley, report) = log::open(&path, actor(6), Ids::seeded(6)).unwrap();
    assert!(report.rejected.is_empty(), "{:?}", report.rejected);
    keyed(&mut riley.log, 6);
    assert_eq!(riley.log.role_of(actor(6)), Role::ReadOnly);
    riley
        .log
        .claim(
            id,
            log::sign::claim_proof(&secret, id, actor(6)),
            "Joined",
            T,
        )
        .unwrap();
    let seg = log::flush(&riley.log, &riley.dir).unwrap().unwrap();
    assert_eq!(header_format(&seg), 4);
    // Copying Riley's claim into Morgan's name doesn't work: the proof names
    // Riley.
    let (mut morgan, _) = log::open(&path, actor(7), Ids::seeded(7)).unwrap();
    keyed(&mut morgan.log, 7);
    let e = morgan
        .log
        .claim(
            id,
            log::sign::claim_proof(&secret, id, actor(6)),
            "Joined",
            T,
        )
        .unwrap_err();
    assert!(e.to_string().contains("secret"), "{e}");

    // Every reader checks the claim with real Ed25519 and gives the role.
    let (reader, report) = log::open(&path, actor(9), Ids::seeded(9)).unwrap();
    assert!(report.rejected.is_empty(), "{:?}", report.rejected);
    assert_eq!(reader.log.role_of(actor(6)), Role::Comment);
    assert_eq!(reader.log.invite_claimant(id), Some(actor(6)));

    // A reader of format 3 (Method Budget 0.7.0 and older) refuses this
    // folder as newer than it reads: `budget.json` says 4, and that reader
    // compares it with its own 3 exactly as this one compares 99 here.
    let info = dir.join("Feature.mthd-log").join("budget.json");
    let text = fs::read_to_string(&info).unwrap();
    assert!(text.contains("\"version\": 4"), "{text}");
    fs::write(&info, text.replace("\"version\": 4", "\"version\": 99")).unwrap();
    let Err(err) = log::open(&path, actor(9), Ids::seeded(9)) else {
        panic!("a folder newer than this program reads was opened");
    };
    assert!(
        err.0.contains("newer than this program reads (4)"),
        "{}",
        err.0
    );
    assert_eq!(log::LOG_FORMAT_VERSION, log::LOG_FORMAT_INVITES);
    let _ = fs::remove_dir_all(&dir);
}
