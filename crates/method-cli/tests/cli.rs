//! The `method` binary end to end: every subcommand run as a script would run
//! it, checking output, files and exit codes (docs/CLI.md).

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn method(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_method"))
        .args(args)
        .output()
        .expect("method runs")
}

fn method_stdin(args: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_method"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("method runs");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
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

/// A fresh folder for one test.
struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Dir {
        let p = std::env::temp_dir().join(format!("method-cli-it-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Dir(p)
    }

    fn path(&self, name: &str) -> String {
        self.0.join(name).to_string_lossy().into_owned()
    }

    /// A starter budget at `name`.
    fn starter(&self, name: &str) -> String {
        let p = self.path(name);
        let o = method(&["new", &p]);
        assert_eq!(code(&o), 0, "{}", err(&o));
        p
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn read(p: &str) -> String {
    std::fs::read_to_string(p).unwrap()
}

/// `name<TAB>amount` from `method totals`.
fn total(p: &str, name: &str) -> String {
    let o = method(&["totals", p]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    out(&o)
        .lines()
        .find_map(|l| l.strip_prefix(&format!("{name}\t")).map(str::to_string))
        .unwrap_or_else(|| panic!("no {name} in {}", out(&o)))
}

#[test]
fn help_and_usage_errors() {
    let o = method(&["help"]);
    assert_eq!(code(&o), 0);
    for c in [
        "show", "check", "export", "report", "totals", "set", "apply", "import",
    ] {
        assert!(out(&o).contains(&format!("  method {c} ")), "{c}");
    }
    assert_eq!(code(&method(&[])), 2);
    let o = method(&["frobnicate"]);
    assert_eq!(code(&o), 2);
    assert!(err(&o).contains("unknown command"));
    assert_eq!(code(&method(&["totals"])), 2);
    assert_eq!(code(&method(&["totals", "a.mthd", "--nope"])), 2);
    let o = method(&["totals", "/no/such/file.mthd"]);
    assert_eq!(code(&o), 1);
}

#[test]
fn version_flag() {
    // The workspace version, the same one the app's About box shows.
    let o = method(&["--version"]);
    assert_eq!(code(&o), 0);
    assert_eq!(
        out(&o),
        format!("method {} (Method Budget)\n", env!("CARGO_PKG_VERSION"))
    );
    assert_eq!(out(&method(&["-V"])), out(&o));
}

#[test]
fn basics_new_check_fmt_show_sample() {
    let d = Dir::new("basics");
    let p = d.starter("a.mthd");
    assert_eq!(code(&method(&["new", &p])), 1, "never over a file");
    let o = method(&["check", &p]);
    assert!(out(&o).contains("ok, net total"));
    let before = read(&p);
    assert_eq!(code(&method(&["fmt", &p])), 0);
    assert_eq!(read(&p), before);
    assert!(out(&method(&["show", &p])).contains("Grand total"));
    let s = d.path("big.mthd");
    assert_eq!(code(&method(&["sample", &s])), 0);
    assert_eq!(code(&method(&["check", &s])), 0);
}

#[test]
fn totals_text_and_json() {
    let d = Dir::new("totals");
    let p = d.starter("a.mthd");
    let o = method(&["totals", &p]);
    let text = out(&o);
    assert!(text.starts_with("category\t10\t"), "{text}");
    let net = total(&p, "net_total");
    let grand = total(&p, "grand_total");
    assert_eq!(net, grand);
    let j = out(&method(&["totals", &p, "--json"]));
    // The JSON's net total in cents is the text's in decimals.
    let cents = net.replace('.', "").trim_start_matches('0').to_string();
    assert!(j.contains(&format!("\"net_total\": {cents},")), "{j}");
    assert!(j.trim_start().starts_with('{') && j.trim_end().ends_with('}'));
    assert!(j.contains("\"accounts\": [{\"code\": \"1001\""));
}

#[test]
fn export_shorthand_flags() {
    let d = Dir::new("export");
    let p = d.starter("Film.mthd");
    let x = d.path("Film.xlsx");
    let o = method(&["export", &p, "--xlsx", &x]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(std::fs::read(&x).unwrap().starts_with(b"PK"));
    let c = d.path("Film.csv");
    assert_eq!(code(&method(&["export", &p, "--csv", &c])), 0);
    assert!(Path::new(&d.path("Film - Detail.csv")).exists());
    assert_eq!(code(&method(&["export", &p])), 2);
}

#[test]
fn reports_in_every_format() {
    let d = Dir::new("report");
    let p = d.starter("a.mthd");
    let date = "2026-10-09 12:00 UTC";
    let o = method(&["report", &p, "--date", date]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    let txt = out(&o);
    assert!(txt.contains("Topsheet\n========"), "{txt}");
    assert!(txt.contains("Detail\n======"));
    assert!(txt.contains("2026-10-09 12:00 UTC"));
    // The same run gives the same text.
    assert_eq!(out(&method(&["report", &p, "--date", date])), txt);

    let all = "topsheet,accounts,detail,fringes,globals,groups,notes,info,chart,units,sets,locations,currencies";
    let h = d.path("r.html");
    let o = method(&["report", &p, "--sections", all, "-o", &h, "--date", date]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    let html = read(&h);
    assert!(html.starts_with("<!DOCTYPE html>"));
    assert!(html.contains("@page { size: letter portrait"));
    assert_eq!(html.matches("<section>").count(), 13);
    assert!(!html.contains("<script"));

    let c = d.path("r.csv");
    assert_eq!(code(&method(&["report", &p, "-o", &c, "--date", date])), 0);
    let csv = read(&c);
    assert!(
        csv.starts_with("Topsheet\r\nCode,Description,Total\r\n"),
        "{csv}"
    );

    let pdf_path = d.path("r.pdf");
    let o = method(&[
        "report",
        &p,
        "--sections",
        all,
        "-o",
        &pdf_path,
        "--date",
        date,
    ]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    check_pdf(&std::fs::read(&pdf_path).unwrap());

    assert_eq!(code(&method(&["report", &p, "--format", "pdf"])), 2);
    assert_eq!(code(&method(&["report", &p, "--sections", "nope"])), 2);
    assert_eq!(code(&method(&["report", &p, "--format", "doc"])), 2);
    let o = method(&["report", &p, "--preset", "Missing"]);
    assert_eq!(code(&o), 1);
    assert!(err(&o).contains("no report presets"));
    assert_eq!(out(&method(&["report", &p, "--list-presets"])), "");
    // Never over the budget itself.
    assert_eq!(
        code(&method(&["report", &p, "-o", &p, "--format", "txt"])),
        1
    );
}

/// A PDF whose cross-reference table points at every object.
fn check_pdf(bytes: &[u8]) {
    assert!(bytes.starts_with(b"%PDF-1.4\n"));
    assert!(bytes.ends_with(b"%%EOF\n"));
    // Everything after the binary marker on line 2 is ASCII.
    let tail = |from: usize| String::from_utf8_lossy(&bytes[from..]).into_owned();
    let marker = b"startxref\n";
    let at = bytes
        .windows(marker.len())
        .rposition(|w| w == marker)
        .unwrap();
    let start: usize = tail(at + marker.len())
        .lines()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let xref = tail(start);
    assert!(xref.starts_with("xref\n"));
    let mut lines = xref.lines().skip(1);
    let count: usize = lines
        .next()
        .unwrap()
        .split(' ')
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    lines.next(); // the free entry
    for n in 1..count {
        let entry = lines.next().unwrap();
        assert_eq!(entry.len(), 19, "{entry:?}"); // 20 bytes with the newline
        let off: usize = entry[..10].parse().unwrap();
        let obj = format!("{n} 0 obj\n");
        assert!(
            bytes[off..].starts_with(obj.as_bytes()),
            "object {n} at {off}"
        );
    }
    let all = tail(0);
    assert!(all.contains("/Type /Page "));
    assert!(all.contains("Page 1 of "));
}

#[test]
fn report_presets_from_the_budget() {
    let d = Dir::new("presets");
    let p = d.starter("a.mthd");
    // Add a preset by editing the file: the format is open.
    let text = read(&p);
    let preset = r#"    "report_presets": [
      {
        "name": "Short",
        "default": true,
        "sections": ["topsheet"],
        "landscape": true
      }
    ]
  }
}
"#;
    let cut = text.rfind("\n  }\n}").unwrap();
    std::fs::write(&p, format!("{},\n{preset}", &text[..cut])).unwrap();
    assert_eq!(
        code(&method(&["check", &p])),
        0,
        "{}",
        err(&method(&["check", &p]))
    );
    assert_eq!(out(&method(&["report", &p, "--list-presets"])), "* Short\n");
    let txt = out(&method(&["report", &p, "--date", "x"]));
    assert!(
        txt.contains("Topsheet") && !txt.contains("Detail\n"),
        "{txt}"
    );
    let txt = out(&method(&["report", &p, "--preset", "short", "--date", "x"]));
    assert!(!txt.contains("Detail\n"));
    let h = d.path("r.html");
    method(&["report", &p, "-o", &h]);
    assert!(read(&h).contains("size: letter landscape"));
}

#[test]
fn set_changes_one_value() {
    let d = Dir::new("set");
    let p = d.starter("a.mthd");
    let before = read(&p);
    let o = method(&["set", &p, "line.1001:1.rate", "60000", "--dry-run"]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(
        out(&o).contains("rate \"50000\" -> \"60000\""),
        "{}",
        out(&o)
    );
    assert!(out(&o).contains("dry run: nothing written"));
    assert_eq!(read(&p), before);

    let grand = total(&p, "grand_total");
    let o = method(&["set", &p, "line.1001:1.rate", "60000"]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(out(&o).contains("Grand total"));
    assert_ne!(total(&p, "grand_total"), grand);
    assert!(read(&p).contains("\"rate\": \"60000\""));

    assert_eq!(code(&method(&["set", &p, "global.SHOOT_DAYS", "30"])), 0);
    assert!(read(&p).contains("\"SHOOT_DAYS\": \"30\""));
    assert_eq!(
        code(&method(&["set", &p, "charge.Contingency.applied", "false"])),
        0
    );
    assert_eq!(total(&p, "charges_total"), "0.00");
    assert_eq!(code(&method(&["set", &p, "title", "New title"])), 0);
    assert!(read(&p).contains("\"title\": \"New title\""));
    // A cutoff, then `none` clears it (docs/CLI.md).
    let o = method(&["set", &p, "fringe.PAYROLL.cutoff", "150,000.00"]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(read(&p).contains("\"cutoff\": 15000000"));
    let o = method(&["set", &p, "fringe.PAYROLL.cutoff", "none"]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(!read(&p).contains("\"cutoff\""));

    // Refused edits change nothing.
    let now = read(&p);
    let o = method(&["set", &p, "fringe.NOPE.rate", "5"]);
    assert_eq!(code(&o), 3);
    assert!(err(&o).contains("there is no fringe NOPE"), "{}", err(&o));
    let o = method(&["set", &p, "line.1001:1.rate", "NO_SUCH_GLOBAL * 2"]);
    assert_eq!(code(&o), 3, "{}", err(&o));
    assert!(err(&o).contains("nothing was written"));
    assert_eq!(code(&method(&["set", &p, "nonsense", "1"])), 2);
    assert_eq!(code(&method(&["set", &p, "line.1001:1"])), 2);
    assert_eq!(read(&p), now);
}

/// Posting by production level, `method check` warns about fringes after
/// the last production total, and `method set trailing_level_fringes drop`
/// leaves them out as Movie Magic does (decisions.md, entry 56).
#[test]
fn check_warns_about_trailing_level_fringes() {
    let d = Dir::new("trailing");
    let p = d.path("t.mthd");
    let line = |id: u32| {
        format!(
            r#"{{"id": {id}, "description": "Pay", "amount": "1", "rate": "1000", "fringes": ["P"]}}"#
        )
    };
    let cat = |code: &str, id: u32| {
        format!(
            r#"{{"kind": "category", "code": "{code}", "name": "C", "accounts": [{{"code": "{code}-01", "name": "A", "details": [{}]}}]}}"#,
            line(id)
        )
    };
    let text = format!(
        r#"{{"format": "method-budget", "version": 1, "budget": {{"title": "T", "base_currency": "USD", "fringe_posting": {{"kind": "production_level"}}, "fringes": {{"P": {{"kind": "percent", "rate": "10"}}}}, "topsheet": [{}, {{"kind": "production_total", "name": "Total"}}, {}]}}}}"#,
        cat("11", 1),
        cat("41", 2)
    );
    std::fs::write(&p, text).unwrap();
    let o = method(&["check", &p]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(out(&o).contains("ok, net total 2,200.00"), "{}", out(&o));
    assert!(
        out(&o).contains(
            "warning: fringes of categories after the last production total are not in any level (41 100.00)"
        ),
        "{}",
        out(&o)
    );

    let o = method(&["set", &p, "trailing_level_fringes", "drop"]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(read(&p).contains("\"trailing_level_fringes\": \"drop\""));
    let o = method(&["check", &p]);
    assert!(out(&o).contains("ok, net total 2,100.00"), "{}", out(&o));
    assert!(out(&o).contains("left out of every total"), "{}", out(&o));
    assert_eq!(
        code(&method(&["set", &p, "trailing_level_fringes", "lose"])),
        2
    );
}

/// The group rules from the command line (decisions.md, entry 57): lines
/// with no group left out, then conflicts counted; written only when not
/// the default.
#[test]
fn set_group_rules() {
    let d = Dir::new("group-rules");
    let p = d.path("g.mthd");
    let text = r#"{"format": "method-budget", "version": 1, "budget": {"title": "T", "base_currency": "USD",
        "groups": {"IN": {}, "OFF": {"in_budget_total": false}},
        "topsheet": [{"kind": "category", "code": "11", "name": "C", "accounts": [{"code": "11-01", "name": "A", "details": [
            {"id": 1, "description": "In", "amount": "1", "rate": "1000", "groups": ["IN"]},
            {"id": 2, "description": "Both", "amount": "1", "rate": "3000", "groups": ["IN", "OFF"]},
            {"id": 3, "description": "None", "amount": "1", "rate": "500"}]}]}]}}"#;
    std::fs::write(&p, text).unwrap();
    let o = method(&["check", &p]);
    assert!(out(&o).contains("ok, net total 1,500.00"), "{}", out(&o));

    let o = method(&["set", &p, "group_rules.ungrouped_lines", "exclude"]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(read(&p).contains("\"ungrouped_lines\": \"exclude\""));
    let o = method(&["check", &p]);
    assert!(out(&o).contains("ok, net total 1,000.00"), "{}", out(&o));

    let o = method(&["set", &p, "group_rules.conflict", "include"]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    let o = method(&["check", &p]);
    assert!(out(&o).contains("ok, net total 4,000.00"), "{}", out(&o));

    method(&["set", &p, "group_rules.ungrouped_lines", "include"]);
    method(&["set", &p, "group_rules.conflict", "exclude"]);
    assert!(!read(&p).contains("group_rules"), "defaults are left out");
    assert_eq!(
        code(&method(&["set", &p, "group_rules.conflict", "maybe"])),
        2
    );
    assert_eq!(
        code(&method(&["set", &p, "group_rules.nope", "include"])),
        2
    );
}

/// A flat fringe's basis and most hours a day from the command line
/// (decisions.md, entry 54).
#[test]
fn set_a_flat_fringes_basis_and_max_hours() {
    let d = Dir::new("set-flat");
    let p = d.starter("a.mthd");
    let text = read(&p).replacen(
        "\"fringes\": {",
        "\"fringes\": { \"MEALS\": { \"kind\": \"flat\", \"rate\": \"25\", \"unit\": \"Day\" },",
        1,
    );
    std::fs::write(&p, text).unwrap();
    let o = method(&["set", &p, "fringe.MEALS.basis", "amount_counts"]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(
        out(&o).contains("basis amount -> amount_counts"),
        "{}",
        out(&o)
    );
    assert!(read(&p).contains("\"basis\": \"amount_counts\""));
    let o = method(&["set", &p, "fringe.MEALS.max_hours_per_day", "12"]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(read(&p).contains("\"max_hours_per_day\": \"12\""));
    let o = method(&["set", &p, "fringe.MEALS.max_hours_per_day", "none"]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(!read(&p).contains("max_hours_per_day"));
    let o = method(&["set", &p, "fringe.MEALS.basis", "amount"]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(!read(&p).contains("\"basis\""), "the default is left out");

    let now = read(&p);
    let o = method(&["set", &p, "fringe.PAYROLL.basis", "quantity"]);
    assert_eq!(code(&o), 3);
    assert!(
        err(&o).contains("only a flat fringe has a basis"),
        "{}",
        err(&o)
    );
    let o = method(&["set", &p, "fringe.MEALS.cutoff", "100"]);
    assert_eq!(code(&o), 3);
    assert!(
        err(&o).contains("cutoffs apply to percent fringes only"),
        "{}",
        err(&o)
    );
    assert_eq!(
        code(&method(&["set", &p, "fringe.MEALS.basis", "hours"])),
        3
    );
    assert_eq!(read(&p), now);
}

#[test]
fn apply_runs_a_script_all_or_nothing() {
    let d = Dir::new("apply");
    let p = d.starter("a.mthd");
    let script = d.path("edit.txt");
    std::fs::write(
        &script,
        "# a week of changes\n\
         set title \"Feature, draft 3\"\n\
         set global SHOOT_DAYS 30\n\
         set fringe PAYROLL rate 18.5\n\
         set fringe PAYROLL cutoff 150,000.00\n\
         set line 1001:1 description \"Screenplay (two drafts)\"\n\
         set line #1 bold true\n\
         remove tag 1001:* fringe PAYROLL\n\
         add tag #1 fringe PAYROLL\n\
         rename global SHOOT_DAYS DAYS\n\
         remove charge Contingency\n\
         \n",
    )
    .unwrap();
    let o = method(&["apply", &p, &script, "--dry-run"]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(out(&o).contains("dry run"));
    let o = method(&["apply", &p, &script]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    let text = read(&p);
    assert!(text.contains("\"title\": \"Feature, draft 3\""));
    assert!(text.contains("\"DAYS\": \"30\""));
    assert!(!text.contains("SHOOT_DAYS\""));
    assert!(text.contains("\"rate\": \"18.5\""));
    assert!(text.contains("\"cutoff\": 15000000"));
    assert!(text.contains("Screenplay (two drafts)"));
    assert!(text.contains("\"bold\": true"));
    assert_eq!(total(&p, "charges_total"), "0.00");

    // One bad line: nothing written, the line named, exit 3.
    let before = read(&p);
    std::fs::write(&script, "set global DAYS 31\nadd tag #1 group Nobody\n").unwrap();
    let o = method(&["apply", &p, &script]);
    assert_eq!(code(&o), 3);
    assert!(
        err(&o).contains("edit.txt line 2: there is no group Nobody"),
        "{}",
        err(&o)
    );
    assert_eq!(read(&p), before);
    std::fs::write(&script, "set global DAYS\n").unwrap();
    let o = method(&["apply", &p, &script]);
    assert_eq!(code(&o), 3);
    assert!(err(&o).contains("line 1: missing a value"));
    assert_eq!(read(&p), before);

    // From stdin.
    let o = method_stdin(&["apply", &p, "-"], "set global DAYS 32\n");
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(read(&p).contains("\"DAYS\": \"32\""));
    let o = method_stdin(&["apply", &p, "-"], "# nothing\n");
    assert!(out(&o).contains("no changes"));

    // A byte order mark (Windows Notepad) before the first command.
    std::fs::write(&script, "\u{feff}set global DAYS 33\r\nset title BOM\r\n").unwrap();
    let o = method(&["apply", &p, &script]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(read(&p).contains("\"DAYS\": \"33\""));
}

/// Writing a budget keeps its permissions: one only its owner may read stays
/// that way after `fmt`, `set` and `apply`.
#[cfg(unix)]
#[test]
fn writes_keep_the_file_permissions() {
    use std::os::unix::fs::PermissionsExt as _;
    let d = Dir::new("perms");
    let p = d.starter("a.mthd");
    let mode = || std::fs::metadata(&p).unwrap().permissions().mode() & 0o777;
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(code(&method(&["fmt", &p])), 0);
    assert_eq!(mode(), 0o600);
    assert_eq!(code(&method(&["set", &p, "title", "Private"])), 0);
    assert_eq!(mode(), 0o600);
    let o = method_stdin(&["apply", &p, "-"], "set title Still\n");
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert_eq!(mode(), 0o600);
    assert!(read(&p).contains("\"title\": \"Still\""));
}

#[test]
fn compact_versions_through_the_cli() {
    let d = Dir::new("compact");
    let p = d.starter("a.mthd");
    assert_eq!(code(&method(&["set", &p, "compact_versions", "true"])), 0);
    assert!(read(&p).contains("\"compact_versions\": true"));
    assert_eq!(code(&method(&["check", &p])), 0);
    assert_eq!(code(&method(&["set", &p, "compact_versions", "false"])), 0);
    assert!(!read(&p).contains("compact_versions"));
}

#[test]
fn import_with_and_without_a_mapping() {
    let d = Dir::new("import");
    let p = d.starter("a.mthd");
    let x = d.path("a.xlsx");
    assert_eq!(code(&method(&["export", &p, "--xlsx", &x])), 0);

    // Into a new budget, accounts created from the sheet.
    let m = d.path("m.json");
    std::fs::write(
        &m,
        r#"{"sheet": "Detail", "create_accounts": true, "create_setup": true}"#,
    )
    .unwrap();
    let out_path = d.path("new.mthd");
    let o = method(&["import", &x, "--mapping", &m, "-o", &out_path]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(out(&o).contains("different: 0"), "{}", out(&o));
    assert_eq!(code(&method(&["check", &out_path])), 0);
    // Lines only: the same detail total as the budget's lines.
    let o = method(&["import", &x, "--mapping", &m, "-o", &out_path]);
    assert_eq!(code(&o), 1, "an existing file is refused");
    assert!(err(&o).contains("--force"));
    assert_eq!(
        code(&method(&[
            "import",
            &x,
            "--mapping",
            &m,
            "-o",
            &out_path,
            "--force"
        ])),
        0
    );

    // Into a copy of a budget, columns guessed: the lines double up.
    let into = d.path("into.mthd");
    let o = method(&["import", &x, "--into", &p, "-o", &into]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    let grand_before = total(&p, "atl_btl");
    let grand_after = total(&into, "atl_btl");
    let cents = |s: &str| -> i64 { s.replace('.', "").parse().unwrap() };
    assert_eq!(cents(&grand_after), 2 * cents(&grand_before));

    // A CSV with an explicit mapping.
    let csv = d.path("lines.csv");
    std::fs::write(&csv, "Acct,What,Qty,Price\r\n10-01,Polish,2,500\r\n").unwrap();
    std::fs::write(
        &m,
        r#"{"header_row": 1, "columns": {"account_code": "A", "description": 2, "amount": "C", "rate": "D"}}"#,
    )
    .unwrap();
    let o = method(&[
        "import",
        &csv,
        "--mapping",
        &m,
        "--into",
        &p,
        "-o",
        &into,
        "--force",
    ]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(read(&into).contains("\"description\": \"Polish\""));

    // Problems are failures that write nothing.
    std::fs::write(&m, r#"{"columns": {"bogus": 1}}"#).unwrap();
    let o = method(&["import", &csv, "--mapping", &m, "-o", &d.path("x.mthd")]);
    assert_eq!(code(&o), 1);
    assert!(err(&o).contains("unknown field `bogus`"));
    assert!(!Path::new(&d.path("x.mthd")).exists());
    let o = method(&["import", &csv, "-o", &d.path("x.mthd")]);
    assert_eq!(code(&o), 1);
    assert!(err(&o).contains("create_accounts"), "{}", err(&o));
    assert_eq!(code(&method(&["import", &csv])), 2);
}

#[test]
fn import_export_checks_and_writes() {
    let d = Dir::new("import-export");
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../method-format/tests/fixtures/exchange/by-level.json");
    let export = d.path("by-level.json");
    std::fs::copy(src, &export).unwrap();
    // Without -o: the report, nothing written.
    let o = method(&["import-export", &export]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    let text = out(&o);
    assert!(text.contains("by production level"), "{text}");
    assert!(
        text.contains("Grand total: export 34,020.00 USD, imported 34,020.00 USD (same)"),
        "{text}"
    );
    assert!(text.contains("nothing written"));
    // With -o: the budget, which checks.
    let mthd = d.path("imported.mthd");
    let o = method(&["import-export", &export, "-o", &mthd, "--title", "Levels"]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(out(&o).contains("wrote"));
    let o = method(&["check", &mthd]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(out(&o).contains("net total 34,020.00"), "{}", out(&o));
    assert!(read(&mthd).contains("\"title\": \"Levels\""));
    // An existing -o is refused without --force.
    let o = method(&["import-export", &export, "-o", &mthd]);
    assert_eq!(code(&o), 1);
    assert!(err(&o).contains("--force"));
    // JSON output.
    let o = method(&["import-export", &export, "--json"]);
    let v: method_format::serde_json::Value =
        method_format::serde_json::from_str(&out(&o)).unwrap();
    assert_eq!(v["reconciliation"]["grand_cents"], 3_402_000);
    assert_eq!(v["reconciliation"]["matches"], true);
    // Bad input.
    let o = method(&["import-export", &export, "--format", "pdf"]);
    assert_eq!(code(&o), 2);
    std::fs::write(d.path("x.txt"), "hello").unwrap();
    let o = method(&["import-export", &d.path("x.txt")]);
    assert_eq!(code(&o), 1);
    assert!(err(&o).contains("budget export"), "{}", err(&o));
}
