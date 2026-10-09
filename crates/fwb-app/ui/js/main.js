// App shell: holds the current view, which level is showing, the toolbar,
// breadcrumb, keyboard shortcuts and menu commands, and wires the grid, side
// panel and footer.
//
// Rust owns the budget. Every change is a command that returns a fresh view;
// this file only decides what to ask for and redraws from what comes back.
// The view is lean: the budget's outline and totals, plus the rows of the one
// account on screen (focus_account picks it). Printing asks for everything.
//
// Every keyboard shortcut and menu item runs a command from COMMANDS by id
// (ids and keys: shortcuts.js). A key press and a menu click reach the same
// function, so they always do the same thing.

import { invoke, connected } from './api.js';
import { el, $, isTextField } from './dom.js';
import { Grid, parseTSV } from './grid.js';
import {
  indexView,
  findCategory,
  accountList,
  TOPSHEET_COLUMNS,
  accountColumns,
  detailColumns,
  columnShown,
  OPTIONAL_DETAIL_COLUMNS,
  topsheetRows,
  accountRows,
  detailRows,
} from './levels.js';
import { renderFooter } from './footer.js';
import { formDialog, confirmDialog, isModalOpen } from './modal.js';
import { renderSetup } from './setup.js';
import { printReport } from './report.js';
import { SHORTCUTS, matchShortcut, menuRunsAppKeys, shortcutById, withShortcut } from './shortcuts.js';
import { isHelpOpen, toggleHelp, closeHelp } from './help.js';
import { openMenu, updateMenu, closeMenu, isMenuOpen, setMenuAnchor } from './menu.js';
import { readPref, writePref } from './prefs.js';
import { setRules, dynamicStyles } from './sheet.js';

const state = {
  view: null,
  ix: null,
  level: 'topsheet', // 'topsheet' | 'accounts' | 'details'
  cat: null, // { code, ti }
  acct: null, // { code, ai }
  tab: 'apply', // side panel tab; 'apply' is the apply panel (apply.js)
  panelOpen: true,
  selection: null, // { count, totals } on the detail grid
  memory: {}, // level -> row key to return to
  columns: readColumnChoices(), // optional detail column -> shown (saved per user)
  // Net totals for the footer's change counters: when the file was opened or
  // created (start), after the latest command (prev), and the latest edit's effect.
  changes: null, // { start, prev, last }
};

const grid = new Grid($('grid-host'), {
  onCommit: (rowKey, colKey, value) => commitCell(rowKey, colKey, value),
  onCommitMany: (rowKeys, colKey, value) => commitMany(rowKeys, colKey, value),
  onActivate: (rowKey) => drillDown(rowKey),
  onEscape: () => goUp(),
  onSelect: (keys) => selectionChanged(keys),
  onClearCells: (cells) => clearCells(cells),
  onPastEnd: () => {
    if (state.level === 'details') insertRow('line');
  },
  // Shift+Tab from the first cell reaches the column menu button.
  onTabOut: () => {
    const b = $('grid-host').querySelector('.col-chooser');
    if (b) b.focus();
    return Boolean(b);
  },
  enterMovesDown: false,
});

// ---------------------------------------------------------------------------
// Talking to Rust

/** Runs a command and redraws from the view it returns. Rejects with a message. */
async function call(cmd, args = {}, opts = {}) {
  const v = await invoke(cmd, args);
  if (v && typeof v === 'object' && 'budget' in v) apply(v, { ...opts, cmd });
  return v;
}

/** Like call(), but problems become a toast instead of a rejection. */
async function act(cmd, args = {}, opts = {}) {
  try {
    return await call(cmd, args, opts);
  } catch (e) {
    toast(e);
    return null;
  }
}

function apply(view, opts = {}) {
  const same = state.view && state.view.rev === view.rev;
  state.view = view;
  state.ix = indexView(view);
  trackChanges(view, opts);
  // Leaving an account (focus_account with none) changes nothing on screen.
  if (opts.quiet && same) return;
  render(opts);
  // The selected rows' totals may have changed even if the selection didn't.
  if (state.level === 'details') requestSelectionTotals(grid.selectedKeys());
}

// Commands that return a view without changing the budget.
const NOT_EDITS = new Set(['get_view', 'save_budget', 'focus_account']);

/**
 * Footer change counters. `opts.fresh` (a file opened or created) starts
 * over; every other command that returns a view is an edit, and its effect on
 * the net total becomes the last change (0 for an edit that moves no money).
 */
function trackChanges(view, opts) {
  const net = view.report ? view.report.net_total : null;
  if (opts.fresh || !state.changes) {
    state.changes = { start: net, prev: net, last: 0 };
    return;
  }
  const c = state.changes;
  if (net === null) return; // no totals right now; compare with the last ones known
  if (c.start === null) {
    c.start = net; // the first totals since the file was opened
    c.prev = net;
    return;
  }
  if (!NOT_EDITS.has(opts.cmd)) c.last = net - c.prev;
  c.prev = net;
}

let toastTimer = null;
function toast(message) {
  const t = $('toast');
  t.textContent = String(message);
  t.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => {
    t.hidden = true;
  }, 6000);
}

// ---------------------------------------------------------------------------
// Where we are

/** Resolves the current category and account, stepping up a level if they're gone. */
function resolveNav() {
  const ix = state.ix;
  let entry = null;
  let ai = -1;
  if (state.level !== 'topsheet') {
    entry = state.cat ? findCategory(ix, state.cat.code, state.cat.ti) : null;
    if (!entry) {
      state.level = 'topsheet';
    } else {
      state.cat = { code: entry.cat.code, ti: entry.ti };
    }
  }
  if (state.level === 'details') {
    // The view carries the account on screen; Rust finds it the same way
    // (codes first), so its place is the one to show.
    const acct = state.view.account;
    const accts = entry.cat.accounts || [];
    if (!acct || acct.ti !== entry.ti || !accts[acct.ai] || accts[acct.ai].code !== acct.code) {
      state.level = 'accounts';
    } else {
      ai = acct.ai;
      state.acct = { code: acct.code, ai };
    }
  }
  return { entry, ai };
}

