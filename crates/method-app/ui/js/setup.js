// Setup tables drawn in place: globals and units, and the kit of helpers the
// budget's own setup tabs use (topsheet-setup.js: charges, credits, info,
// settings, baseline). Fringes, groups, sets, locations and currencies are
// lists in apply.js.
//
// The Budget Setup window (setupwin.js) draws every one of these in full:
// Globals and Units as one dense table each (setupframe.js): a row per item
// with its fields edited in place, an action strip above (New, Delete and
// Save to Library for the selected row, a global's Details…, the search),
// the footer's count. The tools panel (tools.js) draws Globals and Units
// compact (renderSetupTab's `compact`): each global's name, formula and
// value, each unit's name, plural and hours, with renaming, deleting and
// adding kept, while a global's details form and a unit's fields are left to
// Budget Setup (a Setup button, and Edit in Budget Setup on a name's
// right-click menu, open it there: ctx.openSetup).
//
// tools.js draws the whole panel on every view: setupBegin() before, then
// renderSetupTab() for each of these tabs on screen, then setupEnd() swaps
// the new panel in and puts focus back. Budget text only ever reaches the
// page through textContent and input values (never innerHTML), and styling is
// done with classes from setup.css.
//
// A global's or unit's name renames it everywhere (rename.js): double-click
// it, or Enter or F2 on it, or its right-click menu.
//
// Edits commit on Enter or blur and Escape puts the old value back. Commands run
// one at a time through a queue, and each one is built from the newest view when
// it runs, so two quick edits to the same item never undo each other.

import { renderExtraTab } from './topsheet-setup.js';
import { anchoredForm } from './modal.js';
import { renameItem, itemMenu } from './rename.js';
import {
  wantUsage,
  listNames,
  canReorder,
  sortButton,
  showSelect,
  usesButton,
  usageOf,
  dragHandle,
  moveKey,
  MOVE_KEYS,
  inUse,
  redrawLists,
} from './setuplist.js';
import { globalValueText } from './display.js';
import { globalInfoCell, globalDetails } from './display-settings.js';
import { saveItemToLibrary } from './library.js';
import { mountFrame, actionStrip, stripButton, searchBox, gridTable, setFooterTotals } from './setupframe.js';
import { str, plural, errText, sourceText } from './util.js';

// Letters, digits and _, with at least one letter or _ (decisions.md, entry 56).
const GLOBAL_NAME = /^(?=[A-Za-z0-9_]*[A-Za-z_])[A-Za-z0-9_]+$/;
const DECIMAL = /^[+-]?(\d+\.?\d*|\.\d+)$/;

// State that has to survive re-renders.
const state = {
  container: null,
  view: null,
  ctx: null,
  queue: Promise.resolve(),
  pending: 0,
  // Control to focus on the next render, set by our own actions.
  focusKey: null,
  // True while the old DOM is swapped out. Some engines fire blur on a removed
  // focused input; that must not commit a half-typed edit.
  swapping: false,
  // Add-form text per tab, so a re-render never wipes what is being typed.
  drafts: {},
  // Rejected-call messages by item key (or "add" key), shown until the next try.
  errors: new Map(),
  // Tabs shown last time; showing other tabs drops old error messages.
  tabs: '',
  shown: [],
  // Budget Setup's Globals and Units: the selected row and the search, per table.
  selected: {},
  filters: {},
};

let uid = 0;

// True while drawing a compact tab (the tools panel's Globals and Units).
let compact = false;

/**
 * Starts drawing the panel into `container` (the element that stays on the
 * page): notes where focus is so setupEnd() can put it back.
 */
export function setupBegin(container, view, ctx) {
  const focus = captureFocus(container);
  state.container = container;
  state.view = view && typeof view === 'object' ? view : {};
  state.ctx = ctx;
  state.shown = [];
  uid = 0;
  return focus;
}

/**
 * Draws setup tab `tab` (globals, units, or one of the budget's own setup
 * tabs) into `panel`; with `compact`, the tools panel's short form of
 * Globals or Units.
 */
export function renderSetupTab(panel, tab, opts = {}) {
  state.shown.push(tab);
  panel.classList.add('setup-panel');
  compact = Boolean(opts && opts.compact) && (tab === 'globals' || tab === 'units');
  if (compact) panel.classList.add('setup-compact');
  try {
    if (tab === 'globals') renderGlobals(panel);
    else if (tab === 'units') renderUnits(panel);
    else renderExtraTab(tab, panel, kit());
  } finally {
    compact = false;
  }
}

/** The way to `table` (and item `name`) in Budget Setup, when this window has one. */
function canOpenSetup() {
  return Boolean(state.ctx && typeof state.ctx.openSetup === 'function');
}

/** Swaps the newly drawn panel `root` in and restores focus. */
export function setupEnd(container, root, focus) {
  const tabs = state.shown.join(' ');
  if (tabs !== state.tabs) state.errors.clear();
  state.tabs = tabs;
  // One swap, so a scrolled parent keeps its position.
  state.swapping = true;
  try {
    container.replaceChildren(root);
  } finally {
    state.swapping = false;
  }
  applyBusy();
  restoreFocus(container, focus);
}

// ---------------------------------------------------------------------------
// Small helpers

function el(tag, cls, text) {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text !== undefined && text !== null) e.textContent = String(text);
  return e;
}

function key(...parts) {
  return JSON.stringify(parts);
}

function nextId() {
  uid += 1;
  return `setup-f${uid}`;
}

function own(obj, k) {
  if (obj === null || typeof obj !== 'object') return undefined;
  return Object.prototype.hasOwnProperty.call(obj, k) ? obj[k] : undefined;
}

function byteOrder(a, b) {
  return a < b ? -1 : a > b ? 1 : 0;
}

function entries(map) {
  if (!map || typeof map !== 'object' || Array.isArray(map)) return [];
  return Object.keys(map)
    .sort(byteOrder)
    .map((k) => [k, map[k]]);
}

function budget() {
  const b = state.view && state.view.budget;
  return b && typeof b === 'object' ? b : {};
}

function report() {
  const r = state.view && state.view.report;
  return r && typeof r === 'object' ? r : null;
}

function baseCode() {
  return str(budget().base_currency);
}

function issuesFor(kind, name) {
  const list = state.view && state.view.issues;
  if (!Array.isArray(list)) return [];
  return list.filter((i) => {
    const t = i && i.target;
    if (!t || t.kind !== kind) return false;
    const n = t.name !== undefined ? t.name : t.code;
    return n === name;
  });
}

