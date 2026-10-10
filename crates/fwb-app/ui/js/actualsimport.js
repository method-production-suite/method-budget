// File > Import Actuals… (src/actuals.rs; the reading and matching are
// fwb_format::import::actuals): transactions from an accounting export.
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
import { plural } from './util.js';

/** The fields' names in the dialog, by the key Rust uses. */
export const FIELD_LABELS = {
  date: 'Date',
  account: 'Account number',
  amount: 'Amount',
  debit: 'Debit',
  credit: 'Credit',
  vendor: 'Vendor',
  description: 'Description',
  reference: 'Reference',
  tags: 'Tags',
};

const FIELD_TITLES = {
  account: 'Read with this budget’s account numbering. Numbers the budget doesn’t have are listed below for review.',
  amount: 'Used when the row has one; otherwise Debit minus Credit',
  debit: 'A cost (with Credit, when there is no Amount column)',
  credit: 'A refund or credit, taken off',
  reference: 'Invoice, purchase order or check number',
  tags: 'Labels, separated by commas',
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
    ui.ok.textContent = p && p.count ? `Import ${plural(p.count, 'Transaction', 'Transactions')}` : 'Import';
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
      attrs: { 'aria-label': `Column for ${FIELD_LABELS[key] || key}` },
      data: { siKey: `field-${key}` },
    });
    select.append(el('option', { text: '(not imported)', attrs: { value: '' } }));
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
        sheet.append(el('option', { text: `${s.name}${s.hidden ? ' (hidden)' : ''}, ${plural(s.rows, 'row', 'rows')}`, attrs: { value: String(i) } }));
      });
      sheet.value = String(st.sheet);
      sheet.addEventListener('change', () => {
        st.sheet = Number(sheet.value);
        startSheet();
      });
      parts.push(el('label', { class: 'modal-field si-field' }, el('span', { text: 'Sheet' }), sheet));
    }
    const header = el('select', { class: 'modal-input', attrs: { id: 'ai-header' }, data: { siKey: 'header' } });
    header.append(el('option', { text: 'No header row: data from row 1', attrs: { value: '' } }));
    for (const r of p ? p.raw : []) {
      const cells = r.cells.filter((c) => c.trim() !== '').slice(0, 5).join(' | ');
      header.append(el('option', { text: `Row ${r.row}: ${cells || '(empty)'}`, attrs: { value: String(r.row - 1) } }));
    }
    header.value = st.headerRow === null ? '' : String(st.headerRow);
    header.addEventListener('change', () => {
      st.headerRow = header.value === '' ? null : Number(header.value);
      st.presetName = '';
      refresh({ guess: true });
    });
    parts.push(el('label', { class: 'modal-field si-field' }, el('span', { text: 'Header row' }), header));
    ui.top.replaceChildren(...parts);
    const info = p && picked.sheets[st.sheet];
    ui.truncated.hidden = !(info && info.truncated);
  };

  const buildPresets = () => {
    const select = el('select', { class: 'modal-input', attrs: { 'aria-label': 'Saved mappings' }, data: { siKey: 'preset' } });
    select.addEventListener('change', () => {
      const p = st.presets.find((q) => q.name === select.value);
      if (!p) return;
      usePreset(p);
      name.value = p.name;
      refresh();
    });
    const name = el('input', {
      class: 'modal-input si-preset-name',
      attrs: { type: 'text', placeholder: 'Name for this mapping', 'aria-label': 'Name for this mapping', maxlength: String(MAX_NAME), spellcheck: 'false', autocomplete: 'off' },
    });
    const save = el('button', { class: 'btn', text: 'Save Mapping', attrs: { type: 'button' } });
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
      ui.presetNote.textContent = `Saved. A file with these column titles will start with "${n}".`;
    });
    ui.presetControls = { select, name };
    ui.presets.replaceChildren(select, name, save);
  };

  const renderPresets = () => {
    const { select, name } = ui.presetControls;
    select.replaceChildren(el('option', { text: st.presets.length ? 'Saved mappings…' : 'No saved mappings', attrs: { value: '' } }));
    for (const p of st.presets) select.append(el('option', { text: p.name, attrs: { value: p.name } }));
    select.value = st.presetName && st.presets.some((p) => p.name === st.presetName) ? st.presetName : '';
    select.disabled = !st.presets.length;
    if (st.presetName && !name.value) name.value = st.presetName;
    ui.presetNote.textContent = st.presetName ? `Using the saved mapping "${st.presetName}".` : '';
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
    let line = `${plural(p.count, 'transaction', 'transactions')} totaling ${money(p.total)} into ${plural(p.accounts, 'account', 'accounts')}`;
    if (unassigned) line += `, ${unassigned} unassigned`;
    line += '.';
    const skipped = p.skipped_count + p.duplicates_skipped;
    if (skipped) line += ` ${plural(skipped, 'row', 'rows')} skipped.`;
    if (p.headings) line += ` ${plural(p.headings, 'heading row', 'heading rows')} only set the account below.`;
    const parts = [el('p', { class: 'si-summary-main', text: line })];
    parts.push(el('p', { class: 'modal-message', text: `Actual to date ${money(p.actual_before)} now, ${money(p.actual_after)} after the import.` }));
    if (p.date_order_found) parts.push(el('p', { class: 'si-more', text: `Dates read ${p.date_order === 'day_first' ? 'day first (30/9/2026)' : 'month first (9/30/2026)'}, as the file's dates show.` }));
    ui.summary.replaceChildren(...parts);
  };

  const renderRows = () => {
    const p = st.preview;
    if (!p || !p.transactions.length) {
      ui.rows.replaceChildren(el('p', { class: 'modal-message', text: p ? 'No transactions will be imported with these choices.' : '' }));
      return;
    }
    const cols = [
      ['row', 'Row', 'num'],
      ['date', 'Date', ''],
      ['account', 'Account', ''],
      ['vendor', 'Vendor', 'si-desc'],
      ['description', 'Description', 'si-desc'],
      ['reference', 'Reference', ''],
      ['amount', 'Amount', 'num'],
    ];
    const head = el('tr', {}, ...cols.map(([, label, cls]) => el('th', { class: cls, text: label })));
    const body = p.transactions.map((t) =>
      el(
        'tr',
        {},
        ...cols.map(([key, , cls]) => {
          let text;
          if (key === 'amount') text = money(t.amount);
          else if (key === 'account') text = t.account ? (t.assigned ? `${t.account} ${t.account_name}`.trim() : `${t.account} (unassigned)`) : '(none)';
          else text = t[key] === null || t[key] === undefined ? '' : String(t[key]);
          return el('td', { class: cls, text, title: cls === 'si-desc' ? text : '' });
        }),
      ),
    );
    const table = el('table', { class: 'si-table' }, el('thead', {}, head), el('tbody', {}, ...body));
    const more = p.count > p.transactions.length ? el('p', { class: 'si-more', text: `The first ${p.transactions.length} of ${p.count} transactions.` }) : null;
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
    const head = el('tr', {}, el('th', { text: 'Account in the file' }), el('th', { class: 'num', text: 'Rows' }), el('th', { class: 'num', text: 'Amount' }), el('th', { text: 'Import as' }));
    const body = p.unmatched.map((u) => {
      const select = el('select', {
        class: 'modal-input si-col',
        attrs: { 'aria-label': `Import account ${u.key || 'none'} as` },
        data: { siKey: `code-${u.key}` },
      });
      select.append(el('option', { text: 'Unassigned (keep the number)', attrs: { value: 'keep' } }));
      select.append(el('option', { text: 'Skip these rows', attrs: { value: 'skip' } }));
      for (const c of p.chart) select.append(el('option', { text: `${c.code} ${c.name}`.trim(), attrs: { value: `map:${c.code}` } }));
      select.value = u.choice === 'map' ? `map:${u.account}` : u.choice;
      if (select.value === '') select.value = 'keep';
      select.addEventListener('change', () => {
        st.codes[u.key] = select.value;
        schedule();
      });
      const label = u.key ? (u.text && u.text !== u.key ? `${u.text} (${u.key})` : u.key) : '(no account)';
      return el('tr', {}, el('td', { text: label }), el('td', { class: 'num', text: String(u.rows) }), el('td', { class: 'num', text: money(u.amount) }), el('td', {}, select));
    });
    ui.codes.replaceChildren(
      el('h3', { class: 'si-heading', text: 'Account numbers not in this budget' }),
      el('p', { class: 'si-more', text: 'Keep them unassigned (the cost report lists them apart, and adding the account later picks them up), skip them, or put them in an account.' }),
      el('div', { class: 'si-table-wrap' }, el('table', { class: 'si-table' }, el('thead', {}, head), el('tbody', {}, ...body))),
    );
  };

  const noteList = (items, count, what, open = false) => {
    if (!count) return null;
    const list = el('ul', { class: 'si-notes' }, ...items.map((n) => el('li', { text: `Row ${n.row}: ${n.message}` })));
    if (count > items.length) list.append(el('li', { class: 'si-more', text: `and ${count - items.length} more` }));
    const d = el('details', { class: 'si-details' }, el('summary', { text: `${what} (${count})` }), list);
    if (open) d.open = true;
    return d;
  };

  const renderDetails = () => {
    const p = st.preview;
    if (!p) {
      ui.details.replaceChildren();
      return;
    }
    const dups = p.duplicates.map((d) => ({
      row: d.row,
      message: `${d.repeats === null ? 'matches a transaction already in this budget' : `repeats row ${d.repeats}`} (${[d.date, d.account, money(d.amount), d.description].filter(Boolean).join(', ')}): ${d.skipped ? 'skipped' : 'imported'}`,
    }));
    ui.details.replaceChildren(
      ...[
        noteList(dups, p.duplicate_count, 'Possible duplicates', p.duplicate_count > 0 && p.duplicates_skipped < p.duplicate_count),
        noteList(p.skipped, p.skipped_count, 'Skipped rows'),
        noteList(p.warnings, p.warning_count, 'Rows with something left out'),
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
      const title = el('h2', { class: 'modal-title', text: `Import actuals: ${picked.file}`, attrs: { id: 'ai-title' } });
      const intro = el('p', {
        class: 'modal-message',
        text: 'Map the columns of your accounting export. Each transaction is matched to an account by its number, and the cost report adds them up. Nothing changes until you import, and the whole import can be removed later from the Actuals tab.',
      });
      ui.top = el('div', { class: 'si-top' });
      ui.truncated = el('p', { class: 'si-note', text: 'This sheet is larger than the import reads (100,000 rows, 256 columns, 2 million cells): only what was read is shown and imported.' });
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
      order.append(el('option', { text: 'As the file’s dates show (month first when they don’t)', attrs: { value: '' } }));
      order.append(el('option', { text: 'Month first (9/30/2026)', attrs: { value: 'month_first' } }));
      order.append(el('option', { text: 'Day first (30/9/2026)', attrs: { value: 'day_first' } }));
      order.value = st.dateOrder;
      order.addEventListener('change', () => {
        st.dateOrder = order.value;
        schedule();
      });
      const negate = box('ai-negate', 'Costs are negative numbers in this file (flip every sign)', () => st.negate, (v) => (st.negate = v));
      const carry = box('ai-carry', 'Empty account cells take the account above (reports that name each account once)', () => st.carryAccount, (v) => (st.carryAccount = v));
      const existing = box('ai-existing', 'Skip rows that match a transaction already in this budget (same date, account, amount and description)', () => st.skipExisting, (v) => (st.skipExisting = v));
      const repeats = box('ai-repeats', 'Skip rows that repeat an earlier row of the file', () => st.skipRepeats, (v) => (st.skipRepeats = v));
      ui.syncOptions = () => {
        order.value = st.dateOrder;
        negate.b.checked = st.negate;
        carry.b.checked = st.carryAccount;
      };
      const options = el(
        'div',
        { class: 'si-options' },
        el('label', { class: 'si-inline', attrs: { for: 'ai-date-order' } }, el('span', { text: 'Dates' }), order),
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

      const cancel = el('button', { class: 'btn', text: 'Cancel', attrs: { type: 'button', 'data-autofocus': true } });
      ui.ok = el('button', { class: 'btn btn-primary', text: 'Import', attrs: { type: 'button' } });
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
        el('h3', { class: 'si-heading', text: 'Columns' }),
        ui.presets,
        ui.presetNote,
        ui.mapping,
        el('h3', { class: 'si-heading', text: 'Options' }),
        options,
        el('h3', { class: 'si-heading', text: 'Preview' }),
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