function render(opts = {}) {
  const v = state.view;
  if (!v) return;
  const { entry, ai } = resolveNav();
  renderTitle(v);
  renderCrumbs(entry, ai);
  renderBanner(v);
  renderActions();

  const ix = state.ix;
  $('grid-host').dataset.level = state.level;
  if (state.level === 'topsheet') {
    grid.h.enterMovesDown = false;
    grid.setData(TOPSHEET_COLUMNS, topsheetRows(ix), opts);
  } else if (state.level === 'accounts') {
    grid.h.enterMovesDown = false;
    grid.setData(accountColumns(ix), accountRows(ix, entry), opts);
  } else {
    grid.h.enterMovesDown = true;
    const rows = detailRows(ix, entry, ai, v.account);
    if (opts.focusFirst) {
      const first = rows.find((r) => r.selectable !== false);
      opts = { ...opts, focusKey: first ? first.key : null };
    }
    grid.setData(gridDetailColumns(), rows, opts);
  }
  renderPanel();
  renderFooterNow();
}

function renderTitle(v) {
  $('doc-name').textContent = v.file.name;
  $('doc-name').title = v.file.path || 'Not saved yet';
  $('doc-dirty').hidden = !v.file.dirty;
  $('btn-undo').disabled = !v.can_undo;
  $('btn-redo').disabled = !v.can_redo;
}

function crumb(label, onClick, current) {
  if (current) return el('span', { class: 'crumb current', text: label, title: label, attrs: { 'aria-current': 'page' } });
  return el('button', { class: 'crumb', text: label, title: label, attrs: { type: 'button' }, on: { click: onClick } });
}

/** A breadcrumb segment's jump list: a small button that opens a menu of its siblings. */
function crumbPicker(id, label, items, onPick) {
  return el('button', {
    class: 'crumb-pick',
    title: label,
    attrs: { type: 'button', 'aria-label': label, 'aria-haspopup': 'menu', 'aria-expanded': 'false' },
    data: { crumb: id },
    on: {
      click: (e) => openMenu(e.currentTarget, { owner: 'crumbs', label, items, onPick }),
      keydown: (e) => {
        // Down or Up opens the list too, as on a native pop-up button.
        if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
        e.preventDefault();
        e.stopPropagation();
        openMenu(e.currentTarget, { owner: 'crumbs', label, items, onPick });
      },
    },
  });
}

/** Previous or next account (the same as Cmd+[ and Cmd+]). Keeps focus when used from the keyboard. */
function crumbStep(id, label, delta, cmd) {
  return el('button', {
    class: 'crumb-step',
    text: delta < 0 ? '‹' : '›',
    title: withShortcut(label, cmd),
    attrs: { type: 'button', 'aria-label': label },
    data: { crumb: id },
    on: {
      click: (e) => {
        const fromKeyboard = e.detail === 0;
        stepAccount(delta);
        if (fromKeyboard) {
          const again = $('crumbs').querySelector(`[data-crumb="${id}"]`);
          if (again) again.focus();
        }
      },
    },
  });
}

function renderCrumbs(entry, ai) {
  closeMenu('crumbs');
  const ix = state.ix;
  const b = state.view.budget;
  const parts = [crumb(b.title || 'Untitled budget', () => goTo('topsheet'), state.level === 'topsheet')];
  if (entry) {
    const c = entry.cat;
    const cats = ix.cats.map((x) => ({
      id: x.ti,
      label: `${x.cat.code} ${x.cat.name}`.trim(),
      checked: x.ti === entry.ti,
    }));
    parts.push(el('span', { class: 'crumb-sep', text: '/' }));
    parts.push(
      el(
        'span',
        { class: 'crumb-seg' },
        crumb(`${c.code} ${c.name}`.trim(), () => goTo('accounts'), state.level === 'accounts'),
        crumbPicker('cat', 'Go to another category', cats, (it) => openCategory(it.id)),
      ),
    );
  }
  if (state.level === 'details' && entry) {
    const a = entry.cat.accounts[ai];
    const accts = (entry.cat.accounts || []).map((x, i) => ({
      id: i,
      label: `${x.code} ${x.name}`.trim(),
      checked: i === ai,
    }));
    parts.push(el('span', { class: 'crumb-sep', text: '/' }));
    parts.push(
      el(
        'span',
        { class: 'crumb-seg' },
        crumb(`${a.code} ${a.name}`.trim(), null, true),
        crumbPicker('acct', `Go to another account in ${entry.cat.code}`.trim(), accts, (it) => openAccount(entry.ti, it.id)),
      ),
    );
    parts.push(
      el(
        'span',
        { class: 'crumb-steps', attrs: { role: 'group', 'aria-label': 'Step through accounts' } },
        crumbStep('prev', 'Previous account', -1, 'go.prev_account'),
        crumbStep('next', 'Next account', 1, 'go.next_account'),
      ),
    );
  }
  $('crumbs').replaceChildren(...parts);
}

function renderBanner(v) {
  const banner = $('banner');
  if (!v.error) {
    banner.hidden = true;
    banner.replaceChildren();
    return;
  }
  const count = typeof v.issue_count === 'number' ? v.issue_count : (v.issues || []).length;
  const head = v.report
    ? count > 1
      ? `${count} problems need fixing. Until then, totals count the flagged cells as blank.`
      : 'One problem needs fixing. Until then, totals count the flagged cell as blank.'
    : "This budget can't be totaled until this is fixed:";
  banner.replaceChildren(
    el('strong', { text: head }),
    el('span', { class: 'banner-detail', text: `${v.error.at}: ${v.error.message}` }),
  );
  banner.hidden = false;
}

function button(label, onClick, { title = '', cmd = '', danger = false } = {}) {
  return el('button', {
    class: danger ? 'btn btn-quiet btn-danger-text' : 'btn btn-quiet',
    text: label,
    title: withShortcut(title || label, cmd),
    attrs: { type: 'button' },
    on: {
      click: () => {
        onClick();
      },
    },
  });
}

