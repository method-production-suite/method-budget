// The tools panel's Overtime tab (roadmap 3.10; src/overtime.rs): a small
// calculator for a labor line. Every number is the user's; nothing about any
// union or guild is built in.
//
// The line is the grid's cursor line (detail level). The form takes the
// straight-time hourly rate, hours worked a day, straight-time hours, the
// overtime bands ("then 4 hours at 1.5x", the last one "the rest at 2x"),
// optional sixth and seventh day multipliers and days a week. As it is typed
// the tab shows what a day and a week come to (ot_calculate, exact decimals:
// the pay for each multiplier, straight-time equivalent hours, the average
// an hour). "Write into line" puts the day's or week's pay in as the line's
// rate, or splits the line into one line per multiplier (ot_apply), as one
// undo step either way.
//
// Patterns can be saved in the budget by name (ot_save_pattern, Update,
// Delete with ot_delete_pattern) and picked from the list to fill the form.
// What is typed stays while the panel redraws.
//
// Decimals go to Rust as typed strings; the replies' decimals are strings.

import { el } from './dom.js';
import { invoke } from './api.js';
import { decimal, money } from './format.js';
import { anchoredForm } from './modal.js';
import { obj, str } from './util.js';

const BLANK_FORM = Object.freeze({
  pattern: '', // the saved pattern the form came from ('' for none)
  base_rate: '',
  hours_per_day: '12',
  straight_hours: '8',
  bands: [
    { multiplier: '1.5', hours: '4' },
    { multiplier: '2', hours: '' },
  ],
  sixth_day: '',
  seventh_day: '',
  days_per_week: '5',
  period: 'day',
  write: 'rate',
});

// Form boxes that belong to a pattern (the rest are this calculation's).
const PATTERN_KEYS = new Set(['straight_hours', 'sixth_day', 'seventh_day']);

const st = {
  host: null,
  view: null,
  ctx: null,
  form: JSON.parse(JSON.stringify(BLANK_FORM)),
  result: null,
  error: '',
  message: '',
  seq: 0,
  timer: null,
  sig: '', // the form numbers the result is for
  busy: false,
};

/** Draws the Overtime tab into `host`. */
export function renderOvertime(host, view, ctx) {
  st.host = host;
  st.view = view && typeof view === 'object' ? view : {};
  st.ctx = ctx || {};
  draw();
  schedule();
}

// The cursor moved: the line shown follows it.
for (const type of ['fwb:selection', 'fwb:cursor']) {
  document.addEventListener(type, () => {
    if (st.host && st.host.isConnected && st.host.querySelector('.overtime')) drawLine();
  });
}

function errText(e) {
  return typeof e === 'string' ? e : e && e.message ? e.message : String(e);
}

function patterns() {
  const p = obj(st.view.budget).ot_patterns;
  return Array.isArray(p) ? p : [];
}

/** The cursor line: { id, index, d } at the detail level, or null. */
function cursorLine() {
  const sel = typeof st.ctx.selection === 'function' ? st.ctx.selection() : null;
  if (!sel || sel.level !== 'details') return null;
  const key = typeof sel.cursor === 'string' ? sel.cursor : null;
  if (!key || !key.startsWith('r')) return null;
  const id = Number(key.slice(1));
  const details = Array.isArray(obj(st.view.account).details) ? st.view.account.details : [];
  const index = details.findIndex((d) => d && d.id === id);
  if (index < 0) return null;
  return { id, index, d: details[index] };
}

/** What the form sends as the pattern. */
function patternArg(name = '') {
  const f = st.form;
  return {
    name,
    straight_hours: str(f.straight_hours).trim(),
    bands: f.bands.map((b) => ({ multiplier: str(b.multiplier).trim(), hours: str(b.hours).trim() || null })),
    sixth_day: str(f.sixth_day).trim() || null,
    seventh_day: str(f.seventh_day).trim() || null,
  };
}

function numbersArg() {
  const f = st.form;
  return {
    pattern: patternArg(),
    base_rate: str(f.base_rate).trim(),
    hours_per_day: str(f.hours_per_day).trim(),
    days_per_week: Number(f.days_per_week) || 5,
  };
}

/** Asks for a fresh preview shortly after typing stops. */
function schedule() {
  const a = numbersArg();
  const sig = JSON.stringify(a);
  if (sig === st.sig) return;
  clearTimeout(st.timer);
  st.timer = setTimeout(() => preview(a, sig), 120);
}

