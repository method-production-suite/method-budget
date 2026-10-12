// The tools panel's Budget tab and its own tabs: charges, credits, budget
// info, settings (currencies, fringe posting, account numbers, new lines) and
// the baseline.
//
// tools.js draws the Budget tab's tab strip; setup.js owns the command queue
// and focus handling and passes its helpers in as `kit`, so every tab looks
// and behaves the same. Budget text only reaches the page through
// textContent and input values, never innerHTML. (Sets and locations are
// lists in apply.js.)

import { displaySettingsGroup } from './display-settings.js';
import { invoke } from './api.js';
import { applyCoding, pendingRenumber, previewBox } from './renumber.js';
import { obj } from './util.js';

const DECIMAL = /^[+-]?(\d+\.?\d*|\.\d+)$/;
const DATE = /^\d{4}-\d{2}-\d{2}$/;
const SEPARATORS = [
  ['-', 'Hyphen  11-01'],
  ['.', 'Point  11.01'],
  [' ', 'Space  11 01'],
  ['/', 'Slash  11/01'],
  [':', 'Colon  11:01'],
  [',', 'Comma  11,01'],
  [';', 'Semicolon  11;01'],
  ['', 'None  1101'],
];

let k = null; // the kit from setup.js, set on every render

// <details> sections that are open, by key, so a re-render keeps them open.
const openSections = new Set();

export function renderExtraTab(tab, panel, kit) {
  k = kit;
  const render = {
    charges: renderCharges,
    credits: renderCredits,
    info: renderInfo,
    settings: renderSettings,
    baseline: renderBaseline,
  }[tab];
  if (render) render(panel);
}

// ---------------------------------------------------------------------------
// Small helpers

function arr(v) {
  return Array.isArray(v) ? v : [];
}

function isNone(v) {
  return v === undefined || v === null;
}

function intro(text) {
  return k.el('p', 'setup-intro', text);
}

function heading(text) {
  return k.el('h3', 'tss-heading', text);
}

function issuesAt(kind, index) {
  const list = k.view() && k.view().issues;
  if (!Array.isArray(list)) return [];
  return list.filter((i) => i && i.target && i.target.kind === kind && i.target.index === index);
}

// The report entry for item `index` of `list`: reports only list applied items.
function appliedEntry(items, reported, index) {
  if (!Array.isArray(reported)) return undefined;
  if (obj(items[index]).applied === false) return undefined;
  let n = 0;
  for (let i = 0; i < index; i += 1) if (obj(items[i]).applied !== false) n += 1;
  return reported[n];
}

function button(text, cls, fieldKey, onClick, label) {
  const b = k.el('button', cls, text);
  b.type = 'button';
  if (fieldKey) b.dataset.key = fieldKey;
  if (label) {
    b.setAttribute('aria-label', label);
    b.title = label;
  }
  b.addEventListener('click', () => {
    if (k.isBusy() || b.disabled) return;
    onClick();
  });
  return b;
}

function checkbox(fieldKey, checked, labelText, onChange) {
  const box = k.el('input', 'setup-check');
  box.type = 'checkbox';
  box.id = k.nextId();
  box.checked = checked;
  box.dataset.key = fieldKey;
  box.addEventListener('change', () => onChange(box.checked));
  const label = k.el('label', 'setup-check-label', labelText);
  label.htmlFor = box.id;
  const wrap = k.el('div', 'setup-field setup-check-row');
  wrap.append(box, label);
  return wrap;
}

// Up and down buttons for an ordered list. Focus follows the moved item.
function moveButtons(tab, index, count, what, name, command) {
  const wrap = k.el('span', 'tss-move');
  const go = (to) => {
    k.setFocusKey(k.key(tab, to, to === 0 ? 'down' : 'up'));
    k.run(k.key(tab, index), () => [command, { index, to }], {
      onError: () => k.setFocusKey(null),
    });
  };
  const up = button('↑', 'tss-move-btn', k.key(tab, index, 'up'), () => go(index - 1), `Move ${what} ${name} up`);
  const down = button('↓', 'tss-move-btn', k.key(tab, index, 'down'), () => go(index + 1), `Move ${what} ${name} down`);
  up.disabled = index === 0;
  down.disabled = index >= count - 1;
  wrap.append(up, down);
  return wrap;
}

// A collapsible section that stays open across re-renders.
function section(sectionKey, title, ...children) {
  const d = k.el('details', 'tss-more');
  d.open = openSections.has(sectionKey);
  const s = k.el('summary', 'tss-more-title', title);
  s.dataset.key = k.key('section', sectionKey);
  d.addEventListener('toggle', () => {
    if (d.open) openSections.add(sectionKey);
    else openSections.delete(sectionKey);
  });
  d.append(s, ...children);
  return d;
}

// Chips for a set of names picked from `options`, with a select to add one.
function chips({ fieldKey, label, selected, options, onChange, emptyText = 'None' }) {
  const wrap = k.el('div', 'setup-field setup-field-wide');
  const head = k.el('span', 'setup-label', label);
  head.id = k.nextId();
  const row = k.el('div', 'tss-chips');
  row.setAttribute('role', 'list');
  row.setAttribute('aria-labelledby', head.id);
  if (!selected.length) row.append(k.el('span', 'tss-chips-empty', emptyText));
  let addSel = null;
  selected.forEach((name) => {
    const chip = k.el('span', 'tss-chip');
    chip.setAttribute('role', 'listitem');
    chip.append(k.el('span', 'tss-chip-text', name));
    chip.append(
      button(
        '×',
        'tss-chip-x',
        k.key(fieldKey, 'remove', name),
        () => {
          // The chip goes away; keep focus in this field.
          if (addSel) addSel.focus({ preventScroll: true });
          onChange(selected.filter((n) => n !== name));
        },
        `Remove ${name}`,
      ),
    );
    row.append(chip);
  });
  const left = options.filter((o) => !selected.includes(o));
  if (left.length) {
    const sel = k.el('select', 'setup-input setup-select tss-chip-add');
    addSel = sel;
    sel.id = k.nextId();
    sel.dataset.key = k.key(fieldKey, 'add');
    sel.setAttribute('aria-label', `Add to ${label}`);
    const first = k.el('option', null, 'Add…');
    first.value = '';
    sel.append(first);
    for (const o of left) {
      const opt = k.el('option', null, o);
      opt.value = o;
      sel.append(opt);
    }
    sel.addEventListener('change', () => {
      if (sel.value === '' || k.isBusy()) {
        sel.value = '';
        return;
      }
      onChange([...selected, sel.value]);
    });
    row.append(sel);
  }
  wrap.append(head, row);
  return wrap;
}

// A yes/no box under an item, in the same style as setup.js's delete confirm.
function confirmBox(itemKey, opener, text, yesText, danger, build) {
  const li = k.findByData('item', itemKey);
  if (!li) return;
  const open = li.querySelector(':scope > .setup-confirm');
  if (open) {
    open.querySelector('button').focus();
    return;
  }
  const box = k.el('div', 'setup-confirm');
  box.setAttribute('role', 'group');
  const p = k.el('p', 'setup-confirm-text', text);
  p.id = k.nextId();
  box.setAttribute('aria-describedby', p.id);
  const yes = k.el('button', danger ? 'setup-btn setup-btn-danger' : 'setup-btn setup-btn-primary', yesText);
  yes.type = 'button';
  const no = k.el('button', 'setup-btn setup-btn-quiet', 'Cancel');
  no.type = 'button';
  const close = () => {
    box.remove();
    opener.focus();
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
    if (k.isBusy()) return;
    k.run(itemKey, build);
  });
  const actions = k.el('div', 'setup-confirm-actions');
  actions.append(yes, no);
  box.append(p, actions);
  li.append(box);
  yes.focus();
}

function groupNames() {
  return k.entries(k.budget().groups).map(([n]) => n);
}

function fringeNames() {
  return k.entries(k.budget().fringes).map(([n]) => n);
}

function categoryCodes() {
  return arr(k.budget().topsheet)
    .filter((r) => obj(r).kind === 'category')
    .map((r) => k.str(r.code));
}

// Category codes with their names, in topsheet order.
function categoryNames() {
  const out = new Map();
  for (const r of arr(k.budget().topsheet)) {
    if (obj(r).kind === 'category') out.set(k.str(r.code), k.str(r.name));
  }
  return out;
}

// Account codes with their names, in topsheet order.
function accountNames() {
  const out = new Map();
  for (const r of arr(k.budget().topsheet)) {
    if (obj(r).kind !== 'category') continue;
    for (const a of arr(r.accounts)) out.set(k.str(obj(a).code), k.str(obj(a).name));
  }
  return out;
}

// Names already picked stay listed even if they no longer exist.
function withKept(options, picked) {
  const out = [...options];
  for (const p of picked) if (!out.includes(p)) out.push(p);
  return out;
}

// ---------------------------------------------------------------------------
// Charges and credits: a narrow list of the items at the left, the picked
// item's editor at the right. Every item's editor is drawn and only the
// picked one shows, so picking another needs no redraw.

// The picked item of each tab, by index, kept across redraws; and the index
// a newly added item will have, picked once it shows up.
const picked = { charges: 0, credits: 0 };
const pickNew = { charges: null, credits: null };

/**
 * Draws the list-and-editor layout of `tab` into `panel`. `items` are
 * { name, applied, figure, title, bad, editor }, in order; `add` is the add
 * form under the list; `move` the reorder command; `deleteText(i)` and
 * `deleteBuild(i)` the delete confirm's question and command; `onShow(i)`
 * runs when item i's editor shows.
 */
