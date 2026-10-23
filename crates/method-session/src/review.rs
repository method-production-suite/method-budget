//! Review files (roadmap 3.3; decisions.md, entry 81): a reviewer marks up
//! a budget they can't change, the comments go in a small file beside it
//! (method-format's `review`, docs/FORMAT.md "Review files"), and the
//! author opens that file on their budget, sees each comment in its place
//! and merges it in as a dated note or dismisses it.
//!
//! - **Reviewing** ([`Mode::Reviewing`]): the budget is read-only (every
//!   edit, undo and redo is refused, [`crate::session::reviewing`]);
//!   comments are added on a line, an account, a category, a setup item or
//!   the whole budget, edited, resolved and deleted. The host writes the
//!   file after each change (the desktop beside the budget; a browser
//!   offers it as a download).
//! - **Reading** ([`Mode::Reading`]): the author's budget, editable as
//!   ever. [`settle`] merges the picked comments as notes (the notes model
//!   of decisions.md entry 35: text, author, the comment's own time) in one
//!   undo step, and marks dismissed ones in the review file. Setup items
//!   and the whole budget have no notes, so comments on them stay in the
//!   file; a comment whose place is gone (a line deleted, an account
//!   renumbered) is listed with where it was, never dropped.
//!
//! A merged comment is recognized by its note (same text, author and time
//! on the same place), not by what the file says, so merging twice adds
//! nothing and undoing the merge shows the comment as open again.
//!
//! No file system, no clock: times come from the page, and the host reads
//! and writes the file.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use method_core::{next_note_id, Budget, Note, RowId, TopsheetRow};
use serde::{Deserialize, Serialize};

use method_format::review as fmt;
pub use method_format::review::{
    file_name_for, Action, Anchor, Comment, CommentId, Outcome, Review, Reviewed, SetupKind,
    SetupRef, SUFFIX,
};

use crate::edit::EditResult;
use crate::messages::{msg, word};
use crate::session::Session;

/// Writing comments (the budget read-only) or reading them (to merge).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Reviewing,
    Reading,
}

/// A review file open on a budget ([`Session::review`]).
#[derive(Clone, Debug, PartialEq)]
pub struct ReviewState {
    pub review: Review,
    pub mode: Mode,
    /// The file's name, as shown (`Field Station.review.json`).
    pub file: String,
    /// Where the desktop keeps it; `None` in a browser or before it is
    /// first saved.
    pub path: Option<PathBuf>,
    /// Changed since it was last written.
    pub unsaved: bool,
    /// Changes with every change to the comments.
    pub rev: u64,
    /// Comments merged in this session, each with the budget revision its
    /// merge left ([`Session::state_rev`]), so an undone merge opens the
    /// comment again even though the file says "merged".
    pub merged_steps: HashMap<CommentId, u64>,
    /// The merges that were undone when the file was last written: when
    /// that changes (an undo or redo), the file needs writing again.
    pub written_undone: Vec<CommentId>,
    /// What the file on disk looked like when last read or written (the
    /// desktop's; `None` in a browser or for a file not on disk yet).
    pub stamp: Option<crate::safety::DiskStamp>,
    /// The file changed on disk since then (someone else wrote it): it is
    /// not written over until the person replaces it or saves elsewhere.
    pub conflict: bool,
}

static NEXT_REV: AtomicU64 = AtomicU64::new(1);

fn next_rev() -> u64 {
    NEXT_REV.fetch_add(1, Ordering::Relaxed)
}

impl ReviewState {
    pub fn new(review: Review, mode: Mode, file: String, path: Option<PathBuf>) -> ReviewState {
        ReviewState {
            review,
            mode,
            file,
            path,
            unsaved: false,
            rev: next_rev(),
            merged_steps: HashMap::new(),
            written_undone: Vec::new(),
            stamp: None,
            conflict: false,
        }
    }

    fn touched(&mut self) {
        self.unsaved = true;
        self.rev = next_rev();
    }
}

/// Where a comment stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Waiting for the author.
    Open,
    /// The reviewer ticked it off; the author can still merge or dismiss it.
    Resolved,
    /// In the budget as a note.
    Merged,
    /// The author set it aside.
    Dismissed,
}

impl Status {
    /// Still for the author to settle.
    pub fn to_settle(self) -> bool {
        matches!(self, Status::Open | Status::Resolved)
    }
}

// ---------------------------------------------------------------------------
// Where things are

#[derive(Clone, Copy)]
struct RowAt<'a> {
    ti: usize,
    ai: usize,
    line: usize,
    cat: &'a method_core::Category,
    acct: &'a method_core::Account,
    row: &'a method_core::Detail,
}

/// The budget's lines, accounts and categories by identity, made once per
/// list or view.
struct Index<'a> {
    rows: HashMap<RowId, RowAt<'a>>,
    accounts: HashMap<
        &'a str,
        (
            usize,
            usize,
            &'a method_core::Category,
            &'a method_core::Account,
        ),
    >,
    categories: HashMap<&'a str, (usize, &'a method_core::Category)>,
}