async function preview(a, sig) {
  if (!a.base_rate) {
    st.sig = sig;
    st.result = null;
    st.error = '';
    drawResult();
    return;
  }
  const seq = ++st.seq;
  try {
    const r = await invoke('ot_calculate', a);
    if (seq !== st.seq) return;
    st.result = r;
    st.error = '';
  } catch (e) {
    if (seq !== st.seq) return;
    st.result = null;
    st.error = errText(e);
  }
  st.sig = sig;
  drawResult();
}

// ---------------------------------------------------------------------------
// Drawing

function field(label, key, opts = {}) {
  const id = `ot-${key}`;
  const input = el('input', {
    class: 'setup-input setup-num',
    attrs: { id, type: 'text', inputmode: 'decimal', autocomplete: 'off', spellcheck: 'false', placeholder: opts.placeholder || '' },
    data: { key: `ot:${key}` },
    on: {
      input: (e) => {
        st.form[key] = e.target.value;
        // Changing the pattern's own numbers leaves the saved pattern.
        if (PATTERN_KEYS.has(key)) st.form.pattern = '';
        schedule();
      },
    },
  });
  input.value = str(st.form[key]);
  const box = opts.suffix ? el('span', { class: 'setup-adorned' }, input, el('span', { class: 'setup-adorn', text: opts.suffix })) : input;
  return el('label', { class: `setup-field${opts.wide ? ' setup-field-wide' : ''}`, attrs: { for: id } }, el('span', { class: 'setup-label', text: label }), box);
}

function bandRows() {
  const wrap = el('div', { class: 'overtime-bands', attrs: { role: 'group', 'aria-label': 'Overtime bands' } });
  const n = st.form.bands.length;
  st.form.bands.forEach((b, i) => {
    const last = i === n - 1;
    const mult = el('input', {
      class: 'setup-input setup-num overtime-mult',
      attrs: { type: 'text', inputmode: 'decimal', autocomplete: 'off', 'aria-label': `Band ${i + 1} multiplier` },
      data: { key: `ot:band:${i}:m` },
      on: {
        input: (e) => {
          st.form.bands[i].multiplier = e.target.value;
          st.form.pattern = '';
          schedule();
        },
      },
    });
    mult.value = str(b.multiplier);
    const parts = [el('span', { class: 'overtime-word', text: 'then' })];
    if (last) {
      parts.push(el('span', { class: 'overtime-word', text: 'the rest' }));
    } else {
      const hours = el('input', {
        class: 'setup-input setup-num overtime-hours',
        attrs: { type: 'text', inputmode: 'decimal', autocomplete: 'off', 'aria-label': `Band ${i + 1} hours` },
        data: { key: `ot:band:${i}:h` },
        on: {
          input: (e) => {
            st.form.bands[i].hours = e.target.value;
            st.form.pattern = '';
            schedule();
          },
        },
      });
      hours.value = str(b.hours);
      parts.push(hours, el('span', { class: 'overtime-word', text: 'hours' }));
    }
    parts.push(el('span', { class: 'overtime-word', text: 'at' }), mult, el('span', { class: 'overtime-word', text: 'x' }));
    const remove = el('button', {
      class: 'setup-btn overtime-remove',
      text: 'Remove',
      attrs: { type: 'button', 'aria-label': `Remove band ${i + 1}` },
      data: { key: `ot:band:${i}:remove` },
      on: {
        click: () => {
          st.form.bands.splice(i, 1);
          st.form.pattern = '';
          draw();
          schedule();
        },
      },
    });
    wrap.append(el('div', { class: 'overtime-band' }, ...parts, remove));
  });
  const add = el('button', {
    class: 'setup-btn',
    text: 'Add band',
    title: 'Another overtime band (the last one always covers the rest of the hours)',
    attrs: { type: 'button', 'aria-disabled': n >= 8 ? 'true' : null },
    data: { key: 'ot:band:add' },
    on: {
      click: () => {
        if (st.form.bands.length >= 8) return;
        const prev = st.form.bands[st.form.bands.length - 1];
        if (prev && !str(prev.hours).trim()) prev.hours = '2';
        st.form.bands.push({ multiplier: '2', hours: '' });
        st.form.pattern = '';
        draw();
        schedule();
      },
    },
  });
  wrap.append(el('div', { class: 'overtime-actions' }, add));
  return wrap;
}

