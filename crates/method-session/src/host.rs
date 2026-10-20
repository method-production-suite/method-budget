//! The one entry point for a host that isn't the desktop app (a partner's
//! web app, the website's demo): [`Host::dispatch`] takes a command's name
//! and its arguments as JSON, exactly as the page's `invoke` sends them to
//! the desktop, and gives back what the desktop's command returns. The
//! names and shapes are the desktop's (docs/APP.md and
//! docs/research/web-session.md), so the existing page runs against it
//! through a small `invoke` shim.
//!
//! The contract is versioned: [`COMMANDS_VERSION`] goes up whenever a
//! command in [`COMMANDS`] is removed or renamed, or its arguments or reply
//! change in a way an existing caller would notice. Adding a command or an
//! optional argument doesn't change it.
//!
//! Budgets come and go as `.mthd` text ([`Host::open_mthd`],
//! [`Host::save_mthd`]): where they are kept is the host's business.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use method_core::RowId;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;

use crate::commands as cmd;
use crate::display::{DisplayPrefsInput, GlobalInfoInput};
use crate::docs::{AppState, DocGuard, DocId, Docs};
use crate::edit::{
    ChargeInput, CreditInput, CurrencyInput, EditResult, FringeInput, FringeRowInput, GroupInput,
    InfoInput, LocationInput, SetInput, UnitInput,
};
use crate::session::Session;
use crate::view::View;

mod actuals;
mod apply;
mod b64;
mod clipboard;
mod coa_import;
mod compare;
mod diffmerge;
mod explain;
mod export;
mod export_import;
mod find;
mod import_setup;
mod moverows;
mod notes;
mod open_any;
mod overtime;
mod packs;
mod presets;
mod quickadd;
mod rename;
mod renumber;
mod reports;
mod review;
mod rowstyle;
mod schedule_days;
mod setup_lists;
mod setup_sheets;
mod sharing;
mod sheet_import;
mod tabs;
mod templates;
mod versions;

#[cfg(test)]
pub(crate) mod test_files;

/// A file's bytes as the contract's JSON carries them (base64).
pub use b64::{decode as b64_decode, encode as b64_encode};

/// The command contract's version (see the module's notes).
pub const COMMANDS_VERSION: u32 = 1;

