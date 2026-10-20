//! Tauri glue: the commands the UI invokes, native file dialogs, and the
//! unsaved-changes guard on close and quit. What a budget command does is in
//! method-session (`method_session::commands`, decision 70): the commands
//! here lock the budget, call it, and draw the view with the window's side
//! ([`finish`]).
//!
//! Security notes (see docs/APP.md): file paths only ever come from native
//! dialogs shown here in Rust, never from the web view; every command works on
//! one of the open budgets in [`AppState`] (the one in front, or the one its
//! `doc` argument names: src/docs.rs); navigation away from the bundled UI is
//! blocked.

use std::path::{Path, PathBuf};

use method_core::RowId;
use tauri::{AppHandle, Manager, Runtime, State, WebviewWindow};
use tauri_plugin_dialog::{
    DialogExt, MessageDialogButtons, MessageDialogKind, MessageDialogResult,
};

pub use crate::docs::{AppState, DocGuard, DocId};
use crate::edit::{self, CurrencyInput, FringeInput, GroupInput, UnitInput};
use crate::edit::{ChargeInput, CreditInput, InfoInput, LocationInput, SetInput};
use crate::report_dto::SelectionDto;
use crate::session::Session;
use crate::view::{FullView, View};
use method_session::commands;

pub(crate) const APP_NAME: &str = "Method Budget";
/// The product's website (the About box links it).
pub(crate) const WEBSITE: &str = "https://methodbudget.com";
/// The file type's name in Open and Save dialogs (with [`method_format::EXTENSIONS`]).
pub(crate) const FILTER_NAME: &str = "Method Budget file";

type CmdResult<T = View> = Result<T, String>;

pub(crate) fn window_title(s: &Session) -> String {
    let edited = if s.is_dirty() { " (edited)" } else { "" };
    format!("{}{edited} - {APP_NAME}", s.display_name())
}

/// The view after a command, with the tabs listed and the main window's
/// title kept in sync with the budget in front (whichever window sent the
/// command), and the other windows told when the budget changed
/// (src/tools.rs). The Window menu and the list of tabs to reopen next time
/// follow (src/tabs.rs).
pub(crate) fn finish<R: Runtime>(
    window: &WebviewWindow<R>,
    s: &mut DocGuard<'_>,
    created: Option<RowId>,
) -> View {
    if s.is_active() {
        if let Some(main) = window.app_handle().get_webview_window(crate::tools::MAIN) {
            let _ = main.set_title(&window_title(s));
        }
    }
    // A review file open on the budget follows an undo or redo that took
    // back or brought back a merge (src/review.rs).
    if method_session::review::needs_write(s) {
        crate::review::write_back(s);
    }
    let view = s.finish(created);
    crate::tools::changed(window, view.doc, view.rev);
    crate::tabs::after_command(window.app_handle(), s.docs());
    view
}

pub(crate) fn apply<R: Runtime, T>(
    window: &WebviewWindow<R>,
    state: &AppState,
    doc: Option<DocId>,
    f: impl FnOnce(&mut Session) -> edit::EditResult<T>,
) -> CmdResult {
    let mut s = state.lock_doc(doc)?;
    f(&mut s)?;
    Ok(finish(window, &mut s, None))
}

// ---------------------------------------------------------------------------
// Views

/// The view of budget `doc` (`None`: the one in front, which the tools window
/// asks for to follow the main window's tabs).
#[tauri::command]
pub fn get_view<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
) -> CmdResult {
    Ok(finish(&window, &mut state.lock_doc(doc)?, None))
}

/// Which host the page runs in: the desktop, with the command contract's
/// version (`method_session::host`), never read-only. The page will use it
/// to hide what only the desktop does when it runs in a browser.
#[tauri::command]
pub fn host_info(state: State<'_, AppState>) -> method_session::HostInfo {
    method_session::HostInfo::new("desktop", state.docs().read_only())
}

/// Everything, every row: for printing. Other commands return the lean view.
#[tauri::command]
pub fn get_full_view(state: State<'_, AppState>, doc: Option<DocId>) -> CmdResult<FullView> {
    Ok(state.lock_doc(doc)?.full_view())
}

/// Shows an account: its rows go out with this and every later view, until
/// another account (or `null`, for none) is shown. The account is named as
/// the UI knows it: category code and topsheet index, account code and index
/// in the category (codes win when they disagree).
#[tauri::command]
pub fn focus_account<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    category: Option<String>,
    ti: Option<usize>,
    account: Option<String>,
    ai: Option<usize>,
) -> CmdResult {
    apply(&window, &state, doc, |s| {
        commands::focus_account(s, category, ti, account, ai);
        Ok(())
    })
}

// ---------------------------------------------------------------------------
// Files

/// Shows an Open dialog for a budget, a budget export or a spreadsheet and
/// opens the chosen file the way its content asks (src/open_any.rs): a
/// budget in a new tab (or its own tab when it is open already; an
/// untouched Untitled budget gives its tab up to it), an export as Import
/// Budget Export's review, a spreadsheet as Import Spreadsheet's dialog.
/// `None` if cancelled.
#[tauri::command]
pub async fn open_budget<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
) -> CmdResult<Option<crate::open_any::Opened>> {
    let start = state
        .lock()
        .path()
        .and_then(|p| p.parent())
        .map(PathBuf::from);
    let mut dialog = window
        .dialog()
        .file()
        .set_title("Open")
        .add_filter(
            crate::open_any::ALL_FILTER,
            &crate::open_any::open_extensions(),
        )
        .add_filter(FILTER_NAME, method_format::EXTENSIONS)
        .add_filter(
            "Budget export",
            method_format::import::exchange::READABLE_EXTENSIONS,
        )
        .add_filter("Spreadsheet", crate::open_any::SHEET_EXTENSIONS)
        .set_parent(&window);
    if let Some(dir) = start {
        dialog = dialog.set_directory(dir);
    }
    let Some(picked) = dialog.blocking_pick_file() else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(|e| e.to_string())?;
    crate::open_any::open_any(&window, &state, &path).map(Some)
}

