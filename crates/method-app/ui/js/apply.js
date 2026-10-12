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
// The Budget Setup window (setupwin.js) draws the same lists as setup tables
// (ctx.mode 'setup'; "Setup tables" below): one dense table each, with every
// column of each item's definition, most edited in place, an action strip,
// the footer's totals, and the same editing, adding, deleting, renaming and
// ordering, but no selection, so no summary is fetched there and nothing is
// applied. In the tools panel each list shows the item's key
// figure (a fringe's rate, a currency's rate) and a Setup button and
// right-click item that open the item in Budget Setup (ctx.openSetup).
//
// tools.js calls applyView() on every render, then mounts what it shows:
// applyHeadHost() (what is selected, with its totals) and applyListHost(id)
// for each list tab on screen. A fresh summary redraws just those. ctx.call
// runs a command (main.js's, or the tools window's own) and ctx.selection()
// gives the grid's selection; a "method:selection" event on the document says it
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
  usageOf,
  totalText,
  dragHandle,
  moveKey,
  MOVE_KEYS,
  inUse,
  redrawLists,
} from './setuplist.js';
import { saveItemToLibrary } from './library.js';
import { mountFrame, actionStrip, stripButton, searchBox, band, bandField, gridTable, setFooterTotals } from './setupframe.js';
import { currencyBand } from './setupbands.js';
import { obj, str, plural, errText, sourceText, FLAT_BASES, isHourUnit, flatCountWords } from './util.js';