/// Every command [`Host::dispatch`] runs, in the order docs/APP.md lists
/// them. Each takes `doc` (the budget it acts on; missing or `null`: the
/// one in front) besides the arguments named in
/// docs/research/web-session.md.
pub const COMMANDS: &[&str] = &[
    // Views
    "get_view",
    "get_full_view",
    "focus_account",
    "selection_totals",
    "preview_charge",
    "incentives",
    // History
    "undo",
    "redo",
    "history_list",
    "history_jump",
    "history_save_version",
    // Structure
    "set_title",
    "add_category",
    "add_production_total",
    "update_topsheet_row",
    "delete_topsheet_row",
    "add_account",
    "update_account",
    "delete_account",
    // Detail rows
    "set_detail_field",
    "set_rows_field",
    "add_detail",
    "delete_details",
    "set_cells",
    // Setup tables
    "set_global",
    "delete_global",
    "set_global_info",
    "set_display_prefs",
    "set_fringe",
    "delete_fringe",
    "set_group",
    "set_group_rule",
    "delete_group",
    "set_unit",
    "delete_unit",
    "set_currency",
    "delete_currency",
    // Settings
    "set_arithmetic",
    "set_round_account_totals",
    "set_cutoff_factor",
    "set_fringe_row",
    "set_trailing_level_fringes",
    "set_fringe_posting",
    "set_account_coding",
    "set_totals_currency",
    // Charges, credits, sets, locations, budget info, the baseline
    "set_charge",
    "delete_charge",
    "move_charge",
    "set_credit",
    "delete_credit",
    "move_credit",
    "set_set",
    "delete_set",
    "set_location",
    "delete_location",
    "set_info",
    "delete_info",
    "lock_baseline",
    "set_baseline_frozen",
    "clear_baseline",
    // Phase 1b (docs/research/web-session.md), each module's commands
    // dispatched in src/host/<module>.rs.
    // Versions
    "save_version",
    "restore_version",
    "rename_version",
    "delete_version",
    // Templates
    "template_new",
    "template_save",
    // Import setup
    "import_setup_pick",
    "import_setup",
    // Sharing
    "share_copy_save",
    "share_copy_editable",
    "share_copy_original",
    "share_copy_store",
    "sub_budget_new",
    // Setup packs (into and out of a budget; the library is the desktop's)
    "pack_items",
    "pack_export",
    "pack_import_pick",
    "pack_import",
    // Quick entry
    "set_cell_creating",
    "set_new_line_defaults",
    // Find and replace
    "find",
    "replace",
    // Moving rows
    "move_rows",
    "move_accounts",
    "move_topsheet_rows",
    // Renumbering
    "renumber_preview",
    "renumber_accounts",
    // Setup lists
    "setup_usage",
    "set_setup_order",
    "move_setup_item",
    "set_short_code",
    // Explain This Number
    "explain_number",
    // Overtime
    "ot_calculate",
    "ot_apply",
    "ot_save_pattern",
    "ot_delete_pattern",
    // Notes
    "add_note",
    "update_note",
    "delete_note",
    "set_note_done",
    "notes_list",
    // Row style
    "set_row_style",
    "set_group_color",
    "update_group",
    // Clipboard (the app's own, for whole rows)
    "clip_copy",
    "clip_cut",
    "clip_preview",
    "clip_paste",
    "clip_forget",
    // Presets
    "save_tag_preset",
    "tag_preset_from_line",
    "apply_tag_preset",
    "save_line_preset",
    "line_preset_preview",
    "insert_line_preset",
    "rename_tool_preset",
    "delete_tool_preset",
    "move_tool_preset",
    // Apply panel
    "apply_summary",
    "apply_to_rows",
    "assign_to_rows",
    "clear_from_rows",
    "replace_tags_on_rows",
    "make_fringe_range",
    "clear_fringe_range",
    // Rename
    "rename_setup_item",
    // Tabs
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
    // Compare
    "compare_budgets",
    "compare_detail",
    "compare_pick_files",
    // What Changed, Merge Changes From
    "what_changed",
    "merge_preview",
    "merge_apply",
    "merge_pick_file",
    // Export
    "export_budget",
    // Report presets
    "save_report_preset",
    "delete_report_preset",
    "bid_sheet",
    // Import spreadsheet
    "sheet_import_pick",
    "sheet_import_preview",
    "sheet_import",
    // Import chart of accounts
    "coa_import_pick",
    "coa_import_preview",
    "coa_import",
    "chart_templates",
    "chart_template_new",
    // Import budget export
    "export_import_pick",
    "export_import_preview",
    "export_import",
    // Open
    "open_budget",
    // Setup lists as spreadsheets
    "setup_sheet_export",
    "setup_sheet_pick",
    "setup_sheet_guess",
    "setup_sheet_review",
    // Actuals
    "actuals_pick",
    "actuals_preview",
    "actuals_import",
    "actuals_list",
    "actuals_update",
    "actuals_delete",
    "actuals_remove_batch",
    "actuals_set_etc",
    "actuals_lines",
    // Schedule days into globals
    "schedule_days_pick",
    "schedule_days_review",
    "schedule_days_apply",
    // Review files: comments beside a budget, merged back as notes
    "review_start",
    "review_open",
    "review_close",
    "review_list",
    "review_add",
    "review_update",
    "review_resolve",
    "review_delete",
    "review_restore",
    "review_settle",
    "review_save",
    // The host's own (no desktop command of these names)
    "open_mthd",
    "save_mthd",
    // What the host is (the desktop has it too)
    "host_info",
];

/// Why a command didn't run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// No command of that name (or not one this host runs yet).
    UnknownCommand(String),
    /// The arguments didn't have the command's shape.
    BadArguments { command: String, message: String },
    /// The command was refused, with the message the page shows (a closed
    /// budget, an edit the budget can't take); nothing changed.
    Refused(String),
}

impl Error {
    /// The text the page shows: what the desktop's command would reject
    /// with.
    pub fn message(&self) -> String {
        match self {
            Error::UnknownCommand(c) => format!("Method Budget has no command {c}."),
            Error::BadArguments { command, message } => {
                format!("The {command} command can't read its arguments: {message}")
            }
            Error::Refused(m) => m.clone(),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}

impl std::error::Error for Error {}

impl From<String> for Error {
    fn from(m: String) -> Error {
        Error::Refused(m)
    }
}

/// Every open budget, and the commands on them.
#[derive(Debug, Default)]
pub struct Host {
    state: AppState,
    /// What a module keeps between calls (a picked file by its token, as
    /// the desktop keeps it in Tauri's state): one of each type, made on
    /// first use ([`Call::kept`]).
    kept: Mutex<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>,
}

/// A file the person picked, as a host hands it in where the desktop shows
/// an Open dialog: the command of the same name takes it as `file`. The
/// name is only shown (no path); the bytes are one base64 string in the
/// JSON ([`b64_encode`]).
#[derive(Clone, Debug, Default, Deserialize)]
pub struct FileIn {
    pub name: String,
    #[serde(with = "b64")]
    pub bytes: Vec<u8>,
}

/// A file for the person to keep, where the desktop shows a Save dialog
/// and writes it: the host's command of the same name gives it back in
/// [`Downloads`], and the host offers it as a download under `name`. The
/// bytes are one base64 string in the JSON.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct FileOut {
    pub name: String,
    #[serde(with = "b64")]
    pub bytes: Vec<u8>,
}

/// The reply of a command that saves files on the desktop: the desktop's
/// reply as after a save (a file's `path` is its name), and beside it
/// `downloads`, the files themselves. A host offers the downloads and can
/// hand the rest to the page as the desktop's reply.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Downloads<T> {
    #[serde(flatten)]
    pub reply: T,
    pub downloads: Vec<FileOut>,
}

