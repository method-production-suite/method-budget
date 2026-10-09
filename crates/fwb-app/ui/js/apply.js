// Apply panel: what the selected lines carry, and a click to change it.
//
// The first section of the side panel. For fringes, groups, sets, locations
// and currencies it lists every defined item with a three-state checkbox that
// reflects the grid's selection (every selected line has it, some do, none
// do) and the item's dollars within the selection. Clicking adds the item to
// every selected line, or takes it off when they all have it. Sets,
// locations and currencies are one per line: picking one replaces the
// line's old value. Fringe ranges are made and removed here too.
//
// On the topsheet and the account list, the selection is every line under
// the selected categories or accounts. Rust resolves that (apply_summary) and
// returns the row ids, which the edit commands then take, so a click acts on
// exactly the rows the panel shows.
//
// setup.js mounts the panel with renderApply(host, view, ctx); main.js fires
// a "fwb:selection" event on the document when the grid selection changes.
// Budget text only reaches the page through textContent (dom.js el()), and
// styling is classes from apply.css: no inline styles, no innerHTML.

import { invoke } from './api.js';
import { el } from './dom.js';
import { money } from './format.js';

const LISTS = [
  {
    id: 'fringes',
    label: 'Fringes',
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
    field: 'groups',
    multi: true,
    defs: (b) => b.groups,
    eligible: (s) => s.costed,
    noTakers: 'Subtotal rows are never in a group.',
    none: 'No groups yet.',
  },
  {
    id: 'sets',
    label: 'Set',
    field: 'set',
    multi: false,
    defs: (b) => b.sets,
    eligible: (s) => s.costed,
    noTakers: 'Subtotal rows have no set.',
    none: 'No sets yet.',
  },
  {
    id: 'locations',
    label: 'Location',
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
    field: 'currency',
    multi: false,
    defs: (b) => b.currencies,
    eligible: (s) => s.lines,
    noTakers: 'Currencies go on lines, and the selection has none.',
    none: 'No other currencies yet.',
  },
];

const ARIA = { all: 'true', some: 'mixed', none: 'false' };
const DEBOUNCE_MS = 60;

const st = {
  host: null,
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
  closed: new Set(),
};