function renderActions() {
  const bar = $('actions');
  if (state.level === 'topsheet') {
    bar.replaceChildren(
      button('Add category', addCategory),
      button('Add production total', addProductionTotal, { title: 'Add a production total (a subtotal of the categories above it)' }),
      button('Rename budget', renameBudget),
      el('span', { class: 'spacer' }),
      button('Delete', deleteTopsheetRow, { cmd: 'edit.delete_rows', danger: true }),
    );
  } else if (state.level === 'accounts') {
    bar.replaceChildren(
      button('Add account', addAccount),
      el('span', { class: 'spacer' }),
      button('Delete account', deleteAccount, { cmd: 'edit.delete_rows', danger: true }),
    );
  } else {
    bar.replaceChildren(
      button('Insert line', () => insertRow('line'), { cmd: 'insert.line' }),
      button('Insert subtotal', () => insertRow('subtotal'), { cmd: 'insert.subtotal' }),
      button('Insert adjustment', () => insertRow('adjustment'), {
        cmd: 'insert.adjustment',
        title: 'Insert an adjustment: a percentage of the lines above it (Amount 0.1 is 10%)',
      }),
      button('Insert separator', () => insertRow('separator'), {
        cmd: 'insert.separator',
        title: 'Insert a separator: a drawn rule line with no cost',
      }),
      el('span', { class: 'spacer' }),
      button('Delete rows', deleteRows, { cmd: 'edit.delete_rows', danger: true }),
    );
  }
}

function renderPanel() {
  const panel = $('panel');
  panel.hidden = !state.panelOpen;
  if (resizer) resizer.hidden = !state.panelOpen || !dynamicStyles;
  $('btn-panel').setAttribute('aria-pressed', String(state.panelOpen));
  if (!state.panelOpen || !state.view) return;
  renderSetup(panel, state.view, {
    call: (cmd, args) => call(cmd, args),
    tab: state.tab,
    // Apply panel: the grid's selected row keys at the current level.
    selection: () => ({ level: state.level, keys: grid.selectedKeys(), cat: state.cat }),
    setTab: (tab) => {
      state.tab = tab;
      renderPanel();
    },
  });
}

function renderFooterNow() {
  const c = state.changes;
  const changes = c && c.start !== null && c.prev !== null ? { session: c.prev - c.start, last: c.last } : null;
  renderFooter($('footer'), state.view, state.level === 'details' ? state.selection : null, {
    context: footerContext(),
    changes,
  });
}

/** When drilled down: the current account's and its category's totals, labeled with code and name. */
function footerContext() {
  if (state.level === 'topsheet' || !state.ix || !state.cat) return [];
  const entry = findCategory(state.ix, state.cat.code, state.cat.ti);
  if (!entry) return [];
  const out = [];
  if (state.level === 'details' && state.acct) {
    const a = (entry.cat.accounts || [])[state.acct.ai];
    const ar = entry.rep ? entry.rep.accounts[state.acct.ai] : null;
    if (a) out.push({ label: `${a.code} ${a.name}`.trim(), cents: ar ? ar.total : null, title: 'Total of the account you are in' });
  }
  const c = entry.cat;
  out.push({ label: `${c.code} ${c.name}`.trim(), cents: entry.rep ? entry.rep.total : null, title: 'Total of the category you are in' });
  return out;
}

// ---------------------------------------------------------------------------
// Navigation

function goTo(level, opts = {}) {
  const order = ['topsheet', 'accounts', 'details'];
  if (order.indexOf(level) >= order.indexOf(state.level)) return;
  const leaving = state.level === 'details';
  state.level = level;
  state.selection = null;
  render({ focusKey: state.memory[level] || null, ...opts });
  grid.focus();
  // No account on screen: later views leave its rows out.
  if (leaving) act('focus_account', {}, { quiet: true });
}

function goUp() {
  if (state.level === 'details') goTo('accounts');
  else if (state.level === 'accounts') goTo('topsheet');
}

function drillDown(rowKey) {
  if (state.level === 'topsheet') openCategory(Number(rowKey.slice(1)));
  else if (state.level === 'accounts') openAccount(state.cat.ti, Number(rowKey.slice(1)));
}

/** Shows the accounts of the category at topsheet index `ti` (from anywhere). */
function openCategory(ti) {
  const c = state.ix.cats.find((x) => x.ti === ti);
  if (!c) return;
  const leaving = state.level === 'details';
  state.memory.topsheet = `t${ti}`;
  state.cat = { code: c.cat.code, ti };
  state.level = 'accounts';
  state.selection = null;
  render({ focusKey: 'a0', resetSelection: true });
  grid.focus();
  if (leaving) act('focus_account', {}, { quiet: true });
}

// The account being opened, while Rust fetches its rows (so Cmd+] pressed
// again before they arrive steps on from there).
let opening = null;

/**
 * Shows the lines of account `ai` in the category at topsheet index `ti`
 * (from anywhere). The view carries only the account on screen, so this asks
 * Rust for it first and draws when it arrives.
 */
async function openAccount(ti, ai) {
  const c = state.ix.cats.find((x) => x.ti === ti);
  const a = c && (c.cat.accounts || [])[ai];
  if (!a) return;
  const target = { ti, ai };
  opening = target;
  let v;
  try {
    v = await invoke('focus_account', { category: c.cat.code, ti, account: a.code, ai });
  } catch (e) {
    toast(e);
    return;
  } finally {
    if (opening === target) opening = null;
  }
  if (!v || !v.account) return;
  state.memory.topsheet = `t${v.account.ti}`;
  state.memory.accounts = `a${v.account.ai}`;
  state.cat = { code: c.cat.code, ti: v.account.ti };
  state.acct = { code: v.account.code, ai: v.account.ai };
  state.level = 'details';
  state.selection = null;
  apply(v, { cmd: 'focus_account', focusFirst: true, focusCol: 'description', resetSelection: true });
  grid.focus();
}

/** Opens the category or account under the cursor (Cmd/Ctrl+Down). */
function goDown() {
  if (state.level === 'details') return;
  const r = grid.cursorRow();
  if (r && r.activatable) drillDown(r.key);
}

