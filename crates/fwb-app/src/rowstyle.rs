//! Row semantics that never touch the math: text style (bold, italic, indent,
//! color), per-row notes and group colors. Separator rows are added through
//! `add_detail` with kind `"separator"`.
//!
//! Each command is one undo step and either applies completely or returns a
//! message and leaves the budget alone (it runs through `Session::edit`, and
//! every input is checked before anything changes).

use std::collections::BTreeSet;

use fwb_core::{Budget, Detail, RowId, MAX_INDENT, MAX_NOTE_CHARS};
use serde::{Deserialize, Deserializer};
use tauri::ipc::{InvokeBody, Request};
use tauri::{Runtime, State, WebviewWindow};

use crate::app::{edit_budget, AppState};
use crate::edit::EditResult;
use crate::view::View;

/// A color typed by the user: `#` and exactly 6 hex digits. Stored lowercase,
/// so the same color is always written the same way.
pub fn check_color(s: &str) -> EditResult<String> {
    let t = s.trim();
    match t.strip_prefix('#') {
        Some(hex) if hex.len() == 6 && hex.bytes().all(|c| c.is_ascii_hexdigit()) => {
            Ok(t.to_ascii_lowercase())
        }
        _ => Err(format!(
            "`{t}` is not a color: use # and 6 hex digits, like #1a2b3c"
        )),
    }
}

/// `Some(None)` for an explicit JSON `null`, so "clear" and "leave alone"
/// (field absent, `None`) differ.
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(d).map(Some)
}

/// `set_row_style`'s arguments. Only the fields that are present change.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RowStyleInput {
    pub row_ids: Vec<RowId>,
    #[serde(default)]
    pub bold: Option<bool>,
    #[serde(default)]
    pub italic: Option<bool>,
    /// 0 to [`MAX_INDENT`]. Wider than `u8` so an out-of-range number gets a
    /// readable message instead of a parse error.
    #[serde(default)]
    pub indent: Option<i64>,
    /// `Some(Some(c))` sets, `Some(None)` (JSON `null`) clears, `None` (absent)
    /// leaves each row's color alone.
    #[serde(default, deserialize_with = "present")]
    pub color: Option<Option<String>>,
}

impl RowStyleInput {
    /// Reads the arguments object of an IPC call. Tauri's named arguments
    /// can't tell an absent `color` from `color: null`, so the command reads
    /// the whole object itself.
    pub fn from_json(v: &serde_json::Value) -> EditResult<RowStyleInput> {
        RowStyleInput::deserialize(v).map_err(|e| format!("bad row style: {e}"))
    }
}

/// Every row in `ids`, or an error naming the first one that is gone.
fn rows_mut<'a>(b: &'a mut Budget, ids: &[RowId]) -> EditResult<Vec<&'a mut Detail>> {
    let wanted: BTreeSet<RowId> = ids.iter().copied().collect();
    let rows: Vec<&mut Detail> = b
        .categories_mut()
        .flat_map(|c| &mut c.accounts)
        .flat_map(|a| &mut a.details)
        .filter(|d| wanted.contains(&d.id))
        .collect();
    let found: BTreeSet<RowId> = rows.iter().map(|d| d.id).collect();
    if let Some(gone) = wanted.difference(&found).next() {
        return Err(format!("row {gone} no longer exists"));
    }
    Ok(rows)
}

/// Sets bold, italic, indent and/or color on several rows of any kind.
pub fn apply_row_style(b: &mut Budget, s: &RowStyleInput) -> EditResult {
    if s.row_ids.is_empty() {
        return Err("no rows to style".into());
    }
    let indent = match s.indent {
        None => None,
        Some(n) => Some(
            u8::try_from(n)
                .ok()
                .filter(|n| *n <= MAX_INDENT)
                .ok_or_else(|| format!("indent must be 0 to {MAX_INDENT}, not {n}"))?,
        ),
    };
    let color = match &s.color {
        None => None,
        Some(None) => Some(None),
        Some(Some(c)) => Some(Some(check_color(c)?)),
    };
    for d in rows_mut(b, &s.row_ids)? {
        if let Some(bold) = s.bold {
            d.bold = bold;
        }
        if let Some(italic) = s.italic {
            d.italic = italic;
        }
        if let Some(indent) = indent {
            d.indent = indent;
        }
        if let Some(color) = &color {
            d.color.clone_from(color);
        }
    }
    Ok(())
}

/// Sets or clears (`None`, or text that is only whitespace) a row's note. The
/// text is kept as typed.
pub fn apply_row_note(b: &mut Budget, id: RowId, note: Option<&str>) -> EditResult {
    let note = note.filter(|n| !n.trim().is_empty());
    if let Some(n) = note {
        let len = n.chars().count();
        if len > MAX_NOTE_CHARS {
            return Err(format!(
                "that note is {len} characters; notes can be at most {MAX_NOTE_CHARS}"
            ));
        }
    }
    let [d] = &mut rows_mut(b, &[id])?[..] else {
        return Err(format!("row {id} is in the budget twice"));
    };
    d.note = note.map(String::from);
    Ok(())
}

