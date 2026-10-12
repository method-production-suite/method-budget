//! Tauri glue: the commands the UI invokes, native file dialogs, and the
//! unsaved-changes guard on close and quit.
//!
//! Security notes (see docs/APP.md): file paths only ever come from native
//! dialogs shown here in Rust, never from the web view; every command works on
//! one of the open budgets in [`AppState`] (the one in front, or the one its
//! `doc` argument names: src/docs.rs); navigation away from the bundled UI is
//! blocked.

use std::path::PathBuf;

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
use crate::view::{Focus, FullView, View};

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
    let mut view = s.view_with(created);
    view.doc = s.id();
    view.tabs = s.docs().tabs();
    view.panes = Some(s.docs().layout());
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

/// Runs a pure budget edit as one undo step, on budget `doc` (`None`: the
/// one in front). `label` names the step in the History list (src/history.rs).
pub(crate) fn edit_budget<R: Runtime, T>(
    window: &WebviewWindow<R>,
    state: &AppState,
    doc: Option<DocId>,
    label: impl Into<String>,
    f: impl FnOnce(&mut method_core::Budget) -> edit::EditResult<T>,
) -> CmdResult {
    apply(window, state, doc, |s| s.edit(label, f))
}

/// Like [`edit_budget`], with the step's label worked out from the budget as
/// it is before the edit (to name a row by its account and line).
pub(crate) fn edit_budget_as<R: Runtime, T>(
    window: &WebviewWindow<R>,
    state: &AppState,
    doc: Option<DocId>,
    label: impl FnOnce(&method_core::Budget) -> String,
    f: impl FnOnce(&mut method_core::Budget) -> edit::EditResult<T>,
) -> CmdResult {
    apply(window, state, doc, |s| {
        let label = label(s.budget());
        s.edit(label, f)
    })
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
    let mut s = state.lock_doc(doc)?;
    let focus = category.zip(account).map(|(category, account)| Focus {
        category,
        ti: ti.unwrap_or(usize::MAX),
        account,
        ai: ai.unwrap_or(usize::MAX),
    });
    s.set_focus(focus);
    Ok(finish(&window, &mut s, None))
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
    let (current, suggested) = {
        let s = state.lock_doc(Some(id))?;
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
                .set_file_name(format!(
                    "{}.{}",
                    safe_file_stem(&suggested),
                    method_format::EXTENSION
                ))
                .set_parent(window);
            if let Some(dir) = current.as_deref().and_then(|p| p.parent()) {
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

/// A file name stem from a budget title: path separators and other characters
/// file systems reject become spaces.
pub(crate) fn safe_file_stem(title: &str) -> String {
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
    edit_budget(
        &window,
        &state,
        doc,
        format!("Rename budget to {}", title.trim()),
        |b| edit::set_title(b, &title),
    )
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
    edit_budget(
        &window,
        &state,
        doc,
        label2("Add category", &code, &name),
        |b| edit::add_category(b, &code, &name, at),
    )
}

#[tauri::command]
pub fn add_production_total<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
    at: Option<usize>,
) -> CmdResult {
    edit_budget(
        &window,
        &state,
        doc,
        format!("Add production total {}", name.trim()),
        |b| edit::add_production_total(b, &name, at),
    )
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
    edit_budget_as(
        &window,
        &state,
        doc,
        |b| topsheet_label("Edit", b, index),
        |b| edit::update_topsheet_row(b, index, code.as_deref(), &name),
    )
}

#[tauri::command]
pub fn delete_topsheet_row<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    index: usize,
) -> CmdResult {
    edit_budget_as(
        &window,
        &state,
        doc,
        |b| topsheet_label("Delete", b, index),
        |b| edit::delete_topsheet_row(b, index),
    )
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
    edit_budget(
        &window,
        &state,
        doc,
        label2("Add account", &code, &name),
        |b| edit::add_account(b, &category_code, &code, &name, after.as_deref()),
    )
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
    let label = if code.trim() == new_code.trim() {
        format!("Rename account {}", code.trim())
    } else {
        format!("Renumber account {} to {}", code.trim(), new_code.trim())
    };
    edit_budget(&window, &state, doc, label, |b| {
        edit::update_account(b, &code, &new_code, &name)
    })
}

#[tauri::command]
pub fn delete_account<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    code: String,
) -> CmdResult {
    edit_budget(
        &window,
        &state,
        doc,
        format!("Delete account {}", code.trim()),
        |b| edit::delete_account(b, &code),
    )
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
        s.set_detail_field(row_id, &field, &value)
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
    let parsed = edit::Field::parse(&field)?;
    let label = |b: &method_core::Budget| crate::history::rows_field_label(b, &row_ids, &field);
    edit_budget_as(&window, &state, doc, label, |b| {
        edit::set_rows_field(b, &row_ids, parsed, &value)
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
    edit_budget_as(
        &window,
        &state,
        doc,
        |b| crate::history::delete_rows_label(b, &row_ids),
        |b| edit::delete_details(b, &row_ids),
    )
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
    edit_budget_as(
        &window,
        &state,
        doc,
        |b| {
            let verb = if b.globals.contains_key(name.trim()) {
                "Set"
            } else {
                "Add"
            };
            format!("{verb} global {} = {}", name.trim(), formula.trim())
        },
        |b| edit::set_global(b, &name, &formula),
    )
}

#[tauri::command]
pub fn delete_global<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
) -> CmdResult {
    edit_budget(
        &window,
        &state,
        doc,
        format!("Delete global {}", name.trim()),
        |b| edit::delete_global(b, &name),
    )
}

#[tauri::command]
pub fn set_fringe<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
    fringe: FringeInput,
) -> CmdResult {
    edit_budget_as(
        &window,
        &state,
        doc,
        |b| upsert_label("fringe", b.fringes.contains_key(name.trim()), &name),
        |b| edit::set_fringe(b, &name, &fringe),
    )
}

