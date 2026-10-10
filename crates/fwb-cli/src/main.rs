//! `fwb`: check, show, tidy, export and script Film Wizard Budgeting files from
//! a terminal. Every subcommand is documented in docs/CLI.md.
//!
//! Adding a subcommand: write `src/cmd/<name>.rs` with `pub const USAGE` and
//! `pub fn run(args: &[String]) -> Outcome` (see `src/cli.rs`), add one
//! `pub mod` line to `cmd` below and one entry to `COMMANDS`. Keep each
//! command's entry on its own lines so parallel edits don't collide.

mod cli;
mod render;
mod script;

mod cmd {
    pub mod apply;
    pub mod check;
    pub mod diff;
    pub mod explain;
    pub mod export;
    pub mod fmt;
    pub mod import;
    pub mod merge;
    pub mod merge_driver;
    pub mod new;
    pub mod report;
    pub mod sample;
    pub mod set;
    pub mod show;
    pub mod totals;
}

use std::process::ExitCode;

use cli::{exit, CliError, Outcome};

/// A subcommand: its name, its lines of the help text, and what runs it.
struct Command {
    name: &'static str,
    usage: &'static str,
    run: fn(&[String]) -> Outcome,
}

/// Every subcommand, in the order `fwb help` lists them.
const COMMANDS: &[Command] = &[
    Command {
        name: "show",
        usage: cmd::show::USAGE,
        run: cmd::show::run,
    },
    Command {
        name: "check",
        usage: cmd::check::USAGE,
        run: cmd::check::run,
    },
    Command {
        name: "fmt",
        usage: cmd::fmt::USAGE,
        run: cmd::fmt::run,
    },
    Command {
        name: "new",
        usage: cmd::new::USAGE,
        run: cmd::new::run,
    },
    Command {
        name: "sample",
        usage: cmd::sample::USAGE,
        run: cmd::sample::run,
    },
    Command {
        name: "export",
        usage: cmd::export::USAGE,
        run: cmd::export::run,
    },
    Command {
        name: "report",
        usage: cmd::report::USAGE,
        run: cmd::report::run,
    },
    Command {
        name: "totals",
        usage: cmd::totals::USAGE,
        run: cmd::totals::run,
    },
    Command {
        name: "set",
        usage: cmd::set::USAGE,
        run: cmd::set::run,
    },
    Command {
        name: "apply",
        usage: cmd::apply::USAGE,
        run: cmd::apply::run,
    },
    Command {
        name: "import",
        usage: cmd::import::USAGE,
        run: cmd::import::run,
    },
    Command {
        name: "explain",
        usage: cmd::explain::USAGE,
        run: cmd::explain::run,
    },
    Command {
        name: "diff",
        usage: cmd::diff::USAGE,
        run: cmd::diff::run,
    },
    Command {
        name: "merge",
        usage: cmd::merge::USAGE,
        run: cmd::merge::run,
    },
    Command {
        name: "merge-driver",
        usage: cmd::merge_driver::USAGE,
        run: cmd::merge_driver::run,
    },
];

const HEADER: &str = "\
fwb: Film Wizard Budgeting command line

Usage:
";

const FOOTER: &str = "
Exit codes: 0 ok, 1 failed (file, format or calculation problem), 2 usage
error, 3 edit refused (nothing written). See docs/CLI.md.
";

/// The full help text.
fn usage() -> String {
    let mut s = HEADER.to_string();
    for c in COMMANDS {
        s.push_str(c.usage);
    }
    s.push_str("  fwb help\n      Print this help.\n");
    s.push_str(FOOTER);
    s
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(out) => {
            print!("{out}");
            ExitCode::from(exit::OK)
        }
        Err(e) => {
            if !e.message.is_empty() {
                eprintln!("fwb: {}", e.message);
            }
            ExitCode::from(e.code)
        }
    }
}

fn run(args: &[String]) -> Outcome {
    let Some((name, rest)) = args.split_first() else {
        return Err(CliError::new(exit::USAGE, usage()));
    };
    match name.as_str() {
        "help" | "--help" | "-h" => return Ok(usage()),
        _ => {}
    }
    match COMMANDS.iter().find(|c| c.name == name) {
        Some(c) => (c.run)(rest),
        None => Err(CliError::new(
            exit::USAGE,
            format!("unknown command `{name}`\n\n{}", usage()),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_errors() {
        assert_eq!(run(&[]).unwrap_err().code, exit::USAGE);
        let e = run(&["nope".into()]).unwrap_err();
        assert!(e.message.contains("unknown command"));
        assert_eq!(e.code, exit::USAGE);
        assert_eq!(run(&["show".into()]).unwrap_err().code, exit::USAGE);
        assert!(run(&["help".into()]).unwrap().contains("fwb export"));
    }

    #[test]
    fn every_command_is_listed_once() {
        let mut names: Vec<_> = COMMANDS.iter().map(|c| c.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), COMMANDS.len());
        let help = usage();
        for c in COMMANDS {
            assert!(help.contains(&format!("fwb {}", c.name)), "{}", c.name);
        }
    }
}
