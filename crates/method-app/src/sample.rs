//! The sample budget in the desktop app (SUITE-STANDARDS.md, section 8;
//! decisions.md, entry 84; docs/APP.md, "The sample budget").
//!
//! - **Bundled at build time**: [`BYTES`] is Field Station's file
//!   (fixtures/field-station/field-station.mthd, made by
//!   scripts/make-field-station.py). It is only ever read: each opening is a
//!   fresh untitled copy (method-session's `sample::session`), marked
//!   "Sample" in its tab, with a line saying so and how to start your own.
//! - **At the start** (`sample_at_launch`, asked by the page after the files
//!   to open and the budgets to recover): opened in the place of the
//!   untouched Untitled budget when method-session's `sample::at_launch`
//!   says so (the setting on, never shown, nothing open, no budgets of the
//!   person's own on the recent or pinned lists). It then takes the start
//!   window's place.
//! - **`--open-sample`** on the command line (src/launch.rs; Method
//!   Launcher starts the app this way): `sample_at_launch` opens it whatever
//!   the setting says, once, again in the start window's place.
//! - **Help > Open Sample** (`sample_open`): a fresh copy any time, in a new
//!   tab (or in the place of an untouched Untitled budget alone in the
//!   window, as Open does).
//! - **Help > Open the Sample on First Launch**: the setting, a check kept
//!   in `settings.json` beside Autosave (src/safety.rs), on by default.
//!   Either way of showing the sample marks it seen.

use method_session::sample::{self, Launch};
use tauri::menu::{CheckMenuItem, MenuItemKind, Submenu};
use tauri::{AppHandle, Manager, Runtime, State, WebviewWindow};

use crate::app::{finish, AppState};
use crate::safety_app::Safety;
use crate::start::StartState;
use crate::view::View;

/// The sample's file, built into the app.
pub const BYTES: &[u8] = include_bytes!("../../../fixtures/field-station/field-station.mthd");

/// Help > Open Sample: a menu item the page answers (it asks for
/// [`sample_open`]).
pub const OPEN_ITEM: &str = "help.open_sample";
/// Help > Open the Sample on First Launch: a check answered here.
pub const AT_LAUNCH_ITEM: &str = "help.sample_at_launch";

/// Adds the setting's check right after Open Sample in the Help menu.
/// Called by `menu::build`.
pub fn extend_help_menu<R: Runtime>(app: &AppHandle<R>, help: &Submenu<R>) -> tauri::Result<()> {
    let on = app
        .try_state::<Safety>()
        .is_none_or(|s| s.sample_settings().0);
    let check = CheckMenuItem::with_id(
        app,
        AT_LAUNCH_ITEM,
        "Open the Sample on First Launch",
        true,
        on,
        None::<&str>,
    )?;
    crate::safety_app::insert_after(help, OPEN_ITEM, &[&check])
}

/// The setting's check in the menu, turned over. False for any other item
/// (the caller passes it on).
pub fn handle_menu<R: Runtime>(app: &AppHandle<R>, id: &str) -> bool {
    if id != AT_LAUNCH_ITEM {
        return false;
    }
    let Some(safety) = app.try_state::<Safety>() else {
        return true;
    };
    let on = !safety.sample_settings().0;
    safety.set_sample_at_launch(on);
    if let Some(MenuItemKind::Check(c)) = app
        .menu()
        .and_then(|m| crate::safety_app::find(m.items().unwrap_or_default(), AT_LAUNCH_ITEM))
    {
        let _ = c.set_checked(on);
    }
    true
}

/// A fresh copy of the sample in front: in the place of an untouched
/// Untitled budget alone in the window, else in a new tab. Marks it seen.
fn open<R: Runtime>(
    window: &WebviewWindow<R>,
    state: &AppState,
    safety: &Safety,
) -> Result<View, String> {
    let s = sample::session(BYTES)?;
    let instead = state.docs().lone_untouched();
    let id = crate::safety_app::put_in_front(state, safety, s, instead)?;
    safety.mark_sample_seen();
    Ok(finish(window, &mut state.lock_doc(Some(id))?, None))
}

/// At the start, after the files to open and the budgets to recover: opens
/// the sample when it should (see the module's notes); `None` when not.
#[tauri::command]
pub fn sample_at_launch<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    safety: State<'_, Safety>,
    start: State<'_, StartState>,
) -> Result<Option<View>, String> {
    // Asked for on the command line (`--open-sample`, src/launch.rs): opened
    // now whatever the setting says, and only here, so once.
    if safety.take_sample_asked() {
        return open(&window, &state, &safety).map(Some);
    }
    let (setting_on, seen) = safety.sample_settings();
    let launch = Launch {
        setting_on,
        seen,
        nothing_open: state.docs().lone_untouched().is_some(),
        own_budgets: !safety.recent_paths().is_empty() || !start.pinned().is_empty(),
    };
    if sample::retire_at_launch(launch) {
        safety.mark_sample_seen();
    }
    if !sample::at_launch(launch) {
        return Ok(None);
    }
    open(&window, &state, &safety).map(Some)
}

/// Help > Open Sample: a fresh untitled copy of the sample, any time.
#[tauri::command]
pub fn sample_open<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    safety: State<'_, Safety>,
) -> Result<View, String> {
    open(&window, &state, &safety)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundled_sample_is_field_station() {
        let s = sample::session(BYTES).unwrap();
        assert_eq!(s.budget().title, "Field Station");
        assert_eq!(s.tab_label(), "Sample: Field Station");
        assert!(s.is_untouched() && s.path().is_none());
    }
}
