// Setup panel: globals, fringes, groups, units and currencies.
//
// renderSetup(container, view, ctx) rebuilds the panel from scratch on every
// call. Budget text only ever reaches the page through textContent and input
// values (never innerHTML), and styling is done with classes from setup.css.
//
// Edits commit on Enter or blur and Escape puts the old value back. Commands run
// one at a time through a queue, and each one is built from the newest view when
// it runs, so two quick edits to the same item never undo each other.

import { EXTRA_TABS, renderExtraTab } from './topsheet-setup.js';

const TABS = [
  { id: 'globals', label: 'Globals' },
  { id: 'fringes', label: 'Fringes' },
  { id: 'groups', label: 'Groups' },
  { id: 'units', label: 'Units' },
  { id: 'currencies', label: 'Currencies' },
  // Sets, locations, charges, credits, info, settings, baseline.
  ...EXTRA_TABS,
];

// Tabs are split into two sections so the strip fits one row: reusable
// tables lines refer to, and settings for the budget as a whole.
const SECTIONS = [
  { id: 'tables', label: 'Tables', tabs: ['globals', 'fringes', 'groups', 'units', 'currencies', 'sets', 'locations'] },
  { id: 'budget', label: 'Budget', tabs: ['charges', 'credits', 'info', 'settings', 'baseline'] },
];

function sectionOf(tab) {
  return SECTIONS.find((s) => s.tabs.includes(tab)) || SECTIONS[0];
}

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
  // Tab shown last time; switching tabs drops old error messages.
  tab: null,
};

let uid = 0;

