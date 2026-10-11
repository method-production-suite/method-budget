//! `method new`: write a starter budget.

use method_core::starter;

use crate::cli::{save, CliError, Flags, Outcome};

pub const USAGE: &str = "  method new <file.mthd> [--template NAME]
      Write a starter budget, or with --template a new budget with one of the
      bundled charts of accounts and no lines (`method coa list` names them).
";

pub fn run(args: &[String]) -> Outcome {
    let f = Flags::parse(args, &["--template"], &[], USAGE)?;
    let p = &f.exactly(1, "the budget file", USAGE)?[0];
    if std::path::Path::new(p).exists() {
        return Err(CliError::failed(format!("{p} already exists")));
    }
    let b = match f.value("--template") {
        None => starter::budget(),
        Some(name) => {
            let chart = method_format::charts::find(name).ok_or_else(|| {
                let ids: Vec<&str> = method_format::charts::all().iter().map(|c| c.id).collect();
                CliError::failed(format!(
                    "no bundled chart named `{name}` (there are: {})",
                    ids.join(", ")
                ))
            })?;
            let mut b = chart
                .budget()
                .map_err(|e| CliError::failed(format!("the bundled chart {}: {e}", chart.id)))?;
            b.title = "Untitled".into();
            b
        }
    };
    save(p, &b)?;
    Ok(format!("wrote {p}\n"))
}