function groupDigits(digits) {
  return digits.replace(/\B(?=(\d{3})+(?!\d))/g, ',');
}

// Cents (integer) to "1,234.56", negatives in parentheses. '' when not money.
function formatCents(c) {
  if (typeof c === 'number' && !Number.isInteger(c)) return '';
  let n;
  try {
    n = BigInt(c);
  } catch {
    return '';
  }
  const neg = n < 0n;
  if (neg) n = -n;
  const s = `${groupDigits((n / 100n).toString())}.${(n % 100n).toString().padStart(2, '0')}`;
  return neg ? `(${s})` : s;
}

// Cents to plain money text for commands, like "-1234.56".
function centsToText(c) {
  let n;
  try {
    n = BigInt(c);
  } catch {
    return '';
  }
  const neg = n < 0n;
  if (neg) n = -n;
  return `${neg ? '-' : ''}${(n / 100n).toString()}.${(n % 100n).toString().padStart(2, '0')}`;
}

// Decimal string with its whole part grouped: "1234.5" to "1,234.5".
function formatDecimal(s) {
  const m = /^(-?)(\d+)(\.\d+)?$/.exec(str(s));
  if (!m) return str(s);
  return `${m[1]}${groupDigits(m[2])}${m[3] || ''}`;
}

function findByData(attr, value) {
  if (!state.container) return null;
  for (const e of state.container.querySelectorAll(`[data-${attr}]`)) {
    if (e.dataset[attr] === value) return e;
  }
  return null;
}

// ---------------------------------------------------------------------------
// Focus across re-renders

function captureFocus(container) {
  const explicit = state.focusKey;
  state.focusKey = null;
  const a = document.activeElement;
  const inside = a && container.contains(a) && a !== container;
  const nowhere = !a || a === document.body || a === document.documentElement;
  // Never pull focus out of another part of the app.
  if (!inside && !nowhere) return null;

  if (explicit) return { key: explicit };
  if (inside && a.dataset && a.dataset.key) {
    const info = { key: a.dataset.key };
    if (a.dataset.orig !== undefined && a.value !== a.dataset.orig) info.value = a.value;
    if (typeof a.selectionStart === 'number') {
      info.start = a.selectionStart;
      info.end = a.selectionEnd;
    }
    return info;
  }
  return null;
}

function restoreFocus(container, info) {
  if (!info) return;
  let target = findByData('key', info.key);
  if (!target) target = container.querySelector('[role="tab"][aria-selected="true"]');
  if (!target) return;
  if (info.value !== undefined && 'value' in target) target.value = info.value;
  target.focus({ preventScroll: true });
  if (info.start !== undefined && typeof target.setSelectionRange === 'function') {
    try {
      target.setSelectionRange(info.start, info.end);
    } catch {
      // Not a text control; nothing to restore.
    }
  }
}

// ---------------------------------------------------------------------------
// Running commands

function isBusy() {
  return state.pending > 0;
}

// Buttons are marked aria-disabled rather than disabled while a call runs, so
// they keep keyboard focus; their handlers check isBusy() themselves.
function applyBusy() {
  const c = state.container;
  if (!c) return;
  const busy = isBusy();
  for (const panel of c.querySelectorAll('.setup-panel')) panel.setAttribute('aria-busy', busy ? 'true' : 'false');
  for (const b of c.querySelectorAll('button')) {
    if (b.dataset.nobusy) continue;
    if (busy) b.setAttribute('aria-disabled', 'true');
    else b.removeAttribute('aria-disabled');
  }
}

function showError(itemKey, message) {
  const item = findByData('item', itemKey);
  if (!item) return;
  let p = item.querySelector(':scope > .setup-error');
  if (!p) {
    p = errorLine();
    item.append(p);
  }
  p.textContent = message;
}

function clearError(itemKey) {
  state.errors.delete(itemKey);
  const item = findByData('item', itemKey);
  const p = item && item.querySelector(':scope > .setup-error');
  if (p) p.remove();
}

function errorLine(message) {
  const p = el('p', 'setup-error', message);
  p.setAttribute('role', 'alert');
  return p;
}

function restoreField(fieldKey, old) {
  const f = findByData('key', fieldKey);
  if (!f) return;
  if (f.type === 'checkbox') {
    f.checked = Boolean(old);
  } else {
    f.value = old;
    f.dataset.orig = old;
  }
}

// Queues a command. `build` runs when the command's turn comes, against the
// newest view, and returns [command, args].
function run(itemKey, build, opts = {}) {
  clearError(itemKey);
  state.queue = state.queue.then(async () => {
    state.pending += 1;
    applyBusy();
    try {
      const [cmd, args] = build();
      if (!state.ctx || typeof state.ctx.call !== 'function') throw new Error('Not connected');
      await state.ctx.call(cmd, args);
      if (opts.onOk) opts.onOk();
    } catch (e) {
      const message = errText(e);
      state.errors.set(itemKey, message);
      if (opts.restore) restoreField(opts.restore[0], opts.restore[1]);
      if (opts.onError) opts.onError();
      showError(itemKey, message);
    } finally {
      state.pending -= 1;
      applyBusy();
    }
  });
}

// ---------------------------------------------------------------------------
// Controls

// `cls`: more classes (sw-cell-input draws it flat in a table cell).
function textInput({ fieldKey, value, onCommit, num = false, placeholder = '', label, cls = '' }) {
  const input = el('input', `${num ? 'setup-input setup-num' : 'setup-input'}${cls ? ` ${cls}` : ''}`);
  input.type = 'text';
  input.id = nextId();
  input.value = value;
  input.dataset.orig = value;
  input.dataset.key = fieldKey;
  input.autocomplete = 'off';
  input.spellcheck = false;
  input.setAttribute('autocapitalize', 'off');
  input.setAttribute('autocorrect', 'off');
  if (placeholder) input.placeholder = placeholder;
  if (label) input.setAttribute('aria-label', label);

  const commit = () => {
    if (state.swapping) return;
    const v = input.value;
    const old = input.dataset.orig;
    if (v === old) return;
    input.dataset.orig = v;
    onCommit(v, old);
  };
  input.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && !e.isComposing) {
      e.preventDefault();
      commit();
    } else if (e.key === 'Escape' && input.value !== input.dataset.orig) {
      e.preventDefault();
      e.stopPropagation();
      input.value = input.dataset.orig;
    }
  });
  input.addEventListener('blur', commit);
  return input;
}

