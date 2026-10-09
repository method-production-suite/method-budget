//! Tauri glue: app state, the commands the UI invokes, native file dialogs, and
//! the unsaved-changes guard on close and quit.
//!
//! Security notes (see docs/APP.md): file paths only ever come from native
//! dialogs shown here in Rust, never from the web view; every command works on
//! the one budget in [`AppState`]; navigation away from the bundled UI is blocked.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use fwb_core::RowId;
use tauri::{AppHandle, Manager, Runtime, State, WebviewWindow};
use tauri_plugin_dialog::{
    DialogExt, MessageDialogButtons, MessageDialogKind, MessageDialogResult,
};

use crate::edit::{self, CurrencyInput, FringeInput, GroupInput, UnitInput};
use crate::report_dto::SelectionDto;
use crate::session::Session;
use crate::view::View;

const APP_NAME: &str = "Film Wizard Budgeting";
const FILTER_NAME: &str = "Film Wizard budget";

#[derive(Default)]
pub struct AppState {
    session: Mutex<Session>,
}

impl AppState {
    fn lock(&self) -> MutexGuard<'_, Session> {
        // A panic mid-command can't leave the budget half-edited (edits apply
        // to a copy), so a poisoned lock is still safe to use.
        self.session.lock().unwrap_or_else(|e| e.into_inner())
    }
}

type CmdResult<T = View> = Result<T, String>;

fn window_title(s: &Session) -> String {
    let edited = if s.is_dirty() { " (edited)" } else { "" };
    format!("{}{edited} - {APP_NAME}", s.display_name())
}

/// The view after a command, with the native title kept in sync.
fn finish<R: Runtime>(window: &WebviewWindow<R>, s: &mut Session, created: Option<RowId>) -> View {
    let _ = window.set_title(&window_title(s));
    s.view_with(created)
}

fn apply<R: Runtime, T>(
    window: &WebviewWindow<R>,
    state: &AppState,
    f: impl FnOnce(&mut Session) -> edit::EditResult<T>,
) -> CmdResult {
    let mut s = state.lock();
    f(&mut s)?;
    Ok(finish(window, &mut s, None))
}

/// Runs a pure budget edit as one undo step.
fn edit_budget<R: Runtime, T>(
    window: &WebviewWindow<R>,
    state: &AppState,
    f: impl FnOnce(&mut fwb_core::Budget) -> edit::EditResult<T>,
) -> CmdResult {
    apply(window, state, |s| s.edit(f))
}

// ---------------------------------------------------------------------------
// Files

#[tauri::command]
pub fn get_view<R: Runtime>(window: WebviewWindow<R>, state: State<'_, AppState>) -> View {
    finish(&window, &mut state.lock(), None)
}

/// Replaces the budget with a fresh starter. The UI confirms first when there
/// are unsaved changes.
#[tauri::command]
pub fn new_budget<R: Runtime>(window: WebviewWindow<R>, state: State<'_, AppState>) -> View {
    let mut s = state.lock();
    *s = Session::new();
    finish(&window, &mut s, None)
}

/// Shows an Open dialog and loads the chosen file. `None` if cancelled.
#[tauri::command]
pub async fn open_budget<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
) -> CmdResult<Option<View>> {
    let start = state
        .lock()
        .path()
        .and_then(|p| p.parent())
        .map(PathBuf::from);
    let mut dialog = window
        .dialog()
        .file()
        .set_title("Open budget")
        .add_filter(FILTER_NAME, &["fwb"])
        .set_parent(&window);
    if let Some(dir) = start {
        dialog = dialog.set_directory(dir);
    }
    let Some(picked) = dialog.blocking_pick_file() else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(|e| e.to_string())?;
    let opened = Session::open(&path)?;
    let mut s = state.lock();
    *s = opened;
    Ok(Some(finish(&window, &mut s, None)))
}

