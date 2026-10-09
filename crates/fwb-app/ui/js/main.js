// App shell: holds the current view, which level is showing, the toolbar,
// breadcrumb, keyboard shortcuts, and wires the grid, side panel and footer.
//
// Rust owns the budget. Every change is a command that returns a fresh view;
// this file only decides what to ask for and redraws from what comes back.

import { invoke, connected } from './api.js';
import { el, $, hasMod, isTextField, shortcut } from './dom.js';
import { Grid } from './grid.js';
import {
  indexView,
  findCategory,
  TOPSHEET_COLUMNS,
  ACCOUNT_COLUMNS,
  DETAIL_COLUMNS,
  topsheetRows,
  accountRows,
  detailRows,
} from './levels.js';
import { renderFooter } from './footer.js';
import { formDialog, confirmDialog, isModalOpen } from './modal.js';
import { renderSetup } from './setup.js';

const state = {
  view: null,
  ix: null,
  level: 'topsheet', // 'topsheet' | 'accounts' | 'details'
  cat: null, // { code, ti }
  acct: null, // { code, ai }
  tab: 'globals',
  panelOpen: true,
  selection: null, // { count, totals } on the detail grid
  memory: {}, // level -> row key to return to
};

const grid = new Grid($('grid-host'), {
  onCommit: (rowKey, colKey, value) => commitCell(rowKey, colKey, value),
  onCommitMany: (rowKeys, colKey, value) => commitMany(rowKeys, colKey, value),
  onActivate: (rowKey) => drillDown(rowKey),
  onEscape: () => goUp(),
  onSelect: (keys) => selectionChanged(keys),
  onPastEnd: () => {
    if (state.level === 'details') insertRow('line');
  },
  enterMovesDown: false,
});

// ---------------------------------------------------------------------------
// Talking to Rust

