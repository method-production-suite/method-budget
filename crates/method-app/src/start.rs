//! The start window and New Budget (roadmap 3.19; decisions.md, entry 63;
//! design: docs/research/new-project-window.md; docs/APP.md, "The start
//! window").
//!
//! - **The lists.** Pinned budgets (kept in the app's own `start.json`, at
//!   most [`PIN_LIMIT`]) and the recent ones (File > Open Recent's list,
//!   src/safety.rs), each with its title, net total, base currency, when the
//!   file last changed and its folder. A file is read once and its summary
//!   kept until its modified time or size changes; a budget open in a tab is
//!   summed from the tab, so its unsaved changes count.
//! - **Paths stay here.** The page gets display strings and a token per row
//!   from the last listing (`start_list`), and names a row only by that
//!   token, as the Packs list does (src/packs.rs).
//! - **New Budget** builds a budget from the form ([`NewBudgetSpec`]): the
//!   starter, an empty budget, a bundled chart or a picked budget file
//!   (stripped as New From Template strips it, src/templates.rs), then the
//!   title, base currency, budget info, schedule-length globals, fringe
//!   posting and arithmetic, all through the edit functions the setup tabs
//!   use, so the same checks apply. It opens in a new tab (or in place of an
//!   untouched Untitled budget alone in the window), with no file, as one
//!   undo step, or saved into a folder (Save In).
//! - **Setup at creation** (decision 77): everything in the setup library
//!   and any ticked setup packs come in before the form's own details,
//!   through method-session's `new_budget` module; this side reads the
//!   library's file and the listed packs.
//! - **Projects, thumbnails and Duplicate** (decision 91). A project is a
//!   folder: the ones added with Add Folder… or made by New Budget's "Make a
//!   project folder" box are kept in `start.json`, and each is read two
//!   levels deep ([`SCAN_LIMIT`] entries at most) every time the lists are
//!   shown; method-session's `projects` module says what each file is and
//!   how they are arranged. A budget's thumbnail (a bar per category) is
//!   kept with its summary in [`CACHE_FILE`], read again when the file's
//!   modified time or size changes, never written into the budget. Duplicate
//!   copies a budget file beside itself under the first free name and opens
//!   it as a saved tab.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use method_core::{starter, Budget, Dec};
use method_session::new_budget::{self, Ticked};
use method_session::projects::{self, ItemKind, ThumbBar};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::app::{finish, safe_file_stem, AppState, DocId};
use crate::bookmark::{self, Tracked};
use crate::docs::Docs;
use crate::edit::{self, InfoInput};
use crate::library::Library;
use crate::packs::{ListedPack, Packs};
use crate::safety::{self, Access};
use crate::safety_app::Safety;
use crate::session::Session;
use crate::view::View;

mod create;
mod lists;
#[cfg(test)]
mod tests;

pub use self::create::*;
pub use self::lists::*;

/// Most budgets kept pinned.
pub const PIN_LIMIT: usize = 100;
/// The pinned list's file, in the app's local data folder.
pub const START_FILE: &str = "start.json";
/// The kept summaries (title, net total, currency) of listed files, beside
/// [`START_FILE`], so a start doesn't read every file again.
pub const CACHE_FILE: &str = "start-cache.json";
/// Most summaries kept.
const CACHE_LIMIT: usize = 500;
/// Most project folders listed.
pub const PROJECT_LIMIT: usize = 50;
/// Most files and folders looked at in one project folder (it and the
/// folders directly in it).
pub const SCAN_LIMIT: usize = 300;
/// Longest title, and longest text field, the form takes.
const TITLE_MAX: usize = 200;
const TEXT_MAX: usize = 100;

// ---------------------------------------------------------------------------
// The app's state

#[derive(Default, Serialize, Deserialize)]
struct StartJson {
    #[serde(default)]
    pinned: Vec<String>,
    /// Each pinned file's bookmark, by path, as hex (src/bookmark.rs).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    bookmarks: BTreeMap<String, String>,
    /// The folder the last new budget was saved in (New Budget's Save In).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_folder: Option<String>,
    /// The project folders, in the order they were added.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    projects: Vec<String>,
}

