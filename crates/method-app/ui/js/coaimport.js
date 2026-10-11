// File > Import Chart of Accounts… (src/coa_import.rs; the importer is
// method_format::import::coa):
//
// - Rust shows the Open dialog and reads the workbook, CSV, tab-delimited
//   file or budget file; the page names it by a token only.
// - The dialog follows Import Spreadsheet (sheetimport.js): the sheet, the
//   header row and a column for each field, guessed by Rust; then the
//   numbering pattern Rust inferred from the numbers (changeable), the
//   options, and the preview: the chart as it will come in, the rows skipped,
//   the warnings, and for "apply to this budget" what is added, renamed and
//   kept. Nothing changes until Import.
//
// Everything is drawn with el() and textContent: text from a file is never
// read as HTML.

import { el } from './dom.js';
import { customDialog } from './modal.js';
import { plural } from './util.js';

/** The fields' names in the dialog, by the key Rust uses. */
export const COA_FIELD_LABELS = {
  category_code: 'Category number',
  category_name: 'Category name',
  account_code: 'Account number',
  account_name: 'Account name',
  level: 'Level (ATL, BTL...)',
};

const FIELD_TITLES = {
  category_code: 'May be the same column as the account number, for a one-column outline',
  category_name: 'Not mapped: the text to the right of the number',
  account_name: 'Not mapped: the text to the right of the number',
  level: 'Where this changes, a production total goes in',
};

/** Separators a pattern may use: [value, label]. */
export const SEPARATORS = [
  ['', 'None'],
  ['-', 'Hyphen (11-01)'],
  ['.', 'Period (11.01)'],
  [' ', 'Space (11 01)'],
  ['/', 'Slash (11/01)'],
  [':', 'Colon (11:01)'],
  [';', 'Semicolon (11;01)'],
  [',', 'Comma (11,01)'],
];

/** A pattern in words: "2 + 2, no separator". */
export function codingText(c) {
  if (!c) return '';
  const widths = [c.category, c.account];
  if (c.sub) widths.push(c.sub);
  const sep = SEPARATORS.find(([v]) => v === c.separator);
  const sepText = !c.separator ? 'no separator' : sep ? sep[1].replace(/ \(.*\)$/, '').toLowerCase() : `"${c.separator}"`;
  return `${widths.join(' + ')}, ${sepText}`;
}

/** What Rust gets for the dialog's state (`extra`: detect_header, guess). */
export function coaRequestFrom(st, extra = {}) {
  return {
    sheet: st.sheet,
    header_row: st.headerRow,
    mapping: { ...st.mapping },
    detect_header: false,
    guess: false,
    coding: st.coding ? { ...st.coding } : null,
    sub_accounts: st.subAccounts,
    default_levels: st.defaultLevels,
    target: st.target,
    remove_unlisted: st.removeUnlisted,
    ...extra,
  };
}

function columnLabel(c) {
  const title = c.title ? `: ${c.title}` : '';
  const sample = c.sample && c.sample !== c.title ? ` (${c.sample})` : '';
  return `${c.letter}${title}${sample}`;
}

/**
 * File > Import Chart of Accounts…: picks a file (Rust's dialog) and shows
 * the dialog. `ctx`: { invoke, apply(view), toast, focus() }.
 */
export async function importChart(ctx) {
  let picked;
  try {
    picked = await ctx.invoke('coa_import_pick');
  } catch (e) {
    ctx.toast(e);
    ctx.focus();
    return;
  }
  if (!picked) {
    ctx.focus();
    return;
  }
  showChartDialog(ctx, picked);
}

