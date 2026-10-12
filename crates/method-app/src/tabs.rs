//! Budgets as tabs in the one window (decisions.md, entries 19 and 25): the
//! commands the tab strip calls (`ui/js/tabs.js`), the Window menu's list of
//! open budgets, and remembering which files are open for the next start.
//! The documents themselves are kept in src/docs.rs; opening and closing
//! them, with their recovery copies, is in src/safety_app.rs.
//!
//! Every command still works on the budget in front unless it names another
//! with `doc` (src/app.rs).
//!
//! Split view (roadmap 2.26): two panes side by side, each with its own tabs
//! (src/docs.rs, `Split`). The pane commands here split and merge the
//! window, pick the target pane, move tabs within a pane and to the other
//! one, and give the other pane its own view of a budget (`pane_view`: its
//! own account on screen, nothing changed and nobody told). Opening a file (Open, Open Recent, a double-click
//! in the Finder, a recovered budget) puts it in a new tab, or brings its tab
//! forward when it is open already.

use std::sync::{Mutex, MutexGuard};

use serde::Serialize;
use tauri::menu::{CheckMenuItem, MenuItemKind};
use tauri::{AppHandle, Manager, Runtime, State, WebviewWindow};

use crate::app::{finish, AppState, DocId};
use crate::docs::{Docs, TabInfo};
use crate::safety_app::{self, Closed, Safety};
use crate::view::Focus;
use crate::view::View;

type CmdResult<T = View> = Result<T, String>;

/// Window > one open budget: `window.doc.<id>`, answered in the web view,
/// which brings that tab to the front.
pub const DOC_PREFIX: &str = "window.doc.";

/// One entry of the Window menu's list: id, label, in front.
type MenuEntry = (DocId, String, bool);

/// The Window menu's list of budgets as last drawn, so it is drawn again
/// only when a budget opens, closes, moves, is renamed or comes to the front.
#[derive(Default)]
pub struct TabMenu {
    drawn: Mutex<Option<Vec<MenuEntry>>>,
}

impl TabMenu {
    fn drawn(&self) -> MutexGuard<'_, Option<Vec<MenuEntry>>> {
        self.drawn.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Draws the Window menu's list again now (a click on one of its check items
/// ticked or unticked it by itself). Call it on the main thread, without
/// holding the budgets' lock.
pub fn redraw_window_menu<R: Runtime>(app: &AppHandle<R>) {
    let (Some(menu), Some(state)) = (app.try_state::<TabMenu>(), app.try_state::<AppState>())
    else {
        return;
    };
    let entries = menu_entries(&state.docs());
    *menu.drawn() = Some(entries.clone());
    draw_window_menu(app, &entries);
}

/// The Window menu's entries for `docs`, in tab order. `&` is doubled (menus
/// on Windows and Linux read a single one as a mnemonic).
fn menu_entries(docs: &Docs) -> Vec<MenuEntry> {
    docs.tabs()
        .into_iter()
        .map(|t| (t.id, t.label.replace('&', "&&"), t.active))
        .collect()
}

/// After every command (and at the start): the files open as tabs are
/// remembered for the next start, and the Window menu lists the open
/// budgets. Callers may hold the budgets' lock: the menu is changed on the
/// main thread without waiting for it.
pub fn after_command<R: Runtime>(app: &AppHandle<R>, docs: &Docs) {
    if let Some(safety) = app.try_state::<Safety>() {
        safety.remember_tabs(docs);
    }
    let Some(menu) = app.try_state::<TabMenu>() else {
        return;
    };
    let entries = menu_entries(docs);
    {
        let mut drawn = menu.drawn();
        if drawn.as_ref() == Some(&entries) {
            return;
        }
        *drawn = Some(entries.clone());
    }
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || draw_window_menu(&handle, &entries));
}

/// Puts the open budgets at the end of the Window menu's own items, the one
/// in front ticked.
fn draw_window_menu<R: Runtime>(app: &AppHandle<R>, entries: &[MenuEntry]) {
    let Some(menu) = app.menu() else {
        return;
    };
    let Some(MenuItemKind::Submenu(window)) = menu.get(crate::menu::WINDOW_MENU) else {
        return;
    };
    let _ = (|| -> tauri::Result<()> {
        let items = window.items()?;
        for (i, item) in items.iter().enumerate().rev() {
            if item.id().as_ref().starts_with(DOC_PREFIX) {
                window.remove_at(i)?;
            }
        }
        // Right after the menu's own items (macOS adds its list of windows
        // at the end).
        let at = window
            .items()?
            .iter()
            .position(|i| i.id() == crate::menu::LAST_WINDOW_ITEM)
            .map_or(window.items()?.len(), |p| p + 2);
        for (k, (id, label, active)) in entries.iter().enumerate() {
            let item = CheckMenuItem::with_id(
                app,
                format!("{DOC_PREFIX}{id}"),
                label,
                true,
                *active,
                None::<&str>,
            )?;
            window.insert(&item, at + k)?;
        }
        Ok(())
    })();
}

