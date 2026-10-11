// App shell: holds the current view, which level is showing, the toolbar,
// the budget tabs, breadcrumb, keyboard shortcuts and menu commands, and wires
// the grid, side panel and footer.
//
// Rust owns the budget. Every change is a command that returns a fresh view;
// this file only decides what to ask for and redraws from what comes back.
// The view is lean: the budget's outline and totals, plus the rows of the one
// account on screen (focus_account picks it). Printing asks for everything.
//
// Every keyboard shortcut and menu item runs a command from COMMANDS by id
// (ids and keys: shortcuts.js). A key press and a menu click reach the same
// function, so they always do the same thing.
//
// Several budgets can be open, one per tab (tabs.js; src/docs.rs). Every
// command names the budget on screen (`doc`, api.js), every view says which
// budget it is and lists the tabs, and switching tabs swaps in that budget's
// view. Each tab remembers its level, category, account, cursor, scroll and
// footer counters here; Rust remembers its account on screen and its undo.
//
// Split view (roadmap 2.26; pane.js): the window can show two panes, each
// with its own tabs, breadcrumb, grid and footer. Each pane keeps where it is
// in a Pane; `state` and `grid` below read and write the pane in use (`cur`),
// which is the target pane (the one last clicked or focused) but for a
// moment while another pane is drawn (withPane). So every command, key and
// menu item acts on the target, as with one pane. The other pane follows
// edits to its budget with its own view (pane_view: its own account).

import { invoke, connected, setDefaultArgs } from './api.js';
import { el, $, isTextField } from './dom.js';
import { Grid, parseTSV } from './grid.js';
import {
  indexView,
  findCategory,
  accountList,
  topsheetColumns,
  accountColumns,
  detailColumns,
  columnShown,
  OPTIONAL_DETAIL_COLUMNS,
  BASELINE_COLUMNS,
  COST_COLUMNS,
  topsheetRows,
  accountRows,
  detailRows,
} from './levels.js';
import { renderFooter } from './footer.js';
import { formDialog, confirmDialog, customDialog, isModalOpen } from './modal.js';
import { renderTools, selectToolsTab, focusToolsKey, toolsLayout, setToolsLayout, findUses } from './tools.js';
import { isSetupTab } from './setuptabs.js';
import { openFind, findStep } from './find.js';
import { printReport } from './report.js';
import { showReportSetup } from './reportsetup.js';
import { SHORTCUTS, matchShortcut, menuRunsAppKeys, shortcutById, shortcutText, withShortcut } from './shortcuts.js';
import { isHelpOpen, toggleHelp, closeHelp } from './help.js';
import { openMenu, updateMenu, closeMenu, isMenuOpen, setMenuAnchor, openContextMenu } from './menu.js';
import { readPref, writePref } from './prefs.js';
import { wirePanelResizer, syncResizer, sizePanel, isResizer } from './panelsize.js';
import { initSafety } from './safety.js';
import { initUpdates } from './updates.js';
import { initQuickEntry, editAssist, commitDetailCell, editNote, fringeRange, showFormula } from './quickentry.js';
import { initRowFormat, toggleStyle, indent, setColor, clearFormatting, PALETTE } from './rowformat.js';
import { initContextMenu, initContextMenuPresets, watchContextMenu } from './context.js';
import { initClipboard, copyEvent, cellsCopied, pasteEvent, copyRows, pasteRows } from './clipboard.js';
import { createTabStrip } from './tabs.js';
import { firstPane, secondPane, layoutPanes, isSplitLayout, markTarget, moveFormulaBar } from './pane.js';
import { localStamp } from './versions.js';
import { showVersions } from './versions.js';
import { showCompare } from './compare.js';
import { showWhatChanged, showMerge } from './diffmerge.js';
import { importSetup, newFromTemplate, saveAsTemplate } from './importsetup.js';
import { importSpreadsheet } from './sheetimport.js';
import { importChart } from './coaimport.js';
import { exportPack, importPack } from './packs.js';
import { exportSetupLists, importSetupLists } from './setuplists.js';
import { importActuals } from './actualsimport.js';
import { saveShareCopy, newSubBudget, renderShareBanner, offerReturnedCopy } from './sharing.js';
import { displayCommands, userDisplay, withoutZeroRows } from './display.js';
import { setExplainTarget, explainTargetAt } from './explain.js';

// The panes (pane.js): the first is always there, the second once the window
// has been split. `target` is the pane commands act on; `cur` the pane `state`
// and `grid` reach (the target, but while withPane draws another pane).
const panes = [firstPane(), null];
let target = panes[0];
let cur = panes[0];

/** The pane in use. */
function P() {
  return cur;
}

/** Runs `fn` with `pane` as the pane in use (drawing a pane that isn't the target). */
function withPane(pane, fn) {
  const was = cur;
  cur = pane;
  try {
    return fn();
  } finally {
    cur = was;
  }
}

// What a pane keeps (pane.js, Pane.st): the budget on screen in it (`doc`, its
// tab in front there), its view and index, the level ('topsheet' | 'accounts'
// | 'details'), category { code, ti }, account { code, ai }, the selection's
// { count, totals }, level -> row key to return to (`memory`), and the footer's
// change counters { start, prev, last }: net totals when the file was opened
// or created, after the latest command, and the latest edit's effect.
const PANE_FIELDS = new Set(['doc', 'view', 'ix', 'level', 'cat', 'acct', 'selection', 'memory', 'changes']);

const state = new Proxy(
  {
    tabs: [], // every open budget's tab, in order (the view's `tabs`)
    panelOpen: true, // the tools panel shows (docked, or its window)
    position: readPosition(), // 'right', 'bottom' or 'window' (saved per user)
    columns: readColumnChoices(), // optional detail column -> shown (saved per user)
  },
  {
    get: (shared, k) => (PANE_FIELDS.has(k) ? cur.st[k] : shared[k]),
    set: (shared, k, v) => {
      if (PANE_FIELDS.has(k)) cur.st[k] = v;
      else shared[k] = v;
      return true;
    },
  },
);

// Every command names the budget on screen, so one sent just as another tab
// comes to the front still reaches the budget the user was looking at.
setDefaultArgs(() => (state.doc === null ? {} : { doc: state.doc }));

/** A pane's grid: its callbacks run with that pane in use. */
function makeGrid(pane) {
  const on =
    (fn) =>
    (...args) =>
      withPane(pane, () => fn(...args));
  return new Grid(pane.host, {
    onCommit: on((rowKey, colKey, value) => commitCell(rowKey, colKey, value)),
    onCommitMany: on((rowKeys, colKey, value) => commitMany(rowKeys, colKey, value)),
    onActivate: on((rowKey) => drillDown(rowKey)),
    onEscape: on(() => goUp()),
    onSelect: on((keys) => selectionChanged(keys)),
    onClearCells: on((cells) => clearCells(cells)),
    onPastEnd: on(() => {
      if (state.level === 'details') insertRow('line');
    }),
    // Shift+Tab from the first cell reaches the column menu button.
    onTabOut: on(() => {
      const b = pane.host.querySelector('.col-chooser');
      if (b) b.focus();
      return Boolean(b);
    }),
    // Type-ahead in setup-backed cells, and the formula bar (quickentry.js):
    // the target pane's cursor only.
    onEditStart: on((editor) => editAssist(editor)),
    onCursor: on((at) => {
      if (pane !== target) return;
      showFormula(at);
      cursorMoved();
    }),
    enterMovesDown: false,
  });
}
panes[0].grid = makeGrid(panes[0]);

// The grid in use. Display choices (zoom, formulas in a selection) go to
// every pane's grid.
const EVERY_GRID = new Set(['setZoom', 'setShowFormulas']);
const grid = new Proxy(
  {},
  {
    get: (_, k) => {
      if (EVERY_GRID.has(k)) {
        return (...args) => {
          for (const p of panes) if (p && p.grid) p.grid[k](...args);
        };
      }
      const g = cur.grid;
      const v = g[k];
      return typeof v === 'function' ? v.bind(g) : v;
    },
    set: (_, k, v) => {
      cur.grid[k] = v;
      return true;
    },
  },
);

initQuickEntry({ grid, view: () => state.view, level: () => state.level, act, call, toast });
// Per-user display choices (display.js): grid zoom, formulas in a selection.
grid.setZoom(userDisplay().zoom);
grid.setShowFormulas(userDisplay().formulasInSelection);
// Format menu and keys (rowformat.js); the grid's right-click menu (context.js).
initRowFormat({ grid, view: () => state.view, level: () => state.level, act });
// Its items call the commands directly: not through run(), whose guard against
// a key and its menu item both arriving would drop a key pressed right after.
initContextMenu({
  grid,
  host: () => P().host,
  view: () => state.view,
  level: () => state.level,
  run: (id) => COMMANDS[id] && COMMANDS[id](),
  act,
  toast,
  // Explain This Number… on the cell right-clicked (explain.js).
  explainable: (row, colKey) => Boolean(explainTargetFor(row, colKey)),
  explain: (row, colKey) => explainCell(row, colKey),
});
// The right-click menu's preset submenus (presets.js) need the selection, and
// a call that rejects so a form can show its message.
initContextMenuPresets({ selection: () => currentSelection(), call: (cmd, args, opts) => call(cmd, args, opts) });
// Copy and paste whole rows with everything on them (clipboard.js).
initClipboard({ grid, level: () => state.level, ix: () => state.ix, cat: () => state.cat, acct: () => state.acct, act, render, toast });

// ---------------------------------------------------------------------------
// Talking to Rust

/** Runs a command and redraws from the view it returns. Rejects with a message. */
async function call(cmd, args = {}, opts = {}) {
  const v = await invoke(cmd, args);
  if (v && typeof v === 'object' && 'budget' in v) apply(v, { ...opts, cmd });
  return v;
}

/** Like call(), but problems become a toast instead of a rejection. */
async function act(cmd, args = {}, opts = {}) {
  try {
    return await call(cmd, args, opts);
  } catch (e) {
    toast(e);
    return null;
  }
}

function apply(view, opts = {}) {
  if (Array.isArray(view.tabs) && view.tabs.length) state.tabs = view.tabs;
  // Which pane draws it (split view): the target, unless it is the other
  // pane's own view. A reply about a budget that isn't in front (one saved or
  // closed from its tab): its tab is all it changes.
  const pane = routeView(view, opts);
  if (pane) withPane(pane, () => applyHere(view, opts));
  followOtherPane(pane, view);
}

/** Draws `view` in the pane in use. */
function applyHere(view, opts = {}) {
  // Waiting for this pane's own account (refocus): that view comes next.
  if (P().refocusing && !opts.refocused) return;
  let switched = false;
  if (view.doc !== state.doc) {
    // Another tab came to the front (or the first view arrived).
    if (state.doc !== null) leaveTab();
    opts = { ...enterTab(view.doc), ...opts };
    switched = true;
    // Split view: the budget's account on screen in Rust may be the one the
    // other pane last showed; this pane asks for its own, then draws.
    if (refocus(view, opts)) return;
  }
  // Rows a command just made (a line block inserted from the tools panel):
  // the cursor goes to the first one.
  if (opts.focusCreated && typeof view.created_row === 'number') {
    opts = { ...opts, focusKey: `r${view.created_row}`, focusCol: 'description' };
  }
  const same = !switched && state.view && state.view.rev === view.rev;
  state.view = view;
  state.ix = indexView(view);
  trackChanges(view, opts);
  // Leaving an account (focus_account with none) changes nothing on screen.
  if (opts.quiet && same) return;
  if (switched) closeMenu();
  render(opts);
  if (opts.scroll) {
    P().host.scrollTop = opts.scroll.top;
    P().host.scrollLeft = opts.scroll.left;
  }
  // The selected rows' totals may have changed even if the selection didn't.
  if (state.level === 'details') requestSelectionTotals(grid.selectedKeys());
  // A returned copy next to the budget it came from: offer once (sharing.js).
  if (switched) offerReturnedCopy(view, sharingCtx());
}

// Commands that return a view without changing the budget.
const NOT_EDITS = new Set([
  'get_view',
  'save_budget',
  'focus_account',
  'flush_autosave',
  'switch_tab',
  'new_tab',
  'close_tab',
  'open_budget',
  'open_recent',
  'open_pending',
  'open_recovered',
  'template_new',
  'chart_template_new',
  // Split view (src/tabs.rs): the panes changed, not the budget.
  'split_view',
  'close_split',
  'set_target_pane',
  'move_tab_in_pane',
  'move_to_pane',
  'pane_view',
  'share_copy_editable',
  'sub_budget_new',
]);

/**
 * Footer change counters. `opts.fresh` (a file opened or created) starts
 * over; every other command that returns a view is an edit, and its effect on
 * the net total becomes the last change (0 for an edit that moves no money).
 */
function trackChanges(view, opts) {
  const net = view.report ? view.report.net_total : null;
  if (opts.fresh || !state.changes) {
    state.changes = { start: net, prev: net, last: 0 };
    return;
  }
  const c = state.changes;
  if (net === null) return; // no totals right now; compare with the last ones known
  if (c.start === null) {
    c.start = net; // the first totals since the file was opened
    c.prev = net;
    return;
  }
  if (!NOT_EDITS.has(opts.cmd)) c.last = net - c.prev;
  c.prev = net;
}

let toastTimer = null;
function toast(message) {
  const t = $('toast');
  t.textContent = String(message);
  t.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => {
    t.hidden = true;
  }, 6000);
}

// ---------------------------------------------------------------------------
// Where we are

