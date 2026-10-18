//! Review files: a reviewer's comments on a budget, kept beside it in a
//! small sidecar file (`<budget>.review.json`) that the author opens and
//! merges back (roadmap 3.3; docs/FORMAT.md, "Review files"; decisions.md,
//! entry 81).
//!
//! Its own format (`"format": "method-budget-review"`, version 1), apart
//! from the `.mthd`: the budget file never changes when someone reviews it.
//! Each comment is anchored to something with a stable identity in the
//! budget: a line by its row id, an account by its number, a category by its
//! code, a setup item by its kind and name, or the whole budget. It keeps
//! where it was when written (`where`), so a comment whose anchor has gone
//! can still say what it was about.
//!
//! The file is untrusted: it is read whole under a size cap, every field is
//! checked, fields this build doesn't know are refused (as the `.mthd`
//! reader does, so a version number always means a shape), and a bad file
//! is an error in words, never a panic. Writing is deterministic: pretty
//! JSON, two-space indent, a fixed key order, empty fields left out, a
//! trailing newline.

use std::collections::BTreeSet;
use std::fmt;

use method_core::{RowId, MAX_NOTE_CHARS};
use serde::{Deserialize, Serialize};

/// The `format` field of a review file.
pub const FORMAT: &str = "method-budget-review";
/// The newest version this build reads and writes.
pub const VERSION: u64 = 1;
/// What a review file's name ends with, after the budget's name without its
/// extension: `Field Station.review.json`.
pub const SUFFIX: &str = ".review.json";
/// Largest review file read, in bytes.
pub const MAX_FILE_BYTES: usize = 8 * 1024 * 1024;
/// Most comments in one file.
pub const MAX_COMMENTS: usize = 10_000;
/// Longest author name, in characters (as for notes).
pub const MAX_AUTHOR_CHARS: usize = 100;
/// Longest time stamp, in characters (as for notes).
pub const MAX_STAMP_CHARS: usize = 64;
/// Longest `where`, title or setup item name, in characters.
pub const MAX_LABEL_CHARS: usize = 1_000;

/// A comment's id, unique within its file.
pub type CommentId = u64;

/// A whole review file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Review {
    /// Always [`FORMAT`].
    pub format: String,
    /// Always [`VERSION`] when written by this build.
    pub version: u64,
    /// The budget the comments are about.
    pub budget: Reviewed,
    /// Oldest first, as written.
    #[serde(default)]
    pub comments: Vec<Comment>,
}

/// Which budget was reviewed, so the author can tell a review of another
/// budget. The file names no path: nothing about the reviewer's disk
/// travels.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reviewed {
    /// The reviewed budget's title.
    pub title: String,
    /// When the reviewed budget is a copy saved for sharing (decisions.md,
    /// entry 39): the title of the budget it was made from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_title: Option<String>,
    /// That copy's version name, when it was made from a saved version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_version: Option<String>,
    /// When that copy was made (ISO 8601, from its provenance).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saved: Option<String>,
}

/// What a comment is about.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Anchor {
    /// A line (any detail row), by its id.
    Row(RowId),
    /// An account, by its number.
    Account(String),
    /// A category, by its code.
    Category(String),
    /// A setup item, by kind and name.
    Setup(SetupRef),
    /// The budget as a whole.
    Budget,
}

/// A setup item a comment is on.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetupRef {
    pub kind: SetupKind,
    pub name: String,
}

/// The kinds of setup item a comment can be on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupKind {
    Global,
    Fringe,
    Group,
    Unit,
    Set,
    Location,
    Currency,
    Charge,
    Credit,
}

impl SetupKind {
    pub const ALL: [SetupKind; 9] = [
        SetupKind::Global,
        SetupKind::Fringe,
        SetupKind::Group,
        SetupKind::Unit,
        SetupKind::Set,
        SetupKind::Location,
        SetupKind::Currency,
        SetupKind::Charge,
        SetupKind::Credit,
    ];

    /// The word the file uses (`fringe`).
    pub fn key(self) -> &'static str {
        match self {
            SetupKind::Global => "global",
            SetupKind::Fringe => "fringe",
            SetupKind::Group => "group",
            SetupKind::Unit => "unit",
            SetupKind::Set => "set",
            SetupKind::Location => "location",
            SetupKind::Currency => "currency",
            SetupKind::Charge => "charge",
            SetupKind::Credit => "credit",
        }
    }

    /// The kind as a person reads it at the start of a sentence (`Fringe`).
    pub fn label(self) -> &'static str {
        match self {
            SetupKind::Global => "Global",
            SetupKind::Fringe => "Fringe",
            SetupKind::Group => "Group",
            SetupKind::Unit => "Unit",
            SetupKind::Set => "Set",
            SetupKind::Location => "Location",
            SetupKind::Currency => "Currency",
            SetupKind::Charge => "Charge",
            SetupKind::Credit => "Credit",
        }
    }

    /// The kind from the file's word, in any letter case.
    pub fn from_key(s: &str) -> Option<SetupKind> {
        SetupKind::ALL
            .into_iter()
            .find(|k| k.key().eq_ignore_ascii_case(s.trim()))
    }
}

