//! The tools panel's own window, and keeping two windows in step.
//!
//! The tools panel (`ui/js/tools.js`) docks in the main window, at the right
//! or the bottom, or pops out into a window of its own labelled [`TOOLS`]. That
//! window loads the same bundled page as the main one with `?panel=1`
//! ([`TOOLS_PAGE`]) and draws only the panel. Both windows call the same
//! commands on the same budgets in `AppState`, so there is nothing to merge:
//! each command's reply goes to the window that sent it, and every other
//! window hears about the change and asks for a fresh view. The panel
//! follows the budget in front: its commands name the budget it drew
//! (`doc`), and a change of tab is news like any other.
//!
//! Events from Rust:
//! - [`VIEW_CHANGED`] (`{doc, rev}`: which budget, and its view's new `rev`)
//!   to every window except the one whose command changed the budget or
//!   brought another tab to the front (see [`changed`]). Pages listen for it
//!   under their own label, never "any", so a window never hears its own.
//! - [`TOOLS_CLOSED`] to the main window when the tools window goes (its
//!   close button, Close Window while it is in front), so the panel docks back.
//! - [`TOOLS_GEOMETRY`] to the main window when the tools window moves or is
//!   resized: `{x, y, width, height}` in logical pixels, which the main window
//!   remembers and passes back to [`tools_window`] next time.
//!
//! The two pages also talk directly (`emitTo`, each listening under its own
//! label): the main window sends its grid selection and the panel layout; the
//! tools window asks to dock, saves its layout, jumps to a noted line and
//! forwards app keys (Save, Undo...) as menu commands. See docs/APP.md.
//!
//! [`notes_list`] is the Notes tab's list of every note (src/notes.rs).

use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Emitter, EventTarget, Manager, Runtime, State, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, Window, WindowEvent,
};

use crate::app::{AppState, DocId};

/// The main window's label (tauri.conf.json).
pub const MAIN: &str = "main";
/// The tools window's label; `capabilities/tools.json` names it.
pub const TOOLS: &str = "tools";
/// The page the tools window loads: the app's own page, drawing only the panel.
pub const TOOLS_PAGE: &str = "index.html?panel=1";
/// The tools window's title.
pub const TOOLS_TITLE: &str = "Tools";

pub const VIEW_CHANGED: &str = "view-changed";
pub const TOOLS_CLOSED: &str = "tools-closed";
pub const TOOLS_GEOMETRY: &str = "tools-geometry";

/// Size of a new tools window, and the smallest it can be (logical pixels).
pub const DEFAULT_SIZE: (f64, f64) = (380.0, 720.0);
pub const MIN_SIZE: (f64, f64) = (300.0, 320.0);
const MAX_SIDE: f64 = 8000.0;

/// The newest view revision announced to the other windows.
#[derive(Default)]
pub struct Broadcast {
    last_rev: AtomicU64,
}

impl Broadcast {
    /// True the first time `rev` is seen: the budget changed since the last
    /// announcement. A view asked for with nothing changed (`get_view`) is not news.
    pub fn is_new(&self, rev: u64) -> bool {
        self.last_rev.swap(rev, Ordering::SeqCst) != rev
    }
}

/// Whether an event listener's target is some window other than `caller`.
pub fn is_other(target: &EventTarget, caller: &str) -> bool {
    match target {
        EventTarget::AnyLabel { label }
        | EventTarget::Window { label }
        | EventTarget::Webview { label }
        | EventTarget::WebviewWindow { label } => label != caller,
        _ => false,
    }
}

/// What [`VIEW_CHANGED`] says: which budget changed, and its new view rev.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ViewChanged {
    pub doc: DocId,
    pub rev: u64,
}

/// After a command: if what it gave is news (a new `rev`: the budget
/// changed, or another tab came to the front, since revs are never shared
/// between budgets), tells every window but the one that sent the command,
/// which already has the new view.
pub(crate) fn changed<R: Runtime>(window: &WebviewWindow<R>, doc: DocId, rev: u64) {
    let Some(news) = window.try_state::<Broadcast>() else {
        return;
    };
    if !news.is_new(rev) {
        return;
    }
    let caller = window.label().to_string();
    let _ = window
        .app_handle()
        .emit_filter(VIEW_CHANGED, ViewChanged { doc, rev }, |t| {
            is_other(t, &caller)
        });
}