function patternPicker() {
  const list = patterns();
  const select = el('select', {
    class: 'setup-input setup-select',
    attrs: { id: 'ot-pattern', 'aria-label': 'Saved overtime pattern' },
    data: { key: 'ot:pattern' },
    on: { change: (e) => pick(e.target.value) },
  });
  select.append(el('option', { text: list.length ? 'Not a saved pattern' : 'No saved patterns yet', attrs: { value: '' } }));
  for (const p of list) select.append(el('option', { text: str(p.name), attrs: { value: str(p.name) } }));
  select.value = list.some((p) => p.name === st.form.pattern) ? st.form.pattern : '';
  const named = Boolean(select.value);
  const save = el('button', { class: 'setup-btn', text: 'Save as…', attrs: { type: 'button' }, data: { key: 'ot:save' }, on: { click: (e) => saveAs(e.currentTarget) } });
  const update = el('button', {
    class: 'setup-btn',
    text: 'Update',
    title: 'Save the form into the picked pattern',
    attrs: { type: 'button', 'aria-disabled': named ? null : 'true' },
    data: { key: 'ot:update' },
    on: { click: () => named && savePattern(st.form.pattern, st.form.pattern) },
  });
  const del = el('button', {
    class: 'setup-btn',
    text: 'Delete',
    attrs: { type: 'button', 'aria-disabled': named ? null : 'true' },
    data: { key: 'ot:delete' },
    on: { click: (e) => named && deletePattern(st.form.pattern, e.currentTarget) },
  });
  return el(
    'div',
    { class: 'setup-field setup-field-wide' },
    el('label', { class: 'setup-label', text: 'Pattern', attrs: { for: 'ot-pattern' } }),
    el('div', { class: 'overtime-row' }, select, save, update, del),
  );
}

function pick(name) {
  const p = patterns().find((x) => x.name === name);
  st.form.pattern = p ? name : '';
  if (p) {
    st.form.straight_hours = str(p.straight_hours);
    st.form.bands = (Array.isArray(p.bands) ? p.bands : []).map((b) => ({ multiplier: str(b.multiplier), hours: str(b.hours) }));
    st.form.sixth_day = str(p.sixth_day);
    st.form.seventh_day = str(p.seventh_day);
  }
  draw();
  schedule();
}

async function saveAs(anchor) {
  await anchoredForm({
    anchor,
    title: 'Save overtime pattern',
    message: 'Straight time, the bands and the sixth and seventh day multipliers, kept in this budget.',
    fields: [{ key: 'name', label: 'Name', value: st.form.pattern || '' }],
    submitText: 'Save',
    onSubmit: (v) => savePattern(str(v.name).trim(), null),
  });
}

async function savePattern(name, replacing) {
  if (!st.ctx || typeof st.ctx.call !== 'function') return;
  const before = st.form.pattern;
  // Set first: the call redraws the panel from the new view.
  st.form.pattern = name;
  st.message = `Saved ${name}.`;
  try {
    await st.ctx.call('ot_save_pattern', { pattern: patternArg(name), replacing });
  } catch (e) {
    st.form.pattern = before;
    st.message = '';
    throw e;
  }
}

async function deletePattern(name, anchor) {
  const ok = await anchoredForm({ anchor, title: `Delete ${name}?`, message: 'Lines already written with it keep their numbers.', submitText: 'Delete', danger: true, onSubmit: () => Promise.resolve() });
  if (!ok || !st.ctx || typeof st.ctx.call !== 'function') return;
  try {
    await st.ctx.call('ot_delete_pattern', { name });
    st.form.pattern = '';
  } catch (e) {
    st.error = errText(e);
    draw();
  }
}

