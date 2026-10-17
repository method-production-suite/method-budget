// Every keyboard shortcut in the app, in one list. The native menu bar
// (src/menu.rs) uses the same ids and keys; a Rust test reads this file and
// fails if the two drift apart, so keep one entry per line.
//
// Keys are written once for both platforms: `mod` is Command on macOS and Ctrl
// on Windows and Linux; `alt` is Option on macOS; `ctrl` is the Control key
// everywhere (so Ctrl on Windows and Linux too). An entry with `mac` and
// `other` instead of `keys` differs by platform; a list gives alternatives
// (the first is the one menus show).
//
// scope says who answers the key:
//   app      The menu item's key equivalent on macOS (main.js leaves the key
//            alone there); main.js on Windows and Linux. Works anywhere.
//   grid     main.js, whenever focus isn't in a text field outside the grid.
//            The menu item runs the same command when clicked.
//   native   The stock Cut, Copy and Paste: text fields edit natively, and the
//            grid answers the browser's copy, cut and paste events.
//   builtin  The grid itself (grid.js). Listed on the shortcut sheet only.
//   menu     A menu command with no shortcut.
//
// field: true   In a text field the command acts on the field (undo typing,
//               select its text) instead of the budget.
// text: true    The keys also mean something in a text field (move the caret,
//               delete a word), so a text field keeps them.
// anywhere: true  A grid command that also works from a text field outside
//               the grid (a tools panel field): the keys mean nothing there.

import { isMac } from './dom.js';

