//! `method check`: read and recalculate a budget, reporting problems.

use fwb_core::{recalc, RecalcOptions};

use crate::cli::{load, money, CliError, Flags, Outcome};

pub const USAGE: &str = "  method check <file.mthd>
      Read and recalculate; report problems.
";

pub fn run(args: &[String]) -> Outcome {
    let f = Flags::parse(args, &[], &[], USAGE)?;
    let p = &f.exactly(1, "the budget file", USAGE)?[0];
    let b = load(p)?;
    let r =
        recalc(&b, &RecalcOptions::default()).map_err(|e| CliError::failed(format!("{p}: {e}")))?;
    Ok(format!("{p}: ok, net total {}\n", money(r.net_total)))
}
