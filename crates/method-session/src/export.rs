//! File > Export: the budget as an Excel workbook, CSV files (one per
//! table) or the detail as tab-delimited text for accounting tools. The
//! tables and the file formats are `method_format::export`; this is the
//! budget's side: the totals the grid shows, the file names a Save dialog
//! suggests, and the files' bytes. Where they are written is the host's
//! business (the desktop's Save dialog; a download in a browser).

use std::path::{Path, PathBuf};

use method_format::export::{self, Format, Options, OutFile, Workbook};
use serde::Serialize;

use crate::docs::{AppState, DocId};
use crate::host::{Downloads, FileOut};
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

/// An export error as the page shows it: a sentence.
pub fn capitalize(s: &str) -> String {
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
pub fn extensions(format: Format) -> &'static [&'static str] {
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

/// `format` as the page names it: `"xlsx"`, `"csv"` or `"tsv"`.
pub fn parse_format(format: &str) -> Result<Format, String> {
    Format::parse(format).ok_or_else(|| "That export format isn't one the app knows.".into())
}

/// The files exporting `wb` as `format` to `out` makes, with their bytes.
/// CSV files start with a byte order mark so Excel reads accented letters;
/// tab-delimited text doesn't, for accounting importers.
pub fn render_files(wb: &Workbook, format: Format, out: &Path) -> Result<Vec<OutFile>, String> {
    let opts = Options {
        bom: format == Format::Csv,
    };
    export::render(wb, format, out, opts).map_err(|e| capitalize(&e.to_string()))
}

/// A file's name, without its folder.
pub fn display_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.display().to_string())
}

// ---------------------------------------------------------------------------
// Commands

/// Budget `doc` (`None`: the one in front) exported as `format`, under the
/// names the Save dialog would suggest: one file, or for CSV one per table,
/// beside the desktop's reply as after a save.
pub fn export_budget(
    state: &AppState,
    doc: Option<DocId>,
    format: &str,
) -> Result<Downloads<ExportOutcome>, String> {
    let format = parse_format(format)?;
    let prepared = {
        let mut s = state.lock_doc(doc)?;
        prepare(&mut s)?
    };
    let (_, _, name) = dialog_text(format, &prepared.stem);
    let out = with_extension(PathBuf::from(name), format);
    let downloads: Vec<FileOut> = render_files(&prepared.workbook, format, &out)?
        .into_iter()
        .map(|f| FileOut {
            name: display_name(&f.path),
            bytes: f.bytes,
        })
        .collect();
    Ok(Downloads {
        reply: ExportOutcome {
            done: true,
            files: downloads.iter().map(|f| f.name.clone()).collect(),
        },
        downloads,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use method_core::starter;

    #[test]
    fn prepares_every_table_named_after_the_file() {
        let mut s = Session::with_budget(starter::budget(), Some("/films/Pilot v2.mthd".into()));
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
        assert_eq!(
            p("/x/a.mthd", Format::Xlsx),
            PathBuf::from("/x/a.mthd.xlsx")
        );
        assert_eq!(p("/x/v1.2", Format::Csv), PathBuf::from("/x/v1.2.csv"));
        assert_eq!(p("/x/d.tsv", Format::Tsv), PathBuf::from("/x/d.tsv"));
        assert_eq!(p("/x/d", Format::Tsv), PathBuf::from("/x/d.txt"));
        assert_eq!(file_stem("  "), "Budget");
    }

    #[test]
    fn exports_come_back_as_files_through_the_host() {
        use serde_json::json;
        let h = crate::host::Host::new();
        let files = h
            .dispatch("export_budget", json!({"format": "csv"}))
            .unwrap();
        assert_eq!(files["done"], true);
        let files = files["downloads"].as_array().unwrap();
        assert_eq!(files.len(), export::SHEET_NAMES.len());
        let name = files[0]["name"].as_str().unwrap();
        assert!(name.ends_with(" - Topsheet.csv"), "{name}");
        let x = h
            .dispatch("export_budget", json!({"format": "xlsx"}))
            .unwrap();
        assert_eq!(x["downloads"].as_array().unwrap().len(), 1);
        assert!(x["files"][0].as_str().unwrap().ends_with(".xlsx"));
        let e = h
            .dispatch("export_budget", json!({"format": "pdf"}))
            .unwrap_err();
        assert_eq!(e.message(), "That export format isn't one the app knows.");
    }
}
