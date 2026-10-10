// File > Import Spreadsheet… (src/sheet_import.rs; the readers and the
// mapping are fwb_format::import):
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
import { plural } from './util.js';

/** The fields' names in the dialog, by the key Rust uses. */
export const FIELD_LABELS = {
  category_code: 'Category number',
  category_name: 'Category name',
  account_code: 'Account number',
  account_name: 'Account name',
  kind: 'Row kind',
  description: 'Description',
  amount: 'Amount',
  unit: 'Unit',
  x: 'X',
  rate: 'Rate',
  currency: 'Currency',
  x4: '4X',
  fringes: 'Fringes',
  fringe_range: 'Fringe range',
  groups: 'Groups',
  set: 'Set',
  location: 'Location',
  note: 'Note',
  line_total: 'Line total (to check)',
};

const FIELD_TITLES = {
  account_code: 'Read with this budget’s account numbering. An empty cell takes the account of the row above.',
  kind: 'Line, Subtotal, Adjustment or Separator. Anything else imports as a line.',
  fringes: 'Fringe names, separated by commas',
  groups: 'Group names, separated by commas',
  fringe_range: 'Lines with the same number in an account form one fringe range',
  line_total: 'Not imported: each line’s total is compared with it before you import',
};

const SETUP_LABELS = {
  fringe: 'Fringes',
  group: 'Groups',
  unit: 'Units',
  set: 'Sets',
  location: 'Locations',
  currency: 'Currencies',
};

const KIND_LABELS = { line: 'Line', subtotal: 'Subtotal', adjustment: 'Adjustment', separator: 'Separator' };

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