const LISTS = [
  {
    id: 'fringes',
    label: 'Fringes',
    what: 'fringe',
    field: 'fringes',
    multi: true,
    defs: (b) => b.fringes,
    // Lines and adjustment rows take fringes (decisions.md, entry 56).
    eligible: (s) => s.costed,
    noTakers: 'Subtotal rows take no fringes.',
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

// True while draw() swaps a host's content: a removed setup table cell's
// blur (some engines send one) must not commit half-typed text.
let swapping = false;

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

document.addEventListener('method:selection', () => {
  if (visible()) schedule();
});

// ---------------------------------------------------------------------------
// Small helpers

function budget() {
  return obj(st.view && st.view.budget);
}

function report() {
  return obj(st.view && st.view.report);
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

/** True in the Budget Setup window: setup tables, nothing to apply. */
function setupMode() {
  return Boolean(st.ctx && st.ctx.mode === 'setup');
}

/** A rate with at least two places: "18.5" -> "18.50", "0.125" stays. */
function rateText(s) {
  const m = /^(-?)(\d+)(?:\.(\d+))?$/.exec(str(s));
  if (!m) return str(s);
  const frac = m[3] || '';
  return `${m[1]}${m[2]}.${frac.length >= 2 ? frac : frac.padEnd(2, '0')}`;
}

/** A fringe's rate as the lists show it: "18.50%", "25.00 USD per Day". */
function fringeRate(def) {
  if (str(def.rate) === '') return '';
  if (def.kind !== 'flat') return `${rateText(def.rate)}%`;
  const base = str(budget().base_currency);
  const unit = str(def.unit);
  return `${rateText(def.rate)}${base ? ` ${base}` : ''} per ${unit || 'line'}${flatCountWords(def, budget().units)}`;
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
  // The setup window has no selection: never a summary, never applying.
  if (setupMode() || !visible()) return;
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

/** Takes every fringe (or group) off the selected lines. */
function clearAll(list) {
  act((s) => {
    if (!list.eligible(s)) return null;
    const next = { ...s, [list.id]: {} };
    for (const [k, t] of Object.entries(obj(s[list.id]))) next[list.id][k] = { ...t, count: 0, total: 0 };
    return ['clear_from_rows', { rowIds: s.rows, field: list.field }, next];
  });
}

/**
 * Replace tags: a form like a tag preset's editor that sets exactly the
 * ticked fringes and groups and the picked set, location and currency on the
 * selected lines. It starts from what every selected line already shares.
 */
async function replaceTags(anchor) {
  if (st.busy) return;
  const s = await freshSummary();
  if (!s || !s.rows.length || !s.costed) return;
  const b = budget();
  const base = str(b.base_currency);
  const fields = [];
  const shared = (listId, eligible) =>
    Object.keys(obj(s[listId])).filter((n) => eligible > 0 && tally(s, listId, n).count >= eligible);
  const checks = (kind, label, defs, on) => {
    for (const n of orderedNames(b, kind === 'f' ? 'fringes' : 'groups')) {
      if (!Object.prototype.hasOwnProperty.call(obj(defs), n)) continue;
      fields.push({ key: `${kind}:${n}`, label: `${label} ${n}`, type: 'check', value: on.includes(n) });
    }
  };
  checks('f', 'Fringe', b.fringes, shared('fringes', s.costed));
  checks('g', 'Group', b.groups, shared('groups', s.costed));
  const pick = (key, label, listId, defs, eligible, noneLabel) => {
    const names = orderedNames(b, listId).filter((n) => Object.prototype.hasOwnProperty.call(obj(defs), n));
    const one = shared(listId, eligible).find((n) => names.includes(n)) || '';
    fields.push({ key, label, type: 'select', options: [['', noneLabel], ...names.map((n) => [n, n])], value: one });
  };
  pick('set', 'Set', 'sets', b.sets, s.costed, 'None');
  pick('location', 'Location', 'locations', b.locations, s.costed, 'None');
  pick('currency', 'Currency', 'currencies', b.currencies, s.lines, `${base || 'Base'} (base currency)`);
  const rows = s.rows;
  await anchoredForm({
    anchor,
    title: 'Replace tags',
    message: `${scopeLabel(st.summaryScope || scope(), s)}: their fringes, groups, set, location and currency become exactly these.`,
    fields,
    submitText: 'Replace',
    onSubmit: (v) => {
      const chosen = (prefix) =>
        Object.keys(v)
          .filter((k) => k.startsWith(prefix) && v[k] === true)
          .map((k) => k.slice(prefix.length));
      const tags = { fringes: chosen('f:'), groups: chosen('g:') };
      for (const k of ['set', 'location', 'currency']) tags[k] = v[k] ? v[k] : null;
      st.error = '';
      return call('replace_tags_on_rows', { rowIds: rows, tags });
    },
  });
  if (anchor.isConnected) anchor.focus();
  else {
    const again = byKey('apply:replace-tags');
    if (again) again.focus();
  }
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
  // A cell being typed in (setup tables) keeps its text.
  if (a.dataset.orig !== undefined && a.value !== a.dataset.orig) info.value = a.value;
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
  if (info.value !== undefined && 'value' in target) target.value = info.value;
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
  swapping = true;
  try {
    for (const h of st.heads) if (h.isConnected) drawHead(h);
    for (const [id, h] of st.lists) {
      const list = LISTS.find((l) => l.id === id);
      if (h.isConnected && list) drawList(h, list);
    }
  } finally {
    swapping = false;
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
    const replace = button('Replace tags…', 'setup-btn apply-btn', 'apply:replace-tags', () => replaceTags(replace), {
      title: 'Set exactly the fringes, groups, set, location and currency the selected lines carry',
      enabled: Boolean(s && s.costed),
    });
    box.append(el('div', { class: 'apply-range' }, replace));
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
  if (setupMode()) {
    drawSetupList(host, list);
    return;
  }
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

  if (list.multi) {
    const what = list.id === 'fringes' ? 'fringes' : 'groups';
    const any = Object.keys(obj(s[list.id])).some((k) => tally(s, list.id, k).count > 0);
    row.append(
      button('Clear', 'apply-pill', `apply:clear:${list.id}`, () => clearAll(list), {
        label: `Take every ${list.what} off the selected lines`,
        title: `Take all ${what} off the selected lines`,
        enabled: eligible > 0 && any,
      }),
    );
  } else {
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
  // Last: the list's table in Budget Setup.
  const setup = setupButton(list);
  if (setup) row.append(setup);
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
    // Without fringes when the group rule says so (display only).
    const fringes = groupRules().fringes_in_totals === 'exclude' ? 0 : Number(bd.fringe_total) || 0;
    const total = (Number(bd.details_total) || 0) + fringes;
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
    keyFigure(list, name, def),
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
      extra: [
        ...(list.id === 'fringes' || list.id === 'groups' ? [{ label: 'Short code…', title: 'A short code the grid shows instead of the name', run: () => editCode(list, name) }] : []),
        ...openInSetup(list, name),
      ],
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
      stat('Percent', money(s.percent_fringes || 0), 'Dollars from percentage fringes on the selected lines'),
      stat('Flat', money(s.flat_fringes || 0), 'Dollars from flat fringes on the selected lines'),
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
// The way to Budget Setup from the panel, and the panel's key figures

/** The tools row's Setup button: the list's table in Budget Setup. */
function setupButton(list) {
  if (!st.ctx || typeof st.ctx.openSetup !== 'function') return null;
  const b = button('Setup', 'apply-pill', `apply:setup:${list.id}`, () => st.ctx.openSetup(list.id, st.selected[list.id] || null), {
    label: `Edit ${list.label.toLowerCase()} in Budget Setup`,
    title: `Every column of ${list.label.toLowerCase()}, with the library, in the Budget Setup window`,
  });
  b.removeAttribute('aria-disabled'); // opens a window, never busy
  return b;
}

/** The right-click menu's way to item `name` in Budget Setup. */
function openInSetup(list, name) {
  if (!st.ctx || typeof st.ctx.openSetup !== 'function') return [];
  return [{ label: 'Edit in Budget Setup', title: `Show this ${list.what} in the Budget Setup window`, run: () => st.ctx.openSetup(list.id, name) }];
}

/**
 * What a panel row shows after the name: a fringe's rate, a currency's rate,
 * else the description (always in the row's tooltip too).
 */
function keyFigure(list, name, def) {
  let text = str(def.description);
  if (list.id === 'fringes') text = fringeRate(def);
  else if (list.id === 'currencies' && str(def.rate)) text = `${rateText(def.rate)}`;
  return el('span', { class: 'apply-desc', text, title: str(def.description) || undefined });
}

// ---------------------------------------------------------------------------
// Setup tables (the Budget Setup window)
//
// Each list is one dense table (setupframe.js, setup.css): a row per item,
// its definition edited in place (a fringe's description, rate, unit and
// cutoff; a group's description, total switch and color; a set's,
// location's and currency's fields), its line count (a click finds them)
// and its dollars. The name is a button: a click selects the row,
// double-click, Enter or F2 opens the item's editor for the rest (a
// fringe's basis, short codes, renaming), Delete deletes it, and its
// right-click menu has the rest. Above the table the action strip (New,
// Edit, Delete and Save to Library for the selected row, and the search),
// and for groups and currencies a band of their budget-wide settings. The
// footer has the count and, for fringes, the budget's fringe dollars.
//
// A cell commits on Enter or when it loses focus (Escape puts the old value
// back), as one command built from the newest view when its turn comes
// (saveCell), so two quick edits to one item never undo each other.

/** The column heads of each table after the name: [id, label, title, numeric]. */
const SETUP_COLS = {
  fringes: [
    ['description', 'Description', ''],
    ['rate', 'Rate', 'A percent of pay, or a flat amount per line or per unit of time', true],
    ['unit', 'Unit', 'Percent of pay, or flat per line or per unit of time'],
    ['cutoff', 'Cutoff', 'The most pay per earner a percent fringe is figured on', true],
    ['uses', 'In use', 'Lines using it in the whole budget; click a count to find them', true],
    ['total', 'Total', 'Its fringe dollars in the whole budget', true],
  ],
  groups: [
    ['description', 'Description', ''],
    ['counted', 'In budget total', 'Whether its lines count in the budget total'],
    ['color', 'Color', 'The color its lines show in'],
    ['uses', 'In use', 'Lines using it in the whole budget; click a count to find them', true],
    ['total', 'Total', 'Its lines’ dollars, counted in the total or not', true],
    ['active', 'Active total', 'Its lines’ dollars that count in the budget total', true],
  ],
  sets: [
    ['description', 'Description', ''],
    ['uses', 'In use', 'Lines using it in the whole budget; click a count to find them', true],
    ['total', 'Total', 'Its lines’ dollars in the whole budget', true],
  ],
  locations: [
    ['description', 'Description', ''],
    ['city', 'City', ''],
    ['region', 'Region', ''],
    ['country', 'Country', ''],
    ['uses', 'In use', 'Lines using it in the whole budget; click a count to find them', true],
    ['total', 'Total', 'Its lines’ dollars in the whole budget', true],
  ],
  currencies: [
    ['description', 'Description', ''],
    ['symbol', 'Symbol', ''],
    ['uses', 'In use', 'Lines using it in the whole budget; click a count to find them', true],
    ['rate', 'Rate', 'What one unit is worth in the base currency', true],
    ['total', 'Total', 'Its lines’ dollars in the base currency', true],
  ],
};

function has(o, k) {
  return Object.prototype.hasOwnProperty.call(obj(o), k);
}

/** A group's dollars in the whole budget, `part` 'full' or 'active', with fringes unless the group rule leaves them out. */
function groupDollars(name, part) {
  const bd = obj(obj(obj(report().groups)[name])[part]);
  if (bd.details_total === undefined) return null;
  const fringes = groupRules().fringes_in_totals === 'exclude' ? 0 : Number(bd.fringe_total) || 0;
  return (Number(bd.details_total) || 0) + fringes;
}

/** A fringe's unit as the Unit cell picks it: 'percent', or 'flat:' and the unit it is charged per. */
function fringeUnitValue(def) {
  return def.kind === 'flat' ? `flat:${str(def.unit)}` : 'percent';
}

function fringeUnitOptions(def) {
  return [
    ['percent', 'Percent'],
    ['flat:', 'Flat per line'],
    ...timeUnits(def.kind === 'flat' ? str(def.unit) : '').map((u) => [`flat:${u}`, `Flat per ${u}`]),
  ];
}

/** Item `name`'s value in column `col`, for sorting. */
function setupSortValue(list, name, col) {
  const def = obj(obj(list.defs(budget()))[name]);
  const use = usageOf(st.view, list.id, name);
  if (col === 'uses') return use ? Number(use.lines) || 0 : null;
  if (col === 'total') {
    if (list.id === 'groups') return groupDollars(name, 'full');
    return use && use.total !== null && use.total !== undefined ? Number(use.total) : null;
  }
  if (col === 'active') return groupDollars(name, 'active');
  if (col === 'rate' || col === 'cutoff') {
    const n = Number(str(def[col]));
    return str(def[col]) === '' || Number.isNaN(n) ? null : n;
  }
  if (col === 'unit') return (fringeUnitOptions(def).find(([v]) => v === fringeUnitValue(def)) || ['', ''])[1];
  if (col === 'counted') return def.in_budget_total === false ? 0 : 1;
  if (col === 'color') {
    const named = paletteColor(typeof def.color === 'string' ? def.color : '');
    return named ? named.label : str(def.color);
  }
  return str(def[col]);
}

// ---------------------------------------------------------------------------
// Cells edited in place

let cellQueue = Promise.resolve();

/**
 * Runs a cell's edit after any before it: `build()` makes [command, args]
 * from the newest view when its turn comes. On a refusal, `undo()` puts
 * the cell back and the message shows under the table.
 */
function saveCell(build, undo) {
  st.error = '';
  cellQueue = cellQueue.then(async () => {
    try {
      const step = build();
      if (step) await call(step[0], step[1]);
    } catch (e) {
      st.error = errText(e);
      undo();
      draw();
    }
  });
}

/** A text cell: commits on Enter or blur, Escape puts the saved value (data-orig) back. */
function cellInput(key, value, label, save, { num = false, placeholder = '', title = '' } = {}) {
  const input = el('input', {
    class: `setup-input sw-cell-input${num ? ' setup-num sw-num' : ''}`,
    title: title || undefined,
    attrs: {
      type: 'text',
      'aria-label': label,
      placeholder: placeholder || null,
      autocomplete: 'off',
      spellcheck: 'false',
      autocapitalize: 'off',
      autocorrect: 'off',
    },
    data: { key, orig: value },
  });
  input.value = value;
  const commit = () => {
    if (swapping || !input.isConnected) return;
    const v = input.value;
    const old = input.dataset.orig;
    if (v === old) return;
    input.dataset.orig = v;
    save(v, () => {
      input.value = old;
      input.dataset.orig = old;
    });
  };
  input.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && !e.isComposing) {
      e.preventDefault();
      commit();
    } else if (e.key === 'Escape' && input.value !== input.dataset.orig) {
      e.preventDefault();
      e.stopPropagation();
      input.value = input.dataset.orig;
    }
  });
  input.addEventListener('blur', commit);
  return input;
}

/** A pick cell: saves on change. */
function cellSelect(key, value, options, label, save, title = '') {
  const sel = el('select', {
    class: 'setup-input setup-select sw-cell-input',
    title: title || undefined,
    attrs: { 'aria-label': label },
    data: { key, orig: value },
  });
  for (const [v, text] of options) sel.append(el('option', { text, attrs: { value: v } }));
  sel.value = value;
  sel.addEventListener('change', () => {
    const old = sel.dataset.orig;
    if (sel.value === old) return;
    sel.dataset.orig = sel.value;
    save(sel.value, () => {
      sel.value = old;
      sel.dataset.orig = old;
    });
  });
  return sel;
}

function cellKey(list, name, col) {
  return `apply:cell:${list.id}:${name}:${col}`;
}

/** Fringe `name` as set_fringe takes it, from the newest view, with `patch` (editor values) on top. */
function fringeStep(name, patch) {
  const defs = obj(budget().fringes);
  if (!has(defs, name)) throw `Fringe ${name} no longer exists.`;
  const start = fringeFields(obj(defs[name])).filter((f) => f.type !== 'note');
  const values = Object.fromEntries(start.map((f) => [f.key, str(f.value)]));
  const fringe = fringeInput({ ...values, ...patch });
  delete fringe.short_code; // left out: the fringe keeps its own
  return ['set_fringe', { name, fringe }];
}

/** Group `name` as update_group takes it, from the newest view, with `patch` on top. */
function groupStep(name, patch) {
  const defs = obj(budget().groups);
  if (!has(defs, name)) throw `Group ${name} no longer exists.`;
  const def = obj(defs[name]);
  const color = typeof def.color === 'string' && def.color ? def.color : null;
  return [
    'update_group',
    {
      name,
      group: {
        description: 'description' in patch ? patch.description : str(def.description),
        in_budget_total: 'counted' in patch ? patch.counted : def.in_budget_total !== false,
        short_code: str(def.short_code),
      },
      color: 'color' in patch ? patch.color : color,
    },
  ];
}

function locationStep(name, patch) {
  const defs = obj(budget().locations);
  if (!has(defs, name)) throw `Location ${name} no longer exists.`;
  const def = obj(defs[name]);
  const location = Object.fromEntries(['description', 'city', 'region', 'country'].map((k) => [k, str(def[k])]));
  return ['set_location', { name, location: { ...location, ...patch } }];
}

function currencyStep(code, patch) {
  const defs = obj(budget().currencies);
  if (!has(defs, code)) throw `Currency ${code} no longer exists.`;
  const def = obj(defs[code]);
  return ['set_currency', { code, currency: { description: str(def.description), symbol: str(def.symbol), rate: str(def.rate), ...patch } }];
}

/** Item `name`'s cell in column `col` of `list`. */
function setupCell(list, name, def, col) {
  const label = (what) => `${what} of ${list.what} ${name}`;
  const text = (field, step, opts = {}) =>
    cellInput(cellKey(list, name, field), opts.value !== undefined ? opts.value : str(def[field]), label(opts.label || field[0].toUpperCase() + field.slice(1)), (v, undo) => saveCell(() => step(v), undo), opts);
  if (col === 'uses') return usesButton(st.view, list.id, name, `apply:uses:${list.id}:${name}`);
  if (col === 'total' || col === 'active') {
    if (list.id === 'groups') {
      const v = groupDollars(name, col === 'total' ? 'full' : 'active');
      const off = col === 'active' && def.in_budget_total === false;
      return el('span', { class: `sw-num${off ? ' apply-off' : ''}`, text: v === null ? '' : money(v), title: off ? 'Not in the budget total' : undefined });
    }
    return el('span', { class: 'sw-num', text: totalText(st.view, list.id, name) });
  }
  if (list.id === 'fringes') {
    if (col === 'description') return text('description', (v) => fringeStep(name, { description: v }));
    if (col === 'rate') {
      return text('rate', (v) => fringeStep(name, { rate: v }), { value: str(def.rate) === '' ? '' : rateText(def.rate), num: true, title: fringeRate(def) });
    }
    if (col === 'unit') {
      return cellSelect(cellKey(list, name, 'unit'), fringeUnitValue(def), fringeUnitOptions(def), label('Unit'), (v, undo) =>
        saveCell(() => fringeStep(name, v === 'percent' ? { kind: 'percent' } : { kind: 'flat', unit: v.slice('flat:'.length) }), undo),
      );
    }
    if (col === 'cutoff') {
      const cutoff = def.cutoff === undefined || def.cutoff === null ? '' : money(def.cutoff);
      if (def.kind === 'flat') {
        return el('span', { class: 'sw-num apply-off', text: cutoff, title: cutoff ? 'Kept but not used: cutoffs apply to percent fringes only' : 'Cutoffs apply to percent fringes only' });
      }
      return text('cutoff', (v) => fringeStep(name, { cutoff: v }), { value: cutoff, num: true, placeholder: 'None', title: cutoff ? 'Pay per earner above this earns no fringe' : '' });
    }
  }
  if (list.id === 'groups') {
    if (col === 'description') return text('description', (v) => groupStep(name, { description: v }));
    if (col === 'counted') {
      const box = el('input', {
        class: 'setup-check',
        title: 'Whether its lines count in the budget total',
        attrs: { type: 'checkbox', 'aria-label': `${name} counts in the budget total` },
        data: { key: cellKey(list, name, 'counted') },
      });
      box.checked = def.in_budget_total !== false;
      box.addEventListener('change', () => {
        const on = box.checked;
        saveCell(() => groupStep(name, { counted: on }), () => {
          box.checked = !on;
        });
      });
      return box;
    }
    if (col === 'color') {
      const current = typeof def.color === 'string' ? def.color.toLowerCase() : '';
      const ours = paletteColor(current);
      const options = [['', 'None'], ...PALETTE.map((c) => [c.id, c.label])];
      if (current && !ours) options.push(['keep', `Other (${current})`]);
      const sel = cellSelect(cellKey(list, name, 'color'), ours ? ours.id : current ? 'keep' : '', options, label('Color'), (v, undo) => {
        const picked = PALETTE.find((c) => c.id === v);
        saveCell(() => groupStep(name, { color: v === 'keep' ? current : picked ? picked.light : null }), undo);
      });
      const chip = el('span', { class: `sw-chip ${ours ? `sw-${ours.id}` : current ? 'sw-keep' : 'sw-none'}`, attrs: { 'aria-hidden': 'true' } });
      return el('span', { class: 'sw-color' }, chip, sel);
    }
  }
  if (list.id === 'sets' && col === 'description') {
    return text('description', (v) => {
      if (!has(budget().sets, name)) throw `Set ${name} no longer exists.`;
      return ['set_set', { name, set: { description: v } }];
    });
  }
  if (list.id === 'locations') return text(col, (v) => locationStep(name, { [col]: v }));
  if (list.id === 'currencies') {
    if (col === 'rate') {
      return text('rate', (v) => currencyStep(name, { rate: v.trim() }), {
        value: str(def.rate) ? rateText(def.rate) : '',
        num: true,
        title: str(def.rate) ? `1 ${name} = ${rateText(def.rate)} ${str(budget().base_currency)}`.trim() : '',
      });
    }
    return text(col, (v) => currencyStep(name, { [col]: v }));
  }
  return el('span');
}

// ---------------------------------------------------------------------------
// Drawing a setup table

/** The selected row's name in `list`, when it still exists. */
function setupSelected(list) {
  const name = st.selected[list.id];
  return name !== undefined && has(list.defs(budget()), name) ? name : null;
}

/** Marks row `name` as selected without moving focus (a cell in it got focus). */
function markRow(list, name, li) {
  st.selected[list.id] = name;
  const ul = li.closest('.apply-items');
  if (ul) for (const x of ul.querySelectorAll('.apply-item.is-selected')) if (x !== li) x.classList.remove('is-selected');
  li.classList.add('is-selected');
  syncStrip(list);
}

/** The strip's Edit, Delete and Save to Library follow the selected row. */
function syncStrip(list) {
  const name = setupSelected(list);
  for (const h of mounted()) {
    for (const b of h.querySelectorAll('[data-needs-item]')) {
      if (name === null) b.setAttribute('aria-disabled', 'true');
      else b.removeAttribute('aria-disabled');
      b.title = name === null ? `Select a ${list.what} first` : `${b.dataset.does} ${name}`;
    }
  }
}

/** The action strip of `list`'s table. */
function setupStrip(list) {
  const name = setupSelected(list);
  const needs = (text, key, does, run) => {
    const b = stripButton(text, key, () => {
      const n = setupSelected(list);
      if (n !== null) run(n);
    }, { enabled: name !== null, title: name === null ? `Select a ${list.what} first` : `${does} ${name}` });
    b.dataset.needsItem = '1';
    b.dataset.does = does;
    return b;
  };
  return actionStrip(
    list.label,
    [
      stripButton(`New ${list.what}…`, addKey(list.id), () => addItem(list), { title: `Add a ${list.what} to this budget` }),
      needs('Edit…', `apply:edit:${list.id}`, `Edit ${list.what}`, (n) => editItem(list, n)),
      needs('Delete…', `apply:delete:${list.id}`, `Delete ${list.what}`, (n) => deleteItem(list, n)),
      needs('Save to Library…', `apply:save:${list.id}`, `Copy to your library, with what it needs: ${list.what}`, (n) => saveItemToLibrary(renameCtx(), list.what, n)),
    ],
    searchBox({
      key: `apply:filter:${list.id}`,
      value: str(st.filters[list.id]),
      label: `Search ${list.label.toLowerCase()}`,
      placeholder: `Search ${list.label.toLowerCase()}`,
      onChange: (v) => {
        st.filters[list.id] = v;
        draw();
      },
    }),
  );
}

/** The band of budget-wide settings over a table: the group rules, or the base and totals currency. */
function setupBand(list) {
  if (list.id === 'groups') {
    const now = groupRules();
    const fields = GROUP_RULES.map(([rule, label, , title]) => {
      const sel = el(
        'select',
        { class: 'setup-input setup-select', title, attrs: { disabled: st.busy }, data: { key: `apply:grouprule:${rule}` } },
        el('option', { attrs: { value: 'include' }, text: 'Include' }),
        el('option', { attrs: { value: 'exclude' }, text: 'Exclude' }),
      );
      sel.value = now[rule];
      sel.addEventListener('change', () => {
        if (st.busy) return;
        st.busy = true;
        st.error = '';
        call('set_group_rule', { rule, value: sel.value })
          .catch((e) => {
            st.error = errText(e);
          })
          .finally(() => {
            st.busy = false;
            draw();
          });
      });
      return bandField(label, sel, title);
    });
    const b = band('Group rules', el('span', { class: 'sw-band-title', text: 'Group rules' }), ...fields);
    b.title = 'They apply to the whole budget. A line left out still shows, but counts as 0.';
    return b;
  }
  if (list.id === 'currencies') {
    // The base and totals currency (setupbands.js: Budget Setup's own commands).
    return currencyBand(budget(), {
      call: (cmd, args) => call(cmd, args),
      onError: (message) => {
        st.error = message;
        draw();
      },
    });
  }
  return null;
}

function setupHead(list) {
  const cols = SETUP_COLS[list.id] || [];
  const head = (id, label, title, num) => {
    const b = sortButton(list.id, label, redrawLists, `apply:sort:${list.id}:${id}`, id);
    if (title) b.title = `${title}. ${b.title}`;
    return el('span', { class: `sw-head${num ? ' sw-num' : ''}` }, b);
  };
  return [
    el('span', { class: 'sw-head' }),
    head('name', list.id === 'currencies' ? 'Code' : 'Name', '', false),
    ...cols.map(([id, label, title, num]) => head(id, label, title, num)),
  ];
}

function drawSetupList(host, list) {
  const defs = obj(list.defs(budget()));
  const names = Object.keys(defs);
  host.setAttribute('aria-busy', st.busy ? 'true' : 'false');
  host.classList.add('apply-setup', `apply-setup-${list.id}`);
  wantUsage(st.view);
  const q = str(st.filters[list.id]).trim().toLowerCase();
  const { names: shown, filtered } = listNames(
    st.view,
    list.id,
    (n) => {
      if (!q) return true;
      const def = obj(defs[n]);
      return [n, def.description, def.short_code, def.city, def.region, def.country, def.symbol].some((v) => str(v).toLowerCase().includes(q));
    },
    { textFilter: Boolean(q), sortValue: (n, col) => setupSortValue(list, n, col) },
  );
  const movable = canReorder(list.id, filtered);
  const ul = el('ul', { class: 'apply-items', attrs: { 'aria-label': list.label }, data: { list: list.id } });
  for (const n of shown) ul.append(setupItem(list, n, obj(defs[n]), movable));
  const body = [];
  if (st.error) body.push(el('p', { class: 'setup-error apply-error', text: st.error, attrs: { role: 'alert' } }));
  if (!names.length) body.push(el('p', { class: 'setup-empty', text: list.none }));
  else if (!shown.length) body.push(el('p', { class: 'setup-empty', text: 'Nothing matches the search.' }));
  else body.push(gridTable(list.id, setupHead(list), ul));
  host.replaceChildren();
  mountFrame(host, { strip: setupStrip(list), band: setupBand(list), body });
  setupFooter(list, defs, shown.length, names.length);
}

/** One row of a setup table: the handle, the name (a button), then the cells. */
function setupItem(list, name, def, movable) {
  const key = itemKey(list.id, name);
  const infoId = nextId();
  const said = [str(def.description), list.id === 'fringes' ? fringeRate(def) : ''].filter(Boolean).join('; ');
  const tip = [str(def.description), sourceText(st.view, list.what, name), budgetWide(list, name, def), 'Double-click to edit'].filter(Boolean).join('\n');
  const b = el(
    'button',
    {
      class: 'apply-check apply-row sw-cell-name',
      title: tip,
      attrs: { type: 'button', 'aria-label': name, 'aria-describedby': infoId, 'aria-keyshortcuts': `Enter F2 Delete ${MOVE_KEYS}` },
      data: { key, nobusy: '1' },
    },
    el(
      'span',
      { class: 'apply-name' },
      name,
      str(def.short_code) ? el('span', { class: 'apply-code', text: str(def.short_code), title: 'Short code: the grid shows it for this item' }) : null,
    ),
    el('span', { class: 'setup-sr', text: said, attrs: { id: infoId } }),
  );
  b.addEventListener('click', () => {
    select(list.id, name, b);
    syncStrip(list);
  });
  b.addEventListener('dblclick', (e) => {
    e.preventDefault();
    editItem(list, name);
  });
  b.addEventListener('contextmenu', (e) => {
    e.preventDefault();
    select(list.id, name, b);
    syncStrip(list);
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
    if (document.activeElement !== b) syncStrip(list);
  });
  const cells = (SETUP_COLS[list.id] || []).map(([col, , , num]) => el('span', { class: `sw-cell${num ? ' sw-num' : ''}` }, setupCell(list, name, def, col)));
  const li = el(
    'li',
    { class: 'apply-item sw-grid-row', data: { name } },
    dragHandle(list.id, name, movable, (cmd, args) => call(cmd, args)),
    el('span', { class: 'sw-cell' }, b),
    ...cells,
  );
  li.addEventListener('focusin', () => {
    if (st.selected[list.id] !== name) markRow(list, name, li);
  });
  if (st.selected[list.id] === name) li.classList.add('is-selected');
  return li;
}

/** The footer: how many items, and for fringes the budget's fringe dollars from percent and flat fringes. */
function setupFooter(list, defs, shown, all) {
  const words = { fringes: ['fringe', 'fringes'], groups: ['group', 'groups'], sets: ['set', 'sets'], locations: ['location', 'locations'], currencies: ['currency', 'currencies'] }[list.id];
  const count = shown === all ? plural(all, words[0], words[1]) : `${shown} of ${plural(all, words[0], words[1])}`;
  const items = [{ label: '', value: count }];
  if (list.id === 'fringes' && all) {
    const r = obj(report().fringes);
    let percent = 0;
    let flat = 0;
    for (const [n, def] of Object.entries(defs)) {
      const t = Number(obj(r[n]).total) || 0;
      if (obj(def).kind === 'flat') flat += t;
      else percent += t;
    }
    items.push(
      { label: 'Fringe total', value: money(percent + flat), title: 'Every fringe dollar in the budget' },
      { label: 'Percent', value: money(percent), title: 'Fringe dollars from percent fringes in the whole budget' },
      { label: 'Flat', value: money(flat), title: 'Fringe dollars from flat fringes in the whole budget' },
    );
  }
  setFooterTotals(items);
}

/**
 * Movie Magic's budget-wide group rules (decisions.md, entry 57), each
 * Include or Exclude: [rule, label, default, title]. The file leaves out a
 * rule at its default.
 */
const GROUP_RULES = [
  ['ungrouped_lines', 'Lines with no group', 'include', 'Exclude leaves out every line and adjustment row that has no group'],
  ['conflict', 'Lines in a counted and an uncounted group', 'exclude', 'Include counts a line that is in a group in the total and a group that is not'],
  ['fringes_in_totals', 'Fringes in group totals', 'include', 'Whether group totals show their lines’ fringes. Display only: budget totals never change'],
];

function groupRules() {
  const r = obj(budget().group_rules);
  const out = {};
  for (const [rule, , def] of GROUP_RULES) out[rule] = r[rule] === 'include' || r[rule] === 'exclude' ? r[rule] : def;
  return out;
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
  const units = budget().units;
  const basis = FLAT_BASES.some(([b]) => b === def.basis) ? def.basis : 'amount';
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
      // A flat fringe keeps a cutoff it had (from a percent fringe, a file
      // or an import) but never uses it.
      key: 'cutoff_note',
      type: 'note',
      text: (v) => `Its cutoff of ${str(v.cutoff).trim()} is kept but not used: cutoffs apply to percent fringes only.`,
      showIf: (v) => v.kind === 'flat' && str(v.cutoff).trim() !== '',
    },
    {
      key: 'unit',
      label: 'Charged per',
      type: 'select',
      options: [['', 'Line'], ...timeUnits(unit).map((u) => [u, u])],
      value: unit,
      showIf: (v) => v.kind === 'flat',
    },
    { key: 'basis', label: 'Basis', type: 'select', options: FLAT_BASES, value: basis, showIf: (v) => v.kind === 'flat' },
    {
      key: 'max_hours_per_day',
      label: 'Max hours per day',
      value: str(def.max_hours_per_day),
      placeholder: 'None',
      showIf: (v) => v.kind === 'flat' && isHourUnit(units, v.unit),
    },
    { key: 'description', label: 'Description', value: str(def.description) },
    { key: 'short_code', label: 'Short code (shown in the grid)', value: str(def.short_code), placeholder: 'None' },
  ];
}

function fringeInput(v) {
  const flat = v.kind === 'flat';
  const hours = str(v.max_hours_per_day).trim();
  return {
    kind: flat ? 'flat' : 'percent',
    description: v.description,
    rate: v.rate.trim(),
    // Kept on a flat fringe too (never used there), so it comes back if the
    // fringe turns percent again.
    cutoff: moneyText(v.cutoff),
    unit: flat && v.unit ? v.unit : null,
    basis: flat ? v.basis || 'amount' : null,
    // Only a fringe charged per hour has a cap; it goes with the field.
    max_hours_per_day: flat && hours && isHourUnit(budget().units, v.unit) ? hours : null,
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
        // How amounts in it are written where totals show in it (decisions.md, entry 61).
        { key: 'separators', label: 'Separators', type: 'select', options: CURRENCY_SEPARATORS, value: def.separators === 'point_comma' ? 'point_comma' : 'comma_point' },
        { key: 'symbol_after', label: 'Symbol after the amount', type: 'check', value: def.symbol_after === true },
      ],
      build: (v) => [
        'set_currency',
        {
          code: name,
          currency: { description: v.description, symbol: v.symbol, rate: v.rate.trim(), separators: v.separators, symbol_after: Boolean(v.symbol_after) },
        },
      ],
    };
  },
};

/** A currency's separators, as its editor offers them (decisions.md, entry 61). */
const CURRENCY_SEPARATORS = [
  ['comma_point', '1,234.56'],
  ['point_comma', '1.234,56'],
];

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
    build: (code, v) => [
      'set_currency',
      { code, currency: { description: v.description, symbol: v.symbol, rate: v.rate.trim(), separators: v.separators, symbol_after: Boolean(v.symbol_after) } },
    ],
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
