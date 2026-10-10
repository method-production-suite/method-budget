// What every setup list in the tools panel shares (roadmap 2.9, 2.12): the
// fringes, groups, sets, locations and currency lists (apply.js) and the
// globals and units lists (setup.js).
//
// - In-use counts and totals: setup_usage (src/setup_lists.rs) gives every
//   item's line count, other uses and dollars. Fetched once per budget
//   change, a moment after it, and only while a setup list is drawn; the
//   lists draw without them until they arrive (onUsage tells tools.js to
//   redraw). A click on an item's count runs Find for its uses (setFindHook).
// - Show: All, Used or Unused items (used: lines or other setup name it).
// - Order: the budget's own order (order.js), or the name header's sort,
//   which a click cycles A to Z, Z to A, then back to the budget's order. The
//   sort is a view: it never changes the budget's order.
// - Reordering: drag an item by its handle, or Option+Shift+Up and Down on
//   it (Alt+Shift on Windows and Linux). Only in the budget's own order with
//   no filter (text, Show or Applied only), so items that are not shown never
//   move. One command each (set_setup_order, move_setup_item), one undo step
//   (quick key repeats join one).
//
// Dragging uses pointer events, not HTML drag and drop (as the panel's tabs
// do), and feedback is classes: no inline styles, no innerHTML.

import { invoke } from './api.js';
import { el, isMac } from './dom.js';
import { money } from './format.js';
import { orderedNames, TABLE_KIND } from './order.js';

const USAGE_DELAY_MS = 120;
const DRAG_START_PX = 4;

const st = {
  usage: null, // { table: { name: { lines, other, total } } }
  usageKey: '', // `${doc}:${rev}` the counts are for
  wantKey: '',
  timer: null,
  seq: 0,
  sort: {}, // table -> 'custom' | 'asc' | 'desc'
  show: {}, // table -> 'all' | 'used' | 'unused'
  listeners: new Set(),
  findHook: null,
};

function obj(v) {
  return v && typeof v === 'object' && !Array.isArray(v) ? v : {};
}

function plural(n, one, many) {
  return `${n} ${n === 1 ? one : many}`;
}

/** Calls `fn()` whenever new counts arrive. */
export function onUsage(fn) {
  st.listeners.add(fn);
}

/** Redraws every setup list (a sort or Show choice changed). */
export function redrawLists() {
  for (const fn of st.listeners) fn();
}

/** `fn(table, name)` shows Find with the item's uses. */
export function setFindHook(fn) {
  st.findHook = fn;
}

/**
 * The counts for `view` are wanted (a list is being drawn): fetches them a
 * moment after the budget changed, unless they are already for this view.
 */
export function wantUsage(view) {
  const v = obj(view);
  const key = `${v.doc}:${v.rev}`;
  if (key === st.usageKey || key === st.wantKey) return;
  st.wantKey = key;
  clearTimeout(st.timer);
  st.timer = setTimeout(async () => {
    const seq = ++st.seq;
    try {
      const u = await invoke('setup_usage');
      if (seq !== st.seq) return;
      st.usage = u && typeof u === 'object' ? u : null;
      st.usageKey = key;
    } catch {
      if (seq !== st.seq) return;
      // No counts for this view; asked again only when the budget changes.
      st.usage = null;
      st.usageKey = key;
    }
    st.wantKey = '';
    for (const fn of st.listeners) fn();
  }, USAGE_DELAY_MS);
}

/** True when the counts shown are for this view. */
function current(view) {
  const v = obj(view);
  return st.usage !== null && st.usageKey === `${v.doc}:${v.rev}`;
}

/** Item `name`'s use ({ lines, other, total }), or null when not known yet. */
export function usageOf(view, table, name) {
  if (!current(view)) return null;
  const u = obj(obj(st.usage)[table])[name];
  return u && typeof u === 'object' ? u : { lines: 0, other: 0, total: null };
}

function used(u) {
  return Boolean(u) && (Number(u.lines) > 0 || Number(u.other) > 0);
}

/** The sort the name header shows for `table`. */
export function sortOf(table) {
  return st.sort[table] || 'custom';
}

function caseless(a, b) {
  const x = a.toLowerCase();
  const y = b.toLowerCase();
  if (x !== y) return x < y ? -1 : 1;
  return a < b ? -1 : a > b ? 1 : 0;
}