/// One comment.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Comment {
    pub id: CommentId,
    pub on: Anchor,
    /// Where the anchor was when the comment was written, in words
    /// (`21-01 Director, line 3: Director`), for when it has gone since.
    #[serde(rename = "where", default, skip_serializing_if = "String::is_empty")]
    pub place: String,
    /// Plain text, at most [`MAX_NOTE_CHARS`] characters (so it can become a
    /// note).
    pub text: String,
    /// Who wrote it: free text, as a note's author.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub author: String,
    /// When it was written (ISO 8601, from the reviewer's computer).
    pub created: String,
    /// When its text last changed, if it has.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edited: Option<String>,
    /// The reviewer ticked it off: settled, or no longer needed.
    #[serde(default, skip_serializing_if = "is_false")]
    pub resolved: bool,
    /// What the author did with it, once they have.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<Outcome>,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// What the author did with a comment.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Outcome {
    pub action: Action,
    /// When (ISO 8601).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub at: String,
    /// By whom: free text.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub by: String,
}

/// Merged into the budget as a note, or dismissed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Merged,
    Dismissed,
}

/// Why a review file can't be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewError(pub String);

impl fmt::Display for ReviewError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ReviewError {}

fn bad(msg: impl Into<String>) -> ReviewError {
    ReviewError(msg.into())
}

impl Review {
    /// An empty review of a budget.
    pub fn new(budget: Reviewed) -> Review {
        Review {
            format: FORMAT.to_string(),
            version: VERSION,
            budget,
            comments: Vec::new(),
        }
    }

    /// The id a new comment gets: one past the highest.
    pub fn next_id(&self) -> CommentId {
        self.comments.iter().map(|c| c.id).max().unwrap_or(0) + 1
    }

    pub fn get(&self, id: CommentId) -> Option<&Comment> {
        self.comments.iter().find(|c| c.id == id)
    }

    pub fn get_mut(&mut self, id: CommentId) -> Option<&mut Comment> {
        self.comments.iter_mut().find(|c| c.id == id)
    }

    /// Everything [`read`] checks, for a review made in memory.
    pub fn check(&self) -> Result<(), ReviewError> {
        if self.format != FORMAT {
            return Err(bad(format!(
                "this isn't a review file (its format is \"{}\", not \"{FORMAT}\")",
                clip(&self.format)
            )));
        }
        if self.version > VERSION {
            return Err(bad(format!(
                "this review file is version {}, made by a newer Method Budget; this one reads up to version {VERSION}",
                self.version
            )));
        }
        if self.version == 0 {
            return Err(bad("this review file has no version"));
        }
        label("the budget's title", &self.budget.title)?;
        for t in [
            &self.budget.source_title,
            &self.budget.source_version,
            &self.budget.saved,
        ]
        .into_iter()
        .flatten()
        {
            label("the reviewed budget", t)?;
        }
        if self.comments.len() > MAX_COMMENTS {
            return Err(bad(format!(
                "this review file has {} comments; it can have at most {MAX_COMMENTS}",
                self.comments.len()
            )));
        }
        let mut ids = BTreeSet::new();
        for c in &self.comments {
            if c.id == 0 {
                return Err(bad("a comment has the id 0; ids start at 1"));
            }
            if !ids.insert(c.id) {
                return Err(bad(format!("two comments have the id {}", c.id)));
            }
            check_comment(c).map_err(|e| bad(format!("comment {}: {e}", c.id)))?;
        }
        Ok(())
    }
}

fn clip(s: &str) -> String {
    s.chars().take(40).collect()
}

fn label(what: &str, s: &str) -> Result<(), ReviewError> {
    if s.chars().count() > MAX_LABEL_CHARS {
        return Err(bad(format!(
            "{what} is too long (at most {MAX_LABEL_CHARS} characters)"
        )));
    }
    Ok(())
}

/// A comment's text: not blank, at most [`MAX_NOTE_CHARS`] characters.
pub fn check_text(text: &str) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err("a comment needs some text".into());
    }
    let len = text.chars().count();
    if len > MAX_NOTE_CHARS {
        return Err(format!(
            "that comment is {len} characters; comments can be at most {MAX_NOTE_CHARS}"
        ));
    }
    Ok(())
}

/// An author's or settler's name: at most [`MAX_AUTHOR_CHARS`] characters.
pub fn check_author(a: &str) -> Result<(), String> {
    if a.chars().count() > MAX_AUTHOR_CHARS {
        return Err(format!(
            "author names can be at most {MAX_AUTHOR_CHARS} characters"
        ));
    }
    Ok(())
}

