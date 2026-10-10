//! `method show`: print the topsheet.

use std::fmt::Write as _;

use fwb_core::{recalc, Budget, Cents, RecalcOptions, Report};

use crate::cli::{load, money, CliError, Flags, Outcome};

pub const USAGE: &str = "  method show <file.mthd> [--exclude GROUP]...
      Print the topsheet. --exclude leaves a group's lines out (repeatable).
";

pub fn run(args: &[String]) -> Outcome {
    let f = Flags::parse(args, &["--exclude"], &[], USAGE)?;
    let path = &f.exactly(1, "the budget file", USAGE)?[0];
    let mut opts = RecalcOptions::default();
    for g in f.values("--exclude") {
        opts.exclude_groups.insert(g.to_string());
    }
    let b = load(path)?;
    let r = recalc(&b, &opts).map_err(|e| CliError::failed(format!("{path}: {e}")))?;
    Ok(topsheet(&b, &r))
}

pub fn topsheet(b: &Budget, r: &Report) -> String {
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
    use fwb_core::starter;

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
}