/** The dialog for a picked file (exported for the browser harness). */
export function showChartDialog(ctx, picked) {
  const budgetFile = picked.kind === 'budget';
  const st = {
    token: picked.token,
    sheet: picked.sheet || 0,
    headerRow: null,
    mapping: {},
    coding: null,
    subAccounts: false,
    defaultLevels: true,
    target: 'new',
    removeUnlisted: false,
    preview: null,
    // The last preview failed: its controls stay so the choice can be fixed.
    failed: false,
  };
  const fields = Array.isArray(picked.fields) && picked.fields.length ? picked.fields : Object.keys(COA_FIELD_LABELS);
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
    ui.ok.disabled = busy || st.failed || !p || p.categories === 0;
    ui.ok.textContent = st.target === 'current' ? 'Apply Chart' : 'Import Chart';
  };

  const refresh = async (extra = {}) => {
    const mine = ++seq;
    busy = true;
    updateButton();
    try {
      const p = await ctx.invoke('coa_import_preview', { token: st.token, request: coaRequestFrom(st, extra) });
      if (mine !== seq) return null;
      st.preview = p;
      st.failed = false;
      st.headerRow = p.header_row === undefined ? null : p.header_row;
      st.mapping = { ...p.mapping };
      setError('');
      render();
      return p;
    } catch (e) {
      if (mine !== seq) return null;
      st.failed = true;
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

  /** A sheet chosen (or the dialog opened): find the header and guess. */
  const startSheet = () => {
    st.coding = null;
    return refresh({ detect_header: true, guess: true });
  };

  // --- the parts that change with each preview ---

  const fieldSelect = (key) => {
    const isName = key === 'category_name' || key === 'account_name';
    const select = el('select', {
      class: 'modal-input si-col',
      attrs: { 'aria-label': `Column for ${COA_FIELD_LABELS[key] || key}` },
      data: { siKey: `coa-field-${key}` },
    });
    select.append(el('option', { text: isName ? '(the text after the number)' : '(none)', attrs: { value: '' } }));
    for (const c of st.preview ? st.preview.columns : []) {
      select.append(el('option', { text: columnLabel(c), attrs: { value: String(c.index) } }));
    }
    select.value = st.mapping[key] === undefined ? '' : String(st.mapping[key]);
    select.addEventListener('change', () => {
      if (select.value === '') delete st.mapping[key];
      else st.mapping[key] = Number(select.value);
      schedule();
    });
    return select;
  };

  const renderTop = () => {
    const p = st.preview;
    const parts = [];
    if (budgetFile) {
      ui.top.replaceChildren(el('p', { class: 'modal-message', text: 'A budget file: its own categories, accounts and production totals are read, nothing else.' }));
      return;
    }
    if (picked.sheets.length > 1) {
      const sheet = el('select', { class: 'modal-input', data: { siKey: 'coa-sheet' } });
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
    const header = el('select', { class: 'modal-input', data: { siKey: 'coa-header' } });
    header.append(el('option', { text: 'No header row: data from row 1', attrs: { value: '' } }));
    for (const r of p ? p.raw : []) {
      const cells = r.cells.filter((c) => c.trim() !== '').slice(0, 5).join(' | ');
      header.append(el('option', { text: `Row ${r.row}: ${cells || '(empty)'}`, attrs: { value: String(r.row - 1) } }));
    }
    header.value = st.headerRow === null ? '' : String(st.headerRow);
    header.addEventListener('change', () => {
      st.headerRow = header.value === '' ? null : Number(header.value);
      refresh({ guess: true });
    });
    parts.push(el('label', { class: 'modal-field si-field' }, el('span', { text: 'Header row' }), header));
    ui.top.replaceChildren(...parts);
  };

  const renderMapping = () => {
    const p = st.preview;
    if (budgetFile) {
      ui.mapHead.hidden = true;
      ui.mapping.replaceChildren();
      return;
    }
    ui.mapHead.hidden = false;
    if (p && p.kinded) {
      ui.mapping.replaceChildren(
        el('p', { class: 'si-note', text: 'This file has kind, code, name and parent columns: each row says whether it is a category, account, sub-account or total, so no column choices are needed.' }),
      );
      return;
    }
    ui.mapping.replaceChildren(
      ...fields.map((key) =>
        el('label', { class: 'si-map-row', title: FIELD_TITLES[key] || '' }, el('span', { class: 'si-map-label', text: COA_FIELD_LABELS[key] || key }), fieldSelect(key)),
      ),
    );
  };

  const renderCoding = () => {
    const p = st.preview;
    if (!p) {
      ui.coding.replaceChildren();
      return;
    }
    const c = st.coding || p.coding;
    const width = (key, label, min) => {
      const input = el('input', {
        class: 'modal-input coa-width',
        attrs: { type: 'number', min: String(min), max: '6', step: '1', 'aria-label': label },
        data: { siKey: `coa-width-${key}` },
      });
      input.value = String(c[key] || 0);
      input.addEventListener('change', () => {
        const n = Math.max(min, Math.min(6, Math.round(Number(input.value) || 0)));
        st.coding = { ...(st.coding || p.coding), [key]: n };
        schedule();
      });
      return el('label', { class: 'si-inline' }, el('span', { text: label }), input);
    };
    const sep = el('select', { class: 'modal-input', attrs: { 'aria-label': 'Separator' }, data: { siKey: 'coa-sep' } });
    for (const [v, label] of SEPARATORS) sep.append(el('option', { text: label, attrs: { value: v } }));
    sep.value = c.separator || '';
    sep.addEventListener('change', () => {
      st.coding = { ...(st.coding || p.coding), separator: sep.value };
      schedule();
    });
    const parts = [
      el('p', { class: 'modal-message', text: `The numbers in the file follow ${codingText(p.inferred)}.${st.coding ? ` They come in as ${codingText(st.coding)}.` : ''}` }),
      el('div', { class: 'coa-coding' }, width('category', 'Category', 1), width('account', 'Account', 1), width('sub', 'Sub-account', 0), el('label', { class: 'si-inline' }, el('span', { text: 'Separator' }), sep)),
    ];
    if (st.coding) {
      const reset = el('button', { class: 'btn btn-quiet', text: 'Use the File’s Pattern', attrs: { type: 'button' } });
      reset.addEventListener('click', () => {
        st.coding = null;
        refresh();
      });
      parts.push(reset);
    }
    ui.coding.replaceChildren(...parts);
  };

  const renderOptions = () => {
    const p = st.preview;
    const parts = [];
    const check = (id, checked, text, onChange) => {
      const box = el('input', { attrs: { type: 'checkbox', id } });
      box.checked = checked;
      box.addEventListener('change', () => onChange(box.checked));
      return el('label', { class: 'rs-check', attrs: { for: id } }, box, el('span', { text }));
    };
    if (p && p.sub_accounts > 0 && !p.kinded) {
      parts.push(
        check('coa-subs', st.subAccounts, p.sub_accounts === 1 ? 'Keep the sub-account as an account of its own (otherwise it is left out)' : `Keep the ${p.sub_accounts} sub-accounts as accounts of their own (otherwise they are left out)`, (v) => {
          st.subAccounts = v;
          schedule();
        }),
      );
    }
    if (p && !p.levels_from_source) {
      parts.push(
        check('coa-levels', st.defaultLevels, 'The file names no levels: add our production totals (above the line, production, post, other) by category number', (v) => {
          st.defaultLevels = v;
          schedule();
        }),
      );
    }
    const radio = (value, text) => {
      const r = el('input', { attrs: { type: 'radio', name: 'coa-target', value } });
      r.checked = st.target === value;
      r.addEventListener('change', () => {
        if (!r.checked) return;
        st.target = value;
        renderOptions();
        updateButton();
        schedule();
      });
      return el('label', { class: 'rs-check' }, r, el('span', { text }));
    };
    parts.push(
      el(
        'div',
        { class: 'si-target' },
        el('span', { text: 'Import into' }),
        radio('new', 'A new budget tab with this chart and no lines'),
        radio('current', 'This budget: add the categories and accounts it lacks and rename the ones it has by number (one undo step)'),
      ),
    );
    if (st.target === 'current') {
      parts.push(
        check('coa-remove', st.removeUnlisted, 'Also remove empty accounts the chart doesn’t list (accounts with lines are always kept)', (v) => {
          st.removeUnlisted = v;
          schedule();
        }),
      );
    }
    ui.options.replaceChildren(...parts);
  };

  const noteList = (items, count, what, open = false) => {
    if (!count) return null;
    const list = el('ul', { class: 'si-notes' }, ...items.map((n) => el('li', { text: n.row ? `Row ${n.row}: ${n.message}` : n.message })));
    if (count > items.length) list.append(el('li', { class: 'si-more', text: `and ${count - items.length} more` }));
    return el('details', { class: 'si-details', attrs: open ? { open: true } : {} }, el('summary', { text: `${what} (${count})` }), list);
  };

  const textList = (items, count, what, open = false) => {
    if (!count) return null;
    const list = el('ul', { class: 'si-notes' }, ...items.map((t) => el('li', { text: t })));
    if (count > items.length) list.append(el('li', { class: 'si-more', text: `and ${count - items.length} more` }));
    return el('details', { class: 'si-details', attrs: open ? { open: true } : {} }, el('summary', { text: `${what} (${count})` }), list);
  };

  const renderPreview = () => {
    const p = st.failed ? null : st.preview;
    if (!p) {
      ui.summary.replaceChildren();
      ui.rows.replaceChildren();
      ui.details.replaceChildren();
      return;
    }
    let line = `${plural(p.categories, 'category', 'categories')}, ${plural(p.accounts, 'account', 'accounts')} and ${plural(p.levels, 'production total', 'production totals')}, numbered ${codingText(p.coding)}.`;
    if (p.skipped_count) line += ` ${plural(p.skipped_count, 'row', 'rows')} skipped.`;
    if (p.totals_skipped) line += ` ${plural(p.totals_skipped, 'total row', 'total rows')} left out.`;
    const summary = [el('p', { class: 'si-summary-main', text: line })];
    if (p.default_levels_added) summary.push(el('p', { class: 'si-note', text: 'Our production totals were added: change them on the topsheet if the chart groups its categories differently.' }));
    if (p.renumbered_count) summary.push(el('p', { class: 'si-note', text: `${plural(p.renumbered_count, 'account shares its number', 'accounts share their numbers')} with an account of another name in the chart, so ${p.renumbered_count === 1 ? 'it comes' : 'they come'} in under a new number: see Renumbered below.` }));
    if (p.merged_repeats) summary.push(el('p', { class: 'si-note', text: `${plural(p.merged_repeats, 'account is', 'accounts are')} listed twice with the same number and name: each comes in once.` }));
    if (p.placeholders) summary.push(el('p', { class: 'si-note', text: `${plural(p.placeholders, 'account', 'accounts')} named “.” (unused slots in the chart) left out.` }));
    if (p.truncated) summary.push(el('p', { class: 'si-note', text: 'This sheet is larger than the import reads: only what was read is shown and imported.' }));
    ui.summary.replaceChildren(...summary);

    const body = p.rows.map((r) =>
      el(
        'tr',
        { class: `coa-${r.kind}` },
        el('td', { class: 'coa-code', text: r.code }),
        el('td', { class: 'si-desc', text: r.name, title: r.name }),
      ),
    );
    const table = el('table', { class: 'si-table coa-table' }, el('thead', {}, el('tr', {}, el('th', { text: 'Number' }), el('th', { text: 'Name' }))), el('tbody', {}, ...body));
    const total = p.categories + p.accounts + p.levels;
    const more = total > p.rows.length ? el('p', { class: 'si-more', text: `The first ${p.rows.length} of ${total} rows.` }) : null;
    ui.rows.replaceChildren(...[el('div', { class: 'si-table-wrap' }, table), more].filter(Boolean));

    const parts = [
      textList(p.renumbered, p.renumbered_count, 'Renumbered (repeated numbers)', true),
      noteList(p.warnings, p.warning_count, 'To check', true),
      noteList(p.skipped, p.skipped_count, 'Skipped rows'),
    ];
    const a = p.apply;
    if (a) {
      parts.push(
        textList(
          [...a.added_categories.map((c) => `Category ${c}`), ...a.added_accounts.map((x) => `Account ${x}`)],
          a.added_category_count + a.added_account_count,
          'To add',
        ),
        textList(a.renamed, a.renamed_count, 'To rename'),
        textList(a.kept, a.kept_count, 'Not in the chart, kept because they have lines', true),
        textList(a.unlisted_empty, a.unlisted_empty_count, st.removeUnlisted ? 'Not in the chart and empty: to remove' : 'Not in the chart and empty: kept'),
        textList(a.not_fitting, a.not_fitting.length, 'Numbers that don’t fit this budget’s numbering: left out', true),
      );
    }
    ui.details.replaceChildren(...parts.filter(Boolean));
    if (a) {
      const n = a.added_category_count + a.added_account_count + a.renamed_count;
      ui.summary.append(
        el('p', {
          class: 'modal-message',
          text: n
            ? `This budget: ${plural(a.added_category_count, 'category', 'categories')} and ${plural(a.added_account_count, 'account', 'accounts')} to add, ${a.renamed_count} to rename, ${a.unchanged_accounts} already as the chart has them.`
            : 'This budget already has this chart: nothing to add or rename.',
        }),
      );
    }
  };

  function render() {
    const active = document.activeElement;
    const focusKey = active && active.dataset ? active.dataset.siKey : '';
    renderTop();
    renderMapping();
    renderCoding();
    renderOptions();
    renderPreview();
    updateButton();
    if (focusKey && ui.root) {
      const again = ui.root.querySelector(`[data-si-key="${focusKey}"]`);
      if (again && again !== document.activeElement) again.focus({ preventScroll: true });
    }
  }

  customDialog(
    (closeDialog) => {
      const title = el('h2', { class: 'modal-title', text: `Import chart of accounts: ${picked.file}`, attrs: { id: 'coa-title' } });
      const intro = el('p', {
        class: 'modal-message',
        text: 'Only category and account numbers, names and production totals are read. Nothing changes until you import.',
      });
      ui.top = el('div', { class: 'si-top' });
      ui.mapHead = el('h3', { class: 'si-heading', text: 'Columns' });
      ui.mapping = el('div', { class: 'si-map' });
      ui.coding = el('div', { class: 'coa-coding-wrap' });
      ui.options = el('div', { class: 'si-options' });
      ui.summary = el('div', { class: 'si-summary', attrs: { 'aria-live': 'polite' } });
      ui.rows = el('div', { class: 'si-lines' });
      ui.details = el('div', { class: 'si-detail-list' });
      ui.error = el('p', { class: 'modal-error', attrs: { role: 'alert' } });
      ui.error.hidden = true;
      const cancel = el('button', { class: 'btn', text: 'Cancel', attrs: { type: 'button', 'data-autofocus': true } });
      ui.ok = el('button', { class: 'btn btn-primary', text: 'Import Chart', attrs: { type: 'button' } });
      ui.ok.disabled = true;
      cancel.addEventListener('click', () => closeDialog());
      ui.ok.addEventListener('click', async () => {
        if (ui.ok.disabled) return;
        clearTimeout(timer);
        busy = true;
        updateButton();
        try {
          const r = await ctx.invoke('coa_import', { token: st.token, request: coaRequestFrom(st) });
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
        ui.mapHead,
        ui.mapping,
        el('h3', { class: 'si-heading', text: 'Account numbers' }),
        ui.coding,
        el('h3', { class: 'si-heading', text: 'Options' }),
        ui.options,
        el('h3', { class: 'si-heading', text: 'Preview' }),
        ui.summary,
        ui.rows,
        ui.details,
      );
      ui.root = el(
        'div',
        { class: 'modal si-dialog coa-dialog', attrs: { role: 'dialog', 'aria-modal': 'true', 'aria-labelledby': 'coa-title' } },
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
