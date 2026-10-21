// The personal setup library (src/library.rs): the Library tab of the tools
// panel (tools.js) and "Save to Library…" in the setup lists' right-click
// menus (rename.js).
//
// A library is a budget file the user picks once (Rust's dialogs; the page
// never sees a path it could change). The tab lists its items by kind and
// folder with each one's definition, and copies them:
//
// - Add to This Budget…: the ticked items (or the one right-clicked or
//   pressed Enter on) with what they need, after Import Setup's review
//   (importsetup.js); what differs keeps this budget's version unless the
//   user picks otherwise. One undo step.
// - Save to Library…: this budget's setup items into the library, after the
//   same review the other way round (the library keeps its version unless
//   the user picks otherwise). Written into the library file at once.
// - Folders: new, rename, delete (items become unfiled), and Move To.
// - Setup packs (packs.js): exporting and importing packs, and the packs in
//   the user's Packs folder and those that come with the app, at the foot.
//
// When the library is also open as a budget tab, it is read from there and
// Rust refuses changes to it from here (the tab says so).
//
// The Budget Setup window draws the same library in two places
// (renderLibrarySetup, when a tab's rail is switched to Library): its rail
// gets the name, the kind, the sort and the folders as a short tree with
// their buttons; its main area the items as a table (setupframe.js) with
// the actions on a strip above and the packs below.
//
// Text from the library only reaches the page through textContent (dom.js
// el()); styling is classes (apply.css, presets.css, tools.css).

import { el } from './dom.js';
import { readPref, writePref } from './prefs.js';
import { anchoredDialog, anchoredForm } from './modal.js';
import { openContextMenu } from './menu.js';
import { showReview } from './importsetup.js';
import { packsSection } from './packs.js';
import { mountFrame, actionStrip, stripButton, searchBox, gridTable, setFooterTotals } from './setupframe.js';
import { t } from './i18n.js';

/** The kinds, in the library's order: [kind, list heading]. */
export const LIBRARY_KINDS = [
  ['global', t('library.kind.global')],
  ['fringe', t('library.kind.fringe')],
  ['group', t('library.kind.group')],
  ['unit', t('library.kind.unit')],
  ['set', t('library.kind.set')],
  ['location', t('library.kind.location')],
  ['currency', t('library.kind.currency')],
  ['charge', t('library.kind.charge')],
  ['credit', t('library.kind.credit')],
  ['tag_preset', t('library.kind.tag_preset')],
  ['line_preset', t('library.kind.line_preset')],
  ['report_preset', t('library.kind.report_preset')],
  ['ot_pattern', t('library.kind.ot_pattern')],
  ['rate_table', t('library.kind.rate_table')],
];

const KIND_IDS = LIBRARY_KINDS.map(([k]) => k);
/** The folder choice meaning "every item". */
const ALL = '';
/** The folder choice meaning "in no folder". */
const UNFILED = '\u0000unfiled';

const st = {
  view: null,
  ctx: null,
  host: null,
  lib: null,
  sig: null,
  loading: false,
  loadAgain: false,
  busy: false,
  error: '',
  kind: KIND_IDS.includes(readPref('libraryKind', 'fringe')) ? readPref('libraryKind', 'fringe') : 'fringe',
  folder: ALL,
  filter: '',
  ticked: new Set(),
  // Budget Setup draws the library in two places (renderLibrarySetup): its
  // rail and its main area. `sort` is that table's order.
  layout: 'panel',
  rail: null,
  main: null,
  sort: 'library', // 'library' | 'asc' | 'desc'
};

function str(v) {
  return typeof v === 'string' ? v : '';
}

function kindLabel(kind) {
  const k = LIBRARY_KINDS.find(([id]) => id === kind);
  return k ? k[1] : kind;
}

/** The view's kind entry for `kind`, or an empty one. */
export function kindOf(lib, kind) {
  const list = lib && Array.isArray(lib.kinds) ? lib.kinds : [];
  return list.find((k) => k.kind === kind) || { kind, folders: [], items: [] };
}

/** The items of `k` shown for `folder` (ALL, UNFILED or a name) and `filter`. */
export function shownItems(k, folder, filter) {
  const f = str(filter).trim().toLowerCase();
  return (k.items || []).filter((it) => {
    if (folder === UNFILED && it.folder) return false;
    if (folder !== ALL && folder !== UNFILED && it.folder !== folder) return false;
    if (!f) return true;
    return str(it.name).toLowerCase().includes(f) || str(it.definition).toLowerCase().includes(f);
  });
}