/// The desktop's reply after saving one file (Save as Template, Save a
/// Copy for Sharing): its name and its path, here its name too.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct SavedFile {
    pub name: String,
    pub path: String,
}

impl Downloads<SavedFile> {
    /// One file, saved as the desktop replies.
    pub fn one(file: FileOut) -> Downloads<SavedFile> {
        Downloads {
            reply: SavedFile {
                name: file.name.clone(),
                path: file.name.clone(),
            },
            downloads: vec![file],
        }
    }
}

/// What `host_info` gives back: which host the page runs in, so it can
/// hide what only the desktop does (File > Open, printing, pop-out
/// windows).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct HostInfo {
    /// `"desktop"` or `"web"`.
    pub kind: String,
    /// [`COMMANDS_VERSION`].
    pub commands_version: u32,
    /// Every budget is shown read-only ([`Host::set_read_only`]).
    pub read_only: bool,
}

impl HostInfo {
    pub fn new(kind: &str, read_only: bool) -> HostInfo {
        HostInfo {
            kind: kind.to_string(),
            commands_version: COMMANDS_VERSION,
            read_only,
        }
    }
}

/// What `save_mthd` gives back.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Saved {
    /// The budget as `.mthd` text (docs/FORMAT.md).
    pub text: String,
    pub view: View,
}

impl Host {
    /// One untitled starter budget.
    pub fn new() -> Host {
        Host::default()
    }

    /// The budgets in `docs`.
    pub fn with(docs: Docs) -> Host {
        Host {
            state: AppState::with(docs),
            kept: Mutex::default(),
        }
    }

    /// Every open budget, for the host's own use.
    pub fn state(&self) -> &AppState {
        &self.state
    }

    /// Shows every budget read-only (a partner's viewer), those open now
    /// and any opened later, or lets them be changed again. While on, every
    /// edit, undo, redo and history jump is refused with a plain message
    /// and nothing changes.
    pub fn set_read_only(&self, on: bool) {
        self.state.docs().set_read_only(on);
    }

    pub fn read_only(&self) -> bool {
        self.state.docs().read_only()
    }

    /// What `host_info` gives back: a web host.
    pub fn info(&self) -> HostInfo {
        HostInfo::new("web", self.read_only())
    }

    /// The chart templates this host supplies itself (a build without the
    /// bundled ones, as in a browser): what `chart_templates` lists after
    /// any bundled ones. A chart's text is handed in with
    /// [`Host::add_chart`] once it is picked, before `chart_template_new`.
    pub fn set_charts(&self, list: Vec<crate::coa_import::ChartTemplate>) {
        self.kept::<crate::coa_import::HostCharts>().set_list(list);
    }

    /// Chart `id`'s template text (a `.mthd` file), for
    /// `chart_template_new`. Refused when it isn't a budget file.
    pub fn add_chart(&self, id: &str, text: &str) -> Result<(), Error> {
        self.kept::<crate::coa_import::HostCharts>()
            .add(id, text)
            .map_err(Error::Refused)
    }

