//! The History list (roadmap 2.23): every undo step of a budget, named by the
//! command that made it ("Edit rate on 21-01 line 3", "Paste 12 rows",
//! "Rename fringe FICA to Payroll") with its time; a jump to any step (undo
//! or redo to it); and saving any step as a named version without changing
//! the budget as it is now.
//!
//! The steps themselves live in the session's undo and redo stacks
//! (src/session.rs): each keeps its label and time beside what it changed.
//! This module names steps (the helpers every command uses for its label)
//! and holds the three commands the History tab calls (ui/js/history.js).
//! The tools window may list the steps; jumping and saving a version run in
//! the main window, like Undo (decisions.md, entry 24).

use std::sync::Arc;

use fwb_core::{Budget, RowId, RowKind};
use tauri::{Runtime, State, WebviewWindow};

use crate::app::{finish, AppState, DocId};
use crate::session::History;
use crate::view::View;

// ---------------------------------------------------------------------------
// Step labels

/// `n` things: "1 line", "12 lines".
pub fn count(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

/// A detail field as the grid's column calls it.
pub fn field_name(field: &str) -> &str {
    match field {
        "amount" => "amount",
        "x" => "X",
        "rate" => "rate",
        "x4" => "4X",
        "unit" | "unit2" | "unit3" | "unit4" => "unit",
        "description" => "description",
        "currency" => "currency",
        "fringes" => "fringes",
        "groups" => "groups",
        "set" => "set",
        "location" => "location",
        _ => "a cell",
    }
}

/// Where row `id` is: its account's code and its line number there (as the
/// grid's # column counts, from 1).
pub fn row_place(b: &Budget, id: RowId) -> Option<(String, usize)> {
    b.categories().flat_map(|c| &c.accounts).find_map(|a| {
        a.details
            .iter()
            .position(|d| d.id == id)
            .map(|i| (a.code.clone(), i + 1))
    })
}

/// The one account every row in `ids` is in, if they share one.
pub fn shared_account(b: &Budget, ids: &[RowId]) -> Option<String> {
    let mut code: Option<String> = None;
    for id in ids {
        let (c, _) = row_place(b, *id)?;
        match &code {
            Some(k) if *k != c => return None,
            _ => code = Some(c),
        }
    }
    code
}

/// "Edit rate on 21-01 line 3".
pub fn cell_label(b: &Budget, id: RowId, field: &str) -> String {
    verb_cell("Edit", b, id, field)
}

fn verb_cell(verb: &str, b: &Budget, id: RowId, field: &str) -> String {
    match row_place(b, id) {
        Some((code, line)) => format!("{verb} {} on {code} line {line}", field_name(field)),
        None => format!("{verb} {}", field_name(field)),
    }
}

/// "in 21-01" when every row is in one account, else nothing.
fn place_suffix(b: &Budget, ids: &[RowId], word: &str) -> String {
    shared_account(b, ids)
        .map(|c| format!(" {word} {c}"))
        .unwrap_or_default()
}

/// One value into several rows: "Set rate on 4 lines in 21-01".
pub fn rows_field_label(b: &Budget, ids: &[RowId], field: &str) -> String {
    if let [one] = ids {
        return cell_label(b, *one, field);
    }
    format!(
        "Set {} on {}{}",
        field_name(field),
        count(ids.len(), "line", "lines"),
        place_suffix(b, ids, "in")
    )
}

/// Several cells at once (paste, fill down, clear, cut): `action` is what
/// the page says it was doing, one of a few known words; anything else is
/// an edit. "Paste 12 cells in 21-01", "Clear rate on 21-01 line 3".
pub fn cells_label(b: &Budget, cells: &[(RowId, &str, &str)], action: Option<&str>) -> String {
    let all_blank = !cells.is_empty() && cells.iter().all(|(_, _, v)| v.trim().is_empty());
    let verb = match action {
        Some("paste") => "Paste",
        Some("fill") => "Fill down",
        Some("cut") => "Cut",
        Some("clear") => "Clear",
        _ if all_blank => "Clear",
        _ => "Edit",
    };
    if let [(id, field, _)] = cells {
        return verb_cell(verb, b, *id, field);
    }
    let ids: Vec<RowId> = cells.iter().map(|(id, _, _)| *id).collect();
    format!(
        "{verb} {}{}",
        count(cells.len(), "cell", "cells"),
        place_suffix(b, &ids, "in")
    )
}

/// "Delete line 3 of 21-01", "Delete 4 lines from 21-01".
pub fn delete_rows_label(b: &Budget, ids: &[RowId]) -> String {
    if let [one] = ids {
        if let Some((code, line)) = row_place(b, *one) {
            return format!("Delete line {line} of {code}");
        }
    }
    format!(
        "Delete {}{}",
        count(ids.len(), "line", "lines"),
        place_suffix(b, ids, "from")
    )
}

/// "Insert line in 21-01".
pub fn insert_label(kind: RowKind, account: &str) -> String {
    let what = match kind {
        RowKind::Line => "line",
        RowKind::Subtotal => "subtotal",
        RowKind::Adjustment => "adjustment",
        RowKind::Separator => "separator",
    };
    format!("Insert {what} in {}", account.trim())
}

/// Rows of the selection, for a tool applied to them: "4 lines in 21-01".
pub fn rows_phrase(b: &Budget, ids: &[RowId]) -> String {
    if let [one] = ids {
        if let Some((code, line)) = row_place(b, *one) {
            return format!("{code} line {line}");
        }
    }
    format!(
        "{}{}",
        count(ids.len(), "line", "lines"),
        place_suffix(b, ids, "in")
    )
}

// ---------------------------------------------------------------------------
// Saving a step as a version

/// Saves `past` (the budget as it was at some step) as a new version of `b`,
/// named and noted as the Versions dialog checks names and notes. Returns its
/// id. Only the version list of `b` changes.
pub fn save_step_as_version(
    b: &mut Budget,
    past: &Budget,
    name: &str,
    note: Option<&str>,
    created: &str,
) -> crate::edit::EditResult<fwb_core::VersionId> {
    let id = crate::versions::save(b, name, note, created)?;
    if let Some(v) = b.versions.iter_mut().find(|v| v.id == id) {
        v.budget = Arc::new(past.snapshot());
    }
    Ok(id)
}

// ---------------------------------------------------------------------------
// Commands (registered in app.rs, build.rs and capabilities/main.json; the
// tools window may list the steps only)

/// Every undo and redo step of budget `doc` (`None`: the one in front),
/// oldest first, with how many are applied.
#[tauri::command]
pub fn history_list(state: State<'_, AppState>, doc: Option<DocId>) -> Result<History, String> {
    Ok(state.lock_doc(doc)?.history())
}

/// Undoes or redoes budget `doc`'s steps until `to` of them are applied
/// (0: the start of the list): one change, however many steps it passes.
#[tauri::command]
pub fn history_jump<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    to: usize,
) -> Result<View, String> {
    let mut s = state.lock_doc(doc)?;
    s.jump_to(to)?;
    Ok(finish(&window, &mut s, None))
}

