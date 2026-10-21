// File > Import Actuals… (src/actuals.rs; the reading and matching are
// method_format::import::actuals): transactions from an accounting export.
//
// - Rust shows the Open dialog and reads the CSV, tab-delimited or Excel
//   file; the page names it by a token only.
// - The dialog, built like Import Spreadsheet's (sheetimport.js, whose
//   styles it reuses): the sheet, the header row, a column for each field
//   (guessed from the titles), the date order and signs, then the preview
//   Rust works out: the first transactions as they will import, the account
//   numbers this budget doesn't have (keep unassigned, skip, or map each to
//   an account), the duplicates (of transactions the budget has, and repeats
//   within the file) and the rows skipped. Nothing changes until Import,
//   which is one undo step and one import the Actuals tab can remove.
// - Mappings are kept per user in localStorage by the header row's titles,
//   like Import Spreadsheet's; the review of account numbers is per budget
//   and never kept.
//
// Text from the file goes in with textContent only.

import { el } from './dom.js';
import { customDialog } from './modal.js';
import { money } from './format.js';
import { readPref, writePref } from './prefs.js';
import { localStamp } from './versions.js';
import { t } from './i18n.js';

/** The fields' names in the dialog, by the key Rust uses. */
export const FIELD_LABELS = {
  date: t('actualsimport.field.date'),
  account: t('actualsimport.field.account'),
  amount: t('actualsimport.field.amount'),
  debit: t('actualsimport.field.debit'),
  credit: t('actualsimport.field.credit'),
  vendor: t('actualsimport.field.vendor'),
  description: t('actualsimport.field.description'),
  reference: t('actualsimport.field.reference'),
  tags: t('actualsimport.field.tags'),
};

const FIELD_TITLES = {
  account: t('actualsimport.field_tip.account'),
  amount: t('actualsimport.field_tip.amount'),
  debit: t('actualsimport.field_tip.debit'),
  credit: t('actualsimport.field_tip.credit'),
  reference: t('actualsimport.field_tip.reference'),
  tags: t('actualsimport.field_tip.tags'),
};

// ---------------------------------------------------------------------------
// Saved mappings (per user, in localStorage)

const PRESETS_PREF = 'actualsImport.presets';
export const MAX_PRESETS = 50;
const MAX_NAME = 80;

function isIndex(n) {
  return Number.isInteger(n) && n >= 0 && n < 100000;
}

/** A stored mapping, checked; null when it isn't one. */
export function cleanPreset(p) {
  if (!p || typeof p !== 'object') return null;
  const name = typeof p.name === 'string' ? p.name.trim().slice(0, MAX_NAME) : '';
  if (!name || typeof p.signature !== 'string') return null;
  const mapping = {};
  if (p.mapping && typeof p.mapping === 'object') {
    for (const [k, v] of Object.entries(p.mapping)) {
      if (Object.prototype.hasOwnProperty.call(FIELD_LABELS, k) && isIndex(v)) mapping[k] = v;
    }
  }
  return {
    name,
    signature: p.signature,
    header_row: isIndex(p.header_row) ? p.header_row : null,
    mapping,
    date_order: p.date_order === 'month_first' || p.date_order === 'day_first' ? p.date_order : '',
    negate: p.negate === true,
    carry_account: p.carry_account === true,
  };
}

export function loadPresets() {
  const raw = readPref(PRESETS_PREF, []);
  return Array.isArray(raw) ? raw.map(cleanPreset).filter(Boolean).slice(0, MAX_PRESETS) : [];
}

export function withPreset(list, preset) {
  const p = cleanPreset(preset);
  if (!p) return list;
  const key = p.name.toLowerCase();
  return [p, ...list.filter((q) => q.name.toLowerCase() !== key)].slice(0, MAX_PRESETS);
}

export function presetFor(list, signature) {
  if (!signature) return null;
  return list.find((p) => p.signature === signature) || null;
}

// ---------------------------------------------------------------------------
// The dialog

