// File > Import Spreadsheet… (src/sheet_import.rs; the readers and the
// mapping are method_format::import):
//
// - Rust shows the Open dialog, reads the workbook, CSV or tab-delimited
//   file and sends its sheets; the page names the file by a token only.
// - The dialog: the sheet, the header row (found by Rust), a column for each
//   field (guessed from the titles), the options, then the preview Rust works
//   out for those choices: the first lines as they will import, what they
//   come to, the reconciliation against the sheet's own totals, the rows
//   skipped and why, and what will be added. Every change asks for a new
//   preview; nothing changes in the budget until Import.
// - Mapping presets are kept per user in localStorage, by the layout of the
//   header row (its titles), so a sheet laid out the same way next time gets
//   its mapping and options back at once.
//
// Everything is drawn with el() and textContent: text from a spreadsheet is
// never read as HTML.

import { el } from './dom.js';
import { customDialog } from './modal.js';
import { money } from './format.js';
import { readPref, writePref } from './prefs.js';
import { t } from './i18n.js';

/** The fields' names in the dialog, by the key Rust uses. */
export const FIELD_LABELS = {
  category_code: t('sheetimport.field.category_code'),
  category_name: t('sheetimport.field.category_name'),
  account_code: t('sheetimport.field.account_code'),
  account_name: t('sheetimport.field.account_name'),
  kind: t('sheetimport.field.kind'),
  description: t('sheetimport.field.description'),
  amount: t('sheetimport.field.amount'),
  unit: t('sheetimport.field.unit'),
  x: t('sheetimport.field.x'),
  rate: t('sheetimport.field.rate'),
  currency: t('sheetimport.field.currency'),
  x4: t('sheetimport.field.x4'),
  fringes: t('sheetimport.field.fringes'),
  fringe_range: t('sheetimport.field.fringe_range'),
  groups: t('sheetimport.field.groups'),
  set: t('sheetimport.field.set'),
  location: t('sheetimport.field.location'),
  note: t('sheetimport.field.note'),
  line_total: t('sheetimport.field.line_total'),
};

const FIELD_TITLES = {
  account_code: t('sheetimport.field_tip.account_code'),
  kind: t('sheetimport.field_tip.kind'),
  fringes: t('sheetimport.field_tip.fringes'),
  groups: t('sheetimport.field_tip.groups'),
  fringe_range: t('sheetimport.field_tip.fringe_range'),
  line_total: t('sheetimport.field_tip.line_total'),
};

const SETUP_LABELS = {
  fringe: t('sheetimport.setup.fringe'),
  group: t('sheetimport.setup.group'),
  unit: t('sheetimport.setup.unit'),
  set: t('sheetimport.setup.set'),
  location: t('sheetimport.setup.location'),
  currency: t('sheetimport.setup.currency'),
};

const KIND_LABELS = { line: t('sheetimport.kind.line'), subtotal: t('sheetimport.kind.subtotal'), adjustment: t('sheetimport.kind.adjustment'), separator: t('sheetimport.kind.separator') };

// ---------------------------------------------------------------------------
// Presets (per user, in localStorage)

const PRESETS_PREF = 'sheetImport.presets';
export const MAX_PRESETS = 50;
const MAX_NAME = 80;

function isIndex(n) {
  return Number.isInteger(n) && n >= 0 && n < 100000;
}

/** A stored preset, checked; null when it isn't one. */
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
    create_accounts: p.create_accounts === true,
    create_setup: p.create_setup === true,
    default_account: typeof p.default_account === 'string' ? p.default_account.slice(0, 64) : '',
  };
}

/** The saved presets, newest first. */
export function loadPresets() {
  const raw = readPref(PRESETS_PREF, []);
  return Array.isArray(raw) ? raw.map(cleanPreset).filter(Boolean).slice(0, MAX_PRESETS) : [];
}