/// Where a menu command goes: the tools window when it is in front (it runs
/// the panel's own commands and passes the rest on), else the main window.
pub fn menu_target(tools_in_front: bool) -> &'static str {
    if tools_in_front {
        TOOLS
    } else {
        MAIN
    }
}

/// How long after a window loses focus to look whether another of the app's
/// windows has it now (moving between them isn't leaving the app).
pub const FOCUS_SETTLE: std::time::Duration = std::time::Duration::from_millis(150);

/// True when one of the app's windows has focus.
pub fn app_in_front<R: Runtime>(app: &AppHandle<R>) -> bool {
    app.webview_windows()
        .values()
        .any(|w| w.is_focused().unwrap_or(false))
}

/// True when the tools window exists and has focus.
pub fn tools_in_front<R: Runtime>(app: &AppHandle<R>) -> bool {
    app.get_webview_window(TOOLS)
        .is_some_and(|w| w.is_focused().unwrap_or(false))
}

// ---------------------------------------------------------------------------
// Where the tools window opens

/// A window's place and size in logical pixels: `x` and `y` are its outer
/// top-left corner, `width` and `height` its inner size.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Geometry {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// A screen's usable area (its work area) in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Area {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Area {
    fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }
}

/// Where a remembered window goes: its size kept within sensible limits, and
/// its position only when the top of the window (where the title bar is)
/// would be on one of `areas`, so a window last seen on a screen that is gone
/// opens centered instead of out of reach. Nonsense (not a number, negative)
/// falls back to the defaults.
pub fn placement(g: Option<Geometry>, areas: &[Area]) -> ((f64, f64), Option<(f64, f64)>) {
    placement_sized(g, areas, DEFAULT_SIZE, MIN_SIZE)
}

/// [`placement`] for a window with its own default and smallest size (the
/// Budget Setup window, src/setupwin.rs).
pub fn placement_sized(
    g: Option<Geometry>,
    areas: &[Area],
    default_size: (f64, f64),
    min_size: (f64, f64),
) -> ((f64, f64), Option<(f64, f64)>) {
    let side = |v: f64, min: f64, default: f64| {
        if v.is_finite() && v > 0.0 {
            v.clamp(min, MAX_SIDE)
        } else {
            default
        }
    };
    let Some(g) = g else {
        return (default_size, None);
    };
    let size = (
        side(g.width, min_size.0, default_size.0),
        side(g.height, min_size.1, default_size.1),
    );
    let reachable = g.x.is_finite() && g.y.is_finite() && {
        // A point on the title bar, a little in from the left edge.
        let (px, py) = (g.x + (size.0 / 2.0).min(80.0), g.y + 12.0);
        areas.iter().any(|a| a.contains(px, py))
    };
    (size, reachable.then_some((g.x, g.y)))
}

/// Every screen's work area, in logical pixels.
pub(crate) fn screen_areas<R: Runtime>(app: &AppHandle<R>) -> Vec<Area> {
    let Some(main) = app.get_webview_window(MAIN) else {
        return Vec::new();
    };
    main.available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| {
            let s = m.scale_factor().max(0.1);
            let r = m.work_area();
            Area {
                x: f64::from(r.position.x) / s,
                y: f64::from(r.position.y) / s,
                width: f64::from(r.size.width) / s,
                height: f64::from(r.size.height) / s,
            }
        })
        .collect()
}

/// A window's place and size now, or `None` while it is minimized
/// (zero size) or can't be asked.
pub(crate) fn geometry_of<R: Runtime>(window: &Window<R>) -> Option<Geometry> {
    let scale = window.scale_factor().ok()?;
    let at = window.outer_position().ok()?.to_logical::<f64>(scale);
    let size = window.inner_size().ok()?;
    if size.width == 0 || size.height == 0 {
        return None;
    }
    let size = size.to_logical::<f64>(scale);
    Some(Geometry {
        x: at.x,
        y: at.y,
        width: size.width,
        height: size.height,
    })
}

