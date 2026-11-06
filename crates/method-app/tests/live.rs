//! Two sessions whose shared folders never sync, on this computer's loopback
//! address with no relay: changes reach the other side only over the live
//! link (decisions.md, entry 98). Only the heartbeat files are copied across,
//! as a folder service would.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use method_app::edit;
use method_app::session::SessionFiles;
use method_app::shared::{Person, SharedDoc};
use method_app::Session;
use method_core::log::ActorId;
use method_sync::mesh::Mesh;

const ALEX: ActorId = ActorId([1; 32]);
const JORDAN: ActorId = ActorId([2; 32]);
const WAIT: Duration = Duration::from_secs(25);

fn temp_dir() -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "method-app-live-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn person(name: &str, color: &str) -> Person {
    Person {
        name: name.into(),
        color: color.into(),
    }
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

struct Two {
    alex: Session,
    jordan: Session,
    alex_log: PathBuf,
    jordan_log: PathBuf,
}

/// Alex shares a budget, Jordan joins, then each works on their own copy of
/// the folder, which from then on never sync.
fn apart() -> Two {
    let here = temp_dir();
    let path = here.join("Feature.mthd");
    let mut first = Session::with_budget(method_core::starter::budget_with_separator('-'), None);
    first.save_to(&path).unwrap();
    let mut alex = Session::open(&path).unwrap();
    alex.start_sharing(ALEX, person("Alex Kim", "teal"))
        .unwrap();
    let mut jordan = Session::open(&path).unwrap();
    jordan
        .join_shared(JORDAN, person("Jordan Lee", "blue"))
        .unwrap();
    drop(jordan);
    let alex_log = alex.shared().unwrap().folder().to_path_buf();

    let there = temp_dir();
    copy_tree(&here, &there);
    let jordan_path = there.join("Feature.mthd");
    let mut jordan = Session::open(&jordan_path).unwrap();
    jordan
        .join_shared(JORDAN, person("Jordan Lee", "blue"))
        .unwrap();
    let jordan_log = jordan.shared().unwrap().folder().to_path_buf();
    assert_ne!(alex_log, jordan_log);
    Two {
        alex,
        jordan,
        alex_log,
        jordan_log,
    }
}

fn presence(log: &Path, actor: ActorId) -> PathBuf {
    log.join("actors").join(actor.hex()).join("presence.json")
}

impl Two {
    fn doc(s: &mut Session) -> &mut SharedDoc {
        s.shared_mut().unwrap()
    }

    fn both_live(&mut self) {
        for (s, seed) in [(&mut self.alex, 11u8), (&mut self.jordan, 12u8)] {
            let doc = Two::doc(s);
            let budget = doc.live_budget().unwrap();
            doc.live_with(Mesh::start_loopback([seed; 32], &budget).unwrap());
        }
    }

    /// One round: heartbeat files cross, then each side's timer runs. The
    /// clock is ahead so changes flush and heartbeats are read every time.
    fn round(&mut self, ahead: u64) {
        let now = Instant::now() + Duration::from_secs(ahead);
        self.alex.share_tick(now);
        self.jordan.share_tick(now);
        let _ = std::fs::copy(
            presence(&self.alex_log, ALEX),
            presence(&self.jordan_log, ALEX),
        );
        let _ = std::fs::copy(
            presence(&self.jordan_log, JORDAN),
            presence(&self.alex_log, JORDAN),
        );
    }

