//! Reports: saved report presets (kept in the budget, one undo step per
//! change), and what printing a report needs that isn't the window's: the
//! page size and a PDF's file name.
//!
//! The report itself is drawn by the page (ui/js/report.js), from
//! `get_full_view`, into page-sized boxes. Printing it is the host's
//! (the desktop's print sheet; a browser's own print dialog).

use method_core::{Budget, ReportPreset, MAX_PRESETS};
use serde::Serialize;

use crate::edit::EditResult;
use crate::session::Session;

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

// ---------------------------------------------------------------------------
// Commands

/// Saves `preset` (in the file format's shape, docs/FORMAT.md, "Report
/// preset") as a new preset (`index` `None`) or in place of preset `index`,
/// as one undo step. Where it ended up.
pub fn save_report_preset(
    s: &mut Session,
    index: Option<usize>,
    preset: serde_json::Value,
) -> EditResult<usize> {
    let preset = method_format::report_preset_from_value(preset)
        .map_err(|e| format!("That report setup can't be saved: {e}"))?;
    let label = if index.is_some() {
        "Update a report preset"
    } else {
        "Save a report preset"
    };
    s.edit(label, |b| apply_save_preset(b, index, preset))
}

/// Removes preset `index`, as one undo step.
pub fn delete_report_preset(s: &mut Session, index: usize) -> EditResult {
    s.edit("Delete a report preset", |b| apply_delete_preset(b, index))
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
    fn presets_are_saved_and_deleted_through_the_host() {
        use serde_json::json;
        let h = crate::host::Host::new();
        let v = h
            .dispatch(
                "save_report_preset",
                json!({"index": null, "preset": {"name": "Studio", "sections": ["topsheet"]}}),
            )
            .unwrap();
        assert_eq!(v["budget"]["report_presets"][0]["name"], "Studio");
        let steps = h.dispatch("history_list", json!({})).unwrap();
        let last = steps["steps"].as_array().unwrap().last().unwrap().clone();
        assert_eq!(last["label"], "Save a report preset");
        let e = h
            .dispatch(
                "save_report_preset",
                json!({"index": null, "preset": {"name": 3}}),
            )
            .unwrap_err();
        assert!(
            e.message().starts_with("That report setup can't be saved"),
            "{e}"
        );
        let v = h
            .dispatch("delete_report_preset", json!({"index": 0}))
            .unwrap();
        assert!(v["budget"]["report_presets"].get(0).is_none(), "{v}");
    }
}
