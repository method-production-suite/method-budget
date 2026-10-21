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
import { t } from './i18n.js';

// { grid, host, view(), level(), run(id), act(cmd, args), toast(message),
//   explainable(row, colKey), explain(row, colKey) }
// `host` is the grid's host element, or a function giving the one in use
// (split view has a grid per pane; main.js points `grid` at the target's).
// explainable and explain: whether a cell shows a total Explain This
// Number… can explain, and explaining it (explain.js, through main.js).
let ctx = null;

// The cell the menu was opened on ({ row, colKey }, any row, a total row
// too), for Explain This Number…
let menuCell = null;

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
  const out = [SEP, { label: t('context.apply_preset'), items: presetMenu(c) }];
  if (blocks) out.push({ label: t('context.insert_line_block'), items: blockMenu(c) });
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
  menuCell = typeof grid.peek === 'function' ? grid.peek(e) : null;
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
  const row = grid.cursorRow();
  menuCell = row ? { row, colKey: grid.cursorColKey() } : null;
  show({ x, y }, row !== null);
}

function show(at, onRow) {
  const level = ctx.level();
  let items;
  let label;
  if (level === 'details') {
    items = detailItems(onRow);
    label = t('context.line_actions');
  } else if (level === 'accounts') {
    items = accountItems(onRow);
    label = t('context.account_actions');
  } else {
    items = topsheetItems(onRow);
    label = t('context.topsheet_actions');
  }
  items = [...explainItems(), ...blameItems(), ...items];
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
  if (ok) return;
  const key = shortcutText(`edit.${kind}`);
  if (kind === 'cut') ctx.toast(t('context.use_key_to_cut', { key }));
  else if (kind === 'copy') ctx.toast(t('context.use_key_to_copy', { key }));
  else ctx.toast(t('context.use_key_to_paste', { key }));
}

