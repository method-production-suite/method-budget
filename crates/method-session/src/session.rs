//! One open budget: the authoritative [`Budget`], where it lives, whether it
//! has unsaved changes, undo/redo history, and which account the UI shows.
//! No window, no file system and no clock of its own (src/clock.rs), so the
//! whole edit -> recalc -> view path runs in tests and in a browser alike.
//!
//! Reading and writing the budget's file, noticing changes made to it
//! outside and sharing it through a folder are the desktop's (method-app's
//! src/session.rs, `SessionFiles`); the session keeps what they need to
//! know (the path, the file's [`DiskStamp`], the shared log) and offers
//! them the steps to take ([`Session::mark_saved`],
//! [`Session::adopt_file_version`], [`Session::attach_shared`]).

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use method_core::calc::cache::RecalcCache;
use method_core::{
    selection_totals, starter, Breakdown, Budget, Detail, RowId, RowTotals, SelectionTotals,
    TopsheetRow, Version,
};

use crate::collab::SharedLog;
use crate::diagnose::{lenient_recalc_with, Outcome};
use crate::edit::{self, EditResult};
use crate::report_dto::SelectionDto;
use crate::safety::{self, DiskStamp};
use crate::view::{Focus, FullView, View, ViewState};

/// What Save As is told in a shared budget.
pub const SHARED_STAYS: &str = "A shared budget stays in its shared folder. To give someone a copy, use File > Save a Copy for Sharing.";

/// What an edit to a copy saved for sharing is told.
pub const READ_ONLY: &str =
    "This is a copy saved for sharing, so it opens read-only. Make an editable copy to change it.";

/// What an edit is told while the host shows budgets read-only
/// (`Host::set_read_only`).
pub const HELD_READ_ONLY: &str = "This budget is open read-only here, so it can't be changed.";

/// What an edit is told while the budget is open for review
/// (src/review.rs).
pub const REVIEWING: &str = "This budget is open for review, so it can't be changed. Comments go in the review file beside it.";

/// Undo steps kept. Each step keeps only what its edit changed (see [`Patch`]).
pub const UNDO_LIMIT: usize = 200;

/// How soon a repeat of a merging edit must come to join its undo step
/// ([`Session::edit_merging`]).
pub const MERGE_WINDOW: std::time::Duration = std::time::Duration::from_millis(1000);

/// View revisions, unique across sessions in this process, so a new or
/// opened budget never reuses an old number.
static NEXT_VIEW_REV: AtomicU64 = AtomicU64::new(1);

fn next_view_rev() -> u64 {
    NEXT_VIEW_REV.fetch_add(1, Ordering::Relaxed)
}

/// An account's rows as they were before an edit.
#[derive(Debug)]
enum Rows {
    /// Every row (rows were added, removed or moved).
    All(Vec<Detail>),
    /// The same rows in the same order: just the ones that changed, by index.
    Some(Vec<(usize, Detail)>),
}

/// What one undo or redo step puts back: only what the edit changed.
///
/// Budgets run to thousands of lines and an edit usually touches one cell,
/// so keeping a whole budget per step would cost hundreds of megabytes over
/// a long session. Applying a patch gives back the patch that reverses it.
#[derive(Debug)]
struct Patch {
    /// The version list, when the edit changed it.
    versions: Option<Vec<Version>>,
    body: Body,
}

#[derive(Debug)]
enum Body {
    /// The edit changed nothing outside the version list.
    None,
    /// The whole budget (without its versions): categories or accounts were
    /// added, removed or moved.
    Whole(Box<Budget>),
    /// Every category and account kept its place.
    Parts {
        /// Everything except detail rows and versions, when that changed
        /// (a budget whose accounts have no rows).
        shell: Option<Box<Budget>>,
        /// Rows of the accounts that changed, by account position in budget
        /// order.
        rows: Vec<(usize, Rows)>,
    },
}

/// Same categories and accounts in the same places (contents aside).
fn same_shape(a: &Budget, b: &Budget) -> bool {
    a.topsheet.len() == b.topsheet.len()
        && a.topsheet
            .iter()
            .zip(&b.topsheet)
            .all(|(x, y)| match (x, y) {
                (TopsheetRow::Category(x), TopsheetRow::Category(y)) => {
                    x.accounts.len() == y.accounts.len()
                }
                (TopsheetRow::ProductionTotal(_), TopsheetRow::ProductionTotal(_)) => true,
                _ => false,
            })
}

fn accounts_mut(b: &mut Budget) -> impl Iterator<Item = &mut Vec<Detail>> {
    b.categories_mut()
        .flat_map(|c| &mut c.accounts)
        .map(|a| &mut a.details)
}

/// Takes every account's rows out of `b`, in order.
fn take_rows(b: &mut Budget) -> Vec<Vec<Detail>> {
    accounts_mut(b).map(std::mem::take).collect()
}

/// Puts rows taken by [`take_rows`] back.
fn put_rows(b: &mut Budget, rows: Vec<Vec<Detail>>) {
    for (slot, r) in accounts_mut(b).zip(rows) {
        *slot = r;
    }
}

impl Patch {
    /// What turns `new` back into `old`, or `None` when they are the same.
    /// `old` is used up: its parts move into the patch, nothing is copied.
    fn between(mut old: Budget, new: &mut Budget) -> Option<Patch> {
        let versions = (old.versions != new.versions).then(|| std::mem::take(&mut old.versions));
        old.versions = Vec::new();
        let new_versions = std::mem::take(&mut new.versions);

        let body = if !same_shape(&old, new) {
            Body::Whole(Box::new(old))
        } else {
            let mut rows = Vec::new();
            let mut old_rows = take_rows(&mut old);
            for (k, (o, n)) in old_rows.iter_mut().zip(accounts_mut(new)).enumerate() {
                if o == n {
                    continue;
                }
                let same_ids =
                    o.len() == n.len() && o.iter().zip(n.iter()).all(|(a, b)| a.id == b.id);
                let changed = if same_ids {
                    Rows::Some(
                        o.iter_mut()
                            .zip(n.iter())
                            .enumerate()
                            .filter(|(_, (a, b))| a != b)
                            .map(|(i, (a, _))| (i, std::mem::replace(a, Detail::separator(0))))
                            .collect(),
                    )
                } else {
                    Rows::All(std::mem::take(o))
                };
                rows.push((k, changed));
            }
            // Compare everything else with the rows set aside.
            let new_rows = take_rows(new);
            let shell = (old != *new).then(|| Box::new(old));
            put_rows(new, new_rows);
            if shell.is_none() && rows.is_empty() {
                Body::None
            } else {
                Body::Parts { shell, rows }
            }
        };
        new.versions = new_versions;
        if versions.is_none() && matches!(body, Body::None) {
            return None;
        }
        Some(Patch { versions, body })
    }

    /// Applies the patch to `cur` and returns the patch that undoes it.
    fn apply(self, cur: &mut Budget) -> Patch {
        let versions = self
            .versions
            .map(|v| std::mem::replace(&mut cur.versions, v));
        let body = match self.body {
            Body::None => Body::None,
            Body::Whole(mut b) => {
                b.versions = std::mem::take(&mut cur.versions);
                let mut old = std::mem::replace(cur, *b);
                old.versions = Vec::new();
                Body::Whole(Box::new(old))
            }
            Body::Parts { shell, rows } => {
                let mut slots: Vec<&mut Vec<Detail>> = accounts_mut(cur).collect();
                let mut back = Vec::with_capacity(rows.len());
                for (k, r) in rows {
                    let slot = &mut *slots[k];
                    back.push((
                        k,
                        match r {
                            Rows::All(all) => Rows::All(std::mem::replace(slot, all)),
                            Rows::Some(some) => Rows::Some(
                                some.into_iter()
                                    .map(|(i, d)| (i, std::mem::replace(&mut slot[i], d)))
                                    .collect(),
                            ),
                        },
                    ));
                }
                let shell = shell.map(|mut s| {
                    put_rows(&mut s, take_rows(cur));
                    s.versions = std::mem::take(&mut cur.versions);
                    let mut old = std::mem::replace(cur, *s);
                    old.versions = Vec::new();
                    Box::new(old)
                });
                Body::Parts { shell, rows: back }
            }
        };
        Patch { versions, body }
    }
}

