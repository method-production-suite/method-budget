//! In-app updates, this version's release notes, and What's New
//! (docs/APP.md, "Updates"; how releases are made: docs/RELEASING.md).
//!
//! The updater plugin (`tauri-plugin-updater`) is driven from here only. No
//! window is granted the plugin's own commands: the page has four narrow
//! commands of ours (`update_status`, `update_restart`, `whats_new`,
//! `release_notes`) and hears `update-status` events; Help > Check for
//! Updates and the Check for Updates Automatically switch are answered here.
//!
//! What happens:
//!
//! - A while after launch, then every few hours (unless turned off), the app
//!   fetches the manifest from the endpoint in `tauri.conf.json`
//!   (`plugins.updater.endpoints`, the one place it is written). A newer
//!   version downloads in the background; the plugin checks its signature
//!   against the public key the app was built with before anything is kept.
//!   Then the page shows a quiet "Update ready: Restart to update".
//! - Nothing interrupts: an automatic check that fails says nothing. Only a
//!   check the user asked for (Help > Check for Updates) answers, in the same
//!   quiet place: up to date, or "Couldn't check for updates".
//! - Restart to update saves first: every open budget's changes go into its
//!   file (Autosave's own rules, so a file changed outside the app is never
//!   written over), the open tabs and split view are remembered, and what
//!   still can't be saved gets the same Save All / Discard All / Cancel
//!   question as quitting. Then the update is installed and the app starts
//!   again with its tabs, as after any restart (src/tabs.rs).
//! - After an update, What's New for the new version is shown once
//!   (`whats_new`); Help > Release Notes shows it again (`release_notes`).
//!   The notes are `docs/releases/<version>.md`, embedded at build time
//!   (build.rs) and handed to the page as plain text pieces.
//!
//! Until the release key exists, `tauri.conf.json` carries a placeholder
//! public key: the app then never checks (no network at all) and a manual
//! check answers "Couldn't check for updates". The only hosts the updater
//! may reach are the endpoint's (and, for GitHub, its two download hosts),
//! over https, redirects included.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use method_release::minisign;
use method_release::notes;
use method_release::version::Version;
use serde::{Deserialize, Serialize};
use tauri::menu::{CheckMenuItem, MenuItem, MenuItemKind, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager, Runtime, State};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::app::{AppState, DocId};
use crate::safety::{write_atomically, Access};
use crate::safety_app::{autosave_pass, Autosaved, Safety};

/// Help > Check for Updates…, answered here.
pub const CHECK_ITEM: &str = "help.check_updates";
/// Help > Check for Updates Automatically (a check item), answered here.
pub const AUTO_ITEM: &str = "help.auto_updates";
/// The event the main window hears; the payload is a [`Status`].
pub const STATUS_EVENT: &str = "update-status";

/// The first automatic check, after the app starts.
pub const FIRST_CHECK: Duration = Duration::from_secs(2 * 60);
/// Then one every this long, while the app runs.
pub const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);
/// How often the timer looks at the clock (and at the setting).
const TICK: Duration = Duration::from_secs(30);
/// A check that takes longer gives up.
const CHECK_TIMEOUT: Duration = Duration::from_secs(30);
/// A download that takes longer gives up (and is tried again next time).
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(20 * 60);

/// GitHub serves a release's files from these, after a redirect from
/// github.com.
const GITHUB_DOWNLOAD_HOSTS: &[&str] = &[
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
];

/// This version's release notes, `docs/releases/<version>.md`, copied in by
/// build.rs (empty if there were none).
const NOTES_MD: &str = include_str!(concat!(env!("OUT_DIR"), "/release-notes.md"));

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

// ---------------------------------------------------------------------------
// The core, without Tauri (tested below)

/// Where an update is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    Idle,
    /// `manual`: the user asked (Help > Check for Updates), so the page says
    /// what is happening and how it ended.
    Checking {
        manual: bool,
    },
    Downloading {
        version: String,
        manual: bool,
    },
    /// Downloaded and its signature checked: Restart to update installs it.
    Ready {
        version: String,
    },
    Installing {
        version: String,
    },
}

/// A one-time answer the page shows in its quiet place, then lets go.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Said {
    /// A check the user asked for found nothing newer.
    Current,
    /// A check (or its download) the user asked for didn't work.
    Failed,
    /// Restart to update couldn't install; the update is still ready.
    InstallFailed,
}

/// What a check asks for next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Start {
    /// Go ahead and check.
    Check,
    /// A check or download is already under way (now counted as asked for,
    /// if this one was).
    Busy,
    /// An update is ready or installing already.
    Ready,
}

/// The state machine of an update. Every change goes through here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Machine {
    pub phase: Phase,
}

impl Default for Machine {
    fn default() -> Self {
        Machine { phase: Phase::Idle }
    }
}

impl Machine {
    pub fn start_check(&mut self, manual: bool) -> Start {
        match &mut self.phase {
            Phase::Idle => {
                self.phase = Phase::Checking { manual };
                Start::Check
            }
            Phase::Checking { manual: m } | Phase::Downloading { manual: m, .. } => {
                *m |= manual;
                Start::Busy
            }
            Phase::Ready { .. } | Phase::Installing { .. } => Start::Ready,
        }
    }

