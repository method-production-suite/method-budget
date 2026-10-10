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
//
// When the library is also open as a budget tab, it is read from there and
// Rust refuses changes to it from here (the tab says so).
//
// Text from the library only reaches the page through textContent (dom.js
// el()); styling is classes (apply.css, presets.css, tools.css).

import { el } from './dom.js';
import { readPref, writePref } from './prefs.js';
import { anchoredDialog, anchoredForm } from './modal.js';
import { openContextMenu } from './menu.js';
import { showReview } from './importsetup.js';

/** The kinds, in the library's order: [kind, list heading]. */
export const LIBRARY_KINDS = [
  ['global', 'Globals'],
  ['fringe', 'Fringes'],
  ['group', 'Groups'],
  ['unit', 'Units'],
  ['set', 'Sets'],
  ['location', 'Locations'],
  ['currency', 'Currencies'],
  ['charge', 'Charges'],
  ['credit', 'Credits'],
  ['tag_preset', 'Tag presets'],
  ['line_preset', 'Line blocks'],
  ['report_preset', 'Report presets'],
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
  if (!c || typeof c.call !== 'function') return Promise.reject('Not connected');
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
  const tabs = Array.isArray(view.tabs) ? view.tabs.map((t) => `${t.id}:${t.name}`).join('|') : '';
  const rev = st.lib && st.lib.in_tab ? view.rev : '';
  return `${view.doc}#${tabs}#${rev}`;
}

/** The review's words when adding from the library to this budget. */
const ADD_TEXT = {
  title: () => 'Add from your library',
  intro:
    'Tick what to copy into this budget. What an item needs comes with it. Where this budget has an item of the same name set up differently, it keeps its own version unless you choose otherwise. The copies are not linked to the library.',
  okText: 'Add to Budget',
  keepText: "Keep this budget's version",
  takeText: "Use the library's version",
  renameText: (to) => `Add the library's as ${to}`,
  choiceTitle: "The library's version replaces this budget's for everything here that uses it; a new name leaves this budget's as it is.",
  hereLabel: 'Here',
  otherLabel: 'Library',
  status: { new: 'New', same: 'Same here', differs: 'Different here' },
  emptyText: 'Nothing to add.',
  doing: 'to add',
  tickAll: (heading) => `Add every item in ${heading}`,
  tickOne: (what) => `Add ${what}`,
};

/** The review's words when saving this budget's items to the library. */
const SAVE_TEXT = {
  title: (p) => `Save to your library from ${p.source}`,
  intro:
    'Tick what to copy into your library. What an item needs comes with it. Where the library has an item of the same name set up differently, it keeps its own version unless you choose otherwise. New items go in no folder; the copies are not linked to this budget.',
  okText: 'Save to Library',
  keepText: "Keep the library's version",
  takeText: "Replace it with this budget's",
  renameText: (to) => `Add this budget's as ${to}`,
  choiceTitle: "This budget's version replaces the library's; a new name leaves the library's as it is.",
  hereLabel: 'Library',
  otherLabel: 'This budget',
  status: { new: 'New to the library', same: 'Same in the library', differs: 'Different in the library' },
  emptyText: 'This budget has no setup items to save.',
  doing: 'to save',
  tickAll: (heading) => `Save every item in ${heading}`,
  tickOne: (what) => `Save ${what}`,
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
  const t = (m) => typeof ctx.toast === 'function' && ctx.toast(m);
  return saveToLibrary(call, t, [{ kind, name }]);
}

// ---------------------------------------------------------------------------
// Drawing

/** Draws the Library tab into `host` (tools.js, on every view). */
export function renderLibrary(host, view, ctx) {
  st.view = view && typeof view === 'object' ? view : {};
  st.ctx = ctx || {};
  st.host = host;
  const sig = signature(st.view);
  if (st.lib === null || sig !== st.sig) {
    st.sig = sig;
    load();
  }
  draw(host);
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
  if (st.host && st.host.isConnected) draw(st.host);
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
  if (!st.host) return null;
  return [...st.host.querySelectorAll('[data-key]')].find((e) => e.dataset.key === key) || null;
}

function pickFile(cmd) {
  return change(cmd, {});
}

function noLibrary(parts) {
  parts.push(
    el('p', {
      class: 'setup-empty library-intro',
      text: 'Keep the fringes, globals, groups and other setup you reuse in a library file of your own, then copy them into any budget. The library starts empty and stays on this computer, in a folder you choose.',
    }),
    el(
      'div',
      { class: 'library-actions' },
      button('Create Library…', 'library:create', () => pickFile('library_create'), {
        primary: true,
        title: `Choose a folder: the library is made there as “My Method Budget Library.mthd”`,
      }),
      button('Use Existing Library…', 'library:choose', () => pickFile('library_choose'), {
        title: 'Choose a library (any Method Budget file)',
      }),
    ),
  );
}

