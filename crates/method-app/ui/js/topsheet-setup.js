// Budget Setup's Charges, Credits, Info, Settings and Baseline tabs: the
// budget's own setup (currencies are the Currency tab's band, setupbands.js;
// sets and locations are lists in apply.js).
//
// setupwin.js draws them through setup.js, which owns the command queue and
// focus handling and passes its helpers in as `kit`, so every tab looks and
// behaves the same. Each tab is drawn in the shared layout (setupframe.js;
// the classes are documented at the top of setup.css): an action strip, the
// tab's content scrolling under it, and its totals in the window's footer.
// Charges and Credits list their items in the left rail (railHost()), with
// the add form under the list, and edit the picked item at the area's full
// width. Budget text only reaches the page through textContent and input
// values, never innerHTML.

import { displaySettingsGroup } from './display-settings.js';
import { invoke } from './api.js';
import { applyCoding, pendingRenumber, previewBox } from './renumber.js';
import { anchoredForm } from './modal.js';
import { saveItemToLibrary } from './library.js';
import { redrawLists } from './setuplist.js';
import { mountFrame, actionStrip, stripButton, searchBox, gridTable, band, bandField, setFooterTotals, railHost } from './setupframe.js';
import { obj } from './util.js';
import { fringeRowOf } from './levels.js';

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

// What Save to Library… and New… use: the window's own call and toast.
function windowCtx() {
  const c = k.state().ctx || {};
  return {
    call: (cmd, args) => (typeof c.call === 'function' ? c.call(cmd, args) : Promise.reject(new Error('Not connected'))),
    toast: (m) => typeof c.toast === 'function' && c.toast(m),
  };
}

// A field in a band (setupframe.js): its label, the control, then a unit
// like "%". `size` is a width class for the control.
function bandInput(text, control, { size = '', unit = '', title = '' } = {}) {
  if (size) control.classList.add(size);
  const f = bandField(text, control, title);
  if (unit) {
    const u = k.el('span', 'setup-adorn', unit);
    u.setAttribute('aria-hidden', 'true');
    f.append(u);
  }
  return f;
}

// A band's short title, at its left.
function bandTitle(text) {
  return k.el('span', 'sw-band-title', text);
}

// A compact button in a band (the strip's style), acting on the picked item.
function bandButton(text, fieldKey, onClick) {
  return button(text, 'sw-act', fieldKey, onClick);
}

