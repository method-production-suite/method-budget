// Every keyboard shortcut in the app, in one list. The native menu bar
// (src/menu.rs) uses the same ids and keys; a Rust test reads this file and
// fails if the two drift apart, so keep one entry per line.
//
// Keys are written once for both platforms: `mod` is Command on macOS and Ctrl
// on Windows and Linux; `alt` is Option on macOS. An entry with `mac` and
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

import { isMac } from './dom.js';

export const SHORTCUTS = [
  // File
  { id: 'file.new', group: 'File', label: 'New budget', keys: 'mod+n', scope: 'app' },
  { id: 'file.open', group: 'File', label: 'Open a budget', keys: 'mod+o', scope: 'app' },
  { id: 'file.save', group: 'File', label: 'Save', keys: 'mod+s', scope: 'app' },
  { id: 'file.save_as', group: 'File', label: 'Save as a new file', keys: 'mod+shift+s', scope: 'app' },
  { id: 'file.print_budget', group: 'File', label: 'Print the budget (topsheet and detail)', keys: 'mod+p', scope: 'app' },
  { id: 'file.print_topsheet', group: 'File', label: 'Print the topsheet', keys: 'mod+shift+p', scope: 'app' },
  { id: 'file.close', group: 'File', label: 'Close the window', keys: 'mod+w', scope: 'app' },
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
  { id: 'grid.clear', group: 'Edit', label: 'Clear the selected cells', keys: ['backspace', 'delete'], scope: 'builtin' },
  { id: 'edit.note', group: 'Edit', label: 'Add or edit the note on the line', keys: 'mod+shift+space', scope: 'grid' },
  { id: 'edit.make_range', group: 'Edit', label: 'Make the selected lines one fringe range', keys: 'mod+r', scope: 'grid' },
  { id: 'edit.remove_range', group: 'Edit', label: 'Take the selected lines out of their fringe range', keys: 'mod+shift+r', scope: 'grid' },

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

  // View
  { id: 'view.toggle_panel', group: 'View', label: 'Show or hide the setup panel', keys: 'mod+alt+s', scope: 'app' },
  // Setup tabs take the keys long-time users of other budgeting tools have in
  // their hands (decisions.md, entry 15).
  { id: 'view.tab.globals', group: 'View', label: 'Setup: Globals', keys: 'mod+g', scope: 'app' },
  { id: 'view.tab.fringes', group: 'View', label: 'Setup: Fringes', keys: 'mod+3', scope: 'app' },
  { id: 'view.tab.groups', group: 'View', label: 'Setup: Groups', keys: 'mod+4', scope: 'app' },
  { id: 'view.tab.units', group: 'View', label: 'Setup: Units', keys: 'mod+7', scope: 'app' },
  { id: 'view.tab.currencies', group: 'View', label: 'Setup: Currencies', keys: 'mod+6', scope: 'app' },
  { id: 'view.tab.sets', group: 'View', label: 'Setup: Sets', keys: 'mod+8', scope: 'app' },
  { id: 'view.tab.locations', group: 'View', label: 'Setup: Locations', keys: 'mod+5', scope: 'app' },
  { id: 'view.tab.charges', group: 'View', label: 'Setup: Charges', keys: 'mod+/', scope: 'app' },
  { id: 'view.tab.credits', group: 'View', label: 'Setup: Credits', keys: 'mod+backslash', scope: 'app' },
  { id: 'view.tab.info', group: 'View', label: 'Setup: Info', scope: 'menu' },
  { id: 'view.tab.settings', group: 'View', label: 'Setup: Settings (budget preferences)', keys: 'mod+9', scope: 'app' },
  { id: 'view.tab.baseline', group: 'View', label: 'Setup: Baseline', scope: 'menu' },

  // Help
  { id: 'help.shortcuts', group: 'Help', label: 'Show this list of shortcuts', keys: 'mod+shift+/', scope: 'app' },
];

/** Groups in the order the shortcut sheet shows them. */
export const GROUPS = ['File', 'Edit', 'Cells', 'Insert', 'Format', 'Go', 'View', 'Help'];

/** On macOS the native menu answers `app` keys; elsewhere main.js does. */
export const menuRunsAppKeys = isMac;

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
  f10: 'F10',
  space: 'Space',
  backslash: '\\',
};

function parse(spec) {
  const parts = spec.toLowerCase().split('+');
  const key = parts.pop();
  return { key, mod: parts.includes('mod'), shift: parts.includes('shift'), alt: parts.includes('alt') };
}

/** One key spec for display: "⇧⌘S" on a Mac, "Ctrl+Shift+S" elsewhere. */
export function keyText(spec) {
  const p = parse(spec);
  if (isMac) {
    const k = MAC_NAMES[p.key] || p.key.toUpperCase();
    return (p.alt ? '⌥' : '') + (p.shift ? '⇧' : '') + (p.mod ? '⌘' : '') + k;
  }
  const k = PC_NAMES[p.key] || p.key.toUpperCase();
  const mods = [];
  if (p.mod) mods.push('Ctrl');
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
  const mod = isMac ? e.metaKey : e.ctrlKey;
  const stray = isMac ? e.ctrlKey : e.metaKey;
  if (stray || mod !== p.mod || e.shiftKey !== p.shift || e.altKey !== p.alt) return false;
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
