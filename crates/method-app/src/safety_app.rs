//! The Tauri side of saving safety (src/safety.rs): the autosave timer, the
//! Autosave switch, crash recovery, backups, File > Open Recent, and opening
//! files the system hands the app (double-click, Open With, a path on the
//! command line). Behavior: docs/APP.md, "Saving and recovery".
//!
//! Paths still never come from the web view: they come from the native
//! dialogs, the operating system (open events, the command line), the recent
//! list and the recovery folder. The web view names a recent file by its
//! place in the list, a recovery copy by its key and a backup by its file
//! name, and each is checked against what is actually there.
//!
//! Every open budget (each tab, src/docs.rs) has its own autosave timing,
//! recovery copy, backups and file stamp; autosave passes over all of them.
//! The files open as tabs are remembered for the next start (`tabs.json`).
//!
//! Locks: `Safety::writer` is always taken before the budgets' lock, never
//! while holding it, so autosave, saves and opening files can't deadlock.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::menu::{CheckMenuItem, IsMenuItem, MenuItem, MenuItemKind, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager, Runtime, State, WebviewWindow};
use tauri_plugin_dialog::{
    DialogExt, MessageDialogButtons, MessageDialogKind, MessageDialogResult,
};

use crate::app::{finish, window_title, AppState, DocId};
use crate::docs::{Docs, TabInfo};
use crate::safety::{
    BackupEntry, Backups, OpenTabs, RecentFiles, RecoveryEntry, RecoveryStore, Settings, BACKUP_GAP,
};
use crate::session::Session;
use crate::view::{FileDto, View};

type CmdResult<T = View> = Result<T, String>;

/// File > Open Recent (a submenu filled from the recent list).
pub const OPEN_RECENT: &str = "file.open_recent";
/// File > Open Recent > one file: `file.recent.<n>`, answered in the web view
/// (which asks about unsaved changes, then runs `open_recent`).
pub const RECENT_PREFIX: &str = "file.recent.";
/// File > Open Recent > Clear Menu, answered here.
pub const CLEAR_RECENT: &str = "file.recent.clear";
/// File > Autosave (a check item), answered here.
pub const AUTOSAVE_ITEM: &str = "file.autosave";
/// File > Revert to Backup…, answered in the web view (a dialog).
pub const REVERT_BACKUP: &str = "file.revert_backup";

/// Events to the web view.
/// A file to open arrived from the system; the payload is its name.
pub const OPEN_REQUEST: &str = "open-request";
/// A budget was saved into its file without a command's view (autosave,
/// Save All on the way out): `{ doc, rev, file, tabs }` (the view's `doc`,
/// `file` and `tabs`).
pub const FILE_STATE: &str = "file-state";
/// The Autosave switch changed (from the menu); the payload is on or off.
pub const AUTOSAVE_CHANGED: &str = "autosave-changed";
/// Autosave couldn't write into the file; the payload is the message.
pub const AUTOSAVE_ERROR: &str = "autosave-error";
/// A budget's file changed outside the app; autosave into it is paused
/// until the user chooses (`resolve_external_change`). The payload is
/// `{ doc, name }`: which budget, and its file's name.
pub const EXTERNAL_CHANGE: &str = "external-change";

/// A file the system asked to open before the app finished starting (macOS
/// can send it before setup runs); `init` picks it up.
static EARLY_OPEN: Mutex<Option<PathBuf>> = Mutex::new(None);

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Recovery copies, backups, settings and the recent list, all in the app's
/// local data folder.
pub struct Safety {
    store: Option<RecoveryStore>,
    backups: Option<Backups>,
    settings_file: Option<PathBuf>,
    settings: Mutex<Settings>,
    recent: Mutex<RecentFiles>,
    /// The files open as tabs, for the next start.
    open_tabs: Mutex<OpenTabs>,
    /// Held while writing or removing recovery copies and saving.
    writer: Mutex<()>,
    /// A file the system asked us to open, waiting for the web view.
    pending_open: Mutex<Option<PathBuf>>,
    /// The user chose to throw every open budget's changes away (closing or
    /// quitting): don't keep recovery copies of them on the way out.
    discarded: AtomicBool,
    /// The last autosave problem reported, so it is said once.
    last_error: Mutex<Option<String>>,
}

impl Safety {
    /// Everything under `dir` (the app's local data folder). Without one,
    /// nothing is kept between runs, and autosave into files still works.
    pub fn new(dir: Option<&Path>) -> Safety {
        let store = dir.and_then(|d| RecoveryStore::open(&d.join("Recovery")).ok());
        if let Some(s) = &store {
            s.tidy();
        }
        let settings_file = dir.map(|d| d.join("settings.json"));
        let settings = settings_file
            .as_deref()
            .map(Settings::load)
            .unwrap_or_default();
        Safety {
            store,
            backups: dir.map(|d| Backups::new(&d.join("Backups"))),
            settings_file,
            settings: Mutex::new(settings),
            recent: Mutex::new(RecentFiles::load(dir.map(|d| d.join("recent.json")))),
            open_tabs: Mutex::new(OpenTabs::load(dir.map(|d| d.join("tabs.json")))),
            writer: Mutex::new(()),
            pending_open: Mutex::new(None),
            discarded: AtomicBool::new(false),
            last_error: Mutex::new(None),
        }
    }

    pub fn autosave_on(&self) -> bool {
        lock(&self.settings).autosave
    }

    pub fn set_autosave(&self, on: bool) {
        let mut s = lock(&self.settings);
        s.autosave = on;
        if let Some(f) = &self.settings_file {
            let _ = s.save(f);
        }
    }

    fn remove_recovery(&self, key: &str) {
        if let Some(store) = &self.store {
            let _ = store.remove(key);
        }
    }

    /// Backs up what `path` holds now before it is replaced: always for an
    /// explicit save and for the first write in a session, else at most every
    /// [`BACKUP_GAP`]. Best effort: a backup that fails never stops a save.
    fn back_up(&self, s: &mut Session, path: &Path, explicit: bool) {
        let Some(b) = &self.backups else {
            return;
        };
        let gap = (!explicit && s.has_backed_up(path)).then_some(BACKUP_GAP);
        if b.back_up(path, gap).is_ok() {
            s.backed_up(path);
        }
    }

    pub fn recent_paths(&self) -> Vec<PathBuf> {
        lock(&self.recent).paths().to_vec()
    }

    /// Remembers the files open as tabs (and which one is in front) for the
    /// next start. Best effort.
    pub fn remember_tabs(&self, docs: &Docs) {
        let (files, active) = docs.files();
        let split = docs.saved_split();
        let _ = lock(&self.open_tabs).remember_layout(&files, active, split);
    }
}

// ---------------------------------------------------------------------------
// The core, without Tauri (tested below)

/// What one autosave pass did.
#[derive(Debug, PartialEq, Eq)]
pub enum Autosaved {
    Nothing,
    /// Saved into the budget's file; it has no unsaved changes now.
    File,
    /// Wrote the recovery copy (no file, autosave off, or a recovered budget
    /// not yet saved into its file).
    Recovery,
    /// Removed a recovery copy that nothing needs any more.
    Removed,
    /// Couldn't save into the file (a recovery copy was kept instead).
    Failed(String),
    /// The file changed outside the app since we last read or wrote it, so
    /// it was left alone and autosave into it is paused (a recovery copy was
    /// kept instead).
    Conflict,
}

/// One autosave pass over every open budget at `now`, in tab order.
/// `force` skips the waiting (the window lost focus, the app is closing).
pub fn autosave_pass(
    state: &AppState,
    safety: &Safety,
    now: Instant,
    force: bool,
) -> Vec<(DocId, Autosaved)> {
    let ids = state.docs().ids();
    ids.into_iter()
        .map(|id| (id, autosave_doc(state, safety, Some(id), now, force)))
        .collect()
}