function draw() {
  const host = st.host;
  if (!host) return;
  const a = document.activeElement;
  const focusKey = a && host.contains(a) && a.dataset ? a.dataset.key : null;
  const caret = a && typeof a.selectionStart === 'number' ? [a.selectionStart, a.selectionEnd] : null;

  const f = st.form;
  const box = el('div', { class: 'overtime' });
  box.append(el('div', { class: 'overtime-line', attrs: { 'aria-live': 'polite' } }));
  const days = el('select', {
    class: 'setup-input setup-select',
    attrs: { id: 'ot-days_per_week', 'aria-label': 'Days worked a week' },
    data: { key: 'ot:days_per_week' },
    on: {
      change: (e) => {
        f.days_per_week = e.target.value;
        schedule();
      },
    },
  });
  for (let n = 1; n <= 7; n++) days.append(el('option', { text: String(n), attrs: { value: String(n) } }));
  days.value = str(f.days_per_week);
  box.append(
    el(
      'div',
      { class: 'setup-fields' },
      patternPicker(),
      field('Straight-time rate', 'base_rate', { suffix: 'an hour', placeholder: '0.00' }),
      field('Hours worked a day', 'hours_per_day', { suffix: 'hours' }),
      field('Straight time', 'straight_hours', { suffix: 'hours', wide: true }),
    ),
    bandRows(),
    el(
      'div',
      { class: 'setup-fields' },
      field('Sixth day', 'sixth_day', { suffix: 'x', placeholder: 'regular' }),
      field('Seventh day', 'seventh_day', { suffix: 'x', placeholder: 'regular' }),
      el('label', { class: 'setup-field', attrs: { for: 'ot-days_per_week' } }, el('span', { class: 'setup-label', text: 'Days a week' }), days),
    ),
    el('div', { class: 'overtime-result', attrs: { 'aria-live': 'polite' } }),
    writeControls(),
  );
  host.replaceChildren(box);
  drawLine();
  drawResult();
  if (focusKey) {
    const again = [...host.querySelectorAll('[data-key]')].find((n) => n.dataset.key === focusKey);
    if (again) {
      again.focus({ preventScroll: true });
      if (caret && typeof again.setSelectionRange === 'function') {
        try {
          again.setSelectionRange(caret[0], caret[1]);
        } catch {
          // Not a text box.
        }
      }
    }
  }
}

function drawLine() {
  const slot = st.host && st.host.querySelector('.overtime-line');
  if (!slot) return;
  const line = cursorLine();
  if (!line) {
    slot.replaceChildren(el('p', { class: 'setup-hint', text: 'Put the cursor on a line in an account to write into it. The calculator works without one.' }));
  } else {
    const d = line.d;
    const what = str(d.description).trim() || 'no description';
    const kind = d.kind || 'line';
    const nums = [str(d.amount), str(d.unit), str(d.rate) ? `at ${str(d.rate)}` : ''].filter(Boolean).join(' ');
    const parts = [el('p', { class: 'overtime-target' }, el('strong', { text: `Line ${line.index + 1}: ${what}` }), nums ? ` (${nums})` : '')];
    if (kind !== 'line') parts.push(el('p', { class: 'setup-hint', text: 'Overtime goes on a costed line, not a subtotal, adjustment or separator.' }));
    slot.replaceChildren(...parts);
  }
  const btn = st.host.querySelector('[data-key="ot:write"]');
  if (btn) btn.setAttribute('aria-disabled', canWrite() ? 'false' : 'true');
}

function canWrite() {
  const line = cursorLine();
  return Boolean(line && (line.d.kind || 'line') === 'line' && st.result && !st.busy);
}

function sliceRows(slices) {
  return slices.map((s) =>
    el(
      'li',
      { class: 'overtime-slice' },
      el('span', { text: `${decimal(s.hours)} ${s.hours === '1' ? 'hour' : 'hours'} at ${decimal(s.multiplier)}x` }),
      el('span', { class: 'overtime-pay', text: payText(s.pay) }),
    ),
  );
}

/**
 * A decimal string rounded to two places, half away from zero, as text
 * ("64.285714286" is "64.29"). String and integer math only, no floats.
 */
export function cents2(s) {
  const t = str(s).trim();
  const neg = t.startsWith('-');
  const [int = '0', frac = ''] = (neg ? t.slice(1) : t).split('.');
  if (!/^\d+$/.test(int) || !/^\d*$/.test(frac)) return t;
  let c = BigInt(int) * 100n + BigInt(frac.padEnd(2, '0').slice(0, 2));
  if (Number(frac[2] || '0') >= 5) c += 1n;
  const v = Number(c);
  return money(neg && c !== 0n ? -v : v);
}

/** An exact decimal amount: two places, more when it has them. */
function payText(s) {
  const t = str(s);
  const [int, frac = ''] = t.split('.');
  return decimal(`${int}.${frac.padEnd(2, '0')}`);
}