/** Resolves the current category and account, stepping up a level if they're gone. */
function resolveNav() {
  const ix = state.ix;
  let entry = null;
  let ai = -1;
  if (state.level !== 'topsheet') {
    entry = state.cat ? findCategory(ix, state.cat.code, state.cat.ti) : null;
    if (!entry) {
      state.level = 'topsheet';
    } else {
      state.cat = { code: entry.cat.code, ti: entry.ti };
    }
  }
  if (state.level === 'details') {
    // The view carries the account on screen; Rust finds it the same way
    // (codes first), so its place is the one to show.
    const acct = state.view.account;
    const accts = entry.cat.accounts || [];
    if (!acct || acct.ti !== entry.ti || !accts[acct.ai] || accts[acct.ai].code !== acct.code) {
      state.level = 'accounts';
    } else {
      ai = acct.ai;
      state.acct = { code: acct.code, ai };
    }
  }
  return { entry, ai };
}

function render(opts = {}) {
  const v = state.view;
  if (!v) return;
  const { entry, ai } = resolveNav();
  if (cur === target) renderTitle(v);
  renderCrumbs(entry, ai);
  renderBanner(v);
  // A copy saved for sharing: read-only, and where it came from (sharing.js);
  // the target pane's budget, split.
  if (cur === target) renderShareBanner(v, sharingCtx());
  renderActions();

  const ix = state.ix;
  P().host.dataset.level = state.level;
  if (state.level === 'topsheet') {
    grid.h.enterMovesDown = false;
    grid.setData(withColumnMenu(topsheetColumns(ix, state.columns)), zeroFiltered(topsheetRows(ix, state.columns), opts), opts);
  } else if (state.level === 'accounts') {
    grid.h.enterMovesDown = false;
    grid.setData(withColumnMenu(accountColumns(ix, state.columns)), zeroFiltered(accountRows(ix, entry), opts), opts);
  } else {
    grid.h.enterMovesDown = true;
    const cols = gridDetailColumns();
    // The columns on screen decide which hidden factors get a marker.
    const rows = detailRows(ix, entry, ai, v.account, new Set(cols.map((c) => c.key)));
    if (opts.focusFirst) {
      const first = rows.find((r) => r.selectable !== false);
      opts = { ...opts, focusKey: first ? first.key : null };
    }
    grid.setData(cols, rows, opts);
  }
  if (cur === target) renderPanel();
  renderFooterNow();
}

/**
 * Topsheet and account-list rows without the zero-balance ones, when the
 * user hides them (View menu; display.js). The cursor's row stays, so a new
 * account doesn't vanish under it.
 */
function zeroFiltered(rows, opts) {
  if (!userDisplay().hideZero) return rows;
  return withoutZeroRows(rows, [grid.cursorKey(), opts.focusKey].filter((k) => k !== null && k !== undefined));
}

function renderTitle(v) {
  $('doc-name').textContent = v.file.name;
  $('doc-name').title = v.file.path || 'Not saved yet';
  $('doc-dirty').hidden = !v.file.dirty;
  $('btn-undo').disabled = !v.can_undo;
  $('btn-redo').disabled = !v.can_redo;
}

function crumb(label, onClick, current) {
  if (current) return el('span', { class: 'crumb current', text: label, title: label, attrs: { 'aria-current': 'page' } });
  return el('button', { class: 'crumb', text: label, title: label, attrs: { type: 'button' }, on: { click: onClick } });
}

/** A breadcrumb segment's jump list: a small button that opens a menu of its siblings. */
function crumbPicker(id, label, items, onPick) {
  return el('button', {
    class: 'crumb-pick',
    title: label,
    attrs: { type: 'button', 'aria-label': label, 'aria-haspopup': 'menu', 'aria-expanded': 'false' },
    data: { crumb: id },
    on: {
      click: (e) => openMenu(e.currentTarget, { owner: 'crumbs', label, items, onPick }),
      keydown: (e) => {
        // Down or Up opens the list too, as on a native pop-up button.
        if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
        e.preventDefault();
        e.stopPropagation();
        openMenu(e.currentTarget, { owner: 'crumbs', label, items, onPick });
      },
    },
  });
}

/** Previous or next account (the same as Cmd+[ and Cmd+]). Keeps focus when used from the keyboard. */
function crumbStep(id, label, delta, cmd) {
  return el('button', {
    class: 'crumb-step',
    text: delta < 0 ? '‹' : '›',
    title: withShortcut(label, cmd),
    attrs: { type: 'button', 'aria-label': label },
    data: { crumb: id },
    on: {
      click: (e) => {
        const fromKeyboard = e.detail === 0;
        stepAccount(delta);
        if (fromKeyboard) {
          const again = P().crumbs.querySelector(`[data-crumb="${id}"]`);
          if (again) again.focus();
        }
      },
    },
  });
}

function renderCrumbs(entry, ai) {
  closeMenu('crumbs');
  const ix = state.ix;
  const b = state.view.budget;
  const parts = [crumb(b.title || 'Untitled budget', () => goTo('topsheet'), state.level === 'topsheet')];
  if (entry) {
    const c = entry.cat;
    const cats = ix.cats.map((x) => ({
      id: x.ti,
      label: `${x.cat.code} ${x.cat.name}`.trim(),
      checked: x.ti === entry.ti,
    }));
    parts.push(el('span', { class: 'crumb-sep', text: '/' }));
    parts.push(
      el(
        'span',
        { class: 'crumb-seg' },
        crumb(`${c.code} ${c.name}`.trim(), () => goTo('accounts'), state.level === 'accounts'),
        crumbPicker('cat', 'Go to another category', cats, (it) => openCategory(it.id)),
      ),
    );
  }
  if (state.level === 'details' && entry) {
    const a = entry.cat.accounts[ai];
    const accts = (entry.cat.accounts || []).map((x, i) => ({
      id: i,
      label: `${x.code} ${x.name}`.trim(),
      checked: i === ai,
    }));
    parts.push(el('span', { class: 'crumb-sep', text: '/' }));
    parts.push(
      el(
        'span',
        { class: 'crumb-seg' },
        crumb(`${a.code} ${a.name}`.trim(), null, true),
        crumbPicker('acct', `Go to another account in ${entry.cat.code}`.trim(), accts, (it) => openAccount(entry.ti, it.id)),
      ),
    );
    parts.push(
      el(
        'span',
        { class: 'crumb-steps', attrs: { role: 'group', 'aria-label': 'Step through accounts' } },
        crumbStep('prev', 'Previous account', -1, 'go.prev_account'),
        crumbStep('next', 'Next account', 1, 'go.next_account'),
      ),
    );
  }
  P().crumbs.replaceChildren(...parts);
}

function renderBanner(v) {
  const banner = P().banner;
  if (!v.error) {
    banner.hidden = true;
    banner.replaceChildren();
    return;
  }
  const count = typeof v.issue_count === 'number' ? v.issue_count : (v.issues || []).length;
  const head = v.report
    ? count > 1
      ? `${count} problems need fixing. Until then, totals count the flagged cells as blank.`
      : 'One problem needs fixing. Until then, totals count the flagged cell as blank.'
    : "This budget can't be totaled until this is fixed:";
  banner.replaceChildren(
    el('strong', { text: head }),
    el('span', { class: 'banner-detail', text: `${v.error.at}: ${v.error.message}` }),
  );
  banner.hidden = false;
}

function button(label, onClick, { title = '', cmd = '', danger = false } = {}) {
  return el('button', {
    class: danger ? 'btn btn-quiet btn-danger-text' : 'btn btn-quiet',
    text: label,
    title: withShortcut(title || label, cmd),
    attrs: { type: 'button' },
    on: {
      click: () => {
        onClick();
      },
    },
  });
}

function renderActions() {
  const bar = P().actions;
  if (state.level === 'topsheet') {
    bar.replaceChildren(
      button('Add category', addCategory),
      button('Add production total', addProductionTotal, { title: 'Add a production total (a subtotal of the categories above it)' }),
      button('Rename budget', renameBudget),
      el('span', { class: 'spacer' }),
      button('Delete', deleteTopsheetRow, { cmd: 'edit.delete_rows', danger: true }),
    );
  } else if (state.level === 'accounts') {
    bar.replaceChildren(
      button('Add account', addAccount),
      el('span', { class: 'spacer' }),
      button('Delete account', deleteAccount, { cmd: 'edit.delete_rows', danger: true }),
    );
  } else {
    bar.replaceChildren(
      button('Insert line', () => insertRow('line'), { cmd: 'insert.line' }),
      button('Insert subtotal', () => insertRow('subtotal'), { cmd: 'insert.subtotal' }),
      button('Insert adjustment', () => insertRow('adjustment'), {
        cmd: 'insert.adjustment',
        title: 'Insert an adjustment: a percentage of the lines above it (Amount 0.1 is 10%)',
      }),
      button('Insert separator', () => insertRow('separator'), {
        cmd: 'insert.separator',
        title: 'Insert a separator: a drawn rule line with no cost',
      }),
      el('span', { class: 'spacer' }),
      button('Delete rows', deleteRows, { cmd: 'edit.delete_rows', danger: true }),
    );
  }
}

/**
 * The tools panel (tools.js), docked at the right or the bottom. Popped out,
 * the docked place stays empty and the tools window draws the panel; this
 * window sends it the grid's selection.
 */
function renderPanel() {
  const panel = $('panel');
  const docked = state.position !== 'window';
  $('workspace').classList.toggle('tools-bottom', state.position === 'bottom');
  panel.hidden = !state.panelOpen || !docked;
  syncResizer();
  $('btn-panel').setAttribute('aria-pressed', String(state.panelOpen));
  if (!docked) broadcastSelection();
  if (panel.hidden || !state.view) return;
  renderTools(panel, state.view, toolsCtx());
}

function toolsCtx() {
  return {
    call: (cmd, args, opts) => call(cmd, args, opts),
    // The grid's selected row keys at the current level, and the cursor row.
    selection: currentSelection,
    position: state.position,
    setPosition: (where) => setPosition(where),
    jumpTo: (entry) => jumpTo(entry),
    // A short message under the grid (a rename says how many places changed).
    toast: (message) => toast(message),
    // The History tab (history.js): jumps and saving a step run here, like Undo.
    historyJump: (to) => historyJump(to),
    historySave: (step, name) => historySave(step, name),
    // A setup table's full editor, in the Budget Setup window.
    openSetup: (tab, item) => openSetup(tab, item),
  };
}

/** What the tools panel needs to know about the grid (and which budget it is). */
function currentSelection() {
  return {
    doc: state.doc,
    level: state.level,
    keys: grid.selectedKeys(),
    cat: state.cat,
    cursor: grid.cursorKey(),
    account:
      state.level === 'details' && state.acct && state.cat
        ? { code: state.acct.code, ai: state.acct.ai, ti: state.cat.ti }
        : null,
  };
}

function renderFooterNow() {
  const c = state.changes;
  const changes = c && c.start !== null && c.prev !== null ? { session: c.prev - c.start, last: c.last } : null;
  renderFooter(P().footer, state.view, state.level === 'details' ? state.selection : null, {
    context: footerContext(),
    changes,
  });
}

/** When drilled down: the current account's and its category's totals, labeled with code and name. */
function footerContext() {
  if (state.level === 'topsheet' || !state.ix || !state.cat) return [];
  const entry = findCategory(state.ix, state.cat.code, state.cat.ti);
  if (!entry) return [];
  const out = [];
  if (state.level === 'details' && state.acct) {
    const a = (entry.cat.accounts || [])[state.acct.ai];
    const ar = entry.rep ? entry.rep.accounts[state.acct.ai] : null;
    if (a) out.push({ label: `${a.code} ${a.name}`.trim(), cents: ar ? ar.total : null, title: 'Total of the account you are in' });
  }
  const c = entry.cat;
  out.push({ label: `${c.code} ${c.name}`.trim(), cents: entry.rep ? entry.rep.total : null, title: 'Total of the category you are in' });
  return out;
}

// ---------------------------------------------------------------------------
// Navigation

function goTo(level, opts = {}) {
  const order = ['topsheet', 'accounts', 'details'];
  if (order.indexOf(level) >= order.indexOf(state.level)) return;
  const leaving = state.level === 'details';
  state.level = level;
  state.selection = null;
  render({ focusKey: state.memory[level] || null, ...opts });
  grid.focus();
  // No account on screen: later views leave its rows out.
  if (leaving) act('focus_account', {}, { quiet: true });
}

function goUp() {
  if (state.level === 'details') goTo('accounts');
  else if (state.level === 'accounts') goTo('topsheet');
}

function drillDown(rowKey) {
  if (state.level === 'topsheet') openCategory(Number(rowKey.slice(1)));
  else if (state.level === 'accounts') openAccount(state.cat.ti, Number(rowKey.slice(1)));
}

/**
 * Shows the accounts of the category at topsheet index `ti` (from anywhere);
 * `land` ({ focusKey, focusCol }) puts the cursor on another account than the
 * first.
 */
function openCategory(ti, land = null) {
  const c = state.ix.cats.find((x) => x.ti === ti);
  if (!c) return;
  const leaving = state.level === 'details';
  state.memory.topsheet = `t${ti}`;
  state.cat = { code: c.cat.code, ti };
  state.level = 'accounts';
  state.selection = null;
  render({ focusKey: 'a0', resetSelection: true, ...land });
  grid.focus();
  if (leaving) act('focus_account', {}, { quiet: true });
}

// The account being opened, while Rust fetches its rows (so Cmd+] pressed
// again before they arrive steps on from there).
let opening = null;

/**
 * Shows the lines of account `ai` in the category at topsheet index `ti`
 * (from anywhere). The view carries only the account on screen, so this asks
 * Rust for it first and draws when it arrives.
 */