#[derive(Debug)]
struct Snapshot {
    patch: Patch,
    rev: u64,
    /// What the step did, as the History list shows it ("Edit rate on 21-01
    /// line 3"), named by the command that made it.
    label: String,
    /// When the step was made: milliseconds since the Unix epoch (wall clock;
    /// the History list shows it as a time of day).
    at: u64,
}

/// Longest step label kept, in characters (labels name rows and setup
/// items, which can be long).
pub const LABEL_MAX_CHARS: usize = 120;

/// A step label: one line, at most [`LABEL_MAX_CHARS`] characters.
fn tidy_label(label: String) -> String {
    let one_line: String = label
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let t = one_line.trim();
    let t = if t.is_empty() { "Change" } else { t };
    if t.chars().count() <= LABEL_MAX_CHARS {
        return t.to_string();
    }
    let cut: String = t.chars().take(LABEL_MAX_CHARS - 1).collect();
    format!("{}\u{2026}", cut.trim_end())
}

/// One undo step as the History list shows it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct HistoryStep {
    pub label: String,
    /// Milliseconds since the Unix epoch.
    pub at: u64,
    /// Applied (an undo step), or undone (a redo step, after the current
    /// position).
    pub done: bool,
}

/// Every undo and redo step of a budget, oldest first. `current` is how
/// many of them are applied: position 0 is the state before the first one
/// listed (when the budget was opened or created, or the oldest state still
/// kept once more than [`UNDO_LIMIT`] steps were made: `trimmed`).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct History {
    pub steps: Vec<HistoryStep>,
    pub current: usize,
    pub trimmed: bool,
}

#[derive(Debug)]
pub struct Session {
    budget: Budget,
    path: Option<PathBuf>,
    /// Identifies the current state. Every change gets a fresh number, and undo
    /// brings the old number back, so "dirty" is just `rev != saved_rev`.
    rev: u64,
    next_rev: u64,
    /// The rev that matches the file on disk (or the fresh budget).
    saved_rev: u64,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    outcome: Option<Outcome>,
    /// The account the UI shows; its rows go out with every view.
    focus: Option<Focus>,
    /// Changes with every change to the budget, undo and redo included.
    view_rev: u64,
    /// Each account's evaluated rows, so a recalc after an edit evaluates
    /// only the accounts that changed.
    cache: RecalcCache,
    /// Autosave and recovery bookkeeping (src/safety.rs).
    safety: SafetyState,
    /// The last [`Session::edit_merging`] step: the key the next one must
    /// bring to join it, when it was made ([`crate::clock::mono_ms`]), and
    /// the rev it left.
    merge: Option<(String, u64, u64)>,
    /// Older undo steps were dropped (more than [`UNDO_LIMIT`]).
    trimmed: bool,
    /// The actuals summed by account, kept until they or the account pattern
    /// change, so an edit's view costs one pass over the accounts, not over
    /// every transaction (src/actuals.rs).
    rollups: crate::actuals::RollupCache,
    /// The budget is shared through a folder (src/shared.rs): every edit is
    /// recorded in the change log, undo takes back this person's changes
    /// only, and the timer brings in other people's changes.
    shared: Option<Box<dyn SharedLog>>,
    /// Something to tell the person after the last command (an undo that
    /// couldn't put everything back); the next view carries it.
    notice: Option<String>,
    /// Row ids the last edit gave out and replaced with random ones (a
    /// shared budget): old -> new, so the row a command made can still be
    /// found.
    remap: HashMap<RowId, RowId>,
    /// Where the first Save suggests putting a budget never saved (a budget
    /// converted from an export: next to the export, under its name with
    /// `.mthd`; decisions.md, entry 67).
    save_hint: Option<PathBuf>,
    /// The host shows this budget read-only (a partner's viewer): every
    /// edit, undo and redo is refused ([`HELD_READ_ONLY`]).
    held: bool,
    /// The banner of a budget converted from an export ("Converted from
    /// <file>..."; decisions.md, entry 67): every view carries it until the
    /// first Save. Never written into the file.
    converted: Option<String>,
    /// A review file open on this budget (src/review.rs): a reviewer's
    /// comments, being written (the budget is then read-only) or read by
    /// the author to merge back.
    review: Option<Box<crate::review::ReviewState>>,
}

/// What autosave needs to know about one open budget.
#[derive(Debug)]
struct SafetyState {
    /// Names this document's recovery copy (unique per open document).
    recovery_key: String,
    /// The latest change (edit, undo or redo), on [`crate::clock::mono_ms`].
    last_change: Option<u64>,
    /// The first change not yet in the file or a recovery copy.
    pending_since: Option<u64>,
    /// The rev in the recovery copy, if one was written and not removed.
    recovery_rev: Option<u64>,
    /// Recovered from a crash: the first save into the original file asks.
    confirm_overwrite: bool,
    /// The file whose previous version this session has backed up.
    backed_up: Option<PathBuf>,
    /// The file as we last read or wrote it, to notice changes made outside.
    disk: Option<DiskStamp>,
    /// The file changed outside the app: autosave into it is paused until
    /// the user chooses whose version to keep.
    conflict: bool,
}

impl SafetyState {
    fn new(path: Option<&Path>, recovery_key: Option<String>) -> SafetyState {
        SafetyState {
            recovery_key: recovery_key.unwrap_or_else(|| safety::new_recovery_key(path)),
            last_change: None,
            pending_since: None,
            recovery_rev: None,
            confirm_overwrite: false,
            backed_up: None,
            disk: None,
            conflict: false,
        }
    }
}

impl Default for Session {
    fn default() -> Session {
        Session::new()
    }
}

impl Session {
    /// A session on a fresh starter budget, not yet saved anywhere.
    pub fn new() -> Session {
        Session::with_budget(starter::budget(), None)
    }

    pub fn with_budget(budget: Budget, path: Option<PathBuf>) -> Session {
        let safety = SafetyState::new(path.as_deref(), None);
        Session {
            budget,
            path,
            rev: 0,
            next_rev: 1,
            saved_rev: 0,
            undo: Vec::new(),
            redo: Vec::new(),
            outcome: None,
            focus: None,
            view_rev: next_view_rev(),
            cache: RecalcCache::new(),
            safety,
            merge: None,
            trimmed: false,
            rollups: crate::actuals::RollupCache::default(),
            shared: None,
            notice: None,
            remap: HashMap::new(),
            save_hint: None,
            held: false,
            converted: None,
            review: None,
        }
    }

    /// A budget recovered from a crash: unsaved (dirty), pointing at the file
    /// it came from (`None`: never saved), keeping the recovery copy it came
    /// from under `recovery_key`. Saving into the original file asks first.
    pub fn recovered(budget: Budget, path: Option<PathBuf>, recovery_key: String) -> Session {
        let mut s = Session::with_budget(budget, path);
        s.rev = 1;
        s.next_rev = 2;
        s.safety = SafetyState::new(None, Some(recovery_key));
        s.safety.recovery_rev = Some(1);
        s.safety.confirm_overwrite = s.path.is_some();
        s
    }

