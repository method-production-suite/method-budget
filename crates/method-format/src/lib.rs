//! Method Budget native file format.
//!
//! A budget is stored as pretty-printed JSON (2-space indent, trailing newline)
//! with a fixed key order, so files diff cleanly and the same budget always
//! produces the same bytes. The format is open and documented in `docs/FORMAT.md`.
//!
//! ```
//! let b = method_core::Budget::new("My Film", "USD");
//! let text = method_format::to_string(&b);
//! assert_eq!(method_format::from_str(&text).unwrap(), b);
//! ```

/// The bundled chart-of-accounts templates (templates/charts, docs/TEMPLATES.md).
pub mod charts;
/// Version deltas: versions written as changes (`compact_versions`).
mod delta;
mod dto;
/// Spreadsheet export (Excel workbook, CSV, tab-delimited). Not the native
/// format: a one-way copy of the evaluated budget for other tools.
pub mod export;
/// Files on disk: atomic replacement (temporary file, flush, rename) and
/// whole-file reads with a size cap.
pub mod fsio;
/// Spreadsheet import (Excel workbook, CSV, tab-delimited) with a column
/// mapping. Read by hand: our own inflate, ZIP and XML readers.
pub mod import;
/// The change log on disk beside a shared budget (docs/FORMAT.md, "The
/// change log").
pub mod log;
/// Setup items between budgets (dependency walk, conflict review, import),
/// setup packs, and setup lists as spreadsheets.
pub mod setup;

/// The JSON library the format is read and written with, for the
/// workspace's tools that speak JSON (the `method mcp` server in
/// `method-cli`), so they share this copy instead of adding a dependency.
pub use serde_json;

use std::fmt;

use method_core::Budget;

use dto::{FileBudget, FileDoc, Header};

/// The value of the top-level `"format"` field: the only one this build
/// writes or reads. See docs/FORMAT.md ("Identity").
pub const FORMAT_NAME: &str = "method-budget";

/// The native file extension, without the dot. Budgets are saved as `.mthd`.
pub const EXTENSION: &str = "mthd";

/// Every extension a budget file may have (for open dialogs and file
/// associations): just [`EXTENSION`].
pub const EXTENSIONS: &[&str] = &[EXTENSION];

/// The media type of a budget file.
pub const MIME_TYPE: &str = "application/vnd.method-budget+json";

/// Whether `path` has one of the budget extensions ([`EXTENSIONS`]), in any
/// letter case.
pub fn has_budget_extension(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONS.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

/// The newest format version this build writes and reads.
pub const FORMAT_VERSION: u64 = 1;

/// Writes `b` in the native format.
///
/// The output is deterministic: equal budgets give byte-identical text, and
/// `to_string(&from_str(&to_string(b))?) == to_string(b)`.
pub fn to_string(b: &Budget) -> String {
    let mut budget = FileBudget::from(b);
    if b.compact_versions {
        delta::compact(b, &mut budget.versions);
    }
    let doc = FileDoc {
        format: FORMAT_NAME.into(),
        version: FORMAT_VERSION,
        budget,
    };
    // Only string and integer map keys, so serializing cannot fail.
    let mut s = serde_json::to_string_pretty(&doc).expect("budget always serializes");
    s.push('\n');
    s
}

/// Reads a budget written in the native format.
///
/// Checks `format` and `version` first, then parses strictly: unknown fields,
/// numbers where a decimal string belongs, and malformed decimals are errors.
pub fn from_str(s: &str) -> Result<Budget, FormatError> {
    let header: Header = serde_json::from_str(s).map_err(|e| FormatError::from_json(s, &e))?;
    match header.format {
        Some(serde_json::Value::String(f)) if f == FORMAT_NAME => {}
        other => {
            return Err(FormatError::NotABudget {
                found: other.map(|v| v.to_string()),
            })
        }
    }
    match header.version {
        Some(v) => match v.as_u64() {
            Some(n) if n > FORMAT_VERSION => {
                return Err(FormatError::NewerVersion {
                    found: n,
                    supported: FORMAT_VERSION,
                })
            }
            Some(n) if n >= 1 => {}
            _ => {
                return Err(FormatError::BadVersion {
                    found: Some(v.to_string()),
                })
            }
        },
        None => return Err(FormatError::BadVersion { found: None }),
    }
    let mut doc: FileDoc = serde_json::from_str(s).map_err(|e| FormatError::from_json(s, &e))?;
    // Versions may be deltas against the budget or each other, so they are
    // rebuilt once the rest is read.
    let versions = std::mem::take(&mut doc.budget.versions);
    let mut b: Budget = doc.budget.into();
    b.versions = delta::read_versions(&b, versions)?;
    Ok(b)
}

/// Why a file could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormatError {
    /// Not valid JSON, or JSON that does not match the format (unknown field,
    /// wrong type, bad decimal...). `line` and `column` are 1-based; `column`
    /// counts bytes. `path` locates the value, like
    /// `budget.topsheet[0].accounts[1].details[3].rate`, and is empty when the
    /// problem is at the top level.
    Invalid {
        line: usize,
        column: usize,
        path: String,
        message: String,
    },
    /// The top-level `"format"` field is missing or is not [`FORMAT_NAME`].
    /// `found` is the JSON text of the value found, if any.
    NotABudget { found: Option<String> },
    /// The `"version"` field is missing, or not a whole number of at least 1.
    BadVersion { found: Option<String> },
    /// The file was written by a newer format version than this build reads.
    NewerVersion { found: u64, supported: u64 },
    /// A saved version written as a delta (`compact_versions`) can't be
    /// rebuilt: its patch doesn't fit its base, or the result isn't a valid
    /// budget. `index` is its place in `budget.versions`.
    VersionDelta {
        index: usize,
        name: String,
        message: String,
    },
}

