// The apply lists: fringes, groups, sets, locations and currencies, each one
// a tab of the tools panel (tools.js) that is both the item's setup table and
// the way to put items on the selected lines.
//
// Every defined item is listed with a three-state checkbox that reflects the
// grid's selection (every selected line has it, some do, none do) and the
// item's dollars within the selection. A click on the checkbox (or Space)
// adds the item to every selected line, or takes it off when they all have
// it. Sets, locations and currencies are one per line: picking one replaces
// the line's old value. A click on the item's name only selects it; a
// double-click on the name (or Enter or F2) edits the item in a small form
// next to it, Delete removes it (after asking), and the row at the bottom of
// each list adds a new one. The editor's Name (Code) field, or Rename
// everywhere in the item's right-click menu, renames it with every line and
// setting that uses it (rename.js). Fringe ranges are made and removed at the foot of
// the fringe list.
//
// Each item also shows how many lines use it (a click runs Find for them)
// and its dollars across the budget; the list can show only used or unused
// items, sort by name from its header, and be put in the budget's own order
// by dragging or Option+Shift+Up and Down (setuplist.js). Fringes and groups
// can have a short code, set in their editor, which the grid shows.
//
// On the topsheet and the account list, the selection is every line under
// the selected categories or accounts. Rust resolves that (apply_summary) and
// returns the row ids, which the edit commands then take, so a click acts on
// exactly the rows the panel shows.
//
// tools.js calls applyView() on every render, then mounts what it shows:
// applyHeadHost() (what is selected, with its totals) and applyListHost(id)
// for each list tab on screen. A fresh summary redraws just those. ctx.call
// runs a command (main.js's, or the tools window's own) and ctx.selection()
// gives the grid's selection; a "fwb:selection" event on the document says it
// changed. Budget text only reaches the page through textContent (dom.js
// el()), and styling is classes from apply.css: no inline styles, no
// innerHTML.

import { invoke } from './api.js';
import { el } from './dom.js';
import { money } from './format.js';
import { anchoredForm } from './modal.js';
import { PALETTE, paletteColor } from './rowformat.js';
import { renameProblem, renamedText, itemMenu } from './rename.js';
import { orderedNames } from './order.js';
import {
  wantUsage,
  listNames,
  canReorder,
  sortButton,
  showSelect,
  usesButton,
  totalText,
  dragHandle,
  moveKey,
  MOVE_KEYS,
  inUse,
} from './setuplist.js';

const LISTS = [
  {
    id: 'fringes',
    label: 'Fringes',
    what: 'fringe',
    field: 'fringes',
    multi: true,
    defs: (b) => b.fringes,
    eligible: (s) => s.lines,
    noTakers: 'Fringes go on lines, and the selection has none.',
    none: 'No fringes yet.',
  },
  {
    id: 'groups',
    label: 'Groups',
    what: 'group',
    field: 'groups',
    multi: true,
    defs: (b) => b.groups,
    eligible: (s) => s.costed,
    noTakers: 'Subtotal rows are never in a group.',
    none: 'No groups yet.',
  },
  {
    id: 'sets',
    label: 'Sets',
    what: 'set',
    field: 'set',
    multi: false,
    defs: (b) => b.sets,
    eligible: (s) => s.costed,
    noTakers: 'Subtotal rows have no set.',
    none: 'No sets yet.',
  },
  {
    id: 'locations',
    label: 'Locations',
    what: 'location',
    field: 'location',
    multi: false,
    defs: (b) => b.locations,
    eligible: (s) => s.costed,
    noTakers: 'Subtotal rows have no location.',
    none: 'No locations yet.',
  },
  {
    id: 'currencies',
    label: 'Currency',
    what: 'currency',
    field: 'currency',
    multi: false,
    defs: (b) => b.currencies,
    eligible: (s) => s.lines,
    noTakers: 'Currencies go on lines, and the selection has none.',
    none: 'No other currencies yet.',
  },
];

/** The tools panel tabs this file draws. */
export const APPLY_LISTS = LISTS.map((l) => l.id);

const ARIA = { all: 'true', some: 'mixed', none: 'false' };
const DEBOUNCE_MS = 60;

const st = {
  view: null,
  ctx: null,
  gen: 0, // bumped for every new view
  summary: null,
  summarySig: '',
  summaryScope: null, // the selection the summary describes
  summaryGen: -1,
  timer: null,
  seq: 0,
  busy: false,
  error: '',
  filters: {},
  onlyApplied: {},
  selected: {}, // list id -> the item name last clicked (highlighted)
  heads: [], // mounted head hosts
  lists: new Map(), // list id -> mounted host
};

/**
 * A new render of the tools panel: the view and context it draws from. The
 * hosts it mounts next are drawn at once and redrawn when numbers arrive.
 */
export function applyView(view, ctx) {
  if (view !== st.view) st.gen += 1;
  st.view = view && typeof view === 'object' ? view : {};
  st.ctx = ctx || {};
  st.heads = [];
  st.lists = new Map();
  schedule();
}

/** A host showing what is selected and its totals (the panel's header). */
export function applyHeadHost() {
  const host = el('div', { class: 'apply-head-host' });
  st.heads.push(host);
  drawHead(host);
  return host;
}

/** A host showing list `id` (a tab's content). */
export function applyListHost(id) {
  const list = LISTS.find((l) => l.id === id);
  const host = el('div', { class: 'apply apply-tab', attrs: { 'aria-busy': st.busy ? 'true' : 'false' } });
  if (!list) return host;
  st.lists.set(id, host);
  drawList(host, list);
  return host;
}

document.addEventListener('fwb:selection', () => {
  if (visible()) schedule();
});

// ---------------------------------------------------------------------------
// Small helpers

function obj(v) {
  return v && typeof v === 'object' && !Array.isArray(v) ? v : {};
}