export const SHORTCUTS = [
  // File
  { id: 'file.new', group: 'File', label: 'New budget, or open a recent one (the start window)', keys: 'mod+n', scope: 'app' },
  { id: 'file.new_tab', group: 'File', label: 'New tab (a new budget)', keys: 'mod+t', scope: 'app' },
  { id: 'file.open', group: 'File', label: 'Open a budget (in a new tab)', keys: 'mod+o', scope: 'app' },
  { id: 'file.save', group: 'File', label: 'Save', keys: 'mod+s', scope: 'app' },
  { id: 'file.save_as', group: 'File', label: 'Save as a new file', keys: 'mod+shift+s', scope: 'app' },
  { id: 'file.new_from_template', group: 'File', label: 'New budget from a template (any budget file): its chart, setup and settings, in a new tab', scope: 'menu' },
  { id: 'file.new_sub_budget', group: 'File', label: 'New sub-budget: a filtered copy by categories, accounts, groups, sets and locations, in a new tab', scope: 'menu' },
  { id: 'file.save_share_copy', group: 'File', label: 'Save a copy for sharing: a snapshot named by title, version and date, optionally without notes or private info, that opens read-only', scope: 'menu' },
  { id: 'file.share', group: 'File', label: 'Share this budget through a folder: everyone opening it there works in the same budget, with everyone’s changes', scope: 'menu' },
  { id: 'file.save_as_template', group: 'File', label: 'Save a copy of the budget as a template: no lines, info, baseline, versions or notes', scope: 'menu' },
  { id: 'file.import_setup', group: 'File', label: 'Import setup items from another budget file, with what they need', scope: 'menu' },
  { id: 'file.import_setup_pack', group: 'File', label: 'Import a setup pack: its setup items, with what they need, marked with the pack’s title and effective date', scope: 'menu' },
  { id: 'file.export_setup_pack', group: 'File', label: 'Export a setup pack: picked setup items and what they need, with a title, version, effective date and sources', scope: 'menu' },
  { id: 'file.import_spreadsheet', group: 'File', label: 'Import lines from an Excel workbook, CSV or tab-delimited file, mapping its columns to budget fields', scope: 'menu' },
  { id: 'file.import_chart', group: 'File', label: 'Import a chart of accounts from a spreadsheet or another budget: as a new budget with no lines, or applied to this one', scope: 'menu' },
  { id: 'file.import_budget_export', group: 'File', label: 'Import a whole budget from another program’s budget export, as a new tab, with its totals checked against the export’s own', scope: 'menu' },
  { id: 'file.import_setup_lists', group: 'File', label: 'Import setup lists (fringes, globals, groups and more) from an Excel workbook or CSV files, checked row by row', scope: 'menu' },
  { id: 'file.import_actuals', group: 'File', label: 'Import actual costs from an accounting export (Excel, CSV or tab-delimited), matched to accounts, for the cost report', scope: 'menu' },
  { id: 'file.versions', group: 'File', label: 'Versions: save, restore, rename and compare named copies of the budget', scope: 'menu' },
  { id: 'file.compare', group: 'File', label: 'Compare the budget with open budgets, its versions or other files', scope: 'menu' },
  { id: 'file.what_changed', group: 'File', label: 'What changed: every change since a version, the saved file, an open budget or another file, in plain words, with the money it moved', scope: 'menu' },
  { id: 'file.merge', group: 'File', label: 'Merge changes from a returned copy: both sides’ changes since the version it was shared from; pick a side where they clash', scope: 'menu' },
  { id: 'file.print_budget', group: 'File', label: 'Print: choose sections and layout, preview, save as PDF', keys: 'mod+p', scope: 'app' },
  { id: 'file.print_topsheet', group: 'File', label: 'Print the topsheet', keys: 'mod+shift+p', scope: 'app' },
  // File > Export: spreadsheets of the budget (src/export.rs).
  { id: 'file.export_xlsx', group: 'File', label: 'Export an Excel workbook: topsheet, accounts, detail and setup tables', scope: 'menu' },
  { id: 'file.export_csv', group: 'File', label: 'Export CSV files, one per table', scope: 'menu' },
  { id: 'file.export_tsv', group: 'File', label: 'Export the detail as tab-delimited text, for accounting tools', scope: 'menu' },
  { id: 'file.export_setup_lists', group: 'File', label: 'Export setup lists: each kind of setup item as a sheet or CSV file, to edit and import back', scope: 'menu' },
  { id: 'file.close_tab', group: 'File', label: 'Close the budget tab (the window, when it is the last one)', keys: 'mod+w', scope: 'app' },
  { id: 'file.close', group: 'File', label: 'Close the window (every tab)', keys: 'mod+shift+w', scope: 'app' },
  { id: 'app.quit', group: 'File', label: 'Quit', keys: 'mod+q', scope: 'app' },

  // Edit
  { id: 'edit.undo', group: 'Edit', label: 'Undo', keys: 'mod+z', scope: 'app', field: true },
  { id: 'edit.redo', group: 'Edit', label: 'Redo', mac: 'mod+shift+z', other: ['mod+y', 'mod+shift+z'], scope: 'app', field: true },
  { id: 'edit.cut', group: 'Edit', label: 'Cut the selected cells (whole rows: the rows, with everything on them)', keys: 'mod+x', scope: 'native' },
  { id: 'edit.copy', group: 'Edit', label: 'Copy the selected cells (tab-separated, for spreadsheets; whole rows also with everything on them)', keys: 'mod+c', scope: 'native' },
  { id: 'edit.paste', group: 'Edit', label: 'Paste cells, starting at the selection (copied rows: above it)', keys: 'mod+v', scope: 'native' },
  // Whole rows whatever columns are selected (clipboard.js): lines, accounts
  // on the account list, categories on the topsheet.
  { id: 'edit.cut_rows', group: 'Edit', label: 'Cut the selected rows, with everything on them', scope: 'menu' },
  { id: 'edit.copy_rows', group: 'Edit', label: 'Copy the selected rows, with their fringes, groups and other setup', scope: 'menu' },
  { id: 'edit.paste_rows', group: 'Edit', label: 'Paste copied rows above the selected row', scope: 'menu' },
  { id: 'edit.select_all', group: 'Edit', label: 'Select every row', keys: 'mod+a', scope: 'app', field: true },
  { id: 'edit.fill_down', group: 'Edit', label: 'Fill down (top selected cell into the cells below)', keys: 'mod+shift+d', scope: 'grid' },
  { id: 'edit.delete_rows', group: 'Edit', label: 'Delete the selected rows (or the category or account)', keys: 'mod+d', scope: 'grid', text: true },
  // Move the selected rows one place (moverows: lines, accounts, categories
  // and production totals). Text fields keep Option+arrows for the caret.
  { id: 'edit.move_up', group: 'Edit', label: 'Move the selected rows up one place (lines, accounts, or topsheet rows)', keys: 'alt+up', scope: 'grid', text: true },
  { id: 'edit.move_down', group: 'Edit', label: 'Move the selected rows down one place', keys: 'alt+down', scope: 'grid', text: true },
  { id: 'grid.clear', group: 'Edit', label: 'Clear the selected cells', keys: ['backspace', 'delete'], scope: 'builtin' },
  { id: 'edit.note', group: 'Edit', label: 'Add a dated note to the line, account or category', keys: 'mod+shift+space', scope: 'grid' },
  { id: 'edit.make_range', group: 'Edit', label: 'Make the selected lines one fringe range', keys: 'mod+r', scope: 'grid' },
  { id: 'edit.remove_range', group: 'Edit', label: 'Take the selected lines out of their fringe range', keys: 'mod+shift+r', scope: 'grid' },
  // How a total is worked out (explain.js) and the overtime helper (overtime.js).
  { id: 'edit.explain', group: 'Edit', label: 'Explain the total under the cursor: every step of how it is worked out, in the Explain tab', keys: 'mod+e', scope: 'grid' },
  { id: 'edit.overtime', group: 'Edit', label: 'Overtime helper: work out a day or week of a labor line with overtime, and write it into the line', scope: 'menu' },
  { id: 'share.blame', group: 'Edit', label: 'Who changed this: every value the cell under the cursor has had since the budget was shared, and who typed it', scope: 'menu' },
  // Find and replace: the Find tab of the tools panel (find.js). Cmd+G is
  // the Globals tab, so the next and previous result are F3 and Shift+F3
  // (and Enter and Shift+Enter in the Find box).
  { id: 'edit.find', group: 'Edit', label: 'Find (the Find tab of the tools panel)', keys: 'mod+f', scope: 'app' },
  { id: 'edit.replace', group: 'Edit', label: 'Find and replace', keys: 'mod+shift+f', scope: 'app' },
  { id: 'edit.find_next', group: 'Edit', label: 'Go to the next find result', keys: 'f3', scope: 'grid', anywhere: true },
  { id: 'edit.find_previous', group: 'Edit', label: 'Go to the previous find result', keys: 'shift+f3', scope: 'grid', anywhere: true },
  { id: 'find.keys', group: 'Edit', label: 'In the Find box: the next or previous result', keys: ['enter', 'shift+enter'], scope: 'builtin' },

  // Moving around and editing cells
  { id: 'grid.move', group: 'Cells', label: 'Move between cells', keys: ['up', 'down', 'left', 'right'], scope: 'builtin' },
  { id: 'grid.extend', group: 'Cells', label: 'Extend the selection', keys: ['shift+up', 'shift+down', 'shift+left', 'shift+right'], scope: 'builtin' },
  { id: 'grid.tab', group: 'Cells', label: 'Next or previous cell', keys: ['tab', 'shift+tab'], scope: 'builtin' },
  { id: 'grid.row_ends', group: 'Cells', label: 'First or last column', keys: ['home', 'end'], scope: 'builtin' },
  { id: 'grid.grid_ends', group: 'Cells', label: 'First or last row', keys: ['mod+home', 'mod+end'], scope: 'builtin' },
  { id: 'grid.edit', group: 'Cells', label: 'Edit a cell, keeping its text (or just start typing to replace it)', keys: 'f2', scope: 'builtin' },
  { id: 'grid.commit', group: 'Cells', label: 'Finish editing and move down or up (detail lines)', keys: ['enter', 'shift+enter'], scope: 'builtin' },
  { id: 'grid.commit_all', group: 'Cells', label: 'While editing: same value into every selected row', keys: 'mod+enter', scope: 'builtin' },
  { id: 'grid.cancel', group: 'Cells', label: 'Cancel the edit, clear the selection, or go up a level', keys: 'escape', scope: 'builtin' },
  { id: 'grid.context_menu', group: 'Cells', label: 'Open the menu for the cursor cell (also the menu key, or a right-click)', keys: 'shift+f10', scope: 'builtin' },

  // Insert
  { id: 'insert.line', group: 'Insert', label: 'Insert a line below', keys: 'mod+i', scope: 'grid' },
  { id: 'insert.subtotal', group: 'Insert', label: 'Insert a subtotal row', keys: 'mod+=', scope: 'grid' },
  { id: 'insert.adjustment', group: 'Insert', label: 'Insert an adjustment row', keys: ['mod+-', 'mod+alt+a'], scope: 'grid' },
  { id: 'insert.separator', group: 'Insert', label: 'Insert a separator (a drawn rule line)', scope: 'menu' },
  { id: 'insert.account', group: 'Insert', label: 'Add an account', scope: 'menu' },
  { id: 'insert.category', group: 'Insert', label: 'Add a category', scope: 'menu' },
  { id: 'insert.production_total', group: 'Insert', label: 'Add a production total', scope: 'menu' },

  // Format: text style of the selected detail lines (rowformat.js). Cmd+I
  // inserts a line, so italic is Shift+Cmd+I; Cmd+[ and Cmd+] step accounts,
  // so indenting is Option+Cmd+Right and Left.
  { id: 'format.bold', group: 'Format', label: 'Bold on or off for the selected lines', keys: 'mod+b', scope: 'grid' },
  { id: 'format.italic', group: 'Format', label: 'Italic on or off for the selected lines', keys: 'mod+shift+i', scope: 'grid' },
  { id: 'format.indent_more', group: 'Format', label: 'Indent the selected lines one step more', keys: 'mod+alt+right', scope: 'grid' },
  { id: 'format.indent_less', group: 'Format', label: 'Indent the selected lines one step less', keys: 'mod+alt+left', scope: 'grid' },
  { id: 'format.color.none', group: 'Format', label: 'Text color: none (the group color, if any)', scope: 'menu' },
  { id: 'format.color.red', group: 'Format', label: 'Text color: red', scope: 'menu' },
  { id: 'format.color.orange', group: 'Format', label: 'Text color: orange', scope: 'menu' },
  { id: 'format.color.gold', group: 'Format', label: 'Text color: gold', scope: 'menu' },
  { id: 'format.color.green', group: 'Format', label: 'Text color: green', scope: 'menu' },
  { id: 'format.color.teal', group: 'Format', label: 'Text color: teal', scope: 'menu' },
  { id: 'format.color.blue', group: 'Format', label: 'Text color: blue', scope: 'menu' },
  { id: 'format.color.purple', group: 'Format', label: 'Text color: purple', scope: 'menu' },
  { id: 'format.color.pink', group: 'Format', label: 'Text color: pink', scope: 'menu' },
  { id: 'format.clear', group: 'Format', label: 'Clear formatting (bold, italic, indent and color)', scope: 'menu' },

  // Go
  { id: 'go.topsheet', group: 'Go', label: 'Go to the topsheet', keys: 'mod+shift+up', scope: 'grid', text: true },
  { id: 'go.up', group: 'Go', label: 'Up a level', keys: 'mod+up', scope: 'grid', text: true },
  { id: 'go.down', group: 'Go', label: 'Open the category or account (also Enter)', keys: 'mod+down', scope: 'grid', text: true },
  { id: 'go.next_account', group: 'Go', label: 'Next account', keys: ['mod+right', 'mod+]'], scope: 'grid', text: true },
  { id: 'go.prev_account', group: 'Go', label: 'Previous account', keys: ['mod+left', 'mod+['], scope: 'grid', text: true },

  // View: the tools panel (tools.js), wherever it is
  { id: 'view.toggle_panel', group: 'View', label: 'Show or hide the tools panel (docked or in its window)', keys: 'mod+alt+s', scope: 'app' },
  { id: 'view.panel.right', group: 'View', label: 'Dock the tools panel at the right', scope: 'menu' },
  { id: 'view.panel.bottom', group: 'View', label: 'Dock the tools panel at the bottom', scope: 'menu' },
  { id: 'view.panel.window', group: 'View', label: 'Pop the tools panel out into its own window', scope: 'menu' },
  // Split view (pane.js): two panes of budget tabs side by side.
  { id: 'view.split_right', group: 'View', label: 'Split right: a second pane of budget tabs (or, when split, go to the other pane)', keys: 'mod+shift+backslash', scope: 'app' },
  { id: 'view.close_split', group: 'View', label: 'Close the split: every tab back in one strip', scope: 'menu' },
  // Display choices, saved for you (display.js). Cmd+= and Cmd+- insert rows,
  // so the zoom keys add Option.
  { id: 'view.formulas', group: 'View', label: 'Selected cells show their formulas instead of values, or not', keys: 'mod+alt+f', scope: 'app' },
  { id: 'view.hide_zero', group: 'View', label: 'Hide or show categories and accounts that total zero', scope: 'menu' },
  { id: 'view.totals_cents', group: 'View', label: 'Totals with cents, or rounded to whole amounts', scope: 'menu' },
  { id: 'view.zoom_in', group: 'View', label: 'Zoom the grid in', keys: 'mod+alt+=', scope: 'app' },
  { id: 'view.zoom_out', group: 'View', label: 'Zoom the grid out', keys: 'mod+alt+-', scope: 'app' },
  { id: 'view.zoom_reset', group: 'View', label: 'Grid at actual size', keys: 'mod+alt+0', scope: 'app' },
  // Budget Setup (setupwin.js): the window, and each of its tabs, with the
  // keys long-time users of other budgeting tools have in their hands
  // (decisions.md, entries 15 and 42).
  { id: 'view.setup', group: 'View', label: 'Budget Setup: every setup table, charges, credits and the budget settings, in a window of their own', scope: 'menu' },
  { id: 'view.tab.fringes', group: 'View', label: 'Budget Setup: Fringes', keys: 'mod+3', scope: 'app' },
  { id: 'view.tab.groups', group: 'View', label: 'Budget Setup: Groups', keys: 'mod+4', scope: 'app' },
  { id: 'view.tab.globals', group: 'View', label: 'Budget Setup: Globals', keys: 'mod+g', scope: 'app' },
  { id: 'view.tab.units', group: 'View', label: 'Budget Setup: Units', keys: 'mod+7', scope: 'app' },
  { id: 'view.tab.sets', group: 'View', label: 'Budget Setup: Sets', keys: 'mod+8', scope: 'app' },
  { id: 'view.tab.locations', group: 'View', label: 'Budget Setup: Locations', keys: 'mod+5', scope: 'app' },
  { id: 'view.tab.currencies', group: 'View', label: 'Budget Setup: Currency', keys: 'mod+6', scope: 'app' },
  { id: 'view.tab.charges', group: 'View', label: 'Budget Setup: Charges', keys: 'mod+/', scope: 'app' },
  { id: 'view.tab.credits', group: 'View', label: 'Budget Setup: Credits', keys: 'mod+backslash', scope: 'app' },
  { id: 'view.tab.info', group: 'View', label: 'Budget Setup: Info', scope: 'menu' },
  { id: 'view.tab.settings', group: 'View', label: 'Budget Setup: Settings (budget preferences)', keys: 'mod+9', scope: 'app' },
  { id: 'view.tab.baseline', group: 'View', label: 'Budget Setup: Baseline', scope: 'menu' },
  // The tools panel's tabs (tools.js): the apply lists and browsing.
  { id: 'view.tool.fringes', group: 'View', label: 'Tools: Fringes (apply to the selected lines)', scope: 'menu' },
  { id: 'view.tool.groups', group: 'View', label: 'Tools: Groups (apply to the selected lines)', scope: 'menu' },
  { id: 'view.tool.globals', group: 'View', label: 'Tools: Globals', scope: 'menu' },
  { id: 'view.tool.units', group: 'View', label: 'Tools: Units', scope: 'menu' },
  { id: 'view.tab.presets', group: 'View', label: 'Tools: Presets (tag presets and line blocks)', scope: 'menu' },
  { id: 'view.tool.sets', group: 'View', label: 'Tools: Sets (apply to the selected lines)', scope: 'menu' },
  { id: 'view.tool.locations', group: 'View', label: 'Tools: Locations (apply to the selected lines)', scope: 'menu' },
  { id: 'view.tool.currencies', group: 'View', label: 'Tools: Currency (apply to the selected lines)', scope: 'menu' },
  { id: 'view.tab.notes', group: 'View', label: 'Tools: Notes (notes on the line, account or category, and every note as a checklist)', scope: 'menu' },
  { id: 'view.tab.history', group: 'View', label: 'Tools: History (every change you can undo; go back to one, or save it as a version)', scope: 'menu' },
  { id: 'view.tab.library', group: 'View', label: 'Tools: Library (your setup library: copy setup items into this budget or save them to it)', scope: 'menu' },
  { id: 'view.tab.actuals', group: 'View', label: 'Tools: Actuals (imported transactions, imports, and each account’s estimate to complete)', scope: 'menu' },
  { id: 'view.tab.review', group: 'View', label: 'Tools: Review (what needs a person after other people’s changes in a shared budget: values changed by two people and the like)', scope: 'menu' },
  { id: 'view.tab.activity', group: 'View', label: 'Tools: Activity (everyone’s changes to a shared budget, newest first, with who made them and when)', scope: 'menu' },
  // The tools panel's tab strips (tools.js).
  { id: 'tools.tab_keys', group: 'View', label: 'Tools tabs: the previous or next tab in the pane', keys: ['left', 'right'], scope: 'builtin' },
  { id: 'tools.move_tab', group: 'View', label: 'Tools tabs: move the tab left or right, or to the other pane', keys: ['alt+shift+left', 'alt+shift+right', 'alt+shift+up', 'alt+shift+down'], scope: 'builtin' },

  // Window: budgets open as tabs (tabs.js). Cmd+3 to Cmd+9 are the tools tabs.
  { id: 'window.next_tab', group: 'Window', label: 'Next budget tab', keys: ['mod+shift+]', 'ctrl+tab'], scope: 'grid', anywhere: true },
  { id: 'window.prev_tab', group: 'Window', label: 'Previous budget tab', keys: ['mod+shift+[', 'ctrl+shift+tab'], scope: 'grid', anywhere: true },
  { id: 'window.first_tab', group: 'Window', label: 'First budget tab', keys: 'mod+1', scope: 'grid', anywhere: true },
  { id: 'window.second_tab', group: 'Window', label: 'Second budget tab', keys: 'mod+2', scope: 'grid', anywhere: true },
  { id: 'window.move_to_other_pane', group: 'Window', label: 'Move the budget tab to the other pane (split view)', scope: 'menu' },
  { id: 'tabs.keys', group: 'Window', label: 'On a budget tab: the previous or next tab (also Home, End)', keys: ['left', 'right'], scope: 'builtin' },
  { id: 'tabs.move', group: 'Window', label: 'On a budget tab: move it left or right (or drag it)', keys: ['alt+shift+left', 'alt+shift+right'], scope: 'builtin' },

  // Help
  { id: 'help.shortcuts', group: 'Help', label: 'Show this list of shortcuts', keys: 'mod+shift+/', scope: 'app' },
  { id: 'help.user_guide', group: 'Help', label: 'User guide, in your browser', scope: 'menu' },
  { id: 'help.release_notes', group: 'Help', label: 'Release notes: what is new in this version', scope: 'menu' },
  { id: 'help.report_bug', group: 'Help', label: 'Report a bug: a new GitHub issue in your browser, with the app and system versions filled in', scope: 'menu' },
  { id: 'help.licenses', group: 'Help', label: 'Licenses and third-party notices', scope: 'menu' },
  { id: 'help.suite_site', group: 'Help', label: 'methodproduction.com, in your browser', scope: 'menu' },
];

