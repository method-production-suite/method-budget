// Renaming a setup item everywhere (src/rename.rs): a global, fringe, group,
// unit, set, location or currency, with every line, formula and setting that
// names it rewritten, as one undo step. Opened from an item's editor (the
// Rename button), its right-click menu in the tools panel, and the Find
// tab's "Rename everywhere" button.
//
// The checks here only save a round trip; Rust checks again, and also
// refuses a new name something already uses without an item behind it (a
// unit typed on lines, a global a formula names), so a rename never merges
// two names.

import { anchoredForm } from './modal.js';
import { openContextMenu } from './menu.js';
import { saveItemToLibrary } from './library.js';

const GLOBAL_NAME = /^[A-Za-z_][A-Za-z0-9_]*$/;

/** Each kind: what it is called, and its table in the view's budget. */
export const RENAME_KINDS = {
  global: { what: 'global', table: 'globals' },
  fringe: { what: 'fringe', table: 'fringes' },
  group: { what: 'group', table: 'groups' },
  unit: { what: 'unit', table: 'units' },
  set: { what: 'set', table: 'sets' },
  location: { what: 'location', table: 'locations' },
  currency: { what: 'currency', table: 'currencies' },
};

function has(obj, k) {
  return Boolean(obj) && typeof obj === 'object' && Object.prototype.hasOwnProperty.call(obj, k);
}

/** Why `to` can't be the new name of `kind` item `from`, or null. */
export function renameProblem(kind, from, to, budget) {
  const info = RENAME_KINDS[kind];
  if (!info) return 'That can’t be renamed.';
  const b = budget && typeof budget === 'object' ? budget : {};
  if (to === '') return kind === 'currency' ? 'Enter a currency code.' : `Enter a ${info.what} name.`;
  if (kind === 'global') {
    if (!GLOBAL_NAME.test(to)) return `“${to}” can’t be a global name: use letters, digits and _, starting with a letter.`;
  } else if (to.includes(',')) {
    return kind === 'currency' ? 'Codes can’t contain commas.' : 'Names can’t contain commas.';
  }
  if (to === from) return null;
  if (kind === 'currency' && (to === b.base_currency || has(b.currencies, to))) return `${to} is already a currency here.`;
  if (kind !== 'currency' && has(b[info.table], to)) return `There is already a ${info.what} named ${to}.`;
  return null;
}

/** What a finished rename says: the new name, and how many places changed. */
export function renamedText(kind, from, reply) {
  const info = RENAME_KINDS[kind] || { what: 'item' };
  const n = Number(reply && reply.places) || 0;
  const to = reply && typeof reply.name === 'string' ? reply.name : '';
  const where = n === 0 ? 'nothing used it yet' : n === 1 ? '1 place updated' : `${n} places updated`;
  return `Renamed ${info.what} ${from} to ${to}: ${where}.`;
}

/**
 * Opens the Rename form next to `anchor` for item `name` of `kind`.
 * ctx: { call(cmd, args), toast(message), budget() }. Resolves to the new
 * name, or null when nothing was renamed.
 */
export function renameItem({ anchor, kind, name, ctx }) {
  const info = RENAME_KINDS[kind];
  if (!info || !ctx || typeof ctx.call !== 'function') return Promise.resolve(null);
  let reply = null;
  return anchoredForm({
    anchor,
    title: `Rename ${info.what} ${name}`,
    message:
      kind === 'global'
        ? 'Every formula that uses it changes with it, as one step you can undo. Totals stay the same.'
        : 'Every line and setting that uses it changes with it, as one step you can undo. Totals stay the same.',
    fields: [{ key: 'name', label: kind === 'currency' ? 'New code' : 'New name', value: name }],
    submitText: 'Rename',
    onSubmit: (v) => {
      const to = String(v.name || '').trim();
      const bad = renameProblem(kind, name, to, ctx.budget ? ctx.budget() : {});
      if (bad) return Promise.reject(bad);
      if (to === name) return Promise.resolve();
      return Promise.resolve(ctx.call('rename_setup_item', { kind, from: name, to })).then((r) => {
        reply = r;
      });
    },
  }).then(() => {
    if (!reply) return null;
    if (typeof ctx.toast === 'function') ctx.toast(renamedText(kind, name, reply));
    return reply.name;
  });
}

/**
 * The right-click menu of an item in a tools panel list: Edit (when
 * `edit` is given), Rename, Save to Library and Delete (when `remove` is
 * given). `at` is the
 * mouse event or a point.
 */
export function itemMenu(at, { kind, name, anchor, ctx, edit = null, remove = null, onRenamed = null, extra = [] }) {
  const info = RENAME_KINDS[kind];
  if (!info) return;
  const items = [];
  if (edit) items.push({ label: 'Edit…', run: edit });
  items.push({
    label: 'Rename everywhere…',
    title: 'Give it a new name; every line, formula and setting that uses it follows',
    run: () =>
      renameItem({ anchor, kind, name, ctx }).then((to) => {
        if (to !== null && onRenamed) onRenamed(to);
      }),
  });
  // More of the item's own actions (a fringe's or group's short code).
  for (const x of extra) items.push(x);
  // A copy into the personal setup library, with what it needs (library.js).
  items.push({
    label: 'Save to Library…',
    title: 'Copy it, and what it needs, into your setup library',
    run: () => saveItemToLibrary(ctx, kind, name),
  });
  if (remove) items.push({ separator: true }, { label: 'Delete…', run: remove });
  const point = at && typeof at.clientX === 'number' ? { x: at.clientX, y: at.clientY } : at;
  openContextMenu(point, { label: `${info.what} ${name}`, items, returnFocus: () => anchor && anchor.focus && anchor.focus() });
}