    /// The host's one `T`, made on first use: what a module keeps between
    /// calls (a picked file by its token).
    fn kept<T: Default + Send + Sync + 'static>(&self) -> Arc<T> {
        let mut kept = self.kept.lock().unwrap_or_else(|e| e.into_inner());
        let any = kept
            .entry(TypeId::of::<T>())
            .or_insert_with(|| Arc::new(T::default()))
            .clone();
        any.downcast::<T>()
            .unwrap_or_else(|_| unreachable!("kept by its own type"))
    }

    /// Opens `.mthd` text as a budget named `name` (a file name, shown in
    /// the title and the tab), in a new tab in front; an untouched
    /// untitled budget gives its tab up to it. Its view.
    pub fn open_mthd(&self, name: &str, text: &str) -> Result<View, Error> {
        let budget = method_format::from_str(text)
            .map_err(|e| Error::Refused(format!("{name} isn't a readable budget: {e}")))?;
        let mut s = Session::with_budget(budget, Some(Path::new(name).to_path_buf()));
        s.set_disk_stamp(Some(stamp(text)));
        let id = {
            let mut docs = self.state.docs();
            let id = match docs.lone_untouched() {
                Some(old) => docs.replace(old, s)?.0,
                None => docs.add(s),
            };
            docs.switch(id)?;
            id
        };
        Ok(self.state.lock_doc(Some(id))?.finish(None))
    }

    /// Budget `doc` (`None`: the one in front) as `.mthd` text, now saved
    /// under `name` (`None`: the name it has): no unsaved changes after.
    pub fn save_mthd(&self, doc: Option<DocId>, name: Option<&str>) -> Result<Saved, Error> {
        let mut s = self.state.lock_doc(doc)?;
        let text = method_format::to_string(s.budget());
        let name = match name {
            Some(n) => n.to_string(),
            None => s.display_name(),
        };
        s.mark_saved(Path::new(&name), stamp(&text));
        let view = s.finish(None);
        Ok(Saved { text, view })
    }

    /// Runs command `command` with `args` (a JSON object, as the page's
    /// `invoke` sends it) and gives back its reply as JSON.
    pub fn dispatch(&self, command: &str, args: Value) -> Result<Value, Error> {
        let doc = doc_of(command, &args)?;
        let d = Call {
            host: self,
            command,
            args,
            doc,
        };
        match command {
            "get_view" => d.view(|_| Ok(())),
            "get_full_view" => json(self.state.lock_doc(doc)?.full_view()),
            "focus_account" => d.view_with(|s, a: FocusArgs| {
                cmd::focus_account(s, a.category, a.ti, a.account, a.ai);
                Ok(())
            }),
            "selection_totals" => {
                let a: RowIds = d.args()?;
                json(self.state.lock_doc(doc)?.selection(&a.row_ids))
            }
            "preview_charge" => {
                let a: ChargeArgs = d.args()?;
                json(cmd::preview_charge(
                    &mut *self.state.lock_doc(doc)?,
                    a.index,
                    &a.charge,
                )?)
            }
            "incentives" => json(cmd::incentives(&mut *self.state.lock_doc(doc)?)?),
            "undo" => d.view(|s| {
                s.undo();
                Ok(())
            }),
            "redo" => d.view(|s| {
                s.redo();
                Ok(())
            }),
            "history_list" => json(self.state.lock_doc(doc)?.history()),
            "history_jump" => d.view_with(|s, a: To| s.jump_to(a.to)),
            "history_save_version" => d.view_with(|s, a: StepVersion| {
                crate::history::history_save_version(
                    s,
                    a.step,
                    &a.name,
                    a.note.as_deref(),
                    &a.created,
                )
            }),
            "set_title" => d.view_with(|s, a: Title| cmd::set_title(s, &a.title)),
            "add_category" => {
                d.view_with(|s, a: AddCategory| cmd::add_category(s, &a.code, &a.name, a.at))
            }
            "add_production_total" => {
                d.view_with(|s, a: NameAt| cmd::add_production_total(s, &a.name, a.at))
            }
            "update_topsheet_row" => d.view_with(|s, a: UpdateTopsheetRow| {
                cmd::update_topsheet_row(s, a.index, a.code.as_deref(), &a.name)
            }),
            "delete_topsheet_row" => {
                d.view_with(|s, a: Index| cmd::delete_topsheet_row(s, a.index))
            }
            "add_account" => d.view_with(|s, a: AddAccount| {
                cmd::add_account(s, &a.category_code, &a.code, &a.name, a.after.as_deref())
            }),
            "update_account" => d.view_with(|s, a: UpdateAccount| {
                cmd::update_account(s, &a.code, &a.new_code, &a.name)
            }),
            "delete_account" => d.view_with(|s, a: Code| cmd::delete_account(s, &a.code)),
            "set_detail_field" => d.view_with(|s, a: DetailField| {
                cmd::set_detail_field(s, a.row_id, &a.field, &a.value)
            }),
            "set_rows_field" => d.view_with(|s, a: RowsField| {
                cmd::set_rows_field(s, &a.row_ids, &a.field, &a.value)
            }),
            "add_detail" => {
                d.created(|s, a: AddDetail| cmd::add_detail(s, &a.account_code, a.after, &a.kind))
            }
            "delete_details" => d.view_with(|s, a: RowIds| cmd::delete_details(s, &a.row_ids)),
            "set_cells" => {
                d.view_with(|s, a: Cells| cmd::set_cells(s, &a.edits, a.action.as_deref()))
            }
            "set_global" => d.view_with(|s, a: Global| cmd::set_global(s, &a.name, &a.formula)),
            "delete_global" => d.view_with(|s, a: Name| cmd::delete_global(s, &a.name)),
            "set_global_info" => {
                d.view_with(|s, a: GlobalInfo| cmd::set_global_info(s, &a.name, &a.info))
            }
            "set_display_prefs" => {
                d.view_with(|s, a: DisplayPrefs| cmd::set_display_prefs(s, &a.prefs))
            }
            "set_fringe" => d.view_with(|s, a: Fringe| cmd::set_fringe(s, &a.name, &a.fringe)),
            "delete_fringe" => d.view_with(|s, a: Name| cmd::delete_fringe(s, &a.name)),
            "set_group" => d.view_with(|s, a: Group| cmd::set_group(s, &a.name, &a.group)),
            "set_group_rule" => {
                d.view_with(|s, a: GroupRule| cmd::set_group_rule(s, &a.rule, &a.value))
            }
            "delete_group" => d.view_with(|s, a: Name| cmd::delete_group(s, &a.name)),
            "set_unit" => d.view_with(|s, a: Unit| cmd::set_unit(s, &a.name, &a.unit)),
            "delete_unit" => d.view_with(|s, a: DeleteUnit| {
                cmd::delete_unit(s, &a.name, a.clear_lines.unwrap_or(false))
            }),
            "set_currency" => {
                d.view_with(|s, a: Currency| cmd::set_currency(s, &a.code, &a.currency))
            }
            "delete_currency" => d.view_with(|s, a: DeleteCurrency| {
                cmd::delete_currency(s, &a.code, a.clear_lines.unwrap_or(false))
            }),
            "set_arithmetic" => d.view_with(|s, a: Text| cmd::set_arithmetic(s, &a.value)),
            "set_round_account_totals" => {
                d.view_with(|s, a: Flag| cmd::set_round_account_totals(s, a.value))
            }
            "set_cutoff_factor" => d.view_with(|s, a: Text| cmd::set_cutoff_factor(s, &a.value)),
            "set_fringe_row" => d.view_with(|s, a: FringeRow| cmd::set_fringe_row(s, &a.row)),
            "set_trailing_level_fringes" => {
                d.view_with(|s, a: Text| cmd::set_trailing_level_fringes(s, &a.value))
            }
            "set_fringe_posting" => d.view_with(|s, a: FringePosting| {
                cmd::set_fringe_posting(s, &a.mode, a.suffix.as_deref())
            }),
            "set_account_coding" => d.view_with(|s, a: AccountCoding| {
                cmd::set_account_coding(
                    s,
                    [a.category_width, a.account_width, a.sub_width, a.set_width],
                    a.separator.as_deref(),
                )
            }),
            "set_totals_currency" => {
                d.view_with(|s, a: MaybeCode| cmd::set_totals_currency(s, a.code.as_deref()))
            }
            "set_charge" => d.view_with(|s, a: ChargeArgs| cmd::set_charge(s, a.index, &a.charge)),
            "delete_charge" => d.view_with(|s, a: Index| cmd::delete_charge(s, a.index)),
            "move_charge" => d.view_with(|s, a: Move| cmd::move_charge(s, a.index, a.to)),
            "set_credit" => d.view_with(|s, a: Credit| cmd::set_credit(s, a.index, &a.credit)),
            "delete_credit" => d.view_with(|s, a: Index| cmd::delete_credit(s, a.index)),
            "move_credit" => d.view_with(|s, a: Move| cmd::move_credit(s, a.index, a.to)),
            "set_set" => d.view_with(|s, a: Set| cmd::set_set(s, &a.name, &a.set)),
            "delete_set" => d.view_with(|s, a: Name| cmd::delete_set(s, &a.name)),
            "set_location" => {
                d.view_with(|s, a: Location| cmd::set_location(s, &a.name, &a.location))
            }
            "delete_location" => d.view_with(|s, a: Name| cmd::delete_location(s, &a.name)),
            "set_info" => d.view_with(|s, a: Info| cmd::set_info(s, &a.name, &a.value)),
            "delete_info" => d.view_with(|s, a: Name| cmd::delete_info(s, &a.name)),
            "lock_baseline" => d.view(cmd::lock_baseline),
            "set_baseline_frozen" => {
                d.view_with(|s, a: Frozen| cmd::set_baseline_frozen(s, a.frozen))
            }
            "clear_baseline" => d.view(cmd::clear_baseline),
            "open_mthd" => {
                let a: OpenMthd = d.args()?;
                json(self.open_mthd(&a.name, &a.text)?)
            }
            "save_mthd" => {
                let a: SaveMthd = d.args()?;
                json(self.save_mthd(doc, a.name.as_deref())?)
            }
            "host_info" => json(self.info()),
            other => MODULES
                .iter()
                .find_map(|f| f(&d))
                .unwrap_or_else(|| Err(Error::UnknownCommand(other.to_string()))),
        }
    }
}