/** Groups in the order the shortcut sheet shows them. */
export const GROUPS = ['File', 'Edit', 'Cells', 'Insert', 'Format', 'Go', 'View', 'Window', 'Help'];

/**
 * On macOS the native menu answers `app` keys; elsewhere main.js does. A
 * browser has no menu bar, so the page answers them there (host.js).
 */
export let menuRunsAppKeys = isMac;

/** No menu bar to answer `app` keys (a browser): the page answers them. */
export function pageRunsAppKeys() {
  menuRunsAppKeys = false;
}

const byId = new Map(SHORTCUTS.map((s) => [s.id, s]));

export function shortcutById(id) {
  return byId.get(id) || null;
}

/** The key specs for this platform, first one first. */
export function keysFor(entry) {
  if (!entry) return [];
  const k = entry.keys !== undefined ? entry.keys : isMac ? entry.mac : entry.other;
  if (k === undefined || k === null) return [];
  return Array.isArray(k) ? k : [k];
}

// ---------------------------------------------------------------------------
// Showing keys

const MAC_NAMES = {
  enter: '↩',
  backspace: '⌫',
  delete: '⌦',
  escape: 'Esc',
  tab: '⇥',
  up: '↑',
  down: '↓',
  left: '←',
  right: '→',
  home: 'Home',
  end: 'End',
  f2: 'F2',
  f3: 'F3',
  f10: 'F10',
  space: 'Space',
  backslash: '\\',
};

