// Setup tables as the budget shows them (roadmap 2.11, 2.12): their order,
// and fringes' and groups' short codes. Pure functions over the view's
// budget object (docs/FORMAT.md): no DOM, no commands.
//
// A table's order is the budget's hand-made order when it has one
// (`setup_order`, names first to last; names no longer in the table are
// skipped, names not listed follow in byte order), else byte order, which is
// how Rust and the file sort names. The setup lists, the Apply lists, the
// cell type-ahead and the printed setup tables all use orderedNames().
//
// Also: a short summary of an item's notes for a tooltip.

import { obj } from './util.js';
import { t } from './i18n.js';

/** The kind name commands take for each table (`rename_setup_item`, `set_setup_order`...). */
export const TABLE_KIND = {
  units: 'unit',
  globals: 'global',
  fringes: 'fringe',
  groups: 'group',
  sets: 'set',
  locations: 'location',
  currencies: 'currency',
};

export function byteOrder(a, b) {
  return a < b ? -1 : a > b ? 1 : 0;
}

/** The budget's hand-made order for `table`, as stored (may name items no longer there). */
export function customOrder(budget, table) {
  const o = obj(obj(budget).setup_order)[table];
  return Array.isArray(o) ? o.filter((n) => typeof n === 'string') : [];
}

/** Whether `table` has a hand-made order (one that isn't byte order). */
export function hasCustomOrder(budget, table) {
  return customOrder(budget, table).length > 0;
}

/** Every name in `table`, in the order the budget shows it. */
export function orderedNames(budget, table) {
  const defs = obj(obj(budget)[table]);
  const all = Object.keys(defs).sort(byteOrder);
  const have = new Set(all);
  const seen = new Set();
  const out = [];
  for (const n of customOrder(budget, table)) {
    if (have.has(n) && !seen.has(n)) {
      seen.add(n);
      out.push(n);
    }
  }
  for (const n of all) if (!seen.has(n)) out.push(n);
  return out;
}

/** A fringe's or group's short code, or ''. */
export function shortCode(budget, table, name) {
  const def = obj(obj(obj(budget)[table])[name]);
  return typeof def.short_code === 'string' ? def.short_code : '';
}

/**
 * A line's fringes or groups for the grid: `text` with each item's short
 * code where it has one (else its name), `title` with the full names when
 * any code stood in for one (else ''). `edit` is always the names, so
 * editing and copying use names.
 */
export function tagCellText(budget, table, list) {
  const names = Array.isArray(list) ? list.filter((n) => typeof n === 'string') : [];
  const edit = names.join(', ');
  let coded = false;
  const shown = names.map((n) => {
    const c = shortCode(budget, table, n);
    if (c) coded = true;
    return c || n;
  });
  return { text: shown.join(', '), edit, title: coded ? names.map((n) => {
    const c = shortCode(budget, table, n);
    return c ? `${c}: ${n}` : n;
  }).join('\n') : '' };
}

/** The non-blank notes in a notes array (a row's, account's or category's). */
export function noteList(notes) {
  return Array.isArray(notes) ? notes.filter((n) => n && typeof n.text === 'string' && n.text.trim()) : [];
}

const TIP_CHARS = 240;
const TIP_NOTES = 3;

/** A tooltip summing up some notes ('' for none). */
export function notesTitle(notes) {
  const list = noteList(notes);
  if (!list.length) return '';
  const cut = (s) => (s.length > TIP_CHARS ? `${s.slice(0, TIP_CHARS).trimEnd()}…` : s);
  const lines = list.slice(0, TIP_NOTES).map((n) => `${n.done ? '✓ ' : ''}${cut(n.text.trim())}`);
  if (list.length > TIP_NOTES) lines.push(t('order.and_more', { count: list.length - TIP_NOTES }));
  return t('order.notes_title', { count: list.length, lines: lines.join('\n') });
}
