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
import { t } from './i18n.js';

/** The fields' names in the dialog, by the key Rust uses. */
export const COA_FIELD_LABELS = {
  category_code: t('coaimport.field.category_code'),
  category_name: t('coaimport.field.category_name'),
  account_code: t('coaimport.field.account_code'),
  account_name: t('coaimport.field.account_name'),
  level: t('coaimport.field.level'),
};

const FIELD_TITLES = {
  category_code: t('coaimport.field_tip.category_code'),
  category_name: t('coaimport.field_tip.name_not_mapped'),
  account_name: t('coaimport.field_tip.name_not_mapped'),
  level: t('coaimport.field_tip.level'),
};

/** Separators a pattern may use: [value, label, name in a pattern's words]. */
export const SEPARATORS = [
  ['', t('coaimport.sep.none'), ''],
  ['-', t('coaimport.sep.hyphen'), t('coaimport.sep_name.hyphen')],
  ['.', t('coaimport.sep.period'), t('coaimport.sep_name.period')],
  [' ', t('coaimport.sep.space'), t('coaimport.sep_name.space')],
  ['/', t('coaimport.sep.slash'), t('coaimport.sep_name.slash')],
  [':', t('coaimport.sep.colon'), t('coaimport.sep_name.colon')],
  [';', t('coaimport.sep.semicolon'), t('coaimport.sep_name.semicolon')],
  [',', t('coaimport.sep.comma'), t('coaimport.sep_name.comma')],
];