function str(v) {
  return v === undefined || v === null ? '' : String(v);
}

function budget() {
  return obj(st.view && st.view.budget);
}

function report() {
  return obj(st.view && st.view.report);
}

function errText(e) {
  if (typeof e === 'string') return e;
  if (e && typeof e.message === 'string') return e.message;
  return String(e);
}

function plural(n, one, many) {
  return `${n} ${n === 1 ? one : many}`;
}

/** "18.5" -> "18.50", "10" -> "10.00": a percent string with two places. */
function twoPlaces(s) {
  const m = /^(-?)(\d+)(?:\.(\d+))?$/.exec(str(s));
  if (!m) return str(s);
  return `${m[1]}${m[2]}.${(m[3] || '').padEnd(2, '0').slice(0, 2)}`;
}

function mounted() {
  return [...st.heads, ...st.lists.values()].filter((h) => h.isConnected);
}

function visible() {
  return mounted().some((h) => !h.closest('[hidden]'));
}

function stateOf(count, eligible) {
  if (!eligible || !count) return 'none';
  return count >= eligible ? 'all' : 'some';
}

function tally(s, list, name) {
  const t = s && obj(s[list])[name];
  return t ? { count: Number(t.count) || 0, total: t.total } : { count: 0, total: 0 };
}

function itemKey(listId, name) {
  return `apply:item:${listId}:${name}`;
}

function addKey(listId) {
  return `apply:add:${listId}`;
}

function byKey(key) {
  for (const h of mounted()) {
    const found = [...h.querySelectorAll('[data-key]')].find((e) => e.dataset.key === key);
    if (found) return found;
  }
  return null;
}

// ---------------------------------------------------------------------------
// The selection

/**
 * What is selected, as apply_summary's arguments: detail rows by id, accounts
 * by number, categories by topsheet position. ctx.selection() (main.js) gives
 * { level, keys, cat } with the grid's row keys.
 */
function scope() {
  const out = { rowIds: [], accounts: [], categories: [] };
  const sel = st.ctx && typeof st.ctx.selection === 'function' ? st.ctx.selection() : null;
  if (!sel || !Array.isArray(sel.keys)) return out;
  const top = Array.isArray(budget().topsheet) ? budget().topsheet : [];
  const index = (k, prefix) => (typeof k === 'string' && k.startsWith(prefix) ? Number(k.slice(prefix.length)) : NaN);
  if (sel.level === 'details') {
    out.rowIds = sel.keys.map((k) => index(k, 'r')).filter(Number.isSafeInteger);
  } else if (sel.level === 'accounts') {
    const cat = sel.cat ? top[sel.cat.ti] : null;
    if (cat && cat.kind !== 'production_total' && cat.code === sel.cat.code) {
      const accts = Array.isArray(cat.accounts) ? cat.accounts : [];
      for (const k of sel.keys) {
        const a = accts[index(k, 'a')];
        if (a) out.accounts.push(str(a.code));
      }
    }
  } else if (sel.level === 'topsheet') {
    for (const k of sel.keys) {
      const ti = index(k, 't');
      if (top[ti] && top[ti].kind !== 'production_total') out.categories.push(ti);
    }
  }
  return out;
}

function sigOf(sc) {
  return JSON.stringify(sc);
}

/** "All lines in 13 Cast", "All lines in 3 accounts", "4 rows selected". */
function scopeLabel(sc, s) {
  const top = Array.isArray(budget().topsheet) ? budget().topsheet : [];
  const named = (x) => `${str(x.code)} ${str(x.name)}`.trim();
  if (sc.categories.length === 1) return `All lines in ${named(top[sc.categories[0]] || {})}`;
  if (sc.categories.length > 1) return `All lines in ${plural(sc.categories.length, 'category', 'categories')}`;
  if (sc.accounts.length === 1) {
    for (const row of top) {
      const a = (Array.isArray(row.accounts) ? row.accounts : []).find((x) => x.code === sc.accounts[0]);
      if (a) return `All lines in ${named(a)}`;
    }
  }
  if (sc.accounts.length > 1) return `All lines in ${plural(sc.accounts.length, 'account', 'accounts')}`;
  const n = s ? s.rows.length : sc.rowIds.length;
  const lines = s && s.lines !== n ? `, ${plural(s.lines, 'line', 'lines')}` : '';
  return `${plural(n, 'row', 'rows')} selected${lines}`;
}

// ---------------------------------------------------------------------------
// Numbers from Rust

function schedule(ms = DEBOUNCE_MS) {
  clearTimeout(st.timer);
  st.timer = setTimeout(() => {
    fetchSummary().catch(() => {});
  }, ms);
}

async function fetchSummary() {
  if (!visible()) return;
  const sc = scope();
  const sig = sigOf(sc);
  const gen = st.gen;
  const seq = ++st.seq;
  try {
    const s = await invoke('apply_summary', sc);
    if (seq !== st.seq) return;
    if (sig !== st.summarySig) st.error = ''; // a message is about the old selection
    st.summary = s && typeof s === 'object' && Array.isArray(s.rows) ? s : null;
    st.summarySig = sig;
    st.summaryScope = sc;
    st.summaryGen = gen;
  } catch (e) {
    if (seq !== st.seq) return;
    st.error = errText(e);
  }
  draw();
}

/** The summary for the current selection and view, fetched now if stale. */
async function freshSummary() {
  const sig = sigOf(scope());
  if (st.summary && st.summarySig === sig && st.summaryGen === st.gen) return st.summary;
  clearTimeout(st.timer);
  await fetchSummary();
  return st.summary && st.summarySig === sig ? st.summary : null;
}

/** The summary to draw from: it describes the current selection, else none yet. */
function shownSummary() {
  return st.summary && st.summarySig === sigOf(scope()) ? st.summary : null;
}

