//! In-app updates, this version's release notes, and What's New
//! (docs/APP.md, "Updates"; how releases are made: docs/RELEASING.md;
//! why the work moved to the suite's crate: docs/decisions.md, entry 83).
//!
//! The work underneath is the suite's shared updater, `method-update`
//! (method-production-suite/method-libs): the manifest, the download and its
//! minisign signature, the host rule, the Mac bundle swap, the settings file
//! and the state machine. This file keeps Method Budget's window side: the
//! page has four narrow commands of ours (`update_status`, `update_restart`,
//! `whats_new`, `release_notes`) and hears `update-status` events; Help >
//! Check for Updates and the Check for Updates Automatically switch are
//! answered here.
//!
//! What happens:
//!
//! - Two minutes after launch, then every six hours (unless turned off), the
//!   app fetches the manifest from the endpoint in `tauri.conf.json`
//!   (`plugins.updater.endpoints`, the one place it is written, baked in by
//!   build.rs). A newer version downloads in the background, and its
//!   signature is checked against the public key the app was built with
//!   (`plugins.updater.pubkey`, the same way) before anything is kept. The
//!   signature must name the version the manifest announces. Then the page
//!   shows a quiet "Update ready: Restart to update".
//! - Nothing interrupts: an automatic check that fails says nothing. Only a
//!   check the user asked for (Help > Check for Updates) answers, in the same
//!   quiet place: up to date, or "Couldn't check for updates". A computer
//!   the manifest has no build for (anything but a Mac; an Intel Mac reading
//!   a manifest from 0.3 to 0.5, which were Apple silicon only) answers
//!   "There's no update for this computer yet. This version stays as it is."
//!   instead. Releases are universal again from 0.6, so both Macs find theirs.
//! - Restart to update saves first: every open budget's changes go into its
//!   file (Autosave's own rules, so a file changed outside the app is never
//!   written over), the open tabs and split view are remembered, and what
//!   still can't be saved gets the same Save All / Discard All / Cancel
//!   question as quitting. Then the app bundle is swapped for the update in
//!   place and the app starts again with its tabs, as after any restart
//!   (src/tabs.rs).
//! - After an update, What's New for the new version is shown once
//!   (`whats_new`); Help > Release Notes shows it again (`release_notes`).
//!   The notes are `docs/releases/<version>.md`, embedded at build time
//!   (build.rs) and handed to the page as plain text pieces.
//!
//! A build with the placeholder public key never checks (no network at all),
//! and a manual check answers "Couldn't check for updates". The only hosts
//! the updater may reach are the endpoint's (and, for GitHub, its two
//! download hosts), over https, redirects included.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use method_release::notes;
use method_update::install::{self, InstallError};
use method_update::policy::{self, Phase, Settings, Updater, TICK};
use method_update::target::Installer;
use method_update::{transport, AppConfig, Baked, Update, UpdateError};
use serde::Serialize;
use tauri::menu::{CheckMenuItem, MenuItem, MenuItemKind, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

use crate::app::{AppState, DocId};
use crate::safety_app::{autosave_pass, Autosaved, Safety};

/// Help > Check for Updates…, answered here.
pub const CHECK_ITEM: &str = "help.check_updates";
/// Help > Check for Updates Automatically (a check item), answered here.
pub const AUTO_ITEM: &str = "help.auto_updates";
/// The event the main window hears; the payload is a [`Status`].
pub const STATUS_EVENT: &str = "update-status";

/// The first automatic check, after the app starts (the suite's schedule).
pub const FIRST_CHECK: Duration = policy::FIRST_CHECK;
/// Then one every this long, while the app runs.
pub const CHECK_EVERY: Duration = policy::CHECK_EVERY;
/// A check that takes longer gives up.
const CHECK_TIMEOUT: Duration = Duration::from_secs(30);
/// A download that takes longer gives up (and is tried again next time).
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(20 * 60);

/// The bundle every Mac build is (`productName` in tauri.conf.json).
pub const BUNDLE_NAME: &str = "Method Budget.app";

// ENDPOINT and PUBKEY: `plugins.updater` in tauri.conf.json, copied by
// build.rs, so the key is written in one place only.
include!(concat!(env!("OUT_DIR"), "/updater-config.rs"));

/// This version's release notes, `docs/releases/<version>.md`, copied in by
/// build.rs (empty if there were none).
const NOTES_MD: &str = include_str!(concat!(env!("OUT_DIR"), "/release-notes.md"));

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// What Method Budget tells the updater about itself.
pub fn update_config() -> AppConfig {
    AppConfig::new(
        "Method Budget",
        BUNDLE_NAME,
        concat!("method-budget-update/", env!("CARGO_PKG_VERSION")),
        Baked::release(ENDPOINT, PUBKEY),
    )
}

// ---------------------------------------------------------------------------
// The page's side of the policy, without Tauri (tested below)

/// A one-time answer the page shows in its quiet place, then lets go
/// (ui/js/updates.js, `TEXT`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Said {
    /// A check the user asked for found nothing newer.
    Current,
    /// A check (or its download) the user asked for didn't work.
    Failed,
    /// A check the user asked for, on a computer the manifest has no build
    /// for (not a Mac, or an Intel Mac reading an Apple-silicon-only
    /// manifest): this version stays as it is.
    NoBuild,
    /// Restart to update couldn't install; the update is still ready.
    InstallFailed,
}

