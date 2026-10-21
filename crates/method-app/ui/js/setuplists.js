// Setup lists in spreadsheets (src/setup_sheets.rs; decisions.md, entry
// 50), from the main window's File menu:
//
// - File > Export > Setup Lists…: which kinds of setup item, as an Excel
//   workbook (a sheet per kind) or CSV files (one per kind); Rust's Save
//   dialog.
// - File > Import Setup Lists…: Rust's Open dialog, then what each sheet
//   holds and which column is which (guessed from the titles, as Import
//   Spreadsheet does); every row is checked and each problem listed with
//   its row, nothing imported while any is left; then Import Setup's review
//   (importsetup.js), one undo step. Into a pack: open the pack as a tab.
//
// Text from files only reaches the page through textContent (dom.js el()).
// `ctx`: { call(cmd, args), toast(message), focus(), apply(view) }.

import { el } from './dom.js';
import { customDialog } from './modal.js';
import { showReview } from './importsetup.js';
import { t } from './i18n.js';

function str(v) {
  return typeof v === 'string' ? v : '';
}

let uid = 0;
function nextId(prefix) {
  uid += 1;
  return `${prefix}-${uid}`;
}

/** The kinds a spreadsheet holds: [kind, label]. */
export const SHEET_KINDS = [
  ['global', t('setup.sheet_kind.global')],
  ['fringe', t('setup.sheet_kind.fringe')],
  ['group', t('setup.sheet_kind.group')],
  ['unit', t('setup.sheet_kind.unit')],
  ['set', t('setup.sheet_kind.set')],
  ['location', t('setup.sheet_kind.location')],
  ['currency', t('setup.sheet_kind.currency')],
  ['charge', t('setup.sheet_kind.charge')],
  ['credit', t('setup.sheet_kind.credit')],
  ['tag_preset', t('setup.sheet_kind.tag_preset')],
  ['ot_pattern', t('setup.sheet_kind.ot_pattern')],
  ['rate_table', t('setup.sheet_kind.rate_table')],
];

/** File > Export > Setup Lists…: which kinds, as a workbook or CSV files. */
export function exportSetupLists(ctx) {
  const boxes = new Map();
  customDialog(
    (closeDialog) => {
      const titleId = nextId('pk-sheet-title');
      const kinds = el('div', { class: 'pk-kinds' });
      for (const [kind, label] of SHEET_KINDS) {
        const box = el('input', { attrs: { type: 'checkbox' } });
        box.checked = true;
        boxes.set(kind, box);
        kinds.append(el('label', { class: 'rs-check' }, box, el('span', { text: label })));
      }
      const name = nextId('pk-format');
      const xlsx = el('input', { attrs: { type: 'radio', name, value: 'xlsx' } });
      xlsx.checked = true;
      const csv = el('input', { attrs: { type: 'radio', name, value: 'csv' } });
      const error = el('p', { class: 'modal-error', attrs: { role: 'alert' } });
      error.hidden = true;
      const cancel = el('button', { class: 'btn', text: t('setup.cancel'), attrs: { type: 'button' } });
      const ok = el('button', { class: 'btn btn-primary', text: t('setup.export'), attrs: { type: 'button', 'data-autofocus': true } });
      cancel.addEventListener('click', () => closeDialog());
      ok.addEventListener('click', async () => {
        const picked = [...boxes].filter(([, b]) => b.checked).map(([k]) => k);
        if (!picked.length) {
          error.textContent = t('setup.tick_at_least_one_kind');
          error.hidden = false;
          return;
        }
        ok.disabled = true;
        try {
          const r = await ctx.call('setup_sheet_export', { kinds: picked, format: csv.checked ? 'csv' : 'xlsx' });
          if (r && r.done) {
            closeDialog();
            ctx.toast(r.message);
          } else ok.disabled = false;
        } catch (e) {
          error.textContent = String(e);
          error.hidden = false;
          ok.disabled = false;
        }
      });
      return el(
        'div',
        { class: 'modal pk-sheet-dialog', attrs: { role: 'dialog', 'aria-modal': 'true', 'aria-labelledby': titleId } },
        el('h2', { class: 'modal-title', text: t('setup.export_setup_lists'), attrs: { id: titleId } }),
        el('p', { class: 'modal-message', text: t('setup.each_kind_of_setup_item') }),
        kinds,
        el('div', { class: 'pk-formats' }, el('label', { class: 'rs-check' }, xlsx, el('span', { text: t('setup.spreadsheet_xlsx_a_sheet_per') })), el('label', { class: 'rs-check' }, csv, el('span', { text: t('setup.csv_files_one_per_kind') }))),
        error,
        el('div', { class: 'modal-actions' }, cancel, ok),
      );
    },
    { onClose: () => ctx.focus() },
  );
}

/** A problem list: "Fringes, row 4: ..." per line. */
export function problemLines(errors) {
  return (errors || []).map((e) => `${str(e.sheet)}${e.sheet ? ', ' : ''}row ${e.row}: ${str(e.message)}`);
}

