//! Importing a whole budget from another budgeting program's open exports
//! (roadmap 1.16): its JSON, XML (basic and advanced), Excel and
//! tab-delimited exports, read into one model ([`model::Export`]), made
//! into a budget by one builder ([`build`]) and checked against the
//! export's own figures ([`reconcile`]).
//!
//! Each reader is written from the export files themselves, which are the
//! program's documented, user-facing open formats; nothing else. Every
//! file is untrusted input: the readers are plain std code with the same
//! limits as the spreadsheet import.
//!
//! What an export can't carry (formulas, globals, production totals,
//! fringe cutoffs, exchange rates, unit hours, the rules of charges and
//! credits, lines a group leaves out) is worked out from the figures where
//! the figures decide it, and said so in [`Imported::notes`]; the
//! reconciliation then shows, figure by figure, whether the budget comes
//! to what the export says.

pub mod build;
mod infer;
pub mod json;
pub mod model;
pub mod reconcile;
pub mod sheets;
pub mod xmlread;

pub use build::{build, BuildOptions, Counts, ImportNote, Imported, NoteKind};
pub use model::{Export, ExportFormat};
pub use reconcile::{reconcile, summary_lines, Difference, Level, Reconciliation, Tally};

use super::{ReadError, MAX_FILE_BYTES};

/// The layouts this build reads.
pub const READABLE: &[ExportFormat] = &ExportFormat::ALL;

/// Every extension of [`READABLE`] (for Open dialogs).
pub const READABLE_EXTENSIONS: &[&str] = &["json", "xml", "xlsx", "txt", "tab", "tsv"];

/// The layout `bytes` (a file named `file_name`) is in, by its content
/// (a `.json` file that isn't one is still read as JSON, to say why);
/// `None` when it is none of them.
pub fn detect(bytes: &[u8], file_name: &str) -> Option<ExportFormat> {
    if json::looks_like(bytes) {
        return Some(ExportFormat::Json);
    }
    if let Some(advanced) = xmlread::looks_like(bytes) {
        return Some(if advanced {
            ExportFormat::XmlAdvanced
        } else {
            ExportFormat::XmlBasic
        });
    }
    if sheets::looks_like_workbook(bytes) {
        return Some(ExportFormat::Excel);
    }
    if sheets::looks_like_tab(bytes) {
        return Some(ExportFormat::Tab);
    }
    file_name
        .to_ascii_lowercase()
        .ends_with(".json")
        .then_some(ExportFormat::Json)
}

/// Reads an export, in `format` or the one [`detect`] finds.
pub fn read(
    bytes: &[u8],
    file_name: &str,
    format: Option<ExportFormat>,
) -> Result<Export, ReadError> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(ReadError::new(format!(
            "the file is too large to import (more than {} MB)",
            MAX_FILE_BYTES >> 20
        )));
    }
    if matches!(format, None | Some(ExportFormat::Json)) && super::schedule::looks_like(bytes) {
        return Err(ReadError::new(
            "this is a schedule's day counts (a Method Scheduling days file), not a budget export; link it as a schedule instead",
        ));
    }
    let format = match format.or_else(|| detect(bytes, file_name)) {
        Some(f) => f,
        None => {
            return Err(ReadError::new(
                "this isn't a budget export this version reads (budget exchange JSON or XML, or the export's .xlsx spreadsheet or tab-delimited file)",
            ))
        }
    };
    match format {
        ExportFormat::Json => json::read(bytes),
        ExportFormat::XmlBasic | ExportFormat::XmlAdvanced => xmlread::read(bytes, format),
        ExportFormat::Excel => sheets::read_workbook(bytes),
        ExportFormat::Tab => sheets::read_tab(bytes),
    }
}

/// Reads, builds and reconciles in one go. `title` names the budget when
/// the export doesn't (usually the file's name without its extension).
pub fn import(
    bytes: &[u8],
    file_name: &str,
    format: Option<ExportFormat>,
) -> Result<(Imported, Reconciliation), ReadError> {
    let x = read(bytes, file_name, format)?;
    let stem = std::path::Path::new(file_name)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let im = build(&x, &BuildOptions { title: stem });
    let rec = reconcile(&im);
    Ok((im, rec))
}