/// One autosave pass over budget `doc` (`None`: the one in front) at `now`.
/// `force` skips the waiting (closing its tab, before leaving the app).
pub fn autosave_doc(
    state: &AppState,
    safety: &Safety,
    doc: Option<DocId>,
    now: Instant,
    force: bool,
) -> Autosaved {
    let _w = lock(&safety.writer);
    if safety.discarded.load(Ordering::SeqCst) {
        // Closing or quitting without saving: write nothing more.
        return Autosaved::Nothing;
    }
    let Ok(mut s) = state.lock_doc(doc) else {
        return Autosaved::Nothing; // closed meanwhile
    };
    if s.recovery_stale() {
        safety.remove_recovery(s.recovery_key());
        s.recovery_removed();
        return Autosaved::Removed;
    }
    if !s.is_dirty() || !(force || s.autosave_due(now)) {
        return Autosaved::Nothing;
    }
    let mut failed = None;
    let mut conflict = false;
    if safety.autosave_on() && s.can_autosave_to_file() && !s.in_conflict() {
        if s.changed_on_disk() {
            // Never write over someone else's changes: pause and ask.
            s.set_conflict(true);
            conflict = true;
        } else {
            let path = s.path().expect("can_autosave_to_file").to_path_buf();
            safety.back_up(&mut s, &path, false);
            match s.save_to(&path) {
                Ok(()) => {
                    if s.recovery_stale() {
                        safety.remove_recovery(s.recovery_key());
                        s.recovery_removed();
                    }
                    return Autosaved::File;
                }
                Err(e) => failed = Some(e),
            }
        }
    }
    let wrote = !s.recovery_current();
    if wrote {
        if let Some(store) = &safety.store {
            let title = s.budget().title.clone();
            if store
                .write(s.recovery_key(), &s.file_text(), s.path(), &title)
                .is_ok()
            {
                s.recovery_written();
            }
        }
    }
    match (conflict, failed) {
        (true, _) => Autosaved::Conflict,
        (false, Some(e)) => Autosaved::Failed(e),
        (false, None) if wrote => Autosaved::Recovery,
        _ => Autosaved::Nothing,
    }
}

/// An explicit Save (or Save As) of budget `doc` (`None`: the one in front)
/// to `path`: back up what is there, write the budget atomically, and drop
/// the recovery copy. The budget is unchanged on failure. False, with nothing
/// written, when `path` is the budget's own file and it changed outside the
/// app (the user chooses what to do). Refused, with nothing written, when
/// `path` is the file of another open budget: one writer per file, as for
/// exports, templates and copies.
pub fn save(
    state: &AppState,
    safety: &Safety,
    doc: Option<DocId>,
    path: &Path,
) -> Result<bool, String> {
    let _w = lock(&safety.writer);
    let mut s = state.lock_doc(doc)?;
    let me = s.id();
    let docs = s.docs();
    if docs
        .ids()
        .into_iter()
        .any(|id| id != me && docs.get(id).is_some_and(|o| o.is_file(path)))
    {
        return Err(
            "That file is open in another tab. Close that tab first, or save under another name."
                .into(),
        );
    }
    if s.path() == Some(path) && (s.in_conflict() || s.changed_on_disk()) {
        s.set_conflict(true);
        return Ok(false);
    }
    safety.back_up(&mut s, path, true);
    s.save_to(path)?;
    if s.recovery_stale() {
        safety.remove_recovery(s.recovery_key());
        s.recovery_removed();
    }
    drop(s);
    let _ = lock(&safety.recent).add(path);
    Ok(true)
}

/// What to do about a file changed outside the app.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resolve {
    /// Save this budget over the file, after backing up the file's version.
    KeepMine,
    /// Load the file's version, after backing up this budget's.
    LoadTheirs,
}

/// Settles a file changed outside the app, for budget `doc` (`None`: the
/// one in front). The other version is always backed up first; if that
/// fails, nothing changes. ("Save mine as a copy" is Save As, which leaves
/// the file alone.)
pub fn resolve(
    state: &AppState,
    safety: &Safety,
    doc: Option<DocId>,
    choice: Resolve,
) -> Result<(), String> {
    let _w = lock(&safety.writer);
    let mut s = state.lock_doc(doc)?;
    let path = s.path().ok_or("This budget has no file.")?.to_path_buf();
    let backups = safety
        .backups
        .as_ref()
        .ok_or("There is no place to keep a backup, so nothing was changed.")?;
    let backup_failed =
        |e: std::io::Error| format!("Couldn't keep a backup ({e}), so nothing was changed.");
    match choice {
        // A read-only shared copy has nothing of its own to keep: saving it
        // would only write the stale copy over the newer file.
        Resolve::KeepMine if s.is_read_only() => return Err(crate::sharing::READ_ONLY.into()),
        Resolve::KeepMine => {
            backups.back_up(&path, None).map_err(backup_failed)?;
            s.backed_up(&path);
            s.save_to(&path)?;
        }
        Resolve::LoadTheirs => {
            backups
                .add(&path, s.file_text().as_bytes())
                .map_err(backup_failed)?;
            s.load_from_disk()?;
        }
    }
    if s.recovery_stale() {
        safety.remove_recovery(s.recovery_key());
        s.recovery_removed();
    }
    Ok(())
}

/// A new tab with a fresh starter budget, in front.
pub fn new_tab(state: &AppState) -> DocId {
    state.docs().add(Session::new())
}

/// Opens `next` as a budget in front: in the place of tab `instead` when
/// given (that tab's recovery copy goes: it had nothing worth keeping), else
/// in a new tab.
fn put_in_front(
    state: &AppState,
    safety: &Safety,
    next: Session,
    instead: Option<DocId>,
) -> Result<DocId, String> {
    let _w = lock(&safety.writer);
    let mut docs = state.docs();
    Ok(match instead {
        Some(old) => {
            let (id, gone) = docs.replace(old, next)?;
            safety.remove_recovery(gone.recovery_key());
            id
        }
        None => docs.add(next),
    })
}

/// Opens the budget at `path` in a new tab, in front, and lists it in Open
/// Recent. A file that is open already just has its tab brought to the
/// front; an untouched Untitled budget alone in the window gives its tab up
/// to the file. A file that can't be read leaves everything as it was.
pub fn open_path(state: &AppState, safety: &Safety, path: &Path) -> Result<DocId, String> {
    let open = state.docs().find_file(path);
    let id = match open {
        Some(id) => {
            state.docs().switch(id)?;
            id
        }
        None => {
            let opened = Session::open(path)?;
            let instead = state.docs().lone_untouched();
            put_in_front(state, safety, opened, instead)?
        }
    };
    let _ = lock(&safety.recent).add(path);
    Ok(id)
}

/// Opens recovery copy `key` as an unsaved budget pointing at its original
/// file, in front, and makes the copy this run's own (it stays until saved
/// or discarded). It takes the place of its own file's tab when that tab has
/// nothing of its own yet (reopened at the start, untouched), else of an
/// untouched Untitled budget alone in the window, else it gets a new tab.
pub fn open_recovery(state: &AppState, safety: &Safety, key: &str) -> Result<DocId, String> {
    let store = safety
        .store
        .as_ref()
        .ok_or("There are no recovered budgets.")?;
    let (text, meta) = store.read(key).map_err(|e| e.to_string())?;
    let budget = method_format::from_str(&text)
        .map_err(|e| format!("The recovered budget can't be read: {e}"))?;
    let path = meta.path.map(PathBuf::from);
    let instead = {
        let docs = state.docs();
        path.as_deref()
            .and_then(|p| docs.find_file(p))
            .filter(|id| docs.get(*id).is_some_and(Session::is_untouched))
            .or_else(|| docs.lone_untouched())
    };
    let session = Session::recovered(budget, path, meta.key.clone());
    {
        let _w = lock(&safety.writer);
        let _ = store.adopt(key);
    }
    put_in_front(state, safety, session, instead)
}

/// What closing a tab came to.
#[derive(Debug, PartialEq, Eq)]
pub enum Closed {
    /// Closed (its recovery copy, if any, went with it).
    Closed,
    /// It has unsaved changes: ask, then close it with `discard` (or save).
    Unsaved,
    /// The only tab: close the window instead.
    Last,
}

/// Closes budget `id`'s tab. Unsaved changes stop it unless `discard` (the
/// user chose to throw them away; with Autosave on the caller autosaves
/// first, so only what can't be autosaved is left).
pub fn close_doc(
    state: &AppState,
    safety: &Safety,
    id: DocId,
    discard: bool,
) -> Result<Closed, String> {
    let _w = lock(&safety.writer);
    let mut docs = state.docs();
    let s = docs.get(id).ok_or("That budget has been closed.")?;
    if docs.len() == 1 {
        return Ok(Closed::Last);
    }
    if s.is_dirty() && !discard {
        return Ok(Closed::Unsaved);
    }
    let gone = docs.remove(id)?;
    // Saved, or thrown away on purpose: no recovery copy either way.
    safety.remove_recovery(gone.recovery_key());
    Ok(Closed::Closed)
}