    pub fn budget(&self) -> &Budget {
        &self.budget
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Where the first Save suggests putting the budget, when it has no
    /// file yet (a converted export's folder and name).
    pub fn save_hint(&self) -> Option<&Path> {
        self.save_hint.as_deref()
    }

    pub fn set_save_hint(&mut self, path: Option<PathBuf>) {
        self.save_hint = path;
    }

    /// Unsaved changes. In a shared budget: changes of this person's not yet
    /// in the shared folder (or, for the keeper, a file behind the log).
    pub fn is_dirty(&self) -> bool {
        match &self.shared {
            Some(sh) => sh.is_dirty(),
            None => self.rev != self.saved_rev,
        }
    }

    pub fn can_undo(&self) -> bool {
        match &self.shared {
            Some(sh) => sh.can_undo(),
            None => !self.undo.is_empty(),
        }
    }

    pub fn can_redo(&self) -> bool {
        match &self.shared {
            Some(sh) => sh.can_redo(),
            None => !self.redo.is_empty(),
        }
    }

    /// The file name shown in the title bar.
    pub fn display_name(&self) -> String {
        self.path
            .as_deref()
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| format!("Untitled.{}", method_format::EXTENSION))
    }

    /// What the budget's tab (and the Window menu) calls it: the file's
    /// name without its extension, or the budget's title while it has no file.
    pub fn tab_label(&self) -> String {
        if let Some(stem) = self.path.as_deref().and_then(Path::file_stem) {
            return stem.to_string_lossy().into_owned();
        }
        let title = self.budget.title.trim();
        if title.is_empty() {
            "Untitled".to_string()
        } else {
            title.to_string()
        }
    }

    /// Never changed since it was started or read: no edits, nothing to
    /// undo or redo (an Open may take the place of an untouched Untitled).
    pub fn is_untouched(&self) -> bool {
        self.rev == 0 && self.saved_rev == 0 && self.undo.is_empty() && self.redo.is_empty()
    }

    /// The file is saved: `path` holds the budget as it is, whose stamp is
    /// `stamp` (the desktop's Save, after writing it).
    pub fn mark_saved(&mut self, path: &Path, stamp: DiskStamp) {
        self.path = Some(path.to_path_buf());
        self.saved_rev = self.rev;
        self.safety.pending_since = None;
        self.safety.confirm_overwrite = false;
        self.safety.disk = Some(stamp);
        self.safety.conflict = false;
        // Saved as a Method Budget file: no longer just converted.
        self.converted = None;
    }

    /// What the budget's file looked like when it was last read or written.
    pub fn disk_stamp(&self) -> Option<&DiskStamp> {
        self.safety.disk.as_ref()
    }

    pub fn set_disk_stamp(&mut self, stamp: Option<DiskStamp>) {
        self.safety.disk = stamp;
    }

    // Changes made outside the app (a sync service, another computer).

    /// The file changed outside the app and the user hasn't chosen yet:
    /// autosave into it is paused.
    pub fn in_conflict(&self) -> bool {
        self.safety.conflict
    }

    pub fn set_conflict(&mut self, on: bool) {
        self.safety.conflict = on;
    }

    /// Replaces the budget with `theirs`, the file's current content ("Load
    /// theirs", read by the desktop with its `stamp`), as one edit so Undo
    /// brings this version back. Afterwards the budget matches the file: no
    /// unsaved changes, no conflict. A copy saved for sharing (read-only)
    /// takes the file's version too: reading its own file isn't editing it.
    pub fn adopt_file_version(&mut self, theirs: Budget, stamp: DiskStamp) -> Result<(), String> {
        self.edit_unchecked("Load the file's version", &[], |b| {
            *b = theirs;
            Ok(())
        })?;
        self.saved_rev = self.rev;
        self.safety.pending_since = None;
        self.safety.confirm_overwrite = false;
        self.safety.disk = Some(stamp);
        self.safety.conflict = false;
        Ok(())
    }

    // Autosave and recovery (src/safety.rs, src/safety_app.rs).

    /// Names this document's recovery copy.
    pub fn recovery_key(&self) -> &str {
        &self.safety.recovery_key
    }

    /// Notes a change at `at` (every edit, undo and redo does, at now), a
    /// [`crate::clock::mono_ms`] reading.
    pub fn note_change(&mut self, at: u64) {
        self.safety.last_change = Some(at);
        self.safety.pending_since.get_or_insert(at);
    }

    /// Whether autosave should run at `now`: there are unsaved changes not
    /// yet in a recovery copy, and the budget has been quiet long enough (or
    /// changing for too long).
    pub fn autosave_due(&self, now: u64) -> bool {
        self.is_dirty()
            && self.safety.recovery_rev != Some(self.rev)
            && self
                .safety
                .last_change
                .is_some_and(|t| safety::autosave_due(now, t, self.safety.pending_since))
    }

    /// Whether autosave may write into the budget's own file: it has one, and
    /// it isn't a crash recovery still waiting for the user to save it there.
    pub fn can_autosave_to_file(&self) -> bool {
        self.path.is_some() && !self.safety.confirm_overwrite
    }

    /// A recovered budget whose first save into its original file should ask.
    pub fn needs_overwrite_confirm(&self) -> bool {
        self.path.is_some() && self.safety.confirm_overwrite
    }

    /// The budget as file text, for a recovery copy.
    pub fn file_text(&self) -> String {
        method_format::to_string(&self.budget)
    }

    /// The current state is now in the recovery copy.
    pub fn recovery_written(&mut self) {
        self.safety.recovery_rev = Some(self.rev);
        self.safety.pending_since = None;
    }

    /// There is a recovery copy, but nothing unsaved any more (saved, or
    /// undone back to the saved state): the copy should go.
    pub fn recovery_stale(&self) -> bool {
        self.safety.recovery_rev.is_some() && !self.is_dirty()
    }

    /// The recovery copy was deleted.
    pub fn recovery_removed(&mut self) {
        self.safety.recovery_rev = None;
    }

    /// Whether this session has backed up the previous version of `path`
    /// (the first write into a file in a session always does).
    pub fn has_backed_up(&self, path: &Path) -> bool {
        self.safety.backed_up.as_deref() == Some(path)
    }

    /// The file at `path` has had its previous version backed up.
    pub fn backed_up(&mut self, path: &Path) {
        self.safety.backed_up = Some(path.to_path_buf());
    }

    /// Whether the recovery copy holds exactly the current state.
    pub fn recovery_current(&self) -> bool {
        self.safety.recovery_rev == Some(self.rev)
    }

    /// The current view revision (what the next view's `rev` will be).
    pub fn view_rev(&self) -> u64 {
        self.view_rev
    }

    /// The file part of a view: name, path, unsaved changes.
    pub fn file_info(&self) -> crate::view::FileDto {
        crate::view::FileDto {
            path: self.path.as_deref().map(|p| p.display().to_string()),
            name: self.display_name(),
            dirty: self.is_dirty(),
        }
    }

    /// Applies `f` to a copy of the budget. On success the copy becomes current
    /// and what changed goes on the undo stack, named `label` in the History
    /// list ("Edit rate on 21-01 line 3": what the command did); on error
    /// nothing changes. An edit that changes nothing leaves history and the
    /// dirty flag alone.
    pub fn edit<T>(
        &mut self,
        label: impl Into<String>,
        f: impl FnOnce(&mut Budget) -> EditResult<T>,
    ) -> EditResult<T> {
        self.edit_with_hints(label, &[], f)
    }

