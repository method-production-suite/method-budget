// Budget Setup's Rate Tables tab (roadmap 3.9; src/rates.rs; decisions.md,
// entry 90): the rate tables this budget keeps. Each table is the user's:
// classifications with a rate per unit and, when figures change over time,
// the dates each row is in effect. Nothing comes with the app.
//
// The rail lists the tables; the main area edits the one picked as a draft
// (typing changes nothing until Save Table, one undo step), with a column
// per unit. Import from Spreadsheet… reads a sheet the user has into a new
// table, its columns mapped as the spreadsheet import maps lines. Update
// Filled Lines brings every line filled from a table up to what its table
// says now (one undo step); nothing else ever changes a line's figure.
//
// Commands: rate_table_save, rate_table_delete, rate_update,
// rate_import_pick and rate_import_read (capabilities/setup.json).

import { el } from './dom.js';
import { customDialog, confirmDialog } from './modal.js';
import { mountFrame, actionStrip, stripButton, searchBox, gridTable, band, bandField, setFooterTotals, railHost } from './setupframe.js';
import { setRules } from './sheet.js';
import { obj, str, plural, errText } from './util.js';

const state = {
  // The table picked in the rail (its saved name), or null.
  selected: null,
  // The table being edited ({ name, notes, rows: [{ classification, rates, from, to, notes }] }).
  draft: null,
  // Whether the draft differs from what is saved, and what it was made from.
  dirty: false,
  base: '',
  // The unit columns shown (units with a figure, plus ones added).
  units: [],
  filter: '',
  // Where it was drawn last, to draw again after a local change.
  panel: null,
  view: null,
  ctx: null,
};

function tables(view) {
  const list = obj(view && view.budget).rate_tables;
  return Array.isArray(list) ? list : [];
}

function budgetUnits(view) {
  return Object.keys(obj(obj(view && view.budget).units)).sort();
}

function clone(t) {
  return {
    name: str(t.name),
    notes: str(t.notes),
    rows: (Array.isArray(t.rows) ? t.rows : []).map((r) => ({
      classification: str(r.classification),
      rates: { ...obj(r.rates) },
      from: str(r.from),
      to: str(r.to),
      notes: str(r.notes),
    })),
  };
}

function unitsOf(t) {
  const set = new Set();
  for (const r of t.rows) for (const u of Object.keys(r.rates)) set.add(u);
  return [...set].sort();
}

/** Takes the view's tables in: a fresh draft unless one is being edited. */
function sync(view) {
  const list = tables(view);
  if (state.dirty) return;
  let t = list.find((x) => x.name === state.selected);
  if (!t) {
    t = list[0] || null;
    state.selected = t ? t.name : null;
  }
  state.draft = t ? clone(t) : null;
  state.base = JSON.stringify(state.draft);
  const keep = state.units.filter((u) => budgetUnits(view).includes(u));
  state.units = state.draft ? [...new Set([...unitsOf(state.draft), ...keep])].sort() : [];
}

function touch() {
  state.dirty = JSON.stringify(state.draft) !== state.base;
}

function redraw() {
  if (state.panel && state.panel.isConnected) {
    state.panel.replaceChildren();
    draw();
  }
}

async function guardLeave() {
  if (!state.dirty) return true;
  return confirmDialog({
    title: 'Leave without saving?',
    message: `The changes to ${state.draft && state.draft.name ? state.draft.name : 'this table'} aren't saved. Leave them?`,
    confirmText: 'Leave Without Saving',
    danger: true,
  });
}

async function pick(name) {
  if (name === state.selected && !state.dirty) return;
  if (!(await guardLeave())) return;
  state.dirty = false;
  state.selected = name;
  sync(state.view);
  redraw();
}

async function newTable() {
  if (!(await guardLeave())) return;
  const names = new Set(tables(state.view).map((t) => t.name));
  let name = 'New rate table';
  for (let n = 2; names.has(name); n += 1) name = `New rate table ${n}`;
  state.selected = null;
  state.draft = { name, notes: '', rows: [{ classification: '', rates: {}, from: '', to: '', notes: '' }] };
  state.base = '';
  state.dirty = true;
  const day = budgetUnits(state.view).find((u) => u === 'Day');
  state.units = day ? [day] : budgetUnits(state.view).slice(0, 1);
  redraw();
}