/// What a file holds, for its row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Summary {
    title: String,
    net_total: Option<i64>,
    currency: String,
    /// The thumbnail: a bar per category (method-session's `projects`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    bars: Vec<ThumbBar>,
    /// A budget, a sub-budget, a copy or a setup pack (its provenance).
    #[serde(default = "budget_kind")]
    kind: ItemKind,
    /// For a sub-budget or a copy: the title of the budget it came from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source: Option<String>,
}

fn budget_kind() -> ItemKind {
    ItemKind::Budget
}

/// A file's modified time and size: its summary is read again only when
/// one of them changes.
type Stamp = (Option<SystemTime>, u64);

/// A file's summary, with the modified time and size it was read at.
struct Cached {
    stamp: Stamp,
    summary: Result<Summary, String>,
}

/// One kept summary in [`CACHE_FILE`].
#[derive(Serialize, Deserialize)]
struct CachedJson {
    path: String,
    /// Modified time: seconds and nanoseconds since 1970.
    modified: Option<(u64, u32)>,
    size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    summary: Option<Summary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    problem: Option<String>,
}

#[derive(Default, Serialize, Deserialize)]
struct CacheJson {
    /// [`CACHE_VERSION`]: a cache written before thumbnails and kinds were
    /// kept is read as empty, so every file is read once more.
    #[serde(default)]
    version: u32,
    #[serde(default)]
    files: Vec<CachedJson>,
}

/// What [`CACHE_FILE`] holds now: 2 adds thumbnails and kinds.
const CACHE_VERSION: u32 = 2;

#[derive(Default)]
struct Inner {
    token: u64,
    /// The last listing: token -> file.
    listed: Vec<(u64, PathBuf)>,
    cache: HashMap<PathBuf, Cached>,
    /// The cache differs from [`CACHE_FILE`].
    cache_changed: bool,
    /// The budget file last picked as New Budget's starting point.
    picked: Option<(u64, Budget)>,
    /// The folder New Budget saves in, as last given to the page.
    folder: Option<(u64, PathBuf)>,
}

/// The pinned list, the folder new budgets were last saved in and the
/// project folders.
#[derive(Default)]
struct Kept {
    pinned: Vec<Tracked>,
    last_folder: Option<PathBuf>,
    projects: Vec<PathBuf>,
}