function clipboardItems() {
  return [
    { label: t('context.cut'), shortcut: shortcutText('edit.cut'), run: () => clip('cut') },
    { label: t('context.copy'), shortcut: shortcutText('edit.copy'), run: () => clip('copy') },
    { label: t('context.paste'), shortcut: shortcutText('edit.paste'), run: () => clip('paste') },
    SEP,
    // Whole rows with everything attached (clipboard.js); work across budgets.
    cmd('edit.cut_rows', t('context.cut_rows')),
    cmd('edit.copy_rows', t('context.copy_rows')),
    cmd('edit.paste_rows', t('context.paste_rows')),
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
    cmd('format.bold', t('context.bold'), { checked: tri(s.bold) }),
    cmd('format.italic', t('context.italic'), { checked: tri(s.italic) }),
    SEP,
    cmd('format.indent_more', t('context.indent_more'), { disabled: !s.canIndent }),
    cmd('format.indent_less', t('context.indent_less'), { disabled: !s.canOutdent }),
    SEP,
    {
      label: t('context.text_color'),
      items: [
        cmd('format.color.none', t('context.no_color'), { swatch: 'none', checked: s.color === '', title: t('context.no_color_tip') }),
        SEP,
        ...PALETTE.map((c) => cmd(`format.color.${c.id}`, c.label, { swatch: c.id, checked: s.color === c.light })),
      ],
    },
    SEP,
    cmd('format.clear', t('context.clear_formatting'), { disabled: !s.styled }),
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
  // Lines and adjustment rows take fringes and groups (decisions.md, entry 56).
  const takers = rows.filter((d) => kindOf(d) === 'line' || kindOf(d) === 'adjustment');
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
  if (!out.length) out.push({ label: field === 'fringes' ? t('context.no_fringes_yet') : t('context.no_groups_yet'), disabled: true });
  out.push(SEP, cmd(field === 'fringes' ? 'view.tab.fringes' : 'view.tab.groups', field === 'fringes' ? t('context.set_up_fringes') : t('context.set_up_groups')));
  return out;
}

/**
 * Explain This Number… first, when the menu's cell shows a total (any row,
 * total rows included): how it is worked out, in the Explain tab.
 */
function explainItems() {
  const c = menuCell;
  if (!c || typeof ctx.explainable !== 'function' || !ctx.explainable(c.row, c.colKey)) return [];
  return [
    cmd('edit.explain', t('context.explain'), {
      title: t('context.explain_tip'),
      run: () => ctx.explain(c.row, c.colKey),
    }),
    SEP,
  ];
}

/** Who Changed This… on a cell of a shared budget (share.js). */
function blameItems() {
  const c = menuCell;
  if (!c || typeof ctx.blameable !== 'function' || !ctx.blameable(c.row, c.colKey)) return [];
  return [
    {
      label: t('context.blame'),
      title: t('context.blame_tip'),
      run: () => ctx.blame(c.row, c.colKey),
    },
    SEP,
  ];
}

/** Move up and Move down: the selected rows one place (src/moverows.rs). */
function moveItems() {
  return [SEP, cmd('edit.move_up', t('context.move_up')), cmd('edit.move_down', t('context.move_down'))];
}

function detailItems(onRow) {
  const inserts = [
    cmd('insert.line', t('context.insert_line')),
    cmd('insert.subtotal', t('context.insert_subtotal')),
    cmd('insert.adjustment', t('context.insert_adjustment')),
    cmd('insert.separator', t('context.insert_separator')),
  ];
  if (!onRow) return inserts;
  const rows = selectedDetails();
  const costed = rows.filter((d) => kindOf(d) === 'line' || kindOf(d) === 'adjustment');
  return [
    ...clipboardItems(),
    SEP,
    ...inserts,
    cmd('edit.delete_rows', rows.length > 1 ? t('context.delete_rows', { count: rows.length }) : t('context.delete_row')),
    ...moveItems(),
    SEP,
    { label: t('context.format'), items: formatItems(styleState(rows)) },
    cmd('edit.note', t('context.add_note')),
    SEP,
    cmd('edit.make_range', t('context.make_range'), {
      disabled: costed.length < 2,
      title: t('context.make_range_tip'),
    }),
    cmd('edit.remove_range', t('context.remove_range'), { disabled: !rows.some((d) => typeof d.fringe_range === 'number') }),
    { label: t('context.apply_group'), items: tagItems('groups', rows) },
    { label: t('context.apply_fringe'), items: tagItems('fringes', rows) },
    ...presetItems(true),
    SEP,
    cmd('edit.overtime', t('context.overtime'), {
      disabled: !rows.some((d) => kindOf(d) === 'line'),
      title: t('context.overtime_tip'),
    }),
    ...rateItems(rows),
  ];
}

/**
 * Fill rate from table, and Update rate when the line's table now says
 * something else (ratepick.js; decisions.md, entry 90).
 */
function rateItems(rows) {
  const line = rows.length === 1 && kindOf(rows[0]) === 'line' ? rows[0] : null;
  const items = [
    cmd('edit.fill_rate', t('context.fill_rate'), {
      disabled: !line,
      title: t('context.fill_rate_tip'),
    }),
  ];
  const view = typeof ctx.view === 'function' ? ctx.view() : null;
  const check = line && view && view.rate_checks ? view.rate_checks[line.id] : null;
  if (check && check.state === 'changed') {
    items.push({
      label: t('context.update_rate', { rate: check.now }),
      title: t('context.update_rate_tip'),
      run: () => ctx.act('rate_update', { row_ids: [line.id] }),
    });
  }
  return items;
}

function accountItems(onRow) {
  const insert = cmd('insert.account', t('context.insert_account'));
  if (!onRow) return [insert];
  return [
    ...clipboardItems(),
    SEP,
    cmd('go.down', t('context.open_account')),
    SEP,
    insert,
    { label: t('context.rename_account'), run: rename },
    cmd('edit.note', t('context.add_note')),
    cmd('edit.delete_rows', t('context.delete_account')),
    ...moveItems(),
    SEP,
    cmd('file.sub_budget_from_row', t('context.sub_budget_account'), {
      title: t('context.sub_budget_account_tip'),
    }),
    ...presetItems(false),
  ];
}

function topsheetItems(onRow) {
  const inserts = [cmd('insert.category', t('context.insert_category')), cmd('insert.production_total', t('context.insert_production_total'))];
  const setup = [
    cmd('budget.create_charge', t('context.create_charge'), { title: t('context.create_charge_tip') }),
    cmd('budget.create_credit', t('context.create_credit'), { title: t('context.create_credit_tip') }),
    cmd('view.tab.charges', t('context.apply_charge'), { title: t('context.apply_charge_tip') }),
    cmd('view.tab.credits', t('context.apply_credit'), { title: t('context.apply_credit_tip') }),
  ];
  const row = onRow ? ctx.grid.cursorRow() : null;
  if (!row) return [...inserts, SEP, ...setup];
  const category = row.kind === 'category';
  return [
    ...clipboardItems(),
    SEP,
    category ? cmd('go.down', t('context.open_category')) : null,
    SEP,
    ...inserts,
    { label: category ? t('context.rename_category') : t('context.rename_production_total'), run: rename },
    category ? cmd('edit.note', t('context.add_note')) : null,
    cmd('edit.delete_rows', category ? t('context.delete_category') : t('context.delete_production_total')),
    ...moveItems(),
    SEP,
    category
      ? cmd('file.sub_budget_from_row', t('context.sub_budget_category'), {
          title: t('context.sub_budget_category_tip'),
        })
      : null,
    ...setup,
    ...(category ? presetItems(false) : []),
  ];
}
