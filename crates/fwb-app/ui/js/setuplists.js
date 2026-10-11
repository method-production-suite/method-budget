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
  ['global', 'Globals'],
  ['fringe', 'Fringes'],
  ['group', 'Groups'],
  ['unit', 'Units'],
  ['set', 'Sets'],
  ['location', 'Locations'],
  ['currency', 'Currencies'],
  ['charge', 'Charges'],
  ['credit', 'Credits'],
  ['tag_preset', 'Tag presets'],
  ['ot_pattern', 'Overtime patterns'],
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
      const cancel = el('button', { class: 'btn', text: 'Cancel', attrs: { type: 'button' } });
      const ok = el('button', { class: 'btn btn-primary', text: 'Export…', attrs: { type: 'button', 'data-autofocus': true } });
      cancel.addEventListener('click', () => closeDialog());
      ok.addEventListener('click', async () => {
        const picked = [...boxes].filter(([, b]) => b.checked).map(([k]) => k);
        if (!picked.length) {
          error.textContent = 'Tick at least one kind.';
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
        el('h2', { class: 'modal-title', text: 'Export Setup Lists', attrs: { id: titleId } }),
        el('p', { class: 'modal-message', text: "Each kind of setup item as a table: one row per item with its whole definition and where it came from, to edit in a spreadsheet and import back. A charge's excluded amounts stay in the budget." }),
        kinds,
        el('div', { class: 'pk-formats' }, el('label', { class: 'rs-check' }, xlsx, el('span', { text: 'Excel workbook (a sheet per kind)' })), el('label', { class: 'rs-check' }, csv, el('span', { text: 'CSV files (one per kind)' }))),
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
      const sel = el('select', { class: 'modal-input si-col', attrs: { 'aria-label': `${c.name}: column for ${title}` } });
      sel.append(el('option', { text: '(none)', attrs: { value: '' } }));
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
      const use = el('input', { attrs: { type: 'checkbox', 'aria-label': `Import the sheet ${c.name}` } });
      use.checked = c.use;
      use.addEventListener('change', () => {
        c.use = use.checked;
        draw();
      });
      const kind = el('select', { class: 'modal-input', attrs: { 'aria-label': `What the sheet ${c.name} holds` } });
      kind.append(el('option', { text: 'Choose…', attrs: { value: '' } }));
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
      const head = el('div', { class: 'pk-sheet-head' }, el('label', { class: 'rs-check' }, use, el('span', { text: `${c.name} (${c.rows} ${c.rows === 1 ? 'row' : 'rows'})` })), kind);
      parts.push(head);
      if (c.use && c.kind) {
        const hr = c.mapping.header_row;
        parts.push(el('p', { class: 'pk-sheet-note', text: hr === null || hr === undefined ? 'No row of titles: data from the first row.' : `Titles in row ${hr + 1}; data from row ${hr + 2}.` }));
        parts.push(mappingGrid(c, index));
      }
    }
    if (!choice.size) parts.push(el('p', { class: 'modal-message', text: 'The file has no sheets to read.' }));
    body.replaceChildren(...parts);
    ok.disabled = ![...choice.values()].some((c) => c.use && c.kind);
  };

  customDialog(
    (closeDialog) => {
      const titleId = nextId('pk-lists-title');
      body = el('div', { class: 'si-scroll pk-sheets' });
      problems = el('div', { class: 'pk-problems', attrs: { 'aria-live': 'polite' } });
      const cancel = el('button', { class: 'btn', text: 'Cancel', attrs: { type: 'button', 'data-autofocus': true } });
      ok = el('button', { class: 'btn btn-primary', text: 'Check and Review…', attrs: { type: 'button' } });
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
              el('p', { class: 'modal-error', text: `${lines.length} ${lines.length === 1 ? 'problem' : 'problems'} to fix in the sheet (nothing was imported):`, attrs: { role: 'alert' } }),
              el('ul', { class: 'si-notes' }, ...shown.map((t) => el('li', { text: t })), lines.length > shown.length ? el('li', { class: 'si-more', text: `and ${lines.length - shown.length} more` }) : null),
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
        el('h2', { class: 'modal-title', text: `Import setup lists: ${str(picked.file)}`, attrs: { id: titleId } }),
        el('p', { class: 'modal-message', text: 'Say what each sheet holds and which column is which. Every row is checked first; then you choose what to bring in, as with Import Setup.' }),
        body,
        problems,
        el('div', { class: 'modal-actions' }, cancel, ok),
      );
    },
    { onClose: () => ctx.focus() },
  );
  draw();
}