impl FormatError {
    fn from_json(text: &str, e: &serde_json::Error) -> FormatError {
        let (line, column) = (e.line(), e.column());
        let full = e.to_string();
        let suffix = format!(" at line {line} column {column}");
        let message = full.strip_suffix(&suffix).unwrap_or(&full).to_string();
        FormatError::Invalid {
            line,
            column,
            path: path_at(text, line, column, &message),
            message,
        }
    }
}

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FormatError::Invalid {
                line,
                column,
                path,
                message,
            } => {
                write!(f, "line {line}, column {column}")?;
                if !path.is_empty() {
                    write!(f, " ({path})")?;
                }
                write!(f, ": {message}")
            }
            FormatError::NotABudget { found: None } => write!(
                f,
                "not a Method Budget file: no \"format\" field (expected \"{FORMAT_NAME}\")"
            ),
            FormatError::NotABudget { found: Some(v) } => write!(
                f,
                "not a Method Budget file: format is {v}, expected \"{FORMAT_NAME}\""
            ),
            FormatError::BadVersion { found: None } => {
                f.write_str("missing \"version\" field: expected a whole number such as 1")
            }
            FormatError::BadVersion { found: Some(v) } => write!(
                f,
                "invalid format version {v}: expected a whole number such as 1"
            ),
            FormatError::VersionDelta {
                index,
                name,
                message,
            } => write!(
                f,
                "budget.versions[{index}] ({}): {message}",
                serde_json::Value::String(name.clone())
            ),
            FormatError::NewerVersion { found, supported } => write!(
                f,
                "this file uses format version {found}, but this build reads versions up \
                 to {supported}; open it with a newer Method Budget"
            ),
        }
    }
}

impl std::error::Error for FormatError {}

/// The JSON path of the value an error is about, from serde_json's error position,
/// found by scanning the text up to that point. Best effort: it only tracks
/// brackets, commas and object keys, which is all a path needs.
///
/// Objects that serde buffers before checking (kind-tagged objects, topsheet
/// rows) report their errors only after serde_json has also consumed the
/// closing bracket or comma that follows them. Those cases are undone here so
/// the path names the offending element rather than its container or sibling.
fn path_at(text: &str, line: usize, column: usize, message: &str) -> String {
    if line == 0 {
        return String::new();
    }
    let start = if line == 1 {
        0
    } else {
        match text.match_indices('\n').nth(line - 2) {
            Some((i, _)) => i + 1,
            None => text.len(),
        }
    };
    let bytes = &text.as_bytes()[..start.saturating_add(column).min(text.len())];
    let trim = |end: usize| {
        let mut end = end;
        while end > 0 && bytes[end - 1].is_ascii_whitespace() {
            end -= 1;
        }
        end
    };
    let end = trim(bytes.len());
    match end.checked_sub(1).map(|i| bytes[i]) {
        // A sequence consumed the comma after a failed element.
        Some(b',') => scan(bytes, true),
        // A missing field is about the object that just closed.
        Some(b'}' | b']') if !message.starts_with("missing field") => {
            let before_last = trim(end - 1);
            if before_last > 0 && matches!(bytes[before_last - 1], b'}' | b']') {
                // A buffered element closed, then its container closed too: the
                // error is about the container's last element.
                scan(&bytes[..end - 1], false)
            } else {
                scan(bytes, false)
            }
        }
        _ => scan(bytes, false),
    }
}

