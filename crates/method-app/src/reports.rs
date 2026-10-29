//! Reports: saved report presets and printing a report, to the print sheet
//! or straight into a PDF file. The presets and what printing needs that
//! isn't the window's are method-session's `reports` module (re-exported
//! here); these are the Tauri commands and the printing.
//!
//! The report itself is drawn by the page (ui/js/report.js), from
//! `get_full_view`, into page-sized boxes. Printing it hands the main web
//! view to AppKit's print operation with the report's page setup
//! (src/pdf_mac.rs); on other systems the page falls back to
//! `window.print()` and the system dialog's own PDF choice.
//!
//! Security: the PDF's path comes only from the native save dialog shown
//! here; the page can suggest a name, never a path.

use tauri::{Runtime, State, WebviewWindow};

use crate::app::{apply, AppState, DocId};
use crate::view::View;

pub use method_session::reports::*;

/// `{index: number | null, preset}`, the preset in the file format's shape
/// (docs/FORMAT.md, "Report preset").
#[tauri::command]
pub fn save_report_preset<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    index: Option<usize>,
    preset: serde_json::Value,
) -> Result<View, String> {
    apply(&window, &state, doc, |s| {
        method_session::reports::save_report_preset(s, index, preset)
    })
}

/// `{index}`.
#[tauri::command]
pub fn delete_report_preset<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    index: usize,
) -> Result<View, String> {
    apply(&window, &state, doc, |s| {
        method_session::reports::delete_report_preset(s, index)
    })
}

/// `{layout}`: a bid layout in the file format's shape (docs/FORMAT.md, "Bid
/// layout"; `null`: the example) worked out on the current totals. Changes
/// nothing.
#[tauri::command]
pub fn bid_sheet(
    state: State<'_, AppState>,
    doc: Option<DocId>,
    layout: Option<serde_json::Value>,
) -> Result<BidSheetDto, String> {
    let mut s = state.lock_doc(doc)?;
    crate::i18n::engine_err(method_session::reports::bid_sheet(&mut s, layout))
}

/// Prints what the main window's page shows for print media (the page has
/// drawn the report into `#print-root` first): with `pdf`, into a PDF file
/// chosen in a Save dialog, with no print sheet; otherwise through the print
/// sheet. `landscape` sets the orientation of the Letter page. `title` names
/// the job, and suggests the PDF's file name. Waits until the job is over.
#[tauri::command]
pub async fn print_report<R: Runtime>(
    window: WebviewWindow<R>,
    pdf: bool,
    landscape: bool,
    title: String,
) -> Result<PrintOutcome, String> {
    let title: String = title.trim().chars().take(200).collect();
    platform::print(window, pdf, landscape, title).await
}

#[cfg(target_os = "macos")]
mod platform {
    use std::sync::mpsc;

    use tauri::{Runtime, WebviewWindow};
    use tauri_plugin_dialog::DialogExt;

    use super::{pdf_file_name, PrintOutcome, LETTER};
    use crate::pdf_mac::{self, PageSetup};

    pub async fn print<R: Runtime>(
        window: WebviewWindow<R>,
        pdf: bool,
        landscape: bool,
        title: String,
    ) -> Result<PrintOutcome, String> {
        let path = if pdf {
            let picked = window
                .dialog()
                .file()
                .set_title(crate::i18n::tr("dialog.save_report_as_pdf"))
                .add_filter(crate::i18n::tr("dialog.filter_pdf"), &["pdf"])
                .set_file_name(pdf_file_name(&title))
                .set_parent(&window)
                .blocking_save_file();
            let Some(picked) = picked else {
                return Ok(PrintOutcome {
                    supported: true,
                    ..PrintOutcome::default()
                });
            };
            let mut p = picked.into_path().map_err(|e| e.to_string())?;
            if !p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
                p.as_mut_os_string().push(".pdf");
            }
            Some(p)
        } else {
            None
        };

        let setup = PageSetup {
            paper: LETTER,
            landscape,
            margins: [0.0; 4],
            title,
        };
        let (tx, rx) = mpsc::channel::<Result<bool, String>>();
        let started = tx.clone();
        let target = path.clone();
        window
            .with_webview(move |w| {
                let done = Box::new(move |ok: bool| {
                    let _ = tx.send(Ok(ok));
                });
                // SAFETY: Tauri hands us the live WKWebView and its NSWindow,
                // on the main thread.
                let run = unsafe {
                    pdf_mac::run(w.inner(), w.ns_window(), &setup, target.as_deref(), done)
                };
                if let Err(e) = run {
                    let _ = started.send(Err(e));
                }
            })
            .map_err(|e| e.to_string())?;
        let done = tauri::async_runtime::spawn_blocking(move || rx.recv())
            .await
            .map_err(|e| e.to_string())?
            .map_err(|_| crate::i18n::tr("report.printing_stopped"))??;
        if done {
            if let Some(p) = &path {
                if !p.is_file() {
                    return Err(crate::i18n::tr("report.pdf_not_written"));
                }
            }
        }
        Ok(PrintOutcome {
            supported: true,
            done,
            file: path
                .filter(|_| done)
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned())),
        })
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use tauri::{Runtime, WebviewWindow};

    use super::PrintOutcome;

    /// Not built for this system yet: the page prints with `window.print()`.
    pub async fn print<R: Runtime>(
        _window: WebviewWindow<R>,
        _pdf: bool,
        _landscape: bool,
        _title: String,
    ) -> Result<PrintOutcome, String> {
        Ok(PrintOutcome::default())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn commands_are_registered_and_allowed() {
        const APP_RS: &str = include_str!("app.rs");
        const BUILD_RS: &str = include_str!("../build.rs");
        const MAIN: &str = include_str!("../capabilities/main.json");
        const TOOLS: &str = include_str!("../capabilities/tools.json");
        for c in [
            "save_report_preset",
            "delete_report_preset",
            "bid_sheet",
            "print_report",
        ] {
            assert!(
                APP_RS.contains(&format!("crate::reports::{c},")),
                "{c} not in generate_handler!"
            );
            assert!(
                BUILD_RS.contains(&format!("\"{c}\",")),
                "{c} not in build.rs"
            );
            let allow = format!("\"allow-{}\"", c.replace('_', "-"));
            assert!(MAIN.contains(&allow), "{c} not allowed in the main window");
            assert!(
                !TOOLS.contains(&allow),
                "{c} must not reach the tools window"
            );
        }
    }
}
