//! `method-budget show`: print the topsheet.

use std::fmt::Write as _;

use method_core::{recalc, Budget, Cents, Report, TopsheetLine};

use crate::cli::{excluding, load, money, CliError, Flags, Outcome};

pub const USAGE: &str = "  method-budget show <file.mthd> [--exclude GROUP]...
      Print the topsheet. --exclude leaves a group's lines out (repeatable).
";

pub fn run(args: &[String]) -> Outcome {
    let f = Flags::parse(args, &["--exclude"], &[], USAGE)?;
    let path = &f.exactly(1, "the budget file", USAGE)?[0];
    let opts = excluding(&f);
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

    // In the topsheet's order (decisions.md, entry 60): charges newest
    // first, credits in reverse application order.
    for l in r.topsheet_lines(b) {
        match l {
            TopsheetLine::Category(i) => {
                let cat = &r.categories[i];
                line(&format!("{:<8}{}", cat.code, cat.name), cat.total);
            }
            TopsheetLine::LevelFringes(i) => line("        Fringes", r.levels[i].posted_fringes),
            TopsheetLine::Level(i) => {
                let level = &r.levels[i];
                line(&format!("        {}", level.name), level.total);
            }
            TopsheetLine::Atl => line("Total above-the-line", r.total_atl),
            TopsheetLine::Btl => line("Total below-the-line", r.total_btl),
            TopsheetLine::AtlBtl => line("Total above and below-the-line", r.atl_btl),
            TopsheetLine::BudgetFringes => {
                let label = match b.fringe_row.account() {
                    Some(code) => format!("{code:<8}{}", b.fringe_row.description()),
                    None => b.fringe_row.description().to_string(),
                };
                line(&label, r.budget_fringes);
            }
            TopsheetLine::Charge(i) => line(&r.charges[i].name, r.charges[i].amount),
            TopsheetLine::Grand => line("Grand total", r.grand_total),
            TopsheetLine::Credit(i) => {
                line(&format!("Less {}", r.credits[i].name), -r.credits[i].amount)
            }
            TopsheetLine::CreditsSubtotal => line("Sub Total", -r.credits_total),
            TopsheetLine::Net if !r.credits.is_empty() => line("Net total", r.net_total),
            TopsheetLine::Net => {}
        }
    }
    // Converted account by account (decisions.md, entry 60), written in
    // that currency's own separators and symbol (entry 61).
    if let Some(shown) = &r.display {
        let text = match b.currencies.get(&r.totals_currency) {
            Some(c) => c.amount_text(shown.net_total),
            None => money(shown.net_total),
        };
        let label = format!("Net total in {}", r.totals_currency);
        let _ = writeln!(out, "{label:<48}{text:>18}");
    }
    out.push_str(&rule);
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use method_core::{starter, RecalcOptions};

    #[test]
    fn starter_budget_recalcs_and_shows() {
        let b = starter::budget();
        let r = recalc(&b, &RecalcOptions::default()).unwrap();
        assert!(r.net_total > 0);
        let text = topsheet(&b, &r);
        assert!(text.contains("Grand total"));
        assert!(text.contains("Total above-the-line"));
        // Round trips through the file format unchanged.
        assert_eq!(
            method_format::from_str(&method_format::to_string(&b)).unwrap(),
            b
        );
    }

    /// Totals shown in another currency are written in its own separators
    /// and symbol (decisions.md, entry 61).
    #[test]
    fn the_totals_currency_line_uses_its_format() {
        let mut b = starter::budget();
        b.currencies.insert(
            "EUR".into(),
            method_core::Currency {
                description: "Euro".into(),
                symbol: "\u{20ac}".into(),
                rate: "2".parse().unwrap(),
                format: method_core::CurrencyFormat {
                    symbol_after: true,
                    separators: method_core::Separators::PointComma,
                },
            },
        );
        b.totals_currency = Some("EUR".into());
        let r = recalc(&b, &RecalcOptions::default()).unwrap();
        let shown = r.display.as_ref().unwrap().net_total;
        let text = topsheet(&b, &r);
        let want = b.currencies["EUR"].amount_text(shown);
        assert!(want.ends_with(" \u{20ac}") && want.contains(','), "{want}");
        assert!(text.contains("Net total in EUR"), "{text}");
        assert!(text.contains(&want), "{text}");
    }
}
