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
//!   step), like Import Chart of Accounts' new budget. Its first Save starts
//!   in the export's folder under the export's name with `.mthd`.
//! - **Converting.** An export opened with Open, from Finder or dropped on
//!   the window (src/open_any.rs) gets the same review, worded as a
//!   conversion, and the new tab says once where it came from, that the
//!   export is left as it was and that saving keeps a `.mthd` (decisions.md,
//!   entry 67). Nothing is ever written in the export's own layout.
//!
//! Showing the Open dialog and reading the file are the host's (the
//! desktop's src/export_import.rs); this reads the bytes it hands in and
//! keeps them by token. A path here is only where the first Save starts.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use method_core::Cents;
use method_format::import::exchange::{
    self, ExportFormat, Imported as Built, Level, NoteKind, Reconciliation,
};
use serde::Serialize;

use crate::docs::{AppState, DocId};
use crate::history::count;
use crate::session::Session;
use crate::sheet_import::check_size;
use crate::view::View;

/// Differences listed in the review (the counts are complete).
pub const LISTED: usize = 200;

// ---------------------------------------------------------------------------
// The file, kept by token

struct Source {
    token: u64,
    file: String,
    bytes: Arc<Vec<u8>>,
    /// Where the export is (for the first Save's folder and name).
    path: Option<PathBuf>,
    /// It arrived through Open, Finder or a drop: a conversion.
    convert: bool,
}

/// What the import needs of the picked export.
struct Picked {
    file: String,
    bytes: Arc<Vec<u8>>,
    path: Option<PathBuf>,
    convert: bool,
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

    /// Keeps the export read from `path` (named `file`); `convert` when it
    /// arrived through Open rather than Import Budget Export….
    pub fn keep(&self, file: String, bytes: Vec<u8>, path: Option<PathBuf>, convert: bool) -> u64 {
        let mut g = self.lock();
        g.0 += 1;
        let token = g.0;
        g.1 = Some(Source {
            token,
            file,
            bytes: Arc::new(bytes),
            path,
            convert,
        });
        token
    }

    fn get(&self, token: u64) -> Result<Picked, String> {
        let g = self.lock();
        g.1.as_ref()
            .filter(|s| s.token == token)
            .map(|s| Picked {
                file: s.file.clone(),
                bytes: s.bytes.clone(),
                path: s.path.clone(),
                convert: s.convert,
            })
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
    /// The export arrived through Open (or Finder, or a drop): the review
    /// is worded as converting it.
    pub convert: bool,
}

/// What `export_import` did. A conversion's notice comes with the view
/// (`view.notice`), so there is no message then.
#[derive(Debug, Serialize)]
pub struct Imported {
    pub view: View,
    pub message: Option<String>,
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
        convert: false,
    }
}

/// Where a budget converted from the export at `export` is first offered
/// to be saved: the export's folder, its name with `.mthd`
/// ("Pilot budget.json" -> "Pilot budget.mthd").
pub fn save_path_for(export: &Path) -> PathBuf {
    let stem = export
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = format!(
        "{}.{}",
        crate::templates::safe_file_stem(&stem),
        method_format::EXTENSION
    );
    export.with_file_name(name)
}

/// What a converted budget's tab says once.
pub fn converted_notice(file: &str) -> String {
    format!(
        "Converted from {file}. The original file is left as it was. Saving keeps this \
budget as a Method Budget file (.mthd)."
    )
}

/// A new session holding the imported budget, as its first undo step (so
/// it starts unsaved), first saved next to the export. A conversion says
/// so with its first view; an import has a message for the toast.
fn new_session(im: Built, from: &Picked) -> Result<(Session, Option<String>), String> {
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
    s.set_save_hint(from.path.as_deref().map(save_path_for));
    if from.convert {
        s.set_notice(converted_notice(&from.file));
        return Ok((s, None));
    }
    let msg = format!(
        "Imported {}, {} and {}: check the notes in the review before relying on it.",
        count(c.categories, "category", "categories"),
        count(c.accounts, "account", "accounts"),
        count(c.lines, "line", "lines")
    );
    Ok((s, Some(msg)))
}

// ---------------------------------------------------------------------------
// Commands (the desktop's src/export_import.rs and the host's
// src/host/export_import.rs call these)

