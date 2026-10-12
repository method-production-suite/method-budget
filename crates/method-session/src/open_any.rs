//! Open reads more than budgets (decisions.md, entry 66): File > Open, the
//! toolbar's Open, the start window's Open…, a file dropped on the window
//! or the Dock icon and one handed over by the system (double-click, Open
//! With, the command line) all come here.
//!
//! - **A budget** (`.mthd`, or any file whose content is our format: it is
//!   JSON, so a budget saved as `.json` is one) opens in a tab, as before.
//! - **A budget export** (budget exchange JSON or XML, the export's Excel
//!   workbook or tab-delimited topsheet, found by content) goes to Import
//!   Budget Export…'s review with the file already picked, worded as
//!   converting it (entry 67): the page shows the line-by-line check and
//!   Convert opens the budget in a new unsaved tab that says where it came
//!   from and is first saved beside the export as a `.mthd`.
//! - **A spreadsheet** that isn't an export (`.xlsx`, `.csv`, `.tsv`,
//!   `.txt`, `.tab`) goes to Import Spreadsheet's dialog with the file
//!   picked; the page starts it on "a new budget tab".
//! - Anything else says what Open can read.
//!
//! The page gets a view, a review or the sheet dialog's start, each naming
//! the file by a token, as the import commands do. Showing the Open dialog,
//! reading the file and opening a budget in a tab are the host's (the
//! desktop's src/open_any.rs, which keeps the paths); this decides what the
//! bytes are and keeps an export or a spreadsheet by token.

use std::path::{Path, PathBuf};

use method_format::import::exchange::{self, Imported as Built, Reconciliation};
use method_format::import::{read_file, Book};
use serde::Serialize;

use crate::export_import::{ExportSources, Review};
use crate::sheet_import::{check_size, Picked, SheetSources};
use crate::view::View;

/// The spreadsheets Import Spreadsheet reads (its Open dialog's filter, and
/// `.tab`, which the export's tab-delimited topsheet may be called).
pub const SHEET_EXTENSIONS: &[&str] = &["xlsx", "csv", "tsv", "txt", "tab"];

/// The Open dialog's filter name for everything Open reads.
pub const ALL_FILTER: &str = "Budgets, budget exports and spreadsheets";

/// What Open reads, in a sentence (the message for anything else).
pub const OPEN_READS: &str = "Open reads Method Budget files (.mthd), budget exports \
(budget exchange JSON or XML, or the export's Excel or tab-delimited file) and \
spreadsheets (.xlsx, .csv, .tsv, .txt).";

/// Every extension Open's dialog allows: the budget's, the budget export
/// layouts' and the spreadsheets', each once.
pub fn open_extensions() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = Vec::new();
    for e in method_format::EXTENSIONS
        .iter()
        .chain(exchange::READABLE_EXTENSIONS)
        .chain(SHEET_EXTENSIONS)
    {
        if !v.contains(e) {
            v.push(e);
        }
    }
    v
}

/// What a file is to Open.
pub enum Plan {
    /// Our own format: opened as a budget.
    Budget,
    /// A budget export, read, built and checked.
    Export(Box<(Built, Reconciliation)>),
    /// A spreadsheet for Import Spreadsheet.
    Sheet(Book),
}

/// Whether `bytes` are a budget in our format (JSON whose `format` names
/// it), whatever the file is called. Only the start is looked at; a file
/// that says it is a budget and isn't one gets the budget reader's own
/// message when it is opened.
pub fn looks_like_budget(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(4096)];
    let s = String::from_utf8_lossy(head);
    let t = s.trim_start_matches('\u{feff}').trim_start();
    let Some(rest) = t.strip_prefix('{') else {
        return false;
    };
    // `"format": "method-budget"`, with any spacing.
    rest.split("\"format\"").skip(1).any(|after| {
        after
            .trim_start()
            .strip_prefix(':')
            .map(str::trim_start)
            .is_some_and(|v| v.starts_with(&format!("\"{}\"", method_format::FORMAT_NAME)))
    })
}

