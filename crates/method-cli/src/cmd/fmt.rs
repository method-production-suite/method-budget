//! `method fmt`: rewrite a budget in canonical form.

use crate::cli::{load, save, Flags, Outcome};

pub const USAGE: &str = "  method fmt <file.mthd>
      Rewrite in canonical form (atomically).
";

pub fn run(args: &[String]) -> Outcome {
    let f = Flags::parse(args, &[], &[], USAGE)?;
    let p = &f.exactly(1, "the budget file", USAGE)?[0];
    let b = load(p)?;
    save(p, &b)?;
    Ok(String::new())
}
