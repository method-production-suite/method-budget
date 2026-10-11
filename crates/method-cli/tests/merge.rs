//! `method diff`, `method merge` and `method merge-driver` end to end
//! (docs/MERGE.md).

use std::path::PathBuf;
use std::process::{Command, Output};

fn method(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_method"))
        .args(args)
        .output()
        .expect("method runs")
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

/// A fresh folder for one test, with a starter budget as `base.mthd`.
struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Dir {
        let p = std::env::temp_dir().join(format!("method-merge-it-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        let d = Dir(p);
        assert_eq!(code(&method(&["new", &d.path("base.mthd")])), 0);
        d
    }

    fn path(&self, name: &str) -> String {
        self.0.join(name).to_string_lossy().into_owned()
    }

    fn read(&self, name: &str) -> String {
        std::fs::read_to_string(self.path(name)).unwrap()
    }

    /// A copy of the base with the `n`-th `"rate": "<from>"` set to `to`.
    fn edited(&self, name: &str, edits: &[(&str, &str)]) -> String {
        let mut text = self.read("base.mthd");
        for (from, to) in edits {
            let needle = format!("\"rate\": \"{from}\"");
            assert!(text.contains(&needle), "{needle}");
            text = text.replacen(&needle, &format!("\"rate\": \"{to}\""), 1);
        }
        std::fs::write(self.path(name), text).unwrap();
        self.path(name)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn diff_prints_plain_words_and_json() {
    let d = Dir::new("diff");
    let base = d.path("base.mthd");
    let new = d.edited("new.mthd", &[("50000", "55000")]);
    let o = method(&["diff", &base, &new]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    let text = out(&o);
    assert!(
        text.contains("line 1: rate 50,000.00 -> 55,000.00 (+5,000.00 to"),
        "{text}"
    );
    assert!(text.contains("Grand total"), "{text}");
    let o = method(&["diff", &base, &new, "--json"]);
    assert_eq!(code(&o), 0);
    let json = out(&o);
    assert!(json.contains("\"kind\": \"changed\""), "{json}");
    assert!(json.contains("\"field\": \"rate\""), "{json}");
    // The same file: nothing, and --exit-code says so.
    let o = method(&["diff", &base, &base, "--exit-code"]);
    assert_eq!(code(&o), 0);
    assert_eq!(out(&o), "No changes.\n");
    let o = method(&["diff", &base, &new, "--exit-code"]);
    assert_eq!(code(&o), 1);
    assert!(out(&o).contains("rate"));
    // Usage.
    assert_eq!(code(&method(&["diff", &base])), 2);
}

#[test]
fn merge_takes_both_sides_and_lists_conflicts() {
    let d = Dir::new("merge");
    let base = d.path("base.mthd");
    let ours = d.edited("ours.mthd", &[("50000", "55000")]);
    let theirs = d.edited("theirs.mthd", &[("75000", "80000")]);
    let merged = d.path("merged.mthd");
    let o = method(&["merge", &base, &ours, &theirs, "-o", &merged]);
    assert_eq!(code(&o), 0, "{}{}", out(&o), err(&o));
    let text = d.read("merged.mthd");
    assert!(text.contains("\"rate\": \"55000\"") && text.contains("\"rate\": \"80000\""));

    // The same rate both ways: a conflict, ours kept, exit 1.
    let theirs = d.edited("theirs2.mthd", &[("50000", "60000")]);
    let o = method(&["merge", &base, &ours, &theirs, "-o", &merged]);
    assert_eq!(code(&o), 1);
    assert!(
        out(&o).contains("rate: ours 55,000.00, theirs 60,000.00, was 50,000.00 (took ours)"),
        "{}",
        out(&o)
    );
    assert!(
        err(&o).contains("1 conflict settled for ours"),
        "{}",
        err(&o)
    );
    assert!(d.read("merged.mthd").contains("\"rate\": \"55000\""));
    let o = method(&["merge", &base, &ours, &theirs, "-o", &merged, "--theirs"]);
    assert_eq!(code(&o), 1);
    assert!(d.read("merged.mthd").contains("\"rate\": \"60000\""));

    // Never over one of the budgets being merged.
    let before = d.read("base.mthd");
    for input in [&base, &ours, &theirs] {
        let o = method(&["merge", &base, &ours, &theirs, "-o", input]);
        assert_eq!(code(&o), 1);
        assert!(err(&o).contains("budget being merged"), "{}", err(&o));
    }
    assert_eq!(d.read("base.mthd"), before);

    // Usage.
    assert_eq!(code(&method(&["merge", &base, &ours, &theirs])), 2);
    assert_eq!(
        code(&method(&[
            "merge", &base, &ours, &theirs, "-o", &merged, "--ours", "--theirs"
        ])),
        2
    );
}

#[test]
fn merge_driver_writes_ours_in_place() {
    let d = Dir::new("driver");
    let base = d.path("base.mthd");
    let ours = d.edited("ours.mthd", &[("50000", "55000")]);
    let theirs = d.edited("theirs.mthd", &[("75000", "80000")]);
    let o = method(&["merge-driver", &base, &ours, &theirs, "budget.mthd"]);
    assert_eq!(code(&o), 0, "{}{}", out(&o), err(&o));
    let text = d.read("ours.mthd");
    assert!(text.contains("\"rate\": \"55000\"") && text.contains("\"rate\": \"80000\""));
    // A conflict: exit 1 so git marks the file conflicted.
    let theirs = d.edited("theirs2.mthd", &[("50000", "60000")]);
    let o = method(&["merge-driver", &base, &ours, &theirs, "budget.mthd"]);
    assert_eq!(code(&o), 1);
    assert!(err(&o).contains("budget.mthd"), "{}", err(&o));
    // No common base (added on both branches).
    std::fs::write(d.path("empty.mthd"), "").unwrap();
    let o = method(&["merge-driver", &d.path("empty.mthd"), &ours, &theirs]);
    assert_eq!(code(&o), 1);
    assert!(err(&o).contains("no common base"), "{}", err(&o));
}

/// The driver as git runs it (docs/MERGE.md, "With git"): a real `git merge`
/// of two branches that changed different rates of one budget, then two that
/// changed the same one.
#[test]
fn merge_driver_under_git() {
    let d = Dir::new("git");
    let git = |args: &[&str]| -> Output {
        Command::new("git")
            .arg("-C")
            .arg(&d.0)
            .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .output()
            .expect("git runs")
    };
    let ok = |o: Output| assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    ok(git(&["init", "--quiet", "--initial-branch=main"]));
    let driver = format!(
        "\"{}\" merge-driver %O %A %B %P",
        env!("CARGO_BIN_EXE_method")
    );
    ok(git(&["config", "merge.method-budget.driver", &driver]));
    std::fs::write(d.0.join(".gitattributes"), "*.mthd merge=method-budget\n").unwrap();
    std::fs::rename(d.path("base.mthd"), d.path("film.mthd")).unwrap();
    ok(git(&["add", "-A"]));
    ok(git(&["commit", "--quiet", "-m", "base"]));
    let base = d.read("film.mthd");
    let edit = |from: &str, to: &str| {
        let text = d.read("film.mthd").replacen(
            &format!("\"rate\": \"{from}\""),
            &format!("\"rate\": \"{to}\""),
            1,
        );
        std::fs::write(d.path("film.mthd"), text).unwrap();
    };

    ok(git(&["checkout", "--quiet", "-b", "theirs"]));
    edit("75000", "80000");
    ok(git(&["commit", "--quiet", "-am", "theirs"]));
    ok(git(&["checkout", "--quiet", "main"]));
    edit("50000", "55000");
    ok(git(&["commit", "--quiet", "-am", "ours"]));
    ok(git(&["merge", "--quiet", "--no-edit", "theirs"]));
    let merged = d.read("film.mthd");
    assert!(merged.contains("\"rate\": \"55000\"") && merged.contains("\"rate\": \"80000\""));
    assert_eq!(code(&method(&["check", &d.path("film.mthd")])), 0);

    // The same rate changed both ways: git reports a conflict, and the file
    // is still a budget, settled for ours.
    std::fs::write(d.path("film.mthd"), &base).unwrap();
    ok(git(&["commit", "--quiet", "-am", "reset"]));
    ok(git(&["checkout", "--quiet", "-b", "theirs2"]));
    edit("50000", "60000");
    ok(git(&["commit", "--quiet", "-am", "theirs2"]));
    ok(git(&["checkout", "--quiet", "main"]));
    edit("50000", "55000");
    ok(git(&["commit", "--quiet", "-am", "ours2"]));
    let o = git(&["merge", "--no-edit", "theirs2"]);
    assert!(!o.status.success(), "the merge should stop on the conflict");
    let status = String::from_utf8_lossy(&git(&["status", "--porcelain"]).stdout).into_owned();
    assert!(status.contains("UU film.mthd"), "{status}");
    assert!(d.read("film.mthd").contains("\"rate\": \"55000\""));
    assert_eq!(code(&method(&["check", &d.path("film.mthd")])), 0);
}