// ---------------------------------------------------------------------------
// Commands

function panelCall(cmd, args = {}) {
  const c = st.ctx;
  if (!c || typeof c.call !== 'function') return Promise.reject(t('library.not_connected'));
  return c.call(cmd, args);
}

function toast(message) {
  if (st.ctx && typeof st.ctx.toast === 'function') st.ctx.toast(message);
}

/** Runs one library change; the reply's library replaces the one drawn. */
async function change(cmd, args) {
  if (st.busy) return null;
  st.busy = true;
  st.error = '';
  try {
    const r = await panelCall(cmd, args);
    if (r && r.library) setLibrary(r.library);
    if (r && r.message) toast(r.message);
    return r;
  } catch (e) {
    st.error = String(e);
    return null;
  } finally {
    st.busy = false;
    redraw();
  }
}

function setLibrary(lib) {
  st.lib = lib && typeof lib === 'object' ? lib : null;
  // Ticks only for items still there.
  const names = new Set((kindOf(st.lib, st.kind).items || []).map((it) => it.name));
  for (const n of [...st.ticked]) if (!names.has(n)) st.ticked.delete(n);
  const folders = kindOf(st.lib, st.kind).folders || [];
  if (st.folder !== ALL && st.folder !== UNFILED && !folders.includes(st.folder)) st.folder = ALL;
}

async function load() {
  // Asked again while a load is on its way (the library changed meanwhile):
  // load once more when it arrives, so the newest library is the one drawn.
  if (st.loading) {
    st.loadAgain = true;
    return;
  }
  st.loading = true;
  try {
    do {
      st.loadAgain = false;
      try {
        setLibrary(await panelCall('library_view'));
      } catch (e) {
        st.error = String(e);
      }
    } while (st.loadAgain);
  } finally {
    st.loading = false;
    redraw();
  }
}

/** What makes the drawn library stale: another budget in front, tabs opened or closed, or edits in the library's own tab. */
function signature(view) {
  const tabs = Array.isArray(view.tabs) ? view.tabs.map((tab) => `${tab.id}:${tab.name}`).join('|') : '';
  const rev = st.lib && st.lib.in_tab ? view.rev : '';
  return `${view.doc}#${tabs}#${rev}`;
}

/** The review's words when adding from the library to this budget. */
const ADD_TEXT = {
  title: () => t('library.add.title'),
  intro: t('library.add.intro'),
  okText: t('library.add.ok'),
  keepText: t('library.add.keep'),
  takeText: t('library.add.take'),
  renameText: (to) => t('library.add.rename', { to }),
  choiceTitle: t('library.add.choice_tip'),
  hereLabel: t('library.add.here'),
  otherLabel: t('library.add.other'),
  status: { new: t('library.add.status_new'), same: t('library.add.status_same'), differs: t('library.add.status_differs') },
  emptyText: t('library.add.empty'),
  picked: (count) => t('library.add.picked', { count }),
  pickedAlong: (count, along) => t('library.add.picked_along', { count, along }),
  tickPrompt: t('library.add.tick_prompt'),
  tickAll: (heading) => t('library.add.tick_all', { heading }),
  tickOne: (what) => t('library.add.tick_one', { what }),
};

/** The review's words when saving this budget's items to the library. */
const SAVE_TEXT = {
  title: (p) => t('library.save.title', { source: p.source }),
  intro: t('library.save.intro'),
  okText: t('library.save.ok'),
  keepText: t('library.save.keep'),
  takeText: t('library.save.take'),
  renameText: (to) => t('library.save.rename', { to }),
  choiceTitle: t('library.save.choice_tip'),
  hereLabel: t('library.save.here'),
  otherLabel: t('library.save.other'),
  status: { new: t('library.save.status_new'), same: t('library.save.status_same'), differs: t('library.save.status_differs') },
  emptyText: t('library.save.empty'),
  picked: (count) => t('library.save.picked', { count }),
  pickedAlong: (count, along) => t('library.save.picked_along', { count, along }),
  tickPrompt: t('library.save.tick_prompt'),
  tickAll: (heading) => t('library.save.tick_all', { heading }),
  tickOne: (what) => t('library.save.tick_one', { what }),
};

function reviewCtx(call, toastFn) {
  return {
    invoke: call,
    apply: () => {},
    toast: toastFn,
    focus: () => {},
  };
}

