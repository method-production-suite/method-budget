//! The Budget Setup window: every setup table's full editor in a window of
//! its own (decisions.md, entry 42).
//!
//! It is a sibling of the tools window (src/tools.rs), built the same way: a
//! second web view labelled [`SETUP`] on the app's own page, here with
//! `?panel=setup` ([`SETUP_PAGE`]), drawing only the setup tabs
//! (`ui/js/setupwin.js`). It calls the same commands on the same budgets in
//! `AppState` and follows the budget in front (the target pane's, when the
//! window is split): every command it sends names the budget it drew, and
//! [`crate::tools::VIEW_CHANGED`] reaches it like any other window, so it
//! fetches a fresh view after edits made elsewhere and when another tab comes
//! to the front.
//!
//! Its own capability (`capabilities/setup.json`) grants only what the setup
//! tabs call; a test below keeps the two in step.
//!
//! Events from Rust, to the main window:
//! - [`SETUP_GEOMETRY`] when the window moves or is resized (`{x, y, width,
//!   height}` in logical pixels), which the main window remembers and passes
//!   back to [`setup_window`] next time.
//! - [`SETUP_CLOSED`] when it goes; the main window puts focus back in the grid.
//!
//! The pages also talk directly, each listening under its own label: the main
//! window sends `setup-state` (the tab to show and a control to focus), the
//! setup window sends `setup-ready`, `setup-tab` (the tab it shows, which the
//! main window remembers), `setup-find` (an item's line count was clicked:
//! the main window's Find tab shows its uses) and `menu` (keys and menu items
//! for the main window).

use tauri::{
    AppHandle, Emitter, Manager, Runtime, WebviewUrl, WebviewWindowBuilder, Window, WindowEvent,
};

use crate::tools::{placement_sized, screen_areas, Geometry, MAIN};

/// The Budget Setup window's label; `capabilities/setup.json` names it.
pub const SETUP: &str = "setup";
/// The page it loads: the app's own page, drawing only the setup tabs.
pub const SETUP_PAGE: &str = "index.html?panel=setup";
/// Its title.
pub const SETUP_TITLE: &str = "Budget Setup";

pub const SETUP_CLOSED: &str = "setup-closed";
pub const SETUP_GEOMETRY: &str = "setup-geometry";

/// Size of a new Budget Setup window, and the smallest it can be (logical
/// pixels): room for a table's columns and the library beside them.
pub const SETUP_DEFAULT_SIZE: (f64, f64) = (1060.0, 720.0);
pub const SETUP_MIN_SIZE: (f64, f64) = (560.0, 380.0);

/// True when the Budget Setup window exists and has focus.
pub fn in_front<R: Runtime>(app: &AppHandle<R>) -> bool {
    app.get_webview_window(SETUP)
        .is_some_and(|w| w.is_focused().unwrap_or(false))
}

/// Where a menu command goes: the window in front of the app's three.
pub fn menu_target(tools_in_front: bool, setup_in_front: bool) -> &'static str {
    if setup_in_front {
        SETUP
    } else {
        crate::tools::menu_target(tools_in_front)
    }
}

/// Shows the Budget Setup window (made the first time, where `geometry`
/// says, else centered at the default size) and brings it to the front; with
/// `show` false, hides it. Answers whether it was already there (its page is
/// then listening; a new one says `setup-ready` once it is). Async because
/// making a window from a synchronous command can deadlock on Windows.
#[tauri::command]
pub async fn setup_window<R: Runtime>(
    app: AppHandle<R>,
    show: bool,
    geometry: Option<Geometry>,
) -> Result<bool, String> {
    if let Some(w) = app.get_webview_window(SETUP) {
        let shown = if show {
            w.show()
                .and_then(|()| w.unminimize())
                .and_then(|()| w.set_focus())
        } else {
            w.hide()
        };
        return shown.map(|()| true).map_err(|e| e.to_string());
    }
    if !show {
        return Ok(false);
    }
    let ((width, height), at) = placement_sized(
        geometry,
        &screen_areas(&app),
        SETUP_DEFAULT_SIZE,
        SETUP_MIN_SIZE,
    );
    let mut builder = WebviewWindowBuilder::new(&app, SETUP, WebviewUrl::App(SETUP_PAGE.into()))
        .title(SETUP_TITLE)
        .inner_size(width, height)
        .min_inner_size(SETUP_MIN_SIZE.0, SETUP_MIN_SIZE.1);
    builder = match at {
        Some((x, y)) => builder.position(x, y),
        None => builder.center(),
    };
    let window = builder.build().map_err(|e| e.to_string())?;
    // Windows and Linux give every window the app's menu bar; this window's
    // keys are answered by its page (and app keys passed to the main window).
    #[cfg(not(target_os = "macos"))]
    let _ = window.remove_menu();
    #[cfg(target_os = "macos")]
    let _ = window;
    Ok(false)
}

