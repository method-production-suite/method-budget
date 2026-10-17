//! `method`, the command line's old name, end to end: it runs the same code
//! as `method-budget`, adds one note on stderr, and stays silent for `mcp`
//! and `merge-driver` (wired into MCP client and git configs) (docs/CLI.md).

use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

const NOTE: &str =
    "note: `method` is now `method-budget`; the old name will be removed in a future release.\n";

fn run(bin: &str, args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(bin)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary runs");
    let mut input = child.stdin.take().unwrap();
    input.write_all(stdin.as_bytes()).unwrap();
    drop(input);
    child.wait_with_output().unwrap()
}

/// The same arguments under both names: `(method-budget, method)`.
fn both(args: &[&str], stdin: &str) -> (Output, Output) {
    (
        run(env!("CARGO_BIN_EXE_method-budget"), args, stdin),
        run(env!("CARGO_BIN_EXE_method"), args, stdin),
    )
}

fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

/// The alias's output is the real binary's, plus the note first on stderr.
fn assert_noted(args: &[&str]) {
    let (new, old) = both(args, "");
    assert_eq!(old.stdout, new.stdout, "{args:?}");
    assert_eq!(old.status.code(), new.status.code(), "{args:?}");
    assert_eq!(err(&old), format!("{NOTE}{}", err(&new)), "{args:?}");
}

/// The alias's output is the real binary's, byte for byte.
fn assert_identical(new: &Output, old: &Output) {
    assert_eq!(old.stdout, new.stdout);
    assert_eq!(old.stderr, new.stderr);
    assert_eq!(old.status.code(), new.status.code());
    assert!(!err(old).contains("note:"), "{}", err(old));
}

struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Dir {
        let p = std::env::temp_dir().join(format!("method-alias-it-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Dir(p)
    }

    fn path(&self, name: &str) -> String {
        self.0.join(name).to_string_lossy().into_owned()
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn the_real_name_prints_no_note() {
    let o = run(env!("CARGO_BIN_EXE_method-budget"), &["--version"], "");
    assert!(o.status.success());
    assert_eq!(err(&o), "");
}

#[test]
fn the_alias_notes_the_rename_once_and_otherwise_matches() {
    assert_noted(&["--version"]);
    assert_noted(&["help"]);
    assert_noted(&[]);
    assert_noted(&["frobnicate"]);
    assert_noted(&["totals", "/no/such/file.mthd"]);

    let d = Dir::new("noted");
    let file = d.path("a.mthd");
    let (new, _) = both(&["new", &file], "");
    assert!(new.status.success(), "{}", err(&new));
    assert_noted(&["show", &file]);
    assert_noted(&["totals", &file, "--json"]);
    assert_noted(&["check", &file]);
}

#[test]
fn the_alias_is_silent_for_mcp() {
    let d = Dir::new("mcp");
    let session = concat!(
        r#"{"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "alias", "version": "1"}}}"#,
        "\n",
        r#"{"jsonrpc": "2.0", "method": "notifications/initialized"}"#,
        "\n",
        r#"{"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}"#,
        "\n",
    );
    let (new, old) = both(&["mcp", "--root", &d.path("")], session);
    assert!(new.status.success(), "{}", err(&new));
    assert_eq!(String::from_utf8_lossy(&new.stdout).lines().count(), 2);
    assert_identical(&new, &old);
}

#[test]
fn the_alias_is_silent_for_merge_driver() {
    let d = Dir::new("merge-driver");
    let base = d.path("base.mthd");
    let (new, _) = both(&["new", &base], "");
    assert!(new.status.success(), "{}", err(&new));
    let text = std::fs::read_to_string(&base).unwrap();
    for name in ["ours.mthd", "theirs.mthd"] {
        std::fs::write(d.path(name), &text).unwrap();
    }
    let args = [
        "merge-driver",
        &base,
        &d.path("ours.mthd"),
        &d.path("theirs.mthd"),
    ];
    let (new, old) = both(&args, "");
    assert!(new.status.success(), "{}", err(&new));
    assert_identical(&new, &old);

    // A usage error too: the message, and nothing else.
    let (new, old) = both(&["merge-driver"], "");
    assert_eq!(new.status.code(), Some(2));
    assert_identical(&new, &old);
}