function selectInput({ fieldKey, value, options, onCommit, label = '', cls = '' }) {
  const sel = el('select', `setup-input setup-select${cls ? ` ${cls}` : ''}`);
  sel.id = nextId();
  sel.dataset.key = fieldKey;
  if (label) sel.setAttribute('aria-label', label);
  for (const [v, label] of options) {
    const o = el('option', null, label);
    o.value = v;
    sel.append(o);
  }
  sel.value = value;
  sel.dataset.orig = value;
  sel.addEventListener('change', () => {
    const v = sel.value;
    const old = sel.dataset.orig;
    if (v === old) return;
    sel.dataset.orig = v;
    onCommit(v, old);
  });
  return sel;
}

// A labelled cell in a fields grid. `extra` is a trailing unit like "%".
function field(labelText, control, { wide = false, extra = '' } = {}) {
  const wrap = el('div', wide ? 'setup-field setup-field-wide' : 'setup-field');
  const label = el('label', 'setup-label', labelText);
  label.htmlFor = control.id;
  wrap.append(label);
  if (extra) {
    const row = el('div', 'setup-adorned');
    const tag = el('span', 'setup-adorn', extra);
    tag.setAttribute('aria-hidden', 'true');
    row.append(control, tag);
    wrap.append(row);
  } else {
    wrap.append(control);
  }
  return wrap;
}

function hiddenLabel(text, control) {
  const label = el('label', 'setup-sr', text);
  label.htmlFor = control.id;
  return label;
}

// Item shell: an li carrying the item key, with any rejected-call message.
function itemShell(itemKey, cls) {
  const li = el('li', `setup-item ${cls}`);
  li.dataset.item = itemKey;
  return li;
}

function finishItem(li, itemKey, issues, describe) {
  if (issues.length) {
    li.classList.add('setup-item-bad');
    for (const i of issues) {
      const p = el('p', 'setup-issue', str(i.message));
      p.id = nextId();
      li.append(p);
      if (describe) {
        describe.setAttribute('aria-invalid', 'true');
        const prior = describe.getAttribute('aria-describedby');
        describe.setAttribute('aria-describedby', prior ? `${prior} ${p.id}` : p.id);
      }
    }
  }
  if (state.errors.has(itemKey)) li.append(errorLine(state.errors.get(itemKey)));
}

// The delete button and its inline confirmation.
function deleteButton({ itemKey, name, what, confirmText, build, neighborKey, confirmLabel = 'Delete' }) {
  const btn = el('button', 'setup-del', '×');
  btn.type = 'button';
  btn.dataset.key = key('del', itemKey);
  btn.setAttribute('aria-label', `Delete ${what} ${name}`);
  btn.title = `Delete ${what}`;
  btn.addEventListener('click', () => {
    if (isBusy()) return;
    const li = findByData('item', itemKey);
    if (!li) return;
    const open = li.querySelector(':scope > .setup-confirm');
    if (open) {
      open.querySelector('.setup-btn-danger').focus();
      return;
    }
    const box = el('div', 'setup-confirm');
    box.setAttribute('role', 'group');
    box.setAttribute('aria-label', `Delete ${what} ${name}`);
    const text = el('p', 'setup-confirm-text', confirmText);
    text.id = nextId();
    const yes = el('button', 'setup-btn setup-btn-danger', confirmLabel);
    yes.type = 'button';
    yes.dataset.key = key('del-yes', itemKey);
    yes.setAttribute('aria-describedby', text.id);
    const no = el('button', 'setup-btn setup-btn-quiet', 'Cancel');
    no.type = 'button';
    no.dataset.key = key('del-no', itemKey);
    const close = () => {
      box.remove();
      btn.focus();
    };
    no.addEventListener('click', close);
    box.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        close();
      }
    });
    yes.addEventListener('click', () => {
      if (isBusy()) return;
      state.focusKey = neighborKey;
      run(itemKey, build, {
        onError: () => {
          state.focusKey = null;
          const b = findByData('key', btn.dataset.key);
          const stale = findByData('item', itemKey);
          const c = stale && stale.querySelector(':scope > .setup-confirm');
          if (c) c.remove();
          if (b) b.focus();
        },
      });
    });
    const actions = el('div', 'setup-confirm-actions');
    actions.append(yes, no);
    box.append(text, actions);
    li.append(box);
    yes.focus();
  });
  return btn;
}

// Where focus goes after deleting item i: the next item, else the previous,
// else the add form.
function neighbor(tab, names, i, addKey) {
  const n = names[i + 1] !== undefined ? names[i + 1] : names[i - 1];
  return n !== undefined ? key('del', key(tab, n)) : addKey;
}

// ---------------------------------------------------------------------------
// Lists

function list(label) {
  const ul = el('ul', 'setup-list');
  ul.setAttribute('aria-label', label);
  return ul;
}

function emptyNote(text) {
  return el('p', 'setup-empty', text);
}

// ---------------------------------------------------------------------------
// Add forms

function draft(tab) {
  if (!state.drafts[tab]) state.drafts[tab] = {};
  return state.drafts[tab];
}

function addInput(tab, name, { label, placeholder = '', num = false, hintId = '' }) {
  const input = el('input', num ? 'setup-input setup-num' : 'setup-input');
  input.type = 'text';
  input.id = nextId();
  input.name = name;
  input.dataset.key = key('add', tab, name);
  input.value = str(draft(tab)[name]);
  input.autocomplete = 'off';
  input.spellcheck = false;
  input.setAttribute('autocapitalize', 'off');
  input.setAttribute('autocorrect', 'off');
  if (placeholder) input.placeholder = placeholder;
  if (hintId) input.setAttribute('aria-describedby', hintId);
  input.addEventListener('input', () => {
    draft(tab)[name] = input.value;
  });
  return field(label, input);
}

function addSelect(tab, name, label, options) {
  const sel = el('select', 'setup-input setup-select');
  sel.id = nextId();
  sel.name = name;
  sel.dataset.key = key('add', tab, name);
  for (const [v, text] of options) {
    const o = el('option', null, text);
    o.value = v;
    sel.append(o);
  }
  const d = draft(tab)[name];
  sel.value = d !== undefined ? d : options[0][0];
  sel.addEventListener('change', () => {
    draft(tab)[name] = sel.value;
  });
  return field(label, sel);
}