fn extension(file: &str) -> String {
    Path::new(file)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

/// What Open does with `bytes`, a file named `file` (the name only):
/// decided by content first, the extension only telling a spreadsheet
/// from other text. An export that can't be read, or a spreadsheet that
/// can't, says why; anything else says what Open reads.
pub fn plan(file: &str, bytes: &[u8]) -> Result<Plan, String> {
    if method_format::has_budget_extension(Path::new(file)) || looks_like_budget(bytes) {
        return Ok(Plan::Budget);
    }
    // By content only: a `.json` that is no export isn't read as one here.
    let mut export_error = None;
    if exchange::detect(bytes, "").is_some() {
        match exchange::import(bytes, file, None) {
            Ok(done) => return Ok(Plan::Export(Box::new(done))),
            Err(e) => export_error = Some(e.to_string()),
        }
    }
    let is_zip = bytes.starts_with(b"PK\x03\x04");
    if is_zip || SHEET_EXTENSIONS.contains(&extension(file).as_str()) {
        match read_file(bytes, file) {
            Ok(book) if book.tables.iter().any(|t| !t.rows.is_empty()) => {
                return Ok(Plan::Sheet(book))
            }
            Ok(_) => {}
            Err(e) => {
                export_error.get_or_insert(e.to_string());
            }
        }
    }
    Err(match export_error {
        Some(e) => format!("Couldn't read {file}: {e}."),
        None => format!("{file} isn't a budget, a budget export or a spreadsheet. {OPEN_READS}"),
    })
}

/// What the page gets from Open: the budget in a tab, the review of a
/// budget export, or Import Spreadsheet's dialog to show.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Opened {
    Budget { view: Box<View> },
    Export { review: Box<Review> },
    Sheet { picked: Picked },
}

/// What Open gives back for `plan`, the plan for `bytes` (a file named
/// `file`, at `path` when the host has one: where a converted export is
/// first saved). A budget is opened by `budget`, handed the bytes (the
/// host's own way, in a tab); an export or a spreadsheet is kept by token for its review.
pub fn open_planned(
    plan: Plan,
    file: String,
    bytes: Vec<u8>,
    path: Option<PathBuf>,
    exports: &ExportSources,
    sheets: &SheetSources,
    budget: impl FnOnce(Vec<u8>) -> Result<View, String>,
) -> Result<Opened, String> {
    match plan {
        Plan::Budget => Ok(Opened::Budget {
            view: Box::new(budget(bytes)?),
        }),
        Plan::Export(done) => {
            let (built, rec) = *done;
            // Opened, not imported: the review converts it (decisions.md,
            // entry 67), and the first Save starts beside it.
            let token = exports.keep(file.clone(), bytes, path, true);
            let mut review = crate::export_import::review(token, &file, &built, &rec);
            review.convert = true;
            Ok(Opened::Export {
                review: Box::new(review),
            })
        }
        Plan::Sheet(book) => {
            let token = sheets.keep(file.clone(), book);
            let (_, book) = sheets.get(token)?;
            Ok(Opened::Sheet {
                picked: crate::sheet_import::picked(token, file, &book),
            })
        }
    }
}

// ---------------------------------------------------------------------------
// Commands (the desktop's File > Open, src/app.rs and src/safety_app.rs, and
// the host's src/host/open_any.rs call these)

