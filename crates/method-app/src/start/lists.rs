//! The start window's lists: pinned, recent and project folders.

use super::*;

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

pub(super) fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .filter(|h| h.is_absolute())
}

/// `dir` to show: under the home folder written from `~`.
pub(super) fn folder_text(dir: Option<&Path>, home: Option<&Path>) -> String {
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
pub(super) fn is_budget_path(path: &Path) -> bool {
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
            return (
                Err(crate::i18n::tr("start.cant_find_file")),
                modified,
                false,
            );
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
                problem: Some(crate::i18n::tr("start.cant_find_folder")),
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
pub(super) fn scan(dir: &Path) -> (Vec<(PathBuf, Vec<String>)>, bool) {
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
pub(super) fn list(state: &AppState, start: &StartState, recent: &[PathBuf]) -> StartView {
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
pub(super) fn reveal(path: &Path) -> Result<(), String> {
    reveal_command(path)?
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Couldn't show {}: {e}", path.display()))
}

/// The system command that shows `path` selected in its folder (macOS and
/// Windows), else its folder; a folder (a project) is opened itself.
pub(super) fn reveal_command(path: &Path) -> Result<std::process::Command, String> {
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