// Builds the add form. `validate(values)` returns [fieldName, message] or null;
// `build(values)` returns [command, args].
function addForm(tab, { title, fields, hint, validate, build }) {
  const errKey = key('add', tab);
  const form = el('form', 'setup-add');
  form.noValidate = true;
  form.dataset.item = errKey;
  form.setAttribute('aria-label', title);
  const head = el('p', 'setup-add-title', title);
  head.setAttribute('aria-hidden', 'true');
  const grid = el('div', 'setup-fields');
  grid.append(...fields);
  const submit = el('button', 'setup-btn setup-btn-primary', 'Add');
  submit.type = 'submit';
  submit.dataset.key = key('add-submit', tab);
  const actions = el('div', 'setup-add-actions');
  if (hint) actions.append(hint);
  actions.append(submit);
  form.append(head, grid, actions);
  if (state.errors.has(errKey)) form.append(errorLine(state.errors.get(errKey)));

  form.addEventListener('submit', (e) => {
    e.preventDefault();
    if (isBusy()) return;
    const values = {};
    for (const c of form.querySelectorAll('input, select')) values[c.name] = c.value;
    for (const c of form.querySelectorAll('[aria-invalid]')) c.removeAttribute('aria-invalid');
    const bad = validate(values);
    if (bad) {
      const [name, message] = bad;
      state.errors.set(errKey, message);
      showError(errKey, message);
      const c = form.querySelector(`[name="${name}"]`);
      if (c) {
        c.setAttribute('aria-invalid', 'true');
        c.focus();
      }
      return;
    }
    state.focusKey = key('add', tab, form.querySelector('input, select').name);
    run(errKey, () => build(values), {
      onOk: () => {
        state.drafts[tab] = {};
        const f = findByData('item', errKey);
        if (f) {
          for (const c of f.querySelectorAll('input')) c.value = '';
          for (const c of f.querySelectorAll('select')) c.selectedIndex = 0;
        }
      },
      onError: () => {
        state.focusKey = null;
      },
    });
  });
  return form;
}

function checkName(name, existing, what) {
  if (name === '') return `Enter a ${what} name.`;
  if (name.includes(',')) return 'Names can’t contain commas.';
  if (own(existing, name) !== undefined) return `There is already a ${what} named ${name}.`;
  return null;
}

// ---------------------------------------------------------------------------
// Globals

// The tools panel's compact Globals: each global's name, formula and value,
// with renaming, deleting and adding.
function renderGlobalsCompact(panel) {
  const b = budget();
  const r = report();
  const shownList = listNames(state.view, 'globals');
  const movable = canReorder('globals', shownList.filtered);
  const items = shownList.names.map((n) => [n, own(b.globals, n)]);
  const names = items.map(([n]) => n);
  const addKey = key('add', 'globals');
  if (Object.keys(b.globals || {}).length) panel.append(listTools('globals', false));

  if (!items.length) {
    panel.append(emptyNote('No globals yet.'));
  } else {
    const cols = el('div', 'setup-cols setup-grow');
    // The Name head sorts the list (setuplist.js); the others are labels.
    const hidden = (e) => {
      e.setAttribute('aria-hidden', 'true');
      return e;
    };
    const nameHead = el('span');
    nameHead.append(sortButton('globals', 'Name', redrawLists, key('sort', 'globals')));
    cols.append(nameHead, hidden(el('span', null, 'Formula')), hidden(el('span', 'setup-right', 'Value')), el('span'));
    panel.append(cols);
  }

  const ul = list('Globals');
  items.forEach(([name, formula], i) => {
    const ik = key('globals', name);
    const li = itemShell(ik, 'setup-grow');
    li.dataset.name = name;
    const issues = issuesFor('global', name);
    const nameEl = renameable(el('span', 'setup-name', name), 'global', name, ik, (to) => key('globals', to, 'formula'));
    const nameCell = orderable(nameEl, 'globals', name, movable, ik);
    const input = textInput({
      fieldKey: key('globals', name, 'formula'),
      value: str(formula),
      onCommit: (v, old) =>
        run(ik, () => ['set_global', { name, formula: v }], {
          restore: [key('globals', name, 'formula'), old],
        }),
    });
    li.append(nameCell, hiddenLabel(`Formula for ${name}`, input), input, globalValue(name, issues, r), globalDelete(name, ik, issues, r, neighbor('globals', names, i, addKey)));
    finishItem(li, ik, issues, input);
    ul.append(li);
  });
  if (items.length) panel.append(ul);
  panel.append(addRow('globals', 'global', ...globalAdder()));
}

/** A global's value as the table shows it, with its unit and where it came from on hover. */
function globalValue(name, issues, r, cls = '') {
  const b = budget();
  const raw = r ? own(r.globals, name) : undefined;
  // Shown with the global's own decimals, when it has them (display.js).
  const gInfo = own(b.global_info, name);
  // A global with a problem (one that refers back to itself, say) shows
  // #ERR and counts as 0 (decisions.md, entry 59).
  const value = issues.length
    ? el('span', `setup-value setup-num setup-value-bad${cls}`, '#ERR')
    : el('span', `setup-value setup-num${cls}`, raw === undefined ? '' : globalValueText(raw, gInfo));
  const shown = raw === undefined ? '' : formatDecimal(raw);
  // Its unit on hover: the plural unless the value is exactly 1.
  const unitDef = gInfo && gInfo.unit ? own(b.units, gInfo.unit) : undefined;
  const unitWord = gInfo && gInfo.unit ? (raw !== '1' && unitDef && unitDef.plural ? unitDef.plural : gInfo.unit) : '';
  const unitName = unitWord ? ` ${unitWord}` : '';
  value.title = issues.length ? 'This global has a problem, so it counts as 0' : `${shown}${unitName}`;
  // Where it came from (a setup pack's stamp), in Budget Setup.
  const from = compact ? '' : sourceText(state.view, 'global', name);
  if (from) value.title = `${value.title}\n${from}`;
  return value;
}

