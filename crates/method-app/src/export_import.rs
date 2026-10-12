//! File > Import Budget Export…: a whole budget from another budgeting
//! program's open export (budget exchange JSON or XML, its Excel or
//! tab-delimited export), with the reconciliation shown before anything is
//! imported (the importer is `method_format::import::exchange`; roadmap
//! 1.16, docs/APP.md).
//!
//! - **The file.** Rust shows the Open dialog and reads the file; the page
//!   names it by a token afterwards, as Import Spreadsheet does.
//! - **The review** says what was read (categories, accounts, lines, setup),
//!   the notes (what was worked out from the export's figures and should
//!   be checked, and what couldn't come over), and the reconciliation: the
//!   imported budget's totals against the export's own, kind by kind, with
//!   every figure that differs. The layout is detected; the dialog can read
//!   the file as another one.
//! - **Importing** opens the budget in a new tab, unsaved (its first undo
//!   step), like Import Chart of Accounts' new budget.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use method_core::Cents;
use method_format::import::exchange::{
    self, ExportFormat, Imported as Built, Level, NoteKind, Reconciliation,
};
use serde::Serialize;
use tauri::{Runtime, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::app::{finish, AppState, DocId};
use crate::history::count;
use crate::session::Session;
use crate::sheet_import::read_capped;
use crate::view::View;

/// Differences listed in the review (the counts are complete).
pub const LISTED: usize = 200;

// ---------------------------------------------------------------------------
// The file, kept by token

struct Source {
    token: u64,
    file: String,
    bytes: Arc<Vec<u8>>,
}

/// The last export picked (the app's state; one for every budget).
#[derive(Default)]
pub struct ExportSources {
    inner: Mutex<(u64, Option<Source>)>,
}

impl ExportSources {
    fn lock(&self) -> MutexGuard<'_, (u64, Option<Source>)> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn keep(&self, file: String, bytes: Vec<u8>) -> u64 {
        let mut g = self.lock();
        g.0 += 1;
        let token = g.0;
        g.1 = Some(Source {
            token,
            file,
            bytes: Arc::new(bytes),
        });
        token
    }

    fn get(&self, token: u64) -> Result<(String, Arc<Vec<u8>>), String> {
        let g = self.lock();
        g.1.as_ref()
            .filter(|s| s.token == token)
            .map(|s| (s.file.clone(), s.bytes.clone()))
            .ok_or_else(|| "Another file was picked since. Import again.".to_string())
    }
}

// ---------------------------------------------------------------------------
// What the page gets

#[derive(Debug, Serialize)]
pub struct FormatDto {
    pub key: &'static str,
    pub label: &'static str,
}

#[derive(Debug, Serialize)]
pub struct NoteDto {
    /// `check`, `not_imported` or `info`.
    pub kind: &'static str,
    pub text: String,
}

#[derive(Debug, Serialize)]
pub struct TallyDto {
    pub kind: &'static str,
    /// What was compared, in words (`lines`, `account totals`).
    pub label: &'static str,
    pub compared: usize,
    pub different: usize,
}

#[derive(Debug, Serialize)]
pub struct DifferenceDto {
    pub kind: &'static str,
    pub what: String,
    pub export: Cents,
    pub imported: Cents,
}

/// What the review shows.
#[derive(Debug, Serialize)]
pub struct Review {
    pub token: u64,
    pub file: String,
    /// The layout read, and the ones the dialog can read the file as.
    pub format: &'static str,
    pub format_label: &'static str,
    pub formats: Vec<FormatDto>,
    pub title: String,
    pub categories: usize,
    pub accounts: usize,
    pub lines: usize,
    pub adjustments: usize,
    pub fringes: usize,
    pub groups: usize,
    pub sets: usize,
    pub locations: usize,
    pub currencies: usize,
    pub charges: usize,
    pub credits: usize,
    pub notes: Vec<NoteDto>,
    /// The currency the figures are compared in.
    pub currency: String,
    pub tallies: Vec<TallyDto>,
    pub differences: Vec<DifferenceDto>,
    pub difference_count: usize,
    pub export_grand: Cents,
    pub grand: Option<Cents>,
    pub export_net: Cents,
    pub net: Option<Cents>,
    pub calc_error: Option<String>,
    /// Every compared figure is the same.
    pub matches: bool,
}