impl<'a> Index<'a> {
    fn new(b: &'a Budget) -> Index<'a> {
        let mut ix = Index {
            rows: HashMap::new(),
            accounts: HashMap::new(),
            categories: HashMap::new(),
        };
        for (ti, t) in b.topsheet.iter().enumerate() {
            let TopsheetRow::Category(cat) = t else {
                continue;
            };
            ix.categories.entry(cat.code.as_str()).or_insert((ti, cat));
            for (ai, acct) in cat.accounts.iter().enumerate() {
                ix.accounts
                    .entry(acct.code.as_str())
                    .or_insert((ti, ai, cat, acct));
                for (i, row) in acct.details.iter().enumerate() {
                    ix.rows.entry(row.id).or_insert(RowAt {
                        ti,
                        ai,
                        line: i + 1,
                        cat,
                        acct,
                        row,
                    });
                }
            }
        }
        ix
    }
}

fn named(code: &str, name: &str) -> String {
    format!("{code} {name}").trim().to_string()
}

fn row_place(r: &RowAt<'_>) -> String {
    let what = r.row.description.trim();
    let acct = named(&r.acct.code, &r.acct.name);
    if what.is_empty() {
        format!("{acct}, line {}", r.line)
    } else {
        format!("{acct}, line {}: {what}", r.line)
    }
}

fn setup_exists(b: &Budget, r: &SetupRef) -> bool {
    let n = r.name.as_str();
    match r.kind {
        SetupKind::Global => b.globals.contains_key(n),
        SetupKind::Fringe => b.fringes.contains_key(n),
        SetupKind::Group => b.groups.contains_key(n),
        SetupKind::Unit => b.units.contains_key(n),
        SetupKind::Set => b.sets.contains_key(n),
        SetupKind::Location => b.locations.contains_key(n),
        SetupKind::Currency => b.currencies.contains_key(n) || b.base_currency == n,
        SetupKind::Charge => b.charges.iter().any(|c| c.name == n),
        SetupKind::Credit => b.credits.iter().any(|c| c.name == n),
    }
}

/// Where an anchor is now: its place in words and how to get there.
#[derive(Clone, Debug, Default)]
struct Found {
    place: String,
    ti: Option<usize>,
    ai: Option<usize>,
    row_id: Option<RowId>,
    line: Option<usize>,
    category: Option<String>,
    account: Option<String>,
    /// The notes there, when the place can hold notes.
    notes: Option<Vec<Note>>,
}

fn find(b: &Budget, ix: &Index<'_>, on: &Anchor) -> Option<Found> {
    match on {
        Anchor::Row(id) => ix.rows.get(id).map(|r| Found {
            place: row_place(r),
            ti: Some(r.ti),
            ai: Some(r.ai),
            row_id: Some(*id),
            line: Some(r.line),
            category: Some(r.cat.code.clone()),
            account: Some(r.acct.code.clone()),
            notes: Some(r.row.notes.clone()),
        }),
        Anchor::Account(code) => ix
            .accounts
            .get(code.as_str())
            .map(|(ti, ai, cat, acct)| Found {
                place: format!("Account {}", named(&acct.code, &acct.name)),
                ti: Some(*ti),
                ai: Some(*ai),
                category: Some(cat.code.clone()),
                account: Some(acct.code.clone()),
                notes: Some(acct.notes.clone()),
                ..Found::default()
            }),
        Anchor::Category(code) => ix.categories.get(code.as_str()).map(|(ti, cat)| Found {
            place: format!("Category {}", named(&cat.code, &cat.name)),
            ti: Some(*ti),
            category: Some(cat.code.clone()),
            notes: Some(cat.notes.clone()),
            ..Found::default()
        }),
        Anchor::Setup(r) => setup_exists(b, r).then(|| Found {
            place: format!("{} {}", r.kind.label(), r.name),
            ..Found::default()
        }),
        Anchor::Budget => Some(Found {
            place: "The whole budget".into(),
            ..Found::default()
        }),
    }
}

/// The note a merged comment became: same text, time and author.
fn is_its_note(n: &Note, c: &Comment) -> bool {
    n.text == c.text
        && n.created == c.created
        && n.author.as_deref().unwrap_or("") == c.author.trim()
}

/// Merged when the file says so (the author may have edited or deleted the
/// note since), or, for a file that doesn't say, when its note is there.
fn status_of(c: &Comment, found: Option<&Found>) -> Status {
    let merged = c
        .outcome
        .as_ref()
        .is_some_and(|o| o.action == Action::Merged)
        || found
            .and_then(|f| f.notes.as_ref())
            .is_some_and(|ns| ns.iter().any(|n| is_its_note(n, c)));
    if merged {
        Status::Merged
    } else if c
        .outcome
        .as_ref()
        .is_some_and(|o| o.action == Action::Dismissed)
    {
        Status::Dismissed
    } else if c.resolved {
        Status::Resolved
    } else {
        Status::Open
    }
}

/// Why a comment can't become a note, if it can't.
fn why_not(c: &Comment, found: Option<&Found>) -> Option<String> {
    match (found, &c.on) {
        (None, Anchor::Row(_)) => Some(word("engine.review.line_gone")),
        (None, Anchor::Account(code)) => Some(msg("engine.review.account_gone", &[("code", code)])),
        (None, Anchor::Category(code)) => {
            Some(msg("engine.review.category_gone", &[("code", code)]))
        }
        (None, Anchor::Setup(r)) => Some(msg(
            "engine.review.setup_gone",
            &[("what", &setup_word(r.kind)), ("name", &r.name)],
        )),
        (_, Anchor::Setup(_)) => Some(word("engine.review.setup_no_notes")),
        (_, Anchor::Budget) => Some(word("engine.review.budget_no_notes")),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// What the page sees

/// What every view carries while a review file is open.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ReviewBrief {
    pub mode: Mode,
    /// The review file's name.
    pub file: String,
    /// Changes with every change to the comments.
    pub rev: u64,
    /// Changed since it was last written.
    pub unsaved: bool,
    /// The file changed on disk since it was read: not written over until
    /// replaced or saved elsewhere.
    pub conflict: bool,
    /// Comments still to settle (open or resolved).
    pub to_settle: usize,
    pub comments: usize,
    /// The reviewed budget's title, and whether it is this budget's.
    pub title: String,
    pub matches: bool,
    /// Lines, accounts and categories with comments to settle, for the
    /// grid's marks.
    pub rows: Vec<RowId>,
    pub accounts: Vec<String>,
    pub categories: Vec<String>,
}

/// One comment in the Comments tab's list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CommentEntry {
    pub id: CommentId,
    pub on: Anchor,
    pub text: String,
    pub author: String,
    pub created: String,
    pub edited: Option<String>,
    pub resolved: bool,
    pub outcome: Option<Outcome>,
    pub status: Status,
    /// Its place is in the budget now.
    pub found: bool,
    /// Where it is now, or where it was when written when it is gone.
    pub place: String,
    /// Where it was when written.
    pub written_at: String,
    /// It can be merged as a note now.
    pub can_merge: bool,
    /// Why it can't, when it can't (and isn't merged already).
    pub why_not: Option<String>,
    /// For going there: the topsheet index, the account's index in its
    /// category, the row id and its line number, the codes.
    pub ti: Option<usize>,
    pub ai: Option<usize>,
    pub row_id: Option<RowId>,
    pub line: Option<usize>,
    pub category: Option<String>,
    pub account: Option<String>,
}

/// The Comments tab's list: every comment, in the budget's order (the
/// whole budget's first, then categories, accounts and lines as the
/// topsheet runs, then setup items, then those whose place is gone).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ReviewList {
    #[serde(flatten)]
    pub brief: ReviewBrief,
    pub budget: Reviewed,
    pub entries: Vec<CommentEntry>,
}

fn title_matches(r: &Reviewed, b: &Budget) -> bool {
    let me = b.title.trim().to_lowercase();
    std::iter::once(&r.title)
        .chain(r.source_title.as_ref())
        .any(|t| t.trim().to_lowercase() == me)
}