/// The start window's state (one per app).
#[derive(Default)]
pub struct StartState {
    file: Option<PathBuf>,
    cache_file: Option<PathBuf>,
    kept: Mutex<Kept>,
    inner: Mutex<Inner>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn stamp_json(t: Option<SystemTime>) -> Option<(u64, u32)> {
    let d = t?.duration_since(UNIX_EPOCH).ok()?;
    Some((d.as_secs(), d.subsec_nanos()))
}

fn stamp_time(t: Option<(u64, u32)>) -> Option<SystemTime> {
    let (s, n) = t?;
    UNIX_EPOCH.checked_add(Duration::new(s, n.min(999_999_999)))
}

/// The most the start window's own files (its list and its summaries) are
/// read up to: far more than they hold, and nothing larger is read in.
const START_FILE_MAX_BYTES: u64 = 16 * 1024 * 1024;

/// One of the start window's own files as text, up to [`START_FILE_MAX_BYTES`].
fn read_start_file(path: &Path) -> Option<String> {
    method_format::fsio::read_to_string_capped(path, START_FILE_MAX_BYTES).ok()
}

/// The summaries kept in `file` (none when missing or unreadable).
fn load_cache(file: Option<&Path>) -> HashMap<PathBuf, Cached> {
    file.and_then(read_start_file)
        .and_then(|t| serde_json::from_str::<CacheJson>(&t).ok())
        .filter(|c| c.version == CACHE_VERSION)
        .unwrap_or_default()
        .files
        .into_iter()
        .take(CACHE_LIMIT)
        .filter_map(|c| {
            let summary = match (c.summary, c.problem) {
                (Some(s), _) => Ok(s),
                (None, Some(p)) => Err(p),
                (None, None) => return None,
            };
            let stamp = (stamp_time(c.modified), c.size);
            Some((PathBuf::from(c.path), Cached { stamp, summary }))
        })
        .collect()
}

impl StartState {
    /// The pinned list kept in `file` (none: nothing is kept between runs),
    /// and the summaries kept beside it in [`CACHE_FILE`]. A missing or
    /// unreadable file is an empty list.
    pub fn load(file: Option<PathBuf>) -> StartState {
        let json = file
            .as_deref()
            .and_then(read_start_file)
            .and_then(|t| serde_json::from_str::<StartJson>(&t).ok())
            .unwrap_or_default();
        let mut pinned = bookmark::load_list(json.pinned, &json.bookmarks);
        pinned.truncate(PIN_LIMIT);
        let cache_file = file
            .as_deref()
            .and_then(Path::parent)
            .map(|d| d.join(CACHE_FILE));
        let cache = load_cache(cache_file.as_deref());
        StartState {
            file,
            cache_file,
            kept: Mutex::new(Kept {
                pinned,
                last_folder: json
                    .last_folder
                    .filter(|s| !s.is_empty())
                    .map(PathBuf::from),
                projects: json
                    .projects
                    .into_iter()
                    .filter(|s| !s.is_empty())
                    .map(PathBuf::from)
                    .take(PROJECT_LIMIT)
                    .collect(),
            }),
            inner: Mutex::new(Inner {
                cache,
                ..Inner::default()
            }),
        }
    }

    pub fn pinned(&self) -> Vec<PathBuf> {
        lock(&self.kept)
            .pinned
            .iter()
            .map(|t| t.path.clone())
            .collect()
    }

    /// Pins `path` (at the top) or unpins it.
    pub fn set_pinned(&self, path: &Path, on: bool) -> Result<(), String> {
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let mut kept = lock(&self.kept);
        let was = kept
            .pinned
            .iter()
            .position(|t| t.path == path)
            .map(|i| kept.pinned.remove(i));
        if on {
            if kept.pinned.len() >= PIN_LIMIT {
                return Err(format!(
                    "At most {PIN_LIMIT} budgets can be pinned. Unpin one first."
                ));
            }
            let t = was
                .filter(|t| t.mark.is_some())
                .unwrap_or_else(|| Tracked::new(path));
            kept.pinned.insert(0, t);
        }
        self.save(&kept)
    }

    /// Follows pinned files moved or renamed in the Finder since (their
    /// bookmarks). True if any moved.
    pub fn follow_pinned(&self) -> bool {
        let mut kept = lock(&self.kept);
        let moved = bookmark::follow_all(&mut kept.pinned);
        if moved {
            let _ = self.save(&kept);
        }
        moved
    }

    /// The folder the last new budget was saved in.
    pub fn last_folder(&self) -> Option<PathBuf> {
        lock(&self.kept).last_folder.clone()
    }

    /// The project folders, in the order they were added.
    pub fn projects(&self) -> Vec<PathBuf> {
        lock(&self.kept).projects.clone()
    }

    /// Adds folder `dir` to the projects (at the end; once).
    pub fn add_project(&self, dir: &Path) -> Result<(), String> {
        let dir = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
        let mut kept = lock(&self.kept);
        if kept.projects.contains(&dir) {
            return Ok(());
        }
        if kept.projects.len() >= PROJECT_LIMIT {
            return Err(format!(
                "At most {PROJECT_LIMIT} project folders can be listed. Remove one first."
            ));
        }
        kept.projects.push(dir);
        self.save(&kept)
    }

    /// Takes folder `dir` off the projects (the folder stays).
    pub fn remove_project(&self, dir: &Path) -> Result<(), String> {
        let mut kept = lock(&self.kept);
        kept.projects.retain(|p| p != dir);
        self.save(&kept)
    }