// ---------------------------------------------------------------------------
// Edits

function call(cmd, args) {
  if (!st.ctx || typeof st.ctx.call !== 'function') return Promise.reject('Not connected');
  return st.ctx.call(cmd, args);
}

/**
 * Runs one edit: `plan(summary)` returns [command, args, patched summary] or
 * null. Resolves with the new view's rev when a command ran and succeeded,
 * else null.
 */
async function act(plan) {
  if (st.busy) return null;
  st.busy = true;
  st.error = '';
  draw();
  try {
    const s = await freshSummary();
    const step = s && s.rows.length ? plan(s) : null;
    if (!step) return null;
    const [cmd, args, patched] = step;
    if (patched) {
      st.summary = patched;
      draw();
    }
    const v = await call(cmd, args);
    return v && v.rev !== undefined ? v.rev : null;
  } catch (e) {
    st.error = errText(e);
    st.summaryGen = -1; // the patch may be wrong now; fetch the truth
    schedule(0);
    return null;
  } finally {
    st.busy = false;
    draw();
  }
}

/** A copy of `s` with list `list` showing `name` on all of `eligible` rows (or none). */
function patch(s, list, name, eligible, on, single) {
  const next = { ...s, [list]: { ...obj(s[list]) } };
  if (single) {
    for (const k of Object.keys(next[list])) next[list][k] = { ...next[list][k], count: 0 };
  }
  if (name !== null) next[list][name] = { ...tally(s, list, name), count: on ? eligible : 0 };
  return next;
}

function toggle(list, name) {
  return act((s) => {
    const eligible = list.eligible(s);
    if (!eligible) return null;
    const on = stateOf(tally(s, list.id, name).count, eligible) !== 'all';
    const rowIds = s.rows;
    if (list.multi) {
      return ['apply_to_rows', { rowIds, field: list.field, name, on }, patch(s, list.id, name, eligible, on, false)];
    }
    return [
      'assign_to_rows',
      { rowIds, field: list.field, value: on ? name : null },
      patch(s, list.id, name, eligible, on, true),
    ];
  });
}

function clearValue(list) {
  act((s) => {
    if (!list.eligible(s)) return null;
    return ['assign_to_rows', { rowIds: s.rows, field: list.field, value: null }, patch(s, list.id, null, 0, false, true)];
  });
}

function makeRange() {
  act((s) => (s.range_blocked === null ? ['make_fringe_range', { rowIds: s.rows }, null] : null));
}

function removeRange() {
  act((s) => (s.in_range > 0 ? ['clear_fringe_range', { rowIds: s.rows }, null] : null));
}

// ---------------------------------------------------------------------------
// Drawing

let uid = 0;
function nextId() {
  uid += 1;
  return `apply-${uid}`;
}

function captureFocus() {
  const a = document.activeElement;
  if (!a || !a.dataset || !a.dataset.key || !mounted().some((h) => h.contains(a))) return null;
  const info = { key: a.dataset.key };
  if (typeof a.selectionStart === 'number') {
    info.start = a.selectionStart;
    info.end = a.selectionEnd;
  }
  return info;
}

function restoreFocus(info) {
  if (!info) return;
  const target = byKey(info.key);
  if (!target) return;
  target.focus({ preventScroll: true });
  if (info.start !== undefined && typeof target.setSelectionRange === 'function') {
    try {
      target.setSelectionRange(info.start, info.end);
    } catch {
      // Not a text control.
    }
  }
}

function button(text, cls, key, onClick, { label = '', title = '', enabled = true } = {}) {
  const b = el('button', {
    class: cls,
    text,
    title: title || undefined,
    attrs: { type: 'button', 'aria-label': label || null },
    data: { key, nobusy: '1' },
  });
  if (!enabled || st.busy) b.setAttribute('aria-disabled', 'true');
  b.addEventListener('click', () => {
    if (b.getAttribute('aria-disabled') === 'true') return;
    onClick();
  });
  return b;
}

/** Redraws every mounted host (new numbers, a new filter, busy or not). */
function draw() {
  const focus = captureFocus();
  for (const h of st.heads) if (h.isConnected) drawHead(h);
  for (const [id, h] of st.lists) {
    const list = LISTS.find((l) => l.id === id);
    if (h.isConnected && list) drawList(h, list);
  }
  restoreFocus(focus);
}

function drawHead(host) {
  const s = shownSummary();
  // Label and numbers change together: both describe the summary's selection
  // (the current one only until the first summary arrives).
  const sc = s && st.summaryScope ? st.summaryScope : scope();
  const nothing = !sc.rowIds.length && !sc.accounts.length && !sc.categories.length;
  const box = el('div', { class: 'apply-head' });
  if (nothing) {
    box.append(
      el('p', { class: 'apply-scope', text: 'Nothing selected' }),
      el('p', { class: 'apply-hint', text: 'Select lines, an account or a category to apply items to them.' }),
    );
  } else if (s && !s.rows.length) {
    box.append(
      el('p', { class: 'apply-scope', text: scopeLabel(sc, s) }),
      el('p', { class: 'apply-hint', text: 'There are no lines here yet.' }),
    );
  } else {
    box.append(el('p', { class: 'apply-scope', text: scopeLabel(sc, s), title: scopeLabel(sc, s) }));
    const t = obj(obj(s && s.totals).lines);
    const stats = el('p', { class: 'apply-stats' });
    stats.append(
      stat('Lines', s ? money(t.details_total) : ''),
      stat('Fringes', s ? money(t.fringe_total) : ''),
      stat('Effective', !s ? '' : s.fringe_percent === null ? 'n/a' : `${twoPlaces(s.fringe_percent)}%`, 'Fringe dollars as a percent of the line totals'),
    );
    box.append(stats);
  }
  if (st.error) box.append(el('p', { class: 'setup-error apply-error', text: st.error, attrs: { role: 'alert' } }));
  host.replaceChildren(box);
}

