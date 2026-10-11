//! Dated notes on lines, accounts and categories (roadmap 2.8), and the
//! budget-wide list the Notes tab shows as a review checklist.
//!
//! A note belongs to one owner: a detail row (by id), an account (by
//! number) or a category (by code); account numbers and category codes are
//! unique in a budget. Notes are addressed by owner and id (ids are unique
//! within their owner's list). Every command is one undo step. Times come
//! from the page (ISO 8601 text, like versions'); Rust never reads the
//! clock for them. The author is free text the page keeps in its own
//! preferences: there are no accounts.

use method_core::{next_note_id, Budget, Note, NoteId, RowId, TopsheetRow, MAX_NOTE_CHARS};
use serde::{Deserialize, Serialize};
use tauri::{Runtime, State, WebviewWindow};

use crate::app::{edit_budget, AppState, DocId};
use crate::edit::EditResult;
use crate::view::View;

/// Longest author name, in characters.
pub const MAX_AUTHOR_CHARS: usize = 100;
/// Longest time stamp accepted, in characters.
pub const MAX_STAMP_CHARS: usize = 64;
/// How much of each note the list carries, in characters.
pub const NOTE_PREVIEW_CHARS: usize = 280;

/// Who a note is on: `{"row": 12}`, `{"account": "11-01"}` or
/// `{"category": "11"}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteOwner {
    Row(RowId),
    Account(String),
    Category(String),
}

/// The owner's notes, to change.
pub fn notes_mut<'a>(b: &'a mut Budget, owner: &NoteOwner) -> EditResult<&'a mut Vec<Note>> {
    for c in b.categories_mut() {
        if matches!(owner, NoteOwner::Category(code) if *code == c.code) {
            return Ok(&mut c.notes);
        }
        for a in &mut c.accounts {
            if matches!(owner, NoteOwner::Account(code) if *code == a.code) {
                return Ok(&mut a.notes);
            }
            if let NoteOwner::Row(id) = owner {
                if let Some(d) = a.details.iter_mut().find(|d| d.id == *id) {
                    return Ok(&mut d.notes);
                }
            }
        }
    }
    Err(match owner {
        NoteOwner::Row(id) => crate::edit::row_gone(*id),
        NoteOwner::Account(code) => format!("account {code} no longer exists"),
        NoteOwner::Category(code) => format!("category {code} no longer exists"),
    })
}

fn check_text(text: &str) -> EditResult<String> {
    if text.trim().is_empty() {
        return Err("a note needs some text".into());
    }
    let len = text.chars().count();
    if len > MAX_NOTE_CHARS {
        return Err(format!(
            "that note is {len} characters; notes can be at most {MAX_NOTE_CHARS}"
        ));
    }
    Ok(text.to_string())
}

fn check_stamp(s: &str) -> EditResult<String> {
    let s = s.trim();
    if s.chars().count() > MAX_STAMP_CHARS {
        return Err("that time is too long".into());
    }
    Ok(s.to_string())
}

fn check_author(a: Option<&str>) -> EditResult<Option<String>> {
    let Some(a) = a.map(str::trim).filter(|a| !a.is_empty()) else {
        return Ok(None);
    };
    if a.chars().count() > MAX_AUTHOR_CHARS {
        return Err(format!(
            "author names can be at most {MAX_AUTHOR_CHARS} characters"
        ));
    }
    Ok(Some(a.to_string()))
}

/// Adds a note at the end of the owner's list; returns its id.
pub fn add(
    b: &mut Budget,
    owner: &NoteOwner,
    text: &str,
    created: &str,
    author: Option<&str>,
) -> EditResult<NoteId> {
    let text = check_text(text)?;
    let created = check_stamp(created)?;
    let author = check_author(author)?;
    let notes = notes_mut(b, owner)?;
    let id = next_note_id(notes);
    notes.push(Note {
        id,
        text,
        created,
        edited: None,
        author,
        done: false,
    });
    Ok(id)
}