/** File > Import Setup Lists…: the mapping dialog, then Import Setup's review. */
export async function importSetupLists(ctx) {
  let picked;
  try {
    picked = await ctx.call('setup_sheet_pick', {});
  } catch (e) {
    ctx.toast(e);
    ctx.focus();
    return;
  }
  if (!picked) {
    ctx.focus();
    return;
  }
  const kindCols = new Map((picked.kinds || []).map((k) => [k.kind, k.columns]));
  // Per sheet: { use, kind, mapping, titles }.
  const choice = new Map();
  for (const s of picked.sheets || []) choice.set(s.index, { use: Boolean(s.kind), kind: s.kind || '', mapping: s.mapping || { header_row: null, columns: {} }, titles: s.titles || [], name: s.name, rows: s.rows });
  let body = null;
  let problems = null;
  let ok = null;

  const mappingGrid = (c, index) => {
    const grid = el('div', { class: 'si-map' });
    for (const [key, title] of kindCols.get(c.kind) || []) {
      const sel = el('select', { class: 'modal-input si-col', attrs: { 'aria-label': t('setup.column_for', { name: c.name, title }) } });
      sel.append(el('option', { text: t('setup.none_paren'), attrs: { value: '' } }));
      c.titles.forEach((t, i) => sel.append(el('option', { text: t, attrs: { value: String(i) } })));
      const at = c.mapping.columns ? c.mapping.columns[key] : undefined;
      sel.value = at === undefined || at === null ? '' : String(at);
      sel.addEventListener('change', () => {
        if (sel.value === '') delete c.mapping.columns[key];
        else c.mapping.columns[key] = Number(sel.value);
      });
      grid.append(el('label', { class: 'si-map-row', title }, el('span', { class: 'si-map-label', text: title }), sel));
    }
    grid.dataset.sheet = String(index);
    return grid;
  };

  const draw = () => {
    const parts = [];
    for (const [index, c] of choice) {
      const use = el('input', { attrs: { type: 'checkbox', 'aria-label': t('setup.import_the_sheet', { name: c.name }) } });
      use.checked = c.use;
      use.addEventListener('change', () => {
        c.use = use.checked;
        draw();
      });
      const kind = el('select', { class: 'modal-input', attrs: { 'aria-label': t('setup.what_the_sheet_holds', { name: c.name }) } });
      kind.append(el('option', { text: t('setup.choose'), attrs: { value: '' } }));
      for (const [k, label] of SHEET_KINDS) kind.append(el('option', { text: label, attrs: { value: k } }));
      kind.value = c.kind;
      kind.addEventListener('change', async () => {
        c.kind = kind.value;
        if (!c.kind) {
          c.use = false;
          draw();
          return;
        }
        try {
          const g = await ctx.call('setup_sheet_guess', { token: picked.token, index, kind: c.kind });
          c.mapping = g.mapping;
          c.titles = g.titles;
          c.use = true;
        } catch (e) {
          ctx.toast(e);
        }
        draw();
      });
      const head = el('div', { class: 'pk-sheet-head' }, el('label', { class: 'rs-check' }, use, el('span', { text: t('setup.sheet_rows', { name: c.name, count: c.rows }) })), kind);
      parts.push(head);
      if (c.use && c.kind) {
        const hr = c.mapping.header_row;
        parts.push(el('p', { class: 'pk-sheet-note', text: hr === null || hr === undefined ? t('setup.no_title_row') : t('setup.title_row', { title: hr + 1, data: hr + 2 }) }));
        parts.push(mappingGrid(c, index));
      }
    }
    if (!choice.size) parts.push(el('p', { class: 'modal-message', text: t('setup.the_file_has_no_sheets') }));
    body.replaceChildren(...parts);
    ok.disabled = ![...choice.values()].some((c) => c.use && c.kind);
  };

  customDialog(
    (closeDialog) => {
      const titleId = nextId('pk-lists-title');
      body = el('div', { class: 'si-scroll pk-sheets' });
      problems = el('div', { class: 'pk-problems', attrs: { 'aria-live': 'polite' } });
      const cancel = el('button', { class: 'btn', text: t('setup.cancel'), attrs: { type: 'button', 'data-autofocus': true } });
      ok = el('button', { class: 'btn btn-primary', text: t('setup.check_and_review'), attrs: { type: 'button' } });
      cancel.addEventListener('click', () => closeDialog());
      ok.addEventListener('click', async () => {
        const choices = [...choice].filter(([, c]) => c.use && c.kind).map(([index, c]) => ({ index, kind: c.kind, mapping: { header_row: c.mapping.header_row ?? null, columns: c.mapping.columns || {} } }));
        ok.disabled = true;
        try {
          const r = await ctx.call('setup_sheet_review', { token: picked.token, choices });
          if (r.errors && r.errors.length) {
            const lines = problemLines(r.errors);
            const shown = lines.slice(0, 200);
            problems.replaceChildren(
              el('p', { class: 'modal-error', text: t('setup.problems_to_fix', { count: lines.length }), attrs: { role: 'alert' } }),
              el('ul', { class: 'si-notes' }, ...shown.map((line) => el('li', { text: line })), lines.length > shown.length ? el('li', { class: 'si-more', text: t('setup.and_more', { count: lines.length - shown.length }) }) : null),
            );
            ok.disabled = false;
            return;
          }
          closeDialog();
          const reviewCtx = { invoke: (cmd, args) => ctx.call(cmd, args), apply: (v) => ctx.apply && ctx.apply(v), toast: ctx.toast, focus: ctx.focus };
          showReview(reviewCtx, r.preview);
        } catch (e) {
          problems.replaceChildren(el('p', { class: 'modal-error', text: String(e), attrs: { role: 'alert' } }));
          ok.disabled = false;
        }
      });
      return el(
        'div',
        { class: 'modal si-dialog pk-lists-dialog', attrs: { role: 'dialog', 'aria-modal': 'true', 'aria-labelledby': titleId } },
        el('h2', { class: 'modal-title', text: t('setup.import_lists_title', { file: str(picked.file) }), attrs: { id: titleId } }),
        el('p', { class: 'modal-message', text: t('setup.say_what_each_sheet_holds') }),
        body,
        problems,
        el('div', { class: 'modal-actions' }, cancel, ok),
      );
    },
    { onClose: () => ctx.focus() },
  );
  draw();
}