function listEditor(panel, { tab, what, label, items, add, empty, move, deleteText, deleteBuild, onShow }) {
  panel.classList.add('tss-fill');
  const count = items.length;
  if (pickNew[tab] !== null && pickNew[tab] < count) {
    picked[tab] = pickNew[tab];
    pickNew[tab] = null;
  }
  let at = count ? Math.min(Math.max(0, Number(picked[tab]) || 0), count - 1) : 0;
  picked[tab] = at;

  const root = k.el('div', 'tss-md');
  const side = k.el('div', 'tss-side');
  const main = k.el('div', 'tss-main');

  // Reorder and delete act on the picked item.
  const strip = k.el('div', 'tss-strip');
  const up =button('↑', 'tss-move-btn', k.key(tab, 'up'), () => moveBy(-1));
  const down = button('↓', 'tss-move-btn', k.key(tab, 'down'), () => moveBy(1));
  const del = button('Delete', 'setup-btn tss-small tss-strip-end', k.key('del', tab), () => confirmDelete());
  strip.append(up, down, del);

  // Moves the picked item by `by`; focus goes to `focusKey`, or stays on
  // the arrow pressed while it can still move that way.
  const moveBy = (by, focusKey) => {
    const from = at;
    const to = at + by;
    if (!count || to < 0 || to >= count) return;
    k.setFocusKey(focusKey || k.key(tab, to === 0 ? 'down' : to === count - 1 ? 'up' : by < 0 ? 'up' : 'down'));
    k.run(
      k.key(tab, from),
      () => {
        picked[tab] = to;
        return [move, { index: from, to }];
      },
      {
        onError: () => {
          picked[tab] = from;
          k.setFocusKey(null);
        },
      },
    );
  };

  // The delete question, under the strip, as setup.js asks it for a list item.
  const confirmDelete = () => {
    if (!count) return;
    const open = side.querySelector(':scope > .setup-confirm');
    if (open) {
      open.querySelector('.setup-btn-danger').focus();
      return;
    }
    const i = at;
    const ik = k.key(tab, i);
    const box = k.el('div', 'setup-confirm tss-confirm');
    box.setAttribute('role', 'group');
    box.setAttribute('aria-label', `Delete ${what} ${items[i].name}`);
    const text = k.el('p', 'setup-confirm-text', deleteText(i));
    text.id = k.nextId();
    const yes = k.el('button', 'setup-btn setup-btn-danger', 'Delete');
    yes.type = 'button';
    yes.dataset.key = k.key('del-yes', ik);
    yes.setAttribute('aria-describedby', text.id);
    const no = k.el('button', 'setup-btn setup-btn-quiet', 'Cancel');
    no.type = 'button';
    no.dataset.key = k.key('del-no', ik);
    const close = () => {
      box.remove();
      del.focus();
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
      if (k.isBusy()) return;
      // The item that takes its place gets focus, else the add form.
      k.setFocusKey(count > 1 ? k.key(tab, 'pick', Math.min(i, count - 2)) : k.key('add', tab, 'name'));
      k.run(ik, () => deleteBuild(i), {
        onError: () => {
          k.setFocusKey(null);
          box.remove();
          const b = k.findByData('key', k.key('del', tab));
          if (b) b.focus();
        },
      });
    });
    const actions = k.el('div', 'setup-confirm-actions');
    actions.append(yes, no);
    box.append(text, actions);
    strip.after(box);
    yes.focus();
  };

  const ul = k.el('ul', 'tss-pick');
  ul.setAttribute('aria-label', label);
  const buttons = items.map((it, n) => {
    const li = k.el('li', 'tss-pick-item');
    if (!it.applied) li.classList.add('is-off');
    if (it.bad) li.classList.add('is-bad');
    const b = k.el('button', 'tss-pick-btn');
    b.type = 'button';
    b.dataset.key = k.key(tab, 'pick', n);
    b.dataset.nobusy = '1';
    b.title = it.title;
    b.setAttribute('aria-keyshortcuts', 'Alt+Shift+ArrowUp Alt+Shift+ArrowDown');
    b.append(k.el('span', 'tss-pick-name', it.name), k.el('span', it.applied ? 'tss-pick-fig setup-num' : 'tss-pick-fig', it.figure));
    if (it.bad) b.append(k.el('span', 'setup-sr', `, ${what} has a problem`));
    b.addEventListener('click', () => pick(n));
    b.addEventListener('keydown', (e) => listKey(e, n));
    li.append(b);
    ul.append(li);
    return b;
  });

  // Arrows pick the next or previous item; Option+Shift+arrows move it.
  const listKey = (e, n) => {
    if (e.altKey && e.shiftKey && !e.metaKey && !e.ctrlKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown')) {
      e.preventDefault();
      e.stopPropagation();
      if (k.isBusy()) return;
      if (n !== at) pick(n);
      const by = e.key === 'ArrowUp' ? -1 : 1;
      moveBy(by, k.key(tab, 'pick', n + by));
      return;
    }
    if (e.altKey || e.metaKey || e.ctrlKey || e.shiftKey) return;
    const j = { ArrowDown: n + 1, ArrowUp: n - 1, Home: 0, End: count - 1 }[e.key];
    if (j === undefined) return;
    e.preventDefault();
    e.stopPropagation();
    if (j < 0 || j >= count) return;
    pick(j);
    buttons[j].focus();
  };

  const sync = () => {
    items.forEach((it, n) => {
      it.editor.hidden = n !== at;
      buttons[n].setAttribute('aria-current', n === at ? 'true' : 'false');
      buttons[n].parentElement.classList.toggle('is-picked', n === at);
    });
    const name = count ? items[at].name : '';
    up.disabled = !count || at === 0;
    down.disabled = !count || at >= count - 1;
    del.disabled = !count;
    const labels = [
      [up, `Move ${what} ${name} up`],
      [down, `Move ${what} ${name} down`],
      [del, `Delete ${what} ${name}`],
    ];
    for (const [b, text] of labels) {
      b.setAttribute('aria-label', text);
      b.title = text;
    }
  };

  const pick = (n) => {
    if (n === at) return;
    at = n;
    picked[tab] = n;
    const open = side.querySelector(':scope > .setup-confirm');
    if (open) open.remove();
    sync();
    if (onShow) onShow(n);
  };

  side.append(strip, ul, add);
  if (!count) main.append(k.emptyNote(empty));
  items.forEach((it, n) => {
    it.editor.id = k.nextId();
    buttons[n].setAttribute('aria-controls', it.editor.id);
    main.append(it.editor);
  });
  sync();
  if (count && onShow) onShow(at);
  root.append(side, main);
  panel.append(root);
}

// An item's editor: errors and problems go at its end (setup.js finds it by
// data-item).
function editorShell(itemKey, label) {
  const ed = k.el('section', 'tss-ed');
  ed.dataset.item = itemKey;
  ed.setAttribute('aria-label', label);
  return ed;
}

// A line of the editor's head: fields side by side, wrapping when narrow.
function hrow(cls, ...children) {
  const row = k.el('div', cls ? `tss-hrow ${cls}` : 'tss-hrow');
  row.append(...children);
  return row;
}

// A field with its label at its left. `size` is a width class for the
// control; `grow` lets it take the line's spare room; `extra` is a trailing
// unit like "%".
function inlineField(labelText, control, { extra = '', grow = false, size = '' } = {}) {
  const wrap = k.el('div', grow ? 'tss-if tss-if-grow' : 'tss-if');
  const label = k.el('label', 'tss-if-label', labelText);
  label.htmlFor = control.id;
  if (size) control.classList.add(size);
  wrap.append(label, control);
  if (extra) {
    const tag = k.el('span', 'setup-adorn', extra);
    tag.setAttribute('aria-hidden', 'true');
    wrap.append(tag);
  }
  return wrap;
}

// A labelled figure for a summary line.
function figure(labelText, valueEl) {
  const f = k.el('span', 'tss-fig');
  f.append(k.el('span', 'tss-fig-label', labelText), valueEl);
  return f;
}

// A small action strip over a table, with a title at its left.
function actionStrip(title, ...controls) {
  const strip = k.el('div', 'tss-strip');
  strip.append(k.el('span', 'tss-strip-title', title), ...controls);
  return strip;
}

// A select that reads as a small button: picking an option runs
// `onPick(value)` and the select goes back to its title.
function addPicker(fieldKey, title, options, noneText, onPick) {
  const sel = k.el('select', 'setup-input setup-select tss-picker');
  sel.id = k.nextId();
  sel.dataset.key = fieldKey;
  sel.setAttribute('aria-label', title.replace('…', ''));
  const first = k.el('option', null, title);
  first.value = '';
  sel.append(first);
  if (!options.length) {
    const none = k.el('option', null, noneText);
    none.value = '';
    none.disabled = true;
    sel.append(none);
  }
  for (const [v, text] of options) {
    const o = k.el('option', null, text);
    o.value = v;
    sel.append(o);
  }
  sel.addEventListener('change', () => {
    const v = sel.value;
    sel.value = '';
    if (v === '' || k.isBusy()) return;
    onPick(v);
  });
  return sel;
}

// A table cell holding `content` (text, or a control drawn as a cell).
function cell(content, cls) {
  const td = k.el('td', cls);
  if (content instanceof Node) {
    if (content.classList && content.classList.contains('setup-input')) {
      content.classList.add('tss-cell');
      td.classList.add('tss-td-in');
    }
    td.append(content);
  } else if (content !== undefined && content !== null) {
    td.textContent = String(content);
  }
  return td;
}