/// Reopens the files that were open as tabs when the app last ran, in their
/// order, with the one in front again in front. Files that are gone or can't
/// be read are left out. Done before the page asks for its first view, in
/// place of the starting Untitled budget; with nothing to reopen, that stays.
pub fn restore_tabs(state: &AppState, safety: &Safety) -> usize {
    let (files, active) = lock(&safety.open_tabs).files();
    let split = lock(&safety.open_tabs).split();
    let mut opened: Vec<(usize, Session)> = Vec::new();
    for (i, p) in files.iter().enumerate() {
        if let Ok(s) = Session::open(p) {
            opened.push((i, s));
        }
    }
    let n = opened.len();
    let front = active.and_then(|a| opened.iter().position(|(i, _)| *i == a));
    let _w = lock(&safety.writer);
    let mut docs = state.docs();
    let mut instead = docs.lone_untouched();
    let mut ids = Vec::with_capacity(n);
    // The document each saved file opened as, by its place in the list.
    let mut by_place: Vec<Option<DocId>> = vec![None; files.len()];
    for (i, s) in opened {
        let id = match instead.take() {
            Some(old) => match docs.replace(old, s) {
                Ok((id, gone)) => {
                    safety.remove_recovery(gone.recovery_key());
                    id
                }
                Err(_) => continue,
            },
            None => docs.add(s),
        };
        ids.push(id);
        if let Some(slot) = by_place.get_mut(i) {
            *slot = Some(id);
        }
    }
    if let Some(&id) = ids.get(front.unwrap_or(0)) {
        let _ = docs.switch(id);
    }
    // Split view as it was left (the target pane's budget in front).
    if let Some(sp) = &split {
        docs.restore_split(sp, &by_place);
    }
    ids.len()
}

/// Replaces budget `doc`'s content (`None`: the one in front) with backup
/// `id` of its file, as one edit (undo brings the content back; autosave or
/// Save writes it).
pub fn restore(
    state: &AppState,
    safety: &Safety,
    doc: Option<DocId>,
    id: &str,
) -> Result<(), String> {
    let mut s = state.lock_doc(doc)?;
    let path = s
        .path()
        .ok_or("This budget has no backups yet.")?
        .to_path_buf();
    let backups = safety
        .backups
        .as_ref()
        .ok_or("This budget has no backups yet.")?;
    let text = backups.read(&path, id).map_err(|e| e.to_string())?;
    let budget =
        method_format::from_str(&text).map_err(|e| format!("That backup can't be read: {e}"))?;
    s.edit("Revert to a backup", |b| {
        *b = budget;
        Ok(())
    })
}

/// The first command-line argument that names an existing file (Windows and
/// Linux pass a double-clicked file this way; options are skipped).
pub fn path_from_args(args: impl IntoIterator<Item = std::ffi::OsString>) -> Option<PathBuf> {
    args.into_iter()
        .filter(|a| !a.to_string_lossy().starts_with('-'))
        .map(PathBuf::from)
        .find(|p| p.is_file())
}

// ---------------------------------------------------------------------------
// Tauri wiring

/// Sets everything up when the app starts: the data folder, the budgets
/// that were open as tabs last time, the menu's recent list and Autosave
/// check, a file named on the command line, and the autosave timer.
pub fn init<R: Runtime>(app: &AppHandle<R>) {
    let dir = app.path().app_local_data_dir().ok();
    if let Some(d) = &dir {
        let _ = crate::safety::create_private_dir(d);
    }
    app.manage(Safety::new(dir.as_deref()));
    // The budgets that were open as tabs last time, before the page asks.
    restore_tabs(&app.state::<AppState>(), &app.state::<Safety>());
    refresh_menu(app);
    let early = lock(&EARLY_OPEN).take();
    if let Some(p) = early.or_else(|| path_from_args(std::env::args_os().skip(1))) {
        *lock(&app.state::<Safety>().pending_open) = Some(p);
    }
    let handle = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(1));
        autosave_now(&handle, false);
    });
}

/// Runs an autosave pass over every open budget and tells the window what
/// happened.
pub fn autosave_now<R: Runtime>(app: &AppHandle<R>, force: bool) {
    let state = app.state::<AppState>();
    let safety = app.state::<Safety>();
    for (id, done) in autosave_pass(&state, &safety, Instant::now(), force) {
        tell_autosaved(app, id, done);
    }
}

/// Autosaves budget `id` now, if anything is unsaved (before its tab
/// closes), and tells the window what happened.
pub fn autosave_one<R: Runtime>(app: &AppHandle<R>, id: DocId) {
    let done = autosave_doc(
        &app.state::<AppState>(),
        &app.state::<Safety>(),
        Some(id),
        Instant::now(),
        true,
    );
    tell_autosaved(app, id, done);
}

/// Tells the page what an autosave of budget `id` did (also before Restart
/// to update, src/updater.rs).
pub(crate) fn tell_autosaved<R: Runtime>(app: &AppHandle<R>, id: DocId, done: Autosaved) {
    let safety = app.state::<Safety>();
    match done {
        Autosaved::File => {
            *lock(&safety.last_error) = None;
            tell_saved(app, id);
        }
        Autosaved::Conflict => {
            let name = app
                .state::<AppState>()
                .docs()
                .get(id)
                .map(Session::display_name);
            if let Some(name) = name {
                let _ = app.emit_to("main", EXTERNAL_CHANGE, ExternalChange { doc: id, name });
            }
        }
        Autosaved::Failed(msg) => {
            let mut last = lock(&safety.last_error);
            if last.as_deref() != Some(msg.as_str()) {
                let _ = app.emit_to(
                    "main",
                    AUTOSAVE_ERROR,
                    format!("Autosave couldn't save: {msg}. A recovery copy was kept."),
                );
                *last = Some(msg);
            }
        }
        _ => {}
    }
}

/// Budget `id` was saved into its file outside a command (autosave, Save All
/// on the way out): the window's title (when it is in front) and the page's
/// tab and file state follow.
pub fn tell_saved<R: Runtime>(app: &AppHandle<R>, id: DocId) {
    let state = app.state::<AppState>();
    let Ok(s) = state.lock_doc(Some(id)) else {
        return;
    };
    if s.is_active() {
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.set_title(&window_title(&s));
        }
    }
    let _ = app.emit_to("main", FILE_STATE, FileState::of(&s, id, s.docs().tabs()));
}

/// The window has focus again: if a budget's file changed outside the app
/// meanwhile, ask now rather than at the next save. (Once the user chose
/// Decide Later, the question comes back with the next Save, not on every
/// return to the window.)
pub fn check_external<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<AppState>();
    let changed: Vec<ExternalChange> = {
        let mut docs = state.docs();
        let mut out = Vec::new();
        for id in docs.ids() {
            let Some(s) = docs.get_mut(id) else { continue };
            if s.in_conflict() || !s.changed_on_disk() {
                continue;
            }
            s.set_conflict(true);
            out.push(ExternalChange {
                doc: id,
                name: s.display_name(),
            });
        }
        out
    };
    for c in changed {
        let _ = app.emit_to("main", EXTERNAL_CHANGE, c);
    }
}

/// An explicit Save of budget `id` from the Save command: when the file
/// changed outside the app, nothing is written, the page asks what to do,
/// and this is false.
pub fn save_from<R: Runtime>(
    window: &WebviewWindow<R>,
    state: &AppState,
    id: DocId,
    path: &Path,
) -> Result<bool, String> {
    let saved = save(state, &window.state::<Safety>(), Some(id), path)?;
    if !saved {
        let name = state.lock_doc(Some(id))?.display_name();
        let _ = window.emit_to("main", EXTERNAL_CHANGE, ExternalChange { doc: id, name });
    }
    Ok(saved)
}

/// Before closing or quitting: with Autosave on, every budget's unsaved
/// changes go into its file. The budgets still unsaved afterwards (untitled
/// ones, Autosave off, a file changed outside the app), in tab order.
pub fn save_before_leaving<R: Runtime>(app: &AppHandle<R>) -> Vec<DocId> {
    autosave_now(app, true);
    app.state::<AppState>().docs().dirty()
}

/// The user chose to close or quit without saving: drop every recovery copy.
pub fn discarded<R: Runtime>(app: &AppHandle<R>) {
    discard_all(&app.state::<AppState>(), &app.state::<Safety>());
}

/// The exit a Discard All was chosen for didn't happen (Restart to update
/// couldn't install, src/updater.rs): the budgets are still open, so
/// autosave and recovery copies take up again.
pub fn discard_cancelled(safety: &Safety) {
    safety.discarded.store(false, Ordering::SeqCst);
}

/// [`discarded`], without Tauri: nothing more is autosaved or kept as a
/// recovery copy, and every recovery copy goes.
pub fn discard_all(state: &AppState, safety: &Safety) {
    safety.discarded.store(true, Ordering::SeqCst);
    let _w = lock(&safety.writer);
    let mut docs = state.docs();
    for id in docs.ids() {
        if let Some(s) = docs.get_mut(id) {
            safety.remove_recovery(s.recovery_key());
            s.recovery_removed();
        }
    }
}

