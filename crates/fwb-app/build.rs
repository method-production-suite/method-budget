//! Generates Tauri's context and the permission set for our own commands.
//!
//! Listing the commands here turns on Tauri's access control for them: a
//! command must be listed here *and* allowed in the calling window's
//! capability (`capabilities/main.json`, or `capabilities/tools.json` for the
//! tools window), or that web view can't call it. Keep this list in step with
//! `tauri::generate_handler!` in `src/app.rs`.

const COMMANDS: &[&str] = &[
    "get_view",
    "get_full_view",
    "focus_account",
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
    "preview_charge",
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
    // Row style, group colors (src/rowstyle.rs)
    "set_row_style",
    "set_group_color",
    "update_group",
    // Dated notes on lines, accounts and categories (src/notes.rs)
    "add_note",
    "update_note",
    "delete_note",
    "set_note_done",
    // Setup lists: use counts, custom order, short codes (src/setup_lists.rs)
    "setup_usage",
    "set_setup_order",
    "move_setup_item",
    "set_short_code",
    // Apply panel: tag the selected rows, fringe ranges (src/apply.rs)
    "apply_summary",
    "apply_to_rows",
    "assign_to_rows",
    "clear_from_rows",
    "replace_tags_on_rows",
    "make_fringe_range",
    "clear_fringe_range",
    // Quick entry: add setup items from a cell, new-line defaults (src/quickadd.rs)
    "set_cell_creating",
    "set_new_line_defaults",
    // Global info and display settings (src/display.rs)
    "set_global_info",
    "set_display_prefs",
    // Copy and paste whole rows, accounts and categories with their setup
    // (src/clipboard.rs)
    "clip_copy",
    "clip_cut",
    "clip_preview",
    "clip_paste",
    "clip_forget",
    // Saving safety: autosave, recovery, backups, recent and system-opened
    // files (src/safety_app.rs)
    "autosave_setting",
    "set_autosave",
    "flush_autosave",
    "open_pending",
    "open_recent",
    "recovery_list",
    "open_recovered",
    "discard_recovered",
    "backup_list",
    "restore_backup",
    "reveal_backups",
    "resolve_external_change",
    // The tools panel's own window and its Notes tab (src/tools.rs)
    "tools_window",
    "close_tools_window",
    "notes_list",
    // The Budget Setup window (src/setupwin.rs)
    "setup_window",
    "close_setup_window",
    // Versions saved in the budget, and comparing budgets (src/versions.rs,
    // src/compare.rs)
    "save_version",
    "restore_version",
    "rename_version",
    "delete_version",
    // The History list (src/history.rs)
    "history_list",
    "history_jump",
    "history_save_version",
    "compare_budgets",
    "compare_detail",
    "compare_pick_files",
    // What Changed and Merge Changes From (src/diffmerge.rs)
    "what_changed",
    "merge_preview",
    "merge_apply",
    "merge_pick_file",
    // Budgets as tabs (src/tabs.rs)
    "new_tab",
    "switch_tab",
    "move_tab",
    "close_tab",
    "list_tabs",
    // Split view (src/tabs.rs)
    "split_view",
    "close_split",
    "set_target_pane",
    "move_tab_in_pane",
    "move_to_pane",
    "pane_view",
    // Reports: saved report presets, printing and PDF (src/reports.rs)
    "save_report_preset",
    "delete_report_preset",
    "print_report",
    // Spreadsheet export: Excel, CSV, tab-delimited (src/export.rs)
    "export_budget",
    // Tag presets and line blocks (src/presets.rs)
    "save_tag_preset",
    "tag_preset_from_line",
    "apply_tag_preset",
    "save_line_preset",
    "line_preset_preview",
    "insert_line_preset",
    "rename_tool_preset",
    "delete_tool_preset",
    "move_tool_preset",
    // Moving rows up and down (src/moverows.rs)
    "move_rows",
    "move_accounts",
    "move_topsheet_rows",
    // Renaming setup items everywhere (src/rename.rs)
    "rename_setup_item",
    // Find and replace (src/find.rs)
    "find",
    "replace",
    // Import setup from another budget, templates (src/import_setup.rs,
    // src/templates.rs)
    "import_setup_pick",
    "import_setup",
    "template_new",
    "template_save",
    // Import lines from a spreadsheet with a column mapping (src/sheet_import.rs)
    "sheet_import_pick",
    "sheet_import_preview",
    "sheet_import",
    // Actuals and the cost report (src/actuals.rs)
    "actuals_pick",
    "actuals_preview",
    "actuals_import",
    "actuals_list",
    "actuals_update",
    "actuals_delete",
    "actuals_remove_batch",
    "actuals_set_etc",
    "actuals_lines",
    // Renumbering accounts to a new pattern (src/renumber.rs)
    "renumber_preview",
    "renumber_accounts",
    // Copies for sharing and sub-budgets (src/sharing.rs)
    "share_copy_save",
    "share_copy_editable",
    "share_copy_original",
    "share_copy_store",
    "sub_budget_new",
    // The personal setup library (src/library.rs)
    "library_view",
    "library_create",
    "library_choose",
    "library_add_preview",
    "library_add",
    "library_save_preview",
    "library_save",
    "library_folder",
    "library_move",
    // Explain This Number (src/explain.rs)
    "explain_number",
    // The overtime helper (src/overtime.rs)
    "ot_calculate",
    "ot_apply",
    "ot_save_pattern",
    "ot_delete_pattern",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("failed to run tauri-build");
}
