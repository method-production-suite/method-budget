//! `fwb`: check, show and tidy Film Wizard Budgeting files from a terminal.

use std::fmt::Write as _;
use std::process::ExitCode;

use fwb_core::{recalc, starter, Budget, Cents, RecalcOptions, Report};

const USAGE: &str = "\
fwb: Film Wizard Budgeting command line

Usage:
  fwb show <file.fwb> [--exclude GROUP]...   Print the topsheet
  fwb check <file.fwb>                       Read and recalculate; report problems
  fwb fmt <file.fwb>                         Rewrite in canonical form
  fwb new <file.fwb>                         Write a starter budget
  fwb sample <file.fwb>                      Write a large invented test budget (5,000 rows)
  fwb export <file.fwb> --format xlsx|csv|tsv --out <path> [--bom]
      Export the evaluated budget: xlsx is one workbook (Topsheet, Accounts,
      Detail and the setup tables); csv is one file per table, named
      `<stem> - <Table>.csv` from --out (a folder: the budget's file name);
      tsv is the Detail table as tab-delimited text for accounting tools.
      --bom starts CSV and TSV files with a UTF-8 byte order mark (Excel).
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(out) => {
            print!("{out}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("fwb: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<String, String> {
    let (cmd, rest) = args.split_first().ok_or_else(|| USAGE.to_string())?;
    let path = rest.first().ok_or_else(|| USAGE.to_string());
    match cmd.as_str() {
        "show" => {
            let mut opts = RecalcOptions::default();
            let mut it = rest.iter().skip(1);
            while let Some(flag) = it.next() {
                match (flag.as_str(), it.next()) {
                    ("--exclude", Some(g)) => {
                        opts.exclude_groups.insert(g.clone());
                    }
                    _ => return Err(format!("unexpected argument `{flag}`\n\n{USAGE}")),
                }
            }
            let b = load(path?)?;
            let r = recalc(&b, &opts).map_err(|e| e.to_string())?;
            Ok(topsheet(&b, &r))
        }
        "check" => {
            let p = path?;
            let b = load(p)?;
            let r = recalc(&b, &RecalcOptions::default()).map_err(|e| e.to_string())?;
            Ok(format!("{p}: ok, net total {}\n", money(r.net_total)))
        }
        "fmt" => {
            let p = path?;
            let b = load(p)?;
            save(p, &b)?;
            Ok(String::new())
        }
        "new" => {
            let p = path?;
            if std::path::Path::new(p).exists() {
                return Err(format!("{p} already exists"));
            }
            save(p, &starter::budget())?;
            Ok(format!("wrote {p}\n"))
        }
        "sample" => {
            let p = path?;
            if std::path::Path::new(p).exists() {
                return Err(format!("{p} already exists"));
            }
            save(
                p,
                &fwb_core::sample::budget(&fwb_core::sample::SampleSpec::default()),
            )?;
            Ok(format!("wrote {p}\n"))
        }
        "export" => export(rest),
        "help" | "--help" | "-h" => Ok(USAGE.to_string()),
        other => Err(format!("unknown command `{other}`\n\n{USAGE}")),
    }
}

/// `fwb export <file.fwb> --format xlsx|csv|tsv --out <path> [--bom]`.
fn export(rest: &[String]) -> Result<String, String> {
    use fwb_format::export::{self, Format, Options};
    use std::path::{Path, PathBuf};

    let (input, flags) = rest.split_first().ok_or_else(|| USAGE.to_string())?;
    let (mut format, mut out, mut opts) = (None, None, Options::default());
    let mut it = flags.iter();
    while let Some(flag) = it.next() {
        let mut value = || {
            it.next()
                .ok_or_else(|| format!("{flag} needs a value\n\n{USAGE}"))
        };
        match flag.as_str() {
            "--format" => {
                let f = value()?;
                format =
                    Some(Format::parse(f).ok_or_else(|| {
                        format!("unknown export format `{f}`: use xlsx, csv or tsv")
                    })?);
            }
            "--out" => out = Some(PathBuf::from(value()?)),
            "--bom" => opts.bom = true,
            _ => return Err(format!("unexpected argument `{flag}`\n\n{USAGE}")),
        }
    }
    let format = format.ok_or_else(|| format!("export needs --format\n\n{USAGE}"))?;
    let mut out = out.ok_or_else(|| format!("export needs --out\n\n{USAGE}"))?;
    if out.is_dir() {
        // A folder: name the export after the budget's file.
        let stem = Path::new(input)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Budget".into());
        out = out.join(format!("{stem}.{}", format.extension()));
    }

    let b = load(input)?;
    let wb = export::workbook_of(&b).map_err(|e| format!("{input}: {e}"))?;
    let files = export::render(&wb, format, &out, opts).map_err(|e| e.to_string())?;
    // Never write over the budget itself.
    let input_real = std::fs::canonicalize(input).map_err(|e| format!("{input}: {e}"))?;
    for f in &files {
        if std::fs::canonicalize(&f.path).is_ok_and(|p| p == input_real) {
            return Err(format!(
                "{}: that is the budget being exported; choose another --out",
                f.path.display()
            ));
        }
    }
    let mut report = String::new();
    for f in &files {
        std::fs::write(&f.path, &f.bytes).map_err(|e| format!("{}: {e}", f.path.display()))?;
        let _ = writeln!(report, "wrote {}", f.path.display());
    }
    Ok(report)
}

fn load(path: &str) -> Result<Budget, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    fwb_format::from_str(&text).map_err(|e| format!("{path}: {e}"))
}

fn save(path: &str, b: &Budget) -> Result<(), String> {
    std::fs::write(path, fwb_format::to_string(b)).map_err(|e| format!("{path}: {e}"))
}

/// `1234567` cents as `12,345.67`, negatives in parentheses.
fn money(c: Cents) -> String {
    let a = c.unsigned_abs();
    let digits = (a / 100).to_string();
    let mut grouped = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    let s = format!("{grouped}.{:02}", a % 100);
    if c < 0 {
        format!("({s})")
    } else {
        s
    }
}

fn topsheet(b: &Budget, r: &Report) -> String {
    let rule = "-".repeat(66);
    let mut out = format!("{}  ({})\n{rule}\n", b.title, r.base_currency);
    let mut line = |label: &str, amount: Cents| {
        let _ = writeln!(out, "{label:<48}{:>18}", money(amount));
    };

    for level in &r.levels {
        for cat in &r.categories[level.start..level.end] {
            line(&format!("{:<8}{}", cat.code, cat.name), cat.total);
        }
        if level.posted_fringes != 0 {
            line("        Fringes", level.posted_fringes);
        }
        if !level.name.is_empty() {
            line(&format!("        {}", level.name), level.total);
        }
    }
    line("Total above-the-line", r.total_atl);
    line("Total below-the-line", r.total_btl);
    line("Total above and below-the-line", r.atl_btl);
    if r.budget_fringes != 0 {
        line("Total fringes", r.budget_fringes);
    }
    for c in &r.charges {
        line(&c.name, c.amount);
    }
    line("Grand total", r.grand_total);
    for c in &r.credits {
        line(&format!("Less {}", c.name), -c.amount);
    }
    if !r.credits.is_empty() {
        line("Net total", r.net_total);
    }
    if r.totals_currency != r.base_currency {
        if let Some(n) = r.in_totals_currency(r.net_total) {
            line(&format!("Net total in {}", r.totals_currency), n);
        }
    }
    out.push_str(&rule);
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_formats() {
        assert_eq!(money(0), "0.00");
        assert_eq!(money(5), "0.05");
        assert_eq!(money(123_456_789), "1,234,567.89");
        assert_eq!(money(-100_000), "(1,000.00)");
        assert_eq!(money(99_999), "999.99");
    }

    #[test]
    fn starter_budget_recalcs_and_shows() {
        let b = starter::budget();
        let r = recalc(&b, &RecalcOptions::default()).unwrap();
        assert!(r.net_total > 0);
        let text = topsheet(&b, &r);
        assert!(text.contains("Grand total"));
        assert!(text.contains("Total above-the-line"));
        // Round trips through the file format unchanged.
        assert_eq!(fwb_format::from_str(&fwb_format::to_string(&b)).unwrap(), b);
    }

    #[test]
    fn export_writes_each_format() {
        let dir = std::env::temp_dir().join(format!("fwb-cli-export-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let input = dir.join("Film.fwb");
        let input_s = input.to_string_lossy().into_owned();
        save(&input_s, &starter::budget()).unwrap();
        let args = |rest: &[&str]| -> Vec<String> {
            ["export", input_s.as_str()]
                .iter()
                .chain(rest)
                .map(|s| s.to_string())
                .collect()
        };
        let dir_s = dir.to_string_lossy().into_owned();

        // A folder: named after the budget's file.
        let out = run(&args(&["--format", "xlsx", "--out", &dir_s])).unwrap();
        assert!(out.contains("Film.xlsx"), "{out}");
        let bytes = std::fs::read(dir.join("Film.xlsx")).unwrap();
        fwb_format::export::xlsx::verify(&bytes, 10).unwrap();

        let csv_out = dir.join("Out.csv").to_string_lossy().into_owned();
        let out = run(&args(&["--bom", "--format", "csv", "--out", &csv_out])).unwrap();
        assert_eq!(out.lines().count(), 10, "{out}");
        let top = std::fs::read(dir.join("Out - Topsheet.csv")).unwrap();
        assert!(top.starts_with(b"\xEF\xBB\xBFType,Code,Description,Total\r\n"));

        let tsv_out = dir.join("Detail.txt").to_string_lossy().into_owned();
        run(&args(&["--format", "tsv", "--out", &tsv_out])).unwrap();
        let tsv = std::fs::read_to_string(dir.join("Detail.txt")).unwrap();
        assert!(tsv.starts_with("Category\tAccount\tAccount Name\tLine\t"));

        // Never over the budget itself; flags are checked.
        let e = run(&args(&["--format", "xlsx", "--out", &input_s])).unwrap_err();
        assert!(e.contains("budget being exported"), "{e}");
        assert!(load(&input_s).is_ok());
        assert!(run(&args(&["--format", "xls", "--out", &tsv_out]))
            .unwrap_err()
            .contains("unknown export format"));
        assert!(run(&args(&["--out", &tsv_out]))
            .unwrap_err()
            .contains("needs --format"));
        assert!(run(&args(&["--format"]))
            .unwrap_err()
            .contains("needs a value"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn usage_errors() {
        assert!(run(&[]).is_err());
        assert!(run(&["nope".into()])
            .unwrap_err()
            .contains("unknown command"));
        assert!(run(&["show".into()]).is_err());
    }
}