/** What Rust gets for the dialog's state (`extra`: detect_header, guess). */
export function requestFrom(st, extra = {}) {
  const codes = {};
  for (const [key, v] of Object.entries(st.codes)) {
    if (v === 'keep' || v === 'skip') codes[key] = { choice: v, account: '' };
    else if (typeof v === 'string' && v.startsWith('map:')) codes[key] = { choice: 'map', account: v.slice(4) };
  }
  return {
    sheet: st.sheet,
    header_row: st.headerRow,
    mapping: { ...st.mapping },
    detect_header: false,
    guess: false,
    date_order: st.dateOrder,
    negate: st.negate,
    carry_account: st.carryAccount,
    codes,
    skip_existing: st.skipExisting,
    skip_repeats: st.skipRepeats,
    ...extra,
  };
}

function columnLabel(c) {
  const title = c.title ? `: ${c.title}` : '';
  const sample = c.sample && c.sample !== c.title ? ` (${c.sample})` : '';
  return `${c.letter}${title}${sample}`;
}

/**
 * File > Import Actuals…: picks a file (Rust's dialog) and shows the
 * mapping dialog. `ctx`: { invoke, apply(view), toast, focus() }.
 */
export async function importActuals(ctx) {
  let picked;
  try {
    picked = await ctx.invoke('actuals_pick');
  } catch (e) {
    ctx.toast(e);
    ctx.focus();
    return;
  }
  if (!picked) {
    ctx.focus();
    return;
  }
  showDialog(ctx, picked);
}