/// Saves to the current file, or asks where (always, with `save_as`).
/// `None` if the user cancelled the dialog.
#[tauri::command]
pub async fn save_budget<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    save_as: bool,
) -> CmdResult<Option<View>> {
    let (current, suggested) = {
        let s = state.lock();
        let title = s.budget().title.trim().to_string();
        let suggested = match s.path().and_then(|p| p.file_stem()) {
            Some(stem) => stem.to_string_lossy().into_owned(),
            None => title,
        };
        (s.path().map(PathBuf::from), suggested)
    };
    let path = match current {
        Some(p) if !save_as => p,
        _ => {
            let mut dialog = window
                .dialog()
                .file()
                .set_title(if save_as {
                    "Save budget as"
                } else {
                    "Save budget"
                })
                .add_filter(FILTER_NAME, &["fwb"])
                .set_file_name(format!("{}.fwb", safe_file_stem(&suggested)))
                .set_parent(&window);
            if let Some(dir) = current.as_deref().and_then(|p| p.parent()) {
                dialog = dialog.set_directory(dir);
            }
            let Some(picked) = dialog.blocking_save_file() else {
                return Ok(None);
            };
            with_fwb_extension(picked.into_path().map_err(|e| e.to_string())?)
        }
    };
    let mut s = state.lock();
    s.save_to(&path)?;
    Ok(Some(finish(&window, &mut s, None)))
}

/// A file name stem from a budget title: path separators and other characters
/// file systems reject become spaces.
fn safe_file_stem(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => ' ',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').trim();
    if trimmed.is_empty() {
        "Untitled".into()
    } else {
        trimmed.to_string()
    }
}

fn with_fwb_extension(mut p: PathBuf) -> PathBuf {
    let has = p.extension().is_some_and(|e| e.eq_ignore_ascii_case("fwb"));
    if !has {
        let mut name = p.file_name().unwrap_or_default().to_os_string();
        name.push(".fwb");
        p.set_file_name(name);
    }
    p
}

// ---------------------------------------------------------------------------
// History

#[tauri::command]
pub fn undo<R: Runtime>(window: WebviewWindow<R>, state: State<'_, AppState>) -> View {
    let mut s = state.lock();
    s.undo();
    finish(&window, &mut s, None)
}

#[tauri::command]
pub fn redo<R: Runtime>(window: WebviewWindow<R>, state: State<'_, AppState>) -> View {
    let mut s = state.lock();
    s.redo();
    finish(&window, &mut s, None)
}

// ---------------------------------------------------------------------------
// Structure

#[tauri::command]
pub fn set_title<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    title: String,
) -> CmdResult {
    edit_budget(&window, &state, |b| edit::set_title(b, &title))
}

#[tauri::command]
pub fn add_category<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    code: String,
    name: String,
    at: Option<usize>,
) -> CmdResult {
    edit_budget(&window, &state, |b| edit::add_category(b, &code, &name, at))
}

#[tauri::command]
pub fn add_production_total<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    name: String,
    at: Option<usize>,
) -> CmdResult {
    edit_budget(&window, &state, |b| {
        edit::add_production_total(b, &name, at)
    })
}

#[tauri::command]
pub fn update_topsheet_row<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    index: usize,
    code: Option<String>,
    name: String,
) -> CmdResult {
    edit_budget(&window, &state, |b| {
        edit::update_topsheet_row(b, index, code.as_deref(), &name)
    })
}

#[tauri::command]
pub fn delete_topsheet_row<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    index: usize,
) -> CmdResult {
    edit_budget(&window, &state, |b| edit::delete_topsheet_row(b, index))
}

#[tauri::command]
pub fn add_account<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    category_code: String,
    code: String,
    name: String,
    after: Option<String>,
) -> CmdResult {
    edit_budget(&window, &state, |b| {
        edit::add_account(b, &category_code, &code, &name, after.as_deref())
    })
}

#[tauri::command]
pub fn update_account<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    code: String,
    new_code: String,
    name: String,
) -> CmdResult {
    edit_budget(&window, &state, |b| {
        edit::update_account(b, &code, &new_code, &name)
    })
}

#[tauri::command]
pub fn delete_account<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    code: String,
) -> CmdResult {
    edit_budget(&window, &state, |b| edit::delete_account(b, &code))
}

// ---------------------------------------------------------------------------
// Detail rows

#[tauri::command]
pub fn set_detail_field<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    row_id: RowId,
    field: String,
    value: String,
) -> CmdResult {
    apply(&window, &state, |s| {
        s.set_detail_field(row_id, &field, &value)
    })
}

#[tauri::command]
pub fn set_rows_field<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    row_ids: Vec<RowId>,
    field: String,
    value: String,
) -> CmdResult {
    let field = edit::Field::parse(&field)?;
    edit_budget(&window, &state, |b| {
        edit::set_rows_field(b, &row_ids, field, &value)
    })
}

#[tauri::command]
pub fn add_detail<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    account_code: String,
    after: Option<RowId>,
    kind: String,
) -> CmdResult {
    let mut s = state.lock();
    let id = s.add_detail(&account_code, after, &kind)?;
    Ok(finish(&window, &mut s, Some(id)))
}