/// A module's own commands (src/host/*.rs): `None` when the call names
/// none of them.
type Dispatch = fn(&Call<'_>) -> Option<Result<Value, Error>>;

/// The modules moved in phase 1b, each dispatching its own commands.
const MODULES: &[Dispatch] = &[
    versions::dispatch,
    templates::dispatch,
    import_setup::dispatch,
    sharing::dispatch,
    quickadd::dispatch,
    find::dispatch,
    moverows::dispatch,
    renumber::dispatch,
    setup_lists::dispatch,
    explain::dispatch,
    overtime::dispatch,
    packs::dispatch,
    notes::dispatch,
    rowstyle::dispatch,
    clipboard::dispatch,
    presets::dispatch,
    apply::dispatch,
    rename::dispatch,
    tabs::dispatch,
    compare::dispatch,
    diffmerge::dispatch,
    export::dispatch,
    reports::dispatch,
    sheet_import::dispatch,
    coa_import::dispatch,
    export_import::dispatch,
    open_any::dispatch,
    setup_sheets::dispatch,
    actuals::dispatch,
    schedule_days::dispatch,
    review::dispatch,
];

/// The stamp a budget saved as `text` gets (no file, so no time).
fn stamp(text: &str) -> crate::safety::DiskStamp {
    crate::safety::DiskStamp {
        len: text.len() as u64,
        modified: None,
        hash: crate::safety::content_hash(text.as_bytes()),
    }
}

pub(crate) fn json<T: serde::Serialize>(v: T) -> Result<Value, Error> {
    serde_json::to_value(v).map_err(|e| Error::Refused(e.to_string()))
}

/// `args.doc`, the budget a command names.
fn doc_of(command: &str, args: &Value) -> Result<Option<DocId>, Error> {
    match args.get("doc") {
        None | Some(Value::Null) => Ok(None),
        Some(v) => serde_json::from_value(v.clone())
            .map(Some)
            .map_err(|e| Error::BadArguments {
                command: command.to_string(),
                message: format!("doc: {e}"),
            }),
    }
}

/// One call being dispatched.
pub(crate) struct Call<'a> {
    host: &'a Host,
    command: &'a str,
    args: Value,
    doc: Option<DocId>,
}

