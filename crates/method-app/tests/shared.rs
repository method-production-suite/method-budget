//! Two sessions on one shared folder (src/shared.rs): edits in each become
//! changes in the log, the timer passes carry them across, and both end
//! with the same budget; conflicts are listed and settled; undo takes back
//! one person's own changes; the keeper writes the file and takes outside
//! edits in.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use method_app::edit;
use method_app::session::SessionFiles;
use method_app::shared::{Person, SNAPSHOT_QUIET};
use method_app::Session;
use method_core::log::{ActorId, Op};
use method_core::RowId;

fn temp_dir() -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "method-app-shared-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const ALEX: ActorId = ActorId([1; 32]);
const JORDAN: ActorId = ActorId([2; 32]);

fn person(name: &str, color: &str) -> Person {
    Person {
        name: name.into(),
        color: color.into(),
    }
}

/// A saved starter budget, shared by Alex (who keeps the file) and joined
/// by Jordan.
fn two_sessions() -> (PathBuf, Session, Session) {
    let dir = temp_dir();
    let path = dir.join("Feature.mthd");
    let mut first = Session::with_budget(method_core::starter::budget_with_separator('-'), None);
    first.save_to(&path).unwrap();
    let mut alex = Session::open(&path).unwrap();
    alex.start_sharing(ALEX, person("Alex Kim", "teal"))
        .unwrap();
    assert!(alex.is_shared());
    assert!(alex.shared().unwrap().is_keeper());
    let mut jordan = Session::open(&path).unwrap();
    let report = jordan
        .join_shared(JORDAN, person("Jordan Lee", "blue"))
        .unwrap();
    assert!(report.applied >= 1, "{report:?}");
    assert!(!jordan.shared().unwrap().is_keeper());
    assert_eq!(alex.budget(), jordan.budget());
    (path, alex, jordan)
}

fn ids_in(s: &Session, account: &str) -> Vec<RowId> {
    s.budget()
        .categories()
        .flat_map(|c| &c.accounts)
        .find(|a| a.code == account)
        .unwrap()
        .details
        .iter()
        .map(|d| d.id)
        .collect()
}

fn rate_of(s: &Session, id: RowId) -> String {
    edit::details(s.budget())
        .find(|d| d.id == id)
        .unwrap()
        .rate
        .0
        .clone()
}

/// A timer pass well after the last edit, so changes are written.
fn later() -> Instant {
    Instant::now() + Duration::from_secs(3)
}

/// Both sessions write what they have and read what the other wrote.
fn exchange(a: &mut Session, b: &mut Session) {
    a.share_tick(later());
    b.share_tick(later());
    a.share_tick(later());
    b.share_tick(later());
}

#[test]
fn edits_cross_the_folder_and_both_sessions_converge() {
    let (_path, mut alex, mut jordan) = two_sessions();
    let id = ids_in(&alex, "50-01")[0];
    alex.set_detail_field(id, "rate", "4321").unwrap();
    assert!(alex.is_dirty(), "not yet in the folder");
    assert!(alex.can_undo());
    let out = alex.share_tick(later()).unwrap();
    assert!(
        !out.budget_changed,
        "my own changes don't come back as news"
    );
    assert!(!alex.is_dirty() || alex.shared().unwrap().is_keeper());
    let out = jordan.share_tick(later()).unwrap();
    assert!(out.budget_changed);
    assert_eq!(out.remote.len(), 1);
    let rc = &out.remote[0];
    assert_eq!(rc.name, "Alex Kim");
    assert_eq!(rc.color, "teal");
    assert!(rc.label.starts_with("Edit rate on 50-01"), "{}", rc.label);
    assert!(
        rc.places.iter().any(|p| p.kind == "row"
            && p.key == id.to_string()
            && p.field.as_deref() == Some("rate")),
        "{:?}",
        rc.places
    );
    assert_eq!(rate_of(&jordan, id), "4321");
    assert_eq!(alex.budget(), jordan.budget());

    // Jordan adds a line: it gets an id nobody else can make.
    let before: Vec<RowId> = ids_in(&jordan, "50-01");
    let returned = jordan.add_detail("50-01", None, "line").unwrap();
    let after: Vec<RowId> = ids_in(&jordan, "50-01");
    let new: Vec<RowId> = after
        .iter()
        .copied()
        .filter(|i| !before.contains(i))
        .collect();
    assert_eq!(new.len(), 1);
    assert!(
        new[0] >= 1 << 32,
        "random id from the shared range: {}",
        new[0]
    );
    let v = jordan.view_with(Some(returned));
    assert_eq!(
        v.created_row,
        Some(new[0]),
        "the row the command made is found by its new id"
    );
    assert!(v.shared);
    exchange(&mut alex, &mut jordan);
    assert_eq!(alex.budget(), jordan.budget());
    assert_eq!(ids_in(&alex, "50-01"), after);

    // Status: each sees the other person, with name and color from the
    // heartbeat, and nothing to review.
    let st = alex.shared().unwrap().status(Instant::now());
    assert_eq!(st.mode, "folder");
    assert!(st.keeper);
    assert_eq!(st.people.len(), 1);
    assert_eq!(st.people[0].name, "Jordan Lee");
    assert_eq!(st.people[0].color, "blue");
    assert_eq!(st.people[0].initials, "JL");
    assert!(!st.people[0].away);
    assert_eq!(st.review_count, 0);
    assert_eq!(st.unshared, 0);
    assert!(st.error.is_none());
    let st = jordan.shared().unwrap().status(Instant::now());
    assert!(!st.keeper);
    assert!(st.people[0].keeper, "Alex keeps the file");
}

