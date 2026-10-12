//! The compare screen's commands (decisions.md, entries 17, 19 and 27):
//! what they do is in method-session's `compare` module (re-exported here);
//! these are the Tauri commands, and reading the budget files picked in the
//! native Open dialog. Paths never come from the page: the files are kept in
//! [`CompareFiles`] and the page names them by token. A file that is open in
//! a tab comes back as that tab, so its unsaved changes count.

use std::path::PathBuf;

use tauri::{Runtime, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::app::{AppState, DocId};
use crate::docs::Docs;
use crate::session::DocsFiles;

pub use method_session::compare::*;

/// Reads the files at `paths` for comparing. A file open in a tab comes back
/// as that tab; one that can't be read is an error naming it (nothing is kept).
pub fn pick(docs: &Docs, files: &CompareFiles, paths: &[PathBuf]) -> Result<Vec<Picked>, String> {
    let mut read = Vec::with_capacity(paths.len());
    for p in paths {
        if let Some(doc) = docs.find_file(p) {
            let label = docs.get(doc).map(|s| s.tab_label()).unwrap_or_default();
            read.push(Err(Picked {
                source: Source::Tab { doc },
                label,
                path: p.display().to_string(),
            }));
            continue;
        }
        let (budget, _) = crate::session::read_budget(p)?;
        read.push(Ok((p, budget)));
    }
    Ok(read
        .into_iter()
        .map(|r| match r {
            Ok((p, budget)) => files.add(p, budget),
            Err(tab) => tab,
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Commands (registered in app.rs, build.rs and capabilities/main.json; the
// main window only)

/// Budget `doc` (the one in front when `None`) against `sources`.
#[tauri::command]
pub fn compare_budgets(
    state: State<'_, AppState>,
    files: State<'_, CompareFiles>,
    doc: Option<DocId>,
    sources: Vec<Source>,
) -> Result<CompareDto, String> {
    method_session::compare::compare_budgets(&state, &files, doc, &sources)
}

/// One account's lines in budget `doc` against `sources`.
#[tauri::command]
pub fn compare_detail(
    state: State<'_, AppState>,
    files: State<'_, CompareFiles>,
    doc: Option<DocId>,
    sources: Vec<Source>,
    code: String,
) -> Result<DetailDto, String> {
    method_session::compare::compare_detail(&state, &files, doc, &sources, &code)
}

/// Shows an Open dialog for budgets to compare with and reads the chosen
/// files. Empty when cancelled.
#[tauri::command]
pub async fn compare_pick_files<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    files: State<'_, CompareFiles>,
) -> Result<Vec<Picked>, String> {
    let start = state
        .lock()
        .path()
        .and_then(|p| p.parent())
        .map(PathBuf::from);
    let mut dialog = window
        .dialog()
        .file()
        .set_title("Compare with budgets")
        .add_filter(crate::app::FILTER_NAME, method_format::EXTENSIONS)
        .set_parent(&window);
    if let Some(dir) = start {
        dialog = dialog.set_directory(dir);
    }
    let Some(picked) = dialog.blocking_pick_files() else {
        return Ok(Vec::new());
    };
    let paths = picked
        .into_iter()
        .map(|p| p.into_path().map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    let docs = state.docs();
    pick(&docs, &files, &paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Session;
    use crate::session::SessionFiles;
    use method_core::{Budget, RowId};

    /// The first costed line with a rate, and its account's code.
    fn first_line(b: &Budget) -> (RowId, String) {
        b.categories()
            .flat_map(|c| &c.accounts)
            .find_map(|a| {
                a.details
                    .iter()
                    .find(|d| !d.rate.is_blank())
                    .map(|d| (d.id, a.code.clone()))
            })
            .unwrap()
    }

    fn account<'a>(c: &'a CompareDto, code: &str) -> &'a AccountDto {
        c.categories
            .iter()
            .flat_map(|c| &c.accounts)
            .find(|a| a.code == code && !a.fringe_account)
            .unwrap()
    }

    fn temp_dir() -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "method-compare-test-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn compares_with_files_and_open_files_come_back_as_tabs() {
        let dir = temp_dir();
        let mut b = method_core::starter::budget();
        b.title = "Pilot".into();
        let (row, code) = first_line(&b);
        crate::edit::set_detail_field(&mut b, row, crate::edit::Field::Rate, "7").unwrap();
        let path = dir.join("Pilot.mthd");
        std::fs::write(&path, method_format::to_string(&b)).unwrap();
        let opened = dir.join("Open.mthd");
        std::fs::write(
            &opened,
            method_format::to_string(&method_core::starter::budget()),
        )
        .unwrap();

        let mut docs = Docs::with(Session::new());
        let base = docs.active();
        let open_doc = docs.add(Session::open(&opened).unwrap());
        docs.switch(base).unwrap();
        let files = CompareFiles::default();
        let picked = pick(&docs, &files, &[path.clone(), opened.clone()]).unwrap();
        assert_eq!(picked[0].label, "Pilot");
        assert!(matches!(picked[0].source, Source::File { .. }));
        assert_eq!(picked[1].source, Source::Tab { doc: open_doc });

        let sel: Vec<Source> = picked.iter().map(|p| p.source).collect();
        let c = gather(&mut docs, &files, base, &sel)
            .unwrap()
            .compare()
            .unwrap();
        let kinds: Vec<&str> = c.sources.iter().map(|s| s.kind).collect();
        assert_eq!(kinds, ["base", "file", "tab"]);
        assert_eq!(c.sources[1].detail, path.display().to_string());
        let a = account(&c, &code);
        assert_ne!(a.line.values[0], a.line.values[1]);
        // Sum, average and variance cover both others.
        let agg = a.line.agg.as_ref().unwrap();
        assert_eq!(
            agg.sum,
            a.line.values[1].unwrap() + a.line.values[2].unwrap()
        );

        // An unreadable file is an error naming it; nothing is kept.
        let bad = dir.join("Broken.mthd");
        std::fs::write(&bad, "{ not a budget").unwrap();
        let err = pick(&docs, &files, &[path.clone(), bad]).unwrap_err();
        assert!(err.contains("Broken.mthd"), "{err}");
        assert_eq!(files.kept(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn compare_commands_are_wired_for_the_main_window_only() {
        const APP_RS: &str = include_str!("app.rs");
        const BUILD_RS: &str = include_str!("../build.rs");
        const MAIN_CAPS: &str = include_str!("../capabilities/main.json");
        const TOOLS_CAPS: &str = include_str!("../capabilities/tools.json");
        for c in ["compare_budgets", "compare_detail", "compare_pick_files"] {
            assert!(
                APP_RS.contains(&format!("crate::compare::{c},")),
                "{c} not in generate_handler!"
            );
            assert!(
                BUILD_RS.contains(&format!("\"{c}\",")),
                "{c} not in build.rs"
            );
            let allow = format!("\"allow-{}\"", c.replace('_', "-"));
            assert!(
                MAIN_CAPS.contains(&allow),
                "{c} not allowed for the main window"
            );
            assert!(
                !TOOLS_CAPS.contains(&allow),
                "{c} allowed for the tools window"
            );
        }
        assert!(APP_RS.contains(".manage(crate::compare::CompareFiles::default())"));
    }
}