// ---------------------------------------------------------------------------
// Commands (registered in app.rs, build.rs and capabilities/main.json; the
// tools window may call none of them)

/// A new tab with a fresh starter budget, in front.
#[tauri::command]
pub fn new_tab<R: Runtime>(window: WebviewWindow<R>, state: State<'_, AppState>) -> CmdResult {
    let id = safety_app::new_tab(&state);
    Ok(finish(&window, &mut state.lock_doc(Some(id))?, None))
}

/// Brings budget `doc`'s tab to the front; its view. Split, `pane` says
/// which pane's tab was picked (that pane becomes the target); without it
/// the tab comes to the front where it is (the target pane first).
#[tauri::command]
pub fn switch_tab<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: DocId,
    pane: Option<usize>,
) -> CmdResult {
    if let Some(p) = pane {
        state.docs().switch_in(p, doc)?;
    }
    let mut s = state.lock_doc(Some(doc))?;
    if pane.is_none() {
        s.bring_to_front();
    }
    Ok(finish(&window, &mut s, None))
}

/// Moves budget `doc`'s tab to place `to` in tab order (the end when past
/// it); the tabs as they are now.
#[tauri::command]
pub fn move_tab<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    doc: DocId,
    to: usize,
) -> CmdResult<Vec<TabInfo>> {
    let mut docs = state.docs();
    docs.move_tab(doc, to)?;
    after_command(&app, &docs);
    Ok(docs.tabs())
}

/// The open budgets' tabs, in order.
#[tauri::command]
pub fn list_tabs(state: State<'_, AppState>) -> Vec<TabInfo> {
    state.docs().tabs()
}

/// What `close_tab` did.
#[derive(Debug, Serialize)]
pub struct Closing {
    /// The tab closed; `view` is the budget now in front.
    pub closed: bool,
    /// Not closed: it has unsaved changes Autosave couldn't save. Ask, then
    /// close it with `discard` (or save it first).
    pub unsaved: bool,
    /// Not closed: it is the only tab. Close the window instead (which asks
    /// about unsaved changes on its way out).
    pub last: bool,
    pub view: Option<View>,
}