fn entries(b: &Budget, review: &Review) -> Vec<CommentEntry> {
    let ix = Index::new(b);
    let mut out: Vec<(Vec<usize>, CommentEntry)> = review
        .comments
        .iter()
        .map(|c| {
            let found = find(b, &ix, &c.on);
            let status = status_of(c, found.as_ref());
            let why = why_not(c, found.as_ref());
            let f = found.clone().unwrap_or_default();
            let key = match (&c.on, found.is_some()) {
                (_, false) => vec![4],
                (Anchor::Budget, _) => vec![0],
                (Anchor::Setup(r), _) => vec![
                    3,
                    SetupKind::ALL
                        .iter()
                        .position(|k| *k == r.kind)
                        .unwrap_or(0),
                ],
                _ => vec![
                    1,
                    f.ti.unwrap_or(0),
                    f.ai.map_or(0, |a| a + 1),
                    f.line.unwrap_or(0),
                ],
            };
            let entry = CommentEntry {
                id: c.id,
                on: c.on.clone(),
                text: c.text.clone(),
                author: c.author.clone(),
                created: c.created.clone(),
                edited: c.edited.clone(),
                resolved: c.resolved,
                outcome: c.outcome.clone(),
                status,
                found: found.is_some(),
                place: if found.is_some() {
                    f.place.clone()
                } else if c.place.is_empty() {
                    anchor_words(&c.on)
                } else {
                    c.place.clone()
                },
                written_at: c.place.clone(),
                can_merge: why.is_none() && status != Status::Merged,
                why_not: (status != Status::Merged).then_some(why).flatten(),
                ti: f.ti,
                ai: f.ai,
                row_id: f.row_id,
                line: f.line,
                category: f.category,
                account: f.account,
            };
            (key, entry)
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.id.cmp(&b.1.id)));
    out.into_iter().map(|(_, e)| e).collect()
}

/// An anchor in words, without the budget (`line 12`, `account 21-01`).
pub fn anchor_words(on: &Anchor) -> String {
    match on {
        Anchor::Row(id) => format!("Line (row id {id})"),
        Anchor::Account(c) => format!("Account {c}"),
        Anchor::Category(c) => format!("Category {c}"),
        Anchor::Setup(r) => format!("{} {}", r.kind.label(), r.name),
        Anchor::Budget => "The whole budget".into(),
    }
}

fn brief_of(b: &Budget, st: &ReviewState, list: &[CommentEntry]) -> ReviewBrief {
    let mut rows = Vec::new();
    let mut accounts = Vec::new();
    let mut categories = Vec::new();
    for e in list.iter().filter(|e| e.status.to_settle() && e.found) {
        match &e.on {
            Anchor::Row(id) => rows.push(*id),
            Anchor::Account(c) => accounts.push(c.clone()),
            Anchor::Category(c) => categories.push(c.clone()),
            _ => {}
        }
    }
    for v in [&mut accounts, &mut categories] {
        v.sort();
        v.dedup();
    }
    rows.sort_unstable();
    rows.dedup();
    ReviewBrief {
        mode: st.mode,
        file: st.file.clone(),
        rev: st.rev,
        unsaved: st.unsaved,
        conflict: st.conflict,
        to_settle: list.iter().filter(|e| e.status.to_settle()).count(),
        comments: list.len(),
        title: st.review.budget.title.clone(),
        matches: title_matches(&st.review.budget, b),
        rows,
        accounts,
        categories,
    }
}

/// Merges made in this session that are undone now, in id order.
fn undone_merges(s: &Session, st: &ReviewState) -> Vec<CommentId> {
    let mut ids: Vec<CommentId> = st
        .merged_steps
        .iter()
        .filter(|(_, rev)| s.rev_is_undone(**rev))
        .map(|(id, _)| *id)
        .collect();
    ids.sort_unstable();
    ids
}

/// The review as it stands with the budget: a merge taken back by Undo is
/// no longer "merged".
fn effective(s: &Session, st: &ReviewState) -> Review {
    let undone = undone_merges(s, st);
    let mut review = st.review.clone();
    for c in &mut review.comments {
        if undone.contains(&c.id)
            && c.outcome
                .as_ref()
                .is_some_and(|o| o.action == Action::Merged)
        {
            c.outcome = None;
        }
    }
    review
}

/// Whether the review file needs writing: a comment changed, or an undo or
/// redo changed which merges stand.
pub fn needs_write(s: &Session) -> bool {
    s.review()
        .is_some_and(|st| st.unsaved || undone_merges(s, st) != st.written_undone)
}

/// What a view carries about the review file open on the session.
pub fn brief(s: &Session) -> Option<ReviewBrief> {
    let st = s.review()?;
    let review = effective(s, st);
    let list = entries(s.budget(), &review);
    let mut brief = brief_of(s.budget(), st, &list);
    brief.unsaved = needs_write(s);
    Some(brief)
}

/// The list for the review file open on budget `b`.
pub fn list_of(b: &Budget, st: &ReviewState) -> ReviewList {
    let entries = entries(b, &st.review);
    ReviewList {
        brief: brief_of(b, st, &entries),
        budget: st.review.budget.clone(),
        entries,
    }
}

/// The list for the session's review file.
pub fn list(s: &Session) -> EditResult<ReviewList> {
    let st = s.review().ok_or_else(no_review)?;
    let review = effective(s, st);
    let entries = entries(s.budget(), &review);
    let mut brief = brief_of(s.budget(), st, &entries);
    brief.unsaved = needs_write(s);
    Ok(ReviewList {
        brief,
        budget: review.budget.clone(),
        entries,
    })
}

// ---------------------------------------------------------------------------
// Opening and closing

/// What a command on comments is told when no review file is open.
/// What a review command is told when no review file is open.
pub fn no_review() -> String {
    word("engine.review.none_open")
}

/// The noun for a kind of setup item a comment can be on.
fn setup_word(kind: SetupKind) -> String {
    word(match kind {
        SetupKind::Global => "engine.what.global",
        SetupKind::Fringe => "engine.what.fringe",
        SetupKind::Group => "engine.what.group",
        SetupKind::Unit => "engine.what.unit",
        SetupKind::Set => "engine.what.set",
        SetupKind::Location => "engine.what.location",
        SetupKind::Currency => "engine.what.currency",
        SetupKind::Charge => "engine.what.charge",
        SetupKind::Credit => "engine.what.credit",
    })
}

/// What the reviewed budget is, from the budget: its title, and where a
/// copy saved for sharing came from.
pub fn reviewed(b: &Budget) -> Reviewed {
    let p = b.provenance.as_ref().filter(|p| p.is_shared_copy());
    Reviewed {
        title: b.title.clone(),
        source_title: p.map(|p| p.source_title.clone()),
        source_version: p.and_then(|p| p.source_version.clone()),
        saved: p.map(|p| p.saved.clone()),
    }
}

