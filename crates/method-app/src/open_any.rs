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
//! Paths stay here: the page gets a view, a review or the sheet dialog's
//! start, each naming the file by a token, as the import commands do. What
//! a file is to Open, and keeping an export or a spreadsheet by token, are
//! method-session's `open_any` module (re-exported here).

use std::path::Path;

use tauri::{Manager, Runtime, WebviewWindow};

use crate::app::{finish, AppState};
use crate::export_import::ExportSources;
use crate::safety_app::Safety;
use crate::sheet_import::{read_capped, SheetSources};

pub use method_session::open_any::*;

/// Opens the file at `path` the way its content asks (above). A budget is
/// opened in a tab and listed in Open Recent; an export or a spreadsheet is
/// read and kept by token for its review. Nothing changes when the file
/// can't be read.
pub(crate) fn open_any<R: Runtime>(
    window: &WebviewWindow<R>,
    state: &AppState,
    path: &Path,
) -> Result<Opened, String> {
    let file = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    // A `.mthd` is read by the budget reader alone, as it always was.
    let (bytes, plan) = if method_format::has_budget_extension(path) {
        (Vec::new(), Plan::Budget)
    } else {
        let bytes = read_capped(path)?;
        let plan = plan(&file, &bytes)?;
        (bytes, plan)
    };
    open_planned(
        plan,
        file,
        bytes,
        Some(path.to_path_buf()),
        &window.state::<ExportSources>(),
        &window.state::<SheetSources>(),
        |_| {
            let safety = window.state::<Safety>();
            // Opens it in a tab and lists it in Open Recent (src/safety_app.rs).
            let id = crate::safety_app::open_path(state, &safety, path)?;
            crate::safety_app::refresh_menu(window.app_handle());
            Ok(finish(window, &mut state.lock_doc(Some(id))?, None))
        },
    )
}

#[cfg(test)]
mod tests {
    use method_format::import::exchange;

    /// Finder offers the app for budget exports on macOS only, as a second
    /// choice (entry 67): tauri.macos.conf.json replaces the associations
    /// there and must keep the `.mthd` one as it is everywhere else.
    #[test]
    fn macos_offers_the_app_for_exports_as_a_second_choice() {
        let parse = |s: &str| serde_json::from_str::<serde_json::Value>(s).unwrap();
        let all = parse(include_str!("../tauri.conf.json"));
        let mac = parse(include_str!("../tauri.macos.conf.json"));
        let base = all["bundle"]["fileAssociations"].as_array().unwrap();
        let ours = mac["bundle"]["fileAssociations"].as_array().unwrap();
        // Every platform: .mthd only, so Windows and Linux register nothing else.
        assert_eq!(base.len(), 1);
        assert_eq!(base[0]["ext"], serde_json::json!(["mthd"]));
        // macOS: the same .mthd association, first and unchanged.
        assert_eq!(ours[0], base[0]);
        // Only the associations differ from the shared file.
        assert_eq!(mac.as_object().unwrap().len(), 1);
        assert_eq!(mac["bundle"].as_object().unwrap().len(), 1);
        // The rest: existing system types, viewer, never the default.
        let mut exts = Vec::new();
        for a in &ours[1..] {
            assert_eq!(a["role"], "Viewer", "{a}");
            assert_eq!(a["rank"], "Alternate", "{a}");
            assert!(a.get("exportedType").is_none(), "{a}");
            for t in a["contentTypes"].as_array().unwrap() {
                assert!(
                    t.as_str().unwrap().starts_with("public.")
                        || t.as_str().unwrap().starts_with("org.openxmlformats."),
                    "{t}"
                );
            }
            for e in a["ext"].as_array().unwrap() {
                exts.push(e.as_str().unwrap().to_string());
            }
        }
        // Each one is an extension the budget export reader takes.
        for e in &exts {
            assert!(exchange::READABLE_EXTENSIONS.contains(&e.as_str()), "{e}");
        }
        for e in ["json", "xml", "xlsx"] {
            assert!(exts.iter().any(|x| x == e), "{e}");
        }
        // Not plain text: every text and source file conforms to it.
        assert!(!exts.iter().any(|x| x == "txt"));
    }
}
