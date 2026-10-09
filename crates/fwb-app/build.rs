//! Generates Tauri's context and the permission set for our own commands.
//!
//! Listing the commands here turns on Tauri's access control for them: a
//! command must be listed here *and* allowed in `capabilities/main.json`, or
//! the web view can't call it. Keep this list in step with
//! `tauri::generate_handler!` in `src/app.rs`.

const COMMANDS: &[&str] = &[
    "get_view",
    "new_budget",
    "open_budget",
    "save_budget",
    "undo",
    "redo",
    "set_title",
    "add_category",
    "add_production_total",
    "update_topsheet_row",
    "delete_topsheet_row",
    "add_account",
    "update_account",
    "delete_account",
    "set_detail_field",
    "set_rows_field",
    "add_detail",
    "delete_details",
    "selection_totals",
    "set_global",
    "delete_global",
    "set_fringe",
    "delete_fringe",
    "set_group",
    "delete_group",
    "set_unit",
    "delete_unit",
    "set_currency",
    "delete_currency",
    // Topsheet setup tabs
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
    "set_fringe_posting",
    "set_account_coding",
    "set_base_currency",
    "set_totals_currency",
    "lock_baseline",
    "clear_baseline",
    // Several cells at once (paste, fill down, cut)
    "set_cells",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("failed to run tauri-build");
}