async function openAccount(ti, ai, rowKey = null, col = 'description') {
  const c = state.ix.cats.find((x) => x.ti === ti);
  const a = c && (c.cat.accounts || [])[ai];
  if (!a) return;
  const target = { ti, ai };
  opening = target;
  const pane = P();
  let v;
  try {
    v = await invoke('focus_account', { category: c.cat.code, ti, account: a.code, ai });
  } catch (e) {
    toast(e);
    return;
  } finally {
    if (opening === target) opening = null;
  }
  if (!v || !v.account) return;
  // Another pane became the target meanwhile: this pane still opens it.
  if (pane !== cur) {
    withPane(pane, () => landAccount(v, c, rowKey, col));
    return;
  }
  landAccount(v, c, rowKey, col);
}

/** Draws account `v.account` (just fetched) in the pane in use. */
function landAccount(v, c, rowKey, col) {
  state.memory.topsheet = `t${v.account.ti}`;
  state.memory.accounts = `a${v.account.ai}`;
  state.cat = { code: c.cat.code, ti: v.account.ti };
  state.acct = { code: v.account.code, ai: v.account.ai };
  state.level = 'details';
  state.selection = null;
  const land = rowKey ? { focusKey: rowKey } : { focusFirst: true };
  apply(v, { cmd: 'focus_account', ...land, focusCol: col, resetSelection: true, pane: P() });
  grid.focus();
}

/** Opens the category or account under the cursor (Cmd/Ctrl+Down). */
function goDown() {
  if (state.level === 'details') return;
  const r = grid.cursorRow();
  if (r && r.activatable) drillDown(r.key);
}

/** Next (1) or previous (-1) account's lines, across categories, wrapping around. */
function stepAccount(delta) {
  if (state.level !== 'details' || !state.acct) return;
  const list = accountList(state.ix);
  const from = opening || { ti: state.cat.ti, ai: state.acct.ai };
  const i = list.findIndex((x) => x.ti === from.ti && x.ai === from.ai);
  if (i < 0 || list.length < 2) return;
  const next = list[(i + delta + list.length) % list.length];
  openAccount(next.ti, next.ai);
}

// ---------------------------------------------------------------------------
// Editing cells

function commitCell(rowKey, colKey, value) {
  const ix = state.ix;
  if (state.level === 'details') {
    commitDetailCell(rowKey, colKey, value); // adds a new unit, set... with the edit
  } else if (state.level === 'topsheet') {
    const ti = Number(rowKey.slice(1));
    const row = ix.topsheet[ti];
    if (!row) return;
    if (row.kind === 'production_total') {
      act('update_topsheet_row', { index: ti, code: null, name: value });
    } else {
      const code = colKey === 'code' ? value : row.code;
      const name = colKey === 'name' ? value : row.name;
      act('update_topsheet_row', { index: ti, code, name });
    }
  } else {
    const ai = Number(rowKey.slice(1));
    const entry = findCategory(ix, state.cat.code, state.cat.ti);
    const a = entry && entry.cat.accounts[ai];
    if (!a) return;
    const newCode = colKey === 'code' ? value : a.code;
    const name = colKey === 'name' ? value : a.name;
    act('update_account', { code: a.code, newCode, name });
  }
}

function commitMany(rowKeys, colKey, value) {
  if (state.level !== 'details') {
    commitCell(rowKeys[0], colKey, value);
    return;
  }
  act('set_rows_field', { rowIds: rowKeys.map((k) => Number(k.slice(1))), field: colKey, value });
}

// ---------------------------------------------------------------------------
// Several cells at once: clear, paste, cut, fill down (detail grid)

/**
 * Sets detail cells as one undo step. cells: [{ rowKey, colKey, value }].
 * `action` ('paste', 'fill', 'cut', 'clear') names the step in the History
 * list (src/history.rs).
 */
function setCells(cells, action = null) {
  const edits = cells.map((c) => ({ rowId: Number(c.rowKey.slice(1)), field: c.colKey, value: c.value }));
  return act('set_cells', { edits, action });
}

function clearCells(cells, action = 'clear') {
  if (state.level === 'details') {
    setCells(cells.map((c) => ({ ...c, value: '' })), action);
    return;
  }
  // Category and account lists: one cell at a time, the cursor's.
  const at = grid.cursorCell();
  if (at && at.cell.edit !== undefined && at.cell.edit !== '') commitCell(at.row.key, at.col.key, '');
}

function fillDown() {
  if (state.level !== 'details') return;
  const cells = grid.fillDownTargets();
  if (cells.length) setCells(cells, 'fill');
  grid.focus();
}

const NUMBER_FIELDS = new Set(['amount', 'x', 'rate', 'x4']);
const PASTED_NUMBER = /^(\()?(-)?(\d{1,3}(?:,\d{3})+(?:\.\d+)?|\d*\.?\d+)(\))?$/;

/** Spreadsheet number text ("1,500.00", "(250)") as a plain number for formula cells. */
function pasteValue(colKey, value) {
  if (!NUMBER_FIELDS.has(colKey)) return value;
  const m = PASTED_NUMBER.exec(value.trim());
  if (!m || Boolean(m[1]) !== Boolean(m[4])) return value;
  const n = m[3].replace(/,/g, '');
  return m[1] || m[2] ? `-${n}` : n;
}

function pasteText(text) {
  if (!text) return;
  const matrix = parseTSV(text);
  if (state.level !== 'details') {
    // Category and account lists take one cell at a time.
    const at = grid.cursorCell();
    if (!at || at.cell.edit === undefined) return;
    if (matrix.length > 1 || matrix[0].length > 1) toast('Only one cell can be pasted here; pasted the first one.');
    const value = matrix[0][0];
    if (value !== String(at.cell.edit)) commitCell(at.row.key, at.col.key, value);
    return;
  }
  const { cells, rowsShort } = grid.pasteTargets(matrix);
  if (cells.length) setCells(cells.map((c) => ({ ...c, value: pasteValue(c.colKey, c.value) })), 'paste');
  if (rowsShort) {
    const rows = rowsShort === 1 ? '1 pasted row' : `${rowsShort} pasted rows`;
    toast(`${rows} didn't fit below the last line. Insert lines, then paste again.`);
  }
}

/** True when the grid (not a text field or dialog) gets copy, cut and paste. */
function gridHasClipboard() {
  if (isHelpOpen() || isModalOpen() || grid.editor) return false;
  const a = document.activeElement;
  return !a || a === document.body || a === P().host;
}

function wireClipboard() {
  // WebKit enables Copy, Cut and Paste with nothing selected only when these
  // "before" events are cancelled.
  for (const type of ['beforecopy', 'beforecut', 'beforepaste']) {
    document.addEventListener(type, (e) => {
      if (gridHasClipboard()) e.preventDefault();
    });
  }
  // True when cells were copied (a cut then clears them). Whole rows are
  // copied, or cut, with everything on them instead (clipboard.js).
  const copy = (e, cut = false) => {
    if (!gridHasClipboard() || !e.clipboardData) return false;
    e.preventDefault();
    if (copyEvent(e.clipboardData, { cut })) return false;
    e.clipboardData.setData('text/plain', grid.copyText());
    cellsCopied();
    return true;
  };
  document.addEventListener('copy', (e) => copy(e));
  document.addEventListener('cut', (e) => {
    if (copy(e, true)) clearCells(grid.editableCells(), 'cut');
  });
  document.addEventListener('paste', (e) => {
    if (!gridHasClipboard() || !e.clipboardData) return;
    e.preventDefault();
    // Copied rows go in as rows above the selection (clipboard.js); any
    // other text is pasted into cells.
    const text = e.clipboardData.getData('text/plain');
    pasteEvent(text, () => pasteText(text));
  });
}

// ---------------------------------------------------------------------------
// Selection totals (detail grid)

let selTimer = null;
let selSeq = 0;
function selectionChanged(keys) {
  if (cur === target) {
    document.dispatchEvent(new CustomEvent('fwb:selection')); // the tools panel (apply.js, notes.js)
    broadcastSelection();
  }
  if (state.level !== 'details') return;
  requestSelectionTotals(keys);
}

/**
 * Asks for the selected detail rows' totals (debounced; the newest request
 * wins). The footer shows the row count at once and the totals when they come.
 * Called when the selection changes and after every edit, which can change
 * the totals of rows that stay selected.
 */
function requestSelectionTotals(keys) {
  const ids = keys.filter((k) => k.startsWith('r')).map((k) => Number(k.slice(1)));
  state.selection = { count: ids.length, totals: state.selection ? state.selection.totals : null };
  renderFooterNow();
  // Each pane asks for its own (its budget, its rows).
  const pane = P();
  const doc = state.doc;
  clearTimeout(pane.selTimer);
  const seq = (pane.selSeq = (pane.selSeq || 0) + 1);
  pane.selTimer = setTimeout(async () => {
    try {
      const totals = await invoke('selection_totals', { rowIds: ids, doc });
      withPane(pane, () => {
        if (seq !== pane.selSeq || state.level !== 'details' || state.doc !== doc) return;
        state.selection = { count: ids.length, totals };
        renderFooterNow();
      });
    } catch (e) {
      toast(e);
    }
  }, 80);
}

// ---------------------------------------------------------------------------
// Structure: rows, accounts, categories

function currentAccountCode() {
  return state.level === 'details' && state.acct ? state.acct.code : null;
}

async function insertRow(kind) {
  if (state.level !== 'details') return;
  const key = grid.cursorKey();
  const after = key && key.startsWith('r') ? Number(key.slice(1)) : null;
  const v = await act('add_detail', { accountCode: currentAccountCode(), after, kind });
  if (v && v.created_row !== null) {
    render({ focusKey: `r${v.created_row}`, focusCol: 'description' });
  }
  grid.focus();
}

async function deleteRows() {
  if (state.level === 'topsheet') return deleteTopsheetRow();
  if (state.level === 'accounts') return deleteAccount();
  const ids = grid
    .selectedKeys()
    .filter((k) => k.startsWith('r'))
    .map((k) => Number(k.slice(1)));
  if (!ids.length) return;
  await act('delete_details', { rowIds: ids });
  grid.focus();
}

/** The next code after `prev` with its numeric tail incremented, avoiding `taken`. */
function nextCode(prev, taken, fallback) {
  let code = prev || fallback;
  for (let n = 0; n < 1000; n++) {
    const m = /^(.*?)(\d+)$/.exec(code);
    if (!m) return fallback;
    const next = String(Number(m[2]) + 1).padStart(m[2].length, '0');
    code = m[1] + next;
    if (!taken.has(code)) return code;
  }
  return fallback;
}

async function addCategory() {
  const ix = state.ix;
  const key = grid.cursorKey();
  const at = key && key.startsWith('t') ? Number(key.slice(1)) + 1 : ix.topsheet.length;
  const before = ix.cats.filter((c) => c.ti < at);
  const prev = before.length ? before[before.length - 1].cat.code : '';
  const taken = new Set(ix.cats.map((c) => c.cat.code));
  const suggested = prev ? nextCode(prev, taken, '') : '10';
  const ok = await formDialog({
    title: 'Add category',
    fields: [
      { key: 'code', label: 'Category number', value: suggested },
      { key: 'name', label: 'Description', value: '', placeholder: 'e.g. Art department' },
    ],
    submitText: 'Add',
    onSubmit: (f) => call('add_category', { code: f.code, name: f.name, at }, { focusKey: `t${at}`, focusCol: 'name' }),
  });
  if (ok) grid.focus();
}

async function addProductionTotal() {
  const ix = state.ix;
  const key = grid.cursorKey();
  const at = key && key.startsWith('t') ? Number(key.slice(1)) + 1 : ix.topsheet.length;
  const ok = await formDialog({
    title: 'Add production total',
    message: 'A production total adds up every category above it, back to the previous production total. The first one ends above-the-line.',
    fields: [{ key: 'name', label: 'Name', value: '', placeholder: 'e.g. Total production' }],
    submitText: 'Add',
    onSubmit: (f) => call('add_production_total', { name: f.name, at }, { focusKey: `t${at}`, focusCol: 'name' }),
  });
  if (ok) grid.focus();
}

async function renameBudget() {
  const ok = await formDialog({
    title: 'Rename budget',
    fields: [{ key: 'title', label: 'Budget title', value: state.view.budget.title || '' }],
    submitText: 'Rename',
    onSubmit: (f) => call('set_title', { title: f.title }),
  });
  if (ok) grid.focus();
}

/** Insert > Category / Production Total: these live on the topsheet, so go there first. */
async function insertOnTopsheet(add) {
  if (state.level !== 'topsheet') goTo('topsheet');
  await add();
}

/** Insert > Account: in the current category, after the current account. */
async function insertAccount() {
  if (state.level === 'details') {
    goTo('accounts');
  } else if (state.level === 'topsheet') {
    const r = grid.cursorRow();
    if (!r || !r.activatable) return;
    drillDown(r.key);
    const entry = findCategory(state.ix, state.cat.code, state.cat.ti);
    const n = entry ? (entry.cat.accounts || []).length : 0;
    if (n) grid.goTo(`a${n - 1}`);
  }
  await addAccount();
}

async function deleteTopsheetRow() {
  const key = grid.cursorKey();
  if (!key || !key.startsWith('t')) return;
  const ti = Number(key.slice(1));
  const row = state.ix.topsheet[ti];
  if (!row) return;
  if (row.kind === 'production_total') {
    await act('delete_topsheet_row', { index: ti });
    grid.focus();
    return;
  }
  const n = (row.accounts || []).length;
  const what = n === 1 ? '1 account' : `${n} accounts`;
  const ok = await confirmDialog({
    title: 'Delete category',
    message: `Delete ${row.code} ${row.name} and its ${what}, with every line in them? You can undo this.`,
    confirmText: 'Delete',
    danger: true,
  });
  if (ok) await act('delete_topsheet_row', { index: ti });
  grid.focus();
}