/** The dialog for a picked file (exported for the browser harness). */
export function showDialog(ctx, picked) {
  const st = {
    token: picked.token,
    sheet: picked.sheet || 0,
    headerRow: null,
    mapping: {},
    dateOrder: '',
    negate: false,
    carryAccount: false,
    codes: {}, // unmatched key -> 'keep' | 'skip' | 'map:<code>'
    skipExisting: true,
    skipRepeats: false,
    preview: null,
    presetName: '',
    presets: loadPresets(),
  };
  const fields = Array.isArray(picked.fields) && picked.fields.length ? picked.fields : Object.keys(FIELD_LABELS);
  let seq = 0;
  let timer = null;
  let busy = false;
  const ui = {};

  const setError = (msg) => {
    ui.error.textContent = msg ? String(msg) : '';
    ui.error.hidden = !msg;
  };

  const updateButton = () => {
    const p = st.preview;
    ui.ok.disabled = busy || !p || p.count === 0;
    ui.ok.textContent = p && p.count ? t('actualsimport.import_n', { count: p.count }) : t('actualsimport.import');
  };

  const refresh = async (extra = {}) => {
    const mine = ++seq;
    busy = true;
    updateButton();
    try {
      const p = await ctx.invoke('actuals_preview', { token: st.token, request: requestFrom(st, extra) });
      if (mine !== seq) return null;
      st.preview = p;
      st.headerRow = p.header_row === undefined ? null : p.header_row;
      st.mapping = { ...p.mapping };
      setError('');
      render();
      return p;
    } catch (e) {
      if (mine !== seq) return null;
      st.preview = null;
      setError(e);
      render();
      return null;
    } finally {
      if (mine === seq) {
        busy = false;
        updateButton();
      }
    }
  };

  const schedule = () => {
    clearTimeout(timer);
    timer = setTimeout(() => refresh(), 120);
  };

  const usePreset = (p) => {
    const width = st.preview ? st.preview.columns.length : 0;
    st.headerRow = p.header_row;
    st.mapping = {};
    for (const [k, v] of Object.entries(p.mapping)) if (v < width) st.mapping[k] = v;
    st.dateOrder = p.date_order;
    st.negate = p.negate;
    st.carryAccount = p.carry_account;
    st.presetName = p.name;
  };

  const startSheet = async () => {
    st.presetName = '';
    st.codes = {};
    const p = await refresh({ detect_header: true, guess: true });
    if (!p) return;
    const preset = presetFor(st.presets, p.signature);
    if (preset) {
      usePreset(preset);
      await refresh();
    }
  };

  // --- the parts that change with each preview ---

  const fieldSelect = (key) => {
    const select = el('select', {
      class: 'modal-input si-col',
      attrs: { 'aria-label': t('actualsimport.column_for', { field: FIELD_LABELS[key] || key }) },
      data: { siKey: `field-${key}` },
    });
    select.append(el('option', { text: t('actualsimport.not_imported'), attrs: { value: '' } }));
    for (const c of st.preview ? st.preview.columns : []) {
      select.append(el('option', { text: columnLabel(c), attrs: { value: String(c.index) } }));
    }
    select.value = st.mapping[key] === undefined ? '' : String(st.mapping[key]);
    select.addEventListener('change', () => {
      if (select.value === '') delete st.mapping[key];
      else st.mapping[key] = Number(select.value);
      st.presetName = '';
      schedule();
    });
    return select;
  };

  const renderTop = () => {
    const p = st.preview;
    const parts = [];
    if (picked.sheets.length > 1) {
      const sheet = el('select', { class: 'modal-input', attrs: { id: 'ai-sheet' }, data: { siKey: 'sheet' } });
      picked.sheets.forEach((s, i) => {
        sheet.append(el('option', { text: s.hidden ? t('actualsimport.sheet_option_hidden', { name: s.name, count: s.rows }) : t('actualsimport.sheet_option', { name: s.name, count: s.rows }), attrs: { value: String(i) } }));
      });
      sheet.value = String(st.sheet);
      sheet.addEventListener('change', () => {
        st.sheet = Number(sheet.value);
        startSheet();
      });
      parts.push(el('label', { class: 'modal-field si-field' }, el('span', { text: t('actualsimport.sheet') }), sheet));
    }
    const header = el('select', { class: 'modal-input', attrs: { id: 'ai-header' }, data: { siKey: 'header' } });
    header.append(el('option', { text: t('actualsimport.no_header_row'), attrs: { value: '' } }));
    for (const r of p ? p.raw : []) {
      const cells = r.cells.filter((c) => c.trim() !== '').slice(0, 5).join(' | ');
      header.append(el('option', { text: t('actualsimport.header_option', { row: r.row, cells: cells || t('actualsimport.empty_row') }), attrs: { value: String(r.row - 1) } }));
    }
    header.value = st.headerRow === null ? '' : String(st.headerRow);
    header.addEventListener('change', () => {
      st.headerRow = header.value === '' ? null : Number(header.value);
      st.presetName = '';
      refresh({ guess: true });
    });
    parts.push(el('label', { class: 'modal-field si-field' }, el('span', { text: t('actualsimport.header_row') }), header));
    ui.top.replaceChildren(...parts);
    const info = p && picked.sheets[st.sheet];
    ui.truncated.hidden = !(info && info.truncated);
  };

  const buildPresets = () => {
    const select = el('select', { class: 'modal-input', attrs: { 'aria-label': t('actualsimport.saved_mappings') }, data: { siKey: 'preset' } });
    select.addEventListener('change', () => {
      const p = st.presets.find((q) => q.name === select.value);
      if (!p) return;
      usePreset(p);
      name.value = p.name;
      refresh();
    });
    const name = el('input', {
      class: 'modal-input si-preset-name',
      attrs: { type: 'text', placeholder: t('actualsimport.mapping_name'), 'aria-label': t('actualsimport.mapping_name'), maxlength: String(MAX_NAME), spellcheck: 'false', autocomplete: 'off' },
    });
    const save = el('button', { class: 'btn', text: t('actualsimport.save_mapping'), attrs: { type: 'button' } });
    save.addEventListener('click', () => {
      const n = name.value.trim();
      if (!n || !st.preview) {
        name.focus();
        return;
      }
      st.presets = withPreset(st.presets, {
        name: n,
        signature: st.preview.signature,
        header_row: st.headerRow,
        mapping: st.mapping,
        date_order: st.dateOrder,
        negate: st.negate,
        carry_account: st.carryAccount,
      });
      writePref(PRESETS_PREF, st.presets);
      st.presetName = n;
      renderPresets();
      ui.presetNote.textContent = t('actualsimport.saved_note', { name: n });
    });
    ui.presetControls = { select, name };
    ui.presets.replaceChildren(select, name, save);
  };

  const renderPresets = () => {
    const { select, name } = ui.presetControls;
    select.replaceChildren(el('option', { text: st.presets.length ? t('actualsimport.saved_mappings_pick') : t('actualsimport.no_saved_mappings'), attrs: { value: '' } }));
    for (const p of st.presets) select.append(el('option', { text: p.name, attrs: { value: p.name } }));
    select.value = st.presetName && st.presets.some((p) => p.name === st.presetName) ? st.presetName : '';
    select.disabled = !st.presets.length;
    if (st.presetName && !name.value) name.value = st.presetName;
    ui.presetNote.textContent = st.presetName ? t('actualsimport.using_mapping', { name: st.presetName }) : '';
  };

  const renderMapping = () => {
    ui.mapping.replaceChildren(
      ...fields.map((key) =>
        el('label', { class: 'si-map-row', title: FIELD_TITLES[key] || '' }, el('span', { class: 'si-map-label', text: FIELD_LABELS[key] || key }), fieldSelect(key)),
      ),
    );
  };

  const renderSummary = () => {
    const p = st.preview;
    if (!p) {
      ui.summary.replaceChildren();
      return;
    }
    const unassigned = p.unassigned || 0;
    const what = { transactions: t('actualsimport.n_transactions', { count: p.count }), total: money(p.total), accounts: t('actualsimport.n_accounts', { count: p.accounts }) };
    const sentences = [unassigned ? t('actualsimport.summary_unassigned', { ...what, unassigned }) : t('actualsimport.summary', what)];
    const skipped = p.skipped_count + p.duplicates_skipped;
    if (skipped) sentences.push(t('actualsimport.rows_skipped', { count: skipped }));
    if (p.headings) sentences.push(t('actualsimport.heading_rows', { count: p.headings }));
    const line = sentences.join(' ');
    const parts = [el('p', { class: 'si-summary-main', text: line })];
    parts.push(el('p', { class: 'modal-message', text: t('actualsimport.actual_to_date', { before: money(p.actual_before), after: money(p.actual_after) }) }));
    if (p.date_order_found) parts.push(el('p', { class: 'si-more', text: p.date_order === 'day_first' ? t('actualsimport.dates_read_day_first') : t('actualsimport.dates_read_month_first') }));
    ui.summary.replaceChildren(...parts);
  };

  const renderRows = () => {
    const p = st.preview;
    if (!p || !p.transactions.length) {
      ui.rows.replaceChildren(el('p', { class: 'modal-message', text: p ? t('actualsimport.no_transactions') : '' }));
      return;
    }
    const cols = [
      ['row', t('actualsimport.col.row'), 'num'],
      ['date', t('actualsimport.col.date'), ''],
      ['account', t('actualsimport.col.account'), ''],
      ['vendor', t('actualsimport.col.vendor'), 'si-desc'],
      ['description', t('actualsimport.col.description'), 'si-desc'],
      ['reference', t('actualsimport.col.reference'), ''],
      ['amount', t('actualsimport.col.amount'), 'num'],
    ];
    const head = el('tr', {}, ...cols.map(([, label, cls]) => el('th', { class: cls, text: label })));
    const body = p.transactions.map((tx) =>
      el(
        'tr',
        {},
        ...cols.map(([key, , cls]) => {
          let text;
          if (key === 'amount') text = money(tx.amount);
          else if (key === 'account') text = tx.account ? (tx.assigned ? `${tx.account} ${tx.account_name}`.trim() : t('actualsimport.account_unassigned', { account: tx.account })) : t('actualsimport.no_account_cell');
          else text = tx[key] === null || tx[key] === undefined ? '' : String(tx[key]);
          return el('td', { class: cls, text, title: cls === 'si-desc' ? text : '' });
        }),
      ),
    );
    const table = el('table', { class: 'si-table' }, el('thead', {}, head), el('tbody', {}, ...body));
    const more = p.count > p.transactions.length ? el('p', { class: 'si-more', text: t('actualsimport.first_of', { shown: p.transactions.length, count: p.count }) }) : null;
    ui.rows.replaceChildren(...[el('div', { class: 'si-table-wrap' }, table), more].filter(Boolean));
  };

  /** The review of account numbers this budget doesn't have. */
  const renderCodes = () => {
    const p = st.preview;
    if (!p || !p.unmatched.length) {
      ui.codes.replaceChildren();
      ui.codes.hidden = true;
      return;
    }
    ui.codes.hidden = false;
    const head = el('tr', {}, el('th', { text: t('actualsimport.codes.account') }), el('th', { class: 'num', text: t('actualsimport.codes.rows') }), el('th', { class: 'num', text: t('actualsimport.codes.amount') }), el('th', { text: t('actualsimport.codes.import_as') }));
    const body = p.unmatched.map((u) => {
      const select = el('select', {
        class: 'modal-input si-col',
        attrs: { 'aria-label': u.key ? t('actualsimport.codes.import_account_as', { account: u.key }) : t('actualsimport.codes.import_none_as') },
        data: { siKey: `code-${u.key}` },
      });
      select.append(el('option', { text: t('actualsimport.codes.keep'), attrs: { value: 'keep' } }));
      select.append(el('option', { text: t('actualsimport.codes.skip'), attrs: { value: 'skip' } }));
      for (const c of p.chart) select.append(el('option', { text: `${c.code} ${c.name}`.trim(), attrs: { value: `map:${c.code}` } }));
      select.value = u.choice === 'map' ? `map:${u.account}` : u.choice;
      if (select.value === '') select.value = 'keep';
      select.addEventListener('change', () => {
        st.codes[u.key] = select.value;
        schedule();
      });
      const label = u.key ? (u.text && u.text !== u.key ? `${u.text} (${u.key})` : u.key) : t('actualsimport.codes.no_account');
      return el('tr', {}, el('td', { text: label }), el('td', { class: 'num', text: String(u.rows) }), el('td', { class: 'num', text: money(u.amount) }), el('td', {}, select));
    });
    ui.codes.replaceChildren(
      el('h3', { class: 'si-heading', text: t('actualsimport.codes.heading') }),
      el('p', { class: 'si-more', text: t('actualsimport.codes.intro') }),
      el('div', { class: 'si-table-wrap' }, el('table', { class: 'si-table' }, el('thead', {}, head), el('tbody', {}, ...body))),
    );
  };

  /** `what`: the heading, with the count in it. */
  const noteList = (items, count, what, open = false) => {
    if (!count) return null;
    const list = el('ul', { class: 'si-notes' }, ...items.map((n) => el('li', { text: t('actualsimport.row_note', { row: n.row, message: n.message }) })));
    if (count > items.length) list.append(el('li', { class: 'si-more', text: t('actualsimport.and_more', { count: count - items.length }) }));
    const d = el('details', { class: 'si-details' }, el('summary', { text: what }), list);
    if (open) d.open = true;
    return d;
  };

  const renderDetails = () => {
    const p = st.preview;
    if (!p) {
      ui.details.replaceChildren();
      return;
    }
    const dupMessage = (d) => {
      const details = [d.date, d.account, money(d.amount), d.description].filter(Boolean).join(', ');
      if (d.repeats === null) return d.skipped ? t('actualsimport.dup.existing_skipped', { details }) : t('actualsimport.dup.existing_imported', { details });
      return d.skipped ? t('actualsimport.dup.repeat_skipped', { row: d.repeats, details }) : t('actualsimport.dup.repeat_imported', { row: d.repeats, details });
    };
    const dups = p.duplicates.map((d) => ({ row: d.row, message: dupMessage(d) }));
    ui.details.replaceChildren(
      ...[
        noteList(dups, p.duplicate_count, t('actualsimport.possible_duplicates', { count: p.duplicate_count }), p.duplicate_count > 0 && p.duplicates_skipped < p.duplicate_count),
        noteList(p.skipped, p.skipped_count, t('actualsimport.skipped_rows', { count: p.skipped_count })),
        noteList(p.warnings, p.warning_count, t('actualsimport.rows_left_out', { count: p.warning_count })),
      ].filter(Boolean),
    );
  };

  function render() {
    const active = document.activeElement;
    const focusKey = active && active.dataset ? active.dataset.siKey : '';
    renderTop();
    renderPresets();
    renderMapping();
    renderSummary();
    renderRows();
    renderCodes();
    renderDetails();
    updateButton();
    if (ui.syncOptions) ui.syncOptions();
    if (focusKey && ui.root) {
      const again = [...ui.root.querySelectorAll('[data-si-key]')].find((e) => e.dataset.siKey === focusKey);
      if (again && again !== document.activeElement) again.focus({ preventScroll: true });
    }
  }

  customDialog(
    (closeDialog) => {
      const title = el('h2', { class: 'modal-title', text: t('actualsimport.title', { file: picked.file }), attrs: { id: 'ai-title' } });
      const intro = el('p', {
        class: 'modal-message',
        text: t('actualsimport.intro'),
      });
      ui.top = el('div', { class: 'si-top' });
      ui.truncated = el('p', { class: 'si-note', text: t('actualsimport.truncated') });
      ui.truncated.hidden = true;
      ui.presets = el('div', { class: 'si-presets' });
      ui.presetNote = el('p', { class: 'si-more', attrs: { 'aria-live': 'polite' } });
      buildPresets();
      ui.mapping = el('div', { class: 'si-map' });

      const box = (id, text, get, set) => {
        const b = el('input', { attrs: { type: 'checkbox', id } });
        b.checked = get();
        b.addEventListener('change', () => {
          set(b.checked);
          schedule();
        });
        return { b, row: el('label', { class: 'rs-check', attrs: { for: id } }, b, el('span', { text })) };
      };
      const order = el('select', { class: 'modal-input si-col', attrs: { id: 'ai-date-order' } });
      order.append(el('option', { text: t('actualsimport.date_order.auto'), attrs: { value: '' } }));
      order.append(el('option', { text: t('actualsimport.date_order.month_first'), attrs: { value: 'month_first' } }));
      order.append(el('option', { text: t('actualsimport.date_order.day_first'), attrs: { value: 'day_first' } }));
      order.value = st.dateOrder;
      order.addEventListener('change', () => {
        st.dateOrder = order.value;
        schedule();
      });
      const negate = box('ai-negate', t('actualsimport.option.negate'), () => st.negate, (v) => (st.negate = v));
      const carry = box('ai-carry', t('actualsimport.option.carry'), () => st.carryAccount, (v) => (st.carryAccount = v));
      const existing = box('ai-existing', t('actualsimport.option.skip_existing'), () => st.skipExisting, (v) => (st.skipExisting = v));
      const repeats = box('ai-repeats', t('actualsimport.option.skip_repeats'), () => st.skipRepeats, (v) => (st.skipRepeats = v));
      ui.syncOptions = () => {
        order.value = st.dateOrder;
        negate.b.checked = st.negate;
        carry.b.checked = st.carryAccount;
      };
      const options = el(
        'div',
        { class: 'si-options' },
        el('label', { class: 'si-inline', attrs: { for: 'ai-date-order' } }, el('span', { text: t('actualsimport.dates') }), order),
        negate.row,
        carry.row,
        existing.row,
        repeats.row,
      );

      ui.summary = el('div', { class: 'si-summary', attrs: { 'aria-live': 'polite' } });
      ui.rows = el('div', { class: 'si-lines' });
      ui.codes = el('div', { class: 'si-recon' });
      ui.codes.hidden = true;
      ui.details = el('div', { class: 'si-detail-list' });
      ui.error = el('p', { class: 'modal-error', attrs: { role: 'alert' } });
      ui.error.hidden = true;

      const cancel = el('button', { class: 'btn', text: t('modal.cancel'), attrs: { type: 'button', 'data-autofocus': true } });
      ui.ok = el('button', { class: 'btn btn-primary', text: t('actualsimport.import'), attrs: { type: 'button' } });
      ui.ok.disabled = true;
      cancel.addEventListener('click', () => closeDialog());
      ui.ok.addEventListener('click', async () => {
        if (ui.ok.disabled) return;
        clearTimeout(timer);
        busy = true;
        updateButton();
        try {
          const r = await ctx.invoke('actuals_import', { token: st.token, request: requestFrom(st), imported: localStamp() });
          closeDialog();
          if (r && r.view) ctx.apply(r.view);
          if (r && r.message) ctx.toast(r.message);
        } catch (e) {
          setError(e);
          busy = false;
          updateButton();
        }
      });

      const scroll = el(
        'div',
        { class: 'si-scroll' },
        ui.top,
        ui.truncated,
        el('h3', { class: 'si-heading', text: t('actualsimport.columns') }),
        ui.presets,
        ui.presetNote,
        ui.mapping,
        el('h3', { class: 'si-heading', text: t('actualsimport.options') }),
        options,
        el('h3', { class: 'si-heading', text: t('actualsimport.preview') }),
        ui.summary,
        ui.rows,
        ui.codes,
        ui.details,
      );
      ui.root = el(
        'div',
        { class: 'modal si-dialog', attrs: { role: 'dialog', 'aria-modal': 'true', 'aria-labelledby': 'ai-title' } },
        title,
        intro,
        scroll,
        ui.error,
        el('div', { class: 'modal-actions' }, cancel, ui.ok),
      );
      return ui.root;
    },
    {
      onClose: () => {
        clearTimeout(timer);
        seq++;
        ctx.focus();
      },
    },
  );

  startSheet();
}