/** A global's delete button, asking first. */
function globalDelete(name, ik, issues, r, neighborKey) {
  const raw = r ? own(r.globals, name) : undefined;
  const shown = raw === undefined ? '' : formatDecimal(raw);
  const use = inUse(state.view, 'globals', name);
  const usedBy = use.lines ? `${plural(use.lines, 'line uses', 'lines use')} it. ` : '';
  return deleteButton({
    itemKey: ik,
    name,
    what: 'global',
    confirmText:
      shown && !issues.length
        ? `Delete ${name}? ${usedBy}Formulas that use it keep its current value (${shown}) as a plain number.`
        : `Delete ${name}? ${usedBy}Formulas that use it keep its current value as a plain number, where it has one.`,
    build: () => ['delete_global', { name }],
    neighborKey,
  });
}

/** The New global form's fields, check and command (addRow, addAction). */
function globalAdder() {
  return [
    [
      { key: 'name', label: 'Name (letters, digits and _, with at least one letter or _)', value: '', placeholder: 'SHOOT_DAYS' },
      { key: 'formula', label: 'Formula', value: '', placeholder: '30' },
    ],
    (v) => {
      const name = v.name.trim();
      if (name === '') return 'Enter a global name.';
      if (!GLOBAL_NAME.test(name)) return `“${name}” can’t be a global name.`;
      if (own(budget().globals, name) !== undefined) return `There is already a global named ${name}.`;
      if (v.formula.trim() === '') return 'Enter a formula or number.';
      return null;
    },
    (v) => [['set_global', { name: v.name.trim(), formula: v.formula.trim() }], key('globals', v.name.trim(), 'formula')],
  ];
}

/** Global `name`'s description and unit (its details), for searching and sorting. */
function globalInfo(name) {
  const g = own(budget().global_info, name) || {};
  return { description: str(g.description), unit: str(g.unit) };
}

/** Opens global `name`'s details form (display-settings.js) next to `anchor`; focus comes back to its row. */
function openGlobalDetails(name, anchor) {
  if (own(budget().globals, name) === undefined || isBusy()) return;
  const ik = key('globals', name);
  globalDetails(kit(), name, anchor).then(() => {
    const f = findByData('key', key('globals', name, 'formula')) || findByData('key', key('name', ik));
    if (f) f.focus();
  });
}

/** Budget Setup's Globals: one table, a row per global. */
function renderGlobals(panel) {
  if (compact) {
    renderGlobalsCompact(panel);
    return;
  }
  const b = budget();
  const r = report();
  const q = str(state.filters.globals).trim().toLowerCase();
  const shownList = listNames(
    state.view,
    'globals',
    (n) => !q || [n, own(b.globals, n), globalInfo(n).description].some((v) => str(v).toLowerCase().includes(q)),
    { textFilter: Boolean(q), sortValue: (n, col) => globalSort(n, col, r) },
  );
  const movable = canReorder('globals', shownList.filtered);
  const names = shownList.names;
  const all = Object.keys(b.globals || {}).length;
  const addKey = key('add', 'globals');
  const ul = list('Globals');
  names.forEach((name, i) => {
    const formula = own(b.globals, name);
    const ik = key('globals', name);
    const li = gridRow(ik, 'globals', name, movable);
    const issues = issuesFor('global', name);
    const nameEl = renameable(el('span', 'setup-name', name), 'global', name, ik, (to) => key('globals', to, 'formula'), () => openGlobalDetails(name, nameEl));
    const formulaInput = textInput({
      fieldKey: key('globals', name, 'formula'),
      value: str(formula),
      label: `Formula for ${name}`,
      cls: 'sw-cell-input',
      onCommit: (v, old) => run(ik, () => ['set_global', { name, formula: v }], { restore: [key('globals', name, 'formula'), old] }),
    });
    const more = el('button', 'setup-del sw-more', '…');
    more.type = 'button';
    more.dataset.key = key('details', ik);
    more.title = 'Details: description, unit, decimals, folder, show by name';
    more.setAttribute('aria-label', `Details of ${name}`);
    more.addEventListener('click', () => openGlobalDetails(name, more));
    li.append(
      cell(usesButton(state.view, 'globals', name, key('uses', ik)), 'sw-num'),
      cell(gridName(nameEl, 'globals', name, movable, ik)),
      cell(globalInfoCell(kit(), name, ik, 'description')),
      cell(formulaInput),
      cell(globalInfoCell(kit(), name, ik, 'unit')),
      cell(globalValue(name, issues, r), 'sw-num'),
      acts(more, globalDelete(name, ik, issues, r, neighbor('globals', names, i, addKey))),
    );
    finishItem(li, ik, issues, formulaInput);
    ul.append(li);
  });
  const [fields, check, build] = globalAdder();
  const strip = gridStrip('globals', 'global', 'Globals', addAction('globals', 'global', fields, check, build), [
    ['Details…', 'Show and change its details:', (n) => openGlobalDetails(n, findByData('key', key('details', key('globals', n))))],
  ]);
  const body = [];
  if (!all) body.push(emptyNote('No globals yet.'));
  else if (!names.length) body.push(emptyNote('Nothing matches the search.'));
  else {
    body.push(
      gridTable(
        'globals',
        gridHeads('globals', [
          ['uses', 'In use', 'Lines using it; click a count to find them', true],
          ['name', 'Name', ''],
          ['description', 'Description', ''],
          ['formula', 'Calculation', 'A number or a formula; it can use other globals'],
          ['units', 'Units', 'What its value counts, shown on hover'],
          ['value', 'Value', '', true],
          null,
        ]),
        ul,
      ),
    );
  }
  mountFrame(panel, { strip, body });
  setFooterTotals([{ label: '', value: names.length === all ? plural(all, 'global', 'globals') : `${names.length} of ${plural(all, 'global', 'globals')}` }]);
}

/** A global's value in column `col`, for sorting. */
function globalSort(name, col, r) {
  if (col === 'uses') {
    const u = usageOf(state.view, 'globals', name);
    return u ? Number(u.lines) || 0 : null;
  }
  if (col === 'description' || col === 'units') return globalInfo(name)[col === 'units' ? 'unit' : col];
  if (col === 'formula') return str(own(budget().globals, name));
  if (col === 'value') {
    const raw = r ? own(r.globals, name) : undefined;
    const n = Number(raw);
    return raw === undefined || Number.isNaN(n) ? null : n;
  }
  return name;
}

// ---------------------------------------------------------------------------
// Units

function unitArgs(name, patch) {
  const u = own(budget().units, name) || {};
  return [
    'set_unit',
    {
      name,
      unit: {
        plural: str(u.plural),
        description: str(u.description),
        hours: u.hours === undefined || u.hours === null || u.hours === '' ? null : String(u.hours),
        ...patch,
      },
    },
  ];
}