/// Why a check didn't work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckFailed {
    /// No network, no manifest, a bad answer: worth trying again later.
    Failed,
    /// No build for this computer.
    NoBuild,
}

/// What an updater error means for the user. `Platform` is this computer
/// having no build: an Intel Mac (or Windows, or Linux) is refused before
/// anything is fetched, and a manifest without a row for this computer is
/// refused the same way. Anything else is a failed check.
pub fn check_failure(e: &UpdateError) -> CheckFailed {
    match e {
        UpdateError::Platform(_) => CheckFailed::NoBuild,
        _ => CheckFailed::Failed,
    }
}

/// What the page hears (`update-status`, `update_status`): the same five
/// fields and five phases as always. The suite's own `failed` and
/// `read-only` phases are not the page's: a failure is said once and the
/// indicator goes back to idle.
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

/// The suite's status in the page's words, with this answer.
pub fn page_status(s: &policy::Status, said: Option<Said>) -> Status {
    let (phase, version, manual) = match s.phase {
        p @ ("checking" | "downloading" | "ready" | "installing") => {
            (p, s.version.clone(), s.manual)
        }
        _ => ("idle", None, false),
    };
    Status {
        phase,
        version,
        manual,
        said,
        automatic: s.automatic,
    }
}

/// The suite's one-time answer in the page's words: a failure for want of a
/// build for this computer is said as such.
pub fn page_said(said: Option<policy::Said>, failed: Option<CheckFailed>) -> Option<Said> {
    Some(match said? {
        policy::Said::Current => Said::Current,
        policy::Said::Failed if failed == Some(CheckFailed::NoBuild) => Said::NoBuild,
        policy::Said::Failed | policy::Said::ReadOnly => Said::Failed,
        policy::Said::InstallFailed => Said::InstallFailed,
    })
}

/// The updater for the app's life: the suite's policy, and why the last
/// check failed (for the page's no-build answer).
pub struct Updates {
    pub updater: Updater,
    last_failure: Mutex<Option<CheckFailed>>,
    /// `--check-for-updates` on the command line (src/launch.rs): a check
    /// as Help > Check for Updates…, started when the page first asks
    /// where an update is (so it is listening for the answer).
    check_at_start: AtomicBool,
}

impl Updates {
    /// The updater for `config`, running `current`, keeping its settings in
    /// `settings_file`. A first run (or a version older than the one last
    /// seen) remembers this version at once, as Method Budget always has,
    /// so What's New knows a later version is an update.
    pub fn new(config: AppConfig, current: &str, settings_file: Option<PathBuf>) -> Self {
        if let Some(f) = &settings_file {
            remember_first_run(f, current);
        }
        Updates {
            updater: Updater::new(config, current, settings_file, Instant::now()),
            last_failure: Mutex::new(None),
            check_at_start: AtomicBool::new(false),
        }
    }

    /// The command line asked for a check (`--check-for-updates`).
    pub fn ask_to_check_at_start(&self) {
        self.check_at_start.store(true, Ordering::SeqCst);
    }

    /// True once, when the command line asked for a check and it hasn't
    /// been started yet.
    pub fn take_check_at_start(&self) -> bool {
        self.check_at_start.swap(false, Ordering::SeqCst)
    }

    pub fn automatic(&self) -> bool {
        self.updater.automatic()
    }

    /// What the page is told, with this answer.
    pub fn status(&self, said: Option<Said>) -> Status {
        page_status(&self.updater.status(None), said)
    }

    /// One check, then the download if it found something; `check` and
    /// `download` are the network, `tell` hears every step. Blocks: run it
    /// on a thread of its own. `manual`: the user asked. Returns the
    /// one-time answer, which `tell` has also heard.
    pub fn check_with(
        &self,
        manual: bool,
        check: impl FnOnce() -> Result<Option<Update>, UpdateError>,
        download: impl FnOnce(&Update) -> Result<Vec<u8>, UpdateError>,
        tell: impl Fn(Status),
    ) -> Option<Said> {
        let failed = |e: UpdateError| {
            *lock(&self.last_failure) = Some(check_failure(&e));
            e.to_string()
        };
        let said = self.updater.check_with(
            manual,
            || {
                // The check has started: the page shows it (when asked for).
                *lock(&self.last_failure) = None;
                tell(self.status(None));
                check().map_err(failed)
            },
            |update| {
                tell(self.status(None));
                download(update).map_err(failed)
            },
        );
        let said = page_said(said, *lock(&self.last_failure));
        // A failure is said once; the indicator goes back to idle.
        self.updater.dismiss();
        tell(self.status(said));
        said
    }
}

/// A first run, or a version older than the one last seen: remember this
/// version now (What's New is for a later version). A settings file this
/// build can't read is left alone.
pub fn remember_first_run(file: &Path, current: &str) {
    let Ok(read) = Settings::read(file) else {
        return;
    };
    let mut s = read.unwrap_or_default();
    if let (_, Some(v)) = policy::whats_new_due(s.seen.as_deref(), current) {
        s.seen = Some(v);
        if let Some(dir) = file.parent() {
            let _ = crate::safety::create_private_dir(dir);
        }
        let _ = s.save(file);
    }
}