    /// [`Session::edit`] with what the command knows that a diff can't see
    /// (a rename, a renumbering), so a shared budget's change log records
    /// one write and not a delete and an add (src/shared.rs).
    pub fn edit_with_hints<T>(
        &mut self,
        label: impl Into<String>,
        hints: &[method_core::log::Hint],
        f: impl FnOnce(&mut Budget) -> EditResult<T>,
    ) -> EditResult<T> {
        self.check_writable()?;
        self.edit_unchecked(label, hints, f)
    }

    /// [`Session::edit`] without the read-only check: only for what isn't
    /// the user changing the budget ([`Session::load_from_disk`]).
    fn edit_unchecked<T>(
        &mut self,
        label: impl Into<String>,
        hints: &[method_core::log::Hint],
        f: impl FnOnce(&mut Budget) -> EditResult<T>,
    ) -> EditResult<T> {
        let mut next = self.budget.clone();
        let out = f(&mut next)?;
        self.remap.clear();
        if let Some(sh) = self.shared.as_mut() {
            // Rows made here get ids nobody else can make (decisions.md, 62).
            self.remap = sh.randomize_new_rows(&self.budget, &mut next);
        }
        let label = label.into();
        let old = std::mem::replace(&mut self.budget, next);
        if let Some(patch) = Patch::between(old, &mut self.budget) {
            if let Some(sh) = self.shared.as_mut() {
                match sh.record(&self.budget, hints, &label) {
                    Ok(Some(projected)) => {
                        if projected != self.budget {
                            // The log is the truth: what it projects is
                            // what everyone else will see.
                            self.budget = projected;
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        // Nothing was recorded: the edit is taken back too.
                        let _ = patch.apply(&mut self.budget);
                        return Err(e);
                    }
                }
            }
            self.push_step(patch, self.rev, label);
            self.redo.clear();
            self.rev = self.next_rev;
            self.next_rev += 1;
            self.changed();
        }
        Ok(out)
    }

    // Sharing (src/shared.rs)

    /// Whether the budget is shared through a folder.
    pub fn is_shared(&self) -> bool {
        self.shared.is_some()
    }

    /// The shared log, when the budget is shared.
    pub fn shared_log(&self) -> Option<&dyn SharedLog> {
        self.shared.as_deref()
    }

    pub fn shared_log_mut(&mut self) -> Option<&mut dyn SharedLog> {
        self.shared.as_deref_mut()
    }

    /// The budget is shared from now on, through `log`. With `merged` (a
    /// budget joined: what everyone's changes add up to), that replaces
    /// the budget, keeping its provenance. Steps made before sharing can't
    /// be undone through the log, so the history starts again.
    pub fn attach_shared(&mut self, log: Box<dyn SharedLog>, merged: Option<Budget>) {
        self.shared = Some(log);
        self.undo.clear();
        self.redo.clear();
        self.merge = None;
        self.saved_rev = self.rev;
        self.safety.pending_since = None;
        self.safety.conflict = false;
        if let Some(merged) = merged {
            if merged != self.budget {
                let provenance = self.budget.provenance.clone();
                self.budget = merged;
                self.budget.provenance = provenance;
                self.rev = self.next_rev;
                self.next_rev += 1;
                self.changed();
            }
        }
    }

    /// Runs `f` on the shared log and the budget together (the sharing
    /// timer bringing in other people's changes). When `f` says the budget
    /// changed, it gets a new revision. `None` when the budget isn't shared.
    pub fn with_shared<T>(
        &mut self,
        f: impl FnOnce(&mut dyn SharedLog, &mut Budget) -> (T, bool),
    ) -> Option<T> {
        let mut sh = self.shared.take()?;
        let (out, changed) = f(sh.as_mut(), &mut self.budget);
        self.shared = Some(sh);
        if changed {
            self.rev = self.next_rev;
            self.next_rev += 1;
            self.changed();
        }
        Some(out)
    }

    /// Adopts the budget a shared command produced (a value put back, an
    /// undo through the log) as a change with its own revision.
    pub fn adopt_projected(&mut self, budget: Budget, label: &str) {
        if budget != self.budget {
            let old = std::mem::replace(&mut self.budget, budget);
            if let Some(patch) = Patch::between(old, &mut self.budget) {
                self.push_step(patch, self.rev, label.to_string());
            }
        }
        self.rev = self.next_rev;
        self.next_rev += 1;
        self.changed();
    }

    /// Something the last command wants the person told, once.
    pub fn take_notice(&mut self) -> Option<String> {
        self.notice.take()
    }

    /// Something to tell the person with the next view.
    pub fn set_notice(&mut self, text: String) {
        self.notice = Some(text);
    }

    /// Shows `text` as the banner of a budget converted from an export, on
    /// every view until the first Save ([`Session::mark_saved`]).
    pub fn set_converted(&mut self, text: String) {
        self.converted = Some(text);
    }

    /// The converted budget's banner, until it is first saved.
    pub fn converted(&self) -> Option<&str> {
        self.converted.as_deref()
    }

    /// A copy saved for sharing opens read-only (src/sharing.rs): its
    /// provenance asks for it. Every edit goes through [`Session::edit`],
    /// which refuses while this is true.
    pub fn is_read_only(&self) -> bool {
        self.budget
            .provenance
            .as_ref()
            .is_some_and(method_core::Provenance::is_shared_copy)
    }

    /// Whether the host shows this budget read-only
    /// ([`Session::set_held_read_only`]).
    pub fn is_held_read_only(&self) -> bool {
        self.held
    }

    /// Shows this budget read-only for the host (a partner's viewer), or
    /// lets it be changed again: while on, every edit, undo, redo and
    /// history jump is refused ([`HELD_READ_ONLY`]). Set through
    /// [`crate::docs::Docs::set_read_only`], which covers every tab.
    pub fn set_held_read_only(&mut self, on: bool) {
        self.held = on;
    }

    /// Refused while the host shows budgets read-only ([`HELD_READ_ONLY`]),
    /// for a copy saved for sharing ([`READ_ONLY`]) and in a shared budget
    /// this person may only read ([`SharedLog::refusal`]).
    pub fn check_writable(&self) -> EditResult {
        if self.held {
            return Err(HELD_READ_ONLY.into());
        }
        if self.is_reviewing() {
            return Err(REVIEWING.into());
        }
        if self.is_read_only() {
            return Err(READ_ONLY.into());
        }
        if let Some(why) = self.shared.as_ref().and_then(|sh| sh.refusal()) {
            return Err(why);
        }
        Ok(())
    }

    // Review files (src/review.rs)

    /// The review file open on this budget, if any.
    pub fn review(&self) -> Option<&crate::review::ReviewState> {
        self.review.as_deref()
    }

    pub fn review_mut(&mut self) -> Option<&mut crate::review::ReviewState> {
        self.review.as_deref_mut()
    }

    /// Opens a review file on this budget, or closes it (`None`).
    pub fn set_review(&mut self, review: Option<crate::review::ReviewState>) {
        self.review = review.map(Box::new);
    }

    /// The budget is open for review: comments only, every edit, undo and
    /// redo refused ([`REVIEWING`]).
    pub fn is_reviewing(&self) -> bool {
        self.review
            .as_ref()
            .is_some_and(|r| r.mode == crate::review::Mode::Reviewing)
    }

    /// Puts a step on the undo stack (dropping the oldest past the limit).
    fn push_step(&mut self, patch: Patch, rev: u64, label: String) {
        self.undo.push(Snapshot {
            patch,
            rev,
            label: tidy_label(label),
            at: crate::clock::wall_ms(),
        });
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
            self.trimmed = true;
        }
    }

