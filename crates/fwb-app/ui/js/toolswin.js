// The tools window: the tools panel popped out of the main window
// (src/tools.rs). It loads the same page as the main window with ?panel=1
// (boot.js) and draws only the panel (tools.js), with the same commands on
// the same budgets in Rust.
//
// It follows the budget in front in the main window: it fetches the view
// with no `doc` (the one in front), and every command it sends names the
// budget it drew (api.js), so an edit made just as the main window switches
// tabs lands where the user meant.
//
// Keeping in step with the main window, by events listened for under this
// window's label ("tools"):
//   view-changed       the main window changed a budget or brought another
//                      tab to the front ({doc, rev}): fetch the view
//   tools-state        the main window's selection and the tab layout, sent
//                      when this window says tools-ready, and with a tab to
//                      focus when a tab key or menu item asked for one
//   selection-changed  the grid's selection and cursor, and which budget
//   menu               a menu item chosen while this window was in front
// and sent to the main window: tools-ready, tools-position (a dock button),
// tools-layout (tabs moved: the main window keeps the preference),
// tools-jump (a noted line to go to) and menu (keys and menu items for the
// main window: Save, Undo, grid commands...).
//
// Keys: text fields keep their own editing keys; app keys go to the menu bar
// on macOS (whose items come here first) and are handled here on Windows and
// Linux; the tab keys show a tab here, and Find, Find and Replace, Find Next
// and Find Previous work in the Find tab here; everything else goes to the
// main window. Close Window (Cmd+W) closes this window, which docks the panel back.

import { invoke, connected, setDefaultArgs } from './api.js';
import { $, el, isTextField } from './dom.js';
import { renderTools, setToolsLayout, toolsLayout, selectToolsTab, focusToolsKey } from './tools.js';
import { openFind, findStep } from './find.js';
import { matchShortcut, menuRunsAppKeys, shortcutById } from './shortcuts.js';
import { isModalOpen } from './modal.js';

const state = {
  view: null,
  selection: null,
};

const NO_SELECTION = { level: 'topsheet', keys: [], cat: null, cursor: null, account: null };

// Every command names the budget this window drew.
setDefaultArgs(() => (state.view && typeof state.view.doc === 'number' ? { doc: state.view.doc } : {}));

// ---------------------------------------------------------------------------
// The page

function build() {
  document.title = 'Tools';
  document.body.classList.add('tools-window');
  const panel = el('aside', { class: 'panel', attrs: { id: 'panel', 'aria-label': 'Tools' } });
  const shell = document.querySelector('.app');
  if (shell) shell.replaceWith(panel);
  else document.body.prepend(panel);
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
      // The main window brought another budget forward meanwhile: follow it.
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
    selection: () => state.selection || NO_SELECTION,
    position: 'window',
    setPosition: (where) => emitTo('main', 'tools-position', where),
    jumpTo: (entry) => emitTo('main', 'tools-jump', { ...entry, doc: state.view ? state.view.doc : null }),
    onLayout: (layout) => emitTo('main', 'tools-layout', layout),
  };
}

function render() {
  if (state.view) renderTools($('panel'), state.view, ctx());
}

// The main window changed the budget, or shows another account: fetch the
// view, once even if several changes arrive while it is on its way.
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

/** The grid's selection changed in the main window. */
function setSelection(sel) {
  if (!sel || typeof sel !== 'object') return;
  state.selection = sel;
  // Another budget came to the front, or (the Notes tab reads the line from
  // the account on screen) the main window moved to another account: fetch
  // the view.
  const a = sel.account;
  const shown = state.view && state.view.account;
  const otherDoc = state.view && typeof sel.doc === 'number' && sel.doc !== state.view.doc;
  if (otherDoc || (sel.level === 'details' && a && (!shown || shown.code !== a.code))) refresh();
  document.dispatchEvent(new CustomEvent('fwb:selection'));
  document.dispatchEvent(new CustomEvent('fwb:cursor'));
}

function showTab(tab) {
  const key = selectToolsTab(tab);
  if (!key) return;
  render();
  focusToolsKey(key);
  emitTo('main', 'tools-layout', toolsLayout());
}

/** The Find tab, in find or replace mode, with focus in its search box. */
function showFind(replace) {
  if (!selectToolsTab('find')) return;
  render();
  openFind({ replace });
  emitTo('main', 'tools-layout', toolsLayout());
}

// Find and replace live in this window while the panel is popped out: the
// main window passes these on (find.js).
const FIND_COMMANDS = {
  'edit.find': () => showFind(false),
  'edit.replace': () => showFind(true),
  'edit.find_next': () => findStep(1) || showFind(false),
  'edit.find_previous': () => findStep(-1) || showFind(false),
};

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
  if (id.startsWith('view.tab.')) {
    showTab(id.slice('view.tab.'.length));
    return;
  }
  if (Object.prototype.hasOwnProperty.call(FIND_COMMANDS, id)) {
    FIND_COMMANDS[id]();
    return;
  }
  // A grid key while typing in a field was meant for the field (the budget
  // tab keys mean nothing there, so they go on).
  const sc = shortcutById(id);
  if (sc && sc.scope === 'grid' && inField && !sc.anywhere) return;
  // Close Window or Close Tab here (Ctrl+W on Windows and Linux) closes this
  // window, which docks the panel back.
  if (id === 'file.close' || id === 'file.close_tab') {
    emitTo('main', 'tools-position', 'right');
    return;
  }
  emitTo('main', 'menu', id);
}

function onKeyDown(e) {
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
  // Grid commands: a text field keeps its keys (but for the budget tab keys);
  // otherwise the main window's grid gets them (cancelled here so the menu
  // bar doesn't run them twice).
  if (inField && !sc.anywhere) return;
  e.preventDefault();
  onMenu(sc.id);
}

async function listen() {
  const events = window.__TAURI__ && window.__TAURI__.event;
  if (!events || typeof events.listen !== 'function') return;
  const on = (name, fn) => events.listen(name, (e) => fn(e.payload), { target: 'tools' }).catch((err) => toast(err));
  await Promise.all([
    on('view-changed', () => refresh()),
    on('selection-changed', setSelection),
    on('tools-state', (p) => {
      if (!p || typeof p !== 'object') return;
      if (p.layout) setToolsLayout(p.layout);
      if (p.selection) state.selection = p.selection;
      render();
      if (p.selection) setSelection(p.selection);
      // Edit > Find or Find and Replace in the main window (main.js showFind).
      if (p.focus === 'find:open' || p.focus === 'find:open-replace') openFind({ replace: p.focus === 'find:open-replace' });
      else if (p.focus) focusToolsKey(p.focus);
    }),
    on('menu', onMenu),
  ]);
}

async function start() {
  build();
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
  // The main window answers with the selection and the tab layout.
  emitTo('main', 'tools-ready', null);
}

start();
