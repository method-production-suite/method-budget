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

/// The summaries kept in `file` (none when missing or unreadable).
fn load_cache(file: Option<&Path>) -> HashMap<PathBuf, Cached> {
    file.and_then(|f| std::fs::read_to_string(f).ok())
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
            .and_then(|f| std::fs::read_to_string(f).ok())
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
            .ok_or_else(|| "The list changed since. Look at it again.".to_string())
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
            _ => Err("Choose the budget file to start from again.".into()),
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
            _ => Err("Choose the folder to save in again.".into()),
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
// The lists

/// One budget (or, in a project, a schedule or actuals file) in the start
/// window.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StartEntry {
    /// Names it to the other start commands, until the next listing.
    pub token: u64,
    /// The file's name (in a project: its path inside the project folder).
    pub file: String,
    /// The budget's title (empty when it couldn't be read; a schedule's or
    /// an actuals file's name).
    pub title: String,
    /// The folder it is in, the home folder written `~`.
    pub folder: String,
    /// When the file last changed, in milliseconds since 1970 (UTC).
    pub modified: Option<i64>,
    /// The net total in cents, in `currency` (the base currency).
    pub net_total: Option<i64>,
    pub currency: String,
    /// The thumbnail: a bar per category, by its share (empty: nothing to
    /// draw).
    pub bars: Vec<ThumbBar>,
    /// What it is, in a word: "budget", "sub-budget", "shared copy",
    /// "editable copy", "setup pack", "schedule" or "actuals".
    pub kind: String,
    /// Opens as a budget (a schedule or an actuals file is only shown).
    pub openable: bool,
    /// In a project: it came from the budget above it, and is shown under it.
    pub under: bool,
    /// Shared through a folder: a change log (`X.mthd-log/`) is beside it.
    pub shared: bool,
    /// For a shared budget: who was in it, latest first (their heartbeats).
    pub people: Vec<projects::Person>,
    pub pinned: bool,
    /// On the recent list.
    pub recent: bool,
    /// Open in a tab (its unsaved changes are in the total).
    pub open: bool,
    /// Why it can't be opened: the file is gone or isn't a readable budget.
    pub problem: Option<String>,
}

/// A project folder in the start window, with what is in it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ProjectView {
    /// Names the folder to the other start commands, until the next listing.
    pub token: u64,
    /// The folder's name.
    pub name: String,
    /// The folder, the home folder written `~`.
    pub folder: String,
    /// Its budgets, each with what came from it, then its other files.
    pub items: Vec<StartEntry>,
    /// The folder holds more than was looked at ([`SCAN_LIMIT`]).
    pub more: bool,
    /// Why it can't be read: the folder is gone.
    pub problem: Option<String>,
}

/// The start window's lists.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StartView {
    pub pinned: Vec<StartEntry>,
    /// The recent budgets that aren't pinned, newest first.
    pub recent: Vec<StartEntry>,
    /// The project folders, in the order they were added.
    pub projects: Vec<ProjectView>,
    /// Nothing is open but an untouched Untitled budget (the app has just
    /// started with nothing to reopen).
    pub fresh: bool,
}

fn modified_ms(t: Option<SystemTime>) -> Option<i64> {
    let d = t?.duration_since(UNIX_EPOCH).ok()?;
    i64::try_from(d.as_millis()).ok()
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .filter(|h| h.is_absolute())
}

/// `dir` to show: under the home folder written from `~`.
fn folder_text(dir: Option<&Path>, home: Option<&Path>) -> String {
    let Some(dir) = dir else {
        return String::new();
    };
    if let Some(rest) = home.and_then(|h| dir.strip_prefix(h).ok()) {
        let sep = std::path::MAIN_SEPARATOR;
        return if rest.as_os_str().is_empty() {
            "~".into()
        } else {
            format!("~{sep}{}", rest.display())
        };
    }
    dir.display().to_string()
}

/// The net total and thumbnail of a budget's totals (none when they
/// couldn't be worked out).
fn totals_of(report: Option<&method_core::Report>) -> (Option<i64>, Vec<ThumbBar>) {
    match report {
        Some(r) => (Some(r.net_total), projects::thumbnail(r)),
        None => (None, Vec::new()),
    }
}

fn summary_of(b: &Budget, (net_total, bars): (Option<i64>, Vec<ThumbBar>)) -> Summary {
    let (kind, source) = projects::kind_of(b);
    Summary {
        title: b.title.trim().to_string(),
        net_total,
        currency: b.base_currency.clone(),
        bars,
        kind,
        source,
    }
}

/// Reads `path`'s summary (title, net total, currency, thumbnail, kind).
fn read_summary(path: &Path) -> Result<Summary, String> {
    let (b, _) = crate::session::read_budget(path)?;
    let outcome = crate::diagnose::lenient_recalc(&b);
    Ok(summary_of(&b, totals_of(outcome.report.as_ref())))
}

/// Whether `path` names a budget file (by its extension).
fn is_budget_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(method_format::EXTENSION))
}

/// One listing's work: tokens, the kept summaries and the open budgets.
struct Lister<'a> {
    g: &'a mut Inner,
    /// The budgets open in tabs, by file (their unsaved changes count).
    open: HashMap<PathBuf, Summary>,
    home: Option<PathBuf>,
}

impl Lister<'_> {
    fn token_for(&mut self, path: &Path) -> u64 {
        self.g.token += 1;
        let token = self.g.token;
        self.g.listed.push((token, path.to_path_buf()));
        token
    }

    /// `path`'s summary: from its tab when it is open, else the kept one
    /// while the file's modified time and size are the same, else read.
    fn summary(&mut self, path: &Path) -> (Result<Summary, String>, Option<SystemTime>, bool) {
        let meta = std::fs::metadata(path).ok().filter(|m| m.is_file());
        let modified = meta.as_ref().and_then(|m| m.modified().ok());
        let key = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        if let Some(s) = self.open.get(&key) {
            return (Ok(s.clone()), modified, true);
        }
        let Some(m) = meta else {
            return (Err("Can't find this file.".to_string()), modified, false);
        };
        let stamp = (modified, m.len());
        let summary = match self.g.cache.get(path) {
            Some(c) if c.stamp == stamp => c.summary.clone(),
            _ => {
                let summary = read_summary(path);
                self.g.cache_changed = true;
                self.g.cache.insert(
                    path.to_path_buf(),
                    Cached {
                        stamp,
                        summary: summary.clone(),
                    },
                );
                summary
            }
        };
        (summary, modified, false)
    }

    /// The row of budget file `path`, and its summary when it was read.
    fn budget(
        &mut self,
        path: &Path,
        is_pinned: bool,
        is_recent: bool,
    ) -> (StartEntry, Option<Summary>) {
        let token = self.token_for(path);
        let (summary, modified, open) = self.summary(path);
        let mut e = StartEntry {
            token,
            file: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            title: String::new(),
            folder: folder_text(path.parent(), self.home.as_deref()),
            modified: modified_ms(modified),
            net_total: None,
            currency: String::new(),
            bars: Vec::new(),
            kind: ItemKind::Budget.word().into(),
            openable: true,
            under: false,
            shared: false,
            people: Vec::new(),
            pinned: is_pinned,
            recent: is_recent,
            open,
            problem: None,
        };
        // A change log beside it: shared through a folder (read every
        // listing, as people come and go without the file changing).
        let log = method_format::log::LogDir::for_budget(path);
        if log.exists() {
            e.shared = true;
            e.people =
                projects::people(log.presences().into_values().map(|p| projects::Heartbeat {
                    name: p.name,
                    category: p.category,
                    account: p.account,
                    at: p.at,
                }));
        }
        match summary {
            Ok(s) => {
                e.title = s.title.clone();
                e.net_total = s.net_total;
                e.currency = s.currency.clone();
                e.bars = s.bars.clone();
                e.kind = s.kind.word().into();
                (e, Some(s))
            }
            Err(problem) => {
                e.problem = Some(problem);
                (e, None)
            }
        }
    }

    /// Project folder `dir`, with its budgets and other files arranged.
    fn project(&mut self, dir: &Path, pinned: &[PathBuf], recent: &[PathBuf]) -> ProjectView {
        let token = self.token_for(dir);
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| dir.display().to_string());
        let folder = folder_text(Some(dir), self.home.as_deref());
        if !dir.is_dir() {
            return ProjectView {
                token,
                name,
                folder,
                items: Vec::new(),
                more: false,
                problem: Some("Can't find this folder.".into()),
            };
        }
        let (files, more) = scan(dir);
        let mut rows = Vec::new();
        let mut found = Vec::new();
        for (path, rel) in files {
            let parts: Vec<&str> = rel.iter().map(String::as_str).collect();
            let rel_text = rel.join(std::path::MAIN_SEPARATOR_STR);
            let file_name = parts.last().copied().unwrap_or_default().to_string();
            if is_budget_path(&path) {
                let (mut e, summary) =
                    self.budget(&path, pinned.contains(&path), recent.contains(&path));
                e.file = rel_text.clone();
                let (kind, title, source) = match summary {
                    Some(s) => (s.kind, s.title.clone(), s.source.clone()),
                    None => (ItemKind::Budget, String::new(), None),
                };
                found.push(projects::Found {
                    kind,
                    title: if title.is_empty() { file_name } else { title },
                    source,
                    file: rel_text,
                });
                rows.push(e);
                continue;
            }
            let kind = if projects::is_actuals_file(&parts) {
                ItemKind::Actuals
            } else if is_schedule_days(&path) {
                ItemKind::Schedule
            } else {
                continue;
            };
            let modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
            let token = self.token_for(&path);
            rows.push(StartEntry {
                token,
                file: rel_text.clone(),
                title: file_name.clone(),
                folder: folder.clone(),
                modified: modified_ms(modified),
                net_total: None,
                currency: String::new(),
                bars: Vec::new(),
                kind: kind.word().into(),
                openable: false,
                under: false,
                shared: false,
                people: Vec::new(),
                pinned: false,
                recent: false,
                open: false,
                problem: None,
            });
            found.push(projects::Found {
                kind,
                title: file_name,
                source: None,
                file: rel_text,
            });
        }
        let items = projects::arrange(&found)
            .into_iter()
            .map(|p| StartEntry {
                under: p.under,
                ..rows[p.index].clone()
            })
            .collect();
        ProjectView {
            token,
            name,
            folder,
            items,
            more,
            problem: None,
        }
    }
}