// ---------------------------------------------------------------------------
// Commands

/// Shows the tools window (made the first time, where `geometry` says, else
/// centered at the default size) and brings it to the front; with `show`
/// false, hides it. Async because making a window from a synchronous command
/// can deadlock on Windows.
#[tauri::command]
pub async fn tools_window<R: Runtime>(
    app: AppHandle<R>,
    show: bool,
    geometry: Option<Geometry>,
) -> Result<(), String> {
    if let Some(w) = app.get_webview_window(TOOLS) {
        let shown = if show {
            w.show()
                .and_then(|()| w.unminimize())
                .and_then(|()| w.set_focus())
        } else {
            w.hide()
        };
        return shown.map_err(|e| e.to_string());
    }
    if !show {
        return Ok(());
    }
    let ((width, height), at) = placement(geometry, &screen_areas(&app));
    let mut builder = WebviewWindowBuilder::new(&app, TOOLS, WebviewUrl::App(TOOLS_PAGE.into()))
        .title(TOOLS_TITLE)
        .inner_size(width, height)
        .min_inner_size(MIN_SIZE.0, MIN_SIZE.1);
    builder = match at {
        Some((x, y)) => builder.position(x, y),
        None => builder.center(),
    };
    let window = builder.build().map_err(|e| e.to_string())?;
    // Windows and Linux give every window the app's menu bar; the panel's
    // keys are answered by the page (and app keys passed to the main window).
    #[cfg(not(target_os = "macos"))]
    let _ = window.remove_menu();
    #[cfg(target_os = "macos")]
    let _ = window;
    Ok(())
}

/// Closes the tools window, if there is one (the panel is docking again).
#[tauri::command]
pub fn close_tools_window<R: Runtime>(app: AppHandle<R>) {
    if let Some(w) = app.get_webview_window(TOOLS) {
        let _ = w.close();
    }
}