/** `list` with `preset` saved first (replacing one of the same name, ignoring case). */
export function withPreset(list, preset) {
  const p = cleanPreset(preset);
  if (!p) return list;
  const key = p.name.toLowerCase();
  return [p, ...list.filter((q) => q.name.toLowerCase() !== key)].slice(0, MAX_PRESETS);
}

export function withoutPreset(list, name) {
  const key = String(name).toLowerCase();
  return list.filter((q) => q.name.toLowerCase() !== key);
}

/** The newest preset for a header layout, or null. */
export function presetFor(list, signature) {
  if (!signature) return null;
  return list.find((p) => p.signature === signature) || null;
}

// ---------------------------------------------------------------------------
// The dialog

/** What Rust gets for the dialog's state (`extra`: detect_header, guess). */
export function requestFrom(st, extra = {}) {
  return {
    sheet: st.sheet,
    header_row: st.headerRow,
    mapping: { ...st.mapping },
    detect_header: false,
    guess: false,
    create_accounts: st.createAccounts,
    create_setup: st.createSetup,
    default_account: st.defaultAccount,
    target: st.target,
    ...extra,
  };
}

/** The column choices for a field's select: "C: Rate (450)". */
function columnLabel(c) {
  const title = c.title ? `: ${c.title}` : '';
  const sample = c.sample && c.sample !== c.title ? ` (${c.sample})` : '';
  return `${c.letter}${title}${sample}`;
}

/**
 * File > Import Spreadsheet…: picks a file (Rust's dialog) and shows the
 * mapping dialog. `ctx`: { invoke, apply(view), toast, focus(), account() }.
 */