/// The app is exiting. Changes the user didn't throw away are kept (into
/// the file with Autosave on, else a recovery copy): an exit the guard
/// didn't see (logging out, the system quitting the app) loses nothing. A
/// budget with nothing unsaved leaves no recovery copy.
pub fn on_exit<R: Runtime>(app: &AppHandle<R>) {
    let Some(safety) = app.try_state::<Safety>() else {
        return;
    };
    if safety.discarded.load(Ordering::SeqCst) {
        return;
    }
    autosave_now(app, true);
    let state = app.state::<AppState>();
    let _w = lock(&safety.writer);
    let mut docs = state.docs();
    for id in docs.ids() {
        if let Some(s) = docs.get_mut(id).filter(|s| !s.is_dirty()) {
            safety.remove_recovery(s.recovery_key());
            s.recovery_removed();
        }
    }
}

/// The system asked to open `paths` (macOS: double-click, Open With, a drop
/// on the Dock icon). The first one is opened in a tab (the web view asks
/// for it with `open_pending`, after finishing any edit).
pub fn on_opened<R: Runtime>(app: &AppHandle<R>, paths: Vec<PathBuf>) {
    let Some(path) = paths.into_iter().find(|p| p.is_file()) else {
        return;
    };
    let Some(safety) = app.try_state::<Safety>() else {
        *lock(&EARLY_OPEN) = Some(path);
        return;
    };
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    *lock(&safety.pending_open) = Some(path);
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_focus();
    }
    // If the page isn't listening yet, it asks for the pending file itself
    // (open_pending) when it starts.
    let _ = app.emit_to("main", OPEN_REQUEST, name);
}

/// What the web view hears after a save it didn't ask for (autosave, Save
/// All on the way out).
#[derive(Serialize, Clone)]
struct FileState {
    doc: DocId,
    rev: u64,
    file: FileDto,
    tabs: Vec<TabInfo>,
}

impl FileState {
    fn of(s: &Session, doc: DocId, tabs: Vec<TabInfo>) -> FileState {
        FileState {
            doc,
            rev: s.view_rev(),
            file: s.file_info(),
            tabs,
        }
    }
}

/// What the web view hears when a budget's file changed outside the app.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
struct ExternalChange {
    doc: DocId,
    name: String,
}

// ---------------------------------------------------------------------------
// Menu

/// The menu item `id` among `items` and their submenus (also the Help
/// menu's update switch, src/updater.rs).
pub(crate) fn find<R: Runtime>(items: Vec<MenuItemKind<R>>, id: &str) -> Option<MenuItemKind<R>> {
    for item in items {
        if item.id() == id {
            return Some(item);
        }
        if let MenuItemKind::Submenu(sub) = &item {
            if let Some(found) = find(sub.items().unwrap_or_default(), id) {
                return Some(found);
            }
        }
    }
    None
}

fn insert_after<R: Runtime>(
    menu: &Submenu<R>,
    after: &str,
    items: &[&dyn IsMenuItem<R>],
) -> tauri::Result<()> {
    let at = menu
        .items()?
        .iter()
        .position(|i| i.id() == after)
        .map_or(menu.items()?.len(), |p| p + 1);
    menu.insert_items(items, at)
}

/// Adds Open Recent (after Open…), and Autosave and Revert to Backup…
/// (after Save As…) to the File menu. Called by `menu::build`; the recent
/// list is filled in by [`refresh_menu`].
pub fn extend_file_menu<R: Runtime>(app: &AppHandle<R>, file: &Submenu<R>) -> tauri::Result<()> {
    let recent = Submenu::with_id(app, OPEN_RECENT, "Open Recent", true)?;
    insert_after(file, "file.open", &[&recent])?;
    let on = app.try_state::<Safety>().is_none_or(|s| s.autosave_on());
    let autosave = CheckMenuItem::with_id(app, AUTOSAVE_ITEM, "Autosave", true, on, None::<&str>)?;
    let revert = MenuItem::with_id(app, REVERT_BACKUP, "Revert to Backup…", true, None::<&str>)?;
    insert_after(file, "file.save_as", &[&autosave, &revert])
}

/// Brings Open Recent and the Autosave check up to date.
pub fn refresh_menu<R: Runtime>(app: &AppHandle<R>) {
    let (Some(menu), Some(safety)) = (app.menu(), app.try_state::<Safety>()) else {
        return;
    };
    let items = menu.items().unwrap_or_default();
    if let Some(MenuItemKind::Check(c)) = find(items.clone(), AUTOSAVE_ITEM) {
        let _ = c.set_checked(safety.autosave_on());
    }
    let Some(MenuItemKind::Submenu(recent)) = find(items, OPEN_RECENT) else {
        return;
    };
    let labels = {
        let mut r = lock(&safety.recent);
        r.prune();
        r.labels()
    };
    let _ = (|| -> tauri::Result<()> {
        while recent.remove_at(0)?.is_some() {}
        for (i, label) in labels.iter().enumerate() {
            let id = format!("{RECENT_PREFIX}{i}");
            recent.append(&MenuItem::with_id(app, id, label, true, None::<&str>)?)?;
        }
        if !labels.is_empty() {
            recent.append(&PredefinedMenuItem::separator(app)?)?;
        }
        recent.append(&MenuItem::with_id(
            app,
            CLEAR_RECENT,
            "Clear Menu",
            !labels.is_empty(),
            None::<&str>,
        )?)?;
        Ok(())
    })();
}

fn set_autosave_everywhere<R: Runtime>(app: &AppHandle<R>, on: bool) {
    app.state::<Safety>().set_autosave(on);
    refresh_menu(app);
    let _ = app.emit_to("main", AUTOSAVE_CHANGED, on);
}

/// Menu items answered here: Clear Menu and the Autosave check. False for
/// anything else (the caller passes it on).
pub fn handle_menu<R: Runtime>(app: &AppHandle<R>, id: &str) -> bool {
    match id {
        CLEAR_RECENT => {
            let _ = lock(&app.state::<Safety>().recent).clear();
            refresh_menu(app);
            true
        }
        AUTOSAVE_ITEM => {
            let on = !app.state::<Safety>().autosave_on();
            set_autosave_everywhere(app, on);
            true
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Commands

/// Whether Autosave is on.
#[tauri::command]
pub fn autosave_setting(safety: State<'_, Safety>) -> bool {
    safety.autosave_on()
}

/// Turns Autosave on or off (the toolbar switch); returns the new setting.
#[tauri::command]
pub fn set_autosave<R: Runtime>(app: AppHandle<R>, on: bool) -> bool {
    set_autosave_everywhere(&app, on);
    on
}

/// Autosaves budget `doc` (`None`: the one in front) now, if anything is
/// unsaved, and gives its view.
#[tauri::command]
pub fn flush_autosave<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
) -> CmdResult {
    let id = state.lock_doc(doc)?.id();
    autosave_one(window.app_handle(), id);
    Ok(finish(&window, &mut state.lock_doc(Some(id))?, None))
}

/// Opens the file the system asked to open, if there is one, in a tab (its
/// own tab, if it is open already).
#[tauri::command]
pub fn open_pending<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    safety: State<'_, Safety>,
) -> CmdResult<Option<View>> {
    let Some(path) = lock(&safety.pending_open).take() else {
        return Ok(None);
    };
    let id = open_path(&state, &safety, &path)?;
    refresh_menu(window.app_handle());
    Ok(Some(finish(&window, &mut state.lock_doc(Some(id))?, None)))
}

/// Opens File > Open Recent's `index`-th file in a tab (its own tab, if it
/// is open already).
#[tauri::command]
pub fn open_recent<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    safety: State<'_, Safety>,
    index: usize,
) -> CmdResult<Option<View>> {
    let Some(path) = safety.recent_paths().get(index).cloned() else {
        return Ok(None);
    };
    let opened = open_path(&state, &safety, &path);
    // A file that has gone drops out of the menu either way.
    refresh_menu(window.app_handle());
    let id = opened?;
    Ok(Some(finish(&window, &mut state.lock_doc(Some(id))?, None)))
}

/// Recovery copies left by a run of the app that didn't finish (a crash, a
/// forced quit, a power cut), newest first.
#[tauri::command]
pub fn recovery_list(safety: State<'_, Safety>) -> Vec<RecoveryEntry> {
    safety
        .store
        .as_ref()
        .map(RecoveryStore::list)
        .unwrap_or_default()
}

/// Opens recovery copy `key` as an unsaved budget, in a tab.
#[tauri::command]
pub fn open_recovered<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    safety: State<'_, Safety>,
    key: String,
) -> CmdResult {
    let id = open_recovery(&state, &safety, &key)?;
    Ok(finish(&window, &mut state.lock_doc(Some(id))?, None))
}

/// Deletes recovery copy `key` (only one that is listed); returns the rest.
#[tauri::command]
pub fn discard_recovered(safety: State<'_, Safety>, key: String) -> Vec<RecoveryEntry> {
    if let Some(store) = &safety.store {
        if store.list().iter().any(|e| e.key == key) {
            let _w = lock(&safety.writer);
            let _ = store.remove(&key);
        }
    }
    recovery_list(safety)
}