function drawResult() {
  const slot = st.host && st.host.querySelector('.overtime-result');
  if (!slot) return;
  const out = [];
  if (st.error) out.push(el('p', { class: 'setup-error', text: st.error, attrs: { role: 'alert' } }));
  if (st.message) {
    out.push(el('p', { class: 'setup-hint', text: st.message }));
    st.message = '';
  }
  const r = st.result;
  if (!r) {
    if (!st.error) out.push(el('p', { class: 'setup-hint', text: 'Type the straight-time rate to see what a day and a week come to.' }));
  } else {
    out.push(el('p', { class: 'setup-hint', text: r.describe }));
    out.push(el('h3', { class: 'overtime-h', text: 'A day' }), el('ul', { class: 'overtime-slices' }, ...sliceRows(r.day.slices)));
    out.push(
      el(
        'p',
        { class: 'overtime-sum' },
        el('strong', { text: `Day: ${payText(r.day.pay)}` }),
        ` (${decimal(r.day.equivalent_hours)} straight-time hours${r.day_hourly ? `, ${cents2(r.day_hourly)} an hour on average` : ''})`,
      ),
    );
    const n = r.week.length;
    const special = r.week.filter((d) => d.kind !== 'regular').map((d) => `${d.kind} day ${payText(d.pay)}`);
    out.push(el('h3', { class: 'overtime-h', text: `A week of ${n} ${n === 1 ? 'day' : 'days'}` }), el('ul', { class: 'overtime-slices' }, ...sliceRows(r.week_slices)));
    out.push(
      el(
        'p',
        { class: 'overtime-sum' },
        el('strong', { text: `Week: ${payText(r.week_pay)}` }),
        ` (${decimal(r.week_equivalent_hours)} straight-time hours${special.length ? `; ${special.join(', ')}` : ''})`,
      ),
    );
  }
  slot.replaceChildren(...out);
  const btn = st.host.querySelector('[data-key="ot:write"]');
  if (btn) btn.setAttribute('aria-disabled', canWrite() ? 'false' : 'true');
}

function radio(name, value, label, title) {
  const id = `ot-${name}-${value}`;
  const input = el('input', {
    attrs: { type: 'radio', name: `ot-${name}`, id, value },
    data: { key: `ot:${name}:${value}` },
    on: {
      change: () => {
        st.form[name] = value;
      },
    },
  });
  input.checked = st.form[name] === value;
  return el('label', { class: 'overtime-choice', title, attrs: { for: id } }, input, ` ${label}`);
}

function writeControls() {
  const write = el('button', {
    class: 'setup-btn setup-btn-primary',
    text: 'Write into line',
    title: 'One undo step',
    attrs: { type: 'button', 'aria-disabled': 'true' },
    data: { key: 'ot:write' },
    on: { click: () => writeIntoLine() },
  });
  return el(
    'div',
    { class: 'overtime-write' },
    el(
      'div',
      { class: 'overtime-row', attrs: { role: 'radiogroup', 'aria-label': 'Pay for' } },
      radio('period', 'day', 'Per day', 'The line counts days: a regular day'),
      radio('period', 'week', 'Per week', 'The line counts weeks: every day of the week'),
    ),
    el(
      'div',
      { class: 'overtime-row', attrs: { role: 'radiogroup', 'aria-label': 'Write as' } },
      radio('write', 'rate', 'As the rate', "The line's rate becomes the pay, exactly; the line rounds once"),
      radio('write', 'split', 'As split lines', 'One line per multiplier (straight time, then each overtime rate), in one fringe range'),
    ),
    el('div', { class: 'overtime-actions' }, write),
  );
}

async function writeIntoLine() {
  if (!canWrite() || !st.ctx || typeof st.ctx.call !== 'function') return;
  const line = cursorLine();
  st.busy = true;
  try {
    await st.ctx.call('ot_apply', { row_id: line.id, ...numbersArg(), period: st.form.period, write: st.form.write });
    st.message = st.form.write === 'split' ? 'Split the line by multiplier.' : "Wrote the line's rate.";
    st.error = '';
  } catch (e) {
    st.error = errText(e);
  } finally {
    st.busy = false;
  }
  if (st.host && st.host.isConnected) drawResult();
}
