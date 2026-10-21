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
        (
            JORDAN.hex(),
            "editor",
            vec!["50".to_string()],
            "Edits some categories",
        ),
        (
            JORDAN.hex(),
            "read_only",
            vec!["50".to_string()],
            "Edits some categories",
        ),
    ] {
        let err = sh.set_role(&who, role, &scope).unwrap_err();
        assert!(err.contains(why), "{role}: {err}");
    }
    assert_eq!(sh.unshared(), 0, "nothing was written");
}

/// After roles are set, an edit to the `.mthd` by a program that doesn't
/// write the log isn't taken in as the keeper's: it is kept beside the file,
/// the file is written again, and the status says so in words.
#[test]
fn an_outside_edit_after_roles_is_kept_aside_by_the_keeper() {
    let (path, mut alex, mut jordan) = two_sessions();
    alex.shared_mut()
        .unwrap()
        .set_role(&JORDAN.hex(), "editor", &[])
        .unwrap();
    exchange(&mut alex, &mut jordan);
    alex.share_tick(Instant::now() + SNAPSHOT_QUIET + Duration::from_secs(3));
    let changes = alex.shared().unwrap().log().len();
    let title = alex.budget().title.clone();

    let mut edited = method_format::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    edited.title = "Edited by hand".into();
    std::fs::write(&path, method_format::to_string(&edited)).unwrap();
    alex.share_tick(later());

    assert_eq!(alex.budget().title, title);
    assert_eq!(
        alex.shared().unwrap().log().len(),
        changes,
        "nothing taken in"
    );
    let on_disk = method_format::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(on_disk.title, title);
    let st = alex.shared().unwrap().status(Instant::now());
    let notice = st.notice.unwrap();
    assert!(notice.contains("(outside edit"), "{notice}");
    assert!(notice.contains("Feature.mthd"), "{notice}");
    let kept = std::fs::read_dir(path.parent().unwrap())
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().contains("(outside edit"))
        .count();
    assert_eq!(kept, 1);
}

/// A change left out because its author's role didn't allow it is listed
/// in the status: who, when, what, and why, in plain words.
#[test]
fn refused_changes_are_shown_with_who_when_and_what() {
    let (_path, mut alex, mut jordan) = two_sessions();
    exchange(&mut alex, &mut jordan);
    alex.shared_mut()
        .unwrap()
        .set_role(&JORDAN.hex(), "read_only", &[])
        .unwrap();
    alex.share_flush();
    // Another program writes a change in Jordan's name anyway.
    let sh = alex.shared().unwrap();
    let forged = method_core::log::Change {
        actor: JORDAN,
        seq: sh.log().frontier().get(&JORDAN) + 1,
        deps: sh.log().frontier().clone(),
        lamport: sh.log().lamport() + 1,
        time: "2026-10-18T14:03:00Z".into(),
        label: "Edit title".into(),
        ops: vec![Op::Set {
            entity: method_core::log::Uid::ROOT,
            field: method_core::log::Field::Title,
            value: method_core::log::Value::text("Forged"),
        }],
        sig: None,
    };
    method_format::log::LogDir::at(sh.folder())
        .write_segment(&[forged])
        .unwrap();
    alex.share_tick(later());
    let st = alex.shared().unwrap().status(Instant::now());
    assert_eq!(st.refused, 1);
    let r = &st.refused_changes[0];
    assert_eq!(r.name, "Jordan Lee");
    assert_eq!(r.label, "Edit title");
    assert_eq!(r.time, "2026-10-18T14:03:00Z");
    assert!(
        r.why.starts_with("Their role doesn\u{2019}t allow it"),
        "{}",
        r.why
    );
    assert_ne!(alex.budget().title, "Forged");
}

/// The owner makes someone a co-owner, who then sets roles, admins
/// included, from their own computer (decisions.md, entry 78; the owner's
/// answer of 2026-10-18).
#[test]
fn a_co_owner_sets_roles_from_their_own_computer() {
    let (_path, mut alex, mut jordan) = two_sessions();
    let riley = ActorId([3; 32]);
    alex.shared_mut()
        .unwrap()
        .set_role(&JORDAN.hex(), "co_owner", &[])
        .unwrap();
    exchange(&mut alex, &mut jordan);
    let st = jordan.shared().unwrap().status(Instant::now());
    assert_eq!(st.me.role, "co_owner");
    assert!(st.can_set_roles);
    assert!(
        st.me.role_text.starts_with("Co-owner"),
        "{}",
        st.me.role_text
    );
    // Jordan makes Riley an admin, and the owner sees it.
    jordan
        .shared_mut()
        .unwrap()
        .set_role(&riley.hex(), "admin", &[])
        .unwrap();
    // But makes no co-owners and leaves the owner alone.
    let err = jordan
        .shared_mut()
        .unwrap()
        .set_role(&riley.hex(), "co_owner", &[])
        .unwrap_err();
    assert!(err.contains("Only the owner makes co-owners"), "{err}");
    assert!(jordan
        .shared_mut()
        .unwrap()
        .set_role(&ALEX.hex(), "read_only", &[])
        .is_err());
    exchange(&mut alex, &mut jordan);
    let st = alex.shared().unwrap().status(Instant::now());
    let s = st.people.iter().find(|p| p.actor == riley.hex()).unwrap();
    assert_eq!(s.role, "admin");
    assert_eq!(st.refused, 0);
}

