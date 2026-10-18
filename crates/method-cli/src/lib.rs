//! `method-budget`: check, show, tidy, export and script Method Budget files
//! from a terminal. Every subcommand is documented in docs/CLI.md.
//!
//! Two binaries share this entry point ([`main`]): `method-budget`
//! (src/main.rs) and `method` (src/bin/method.rs), the old name kept as an
//! alias that adds a one-line note on stderr.
//!
//! Adding a subcommand: write `src/cmd/<name>.rs` with `pub const USAGE` and
//! `pub fn run(args: &[String]) -> Outcome` (see `src/cli.rs`), add one
//! `pub mod` line to `cmd` below and one entry to `COMMANDS`. Keep each
//! command's entry on its own lines so parallel edits don't collide.

#![forbid(unsafe_code)]

mod cli;
mod render;
mod script;
// The MCP server behind `method-budget mcp` (docs/MCP.md).
mod mcp;

mod cmd {
    pub mod apply;
    pub mod check;
    pub mod coa;
    pub mod diff;
    pub mod explain;
    pub mod export;
    pub mod fmt;
    pub mod import;
    pub mod import_export;
    pub mod log;
    pub mod mcp;
    pub mod merge;
    pub mod merge_driver;
    pub mod new;
    pub mod pack;
    pub mod report;
    pub mod sample;
    pub mod schedule_days;
    pub mod set;
    pub mod show;
    pub mod sync;
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

/// Every subcommand, in the order `method-budget help` lists them.
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
        name: "import-export",
        usage: cmd::import_export::USAGE,
        run: cmd::import_export::run,
    },
    Command {
        name: "schedule-days",
        usage: cmd::schedule_days::USAGE,
        run: cmd::schedule_days::run,
    },
    Command {
        name: "coa",
        usage: cmd::coa::USAGE,
        run: cmd::coa::run,
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
    Command {
        name: "pack",
        usage: cmd::pack::USAGE,
        run: cmd::pack::run,
    },
    Command {
        name: "log",
        usage: cmd::log::USAGE,
        run: cmd::log::run,
    },
    Command {
        name: "sync",
        usage: cmd::sync::USAGE,
        run: cmd::sync::run,
    },
    Command {
        name: "mcp",
        usage: cmd::mcp::USAGE,
        run: cmd::mcp::run,
    },
];

const HEADER: &str = "\
method-budget: Method Budget command line

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
    s.push_str("  method-budget help\n      Print this help.\n");
    s.push_str(
        "  method-budget --version\n      Print the version (the app's Help > About shows the same).\n",
    );
    s.push_str(FOOTER);
    s
}

/// `method-budget --version`: the workspace version, the one the app and its
/// releases carry (docs/RELEASING.md).
fn version() -> String {
    format!(
        "method-budget {} (Method Budget)\n",
        env!("CARGO_PKG_VERSION")
    )
}

/// The name a binary was run under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Invocation {
    /// `method-budget`, the command line's name.
    Current,
    /// `method`, the old name, kept for now as an alias.
    Alias,
}

/// The note the `method` alias prints to stderr before it runs.
pub const ALIAS_NOTE: &str =
    "note: `method` is now `method-budget`; the old name will be removed in a future release.";

/// Subcommands the alias runs without its note: tools wired into other
/// programs' configs (MCP clients, git) where stderr noise does harm.
const QUIET_UNDER_ALIAS: &[&str] = &["mcp", "merge-driver"];

/// Whether the alias should print [`ALIAS_NOTE`] for these arguments.
fn alias_note_wanted(args: &[String]) -> bool {
    !matches!(args.first(), Some(name) if QUIET_UNDER_ALIAS.contains(&name.as_str()))
}

/// Runs the command line with the process's arguments: what both binaries'
/// `main` does.
pub fn main(invocation: Invocation) -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if invocation == Invocation::Alias && alias_note_wanted(&args) {
        eprintln!("{ALIAS_NOTE}");
    }
    match run(&args) {
        Ok(out) => {
            print!("{out}");
            ExitCode::from(exit::OK)
        }
        Err(e) => {
            if !e.message.is_empty() {
                eprintln!("method-budget: {}", e.message);
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
        "--version" | "-V" | "version" => return Ok(version()),
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
        assert!(run(&["help".into()])
            .unwrap()
            .contains("method-budget export"));
    }

    #[test]
    fn version() {
        let want = format!(
            "method-budget {} (Method Budget)\n",
            env!("CARGO_PKG_VERSION")
        );
        for flag in ["--version", "-V", "version"] {
            assert_eq!(run(&[flag.into()]).unwrap(), want);
        }
        assert!(usage().contains("method-budget --version"));
    }

    #[test]
    fn alias_note_is_skipped_for_wired_in_tools() {
        let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(alias_note_wanted(&args(&[])));
        assert!(alias_note_wanted(&args(&["show", "f.mthd"])));
        assert!(alias_note_wanted(&args(&["--version"])));
        assert!(!alias_note_wanted(&args(&["mcp"])));
        assert!(!alias_note_wanted(&args(&["mcp", "--read-only"])));
        assert!(!alias_note_wanted(&args(&["merge-driver", "a", "b", "c"])));
    }

    #[test]
    fn every_command_is_listed_once() {
        let mut names: Vec<_> = COMMANDS.iter().map(|c| c.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), COMMANDS.len());
        let help = usage();
        for c in COMMANDS {
            assert!(
                help.contains(&format!("method-budget {}", c.name)),
                "{}",
                c.name
            );
        }
    }
}