/** Next (1) or previous (-1) account's lines, across categories, wrapping around. */
function stepAccount(delta) {
  if (state.level !== 'details' || !state.acct) return;
  const list = accountList(state.ix);
  const from = opening || { ti: state.cat.ti, ai: state.acct.ai };
  const i = list.findIndex((x) => x.ti === from.ti && x.ai === from.ai);
  if (i < 0 || list.length < 2) return;
  const next = list[(i + delta + list.length) % list.length];
  openAccount(next.ti, next.ai);
}

// ---------------------------------------------------------------------------
// Editing cells

function commitCell(rowKey, colKey, value) {
  const ix = state.ix;
  if (state.level === 'details') {
    act('set_detail_field', { rowId: Number(rowKey.slice(1)), field: colKey, value });
  } else if (state.level === 'topsheet') {
    const ti = Number(rowKey.slice(1));
    const row = ix.topsheet[ti];
    if (!row) return;
    if (row.kind === 'production_total') {
      act('update_topsheet_row', { index: ti, code: null, name: value });
    } else {
      const code = colKey === 'code' ? value : row.code;
      const name = colKey === 'name' ? value : row.name;
      act('update_topsheet_row', { index: ti, code, name });
    }
  } else {
    const ai = Number(rowKey.slice(1));
    const entry = findCategory(ix, state.cat.code, state.cat.ti);
    const a = entry && entry.cat.accounts[ai];
    if (!a) return;
    const newCode = colKey === 'code' ? value : a.code;
    const name = colKey === 'name' ? value : a.name;
    act('update_account', { code: a.code, newCode, name });
  }
}

function commitMany(rowKeys, colKey, value) {
  if (state.level !== 'details') {
    commitCell(rowKeys[0], colKey, value);
    return;
  }
  act('set_rows_field', { rowIds: rowKeys.map((k) => Number(k.slice(1))), field: colKey, value });
}

// ---------------------------------------------------------------------------
// Several cells at once: clear, paste, cut, fill down (detail grid)

/** Sets detail cells as one undo step. cells: [{ rowKey, colKey, value }]. */
function setCells(cells) {
  const edits = cells.map((c) => ({ rowId: Number(c.rowKey.slice(1)), field: c.colKey, value: c.value }));
  return act('set_cells', { edits });
}

function clearCells(cells) {
  if (state.level === 'details') {
    setCells(cells.map((c) => ({ ...c, value: '' })));
    return;
  }
  // Category and account lists: one cell at a time, the cursor's.
  const at = grid.cursorCell();
  if (at && at.cell.edit !== undefined && at.cell.edit !== '') commitCell(at.row.key, at.col.key, '');
}

function fillDown() {
  if (state.level !== 'details') return;
  const cells = grid.fillDownTargets();
  if (cells.length) setCells(cells);
  grid.focus();
}

const NUMBER_FIELDS = new Set(['amount', 'x', 'rate', 'x4']);
const PASTED_NUMBER = /^(\()?(-)?(\d{1,3}(?:,\d{3})+(?:\.\d+)?|\d*\.?\d+)(\))?$/;

/** Spreadsheet number text ("1,500.00", "(250)") as a plain number for formula cells. */
function pasteValue(colKey, value) {
  if (!NUMBER_FIELDS.has(colKey)) return value;
  const m = PASTED_NUMBER.exec(value.trim());
  if (!m || Boolean(m[1]) !== Boolean(m[4])) return value;
  const n = m[3].replace(/,/g, '');
  return m[1] || m[2] ? `-${n}` : n;
}

function pasteText(text) {
  if (!text) return;
  const matrix = parseTSV(text);
  if (state.level !== 'details') {
    // Category and account lists take one cell at a time.
    const at = grid.cursorCell();
    if (!at || at.cell.edit === undefined) return;
    if (matrix.length > 1 || matrix[0].length > 1) toast('Only one cell can be pasted here; pasted the first one.');
    const value = matrix[0][0];
    if (value !== String(at.cell.edit)) commitCell(at.row.key, at.col.key, value);
    return;
  }
  const { cells, rowsShort } = grid.pasteTargets(matrix);
  if (cells.length) setCells(cells.map((c) => ({ ...c, value: pasteValue(c.colKey, c.value) })));
  if (rowsShort) {
    const rows = rowsShort === 1 ? '1 pasted row' : `${rowsShort} pasted rows`;
    toast(`${rows} didn't fit below the last line. Insert lines, then paste again.`);
  }
}

/** True when the grid (not a text field or dialog) gets copy, cut and paste. */
function gridHasClipboard() {
  if (isHelpOpen() || isModalOpen() || grid.editor) return false;
  const a = document.activeElement;
  return !a || a === document.body || a === $('grid-host');
}

function wireClipboard() {
  // WebKit enables Copy, Cut and Paste with nothing selected only when these
  // "before" events are cancelled.
  for (const type of ['beforecopy', 'beforecut', 'beforepaste']) {
    document.addEventListener(type, (e) => {
      if (gridHasClipboard()) e.preventDefault();
    });
  }
  const copy = (e) => {
    if (!gridHasClipboard() || !e.clipboardData) return false;
    e.preventDefault();
    e.clipboardData.setData('text/plain', grid.copyText());
    return true;
  };
  document.addEventListener('copy', copy);
  document.addEventListener('cut', (e) => {
    if (copy(e)) clearCells(grid.editableCells());
  });
  document.addEventListener('paste', (e) => {
    if (!gridHasClipboard() || !e.clipboardData) return;
    e.preventDefault();
    pasteText(e.clipboardData.getData('text/plain'));
  });
}

// ---------------------------------------------------------------------------
// Selection totals (detail grid)

let selTimer = null;
let selSeq = 0;
function selectionChanged(keys) {
  document.dispatchEvent(new CustomEvent('fwb:selection')); // apply panel (apply.js)
  if (state.level !== 'details') return;
  requestSelectionTotals(keys);
}

/**
 * Asks for the selected detail rows' totals (debounced; the newest request
 * wins). The footer shows the row count at once and the totals when they come.
 * Called when the selection changes and after every edit, which can change
 * the totals of rows that stay selected.
 */
