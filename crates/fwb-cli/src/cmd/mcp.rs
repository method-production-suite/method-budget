//! `method mcp`: serve budget tools to coding agents over the Model Context
//! Protocol on stdin and stdout (src/mcp/, docs/MCP.md).

use std::path::PathBuf;

use crate::cli::{CliError, Flags, Outcome};
use crate::mcp::{serve_stdio, Config};

pub const USAGE: &str = "  method mcp [--root DIR]... [--read-only]
      Serve budget tools to coding agents (Claude Code and other MCP clients)
      over stdio: open, read, edit, check, diff, merge, export and import
      budgets by path. Every path must be inside a --root folder (repeatable;
      default: the current folder). --read-only hides every tool that writes.
      Logs go to stderr. See docs/MCP.md.
";

pub fn run(args: &[String]) -> Outcome {
    let f = Flags::parse(args, &["--root"], &["--read-only"], USAGE)?;
    f.exactly(0, "", USAGE)?;
    let roots: Vec<PathBuf> = match f.values("--root") {
        v if v.is_empty() => {
            vec![std::env::current_dir().map_err(|e| format!("current folder: {e}"))?]
        }
        v => v.into_iter().map(PathBuf::from).collect(),
    };
    serve_stdio(Config {
        roots,
        read_only: f.has("--read-only"),
    })
    .map_err(CliError::failed)?;
    Ok(String::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::exit;

    #[test]
    fn bad_command_lines_are_usage_errors() {
        let e = run(&["extra".to_string()]).unwrap_err();
        assert_eq!(e.code, exit::USAGE);
        assert!(
            e.message.contains("unexpected argument `extra`"),
            "{}",
            e.message
        );
        let e = run(&["--root".to_string()]).unwrap_err();
        assert_eq!(e.code, exit::USAGE);
        let e = run(&["--root".to_string(), "/no/such/folder/anywhere".to_string()]).unwrap_err();
        assert_eq!(e.code, exit::FAILED);
        assert!(
            e.message.contains("/no/such/folder/anywhere"),
            "{}",
            e.message
        );
    }
}
