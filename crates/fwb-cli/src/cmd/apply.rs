//! `fwb apply`: run an edit script against a budget (docs/CLI.md, "Edit
//! scripts").

use crate::cli::{CliError, Flags, Outcome};
use crate::script;

pub const USAGE: &str = "  fwb apply <file.fwb> <script.txt> [--dry-run]
      Run an edit script (one command per line; `-` reads stdin) and save
      atomically. Every line must succeed and the result must recalculate, or
      nothing is written (exit 3). --dry-run prints each change and the totals
      before and after, and writes nothing.
";

pub fn run(args: &[String]) -> Outcome {
    let f = Flags::parse(args, &[], &["--dry-run"], USAGE)?;
    let p = f.exactly(2, "the budget file and the script", USAGE)?;
    let (budget, script_path) = (&p[0], &p[1]);
    let text = if script_path == "-" {
        let mut s = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut s)
            .map_err(|e| CliError::failed(format!("stdin: {e}")))?;
        s
    } else {
        std::fs::read_to_string(script_path)
            .map_err(|e| CliError::failed(format!("{script_path}: {e}")))?
    };
    let name = if script_path == "-" {
        "stdin"
    } else {
        script_path
    };
    let mut ops = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let label = format!("{name} line {}", i + 1);
        match script::parse(line) {
            Ok(Some(op)) => ops.push((label, op)),
            Ok(None) => {}
            Err(e) => return Err(CliError::rejected(format!("{label}: {e}"))),
        }
    }
    script::run(budget, &ops, f.has("--dry-run"))
}