const PC_NAMES = {
  enter: 'Enter',
  backspace: 'Backspace',
  delete: 'Delete',
  escape: 'Esc',
  tab: 'Tab',
  up: 'Up',
  down: 'Down',
  left: 'Left',
  right: 'Right',
  home: 'Home',
  end: 'End',
  f2: 'F2',
  f3: 'F3',
  f10: 'F10',
  space: 'Space',
  backslash: '\\',
};

function parse(spec) {
  const parts = spec.toLowerCase().split('+');
  const key = parts.pop();
  return { key, mod: parts.includes('mod'), ctrl: parts.includes('ctrl'), shift: parts.includes('shift'), alt: parts.includes('alt') };
}

/** One key spec for display: "⇧⌘S" on a Mac, "Ctrl+Shift+S" elsewhere. */
export function keyText(spec) {
  const p = parse(spec);
  if (isMac) {
    const k = MAC_NAMES[p.key] || p.key.toUpperCase();
    return (p.ctrl ? '⌃' : '') + (p.alt ? '⌥' : '') + (p.shift ? '⇧' : '') + (p.mod ? '⌘' : '') + k;
  }
  const k = PC_NAMES[p.key] || p.key.toUpperCase();
  const mods = [];
  if (p.mod || p.ctrl) mods.push('Ctrl');
  if (p.alt) mods.push('Alt');
  if (p.shift) mods.push('Shift');
  return [...mods, k].join('+');
}