function stat(label, value, title = '') {
  return el(
    'span',
    { class: 'apply-stat', title: title || undefined },
    el('span', { class: 'apply-stat-label', text: label }),
    el('span', { class: 'apply-num', text: value }),
  );
}

/** An empty summary: nothing selected (or not known yet), so nothing can be applied. */
const NO_SUMMARY = { rows: [], lines: 0, costed: 0, in_range: 0, range_blocked: '', fringe_percent: null, totals: {} };

function drawList(host, list) {
  const s = shownSummary() || NO_SUMMARY;
  const defs = obj(list.defs(budget()));
  const names = Object.keys(defs);
  const eligible = Number(list.eligible(s)) || 0;
  host.setAttribute('aria-busy', st.busy ? 'true' : 'false');
  wantUsage(st.view);
  const parts = [];

  if (names.length) parts.push(tools(list, s, eligible));
  if (names.length && s.rows.length && !eligible) parts.push(el('p', { class: 'apply-hint', text: list.noTakers }));

  const q = str(st.filters[list.id]).trim().toLowerCase();
  const only = Boolean(st.onlyApplied[list.id]);
  const { names: shown, filtered } = listNames(
    st.view,
    list.id,
    (n) => {
      const def = obj(defs[n]);
      if (only && tally(s, list.id, n).count === 0) return false;
      return !q || n.toLowerCase().includes(q) || str(def.description).toLowerCase().includes(q) || str(def.short_code).toLowerCase().includes(q);
    },
    { textFilter: Boolean(q) || only },
  );
  const movable = canReorder(list.id, filtered);
  const ul = el('ul', { class: 'apply-items', attrs: { 'aria-label': list.label }, data: { list: list.id } });
  for (const n of shown) ul.append(item(list, s, n, obj(defs[n]), eligible, movable));
  ul.append(addRow(list));
  if (names.length) parts.push(colHead(list));
  if (!names.length) parts.push(el('p', { class: 'setup-empty', text: list.none }));
  else if (!shown.length) parts.push(el('p', { class: 'setup-empty', text: only ? 'None of these are on the selection.' : 'Nothing matches the filter.' }));
  parts.push(ul);
  if (list.id === 'fringes' && s.rows.length) parts.push(fringeFoot(s));
  host.replaceChildren(...parts);
}

function tools(list, s, eligible) {
  const row = el('div', { class: 'apply-tools' });
  const input = el('input', {
    class: 'setup-input apply-filter',
    attrs: {
      type: 'text',
      placeholder: 'Filter',
      'aria-label': `Filter ${list.label.toLowerCase()}`,
      autocomplete: 'off',
      spellcheck: 'false',
      autocapitalize: 'off',
      autocorrect: 'off',
    },
    data: { key: `apply:filter:${list.id}` },
  });
  input.value = str(st.filters[list.id]);
  input.addEventListener('input', (e) => {
    st.filters[list.id] = input.value;
    if (!e.isComposing) draw();
  });
  input.addEventListener('compositionend', () => {
    st.filters[list.id] = input.value;
    draw();
  });
  input.addEventListener('keydown', (e) => {
    if (e.key === 'Escape' && input.value) {
      e.preventDefault();
      e.stopPropagation();
      st.filters[list.id] = '';
      draw();
    }
  });
  const only = Boolean(st.onlyApplied[list.id]);
  const onlyBtn = button('Applied only', 'apply-pill', `apply:only:${list.id}`, () => {
    st.onlyApplied[list.id] = !only;
    draw();
  }, { title: 'Show only the items on the selected lines' });
  onlyBtn.setAttribute('aria-pressed', String(only));
  onlyBtn.removeAttribute('aria-disabled'); // a view toggle, never busy
  row.append(input, showSelect(list.id, draw, `apply:show:${list.id}`), onlyBtn);

  if (!list.multi) {
    const base = str(budget().base_currency);
    const hasValue =
      list.id === 'currencies'
        ? tally(s, 'currencies', base).count < eligible
        : Object.keys(obj(s[list.id])).some((k) => tally(s, list.id, k).count > 0);
    const what = list.what;
    row.append(
      button('Clear', 'apply-pill', `apply:clear:${list.id}`, () => clearValue(list), {
        label: list.id === 'currencies' ? `Use the base currency on the selected lines` : `Clear the ${what} on the selected lines`,
        title: list.id === 'currencies' ? `Back to the base currency${base ? `, ${base}` : ''}` : `Take the ${what} off the selected lines`,
        enabled: eligible > 0 && hasValue,
      }),
    );
  }
  return row;
}

/** The list's column heads: the name (a click cycles its sort) and what the numbers are. */
function colHead(list) {
  return el(
    'div',
    { class: 'apply-colhead' },
    el('span'),
    el(
      'span',
      { class: 'apply-colhead-name' },
      sortButton(list.id, 'Name', draw, `apply:sort:${list.id}`),
      el('span', { class: 'apply-colhead-num', text: 'Selected', title: 'Dollars on the selected lines' }),
    ),
    el('span', { class: 'apply-colhead-num', text: 'Lines', title: 'Lines using it in the whole budget; click a count to find them' }),
    el('span', { class: 'apply-colhead-num', text: 'Budget', title: 'Its dollars in the whole budget' }),
  );
}