export async function importSpreadsheet(ctx) {
  let picked;
  try {
    picked = await ctx.invoke('sheet_import_pick');
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

/**
 * The dialog for a picked file (also Open's, for a spreadsheet; exported
 * for the browser harness). `opts.target`: where it starts importing to,
 * `current` (this budget) or `new` (a new tab; Open's).
 */
export function showDialog(ctx, picked, opts = {}) {
  const st = {
    token: picked.token,
    sheet: picked.sheet || 0,
    headerRow: null,
    mapping: {},
    createAccounts: false,
    createSetup: false,
    defaultAccount: (ctx.account && ctx.account()) || '',
    // Typed by the user (else it is just the account on screen, which a
    // preset doesn't keep).
    accountTyped: false,
    target: opts.target === 'new' ? 'new' : 'current',
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
    ui.ok.disabled = busy || !p || p.line_count === 0;
    ui.ok.textContent = p && p.line_count ? t('sheetimport.import_n', { count: p.line_count }) : t('sheetimport.import');
  };

  /** Asks Rust for the preview of the current choices. */
  const refresh = async (extra = {}) => {
    const mine = ++seq;
    busy = true;
    updateButton();
    try {
      const p = await ctx.invoke('sheet_import_preview', { token: st.token, request: requestFrom(st, extra) });
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

  /** The preset's choices, keeping only columns the sheet has. */
  const usePreset = (p) => {
    const width = st.preview ? st.preview.columns.length : 0;
    st.headerRow = p.header_row;
    st.mapping = {};
    for (const [k, v] of Object.entries(p.mapping)) if (v < width) st.mapping[k] = v;
    st.createAccounts = p.create_accounts;
    st.createSetup = p.create_setup;
    if (p.default_account) {
      st.defaultAccount = p.default_account;
      st.accountTyped = true;
    }
    st.presetName = p.name;
  };

  /** A sheet chosen (or the dialog opened): find the header, guess, then any preset. */
  const startSheet = async () => {
    st.presetName = '';
    const p = await refresh({ detect_header: true, guess: true });
    if (!p) return;
    const preset = presetFor(st.presets, p.signature);
    if (preset) {
      usePreset(preset);
      await refresh();
    }
  };

  // --- building the parts that change with each preview ---

  const fieldSelect = (key) => {
    const select = el('select', {
      class: 'modal-input si-col',
      attrs: { 'aria-label': t('sheetimport.column_for', { field: FIELD_LABELS[key] || key }) },
      data: { siKey: `field-${key}` },
    });
    select.append(el('option', { text: t('sheetimport.not_imported'), attrs: { value: '' } }));
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
      const sheet = el('select', { class: 'modal-input', attrs: { id: 'si-sheet' }, data: { siKey: 'sheet' } });
      picked.sheets.forEach((s, i) => {
        const label = s.hidden ? t('sheetimport.sheet_option_hidden', { name: s.name, count: s.rows }) : t('sheetimport.sheet_option', { name: s.name, count: s.rows });
        sheet.append(el('option', { text: label, attrs: { value: String(i) } }));
      });
      sheet.value = String(st.sheet);
      sheet.addEventListener('change', () => {
        st.sheet = Number(sheet.value);
        startSheet();
      });
      parts.push(el('label', { class: 'modal-field si-field' }, el('span', { text: t('sheetimport.sheet') }), sheet));
    }
    const header = el('select', { class: 'modal-input', attrs: { id: 'si-header' }, data: { siKey: 'header' } });
    header.append(el('option', { text: t('sheetimport.no_header_row'), attrs: { value: '' } }));
    for (const r of p ? p.raw : []) {
      const cells = r.cells.filter((c) => c.trim() !== '').slice(0, 5).join(' | ');
      header.append(el('option', { text: t('sheetimport.header_option', { row: r.row, cells: cells || t('sheetimport.empty_row') }), attrs: { value: String(r.row - 1) } }));
    }
    header.value = st.headerRow === null ? '' : String(st.headerRow);
    header.addEventListener('change', () => {
      st.headerRow = header.value === '' ? null : Number(header.value);
      st.presetName = '';
      // New titles: guess the columns again.
      refresh({ guess: true });
    });
    parts.push(el('label', { class: 'modal-field si-field' }, el('span', { text: t('sheetimport.header_row') }), header));
    ui.top.replaceChildren(...parts);
    const sheetInfo = p && picked.sheets[st.sheet];
    ui.truncated.hidden = !(sheetInfo && sheetInfo.truncated);
  };

  /** The saved-mapping controls, built once; renderPresets keeps them current. */
  const buildPresets = () => {
    const select = el('select', { class: 'modal-input', attrs: { 'aria-label': t('sheetimport.saved_mappings') }, data: { siKey: 'preset' } });
    select.addEventListener('change', () => {
      const p = st.presets.find((q) => q.name === select.value);
      if (!p) return;
      usePreset(p);
      name.value = p.name;
      refresh();
    });
    const name = el('input', {
      class: 'modal-input si-preset-name',
      attrs: { type: 'text', placeholder: t('sheetimport.mapping_name'), 'aria-label': t('sheetimport.mapping_name'), maxlength: String(MAX_NAME), spellcheck: 'false', autocomplete: 'off' },
    });
    const save = el('button', { class: 'btn', text: t('sheetimport.save_mapping'), attrs: { type: 'button' } });
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
        create_accounts: st.createAccounts,
        create_setup: st.createSetup,
        default_account: st.accountTyped ? st.defaultAccount : '',
      });
      writePref(PRESETS_PREF, st.presets);
      st.presetName = n;
      renderPresets();
      ui.presetNote.textContent = t('sheetimport.saved_note', { name: n });
    });
    const del = el('button', { class: 'btn btn-quiet', text: t('sheetimport.delete_mapping'), attrs: { type: 'button' } });
    del.addEventListener('click', () => {
      if (!select.value) return;
      st.presets = withoutPreset(st.presets, select.value);
      writePref(PRESETS_PREF, st.presets);
      st.presetName = '';
      name.value = '';
      renderPresets();
    });
    ui.presetControls = { select, name, del };
    ui.presets.replaceChildren(select, name, save, del);
  };

  const renderPresets = () => {
    const { select, name, del } = ui.presetControls;
    select.replaceChildren(el('option', { text: st.presets.length ? t('sheetimport.saved_mappings_pick') : t('sheetimport.no_saved_mappings'), attrs: { value: '' } }));
    for (const p of st.presets) select.append(el('option', { text: p.name, attrs: { value: p.name } }));
    select.value = st.presetName && st.presets.some((p) => p.name === st.presetName) ? st.presetName : '';
    select.disabled = !st.presets.length;
    del.disabled = !select.value;
    if (st.presetName && !name.value) name.value = st.presetName;
    ui.presetNote.textContent = st.presetName ? t('sheetimport.using_mapping', { name: st.presetName }) : '';
  };

  const renderMapping = () => {
    const rows = fields.map((key) =>
      el(
        'label',
        { class: 'si-map-row', title: FIELD_TITLES[key] || '' },
        el('span', { class: 'si-map-label', text: FIELD_LABELS[key] || key }),
        fieldSelect(key),
      ),
    );
    ui.mapping.replaceChildren(...rows);
  };

  const cellText = (v) => (v === null || v === undefined ? '' : String(v));

  const renderLines = () => {
    const p = st.preview;
    if (!p || !p.lines.length) {
      ui.lines.replaceChildren(el('p', { class: 'modal-message', text: p ? t('sheetimport.no_rows') : '' }));
      return;
    }
    const mapped = (k) => st.mapping[k] !== undefined;
    const showKind = p.lines.some((l) => l.kind !== 'line');
    const cols = [
      ['row', t('sheetimport.col.row'), 'num'],
      ['account', t('sheetimport.col.account'), ''],
      showKind ? ['kind', t('sheetimport.col.kind'), ''] : null,
      ['description', t('sheetimport.col.description'), 'si-desc'],
      mapped('amount') ? ['amount', t('sheetimport.col.amount'), 'num'] : null,
      mapped('unit') ? ['unit', t('sheetimport.col.unit'), ''] : null,
      mapped('x') ? ['x', t('sheetimport.col.x'), 'num'] : null,
      mapped('rate') ? ['rate', t('sheetimport.col.rate'), 'num'] : null,
      mapped('currency') ? ['currency', t('sheetimport.col.currency'), ''] : null,
      mapped('x4') ? ['x4', t('sheetimport.col.x4'), 'num'] : null,
      mapped('fringes') ? ['fringes', t('sheetimport.col.fringes'), ''] : null,
      mapped('groups') ? ['groups', t('sheetimport.col.groups'), ''] : null,
      mapped('set') ? ['set', t('sheetimport.col.set'), ''] : null,
      mapped('location') ? ['location', t('sheetimport.col.location'), ''] : null,
      ['total', t('sheetimport.col.total'), 'num'],
      mapped('line_total') ? ['sheet_total', t('sheetimport.col.sheet_total'), 'num'] : null,
    ].filter(Boolean);
    const head = el('tr', {}, ...cols.map(([, label, cls]) => el('th', { class: cls, text: label })));
    const body = p.lines.map((l) => {
      const differs = mapped('line_total') && l.sheet_total !== null && l.total !== null && l.sheet_total !== l.total;
      const cells = cols.map(([key, , cls]) => {
        let text;
        if (key === 'total' || key === 'sheet_total') text = l[key] === null ? '' : money(l[key]);
        else if (key === 'kind') text = KIND_LABELS[l.kind] || l.kind;
        else if (key === 'account') text = l.new_account ? t('sheetimport.account_new', { account: l.account }) : l.account;
        else text = cellText(l[key]);
        return el('td', { class: cls, text, title: key === 'description' ? text : '' });
      });
      return el('tr', { class: differs ? 'si-differs' : '' }, ...cells);
    });
    const table = el('table', { class: 'si-table' }, el('thead', {}, head), el('tbody', {}, ...body));
    const more = p.line_count > p.lines.length ? el('p', { class: 'si-more', text: t('sheetimport.first_of', { shown: p.lines.length, count: p.line_count }) }) : null;
    ui.lines.replaceChildren(...[el('div', { class: 'si-table-wrap' }, table), more].filter(Boolean));
  };

  /** `what`: the heading, with the count in it. */
  const noteList = (items, count, what) => {
    if (!count) return null;
    const list = el('ul', { class: 'si-notes' }, ...items.map((n) => el('li', { text: t('sheetimport.row_note', { row: n.row, message: n.message }) })));
    const more = count > items.length ? el('li', { class: 'si-more', text: t('sheetimport.and_more', { count: count - items.length }) }) : null;
    if (more) list.append(more);
    return el('details', { class: 'si-details' }, el('summary', { text: what }), list);
  };

  const renderSummary = () => {
    const p = st.preview;
    if (!p) {
      ui.summary.replaceChildren();
      return;
    }
    const parts = [];
    const accounts = p.matched_accounts + p.new_accounts.length;
    const what = { lines: t('sheetimport.n_lines', { count: p.line_count }), accounts: t('sheetimport.n_accounts', { count: accounts }) };
    const newAccounts = p.new_accounts.length ? t('sheetimport.n_new_accounts', { count: p.new_accounts.length }) : '';
    const newCategories = p.new_categories.length ? t('sheetimport.n_new_categories', { count: p.new_categories.length }) : '';
    const extra = newAccounts && newCategories ? t('sheetimport.new_both', { accounts: newAccounts, categories: newCategories }) : newAccounts || newCategories;
    let main;
    if (st.target === 'new') main = extra ? t('sheetimport.summary_new_extra', { ...what, extra }) : t('sheetimport.summary_new', what);
    else main = extra ? t('sheetimport.summary_this_extra', { ...what, extra }) : t('sheetimport.summary_this', what);
    const sentences = [main];
    if (p.skipped_count) sentences.push(t('sheetimport.rows_skipped', { count: p.skipped_count }));
    if (p.headings) sentences.push(t('sheetimport.heading_rows', { count: p.headings }));
    parts.push(el('p', { class: 'si-summary-main', text: sentences.join(' ') }));
    if (p.grand_total_before !== null && p.grand_total_after !== null) {
      parts.push(el('p', { class: 'modal-message', text: t('sheetimport.grand_total', { before: money(p.grand_total_before), after: money(p.grand_total_after) }) }));
    }
    if (p.calc_error) parts.push(el('p', { class: 'modal-error', text: t('sheetimport.calc_error', { error: p.calc_error }) }));
    for (const n of p.notes || []) parts.push(el('p', { class: 'si-note', text: n }));
    ui.summary.replaceChildren(...parts);
  };

  const renderReconciliation = () => {
    const p = st.preview;
    const r = p && p.reconciliation;
    if (!r) {
      ui.recon.replaceChildren();
      ui.recon.hidden = true;
      return;
    }
    ui.recon.hidden = false;
    const parts = [el('h3', { class: 'si-heading', text: t('sheetimport.recon.heading') })];
    if (!r.compared) {
      parts.push(el('p', { class: 'modal-message', text: t('sheetimport.recon.no_numbers') }));
    } else if (!r.mismatched) {
      parts.push(
        el('p', {
          class: 'si-ok',
          text: t('sheetimport.recon.all_match', { count: r.compared, sum: money(r.computed_sum) }),
        }),
      );
    } else {
      parts.push(
        el('p', {
          class: 'si-bad',
          text: t('sheetimport.recon.mismatched', { mismatched: r.mismatched, count: r.compared, computed: money(r.computed_sum), sheet: money(r.sheet_sum), difference: money(r.difference) }),
        }),
      );
      const list = el(
        'ul',
        { class: 'si-notes' },
        ...r.first.map((m) =>
          el('li', {
            text: m.description
                ? t('sheetimport.recon.row_desc', { row: m.row, account: m.account, description: m.description, sheet: money(m.sheet), imported: money(m.computed), difference: money(m.computed - m.sheet) })
                : t('sheetimport.recon.row', { row: m.row, account: m.account, sheet: money(m.sheet), imported: money(m.computed), difference: money(m.computed - m.sheet) }),
          }),
        ),
      );
      if (r.mismatched > r.first.length) list.append(el('li', { class: 'si-more', text: t('sheetimport.and_more', { count: r.mismatched - r.first.length }) }));
      parts.push(list);
    }
    ui.recon.replaceChildren(...parts);
  };

  const renderDetails = () => {
    const p = st.preview;
    if (!p) {
      ui.details.replaceChildren();
      return;
    }
    const parts = [noteList(p.skipped, p.skipped_count, t('sheetimport.skipped_rows', { count: p.skipped_count })), noteList(p.warnings, p.warning_count, t('sheetimport.rows_left_out', { count: p.warning_count }))];
    if (p.new_accounts.length || p.new_categories.length) {
      const items = [...p.new_categories.map((c) => t('sheetimport.new_category', { code: c })), ...p.new_accounts.map((a) => t('sheetimport.new_account', { code: a }))];
      parts.push(el('details', { class: 'si-details' }, el('summary', { text: t('sheetimport.new_accounts_heading', { count: items.length }) }), el('ul', { class: 'si-notes' }, ...items.map((item) => el('li', { text: item })))));
    }
    if (p.added.length) {
      const n = p.added.reduce((a, s) => a + s.names.length, 0);
      const items = p.added.map((s) => el('li', { text: `${SETUP_LABELS[s.kind] || s.kind}: ${s.names.join(', ')}` }));
      parts.push(el('details', { class: 'si-details' }, el('summary', { text: t('sheetimport.setup_to_add', { count: n }) }), el('ul', { class: 'si-notes' }, ...items)));
    }
    if (p.dropped.length) {
      const n = p.dropped.reduce((a, s) => a + s.names.length, 0);
      const items = p.dropped.map((s) => {
        const names = s.names.map((name, i) => t('sheetimport.dropped_name', { name, count: s.rows[i] || 0 })).join(', ');
        return el('li', { text: `${SETUP_LABELS[s.kind] || s.kind}: ${names}` });
      });
      parts.push(el('details', { class: 'si-details', attrs: { open: true } }, el('summary', { text: t('sheetimport.dropped_heading', { count: n }) }), el('ul', { class: 'si-notes' }, ...items)));
    }
    ui.details.replaceChildren(...parts.filter(Boolean));
  };

  function render() {
    // Rebuilt controls keep the keyboard focus: each has a data-si-key.
    const active = document.activeElement;
    const focusKey = active && active.dataset ? active.dataset.siKey : '';
    renderTop();
    renderPresets();
    renderMapping();
    renderSummary();
    renderLines();
    renderReconciliation();
    renderDetails();
    updateButton();
    if (ui.syncOptions) ui.syncOptions();
    if (focusKey && ui.root) {
      const again = ui.root.querySelector(`[data-si-key="${focusKey}"]`);
      if (again && again !== document.activeElement) again.focus({ preventScroll: true });
    }
  }

  // --- the fixed parts ---

  customDialog(
    (closeDialog) => {
      const title = el('h2', { class: 'modal-title', text: t('sheetimport.title', { file: picked.file }), attrs: { id: 'si-title' } });
      const intro = el('p', {
        class: 'modal-message',
        text: t('sheetimport.intro'),
      });
      ui.top = el('div', { class: 'si-top' });
      ui.truncated = el('p', { class: 'si-note', text: t('sheetimport.truncated') });
      ui.truncated.hidden = true;
      ui.presets = el('div', { class: 'si-presets' });
      ui.presetNote = el('p', { class: 'si-more', attrs: { 'aria-live': 'polite' } });
      buildPresets();
      ui.mapping = el('div', { class: 'si-map' });

      const accounts = el('input', { attrs: { type: 'checkbox', id: 'si-create-accounts' } });
      accounts.checked = st.createAccounts;
      accounts.addEventListener('change', () => {
        st.createAccounts = accounts.checked;
        schedule();
      });
      const setup = el('input', { attrs: { type: 'checkbox', id: 'si-create-setup' } });
      setup.checked = st.createSetup;
      setup.addEventListener('change', () => {
        st.createSetup = setup.checked;
        schedule();
      });
      const fallback = el('input', {
        class: 'modal-input si-account',
        attrs: { type: 'text', id: 'si-default-account', placeholder: t('sheetimport.account_none'), maxlength: '64', spellcheck: 'false', autocomplete: 'off' },
      });
      fallback.value = st.defaultAccount;
      fallback.addEventListener('input', () => {
        st.defaultAccount = fallback.value;
        st.accountTyped = true;
        schedule();
      });
      const radio = (value, text) => {
        const r = el('input', { attrs: { type: 'radio', name: 'si-target', value } });
        r.checked = st.target === value;
        r.addEventListener('change', () => {
          if (!r.checked) return;
          st.target = value;
          schedule();
        });
        return el('label', { class: 'rs-check' }, r, el('span', { text }));
      };
      // A preset can change the options: the boxes follow each preview.
      ui.syncOptions = () => {
        accounts.checked = st.createAccounts;
        setup.checked = st.createSetup;
        if (fallback.value !== st.defaultAccount) fallback.value = st.defaultAccount;
      };
      const options = el(
        'div',
        { class: 'si-options' },
        el('label', { class: 'rs-check', attrs: { for: 'si-create-accounts' } }, accounts, el('span', { text: t('sheetimport.option.create_accounts') })),
        el('label', { class: 'rs-check', attrs: { for: 'si-create-setup' } }, setup, el('span', { text: t('sheetimport.option.create_setup') })),
        el('label', { class: 'si-inline', attrs: { for: 'si-default-account' } }, el('span', { text: t('sheetimport.option.default_account') }), fallback),
        el('div', { class: 'si-target' }, el('span', { text: t('sheetimport.option.import_into') }), radio('current', t('sheetimport.option.target_current')), radio('new', t('sheetimport.option.target_new'))),
      );

      ui.summary = el('div', { class: 'si-summary', attrs: { 'aria-live': 'polite' } });
      ui.lines = el('div', { class: 'si-lines' });
      ui.recon = el('div', { class: 'si-recon' });
      ui.details = el('div', { class: 'si-detail-list' });
      ui.error = el('p', { class: 'modal-error', attrs: { role: 'alert' } });
      ui.error.hidden = true;

      const cancel = el('button', { class: 'btn', text: t('modal.cancel'), attrs: { type: 'button', 'data-autofocus': true } });
      ui.ok = el('button', { class: 'btn btn-primary', text: t('sheetimport.import'), attrs: { type: 'button' } });
      ui.ok.disabled = true;
      cancel.addEventListener('click', () => closeDialog());
      ui.ok.addEventListener('click', async () => {
        if (ui.ok.disabled) return;
        clearTimeout(timer);
        busy = true;
        updateButton();
        try {
          const r = await ctx.invoke('sheet_import', { token: st.token, request: requestFrom(st) });
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
        el('h3', { class: 'si-heading', text: t('sheetimport.columns') }),
        ui.presets,
        ui.presetNote,
        ui.mapping,
        el('h3', { class: 'si-heading', text: t('sheetimport.options') }),
        options,
        el('h3', { class: 'si-heading', text: t('sheetimport.preview') }),
        ui.summary,
        ui.lines,
        ui.recon,
        ui.details,
      );
      ui.root = el(
        'div',
        { class: 'modal si-dialog', attrs: { role: 'dialog', 'aria-modal': 'true', 'aria-labelledby': 'si-title' } },
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