/// Sets or clears (`None`) the display color of a group's lines.
pub fn apply_group_color(b: &mut Budget, name: &str, color: Option<&str>) -> EditResult {
    let color = color.map(check_color).transpose()?;
    let g = b
        .groups
        .get_mut(name)
        .ok_or_else(|| format!("group {name} no longer exists"))?;
    g.color = color;
    Ok(())
}

// ---------------------------------------------------------------------------
// Commands

/// `{rowIds, bold?, italic?, indent?, color?: string | null}`.
#[tauri::command]
pub fn set_row_style<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    request: Request<'_>,
) -> Result<View, String> {
    let InvokeBody::Json(args) = request.body() else {
        return Err("bad row style: expected JSON arguments".into());
    };
    let input = RowStyleInput::from_json(args)?;
    edit_budget(&window, &state, |b| apply_row_style(b, &input))
}

/// `{rowId, note: string | null}`.
#[tauri::command]
pub fn set_row_note<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    row_id: RowId,
    note: Option<String>,
) -> Result<View, String> {
    edit_budget(&window, &state, |b| {
        apply_row_note(b, row_id, note.as_deref())
    })
}

/// `{name, color: string | null}`.
#[tauri::command]
pub fn set_group_color<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    name: String,
    color: Option<String>,
) -> Result<View, String> {
    edit_budget(&window, &state, |b| {
        apply_group_color(b, &name, color.as_deref())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::{self, GroupInput};
    use crate::session::Session;
    use fwb_core::RowKind;
    use serde_json::json;

    fn row(s: &Session, id: RowId) -> &Detail {
        edit::details(s.budget()).find(|d| d.id == id).unwrap()
    }

    /// The first account with at least two rows, and its first two row ids.
    fn two_rows(s: &Session) -> (String, RowId, RowId) {
        let a = s
            .budget()
            .categories()
            .flat_map(|c| &c.accounts)
            .find(|a| a.details.len() >= 2)
            .unwrap();
        (a.code.clone(), a.details[0].id, a.details[1].id)
    }

    fn style(v: serde_json::Value) -> RowStyleInput {
        RowStyleInput::from_json(&v).unwrap()
    }

    #[test]
    fn colors() {
        assert_eq!(check_color(" #A1b2C3 ").unwrap(), "#a1b2c3");
        for bad in [
            "",
            "#",
            "a1b2c3",
            "#a1b2c",
            "#a1b2c3d",
            "#g1b2c3",
            "#a1b2c3 x",
            "##a1b2c",
        ] {
            assert!(check_color(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn style_arguments_tell_absent_from_null() {
        let s = style(json!({"rowIds": [1, 2], "bold": true}));
        assert_eq!(s.row_ids, [1, 2]);
        assert_eq!((s.bold, s.italic, s.indent), (Some(true), None, None));
        assert_eq!(s.color, None, "absent leaves color alone");
        assert_eq!(
            style(json!({"rowIds": [1], "color": null})).color,
            Some(None)
        );
        assert_eq!(
            style(json!({"rowIds": [1], "color": "#000000"})).color,
            Some(Some("#000000".into()))
        );
        assert!(RowStyleInput::from_json(&json!({"bold": true})).is_err());
        assert!(RowStyleInput::from_json(&json!({"rowIds": [1], "indent": 1.5})).is_err());
    }

    #[test]
    fn style_changes_only_given_fields_as_one_undo_step() {
        let mut s = Session::new();
        let (_, a, b) = two_rows(&s);
        let before = s.view().report;
        s.edit(|bud| {
            apply_row_style(
                bud,
                &style(json!({"rowIds": [a, b], "bold": true, "indent": 2, "color": "#ABCDEF"})),
            )
        })
        .unwrap();
        s.edit(|bud| apply_row_style(bud, &style(json!({"rowIds": [b], "italic": true}))))
            .unwrap();
        for id in [a, b] {
            let d = row(&s, id);
            assert!(d.bold);
            assert_eq!(d.indent, 2);
            assert_eq!(d.color.as_deref(), Some("#abcdef"));
        }
        assert!(!row(&s, a).italic && row(&s, b).italic);
        assert_eq!(s.view().report, before, "style never changes totals");

        s.edit(|bud| apply_row_style(bud, &style(json!({"rowIds": [a], "color": null}))))
            .unwrap();
        assert_eq!(row(&s, a).color, None);
        assert_eq!(row(&s, b).color.as_deref(), Some("#abcdef"));

        // The view carries the new fields in file-format JSON.
        let v = serde_json::to_value(s.view()).unwrap();
        let text = v["budget"].to_string();
        assert!(text.contains("\"italic\":true") && text.contains("\"indent\":2"));

        assert!(s.undo());
        assert_eq!(row(&s, a).color.as_deref(), Some("#abcdef"));
        assert!(s.undo());
        assert!(!row(&s, b).italic && row(&s, b).bold);
        assert!(s.undo());
        assert!(!row(&s, a).bold && row(&s, a).indent == 0);
        assert!(!s.can_undo());
    }

    #[test]
    fn bad_style_changes_nothing() {
        let mut s = Session::new();
        let (_, a, b) = two_rows(&s);
        let original = s.budget().clone();
        for bad in [
            json!({"rowIds": [], "bold": true}),
            json!({"rowIds": [a, b], "bold": true, "indent": 5}),
            json!({"rowIds": [a, b], "bold": true, "indent": -1}),
            json!({"rowIds": [a, b], "bold": true, "color": "blue"}),
            json!({"rowIds": [a, 999_999], "bold": true}),
        ] {
            let input = style(bad.clone());
            assert!(s.edit(|bud| apply_row_style(bud, &input)).is_err(), "{bad}");
            assert_eq!(s.budget(), &original, "{bad}");
        }
        assert!(!s.can_undo() && !s.is_dirty());
        // Indent 4 is the deepest allowed.
        let ok = style(json!({"rowIds": [a], "indent": 4}));
        s.edit(|bud| apply_row_style(bud, &ok)).unwrap();
        assert_eq!(row(&s, a).indent, 4);
    }

    #[test]
    fn notes() {
        let mut s = Session::new();
        let (_, a, _) = two_rows(&s);
        s.edit(|b| apply_row_note(b, a, Some("  Ask about\nkit fee  ")))
            .unwrap();
        assert_eq!(row(&s, a).note.as_deref(), Some("  Ask about\nkit fee  "));
        s.edit(|b| apply_row_note(b, a, Some("   "))).unwrap();
        assert_eq!(row(&s, a).note, None, "blank clears");
        s.edit(|b| apply_row_note(b, a, Some("x"))).unwrap();
        s.edit(|b| apply_row_note(b, a, None)).unwrap();
        assert_eq!(row(&s, a).note, None);

        let longest = "\u{e9}".repeat(MAX_NOTE_CHARS);
        s.edit(|b| apply_row_note(b, a, Some(&longest))).unwrap();
        let before = s.budget().clone();
        let too_long = format!("{longest}x");
        assert!(s.edit(|b| apply_row_note(b, a, Some(&too_long))).is_err());
        assert!(s.edit(|b| apply_row_note(b, 999_999, Some("x"))).is_err());
        assert_eq!(s.budget(), &before);
        assert!(s.undo());
        assert_eq!(row(&s, a).note, None);
    }

    #[test]
    fn group_colors() {
        let mut s = Session::new();
        let g = GroupInput {
            description: "Option A".into(),
            in_budget_total: true,
        };
        s.edit(|b| edit::set_group(b, "Alt", &g)).unwrap();
        s.edit(|b| apply_group_color(b, "Alt", Some("#00FF7F")))
            .unwrap();
        assert_eq!(s.budget().groups["Alt"].color.as_deref(), Some("#00ff7f"));

        // Editing the group elsewhere keeps its color.
        let g2 = GroupInput {
            description: "Option A, revised".into(),
            in_budget_total: false,
        };
        s.edit(|b| edit::set_group(b, "Alt", &g2)).unwrap();
        assert_eq!(s.budget().groups["Alt"].color.as_deref(), Some("#00ff7f"));

        let before = s.budget().clone();
        assert!(s
            .edit(|b| apply_group_color(b, "Alt", Some("#00ff7")))
            .is_err());
        assert!(s
            .edit(|b| apply_group_color(b, "Nope", Some("#000000")))
            .is_err());
        assert_eq!(s.budget(), &before);

        s.edit(|b| apply_group_color(b, "Alt", None)).unwrap();
        assert_eq!(s.budget().groups["Alt"].color, None);
        assert!(s.undo());
        assert_eq!(s.budget().groups["Alt"].color.as_deref(), Some("#00ff7f"));
    }

    #[test]
    fn separators_are_added_like_any_row_and_cost_nothing() {
        let mut s = Session::new();
        let (account, a, _) = two_rows(&s);
        let before = s.view();
        let id = s.add_detail(&account, Some(a), "separator").unwrap();
        let sep = row(&s, id);
        assert_eq!(sep.kind, RowKind::Separator);
        let acct = s
            .budget()
            .categories()
            .flat_map(|c| &c.accounts)
            .find(|x| x.code == account)
            .unwrap();
        assert_eq!(acct.details[1].id, id, "inserted after row a");

        let after = s.view();
        let (r0, r1) = (before.report.unwrap(), after.report.unwrap());
        assert_eq!(r0.grand_total, r1.grand_total);
        assert_eq!(r0.net_total, r1.net_total);
        assert!(after.issues.is_empty());
        let v = serde_json::to_value(s.view()).unwrap();
        assert!(v["budget"].to_string().contains("\"kind\":\"separator\""));

        // Separators use no columns, but take style and notes.
        assert!(s.set_detail_field(id, "description", "x").is_err());
        assert!(s.set_detail_field(id, "rate", "1").is_err());
        let input = style(json!({"rowIds": [id], "color": "#808080"}));
        s.edit(|b| apply_row_style(b, &input)).unwrap();
        s.edit(|b| apply_row_note(b, id, Some("Day 2"))).unwrap();

        assert!(s.undo() && s.undo() && s.undo());
        assert!(edit::details(s.budget()).all(|d| d.id != id));
        assert!(s.add_detail(&account, None, "rule").is_err());
    }
}