async function addAccount() {
  const ix = state.ix;
  const entry = findCategory(ix, state.cat.code, state.cat.ti);
  if (!entry) return;
  const accts = entry.cat.accounts || [];
  const key = grid.cursorKey();
  const ai = key && key.startsWith('a') ? Number(key.slice(1)) : accts.length - 1;
  const after = accts[ai] ? accts[ai].code : null;
  const taken = new Set(ix.cats.flatMap((c) => (c.cat.accounts || []).map((a) => a.code)));
  const coding = ix.b.account_coding || {};
  const sep = coding.separator === undefined ? '-' : coding.separator;
  const width = coding.account_width || 2;
  const first = `${entry.cat.code}${sep}${'0'.repeat(Math.max(0, width - 1))}1`;
  const suggested = after ? nextCode(after, taken, first) : taken.has(first) ? nextCode(first, taken, first) : first;
  const insertAt = after ? ai + 1 : accts.length;
  const ok = await formDialog({
    title: 'Add account',
    fields: [
      { key: 'code', label: 'Account number', value: suggested },
      { key: 'name', label: 'Description', value: '', placeholder: 'e.g. Set dressing' },
    ],
    submitText: 'Add',
    onSubmit: (f) =>
      call(
        'add_account',
        { categoryCode: entry.cat.code, code: f.code, name: f.name, after },
        { focusKey: `a${insertAt}`, focusCol: 'name' },
      ),
  });
  if (ok) grid.focus();
}

async function deleteAccount() {
  const key = grid.cursorKey();
  if (!key || !key.startsWith('a')) return;
  const entry = findCategory(state.ix, state.cat.code, state.cat.ti);
  const a = entry && entry.cat.accounts[Number(key.slice(1))];
  if (!a) return;
  const n = typeof a.lines === 'number' ? a.lines : (a.details || []).length;
  const ok = n === 0 || (await confirmDialog({
    title: 'Delete account',
    message: `Delete ${a.code} ${a.name} and its ${n === 1 ? '1 line' : `${n} lines`}? You can undo this.`,
    confirmText: 'Delete',
    danger: true,
  }));
  if (ok) await act('delete_account', { code: a.code });
  grid.focus();
}

// ---------------------------------------------------------------------------
// Budget tabs (tabs.js draws the strip; src/tabs.rs keeps the budgets)

function setTabs(tabs) {
  state.tabs = tabs;
  drawStrips();
}

/** Remembers where this tab was left, to come back to it. */
function leaveTab() {
  if (!state.tabs.some((t) => t.id === state.doc)) {
    P().tabMemory.delete(state.doc); // closed
    return;
  }
  P().tabMemory.set(state.doc, whereHere());
}

/** Where the pane in use is: its level, category, account, cursor, scroll. */
function whereHere() {
  const host = P().host;
  return {
    level: state.level,
    cat: state.cat,
    acct: state.acct,
    memory: { ...state.memory },
    changes: state.changes,
    cursor: grid.cursorKey(),
    col: grid.cursorColKey(),
    scroll: { top: host.scrollTop, left: host.scrollLeft },
  };
}

/**
 * Makes budget `doc` the one on screen: where its tab was left, or the
 * topsheet for a budget not seen yet. Returns options for drawing it.
 */
function enterTab(doc) {
  state.doc = doc;
  state.selection = null;
  const m = P().tabMemory.get(doc);
  if (!m) {
    resetNav();
    state.changes = null;
    return { focusKey: 't0', resetSelection: true, fresh: true };
  }
  P().tabMemory.delete(doc);
  state.level = m.level;
  state.cat = m.cat;
  state.acct = m.acct;
  state.memory = m.memory;
  state.changes = m.changes;
  return { focusKey: m.cursor, focusCol: m.col, resetSelection: true, scroll: m.scroll };
}

// The tab on its way to the front, so Ctrl+Tab pressed again before it
// arrives steps on from there.
let switching = null;

/**
 * Brings budget `id`'s tab to the front. `keepFocus`: the tab strip keeps the
 * keyboard. `pane`: the pane whose strip it was picked in (split view; it
 * becomes the target); without one, the tab comes forward where it is.
 */
async function switchTo(id, { keepFocus = false, pane = null } = {}) {
  if (typeof id !== 'number' || !state.tabs.some((t) => t.id === id)) return;
  if (pane && pane !== target) setTarget(pane);
  if (id === (switching === null ? state.doc : switching)) {
    if (keepFocus) target.strip.focus(id);
    return;
  }
  if (grid.editor) grid.commitEdit(null);
  switching = id;
  try {
    await act('switch_tab', pane && isSplit() ? { doc: id, pane: pane.index } : { doc: id });
  } finally {
    if (switching === id) switching = null;
  }
  if (keepFocus) target.strip.focus(id);
  else grid.focus();
}

/** The next (1) or previous (-1) tab of the target pane, wrapping round. */
function stepTab(delta) {
  const ids = paneTabs(target);
  const n = ids.length;
  const from = switching === null ? state.doc : switching;
  const i = ids.indexOf(from);
  if (n < 2 || i < 0) return;
  switchTo(ids[(i + delta + n) % n], { keepFocus: tabsHaveFocus(), pane: target });
}

/** The target pane's tab at place `i` (Cmd+1, Cmd+2). */
function tabAt(i) {
  const id = paneTabs(target)[i];
  if (id !== undefined) switchTo(id, { keepFocus: tabsHaveFocus(), pane: target });
}

/** True when keyboard focus is on a tab strip. */
function tabsHaveFocus() {
  return panes.some((p) => p && p.strip && p.strip.hasFocus());
}

/** Moves budget `id`'s tab to place `to` (in `pane`'s strip, split). */
async function moveTab(id, to, pane = target) {
  try {
    if (isSplit()) {
      await call('move_tab_in_pane', { pane: pane.index, doc: id, to });
    } else {
      const tabs = await invoke('move_tab', { doc: id, to });
      if (Array.isArray(tabs)) setTabs(tabs);
    }
    pane.strip.focus(id);
  } catch (e) {
    toast(e);
  }
}

/**
 * Asks what to do with unsaved changes before a tab closes: resolves
 * 'save', 'discard' or null (keep the tab).
 */
function unsavedDialog(name) {
  return new Promise((resolve) => {
    let answer = null;
    customDialog(
      (close) => {
        const pick = (choice) => {
          answer = choice;
          close();
        };
        const cancel = el('button', { class: 'btn', text: 'Cancel', attrs: { type: 'button' } });
        const discard = el('button', { class: 'btn btn-danger-text', text: 'Close Without Saving', attrs: { type: 'button' } });
        const save = el('button', { class: 'btn btn-primary', text: 'Save', attrs: { type: 'button', 'data-autofocus': true } });
        cancel.addEventListener('click', () => pick(null));
        discard.addEventListener('click', () => pick('discard'));
        save.addEventListener('click', () => pick('save'));
        return el(
          'div',
          { class: 'modal', attrs: { role: 'dialog', 'aria-labelledby': 'unsaved-title', 'aria-describedby': 'unsaved-message' } },
          el('h2', { class: 'modal-title', text: 'Unsaved changes', attrs: { id: 'unsaved-title' } }),
          el('p', {
            class: 'modal-message',
            text: `"${name}" has changes that are not saved. Save them before closing the tab?`,
            attrs: { id: 'unsaved-message' },
          }),
          el('div', { class: 'modal-actions' }, discard, el('span', { class: 'spacer' }), cancel, save),
        );
      },
      // Escape or a click outside: keep the tab.
      { onClose: () => resolve(answer) },
    );
  });
}

/**
 * Closes budget `id`'s tab (Cmd+W: the one in front). Unsaved changes are
 * autosaved into the file first when Autosave is on; what is still unsaved
 * asks Save, Close Without Saving or Cancel. The last tab closes the window
 * instead, which asks about unsaved changes in Rust.
 */
async function closeTab(id = state.doc, pane = target) {
  // Split, with the budget in the other pane too: only this tab goes.
  const both = isSplit() && panes.every((p) => paneTabs(p).includes(id));
  if (state.tabs.length <= 1 && !both) {
    closeWindow();
    return;
  }
  if (grid.editor) grid.commitEdit(null);
  const where = isSplit() ? { pane: pane.index } : {};
  let r;
  try {
    r = await invoke('close_tab', { doc: id, discard: false, ...where });
    if (r && r.unsaved) {
      const tab = state.tabs.find((t) => t.id === id);
      const choice = await unsavedDialog(tab ? tab.label : 'This budget');
      if (choice === 'save') {
        const v = await call('save_budget', { saveAs: false, doc: id });
        // Cancelled, or not written (the file changed outside the app).
        if (!v || v.file.dirty) return grid.focus();
        r = await invoke('close_tab', { doc: id, discard: false, ...where });
      } else if (choice === 'discard') {
        r = await invoke('close_tab', { doc: id, discard: true, ...where });
      } else {
        return grid.focus();
      }
    }
  } catch (e) {
    toast(e);
    return grid.focus();
  }
  if (r && r.last) {
    closeWindow();
    return;
  }
  if (r && r.closed && r.view) apply(r.view, { cmd: 'close_tab' });
  pane.tabMemory.delete(id);
  grid.focus();
}

// ---------------------------------------------------------------------------
// Files and history

function resetNav() {
  state.level = 'topsheet';
  state.cat = null;
  state.acct = null;
  state.memory = {};
  state.selection = null;
}

/** New (and New Tab): a fresh budget in a new tab. */
async function doNew() {
  if (grid.editor) grid.commitEdit(null);
  await act('new_tab');
  grid.focus();
}

/** Open: the chosen file in a new tab (or its own tab, when it is open). */
async function doOpen() {
  if (grid.editor) grid.commitEdit(null);
  try {
    // Resolves null when the user cancels the dialog.
    const v = await invoke('open_budget');
    if (v) apply(v, { cmd: 'open_budget' });
  } catch (e) {
    toast(e);
  }
  grid.focus();
}

async function doSave(saveAs) {
  if (grid.editor) grid.commitEdit(null);
  await act('save_budget', { saveAs });
  grid.focus();
}

function doUndo() {
  act('undo');
}

function doRedo() {
  act('redo');
}

// Saving safety: Autosave, recovered budgets, backups, Open Recent and files
// the system opens (safety.js, src/safety_app.rs).

/** Opens the budget `cmd` returns (null: nothing) in a tab: its own when it is open already. */
async function openBy(cmd, args = {}) {
  if (grid.editor) grid.commitEdit(null);
  try {
    const v = await invoke(cmd, args);
    if (v) apply(v, { cmd });
  } catch (e) {
    toast(e);
  }
  grid.focus();
}

const safety = initSafety({
  grid,
  toast,
  call,
  openBy,
  saveAs: () => doSave(true),
  view: () => state.view,
  // A budget was saved into its file without a command (autosave, Save All
  // on the way out): only its tab and file state changed.
  fileState: (p) => {
    if (Array.isArray(p.tabs) && p.tabs.length) setTabs(p.tabs);
    if (state.view && p.doc === state.doc && state.view.rev === p.rev) {
      state.view.file = p.file;
      renderTitle(state.view);
    }
  },
  // Brings budget `doc`'s tab to the front (a question about it is coming).
  showDoc: (doc) => switchTo(doc),
  doc: () => state.doc,
});

/**
 * File > Print: the printout needs every row, which views leave out, so this
 * asks for everything (after any queued edits, so the printout has them).
 */
async function doPrint(kind) {
  let full;
  try {
    full = await invoke('get_full_view');
  } catch (e) {
    toast(e);
    return;
  }
  try {
    await printReport(full, kind);
  } catch (e) {
    toast(e);
  }
  grid.focus();
}

/** File > Print: the report setup dialog, with its preview, presets and PDF. */
function openReportSetup() {
  if (grid.editor) grid.commitEdit(null);
  if (!state.view) return;
  showReportSetup({ view: () => state.view, call: (cmd, args) => call(cmd, args), toast });
}

// ---------------------------------------------------------------------------
// View: the tools panel, docked at the right (the default) or the bottom, or
// popped out into its own window (src/tools.rs). Where it is is saved per user.

// A function declaration, not a const: `state` reads it before this line runs.
function positions() {
  return ['right', 'bottom', 'window'];
}

function readPosition() {
  const p = readPref('toolsPosition', 'right');
  return positions().includes(p) ? p : 'right';
}

function panelHasFocus() {
  const a = document.activeElement;
  return $('panel').contains(a) || isResizer(a);
}

/** Shows or hides the panel wherever it is (Option+Cmd+S). */
function togglePanel() {
  const hadFocus = panelHasFocus();
  state.panelOpen = !state.panelOpen;
  if (state.position === 'window') openToolsWindow(state.panelOpen);
  renderPanel();
  if (!state.panelOpen && hadFocus) grid.focus();
}

// The tab to focus in the tools window once it has the layout.
let toolsFocus = null;

/** Opens the tools panel on a tab and puts focus on the tab. */
function showTab(tab, focus = null) {
  const key = selectToolsTab(tab);
  // `focus`: a control in the tab to focus instead of the tab itself.
  const target = key && focus ? focus : key;
  state.panelOpen = true;
  if (state.position === 'window') {
    toolsFocus = target;
    openToolsWindow(true).then(sendToolsState);
    return;
  }
  renderPanel();
  if (!focusToolsKey(target)) focusToolsKey(key);
}

