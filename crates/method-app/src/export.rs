//! File > Export: what is exported is method-session's `export` module
//! (re-exported here); this is the app's side: the Save dialog and writing
//! the files.
//!
//! Security: the page names only the format and the budget (`doc`); the path
//! comes from the native Save dialog shown here, and the extension is the
//! format's. Each file is written atomically (`safety::write_atomically`),
//! and an export never replaces a budget that is open in a tab.

use method_format::export::OutFile;
use tauri::{Runtime, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::app::{AppState, DocId};
use crate::docs::Docs;
use crate::safety::{write_atomically, Access};
use crate::session::DocsFiles;

pub use method_session::export::*;

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
    let format = parse_format(&format)?;
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
    let files = render_files(&prepared.workbook, format, &out)?;
    let files = write_files(&state.docs(), &files)?;
    Ok(ExportOutcome { done: true, files })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::session::{Session, SessionFiles};
    use method_core::starter;
    use method_format::export::{self, Format, Options};

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "method-export-{tag}-{}-{}",
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
        let budget_file = dir.join("Open.mthd");
        std::fs::write(&budget_file, method_format::to_string(&starter::budget())).unwrap();
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
        assert!(method_format::from_str(&std::fs::read_to_string(&budget_file).unwrap()).is_ok());
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