// A grid table with `cols` ([class, heading, align right]) and a hidden
// heading for the delete column at the end.
function gridTable(cls, label, cols) {
  const table = k.el('table', `tss-grid ${cls}`);
  table.setAttribute('aria-label', label);
  const group = k.el('colgroup');
  const head = k.el('tr');
  for (const [c, text, right] of cols) {
    group.append(k.el('col', c));
    const th = k.el('th', right ? 'tss-r' : null, text);
    th.scope = 'col';
    head.append(th);
  }
  group.append(k.el('col', 'tss-c-del'));
  const delHead = k.el('th');
  delHead.append(k.el('span', 'setup-sr', 'Delete'));
  head.append(delHead);
  const thead = k.el('thead');
  thead.append(head);
  table.append(group, thead);
  return table;
}

// A table in a box that takes the editor's spare height and scrolls.
function gridWrap(table) {
  const wrap = k.el('div', 'tss-gridwrap');
  wrap.append(table);
  return wrap;
}

// A body row standing in for an empty table.
function emptyRow(colspan, text) {
  const tr = k.el('tr', 'tss-grid-empty');
  const td = k.el('td', 'setup-muted', text);
  td.colSpan = colspan;
  tr.append(td);
  return tr;
}

// Cents (numbers or BigInt) added up exactly.
function centsSum(list) {
  let total = 0n;
  for (const v of list) {
    try {
      total += BigInt(v);
    } catch {
      return null;
    }
  }
  return total;
}

// Decimal strings ("1307.375") added up exactly; null when one isn't a number.
function decimalSum(list) {
  const parts = [];
  let scale = 0;
  for (const v of list) {
    const m = /^(-?)(\d+)(?:\.(\d+))?$/.exec(k.str(v));
    if (!m) return null;
    parts.push(m);
    scale = Math.max(scale, (m[3] || '').length);
  }
  let total = 0n;
  for (const m of parts) {
    const n = BigInt(m[2] + (m[3] || '').padEnd(scale, '0'));
    total += m[1] ? -n : n;
  }
  const neg = total < 0n;
  const digits = (neg ? -total : total).toString().padStart(scale + 1, '0');
  const text = scale ? `${digits.slice(0, -scale)}.${digits.slice(-scale)}` : digits;
  return neg ? `-${text}` : text;
}

// ---------------------------------------------------------------------------
// Charges

function chargeInput(c0) {
  const c = obj(c0);
  return {
    name: k.str(c.name),
    account: isNone(c.account) ? null : k.str(c.account),
    applied: c.applied !== false,
    kind: c.kind === 'flat' ? 'flat' : 'percent',
    percent: isNone(c.percent) ? null : k.str(c.percent),
    amount: isNone(c.amount) ? null : k.centsToText(c.amount),
    currency: isNone(c.currency) ? null : k.str(c.currency),
    excluded_amounts: arr(c.excluded_amounts).map((x) => ({
      description: k.str(obj(x).description),
      amount: k.centsToText(obj(x).amount),
    })),
    excluded_groups: arr(c.excluded_groups).map(k.str),
    excluded_categories: arr(c.excluded_categories).map(k.str),
    excluded_accounts: arr(c.excluded_accounts).map(k.str),
    label_percent: c.label_percent === true,
    label_excluded: c.label_excluded === true,
  };
}

// set_charge for charge `index`, from the newest view, with `patch` applied.
// `patch` may be a function of the current input.
function chargeArgs(index, patch) {
  const cur = chargeInput(arr(k.budget().charges)[index]);
  const p = typeof patch === 'function' ? patch(cur) : patch;
  return ['set_charge', { index, charge: { ...cur, ...p } }];
}

function currencyOptions(current) {
  const base = k.baseCode();
  const codes = k.entries(k.budget().currencies).map(([c]) => c);
  if (current && current !== base && !codes.includes(current)) codes.push(current);
  return [['', base ? `${base} (base)` : 'Base currency'], ...codes.map((c) => [c, c])];
}

// Per charge, what fills its editor's left-out totals when it shows.
let chargeLoaders = [];

function renderCharges(panel) {
  const b = k.budget();
  const r = k.report();
  const charges = arr(b.charges);
  chargeLoaders = [];
  panel.append(
    intro(
      'Fees worked out on the above- plus below-the-line total, like contingency or insurance. Each one is figured on that total alone, never on another charge.',
    ),
  );
  const items = charges.map((c0, i) => {
    const c = chargeInput(c0);
    const entry = appliedEntry(charges, r && r.charges, i);
    const issues = issuesAt('charge', i);
    const fig = !c.applied ? 'Not applied' : entry ? k.formatCents(entry.amount) : '';
    const figTitle = !c.applied ? 'Not applied' : entry ? `Total charge, ${chargeTitle(entry, false).toLowerCase()}` : '';
    return {
      name: c.name,
      applied: c.applied,
      figure: fig,
      title: [c.name, figTitle, issues.length ? 'This charge has a problem' : ''].filter(Boolean).join('\n'),
      bad: issues.length > 0,
      editor: chargeEditor(i, c0, c, entry, issues),
    };
  });

  const taken = {};
  for (const c of charges) taken[k.str(obj(c).name)] = true;
  const add = k.addForm('charges', {
    title: 'New charge',
    fields: [
      k.addInput('charges', 'name', { label: 'Name', placeholder: 'Insurance' }),
      k.addSelect('charges', 'kind', 'Type', [
        ['percent', 'Percent'],
        ['flat', 'Flat fee'],
      ]),
      k.addInput('charges', 'value', { label: 'Percent or amount', placeholder: '10', num: true }),
    ],
    validate: (v) => {
      const name = v.name.trim();
      if (name === '') return ['name', 'Enter a charge name.'];
      if (taken[name]) return ['name', `There is already a charge named ${name}.`];
      const val = v.value.trim();
      if (val === '') return ['value', v.kind === 'flat' ? 'Enter an amount.' : 'Enter a percent.'];
      if (v.kind !== 'flat' && !DECIMAL.test(val)) return ['value', 'The percent must be a number, like 10 or 2.5.'];
      return null;
    },
    build: (v) => {
      const flat = v.kind === 'flat';
      // The new charge is picked once it shows up.
      pickNew.charges = arr(k.budget().charges).length;
      return [
        'set_charge',
        {
          index: null,
          charge: {
            name: v.name.trim(),
            account: null,
            applied: true,
            kind: flat ? 'flat' : 'percent',
            percent: flat ? null : v.value.trim(),
            amount: flat ? v.value.trim() : null,
            currency: null,
            excluded_amounts: [],
            excluded_groups: [],
            excluded_categories: [],
            excluded_accounts: [],
            label_percent: false,
            label_excluded: false,
          },
        },
      ];
    },
  });

  listEditor(panel, {
    tab: 'charges',
    what: 'charge',
    label: 'Charges',
    items,
    add,
    empty: 'No charges yet.',
    move: 'move_charge',
    deleteText: (i) => `Delete charge ${items[i].name}?`,
    deleteBuild: (i) => ['delete_charge', { index: i }],
    onShow: (i) => {
      if (chargeLoaders[i]) chargeLoaders[i]();
    },
  });
}

// Charge `i`'s editor: name and Applied, then type, value and account, then
// its total and the topsheet label choices, then what it leaves out.
function chargeEditor(i, c0, c, entry, issues) {
  const ik = k.key('charges', i);
  const fk = (f) => k.key('charges', i, f);
  const commit = (f, patchOf) => (v, old) =>
    k.run(ik, () => chargeArgs(i, patchOf(v)), { restore: [fk(f), old] });
  const ed = editorShell(ik, `Charge ${c.name}`);

  const name = k.textInput({ fieldKey: fk('name'), value: c.name, onCommit: commit('name', (v) => ({ name: v })) });
  const applied = checkbox(fk('applied'), c.applied, 'Applied', (v) =>
    k.run(ik, () => chargeArgs(i, { applied: v }), { restore: [fk('applied'), !v] }),
  );
  ed.append(hrow('', inlineField('Name', name, { grow: true }), applied));

  const kind = k.selectInput({
    fieldKey: fk('kind'),
    value: c.kind,
    options: [
      ['percent', 'Percent of the total'],
      ['flat', 'Flat fee'],
    ],
    onCommit: commit('kind', (v) => (cur) =>
      v === 'flat'
        ? { kind: 'flat', amount: cur.amount === null ? '0' : cur.amount }
        : { kind: 'percent', percent: cur.percent === null ? '0' : cur.percent },
    ),
  });
  const values = hrow('', inlineField('Type', kind, { size: 'tss-w-kind' }));
  if (c.kind === 'percent') {
    const pct = k.textInput({
      fieldKey: fk('percent'),
      value: k.str(c.percent),
      num: true,
      onCommit: commit('percent', (v) => ({ percent: v.trim() })),
    });
    livePreview(pct, i);
    values.append(inlineField('Percent', pct, { extra: '%', size: 'tss-w-pct' }));
  } else {
    const amount = k.textInput({
      fieldKey: fk('amount'),
      value: isNone(c0.amount) ? '' : k.formatCents(c0.amount),
      num: true,
      onCommit: commit('amount', (v) => ({ amount: v.trim() })),
    });
    livePreview(amount, i);
    const cur = k.selectInput({
      fieldKey: fk('currency'),
      value: c.currency || '',
      options: currencyOptions(c.currency),
      onCommit: commit('currency', (v) => ({ currency: v === '' ? null : v })),
    });
    values.append(inlineField('Amount', amount, { size: 'tss-w-num' }), inlineField('Currency', cur, { size: 'tss-w-cur' }));
  }
  const account = k.textInput({
    fieldKey: fk('account'),
    value: c.account || '',
    placeholder: 'None',
    onCommit: commit('account', (v) => ({ account: v.trim() === '' ? null : v.trim() })),
  });
  values.append(inlineField('Posts to account', account, { size: 'tss-w-acct' }));
  ed.append(values);

  // The total, and what the topsheet label shows besides the name (display only).
  const sum = hrow('tss-sum');
  if (!c.applied) sum.append(k.el('span', 'setup-muted', 'Not applied'));
  else if (entry) {
    const t = k.el('span', 'setup-num', k.formatCents(entry.amount));
    t.title = chargeTitle(entry, false);
    t.dataset.key = fk('total');
    sum.append(figure('Total charge', t));
  }
  if (c.kind === 'percent') {
    sum.append(
      checkbox(fk('label-percent'), c.label_percent, 'Show the percent on the topsheet', (v) =>
        k.run(ik, () => chargeArgs(i, { label_percent: v }), { restore: [fk('label-percent'), !v] }),
      ),
    );
  }
  sum.append(
    checkbox(fk('label-excluded'), c.label_excluded, 'Show the amount left out on the topsheet', (v) =>
      k.run(ik, () => chargeArgs(i, { label_excluded: v }), { restore: [fk('label-excluded'), !v] }),
    ),
  );
  ed.append(sum);
  ed.append(...chargeExclusions(i, c, ik, entry));
  k.finishItem(ed, ik, issues, null);
  return ed;
}