/** The first shortcut for a command, for tooltips; '' when it has none. */
export function shortcutText(id) {
  const k = keysFor(byId.get(id));
  return k.length ? keyText(k[0]) : '';
}

/** "Save (⌘S)", or just the label when there's no shortcut. */
export function withShortcut(label, id) {
  const k = shortcutText(id);
  return k ? `${label} (${k})` : label;
}

// ---------------------------------------------------------------------------
// Matching key presses

const NAMED = {
  enter: 'Enter',
  backspace: 'Backspace',
  delete: 'Delete',
  escape: 'Escape',
  tab: 'Tab',
  up: 'ArrowUp',
  down: 'ArrowDown',
  left: 'ArrowLeft',
  right: 'ArrowRight',
  home: 'Home',
  end: 'End',
  f2: 'F2',
  f3: 'F3',
  f10: 'F10',
  space: ' ',
  backslash: '\\',
};

// Physical keys, for when a modifier changes the character (Shift+/ types ?).
const CODES = {
  '[': 'BracketLeft',
  ']': 'BracketRight',
  '/': 'Slash',
  '=': 'Equal',
  '-': 'Minus',
  space: 'Space',
  backslash: 'Backslash',
};

function keyMatches(e, k) {
  if (NAMED[k]) return e.key === NAMED[k] || (CODES[k] !== undefined && e.code === CODES[k]);
  const key = typeof e.key === 'string' ? e.key : '';
  if (/^[a-z]$/.test(k)) {
    // Trust the layout's letter; Option on a Mac turns letters into symbols,
    // so fall back to the physical key then.
    if (/^[a-z]$/i.test(key)) return key.toLowerCase() === k;
    return e.code === `Key${k.toUpperCase()}`;
  }
  if (/^[0-9]$/.test(k)) {
    if (/^[0-9]$/.test(key)) return key === k;
    return e.code === `Digit${k}`;
  }
  return key === k || (CODES[k] !== undefined && e.code === CODES[k]);
}

/** True when a key event is exactly this spec (no extra modifiers). */
export function matches(e, spec) {
  const p = parse(spec);
  if (isMac) {
    if (e.metaKey !== p.mod || e.ctrlKey !== p.ctrl) return false;
  } else if (e.metaKey || e.ctrlKey !== (p.mod || p.ctrl)) {
    return false;
  }
  if (e.shiftKey !== p.shift || e.altKey !== p.alt) return false;
  // AltGr (Ctrl+Alt on Windows) types characters on many layouts; never a shortcut.
  if (typeof e.getModifierState === 'function' && e.getModifierState('AltGraph')) return false;
  return keyMatches(e, p.key);
}

/** The `app` or `grid` command a key event triggers, or null. */
export function matchShortcut(e) {
  if (e.isComposing) return null;
  for (const s of SHORTCUTS) {
    if (s.scope !== 'app' && s.scope !== 'grid') continue;
    if (keysFor(s).some((k) => matches(e, k))) return s;
  }
  return null;
}
