//! File > Export: the budget as an Excel workbook, CSV files (one per
//! table) or the detail as tab-delimited text for accounting tools. The
//! tables and the file formats are `fwb_format::export`; this is the app's
//! side: the totals the grid shows, the Save dialog, writing the files.
//!
//! Security: the page names only the format and the budget (`doc`); the path
//! comes from the native Save dialog shown here, and the extension is the
//! format's. Each file is written atomically (`safety::write_atomically`),
//! and an export never replaces a budget that is open in a tab.

use std::path::{Path, PathBuf};

use fwb_format::export::{self, Format, Options, OutFile, Workbook};
use serde::Serialize;
use tauri::{Runtime, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::app::{AppState, DocId};
use crate::docs::Docs;
use crate::safety::{write_atomically, Access};
use crate::session::Session;

/// How an export ended.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ExportOutcome {
    /// False when the Save dialog was cancelled: nothing was written.
    pub done: bool,
    /// The names of the files written, in order.
    pub files: Vec<String>,
}

/// One budget's export, ready for the dialog: its tables, and the folder and
/// file name the dialog suggests.
pub struct Prepared {
    pub workbook: Workbook,
    pub dir: Option<PathBuf>,
    pub stem: String,
}

/// The tables of the budget in `s`, with the totals its grid shows. Refused
/// while any cell has a problem: the grid then counts those cells as blank,
/// and an export must carry the budget's real figures.
pub fn prepare(s: &mut Session) -> Result<Prepared, String> {
    let dir = s.path().and_then(Path::parent).map(PathBuf::from);
    let stem = s
        .path()
        .and_then(Path::file_stem)
        .map(|n| n.to_string_lossy().into_owned());
    let (budget, outcome) = s.totals();
    if let Some(e) = &outcome.error {
        let n = outcome.issues.len().max(1);
        let (cells, them) = if n == 1 {
            ("a cell with a problem".to_string(), "it")
        } else {
            (format!("{n} cells with problems"), "them")
        };
        return Err(format!(
            "This budget has {cells} ({e}). Fix {them} before exporting, so \
             the export carries the budget's real totals."
        ));
    }
    let report = outcome
        .report
        .as_ref()
        .ok_or("This budget's totals can't be worked out, so it can't be exported.")?;
    let workbook = export::workbook(budget, report).map_err(|e| capitalize(&e.to_string()))?;
    let stem = stem.unwrap_or_else(|| file_stem(&budget.title));
    Ok(Prepared {
        workbook,
        dir,
        stem,
    })
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect::<String>() + ".",
        None => String::new(),
    }
}

/// A file name stem from a budget title: characters file systems refuse
/// become spaces.
fn file_stem(title: &str) -> String {
    let clean: String = title
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => ' ',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    let clean = clean.trim().trim_matches('.').trim();
    let stem: String = clean.chars().take(120).collect();
    if stem.is_empty() {
        "Budget".into()
    } else {
        stem
    }
}

/// The Save dialog's title, filter and suggested file name for `format`.
pub fn dialog_text(format: Format, stem: &str) -> (&'static str, &'static str, String) {
    match format {
        Format::Xlsx => (
            "Export Excel workbook",
            "Excel workbook",
            format!("{stem}.xlsx"),
        ),
        Format::Csv => ("Export CSV files", "CSV files", format!("{stem}.csv")),
        Format::Tsv => (
            "Export tab-delimited detail",
            "Tab-delimited text",
            format!("{stem} Detail.txt"),
        ),
    }
}

/// The extensions the dialog offers for `format` (the first is added when
/// the chosen name has none of them).
fn extensions(format: Format) -> &'static [&'static str] {
    match format {
        Format::Xlsx => &["xlsx"],
        Format::Csv => &["csv"],
        Format::Tsv => &["txt", "tsv"],
    }
}

