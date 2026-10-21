// Every keyboard shortcut in the app, in one list. The native menu bar
// (src/menu.rs) uses the same ids and keys; a Rust test reads this file and
// fails if the two drift apart, so keep one entry per line.
//
// Keys are written once for both platforms: `mod` is Command on macOS and Ctrl
// on Windows and Linux; `alt` is Option on macOS; `ctrl` is the Control key
// everywhere (so Ctrl on Windows and Linux too). An entry with `mac` and
// `other` instead of `keys` differs by platform; a list gives alternatives
// (the first is the one menus show). Labels are looked up in the string
// table (i18n.js) under `shortcut.<id>`.
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
import { t } from './i18n.js';

export const SHORTCUTS = [
  // File
  { id: 'file.new', group: 'File', label: t('shortcut.file.new'), keys: 'mod+n', scope: 'app' },
  { id: 'file.new_tab', group: 'File', label: t('shortcut.file.new_tab'), keys: 'mod+t', scope: 'app' },
  { id: 'file.open', group: 'File', label: t('shortcut.file.open'), keys: 'mod+o', scope: 'app' },
  { id: 'file.save', group: 'File', label: t('shortcut.file.save'), keys: 'mod+s', scope: 'app' },
  { id: 'file.save_as', group: 'File', label: t('shortcut.file.save_as'), keys: 'mod+shift+s', scope: 'app' },
  { id: 'file.new_from_template', group: 'File', label: t('shortcut.file.new_from_template'), scope: 'menu' },
  { id: 'file.new_sub_budget', group: 'File', label: t('shortcut.file.new_sub_budget'), scope: 'menu' },
  { id: 'file.save_share_copy', group: 'File', label: t('shortcut.file.save_share_copy'), scope: 'menu' },
  { id: 'file.share', group: 'File', label: t('shortcut.file.share'), scope: 'menu' },
  { id: 'file.join_link', group: 'File', label: t('shortcut.file.join_link'), scope: 'menu' },
  { id: 'file.save_as_template', group: 'File', label: t('shortcut.file.save_as_template'), scope: 'menu' },
  { id: 'file.import_setup', group: 'File', label: t('shortcut.file.import_setup'), scope: 'menu' },
  { id: 'file.import_setup_pack', group: 'File', label: t('shortcut.file.import_setup_pack'), scope: 'menu' },
  { id: 'file.export_setup_pack', group: 'File', label: t('shortcut.file.export_setup_pack'), scope: 'menu' },
  { id: 'file.import_spreadsheet', group: 'File', label: t('shortcut.file.import_spreadsheet'), scope: 'menu' },
  { id: 'file.import_chart', group: 'File', label: t('shortcut.file.import_chart'), scope: 'menu' },
  { id: 'file.import_budget_export', group: 'File', label: t('shortcut.file.import_budget_export'), scope: 'menu' },
  { id: 'file.import_schedule_days', group: 'File', label: t('shortcut.file.import_schedule_days'), scope: 'menu' },
  { id: 'file.import_setup_lists', group: 'File', label: t('shortcut.file.import_setup_lists'), scope: 'menu' },
  { id: 'file.import_actuals', group: 'File', label: t('shortcut.file.import_actuals'), scope: 'menu' },
  { id: 'file.versions', group: 'File', label: t('shortcut.file.versions'), scope: 'menu' },
  { id: 'file.compare', group: 'File', label: t('shortcut.file.compare'), scope: 'menu' },
  { id: 'file.what_changed', group: 'File', label: t('shortcut.file.what_changed'), scope: 'menu' },
  { id: 'file.open_for_review', group: 'File', label: t('shortcut.file.open_for_review'), scope: 'menu' },
  { id: 'file.open_review_comments', group: 'File', label: t('shortcut.file.open_review_comments'), scope: 'menu' },
  { id: 'file.merge', group: 'File', label: t('shortcut.file.merge'), scope: 'menu' },
  { id: 'file.print_budget', group: 'File', label: t('shortcut.file.print_budget'), keys: 'mod+p', scope: 'app' },
  { id: 'file.print_topsheet', group: 'File', label: t('shortcut.file.print_topsheet'), keys: 'mod+shift+p', scope: 'app' },
  // File > Export: spreadsheets of the budget (src/export.rs).
  { id: 'file.export_xlsx', group: 'File', label: t('shortcut.file.export_xlsx'), scope: 'menu' },
  { id: 'file.export_csv', group: 'File', label: t('shortcut.file.export_csv'), scope: 'menu' },
  { id: 'file.export_tsv', group: 'File', label: t('shortcut.file.export_tsv'), scope: 'menu' },
  { id: 'file.export_setup_lists', group: 'File', label: t('shortcut.file.export_setup_lists'), scope: 'menu' },
  { id: 'file.close_tab', group: 'File', label: t('shortcut.file.close_tab'), keys: 'mod+w', scope: 'app' },
  { id: 'file.close', group: 'File', label: t('shortcut.file.close'), keys: 'mod+shift+w', scope: 'app' },
  { id: 'app.quit', group: 'File', label: t('shortcut.app.quit'), keys: 'mod+q', scope: 'app' },

  // Edit
  { id: 'edit.undo', group: 'Edit', label: t('shortcut.edit.undo'), keys: 'mod+z', scope: 'app', field: true },
  { id: 'edit.redo', group: 'Edit', label: t('shortcut.edit.redo'), mac: 'mod+shift+z', other: ['mod+y', 'mod+shift+z'], scope: 'app', field: true },
  { id: 'edit.cut', group: 'Edit', label: t('shortcut.edit.cut'), keys: 'mod+x', scope: 'native' },
  { id: 'edit.copy', group: 'Edit', label: t('shortcut.edit.copy'), keys: 'mod+c', scope: 'native' },
  { id: 'edit.paste', group: 'Edit', label: t('shortcut.edit.paste'), keys: 'mod+v', scope: 'native' },
  // Whole rows whatever columns are selected (clipboard.js): lines, accounts
  // on the account list, categories on the topsheet.
  { id: 'edit.cut_rows', group: 'Edit', label: t('shortcut.edit.cut_rows'), scope: 'menu' },
  { id: 'edit.copy_rows', group: 'Edit', label: t('shortcut.edit.copy_rows'), scope: 'menu' },
  { id: 'edit.paste_rows', group: 'Edit', label: t('shortcut.edit.paste_rows'), scope: 'menu' },
  { id: 'edit.select_all', group: 'Edit', label: t('shortcut.edit.select_all'), keys: 'mod+a', scope: 'app', field: true },
  { id: 'edit.fill_down', group: 'Edit', label: t('shortcut.edit.fill_down'), keys: 'mod+shift+d', scope: 'grid' },
  { id: 'edit.delete_rows', group: 'Edit', label: t('shortcut.edit.delete_rows'), keys: 'mod+d', scope: 'grid', text: true },
  // Move the selected rows one place (moverows: lines, accounts, categories
  // and production totals). Text fields keep Option+arrows for the caret.
  { id: 'edit.move_up', group: 'Edit', label: t('shortcut.edit.move_up'), keys: 'alt+up', scope: 'grid', text: true },
  { id: 'edit.move_down', group: 'Edit', label: t('shortcut.edit.move_down'), keys: 'alt+down', scope: 'grid', text: true },
  { id: 'grid.clear', group: 'Edit', label: t('shortcut.grid.clear'), keys: ['backspace', 'delete'], scope: 'builtin' },
  { id: 'edit.note', group: 'Edit', label: t('shortcut.edit.note'), keys: 'mod+shift+space', scope: 'grid' },
  { id: 'edit.make_range', group: 'Edit', label: t('shortcut.edit.make_range'), keys: 'mod+r', scope: 'grid' },
  { id: 'edit.remove_range', group: 'Edit', label: t('shortcut.edit.remove_range'), keys: 'mod+shift+r', scope: 'grid' },
  // How a total is worked out (explain.js) and the overtime helper (overtime.js).
  { id: 'edit.explain', group: 'Edit', label: t('shortcut.edit.explain'), keys: 'mod+e', scope: 'grid' },
  { id: 'edit.overtime', group: 'Edit', label: t('shortcut.edit.overtime'), scope: 'menu' },
  { id: 'edit.fill_rate', group: 'Edit', label: t('shortcut.edit.fill_rate'), scope: 'menu' },
  { id: 'share.blame', group: 'Edit', label: t('shortcut.share.blame'), scope: 'menu' },
  // Find and replace: the Find tab of the tools panel (find.js). Cmd+G is
  // the Globals tab, so the next and previous result are F3 and Shift+F3
  // (and Enter and Shift+Enter in the Find box).
  { id: 'edit.find', group: 'Edit', label: t('shortcut.edit.find'), keys: 'mod+f', scope: 'app' },
  { id: 'edit.replace', group: 'Edit', label: t('shortcut.edit.replace'), keys: 'mod+shift+f', scope: 'app' },
  { id: 'edit.find_next', group: 'Edit', label: t('shortcut.edit.find_next'), keys: 'f3', scope: 'grid', anywhere: true },
  { id: 'edit.find_previous', group: 'Edit', label: t('shortcut.edit.find_previous'), keys: 'shift+f3', scope: 'grid', anywhere: true },
  { id: 'find.keys', group: 'Edit', label: t('shortcut.find.keys'), keys: ['enter', 'shift+enter'], scope: 'builtin' },

  // Moving around and editing cells
  { id: 'grid.move', group: 'Cells', label: t('shortcut.grid.move'), keys: ['up', 'down', 'left', 'right'], scope: 'builtin' },
  { id: 'grid.extend', group: 'Cells', label: t('shortcut.grid.extend'), keys: ['shift+up', 'shift+down', 'shift+left', 'shift+right'], scope: 'builtin' },
  { id: 'grid.tab', group: 'Cells', label: t('shortcut.grid.tab'), keys: ['tab', 'shift+tab'], scope: 'builtin' },
  { id: 'grid.row_ends', group: 'Cells', label: t('shortcut.grid.row_ends'), keys: ['home', 'end'], scope: 'builtin' },
  { id: 'grid.grid_ends', group: 'Cells', label: t('shortcut.grid.grid_ends'), keys: ['mod+home', 'mod+end'], scope: 'builtin' },
  { id: 'grid.edit', group: 'Cells', label: t('shortcut.grid.edit'), keys: 'f2', scope: 'builtin' },
  { id: 'grid.commit', group: 'Cells', label: t('shortcut.grid.commit'), keys: ['enter', 'shift+enter'], scope: 'builtin' },
  { id: 'grid.commit_all', group: 'Cells', label: t('shortcut.grid.commit_all'), keys: 'mod+enter', scope: 'builtin' },
  { id: 'grid.cancel', group: 'Cells', label: t('shortcut.grid.cancel'), keys: 'escape', scope: 'builtin' },
  { id: 'grid.context_menu', group: 'Cells', label: t('shortcut.grid.context_menu'), keys: 'shift+f10', scope: 'builtin' },

  // Insert
  { id: 'insert.line', group: 'Insert', label: t('shortcut.insert.line'), keys: 'mod+i', scope: 'grid' },
  { id: 'insert.subtotal', group: 'Insert', label: t('shortcut.insert.subtotal'), keys: 'mod+=', scope: 'grid' },
  { id: 'insert.adjustment', group: 'Insert', label: t('shortcut.insert.adjustment'), keys: ['mod+-', 'mod+alt+a'], scope: 'grid' },
  { id: 'insert.separator', group: 'Insert', label: t('shortcut.insert.separator'), scope: 'menu' },
  { id: 'insert.account', group: 'Insert', label: t('shortcut.insert.account'), scope: 'menu' },
  { id: 'insert.category', group: 'Insert', label: t('shortcut.insert.category'), scope: 'menu' },
  { id: 'insert.production_total', group: 'Insert', label: t('shortcut.insert.production_total'), scope: 'menu' },

  // Format: text style of the selected detail lines (rowformat.js). Cmd+I
  // inserts a line, so italic is Shift+Cmd+I; Cmd+[ and Cmd+] step accounts,
  // so indenting is Option+Cmd+Right and Left.
  { id: 'format.bold', group: 'Format', label: t('shortcut.format.bold'), keys: 'mod+b', scope: 'grid' },
  { id: 'format.italic', group: 'Format', label: t('shortcut.format.italic'), keys: 'mod+shift+i', scope: 'grid' },
  { id: 'format.indent_more', group: 'Format', label: t('shortcut.format.indent_more'), keys: 'mod+alt+right', scope: 'grid' },
  { id: 'format.indent_less', group: 'Format', label: t('shortcut.format.indent_less'), keys: 'mod+alt+left', scope: 'grid' },
  { id: 'format.color.none', group: 'Format', label: t('shortcut.format.color.none'), scope: 'menu' },
  { id: 'format.color.red', group: 'Format', label: t('shortcut.format.color.red'), scope: 'menu' },
  { id: 'format.color.orange', group: 'Format', label: t('shortcut.format.color.orange'), scope: 'menu' },
  { id: 'format.color.gold', group: 'Format', label: t('shortcut.format.color.gold'), scope: 'menu' },
  { id: 'format.color.green', group: 'Format', label: t('shortcut.format.color.green'), scope: 'menu' },
  { id: 'format.color.teal', group: 'Format', label: t('shortcut.format.color.teal'), scope: 'menu' },
  { id: 'format.color.blue', group: 'Format', label: t('shortcut.format.color.blue'), scope: 'menu' },
  { id: 'format.color.purple', group: 'Format', label: t('shortcut.format.color.purple'), scope: 'menu' },
  { id: 'format.color.pink', group: 'Format', label: t('shortcut.format.color.pink'), scope: 'menu' },
  { id: 'format.clear', group: 'Format', label: t('shortcut.format.clear'), scope: 'menu' },

  // Go
  { id: 'go.topsheet', group: 'Go', label: t('shortcut.go.topsheet'), keys: 'mod+shift+up', scope: 'grid', text: true },
  { id: 'go.up', group: 'Go', label: t('shortcut.go.up'), keys: 'mod+up', scope: 'grid', text: true },
  { id: 'go.down', group: 'Go', label: t('shortcut.go.down'), keys: 'mod+down', scope: 'grid', text: true },
  { id: 'go.next_account', group: 'Go', label: t('shortcut.go.next_account'), keys: ['mod+right', 'mod+]'], scope: 'grid', text: true },
  { id: 'go.prev_account', group: 'Go', label: t('shortcut.go.prev_account'), keys: ['mod+left', 'mod+['], scope: 'grid', text: true },

  // View: the tools panel (tools.js), wherever it is
  { id: 'view.toggle_panel', group: 'View', label: t('shortcut.view.toggle_panel'), keys: 'mod+alt+s', scope: 'app' },
  { id: 'view.panel.right', group: 'View', label: t('shortcut.view.panel.right'), scope: 'menu' },
  { id: 'view.panel.bottom', group: 'View', label: t('shortcut.view.panel.bottom'), scope: 'menu' },
  { id: 'view.panel.window', group: 'View', label: t('shortcut.view.panel.window'), scope: 'menu' },
  // Split view (pane.js): two panes of budget tabs side by side.
  { id: 'view.split_right', group: 'View', label: t('shortcut.view.split_right'), keys: 'mod+shift+backslash', scope: 'app' },
  { id: 'view.close_split', group: 'View', label: t('shortcut.view.close_split'), scope: 'menu' },
  // Display choices, saved for you (display.js). Cmd+= and Cmd+- insert rows,
  // so the zoom keys add Option.
  { id: 'view.formulas', group: 'View', label: t('shortcut.view.formulas'), keys: 'mod+alt+f', scope: 'app' },
  { id: 'view.hide_zero', group: 'View', label: t('shortcut.view.hide_zero'), scope: 'menu' },
  { id: 'view.totals_cents', group: 'View', label: t('shortcut.view.totals_cents'), scope: 'menu' },
  { id: 'view.zoom_in', group: 'View', label: t('shortcut.view.zoom_in'), keys: 'mod+alt+=', scope: 'app' },
  { id: 'view.zoom_out', group: 'View', label: t('shortcut.view.zoom_out'), keys: 'mod+alt+-', scope: 'app' },
  { id: 'view.zoom_reset', group: 'View', label: t('shortcut.view.zoom_reset'), keys: 'mod+alt+0', scope: 'app' },
  // Budget Setup (setupwin.js): the window, and each of its tabs, with the
  // keys long-time users of other budgeting tools have in their hands
  // (decisions.md, entries 15 and 42).
  { id: 'view.setup', group: 'View', label: t('shortcut.view.setup'), scope: 'menu' },
  { id: 'view.tab.fringes', group: 'View', label: t('shortcut.view.tab.fringes'), keys: 'mod+3', scope: 'app' },
  { id: 'view.tab.groups', group: 'View', label: t('shortcut.view.tab.groups'), keys: 'mod+4', scope: 'app' },
  { id: 'view.tab.globals', group: 'View', label: t('shortcut.view.tab.globals'), keys: 'mod+g', scope: 'app' },
  { id: 'view.tab.units', group: 'View', label: t('shortcut.view.tab.units'), keys: 'mod+7', scope: 'app' },
  { id: 'view.tab.sets', group: 'View', label: t('shortcut.view.tab.sets'), keys: 'mod+8', scope: 'app' },
  { id: 'view.tab.locations', group: 'View', label: t('shortcut.view.tab.locations'), keys: 'mod+5', scope: 'app' },
  { id: 'view.tab.currencies', group: 'View', label: t('shortcut.view.tab.currencies'), keys: 'mod+6', scope: 'app' },
  { id: 'view.tab.charges', group: 'View', label: t('shortcut.view.tab.charges'), keys: 'mod+/', scope: 'app' },
  { id: 'view.tab.credits', group: 'View', label: t('shortcut.view.tab.credits'), keys: 'mod+backslash', scope: 'app' },
  { id: 'view.tab.rates', group: 'View', label: t('shortcut.view.tab.rates'), scope: 'menu' },
  { id: 'view.tab.info', group: 'View', label: t('shortcut.view.tab.info'), scope: 'menu' },
  { id: 'view.tab.settings', group: 'View', label: t('shortcut.view.tab.settings'), keys: 'mod+9', scope: 'app' },
  { id: 'view.tab.baseline', group: 'View', label: t('shortcut.view.tab.baseline'), scope: 'menu' },
  // The tools panel's tabs (tools.js): the apply lists and browsing.
  { id: 'view.tool.fringes', group: 'View', label: t('shortcut.view.tool.fringes'), scope: 'menu' },
  { id: 'view.tool.groups', group: 'View', label: t('shortcut.view.tool.groups'), scope: 'menu' },
  { id: 'view.tool.globals', group: 'View', label: t('shortcut.view.tool.globals'), scope: 'menu' },
  { id: 'view.tool.units', group: 'View', label: t('shortcut.view.tool.units'), scope: 'menu' },
  { id: 'view.tab.presets', group: 'View', label: t('shortcut.view.tab.presets'), scope: 'menu' },
  { id: 'view.tool.sets', group: 'View', label: t('shortcut.view.tool.sets'), scope: 'menu' },
  { id: 'view.tool.locations', group: 'View', label: t('shortcut.view.tool.locations'), scope: 'menu' },
  { id: 'view.tool.currencies', group: 'View', label: t('shortcut.view.tool.currencies'), scope: 'menu' },
  { id: 'view.tab.notes', group: 'View', label: t('shortcut.view.tab.notes'), scope: 'menu' },
  { id: 'view.tab.history', group: 'View', label: t('shortcut.view.tab.history'), scope: 'menu' },
  { id: 'view.tab.library', group: 'View', label: t('shortcut.view.tab.library'), scope: 'menu' },
  { id: 'view.tab.actuals', group: 'View', label: t('shortcut.view.tab.actuals'), scope: 'menu' },
  { id: 'view.tab.review', group: 'View', label: t('shortcut.view.tab.review'), scope: 'menu' },
  { id: 'view.tab.comments', group: 'View', label: t('shortcut.view.tab.comments'), scope: 'menu' },
  { id: 'view.tab.activity', group: 'View', label: t('shortcut.view.tab.activity'), scope: 'menu' },
  // The tools panel's tab strips (tools.js).
  { id: 'tools.tab_keys', group: 'View', label: t('shortcut.tools.tab_keys'), keys: ['left', 'right'], scope: 'builtin' },
  { id: 'tools.move_tab', group: 'View', label: t('shortcut.tools.move_tab'), keys: ['alt+shift+left', 'alt+shift+right', 'alt+shift+up', 'alt+shift+down'], scope: 'builtin' },

  // Window: budgets open as tabs (tabs.js). Cmd+3 to Cmd+9 are the tools tabs.
  { id: 'window.next_tab', group: 'Window', label: t('shortcut.window.next_tab'), keys: ['mod+shift+]', 'ctrl+tab'], scope: 'grid', anywhere: true },
  { id: 'window.prev_tab', group: 'Window', label: t('shortcut.window.prev_tab'), keys: ['mod+shift+[', 'ctrl+shift+tab'], scope: 'grid', anywhere: true },
  { id: 'window.first_tab', group: 'Window', label: t('shortcut.window.first_tab'), keys: 'mod+1', scope: 'grid', anywhere: true },
  { id: 'window.second_tab', group: 'Window', label: t('shortcut.window.second_tab'), keys: 'mod+2', scope: 'grid', anywhere: true },
  { id: 'window.move_to_other_pane', group: 'Window', label: t('shortcut.window.move_to_other_pane'), scope: 'menu' },
  { id: 'tabs.keys', group: 'Window', label: t('shortcut.tabs.keys'), keys: ['left', 'right'], scope: 'builtin' },
  { id: 'tabs.move', group: 'Window', label: t('shortcut.tabs.move'), keys: ['alt+shift+left', 'alt+shift+right'], scope: 'builtin' },

  // Help
  { id: 'help.shortcuts', group: 'Help', label: t('shortcut.help.shortcuts'), keys: 'mod+shift+/', scope: 'app' },
  { id: 'help.user_guide', group: 'Help', label: t('shortcut.help.user_guide'), scope: 'menu' },
  { id: 'help.open_sample', group: 'Help', label: t('shortcut.help.open_sample'), scope: 'menu' },
  { id: 'help.release_notes', group: 'Help', label: t('shortcut.help.release_notes'), scope: 'menu' },
  { id: 'help.report_bug', group: 'Help', label: t('shortcut.help.report_bug'), scope: 'menu' },
  { id: 'help.licenses', group: 'Help', label: t('shortcut.help.licenses'), scope: 'menu' },
  { id: 'help.suite_site', group: 'Help', label: t('shortcut.help.suite_site'), scope: 'menu' },
];

/** Groups in the order the shortcut sheet shows them. */
export const GROUPS = ['File', 'Edit', 'Cells', 'Insert', 'Format', 'Go', 'View', 'Window', 'Help'];

/** Each group's heading on the shortcut sheet. */
export const GROUP_NAMES = Object.freeze({
  File: t('shortcut.group.file'),
  Edit: t('shortcut.group.edit'),
  Cells: t('shortcut.group.cells'),
  Insert: t('shortcut.group.insert'),
  Format: t('shortcut.group.format'),
  Go: t('shortcut.group.go'),
  View: t('shortcut.group.view'),
  Window: t('shortcut.group.window'),
  Help: t('shortcut.group.help'),
});

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