/// Saves the budget as it was with `step` steps applied as a new version
/// named `name` (one undo step, like any version save), leaving its content
/// as it is now. `created` is the page's local time.
#[tauri::command]
pub fn history_save_version<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    step: usize,
    name: String,
    note: Option<String>,
    created: String,
) -> Result<View, String> {
    let mut s = state.lock_doc(doc)?;
    let checked = crate::versions::check_name(&name)?;
    let past = s.budget_at(step)?;
    let label = format!("Save version \u{201c}{checked}\u{201d} from the history");
    s.edit(label, |b| {
        save_step_as_version(b, &past, &checked, note.as_deref(), &created)
    })?;
    Ok(finish(&window, &mut s, None))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Session;

    const AT: &str = "2026-10-09T14:30:00-07:00";

    fn first_line(s: &Session) -> RowId {
        crate::edit::details(s.budget())
            .find(|d| d.kind == RowKind::Line)
            .unwrap()
            .id
    }

    fn labels(s: &Session) -> Vec<String> {
        s.history().steps.into_iter().map(|t| t.label).collect()
    }

    #[test]
    fn steps_are_named_by_the_command_that_made_them() {
        let mut s = Session::new();
        let id = first_line(&s);
        let (code, line) = row_place(s.budget(), id).unwrap();
        s.set_detail_field(id, "rate", "450").unwrap();
        s.set_detail_field(id, "x4", "2").unwrap();
        s.add_detail(&code, Some(id), "subtotal").unwrap();
        s.edit("Rename budget", |b| crate::edit::set_title(b, "Feature"))
            .unwrap();
        assert_eq!(
            labels(&s),
            [
                format!("Edit rate on {code} line {line}"),
                format!("Edit 4X on {code} line {line}"),
                format!("Insert subtotal in {code}"),
                "Rename budget".to_string(),
            ]
        );
        let h = s.history();
        assert_eq!(h.current, 4);
        assert!(h.steps.iter().all(|t| t.done && t.at > 0));
        assert!(!h.trimmed);
        // Undone steps stay listed, after the current position.
        s.undo();
        s.undo();
        let h = s.history();
        assert_eq!(h.current, 2);
        assert_eq!(
            h.steps.iter().map(|t| t.done).collect::<Vec<_>>(),
            [true, true, false, false]
        );
        assert_eq!(h.steps[3].label, "Rename budget");
        // A new edit drops them, as Redo would have lost them.
        s.edit("Rename budget", |b| crate::edit::set_title(b, "Other"))
            .unwrap();
        assert_eq!(s.history().steps.len(), 3);
    }

    #[test]
    fn labels_name_rows_and_counts() {
        let s = Session::new();
        let b = s.budget();
        let ids: Vec<RowId> = crate::edit::details(b).map(|d| d.id).take(3).collect();
        let (code, _) = row_place(b, ids[0]).unwrap();
        let same: Vec<RowId> = b
            .categories()
            .flat_map(|c| &c.accounts)
            .find(|a| a.code == code)
            .unwrap()
            .details
            .iter()
            .map(|d| d.id)
            .collect();
        assert_eq!(
            cells_label(
                b,
                &[(same[0], "rate", "1"), (same[0], "amount", "2")],
                Some("paste")
            ),
            format!("Paste 2 cells in {code}")
        );
        assert_eq!(
            cells_label(b, &[(same[0], "rate", "")], None),
            format!("Clear rate on {code} line 1")
        );
        assert_eq!(
            cells_label(b, &[(same[0], "rate", "3")], Some("<b>x</b>")),
            format!("Edit rate on {code} line 1"),
            "an unknown action is just an edit"
        );
        assert_eq!(
            delete_rows_label(b, &same[..1]),
            format!("Delete line 1 of {code}")
        );
        if same.len() > 1 {
            assert_eq!(
                delete_rows_label(b, &same[..2]),
                format!("Delete 2 lines from {code}")
            );
            assert_eq!(
                rows_field_label(b, &same[..2], "unit2"),
                format!("Set unit on 2 lines in {code}")
            );
        }
        assert_eq!(count(1, "row", "rows"), "1 row");
        assert_eq!(count(12, "row", "rows"), "12 rows");
        assert_eq!(cell_label(b, 999_999, "rate"), "Edit rate");
    }

    #[test]
    fn labels_are_one_tidy_line() {
        let mut s = Session::new();
        let long = format!("Rename\tfringe {}", "x".repeat(400));
        s.edit(long, |b| crate::edit::set_title(b, "A")).unwrap();
        s.edit("   ", |b| crate::edit::set_title(b, "B")).unwrap();
        let l = labels(&s);
        assert!(l[0].starts_with("Rename fringe x"));
        assert_eq!(l[0].chars().count(), crate::session::LABEL_MAX_CHARS);
        assert!(l[0].ends_with('\u{2026}'));
        assert_eq!(l[1], "Change");
    }

    #[test]
    fn jumping_moves_through_the_steps_as_undo_and_redo_do() {
        let mut s = Session::new();
        let id = first_line(&s);
        let mut states = vec![s.budget().clone()];
        for (i, rate) in ["10", "20", "30", "40"].iter().enumerate() {
            s.set_detail_field(id, "rate", rate).unwrap();
            states.push(s.budget().clone());
            assert_eq!(s.history().current, i + 1);
        }
        let rev = s.view_rev();
        s.jump_to(1).unwrap();
        assert_eq!(s.budget(), &states[1]);
        assert_eq!(s.history().current, 1);
        assert_ne!(s.view_rev(), rev, "a jump is a change");
        assert!(s.can_undo() && s.can_redo());
        s.jump_to(4).unwrap();
        assert_eq!(s.budget(), &states[4]);
        s.jump_to(0).unwrap();
        assert_eq!(s.budget(), &states[0]);
        assert!(!s.is_dirty(), "back at the start: nothing unsaved");
        s.jump_to(3).unwrap();
        assert_eq!(s.budget(), &states[3]);
        // Where we are already: nothing happens.
        let rev = s.view_rev();
        s.jump_to(3).unwrap();
        assert_eq!(s.view_rev(), rev);
        assert!(s.jump_to(5).is_err(), "past the last step");
        assert_eq!(s.budget(), &states[3]);
        // Undo and redo still work from wherever a jump left off.
        s.undo();
        assert_eq!(s.budget(), &states[2]);
        s.redo();
        s.redo();
        assert_eq!(s.budget(), &states[4]);
    }

    #[test]
    fn a_step_reads_back_without_touching_the_budget() {
        let mut s = Session::new();
        let id = first_line(&s);
        let mut states = vec![s.budget().clone()];
        for rate in ["11", "22", "33"] {
            s.set_detail_field(id, "rate", rate).unwrap();
            states.push(s.budget().clone());
        }
        // Add a category: a whole-budget step, then undo one so a redo step
        // is in the list too.
        s.edit("Add category 90", |b| {
            crate::edit::add_category(b, "90", "Other", None)
        })
        .unwrap();
        states.push(s.budget().clone());
        s.undo();
        let before = s.history();
        let rev = s.view_rev();
        let dirty = s.is_dirty();
        for (pos, want) in states.iter().enumerate() {
            assert_eq!(&s.budget_at(pos).unwrap(), want, "step {pos}");
            assert_eq!(s.budget(), &states[3], "the budget is as it was");
            assert_eq!(s.history(), before, "and so is its history");
        }
        assert_eq!(s.view_rev(), rev);
        assert_eq!(s.is_dirty(), dirty);
        assert!(s.budget_at(states.len()).is_err());
        // The history still undoes and redoes to the same states.
        s.redo();
        assert_eq!(s.budget(), &states[4]);
        for want in states.iter().rev().skip(1) {
            s.undo();
            assert_eq!(s.budget(), want);
        }
    }

    #[test]
    fn a_step_saved_as_a_version_keeps_the_budget_as_it_is() {
        let mut s = Session::new();
        let id = first_line(&s);
        s.set_detail_field(id, "rate", "100").unwrap();
        let at_one = s.budget().clone();
        s.set_detail_field(id, "rate", "200").unwrap();
        let now = s.budget().clone();
        let past = s.budget_at(1).unwrap();
        s.edit("Save version", |b| {
            save_step_as_version(b, &past, " Rate at 100 ", Some("first try"), AT)
        })
        .unwrap();
        // The content is unchanged; the new version holds the step's content.
        let b = s.budget();
        assert_eq!(b.snapshot(), now.snapshot());
        let v = b.versions.last().unwrap();
        assert_eq!(v.name, "Rate at 100");
        assert_eq!(v.note.as_deref(), Some("first try"));
        assert_eq!(*v.budget, at_one.snapshot());
        // One undo step, which takes the version back off.
        assert_eq!(s.history().steps.len(), 3);
        s.undo();
        assert!(s.budget().versions.is_empty());
        // Names and times are checked as the Versions dialog checks them.
        let past = s.budget_at(0).unwrap();
        assert!(s
            .edit("x", |b| save_step_as_version(b, &past, "  ", None, AT))
            .is_err());
        assert!(s
            .edit("x", |b| save_step_as_version(b, &past, "Ok", None, "soon"))
            .is_err());
    }

    #[test]
    fn the_oldest_steps_go_past_the_limit() {
        let mut s = Session::new();
        let id = first_line(&s);
        for i in 0..crate::session::UNDO_LIMIT + 3 {
            s.set_detail_field(id, "rate", &format!("{}", i + 1))
                .unwrap();
        }
        let h = s.history();
        assert_eq!(h.steps.len(), crate::session::UNDO_LIMIT);
        assert!(h.trimmed);
        assert_eq!(h.current, crate::session::UNDO_LIMIT);
    }

    #[test]
    fn history_commands_are_wired() {
        const APP_RS: &str = include_str!("app.rs");
        const BUILD_RS: &str = include_str!("../build.rs");
        const MAIN_CAPS: &str = include_str!("../capabilities/main.json");
        const TOOLS_CAPS: &str = include_str!("../capabilities/tools.json");
        for c in ["history_list", "history_jump", "history_save_version"] {
            assert!(APP_RS.contains(&format!("crate::history::{c},")), "{c}");
            assert!(BUILD_RS.contains(&format!("\"{c}\",")), "{c}");
            let allow = format!("\"allow-{}\"", c.replace('_', "-"));
            assert!(MAIN_CAPS.contains(&allow), "{c} for the main window");
            // Jumping and saving run in the main window, like Undo.
            assert_eq!(TOOLS_CAPS.contains(&allow), c == "history_list", "{c}");
        }
    }
}