// The head figure's tooltip: what the charge is figured on.
function chargeTitle(t, typing) {
  const on = `On ${k.formatCents(t.base)}, less ${k.formatCents(t.excluded)} left out`;
  return typing ? `${on} (with the value being typed)` : on;
}

// Live preview: while a charge's percent, amount or a left-out amount is
// being typed, its total (and what it leaves out) is figured from the typed
// text, before Enter commits it. Charges whose total isn't shown (not
// applied) have nothing to update.
const preview = { timer: null, seq: 0 };
const PREVIEW_MS = 120;

function livePreview(input, i) {
  input.addEventListener('input', () => {
    clearTimeout(preview.timer);
    preview.timer = setTimeout(() => runPreview(i), PREVIEW_MS);
  });
}

// Charge `i` as the editor shows it right now: the saved charge with every
// typed-but-not-committed value in its place.
function draftCharge(i) {
  const cur = chargeInput(arr(k.budget().charges)[i]);
  const typed = (f) => {
    const e = k.findByData('key', k.key('charges', i, f));
    return e && typeof e.value === 'string' ? e.value : null;
  };
  if (cur.kind === 'percent') {
    const v = typed('percent');
    if (v !== null) cur.percent = v.trim();
  } else {
    const v = typed('amount');
    if (v !== null) cur.amount = v.trim();
  }
  cur.excluded_amounts = cur.excluded_amounts.map((x, n) => {
    const v = typed(`amount-value-${n}`);
    return v === null ? x : { ...x, amount: v.trim() };
  });
  return cur;
}

async function runPreview(i) {
  const total = k.findByData('key', k.key('charges', i, 'total'));
  if (!total) return;
  const seq = ++preview.seq;
  try {
    const t = await invoke('preview_charge', { index: i, charge: draftCharge(i) });
    if (seq !== preview.seq || !total.isConnected) return;
    total.textContent = k.formatCents(t.amount);
    total.title = chargeTitle(t, true);
    total.classList.remove('tss-stale');
    const out = k.findByData('key', k.key('charges', i, 'excluded'));
    if (out) out.textContent = k.formatCents(t.excluded);
  } catch {
    // Not a number yet: keep the last total, marked as not current.
    if (seq !== preview.seq || !total.isConnected) return;
    total.classList.add('tss-stale');
    total.title = 'Finish typing a number to see the new total';
  }
}

// What leaving out one group, category or account takes off a charge, as
// the totals work it out (each line with its fringes): the saved charge with
// only that exclusion, through preview_charge. The figure doesn't depend on
// the charge, so it is kept per budget revision by what is left out; the
// last one shows until the new one arrives.
const leftOut = { stamp: '', now: new Map(), last: new Map() };

function leftOutTotal(i, f, name, td) {
  const v = obj(k.view());
  const stamp = `${v.doc}:${v.rev}`;
  if (leftOut.stamp !== stamp) {
    leftOut.stamp = stamp;
    leftOut.now = new Map();
  }
  const id = JSON.stringify([v.doc, f, name]);
  if (leftOut.last.has(id)) td.textContent = k.formatCents(leftOut.last.get(id));
  let p = leftOut.now.get(id);
  if (!p) {
    const one = {
      ...chargeInput(arr(k.budget().charges)[i]),
      excluded_amounts: [],
      excluded_groups: [],
      excluded_categories: [],
      excluded_accounts: [],
      [f]: [name],
    };
    p = invoke('preview_charge', { index: i, charge: one }).then((t) => {
      leftOut.last.set(id, t.excluded);
      return t.excluded;
    });
    leftOut.now.set(id, p);
  }
  p.then(
    (cents) => {
      td.textContent = k.formatCents(cents);
    },
    () => {
      td.textContent = '';
    },
  );
}

// What charge `i` leaves out, as one table: amounts typed in, then groups,
// categories and accounts, each with what it takes off. An item the charge
// names that the budget no longer has stays listed until it is removed.
function chargeExclusions(i, c, ik, entry) {
  const fk = (f) => k.key('charges', i, f);
  const rawAmounts = arr(obj(arr(k.budget().charges)[i]).excluded_amounts);
  const groupDefs = obj(k.budget().groups);
  const cats = categoryNames();
  const accts = accountNames();

  const leave = (f, value) =>
    k.run(ik, () => chargeArgs(i, (cur) => ({ [f]: [...cur[f].filter((x) => x !== value), value] })));
  const named = (map, f) =>
    [...map.keys()].filter((code) => !c[f].includes(code)).map((code) => [code, `${code} ${map.get(code)}`.trim()]);
  const strip = actionStrip(
    'Left out',
    button('Exclude amount', 'setup-btn tss-small', fk('amount-add'), () => {
      k.setFocusKey(fk(`amount-desc-${c.excluded_amounts.length}`));
      k.run(ik, () => chargeArgs(i, (cur) => ({ excluded_amounts: [...cur.excluded_amounts, { description: '', amount: '0' }] })), {
        onError: () => k.setFocusKey(null),
      });
    }),
    addPicker(
      fk('excl-add-group'),
      'Exclude group…',
      groupNames()
        .filter((g) => !c.excluded_groups.includes(g))
        .map((g) => [g, g]),
      'No groups left',
      (g) => leave('excluded_groups', g),
    ),
    addPicker(fk('excl-add-category'), 'Exclude category…', named(cats, 'excluded_categories'), 'No categories left', (v) =>
      leave('excluded_categories', v),
    ),
    addPicker(fk('excl-add-account'), 'Exclude account…', named(accts, 'excluded_accounts'), 'No accounts left', (v) =>
      leave('excluded_accounts', v),
    ),
  );

  const table = gridTable('tss-grid-excl', `Left out of ${c.name}`, [
    ['tss-c-type', 'Type'],
    ['tss-c-name', 'Name'],
    ['tss-c-desc', 'Description'],
    ['tss-c-fig', 'Total', true],
  ]);
  const tbody = k.el('tbody');
  // Each row's delete key, so focus can go to a neighbour once one is removed.
  const rows = [];
  const focusAfter = (p) => {
    const next = rows[p + 1];
    const prev = rows[p - 1];
    if (next) return next.amount !== undefined && rows[p].amount !== undefined ? fk(`amount-del-${next.amount - 1}`) : next.del;
    if (prev) return prev.del;
    return fk('amount-add');
  };
  const removeButton = (delKey, label, build) => {
    const p = rows.length;
    return button('×', 'setup-del tss-row-del', delKey, () => {
      k.setFocusKey(focusAfter(p));
      k.run(ik, () => chargeArgs(i, build), { onError: () => k.setFocusKey(null) });
    }, label);
  };

  c.excluded_amounts.forEach((x, n) => {
    const desc = k.textInput({
      fieldKey: fk(`amount-desc-${n}`),
      value: x.description,
      placeholder: 'Description',
      label: `Description of amount ${n + 1}`,
      onCommit: (v, old) =>
        k.run(
          ik,
          () => chargeArgs(i, (cur) => ({ excluded_amounts: cur.excluded_amounts.map((y, j) => (j === n ? { ...y, description: v } : y)) })),
          { restore: [fk(`amount-desc-${n}`), old] },
        ),
    });
    const amt = k.textInput({
      fieldKey: fk(`amount-value-${n}`),
      value: k.formatCents(obj(rawAmounts[n]).amount),
      num: true,
      label: `Amount ${n + 1}`,
      onCommit: (v, old) =>
        k.run(
          ik,
          () => chargeArgs(i, (cur) => ({ excluded_amounts: cur.excluded_amounts.map((y, j) => (j === n ? { ...y, amount: v.trim() } : y)) })),
          { restore: [fk(`amount-value-${n}`), old] },
        ),
    });
    livePreview(amt, i);
    const delKey = fk(`amount-del-${n}`);
    const del = removeButton(delKey, `Remove amount ${n + 1}`, (cur) => ({
      excluded_amounts: cur.excluded_amounts.filter((_, j) => j !== n),
    }));
    rows.push({ del: delKey, amount: n });
    const tr = k.el('tr');
    tr.append(cell('Amount', 'setup-muted'), cell(''), cell(desc), cell(amt), cell(del, 'tss-td-del'));
    tbody.append(tr);
  });

  const loads = [];
  const namedRow = (type, f, name, desc, known, title) => {
    const delKey = fk(`excl-${type}-del-${name}`);
    const del = removeButton(delKey, `Remove ${type} ${name}`, (cur) => ({ [f]: cur[f].filter((x) => x !== name) }));
    rows.push({ del: delKey });
    const total = cell('', 'tss-r setup-num');
    if (known) {
      total.title = title;
      loads.push(() => leftOutTotal(i, f, name, total));
    }
    const descCell = known ? cell(desc) : cell('Not in this budget', 'setup-muted');
    if (known && desc) descCell.title = desc;
    const nameCell = cell(name);
    nameCell.title = name;
    const tr = k.el('tr');
    tr.append(cell(type[0].toUpperCase() + type.slice(1), 'setup-muted'), nameCell, descCell, total, cell(del, 'tss-td-del'));
    tbody.append(tr);
  };
  const usual = 'Its lines with their fringes, as the totals stand now';
  for (const g of c.excluded_groups) {
    const known = Object.prototype.hasOwnProperty.call(groupDefs, g);
    const def = obj(groupDefs[g]);
    const title = def.in_budget_total === false ? 'Left out of the budget total, so nothing to leave out here' : usual;
    namedRow('group', 'excluded_groups', g, k.str(def.description), known, title);
  }
  for (const code of c.excluded_categories) {
    namedRow('category', 'excluded_categories', code, cats.get(code) || '', cats.has(code), usual);
  }
  for (const code of c.excluded_accounts) {
    namedRow('account', 'excluded_accounts', code, accts.get(code) || '', accts.has(code), usual);
  }
  if (!rows.length) tbody.append(emptyRow(5, 'Nothing left out'));
  chargeLoaders[i] = () => loads.forEach((fn) => fn());

  table.append(tbody);
  if (entry) {
    // Everything left out together: each line once, even when several rows
    // pick it.
    const n = k.el('span', 'setup-num', k.formatCents(entry.excluded));
    n.dataset.key = fk('excluded');
    const label = cell('Left out in all');
    label.colSpan = 3;
    label.title = 'Each line counts once, even when several rows pick it';
    const total = cell(n, 'tss-r');
    const tr = k.el('tr');
    tr.append(label, total, cell(''));
    const tfoot = k.el('tfoot');
    tfoot.append(tr);
    table.append(tfoot);
  }
  return [strip, gridWrap(table)];
}

