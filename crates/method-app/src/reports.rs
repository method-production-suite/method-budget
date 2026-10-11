//! Reports: saved report presets (kept in the budget, one undo step per
//! change) and printing a report, to the print sheet or straight into a PDF
//! file.
//!
//! The report itself is drawn by the page (ui/js/report.js), from
//! `get_full_view`, into page-sized boxes. Printing it hands the main web
//! view to AppKit's print operation with the report's page setup
//! (src/pdf_mac.rs); on other systems the page falls back to
//! `window.print()` and the system dialog's own PDF choice.
//!
//! Security: the PDF's path comes only from the native save dialog shown
//! here; the page can suggest a name, never a path.

use method_core::{Budget, ReportPreset, MAX_PRESETS};
use serde::Serialize;
use tauri::{Runtime, State, WebviewWindow};

use crate::app::{edit_budget, AppState, DocId};
use crate::edit::EditResult;
use crate::view::View;

/// Letter paper, portrait, in points: the size report.js lays pages out for.
pub const LETTER: [f64; 2] = [612.0, 792.0];

/// Saves `preset` as a new preset at the end of the list (`index` `None`), or
/// in place of preset `index`: its name, spec and default flag in one step
/// (so renaming is saving with a new name). Names are unique, ignoring case
/// and surrounding spaces, and stored trimmed. Making a preset the default
/// clears the flag on every other one. Returns where it ended up.
pub fn apply_save_preset(
    b: &mut Budget,
    index: Option<usize>,
    mut preset: ReportPreset,
) -> EditResult<usize> {
    preset.name = preset.name.trim().to_string();
    preset.validate().map_err(|e| capitalize(&e.to_string()))?;
    if let Some(i) = index {
        if i >= b.report_presets.len() {
            return Err("That report preset no longer exists.".into());
        }
    } else if b.report_presets.len() >= MAX_PRESETS {
        return Err(format!(
            "A budget can keep at most {MAX_PRESETS} report presets."
        ));
    }
    let key = preset.name.to_lowercase();
    let clash = b
        .report_presets
        .iter()
        .enumerate()
        .any(|(j, p)| Some(j) != index && p.name.trim().to_lowercase() == key);
    if clash {
        return Err(format!(
            "There is already a report preset called \"{}\".",
            preset.name
        ));
    }
    if preset.default {
        for p in &mut b.report_presets {
            p.default = false;
        }
    }
    Ok(match index {
        Some(i) => {
            b.report_presets[i] = preset;
            i
        }
        None => {
            b.report_presets.push(preset);
            b.report_presets.len() - 1
        }
    })
}

/// Removes preset `index`.
pub fn apply_delete_preset(b: &mut Budget, index: usize) -> EditResult {
    if index >= b.report_presets.len() {
        return Err("That report preset no longer exists.".into());
    }
    b.report_presets.remove(index);
    Ok(())
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect::<String>() + ".",
        None => String::new(),
    }
}

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
    let preset = method_format::report_preset_from_value(preset)
        .map_err(|e| format!("That report setup can't be saved: {e}"))?;
    edit_budget(
        &window,
        &state,
        doc,
        if index.is_some() {
            "Update a report preset"
        } else {
            "Save a report preset"
        },
        |b| apply_save_preset(b, index, preset),
    )
}

/// `{index}`.
#[tauri::command]
pub fn delete_report_preset<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    index: usize,
) -> Result<View, String> {
    edit_budget(&window, &state, doc, "Delete a report preset", |b| {
        apply_delete_preset(b, index)
    })
}

/// How a print or PDF request ended.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct PrintOutcome {
    /// False when this system can't print this way: the page uses
    /// `window.print()` instead.
    pub supported: bool,
    /// True when it printed, or the PDF was written; false when cancelled.
    pub done: bool,
    /// The PDF's file name, when one was written.
    pub file: Option<String>,
}