/// Reads a picked budget export (`bytes` from a file named `file`, at
/// `path` when the host has one: where the first Save starts), keeps it
/// and returns the review.
pub fn export_import_pick(
    sources: &ExportSources,
    file: String,
    bytes: Vec<u8>,
    path: Option<PathBuf>,
) -> Result<Review, String> {
    check_size(bytes.len())?;
    let (im, rec) = read(&file, &bytes, None)?;
    let token = sources.keep(file.clone(), bytes, path, false);
    Ok(review(token, &file, &im, &rec))
}

/// The review of the picked export read as `format` (`None`: detected).
/// Changes nothing.
pub fn export_import_preview(
    sources: &ExportSources,
    token: u64,
    format: Option<&str>,
) -> Result<Review, String> {
    let p = sources.get(token)?;
    let (im, rec) = read(&p.file, &p.bytes, format_of(format)?)?;
    let mut r = review(token, &p.file, &im, &rec);
    r.convert = p.convert;
    Ok(r)
}

/// Imports the picked export as a new budget in a new tab, in front. Its
/// id, and the message for the toast (none for a conversion, whose view
/// says so).
pub fn export_import(
    state: &AppState,
    sources: &ExportSources,
    token: u64,
    format: Option<&str>,
) -> Result<(DocId, Option<String>), String> {
    let p = sources.get(token)?;
    let (im, _) = read(&p.file, &p.bytes, format_of(format)?)?;
    let (session, message) = new_session(im, &p)?;
    Ok((state.docs().add(session), message))
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
        let from = Picked {
            file: "x.json".into(),
            bytes: Arc::new(Vec::new()),
            path: Some(PathBuf::from("/films/x.json")),
            convert: false,
        };
        let (mut s, msg) = new_session(im, &from).unwrap();
        assert!(s.is_dirty());
        assert_eq!(s.budget().charges.len(), 1);
        let msg = msg.unwrap();
        assert!(msg.contains("1 line"), "{msg}");
        // Imported, not converted: no notice; still saved by the export.
        assert_eq!(s.take_notice(), None);
        assert_eq!(s.save_hint(), Some(Path::new("/films/x.mthd")));
    }

    #[test]
    fn a_converted_export_says_so_once_and_saves_beside_it() {
        let (im, _) = read("x.json", EXPORT.as_bytes(), None).unwrap();
        let from = Picked {
            file: "Pilot budget.json".into(),
            bytes: Arc::new(Vec::new()),
            path: Some(PathBuf::from("/films/Pilot budget.json")),
            convert: true,
        };
        let (mut s, msg) = new_session(im, &from).unwrap();
        assert_eq!(msg, None, "the notice comes with the view instead");
        assert!(s.is_dirty() && s.path().is_none(), "never written anywhere");
        assert_eq!(s.save_hint(), Some(Path::new("/films/Pilot budget.mthd")));
        let notice = s.take_notice().unwrap();
        assert_eq!(notice, converted_notice("Pilot budget.json"));
        assert!(
            notice.starts_with("Converted from Pilot budget.json."),
            "{notice}"
        );
        assert!(
            notice.contains("original file is left as it was"),
            "{notice}"
        );
        assert!(notice.contains(".mthd"), "{notice}");
        assert!(!notice.contains('\u{2014}'), "no em dashes");
        // Once.
        assert_eq!(s.take_notice(), None);
    }

    #[test]
    fn the_saved_name_is_the_exports_with_mthd() {
        for (export, saved) in [
            ("/a/Pilot budget.json", "/a/Pilot budget.mthd"),
            ("/a/b/ZTEST.XML", "/a/b/ZTEST.mthd"),
            ("/a/topsheet.tab.txt", "/a/topsheet.tab.mthd"),
        ] {
            assert_eq!(
                save_path_for(Path::new(export)),
                PathBuf::from(saved),
                "{export}"
            );
        }
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
        let t = sources.keep("a.json".into(), b"{}".to_vec(), None, false);
        assert!(sources.get(t).is_ok_and(|p| !p.convert));
        let t2 = sources.keep("b.json".into(), b"{}".to_vec(), None, true);
        assert!(sources.get(t).is_err() && sources.get(t2).is_ok_and(|p| p.convert));
    }
}
