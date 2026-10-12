//! The notes' commands (src/notes.rs), and the Notes tab's list (the
//! desktop's `notes_list` is in its src/tools.rs).

use method_core::NoteId;
use serde::Deserialize;
use serde_json::Value;

use super::{json, Call, Error};
use crate::notes::{self as n, NoteOwner};

pub(super) fn dispatch(d: &Call<'_>) -> Option<Result<Value, Error>> {
    Some(match d.command() {
        "add_note" => d.view_with(|s, a: Add| {
            n::add_note(s, &a.owner, &a.text, &a.created, a.author.as_deref())
        }),
        "update_note" => {
            d.view_with(|s, a: Update| n::update_note(s, &a.owner, a.id, &a.text, &a.edited))
        }
        "delete_note" => d.view_with(|s, a: Id| n::delete_note(s, &a.owner, a.id)),
        "set_note_done" => d.view_with(|s, a: Done| n::set_note_done(s, &a.owner, a.id, a.done)),
        "notes_list" => d.lock().and_then(|s| json(n::budget_notes(s.budget()))),
        _ => return None,
    })
}

#[derive(Deserialize)]
struct Add {
    owner: NoteOwner,
    text: String,
    created: String,
    author: Option<String>,
}

#[derive(Deserialize)]
struct Update {
    owner: NoteOwner,
    id: NoteId,
    text: String,
    edited: String,
}

#[derive(Deserialize)]
struct Id {
    owner: NoteOwner,
    id: NoteId,
}

#[derive(Deserialize)]
struct Done {
    owner: NoteOwner,
    id: NoteId,
    done: bool,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::host::Host;

    #[test]
    fn notes_go_through_json_as_the_page_sends_them() {
        let h = Host::new();
        let v = h.dispatch("get_view", json!({})).unwrap();
        let code = v["budget"]["topsheet"][0]["accounts"][0]["code"].clone();
        let owner = json!({ "account": code });
        h.dispatch(
            "add_note",
            json!({"owner": owner, "text": "Check it", "created": "2026-10-09T10:00:00Z"}),
        )
        .unwrap();
        let list = h.dispatch("notes_list", json!({})).unwrap();
        assert_eq!(list[0]["note"], "Check it");
        assert_eq!(list[0]["kind"], "account");
        let id = list[0]["id"].clone();
        h.dispatch(
            "set_note_done",
            json!({"owner": owner, "id": id, "done": true}),
        )
        .unwrap();
        let steps = h.dispatch("history_list", json!({})).unwrap();
        assert_eq!(steps["steps"][1]["label"], "Check off a note");
        h.dispatch("delete_note", json!({"owner": owner, "id": id}))
            .unwrap();
        assert_eq!(h.dispatch("notes_list", json!({})).unwrap(), json!([]));
        assert!(h
            .dispatch("delete_note", json!({"owner": owner, "id": id}))
            .is_err());
    }
}