async function save() {
  if (!state.draft) return;
  try {
    await state.ctx.call('rate_table_save', { table: state.draft, replacing: state.selected });
    state.selected = state.draft.name.trim();
    state.dirty = false;
    sync(state.view);
    redraw();
  } catch (e) {
    state.ctx.toast(errText(e));
  }
}

function revert() {
  state.dirty = false;
  sync(state.view);
  redraw();
}

async function remove() {
  if (!state.selected) {
    revert();
    return;
  }
  const yes = await confirmDialog({
    title: `Delete the rate table ${state.selected}?`,
    message: 'Lines filled from it keep their rates; they will say the table is gone. Undo brings the table back.',
    confirmText: 'Delete Table',
    danger: true,
  });
  if (!yes) return;
  try {
    const name = state.selected;
    state.dirty = false;
    state.selected = null;
    await state.ctx.call('rate_table_delete', { name });
  } catch (e) {
    state.ctx.toast(errText(e));
  }
}

async function updateLines() {
  try {
    await state.ctx.call('rate_update', { row_ids: [] });
    state.ctx.toast('The lines filled from rate tables now have what their tables say.');
  } catch (e) {
    state.ctx.toast(errText(e));
  }
}

// ---------------------------------------------------------------------------
// Drawing

function input(value, key, label, onInput, cls = '') {
  const i = el('input', {
    class: `setup-input sw-cell-input${cls ? ` ${cls}` : ''}`,
    attrs: { type: 'text', 'aria-label': label, autocomplete: 'off', spellcheck: 'false' },
    data: { key },
  });
  i.value = value;
  i.addEventListener('input', () => {
    onInput(i.value);
    touch();
    markDirty();
  });
  return i;
}

/** Shows Save and Revert as the draft changes, without a redraw. */
function markDirty() {
  const host = state.panel;
  if (!host) return;
  for (const b of host.querySelectorAll('[data-dirty]')) b.setAttribute('aria-disabled', state.dirty ? 'false' : 'true');
}

function rail() {
  const host = railHost();
  if (!host) return;
  host.replaceChildren();
  const list = el('ul', { class: 'rates-rail-list', attrs: { 'aria-label': 'Rate tables' } });
  for (const t of tables(state.view)) {
    const on = t.name === state.selected;
    const rows = Array.isArray(t.rows) ? t.rows.length : 0;
    const b = el('button', {
      class: `rates-rail-item${on ? ' is-selected' : ''}`,
      text: t.name,
      title: `${t.name}: ${plural(rows, 'row', 'rows')}`,
      attrs: { type: 'button', 'aria-pressed': String(on) },
      data: { key: `rates:table:${t.name}`, nobusy: '1' },
    });
    b.addEventListener('click', () => pick(t.name));
    list.append(el('li', {}, b));
  }
  host.append(list);
}

function columnsRule(n) {
  const units = Array.from({ length: n }, () => 'minmax(64px, 96px)').join(' ');
  setRules(
    'rates-cols',
    `.sw-cols-rates .sw-grid-head, .sw-cols-rates .sw-grid-row { grid-template-columns: minmax(140px, 1.4fr) ${units} 104px 104px minmax(80px, 1fr) 28px; }`,
  );
}

function unitAdder() {
  const missing = budgetUnits(state.view).filter((u) => !state.units.includes(u));
  if (!missing.length) return null;
  const s = el('select', {
    class: 'setup-input rates-add-unit',
    attrs: { 'aria-label': 'Add a unit column' },
    data: { key: 'rates:add-unit' },
  });
  s.append(el('option', { text: 'Add unit…', attrs: { value: '' } }));
  for (const u of missing) s.append(el('option', { text: u, attrs: { value: u } }));
  s.addEventListener('change', () => {
    if (!s.value) return;
    state.units = [...state.units, s.value].sort();
    redraw();
  });
  return s;
}