    /// Like [`Session::edit`], but quick repeats join one undo step: when the
    /// previous step was made by this method less than [`MERGE_WINDOW`] ago,
    /// nothing has changed since (no other edit, undo or redo), and it handed
    /// out `key`, this edit joins that step. `f` returns its result and the
    /// key the next edit must bring to join this one. Moving rows uses it, so
    /// holding Option+Down is one undo step, not one per row passed. A run
    /// that ends where it started leaves no step at all.
    ///
    /// `label` names the step in the History list; a joined step takes the
    /// label of the latest edit that joined it.
    pub fn edit_merging<T>(
        &mut self,
        key: &str,
        label: impl Into<String>,
        f: impl FnOnce(&mut Budget) -> EditResult<(T, String)>,
    ) -> EditResult<T> {
        self.check_writable()?;
        if self.shared.is_some() {
            // Each step is its own change in the log, so none can join.
            let (out, _) = self.edit(label, f)?;
            return Ok(out);
        }
        let now = crate::clock::mono_ms();
        let window = u64::try_from(MERGE_WINDOW.as_millis()).unwrap_or(u64::MAX);
        let joins = matches!(&self.merge, Some((k, at, rev))
            if k == key && *rev == self.rev && now.saturating_sub(*at) < window)
            && !self.undo.is_empty();
        if !joins {
            let before = self.rev;
            let (out, next_key) = self.edit(label, f)?;
            // Only a step this made can be joined: one that changed nothing
            // left someone else's step on top.
            self.merge = (self.rev != before).then_some((next_key, now, self.rev));
            return Ok(out);
        }
        let mut next = self.budget.clone();
        let (out, next_key) = f(&mut next)?;
        if next == self.budget {
            self.merge = Some((next_key, now, self.rev));
            return Ok(out);
        }
        // Back to where the step began, then one step from there to here.
        let Some(top) = self.undo.pop() else {
            unreachable!("checked above")
        };
        let _ = top.patch.apply(&mut self.budget);
        let start = std::mem::replace(&mut self.budget, next);
        // Like any edit, this drops what was undone, even when it comes back
        // to where the step began: an undone step was made on top of this
        // one, so redoing it from here would apply it to a budget it wasn't
        // made from.
        self.redo.clear();
        match Patch::between(start, &mut self.budget) {
            Some(patch) => {
                self.push_step(patch, top.rev, label.into());
                self.rev = self.next_rev;
                self.next_rev += 1;
            }
            None => self.rev = top.rev,
        }
        self.changed();
        // Back where it began: no step of ours is left to join.
        self.merge = (self.rev != top.rev).then_some((next_key, now, self.rev));
        Ok(out)
    }

    /// The budget changed: totals must be worked out again, and views get a
    /// new revision.
    fn changed(&mut self) {
        self.outcome = None;
        self.view_rev = next_view_rev();
        self.note_change(crate::clock::mono_ms());
    }

    pub fn undo(&mut self) -> bool {
        if self.held || self.is_reviewing() {
            return false;
        }
        if self.shared.is_some() {
            return self.shared_step(true);
        }
        if !self.step_back() {
            return false;
        }
        self.changed();
        true
    }

    pub fn redo(&mut self) -> bool {
        if self.held || self.is_reviewing() {
            return false;
        }
        if self.shared.is_some() {
            return self.shared_step(false);
        }
        if !self.step_forward() {
            return false;
        }
        self.changed();
        true
    }

    /// Undo or redo in a shared budget: this person's own step, through the
    /// log (src/shared.rs). What couldn't be put back becomes the notice.
    fn shared_step(&mut self, back: bool) -> bool {
        let Some(sh) = self.shared.as_mut() else {
            return false;
        };
        let done = if back { sh.undo() } else { sh.redo() };
        let Some((budget, note)) = done else {
            return false;
        };
        self.notice = note;
        self.adopt_projected(budget, if back { "Undo" } else { "Redo" });
        true
    }

    /// Undoes one step without telling anyone (undo, and the History jumps).
    fn step_back(&mut self) -> bool {
        let Some(s) = self.undo.pop() else {
            return false;
        };
        let back = s.patch.apply(&mut self.budget);
        self.redo.push(Snapshot {
            patch: back,
            rev: self.rev,
            label: s.label,
            at: s.at,
        });
        self.rev = s.rev;
        true
    }

    /// Redoes one step without telling anyone.
    fn step_forward(&mut self) -> bool {
        let Some(s) = self.redo.pop() else {
            return false;
        };
        let back = s.patch.apply(&mut self.budget);
        self.undo.push(Snapshot {
            patch: back,
            rev: self.rev,
            label: s.label,
            at: s.at,
        });
        self.rev = s.rev;
        true
    }

    // The History list (src/history.rs): every step, a jump to any of them,
    // and the budget as it was at any of them.

    /// Every undo and redo step, oldest first, and how many are applied.
    pub fn history(&self) -> History {
        if let Some(sh) = &self.shared {
            return sh.history();
        }
        let steps = self
            .undo
            .iter()
            .map(|s| (s, true))
            .chain(self.redo.iter().rev().map(|s| (s, false)))
            .map(|(s, done)| HistoryStep {
                label: s.label.clone(),
                at: s.at,
                done,
            })
            .collect();
        History {
            steps,
            current: self.undo.len(),
            trimmed: self.trimmed,
        }
    }

    /// How many history positions there are past the start (every undo and
    /// redo step).
    fn history_len(&self) -> usize {
        self.undo.len() + self.redo.len()
    }

    /// Undoes or redoes steps until `pos` of them are applied (0: the start
    /// of the list). One change, however many steps it passes. Refused
    /// when there is no such position.
    pub fn jump_to(&mut self, pos: usize) -> Result<(), String> {
        if self.held {
            return Err(HELD_READ_ONLY.into());
        }
        if self.is_reviewing() {
            return Err(REVIEWING.into());
        }
        if let Some(sh) = &self.shared {
            if pos > sh.history_len() {
                return Err("That step is no longer in the history.".into());
            }
            let mut notes = Vec::new();
            while self
                .shared
                .as_ref()
                .is_some_and(|s| s.applied_steps() > pos)
                && self.undo()
            {
                notes.extend(self.notice.take());
            }
            while self
                .shared
                .as_ref()
                .is_some_and(|s| s.applied_steps() < pos)
                && self.redo()
            {
                notes.extend(self.notice.take());
            }
            if !notes.is_empty() {
                self.notice = Some(notes.join(" "));
            }
            return Ok(());
        }
        if pos > self.history_len() {
            return Err("That step is no longer in the history.".into());
        }
        if pos == self.undo.len() {
            return Ok(());
        }
        while self.undo.len() > pos && self.step_back() {}
        while self.undo.len() < pos && self.step_forward() {}
        self.changed();
        Ok(())
    }