/**
 * Shows the Find tab in find or replace mode, with focus in its search box
 * (in the tools window when the panel is popped out).
 */
function showFind(replace) {
  selectToolsTab('find');
  state.panelOpen = true;
  if (state.position === 'window') {
    toolsFocus = replace ? 'find:open-replace' : 'find:open';
    openToolsWindow(true).then(sendToolsState);
    return;
  }
  renderPanel();
  openFind({ replace });
}

/**
 * The next (1) or previous (-1) Find result (F3, Shift+F3), searched where
 * the panel is; with nothing to find yet, the Find tab opens instead.
 */
function stepFind(delta) {
  if (state.position === 'window') {
    if (state.panelOpen) emitTo('tools', 'menu', delta > 0 ? 'edit.find_next' : 'edit.find_previous');
    else showFind(false);
    return;
  }
  if (!findStep(delta)) showFind(false);
}

/**
 * Moves the panel: 'right' or 'bottom' docks it in this window, 'window' pops
 * it out. Showing it if it was hidden. `fromTools`: asked by the tools window
 * (its dock buttons) or because it closed.
 */
function setPosition(where, { fromTools = false } = {}) {
  if (!positions().includes(where)) return;
  const was = state.position;
  if (where === was && state.panelOpen) return;
  const hadFocus = panelHasFocus();
  state.position = where;
  state.panelOpen = true;
  writePref('toolsPosition', where);
  sizePanel();
  renderPanel();
  if (where === 'window') {
    openToolsWindow(true).then(sendToolsState);
    if (hadFocus) grid.focus();
    return;
  }
  if (was === 'window') invoke('close_tools_window').catch(() => {});
  if (hadFocus || fromTools) focusToolsKey(`tools:place:${where}`);
}

/** Shows (made if needed, where it was last) or hides the tools window. */
async function openToolsWindow(show) {
  try {
    await invoke('tools_window', { show, geometry: geometryPref('toolsWindow') });
  } catch (e) {
    toast(e);
    if (show && state.position === 'window') {
      // No window: keep the panel where it can be used.
      state.position = 'right';
      writePref('toolsPosition', 'right');
      sizePanel();
      renderPanel();
    }
  }
}

function emitTo(target, name, payload) {
  const events = window.__TAURI__ && window.__TAURI__.event;
  if (!events || typeof events.emitTo !== 'function') return Promise.resolve();
  return events.emitTo(target, name, payload).catch(() => {});
}

/** Everything the tools window needs to draw: the selection and the layout. */
function sendToolsState() {
  if (state.position !== 'window') return;
  const sel = currentSelection();
  lastSelectionSig = JSON.stringify(sel);
  emitTo('tools', 'tools-state', { selection: sel, layout: toolsLayout(), focus: toolsFocus, explain: toolsExplain });
  toolsFocus = null;
  toolsExplain = null;
}

// The grid's selection as last sent to the tools window.
let lastSelectionSig = '';
let selectionTimer = null;

/** Sends the grid's selection to the tools window when it changed. */
function broadcastSelection() {
  if (state.position !== 'window' || !state.panelOpen) return;
  clearTimeout(selectionTimer);
  selectionTimer = setTimeout(() => {
    const sel = currentSelection();
    const sig = JSON.stringify(sel);
    if (sig === lastSelectionSig) return;
    lastSelectionSig = sig;
    emitTo('tools', 'selection-changed', sel);
  }, 30);
}

/** The grid's cursor moved: the Notes tab shows that line's note. */
function cursorMoved() {
  document.dispatchEvent(new CustomEvent('fwb:cursor'));
  broadcastSelection();
}

/**
 * Goes to a line from the Notes tab's list or a Find result (or, for a note
 * on an account or a category, to that account or category): its account,
 * with the line selected (on the cell of `entry.field`, when given). A Find
 * result with no `row_id` is an account's name or number: the category's
 * account list, on that account. Resolves once it is on screen.
 */
function jumpTo(entry) {
  if (!entry || typeof entry !== 'object' || !state.ix) return Promise.resolve();
  // A category's own note (notes.js): the topsheet, on that category.
  if (entry.kind === 'category') {
    const c = state.ix.cats.find((x) => x.ti === entry.ti && x.cat.code === entry.category) || state.ix.cats.find((x) => x.cat.code === entry.category);
    if (!c) {
      toast('That category is no longer in the budget.');
      return Promise.resolve();
    }
    if (grid.editor) grid.commitEdit(null);
    state.memory.topsheet = `t${c.ti}`;
    if (state.level === 'topsheet') {
      render({ focusKey: `t${c.ti}` });
      grid.focus();
    } else {
      goTo('topsheet');
    }
    return Promise.resolve();
  }
  const accountOnly = entry.row_id === null || entry.row_id === undefined;
  const id = Number(entry.row_id);
  if (!accountOnly && !Number.isSafeInteger(id)) return Promise.resolve();
  const col = typeof entry.field === 'string' && entry.field ? entry.field : null;
  let target = null;
  const c = state.ix.cats.find((x) => x.ti === entry.ti);
  const a = c && (c.cat.accounts || [])[entry.ai];
  if (a && a.code === entry.account) target = { ti: entry.ti, ai: entry.ai };
  for (const x of target ? [] : state.ix.cats) {
    const i = (x.cat.accounts || []).findIndex((y) => y.code === entry.account);
    if (i >= 0) {
      target = { ti: x.ti, ai: i };
      break;
    }
  }
  if (!target) {
    toast(accountOnly ? 'That account is no longer in the budget.' : 'That line is no longer in the budget.');
    return Promise.resolve();
  }
  if (grid.editor) grid.commitEdit(null);
  if (accountOnly) {
    openCategory(target.ti, { focusKey: `a${target.ai}`, focusCol: col || 'name' });
    return Promise.resolve();
  }
  return openAccount(target.ti, target.ai, `r${id}`, col || 'description');
}

// Another window changed the budget (src/tools.rs): fetch the view, once even
// if several changes arrive while it is on its way.
let refreshing = false;
let refreshAgain = false;

async function refreshView() {
  if (refreshing) {
    refreshAgain = true;
    return;
  }
  refreshing = true;
  try {
    do {
      refreshAgain = false;
      try {
        // The budget on screen; the one in front if it has gone meanwhile.
        const v = await invoke('get_view').catch(() => invoke('get_view', { doc: null }));
        // An edit made in the tools window: it counts in the footer's changes.
        if (v) apply(v, { cmd: 'view-changed' });
      } catch (e) {
        toast(e);
      }
    } while (refreshAgain);
  } finally {
    refreshing = false;
  }
}

// ---------------------------------------------------------------------------
// The Budget Setup window (setupwin.js, src/setupwin.rs): every setup table's
// full editor. This window remembers its tab, place and size (per user), and
// tells it what to show.

// What the setup window shows once its page is listening: { tab, item, focus }.
let setupPending = null;

function readSetupTab() {
  const t = readPref('setupTab', 'fringes');
  return isSetupTab(t) ? t : 'fringes';
}

/** `g` as a window place { x, y, width, height }, or null when it is nonsense. */
function geometryOf(g) {
  return g && typeof g === 'object' && ['x', 'y', 'width', 'height'].every((k) => typeof g[k] === 'number' && Number.isFinite(g[k]))
    ? { x: g.x, y: g.y, width: g.width, height: g.height }
    : null;
}

/** A remembered window place from prefs, or null when there is none or it is nonsense. */
function geometryPref(name) {
  return geometryOf(readPref(name, null));
}

/**
 * Remembers a window's place under pref `name` when it moves or is resized
 * (the tools window: src/tools.rs; Budget Setup: src/setupwin.rs), once it
 * has stopped for a moment.
 */
function geometrySaver(name) {
  let timer = null;
  return (g) => {
    if (!g || typeof g !== 'object') return;
    clearTimeout(timer);
    timer = setTimeout(() => {
      const place = geometryOf(g);
      if (place) writePref(name, place);
    }, 250);
  };
}

const saveGeometry = geometrySaver('toolsWindow');
const saveSetupGeometry = geometrySaver('setupWindow');

/**
 * Opens the Budget Setup window, or brings it forward, on `tab` (the one it
 * showed last when none), with item `item` or the control with data-key
 * `focus` focused.
 */
async function openSetup(tab = null, item = null, focus = null) {
  if (isSetupTab(tab)) writePref('setupTab', tab);
  const msg = {
    tab: isSetupTab(tab) ? tab : readSetupTab(),
    item: typeof item === 'string' ? item : null,
    focus: typeof focus === 'string' ? focus : null,
  };
  setupPending = msg;
  try {
    const existed = await invoke('setup_window', { show: true, geometry: geometryPref('setupWindow') });
    // A window already there is listening; a new one says setup-ready.
    if (existed) emitTo('setup', 'setup-state', msg);
  } catch (e) {
    toast(e);
  }
}

/** An item's line count was clicked in the setup window: Find shows its uses, wherever the panel is. */
function findUsesFromSetup(p) {
  if (!p || typeof p !== 'object' || typeof p.table !== 'string' || typeof p.name !== 'string') return;
  state.panelOpen = true;
  if (state.position === 'window') {
    openToolsWindow(true).then(() => emitTo('tools', 'tools-find', { table: p.table, name: p.name }));
    return;
  }
  renderPanel();
  findUses(p.table, p.name);
}

// ---------------------------------------------------------------------------
// View: optional detail columns (the menu in the grid's top-left corner; saved per user)

function readColumnChoices() {
  const saved = readPref('detailColumns', null);
  const out = {};
  if (saved && typeof saved === 'object') {
    for (const c of [...OPTIONAL_DETAIL_COLUMNS, ...COST_COLUMNS]) if (typeof saved[c.key] === 'boolean') out[c.key] = saved[c.key];
  }
  return out;
}

/**
 * What the column choices depend on: whether the budget has a baseline (Orig
 * and Var) and actuals (the cost report columns, levels.js).
 */
function columnContext() {
  return { baseline: Boolean(state.ix && state.ix.variance), cost: Boolean(state.ix && state.ix.cost) };
}

function gridDetailColumns() {
  return detailColumns(state.columns, columnContext()).map((c) => (c.key === 'num' ? { ...c, head: columnMenuButton } : c));
}

/**
 * Topsheet and account-list columns: with a baseline, the column menu (Orig
 * and Var) sits in the Acct # header, as it does in the # header of lines.
 */
function withColumnMenu(cols) {
  const ctx = columnContext();
  if (!ctx.baseline && !ctx.cost) return cols;
  const head = (label) => () => el('span', { class: 'col-head-menu' }, columnMenuButton(), el('span', { text: label }));
  return cols.map((c) => (c.key === 'code' ? { ...c, head: head(c.label) } : c));
}

/** The column menu: every optional column on the lines, Orig, Var and the cost columns elsewhere. */
function columnMenuItems() {
  const ctx = columnContext();
  const list = state.level === 'details' ? OPTIONAL_DETAIL_COLUMNS : [...BASELINE_COLUMNS, ...COST_COLUMNS];
  return list
    .filter((c) => (!c.baseline || ctx.baseline) && (!c.cost || ctx.cost))
    .map((c) => ({
      id: c.key,
      label: c.name || c.label,
      title: c.title,
      checked: columnShown(c, state.columns, ctx),
    }));
}

function columnsIcon() {
  const NS = 'http://www.w3.org/2000/svg';
  const svg = document.createElementNS(NS, 'svg');
  for (const [k, v] of Object.entries({ viewBox: '0 0 16 16', width: '14', height: '14', 'aria-hidden': 'true', focusable: 'false' })) {
    svg.setAttribute(k, v);
  }
  for (const x of [1.5, 6.5, 11.5]) {
    const r = document.createElementNS(NS, 'rect');
    for (const [k, v] of Object.entries({ x: String(x), y: '2', width: '3', height: '12', rx: '0.8' })) r.setAttribute(k, v);
    svg.append(r);
  }
  return svg;
}

function openColumnMenu(anchor) {
  openMenu(anchor, {
    owner: 'columns',
    label: 'Columns',
    checkable: true,
    items: columnMenuItems(),
    returnFocus: () => P().host.querySelector('.col-chooser'),
    onPick: (item) => {
      const col = OPTIONAL_DETAIL_COLUMNS.find((c) => c.key === item.id) || COST_COLUMNS.find((c) => c.key === item.id);
      if (!col) return;
      state.columns = { ...state.columns, [col.key]: !columnShown(col, state.columns, columnContext()) };
      writePref('detailColumns', state.columns);
      renderAll();
      updateMenu(columnMenuItems());
    },
  });
}

/** The header button for the column menu (the grid draws it in the # column's header). */
function columnMenuButton() {
  const b = el(
    'button',
    {
      class: 'col-chooser',
      title: 'Show or hide columns',
      attrs: { type: 'button', 'aria-label': 'Choose columns', 'aria-haspopup': 'menu', 'aria-expanded': 'false' },
      data: { focusId: 'col-chooser' },
      on: {
        click: (e) => openColumnMenu(e.currentTarget),
        keydown: (e) => {
          if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
          e.preventDefault();
          e.stopPropagation();
          openColumnMenu(e.currentTarget);
        },
      },
    },
    columnsIcon(),
  );
  // The grid redraws while the menu is open (a column was toggled): follow it.
  if (isMenuOpen('columns')) setMenuAnchor('columns', b);
  return b;
}

/** In a text field, Undo, Redo and Select All act on the field. */
function fieldEdit(kind) {
  const a = document.activeElement;
  if (!isTextField(a)) return false;
  if (kind === 'selectAll') {
    if (typeof a.select === 'function') a.select();
  } else {
    document.execCommand(kind); // the field's own typing history
  }
  return true;
}