/** A unit's delete button, asking first (what it does depends on its use). */
function unitDelete(name, ik, neighborKey) {
  const use = inUse(state.view, 'units', name);
  return deleteButton({
    itemKey: ik,
    name,
    what: 'unit',
    confirmText: use.lines
      ? `Delete unit ${name}? ${plural(use.lines, 'line uses', 'lines use')} it; deleting blanks it on ${use.lines === 1 ? 'that line' : 'those lines'}.`
      : `Delete unit ${name}? No lines use it.${use.other ? ' A flat fringe or a global that names it stops converting.' : ''}`,
    confirmLabel: use.lines ? `Remove from ${plural(use.lines, 'line', 'lines')} and delete` : 'Delete',
    build: () => ['delete_unit', { name, clearLines: use.lines > 0 }],
    neighborKey,
  });
}

/** The New unit form's fields, check and command (addRow, addAction). */
function unitAdder() {
  return [
    [
      { key: 'name', label: 'Name', value: '', placeholder: 'Night' },
      { key: 'plural', label: 'Plural', value: '', placeholder: 'Nights' },
      { key: 'hours', label: 'Hours (blank for units like Flat)', value: '', placeholder: 'None' },
      { key: 'description', label: 'Description', value: '' },
    ],
    (v) => {
      const bad = checkName(v.name.trim(), budget().units, 'unit');
      if (bad) return bad;
      const h = v.hours.trim();
      if (h !== '' && !DECIMAL.test(h)) return 'Hours must be a number, like 10 or 7.5.';
      return null;
    },
    (v) => [
      [
        'set_unit',
        {
          name: v.name.trim(),
          unit: { plural: v.plural.trim(), description: v.description.trim(), hours: v.hours.trim() === '' ? null : v.hours.trim() },
        },
      ],
      key('units', v.name.trim(), 'plural'),
    ],
  ];
}

// The tools panel's compact Units: each unit's name, plural and hours as
// text, with renaming, deleting and adding.
function renderUnitsCompact(panel) {
  const b = budget();
  const shownList = listNames(state.view, 'units');
  const movable = canReorder('units', shownList.filtered);
  const items = shownList.names.map((n) => [n, own(b.units, n)]);
  const names = items.map(([n]) => n);
  if (Object.keys(b.units || {}).length) panel.append(listTools('units'));
  else panel.append(emptyNote('No units yet.'));

  const ul = list('Units');
  items.forEach(([name, u0], i) => {
    const u = u0 && typeof u0 === 'object' ? u0 : {};
    const ik = key('units', name);
    const li = itemShell(ik, 'setup-block');
    li.dataset.name = name;
    const head = el('div', 'setup-head');
    const nameEl = renameable(el('span', 'setup-name', name), 'unit', name, ik, (to) => key('units', to, 'plural'));
    const figs = el('span', 'setup-figs');
    figs.append(usesButton(state.view, 'units', name, key('uses', ik)));
    head.append(orderable(nameEl, 'units', name, movable, ik, false), figs, unitDelete(name, ik, neighbor('units', names, i, key('add', 'units'))));
    // Its plural and hours as text; they are edited in Budget Setup.
    const hasHours = u.hours !== undefined && u.hours !== null && u.hours !== '';
    const facts = [str(u.plural), hasHours ? `${u.hours} ${Number(u.hours) === 1 ? 'hour' : 'hours'}` : ''].filter(Boolean).join(', ');
    if (facts) head.insertBefore(el('span', 'setup-desc setup-unit-facts', facts), figs);
    li.append(head);
    finishItem(li, ik, issuesFor('unit', name), null);
    ul.append(li);
  });
  if (items.length) panel.append(ul);
  panel.append(addRow('units', 'unit', ...unitAdder()));
}

/** Budget Setup's Units: one table, a row per unit. */
function renderUnits(panel) {
  if (compact) {
    renderUnitsCompact(panel);
    return;
  }
  const b = budget();
  const q = str(state.filters.units).trim().toLowerCase();
  const shownList = listNames(
    state.view,
    'units',
    (n) => {
      if (!q) return true;
      const u = own(b.units, n) || {};
      return [n, u.plural, u.description].some((v) => str(v).toLowerCase().includes(q));
    },
    { textFilter: Boolean(q), sortValue: (n, col) => unitSort(n, col) },
  );
  const movable = canReorder('units', shownList.filtered);
  const names = shownList.names;
  const all = Object.keys(b.units || {}).length;
  const ul = list('Units');
  names.forEach((name, i) => {
    const u0 = own(b.units, name);
    const u = u0 && typeof u0 === 'object' ? u0 : {};
    const ik = key('units', name);
    const li = gridRow(ik, 'units', name, movable);
    const nameEl = renameable(el('span', 'setup-name', name), 'unit', name, ik, (to) => key('units', to, 'plural'));
    const field = (f, value, label, patchOf, opts = {}) =>
      textInput({
        fieldKey: key('units', name, f),
        value,
        label: `${label} of ${name}`,
        cls: 'sw-cell-input',
        ...opts,
        onCommit: (v, old) => run(ik, () => unitArgs(name, patchOf(v)), { restore: [key('units', name, f), old] }),
      });
    li.append(
      cell(usesButton(state.view, 'units', name, key('uses', ik)), 'sw-num'),
      cell(gridName(nameEl, 'units', name, movable, ik)),
      cell(field('plural', str(u.plural), 'Plural', (v) => ({ plural: v }))),
      cell(field('description', str(u.description), 'Description', (v) => ({ description: v }))),
      cell(
        field('hours', u.hours === undefined || u.hours === null ? '' : String(u.hours), 'Hours', (v) => ({ hours: v.trim() === '' ? null : v.trim() }), {
          num: true,
          placeholder: 'None',
        }),
      ),
      acts(unitDelete(name, ik, neighbor('units', names, i, key('add', 'units')))),
    );
    finishItem(li, ik, issuesFor('unit', name), null);
    ul.append(li);
  });
  const [fields, check, build] = unitAdder();
  const strip = gridStrip('units', 'unit', 'Units', addAction('units', 'unit', fields, check, build), []);
  const body = [];
  if (!all) body.push(emptyNote('No units yet.'));
  else if (!names.length) body.push(emptyNote('Nothing matches the search.'));
  else {
    body.push(
      gridTable(
        'units',
        gridHeads('units', [
          ['uses', 'In use', 'Lines using it; click a count to find them', true],
          ['name', 'Singular', ''],
          ['plural', 'Plural', ''],
          ['description', 'Description', ''],
          ['hours', 'Hours', 'Hours in one; lets flat fringes convert between time units. Blank for units like Flat', true],
          null,
        ]),
        ul,
      ),
    );
  }
  mountFrame(panel, { strip, body });
  setFooterTotals([{ label: '', value: names.length === all ? plural(all, 'unit', 'units') : `${names.length} of ${plural(all, 'unit', 'units')}` }]);
}