export function renderSetup(container, view, ctx) {
  const focus = captureFocus(container);
  state.container = container;
  state.view = view && typeof view === 'object' ? view : {};
  state.ctx = ctx;
  uid = 0;

  const tab = TABS.some((t) => t.id === (ctx && ctx.tab)) ? ctx.tab : 'globals';
  if (tab !== state.tab) state.errors.clear();
  state.tab = tab;
  const root = el('div', 'setup');
  root.append(renderSections(tab), renderTabs(tab), renderPanel(tab));
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

// Money as typed ("150,000", "(12.50)") to text the backend parses.
function moneyInput(s) {
  let t = s.replace(/[,\s]/g, '');
  if (t === '') return null;
  const m = /^\((.*)\)$/.exec(t);
  if (m) t = `-${m[1]}`;
  return t;
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
    if (a.dataset.orig !== undefined && a.value !== a.dataset.orig) {
      info.value = a.value;
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
  if (!target) target = container.querySelector('.setup-tab[aria-selected="true"]');
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
  const panel = c.querySelector('.setup-panel');
  if (panel) panel.setAttribute('aria-busy', busy ? 'true' : 'false');
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
function deleteButton({ itemKey, name, what, confirmText, build, neighborKey }) {
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
    const yes = el('button', 'setup-btn setup-btn-danger', 'Delete');
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
// Tabs and panel

// Remembers the last tab used in each section, so switching back returns there.
const lastTab = {};

function renderSections(active) {
  const current = sectionOf(active);
  lastTab[current.id] = active;
  const bar = el('div', 'setup-sections');
  bar.setAttribute('role', 'group');
  bar.setAttribute('aria-label', 'Setup section');
  for (const s of SECTIONS) {
    const b = el('button', 'setup-section', s.label);
    b.type = 'button';
    b.dataset.key = key('section', s.id);
    b.dataset.nobusy = '1';
    b.setAttribute('aria-pressed', s.id === current.id ? 'true' : 'false');
    b.addEventListener('click', () => {
      if (s.id === current.id) return;
      const tab = lastTab[s.id] || s.tabs[0];
      state.focusKey = key('section', s.id);
      if (state.ctx && typeof state.ctx.setTab === 'function') state.ctx.setTab(tab);
    });
    bar.append(b);
  }
  return bar;
}

function renderTabs(active) {
  const ids = sectionOf(active).tabs;
  const tabs = TABS.filter((t) => ids.includes(t.id));
  const bar = el('div', 'setup-tabs');
  bar.setAttribute('role', 'tablist');
  bar.setAttribute('aria-label', 'Budget setup');
  const go = (id) => {
    if (id === active) return;
    state.focusKey = key('tab', id);
    if (state.ctx && typeof state.ctx.setTab === 'function') state.ctx.setTab(id);
  };
  tabs.forEach((t, i) => {
    const b = el('button', 'setup-tab', t.label);
    b.type = 'button';
    b.id = `setup-tab-${t.id}`;
    b.setAttribute('role', 'tab');
    b.setAttribute('aria-selected', t.id === active ? 'true' : 'false');
    b.setAttribute('aria-controls', 'setup-panel');
    b.tabIndex = t.id === active ? 0 : -1;
    b.dataset.key = key('tab', t.id);
    b.dataset.nobusy = '1';
    b.addEventListener('click', () => go(t.id));
    b.addEventListener('keydown', (e) => {
      let j = null;
      if (e.key === 'ArrowRight') j = (i + 1) % tabs.length;
      else if (e.key === 'ArrowLeft') j = (i - 1 + tabs.length) % tabs.length;
      else if (e.key === 'Home') j = 0;
      else if (e.key === 'End') j = tabs.length - 1;
      if (j === null) return;
      e.preventDefault();
      go(tabs[j].id);
    });
    bar.append(b);
  });
  return bar;
}

function renderPanel(tab) {
  const panel = el('div', 'setup-panel');
  panel.id = 'setup-panel';
  panel.setAttribute('role', 'tabpanel');
  panel.setAttribute('aria-labelledby', `setup-tab-${tab}`);
  const render = {
    globals: renderGlobals,
    fringes: renderFringes,
    groups: renderGroups,
    units: renderUnits,
    currencies: renderCurrencies,
  }[tab];
  if (render) render(panel);
  else renderExtraTab(tab, panel, kit());
  return panel;
}

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
  const items = entries(b.globals);
  const names = items.map(([n]) => n);
  const addKey = key('add', 'globals', 'name');

  if (!items.length) {
    panel.append(emptyNote('No globals yet.'));
  } else {
    const cols = el('div', 'setup-cols setup-grow');
    cols.setAttribute('aria-hidden', 'true');
    cols.append(el('span', null, 'Name'), el('span', null, 'Formula'), el('span', 'setup-right', 'Value'), el('span'));
    panel.append(cols);
  }

  const ul = list('Globals');
  items.forEach(([name, formula], i) => {
    const ik = key('globals', name);
    const li = itemShell(ik, 'setup-grow');
    const issues = issuesFor('global', name);
    const nameEl = el('span', 'setup-name', name);
    nameEl.title = name;
    const input = textInput({
      fieldKey: key('globals', name, 'formula'),
      value: str(formula),
      onCommit: (v, old) =>
        run(ik, () => ['set_global', { name, formula: v }], {
          restore: [key('globals', name, 'formula'), old],
        }),
    });
    const raw = r ? own(r.globals, name) : undefined;
    const value = issues.length
      ? el('span', 'setup-value setup-num setup-value-bad', 'Error')
      : el('span', 'setup-value setup-num', raw === undefined ? '' : formatDecimal(raw));
    const shown = raw === undefined ? '' : formatDecimal(raw);
    value.title = issues.length ? 'This global has a problem' : shown;
    li.append(
      nameEl,
      hiddenLabel(`Formula for ${name}`, input),
      input,
      value,
      deleteButton({
        itemKey: ik,
        name,
        what: 'global',
        confirmText:
          shown && !issues.length
            ? `Delete ${name}? Formulas that use it keep its current value (${shown}) as a plain number.`
            : `Delete ${name}? Formulas that use it keep its current value as a plain number, where it has one.`,
        build: () => ['delete_global', { name }],
        neighborKey: neighbor('globals', names, i, addKey),
      }),
    );
    finishItem(li, ik, issues, input);
    ul.append(li);
  });
  if (items.length) panel.append(ul);

  const hint = el('p', 'setup-hint', 'Letters, digits and _, not starting with a digit.');
  hint.id = nextId();
  const nameField = addInput('globals', 'name', { label: 'Name', placeholder: 'SHOOT_DAYS', hintId: hint.id });
  const nameInput = nameField.querySelector('input');
  const check = () => {
    const v = nameInput.value.trim();
    const bad = v !== '' && !GLOBAL_NAME.test(v);
    hint.classList.toggle('setup-hint-bad', bad);
    if (bad) nameInput.setAttribute('aria-invalid', 'true');
    else nameInput.removeAttribute('aria-invalid');
  };
  nameInput.addEventListener('input', check);
  check();
  panel.append(
    addForm('globals', {
      title: 'New global',
      fields: [nameField, addInput('globals', 'formula', { label: 'Formula', placeholder: '30' })],
      hint,
      validate: (v) => {
        const name = v.name.trim();
        if (name === '') return ['name', 'Enter a global name.'];
        if (!GLOBAL_NAME.test(name)) return ['name', `“${name}” can’t be a global name.`];
        if (own(b.globals, name) !== undefined) return ['name', `There is already a global named ${name}.`];
        if (v.formula.trim() === '') return ['formula', 'Enter a formula or number.'];
        return null;
      },
      build: (v) => ['set_global', { name: v.name.trim(), formula: v.formula.trim() }],
    }),
  );
}

// ---------------------------------------------------------------------------
// Fringes

function fringeArgs(name, patch) {
  const f = own(budget().fringes, name) || {};
  const out = {
    kind: f.kind === 'flat' ? 'flat' : 'percent',
    description: str(f.description),
    rate: str(f.rate),
    cutoff: f.cutoff === undefined || f.cutoff === null ? null : centsToText(f.cutoff),
    unit: f.unit === undefined || f.unit === null ? null : String(f.unit),
    ...patch,
  };
  if (out.kind === 'percent') out.unit = null;
  else out.cutoff = null;
  return ['set_fringe', { name, fringe: out }];
}

function timeUnits(current) {
  const names = entries(budget().units)
    .filter(([, u]) => u && typeof u === 'object' && u.hours !== undefined && u.hours !== null)
    .map(([n]) => n);
  if (current && !names.includes(current)) names.push(current);
  return names;
}

function renderFringes(panel) {
  const b = budget();
  const r = report();
  const base = baseCode();
  panel.append(
    el(
      'p',
      'setup-intro',
      'Costs added on top of the lines they are applied to: a percent of pay, or a flat amount per line or per unit of time.',
    ),
  );
  const items = entries(b.fringes);
  const names = items.map(([n]) => n);
  if (!items.length) panel.append(emptyNote('No fringes yet.'));

  const ul = list('Fringes');
  items.forEach(([name, f0], i) => {
    const f = f0 && typeof f0 === 'object' ? f0 : {};
    const ik = key('fringes', name);
    const li = itemShell(ik, 'setup-block');
    const kind = f.kind === 'flat' ? 'flat' : 'percent';
    const sum = r ? own(r.fringes, name) : undefined;
    const uses = sum && Number.isFinite(sum.uses) ? sum.uses : null;

    const head = el('div', 'setup-head');
    const nameEl = el('span', 'setup-name', name);
    nameEl.title = name;
    const figs = el('span', 'setup-figs');
    if (uses !== null) figs.append(el('span', 'setup-muted', uses ? plural(uses, 'line', 'lines') : 'Not used'));
    if (sum && sum.total !== undefined) {
      const t = el('span', 'setup-num', formatCents(sum.total));
      t.title = 'Total of this fringe';
      figs.append(t);
    }
    head.append(
      nameEl,
      figs,
      deleteButton({
        itemKey: ik,
        name,
        what: 'fringe',
        confirmText: uses
          ? `Delete fringe ${name}? It comes off the ${plural(uses, 'line', 'lines')} it is applied to.`
          : `Delete fringe ${name}?`,
        build: () => ['delete_fringe', { name }],
        neighborKey: neighbor('fringes', names, i, key('add', 'fringes', 'name')),
      }),
    );

    const grid = el('div', 'setup-fields');
    const kindSel = selectInput({
      fieldKey: key('fringes', name, 'kind'),
      value: kind,
      options: [
        ['percent', 'Percent of pay'],
        ['flat', 'Flat amount'],
      ],
      onCommit: (v, old) =>
        run(ik, () => fringeArgs(name, { kind: v }), { restore: [key('fringes', name, 'kind'), old] }),
    });
    const rate = textInput({
      fieldKey: key('fringes', name, 'rate'),
      value: str(f.rate),
      num: true,
      onCommit: (v, old) =>
        run(ik, () => fringeArgs(name, { rate: v }), { restore: [key('fringes', name, 'rate'), old] }),
    });
    grid.append(field('Kind', kindSel), field('Rate', rate, { extra: kind === 'percent' ? '%' : base }));

    if (kind === 'percent') {
      const shown = f.cutoff === undefined || f.cutoff === null ? '' : formatCents(f.cutoff);
      const cutoff = textInput({
        fieldKey: key('fringes', name, 'cutoff'),
        value: shown,
        num: true,
        placeholder: 'None',
        onCommit: (v, old) =>
          run(ik, () => fringeArgs(name, { cutoff: moneyInput(v) }), {
            restore: [key('fringes', name, 'cutoff'), old],
          }),
      });
      const fc = field('Cutoff (pay per earner)', cutoff, { wide: true });
      grid.append(fc);
    } else {
      const current = f.unit === undefined || f.unit === null ? '' : String(f.unit);
      const per = selectInput({
        fieldKey: key('fringes', name, 'unit'),
        value: current,
        options: [['', 'Line'], ...timeUnits(current).map((u) => [u, u])],
        onCommit: (v, old) =>
          run(ik, () => fringeArgs(name, { unit: v === '' ? null : v }), {
            restore: [key('fringes', name, 'unit'), old],
          }),
      });
      grid.append(field('Charged per', per, { wide: true }));
    }

    const desc = textInput({
      fieldKey: key('fringes', name, 'description'),
      value: str(f.description),
      onCommit: (v, old) =>
        run(ik, () => fringeArgs(name, { description: v }), {
          restore: [key('fringes', name, 'description'), old],
        }),
    });
    grid.append(field('Description', desc, { wide: true }));
    li.append(head, grid);
    finishItem(li, ik, issuesFor('fringe', name), rate);
    ul.append(li);
  });
  if (items.length) panel.append(ul);

  panel.append(
    addForm('fringes', {
      title: 'New fringe',
      fields: [
        addInput('fringes', 'name', { label: 'Name', placeholder: 'Payroll' }),
        addSelect('fringes', 'kind', 'Kind', [
          ['percent', 'Percent'],
          ['flat', 'Flat'],
        ]),
        addInput('fringes', 'rate', { label: 'Rate', placeholder: '18.5', num: true }),
      ],
      validate: (v) => {
        const bad = checkName(v.name.trim(), b.fringes, 'fringe');
        if (bad) return ['name', bad];
        if (v.rate.trim() === '') return ['rate', 'Enter a rate.'];
        return null;
      },
      build: (v) => [
        'set_fringe',
        {
          name: v.name.trim(),
          fringe: {
            kind: v.kind === 'flat' ? 'flat' : 'percent',
            description: '',
            rate: v.rate.trim(),
            cutoff: null,
            unit: null,
          },
        },
      ],
    }),
  );
}

// ---------------------------------------------------------------------------
// Groups

function groupArgs(name, patch) {
  const g = own(budget().groups, name) || {};
  return [
    'set_group',
    {
      name,
      group: {
        description: str(g.description),
        in_budget_total: g.in_budget_total !== false,
        ...patch,
      },
    },
  ];
}

function breakdownTotal(bd) {
  if (!bd || typeof bd !== 'object') return null;
  try {
    return BigInt(bd.details_total || 0) + BigInt(bd.fringe_total || 0);
  } catch {
    return null;
  }
}

function stat(label, bd, help) {
  const wrap = el('span', 'setup-stat');
  const total = breakdownTotal(bd);
  wrap.append(el('span', 'setup-muted', label), el('span', 'setup-num', total === null ? '' : formatCents(total)));
  if (total !== null) {
    wrap.title = `${help}: lines ${formatCents(bd.details_total || 0)} plus fringes ${formatCents(bd.fringe_total || 0)}`;
  }
  return wrap;
}

function renderGroups(panel) {
  const b = budget();
  const r = report();
  panel.append(
    el(
      'p',
      'setup-intro',
      'Tags for lines. Untick Counts in total to show a group’s lines without adding them to the budget.',
    ),
  );
  const items = entries(b.groups);
  const names = items.map(([n]) => n);
  if (!items.length) panel.append(emptyNote('No groups yet.'));

  const ul = list('Groups');
  items.forEach(([name, g0], i) => {
    const g = g0 && typeof g0 === 'object' ? g0 : {};
    const ik = key('groups', name);
    const li = itemShell(ik, 'setup-block');
    const counted = g.in_budget_total !== false;
    const totals = r ? own(r.groups, name) : undefined;

    const head = el('div', 'setup-head');
    const nameEl = el('span', 'setup-name', name);
    nameEl.title = name;
    head.append(
      nameEl,
      el('span', 'setup-figs'),
      deleteButton({
        itemKey: ik,
        name,
        what: 'group',
        confirmText: counted
          ? `Delete group ${name}? Its lines lose the tag.`
          : `Delete group ${name}? Its lines lose the tag and count in the total again.`,
        build: () => ['delete_group', { name }],
        neighborKey: neighbor('groups', names, i, key('add', 'groups', 'name')),
      }),
    );

    const grid = el('div', 'setup-fields');
    const boxKey = key('groups', name, 'in_budget_total');
    const box = el('input', 'setup-check');
    box.type = 'checkbox';
    box.id = nextId();
    box.checked = counted;
    box.dataset.key = boxKey;
    box.addEventListener('change', () => {
      const v = box.checked;
      run(ik, () => groupArgs(name, { in_budget_total: v }), { restore: [boxKey, !v] });
    });
    const boxLabel = el('label', 'setup-check-label', 'Counts in total');
    boxLabel.htmlFor = box.id;
    const boxWrap = el('div', 'setup-field setup-field-wide setup-check-row');
    boxWrap.append(box, boxLabel);

    const desc = textInput({
      fieldKey: key('groups', name, 'description'),
      value: str(g.description),
      onCommit: (v, old) =>
        run(ik, () => groupArgs(name, { description: v }), {
          restore: [key('groups', name, 'description'), old],
        }),
    });
    grid.append(boxWrap, field('Description', desc, { wide: true }));
    if (!counted) li.classList.add('setup-item-off');
    li.append(head);
    if (totals) {
      const stats = el('div', 'setup-stats');
      stats.append(
        stat('Full', totals.full, 'Every tagged line'),
        stat('Counted', totals.active, 'Tagged lines that count in the budget'),
      );
      li.append(stats);
    }
    li.append(grid);
    finishItem(li, ik, issuesFor('group', name), null);
    ul.append(li);
  });
  if (items.length) panel.append(ul);

  panel.append(
    addForm('groups', {
      title: 'New group',
      fields: [addInput('groups', 'name', { label: 'Name', placeholder: 'Second unit' })],
      validate: (v) => {
        const bad = checkName(v.name.trim(), b.groups, 'group');
        return bad ? ['name', bad] : null;
      },
      build: (v) => ['set_group', { name: v.name.trim(), group: { description: '', in_budget_total: true } }],
    }),
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
  const items = entries(b.units);
  const names = items.map(([n]) => n);
  if (!items.length) panel.append(emptyNote('No units yet.'));

  const ul = list('Units');
  items.forEach(([name, u0], i) => {
    const u = u0 && typeof u0 === 'object' ? u0 : {};
    const ik = key('units', name);
    const li = itemShell(ik, 'setup-block');
    const head = el('div', 'setup-head');
    const nameEl = el('span', 'setup-name', name);
    nameEl.title = name;
    head.append(
      nameEl,
      el('span', 'setup-figs'),
      deleteButton({
        itemKey: ik,
        name,
        what: 'unit',
        confirmText: `Delete unit ${name}? Lines keep the name, but it stops converting for flat fringes.`,
        build: () => ['delete_unit', { name }],
        neighborKey: neighbor('units', names, i, key('add', 'units', 'name')),
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
    addForm('units', {
      title: 'New unit',
      fields: [
        addInput('units', 'name', { label: 'Name', placeholder: 'Night' }),
        addInput('units', 'plural', { label: 'Plural', placeholder: 'Nights' }),
        addInput('units', 'hours', { label: 'Hours', placeholder: 'None', num: true }),
      ],
      validate: (v) => {
        const bad = checkName(v.name.trim(), b.units, 'unit');
        if (bad) return ['name', bad];
        const h = v.hours.trim();
        if (h !== '' && !DECIMAL.test(h)) return ['hours', 'Hours must be a number, like 10 or 7.5.'];
        return null;
      },
      build: (v) => [
        'set_unit',
        {
          name: v.name.trim(),
          unit: { plural: v.plural.trim(), description: '', hours: v.hours.trim() === '' ? null : v.hours.trim() },
        },
      ],
    }),
  );
}

// ---------------------------------------------------------------------------
// Currencies

function currencyArgs(code, patch) {
  const c = own(budget().currencies, code) || {};
  return [
    'set_currency',
    {
      code,
      currency: {
        description: str(c.description),
        symbol: str(c.symbol),
        rate: str(c.rate),
        ...patch,
      },
    },
  ];
}

function renderCurrencies(panel) {
  const b = budget();
  const base = baseCode();
  const intro = el('div', 'setup-intro');
  const baseLine = el('p', 'setup-base');
  baseLine.append(el('span', 'setup-muted', 'Base currency '), el('strong', null, base || 'not set'));
  intro.append(
    baseLine,
    el(
      'p',
      null,
      `Totals are worked out in ${base || 'the base currency'}. A rate is how many ${base || 'base units'} one unit of the other currency is worth.`,
    ),
  );
  panel.append(intro);
  const items = entries(b.currencies);
  const names = items.map(([n]) => n);
  if (!items.length) panel.append(emptyNote('No other currencies yet.'));

  const ul = list('Currencies');
  items.forEach(([code, c0], i) => {
    const c = c0 && typeof c0 === 'object' ? c0 : {};
    const ik = key('currencies', code);
    const li = itemShell(ik, 'setup-block');
    const head = el('div', 'setup-head');
    const nameEl = el('span', 'setup-name', code);
    nameEl.title = code;
    const figs = el('span', 'setup-figs');
    const direction = el('span', 'setup-muted', str(c.rate) === '' ? '' : `1 ${code} = ${formatDecimal(c.rate)} ${base}`);
    direction.id = nextId();
    figs.append(direction);
    head.append(
      nameEl,
      figs,
      deleteButton({
        itemKey: ik,
        name: code,
        what: 'currency',
        confirmText: `Delete currency ${code}? It can\u2019t be deleted while lines, charges or the totals use it.`,
        build: () => ['delete_currency', { code }],
        neighborKey: neighbor('currencies', names, i, key('add', 'currencies', 'code')),
      }),
    );
    const grid = el('div', 'setup-fields');
    const rate = textInput({
      fieldKey: key('currencies', code, 'rate'),
      value: str(c.rate),
      num: true,
      onCommit: (v, old) =>
        run(ik, () => currencyArgs(code, { rate: v.trim() }), {
          restore: [key('currencies', code, 'rate'), old],
        }),
    });
    const sym = textInput({
      fieldKey: key('currencies', code, 'symbol'),
      value: str(c.symbol),
      onCommit: (v, old) =>
        run(ik, () => currencyArgs(code, { symbol: v }), { restore: [key('currencies', code, 'symbol'), old] }),
    });
    const desc = textInput({
      fieldKey: key('currencies', code, 'description'),
      value: str(c.description),
      onCommit: (v, old) =>
        run(ik, () => currencyArgs(code, { description: v }), {
          restore: [key('currencies', code, 'description'), old],
        }),
    });
    rate.setAttribute('aria-describedby', direction.id);
    grid.classList.add('setup-fields-3');
    grid.append(field('Rate', rate), field('Symbol', sym), field('Description', desc));
    li.append(head, grid);
    finishItem(li, ik, issuesFor('currency', code), rate);
    ul.append(li);
  });
  if (items.length) panel.append(ul);

  panel.append(
    addForm('currencies', {
      title: 'New currency',
      fields: [
        addInput('currencies', 'code', { label: 'Code', placeholder: 'EUR' }),
        addInput('currencies', 'rate', { label: `Rate (${base || 'base'} per 1)`, placeholder: '1.08', num: true }),
      ],
      validate: (v) => {
        const code = v.code.trim();
        if (code === '') return ['code', 'Enter a currency code.'];
        if (code.includes(',')) return ['code', 'Codes can’t contain commas.'];
        if (code === base) return ['code', `${code} is the base currency.`];
        if (own(b.currencies, code) !== undefined) return ['code', `${code} is already in the list.`];
        const rate = v.rate.trim();
        if (rate === '') return ['rate', 'Enter a rate.'];
        if (!DECIMAL.test(rate) || Number(rate) <= 0) return ['rate', 'The rate must be a number above 0, like 1.08.'];
        return null;
      },
      build: (v) => [
        'set_currency',
        { code: v.code.trim(), currency: { description: '', symbol: '', rate: v.rate.trim() } },
      ],
    }),
  );
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
