//! Tauri glue: the commands the UI invokes, native file dialogs, and the
//! unsaved-changes guard on close and quit.
//!
//! Security notes (see docs/APP.md): file paths only ever come from native
//! dialogs shown here in Rust, never from the web view; every command works on
//! one of the open budgets in [`AppState`] (the one in front, or the one its
//! `doc` argument names: src/docs.rs); navigation away from the bundled UI is
//! blocked.

use std::path::PathBuf;

use fwb_core::RowId;
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

const APP_NAME: &str = "Film Wizard Budgeting";
const FILTER_NAME: &str = "Film Wizard budget";

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
    crate::tools::changed(window, view.doc, view.rev);
    crate::tabs::after_command(window.app_handle(), s.docs());
    view
}

fn apply<R: Runtime, T>(
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
/// one in front).
pub(crate) fn edit_budget<R: Runtime, T>(
    window: &WebviewWindow<R>,
    state: &AppState,
    doc: Option<DocId>,
    f: impl FnOnce(&mut fwb_core::Budget) -> edit::EditResult<T>,
) -> CmdResult {
    apply(window, state, doc, |s| s.edit(f))
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

/// Shows an Open dialog and opens the chosen file in a new tab (or brings its
/// tab to the front when it is open already; an untouched Untitled budget
/// gives its tab up to it). `None` if cancelled.
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
    // Opens it in a tab and lists it in Open Recent (src/safety_app.rs).
    let id = crate::safety_app::open_path(&state, &window.state(), &path)?;
    crate::safety_app::refresh_menu(window.app_handle());
    Ok(Some(finish(&window, &mut state.lock_doc(Some(id))?, None)))
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
                .add_filter(FILTER_NAME, &["fwb"])
                .set_file_name(format!("{}.fwb", safe_file_stem(&suggested)))
                .set_parent(window);
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
    crate::safety_app::save_from(window, state, id, &path).map(Some)
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
    edit_budget(&window, &state, doc, |b| edit::set_title(b, &title))
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
    edit_budget(&window, &state, doc, |b| {
        edit::add_category(b, &code, &name, at)
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
    edit_budget(&window, &state, doc, |b| {
        edit::add_production_total(b, &name, at)
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
    edit_budget(&window, &state, doc, |b| {
        edit::update_topsheet_row(b, index, code.as_deref(), &name)
    })
}

#[tauri::command]
pub fn delete_topsheet_row<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    index: usize,
) -> CmdResult {
    edit_budget(&window, &state, doc, |b| {
        edit::delete_topsheet_row(b, index)
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
    edit_budget(&window, &state, doc, |b| {
        edit::add_account(b, &category_code, &code, &name, after.as_deref())
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
    edit_budget(&window, &state, doc, |b| {
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
    edit_budget(&window, &state, doc, |b| edit::delete_account(b, &code))
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
    let field = edit::Field::parse(&field)?;
    edit_budget(&window, &state, doc, |b| {
        edit::set_rows_field(b, &row_ids, field, &value)
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
    edit_budget(&window, &state, doc, |b| edit::delete_details(b, &row_ids))
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
    edit_budget(&window, &state, doc, |b| {
        edit::set_global(b, &name, &formula)
    })
}

#[tauri::command]
pub fn delete_global<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
) -> CmdResult {
    edit_budget(&window, &state, doc, |b| edit::delete_global(b, &name))
}

#[tauri::command]
pub fn set_fringe<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
    fringe: FringeInput,
) -> CmdResult {
    edit_budget(&window, &state, doc, |b| {
        edit::set_fringe(b, &name, &fringe)
    })
}

#[tauri::command]
pub fn delete_fringe<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
) -> CmdResult {
    edit_budget(&window, &state, doc, |b| edit::delete_fringe(b, &name))
}

#[tauri::command]
pub fn set_group<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
    group: GroupInput,
) -> CmdResult {
    edit_budget(&window, &state, doc, |b| edit::set_group(b, &name, &group))
}

#[tauri::command]
pub fn delete_group<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
) -> CmdResult {
    edit_budget(&window, &state, doc, |b| edit::delete_group(b, &name))
}

#[tauri::command]
pub fn set_unit<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
    unit: UnitInput,
) -> CmdResult {
    edit_budget(&window, &state, doc, |b| edit::set_unit(b, &name, &unit))
}

#[tauri::command]
pub fn delete_unit<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
) -> CmdResult {
    edit_budget(&window, &state, doc, |b| edit::delete_unit(b, &name))
}

#[tauri::command]
pub fn set_currency<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    code: String,
    currency: CurrencyInput,
) -> CmdResult {
    edit_budget(&window, &state, doc, |b| {
        edit::set_currency(b, &code, &currency)
    })
}

#[tauri::command]
pub fn delete_currency<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    code: String,
) -> CmdResult {
    edit_budget(&window, &state, doc, |b| edit::delete_currency(b, &code))
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
    edit_budget(&window, &state, doc, |b| {
        edit::set_charge(b, index, &charge)
    })
}

#[tauri::command]
pub fn delete_charge<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    index: usize,
) -> CmdResult {
    edit_budget(&window, &state, doc, |b| edit::delete_charge(b, index))
}

#[tauri::command]
pub fn move_charge<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    index: usize,
    to: usize,
) -> CmdResult {
    edit_budget(&window, &state, doc, |b| edit::move_charge(b, index, to))
}

#[tauri::command]
pub fn set_credit<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    index: Option<usize>,
    credit: CreditInput,
) -> CmdResult {
    edit_budget(&window, &state, doc, |b| {
        edit::set_credit(b, index, &credit)
    })
}