function header(lib) {
  return el(
    'div',
    { class: 'library-head' },
    el('span', { class: 'library-title', text: str(lib.name), title: str(lib.location) }),
    button('Change…', 'library:choose', () => pickFile('library_choose'), { title: 'Use another library file' }),
    button('New…', 'library:create', () => pickFile('library_create'), { title: 'Make a new library in a folder you choose' }),
  );
}

function kindSelect(lib) {
  const sel = el('select', {
    class: 'setup-input setup-select library-kind',
    attrs: { 'aria-label': 'Kind of setup item' },
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
    attrs: { 'aria-label': 'Folder' },
    data: { key: 'library:folder' },
  });
  sel.append(el('option', { text: 'All folders', attrs: { value: ALL } }));
  sel.append(el('option', { text: 'Unfiled', attrs: { value: UNFILED } }));
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
    attrs: { type: 'text', placeholder: 'Filter', 'aria-label': 'Filter the library', autocomplete: 'off', spellcheck: 'false', autocapitalize: 'off', autocorrect: 'off' },
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
  const items = [{ label: 'Unfiled', run: () => change('library_move', { kind: k.kind, names, folder: null }) }];
  for (const f of k.folders || []) items.push({ label: f, run: () => change('library_move', { kind: k.kind, names, folder: f }) });
  openContextMenu(at, { label: 'Move to folder', items, returnFocus: () => anchor && anchor.focus && anchor.focus() });
}

function pointBelow(b) {
  const r = b.getBoundingClientRect();
  return { x: r.left, y: r.bottom + 2 };
}

function itemRow(k, it, writable) {
  const key = `library:item:${k.kind}:${it.name}`;
  const box = el('input', {
    class: 'library-tick',
    attrs: { type: 'checkbox', 'aria-label': `Pick ${it.name}` },
    data: { key: `library:tick:${k.kind}:${it.name}` },
  });
  box.checked = st.ticked.has(it.name);
  box.addEventListener('change', () => {
    if (box.checked) st.ticked.add(it.name);
    else st.ticked.delete(it.name);
    redraw();
  });
  const needs = (it.needs || []).map((n) => n.name).filter(Boolean);
  const tip = [str(it.definition), needs.length ? `Brings along: ${needs.join(', ')}` : '', 'Enter or a double-click adds it to this budget'].filter(Boolean).join('\n');
  const name = el(
    'button',
    {
      class: 'preset-name library-name',
      title: tip,
      attrs: { type: 'button', 'aria-keyshortcuts': 'Enter' },
      data: { key, nobusy: '1' },
    },
    el('span', { class: 'apply-name', text: str(it.name) }),
    el('span', { class: 'apply-desc', text: str(it.definition) }),
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
      const all = [...st.host.querySelectorAll('.library-name')];
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
    const what = names.length === 1 ? it.name : `${names.length} items`;
    const items = [{ label: names.length === 1 ? 'Add to This Budget…' : `Add ${what} to This Budget…`, run: () => addToBudget(k.kind, names) }];
    if (writable && (k.folders || []).length) {
      const sub = [{ label: 'Unfiled', run: () => change('library_move', { kind: k.kind, names, folder: null }) }];
      for (const f of k.folders) sub.push({ label: f, run: () => change('library_move', { kind: k.kind, names, folder: f }) });
      items.push({ label: 'Move To', items: sub });
    }
    openContextMenu({ x: e.clientX, y: e.clientY }, { label: what, items, returnFocus: () => name.focus() });
  });
  const parts = [el('label', { class: 'imp-tick library-tickbox' }, box), name];
  if (st.folder === ALL && it.folder) parts.push(el('span', { class: 'library-in', text: str(it.folder), title: 'Its folder' }));
  return el('li', { class: 'library-item' }, ...parts);
}

