//! `fwb new`: write a starter budget.

use fwb_core::starter;

use crate::cli::{save, CliError, Flags, Outcome};

pub const USAGE: &str = "  fwb new <file.fwb>
      Write a starter budget.
";

pub fn run(args: &[String]) -> Outcome {
    let f = Flags::parse(args, &[], &[], USAGE)?;
    let p = &f.exactly(1, "the budget file", USAGE)?[0];
    if std::path::Path::new(p).exists() {
        return Err(CliError::failed(format!("{p} already exists")));
    }
    save(p, &starter::budget())?;
    Ok(format!("wrote {p}\n"))
}
