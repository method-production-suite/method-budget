// Tag presets and line blocks (src/presets.rs): the Presets tab of the tools
// panel (tools.js) and the grid's right-click submenus (context.js).
//
// A tag preset is a named bundle of a line's tags (fringes, groups, set,
// location, currency). Add puts it on the selection on top of what the
// lines have (a set, location or currency only where a line has none);
// Replace makes the lines' tags exactly the preset's. The selection is the
// selected lines, or every line under the selected accounts or categories,
// as for the apply lists (Rust resolves accounts and categories).
//
// A line block is a named copy of rows. Insert puts fresh copies above the
// cursor line (at the end of the account when the cursor is on no line), as
// one undo step; when the block uses setup items this budget no longer has,
// the same review as pasting rows lists them first (clipboard.js).
//
// Keys on a focused preset or block: Enter adds (a block: inserts),
// Shift+Enter replaces, F2 (or a double-click on the name) edits or renames,
// Delete deletes after asking, Option+Shift+Up and Down move it in its list.
//
// Budget text only reaches the page through textContent (dom.js el()), and
// styling is classes (apply.css, tools.css, presets.css): no inline styles,
// no innerHTML.

import { invoke } from './api.js';
import { el, isMac } from './dom.js';
import { anchoredForm } from './modal.js';
import { review } from './clipboard.js';
import { obj, str, errText } from './util.js';
import { t } from './i18n.js';

const st = {
  view: null,
  ctx: null,
  host: null,
  busy: false,
  error: '',
  errorSig: '',
};

// ---------------------------------------------------------------------------
// Small helpers

function list(v) {
  return Array.isArray(v) ? v : [];
}

function byteOrder(a, b) {
  return a < b ? -1 : a > b ? 1 : 0;
}

function budgetOf(view) {
  return obj(view && view.budget);
}

/** The budget's tag presets, in the file's shape. */
export function tagPresets(view) {
  return list(budgetOf(view).tag_presets).map(obj);
}

/** The budget's line blocks as the view lists them: { name, count }. */
export function lineBlocks(view) {
  return list(budgetOf(view).line_presets).map(obj);
}

/**
 * What a selection means, as the preset commands take it: detail rows by id,
 * accounts by number, categories by topsheet position. `sel` is the grid's
 * { level, keys, cat } (main.js currentSelection, or the tools window's copy).
 */