/// A time stamp: at most [`MAX_STAMP_CHARS`] characters.
pub fn check_stamp(s: &str) -> Result<(), String> {
    if s.chars().count() > MAX_STAMP_CHARS {
        return Err("that time is too long".into());
    }
    Ok(())
}

/// An anchor's names: not blank, not too long.
pub fn check_anchor(on: &Anchor) -> Result<(), String> {
    let name = match on {
        Anchor::Row(_) | Anchor::Budget => return Ok(()),
        Anchor::Account(s) | Anchor::Category(s) => s,
        Anchor::Setup(r) => &r.name,
    };
    if name.trim().is_empty() {
        return Err("it is on nothing (a blank name)".into());
    }
    if name.chars().count() > MAX_LABEL_CHARS {
        return Err("the name it is on is too long".into());
    }
    Ok(())
}

fn check_comment(c: &Comment) -> Result<(), String> {
    check_anchor(&c.on)?;
    check_text(&c.text)?;
    check_author(&c.author)?;
    check_stamp(&c.created)?;
    if let Some(e) = &c.edited {
        check_stamp(e)?;
    }
    if c.place.chars().count() > MAX_LABEL_CHARS {
        return Err("its place is too long".into());
    }
    if let Some(o) = &c.outcome {
        check_stamp(&o.at)?;
        check_author(&o.by)?;
    }
    Ok(())
}

/// Reads a review file.
pub fn read(bytes: &[u8]) -> Result<Review, ReviewError> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(bad(format!(
            "this file is {} MB; review files can be at most {} MB",
            bytes.len() / (1024 * 1024),
            MAX_FILE_BYTES / (1024 * 1024)
        )));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| bad("this file isn't text (UTF-8)"))?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    // The format and version first, so a newer or foreign file says so
    // rather than naming a field this build doesn't know.
    let head: serde_json::Value =
        serde_json::from_str(text).map_err(|e| bad(format!("this file isn't JSON: {e}")))?;
    let format = head.get("format").and_then(|f| f.as_str()).unwrap_or("");
    if format != FORMAT {
        return Err(bad(
            "this isn't a review file (it has no \"format\": \"method-budget-review\")",
        ));
    }
    if let Some(v) = head.get("version").and_then(|v| v.as_u64()) {
        if v > VERSION {
            return Err(bad(format!(
                "this review file is version {v}, made by a newer Method Budget; this one reads up to version {VERSION}"
            )));
        }
    }
    let review: Review = serde_json::from_value(head)
        .map_err(|e| bad(format!("this review file can't be read: {e}")))?;
    review.check()?;
    Ok(review)
}

/// Writes a review file: pretty JSON with a fixed key order and a trailing
/// newline, the same bytes for the same review.
pub fn write(review: &Review) -> String {
    let mut out = serde_json::to_string_pretty(review).unwrap_or_default();
    out.push('\n');
    out
}

