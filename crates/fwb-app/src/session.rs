//! One open budget: the authoritative [`Budget`], where it lives on disk, whether
//! it has unsaved changes, undo/redo history, and which account the UI shows.
//! No Tauri types here, so the whole edit -> recalc -> view path is testable
//! without a window.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use fwb_core::calc::cache::RecalcCache;
use fwb_core::{
    selection_totals, starter, Breakdown, Budget, Detail, RowId, RowTotals, SelectionTotals,
    TopsheetRow, Version,
};

use crate::diagnose::{lenient_recalc_with, Outcome};
use crate::edit::{self, EditResult};
use crate::report_dto::SelectionDto;
use crate::safety::{self, Access};
use crate::view::{Focus, FullView, View, ViewState};

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
    /// bring to join it, when it was made, and the rev it left.
    merge: Option<(String, Instant, u64)>,
}

/// What autosave needs to know about one open budget.
#[derive(Debug)]
struct SafetyState {
    /// Names this document's recovery copy (unique per open document).
    recovery_key: String,
    /// The latest change (edit, undo or redo).
    last_change: Option<Instant>,
    /// The first change not yet in the file or a recovery copy.
    pending_since: Option<Instant>,
    /// The rev in the recovery copy, if one was written and not removed.
    recovery_rev: Option<u64>,
    /// Recovered from a crash: the first save into the original file asks.
    confirm_overwrite: bool,
    /// The file whose previous version this session has backed up.
    backed_up: Option<PathBuf>,
    /// The file as we last read or wrote it, to notice changes made outside.
    disk: Option<safety::DiskStamp>,
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