/// The review file's name beside budget `s`'s file
/// (`Field Station.review.json`).
pub fn default_file(s: &Session) -> String {
    file_name_for(&s.display_name())
}

/// What [`settle`] replies to the page: the view and what was done.
#[derive(Clone, Debug, Serialize)]
pub struct SettledView {
    pub view: crate::View,
    pub message: String,
}

/// Reads a review file's bytes, `name` naming it in the error.
pub fn read(name: &str, bytes: &[u8]) -> EditResult<Review> {
    fmt::read(bytes).map_err(|e| msg("engine.review.cant_read", &[("name", &name), ("why", &e)]))
}

/// Opens budget `s` for review: read-only from now on, with the comments
/// of `existing` (a review file found beside it) or none.
pub fn start_reviewing(
    s: &mut Session,
    existing: Option<Review>,
    file: String,
    path: Option<PathBuf>,
) -> EditResult {
    if s.is_shared() {
        return Err(word("engine.review.shared_folder"));
    }
    let review = existing.unwrap_or_else(|| Review::new(reviewed(s.budget())));
    s.set_review(Some(ReviewState::new(review, Mode::Reviewing, file, path)));
    Ok(())
}

/// Opens a review file on the author's budget to read and merge.
pub fn open_for_author(
    s: &mut Session,
    review: Review,
    file: String,
    path: Option<PathBuf>,
) -> EditResult {
    if s.is_reviewing() {
        return Err(word("engine.review.already_reviewing"));
    }
    s.set_review(Some(ReviewState::new(review, Mode::Reading, file, path)));
    Ok(())
}

/// Closes the review file; a budget open for review can be changed again.
pub fn close(s: &mut Session) {
    s.set_review(None);
}

/// The review file's text as it would be written now.
pub fn text(s: &Session) -> EditResult<String> {
    let st = s.review().ok_or_else(no_review)?;
    Ok(fmt::write(&effective(s, st)))
}

/// The review file was written (to `path`, when the host knows it).
pub fn mark_saved(s: &mut Session, path: Option<PathBuf>, file: Option<String>) {
    let undone = s
        .review()
        .map(|st| undone_merges(s, st))
        .unwrap_or_default();
    if let Some(st) = s.review_mut() {
        st.unsaved = false;
        st.written_undone = undone;
        if let Some(p) = path {
            st.path = Some(p);
        }
        if let Some(f) = file {
            st.file = f;
        }
        st.rev = next_rev();
    }
}

// ---------------------------------------------------------------------------
// The reviewer's commands

fn reviewing(s: &mut Session) -> EditResult<&mut ReviewState> {
    match s.review_mut() {
        Some(st) if st.mode == Mode::Reviewing => Ok(st),
        Some(_) => Err(word("engine.review.write_while_reviewing")),
        None => Err(no_review()),
    }
}

fn comment_mut(st: &mut ReviewState, id: CommentId) -> EditResult<&mut Comment> {
    st.review
        .get_mut(id)
        .ok_or_else(|| word("engine.review.comment_gone"))
}

fn clean_author(a: Option<&str>) -> EditResult<String> {
    let a = a.map(str::trim).unwrap_or("");
    fmt::check_author(a)?;
    Ok(a.to_string())
}

/// Adds a comment on `on` (which must be in the budget); its id.
pub fn add(
    s: &mut Session,
    on: Anchor,
    text: &str,
    created: &str,
    author: Option<&str>,
) -> EditResult<CommentId> {
    fmt::check_anchor(&on)?;
    fmt::check_text(text)?;
    let created = created.trim();
    fmt::check_stamp(created)?;
    let author = clean_author(author)?;
    let place = {
        let b = s.budget();
        let ix = Index::new(b);
        find(b, &ix, &on)
            .ok_or_else(|| msg("engine.review.place_gone", &[("place", &anchor_words(&on))]))?
            .place
    };
    let st = reviewing(s)?;
    if st.review.comments.len() >= fmt::MAX_COMMENTS {
        return Err(msg(
            "engine.review.too_many",
            &[("max", &fmt::MAX_COMMENTS)],
        ));
    }
    let id = st.review.next_id();
    st.review.comments.push(Comment {
        id,
        on,
        place,
        text: text.to_string(),
        author,
        created: created.to_string(),
        edited: None,
        resolved: false,
        outcome: None,
    });
    st.touched();
    Ok(id)
}

/// Changes a comment's text, stamping `edited`; blank text deletes it and
/// the same text changes nothing.
pub fn update(s: &mut Session, id: CommentId, text: &str, edited: &str) -> EditResult {
    if text.trim().is_empty() {
        return delete(s, id);
    }
    fmt::check_text(text)?;
    let edited = edited.trim();
    fmt::check_stamp(edited)?;
    let st = reviewing(s)?;
    let c = comment_mut(st, id)?;
    if c.text == text {
        return Ok(());
    }
    c.text = text.to_string();
    c.edited = (!edited.is_empty()).then(|| edited.to_string());
    st.touched();
    Ok(())
}

/// Ticks a comment off as resolved, or opens it again.
pub fn resolve(s: &mut Session, id: CommentId, resolved: bool) -> EditResult {
    let st = reviewing(s)?;
    let c = comment_mut(st, id)?;
    if c.resolved != resolved {
        c.resolved = resolved;
        st.touched();
    }
    Ok(())
}

pub fn delete(s: &mut Session, id: CommentId) -> EditResult {
    let st = reviewing(s)?;
    let before = st.review.comments.len();
    st.review.comments.retain(|c| c.id != id);
    if st.review.comments.len() == before {
        return Err(word("engine.review.comment_gone"));
    }
    st.touched();
    Ok(())
}

// ---------------------------------------------------------------------------
// The author's commands

/// What to do with one comment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PickAction {
    /// Into the budget as a note on its place.
    Merge,
    /// Set aside: marked in the review file, the budget untouched.
    Dismiss,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pick {
    pub id: CommentId,
    pub action: PickAction,
}

/// What [`settle`] did.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Settled {
    pub merged: usize,
    pub dismissed: usize,
    /// Asked to merge, but their place is gone or has no notes: still open.
    pub kept: usize,
    /// Asked to merge, and in the budget already.
    pub already: usize,
    pub message: String,
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn reading(s: &Session) -> EditResult<&ReviewState> {
    match s.review() {
        Some(st) if st.mode == Mode::Reading => Ok(st),
        Some(_) => Err(word("engine.review.own_comments")),
        None => Err(no_review()),
    }
}

