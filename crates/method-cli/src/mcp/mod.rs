//! `method-budget mcp`: a Model Context Protocol server over stdio, so coding agents
//! can read and edit budget files with typed tools instead of shell commands
//! (docs/MCP.md).
//!
//! It works on files, not on the running app: every tool names a budget by
//! its path, reads it from disk and writes it back atomically. The server
//! keeps a small amount of state per file (the bytes it last read or wrote,
//! for conflict detection, and the undo steps of its own changes), keyed by
//! the file's real path, which is the explicit handle the protocol's
//! stateless revision asks for.
//!
//! - [`protocol`]: JSON-RPC 2.0 over newline-delimited stdio, the
//!   `initialize` handshake (revisions 2024-11-05 to 2025-11-25) and the
//!   per-request `_meta` of revision 2026-07-28, cancellation.
//! - [`paths`]: every path resolves inside an allowed root; symlinks that
//!   leave it, hidden folders and unexpected extensions are refused.
//! - [`docs`]: open files, conflict detection, atomic writes, undo.
//! - [`schema`]: the JSON Schema subset tool inputs are written in, and the
//!   validator every call's arguments go through.
//! - [`tools`]: the tools themselves, thin layers over `method-core`,
//!   `method-format` and the command line's own edit language.
//!
//! Nothing here runs a program or opens a network connection. Logs go to
//! stderr; stdout carries protocol messages only.

pub mod docs;
pub mod paths;
pub mod protocol;
pub mod schema;
pub mod tools;

#[cfg(test)]
mod tests;

use std::path::PathBuf;

/// How the server was started.
#[derive(Clone, Debug)]
pub struct Config {
    /// Folders the server may read and write in (at least one).
    pub roots: Vec<PathBuf>,
    /// Hide every tool that writes a file.
    pub read_only: bool,
}

/// Limits on what the server accepts, so a confused or hostile client can't
/// make it use unbounded memory or time.
pub mod limits {
    /// Longest protocol message (one line of stdin), in bytes.
    pub const MAX_MESSAGE_BYTES: usize = 8 * 1024 * 1024;
    /// Largest budget file the server reads or writes, in bytes.
    pub const MAX_FILE_BYTES: u64 = 128 * 1024 * 1024;
    /// Most rows one batch tool call may add, change, delete or move.
    pub const MAX_BATCH: usize = 5_000;
    /// Longest description, note or other free text, in characters.
    pub const MAX_TEXT: usize = 10_000;
    /// Longest name or code (a fringe, an account number...), in characters.
    pub const MAX_NAME: usize = 200;
    /// Longest formula, in characters (the engine's own limit).
    pub const MAX_FORMULA: usize = method_core::expr::MAX_FORMULA_LEN;
    /// Longest path argument, in bytes.
    pub const MAX_PATH: usize = 4_096;
    /// Budget files kept open at once; the least recently used is closed
    /// (and its undo steps forgotten) past this.
    pub const MAX_OPEN_FILES: usize = 32;
    /// Undo steps kept per file.
    pub const MAX_UNDO_STEPS: usize = 50;
    /// Bytes of undo steps kept per file; the oldest go first past this.
    pub const MAX_UNDO_BYTES: usize = 256 * 1024 * 1024;
    /// Deepest explanation tree returned, and the steps in it.
    pub const MAX_EXPLAIN_DEPTH: u64 = 16;
    pub const MAX_EXPLAIN_STEPS: usize = 2_000;
    /// Most results a search returns.
    pub const MAX_FIND: u64 = 1_000;
    /// Most rows one `get_account` call returns.
    pub const MAX_ROWS_PAGE: u64 = 5_000;
    /// Most changes a diff lists in full.
    pub const MAX_DIFF_ITEMS: usize = 1_000;
    /// Most files `list_budgets` returns, and how deep it looks.
    pub const MAX_LISTED_FILES: usize = 2_000;
    pub const MAX_LIST_DEPTH: usize = 8;
}

/// Runs the server on this process's stdin and stdout until stdin closes.
pub fn serve_stdio(config: Config) -> Result<(), String> {
    let server = protocol::Server::new(&config)?;
    eprintln!(
        "method-budget mcp: serving {} tool(s) over stdio{}; roots: {}",
        server.tool_count(),
        if config.read_only { " (read-only)" } else { "" },
        server
            .roots()
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
    let source = protocol::spawn_reader(std::io::stdin());
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    protocol::run(server, source, &mut out).map_err(|e| format!("stdout: {e}"))
}
