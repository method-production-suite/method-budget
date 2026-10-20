//! `method-budget log` and `method-budget sync` end to end (docs/CLI.md).

use std::path::PathBuf;
use std::process::{Command, Output};

fn method_as(actor_file: &str, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_method-budget"))
        .env("METHOD_ACTOR_FILE", actor_file)
        .args(args)
        .output()
        .expect("method-budget runs")
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn code(o: &Output) -> i32 {
    o.status.code().expect("exited")
}

/// A fresh folder for one test, with a starter budget as `Feature.mthd`
/// and two actor files.
struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Dir {
        let p = std::env::temp_dir().join(format!("method-log-it-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        let d = Dir(p);
        assert_eq!(
            code(&method_as(&d.actor("a"), &["new", &d.path("Feature.mthd")])),
            0
        );
        d
    }

    fn path(&self, name: &str) -> String {
        self.0.join(name).to_string_lossy().into_owned()
    }

    fn actor(&self, who: &str) -> String {
        self.path(&format!("actor-{who}"))
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn log_init_show_and_sync() {
    let d = Dir::new("sync");
    let file = d.path("Feature.mthd");
    let alex = d.actor("alex");
    let jordan = d.actor("jordan");

    // No log yet.
    let o = method_as(&alex, &["log", &file]);
    assert_eq!(code(&o), 1);
    assert!(err(&o).contains("--init"), "{}", err(&o));

    // Alex starts it; the actor file is made on first use.
    let o = method_as(&alex, &["log", &file, "--init"]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(out(&o).contains("Started the change log"), "{}", out(&o));
    assert_eq!(std::fs::read_to_string(&alex).unwrap().trim().len(), 64);
    assert!(d.0.join("Feature.mthd-log").join("budget.json").is_file());
    let o = method_as(&alex, &["log", &file, "--init"]);
    assert_eq!(code(&o), 1);

    // Showing it.
    let o = method_as(&alex, &["log", &file]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    let text = out(&o);
    assert!(text.contains("1 changes by 1 actors"), "{text}");
    assert!(text.contains("(this computer)"), "{text}");
    assert!(text.contains("Adopted Feature.mthd"), "{text}");
    let o = method_as(&alex, &["log", &file, "--json", "--last", "1"]);
    assert_eq!(code(&o), 0);
    let j: method_format::serde_json::Value =
        method_format::serde_json::from_str(&out(&o)).unwrap();
    assert_eq!(j["changes"], 1);
    assert_eq!(j["file"], "Feature.mthd");
    assert_eq!(j["last"].as_array().unwrap().len(), 1);

    // Jordan changes the shared budget with `method-budget set`: the edit is
    // recorded as Jordan's change in the log; the file is Alex's to write.
    let o = method_as(&jordan, &["set", &file, "title", "Shared Feature"]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(out(&o).contains("recorded as one change"), "{}", out(&o));
    let o = method_as(&jordan, &["sync", &file]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    let text = out(&o);
    assert!(!text.contains("recorded an outside edit"), "{text}");
    assert!(text.contains("wrote the merged budget"), "{text}");
    let o = method_as(&alex, &["log", &file]);
    let text = out(&o);
    assert!(text.contains("2 changes by 2 actors"), "{text}");
    assert!(text.contains("Command line: set title"), "{text}");

    // A program that doesn't know the log edits the file: sync takes the
    // edit in as this computer's change.
    let mut b = method_format::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    b.title = "Edited by hand".into();
    std::fs::write(&file, method_format::to_string(&b)).unwrap();
    let o = method_as(&jordan, &["sync", &file]);
    assert!(out(&o).contains("recorded an outside edit"), "{}", out(&o));
    let o = method_as(&jordan, &["set", &file, "title", "Shared Feature"]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    let o = method_as(&alex, &["sync", &file]);
    assert_eq!(code(&o), 0, "{}", err(&o));

    // Syncing the folder finds the budget; nothing new to record.
    let o = method_as(&alex, &["sync", &d.path(""), "--no-write"]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(!out(&o).contains("recorded an outside edit"), "{}", out(&o));
    let budget = method_format::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(budget.title, "Shared Feature");

    // Rolling the segments of Alex's command line after two hand edits it
    // took in (`set` and `sync` write as a program's own actor, never as
    // the person the app writes as).
    for title in ["Shared Feature 2", "Shared Feature 3"] {
        let mut b = method_format::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        b.title = title.into();
        std::fs::write(&file, method_format::to_string(&b)).unwrap();
        let o = method_as(&alex, &["sync", &file]);
        assert!(out(&o).contains("recorded an outside edit"), "{}", out(&o));
    }
    let o = method_as(&alex, &["sync", &file, "--roll"]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(out(&o).contains("rolled"), "{}", out(&o));
    // Alex, Jordan's command line and Alex's.
    let o = method_as(&jordan, &["log", &file]);
    assert!(out(&o).contains("6 changes by 3 actors"), "{}", out(&o));

    // A folder with no shared budget.
    let empty = d.path("empty");
    std::fs::create_dir_all(&empty).unwrap();
    let o = method_as(&alex, &["sync", &empty]);
    assert_eq!(code(&o), 1);
    assert!(err(&o).contains("no shared budget"), "{}", err(&o));
}

/// Gives `who` (the actor in `who_file`) a role, as the owner in
/// `owner_file`, through the library the app uses.
fn grant(file: &str, owner_file: &str, who_file: &str, role: method_core::log::Role) {
    use method_format::log;
    let owner =
        method_core::log::ActorId::parse(&std::fs::read_to_string(owner_file).unwrap()).unwrap();
    let who =
        method_core::log::ActorId::parse(&std::fs::read_to_string(who_file).unwrap()).unwrap();
    let path = std::path::Path::new(file);
    let (mut shared, _) = log::open(path, owner, method_core::log::Ids::seeded(1)).unwrap();
    shared
        .log
        .grant(who, role, "role", "2026-10-18T00:00:00Z")
        .unwrap();
    log::flush(&shared.log, &shared.dir).unwrap();
}

/// `set`, `apply` and the commands that rewrite a whole file, on a budget
/// shared through a folder whose owner has set roles (decisions.md, entry
/// 78).
#[test]
fn command_line_writes_to_a_shared_budget_follow_roles() {
    let d = Dir::new("roles");
    let file = d.path("Feature.mthd");
    let alex = d.actor("alex");
    let jordan = d.actor("jordan");
    assert_eq!(code(&method_as(&alex, &["log", &file, "--init"])), 0);
    // Jordan joins (the actor file is made) and edits before any role.
    let o = method_as(&jordan, &["set", &file, "title", "Before roles"]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    grant(&file, &alex, &jordan, method_core::log::Role::ReadOnly);

    // Now Jordan reads only: set and apply are refused, nothing written.
    let o = method_as(&jordan, &["set", &file, "title", "Jordan's"]);
    assert_ne!(code(&o), 0);
    assert!(err(&o).contains("role"), "{}", err(&o));
    let script = d.path("edits.txt");
    std::fs::write(&script, "set title \"Jordan again\"\n").unwrap();
    let o = method_as(&jordan, &["apply", &file, &script]);
    assert_ne!(code(&o), 0);
    assert!(err(&o).contains("role"), "{}", err(&o));
    // A computer with no person in the budget is refused too.
    let o = method_as(&d.actor("nobody"), &["set", &file, "title", "Nobody's"]);
    assert_ne!(code(&o), 0);
    assert!(err(&o).contains("no person"), "{}", err(&o));
    assert!(!std::path::Path::new(&d.actor("nobody")).exists());
    // Commands that rewrite the whole file leave a shared budget alone.
    let o = method_as(&alex, &["fmt", &file]);
    assert_ne!(code(&o), 0);
    assert!(err(&o).contains("shared through a folder"), "{}", err(&o));

    // The owner's change goes through, and the owner keeps the file.
    let o = method_as(&alex, &["set", &file, "title", "Alex's"]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    let b = method_format::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(b.title, "Alex's");

    // A hand edit to the file after roles is kept aside, not taken in.
    let mut b = b;
    b.title = "Edited by hand".into();
    std::fs::write(&file, method_format::to_string(&b)).unwrap();
    let o = method_as(&alex, &["sync", &file]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(out(&o).contains("isn't taken in"), "{}", out(&o));
    let b = method_format::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(b.title, "Alex's");
    let kept = std::fs::read_dir(&d.0)
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().contains("(outside edit"))
        .count();
    assert_eq!(kept, 1);
}

/// Once signing has started, the command line writes as a program of this
/// computer's person and signs with the person's key, kept beside the actor
/// id; until the person's app has put that key in the budget it is refused
/// in words (decisions.md, entry 89).
#[test]
fn command_line_writes_are_signed_with_the_persons_key() {
    use method_core::log::{ActorId, Ids, Role};
    use method_format::log;

    let d = Dir::new("signed");
    let file = d.path("Feature.mthd");
    let alex = d.actor("alex");
    let jordan = d.actor("jordan");
    assert_eq!(code(&method_as(&alex, &["log", &file, "--init"])), 0);
    let o = method_as(&jordan, &["set", &file, "title", "Before roles"]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    let actor_of = |f: &str| ActorId::parse(&std::fs::read_to_string(f).unwrap()).unwrap();
    let key_of = |f: &str| {
        std::sync::Arc::new(
            log::sign::load_or_make_key(&log::sign::key_path(std::path::Path::new(f))).unwrap(),
        )
    };
    let path = std::path::Path::new(&file);
    // The owner's app sets the first role: signing starts.
    let (mut owner, _) = log::open(path, actor_of(&alex), Ids::seeded(1)).unwrap();
    owner.log.set_signer(key_of(&alex));
    owner
        .log
        .grant(
            actor_of(&jordan),
            Role::Editor,
            "Jordan edits",
            "2026-10-20T00:00:00Z",
        )
        .unwrap();
    log::flush(&owner.log, &owner.dir).unwrap();
    let o = method_as(&alex, &["log", &file]);
    assert!(out(&o).contains("Changes are signed"), "{}", out(&o));

    // Jordan's key isn't in the budget yet: refused, nothing written.
    let o = method_as(&jordan, &["set", &file, "title", "Too soon"]);
    assert_ne!(code(&o), 0);
    assert!(err(&o).contains("key isn't in it yet"), "{}", err(&o));

    // Jordan's app opens the budget and adds the key.
    let (mut app, _) = log::open(path, actor_of(&jordan), Ids::seeded(2)).unwrap();
    app.log.set_signer(key_of(&jordan));
    app.log
        .publish_key("Added this computer's key", "2026-10-20T00:00:00Z")
        .unwrap()
        .unwrap();
    log::flush(&app.log, &app.dir).unwrap();

    let o = method_as(
        &jordan,
        &["set", &file, "title", "Signed from the command line"],
    );
    assert_eq!(code(&o), 0, "{}", err(&o));
    let (reader, report) = log::open(path, ActorId::NONE, Ids::seeded(9)).unwrap();
    assert!(report.rejected.is_empty(), "{:?}", report.rejected);
    assert_eq!(
        reader.log.project().budget.title,
        "Signed from the command line"
    );
    let last = reader.log.changes().last().unwrap();
    assert!(last.sig.is_some());
    assert_ne!(last.actor, actor_of(&jordan), "a program's own actor");
    assert_eq!(reader.log.person_of(last.actor), actor_of(&jordan));
    assert_eq!(
        reader.log.key_of(last.actor),
        reader.log.key_of(actor_of(&jordan))
    );
    let o = method_as(&jordan, &["log", &file, "--json"]);
    assert!(out(&o).contains("\"signed\": true"), "{}", out(&o));
}
