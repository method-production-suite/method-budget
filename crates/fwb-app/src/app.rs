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
use crate::edit::{ChargeInput, CreditInput, InfoInput, LocationInput, SetInput};
use crate::report_dto::SelectionDto;
use crate::session::Session;
use crate::view::{Focus, FullView, View};

const APP_NAME: &str = "Film Wizard Budgeting";
const FILTER_NAME: &str = "Film Wizard budget";

#[derive(Default)]
pub struct AppState {
    session: Mutex<Session>,
}

impl AppState {
    pub(crate) fn lock(&self) -> MutexGuard<'_, Session> {
        // A panic mid-command can't leave the budget half-edited (edits apply
        // to a copy), so a poisoned lock is still safe to use.
        self.session.lock().unwrap_or_else(|e| e.into_inner())
    }
}

type CmdResult<T = View> = Result<T, String>;

pub(crate) fn window_title(s: &Session) -> String {
    let edited = if s.is_dirty() { " (edited)" } else { "" };
    format!("{}{edited} - {APP_NAME}", s.display_name())
}

/// The view after a command, with the main window's title kept in sync
/// (whichever window sent the command), and the other windows told when the
/// budget changed (src/tools.rs).
pub(crate) fn finish<R: Runtime>(
    window: &WebviewWindow<R>,
    s: &mut Session,
    created: Option<RowId>,
) -> View {
    if let Some(main) = window.app_handle().get_webview_window(crate::tools::MAIN) {
        let _ = main.set_title(&window_title(s));
    }
    let view = s.view_with(created);
    crate::tools::changed(window, view.rev);
    view
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
pub(crate) fn edit_budget<R: Runtime, T>(
    window: &WebviewWindow<R>,
    state: &AppState,
    f: impl FnOnce(&mut fwb_core::Budget) -> edit::EditResult<T>,
) -> CmdResult {
    apply(window, state, |s| s.edit(f))
}

// ---------------------------------------------------------------------------
// Views

#[tauri::command]
pub fn get_view<R: Runtime>(window: WebviewWindow<R>, state: State<'_, AppState>) -> View {
    finish(&window, &mut state.lock(), None)
}

/// Everything, every row: for printing. Other commands return the lean view.
#[tauri::command]
pub fn get_full_view(state: State<'_, AppState>) -> FullView {
    state.lock().full_view()
}

/// Shows an account: its rows go out with this and every later view, until
/// another account (or `null`, for none) is shown. The account is named as
/// the UI knows it: category code and topsheet index, account code and index
/// in the category (codes win when they disagree).
#[tauri::command]
pub fn focus_account<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    category: Option<String>,
    ti: Option<usize>,
    account: Option<String>,
    ai: Option<usize>,
) -> View {
    let mut s = state.lock();
    let focus = category.zip(account).map(|(category, account)| Focus {
        category,
        ti: ti.unwrap_or(usize::MAX),
        account,
        ai: ai.unwrap_or(usize::MAX),
    });
    s.set_focus(focus);
    finish(&window, &mut s, None)
}

// ---------------------------------------------------------------------------
// Files

/// Replaces the budget with a fresh starter. The UI confirms first when there
/// are unsaved changes.
#[tauri::command]
pub fn new_budget<R: Runtime>(window: WebviewWindow<R>, state: State<'_, AppState>) -> View {
    // The old budget's recovery copy goes with it (src/safety_app.rs).
    crate::safety_app::replace_session(&state, &window.state(), Session::new());
    let mut s = state.lock();
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
    // Opens it, drops the old budget's recovery copy, lists it in Open Recent.
    crate::safety_app::open_path(&state, &window.state(), &path)?;
    crate::safety_app::refresh_menu(window.app_handle());
    let mut s = state.lock();
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
        Some(p) if !save_as => {
            // A budget recovered after a crash asks before replacing its file.
            if !crate::safety_app::confirm_overwrite(&window, &state) {
                return Ok(None);
            }
            p
        }
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
    // Backs up the file's previous version, saves atomically, drops the
    // recovery copy and lists the file in Open Recent (src/safety_app.rs).
    // If the file changed outside the app, nothing is written and the page
    // asks what to do instead.
    crate::safety_app::save_from(&window, &state, &path)?;
    crate::safety_app::refresh_menu(window.app_handle());
    let mut s = state.lock();
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
// Charges, credits, sets, locations, budget info, settings and the baseline
// (the topsheet setup tabs in ui/js/topsheet-setup.js)

#[tauri::command]
pub fn set_charge<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    index: Option<usize>,
    charge: ChargeInput,
) -> CmdResult {
    edit_budget(&window, &state, |b| edit::set_charge(b, index, &charge))
}

#[tauri::command]
pub fn delete_charge<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    index: usize,
) -> CmdResult {
    edit_budget(&window, &state, |b| edit::delete_charge(b, index))
}