/// The review file's name for a budget file's name: its name without the
/// extension, then [`SUFFIX`] (`Field Station.mthd` gives
/// `Field Station.review.json`).
pub fn file_name_for(budget_file: &str) -> String {
    let stem = match budget_file.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && ext.eq_ignore_ascii_case(crate::EXTENSION) => stem,
        _ => budget_file,
    };
    format!("{stem}{SUFFIX}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Review {
        let mut r = Review::new(Reviewed {
            title: "Field Station".into(),
            source_title: Some("Field Station".into()),
            source_version: None,
            saved: Some("2027-01-04T09:00:00Z".into()),
        });
        r.comments.push(Comment {
            id: 1,
            on: Anchor::Row(12),
            place: "21-01 Director, line 1: Director".into(),
            text: "Is ten weeks right?".into(),
            author: "Jordan Vale".into(),
            created: "2027-01-05T10:00:00Z".into(),
            edited: None,
            resolved: false,
            outcome: None,
        });
        r.comments.push(Comment {
            id: 2,
            on: Anchor::Setup(SetupRef {
                kind: SetupKind::Fringe,
                name: "FICA".into(),
            }),
            place: "Fringe FICA".into(),
            text: "Check this year's cap.".into(),
            author: String::new(),
            created: "2027-01-05T10:05:00Z".into(),
            edited: Some("2027-01-05T10:06:00Z".into()),
            resolved: true,
            outcome: Some(Outcome {
                action: Action::Dismissed,
                at: "2027-01-06T08:00:00Z".into(),
                by: "Sky".into(),
            }),
        });
        r.comments.push(Comment {
            id: 3,
            on: Anchor::Budget,
            place: String::new(),
            text: "Overall fine.".into(),
            author: String::new(),
            created: String::new(),
            edited: None,
            resolved: false,
            outcome: None,
        });
        r
    }

    #[test]
    fn writes_and_reads_back_the_same_bytes() {
        let r = sample();
        let text = write(&r);
        assert!(text.ends_with("}\n"));
        assert_eq!(read(text.as_bytes()).unwrap(), r);
        assert_eq!(write(&read(text.as_bytes()).unwrap()), text);
        // Key order and empty fields left out.
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["comments"][0]["on"], serde_json::json!({"row": 12}));
        assert_eq!(
            v["comments"][1]["on"],
            serde_json::json!({"setup": {"kind": "fringe", "name": "FICA"}})
        );
        assert_eq!(v["comments"][2]["on"], serde_json::json!("budget"));
        assert!(v["comments"][0].get("resolved").is_none());
        assert!(v["comments"][2].get("author").is_none());
        assert_eq!(v["comments"][1]["outcome"]["action"], "dismissed");
        assert!(text.find("\"format\"").unwrap() < text.find("\"version\"").unwrap());
        assert!(text.find("\"id\"").unwrap() < text.find("\"on\"").unwrap());
    }

    #[test]
    fn a_small_file_written_by_hand_reads() {
        let text = r#"{
  "format": "method-budget-review",
  "version": 1,
  "budget": {"title": "Field Station"},
  "comments": [
    {"id": 4, "on": {"account": "21-01"}, "text": "Why so high?", "created": "2027-01-05"}
  ]
}"#;
        let r = read(text.as_bytes()).unwrap();
        assert_eq!(r.comments[0].on, Anchor::Account("21-01".into()));
        assert_eq!(r.next_id(), 5);
        // No comments at all is a review too.
        let empty = r#"{"format": "method-budget-review", "version": 1, "budget": {"title": ""}}"#;
        assert!(read(empty.as_bytes()).unwrap().comments.is_empty());
    }

    #[test]
    fn bad_files_are_refused_in_words() {
        let err = |t: &str| read(t.as_bytes()).unwrap_err().0;
        assert!(err("not json").contains("isn't JSON"));
        assert!(err(r#"{"format": "method-budget", "version": 1}"#).contains("isn't a review file"));
        assert!(err(
            r#"{"format": "method-budget-review", "version": 2, "budget": {"title": ""}, "new": 1}"#
        )
        .contains("newer Method Budget"));
        assert!(err(r#"{"format": "method-budget-review", "version": 1, "budget": {"title": ""}, "extra": 1}"#)
            .contains("can't be read"));
        let c = |body: &str| {
            err(&format!(
                r#"{{"format": "method-budget-review", "version": 1, "budget": {{"title": ""}}, "comments": [{body}]}}"#
            ))
        };
        assert!(c(r#"{"id": 0, "on": "budget", "text": "x", "created": ""}"#).contains("id 0"));
        assert!(c(r#"{"id": 1, "on": "budget", "text": "x", "created": ""}, {"id": 1, "on": "budget", "text": "y", "created": ""}"#)
            .contains("two comments have the id 1"));
        assert!(
            c(r#"{"id": 1, "on": "budget", "text": "  ", "created": ""}"#)
                .contains("needs some text")
        );
        assert!(
            c(r#"{"id": 1, "on": {"account": " "}, "text": "x", "created": ""}"#)
                .contains("blank name")
        );
        assert!(c(r#"{"id": 1, "on": {"setup": {"kind": "spaceship", "name": "x"}}, "text": "x", "created": ""}"#)
            .contains("can't be read"));
        let long = "x".repeat(MAX_NOTE_CHARS + 1);
        assert!(c(&format!(
            r#"{{"id": 1, "on": "budget", "text": "{long}", "created": ""}}"#
        ))
        .contains("at most"));
        assert!(read(&vec![b' '; MAX_FILE_BYTES + 1])
            .unwrap_err()
            .0
            .contains("MB"));
        assert!(read(&[0xff, 0xfe]).unwrap_err().0.contains("UTF-8"));
    }

    #[test]
    fn the_file_name_sits_beside_the_budget() {
        assert_eq!(
            file_name_for("Field Station.mthd"),
            "Field Station.review.json"
        );
        assert_eq!(file_name_for("a.b.MTHD"), "a.b.review.json");
        assert_eq!(file_name_for("notes.txt"), "notes.txt.review.json");
        assert_eq!(file_name_for("Untitled"), "Untitled.review.json");
    }

    #[test]
    fn setup_kinds_read_their_words() {
        for k in SetupKind::ALL {
            assert_eq!(SetupKind::from_key(k.key()), Some(k));
            assert_eq!(serde_json::to_value(k).unwrap(), serde_json::json!(k.key()));
        }
        assert_eq!(SetupKind::from_key(" Fringe "), Some(SetupKind::Fringe));
        assert_eq!(SetupKind::from_key("account"), None);
    }
}