/// Merges the comments picked to merge into the budget as dated notes, in
/// one undo step, and marks the dismissed ones in the review file (which
/// the host then writes). A comment whose place is gone or can't hold a
/// note stays open and is counted; one already merged adds nothing. `at`
/// and `by` say when and by whom, for the review file.
pub fn settle(s: &mut Session, picks: &[Pick], at: &str, by: Option<&str>) -> EditResult<Settled> {
    let at = at.trim().to_string();
    fmt::check_stamp(&at)?;
    let by = clean_author(by)?;
    let review = effective(s, reading(s)?);
    for p in picks {
        if review.get(p.id).is_none() {
            return Err(msg("engine.review.not_in_review", &[("id", &p.id)]));
        }
    }
    let list = entries(s.budget(), &review);
    let by_id: HashMap<CommentId, &CommentEntry> = list.iter().map(|e| (e.id, e)).collect();
    let mut done = Settled::default();
    let mut merge: Vec<&Comment> = Vec::new();
    let mut dismiss: Vec<CommentId> = Vec::new();
    for p in picks {
        let (Some(c), Some(e)) = (review.get(p.id), by_id.get(&p.id)) else {
            continue;
        };
        match p.action {
            PickAction::Merge if e.status == Status::Merged => done.already += 1,
            PickAction::Merge if !e.can_merge => done.kept += 1,
            PickAction::Merge => {
                if !merge.iter().any(|m| m.id == c.id) {
                    merge.push(c);
                }
            }
            PickAction::Dismiss => {
                if e.status != Status::Dismissed && !dismiss.contains(&c.id) {
                    dismiss.push(c.id);
                }
            }
        }
    }
    if !merge.is_empty() {
        let label = format!(
            "Merge {} as notes",
            plural(merge.len(), "review comment", "review comments")
        );
        s.edit(label, |b| {
            for c in &merge {
                let owner = match &c.on {
                    Anchor::Row(id) => crate::notes::NoteOwner::Row(*id),
                    Anchor::Account(code) => crate::notes::NoteOwner::Account(code.clone()),
                    Anchor::Category(code) => crate::notes::NoteOwner::Category(code.clone()),
                    _ => continue,
                };
                let notes = crate::notes::notes_mut(b, &owner)?;
                let author = c.author.trim();
                notes.push(Note {
                    id: next_note_id(notes),
                    text: c.text.clone(),
                    created: c.created.clone(),
                    edited: c.edited.clone(),
                    author: (!author.is_empty()).then(|| author.to_string()),
                    done: c.resolved,
                });
            }
            Ok(())
        })?;
        done.merged = merge.len();
    }
    done.dismissed = dismiss.len();
    let merged_ids: Vec<CommentId> = merge.iter().map(|c| c.id).collect();
    let rev = s.state_rev();
    if let Some(st) = s.review_mut() {
        for id in &merged_ids {
            st.merged_steps.insert(*id, rev);
        }
        let mut changed = false;
        for c in &mut st.review.comments {
            let action = if merged_ids.contains(&c.id) {
                Action::Merged
            } else if dismiss.contains(&c.id) {
                Action::Dismissed
            } else {
                continue;
            };
            c.outcome = Some(Outcome {
                action,
                at: at.clone(),
                by: by.clone(),
            });
            changed = true;
        }
        if changed {
            st.touched();
        }
    }
    done.message = settled_message(&done);
    Ok(done)
}

/// What [`settle`] did, in sentences joined by a space (each one a message
/// of its own, so the desktop translates them one by one).
fn settled_message(d: &Settled) -> String {
    let comments = |n: usize| msg("engine.phrase.comments", &[("count", &n)]);
    let mut out = vec![match (d.merged, d.dismissed) {
        (0, 0) => word("engine.review.settled_nothing"),
        (m, 0) => msg("engine.review.settled_merged", &[("count", &m)]),
        (0, n) => msg("engine.review.settled_dismissed", &[("count", &n)]),
        (m, n) => msg(
            "engine.review.settled_both",
            &[("merged", &comments(m)), ("dismissed", &comments(n))],
        ),
    }];
    if d.already > 0 {
        out.push(msg(
            "engine.review.settled_already",
            &[("count", &d.already)],
        ));
    }
    if d.kept > 0 {
        out.push(msg("engine.review.settled_kept", &[("count", &d.kept)]));
    }
    out.join(" ")
}

/// Takes a dismissal back: the comment is open again.
pub fn restore(s: &mut Session, id: CommentId) -> EditResult {
    reading(s)?;
    let Some(st) = s.review_mut() else {
        return Err(no_review());
    };
    let c = comment_mut(st, id)?;
    if c.outcome
        .as_ref()
        .is_some_and(|o| o.action == Action::Dismissed)
    {
        c.outcome = None;
        st.touched();
    }
    Ok(())
}