function selectAll() {
  grid.selectAll();
  grid.focus();
}

// ---------------------------------------------------------------------------
// Commands: one per shortcut-list id, run from keys and the menu bar alike

const COMMANDS = {
  'file.new': () => doNew(),
  'file.new_tab': () => doNew(),
  'file.close_tab': () => closeTab(),
  'window.next_tab': () => stepTab(1),
  'window.prev_tab': () => stepTab(-1),
  'window.first_tab': () => tabAt(0),
  'window.second_tab': () => tabAt(1),
  'file.open': () => doOpen(),
  'file.save': () => doSave(false),
  'file.save_as': () => doSave(true),
  'file.print_topsheet': () => doPrint('topsheet'),
  'file.print_budget': () => openReportSetup(),
  'edit.undo': () => fieldEdit('undo') || doUndo(),
  'edit.redo': () => fieldEdit('redo') || doRedo(),
  'edit.select_all': () => fieldEdit('selectAll') || selectAll(),
  'edit.fill_down': fillDown,
  'edit.delete_rows': deleteRows,
  'edit.note': editNote,
  'edit.make_range': () => fringeRange(true),
  'edit.remove_range': () => fringeRange(false),
  'insert.line': () => insertRow('line'),
  'insert.subtotal': () => insertRow('subtotal'),
  'insert.adjustment': () => insertRow('adjustment'),
  'insert.separator': () => insertRow('separator'),
  'insert.account': insertAccount,
  'insert.category': () => insertOnTopsheet(addCategory),
  'insert.production_total': () => insertOnTopsheet(addProductionTotal),
  'go.topsheet': () => goTo('topsheet'),
  'go.up': goUp,
  'go.down': goDown,
  'go.next_account': () => stepAccount(1),
  'go.prev_account': () => stepAccount(-1),
  'view.toggle_panel': togglePanel,
  'view.panel.right': () => setPosition('right'),
  'view.panel.bottom': () => setPosition('bottom'),
  'view.panel.window': () => setPosition('window'),
  'help.shortcuts': toggleHelp,
  // The menu items are answered in Rust; these run only for the keys on
  // Windows and Linux, where the page sees them first.
  'file.close': closeWindow,
  'app.quit': closeWindow,
};
// View > Budget Setup's tabs (and their keys) open the Budget Setup window on
// that tab; the other view.tab items and View > Tools Panel's tabs show a tab
// of the tools panel.
for (const s of SHORTCUTS) {
  if (s.id.startsWith('view.tab.')) {
    const tab = s.id.slice('view.tab.'.length);
    COMMANDS[s.id] = isSetupTab(tab) ? () => openSetup(tab) : () => showTab(tab);
  } else if (s.id.startsWith('view.tool.')) {
    COMMANDS[s.id] = () => showTab(s.id.slice('view.tool.'.length));
  }
}
COMMANDS['view.setup'] = () => openSetup();
// The topsheet's right-click Create charge and Create credit: Budget Setup
// on that tab, with the new item's name box focused (topsheet-setup.js's add form).
COMMANDS['budget.create_charge'] = () => openSetup('charges', null, JSON.stringify(['add', 'charges', 'name']));
COMMANDS['budget.create_credit'] = () => openSetup('credits', null, JSON.stringify(['add', 'credits', 'name']));
// File > Open Recent and File > Revert to Backup (safety.js).
Object.assign(COMMANDS, safety.commands);
// Updates: the toolbar's quiet indicator, What's New once after an update,
// and Help > Release Notes (updates.js, src/updater.rs).
const updates = initUpdates({ grid, toast });
Object.assign(COMMANDS, updates.commands);
// View: formulas in a selection, zero accounts, totals' cents, grid zoom
// (display.js; per-user choices).
Object.assign(COMMANDS, displayCommands({ grid, render: () => renderAll(), toast }));
// Format: text style of the selected detail lines (rowformat.js).
Object.assign(COMMANDS, {
  'format.bold': () => toggleStyle('bold'),
  'format.italic': () => toggleStyle('italic'),
  'format.indent_more': () => indent(1),
  'format.indent_less': () => indent(-1),
  'format.clear': () => clearFormatting(),
  'format.color.none': () => setColor(null),
});
for (const c of PALETTE) COMMANDS[`format.color.${c.id}`] = () => setColor(c.id);
// Edit > Cut Rows, Copy Rows, Paste Rows: whole rows with everything on
// them, whatever columns are selected (clipboard.js).
Object.assign(COMMANDS, {
  'edit.cut_rows': () => copyRows(true),
  'edit.copy_rows': () => copyRows(false),
  'edit.paste_rows': () => pasteRows(),
});
// Edit > Find, Find and Replace, Find Next, Find Previous: the tools panel's
// Find tab (find.js), wherever the panel is.
Object.assign(COMMANDS, {
  'edit.find': () => showFind(false),
  'edit.replace': () => showFind(true),
  'edit.find_next': () => stepFind(1),
  'edit.find_previous': () => stepFind(-1),
});

// Edit > Move Up and Move Down (Option+Up, Option+Down): the selected lines,
// accounts or topsheet rows one place (src/moverows.rs).
Object.assign(COMMANDS, {
  'edit.move_up': () => moveSelected(-1),
  'edit.move_down': () => moveSelected(1),
});

// Edit > Explain This Number (the cursor cell's total, in the Explain tab:
// explain.js) and Overtime Helper (the Overtime tab: overtime.js).
Object.assign(COMMANDS, {
  'edit.explain': () => explainCell(grid.cursorRow(), grid.cursorColKey()),
  'edit.overtime': () => showTab('overtime'),
});

/** The total a grid cell shows, as the Explain tab names it, or null. */
function explainTargetFor(row, colKey) {
  if (!state.ix || !row) return null;
  const cat = state.cat ? findCategory(state.ix, state.cat.code, state.cat.ti) : null;
  return explainTargetAt({ level: state.level, row, colKey, ix: state.ix, cat, ai: state.acct ? state.acct.ai : null });
}

// A total to explain in the tools window, sent with the panel's state.
let toolsExplain = null;

/** Opens the Explain tab on the total a cell shows. */
function explainCell(row, colKey) {
  const t = explainTargetFor(row, colKey);
  if (!t) {
    toast('Put the cursor on a total to explain it.');
    return;
  }
  setExplainTarget(t, state.doc);
  toolsExplain = state.position === 'window' ? t : null;
  showTab('explain');
}

// One move at a time, each built from the newest view (accounts and
// topsheet rows are named by place, which a move changes). A held key adds
// at most one press behind the one running, so moves never pile up.
let moveQueue = Promise.resolve();
let movesWaiting = 0;

function moveSelected(by) {
  if (!state.view || movesWaiting > 1) return;
  movesWaiting += 1;
  moveQueue = moveQueue
    .then(() => moveOnce(by))
    .catch((e) => toast(e))
    .finally(() => {
      movesWaiting -= 1;
    });
}

async function moveOnce(by) {
  const keys = grid.selectedKeys();
  const cursor = grid.cursorKey();
  let cmd;
  let args;
  let keyFor;
  if (state.level === 'details') {
    const ids = keys.filter((k) => /^r\d+$/.test(k)).map((k) => Number(k.slice(1)));
    if (!ids.length) return;
    [cmd, args, keyFor] = ['move_rows', { rowIds: ids, by }, (id) => `r${id}`];
  } else if (state.level === 'accounts') {
    const entry = state.cat ? findCategory(state.ix, state.cat.code, state.cat.ti) : null;
    const accts = entry ? entry.cat.accounts || [] : [];
    const codes = keys
      .filter((k) => /^a\d+$/.test(k))
      .map((k) => accts[Number(k.slice(1))])
      .filter(Boolean)
      .map((a) => a.code);
    if (!codes.length) return;
    [cmd, args, keyFor] = ['move_accounts', { codes, by }, (i) => `a${i}`];
  } else {
    const indexes = keys.filter((k) => /^t\d+$/.test(k)).map((k) => Number(k.slice(1)));
    if (!indexes.length) return;
    [cmd, args, keyFor] = ['move_topsheet_rows', { indexes, by }, (i) => `t${i}`];
  }
  const v = await call(cmd, args);
  if (!v || !Array.isArray(v.moved)) return;
  // The moved rows stay selected and the cursor stays on its row: lines
  // keep their keys; accounts and topsheet rows are found by their order
  // among the selected ones, which a move keeps.
  const moved = v.moved.map(keyFor);
  let at = cursor;
  if (state.level !== 'details') {
    const i = keys.indexOf(cursor);
    at = moved[i >= 0 ? i : 0] || cursor;
  }
  grid.selectRows(moved, at);
}

// File > Versions… and File > Compare Budgets…, and their toolbar buttons
// (versions.js, compare.js).
Object.assign(COMMANDS, {
  'file.versions': () => openVersions(),
  'file.compare': () => openCompare(),
});

// File > What Changed… and Merge Changes From… (diffmerge.js;
// src/diffmerge.rs).
Object.assign(COMMANDS, {
  'file.what_changed': () => {
    if (grid.editor) grid.commitEdit(null);
    if (state.view) showWhatChanged(diffMergeCtx());
  },
  'file.merge': () => {
    if (grid.editor) grid.commitEdit(null);
    if (state.view) showMerge(diffMergeCtx());
  },
});

/** What diffmerge.js needs from the shell. */
function diffMergeCtx() {
  return {
    doc: () => state.doc,
    view: () => state.view,
    tabs: () => state.tabs,
    invoke,
    apply: (v) => apply(v, { cmd: 'merge_apply' }),
    toast,
    go: (target) => goToChange(target),
  };
}

/**
 * Goes to a change picked in What Changed (diffmerge.js): a setup tab, the
 * Versions dialog, a category on the topsheet, an account in its category's
 * list, or a row's cell.
 */
function goToChange(go) {
  if (!go || typeof go !== 'object') return;
  if (go.kind === 'setup') {
    const run = COMMANDS[`view.tab.${go.tab}`];
    if (run) run();
    return;
  }
  if (go.kind === 'versions') {
    openVersions();
    return;
  }
  if (go.kind === 'category') {
    jumpTo({ kind: 'category', ti: go.ti, category: go.category });
    return;
  }
  jumpTo({ ti: go.ti, ai: go.ai, account: go.account, row_id: go.row_id, field: go.field || null });
}

// File > Export: an Excel workbook, CSV files or tab-delimited detail. Rust
// shows the Save dialog and writes the files (src/export.rs).
Object.assign(COMMANDS, {
  'file.export_xlsx': () => doExport('xlsx'),
  'file.export_csv': () => doExport('csv'),
  'file.export_tsv': () => doExport('tsv'),
});

/** Exports the budget on screen (after any queued edits) as `format`. */
async function doExport(format) {
  if (grid.editor) grid.commitEdit(null);
  if (!state.view) return;
  try {
    const r = await invoke('export_budget', { format });
    if (r && r.done && Array.isArray(r.files) && r.files.length) {
      toast(r.files.length === 1 ? `Exported ${r.files[0]}.` : `Exported ${r.files.length} files: ${r.files.join(', ')}.`);
    }
  } catch (e) {
    toast(e);
  }
  grid.focus();
}

// File > New From Template…, Save as Template… and Import Setup From…
// (importsetup.js; src/templates.rs, src/import_setup.rs).
Object.assign(COMMANDS, {
  'file.new_from_template': () => startFromBudget(newFromTemplate, 'template_new'),
  'file.save_as_template': () => startFromBudget(saveAsTemplate, 'template_save'),
  'file.import_setup': () => startFromBudget(importSetup, 'import_setup'),
});

// File > Import Setup Pack…, Export Setup Pack…, Import Setup Lists… and
// Export > Setup Lists… (packs.js, setuplists.js; src/packs.rs,
// src/setup_sheets.rs).
Object.assign(COMMANDS, {
  'file.import_setup_pack': () => startFromBudget((c) => importPack(packsCtx(c), 'budget'), 'pack_import'),
  'file.export_setup_pack': () => startFromBudget((c) => exportPack(packsCtx(c), 'budget'), 'pack_export'),
  'file.import_setup_lists': () => startFromBudget((c) => importSetupLists(packsCtx(c)), 'import_setup'),
  'file.export_setup_lists': () => startFromBudget((c) => exportSetupLists(packsCtx(c)), 'setup_sheet_export'),
});

/** What packs.js and setuplists.js need, from startFromBudget's context. */
function packsCtx(c) {
  return { call: (cmd, args) => c.invoke(cmd, args), apply: c.apply, toast: c.toast, focus: c.focus };
}

// File > Save a Copy for Sharing… and New Sub-Budget… (sharing.js;
// src/sharing.rs).
Object.assign(COMMANDS, {
  'file.save_share_copy': () => {
    if (grid.editor) grid.commitEdit(null);
    if (state.view) saveShareCopy(sharingCtx());
  },
  'file.new_sub_budget': () => {
    if (grid.editor) grid.commitEdit(null);
    if (state.view) newSubBudget(sharingCtx());
  },
});

/** What sharing.js needs from the shell. */
function sharingCtx() {
  return {
    view: () => state.view,
    doc: () => state.doc,
    tabs: () => state.tabs,
    invoke,
    apply: (v) => apply(v, { cmd: 'share_copy' }),
    toast,
    focus: () => grid.focus(),
    // The original in front, comparing it with the copy.
    compare: async (original, copy) => {
      await switchTo(original);
      if (state.doc === original) openCompare({ tab: copy });
    },
  };
}