    fn set_last_folder(&self, dir: &Path) -> Result<(), String> {
        let mut kept = lock(&self.kept);
        kept.last_folder = Some(dir.to_path_buf());
        self.save(&kept)
    }

    fn save(&self, kept: &Kept) -> Result<(), String> {
        let Some(file) = &self.file else {
            return Ok(());
        };
        if let Some(dir) = file.parent() {
            safety::create_private_dir(dir).map_err(|e| e.to_string())?;
        }
        let (pinned, bookmarks) = bookmark::save_list(&kept.pinned);
        let json = StartJson {
            pinned,
            bookmarks,
            last_folder: kept
                .last_folder
                .as_deref()
                .and_then(Path::to_str)
                .map(str::to_string),
            projects: kept
                .projects
                .iter()
                .filter_map(|p| p.to_str())
                .map(str::to_string)
                .collect(),
        };
        let bytes = serde_json::to_vec_pretty(&json).map_err(|e| e.to_string())?;
        safety::write_atomically(file, &bytes, Access::Private)
            .map_err(|e| format!("Couldn't keep the pinned list: {e}"))
    }

    /// Writes the summaries to [`CACHE_FILE`] when they changed. Best
    /// effort: without it the next start reads the files again.
    fn save_cache(&self, g: &mut Inner) {
        let Some(file) = &self.cache_file else {
            return;
        };
        if !g.cache_changed {
            return;
        }
        let mut files: Vec<CachedJson> = g
            .cache
            .iter()
            .filter_map(|(p, c)| {
                Some(CachedJson {
                    path: p.to_str()?.to_string(),
                    modified: stamp_json(c.stamp.0),
                    size: c.stamp.1,
                    summary: c.summary.as_ref().ok().cloned(),
                    problem: c.summary.as_ref().err().cloned(),
                })
            })
            .collect();
        files.sort_by(|a, b| a.path.cmp(&b.path));
        let Ok(bytes) = serde_json::to_vec_pretty(&CacheJson {
            version: CACHE_VERSION,
            files,
        }) else {
            return;
        };
        if let Some(dir) = file.parent() {
            if safety::create_private_dir(dir).is_err() {
                return;
            }
        }
        if safety::write_atomically(file, &bytes, Access::Private).is_ok() {
            g.cache_changed = false;
        }
    }

    /// The file listed under `token` in the last listing.
    pub fn listed(&self, token: u64) -> Result<PathBuf, String> {
        lock(&self.inner)
            .listed
            .iter()
            .find(|(t, _)| *t == token)
            .map(|(_, p)| p.clone())
            .ok_or_else(|| crate::i18n::tr("start.list_changed"))
    }

    /// Keeps `b` as the picked starting point; its token.
    fn keep_picked(&self, b: Budget) -> u64 {
        let mut g = lock(&self.inner);
        g.token += 1;
        let t = g.token;
        g.picked = Some((t, b));
        t
    }

    fn picked(&self, token: Option<u64>) -> Result<Budget, String> {
        let g = lock(&self.inner);
        match (&g.picked, token) {
            (Some((t, b)), Some(want)) if *t == want => Ok(b.clone()),
            _ => Err(crate::i18n::tr("start.choose_file_again")),
        }
    }

    /// Keeps `dir` as the folder to save in; its token.
    fn keep_folder(&self, dir: PathBuf) -> u64 {
        let mut g = lock(&self.inner);
        g.token += 1;
        let t = g.token;
        g.folder = Some((t, dir));
        t
    }

