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
// - Order: the budget's own order (order.js), or a column head's sort,
//   which a click cycles A to Z, Z to A, then back to the budget's order (the
//   name's in the tools panel; Budget Setup's tables sort on any column,
//   giving listNames() each item's value, and its rail picks the order too:
//   sortSelect). The sort is a view: it never changes the budget's order.
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
import { obj } from './util.js';
import { t } from './i18n.js';

const USAGE_DELAY_MS = 120;
const DRAG_START_PX = 4;

const st = {
  usage: null, // { table: { name: { lines, other, total } } }
  usageKey: '', // `${doc}:${rev}` the counts are for
  wantKey: '',
  timer: null,
  seq: 0,
  sort: {}, // table -> 'custom' | 'asc' | 'desc'
  sortCol: {}, // table -> the column sorted on ('name' unless a head picked another)
  show: {}, // table -> 'all' | 'used' | 'unused'
  listeners: new Set(),
  findHook: null,
};

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

/** The column `table` is sorted on: 'name' unless a column head picked another. */
export function sortColOf(table) {
  return sortOf(table) === 'custom' ? 'name' : st.sortCol[table] || 'name';
}

function caseless(a, b) {
  const x = a.toLowerCase();
  const y = b.toLowerCase();
  if (x !== y) return x < y ? -1 : 1;
  return a < b ? -1 : a > b ? 1 : 0;
}

/** Two column values: numbers by size, anything else as caseless text; blanks last. */
function compareValues(a, b) {
  const blank = (v) => v === null || v === undefined || v === '';
  if (blank(a) || blank(b)) return blank(a) === blank(b) ? 0 : blank(a) ? 1 : -1;
  if (typeof a === 'number' && typeof b === 'number') return a - b;
  return caseless(String(a), String(b));
}

/**
 * The names `table` shows: in the budget's order or the header's sort, less
 * those the Show choice and `keep(name)` (a text filter) leave out.
 * `filtered` is true when anything could be hidden (reordering is off then).
 * `sortValue(name, col)` gives an item's value in a column other than the
 * name (Budget Setup's tables); without it the name sort is used.
 */
export function listNames(view, table, keep = null, { textFilter = false, sortValue = null } = {}) {
  const budget = obj(obj(view).budget);
  let names = orderedNames(budget, table);
  const sort = sortOf(table);
  if (sort !== 'custom') {
    const col = sortColOf(table);
    if (col !== 'name' && typeof sortValue === 'function') {
      const value = new Map(names.map((n) => [n, sortValue(n, col)]));
      names = [...names].sort((a, b) => compareValues(value.get(a), value.get(b)) || caseless(a, b));
    } else {
      names = [...names].sort(caseless);
    }
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

/**
 * A column head: a button cycling the sort on column `col` (the name by
 * default): A to Z, Z to A, then the budget's order. A click on another
 * column's head starts it at A to Z.
 */
export function sortButton(table, label, onChange, key, col = 'name') {
  const on = sortOf(table) !== 'custom' && sortColOf(table) === col;
  const sort = on ? sortOf(table) : 'custom';
  const next = { custom: 'asc', asc: 'desc', desc: 'custom' }[sort];
  const says = {
    custom: col === 'name' ? t('setup.sort.in_budget_order') : t('setup.sort.not_sorted'),
    asc: col === 'name' ? t('setup.sort.a_to_z') : t('setup.sort.ascending'),
    desc: col === 'name' ? t('setup.sort.z_to_a') : t('setup.sort.descending'),
  };
  const then = next === 'custom' ? t('setup.sort.budget_order_then') : says[next];
  const b = el(
    'button',
    {
      class: `setup-sort${on ? ' is-sorted' : ''}`,
      title: t('setup.sort.tip', { label, now: says[sort], then }),
      attrs: { type: 'button', 'aria-label': t('setup.sort.aria', { label, now: says[sort], then }) },
      data: { key, nobusy: '1' },
    },
    el('span', { text: label }),
    el('span', { class: 'setup-sort-mark', text: sort === 'asc' ? '↑' : sort === 'desc' ? '↓' : '', attrs: { 'aria-hidden': 'true' } }),
  );
  b.addEventListener('click', () => {
    st.sort[table] = next;
    st.sortCol[table] = col;
    onChange();
  });
  return b;
}

/**
 * The order as a choice (Budget Setup's rail): the budget's order, or by
 * name A to Z or Z to A; while a column head sorts on another column, that
 * sort is listed too. `cols` gives the columns' labels ({ id: label }).
 */
export function sortSelect(table, onChange, key, cols = {}) {
  const sel = el('select', {
    class: 'setup-input setup-select setup-sortpick',
    title: t('setup.the_order_the_table_lists'),
    attrs: { 'aria-label': t('setup.sort') },
    data: { key, nobusy: '1' },
  });
  const options = [
    ['custom', t('setup.sort.budget_order')],
    ['name:asc', t('setup.sort.name_a_to_z')],
    ['name:desc', t('setup.sort.name_z_to_a')],
  ];
  const col = sortColOf(table);
  const sort = sortOf(table);
  if (sort !== 'custom' && col !== 'name') options.push([`${col}:${sort}`, t('setup.sort.column', { column: cols[col] || col, dir: sort === 'asc' ? t('setup.sort.ascending') : t('setup.sort.descending') })]);
  for (const [v, text] of options) {
    const o = el('option', { text });
    o.value = v;
    sel.append(o);
  }
  sel.value = sort === 'custom' ? 'custom' : `${col}:${sort}`;
  sel.addEventListener('change', () => {
    if (sel.value === 'custom') st.sort[table] = 'custom';
    else {
      const [c, dir] = sel.value.split(':');
      st.sortCol[table] = c;
      st.sort[table] = dir;
    }
    onChange();
  });
  return sel;
}

/** The Show choice: All, Used or Unused items. */
export function showSelect(table, onChange, key) {
  const sel = el('select', {
    class: 'setup-input setup-select setup-show',
    title: t('setup.show_every_item_or_only'),
    attrs: { 'aria-label': t('setup.show') },
    data: { key, nobusy: '1' },
  });
  for (const [v, text] of [
    ['all', t('setup.show.all')],
    ['used', t('setup.show.used')],
    ['unused', t('setup.show.unused')],
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
  if (Number(u.lines)) parts.push(t('setup.uses.lines', { count: Number(u.lines) }));
  if (Number(u.other)) parts.push(Number(u.lines) ? t('setup.uses.other_too', { count: Number(u.other) }) : t('setup.uses.other', { count: Number(u.other) }));
  return parts.length ? parts.join('; ') : t('setup.uses.nothing');
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
    title: u ? (lines ? t('setup.uses.tip_find', { uses: useWords(u) }) : t('setup.uses.tip', { uses: useWords(u) })) : t('setup.uses.counting'),
    attrs: { type: 'button', 'aria-label': u ? (lines ? t('setup.uses.aria_find', { uses: useWords(u) }) : useWords(u)) : t('setup.uses.aria_counting', { name }) },
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
      ? t('setup.drag_to_reorder', { alt: isMac ? t('setup.key.option') : t('setup.key.alt') })
      : t('setup.reorder_off'),
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