#[test]
fn a_value_changed_by_two_people_is_listed_and_settled() {
    let (_path, mut alex, mut jordan) = two_sessions();
    let id = ids_in(&alex, "50-01")[0];
    alex.set_detail_field(id, "rate", "100").unwrap();
    jordan.set_detail_field(id, "rate", "200").unwrap();
    exchange(&mut alex, &mut jordan);
    assert_eq!(alex.budget(), jordan.budget());
    let kept = rate_of(&alex, id);
    assert!(kept == "100" || kept == "200");
    for s in [&alex, &jordan] {
        let review = s.shared().unwrap().review();
        assert_eq!(review.len(), 1, "{review:?}");
        let r = &review[0];
        assert_eq!(r.key, "overwritten");
        let place = r.place.as_ref().unwrap();
        assert_eq!(place.kind, "row");
        assert_eq!(place.key, id.to_string());
        assert_eq!(place.field.as_deref(), Some("rate"));
        assert_eq!(place.parent.as_deref(), Some("50-01"));
        assert_eq!(r.values.len(), 2);
        assert!(r.values.iter().any(|v| v.kept && v.value == kept));
        let names: Vec<&str> = r.values.iter().map(|v| v.name.as_str()).collect();
        assert!(
            names.contains(&"Alex Kim") && names.contains(&"Jordan Lee"),
            "{names:?}"
        );
        assert!(
            r.sentence.contains("Alex Kim") || r.sentence.contains("Jordan Lee"),
            "{}",
            r.sentence
        );
        // The status marks the cell.
        let st = s.shared().unwrap().status(Instant::now());
        assert_eq!(st.review_count, 1);
        assert_eq!(st.conflicts.len(), 1);
    }
    // Who changed this: every value, oldest first, the current one marked.
    let blame = alex
        .shared()
        .unwrap()
        .blame("row", &id.to_string(), "rate")
        .unwrap();
    assert!(blame.len() >= 3, "{blame:?}");
    assert!(blame.last().unwrap().kept);
    assert_eq!(blame.last().unwrap().value, kept);
    // Jordan picks the other value back: a new change, shared like any.
    let lost = blame
        .iter()
        .position(|v| !v.kept && (v.value == "100" || v.value == "200"))
        .unwrap();
    let wanted = blame[lost].value.clone();
    jordan
        .share_use_value("row", &id.to_string(), "rate", lost)
        .unwrap();
    assert_eq!(rate_of(&jordan, id), wanted);
    exchange(&mut alex, &mut jordan);
    assert_eq!(alex.budget(), jordan.budget());
    assert_eq!(rate_of(&alex, id), wanted);
    assert!(
        alex.shared().unwrap().review().is_empty(),
        "a write that knows both settles it"
    );
}