/// The real network: this app's endpoint and key, a short timeout for the
/// check and a long one for the download, and the download's host checked
/// before it is fetched.
fn check_over_network(config: &AppConfig, current: &str) -> Result<Option<Update>, UpdateError> {
    let mut c = config.transport(Some(Installer::App));
    c.timeout = Some(CHECK_TIMEOUT);
    let version = method_update::manifest::parse_version(current).map_err(UpdateError::Manifest)?;
    let found = transport::check(&c, &version)?;
    if let Some(u) = &found {
        // The update is on a host the updater doesn't use.
        if !transport::host_allowed(&u.download_url, &c.allowed_hosts, c.allow_insecure) {
            return Err(UpdateError::Host(u.download_url.clone()));
        }
    }
    Ok(found)
}

fn download_over_network(config: &AppConfig, update: &Update) -> Result<Vec<u8>, UpdateError> {
    let mut c = config.transport(Some(Installer::App));
    c.timeout = Some(DOWNLOAD_TIMEOUT);
    transport::download(&c, update, |_, _| {}, || {})
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

/// Installing failed (the update itself stays ready for another try): a
/// Discard All chosen on the way to the restart is taken back. The app goes
/// on running with those budgets open, so autosave and recovery copies take
/// up again (otherwise nothing would be kept for the rest of the run).
pub fn install_failed(safety: Option<&Safety>) -> Said {
    if let Some(s) = safety {
        crate::safety_app::discard_cancelled(s);
    }
    Said::InstallFailed
}

// ---------------------------------------------------------------------------
// Tauri wiring

fn tell<R: Runtime>(app: &AppHandle<R>, status: Status) {
    let _ = app.emit_to(crate::tools::MAIN, STATUS_EVENT, status);
}

/// Sets everything up when the app starts: puts back a bundle an
/// interrupted update left hidden, reads the settings, decides whether
/// What's New is due, ticks the menu's switch, and starts the timer for
/// automatic checks.
pub fn init<R: Runtime>(app: &AppHandle<R>) {
    let config = update_config();
    if let Ok(exe) = std::env::current_exe() {
        // Leftovers of a swap that was cut short; nothing to say about it.
        let _ = install::sweep_at_startup(&exe, &config.bundle);
    }
    let settings_file = app
        .path()
        .app_local_data_dir()
        .ok()
        .map(|d| d.join("updates.json"));
    let current = app.package_info().version.to_string();
    app.manage(Updates::new(config, &current, settings_file));
    if crate::launch::LaunchArgs::from_env().check_for_updates {
        // Even a build that can't update answers, as the menu item does.
        app.state::<Updates>().ask_to_check_at_start();
    }
    refresh_menu(app);
    if app.state::<Updates>().updater.unavailable().is_some() {
        // Never checks: no timer, no network.
        return;
    }
    let handle = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(TICK);
        if handle.state::<Updates>().updater.due(Instant::now()) {
            check_now(&handle, false);
        }
    });
}

/// One check, then the download if it found something. Blocks: run it on a
/// thread of its own. `manual`: the user asked (Help > Check for Updates).
pub fn check_now<R: Runtime>(app: &AppHandle<R>, manual: bool) {
    let updates = app.state::<Updates>();
    let config = updates.updater.app().clone();
    let current = app.package_info().version.to_string();
    updates.check_with(
        manual,
        || check_over_network(&config, &current),
        |update| download_over_network(&config, update),
        |status| tell(app, status),
    );
}