/// Opens a picked file, `bytes` from a file named `file`, the way its
/// content asks: a budget by `budget` (handed the bytes), an export or a
/// spreadsheet kept by token for its review. Nothing changes when the file
/// can't be read.
pub fn open_budget(
    file: String,
    bytes: Vec<u8>,
    exports: &ExportSources,
    sheets: &SheetSources,
    budget: impl FnOnce(Vec<u8>) -> Result<View, String>,
) -> Result<Opened, String> {
    let plan = plan(&file, &bytes)?;
    if !matches!(plan, Plan::Budget) {
        check_size(bytes.len())?;
    }
    open_planned(plan, file, bytes, None, exports, sheets, budget)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Vec<u8> {
        let p = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../method-format/tests/fixtures/exchange")
            .join(name);
        std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
    }

    fn kind(p: &Result<Plan, String>) -> &'static str {
        match p {
            Ok(Plan::Budget) => "budget",
            Ok(Plan::Export(_)) => "export",
            Ok(Plan::Sheet(_)) => "sheet",
            Err(_) => "error",
        }
    }

    #[test]
    fn the_dialog_allows_budgets_exports_and_spreadsheets() {
        let ext = open_extensions();
        for e in ["mthd", "json", "xml", "xlsx", "csv", "tsv", "txt", "tab"] {
            assert!(ext.contains(&e), "{e}");
        }
        assert_eq!(ext[0], "mthd");
        let mut sorted = ext.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), ext.len(), "each once");
    }

    #[test]
    fn a_budget_saved_as_json_opens_as_a_budget() {
        let b = method_core::Budget::new("Pilot", "USD".to_string());
        let text = method_format::to_string(&b);
        assert!(looks_like_budget(text.as_bytes()));
        assert_eq!(kind(&plan("pilot.json", text.as_bytes())), "budget");
        assert_eq!(kind(&plan("pilot.txt", text.as_bytes())), "budget");
        // With a byte order mark and the field anywhere near the start.
        let moved = "\u{feff} { \"version\" : 1, \"format\" :\"method-budget\" }";
        assert!(looks_like_budget(moved.as_bytes()));
        // The extension alone is enough for a .mthd (its reader says what's wrong).
        assert_eq!(kind(&plan("broken.MTHD", b"nonsense")), "budget");
        // Something else named method-budget is not a budget.
        assert!(!looks_like_budget(br#"{"title": "method-budget"}"#));
        assert!(!looks_like_budget(
            br#"{"format": "other", "x": "method-budget"}"#
        ));
    }

    #[test]
    fn a_json_export_goes_to_the_review() {
        let bytes = fixture("by-category-charges.json");
        let p = plan("by-category-charges.json", &bytes);
        assert_eq!(kind(&p), "export");
        if let Ok(Plan::Export(done)) = p {
            assert!(done.0.counts.categories > 0);
        }
        // Read by content, whatever its name.
        assert_eq!(kind(&plan("renamed.dat", &bytes)), "export");
    }

    #[test]
    fn xml_and_workbook_exports_go_to_the_review() {
        for f in [
            "by-category-charges-xmladv.xml",
            "by-category-charges-xmlbasic.xml",
            "by-category-charges-excel.xlsx",
            "by-category-charges-tab.txt",
        ] {
            assert_eq!(kind(&plan(f, &fixture(f))), "export", "{f}");
        }
    }

    #[test]
    fn a_plain_spreadsheet_goes_to_import_spreadsheet() {
        let b = method_core::Budget::new("Pilot", "USD".to_string());
        let wb = method_format::export::workbook_of(&b).unwrap();
        let xlsx = method_format::export::xlsx::write(&wb).unwrap();
        assert_eq!(kind(&plan("lines.xlsx", &xlsx)), "sheet");
        let csv = b"Account,Description,Amount,Rate\n1101,Writer,1,5000\n";
        assert_eq!(kind(&plan("lines.csv", csv)), "sheet");
        assert_eq!(kind(&plan("lines.txt", b"a\tb\tc\n1\t2\t3\n")), "sheet");
    }

    #[test]
    fn anything_else_says_what_open_reads() {
        for (f, bytes) in [
            ("notes.json", br#"{"hello": 1}"#.as_slice()),
            ("feed.xml", b"<?xml version=\"1.0\"?><rss></rss>".as_slice()),
            ("picture.png", b"\x89PNG\r\n".as_slice()),
            ("empty.csv", b"".as_slice()),
        ] {
            let e = plan(f, bytes).err().unwrap_or_else(|| panic!("{f} opened"));
            assert!(e.contains(OPEN_READS), "{f}: {e}");
            assert!(e.starts_with(f), "{e}");
            assert!(!e.contains('\u{2014}'), "no em dashes");
        }
        // A workbook that isn't one says why.
        let e = plan("broken.xlsx", b"not a zip").err().unwrap();
        assert!(e.starts_with("Couldn't read broken.xlsx"), "{e}");
    }
}