#[tauri::command]
pub fn delete_fringe<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
) -> CmdResult {
    edit_budget(
        &window,
        &state,
        doc,
        format!("Delete fringe {}", name.trim()),
        |b| edit::delete_fringe(b, &name),
    )
}

#[tauri::command]
pub fn set_group<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
    group: GroupInput,
) -> CmdResult {
    edit_budget_as(
        &window,
        &state,
        doc,
        |b| upsert_label("group", b.groups.contains_key(name.trim()), &name),
        |b| edit::set_group(b, &name, &group),
    )
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
    edit_budget(&window, &state, doc, "Change a group rule", |b| {
        edit::set_group_rule(b, &rule, &value)
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
    edit_budget(&window, &state, doc, "Change rounding", |b| {
        edit::set_arithmetic(b, &value)
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
    edit_budget(&window, &state, doc, "Change the cutoff factor", |b| {
        edit::set_cutoff_factor(b, &value)
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
    edit_budget(&window, &state, doc, "Change the fringe row", |b| {
        edit::set_fringe_row(b, &row)
    })
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
    edit_budget(
        &window,
        &state,
        doc,
        "Change fringes after the last production total",
        |b| edit::set_trailing_level_fringes(b, &value),
    )
}

#[tauri::command]
pub fn delete_group<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
) -> CmdResult {
    edit_budget(
        &window,
        &state,
        doc,
        format!("Delete group {}", name.trim()),
        |b| edit::delete_group(b, &name),
    )
}

#[tauri::command]
pub fn set_unit<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
    unit: UnitInput,
) -> CmdResult {
    edit_budget_as(
        &window,
        &state,
        doc,
        |b| upsert_label("unit", b.units.contains_key(name.trim()), &name),
        |b| edit::set_unit(b, &name, &unit),
    )
}

#[tauri::command]
pub fn delete_unit<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
    clear_lines: Option<bool>,
) -> CmdResult {
    // `clearLines`: also blank the unit columns that name it.
    edit_budget(
        &window,
        &state,
        doc,
        format!("Delete unit {}", name.trim()),
        |b| edit::delete_unit_with(b, &name, clear_lines.unwrap_or(false)),
    )
}

#[tauri::command]
pub fn set_currency<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    code: String,
    currency: CurrencyInput,
) -> CmdResult {
    edit_budget_as(
        &window,
        &state,
        doc,
        |b| upsert_label("currency", b.currencies.contains_key(code.trim()), &code),
        |b| edit::set_currency(b, &code, &currency),
    )
}

#[tauri::command]
pub fn delete_currency<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    code: String,
    clear_lines: Option<bool>,
) -> CmdResult {
    // `clearLines`: lines in it go back to the base currency first.
    edit_budget(
        &window,
        &state,
        doc,
        format!("Delete currency {}", code.trim()),
        |b| edit::delete_currency_with(b, &code, clear_lines.unwrap_or(false)),
    )
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
    edit_budget(
        &window,
        &state,
        doc,
        if index.is_some() {
            "Edit a charge"
        } else {
            "Add a charge"
        },
        |b| edit::set_charge(b, index, &charge),
    )
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
    let (budget, outcome) = s.totals();
    let c = edit::build_charge(budget, index, charge)?;
    let report = outcome
        .report
        .as_ref()
        .ok_or("the totals can't be worked out until the budget's problems are fixed")?;
    let figured = outcome.sanitized.as_ref().unwrap_or(budget);
    let t = method_core::charge_preview(figured, report, &c).map_err(|e| e.to_string())?;
    Ok(crate::report_dto::ChargeDto::from(&t))
}

#[tauri::command]
pub fn delete_charge<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    index: usize,
) -> CmdResult {
    edit_budget(&window, &state, doc, "Delete a charge", |b| {
        edit::delete_charge(b, index)
    })
}

