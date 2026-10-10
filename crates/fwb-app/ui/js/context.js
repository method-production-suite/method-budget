// The grid's right-click menu, at all three levels: detail lines, the account
// list and the topsheet.
//
// A right-click on a cell moves the cursor there (a click inside a selection
// of several rows keeps the selection) and opens the menu at the mouse;
// Shift+F10 or the keyboard's menu key opens it at the cursor cell. Items run
// the same commands as the menu bar and their keys, by shortcut-list id
// through main.js, and show those keys. The menu itself is menu.js's
// openContextMenu (arrows, Enter, Escape, submenus).
//
// A right-click inside the cell editor is left to the system's own text menu.

import { openContextMenu, isContextMenuOpen } from './menu.js';
import { shortcutText } from './shortcuts.js';
import { isModalOpen } from './modal.js';
import { isHelpOpen } from './help.js';
import { PALETTE, paletteColor, selectedDetails, styleState } from './rowformat.js';
import { presetMenu, blockMenu } from './presets.js';

// { grid, host, view(), level(), run(id), act(cmd, args), toast(message) }
// `host` is the grid's host element, or a function giving the one in use
// (split view has a grid per pane; main.js points `grid` at the target's).
let ctx = null;

const SEP = { separator: true };

export function initContextMenu(context) {
  ctx = context;
  watchContextMenu(host());
  window.addEventListener('keydown', onKey);
}

/** Opens the menu on right-clicks in another grid host too (split view's second pane). */
export function watchContextMenu(el) {
  if (el) el.addEventListener('contextmenu', onContextMenu);
}

/** The grid host in use. */
function host() {
  return typeof ctx.host === 'function' ? ctx.host() : ctx.host;
}

// { selection(), call(cmd, args, opts) } for the preset submenus (main.js).
let presets = null;

export function initContextMenuPresets(context) {
  presets = context;
}

/**
 * Apply preset > (every level) and Insert line block > (detail lines): the
 * submenus presets.js builds, for the grid's selection.
 */
function presetItems(blocks) {
  if (!presets) return [];
  const c = {
    view: ctx.view(),
    sel: presets.selection(),
    call: presets.call,
    toast: ctx.toast,
    anchor: () => ctx.grid.cellFor(ctx.grid.cursorKey(), ctx.grid.cursorColKey()) || host(),
    openTab: () => ctx.run('view.tab.presets'),
  };
  const out = [SEP, { label: 'Apply preset', items: presetMenu(c) }];
  if (blocks) out.push({ label: 'Insert line block', items: blockMenu(c) });
  return out;
}

// ---------------------------------------------------------------------------
// Opening

function onContextMenu(e) {
  if (!ctx) return;
  const grid = ctx.grid;
  // The cell editor keeps the system's text menu (cut, copy, paste, spelling).
  if (grid.editor && e.target === grid.editor.input) return;
  e.preventDefault();
  if (isModalOpen() || isHelpOpen() || isContextMenuOpen()) return;
  const h = grid.hit(e);
  if (h) {
    if (grid.editor) grid.commitEdit(null);
    const keys = grid.selectedKeys();
    if (!(keys.length > 1 && keys.includes(h.row.key))) grid.setCursor(h.nav, h.col);
  }
  grid.focus();
  show({ x: e.clientX, y: e.clientY }, Boolean(h));
}

function onKey(e) {
  if (!ctx || e.defaultPrevented || e.isComposing) return;
  const plain = !e.metaKey && !e.ctrlKey && !e.altKey;
  const menuKey = e.key === 'ContextMenu' && plain && !e.shiftKey;
  const shiftF10 = e.key === 'F10' && plain && e.shiftKey;
  if (!menuKey && !shiftF10) return;
  if (isModalOpen() || isHelpOpen() || isContextMenuOpen()) return;
  const a = document.activeElement;
  const inGrid = a && host().contains(a) && !(a.closest && a.closest('thead'));
  if (!inGrid && a && a !== document.body) return;
  e.preventDefault();
  openAtCursor();
}

/** Opens the menu at the cursor cell (the keyboard's way in). */
export function openAtCursor() {
  if (!ctx) return;
  const grid = ctx.grid;
  if (grid.editor) grid.commitEdit(null);
  grid.focus();
  const box = host().getBoundingClientRect();
  const td = grid.cellFor(grid.cursorKey(), grid.cursorColKey());
  let x = box.left + 24;
  let y = box.top + 40;
  if (td) {
    const r = td.getBoundingClientRect();
    x = r.left + Math.min(24, r.width / 2);
    y = r.bottom - 2;
  }
  x = Math.max(box.left + 4, Math.min(x, box.right - 8));
  y = Math.max(box.top + 24, Math.min(y, box.bottom - 8));
  show({ x, y }, grid.cursorRow() !== null);
}

