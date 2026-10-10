//! `fwb sample`: write a large invented test budget.

use crate::cli::{save, CliError, Flags, Outcome};

pub const USAGE: &str = "  fwb sample <file.fwb>
      Write a large invented test budget (5,000 rows).
";

pub fn run(args: &[String]) -> Outcome {
    let f = Flags::parse(args, &[], &[], USAGE)?;
    let p = &f.exactly(1, "the budget file", USAGE)?[0];
    if std::path::Path::new(p).exists() {
        return Err(CliError::failed(format!("{p} already exists")));
    }
    save(
        p,
        &fwb_core::sample::budget(&fwb_core::sample::SampleSpec::default()),
    )?;
    Ok(format!("wrote {p}\n"))
}