fn note_mut<'a>(b: &'a mut Budget, owner: &NoteOwner, id: NoteId) -> EditResult<&'a mut Note> {
    notes_mut(b, owner)?
        .iter_mut()
        .find(|n| n.id == id)
        .ok_or_else(|| "that note no longer exists".to_string())
}

/// Changes a note's text, stamping `edited`. Blank text removes the note.
/// The same text changes nothing.
pub fn update(
    b: &mut Budget,
    owner: &NoteOwner,
    id: NoteId,
    text: &str,
    edited: &str,
) -> EditResult {
    if text.trim().is_empty() {
        return remove(b, owner, id);
    }
    let text = check_text(text)?;
    let edited = check_stamp(edited)?;
    let n = note_mut(b, owner, id)?;
    if n.text != text {
        n.text = text;
        n.edited = (!edited.is_empty()).then_some(edited);
    }
    Ok(())
}

pub fn remove(b: &mut Budget, owner: &NoteOwner, id: NoteId) -> EditResult {
    let notes = notes_mut(b, owner)?;
    let before = notes.len();
    notes.retain(|n| n.id != id);
    if notes.len() == before {
        return Err("that note no longer exists".into());
    }
    Ok(())
}

/// Ticks a note off in the review list, or back on.
pub fn mark_done(b: &mut Budget, owner: &NoteOwner, id: NoteId, done: bool) -> EditResult {
    note_mut(b, owner, id)?.done = done;
    Ok(())
}

// ---------------------------------------------------------------------------
// The budget-wide list

/// One note in the Notes tab's list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct NoteEntry {
    pub owner: NoteOwner,
    pub id: NoteId,
    /// `row`, `account` or `category`.
    pub kind: &'static str,
    /// Where it is, for going there: the topsheet index; the account's index
    /// in its category (not for a category's notes); the row id (a line's).
    pub ti: usize,
    pub ai: Option<usize>,
    pub row_id: Option<RowId>,
    pub category: String,
    pub category_name: String,
    /// The account's number and name ("" for a category's notes).
    pub account: String,
    pub account_name: String,
    /// The row's number in its account, as the grid's # column shows it.
    pub line: Option<usize>,
    /// The row's description (a line's notes).
    pub description: String,
    /// The note, cut to [`NOTE_PREVIEW_CHARS`] characters.
    pub note: String,
    /// True when `note` was cut.
    pub more: bool,
    pub created: String,
    pub edited: Option<String>,
    pub author: Option<String>,
    pub done: bool,
}

fn entry_text(n: &Note) -> (String, bool) {
    let more = n.text.chars().count() > NOTE_PREVIEW_CHARS;
    (n.text.chars().take(NOTE_PREVIEW_CHARS).collect(), more)
}