// ---------------------------------------------------------------------------
// Credits

function creditRowInput(r0) {
  const r = obj(r0);
  const src = obj(r.source);
  const cap = obj(r.cap);
  return {
    description: k.str(r.description),
    source:
      src.kind === 'group'
        ? { kind: 'group', group: k.str(src.group) }
        : src.kind === 'category'
          ? { kind: 'category', category: k.str(src.category) }
          : { kind: 'flat', amount: k.centsToText(isNone(src.amount) ? 0 : src.amount) },
    percent: k.str(r.percent) || '0',
    cap:
      cap.kind === 'qualified_spend' || cap.kind === 'credit'
        ? { kind: cap.kind, amount: k.centsToText(cap.amount) }
        : { kind: 'none' },
  };
}

function creditInput(c0) {
  const c = obj(c0);
  return {
    name: k.str(c.name),
    account: isNone(c.account) ? null : k.str(c.account),
    applied: c.applied !== false,
    cap: isNone(c.cap) ? null : k.centsToText(c.cap),
    discount_percent: k.str(c.discount_percent) || '0',
    // One list per credit, for all its group and category rows
    // (decisions.md, entry 57), so changing a row's spend keeps it.
    excluded_fringes: arr(c.excluded_fringes).map(k.str),
    rows: arr(c.rows).map(creditRowInput),
  };
}

function creditArgs(index, patch) {
  const cur = creditInput(arr(k.budget().credits)[index]);
  const p = typeof patch === 'function' ? patch(cur) : patch;
  return ['set_credit', { index, credit: { ...cur, ...p } }];
}

function rowPatch(n, f) {
  return (cur) => ({ rows: cur.rows.map((r, j) => (j === n ? { ...r, ...f(r) } : r)) });
}

function renderCredits(panel) {
  const b = k.budget();
  const r = k.report();
  const credits = arr(b.credits);
  panel.append(
    intro(
      'Incentives and rebates taken off the grand total to give the net total. Each row is a percent of some spend; the row credits add up, then the cap and discount apply.',
    ),
  );
  const items = credits.map((c0, i) => {
    const c = creditInput(c0);
    const entry = appliedEntry(credits, r && r.credits, i);
    const issues = issuesAt('credit', i);
    return {
      name: c.name,
      applied: c.applied,
      figure: !c.applied ? 'Not applied' : entry ? k.formatCents(entry.amount) : '',
      title: [c.name, !c.applied ? 'Not applied' : entry ? 'Credit taken off the grand total' : '', issues.length ? 'This credit has a problem' : '']
        .filter(Boolean)
        .join('\n'),
      bad: issues.length > 0,
      editor: creditEditor(i, c0, c, entry, issues),
    };
  });

  const taken = {};
  for (const c of credits) taken[k.str(obj(c).name)] = true;
  const add = k.addForm('credits', {
    title: 'New credit',
    fields: [k.addInput('credits', 'name', { label: 'Name', placeholder: 'State incentive' })],
    validate: (v) => {
      const name = v.name.trim();
      if (name === '') return ['name', 'Enter a credit name.'];
      if (taken[name]) return ['name', `There is already a credit named ${name}.`];
      return null;
    },
    build: (v) => {
      // The new credit is picked once it shows up.
      pickNew.credits = arr(k.budget().credits).length;
      return [
        'set_credit',
        { index: null, credit: { name: v.name.trim(), account: null, applied: true, cap: null, discount_percent: '0', excluded_fringes: [], rows: [] } },
      ];
    },
  });

  listEditor(panel, {
    tab: 'credits',
    what: 'credit',
    label: 'Credits',
    items,
    add,
    empty: 'No credits yet.',
    move: 'move_credit',
    deleteText: (i) => `Delete credit ${items[i].name} and its ${k.plural(arr(obj(credits[i]).rows).length, 'row', 'rows')}?`,
    deleteBuild: (i) => ['delete_credit', { index: i }],
  });
}

// Credit `i`'s editor: name and Applied; cap, discount and account; the
// fringes left out of its spend; its figures; then its rows as one table.
function creditEditor(i, c0, c, entry, issues) {
  const raw = obj(c0);
  const ik = k.key('credits', i);
  const fk = (f) => k.key('credits', i, f);
  const commit = (f, patchOf) => (v, old) =>
    k.run(ik, () => creditArgs(i, patchOf(v)), { restore: [fk(f), old] });
  const ed = editorShell(ik, `Credit ${c.name}`);

  const name = k.textInput({ fieldKey: fk('name'), value: c.name, onCommit: commit('name', (v) => ({ name: v })) });
  const applied = checkbox(fk('applied'), c.applied, 'Applied', (v) =>
    k.run(ik, () => creditArgs(i, { applied: v }), { restore: [fk('applied'), !v] }),
  );
  ed.append(hrow('', inlineField('Name', name, { grow: true }), applied));

  const cap = k.textInput({
    fieldKey: fk('cap'),
    value: isNone(raw.cap) ? '' : k.formatCents(raw.cap),
    num: true,
    placeholder: 'None',
    onCommit: commit('cap', (v) => ({ cap: v.trim() === '' ? null : v.trim() })),
  });
  const discount = k.textInput({
    fieldKey: fk('discount'),
    value: c.discount_percent,
    num: true,
    onCommit: commit('discount', (v) => ({ discount_percent: v.trim() })),
  });
  const account = k.textInput({
    fieldKey: fk('account'),
    value: c.account || '',
    placeholder: 'None',
    onCommit: commit('account', (v) => ({ account: v.trim() === '' ? null : v.trim() })),
  });
  ed.append(
    hrow(
      '',
      inlineField('Cap on the credit', cap, { size: 'tss-w-num' }),
      inlineField('Discount', discount, { extra: '%', size: 'tss-w-pct' }),
      inlineField('Posts to account', account, { size: 'tss-w-acct' }),
    ),
  );

  const fringes = chips({
    fieldKey: fk('fringes'),
    label: 'Fringes left out of the spend',
    selected: c.excluded_fringes,
    options: withKept(fringeNames(), c.excluded_fringes),
    onChange: (list) => k.run(ik, () => creditArgs(i, { excluded_fringes: list })),
  });
  fringes.classList.add('tss-chiprow');
  ed.append(fringes);

  const sum = hrow('tss-sum');
  if (!c.applied) sum.append(k.el('span', 'setup-muted', 'Not applied'));
  else if (entry) {
    // The discount comes off after the cap; the credit is rounded once.
    const before = dollarsToCents(entry.capped);
    if (before !== null) {
      const d = k.el('span', 'setup-num', k.formatCents(before - BigInt(entry.amount)));
      d.title = `Exactly ${k.formatDecimal(entry.capped)} before the discount, after the cap`;
      sum.append(figure('Discount', d));
    }
    const net = k.el('span', 'setup-num', k.formatCents(entry.amount));
    net.title = 'Credit taken off the grand total';
    sum.append(figure('Net credit', net));
  }
  if (sum.childNodes.length) ed.append(sum);

  const addRow = button('Add row', 'setup-btn tss-small', fk('add-row'), () => {
    k.setFocusKey(k.key('credits', i, c.rows.length, 'description'));
    k.run(
      ik,
      () =>
        creditArgs(i, (cur) => ({
          rows: [...cur.rows, { description: '', source: { kind: 'flat', amount: '0' }, percent: '0', cap: { kind: 'none' } }],
        })),
      { onError: () => k.setFocusKey(null) },
    );
  });
  ed.append(actionStrip('Rows', addRow), gridWrap(creditGrid(i, c, raw, entry, ik)));
  k.finishItem(ed, ik, issues, null);
  return ed;
}

