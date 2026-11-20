// Budget Setup's tabs (topsheet-setup.js and its topsheet-setup-*.js parts):
// the helpers and table pieces every tab shares.

import { bandField } from './setupframe.js';
import { obj } from './util.js';
import { t } from './i18n.js';
import { picked } from './topsheet-setup.js';

let k = null; // the kit from setup.js, set by useKit() on every render

/** Takes the kit for this render (topsheet-setup.js calls it). */
export function useKit(kit) {
  k = kit;
}

export const DECIMAL = /^[+-]?(\d+\.?\d*|\.\d+)$/;
export const DATE = /^\d{4}-\d{2}-\d{2}$/;
export const SEPARATORS = [
  ['-', t('topsheet.sep.hyphen')],
  ['.', t('topsheet.sep.point')],
  [' ', t('topsheet.sep.space')],
  ['/', t('topsheet.sep.slash')],
  [':', t('topsheet.sep.colon')],
  [',', t('topsheet.sep.comma')],
  [';', t('topsheet.sep.semicolon')],
  ['', t('topsheet.sep.none')],
];
// ---------------------------------------------------------------------------
// Small helpers

export function arr(v) {
  return Array.isArray(v) ? v : [];
}

export function isNone(v) {
  return v === undefined || v === null;
}

export function heading(text) {
  return k.el('h3', 'tss-heading', text);
}

export function issuesAt(kind, index) {
  const list = k.view() && k.view().issues;
  if (!Array.isArray(list)) return [];
  return list.filter((i) => i && i.target && i.target.kind === kind && i.target.index === index);
}

// The report entry for item `index` of `list`: reports only list applied items.
export function appliedEntry(items, reported, index) {
  if (!Array.isArray(reported)) return undefined;
  if (obj(items[index]).applied === false) return undefined;
  let n = 0;
  for (let i = 0; i < index; i += 1) if (obj(items[i]).applied !== false) n += 1;
  return reported[n];
}

export function button(text, cls, fieldKey, onClick, label) {
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

export function checkbox(fieldKey, checked, labelText, onChange) {
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
export function windowCtx() {
  const c = k.state().ctx || {};
  return {
    call: (cmd, args) => (typeof c.call === 'function' ? c.call(cmd, args) : Promise.reject(new Error(t('topsheet.not_connected')))),
    toast: (m) => typeof c.toast === 'function' && c.toast(m),
  };
}

// A field in a band (setupframe.js): its label, the control, then a unit
// like "%". `size` is a width class for the control.
export function bandInput(text, control, { size = '', unit = '', title = '' } = {}) {
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
export function bandTitle(text) {
  return k.el('span', 'sw-band-title', text);
}

// A compact button in a band (the strip's style), acting on the picked item.
export function bandButton(text, fieldKey, onClick) {
  return button(text, 'sw-act', fieldKey, onClick);
}

// Chips for a set of names picked from `options`, with a select to add one,
// as a band field.
export function chips({ fieldKey, label, selected, options, onChange, emptyText = t('topsheet.none') }) {
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
        t('topsheet.remove_named', { name }),
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
    sel.setAttribute('aria-label', t('topsheet.add_to', { label }));
    const first = k.el('option', null, t('topsheet.add_ellipsis'));
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
export function confirmBox(itemKey, opener, text, yesText, danger, build) {
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
  const no = k.el('button', 'setup-btn setup-btn-quiet', t('topsheet.cancel'));
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

export function groupNames() {
  return k.entries(k.budget().groups).map(([n]) => n);
}

export function fringeNames() {
  return k.entries(k.budget().fringes).map(([n]) => n);
}

export function locationNames() {
  return k.entries(k.budget().locations).map(([n]) => n);
}

export function categoryCodes() {
  return arr(k.budget().topsheet)
    .filter((r) => obj(r).kind === 'category')
    .map((r) => k.str(r.code));
}

// Category codes with their names, in topsheet order.
export function categoryNames() {
  const out = new Map();
  for (const r of arr(k.budget().topsheet)) {
    if (obj(r).kind === 'category') out.set(k.str(r.code), k.str(r.name));
  }
  return out;
}

// Account codes with their names, in topsheet order.
export function accountNames() {
  const out = new Map();
  for (const r of arr(k.budget().topsheet)) {
    if (obj(r).kind !== 'category') continue;
    for (const a of arr(r.accounts)) out.set(k.str(obj(a).code), k.str(obj(a).name));
  }
  return out;
}

// Names already picked stay listed even if they no longer exist.
export function withKept(options, picked) {
  const out = [...options];
  for (const p of picked) if (!out.includes(p)) out.push(p);
  return out;
}

// A select that reads as a small button: picking an option runs
// `onPick(value)` and the select goes back to its title.
export function addPicker(fieldKey, title, options, noneText, onPick) {
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
export function head(text, { num = false, title = '' } = {}) {
  const h = k.el('span', num ? 'sw-head sw-num' : 'sw-head', text);
  if (title) h.title = title;
  return h;
}

/** A cell holding `content` (a control, or nothing). */
export function cell(content, cls = '') {
  const c = k.el('span', cls ? `sw-cell ${cls}` : 'sw-cell');
  if (content instanceof Node) c.append(content);
  return c;
}

/** A cell of text: a figure (`num`), muted text, or plain text. */
export function textCell(text, { num = false, muted = false, title = '' } = {}) {
  const span = k.el('span', num ? 'sw-num' : muted ? 'sw-cell-text' : 'sw-cell-label', text);
  const c = cell(span, num ? 'sw-num' : '');
  if (title) c.title = title;
  return c;
}

/** The first column, where the list tables keep their drag handle. */
export function lead() {
  return k.el('span', 'sw-cell');
}

/** A row; a focus inside it selects it, as in the list tables. */
export function gridRow() {
  const li = k.el('li', 'sw-grid-row');
  li.addEventListener('focusin', () => {
    const ul = li.parentElement;
    if (ul) for (const x of ul.querySelectorAll(':scope > .is-selected')) if (x !== li) x.classList.remove('is-selected');
    li.classList.add('is-selected');
  });
  return li;
}

/** A row's own buttons (delete), at its right end. */
export function acts(...buttons) {
  const c = k.el('span', 'sw-cell sw-acts');
  c.append(...buttons);
  return c;
}

// Cents (numbers or BigInt) added up exactly.
export function centsSum(list) {
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
export function decimalSum(list) {
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
