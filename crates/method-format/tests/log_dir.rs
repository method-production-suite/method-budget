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