/** What an item adds up to across the whole budget, for its tooltip. */
function budgetWide(list, name, def) {
  const r = report();
  if (list.id === 'fringes') {
    const f = obj(r.fringes)[name];
    if (!f) return '';
    const uses = Number(f.uses) || 0;
    return uses ? `Whole budget: ${plural(uses, 'line', 'lines')}, ${money(f.total)}` : 'Not used in the budget';
  }
  if (list.id === 'groups') {
    const g = obj(obj(r.groups)[name]);
    const bd = def.in_budget_total === false ? g.full : g.active;
    if (!bd) return '';
    const total = (Number(bd.details_total) || 0) + (Number(bd.fringe_total) || 0);
    return `Whole budget: ${money(total)}${def.in_budget_total === false ? ', not in the total' : ''}`;
  }
  if (list.id === 'sets' || list.id === 'locations') {
    const bd = obj(r[list.id])[name];
    if (!bd) return 'Not used in the budget';
    return `Whole budget: ${money((Number(bd.details_total) || 0) + (Number(bd.fringe_total) || 0))}`;
  }
  return '';
}

function item(list, s, name, def, eligible, movable = false) {
  const t = tally(s, list.id, name);
  const state = stateOf(t.count, eligible);
  const infoId = nextId();
  const off = list.id === 'groups' && def.in_budget_total === false;
  const amount = off ? 'Not in total' : t.count ? money(t.total) : '';
  const info = [];
  if (state === 'some') info.push(`on ${t.count} of ${plural(eligible, 'line', 'lines')}`);
  if (str(def.description)) info.push(str(def.description));
  const key = itemKey(list.id, name);
  const tip = [str(def.description), budgetWide(list, name, def), 'Click the box to apply; double-click the name to edit'].filter(Boolean).join('\n');

  const b = el(
    'button',
    {
      class: `apply-check${list.multi ? '' : ' apply-radio'}`,
      title: tip,
      attrs: {
        type: 'button',
        role: 'checkbox',
        'aria-checked': ARIA[state],
        'aria-label': name,
        'aria-describedby': infoId,
        'aria-keyshortcuts': `Space Enter F2 Delete ${MOVE_KEYS}`,
      },
      data: { key, nobusy: '1' },
    },
    el('span', { class: 'apply-boxhit', attrs: { 'aria-hidden': 'true' } }, el('span', { class: 'apply-box' })),
    el(
      'span',
      { class: 'apply-name' },
      name,
      str(def.short_code) ? el('span', { class: 'apply-code', text: str(def.short_code), title: 'Short code: the grid shows it for this item' }) : null,
    ),
    el('span', { class: 'apply-desc', text: str(def.description) }),
    state === 'some' ? el('span', { class: 'apply-count', text: `${t.count}/${eligible}`, attrs: { 'aria-hidden': 'true' } }) : null,
    el('span', { class: `apply-num apply-amt${off ? ' apply-off' : ''}`, text: amount }),
    el('span', { class: 'setup-sr', text: [info.join('; '), amount].filter(Boolean).join('. '), attrs: { id: infoId } }),
  );
  if (!eligible || st.busy) b.setAttribute('aria-disabled', 'true');
  b.addEventListener('click', (e) => {
    // The box (or Space) applies; a click on the name only selects the item.
    const onBox = e.detail === 0 || (e.target instanceof Element && e.target.closest('.apply-boxhit'));
    select(list.id, name, b);
    if (!onBox || b.getAttribute('aria-disabled') === 'true') return;
    toggle(list, name);
  });
  b.addEventListener('dblclick', (e) => {
    // Two clicks on the box are two ticks, as with any checkbox.
    if (e.target instanceof Element && e.target.closest('.apply-boxhit')) return;
    e.preventDefault();
    editItem(list, name);
  });
  // Right-click: Edit, Rename everywhere, Delete.
  b.addEventListener('contextmenu', (e) => {
    e.preventDefault();
    select(list.id, name, b);
    itemMenu(e, {
      kind: list.what,
      name,
      anchor: b,
      ctx: renameCtx(),
      edit: () => editItem(list, name),
      remove: () => deleteItem(list, name),
      extra: list.id === 'fringes' || list.id === 'groups' ? [{ label: 'Short code…', title: 'A short code the grid shows instead of the name', run: () => editCode(list, name) }] : [],
      onRenamed: (to) => {
        st.selected[list.id] = to;
        focusAfter(itemKey(list.id, to), list.id);
      },
    });
  });
  b.addEventListener('keydown', (e) => {
    if (moveKey(e, list.id, name, movable, (cmd, args) => call(cmd, args).then(() => focusAfter(key, list.id)))) return;
    const plain = !e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey;
    if ((e.key === 'Enter' || e.key === 'F2') && plain) {
      e.preventDefault();
      editItem(list, name);
      return;
    }
    if ((e.key === 'Delete' || e.key === 'Backspace') && plain) {
      e.preventDefault();
      deleteItem(list, name);
      return;
    }
    moveFocus(e, b);
  });
  const total = totalText(st.view, list.id, name);
  const li = el(
    'li',
    { class: `apply-item apply-${state}`, data: { name } },
    dragHandle(list.id, name, movable, (cmd, args) => call(cmd, args)),
    b,
    usesButton(st.view, list.id, name, `apply:uses:${list.id}:${name}`),
    el('span', { class: `apply-num apply-total${off ? ' apply-off' : ''}`, text: total, title: off && total ? 'Not in the budget total' : undefined }),
  );
  if (st.selected[list.id] === name) li.classList.add('is-selected');
  return li;
}

/** Up and Down (Home and End) move between a list's items and its add row. */
function moveFocus(e, from) {
  const ul = from.closest('.apply-items');
  if (!ul) return;
  const items = [...ul.querySelectorAll('.apply-check, .apply-add-btn')];
  const i = items.indexOf(from);
  let j = null;
  if (e.key === 'ArrowDown') j = i + 1;
  else if (e.key === 'ArrowUp') j = i - 1;
  else if (e.key === 'Home') j = 0;
  else if (e.key === 'End') j = items.length - 1;
  if (j === null || !items[j]) return;
  e.preventDefault();
  const to = items[j];
  const listId = ul.dataset.list;
  const name = to.getAttribute('aria-label');
  if (to.classList.contains('apply-check') && listId && name !== null) select(listId, name, to);
  else to.focus();
}