/// The files in `dir` and in the folders directly in it, with their paths
/// inside `dir` as parts, by name; hidden files and folders and change-log
/// folders (`X.mthd-log`) are passed over. At most [`SCAN_LIMIT`] files and
/// folders are looked at: true when there were more.
fn scan(dir: &Path) -> (Vec<(PathBuf, Vec<String>)>, bool) {
    fn entries(dir: &Path) -> Vec<(String, PathBuf, bool)> {
        let mut out: Vec<(String, PathBuf, bool)> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.starts_with('.') {
                    return None;
                }
                let kind = e.file_type().ok()?;
                // Links are passed over: a project is what is in its folder.
                if kind.is_symlink() {
                    return None;
                }
                Some((name, e.path(), kind.is_dir()))
            })
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }
    let log_suffix = format!(".{}-log", method_format::EXTENSION);
    let mut seen = 0;
    let mut out = Vec::new();
    let mut more = false;
    'top: for (name, path, is_dir) in entries(dir) {
        seen += 1;
        if seen > SCAN_LIMIT {
            more = true;
            break;
        }
        if !is_dir {
            out.push((path, vec![name]));
            continue;
        }
        if name.ends_with(&log_suffix) {
            continue;
        }
        for (inner, ipath, idir) in entries(&path) {
            seen += 1;
            if seen > SCAN_LIMIT {
                more = true;
                break 'top;
            }
            if !idir {
                out.push((ipath, vec![name.clone(), inner]));
            }
        }
    }
    (out, more)
}

/// Whether `path` is a schedule's days file: a `.json` file of at most a
/// few megabytes whose start says so.
fn is_schedule_days(path: &Path) -> bool {
    use std::io::Read;
    let json = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("json"));
    if !json {
        return false;
    }
    let Ok(f) = std::fs::File::open(path) else {
        return false;
    };
    if f.metadata().map_or(true, |m| m.len() > 8 * 1024 * 1024) {
        return false;
    }
    let mut head = Vec::with_capacity(4096);
    if f.take(4096).read_to_end(&mut head).is_err() {
        return false;
    }
    projects::looks_like_schedule_days(&String::from_utf8_lossy(&head))
}

/// Lists `pinned` then `recent` (without the pinned ones), then each
/// project folder, giving each row a new token. Pinned files moved in the
/// Finder are followed first. Open budgets are summed from their tabs;
/// files from the kept summaries (by modified time and size) or the disk.
fn list(state: &AppState, start: &StartState, recent: &[PathBuf]) -> StartView {
    start.follow_pinned();
    let pinned = start.pinned();
    let project_dirs = start.projects();
    let mut open: HashMap<PathBuf, Summary> = HashMap::new();
    let fresh;
    {
        let mut docs = state.docs();
        fresh = docs.lone_untouched().is_some();
        for id in docs.ids() {
            let Some(s) = docs.get_mut(id) else { continue };
            let Some(p) = s.path().map(Path::to_path_buf) else {
                continue;
            };
            let totals = totals_of(s.outcome().report.as_ref());
            let sum = summary_of(s.budget(), totals);
            let key = std::fs::canonicalize(&p).unwrap_or(p);
            open.insert(key, sum);
        }
    }
    let mut guard = lock(&start.inner);
    guard.listed.clear();
    let mut l = Lister {
        g: &mut guard,
        open,
        home: home(),
    };
    let pinned_rows: Vec<StartEntry> = pinned
        .iter()
        .map(|p| l.budget(p, true, recent.contains(p)).0)
        .collect();
    let recent_rows: Vec<StartEntry> = recent
        .iter()
        .filter(|p| !pinned.contains(p))
        .map(|p| l.budget(p, false, true).0)
        .collect();
    let projects: Vec<ProjectView> = project_dirs
        .iter()
        .map(|d| l.project(d, &pinned, recent))
        .collect();
    let g: &mut Inner = &mut guard;
    // Summaries of files no longer listed aren't needed again soon.
    let keep: std::collections::HashSet<PathBuf> =
        g.listed.iter().map(|(_, p)| p.clone()).collect();
    let before = g.cache.len();
    g.cache.retain(|p, _| keep.contains(p));
    g.cache_changed |= g.cache.len() != before;
    start.save_cache(g);
    StartView {
        pinned: pinned_rows,
        recent: recent_rows,
        projects,
        fresh,
    }
}

/// Shows `path` in Finder, Explorer or the file manager (its folder when
/// the file is gone).
fn reveal(path: &Path) -> Result<(), String> {
    reveal_command(path)?
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Couldn't show {}: {e}", path.display()))
}

/// The system command that shows `path` selected in its folder (macOS and
/// Windows), else its folder; a folder (a project) is opened itself.
fn reveal_command(path: &Path) -> Result<std::process::Command, String> {
    use std::process::Command;
    let dir = if path.is_dir() {
        Some(path)
    } else {
        path.parent().filter(|d| d.is_dir())
    };
    if path.is_file() && cfg!(target_os = "macos") {
        let mut cmd = Command::new("open");
        cmd.arg("-R").arg(path);
        return Ok(cmd);
    }
    if path.is_file() && cfg!(windows) {
        let mut arg = std::ffi::OsString::from("/select,");
        arg.push(path.as_os_str());
        let mut cmd = Command::new("explorer");
        cmd.arg(arg);
        return Ok(cmd);
    }
    let dir = dir.ok_or("That folder isn't there any more.")?;
    let program = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    let mut cmd = Command::new(program);
    cmd.arg(dir);
    Ok(cmd)
}

// ---------------------------------------------------------------------------
// New Budget

/// The kinds of production the form offers.
const KINDS: &[(&str, &str)] = &[
    ("feature", "Feature"),
    ("tv_episode", "TV episode"),
    ("tv_season", "TV season"),
    ("commercial", "Commercial"),
    ("documentary", "Documentary"),
    ("other", "Other"),
];

/// New Budget's form. Every text is as typed; blank leaves a field out.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct NewBudgetSpec {
    pub title: String,
    /// `feature`, `tv_episode`, `tv_season`, `commercial`, `documentary` or
    /// `other`.
    pub kind: String,
    /// TV only.
    pub season: String,
    /// A TV episode's number.
    pub episode: String,
    /// A TV season's episode count.
    pub episodes: String,
    /// Dates, `YYYY-MM-DD`.
    pub prep_start: String,
    pub shoot_start: String,
    pub shoot_end: String,
    pub wrap_end: String,
    /// The schedule's length: globals `PREP_WEEKS`, `SHOOT_DAYS`,
    /// `WRAP_WEEKS`.
    pub prep_weeks: String,
    pub shoot_days: String,
    pub wrap_weeks: String,
    pub country: String,
    pub region: String,
    pub city: String,
    /// The base currency's code (blank: USD).
    pub currency: String,
    /// `starter`, `blank`, `chart` (with `chart`) or `file` (with
    /// `template`, from `new_budget_pick_template`).
    pub start: String,
    pub chart: String,
    pub template: Option<u64>,
    /// `budget`, `production_level`, `category` or `account` (blank: as the
    /// starting point has it).
    pub fringe_posting: String,
    /// `binary` or `exact` (blank: as the starting point has it).
    pub arithmetic: String,
    /// Save In: the folder's token (from `new_budget_folder` or
    /// `new_budget_pick_folder`). `None`: the budget opens with no file.
    pub folder: Option<u64>,
    /// The file's name, with or without `.mthd` (blank: from the title).
    pub file_name: String,
    /// Setup: bring in everything in the setup library.
    pub library: bool,
    /// Setup: the packs to bring in, by their tokens from
    /// `new_budget_setup`'s listing.
    pub packs: Vec<u64>,
    /// Save In: make a project folder named like the file in the folder,
    /// save the budget in it and list it with the projects.
    pub project: bool,
    /// Link a Schedule: the token of a schedule days file picked with
    /// `schedule_days_pick` (its shoot days and dates come in, linked).
    pub schedule: Option<u64>,
}

fn text(field: &str, value: &str, max: usize) -> Result<String, String> {
    let v = value.trim();
    if v.chars().count() > max {
        return Err(format!("{field} is too long: at most {max} characters."));
    }
    if v.chars().any(char::is_control) {
        return Err(format!("{field} has to fit on one line."));
    }
    Ok(v.to_string())
}

/// An edit function's refusal `e` as a sentence about `field`.
fn said(field: &str, e: &str) -> String {
    format!("{field}: {}.", e.trim().trim_end_matches('.'))
}

/// The budget the form describes, starting from `template` when the form
/// says `file`, with the `setup` ticked (the library, packs) brought in
/// before the form's own details, and a sentence saying what it holds.
pub fn build(
    spec: &NewBudgetSpec,
    template: Option<&Budget>,
    setup: &[Ticked],
) -> Result<(Budget, String), String> {
    build_with(spec, template, setup, None)
}