function show(at, onRow) {
  const level = ctx.level();
  let items;
  let label;
  if (level === 'details') {
    items = detailItems(onRow);
    label = 'Line actions';
  } else if (level === 'accounts') {
    items = accountItems(onRow);
    label = 'Account actions';
  } else {
    items = topsheetItems(onRow);
    label = 'Topsheet actions';
  }
  openContextMenu(at, { label, items: tidy(items), returnFocus: () => ctx.grid.focus() });
}

/** Drops empty entries and separators at either end or next to each other. */
function tidy(items) {
  const out = [];
  for (const it of items) {
    if (!it) continue;
    if (it.separator && (!out.length || out[out.length - 1].separator)) continue;
    out.push(Array.isArray(it.items) ? { ...it, items: tidy(it.items) } : it);
  }
  while (out.length && out[out.length - 1].separator) out.pop();
  return out;
}

// ---------------------------------------------------------------------------
// Items

/** An item that runs a shortcut-list command, showing its key. */
function cmd(id, label, extra = {}) {
  return { label, shortcut: shortcutText(id), run: () => ctx.run(id), ...extra };
}

/**
 * Cut, Copy and Paste: the same clipboard events as the keys, so they go
 * through the grid's own handlers in main.js. A web view may refuse to paste
 * from a menu without the key; then a message says to use the key.
 */
function clip(kind) {
  ctx.grid.focus();
  let ok = false;
  try {
    ok = document.execCommand(kind);
  } catch {
    ok = false;
  }
  if (!ok) ctx.toast(`Use ${shortcutText(`edit.${kind}`)} to ${kind} here.`);
}

function clipboardItems() {
  return [
    { label: 'Cut', shortcut: shortcutText('edit.cut'), run: () => clip('cut') },
    { label: 'Copy', shortcut: shortcutText('edit.copy'), run: () => clip('copy') },
    { label: 'Paste', shortcut: shortcutText('edit.paste'), run: () => clip('paste') },
    SEP,
    // Whole rows with everything attached (clipboard.js); work across budgets.
    cmd('edit.cut_rows', 'Cut rows'),
    cmd('edit.copy_rows', 'Copy rows'),
    cmd('edit.paste_rows', 'Paste rows'),
  ];
}

/** Starts editing the cursor row's name in place (the account list and topsheet). */
function rename() {
  const grid = ctx.grid;
  const key = grid.cursorKey();
  if (key === null) return;
  grid.goTo(key, 'name');
  grid.startEdit(null);
}

function kindOf(d) {
  return d.kind || 'line';
}

function tri(state) {
  return state === 'all' ? true : state === 'some' ? 'mixed' : false;
}

function byteOrder(a, b) {
  return a < b ? -1 : a > b ? 1 : 0;
}

function formatItems(s) {
  return [
    cmd('format.bold', 'Bold', { checked: tri(s.bold) }),
    cmd('format.italic', 'Italic', { checked: tri(s.italic) }),
    SEP,
    cmd('format.indent_more', 'Indent more', { disabled: !s.canIndent }),
    cmd('format.indent_less', 'Indent less', { disabled: !s.canOutdent }),
    SEP,
    {
      label: 'Text color',
      items: [
        cmd('format.color.none', 'No color', { swatch: 'none', checked: s.color === '', title: 'The line takes its group color, if it has one' }),
        SEP,
        ...PALETTE.map((c) => cmd(`format.color.${c.id}`, c.label, { swatch: c.id, checked: s.color === c.light })),
      ],
    },
    SEP,
    cmd('format.clear', 'Clear formatting', { disabled: !s.styled }),
  ];
}

/**
 * Apply group > or Apply fringe >: every defined item, checked when every
 * selected row that can take it has it (a dash when some do). Choosing one
 * puts it on them all, or takes it off when they all have it, as the apply
 * panel does.
 */