/** Marks item `name` as the one picked in its list, without redrawing. */
function select(listId, name, b) {
  st.selected[listId] = name;
  const ul = b.closest('.apply-items');
  if (ul) for (const li of ul.querySelectorAll('.apply-item.is-selected')) li.classList.remove('is-selected');
  const li = b.closest('.apply-item');
  if (li) li.classList.add('is-selected');
  if (document.activeElement !== b) b.focus({ preventScroll: true });
}

function addRow(list) {
  const b = el('button', {
    class: 'apply-add-btn',
    text: `New ${list.what}…`,
    title: `Add a ${list.what} to this budget`,
    attrs: { type: 'button' },
    data: { key: addKey(list.id), nobusy: '1' },
    on: { click: () => addItem(list) },
  });
  b.addEventListener('keydown', (e) => moveFocus(e, b));
  return el('li', { class: 'apply-add' }, b);
}

function fringeFoot(s) {
  const foot = el('div', { class: 'apply-foot' });
  const t = obj(obj(s.totals).lines);
  foot.append(
    el(
      'p',
      { class: 'apply-stats' },
      stat('Fringe total', money(t.fringe_total)),
      stat('Effective', s.fringe_percent === null ? 'n/a' : `${twoPlaces(s.fringe_percent)}%`, 'Fringe dollars as a percent of the line totals'),
    ),
  );
  const ranged = Number(s.in_range) || 0;
  const range = el('div', { class: 'apply-range', attrs: { role: 'group', 'aria-label': 'Fringe range' } });
  range.append(
    button('Make range', 'setup-btn apply-btn', 'apply:range:make', makeRange, {
      title: s.range_blocked ? `Can't: ${s.range_blocked}` : 'Make the selected lines one earner: percent fringe cutoffs apply to their combined total',
      enabled: s.range_blocked === null,
    }),
    button('Remove range', 'setup-btn apply-btn', 'apply:range:remove', removeRange, {
      title: ranged ? 'Take the selected lines out of their fringe range' : 'None of the selected lines are in a fringe range',
      enabled: ranged > 0,
    }),
  );
  if (ranged) range.append(el('span', { class: 'apply-hint', text: `${plural(ranged, 'row', 'rows')} in a range` }));
  foot.append(range);
  return foot;
}

// ---------------------------------------------------------------------------
// Editing, adding and deleting an item where it is listed

/** Money typed as "176,000.00" or "(12.50)" as plain text for a command; null when blank. */
function moneyText(s) {
  let t = str(s).replace(/[,\s]/g, '');
  if (t === '') return null;
  const m = /^\((.*)\)$/.exec(t);
  if (m) t = `-${m[1]}`;
  return t;
}

/** Units with hours (what a flat fringe can be charged per), plus `current`. */
function timeUnits(current) {
  const units = obj(budget().units);
  const out = orderedNames(budget(), 'units')
    .filter((n) => {
      const u = obj(units[n]);
      return u.hours !== undefined && u.hours !== null && u.hours !== '';
    });
  if (current && !out.includes(current)) out.push(current);
  return out;
}

const DECIMAL = /^[+-]?(\d+\.?\d*|\.\d+)$/;

function fringeFields(def) {
  const base = str(budget().base_currency);
  const unit = def.unit === undefined || def.unit === null ? '' : String(def.unit);
  return [
    { key: 'kind', label: 'Kind', type: 'select', options: [['percent', 'Percent of pay'], ['flat', 'Flat amount']], value: def.kind === 'flat' ? 'flat' : 'percent' },
    { key: 'rate', label: 'Rate', value: str(def.rate), placeholder: '18.5', suffix: (v) => (v.kind === 'flat' ? base : '%') },
    {
      key: 'cutoff',
      label: 'Cutoff (pay per earner)',
      value: def.cutoff === undefined || def.cutoff === null ? '' : money(def.cutoff),
      placeholder: 'None',
      showIf: (v) => v.kind !== 'flat',
    },
    {
      key: 'unit',
      label: 'Charged per',
      type: 'select',
      options: [['', 'Line'], ...timeUnits(unit).map((u) => [u, u])],
      value: unit,
      showIf: (v) => v.kind === 'flat',
    },
    { key: 'description', label: 'Description', value: str(def.description) },
    { key: 'short_code', label: 'Short code (shown in the grid)', value: str(def.short_code), placeholder: 'None' },
  ];
}

function fringeInput(v) {
  return {
    kind: v.kind === 'flat' ? 'flat' : 'percent',
    description: v.description,
    rate: v.rate.trim(),
    cutoff: v.kind === 'flat' ? null : moneyText(v.cutoff),
    unit: v.kind === 'flat' && v.unit ? v.unit : null,
    short_code: str(v.short_code).trim(),
  };
}