    fn folder(&self, token: u64) -> Result<PathBuf, String> {
        let g = lock(&self.inner);
        match &g.folder {
            Some((t, dir)) if *t == token => Ok(dir.clone()),
            _ => Err(crate::i18n::tr("start.choose_folder_again")),
        }
    }
}

/// Puts the start window's state in the app (from `app.rs`'s setup).
pub fn init<R: Runtime>(app: &AppHandle<R>) {
    let file = app
        .path()
        .app_local_data_dir()
        .ok()
        .map(|d| d.join(START_FILE));
    app.manage(StartState::load(file));
}

// ---------------------------------------------------------------------------
// Commands (registered in app.rs, build.rs and capabilities/main.json; only
// the main window may call them)

/// The pinned and recent budgets, each with a token for the other commands.
/// Files moved or renamed in the Finder are followed first.
#[tauri::command]
pub async fn start_list<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    safety: State<'_, Safety>,
    start: State<'_, StartState>,
) -> Result<StartView, String> {
    if safety.follow_recent() {
        crate::safety_app::refresh_menu(&app);
    }
    Ok(list(&state, &start, &safety.recent_paths()))
}

/// Opens listed budget `token` in a tab (its own tab, if it is open
/// already), and lists it in Open Recent.
#[tauri::command]
pub fn start_open<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    safety: State<'_, Safety>,
    start: State<'_, StartState>,
    token: u64,
) -> Result<View, String> {
    let path = start.listed(token)?;
    if !is_budget_path(&path) {
        return Err(crate::i18n::tr("start.not_a_budget_file"));
    }
    let opened = crate::safety_app::open_path(&state, &safety, &path);
    crate::safety_app::refresh_menu(window.app_handle());
    let id = opened?;
    Ok(finish(&window, &mut state.lock_doc(Some(id))?, None))
}

/// Pins listed budget `token`, or unpins it; the lists again.
#[tauri::command]
pub async fn start_pin(
    state: State<'_, AppState>,
    safety: State<'_, Safety>,
    start: State<'_, StartState>,
    token: u64,
    pinned: bool,
) -> Result<StartView, String> {
    let path = start.listed(token)?;
    start.set_pinned(&path, pinned)?;
    Ok(list(&state, &start, &safety.recent_paths()))
}

/// Takes listed budget `token` off the recent list and unpins it (the file
/// is not touched); the lists again.
#[tauri::command]
pub async fn start_forget<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    safety: State<'_, Safety>,
    start: State<'_, StartState>,
    token: u64,
) -> Result<StartView, String> {
    let path = start.listed(token)?;
    safety.forget_recent(&path);
    start.set_pinned(&path, false)?;
    crate::safety_app::refresh_menu(&app);
    Ok(list(&state, &start, &safety.recent_paths()))
}

/// Shows listed budget `token` in Finder, Explorer or the file manager (a
/// listed project folder is opened itself).
#[tauri::command]
pub fn start_reveal(start: State<'_, StartState>, token: u64) -> Result<(), String> {
    reveal(&start.listed(token)?)
}

/// Duplicates listed budget file `token` beside it ("Big Show 2.mthd"),
/// never over a file, and opens the duplicate in front as a saved budget.
#[tauri::command]
pub fn start_duplicate<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    safety: State<'_, Safety>,
    start: State<'_, StartState>,
    token: u64,
) -> Result<Created, String> {
    let path = start.listed(token)?;
    let (id, to) = duplicate(&state, &safety, &path)?;
    crate::safety_app::refresh_menu(window.app_handle());
    let name = to
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(Created {
        view: Some(finish(&window, &mut state.lock_doc(Some(id))?, None)),
        message: format!(
            "Duplicated as {name}, saved in {}.",
            folder_text(to.parent(), home().as_deref())
        ),
        taken: None,
    })
}

/// Shows a folder dialog and lists the folder picked with the projects;
/// the lists again (`None` when cancelled).
#[tauri::command]
pub async fn start_add_project<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    safety: State<'_, Safety>,
    start: State<'_, StartState>,
) -> Result<Option<StartView>, String> {
    let picked = window
        .dialog()
        .file()
        .set_title(crate::i18n::tr("dialog.add_a_project_folder"))
        .set_parent(&window)
        .blocking_pick_folder();
    let Some(picked) = picked else {
        return Ok(None);
    };
    let dir = picked.into_path().map_err(|e| e.to_string())?;
    start.add_project(&dir)?;
    Ok(Some(list(&state, &start, &safety.recent_paths())))
}

