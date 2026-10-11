//! `method check`: read and recalculate a budget, reporting problems.

use method_core::{recalc, warnings, RecalcOptions};

use crate::cli::{load, money, CliError, Flags, Outcome};

pub const USAGE: &str = "  method check <file.mthd>
      Read and recalculate; report problems, then any warnings (totals that
      leave something out, such as fringes after the last production total).
";

pub fn run(args: &[String]) -> Outcome {
    let f = Flags::parse(args, &[], &[], USAGE)?;
    let p = &f.exactly(1, "the budget file", USAGE)?[0];
    let b = load(p)?;
    let r =
        recalc(&b, &RecalcOptions::default()).map_err(|e| CliError::failed(format!("{p}: {e}")))?;
    let mut out = format!("{p}: ok, net total {}\n", money(r.net_total));
    for w in warnings(&b, &r) {
        out.push_str(&format!("{p}: warning: {w}\n"));
    }
    Ok(out)
}