/// Rust's Open dialog for one budget file (`.mthd`),
/// titled `title`, starting in folder `start`: the file picked, or `None`
/// when cancelled. Blocks until the user answers: never on the main thread.
pub(crate) fn pick_budget<R: Runtime>(
    window: &WebviewWindow<R>,
    title: &str,
    start: Option<PathBuf>,
) -> CmdResult<Option<PathBuf>> {
    let mut dialog = window
        .dialog()
        .file()
        .set_title(title)
        .add_filter(FILTER_NAME, method_format::EXTENSIONS)
        .set_parent(window);
    if let Some(dir) = start {
        dialog = dialog.set_directory(dir);
    }
    let Some(picked) = dialog.blocking_pick_file() else {
        return Ok(None);
    };
    picked.into_path().map(Some).map_err(|e| e.to_string())
}

/// Saves budget `doc` (`None`: the one in front) to its file, or asks where
/// (always, with `save_as`). `None` if the user cancelled the dialog; the
/// view's `file.dirty` stays true when the file changed outside the app and
/// nothing was written (the page asks what to do).
#[tauri::command]
pub async fn save_budget<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    save_as: bool,
    doc: Option<DocId>,
) -> CmdResult<Option<View>> {
    let id = state.lock_doc(doc)?.id();
    if save_doc(&window, &state, id, save_as, None)?.is_none() {
        return Ok(None);
    }
    crate::safety_app::refresh_menu(window.app_handle());
    Ok(Some(finish(&window, &mut state.lock_doc(Some(id))?, None)))
}

/// Saves budget `id` to its file, or asks where (always, with `save_as`;
/// `dialog_title` names the budget when several are being saved). Shows
/// dialogs, so never call it on the main thread. `None` when the user
/// cancelled; `Some(false)` when the file changed outside the app and nothing
/// was written (the page has been told and asks what to do).
fn save_doc<R: Runtime>(
    window: &WebviewWindow<R>,
    state: &AppState,
    id: DocId,
    save_as: bool,
    dialog_title: Option<&str>,
) -> CmdResult<Option<bool>> {
    let (current, start) = {
        let s = state.lock_doc(Some(id))?;
        let start = save_dialog_start(s.path(), s.save_hint(), &s.budget().title);
        (s.path().map(PathBuf::from), start)
    };
    let path = match current {
        Some(p) if !save_as => {
            // A budget recovered after a crash asks before replacing its file.
            if !crate::safety_app::confirm_overwrite(window, state, id) {
                return Ok(None);
            }
            p
        }
        _ => {
            let title = match dialog_title {
                Some(t) => t.to_string(),
                None if save_as => "Save budget as".to_string(),
                None => "Save budget".to_string(),
            };
            let mut dialog = window
                .dialog()
                .file()
                .set_title(title)
                .add_filter(FILTER_NAME, &[method_format::EXTENSION])
                .set_file_name(start.1)
                .set_parent(window);
            if let Some(dir) = start.0 {
                dialog = dialog.set_directory(dir);
            }
            let Some(picked) = dialog.blocking_save_file() else {
                return Ok(None);
            };
            with_budget_extension(picked.into_path().map_err(|e| e.to_string())?)
        }
    };
    // Backs up the file's previous version, saves atomically, drops the
    // recovery copy and lists the file in Open Recent (src/safety_app.rs).
    // If the file changed outside the app, nothing is written and the page
    // asks what to do instead.
    crate::safety_app::save_from(window, state, id, &path).map(Some)
}

/// Where the Save dialog starts and the file name it suggests, for a budget
/// at `path` (`None`: never saved) titled `title`. A budget never saved
/// that came from a file (`hint`: a converted export's folder and name with
/// `.mthd`, decisions.md entry 67) starts there; otherwise a saved budget's
/// own folder and name, or the title. The dialog asks before replacing a
/// file, as always.
pub(crate) fn save_dialog_start(
    path: Option<&Path>,
    hint: Option<&Path>,
    title: &str,
) -> (Option<PathBuf>, String) {
    let at = path.or(hint);
    let stem = match at.and_then(|p| p.file_stem()) {
        Some(stem) => stem.to_string_lossy().into_owned(),
        None => title.trim().to_string(),
    };
    let dir = at
        .and_then(|p| p.parent())
        .filter(|d| !d.as_os_str().is_empty())
        .map(PathBuf::from);
    (
        dir,
        format!("{}.{}", safe_file_stem(&stem), method_format::EXTENSION),
    )
}

pub(crate) use method_session::templates::safe_file_stem;

/// `p` as picked in a Save dialog, with `.mthd` added unless it already has
/// the budget extension.
pub(crate) fn with_budget_extension(mut p: PathBuf) -> PathBuf {
    if !method_format::has_budget_extension(&p) {
        let mut name = p.file_name().unwrap_or_default().to_os_string();
        name.push(".");
        name.push(method_format::EXTENSION);
        p.set_file_name(name);
    }
    p
}

// ---------------------------------------------------------------------------
// History

#[tauri::command]
pub fn undo<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
) -> CmdResult {
    let mut s = state.lock_doc(doc)?;
    s.undo();
    Ok(finish(&window, &mut s, None))
}

#[tauri::command]
pub fn redo<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
) -> CmdResult {
    let mut s = state.lock_doc(doc)?;
    s.redo();
    Ok(finish(&window, &mut s, None))
}

// ---------------------------------------------------------------------------
// Structure

#[tauri::command]
pub fn set_title<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    title: String,
) -> CmdResult {
    apply(&window, &state, doc, |s| commands::set_title(s, &title))
}

#[tauri::command]
pub fn add_category<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    code: String,
    name: String,
    at: Option<usize>,
) -> CmdResult {
    apply(&window, &state, doc, |s| {
        commands::add_category(s, &code, &name, at)
    })
}

#[tauri::command]
pub fn add_production_total<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
    at: Option<usize>,
) -> CmdResult {
    apply(&window, &state, doc, |s| {
        commands::add_production_total(s, &name, at)
    })
}