impl Call<'_> {
    /// The command's name.
    pub(crate) fn command(&self) -> &str {
        self.command
    }

    /// The budget the command names (`None`: the one in front).
    pub(crate) fn doc(&self) -> Option<DocId> {
        self.doc
    }

    /// Every open budget.
    pub(crate) fn state(&self) -> &AppState {
        &self.host.state
    }

    /// The host's one `T`, made on first use: what a module keeps between
    /// calls (a picked file by its token).
    pub(crate) fn kept<T: Default + Send + Sync + 'static>(&self) -> Arc<T> {
        self.host.kept()
    }

    pub(crate) fn args<A: DeserializeOwned>(&self) -> Result<A, Error> {
        // The page sends `{}` or nothing for a command without arguments.
        let args = match &self.args {
            Value::Null => Value::Object(Default::default()),
            a => a.clone(),
        };
        serde_json::from_value(args).map_err(|e| Error::BadArguments {
            command: self.command.to_string(),
            message: e.to_string(),
        })
    }

    pub(crate) fn lock(&self) -> Result<DocGuard<'_>, Error> {
        Ok(self.host.state.lock_doc(self.doc)?)
    }

    /// Runs `f` on the budget and replies with its view.
    pub(crate) fn view<T>(
        &self,
        f: impl FnOnce(&mut Session) -> EditResult<T>,
    ) -> Result<Value, Error> {
        let mut s = self.lock()?;
        f(&mut s)?;
        json(s.finish(None))
    }

    /// Reads the arguments as `A`, runs `f` on the budget and replies with
    /// its view.
    pub(crate) fn view_with<A: DeserializeOwned, T>(
        &self,
        f: impl FnOnce(&mut Session, A) -> EditResult<T>,
    ) -> Result<Value, Error> {
        let a = self.args()?;
        self.view(|s| f(s, a))
    }

    /// As [`Call::view_with`], the view noting the row `f` made.
    pub(crate) fn created<A: DeserializeOwned>(
        &self,
        f: impl FnOnce(&mut Session, A) -> EditResult<RowId>,
    ) -> Result<Value, Error> {
        let a = self.args()?;
        let mut s = self.lock()?;
        let id = f(&mut s, a)?;
        json(s.finish(Some(id)))
    }
}

// The arguments, named as the page sends them: camelCase, as Tauri reads a
// command's arguments (`set_account_coding` alone takes snake_case).

