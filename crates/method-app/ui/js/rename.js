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
import { t } from './i18n.js';

// Letters, digits and _, with at least one letter or _ (decisions.md, entry 56).
const GLOBAL_NAME = /^(?=[A-Za-z0-9_]*[A-Za-z_])[A-Za-z0-9_]+$/;

/** Each kind: what it is called, and its table in the view's budget. */
export const RENAME_KINDS = {
  global: { what: t('rename.kind.global'), table: 'globals' },
  fringe: { what: t('rename.kind.fringe'), table: 'fringes' },
  group: { what: t('rename.kind.group'), table: 'groups' },
  unit: { what: t('rename.kind.unit'), table: 'units' },
  set: { what: t('rename.kind.set'), table: 'sets' },
  location: { what: t('rename.kind.location'), table: 'locations' },
  currency: { what: t('rename.kind.currency'), table: 'currencies' },
};

function has(obj, k) {
  return Boolean(obj) && typeof obj === 'object' && Object.prototype.hasOwnProperty.call(obj, k);
}

/** Why `to` can't be the new name of `kind` item `from`, or null. */
export function renameProblem(kind, from, to, budget) {
  const info = RENAME_KINDS[kind];
  if (!info) return t('rename.cannot');
  const b = budget && typeof budget === 'object' ? budget : {};
  if (to === '') return kind === 'currency' ? t('rename.enter_code') : t('rename.enter_name', { what: info.what });
  if (kind === 'global') {
    if (!GLOBAL_NAME.test(to)) return t('rename.bad_global', { name: to });
  } else if (to.includes(',')) {
    return kind === 'currency' ? t('rename.code_commas') : t('rename.name_commas');
  }
  if (to === from) return null;
  if (kind === 'currency' && (to === b.base_currency || has(b.currencies, to))) return t('rename.currency_taken', { name: to });
  if (kind !== 'currency' && has(b[info.table], to)) return t('rename.name_taken', { what: info.what, name: to });
  return null;
}

/** What a finished rename says: the new name, and how many places changed. */
export function renamedText(kind, from, reply) {
  const info = RENAME_KINDS[kind] || { what: t('rename.kind.item') };
  const n = Number(reply && reply.places) || 0;
  const to = reply && typeof reply.name === 'string' ? reply.name : '';
  if (n === 0) return t('rename.renamed_none', { what: info.what, from, to });
  return t('rename.renamed', { count: n, what: info.what, from, to });
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
    title: t('rename.title', { what: info.what, name }),
    message:
      kind === 'global'
        ? t('rename.message_global')
        : t('rename.message'),
    fields: [{ key: 'name', label: kind === 'currency' ? t('rename.new_code') : t('rename.new_name'), value: name }],
    submitText: t('rename.rename'),
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
  if (edit) items.push({ label: t('rename.edit'), run: edit });
  items.push({
    label: t('rename.rename_everywhere'),
    title: t('rename.rename_everywhere_title'),
    run: () =>
      renameItem({ anchor, kind, name, ctx }).then((to) => {
        if (to !== null && onRenamed) onRenamed(to);
      }),
  });
  // More of the item's own actions (a fringe's or group's short code).
  for (const x of extra) items.push(x);
  // A copy into the personal setup library, with what it needs (library.js).
  items.push({
    label: t('rename.save_to_library'),
    title: t('rename.save_to_library_title'),
    run: () => saveItemToLibrary(ctx, kind, name),
  });
  if (remove) items.push({ separator: true }, { label: t('rename.delete'), run: remove });
  const point = at && typeof at.clientX === 'number' ? { x: at.clientX, y: at.clientY } : at;
  openContextMenu(point, { label: t('rename.menu_label', { what: info.what, name }), items, returnFocus: () => anchor && anchor.focus && anchor.focus() });
}