/// Reads an anchor written on the command line: `row:12`, `account:21-01`,
/// `category:21`, `fringe:FICA` (any setup kind) or `budget`. A bare
/// account number or category code is looked up in `b`.
pub fn parse_anchor(b: &Budget, s: &str) -> EditResult<Anchor> {
    let s = s.trim();
    if s.eq_ignore_ascii_case("budget") {
        return Ok(Anchor::Budget);
    }
    if let Some((kind, rest)) = s.split_once(':') {
        let rest = rest.trim().to_string();
        return match kind.trim().to_ascii_lowercase().as_str() {
            "row" | "line" => rest
                .parse::<RowId>()
                .map(Anchor::Row)
                .map_err(|_| msg("engine.review.not_a_row_id", &[("text", &rest)])),
            "account" => Ok(Anchor::Account(rest)),
            "category" => Ok(Anchor::Category(rest)),
            k => SetupKind::from_key(k)
                .map(|kind| Anchor::Setup(SetupRef { kind, name: rest }))
                .ok_or_else(|| msg("engine.review.not_a_place_kind", &[("kind", &k)])),
        };
    }
    let ix = Index::new(b);
    if ix.accounts.contains_key(s) {
        return Ok(Anchor::Account(s.to_string()));
    }
    if ix.categories.contains_key(s) {
        return Ok(Anchor::Category(s.to_string()));
    }
    Err(msg("engine.review.not_a_place", &[("text", &s)]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes::{budget_notes, NoteOwner};

    /// The starter budget with its first line, account and category.
    fn starter() -> (Session, RowId, String, String) {
        let s = Session::new();
        let c = s.budget().categories().next().unwrap();
        let a = c.accounts.iter().find(|a| !a.details.is_empty()).unwrap();
        let (cat, acct, row) = (c.code.clone(), a.code.clone(), a.details[0].id);
        (s, row, acct, cat)
    }

    fn reviewer() -> (Session, RowId, String, String) {
        let (mut s, row, acct, cat) = starter();
        start_reviewing(&mut s, None, "Untitled.review.json".into(), None).unwrap();
        (s, row, acct, cat)
    }

    #[test]
    fn a_reviewer_comments_on_a_read_only_budget() {
        let (mut s, row, acct, cat) = reviewer();
        let before = s.budget().clone();
        assert!(s.is_reviewing());
        // The budget can't change, nor go back.
        let err = s.edit("Test edit", |b| {
            b.title = "Changed".into();
            Ok(())
        });
        assert_eq!(err.unwrap_err(), crate::session::reviewing());
        assert!(!s.undo());
        assert!(s.jump_to(0).is_err());

        let a = add(
            &mut s,
            Anchor::Row(row),
            "Ten weeks?",
            "2027-01-05T10:00:00Z",
            Some(" Jordan "),
        )
        .unwrap();
        let b2 = add(
            &mut s,
            Anchor::Account(acct.clone()),
            "High overall",
            "t2",
            None,
        )
        .unwrap();
        let c = add(&mut s, Anchor::Category(cat.clone()), "Fine", "t3", None).unwrap();
        let d = add(
            &mut s,
            Anchor::Budget,
            "Overall: good",
            "t4",
            Some("Jordan"),
        )
        .unwrap();
        assert_eq!((a, b2, c, d), (1, 2, 3, 4));
        assert!(add(&mut s, Anchor::Row(999_999), "x", "", None)
            .unwrap_err()
            .contains("isn't in the budget"));
        assert!(add(&mut s, Anchor::Budget, "  ", "", None).is_err());
        let fringe = s.budget().fringes.keys().next().cloned().unwrap();
        let on_fringe = Anchor::Setup(SetupRef {
            kind: SetupKind::Fringe,
            name: fringe.clone(),
        });
        assert_eq!(
            add(&mut s, on_fringe.clone(), "Cap?", "t5", None).unwrap(),
            5
        );
        assert!(add(
            &mut s,
            Anchor::Setup(SetupRef {
                kind: SetupKind::Group,
                name: "Nope".into()
            }),
            "x",
            "",
            None
        )
        .is_err());

        update(&mut s, a, "Ten weeks of prep?", "t6").unwrap();
        resolve(&mut s, c, true).unwrap();
        update(&mut s, d, " ", "t7").unwrap(); // blank deletes
        delete(&mut s, b2).unwrap();
        assert!(delete(&mut s, b2).is_err());

        assert_eq!(s.budget(), &before, "the budget never changed");
        let l = list(&s).unwrap();
        assert_eq!(l.brief.mode, Mode::Reviewing);
        assert!(l.brief.unsaved);
        let ids: Vec<CommentId> = l.entries.iter().map(|e| e.id).collect();
        // Category, then its line, then setup.
        assert_eq!(ids, [3, 1, 5]);
        let e = &l.entries[1];
        assert_eq!(e.text, "Ten weeks of prep?");
        assert_eq!(e.author, "Jordan");
        assert_eq!(e.edited.as_deref(), Some("t6"));
        assert_eq!(e.row_id, Some(row));
        assert_eq!(e.line, Some(1));
        assert!(e.written_at.contains(", line 1"));
        assert_eq!(l.entries[0].status, Status::Resolved);
        assert_eq!(l.entries[2].place, format!("Fringe {fringe}"));
        assert_eq!(l.brief.rows, vec![row]);
        assert_eq!(l.brief.categories, vec![cat]);
        assert_eq!(l.brief.to_settle, 3);

        // The file it writes reads back as the same comments.
        let text = text(&s).unwrap();
        let back = read("x", text.as_bytes()).unwrap();
        assert_eq!(back.comments.len(), 3);
        assert_eq!(back.budget.title, s.budget().title);
        mark_saved(&mut s, None, None);
        assert!(!list(&s).unwrap().brief.unsaved);

        // Closing it makes the budget editable again.
        close(&mut s);
        assert!(s
            .edit("Test edit", |b| {
                b.title = "Changed".into();
                Ok(())
            })
            .is_ok());
    }

    /// A reviewer's file with a comment on each kind of place, read by the
    /// author on the same budget.
    fn authored() -> (Session, Review, RowId, String) {
        let (mut r, row, acct, cat) = reviewer();
        add(
            &mut r,
            Anchor::Row(row),
            "Ten weeks?",
            "2027-01-05T10:00:00Z",
            Some("Jordan"),
        )
        .unwrap();
        add(
            &mut r,
            Anchor::Account(acct.clone()),
            "High",
            "2027-01-05T10:01:00Z",
            Some("Jordan"),
        )
        .unwrap();
        add(
            &mut r,
            Anchor::Category(cat),
            "Fine",
            "2027-01-05T10:02:00Z",
            None,
        )
        .unwrap();
        resolve(&mut r, 3, true).unwrap();
        add(
            &mut r,
            Anchor::Budget,
            "Overall good",
            "2027-01-05T10:03:00Z",
            None,
        )
        .unwrap();
        let review = read("x", text(&r).unwrap().as_bytes()).unwrap();
        let (author, ..) = starter();
        (author, review, row, acct)
    }

    #[test]
    fn the_author_merges_comments_as_notes_in_one_undo_step() {
        let (mut s, review, row, acct) = authored();
        open_for_author(&mut s, review, "Untitled.review.json".into(), None).unwrap();
        assert!(!s.is_reviewing());
        let l = list(&s).unwrap();
        assert!(l.brief.matches);
        assert_eq!(l.brief.to_settle, 4);
        let budget_entry = l.entries.iter().find(|e| e.id == 4).unwrap();
        assert!(!budget_entry.can_merge);
        assert!(budget_entry
            .why_not
            .as_deref()
            .unwrap()
            .contains("stays in the review file"));
        // Comments can't be written here.
        assert!(add(&mut s, Anchor::Budget, "x", "", None).is_err());

        let picks = [
            Pick {
                id: 1,
                action: PickAction::Merge,
            },
            Pick {
                id: 2,
                action: PickAction::Dismiss,
            },
            Pick {
                id: 3,
                action: PickAction::Merge,
            },
            Pick {
                id: 4,
                action: PickAction::Merge,
            },
        ];
        let notes_before = budget_notes(s.budget()).len();
        let done = settle(&mut s, &picks, "2027-01-06T09:00:00Z", Some("Sky")).unwrap();
        assert_eq!(
            (done.merged, done.dismissed, done.kept, done.already),
            (2, 1, 1, 0)
        );
        assert_eq!(
            done.message,
            "Merged 2 comments as notes and dismissed 1 comment. 1 comment stays open: its place is gone or has no notes."
        );
        let notes = budget_notes(s.budget());
        assert_eq!(notes.len(), notes_before + 2);
        let on_row = notes
            .iter()
            .find(|n| n.owner == NoteOwner::Row(row))
            .unwrap();
        assert_eq!(on_row.note, "Ten weeks?");
        assert_eq!(on_row.author.as_deref(), Some("Jordan"));
        assert_eq!(on_row.created, "2027-01-05T10:00:00Z");
        assert!(!on_row.done);
        let on_cat = notes.iter().find(|n| n.kind == "category").unwrap();
        assert!(on_cat.done, "a resolved comment lands ticked off");
        assert!(!notes
            .iter()
            .any(|n| n.owner == NoteOwner::Account(acct.clone())));

        let l = list(&s).unwrap();
        let st = |id| l.entries.iter().find(|e| e.id == id).unwrap().status;
        assert_eq!(
            (st(1), st(2), st(3), st(4)),
            (
                Status::Merged,
                Status::Dismissed,
                Status::Merged,
                Status::Open
            )
        );
        assert!(l.brief.unsaved);
        let file = read("x", text(&s).unwrap().as_bytes()).unwrap();
        let o = file.get(2).unwrap().outcome.clone().unwrap();
        assert_eq!(
            (o.action, o.at.as_str(), o.by.as_str()),
            (Action::Dismissed, "2027-01-06T09:00:00Z", "Sky")
        );
        assert_eq!(
            file.get(1).unwrap().outcome.as_ref().unwrap().action,
            Action::Merged
        );

        // Merging again adds nothing.
        let again = settle(&mut s, &picks[..1], "t", None).unwrap();
        assert_eq!((again.merged, again.already), (0, 1));
        assert_eq!(budget_notes(s.budget()).len(), notes_before + 2);

        // One undo takes both notes back, and the comments show open again.
        assert!(s.undo());
        assert_eq!(budget_notes(s.budget()).len(), notes_before);
        let l = list(&s).unwrap();
        assert_eq!(
            l.entries.iter().find(|e| e.id == 1).unwrap().status,
            Status::Open
        );
        // A dismissal is taken back by restoring.
        restore(&mut s, 2).unwrap();
        assert_eq!(
            list(&s)
                .unwrap()
                .entries
                .iter()
                .find(|e| e.id == 2)
                .unwrap()
                .status,
            Status::Open
        );
        // Unknown ids refuse the whole settle.
        assert!(settle(
            &mut s,
            &[Pick {
                id: 99,
                action: PickAction::Dismiss
            }],
            "",
            None
        )
        .is_err());
    }

    /// A shared budget's log that takes changes only within this person's
    /// role (decisions.md, entry 78): `read_only` refuses every edit up
    /// front; otherwise `record` refuses the change, as a department
    /// editor's role refuses a note outside its categories.
    #[derive(Debug)]
    struct Roles {
        read_only: bool,
        recorded: usize,
    }

    impl crate::collab::SharedLog for Roles {
        fn is_dirty(&self) -> bool {
            false
        }
        fn can_undo(&self) -> bool {
            false
        }
        fn can_redo(&self) -> bool {
            false
        }
        fn randomize_new_rows(
            &mut self,
            _before: &Budget,
            _after: &mut Budget,
        ) -> HashMap<RowId, RowId> {
            HashMap::new()
        }
        fn record(
            &mut self,
            _after: &Budget,
            _hints: &[method_core::log::Hint],
            _label: &str,
        ) -> Result<Option<Budget>, String> {
            Err("Your role doesn't let you change that account.".into())
        }
        fn undo(&mut self) -> Option<(Budget, Option<String>)> {
            None
        }
        fn redo(&mut self) -> Option<(Budget, Option<String>)> {
            None
        }
        fn history(&self) -> crate::session::History {
            crate::session::History {
                steps: Vec::new(),
                current: self.recorded,
                trimmed: false,
            }
        }
        fn history_len(&self) -> usize {
            0
        }
        fn applied_steps(&self) -> usize {
            0
        }
        fn refusal(&self) -> Option<String> {
            self.read_only
                .then(|| "You can read this budget but not change it.".to_string())
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }
    }

    #[test]
    fn merging_into_a_shared_budget_goes_through_the_role_check() {
        for read_only in [true, false] {
            let (mut s, review, ..) = authored();
            s.attach_shared(
                Box::new(Roles {
                    read_only,
                    recorded: 0,
                }),
                None,
            );
            open_for_author(&mut s, review, "r.review.json".into(), None).unwrap();
            let before = s.budget().clone();
            let err = settle(
                &mut s,
                &[
                    Pick {
                        id: 1,
                        action: PickAction::Merge,
                    },
                    Pick {
                        id: 2,
                        action: PickAction::Dismiss,
                    },
                ],
                "t",
                None,
            )
            .unwrap_err();
            let want = if read_only {
                "You can read this budget but not change it."
            } else {
                "Your role doesn't let you change that account."
            };
            assert_eq!(err, want);
            assert_eq!(s.budget(), &before, "no note was added");
            // Nothing was marked in the review file either.
            let l = list(&s).unwrap();
            assert!(l.entries.iter().all(|e| e.outcome.is_none()));
            assert!(!l.brief.unsaved);
            // Dismissing alone changes only the review file: allowed.
            settle(
                &mut s,
                &[Pick {
                    id: 2,
                    action: PickAction::Dismiss,
                }],
                "t",
                None,
            )
            .unwrap();
            assert_eq!(s.budget(), &before);
        }
        // Reviewing a shared budget is refused: its change log is the way.
        let (mut s, ..) = starter();
        s.attach_shared(
            Box::new(Roles {
                read_only: false,
                recorded: 0,
            }),
            None,
        );
        assert!(start_reviewing(&mut s, None, "x".into(), None).is_err());
    }

    #[test]
    fn an_edited_or_deleted_merged_note_is_never_merged_again() {
        let (mut s, review, row, _) = authored();
        open_for_author(&mut s, review, "r.review.json".into(), None).unwrap();
        let all = |s: &Session| -> Vec<Pick> {
            list(s)
                .unwrap()
                .entries
                .iter()
                .filter(|e| e.can_merge)
                .map(|e| Pick {
                    id: e.id,
                    action: PickAction::Merge,
                })
                .collect()
        };
        let first = all(&s);
        assert_eq!(settle(&mut s, &first, "t1", Some("Sky")).unwrap().merged, 3);
        let notes_after_merge = budget_notes(s.budget()).len();
        // The author rewrites the line's note and deletes the account's.
        let owner = NoteOwner::Row(row);
        let id = budget_notes(s.budget())
            .into_iter()
            .find(|n| n.owner == owner)
            .unwrap()
            .id;
        crate::notes::update_note(
            &mut s,
            &owner,
            id,
            "Ten weeks: agreed with the director",
            "t2",
        )
        .unwrap();
        let acct = budget_notes(s.budget())
            .into_iter()
            .find(|n| n.kind == "account")
            .unwrap();
        crate::notes::delete_note(&mut s, &acct.owner, acct.id).unwrap();
        // Still merged, so Merge All has nothing to do.
        let l = list(&s).unwrap();
        assert!(l
            .entries
            .iter()
            .filter(|e| e.id <= 3)
            .all(|e| e.status == Status::Merged));
        assert!(all(&s).is_empty());
        let picks: Vec<Pick> = (1..=3)
            .map(|id| Pick {
                id,
                action: PickAction::Merge,
            })
            .collect();
        let again = settle(&mut s, &picks, "t3", None).unwrap();
        assert_eq!((again.merged, again.already), (0, 3));
        assert_eq!(budget_notes(s.budget()).len(), notes_after_merge - 1);
    }

    #[test]
    fn undoing_a_merge_opens_the_comments_and_the_file_follows() {
        let (mut s, review, ..) = authored();
        open_for_author(&mut s, review, "r.review.json".into(), None).unwrap();
        settle(
            &mut s,
            &[Pick {
                id: 1,
                action: PickAction::Merge,
            }],
            "t",
            None,
        )
        .unwrap();
        mark_saved(&mut s, None, None);
        assert!(!needs_write(&s));
        let merged = |s: &Session| text(s).unwrap().contains("\"merged\"");
        assert!(merged(&s));
        assert!(s.undo());
        // Open again, the file to write says so, and it needs writing.
        assert_eq!(
            list(&s)
                .unwrap()
                .entries
                .iter()
                .find(|e| e.id == 1)
                .unwrap()
                .status,
            Status::Open
        );
        assert!(!merged(&s));
        assert!(needs_write(&s));
        assert!(brief(&s).unwrap().unsaved);
        assert!(s.redo());
        assert!(merged(&s));
        assert!(!needs_write(&s));
        // Undone and then replaced by another edit: still open, and it can
        // be merged again.
        assert!(s.undo());
        s.edit("Test edit", |b| {
            b.title = "Retitled".into();
            Ok(())
        })
        .unwrap();
        let e = list(&s)
            .unwrap()
            .entries
            .into_iter()
            .find(|e| e.id == 1)
            .unwrap();
        assert_eq!(e.status, Status::Open);
        assert!(e.can_merge);
        assert_eq!(
            settle(
                &mut s,
                &[Pick {
                    id: 1,
                    action: PickAction::Merge
                }],
                "t",
                None
            )
            .unwrap()
            .merged,
            1
        );
    }

    #[test]
    fn comments_whose_place_is_gone_are_listed_not_dropped() {
        let (mut s, review, row, acct) = authored();
        // The author deleted the line and renumbered the account since.
        s.edit("Test edit", |b| {
            for c in b.categories_mut() {
                for a in &mut c.accounts {
                    a.details.retain(|d| d.id != row);
                    if a.code == acct {
                        a.code = "99-99".into();
                    }
                }
            }
            Ok(())
        })
        .unwrap();
        open_for_author(&mut s, review, "r.review.json".into(), None).unwrap();
        let l = list(&s).unwrap();
        assert_eq!(l.entries.len(), 4);
        let gone: Vec<&CommentEntry> = l.entries.iter().filter(|e| !e.found).collect();
        assert_eq!(gone.len(), 2);
        // Listed last, saying where they were.
        assert_eq!(
            l.entries[2..].iter().map(|e| e.found).collect::<Vec<_>>(),
            [false, false]
        );
        assert!(gone
            .iter()
            .all(|e| !e.can_merge && e.status == Status::Open));
        assert!(gone[0].place.contains("line 1"));
        assert!(gone.iter().any(|e| e
            .why_not
            .as_deref()
            .unwrap()
            .contains("no longer in the budget")));
        assert!(gone
            .iter()
            .any(|e| e.why_not.as_deref().unwrap().contains("renumbered")));
        let picks: Vec<Pick> = l
            .entries
            .iter()
            .map(|e| Pick {
                id: e.id,
                action: PickAction::Merge,
            })
            .collect();
        let done = settle(&mut s, &picks, "t", None).unwrap();
        assert_eq!((done.merged, done.kept), (1, 3));
        // Still in the file, still open.
        let after = list(&s).unwrap();
        assert_eq!(after.entries.len(), 4);
        assert_eq!(after.brief.to_settle, 3);
    }

    #[test]
    fn a_review_of_another_budget_says_so() {
        let (mut s, review, ..) = authored();
        s.edit("Test edit", |b| {
            b.title = "Something Else".into();
            Ok(())
        })
        .unwrap();
        open_for_author(&mut s, review, "r.review.json".into(), None).unwrap();
        assert!(!list(&s).unwrap().brief.matches);
    }

    #[test]
    fn a_shared_copy_records_where_it_came_from() {
        let mut b = method_core::starter::budget();
        b.provenance = Some(method_core::Provenance {
            kind: method_core::ProvenanceKind::SharedCopy,
            source_title: "Field Station".into(),
            source_version: Some("Draft 2".into()),
            saved: "2027-01-04T09:00:00Z".into(),
            app_version: "0.3.0".into(),
            note: None,
            pack: None,
        });
        let r = reviewed(&b);
        assert_eq!(r.source_title.as_deref(), Some("Field Station"));
        assert_eq!(r.source_version.as_deref(), Some("Draft 2"));
        assert_eq!(r.saved.as_deref(), Some("2027-01-04T09:00:00Z"));
        // The author's budget is the original, titled as the source.
        let mut author = method_core::starter::budget();
        author.title = "Field Station".into();
        assert!(title_matches(&r, &author));
    }

    #[test]
    fn anchors_read_from_the_command_line() {
        let (s, _, acct, cat) = starter();
        let b = s.budget();
        assert_eq!(parse_anchor(b, "row:12").unwrap(), Anchor::Row(12));
        assert_eq!(
            parse_anchor(b, &acct).unwrap(),
            Anchor::Account(acct.clone())
        );
        assert_eq!(
            parse_anchor(b, &cat).unwrap(),
            Anchor::Category(cat.clone())
        );
        assert_eq!(
            parse_anchor(b, "Fringe:FICA").unwrap(),
            Anchor::Setup(SetupRef {
                kind: SetupKind::Fringe,
                name: "FICA".into()
            })
        );
        assert_eq!(parse_anchor(b, "budget").unwrap(), Anchor::Budget);
        assert!(parse_anchor(b, "row:x").is_err());
        assert!(parse_anchor(b, "spaceship:x").is_err());
        assert!(parse_anchor(b, "no-such-code").is_err());
    }
}