/// Takes listed project folder `token` off the projects (the folder and
/// what is in it are not touched); the lists again.
#[tauri::command]
pub async fn start_remove_project(
    state: State<'_, AppState>,
    safety: State<'_, Safety>,
    start: State<'_, StartState>,
    token: u64,
) -> Result<StartView, String> {
    let dir = start.listed(token)?;
    start.remove_project(&dir)?;
    Ok(list(&state, &start, &safety.recent_paths()))
}

/// Listed project folder `token` as New Budget's Save In (New Budget Here).
#[tauri::command]
pub fn start_project_folder(
    start: State<'_, StartState>,
    token: u64,
) -> Result<FolderChoice, String> {
    let dir = start.listed(token)?;
    if !dir.is_dir() {
        return Err(crate::i18n::tr("start.cant_find_folder"));
    }
    let folder = folder_text(Some(&dir), home().as_deref());
    let token = start.keep_folder(dir);
    Ok(FolderChoice { token, folder })
}

/// What `new_budget_pick_template` picked.
#[derive(Debug, Serialize)]
pub struct PickedTemplate {
    /// Names it to `new_budget_create`.
    pub token: u64,
    /// The file's name and the budget's title.
    pub file: String,
    pub title: String,
    pub categories: usize,
    pub accounts: usize,
}

/// Shows an Open dialog for a budget file to start a new budget from (read
/// from its tab when it is open). `None` when cancelled.
#[tauri::command]
pub async fn new_budget_pick_template<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    start: State<'_, StartState>,
) -> Result<Option<PickedTemplate>, String> {
    let from = state
        .lock()
        .path()
        .and_then(|p| p.parent())
        .map(PathBuf::from);
    let Some(path) = crate::app::pick_budget(
        &window,
        &crate::i18n::tr("dialog.start_from_a_budget_file"),
        from,
    )?
    else {
        return Ok(None);
    };
    let b = crate::templates::read(&state, &path)?;
    let picked = PickedTemplate {
        token: 0,
        file: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        title: b.title.trim().to_string(),
        categories: b.categories().count(),
        accounts: b.categories().map(|c| c.accounts.len()).sum(),
    };
    let token = start.keep_picked(b);
    Ok(Some(PickedTemplate { token, ..picked }))
}

/// A folder to save a new budget in, as the page sees it.
#[derive(Debug, Serialize)]
pub struct FolderChoice {
    /// Names it to `new_budget_create`.
    pub token: u64,
    /// The folder, the home folder written `~`.
    pub folder: String,
}

/// The folder New Budget offers to save in: the one the last new budget
/// went to, else Documents, else the home folder. `None` when none is there.
#[tauri::command]
pub fn new_budget_folder<R: Runtime>(
    app: AppHandle<R>,
    start: State<'_, StartState>,
) -> Option<FolderChoice> {
    let dir = default_folder(start.last_folder(), app.path().document_dir().ok(), home())?;
    let folder = folder_text(Some(&dir), home().as_deref());
    let token = start.keep_folder(dir);
    Some(FolderChoice { token, folder })
}

/// Shows a folder dialog (from the folder `current` names, when given) for
/// New Budget's Save In. `None` when cancelled.
#[tauri::command]
pub async fn new_budget_pick_folder<R: Runtime>(
    window: WebviewWindow<R>,
    start: State<'_, StartState>,
    current: Option<u64>,
) -> Result<Option<FolderChoice>, String> {
    let mut dialog = window
        .dialog()
        .file()
        .set_title(crate::i18n::tr("dialog.save_the_new_budget_in"))
        .set_parent(&window);
    if let Some(dir) = current.and_then(|t| start.folder(t).ok()) {
        dialog = dialog.set_directory(dir);
    }
    let Some(picked) = dialog.blocking_pick_folder() else {
        return Ok(None);
    };
    let dir = picked.into_path().map_err(|e| e.to_string())?;
    let folder = folder_text(Some(&dir), home().as_deref());
    let token = start.keep_folder(dir);
    Ok(Some(FolderChoice { token, folder }))
}