    /// The budget as it was with `pos` steps applied, leaving the current
    /// one, its history and its totals exactly as they are: the steps are
    /// walked on the budget itself and walked back (a patch applied twice
    /// gives what it started from), so nothing but the copy is made.
    pub fn budget_at(&mut self, pos: usize) -> Result<Budget, String> {
        if self.shared.is_some() {
            return Err(
                "While a budget is shared, versions are saved from the budget as it is now.".into(),
            );
        }
        let n = self.history_len();
        if pos > n {
            return Err("That step is no longer in the history.".into());
        }
        let here = self.undo.len();
        if pos == here {
            return Ok(self.budget.clone());
        }
        // (patch that walks back to here, rev, label, time), in walk order.
        let mut walked: Vec<Snapshot> = Vec::new();
        if pos < here {
            for _ in pos..here {
                let s = self.undo.pop().expect("counted above");
                let back = s.patch.apply(&mut self.budget);
                walked.push(Snapshot { patch: back, ..s });
            }
        } else {
            for _ in here..pos {
                let s = self.redo.pop().expect("counted above");
                let back = s.patch.apply(&mut self.budget);
                walked.push(Snapshot { patch: back, ..s });
            }
        }
        let copy = self.budget.clone();
        let forward = pos < here;
        while let Some(s) = walked.pop() {
            let again = s.patch.apply(&mut self.budget);
            let restored = Snapshot { patch: again, ..s };
            if forward {
                self.undo.push(restored);
            } else {
                self.redo.push(restored);
            }
        }
        Ok(copy)
    }

    /// The (cached) lenient recalc of the current budget.
    pub fn outcome(&mut self) -> &Outcome {
        self.outcome
            .get_or_insert_with(|| lenient_recalc_with(&self.budget, &mut self.cache))
    }

    /// Sets the account the UI shows (`None`: none, on the topsheet and the
    /// account list). Its rows go out with every view from now on.
    pub fn set_focus(&mut self, focus: Option<Focus>) {
        self.focus = focus;
    }

    /// The account on screen, if it still exists: (topsheet index, account index).
    pub fn focused(&self) -> Option<(usize, usize)> {
        self.focus.as_ref().and_then(|f| f.resolve(&self.budget))
    }

    /// What the UI draws after a command.
    pub fn view(&mut self) -> View {
        self.view_with(None)
    }

    /// The view with `focus` on screen instead of the session's own account,
    /// changing nothing: split view's other pane showing this budget at
    /// another account (src/tabs.rs, `pane_view`).
    pub fn view_at(&mut self, focus: Option<&Focus>) -> View {
        self.outcome();
        let focus = focus.and_then(|f| f.resolve(&self.budget));
        let st = ViewState {
            rev: self.view_rev,
            outcome: self.outcome.as_ref().expect("just computed"),
            path: self.path.as_deref(),
            name: self.display_name(),
            dirty: self.is_dirty(),
            can_undo: self.can_undo(),
            can_redo: self.can_redo(),
        };
        let mut view = View::build(&mut self.budget, &st, focus, None);
        view.cost = self.cost(focus, false);
        view.shared = self.shared.is_some();
        view.converted = self.converted.clone();
        view.review = crate::review::brief(self);
        view
    }

    /// The view, noting a row an edit just created so the UI can select it.
    pub fn view_with(&mut self, created_row: Option<RowId>) -> View {
        self.outcome();
        let created_row = created_row.map(|id| self.remap.get(&id).copied().unwrap_or(id));
        let focus = self.focused();
        let st = ViewState {
            rev: self.view_rev,
            outcome: self.outcome.as_ref().expect("just computed"),
            path: self.path.as_deref(),
            name: self.display_name(),
            dirty: self.is_dirty(),
            can_undo: self.can_undo(),
            can_redo: self.can_redo(),
        };
        let mut view = View::build(&mut self.budget, &st, focus, created_row);
        view.cost = self.cost(focus, false);
        view.shared = self.shared.is_some();
        view.notice = self.notice.take();
        view.converted = self.converted.clone();
        view.review = crate::review::brief(self);
        view
    }

    /// Everything, every row (for printing).
    pub fn full_view(&mut self) -> FullView {
        self.outcome();
        let st = ViewState {
            rev: self.view_rev,
            outcome: self.outcome.as_ref().expect("just computed"),
            path: self.path.as_deref(),
            name: self.display_name(),
            dirty: self.is_dirty(),
            can_undo: self.can_undo(),
            can_redo: self.can_redo(),
        };
        let mut view = FullView::build(&mut self.budget, &st);
        view.cost = self.cost(None, true);
        view
    }

    /// The cost figures a view carries when the budget has actuals: every
    /// account, category and summary row, and the line sums of the account
    /// at `focus` (every line's with `all_lines`). The rollup over the
    /// transactions is cached (see `rollups`).
    fn cost(
        &mut self,
        focus: Option<(usize, usize)>,
        all_lines: bool,
    ) -> Option<crate::actuals::CostDto> {
        if self.budget.actuals.is_empty() {
            return None;
        }
        let roll = self.rollups.get(&self.budget);
        let report = self.outcome.as_ref()?.report.as_ref()?;
        let shown: BTreeSet<RowId> = match focus {
            Some((ti, ai)) if !all_lines => match self.budget.topsheet.get(ti) {
                Some(TopsheetRow::Category(c)) => c
                    .accounts
                    .get(ai)
                    .map(|a| a.details.iter().map(|d| d.id).collect())
                    .unwrap_or_default(),
                _ => BTreeSet::new(),
            },
            _ => BTreeSet::new(),
        };
        Some(crate::actuals::cost_dto(
            &self.budget,
            report,
            roll.as_deref(),
            |id| all_lines || shown.contains(&id),
        ))
    }

    /// How many times the actuals were rolled up (for tests of the cache).
    pub fn rollups_computed(&self) -> u64 {
        self.rollups.computed
    }

    /// The budget with its (cached) lenient recalc, borrowed together.
    pub fn totals(&mut self) -> (&Budget, &Outcome) {
        self.outcome();
        (&self.budget, self.outcome.as_ref().expect("just computed"))
    }

    /// Totals for the selected rows, or zeros when totals can't be computed.
    pub fn selection(&mut self, ids: &[RowId]) -> SelectionDto {
        let ids: BTreeSet<RowId> = ids.iter().copied().collect();
        self.outcome();
        let outcome = self.outcome.as_ref().expect("just computed");
        let Some(report) = &outcome.report else {
            return SelectionDto::default();
        };
        let budget = outcome.sanitized.as_ref().unwrap_or(&self.budget);
        selection_totals(budget, report, &ids)
            .map(|s| SelectionDto::from(&s))
            .unwrap_or_default()
    }

    // Convenience wrappers used by the commands and tests.

    pub fn set_detail_field(&mut self, id: RowId, field: &str, value: &str) -> EditResult {
        let label = crate::history::cell_label(&self.budget, id, field);
        let field = edit::Field::parse(field)?;
        self.edit(label, |b| edit::set_detail_field(b, id, field, value))
    }

    pub fn add_detail(
        &mut self,
        account: &str,
        after: Option<RowId>,
        kind: &str,
    ) -> EditResult<RowId> {
        let kind = edit::parse_kind(kind)?;
        let label = crate::history::insert_label(kind, account);
        self.edit(label, |b| edit::add_detail(b, account, after, kind))
    }
}

/// Selection totals for many selections of one budget: the rows are looked
/// up once, so each selection costs only its own rows (the apply panel asks
/// for one per set, location and currency in use).
pub struct SelectionIndex<'a> {
    rows: Option<HashMap<RowId, (&'a Detail, &'a RowTotals)>>,
}