/// [`build`], with a linked schedule's shoot days and dates
/// (`new_budget::link_schedule`) after the ticked setup and before the
/// form's own details, so what is typed in the form wins.
pub fn build_with(
    spec: &NewBudgetSpec,
    template: Option<&Budget>,
    setup: &[Ticked],
    schedule: Option<&method_format::import::schedule::ScheduleDays>,
) -> Result<(Budget, String), String> {
    let title = text("The title", &spec.title, TITLE_MAX)?;
    if title.is_empty() {
        return Err("Give the budget a title.".into());
    }
    let kind = KINDS
        .iter()
        .find(|(k, _)| *k == spec.kind.trim())
        .map(|(_, label)| *label)
        .ok_or_else(|| format!("Unknown production type {:?}.", spec.kind))?;
    let tv = kind.starts_with("TV");
    let currency = match text("The currency", &spec.currency, 10)?.to_uppercase() {
        c if c.is_empty() => "USD".to_string(),
        c => c,
    };

    let mut b = match spec.start.trim() {
        "starter" | "" => starter::budget(),
        "blank" => starter::blank(title.as_str(), currency.as_str()),
        "chart" => {
            let chart = method_format::charts::find(&spec.chart)
                .ok_or("That chart of accounts isn't bundled with this version.")?;
            let b = chart
                .budget()
                .map_err(|e| format!("The {} chart can't be read: {e}.", chart.name))?;
            crate::templates::strip(&b, false, &title)
        }
        "file" => crate::templates::strip(
            template.ok_or("Choose the budget file to start from again.")?,
            false,
            &title,
        ),
        other => return Err(format!("Unknown starting point {other:?}.")),
    };
    edit::set_title(&mut b, &title)?;
    if b.base_currency != currency {
        if b.currencies.contains_key(&currency) {
            return Err(format!(
                "The budget you start from lists {currency} as a foreign currency, so it can't be the base currency here. Pick another currency, or start from something else."
            ));
        }
        edit::set_base_currency(&mut b, &currency).map_err(|e| said("Currency", &e))?;
    }

    // The ticked setup, before the details typed here, so those win (a
    // library's SHOOT_DAYS gives way to the Shoot days typed).
    let brought = new_budget::bring_in(&mut b, setup)?;
    let linked = schedule
        .map(|days| new_budget::link_schedule(&mut b, days))
        .transpose()?;

    // Budget info, in plain words.
    let mut info = |name: &str, kind: &str, value: &str| -> Result<(), String> {
        let value = text(name, value, TEXT_MAX)?;
        if value.is_empty() {
            return Ok(());
        }
        let input = InfoInput {
            kind: kind.into(),
            value,
        };
        edit::set_info(&mut b, name, &input).map_err(|e| said(name, &e))
    };
    info("Production type", "text", kind)?;
    if tv {
        info("Season", "text", &spec.season)?;
    }
    if kind == "TV episode" {
        info("Episode", "text", &spec.episode)?;
    }
    if kind == "TV season" && !spec.episodes.trim().is_empty() {
        let n: u32 = spec
            .episodes
            .trim()
            .parse()
            .ok()
            .filter(|n| (1..=999).contains(n))
            .ok_or("Episodes is a whole number from 1 to 999.")?;
        info("Episodes", "number", &n.to_string())?;
    }
    for (name, value) in [
        ("Prep start", &spec.prep_start),
        ("Shoot start", &spec.shoot_start),
        ("Shoot end", &spec.shoot_end),
        ("Wrap end", &spec.wrap_end),
    ] {
        info(name, "date", value)?;
    }
    for (name, value) in [
        ("Country", &spec.country),
        ("Region", &spec.region),
        ("City", &spec.city),
    ] {
        info(name, "text", value)?;
    }

    // The schedule's length, as the starter's globals.
    for (label, global, value) in [
        ("Prep weeks", "PREP_WEEKS", &spec.prep_weeks),
        ("Shoot days", "SHOOT_DAYS", &spec.shoot_days),
        ("Wrap weeks", "WRAP_WEEKS", &spec.wrap_weeks),
    ] {
        let v = value.trim();
        if v.is_empty() {
            continue;
        }
        let n: Dec = v
            .replace(',', "")
            .parse()
            .map_err(|_| format!("{label} is a number, like 4 or 2.5."))?;
        if n < Dec::ZERO {
            return Err(format!("{label} can't be below 0."));
        }
        edit::set_global(&mut b, global, &n.to_string()).map_err(|e| said(label, &e))?;
    }

    if !spec.fringe_posting.trim().is_empty() {
        edit::set_fringe_posting(&mut b, spec.fringe_posting.trim(), None)
            .map_err(|e| said("Fringe posting", &e))?;
    }
    if !spec.arithmetic.trim().is_empty() {
        edit::set_arithmetic(&mut b, &spec.arithmetic).map_err(|e| said("Arithmetic", &e))?;
    }

    let categories = b.categories().count();
    let accounts: usize = b.categories().map(|c| c.accounts.len()).sum();
    let lines: usize = b
        .categories()
        .flat_map(|c| &c.accounts)
        .map(|a| a.details.len())
        .sum();
    let n = |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    let message = if lines > 0 {
        format!(
            "New budget \"{title}\" with {}, {} and {}.",
            n(categories, "category", "categories"),
            n(accounts, "account", "accounts"),
            n(lines, "line", "lines")
        )
    } else {
        format!(
            "New budget \"{title}\" with {} and {}.",
            n(categories, "category", "categories"),
            n(accounts, "account", "accounts")
        )
    };
    let message = match brought.sentence() {
        Some(more) => format!("{message} {more}"),
        None => message,
    };
    let message = match linked {
        Some(more) => format!("{message} {more}"),
        None => message,
    };
    Ok((b, message))
}

/// Opens `b` in front, with no file, as one undo step ("New budget"): in
/// the place of an untouched Untitled budget alone in the window, else in a
/// new tab. Its id.
pub fn open_new(state: &AppState, safety: &Safety, b: Budget) -> Result<DocId, String> {
    let mut s = Session::with_budget(
        starter::blank(b.title.as_str(), b.base_currency.as_str()),
        None,
    );
    s.edit("New budget", |x| {
        *x = b;
        Ok(())
    })?;
    let instead = state.docs().lone_untouched();
    crate::safety_app::put_in_front(state, safety, s, instead)
}

// ---------------------------------------------------------------------------
// Charts of accounts to start from

/// A bundled chart of accounts, as New Budget lists it.
#[derive(Clone, Debug, Serialize)]
pub struct ChartChoice {
    pub id: &'static str,
    pub name: &'static str,
    /// `Feature`, `Television`, `Commercial` or `Other`.
    pub group: &'static str,
    pub description: &'static str,
    /// Our own chart, not one that follows a studio's or network's
    /// (those are numbers, names and levels only, decision 51).
    pub ours: bool,
    pub categories: usize,
    pub accounts: usize,
}

/// Every bundled chart with its size, read once per run.
pub fn chart_choices() -> &'static [ChartChoice] {
    static CHOICES: OnceLock<Vec<ChartChoice>> = OnceLock::new();
    CHOICES.get_or_init(|| {
        method_format::charts::all()
            .iter()
            .map(|c| {
                let (categories, accounts) = c
                    .budget()
                    .map(|b| {
                        (
                            b.categories().count(),
                            b.categories().map(|c| c.accounts.len()).sum(),
                        )
                    })
                    .unwrap_or((0, 0));
                ChartChoice {
                    id: c.id,
                    name: c.name,
                    group: c.group,
                    description: c.description,
                    // Our charts are described as ours by the manifest
                    // (templates/charts/charts.json); the studios' as
                    // following one.
                    ours: c.description.starts_with("Our own"),
                    categories,
                    accounts,
                }
            })
            .collect()
    })
}

// ---------------------------------------------------------------------------
// Setup: the library and setup packs to bring in

/// The library, as New Budget's setup step offers it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LibraryChoice {
    /// Its file name without the extension.
    pub name: String,
    /// How many setup items it holds.
    pub items: usize,
    /// Why it can't be read (moved, deleted, not a budget).
    pub problem: Option<String>,
}

/// What New Budget's setup step offers.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct SetupChoices {
    /// The setup library (`None`: no library picked yet).
    pub library: Option<LibraryChoice>,
    /// The Packs list (the user's Packs folder, then the bundled packs),
    /// each named by its token.
    pub packs: Vec<ListedPack>,
}

/// How many setup items `b` holds that New Budget brings (no settings).
fn setup_items(b: &Budget) -> usize {
    crate::import_setup::all_items(b)
        .into_iter()
        .filter(|(w, _)| !w.is_setting())
        .count()
}

/// The library and the packs to offer, listing the packs again (their
/// tokens are the Packs list's).
pub fn setup_choices(docs: &Docs, library: &Library, packs: &Packs) -> SetupChoices {
    let library = library.path().map(|path| {
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        match crate::library::read(docs, &path) {
            Ok((b, _)) => LibraryChoice {
                name,
                items: setup_items(&b),
                problem: None,
            },
            Err(e) => LibraryChoice {
                name,
                items: 0,
                problem: Some(e),
            },
        }
    });
    SetupChoices {
        library,
        packs: crate::packs::list(packs).packs,
    }
}

