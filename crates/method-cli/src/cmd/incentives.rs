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
  method-budget incentives what-if <file.mthd> (--to-location NAME | --no-location)
                [--lines ROWS] [--account CODE] [--category CODE]
                [--from-location NAME | --from-no-location] [--except ROWS]
                [--json] [--apply]
      What moving lines to another location would do to every program,
      without changing the file: the lines (`line<TAB>#ID<TAB>ACCOUNT:N<TAB>
      description<TAB>location now<TAB>total`), each program
      (`program<TAB>name<TAB>applied|estimate<TAB>before<TAB>after<TAB>
      difference`), the spend at each location (`location<TAB>name<TAB>
      before<TAB>after`), then the credits taken off, the estimates and the
      net total (`total<TAB>credits|estimates|net<TAB>before<TAB>after`).
      ROWS are #ID, ACCOUNT:N or ACCOUNT:* (comma-separated); --account
      and --category take every line in them (each repeatable);
      --from-location keeps only the lines there now. --apply moves the
      lines in the file (saved atomically) and prints `moved<TAB>N` last.
      --json prints the figures as JSON, money in integer cents.
";

pub fn run(args: &[String]) -> Outcome {
    if args.first().map(String::as_str) == Some("what-if") {
        return what_if(&args[1..]);
    }
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

/// `method-budget incentives what-if`: every program before and after
/// moving lines to a location (decisions.md, entry 93).
fn what_if(args: &[String]) -> Outcome {
    use method_core::incentives::what_if::{move_lines, pick_lines, what_if, LinePick};
    let f = Flags::parse(
        args,
        &[
            "--to-location",
            "--from-location",
            "--lines",
            "--account",
            "--category",
            "--except",
        ],
        &["--no-location", "--from-no-location", "--json", "--apply"],
        USAGE,
    )?;
    let path = &f.exactly(1, "the budget file", USAGE)?[0];
    let mut b = load(path)?;
    let to = match (f.value("--to-location"), f.has("--no-location")) {
        (Some(n), false) => Some(n.to_string()),
        (None, true) => None,
        (Some(_), true) => {
            return Err(CliError::usage(
                "give --to-location or --no-location, not both",
                USAGE,
            ))
        }
        (None, false) => {
            return Err(CliError::usage(
                "missing --to-location (or --no-location)",
                USAGE,
            ))
        }
    };
    let from = match (f.value("--from-location"), f.has("--from-no-location")) {
        (Some(n), false) => Some(Some(n.to_string())),
        (None, true) => Some(None),
        (None, false) => None,
        (Some(_), true) => {
            return Err(CliError::usage(
                "give --from-location or --from-no-location, not both",
                USAGE,
            ))
        }
    };
    let rows_of = |flag: &str| -> Result<Vec<method_core::RowId>, CliError> {
        let mut ids = Vec::new();
        for list in f.values(flag) {
            for word in list.split(',').map(str::trim).filter(|w| !w.is_empty()) {
                let r =
                    crate::script::RowRef::parse(word).map_err(|e| CliError::usage(e, USAGE))?;
                ids.extend(
                    crate::script::resolve(&b, &r)
                        .map_err(|e| CliError::rejected(format!("{path}: {e}")))?,
                );
            }
        }
        Ok(ids)
    };
    let pick = LinePick {
        rows: rows_of("--lines")?,
        accounts: f
            .values("--account")
            .into_iter()
            .map(str::to_string)
            .collect(),
        categories: f
            .values("--category")
            .into_iter()
            .map(str::to_string)
            .collect(),
        from,
        except: rows_of("--except")?,
    };
    let refused = |e: method_core::incentives::what_if::WhatIfError| {
        CliError::rejected(format!("{path}: {e}"))
    };
    let rows = pick_lines(&b, &pick).map_err(refused)?;
    let w = what_if(&b, &rows, to.as_deref()).map_err(refused)?;
    let r = recalc(&b, &RecalcOptions::default())
        .map_err(|e| CliError::failed(format!("{path}: {e}")))?;
    let dto = method_session::what_if::dto(&b, &r, &w, false);
    let mut out = if f.has("--json") {
        let mut s = method_format::serde_json::to_string_pretty(&dto)
            .map_err(|e| CliError::failed(format!("{path}: {e}")))?;
        s.push('\n');
        s
    } else {
        what_if_text(&dto)
    };
    if f.has("--apply") {
        let n = move_lines(&mut b, &rows, to.as_deref()).map_err(refused)?;
        crate::cli::save(path, &b)?;
        if !f.has("--json") {
            let _ = writeln!(out, "moved\t{n}");
        }
    }
    Ok(out)
}

/// Tab-separated text: the lines, the programs, the locations, the totals.
fn what_if_text(w: &method_session::what_if::WhatIfDto) -> String {
    let mut out = String::new();
    let opt = |c: Option<method_core::Cents>| c.map(money_text).unwrap_or_default();
    for l in &w.lines {
        let _ = writeln!(
            out,
            "line\t#{}\t{}:{}\t{}\t{}\t{}",
            l.id,
            l.account,
            l.line,
            l.description,
            l.location.as_deref().unwrap_or("(none)"),
            money_text(l.total)
        );
    }
    for p in &w.programs {
        let state = if p.applied { "applied" } else { "estimate" };
        let _ = write!(
            out,
            "program\t{}\t{state}\t{}\t{}\t{}",
            p.name,
            opt(p.before),
            opt(p.after),
            opt(p.difference)
        );
        if let Some(problem) = &p.problem {
            let _ = write!(out, "\t{problem}");
        }
        out.push('\n');
    }
    for l in &w.locations {
        let _ = writeln!(
            out,
            "location\t{}\t{}\t{}",
            l.location.as_deref().unwrap_or("(none)"),
            money_text(l.before),
            money_text(l.after)
        );
    }
    for (what, a, z) in [
        ("credits", w.before.credits_total, w.after.credits_total),
        (
            "estimates",
            w.before.estimates_total,
            w.after.estimates_total,
        ),
        ("net", w.before.net_total, w.after.net_total),
    ] {
        let _ = writeln!(out, "total\t{what}\t{}\t{}", money_text(a), money_text(z));
    }
    out
}