/** Draws the panel into `host` and asks Rust for fresh numbers. */
export function renderApply(host, view, ctx) {
  if (view !== st.view) st.gen += 1;
  st.host = host;
  st.view = view && typeof view === 'object' ? view : {};
  st.ctx = ctx || {};
  draw();
  schedule();
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

function byteOrder(a, b) {
  return a < b ? -1 : a > b ? 1 : 0;
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

function visible() {
  return Boolean(st.host && st.host.isConnected && !st.host.closest('[hidden]'));
}

function stateOf(count, eligible) {
  if (!eligible || !count) return 'none';
  return count >= eligible ? 'all' : 'some';
}

function tally(s, list, name) {
  const t = s && obj(s[list])[name];
  return t ? { count: Number(t.count) || 0, total: t.total } : { count: 0, total: 0 };
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
  const sel = typeof st.ctx.selection === 'function' ? st.ctx.selection() : null;
  if (!sel || !Array.isArray(sel.keys)) return out;
  const top = Array.isArray(budget().topsheet) ? budget().topsheet : [];
  const index = (k, prefix) => (k.startsWith(prefix) ? Number(k.slice(prefix.length)) : NaN);
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

/** "All lines in 13 Cast", "All lines in 3 accounts", "4 rows". */
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
  if (!st.host) return;
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

// ---------------------------------------------------------------------------
// Edits

/** Runs one edit: `plan(summary)` returns [command, args, patched summary] or null. */
async function act(plan) {
  if (st.busy) return;
  st.busy = true;
  st.error = '';
  draw();
  try {
    const s = await freshSummary();
    const step = s && s.rows.length ? plan(s) : null;
    if (!step) return;
    const [cmd, args, patched] = step;
    if (patched) {
      st.summary = patched;
      draw();
    }
    if (typeof st.ctx.call !== 'function') throw new Error('Not connected');
    await st.ctx.call(cmd, args);
  } catch (e) {
    st.error = errText(e);
    st.summaryGen = -1; // the patch may be wrong now; fetch the truth
    schedule(0);
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
  act((s) => {
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
  if (!a || !st.host || !st.host.contains(a) || !a.dataset || !a.dataset.key) return null;
  const info = { key: a.dataset.key };
  if (typeof a.selectionStart === 'number') {
    info.start = a.selectionStart;
    info.end = a.selectionEnd;
  }
  return info;
}

function restoreFocus(info) {
  if (!info) return;
  const target = [...st.host.querySelectorAll('[data-key]')].find((e) => e.dataset.key === info.key);
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

function draw() {
  const host = st.host;
  if (!host) return;
  uid = 0;
  const focus = captureFocus();
  const s = st.summary;
  // Label and numbers change together: both describe the summary's selection
  // (the current one only until the first summary arrives).
  const sc = s && st.summaryScope ? st.summaryScope : scope();
  const root = el('div', { class: 'apply', attrs: { 'aria-busy': st.busy ? 'true' : 'false' } });
  root.append(head(sc, s));
  if (st.error) root.append(el('p', { class: 'setup-error apply-error', text: st.error, attrs: { role: 'alert' } }));
  if (s && s.rows.length) {
    for (const list of LISTS) root.append(section(list, s));
  }
  host.replaceChildren(root);
  restoreFocus(focus);
}

function head(sc, s) {
  const box = el('div', { class: 'apply-head' });
  const nothing = !sc.rowIds.length && !sc.accounts.length && !sc.categories.length;
  if (nothing) {
    box.append(
      el('p', { class: 'apply-scope', text: 'Nothing selected' }),
      el('p', {
        class: 'apply-hint',
        text: 'Select lines in the grid, or a category or account, to see and change their fringes, groups, set, location and currency.',
      }),
    );
    return box;
  }
  if (s && !s.rows.length) {
    box.append(
      el('p', { class: 'apply-scope', text: scopeLabel(sc, s) }),
      el('p', { class: 'apply-hint', text: 'There are no lines here yet.' }),
    );
    return box;
  }
  box.append(el('p', { class: 'apply-scope', text: scopeLabel(sc, s) }));
  if (s) {
    const t = obj(obj(s.totals).lines);
    const stats = el('p', { class: 'apply-stats' });
    stats.append(
      stat('Lines', money(t.details_total)),
      stat('Fringes', money(t.fringe_total)),
      stat('Effective', s.fringe_percent === null ? 'n/a' : `${twoPlaces(s.fringe_percent)}%`, 'Fringe dollars as a percent of the line totals'),
    );
    box.append(stats);
  }
  return box;
}

function stat(label, value, title = '') {
  return el(
    'span',
    { class: 'apply-stat', title: title || undefined },
    el('span', { class: 'apply-stat-label', text: label }),
    el('span', { class: 'apply-num', text: value }),
  );
}

function section(list, s) {
  const defs = obj(list.defs(budget()));
  const names = Object.keys(defs).sort(byteOrder);
  const eligible = Number(list.eligible(s)) || 0;
  const open = !st.closed.has(list.id);
  const bodyId = nextId();
  const applied = names.filter((n) => tally(s, list.id, n).count > 0).length;

  const sec = el('section', { class: 'apply-list', attrs: { 'aria-label': list.label } });
  const toggleBtn = el(
    'button',
    {
      class: 'apply-fold',
      attrs: { type: 'button', 'aria-expanded': String(open), 'aria-controls': bodyId },
      data: { key: `apply:fold:${list.id}`, nobusy: '1' },
      on: {
        click: () => {
          if (st.closed.has(list.id)) st.closed.delete(list.id);
          else st.closed.add(list.id);
          draw();
        },
      },
    },
    el('span', { class: 'apply-fold-mark', attrs: { 'aria-hidden': 'true' } }),
    el('span', { class: 'apply-title', text: list.label }),
    applied ? el('span', { class: 'apply-badge', text: String(applied), title: `${plural(applied, 'item', 'items')} on the selection` }) : null,
  );
  const edit = button('Edit', 'apply-link', `apply:edit:${list.id}`, () => st.ctx.setTab && st.ctx.setTab(list.id), {
    label: `Edit the ${list.label.toLowerCase()} list`,
    title: `Open the ${list.label.toLowerCase()} setup tab`,
  });
  edit.removeAttribute('aria-disabled'); // switching tabs is never busy
  sec.append(el('div', { class: 'apply-list-head' }, toggleBtn, edit));

  const body = el('div', { class: 'apply-body', attrs: { id: bodyId } });
  body.hidden = !open;
  sec.append(body);
  if (!open) return sec;

  if (!names.length) {
    body.append(el('p', { class: 'setup-empty', text: `${list.none} Add them under Edit.` }));
    return sec;
  }
  body.append(tools(list, s, eligible));
  if (!eligible) body.append(el('p', { class: 'apply-hint', text: list.noTakers }));

  const q = str(st.filters[list.id]).trim().toLowerCase();
  const only = Boolean(st.onlyApplied[list.id]);
  const shown = names.filter((n) => {
    const def = obj(defs[n]);
    if (only && tally(s, list.id, n).count === 0) return false;
    return !q || n.toLowerCase().includes(q) || str(def.description).toLowerCase().includes(q);
  });
  const ul = el('ul', { class: 'apply-items', attrs: { 'aria-label': list.label } });
  for (const n of shown) ul.append(item(list, s, n, obj(defs[n]), eligible));
  body.append(ul);
  if (!shown.length) {
    body.append(el('p', { class: 'setup-empty', text: only ? 'None of these are on the selection.' : 'Nothing matches the filter.' }));
  }
  if (list.id === 'fringes') body.append(fringeFoot(s));
  return sec;
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
  row.append(input, onlyBtn);

  if (!list.multi) {
    const base = str(budget().base_currency);
    const hasValue =
      list.id === 'currencies'
        ? tally(s, 'currencies', base).count < eligible
        : Object.keys(obj(s[list.id])).some((k) => tally(s, list.id, k).count > 0);
    const what = list.label.toLowerCase();
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

function item(list, s, name, def, eligible) {
  const t = tally(s, list.id, name);
  const state = stateOf(t.count, eligible);
  const infoId = nextId();
  const off = list.id === 'groups' && def.in_budget_total === false;
  const amount = off ? 'Not in total' : t.count ? money(t.total) : '';
  const info = [];
  if (state === 'some') info.push(`on ${t.count} of ${plural(eligible, 'line', 'lines')}`);
  if (str(def.description)) info.push(str(def.description));

  const b = el(
    'button',
    {
      class: `apply-check${list.multi ? '' : ' apply-radio'}`,
      title: str(def.description) || undefined,
      attrs: {
        type: 'button',
        role: 'checkbox',
        'aria-checked': ARIA[state],
        'aria-label': name,
        'aria-describedby': infoId,
      },
      data: { key: `apply:item:${list.id}:${name}`, nobusy: '1' },
    },
    el('span', { class: 'apply-box', attrs: { 'aria-hidden': 'true' } }),
    el('span', { class: 'apply-name', text: name }),
    el('span', { class: 'apply-desc', text: str(def.description) }),
    state === 'some' ? el('span', { class: 'apply-count', text: `${t.count}/${eligible}`, attrs: { 'aria-hidden': 'true' } }) : null,
    el('span', { class: `apply-num apply-amt${off ? ' apply-off' : ''}`, text: amount }),
    el('span', { class: 'setup-sr', text: [info.join('; '), amount].filter(Boolean).join('. '), attrs: { id: infoId } }),
  );
  if (!eligible || st.busy) b.setAttribute('aria-disabled', 'true');
  b.addEventListener('click', () => {
    if (b.getAttribute('aria-disabled') === 'true') return;
    toggle(list, name);
  });
  b.addEventListener('keydown', (e) => {
    const step = e.key === 'ArrowDown' ? 1 : e.key === 'ArrowUp' ? -1 : 0;
    if (!step) return;
    const items = [...b.closest('.apply-items').querySelectorAll('.apply-check')];
    const next = items[items.indexOf(b) + step];
    if (next) {
      e.preventDefault();
      next.focus();
    }
  });
  return el('li', { class: `apply-item apply-${state}` }, b);
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
