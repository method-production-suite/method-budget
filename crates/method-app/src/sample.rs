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
//!   says so (never shown, nothing open, no budgets of the person's own on
//!   the recent or pinned lists). It then takes the start window's place.
//!   A start that finds the person already at work marks it seen unshown.
//! - **`--open-sample`** on the command line (src/launch.rs; Method
//!   Launcher starts the app this way): `sample_at_launch` opens it whatever
//!   else is true, once, again in the start window's place.
//! - **Help > Open Sample** (`sample_open`): a fresh copy any time, in a new
//!   tab (or in the place of an untouched Untitled budget alone in the
//!   window, as Open does).
//!
//! There is no setting for it (decisions.md, entry 86). Every way of
//! showing the sample marks it seen (`sample_seen` in `settings.json`,
//! src/safety.rs), so a start never opens it again.

use method_session::sample::{self, Launch};
use tauri::{Runtime, State, WebviewWindow};

use crate::app::{finish, AppState};
use crate::safety_app::Safety;
use crate::start::StartState;
use crate::view::View;

/// The sample's file, built into the app.
pub const BYTES: &[u8] = include_bytes!("../../../fixtures/field-station/field-station.mthd");

/// Help > Open Sample: a menu item the page answers (it asks for
/// [`sample_open`]).
pub const OPEN_ITEM: &str = "help.open_sample";
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
    // Asked for on the command line (`--open-sample`, src/launch.rs): taken
    // here, so once.
    let asked = safety.take_sample_asked();
    let launch = Launch {
        seen: safety.sample_seen(),
        nothing_open: state.docs().lone_untouched().is_some(),
        own_budgets: !safety.recent_paths().is_empty() || !start.pinned().is_empty(),
    };
    match at_start(asked, launch) {
        AtStart::Open => open(&window, &state, &safety).map(Some),
        AtStart::MarkSeen => {
            safety.mark_sample_seen();
            Ok(None)
        }
        AtStart::Nothing => Ok(None),
    }
}

/// What the start does about the sample.
#[derive(Debug, PartialEq, Eq)]
enum AtStart {
    /// Opens a copy (which marks it seen).
    Open,
    /// Leaves it unshown and marks it seen: the person is already at work.
    MarkSeen,
    Nothing,
}

/// `asked`: `--open-sample` was on the command line, which opens it
/// whatever else is true. Else method-session's rules.
fn at_start(asked: bool, launch: Launch) -> AtStart {
    if asked || sample::at_launch(launch) {
        AtStart::Open
    } else if sample::retire_at_launch(launch) {
        AtStart::MarkSeen
    } else {
        AtStart::Nothing
    }
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

    fn first_start() -> Launch {
        Launch {
            seen: false,
            nothing_open: true,
            own_budgets: false,
        }
    }

    #[test]
    fn the_first_start_opens_it_once() {
        assert_eq!(at_start(false, first_start()), AtStart::Open);
        // Shown before: never again unasked.
        let seen = Launch {
            seen: true,
            ..first_start()
        };
        assert_eq!(at_start(false, seen), AtStart::Nothing);
    }

    #[test]
    fn someone_already_at_work_has_it_marked_seen_unshown() {
        for l in [
            Launch {
                own_budgets: true,
                ..first_start()
            },
            Launch {
                nothing_open: false,
                ..first_start()
            },
        ] {
            assert_eq!(at_start(false, l), AtStart::MarkSeen, "{l:?}");
        }
    }

    #[test]
    fn open_sample_on_the_command_line_opens_it_whatever_else_is_true() {
        for seen in [false, true] {
            for nothing_open in [false, true] {
                for own_budgets in [false, true] {
                    let l = Launch {
                        seen,
                        nothing_open,
                        own_budgets,
                    };
                    assert_eq!(at_start(true, l), AtStart::Open, "{l:?}");
                }
            }
        }
    }

    #[test]
    fn there_is_no_switch_for_it_in_the_help_menu() {
        let src = include_str!("sample.rs");
        let menu = include_str!("menu.rs");
        let app = include_str!("app.rs");
        let needle = ["help.sample", "_at_launch"].concat();
        assert!(!src.contains(&needle) && !menu.contains(&needle));
        assert!(!app.contains(&["crate::sample::", "handle_menu"].concat()));
        assert!(!menu.contains(&["crate::sample::", "extend_help_menu"].concat()));
        assert!(!src.contains(&["First ", "Launch"].concat()));
    }

    #[test]
    fn the_bundled_sample_is_field_station() {
        let s = sample::session(BYTES).unwrap();
        assert_eq!(s.budget().title, "Field Station");
        assert_eq!(s.tab_label(), "Sample: Field Station");
        assert!(s.is_untouched() && s.path().is_none());
    }
}