/// Installs the ready update and starts the app again. Runs on a thread of
/// its own (an /Applications the user can't write asks for an
/// administrator, through the system's own prompt).
fn install_and_restart<R: Runtime>(app: &AppHandle<R>) {
    let updates = app.state::<Updates>();
    if !matches!(updates.updater.phase(), Phase::Ready { .. }) {
        return;
    }
    let mut installing = updates.status(None);
    installing.phase = "installing";
    tell(app, installing);
    // The tabs as they are now (Save All may have given a budget its file).
    if let Some(safety) = app.try_state::<Safety>() {
        safety.remember_tabs(&app.state::<AppState>().docs());
    }
    let names = updates.updater.app().bundle.clone();
    let result = match std::env::current_exe()
        .ok()
        .as_deref()
        .and_then(install::bundle_of_exe)
    {
        Some(bundle) => updates
            .updater
            .install_ready(&bundle, &|b, new| install::admin_swap(b, new, &names)),
        None => Err(InstallError::Io("not running from an app bundle".into())),
    };
    match result {
        // The bundle was swapped in place (Method Budget has never been
        // renamed), so Tauri's own restart starts the new version, after
        // the app's exit work, as before.
        Ok(_) => app.restart(),
        Err(_) => {
            let safety = app.try_state::<Safety>();
            let said = install_failed(safety.as_deref());
            tell(app, updates.status(Some(said)));
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
        crate::i18n::tr("menu.help.check_updates"),
        true,
        None::<&str>,
    )?)?;
    help.append(&CheckMenuItem::with_id(
        app,
        AUTO_ITEM,
        crate::i18n::tr("menu.help.check_updates_auto"),
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
                let _ = u.updater.set_automatic(on);
                tell(app, u.status(None));
            }
            refresh_menu(app);
            true
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Commands (registered in app.rs, build.rs and capabilities/main.json; the
// main window only)

/// Where an update is, for the page's quiet indicator. The page asks once
/// it is listening, at the end of its start: a check the command line asked
/// for (`--check-for-updates`) begins then, as Help > Check for Updates…
#[tauri::command]
pub fn update_status<R: Runtime>(app: AppHandle<R>, updates: State<'_, Updates>) -> Status {
    let status = updates.status(None);
    if updates.take_check_at_start() {
        std::thread::spawn(move || check_now(&app, true));
    }
    status
}

/// Restart to update: saves what can be saved, asks about what can't (as
/// quitting does), then installs the ready update and starts again.
#[tauri::command]
pub fn update_restart<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    if !matches!(app.state::<Updates>().updater.phase(), Phase::Ready { .. }) {
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
    if !updates.updater.take_whats_new() {
        return None;
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
    use method_update::policy::{Machine, Start};
    use std::cell::RefCell;

    const ENDPOINT_HERE: &str =
        "https://github.com/method-production-suite/method-budget/releases/latest/download/latest.json";

    fn leak(s: &str) -> &'static str {
        Box::leak(s.to_string().into_boxed_str())
    }

    /// Method Budget's config with a key of the right shape (no network is
    /// ever used with it here).
    fn config_with(endpoint: &str, pubkey: &str) -> AppConfig {
        AppConfig::new(
            "Method Budget",
            BUNDLE_NAME,
            "method-budget-update/test",
            Baked::release(leak(endpoint), leak(pubkey)),
        )
    }

    fn updates(file: Option<PathBuf>) -> Updates {
        Updates::new(
            config_with(ENDPOINT_HERE, &fixture::public_key(7)),
            "0.3.0",
            file,
        )
    }

    fn update(v: &str) -> Update {
        Update {
            current_version: "0.3.0".into(),
            version: v.into(),
            body: None,
            date: None,
            target: "darwin-aarch64".into(),
            download_url: "https://github.com/x".into(),
            signature: "s".into(),
            raw_json: serde_json::Value::Null,
        }
    }

    fn offline() -> UpdateError {
        UpdateError::Network("offline".into())
    }

    fn intel() -> UpdateError {
        UpdateError::Platform("Updates are for Apple silicon Macs only (`darwin-aarch64`).".into())
    }

    /// Runs one check through the page's side, recording what the page
    /// hears.
    fn run(
        u: &Updates,
        manual: bool,
        check: impl FnOnce() -> Result<Option<Update>, UpdateError>,
        download: impl FnOnce(&Update) -> Result<Vec<u8>, UpdateError>,
    ) -> (Option<Said>, Vec<Status>) {
        let heard = RefCell::new(Vec::new());
        let said = u.check_with(manual, check, download, |s| heard.borrow_mut().push(s));
        (said, heard.into_inner())
    }

    fn phases(heard: &[Status]) -> Vec<(&'static str, bool, Option<Said>)> {
        heard.iter().map(|s| (s.phase, s.manual, s.said)).collect()
    }

    // The state machine (now method_update::policy's), pinned as Method
    // Budget has always used it.

    #[test]
    fn the_state_machine_keeps_automatic_checks_quiet() {
        let mut m = Machine::default();
        assert_eq!(m.start_check(false), Start::Check);
        // Nothing newer, or no network: back to idle, nothing said.
        assert_eq!(m.checked(Ok(None)), (false, None));
        assert_eq!(m.phase, Phase::Idle);
        m.start_check(false);
        assert_eq!(m.checked(Err("offline".into())), (false, None));
        assert_eq!(m.phase, Phase::Idle);
        // Found: downloads quietly, then it's ready.
        m.start_check(false);
        assert_eq!(m.checked(Ok(Some("0.4.0".into()))), (true, None));
        assert_eq!(m.downloaded(Ok(())), None);
        assert!(m.is_ready());
        // A failed download says nothing either, and is tried again later.
        let mut m = Machine::default();
        m.start_check(false);
        m.checked(Ok(Some("0.4.0".into())));
        assert_eq!(m.downloaded(Err("bad signature".into())), None);
        assert_eq!(m.phase, Phase::Idle);
    }

    #[test]
    fn the_state_machine_answers_a_check_the_user_asked_for() {
        let mut m = Machine::default();
        assert_eq!(m.start_check(true), Start::Check);
        assert_eq!(m.checked(Ok(None)), (false, Some(policy::Said::Current)));
        m.start_check(true);
        assert_eq!(
            m.checked(Err("offline".into())),
            (false, Some(policy::Said::Failed))
        );
        m.start_check(true);
        m.checked(Ok(Some("0.4.0".into())));
        assert_eq!(m.downloaded(Err("x".into())), Some(policy::Said::Failed));
        // Asking while an automatic check runs makes it the user's.
        let mut m = Machine::default();
        m.start_check(false);
        assert_eq!(m.start_check(true), Start::Busy);
        assert_eq!(m.checked(Ok(None)), (false, Some(policy::Said::Current)));
        // Asking once an update is ready changes nothing.
        let mut m = Machine::default();
        m.start_check(false);
        m.checked(Ok(Some("0.4.0".into())));
        m.downloaded(Ok(()));
        assert_eq!(m.start_check(true), Start::Ready);
        // Installing, once; a failed install leaves it ready for another try.
        assert_eq!(m.start_install().as_deref(), Some("0.4.0"));
        assert_eq!(m.start_install(), None, "only once");
        assert_eq!(m.install_failed(), policy::Said::InstallFailed);
        assert!(m.is_ready());
    }

    // What the page hears, through Method Budget's side.

    #[test]
    fn automatic_checks_are_quiet() {
        let u = updates(None);
        // No network: the page hears "checking" without manual, then idle,
        // and nothing is said.
        let (said, heard) = run(&u, false, || Err(offline()), |_| unreachable!());
        assert_eq!(said, None);
        assert_eq!(
            phases(&heard),
            [("checking", false, None), ("idle", false, None)]
        );
        // Nothing newer, and no build for this computer: silent too.
        assert_eq!(run(&u, false, || Ok(None), |_| unreachable!()).0, None);
        assert_eq!(run(&u, false, || Err(intel()), |_| unreachable!()).0, None);
        // A download that doesn't verify: silent, back to idle.
        let (said, heard) = run(
            &u,
            false,
            || Ok(Some(update("0.4.0"))),
            |_| Err(UpdateError::Download("bad".into())),
        );
        assert_eq!(said, None);
        assert_eq!(heard.last().map(|s| s.phase), Some("idle"));
        // Found and downloaded: ready, quietly.
        let (said, heard) = run(&u, false, || Ok(Some(update("0.4.0"))), |_| Ok(vec![1, 2]));
        assert_eq!(said, None);
        assert_eq!(
            phases(&heard),
            [
                ("checking", false, None),
                ("downloading", false, None),
                ("ready", false, None)
            ]
        );
        assert_eq!(heard[1].version.as_deref(), Some("0.4.0"));
    }

    #[test]
    fn a_check_the_user_asked_for_answers() {
        let u = updates(None);
        let (said, heard) = run(&u, true, || Ok(None), |_| unreachable!());
        assert_eq!(said, Some(Said::Current));
        assert_eq!(
            phases(&heard),
            [
                ("checking", true, None),
                ("idle", false, Some(Said::Current))
            ]
        );
        // A failure is said once and the indicator goes back to idle (the
        // suite's "failed" phase is not the page's).
        let (said, heard) = run(&u, true, || Err(offline()), |_| unreachable!());
        assert_eq!(said, Some(Said::Failed));
        assert_eq!(heard.last().unwrap().phase, "idle");
        assert_eq!(u.status(None).phase, "idle");
        // The download shows its progress, and its failure is said.
        let (said, heard) = run(
            &u,
            true,
            || Ok(Some(update("0.4.0"))),
            |_| Err(UpdateError::Download("x".into())),
        );
        assert_eq!(said, Some(Said::Failed));
        assert_eq!(heard[1].phase, "downloading");
        assert!(heard[1].manual, "the download shows its progress");
        // Once an update is ready, asking changes nothing and fetches nothing.
        run(&u, false, || Ok(Some(update("0.4.0"))), |_| Ok(vec![1]));
        let (said, heard) = run(&u, true, || unreachable!(), |_| unreachable!());
        assert_eq!(said, None);
        assert_eq!(phases(&heard), [("ready", false, None)]);
    }

    #[test]
    fn a_mac_with_no_build_in_the_manifest_is_told_so() {
        assert_eq!(check_failure(&intel()), CheckFailed::NoBuild);
        for other in [
            offline(),
            UpdateError::ReleaseNotFound,
            UpdateError::EmptyEndpoints,
            UpdateError::Insecure,
            UpdateError::Host("https://example.com/x".into()),
            UpdateError::Manifest("bad".into()),
            UpdateError::Download("x".into()),
        ] {
            assert_eq!(check_failure(&other), CheckFailed::Failed, "{other}");
        }
        // Asked for: the page says so. Automatic: nothing.
        let u = updates(None);
        let (said, heard) = run(&u, true, || Err(intel()), |_| unreachable!());
        assert_eq!(said, Some(Said::NoBuild));
        assert_eq!(heard.last().unwrap().phase, "idle");
        assert_eq!(run(&u, false, || Err(intel()), |_| unreachable!()).0, None);
        // And the next failure is an ordinary one again.
        assert_eq!(
            run(&u, true, || Err(offline()), |_| unreachable!()).0,
            Some(Said::Failed)
        );
        // The page's key for it (ui/js/updates.js, TEXT), and its words
        // (the string table, ui/i18n/en.js).
        assert_eq!(
            serde_json::to_value(Said::NoBuild).ok(),
            Some(serde_json::json!("no-build"))
        );
        assert!(UPDATES_JS.contains("'no-build': t('updates.no_build'),"));
        assert_eq!(
            crate::i18n::tr("updates.no_build"),
            "There’s no update for this computer yet. This version stays as it is."
        );
    }

    /// The real check against a manifest served here: an Intel Mac (asked
    /// for its own keys) is refused with `Platform`, which the page says as
    /// "no build"; an Apple silicon Mac gets the update from the same file.
    #[test]
    fn an_intel_mac_reading_a_real_manifest_has_no_build() {
        use method_update::target::TargetSet;
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let manifest = serde_json::json!({
            "version": "0.4.0",
            "notes": "Notes",
            "pub_date": "2026-10-01T00:00:00Z",
            "platforms": {
                "darwin-aarch64": {
                    "signature": fixture::signature(7, "Method-Budget_0.4.0_aarch64.app.tar.gz"),
                    "url": format!("http://127.0.0.1:{port}/Method-Budget_0.4.0_aarch64.app.tar.gz")
                }
            }
        })
        .to_string();
        let server = std::thread::spawn(move || {
            for _ in 0..2 {
                let (mut s, _) = listener.accept().unwrap();
                let mut buf = [0u8; 4096];
                let _ = s.read(&mut buf);
                let reply = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{manifest}",
                    manifest.len()
                );
                s.write_all(reply.as_bytes()).unwrap();
            }
        });
        let config = AppConfig::new(
            "Method Budget",
            BUNDLE_NAME,
            "method-budget-update/test",
            Baked {
                endpoint: leak(&format!("http://127.0.0.1:{port}/latest.json")),
                pubkey: leak(&fixture::public_key(7)),
                insecure: true,
                rehearsal: true,
            },
        );
        let version = method_update::manifest::parse_version("0.3.0").unwrap();
        let mut c = config.transport(Some(Installer::App));
        c.targets = Some(TargetSet::new("darwin", "x86_64", Some(Installer::App)));
        let err = transport::check(&c, &version).unwrap_err();
        assert_eq!(check_failure(&err), CheckFailed::NoBuild, "{err}");
        c.targets = Some(TargetSet::new("darwin", "aarch64", Some(Installer::App)));
        let found = transport::check(&c, &version).unwrap().unwrap();
        assert_eq!(found.version, "0.4.0");
        assert_eq!(found.target, "darwin-aarch64");
        server.join().unwrap();
        // Through the page's side: said as no build.
        let u = updates(None);
        assert_eq!(
            run(&u, true, || Err(err), |_| unreachable!()).0,
            Some(Said::NoBuild)
        );
    }

    /// A universal release's manifest lists the same archive under both Mac keys
    /// (crates/xtask/src/manifest.rs), so an Intel Mac finds the update too.
    #[test]
    fn an_intel_mac_reading_a_universal_manifest_finds_the_update() {
        use method_update::target::TargetSet;
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let name = "Method-Budget_0.6.0_universal.app.tar.gz";
        let entry = serde_json::json!({
            "signature": fixture::signature(7, name),
            "url": format!("http://127.0.0.1:{port}/{name}")
        });
        let manifest = serde_json::json!({
            "version": "0.6.0",
            "notes": "Notes",
            "pub_date": "2026-10-20T00:00:00Z",
            "platforms": { "darwin-aarch64": entry, "darwin-x86_64": entry }
        })
        .to_string();
        let server = std::thread::spawn(move || {
            for _ in 0..2 {
                let (mut s, _) = listener.accept().unwrap();
                let mut buf = [0u8; 4096];
                let _ = s.read(&mut buf);
                let reply = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{manifest}",
                    manifest.len()
                );
                s.write_all(reply.as_bytes()).unwrap();
            }
        });
        let config = AppConfig::new(
            "Method Budget",
            BUNDLE_NAME,
            "method-budget-update/test",
            Baked {
                endpoint: leak(&format!("http://127.0.0.1:{port}/latest.json")),
                pubkey: leak(&fixture::public_key(7)),
                insecure: true,
                rehearsal: true,
            },
        );
        let version = method_update::manifest::parse_version("0.5.0").unwrap();
        let mut c = config.transport(Some(Installer::App));
        for (arch, key) in [("x86_64", "darwin-x86_64"), ("aarch64", "darwin-aarch64")] {
            c.targets = Some(TargetSet::new("darwin", arch, Some(Installer::App)));
            let found = transport::check(&c, &version).unwrap().unwrap();
            assert_eq!(found.version, "0.6.0", "{arch}");
            assert_eq!(found.target, key, "{arch}");
        }
        server.join().unwrap();
    }

    #[test]
    fn installing() {
        let u = updates(None);
        assert!(!matches!(u.updater.phase(), Phase::Ready { .. }));
        run(
            &u,
            false,
            || Ok(Some(update("0.4.0"))),
            |_| Ok(b"not gzip".to_vec()),
        );
        assert!(matches!(u.updater.phase(), Phase::Ready { .. }));
        // An install that fails (here: not an archive) leaves it ready, and
        // the old bundle untouched.
        let apps = temp_dir("updates-install");
        let bundle = apps.join(BUNDLE_NAME);
        std::fs::create_dir_all(bundle.join("Contents")).unwrap();
        let no_admin = |_: &Path, _: &Path| -> Result<(), InstallError> { unreachable!() };
        assert!(u.updater.install_ready(&bundle, &no_admin).is_err());
        assert!(matches!(u.updater.phase(), Phase::Ready { .. }));
        assert!(bundle.join("Contents").is_dir());
        // What the page hears then: the same five fields as always.
        let s = serde_json::to_value(u.status(Some(install_failed(None)))).unwrap();
        assert_eq!(
            s,
            serde_json::json!({ "phase": "ready", "version": "0.4.0", "manual": false, "said": "install-failed", "automatic": true })
        );
        std::fs::remove_dir_all(&apps).unwrap();
    }

    #[test]
    fn the_page_hears_only_its_own_phases_and_answers() {
        let base = policy::Status {
            phase: "idle",
            version: None,
            manual: false,
            message: None,
            said: None,
            automatic: true,
            available: true,
        };
        for (suite, page) in [
            ("idle", "idle"),
            ("checking", "checking"),
            ("downloading", "downloading"),
            ("ready", "ready"),
            ("installing", "installing"),
            ("failed", "idle"),
            ("read-only", "idle"),
        ] {
            let s = policy::Status {
                phase: suite,
                ..base.clone()
            };
            assert_eq!(page_status(&s, None).phase, page, "{suite}");
        }
        // Every phase and answer the page can be told is one it knows
        // (ui/js/updates.js, PHASES and TEXT).
        for p in ["idle", "checking", "downloading", "ready", "installing"] {
            assert!(UPDATES_JS.contains(&format!("'{p}'")), "{p}");
        }
        for (said, failed, page) in [
            (policy::Said::Current, None, Said::Current),
            (policy::Said::Failed, None, Said::Failed),
            (
                policy::Said::Failed,
                Some(CheckFailed::Failed),
                Said::Failed,
            ),
            (
                policy::Said::Failed,
                Some(CheckFailed::NoBuild),
                Said::NoBuild,
            ),
            (policy::Said::InstallFailed, None, Said::InstallFailed),
            (policy::Said::ReadOnly, None, Said::Failed),
        ] {
            assert_eq!(page_said(Some(said), failed), Some(page));
            let key = serde_json::to_value(page).unwrap();
            let key = key.as_str().unwrap();
            assert!(
                UPDATES_JS.contains(&format!("  {key}: "))
                    || UPDATES_JS.contains(&format!("'{key}': ")),
                "{key} has no words in updates.js"
            );
        }
        assert_eq!(page_said(None, Some(CheckFailed::NoBuild)), None);
    }

    #[test]
    fn the_quiet_check_comes_two_minutes_after_launch_then_every_six_hours() {
        assert_eq!(FIRST_CHECK, Duration::from_secs(120));
        assert_eq!(CHECK_EVERY, Duration::from_secs(6 * 60 * 60));
        assert!(TICK < FIRST_CHECK);
        let u = updates(None);
        let t0 = Instant::now();
        assert!(!u.updater.due(t0), "not at launch");
        assert!(u.updater.due(t0 + FIRST_CHECK + Duration::from_secs(1)));
        assert!(!u.updater.due(t0 + FIRST_CHECK + Duration::from_secs(60)));
        assert!(u
            .updater
            .due(t0 + FIRST_CHECK + CHECK_EVERY + Duration::from_secs(2)));
        // Turned off: never due.
        let off = updates(None);
        off.updater.set_automatic(false).unwrap();
        assert!(!off.updater.due(t0 + FIRST_CHECK + Duration::from_secs(1)));
    }

    #[test]
    fn no_real_key_means_no_checks() {
        let reason = |key: &str, endpoint: &str| config_with(endpoint, key).unavailable_reason();
        assert!(reason(PLACEHOLDER_PUBLIC_KEY, ENDPOINT_HERE).is_some());
        assert!(reason("", ENDPOINT_HERE).is_some());
        assert!(reason("garbage", ENDPOINT_HERE).is_some());
        let key = fixture::public_key(7);
        assert_eq!(reason(&key, ENDPOINT_HERE), None);
        assert!(reason(&key, "").is_some());
        assert!(reason(&key, "http://github.com/x").is_some());
        // A build that can't update never checks, and a manual check says
        // it couldn't.
        let u = Updates::new(
            config_with(ENDPOINT_HERE, PLACEHOLDER_PUBLIC_KEY),
            "0.3.0",
            None,
        );
        assert!(!u
            .updater
            .due(Instant::now() + FIRST_CHECK + Duration::from_secs(1)));
        assert_eq!(
            run(&u, false, || unreachable!(), |_| unreachable!()).0,
            None
        );
        let (said, heard) = run(&u, true, || unreachable!(), |_| unreachable!());
        assert_eq!(said, Some(Said::Failed));
        assert_eq!(phases(&heard), [("idle", false, Some(Said::Failed))]);

        // The endpoint and key the app is built with are tauri.conf.json's,
        // copied by build.rs: written in one place. Once the key is real, it
        // must be a key.
        let conf: serde_json::Value = serde_json::from_str(CONF).unwrap();
        let up = &conf["plugins"]["updater"];
        let pubkey = up["pubkey"].as_str().unwrap();
        assert_eq!(up["endpoints"], serde_json::json!([ENDPOINT_HERE]));
        let app = update_config();
        assert_eq!(app.endpoint, ENDPOINT_HERE);
        assert_eq!(app.pubkey, pubkey);
        assert_eq!((ENDPOINT, PUBKEY), (ENDPOINT_HERE, pubkey));
        if pubkey != PLACEHOLDER_PUBLIC_KEY {
            assert_eq!(app.unavailable_reason(), None);
        }
        assert!(!app.allow_insecure);
        assert_eq!(app.bundle.current, "Method Budget.app");
        assert_eq!(app.bundle.legacy, None);
        assert_eq!(
            format!("{}.app", conf["productName"].as_str().unwrap()),
            BUNDLE_NAME
        );
        // Signatures must name the version the manifest announces.
        assert!(app.transport(Some(Installer::App)).require_signed_version);
        // Never loosened.
        for k in [
            "dangerousInsecureTransportProtocol",
            "dangerousAcceptInvalidCerts",
            "dangerousAcceptInvalidHostnames",
            "allowDowngrades",
        ] {
            assert!(up.get(k).is_none(), "{k}");
        }
        // The key is written once: nowhere in the source but tauri.conf.json.
        assert!(!include_str!("updater.rs").contains(&pubkey[..40]));
    }

    #[test]
    fn only_the_endpoint_hosts_over_https() {
        let app = config_with(ENDPOINT_HERE, &fixture::public_key(7));
        let hosts = app.allowed_hosts();
        assert_eq!(
            hosts,
            [
                "github.com",
                "objects.githubusercontent.com",
                "release-assets.githubusercontent.com"
            ]
        );
        let ok = |url: &str| transport::host_allowed(url, &hosts, false);
        assert!(ok("https://github.com/x"));
        assert!(ok("https://release-assets.githubusercontent.com/x"));
        assert!(!ok("http://github.com/x"));
        assert!(!ok("https://example.com/x"));
        assert!(!ok("https://github.com.example.com/x"));
        assert!(!ok("not a url"));
        let other = config_with("https://updates.example.org/latest.json", "k");
        assert_eq!(other.allowed_hosts(), ["updates.example.org"]);
        // A manifest pointing the download anywhere else fails the check
        // before anything is fetched.
        let u = updates(None);
        let mut elsewhere = update("0.4.0");
        elsewhere.download_url = "https://example.com/evil.tar.gz".into();
        assert!(!transport::host_allowed(
            &elsewhere.download_url,
            &u.updater.app().allowed_hosts(),
            false
        ));
    }

    #[test]
    fn whats_new_once_after_an_update() {
        use policy::whats_new_due;
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

        // Through the app: a first run writes this version down at once; the
        // next version shows What's New once.
        let dir = temp_dir("updates-whats-new");
        let file = dir.join("updates.json");
        let config = || config_with(ENDPOINT_HERE, &fixture::public_key(7));
        let first = Updates::new(config(), "0.3.0", Some(file.clone()));
        assert!(
            !first.updater.take_whats_new(),
            "a first run is not an update"
        );
        assert_eq!(Settings::load(&file).seen.as_deref(), Some("0.3.0"));
        let next = Updates::new(config(), "0.4.0", Some(file.clone()));
        assert!(next.updater.take_whats_new());
        assert!(!next.updater.take_whats_new(), "once");
        assert_eq!(Settings::load(&file).seen.as_deref(), Some("0.4.0"));
        let again = Updates::new(config(), "0.4.0", Some(file.clone()));
        assert!(!again.updater.take_whats_new());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_check_asked_for_on_the_command_line_starts_once() {
        let u = Updates::new(
            config_with(ENDPOINT_HERE, &fixture::public_key(7)),
            "0.4.0",
            None,
        );
        assert!(!u.take_check_at_start(), "not unless asked");
        u.ask_to_check_at_start();
        assert!(u.take_check_at_start());
        assert!(!u.take_check_at_start(), "once");
    }

    #[test]
    fn settings_are_kept() {
        let dir = temp_dir("updates-settings");
        let file = dir.join("updates.json");
        assert_eq!(Settings::load(&file), Settings::default());
        assert!(Settings::default().automatic, "on by default");
        // The file Method Budget has always written reads the same.
        std::fs::write(
            &file,
            "{\n  \"automatic\": false,\n  \"seen\": \"0.1.0\"\n}",
        )
        .unwrap();
        let s = Settings::load(&file);
        assert_eq!(
            s,
            Settings {
                automatic: false,
                seen: Some("0.1.0".into())
            }
        );
        s.save(&file).unwrap();
        assert_eq!(Settings::load(&file), s);
        std::fs::write(&file, "{\"seen\": \"0.1.0\"}").unwrap();
        assert!(
            Settings::load(&file).automatic,
            "a missing field is its default"
        );
        // The switch, through the app, is kept across launches.
        std::fs::remove_file(&file).unwrap();
        let u = updates(Some(file.clone()));
        assert!(u.automatic(), "on by default");
        u.updater.set_automatic(false).unwrap();
        assert!(!updates(Some(file.clone())).automatic());
        // A file this build can't read is never written over on its own.
        std::fs::write(&file, "{ not json").unwrap();
        let u = updates(Some(file.clone()));
        assert!(u.automatic(), "the default, for this run");
        run(&u, false, || Ok(None), |_| unreachable!());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "{ not json");
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
        let u = updates(None);
        run(
            &u,
            false,
            || Ok(Some(update("0.4.0"))),
            |_| Ok(b"not gzip".to_vec()),
        );
        let bundle = dir.join(BUNDLE_NAME);
        std::fs::create_dir_all(bundle.join("Contents")).unwrap();
        let no_admin = |_: &Path, _: &Path| -> Result<(), InstallError> { unreachable!() };
        assert!(u.updater.install_ready(&bundle, &no_admin).is_err());
        assert_eq!(install_failed(Some(&safety)), Said::InstallFailed);
        assert!(
            matches!(u.updater.phase(), Phase::Ready { .. }),
            "ready for another try"
        );
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
    const CARGO_TOML: &str = include_str!("../Cargo.toml");
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
        // No updater plugin at all: no window can be granted its commands.
        for caps in [MAIN_CAPS, TOOLS_CAPS, SETUP_CAPS] {
            assert!(
                !caps.contains("updater:"),
                "a window is granted updater commands"
            );
        }
        assert!(!APP_RS.contains("tauri_plugin_updater"));
        assert!(!CARGO_TOML.contains("tauri-plugin-updater"));
        // method-update is pinned to a full commit (the crate-age check and
        // cargo deny allow method-libs only that way).
        let line = CARGO_TOML
            .lines()
            .find(|l| l.starts_with("method-update"))
            .unwrap();
        let rev = line
            .split("rev = \"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap();
        assert!(
            rev.len() == 40 && rev.bytes().all(|b| b.is_ascii_hexdigit()),
            "{line}"
        );
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