#[tauri::command]
pub fn update_topsheet_row<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    index: usize,
    code: Option<String>,
    name: String,
) -> CmdResult {
    apply(&window, &state, doc, |s| {
        commands::update_topsheet_row(s, index, code.as_deref(), &name)
    })
}

#[tauri::command]
pub fn delete_topsheet_row<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    index: usize,
) -> CmdResult {
    apply(&window, &state, doc, |s| {
        commands::delete_topsheet_row(s, index)
    })
}

#[tauri::command]
pub fn add_account<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    category_code: String,
    code: String,
    name: String,
    after: Option<String>,
) -> CmdResult {
    apply(&window, &state, doc, |s| {
        commands::add_account(s, &category_code, &code, &name, after.as_deref())
    })
}

#[tauri::command]
pub fn update_account<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    code: String,
    new_code: String,
    name: String,
) -> CmdResult {
    apply(&window, &state, doc, |s| {
        commands::update_account(s, &code, &new_code, &name)
    })
}

#[tauri::command]
pub fn delete_account<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    code: String,
) -> CmdResult {
    apply(&window, &state, doc, |s| commands::delete_account(s, &code))
}

// ---------------------------------------------------------------------------
// Detail rows

#[tauri::command]
pub fn set_detail_field<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    row_id: RowId,
    field: String,
    value: String,
) -> CmdResult {
    apply(&window, &state, doc, |s| {
        commands::set_detail_field(s, row_id, &field, &value)
    })
}

#[tauri::command]
pub fn set_rows_field<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    row_ids: Vec<RowId>,
    field: String,
    value: String,
) -> CmdResult {
    apply(&window, &state, doc, |s| {
        commands::set_rows_field(s, &row_ids, &field, &value)
    })
}

#[tauri::command]
pub fn add_detail<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    account_code: String,
    after: Option<RowId>,
    kind: String,
) -> CmdResult {
    let mut s = state.lock_doc(doc)?;
    let id = s.add_detail(&account_code, after, &kind)?;
    Ok(finish(&window, &mut s, Some(id)))
}

#[tauri::command]
pub fn delete_details<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    row_ids: Vec<RowId>,
) -> CmdResult {
    apply(&window, &state, doc, |s| {
        commands::delete_details(s, &row_ids)
    })
}

#[tauri::command]
pub fn selection_totals(
    state: State<'_, AppState>,
    doc: Option<DocId>,
    row_ids: Vec<RowId>,
) -> CmdResult<SelectionDto> {
    Ok(state.lock_doc(doc)?.selection(&row_ids))
}

// ---------------------------------------------------------------------------
// Setup tables

#[tauri::command]
pub fn set_global<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
    formula: String,
) -> CmdResult {
    apply(&window, &state, doc, |s| {
        commands::set_global(s, &name, &formula)
    })
}

#[tauri::command]
pub fn delete_global<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
) -> CmdResult {
    apply(&window, &state, doc, |s| commands::delete_global(s, &name))
}

#[tauri::command]
pub fn set_fringe<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
    fringe: FringeInput,
) -> CmdResult {
    apply(&window, &state, doc, |s| {
        commands::set_fringe(s, &name, &fringe)
    })
}

#[tauri::command]
pub fn delete_fringe<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
) -> CmdResult {
    apply(&window, &state, doc, |s| commands::delete_fringe(s, &name))
}

#[tauri::command]
pub fn set_group<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
    group: GroupInput,
) -> CmdResult {
    apply(&window, &state, doc, |s| {
        commands::set_group(s, &name, &group)
    })
}

/// One of the budget's group rules (decisions.md, entry 57): `rule` is
/// `ungrouped_lines`, `conflict` or `fringes_in_totals`, `value` is
/// `include` or `exclude`.
#[tauri::command]
pub fn set_group_rule<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    rule: String,
    value: String,
) -> CmdResult {
    apply(&window, &state, doc, |s| {
        commands::set_group_rule(s, &rule, &value)
    })
}

/// How the budget works out its totals (decisions.md, entry 55): `value` is
/// `binary` or `exact`.
#[tauri::command]
pub fn set_arithmetic<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    value: String,
) -> CmdResult {
    apply(&window, &state, doc, |s| {
        commands::set_arithmetic(s, &value)
    })
}

/// Whether account totals are rounded to whole units (decisions.md, entry
/// 69).
#[tauri::command]
pub fn set_round_account_totals<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    value: bool,
) -> CmdResult {
    apply(&window, &state, doc, |s| {
        commands::set_round_account_totals(s, value)
    })
}

/// What every percent fringe's cutoff is multiplied by (decisions.md, entry
/// 59): `value` is `1.0`, `1.5`, `2.0`, `2.5` or `3.0`.
#[tauri::command]
pub fn set_cutoff_factor<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    value: String,
) -> CmdResult {
    apply(&window, &state, doc, |s| {
        commands::set_cutoff_factor(s, &value)
    })
}

/// The budget's fringe row: its account number, description and place on
/// the topsheet (decisions.md, entry 60).
#[tauri::command]
pub fn set_fringe_row<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    row: edit::FringeRowInput,
) -> CmdResult {
    apply(&window, &state, doc, |s| commands::set_fringe_row(s, &row))
}

/// The fringes of categories after the last production total (decisions.md,
/// entry 56): `value` is `keep` or `drop`.
#[tauri::command]
pub fn set_trailing_level_fringes<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    value: String,
) -> CmdResult {
    apply(&window, &state, doc, |s| {
        commands::set_trailing_level_fringes(s, &value)
    })
}

#[tauri::command]
pub fn delete_group<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
) -> CmdResult {
    apply(&window, &state, doc, |s| commands::delete_group(s, &name))
}

#[tauri::command]
pub fn set_unit<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
    unit: UnitInput,
) -> CmdResult {
    apply(&window, &state, doc, |s| {
        commands::set_unit(s, &name, &unit)
    })
}

#[tauri::command]
pub fn delete_unit<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
    clear_lines: Option<bool>,
) -> CmdResult {
    apply(&window, &state, doc, |s| {
        commands::delete_unit(s, &name, clear_lines.unwrap_or(false))
    })
}