function grid() {
  const d = state.draft;
  const head = (text, cls = '') => el('span', { class: `sw-head${cls ? ` ${cls}` : ''}`, text });
  const heads = [head('Classification'), ...state.units.map((u) => head(`Per ${u}`, 'sw-num')), head('From'), head('To'), head('Notes'), head('')];
  const list = el('ul', { attrs: { 'aria-label': `Rows of ${d.name}` } });
  const needle = state.filter.trim().toLowerCase();
  d.rows.forEach((r, i) => {
    if (needle && !r.classification.toLowerCase().includes(needle)) return;
    const cells = [input(r.classification, `rates:row:${i}:class`, `Row ${i + 1} classification`, (v) => (r.classification = v))];
    for (const u of state.units) {
      cells.push(
        input(str(r.rates[u]), `rates:row:${i}:${u}`, `Row ${i + 1} rate per ${u}`, (v) => {
          if (v.trim()) r.rates[u] = v.trim();
          else delete r.rates[u];
        }, 'sw-num'),
      );
    }
    cells.push(input(r.from, `rates:row:${i}:from`, `Row ${i + 1} first day (YYYY-MM-DD)`, (v) => (r.from = v)));
    cells.push(input(r.to, `rates:row:${i}:to`, `Row ${i + 1} last day (YYYY-MM-DD)`, (v) => (r.to = v)));
    cells.push(input(r.notes, `rates:row:${i}:notes`, `Row ${i + 1} notes`, (v) => (r.notes = v)));
    const del = el('button', {
      class: 'setup-btn sw-acts',
      text: '×',
      title: 'Remove this row',
      attrs: { type: 'button', 'aria-label': `Remove row ${i + 1}` },
      data: { key: `rates:row:${i}:remove`, nobusy: '1' },
    });
    del.addEventListener('click', () => {
      d.rows.splice(i, 1);
      touch();
      redraw();
    });
    cells.push(del);
    list.append(el('li', { class: 'sw-grid-row' }, ...cells.map((c) => (c.tagName === 'BUTTON' ? c : el('span', { class: 'sw-cell' }, c)))));
  });
  columnsRule(state.units.length);
  return gridTable('rates', heads, list);
}

function draw() {
  const panel = state.panel;
  const d = state.draft;
  const actions = [
    stripButton('New Table', 'rates:new', () => newTable(), { title: 'Start a rate table of your own' }),
    stripButton('Import from Spreadsheet…', 'rates:import', () => importDialog(), {
      title: 'Read a rate table from a spreadsheet you have (.xlsx or CSV), mapping its columns',
    }),
  ];
  if (d) {
    const saveBtn = stripButton('Save Table', 'rates:save', () => save(), { primary: true, title: 'Keep these changes (one undo step)' });
    saveBtn.dataset.dirty = '1';
    saveBtn.setAttribute('aria-disabled', state.dirty ? 'false' : 'true');
    const revertBtn = stripButton('Revert', 'rates:revert', () => revert(), { title: 'Drop the changes not saved' });
    revertBtn.dataset.dirty = '1';
    revertBtn.setAttribute('aria-disabled', state.dirty ? 'false' : 'true');
    actions.push(saveBtn, revertBtn, stripButton('Delete Table…', 'rates:delete', () => remove(), { title: 'Delete this rate table' }));
  }
  actions.push(
    stripButton('Update Filled Lines', 'rates:update', () => updateLines(), {
      title: 'Give every line filled from a rate table what its table says now (one undo step). Lines changed by hand are left as they are.',
    }),
  );
  const search = d
    ? searchBox({
        key: 'rates:search',
        value: state.filter,
        label: 'Search classifications',
        placeholder: 'Search classifications',
        onChange: (v) => {
          state.filter = v;
          redraw();
        },
      })
    : null;
  const strip = actionStrip('Rate tables', actions, search);
  if (!d) {
    mountFrame(panel, {
      strip,
      body: [
        el('p', {
          class: 'setup-empty',
          text: 'No rate tables yet. A rate table is yours to keep: classifications with a rate per unit, typed in or imported from a spreadsheet. Pick one on a line to fill its rate. Nothing comes with the app.',
        }),
      ],
    });
    rail();
    setFooterTotals([{ label: '', value: '0 rate tables' }]);
    return;
  }
  const name = input(d.name, 'rates:name', 'Table name', (v) => (d.name = v));
  const notes = input(d.notes, 'rates:notes', 'Table notes', (v) => (d.notes = v));
  const adder = unitAdder();
  const bandEl = band('Rate table', bandField('Name', name), bandField('Notes', notes, 'Where the figures came from, what they cover'), adder ? bandField('Units', adder) : null);
  const add = el('button', {
    class: 'setup-btn sw-after',
    text: 'Add classification',
    attrs: { type: 'button' },
    data: { key: 'rates:add-row', nobusy: '1' },
  });
  add.addEventListener('click', () => {
    d.rows.push({ classification: '', rates: {}, from: '', to: '', notes: '' });
    touch();
    redraw();
  });
  mountFrame(panel, { strip, band: bandEl, body: [grid()], after: [add] });
  rail();
  const classes = new Set(d.rows.map((r) => r.classification.trim().toLowerCase()).filter(Boolean));
  setFooterTotals([
    { label: '', value: plural(classes.size, 'classification', 'classifications') },
    { label: '', value: plural(d.rows.length, 'row', 'rows') },
    state.dirty ? { label: '', value: 'Not saved', dim: true } : null,
  ]);
}