/** A pattern in words: "2 + 2, no separator". */
export function codingText(c) {
  if (!c) return '';
  const widths = [c.category, c.account];
  if (c.sub) widths.push(c.sub);
  const sep = SEPARATORS.find(([v]) => v === c.separator);
  const sepText = !c.separator ? t('coaimport.no_separator') : sep ? sep[2] : t('coaimport.quoted_separator', { separator: c.separator });
  return t('coaimport.pattern', { widths: widths.join(' + '), separator: sepText });
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
    ui.ok.textContent = st.target === 'current' ? t('coaimport.apply_chart') : t('coaimport.import_chart');
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
      attrs: { 'aria-label': t('coaimport.column_for', { field: COA_FIELD_LABELS[key] || key }) },
      data: { siKey: `coa-field-${key}` },
    });
    select.append(el('option', { text: isName ? t('coaimport.text_after_number') : t('coaimport.no_column'), attrs: { value: '' } }));
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
      ui.top.replaceChildren(el('p', { class: 'modal-message', text: t('coaimport.budget_file') }));
      return;
    }
    if (picked.sheets.length > 1) {
      const sheet = el('select', { class: 'modal-input', data: { siKey: 'coa-sheet' } });
      picked.sheets.forEach((s, i) => {
        sheet.append(el('option', { text: s.hidden ? t('coaimport.sheet_option_hidden', { name: s.name, count: s.rows }) : t('coaimport.sheet_option', { name: s.name, count: s.rows }), attrs: { value: String(i) } }));
      });
      sheet.value = String(st.sheet);
      sheet.addEventListener('change', () => {
        st.sheet = Number(sheet.value);
        startSheet();
      });
      parts.push(el('label', { class: 'modal-field si-field' }, el('span', { text: t('coaimport.sheet') }), sheet));
    }
    const header = el('select', { class: 'modal-input', data: { siKey: 'coa-header' } });
    header.append(el('option', { text: t('coaimport.no_header_row'), attrs: { value: '' } }));
    for (const r of p ? p.raw : []) {
      const cells = r.cells.filter((c) => c.trim() !== '').slice(0, 5).join(' | ');
      header.append(el('option', { text: t('coaimport.header_option', { row: r.row, cells: cells || t('coaimport.empty_row') }), attrs: { value: String(r.row - 1) } }));
    }
    header.value = st.headerRow === null ? '' : String(st.headerRow);
    header.addEventListener('change', () => {
      st.headerRow = header.value === '' ? null : Number(header.value);
      refresh({ guess: true });
    });
    parts.push(el('label', { class: 'modal-field si-field' }, el('span', { text: t('coaimport.header_row') }), header));
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
        el('p', { class: 'si-note', text: t('coaimport.kinded') }),
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
    const sep = el('select', { class: 'modal-input', attrs: { 'aria-label': t('coaimport.separator') }, data: { siKey: 'coa-sep' } });
    for (const [v, label] of SEPARATORS) sep.append(el('option', { text: label, attrs: { value: v } }));
    sep.value = c.separator || '';
    sep.addEventListener('change', () => {
      st.coding = { ...(st.coding || p.coding), separator: sep.value };
      schedule();
    });
    const parts = [
      el('p', {
        class: 'modal-message',
        text: st.coding ? t('coaimport.pattern_changed', { found: codingText(p.inferred), chosen: codingText(st.coding) }) : t('coaimport.pattern_found', { found: codingText(p.inferred) }),
      }),
      el('div', { class: 'coa-coding' }, width('category', t('coaimport.width.category'), 1), width('account', t('coaimport.width.account'), 1), width('sub', t('coaimport.width.sub'), 0), el('label', { class: 'si-inline' }, el('span', { text: t('coaimport.separator') }), sep)),
    ];
    if (st.coding) {
      const reset = el('button', { class: 'btn btn-quiet', text: t('coaimport.use_file_pattern'), attrs: { type: 'button' } });
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
        check('coa-subs', st.subAccounts, t('coaimport.option.keep_subs', { count: p.sub_accounts }), (v) => {
          st.subAccounts = v;
          schedule();
        }),
      );
    }
    if (p && !p.levels_from_source) {
      parts.push(
        check('coa-levels', st.defaultLevels, t('coaimport.option.default_levels'), (v) => {
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
        el('span', { text: t('coaimport.option.import_into') }),
        radio('new', t('coaimport.option.target_new')),
        radio('current', t('coaimport.option.target_current')),
      ),
    );
    if (st.target === 'current') {
      parts.push(
        check('coa-remove', st.removeUnlisted, t('coaimport.option.remove_unlisted'), (v) => {
          st.removeUnlisted = v;
          schedule();
        }),
      );
    }
    ui.options.replaceChildren(...parts);
  };

  // `what`: the heading, with the count in it.
  const noteList = (items, count, what, open = false) => {
    if (!count) return null;
    const list = el('ul', { class: 'si-notes' }, ...items.map((n) => el('li', { text: n.row ? t('coaimport.row_note', { row: n.row, message: n.message }) : n.message })));
    if (count > items.length) list.append(el('li', { class: 'si-more', text: t('coaimport.and_more', { count: count - items.length }) }));
    return el('details', { class: 'si-details', attrs: open ? { open: true } : {} }, el('summary', { text: what }), list);
  };

  const textList = (items, count, what, open = false) => {
    if (!count) return null;
    const list = el('ul', { class: 'si-notes' }, ...items.map((item) => el('li', { text: item })));
    if (count > items.length) list.append(el('li', { class: 'si-more', text: t('coaimport.and_more', { count: count - items.length }) }));
    return el('details', { class: 'si-details', attrs: open ? { open: true } : {} }, el('summary', { text: what }), list);
  };

  const renderPreview = () => {
    const p = st.failed ? null : st.preview;
    if (!p) {
      ui.summary.replaceChildren();
      ui.rows.replaceChildren();
      ui.details.replaceChildren();
      return;
    }
    const sentences = [
      t('coaimport.summary', {
        categories: t('coaimport.n_categories', { count: p.categories }),
        accounts: t('coaimport.n_accounts', { count: p.accounts }),
        levels: t('coaimport.n_levels', { count: p.levels }),
        pattern: codingText(p.coding),
      }),
    ];
    if (p.skipped_count) sentences.push(t('coaimport.rows_skipped', { count: p.skipped_count }));
    if (p.totals_skipped) sentences.push(t('coaimport.total_rows_left_out', { count: p.totals_skipped }));
    const summary = [el('p', { class: 'si-summary-main', text: sentences.join(' ') })];
    if (p.default_levels_added) summary.push(el('p', { class: 'si-note', text: t('coaimport.levels_added') }));
    if (p.renumbered_count) summary.push(el('p', { class: 'si-note', text: t('coaimport.renumbered_note', { count: p.renumbered_count }) }));
    if (p.merged_repeats) summary.push(el('p', { class: 'si-note', text: t('coaimport.merged_note', { count: p.merged_repeats }) }));
    if (p.placeholders) summary.push(el('p', { class: 'si-note', text: t('coaimport.placeholders_note', { count: p.placeholders }) }));
    if (p.truncated) summary.push(el('p', { class: 'si-note', text: t('coaimport.truncated') }));
    ui.summary.replaceChildren(...summary);

    const body = p.rows.map((r) =>
      el(
        'tr',
        { class: `coa-${r.kind}` },
        el('td', { class: 'coa-code', text: r.code }),
        el('td', { class: 'si-desc', text: r.name, title: r.name }),
      ),
    );
    const table = el('table', { class: 'si-table coa-table' }, el('thead', {}, el('tr', {}, el('th', { text: t('coaimport.col.number') }), el('th', { text: t('coaimport.col.name') }))), el('tbody', {}, ...body));
    const total = p.categories + p.accounts + p.levels;
    const more = total > p.rows.length ? el('p', { class: 'si-more', text: t('coaimport.first_of', { shown: p.rows.length, count: total }) }) : null;
    ui.rows.replaceChildren(...[el('div', { class: 'si-table-wrap' }, table), more].filter(Boolean));

    const parts = [
      textList(p.renumbered, p.renumbered_count, t('coaimport.list.renumbered', { count: p.renumbered_count }), true),
      noteList(p.warnings, p.warning_count, t('coaimport.list.to_check', { count: p.warning_count }), true),
      noteList(p.skipped, p.skipped_count, t('coaimport.list.skipped', { count: p.skipped_count })),
    ];
    const a = p.apply;
    if (a) {
      const adding = a.added_category_count + a.added_account_count;
      parts.push(
        textList(
          [...a.added_categories.map((c) => t('coaimport.category_item', { code: c })), ...a.added_accounts.map((x) => t('coaimport.account_item', { code: x }))],
          adding,
          t('coaimport.list.to_add', { count: adding }),
        ),
        textList(a.renamed, a.renamed_count, t('coaimport.list.to_rename', { count: a.renamed_count })),
        textList(a.kept, a.kept_count, t('coaimport.list.kept', { count: a.kept_count }), true),
        textList(
          a.unlisted_empty,
          a.unlisted_empty_count,
          st.removeUnlisted ? t('coaimport.list.empty_remove', { count: a.unlisted_empty_count }) : t('coaimport.list.empty_kept', { count: a.unlisted_empty_count }),
        ),
        textList(a.not_fitting, a.not_fitting.length, t('coaimport.list.not_fitting', { count: a.not_fitting.length }), true),
      );
    }
    ui.details.replaceChildren(...parts.filter(Boolean));
    if (a) {
      const n = a.added_category_count + a.added_account_count + a.renamed_count;
      ui.summary.append(
        el('p', {
          class: 'modal-message',
          text: n
            ? t('coaimport.apply_summary', {
                categories: t('coaimport.n_categories', { count: a.added_category_count }),
                accounts: t('coaimport.n_accounts', { count: a.added_account_count }),
                renamed: a.renamed_count,
                unchanged: a.unchanged_accounts,
              })
            : t('coaimport.apply_nothing'),
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
      const title = el('h2', { class: 'modal-title', text: t('coaimport.title', { file: picked.file }), attrs: { id: 'coa-title' } });
      const intro = el('p', {
        class: 'modal-message',
        text: t('coaimport.intro'),
      });
      ui.top = el('div', { class: 'si-top' });
      ui.mapHead = el('h3', { class: 'si-heading', text: t('coaimport.columns') });
      ui.mapping = el('div', { class: 'si-map' });
      ui.coding = el('div', { class: 'coa-coding-wrap' });
      ui.options = el('div', { class: 'si-options' });
      ui.summary = el('div', { class: 'si-summary', attrs: { 'aria-live': 'polite' } });
      ui.rows = el('div', { class: 'si-lines' });
      ui.details = el('div', { class: 'si-detail-list' });
      ui.error = el('p', { class: 'modal-error', attrs: { role: 'alert' } });
      ui.error.hidden = true;
      const cancel = el('button', { class: 'btn', text: t('modal.cancel'), attrs: { type: 'button', 'data-autofocus': true } });
      ui.ok = el('button', { class: 'btn btn-primary', text: t('coaimport.import_chart'), attrs: { type: 'button' } });
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
        el('h3', { class: 'si-heading', text: t('coaimport.account_numbers') }),
        ui.coding,
        el('h3', { class: 'si-heading', text: t('coaimport.options') }),
        ui.options,
        el('h3', { class: 'si-heading', text: t('coaimport.preview') }),
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