/// Renders the path open at the end of `bytes`. With `after_comma`, an array
/// position points back at the element before the comma.
fn scan(bytes: &[u8], after_comma: bool) -> String {
    enum Frame {
        Object { key: Option<String>, want_key: bool },
        Array(usize),
    }
    let mut stack: Vec<Frame> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                let from = i + 1;
                let mut j = from;
                while j < bytes.len() && bytes[j] != b'"' {
                    j += if bytes[j] == b'\\' { 2 } else { 1 };
                }
                if let Some(Frame::Object { key, want_key }) = stack.last_mut() {
                    if *want_key {
                        *key = Some(unescape(&bytes[from..j.min(bytes.len())]));
                        *want_key = false;
                    }
                }
                i = j;
            }
            b'{' => stack.push(Frame::Object {
                key: None,
                want_key: true,
            }),
            b'[' => stack.push(Frame::Array(0)),
            b'}' | b']' => {
                stack.pop();
            }
            b',' => match stack.last_mut() {
                Some(Frame::Object { want_key, .. }) => *want_key = true,
                Some(Frame::Array(n)) => *n += 1,
                None => {}
            },
            _ => {}
        }
        i += 1;
    }
    if after_comma {
        if let Some(Frame::Array(n)) = stack.last_mut() {
            *n = n.saturating_sub(1);
        }
    }

    let mut out = String::new();
    for frame in &stack {
        match frame {
            Frame::Object {
                key: Some(k),
                want_key: false,
            } => {
                let plain =
                    !k.is_empty() && k.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
                if plain {
                    if !out.is_empty() {
                        out.push('.');
                    }
                    out.push_str(k);
                } else {
                    out.push('[');
                    out.push_str(&serde_json::Value::String(k.clone()).to_string());
                    out.push(']');
                }
            }
            Frame::Object { .. } => {}
            Frame::Array(n) => out.push_str(&format!("[{n}]")),
        }
    }
    out
}

/// Decodes a JSON string body (without quotes), falling back to the raw text.
fn unescape(raw: &[u8]) -> String {
    let mut quoted = Vec::with_capacity(raw.len() + 2);
    quoted.push(b'"');
    quoted.extend_from_slice(raw);
    quoted.push(b'"');
    serde_json::from_slice(&quoted).unwrap_or_else(|_| String::from_utf8_lossy(raw).into_owned())
}

/// A report preset as the file writes it: one entry of the budget's
/// `report_presets` (docs/FORMAT.md, "Report preset"). For the app, which
/// sends and receives presets in this shape.
pub fn report_preset_to_value(p: &method_core::ReportPreset) -> serde_json::Value {
    serde_json::to_value(dto::FileReportPreset::from(p)).expect("a preset always serializes")
}

/// Reads a report preset in the file's shape, as strictly as a file is read
/// (unknown fields are errors).
pub fn report_preset_from_value(v: serde_json::Value) -> Result<method_core::ReportPreset, String> {
    serde_json::from_value::<dto::FileReportPreset>(v)
        .map(method_core::ReportPreset::from)
        .map_err(|e| e.to_string())
}

/// A tag preset as the file writes it: one entry of the budget's
/// `tag_presets` (docs/FORMAT.md, "Tag preset"). For the app, which sends and
/// receives tag presets in this shape.
pub fn tag_preset_to_value(p: &method_core::TagPreset) -> serde_json::Value {
    serde_json::to_value(dto::FileTagPreset::from(p)).expect("a tag preset always serializes")
}

/// Reads a tag preset in the file's shape, as strictly as a file is read.
pub fn tag_preset_from_value(v: serde_json::Value) -> Result<method_core::TagPreset, String> {
    serde_json::from_value::<dto::FileTagPreset>(v)
        .map(method_core::TagPreset::from)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_tracks_keys_and_indexes() {
        let text = "{\n  \"budget\": {\n    \"topsheet\": [\n      {\"a\": 1},\n      {\"b\": [1, 2, {\"odd key\": x";
        let line = text.lines().count();
        let column = text.lines().last().unwrap().len();
        assert_eq!(
            path_at(text, line, column, ""),
            "budget.topsheet[1].b[2][\"odd key\"]"
        );
    }

    #[test]
    fn path_after_comma_drops_previous_key() {
        let text = "{\"a\": 1, ";
        assert_eq!(path_at(text, 1, text.len(), ""), "");
        assert_eq!(path_at("{\"a\": 1", 1, 7, ""), "a");
        // Buffered elements: comma or closing bracket already consumed.
        assert_eq!(path_at("{\"r\": [{}, ", 1, 11, ""), "r[0]");
        assert_eq!(path_at("{\"r\": [{}, {}]", 1, 14, ""), "r[1]");
        assert_eq!(path_at("{\"m\": {\"k\": {}}", 1, 15, ""), "m.k");
        assert_eq!(
            path_at("{\"m\": {\"k\": {}}", 1, 15, "missing field `x`"),
            "m"
        );
    }

    #[test]
    fn budget_extensions() {
        use std::path::Path;
        for ok in ["a.mthd", "a.MTHD", "/x/v1.2.Mthd"] {
            assert!(has_budget_extension(Path::new(ok)), "{ok}");
        }
        for no in ["a", "a.json", "a.mthd.xlsx", "mthd", "a.mthd-tmp"] {
            assert!(!has_budget_extension(Path::new(no)), "{no}");
        }
        assert_eq!(EXTENSIONS, ["mthd"]);
    }
}
