//! Naming undo steps for the History list (roadmap 2.23): every command
//! labels its step with what it did ("Edit rate on 21-01 line 3", "Paste 12
//! rows", "Rename fringe FICA to Payroll"). The steps themselves live in the
//! session's undo and redo stacks (src/session.rs), each with its label and
//! time beside what it changed. The History tab's list and jump are in
//! src/commands.rs; saving a step as a version is here.

use std::sync::Arc;

use method_core::{Budget, RowId, RowKind};

use crate::messages::{msg, word};
use crate::session::Session;

// ---------------------------------------------------------------------------
// Step labels
//
// Each is made from a code in the engine's catalog (`engine.undo.*`,
// src/messages.rs), so the desktop shows it in the person's language; the
// label itself, kept in the history and in a shared budget's change log, is
// always the English.

/// `n` things: "1 line", "12 lines".
pub fn count(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

/// A detail field as the grid's column calls it.
pub fn field_name(field: &str) -> String {
    word(match field {
        "amount" => "engine.what.amount",
        "x" => "engine.what.x",
        "rate" => "engine.what.rate",
        "x4" => "engine.what.x4",
        "unit" | "unit2" | "unit3" | "unit4" => "engine.what.unit",
        "description" => "engine.what.description",
        "currency" => "engine.what.currency",
        "fringes" => "engine.what.fringes",
        "groups" => "engine.what.groups",
        "set" => "engine.what.set",
        "location" => "engine.what.location",
        _ => "engine.what.a_cell",
    })
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
    verb_cell(Verb::Edit, b, id, field)
}

/// What was done to cells, for a step's label.
#[derive(Clone, Copy)]
enum Verb {
    Edit,
    Paste,
    Fill,
    Cut,
    Clear,
}

impl Verb {
    /// The codes for one cell (somewhere, and on a line), several cells, and
    /// several cells in one account.
    fn codes(self) -> [&'static str; 4] {
        match self {
            Verb::Edit => [
                "engine.undo.edit_cell",
                "engine.undo.edit_cell_at",
                "engine.undo.edit_cells",
                "engine.undo.edit_cells_in",
            ],
            Verb::Paste => [
                "engine.undo.paste_cell",
                "engine.undo.paste_cell_at",
                "engine.undo.paste_cells",
                "engine.undo.paste_cells_in",
            ],
            Verb::Fill => [
                "engine.undo.fill_cell",
                "engine.undo.fill_cell_at",
                "engine.undo.fill_cells",
                "engine.undo.fill_cells_in",
            ],
            Verb::Cut => [
                "engine.undo.cut_cell",
                "engine.undo.cut_cell_at",
                "engine.undo.cut_cells",
                "engine.undo.cut_cells_in",
            ],
            Verb::Clear => [
                "engine.undo.clear_cell",
                "engine.undo.clear_cell_at",
                "engine.undo.clear_cells",
                "engine.undo.clear_cells_in",
            ],
        }
    }
}

fn verb_cell(verb: Verb, b: &Budget, id: RowId, field: &str) -> String {
    let [somewhere, at, _, _] = verb.codes();
    let what = field_name(field);
    match row_place(b, id) {
        Some((code, line)) => msg(at, &[("what", &what), ("code", &code), ("line", &line)]),
        None => msg(somewhere, &[("what", &what)]),
    }
}

/// `n` things, in account `code` when there is one: the code `in_code`'s
/// words, or `anywhere`'s.
fn counted(n: usize, code: Option<String>, anywhere: &str, in_code: &str) -> String {
    match code {
        Some(c) => msg(in_code, &[("count", &n), ("code", &c)]),
        None => msg(anywhere, &[("count", &n)]),
    }
}

/// "4 lines in 21-01" when every row is in one account, else "4 lines".
fn lines_phrase(b: &Budget, ids: &[RowId]) -> String {
    counted(
        ids.len(),
        shared_account(b, ids),
        "engine.phrase.lines",
        "engine.phrase.lines_in",
    )
}

/// One value into several rows: "Set rate on 4 lines in 21-01".
pub fn rows_field_label(b: &Budget, ids: &[RowId], field: &str) -> String {
    if let [one] = ids {
        return cell_label(b, *one, field);
    }
    msg(
        "engine.undo.set_field_rows",
        &[
            ("what", &field_name(field)),
            ("rows", &lines_phrase(b, ids)),
        ],
    )
}

/// Several cells at once (paste, fill down, clear, cut): `action` is what
/// the page says it was doing, one of a few known words; anything else is
/// an edit. "Paste 12 cells in 21-01", "Clear rate on 21-01 line 3".
pub fn cells_label(b: &Budget, cells: &[(RowId, &str, &str)], action: Option<&str>) -> String {
    let all_blank = !cells.is_empty() && cells.iter().all(|(_, _, v)| v.trim().is_empty());
    let verb = match action {
        Some("paste") => Verb::Paste,
        Some("fill") => Verb::Fill,
        Some("cut") => Verb::Cut,
        Some("clear") => Verb::Clear,
        _ if all_blank => Verb::Clear,
        _ => Verb::Edit,
    };
    if let [(id, field, _)] = cells {
        return verb_cell(verb, b, *id, field);
    }
    let ids: Vec<RowId> = cells.iter().map(|(id, _, _)| *id).collect();
    let [_, _, anywhere, in_code] = verb.codes();
    counted(cells.len(), shared_account(b, &ids), anywhere, in_code)
}

/// "Delete line 3 of 21-01", "Delete 4 lines from 21-01".
pub fn delete_rows_label(b: &Budget, ids: &[RowId]) -> String {
    if let [one] = ids {
        if let Some((code, line)) = row_place(b, *one) {
            return msg(
                "engine.undo.delete_line_at",
                &[("line", &line), ("code", &code)],
            );
        }
    }
    counted(
        ids.len(),
        shared_account(b, ids),
        "engine.undo.delete_lines",
        "engine.undo.delete_lines_from",
    )
}

/// "Insert line in 21-01".
pub fn insert_label(kind: RowKind, account: &str) -> String {
    let code = match kind {
        RowKind::Line => "engine.undo.insert_line",
        RowKind::Subtotal => "engine.undo.insert_subtotal",
        RowKind::Adjustment => "engine.undo.insert_adjustment",
        RowKind::Separator => "engine.undo.insert_separator",
    };
    msg(code, &[("code", &account.trim())])
}

/// Rows of the selection, for a tool applied to them: "4 lines in 21-01",
/// or "21-01 line 3" for one line.
pub fn rows_phrase(b: &Budget, ids: &[RowId]) -> String {
    if let [one] = ids {
        if let Some((code, line)) = row_place(b, *one) {
            return msg("engine.phrase.line_at", &[("code", &code), ("line", &line)]);
        }
    }
    lines_phrase(b, ids)
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
) -> crate::edit::EditResult<method_core::VersionId> {
    let id = crate::versions::save(b, name, note, created)?;
    if let Some(v) = b.versions.iter_mut().find(|v| v.id == id) {
        v.budget = Arc::new(past.snapshot());
    }
    Ok(id)
}

/// The History tab's Save as Version: the budget as it was with `step`
/// steps applied, saved as a new version named `name` (one undo step, like
/// any version save), leaving its content as it is now. `created` is the
/// page's local time.
pub fn history_save_version(
    s: &mut Session,
    step: usize,
    name: &str,
    note: Option<&str>,
    created: &str,
) -> crate::edit::EditResult<method_core::VersionId> {
    let checked = crate::versions::check_name(name)?;
    let past = s.budget_at(step)?;
    let label = msg(
        "engine.undo.save_version_from_history",
        &[("name", &checked)],
    );
    s.edit(label, |b| {
        save_step_as_version(b, &past, &checked, note, created)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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

    const AT: &str = "2026-10-09T14:30:00-07:00";

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
}