function tagItems(field, rows) {
  const b = (ctx.view() && ctx.view().budget) || {};
  const defs = b[field] && typeof b[field] === 'object' ? b[field] : {};
  const takers = rows.filter((d) => (field === 'fringes' ? kindOf(d) === 'line' : kindOf(d) === 'line' || kindOf(d) === 'adjustment'));
  const what = field === 'fringes' ? 'fringes' : 'groups';
  const out = Object.keys(defs)
    .sort(byteOrder)
    .map((name) => {
      const def = defs[name] && typeof defs[name] === 'object' ? defs[name] : {};
      const n = takers.filter((d) => Array.isArray(d[field]) && d[field].includes(name)).length;
      const state = !n ? false : n === takers.length ? true : 'mixed';
      const color = field === 'groups' ? paletteColor(def.color) : null;
      return {
        label: name,
        checked: state,
        disabled: !takers.length,
        title: typeof def.description === 'string' && def.description ? def.description : undefined,
        swatch: color ? color.id : undefined,
        run: () => ctx.act('apply_to_rows', { rowIds: rows.map((d) => d.id), field, name, on: state !== true }),
      };
    });
  if (!out.length) out.push({ label: `No ${what} yet`, disabled: true });
  out.push(SEP, cmd(field === 'fringes' ? 'view.tab.fringes' : 'view.tab.groups', `Set up ${what}…`));
  return out;
}

/** Move up and Move down: the selected rows one place (src/moverows.rs). */
function moveItems() {
  return [SEP, cmd('edit.move_up', 'Move up'), cmd('edit.move_down', 'Move down')];
}

function detailItems(onRow) {
  const inserts = [
    cmd('insert.line', 'Insert line'),
    cmd('insert.subtotal', 'Insert subtotal'),
    cmd('insert.adjustment', 'Insert adjustment'),
    cmd('insert.separator', 'Insert separator'),
  ];
  if (!onRow) return inserts;
  const rows = selectedDetails();
  const costed = rows.filter((d) => kindOf(d) === 'line' || kindOf(d) === 'adjustment');
  return [
    ...clipboardItems(),
    SEP,
    ...inserts,
    cmd('edit.delete_rows', rows.length > 1 ? `Delete ${rows.length} rows` : 'Delete row'),
    ...moveItems(),
    SEP,
    { label: 'Format', items: formatItems(styleState(rows)) },
    cmd('edit.note', 'Add note…'),
    SEP,
    cmd('edit.make_range', 'Make fringe range', {
      disabled: costed.length < 2,
      title: 'Percent fringe cutoffs apply to the selected lines together',
    }),
    cmd('edit.remove_range', 'Remove fringe range', { disabled: !rows.some((d) => typeof d.fringe_range === 'number') }),
    { label: 'Apply group', items: tagItems('groups', rows) },
    { label: 'Apply fringe', items: tagItems('fringes', rows) },
    ...presetItems(true),
  ];
}

function accountItems(onRow) {
  const insert = cmd('insert.account', 'Insert account…');
  if (!onRow) return [insert];
  return [
    ...clipboardItems(),
    SEP,
    cmd('go.down', 'Open account'),
    SEP,
    insert,
    { label: 'Rename account', run: rename },
    cmd('edit.note', 'Add note…'),
    cmd('edit.delete_rows', 'Delete account'),
    ...moveItems(),
    ...presetItems(false),
  ];
}

function topsheetItems(onRow) {
  const inserts = [cmd('insert.category', 'Insert category…'), cmd('insert.production_total', 'Insert production total…')];
  const setup = [
    cmd('budget.create_charge', 'Create charge…', { title: 'Open the charges tab with a new charge ready to name' }),
    cmd('budget.create_credit', 'Create credit…', { title: 'Open the credits tab with a new credit ready to name' }),
    cmd('view.tab.charges', 'Add or apply a charge…', { title: 'Open the charges tab: add a charge, or turn one on or off' }),
    cmd('view.tab.credits', 'Add or apply a credit…', { title: 'Open the credits tab: add a credit, or turn one on or off' }),
  ];
  const row = onRow ? ctx.grid.cursorRow() : null;
  if (!row) return [...inserts, SEP, ...setup];
  const category = row.kind === 'category';
  return [
    ...clipboardItems(),
    SEP,
    category ? cmd('go.down', 'Open category') : null,
    SEP,
    ...inserts,
    { label: category ? 'Rename category' : 'Rename production total', run: rename },
    category ? cmd('edit.note', 'Add note…') : null,
    cmd('edit.delete_rows', category ? 'Delete category…' : 'Delete production total'),
    ...moveItems(),
    SEP,
    ...setup,
    ...(category ? presetItems(false) : []),
  ];
}
