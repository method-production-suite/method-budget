//! `fwb`: check, show and tidy Film Wizard Budgeting files from a terminal.

use std::fmt::Write as _;
use std::process::ExitCode;

use fwb_core::{recalc, Budget, Cents, RecalcOptions, Report};

mod starter;

const USAGE: &str = "\
fwb: Film Wizard Budgeting command line

Usage:
  fwb show <file.fwb> [--exclude GROUP]...   Print the topsheet
  fwb check <file.fwb>                       Read and recalculate; report problems
  fwb fmt <file.fwb>                         Rewrite in canonical form
  fwb new <file.fwb>                         Write a starter budget
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
            let mut it = rest[1..].iter();
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
        "help" | "--help" | "-h" => Ok(USAGE.to_string()),
        other => Err(format!("unknown command `{other}`\n\n{USAGE}")),
    }
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
        if i > 0 && (digits.len() - i) % 3 == 0 {
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
    fn usage_errors() {
        assert!(run(&[]).is_err());
        assert!(run(&["nope".into()])
            .unwrap_err()
            .contains("unknown command"));
        assert!(run(&["show".into()]).is_err());
    }
}