#[test]
fn undo_takes_back_only_my_own_changes() {
    let (_path, mut alex, mut jordan) = two_sessions();
    let ids: Vec<RowId> = edit::details(alex.budget())
        .filter(|d| d.kind == method_core::RowKind::Line)
        .map(|d| d.id)
        .take(3)
        .collect();
    assert_eq!(ids.len(), 3);
    alex.set_detail_field(ids[0], "rate", "111").unwrap();
    jordan.set_detail_field(ids[1], "rate", "222").unwrap();
    exchange(&mut alex, &mut jordan);
    assert_eq!(rate_of(&alex, ids[0]), "111");
    assert_eq!(rate_of(&alex, ids[1]), "222");
    // Alex's history lists Alex's step only.
    let h = alex.history();
    assert_eq!(h.steps.len(), 1);
    assert!(
        h.steps[0].label.starts_with("Edit rate on "),
        "{}",
        h.steps[0].label
    );
    assert!(alex.undo());
    assert_ne!(rate_of(&alex, ids[0]), "111", "Alex's change is taken back");
    assert_eq!(rate_of(&alex, ids[1]), "222", "Jordan's stays");
    assert!(alex.can_redo());
    assert!(!alex.can_undo());
    exchange(&mut alex, &mut jordan);
    assert_eq!(alex.budget(), jordan.budget());
    assert!(alex.redo());
    assert_eq!(rate_of(&alex, ids[0]), "111");
    exchange(&mut alex, &mut jordan);
    assert_eq!(alex.budget(), jordan.budget());

    // A change someone made on top of mine is skipped, and said so.
    alex.set_detail_field(ids[2], "rate", "333").unwrap();
    exchange(&mut alex, &mut jordan);
    jordan.set_detail_field(ids[2], "rate", "444").unwrap();
    exchange(&mut alex, &mut jordan);
    assert!(alex.undo());
    let v = alex.view();
    let notice = v.notice.expect("the skip is reported");
    assert!(notice.contains("Jordan Lee"), "{notice}");
    assert_eq!(rate_of(&alex, ids[2]), "444", "Jordan's later value stays");
}

#[test]
fn the_keeper_writes_the_file_and_takes_outside_edits_in() {
    let (path, mut alex, mut jordan) = two_sessions();
    let id = ids_in(&alex, "50-01")[0];
    jordan.set_detail_field(id, "rate", "777").unwrap();
    exchange(&mut alex, &mut jordan);
    // Quiet long enough: the keeper rewrites the file from the merged state.
    alex.share_tick(Instant::now() + SNAPSHOT_QUIET + Duration::from_secs(4));
    let on_disk = method_format::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(&on_disk, alex.budget());
    assert!(!alex.is_dirty());
    let st = alex.shared().unwrap().status(Instant::now());
    assert!(st.file_written.is_some());

    // Another program edits the file: the keeper records it as a change
    // and everyone gets it.
    let mut outside = on_disk.clone();
    edit::set_detail_field(&mut outside, id, edit::Field::Rate, "888").unwrap();
    std::fs::write(&path, method_format::to_string(&outside)).unwrap();
    let out = alex.share_tick(later()).unwrap();
    assert!(out.budget_changed);
    assert_eq!(rate_of(&alex, id), "888");
    let last = alex
        .shared()
        .unwrap()
        .log()
        .changes()
        .last()
        .unwrap()
        .clone();
    assert!(last.label.starts_with("Outside edit to"), "{}", last.label);
    exchange(&mut alex, &mut jordan);
    assert_eq!(rate_of(&jordan, id), "888");
    assert_eq!(alex.budget(), jordan.budget());

    // Jordan never writes the file; Save puts Jordan's changes in the folder.
    jordan.set_detail_field(id, "rate", "999").unwrap();
    assert!(jordan.is_dirty());
    jordan.save_to(&path).unwrap();
    assert!(!jordan.is_dirty());
    let on_disk = method_format::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_ne!(
        edit::details(&on_disk).find(|d| d.id == id).unwrap().rate.0,
        "999",
        "only the keeper writes the file"
    );
    let other = path.with_file_name("Elsewhere.mthd");
    let e = jordan.save_to(&other).unwrap_err();
    assert!(e.contains("stays in its shared folder"), "{e}");
    assert!(!other.exists());

    // A newcomer opening the file later gets the merged budget, not the file's.
    let mut priya = Session::open(&path).unwrap();
    assert_eq!(rate_of(&priya, id), "888", "the file is behind the log");
    priya
        .join_shared(ActorId([3; 32]), person("Priya", "rose"))
        .unwrap();
    assert_eq!(
        rate_of(&priya, id),
        "999",
        "joining takes the merged budget"
    );
    assert_eq!(priya.budget(), jordan.budget());
}