/**
 * The names `table` shows: in the budget's order or the header's sort, less
 * those the Show choice and `keep(name)` (a text filter) leave out.
 * `filtered` is true when anything could be hidden (reordering is off then).
 */
export function listNames(view, table, keep = null, { textFilter = false } = {}) {
  const budget = obj(obj(view).budget);
  let names = orderedNames(budget, table);
  const sort = sortOf(table);
  if (sort !== 'custom') {
    names = [...names].sort(caseless);
    if (sort === 'desc') names.reverse();
  }
  const show = st.show[table] || 'all';
  if (show !== 'all' && current(view)) {
    names = names.filter((n) => used(usageOf(view, table, n)) === (show === 'used'));
  }
  if (keep) names = names.filter(keep);
  return { names, filtered: show !== 'all' || textFilter };
}

/** Whether `table`'s items can be reordered now. */
export function canReorder(table, filtered) {
  return sortOf(table) === 'custom' && !filtered;
}

/** The name header: a button cycling the sort (A to Z, Z to A, the budget's order). */
export function sortButton(table, label, onChange, key) {
  const sort = sortOf(table);
  const next = { custom: 'asc', asc: 'desc', desc: 'custom' }[sort];
  const says = { custom: 'in the budget’s order', asc: 'A to Z', desc: 'Z to A' };
  const b = el(
    'button',
    {
      class: `setup-sort${sort === 'custom' ? '' : ' is-sorted'}`,
      title: `${label} ${says[sort]}. Click for ${says[next]}.`,
      attrs: { type: 'button', 'aria-label': `${label}, ${says[sort]}; click for ${says[next]}` },
      data: { key, nobusy: '1' },
    },
    el('span', { text: label }),
    el('span', { class: 'setup-sort-mark', text: sort === 'asc' ? '↑' : sort === 'desc' ? '↓' : '', attrs: { 'aria-hidden': 'true' } }),
  );
  b.addEventListener('click', () => {
    st.sort[table] = next;
    onChange();
  });
  return b;
}

/** The Show choice: All, Used or Unused items. */
export function showSelect(table, onChange, key) {
  const sel = el('select', {
    class: 'setup-input setup-select setup-show',
    title: 'Show every item, or only the ones lines and setup use, or only the unused ones',
    attrs: { 'aria-label': 'Show' },
    data: { key, nobusy: '1' },
  });
  for (const [v, text] of [
    ['all', 'All'],
    ['used', 'Used'],
    ['unused', 'Unused'],
  ]) {
    const o = el('option', { text });
    o.value = v;
    sel.append(o);
  }
  sel.value = st.show[table] || 'all';
  sel.addEventListener('change', () => {
    st.show[table] = sel.value;
    onChange();
  });
  return sel;
}

function useWords(u) {
  const parts = [];
  if (Number(u.lines)) parts.push(`${plural(Number(u.lines), 'line uses', 'lines use')} it`);
  if (Number(u.other)) parts.push(`${plural(Number(u.other), 'other setting', 'other settings')} ${Number(u.lines) ? 'too' : 'use it'}`);
  return parts.length ? parts.join('; ') : 'Nothing uses it';
}

/**
 * Item `name`'s line count: a button that runs Find for its uses (blank
 * until the counts arrive).
 */
export function usesButton(view, table, name, key) {
  const u = usageOf(view, table, name);
  const lines = u ? Number(u.lines) || 0 : null;
  const b = el('button', {
    class: `setup-uses${lines ? '' : ' is-zero'}`,
    text: lines === null ? '' : String(lines),
    title: u ? `${useWords(u)}${lines ? '. Click to find them.' : '.'}` : 'Counting…',
    attrs: { type: 'button', 'aria-label': u ? `${useWords(u)}${lines ? ', find them' : ''}` : `Lines using ${name}: counting` },
    data: { key, nobusy: '1' },
  });
  if (!lines) b.setAttribute('aria-disabled', 'true');
  b.addEventListener('click', (e) => {
    e.stopPropagation();
    if (lines && st.findHook) st.findHook(table, name);
  });
  b.addEventListener('dblclick', (e) => e.stopPropagation());
  return b;
}

/** Item `name`'s dollars across the budget, as text ('' where none apply). */
export function totalText(view, table, name) {
  const u = usageOf(view, table, name);
  if (!u || u.total === null || u.total === undefined) return '';
  return money(Number(u.total));
}