    fn until(&mut self, what: &str, mut done: impl FnMut(&mut Two) -> bool) {
        let start = Instant::now();
        let mut ahead = 10;
        while !done(self) {
            assert!(start.elapsed() < WAIT, "not seen in time: {what}");
            ahead += 10;
            self.round(ahead);
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn linked(&mut self) {
        self.both_live();
        self.until("the link", |t| {
            Two::doc(&mut t.alex).live_connected() == 1
                && Two::doc(&mut t.jordan).live_connected() == 1
        });
    }
}

fn first_line(s: &Session) -> method_core::RowId {
    s.budget()
        .categories()
        .flat_map(|c| &c.accounts)
        .find(|a| a.code == "50-01")
        .unwrap()
        .details[0]
        .id
}

fn rate_of(s: &Session, id: method_core::RowId) -> String {
    edit::details(s.budget())
        .find(|d| d.id == id)
        .unwrap()
        .rate
        .0
        .clone()
}

fn address_in(log: &Path, actor: ActorId) -> bool {
    std::fs::read_to_string(presence(log, actor)).is_ok_and(|text| text.contains("\"live\""))
}

#[test]
fn with_live_mode_off_nothing_crosses_and_no_address_is_written() {
    let mut t = apart();
    let id = first_line(&t.alex);
    t.alex.set_detail_field(id, "rate", "4321").unwrap();
    for ahead in [10, 40, 70] {
        t.round(ahead);
    }
    assert!(!address_in(&t.alex_log, ALEX));
    assert!(!address_in(&t.jordan_log, JORDAN));
    assert_ne!(rate_of(&t.jordan, id), "4321", "the folders never sync");
    assert_eq!(Two::doc(&mut t.alex).live_connected(), 0);
}

#[test]
fn an_edit_crosses_the_link_and_both_sessions_converge() {
    let mut t = apart();
    t.linked();
    assert!(address_in(&t.alex_log, ALEX));
    let id = first_line(&t.alex);
    t.alex.set_detail_field(id, "rate", "4321").unwrap();
    t.until("Alex's edit at Jordan", |t| {
        rate_of(&t.jordan, id) == "4321"
    });

    let other = first_line(&t.jordan);
    t.jordan
        .set_detail_field(other, "description", "From Jordan")
        .unwrap();
    t.until("Jordan's edit at Alex", |t| {
        edit::details(t.alex.budget()).any(|d| d.description == "From Jordan")
    });
    assert_eq!(t.alex.budget(), t.jordan.budget());
}

/// A change goes on the link only once a segment holds it, so a computer that
/// stops before writing can't reuse its number for a different change.
#[test]
fn a_change_not_yet_written_stays_here() {
    let mut t = apart();
    t.linked();
    let id = first_line(&t.alex);
    t.alex.set_detail_field(id, "rate", "4321").unwrap();
    // A pass right after the edit: typing hasn't paused, so nothing is written.
    let soon = Instant::now();
    t.alex.share_tick(soon);
    assert!(
        Two::doc(&mut t.alex).unshared() > 0,
        "should still be unwritten"
    );
    std::thread::sleep(Duration::from_millis(500));
    t.jordan
        .share_tick(Instant::now() + Duration::from_secs(10));
    assert_ne!(rate_of(&t.jordan, id), "4321");
    t.until("the written change", |t| rate_of(&t.jordan, id) == "4321");
}

/// Someone who connects later, or was away, gets what they lack.
#[test]
fn changes_made_before_the_link_arrive_when_it_opens() {
    let mut t = apart();
    let id = first_line(&t.alex);
    t.alex.set_detail_field(id, "rate", "4321").unwrap();
    t.round(10);
    assert_ne!(rate_of(&t.jordan, id), "4321");
    t.linked();
    t.until("the earlier edit", |t| rate_of(&t.jordan, id) == "4321");
}

#[test]
fn turning_it_off_closes_the_link_and_takes_the_address_out() {
    let mut t = apart();
    t.linked();
    assert!(address_in(&t.jordan_log, JORDAN));
    Two::doc(&mut t.jordan).live_stop();
    assert!(!Two::doc(&mut t.jordan).live_on());
    assert!(!address_in(&t.jordan_log, JORDAN));
    t.until("Alex hears", |t| {
        Two::doc(&mut t.alex).live_connected() == 0
    });

    // Alex's edits no longer reach Jordan.
    let id = first_line(&t.alex);
    t.alex.set_detail_field(id, "rate", "4321").unwrap();
    for ahead in [200, 230, 260] {
        t.round(ahead);
    }
    assert_ne!(rate_of(&t.jordan, id), "4321");
}

/// One person turning it on doesn't take anyone else online.
#[test]
fn one_person_turning_it_on_leaves_the_other_offline() {
    let mut t = apart();
    let doc = Two::doc(&mut t.alex);
    let budget = doc.live_budget().unwrap();
    doc.live_with(Mesh::start_loopback([11; 32], &budget).unwrap());
    for ahead in [10, 40, 70] {
        t.round(ahead);
    }
    assert!(!Two::doc(&mut t.jordan).live_on());
    assert!(!address_in(&t.jordan_log, JORDAN));
    assert_eq!(Two::doc(&mut t.alex).live_connected(), 0);
}

/// Saving is not a way out: live mode stays on.
#[test]
fn saving_leaves_it_on() {
    let mut t = apart();
    t.linked();
    Two::doc(&mut t.alex).flush_all();
    assert!(Two::doc(&mut t.alex).live_on());
    assert!(address_in(&t.alex_log, ALEX));
}

/// Closing the budget ends live mode with it.
#[test]
fn closing_the_budget_closes_the_link_and_takes_the_address_out() {
    let mut t = apart();
    t.linked();
    let Two {
        alex,
        mut jordan,
        alex_log,
        ..
    } = t;
    drop(alex);
    assert!(!address_in(&alex_log, ALEX));
    let start = Instant::now();
    while Two::doc(&mut jordan).live_connected() != 0 {
        assert!(start.elapsed() < WAIT, "Jordan never heard");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// A person removed from the budget stops receiving changes at once.
#[test]
fn someone_removed_from_the_budget_loses_their_link() {
    let mut t = apart();
    let key = |seed: u8| -> std::sync::Arc<dyn method_core::log::Signer> {
        std::sync::Arc::new(method_format::log::sign::KeyPair::from_seed([seed; 32]))
    };
    Two::doc(&mut t.alex).use_key(key(1)).unwrap();
    Two::doc(&mut t.jordan).use_key(key(2)).unwrap();
    t.linked();
    Two::doc(&mut t.alex)
        .set_role(&JORDAN.hex(), "editor", &[])
        .unwrap();
    t.until("Jordan's role at Jordan", |t| {
        Two::doc(&mut t.jordan).log().roles_started()
    });
    // A damaged heartbeat from someone else doesn't stop the removal.
    let third = ActorId([3; 32]);
    let at = presence(&t.alex_log, ALEX);
    let mine: serde_json::Value = serde_json::from_slice(&std::fs::read(&at).unwrap()).unwrap();
    let mut bad = mine.clone();
    bad["live"] = serde_json::Value::String("{\"id\":\"x\"}".to_string());
    let bad_path = presence(&t.alex_log, third);
    std::fs::create_dir_all(bad_path.parent().unwrap()).unwrap();
    std::fs::write(&bad_path, serde_json::to_vec(&bad).unwrap()).unwrap();
    t.round(100);

    Two::doc(&mut t.alex).remove_person(&JORDAN.hex()).unwrap();
    t.until("the link gone", |t| {
        Two::doc(&mut t.alex).live_connected() == 0 && Two::doc(&mut t.jordan).live_connected() == 0
    });
}

fn set_field(path: &Path, field: &str, value: &str) {
    let mut p: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    p[field] = serde_json::Value::String(value.to_string());
    std::fs::write(path, serde_json::to_vec(&p).unwrap()).unwrap();
}

const LONG_AGO: &str = "2020-01-01T00:00:00Z";

/// A folder service that lags doesn't close a link that works.
#[test]
fn an_old_heartbeat_leaves_an_open_link_open() {
    let mut t = apart();
    t.linked();
    set_field(&presence(&t.alex_log, JORDAN), "at", LONG_AGO);
    set_field(&presence(&t.jordan_log, ALEX), "at", LONG_AGO);
    let now = Instant::now() + Duration::from_secs(500);
    t.alex.share_tick(now);
    t.jordan.share_tick(now);
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(Two::doc(&mut t.alex).live_connected(), 1);
    assert_eq!(Two::doc(&mut t.jordan).live_connected(), 1);
}

/// An address left by a computer gone quiet long ago isn't dialed.
#[test]
fn an_old_heartbeat_with_no_link_is_not_dialed() {
    let mut t = apart();
    t.both_live();
    // The addresses cross already old, so nobody has dialed them yet.
    std::fs::copy(presence(&t.alex_log, ALEX), presence(&t.jordan_log, ALEX)).unwrap();
    std::fs::copy(
        presence(&t.jordan_log, JORDAN),
        presence(&t.alex_log, JORDAN),
    )
    .unwrap();
    set_field(&presence(&t.alex_log, JORDAN), "at", LONG_AGO);
    set_field(&presence(&t.jordan_log, ALEX), "at", LONG_AGO);
    let now = Instant::now() + Duration::from_secs(500);
    t.alex.share_tick(now);
    t.jordan.share_tick(now);
    std::thread::sleep(Duration::from_millis(800));
    assert_eq!(Two::doc(&mut t.alex).live_connected(), 0);
}

/// An address a crash left behind comes out the next time the budget opens.
#[test]
fn the_next_start_clears_an_address_left_by_a_crash() {
    let mut t = apart();
    t.round(10);
    set_field(
        &presence(&t.jordan_log, JORDAN),
        "live",
        "{\"id\":\"left over\"}",
    );
    assert!(address_in(&t.jordan_log, JORDAN));
    t.round(40);
    assert!(!address_in(&t.jordan_log, JORDAN));
}

/// The owner starting the budget again ends live mode here.
#[test]
fn a_budget_started_again_ends_it() {
    let mut t = apart();
    t.linked();
    set_field(
        &t.jordan_log.join("budget.json"),
        "budget_id",
        "another budget",
    );
    t.jordan
        .share_tick(Instant::now() + Duration::from_secs(10));
    assert!(!Two::doc(&mut t.jordan).live_on());
    assert!(!address_in(&t.jordan_log, JORDAN));
    let start = Instant::now();
    while Two::doc(&mut t.alex).live_connected() != 0 {
        assert!(start.elapsed() < WAIT, "Alex never heard");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Quitting doesn't drop the budgets, so it stops live changes itself.
#[test]
fn quitting_takes_the_address_out() {
    let mut t = apart();
    t.linked();
    assert!(address_in(&t.alex_log, ALEX));
    let Two {
        alex,
        mut jordan,
        alex_log,
        ..
    } = t;
    let state = method_app::app::AppState::default();
    let kept = state.docs().add(alex);
    method_app::safety_app::stop_live_everywhere(&state);
    assert!(!address_in(&alex_log, ALEX));
    assert!(!state
        .docs()
        .get_mut(kept)
        .unwrap()
        .shared_mut()
        .unwrap()
        .live_on());
    let start = Instant::now();
    while Two::doc(&mut jordan).live_connected() != 0 {
        assert!(start.elapsed() < WAIT, "Jordan never heard");
        std::thread::sleep(Duration::from_millis(50));
    }
}