#[test]
fn renames_are_one_write_and_versions_from_history_are_refused() {
    let (_path, mut alex, mut jordan) = two_sessions();
    let fringe = alex
        .budget()
        .fringes
        .keys()
        .next()
        .cloned()
        .expect("the starter has fringes");
    let kind = method_core::rename::SetupKind::Fringe;
    alex.edit_with_hints(
        "Rename fringe",
        &[method_core::log::Hint::rename(
            kind,
            &fringe,
            "Payroll Taxes",
        )],
        |b| {
            method_core::rename::rename(b, kind, &fringe, "Payroll Taxes")
                .map(|_| ())
                .map_err(|e| e.to_string())
        },
    )
    .unwrap();
    let last = alex
        .shared()
        .unwrap()
        .log()
        .changes()
        .last()
        .unwrap()
        .clone();
    let creates = last
        .ops
        .iter()
        .filter(|op| matches!(op, Op::Create { .. }))
        .count();
    assert_eq!(creates, 0, "a rename makes nothing new: {:?}", last.ops);
    assert!(last.ops.iter().any(|op| matches!(
        op,
        Op::Set {
            field: method_core::log::Field::Name,
            ..
        }
    )));
    exchange(&mut alex, &mut jordan);
    assert!(jordan.budget().fringes.contains_key("Payroll Taxes"));
    assert_eq!(alex.budget(), jordan.budget());

    let e = alex.budget_at(0).unwrap_err();
    assert!(e.contains("as it is now"), "{e}");
    // The History tab can still jump between my steps.
    alex.jump_to(0).unwrap();
    assert!(!alex.budget().fringes.contains_key("Payroll Taxes"));
    alex.jump_to(1).unwrap();
    assert!(alex.budget().fringes.contains_key("Payroll Taxes"));
}

/// The category an account is in, and a line in an account of another
/// category.
fn category_of(s: &Session, account: &str) -> String {
    s.budget()
        .categories()
        .find(|c| c.accounts.iter().any(|a| a.code == account))
        .unwrap()
        .code
        .clone()
}

fn line_outside(s: &Session, category: &str) -> RowId {
    s.budget()
        .categories()
        .filter(|c| c.code != category)
        .flat_map(|c| &c.accounts)
        .flat_map(|a| &a.details)
        .find(|d| d.kind == method_core::RowKind::Line)
        .unwrap()
        .id
}

#[test]
fn roles_limit_what_each_person_changes() {
    let (path, mut alex, mut jordan) = two_sessions();
    exchange(&mut alex, &mut jordan);
    let cat = category_of(&alex, "50-01");
    let inside = ids_in(&alex, "50-01")[0];
    let outside = line_outside(&alex, &cat);

    // Before any role, Jordan edits everything and nobody reads only.
    let st = alex.shared().unwrap().status(Instant::now());
    assert!(st.can_set_roles);
    assert!(!st.roles_started);
    assert_eq!(st.me.role, "owner");

    // Alex lets Jordan edit one category.
    alex.shared_mut()
        .unwrap()
        .set_role(&JORDAN.hex(), "department", std::slice::from_ref(&cat))
        .unwrap();
    assert!(alex.is_dirty(), "the role is a change to share");
    let st = alex.shared().unwrap().status(Instant::now());
    assert!(st.roles_started);
    let j = st.people.iter().find(|p| p.actor == JORDAN.hex()).unwrap();
    assert_eq!(j.role, "department");
    assert_eq!(j.scope, vec![cat.clone()]);
    assert!(j.role_text.contains(&cat), "{}", j.role_text);
    exchange(&mut alex, &mut jordan);
    let st = jordan.shared().unwrap().status(Instant::now());
    assert_eq!(st.me.role, "department");
    assert!(!st.can_set_roles);

    // Inside the category: fine. Outside: refused in plain words, and
    // nothing changes.
    jordan.set_detail_field(inside, "rate", "77").unwrap();
    let before = jordan.budget().clone();
    let err = jordan.set_detail_field(outside, "rate", "88").unwrap_err();
    assert!(
        err.starts_with("You can change lines and notes in"),
        "{err}"
    );
    assert!(err.contains("Alex Kim"), "{err}");
    assert_eq!(jordan.budget(), &before);
    // Jordan sets nobody's role.
    assert!(jordan
        .shared_mut()
        .unwrap()
        .set_role(&ALEX.hex(), "read_only", &[])
        .is_err());
    assert!(jordan
        .shared_mut()
        .unwrap()
        .set_role(&ActorId([3; 32]).hex(), "editor", &[])
        .is_err());

    // Apart: Alex makes Jordan read only while Jordan changes the rate
    // again. Jordan hadn't seen it, so that change is kept everywhere.
    alex.shared_mut()
        .unwrap()
        .set_role(&JORDAN.hex(), "read_only", &[])
        .unwrap();
    jordan.set_detail_field(inside, "rate", "79").unwrap();
    exchange(&mut alex, &mut jordan);
    assert_eq!(rate_of(&alex, inside), "79");
    assert_eq!(alex.budget(), jordan.budget());
    let st = alex.shared().unwrap().status(Instant::now());
    assert_eq!(st.refused, 0);

    // Now every edit of Jordan's is refused before it is made.
    let err = jordan.set_detail_field(inside, "rate", "80").unwrap_err();
    assert!(
        err.starts_with("You can read this budget but not change it."),
        "{err}"
    );
    assert!(jordan.check_writable().is_err());
    assert_eq!(rate_of(&jordan, inside), "79");

    // The role is in the folder: Jordan opening the budget again reads only.
    alex.share_flush();
    let mut again = Session::open(&path).unwrap();
    again
        .join_shared(JORDAN, person("Jordan Lee", "blue"))
        .unwrap();
    assert!(again.check_writable().is_err());
    assert_eq!(again.budget(), alex.budget());
}