/** Runs a command and redraws from the view it returns. Rejects with a message. */
async function call(cmd, args = {}, opts = {}) {
  const v = await invoke(cmd, args);
  if (v && typeof v === 'object' && 'budget' in v) apply(v, opts);
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
  state.view = view;
  state.ix = indexView(view);
  render(opts);
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
    const accts = entry.cat.accounts || [];
    ai = accts.findIndex((a, i) => a.code === state.acct.code && i === state.acct.ai);
    if (ai < 0) ai = accts.findIndex((a) => a.code === state.acct.code);
    if (ai < 0) {
      state.level = 'accounts';
    } else {
      state.acct = { code: accts[ai].code, ai };
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
  if (state.level === 'topsheet') {
    grid.h.enterMovesDown = false;
    grid.setData(TOPSHEET_COLUMNS, topsheetRows(ix), opts);
  } else if (state.level === 'accounts') {
    grid.h.enterMovesDown = false;
    grid.setData(ACCOUNT_COLUMNS, accountRows(ix, entry), opts);
  } else {
    grid.h.enterMovesDown = true;
    grid.setData(DETAIL_COLUMNS, detailRows(ix, entry, ai), opts);
  }
  $('grid-host').dataset.level = state.level;
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
  if (current) return el('span', { class: 'crumb current', text: label, attrs: { 'aria-current': 'page' } });
  return el('button', { class: 'crumb', text: label, attrs: { type: 'button' }, on: { click: onClick } });
}

function renderCrumbs(entry, ai) {
  const b = state.view.budget;
  const parts = [crumb(b.title || 'Untitled budget', () => goTo('topsheet'), state.level === 'topsheet')];
  if (entry) {
    const c = entry.cat;
    parts.push(el('span', { class: 'crumb-sep', text: '/' }));
    parts.push(crumb(`${c.code} ${c.name}`.trim(), () => goTo('accounts'), state.level === 'accounts'));
  }
  if (state.level === 'details' && entry) {
    const a = entry.cat.accounts[ai];
    parts.push(el('span', { class: 'crumb-sep', text: '/' }));
    parts.push(crumb(`${a.code} ${a.name}`.trim(), null, true));
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
  const count = (v.issues || []).length;
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

function button(label, onClick, { title = '', keys = '', danger = false } = {}) {
  const hint = keys ? ` (${shortcut(keys)})` : '';
  return el('button', {
    class: danger ? 'btn btn-quiet btn-danger-text' : 'btn btn-quiet',
    text: label,
    title: (title || label) + hint,
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
      button('Delete', deleteTopsheetRow, { keys: 'mod+backspace', danger: true }),
    );
  } else if (state.level === 'accounts') {
    bar.replaceChildren(
      button('Add account', addAccount),
      el('span', { class: 'spacer' }),
      button('Delete account', deleteAccount, { keys: 'mod+backspace', danger: true }),
    );
  } else {
    bar.replaceChildren(
      button('Insert line', () => insertRow('line'), { keys: 'mod+enter' }),
      button('Insert subtotal', () => insertRow('subtotal'), { keys: 'mod+alt+t' }),
      button('Insert adjustment', () => insertRow('adjustment'), {
        keys: 'mod+alt+a',
        title: 'Insert an adjustment: a percentage of the lines above it (Amount 0.1 is 10%)',
      }),
      el('span', { class: 'spacer' }),
      button('Delete rows', deleteRows, { keys: 'mod+backspace', danger: true }),
    );
  }
}

function renderPanel() {
  const panel = $('panel');
  panel.hidden = !state.panelOpen;
  $('btn-panel').setAttribute('aria-pressed', String(state.panelOpen));
  if (!state.panelOpen || !state.view) return;
  renderSetup(panel, state.view, {
    call: (cmd, args) => call(cmd, args),
    tab: state.tab,
    setTab: (tab) => {
      state.tab = tab;
      renderPanel();
    },
  });
}

function renderFooterNow() {
  renderFooter($('footer'), state.view, state.level === 'details' ? state.selection : null);
}

// ---------------------------------------------------------------------------
// Navigation

function goTo(level, opts = {}) {
  const order = ['topsheet', 'accounts', 'details'];
  if (order.indexOf(level) >= order.indexOf(state.level)) return;
  state.level = level;
  state.selection = null;
  render({ focusKey: state.memory[level] || null, ...opts });
  grid.focus();
}

function goUp() {
  if (state.level === 'details') goTo('accounts');
  else if (state.level === 'accounts') goTo('topsheet');
}

function drillDown(rowKey) {
  const ix = state.ix;
  if (state.level === 'topsheet') {
    const ti = Number(rowKey.slice(1));
    const c = ix.cats.find((x) => x.ti === ti);
    if (!c) return;
    state.memory.topsheet = rowKey;
    state.cat = { code: c.cat.code, ti };
    state.level = 'accounts';
    render({ focusKey: 'a0', resetSelection: true });
  } else if (state.level === 'accounts') {
    const ai = Number(rowKey.slice(1));
    const entry = findCategory(ix, state.cat.code, state.cat.ti);
    const a = entry && entry.cat.accounts[ai];
    if (!a) return;
    state.memory.accounts = rowKey;
    state.acct = { code: a.code, ai };
    state.level = 'details';
    state.selection = null;
    const first = (a.details || [])[0];
    render({ focusKey: first ? `r${first.id}` : null, focusCol: 'description', resetSelection: true });
  }
  grid.focus();
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
// Selection totals (detail grid)

let selTimer = null;
let selSeq = 0;
function selectionChanged(keys) {
  if (state.level !== 'details') return;
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
  const n = (a.details || []).length;
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
  await act('new_budget', {}, { resetSelection: true, focusKey: 't0' });
  grid.focus();
}

async function doOpen() {
  if (!(await discardOk())) return grid.focus();
  try {
    // Resolves null when the user cancels the dialog.
    const v = await invoke('open_budget');
    if (v) {
      resetNav();
      apply(v, { resetSelection: true, focusKey: 't0' });
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

// ---------------------------------------------------------------------------
// Keyboard

function onKeyDown(e) {
  if (isModalOpen()) return;
  const mod = hasMod(e);
  const key = e.key.toLowerCase();
  const inGrid = $('grid-host').contains(document.activeElement);
  const gridEditing = Boolean(grid.editor);
  const inField = isTextField(document.activeElement);

  if (mod && !e.altKey) {
    const fileCmd = { n: doNew, o: doOpen };
    if (!e.shiftKey && fileCmd[key]) {
      e.preventDefault();
      if (gridEditing) grid.commitEdit(null);
      fileCmd[key]();
      return;
    }
    if (key === 's') {
      e.preventDefault();
      if (gridEditing) grid.commitEdit(null);
      doSave(e.shiftKey);
      return;
    }
    // In a text field, undo and redo belong to the field.
    if ((key === 'z' || key === 'y') && !inField) {
      e.preventDefault();
      if (key === 'y' || e.shiftKey) doRedo();
      else doUndo();
      return;
    }
  }

  if (inField && !gridEditing) return; // the side panel's inputs handle their own keys

  if (state.level === 'details' && mod && !gridEditing) {
    if (e.key === 'Enter') {
      e.preventDefault();
      insertRow('line');
      return;
    }
    if (e.altKey && e.code === 'KeyT') {
      e.preventDefault();
      insertRow('subtotal');
      return;
    }
    if (e.altKey && e.code === 'KeyA') {
      e.preventDefault();
      insertRow('adjustment');
      return;
    }
  }
  if (mod && !gridEditing && (e.key === 'Backspace' || e.key === 'Delete') && (inGrid || document.activeElement === document.body)) {
    e.preventDefault();
    deleteRows();
    return;
  }

  const bodyFocused = document.activeElement === document.body || document.activeElement === null;
  if (inGrid || bodyFocused) {
    if (bodyFocused) grid.focus();
    if (grid.handleKey(e)) e.preventDefault();
  }
}

// ---------------------------------------------------------------------------
// Start

function wire() {
  $('btn-new').addEventListener('click', doNew);
  $('btn-open').addEventListener('click', doOpen);
  $('btn-save').addEventListener('click', () => doSave(false));
  $('btn-saveas').addEventListener('click', () => doSave(true));
  $('btn-undo').addEventListener('click', doUndo);
  $('btn-redo').addEventListener('click', doRedo);
  $('btn-panel').addEventListener('click', () => {
    state.panelOpen = !state.panelOpen;
    renderPanel();
  });
  const tips = {
    'btn-new': ['New budget', 'mod+n'],
    'btn-open': ['Open a budget', 'mod+o'],
    'btn-save': ['Save', 'mod+s'],
    'btn-saveas': ['Save as a new file', 'mod+shift+s'],
    'btn-undo': ['Undo', 'mod+z'],
    'btn-redo': ['Redo', 'mod+shift+z'],
  };
  for (const [id, [label, keys]] of Object.entries(tips)) $(id).title = `${label} (${shortcut(keys)})`;
  window.addEventListener('keydown', onKeyDown);
}

async function start() {
  wire();
  if (!connected) {
    $('banner').hidden = false;
    $('banner').replaceChildren(el('strong', { text: 'Open this page in the Film Wizard Budgeting app.' }));
    return;
  }
  try {
    apply(await invoke('get_view'), { focusKey: 't0' });
  } catch (e) {
    toast(e);
  }
  grid.focus();
}

start();