function folderButtons(k, writable) {
  const real = st.folder !== ALL && st.folder !== UNFILED;
  const label = kindLabel(k.kind).toLowerCase();
  return el(
    'div',
    { class: 'library-actions' },
    button(
      'New Folder…',
      'library:folder:new',
      (b) =>
        anchoredDialog({
          anchor: b,
          title: `New folder of ${label}`,
          label: 'Name',
          submitText: 'Make Folder',
          onSubmit: async (v) => {
            const r = await panelCall('library_folder', { kind: k.kind, action: 'create', name: v, to: null });
            if (r && r.library) {
              setLibrary(r.library);
              st.folder = (kindOf(st.lib, k.kind).folders || []).find((f) => f.toLowerCase() === v.trim().toLowerCase()) || ALL;
            }
            redraw();
          },
        }),
      { enabled: writable, title: `A folder to sort the library’s ${label} into` },
    ),
    button(
      'Rename Folder…',
      'library:folder:rename',
      (b) =>
        anchoredDialog({
          anchor: b,
          title: `Rename the folder ${st.folder}`,
          label: 'Name',
          value: st.folder,
          submitText: 'Rename',
          onSubmit: async (v) => {
            const r = await panelCall('library_folder', { kind: k.kind, action: 'rename', name: st.folder, to: v });
            if (r && r.library) {
              setLibrary(r.library);
              st.folder = (kindOf(st.lib, k.kind).folders || []).find((f) => f.toLowerCase() === v.trim().toLowerCase()) || ALL;
            }
            redraw();
          },
        }),
      { enabled: writable && real, title: real ? 'Rename the folder shown' : 'Show a folder first' },
    ),
    button(
      'Delete Folder…',
      'library:folder:delete',
      (b) => {
        const name = st.folder;
        anchoredForm({
          anchor: b,
          title: `Delete the folder ${name}?`,
          message: 'Its items stay in the library, in no folder.',
          submitText: 'Delete Folder',
          danger: true,
          onSubmit: async () => {
            st.folder = ALL;
            await change('library_folder', { kind: k.kind, action: 'delete', name, to: null });
          },
        });
      },
      { enabled: writable && real, title: real ? 'Delete the folder shown; its items stay' : 'Show a folder first' },
    ),
  );
}

function draw(host) {
  const lib = st.lib;
  const parts = [];
  if (!lib) {
    parts.push(el('p', { class: 'setup-empty', text: st.loading ? 'Reading the library…' : 'The library could not be read.' }));
  } else if (!lib.configured) {
    noLibrary(parts);
  } else {
    parts.push(header(lib));
    if (lib.problem) {
      parts.push(el('p', { class: 'setup-error apply-error', text: lib.problem, attrs: { role: 'alert' } }));
    } else {
      const writable = !lib.in_tab;
      if (lib.in_tab) {
        parts.push(el('p', { class: 'setup-empty', text: 'Your library is open as a budget tab: you can add from it here, and change it in that tab.' }));
      }
      const k = kindOf(lib, st.kind);
      parts.push(el('div', { class: 'library-picks' }, kindSelect(lib), folderSelect(k)));
      parts.push(el('div', { class: 'apply-tools' }, filterBox()));
      const shown = shownItems(k, st.folder, st.filter);
      const list = el('ul', { class: 'apply-items library-list', attrs: { 'aria-label': `${kindLabel(k.kind)} in the library` } });
      for (const it of shown) list.append(itemRow(k, it, writable));
      if (!(k.items || []).length) parts.push(el('p', { class: 'setup-empty', text: `No ${kindLabel(k.kind).toLowerCase()} in the library yet.` }));
      else if (!shown.length) parts.push(el('p', { class: 'setup-empty', text: 'Nothing here matches.' }));
      parts.push(list);
      const ticked = [...st.ticked];
      parts.push(
        el(
          'div',
          { class: 'library-actions' },
          button(ticked.length > 1 ? `Add ${ticked.length} to This Budget…` : 'Add to This Budget…', 'library:add', () => addToBudget(k.kind, ticked), {
            enabled: ticked.length > 0,
            title: ticked.length ? 'Copy the picked items, and what they need, into this budget' : 'Tick items first (or double-click one)',
          }),
          button('Move To…', 'library:move', (b) => moveMenu(pointBelow(b), k, ticked, b), {
            enabled: writable && ticked.length > 0 && (k.folders || []).length > 0,
            title: (k.folders || []).length ? 'Put the picked items in a folder' : 'Make a folder first',
          }),
        ),
      );
      parts.push(folderButtons(k, writable));
      parts.push(
        el(
          'div',
          { class: 'library-actions library-save' },
          button('Save to Library…', 'library:save', () => saveToLibrary(panelCall, toast, null), {
            enabled: writable,
            title: writable ? 'Copy setup items from this budget into the library' : 'The library is open as a tab: change it there',
          }),
        ),
      );
    }
  }
  if (st.error) parts.push(el('p', { class: 'setup-error apply-error', text: st.error, attrs: { role: 'alert' } }));
  const box = el('div', { class: 'apply library', attrs: { 'aria-busy': st.busy || st.loading ? 'true' : 'false' } }, ...parts);
  const a = document.activeElement;
  const had = a && host.contains(a) && a.dataset ? a.dataset.key : null;
  host.replaceChildren(box);
  if (had) {
    const again = byKey(had);
    if (again) again.focus({ preventScroll: true });
  }
}