/// What `export_import` did.
#[derive(Debug, Serialize)]
pub struct Imported {
    pub view: View,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Plain functions

fn format_of(key: Option<&str>) -> Result<Option<ExportFormat>, String> {
    match key.map(str::trim).filter(|k| !k.is_empty()) {
        None => Ok(None),
        Some(k) => ExportFormat::parse(k)
            .map(Some)
            .ok_or_else(|| format!("Unknown export layout `{k}`.")),
    }
}

fn read(
    file: &str,
    bytes: &[u8],
    format: Option<ExportFormat>,
) -> Result<(Built, Reconciliation), String> {
    exchange::import(bytes, file, format).map_err(|e| {
        let mut s = format!("Couldn't read {file}: {e}");
        s.push('.');
        s
    })
}

/// The review of an import.
pub fn review(token: u64, file: &str, im: &Built, rec: &Reconciliation) -> Review {
    let c = &im.counts;
    let order = [NoteKind::Check, NoteKind::Lost, NoteKind::Info];
    let mut notes: Vec<&exchange::ImportNote> = im.notes.iter().collect();
    notes.sort_by_key(|n| order.iter().position(|k| *k == n.kind));
    Review {
        token,
        file: file.to_string(),
        format: im.format.key(),
        format_label: im.format.label(),
        formats: exchange::READABLE
            .iter()
            .map(|f| FormatDto {
                key: f.key(),
                label: f.label(),
            })
            .collect(),
        title: im.budget.title.clone(),
        categories: c.categories,
        accounts: c.accounts,
        lines: c.lines,
        adjustments: c.adjustments,
        fringes: c.fringes,
        groups: c.groups,
        sets: c.sets,
        locations: c.locations,
        currencies: c.currencies,
        charges: c.charges,
        credits: c.credits,
        notes: notes
            .into_iter()
            .map(|n| NoteDto {
                kind: n.kind.key(),
                text: n.text.clone(),
            })
            .collect(),
        currency: rec.currency.clone(),
        tallies: Level::ALL
            .iter()
            .filter_map(|l| {
                rec.tallies.get(l).map(|t| TallyDto {
                    kind: l.key(),
                    label: l.label(),
                    compared: t.compared,
                    different: t.different,
                })
            })
            .collect(),
        differences: rec
            .differences
            .iter()
            .take(LISTED)
            .map(|d| DifferenceDto {
                kind: d.level.key(),
                what: d.what.clone(),
                export: d.export,
                imported: d.ours,
            })
            .collect(),
        difference_count: rec.differences.len(),
        export_grand: rec.export_grand,
        grand: rec.grand,
        export_net: rec.export_net,
        net: rec.net,
        calc_error: rec.calc_error.clone(),
        matches: rec.matches(),
    }
}

/// A new session holding the imported budget, as its first undo step (so
/// it starts unsaved).
fn new_session(im: Built) -> Result<(Session, String), String> {
    let c = im.counts.clone();
    if c.categories == 0 {
        return Err("The export has no categories to import.".into());
    }
    let mut s = Session::with_budget(
        method_core::Budget::new(crate::templates::UNTITLED, im.budget.base_currency.clone()),
        None,
    );
    let made = im.budget;
    s.edit("Import a budget export", |b| {
        *b = made;
        Ok(())
    })?;
    let msg = format!(
        "Imported {}, {} and {}: check the notes in the review before relying on it.",
        count(c.categories, "category", "categories"),
        count(c.accounts, "account", "accounts"),
        count(c.lines, "line", "lines")
    );
    Ok((s, msg))
}

// ---------------------------------------------------------------------------
// Commands (registered in app.rs, build.rs and capabilities/main.json; the
// tools window may call none of them)

/// Shows an Open dialog for a budget export, reads it and returns the
/// review. `None` when cancelled.
#[tauri::command]
pub async fn export_import_pick<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    sources: State<'_, ExportSources>,
    doc: Option<DocId>,
) -> Result<Option<Review>, String> {
    let start = {
        let s = state.lock_doc(doc)?;
        s.path().and_then(|p| p.parent()).map(PathBuf::from)
    };
    let mut dialog = window
        .dialog()
        .file()
        .set_title("Import budget export")
        .add_filter("Budget export", exchange::READABLE_EXTENSIONS)
        .set_parent(&window);
    for f in exchange::READABLE {
        dialog = dialog.add_filter(f.label(), f.extensions());
    }
    if let Some(dir) = start {
        dialog = dialog.set_directory(dir);
    }
    let Some(picked_file) = dialog.blocking_pick_file() else {
        return Ok(None);
    };
    let path = picked_file.into_path().map_err(|e| e.to_string())?;
    let file = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let bytes = read_capped(&path)?;
    let (im, rec) = read(&file, &bytes, None)?;
    let token = sources.keep(file.clone(), bytes);
    Ok(Some(review(token, &file, &im, &rec)))
}