/// The setup the form ticked, read: the library first (from its tab when it
/// is open), then the packs in the order listed.
pub fn ticked_setup(
    docs: &Docs,
    library: &Library,
    packs: &Packs,
    spec: &NewBudgetSpec,
) -> Result<Vec<Ticked>, String> {
    let mut out = Vec::new();
    if spec.library {
        let path = library.path().ok_or(
            "There's no setup library yet. Untick it, or make one in the Library tab first.",
        )?;
        let (budget, _) = crate::library::read(docs, &path)
            .map_err(|e| format!("Your library can't be read: {}.", e.trim_end_matches('.')))?;
        out.push(Ticked {
            label: "your library".into(),
            budget,
        });
    }
    let mut seen = Vec::new();
    for &token in &spec.packs {
        if seen.contains(&token) {
            continue;
        }
        seen.push(token);
        let path = packs.listed(token)?;
        let (budget, _) = crate::session::read_budget(&path)?;
        let label = method_core::pack::info(&budget)
            .map(|i| i.title.trim().to_string())
            .filter(|t| !t.is_empty())
            .or_else(|| path.file_stem().map(|s| s.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "a pack".into());
        out.push(Ticked { label, budget });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Save In

/// The folder New Budget offers first: the one the last new budget was
/// saved in, else Documents, else the home folder (each only while it is a
/// folder).
fn default_folder(
    last: Option<PathBuf>,
    documents: Option<PathBuf>,
    home: Option<PathBuf>,
) -> Option<PathBuf> {
    [last, documents, home]
        .into_iter()
        .flatten()
        .find(|d| d.is_dir())
}

/// The file name's stem from what was typed (blank: from `title`), without
/// the budget extension. Refused when it can't be a file's name.
pub fn file_stem_for(typed: &str, title: &str) -> Result<String, String> {
    let typed = typed.trim();
    let ext = format!(".{}", method_format::EXTENSION);
    let typed = match typed.len().checked_sub(ext.len()) {
        Some(at) if typed.is_char_boundary(at) && typed[at..].eq_ignore_ascii_case(&ext) => {
            typed[..at].trim()
        }
        _ => typed,
    };
    if typed.is_empty() {
        return Ok(safe_file_stem(title));
    }
    if typed.chars().count() > TITLE_MAX {
        return Err(format!(
            "The file name is too long: at most {TITLE_MAX} characters."
        ));
    }
    if typed
        .chars()
        .any(|c| c.is_control() || "/\\:*?\"<>|".contains(c))
    {
        return Err("A file name can't have / \\ : * ? \" < > or |.".into());
    }
    if typed.starts_with('.') {
        return Err("A file name can't start with a dot.".into());
    }
    Ok(typed.to_string())
}

/// `stem` with the budget extension.
fn file_name(stem: &str) -> String {
    format!("{stem}.{}", method_format::EXTENSION)
}

/// The first of "`stem` 2", "`stem` 3"… with no file of that name in `dir`.
fn free_stem(dir: &Path, stem: &str) -> String {
    free_name(stem, |s| dir.join(file_name(s)))
}

/// The first of "`stem` 2", "`stem` 3"… for which nothing is at `at(name)`.
fn free_name(stem: &str, at: impl Fn(&str) -> PathBuf) -> String {
    (2..10_000)
        .map(|n| format!("{stem} {n}"))
        .find(|s| std::fs::symlink_metadata(at(s)).is_err())
        .unwrap_or_else(|| format!("{stem} {}", unique_suffix()))
}

/// A number no other free name took, for when 2 to 9,999 are all taken.
fn unique_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos())
}

/// What saving a new budget into a folder came to.
#[derive(Debug, PartialEq, Eq)]
pub enum Made {
    /// Written and opened in front, as a saved budget.
    Opened(DocId, PathBuf),
    /// A file of that name is there already; nothing was written. A free
    /// name to offer instead.
    Taken(String),
}

/// Writes `b` as `stem`.mthd in `dir`, never over a file that is there, and
/// opens it in front as a saved budget (in place of an untouched Untitled
/// budget alone in the window), listed in Open Recent.
pub fn save_new(
    state: &AppState,
    safety: &Safety,
    b: &Budget,
    dir: &Path,
    stem: &str,
) -> Result<Made, String> {
    if !dir.is_dir() {
        return Err("That folder isn't there any more. Choose another one.".into());
    }
    let path = dir.join(file_name(stem));
    let text = method_format::to_string(b);
    match safety::write_new(&path, text.as_bytes()) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            return Ok(Made::Taken(free_stem(dir, stem)));
        }
        Err(e) => {
            return Err(format!(
                "Couldn't save in {}: {e}.",
                folder_text(Some(dir), home().as_deref())
            ))
        }
    }
    let id = crate::safety_app::open_path(state, safety, &path)?;
    Ok(Made::Opened(id, path))
}

/// Makes project folder `stem` in `dir`, writes `b` in it as `stem`.mthd
/// and opens it as [`save_new`] does; the folder joins the projects. A
/// folder or file of that name already in `dir` makes nothing: a name free
/// for both is offered.
pub fn save_new_project(
    state: &AppState,
    safety: &Safety,
    start: &StartState,
    b: &Budget,
    dir: &Path,
    stem: &str,
) -> Result<Made, String> {
    if !dir.is_dir() {
        return Err("That folder isn't there any more. Choose another one.".into());
    }
    let free = || {
        free_name(stem, |s| {
            // Both the folder and a loose file of the name must be free.
            let folder = dir.join(s);
            if std::fs::symlink_metadata(dir.join(file_name(s))).is_ok() {
                dir.join(file_name(s))
            } else {
                folder
            }
        })
    };
    let project = dir.join(stem);
    if std::fs::symlink_metadata(&project).is_ok()
        || std::fs::symlink_metadata(dir.join(file_name(stem))).is_ok()
    {
        return Ok(Made::Taken(free()));
    }
    match std::fs::create_dir(&project) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            return Ok(Made::Taken(free()));
        }
        Err(e) => {
            return Err(format!(
                "Couldn't make the folder {stem} in {}: {e}.",
                folder_text(Some(dir), home().as_deref())
            ))
        }
    }
    let made = save_new(state, safety, b, &project, stem);
    if !matches!(made, Ok(Made::Opened(..))) {
        // Nothing was written in it: take the empty folder back.
        let _ = std::fs::remove_dir(&project);
        return made;
    }
    // Best effort: the budget is saved either way.
    let _ = start.add_project(&project);
    made
}

/// Writes `b` in `dir` as `stem`.mthd, or the first free "`stem` 2",
/// "`stem` 3"… when that is taken (never over a file), and opens it as
/// [`save_new`] does. Its id and path.
pub fn save_new_free(
    state: &AppState,
    safety: &Safety,
    b: &Budget,
    dir: &Path,
    stem: &str,
) -> Result<(DocId, PathBuf), String> {
    let mut name = stem.to_string();
    for _ in 0..5 {
        match save_new(state, safety, b, dir, &name)? {
            Made::Opened(id, path) => return Ok((id, path)),
            Made::Taken(free) => name = free,
        }
    }
    Err("Couldn't find a free name for the file. Try again.".into())
}