#[tauri::command]
pub fn set_currency<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    code: String,
    currency: CurrencyInput,
) -> CmdResult {
    apply(&window, &state, doc, |s| {
        commands::set_currency(s, &code, &currency)
    })
}

#[tauri::command]
pub fn delete_currency<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    code: String,
    clear_lines: Option<bool>,
) -> CmdResult {
    apply(&window, &state, doc, |s| {
        commands::delete_currency(s, &code, clear_lines.unwrap_or(false))
    })
}

// ---------------------------------------------------------------------------
// Charges, credits, sets, locations, budget info, settings and the baseline
// (the topsheet setup tabs in ui/js/topsheet-setup.js)

#[tauri::command]
pub fn set_charge<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    index: Option<usize>,
    charge: ChargeInput,
) -> CmdResult {
    apply(&window, &state, doc, |s| {
        commands::set_charge(s, index, &charge)
    })
}

/// What charge `index` (or a new charge, `null`) would come to as `charge`
/// describes it, without changing anything: the charges tab's live total
/// while a value is still being typed. Figured on the current totals, so it
/// matches what saving the edit gives.
#[tauri::command]
pub fn preview_charge(
    state: State<'_, AppState>,
    doc: Option<DocId>,
    index: Option<usize>,
    charge: ChargeInput,
) -> CmdResult<crate::report_dto::ChargeDto> {
    let mut s = state.lock_doc(doc)?;
    charge_preview_in(&mut s, index, &charge)
}

/// [`preview_charge`] on session `s`.
pub fn charge_preview_in(
    s: &mut Session,
    index: Option<usize>,
    charge: &ChargeInput,
) -> CmdResult<crate::report_dto::ChargeDto> {
    commands::preview_charge(s, index, charge)
}

#[tauri::command]
pub fn delete_charge<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    index: usize,
) -> CmdResult {
    apply(&window, &state, doc, |s| commands::delete_charge(s, index))
}

#[tauri::command]
pub fn move_charge<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    index: usize,
    to: usize,
) -> CmdResult {
    apply(&window, &state, doc, |s| {
        commands::move_charge(s, index, to)
    })
}

/// Spend by location and an estimate for every credit, applied or not
/// (roadmap 3.6): the Credits tab's programs side by side. Nothing changes.
#[tauri::command]
pub fn incentives(
    state: State<'_, AppState>,
    doc: Option<DocId>,
) -> CmdResult<crate::report_dto::IncentivesDto> {
    let mut s = state.lock_doc(doc)?;
    incentives_in(&mut s)
}

/// [`incentives`] on session `s`.
pub fn incentives_in(s: &mut Session) -> CmdResult<crate::report_dto::IncentivesDto> {
    commands::incentives(s)
}

#[tauri::command]
pub fn set_credit<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    index: Option<usize>,
    credit: CreditInput,
) -> CmdResult {
    apply(&window, &state, doc, |s| {
        commands::set_credit(s, index, &credit)
    })
}

#[tauri::command]
pub fn delete_credit<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    index: usize,
) -> CmdResult {
    apply(&window, &state, doc, |s| commands::delete_credit(s, index))
}

#[tauri::command]
pub fn move_credit<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    index: usize,
    to: usize,
) -> CmdResult {
    apply(&window, &state, doc, |s| {
        commands::move_credit(s, index, to)
    })
}

#[tauri::command]
pub fn set_set<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
    set: SetInput,
) -> CmdResult {
    apply(&window, &state, doc, |s| commands::set_set(s, &name, &set))
}

#[tauri::command]
pub fn delete_set<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
) -> CmdResult {
    apply(&window, &state, doc, |s| commands::delete_set(s, &name))
}

#[tauri::command]
pub fn set_location<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
    location: LocationInput,
) -> CmdResult {
    apply(&window, &state, doc, |s| {
        commands::set_location(s, &name, &location)
    })
}

#[tauri::command]
pub fn delete_location<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
) -> CmdResult {
    apply(&window, &state, doc, |s| {
        commands::delete_location(s, &name)
    })
}

#[tauri::command]
pub fn set_info<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
    value: InfoInput,
) -> CmdResult {
    apply(&window, &state, doc, |s| {
        commands::set_info(s, &name, &value)
    })
}

#[tauri::command]
pub fn delete_info<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
) -> CmdResult {
    apply(&window, &state, doc, |s| commands::delete_info(s, &name))
}

#[tauri::command]
pub fn set_fringe_posting<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    mode: String,
    suffix: Option<String>,
) -> CmdResult {
    apply(&window, &state, doc, |s| {
        commands::set_fringe_posting(s, &mode, suffix.as_deref())
    })
}

/// Arguments are snake_case here (`category_width`...), matching the file format.
/// Each width is its own argument, as the page sends them.
#[allow(clippy::too_many_arguments)]
#[tauri::command(rename_all = "snake_case")]
pub fn set_account_coding<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    category_width: u8,
    account_width: u8,
    sub_width: u8,
    set_width: u8,
    separator: Option<String>,
) -> CmdResult {
    let widths = [category_width, account_width, sub_width, set_width];
    apply(&window, &state, doc, |s| {
        commands::set_account_coding(s, widths, separator.as_deref())
    })
}

#[tauri::command]
pub fn set_totals_currency<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    code: Option<String>,
) -> CmdResult {
    apply(&window, &state, doc, |s| {
        commands::set_totals_currency(s, code.as_deref())
    })
}

#[tauri::command]
pub fn lock_baseline<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
) -> CmdResult {
    apply(&window, &state, doc, commands::lock_baseline)
}

#[tauri::command]
pub fn set_baseline_frozen<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    frozen: bool,
) -> CmdResult {
    apply(&window, &state, doc, |s| {
        commands::set_baseline_frozen(s, frozen)
    })
}

#[tauri::command]
pub fn clear_baseline<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
) -> CmdResult {
    apply(&window, &state, doc, commands::clear_baseline)
}

// ---------------------------------------------------------------------------
// Several cells at once: paste, fill down, cut and clear (src/batch.rs)