/// A file name for `title` (no path separators or other characters file
/// systems refuse), ending in `.pdf`.
pub fn pdf_file_name(title: &str) -> String {
    let clean: String = title
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    let clean = clean.trim().trim_start_matches('.').trim();
    let stem: String = clean.chars().take(120).collect();
    let stem = if stem.is_empty() { "Report" } else { &stem };
    format!("{stem}.pdf")
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
                .set_title("Save report as PDF")
                .add_filter("PDF", &["pdf"])
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
            .map_err(|_| "Printing stopped unexpectedly.".to_string())??;
        if done {
            if let Some(p) = &path {
                if !p.is_file() {
                    return Err("The PDF could not be written there.".into());
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
    use super::*;
    use crate::session::Session;
    use method_core::{ReportSection, ReportSpec};

    fn preset(name: &str, default: bool) -> ReportPreset {
        ReportPreset {
            name: name.into(),
            default,
            spec: ReportSpec::default(),
        }
    }

    fn names(s: &Session) -> Vec<(String, bool)> {
        s.budget()
            .report_presets
            .iter()
            .map(|p| (p.name.clone(), p.default))
            .collect()
    }

    #[test]
    fn save_rename_default_delete() {
        let mut s = Session::new();
        assert_eq!(
            s.edit("Test edit", |b| apply_save_preset(
                b,
                None,
                preset("  Studio  ", true)
            ))
            .unwrap(),
            0
        );
        s.edit("Test edit", |b| {
            apply_save_preset(b, None, preset("Crew", false))
        })
        .unwrap();
        assert_eq!(
            names(&s),
            vec![("Studio".into(), true), ("Crew".into(), false)]
        );
        // One default: making Crew the default clears Studio's flag.
        s.edit("Test edit", |b| {
            apply_save_preset(b, Some(1), preset("Crew", true))
        })
        .unwrap();
        assert_eq!(
            names(&s),
            vec![("Studio".into(), false), ("Crew".into(), true)]
        );
        // Rename in place, and change the spec in the same step.
        let mut p = preset("Crew pack", true);
        p.spec.sections = vec![ReportSection::Notes];
        s.edit("Test edit", |b| apply_save_preset(b, Some(1), p.clone()))
            .unwrap();
        assert_eq!(s.budget().report_presets[1], p);
        // Each change is one undo step.
        assert!(s.undo());
        assert_eq!(s.budget().report_presets[1].name, "Crew");
        assert!(s.redo());
        s.edit("Test edit", |b| apply_delete_preset(b, 0)).unwrap();
        assert_eq!(names(&s), vec![("Crew pack".into(), true)]);
        assert!(s.undo());
        assert_eq!(s.budget().report_presets.len(), 2);
    }

    #[test]
    fn refused_changes_leave_the_budget_alone() {
        let mut s = Session::new();
        s.edit("Test edit", |b| {
            apply_save_preset(b, None, preset("Studio", false))
        })
        .unwrap();
        s.edit("Test edit", |b| {
            apply_save_preset(b, None, preset("Crew", false))
        })
        .unwrap();
        let before = s.budget().clone();
        let dup = s.edit("Test edit", |b| {
            apply_save_preset(b, None, preset(" studio ", false))
        });
        assert!(dup.unwrap_err().contains("already"));
        // Renaming onto another preset's name is refused too; keeping its
        // own name is fine.
        assert!(s
            .edit("Test edit", |b| apply_save_preset(
                b,
                Some(1),
                preset("STUDIO", false)
            ))
            .is_err());
        assert!(s
            .edit("Test edit", |b| apply_save_preset(
                b,
                None,
                preset("   ", false)
            ))
            .is_err());
        assert!(s
            .edit("Test edit", |b| apply_save_preset(
                b,
                Some(5),
                preset("X", false)
            ))
            .is_err());
        assert!(s.edit("Test edit", |b| apply_delete_preset(b, 2)).is_err());
        let mut twice = preset("Twice", false);
        twice.spec.sections = vec![ReportSection::Topsheet, ReportSection::Topsheet];
        let e = s
            .edit("Test edit", |b| apply_save_preset(b, None, twice))
            .unwrap_err();
        assert!(e.starts_with("The section"), "{e}");
        assert_eq!(s.budget(), &before);
        s.edit("Test edit", |b| {
            apply_save_preset(b, Some(1), preset("crew", false))
        })
        .unwrap();
        assert_eq!(s.budget().report_presets[1].name, "crew");
    }

    #[test]
    fn at_most_max_presets() {
        let mut s = Session::new();
        s.edit("Test edit", |b| {
            for i in 0..MAX_PRESETS {
                apply_save_preset(b, None, preset(&format!("P{i}"), false))?;
            }
            Ok(())
        })
        .unwrap();
        assert!(s
            .edit("Test edit", |b| apply_save_preset(
                b,
                None,
                preset("One more", false)
            ))
            .is_err());
        // Replacing one is still fine at the limit.
        s.edit("Test edit", |b| {
            apply_save_preset(b, Some(0), preset("First", false))
        })
        .unwrap();
    }

    #[test]
    fn presets_come_from_the_file_shape() {
        let v = serde_json::json!({
            "name": "Topsheet only",
            "default": true,
            "sections": ["topsheet"],
            "landscape": true
        });
        let p = method_format::report_preset_from_value(v).unwrap();
        let mut s = Session::new();
        s.edit("Test edit", |b| apply_save_preset(b, None, p))
            .unwrap();
        let view = serde_json::to_value(s.view()).unwrap();
        assert_eq!(
            view["budget"]["report_presets"],
            serde_json::json!([{
                "name": "Topsheet only",
                "default": true,
                "sections": ["topsheet"],
                "landscape": true
            }])
        );
    }

    #[test]
    fn pdf_names() {
        assert_eq!(pdf_file_name("The Long Night"), "The Long Night.pdf");
        assert_eq!(pdf_file_name("A/B: C?"), "A-B- C-.pdf");
        assert_eq!(pdf_file_name("  "), "Report.pdf");
        assert_eq!(pdf_file_name("..hidden"), "hidden.pdf");
        assert_eq!(pdf_file_name("line\nbreak"), "line break.pdf");
        assert_eq!(pdf_file_name(&"x".repeat(300)).len(), 124);
    }

    #[test]
    fn commands_are_registered_and_allowed() {
        const APP_RS: &str = include_str!("app.rs");
        const BUILD_RS: &str = include_str!("../build.rs");
        const MAIN: &str = include_str!("../capabilities/main.json");
        const TOOLS: &str = include_str!("../capabilities/tools.json");
        for c in ["save_report_preset", "delete_report_preset", "print_report"] {
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
