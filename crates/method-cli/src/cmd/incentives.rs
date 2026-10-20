//! `method-budget incentives`: spend by location and every credit's estimate,
//! applied or not (roadmap 3.6; docs/decisions.md, entries 80 and 88).

use std::fmt::Write as _;

use method_core::incentives::{incentives, Incentives};
use method_core::{recalc, RecalcOptions};
use method_format::export::money_text;
use method_session::report_dto::IncentivesDto;

use crate::cli::{load, CliError, Flags, Outcome};

pub const USAGE: &str = "  method-budget incentives <file.mthd> [--json]
      Print the spend at each location (`location<TAB>name<TAB>lines<TAB>
      fringes<TAB>total`, `(none)` for lines with no location), then every
      credit as an incentive estimate, applied or not (`program<TAB>name<TAB>
      applied|estimate<TAB>qualified spend<TAB>credit<TAB>locations`, then
      `<TAB>note` when the credit is below its minimum spend, held to a
      shared cap or less a broker's fee). --json prints the same as JSON,
      money in integer cents.
";

pub fn run(args: &[String]) -> Outcome {
    let f = Flags::parse(args, &[], &["--json"], USAGE)?;
    let path = &f.exactly(1, "the budget file", USAGE)?[0];
    let b = load(path)?;
    let r = recalc(&b, &RecalcOptions::default())
        .map_err(|e| CliError::failed(format!("{path}: {e}")))?;
    let got = incentives(&b, &r).map_err(|e| CliError::failed(format!("{path}: {e}")))?;
    Ok(if f.has("--json") {
        let mut s = method_format::serde_json::to_string_pretty(&IncentivesDto::from(&got))
            .map_err(|e| CliError::failed(format!("{path}: {e}")))?;
        s.push('\n');
        s
    } else {
        text(&got)
    })
}

/// Tab-separated text: the locations, then the programs.
pub fn text(got: &Incentives) -> String {
    let mut out = String::new();
    for l in &got.locations {
        let _ = writeln!(
            out,
            "location\t{}\t{}\t{}\t{}",
            l.location.as_deref().unwrap_or("(none)"),
            money_text(l.lines),
            money_text(l.fringes),
            money_text(l.total)
        );
    }
    for p in &got.programs {
        let state = if p.applied { "applied" } else { "estimate" };
        let places = p.locations.iter().cloned().collect::<Vec<_>>().join(", ");
        match (p.amount(), &p.problem) {
            (Some(amount), _) => {
                let _ = write!(
                    out,
                    "program\t{}\t{state}\t{}\t{}\t{places}",
                    p.name,
                    money_text(p.qualified),
                    money_text(amount)
                );
                let note = note(p);
                if !note.is_empty() {
                    let _ = write!(out, "\t{note}");
                }
                out.push('\n');
            }
            (None, problem) => {
                let _ = writeln!(
                    out,
                    "program\t{}\tproblem\t\t\t{places}\t{}",
                    p.name,
                    problem.as_deref().unwrap_or("")
                );
            }
        }
    }
    out
}

/// Why a program's figure is what it is, beyond its rows and its own cap
/// (decisions.md, entry 88): below its minimum, held to a shared cap, less
/// a broker's fee. Empty when none applies.
pub fn note(p: &method_core::incentives::Program) -> String {
    let Some(f) = &p.figures else {
        return String::new();
    };
    let mut parts = Vec::new();
    if f.below_minimum {
        if let Some(m) = p.minimum_spend {
            parts.push(format!("below the minimum spend of {}", money_text(m)));
        }
    }
    if let (Some(share), Some(name)) = (f.shared, &p.shared_cap) {
        let of = p
            .shared_cap_amount
            .map(|a| format!(" of {}", money_text(a)))
            .unwrap_or_default();
        parts.push(format!("shared cap {name}: {}{of}", money_text(share)));
    }
    if f.fee != 0 {
        parts.push(format!("less a {} broker's fee", money_text(f.fee)));
    }
    parts.join("; ")
}