// Chips for a set of names picked from `options`, with a select to add one,
// as a band field.
function chips({ fieldKey, label, selected, options, onChange, emptyText = 'None' }) {
  const wrap = k.el('span', 'sw-band-field tss-chipfield');
  const head = k.el('span', 'sw-band-label', label);
  head.id = k.nextId();
  const row = k.el('span', 'tss-chips');
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

// A yes/no box in an item, in the same style as setup.js's delete confirm.
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

function locationNames() {
  return k.entries(k.budget().locations).map(([n]) => n);
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

// ---------------------------------------------------------------------------
// Tables (setupframe.js gridTable: the shared heads, rows and cells)

/** A column head; `num` right-aligns it over figures. */
function head(text, { num = false, title = '' } = {}) {
  const h = k.el('span', num ? 'sw-head sw-num' : 'sw-head', text);
  if (title) h.title = title;
  return h;
}

/** A cell holding `content` (a control, or nothing). */
function cell(content, cls = '') {
  const c = k.el('span', cls ? `sw-cell ${cls}` : 'sw-cell');
  if (content instanceof Node) c.append(content);
  return c;
}

/** A cell of text: a figure (`num`), muted text, or plain text. */
function textCell(text, { num = false, muted = false, title = '' } = {}) {
  const span = k.el('span', num ? 'sw-num' : muted ? 'sw-cell-text' : 'sw-cell-label', text);
  const c = cell(span, num ? 'sw-num' : '');
  if (title) c.title = title;
  return c;
}

/** The first column, where the list tables keep their drag handle. */
function lead() {
  return k.el('span', 'sw-cell');
}

/** A row; a focus inside it selects it, as in the list tables. */
function gridRow() {
  const li = k.el('li', 'sw-grid-row');
  li.addEventListener('focusin', () => {
    const ul = li.parentElement;
    if (ul) for (const x of ul.querySelectorAll(':scope > .is-selected')) if (x !== li) x.classList.remove('is-selected');
    li.classList.add('is-selected');
  });
  return li;
}

/** A row's own buttons (delete), at its right end. */
function acts(...buttons) {
  const c = k.el('span', 'sw-cell sw-acts');
  c.append(...buttons);
  return c;
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
// Charges and credits: the items listed in the left rail, the picked item's
// editor filling the area under the strip. Every item's editor is drawn and
// only the picked one shows, so picking another needs no redraw.

// The picked item of each tab, by index, kept across redraws; and the index
// a newly added item will have, picked once it shows up.
const picked = { charges: 0, credits: 0 };
const pickNew = { charges: null, credits: null };

/**
 * Draws `tab` into `panel`: its items in the left rail (railHost()) with
 * `add` (the add form) under them, and the picked item's editor under the
 * action strip (New…, Move Up, Move Down, Delete… and Save to Library…).
 * `items` are { name, applied, figure, title, bad, editor }, in order;
 * `library` is the library kind (Save to Library…); `move` the reorder
 * command; `deleteText(i)` and `deleteBuild(i)` the delete question and
 * command; `foot(i)` the footer's totals with item i picked; `onShow(i)`
 * runs when item i's editor shows; `extra` are more nodes after the editors.
 */
function pickEditor(panel, { tab, what, label, library, items, add, empty, move, deleteText, deleteBuild, foot, onShow, extra = [] }) {
  const count = items.length;
  if (pickNew[tab] !== null && pickNew[tab] < count) {
    picked[tab] = pickNew[tab];
    pickNew[tab] = null;
  }
  let at = count ? Math.min(Math.max(0, Number(picked[tab]) || 0), count - 1) : 0;
  picked[tab] = at;
  const editors = k.el('div', 'tss-editors');

  // Moves the picked item by `by`; focus stays on the button pressed (or
  // goes to `focusKey`).
  const moveBy = (by, focusKey) => {
    const from = at;
    const to = at + by;
    if (!count || to < 0 || to >= count) return;
    k.setFocusKey(focusKey || k.key(tab, by < 0 ? 'up' : 'down'));
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

  // The delete question, at the top of the picked item's editor, as
  // setup.js asks it for a list item.
  const confirmDelete = () => {
    if (!count) return;
    const open = editors.querySelector('.tss-confirm');
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
    items[i].editor.prepend(box);
    yes.focus();
  };

  // New…: the add form's first box, in the rail.
  const focusAdd = () => {
    const f = k.findByData('key', k.key('add', tab, 'name'));
    if (!f) return;
    f.focus();
    if (typeof f.scrollIntoView === 'function') f.scrollIntoView({ block: 'nearest' });
  };

  const busy = (fn) => () => {
    if (!k.isBusy()) fn();
  };
  const neu = stripButton(`New ${what}…`, k.key('new', tab), focusAdd, { title: `Add a ${what} to this budget` });
  const up = stripButton('Move up', k.key(tab, 'up'), busy(() => moveBy(-1)));
  const down = stripButton('Move down', k.key(tab, 'down'), busy(() => moveBy(1)));
  const del = stripButton('Delete…', k.key('del', tab), busy(confirmDelete));
  const save = stripButton('Save to Library…', k.key('save', tab), () => {
    if (count) saveItemToLibrary(windowCtx(), library, items[at].name);
  });

  const ul = k.el('ul', 'sw-tree tss-pick');
  ul.setAttribute('aria-label', label);
  const buttons = items.map((it, n) => {
    const li = k.el('li');
    const b = k.el('button', 'sw-tree-item tss-pick-item');
    if (!it.applied) b.classList.add('is-off');
    if (it.bad) b.classList.add('is-bad');
    b.type = 'button';
    b.dataset.key = k.key(tab, 'pick', n);
    b.dataset.nobusy = '1';
    b.title = it.title;
    b.setAttribute('aria-keyshortcuts', 'Alt+Shift+ArrowUp Alt+Shift+ArrowDown');
    b.append(k.el('span', 'sw-tree-name', it.name), k.el('span', it.applied ? 'sw-tree-count setup-num' : 'sw-tree-count', it.figure));
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

  // The strip's buttons, the list and the footer follow the picked item.
  const sync = () => {
    items.forEach((it, n) => {
      it.editor.hidden = n !== at;
      buttons[n].setAttribute('aria-current', n === at ? 'true' : 'false');
    });
    const name = count ? items[at].name : '';
    const able = (b, on, text, none) => {
      if (on) b.removeAttribute('aria-disabled');
      else b.setAttribute('aria-disabled', 'true');
      b.title = on ? text : none;
      b.setAttribute('aria-label', on ? text : none);
    };
    const first = `Add a ${what} first`;
    able(up, count > 0 && at > 0, `Move ${what} ${name} up`, count ? `${name} is first` : first);
    able(down, count > 0 && at < count - 1, `Move ${what} ${name} down`, count ? `${name} is last` : first);
    able(del, count > 0, `Delete ${what} ${name}`, first);
    able(save, count > 0, `Copy to your library, with what it needs: ${what} ${name}`, first);
    setFooterTotals(foot(at));
  };

  const pick = (n) => {
    if (n === at) return;
    at = n;
    picked[tab] = n;
    const open = editors.querySelector('.tss-confirm');
    if (open) open.remove();
    sync();
    if (onShow) onShow(n);
  };

  const listLabel = k.el('span', 'sw-rail-label', label);
  listLabel.setAttribute('aria-hidden', 'true');
  const list = k.el('div', 'sw-rail-field');
  list.append(listLabel, count ? ul : k.el('p', 'tss-pick-empty', 'None yet'));
  add.classList.add('tss-rail-add');
  const rail = railHost();
  if (rail) rail.append(list, add);

  if (!count) editors.append(k.emptyNote(empty));
  items.forEach((it, n) => {
    it.editor.id = k.nextId();
    buttons[n].setAttribute('aria-controls', it.editor.id);
    editors.append(it.editor);
  });
  mountFrame(panel, {
    strip: actionStrip(label, [neu, up, down, del, save]),
    // No rail (it shows the library): the list and the add form go first.
    body: rail ? [editors, ...extra] : [list, add, editors, ...extra],
  });
  sync();
  if (count && onShow) onShow(at);
}

// An item's editor: errors and problems go at its end (setup.js finds it by
// data-item).
function editorShell(itemKey, label) {
  const ed = k.el('section', 'tss-ed');
  ed.dataset.item = itemKey;
  ed.setAttribute('aria-label', label);
  return ed;
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

// Per charge, what fills its editor's left-out totals when it shows, and
// its footer figures as saved: { applied, entry }.
let chargeLoaders = [];
let chargeFeet = [];

// The footer with charge `i` picked: how many charges, its total and what
// it leaves out. `live` is a total figured from typed text (preview_charge);
// `stale` dims it while the text isn't a number yet.
function chargeFoot(i, live = null, stale = false) {
  const out = [{ label: '', value: k.plural(chargeFeet.length, 'charge', 'charges') }];
  const f = chargeFeet[i];
  if (!f) return out;
  if (!f.applied) {
    out.push({ label: 'Total charge', value: 'Not applied' });
    return out;
  }
  const t = live || f.entry;
  if (!t) return out;
  out.push(
    {
      label: 'Total charge',
      value: k.formatCents(t.amount),
      title: stale ? 'Finish typing a number to see the new total' : chargeTitle(t, Boolean(live)),
      dim: stale,
    },
    { label: 'Left out', value: k.formatCents(t.excluded), title: 'Everything the charge leaves out: each line counts once, even when several rows pick it', dim: stale },
  );
  return out;
}

function renderCharges(panel) {
  const b = k.budget();
  const r = k.report();
  const charges = arr(b.charges);
  chargeLoaders = [];
  chargeFeet = [];
  preview.live = null;
  const items = charges.map((c0, i) => {
    const c = chargeInput(c0);
    const entry = appliedEntry(charges, r && r.charges, i);
    const issues = issuesAt('charge', i);
    chargeFeet[i] = { applied: c.applied, entry: entry || null };
    const fig = !c.applied ? 'Not applied' : entry ? k.formatCents(entry.amount) : '';
    const figTitle = !c.applied ? 'Not applied' : entry ? `Total charge, ${chargeTitle(entry, false).toLowerCase()}` : '';
    return {
      name: c.name,
      applied: c.applied,
      figure: fig,
      title: [c.name, figTitle, issues.length ? 'This charge has a problem' : ''].filter(Boolean).join('\n'),
      bad: issues.length > 0,
      editor: chargeEditor(i, c0, c, issues),
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

  pickEditor(panel, {
    tab: 'charges',
    what: 'charge',
    label: 'Charges',
    library: 'charge',
    items,
    add,
    empty: 'No charges yet.',
    move: 'move_charge',
    deleteText: (i) => `Delete charge ${items[i].name}?`,
    deleteBuild: (i) => ['delete_charge', { index: i }],
    foot: (i) => chargeFoot(i),
    onShow: (i) => {
      if (chargeLoaders[i]) chargeLoaders[i]();
    },
  });
}

// Charge `i`'s editor: a band with its name, Applied, type, value and
// account; one with the topsheet label choices; then what it leaves out.
// Its total and what it leaves out in all are the footer's.
function chargeEditor(i, c0, c, issues) {
  const ik = k.key('charges', i);
  const fk = (f) => k.key('charges', i, f);
  const commit = (f, patchOf) => (v, old) =>
    k.run(ik, () => chargeArgs(i, patchOf(v)), { restore: [fk(f), old] });
  const ed = editorShell(ik, `Charge ${c.name}`);

  const name = k.textInput({ fieldKey: fk('name'), value: c.name, onCommit: commit('name', (v) => ({ name: v })) });
  const applied = checkbox(fk('applied'), c.applied, 'Applied', (v) =>
    k.run(ik, () => chargeArgs(i, { applied: v }), { restore: [fk('applied'), !v] }),
  );
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
  const values = [];
  if (c.kind === 'percent') {
    const pct = k.textInput({
      fieldKey: fk('percent'),
      value: k.str(c.percent),
      num: true,
      onCommit: commit('percent', (v) => ({ percent: v.trim() })),
    });
    livePreview(pct, i);
    values.push(bandInput('Percent', pct, { size: 'tss-w-pct', unit: '%' }));
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
    values.push(bandInput('Amount', amount, { size: 'tss-w-num' }), bandInput('Currency', cur));
  }
  const account = k.textInput({
    fieldKey: fk('account'),
    value: c.account || '',
    placeholder: 'None',
    onCommit: commit('account', (v) => ({ account: v.trim() === '' ? null : v.trim() })),
  });
  ed.append(
    band(
      `Charge ${c.name}`,
      bandInput('Name', name, { size: 'tss-w-name' }),
      applied,
      bandInput('Type', kind),
      ...values,
      bandInput('Posts to account', account, { size: 'tss-w-acct' }),
    ),
  );

  // What the topsheet label shows besides the name (display only).
  const labels = [];
  if (c.kind === 'percent') {
    labels.push(
      checkbox(fk('label-percent'), c.label_percent, 'Show the percent on the topsheet', (v) =>
        k.run(ik, () => chargeArgs(i, { label_percent: v }), { restore: [fk('label-percent'), !v] }),
      ),
    );
  }
  labels.push(
    checkbox(fk('label-excluded'), c.label_excluded, 'Show the amount left out on the topsheet', (v) =>
      k.run(ik, () => chargeArgs(i, { label_excluded: v }), { restore: [fk('label-excluded'), !v] }),
    ),
  );
  ed.append(band('Topsheet label', bandTitle('Topsheet label'), ...labels));
  ed.append(...chargeExclusions(i, c, ik));
  k.finishItem(ed, ik, issues, null);
  return ed;
}

// The footer figure's tooltip: what the charge is figured on.
function chargeTitle(t, typing) {
  const on = `On ${k.formatCents(t.base)}, less ${k.formatCents(t.excluded)} left out`;
  return typing ? `${on} (with the value being typed)` : on;
}

// Live preview: while a charge's percent, amount or a left-out amount is
// being typed, its total (and what it leaves out) is figured from the typed
// text, before Enter commits it, and shown in the footer. Charges whose
// total isn't shown (not applied) have nothing to update.
const preview = { timer: null, seq: 0, live: null };
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
  const f = chargeFeet[i];
  if (!f || !f.applied || !f.entry || picked.charges !== i) return;
  const seq = ++preview.seq;
  const shown = () => seq === preview.seq && picked.charges === i && chargeFeet[i] === f;
  try {
    const t = await invoke('preview_charge', { index: i, charge: draftCharge(i) });
    if (!shown()) return;
    preview.live = { i, t };
    setFooterTotals(chargeFoot(i, t));
  } catch {
    // Not a number yet: keep the last total, dimmed as not current.
    if (!shown()) return;
    const last = preview.live && preview.live.i === i ? preview.live.t : null;
    setFooterTotals(chargeFoot(i, last, true));
  }
}

// What leaving out one group, category or account takes off a charge, as
// the totals work it out (each line with its fringes): the saved charge with
// only that exclusion, through preview_charge. The figure doesn't depend on
// the charge, so it is kept per budget revision by what is left out; the
// last one shows until the new one arrives.
const leftOut = { stamp: '', now: new Map(), last: new Map() };

function leftOutTotal(i, f, name, span) {
  const v = obj(k.view());
  const stamp = `${v.doc}:${v.rev}`;
  if (leftOut.stamp !== stamp) {
    leftOut.stamp = stamp;
    leftOut.now = new Map();
  }
  const id = JSON.stringify([v.doc, f, name]);
  if (leftOut.last.has(id)) span.textContent = k.formatCents(leftOut.last.get(id));
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
      span.textContent = k.formatCents(cents);
    },
    () => {
      span.textContent = '';
    },
  );
}

// What charge `i` leaves out: a band with the ways to leave more out, then
// one table: amounts typed in, then groups, categories and accounts, each
// with what it takes off. An item the charge names that the budget no
// longer has stays listed until it is removed.
function chargeExclusions(i, c, ik) {
  const fk = (f) => k.key('charges', i, f);
  const rawAmounts = arr(obj(arr(k.budget().charges)[i]).excluded_amounts);
  const groupDefs = obj(k.budget().groups);
  const cats = categoryNames();
  const accts = accountNames();

  const leave = (f, value) =>
    k.run(ik, () => chargeArgs(i, (cur) => ({ [f]: [...cur[f].filter((x) => x !== value), value] })));
  const named = (map, f) =>
    [...map.keys()].filter((code) => !c[f].includes(code)).map((code) => [code, `${code} ${map.get(code)}`.trim()]);
  const adders = band(
    `Left out of ${c.name}`,
    bandTitle('Left out'),
    bandButton('Exclude amount', fk('amount-add'), () => {
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

  const ul = k.el('ul');
  ul.setAttribute('aria-label', `Left out of ${c.name}`);
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
    return button('×', 'setup-del', delKey, () => {
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
      cls: 'sw-cell-input',
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
      cls: 'sw-cell-input',
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
    const li = gridRow();
    li.append(lead(), textCell('Amount', { muted: true }), cell(desc, 'tss-span2'), cell(amt), acts(del));
    ul.append(li);
  });

  const loads = [];
  const namedRow = (type, f, name, desc, known, title) => {
    const delKey = fk(`excl-${type}-del-${name}`);
    const del = removeButton(delKey, `Remove ${type} ${name}`, (cur) => ({ [f]: cur[f].filter((x) => x !== name) }));
    rows.push({ del: delKey });
    const figure = k.el('span', 'sw-num');
    const total = cell(figure, 'sw-num');
    if (known) {
      total.title = title;
      loads.push(() => leftOutTotal(i, f, name, figure));
    }
    const descCell = known ? textCell(desc, { title: desc }) : textCell('Not in this budget', { muted: true });
    const li = gridRow();
    li.append(
      lead(),
      textCell(type[0].toUpperCase() + type.slice(1), { muted: true }),
      textCell(name, { title: name }),
      descCell,
      total,
      acts(del),
    );
    ul.append(li);
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
  chargeLoaders[i] = () => loads.forEach((fn) => fn());
  if (!rows.length) return [adders, k.emptyNote('Nothing left out.')];
  return [
    adders,
    gridTable(
      'charge-excl',
      [lead(), head('Type'), head('Name'), head('Description'), head('Total', { num: true, title: 'What leaving it out takes off the charge' }), head('')],
      ul,
    ),
  ];
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
          : src.kind === 'account'
            ? { kind: 'account', account: k.str(src.account) }
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
    // One list per credit, for all its group, category and account rows
    // (decisions.md, entries 57 and 61), so changing a row's spend keeps it.
    excluded_fringes: arr(c.excluded_fringes).map(k.str),
    // Where the spend has to be (decisions.md, entry 80); none: anywhere.
    locations: arr(c.locations).map(k.str),
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

// Per credit, its footer figures: { applied, entry, rows (the report's) }.
let creditFeet = [];

// The footer with credit `i` picked: how many credits, its rows' spend,
// fringes left out, net spend and credit added up, then the discount and the
// net credit taken off the grand total.
function creditFoot(i) {
  const out = [{ label: '', value: k.plural(creditFeet.length, 'credit', 'credits') }];
  const f = creditFeet[i];
  if (!f) return out;
  if (!f.applied) {
    // Not taken off the grand total: its estimate, once it has come.
    const p = estimateOf(i);
    if (p && !isNone(p.amount)) {
      out.push({ label: 'Qualified', value: k.formatCents(p.qualified), title: 'The rows’ net spend added up' });
      out.push({ label: 'Estimate', value: k.formatCents(p.amount), title: 'What this credit would take off the grand total if applied' });
    } else {
      out.push({ label: 'Net credit', value: 'Not applied' });
    }
    return out;
  }
  if (f.rows.length) {
    const add = (field) => {
      const s = centsSum(f.rows.map((t) => (isNone(t[field]) ? 0 : t[field])));
      return s === null ? '' : k.formatCents(s);
    };
    const credit = decimalSum(f.rows.map((t) => t.credit));
    out.push(
      { label: 'Gross', value: add('gross'), title: 'The rows’ spend before fringes are left out' },
      { label: 'Fringes out', value: add('excluded_fringes'), title: 'Fringes left out of the spend' },
      { label: 'Net', value: add('net'), title: 'The spend the percents apply to' },
      {
        label: 'Credit',
        value: credit === null ? '' : money(credit),
        title: credit === null ? '' : `The rows added up: exactly ${k.formatDecimal(credit)}, before the credit’s cap and discount`,
      },
    );
  }
  if (f.entry) {
    // The discount comes off after the cap; the credit is rounded once.
    const before = dollarsToCents(f.entry.capped);
    if (before !== null && before !== BigInt(f.entry.amount)) {
      out.push({
        label: 'Discount',
        value: k.formatCents(before - BigInt(f.entry.amount)),
        title: `Exactly ${k.formatDecimal(f.entry.capped)} before the discount, after the cap`,
      });
    }
    out.push({ label: 'Net credit', value: k.formatCents(f.entry.amount), title: 'Credit taken off the grand total' });
  }
  return out;
}

function renderCredits(panel) {
  const b = k.budget();
  const r = k.report();
  const credits = arr(b.credits);
  creditFeet = [];
  const items = credits.map((c0, i) => {
    const c = creditInput(c0);
    const entry = appliedEntry(credits, r && r.credits, i);
    const issues = issuesAt('credit', i);
    creditFeet[i] = { applied: c.applied, entry: entry || null, rows: entry && Array.isArray(entry.rows) ? entry.rows.map(obj) : [] };
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
        { index: null, credit: { name: v.name.trim(), account: null, applied: true, cap: null, discount_percent: '0', excluded_fringes: [], locations: [], rows: [] } },
      ];
    },
  });

  pickEditor(panel, {
    tab: 'credits',
    what: 'credit',
    label: 'Credits',
    library: 'credit',
    items,
    add,
    empty: 'No credits yet.',
    move: 'move_credit',
    deleteText: (i) => `Delete credit ${items[i].name} and its ${k.plural(arr(obj(credits[i]).rows).length, 'row', 'rows')}?`,
    deleteBuild: (i) => ['delete_credit', { index: i }],
    foot: (i) => creditFoot(i),
    extra: [programsBox()],
  });
  showEstimates();
}

// ---------------------------------------------------------------------------
// Incentive programs side by side (roadmap 3.6; decisions.md, entry 80)

// Every credit's estimate, applied or not, and spend by location, from the
// read-only `incentives` command: fetched once per budget revision, the last
// one kept for the footer until the new one comes.
const estimates = { stamp: '', p: null, last: null };

function loadEstimates() {
  const v = obj(k.view());
  const stamp = `${v.doc}:${v.rev}`;
  if (estimates.stamp !== stamp || !estimates.p) {
    estimates.stamp = stamp;
    estimates.p = invoke('incentives', {}).then(
      (got) => {
        if (estimates.stamp === stamp) estimates.last = obj(got);
        return obj(got);
      },
      () => null,
    );
  }
  return estimates.p;
}

function estimateOf(i) {
  const got = estimates.last;
  return got ? arr(got.programs).map(obj).find((p) => p.index === i) || null : null;
}

// The Credits tab's box under the editors: the programs side by side and
// the spend at each location, filled once the estimates come.
function programsBox() {
  const box = k.el('section', 'tss-programs');
  box.setAttribute('aria-label', 'Programs side by side');
  box.dataset.key = 'credits-programs';
  return box;
}

function showEstimates() {
  loadEstimates().then((got) => {
    const box = k.findByData('key', 'credits-programs');
    if (!box || !got) return;
    box.replaceChildren();
    const programs = arr(got.programs).map(obj);
    // Unapplied credits show their estimate in the list too.
    for (const p of programs) {
      if (p.applied || isNone(p.amount)) continue;
      const b = k.findByData('key', k.key('credits', 'pick', p.index));
      const fig = b && b.querySelector('.sw-tree-count');
      if (fig) {
        fig.textContent = `Est. ${k.formatCents(p.amount)}`;
        b.title = `${p.name}\nNot applied: estimated, not taken off the grand total`;
      }
    }
    if (picked.credits < programs.length) setFooterTotals(creditFoot(picked.credits));
    if (programs.length) box.append(programsTable(programs));
    const spend = arr(got.locations).map(obj);
    if (spend.length) box.append(locationsTable(spend));
  });
}

function where(list) {
  const names = arr(list).map(k.str);
  return names.length ? names.join(', ') : 'Anywhere';
}

function programsTable(programs) {
  const ul = k.el('ul');
  ul.setAttribute('aria-label', 'Programs side by side');
  for (const p of programs) {
    const li = gridRow();
    const spend = arr(p.by_location)
      .map(obj)
      .map((s) => `${isNone(s.location) ? 'No location' : k.str(s.location)} ${k.formatCents(s.net)}`)
      .join('; ');
    li.append(
      lead(),
      textCell(k.str(p.name)),
      textCell(where(p.locations), { muted: !arr(p.locations).length }),
      textCell(k.formatCents(p.qualified), { num: true, title: spend ? `Net spend by location: ${spend}` : '' }),
      textCell(isNone(p.amount) ? '' : k.formatCents(p.amount), {
        num: true,
        title: isNone(p.amount) ? k.str(p.problem) : p.capped ? 'Held down by the credit’s cap' : '',
      }),
      textCell(isNone(p.amount) ? 'Problem' : p.applied ? 'Applied' : 'Estimate', { muted: !p.applied }),
    );
    ul.append(li);
  }
  return gridTable('programs', [
    lead(),
    head('Program'),
    head('Spend at'),
    head('Qualified', { num: true, title: 'The rows’ net spend added up' }),
    head('Credit', { num: true, title: 'After the caps and the discount, rounded once' }),
    head(''),
  ], ul);
}

function locationsTable(spend) {
  const ul = k.el('ul');
  ul.setAttribute('aria-label', 'Spend by location');
  for (const l of spend) {
    const li = gridRow();
    const name = isNone(l.location) ? 'No location' : k.str(l.location);
    li.append(
      lead(),
      textCell(name, { muted: isNone(l.location), title: l.known === false ? 'Not in the budget’s locations' : '' }),
      textCell(k.formatCents(l.lines), { num: true }),
      textCell(k.formatCents(l.fringes), { num: true }),
      textCell(k.formatCents(l.total), { num: true }),
    );
    ul.append(li);
  }
  return gridTable('location-spend', [
    lead(),
    head('Location'),
    head('Lines', { num: true }),
    head('Fringes', { num: true, title: 'The fringes the lines earn, wherever fringes post' }),
    head('Spend', { num: true }),
  ], ul);
}

// Credit `i`'s editor: a band with its name, Applied, cap, discount and
// account; one with the fringes left out of its spend; one with the
// locations its spend has to be at; one with Add row;
// then its rows as one table. Its figures are the footer's.
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
    band(
      `Credit ${c.name}`,
      bandInput('Name', name, { size: 'tss-w-name' }),
      applied,
      bandInput('Cap on the credit', cap, { size: 'tss-w-num' }),
      bandInput('Discount', discount, { size: 'tss-w-pct', unit: '%' }),
      bandInput('Posts to account', account, { size: 'tss-w-acct' }),
    ),
  );

  ed.append(
    band(
      'Fringes left out of the spend',
      chips({
        fieldKey: fk('fringes'),
        label: 'Fringes left out of the spend',
        selected: c.excluded_fringes,
        options: withKept(fringeNames(), c.excluded_fringes),
        onChange: (list) => k.run(ik, () => creditArgs(i, { excluded_fringes: list })),
      }),
    ),
  );

  // Only lines at these locations count toward its group, category and
  // account rows (decisions.md, entry 80); none picked: every line.
  ed.append(
    band(
      'Spend at these locations',
      chips({
        fieldKey: fk('locations'),
        label: 'Spend at these locations',
        selected: c.locations,
        options: withKept(locationNames(), c.locations),
        emptyText: 'Anywhere',
        onChange: (list) => k.run(ik, () => creditArgs(i, { locations: list })),
      }),
    ),
  );

  const addRow = bandButton('Add row', fk('add-row'), () => {
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
  ed.append(band(`Rows of ${c.name}`, bandTitle('Rows'), addRow));
  ed.append(creditGrid(i, c, raw, entry, ik));
  k.finishItem(ed, ik, issues, null);
  return ed;
}

// A credit's rows as one table: what each is a percent of, its figures from
// the totals, its cap. Their sums are the footer's.
function creditGrid(i, c, raw, entry, ik) {
  if (!c.rows.length) return k.emptyNote('No rows yet.');
  const totals = entry && Array.isArray(entry.rows) ? entry.rows.map(obj) : [];
  const ul = k.el('ul');
  ul.setAttribute('aria-label', `Rows of ${c.name}`);
  c.rows.forEach((row, n) => ul.append(creditRow(i, n, row, raw, totals[n] || null, ik, c.rows.length)));
  return gridTable('credit-rows', [
    lead(),
    head('Spend'),
    head('Description'),
    head('Amount', { num: true }),
    head('Gross', { num: true, title: 'The row’s spend before fringes are left out' }),
    head('Fringes out', { num: true, title: 'Fringes left out of the spend' }),
    head('Net', { num: true, title: 'The spend the percent applies to' }),
    head('Cap type'),
    head('Cap', { num: true }),
    head('%', { num: true }),
    head('Credit', { num: true }),
    head('Over cap', { num: true }),
    head(''),
  ], ul);
}

function creditRow(i, n, row, raw, rt, ik, count) {
  const li = gridRow();
  const fk = (f) => k.key('credits', i, n, f);
  const send = (f, old) => (v) => k.run(ik, () => creditArgs(i, rowPatch(n, () => v)), old === undefined ? {} : { restore: [f, old] });
  const rawRow = obj(arr(raw.rows)[n]);
  const of = (what) => `${what} of row ${n + 1}`;

  const groups = groupNames();
  const cats = categoryNames();
  const accts = accountNames();
  const current =
    row.source.kind === 'group'
      ? `group:${row.source.group}`
      : row.source.kind === 'category'
        ? `category:${row.source.category}`
        : row.source.kind === 'account'
          ? `account:${row.source.account}`
          : 'flat';
  const catLabel = (code) => (cats.get(code) ? `Category: ${code} ${cats.get(code)}` : `Category: ${code}`);
  const acctLabel = (code) => (accts.get(code) ? `Account: ${code} ${accts.get(code)}` : `Account: ${code}`);
  const spend = k.selectInput({
    fieldKey: fk('source'),
    value: current,
    cls: 'sw-cell-input',
    options: [
      ['flat', 'Amount typed in'],
      ...withKept(groups, row.source.kind === 'group' ? [row.source.group] : []).map((g) => [`group:${g}`, `Group: ${g}`]),
      // A topsheet category's total with its fringes (decisions.md, entry 57).
      ...withKept([...cats.keys()], row.source.kind === 'category' ? [row.source.category] : []).map((code) => [`category:${code}`, catLabel(code)]),
      // An account's total with its fringes (decisions.md, entry 61).
      ...withKept([...accts.keys()], row.source.kind === 'account' ? [row.source.account] : []).map((code) => [`account:${code}`, acctLabel(code)]),
    ],
    onCommit: (v, old) =>
      send(fk('source'), old)(
        v === 'flat'
          ? { source: { kind: 'flat', amount: '0' } }
          : v.startsWith('category:')
            ? { source: { kind: 'category', category: v.slice(9) } }
            : v.startsWith('account:')
              ? { source: { kind: 'account', account: v.slice(8) } }
              : { source: { kind: 'group', group: v.slice(6) } },
      ),
  });
  spend.setAttribute('aria-label', of('Spend'));
  spend.title = spend.options[spend.selectedIndex] ? spend.options[spend.selectedIndex].textContent : '';

  const desc = k.textInput({
    fieldKey: fk('description'),
    value: row.description,
    label: of('Description'),
    cls: 'sw-cell-input',
    onCommit: (v, old) => send(fk('description'), old)({ description: v }),
  });

  const amount =
    row.source.kind === 'flat'
      ? k.textInput({
          fieldKey: fk('amount'),
          value: k.formatCents(obj(rawRow.source).amount || 0),
          num: true,
          label: of('Amount'),
          cls: 'sw-cell-input',
          onCommit: (v, old) => send(fk('amount'), old)({ source: { kind: 'flat', amount: v.trim() } }),
        })
      : null;

  const capKind = k.selectInput({
    fieldKey: fk('cap-kind'),
    value: row.cap.kind,
    cls: 'sw-cell-input',
    options: [
      ['none', 'None'],
      ['qualified_spend', 'Qualified spend'],
      ['credit', 'Credit cap'],
    ],
    onCommit: (v, old) => send(fk('cap-kind'), old)({ cap: v === 'none' ? { kind: 'none' } : { kind: v, amount: row.cap.amount || '0' } }),
  });
  capKind.setAttribute('aria-label', of('Cap type'));
  capKind.title = 'Qualified spend caps the spend the percent applies to; Credit cap caps the row’s credit';
  const capAmount =
    row.cap.kind !== 'none'
      ? k.textInput({
          fieldKey: fk('cap-amount'),
          value: k.formatCents(obj(rawRow.cap).amount || 0),
          num: true,
          label: of('Cap'),
          cls: 'sw-cell-input',
          onCommit: (v, old) => send(fk('cap-amount'), old)({ cap: { kind: row.cap.kind, amount: v.trim() } }),
        })
      : null;
  const percent = k.textInput({
    fieldKey: fk('percent'),
    value: row.percent,
    num: true,
    label: of('Percent'),
    cls: 'sw-cell-input',
    onCommit: (v, old) => send(fk('percent'), old)({ percent: v.trim() }),
  });

  const has = rt && !isNone(rt.net);
  const fig = (v, title) => textCell(has ? v : '', { num: true, title: has ? title : '' });
  const [overText, overTitle] = has ? overCap(rt, row) : ['', ''];

  // Focus goes to the row taking this one's place, else the one before,
  // else Add row.
  const after = n < count - 1 ? fk('del') : n > 0 ? k.key('credits', i, n - 1, 'del') : k.key('credits', i, 'add-row');
  const del = button('×', 'setup-del', fk('del'), () => {
    k.setFocusKey(after);
    k.run(ik, () => creditArgs(i, (cur) => ({ rows: cur.rows.filter((_, j) => j !== n) })), { onError: () => k.setFocusKey(null) });
  }, `Delete row ${n + 1}`);

  li.append(
    lead(),
    cell(spend),
    cell(desc),
    cell(amount),
    fig(has ? k.formatCents(rt.gross) : '', 'The row’s spend before fringes are left out'),
    fig(has ? k.formatCents(rt.excluded_fringes) : '', 'Fringes left out of the spend'),
    fig(has ? k.formatCents(rt.net) : '', 'The spend the percent applies to'),
    cell(capKind),
    cell(capAmount),
    cell(percent),
    fig(has ? money(rt.credit) : '', has ? `Exactly ${k.formatDecimal(rt.credit)}; rows are added before rounding` : ''),
    fig(overText, overTitle),
    acts(del),
  );
  return li;
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

// New field…: a small form next to the strip's button; focus goes to the
// new field's value once it is added, else back to the button.
function newInfoField(anchor) {
  const ctx = windowCtx();
  let added = null;
  anchoredForm({
    anchor,
    title: 'New field',
    fields: [
      { key: 'name', label: 'Label', value: '', placeholder: 'Director' },
      { key: 'kind', label: 'Kind', type: 'select', options: INFO_KINDS, value: 'text' },
      { key: 'value', label: 'Value', value: '' },
    ],
    submitText: 'Add',
    onSubmit: (v) => {
      const name = k.str(v.name).trim();
      if (name === '') return Promise.reject('Enter a label.');
      if (k.own(k.budget().info, name) !== undefined) return Promise.reject(`There is already a field named ${name}.`);
      const kind = k.str(v.kind) || 'text';
      const bad = infoCheck(kind, k.str(v.value));
      if (bad) return Promise.reject(bad);
      return ctx.call('set_info', { name, value: { kind, value: k.str(v.value).trim() } }).then(() => {
        added = name;
      });
    },
  }).then(() => {
    const f = k.findByData('key', added === null ? k.key('add', 'info') : k.key('info', added, 'value'));
    if (f) f.focus();
  });
}

// One table: a row per field (name, kind, value, delete), an action strip
// over it (New field…, Delete… for the selected row, the search) and the
// count in the footer.
function renderInfo(panel) {
  const b = k.budget();
  const st = k.state();
  if (!st.filters) st.filters = {};
  if (!st.selected) st.selected = {};
  const q = k.str(st.filters.info).trim().toLowerCase();
  const all = k.entries(b.info);
  const items = all.filter(([n, v0]) => !q || [n, obj(v0).value].some((x) => k.str(x).toLowerCase().includes(q)));
  const names = items.map(([n]) => n);
  const selected = () => (st.selected.info !== undefined && k.own(k.budget().info, st.selected.info) !== undefined ? st.selected.info : null);

  const remove = stripButton('Delete…', k.key('strip', 'info', 'delete'), () => {
    const n = selected();
    if (n === null || k.isBusy()) return;
    const d = k.findByData('key', k.key('del', k.key('info', n)));
    if (d) d.click();
  });
  const syncStrip = () => {
    const n = selected();
    if (n === null) remove.setAttribute('aria-disabled', 'true');
    else remove.removeAttribute('aria-disabled');
    remove.title = n === null ? 'Select a field first' : `Delete field ${n}`;
  };
  const strip = actionStrip(
    'Info',
    [stripButton('New field…', k.key('add', 'info'), (btn) => newInfoField(btn), { title: 'Add a field to this budget' }), remove],
    searchBox({
      key: k.key('search', 'info'),
      value: k.str(st.filters.info),
      label: 'Search info',
      placeholder: 'Search info',
      onChange: (v) => {
        st.filters.info = v;
        redrawLists();
      },
    }),
  );

  const ul = k.list('Budget information');
  items.forEach(([name, v0], i) => {
    const v = obj(v0);
    const kind = ['text', 'number', 'date'].includes(v.kind) ? v.kind : 'text';
    const ik = k.key('info', name);
    const li = k.itemShell(ik, 'sw-grid-row');
    if (st.selected.info === name) li.classList.add('is-selected');
    li.addEventListener('focusin', () => {
      if (st.selected.info === name) return;
      st.selected.info = name;
      for (const x of ul.querySelectorAll(':scope > .is-selected')) x.classList.remove('is-selected');
      li.classList.add('is-selected');
      syncStrip();
    });
    const nameEl = k.el('span', 'setup-name', name);
    nameEl.title = name;
    const valueKey = k.key('info', name, 'value');
    const value = k.textInput({
      fieldKey: valueKey,
      value: k.str(v.value),
      num: kind === 'number',
      placeholder: kind === 'date' ? 'YYYY-MM-DD' : '',
      label: `${name} value`,
      cls: 'sw-cell-input',
      onCommit: (val, old) => k.run(ik, () => ['set_info', { name, value: { kind, value: val } }], { restore: [valueKey, old] }),
    });
    const kindSel = k.selectInput({
      fieldKey: k.key('info', name, 'kind'),
      value: kind,
      options: INFO_KINDS,
      label: `${name} kind`,
      cls: 'sw-cell-input',
      onCommit: (nk, old) =>
        k.run(ik, () => ['set_info', { name, value: { kind: nk, value: k.str(obj(k.own(k.budget().info, name)).value) } }], {
          restore: [k.key('info', name, 'kind'), old],
        }),
    });
    li.append(
      lead(),
      cell(nameEl),
      cell(kindSel),
      cell(value),
      acts(
        k.deleteButton({
          itemKey: ik,
          name,
          what: 'field',
          confirmText: `Delete the field ${name}?`,
          build: () => ['delete_info', { name }],
          neighborKey: k.neighbor('info', names, i, k.key('add', 'info')),
        }),
      ),
    );
    k.finishItem(li, ik, [], null);
    ul.append(li);
  });
  const body = [];
  if (!all.length) body.push(k.emptyNote('No fields yet.'));
  else if (!items.length) body.push(k.emptyNote('Nothing matches the search.'));
  else body.push(gridTable('info', [lead(), head('Name'), head('Kind'), head('Value'), head('')], ul));
  mountFrame(panel, { strip, body });
  syncStrip();
  setFooterTotals([
    { label: '', value: items.length === all.length ? k.plural(all.length, 'field', 'fields') : `${items.length} of ${k.plural(all.length, 'field', 'fields')}` },
  ]);
}

// ---------------------------------------------------------------------------
// Settings: fringe posting, calculation, new lines, numbers on screen,
// account numbers (the base and totals currency are the Currency tab's)

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
  // The sub-account and the set each need the account; the set may follow
  // the account directly when there is no sub-account.
  if (p.sub_width > 0 && p.account_width === 0) return 'The sub-account part needs the account part before it.';
  if (p.set_width > 0 && p.account_width === 0) return 'The set part needs the account part before it.';
  return null;
}

function fringeAccountExample(p, suffix) {
  const first = categoryCodes()[0] || '11';
  if (!p.account_width || !suffix) return null;
  const cat = fit(first, p.category_width);
  return { cat: first, number: [cat, fit(suffix, p.account_width)].join(p.separator) };
}

// A settings row: a fixed-width label on the left, its controls on the right.
// `hint` is one short muted line after the controls; `tip` is the longer
// explanation, shown as the label's tooltip and read out with the controls.
// With `group`, the label names several controls (each with its own label).
function tsRow(labelText, controls, { hint = '', tip = '', group = false } = {}) {
  const row = k.el('div', 'ts-row');
  const label = k.el(group ? 'span' : 'label', 'ts-row-label', labelText);
  label.id = k.nextId();
  if (!group && controls[0] && controls[0].id) label.htmlFor = controls[0].id;
  if (tip) {
    label.title = tip;
    label.classList.add('ts-has-tip');
  }
  const ctl = k.el('div', 'ts-row-ctl');
  if (group) {
    ctl.setAttribute('role', 'group');
    ctl.setAttribute('aria-labelledby', label.id);
  }
  ctl.append(...controls);
  const described = [];
  if (hint) {
    const h = k.el('span', 'ts-hint', hint);
    h.id = k.nextId();
    if (tip) h.title = tip;
    ctl.append(h);
    described.push(h.id);
  }
  if (tip) {
    const t = k.el('span', 'setup-sr', tip);
    t.id = k.nextId();
    ctl.append(t);
    described.push(t.id);
  }
  if (described.length) {
    for (const c of ctl.querySelectorAll('input, select')) {
      const prior = c.getAttribute('aria-describedby');
      c.setAttribute('aria-describedby', [prior, ...described].filter(Boolean).join(' '));
    }
  }
  row.append(label, ctl);
  return row;
}

// A small control with its own short label, for rows holding several.
function tsInline(labelText, control) {
  const wrap = k.el('span', 'ts-inline');
  const label = k.el('label', 'ts-inline-label', labelText);
  label.htmlFor = control.id;
  wrap.append(label, control);
  return wrap;
}

function renderSettings(panel) {
  const b = k.budget();
  panel.classList.add('ts-settings');

  // Currency: a pointer to where it moved.
  const cur = k.el('div', 'tss-group');
  cur.append(heading('Currency'), k.el('p', 'ts-hint ts-pointer', 'The base currency and Show totals in are on the Currency tab.'));

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
  modeSel.classList.add('ts-w-l');
  fr.append(
    heading('Fringe totals'),
    tsRow(
      'Fringes show as',
      [modeSel],
      mode === 'budget'
        ? {
            hint: 'Percent charges count it.',
            tip: 'A budget-wide fringe total sits outside above- and below-the-line. It is in the grand total, and percent charges count it; excluding a line, account or category from a charge takes its fringes off too.',
          }
        : {},
    ),
  );
  if (mode === 'category') {
    const suf = k.textInput({
      fieldKey: k.key('settings', 'suffix'),
      value: suffix,
      onCommit: (v, old) =>
        k.run(fk, () => ['set_fringe_posting', { mode: 'category', suffix: v.trim() }], { restore: [k.key('settings', 'suffix'), old] }),
    });
    suf.classList.add('ts-w-s');
    const ex = fringeAccountExample(coding(), suffix);
    fr.append(tsRow('Account number ends in', [suf], { hint: ex ? `Category ${ex.cat}: ${ex.number}` : '' }));
  }
  if (mode === 'budget') fr.append(...fringeRowRows(fk));
  if (k.state().errors.has(fk)) fr.append(k.errorLine(k.state().errors.get(fk)));

  mountFrame(panel, {
    strip: actionStrip('Settings'),
    body: [
      cur,
      fr,
      calculationGroup(),
      newLinesGroup(),
      // Numbers on screen: rate decimals, summary rows, net total (display-settings.js).
      displaySettingsGroup(k),
      codingForm(),
    ],
  });
}

// The budget's one fringe row (fringes posted by budget; decisions.md, entry
// 59): its account number, description and place on the topsheet. Display
// only; the file leaves it out at its defaults (no number, "Total Fringes",
// after the summary rows).
function fringeRowRows(fk) {
  const b = k.budget();
  const now = fringeRowOf(b);
  const send = (patch) => ({
    account: now.account,
    description: obj(b.fringe_row).description || '',
    position: now.position,
    ...patch,
  });
  const acct = k.textInput({
    fieldKey: k.key('settings', 'fringe_row', 'account'),
    value: now.account,
    onCommit: (v, old) =>
      k.run(fk, () => ['set_fringe_row', { row: send({ account: v.trim() }) }], { restore: [k.key('settings', 'fringe_row', 'account'), old] }),
  });
  acct.classList.add('ts-w-s');
  const desc = k.textInput({
    fieldKey: k.key('settings', 'fringe_row', 'description'),
    value: now.description,
    onCommit: (v, old) =>
      k.run(fk, () => ['set_fringe_row', { row: send({ description: v.trim() }) }], { restore: [k.key('settings', 'fringe_row', 'description'), old] }),
  });
  desc.classList.add('ts-w-m');
  // Its place: after the summary rows, or above one of the topsheet's rows.
  const ts = arr(b.topsheet);
  const options = [['', 'After the summary rows']];
  ts.forEach((r, i) => {
    const row = obj(r);
    const label = row.kind === 'production_total' ? k.str(row.name) : `${k.str(row.code)} ${k.str(row.name)}`.trim();
    options.push([String(i), `Above ${label}`]);
  });
  if (ts.length) options.push([String(ts.length), 'After the last topsheet row']);
  const at = now.position === null ? '' : String(Math.min(now.position, ts.length));
  const place = k.selectInput({
    fieldKey: k.key('settings', 'fringe_row', 'position'),
    value: at,
    options,
    onCommit: (v, old) =>
      k.run(fk, () => ['set_fringe_row', { row: send({ position: v === '' ? null : Number(v) }) }], { restore: [k.key('settings', 'fringe_row', 'position'), old] }),
  });
  place.classList.add('ts-w-l');
  return [
    tsRow('Fringe row number', [acct], { hint: 'Blank for none.' }),
    tsRow('Fringe row description', [desc]),
    tsRow('Fringe row place', [place], {
      tip: 'Where the budget’s fringe row shows on the topsheet. Moving it changes no total.',
    }),
  ];
}

// How totals are worked out (budget settings): rounding, account totals in
// whole units, the percent fringe cutoff factor and the fringes of
// categories after the last production total. The file leaves out each at
// its default (binary, off, 1.0, keep).
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
  const factors = ['1.0', '1.5', '2.0', '2.5', '3.0'];
  const factor = factors.includes(k.str(b.cutoff_factor)) ? k.str(b.cutoff_factor) : '1.0';
  const factorSel = k.selectInput({
    fieldKey: k.key('settings', 'cutoff_factor'),
    value: factor,
    options: factors.map((f) => [f, f]),
    onCommit: (v, old) => k.run(ck, () => ['set_cutoff_factor', { value: v }], { restore: [k.key('settings', 'cutoff_factor'), old] }),
  });
  const wholeKey = k.key('settings', 'round_account_totals');
  const whole = checkbox(wholeKey, b.round_account_totals === true, 'Round account totals to whole units', (on) =>
    k.run(ck, () => ['set_round_account_totals', { value: on }], { restore: [wholeKey, !on] }),
  );
  arithSel.classList.add('ts-w-m');
  trailingSel.classList.add('ts-w-s');
  factorSel.classList.add('ts-w-xs');
  group.append(
    heading('Calculation'),
    tsRow('Rounding', [arithSel], {
      hint: '1 x 1.005: 1.00 binary, 1.01 exact.',
      tip: 'Spreadsheet rounding works in binary numbers, so 1 x 1.005 is 1.00; exact decimal makes it 1.01.',
    }),
    tsRow('Account totals', [whole], {
      hint: 'Categories and the grand total add up the rounded accounts.',
      tip: 'Each account’s total is rounded to the nearest whole unit (3,426.50 is 3,427.00), and categories, levels, charges and the grand and net totals are worked out from the rounded accounts. Lines and fringes keep their cents.',
    }),
    tsRow('Percent fringe cutoff factor', [factorSel], {
      hint: 'Multiplies every percent fringe’s cutoff.',
      tip: 'Every percent fringe’s cutoff is multiplied by this: a 2,222.22 cutoff at 1.5 is 3,333.33. Flat fringes have no cutoff.',
    }),
    tsRow('Fringes after the last total', [trailingSel], {
      hint: 'Only when fringes post above each production total.',
      tip: 'Fringes of categories after the last production total. Dropped fringes count in no total; this only applies when fringes post above each production total.',
    }),
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
  const boxes = NEW_LINE_FIELDS.map(([f, label]) => {
    const fieldKey = k.key('settings', 'new_lines', f);
    return checkbox(fieldKey, now[f], label, (on) =>
      k.run(nk, () => ['set_new_line_defaults', { defaults: { ...newLineDefaults(), [f]: on } }], { restore: [fieldKey, !on] }),
    );
  });
  group.append(
    heading('New lines'),
    tsRow('Copy from the line above', boxes, {
      group: true,
      tip: 'A line you insert copies these from the line above it (or the line below, when there is none above).',
    }),
  );
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
  const widthCells = [];
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
    sel.classList.add('ts-w-xs');
    sels[f] = sel;
    widthCells.push(tsInline(label, sel));
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
  sep.classList.add('ts-w-m');

  const example = k.el('span', 'tss-example');
  example.setAttribute('aria-live', 'polite');
  const problem = k.el('p', 'setup-hint setup-hint-bad');
  const read = () => ({
    category_width: Number(sels.category_width.value),
    account_width: Number(sels.account_width.value),
    sub_width: Number(sels.sub_width.value),
    set_width: Number(sels.set_width.value),
    separator: sep.value,
  });
  const apply = k.el('button', 'sw-act sw-act-primary', 'Apply');
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
  const tip =
    'Numbers you type from now on are read and written with this pattern. When numbers already in the budget would change, Apply offers to renumber them.';
  form.append(
    heading('Account numbers'),
    tsRow('Digits per part', widthCells, { group: true, tip }),
    tsRow('Separator', [sep, example, apply]),
    problem,
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

// The action strip (Lock current totals, or Lock again… and Clear…), the
// original, current and change of the grand and net totals as a table, the
// freeze switch (while frozen, Lock again… and Clear… are off; decisions.md,
// entry 61), and what has changed since in the footer.
function renderBaseline(panel) {
  const b = k.budget();
  const view = k.view() || {};
  const variance = view.variance && typeof view.variance === 'object' ? view.variance : null;
  const has = b.baseline && typeof b.baseline === 'object';
  const ik = k.key('baseline');
  const box = k.el('div', 'tss-body');
  box.dataset.item = ik;
  const actions = [];
  const foot = [];

  if (has) {
    const ul = k.el('ul');
    ul.setAttribute('aria-label', 'Original and current totals');
    const line = (label, ch0, fallback) => {
      const ch = obj(ch0);
      const orig = isNone(ch.original) ? fallback : ch.original;
      const v = ch.variance;
      const change = textCell(isNone(v) ? '' : k.formatCents(v), { num: true });
      if (typeof v === 'number' && v !== 0) change.classList.add(v > 0 ? 'tss-up' : 'tss-down');
      const li = gridRow();
      li.append(
        lead(),
        cell(k.el('span', 'setup-name', label)),
        textCell(isNone(orig) ? '' : k.formatCents(orig), { num: true }),
        textCell(isNone(ch.current) ? '' : k.formatCents(ch.current), { num: true }),
        change,
      );
      ul.append(li);
    };
    line('Grand total', variance && variance.grand_total, b.baseline.grand_total);
    line('Net total', variance && variance.net_total, b.baseline.net_total);
    box.append(gridTable('baseline', [lead(), head(''), head('Original', { num: true }), head('Current', { num: true }), head('Change', { num: true })], ul));
    if (variance) {
      // Views list only the rows on screen; the count covers every row.
      const lines = typeof variance.rows_changed === 'number' ? variance.rows_changed : changedCount(variance.rows);
      const accounts = changedCount(variance.accounts);
      foot.push(
        lines || accounts
          ? { label: 'Changed since locked', value: `${k.plural(lines, 'line', 'lines')} in ${k.plural(accounts, 'account', 'accounts')}` }
          : { label: '', value: 'Nothing has changed since the totals were locked' },
      );
    } else {
      foot.push({ label: '', value: 'Current totals can’t be worked out right now, so there is nothing to compare' });
    }
    if (view.partial) box.append(k.el('p', 'setup-hint tss-note', 'Current totals are partial while the budget has errors.'));
    const frozen = b.baseline.frozen === true;
    const frozenKey = k.key('baseline', 'frozen');
    box.append(
      checkbox(frozenKey, frozen, 'Freeze the original totals', (on) =>
        k.run(ik, () => ['set_baseline_frozen', { frozen: on }], { restore: [frozenKey, !on] }),
      ),
    );
    const offWhileFrozen = 'Off while the original totals are frozen';
    const relock = stripButton('Lock again…', k.key('baseline', 'relock'), () => {
      if (!k.isBusy()) confirmBox(ik, relock, 'Replace the original totals with the current ones?', 'Lock again', false, () => ['lock_baseline', {}]);
    }, { title: frozen ? offWhileFrozen : 'Replace the original totals with the current ones', enabled: !frozen });
    const clear = stripButton('Clear…', k.key('baseline', 'clear'), () => {
      if (!k.isBusy()) confirmBox(ik, clear, 'Clear the original totals? The budget stops showing what changed.', 'Clear', true, () => ['clear_baseline', {}]);
    }, { title: frozen ? offWhileFrozen : 'Clear the original totals', enabled: !frozen });
    actions.push(relock, clear);
  } else {
    box.append(
      k.emptyNote(
        view.error
          ? 'No original totals locked yet. The budget has a problem to fix before its totals can be locked.'
          : 'No original totals locked yet.',
      ),
    );
    actions.push(
      stripButton('Lock current totals', k.key('baseline', 'lock'), () => {
        if (k.isBusy()) return;
        k.setFocusKey(k.key('baseline', 'relock'));
        k.run(ik, () => ['lock_baseline', {}], { onError: () => k.setFocusKey(null) });
      }, { primary: true, title: 'Keep the current totals as the original budget' }),
    );
  }
  if (k.state().errors.has(ik)) box.append(k.errorLine(k.state().errors.get(ik)));
  mountFrame(panel, { strip: actionStrip('Baseline', actions), body: [box] });
  setFooterTotals(foot);
}
