// The Budget Setup window (src/setupwin.rs; decisions.md, entry 42): every
// setup table's full editor, one tab each (setuptabs.js), with the setup
// library beside the table when the window is wide enough. It loads the same
// page as the main window with ?panel=setup (boot.js) and calls the same
// commands on the same budgets in Rust.
//
// The tabs reuse the editors the tools panel used to hold: the fringe,
// group, set, location and currency tables are apply.js's lists drawn as
// setup tables (ctx.mode 'setup': every column of each item's definition, no
// selection and nothing to apply), Globals and Units are setup.js's full
// tables, and Charges, Credits, Info, Settings and Baseline are
// topsheet-setup.js's (through setup.js). The library is library.js's tab,
// set to the table's kind.
//
// It follows the budget in front (the target pane's when the main window is
// split): it fetches the view with no `doc`, and every command it sends names
// the budget it drew (api.js).
//
// Events listened for under this window's label ("setup"):
//   view-changed   a budget changed or another tab came to the front: fetch
//   setup-state    the tab to show, and an item or control to focus
//   menu           a menu item chosen while this window was in front
// and sent to the main window: setup-ready, setup-tab (the tab shown: the
// main window remembers it), setup-find (an item's line count was clicked:
// Find shows its uses in the tools panel), setup-close (Escape, or Close
// Window on Windows and Linux) and menu (keys and menu items for the main
// window: Save, Undo...).
//
// Keys: text fields keep their own editing keys; the setup keys (Cmd+3...,
// Cmd+G, Cmd+/, Cmd+\) show their tab here; Escape (when nothing in the
// window wants it) and Close Window or Close Tab close the window, and the
// main window's grid gets focus back; everything else goes to the main window.

import { invoke, connected, setDefaultArgs } from './api.js';
import { $, el, isTextField } from './dom.js';
import { applyView, applyListHost, APPLY_LISTS } from './apply.js';
import { setupBegin, renderSetupTab, setupEnd } from './setup.js';
import { setFindHook, onUsage } from './setuplist.js';
import { renderLibrary, setLibraryKind } from './library.js';
import { SETUP_TABS, isSetupTab } from './setuptabs.js';
import { matchShortcut, menuRunsAppKeys, shortcutById } from './shortcuts.js';
import { isModalOpen } from './modal.js';
import { isMenuOpen, isContextMenuOpen } from './menu.js';

const state = {
  view: null,
  tab: SETUP_TABS[0].id,
  // The library kind last set for the rail: set again only when the tab
  // changes, so a kind picked in the rail stays while editing.
  libraryTab: null,
};

const NO_SELECTION = { level: 'topsheet', keys: [], cat: null, cursor: null, account: null };

// Every command names the budget this window drew.
setDefaultArgs(() => (state.view && typeof state.view.doc === 'number' ? { doc: state.view.doc } : {}));

// An item's line count was clicked: the tools panel's Find tab shows its
// uses (in the main window, or the tools window when popped out).
setFindHook((table, name) => emitTo('main', 'setup-find', { table, name }));

// The tables' line counts arrived, or their sort or Show choice changed.
onUsage(() => render());

// ---------------------------------------------------------------------------
// The page

function build() {
  document.title = 'Budget Setup';
  document.body.classList.add('setup-window');
  const host = el('div', { class: 'sw-host', attrs: { id: 'sw-host' } });
  const shell = document.querySelector('.app');
  if (shell) shell.replaceWith(host);
  else document.body.prepend(host);
}

let toastTimer = null;
function toast(message) {
  const t = $('toast');
  if (!t) return;
  t.textContent = String(message);
  t.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => {
    t.hidden = true;
  }, 6000);
}

function emitTo(target, name, payload) {
  const events = window.__TAURI__ && window.__TAURI__.event;
  if (!events || typeof events.emitTo !== 'function') return Promise.resolve();
  return events.emitTo(target, name, payload).catch(() => {});
}

/** Runs a command and redraws from the view it returns. Rejects with a message. */
async function call(cmd, args = {}) {
  const v = await invoke(cmd, args);
  if (v && typeof v === 'object' && 'budget' in v) {
    const front = Array.isArray(v.tabs) ? v.tabs.find((t) => t.active) : null;
    if (front && front.id !== v.doc) {
      // Another budget came forward meanwhile: follow it.
      refresh();
    } else {
      state.view = v;
      render();
    }
  }
  return v;
}

