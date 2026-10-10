// Setup tables drawn in place: globals and units (tabs of the tools panel,
// tools.js), and the kit of helpers the Budget tab's own tabs use
// (topsheet-setup.js: charges, credits, info, settings, baseline). Fringes,
// groups, sets, locations and currencies are lists in apply.js.
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
  dragHandle,
  moveKey,
  MOVE_KEYS,
  inUse,
  redrawLists,
} from './setuplist.js';
import { globalValueText } from './display.js';
import { globalInfoBlock } from './display-settings.js';

const GLOBAL_NAME = /^[A-Za-z_][A-Za-z0-9_]*$/;
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
};

let uid = 0;

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

/** Draws setup tab `tab` (globals, units, or one of the Budget tab's) into `panel`. */
export function renderSetupTab(panel, tab) {
  state.shown.push(tab);
  panel.classList.add('setup-panel');
  if (tab === 'globals') renderGlobals(panel);
  else if (tab === 'units') renderUnits(panel);
  else renderExtraTab(tab, panel, kit());
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

function str(v) {
  return v === undefined || v === null ? '' : String(v);
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

function errText(e) {
  if (typeof e === 'string') return e;
  if (e && typeof e.message === 'string') return e.message;
  return String(e);
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

function plural(n, one, many) {
  return `${n} ${n === 1 ? one : many}`;
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

function textInput({ fieldKey, value, onCommit, num = false, placeholder = '', label }) {
  const input = el('input', num ? 'setup-input setup-num' : 'setup-input');
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

function selectInput({ fieldKey, value, options, onCommit }) {
  const sel = el('select', 'setup-input setup-select');
  sel.id = nextId();
  sel.dataset.key = fieldKey;
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

function renderGlobals(panel) {
  const b = budget();
  const r = report();
  panel.append(
    el('p', 'setup-intro', 'Named numbers you can use in any formula, like DAYS * 5. A global can use other globals.'),
  );
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
    const raw = r ? own(r.globals, name) : undefined;
    // Shown with the global's own decimals, when it has them (display.js).
    const gInfo = own(b.global_info, name);
    const value = issues.length
      ? el('span', 'setup-value setup-num setup-value-bad', 'Error')
      : el('span', 'setup-value setup-num', raw === undefined ? '' : globalValueText(raw, gInfo));
    const shown = raw === undefined ? '' : formatDecimal(raw);
    // Its unit on hover: the plural unless the value is exactly 1.
    const unitDef = gInfo && gInfo.unit ? own(b.units, gInfo.unit) : undefined;
    const unitWord = gInfo && gInfo.unit ? (raw !== '1' && unitDef && unitDef.plural ? unitDef.plural : gInfo.unit) : '';
    const unitName = unitWord ? ` ${unitWord}` : '';
    value.title = issues.length ? 'This global has a problem' : `${shown}${unitName}`;
    const use = inUse(state.view, 'globals', name);
    const usedBy = use.lines ? `${plural(use.lines, 'line uses', 'lines use')} it. ` : '';
    li.append(
      nameCell,
      hiddenLabel(`Formula for ${name}`, input),
      input,
      value,
      deleteButton({
        itemKey: ik,
        name,
        what: 'global',
        confirmText:
          shown && !issues.length
            ? `Delete ${name}? ${usedBy}Formulas that use it keep its current value (${shown}) as a plain number.`
            : `Delete ${name}? ${usedBy}Formulas that use it keep its current value as a plain number, where it has one.`,
        build: () => ['delete_global', { name }],
        neighborKey: neighbor('globals', names, i, addKey),
      }),
    );
    // Description, unit, decimals, show by name and folder (display-settings.js).
    li.append(globalInfoBlock(kit(), name, ik));
    finishItem(li, ik, issues, input);
    ul.append(li);
  });
  if (items.length) panel.append(ul);

  panel.append(
    addRow('globals', 'global', [
      { key: 'name', label: 'Name (letters, digits and _, not starting with a digit)', value: '', placeholder: 'SHOOT_DAYS' },
      { key: 'formula', label: 'Formula', value: '', placeholder: '30' },
    ], (v) => {
      const name = v.name.trim();
      if (name === '') return 'Enter a global name.';
      if (!GLOBAL_NAME.test(name)) return `“${name}” can’t be a global name.`;
      if (own(budget().globals, name) !== undefined) return `There is already a global named ${name}.`;
      if (v.formula.trim() === '') return 'Enter a formula or number.';
      return null;
    }, (v) => [['set_global', { name: v.name.trim(), formula: v.formula.trim() }], key('globals', v.name.trim(), 'formula')]),
  );
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

function renderUnits(panel) {
  const b = budget();
  panel.append(
    el(
      'p',
      'setup-intro',
      'Units for quantities on lines. Hours lets flat fringes convert between time units; leave it blank for units like Flat.',
    ),
  );
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
    const use = inUse(state.view, 'units', name);
    head.append(
      orderable(nameEl, 'units', name, movable, ik, false),
      figs,
      deleteButton({
        itemKey: ik,
        name,
        what: 'unit',
        confirmText: use.lines
          ? `Delete unit ${name}? ${plural(use.lines, 'line uses', 'lines use')} it; deleting blanks it on ${use.lines === 1 ? 'that line' : 'those lines'}.`
          : `Delete unit ${name}? No lines use it.${use.other ? ' A flat fringe or a global that names it stops converting.' : ''}`,
        confirmLabel: use.lines ? `Remove from ${plural(use.lines, 'line', 'lines')} and delete` : 'Delete',
        build: () => ['delete_unit', { name, clearLines: use.lines > 0 }],
        neighborKey: neighbor('units', names, i, key('add', 'units')),
      }),
    );
    const grid = el('div', 'setup-fields');
    const pl = textInput({
      fieldKey: key('units', name, 'plural'),
      value: str(u.plural),
      onCommit: (v, old) =>
        run(ik, () => unitArgs(name, { plural: v }), { restore: [key('units', name, 'plural'), old] }),
    });
    const hours = textInput({
      fieldKey: key('units', name, 'hours'),
      value: u.hours === undefined || u.hours === null ? '' : String(u.hours),
      num: true,
      placeholder: 'None',
      onCommit: (v, old) =>
        run(ik, () => unitArgs(name, { hours: v.trim() === '' ? null : v.trim() }), {
          restore: [key('units', name, 'hours'), old],
        }),
    });
    const desc = textInput({
      fieldKey: key('units', name, 'description'),
      value: str(u.description),
      onCommit: (v, old) =>
        run(ik, () => unitArgs(name, { description: v }), {
          restore: [key('units', name, 'description'), old],
        }),
    });
    grid.classList.add('setup-fields-3');
    grid.append(field('Plural', pl), field('Hours', hours), field('Description', desc));
    li.append(head, grid);
    finishItem(li, ik, issuesFor('unit', name), null);
    ul.append(li);
  });
  if (items.length) panel.append(ul);

  panel.append(
    addRow('units', 'unit', [
      { key: 'name', label: 'Name', value: '', placeholder: 'Night' },
      { key: 'plural', label: 'Plural', value: '', placeholder: 'Nights' },
      { key: 'hours', label: 'Hours (blank for units like Flat)', value: '', placeholder: 'None' },
      { key: 'description', label: 'Description', value: '' },
    ], (v) => {
      const bad = checkName(v.name.trim(), budget().units, 'unit');
      if (bad) return bad;
      const h = v.hours.trim();
      if (h !== '' && !DECIMAL.test(h)) return 'Hours must be a number, like 10 or 7.5.';
      return null;
    }, (v) => [
      [
        'set_unit',
        {
          name: v.name.trim(),
          unit: { plural: v.plural.trim(), description: v.description.trim(), hours: v.hours.trim() === '' ? null : v.hours.trim() },
        },
      ],
      key('units', v.name.trim(), 'plural'),
    ]),
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
 * is renamed.
 */
function renameable(nameEl, kind, name, itemKey, focusAfter) {
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
  b.addEventListener('click', () => {
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
  });
  const wrap = el('div', 'setup-add-row');
  wrap.append(b);
  return wrap;
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
