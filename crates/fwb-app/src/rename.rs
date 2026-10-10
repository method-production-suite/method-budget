//! Renaming a setup item (a global, fringe, group, unit, set, location or
//! currency) with every reference to it rewritten, as one undo step (roadmap
//! 1.18, decision 7). The engine does the work (`fwb_core::rename`); this is
//! the command the tools panel calls from an item's editor, its right-click
//! menu, and the Find tab.

use fwb_core::rename::{self as core_rename, SetupKind};
use fwb_core::Budget;
use serde::Serialize;
use tauri::{Runtime, State, WebviewWindow};

use crate::app::{edit_budget, AppState, DocId};
use crate::edit::EditResult;
use crate::view::View;

/// Renames item `from` of kind `kind` (`"global"`, `"fringe"`, `"group"`,
/// `"unit"`, `"set"`, `"location"` or `"currency"`) to `to`, trimmed.
/// Returns the new name and how many places now use it. Nothing changes on
/// an error.
pub fn rename_item(
    b: &mut Budget,
    kind: &str,
    from: &str,
    to: &str,
) -> EditResult<(String, usize)> {
    let kind =
        SetupKind::from_name(kind).ok_or_else(|| format!("unknown kind of item `{kind}`"))?;
    let places = core_rename::rename(b, kind, from, to).map_err(|e| e.to_string())?;
    Ok((to.trim().to_string(), places))
}

/// A kind of setup item as a step label names it ("fringe", "global"); an
/// unknown kind is just "item" (the rename refuses it anyway).
fn kind_noun(kind: &str) -> &'static str {
    match kind {
        "global" => "global",
        "fringe" => "fringe",
        "group" => "group",
        "unit" => "unit",
        "set" => "set",
        "location" => "location",
        "currency" => "currency",
        _ => "item",
    }
}

/// The view after a rename, the new name, and how many places were rewritten.
#[derive(Clone, Debug, Serialize)]
pub struct Renamed {
    #[serde(flatten)]
    pub view: View,
    pub name: String,
    pub places: usize,
}

/// Renames a setup item everywhere, as one undo step. See [`rename_item`].
#[tauri::command]
pub fn rename_setup_item<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    kind: String,
    from: String,
    to: String,
) -> Result<Renamed, String> {
    let mut out = (String::new(), 0);
    let label = format!(
        "Rename {} {} to {}",
        kind_noun(&kind),
        from.trim(),
        to.trim()
    );
    let view = edit_budget(&window, &state, doc, label, |b| {
        out = rename_item(b, &kind, &from, &to)?;
        Ok(())
    })?;
    Ok(Renamed {
        view,
        name: out.0,
        places: out.1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Session;
    use fwb_core::{Detail, RowId};

    fn first_line(b: &Budget) -> RowId {
        b.categories()
            .flat_map(|c| &c.accounts)
            .flat_map(|a| &a.details)
            .find(|d| d.kind == fwb_core::RowKind::Line)
            .unwrap()
            .id
    }

    fn row(b: &Budget, id: RowId) -> &Detail {
        b.categories()
            .flat_map(|c| &c.accounts)
            .flat_map(|a| &a.details)
            .find(|d| d.id == id)
            .unwrap()
    }

    #[test]
    fn a_rename_is_one_undo_step_and_undoes_cleanly() {
        let mut s = Session::new();
        let id = first_line(s.budget());
        s.edit("Test edit", |b| {
            crate::edit::set_global(b, "DAYS", "5")?;
            crate::edit::set_detail_field(b, id, crate::edit::Field::Amount, "DAYS*2")?;
            crate::edit::set_group(
                b,
                "Crew",
                &serde_json::from_value(serde_json::json!({"in_budget_total": true})).unwrap(),
            )?;
            crate::apply::apply_tag(b, &[id], "groups", "Crew", true)
        })
        .unwrap();
        let before = s.budget().clone();
        let total = s.totals().1.report.as_ref().map(|r| r.net_total);
        let (name, n) = s
            .edit("Test edit", |b| {
                rename_item(b, "global", "DAYS", " BUILD_DAYS ")
            })
            .unwrap();
        assert_eq!((name.as_str(), n), ("BUILD_DAYS", 1));
        assert_eq!(row(s.budget(), id).amount.0, "BUILD_DAYS*2");
        assert_eq!(s.totals().1.report.as_ref().map(|r| r.net_total), total);
        s.edit("Test edit", |b| {
            rename_item(b, "group", "Crew", "Shoot crew")
        })
        .unwrap();
        assert!(row(s.budget(), id).groups.contains("Shoot crew"));
        assert!(s.undo() && s.undo());
        assert_eq!(*s.budget(), before);
    }

    #[test]
    fn refusals_say_why_and_change_nothing() {
        let mut s = Session::new();
        s.edit("Test edit", |b| crate::edit::set_global(b, "DAYS", "5"))
            .unwrap();
        let before = s.budget().clone();
        let e = s
            .edit("Test edit", |b| rename_item(b, "global", "DAYS", "2x"))
            .unwrap_err();
        assert!(e.contains("can't be a global name"), "{e}");
        let e = s
            .edit("Test edit", |b| rename_item(b, "unit", "Day", "Week"))
            .unwrap_err();
        assert!(e.contains("already a unit named Week"), "{e}");
        let e = s
            .edit("Test edit", |b| rename_item(b, "thing", "a", "b"))
            .unwrap_err();
        assert!(e.contains("unknown kind"), "{e}");
        let e = s
            .edit("Test edit", |b| rename_item(b, "fringe", "NOPE", "b"))
            .unwrap_err();
        assert!(e.contains("no longer exists"), "{e}");
        assert_eq!(*s.budget(), before);
        assert!(!s.can_undo() || s.undo());
    }

    const APP_RS: &str = include_str!("app.rs");
    const BUILD_RS: &str = include_str!("../build.rs");
    const MAIN_CAPS: &str = include_str!("../capabilities/main.json");
    const TOOLS_CAPS: &str = include_str!("../capabilities/tools.json");

    #[test]
    fn the_rename_command_is_wired_for_both_windows() {
        assert!(APP_RS.contains("crate::rename::rename_setup_item,"));
        assert!(BUILD_RS.contains("\"rename_setup_item\","));
        assert!(MAIN_CAPS.contains("\"allow-rename-setup-item\""));
        assert!(TOOLS_CAPS.contains("\"allow-rename-setup-item\""));
    }
}