// A credit's rows as one table: what each is a percent of, its figures from
// the totals, its cap, and a totals line under them.
function creditGrid(i, c, raw, entry, ik) {
  const table = gridTable('tss-grid-credit', `Rows of ${c.name}`, [
    ['tss-c-spend', 'Spend'],
    ['tss-c-desc', 'Description'],
    ['tss-c-fig', 'Amount', true],
    ['tss-c-fig', 'Gross', true],
    ['tss-c-fig', 'Fringes out', true],
    ['tss-c-fig', 'Net', true],
    ['tss-c-cap', 'Cap type'],
    ['tss-c-fig', 'Cap', true],
    ['tss-c-pct', '%', true],
    ['tss-c-fig', 'Credit', true],
    ['tss-c-fig', 'Over cap', true],
  ]);
  const totals = entry && Array.isArray(entry.rows) ? entry.rows.map(obj) : [];
  const tbody = k.el('tbody');
  c.rows.forEach((row, n) => tbody.append(creditRow(i, n, row, raw, totals[n] || null, ik, c.rows.length)));
  if (!c.rows.length) tbody.append(emptyRow(12, 'No rows yet'));
  table.append(tbody);

  if (c.rows.length && totals.length) {
    const add = (f) => {
      const s = centsSum(totals.map((t) => (isNone(t[f]) ? 0 : t[f])));
      return s === null ? '' : k.formatCents(s);
    };
    const label = cell('Total');
    label.colSpan = 3;
    const credit = decimalSum(totals.map((t) => t.credit));
    const creditCell = cell(credit === null ? '' : money(credit), 'tss-r setup-num');
    if (credit !== null) creditCell.title = `Exactly ${k.formatDecimal(credit)}, before the credit's cap and discount`;
    const gap = cell('');
    gap.colSpan = 3;
    const tr = k.el('tr');
    tr.append(
      label,
      cell(add('gross'), 'tss-r setup-num'),
      cell(add('excluded_fringes'), 'tss-r setup-num'),
      cell(add('net'), 'tss-r setup-num'),
      gap,
      creditCell,
      cell(''),
      cell(''),
    );
    const tfoot = k.el('tfoot');
    tfoot.append(tr);
    table.append(tfoot);
  }
  return table;
}

function creditRow(i, n, row, raw, rt, ik, count) {
  const tr = k.el('tr');
  const fk = (f) => k.key('credits', i, n, f);
  const send = (f, old) => (v) => k.run(ik, () => creditArgs(i, rowPatch(n, () => v)), old === undefined ? {} : { restore: [f, old] });
  const rawRow = obj(arr(raw.rows)[n]);
  const of = (what) => `${what} of row ${n + 1}`;

  const groups = groupNames();
  const cats = categoryNames();
  const current =
    row.source.kind === 'group' ? `group:${row.source.group}` : row.source.kind === 'category' ? `category:${row.source.category}` : 'flat';
  const catLabel = (code) => (cats.get(code) ? `Category: ${code} ${cats.get(code)}` : `Category: ${code}`);
  const spend = k.selectInput({
    fieldKey: fk('source'),
    value: current,
    options: [
      ['flat', 'Amount typed in'],
      ...withKept(groups, row.source.kind === 'group' ? [row.source.group] : []).map((g) => [`group:${g}`, `Group: ${g}`]),
      // A topsheet category's total with its fringes (decisions.md, entry 57).
      ...withKept([...cats.keys()], row.source.kind === 'category' ? [row.source.category] : []).map((code) => [`category:${code}`, catLabel(code)]),
    ],
    onCommit: (v, old) =>
      send(fk('source'), old)(
        v === 'flat'
          ? { source: { kind: 'flat', amount: '0' } }
          : v.startsWith('category:')
            ? { source: { kind: 'category', category: v.slice(9) } }
            : { source: { kind: 'group', group: v.slice(6) } },
      ),
  });
  spend.setAttribute('aria-label', of('Spend'));
  spend.classList.add('tss-spend');
  spend.title = spend.options[spend.selectedIndex] ? spend.options[spend.selectedIndex].textContent : '';

  const desc = k.textInput({
    fieldKey: fk('description'),
    value: row.description,
    label: of('Description'),
    onCommit: (v, old) => send(fk('description'), old)({ description: v }),
  });
  desc.classList.add('tss-desc');

  const amount =
    row.source.kind === 'flat'
      ? k.textInput({
          fieldKey: fk('amount'),
          value: k.formatCents(obj(rawRow.source).amount || 0),
          num: true,
          label: of('Amount'),
          onCommit: (v, old) => send(fk('amount'), old)({ source: { kind: 'flat', amount: v.trim() } }),
        })
      : '';

  const capKind = k.selectInput({
    fieldKey: fk('cap-kind'),
    value: row.cap.kind,
    options: [
      ['none', 'None'],
      ['qualified_spend', 'Qualified spend'],
      ['credit', 'Credit cap'],
    ],
    onCommit: (v, old) => send(fk('cap-kind'), old)({ cap: v === 'none' ? { kind: 'none' } : { kind: v, amount: row.cap.amount || '0' } }),
  });
  capKind.setAttribute('aria-label', of('Cap type'));
  capKind.classList.add('tss-captype');
  capKind.title = 'Qualified spend caps the spend the percent applies to; Credit cap caps the row’s credit';
  const capAmount =
    row.cap.kind !== 'none'
      ? k.textInput({
          fieldKey: fk('cap-amount'),
          value: k.formatCents(obj(rawRow.cap).amount || 0),
          num: true,
          label: of('Cap'),
          onCommit: (v, old) => send(fk('cap-amount'), old)({ cap: { kind: row.cap.kind, amount: v.trim() } }),
        })
      : '';
  const percent = k.textInput({
    fieldKey: fk('percent'),
    value: row.percent,
    num: true,
    label: of('Percent'),
    onCommit: (v, old) => send(fk('percent'), old)({ percent: v.trim() }),
  });
  percent.classList.add('tss-pct');

  const fig = (v, title) => {
    const td = cell(v, 'tss-r setup-num');
    if (title) td.title = title;
    return td;
  };
  const has = rt && !isNone(rt.net);
  const credit = has ? fig(money(rt.credit), `Exactly ${k.formatDecimal(rt.credit)}; rows are added before rounding`) : fig('');
  const [overText, overTitle] = has ? overCap(rt, row) : ['', ''];

  // Focus goes to the row taking this one's place, else the one before,
  // else Add row.
  const after = n < count - 1 ? fk('del') : n > 0 ? k.key('credits', i, n - 1, 'del') : k.key('credits', i, 'add-row');
  const del = button('×', 'setup-del tss-row-del', fk('del'), () => {
    k.setFocusKey(after);
    k.run(ik, () => creditArgs(i, (cur) => ({ rows: cur.rows.filter((_, j) => j !== n) })), { onError: () => k.setFocusKey(null) });
  }, `Delete row ${n + 1}`);

  tr.append(
    cell(spend),
    cell(desc),
    cell(amount),
    fig(has ? k.formatCents(rt.gross) : '', 'The row’s spend before fringes are left out'),
    fig(has ? k.formatCents(rt.excluded_fringes) : '', 'Fringes left out of the spend'),
    fig(has ? k.formatCents(rt.net) : '', 'The spend the percent applies to'),
    cell(capKind),
    cell(capAmount),
    cell(percent),
    credit,
    fig(overText, overTitle),
    cell(del, 'tss-td-del'),
  );
  return tr;
}

// A credit row against its cap: how far over it is (in parentheses when
// under), and a tooltip that says which and by exactly how much.
function overCap(rt, row) {
  if (isNone(rt.cap_variance) || row.cap.kind === 'none') return ['', ''];
  const what = row.cap.kind === 'qualified_spend' ? 'Spend' : 'Credit';
  const exact = k.formatDecimal(k.str(rt.cap_variance).replace(/^-/, ''));
  const c = dollarsToCents(rt.cap_variance);
  if (c === null) return [k.str(rt.cap_variance), `Exactly ${exact} against the cap`];
  if (c === 0n) return [k.formatCents(0), 'At the cap'];
  const over = c < 0n;
  return [k.formatCents(-c), `${what} ${over ? 'over' : 'under'} the cap by exactly ${exact}`];
}

// Dollars as a decimal string ("1307.375") rounded half away from zero to
// cents, for showing an unrounded credit figure. Null when not a number.
function dollarsToCents(v) {
  const m = /^(-?)(\d+)(?:\.(\d+))?$/.exec(k.str(v));
  if (!m) return null;
  const frac = (m[3] || '').padEnd(3, '0');
  let cents = BigInt(m[2]) * 100n + BigInt(frac.slice(0, 2));
  if (Number(frac[2]) >= 5) cents += 1n;
  return m[1] && cents !== 0n ? -cents : cents;
}

function money(v) {
  const c = dollarsToCents(v);
  return c === null ? k.str(v) : k.formatCents(c);
}

// ---------------------------------------------------------------------------
// Budget info

const INFO_KINDS = [
  ['text', 'Text'],
  ['number', 'Number'],
  ['date', 'Date'],
];

function infoCheck(kind, value) {
  const v = value.trim();
  if (kind === 'number' && !DECIMAL.test(v)) return 'Enter a number, like 30 or 2.5.';
  if (kind === 'date' && !DATE.test(v)) return 'Enter a date as YYYY-MM-DD, like 2027-03-01.';
  return null;
}