    /// The check finished: `Ok(Some(v))` found version `v`, `Ok(None)` found
    /// nothing newer, `Err` didn't work. True when `v` should be downloaded;
    /// and what to tell a user who asked.
    pub fn checked(&mut self, found: Result<Option<String>, ()>) -> (bool, Option<Said>) {
        let Phase::Checking { manual } = self.phase else {
            return (false, None);
        };
        match found {
            Ok(Some(version)) => {
                self.phase = Phase::Downloading { version, manual };
                (true, None)
            }
            Ok(None) => {
                self.phase = Phase::Idle;
                (false, manual.then_some(Said::Current))
            }
            Err(()) => {
                self.phase = Phase::Idle;
                (false, manual.then_some(Said::Failed))
            }
        }
    }

    /// The download finished (`ok`: downloaded and its signature checked).
    pub fn downloaded(&mut self, ok: bool) -> Option<Said> {
        let Phase::Downloading { version, manual } = &self.phase else {
            return None;
        };
        let manual = *manual;
        if ok {
            self.phase = Phase::Ready {
                version: version.clone(),
            };
            None
        } else {
            self.phase = Phase::Idle;
            manual.then_some(Said::Failed)
        }
    }

    /// Restart to update was chosen and nothing unsaved stands in the way:
    /// the version to install, if one is ready.
    pub fn start_install(&mut self) -> Option<String> {
        let Phase::Ready { version } = &self.phase else {
            return None;
        };
        let v = version.clone();
        self.phase = Phase::Installing { version: v.clone() };
        Some(v)
    }

    /// Installing failed: the update stays ready for another try.
    pub fn install_failed(&mut self) -> Said {
        if let Phase::Installing { version } = &self.phase {
            self.phase = Phase::Ready {
                version: version.clone(),
            };
        }
        Said::InstallFailed
    }

    pub fn is_ready(&self) -> bool {
        matches!(self.phase, Phase::Ready { .. })
    }

    /// What the page is told.
    pub fn status(&self, automatic: bool, said: Option<Said>) -> Status {
        let (phase, version, manual) = match &self.phase {
            Phase::Idle => ("idle", None, false),
            Phase::Checking { manual } => ("checking", None, *manual),
            Phase::Downloading { version, manual } => {
                ("downloading", Some(version.clone()), *manual)
            }
            Phase::Ready { version } => ("ready", Some(version.clone()), false),
            Phase::Installing { version } => ("installing", Some(version.clone()), false),
        };
        Status {
            phase,
            version,
            manual,
            said,
            automatic,
        }
    }
}

/// What the page hears (`update-status`, `update_status`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Status {
    /// "idle", "checking", "downloading", "ready" or "installing".
    pub phase: &'static str,
    /// The update's version, once one was found.
    pub version: Option<String>,
    /// The user asked for this check: show its progress.
    pub manual: bool,
    /// A one-time answer to show (then let go).
    pub said: Option<Said>,
    /// Automatic checks are on.
    pub automatic: bool,
}

/// Why this copy can't update itself, from its updater settings: `None` when
/// it can. The placeholder key (or any key that isn't one) and a missing or
/// non-https endpoint all mean no checks at all.
pub fn unavailable_reason(pubkey: &str, endpoints: &[String]) -> Option<String> {
    if let Err(e) = minisign::parse_public_key_box(pubkey) {
        return Some(format!("no updater key ({e})"));
    }
    if endpoints.is_empty() {
        return Some("no update endpoint".into());
    }
    if endpoints.iter().any(|e| !e.starts_with("https://")) {
        return Some("an update endpoint isn't https".into());
    }
    None
}

/// The hosts the updater may reach: each endpoint's, plus GitHub's download
/// hosts when an endpoint is on github.com.
pub fn allowed_hosts(endpoints: &[String]) -> Vec<String> {
    let mut hosts: Vec<String> = Vec::new();
    for e in endpoints {
        let Ok(url) = tauri::Url::parse(e) else {
            continue;
        };
        if let Some(h) = url.host_str() {
            if !hosts.iter().any(|x| x == h) {
                hosts.push(h.to_string());
            }
            if h == "github.com" {
                for g in GITHUB_DOWNLOAD_HOSTS {
                    if !hosts.iter().any(|x| x == g) {
                        hosts.push((*g).to_string());
                    }
                }
            }
        }
    }
    hosts
}

/// True when a request to `scheme://host` is one the updater may make.
pub fn allowed(scheme: &str, host: Option<&str>, hosts: &[String]) -> bool {
    scheme == "https" && host.is_some_and(|h| hosts.iter().any(|a| a == h))
}

/// The updater's settings, in `updates.json` in the app's data folder.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateSettings {
    /// Check now and then without being asked. On unless turned off.
    pub automatic: bool,
    /// The version whose What's New was shown last (or that ran first).
    pub seen: Option<String>,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        UpdateSettings {
            automatic: true,
            seen: None,
        }
    }
}

impl UpdateSettings {
    /// Reads `file`; missing or unreadable means the defaults.
    pub fn load(file: &Path) -> UpdateSettings {
        std::fs::read_to_string(file)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, file: &Path) -> std::io::Result<()> {
        if let Some(dir) = file.parent() {
            crate::safety::create_private_dir(dir)?;
        }
        let bytes = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        write_atomically(file, &bytes, Access::Private)
    }
}