#[tauri::command]
pub fn delete_credit<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    index: usize,
) -> CmdResult {
    edit_budget(&window, &state, doc, |b| edit::delete_credit(b, index))
}

#[tauri::command]
pub fn move_credit<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    index: usize,
    to: usize,
) -> CmdResult {
    edit_budget(&window, &state, doc, |b| edit::move_credit(b, index, to))
}

#[tauri::command]
pub fn set_set<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
    set: SetInput,
) -> CmdResult {
    edit_budget(&window, &state, doc, |b| edit::set_set(b, &name, &set))
}

#[tauri::command]
pub fn delete_set<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
) -> CmdResult {
    edit_budget(&window, &state, doc, |b| edit::delete_set(b, &name))
}

#[tauri::command]
pub fn set_location<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
    location: LocationInput,
) -> CmdResult {
    edit_budget(&window, &state, doc, |b| {
        edit::set_location(b, &name, &location)
    })
}

#[tauri::command]
pub fn delete_location<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
) -> CmdResult {
    edit_budget(&window, &state, doc, |b| edit::delete_location(b, &name))
}

#[tauri::command]
pub fn set_info<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
    value: InfoInput,
) -> CmdResult {
    edit_budget(&window, &state, doc, |b| edit::set_info(b, &name, &value))
}

#[tauri::command]
pub fn delete_info<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    name: String,
) -> CmdResult {
    edit_budget(&window, &state, doc, |b| edit::delete_info(b, &name))
}

#[tauri::command]
pub fn set_fringe_posting<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    mode: String,
    suffix: Option<String>,
) -> CmdResult {
    edit_budget(&window, &state, doc, |b| {
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
    edit_budget(&window, &state, doc, |b| {
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
    doc: Option<DocId>,
    code: String,
) -> CmdResult {
    edit_budget(&window, &state, doc, |b| edit::set_base_currency(b, &code))
}

#[tauri::command]
pub fn set_totals_currency<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
    code: Option<String>,
) -> CmdResult {
    edit_budget(&window, &state, doc, |b| {
        edit::set_totals_currency(b, code.as_deref())
    })
}

#[tauri::command]
pub fn lock_baseline<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
) -> CmdResult {
    edit_budget(&window, &state, doc, edit::lock_baseline)
}

#[tauri::command]
pub fn clear_baseline<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, AppState>,
    doc: Option<DocId>,
) -> CmdResult {
    edit_budget(&window, &state, doc, edit::clear_baseline)
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
) -> CmdResult {
    edit_budget(&window, &state, doc, |b| crate::batch::set_cells(b, &edits))
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
fn confirm_close<R: Runtime>(app: &AppHandle<R>, then: impl FnOnce() + Send + 'static) {
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
                confirm_close(app, move || handle.exit(0));
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
            // Budgets as tabs (src/tabs.rs)
            crate::tabs::new_tab,
            crate::tabs::switch_tab,
            crate::tabs::move_tab,
            crate::tabs::close_tab,
            crate::tabs::list_tabs,
        ])
        .setup(|app| {
            // Autosave, recovery, backups, recent files, and the budgets that
            // were open as tabs last time (src/safety_app.rs).
            crate::safety_app::init(app.handle());
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.set_title(&window_title(&app.state::<AppState>().lock()));
            }
            crate::tabs::after_command(app.handle(), &app.state::<AppState>().docs());
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