/// `p`, ending in one of `format`'s extensions.
pub fn with_extension(mut p: PathBuf, format: Format) -> PathBuf {
    let exts = extensions(format);
    let has = p
        .extension()
        .is_some_and(|e| exts.iter().any(|x| e.eq_ignore_ascii_case(x)));
    if !has {
        p.as_mut_os_string().push(format!(".{}", exts[0]));
    }
    p
}

/// Writes `files`, refusing (before writing any) to replace a budget open in
/// a tab. Returns the names of the files written.
pub fn write_files(docs: &Docs, files: &[OutFile]) -> Result<Vec<String>, String> {
    for f in files {
        if docs.find_file(&f.path).is_some() {
            return Err(format!(
                "\"{}\" is a budget open in a tab; export to another name.",
                display_name(&f.path)
            ));
        }
    }
    let mut names = Vec::with_capacity(files.len());
    for f in files {
        write_atomically(&f.path, &f.bytes, Access::Document)
            .map_err(|e| format!("Couldn't write \"{}\": {e}", display_name(&f.path)))?;
        names.push(display_name(&f.path));
    }
    Ok(names)
}

fn display_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.display().to_string())
}

/// `format`: `"xlsx"`, `"csv"` or `"tsv"`. Exports budget `doc` (`None`: the
/// one in front) where a Save dialog says. CSV files start with a byte order
/// mark so Excel reads accented letters; tab-delimited text doesn't, for
/// accounting importers.
#[tauri::command]
pub async fn export_budget<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    format: String,
) -> Result<ExportOutcome, String> {
    let format = Format::parse(&format).ok_or("That export format isn't one the app knows.")?;
    // The tables are taken under the lock; the dialog and the writing happen
    // after it is released, so other commands aren't held up.
    let prepared = {
        let mut s = state.lock_doc(doc)?;
        prepare(&mut s)?
    };
    let (title, filter, name) = dialog_text(format, &prepared.stem);
    let mut dialog = window
        .dialog()
        .file()
        .set_title(title)
        .add_filter(filter, extensions(format))
        .set_file_name(name)
        .set_parent(&window);
    if let Some(dir) = &prepared.dir {
        dialog = dialog.set_directory(dir);
    }
    let Some(picked) = dialog.blocking_save_file() else {
        return Ok(ExportOutcome::default());
    };
    let out = with_extension(picked.into_path().map_err(|e| e.to_string())?, format);
    let opts = Options {
        bom: format == Format::Csv,
    };
    let files = export::render(&prepared.workbook, format, &out, opts)
        .map_err(|e| capitalize(&e.to_string()))?;
    let files = write_files(&state.docs(), &files)?;
    Ok(ExportOutcome { done: true, files })
}

#[cfg(test)]
mod tests {
    use super::*;
    use fwb_core::starter;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fwb-export-{tag}-{}-{}",
            std::process::id(),
            next_n()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn next_n() -> u64 {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        N.fetch_add(1, Ordering::Relaxed)
    }

    #[test]
    fn prepares_every_table_named_after_the_file() {
        let mut s = Session::with_budget(starter::budget(), Some("/films/Pilot v2.fwb".into()));
        let p = prepare(&mut s).unwrap();
        assert_eq!(p.stem, "Pilot v2");
        assert_eq!(p.dir, Some(PathBuf::from("/films")));
        assert_eq!(p.workbook.sheets.len(), export::SHEET_NAMES.len());
        // Untitled: named after the title.
        let mut b = starter::budget();
        b.title = "My Film: Part 2/3".into();
        let p = prepare(&mut Session::with_budget(b, None)).unwrap();
        assert_eq!(p.stem, "My Film  Part 2 3");
        assert_eq!(p.dir, None);
    }