function requestSelectionTotals(keys) {
  const ids = keys.filter((k) => k.startsWith('r')).map((k) => Number(k.slice(1)));
  state.selection = { count: ids.length, totals: state.selection ? state.selection.totals : null };
  renderFooterNow();
  clearTimeout(selTimer);
  const seq = ++selSeq;
  selTimer = setTimeout(async () => {
    try {
      const totals = await invoke('selection_totals', { rowIds: ids });
      if (seq !== selSeq || state.level !== 'details') return;
      state.selection = { count: ids.length, totals };
      renderFooterNow();
    } catch (e) {
      toast(e);
    }
  }, 80);
}

// ---------------------------------------------------------------------------
// Structure: rows, accounts, categories

function currentAccountCode() {
  return state.level === 'details' && state.acct ? state.acct.code : null;
}

async function insertRow(kind) {
  if (state.level !== 'details') return;
  const key = grid.cursorKey();
  const after = key && key.startsWith('r') ? Number(key.slice(1)) : null;
  const v = await act('add_detail', { accountCode: currentAccountCode(), after, kind });
  if (v && v.created_row !== null) {
    render({ focusKey: `r${v.created_row}`, focusCol: 'description' });
  }
  grid.focus();
}

async function deleteRows() {
  if (state.level === 'topsheet') return deleteTopsheetRow();
  if (state.level === 'accounts') return deleteAccount();
  const ids = grid
    .selectedKeys()
    .filter((k) => k.startsWith('r'))
    .map((k) => Number(k.slice(1)));
  if (!ids.length) return;
  await act('delete_details', { rowIds: ids });
  grid.focus();
}

/** The next code after `prev` with its numeric tail incremented, avoiding `taken`. */
function nextCode(prev, taken, fallback) {
  let code = prev || fallback;
  for (let n = 0; n < 1000; n++) {
    const m = /^(.*?)(\d+)$/.exec(code);
    if (!m) return fallback;
    const next = String(Number(m[2]) + 1).padStart(m[2].length, '0');
    code = m[1] + next;
    if (!taken.has(code)) return code;
  }
  return fallback;
}

async function addCategory() {
  const ix = state.ix;
  const key = grid.cursorKey();
  const at = key && key.startsWith('t') ? Number(key.slice(1)) + 1 : ix.topsheet.length;
  const before = ix.cats.filter((c) => c.ti < at);
  const prev = before.length ? before[before.length - 1].cat.code : '';
  const taken = new Set(ix.cats.map((c) => c.cat.code));
  const suggested = prev ? nextCode(prev, taken, '') : '10';
  const ok = await formDialog({
    title: 'Add category',
    fields: [
      { key: 'code', label: 'Category number', value: suggested },
      { key: 'name', label: 'Description', value: '', placeholder: 'e.g. Art department' },
    ],
    submitText: 'Add',
    onSubmit: (f) => call('add_category', { code: f.code, name: f.name, at }, { focusKey: `t${at}`, focusCol: 'name' }),
  });
  if (ok) grid.focus();
}

async function addProductionTotal() {
  const ix = state.ix;
  const key = grid.cursorKey();
  const at = key && key.startsWith('t') ? Number(key.slice(1)) + 1 : ix.topsheet.length;
  const ok = await formDialog({
    title: 'Add production total',
    message: 'A production total adds up every category above it, back to the previous production total. The first one ends above-the-line.',
    fields: [{ key: 'name', label: 'Name', value: '', placeholder: 'e.g. Total production' }],
    submitText: 'Add',
    onSubmit: (f) => call('add_production_total', { name: f.name, at }, { focusKey: `t${at}`, focusCol: 'name' }),
  });
  if (ok) grid.focus();
}

async function renameBudget() {
  const ok = await formDialog({
    title: 'Rename budget',
    fields: [{ key: 'title', label: 'Budget title', value: state.view.budget.title || '' }],
    submitText: 'Rename',
    onSubmit: (f) => call('set_title', { title: f.title }),
  });
  if (ok) grid.focus();
}

/** Insert > Category / Production Total: these live on the topsheet, so go there first. */
async function insertOnTopsheet(add) {
  if (state.level !== 'topsheet') goTo('topsheet');
  await add();
}

/** Insert > Account: in the current category, after the current account. */
async function insertAccount() {
  if (state.level === 'details') {
    goTo('accounts');
  } else if (state.level === 'topsheet') {
    const r = grid.cursorRow();
    if (!r || !r.activatable) return;
    drillDown(r.key);
    const entry = findCategory(state.ix, state.cat.code, state.cat.ti);
    const n = entry ? (entry.cat.accounts || []).length : 0;
    if (n) grid.goTo(`a${n - 1}`);
  }
  await addAccount();
}

async function deleteTopsheetRow() {
  const key = grid.cursorKey();
  if (!key || !key.startsWith('t')) return;
  const ti = Number(key.slice(1));
  const row = state.ix.topsheet[ti];
  if (!row) return;
  if (row.kind === 'production_total') {
    await act('delete_topsheet_row', { index: ti });
    grid.focus();
    return;
  }
  const n = (row.accounts || []).length;
  const what = n === 1 ? '1 account' : `${n} accounts`;
  const ok = await confirmDialog({
    title: 'Delete category',
    message: `Delete ${row.code} ${row.name} and its ${what}, with every line in them? You can undo this.`,
    confirmText: 'Delete',
    danger: true,
  });
  if (ok) await act('delete_topsheet_row', { index: ti });
  grid.focus();
}

async function addAccount() {
  const ix = state.ix;
  const entry = findCategory(ix, state.cat.code, state.cat.ti);
  if (!entry) return;
  const accts = entry.cat.accounts || [];
  const key = grid.cursorKey();
  const ai = key && key.startsWith('a') ? Number(key.slice(1)) : accts.length - 1;
  const after = accts[ai] ? accts[ai].code : null;
  const taken = new Set(ix.cats.flatMap((c) => (c.cat.accounts || []).map((a) => a.code)));
  const coding = ix.b.account_coding || {};
  const sep = coding.separator === undefined ? '-' : coding.separator;
  const width = coding.account_width || 2;
  const first = `${entry.cat.code}${sep}${'0'.repeat(Math.max(0, width - 1))}1`;
  const suggested = after ? nextCode(after, taken, first) : taken.has(first) ? nextCode(first, taken, first) : first;
  const insertAt = after ? ai + 1 : accts.length;
  const ok = await formDialog({
    title: 'Add account',
    fields: [
      { key: 'code', label: 'Account number', value: suggested },
      { key: 'name', label: 'Description', value: '', placeholder: 'e.g. Set dressing' },
    ],
    submitText: 'Add',
    onSubmit: (f) =>
      call(
        'add_account',
        { categoryCode: entry.cat.code, code: f.code, name: f.name, after },
        { focusKey: `a${insertAt}`, focusCol: 'name' },
      ),
  });
  if (ok) grid.focus();
}