/// Setting a role checks what is asked for before anything is written.
#[test]
fn setting_a_role_checks_what_is_asked() {
    let (_path, mut alex, _jordan) = two_sessions();
    let sh = alex.shared_mut().unwrap();
    for (who, role, scope, why) in [
        (JORDAN.hex(), "producer", vec![], "role"),
        (JORDAN.hex(), "department", vec![], "Pick"),
        (
            JORDAN.hex(),
            "department",
            vec!["99-99".to_string()],
            "99-99",
        ),
        ("not an id".to_string(), "editor", vec![], "person"),
        (ALEX.hex(), "read_only", vec![], "owner"),
        (JORDAN.hex(), "owner", vec![], "owner"),
    ] {
        let err = sh.set_role(&who, role, &scope).unwrap_err();
        assert!(err.contains(why), "{role}: {err}");
    }
    assert_eq!(sh.unshared(), 0, "nothing was written");
}

/// A second person for a live check of the built app (ignored: run by
/// hand). `METHOD_LIVE_BUDGET` names a shared `.mthd`; this joins it as
/// Jordan Lee, changes the first line's rate (to `METHOD_LIVE_RATE`,
/// 4321), adds a line (unless `METHOD_LIVE_NO_INSERT` is set), and then
/// stays for `METHOD_LIVE_SECONDS` (60) writing heartbeats and printing
/// what arrives, so the app shows presence, flashes and a conflict.
#[test]
#[ignore]
fn live_second_actor() {
    let Some(path) = std::env::var_os("METHOD_LIVE_BUDGET") else {
        return;
    };
    let secs: u64 = std::env::var("METHOD_LIVE_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(60);
    let path = PathBuf::from(path);
    let mut jordan = Session::open(&path).unwrap();
    let report = jordan
        .join_shared(ActorId([9; 32]), person("Jordan Lee", "blue"))
        .unwrap();
    println!("joined: {report:?}");
    let first = edit::details(jordan.budget())
        .find(|d| d.kind == method_core::RowKind::Line)
        .map(|d| d.id)
        .unwrap();
    let account = method_app::history::row_place(jordan.budget(), first)
        .unwrap()
        .0;
    let rate = std::env::var("METHOD_LIVE_RATE").unwrap_or_else(|_| "4321".into());
    jordan.set_detail_field(first, "rate", &rate).unwrap();
    if std::env::var_os("METHOD_LIVE_NO_INSERT").is_none() {
        jordan.add_detail(&account, Some(first), "line").unwrap();
    }
    jordan.share_flush();
    println!("edited line {first} in {account} (rate {rate})");
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(secs) {
        std::thread::sleep(Duration::from_secs(1));
        if let Some(out) = jordan.share_tick(Instant::now()) {
            for rc in &out.remote {
                println!(
                    "arrived: {} {} ({} places)",
                    rc.name,
                    rc.label,
                    rc.places.len()
                );
            }
        }
    }
    let st = jordan.shared().unwrap().status(Instant::now());
    println!(
        "people seen: {:?}",
        st.people
            .iter()
            .map(|p| (&p.name, &p.color, p.seen_secs))
            .collect::<Vec<_>>()
    );
    println!("to review: {}", st.review_count);
    for r in jordan.shared().unwrap().review() {
        println!("  {}", r.sentence);
    }
}