#[tauri::command]
pub fn move_charge<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    index: usize,
    to: usize,
) -> CmdResult {
    edit_budget(&window, &state, doc, "Move a charge", |b| {
        edit::move_charge(b, index, to)
    })
}

#[tauri::command]
pub fn set_credit<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    index: Option<usize>,
    credit: CreditInput,
) -> CmdResult {
    edit_budget(
        &window,
        &state,
        doc,
        if index.is_some() {
            "Edit a credit"
        } else {
            "Add a credit"
        },
        |b| edit::set_credit(b, index, &credit),
    )
}

#[tauri::command]
pub fn delete_credit<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    index: usize,
) -> CmdResult {
    edit_budget(&window, &state, doc, "Delete a credit", |b| {
        edit::delete_credit(b, index)
    })
}

#[tauri::command]
pub fn move_credit<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    index: usize,
    to: usize,
) -> CmdResult {
    edit_budget(&window, &state, doc, "Move a credit", |b| {
        edit::move_credit(b, index, to)
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
    edit_budget_as(
        &window,
        &state,
        doc,
        |b| upsert_label("set", b.sets.contains_key(name.trim()), &name),
        |b| edit::set_set(b, &name, &set),
    )
}

#[tauri::command]
pub fn delete_set<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
) -> CmdResult {
    edit_budget(
        &window,
        &state,
        doc,
        format!("Delete set {}", name.trim()),
        |b| edit::delete_set(b, &name),
    )
}

#[tauri::command]
pub fn set_location<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
    location: LocationInput,
) -> CmdResult {
    edit_budget_as(
        &window,
        &state,
        doc,
        |b| upsert_label("location", b.locations.contains_key(name.trim()), &name),
        |b| edit::set_location(b, &name, &location),
    )
}

#[tauri::command]
pub fn delete_location<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
) -> CmdResult {
    edit_budget(
        &window,
        &state,
        doc,
        format!("Delete location {}", name.trim()),
        |b| edit::delete_location(b, &name),
    )
}

#[tauri::command]
pub fn set_info<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
    value: InfoInput,
) -> CmdResult {
    edit_budget(
        &window,
        &state,
        doc,
        format!("Edit budget info {}", name.trim()),
        |b| edit::set_info(b, &name, &value),
    )
}