    #[test]
    fn a_budget_with_problems_is_not_exported() {
        let mut b = starter::budget();
        let d = &mut b.categories_mut().next().unwrap().accounts[0].details[0];
        d.rate = "NO_SUCH_GLOBAL * 2".into();
        let e = prepare(&mut Session::with_budget(b, None))
            .err()
            .expect("refused");
        assert!(e.contains("a cell with a problem"), "{e}");
        assert!(e.contains("NO_SUCH_GLOBAL"), "{e}");
        assert!(!e.contains('\u{2014}'), "no em dashes");
    }

    #[test]
    fn dialog_names_and_extensions() {
        assert_eq!(
            dialog_text(Format::Xlsx, "Pilot"),
            (
                "Export Excel workbook",
                "Excel workbook",
                "Pilot.xlsx".into()
            )
        );
        assert_eq!(dialog_text(Format::Csv, "Pilot").2, "Pilot.csv");
        assert_eq!(dialog_text(Format::Tsv, "Pilot").2, "Pilot Detail.txt");
        let p = |s: &str, f| with_extension(PathBuf::from(s), f);
        assert_eq!(p("/x/a", Format::Xlsx), PathBuf::from("/x/a.xlsx"));
        assert_eq!(p("/x/a.XLSX", Format::Xlsx), PathBuf::from("/x/a.XLSX"));
        assert_eq!(p("/x/a.fwb", Format::Xlsx), PathBuf::from("/x/a.fwb.xlsx"));
        assert_eq!(p("/x/v1.2", Format::Csv), PathBuf::from("/x/v1.2.csv"));
        assert_eq!(p("/x/d.tsv", Format::Tsv), PathBuf::from("/x/d.tsv"));
        assert_eq!(p("/x/d", Format::Tsv), PathBuf::from("/x/d.txt"));
        assert_eq!(file_stem("  "), "Budget");
    }

    #[test]
    fn writes_every_file_and_never_an_open_budget() {
        let dir = temp_dir("write");
        let mut s = Session::with_budget(starter::budget(), None);
        let p = prepare(&mut s).unwrap();
        let files = export::render(
            &p.workbook,
            Format::Csv,
            &dir.join("Pilot.csv"),
            Options { bom: true },
        )
        .unwrap();
        let docs = Docs::with(Session::new());
        let names = write_files(&docs, &files).unwrap();
        assert_eq!(names.len(), export::SHEET_NAMES.len());
        assert_eq!(names[0], "Pilot - Topsheet.csv");
        let top = std::fs::read(dir.join("Pilot - Topsheet.csv")).unwrap();
        assert!(top.starts_with(b"\xEF\xBB\xBFType,Code,Description,Total\r\n"));

        // A budget open in a tab is never replaced, and nothing is written.
        let budget_file = dir.join("Open.fwb");
        std::fs::write(&budget_file, fwb_format::to_string(&starter::budget())).unwrap();
        let open = Session::open(&budget_file).unwrap();
        let docs = Docs::with(open);
        let clash = vec![
            OutFile {
                path: dir.join("Other.txt"),
                bytes: b"x".to_vec(),
            },
            OutFile {
                path: budget_file.clone(),
                bytes: b"not a budget".to_vec(),
            },
        ];
        let e = write_files(&docs, &clash).unwrap_err();
        assert!(e.contains("open in a tab"), "{e}");
        assert!(!dir.join("Other.txt").exists());
        assert!(fwb_format::from_str(&std::fs::read_to_string(&budget_file).unwrap()).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn command_is_registered_and_allowed_in_the_main_window_only() {
        const APP_RS: &str = include_str!("app.rs");
        const BUILD_RS: &str = include_str!("../build.rs");
        const MAIN: &str = include_str!("../capabilities/main.json");
        const TOOLS: &str = include_str!("../capabilities/tools.json");
        assert!(APP_RS.contains("crate::export::export_budget,"));
        assert!(BUILD_RS.contains("\"export_budget\","));
        assert!(MAIN.contains("\"allow-export-budget\""));
        assert!(!TOOLS.contains("\"allow-export-budget\""));
    }
}