async function deleteAccount() {
  const key = grid.cursorKey();
  if (!key || !key.startsWith('a')) return;
  const entry = findCategory(state.ix, state.cat.code, state.cat.ti);
  const a = entry && entry.cat.accounts[Number(key.slice(1))];
  if (!a) return;
  const n = typeof a.lines === 'number' ? a.lines : (a.details || []).length;
  const ok = n === 0 || (await confirmDialog({
    title: 'Delete account',
    message: `Delete ${a.code} ${a.name} and its ${n === 1 ? '1 line' : `${n} lines`}? You can undo this.`,
    confirmText: 'Delete',
    danger: true,
  }));
  if (ok) await act('delete_account', { code: a.code });
  grid.focus();
}

// ---------------------------------------------------------------------------
// Files and history

async function discardOk() {
  if (!state.view || !state.view.file.dirty) return true;
  return confirmDialog({
    title: 'Unsaved changes',
    message: 'This budget has changes that are not saved. Continue and lose them?',
    confirmText: 'Discard changes',
    danger: true,
  });
}

function resetNav() {
  state.level = 'topsheet';
  state.cat = null;
  state.acct = null;
  state.memory = {};
  state.selection = null;
}

async function doNew() {
  if (!(await discardOk())) return grid.focus();
  resetNav();
  await act('new_budget', {}, { resetSelection: true, focusKey: 't0', fresh: true });
  grid.focus();
}

async function doOpen() {
  if (!(await discardOk())) return grid.focus();
  try {
    // Resolves null when the user cancels the dialog.
    const v = await invoke('open_budget');
    if (v) {
      resetNav();
      apply(v, { resetSelection: true, focusKey: 't0', fresh: true });
    }
  } catch (e) {
    toast(e);
  }
  grid.focus();
}

async function doSave(saveAs) {
  await act('save_budget', { saveAs });
  grid.focus();
}

function doUndo() {
  act('undo');
}

function doRedo() {
  act('redo');
}

/**
 * File > Print: the printout needs every row, which views leave out, so this
 * asks for everything (after any queued edits, so the printout has them).
 */
async function doPrint(kind) {
  let full;
  try {
    full = await invoke('get_full_view');
  } catch (e) {
    toast(e);
    return;
  }
  try {
    await printReport(full, kind);
  } catch (e) {
    toast(e);
  }
  grid.focus();
}

// ---------------------------------------------------------------------------
// View: setup panel

function togglePanel() {
  const a = document.activeElement;
  const hadFocus = $('panel').contains(a) || a === resizer;
  state.panelOpen = !state.panelOpen;
  renderPanel();
  if (!state.panelOpen && hadFocus) grid.focus();
}

/** Opens the setup panel on a tab and puts focus on the tab. */
function showTab(tab) {
  state.tab = tab;
  state.panelOpen = true;
  renderPanel();
  const button = document.getElementById(`setup-tab-${tab}`);
  if (button) button.focus();
}

// ---------------------------------------------------------------------------
// View: the setup panel's width (drag its edge; saved per user)

const PANEL_DEFAULT = 380; // styles.css .panel
const PANEL_MIN = 260;
let resizer = null;
let panelWidth = null; // null: the default width

function panelMax() {
  return Math.max(PANEL_MIN, Math.min(760, window.innerWidth - 480));
}

/** Sets the panel's width (null: back to the default). Saves it when `save`. */
function setPanelWidth(w, { save = false } = {}) {
  panelWidth = w === null ? null : Math.round(Math.max(PANEL_MIN, Math.min(w, panelMax())));
  setRules('panel-width', panelWidth === null ? '' : `.workspace > .panel { width: ${panelWidth}px; }`);
  if (resizer) {
    resizer.setAttribute('aria-valuenow', String(panelWidth === null ? PANEL_DEFAULT : panelWidth));
    resizer.setAttribute('aria-valuemax', String(panelMax()));
  }
  if (save) writePref('panelWidth', panelWidth);
}

function currentPanelWidth() {
  return panelWidth === null ? $('panel').getBoundingClientRect().width || PANEL_DEFAULT : panelWidth;
}

function wirePanelResizer() {
  resizer = el('div', {
    class: 'panel-resizer',
    title: 'Drag to resize the setup panel; double-click for the standard width',
    attrs: {
      role: 'separator',
      tabindex: '0',
      'aria-orientation': 'vertical',
      'aria-controls': 'panel',
      'aria-label': 'Setup panel width',
      'aria-valuemin': String(PANEL_MIN),
      'aria-valuemax': String(panelMax()),
      'aria-valuenow': String(PANEL_DEFAULT),
    },
  });
  $('panel').before(resizer);
  const saved = readPref('panelWidth', null);
  if (typeof saved === 'number' && Number.isFinite(saved)) setPanelWidth(saved);

  resizer.addEventListener('pointerdown', (e) => {
    if (e.button !== 0) return;
    e.preventDefault();
    const startX = e.clientX;
    const startW = currentPanelWidth();
    resizer.setPointerCapture(e.pointerId);
    document.body.classList.add('panel-resizing');
    const move = (ev) => setPanelWidth(startW + (startX - ev.clientX));
    const end = () => {
      resizer.removeEventListener('pointermove', move);
      resizer.removeEventListener('pointerup', end);
      resizer.removeEventListener('pointercancel', end);
      document.body.classList.remove('panel-resizing');
      if (panelWidth !== null) writePref('panelWidth', panelWidth);
    };
    resizer.addEventListener('pointermove', move);
    resizer.addEventListener('pointerup', end);
    resizer.addEventListener('pointercancel', end);
  });
  resizer.addEventListener('dblclick', () => setPanelWidth(null, { save: true }));
  // Left makes the panel wider (the edge moves left), Right narrower; Shift for
  // bigger steps; Home and End for the narrowest and widest; Escape to the grid.
  resizer.addEventListener('keydown', (e) => {
    const step = e.shiftKey ? 80 : 20;
    const w = currentPanelWidth();
    let next = null;
    if (e.key === 'ArrowLeft') next = w + step;
    else if (e.key === 'ArrowRight') next = w - step;
    else if (e.key === 'Home') next = PANEL_MIN;
    else if (e.key === 'End') next = panelMax();
    else if (e.key === 'Escape') {
      e.preventDefault();
      grid.focus();
      return;
    } else return;
    e.preventDefault();
    e.stopPropagation();
    setPanelWidth(next, { save: true });
  });
  window.addEventListener('resize', () => {
    if (panelWidth !== null) setPanelWidth(panelWidth);
  });
}