// Per list: the editor's fields for item `name` (its definition `def`), and
// build(values) -> [command, args]. One command each, so one undo step.
const EDITORS = {
  fringes: (name, def) => ({
    fields: fringeFields(def),
    build: (v) => ['set_fringe', { name, fringe: fringeInput(v) }],
  }),
  groups: (name, def) => {
    const current = typeof def.color === 'string' ? def.color.toLowerCase() : '';
    const ours = paletteColor(current);
    const options = [{ id: '', label: 'No color', cls: 'sw-none' }, ...PALETTE.map((c) => ({ id: c.id, label: c.label, cls: `sw-${c.id}` }))];
    if (current && !ours) options.push({ id: 'keep', label: `Keep its color (${current})`, cls: 'sw-keep' });
    return {
      fields: [
        { key: 'description', label: 'Description', value: str(def.description) },
        { key: 'short_code', label: 'Short code (shown in the grid)', value: str(def.short_code), placeholder: 'None' },
        { key: 'counted', label: 'Counts in total', type: 'check', value: def.in_budget_total !== false },
        { key: 'color', label: 'Line color', type: 'swatches', options, value: ours ? ours.id : current ? 'keep' : '' },
      ],
      build: (v) => {
        const picked = PALETTE.find((c) => c.id === v.color);
        return [
          'update_group',
          {
            name,
            group: { description: v.description, in_budget_total: Boolean(v.counted), short_code: str(v.short_code).trim() },
            color: v.color === 'keep' ? current : picked ? picked.light : null,
          },
        ];
      },
    };
  },
  sets: (name, def) => ({
    fields: [{ key: 'description', label: 'Description', value: str(def.description) }],
    build: (v) => ['set_set', { name, set: { description: v.description } }],
  }),
  locations: (name, def) => ({
    fields: ['description', 'city', 'region', 'country'].map((k) => ({ key: k, label: k[0].toUpperCase() + k.slice(1), value: str(def[k]) })),
    build: (v) => ['set_location', { name, location: { description: v.description, city: v.city, region: v.region, country: v.country } }],
  }),
  currencies: (name, def) => {
    const base = str(budget().base_currency);
    return {
      fields: [
        { key: 'rate', label: `Rate (${base || 'base'} per 1 ${name || 'unit'})`, value: str(def.rate), placeholder: '1.08' },
        { key: 'symbol', label: 'Symbol', value: str(def.symbol) },
        { key: 'description', label: 'Description', value: str(def.description) },
      ],
      build: (v) => ['set_currency', { code: name, currency: { description: v.description, symbol: v.symbol, rate: v.rate.trim() } }],
    };
  },
};

/** What a new item of each list asks for, and the command that adds it. */
const ADDERS = {
  fringes: {
    fields: () => fringeFields({}),
    build: (name, v) => ['set_fringe', { name, fringe: fringeInput(v) }],
    check: (v) => (v.rate.trim() === '' ? 'Enter a rate.' : null),
  },
  groups: {
    fields: () => [
      { key: 'description', label: 'Description', value: '' },
      { key: 'short_code', label: 'Short code (shown in the grid)', value: '', placeholder: 'None' },
      { key: 'counted', label: 'Counts in total', type: 'check', value: true },
    ],
    build: (name, v) => ['set_group', { name, group: { description: v.description, in_budget_total: Boolean(v.counted), short_code: str(v.short_code).trim() } }],
  },
  sets: {
    fields: () => [{ key: 'description', label: 'Description', value: '', placeholder: 'Interior, night' }],
    build: (name, v) => ['set_set', { name, set: { description: v.description.trim() } }],
  },
  locations: {
    fields: () => EDITORS.locations('', {}).fields,
    build: (name, v) => ['set_location', { name, location: { description: v.description, city: v.city, region: v.region, country: v.country } }],
  },
  currencies: {
    fields: () => EDITORS.currencies('', {}).fields,
    build: (code, v) => ['set_currency', { code, currency: { description: v.description, symbol: v.symbol, rate: v.rate.trim() } }],
    check: (v) => {
      const rate = v.rate.trim();
      if (rate === '') return 'Enter a rate.';
      if (!DECIMAL.test(rate) || Number(rate) <= 0) return 'The rate must be a number above 0, like 1.08.';
      return null;
    },
  },
};

/** Why `name` can't be a new item of `list`, or null. */
function badName(list, name) {
  const defs = obj(list.defs(budget()));
  const has = Object.prototype.hasOwnProperty.call(defs, name);
  if (list.id === 'currencies') {
    if (name === '') return 'Enter a currency code.';
    if (name.includes(',')) return 'Codes can’t contain commas.';
    if (name === str(budget().base_currency)) return `${name} is the base currency.`;
    if (has) return `${name} is already in the list.`;
    return null;
  }
  if (name === '') return `Enter a ${list.what} name.`;
  if (name.includes(',')) return 'Names can’t contain commas.';
  if (has) return `There is already a ${list.what} named ${name}.`;
  return null;
}

/** Focuses item `key` once the panel has redrawn, or the list's add row. */
function focusAfter(key, listId) {
  const b = byKey(key) || byKey(addKey(listId));
  if (b) b.focus({ preventScroll: false });
}

/** What rename.js needs from this panel. */
function renameCtx() {
  return {
    call: (cmd, args) => call(cmd, args),
    toast: (m) => st.ctx && typeof st.ctx.toast === 'function' && st.ctx.toast(m),
    budget: () => budget(),
  };
}

/**
 * Opens the editor for item `name` of `list` next to it. Its first field is
 * the name (a currency's code): a new one renames the item everywhere first
 * (rename.js; its own undo step), then any other change is saved under it.
 */
