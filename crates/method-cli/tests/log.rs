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

    // Jordan edits the file with `method-budget set` (an outside edit) and syncs:
    // the edit becomes Jordan's change, and the file is rewritten.
    let o = method_as(&jordan, &["set", &file, "title", "Shared Feature"]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    let o = method_as(&jordan, &["sync", &file]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    let text = out(&o);
    assert!(text.contains("recorded an outside edit"), "{text}");
    assert!(text.contains("wrote the merged budget"), "{text}");
    let o = method_as(&alex, &["log", &file]);
    let text = out(&o);
    assert!(text.contains("2 changes by 2 actors"), "{text}");
    assert!(text.contains("Outside edit to Feature.mthd"), "{text}");

    // Syncing the folder finds the budget; nothing new to record.
    let o = method_as(&alex, &["sync", &d.path(""), "--no-write"]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(!out(&o).contains("recorded an outside edit"), "{}", out(&o));
    let budget = method_format::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(budget.title, "Shared Feature");

    // Rolling Alex's segments after a second edit of Alex's own.
    let o = method_as(&alex, &["set", &file, "title", "Shared Feature 2"]);
    assert_eq!(code(&o), 0);
    let o = method_as(&alex, &["sync", &file, "--roll"]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(out(&o).contains("rolled"), "{}", out(&o));
    let o = method_as(&jordan, &["log", &file]);
    assert!(out(&o).contains("3 changes by 2 actors"), "{}", out(&o));

    // A folder with no shared budget.
    let empty = d.path("empty");
    std::fs::create_dir_all(&empty).unwrap();
    let o = method_as(&alex, &["sync", &empty]);
    assert_eq!(code(&o), 1);
    assert!(err(&o).contains("no shared budget"), "{}", err(&o));
}