/// Whether What's New is due for `current`, given the version last `seen`:
/// (show it, the version to remember now). A first run shows nothing (it
/// isn't an update); a newer version shows it (and is remembered once
/// shown); going back to an older one shows nothing.
pub fn whats_new_due(seen: Option<&str>, current: &str) -> (bool, Option<String>) {
    let Ok(now) = Version::parse(current) else {
        return (false, None);
    };
    match seen.map(Version::parse) {
        None | Some(Err(_)) => (false, Some(current.to_string())),
        Some(Ok(before)) if before < now => (true, None),
        Some(Ok(before)) if before == now => (false, None),
        Some(Ok(_)) => (false, Some(current.to_string())),
    }
}

/// Release notes as the page draws them: plain text pieces, never markup.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct NotesDto {
    pub version: String,
    pub title: String,
    pub headline: String,
    pub sections: Vec<SectionDto>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SectionDto {
    pub title: String,
    pub items: Vec<ItemDto>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ItemDto {
    pub lead: String,
    pub body: String,
}

/// `markdown` read into what the page draws, for `version`.
pub fn notes_dto(markdown: &str, version: &str) -> Option<NotesDto> {
    let n = notes::parse(markdown).ok()?;
    Some(NotesDto {
        version: version.to_string(),
        title: n.title,
        headline: n.headline,
        sections: n
            .sections
            .into_iter()
            .map(|s| SectionDto {
                title: s.title,
                items: s
                    .items
                    .into_iter()
                    .map(|i| ItemDto {
                        lead: i.lead,
                        body: i.body,
                    })
                    .collect(),
            })
            .collect(),
    })
}

/// This build's own release notes.
pub fn embedded_notes() -> Option<NotesDto> {
    notes_dto(NOTES_MD, env!("CARGO_PKG_VERSION"))
}

/// What flushing before a restart did: each autosave's result, and the
/// budgets still unsaved (untitled, Autosave off, or their file changed
/// outside the app), in tab order.
#[derive(Debug, PartialEq, Eq)]
pub struct Flushed {
    pub autosaved: Vec<(DocId, Autosaved)>,
    pub unsaved: Vec<DocId>,
}

/// Before restarting into an update: every open budget's unsaved changes go
/// into its file where Autosave may write them (never over a file changed
/// outside the app: that budget stays unsaved and the page asks), and the
/// open tabs and split view are remembered for the restart.
pub fn flush_for_restart(state: &AppState, safety: &Safety, now: u64) -> Flushed {
    let autosaved = autosave_pass(state, safety, now, true);
    let docs = state.docs();
    safety.remember_tabs(&docs);
    Flushed {
        autosaved,
        unsaved: docs.dirty(),
    }
}

/// Installing failed: the update stays ready for another try, and a Discard
/// All chosen on the way to the restart is taken back. The app goes on
/// running with those budgets open, so autosave and recovery copies take up
/// again (otherwise nothing would be kept for the rest of the run).
pub fn install_failed(machine: &mut Machine, safety: Option<&Safety>) -> Said {
    if let Some(s) = safety {
        crate::safety_app::discard_cancelled(s);
    }
    machine.install_failed()
}

// ---------------------------------------------------------------------------
// Tauri wiring

/// The updater's state for the app's life.
pub struct Updates {
    machine: Mutex<Machine>,
    /// The downloaded update, ready to install.
    pending: Mutex<Option<(Update, Vec<u8>)>>,
    settings: Mutex<UpdateSettings>,
    settings_file: Option<PathBuf>,
    /// Why this copy can't update itself (`None`: it can).
    unavailable: Option<String>,
    hosts: Vec<String>,
    /// What's New is to be shown once (decided at the start).
    whats_new_due: Mutex<bool>,
    current: String,
}

impl Updates {
    pub fn automatic(&self) -> bool {
        lock(&self.settings).automatic
    }

    fn set_automatic(&self, on: bool) {
        let mut s = lock(&self.settings);
        s.automatic = on;
        if let Some(f) = &self.settings_file {
            let _ = s.save(f);
        }
    }

    fn status(&self, said: Option<Said>) -> Status {
        lock(&self.machine).status(self.automatic(), said)
    }
}

/// The updater settings `tauri.conf.json` carries: the public key and the
/// endpoints.
fn configured<R: Runtime>(app: &AppHandle<R>) -> (String, Vec<String>) {
    let conf = app
        .config()
        .plugins
        .0
        .get("updater")
        .cloned()
        .unwrap_or_default();
    let key = conf["pubkey"].as_str().unwrap_or_default().to_string();
    let endpoints = conf["endpoints"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|e| e.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    (key, endpoints)
}

fn tell<R: Runtime>(app: &AppHandle<R>, said: Option<Said>) {
    if let Some(u) = app.try_state::<Updates>() {
        let _ = app.emit_to(crate::tools::MAIN, STATUS_EVENT, u.status(said));
    }
}

/// Sets everything up when the app starts: the settings, whether What's New
/// is due, the menu's check, and the timer for automatic checks.
pub fn init<R: Runtime>(app: &AppHandle<R>) {
    let current = app.package_info().version.to_string();
    let settings_file = app
        .path()
        .app_local_data_dir()
        .ok()
        .map(|d| d.join("updates.json"));
    let mut settings = settings_file
        .as_deref()
        .map(UpdateSettings::load)
        .unwrap_or_default();
    let (due, remember) = whats_new_due(settings.seen.as_deref(), &current);
    if let Some(v) = remember {
        settings.seen = Some(v);
        if let Some(f) = &settings_file {
            let _ = settings.save(f);
        }
    }
    let (key, endpoints) = configured(app);
    let unavailable = unavailable_reason(&key, &endpoints);
    app.manage(Updates {
        machine: Mutex::new(Machine::default()),
        pending: Mutex::new(None),
        settings: Mutex::new(settings),
        settings_file,
        hosts: allowed_hosts(&endpoints),
        unavailable,
        whats_new_due: Mutex::new(due),
        current,
    });
    refresh_menu(app);
    if app.state::<Updates>().unavailable.is_some() {
        // Never checks: no timer, no network.
        return;
    }
    let handle = app.clone();
    std::thread::spawn(move || {
        let mut next = Instant::now() + FIRST_CHECK;
        loop {
            std::thread::sleep(TICK);
            if Instant::now() < next {
                continue;
            }
            next = Instant::now() + CHECK_EVERY;
            if handle.state::<Updates>().automatic() {
                check_now(&handle, false);
            }
        }
    });
}

/// The redirect rule for the updater's requests: https, to the allowed
/// hosts only, at most ten hops.
fn redirect_policy(hosts: Vec<String>) -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(move |attempt| {
        if attempt.previous().len() >= 10 {
            return attempt.error("too many redirects");
        }
        let ok = allowed(attempt.url().scheme(), attempt.url().host_str(), &hosts);
        if ok {
            attempt.follow()
        } else {
            attempt.error("a redirect to a host the updater doesn't use")
        }
    })
}

/// Asks the endpoint for a newer version.
async fn fetch<R: Runtime>(
    app: &AppHandle<R>,
    hosts: Vec<String>,
) -> Result<Option<Update>, String> {
    let client_hosts = hosts.clone();
    let updater = app
        .updater_builder()
        .timeout(CHECK_TIMEOUT)
        .configure_client(move |b| {
            b.https_only(true)
                .redirect(redirect_policy(client_hosts.clone()))
        })
        .build()
        .map_err(|e| e.to_string())?;
    let update = updater.check().await.map_err(|e| e.to_string())?;
    if let Some(u) = &update {
        let url = &u.download_url;
        if !allowed(url.scheme(), url.host_str(), &hosts) {
            return Err(format!(
                "the update is on a host the updater doesn't use ({url})"
            ));
        }
    }
    Ok(update)
}

/// One check, then the download if it found something. Blocks: run it on a
/// thread of its own. `manual`: the user asked (Help > Check for Updates).
pub fn check_now<R: Runtime>(app: &AppHandle<R>, manual: bool) {
    let updates = app.state::<Updates>();
    if updates.unavailable.is_some() {
        if manual {
            tell(app, Some(Said::Failed));
        }
        return;
    }
    let start = lock(&updates.machine).start_check(manual);
    tell(app, None);
    if start != Start::Check {
        return;
    }
    let found = tauri::async_runtime::block_on(fetch(app, updates.hosts.clone()));
    let version = match &found {
        Ok(Some(u)) => Ok(Some(u.version.clone())),
        Ok(None) => Ok(None),
        Err(_) => Err(()),
    };
    let (download, said) = lock(&updates.machine).checked(version);
    tell(app, said);
    let (true, Ok(Some(mut update))) = (download, found) else {
        return;
    };
    update.timeout = Some(DOWNLOAD_TIMEOUT);
    // The plugin checks the signature against the app's public key before
    // it hands the bytes over; a bad one is an error here.
    let bytes = tauri::async_runtime::block_on(update.download(|_, _| {}, || {}));
    let said = match bytes {
        Ok(bytes) => {
            *lock(&updates.pending) = Some((update, bytes));
            lock(&updates.machine).downloaded(true)
        }
        Err(_) => lock(&updates.machine).downloaded(false),
    };
    tell(app, said);
}

/// Installs the ready update and starts the app again. Runs on a thread of
/// its own (installing on macOS may need the main thread to ask for an
/// administrator's password).
fn install_and_restart<R: Runtime>(app: &AppHandle<R>) {
    let updates = app.state::<Updates>();
    if lock(&updates.machine).start_install().is_none() {
        return;
    }
    tell(app, None);
    // The tabs as they are now (Save All may have given a budget its file).
    if let Some(safety) = app.try_state::<Safety>() {
        safety.remember_tabs(&app.state::<AppState>().docs());
    }
    let pending = lock(&updates.pending).take();
    let result = match &pending {
        Some((update, bytes)) => update.install(bytes).map_err(|e| e.to_string()),
        None => Err("the update is gone".to_string()),
    };
    match result {
        Ok(()) => app.restart(),
        Err(_) => {
            *lock(&updates.pending) = pending;
            let safety = app.try_state::<Safety>();
            let said = install_failed(&mut lock(&updates.machine), safety.as_deref());
            tell(app, Some(said));
        }
    }
}

/// Adds Check for Updates… and the Check for Updates Automatically switch to
/// the Help menu. Called by `menu::build`.
pub fn extend_help_menu<R: Runtime>(app: &AppHandle<R>, help: &Submenu<R>) -> tauri::Result<()> {
    let on = app.try_state::<Updates>().is_none_or(|u| u.automatic());
    help.append(&PredefinedMenuItem::separator(app)?)?;
    help.append(&MenuItem::with_id(
        app,
        CHECK_ITEM,
        "Check for Updates…",
        true,
        None::<&str>,
    )?)?;
    help.append(&CheckMenuItem::with_id(
        app,
        AUTO_ITEM,
        "Check for Updates Automatically",
        true,
        on,
        None::<&str>,
    )?)?;
    Ok(())
}

/// Brings the Check for Updates Automatically check up to date.
pub fn refresh_menu<R: Runtime>(app: &AppHandle<R>) {
    let (Some(menu), Some(updates)) = (app.menu(), app.try_state::<Updates>()) else {
        return;
    };
    if let Some(MenuItemKind::Check(c)) =
        crate::safety_app::find(menu.items().unwrap_or_default(), AUTO_ITEM)
    {
        let _ = c.set_checked(updates.automatic());
    }
}

/// Menu items answered here: Check for Updates… and the automatic switch.
/// False for anything else (the caller passes it on).
pub fn handle_menu<R: Runtime>(app: &AppHandle<R>, id: &str) -> bool {
    match id {
        CHECK_ITEM => {
            let handle = app.clone();
            std::thread::spawn(move || check_now(&handle, true));
            true
        }
        AUTO_ITEM => {
            if let Some(u) = app.try_state::<Updates>() {
                let on = !u.automatic();
                u.set_automatic(on);
            }
            refresh_menu(app);
            tell(app, None);
            true
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Commands (registered in app.rs, build.rs and capabilities/main.json; the
// main window only)

/// Where an update is, for the page's quiet indicator.
#[tauri::command]
pub fn update_status(updates: State<'_, Updates>) -> Status {
    updates.status(None)
}

/// Restart to update: saves what can be saved, asks about what can't (as
/// quitting does), then installs the ready update and starts again.
#[tauri::command]
pub fn update_restart<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    if !lock(&app.state::<Updates>().machine).is_ready() {
        return Err("No update is ready to install.".into());
    }
    let flushed = flush_for_restart(
        &app.state::<AppState>(),
        &app.state::<Safety>(),
        method_session::clock::mono_ms(),
    );
    for (id, done) in flushed.autosaved {
        crate::safety_app::tell_autosaved(&app, id, done);
    }
    let handle = app.clone();
    let go = move || {
        std::thread::spawn(move || install_and_restart(&handle));
    };
    if flushed.unsaved.is_empty() {
        go();
    } else {
        // Untitled budgets, Autosave off, or a file changed outside the
        // app: the quit question, then the restart (or nothing, on Cancel).
        crate::app::confirm_close(&app, go);
    }
    Ok(())
}

/// What's New for this version, once after an update (`None` otherwise, and
/// from then on).
#[tauri::command]
pub fn whats_new(updates: State<'_, Updates>) -> Option<NotesDto> {
    {
        let mut due = lock(&updates.whats_new_due);
        if !*due {
            return None;
        }
        *due = false;
    }
    {
        let mut s = lock(&updates.settings);
        s.seen = Some(updates.current.clone());
        if let Some(f) = &updates.settings_file {
            let _ = s.save(f);
        }
    }
    embedded_notes()
}

/// This version's release notes (Help > Release Notes).
#[tauri::command]
pub fn release_notes() -> Option<NotesDto> {
    embedded_notes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::safety::tests::temp_dir;
    use crate::safety_app::{autosave_doc, open_path};
    use crate::session::Session;
    use method_release::minisign::{fixture, PLACEHOLDER_PUBLIC_KEY};

    const ENDPOINT: &str =
        "https://github.com/ssandweiss/method-budget/releases/latest/download/latest.json";

    #[test]
    fn automatic_checks_are_quiet() {
        let mut m = Machine::default();
        assert_eq!(m.start_check(false), Start::Check);
        assert_eq!(m.status(true, None).phase, "checking");
        assert!(!m.status(true, None).manual);
        // Nothing newer, or no network: back to idle, nothing said.
        assert_eq!(m.checked(Ok(None)), (false, None));
        assert_eq!(m.phase, Phase::Idle);
        m.start_check(false);
        assert_eq!(m.checked(Err(())), (false, None));
        assert_eq!(m.phase, Phase::Idle);
        // Found: downloads quietly, then it's ready.
        m.start_check(false);
        assert_eq!(m.checked(Ok(Some("0.2.0".into()))), (true, None));
        let s = m.status(true, None);
        assert_eq!(
            (s.phase, s.version.as_deref(), s.manual),
            ("downloading", Some("0.2.0"), false)
        );
        assert_eq!(m.downloaded(true), None);
        assert_eq!(m.status(true, None).phase, "ready");
        // A failed download says nothing either, and is tried again later.
        let mut m = Machine::default();
        m.start_check(false);
        m.checked(Ok(Some("0.2.0".into())));
        assert_eq!(m.downloaded(false), None);
        assert_eq!(m.phase, Phase::Idle);
    }

    #[test]
    fn a_check_the_user_asked_for_answers() {
        let mut m = Machine::default();
        assert_eq!(m.start_check(true), Start::Check);
        assert!(m.status(true, None).manual);
        assert_eq!(m.checked(Ok(None)), (false, Some(Said::Current)));
        m.start_check(true);
        assert_eq!(m.checked(Err(())), (false, Some(Said::Failed)));
        m.start_check(true);
        m.checked(Ok(Some("0.3.0".into())));
        assert!(
            m.status(true, None).manual,
            "the download shows its progress"
        );
        assert_eq!(m.downloaded(false), Some(Said::Failed));

        // Asking while an automatic check runs makes it the user's.
        let mut m = Machine::default();
        m.start_check(false);
        assert_eq!(m.start_check(true), Start::Busy);
        assert_eq!(m.checked(Ok(None)), (false, Some(Said::Current)));
        // Asking once an update is ready changes nothing.
        let mut m = Machine::default();
        m.start_check(false);
        m.checked(Ok(Some("0.3.0".into())));
        m.downloaded(true);
        assert_eq!(m.start_check(true), Start::Ready);
        assert_eq!(m.status(false, None).phase, "ready");
    }

    #[test]
    fn installing() {
        let mut m = Machine::default();
        assert_eq!(m.start_install(), None, "nothing is ready");
        m.start_check(false);
        m.checked(Ok(Some("0.2.0".into())));
        m.downloaded(true);
        assert_eq!(m.start_install().as_deref(), Some("0.2.0"));
        assert_eq!(m.status(true, None).phase, "installing");
        assert_eq!(m.start_install(), None, "only once");
        assert_eq!(m.start_check(true), Start::Ready);
        // A failed install leaves it ready for another try.
        assert_eq!(m.install_failed(), Said::InstallFailed);
        assert!(m.is_ready());
        let s = serde_json::to_value(m.status(false, Some(Said::InstallFailed))).unwrap();
        assert_eq!(
            s,
            serde_json::json!({ "phase": "ready", "version": "0.2.0", "manual": false, "said": "install-failed", "automatic": false })
        );
    }

    #[test]
    fn no_real_key_means_no_checks() {
        let endpoints = vec![ENDPOINT.to_string()];
        assert!(unavailable_reason(PLACEHOLDER_PUBLIC_KEY, &endpoints).is_some());
        assert!(unavailable_reason("", &endpoints).is_some());
        assert!(unavailable_reason("garbage", &endpoints).is_some());
        let key = fixture::public_key(7);
        assert_eq!(unavailable_reason(&key, &endpoints), None);
        assert!(unavailable_reason(&key, &[]).is_some());
        assert!(unavailable_reason(&key, &["http://github.com/x".to_string()]).is_some());

        // This repository ships the placeholder until the release key exists
        // (docs/RELEASING.md), so a build from it never checks; once the key
        // is in, it must be a real one.
        let conf: serde_json::Value = serde_json::from_str(CONF).unwrap();
        let u = &conf["plugins"]["updater"];
        let pubkey = u["pubkey"].as_str().unwrap();
        let endpoints: Vec<String> = u["endpoints"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e.as_str().unwrap().to_string())
            .collect();
        assert_eq!(endpoints, [ENDPOINT]);
        if pubkey != PLACEHOLDER_PUBLIC_KEY {
            assert_eq!(unavailable_reason(pubkey, &endpoints), None);
        }
        // Never loosened.
        assert!(u.get("dangerousInsecureTransportProtocol").is_none());
        assert!(u.get("dangerousAcceptInvalidCerts").is_none());
        assert!(u.get("dangerousAcceptInvalidHostnames").is_none());
        assert!(u.get("allowDowngrades").is_none());
    }

    #[test]
    fn only_the_endpoint_hosts_over_https() {
        let hosts = allowed_hosts(&[ENDPOINT.to_string()]);
        assert_eq!(
            hosts,
            [
                "github.com",
                "objects.githubusercontent.com",
                "release-assets.githubusercontent.com"
            ]
        );
        assert!(allowed("https", Some("github.com"), &hosts));
        assert!(allowed(
            "https",
            Some("release-assets.githubusercontent.com"),
            &hosts
        ));
        assert!(!allowed("http", Some("github.com"), &hosts));
        assert!(!allowed("https", Some("example.com"), &hosts));
        assert!(!allowed("https", Some("github.com.example.com"), &hosts));
        assert!(!allowed("https", None, &hosts));
        let other = allowed_hosts(&["https://updates.example.org/latest.json".to_string()]);
        assert_eq!(other, ["updates.example.org"]);
    }

    #[test]
    fn whats_new_once_after_an_update() {
        // First run: nothing to show; this version is remembered.
        assert_eq!(whats_new_due(None, "0.1.0"), (false, Some("0.1.0".into())));
        // An update: shown (and remembered when it is).
        assert_eq!(whats_new_due(Some("0.1.0"), "0.2.0"), (true, None));
        assert_eq!(whats_new_due(Some("0.2.0-beta.1"), "0.2.0"), (true, None));
        // Same version again: nothing.
        assert_eq!(whats_new_due(Some("0.2.0"), "0.2.0"), (false, None));
        // Back to an older version, or a setting that isn't a version.
        assert_eq!(
            whats_new_due(Some("0.3.0"), "0.2.0"),
            (false, Some("0.2.0".into()))
        );
        assert_eq!(
            whats_new_due(Some("junk"), "0.2.0"),
            (false, Some("0.2.0".into()))
        );
    }

    #[test]
    fn settings_are_kept() {
        let dir = temp_dir("updates-settings");
        let file = dir.join("updates.json");
        assert_eq!(UpdateSettings::load(&file), UpdateSettings::default());
        assert!(UpdateSettings::default().automatic, "on by default");
        let s = UpdateSettings {
            automatic: false,
            seen: Some("0.1.0".into()),
        };
        s.save(&file).unwrap();
        assert_eq!(UpdateSettings::load(&file), s);
        std::fs::write(&file, "{ not json").unwrap();
        assert_eq!(UpdateSettings::load(&file), UpdateSettings::default());
        std::fs::write(&file, "{\"seen\": \"0.1.0\"}").unwrap();
        assert!(
            UpdateSettings::load(&file).automatic,
            "a missing field is its default"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn this_versions_notes_are_built_in() {
        let n = embedded_notes().expect("docs/releases/<version>.md is embedded and readable");
        assert_eq!(n.version, env!("CARGO_PKG_VERSION"));
        assert!(n.title.ends_with(env!("CARGO_PKG_VERSION")), "{}", n.title);
        assert!(!n.headline.is_empty() && !n.sections.is_empty());
        // Plain text: no markup survives into what the page draws.
        let all = serde_json::to_string(&n).unwrap();
        assert!(!all.contains("**") && !all.contains("<!--"), "{all}");
        assert!(notes_dto("not notes", "1.0.0").is_none());
    }

    fn write_budget(path: &Path, title: &str) {
        let mut b = method_core::starter::budget();
        b.title = title.into();
        std::fs::write(path, method_format::to_string(&b)).unwrap();
    }

    fn title_on_disk(path: &Path) -> String {
        method_format::from_str(&std::fs::read_to_string(path).unwrap())
            .unwrap()
            .title
    }

    fn retitle(state: &AppState, id: DocId, t: &str) {
        state
            .lock_doc(Some(id))
            .unwrap()
            .edit("Test edit", |b| crate::edit::set_title(b, t))
            .unwrap();
    }

    #[test]
    fn restarting_saves_every_budget_and_brings_the_tabs_back() {
        let dir = temp_dir("updates-restart");
        let (a, b, c) = (dir.join("a.mthd"), dir.join("b.mthd"), dir.join("c.mthd"));
        for p in [&a, &b, &c] {
            write_budget(p, &p.file_stem().unwrap().to_string_lossy());
        }
        {
            let safety = Safety::new(Some(&dir));
            let state = AppState::default();
            let ia = open_path(&state, &safety, &a).unwrap();
            let ib = open_path(&state, &safety, &b).unwrap();
            let ic = open_path(&state, &safety, &c).unwrap();
            // Split view: c on its own on the right, b in front on the left.
            state.docs().split_right().unwrap();
            state.docs().switch_in(0, ib).unwrap();
            // Unsaved edits everywhere, typed a moment ago (not yet due for
            // autosave), and c's file changed by someone else meanwhile.
            retitle(&state, ia, "a, edited");
            retitle(&state, ib, "b, edited");
            retitle(&state, ic, "c, mine");
            write_budget(&c, "c, theirs");

            let flushed = flush_for_restart(&state, &safety, method_session::clock::mono_ms());
            assert_eq!(
                flushed.autosaved,
                [
                    (ia, Autosaved::File),
                    (ib, Autosaved::File),
                    (ic, Autosaved::Conflict)
                ]
            );
            // The two went into their files at once; c's file is left alone
            // and c is still unsaved, so the page asks before restarting.
            assert_eq!(title_on_disk(&a), "a, edited");
            assert_eq!(title_on_disk(&b), "b, edited");
            assert_eq!(title_on_disk(&c), "c, theirs");
            assert_eq!(flushed.unsaved, [ic]);
            // Nothing to save the second time round.
            let again = flush_for_restart(&state, &safety, method_session::clock::mono_ms());
            assert!(again
                .autosaved
                .iter()
                .all(|(_, d)| *d == Autosaved::Nothing));
        }
        // After the restart: the same tabs, in order, and the same split.
        let safety = Safety::new(Some(&dir));
        let state = AppState::default();
        assert_eq!(crate::safety_app::restore_tabs(&state, &safety), 3);
        let titles: Vec<String> = {
            let docs = state.docs();
            docs.ids()
                .into_iter()
                .map(|id| docs.get(id).unwrap().budget().title.clone())
                .collect()
        };
        assert_eq!(titles, ["a, edited", "b, edited", "c, theirs"]);
        let layout = state.docs().layout();
        assert!(layout.split);
        assert_eq!(layout.panes[0].tabs.len(), 3);
        assert_eq!(layout.panes[1].tabs.len(), 1);
        let front = |p: usize| {
            state
                .docs()
                .get(layout.panes[p].front)
                .unwrap()
                .budget()
                .title
                .clone()
        };
        assert_eq!(front(0), "b, edited");
        assert_eq!(front(1), "c, theirs");
        drop(safety);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Discard All on the way to a restart that then fails to install: the
    /// app goes on with the budgets open, and keeps their changes again
    /// (recovery copies, autosave into files) rather than nothing for the
    /// rest of the run.
    #[test]
    fn a_failed_install_takes_discard_all_back() {
        let dir = temp_dir("updates-discarded");
        let safety = Safety::new(Some(&dir));
        let state = AppState::default();
        let untitled = state.docs().active();
        retitle(&state, untitled, "Draft");
        crate::safety_app::discard_all(&state, &safety);
        let now = method_session::clock::mono_ms();
        assert_eq!(
            autosave_doc(&state, &safety, None, now, true),
            Autosaved::Nothing,
            "thrown away on the way out"
        );
        let mut m = Machine::default();
        m.start_check(false);
        m.checked(Ok(Some("0.2.0".into())));
        m.downloaded(true);
        m.start_install();
        assert_eq!(install_failed(&mut m, Some(&safety)), Said::InstallFailed);
        assert!(m.is_ready(), "ready for another try");
        assert_eq!(
            autosave_doc(&state, &safety, None, now, true),
            Autosaved::Recovery
        );
        let a = dir.join("a.mthd");
        write_budget(&a, "a");
        let ia = open_path(&state, &safety, &a).unwrap();
        retitle(&state, ia, "a, edited");
        assert_eq!(
            autosave_doc(&state, &safety, Some(ia), now, true),
            Autosaved::File
        );
        assert_eq!(title_on_disk(&a), "a, edited");
        drop(safety);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn untitled_budgets_and_autosave_off_wait_for_the_question() {
        let dir = temp_dir("updates-untitled");
        let safety = Safety::new(Some(&dir));
        let state = AppState::default();
        let a = dir.join("a.mthd");
        write_budget(&a, "a");
        let ia = open_path(&state, &safety, &a).unwrap();
        let untitled = state.docs().add(Session::new());
        retitle(&state, untitled, "Draft");
        safety.set_autosave(false);
        retitle(&state, ia, "a, edited");
        let flushed = flush_for_restart(&state, &safety, method_session::clock::mono_ms());
        // Autosave off: the file is untouched and a recovery copy is kept;
        // the untitled budget too. Both wait for Save All / Discard All.
        assert_eq!(title_on_disk(&a), "a");
        assert_eq!(flushed.unsaved, [ia, untitled]);
        assert!(flushed
            .autosaved
            .iter()
            .all(|(_, d)| *d == Autosaved::Recovery));
        drop(safety);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    const APP_RS: &str = include_str!("app.rs");
    const BUILD_RS: &str = include_str!("../build.rs");
    const MAIN_CAPS: &str = include_str!("../capabilities/main.json");
    const TOOLS_CAPS: &str = include_str!("../capabilities/tools.json");
    const SETUP_CAPS: &str = include_str!("../capabilities/setup.json");
    const CONF: &str = include_str!("../tauri.conf.json");
    const UPDATES_JS: &str = include_str!("../ui/js/updates.js");

    #[test]
    fn the_commands_are_the_main_windows_only() {
        for c in [
            "update_status",
            "update_restart",
            "whats_new",
            "release_notes",
        ] {
            assert!(
                APP_RS.contains(&format!("crate::updater::{c},")),
                "{c} not in generate_handler!"
            );
            assert!(
                BUILD_RS.contains(&format!("\"{c}\",")),
                "{c} not in build.rs"
            );
            let allow = format!("\"allow-{}\"", c.replace('_', "-"));
            assert!(
                MAIN_CAPS.contains(&allow),
                "{c} not allowed for the main window"
            );
            assert!(
                !TOOLS_CAPS.contains(&allow),
                "{c} allowed for the tools window"
            );
            assert!(
                !SETUP_CAPS.contains(&allow),
                "{c} allowed for the setup window"
            );
        }
        // No window gets the plugin's own commands.
        for caps in [MAIN_CAPS, TOOLS_CAPS, SETUP_CAPS] {
            assert!(
                !caps.contains("updater:"),
                "a window is granted updater commands"
            );
        }
        assert!(APP_RS.contains("tauri_plugin_updater::Builder::new().build()"));
    }

    #[test]
    fn the_web_view_still_has_no_network() {
        // The updater runs in Rust; the page's CSP allows only the app's own
        // files and IPC, never a web host.
        let conf: serde_json::Value = serde_json::from_str(CONF).unwrap();
        let csp = conf["app"]["security"]["csp"].as_str().unwrap();
        assert!(
            csp.contains("connect-src ipc: http://ipc.localhost;"),
            "{csp}"
        );
        for src in ["https:", "github", "*"] {
            assert!(
                !csp.contains(src),
                "the CSP lets the page reach {src}: {csp}"
            );
        }
        // Release notes are drawn with textContent and DOM calls, never as markup.
        assert!(!UPDATES_JS.contains("innerHTML"));
        assert!(!UPDATES_JS.contains("insertAdjacentHTML"));
        assert!(!UPDATES_JS.contains("outerHTML"));
    }
}