#[tauri::command]
pub fn move_charge<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    index: usize,
    to: usize,
) -> CmdResult {
    edit_budget(&window, &state, |b| edit::move_charge(b, index, to))
}

#[tauri::command]
pub fn set_credit<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    index: Option<usize>,
    credit: CreditInput,
) -> CmdResult {
    edit_budget(&window, &state, |b| edit::set_credit(b, index, &credit))
}

#[tauri::command]
pub fn delete_credit<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    index: usize,
) -> CmdResult {
    edit_budget(&window, &state, |b| edit::delete_credit(b, index))
}

#[tauri::command]
pub fn move_credit<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    index: usize,
    to: usize,
) -> CmdResult {
    edit_budget(&window, &state, |b| edit::move_credit(b, index, to))
}

#[tauri::command]
pub fn set_set<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    name: String,
    set: SetInput,
) -> CmdResult {
    edit_budget(&window, &state, |b| edit::set_set(b, &name, &set))
}

#[tauri::command]
pub fn delete_set<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    name: String,
) -> CmdResult {
    edit_budget(&window, &state, |b| edit::delete_set(b, &name))
}

#[tauri::command]
pub fn set_location<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    name: String,
    location: LocationInput,
) -> CmdResult {
    edit_budget(&window, &state, |b| edit::set_location(b, &name, &location))
}

#[tauri::command]
pub fn delete_location<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    name: String,
) -> CmdResult {
    edit_budget(&window, &state, |b| edit::delete_location(b, &name))
}

#[tauri::command]
pub fn set_info<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    name: String,
    value: InfoInput,
) -> CmdResult {
    edit_budget(&window, &state, |b| edit::set_info(b, &name, &value))
}

#[tauri::command]
pub fn delete_info<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    name: String,
) -> CmdResult {
    edit_budget(&window, &state, |b| edit::delete_info(b, &name))
}

#[tauri::command]
pub fn set_fringe_posting<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    mode: String,
    suffix: Option<String>,
) -> CmdResult {
    edit_budget(&window, &state, |b| {
        edit::set_fringe_posting(b, &mode, suffix.as_deref())
    })
}

/// Arguments are snake_case here (`category_width`...), matching the file format.
#[tauri::command(rename_all = "snake_case")]
pub fn set_account_coding<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    category_width: u8,
    account_width: u8,
    sub_width: u8,
    set_width: u8,
    separator: Option<String>,
) -> CmdResult {
    edit_budget(&window, &state, |b| {
        edit::set_account_coding(
            b,
            [category_width, account_width, sub_width, set_width],
            separator.as_deref(),
        )
    })
}

#[tauri::command]
pub fn set_base_currency<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    code: String,
) -> CmdResult {
    edit_budget(&window, &state, |b| edit::set_base_currency(b, &code))
}

#[tauri::command]
pub fn set_totals_currency<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    code: Option<String>,
) -> CmdResult {
    edit_budget(&window, &state, |b| {
        edit::set_totals_currency(b, code.as_deref())
    })
}

#[tauri::command]
pub fn lock_baseline<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
) -> CmdResult {
    edit_budget(&window, &state, edit::lock_baseline)
}

#[tauri::command]
pub fn clear_baseline<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
) -> CmdResult {
    edit_budget(&window, &state, edit::clear_baseline)
}

// ---------------------------------------------------------------------------
// Several cells at once: paste, fill down, cut and clear (src/batch.rs)

/// Sets many detail cells as one undo step, recalculating once.
#[tauri::command]
pub fn set_cells<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    edits: Vec<crate::batch::CellEdit>,
) -> CmdResult {
    edit_budget(&window, &state, |b| crate::batch::set_cells(b, &edits))
}

// ---------------------------------------------------------------------------
// Unsaved-changes guard

const KEEP_EDITING: &str = "Keep editing";
const DISCARD: &str = "Close without saving";