impl<'a> SelectionIndex<'a> {
    /// `budget` and its recalc, as [`Session::totals`] gives them.
    pub fn new(budget: &'a Budget, outcome: &'a Outcome) -> SelectionIndex<'a> {
        let rows = outcome.report.as_ref().map(|report| {
            // The report's rows line up with the budget it was computed from.
            let b = outcome.sanitized.as_ref().unwrap_or(budget);
            b.categories()
                .zip(&report.categories)
                .flat_map(|(cat, ct)| {
                    cat.accounts
                        .iter()
                        .zip(&ct.accounts)
                        .flat_map(|(acct, at)| acct.details.iter().zip(&at.rows))
                })
                .map(|(d, r)| (d.id, (d, r)))
                .collect()
        });
        SelectionIndex { rows }
    }

    /// What [`Session::selection`] gives for `ids` (rows in budget order,
    /// each once), without a pass over the whole budget.
    pub fn totals(&self, ids: &[RowId]) -> SelectionDto {
        fn add(b: &mut Breakdown, r: &RowTotals) -> Option<()> {
            b.details_total = b.details_total.checked_add(r.total)?;
            b.fringe_total = b.fringe_total.checked_add(r.fringe_total)?;
            Some(())
        }
        let Some(rows) = &self.rows else {
            return SelectionDto::default();
        };
        let mut out = SelectionTotals::default();
        let mut sum = || -> Option<()> {
            for (d, r) in ids.iter().filter_map(|id| rows.get(id)) {
                add(&mut out.lines, r)?;
                for (name, amt) in &r.fringes {
                    let slot = out.fringes.entry(name.clone()).or_insert(0);
                    *slot = slot.checked_add(*amt)?;
                }
                for g in &d.groups {
                    add(out.groups.entry(g.clone()).or_default(), r)?;
                }
            }
            Some(())
        };
        match sum() {
            Some(()) => SelectionDto::from(&out),
            None => SelectionDto::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use method_core::sample::{self, SampleSpec};
    use method_core::{Formula, Group, RowKind};

    /// xorshift64*, for random edit sequences.
    struct Rng(u64);

    impl Rng {
        fn below(&mut self, n: u64) -> u64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) % n.max(1)
        }
    }

    fn small_sample(seed: u64) -> Budget {
        sample::budget(&SampleSpec {
            seed,
            rows: 120,
            accounts: 9,
            categories: 3,
            fringes: 6,
            groups: 4,
            baseline: true,
        })
    }

    fn row_ids(b: &Budget) -> Vec<RowId> {
        edit::details(b).map(|d| d.id).collect()
    }

    fn version(id: u64, b: &Budget) -> Version {
        Version {
            id,
            name: format!("v{id}"),
            created: "2026-10-09T00:00:00Z".into(),
            note: None,
            budget: std::sync::Arc::new(b.snapshot()),
        }
    }

    /// One random edit of every kind the app makes: cells, inserts and
    /// deletes, setup tables, structure, versions.
    fn random_edit(rng: &mut Rng, b: &mut Budget) {
        let ids = row_ids(b);
        let id = ids[rng.below(ids.len() as u64) as usize];
        match rng.below(10) {
            0..=2 => {
                let _ = edit::set_detail_field(
                    b,
                    id,
                    edit::Field::Rate,
                    &format!("{}", rng.below(5000)),
                );
            }
            3 => {
                let _ = edit::set_detail_field(b, id, edit::Field::Description, "Changed");
                let other = ids[rng.below(ids.len() as u64) as usize];
                let _ = edit::set_detail_field(b, other, edit::Field::Amount, "SHOOT_DAYS");
            }
            4 => {
                let acct = b
                    .categories()
                    .flat_map(|c| &c.accounts)
                    .nth(rng.below(9) as usize)
                    .map(|a| a.code.clone())
                    .unwrap();
                let _ = edit::add_detail(b, &acct, Some(id), RowKind::Line);
            }
            5 => {
                let _ = edit::delete_details(b, &[id]);
            }
            6 => {
                b.globals.insert(
                    "SHOOT_DAYS".into(),
                    Formula::from(format!("{}", rng.below(60) + 1)),
                );
            }
            7 => {
                b.groups.insert(
                    format!("NEW{}", rng.below(5)),
                    Group {
                        short_code: None,
                        description: String::new(),
                        in_budget_total: rng.below(2) == 0,
                        color: None,
                    },
                );
                let _ = edit::set_detail_field(b, id, edit::Field::Rate, "77");
            }
            8 => {
                // Structure: move the last category to the front.
                if let Some(last) = b.topsheet.pop() {
                    b.topsheet.insert(0, last);
                }
            }
            _ => {
                let next = b.versions.len() as u64 + 1;
                let v = version(next, b);
                b.versions.push(v);
            }
        }
    }

    /// Patch-based undo and redo bring back exactly the budgets a full copy
    /// per step would, over long random edit sequences.
    #[test]
    fn undo_and_redo_restore_every_state() {
        for seed in 1..=6 {
            let mut rng = Rng(seed * 7_919 + 1);
            let mut s = Session::with_budget(small_sample(seed), None);
            let mut states = vec![s.budget().clone()];
            for _ in 0..60 {
                let before = s.budget().clone();
                s.edit("Test edit", |b| {
                    random_edit(&mut rng, b);
                    Ok(())
                })
                .unwrap();
                if *s.budget() != before {
                    states.push(s.budget().clone());
                }
                if rng.below(3) == 0 {
                    // Sometimes break a cell, so partial totals are cached too.
                    let before = s.budget().clone();
                    let id = row_ids(s.budget())[0];
                    let bad = if rng.below(2) == 0 { "NOPE" } else { "7" };
                    s.set_detail_field(id, "amount", bad).unwrap();
                    if *s.budget() != before {
                        states.push(s.budget().clone());
                    }
                }
                s.outcome();
            }
            assert_eq!(s.undo.len(), states.len() - 1);
            // All the way back, then all the way forward; the cached totals
            // are always what a fresh recalc gives.
            let fresh = |s: &mut Session| {
                let want = crate::diagnose::lenient_recalc(s.budget());
                let got = s.outcome();
                assert_eq!(got.report, want.report, "seed {seed}: report");
                assert_eq!(got.error, want.error, "seed {seed}: error");
                assert_eq!(got.issues, want.issues, "seed {seed}: issues");
                assert_eq!(got.sanitized, want.sanitized, "seed {seed}: sanitized");
            };
            fresh(&mut s);
            for want in states.iter().rev().skip(1) {
                assert!(s.undo());
                assert_eq!(s.budget(), want, "seed {seed}: undo");
                fresh(&mut s);
            }
            assert!(!s.undo());
            for want in states.iter().skip(1) {
                assert!(s.redo());
                assert_eq!(s.budget(), want, "seed {seed}: redo");
            }
            assert!(!s.redo());
            // Halfway back, then a new edit drops the redo steps.
            for _ in 0..states.len() / 2 {
                s.undo();
            }
            let at = states.len() - 1 - states.len() / 2;
            assert_eq!(s.budget(), &states[at]);
            s.set_detail_field(row_ids(s.budget())[0], "rate", "12345")
                .unwrap();
            assert!(!s.can_redo());
            assert!(s.undo());
            assert_eq!(s.budget(), &states[at]);
        }
    }

    /// Split view's other pane gets its own account in its view without
    /// moving the budget's own account on screen or changing anything.
    #[test]
    fn a_view_at_another_account_changes_nothing() {
        let mut s = Session::with_budget(small_sample(5), None);
        let accts: Vec<(usize, String, usize, String)> = s
            .budget()
            .topsheet
            .iter()
            .enumerate()
            .filter_map(|(ti, r)| match r {
                TopsheetRow::Category(c) => Some((ti, c)),
                _ => None,
            })
            .flat_map(|(ti, c)| {
                c.accounts
                    .iter()
                    .enumerate()
                    .map(move |(ai, a)| (ti, c.code.clone(), ai, a.code.clone()))
            })
            .collect();
        let focus = |i: usize| {
            let (ti, cat, ai, acct) = accts[i].clone();
            Focus {
                category: cat,
                ti,
                account: acct,
                ai,
            }
        };
        s.set_focus(Some(focus(0)));
        let rev = s.view_rev();
        let mine = s.view();
        let other = s.view_at(Some(&focus(1)));
        assert_eq!(mine.account.as_ref().unwrap().code, accts[0].3);
        assert_eq!(other.account.as_ref().unwrap().code, accts[1].3);
        assert_eq!(other.rev, mine.rev, "the same budget, as it is");
        assert_eq!(
            s.focused(),
            Some((accts[0].0, accts[0].2)),
            "its own account stays"
        );
        assert_eq!(s.view_rev(), rev);
        assert!(s.view_at(None).account.is_none());
        assert_eq!(s.view().account.unwrap().code, accts[0].3);
    }

    /// A one-cell edit keeps one row, not a budget.
    #[test]
    fn a_cell_edit_keeps_only_that_row() {
        let mut s = Session::with_budget(small_sample(3), None);
        let id = row_ids(s.budget())[5];
        s.set_detail_field(id, "rate", "999").unwrap();
        match &s.undo[0].patch {
            Patch {
                versions: None,
                body: Body::Parts { shell: None, rows },
            } => {
                assert_eq!(rows.len(), 1);
                assert!(matches!(&rows[0].1, Rows::Some(r) if r.len() == 1));
            }
            other => panic!("unexpected patch {other:?}"),
        }
        // A setup edit keeps the shell, not the rows.
        s.edit("Test edit", |b| {
            b.globals.insert("X".into(), "1".into());
            Ok(())
        })
        .unwrap();
        assert!(matches!(
            &s.undo[1].patch.body,
            Body::Parts { shell: Some(_), rows } if rows.is_empty()
        ));
        // An edit that changes nothing adds no step.
        s.edit("Test edit", |_| Ok(())).unwrap();
        assert_eq!(s.undo.len(), 2);
    }

    /// The index gives what a pass over the budget gives, for any selection,
    /// with totals partial or missing too.
    #[test]
    fn selection_index_matches_selection_totals() {
        let mut rng = Rng(99);
        for broken in [None, Some("NOPE"), Some("dup")] {
            let mut b = small_sample(5);
            let ids = row_ids(&b);
            match broken {
                Some("NOPE") => {
                    edit::set_detail_field(&mut b, ids[3], edit::Field::Rate, "NOPE").unwrap();
                    edit::set_detail_field(&mut b, ids[9], edit::Field::Groups, "GHOST").unwrap();
                }
                Some(_) => {
                    // Duplicate ids: no totals at all.
                    let first = ids[0];
                    b.categories_mut()
                        .flat_map(|c| &mut c.accounts)
                        .flat_map(|a| &mut a.details)
                        .nth(1)
                        .unwrap()
                        .id = first;
                }
                None => {}
            }
            let mut s = Session::with_budget(b, None);
            let ids = row_ids(s.budget());
            for _ in 0..40 {
                let pick: Vec<RowId> = ids.iter().copied().filter(|_| rng.below(4) == 0).collect();
                let want = s.selection(&pick);
                let (b, o) = s.totals();
                let index = SelectionIndex::new(b, o);
                let mut unique = pick.clone();
                unique.dedup();
                assert_eq!(index.totals(&unique), want, "{broken:?}");
            }
        }
    }

    /// Autosave runs only with unsaved changes, after a quiet spell (or a
    /// long busy one), once per state.
    #[test]
    fn autosave_triggers_follow_changes() {
        let secs = |n: u64| n * 1000;
        let ms = |d: std::time::Duration| u64::try_from(d.as_millis()).unwrap();
        let mut s = Session::with_budget(small_sample(6), None);
        let t0 = crate::clock::mono_ms();
        assert!(!s.autosave_due(t0 + secs(600)), "nothing unsaved");
        let id = row_ids(s.budget())[0];
        s.set_detail_field(id, "rate", "11").unwrap();
        let changed = s.safety.last_change.unwrap();
        assert!(!s.autosave_due(changed + secs(1)), "still typing");
        assert!(s.autosave_due(changed + ms(safety::AUTOSAVE_QUIET)));

        // Busy: a change every two seconds still autosaves after the cap.
        let start = s.safety.pending_since.unwrap();
        let mut t = start;
        while t < start + ms(safety::AUTOSAVE_MAX_WAIT) {
            t += secs(2);
            s.note_change(t);
            if t < start + ms(safety::AUTOSAVE_MAX_WAIT) {
                assert!(!s.autosave_due(t + secs(1)));
            }
        }
        assert!(s.autosave_due(t));

        // Written to a recovery copy: not due again until the next change.
        s.recovery_written();
        assert!(s.recovery_current());
        assert!(!s.autosave_due(t + secs(600)));
        s.set_detail_field(id, "rate", "12").unwrap();
        assert!(!s.recovery_current());
        assert!(s.autosave_due(crate::clock::mono_ms() + secs(600)));

        // Undo back to the saved state: nothing to autosave, and the copy
        // is stale; once removed, nothing is left to do.
        s.undo();
        s.undo();
        assert!(!s.is_dirty());
        assert!(!s.autosave_due(crate::clock::mono_ms() + secs(600)));
        assert!(s.recovery_stale());
        s.recovery_removed();
        assert!(!s.recovery_stale());

        // Without a file, autosave can't write into one.
        assert!(!s.can_autosave_to_file());
        assert!(s.recovery_key().starts_with("untitled-"));
    }

    #[test]
    fn view_revisions_follow_changes() {
        let mut s = Session::with_budget(small_sample(4), None);
        let r0 = s.view().rev;
        assert_eq!(s.view().rev, r0, "nothing changed");
        s.set_detail_field(row_ids(s.budget())[0], "rate", "5")
            .unwrap();
        let r1 = s.view().rev;
        assert_ne!(r1, r0);
        s.undo();
        let r2 = s.view().rev;
        assert!(r2 != r0 && r2 != r1, "undo is a change too");
    }

    /// A merging edit that joins its step after another step was made on
    /// top and undone drops that undone step, like any edit, even when it
    /// comes back to where its step began. Redo would otherwise replay it on
    /// a budget it wasn't made from.
    #[test]
    fn a_joined_step_drops_what_was_undone_since() {
        let mut s = Session::with_budget(small_sample(8), None);
        let start = s.budget().clone();
        let id = edit::details(&start)
            .find(|d| d.kind == RowKind::Line)
            .unwrap()
            .id;
        let text = |b: &Budget| {
            edit::details(b)
                .find(|d| d.id == id)
                .unwrap()
                .description
                .clone()
        };
        let before = text(&start);
        let press = |s: &mut Session, value: &str| {
            s.edit_merging("text", "Test edit", |b| {
                edit::set_detail_field(b, id, edit::Field::Description, value)?;
                Ok(((), "text".to_string()))
            })
            .unwrap();
        };
        press(&mut s, "Changed");
        s.edit("Test edit", |b| edit::set_title(b, "Another edit"))
            .unwrap();
        assert!(s.undo());
        // Within the window, back to where the merging step began.
        press(&mut s, &before);
        assert_eq!(*s.budget(), start);
        assert!(!s.is_dirty());
        assert!(!s.can_redo(), "the undone step went, as after any edit");
        assert!(!s.redo());
        assert_eq!(*s.budget(), start);

        // Joining with a change drops it too (as it always did).
        press(&mut s, "Changed");
        s.edit("Test edit", |b| edit::set_title(b, "Another edit"))
            .unwrap();
        assert!(s.undo());
        press(&mut s, "Changed again");
        assert!(!s.can_redo());
        assert_eq!(text(s.budget()), "Changed again");
        assert!(s.undo());
        assert_eq!(*s.budget(), start);
    }
}