/// Sets many detail cells as one undo step, recalculating once.
#[tauri::command]
pub fn set_cells<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    edits: Vec<crate::batch::CellEdit>,
    action: Option<String>,
) -> CmdResult {
    apply(&window, &state, doc, |s| {
        commands::set_cells(s, &edits, action.as_deref())
    })
}

// ---------------------------------------------------------------------------
// Unsaved-changes guard: closing the window or quitting asks once about every
// budget with unsaved changes.

/// The question when budgets `names` have unsaved changes: the message, and
/// the labels of the save, discard and cancel buttons.
pub(crate) fn close_question(names: &[String]) -> (String, [&'static str; 3]) {
    if let [one] = names {
        return (
            format!("\"{one}\" has unsaved changes. Save them before closing? If you don't, they will be lost."),
            ["Save", "Discard", "Cancel"],
        );
    }
    let list: Vec<String> = names.iter().map(|n| format!("\u{2022} {n}")).collect();
    (
        format!(
            "{} budgets have unsaved changes:\n\n{}\n\nSave them before closing? If you don't, they will be lost.",
            names.len(),
            list.join("\n")
        ),
        ["Save All", "Discard All", "Cancel"],
    )
}

/// Asks before closing with unsaved changes, in one question about every
/// budget that has them: Save All, Discard All or Cancel. Runs `then` once
/// nothing unsaved is left (saved, thrown away on purpose, or nothing to
/// lose). Never blocks the event loop.
/// (Also Restart to update: src/updater.rs.)
pub(crate) fn confirm_close<R: Runtime>(app: &AppHandle<R>, then: impl FnOnce() + Send + 'static) {
    // With Autosave on, unsaved changes go into their files first
    // (src/safety_app.rs); what is left is untitled or can't be autosaved.
    let dirty = crate::safety_app::save_before_leaving(app);
    if dirty.is_empty() {
        then();
        return;
    }
    let names: Vec<String> = {
        let state = app.state::<AppState>();
        let docs = state.docs();
        dirty
            .iter()
            .filter_map(|id| docs.get(*id).map(Session::tab_label))
            .collect()
    };
    let (message, [save, discard, cancel]) = close_question(&names);
    let mut dialog = app
        .dialog()
        .message(message)
        .title("Unsaved changes")
        .kind(MessageDialogKind::Warning)
        // Saving comes first, so it is the default (Enter); Escape and
        // closing the alert cancel. Only an explicit click on the discard
        // button throws work away.
        .buttons(MessageDialogButtons::YesNoCancelCustom(
            save.into(),
            discard.into(),
            cancel.into(),
        ));
    if let Some(w) = app.get_webview_window(crate::tools::MAIN) {
        dialog = dialog.parent(&w);
    }
    let handle = app.clone();
    dialog.show_with_result(move |choice| match choice {
        MessageDialogResult::Custom(c) if c == save => {
            // Save dialogs wait for the user, so not on the event loop.
            std::thread::spawn(move || {
                if save_all(&handle, &dirty) {
                    then();
                }
            });
        }
        MessageDialogResult::Custom(c) if c == discard => {
            // Thrown away on purpose: no recovery copies of them either.
            crate::safety_app::discarded(&handle);
            then();
        }
        _ => {}
    });
}

/// Save All, on the way out: each budget in `ids` into its file, an untitled
/// one where its Save dialog says. Stops at the first one that isn't saved
/// (its dialog cancelled, its file changed outside the app, a failed write),
/// so nothing closes. True when every one was saved.
fn save_all<R: Runtime>(app: &AppHandle<R>, ids: &[DocId]) -> bool {
    let Some(window) = app.get_webview_window(crate::tools::MAIN) else {
        return false;
    };
    let state = app.state::<AppState>();
    for &id in ids {
        let Some(label) = state.docs().get(id).map(Session::tab_label) else {
            continue; // closed meanwhile
        };
        let title = format!("Save \"{label}\"");
        match save_doc(&window, &state, id, false, Some(&title)) {
            Ok(Some(true)) => crate::safety_app::tell_saved(app, id),
            Ok(_) => return false,
            Err(e) => {
                window
                    .dialog()
                    .message(format!("{e}\n\nNothing was closed."))
                    .title("Couldn't save")
                    .kind(MessageDialogKind::Error)
                    .parent(&window)
                    .blocking_show();
                return false;
            }
        }
    }
    crate::safety_app::refresh_menu(app);
    true
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
    let context = tauri::generate_context!();

    let nav_guard = tauri::plugin::Builder::<tauri::Wry>::new("nav-guard")
        .on_navigation(|_, url| is_app_url(url))
        .build();

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(nav_guard)
        .manage(AppState::default())
        // The Window menu's list of budgets (src/tabs.rs).
        .manage(crate::tabs::TabMenu::default())
        // The row clipboard: one for the app, shared by every budget (src/clipboard.rs).
        .manage(crate::clipboard::ClipboardState::default())
        // Files picked for the compare screen, read once (src/compare.rs).
        .manage(crate::compare::CompareFiles::default())
        // The budget picked to import setup from (src/import_setup.rs).
        .manage(crate::import_setup::ImportSources::default())
        // The spreadsheet picked to import lines from (src/sheet_import.rs).
        .manage(crate::sheet_import::SheetSources::default())
        // The chart of accounts picked to import (src/coa_import.rs).
        .manage(crate::coa_import::CoaSources::default())
        // The accounting export picked to import actuals from (src/actuals.rs).
        .manage(crate::actuals::ActualsSources::default())
        // The budget export picked to import (src/export_import.rs).
        .manage(crate::export_import::ExportSources::default())
        // The schedule days file picked to set globals from (src/schedule_days.rs).
        .manage(crate::schedule_days::ScheduleSources::default())
        // Which budget change the other window has heard about (src/tools.rs).
        .manage(crate::tools::Broadcast::default())
        // The folder picked for sharing and the last status sent (src/shared_app.rs).
        .manage(crate::shared_app::ShareState::default());

    // The menu bar (src/menu.rs). Quit asks about unsaved changes first, like
    // closing the window; every other item goes to menu::handle.
    let builder = builder
        .menu(crate::menu::build)
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            if id == crate::menu::QUIT {
                let handle = app.clone();
                confirm_close(app, move || handle.exit(0));
            } else if !crate::safety_app::handle_menu(app, id)
                && !crate::updater::handle_menu(app, id)
                && !crate::help::handle_menu(app, id)
            {
                // Open Recent's Clear Menu and the Autosave check are answered
                // in src/safety_app.rs, Check for Updates and its automatic
                // switch in src/updater.rs, the Help menu's links in
                // src/help.rs.
                crate::menu::handle(app, id);
            }
        });

    builder
        .on_window_event(|window, event| {
            // The tools window: remember where it is, dock the panel back when
            // it closes, and close it with the main window (src/tools.rs).
            crate::tools::on_window_event(window, event);
            // The Budget Setup window, likewise (src/setupwin.rs).
            crate::setupwin::on_window_event(window, event);
            // A file dropped on the main window opens as one from the system
            // would (Open's routing, src/open_any.rs); the path stays here.
            if let tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) = event {
                if window.label() == crate::tools::MAIN {
                    crate::safety_app::on_opened(window.app_handle(), paths.clone());
                }
            }
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
            // Only the main window asks about unsaved changes (in every
            // tab); closing the tools window just docks the panel.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == crate::tools::MAIN
                    && !window.state::<AppState>().docs().dirty().is_empty()
                {
                    api.prevent_close();
                    let w = window.clone();
                    confirm_close(window.app_handle(), move || {
                        let _ = w.destroy();
                    });
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_view,
            get_full_view,
            host_info,
            focus_account,
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
            set_group_rule,
            delete_group,
            set_unit,
            delete_unit,
            set_currency,
            delete_currency,
            // Topsheet setup tabs
            set_charge,
            preview_charge,
            delete_charge,
            move_charge,
            set_credit,
            delete_credit,
            move_credit,
            incentives,
            set_set,
            delete_set,
            set_location,
            delete_location,
            set_info,
            delete_info,
            set_fringe_posting,
            set_arithmetic,
            set_trailing_level_fringes,
            set_cutoff_factor,
            set_round_account_totals,
            set_fringe_row,
            set_account_coding,
            set_totals_currency,
            lock_baseline,
            clear_baseline,
            set_baseline_frozen,
            // Several cells at once
            set_cells,
            // Row style, group colors
            crate::rowstyle::set_row_style,
            crate::rowstyle::set_group_color,
            crate::rowstyle::update_group,
            // Notes (src/notes.rs)
            crate::notes::add_note,
            crate::notes::update_note,
            crate::notes::delete_note,
            crate::notes::set_note_done,
            // Setup lists (src/setup_lists.rs)
            crate::setup_lists::setup_usage,
            crate::setup_lists::set_setup_order,
            crate::setup_lists::move_setup_item,
            crate::setup_lists::set_short_code,
            // Apply panel (src/apply.rs)
            crate::apply::apply_summary,
            crate::apply::apply_to_rows,
            crate::apply::assign_to_rows,
            crate::apply::clear_from_rows,
            crate::apply::replace_tags_on_rows,
            crate::apply::make_fringe_range,
            crate::apply::clear_fringe_range,
            // Quick entry: add setup items from a cell, new-line defaults (src/quickadd.rs)
            crate::quickadd::set_cell_creating,
            crate::quickadd::set_new_line_defaults,
            // Global info and display settings (src/display.rs)
            crate::display::set_global_info,
            crate::display::set_display_prefs,
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
            // The Budget Setup window (src/setupwin.rs)
            crate::setupwin::setup_window,
            crate::setupwin::close_setup_window,
            // Versions saved in the budget, and comparing budgets
            // (src/versions.rs, src/compare.rs)
            crate::versions::save_version,
            crate::versions::restore_version,
            crate::versions::rename_version,
            crate::versions::delete_version,
            // The History list: every undo step, jumps, a step as a version
            // (src/history.rs)
            crate::history::history_list,
            crate::history::history_jump,
            crate::history::history_save_version,
            crate::compare::compare_budgets,
            crate::compare::compare_detail,
            crate::compare::compare_pick_files,
            // What Changed and Merge Changes From (src/diffmerge.rs)
            crate::diffmerge::what_changed,
            crate::diffmerge::merge_preview,
            crate::diffmerge::merge_apply,
            crate::diffmerge::merge_pick_file,
            // Budgets as tabs (src/tabs.rs)
            crate::tabs::new_tab,
            crate::tabs::switch_tab,
            crate::tabs::move_tab,
            crate::tabs::close_tab,
            crate::tabs::list_tabs,
            // Split view: two panes of tabs side by side (src/tabs.rs)
            crate::tabs::split_view,
            crate::tabs::close_split,
            crate::tabs::set_target_pane,
            crate::tabs::move_tab_in_pane,
            crate::tabs::move_to_pane,
            crate::tabs::pane_view,
            // Reports: saved presets, printing and PDF (src/reports.rs)
            crate::reports::save_report_preset,
            crate::reports::delete_report_preset,
            crate::reports::bid_sheet,
            crate::reports::print_report,
            // Spreadsheet export: Excel, CSV, tab-delimited (src/export.rs)
            crate::export::export_budget,
            // Tag presets and line blocks (src/presets.rs)
            crate::presets::save_tag_preset,
            crate::presets::tag_preset_from_line,
            crate::presets::apply_tag_preset,
            crate::presets::save_line_preset,
            crate::presets::line_preset_preview,
            crate::presets::insert_line_preset,
            crate::presets::rename_tool_preset,
            crate::presets::delete_tool_preset,
            crate::presets::move_tool_preset,
            // Find and replace (src/find.rs)
            crate::find::find,
            crate::find::replace,
            // Moving rows up and down (src/moverows.rs)
            crate::moverows::move_rows,
            crate::moverows::move_accounts,
            crate::moverows::move_topsheet_rows,
            // Renaming a setup item everywhere (src/rename.rs)
            crate::rename::rename_setup_item,
            // Import setup from another budget, templates (src/import_setup.rs,
            // src/templates.rs)
            crate::import_setup::import_setup_pick,
            crate::import_setup::import_setup,
            crate::templates::template_new,
            crate::templates::template_save,
            // Import lines from a spreadsheet with a column mapping
            // (src/sheet_import.rs)
            crate::sheet_import::sheet_import_pick,
            crate::sheet_import::sheet_import_preview,
            crate::sheet_import::sheet_import,
            // Charts of accounts: import, apply, bundled templates
            // (src/coa_import.rs)
            crate::coa_import::coa_import_pick,
            crate::coa_import::coa_import_preview,
            crate::coa_import::coa_import,
            crate::coa_import::chart_templates,
            crate::coa_import::chart_template_new,
            // The start window and New Budget (src/start.rs)
            crate::start::start_list,
            crate::start::start_open,
            // The sample budget (src/sample.rs)
            crate::sample::sample_at_launch,
            crate::sample::sample_open,
            crate::start::start_pin,
            crate::start::start_forget,
            crate::start::start_reveal,
            crate::start::new_budget_pick_template,
            crate::start::new_budget_create,
            crate::start::new_budget_folder,
            crate::start::new_budget_pick_folder,
            crate::start::new_budget_charts,
            crate::start::new_budget_setup,
            // A whole budget from another program's export, with the
            // reconciliation (src/export_import.rs)
            crate::export_import::export_import_pick,
            crate::export_import::export_import_preview,
            crate::export_import::export_import,
            // A schedule's day counts into globals (src/schedule_days.rs)
            crate::schedule_days::schedule_days_pick,
            crate::schedule_days::schedule_days_review,
            crate::schedule_days::schedule_days_apply,
            // Review files: comments beside a budget (src/review.rs)
            crate::review::review_open_budget,
            crate::review::review_start,
            crate::review::review_open,
            crate::review::review_close,
            crate::review::review_list,
            crate::review::review_add,
            crate::review::review_update,
            crate::review::review_resolve,
            crate::review::review_delete,
            crate::review::review_restore,
            crate::review::review_settle,
            crate::review::review_save,
            // Actuals and the cost report (src/actuals.rs)
            crate::actuals::actuals_pick,
            crate::actuals::actuals_preview,
            crate::actuals::actuals_import,
            crate::actuals::actuals_list,
            crate::actuals::actuals_update,
            crate::actuals::actuals_delete,
            crate::actuals::actuals_remove_batch,
            crate::actuals::actuals_set_etc,
            crate::actuals::actuals_lines,
            // Renumbering accounts to a new pattern (src/renumber.rs)
            crate::renumber::renumber_preview,
            crate::renumber::renumber_accounts,
            // Copies for sharing and sub-budgets (src/sharing.rs)
            crate::sharing::share_copy_save,
            crate::sharing::share_copy_editable,
            crate::sharing::share_copy_original,
            crate::sharing::share_copy_store,
            crate::sharing::sub_budget_new,
            // Working together through a shared folder (src/shared_app.rs)
            crate::shared_app::share_status,
            crate::shared_app::share_setup,
            crate::shared_app::share_pick_folder,
            crate::shared_app::share_start,
            crate::shared_app::share_set_me,
            crate::shared_app::share_place,
            crate::shared_app::share_review,
            crate::shared_app::share_activity,
            crate::shared_app::share_blame,
            crate::shared_app::share_use_value,
            crate::shared_app::share_set_role,
            // The personal setup library (src/library.rs)
            crate::library::library_view,
            crate::library::library_create,
            crate::library::library_choose,
            crate::library::library_add_preview,
            crate::library::library_add,
            crate::library::library_save_preview,
            crate::library::library_save,
            crate::library::library_folder,
            crate::library::library_move,
            // Setup packs: export, import, the Packs list (src/packs.rs)
            crate::packs::packs_list,
            crate::packs::packs_reveal,
            crate::packs::pack_items,
            crate::packs::pack_export,
            crate::packs::pack_import_pick,
            crate::packs::pack_review,
            crate::packs::pack_import,
            crate::packs::pack_import_library,
            // Setup lists as spreadsheets (src/setup_sheets.rs)
            crate::setup_sheets::setup_sheet_export,
            crate::setup_sheets::setup_sheet_pick,
            crate::setup_sheets::setup_sheet_guess,
            crate::setup_sheets::setup_sheet_review,
            // Explain This Number (src/explain.rs)
            crate::explain::explain_number,
            // The overtime helper (src/overtime.rs)
            crate::overtime::ot_calculate,
            crate::overtime::ot_apply,
            crate::overtime::ot_save_pattern,
            crate::overtime::ot_delete_pattern,
            // Updates, release notes and What's New (src/updater.rs)
            crate::updater::update_status,
            crate::updater::update_restart,
            crate::updater::whats_new,
            crate::updater::release_notes,
            // Help > Licenses and Notices (src/help.rs)
            crate::help::licenses,
        ])
        .setup(|app| {
            // Autosave, recovery, backups, recent files, and the budgets that
            // were open as tabs last time (src/safety_app.rs).
            crate::safety_app::init(app.handle());
            // Which file is the personal setup library (src/library.rs).
            crate::library::init(app.handle());
            // Where setup packs are: the user's Packs folder and the
            // bundled ones (src/packs.rs).
            crate::packs::init(app.handle());
            // The start window's pinned budgets (src/start.rs).
            crate::start::init(app.handle());
            // Update checks, What's New and the Help menu's switch
            // (src/updater.rs).
            crate::updater::init(app.handle());
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.set_title(&window_title(&app.state::<AppState>().lock()));
            }
            crate::tabs::after_command(app.handle(), &app.state::<AppState>().docs());
            Ok(())
        })
        .build(context)
        .expect("error while running Method Budget")
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
    fn the_save_dialog_starts_by_the_file_the_budget_came_from() {
        // A converted export: its folder, its name with .mthd.
        let hint = crate::export_import::save_path_for(Path::new("/films/Pilot budget.json"));
        assert_eq!(hint, PathBuf::from("/films/Pilot budget.mthd"));
        assert_eq!(
            save_dialog_start(None, Some(&hint), "Something else"),
            (
                Some(PathBuf::from("/films")),
                "Pilot budget.mthd".to_string()
            )
        );
        // Once saved, its own file decides.
        assert_eq!(
            save_dialog_start(Some(Path::new("/b/Mine.mthd")), Some(&hint), "x"),
            (Some(PathBuf::from("/b")), "Mine.mthd".to_string())
        );
        // Never saved, from nowhere: the title, wherever the dialog likes.
        assert_eq!(
            save_dialog_start(None, None, " My Film: Part 2 "),
            (None, "My Film  Part 2.mthd".to_string())
        );
    }

    #[test]
    fn file_names() {
        assert_eq!(safe_file_stem("My Film: Part 2/3"), "My Film  Part 2 3");
        assert_eq!(safe_file_stem(" .. "), "Untitled");
        assert_eq!(
            with_budget_extension(PathBuf::from("/x/budget")),
            PathBuf::from("/x/budget.mthd")
        );
        assert_eq!(
            with_budget_extension(PathBuf::from("/x/budget.MTHD")),
            PathBuf::from("/x/budget.MTHD")
        );
        assert_eq!(
            with_budget_extension(PathBuf::from("/x/v1.2")),
            PathBuf::from("/x/v1.2.mthd")
        );
    }

    #[test]
    fn closing_asks_about_every_unsaved_budget_at_once() {
        let (one, buttons) = close_question(&["Pilot".to_string()]);
        assert!(one.contains("\"Pilot\" has unsaved changes"), "{one}");
        assert_eq!(buttons, ["Save", "Discard", "Cancel"]);
        let (two, buttons) = close_question(&["Pilot".into(), "Untitled feature".into()]);
        assert!(two.starts_with("2 budgets have unsaved changes"), "{two}");
        assert!(
            two.contains("\u{2022} Pilot\n\u{2022} Untitled feature"),
            "{two}"
        );
        assert_eq!(buttons, ["Save All", "Discard All", "Cancel"]);
        assert!(!two.contains('\u{2014}'), "no em dashes");
    }

    /// Every command method-session's dispatcher runs is a desktop command
    /// of the same name (decision 70), apart from the host's own two, so the
    /// page runs against either.
    #[test]
    fn dispatched_commands_are_desktop_commands() {
        const BUILD_RS: &str = include_str!("../build.rs");
        let host_only = ["open_mthd", "save_mthd"];
        for c in method_session::COMMANDS {
            if host_only.contains(c) {
                assert!(!BUILD_RS.contains(&format!("\"{c}\"")), "{c}");
            } else {
                assert!(BUILD_RS.contains(&format!("    \"{c}\",")), "{c}");
            }
        }
    }

    /// Every command is in `generate_handler!` above and in build.rs's list
    /// (which turns access control on for it), the same set both ways, and
    /// at least one window's capability allows it. A capability allows
    /// nothing else but a few core permissions: no file system, shell,
    /// dialog or updater plugin access from any page, and only the main
    /// window prints and closes itself. (Which window gets which command is
    /// tested beside each command.)
    #[test]
    fn commands_handlers_and_capabilities_agree() {
        use std::collections::BTreeSet;
        const BUILD_RS: &str = include_str!("../build.rs");
        const APP_RS: &str = include_str!("app.rs");
        let between = |text: &'static str, start: &str, end: &str| -> &'static str {
            let from = text.split(start).nth(1).expect("start marker");
            from.split(end).next().expect("end marker")
        };
        let code_lines = |text: &'static str| {
            text.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with("//"))
        };
        let listed: BTreeSet<&str> = code_lines(between(BUILD_RS, "const COMMANDS", "];"))
            .filter_map(|l| l.strip_prefix('"')?.strip_suffix("\","))
            .collect();
        let handled: BTreeSet<&str> = code_lines(between(APP_RS, "generate_handler![", "])"))
            .map(|l| l.trim_end_matches(',').rsplit("::").next().unwrap())
            .collect();
        assert!(listed.len() > 100, "build.rs's list wasn't read");
        assert_eq!(listed, handled, "build.rs and generate_handler! differ");

        let mut allowed = BTreeSet::new();
        for (window, json) in [
            ("main", include_str!("../capabilities/main.json")),
            ("tools", include_str!("../capabilities/tools.json")),
            ("setup", include_str!("../capabilities/setup.json")),
        ] {
            let cap: serde_json::Value = serde_json::from_str(json).unwrap();
            assert_eq!(cap["identifier"], window);
            assert_eq!(cap["windows"], serde_json::json!([window]));
            for p in cap["permissions"].as_array().unwrap() {
                let p = p.as_str().expect("permissions are plain names");
                if let Some(cmd) = p.strip_prefix("allow-") {
                    let cmd = cmd.replace('-', "_");
                    assert!(
                        listed.contains(cmd.as_str()),
                        "{window}: {p} isn't a command"
                    );
                    allowed.insert(cmd);
                    continue;
                }
                let core = ["core:event:allow-listen", "core:event:allow-emit-to"];
                let main_only = ["core:webview:allow-print", "core:window:allow-close"];
                assert!(
                    core.contains(&p) || (window == "main" && main_only.contains(&p)),
                    "{window} is granted {p}"
                );
            }
        }
        let unreachable: Vec<&&str> = listed.iter().filter(|c| !allowed.contains(**c)).collect();
        assert!(unreachable.is_empty(), "no window may call {unreachable:?}");
        assert!(
            !listed.contains("set_base_currency"),
            "renaming the base currency goes through rename_setup_item (decisions.md)"
        );
    }

    #[test]
    fn title_marks_unsaved_changes() {
        let mut s = Session::new();
        assert_eq!(window_title(&s), "Untitled.mthd - Method Budget");
        s.edit("Test edit", |b| edit::set_title(b, "Changed"))
            .unwrap();
        assert_eq!(window_title(&s), "Untitled.mthd (edited) - Method Budget");
    }
}