/// Closes budget `doc`'s tab (`None`: the one in front). Its unsaved changes
/// are autosaved into its file first when Autosave is on; what is still
/// unsaved stops it unless `discard` (the user chose to throw it away, and
/// its recovery copy goes too). Split, `pane` names the pane whose tab it
/// is: when the other pane shows the budget too, only this tab goes. With
/// `to_start`, the last tab closes too and an untouched Untitled budget takes
/// its place, for the start window to show over (src/start.rs); without it,
/// the last tab answers `last`.
#[tauri::command]
pub fn close_tab<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    safety: State<'_, Safety>,
    doc: Option<DocId>,
    discard: bool,
    pane: Option<usize>,
    to_start: Option<bool>,
) -> CmdResult<Closing> {
    let id = state.lock_doc(doc)?.id();
    let mut out = Closing {
        closed: false,
        unsaved: false,
        last: false,
        view: None,
    };
    if pane.is_some_and(|p| state.docs().close_in_pane(p, id)) {
        out.closed = true;
        out.view = Some(finish(&window, &mut state.lock(), None));
        return Ok(out);
    }
    if !discard {
        safety_app::autosave_one(window.app_handle(), id);
    }
    let closed = if to_start == Some(true) {
        safety_app::close_last_for_start(&state, &safety, id, discard)?
    } else {
        safety_app::close_doc(&state, &safety, id, discard)?
    };
    match closed {
        Closed::Last => out.last = true,
        Closed::Unsaved => out.unsaved = true,
        Closed::Closed => {
            out.closed = true;
            out.view = Some(finish(&window, &mut state.lock(), None));
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Split view (registered like the tab commands: the main window only)

/// The view of the budget in front, after a change to the panes.
fn front_view<R: Runtime>(window: &WebviewWindow<R>, state: &AppState) -> View {
    finish(window, &mut state.lock(), None)
}

/// Splits the window: a second pane at the right showing the budget in front
/// too, now the target.
#[tauri::command]
pub fn split_view<R: Runtime>(window: WebviewWindow<R>, state: State<'_, AppState>) -> CmdResult {
    state.docs().split_right()?;
    Ok(front_view(&window, &state))
}

/// Closes the split: every tab back in one strip, the target's budget in front.
#[tauri::command]
pub fn close_split<R: Runtime>(window: WebviewWindow<R>, state: State<'_, AppState>) -> CmdResult {
    state.docs().close_split()?;
    Ok(front_view(&window, &state))
}

/// The account named as `focus_account` names it, or `None`.
fn focus_of(
    category: Option<String>,
    ti: Option<usize>,
    account: Option<String>,
    ai: Option<usize>,
) -> Option<Focus> {
    category.zip(account).map(|(category, account)| Focus {
        category,
        ti: ti.unwrap_or(usize::MAX),
        account,
        ai: ai.unwrap_or(usize::MAX),
    })
}

/// Makes `pane` the target (a click or focus in it): its budget in front
/// becomes the one commands act on, showing the account that pane is on
/// (named as `focus_account` names it; none for the topsheet or a list of
/// accounts). Its view.
#[tauri::command]
pub fn set_target_pane<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    pane: usize,
    category: Option<String>,
    ti: Option<usize>,
    account: Option<String>,
    ai: Option<usize>,
) -> CmdResult {
    state.docs().set_target(pane)?;
    let mut s = state.lock();
    s.set_focus(focus_of(category, ti, account, ai));
    Ok(finish(&window, &mut s, None))
}

/// Moves budget `doc`'s tab to place `to` among `pane`'s tabs.
#[tauri::command]
pub fn move_tab_in_pane<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    pane: usize,
    doc: DocId,
    to: usize,
) -> CmdResult {
    state.docs().move_in_pane(pane, doc, to)?;
    Ok(front_view(&window, &state))
}

/// Moves budget `doc`'s tab from pane `from` to the other pane (at place
/// `to` there, or the end), in front there; that pane becomes the target.
#[tauri::command]
pub fn move_to_pane<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: DocId,
    from: usize,
    to: Option<usize>,
) -> CmdResult {
    state.docs().move_to_pane(doc, from, to)?;
    Ok(front_view(&window, &state))
}

/// Budget `doc` as the pane that isn't the target draws it: with its own
/// account on screen (named as `focus_account` names it), without changing
/// the budget's account, the budget in front or anything else, and without
/// telling the other windows (nothing changed).
#[tauri::command]
pub fn pane_view(
    state: State<'_, AppState>,
    doc: DocId,
    category: Option<String>,
    ti: Option<usize>,
    account: Option<String>,
    ai: Option<usize>,
) -> CmdResult {
    let mut s = state.lock_doc(Some(doc))?;
    let focus = focus_of(category, ti, account, ai);
    let mut view = s.view_at(focus.as_ref());
    view.doc = s.id();
    view.tabs = s.docs().tabs();
    view.panes = Some(s.docs().layout());
    Ok(view)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Session;

    const APP_RS: &str = include_str!("app.rs");
    const BUILD_RS: &str = include_str!("../build.rs");
    const MAIN_CAPS: &str = include_str!("../capabilities/main.json");
    const TOOLS_CAPS: &str = include_str!("../capabilities/tools.json");

    #[test]
    fn tab_commands_are_wired_for_the_main_window_only() {
        for c in [
            "new_tab",
            "switch_tab",
            "move_tab",
            "close_tab",
            "list_tabs",
            "split_view",
            "close_split",
            "set_target_pane",
            "move_tab_in_pane",
            "move_to_pane",
            "pane_view",
        ] {
            assert!(
                APP_RS.contains(&format!("crate::tabs::{c},")),
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
        }
        assert!(!BUILD_RS.contains("\"new_budget\""), "New opens a tab now");
    }

    #[test]
    fn the_window_menu_lists_every_budget_in_tab_order() {
        let mut docs = Docs::default();
        let first = docs.active();
        let mut s = Session::new();
        s.edit("Test edit", |b| crate::edit::set_title(b, "Rock & Roll"))
            .unwrap();
        let second = docs.add(s);
        assert_eq!(
            menu_entries(&docs),
            [
                (first, "Untitled feature".to_string(), false),
                (second, "Rock && Roll".to_string(), true),
            ]
        );
        docs.move_tab(second, 0).unwrap();
        assert_eq!(menu_entries(&docs)[0].0, second);
    }
}