#[tauri::command]
pub fn delete_details<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    row_ids: Vec<RowId>,
) -> CmdResult {
    edit_budget(&window, &state, |b| edit::delete_details(b, &row_ids))
}

#[tauri::command]
pub fn selection_totals(state: State<'_, AppState>, row_ids: Vec<RowId>) -> SelectionDto {
    state.lock().selection(&row_ids)
}

// ---------------------------------------------------------------------------
// Setup tables

#[tauri::command]
pub fn set_global<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    name: String,
    formula: String,
) -> CmdResult {
    edit_budget(&window, &state, |b| edit::set_global(b, &name, &formula))
}

#[tauri::command]
pub fn delete_global<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    name: String,
) -> CmdResult {
    edit_budget(&window, &state, |b| edit::delete_global(b, &name))
}

#[tauri::command]
pub fn set_fringe<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    name: String,
    fringe: FringeInput,
) -> CmdResult {
    edit_budget(&window, &state, |b| edit::set_fringe(b, &name, &fringe))
}

#[tauri::command]
pub fn delete_fringe<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    name: String,
) -> CmdResult {
    edit_budget(&window, &state, |b| edit::delete_fringe(b, &name))
}

#[tauri::command]
pub fn set_group<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    name: String,
    group: GroupInput,
) -> CmdResult {
    edit_budget(&window, &state, |b| edit::set_group(b, &name, &group))
}

#[tauri::command]
pub fn delete_group<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    name: String,
) -> CmdResult {
    edit_budget(&window, &state, |b| edit::delete_group(b, &name))
}

#[tauri::command]
pub fn set_unit<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    name: String,
    unit: UnitInput,
) -> CmdResult {
    edit_budget(&window, &state, |b| edit::set_unit(b, &name, &unit))
}

#[tauri::command]
pub fn delete_unit<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    name: String,
) -> CmdResult {
    edit_budget(&window, &state, |b| edit::delete_unit(b, &name))
}

#[tauri::command]
pub fn set_currency<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    code: String,
    currency: CurrencyInput,
) -> CmdResult {
    edit_budget(&window, &state, |b| edit::set_currency(b, &code, &currency))
}

#[tauri::command]
pub fn delete_currency<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    code: String,
) -> CmdResult {
    edit_budget(&window, &state, |b| edit::delete_currency(b, &code))
}

// ---------------------------------------------------------------------------
// Unsaved-changes guard

const KEEP_EDITING: &str = "Keep editing";
const DISCARD: &str = "Close without saving";

/// Asks before throwing away unsaved changes; runs `then` if the user agrees
/// (or there was nothing to lose). Never blocks the event loop.
fn confirm_discard<R: Runtime>(app: &AppHandle<R>, then: impl FnOnce() + Send + 'static) {
    let dirty = app.state::<AppState>().lock().is_dirty();
    if !dirty {
        then();
        return;
    }
    let mut dialog = app
        .dialog()
        .message("This budget has unsaved changes. If you close it now, they will be lost.")
        .title("Unsaved changes")
        .kind(MessageDialogKind::Warning)
        // The safe choice comes first so it's the default (Enter). Only an
        // explicit click on the discard button throws work away; Escape,
        // closing the alert or anything unexpected keeps editing.
        .buttons(MessageDialogButtons::OkCancelCustom(
            KEEP_EDITING.into(),
            DISCARD.into(),
        ));
    if let Some(w) = app.get_webview_window("main") {
        dialog = dialog.parent(&w);
    }
    dialog.show_with_result(move |choice| {
        if choice == MessageDialogResult::Custom(DISCARD.into()) {
            then();
        }
    });
}

/// Only the bundled UI may load in the web view: no navigating to other
/// sites or files (a dropped link, a stray anchor, injected content).
fn is_app_url(url: &tauri::Url) -> bool {
    match url.scheme() {
        "tauri" => url.host_str() == Some("localhost"),
        "http" | "https" => url.host_str() == Some("tauri.localhost"),
        _ => false,
    }
}

