//! `method coa` and `method new --template` end to end.

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

fn dir(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("method-cli-coa-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn s(p: &std::path::Path) -> String {
    p.to_string_lossy().into_owned()
}

const CHART: &str = "kind,code,name,parent\n\
category,1100,STORY & RIGHTS,\n\
account,1101,Rights purchased,1100\n\
account,1102,Writers,1100\n\
total,,TOTAL ABOVE-THE-LINE,\n\
category,2000,PRODUCTION STAFF,\n\
account,2001,Unit production manager,2000\n\
total,,TOTAL PRODUCTION,\n";

#[test]
fn import_writes_a_template_that_reads_back() {
    let d = dir("import");
    let src = d.join("Studio Chart.csv");
    std::fs::write(&src, CHART).unwrap();
    let t = d.join("t.mthd");
    let o = method(&["coa", "import", &s(&src), "-o", &s(&t)]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(
        out(&o).contains("2 categories, 3 accounts, 2 production totals"),
        "{}",
        out(&o)
    );
    let b = method_format::from_str(&std::fs::read_to_string(&t).unwrap()).unwrap();
    assert_eq!(b.title, "Studio Chart");
    let codes: Vec<&str> = b
        .categories()
        .flat_map(|c| &c.accounts)
        .map(|a| a.code.as_str())
        .collect();
    assert_eq!(codes, ["1101", "1102", "2001"]);

    // The template read back as a chart gives the same file.
    let again = d.join("again.mthd");
    let o = method(&[
        "coa",
        "import",
        &s(&t),
        "-o",
        &s(&again),
        "--title",
        "Studio Chart",
    ]);
    assert!(o.status.success());
    assert_eq!(std::fs::read(&t).unwrap(), std::fs::read(&again).unwrap());

    // An existing output is refused without --force; --dry-run writes nothing.
    assert_eq!(
        method(&["coa", "import", &s(&src), "-o", &s(&t)])
            .status
            .code(),
        Some(1)
    );
    let dry = d.join("dry.mthd");
    assert!(
        method(&["coa", "import", &s(&src), "-o", &s(&dry), "--dry-run"])
            .status
            .success()
    );
    assert!(!dry.exists());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn applying_to_a_budget_keeps_accounts_with_lines() {
    let d = dir("apply");
    let src = d.join("chart.csv");
    std::fs::write(&src, CHART).unwrap();
    let budget = d.join("b.mthd");
    assert!(method(&["new", &s(&budget)]).status.success());
    let before = method_format::from_str(&std::fs::read_to_string(&budget).unwrap()).unwrap();
    let lines = |b: &method_core::Budget| -> usize {
        b.categories()
            .flat_map(|c| &c.accounts)
            .map(|a| a.details.len())
            .sum()
    };
    let o = method(&[
        "coa",
        "import",
        &s(&src),
        "-o",
        &s(&budget),
        "--into",
        &s(&budget),
        "--remove-unlisted",
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(out(&o).contains("kept"), "{}", out(&o));
    let after = method_format::from_str(&std::fs::read_to_string(&budget).unwrap()).unwrap();
    assert_eq!(lines(&after), lines(&before), "no line is lost");
    assert!(after
        .categories()
        .any(|c| c.accounts.iter().any(|a| a.name == "Rights purchased")));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn new_from_a_bundled_chart_and_the_list() {
    let d = dir("new");
    let o = method(&["coa", "list"]);
    assert!(o.status.success());
    for c in method_format::charts::all() {
        assert!(out(&o).contains(c.id), "{}", c.id);
        let p = d.join(format!("{}.mthd", c.id));
        let o = method(&["new", &s(&p), "--template", c.id]);
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        let b = method_format::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        assert_eq!(b.title, "Untitled");
        assert!(b.categories().count() > 3);
        let o = method(&["check", &s(&p)]);
        assert!(o.status.success(), "{}: {}", c.id, out(&o));
    }
    let o = method(&["new", &s(&d.join("x.mthd")), "--template", "no-such-chart"]);
    assert_eq!(o.status.code(), Some(1));
    let _ = std::fs::remove_dir_all(&d);
}