/// A command line or agent on Jordan's computer writes as an actor of its
/// own that writes for Jordan: its changes arrive under Jordan's name, and
/// it isn't listed as another person.
#[test]
fn a_program_writing_for_a_person_shows_with_their_name() {
    let (path, mut alex, mut jordan) = two_sessions();
    alex.shared_mut()
        .unwrap()
        .set_role(&JORDAN.hex(), "editor", &[])
        .unwrap();
    exchange(&mut alex, &mut jordan);
    let dir = path.parent().unwrap().to_path_buf();
    let jordan_file = dir.join("jordan-actor");
    std::fs::write(&jordan_file, JORDAN.hex()).unwrap();
    let mut e = method_format::log::SharedEdit::open(&path, Some(&jordan_file)).unwrap();
    let mut b = e.budget();
    b.title = "From Jordan's command line".into();
    e.record(&b, "Command line: set title", "2026-10-18T14:00:00Z")
        .unwrap();
    drop(e);
    let out = alex.share_tick(later()).unwrap();
    assert_eq!(alex.budget().title, "From Jordan's command line");
    let names: Vec<&str> = out.remote.iter().map(|r| r.name.as_str()).collect();
    assert!(
        names.contains(&"Jordan Lee (command line or agent)"),
        "{names:?}"
    );
    let st = alex.shared().unwrap().status(Instant::now());
    assert_eq!(st.people.len(), 1, "{:?}", st.people);
    assert_eq!(st.refused, 0);
}

/// A file put back to an older version of the budget (a sync service's
/// history, a backup) is behind the log, not an outside edit: the keeper
/// writes it again and says so.
#[test]
fn an_older_copy_of_the_file_is_written_again() {
    let (path, mut alex, _jordan) = two_sessions();
    let id = ids_in(&alex, "50-01")[0];
    alex.set_detail_field(id, "rate", "111").unwrap();
    alex.share_tick(Instant::now() + SNAPSHOT_QUIET + Duration::from_secs(3));
    let older = std::fs::read(&path).unwrap();
    alex.set_detail_field(id, "rate", "222").unwrap();
    alex.share_tick(Instant::now() + SNAPSHOT_QUIET + Duration::from_secs(3));
    let newer = std::fs::read(&path).unwrap();
    assert_ne!(older, newer);
    std::fs::write(&path, &older).unwrap();
    alex.share_tick(later());
    assert_eq!(std::fs::read(&path).unwrap(), newer);
    assert_eq!(rate_of(&alex, id), "222");
    let notice = alex
        .shared()
        .unwrap()
        .status(Instant::now())
        .notice
        .unwrap();
    assert!(notice.contains("older version"), "{notice}");
}