/** setupwin.js: draws the Rate Tables tab into `panel`. */
export function renderRatesTab(panel, view, ctx) {
  state.panel = panel;
  state.view = view;
  state.ctx = ctx;
  sync(view);
  draw();
}

// ---------------------------------------------------------------------------
// Import from a spreadsheet

const FIELDS = [
  ['classification', 'Classification'],
  ['from', 'From'],
  ['to', 'To'],
  ['notes', 'Notes'],
  ['unit', 'Unit column'],
  ['rate', 'Rate column'],
];

async function importDialog() {
  let read;
  try {
    read = await state.ctx.call('rate_import_pick', {});
  } catch (e) {
    state.ctx.toast(errText(e));
    return;
  }
  if (!read) return;
  let current = read;
  let name = read.name;
  const body = el('div', { class: 'rates-import-body' });
  const error = el('p', { class: 'modal-error', attrs: { role: 'alert' } });
  error.hidden = true;
  const importBtn = el('button', { class: 'btn btn-primary', text: 'Import', attrs: { type: 'button' } });

  const reread = async (mapping, index = current.index) => {
    try {
      current = await state.ctx.call('rate_import_read', { token: current.token, index, mapping, name });
      fill();
    } catch (e) {
      error.textContent = errText(e);
      error.hidden = false;
    }
  };

  const colSelect = (label, value, onChange) => {
    const s = el('select', { class: 'modal-input', attrs: { 'aria-label': label } });
    s.append(el('option', { text: '(none)', attrs: { value: '' } }));
    current.titles.forEach((t, i) => s.append(el('option', { text: t, attrs: { value: String(i) } })));
    s.value = value === null || value === undefined ? '' : String(value);
    s.addEventListener('change', () => onChange(s.value === '' ? null : Number(s.value)));
    return s;
  };

  const fill = () => {
    error.hidden = true;
    const m = { ...current.mapping, units: { ...obj(current.mapping.units) } };
    const rows = [];
    const sheet = el('select', { class: 'modal-input', attrs: { 'aria-label': 'Sheet' } });
    for (const s of current.sheets) sheet.append(el('option', { text: `${s.name} (${plural(s.rows, 'row', 'rows')})`, attrs: { value: String(s.index) } }));
    sheet.value = String(current.index);
    sheet.addEventListener('change', () => reread(null, Number(sheet.value)));
    if (current.sheets.length > 1) rows.push(el('label', { class: 'modal-field' }, el('span', { text: 'Sheet' }), sheet));
    const nameIn = el('input', { class: 'modal-input', attrs: { type: 'text', 'aria-label': 'Table name' } });
    nameIn.value = name;
    nameIn.addEventListener('change', () => {
      name = nameIn.value;
      reread(m);
    });
    rows.push(el('label', { class: 'modal-field' }, el('span', { text: 'Table name' }), nameIn));
    const header = el('input', { class: 'modal-input', attrs: { type: 'number', min: '0', 'aria-label': 'Row of column titles (0 for none)' } });
    header.value = m.header_row === null || m.header_row === undefined ? '0' : String(m.header_row + 1);
    header.addEventListener('change', () => {
      const n = Number(header.value);
      reread({ ...m, header_row: n > 0 ? n - 1 : null });
    });
    rows.push(el('label', { class: 'modal-field' }, el('span', { text: 'Titles on row' }), header));
    for (const [k, label] of FIELDS) {
      rows.push(el('label', { class: 'modal-field' }, el('span', { text: label }), colSelect(label, m[k], (v) => reread({ ...m, [k]: v }))));
    }
    for (const u of current.units) {
      rows.push(
        el(
          'label',
          { class: 'modal-field' },
          el('span', { text: `Rate per ${u}` }),
          colSelect(`Rate per ${u}`, m.units[u], (v) => {
            const units = { ...m.units };
            if (v === null) delete units[u];
            else units[u] = v;
            reread({ ...m, units });
          }),
        ),
      );
    }
    const out = [];
    if (current.errors.length) {
      const ul = el('ul', { class: 'rates-import-errors', attrs: { 'aria-label': 'Problems' } });
      for (const e of current.errors.slice(0, 50)) ul.append(el('li', { text: `Row ${e.row}: ${e.message}` }));
      out.push(el('p', { class: 'modal-message', text: 'Nothing is imported while there are problems:' }), ul);
    } else if (current.table) {
      const t = current.table;
      const units = unitsOf(t);
      const ul = el('ul', { class: 'rates-import-preview', attrs: { 'aria-label': 'What the table will hold' } });
      for (const r of t.rows.slice(0, 40)) {
        const figures = units.filter((u) => r.rates[u] !== undefined).map((u) => `${r.rates[u]} per ${u}`);
        const when = [r.from && `from ${r.from}`, r.to && `to ${r.to}`].filter(Boolean).join(' ');
        ul.append(el('li', { text: `${r.classification}: ${figures.join(', ')}${when ? ` (${when})` : ''}` }));
      }
      out.push(el('p', { class: 'modal-message', text: `${plural(t.rows.length, 'row', 'rows')} in ${t.name}${current.exists ? '. This replaces the table of that name.' : '.'}` }), ul);
    }
    importBtn.textContent = current.exists ? 'Replace Table' : 'Import';
    importBtn.disabled = !current.table;
    body.replaceChildren(el('div', { class: 'rates-import-map' }, ...rows), ...out);
  };

  customDialog((close) => {
    const cancel = el('button', { class: 'btn', text: 'Cancel', attrs: { type: 'button' } });
    cancel.addEventListener('click', () => close());
    importBtn.addEventListener('click', async () => {
      if (!current.table) return;
      try {
        await state.ctx.call('rate_table_save', { table: current.table, replacing: current.exists ? current.table.name : null });
        state.dirty = false;
        state.selected = current.table.name;
        close();
        sync(state.view);
        redraw();
      } catch (e) {
        error.textContent = errText(e);
        error.hidden = false;
      }
    });
    fill();
    return el(
      'div',
      { class: 'modal rates-import', attrs: { 'aria-labelledby': 'rates-import-title' } },
      el('h2', { class: 'modal-title', text: `Import a rate table from ${current.file}`, attrs: { id: 'rates-import-title' } }),
      el('p', { class: 'modal-message', text: 'Map the sheet’s columns: a classification column, then a column for each unit’s rate, or a unit column and a rate column.' }),
      body,
      error,
      el('div', { class: 'modal-actions' }, cancel, importBtn),
    );
  });
}