// File > Import Spreadsheet…: lines from a workbook, CSV or tab-delimited
// file with a column mapping (sheetimport.js; src/sheet_import.rs). The
// account on screen is the default for rows without one.
Object.assign(COMMANDS, {
  'file.import_spreadsheet': () => {
    if (grid.editor) grid.commitEdit(null);
    if (!state.view) return;
    importSpreadsheet({
      invoke,
      apply: (v) => apply(v, { cmd: 'sheet_import' }),
      toast,
      focus: () => grid.focus(),
      account: () => (state.level === 'details' && state.acct ? state.acct.code : ''),
    });
  },
});

// File > Import Chart of Accounts…: a chart from a spreadsheet or another
// budget, as a new budget or applied to this one (coaimport.js;
// src/coa_import.rs).
Object.assign(COMMANDS, {
  'file.import_chart': () => {
    if (grid.editor) grid.commitEdit(null);
    if (!state.view) return;
    importChart({ invoke, apply: (v) => apply(v, { cmd: 'coa_import' }), toast, focus: () => grid.focus() });
  },
});

// File > Import Actuals…: transactions from an accounting export for the
// cost report (actualsimport.js; src/actuals.rs). One undo step; the Actuals
// tab lists them and can remove the import.
Object.assign(COMMANDS, {
  'file.import_actuals': () => {
    if (grid.editor) grid.commitEdit(null);
    if (!state.view) return;
    importActuals({
      invoke,
      apply: (v) => apply(v, { cmd: 'actuals_import' }),
      toast,
      focus: () => grid.focus(),
    });
  },
});

/** Runs one of importsetup.js's commands for the budget on screen. */
function startFromBudget(run, cmd) {
  if (grid.editor) grid.commitEdit(null);
  if (!state.view) return;
  run({ invoke, apply: (v) => apply(v, { cmd }), toast, focus: () => grid.focus() });
}

/** The Versions dialog for the budget on screen. */
function openVersions() {
  if (grid.editor) grid.commitEdit(null);
  if (!state.view) return;
  showVersions({ view: () => state.view, call: (cmd, args) => call(cmd, args), compare: (id) => openCompare({ version: id }) });
}

/** The compare screen for the budget on screen (`opts.version`: just that version; `opts.tab`: just that open budget). */
function openCompare(opts = {}) {
  if (grid.editor) grid.commitEdit(null);
  if (!state.view) return;
  showCompare(
    {
      doc: () => state.doc,
      view: () => state.view,
      tabs: () => state.tabs,
      pickFiles: () => invoke('compare_pick_files'),
      jump: (place) => goToCompared(place),
    },
    opts,
  );
}

/**
 * Goes to a row picked on the compare screen, in the open budget it names
 * ({ doc, kind: 'category' | 'account' | 'row', ci, ai, rowId }; ci is the
 * category's place among the categories, ai the account's in it, one past
 * the last for a category's fringe account).
 */
async function goToCompared(place) {
  if (!place || typeof place !== 'object') return;
  if (grid.editor) grid.commitEdit(null);
  if (typeof place.doc === 'number' && place.doc !== state.doc) {
    await switchTo(place.doc);
    if (state.doc !== place.doc) return;
  }
  const c = state.ix && state.ix.cats.find((x) => x.ci === place.ci);
  if (!c) {
    toast('That is no longer in the budget.');
    return;
  }
  const n = (c.cat.accounts || []).length;
  if (place.kind === 'category') {
    openCategory(c.ti);
  } else if (place.ai >= n) {
    openCategory(c.ti, { focusKey: 'fa' });
  } else {
    await openAccount(c.ti, place.ai, place.kind === 'row' ? `r${place.rowId}` : null);
  }
  grid.focus();
}

for (const [id, cmd, label] of [
  ['btn-versions', 'file.versions', 'Versions: save, restore and compare named copies of this budget'],
  ['btn-compare', 'file.compare', 'Compare this budget with others, or with its versions'],
  ['btn-setup', 'view.setup', 'Budget Setup: every setup table, charges, credits and the budget settings, in a window of their own'],
]) {
  const b = $(id);
  if (!b) continue;
  b.title = withShortcut(label, cmd);
  b.addEventListener('click', () => COMMANDS[cmd]());
}

/** Commands that act on a focused text field (and so leave a cell edit open). */
const FIELD_COMMANDS = new Set(SHORTCUTS.filter((s) => s.field).map((s) => s.id));

// A key and its menu item can both arrive for one press (a native menu key
// that also reaches the page). The second one within this window is dropped.
const REPEAT_MS = 400;
let lastRun = { id: '', via: '', at: 0 };

function run(id, via) {
  const fn = COMMANDS[id];
  if (!fn) return;
  const now = performance.now();
  if (lastRun.id === id && lastRun.via !== via && now - lastRun.at < REPEAT_MS) {
    lastRun = { id: '', via: '', at: 0 };
    return;
  }
  lastRun = { id, via, at: now };
  if (grid.editor && !FIELD_COMMANDS.has(id)) {
    grid.commitEdit(null);
    // Fill down would read the cell before the edit lands; just finish the edit.
    if (id === 'edit.fill_down') return;
  }
  fn();
}

/** A key left to the browser (undo in a text field): a repeat from the menu is dropped. */
function leftToBrowser(id) {
  lastRun = { id, via: 'key', at: performance.now() };
}

function onMenu(id) {
  // Window > a budget: bring its tab to the front.
  if (typeof id === 'string' && id.startsWith('window.doc.')) {
    if (isModalOpen()) return;
    if (isHelpOpen()) closeHelp();
    switchTo(Number(id.slice('window.doc.'.length)));
    return;
  }
  if (typeof id !== 'string' || !COMMANDS[id]) return;
  if (isHelpOpen() && id !== 'help.shortcuts') closeHelp();
  // A dialog is open: only field commands (undo typing in it) make sense.
  if (isModalOpen() && !FIELD_COMMANDS.has(id)) return;
  // A grid command while a text field has focus: where menus see keys before
  // the page (Linux), the key was meant for the field (Ctrl+Backspace deletes a
  // word, not rows). The cell editor still takes the ones without a text meaning.
  const sc = shortcutById(id);
  if (sc && sc.scope === 'grid' && !sc.anywhere && isTextField(document.activeElement) && (sc.text || !grid.editor)) return;
  run(id, 'menu');
}

/**
 * Close Window and Quit from the keyboard on Windows and Linux (and Close Tab
 * on the last tab): closing the window goes through the same unsaved-changes
 * question in Rust, about every tab.
 */
function closeWindow() {
  const w = window.__TAURI__ && window.__TAURI__.window;
  if (w && typeof w.getCurrentWindow === 'function') {
    w.getCurrentWindow()
      .close()
      .catch((e) => toast(e));
  }
}

/**
 * Events for this window: menu commands (from the menu bar, or keys the tools
 * window passes on), and the tools window's news (src/tools.rs). Listened for
 * under this window's label, so events meant for the tools window, and this
 * window's own view changes, never arrive here.
 */
function listenToEvents() {
  const events = window.__TAURI__ && window.__TAURI__.event;
  if (!events || typeof events.listen !== 'function') return;
  const on = (name, fn) => events.listen(name, (e) => fn(e.payload), { target: 'main' }).catch((err) => toast(err));
  on('menu', onMenu);
  // Another window changed a budget ({doc, rev}): what is on screen is fetched
  // again (with its tab's dirty dot and every other tab).
  on('view-changed', () => refreshView());
  on('tools-ready', () => sendToolsState());
  on('tools-closed', () => {
    // Closed by its own close button (or Close Window): the panel docks back.
    if (state.position === 'window') setPosition('right', { fromTools: true });
  });
  on('tools-position', (where) => setPosition(where, { fromTools: true }));
  on('tools-layout', (layout) => {
    if (setToolsLayout(layout) && state.position !== 'window') renderPanel();
  });
  on('tools-geometry', saveGeometry);
  // The tools window's History tab (of the budget it drew): undo or redo to a
  // step, or save one as a version (history.js).
  on('tools-history', (p) => {
    if (!p || typeof p !== 'object' || p.doc !== state.doc) return;
    if (p.action === 'jump' && Number.isSafeInteger(p.to) && p.to >= 0) historyJump(p.to);
    else if (p.action === 'save' && Number.isSafeInteger(p.step) && p.step >= 0 && typeof p.name === 'string') {
      historySave(p.step, p.name).catch((e) => toast(e));
    }
  });
  // A noted line, from the tools window's Notes tab (of the budget it drew).
  on('tools-jump', (entry) => {
    if (entry && typeof entry === 'object' && 'doc' in entry && entry.doc !== state.doc) return;
    jumpTo(entry);
  });
  // The tools window's way to Budget Setup (its Setup buttons).
  on('tools-setup', (p) => {
    if (p && typeof p === 'object') openSetup(p.tab, p.item);
  });
  // The Budget Setup window (src/setupwin.rs): what to show once it listens,
  // the tab it shows, a line count clicked there, Escape there, its place,
  // and its closing (focus back to the grid).
  on('setup-ready', () => {
    emitTo('setup', 'setup-state', setupPending || { tab: readSetupTab(), item: null, focus: null });
    setupPending = null;
  });
  on('setup-tab', (tab) => {
    if (isSetupTab(tab)) writePref('setupTab', tab);
  });
  on('setup-find', findUsesFromSetup);
  on('setup-close', () => invoke('close_setup_window').catch(() => {}));
  on('setup-geometry', saveSetupGeometry);
  on('setup-closed', () => {
    setupPending = null;
    if (!isModalOpen()) grid.focus();
  });
}

// ---------------------------------------------------------------------------
// Keyboard

function onShortcutKey(e, sc, { editing, inField }) {
  if (sc.scope === 'app') {
    if (menuRunsAppKeys) return; // macOS: the menu item's key equivalent runs it
    if (inField && sc.field) {
      leftToBrowser(sc.id); // the field's own undo, redo or select all
      return;
    }
    e.preventDefault();
    run(sc.id, 'key');
    return;
  }
  // Grid commands. Keys that move the caret or delete in a text field stay there.
  if (inField && sc.text) return;
  // Cancelling also keeps the key from reaching the menu bar a second time.
  e.preventDefault();
  // A setup or dialog field: not meant for the grid (tab keys work anywhere).
  if (inField && !editing && !sc.anywhere) return;
  run(sc.id, 'key');
}

function onKeyDown(e) {
  if (isHelpOpen() || isModalOpen()) return; // they handle their own keys
  const active = document.activeElement;
  const editing = Boolean(grid.editor);
  const inField = isTextField(active);
  // A button in the grid's header row (the column menu) is not the grid.
  const inGrid = P().host.contains(active) && !(active.closest && active.closest('thead'));
  const bodyFocused = !active || active === document.body;

  // The cell editor answers its own keys first: Enter, Tab, Escape, arrows,
  // and Cmd/Ctrl+Enter (same value into every selected row).
  if (editing && inGrid && grid.handleKey(e)) {
    e.preventDefault();
    return;
  }

  const sc = matchShortcut(e);
  if (sc) {
    onShortcutKey(e, sc, { editing, inField });
    return;
  }

  // Escape on a setup panel tab or button, a breadcrumb button or the column
  // menu button goes back to the grid.
  const chrome =
    $('panel').contains(active) ||
    panes.some((p) => p && (p.crumbs.contains(active) || p.tabbar.contains(active))) ||
    (active && active.closest && active.closest('.grid-host thead'));
  if (e.key === 'Escape' && !inField && chrome) {
    e.preventDefault();
    grid.focus();
    return;
  }

  if (inField) return; // text fields handle their own keys
  if (inGrid || bodyFocused) {
    if (bodyFocused) grid.focus();
    if (grid.handleKey(e)) e.preventDefault();
  }
}

// ---------------------------------------------------------------------------
// Split view (roadmap 2.26; pane.js, src/docs.rs): two panes side by side,
// each with its own tabs, breadcrumb, grid and footer. Rust keeps which tabs
// are in which pane and which pane is the target (every view's `panes`);
// this keeps where each pane is. The target is the pane last clicked or
// focused: the toolbar, the menus, the keys, the tools panel and the formula
// bar act on it. A budget can be in both panes: one session, two views, so
// an edit in one pane shows in the other (which fetches its own view, at its
// own account: pane_view).

// The window's panes as the latest view gave them (src/docs.rs, PanesInfo).
let layout = { split: false, target: 0, panes: [{ tabs: [], front: null }] };
// set_target_pane calls on their way: until they are answered, views from
// earlier commands don't move the target back.
let targetCalls = 0;

function isSplit() {
  return layout.split && Boolean(panes[1]);
}

/** Budget ids in `pane`'s strip, in order. */
function paneTabs(pane) {
  if (!isSplit()) return state.tabs.map((t) => t.id);
  const info = layout.panes[pane.index];
  return info ? info.tabs.slice() : [];
}

/** The budget in front in `pane`. */
function paneFront(pane) {
  if (!isSplit()) {
    const t = state.tabs.find((x) => x.active);
    return t ? t.id : null;
  }
  const info = layout.panes[pane.index];
  return info ? info.front : null;
}

/** Each pane's strip draws its own tabs, its budget in front marked. */
function drawStrips() {
  for (const p of panes) {
    if (!p || !p.strip) continue;
    if (p.index === 1 && !isSplit()) continue;
    const front = paneFront(p);
    const byId = new Map(state.tabs.map((t) => [t.id, t]));
    const list = paneTabs(p)
      .map((id) => byId.get(id))
      .filter(Boolean)
      .map((t) => ({ ...t, active: t.id === front }));
    p.strip.render(list);
  }
}