/// Before roles, a program's actor doesn't say whose it is in the log.
/// This computer knows its own person's programs from its tool pool and
/// shows them as that person; another computer's program shows as a
/// program whose person isn't known yet, never as a new person.
#[test]
fn programs_before_roles_are_never_listed_as_people() {
    let (path, mut alex, mut jordan) = two_sessions();
    exchange(&mut alex, &mut jordan);
    let dir = path.parent().unwrap().to_path_buf();
    let jordan_file = dir.join("jordan-actor");
    std::fs::write(&jordan_file, JORDAN.hex()).unwrap();
    let mut e = method_format::log::SharedEdit::open(&path, Some(&jordan_file)).unwrap();
    let tool = e.actor();
    let mut b = e.budget();
    b.title = "From Jordan's command line".into();
    e.record(&b, "Command line: set title", "2026-10-18T14:00:00Z")
        .unwrap();
    drop(e);

    // On Jordan's computer: Jordan's own program.
    jordan
        .shared_mut()
        .unwrap()
        .set_tools_root(method_format::log::tools_root(&jordan_file));
    let out = jordan.share_tick(later()).unwrap();
    assert_eq!(jordan.budget().title, "From Jordan's command line");
    assert_eq!(out.remote[0].name, "Jordan Lee (command line or agent)");
    let st = jordan.shared().unwrap().status(Instant::now());
    assert!(
        st.people.iter().all(|p| p.actor != tool.hex()),
        "{:?}",
        st.people
    );

    // On Alex's computer: a program, whose isn't known yet.
    let out = alex.share_tick(later()).unwrap();
    assert_eq!(
        out.remote[0].name,
        "A command line or agent (whose isn\u{2019}t known yet)"
    );
    let st = alex.shared().unwrap().status(Instant::now());
    assert_eq!(st.people.len(), 1, "{:?}", st.people);
    assert_eq!(st.people[0].actor, JORDAN.hex());
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

// ---------------------------------------------------------------------------
// Signed changes and removing a person (decisions.md, entry 89)

fn key(seed: u8) -> std::sync::Arc<dyn method_core::log::Signer> {
    std::sync::Arc::new(method_format::log::sign::KeyPair::from_seed([seed; 32]))
}

/// Two sessions with keys, Alex having made Jordan an editor (the first
/// role, which starts signing), everything exchanged.
fn signed_sessions() -> (PathBuf, Session, Session) {
    let (path, mut alex, mut jordan) = two_sessions();
    alex.shared_mut().unwrap().use_key(key(1)).unwrap();
    jordan.shared_mut().unwrap().use_key(key(2)).unwrap();
    let st = alex.shared().unwrap().status(Instant::now());
    assert!(!st.signed, "no signing before the first role");
    alex.shared_mut()
        .unwrap()
        .set_role(&JORDAN.hex(), "editor", &[])
        .unwrap();
    exchange(&mut alex, &mut jordan);
    exchange(&mut alex, &mut jordan);
    (path, alex, jordan)
}

#[test]
fn changes_are_signed_from_the_first_role_and_the_lists_say_so() {
    let (_path, mut alex, mut jordan) = signed_sessions();
    let id = ids_in(&alex, "50-01")[0];
    jordan.set_detail_field(id, "rate", "4321").unwrap();
    exchange(&mut alex, &mut jordan);
    assert_eq!(rate_of(&alex, id), "4321");
    let st = alex.shared().unwrap().status(Instant::now());
    assert!(st.signed);
    assert!(
        st.signed_text.starts_with("Changes are signed"),
        "{}",
        st.signed_text
    );
    assert_eq!(st.refused, 0, "{:?}", st.refused_changes);
    assert!(st.me.signs);
    let j = st.people.iter().find(|p| p.actor == JORDAN.hex()).unwrap();
    assert!(j.signs, "Jordan's key arrived");
    // The Activity list marks signed changes: Jordan's edit is.
    let activity = alex.shared().unwrap().activity(10);
    let edit = activity
        .iter()
        .find(|a| a.label.starts_with("Edit rate on 50-01"))
        .unwrap();
    assert!(edit.signed);
    assert_eq!(edit.name, "Jordan Lee");
    // The first change, made before signing, isn't.
    let all = alex.shared().unwrap().activity(500);
    assert!(!all.last().unwrap().signed);
    // The file says so: format 3.
    let dir = alex.shared().unwrap().folder().to_path_buf();
    let info = std::fs::read_to_string(dir.join("budget.json")).unwrap();
    assert!(info.contains("\"version\": 3"), "{info}");
}

#[test]
fn a_change_altered_in_the_folder_is_left_out_with_why() {
    let (_path, mut alex, mut jordan) = signed_sessions();
    jordan.shared_mut().unwrap().flush_all();
    // Jordan's next change, altered in the folder before Alex reads it.
    let id = ids_in(&jordan, "50-01")[0];
    jordan.set_detail_field(id, "rate", "555").unwrap();
    jordan.share_tick(later());
    let dir = method_format::log::LogDir::at(jordan.shared().unwrap().folder());
    let seg = dir.segments(JORDAN).last().unwrap().path.clone();
    let text = std::fs::read_to_string(&seg).unwrap();
    let line = text.lines().nth(1).unwrap().replace("\"555\"", "\"5\"");
    let mut changes: Vec<method_core::log::Change> =
        vec![method_format::log::change_from_json(&line).unwrap()];
    changes.truncate(1);
    std::fs::write(&seg, method_format::log::segment_bytes(&changes).unwrap()).unwrap();
    alex.share_tick(later());
    let st = alex.shared().unwrap().status(Instant::now());
    assert_eq!(st.refused, 1, "{:?}", st.refused_changes);
    let r = &st.refused_changes[0];
    assert_eq!(r.name, "Jordan Lee");
    assert!(
        r.why.starts_with("Its signature doesn\u{2019}t match"),
        "{}",
        r.why
    );
    assert_ne!(rate_of(&alex, id), "5");
}

#[test]
fn removing_a_person_leaves_out_their_later_changes() {
    let (_path, mut alex, mut jordan) = signed_sessions();
    let id = ids_in(&alex, "50-01")[0];
    // Seen by Alex before the removal: kept.
    jordan.set_detail_field(id, "rate", "100").unwrap();
    exchange(&mut alex, &mut jordan);
    // Made while Alex removes Jordan, unseen: left out everywhere.
    jordan.set_detail_field(id, "rate", "200").unwrap();
    jordan.shared_mut().unwrap().flush_all();
    let err = jordan
        .shared_mut()
        .unwrap()
        .remove_person(&ALEX.hex())
        .unwrap_err();
    assert!(err.contains("owner"), "{err}");
    alex.shared_mut()
        .unwrap()
        .remove_person(&JORDAN.hex())
        .unwrap();
    exchange(&mut alex, &mut jordan);
    exchange(&mut alex, &mut jordan);
    for s in [&alex, &jordan] {
        assert_eq!(rate_of(s, id), "100");
    }
    let st = alex.shared().unwrap().status(Instant::now());
    let j = st.people.iter().find(|p| p.actor == JORDAN.hex()).unwrap();
    assert!(j.removed);
    assert!(j.role_text.starts_with("Removed"), "{}", j.role_text);
    let r = st
        .refused_changes
        .iter()
        .find(|r| r.name == "Jordan Lee")
        .unwrap();
    assert_eq!(r.why, "It was made after its author was removed.");
    // Jordan's computer says so, and refuses edits before they are made.
    let err = jordan.set_detail_field(id, "rate", "300").unwrap_err();
    assert!(err.starts_with("You were removed"), "{err}");
    // Given a role again, Jordan is back from then on.
    alex.shared_mut()
        .unwrap()
        .set_role(&JORDAN.hex(), "editor", &[])
        .unwrap();
    exchange(&mut alex, &mut jordan);
    jordan.set_detail_field(id, "rate", "400").unwrap();
    exchange(&mut alex, &mut jordan);
    assert_eq!(rate_of(&alex, id), "400");
}

/// Writes a change into the folder, in `who`'s name, publishing a key that
/// isn't theirs: what someone with access to the folder could do.
fn plant_key(s: &Session, who: ActorId) {
    use method_core::log::{Op, Signer};
    let sh = s.shared().unwrap();
    let theirs = method_format::log::sign::KeyPair::from_seed([77; 32]);
    let mut c = method_core::log::Change {
        actor: who,
        seq: sh.log().frontier().get(&who) + 1,
        deps: sh.log().frontier().clone(),
        lamport: sh.log().lamport() + 1,
        time: "2026-10-20T10:00:00Z".into(),
        label: "Added this computer\u{2019}s key".into(),
        ops: vec![Op::Key {
            key: theirs.public(),
        }],
        sig: None,
    };
    c.sig = Some(theirs.sign(&c));
    method_format::log::LogDir::at(sh.folder())
        .write_segment(&[c])
        .unwrap();
}

/// A key put in a member's name by someone else (MReview, pull request #73):
/// the member's app says so plainly and changes nothing.
#[test]
fn a_key_in_my_name_that_isnt_mine_is_said_and_stops_my_edits() {
    let (_path, mut alex, mut jordan) = two_sessions();
    alex.shared_mut().unwrap().use_key(key(1)).unwrap();
    alex.shared_mut()
        .unwrap()
        .set_role(&JORDAN.hex(), "editor", &[])
        .unwrap();
    alex.share_tick(later());
    // Before Jordan's app puts its key in, someone else puts one in for Jordan.
    jordan.share_tick(later());
    plant_key(&jordan, JORDAN);
    jordan.share_tick(later());
    jordan.shared_mut().unwrap().use_key(key(2)).unwrap();
    jordan.share_tick(later());
    let st = jordan.shared().unwrap().status(Instant::now());
    let w = st.key_warning.clone().expect("a warning");
    assert!(
        w.starts_with("A key in your name is in this shared budget"),
        "{w}"
    );
    assert!(!w.contains('\u{2014}') && !w.contains('!'), "{w}");
    let id = ids_in(&jordan, "50-01")[0];
    let before = rate_of(&jordan, id);
    let err = jordan.set_detail_field(id, "rate", "999").unwrap_err();
    assert!(err.starts_with("A key in your name"), "{err}");
    assert_eq!(rate_of(&jordan, id), before);
    // The owner sees nothing wrong with their own key.
    alex.share_tick(later());
    assert!(alex
        .shared()
        .unwrap()
        .status(Instant::now())
        .key_warning
        .is_none());
}

/// The owner's case on a budget whose roles were set before signing (format
/// 2): a key put in the owner's name before the owner's app opens it in this
/// version is found, said, and the owner's app signs nothing as that actor.
/// Who then counts as the owner is the owner's decision (roadmap, waiting on
/// the owner); this only detects and refuses.
#[test]
fn a_key_in_the_owners_name_on_a_roles_budget_is_found_and_refused() {
    let (_path, mut alex, mut jordan) = two_sessions();
    // Roles set without signing (an older version's budget).
    alex.shared_mut()
        .unwrap()
        .set_role(&JORDAN.hex(), "editor", &[])
        .unwrap();
    exchange(&mut alex, &mut jordan);
    assert!(!alex.shared().unwrap().status(Instant::now()).signed);
    // Someone puts a key in the owner's name into the folder.
    plant_key(&jordan, ALEX);
    alex.share_tick(later());
    // The owner opens it in this version: its key isn't the one in the log.
    alex.shared_mut().unwrap().use_key(key(1)).unwrap();
    let st = alex.shared().unwrap().status(Instant::now());
    let w = st.key_warning.expect("a warning");
    assert!(w.contains("the people you share this budget with"), "{w}");
    let err = alex
        .shared_mut()
        .unwrap()
        .set_role(&JORDAN.hex(), "read_only", &[])
        .unwrap_err();
    assert!(err.contains("key"), "{err}");
}

// ---------------------------------------------------------------------------
// The owner's key (docs/research/owner-key.md)

/// A key put in Alex's name before Alex's app did: Alex's status offers Start
/// Again, which makes a new shared budget signed with Alex's own key; Jordan's
/// app says the owner started again and refuses edits until Jordan reopens,
/// and after that the two share as before.
#[test]
fn the_owner_starts_again_and_a_member_reopens_to_join() {
    use method_core::log::Signer;

    let (path, mut alex, mut jordan) = two_sessions();
    let dir = path.parent().unwrap().to_path_buf();
    alex.shared_mut()
        .unwrap()
        .set_owner_keys_dir(Some(dir.join("keys-alex")));
    jordan
        .shared_mut()
        .unwrap()
        .set_owner_keys_dir(Some(dir.join("keys-jordan")));
    alex.shared_mut().unwrap().use_key(key(1)).unwrap();
    jordan.shared_mut().unwrap().use_key(key(2)).unwrap();

    // Someone with the folder puts a key in Alex's name.
    let theirs = method_format::log::sign::KeyPair::from_seed([77; 32]);
    let log = alex.shared().unwrap().log();
    let mut planted = method_core::log::Change {
        actor: ALEX,
        seq: log.own_seq() + 1,
        deps: log.frontier().clone(),
        lamport: log.lamport() + 1,
        time: "2026-10-20T12:00:00Z".into(),
        label: "Added this computer's key".into(),
        ops: vec![Op::Key {
            key: theirs.public(),
        }],
        sig: None,
    };
    planted.sig = Some(theirs.sign(&planted));
    method_format::log::LogDir::for_budget(&path)
        .write_segment(&[planted])
        .unwrap();
    exchange(&mut alex, &mut jordan);

    let st = alex.shared().unwrap().status(Instant::now());
    assert!(st.key_warning.is_some());
    assert!(st.can_start_again);
    let st = jordan.shared().unwrap().status(Instant::now());
    assert!(!st.can_start_again, "only on the owner's computer");
    assert!(jordan.shared_mut().unwrap().start_again().is_err());

    alex.shared_mut().unwrap().start_again().unwrap();
    let st = alex.shared().unwrap().status(Instant::now());
    assert!(st.key_warning.is_none(), "{:?}", st.key_warning);
    assert!(!st.can_start_again);
    assert!(st.signed);
    assert!(st.replaced.is_none());

    // Jordan's app notices and refuses edits until Jordan reopens.
    exchange(&mut alex, &mut jordan);
    let st = jordan.shared().unwrap().status(Instant::now());
    assert!(st.replaced.is_some(), "{st:?}");
    let id = ids_in(&jordan, "50-01")[0];
    assert!(jordan.set_detail_field(id, "rate", "1234").is_err());

    // Jordan's computer had seen the planted key as Alex's, so the new
    // budget's key (Alex's real one) is a changed key there: Jordan asks
    // Alex, then trusts it (MReview on pull request #73).
    jordan.shared_mut().unwrap().reopen().unwrap();
    let st = jordan.shared().unwrap().status(Instant::now());
    assert!(st.replaced.is_none());
    assert!(st.owner_key_warning.is_some(), "{st:?}");
    assert!(jordan.set_detail_field(id, "rate", "1234").is_err());
    let merged = jordan.shared_mut().unwrap().trust_owner_key().unwrap();
    assert_eq!(&merged, alex.budget());
    let st = jordan.shared().unwrap().status(Instant::now());
    assert!(st.owner_key_warning.is_none());
    jordan.set_detail_field(id, "rate", "1234").unwrap();
    exchange(&mut alex, &mut jordan);
    exchange(&mut alex, &mut jordan);
    assert_eq!(rate_of(&alex, id), "1234");
    let st = alex.shared().unwrap().status(Instant::now());
    assert_eq!(st.refused, 0, "{:?}", st.refused_changes);
}

// ---------------------------------------------------------------------------
// Muted cells and access requests (docs/research/collaboration.md, 9.7)

/// A timer pass `secs` from now, so heartbeats are read again (they are
/// read every few seconds, not every pass).
fn tick_at(s: &mut Session, secs: u64) {
    s.share_tick(Instant::now() + Duration::from_secs(secs));
}

/// An account in a category other than `category`.
fn account_outside(s: &Session, category: &str) -> String {
    s.budget()
        .categories()
        .filter(|c| c.code != category)
        .flat_map(|c| &c.accounts)
        .find(|a| {
            a.details
                .iter()
                .any(|d| d.kind == method_core::RowKind::Line)
        })
        .unwrap()
        .code
        .clone()
}

fn first_line(s: &Session, account: &str) -> RowId {
    s.budget()
        .categories()
        .flat_map(|c| &c.accounts)
        .find(|a| a.code == account)
        .unwrap()
        .details
        .iter()
        .find(|d| d.kind == method_core::RowKind::Line)
        .unwrap()
        .id
}

const KIM: ActorId = ActorId([3; 32]);

#[test]
fn a_department_editors_request_reaches_who_can_allow_it() {
    let (path, mut alex, mut jordan) = two_sessions();
    let mut kim = Session::open(&path).unwrap();
    kim.join_shared(KIM, person("Kim Park", "green")).unwrap();
    let cat = category_of(&alex, "50-01");
    let other = account_outside(&alex, &cat);
    let other_cat = category_of(&alex, &other);
    alex.shared_mut()
        .unwrap()
        .set_role(&JORDAN.hex(), "department", std::slice::from_ref(&cat))
        .unwrap();
    alex.shared_mut()
        .unwrap()
        .set_role(&KIM.hex(), "editor", &[])
        .unwrap();
    exchange(&mut alex, &mut jordan);
    exchange(&mut alex, &mut kim);

    // What Jordan's page mutes: every category but Jordan's, and every
    // account outside it.
    let st = jordan.shared().unwrap().status(Instant::now());
    assert!(st.can_ask);
    assert_eq!(st.ask_name, "Alex Kim");
    assert!(!st.locked_categories.contains(&cat));
    assert!(st.locked_categories.contains(&other_cat));
    assert!(!st.locked_accounts.contains(&"50-01".to_string()));
    assert!(st.locked_accounts.contains(&other));
    let all_cats: Vec<String> = alex.budget().categories().map(|c| c.code.clone()).collect();
    assert_eq!(st.locked_categories.len(), all_cats.len() - 1);
    // Nobody else's page mutes anything, and only Jordan may ask.
    for s in [&alex, &kim] {
        let st = s.shared().unwrap().status(Instant::now());
        assert!(st.locked_categories.is_empty() && st.locked_accounts.is_empty());
        assert!(!st.can_ask);
    }

    // What is asked is checked before it is written.
    let sh = jordan.shared_mut().unwrap();
    assert!(sh
        .request_access("editor", &["no-such".into()], "")
        .unwrap_err()
        .contains("no-such"));
    assert!(sh.request_access("department", &[], "").is_err());
    assert!(sh.request_access("admin", &[], "").is_err());
    assert_eq!(
        sh.request_access("department", std::slice::from_ref(&cat), ""),
        Err("You can already change that.".to_string())
    );
    assert!(kim
        .shared_mut()
        .unwrap()
        .request_access("editor", &[], "")
        .is_err());

    // Jordan asks about the other account, from the cell they typed in:
    // to edit that account, not everything (MReview on pull request #87).
    let before = alex.shared().unwrap().log().len();
    jordan
        .shared_mut()
        .unwrap()
        .request_access(
            "department",
            std::slice::from_ref(&other),
            "  For the grip\nrentals ",
        )
        .unwrap();
    let mine = jordan
        .shared()
        .unwrap()
        .status(Instant::now())
        .my_request
        .unwrap();
    assert_eq!(mine.note, "For the grip rentals");
    // Written into Jordan's heartbeat, not the change log.
    let dir = method_format::log::LogDir::for_budget(&path);
    let req = dir.read_presence(JORDAN).unwrap().request.unwrap();
    assert_eq!(req.role, "department");
    assert_eq!(req.codes, vec![other.clone()]);
    tick_at(&mut alex, 60);
    tick_at(&mut kim, 60);
    assert_eq!(alex.shared().unwrap().log().len(), before);

    // The owner sees it; another editor, who can't give roles, doesn't.
    let st = alex.shared().unwrap().status(Instant::now());
    assert_eq!(st.requests.len(), 1, "{:?}", st.requests);
    let r = &st.requests[0];
    assert_eq!(r.name, "Jordan Lee");
    assert!(r.role_text.starts_with("Edits "), "{}", r.role_text);
    assert_ne!(r.role_text, "Edits everything");
    assert!(!r.only, "Allow already gives just what was asked");
    assert!(r.codes_text.starts_with(&other), "{}", r.codes_text);
    assert_eq!(r.at, mine.at);
    assert!(kim
        .shared()
        .unwrap()
        .status(Instant::now())
        .requests
        .is_empty());

    // Allow adds that account to Jordan's categories: a role change through
    // the log, and the request is gone.
    alex.shared_mut()
        .unwrap()
        .allow_request(&JORDAN.hex(), &r.at, false)
        .unwrap();
    assert_eq!(alex.shared().unwrap().log().len(), before + 1);
    let st = alex.shared().unwrap().status(Instant::now());
    assert!(st.requests.is_empty());
    let j = st.people.iter().find(|p| p.actor == JORDAN.hex()).unwrap();
    assert_eq!(j.role, "department");
    assert!(
        j.scope.contains(&cat) && j.scope.contains(&other),
        "{:?}",
        j.scope
    );
    exchange(&mut alex, &mut jordan);
    let line = first_line(&jordan, &other);
    jordan.set_detail_field(line, "rate", "55").unwrap();
    // Jordan's app takes the request back once it is given.
    tick_at(&mut jordan, 60);
    let st = jordan.shared().unwrap().status(Instant::now());
    assert!(st.my_request.is_none());
    assert!(!st.locked_accounts.contains(&other));
    assert!(dir.read_presence(JORDAN).unwrap().request.is_none());
    exchange(&mut alex, &mut jordan);
    assert_eq!(rate_of(&alex, line), "55");
}

#[test]
fn a_read_only_persons_request_is_declined_on_one_computer_and_allowed() {
    let (path, mut alex, mut jordan) = two_sessions();
    let dir = path.parent().unwrap().to_path_buf();
    alex.shared_mut()
        .unwrap()
        .set_declined_dir(Some(dir.join("declined-alex")));
    alex.shared_mut()
        .unwrap()
        .set_role(&JORDAN.hex(), "read_only", &[])
        .unwrap();
    exchange(&mut alex, &mut jordan);
    // Read only keeps its own state (no muted cells, edits refused) and may ask.
    let st = jordan.shared().unwrap().status(Instant::now());
    assert!(st.can_ask);
    assert!(st.locked_accounts.is_empty());
    assert!(jordan.check_writable().is_err());
    jordan
        .shared_mut()
        .unwrap()
        .request_access("editor", &[], "")
        .unwrap();
    tick_at(&mut alex, 60);
    let st = alex.shared().unwrap().status(Instant::now());
    assert_eq!(st.requests.len(), 1);
    let at = st.requests[0].at.clone();
    assert!(
        !st.requests[0].only,
        "no categories named, no second choice"
    );
    let refused = alex
        .shared_mut()
        .unwrap()
        .allow_request(&JORDAN.hex(), &at, true)
        .unwrap_err();
    assert!(refused.contains("doesn"), "{refused}");

    // Declined: hidden on Alex's computer, and nothing in the log.
    let before = alex.shared().unwrap().log().len();
    alex.shared_mut()
        .unwrap()
        .decline_request(&JORDAN.hex(), &at)
        .unwrap();
    assert!(alex
        .shared()
        .unwrap()
        .status(Instant::now())
        .requests
        .is_empty());
    assert_eq!(alex.shared().unwrap().log().len(), before);
    assert!(jordan
        .shared()
        .unwrap()
        .status(Instant::now())
        .my_request
        .is_some());
    // Remembered on that computer: the budget opened again there still
    // hides it.
    alex.share_flush();
    let mut again = Session::open(&path).unwrap();
    again.join_shared(ALEX, person("Alex Kim", "teal")).unwrap();
    again
        .shared_mut()
        .unwrap()
        .set_declined_dir(Some(dir.join("declined-alex")));
    tick_at(&mut again, 60);
    assert!(again
        .shared()
        .unwrap()
        .status(Instant::now())
        .requests
        .is_empty());
    // Another computer of the owner's that didn't decline it still shows it,
    // and allowing it there makes Jordan an editor.
    let mut other = Session::open(&path).unwrap();
    other.join_shared(ALEX, person("Alex Kim", "teal")).unwrap();
    tick_at(&mut other, 60);
    let st = other.shared().unwrap().status(Instant::now());
    assert_eq!(st.requests.len(), 1);
    other
        .shared_mut()
        .unwrap()
        .allow_request(&JORDAN.hex(), &at, false)
        .unwrap();
    other.share_flush();
    tick_at(&mut jordan, 60);
    assert!(jordan.check_writable().is_ok());
    assert_eq!(
        jordan.shared().unwrap().status(Instant::now()).me.role,
        "editor"
    );
    // Given: Jordan's app takes the request back.
    tick_at(&mut jordan, 120);
    assert!(jordan
        .shared()
        .unwrap()
        .status(Instant::now())
        .my_request
        .is_none());
}

#[test]
fn a_removed_person_asks_and_is_let_back_in() {
    let (_path, mut alex, mut jordan) = signed_sessions();
    alex.shared_mut()
        .unwrap()
        .remove_person(&JORDAN.hex())
        .unwrap();
    exchange(&mut alex, &mut jordan);
    let st = jordan.shared().unwrap().status(Instant::now());
    assert!(st.can_ask);
    jordan
        .shared_mut()
        .unwrap()
        .request_access("editor", &[], "Back on the show")
        .unwrap();
    tick_at(&mut alex, 60);
    let st = alex.shared().unwrap().status(Instant::now());
    assert_eq!(st.requests.len(), 1);
    assert_eq!(st.requests[0].note, "Back on the show");
    // Withdrawn: gone from the owner's list.
    jordan.shared_mut().unwrap().withdraw_request();
    tick_at(&mut alex, 120);
    assert!(alex
        .shared()
        .unwrap()
        .status(Instant::now())
        .requests
        .is_empty());
    jordan
        .shared_mut()
        .unwrap()
        .request_access("editor", &[], "")
        .unwrap();
    tick_at(&mut alex, 180);
    let at = alex.shared().unwrap().status(Instant::now()).requests[0]
        .at
        .clone();
    alex.shared_mut()
        .unwrap()
        .allow_request(&JORDAN.hex(), &at, false)
        .unwrap();
    exchange(&mut alex, &mut jordan);
    let st = jordan.shared().unwrap().status(Instant::now());
    assert_eq!(st.me.role, "editor");
    assert!(!st.me.removed);
}

// ---------------------------------------------------------------------------
// Invite links (decisions.md, entry 97)

const RILEY: ActorId = ActorId([6; 32]);
const MORGAN: ActorId = ActorId([7; 32]);

/// Every file under `dir`, read as text (the shared folder's contents).
fn folder_text(dir: &std::path::Path) -> String {
    let mut out = String::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push_str(&String::from_utf8_lossy(&std::fs::read(&p).unwrap()));
            }
        }
    }
    out
}