function ctx() {
  return {
    call: (cmd, args) => call(cmd, args),
    // Setup tables: no selection, nothing to apply (apply.js).
    mode: 'setup',
    selection: () => NO_SELECTION,
    position: 'setup',
    toast: (message) => toast(message),
  };
}

function tabButton(t, i) {
  const on = t.id === state.tab;
  const b = el('button', {
    class: 'sw-tab',
    text: t.label,
    attrs: {
      type: 'button',
      role: 'tab',
      id: `sw-tab-${t.id}`,
      'aria-selected': String(on),
      'aria-controls': 'sw-panel',
      tabindex: on ? '0' : '-1',
    },
    data: { key: `setup:tab:${t.id}`, tab: t.id, nobusy: '1' },
  });
  b.addEventListener('click', () => showTab(t.id));
  b.addEventListener('keydown', (e) => {
    if (e.altKey || e.metaKey || e.ctrlKey) return;
    const n = SETUP_TABS.length;
    let j = null;
    if (e.key === 'ArrowRight') j = (i + 1) % n;
    else if (e.key === 'ArrowLeft') j = (i - 1 + n) % n;
    else if (e.key === 'Home') j = 0;
    else if (e.key === 'End') j = n - 1;
    if (j === null) return;
    e.preventDefault();
    showTab(SETUP_TABS[j].id);
  });
  return b;
}

function render() {
  const host = $('sw-host');
  if (!host || !state.view) return;
  const c = ctx();
  const focus = setupBegin(host, state.view, c);
  applyView(state.view, c);
  const strip = el('div', { class: 'sw-tabs', attrs: { role: 'tablist', 'aria-label': 'Budget setup' } });
  SETUP_TABS.forEach((t, i) => strip.append(tabButton(t, i)));
  const tab = SETUP_TABS.find((t) => t.id === state.tab) || SETUP_TABS[0];
  const main = el('div', { class: `sw-main sw-main-${tab.id}` });
  if (APPLY_LISTS.includes(tab.id)) main.append(applyListHost(tab.id));
  else renderSetupTab(main, tab.id);
  const body = el(
    'div',
    {
      class: `sw-body${tab.library ? ' has-library' : ''}`,
      attrs: { role: 'tabpanel', id: 'sw-panel', 'aria-labelledby': `sw-tab-${tab.id}` },
    },
    main,
  );
  if (tab.library) {
    if (state.libraryTab !== tab.id) {
      setLibraryKind(tab.library);
      state.libraryTab = tab.id;
    }
    const lib = el('div', { class: 'library-host' });
    renderLibrary(lib, state.view, c);
    body.append(el('aside', { class: 'sw-library', attrs: { 'aria-label': 'Setup library' } }, lib));
  }
  const root = el('div', { class: 'sw setup' }, strip, body);
  setupEnd(host, root, focus);
}

/** Focuses the control with data-key `key`; true when there is one. */
function focusKey(key) {
  const host = $('sw-host');
  if (!host || !key) return false;
  const t = [...host.querySelectorAll('[data-key]')].find((e) => e.dataset.key === key);
  if (!t) return false;
  t.focus();
  if (typeof t.scrollIntoView === 'function') t.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  return true;
}

/** The control that edits item `name` of tab `tab`, as a data-key. */
function itemKey(tab, name) {
  if (APPLY_LISTS.includes(tab)) return `apply:item:${tab}:${name}`;
  if (tab === 'globals') return JSON.stringify(['globals', name, 'formula']);
  if (tab === 'units') return JSON.stringify(['units', name, 'plural']);
  return null;
}

/** Shows tab `id`, with focus on its tab (or on `focus`, a control in it). */
function showTab(id, focus = null) {
  if (!isSetupTab(id)) return;
  const changed = state.tab !== id;
  state.tab = id;
  render();
  if (!focus || !focusKey(focus)) focusKey(`setup:tab:${id}`);
  if (changed) emitTo('main', 'setup-tab', id);
}

// A budget changed, or another one came to the front: fetch the view, once
// even if several changes arrive while it is on its way.
let refreshing = false;
let refreshAgain = false;

async function refresh() {
  if (refreshing) {
    refreshAgain = true;
    return;
  }
  refreshing = true;
  try {
    do {
      refreshAgain = false;
      try {
        // The budget in front, whichever this window drew last.
        const v = await invoke('get_view', { doc: null });
        if (v && typeof v === 'object' && 'budget' in v) {
          state.view = v;
          render();
        }
      } catch (e) {
        toast(e);
      }
    } while (refreshAgain);
  } finally {
    refreshing = false;
  }
}