function renderInfo(panel) {
  const b = k.budget();
  panel.append(intro('Facts about the production, like the director, shoot days or start date. They are notes only and don’t feed formulas.'));
  const items = k.entries(b.info);
  const names = items.map(([n]) => n);
  if (!items.length) panel.append(k.emptyNote('No fields yet.'));
  const ul = k.list('Budget information');
  items.forEach(([name, v0], i) => {
    const v = obj(v0);
    const kind = ['text', 'number', 'date'].includes(v.kind) ? v.kind : 'text';
    const ik = k.key('info', name);
    const li = k.itemShell(ik, 'setup-block');
    const head = k.el('div', 'setup-head');
    const nameEl = k.el('span', 'setup-name', name);
    nameEl.title = name;
    head.append(
      nameEl,
      k.el('span', 'setup-figs'),
      k.deleteButton({
        itemKey: ik,
        name,
        what: 'field',
        confirmText: `Delete the field ${name}?`,
        build: () => ['delete_info', { name }],
        neighborKey: k.neighbor('info', names, i, k.key('add', 'info', 'name')),
      }),
    );
    const grid = k.el('div', 'setup-fields tss-info');
    const valueKey = k.key('info', name, 'value');
    const value = k.textInput({
      fieldKey: valueKey,
      value: k.str(v.value),
      num: kind === 'number',
      placeholder: kind === 'date' ? 'YYYY-MM-DD' : '',
      onCommit: (val, old) => k.run(ik, () => ['set_info', { name, value: { kind, value: val } }], { restore: [valueKey, old] }),
    });
    const kindSel = k.selectInput({
      fieldKey: k.key('info', name, 'kind'),
      value: kind,
      options: INFO_KINDS,
      onCommit: (nk, old) =>
        k.run(ik, () => ['set_info', { name, value: { kind: nk, value: k.str(obj(k.own(k.budget().info, name)).value) } }], {
          restore: [k.key('info', name, 'kind'), old],
        }),
    });
    grid.append(k.field('Kind', kindSel), k.field('Value', value));
    li.append(head, grid);
    k.finishItem(li, ik, [], null);
    ul.append(li);
  });
  if (items.length) panel.append(ul);
  panel.append(
    k.addForm('info', {
      title: 'New field',
      fields: [
        k.addInput('info', 'name', { label: 'Label', placeholder: 'Director' }),
        k.addSelect('info', 'kind', 'Kind', INFO_KINDS),
        k.addInput('info', 'value', { label: 'Value', placeholder: '' }),
      ],
      validate: (v) => {
        const name = v.name.trim();
        if (name === '') return ['name', 'Enter a label.'];
        if (k.own(b.info, name) !== undefined) return ['name', `There is already a field named ${name}.`];
        const bad = infoCheck(v.kind, v.value);
        return bad ? ['value', bad] : null;
      },
      build: (v) => ['set_info', { name: v.name.trim(), value: { kind: v.kind, value: v.value.trim() } }],
    }),
  );
}

// ---------------------------------------------------------------------------
// Settings: currencies, fringe posting, account numbers

function coding() {
  const c = obj(k.budget().account_coding);
  const w = (f, d) => (Number.isInteger(c[f]) ? c[f] : d);
  return {
    category_width: w('category_width', 2),
    account_width: w('account_width', 2),
    sub_width: w('sub_width', 0),
    set_width: w('set_width', 0),
    separator: typeof c.separator === 'string' ? c.separator : '-',
  };
}

function fit(text, width) {
  if (!width || !text) return '';
  return text.slice(0, width).padStart(width, '0');
}

// An example number in the pattern: category 11, account 1, sub 2, set 3.
function exampleNumber(p) {
  const segs = [fit('11', p.category_width), fit('1', p.account_width), fit('2', p.sub_width), fit('3', p.set_width)];
  return segs.filter((s) => s !== '').join(p.separator);
}

// The pattern rules from method_core::coding, so the form can explain a problem
// before it is applied. The app checks them again.
function codingProblem(p) {
  if (p.category_width < 1) return 'Every account number starts with a category, so its width is at least 1.';
  const w = [p.category_width, p.account_width, p.sub_width, p.set_width];
  const names = ['category', 'account', 'sub-account', 'set'];
  for (let i = 1; i < 4; i += 1) {
    if (w[i] > 0 && w[i - 1] === 0) return `The ${names[i]} part needs the ${names[i - 1]} part before it.`;
  }
  return null;
}

function fringeAccountExample(p, suffix) {
  const first = categoryCodes()[0] || '11';
  if (!p.account_width || !suffix) return null;
  const cat = fit(first, p.category_width);
  return { cat: first, number: [cat, fit(suffix, p.account_width)].join(p.separator) };
}

function renderSettings(panel) {
  const b = k.budget();
  const base = k.baseCode();
  const ik = k.key('settings', 'currency');

  // Currency
  const cur = k.el('div', 'tss-group');
  cur.dataset.item = ik;
  const baseInput = k.textInput({
    fieldKey: k.key('settings', 'base'),
    value: base,
    // Renaming the base currency renames it on every line, charge and
    // setting that names it too (src/rename.rs), as one undo step.
    onCommit: (v, old) =>
      k.run(ik, () => ['rename_setup_item', { kind: 'currency', from: k.baseCode(), to: v.trim() }], {
        restore: [k.key('settings', 'base'), old],
      }),
  });
  const totals = k.selectInput({
    fieldKey: k.key('settings', 'totals'),
    value: k.str(b.totals_currency),
    options: currencyOptions(k.str(b.totals_currency)),
    onCommit: (v, old) =>
      k.run(ik, () => ['set_totals_currency', { code: v === '' ? null : v }], { restore: [k.key('settings', 'totals'), old] }),
  });
  const grid = k.el('div', 'setup-fields');
  grid.append(k.field('Base currency', baseInput), k.field('Show totals in', totals));
  cur.append(
    heading('Currency'),
    grid,
    k.el('p', 'setup-hint tss-note', 'Everything is worked out in the base currency. Renaming it doesn’t convert any amounts or rates.'),
  );
  if (k.state().errors.has(ik)) cur.append(k.errorLine(k.state().errors.get(ik)));
  panel.append(cur);

  // Fringe posting
  const fk = k.key('settings', 'fringes');
  const fp = obj(b.fringe_posting);
  const mode = ['budget', 'production_level', 'category', 'account'].includes(fp.kind) ? fp.kind : 'category';
  const suffix = mode === 'category' ? (isNone(fp.suffix) ? '99' : k.str(fp.suffix)) : '99';
  const fr = k.el('div', 'tss-group');
  fr.dataset.item = fk;
  const modeSel = k.selectInput({
    fieldKey: k.key('settings', 'posting'),
    value: mode,
    options: [
      ['category', 'An account at the end of each category'],
      ['account', 'A line at the end of each account'],
      ['production_level', 'A row above each production total'],
      ['budget', 'One total for the whole budget'],
    ],
    onCommit: (v, old) => k.run(fk, () => ['set_fringe_posting', { mode: v, suffix: null }], { restore: [k.key('settings', 'posting'), old] }),
  });
  const fgrid = k.el('div', 'setup-fields');
  fgrid.append(k.field('Fringes show as', modeSel, { wide: true }));
  if (mode === 'category') {
    const suf = k.textInput({
      fieldKey: k.key('settings', 'suffix'),
      value: suffix,
      onCommit: (v, old) =>
        k.run(fk, () => ['set_fringe_posting', { mode: 'category', suffix: v.trim() }], { restore: [k.key('settings', 'suffix'), old] }),
    });
    fgrid.append(k.field('Account number ends in', suf));
    const ex = fringeAccountExample(coding(), suffix);
    if (ex) fgrid.append(k.el('p', 'setup-hint tss-example-inline', `Category ${ex.cat}: ${ex.number}`));
  }
  fr.append(heading('Fringe totals'), fgrid);
  if (mode === 'budget') {
    fr.append(k.el('p', 'setup-hint tss-note', 'A budget-wide fringe total sits outside above- and below-the-line, so charges don’t apply to it.'));
  }
  if (k.state().errors.has(fk)) fr.append(k.errorLine(k.state().errors.get(fk)));
  panel.append(fr);

  panel.append(calculationGroup());
  panel.append(newLinesGroup());
  // Numbers on screen: rate decimals, summary rows, net total (display-settings.js).
  panel.append(displaySettingsGroup(k));
  panel.append(codingForm());
}

// How totals are worked out (budget settings): rounding and the fringes of
// categories after the last production total. The file leaves out each at
// its default (binary, keep).
function calculationGroup() {
  const b = k.budget();
  const ck = k.key('settings', 'calculation');
  const group = k.el('div', 'tss-group');
  group.dataset.item = ck;
  const arith = b.arithmetic === 'exact' ? 'exact' : 'binary';
  const trailing = b.trailing_level_fringes === 'drop' ? 'drop' : 'keep';
  const arithSel = k.selectInput({
    fieldKey: k.key('settings', 'arithmetic'),
    value: arith,
    options: [
      ['binary', 'Spreadsheet (binary)'],
      ['exact', 'Exact decimal'],
    ],
    onCommit: (v, old) => k.run(ck, () => ['set_arithmetic', { value: v }], { restore: [k.key('settings', 'arithmetic'), old] }),
  });
  const trailingSel = k.selectInput({
    fieldKey: k.key('settings', 'trailing'),
    value: trailing,
    options: [
      ['keep', 'Keep'],
      ['drop', 'Drop'],
    ],
    onCommit: (v, old) =>
      k.run(ck, () => ['set_trailing_level_fringes', { value: v }], { restore: [k.key('settings', 'trailing'), old] }),
  });
  const grid = k.el('div', 'setup-fields');
  grid.append(
    k.field('Rounding', arithSel, { wide: true }),
    k.field('Fringes of categories after the last production total', trailingSel, { wide: true }),
  );
  group.append(
    heading('Calculation'),
    grid,
    k.el(
      'p',
      'setup-hint tss-note',
      'Spreadsheet rounding works in binary numbers, so 1 x 1.005 is 1.00; exact decimal makes it 1.01. Dropped fringes count in no total; this only applies when fringes post above each production total.',
    ),
  );
  if (k.state().errors.has(ck)) group.append(k.errorLine(k.state().errors.get(ck)));
  return group;
}