/** The second pane, made the first time the window splits. */
function ensureSecondPane() {
  if (panes[1]) return panes[1];
  const p = secondPane();
  p.grid = makeGrid(p);
  p.grid.setZoom(userDisplay().zoom);
  p.grid.setShowFormulas(userDisplay().formulasInSelection);
  p.strip = createTabStrip(p.tabbar, stripHandlers(p));
  watchContextMenu(p.host);
  panes[1] = p;
  return p;
}

/**
 * Takes the panes a view gives (split or not, each pane's tabs and front,
 * the target) and returns the pane to draw the view in, or null when no
 * pane shows its budget in front.
 */
function routeView(view, opts) {
  const info = view.panes;
  if (info && Array.isArray(info.panes) && info.panes.length) setLayout(info);
  drawStrips();
  // A pane's own view (pane_view), or an account it asked for.
  if (opts.pane) return paneFront(opts.pane) === view.doc || opts.pane.st.doc === null ? opts.pane : null;
  const front = paneFront(target);
  return front === null || front === view.doc ? target : null;
}

/** Lays the window out as `info` says (two panes or one) and follows its target. */
function setLayout(info) {
  const was = isSplit();
  layout = info;
  const split = Boolean(info.split);
  if (split && !was) {
    const second = ensureSecondPane();
    layoutPanes(panes[0], second);
  } else if (!split && was) {
    mergePanes();
  }
  // The target, unless a click has just picked another one (its call is on
  // the way).
  const t = split ? panes[info.target] || panes[0] : panes[0];
  if (t !== target && (targetCalls === 0 || !split)) adoptTarget(t);
  markTarget(panes, target);
}

/**
 * Back to one pane: the first pane carries on, showing the target's budget
 * where the target pane had it. Every budget comes back where it was last
 * seen: where a pane shows it now (the target's view first), else where the
 * target pane, then the other one, left it.
 */
function mergePanes() {
  const [first, second] = panes;
  const fromSecond = target === second;
  const now = (p) => (p && p.st.doc !== null ? [p.st.doc, withPane(p, () => whereHere())] : null);
  const order = fromSecond ? [first, second] : [second, first]; // the later wins
  const merged = new Map();
  for (const p of order) if (p) for (const [doc, m] of p.tabMemory) merged.set(doc, m);
  for (const p of order) {
    const n = now(p);
    if (n) merged.set(n[0], n[1]);
  }
  first.tabMemory = merged;
  // The first pane takes up the target's budget from there.
  first.st.doc = null;
  layoutPanes(first, null);
  if (second) {
    second.tabMemory.clear();
    Object.assign(second.st, { doc: null, view: null, ix: null, level: 'topsheet', cat: null, acct: null, selection: null, memory: {}, changes: null });
  }
  adoptTarget(first);
}

/**
 * A budget just came to the front in the pane in use, remembered on an
 * account other than the one `view` carries (another pane moved the
 * budget's account on screen): asks for the view with its own account (the
 * target moves Rust's account on screen, the other pane only looks) and
 * returns true; it is drawn when that arrives.
 */
function refocus(view, opts) {
  const s = state;
  if (s.level !== 'details' || !s.acct || !s.cat) return false;
  const a = view.account;
  if (a && a.code === s.acct.code && a.ti === s.cat.ti) return false;
  const pane = P();
  const args = { doc: s.doc, category: s.cat.code, ti: s.cat.ti, account: s.acct.code, ai: s.acct.ai };
  const cmd = pane === target ? 'focus_account' : 'pane_view';
  pane.refocusing = true;
  invoke(cmd, args)
    .then((v) => {
      pane.refocusing = false;
      if (v && pane.st.doc === v.doc) apply(v, { ...opts, cmd, pane, refocused: true });
    })
    .catch((e) => {
      pane.refocusing = false;
      toast(e);
    });
  return true;
}

/** Makes `p` the target without telling Rust (it told us). */
function adoptTarget(p) {
  const old = target;
  if (cur === old) cur = p;
  target = p;
  markTarget(panes, target);
  moveFormulaBar(target);
}

/**
 * Makes `p` the target (a click or focus in it): the toolbar, the tools
 * panel and the formula bar follow it, and Rust brings its budget forward
 * with its account on screen.
 */
function setTarget(p) {
  if (!p || p === target || !isSplit()) return;
  const old = target;
  if (old.grid && old.grid.editor) withPane(old, () => old.grid.commitEdit(null));
  closeMenu();
  adoptTarget(p);
  cur = p;
  if (state.view) {
    renderTitle(state.view);
    // The one share banner follows the target's budget, as the title does.
    renderShareBanner(state.view, sharingCtx());
    renderPanel();
    showFormula(grid.cursorCell());
    document.dispatchEvent(new CustomEvent('fwb:selection'));
    document.dispatchEvent(new CustomEvent('fwb:cursor'));
    broadcastSelection();
  }
  targetCalls += 1;
  invoke('set_target_pane', { pane: p.index, ...accountArgs(p) })
    .then((v) => {
      targetCalls -= 1;
      if (v) apply(v, { cmd: 'set_target_pane', quiet: true });
    })
    .catch((e) => {
      targetCalls -= 1;
      toast(e);
    });
}

/** The account `p` shows, as focus_account names it (nulls: none). */
function accountArgs(p) {
  const s = p.st;
  if (s.level === 'details' && s.acct && s.cat) return { category: s.cat.code, ti: s.cat.ti, account: s.acct.code, ai: s.acct.ai };
  return { category: null, ti: null, account: null, ai: null };
}

// The other pane's views on their way, so changes arriving meanwhile fetch once more.
const following = new Map(); // pane -> { busy, again }

/**
 * After a view of a budget is drawn in `pane`: the other pane catches up
 * when it shows the same budget (with this view when it shows no account or
 * the same one, else with its own view), and fetches its own budget's view
 * when what is in front there changed (a tab moved or closed).
 */
function followOtherPane(pane, view) {
  if (!isSplit()) return;
  for (const p of panes) {
    if (!p || p === pane) continue;
    const front = paneFront(p);
    if (front === null) continue;
    if (p.st.doc !== front) {
      fetchPaneView(p);
      continue;
    }
    if (front !== view.doc || !p.st.view || p.st.view.rev === view.rev) continue;
    const a = p.st.acct;
    const shown = view.account;
    const same = p.st.level !== 'details' || (a && shown && shown.code === a.code && p.st.cat && shown.ti === p.st.cat.ti);
    if (same) withPane(p, () => applyHere(view, { cmd: 'view-changed' }));
    else fetchPaneView(p);
  }
}

/** Fetches `p`'s budget in front with `p`'s own account (pane_view), once at a time. */
async function fetchPaneView(p) {
  const f = following.get(p) || { busy: false, again: false };
  following.set(p, f);
  if (f.busy) {
    f.again = true;
    return;
  }
  f.busy = true;
  try {
    do {
      f.again = false;
      const doc = paneFront(p);
      if (doc === null || p === target) break;
      // Where it was left (a budget coming to the front there), or where it is.
      const m = p.st.doc === doc ? null : p.tabMemory.get(doc);
      const at = p.st.doc === doc ? p : { st: m ? { level: m.level, cat: m.cat, acct: m.acct } : { level: 'topsheet' } };
      try {
        const v = await invoke('pane_view', { doc, ...accountArgs(at) });
        if (v && p !== target) apply(v, { cmd: p.st.doc === doc ? 'view-changed' : 'pane_view', pane: p });
      } catch (e) {
        toast(e);
      }
    } while (f.again);
  } finally {
    f.busy = false;
  }
}

/** Redraws every pane (a display choice or the columns changed). */
function renderAll() {
  for (const p of panes) {
    if (!p || !p.st.view) continue;
    if (p.index === 1 && !isSplit()) continue;
    withPane(p, () => render());
  }
}

/** What a pane's tab strip does (tabs.js). */
function stripHandlers(pane) {
  return {
    switchTo: (id, opts) => switchTo(id, { ...opts, pane }),
    close: (id) => closeTab(id, pane),
    add: () => {
      setTarget(pane);
      doNew();
    },
    move: (id, to) => moveTab(id, to, pane),
    moveToOther: (id, to) => moveToOtherPane(id, pane, to),
    menu: (id, at) => tabMenu(id, pane, at),
  };
}

/** View > Split Right: a second pane showing the budget in front too, where this pane is. */
async function splitRight() {
  if (isSplit()) {
    // Split already: the other pane becomes the target.
    const other = panes[1 - target.index];
    setTarget(other);
    grid.focus();
    return;
  }
  if (grid.editor) grid.commitEdit(null);
  const second = ensureSecondPane();
  if (state.doc !== null) second.tabMemory.set(state.doc, whereHere());
  await act('split_view');
  grid.focus();
}

/** View > Close Split: every tab back in one strip. */
async function closeSplit() {
  if (!isSplit()) return;
  if (grid.editor) grid.commitEdit(null);
  await act('close_split');
  grid.focus();
}

/**
 * Moves budget `id`'s tab from `from` to the other pane (at place `to`
 * there, or the end), where it comes to the front, where it was left.
 */
async function moveToOtherPane(id, from = target, to = null) {
  if (!isSplit()) return;
  const into = panes[1 - from.index];
  if (paneTabs(from).length < 2) {
    toast("A pane's only tab can't move to the other pane.");
    return;
  }
  if (grid.editor) grid.commitEdit(null);
  // It comes up in the other pane where it was in this one.
  const m = from.st.doc === id ? withPane(from, () => whereHere()) : from.tabMemory.get(id);
  if (m && into.st.doc !== id) into.tabMemory.set(id, m);
  await act('move_to_pane', { doc: id, from: from.index, to: typeof to === 'number' ? to : null });
  from.tabMemory.delete(id);
  grid.focus();
}

/** A tab's right-click menu: close it, move it to the other pane, split or merge. */
function tabMenu(id, pane, at) {
  const lone = paneTabs(pane).length < 2;
  const items = [{ label: 'Close Tab', shortcut: shortcutText('file.close_tab'), run: () => closeTab(id, pane) }, { separator: true }];
  if (isSplit()) {
    items.push({
      label: 'Move to Other Pane',
      title: lone ? "A pane's only tab can't move" : 'Show this budget in the other pane instead',
      disabled: lone,
      run: () => moveToOtherPane(id, pane),
    });
    items.push({ label: 'Close Split', run: () => closeSplit() });
  } else {
    items.push({
      label: 'Split Right',
      shortcut: shortcutText('view.split_right'),
      title: 'Show this budget in a second pane at the right too',
      run: async () => {
        if (id !== state.doc) await switchTo(id, { pane });
        if (state.doc === id) splitRight();
      },
    });
  }
  openContextMenu(at, { label: 'Tab actions', items, returnFocus: () => pane.strip.focus(id) });
}

// The History tab (history.js): every undo step of the target's budget.

/** Undoes or redoes the target's budget to `to` steps applied. */
async function historyJump(to) {
  if (grid.editor) grid.commitEdit(null);
  await act('history_jump', { to });
}

/** Saves the target's budget as it was at step `step` as version `name`; rejects with a message. */
function historySave(step, name) {
  return call('history_save_version', { step, name, note: null, created: localStamp() });
}

/** A click or focus in a pane makes it the target. */
function wireSplit() {
  const pick = (e) => {
    if (!isSplit()) return;
    const p = panes.find((x) => x && x.contains(e.target));
    if (p && p !== target) setTarget(p);
  };
  document.addEventListener('pointerdown', pick, true);
  document.addEventListener('focusin', pick, true);
}

Object.assign(COMMANDS, {
  'view.split_right': () => splitRight(),
  'view.close_split': () => closeSplit(),
  'window.move_to_other_pane': () => {
    if (state.doc !== null) moveToOtherPane(state.doc, target);
  },
});

// ---------------------------------------------------------------------------
// Start

function wire() {
  wirePanelResizer({ position: () => state.position, focusGrid: () => grid.focus() });
  panes[0].strip = createTabStrip(panes[0].tabbar, stripHandlers(panes[0]));
  wireSplit();
  $('btn-new').addEventListener('click', doNew);
  $('btn-open').addEventListener('click', doOpen);
  $('btn-save').addEventListener('click', () => doSave(false));
  $('btn-saveas').addEventListener('click', () => doSave(true));
  $('btn-undo').addEventListener('click', doUndo);
  $('btn-redo').addEventListener('click', doRedo);
  $('btn-panel').addEventListener('click', togglePanel);
  const tips = {
    'btn-new': ['New budget, in a new tab', 'file.new'],
    'btn-open': ['Open a budget, in a new tab', 'file.open'],
    'btn-save': ['Save', 'file.save'],
    'btn-saveas': ['Save as a new file', 'file.save_as'],
    'btn-undo': ['Undo', 'edit.undo'],
    'btn-redo': ['Redo', 'edit.redo'],
    'btn-panel': ['Show or hide the tools panel', 'view.toggle_panel'],
  };
  for (const [id, [label, cmd]] of Object.entries(tips)) $(id).title = withShortcut(label, cmd);
  window.addEventListener('keydown', onKeyDown);
  wireClipboard();
  listenToEvents();
}

async function start() {
  wire();
  if (!connected) {
    $('banner').hidden = false;
    $('banner').replaceChildren(el('strong', { text: 'Open this page in the Method Budget app.' }));
    return;
  }
  try {
    apply(await invoke('get_view'), { focusKey: 't0', fresh: true });
  } catch (e) {
    toast(e);
  }
  grid.focus();
  // Popped out last time: the tools window opens where it was.
  if (state.position === 'window' && state.panelOpen) openToolsWindow(true).then(sendToolsState);
  await safety.start(); // a file to open, budgets to recover
  await updates.start(); // where an update is; What's New, once after one
}

start();