/** A unit's value in column `col`, for sorting. */
function unitSort(name, col) {
  const u = own(budget().units, name) || {};
  if (col === 'uses') {
    const use = usageOf(state.view, 'units', name);
    return use ? Number(use.lines) || 0 : null;
  }
  if (col === 'hours') {
    const n = Number(u.hours);
    return u.hours === undefined || u.hours === null || u.hours === '' || Number.isNaN(n) ? null : n;
  }
  return str(u[col]);
}

// ---------------------------------------------------------------------------
// Budget Setup's tables (setupframe.js): rows, heads, the strip

/** A cell of a table row. */
function cell(child, cls = '') {
  const c = el('span', `sw-cell${cls ? ` ${cls}` : ''}`);
  c.append(child);
  return c;
}

/** The last cell of a row: its own buttons (details, delete). */
function acts(...buttons) {
  const c = el('span', 'sw-cell sw-acts');
  c.append(...buttons);
  return c;
}

/**
 * A table row for item `name` of `table`: its drag handle; a focus inside
 * it selects it (the strip's Delete and Save to Library act on it).
 */
function gridRow(itemKey, table, name, movable) {
  const li = itemShell(itemKey, 'sw-grid-row');
  li.dataset.name = name;
  li.append(dragHandle(table, name, movable, callCmd));
  if (state.selected[table] === name) li.classList.add('is-selected');
  li.addEventListener('focusin', () => {
    if (state.selected[table] === name) return;
    state.selected[table] = name;
    const ul = li.parentElement;
    if (ul) for (const x of ul.querySelectorAll(':scope > .is-selected')) x.classList.remove('is-selected');
    li.classList.add('is-selected');
    syncGridStrip(table);
  });
  return li;
}

/** A row's name: the rename button, with Option+Shift+Up and Down to move it. */
function gridName(nameEl, table, name, movable, itemKey) {
  const shortcuts = nameEl.getAttribute('aria-keyshortcuts');
  nameEl.setAttribute('aria-keyshortcuts', shortcuts ? `${shortcuts} ${MOVE_KEYS}` : MOVE_KEYS);
  nameEl.addEventListener('keydown', (e) => {
    moveKey(e, table, name, movable, (cmd, args) =>
      callCmd(cmd, args).then(() => {
        const f = findByData('key', key('name', itemKey));
        if (f) f.focus();
      }),
    );
  });
  return nameEl;
}

/** The column heads: a sort button each ([id, label, title, numeric]); null for a blank head. */
function gridHeads(table, cols) {
  return [
    el('span', 'sw-head'),
    ...cols.map((c) => {
      if (!c) return el('span', 'sw-head');
      const [id, label, title, num] = c;
      const b = sortButton(table, label, redrawLists, key('sort', table, id), id);
      if (title) b.title = `${title}. ${b.title}`;
      const h = el('span', `sw-head${num ? ' sw-num' : ''}`);
      h.append(b);
      return h;
    }),
  ];
}

/** The selected row's name in `table`, when it still exists. */
function selectedIn(table) {
  const name = state.selected[table];
  return name !== undefined && own(budget()[table], name) !== undefined ? name : null;
}

/** The strip's buttons that act on the selected row follow it. */
function syncGridStrip(table) {
  const name = selectedIn(table);
  if (!state.container) return;
  for (const b of state.container.querySelectorAll('[data-needs-item]')) {
    if (name === null) b.setAttribute('aria-disabled', 'true');
    else b.removeAttribute('aria-disabled');
    b.title = name === null ? `Select a ${b.dataset.what} first` : `${b.dataset.does} ${name}`;
  }
}

/**
 * A table's action strip: New (`add`), then `extra` ([label, does, run(name)])
 * and Delete and Save to Library for the selected row, then the search.
 */
function gridStrip(table, what, label, add, extra) {
  const name = selectedIn(table);
  const needs = (text, does, run) => {
    const b = stripButton(text, key('strip', table, text), () => {
      const n = selectedIn(table);
      if (n !== null) run(n);
    }, { enabled: name !== null, title: name === null ? `Select a ${what} first` : `${does} ${name}` });
    b.dataset.needsItem = '1';
    b.dataset.does = does;
    b.dataset.what = what;
    return b;
  };
  return actionStrip(
    label,
    [
      stripButton(`New ${what}…`, key('add', table), add, { title: `Add a ${what} to this budget` }),
      ...extra.map(([text, does, run]) => needs(text, does, run)),
      needs('Delete…', `Delete ${what}`, (n) => {
        const del = findByData('key', key('del', key(table, n)));
        if (del) del.click();
      }),
      needs('Save to Library…', `Copy to your library, with what it needs: ${what}`, (n) =>
        saveItemToLibrary({ call: callCmd, toast: (m) => state.ctx && typeof state.ctx.toast === 'function' && state.ctx.toast(m) }, what, n),
      ),
    ],
    searchBox({
      key: key('search', table),
      value: str(state.filters[table]),
      label: `Search ${label.toLowerCase()}`,
      placeholder: `Search ${label.toLowerCase()}`,
      onChange: (v) => {
        state.filters[table] = v;
        redrawLists();
      },
    }),
  );
}

// ---------------------------------------------------------------------------
// Counts, sorting and reordering (setuplist.js)

/**
 * Above a list: its name sort (unless the list's own column heads have it)
 * and the Show choice. Asks for the counts.
 */
function listTools(table, sort = true) {
  wantUsage(state.view);
  const row = el('div', 'setup-listtools');
  row.append(sort ? sortButton(table, 'Name', redrawLists, key('sort', table)) : el('span'), showSelect(table, redrawLists, key('show', table)));
  if (compact && canOpenSetup()) {
    const label = table === 'globals' ? 'globals' : 'units';
    const go = el('button', 'apply-pill setup-open-setup', 'Setup');
    go.type = 'button';
    go.dataset.key = key('open-setup', table);
    go.dataset.nobusy = '1';
    go.title = `Every detail of the ${label}, with the library, in the Budget Setup window`;
    go.setAttribute('aria-label', `Edit ${label} in Budget Setup`);
    go.addEventListener('click', () => state.ctx.openSetup(table, null));
    row.append(go);
  }
  return row;
}