/// Writes a duplicate of budget file `path` beside it, as the first free
/// "`stem` 2", "`stem` 3"… (the file's bytes as they are on disk, so an open
/// tab's unsaved changes are not in it), never over a file, and opens it in
/// front as a saved budget listed in Open Recent. Its path.
pub fn duplicate(
    state: &AppState,
    safety: &Safety,
    path: &Path,
) -> Result<(DocId, PathBuf), String> {
    if !is_budget_path(path) {
        return Err("Only a budget file can be duplicated here.".into());
    }
    // Refuse what isn't a readable budget before copying it.
    crate::session::read_budget(path)?;
    let bytes =
        std::fs::read(path).map_err(|e| format!("Couldn't read {}: {e}.", path.display()))?;
    let dir = path
        .parent()
        .filter(|d| d.is_dir())
        .ok_or("That folder isn't there any more.")?;
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Budget".into());
    // Another program may take the free name first: try the next.
    for _ in 0..5 {
        let to = dir.join(file_name(&free_stem(dir, &stem)));
        match safety::write_new(&to, &bytes) {
            Ok(()) => {
                let id = crate::safety_app::open_path(state, safety, &to)?;
                return Ok((id, to));
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                return Err(format!(
                    "Couldn't save the duplicate in {}: {e}.",
                    folder_text(Some(dir), home().as_deref())
                ))
            }
        }
    }
    Err("Couldn't find a free name for the duplicate. Try again.".into())
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
        return Err("That isn't a budget file. Show in Folder shows it.".into());
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
        .set_title("Add a project folder")
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
        return Err("Can't find this folder.".into());
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
    let Some(path) = crate::app::pick_budget(&window, "Start from a budget file", from)? else {
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
        .set_title("Save the new budget in")
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
                .map_err(|_| "Choose the schedule to link again.".to_string())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::SessionFiles;
    use method_core::{FringePosting, InfoValue};

    fn spec(title: &str) -> NewBudgetSpec {
        NewBudgetSpec {
            title: title.into(),
            kind: "feature".into(),
            ..NewBudgetSpec::default()
        }
    }

    fn info(b: &Budget, name: &str) -> Option<InfoValue> {
        b.info.get(name).cloned()
    }

    #[test]
    fn the_form_fills_title_info_globals_currency_and_settings() {
        let s = NewBudgetSpec {
            title: "  Big Show ".into(),
            kind: "tv_season".into(),
            season: "2".into(),
            episode: "ignored for a season".into(),
            episodes: "10".into(),
            prep_start: "2027-01-04".into(),
            shoot_start: "2027-02-01".into(),
            shoot_end: "".into(),
            wrap_end: "2027-04-30".into(),
            prep_weeks: "6".into(),
            shoot_days: "1,200".into(),
            wrap_weeks: "2.5".into(),
            country: "Canada".into(),
            region: "British Columbia".into(),
            city: "Vancouver".into(),
            currency: "cad".into(),
            start: "starter".into(),
            fringe_posting: "category".into(),
            arithmetic: "exact".into(),
            ..NewBudgetSpec::default()
        };
        let (b, msg) = build(&s, None, &[]).unwrap();
        assert_eq!(b.title, "Big Show");
        assert_eq!(b.base_currency, "CAD");
        assert_eq!(
            info(&b, "Production type"),
            Some(InfoValue::Text("TV season".into()))
        );
        assert_eq!(info(&b, "Season"), Some(InfoValue::Text("2".into())));
        assert_eq!(info(&b, "Episode"), None, "a season has no episode number");
        assert_eq!(
            info(&b, "Episodes"),
            Some(InfoValue::Number(Dec::from_int(10)))
        );
        assert_eq!(
            info(&b, "Prep start"),
            Some(InfoValue::Date("2027-01-04".into()))
        );
        assert_eq!(info(&b, "Shoot end"), None, "blank is left out");
        assert_eq!(info(&b, "City"), Some(InfoValue::Text("Vancouver".into())));
        assert_eq!(b.globals.get("PREP_WEEKS").map(|f| f.0.as_str()), Some("6"));
        assert_eq!(
            b.globals.get("SHOOT_DAYS").map(|f| f.0.as_str()),
            Some("1200")
        );
        assert_eq!(
            b.globals.get("WRAP_WEEKS").map(|f| f.0.as_str()),
            Some("2.5")
        );
        // The starter's own formula follows the new shoot days.
        assert_eq!(
            b.globals.get("SHOOT_WEEKS").map(|f| f.0.as_str()),
            Some("SHOOT_DAYS / 5")
        );
        assert!(matches!(b.fringe_posting, FringePosting::Category { .. }));
        assert_eq!(b.arithmetic, method_core::Arithmetic::Exact);
        assert!(msg.starts_with("New budget \"Big Show\" with "), "{msg}");
        assert!(msg.contains("lines"), "the starter has lines: {msg}");
        // It recalculates with the new globals.
        assert!(method_core::recalc(&b, &Default::default()).is_ok());
    }

    #[test]
    fn a_chart_an_empty_budget_and_a_file_start_without_lines() {
        let chart = NewBudgetSpec {
            start: "chart".into(),
            chart: "feature".into(),
            ..spec("Chart")
        };
        let (b, msg) = build(&chart, None, &[]).unwrap();
        assert!(b.categories().count() > 5);
        assert!(b
            .categories()
            .flat_map(|c| &c.accounts)
            .all(|a| a.details.is_empty()));
        assert_eq!(b.title, "Chart");
        assert!(!msg.contains("line"), "{msg}");
        // A type alone never adds globals.
        assert!(!b.globals.contains_key("SHOOT_DAYS"));

        let blank = NewBudgetSpec {
            start: "blank".into(),
            currency: "EUR".into(),
            ..spec("Empty")
        };
        let (b, _) = build(&blank, None, &[]).unwrap();
        assert_eq!(b.categories().count(), 0);
        assert_eq!(b.base_currency, "EUR");
        assert_eq!(
            b.fringe_posting,
            FringePosting::Budget,
            "new-budget settings"
        );

        let mut from = starter::budget();
        from.title = "Old show".into();
        from.info
            .insert("Director".into(), InfoValue::Text("Someone".into()));
        let file = NewBudgetSpec {
            start: "file".into(),
            ..spec("From file")
        };
        let (b, _) = build(&file, Some(&from), &[]).unwrap();
        assert_eq!(b.title, "From file");
        assert_eq!(info(&b, "Director"), None, "a template's info is left out");
        assert_eq!(b.fringes, from.fringes, "its setup is kept");
        assert!(
            build(&file, None, &[]).is_err(),
            "the file has to be picked"
        );
    }

    #[test]
    fn the_form_refuses_what_it_cant_use() {
        let bad = |s: NewBudgetSpec| build(&s, None, &[]).unwrap_err();
        assert_eq!(bad(spec("  ")), "Give the budget a title.");
        assert!(bad(NewBudgetSpec {
            kind: "opera".into(),
            ..spec("X")
        })
        .contains("production type"));
        let date = bad(NewBudgetSpec {
            shoot_start: "2027-02-30".into(),
            ..spec("X")
        });
        assert!(date.starts_with("Shoot start: "), "{date}");
        assert!(date.ends_with('.') && !date.ends_with(".."), "{date}");
        assert!(bad(NewBudgetSpec {
            shoot_days: "lots".into(),
            ..spec("X")
        })
        .starts_with("Shoot days is a number"));
        assert!(bad(NewBudgetSpec {
            prep_weeks: "-1".into(),
            ..spec("X")
        })
        .contains("below 0"));
        assert!(bad(NewBudgetSpec {
            kind: "tv_season".into(),
            episodes: "0".into(),
            ..spec("X")
        })
        .starts_with("Episodes"));
        assert!(bad(NewBudgetSpec {
            start: "chart".into(),
            chart: "no-such-chart".into(),
            ..spec("X")
        })
        .contains("isn't bundled"));
        assert!(bad(NewBudgetSpec {
            fringe_posting: "sideways".into(),
            ..spec("X")
        })
        .starts_with("Fringe posting"));
        assert!(bad(NewBudgetSpec {
            city: "a\nb".into(),
            ..spec("X")
        })
        .contains("one line"));
        // A currency the starting point lists as foreign.
        let mut from = starter::budget();
        from.currencies.insert(
            "GBP".into(),
            method_core::Currency {
                description: "Pounds".into(),
                symbol: String::new(),
                rate: Dec::from_int(1),
                format: Default::default(),
            },
        );
        let e = build(
            &NewBudgetSpec {
                start: "file".into(),
                currency: "GBP".into(),
                ..spec("X")
            },
            Some(&from),
            &[],
        )
        .unwrap_err();
        assert!(e.contains("foreign currency"), "{e}");
    }

    #[test]
    fn a_new_budget_takes_the_place_of_an_untouched_untitled_one() {
        let dir = safety::tests::temp_dir("start-new");
        let safety = Safety::new(Some(&dir));
        let state = AppState::with(Docs::default());
        let first = state.docs().active();
        let (b, _) = build(&spec("Big Show"), None, &[]).unwrap();
        let id = open_new(&state, &safety, b).unwrap();
        {
            let docs = state.docs();
            assert_eq!(docs.len(), 1, "the untouched Untitled gave its tab up");
            assert_ne!(id, first);
            let s = docs.get(id).unwrap();
            assert!(s.path().is_none(), "no file: the first Save asks where");
            assert!(s.is_dirty(), "closing it asks to save");
            assert_eq!(s.budget().title, "Big Show");
        }
        // With a budget in use, the next one gets its own tab.
        let (b, _) = build(&spec("Pilot"), None, &[]).unwrap();
        let second = open_new(&state, &safety, b).unwrap();
        let docs = state.docs();
        assert_eq!(docs.len(), 2);
        assert_eq!(docs.active(), second, "in front");
        drop(docs);
        // One undo step takes the form's work back.
        let mut s = state.lock_doc(Some(second)).unwrap();
        assert!(s.undo());
        assert_eq!(s.budget().categories().count(), 0);
        drop(s);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_lists_show_pinned_then_recent_with_totals_and_problems() {
        let dir = safety::tests::temp_dir("start-list");
        let a = dir.join("a.mthd");
        let b = dir.join("b.mthd");
        let gone = dir.join("gone.mthd");
        let junk = dir.join("junk.mthd");
        let mut s = Session::with_budget(starter::budget(), None);
        s.edit("Test edit", |x| edit::set_title(x, "Alpha"))
            .unwrap();
        s.save_to(&a).unwrap();
        Session::with_budget(starter::blank("Beta", "EUR"), None)
            .save_to(&b)
            .unwrap();
        std::fs::write(&junk, "not a budget").unwrap();
        let canon = |p: &Path| std::fs::canonicalize(p).unwrap();
        let (a, b, junk) = (canon(&a), canon(&b), canon(&junk));
        let gone = canon(&dir).join(gone.file_name().unwrap());

        let start = StartState::load(Some(dir.join(START_FILE)));
        start.set_pinned(&b, true).unwrap();
        let state = AppState::with(Docs::default());
        let recent = vec![a.clone(), b.clone(), gone.clone(), junk.clone()];
        let v = list(&state, &start, &recent);
        assert!(v.fresh, "only an untouched Untitled budget is open");
        assert_eq!(v.pinned.len(), 1);
        assert_eq!(v.pinned[0].title, "Beta");
        assert_eq!(v.pinned[0].currency, "EUR");
        assert_eq!(v.pinned[0].net_total, Some(0));
        assert!(v.pinned[0].pinned && v.pinned[0].recent);
        let files: Vec<&str> = v.recent.iter().map(|e| e.file.as_str()).collect();
        assert_eq!(
            files,
            ["a.mthd", "gone.mthd", "junk.mthd"],
            "pinned ones once"
        );
        let alpha = &v.recent[0];
        assert_eq!(alpha.title, "Alpha");
        let want = crate::diagnose::lenient_recalc(&starter::budget())
            .report
            .unwrap()
            .net_total;
        assert_eq!(alpha.net_total, Some(want));
        assert!(alpha.modified.is_some());
        assert!(!alpha.open);
        assert_eq!(
            v.recent[1].problem.as_deref(),
            Some("Can't find this file.")
        );
        assert!(v.recent[2].problem.is_some(), "not a budget");

        // Tokens name the rows of the last listing only.
        assert_eq!(start.listed(alpha.token).unwrap(), a);
        let again = list(&state, &start, &recent);
        assert!(start.listed(alpha.token).is_err());
        assert_eq!(start.listed(again.recent[0].token).unwrap(), a);

        // A budget open in a tab is summed from the tab, unsaved changes too.
        let safety = Safety::new(Some(&dir));
        let id = crate::safety_app::open_path(&state, &safety, &a).unwrap();
        state
            .lock_doc(Some(id))
            .unwrap()
            .edit("Test edit", |x| edit::set_title(x, "Alpha (edited)"))
            .unwrap();
        let v = list(&state, &start, &recent);
        assert!(!v.fresh, "a file is open");
        assert!(v.recent[0].open);
        assert_eq!(v.recent[0].title, "Alpha (edited)");

        // Remove takes it off the recent list; the file stays.
        assert_eq!(safety.recent_paths(), std::slice::from_ref(&a));
        safety.forget_recent(&a);
        assert!(safety.recent_paths().is_empty());
        assert!(a.is_file());

        // The pinned list is kept, at the top, and unpinned again.
        start.set_pinned(&a, true).unwrap();
        let kept = StartState::load(Some(dir.join(START_FILE)));
        assert_eq!(kept.pinned(), [a.clone(), b.clone()]);
        kept.set_pinned(&b, false).unwrap();
        assert_eq!(StartState::load(Some(dir.join(START_FILE))).pinned(), [a]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn folders_under_home_are_written_from_tilde() {
        let sep = std::path::MAIN_SEPARATOR;
        let home = PathBuf::from(format!("{sep}Users{sep}riley"));
        let under = home.join("Films").join("Big Show");
        assert_eq!(
            folder_text(Some(&under), Some(&home)),
            format!("~{sep}Films{sep}Big Show")
        );
        assert_eq!(folder_text(Some(&home), Some(&home)), "~");
        let other = PathBuf::from(format!("{sep}Volumes{sep}Shared"));
        assert_eq!(
            folder_text(Some(&other), Some(&home)),
            other.display().to_string()
        );
        assert_eq!(folder_text(None, Some(&home)), "");
    }

    #[test]
    fn a_picked_template_is_named_by_its_token() {
        let start = StartState::load(None);
        assert!(start.picked(Some(1)).is_err());
        let t = start.keep_picked(starter::budget());
        assert!(start.picked(Some(t)).is_ok());
        assert!(start.picked(Some(t + 1)).is_err());
        assert!(start.picked(None).is_err());
        let newer = start.keep_picked(starter::blank("B", "USD"));
        assert!(start.picked(Some(t)).is_err(), "only the last one is kept");
        assert_eq!(start.picked(Some(newer)).unwrap().title, "B");
    }

    #[test]
    fn every_bundled_chart_is_listed_with_its_size() {
        let charts = chart_choices();
        assert_eq!(charts.len(), method_format::charts::all().len());
        assert!(charts.iter().all(|c| c.accounts > 0 && c.categories > 0));
        assert!(charts
            .iter()
            .all(|c| method_format::charts::GROUPS.contains(&c.group)));
        // The charts each production type starts from are our own.
        for id in ["feature", "tv-one-hour", "commercial", "documentary"] {
            let c = charts.iter().find(|c| c.id == id).unwrap();
            assert!(c.ours, "{id}");
        }
        assert!(charts.iter().any(|c| !c.ours), "studio charts too");
    }

    #[test]
    fn file_names_come_from_the_title_or_what_was_typed() {
        assert_eq!(file_stem_for("", "Big Show").unwrap(), "Big Show");
        assert_eq!(file_stem_for("  ", "A/B: C").unwrap(), "A B  C");
        assert_eq!(file_stem_for("Pilot v3.MTHD", "X").unwrap(), "Pilot v3");
        assert_eq!(file_stem_for("Pilot.mthd ", "X").unwrap(), "Pilot");
        assert!(file_stem_for("a/b", "X")
            .unwrap_err()
            .contains("can't have"));
        assert!(file_stem_for(".hidden", "X").unwrap_err().contains("dot"));
        assert!(file_stem_for(&"x".repeat(201), "X").is_err());
    }

    #[test]
    fn save_in_falls_back_from_the_last_folder_to_documents_to_home() {
        let dir = safety::tests::temp_dir("start-folder");
        let gone = dir.join("gone");
        let docs = dir.join("Documents");
        std::fs::create_dir(&docs).unwrap();
        assert_eq!(
            default_folder(Some(dir.clone()), Some(docs.clone()), None),
            Some(dir.clone())
        );
        assert_eq!(
            default_folder(Some(gone.clone()), Some(docs.clone()), Some(dir.clone())),
            Some(docs)
        );
        assert_eq!(
            default_folder(Some(gone.clone()), None, Some(dir.clone())),
            Some(dir.clone())
        );
        assert_eq!(default_folder(Some(gone), None, None), None);
        // The folder is named by its token, the last one given only.
        let start = StartState::load(None);
        let t = start.keep_folder(dir.clone());
        assert_eq!(start.folder(t).unwrap(), dir);
        assert!(start.folder(t + 1).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn create_saves_into_the_folder_and_never_over_a_file() {
        let dir = std::fs::canonicalize(safety::tests::temp_dir("start-save")).unwrap();
        let safety = Safety::new(Some(&dir.join("data")));
        let state = AppState::with(Docs::default());
        let (b, _) = build(&spec("Big Show"), None, &[]).unwrap();
        let made = save_new(&state, &safety, &b, &dir, "Big Show").unwrap();
        let path = dir.join("Big Show.mthd");
        let Made::Opened(id, at) = made else {
            panic!("not saved: {made:?}")
        };
        assert_eq!(at, path);
        {
            let docs = state.docs();
            assert_eq!(docs.len(), 1, "in place of the untouched Untitled");
            let s = docs.get(id).unwrap();
            assert_eq!(s.path(), Some(path.as_path()));
            assert!(!s.is_dirty(), "a saved budget");
            assert_eq!(s.budget().title, "Big Show");
        }
        assert_eq!(safety.recent_paths(), std::slice::from_ref(&path));
        let text = std::fs::read_to_string(&path).unwrap();

        // The same name again: nothing written, a free name offered.
        let (other, _) = build(&spec("Other"), None, &[]).unwrap();
        assert_eq!(
            save_new(&state, &safety, &other, &dir, "Big Show").unwrap(),
            Made::Taken("Big Show 2".into())
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text, "untouched");
        assert_eq!(state.docs().len(), 1);
        std::fs::write(dir.join("Big Show 2.mthd"), "x").unwrap();
        assert_eq!(free_stem(&dir, "Big Show"), "Big Show 3");
        // No temporary files are left behind.
        let names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".tmp"))
            .collect();
        assert!(names.is_empty(), "{names:?}");
        // A folder that has gone is refused.
        assert!(save_new(&state, &safety, &other, &dir.join("gone"), "X").is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn summaries_are_kept_between_runs_until_the_file_changes() {
        let dir = std::fs::canonicalize(safety::tests::temp_dir("start-cache")).unwrap();
        let a = dir.join("a.mthd");
        Session::with_budget(starter::blank("Alpha", "USD"), None)
            .save_to(&a)
            .unwrap();
        let state = AppState::with(Docs::default());
        let start = StartState::load(Some(dir.join(START_FILE)));
        let v = list(&state, &start, std::slice::from_ref(&a));
        assert_eq!(v.recent[0].title, "Alpha");
        assert!(dir.join(CACHE_FILE).is_file(), "kept for the next start");

        // The next run takes the kept summary without reading the file:
        // a kept title that differs from the file's shows it was not read.
        let kept = std::fs::read_to_string(dir.join(CACHE_FILE)).unwrap();
        std::fs::write(
            dir.join(CACHE_FILE),
            kept.replace("\"Alpha\"", "\"From the cache\""),
        )
        .unwrap();
        let start = StartState::load(Some(dir.join(START_FILE)));
        let v = list(&state, &start, std::slice::from_ref(&a));
        assert_eq!(v.recent[0].title, "From the cache");

        // A changed file (another size) is read again.
        Session::with_budget(starter::blank("Alpha, longer title", "USD"), None)
            .save_to(&a)
            .unwrap();
        let v = list(&state, &start, std::slice::from_ref(&a));
        assert_eq!(v.recent[0].title, "Alpha, longer title");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_pinned_file_moved_in_the_finder_is_followed() {
        let dir = std::fs::canonicalize(safety::tests::temp_dir("start-moved")).unwrap();
        let a = dir.join("a.mthd");
        Session::with_budget(starter::blank("Alpha", "USD"), None)
            .save_to(&a)
            .unwrap();
        let start = StartState::load(Some(dir.join(START_FILE)));
        start.set_pinned(&a, true).unwrap();
        let b = dir.join("renamed.mthd");
        std::fs::rename(&a, &b).unwrap();
        let state = AppState::with(Docs::default());
        let v = list(&state, &start, &[]);
        if cfg!(target_os = "macos") {
            assert_eq!(v.pinned[0].file, "renamed.mthd");
            assert_eq!(v.pinned[0].title, "Alpha");
            // The new place is kept.
            let again = StartState::load(Some(dir.join(START_FILE)));
            assert_eq!(again.pinned(), [b]);
        } else {
            assert_eq!(
                v.pinned[0].problem.as_deref(),
                Some("Can't find this file.")
            );
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_last_tab_closes_into_an_untouched_untitled_budget() {
        let dir = safety::tests::temp_dir("start-last");
        let safety = Safety::new(Some(&dir));
        let state = AppState::with(Docs::default());
        let id = state.docs().active();
        state
            .lock_doc(Some(id))
            .unwrap()
            .edit("Test edit", |b| edit::set_title(b, "Changed"))
            .unwrap();
        use crate::safety_app::{close_last_for_start, Closed};
        assert_eq!(
            close_last_for_start(&state, &safety, id, false).unwrap(),
            Closed::Unsaved
        );
        assert_eq!(
            close_last_for_start(&state, &safety, id, true).unwrap(),
            Closed::Closed
        );
        let docs = state.docs();
        assert_eq!(docs.len(), 1);
        assert!(docs.lone_untouched().is_some(), "as at launch");
        assert!(docs.get(id).is_none(), "the old budget is gone");
        drop(docs);
        // With other tabs open, it is an ordinary close.
        let second = crate::safety_app::new_tab(&state);
        assert_eq!(
            close_last_for_start(&state, &safety, second, false).unwrap(),
            Closed::Closed
        );
        assert_eq!(state.docs().len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A library with a global the starter has too and one it hasn't.
    fn a_library() -> Budget {
        let mut lib = crate::library::new_library("USD");
        lib.globals.insert("SHOOT_DAYS".into(), "40".into());
        lib.globals.insert("KIT_WEEKS".into(), "10".into());
        lib
    }

    #[test]
    fn ticked_setup_comes_in_before_what_the_form_says() {
        let s = NewBudgetSpec {
            shoot_days: "30".into(),
            ..spec("Big Show")
        };
        let ticked = [Ticked {
            label: "your library".into(),
            budget: a_library(),
        }];
        let (b, msg) = build(&s, None, &ticked).unwrap();
        assert_eq!(b.globals["KIT_WEEKS"].0, "10");
        assert_eq!(b.globals["SHOOT_DAYS"].0, "30", "the form wins");
        assert!(
            msg.ends_with(" Setup from your library: 1 item added, 1 item replaced."),
            "{msg}"
        );
        // Settings typed in the form are untouched by the library.
        let (b, _) = build(
            &NewBudgetSpec {
                fringe_posting: "account".into(),
                ..spec("Big Show")
            },
            None,
            &ticked,
        )
        .unwrap();
        assert_eq!(b.fringe_posting, FringePosting::Account);
    }

    #[test]
    fn the_setup_step_lists_the_library_and_packs_and_reads_what_is_ticked() {
        let dir = std::fs::canonicalize(safety::tests::temp_dir("start-setup")).unwrap();
        let docs = Docs::default();
        let library = Library::new(Some(&dir.join("data")));
        let packs = Packs::new(Some(dir.join("Packs")), None);
        let none = NewBudgetSpec {
            library: true,
            ..spec("X")
        };
        // No library yet: none offered, and ticking it is refused.
        let c = setup_choices(&docs, &library, &packs);
        assert_eq!(c.library, None);
        assert!(c.packs.is_empty());
        assert!(ticked_setup(&docs, &library, &packs, &none)
            .unwrap_err()
            .contains("no setup library"));

        // A library and a pack.
        let lib_path = dir.join(crate::library::LIBRARY_FILE_NAME);
        crate::library::write(&lib_path, &a_library()).unwrap();
        library.set_path(lib_path.clone()).unwrap();
        let mut items = crate::library::new_library("USD");
        items.globals.insert("PENSION_RATE".into(), "5".into());
        let picks = vec![crate::import_setup::ItemRef {
            kind: "global".into(),
            name: "PENSION_RATE".into(),
        }];
        let details = method_format::setup::pack::PackDetails {
            title: "Union Basics".into(),
            ..Default::default()
        };
        let (pack, _) = crate::packs::pack_build(&items, &picks, &details, "2026-10-18").unwrap();
        crate::library::write(&dir.join("Packs").join("union.mthd"), &pack).unwrap();

        let c = setup_choices(&docs, &library, &packs);
        let lib = c.library.unwrap();
        assert_eq!(lib.name, "My Method Budget Library");
        assert_eq!(lib.items, 2);
        assert_eq!(lib.problem, None);
        assert_eq!(c.packs.len(), 1);
        assert_eq!(c.packs[0].title, "Union Basics");

        let s = NewBudgetSpec {
            library: true,
            packs: vec![c.packs[0].token, c.packs[0].token],
            ..spec("Big Show")
        };
        let t = ticked_setup(&docs, &library, &packs, &s).unwrap();
        let labels: Vec<&str> = t.iter().map(|t| t.label.as_str()).collect();
        assert_eq!(
            labels,
            ["your library", "Union Basics"],
            "each once, library first"
        );
        let (b, msg) = build(&s, None, &t).unwrap();
        assert_eq!(b.globals["PENSION_RATE"].0, "5");
        assert!(
            msg.contains("Setup from your library and Union Basics"),
            "{msg}"
        );

        // A token from an older listing is refused.
        setup_choices(&docs, &library, &packs);
        assert!(ticked_setup(&docs, &library, &packs, &s).is_err());

        // A library that has gone says so.
        std::fs::remove_file(&lib_path).unwrap();
        let c = setup_choices(&docs, &library, &packs);
        assert!(c.library.unwrap().problem.is_some());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The names of the files and folders in `dir`, sorted.
    fn names_in(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    #[test]
    fn new_budget_can_make_a_project_folder_with_the_budget_in_it() {
        let dir = std::fs::canonicalize(safety::tests::temp_dir("start-project")).unwrap();
        let data = dir.join("data");
        let safety = Safety::new(Some(&data));
        let start = StartState::load(Some(data.join(START_FILE)));
        let state = AppState::with(Docs::default());
        let (b, _) = build(&spec("Big Show"), None, &[]).unwrap();
        let made = save_new_project(&state, &safety, &start, &b, &dir, "Big Show").unwrap();
        let project = dir.join("Big Show");
        let path = project.join("Big Show.mthd");
        assert_eq!(made, Made::Opened(state.docs().active(), path.clone()));
        assert_eq!(
            names_in(&project),
            ["Big Show.mthd"],
            "the budget, nothing else"
        );
        {
            let docs = state.docs();
            let s = docs.get(docs.active()).unwrap();
            assert_eq!(s.path(), Some(path.as_path()));
            assert!(!s.is_dirty(), "a saved budget");
        }
        // The folder is listed with the projects, and kept between runs.
        assert_eq!(start.projects(), std::slice::from_ref(&project));
        let again = StartState::load(Some(data.join(START_FILE)));
        assert_eq!(again.projects(), std::slice::from_ref(&project));

        // The same name again makes nothing and offers a free one, also
        // when only a loose file of that name is there.
        let (other, _) = build(&spec("Other"), None, &[]).unwrap();
        assert_eq!(
            save_new_project(&state, &safety, &start, &other, &dir, "Big Show").unwrap(),
            Made::Taken("Big Show 2".into())
        );
        std::fs::write(dir.join("Pilot.mthd"), "x").unwrap();
        std::fs::create_dir(dir.join("Pilot 2")).unwrap();
        assert_eq!(
            save_new_project(&state, &safety, &start, &other, &dir, "Pilot").unwrap(),
            Made::Taken("Pilot 3".into())
        );
        assert_eq!(names_in(&project), ["Big Show.mthd"], "untouched");
        assert_eq!(start.projects().len(), 1);
        // A folder that has gone is refused, and nothing is left behind.
        assert!(save_new_project(&state, &safety, &start, &other, &dir.join("gone"), "X").is_err());
        assert!(!dir.join("gone").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_duplicate_is_a_new_file_beside_it_never_over_one() {
        let dir = std::fs::canonicalize(safety::tests::temp_dir("start-dup")).unwrap();
        let safety = Safety::new(Some(&dir.join("data")));
        let state = AppState::with(Docs::default());
        let a = dir.join("Big Show.mthd");
        let mut s = Session::with_budget(starter::budget(), None);
        s.edit("Test edit", |x| edit::set_title(x, "Big Show"))
            .unwrap();
        s.save_to(&a).unwrap();
        let text = std::fs::read(&a).unwrap();
        // "Big Show 2" is taken by something else: it is left alone.
        std::fs::write(dir.join("Big Show 2.mthd"), "someone else's").unwrap();

        let (id, to) = duplicate(&state, &safety, &a).unwrap();
        assert_eq!(to, dir.join("Big Show 3.mthd"));
        assert_eq!(std::fs::read(&to).unwrap(), text, "the same budget");
        assert_eq!(std::fs::read(&a).unwrap(), text, "the original untouched");
        assert_eq!(
            std::fs::read_to_string(dir.join("Big Show 2.mthd")).unwrap(),
            "someone else's"
        );
        {
            let docs = state.docs();
            let s = docs.get(id).unwrap();
            assert_eq!(docs.active(), id, "in front");
            assert_eq!(s.path(), Some(to.as_path()));
            assert!(!s.is_dirty(), "opened as a saved budget");
            assert_eq!(s.budget().title, "Big Show");
        }
        assert!(safety.recent_paths().contains(&to), "listed in Open Recent");
        // Again: the next free name.
        let (_, next) = duplicate(&state, &safety, &a).unwrap();
        assert_eq!(next, dir.join("Big Show 4.mthd"));
        // Only a readable budget file is duplicated.
        std::fs::write(dir.join("junk.mthd"), "not a budget").unwrap();
        assert!(duplicate(&state, &safety, &dir.join("junk.mthd")).is_err());
        std::fs::write(dir.join("notes.txt"), "x").unwrap();
        assert!(duplicate(&state, &safety, &dir.join("notes.txt")).is_err());
        assert!(duplicate(&state, &safety, &dir.join("gone.mthd")).is_err());
        let names = names_in(&dir);
        assert!(!names.iter().any(|n| n.ends_with(".tmp")), "{names:?}");
        assert!(!names.contains(&"junk 2.mthd".to_string()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn save_new_free_takes_the_next_free_name() {
        let dir = std::fs::canonicalize(safety::tests::temp_dir("start-free")).unwrap();
        let safety = Safety::new(Some(&dir.join("data")));
        let state = AppState::with(Docs::default());
        std::fs::write(dir.join("Big Show - Camera.mthd"), "kept").unwrap();
        let (b, _) = build(&spec("Big Show - Camera"), None, &[]).unwrap();
        let (_, path) = save_new_free(&state, &safety, &b, &dir, "Big Show - Camera").unwrap();
        assert_eq!(path, dir.join("Big Show - Camera 2.mthd"));
        assert_eq!(
            std::fs::read_to_string(dir.join("Big Show - Camera.mthd")).unwrap(),
            "kept"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn thumbnails_are_kept_with_the_summary_until_the_file_changes() {
        let dir = std::fs::canonicalize(safety::tests::temp_dir("start-thumb")).unwrap();
        let a = dir.join("a.mthd");
        Session::with_budget(starter::budget(), None)
            .save_to(&a)
            .unwrap();
        let state = AppState::with(Docs::default());
        let start = StartState::load(Some(dir.join(START_FILE)));
        let v = list(&state, &start, std::slice::from_ref(&a));
        let report = crate::diagnose::lenient_recalc(&starter::budget())
            .report
            .unwrap();
        let bars = projects::thumbnail(&report);
        assert!(!bars.is_empty());
        assert_eq!(v.recent[0].bars, bars, "drawn from the topsheet's totals");
        assert_eq!(v.recent[0].kind, "budget");
        // Kept in the app's data folder, keyed by the file, never in the file.
        let kept = std::fs::read_to_string(dir.join(CACHE_FILE)).unwrap();
        assert!(kept.contains("\"bars\""), "{kept}");
        assert!(kept.contains(&format!("\"version\": {CACHE_VERSION}")));
        assert!(!std::fs::read_to_string(&a).unwrap().contains("\"bars\""));

        // The next run takes the kept bars without reading the file: kept
        // bars that differ from the file's show it was not read.
        let marked = kept.replacen("\"10 Story and rights\"", "\"From the cache\"", 1);
        assert_ne!(marked, kept);
        std::fs::write(dir.join(CACHE_FILE), &marked).unwrap();
        let start = StartState::load(Some(dir.join(START_FILE)));
        let v = list(&state, &start, std::slice::from_ref(&a));
        assert_eq!(v.recent[0].bars[0].label, "From the cache");

        // A changed file is read again: one category now.
        let mut one = starter::budget();
        let keep_first = one.categories().next().unwrap().code.clone();
        one.topsheet.retain(|r| match r {
            method_core::TopsheetRow::Category(c) => c.code == keep_first,
            _ => false,
        });
        Session::with_budget(one, None).save_to(&a).unwrap();
        let v = list(&state, &start, std::slice::from_ref(&a));
        assert_eq!(v.recent[0].bars.len(), 1);
        assert_eq!(v.recent[0].bars[0].share, 1000);

        // A cache from before thumbnails (no version) is read as empty.
        std::fs::write(
            dir.join(CACHE_FILE),
            marked.replace(&format!("\"version\": {CACHE_VERSION},"), ""),
        )
        .unwrap();
        let start = StartState::load(Some(dir.join(START_FILE)));
        let v = list(&state, &start, std::slice::from_ref(&a));
        assert_eq!(v.recent[0].bars.len(), 1, "read from the file");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn projects_show_their_budgets_by_folder() {
        let dir = std::fs::canonicalize(safety::tests::temp_dir("start-projects")).unwrap();
        let data = dir.join("data");
        let project = dir.join("Big Show");
        std::fs::create_dir_all(project.join("actuals")).unwrap();
        std::fs::create_dir_all(project.join("Big Show.mthd-log")).unwrap();
        std::fs::create_dir_all(project.join("versions")).unwrap();
        let mut parent = starter::budget();
        parent.title = "Big Show".into();
        Session::with_budget(parent.clone(), None)
            .save_to(&project.join("Big Show.mthd"))
            .unwrap();
        let row = method_session::sharing::RowInput {
            kind: "category".into(),
            code: "21".into(),
        };
        let sub = method_session::sharing::make_row_sub_budget(
            &parent,
            &row,
            "2026-10-20T10:00:00-07:00",
        )
        .unwrap();
        Session::with_budget(sub, None)
            .save_to(&project.join("versions").join("Camera.mthd"))
            .unwrap();
        Session::with_budget(starter::blank("Pilot", "USD"), None)
            .save_to(&project.join("Pilot.mthd"))
            .unwrap();
        std::fs::write(
            project.join("Big Show.days.json"),
            "{\"format\": \"method-schedule-days\", \"version\": 1}",
        )
        .unwrap();
        std::fs::write(project.join("other.json"), "{\"format\": \"x\"}").unwrap();
        std::fs::write(project.join("actuals").join("2026-09 GL.csv"), "a,b").unwrap();
        std::fs::write(project.join("notes.txt"), "x").unwrap();
        std::fs::write(project.join(".hidden.mthd"), "x").unwrap();
        std::fs::write(project.join("Big Show.mthd-log").join("x.mthd"), "x").unwrap();

        let start = StartState::load(Some(data.join(START_FILE)));
        start.add_project(&project).unwrap();
        start.add_project(&project).unwrap();
        start.add_project(&dir.join("gone")).unwrap();
        let state = AppState::with(Docs::default());
        let v = list(&state, &start, &[]);
        assert_eq!(v.projects.len(), 2, "each folder once");
        let p = &v.projects[0];
        assert_eq!(p.name, "Big Show");
        assert!(p.problem.is_none() && !p.more);
        let sep = std::path::MAIN_SEPARATOR;
        let rows: Vec<(String, &str, bool, bool)> = p
            .items
            .iter()
            .map(|e| (e.file.clone(), e.kind.as_str(), e.under, e.openable))
            .collect();
        assert_eq!(
            rows,
            [
                ("Big Show.mthd".to_string(), "budget", false, true),
                (
                    format!("versions{sep}Camera.mthd"),
                    "sub-budget",
                    true,
                    true
                ),
                ("Pilot.mthd".to_string(), "budget", false, true),
                ("Big Show.days.json".to_string(), "schedule", false, false),
                (
                    format!("actuals{sep}2026-09 GL.csv"),
                    "actuals",
                    false,
                    false
                ),
            ]
        );
        assert_eq!(p.items[1].title, "Big Show - Camera");
        assert!(p.items[0].net_total.is_some() && !p.items[0].bars.is_empty());
        assert_eq!(
            v.projects[1].problem.as_deref(),
            Some("Can't find this folder.")
        );
        // Tokens name the folder and its files; only budgets open.
        assert_eq!(start.listed(p.token).unwrap(), project);
        assert_eq!(
            start.listed(p.items[0].token).unwrap(),
            project.join("Big Show.mthd")
        );
        assert!(is_budget_path(&start.listed(p.items[0].token).unwrap()));
        assert!(!is_budget_path(&start.listed(p.items[3].token).unwrap()));

        // Removing a project takes it off the list; the folder stays.
        start.remove_project(&project).unwrap();
        assert_eq!(start.projects(), [dir.join("gone")]);
        assert!(project.join("Big Show.mthd").is_file());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_big_project_folder_is_read_up_to_the_limit() {
        let dir = std::fs::canonicalize(safety::tests::temp_dir("start-big")).unwrap();
        for n in 0..SCAN_LIMIT + 5 {
            std::fs::write(dir.join(format!("{n:04}.txt")), "x").unwrap();
        }
        let (files, more) = scan(&dir);
        assert!(more);
        assert_eq!(files.len(), SCAN_LIMIT);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_project_folder_is_shown_itself() {
        let dir = std::fs::canonicalize(safety::tests::temp_dir("start-reveal")).unwrap();
        let cmd = reveal_command(&dir).unwrap();
        let args: Vec<&std::ffi::OsStr> = cmd.get_args().collect();
        assert_eq!(args, [dir.as_os_str()]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_shared_budget_says_so_with_who_was_in_it() {
        use method_core::log::ActorId;
        use method_format::log::{LogDir, Presence};
        let dir = std::fs::canonicalize(safety::tests::temp_dir("start-shared")).unwrap();
        let a = dir.join("Big Show.mthd");
        let b = dir.join("Pilot.mthd");
        for p in [&a, &b] {
            Session::with_budget(starter::budget(), None)
                .save_to(p)
                .unwrap();
        }
        let log = LogDir::for_budget(&a);
        std::fs::create_dir_all(log.root()).unwrap();
        std::fs::write(log.root().join("budget.json"), "{}").unwrap();
        for (n, name, acct, at) in [
            (1u8, "Ann", None, "2026-10-19T10:00:00Z"),
            (2, "Bo", Some("2101"), "2026-10-20T09:00:00Z"),
        ] {
            let p = Presence {
                name: name.into(),
                account: acct.map(str::to_string),
                at: at.into(),
                ..Presence::default()
            };
            log.write_presence(ActorId([n; 32]), &p).unwrap();
        }
        let state = AppState::with(Docs::default());
        let start = StartState::load(Some(dir.join(START_FILE)));
        let v = list(&state, &start, &[a.clone(), b.clone()]);
        assert!(v.recent[0].shared);
        let people: Vec<(&str, &str)> = v.recent[0]
            .people
            .iter()
            .map(|p| (p.name.as_str(), p.place.as_str()))
            .collect();
        assert_eq!(people, [("Bo", "account 2101"), ("Ann", "")]);
        assert!(!v.recent[1].shared && v.recent[1].people.is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_linked_schedule_comes_in_before_what_the_form_says() {
        let days = method_session::schedule_days::read(
            "every-field-days.json",
            include_bytes!("../../method-format/tests/fixtures/schedule/every-field-days.json"),
        )
        .unwrap();
        let (b, msg) = build_with(&spec("Harbor"), None, &[], Some(&days)).unwrap();
        assert_eq!(b.globals["SHOOT_DAYS"].0, "6");
        assert_eq!(
            info(&b, "Wrap end"),
            Some(InfoValue::Date("2027-03-26".into()))
        );
        assert!(msg.ends_with("linked for later imports."), "{msg}");
        // What is typed in the form wins.
        let typed = NewBudgetSpec {
            shoot_days: "30".into(),
            shoot_start: "2027-02-22".into(),
            ..spec("Harbor")
        };
        let (b, _) = build_with(&typed, None, &[], Some(&days)).unwrap();
        assert_eq!(b.globals["SHOOT_DAYS"].0, "30");
        assert_eq!(
            info(&b, "Shoot start"),
            Some(InfoValue::Date("2027-02-22".into()))
        );
        assert!(
            b.setup_sources.get("global", "SHOOT_DAYS").is_some(),
            "still linked"
        );
    }

    #[test]
    fn projects_duplicate_and_row_sub_budgets_are_wired_for_the_main_window() {
        for (cmd, perm) in [
            ("start_duplicate", "allow-start-duplicate"),
            ("start_add_project", "allow-start-add-project"),
            ("start_remove_project", "allow-start-remove-project"),
            ("start_project_folder", "allow-start-project-folder"),
            ("sub_budget_from_row", "allow-sub-budget-from-row"),
        ] {
            assert!(
                include_str!("app.rs").contains(&format!("::{cmd},")),
                "{cmd}"
            );
            assert!(
                include_str!("../build.rs").contains(&format!("\"{cmd}\",")),
                "{cmd}"
            );
            assert!(
                include_str!("../capabilities/main.json").contains(&format!("\"{perm}\"")),
                "{perm}"
            );
        }
    }

    #[test]
    fn the_setup_step_is_wired_for_the_main_window() {
        assert!(include_str!("app.rs").contains("crate::start::new_budget_setup,"));
        assert!(include_str!("../build.rs").contains("\"new_budget_setup\","));
        assert!(include_str!("../capabilities/main.json").contains("\"allow-new-budget-setup\""));
    }
}