/// What New Budget's setup step offers: the library and the Packs list.
#[tauri::command]
pub fn new_budget_setup(
    state: State<'_, AppState>,
    library: State<'_, Library>,
    packs: State<'_, Packs>,
) -> SetupChoices {
    setup_choices(&state.docs(), &library, &packs)
}

/// Every bundled chart of accounts, with its group, description and size.
#[tauri::command]
pub async fn new_budget_charts() -> Result<Vec<ChartChoice>, String> {
    Ok(chart_choices().to_vec())
}

/// What `new_budget_create` made.
#[derive(Debug, Serialize)]
pub struct Created {
    /// The new budget in front (`None` when nothing was made: `taken`).
    pub view: Option<View>,
    pub message: String,
    /// A file of the name asked for is in the folder already, so nothing was
    /// made: a free name to offer (its stem, like "Big Show 2").
    pub taken: Option<String>,
}

/// Makes the budget New Budget's form describes and opens it in front (in
/// place of an untouched Untitled budget alone in the window, else in a new
/// tab). With a folder, it is saved there first, never over another file,
/// and opens as a saved budget listed in Open Recent; without, it opens with
/// no file, as one undo step.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn new_budget_create<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    safety: State<'_, Safety>,
    start: State<'_, StartState>,
    library: State<'_, Library>,
    packs: State<'_, Packs>,
    schedules: State<'_, crate::schedule_days::ScheduleSources>,
    spec: NewBudgetSpec,
) -> Result<Created, String> {
    let template = if spec.start.trim() == "file" {
        Some(start.picked(spec.template)?)
    } else {
        None
    };
    let folder = spec.folder.map(|t| start.folder(t)).transpose()?;
    let setup = ticked_setup(&state.docs(), &library, &packs, &spec)?;
    let schedule = spec
        .schedule
        .map(|t| {
            schedules
                .get(t)
                .map(|(_, days)| days)
                .map_err(|_| crate::i18n::tr("start.choose_schedule_again"))
        })
        .transpose()?;
    let (b, message) = build_with(&spec, template.as_ref(), &setup, schedule.as_deref())?;
    let Some(dir) = folder else {
        let id = open_new(&state, &safety, b)?;
        return Ok(Created {
            view: Some(finish(&window, &mut state.lock_doc(Some(id))?, None)),
            message,
            taken: None,
        });
    };
    let stem = file_stem_for(&spec.file_name, &b.title)?;
    let where_ = folder_text(Some(&dir), home().as_deref());
    let made = if spec.project {
        save_new_project(&state, &safety, &start, &b, &dir, &stem)?
    } else {
        save_new(&state, &safety, &b, &dir, &stem)?
    };
    match made {
        Made::Taken(free) => {
            Ok(Created {
                view: None,
                message: if spec.project {
                    format!("A folder or file named \"{stem}\" is already in {where_}. Pick another name.")
                } else {
                    format!(
                        "A file named \"{}\" is already in {where_}. Pick another name.",
                        file_name(&stem)
                    )
                },
                taken: Some(free),
            })
        }
        Made::Opened(id, _) => {
            // Best effort: the budget is saved either way.
            let _ = start.set_last_folder(&dir);
            crate::safety_app::refresh_menu(window.app_handle());
            Ok(Created {
                view: Some(finish(&window, &mut state.lock_doc(Some(id))?, None)),
                message: if spec.project {
                    format!(
                        "{}, saved as {} in a new project folder in {where_}.",
                        message.trim_end_matches('.'),
                        Path::new(&stem).join(file_name(&stem)).display()
                    )
                } else {
                    format!(
                        "{}, saved in {where_} as {}.",
                        message.trim_end_matches('.'),
                        file_name(&stem)
                    )
                },
                taken: None,
            })
        }
    }
}