/// Asks before throwing away unsaved changes; runs `then` if the user agrees
/// (or there was nothing to lose). Never blocks the event loop.
fn confirm_discard<R: Runtime>(app: &AppHandle<R>, then: impl FnOnce() + Send + 'static) {
    // With Autosave on, unsaved changes go into the file first (src/safety_app.rs).
    let saved = crate::safety_app::save_before_leaving(app);
    let dirty = !saved && app.state::<AppState>().lock().is_dirty();
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
    let handle = app.clone();
    dialog.show_with_result(move |choice| {
        if choice == MessageDialogResult::Custom(DISCARD.into()) {
            // Thrown away on purpose: no recovery copy of it either.
            crate::safety_app::discarded(&handle);
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

pub fn run() {
    let nav_guard = tauri::plugin::Builder::<tauri::Wry>::new("nav-guard")
        .on_navigation(|_, url| is_app_url(url))
        .build();

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(nav_guard)
        .manage(AppState::default())
        // The row clipboard: one for the app, shared by every budget (src/clipboard.rs).
        .manage(crate::clipboard::ClipboardState::default())
        // Which budget change the other window has heard about (src/tools.rs).
        .manage(crate::tools::Broadcast::default());

    // The menu bar (src/menu.rs). Quit asks about unsaved changes first, like
    // closing the window; every other item goes to menu::handle.
    let builder = builder
        .menu(crate::menu::build)
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            if id == crate::menu::QUIT {
                let handle = app.clone();
                confirm_discard(app, move || handle.exit(0));
            } else if !crate::safety_app::handle_menu(app, id) {
                // Open Recent's Clear Menu and the Autosave check are answered
                // in src/safety_app.rs.
                crate::menu::handle(app, id);
            }
        });

    builder
        .on_window_event(|window, event| {
            // The tools window: remember where it is, dock the panel back when
            // it closes, and close it with the main window (src/tools.rs).
            crate::tools::on_window_event(window, event);
            // Leaving the app autosaves now rather than in a few seconds.
            // Going from the main window to the tools window (or back) isn't
            // leaving: focus lands on the other window a moment later.
            if let tauri::WindowEvent::Focused(false) = event {
                let app = window.app_handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(crate::tools::FOCUS_SETTLE);
                    if !crate::tools::app_in_front(&app) {
                        crate::safety_app::autosave_now(&app, true);
                    }
                });
            }
            // Back in the window: ask now if the file changed outside meanwhile.
            if let tauri::WindowEvent::Focused(true) = event {
                let app = window.app_handle().clone();
                std::thread::spawn(move || crate::safety_app::check_external(&app));
            }
            // Only the main window asks about unsaved changes; closing the
            // tools window just docks the panel.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == crate::tools::MAIN
                    && window.state::<AppState>().lock().is_dirty()
                {
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
            get_full_view,
            focus_account,
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
            // Topsheet setup tabs
            set_charge,
            delete_charge,
            move_charge,
            set_credit,
            delete_credit,
            move_credit,
            set_set,
            delete_set,
            set_location,
            delete_location,
            set_info,
            delete_info,
            set_fringe_posting,
            set_account_coding,
            set_base_currency,
            set_totals_currency,
            lock_baseline,
            clear_baseline,
            // Several cells at once
            set_cells,
            // Row style, notes, group colors
            crate::rowstyle::set_row_style,
            crate::rowstyle::set_row_note,
            crate::rowstyle::set_group_color,
            crate::rowstyle::update_group,
            // Apply panel (src/apply.rs)
            crate::apply::apply_summary,
            crate::apply::apply_to_rows,
            crate::apply::assign_to_rows,
            crate::apply::make_fringe_range,
            crate::apply::clear_fringe_range,
            // Quick entry: add setup items from a cell, new-line defaults (src/quickadd.rs)
            crate::quickadd::set_cell_creating,
            crate::quickadd::set_new_line_defaults,
            // Copy and paste whole rows, accounts and categories with their
            // setup (src/clipboard.rs)
            crate::clipboard::clip_copy,
            crate::clipboard::clip_cut,
            crate::clipboard::clip_preview,
            crate::clipboard::clip_paste,
            crate::clipboard::clip_forget,
            // Saving safety: autosave, recovery, backups, recent and
            // system-opened files, outside changes (src/safety_app.rs)
            crate::safety_app::autosave_setting,
            crate::safety_app::set_autosave,
            crate::safety_app::flush_autosave,
            crate::safety_app::open_pending,
            crate::safety_app::open_recent,
            crate::safety_app::recovery_list,
            crate::safety_app::open_recovered,
            crate::safety_app::discard_recovered,
            crate::safety_app::backup_list,
            crate::safety_app::restore_backup,
            crate::safety_app::reveal_backups,
            crate::safety_app::resolve_external_change,
            // The tools panel's own window and its Notes tab (src/tools.rs)
            crate::tools::tools_window,
            crate::tools::close_tools_window,
            crate::tools::notes_list,
        ])
        .setup(|app| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.set_title(&window_title(&app.state::<AppState>().lock()));
            }
            // Autosave, recovery, backups, recent files (src/safety_app.rs).
            crate::safety_app::init(app.handle());
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while running Film Wizard Budgeting")
        .run(|app, event| match event {
            // macOS hands over double-clicked and Open With files here
            // (Windows and Linux pass them on the command line).
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Opened { urls } => crate::safety_app::on_opened(
                app,
                urls.iter().filter_map(|u| u.to_file_path().ok()).collect(),
            ),
            tauri::RunEvent::Exit => crate::safety_app::on_exit(app),
            _ => {}
        });
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
