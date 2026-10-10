//! `fwb`: the command line's name from before the product was renamed to
//! Method Budget. Deprecated: it runs exactly what `method` runs (same
//! commands, output and exit codes) so existing scripts and git merge driver
//! settings keep working. Use `method`; this alias will be removed in a later
//! release (docs/CLI.md).

#[path = "../main.rs"]
mod method;

// The modules' own `crate::cli` (and so on) paths resolve through these.
use method::{cli, render, script};

fn main() -> std::process::ExitCode {
    method::main()
}