/// Budget `doc`'s file's backups (`None`: the one in front), newest first.
#[tauri::command]
pub fn backup_list(
    state: State<'_, AppState>,
    safety: State<'_, Safety>,
    doc: Option<DocId>,
) -> CmdResult<Vec<BackupEntry>> {
    let path = state.lock_doc(doc)?.path().map(PathBuf::from);
    Ok(match (&safety.backups, path) {
        (Some(b), Some(p)) => b.entries(&p),
        _ => Vec::new(),
    })
}

/// Puts backup `id`'s content in place of budget `doc`'s (`None`: the one
/// in front), as one undo step.
#[tauri::command]
pub fn restore_backup<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    safety: State<'_, Safety>,
    doc: Option<DocId>,
    id: String,
) -> CmdResult {
    restore(&state, &safety, doc, &id)?;
    Ok(finish(&window, &mut state.lock_doc(doc)?, None))
}

/// Settles a file changed outside the app, for budget `doc` (`None`: the one
/// in front): `choice` is `keep` (save this budget over it, its version
/// backed up first) or `load` (load its version, this one backed up first).
#[tauri::command]
pub fn resolve_external_change<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    safety: State<'_, Safety>,
    doc: Option<DocId>,
    choice: String,
) -> CmdResult {
    let choice = match choice.as_str() {
        "keep" => Resolve::KeepMine,
        "load" => Resolve::LoadTheirs,
        other => return Err(format!("Unknown choice {other:?}.")),
    };
    resolve(&state, &safety, doc, choice)?;
    Ok(finish(&window, &mut state.lock_doc(doc)?, None))
}

/// Shows budget `doc`'s backups folder (`None`: the one in front; else the
/// folder of all backups) in Finder, Explorer or the file manager.
#[tauri::command]
pub fn reveal_backups(
    state: State<'_, AppState>,
    safety: State<'_, Safety>,
    doc: Option<DocId>,
) -> CmdResult<()> {
    let backups = safety.backups.as_ref().ok_or("There are no backups yet.")?;
    let folder = state
        .lock_doc(doc)?
        .path()
        .map(|p| backups.folder(p))
        .filter(|f| f.is_dir())
        .unwrap_or_else(|| backups.root().to_path_buf());
    crate::safety::create_private_dir(&folder).map_err(|e| e.to_string())?;
    let program = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    std::process::Command::new(program)
        .arg(&folder)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Couldn't show {}: {e}", folder.display()))
}