/** The dialog for a picked file (exported for the browser harness). */
export function showDialog(ctx, picked) {
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
    target: 'current',
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
    ui.ok.textContent = p && p.line_count ? `Import ${plural(p.line_count, 'Line', 'Lines')}` : 'Import';
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
      const sheet = el('select', { class: 'modal-input', attrs: { id: 'si-sheet' }, data: { siKey: 'sheet' } });
      picked.sheets.forEach((s, i) => {
        const label = `${s.name}${s.hidden ? ' (hidden)' : ''}, ${plural(s.rows, 'row', 'rows')}`;
        sheet.append(el('option', { text: label, attrs: { value: String(i) } }));
      });
      sheet.value = String(st.sheet);
      sheet.addEventListener('change', () => {
        st.sheet = Number(sheet.value);
        startSheet();
      });
      parts.push(el('label', { class: 'modal-field si-field' }, el('span', { text: 'Sheet' }), sheet));
    }
    const header = el('select', { class: 'modal-input', attrs: { id: 'si-header' }, data: { siKey: 'header' } });
    header.append(el('option', { text: 'No header row: data from row 1', attrs: { value: '' } }));
    for (const r of p ? p.raw : []) {
      const cells = r.cells.filter((c) => c.trim() !== '').slice(0, 5).join(' | ');
      header.append(el('option', { text: `Row ${r.row}: ${cells || '(empty)'}`, attrs: { value: String(r.row - 1) } }));
    }
    header.value = st.headerRow === null ? '' : String(st.headerRow);
    header.addEventListener('change', () => {
      st.headerRow = header.value === '' ? null : Number(header.value);
      st.presetName = '';
      // New titles: guess the columns again.
      refresh({ guess: true });
    });
    parts.push(el('label', { class: 'modal-field si-field' }, el('span', { text: 'Header row' }), header));
    ui.top.replaceChildren(...parts);
    const sheetInfo = p && picked.sheets[st.sheet];
    ui.truncated.hidden = !(sheetInfo && sheetInfo.truncated);
  };

  /** The saved-mapping controls, built once; renderPresets keeps them current. */
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
        create_accounts: st.createAccounts,
        create_setup: st.createSetup,
        default_account: st.accountTyped ? st.defaultAccount : '',
      });
      writePref(PRESETS_PREF, st.presets);
      st.presetName = n;
      renderPresets();
      ui.presetNote.textContent = `Saved. A sheet with these column titles will start with "${n}".`;
    });
    const del = el('button', { class: 'btn btn-quiet', text: 'Delete', attrs: { type: 'button' } });
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
    select.replaceChildren(el('option', { text: st.presets.length ? 'Saved mappings\u2026' : 'No saved mappings', attrs: { value: '' } }));
    for (const p of st.presets) select.append(el('option', { text: p.name, attrs: { value: p.name } }));
    select.value = st.presetName && st.presets.some((p) => p.name === st.presetName) ? st.presetName : '';
    select.disabled = !st.presets.length;
    del.disabled = !select.value;
    if (st.presetName && !name.value) name.value = st.presetName;
    ui.presetNote.textContent = st.presetName ? `Using the saved mapping "${st.presetName}".` : '';
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
      ui.lines.replaceChildren(el('p', { class: 'modal-message', text: p ? 'No rows will be imported with these choices.' : '' }));
      return;
    }
    const mapped = (k) => st.mapping[k] !== undefined;
    const showKind = p.lines.some((l) => l.kind !== 'line');
    const cols = [
      ['row', 'Row', 'num'],
      ['account', 'Account', ''],
      showKind ? ['kind', 'Kind', ''] : null,
      ['description', 'Description', 'si-desc'],
      mapped('amount') ? ['amount', 'Amount', 'num'] : null,
      mapped('unit') ? ['unit', 'Unit', ''] : null,
      mapped('x') ? ['x', 'X', 'num'] : null,
      mapped('rate') ? ['rate', 'Rate', 'num'] : null,
      mapped('currency') ? ['currency', 'Cur', ''] : null,
      mapped('x4') ? ['x4', '4X', 'num'] : null,
      mapped('fringes') ? ['fringes', 'Fringes', ''] : null,
      mapped('groups') ? ['groups', 'Groups', ''] : null,
      mapped('set') ? ['set', 'Set', ''] : null,
      mapped('location') ? ['location', 'Location', ''] : null,
      ['total', 'Total', 'num'],
      mapped('line_total') ? ['sheet_total', 'Sheet total', 'num'] : null,
    ].filter(Boolean);
    const head = el('tr', {}, ...cols.map(([, label, cls]) => el('th', { class: cls, text: label })));
    const body = p.lines.map((l) => {
      const differs = mapped('line_total') && l.sheet_total !== null && l.total !== null && l.sheet_total !== l.total;
      const cells = cols.map(([key, , cls]) => {
        let text;
        if (key === 'total' || key === 'sheet_total') text = l[key] === null ? '' : money(l[key]);
        else if (key === 'kind') text = KIND_LABELS[l.kind] || l.kind;
        else if (key === 'account') text = l.new_account ? `${l.account} (new)` : l.account;
        else text = cellText(l[key]);
        return el('td', { class: cls, text, title: key === 'description' ? text : '' });
      });
      return el('tr', { class: differs ? 'si-differs' : '' }, ...cells);
    });
    const table = el('table', { class: 'si-table' }, el('thead', {}, head), el('tbody', {}, ...body));
    const more = p.line_count > p.lines.length ? el('p', { class: 'si-more', text: `The first ${p.lines.length} of ${p.line_count} lines.` }) : null;
    ui.lines.replaceChildren(...[el('div', { class: 'si-table-wrap' }, table), more].filter(Boolean));
  };

  const noteList = (items, count, what) => {
    if (!count) return null;
    const list = el('ul', { class: 'si-notes' }, ...items.map((n) => el('li', { text: `Row ${n.row}: ${n.message}` })));
    const more = count > items.length ? el('li', { class: 'si-more', text: `and ${count - items.length} more` }) : null;
    if (more) list.append(more);
    return el('details', { class: 'si-details' }, el('summary', { text: `${what} (${count})` }), list);
  };

  const renderSummary = () => {
    const p = st.preview;
    if (!p) {
      ui.summary.replaceChildren();
      return;
    }
    const where = st.target === 'new' ? 'a new budget tab' : 'this budget';
    const parts = [];
    const accounts = p.matched_accounts + p.new_accounts.length;
    let line = `${plural(p.line_count, 'line', 'lines')} into ${plural(accounts, 'account', 'accounts')} of ${where}`;
    const extra = [];
    if (p.new_accounts.length) extra.push(plural(p.new_accounts.length, 'new account', 'new accounts'));
    if (p.new_categories.length) extra.push(plural(p.new_categories.length, 'new category', 'new categories'));
    if (extra.length) line += ` (${extra.join(', ')})`;
    line += '.';
    if (p.skipped_count) line += ` ${plural(p.skipped_count, 'row', 'rows')} skipped.`;
    if (p.headings) line += ` ${plural(p.headings, 'heading row', 'heading rows')} only set the account below.`;
    parts.push(el('p', { class: 'si-summary-main', text: line }));
    if (p.grand_total_before !== null && p.grand_total_after !== null) {
      parts.push(el('p', { class: 'modal-message', text: `Grand total ${money(p.grand_total_before)} now, ${money(p.grand_total_after)} after the import.` }));
    }
    if (p.calc_error) parts.push(el('p', { class: 'modal-error', text: `The totals can't be worked out: ${p.calc_error}` }));
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
    const parts = [el('h3', { class: 'si-heading', text: 'Check against the sheet’s totals' })];
    if (!r.compared) {
      parts.push(el('p', { class: 'modal-message', text: 'The total column has no numbers on the imported rows.' }));
    } else if (!r.mismatched) {
      parts.push(
        el('p', {
          class: 'si-ok',
          text: `All ${plural(r.compared, 'line', 'lines')} with a total in the sheet come to it exactly (${money(r.computed_sum)}).`,
        }),
      );
    } else {
      parts.push(
        el('p', {
          class: 'si-bad',
          text: `${r.mismatched} of ${plural(r.compared, 'line', 'lines')} differ from the sheet: they come to ${money(r.computed_sum)} against the sheet's ${money(r.sheet_sum)}, a difference of ${money(r.difference)}.`,
        }),
      );
      const list = el(
        'ul',
        { class: 'si-notes' },
        ...r.first.map((m) =>
          el('li', {
            text: `Row ${m.row}, ${m.account}${m.description ? ` ${m.description}` : ''}: sheet ${money(m.sheet)}, imported ${money(m.computed)}, difference ${money(m.computed - m.sheet)}`,
          }),
        ),
      );
      if (r.mismatched > r.first.length) list.append(el('li', { class: 'si-more', text: `and ${r.mismatched - r.first.length} more` }));
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
    const parts = [noteList(p.skipped, p.skipped_count, 'Skipped rows'), noteList(p.warnings, p.warning_count, 'Rows with something left out')];
    if (p.new_accounts.length || p.new_categories.length) {
      const items = [...p.new_categories.map((c) => `Category ${c}`), ...p.new_accounts.map((a) => `Account ${a}`)];
      parts.push(el('details', { class: 'si-details' }, el('summary', { text: `New accounts and categories (${items.length})` }), el('ul', { class: 'si-notes' }, ...items.map((t) => el('li', { text: t })))));
    }
    if (p.added.length) {
      const n = p.added.reduce((a, s) => a + s.names.length, 0);
      const items = p.added.map((s) => el('li', { text: `${SETUP_LABELS[s.kind] || s.kind}: ${s.names.join(', ')}` }));
      parts.push(el('details', { class: 'si-details' }, el('summary', { text: `Setup items to add (${n})` }), el('ul', { class: 'si-notes' }, ...items)));
    }
    if (p.dropped.length) {
      const n = p.dropped.reduce((a, s) => a + s.names.length, 0);
      const items = p.dropped.map((s) =>
        el('li', { text: `${SETUP_LABELS[s.kind] || s.kind}: ${s.names.map((name, i) => `${name} (${plural(s.rows[i] || 0, 'row', 'rows')})`).join(', ')}` }),
      );
      parts.push(el('details', { class: 'si-details', attrs: { open: true } }, el('summary', { text: `Not in this budget, left off the lines (${n})` }), el('ul', { class: 'si-notes' }, ...items)));
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
      const title = el('h2', { class: 'modal-title', text: `Import spreadsheet: ${picked.file}`, attrs: { id: 'si-title' } });
      const intro = el('p', {
        class: 'modal-message',
        text: 'Pick the header row and the column for each field. Lines are added at the end of their accounts. Nothing changes until you import.',
      });
      ui.top = el('div', { class: 'si-top' });
      ui.truncated = el('p', { class: 'si-note', text: 'This sheet is larger than the import reads (100,000 rows, 256 columns, 2 million cells): only what was read is shown and imported.' });
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
        attrs: { type: 'text', id: 'si-default-account', placeholder: 'none', maxlength: '64', spellcheck: 'false', autocomplete: 'off' },
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
        el('label', { class: 'rs-check', attrs: { for: 'si-create-accounts' } }, accounts, el('span', { text: 'Add accounts and categories this budget doesn’t have' })),
        el('label', { class: 'rs-check', attrs: { for: 'si-create-setup' } }, setup, el('span', { text: 'Add fringes, groups, units, sets, locations and currencies it doesn’t have (otherwise they are left off the lines)' })),
        el('label', { class: 'si-inline', attrs: { for: 'si-default-account' } }, el('span', { text: 'Account for rows without one' }), fallback),
        el('div', { class: 'si-target' }, el('span', { text: 'Import into' }), radio('current', 'This budget (one undo step)'), radio('new', 'A new budget tab with this budget’s accounts and setup')),
      );

      ui.summary = el('div', { class: 'si-summary', attrs: { 'aria-live': 'polite' } });
      ui.lines = el('div', { class: 'si-lines' });
      ui.recon = el('div', { class: 'si-recon' });
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
        el('h3', { class: 'si-heading', text: 'Columns' }),
        ui.presets,
        ui.presetNote,
        ui.mapping,
        el('h3', { class: 'si-heading', text: 'Options' }),
        options,
        el('h3', { class: 'si-heading', text: 'Preview' }),
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