    pub fn is_dirty(&self) -> bool {
        self.rev != self.saved_rev
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// The file name shown in the title bar.
    pub fn display_name(&self) -> String {
        self.path
            .as_deref()
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled.fwb".to_string())
    }

    /// What the budget's tab (and the Window menu) calls it: the file's
    /// name without `.fwb`, or the budget's title while it has no file.
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

    /// Reads a budget file and starts a fresh session on it.
    pub fn open(path: &Path) -> Result<Session, String> {
        let (budget, stamp) = read_budget(path)?;
        let mut s = Session::with_budget(budget, Some(path.to_path_buf()));
        s.safety.disk = Some(stamp);
        Ok(s)
    }

    /// Writes the budget to `path` (through a temporary file in the same folder,
    /// so a failed write never leaves half a file), then remembers the path and
    /// marks the budget saved.
    pub fn save_to(&mut self, path: &Path) -> Result<(), String> {
        let text = fwb_format::to_string(&self.budget);
        safety::write_atomically(path, text.as_bytes(), Access::Document)
            .map_err(|e| format!("Couldn't save {}: {e}", path.display()))?;
        self.path = Some(path.to_path_buf());
        self.saved_rev = self.rev;
        self.safety.pending_since = None;
        self.safety.confirm_overwrite = false;
        self.safety.disk = Some(safety::DiskStamp::of(path, text.as_bytes()));
        self.safety.conflict = false;
        Ok(())
    }

    // Changes made outside the app (a sync service, another computer).

    /// Whether the budget's file changed on disk since the app last read or
    /// wrote it. Missing (moved or deleted) is not a change: a save puts the
    /// file back. Unknown (no file, or not read by this session) is not
    /// either.
    pub fn changed_on_disk(&mut self) -> bool {
        let (Some(path), Some(stamp)) = (self.path.as_deref(), &self.safety.disk) else {
            return false;
        };
        match stamp.check(path) {
            safety::DiskCheck::Changed => true,
            safety::DiskCheck::Same(fresh) => {
                if let Some(fresh) = fresh {
                    self.safety.disk = Some(fresh);
                }
                false
            }
            safety::DiskCheck::Missing => false,
        }
    }

    /// The file changed outside the app and the user hasn't chosen yet:
    /// autosave into it is paused.
    pub fn in_conflict(&self) -> bool {
        self.safety.conflict
    }

    pub fn set_conflict(&mut self, on: bool) {
        self.safety.conflict = on;
    }

    /// Replaces the budget with the file's current content ("Load theirs"),
    /// as one edit so Undo brings this version back. Afterwards the budget
    /// matches the file: no unsaved changes, no conflict.
    pub fn load_from_disk(&mut self) -> Result<(), String> {
        let path = self.path.clone().ok_or("This budget has no file.")?;
        let (theirs, stamp) = read_budget(&path)?;
        self.edit(|b| {
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

    /// Notes a change at `at` (every edit, undo and redo does, at now).
    pub fn note_change(&mut self, at: Instant) {
        self.safety.last_change = Some(at);
        self.safety.pending_since.get_or_insert(at);
    }

    /// Whether autosave should run at `now`: there are unsaved changes not
    /// yet in a recovery copy, and the budget has been quiet long enough (or
    /// changing for too long).
    pub fn autosave_due(&self, now: Instant) -> bool {
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
        fwb_format::to_string(&self.budget)
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

    /// Whether the budget lives in the file at `path` (the same file by
    /// another spelling of its path counts too).
    pub fn is_file(&self, path: &Path) -> bool {
        self.path
            .as_deref()
            .is_some_and(|p| safety::same_file(p, path))
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
    /// and what changed goes on the undo stack; on error nothing changes. An
    /// edit that changes nothing leaves history and the dirty flag alone.
    pub fn edit<T>(&mut self, f: impl FnOnce(&mut Budget) -> EditResult<T>) -> EditResult<T> {
        let mut next = self.budget.clone();
        let out = f(&mut next)?;
        let old = std::mem::replace(&mut self.budget, next);
        if let Some(patch) = Patch::between(old, &mut self.budget) {
            self.undo.push(Snapshot {
                patch,
                rev: self.rev,
            });
            if self.undo.len() > UNDO_LIMIT {
                self.undo.remove(0);
            }
            self.redo.clear();
            self.rev = self.next_rev;
            self.next_rev += 1;
            self.changed();
        }
        Ok(out)
    }

    /// Like [`Session::edit`], but quick repeats join one undo step: when the
    /// previous step was made by this method less than [`MERGE_WINDOW`] ago,
    /// nothing has changed since (no other edit, undo or redo), and it handed
    /// out `key`, this edit joins that step. `f` returns its result and the
    /// key the next edit must bring to join this one. Moving rows uses it, so
    /// holding Option+Down is one undo step, not one per row passed. A run
    /// that ends where it started leaves no step at all.
    pub fn edit_merging<T>(
        &mut self,
        key: &str,
        f: impl FnOnce(&mut Budget) -> EditResult<(T, String)>,
    ) -> EditResult<T> {
        let now = Instant::now();
        let joins = matches!(&self.merge, Some((k, at, rev))
            if k == key && *rev == self.rev && now.duration_since(*at) < MERGE_WINDOW)
            && !self.undo.is_empty();
        if !joins {
            let before = self.rev;
            let (out, next_key) = self.edit(f)?;
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
        match Patch::between(start, &mut self.budget) {
            Some(patch) => {
                self.undo.push(Snapshot {
                    patch,
                    rev: top.rev,
                });
                self.redo.clear();
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
        self.note_change(Instant::now());
    }

    pub fn undo(&mut self) -> bool {
        let Some(s) = self.undo.pop() else {
            return false;
        };
        let back = s.patch.apply(&mut self.budget);
        self.redo.push(Snapshot {
            patch: back,
            rev: self.rev,
        });
        self.rev = s.rev;
        self.changed();
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(s) = self.redo.pop() else {
            return false;
        };
        let back = s.patch.apply(&mut self.budget);
        self.undo.push(Snapshot {
            patch: back,
            rev: self.rev,
        });
        self.rev = s.rev;
        self.changed();
        true
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

    /// The view, noting a row an edit just created so the UI can select it.
    pub fn view_with(&mut self, created_row: Option<RowId>) -> View {
        self.outcome();
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
        View::build(&mut self.budget, &st, focus, created_row)
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
        FullView::build(&mut self.budget, &st)
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
        let field = edit::Field::parse(field)?;
        self.edit(|b| edit::set_detail_field(b, id, field, value))
    }

    pub fn add_detail(
        &mut self,
        account: &str,
        after: Option<RowId>,
        kind: &str,
    ) -> EditResult<RowId> {
        let kind = edit::parse_kind(kind)?;
        self.edit(|b| edit::add_detail(b, account, after, kind))
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

/// Reads and parses a budget file, with the stamp of what was read.
pub(crate) fn read_budget(path: &Path) -> Result<(Budget, safety::DiskStamp), String> {
    let bytes =
        std::fs::read(path).map_err(|e| format!("Couldn't read {}: {e}", path.display()))?;
    let text = std::str::from_utf8(&bytes).map_err(|_| {
        format!(
            "Couldn't read {}: stream did not contain valid UTF-8",
            path.display()
        )
    })?;
    let budget = fwb_format::from_str(text)
        .map_err(|e| format!("{} isn't a readable budget: {e}", path.display()))?;
    Ok((budget, safety::DiskStamp::of(path, &bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use fwb_core::sample::{self, SampleSpec};
    use fwb_core::{Formula, Group, RowKind};

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
                s.edit(|b| {
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
        s.edit(|b| {
            b.globals.insert("X".into(), "1".into());
            Ok(())
        })
        .unwrap();
        assert!(matches!(
            &s.undo[1].patch.body,
            Body::Parts { shell: Some(_), rows } if rows.is_empty()
        ));
        // An edit that changes nothing adds no step.
        s.edit(|_| Ok(())).unwrap();
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
        use std::time::Duration;
        let secs = |n| Duration::from_secs(n);
        let mut s = Session::with_budget(small_sample(6), None);
        let t0 = Instant::now();
        assert!(!s.autosave_due(t0 + secs(600)), "nothing unsaved");
        let id = row_ids(s.budget())[0];
        s.set_detail_field(id, "rate", "11").unwrap();
        let changed = s.safety.last_change.unwrap();
        assert!(!s.autosave_due(changed + secs(1)), "still typing");
        assert!(s.autosave_due(changed + safety::AUTOSAVE_QUIET));

        // Busy: a change every two seconds still autosaves after the cap.
        let start = s.safety.pending_since.unwrap();
        let mut t = start;
        while t < start + safety::AUTOSAVE_MAX_WAIT {
            t += secs(2);
            s.note_change(t);
            if t < start + safety::AUTOSAVE_MAX_WAIT {
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
        assert!(s.autosave_due(Instant::now() + secs(600)));

        // Undo back to the saved state: nothing to autosave, and the copy
        // is stale; once removed, nothing is left to do.
        s.undo();
        s.undo();
        assert!(!s.is_dirty());
        assert!(!s.autosave_due(Instant::now() + secs(600)));
        assert!(s.recovery_stale());
        s.recovery_removed();
        assert!(!s.recovery_stale());

        // Without a file, autosave can't write into one.
        assert!(!s.can_autosave_to_file());
        assert!(s.recovery_key().starts_with("untitled-"));
    }

    #[test]
    fn a_recovered_budget_is_unsaved_and_asks_before_replacing_its_file() {
        let dir = safety::tests::temp_dir("session-recovered");
        let path = dir.join("film.fwb");
        let mut s = Session::recovered(small_sample(7), Some(path.clone()), "abc-1".into());
        assert!(s.is_dirty());
        assert!(!s.can_undo());
        assert_eq!(s.recovery_key(), "abc-1");
        assert!(s.recovery_current(), "its copy already holds it");
        assert!(!s.autosave_due(Instant::now() + std::time::Duration::from_secs(600)));
        assert!(s.needs_overwrite_confirm() && !s.can_autosave_to_file());
        assert!(!s.has_backed_up(&path));
        s.save_to(&path).unwrap();
        assert!(!s.is_dirty() && s.recovery_stale());
        assert!(!s.needs_overwrite_confirm() && s.can_autosave_to_file());
        // Never saved: nothing to confirm, still unsaved.
        let u = Session::recovered(small_sample(7), None, "abc-2".into());
        assert!(u.is_dirty() && !u.needs_overwrite_confirm());
        std::fs::remove_dir_all(&dir).unwrap();
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
}