/// A recovered budget's first save into its original file asks first.
/// True to go ahead.
pub fn confirm_overwrite<R: Runtime>(
    window: &WebviewWindow<R>,
    state: &AppState,
    id: DocId,
) -> bool {
    let name = {
        let Ok(s) = state.lock_doc(Some(id)) else {
            return false;
        };
        if !s.needs_overwrite_confirm() {
            return true;
        }
        s.display_name()
    };
    const REPLACE: &str = "Replace";
    let choice = window
        .dialog()
        .message(format!(
            "This budget was recovered after the app closed unexpectedly. Saving replaces \
             {name} on disk with the recovered budget. The file as it is now is kept in \
             File > Revert to Backup."
        ))
        .title("Save recovered budget?")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Cancel".into(),
            REPLACE.into(),
        ))
        .parent(window)
        .blocking_show_with_result();
    choice == MessageDialogResult::Custom(REPLACE.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit;
    use crate::safety::tests::temp_dir;
    use crate::safety::AUTOSAVE_QUIET;

    fn later(secs: u64) -> Instant {
        Instant::now() + Duration::from_secs(secs)
    }

    fn retitle(state: &AppState, t: &str) {
        state
            .lock()
            .edit("Test edit", |b| edit::set_title(b, t))
            .unwrap();
    }

    fn files(dir: &Path, ext: &str) -> usize {
        std::fs::read_dir(dir)
            .map(|d| {
                d.flatten()
                    .filter(|e| e.path().extension().is_some_and(|x| x == ext))
                    .count()
            })
            .unwrap_or(0)
    }

    #[test]
    fn untitled_budgets_autosave_to_a_recovery_copy() {
        let dir = temp_dir("app-untitled");
        let safety = Safety::new(Some(&dir));
        let state = AppState::default();
        let rec = dir.join("Recovery");

        // Nothing unsaved: nothing written.
        assert_eq!(
            autosave_doc(&state, &safety, None, later(60), false),
            Autosaved::Nothing
        );
        retitle(&state, "One");
        // Not quiet long enough yet.
        assert_eq!(
            autosave_doc(&state, &safety, None, Instant::now(), false),
            Autosaved::Nothing
        );
        assert_eq!(
            autosave_doc(
                &state,
                &safety,
                None,
                later(AUTOSAVE_QUIET.as_secs()),
                false
            ),
            Autosaved::Recovery
        );
        assert_eq!(files(&rec, "mthd"), 1);
        assert!(
            state.lock().is_dirty(),
            "a recovery copy doesn't save the budget"
        );
        // Written once per change.
        assert_eq!(
            autosave_doc(&state, &safety, None, later(60), false),
            Autosaved::Nothing
        );
        retitle(&state, "Two");
        assert_eq!(
            autosave_doc(&state, &safety, None, later(60), false),
            Autosaved::Recovery
        );
        assert_eq!(files(&rec, "mthd"), 1, "the same copy, rewritten");

        // Undo back to the starter: nothing unsaved, so the copy goes.
        state.lock().undo();
        state.lock().undo();
        assert!(!state.lock().is_dirty());
        assert_eq!(
            autosave_doc(&state, &safety, None, later(60), false),
            Autosaved::Removed
        );
        assert_eq!(files(&rec, "mthd"), 0);

        // Saving (Save As) removes the copy too, and lists the file.
        retitle(&state, "Three");
        assert_eq!(
            autosave_doc(&state, &safety, None, later(60), true),
            Autosaved::Recovery
        );
        let path = dir.join("film.mthd");
        save(&state, &safety, None, &path).unwrap();
        assert_eq!(files(&rec, "mthd"), 0);
        assert!(!state.lock().is_dirty());
        assert_eq!(safety.recent_paths(), [path.canonicalize().unwrap()]);
        drop(safety);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn autosave_on_saves_into_the_file_and_undo_still_works() {
        let dir = temp_dir("app-file");
        let safety = Safety::new(Some(&dir));
        assert!(safety.autosave_on(), "on by default");
        let state = AppState::default();
        let path = dir.join("film.mthd");
        save(&state, &safety, None, &path).unwrap();

        retitle(&state, "Autosaved");
        assert_eq!(
            autosave_doc(&state, &safety, None, Instant::now(), false),
            Autosaved::Nothing
        );
        assert_eq!(
            autosave_doc(&state, &safety, None, later(5), false),
            Autosaved::File
        );
        assert!(!state.lock().is_dirty());
        let on_disk = method_format::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(on_disk.title, "Autosaved");
        assert_eq!(
            files(&dir.join("Recovery"), "mthd"),
            0,
            "no recovery copy needed"
        );
        // The version it replaced is backed up (the first write this session).
        let backups = Backups::new(&dir.join("Backups"));
        assert_eq!(backups.entries(&path).len(), 1);

        // Undo still works across the autosave, and is saved in turn.
        assert!(state.lock().undo());
        assert!(state.lock().is_dirty());
        assert_eq!(
            autosave_doc(&state, &safety, None, later(5), true),
            Autosaved::File
        );
        let on_disk = method_format::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_ne!(on_disk.title, "Autosaved");
        // Later autosaves don't back up again within the gap.
        assert_eq!(backups.entries(&path).len(), 1);

        // Restoring a backup (the file as first saved) is one edit, undoable.
        retitle(&state, "Autosaved again");
        let id = backups.entries(&path)[0].id.clone();
        restore(&state, &safety, None, &id).unwrap();
        assert_eq!(state.lock().budget().title, "Untitled feature");
        assert!(state.lock().undo());
        assert_eq!(state.lock().budget().title, "Autosaved again");
        assert!(state.lock().can_undo());
        assert!(restore(&state, &safety, None, "nope.mthd").is_err());

        // Autosave off: a recovery copy instead, the file untouched.
        safety.set_autosave(false);
        assert!(
            !Safety::new(Some(&dir)).autosave_on(),
            "the setting is kept"
        );
        let before = std::fs::read_to_string(&path).unwrap();
        retitle(&state, "Not in the file");
        assert_eq!(
            autosave_doc(&state, &safety, None, later(5), false),
            Autosaved::Recovery
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
        assert!(state.lock().is_dirty());
        drop(safety);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_failed_autosave_keeps_a_recovery_copy() {
        let dir = temp_dir("app-fail");
        let safety = Safety::new(Some(&dir));
        let state = AppState::default();
        let path = dir.join("gone").join("film.mthd");
        std::fs::create_dir(dir.join("gone")).unwrap();
        save(&state, &safety, None, &path).unwrap();
        std::fs::remove_dir_all(dir.join("gone")).unwrap();
        retitle(&state, "Changed");
        assert!(matches!(
            autosave_doc(&state, &safety, None, later(5), false),
            Autosaved::Failed(_)
        ));
        assert!(state.lock().is_dirty());
        assert_eq!(files(&dir.join("Recovery"), "mthd"), 1);
        drop(safety);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn crash_recovery_round_trip() {
        let dir = temp_dir("app-crash");
        let path = dir.join("film.mthd");
        {
            // A run that saves, edits with Autosave off, then "crashes".
            let safety = Safety::new(Some(&dir));
            safety.set_autosave(false);
            let state = AppState::default();
            save(&state, &safety, None, &path).unwrap();
            retitle(&state, "Unsaved work");
            assert_eq!(
                autosave_doc(&state, &safety, None, later(5), false),
                Autosaved::Recovery
            );
            // Its own copy isn't offered while it runs.
            assert!(Safety::new(Some(&dir)).store.unwrap().list().is_empty());
        }
        let safety = Safety::new(Some(&dir));
        safety.set_autosave(true);
        let state = AppState::default();
        let listed = safety.store.as_ref().unwrap().list();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].title, "Unsaved work");
        assert_eq!(listed[0].name, "film.mthd");
        open_recovery(&state, &safety, &listed[0].key).unwrap();
        {
            let s = state.lock();
            assert_eq!(s.budget().title, "Unsaved work");
            assert!(s.is_dirty());
            assert_eq!(s.path(), Some(path.as_path()));
            assert!(s.needs_overwrite_confirm());
        }
        // Autosave doesn't write into the original file until a save says so.
        let before = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            autosave_doc(&state, &safety, None, later(60), true),
            Autosaved::Nothing
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
        // Now it is this run's: another run starting doesn't offer it.
        assert!(Safety::new(Some(&dir)).store.unwrap().list().is_empty());

        save(&state, &safety, None, &path).unwrap();
        assert!(!state.lock().needs_overwrite_confirm());
        assert_eq!(
            files(&dir.join("Recovery"), "mthd"),
            0,
            "saved: the copy goes"
        );
        assert!(open_recovery(&state, &safety, &listed[0].key).is_err());
        drop(safety);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn write_budget(path: &Path, title: &str) {
        let mut b = method_core::starter::budget();
        b.title = title.into();
        std::fs::write(path, method_format::to_string(&b)).unwrap();
    }

    #[test]
    fn opening_a_file_adds_a_tab_or_finds_its_tab() {
        let dir = temp_dir("app-open-tabs");
        let safety = Safety::new(Some(&dir));
        let state = AppState::default();
        let other = dir.join("other.mthd");
        write_budget(&other, "Other");

        // An untouched Untitled budget alone in the window gives its tab up.
        let untitled = state.docs().active();
        let first = open_path(&state, &safety, &other).unwrap();
        assert_eq!(state.docs().ids(), [first]);
        assert_ne!(first, untitled);

        // A touched one keeps its tab (and its recovery copy).
        let state = AppState::default();
        retitle(&state, "Keep me");
        let untitled = state.docs().active();
        assert_eq!(
            autosave_doc(&state, &safety, None, later(5), false),
            Autosaved::Recovery
        );
        let opened = open_path(&state, &safety, &other).unwrap();
        assert_eq!(state.docs().ids(), [untitled, opened]);
        assert_eq!(state.lock().path(), Some(other.as_path()));
        assert_eq!(
            files(&dir.join("Recovery"), "mthd"),
            1,
            "the untitled one's copy stays"
        );

        // The same file again (by another spelling): its tab comes forward,
        // nothing is read again.
        state
            .lock_doc(Some(opened))
            .unwrap()
            .edit("Test edit", |b| edit::set_title(b, "Edited here"))
            .unwrap();
        state.docs().switch(untitled).unwrap();
        let again = open_path(&state, &safety, &dir.join(".").join("other.mthd")).unwrap();
        assert_eq!(again, opened);
        assert_eq!(state.docs().len(), 2);
        assert_eq!(state.lock().budget().title, "Edited here");

        // A file that can't be read changes nothing.
        assert!(open_path(&state, &safety, &dir.join("missing.mthd")).is_err());
        assert_eq!(state.docs().ids(), [untitled, opened]);
        assert_eq!(state.docs().active(), opened, "unchanged on failure");

        // New makes a new tab every time.
        let fresh = new_tab(&state);
        assert_eq!(state.docs().ids(), [untitled, opened, fresh]);
        drop(safety);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn every_tab_autosaves_recovers_and_undoes_on_its_own() {
        let dir = temp_dir("app-tabs-autosave");
        let safety = Safety::new(Some(&dir));
        let state = AppState::default();
        let film = dir.join("film.mthd");
        write_budget(&film, "Film");
        // Edit the untitled one first (so the file doesn't take its tab),
        // then the file's.
        let untitled = state.docs().active();
        retitle(&state, "Draft");
        let saved = open_path(&state, &safety, &film).unwrap();
        assert_eq!(state.docs().ids(), [untitled, saved]);
        state
            .lock_doc(Some(saved))
            .unwrap()
            .edit("Test edit", |b| edit::set_title(b, "Film, edited"))
            .unwrap();
        assert_eq!(state.docs().dirty(), [untitled, saved]);

        // One pass: the file's budget into its file, the untitled one into
        // its own recovery copy.
        let done = autosave_pass(&state, &safety, later(5), false);
        assert_eq!(
            done,
            [(untitled, Autosaved::Recovery), (saved, Autosaved::File)]
        );
        assert_eq!(title_on_disk(&film), "Film, edited");
        assert_eq!(files(&dir.join("Recovery"), "mthd"), 1);
        assert_eq!(state.docs().dirty(), [untitled]);
        // Backups are per file.
        assert_eq!(Backups::new(&dir.join("Backups")).entries(&film).len(), 1);

        // Undo is per tab: undoing in one leaves the other alone.
        assert!(state.lock_doc(Some(untitled)).unwrap().undo());
        assert_eq!(
            state.lock_doc(Some(untitled)).unwrap().budget().title,
            "Untitled feature"
        );
        assert_eq!(
            state.lock_doc(Some(saved)).unwrap().budget().title,
            "Film, edited"
        );
        assert!(state.lock_doc(Some(saved)).unwrap().can_undo());
        assert!(!state.lock_doc(Some(untitled)).unwrap().can_undo());
        // Back at its start: its copy goes on the next pass.
        let done = autosave_pass(&state, &safety, later(5), false);
        assert_eq!(done[0], (untitled, Autosaved::Removed));
        assert_eq!(files(&dir.join("Recovery"), "mthd"), 0);
        drop(safety);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn closing_a_tab_asks_only_about_unsaved_work() {
        let dir = temp_dir("app-close-tab");
        let safety = Safety::new(Some(&dir));
        let state = AppState::default();
        let only = state.docs().active();
        assert_eq!(
            close_doc(&state, &safety, only, false).unwrap(),
            Closed::Last
        );

        let film = dir.join("film.mthd");
        write_budget(&film, "Film");
        retitle(&state, "Draft");
        let saved = open_path(&state, &safety, &film).unwrap();
        // Clean: closes; the untitled one comes to the front.
        assert_eq!(
            close_doc(&state, &safety, saved, false).unwrap(),
            Closed::Closed
        );
        assert_eq!(state.docs().ids(), [only]);
        let saved = open_path(&state, &safety, &film).unwrap();

        // Unsaved (an untitled draft with a recovery copy): asks first.
        assert_eq!(
            autosave_doc(&state, &safety, Some(only), later(5), false),
            Autosaved::Recovery
        );
        assert_eq!(
            close_doc(&state, &safety, only, false).unwrap(),
            Closed::Unsaved
        );
        assert_eq!(state.docs().len(), 2);
        // Thrown away on purpose: closed, its recovery copy gone with it.
        assert_eq!(
            close_doc(&state, &safety, only, true).unwrap(),
            Closed::Closed
        );
        assert_eq!(state.docs().ids(), [saved]);
        assert_eq!(files(&dir.join("Recovery"), "mthd"), 0);
        assert!(
            close_doc(&state, &safety, only, true).is_err(),
            "already closed"
        );

        // With Autosave on, a file's budget is saved first and closes clean.
        let other = dir.join("other.mthd");
        write_budget(&other, "Other");
        let o = open_path(&state, &safety, &other).unwrap();
        state
            .lock_doc(Some(o))
            .unwrap()
            .edit("Test edit", |b| edit::set_title(b, "Other, edited"))
            .unwrap();
        assert_eq!(
            autosave_doc(&state, &safety, Some(o), Instant::now(), true),
            Autosaved::File
        );
        assert_eq!(
            close_doc(&state, &safety, o, false).unwrap(),
            Closed::Closed
        );
        assert_eq!(title_on_disk(&other), "Other, edited");
        drop(safety);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn crash_recovery_gives_each_tab_back() {
        let dir = temp_dir("app-crash-tabs");
        let film = dir.join("film.mthd");
        write_budget(&film, "Film");
        {
            // A run with two tabs of unsaved work, Autosave off, then a crash.
            let safety = Safety::new(Some(&dir));
            safety.set_autosave(false);
            let state = AppState::default();
            retitle(&state, "Untitled draft");
            let f = open_path(&state, &safety, &film).unwrap();
            state
                .lock_doc(Some(f))
                .unwrap()
                .edit("Test edit", |b| edit::set_title(b, "Film, unsaved"))
                .unwrap();
            let done = autosave_pass(&state, &safety, later(5), false);
            assert!(
                done.iter().all(|(_, d)| *d == Autosaved::Recovery),
                "{done:?}"
            );
            assert_eq!(files(&dir.join("Recovery"), "mthd"), 2, "one copy per tab");
            // The next start would reopen the file.
            safety.remember_tabs(&state.docs());
        }
        let safety = Safety::new(Some(&dir));
        let state = AppState::default();
        // The file comes back as it is on disk...
        assert_eq!(restore_tabs(&state, &safety), 1);
        let reopened = state.docs().active();
        assert_eq!(state.lock().budget().title, "Film");
        let listed = safety.store.as_ref().unwrap().list();
        assert_eq!(listed.len(), 2);
        // ...and its recovered copy takes the place of that untouched tab.
        let film_key = listed
            .iter()
            .find(|e| e.path.is_some())
            .unwrap()
            .key
            .clone();
        let draft_key = listed
            .iter()
            .find(|e| e.path.is_none())
            .unwrap()
            .key
            .clone();
        let recovered = open_recovery(&state, &safety, &film_key).unwrap();
        assert_eq!(state.docs().len(), 1);
        assert_ne!(recovered, reopened);
        assert_eq!(state.lock().budget().title, "Film, unsaved");
        assert!(state.lock().needs_overwrite_confirm());
        // The untitled draft gets a tab of its own.
        let draft = open_recovery(&state, &safety, &draft_key).unwrap();
        assert_eq!(state.docs().ids(), [recovered, draft]);
        assert_eq!(state.lock().budget().title, "Untitled draft");
        assert!(state.lock().is_dirty() && state.lock().path().is_none());
        drop(safety);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_next_start_reopens_the_files_that_were_open() {
        let dir = temp_dir("app-restore");
        let (a, b, gone) = (
            dir.join("a.mthd"),
            dir.join("b.mthd"),
            dir.join("gone.mthd"),
        );
        for p in [&a, &b, &gone] {
            write_budget(p, &p.file_stem().unwrap().to_string_lossy());
        }
        {
            let safety = Safety::new(Some(&dir));
            let state = AppState::default();
            open_path(&state, &safety, &a).unwrap();
            new_tab(&state); // untitled: not reopened
            open_path(&state, &safety, &gone).unwrap();
            let bb = open_path(&state, &safety, &b).unwrap();
            state.docs().move_tab(bb, 1).unwrap();
            safety.remember_tabs(&state.docs());
        }
        std::fs::remove_file(&gone).unwrap();
        let safety = Safety::new(Some(&dir));
        let state = AppState::default();
        assert_eq!(
            restore_tabs(&state, &safety),
            2,
            "the missing file is left out"
        );
        let labels: Vec<String> = state.docs().tabs().into_iter().map(|t| t.label).collect();
        assert_eq!(labels, ["a", "b"], "in tab order");
        assert_eq!(
            state.lock().budget().title,
            "b",
            "the one in front is in front"
        );
        assert!(state.docs().dirty().is_empty());

        // Nothing to reopen: the starting Untitled budget stays.
        let empty = temp_dir("app-restore-none");
        let safety = Safety::new(Some(&empty));
        let state = AppState::default();
        assert_eq!(restore_tabs(&state, &safety), 0);
        assert_eq!(state.docs().len(), 1);
        drop(safety);
        std::fs::remove_dir_all(&dir).unwrap();
        std::fs::remove_dir_all(&empty).unwrap();
    }

    /// Save As onto the file of another open budget is refused, as exports,
    /// templates and copies are: two tabs writing one file would each find
    /// the other's saves as changes made outside the app.
    #[test]
    fn save_as_never_writes_over_another_tabs_file() {
        let dir = temp_dir("app-save-over-tab");
        let safety = Safety::new(Some(&dir));
        let state = AppState::default();
        let (one, two) = (dir.join("one.mthd"), dir.join("two.mthd"));
        write_budget(&one, "One");
        write_budget(&two, "Two");
        let a = open_path(&state, &safety, &one).unwrap();
        let b = open_path(&state, &safety, &two).unwrap();
        for (id, t) in [(a, "One, mine"), (b, "Two, mine")] {
            state
                .lock_doc(Some(id))
                .unwrap()
                .edit("Test edit", |bud| edit::set_title(bud, t))
                .unwrap();
        }
        let refused = save(&state, &safety, Some(a), &two).unwrap_err();
        assert!(refused.contains("open in another tab"), "{refused}");
        // Another spelling of the same file is the same file.
        let spelled = dir.join(".").join("two.mthd");
        assert!(save(&state, &safety, Some(a), &spelled).is_err());
        assert_eq!(title_on_disk(&two), "Two");
        assert_eq!(state.lock_doc(Some(a)).unwrap().path(), Some(one.as_path()));
        // Its own file, and a new name, save as ever.
        assert!(save(&state, &safety, Some(a), &one).unwrap());
        assert!(save(&state, &safety, Some(b), &two).unwrap());
        let three = dir.join("three.mthd");
        assert!(save(&state, &safety, Some(a), &three).unwrap());
        assert_eq!(title_on_disk(&three), "One, mine");
        assert_eq!(title_on_disk(&two), "Two, mine");
        drop(safety);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn saving_and_conflicts_are_per_tab() {
        let dir = temp_dir("app-tab-conflict");
        let safety = Safety::new(Some(&dir));
        let state = AppState::default();
        let (one, two) = (dir.join("one.mthd"), dir.join("two.mthd"));
        write_budget(&one, "One");
        write_budget(&two, "Two");
        let a = open_path(&state, &safety, &one).unwrap();
        let b = open_path(&state, &safety, &two).unwrap();
        for (id, t) in [(a, "One, mine"), (b, "Two, mine")] {
            state
                .lock_doc(Some(id))
                .unwrap()
                .edit("Test edit", |bud| edit::set_title(bud, t))
                .unwrap();
        }
        write_theirs(&one, "One, theirs");
        // Only the changed file's tab is held back; the other saves.
        let done = autosave_pass(&state, &safety, later(5), false);
        assert_eq!(done, [(a, Autosaved::Conflict), (b, Autosaved::File)]);
        assert!(state.docs().tabs()[0].conflict && !state.docs().tabs()[1].conflict);
        // Settled for that tab only, with the other in front.
        assert_eq!(state.docs().active(), b);
        resolve(&state, &safety, Some(a), Resolve::LoadTheirs).unwrap();
        assert_eq!(state.docs().get(a).unwrap().budget().title, "One, theirs");
        assert_eq!(state.lock().budget().title, "Two, mine");
        // An explicit Save names its tab.
        state
            .lock_doc(Some(a))
            .unwrap()
            .edit("Test edit", |bud| edit::set_title(bud, "One, again"))
            .unwrap();
        assert!(save(&state, &safety, Some(a), &one).unwrap());
        assert_eq!(title_on_disk(&one), "One, again");
        drop(safety);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Someone else's version of the budget file (another app, a sync).
    fn write_theirs(path: &Path, title: &str) {
        let mut b = method_core::starter::budget();
        b.title = title.into();
        std::fs::write(path, method_format::to_string(&b)).unwrap();
    }

    fn title_on_disk(path: &Path) -> String {
        method_format::from_str(&std::fs::read_to_string(path).unwrap())
            .unwrap()
            .title
    }

    /// A saved budget with an unsaved edit, and the file changed elsewhere.
    fn conflicted(tag: &str) -> (PathBuf, Safety, AppState, PathBuf) {
        let dir = temp_dir(tag);
        let safety = Safety::new(Some(&dir));
        let state = AppState::default();
        let path = dir.join("film.mthd");
        assert!(save(&state, &safety, None, &path).unwrap());
        retitle(&state, "Mine");
        write_theirs(&path, "Theirs, from the other computer");
        (dir, safety, state, path)
    }

    #[test]
    fn outside_changes_pause_autosave_and_block_save() {
        let (dir, safety, state, path) = conflicted("app-outside");
        assert_eq!(
            autosave_doc(&state, &safety, None, later(5), false),
            Autosaved::Conflict
        );
        assert_eq!(title_on_disk(&path), "Theirs, from the other computer");
        assert!(state.lock().in_conflict() && state.lock().is_dirty());
        // The unsaved work is still safe: a recovery copy.
        assert_eq!(files(&dir.join("Recovery"), "mthd"), 1);
        // Paused: later passes and edits never write into the file, and the
        // question isn't raised again by autosave.
        assert_eq!(
            autosave_doc(&state, &safety, None, later(60), true),
            Autosaved::Nothing
        );
        retitle(&state, "Mine, edited more");
        assert_eq!(
            autosave_doc(&state, &safety, None, later(60), false),
            Autosaved::Recovery
        );
        assert_eq!(title_on_disk(&path), "Theirs, from the other computer");
        // An explicit Save into the file is refused too.
        assert!(!save(&state, &safety, None, &path).unwrap());
        assert_eq!(title_on_disk(&path), "Theirs, from the other computer");
        drop(safety);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn keep_mine_backs_up_theirs_then_saves_over_it() {
        let (dir, safety, state, path) = conflicted("app-keep");
        assert!(!save(&state, &safety, None, &path).unwrap());
        resolve(&state, &safety, None, Resolve::KeepMine).unwrap();
        assert_eq!(title_on_disk(&path), "Mine");
        let s = state.lock();
        assert!(!s.in_conflict() && !s.is_dirty());
        drop(s);
        let backups = Backups::new(&dir.join("Backups"));
        let newest = &backups.entries(&path)[0];
        let theirs = method_format::from_str(&backups.read(&path, &newest.id).unwrap()).unwrap();
        assert_eq!(theirs.title, "Theirs, from the other computer");
        // Autosave carries on into the file.
        retitle(&state, "Mine again");
        assert_eq!(
            autosave_doc(&state, &safety, None, later(5), false),
            Autosaved::File
        );
        assert_eq!(title_on_disk(&path), "Mine again");
        drop(safety);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn load_theirs_backs_up_mine_and_undo_brings_it_back() {
        let (dir, safety, state, path) = conflicted("app-load");
        assert_eq!(
            autosave_doc(&state, &safety, None, later(5), false),
            Autosaved::Conflict
        );
        resolve(&state, &safety, None, Resolve::LoadTheirs).unwrap();
        {
            let s = state.lock();
            assert_eq!(s.budget().title, "Theirs, from the other computer");
            assert!(!s.in_conflict() && !s.is_dirty());
        }
        assert_eq!(title_on_disk(&path), "Theirs, from the other computer");
        let backups = Backups::new(&dir.join("Backups"));
        let newest = &backups.entries(&path)[0];
        let mine = method_format::from_str(&backups.read(&path, &newest.id).unwrap()).unwrap();
        assert_eq!(mine.title, "Mine");
        // Nothing unsaved: the recovery copy went with the conflict.
        assert_eq!(files(&dir.join("Recovery"), "mthd"), 0);
        // Undo brings mine back, unsaved; autosave then writes it.
        assert!(state.lock().undo());
        assert_eq!(state.lock().budget().title, "Mine");
        assert!(state.lock().is_dirty());
        assert_eq!(
            autosave_doc(&state, &safety, None, later(5), false),
            Autosaved::File
        );
        drop(safety);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A copy saved for sharing opens read-only; when its file changes
    /// outside the app (the sender's newer copy saved over it), Load Theirs
    /// still brings the file's version in: reading its own file isn't an
    /// edit. Refusing it would leave only Keep Mine, which writes the stale
    /// copy over the newer one.
    #[test]
    fn load_theirs_works_on_a_read_only_shared_copy() {
        let dir = temp_dir("app-load-shared");
        let safety = Safety::new(Some(&dir));
        let path = dir.join("shared.mthd");
        let shared = method_core::share::share_copy(
            &method_core::starter::budget(),
            None,
            &method_core::share::ShareOptions::default(),
            "2026-10-09T14:30:00-07:00",
            "0.1.0",
            None,
        )
        .unwrap();
        std::fs::write(&path, method_format::to_string(&shared)).unwrap();
        let state = AppState::with(Docs::with(Session::open(&path).unwrap()));
        assert!(state.lock().is_read_only());
        let mut newer = shared.clone();
        newer.title = "The newer copy".into();
        std::fs::write(&path, method_format::to_string(&newer)).unwrap();

        // Keep Mine is refused: it would write the stale copy over the newer one.
        assert_eq!(
            resolve(&state, &safety, None, Resolve::KeepMine),
            Err(crate::sharing::READ_ONLY.to_string())
        );
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            method_format::to_string(&newer)
        );
        resolve(&state, &safety, None, Resolve::LoadTheirs).unwrap();
        let mut s = state.lock();
        assert_eq!(s.budget().title, "The newer copy");
        assert!(s.is_read_only() && !s.is_dirty() && !s.in_conflict());
        // Still read-only to the user's edits.
        assert_eq!(
            s.edit("Test edit", |b| edit::set_title(b, "Mine")),
            Err(crate::sharing::READ_ONLY.to_string())
        );
        // Undo brings the copy as it was back.
        assert!(s.undo());
        assert_eq!(s.budget().title, shared.title);
        drop(s);
        drop(safety);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn save_mine_as_a_copy_leaves_theirs_alone() {
        let (dir, safety, state, path) = conflicted("app-copy");
        let copy = dir.join("film (mine).mthd");
        assert!(save(&state, &safety, None, &copy).unwrap());
        assert_eq!(title_on_disk(&path), "Theirs, from the other computer");
        assert_eq!(title_on_disk(&copy), "Mine");
        let s = state.lock();
        assert_eq!(s.path(), Some(copy.as_path()));
        assert!(!s.in_conflict() && !s.is_dirty());
        drop(s);
        drop(safety);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn touched_or_missing_files_are_not_outside_changes() {
        let dir = temp_dir("app-touch");
        let safety = Safety::new(Some(&dir));
        let state = AppState::default();
        let path = dir.join("film.mthd");
        save(&state, &safety, None, &path).unwrap();
        // A sync service rewrites the same bytes with a new time.
        let later_time = std::time::SystemTime::now() + Duration::from_secs(30);
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(later_time)
            .unwrap();
        retitle(&state, "Edited");
        assert_eq!(
            autosave_doc(&state, &safety, None, later(5), false),
            Autosaved::File
        );
        // Moved away or deleted: the next save puts it back.
        std::fs::remove_file(&path).unwrap();
        retitle(&state, "Edited again");
        assert_eq!(
            autosave_doc(&state, &safety, None, later(5), false),
            Autosaved::File
        );
        assert_eq!(title_on_disk(&path), "Edited again");
        // Opened from disk, then changed elsewhere: noticed too.
        let opened = dir.join("opened.mthd");
        write_theirs(&opened, "Before");
        open_path(&state, &safety, &opened).unwrap();
        write_theirs(&opened, "After, from elsewhere");
        assert!(state.lock().changed_on_disk());
        drop(safety);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Every command here is registered, listed for access control and
    /// allowed for the window (a command missing from any of the three
    /// can't be called).
    #[test]
    fn safety_commands_are_wired() {
        const APP_RS: &str = include_str!("app.rs");
        const BUILD_RS: &str = include_str!("../build.rs");
        const CAPS: &str = include_str!("../capabilities/main.json");
        const COMMANDS: &[&str] = &[
            "autosave_setting",
            "set_autosave",
            "flush_autosave",
            "open_pending",
            "open_recent",
            "recovery_list",
            "open_recovered",
            "discard_recovered",
            "backup_list",
            "restore_backup",
            "reveal_backups",
            "resolve_external_change",
        ];
        for c in COMMANDS {
            assert!(
                APP_RS.contains(&format!("crate::safety_app::{c},")),
                "{c} not in generate_handler!"
            );
            assert!(
                BUILD_RS.contains(&format!("\"{c}\",")),
                "{c} not in build.rs"
            );
            let allow = format!("\"allow-{}\"", c.replace('_', "-"));
            assert!(CAPS.contains(&allow), "{c} not allowed in capabilities");
        }
    }

    #[test]
    fn command_line_paths() {
        let dir = temp_dir("app-args");
        let f = dir.join("film.mthd");
        std::fs::write(&f, "{}").unwrap();
        let args = |v: &[&std::ffi::OsStr]| path_from_args(v.iter().map(|s| s.to_os_string()));
        assert_eq!(
            args(&["-psn_0_123".as_ref(), f.as_os_str()]),
            Some(f.clone())
        );
        assert_eq!(args(&[dir.join("missing.mthd").as_os_str()]), None);
        assert_eq!(args(&[dir.as_os_str()]), None, "a folder isn't a budget");
        assert_eq!(args(&[]), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
