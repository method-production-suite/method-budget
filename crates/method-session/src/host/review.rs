//! Review files (src/review.rs): a reviewer's comments beside a budget,
//! and the author merging them back. The host hands the review file in as
//! `file` and gets it back as a download from `review_save`; the desktop
//! reads and writes it beside the budget instead (its src/review.rs).

use serde::Deserialize;
use serde_json::Value;

use super::{json, Call, Downloads, Error, FileIn, FileOut};
use crate::review::{self as rv, Anchor, CommentId, Pick, SettledView};

pub(super) fn dispatch(d: &Call<'_>) -> Option<Result<Value, Error>> {
    Some(match d.command() {
        // `{doc, file?}`: the budget open for review from now on, with the
        // comments of `file` (a review file kept from before) or none.
        "review_start" => d.view_with(|s, a: MaybeFile| {
            let (existing, name) = match a.file {
                Some(f) => (Some(rv::read(&f.name, &f.bytes)?), f.name),
                None => (None, rv::default_file(s)),
            };
            rv::start_reviewing(s, existing, name, None)
        }),
        // `{doc, file}`: a reviewer's file opened on the author's budget.
        "review_open" => d.view_with(|s, a: OneFile| {
            let review = rv::read(&a.file.name, &a.file.bytes)?;
            rv::open_for_author(s, review, a.file.name, None)
        }),
        "review_close" => d.view(|s| {
            rv::close(s);
            Ok(())
        }),
        "review_list" => d.lock().and_then(|s| json(rv::list(&s)?)),
        "review_add" => {
            d.view_with(|s, a: Add| rv::add(s, a.on, &a.text, &a.created, a.author.as_deref()))
        }
        "review_update" => d.view_with(|s, a: Update| rv::update(s, a.id, &a.text, &a.edited)),
        "review_resolve" => d.view_with(|s, a: Resolve| rv::resolve(s, a.id, a.resolved)),
        "review_delete" => d.view_with(|s, a: Id| rv::delete(s, a.id)),
        "review_restore" => d.view_with(|s, a: Id| rv::restore(s, a.id)),
        "review_settle" => (|| {
            let a: Settle = d.args()?;
            let mut s = d.lock()?;
            let done = rv::settle(&mut s, &a.picks, &a.at, a.by.as_deref())?;
            json(SettledView {
                view: s.finish(None),
                message: done.message,
            })
        })(),
        // The review file as a download, under the name it has.
        "review_save" => (|| {
            let mut s = d.lock()?;
            let text = rv::text(&s)?;
            let name = s.review().map(|r| r.file.clone()).unwrap_or_default();
            rv::mark_saved(&mut s, None, None);
            json(Downloads::one(FileOut {
                name,
                bytes: text.into_bytes(),
            }))
        })(),
        _ => return None,
    })
}

#[derive(Deserialize)]
struct MaybeFile {
    #[serde(default)]
    file: Option<FileIn>,
}

#[derive(Deserialize)]
struct OneFile {
    file: FileIn,
}

#[derive(Deserialize)]
struct Add {
    on: Anchor,
    text: String,
    created: String,
    #[serde(default)]
    author: Option<String>,
}

#[derive(Deserialize)]
struct Update {
    id: CommentId,
    text: String,
    edited: String,
}

#[derive(Deserialize)]
struct Resolve {
    id: CommentId,
    resolved: bool,
}

#[derive(Deserialize)]
struct Id {
    id: CommentId,
}

#[derive(Deserialize)]
struct Settle {
    picks: Vec<Pick>,
    at: String,
    #[serde(default)]
    by: Option<String>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::host::test_files::{bytes_of, call, file};
    use crate::Host;

    #[test]
    fn a_review_goes_out_as_a_file_and_comes_back_as_notes() {
        // The reviewer.
        let r = Host::new();
        let v = call(&r, "review_start", json!({}));
        assert_eq!(v["review"]["mode"], "reviewing");
        assert_eq!(v["review"]["file"], "Untitled.review.json");
        let code = v["budget"]["topsheet"][0]["accounts"][0]["code"].clone();
        let v = call(
            &r,
            "review_add",
            json!({"on": {"account": code}, "text": "Why so high?", "created": "2027-01-05T10:00:00Z", "author": "Jordan"}),
        );
        assert_eq!(v["review"]["accounts"], json!([code]));
        assert_eq!(v["review"]["unsaved"], true);
        assert!(r
            .dispatch("set_title", json!({"title": "Changed"}))
            .unwrap_err()
            .message()
            .contains("open for review"));
        let list = call(&r, "review_list", json!({}));
        assert_eq!(list["entries"][0]["text"], "Why so high?");
        let saved = call(&r, "review_save", json!({}));
        assert_eq!(saved["name"], "Untitled.review.json");
        let bytes = bytes_of(&saved["downloads"][0]);
        assert_eq!(call(&r, "get_view", json!({}))["review"]["unsaved"], false);

        // The author, on the same budget.
        let a = Host::new();
        let v = call(
            &a,
            "review_open",
            json!({"file": file("Untitled.review.json", &bytes)}),
        );
        assert_eq!(v["review"]["mode"], "reading");
        assert_eq!(v["review"]["to_settle"], 1);
        let done = call(
            &a,
            "review_settle",
            json!({"picks": [{"id": 1, "action": "merge"}], "at": "2027-01-06T09:00:00Z", "by": "Sky"}),
        );
        assert_eq!(done["message"], "Merged 1 comment as notes.");
        assert_eq!(done["view"]["can_undo"], true);
        assert_eq!(done["view"]["review"]["to_settle"], 0);
        let notes = call(&a, "notes_list", json!({}));
        assert_eq!(notes[0]["note"], "Why so high?");
        assert_eq!(notes[0]["author"], "Jordan");
        // A file that isn't a review is refused in words.
        assert!(a
            .dispatch("review_open", json!({"file": file("x.json", b"{}")}))
            .unwrap_err()
            .message()
            .contains("isn't a review file"));
        call(&a, "review_close", json!({}));
        assert!(call(&a, "get_view", json!({})).get("review").is_none());
    }
}