#[derive(Deserialize)]
struct FocusArgs {
    category: Option<String>,
    ti: Option<usize>,
    account: Option<String>,
    ai: Option<usize>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RowIds {
    row_ids: Vec<RowId>,
}

#[derive(Deserialize)]
struct ChargeArgs {
    index: Option<usize>,
    charge: ChargeInput,
}

#[derive(Deserialize)]
struct To {
    to: usize,
}

#[derive(Deserialize)]
struct StepVersion {
    step: usize,
    name: String,
    note: Option<String>,
    created: String,
}

#[derive(Deserialize)]
struct Title {
    title: String,
}

#[derive(Deserialize)]
struct AddCategory {
    code: String,
    name: String,
    at: Option<usize>,
}

#[derive(Deserialize)]
struct NameAt {
    name: String,
    at: Option<usize>,
}

#[derive(Deserialize)]
struct UpdateTopsheetRow {
    index: usize,
    code: Option<String>,
    name: String,
}

#[derive(Deserialize)]
struct Index {
    index: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddAccount {
    category_code: String,
    code: String,
    name: String,
    after: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateAccount {
    code: String,
    new_code: String,
    name: String,
}

#[derive(Deserialize)]
struct Code {
    code: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DetailField {
    row_id: RowId,
    field: String,
    value: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RowsField {
    row_ids: Vec<RowId>,
    field: String,
    value: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddDetail {
    account_code: String,
    after: Option<RowId>,
    kind: String,
}

#[derive(Deserialize)]
struct Cells {
    edits: Vec<crate::batch::CellEdit>,
    action: Option<String>,
}

#[derive(Deserialize)]
struct Global {
    name: String,
    formula: String,
}

#[derive(Deserialize)]
struct Name {
    name: String,
}

#[derive(Deserialize)]
struct GlobalInfo {
    name: String,
    info: GlobalInfoInput,
}

#[derive(Deserialize)]
struct DisplayPrefs {
    prefs: DisplayPrefsInput,
}

#[derive(Deserialize)]
struct Fringe {
    name: String,
    fringe: FringeInput,
}

#[derive(Deserialize)]
struct Group {
    name: String,
    group: GroupInput,
}

#[derive(Deserialize)]
struct GroupRule {
    rule: String,
    value: String,
}

#[derive(Deserialize)]
struct Unit {
    name: String,
    unit: UnitInput,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeleteUnit {
    name: String,
    clear_lines: Option<bool>,
}

#[derive(Deserialize)]
struct Currency {
    code: String,
    currency: CurrencyInput,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeleteCurrency {
    code: String,
    clear_lines: Option<bool>,
}

#[derive(Deserialize)]
struct Text {
    value: String,
}

#[derive(Deserialize)]
struct Flag {
    value: bool,
}

#[derive(Deserialize)]
struct FringeRow {
    row: FringeRowInput,
}

#[derive(Deserialize)]
struct FringePosting {
    mode: String,
    suffix: Option<String>,
}

#[derive(Deserialize)]
struct AccountCoding {
    category_width: u8,
    account_width: u8,
    sub_width: u8,
    set_width: u8,
    separator: Option<String>,
}

#[derive(Deserialize)]
struct MaybeCode {
    code: Option<String>,
}

#[derive(Deserialize)]
struct Move {
    index: usize,
    to: usize,
}

#[derive(Deserialize)]
struct Credit {
    index: Option<usize>,
    credit: CreditInput,
}

#[derive(Deserialize)]
struct Set {
    name: String,
    set: SetInput,
}

#[derive(Deserialize)]
struct Location {
    name: String,
    location: LocationInput,
}

#[derive(Deserialize)]
struct Info {
    name: String,
    value: InfoInput,
}

#[derive(Deserialize)]
struct Frozen {
    frozen: bool,
}

#[derive(Deserialize)]
struct OpenMthd {
    name: String,
    text: String,
}

#[derive(Deserialize)]
struct SaveMthd {
    name: Option<String>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn call(h: &Host, c: &str, args: Value) -> Value {
        h.dispatch(c, args).unwrap_or_else(|e| panic!("{c}: {e}"))
    }

    #[test]
    fn every_listed_command_is_dispatched_and_nothing_else() {
        let h = Host::new();
        for c in COMMANDS {
            // Empty arguments: a command either runs or says what's
            // missing, but it is never unknown.
            let r = h.dispatch(c, json!({}));
            assert!(
                !matches!(r, Err(Error::UnknownCommand(_))),
                "{c} isn't dispatched"
            );
        }
        assert!(matches!(
            h.dispatch("open_recent", json!({})),
            Err(Error::UnknownCommand(_))
        ));
        let mut seen = std::collections::BTreeSet::new();
        assert!(COMMANDS.iter().all(|c| seen.insert(c)), "listed twice");
    }

    #[test]
    fn edits_undo_and_views_go_through_json_as_the_page_sends_them() {
        let h = Host::new();
        let v = call(&h, "get_view", json!({}));
        let doc = v["doc"].as_u64().unwrap();
        assert_eq!(v["can_undo"], false);

        let v = call(&h, "set_title", json!({"doc": doc, "title": "Feature"}));
        assert_eq!(v["budget"]["title"], "Feature");
        assert_eq!(v["can_undo"], true);
        assert_eq!(v["file"]["dirty"], true);

        // camelCase arguments, as Tauri reads them.
        let code = v["budget"]["topsheet"][0]["accounts"][0]["code"]
            .as_str()
            .unwrap()
            .to_string();
        let v = call(
            &h,
            "add_detail",
            json!({"doc": doc, "accountCode": code, "after": null, "kind": "line"}),
        );
        let row = v["created_row"].as_u64().expect("the new row");
        let v = call(
            &h,
            "set_detail_field",
            json!({"rowId": row, "field": "rate", "value": "100"}),
        );
        assert_eq!(v["error"], Value::Null);

        let steps = call(&h, "history_list", json!({"doc": doc}));
        assert_eq!(steps["current"], 3);
        assert_eq!(
            steps["steps"][0]["label"], "Rename budget to Feature",
            "labelled as the desktop labels it"
        );
        let v = call(&h, "undo", json!({}));
        assert_eq!(v["can_redo"], true);
        let v = call(&h, "history_jump", json!({"to": 0}));
        assert_eq!(v["budget"]["title"], "Untitled feature");
    }

    #[test]
    fn refusals_and_bad_arguments_change_nothing() {
        let h = Host::new();
        let before = call(&h, "get_view", json!({}))["rev"].clone();
        let e = h
            .dispatch("delete_account", json!({"code": "99-99"}))
            .unwrap_err();
        assert!(matches!(e, Error::Refused(_)), "{e:?}");
        let e = h.dispatch("set_title", json!({"title": 3})).unwrap_err();
        assert!(matches!(e, Error::BadArguments { .. }), "{e:?}");
        assert!(e.message().contains("set_title"));
        let e = h.dispatch("get_view", json!({"doc": 99})).unwrap_err();
        assert_eq!(e.message(), "That budget has been closed.");
        assert_eq!(call(&h, "get_view", json!({}))["rev"], before);
    }

    #[test]
    fn versions_and_a_step_saved_as_a_version() {
        let h = Host::new();
        let at = "2026-10-09T14:30:00-07:00";
        call(&h, "set_title", json!({"title": "One"}));
        let v = call(
            &h,
            "save_version",
            json!({"name": "First", "note": null, "created": at}),
        );
        let id = v["budget"]["versions"][0]["id"].clone();
        call(&h, "rename_version", json!({"id": id, "name": "Draft"}));
        call(&h, "set_title", json!({"title": "Two"}));
        let v = call(
            &h,
            "history_save_version",
            json!({"step": 1, "name": "From the history", "created": at}),
        );
        assert_eq!(v["budget"]["title"], "Two", "the content stays as it is");
        assert_eq!(v["budget"]["versions"][1]["name"], "From the history");
        let v = call(&h, "restore_version", json!({"id": id, "created": at}));
        assert_eq!(v["budget"]["title"], "One");
        let v = call(&h, "delete_version", json!({"id": id}));
        assert!(v["budget"]["versions"]
            .as_array()
            .unwrap()
            .iter()
            .all(|x| x["name"] != "Draft"));
    }

    #[test]
    fn a_read_only_host_refuses_every_edit_and_changes_nothing() {
        let h = Host::new();
        call(&h, "set_title", json!({"title": "Mine"}));
        let info = call(&h, "host_info", json!({}));
        assert_eq!(info["kind"], "web");
        assert_eq!(info["commands_version"], COMMANDS_VERSION);
        assert_eq!(info["read_only"], false);

        h.set_read_only(true);
        assert_eq!(call(&h, "host_info", json!({}))["read_only"], true);
        let before = call(&h, "get_view", json!({}));
        let e = h
            .dispatch("set_title", json!({"title": "Theirs"}))
            .unwrap_err();
        assert_eq!(e.message(), crate::session::HELD_READ_ONLY);
        let e = h.dispatch("history_jump", json!({"to": 0})).unwrap_err();
        assert_eq!(e.message(), crate::session::HELD_READ_ONLY);
        // Undo and redo do nothing; views still come.
        call(&h, "undo", json!({}));
        let after = call(&h, "get_view", json!({}));
        assert_eq!(after["rev"], before["rev"]);
        assert_eq!(after["budget"]["title"], "Mine");

        // A budget opened later is read-only too, and saving still works.
        let text = method_format::to_string(&method_core::starter::budget());
        call(&h, "open_mthd", json!({"name": "Later.mthd", "text": text}));
        assert!(h.dispatch("set_title", json!({"title": "x"})).is_err());
        call(&h, "save_mthd", json!({}));

        h.set_read_only(false);
        let v = call(&h, "set_title", json!({"title": "Editable"}));
        assert_eq!(v["budget"]["title"], "Editable");
    }

    #[test]
    fn budgets_come_and_go_as_mthd_text() {
        let h = Host::new();
        let first = call(&h, "get_view", json!({}))["doc"].as_u64().unwrap();
        let mut b = method_core::starter::budget();
        b.title = "Pilot".into();
        let text = method_format::to_string(&b);
        let v = call(&h, "open_mthd", json!({"name": "Pilot.mthd", "text": text}));
        assert_eq!(v["budget"]["title"], "Pilot");
        assert_eq!(v["file"]["name"], "Pilot.mthd");
        assert_eq!(v["file"]["dirty"], false);
        let doc = v["doc"].as_u64().unwrap();
        assert_ne!(doc, first);
        assert_eq!(
            v["tabs"].as_array().unwrap().len(),
            1,
            "took Untitled's tab"
        );

        call(&h, "set_title", json!({"title": "Pilot 2"}));
        let saved = call(&h, "save_mthd", json!({"doc": doc}));
        assert_eq!(saved["view"]["file"]["dirty"], false);
        let back = method_format::from_str(saved["text"].as_str().unwrap()).unwrap();
        assert_eq!(back.title, "Pilot 2");

        let e = h
            .dispatch("open_mthd", json!({"name": "x.mthd", "text": "nope"}))
            .unwrap_err();
        assert!(
            e.message().starts_with("x.mthd isn't a readable budget"),
            "{e}"
        );
    }
}