// ---------------------------------------------------------------------------
// View: optional detail columns (the menu in the grid's top-left corner; saved per user)

function readColumnChoices() {
  const saved = readPref('detailColumns', null);
  const out = {};
  if (saved && typeof saved === 'object') {
    for (const c of OPTIONAL_DETAIL_COLUMNS) if (typeof saved[c.key] === 'boolean') out[c.key] = saved[c.key];
  }
  return out;
}

function gridDetailColumns() {
  return detailColumns(state.columns).map((c) => (c.key === 'num' ? { ...c, head: columnMenuButton } : c));
}

function columnMenuItems() {
  return OPTIONAL_DETAIL_COLUMNS.map((c) => ({
    id: c.key,
    label: c.name || c.label,
    title: c.title,
    checked: columnShown(c, state.columns),
  }));
}

function columnsIcon() {
  const NS = 'http://www.w3.org/2000/svg';
  const svg = document.createElementNS(NS, 'svg');
  for (const [k, v] of Object.entries({ viewBox: '0 0 16 16', width: '14', height: '14', 'aria-hidden': 'true', focusable: 'false' })) {
    svg.setAttribute(k, v);
  }
  for (const x of [1.5, 6.5, 11.5]) {
    const r = document.createElementNS(NS, 'rect');
    for (const [k, v] of Object.entries({ x: String(x), y: '2', width: '3', height: '12', rx: '0.8' })) r.setAttribute(k, v);
    svg.append(r);
  }
  return svg;
}

function openColumnMenu(anchor) {
  openMenu(anchor, {
    owner: 'columns',
    label: 'Columns',
    checkable: true,
    items: columnMenuItems(),
    returnFocus: () => $('grid-host').querySelector('.col-chooser'),
    onPick: (item) => {
      const col = OPTIONAL_DETAIL_COLUMNS.find((c) => c.key === item.id);
      if (!col) return;
      state.columns = { ...state.columns, [col.key]: !columnShown(col, state.columns) };
      writePref('detailColumns', state.columns);
      if (state.level === 'details') render();
      updateMenu(columnMenuItems());
    },
  });
}

/** The header button for the column menu (the grid draws it in the # column's header). */
function columnMenuButton() {
  const b = el(
    'button',
    {
      class: 'col-chooser',
      title: 'Show or hide columns',
      attrs: { type: 'button', 'aria-label': 'Choose columns', 'aria-haspopup': 'menu', 'aria-expanded': 'false' },
      data: { focusId: 'col-chooser' },
      on: {
        click: (e) => openColumnMenu(e.currentTarget),
        keydown: (e) => {
          if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
          e.preventDefault();
          e.stopPropagation();
          openColumnMenu(e.currentTarget);
        },
      },
    },
    columnsIcon(),
  );
  // The grid redraws while the menu is open (a column was toggled): follow it.
  if (isMenuOpen('columns')) setMenuAnchor('columns', b);
  return b;
}

/** In a text field, Undo, Redo and Select All act on the field. */
function fieldEdit(kind) {
  const a = document.activeElement;
  if (!isTextField(a)) return false;
  if (kind === 'selectAll') {
    if (typeof a.select === 'function') a.select();
  } else {
    document.execCommand(kind); // the field's own typing history
  }
  return true;
}

function selectAll() {
  grid.selectAll();
  grid.focus();
}

// ---------------------------------------------------------------------------
// Commands: one per shortcut-list id, run from keys and the menu bar alike

const COMMANDS = {
  'file.new': () => doNew(),
  'file.open': () => doOpen(),
  'file.save': () => doSave(false),
  'file.save_as': () => doSave(true),
  'file.print_topsheet': () => doPrint('topsheet'),
  'file.print_budget': () => doPrint('both'),
  'edit.undo': () => fieldEdit('undo') || doUndo(),
  'edit.redo': () => fieldEdit('redo') || doRedo(),
  'edit.select_all': () => fieldEdit('selectAll') || selectAll(),
  'edit.fill_down': fillDown,
  'edit.delete_rows': deleteRows,
  'insert.line': () => insertRow('line'),
  'insert.subtotal': () => insertRow('subtotal'),
  'insert.adjustment': () => insertRow('adjustment'),
  'insert.separator': () => insertRow('separator'),
  'insert.account': insertAccount,
  'insert.category': () => insertOnTopsheet(addCategory),
  'insert.production_total': () => insertOnTopsheet(addProductionTotal),
  'go.topsheet': () => goTo('topsheet'),
  'go.up': goUp,
  'go.down': goDown,
  'go.next_account': () => stepAccount(1),
  'go.prev_account': () => stepAccount(-1),
  'view.toggle_panel': togglePanel,
  'help.shortcuts': toggleHelp,
  // The menu items are answered in Rust; these run only for the keys on
  // Windows and Linux, where the page sees them first.
  'file.close': closeWindow,
  'app.quit': closeWindow,
};
for (const s of SHORTCUTS) {
  if (s.id.startsWith('view.tab.')) COMMANDS[s.id] = () => showTab(s.id.slice('view.tab.'.length));
}