/// Closes the Budget Setup window, if there is one.
#[tauri::command]
pub fn close_setup_window<R: Runtime>(app: AppHandle<R>) {
    if let Some(w) = app.get_webview_window(SETUP) {
        let _ = w.close();
    }
}

/// Window events that concern the Budget Setup window: its place and size go
/// to the main window to remember; when it closes the main window comes to
/// the front (and puts focus in its grid); closing the main window takes it
/// along (a setup window alone would keep the app running with no budget).
pub fn on_window_event<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    match (window.label(), event) {
        (SETUP, WindowEvent::Moved(_) | WindowEvent::Resized(_)) => {
            if let Some(g) = crate::tools::geometry_of(window) {
                let _ = window.emit_to(MAIN, SETUP_GEOMETRY, g);
            }
        }
        (SETUP, WindowEvent::Destroyed) => {
            if let Some(main) = window.app_handle().get_webview_window(MAIN) {
                let _ = main.set_focus();
            }
            let _ = window.emit_to(MAIN, SETUP_CLOSED, ());
        }
        (MAIN, WindowEvent::Destroyed) => {
            if let Some(setup) = window.app_handle().get_webview_window(SETUP) {
                let _ = setup.destroy();
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::{placement, Area, TOOLS};

    #[test]
    fn the_setup_page_is_the_bundled_page() {
        let url = tauri::Url::parse("tauri://localhost/")
            .unwrap()
            .join(SETUP_PAGE)
            .unwrap();
        assert_eq!(url.as_str(), "tauri://localhost/index.html?panel=setup");
        assert_eq!(url.path(), "/index.html");
        // boot.js picks the page's script from the same value.
        let boot = include_str!("../ui/js/boot.js");
        assert!(
            boot.contains("'setup'"),
            "boot.js doesn't know ?panel=setup"
        );
    }

    #[test]
    fn menu_commands_go_to_the_window_in_front() {
        assert_eq!(menu_target(false, true), SETUP);
        assert_eq!(menu_target(true, false), TOOLS);
        assert_eq!(menu_target(false, false), MAIN);
    }

    #[test]
    fn the_setup_window_has_its_own_sizes() {
        const SCREEN: Area = Area {
            x: 0.0,
            y: 25.0,
            width: 1440.0,
            height: 875.0,
        };
        let none = placement_sized(None, &[SCREEN], SETUP_DEFAULT_SIZE, SETUP_MIN_SIZE);
        assert_eq!(none, (SETUP_DEFAULT_SIZE, None));
        let small = Geometry {
            x: 10.0,
            y: 40.0,
            width: 100.0,
            height: 100.0,
        };
        let (size, at) =
            placement_sized(Some(small), &[SCREEN], SETUP_DEFAULT_SIZE, SETUP_MIN_SIZE);
        assert_eq!(size, SETUP_MIN_SIZE);
        assert_eq!(at, Some((10.0, 40.0)));
        // The tools window's own placement is unchanged.
        assert_eq!(placement(Some(small), &[SCREEN]).0, crate::tools::MIN_SIZE);
    }

    const APP_RS: &str = include_str!("app.rs");
    const BUILD_RS: &str = include_str!("../build.rs");
    const MAIN_CAPS: &str = include_str!("../capabilities/main.json");
    const SETUP_CAPS: &str = include_str!("../capabilities/setup.json");
    const CONF: &str = include_str!("../tauri.conf.json");

    /// The app's own commands, as build.rs lists them.
    fn commands() -> Vec<String> {
        let list = &BUILD_RS[BUILD_RS.find("const COMMANDS").unwrap()..];
        let list = &list[..list.find("];").unwrap()];
        list.lines()
            .map(str::trim)
            .filter(|l| l.starts_with('"'))
            .map(|l| l.trim_end_matches(',').trim_matches('"').to_string())
            .collect()
    }

    fn permissions(caps: &str) -> Vec<String> {
        let v: serde_json::Value = serde_json::from_str(caps).unwrap();
        v["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p.as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn setup_window_commands_are_wired() {
        for c in ["setup_window", "close_setup_window"] {
            assert!(
                APP_RS.contains(&format!("crate::setupwin::{c},")),
                "{c} not in generate_handler!"
            );
            assert!(commands().contains(&c.to_string()), "{c} not in build.rs");
            let allow = format!("allow-{}", c.replace('_', "-"));
            assert!(
                permissions(MAIN_CAPS).contains(&allow),
                "{c} not allowed for the main window"
            );
        }
        let conf: serde_json::Value = serde_json::from_str(CONF).unwrap();
        let caps = conf["app"]["security"]["capabilities"].as_array().unwrap();
        assert!(
            caps.iter().any(|c| c == "setup"),
            "the setup capability is not in force"
        );
    }

    /// The apply half of apply.js: the setup window draws its lists as setup
    /// tables (no selection, so no summary and no applying), and may not call these.
    const APPLY_ONLY: &[&str] = &[
        "apply_summary",
        "apply_to_rows",
        "assign_to_rows",
        "clear_from_rows",
        "replace_tags_on_rows",
        "make_fringe_range",
        "clear_fringe_range",
    ];

    /// The Budget Setup window may call only what the setup tabs call, and
    /// only from that window: no files, saving, undo, printing, clipboard,
    /// recovery, tab or window commands, nothing on the selected lines, and
    /// nothing the main window isn't allowed either.
    #[test]
    fn the_setup_window_gets_only_what_its_tabs_use() {
        let v: serde_json::Value = serde_json::from_str(SETUP_CAPS).unwrap();
        assert_eq!(v["identifier"], "setup");
        assert_eq!(v["windows"], serde_json::json!([SETUP]));
        let main = permissions(MAIN_CAPS);
        let all = commands();
        let mut granted = Vec::new();
        for p in permissions(SETUP_CAPS) {
            if p == "core:event:allow-listen" || p == "core:event:allow-emit-to" {
                continue;
            }
            let cmd = p
                .strip_prefix("allow-")
                .unwrap_or_else(|| {
                    panic!("{p}: the setup window gets no core or plugin permissions")
                })
                .replace('-', "_");
            assert!(all.contains(&cmd), "{p} is not one of the app's commands");
            assert!(
                main.contains(&p),
                "{p} is not allowed for the main window either"
            );
            granted.push(cmd);
        }
        for never in [
            "new_tab",
            "switch_tab",
            "move_tab",
            "close_tab",
            "open_budget",
            "save_budget",
            "undo",
            "redo",
            "get_full_view",
            "set_cells",
            "clip_copy",
            "clip_paste",
            "open_recent",
            "restore_backup",
            "reveal_backups",
            "tools_window",
            "close_tools_window",
            "setup_window",
            "close_setup_window",
            "notes_list",
            "find",
            "replace",
            "history_list",
        ]
        .iter()
        .chain(APPLY_ONLY)
        {
            assert!(!granted.iter().any(|c| c == never), "{never} is granted");
        }

        // Every command the setup window's scripts name is granted (but the
        // apply half of apply.js, which setup mode never reaches).
        let scripts = [
            include_str!("../ui/js/setupwin.js"),
            include_str!("../ui/js/setuptabs.js"),
            include_str!("../ui/js/apply.js"),
            include_str!("../ui/js/setup.js"),
            include_str!("../ui/js/topsheet-setup.js"),
            include_str!("../ui/js/renumber.js"),
            include_str!("../ui/js/display-settings.js"),
            // The Currency tab's base and totals currency band.
            include_str!("../ui/js/setupbands.js"),
            include_str!("../ui/js/setuplist.js"),
            include_str!("../ui/js/rename.js"),
            // The library beside each table, and Save to Library in the
            // right-click menus (rename.js).
            include_str!("../ui/js/library.js"),
            // The library's Packs section (setup packs, src/packs.rs).
            include_str!("../ui/js/packs.js"),
            // Which host it runs in (host_info): a browser has no menu bar.
            include_str!("../ui/js/host.js"),
            // The Rate Tables tab (src/rates.rs).
            include_str!("../ui/js/rates.js"),
        ];
        let mut named = std::collections::BTreeSet::new();
        for js in scripts {
            for c in &all {
                if js.contains(&format!("'{c}'")) && !APPLY_ONLY.contains(&c.as_str()) {
                    named.insert(c.clone());
                }
            }
        }
        assert!(named.len() > 20, "found only {named:?}");
        for c in &named {
            assert!(
                granted.contains(c),
                "the setup window calls {c}, which it may not"
            );
        }
        for c in &granted {
            assert!(
                named.contains(c),
                "{c} is granted but the setup window never calls it"
            );
        }
    }

    /// apply.js keeps the apply half out of setup mode: the summary is never
    /// fetched there, so nothing on the selected lines can run.
    #[test]
    fn setup_mode_never_fetches_a_summary() {
        let apply = include_str!("../ui/js/apply.js");
        let at = apply.find("async function fetchSummary()").unwrap();
        let body = &apply[at..at + 200];
        assert!(
            body.contains("setupMode()"),
            "fetchSummary must stop in setup mode"
        );
    }
}
