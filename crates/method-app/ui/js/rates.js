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
import { obj, str, errText } from './util.js';
import { t } from './i18n.js';

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
    title: t('rates.leave_without_saving'),
    message: t('rates.leave_message', { name: state.draft && state.draft.name ? state.draft.name : t('rates.this_table') }),
    confirmText: t('rates.leave_without_saving_button'),
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
  const names = new Set(tables(state.view).map((x) => x.name));
  let name = t('rates.new_table_name');
  for (let n = 2; names.has(name); n += 1) name = t('rates.new_table_name_n', { n });
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
    title: t('rates.delete_the_rate_table', { selected: state.selected }),
    message: t('rates.lines_filled_from_it_keep'),
    confirmText: t('rates.delete_table'),
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
    state.ctx.toast(t('rates.the_lines_filled_from_rate'));
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
  const list = el('ul', { class: 'rates-rail-list', attrs: { 'aria-label': t('rates.rate_tables') } });
  for (const tb of tables(state.view)) {
    const on = tb.name === state.selected;
    const rows = Array.isArray(tb.rows) ? tb.rows.length : 0;
    const b = el('button', {
      class: `rates-rail-item${on ? ' is-selected' : ''}`,
      text: tb.name,
      title: `${tb.name}: ${t('rates.count_rows', { count: rows })}`,
      attrs: { type: 'button', 'aria-pressed': String(on) },
      data: { key: `rates:table:${tb.name}`, nobusy: '1' },
    });
    b.addEventListener('click', () => pick(tb.name));
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
    attrs: { 'aria-label': t('rates.add_a_unit_column') },
    data: { key: 'rates:add-unit' },
  });
  s.append(el('option', { text: t('rates.add_unit'), attrs: { value: '' } }));
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
  const heads = [head(t('rates.col.classification')), ...state.units.map((u) => head(t('rates.per_unit', { unit: u }), 'sw-num')), head(t('rates.col.from')), head(t('rates.col.to')), head(t('rates.col.notes')), head('')];
  const list = el('ul', { attrs: { 'aria-label': t('rates.rows_of', { name: d.name }) } });
  const needle = state.filter.trim().toLowerCase();
  d.rows.forEach((r, i) => {
    if (needle && !r.classification.toLowerCase().includes(needle)) return;
    const cells = [input(r.classification, `rates:row:${i}:class`, t('rates.aria.classification', { row: i + 1 }), (v) => (r.classification = v))];
    for (const u of state.units) {
      cells.push(
        input(str(r.rates[u]), `rates:row:${i}:${u}`, t('rates.aria.rate', { row: i + 1, unit: u }), (v) => {
          if (v.trim()) r.rates[u] = v.trim();
          else delete r.rates[u];
        }, 'sw-num'),
      );
    }
    cells.push(input(r.from, `rates:row:${i}:from`, t('rates.aria.from', { row: i + 1 }), (v) => (r.from = v)));
    cells.push(input(r.to, `rates:row:${i}:to`, t('rates.aria.to', { row: i + 1 }), (v) => (r.to = v)));
    cells.push(input(r.notes, `rates:row:${i}:notes`, t('rates.aria.notes', { row: i + 1 }), (v) => (r.notes = v)));
    const del = el('button', {
      class: 'setup-btn sw-acts',
      text: '×',
      title: t('rates.remove_this_row'),
      attrs: { type: 'button', 'aria-label': t('rates.aria.remove', { row: i + 1 }) },
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
    stripButton(t('rates.new_table'), 'rates:new', () => newTable(), { title: t('rates.start_a_rate_table_of') }),
    stripButton(t('rates.import_sheet'), 'rates:import', () => importDialog(), {
      title: t('rates.read_a_rate_table_from'),
    }),
  ];
  if (d) {
    const saveBtn = stripButton(t('rates.save_table'), 'rates:save', () => save(), { primary: true, title: t('rates.keep_these_changes_one_undo') });
    saveBtn.dataset.dirty = '1';
    saveBtn.setAttribute('aria-disabled', state.dirty ? 'false' : 'true');
    const revertBtn = stripButton(t('rates.revert'), 'rates:revert', () => revert(), { title: t('rates.drop_the_changes_not_saved') });
    revertBtn.dataset.dirty = '1';
    revertBtn.setAttribute('aria-disabled', state.dirty ? 'false' : 'true');
    actions.push(saveBtn, revertBtn, stripButton(t('rates.delete_table_strip'), 'rates:delete', () => remove(), { title: t('rates.delete_this_rate_table') }));
  }
  actions.push(
    stripButton(t('rates.update_lines'), 'rates:update', () => updateLines(), {
      title: t('rates.give_every_line_filled_from'),
    }),
  );
  const search = d
    ? searchBox({
        key: 'rates:search',
        value: state.filter,
        label: t('rates.search_classifications'),
        placeholder: t('rates.search_classifications'),
        onChange: (v) => {
          state.filter = v;
          redraw();
        },
      })
    : null;
  const strip = actionStrip(t('setup.sheet_kind.rate_table'), actions, search);
  if (!d) {
    mountFrame(panel, {
      strip,
      body: [
        el('p', {
          class: 'setup-empty',
          text: t('rates.no_rate_tables_yet_a'),
        }),
      ],
    });
    rail();
    setFooterTotals([{ label: '', value: t('rates.count_tables', { count: 0 }) }]);
    return;
  }
  const name = input(d.name, 'rates:name', t('rates.table_name'), (v) => (d.name = v));
  const notes = input(d.notes, 'rates:notes', t('rates.table_notes'), (v) => (d.notes = v));
  const adder = unitAdder();
  const bandEl = band(t('rates.rate_table'), bandField(t('rates.name'), name), bandField(t('rates.col.notes'), notes, t('rates.notes_hint')), adder ? bandField(t('rates.units'), adder) : null);
  const add = el('button', {
    class: 'setup-btn sw-after',
    text: t('rates.add_classification'),
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
    { label: '', value: t('rates.count_classifications', { count: classes.size }) },
    { label: '', value: t('rates.count_rows', { count: d.rows.length }) },
    state.dirty ? { label: '', value: t('rates.not_saved'), dim: true } : null,
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
  ['classification', t('rates.col.classification')],
  ['from', t('rates.col.from')],
  ['to', t('rates.col.to')],
  ['notes', t('rates.col.notes')],
  ['unit', t('rates.col.unit_column')],
  ['rate', t('rates.col.rate_column')],
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
  const importBtn = el('button', { class: 'btn btn-primary', text: t('rates.import'), attrs: { type: 'button' } });

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
    s.append(el('option', { text: t('rates.none'), attrs: { value: '' } }));
    current.titles.forEach((t, i) => s.append(el('option', { text: t, attrs: { value: String(i) } })));
    s.value = value === null || value === undefined ? '' : String(value);
    s.addEventListener('change', () => onChange(s.value === '' ? null : Number(s.value)));
    return s;
  };

  const fill = () => {
    error.hidden = true;
    const m = { ...current.mapping, units: { ...obj(current.mapping.units) } };
    const rows = [];
    const sheet = el('select', { class: 'modal-input', attrs: { 'aria-label': t('rates.sheet') } });
    for (const s of current.sheets) sheet.append(el('option', { text: t('setup.sheet_rows', { name: s.name, count: s.rows }), attrs: { value: String(s.index) } }));
    sheet.value = String(current.index);
    sheet.addEventListener('change', () => reread(null, Number(sheet.value)));
    if (current.sheets.length > 1) rows.push(el('label', { class: 'modal-field' }, el('span', { text: t('rates.sheet') }), sheet));
    const nameIn = el('input', { class: 'modal-input', attrs: { type: 'text', 'aria-label': t('rates.table_name') } });
    nameIn.value = name;
    nameIn.addEventListener('change', () => {
      name = nameIn.value;
      reread(m);
    });
    rows.push(el('label', { class: 'modal-field' }, el('span', { text: t('rates.table_name') }), nameIn));
    const header = el('input', { class: 'modal-input', attrs: { type: 'number', min: '0', 'aria-label': t('rates.title_row') } });
    header.value = m.header_row === null || m.header_row === undefined ? '0' : String(m.header_row + 1);
    header.addEventListener('change', () => {
      const n = Number(header.value);
      reread({ ...m, header_row: n > 0 ? n - 1 : null });
    });
    rows.push(el('label', { class: 'modal-field' }, el('span', { text: t('rates.titles_on_row') }), header));
    for (const [k, label] of FIELDS) {
      rows.push(el('label', { class: 'modal-field' }, el('span', { text: label }), colSelect(label, m[k], (v) => reread({ ...m, [k]: v }))));
    }
    for (const u of current.units) {
      rows.push(
        el(
          'label',
          { class: 'modal-field' },
          el('span', { text: t('rates.rate_per', { u }) }),
          colSelect(t('rates.rate_per', { u }), m.units[u], (v) => {
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
      const ul = el('ul', { class: 'rates-import-errors', attrs: { 'aria-label': t('rates.problems') } });
      for (const e of current.errors.slice(0, 50)) ul.append(el('li', { text: t('rates.row', { row: e.row, message: e.message }) }));
      out.push(el('p', { class: 'modal-message', text: t('rates.nothing_is_imported_while_there') }), ul);
    } else if (current.table) {
      const tb = current.table;
      const units = unitsOf(tb);
      const ul = el('ul', { class: 'rates-import-preview', attrs: { 'aria-label': t('rates.what_the_table_will_hold') } });
      for (const r of tb.rows.slice(0, 40)) {
        const figures = units.filter((u) => r.rates[u] !== undefined).map((u) => t('rates.figure_per', { rate: r.rates[u], unit: u }));
        const when = [r.from && t('rates.when_from', { date: r.from }), r.to && t('rates.when_to', { date: r.to })].filter(Boolean).join(' ');
        ul.append(el('li', { text: `${r.classification}: ${figures.join(', ')}${when ? ` (${when})` : ''}` }));
      }
      out.push(el('p', { class: 'modal-message', text: current.exists ? t('rates.rows_in_replaces', { count: tb.rows.length, name: tb.name }) : t('rates.rows_in', { count: tb.rows.length, name: tb.name }) }), ul);
    }
    importBtn.textContent = current.exists ? t('rates.replace_table') : t('rates.import');
    importBtn.disabled = !current.table;
    body.replaceChildren(el('div', { class: 'rates-import-map' }, ...rows), ...out);
  };

  customDialog((close) => {
    const cancel = el('button', { class: 'btn', text: t('rates.cancel'), attrs: { type: 'button' } });
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
      el('h2', { class: 'modal-title', text: t('rates.import_a_rate_table_from', { file: current.file }), attrs: { id: 'rates-import-title' } }),
      el('p', { class: 'modal-message', text: t('rates.map_the_sheet_s_columns') }),
      body,
      error,
      el('div', { class: 'modal-actions' }, cancel, importBtn),
    );
  });
}