/** Add to This Budget… for `names` of `kind`. */
async function addToBudget(kind, names) {
  if (!names.length || st.busy) return;
  let p;
  try {
    p = await panelCall('library_add_preview', { picks: names.map((name) => ({ kind, name })) });
  } catch (e) {
    toast(e);
    return;
  }
  showReview(reviewCtx(panelCall, toast), p, {
    ...ADD_TEXT,
    picked: p.picked,
    run: async (req) => {
      const r = await panelCall('library_add', { token: p.token, picks: req.picks, resolutions: req.resolutions });
      // The budget changed: draw its new view.
      await panelCall('get_view', {});
      return { message: r && r.message };
    },
  });
}

/**
 * Save to Library…: `picks` ({ kind, name } items and what they need), or
 * none for every setup item in the budget. `call` and `toast` are the
 * caller's (a setup list's right-click menu, or this tab).
 */
async function saveToLibrary(call, toastFn, picks) {
  let p;
  try {
    p = await call('library_save_preview', { picks });
  } catch (e) {
    toastFn(e);
    return;
  }
  showReview(reviewCtx(call, toastFn), p, {
    ...SAVE_TEXT,
    picked: p.picked,
    run: async (req) => {
      const r = await call('library_save', { token: p.token, picks: req.picks, resolutions: req.resolutions });
      if (r && r.library) {
        setLibrary(r.library);
        redraw();
      }
      return { message: r && r.message };
    },
  });
}

/**
 * "Save to Library…" in a setup list's right-click menu (rename.js): item
 * `name` of `kind` and what it needs. `ctx`: { call(cmd, args), toast }.
 */
export function saveItemToLibrary(ctx, kind, name) {
  const call = (cmd, args) => ctx.call(cmd, args);
  const say = (m) => typeof ctx.toast === 'function' && ctx.toast(m);
  return saveToLibrary(call, say, [{ kind, name }]);
}

// ---------------------------------------------------------------------------
// Drawing

/** Draws the Library tab into `host` (tools.js, on every view). */
export function renderLibrary(host, view, ctx) {
  st.view = view && typeof view === 'object' ? view : {};
  st.ctx = ctx || {};
  st.layout = 'panel';
  st.host = host;
  const sig = signature(st.view);
  if (st.lib === null || sig !== st.sig) {
    st.sig = sig;
    load();
  }
  draw(host);
}

/**
 * Draws the library as Budget Setup shows it (setupwin.js, on every view,
 * while a tab's rail is switched to Library): into `rail` the library's
 * name, the kind, the sort and the folders with their buttons; into `main`
 * the items of the kind and folder as a table (setupframe.js), with Add to
 * This Budget…, Move To… and Save to Library… above it and the setup packs
 * under it.
 */
export function renderLibrarySetup(rail, main, view, ctx) {
  st.view = view && typeof view === 'object' ? view : {};
  st.ctx = ctx || {};
  st.layout = 'setup';
  st.rail = rail;
  st.main = main;
  st.host = null;
  const sig = signature(st.view);
  if (st.lib === null || sig !== st.sig) {
    st.sig = sig;
    load();
  }
  drawSetup();
}

/**
 * Shows kind `kind` (one of LIBRARY_KINDS) from now on, at every folder: the
 * Budget Setup window's library beside a table (setupwin.js). Not saved as
 * the Library tab's choice.
 */
export function setLibraryKind(kind) {
  if (!KIND_IDS.includes(kind) || kind === st.kind) return;
  st.kind = kind;
  st.folder = ALL;
  st.ticked = new Set();
}

function redraw() {
  if (st.layout === 'setup') drawSetup();
  else if (st.host && st.host.isConnected) draw(st.host);
}