/// The review of the picked export read as `format` (`None`: detected).
/// Changes nothing.
#[tauri::command]
pub fn export_import_preview(
    sources: State<'_, ExportSources>,
    token: u64,
    format: Option<String>,
) -> Result<Review, String> {
    let (file, bytes) = sources.get(token)?;
    let (im, rec) = read(&file, &bytes, format_of(format.as_deref())?)?;
    Ok(review(token, &file, &im, &rec))
}

/// Imports the picked export as a new budget in a new tab.
#[tauri::command]
pub fn export_import<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    sources: State<'_, ExportSources>,
    token: u64,
    format: Option<String>,
) -> Result<Imported, String> {
    let (file, bytes) = sources.get(token)?;
    let (im, _) = read(&file, &bytes, format_of(format.as_deref())?)?;
    let (session, message) = new_session(im)?;
    let id = state.docs().add(session);
    Ok(Imported {
        view: finish(&window, &mut state.lock_doc(Some(id))?, None),
        message,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXPORT: &str = r#"{"metadata": {"budgetdata": {"BaseCurrency": "USD", "ConvCurrency": "USD"}},
     "categories": [{"cID": 0, "cNumber": "1100", "cDescription": "Story", "cFringe": 0, "cOriginal": 0, "cTotal": 300, "cVariance": 300},
                    {"cID": 1, "cNumber": "", "cDescription": "Total Fringes", "cFringe": null, "cOriginal": 0, "cTotal": 0, "cVariance": 0, "fringeDetail": []},
                    {"cID": 2, "cNumber": "", "cDescription": "Fee", "cFringe": null, "cOriginal": 0, "cTotal": 15, "cVariance": 15}],
     "accounts": [{"categoryID": 0, "aID": 0, "aNumber": "1101", "aDescription": "Rights", "aFringe": 0, "aOriginal": 0, "aTotal": 300, "aVariance": 300}],
     "details": [{"accountID": 0, "dDescription": "Option", "dAmount": 3, "dUnit": "Weeks", "dX": 1, "dUnit2": "", "dCurrency": "", "dRate": 100, "dUnit3": "", "dUnit4": "", "dSubtotal": 300, "dFourthMlt": 1, "dFringes": [], "dGroups": [], "dGlobalsUsed": [], "dLocation": "", "dSet": "", "dAggPercent": "", "dAggSumPercent": ""}]}"#;

    #[test]
    fn the_review_reconciles_and_the_import_starts_unsaved() {
        let (im, rec) = read("x.json", EXPORT.as_bytes(), None).unwrap();
        let r = review(7, "x.json", &im, &rec);
        assert_eq!((r.categories, r.accounts, r.lines, r.charges), (1, 1, 1, 1));
        assert!(r.matches, "{:?}", r.differences);
        assert_eq!(r.export_grand, 31_500);
        assert_eq!(r.grand, Some(31_500));
        assert_eq!(r.format, "json");
        assert!(r.formats.iter().any(|f| f.key == "json"));
        // Things to check come first.
        assert_eq!(r.notes[0].kind, "check");
        let (s, msg) = new_session(im).unwrap();
        assert!(s.is_dirty());
        assert_eq!(s.budget().charges.len(), 1);
        assert!(msg.contains("1 line"), "{msg}");
    }

    #[test]
    fn bad_files_and_layouts_say_why() {
        assert!(read("x.txt", b"hello", None)
            .unwrap_err()
            .contains("Couldn't read x.txt"));
        assert!(format_of(Some("pdf")).is_err());
        assert_eq!(format_of(Some("")).unwrap(), None);
        assert_eq!(format_of(Some("json")).unwrap(), Some(ExportFormat::Json));
        let sources = ExportSources::default();
        let t = sources.keep("a.json".into(), b"{}".to_vec());
        assert!(sources.get(t).is_ok());
        let t2 = sources.keep("b.json".into(), b"{}".to_vec());
        assert!(sources.get(t).is_err() && sources.get(t2).is_ok());
    }
}