/// A session that opens the shared budget at `path` as `who`, with key `k`.
fn newcomer(path: &std::path::Path, who: ActorId, name: &str, k: u8) -> Session {
    let mut s = Session::open(path).unwrap();
    s.join_shared(who, person(name, "green")).unwrap();
    s.shared_mut().unwrap().use_key(key(k)).unwrap();
    s
}

#[test]
fn an_invite_link_lets_someone_new_join_with_its_role_once() {
    use method_app::invite_link::{base64url, InviteLink};

    let (path, mut alex, mut jordan) = signed_sessions();
    let cat = category_of(&alex, "50-01");
    // Jordan edits everything but doesn't give roles, so can't invite.
    let st = jordan.shared().unwrap().status(Instant::now());
    assert!(!st.can_invite && st.invites.is_empty());
    assert!(jordan
        .shared_mut()
        .unwrap()
        .create_invite("editor", &[], 7)
        .is_err());
    // Alex invites someone to edit one category, for a week.
    let sh = alex.shared_mut().unwrap();
    assert!(
        sh.create_invite("department", &[], 7).is_err(),
        "no category"
    );
    assert!(
        sh.create_invite("editor", &[], 3).is_err(),
        "1, 7 or 30 days"
    );
    let made = sh
        .create_invite("department", std::slice::from_ref(&cat), 7)
        .unwrap();
    assert!(
        made.link.starts_with("methodbudget:join?v=1&b="),
        "{}",
        made.link
    );
    assert!(made.role_text.starts_with("Edits "), "{}", made.role_text);
    let link = InviteLink::parse(&made.link).unwrap();
    assert_eq!(link.file, "Feature.mthd");
    let st = alex.shared().unwrap().status(Instant::now());
    assert!(st.can_invite);
    assert_eq!(st.invites.len(), 1);
    assert_eq!(st.invites[0].state, "open");
    assert_eq!(st.invites[0].by, "Alex Kim");
    assert!(st.invites[0].can_cancel);
    exchange(&mut alex, &mut jordan);
    // The secret is in the link only: never in the shared folder.
    let folder = alex.shared().unwrap().folder().to_path_buf();
    let text = folder_text(&folder);
    assert!(text.contains(r#""op":"invite""#));
    assert!(!text.contains(&base64url(&link.secret)));
    assert!(!text.contains(&method_core::log::ids::hex(&link.secret)));
    let info = std::fs::read_to_string(folder.join("budget.json")).unwrap();
    assert!(info.contains("\"version\": 4"), "{info}");

    // Riley opens the budget in their copy of the folder and joins from the
    // link: the role it gives, nothing more.
    let mut riley = newcomer(&path, RILEY, "Riley", 6);
    assert!(riley.shared().unwrap().role().reads_only());
    let role = riley.shared_mut().unwrap().join_invite(&link).unwrap();
    assert_eq!(role, made.role_text);
    let line = first_line(&riley, "50-01");
    riley.set_detail_field(line, "rate", "777").unwrap();
    let other = account_outside(&riley, &cat);
    let outside = first_line(&riley, &other);
    let e = riley
        .set_detail_field(outside, "rate", "1")
        .unwrap_err()
        .to_string();
    assert!(e.contains("lines and notes in"), "{e}");
    tick_at(&mut riley, 3);
    // Heartbeats are read every few seconds.
    tick_at(&mut alex, 30);
    assert_eq!(rate_of(&alex, line), "777");
    let st = alex.shared().unwrap().status(Instant::now());
    assert_eq!(st.refused, 0, "{:?}", st.refused_changes);
    let r = st.people.iter().find(|p| p.actor == RILEY.hex()).unwrap();
    assert_eq!(r.role, "department");
    assert_eq!(r.name, "Riley");
    assert_eq!(st.invites.len(), 1);
    assert_eq!(st.invites[0].state, "used");
    assert_eq!(st.invites[0].joined.as_deref(), Some("Riley"));
    assert!(!st.invites[0].can_cancel);
    // Alex's app sealed the used invite as soon as Riley's claim arrived:
    // one cancel, written to the folder, and none again on the next pass.
    let seals = |s: &Session| {
        s.shared()
            .unwrap()
            .log()
            .changes()
            .flat_map(|c| &c.ops)
            .filter(|op| matches!(op, Op::CancelInvite { .. }))
            .count()
    };
    assert_eq!(seals(&alex), 1);
    tick_at(&mut alex, 40);
    assert_eq!(seals(&alex), 1);
    // Cancelling it by hand is allowed and changes nothing for Riley.
    let used = st.invites[0].id.clone();
    alex.shared_mut().unwrap().cancel_invite(&used).unwrap();
    assert_eq!(seals(&alex), 1, "already sealed");
    // Riley already has a role: the link is no use to them again.
    let e = riley.shared_mut().unwrap().join_invite(&link).unwrap_err();
    assert!(e.contains("already have a role"), "{e}");

    // Used once: Morgan, with the same link, is refused.
    let mut morgan = newcomer(&path, MORGAN, "Morgan", 7);
    let e = morgan.shared_mut().unwrap().join_invite(&link).unwrap_err();
    assert!(e.contains("already used"), "{e}");
    assert!(morgan.shared().unwrap().role().reads_only());

    // Cancelled: a second invite, cancelled before Morgan uses it.
    let second = alex
        .shared_mut()
        .unwrap()
        .create_invite("comment", &[], 1)
        .unwrap();
    let second_link = InviteLink::parse(&second.link).unwrap();
    let st = alex.shared().unwrap().status(Instant::now());
    let open = st.invites.iter().find(|i| i.state == "open").unwrap();
    assert_eq!(open.role, "comment");
    alex.shared_mut().unwrap().cancel_invite(&open.id).unwrap();
    let st = alex.shared().unwrap().status(Instant::now());
    assert!(
        st.invites.iter().all(|i| i.state == "used"),
        "a cancelled one isn't listed"
    );
    tick_at(&mut alex, 3);
    tick_at(&mut morgan, 6);
    let e = morgan
        .shared_mut()
        .unwrap()
        .join_invite(&second_link)
        .unwrap_err();
    assert!(e.contains("cancelled"), "{e}");

    // Not here yet: an invite this computer hasn't seen says to try again.
    let third = alex
        .shared_mut()
        .unwrap()
        .create_invite("comment", &[], 30)
        .unwrap();
    let third_link = InviteLink::parse(&third.link).unwrap();
    let e = morgan
        .shared_mut()
        .unwrap()
        .join_invite(&third_link)
        .unwrap_err();
    assert!(e.contains("try again"), "{e}");
    tick_at(&mut alex, 3);
    tick_at(&mut morgan, 6);
    let role = morgan
        .shared_mut()
        .unwrap()
        .join_invite(&third_link)
        .unwrap();
    assert_eq!(role, "Notes only");

    // A link for another shared budget is refused in words.
    let mut wrong = third_link.clone();
    wrong.budget = "0".repeat(64);
    let e = riley.shared_mut().unwrap().join_invite(&wrong).unwrap_err();
    assert!(e.contains("different shared budget"), "{e}");
    assert!(!e.contains('\u{2014}') && !e.contains('!'), "{e}");
}