function button(text, key, onClick, { title = '', label = '', enabled = true, primary = false } = {}) {
  const b = el('button', {
    class: `setup-btn apply-btn${primary ? ' setup-btn-primary' : ''}`,
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

function byKey(key) {
  for (const h of [st.host, st.rail, st.main]) {
    if (!h) continue;
    const found = [...h.querySelectorAll('[data-key]')].find((e) => e.dataset.key === key);
    if (found) return found;
  }
  return null;
}

function pickFile(cmd) {
  return change(cmd, {});
}

function noLibrary(parts) {
  parts.push(
    el('p', {
      class: 'setup-empty library-intro',
      text: t('library.intro'),
    }),
    el(
      'div',
      { class: 'library-actions' },
      button(t('library.create'), 'library:create', () => pickFile('library_create'), {
        primary: true,
        title: t('library.create_tip'),
      }),
      button(t('library.use_existing'), 'library:choose', () => pickFile('library_choose'), {
        title: t('library.use_existing_tip'),
      }),
    ),
  );
}

function header(lib) {
  return el(
    'div',
    { class: 'library-head' },
    el('span', { class: 'library-title', text: str(lib.name), title: str(lib.location) }),
    button(t('library.change'), 'library:choose', () => pickFile('library_choose'), { title: t('library.change_tip') }),
    button(t('library.new'), 'library:create', () => pickFile('library_create'), { title: t('library.new_tip') }),
  );
}

function kindSelect(lib) {
  const sel = el('select', {
    class: 'setup-input setup-select library-kind',
    attrs: { 'aria-label': t('library.kind_of_item') },
    data: { key: 'library:kind' },
  });
  for (const [id, label] of LIBRARY_KINDS) {
    const n = (kindOf(lib, id).items || []).length;
    sel.append(el('option', { text: n ? `${label} (${n})` : label, attrs: { value: id } }));
  }
  sel.value = st.kind;
  sel.addEventListener('change', () => {
    st.kind = sel.value;
    writePref('libraryKind', st.kind);
    st.folder = ALL;
    st.ticked.clear();
    redraw();
  });
  return sel;
}

function folderSelect(k) {
  const sel = el('select', {
    class: 'setup-input setup-select library-folder-pick',
    attrs: { 'aria-label': t('library.folder') },
    data: { key: 'library:folder' },
  });
  sel.append(el('option', { text: t('library.all_folders'), attrs: { value: ALL } }));
  sel.append(el('option', { text: t('library.unfiled'), attrs: { value: UNFILED } }));
  for (const f of k.folders || []) sel.append(el('option', { text: f, attrs: { value: f } }));
  sel.value = st.folder;
  sel.addEventListener('change', () => {
    st.folder = sel.value;
    redraw();
  });
  return sel;
}

function filterBox() {
  const input = el('input', {
    class: 'setup-input apply-filter',
    attrs: { type: 'text', placeholder: t('library.filter'), 'aria-label': t('library.filter_label'), autocomplete: 'off', spellcheck: 'false', autocapitalize: 'off', autocorrect: 'off' },
    data: { key: 'library:filter' },
  });
  input.value = st.filter;
  input.addEventListener('input', (e) => {
    st.filter = input.value;
    if (!e.isComposing) redraw();
  });
  input.addEventListener('keydown', (e) => {
    if (e.key === 'Escape' && input.value) {
      e.preventDefault();
      e.stopPropagation();
      st.filter = '';
      redraw();
    }
  });
  return input;
}

/** The Move To menu: Unfiled and each folder, at `anchor` or a point. */
function moveMenu(at, k, names, anchor) {
  const items = [{ label: t('library.unfiled'), run: () => change('library_move', { kind: k.kind, names, folder: null }) }];
  for (const f of k.folders || []) items.push({ label: f, run: () => change('library_move', { kind: k.kind, names, folder: f }) });
  openContextMenu(at, { label: t('library.move_to_folder'), items, returnFocus: () => anchor && anchor.focus && anchor.focus() });
}

function pointBelow(b) {
  const r = b.getBoundingClientRect();
  return { x: r.left, y: r.bottom + 2 };
}

/**
 * One library item: its tick box and its name (a button: Enter or a
 * double-click adds it, Space ticks it, Up and Down move along). `grid`:
 * Budget Setup's table row, with the definition and the folder in cells of
 * their own.
 */
function itemRow(k, it, writable, grid = false) {
  const key = `library:item:${k.kind}:${it.name}`;
  const box = el('input', {
    class: 'library-tick',
    attrs: { type: 'checkbox', 'aria-label': t('library.pick_item', { name: it.name }) },
    data: { key: `library:tick:${k.kind}:${it.name}` },
  });
  box.checked = st.ticked.has(it.name);
  box.addEventListener('change', () => {
    if (box.checked) st.ticked.add(it.name);
    else st.ticked.delete(it.name);
    redraw();
  });
  const needs = (it.needs || []).map((n) => n.name).filter(Boolean);
  const tip = [str(it.definition), needs.length ? t('library.brings_along', { names: needs.join(', ') }) : '', t('library.item_tip')].filter(Boolean).join('\n');
  const name = el(
    'button',
    {
      class: 'preset-name library-name',
      title: tip,
      attrs: { type: 'button', 'aria-keyshortcuts': 'Enter' },
      data: { key, nobusy: '1' },
    },
    el('span', { class: 'apply-name', text: str(it.name) }),
    grid ? null : el('span', { class: 'apply-desc', text: str(it.definition) }),
  );
  const add = () => addToBudget(k.kind, [it.name]);
  name.addEventListener('dblclick', (e) => {
    e.preventDefault();
    add();
  });
  name.addEventListener('keydown', (e) => {
    const plain = !e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey;
    if (e.key === 'Enter' && plain) {
      e.preventDefault();
      add();
    } else if (e.key === ' ' && plain) {
      e.preventDefault();
      box.click();
    } else if ((e.key === 'ArrowDown' || e.key === 'ArrowUp') && plain) {
      const list = name.closest('ul');
      const all = list ? [...list.querySelectorAll('.library-name')] : [];
      const i = all.indexOf(name);
      const to = all[i + (e.key === 'ArrowDown' ? 1 : -1)];
      if (to) {
        e.preventDefault();
        to.focus();
      }
    }
  });
  name.addEventListener('contextmenu', (e) => {
    e.preventDefault();
    const names = st.ticked.has(it.name) && st.ticked.size > 1 ? [...st.ticked] : [it.name];
    const what = names.length === 1 ? it.name : t('library.n_items', { count: names.length });
    const items = [{ label: names.length === 1 ? t('library.add_to_budget') : t('library.add_what_to_budget', { what }), run: () => addToBudget(k.kind, names) }];
    if (writable && (k.folders || []).length) {
      const sub = [{ label: t('library.unfiled'), run: () => change('library_move', { kind: k.kind, names, folder: null }) }];
      for (const f of k.folders) sub.push({ label: f, run: () => change('library_move', { kind: k.kind, names, folder: f }) });
      items.push({ label: t('library.move_to_menu'), items: sub });
    }
    openContextMenu({ x: e.clientX, y: e.clientY }, { label: what, items, returnFocus: () => name.focus() });
  });
  const parts = [el('label', { class: 'imp-tick library-tickbox' }, box), name];
  if (grid) {
    parts.push(
      el('span', { class: 'sw-cell sw-cell-text library-def', text: str(it.definition), title: str(it.definition) || undefined }),
      el('span', { class: 'sw-cell sw-cell-text library-in', text: str(it.folder), title: it.folder ? t('library.its_folder') : undefined }),
    );
    const row = el('li', { class: 'library-item sw-grid-row' }, ...parts);
    if (st.ticked.has(it.name)) row.classList.add('is-selected');
    return row;
  }
  if (st.folder === ALL && it.folder) parts.push(el('span', { class: 'library-in', text: str(it.folder), title: t('library.its_folder') }));
  return el('li', { class: 'library-item' }, ...parts);
}

function folderButtons(k, writable) {
  const real = st.folder !== ALL && st.folder !== UNFILED;
  const label = kindLabel(k.kind).toLowerCase();
  return el(
    'div',
    { class: 'library-actions' },
    button(
      t('library.new_folder'),
      'library:folder:new',
      (b) =>
        anchoredDialog({
          anchor: b,
          title: t('library.new_folder_title', { kinds: label }),
          label: t('library.name'),
          submitText: t('library.make_folder'),
          onSubmit: async (v) => {
            const r = await panelCall('library_folder', { kind: k.kind, action: 'create', name: v, to: null });
            if (r && r.library) {
              setLibrary(r.library);
              st.folder = (kindOf(st.lib, k.kind).folders || []).find((f) => f.toLowerCase() === v.trim().toLowerCase()) || ALL;
            }
            redraw();
          },
        }),
      { enabled: writable, title: t('library.new_folder_tip', { kinds: label }) },
    ),
    button(
      t('library.rename_folder'),
      'library:folder:rename',
      (b) =>
        anchoredDialog({
          anchor: b,
          title: t('library.rename_folder_title', { name: st.folder }),
          label: t('library.name'),
          value: st.folder,
          submitText: t('library.rename'),
          onSubmit: async (v) => {
            const r = await panelCall('library_folder', { kind: k.kind, action: 'rename', name: st.folder, to: v });
            if (r && r.library) {
              setLibrary(r.library);
              st.folder = (kindOf(st.lib, k.kind).folders || []).find((f) => f.toLowerCase() === v.trim().toLowerCase()) || ALL;
            }
            redraw();
          },
        }),
      { enabled: writable && real, title: real ? t('library.rename_folder_tip') : t('library.show_a_folder_first') },
    ),
    button(
      t('library.delete_folder'),
      'library:folder:delete',
      (b) => {
        const name = st.folder;
        anchoredForm({
          anchor: b,
          title: t('library.delete_folder_title', { name }),
          message: t('library.delete_folder_message'),
          submitText: t('library.delete_folder_ok'),
          danger: true,
          onSubmit: async () => {
            st.folder = ALL;
            await change('library_folder', { kind: k.kind, action: 'delete', name, to: null });
          },
        });
      },
      { enabled: writable && real, title: real ? t('library.delete_folder_tip') : t('library.show_a_folder_first') },
    ),
  );
}

function draw(host) {
  const lib = st.lib;
  const parts = [];
  if (!lib) {
    parts.push(el('p', { class: 'setup-empty', text: st.loading ? t('library.reading') : t('library.could_not_read') }));
  } else if (!lib.configured) {
    noLibrary(parts);
  } else {
    parts.push(header(lib));
    if (lib.problem) {
      parts.push(el('p', { class: 'setup-error apply-error', text: lib.problem, attrs: { role: 'alert' } }));
    } else {
      const writable = !lib.in_tab;
      if (lib.in_tab) {
        parts.push(el('p', { class: 'setup-empty', text: t('library.open_as_tab') }));
      }
      const k = kindOf(lib, st.kind);
      parts.push(el('div', { class: 'library-picks' }, kindSelect(lib), folderSelect(k)));
      parts.push(el('div', { class: 'apply-tools' }, filterBox()));
      const shown = shownItems(k, st.folder, st.filter);
      const list = el('ul', { class: 'apply-items library-list', attrs: { 'aria-label': t('library.kinds_in_library', { kinds: kindLabel(k.kind) }) } });
      for (const it of shown) list.append(itemRow(k, it, writable));
      if (!(k.items || []).length) parts.push(el('p', { class: 'setup-empty', text: t('library.none_yet', { kinds: kindLabel(k.kind).toLowerCase() }) }));
      else if (!shown.length) parts.push(el('p', { class: 'setup-empty', text: t('library.nothing_matches') }));
      parts.push(list);
      const ticked = [...st.ticked];
      parts.push(
        el(
          'div',
          { class: 'library-actions' },
          button(ticked.length > 1 ? t('library.add_n_to_budget', { count: ticked.length }) : t('library.add_to_budget'), 'library:add', () => addToBudget(k.kind, ticked), {
            enabled: ticked.length > 0,
            title: ticked.length ? t('library.add_tip') : t('library.tick_first'),
          }),
          button(t('library.move_to'), 'library:move', (b) => moveMenu(pointBelow(b), k, ticked, b), {
            enabled: writable && ticked.length > 0 && (k.folders || []).length > 0,
            title: (k.folders || []).length ? t('library.move_tip') : t('library.make_a_folder_first'),
          }),
        ),
      );
      parts.push(folderButtons(k, writable));
      parts.push(
        el(
          'div',
          { class: 'library-actions library-save' },
          button(t('library.save_to_library'), 'library:save', () => saveToLibrary(panelCall, toast, null), {
            enabled: writable,
            title: writable ? t('library.save_tip') : t('library.open_as_tab_tip'),
          }),
        ),
      );
    }
  }
  if (st.error) parts.push(el('p', { class: 'setup-error apply-error', text: st.error, attrs: { role: 'alert' } }));
  if (lib) parts.push(packsPart(lib));
  const box = el('div', { class: 'apply library', attrs: { 'aria-busy': st.busy || st.loading ? 'true' : 'false' } }, ...parts);
  const a = document.activeElement;
  const had = a && host.contains(a) && a.dataset ? a.dataset.key : null;
  host.replaceChildren(box);
  if (had) {
    const again = byKey(had);
    if (again) again.focus({ preventScroll: true });
  }
}

/** The setup packs section (packs.js), for the library `lib`. */
function packsPart(lib) {
  const usable = Boolean(lib.configured && !lib.problem);
  return packsSection(
    {
      call: panelCall,
      toast,
      focus: () => {},
      redraw,
      // The budget changed: draw its new view.
      apply: () => panelCall('get_view', {}).catch(() => {}),
      libraryChanged: (l) => {
        setLibrary(l);
        redraw();
      },
    },
    { hasLibrary: usable, libraryWritable: usable && !lib.in_tab },
  );
}

// ---------------------------------------------------------------------------
// Budget Setup's library: the rail and the main area (renderLibrarySetup)

const FOLDER_KEYS = { [ALL]: 'all', [UNFILED]: 'unfiled' };

function folderKey(f) {
  return `library:folder-pick:${FOLDER_KEYS[f] || `f:${f}`}`;
}

/** The folders of kind `k` as a short tree: All, Unfiled, then each folder, with counts. */
function folderTree(k) {
  const items = k.items || [];
  const count = (f) => (f === ALL ? items.length : f === UNFILED ? items.filter((i) => !i.folder).length : items.filter((i) => i.folder === f).length);
  const ul = el('ul', { class: 'sw-tree', attrs: { 'aria-label': t('library.folders') } });
  const picks = [
    [ALL, t('library.all_folders')],
    [UNFILED, t('library.unfiled')],
    ...(k.folders || []).map((f) => [f, f]),
  ];
  for (const [f, label] of picks) {
    const real = f !== ALL && f !== UNFILED;
    const b = el(
      'button',
      {
        class: `sw-tree-item${real ? ' is-folder' : ''}`,
        title: real ? t('library.show_folder', { name: f }) : f === ALL ? t('library.show_every_item') : t('library.show_unfiled'),
        attrs: { type: 'button', 'aria-pressed': String(st.folder === f) },
        data: { key: folderKey(f), nobusy: '1' },
      },
      el('span', { class: 'sw-tree-name', text: label }),
      el('span', { class: 'sw-tree-count', text: String(count(f)), attrs: { 'aria-label': t('library.n_items', { count: count(f) }) } }),
    );
    b.addEventListener('click', () => {
      st.folder = f;
      redraw();
    });
    b.addEventListener('keydown', (e) => {
      if (e.altKey || e.metaKey || e.ctrlKey || e.shiftKey) return;
      if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
      const all = [...ul.querySelectorAll('.sw-tree-item')];
      const to = all[all.indexOf(b) + (e.key === 'ArrowDown' ? 1 : -1)];
      if (to) {
        e.preventDefault();
        to.focus();
      }
    });
    ul.append(el('li', {}, b));
  }
  return ul;
}

/** The library table's order: the library's own, or by name. */
function sortPick() {
  const sel = el('select', {
    class: 'setup-input setup-select setup-sortpick',
    attrs: { 'aria-label': t('library.sort') },
    data: { key: 'library:sort', nobusy: '1' },
  });
  for (const [v, text] of [
    ['library', t('library.sort_library')],
    ['asc', t('library.sort_asc')],
    ['desc', t('library.sort_desc')],
  ]) {
    sel.append(el('option', { text, attrs: { value: v } }));
  }
  sel.value = st.sort;
  sel.addEventListener('change', () => {
    st.sort = sel.value;
    redraw();
  });
  return sel;
}

function railLabel(text) {
  return el('span', { class: 'sw-rail-label', text, attrs: { 'aria-hidden': 'true' } });
}

/** The rail: the library's name, its kind, the sort, then the folders. */
function railParts() {
  const lib = st.lib;
  if (!lib) return [el('p', { class: 'setup-empty', text: st.loading ? t('library.reading') : t('library.could_not_read') })];
  if (!lib.configured) return [el('p', { class: 'setup-empty', text: t('library.no_library_yet') })];
  const parts = [header(lib)];
  if (lib.problem) return parts;
  const k = kindOf(lib, st.kind);
  const writable = !lib.in_tab;
  parts.push(
    el('div', { class: 'sw-rail-field' }, railLabel(t('library.kind_label')), kindSelect(lib)),
    el('div', { class: 'sw-rail-field' }, railLabel(t('library.sort')), sortPick()),
    el('div', { class: 'sw-rail-field' }, railLabel(t('library.folders')), folderTree(k)),
    folderButtons(k, writable),
  );
  return parts;
}

/** The main area: the strip, the items as a table, then the packs. */
function mainFrame(main) {
  const lib = st.lib;
  const usable = Boolean(lib && lib.configured && !lib.problem);
  const k = kindOf(lib, st.kind);
  const writable = usable && !lib.in_tab;
  const ticked = [...st.ticked];
  const folders = k.folders || [];
  const strip = actionStrip(
    t('library.strip'),
    [
      stripButton(ticked.length > 1 ? t('library.add_n_to_budget', { count: ticked.length }) : t('library.add_to_budget'), 'library:add', () => addToBudget(k.kind, ticked), {
        enabled: usable && ticked.length > 0,
        title: ticked.length ? t('library.add_tip') : t('library.tick_first'),
      }),
      stripButton(t('library.move_to'), 'library:move', (b) => moveMenu(pointBelow(b), k, ticked, b), {
        enabled: writable && ticked.length > 0 && folders.length > 0,
        title: folders.length ? t('library.move_tip') : t('library.make_a_folder_first'),
      }),
      stripButton(t('library.save_to_library'), 'library:save', () => saveToLibrary(panelCall, toast, null), {
        enabled: writable,
        title: writable ? t('library.save_tip') : t('library.open_as_tab_tip'),
      }),
    ],
    usable
      ? searchBox({
          key: 'library:filter',
          value: st.filter,
          label: t('library.search'),
          placeholder: t('library.search_placeholder', { kinds: kindLabel(k.kind).toLowerCase() }),
          onChange: (v) => {
            st.filter = v;
            redraw();
          },
        })
      : null,
  );
  const body = [];
  let shownCount = 0;
  if (!lib) {
    body.push(el('p', { class: 'setup-empty', text: st.loading ? t('library.reading') : t('library.could_not_read') }));
  } else if (!lib.configured) {
    noLibrary(body);
  } else if (lib.problem) {
    body.push(el('p', { class: 'setup-error apply-error', text: lib.problem, attrs: { role: 'alert' } }));
  } else {
    if (lib.in_tab) {
      body.push(el('p', { class: 'setup-empty', text: t('library.open_as_tab') }));
    }
    let shown = shownItems(k, st.folder, st.filter);
    if (st.sort !== 'library') {
      shown = [...shown].sort((a, b) => {
        const x = str(a.name).toLowerCase();
        const y = str(b.name).toLowerCase();
        return x < y ? -1 : x > y ? 1 : 0;
      });
      if (st.sort === 'desc') shown.reverse();
    }
    shownCount = shown.length;
    const list = el('ul', { class: 'apply-items library-list', attrs: { 'aria-label': t('library.kinds_in_library', { kinds: kindLabel(k.kind) }) } });
    for (const it of shown) list.append(itemRow(k, it, writable, true));
    if (!(k.items || []).length) body.push(el('p', { class: 'setup-empty', text: t('library.none_yet', { kinds: kindLabel(k.kind).toLowerCase() }) }));
    else if (!shown.length) body.push(el('p', { class: 'setup-empty', text: t('library.nothing_matches') }));
    if (shown.length) {
      body.push(
        gridTable(
          'library',
          [
            el('span', { class: 'sw-head' }),
            el('span', { class: 'sw-head', text: t('library.name') }),
            el('span', { class: 'sw-head', text: t('library.definition') }),
            el('span', { class: 'sw-head', text: t('library.folder') }),
          ],
          list,
        ),
      );
    }
  }
  if (st.error) body.push(el('p', { class: 'setup-error apply-error', text: st.error, attrs: { role: 'alert' } }));
  const after = lib ? [el('div', { class: 'sw-after' }, packsPart(lib))] : [];
  mountFrame(main, { strip, body, after });
  main.setAttribute('aria-busy', st.busy || st.loading ? 'true' : 'false');
  const items = (k.items || []).length;
  setFooterTotals(
    usable
      ? [
          { label: '', value: shownCount === items ? t('library.footer_items', { count: items }) : t('library.footer_shown_of', { shown: shownCount, total: items }) },
          ticked.length ? { label: '', value: t('library.footer_ticked', { count: ticked.length }) } : null,
        ]
      : [],
  );
}

/** Draws the rail and the main area, keeping focus (and a search box's caret) where it was. */
function paintSetup() {
  const { rail, main } = st;
  if (!rail || !main) return;
  const a = document.activeElement;
  const inside = a && (rail.contains(a) || main.contains(a));
  const had = inside && a.dataset ? a.dataset.key : null;
  const caret = inside && typeof a.selectionStart === 'number' ? [a.selectionStart, a.selectionEnd] : null;
  rail.replaceChildren(...railParts());
  main.replaceChildren();
  mainFrame(main);
  if (had) {
    const again = byKey(had);
    if (again) {
      again.focus({ preventScroll: true });
      if (caret && typeof again.setSelectionRange === 'function') {
        try {
          again.setSelectionRange(caret[0], caret[1]);
        } catch {
          // Not a text control.
        }
      }
    }
  }
}

/** Budget Setup: draws now (a new view), or again while still on screen (the library changed). */
function drawSetup() {
  if (!st.main) return;
  // A redraw for hosts an older render left behind does nothing.
  if (st.main.isConnected || !st.main.childNodes.length) paintSetup();
}
