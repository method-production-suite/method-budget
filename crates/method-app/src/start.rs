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

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use method_core::{starter, Budget, Dec};
use method_session::new_budget::{self, Ticked};
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
const CACHE_LIMIT: usize = 200;
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
}

/// What a file holds, for its row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Summary {
    title: String,
    net_total: Option<i64>,
    currency: String,
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
    #[serde(default)]
    files: Vec<CachedJson>,
}

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

/// The pinned list and the folder new budgets were last saved in.
#[derive(Default)]
struct Kept {
    pinned: Vec<Tracked>,
    last_folder: Option<PathBuf>,
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
        let Ok(bytes) = serde_json::to_vec_pretty(&CacheJson { files }) else {
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

/// One budget in the start window.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StartEntry {
    /// Names it to the other start commands, until the next listing.
    pub token: u64,
    /// The file's name.
    pub file: String,
    /// The budget's title (empty when it couldn't be read).
    pub title: String,
    /// The folder it is in, the home folder written `~`.
    pub folder: String,
    /// When the file last changed, in milliseconds since 1970 (UTC).
    pub modified: Option<i64>,
    /// The net total in cents, in `currency` (the base currency).
    pub net_total: Option<i64>,
    pub currency: String,
    pub pinned: bool,
    /// On the recent list.
    pub recent: bool,
    /// Open in a tab (its unsaved changes are in the total).
    pub open: bool,
    /// Why it can't be opened: the file is gone or isn't a readable budget.
    pub problem: Option<String>,
}

/// The start window's lists.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StartView {
    pub pinned: Vec<StartEntry>,
    /// The recent budgets that aren't pinned, newest first.
    pub recent: Vec<StartEntry>,
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

fn summary_of(b: &Budget, net_total: Option<i64>) -> Summary {
    Summary {
        title: b.title.trim().to_string(),
        net_total,
        currency: b.base_currency.clone(),
    }
}

/// Reads `path`'s summary (title, net total, currency).
fn read_summary(path: &Path) -> Result<Summary, String> {
    let (b, _) = crate::session::read_budget(path)?;
    let outcome = crate::diagnose::lenient_recalc(&b);
    Ok(summary_of(&b, outcome.report.as_ref().map(|r| r.net_total)))
}

/// Lists `pinned` then `recent` (without the pinned ones), giving each a
/// new token. Pinned files moved in the Finder are followed first. Open
/// budgets are summed from their tabs; files from the kept summaries (by
/// modified time and size) or the disk.
fn list(state: &AppState, start: &StartState, recent: &[PathBuf]) -> StartView {
    start.follow_pinned();
    let pinned = start.pinned();
    let home = home();
    // The budgets open in tabs, by file (their unsaved changes count).
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
            let total = s.outcome().report.as_ref().map(|r| r.net_total);
            let sum = summary_of(s.budget(), total);
            let key = std::fs::canonicalize(&p).unwrap_or(p);
            open.insert(key, sum);
        }
    }
    let mut guard = lock(&start.inner);
    let g: &mut Inner = &mut guard;
    g.listed.clear();
    let entry = |g: &mut Inner, path: &Path, is_pinned: bool, is_recent: bool| {
        g.token += 1;
        let token = g.token;
        g.listed.push((token, path.to_path_buf()));
        let meta = std::fs::metadata(path).ok().filter(|m| m.is_file());
        let modified = meta.as_ref().and_then(|m| m.modified().ok());
        let key = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let summary = match (open.get(&key), &meta) {
            (Some(s), _) => Ok(s.clone()),
            (None, None) => Err("Can't find this file.".to_string()),
            (None, Some(m)) => {
                let stamp = (modified, m.len());
                match g.cache.get(path) {
                    Some(c) if c.stamp == stamp => c.summary.clone(),
                    _ => {
                        let summary = read_summary(path);
                        g.cache_changed = true;
                        g.cache.insert(
                            path.to_path_buf(),
                            Cached {
                                stamp,
                                summary: summary.clone(),
                            },
                        );
                        summary
                    }
                }
            }
        };
        let (title, net_total, currency, problem) = match summary {
            Ok(s) => (s.title, s.net_total, s.currency, None),
            Err(e) => (String::new(), None, String::new(), Some(e)),
        };
        StartEntry {
            token,
            file: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            title,
            folder: folder_text(path.parent(), home.as_deref()),
            modified: modified_ms(modified),
            net_total,
            currency,
            pinned: is_pinned,
            recent: is_recent,
            open: open.contains_key(&key),
            problem,
        }
    };
    let pinned_rows: Vec<StartEntry> = pinned
        .iter()
        .map(|p| entry(g, p, true, recent.contains(p)))
        .collect();
    let recent_rows: Vec<StartEntry> = recent
        .iter()
        .filter(|p| !pinned.contains(p))
        .map(|p| entry(g, p, false, true))
        .collect();
    // Summaries of files no longer listed aren't needed again soon.
    let keep: Vec<PathBuf> = g.listed.iter().map(|(_, p)| p.clone()).collect();
    let before = g.cache.len();
    g.cache.retain(|p, _| keep.contains(p));
    g.cache_changed |= g.cache.len() != before;
    start.save_cache(g);
    StartView {
        pinned: pinned_rows,
        recent: recent_rows,
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
/// Windows), else its folder.
fn reveal_command(path: &Path) -> Result<std::process::Command, String> {
    use std::process::Command;
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
    let dir = path
        .parent()
        .filter(|d| d.is_dir())
        .ok_or("That folder isn't there any more.")?;
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
    (2..10_000)
        .map(|n| format!("{stem} {n}"))
        .find(|s| std::fs::symlink_metadata(dir.join(file_name(s))).is_err())
        .unwrap_or_else(|| format!("{stem} copy"))
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

/// Shows listed budget `token` in Finder, Explorer or the file manager.
#[tauri::command]
pub fn start_reveal(start: State<'_, StartState>, token: u64) -> Result<(), String> {
    reveal(&start.listed(token)?)
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
    spec: NewBudgetSpec,
) -> Result<Created, String> {
    let template = if spec.start.trim() == "file" {
        Some(start.picked(spec.template)?)
    } else {
        None
    };
    let folder = spec.folder.map(|t| start.folder(t)).transpose()?;
    let setup = ticked_setup(&state.docs(), &library, &packs, &spec)?;
    let (b, message) = build(&spec, template.as_ref(), &setup)?;
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
    match save_new(&state, &safety, &b, &dir, &stem)? {
        Made::Taken(free) => Ok(Created {
            view: None,
            message: format!(
                "A file named \"{}\" is already in {where_}. Pick another name.",
                file_name(&stem)
            ),
            taken: Some(free),
        }),
        Made::Opened(id, _) => {
            // Best effort: the budget is saved either way.
            let _ = start.set_last_folder(&dir);
            crate::safety_app::refresh_menu(window.app_handle());
            Ok(Created {
                view: Some(finish(&window, &mut state.lock_doc(Some(id))?, None)),
                message: format!(
                    "{}, saved in {where_} as {}.",
                    message.trim_end_matches('.'),
                    file_name(&stem)
                ),
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
        let home = PathBuf::from(format!("{sep}Users{sep}sam"));
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

    #[test]
    fn the_setup_step_is_wired_for_the_main_window() {
        assert!(include_str!("app.rs").contains("crate::start::new_budget_setup,"));
        assert!(include_str!("../build.rs").contains("\"new_budget_setup\","));
        assert!(include_str!("../capabilities/main.json").contains("\"allow-new-budget-setup\""));
    }
}