/** The main window asked for a tab: { tab, item, focus }. */
function onState(p) {
  if (!p || typeof p !== 'object') return;
  const tab = isSetupTab(p.tab) ? p.tab : state.tab;
  const focus = typeof p.focus === 'string' && p.focus ? p.focus : typeof p.item === 'string' ? itemKey(tab, p.item) : null;
  showTab(tab, focus);
}

/** Closes this window (the main window does it, and takes focus back). */
function closeWindow() {
  emitTo('main', 'setup-close', null);
}

// ---------------------------------------------------------------------------
// Menu commands and keys

const FIELD_COMMANDS = new Set(['edit.undo', 'edit.redo', 'edit.select_all']);

/** Undo, Redo and Select All in a text field act on the field. */
function fieldEdit(id) {
  const a = document.activeElement;
  if (id === 'edit.select_all') {
    if (typeof a.select === 'function') a.select();
  } else {
    document.execCommand(id.slice('edit.'.length)); // the field's own typing history
  }
}

/** A menu item chosen while this window was in front, or a key pressed here. */
function onMenu(id) {
  if (typeof id !== 'string') return;
  const inField = isTextField(document.activeElement);
  if (inField && FIELD_COMMANDS.has(id)) {
    fieldEdit(id);
    return;
  }
  if (isModalOpen()) return; // a small editor is open: it has the keys
  if (id.startsWith('view.tab.') && isSetupTab(id.slice('view.tab.'.length))) {
    showTab(id.slice('view.tab.'.length));
    return;
  }
  if (id === 'view.setup') {
    focusKey(`setup:tab:${state.tab}`);
    return;
  }
  // Close Window or Close Tab here (Ctrl+W on Windows and Linux).
  if (id === 'file.close' || id === 'file.close_tab') {
    closeWindow();
    return;
  }
  // A grid key while typing in a field was meant for the field.
  const sc = shortcutById(id);
  if (sc && sc.scope === 'grid' && inField && !sc.anywhere) return;
  emitTo('main', 'menu', id);
}

// Whether something open in the window (a small editor, a menu) had Escape
// before it reached the page: noted as the key goes down, before they close.
let escapeOwned = false;

function onKeyDownCapture(e) {
  if (e.key === 'Escape') escapeOwned = isModalOpen() || isContextMenuOpen() || isMenuOpen();
}

/** A field's typing not yet committed (setup.js keeps the old value in data-orig). */
function editing(node) {
  if (!isTextField(node)) return false;
  if (node.tagName === 'SELECT') return false;
  const orig = node.dataset && node.dataset.orig !== undefined ? node.dataset.orig : node.defaultValue;
  return node.value !== orig;
}

function onKeyDown(e) {
  if (e.key === 'Escape' && !e.altKey && !e.metaKey && !e.ctrlKey && !e.shiftKey) {
    const owned = escapeOwned;
    escapeOwned = false;
    if (owned || e.defaultPrevented || isModalOpen() || editing(document.activeElement)) return;
    e.preventDefault();
    closeWindow();
    return;
  }
  if (isModalOpen()) return; // the editors handle their own keys
  const sc = matchShortcut(e);
  if (!sc) return;
  const inField = isTextField(document.activeElement);
  if (sc.scope === 'app') {
    if (menuRunsAppKeys) return; // macOS: the menu item's key runs it (and comes here)
    if (inField && sc.field) return; // the field's own undo, redo or select all
    e.preventDefault();
    onMenu(sc.id);
    return;
  }
  if (inField && !sc.anywhere) return;
  e.preventDefault();
  onMenu(sc.id);
}

async function listen() {
  const events = window.__TAURI__ && window.__TAURI__.event;
  if (!events || typeof events.listen !== 'function') return;
  const on = (name, fn) => events.listen(name, (e) => fn(e.payload), { target: 'setup' }).catch((err) => toast(err));
  await Promise.all([on('view-changed', () => refresh()), on('setup-state', onState), on('menu', onMenu)]);
}

async function start() {
  build();
  window.addEventListener('keydown', onKeyDownCapture, true);
  window.addEventListener('keydown', onKeyDown);
  if (!connected) {
    toast('Open this page in the Film Wizard Budgeting app.');
    return;
  }
  await listen();
  try {
    state.view = await invoke('get_view', { doc: null });
  } catch (e) {
    toast(e);
  }
  render();
  // The main window answers with the tab to show (and what to focus).
  emitTo('main', 'setup-ready', null);
}

start();