export function scopeOf(sel, view) {
  const out = { rowIds: [], accounts: [], categories: [] };
  if (!sel || !Array.isArray(sel.keys)) return out;
  const top = list(budgetOf(view).topsheet);
  const index = (k, prefix) => (typeof k === 'string' && k.startsWith(prefix) ? Number(k.slice(prefix.length)) : NaN);
  if (sel.level === 'details') {
    out.rowIds = sel.keys.map((k) => index(k, 'r')).filter(Number.isSafeInteger);
  } else if (sel.level === 'accounts') {
    const cat = sel.cat ? top[sel.cat.ti] : null;
    if (cat && cat.kind !== 'production_total' && cat.code === sel.cat.code) {
      const accts = list(cat.accounts);
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

function emptyScope(sc) {
  return !sc.rowIds.length && !sc.accounts.length && !sc.categories.length;
}

/** The cursor's line, when the cursor is on a detail row: its id, else null. */
function cursorRowId(sel) {
  if (!sel || sel.level !== 'details' || typeof sel.cursor !== 'string' || !/^r\d+$/.test(sel.cursor)) return null;
  return Number(sel.cursor.slice(1));
}

/** The rows of the account on screen (the view's `account`), by id. */
function shownRows(view) {
  const a = obj(view && view.account);
  return list(a.details).map(obj);
}

/** Where a block goes from here: { account, beforeRow } or { error }. */
function insertPlace(sel) {
  if (!sel || sel.level !== 'details' || !sel.account || !sel.account.code) {
    return { error: t('presets.open_account_first') };
  }
  return { account: str(sel.account.code), beforeRow: cursorRowId(sel) };
}

// A set, location or currency with its value, in words.
const TAG_WORDS = [
  ['set', (value) => t('presets.tag.set', { value })],
  ['location', (value) => t('presets.tag.location', { value })],
  ['currency', (value) => t('presets.tag.currency', { value })],
];

/** The parts of a tag preset in words: [short, long]. */
function describeTags(p) {
  const parts = [];
  const long = [];
  const fr = list(p.fringes).map(str);
  const gr = list(p.groups).map(str);
  if (fr.length) {
    parts.push(fr.join(', '));
    long.push(t('presets.tag.fringes', { value: fr.join(', ') }));
  }
  if (gr.length) {
    parts.push(gr.join(', '));
    long.push(t('presets.tag.groups', { value: gr.join(', ') }));
  }
  for (const [k, label] of TAG_WORDS) {
    if (str(p[k])) {
      parts.push(str(p[k]));
      long.push(label(str(p[k])));
    }
  }
  if (!parts.length) return [t('presets.no_tags'), t('presets.no_tags_long')];
  return [parts.join(' · '), long.join('\n')];
}

// ---------------------------------------------------------------------------
// Running commands

/**
 * Runs `cmd` through the panel's (or menu's) `call`, one at a time; a refusal
 * shows in the tab (`toast` from the menu). Resolves with the view, or null.
 */
async function run(call, cmd, args, opts, onError) {
  if (st.busy) return null;
  st.busy = true;
  setError('');
  try {
    return await call(cmd, args, opts);
  } catch (e) {
    onError(errText(e));
    return null;
  } finally {
    st.busy = false;
    redraw();
  }
}

function setError(message) {
  st.error = message;
  st.errorSig = message ? JSON.stringify(scopeOf(selection(), st.view)) : '';
}

function selection() {
  return st.ctx && typeof st.ctx.selection === 'function' ? st.ctx.selection() : null;
}

function panelCall(cmd, args, opts) {
  if (!st.ctx || typeof st.ctx.call !== 'function') return Promise.reject(t('presets.not_connected'));
  return st.ctx.call(cmd, args, opts);
}

function panelError(message) {
  setError(message);
  redraw();
}

/** Puts tag preset `index` on the selection: `mode` 'add' or 'replace'. */
export function applyPreset({ call, sel, view, index, mode, onError }) {
  const sc = scopeOf(sel, view);
  if (emptyScope(sc)) {
    onError(t('presets.select_first'));
    return Promise.resolve(null);
  }
  return run(call, 'apply_tag_preset', { index, mode, ...sc }, {}, onError);
}

/**
 * Inserts block `index` above the cursor line, after the review when the
 * block uses setup items this budget no longer has.
 */
export async function insertBlock({ call, sel, index, onError }) {
  const place = insertPlace(sel);
  if (place.error) {
    onError(place.error);
    return null;
  }
  let preview = null;
  try {
    preview = await invoke('line_preset_preview', { index });
  } catch (e) {
    onError(errText(e));
    return null;
  }
  if (preview && preview.review) {
    const ok = await review(preview, { action: t('presets.insert') });
    if (!ok) return null;
  }
  return run(call, 'insert_line_preset', { index, account: place.account, beforeRow: place.beforeRow }, { focusCreated: true }, onError);
}

// ---------------------------------------------------------------------------
// Saving: tags from the cursor line, a block from the selected lines

/** "Save tags as preset…": name and which of the cursor line's tags. */
export function saveTagsForm({ call, sel, view, anchor, onError, onSaved = () => {} }) {
  const id = cursorRowId(sel);
  const d = id === null ? null : shownRows(view).find((r) => r.id === id);
  const kind = d ? d.kind || 'line' : '';
  if (!d || (kind !== 'line' && kind !== 'adjustment')) {
    onError(t('presets.cursor_on_line_first'));
    return Promise.resolve(false);
  }
  const line = kind === 'line';
  const value = (k) => (Array.isArray(d[k]) ? d[k].join(', ') : str(d[k]));
  // Each tag part with the line's value, or "(none)".
  const words = {
    fringes: [(v) => t('presets.tag.fringes', { value: v }), () => t('presets.tag.fringes_none')],
    groups: [(v) => t('presets.tag.groups', { value: v }), () => t('presets.tag.groups_none')],
    set: [(v) => t('presets.tag.set', { value: v }), () => t('presets.tag.set_none')],
    location: [(v) => t('presets.tag.location', { value: v }), () => t('presets.tag.location_none')],
    currency: [(v) => t('presets.tag.currency', { value: v }), () => t('presets.tag.currency_none')],
  };
  const part = (key, has) => ({
    key,
    label: value(key) ? words[key][0](value(key)) : words[key][1](),
    type: 'check',
    value: has && Boolean(value(key)),
  });
  const fields = [{ key: 'name', label: t('presets.preset_name'), value: str(d.description).trim().slice(0, 80) }];
  if (line) fields.push(part('fringes', true));
  fields.push(part('groups', true), part('set', true), part('location', true));
  if (line) fields.push(part('currency', true));
  let saved = false;
  return anchoredForm({
    anchor,
    title: t('presets.save_tags_title'),
    message: t('presets.save_tags_message'),
    fields,
    submitText: t('modal.save'),
    onSubmit: (v) => {
      const parts = {
        fringes: line && Boolean(v.fringes),
        groups: Boolean(v.groups),
        set: Boolean(v.set),
        location: Boolean(v.location),
        currency: line && Boolean(v.currency),
      };
      return call('tag_preset_from_line', { rowId: id, name: v.name, parts }).then(() => {
        saved = true;
      });
    },
  }).then(() => {
    if (saved) onSaved();
    return saved;
  });
}

/** "Save lines as block…": name and which fields to keep. */
export function saveBlockForm({ call, sel, view, anchor, onError, onSaved = () => {} }) {
  const keys = sel && sel.level === 'details' && Array.isArray(sel.keys) ? sel.keys : [];
  const ids = keys.filter((k) => /^r\d+$/.test(k)).map((k) => Number(k.slice(1)));
  if (!ids.length) {
    onError(t('presets.select_lines_first'));
    return Promise.resolve(false);
  }
  const rows = shownRows(view).filter((r) => ids.includes(r.id));
  const has = (pred) => rows.some(pred);
  const filled = (r, ks) => ks.some((k) => (Array.isArray(r[k]) ? r[k].length > 0 : str(r[k]).trim() !== '' && r[k] !== null));
  // A field nothing on the lines uses starts unticked, the rest ticked.
  const fields = [
    { key: 'name', label: t('presets.block_name'), value: '' },
    { key: 'formulas', label: t('presets.keep.formulas'), type: 'check', value: has((r) => filled(r, ['amount', 'x', 'rate', 'x4'])) },
    { key: 'units', label: t('presets.keep.units'), type: 'check', value: has((r) => filled(r, ['unit', 'unit2', 'unit3', 'unit4'])) },
    {
      key: 'tags',
      label: t('presets.keep.tags'),
      type: 'check',
      value: has((r) => filled(r, ['fringes', 'groups', 'set', 'location', 'currency', 'fringe_range'])),
    },
    { key: 'notes', label: t('presets.keep.notes'), type: 'check', value: has((r) => filled(r, ['notes'])) },
  ];
  let saved = false;
  return anchoredForm({
    anchor,
    title: t('presets.save_block_title', { count: ids.length }),
    message: t('presets.save_block_message'),
    fields,
    submitText: t('modal.save'),
    onSubmit: (v) => {
      const keep = { formulas: Boolean(v.formulas), units: Boolean(v.units), tags: Boolean(v.tags), notes: Boolean(v.notes) };
      return call('save_line_preset', { name: v.name, rowIds: ids, keep }).then(() => {
        saved = true;
      });
    },
  }).then(() => {
    if (saved) onSaved();
    return saved;
  });
}

// ---------------------------------------------------------------------------
// Editing, renaming, deleting and moving (the tab)

/** The editor for tag preset `index`: its name and every part. */
function editTagPreset(index, anchor) {
  const p = tagPresets(st.view)[index];
  if (!p) return;
  const b = budgetOf(st.view);
  const base = str(b.base_currency);
  const fields = [{ key: 'name', label: t('presets.name'), value: str(p.name) }];
  // Every defined fringe and group, plus any the preset names that the
  // budget no longer has (untick them to save).
  const checks = (kind, label, defs, chosen) => {
    const names = [...new Set([...Object.keys(obj(defs)), ...chosen])].sort(byteOrder);
    for (const n of names) {
      const missing = !Object.prototype.hasOwnProperty.call(obj(defs), n);
      fields.push({ key: `${kind}:${n}`, label: label(n, missing), type: 'check', value: chosen.includes(n) });
    }
  };
  checks('f', (name, missing) => (missing ? t('presets.edit.fringe_missing', { name }) : t('presets.edit.fringe', { name })), b.fringes, list(p.fringes).map(str));
  checks('g', (name, missing) => (missing ? t('presets.edit.group_missing', { name }) : t('presets.edit.group', { name })), b.groups, list(p.groups).map(str));
  const pick = (key, label, defs, current, noneLabel) => {
    const names = Object.keys(obj(defs)).sort(byteOrder);
    if (current && !names.includes(current)) names.push(current);
    fields.push({ key, label, type: 'select', options: [['', noneLabel], ...names.map((n) => [n, n])], value: current });
  };
  pick('set', t('presets.edit.set'), b.sets, str(p.set), t('presets.edit.none'));
  pick('location', t('presets.edit.location'), b.locations, str(p.location), t('presets.edit.none'));
  pick('currency', t('presets.edit.currency'), b.currencies, str(p.currency), base ? t('presets.edit.base_currency_named', { base }) : t('presets.edit.base_currency'));
  const build = (v) => {
    const chosen = (prefix) =>
      Object.keys(v)
        .filter((k) => k.startsWith(prefix) && v[k] === true)
        .map((k) => k.slice(prefix.length));
    const out = { name: v.name };
    const fr = chosen('f:');
    const gr = chosen('g:');
    if (fr.length) out.fringes = fr;
    if (gr.length) out.groups = gr;
    for (const k of ['set', 'location', 'currency']) if (v[k]) out[k] = v[k];
    return out;
  };
  const start = Object.fromEntries(fields.map((f) => [f.key, f.type === 'check' ? Boolean(f.value) : str(f.value)]));
  const unchanged = JSON.stringify(build(start));
  let deleting = false;
  const key = tagKey(index);
  anchoredForm({
    anchor,
    title: t('presets.edit.title', { name: str(p.name) }),
    fields,
    extra: { text: t('presets.delete_more'), title: t('presets.delete_tag_preset'), run: () => (deleting = true) },
    onSubmit: (v) => {
      const preset = build(v);
      if (JSON.stringify(preset) === unchanged) return Promise.resolve();
      return panelCall('save_tag_preset', { index, preset });
    },
  }).then(() => {
    if (deleting) deleteItem('tags', index);
    else focusKey(key);
  });
}

/** Renames line block `index`. */
function renameBlock(index, anchor) {
  const p = lineBlocks(st.view)[index];
  if (!p) return;
  let deleting = false;
  anchoredForm({
    anchor,
    title: t('presets.block_title', { name: str(p.name) }),
    message: t('presets.rows', { count: Number(p.count) || 0 }),
    fields: [{ key: 'name', label: t('presets.name'), value: str(p.name) }],
    extra: { text: t('presets.delete_more'), title: t('presets.delete_line_block'), run: () => (deleting = true) },
    onSubmit: (v) => {
      if (v.name.trim() === str(p.name)) return Promise.resolve();
      return panelCall('rename_tool_preset', { which: 'lines', index, name: v.name });
    },
  }).then(() => {
    if (deleting) deleteItem('lines', index);
    else focusKey(blockKey(index));
  });
}

/** Asks, next to the item, then deletes it (one undo step). */
function deleteItem(which, index) {
  const items = which === 'tags' ? tagPresets(st.view) : lineBlocks(st.view);
  const p = items[index];
  if (!p) return;
  const key = which === 'tags' ? tagKey(index) : blockKey(index);
  let done = false;
  anchoredForm({
    anchor: byKey(key),
    title: which === 'tags' ? t('presets.delete_tag_title', { name: str(p.name) }) : t('presets.delete_block_title', { name: str(p.name) }),
    message: which === 'tags' ? t('presets.delete_tag_message') : t('presets.delete_block_message'),
    submitText: t('presets.delete'),
    danger: true,
    onSubmit: () =>
      panelCall('delete_tool_preset', { which, index }).then(() => {
        done = true;
      }),
  }).then(() => {
    if (!done) return focusKey(key);
    const left = items.length - 1;
    const next = Math.min(index, left - 1);
    focusKey(next >= 0 ? (which === 'tags' ? tagKey(next) : blockKey(next)) : which === 'tags' ? 'presets:save-tags' : 'presets:save-block');
  });
}

function moveItem(which, index, step) {
  const n = which === 'tags' ? tagPresets(st.view).length : lineBlocks(st.view).length;
  const to = index + step;
  if (to < 0 || to >= n) return;
  run(panelCall, 'move_tool_preset', { which, index, to }, {}, panelError).then((v) => {
    if (v) focusKey(which === 'tags' ? tagKey(to) : blockKey(to));
  });
}

// ---------------------------------------------------------------------------
// Drawing the tab

function tagKey(i) {
  return `presets:tag:${i}`;
}

function blockKey(i) {
  return `presets:block:${i}`;
}

function byKey(key) {
  const root = st.host && st.host.isConnected ? st.host : document;
  return [...root.querySelectorAll('[data-key]')].find((e) => e.dataset.key === key) || null;
}

function focusKey(key) {
  const target = byKey(key);
  if (target) target.focus({ preventScroll: false });
}

function redraw() {
  if (st.host && st.host.isConnected) draw(st.host);
}

/** Draws the Presets tab into `host` (tools.js, on every view). */
export function renderPresets(host, view, ctx) {
  st.view = view && typeof view === 'object' ? view : {};
  st.ctx = ctx || {};
  st.host = host;
  draw(host);
}

// A new selection: an old message was about the old one.
document.addEventListener('method:selection', () => {
  if (!st.error || !st.host || !st.host.isConnected) return;
  if (JSON.stringify(scopeOf(selection(), st.view)) !== st.errorSig) {
    setError('');
    redraw();
  }
});

function smallButton(text, key, onClick, { title = '', label = '', enabled = true } = {}) {
  const b = el('button', {
    class: 'setup-btn apply-btn',
    text,
    title: title || undefined,
    attrs: { type: 'button', 'aria-label': label || null },
    data: { key, nobusy: '1' },
  });
  if (!enabled || st.busy) b.setAttribute('aria-disabled', 'true');
  b.addEventListener('click', () => {
    if (b.getAttribute('aria-disabled') === 'true') return;
    onClick(b);
  });
  return b;
}

function addButton(text, key, title, onClick) {
  const b = el('button', {
    class: 'apply-add-btn',
    text,
    title,
    attrs: { type: 'button' },
    data: { key, nobusy: '1' },
  });
  b.addEventListener('click', () => onClick(b));
  b.addEventListener('keydown', (e) => moveFocus(e, b));
  return el('li', { class: 'apply-add' }, b);
}

/** Up and Down (Home and End) move between a list's items and its add row. */
function moveFocus(e, from) {
  if (e.altKey || e.metaKey || e.ctrlKey || e.shiftKey) return;
  const ul = from.closest('.apply-items');
  if (!ul) return;
  const items = [...ul.querySelectorAll('.preset-name, .apply-add-btn')];
  const i = items.indexOf(from);
  let j = null;
  if (e.key === 'ArrowDown') j = i + 1;
  else if (e.key === 'ArrowUp') j = i - 1;
  else if (e.key === 'Home') j = 0;
  else if (e.key === 'End') j = items.length - 1;
  if (j === null || !items[j]) return;
  e.preventDefault();
  items[j].focus();
}

function itemKeys(e, b, { primary, secondary, edit, remove, move }) {
  const plain = !e.metaKey && !e.ctrlKey && !e.altKey;
  if (e.key === 'Enter' && plain && !e.shiftKey) {
    e.preventDefault();
    primary();
  } else if (e.key === 'Enter' && plain && e.shiftKey && secondary) {
    e.preventDefault();
    secondary();
  } else if (e.key === 'F2' && plain && !e.shiftKey) {
    e.preventDefault();
    edit();
  } else if ((e.key === 'Delete' || e.key === 'Backspace') && plain && !e.shiftKey) {
    e.preventDefault();
    remove();
  } else if (e.altKey && e.shiftKey && !e.metaKey && !e.ctrlKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown')) {
    e.preventDefault();
    e.stopPropagation();
    move(e.key === 'ArrowUp' ? -1 : 1);
  } else {
    moveFocus(e, b);
  }
}

function draw(host) {
  const sel = selection();
  const sc = scopeOf(sel, st.view);
  const place = insertPlace(sel);
  const tags = tagPresets(st.view);
  const blocks = lineBlocks(st.view);
  const moveKeys = isMac ? t('presets.move_keys_mac') : t('presets.move_keys');
  const parts = [];

  // Tag presets
  parts.push(el('h3', { class: 'notes-heading', text: t('presets.tag_presets') }));
  if (!tags.length) parts.push(el('p', { class: 'setup-empty', text: t('presets.no_tag_presets_hint') }));
  const tagList = el('ul', { class: 'apply-items presets-list', attrs: { 'aria-label': t('presets.tag_presets') } });
  const canApply = !emptyScope(sc);
  tags.forEach((p, i) => {
    const [short, long] = describeTags(p);
    const add = () => applyPreset({ call: panelCall, sel: selection(), view: st.view, index: i, mode: 'add', onError: panelError });
    const replace = () => applyPreset({ call: panelCall, sel: selection(), view: st.view, index: i, mode: 'replace', onError: panelError });
    const name = el(
      'button',
      {
        class: 'preset-name',
        title: `${long}\n${t('presets.tag_keys', { keys: moveKeys })}`,
        attrs: { type: 'button', 'aria-keyshortcuts': 'Enter Shift+Enter F2 Delete Alt+Shift+ArrowUp Alt+Shift+ArrowDown' },
        data: { key: tagKey(i), nobusy: '1' },
      },
      el('span', { class: 'apply-name', text: str(p.name) }),
      el('span', { class: 'apply-desc', text: short }),
    );
    name.addEventListener('dblclick', (e) => {
      e.preventDefault();
      editTagPreset(i, name);
    });
    name.addEventListener('keydown', (e) =>
      itemKeys(e, name, {
        primary: add,
        secondary: replace,
        edit: () => editTagPreset(i, name),
        remove: () => deleteItem('tags', i),
        move: (step) => moveItem('tags', i, step),
      }),
    );
    tagList.append(
      el(
        'li',
        { class: 'preset-item' },
        name,
        smallButton(t('presets.add'), `presets:add:${i}`, add, {
          label: t('presets.add_named', { name: str(p.name) }),
          title: canApply ? t('presets.add_tip') : t('presets.add_tip_select'),
          enabled: canApply,
        }),
        smallButton(t('presets.replace'), `presets:replace:${i}`, replace, {
          label: t('presets.replace_named', { name: str(p.name) }),
          title: canApply ? t('presets.replace_tip') : t('presets.replace_tip_select'),
          enabled: canApply,
        }),
      ),
    );
  });
  tagList.append(
    addButton(t('presets.save_tags_more'), 'presets:save-tags', t('presets.save_tags_tip'), (b) =>
      saveTagsForm({ call: panelCall, sel: selection(), view: st.view, anchor: b, onError: panelError }),
    ),
  );
  parts.push(tagList);

  // Line blocks
  parts.push(el('h3', { class: 'notes-heading', text: t('presets.line_blocks') }));
  if (!blocks.length) parts.push(el('p', { class: 'setup-empty', text: t('presets.no_line_blocks_hint') }));
  const blockList = el('ul', { class: 'apply-items presets-list', attrs: { 'aria-label': t('presets.line_blocks') } });
  blocks.forEach((p, i) => {
    const count = Number(p.count) || 0;
    const insert = () => insertBlock({ call: panelCall, sel: selection(), index: i, onError: panelError });
    const name = el(
      'button',
      {
        class: 'preset-name',
        title: `${t('presets.rows', { count })}\n${t('presets.block_keys', { keys: moveKeys })}`,
        attrs: { type: 'button', 'aria-keyshortcuts': 'Enter F2 Delete Alt+Shift+ArrowUp Alt+Shift+ArrowDown' },
        data: { key: blockKey(i), nobusy: '1' },
      },
      el('span', { class: 'apply-name', text: str(p.name) }),
      el('span', { class: 'apply-desc', text: t('presets.rows', { count }) }),
    );
    name.addEventListener('dblclick', (e) => {
      e.preventDefault();
      renameBlock(i, name);
    });
    name.addEventListener('keydown', (e) =>
      itemKeys(e, name, {
        primary: insert,
        secondary: null,
        edit: () => renameBlock(i, name),
        remove: () => deleteItem('lines', i),
        move: (step) => moveItem('lines', i, step),
      }),
    );
    blockList.append(
      el(
        'li',
        { class: 'preset-item preset-block' },
        name,
        smallButton(t('presets.insert'), `presets:insert:${i}`, insert, {
          label: t('presets.insert_named', { name: str(p.name) }),
          title: place.error || t('presets.insert_tip'),
          enabled: !place.error,
        }),
      ),
    );
  });
  blockList.append(
    addButton(t('presets.save_block_more'), 'presets:save-block', t('presets.save_block_tip'), (b) =>
      saveBlockForm({ call: panelCall, sel: selection(), view: st.view, anchor: b, onError: panelError }),
    ),
  );
  parts.push(blockList);

  if (st.error) parts.push(el('p', { class: 'setup-error apply-error', text: st.error, attrs: { role: 'alert' } }));
  const box = el('div', { class: 'apply presets', attrs: { 'aria-busy': st.busy ? 'true' : 'false' } }, ...parts);
  // Keep focus on the control it was on across a redraw.
  const a = document.activeElement;
  const had = a && host.contains(a) && a.dataset ? a.dataset.key : null;
  host.replaceChildren(box);
  if (had) {
    const again = byKey(had);
    if (again) again.focus({ preventScroll: true });
  }
}

// ---------------------------------------------------------------------------
// The grid's right-click menu (context.js)

/**
 * The "Apply preset" submenu: each tag preset with Add and Replace, then the
 * ways to save one and set them up. `c`: { view, sel, call, toast, anchor(),
 * openTab() }.
 */
export function presetMenu(c) {
  const sc = scopeOf(c.sel, c.view);
  const none = emptyScope(sc);
  const items = tagPresets(c.view).map((p, index) => {
    const [short, long] = describeTags(p);
    const go = (mode) => () => applyPreset({ call: c.call, sel: c.sel, view: c.view, index, mode, onError: c.toast });
    return {
      label: str(p.name),
      title: long || short,
      disabled: none,
      items: [
        { label: t('presets.add_tags'), title: t('presets.add_tip'), run: go('add') },
        { label: t('presets.replace_tags'), title: t('presets.replace_tip'), run: go('replace') },
      ],
    };
  });
  if (!items.length) items.push({ label: t('presets.no_tag_presets'), disabled: true });
  items.push({ separator: true });
  if (c.sel && c.sel.level === 'details') {
    items.push({
      label: t('presets.save_tags_more'),
      disabled: cursorRowId(c.sel) === null,
      run: () => saveTagsForm({ call: c.call, sel: c.sel, view: c.view, anchor: c.anchor(), onError: c.toast }),
    });
  }
  items.push({ label: t('presets.set_up_more'), run: c.openTab });
  return items;
}

/** The "Insert line block" submenu (detail lines only). */
export function blockMenu(c) {
  const items = lineBlocks(c.view).map((p, index) => ({
    label: str(p.name),
    title: t('presets.rows', { count: Number(p.count) || 0 }),
    run: () => insertBlock({ call: c.call, sel: c.sel, index, onError: c.toast }),
  }));
  if (!items.length) items.push({ label: t('presets.no_line_blocks'), disabled: true });
  items.push(
    { separator: true },
    {
      label: t('presets.save_block_more'),
      disabled: !scopeOf(c.sel, c.view).rowIds.length,
      run: () => saveBlockForm({ call: c.call, sel: c.sel, view: c.view, anchor: c.anchor(), onError: c.toast }),
    },
  );
  return items;
}
