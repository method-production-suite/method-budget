//! The `fwb` binary end to end: every subcommand run as a script would run
//! it, checking output, files and exit codes (docs/CLI.md).

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn fwb(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fwb"))
        .args(args)
        .output()
        .expect("fwb runs")
}

fn fwb_stdin(args: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_fwb"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("fwb runs");
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
        let p = std::env::temp_dir().join(format!("fwb-cli-it-{name}-{}", std::process::id()));
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
        let o = fwb(&["new", &p]);
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

/// `name<TAB>amount` from `fwb totals`.
fn total(p: &str, name: &str) -> String {
    let o = fwb(&["totals", p]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    out(&o)
        .lines()
        .find_map(|l| l.strip_prefix(&format!("{name}\t")).map(str::to_string))
        .unwrap_or_else(|| panic!("no {name} in {}", out(&o)))
}

#[test]
fn help_and_usage_errors() {
    let o = fwb(&["help"]);
    assert_eq!(code(&o), 0);
    for c in [
        "show", "check", "export", "report", "totals", "set", "apply", "import",
    ] {
        assert!(out(&o).contains(&format!("  fwb {c} ")), "{c}");
    }
    assert_eq!(code(&fwb(&[])), 2);
    let o = fwb(&["frobnicate"]);
    assert_eq!(code(&o), 2);
    assert!(err(&o).contains("unknown command"));
    assert_eq!(code(&fwb(&["totals"])), 2);
    assert_eq!(code(&fwb(&["totals", "a.fwb", "--nope"])), 2);
    let o = fwb(&["totals", "/no/such/file.fwb"]);
    assert_eq!(code(&o), 1);
}

#[test]
fn basics_new_check_fmt_show_sample() {
    let d = Dir::new("basics");
    let p = d.starter("a.fwb");
    assert_eq!(code(&fwb(&["new", &p])), 1, "never over a file");
    let o = fwb(&["check", &p]);
    assert!(out(&o).contains("ok, net total"));
    let before = read(&p);
    assert_eq!(code(&fwb(&["fmt", &p])), 0);
    assert_eq!(read(&p), before);
    assert!(out(&fwb(&["show", &p])).contains("Grand total"));
    let s = d.path("big.fwb");
    assert_eq!(code(&fwb(&["sample", &s])), 0);
    assert_eq!(code(&fwb(&["check", &s])), 0);
}

#[test]
fn totals_text_and_json() {
    let d = Dir::new("totals");
    let p = d.starter("a.fwb");
    let o = fwb(&["totals", &p]);
    let text = out(&o);
    assert!(text.starts_with("category\t10\t"), "{text}");
    let net = total(&p, "net_total");
    let grand = total(&p, "grand_total");
    assert_eq!(net, grand);
    let j = out(&fwb(&["totals", &p, "--json"]));
    // The JSON's net total in cents is the text's in decimals.
    let cents = net.replace('.', "").trim_start_matches('0').to_string();
    assert!(j.contains(&format!("\"net_total\": {cents},")), "{j}");
    assert!(j.trim_start().starts_with('{') && j.trim_end().ends_with('}'));
    assert!(j.contains("\"accounts\": [{\"code\": \"10-01\""));
}

#[test]
fn export_shorthand_flags() {
    let d = Dir::new("export");
    let p = d.starter("Film.fwb");
    let x = d.path("Film.xlsx");
    let o = fwb(&["export", &p, "--xlsx", &x]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(std::fs::read(&x).unwrap().starts_with(b"PK"));
    let c = d.path("Film.csv");
    assert_eq!(code(&fwb(&["export", &p, "--csv", &c])), 0);
    assert!(Path::new(&d.path("Film - Detail.csv")).exists());
    assert_eq!(code(&fwb(&["export", &p])), 2);
}

#[test]
fn reports_in_every_format() {
    let d = Dir::new("report");
    let p = d.starter("a.fwb");
    let date = "2026-10-09 12:00 UTC";
    let o = fwb(&["report", &p, "--date", date]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    let txt = out(&o);
    assert!(txt.contains("Topsheet\n========"), "{txt}");
    assert!(txt.contains("Detail\n======"));
    assert!(txt.contains("2026-10-09 12:00 UTC"));
    // The same run gives the same text.
    assert_eq!(out(&fwb(&["report", &p, "--date", date])), txt);

    let all = "topsheet,accounts,detail,fringes,globals,groups,notes,info,chart,units,sets,locations,currencies";
    let h = d.path("r.html");
    let o = fwb(&["report", &p, "--sections", all, "-o", &h, "--date", date]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    let html = read(&h);
    assert!(html.starts_with("<!DOCTYPE html>"));
    assert!(html.contains("@page { size: letter portrait"));
    assert_eq!(html.matches("<section>").count(), 13);
    assert!(!html.contains("<script"));

    let c = d.path("r.csv");
    assert_eq!(code(&fwb(&["report", &p, "-o", &c, "--date", date])), 0);
    let csv = read(&c);
    assert!(
        csv.starts_with("Topsheet\r\nCode,Description,Total\r\n"),
        "{csv}"
    );

    let pdf_path = d.path("r.pdf");
    let o = fwb(&[
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

    assert_eq!(code(&fwb(&["report", &p, "--format", "pdf"])), 2);
    assert_eq!(code(&fwb(&["report", &p, "--sections", "nope"])), 2);
    assert_eq!(code(&fwb(&["report", &p, "--format", "doc"])), 2);
    let o = fwb(&["report", &p, "--preset", "Missing"]);
    assert_eq!(code(&o), 1);
    assert!(err(&o).contains("no report presets"));
    assert_eq!(out(&fwb(&["report", &p, "--list-presets"])), "");
    // Never over the budget itself.
    assert_eq!(code(&fwb(&["report", &p, "-o", &p, "--format", "txt"])), 1);
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
    let p = d.starter("a.fwb");
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
        code(&fwb(&["check", &p])),
        0,
        "{}",
        err(&fwb(&["check", &p]))
    );
    assert_eq!(out(&fwb(&["report", &p, "--list-presets"])), "* Short\n");
    let txt = out(&fwb(&["report", &p, "--date", "x"]));
    assert!(
        txt.contains("Topsheet") && !txt.contains("Detail\n"),
        "{txt}"
    );
    let txt = out(&fwb(&["report", &p, "--preset", "short", "--date", "x"]));
    assert!(!txt.contains("Detail\n"));
    let h = d.path("r.html");
    fwb(&["report", &p, "-o", &h]);
    assert!(read(&h).contains("size: letter landscape"));
}

#[test]
fn set_changes_one_value() {
    let d = Dir::new("set");
    let p = d.starter("a.fwb");
    let before = read(&p);
    let o = fwb(&["set", &p, "line.10-01:1.rate", "60000", "--dry-run"]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(
        out(&o).contains("rate \"50000\" -> \"60000\""),
        "{}",
        out(&o)
    );
    assert!(out(&o).contains("dry run: nothing written"));
    assert_eq!(read(&p), before);

    let grand = total(&p, "grand_total");
    let o = fwb(&["set", &p, "line.10-01:1.rate", "60000"]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(out(&o).contains("Grand total"));
    assert_ne!(total(&p, "grand_total"), grand);
    assert!(read(&p).contains("\"rate\": \"60000\""));

    assert_eq!(code(&fwb(&["set", &p, "global.SHOOT_DAYS", "30"])), 0);
    assert!(read(&p).contains("\"SHOOT_DAYS\": \"30\""));
    assert_eq!(
        code(&fwb(&["set", &p, "charge.Contingency.applied", "false"])),
        0
    );
    assert_eq!(total(&p, "charges_total"), "0.00");
    assert_eq!(code(&fwb(&["set", &p, "title", "New title"])), 0);
    assert!(read(&p).contains("\"title\": \"New title\""));

    // Refused edits change nothing.
    let now = read(&p);
    let o = fwb(&["set", &p, "fringe.NOPE.rate", "5"]);
    assert_eq!(code(&o), 3);
    assert!(err(&o).contains("there is no fringe NOPE"), "{}", err(&o));
    let o = fwb(&["set", &p, "line.10-01:1.rate", "NO_SUCH_GLOBAL * 2"]);
    assert_eq!(code(&o), 3, "{}", err(&o));
    assert!(err(&o).contains("nothing was written"));
    assert_eq!(code(&fwb(&["set", &p, "nonsense", "1"])), 2);
    assert_eq!(code(&fwb(&["set", &p, "line.10-01:1"])), 2);
    assert_eq!(read(&p), now);
}

#[test]
fn apply_runs_a_script_all_or_nothing() {
    let d = Dir::new("apply");
    let p = d.starter("a.fwb");
    let script = d.path("edit.txt");
    std::fs::write(
        &script,
        "# a week of changes\n\
         set title \"Feature, draft 3\"\n\
         set global SHOOT_DAYS 30\n\
         set fringe PAYROLL rate 18.5\n\
         set fringe PAYROLL cutoff 150,000.00\n\
         set line 10-01:1 description \"Screenplay (two drafts)\"\n\
         set line #1 bold true\n\
         remove tag 10-01:* fringe PAYROLL\n\
         add tag #1 fringe PAYROLL\n\
         rename global SHOOT_DAYS DAYS\n\
         remove charge Contingency\n\
         \n",
    )
    .unwrap();
    let o = fwb(&["apply", &p, &script, "--dry-run"]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(out(&o).contains("dry run"));
    let o = fwb(&["apply", &p, &script]);
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
    let o = fwb(&["apply", &p, &script]);
    assert_eq!(code(&o), 3);
    assert!(
        err(&o).contains("edit.txt line 2: there is no group Nobody"),
        "{}",
        err(&o)
    );
    assert_eq!(read(&p), before);
    std::fs::write(&script, "set global DAYS\n").unwrap();
    let o = fwb(&["apply", &p, &script]);
    assert_eq!(code(&o), 3);
    assert!(err(&o).contains("line 1: missing a value"));
    assert_eq!(read(&p), before);

    // From stdin.
    let o = fwb_stdin(&["apply", &p, "-"], "set global DAYS 32\n");
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(read(&p).contains("\"DAYS\": \"32\""));
    let o = fwb_stdin(&["apply", &p, "-"], "# nothing\n");
    assert!(out(&o).contains("no changes"));
}

#[test]
fn compact_versions_through_the_cli() {
    let d = Dir::new("compact");
    let p = d.starter("a.fwb");
    assert_eq!(code(&fwb(&["set", &p, "compact_versions", "true"])), 0);
    assert!(read(&p).contains("\"compact_versions\": true"));
    assert_eq!(code(&fwb(&["check", &p])), 0);
    assert_eq!(code(&fwb(&["set", &p, "compact_versions", "false"])), 0);
    assert!(!read(&p).contains("compact_versions"));
}

#[test]
fn import_with_and_without_a_mapping() {
    let d = Dir::new("import");
    let p = d.starter("a.fwb");
    let x = d.path("a.xlsx");
    assert_eq!(code(&fwb(&["export", &p, "--xlsx", &x])), 0);

    // Into a new budget, accounts created from the sheet.
    let m = d.path("m.json");
    std::fs::write(
        &m,
        r#"{"sheet": "Detail", "create_accounts": true, "create_setup": true}"#,
    )
    .unwrap();
    let out_path = d.path("new.fwb");
    let o = fwb(&["import", &x, "--mapping", &m, "-o", &out_path]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(out(&o).contains("different: 0"), "{}", out(&o));
    assert_eq!(code(&fwb(&["check", &out_path])), 0);
    // Lines only: the same detail total as the budget's lines.
    let o = fwb(&["import", &x, "--mapping", &m, "-o", &out_path]);
    assert_eq!(code(&o), 1, "an existing file is refused");
    assert!(err(&o).contains("--force"));
    assert_eq!(
        code(&fwb(&[
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
    let into = d.path("into.fwb");
    let o = fwb(&["import", &x, "--into", &p, "-o", &into]);
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
    let o = fwb(&[
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
    let o = fwb(&["import", &csv, "--mapping", &m, "-o", &d.path("x.fwb")]);
    assert_eq!(code(&o), 1);
    assert!(err(&o).contains("unknown field `bogus`"));
    assert!(!Path::new(&d.path("x.fwb")).exists());
    let o = fwb(&["import", &csv, "-o", &d.path("x.fwb")]);
    assert_eq!(code(&o), 1);
    assert!(err(&o).contains("create_accounts"), "{}", err(&o));
    assert_eq!(code(&fwb(&["import", &csv])), 2);
}