/// Every note in the budget, in budget order: a category's own notes, then
/// for each account its own notes and then its lines'. Blank notes (from a
/// file) are left out.
pub fn budget_notes(b: &Budget) -> Vec<NoteEntry> {
    let mut out = Vec::new();
    let push = |out: &mut Vec<NoteEntry>, n: &Note, base: &NoteEntry| {
        if n.text.trim().is_empty() {
            return;
        }
        let (note, more) = entry_text(n);
        out.push(NoteEntry {
            id: n.id,
            note,
            more,
            created: n.created.clone(),
            edited: n.edited.clone(),
            author: n.author.clone(),
            done: n.done,
            ..base.clone()
        });
    };
    for (ti, row) in b.topsheet.iter().enumerate() {
        let TopsheetRow::Category(c) = row else {
            continue;
        };
        let cat = NoteEntry {
            owner: NoteOwner::Category(c.code.clone()),
            id: 0,
            kind: "category",
            ti,
            ai: None,
            row_id: None,
            category: c.code.clone(),
            category_name: c.name.clone(),
            account: String::new(),
            account_name: String::new(),
            line: None,
            description: String::new(),
            note: String::new(),
            more: false,
            created: String::new(),
            edited: None,
            author: None,
            done: false,
        };
        for n in &c.notes {
            push(&mut out, n, &cat);
        }
        for (ai, a) in c.accounts.iter().enumerate() {
            let acct = NoteEntry {
                owner: NoteOwner::Account(a.code.clone()),
                kind: "account",
                ai: Some(ai),
                account: a.code.clone(),
                account_name: a.name.clone(),
                ..cat.clone()
            };
            for n in &a.notes {
                push(&mut out, n, &acct);
            }
            for (i, d) in a.details.iter().enumerate() {
                let line = NoteEntry {
                    owner: NoteOwner::Row(d.id),
                    kind: "row",
                    row_id: Some(d.id),
                    line: Some(i + 1),
                    description: d.description.clone(),
                    ..acct.clone()
                };
                for n in &d.notes {
                    push(&mut out, n, &line);
                }
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Commands

/// `{owner, text, created, author?}`: adds a note to the owner.
#[tauri::command]
pub fn add_note<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    owner: NoteOwner,
    text: String,
    created: String,
    author: Option<String>,
) -> Result<View, String> {
    edit_budget(&window, &state, doc, "Add a note", |b| {
        add(b, &owner, &text, &created, author.as_deref()).map(|_| ())
    })
}

/// `{owner, id, text, edited}`: changes a note's text (blank removes it).
#[tauri::command]
pub fn update_note<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    owner: NoteOwner,
    id: NoteId,
    text: String,
    edited: String,
) -> Result<View, String> {
    edit_budget(&window, &state, doc, "Edit a note", |b| {
        update(b, &owner, id, &text, &edited)
    })
}

/// `{owner, id}`.
#[tauri::command]
pub fn delete_note<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    owner: NoteOwner,
    id: NoteId,
) -> Result<View, String> {
    edit_budget(&window, &state, doc, "Delete a note", |b| {
        remove(b, &owner, id)
    })
}

/// `{owner, id, done}`.
#[tauri::command]
pub fn set_note_done<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    owner: NoteOwner,
    id: NoteId,
    done: bool,
) -> Result<View, String> {
    edit_budget(
        &window,
        &state,
        doc,
        if done {
            "Check off a note"
        } else {
            "Uncheck a note"
        },
        |b| mark_done(b, &owner, id, done),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Session;

    fn first_row(s: &Session) -> (String, String, RowId) {
        let c = s.budget().categories().next().unwrap();
        let a = c.accounts.iter().find(|a| !a.details.is_empty()).unwrap();
        (c.code.clone(), a.code.clone(), a.details[0].id)
    }

    #[test]
    fn notes_on_rows_accounts_and_categories() {
        let mut s = Session::new();
        let (cat, acct, row) = first_row(&s);
        let on_row = NoteOwner::Row(row);
        let on_acct = NoteOwner::Account(acct.clone());
        let on_cat = NoteOwner::Category(cat.clone());
        s.edit("Test edit", |b| {
            add(
                b,
                &on_row,
                "Check the rate",
                "2026-10-09T10:00:00Z",
                Some(" Alex "),
            )
        })
        .unwrap();
        s.edit("Test edit", |b| {
            add(b, &on_row, "Union minimum", "2026-10-09T11:00:00Z", None)
        })
        .unwrap();
        s.edit("Test edit", |b| {
            add(b, &on_acct, "Whole account to review", "t", None)
        })
        .unwrap();
        s.edit("Test edit", |b| {
            add(b, &on_cat, "Category note", "t", Some(""))
        })
        .unwrap();
        let mut b = s.budget().clone();
        let notes = notes_mut(&mut b, &on_row).unwrap().clone();
        assert_eq!(notes.len(), 2);
        assert_eq!((notes[0].id, notes[1].id), (1, 2));
        assert_eq!(notes[0].author.as_deref(), Some("Alex"));
        assert_eq!(notes[1].author, None);
        assert_eq!(notes[0].created, "2026-10-09T10:00:00Z");

        // Edit stamps; the same text changes nothing; blank removes.
        s.edit("Test edit", |b| {
            update(
                b,
                &on_row,
                1,
                "Check the rate again",
                "2026-10-10T09:00:00Z",
            )
        })
        .unwrap();
        let rev = s.view_rev();
        s.edit("Test edit", |b| {
            update(b, &on_row, 1, "Check the rate again", "later")
        })
        .unwrap();
        assert_eq!(s.view_rev(), rev, "nothing changed, no undo step");
        let mut b = s.budget().clone();
        let n = &notes_mut(&mut b, &on_row).unwrap()[0];
        assert_eq!(n.edited.as_deref(), Some("2026-10-10T09:00:00Z"));
        s.edit("Test edit", |b| mark_done(b, &on_row, 2, true))
            .unwrap();
        s.edit("Test edit", |b| update(b, &on_row, 1, "  ", "x"))
            .unwrap();
        let mut b = s.budget().clone();
        let left = notes_mut(&mut b, &on_row).unwrap();
        assert_eq!(left.len(), 1);
        assert!(left[0].done);
        // A new note takes the next id, never a deleted one's place... the
        // highest left is 2, so 3.
        assert_eq!(
            s.edit("Test edit", |b| add(b, &on_row, "x", "", None))
                .unwrap(),
            3
        );

        // The list: category, account, then the line's, in budget order.
        let list = budget_notes(s.budget());
        let kinds: Vec<&str> = list.iter().map(|e| e.kind).collect();
        assert_eq!(kinds, ["category", "account", "row", "row"]);
        assert_eq!(list[0].owner, on_cat);
        assert_eq!(list[1].account, acct);
        assert_eq!(list[2].row_id, Some(row));
        assert_eq!(list[2].line, Some(1));
        assert!(list[2].done);
        assert_eq!(list[1].category, cat);

        // Undo walks it back step by step.
        assert!(s.undo());
        assert_eq!(budget_notes(s.budget()).len(), 3);
    }

    #[test]
    fn bad_notes_are_refused_and_change_nothing() {
        let mut s = Session::new();
        let (_, _, row) = first_row(&s);
        let on_row = NoteOwner::Row(row);
        let before = s.budget().clone();
        assert!(s
            .edit("Test edit", |b| add(b, &on_row, "   ", "", None))
            .is_err());
        let long = "x".repeat(MAX_NOTE_CHARS + 1);
        assert!(s
            .edit("Test edit", |b| add(b, &on_row, &long, "", None))
            .is_err());
        let author = "a".repeat(MAX_AUTHOR_CHARS + 1);
        assert!(s
            .edit("Test edit", |b| add(b, &on_row, "x", "", Some(&author)))
            .is_err());
        assert!(s
            .edit("Test edit", |b| add(
                b,
                &NoteOwner::Row(999_999),
                "x",
                "",
                None
            ))
            .is_err());
        assert!(s
            .edit("Test edit", |b| add(
                b,
                &NoteOwner::Account("nope".into()),
                "x",
                "",
                None
            ))
            .is_err());
        assert!(s.edit("Test edit", |b| remove(b, &on_row, 1)).is_err());
        assert!(s
            .edit("Test edit", |b| mark_done(b, &on_row, 1, true))
            .is_err());
        assert_eq!(s.budget(), &before);
        let longest = "é".repeat(MAX_NOTE_CHARS);
        s.edit("Test edit", |b| add(b, &on_row, &longest, "", None))
            .unwrap();
        let list = budget_notes(s.budget());
        assert_eq!(list[0].note.chars().count(), NOTE_PREVIEW_CHARS);
        assert!(list[0].more);
    }

    #[test]
    fn owners_read_from_the_page() {
        let o: NoteOwner = serde_json::from_str(r#"{"row": 12}"#).unwrap();
        assert_eq!(o, NoteOwner::Row(12));
        let o: NoteOwner = serde_json::from_str(r#"{"account": "11-01"}"#).unwrap();
        assert_eq!(o, NoteOwner::Account("11-01".into()));
        let o: NoteOwner = serde_json::from_str(r#"{"category": "11"}"#).unwrap();
        assert_eq!(
            serde_json::to_value(&o).unwrap(),
            serde_json::json!({"category": "11"})
        );
    }
}