#[cfg(target_os = "macos")]
fn mac_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<tauri::menu::Menu<R>> {
    use tauri::menu::{AboutMetadata, Menu, MenuItem, PredefinedMenuItem, Submenu};
    let about = AboutMetadata {
        name: Some(APP_NAME.into()),
        version: Some(app.package_info().version.to_string()),
        ..Default::default()
    };
    // The stock Quit item would end the app without asking about unsaved
    // changes, so ours goes through the same guard as closing the window.
    let quit = MenuItem::with_id(
        app,
        "quit",
        format!("Quit {APP_NAME}"),
        true,
        Some("CmdOrCtrl+Q"),
    )?;
    Menu::with_items(
        app,
        &[
            &Submenu::with_items(
                app,
                APP_NAME,
                true,
                &[
                    &PredefinedMenuItem::about(app, None, Some(about))?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::hide(app, None)?,
                    &PredefinedMenuItem::hide_others(app, None)?,
                    &PredefinedMenuItem::separator(app)?,
                    &quit,
                ],
            )?,
            &Submenu::with_items(
                app,
                "File",
                true,
                &[&PredefinedMenuItem::close_window(app, None)?],
            )?,
            // Text editing shortcuts (copy, paste...) in inputs need these.
            &Submenu::with_items(
                app,
                "Edit",
                true,
                &[
                    &PredefinedMenuItem::undo(app, None)?,
                    &PredefinedMenuItem::redo(app, None)?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::cut(app, None)?,
                    &PredefinedMenuItem::copy(app, None)?,
                    &PredefinedMenuItem::paste(app, None)?,
                    &PredefinedMenuItem::select_all(app, None)?,
                ],
            )?,
            &Submenu::with_items(
                app,
                "Window",
                true,
                &[
                    &PredefinedMenuItem::minimize(app, None)?,
                    &PredefinedMenuItem::maximize(app, None)?,
                    &PredefinedMenuItem::fullscreen(app, None)?,
                ],
            )?,
        ],
    )
}

pub fn run() {
    let nav_guard = tauri::plugin::Builder::<tauri::Wry>::new("nav-guard")
        .on_navigation(|_, url| is_app_url(url))
        .build();

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(nav_guard)
        .manage(AppState::default());

    #[cfg(target_os = "macos")]
    let builder = builder.menu(mac_menu).on_menu_event(|app, event| {
        if event.id() == "quit" {
            let handle = app.clone();
            confirm_discard(app, move || handle.exit(0));
        }
    });

    builder
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.state::<AppState>().lock().is_dirty() {
                    api.prevent_close();
                    let w = window.clone();
                    confirm_discard(window.app_handle(), move || {
                        let _ = w.destroy();
                    });
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_view,
            new_budget,
            open_budget,
            save_budget,
            undo,
            redo,
            set_title,
            add_category,
            add_production_total,
            update_topsheet_row,
            delete_topsheet_row,
            add_account,
            update_account,
            delete_account,
            set_detail_field,
            set_rows_field,
            add_detail,
            delete_details,
            selection_totals,
            set_global,
            delete_global,
            set_fringe,
            delete_fringe,
            set_group,
            delete_group,
            set_unit,
            delete_unit,
            set_currency,
            delete_currency,
        ])
        .setup(|app| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.set_title(&window_title(&app.state::<AppState>().lock()));
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Film Wizard Budgeting");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_bundled_ui_urls_are_allowed() {
        let ok = |s: &str| is_app_url(&s.parse().unwrap());
        assert!(ok("tauri://localhost/index.html"));
        assert!(ok("http://tauri.localhost/index.html"));
        assert!(!ok("https://example.com/"));
        assert!(!ok("file:///etc/passwd"));
        assert!(!ok("tauri://evil/"));
        assert!(!ok("http://tauri.localhost.example.com/"));
    }

    #[test]
    fn file_names() {
        assert_eq!(safe_file_stem("My Film: Part 2/3"), "My Film  Part 2 3");
        assert_eq!(safe_file_stem(" .. "), "Untitled");
        assert_eq!(
            with_fwb_extension(PathBuf::from("/x/budget")),
            PathBuf::from("/x/budget.fwb")
        );
        assert_eq!(
            with_fwb_extension(PathBuf::from("/x/budget.FWB")),
            PathBuf::from("/x/budget.FWB")
        );
        assert_eq!(
            with_fwb_extension(PathBuf::from("/x/v1.2")),
            PathBuf::from("/x/v1.2.fwb")
        );
    }

    #[test]
    fn title_marks_unsaved_changes() {
        let mut s = Session::new();
        assert_eq!(window_title(&s), "Untitled.fwb - Film Wizard Budgeting");
        s.edit(|b| edit::set_title(b, "Changed")).unwrap();
        assert_eq!(
            window_title(&s),
            "Untitled.fwb (edited) - Film Wizard Budgeting"
        );
    }
}