// What a line inserted in the grid copies from its neighbor (a budget setting).
const NEW_LINE_FIELDS = [
  ['fringes', 'Fringes'],
  ['groups', 'Groups'],
  ['set', 'Set'],
  ['location', 'Location'],
  ['currency', 'Currency'],
];

function newLineDefaults() {
  const d = obj(k.budget().new_line_defaults);
  // The file leaves the setting out at its default: fringes on, the rest off.
  const has = Object.keys(d).length > 0;
  const out = {};
  for (const [f] of NEW_LINE_FIELDS) out[f] = has ? d[f] === true : f === 'fringes';
  return out;
}

function newLinesGroup() {
  const nk = k.key('settings', 'new_lines');
  const group = k.el('div', 'tss-group');
  group.dataset.item = nk;
  const now = newLineDefaults();
  group.append(
    heading('New lines'),
    k.el('p', 'setup-hint tss-note', 'A line you insert copies these from the line above it (or the line below, when there is none above).'),
  );
  for (const [f, label] of NEW_LINE_FIELDS) {
    const fieldKey = k.key('settings', 'new_lines', f);
    group.append(
      checkbox(fieldKey, now[f], label, (on) =>
        k.run(nk, () => ['set_new_line_defaults', { defaults: { ...newLineDefaults(), [f]: on } }], { restore: [fieldKey, !on] }),
      ),
    );
  }
  if (k.state().errors.has(nk)) group.append(k.errorLine(k.state().errors.get(nk)));
  return group;
}

function codingForm() {
  const ck = k.key('settings', 'coding');
  const p = coding();
  const form = k.el('form', 'tss-group');
  form.noValidate = true;
  form.dataset.item = ck;
  const widths = [
    ['category_width', 'Category', 1],
    ['account_width', 'Account', 0],
    ['sub_width', 'Sub-account', 0],
    ['set_width', 'Set', 0],
  ];
  const sels = {};
  const grid = k.el('div', 'setup-fields tss-widths');
  for (const [f, label, min] of widths) {
    const sel = k.el('select', 'setup-input setup-select');
    sel.id = k.nextId();
    sel.name = f;
    sel.dataset.key = k.key('settings', 'coding', f);
    for (let w = min; w <= 6; w += 1) {
      const o = k.el('option', null, w === 0 ? 'Not used' : String(w));
      o.value = String(w);
      sel.append(o);
    }
    sel.value = String(p[f]);
    sels[f] = sel;
    grid.append(k.field(label, sel));
  }
  const sep = k.el('select', 'setup-input setup-select');
  sep.id = k.nextId();
  sep.name = 'separator';
  sep.dataset.key = k.key('settings', 'coding', 'separator');
  const known = SEPARATORS.some(([v]) => v === p.separator);
  for (const [v, label] of known ? SEPARATORS : [...SEPARATORS, [p.separator, `“${p.separator}”`]]) {
    const o = k.el('option', null, label);
    o.value = v;
    sep.append(o);
  }
  sep.value = p.separator;
  grid.append(k.field('Separator', sep, { wide: true }));

  const example = k.el('p', 'tss-example');
  example.setAttribute('aria-live', 'polite');
  const problem = k.el('p', 'setup-hint setup-hint-bad');
  const read = () => ({
    category_width: Number(sels.category_width.value),
    account_width: Number(sels.account_width.value),
    sub_width: Number(sels.sub_width.value),
    set_width: Number(sels.set_width.value),
    separator: sep.value,
  });
  const apply = k.el('button', 'setup-btn setup-btn-primary', 'Apply');
  apply.type = 'submit';
  apply.dataset.key = k.key('settings', 'coding', 'apply');
  const update = () => {
    const q = read();
    const bad = codingProblem(q);
    example.replaceChildren(k.el('span', 'setup-muted', 'Example '), k.el('strong', 'setup-num', bad ? '?' : exampleNumber(q)));
    problem.textContent = bad || '';
    problem.hidden = !bad;
    const same = Object.keys(q).every((f) => q[f] === p[f]);
    apply.hidden = same;
  };
  // A renumber preview still open (renumber.js) keeps the pattern it is for.
  const open = pendingRenumber(k.view() && k.view().doc);
  if (open) {
    for (const f of Object.keys(sels)) sels[f].value = String(open.q[f]);
    sep.value = open.q.separator;
  }
  const reset = () => {
    for (const f of Object.keys(sels)) sels[f].value = String(p[f]);
    sep.value = p.separator;
    update();
    sels.category_width.focus();
  };
  for (const s of [...Object.values(sels), sep]) s.addEventListener('change', update);
  update();

  form.addEventListener('submit', (e) => {
    e.preventDefault();
    if (k.isBusy()) return;
    const q = read();
    const bad = codingProblem(q);
    if (bad) {
      problem.textContent = bad;
      problem.hidden = false;
      return;
    }
    // Offers to renumber the accounts when any number would change.
    applyCoding(k, ck, q, form, {
      onProblem: (text) => {
        problem.textContent = text;
        problem.hidden = false;
      },
      onCancel: reset,
    });
  });
  const actions = k.el('div', 'setup-add-actions tss-coding-actions');
  actions.append(example, apply);
  form.append(
    heading('Account numbers'),
    grid,
    actions,
    problem,
    k.el('p', 'setup-hint tss-note', 'Numbers you type from now on are read and written with this pattern. When numbers already in the budget would change, Apply offers to renumber them.'),
  );
  if (k.state().errors.has(ck)) form.append(k.errorLine(k.state().errors.get(ck)));
  if (open) form.append(previewBox(k, ck, open, reset));
  return form;
}

// ---------------------------------------------------------------------------
// Baseline

function changedCount(map) {
  let n = 0;
  for (const [, c] of k.entries(map)) if (obj(c).variance !== 0) n += 1;
  return n;
}

function renderBaseline(panel) {
  const b = k.budget();
  const view = k.view() || {};
  const variance = view.variance && typeof view.variance === 'object' ? view.variance : null;
  const has = b.baseline && typeof b.baseline === 'object';
  const ik = k.key('baseline');
  panel.append(
    intro(
      'Lock the current totals as the original budget. From then on every line, account and category can show its original total and how far it has moved.',
    ),
  );
  const box = k.el('div', 'tss-group');
  box.dataset.item = ik;

  if (has) {
    const table = k.el('table', 'tss-table');
    const thead = k.el('thead');
    const hr = k.el('tr');
    for (const h of ['', 'Original', 'Current', 'Change']) {
      const th = k.el('th', h ? 'setup-right' : null, h);
      th.scope = 'col';
      hr.append(th);
    }
    thead.append(hr);
    const tbody = k.el('tbody');
    const line = (label, ch0, fallback) => {
      const ch = obj(ch0);
      const tr = k.el('tr');
      const th = k.el('th', null, label);
      th.scope = 'row';
      const orig = isNone(ch.original) ? fallback : ch.original;
      const v = ch.variance;
      const change = k.el('td', 'setup-num setup-right', isNone(v) ? '' : k.formatCents(v));
      if (typeof v === 'number' && v !== 0) change.classList.add(v > 0 ? 'tss-up' : 'tss-down');
      tr.append(
        th,
        k.el('td', 'setup-num setup-right', isNone(orig) ? '' : k.formatCents(orig)),
        k.el('td', 'setup-num setup-right', isNone(ch.current) ? '' : k.formatCents(ch.current)),
        change,
      );
      tbody.append(tr);
    };
    line('Grand total', variance && variance.grand_total, b.baseline.grand_total);
    line('Net total', variance && variance.net_total, b.baseline.net_total);
    table.append(thead, tbody);
    box.append(table);
    if (variance) {
      // Views list only the rows on screen; the count covers every row.
      const lines = typeof variance.rows_changed === 'number' ? variance.rows_changed : changedCount(variance.rows);
      const accounts = changedCount(variance.accounts);
      box.append(
        k.el(
          'p',
          'setup-hint tss-note',
          lines || accounts
            ? `Changed since locked: ${k.plural(lines, 'line', 'lines')} in ${k.plural(accounts, 'account', 'accounts')}.`
            : 'Nothing has changed since the totals were locked.',
        ),
      );
    } else {
      box.append(k.el('p', 'setup-hint tss-note', 'Current totals can’t be worked out right now, so there is nothing to compare.'));
    }
    if (view.partial) box.append(k.el('p', 'setup-hint tss-note', 'Current totals are partial while the budget has errors.'));
    const actions = k.el('div', 'tss-actions');
    const relock = button('Lock again', 'setup-btn', k.key('baseline', 'relock'), () =>
      confirmBox(ik, relock, 'Replace the original totals with the current ones?', 'Lock again', false, () => ['lock_baseline', {}]),
    );
    const clear = button('Clear', 'setup-btn', k.key('baseline', 'clear'), () =>
      confirmBox(ik, clear, 'Clear the original totals? The budget stops showing what changed.', 'Clear', true, () => ['clear_baseline', {}]),
    );
    actions.append(relock, clear);
    box.append(actions);
  } else {
    box.append(k.el('p', 'setup-empty', 'No original totals locked yet.'));
    if (view.error) {
      box.append(k.el('p', 'setup-hint tss-note', 'The budget has a problem to fix before its totals can be locked.'));
    }
    const actions = k.el('div', 'tss-actions');
    actions.append(
      button('Lock current totals', 'setup-btn setup-btn-primary', k.key('baseline', 'lock'), () => {
        k.setFocusKey(k.key('baseline', 'relock'));
        k.run(ik, () => ['lock_baseline', {}], { onError: () => k.setFocusKey(null) });
      }),
    );
    box.append(actions);
  }
  if (k.state().errors.has(ik)) box.append(k.errorLine(k.state().errors.get(ik)));
  panel.append(box);
}