function editItem(list, name) {
  const defs = obj(list.defs(budget()));
  const spec = EDITORS[list.id];
  if (!spec || !Object.prototype.hasOwnProperty.call(defs, name)) return;
  const def = obj(defs[name]);
  const { fields, build } = spec(name, def);
  const key = itemKey(list.id, name);
  const start = Object.fromEntries(fields.map((f) => [f.key, f.type === 'check' ? Boolean(f.value) : str(f.value)]));
  const unchanged = JSON.stringify(build(start));
  const what = list.what[0].toUpperCase() + list.what.slice(1);
  const nameField = { key: 'newName', label: list.id === 'currencies' ? 'Code' : 'Name', value: name };
  let deleting = false;
  let now = name;
  anchoredForm({
    anchor: byKey(key),
    title: `${what} ${name}`,
    fields: [nameField, ...fields],
    extra: {
      text: 'Delete…',
      title: `Delete this ${list.what}`,
      run: () => {
        deleting = true;
      },
    },
    onSubmit: async (v) => {
      const to = str(v.newName).trim();
      const other = JSON.stringify(build(v)) !== unchanged;
      if (to !== now) {
        const bad = renameProblem(list.what, now, to, budget());
        if (bad) throw bad;
        const reply = await call('rename_setup_item', { kind: list.what, from: now, to });
        renameCtx().toast(renamedText(list.what, now, reply));
        now = reply && typeof reply.name === 'string' ? reply.name : to;
        if (st.selected[list.id] === name) st.selected[list.id] = now;
      }
      // Nothing else changed: no command, no undo step.
      if (!other) return;
      const step = spec(now, def).build(v);
      await call(step[0], step[1]);
    },
  }).then(() => {
    if (deleting) deleteItem(list, name);
    else focusAfter(itemKey(list.id, now), list.id);
  });
}

/** Sets or clears a fringe's or group's short code, in a small form next to it (one undo step). */
function editCode(list, name) {
  const defs = obj(list.defs(budget()));
  if (!Object.prototype.hasOwnProperty.call(defs, name)) return;
  const key = itemKey(list.id, name);
  anchoredForm({
    anchor: byKey(key),
    title: `Short code for ${list.what} ${name}`,
    message: 'The grid shows it instead of the name. Leave it empty for none.',
    fields: [{ key: 'code', label: 'Short code', value: str(obj(defs[name]).short_code), placeholder: 'FICA' }],
    onSubmit: (v) => call('set_short_code', { kind: list.what, name, code: str(v.code).trim() }),
  }).then(() => focusAfter(key, list.id));
}

/** The question to ask before deleting, and the command. */
function deletePlan(list, name, def) {
  const r = report();
  const use = inUse(st.view, list.id, name);
  // The fringe's own count is in every view; the others wait for the counts.
  const lines = list.id === 'fringes' ? Number(obj(obj(r.fringes)[name]).uses) || use.lines : use.lines;
  const used = lines ? `${plural(lines, 'line uses', 'lines use')} it. ` : '';
  const removeText = lines ? `Remove from ${plural(lines, 'line', 'lines')} and delete` : 'Delete';
  if (list.id === 'fringes') {
    return {
      message: lines ? `${used}Deleting takes it off them.` : 'No lines use it.',
      submitText: removeText,
      step: ['delete_fringe', { name }],
    };
  }
  if (list.id === 'groups') {
    const back = def.in_budget_total === false ? ' and they count in the total again' : '';
    return {
      message: lines ? `${used}Deleting takes it off them${back}.` : 'No lines use it.',
      submitText: removeText,
      step: ['delete_group', { name }],
    };
  }
  if (list.id === 'sets' || list.id === 'locations') {
    return {
      message: lines ? `${used}Deleting takes it off them.` : 'No lines use it.',
      submitText: removeText,
      step: [list.id === 'sets' ? 'delete_set' : 'delete_location', { name }],
    };
  }
  const base = str(budget().base_currency);
  return {
    message: lines
      ? `${plural(lines, 'line is', 'lines are')} in ${name}. Deleting puts ${lines === 1 ? 'it' : 'them'} back in ${base || 'the base currency'}. Charges and the totals currency must not use it.`
      : 'It can’t be deleted while charges or the totals currency use it.',
    submitText: lines ? `Move ${plural(lines, 'line', 'lines')} and delete` : 'Delete',
    step: ['delete_currency', { code: name, clearLines: lines > 0 }],
  };
}

/** Asks, next to the item, before deleting it; one command, one undo step. */
function deleteItem(list, name) {
  const defs = obj(list.defs(budget()));
  if (!Object.prototype.hasOwnProperty.call(defs, name)) return;
  const key = itemKey(list.id, name);
  // Focus goes to the next item after a delete (else the one before, else the add row).
  const ul = byKey(key) && byKey(key).closest('.apply-items');
  const order = ul ? [...ul.querySelectorAll('.apply-check')].map((b) => b.dataset.key) : [];
  const i = order.indexOf(key);
  const neighbor = order[i + 1] || order[i - 1] || addKey(list.id);
  const { message, step, submitText } = deletePlan(list, name, obj(defs[name]));
  let done = false;
  anchoredForm({
    anchor: byKey(key),
    title: `Delete ${list.what} ${name}?`,
    message,
    submitText: submitText || 'Delete',
    danger: true,
    onSubmit: () =>
      call(step[0], step[1]).then(() => {
        done = true;
        if (st.selected[list.id] === name) delete st.selected[list.id];
      }),
  }).then(() => focusAfter(done ? neighbor : key, list.id));
}

/** Opens the form for a new item of `list`, next to its add row. */
function addItem(list) {
  const spec = ADDERS[list.id];
  if (!spec) return;
  const nameField = list.id === 'currencies'
    ? { key: 'name', label: 'Code', value: '', placeholder: 'EUR' }
    : { key: 'name', label: 'Name', value: '' };
  let added = null;
  anchoredForm({
    anchor: byKey(addKey(list.id)),
    title: `New ${list.what}`,
    fields: [nameField, ...spec.fields()],
    submitText: 'Add',
    onSubmit: (v) => {
      const name = v.name.trim();
      const bad = badName(list, name) || (spec.check ? spec.check(v) : null);
      if (bad) return Promise.reject(bad);
      const [cmd, args] = spec.build(name, v);
      return call(cmd, args).then(() => {
        added = name;
        st.selected[list.id] = name;
      });
    },
  }).then(() => focusAfter(added === null ? addKey(list.id) : itemKey(list.id, added), list.id));
}