/** How many lines use item `name` (0 until the counts arrive). */
export function linesUsing(view, table, name) {
  const u = usageOf(view, table, name);
  return u ? Number(u.lines) || 0 : 0;
}

// ---------------------------------------------------------------------------
// Reordering

function kind(table) {
  return TABLE_KIND[table];
}

/**
 * Option+Shift+Up or Down on item `name`: moves it one place. True when the
 * key was ours (even if moving isn't possible now).
 */
export function moveKey(e, table, name, allowed, call) {
  const ours = e.altKey && e.shiftKey && !e.metaKey && !e.ctrlKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown');
  if (!ours) return false;
  e.preventDefault();
  e.stopPropagation();
  if (allowed) call('move_setup_item', { kind: kind(table), name, by: e.key === 'ArrowUp' ? -1 : 1 }).catch(() => {});
  return true;
}

/** The keys that move an item, for aria-keyshortcuts. */
export const MOVE_KEYS = 'Alt+Shift+ArrowUp Alt+Shift+ArrowDown';

/**
 * A drag handle for item `name` in list `ul` (each item an li with
 * data-name). Dropping it before or after another item saves the new order.
 */
export function dragHandle(table, name, allowed, call) {
  const h = el('span', {
    class: `setup-handle${allowed ? '' : ' is-off'}`,
    title: allowed
      ? `Drag to reorder, or ${isMac ? 'Option' : 'Alt'}+Shift+Up and Down`
      : 'Reordering is off while the list is sorted or filtered',
    attrs: { 'aria-hidden': 'true' },
  });
  if (!allowed) return h;
  h.addEventListener('pointerdown', (e) => startDrag(e, h, table, name, call));
  return h;
}

function startDrag(e, h, table, name, call) {
  if (e.button !== 0) return;
  const li = h.closest('li[data-name]');
  const ul = li && li.parentElement;
  if (!ul) return;
  e.preventDefault();
  const y0 = e.clientY;
  const x0 = e.clientX;
  let dragging = false;
  let target = null;
  try {
    h.setPointerCapture(e.pointerId);
  } catch {
    // Not a real pointer (a test); moves still arrive.
  }
  const paint = () => {
    for (const n of ul.querySelectorAll('.drop-before, .drop-after')) n.classList.remove('drop-before', 'drop-after');
    if (target) target.li.classList.add(target.after ? 'drop-after' : 'drop-before');
  };
  const move = (ev) => {
    if (!dragging) {
      if (Math.hypot(ev.clientX - x0, ev.clientY - y0) < DRAG_START_PX) return;
      dragging = true;
      li.classList.add('is-dragging');
      document.body.classList.add('setup-dragging');
    }
    const hit = typeof document.elementFromPoint === 'function' ? document.elementFromPoint(ev.clientX, ev.clientY) : null;
    const over = hit && hit.closest ? hit.closest('li[data-name]') : null;
    if (over && over.parentElement === ul && over !== li) {
      const r = over.getBoundingClientRect();
      target = { li: over, after: ev.clientY > r.top + r.height / 2 };
    } else if (!over || over.parentElement !== ul) {
      target = null;
    }
    paint();
  };
  const end = () => {
    h.removeEventListener('pointermove', move);
    h.removeEventListener('pointerup', end);
    h.removeEventListener('pointercancel', cancel);
    li.classList.remove('is-dragging');
    document.body.classList.remove('setup-dragging');
    const t = target;
    target = null;
    paint();
    if (!dragging || !t) return;
    const names = [...ul.querySelectorAll(':scope > li[data-name]')].map((x) => x.dataset.name).filter((n) => n !== name);
    let at = names.indexOf(t.li.dataset.name);
    if (at < 0) return;
    if (t.after) at += 1;
    names.splice(at, 0, name);
    call('set_setup_order', { kind: kind(table), names }).catch(() => {});
  };
  const cancel = () => {
    dragging = false;
    end();
  };
  h.addEventListener('pointermove', move);
  h.addEventListener('pointerup', end);
  h.addEventListener('pointercancel', cancel);
}

// ---------------------------------------------------------------------------
// Deleting something in use

/**
 * What deleting item `name` does, given how much it is used: the question,
 * the button's words and whether lines must be cleared. `count` is its
 * lines (0 until the counts arrive).
 */
export function inUse(view, table, name) {
  const u = usageOf(view, table, name);
  return { lines: u ? Number(u.lines) || 0 : 0, other: u ? Number(u.other) || 0 : 0, known: Boolean(u) };
}