/** Commands that act on a focused text field (and so leave a cell edit open). */
const FIELD_COMMANDS = new Set(SHORTCUTS.filter((s) => s.field).map((s) => s.id));

// A key and its menu item can both arrive for one press (a native menu key
// that also reaches the page). The second one within this window is dropped.
const REPEAT_MS = 400;
let lastRun = { id: '', via: '', at: 0 };

function run(id, via) {
  const fn = COMMANDS[id];
  if (!fn) return;
  const now = performance.now();
  if (lastRun.id === id && lastRun.via !== via && now - lastRun.at < REPEAT_MS) {
    lastRun = { id: '', via: '', at: 0 };
    return;
  }
  lastRun = { id, via, at: now };
  if (grid.editor && !FIELD_COMMANDS.has(id)) {
    grid.commitEdit(null);
    // Fill down would read the cell before the edit lands; just finish the edit.
    if (id === 'edit.fill_down') return;
  }
  fn();
}

/** A key left to the browser (undo in a text field): a repeat from the menu is dropped. */
function leftToBrowser(id) {
  lastRun = { id, via: 'key', at: performance.now() };
}

function onMenu(id) {
  if (typeof id !== 'string' || !COMMANDS[id]) return;
  if (isHelpOpen() && id !== 'help.shortcuts') closeHelp();
  // A dialog is open: only field commands (undo typing in it) make sense.
  if (isModalOpen() && !FIELD_COMMANDS.has(id)) return;
  // A grid command while a text field has focus: where menus see keys before
  // the page (Linux), the key was meant for the field (Ctrl+Backspace deletes a
  // word, not rows). The cell editor still takes the ones without a text meaning.
  const sc = shortcutById(id);
  if (sc && sc.scope === 'grid' && isTextField(document.activeElement) && (sc.text || !grid.editor)) return;
  run(id, 'menu');
}

/**
 * Close Window and Quit from the keyboard on Windows and Linux: closing the
 * (only) window goes through the same unsaved-changes question in Rust.
 */
function closeWindow() {
  const w = window.__TAURI__ && window.__TAURI__.window;
  if (w && typeof w.getCurrentWindow === 'function') {
    w.getCurrentWindow()
      .close()
      .catch((e) => toast(e));
  }
}

function listenToMenu() {
  const events = window.__TAURI__ && window.__TAURI__.event;
  if (!events || typeof events.listen !== 'function') return;
  events.listen('menu', (e) => onMenu(e.payload)).catch((err) => toast(err));
}

// ---------------------------------------------------------------------------
// Keyboard

function onShortcutKey(e, sc, { editing, inField }) {
  if (sc.scope === 'app') {
    if (menuRunsAppKeys) return; // macOS: the menu item's key equivalent runs it
    if (inField && sc.field) {
      leftToBrowser(sc.id); // the field's own undo, redo or select all
      return;
    }
    e.preventDefault();
    run(sc.id, 'key');
    return;
  }
  // Grid commands. Keys that move the caret or delete in a text field stay there.
  if (inField && sc.text) return;
  // Cancelling also keeps the key from reaching the menu bar a second time.
  e.preventDefault();
  if (inField && !editing) return; // a setup or dialog field: not meant for the grid
  run(sc.id, 'key');
}

function onKeyDown(e) {
  if (isHelpOpen() || isModalOpen()) return; // they handle their own keys
  const active = document.activeElement;
  const editing = Boolean(grid.editor);
  const inField = isTextField(active);
  // A button in the grid's header row (the column menu) is not the grid.
  const inGrid = $('grid-host').contains(active) && !(active.closest && active.closest('thead'));
  const bodyFocused = !active || active === document.body;

  // The cell editor answers its own keys first: Enter, Tab, Escape, arrows,
  // and Cmd/Ctrl+Enter (same value into every selected row).
  if (editing && inGrid && grid.handleKey(e)) {
    e.preventDefault();
    return;
  }

  const sc = matchShortcut(e);
  if (sc) {
    onShortcutKey(e, sc, { editing, inField });
    return;
  }

  // Escape on a setup panel tab or button, a breadcrumb button or the column
  // menu button goes back to the grid.
  const chrome = $('panel').contains(active) || $('crumbs').contains(active) || (active && active.closest && active.closest('#grid-host thead'));
  if (e.key === 'Escape' && !inField && chrome) {
    e.preventDefault();
    grid.focus();
    return;
  }

  if (inField) return; // text fields handle their own keys
  if (inGrid || bodyFocused) {
    if (bodyFocused) grid.focus();
    if (grid.handleKey(e)) e.preventDefault();
  }
}

// ---------------------------------------------------------------------------
// Start

function wire() {
  wirePanelResizer();
  $('btn-new').addEventListener('click', doNew);
  $('btn-open').addEventListener('click', doOpen);
  $('btn-save').addEventListener('click', () => doSave(false));
  $('btn-saveas').addEventListener('click', () => doSave(true));
  $('btn-undo').addEventListener('click', doUndo);
  $('btn-redo').addEventListener('click', doRedo);
  $('btn-panel').addEventListener('click', togglePanel);
  const tips = {
    'btn-new': ['New budget', 'file.new'],
    'btn-open': ['Open a budget', 'file.open'],
    'btn-save': ['Save', 'file.save'],
    'btn-saveas': ['Save as a new file', 'file.save_as'],
    'btn-undo': ['Undo', 'edit.undo'],
    'btn-redo': ['Redo', 'edit.redo'],
    'btn-panel': ['Show or hide the setup panel', 'view.toggle_panel'],
  };
  for (const [id, [label, cmd]] of Object.entries(tips)) $(id).title = withShortcut(label, cmd);
  window.addEventListener('keydown', onKeyDown);
  wireClipboard();
  listenToMenu();
}

async function start() {
  wire();
  if (!connected) {
    $('banner').hidden = false;
    $('banner').replaceChildren(el('strong', { text: 'Open this page in the Film Wizard Budgeting app.' }));
    return;
  }
  try {
    apply(await invoke('get_view'), { focusKey: 't0', fresh: true });
  } catch (e) {
    toast(e);
  }
  grid.focus();
}

start();