function callCmd(cmd, args) {
  if (!state.ctx || typeof state.ctx.call !== 'function') return Promise.reject(new Error('Not connected'));
  return state.ctx.call(cmd, args);
}

/**
 * An item's name with its drag handle (and, with `uses`, its line count),
 * and Option+Shift+Up and Down on the name to move it.
 */
function orderable(nameEl, table, name, movable, itemKey, uses = true) {
  const wrap = el('span', 'setup-namewrap');
  wrap.append(dragHandle(table, name, movable, callCmd), nameEl);
  if (uses) wrap.append(usesButton(state.view, table, name, key('uses', itemKey)));
  const shortcuts = nameEl.getAttribute('aria-keyshortcuts');
  nameEl.setAttribute('aria-keyshortcuts', shortcuts ? `${shortcuts} ${MOVE_KEYS}` : MOVE_KEYS);
  nameEl.addEventListener('keydown', (e) => {
    moveKey(e, table, name, movable, (cmd, args) =>
      callCmd(cmd, args).then(() => {
        const f = findByData('key', key('name', itemKey));
        if (f) f.focus();
      }),
    );
  });
  return wrap;
}

// ---------------------------------------------------------------------------
// Renaming a global or unit from its name

/**
 * Makes an item's name the way to rename it: focusable, double-click or
 * Enter or F2 opens the Rename form, and a right-click menu offers Rename
 * and Delete. `focusAfter(to)` is the key of the control to focus once it
 * is renamed. `details`, when given, opens the item's details (a global's,
 * in Budget Setup), from the menu too.
 */
function renameable(nameEl, kind, name, itemKey, focusAfter, details = null) {
  nameEl.title = `${name}\nDouble-click to rename it everywhere`;
  nameEl.tabIndex = 0;
  nameEl.dataset.key = key('name', itemKey);
  nameEl.setAttribute('role', 'button');
  nameEl.setAttribute('aria-label', `Rename ${kind} ${name}`);
  nameEl.setAttribute('aria-keyshortcuts', 'Enter F2');
  const ctx = () => ({
    call: (cmd, args) => state.ctx.call(cmd, args),
    toast: (m) => state.ctx && typeof state.ctx.toast === 'function' && state.ctx.toast(m),
    budget,
  });
  const after = (to) => {
    if (to === null) return;
    const f = findByData('key', focusAfter(to));
    if (f) f.focus();
  };
  const rename = () => {
    if (isBusy() || !state.ctx || typeof state.ctx.call !== 'function') return;
    renameItem({ anchor: nameEl, kind, name, ctx: ctx() }).then(after);
  };
  nameEl.addEventListener('dblclick', (e) => {
    e.preventDefault();
    rename();
  });
  nameEl.addEventListener('keydown', (e) => {
    if ((e.key === 'Enter' || e.key === 'F2') && !e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey) {
      e.preventDefault();
      rename();
    }
  });
  nameEl.addEventListener('contextmenu', (e) => {
    e.preventDefault();
    if (isBusy() || !state.ctx) return;
    const table = kind === 'global' ? 'globals' : 'units';
    itemMenu(e, {
      kind,
      name,
      anchor: nameEl,
      ctx: ctx(),
      onRenamed: after,
      remove: () => {
        const del = findByData('key', key('del', itemKey));
        if (del) del.click();
      },
      extra: [
        ...(details ? [{ label: 'Details…', title: 'Its description, unit, decimals, folder and show by name', run: details }] : []),
        ...(canOpenSetup() ? [{ label: 'Edit in Budget Setup', title: `Show this ${kind} in the Budget Setup window`, run: () => state.ctx.openSetup(table, name) }] : []),
      ],
    });
  });
  return nameEl;
}

// ---------------------------------------------------------------------------
// Adding an item: the row at the bottom of a list opens a small form next to it

/**
 * The "New ..." row at the bottom of a list. `fields` are anchoredForm's;
 * `check(values)` returns a message or null; `build(values)` returns
 * [[command, args], key of the control to focus once it is added].
 */
function addRow(tab, what, fields, check, build) {
  const b = el('button', 'apply-add-btn', `New ${what}…`);
  b.type = 'button';
  b.title = `Add a ${what} to this budget`;
  b.dataset.key = key('add', tab);
  b.dataset.nobusy = '1';
  const add = addAction(tab, what, fields, check, build);
  b.addEventListener('click', () => add(b));
  const wrap = el('div', 'setup-add-row');
  wrap.append(b);
  return wrap;
}

/**
 * What a New button does (the row above, or Budget Setup's strip): opens
 * the form next to the button `b` it is given, and focuses the new item's
 * control once it is added (else the button, by its key).
 */
function addAction(tab, what, fields, check, build) {
  return (b) => {
    let focus = null;
    anchoredForm({
      anchor: b,
      title: `New ${what}`,
      fields,
      submitText: 'Add',
      onSubmit: (v) => {
        const bad = check(v);
        if (bad) return Promise.reject(bad);
        const [[cmd, args], then] = build(v);
        if (!state.ctx || typeof state.ctx.call !== 'function') return Promise.reject('Not connected');
        return state.ctx.call(cmd, args).then(() => {
          focus = then;
        });
      },
    }).then(() => {
      const target = findByData('key', focus || key('add', tab));
      if (target) target.focus();
    });
  };
}

// ---------------------------------------------------------------------------
// Helpers shared with the tabs in topsheet-setup.js, so they use the same
// queue, focus handling, error display and controls as the tabs above.

function kit() {
  return {
    el,
    key,
    nextId,
    own,
    str,
    entries,
    budget,
    report,
    baseCode,
    formatCents,
    centsToText,
    formatDecimal,
    plural,
    findByData,
    isBusy,
    run,
    errorLine,
    textInput,
    selectInput,
    field,
    itemShell,
    finishItem,
    deleteButton,
    neighbor,
    list,
    emptyNote,
    addInput,
    addSelect,
    addForm,
    checkName,
    view: () => state.view,
    state: () => state,
    setFocusKey: (k) => {
      state.focusKey = k;
    },
  };
}