#[tauri::command]
pub fn delete_info<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
) -> CmdResult {
    edit_budget(
        &window,
        &state,
        doc,
        format!("Delete budget info {}", name.trim()),
        |b| edit::delete_info(b, &name),
    )
}

#[tauri::command]
pub fn set_fringe_posting<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    mode: String,
    suffix: Option<String>,
) -> CmdResult {
    edit_budget(&window, &state, doc, "Change fringe posting", |b| {
        edit::set_fringe_posting(b, &mode, suffix.as_deref())
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
    edit_budget(&window, &state, doc, "Change account numbering", |b| {
        edit::set_account_coding(
            b,
            [category_width, account_width, sub_width, set_width],
            separator.as_deref(),
        )
    })
}

#[tauri::command]
pub fn set_totals_currency<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    code: Option<String>,
) -> CmdResult {
    edit_budget(&window, &state, doc, "Change the totals currency", |b| {
        edit::set_totals_currency(b, code.as_deref())
    })
}

#[tauri::command]
pub fn lock_baseline<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
) -> CmdResult {
    edit_budget(
        &window,
        &state,
        doc,
        "Lock the baseline",
        edit::lock_baseline,
    )
}

#[tauri::command]
pub fn set_baseline_frozen<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    frozen: bool,
) -> CmdResult {
    let label = if frozen {
        "Freeze the original totals"
    } else {
        "Unfreeze the original totals"
    };
    edit_budget(&window, &state, doc, label, |b| {
        edit::set_baseline_frozen(b, frozen)
    })
}

#[tauri::command]
pub fn clear_baseline<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
) -> CmdResult {
    edit_budget(
        &window,
        &state,
        doc,
        "Clear the baseline",
        edit::clear_baseline,
    )
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
    // `action`: what the page was doing (paste, fill, cut, clear), for the
    // step's name in the History list; only known words are used.
    let label = |b: &method_core::Budget| {
        let cells: Vec<(RowId, &str, &str)> = edits
            .iter()
            .map(|e| (e.row_id, e.field.as_str(), e.value.as_str()))
            .collect();
        crate::history::cells_label(b, &cells, action.as_deref())
    };
    edit_budget_as(&window, &state, doc, label, |b| {
        crate::batch::set_cells(b, &edits)
    })
}

/// "Add account 21-01 Camera": a verb, a code and a name.
fn label2(verb: &str, code: &str, name: &str) -> String {
    format!("{verb} {} {}", code.trim(), name.trim())
        .trim_end()
        .to_string()
}

/// "Add fringe FICA" or "Edit fringe FICA".
fn upsert_label(kind: &str, exists: bool, name: &str) -> String {
    let verb = if exists { "Edit" } else { "Add" };
    format!("{verb} {kind} {}", name.trim())
}

/// "Edit category 10", "Delete production total Total above the line".
fn topsheet_label(verb: &str, b: &method_core::Budget, index: usize) -> String {
    match b.topsheet.get(index) {
        Some(method_core::TopsheetRow::Category(c)) => format!("{verb} category {}", c.code),
        Some(method_core::TopsheetRow::ProductionTotal(p)) => {
            format!("{verb} production total {}", p.name)
        }
        None => format!("{verb} a topsheet row"),
    }
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
        // In-app updates, driven from Rust only (src/updater.rs): no window
        // is granted the plugin's own commands.
        .plugin(tauri_plugin_updater::Builder::new().build())
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
            {
                // Open Recent's Clear Menu and the Autosave check are answered
                // in src/safety_app.rs, Check for Updates and its automatic
                // switch in src/updater.rs.
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
            crate::start::start_pin,
            crate::start::start_forget,
            crate::start::start_reveal,
            crate::start::new_budget_pick_template,
            crate::start::new_budget_create,
            crate::start::new_budget_folder,
            crate::start::new_budget_pick_folder,
            crate::start::new_budget_charts,
            // A whole budget from another program's export, with the
            // reconciliation (src/export_import.rs)
            crate::export_import::export_import_pick,
            crate::export_import::export_import_preview,
            crate::export_import::export_import,
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
