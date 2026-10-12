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
//!   undo step.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use method_core::{starter, Budget, Dec};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime, State, WebviewWindow};

use crate::app::{finish, AppState, DocId};
use crate::edit::{self, InfoInput};
use crate::safety::{self, Access};
use crate::safety_app::Safety;
use crate::session::Session;
use crate::view::View;

/// Most budgets kept pinned.
pub const PIN_LIMIT: usize = 100;
/// The pinned list's file, in the app's local data folder.
pub const START_FILE: &str = "start.json";
/// Longest title, and longest text field, the form takes.
const TITLE_MAX: usize = 200;
const TEXT_MAX: usize = 100;

// ---------------------------------------------------------------------------
// The app's state

#[derive(Default, Serialize, Deserialize)]
struct StartJson {
    #[serde(default)]
    pinned: Vec<String>,
}

/// What a file holds, for its row.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Summary {
    title: String,
    net_total: Option<i64>,
    currency: String,
}

/// A file's summary, with the modified time and size it was read at.
struct Cached {
    stamp: (Option<SystemTime>, u64),
    summary: Result<Summary, String>,
}

#[derive(Default)]
struct Inner {
    token: u64,
    /// The last listing: token -> file.
    listed: Vec<(u64, PathBuf)>,
    cache: HashMap<PathBuf, Cached>,
    /// The budget file last picked as New Budget's starting point.
    picked: Option<(u64, Budget)>,
}

/// The start window's state (one per app).
#[derive(Default)]
pub struct StartState {
    file: Option<PathBuf>,
    pinned: Mutex<Vec<PathBuf>>,
    inner: Mutex<Inner>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl StartState {
    /// The pinned list kept in `file` (none: nothing is kept between runs).
    /// A missing or unreadable file is an empty list.
    pub fn load(file: Option<PathBuf>) -> StartState {
        let pinned = file
            .as_deref()
            .and_then(|f| std::fs::read_to_string(f).ok())
            .and_then(|t| serde_json::from_str::<StartJson>(&t).ok())
            .unwrap_or_default()
            .pinned
            .into_iter()
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .take(PIN_LIMIT)
            .collect();
        StartState {
            file,
            pinned: Mutex::new(pinned),
            inner: Mutex::default(),
        }
    }

    pub fn pinned(&self) -> Vec<PathBuf> {
        lock(&self.pinned).clone()
    }

    /// Pins `path` (at the top) or unpins it.
    pub fn set_pinned(&self, path: &Path, on: bool) -> Result<(), String> {
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let mut list = lock(&self.pinned);
        list.retain(|p| *p != path);
        if on {
            if list.len() >= PIN_LIMIT {
                return Err(format!(
                    "At most {PIN_LIMIT} budgets can be pinned. Unpin one first."
                ));
            }
            list.insert(0, path);
        }
        self.save(&list)
    }

    fn save(&self, list: &[PathBuf]) -> Result<(), String> {
        let Some(file) = &self.file else {
            return Ok(());
        };
        if let Some(dir) = file.parent() {
            safety::create_private_dir(dir).map_err(|e| e.to_string())?;
        }
        let json = StartJson {
            pinned: list
                .iter()
                .filter_map(|p| p.to_str().map(str::to_string))
                .collect(),
        };
        let bytes = serde_json::to_vec_pretty(&json).map_err(|e| e.to_string())?;
        safety::write_atomically(file, &bytes, Access::Private)
            .map_err(|e| format!("Couldn't keep the pinned list: {e}"))
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
/// new token. Open budgets are summed from their tabs; files from the cache
/// or the disk.
fn list(state: &AppState, start: &StartState, recent: &[PathBuf]) -> StartView {
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
    g.cache.retain(|p, _| keep.contains(p));
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
/// says `file`, and a sentence saying what it holds.
pub fn build(spec: &NewBudgetSpec, template: Option<&Budget>) -> Result<(Budget, String), String> {
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
// Commands (registered in app.rs, build.rs and capabilities/main.json; only
// the main window may call them)

/// The pinned and recent budgets, each with a token for the other commands.
#[tauri::command]
pub async fn start_list(
    state: State<'_, AppState>,
    safety: State<'_, Safety>,
    start: State<'_, StartState>,
) -> Result<StartView, String> {
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

/// What `new_budget_create` made.
#[derive(Debug, Serialize)]
pub struct Created {
    pub view: View,
    pub message: String,
}

/// Makes the budget New Budget's form describes and opens it in front (in
/// place of an untouched Untitled budget alone in the window, else in a new
/// tab), with no file, as one undo step.
#[tauri::command]
pub fn new_budget_create<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    safety: State<'_, Safety>,
    start: State<'_, StartState>,
    spec: NewBudgetSpec,
) -> Result<Created, String> {
    let template = if spec.start.trim() == "file" {
        Some(start.picked(spec.template)?)
    } else {
        None
    };
    let (b, message) = build(&spec, template.as_ref())?;
    let id = open_new(&state, &safety, b)?;
    Ok(Created {
        view: finish(&window, &mut state.lock_doc(Some(id))?, None),
        message,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docs::Docs;
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
        let (b, msg) = build(&s, None).unwrap();
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
        let (b, msg) = build(&chart, None).unwrap();
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
        let (b, _) = build(&blank, None).unwrap();
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
        let (b, _) = build(&file, Some(&from)).unwrap();
        assert_eq!(b.title, "From file");
        assert_eq!(info(&b, "Director"), None, "a template's info is left out");
        assert_eq!(b.fringes, from.fringes, "its setup is kept");
        assert!(build(&file, None).is_err(), "the file has to be picked");
    }

    #[test]
    fn the_form_refuses_what_it_cant_use() {
        let bad = |s: NewBudgetSpec| build(&s, None).unwrap_err();
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
        let (b, _) = build(&spec("Big Show"), None).unwrap();
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
        let (b, _) = build(&spec("Pilot"), None).unwrap();
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
}