/// Window events that concern the tools window: its place and size go to the
/// main window to remember, its closing docks the panel back, and closing the
/// main window takes the tools window with it (else a hidden tools window
/// would keep the app running with nothing on screen).
pub fn on_window_event<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    match (window.label(), event) {
        (TOOLS, WindowEvent::Moved(_) | WindowEvent::Resized(_)) => {
            if let Some(g) = geometry_of(window) {
                let _ = window.emit_to(MAIN, TOOLS_GEOMETRY, g);
            }
        }
        (TOOLS, WindowEvent::Destroyed) => {
            let _ = window.emit_to(MAIN, TOOLS_CLOSED, ());
        }
        (MAIN, WindowEvent::Destroyed) => {
            if let Some(tools) = window.app_handle().get_webview_window(TOOLS) {
                let _ = tools.destroy();
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// The Notes tab: every note in the budget

/// The Notes tab's list: every note in budget `doc` (`None`: the one in
/// front), on lines, accounts and categories (src/notes.rs).
#[tauri::command]
pub fn notes_list(
    state: State<'_, AppState>,
    doc: Option<DocId>,
) -> Result<Vec<crate::notes::NoteEntry>, String> {
    Ok(crate::notes::budget_notes(state.lock_doc(doc)?.budget()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_view_is_news_once() {
        let b = Broadcast::default();
        assert!(b.is_new(5), "a change is announced");
        assert!(!b.is_new(5), "the same view asked for again is not");
        assert!(b.is_new(6));
        assert!(
            b.is_new(5),
            "undo gets a fresh rev in practice, but any other rev is news"
        );
    }

    #[test]
    fn view_changes_skip_the_window_that_made_them() {
        let main = EventTarget::AnyLabel { label: MAIN.into() };
        let tools = EventTarget::AnyLabel {
            label: TOOLS.into(),
        };
        assert!(!is_other(&main, MAIN));
        assert!(is_other(&tools, MAIN));
        assert!(is_other(&main, TOOLS));
        assert!(!is_other(&tools, TOOLS));
        assert!(is_other(
            &EventTarget::WebviewWindow {
                label: TOOLS.into()
            },
            MAIN
        ));
        // Listeners on the app itself, or "any", are not a window to tell.
        assert!(!is_other(&EventTarget::App, MAIN));
        assert!(!is_other(&EventTarget::Any, MAIN));
    }

    #[test]
    fn menu_commands_go_to_the_window_in_front() {
        assert_eq!(menu_target(true), TOOLS);
        assert_eq!(menu_target(false), MAIN);
    }

    #[test]
    fn the_tools_page_is_the_bundled_page() {
        let url = tauri::Url::parse("tauri://localhost/")
            .unwrap()
            .join(TOOLS_PAGE)
            .unwrap();
        assert_eq!(url.as_str(), "tauri://localhost/index.html?panel=1");
        assert_eq!(url.path(), "/index.html");
    }

    const SCREEN: Area = Area {
        x: 0.0,
        y: 25.0,
        width: 1440.0,
        height: 875.0,
    };
    const LEFT_SCREEN: Area = Area {
        x: -1920.0,
        y: 0.0,
        width: 1920.0,
        height: 1080.0,
    };

    fn g(x: f64, y: f64, width: f64, height: f64) -> Option<Geometry> {
        Some(Geometry {
            x,
            y,
            width,
            height,
        })
    }

    #[test]
    fn a_remembered_window_opens_where_it_was() {
        let (size, at) = placement(g(1000.0, 100.0, 400.0, 700.0), &[SCREEN]);
        assert_eq!(size, (400.0, 700.0));
        assert_eq!(at, Some((1000.0, 100.0)));
        // On a second screen to the left.
        let (_, at) = placement(g(-1500.0, 40.0, 400.0, 700.0), &[SCREEN, LEFT_SCREEN]);
        assert_eq!(at, Some((-1500.0, 40.0)));
    }

    #[test]
    fn a_window_off_every_screen_is_centered() {
        // The screen it was on is gone.
        let (size, at) = placement(g(-1500.0, 40.0, 400.0, 700.0), &[SCREEN]);
        assert_eq!(size, (400.0, 700.0));
        assert_eq!(at, None);
        // Title bar above the top of the screen, or past the right edge.
        assert_eq!(placement(g(100.0, -300.0, 400.0, 700.0), &[SCREEN]).1, None);
        assert_eq!(placement(g(1430.0, 100.0, 400.0, 700.0), &[SCREEN]).1, None);
        // No screens known: never guess.
        assert_eq!(placement(g(100.0, 100.0, 400.0, 700.0), &[]).1, None);
    }

    #[test]
    fn sizes_are_kept_sensible() {
        assert_eq!(placement(None, &[SCREEN]), (DEFAULT_SIZE, None));
        let (size, _) = placement(g(10.0, 40.0, 50.0, 90.0), &[SCREEN]);
        assert_eq!(size, MIN_SIZE);
        let (size, _) = placement(g(10.0, 40.0, 1e9, 1e9), &[SCREEN]);
        assert_eq!(size, (MAX_SIDE, MAX_SIDE));
        let (size, at) = placement(g(f64::NAN, 40.0, f64::INFINITY, -5.0), &[SCREEN]);
        assert_eq!(size, DEFAULT_SIZE);
        assert_eq!(at, None);
    }

    #[test]
    fn geometry_reads_what_the_page_sends() {
        let g: Geometry =
            serde_json::from_str(r#"{"x": 12.5, "y": 40, "width": 380, "height": 720}"#).unwrap();
        assert_eq!(
            g,
            Geometry {
                x: 12.5,
                y: 40.0,
                width: 380.0,
                height: 720.0
            }
        );
    }

    const APP_RS: &str = include_str!("app.rs");
    const BUILD_RS: &str = include_str!("../build.rs");
    const MAIN_CAPS: &str = include_str!("../capabilities/main.json");
    const TOOLS_CAPS: &str = include_str!("../capabilities/tools.json");
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
    fn tools_commands_are_wired() {
        for c in ["tools_window", "close_tools_window", "notes_list"] {
            assert!(
                APP_RS.contains(&format!("crate::tools::{c},")),
                "{c} not in generate_handler!"
            );
            assert!(commands().contains(&c.to_string()), "{c} not in build.rs");
            let allow = format!("allow-{}", c.replace('_', "-"));
            assert!(
                permissions(MAIN_CAPS).contains(&allow),
                "{c} not allowed for the main window"
            );
        }
        // Every window's capability is in force (the Budget Setup window's
        // own is src/setupwin.rs).
        let conf: serde_json::Value = serde_json::from_str(CONF).unwrap();
        assert_eq!(
            conf["app"]["security"]["capabilities"],
            serde_json::json!(["main", "tools", "setup"])
        );
    }

    /// The tools window may call only what the panel calls, and only from the
    /// tools window: no files, saving, undo, printing, clipboard, recovery or
    /// window commands, and nothing the main window isn't allowed either.
    #[test]
    fn the_tools_window_gets_only_what_the_panel_uses() {
        let v: serde_json::Value = serde_json::from_str(TOOLS_CAPS).unwrap();
        assert_eq!(v["identifier"], "tools");
        assert_eq!(v["windows"], serde_json::json!([TOOLS]));
        let main = permissions(MAIN_CAPS);
        let all = commands();
        let granted = permissions(TOOLS_CAPS);
        let mut granted_cmds = Vec::new();
        for p in &granted {
            if p == "core:event:allow-listen" || p == "core:event:allow-emit-to" {
                continue;
            }
            let cmd = p
                .strip_prefix("allow-")
                .unwrap_or_else(|| {
                    panic!("{p}: the tools window gets no core or plugin permissions")
                })
                .replace('-', "_");
            assert!(all.contains(&cmd), "{p} is not one of the app's commands");
            assert!(
                main.contains(p),
                "{p} is not allowed for the main window either"
            );
            granted_cmds.push(cmd);
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
        ] {
            assert!(
                !granted_cmds.iter().any(|c| c == never),
                "{never} is granted"
            );
        }

        // Every command the panel's scripts name is granted.
        let panel_js = [
            include_str!("../ui/js/tools.js"),
            include_str!("../ui/js/toolswin.js"),
            include_str!("../ui/js/apply.js"),
            // Compact Globals and Units (the full tables, charges, credits,
            // info, settings and the baseline are the Budget Setup window's,
            // src/setupwin.rs: topsheet-setup.js, display-settings.js,
            // setupbands.js and renumber.js are not the panel's).
            include_str!("../ui/js/setup.js"),
            include_str!("../ui/js/notes.js"),
            include_str!("../ui/js/find.js"),
            include_str!("../ui/js/rename.js"),
            include_str!("../ui/js/presets.js"),
            include_str!("../ui/js/setuplist.js"),
            include_str!("../ui/js/history.js"),
            // The Explain and Overtime tabs.
            include_str!("../ui/js/explain.js"),
            include_str!("../ui/js/overtime.js"),
            // The Actuals tab (importing is the main window's File menu,
            // actualsimport.js).
            include_str!("../ui/js/actuals.js"),
            // The Library tab (its reviews run its own commands, never
            // importsetup.js's import_setup).
            include_str!("../ui/js/library.js"),
            // Its Packs section (setup packs, src/packs.rs).
            include_str!("../ui/js/packs.js"),
            // The Review and Activity tabs of a shared budget (src/shared_app.rs).
            include_str!("../ui/js/review.js"),
            include_str!("../ui/js/activity.js"),
        ];
        let mut named = std::collections::BTreeSet::new();
        for js in panel_js {
            for c in &all {
                if js.contains(&format!("'{c}'")) {
                    named.insert(c.clone());
                }
            }
        }
        assert!(named.len() > 20, "found only {named:?}");
        for c in &named {
            assert!(
                granted_cmds.contains(c),
                "the panel calls {c}, which the tools window may not"
            );
        }
        // And nothing is granted that the panel never calls.
        for c in &granted_cmds {
            assert!(
                named.contains(c),
                "{c} is granted but the panel never calls it"
            );
        }
    }
}
