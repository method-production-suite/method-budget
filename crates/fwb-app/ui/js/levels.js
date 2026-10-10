// Turns a View (the budget's outline and totals, plus the account on screen in
// full) into grid columns and rows for each of the three levels. Pure
// functions: no DOM, no commands.

import { decimal } from './format.js';
import { tagCellText, notesTitle } from './order.js';
import { totalText, numberCell, budgetDisplay, hiddenFactors } from './display.js';

// Totals with the user's choice of decimals (display.js; 2 unless they chose
// whole amounts). Never changes a value.
const money = totalText;

const NA = { text: '', placeholder: 'n/a', cls: 'na' };

/** Looks things up across the budget and report once per view. */
export function indexView(view) {
  const b = view.budget || {};
  const r = view.report;
  const topsheet = Array.isArray(b.topsheet) ? b.topsheet : [];
  const cats = []; // { ti, ci, cat, rep }
  const levels = []; // { ti, li, row, rep }
  let ci = 0;
  let li = 0;
  topsheet.forEach((row, ti) => {
    if (row.kind === 'production_total') {
      levels.push({ ti, li, row, rep: r ? r.levels[li] : null });
      li += 1;
    } else {
      cats.push({ ti, ci, cat: row, rep: r ? r.categories[ci] : null });
      ci += 1;
    }
  });

  // Problems on detail cells (the view lists the ones in the account on
  // screen): row id -> { field: message }.
  const cellIssues = new Map();
  for (const issue of view.issues || []) {
    const t = issue.target || {};
    if (t.kind !== 'cell') continue;
    const m = cellIssues.get(t.row_id) || {};
    m[t.field] = m[t.field] ? `${m[t.field]}; ${issue.message}` : issue.message;
    cellIssues.set(t.row_id, m);
  }
  // Accounts with a problem cell anywhere, by topsheet and account index.
  const badAccounts = new Set();
  const badCategories = new Set();
  for (const at of Array.isArray(view.issue_accounts) ? view.issue_accounts : []) {
    if (!Array.isArray(at) || at.length !== 2) continue;
    badAccounts.add(`${at[0]}:${at[1]}`);
    badCategories.add(at[0]);
  }
  const accountHasIssue = (ti, ai) => badAccounts.has(`${ti}:${ai}`);
  const categoryHasIssue = (ti) => badCategories.has(ti);

  // Original (baseline) vs current, when the budget has a baseline: every
  // category and account, and the rows of the account on screen.
  const variance = view.variance && typeof view.variance === 'object' ? view.variance : null;

  return { b, r, topsheet, cats, levels, cellIssues, accountHasIssue, categoryHasIssue, variance };
}

// ---------------------------------------------------------------------------
// Original and variance columns (Orig, Var): on every level when the budget
// has a baseline (view.variance). Shown until the user hides them from the
// column menu; never without a baseline.

/** The Orig and Var cells for one compared figure ({ original, current, variance }). */
export function varianceCells(ch) {
  if (!ch || typeof ch !== 'object') return {};
  const orig =
    ch.original === null || ch.original === undefined
      ? { text: '', placeholder: 'new', cls: 'muted', title: 'Added since the baseline was locked' }
      : { text: money(ch.original), title: 'When the baseline was locked' };
  const diff = ch.variance === null || ch.variance === undefined ? { ...NA } : { text: money(ch.variance), title: 'Change since the baseline was locked' };
  return { orig, var: diff };
}

/** The category entry for a topsheet index, or null. */
export function categoryAt(ix, ti) {
  return ix.cats.find((c) => c.ti === ti) || null;
}

/** Finds a category by code, preferring the one at `ti` when codes repeat. */
export function findCategory(ix, code, ti) {
  const same = ix.cats.filter((c) => c.cat.code === code);
  return same.find((c) => c.ti === ti) || same[0] || null;
}

/**
 * Every account in topsheet order, across categories:
 * [{ catCode, ti, code, ai }] (ai is the account's index in its category).
 */
export function accountList(ix) {
  const out = [];
  for (const c of ix.cats) {
    (c.cat.accounts || []).forEach((a, ai) => out.push({ catCode: c.cat.code, ti: c.ti, code: a.code, ai }));
  }
  return out;
}

function moneyCell(cents, extra = {}) {
  if (cents === null || cents === undefined) return { ...NA, ...extra };
  return { text: money(cents), ...extra };
}

function staticRow(key, cls, label, total, extra = {}) {
  return {
    key,
    cls,
    selectable: false,
    cells: { name: { text: label }, total: moneyCell(total), ...extra },
  };
}

const PROBLEM = 'Some cells inside have problems; totals count them as blank.';

/**
 * Whether a category or account row may be hidden as zero-balance (View >
 * Show or Hide Zero Accounts): it totals 0, has no problem cells, and wasn't
 * different when the baseline was locked.
 */
function isZero(total, bad, change) {
  if (total !== 0 || bad) return false;
  return !(change && typeof change.original === 'number' && change.original !== 0);
}

// ---------------------------------------------------------------------------
// Topsheet

// Double-clicking a code or name edits it in place; elsewhere on a category's
// row, double-click opens the category (grid.js editOnDoubleClick).
export const TOPSHEET_COLUMNS = [
  { key: 'code', label: 'Acct #', editOnDoubleClick: true },
  { key: 'name', label: 'Description', editOnDoubleClick: true },
  { key: 'total', label: 'Total', cls: 'num' },
];

/** Columns with the shown Orig and Var columns put before Total. */
function withBaselineColumns(cols, choices, ctx) {
  const extra = BASELINE_COLUMNS.filter((c) => columnShown(c, choices, ctx)).map(({ key, label, cls, title }) => ({ key, label, cls, title }));
  if (!extra.length) return cols;
  const at = cols.findIndex((c) => c.key === 'total');
  return [...cols.slice(0, at), ...extra, ...cols.slice(at)];
}

/** Topsheet columns, with Orig and Var when the budget has a baseline (and they aren't hidden). */
export function topsheetColumns(ix, choices) {
  return withBaselineColumns(TOPSHEET_COLUMNS, choices, { baseline: Boolean(ix.variance) });
}

export function topsheetRows(ix) {
  const r = ix.r;
  const rows = [];
  for (const [ti, row] of ix.topsheet.entries()) {
    if (row.kind === 'production_total') {
      const lv = ix.levels.find((l) => l.ti === ti);
      if (lv && lv.rep && lv.rep.posted_fringes) {
        rows.push(staticRow(`f${ti}`, 'posted', 'Fringes', lv.rep.posted_fringes));
      }
      rows.push({
        key: `t${ti}`,
        cls: 'level',
        ti,
        kind: 'level',
        cells: {
          code: {},
          name: { text: row.name, edit: row.name },
          total: lv && lv.rep ? moneyCell(lv.rep.total) : NA,
        },
      });
      continue;
    }
    const c = ix.cats.find((x) => x.ti === ti);
    const bad = ix.categoryHasIssue(ti);
    rows.push({
      key: `t${ti}`,
      cls: 'category',
      ti,
      kind: 'category',
      activatable: true,
      cells: {
        code: { text: row.code, edit: row.code },
        name: noted({ text: row.name, edit: row.name }, row.notes),
        total: c && c.rep ? moneyCell(c.rep.total, bad ? { error: PROBLEM } : {}) : NA,
        ...(ix.variance ? varianceCells(ix.variance.categories[row.code]) : {}),
      },
      zero: Boolean(c && c.rep) && isZero(c.rep.total, bad, ix.variance && ix.variance.categories[row.code]),
    });
  }
  if (!r) return rows;

  // The budget's display settings can leave out the summary rows and the
  // net total (Budget > Settings). Display only.
  const disp = budgetDisplay(ix.b);
  if (disp.summary_rows) {
    rows.push(staticRow('s-atl', 'summary first', 'Total above-the-line', r.total_atl));
    rows.push(staticRow('s-btl', 'summary', 'Total below-the-line', r.total_btl));
    rows.push(staticRow('s-atlbtl', 'summary', 'Total above and below-the-line', r.atl_btl));
  }
  if (r.budget_fringes) rows.push(staticRow('s-fr', disp.summary_rows ? 'summary' : 'summary first', 'Total fringes', r.budget_fringes));
  r.charges.forEach((c, i) => {
    rows.push(staticRow(`s-ch${i}`, 'summary charge', c.name, c.amount));
  });
  rows.push(staticRow('s-grand', 'summary grand', 'Grand total', r.grand_total, ix.variance ? varianceCells(ix.variance.grand_total) : {}));
  r.credits.forEach((c, i) => {
    rows.push(staticRow(`s-cr${i}`, 'summary credit', `Less ${c.name}`, -c.amount));
  });
  if (r.credits.length && disp.net_total) rows.push(staticRow('s-net', 'summary grand', 'Net total', r.net_total, ix.variance ? varianceCells(ix.variance.net_total) : {}));
  if (r.display && disp.net_total) {
    const cur = r.totals_currency;
    rows.push(staticRow('s-disp', 'summary', `Net total in ${cur}`, r.display.net_total));
  }
  return rows;
}

// ---------------------------------------------------------------------------
// Accounts in one category

export const ACCOUNT_COLUMNS = [
  { key: 'code', label: 'Acct #', editOnDoubleClick: true },
  { key: 'name', label: 'Description', editOnDoubleClick: true },
  { key: 'fringe_total', label: 'Fringes', cls: 'num' },
  { key: 'total', label: 'Total', cls: 'num' },
];

const FRINGE_POSTED = {
  account: 'Fringes earned by lines in this account; they post to the account, inside its total',
  category: "Fringes earned by lines in this account; they post to the category's fringe account",
  production_level: 'Fringes earned by lines in this account; they post above the production total',
  budget: "Fringes earned by lines in this account; they post to the budget's total fringes",
};

/**
 * Account-list columns; the Fringes header says where the fringes post. Orig
 * and Var come before Total when the budget has a baseline (and `choices`
 * doesn't hide them).
 */
export function accountColumns(ix, choices) {
  const title = FRINGE_POSTED[postingKind(ix)] || FRINGE_POSTED.category;
  const cols = ACCOUNT_COLUMNS.map((c) => (c.key === 'fringe_total' ? { ...c, title } : c));
  return withBaselineColumns(cols, choices, { baseline: Boolean(ix.variance) });
}

/** How fringes post: 'budget', 'production_level', 'category' or 'account'. */
export function postingKind(ix) {
  return ix.b.fringe_posting ? ix.b.fringe_posting.kind : 'category';
}

export function accountRows(ix, entry) {
  const accounts = entry.cat.accounts || [];
  const rep = entry.rep;
  // Posting by account puts each account's fringes in its total, so the
  // column reads as part of the sum rather than as a side note.
  const frCls = postingKind(ix) === 'account' ? '' : 'muted';
  const rows = accounts.map((a, ai) => {
    const ar = rep ? rep.accounts[ai] : null;
    const bad = ix.accountHasIssue(entry.ti, ai);
    return {
      key: `a${ai}`,
      code: a.code,
      ai,
      cls: 'account',
      activatable: true,
      cells: {
        code: { text: a.code, edit: a.code },
        name: noted({ text: a.name, edit: a.name }, a.notes),
        fringe_total: ar ? { text: ar.fringe_total ? money(ar.fringe_total) : '', cls: frCls } : NA,
        total: ar ? moneyCell(ar.total, bad ? { error: PROBLEM } : {}) : NA,
        ...(ix.variance ? varianceCells(ix.variance.accounts[a.code]) : {}),
      },
      zero: Boolean(ar) && isZero(ar.total, bad, ix.variance && ix.variance.accounts[a.code]),
    };
  });
  // Posting by category: the category's fringe account, read-only, before the total.
  if (rep && rep.fringe_account) {
    rows.push({
      key: 'fa',
      cls: 'fringe-account',
      selectable: false,
      cells: {
        code: { text: rep.fringe_account.code },
        name: { text: 'Fringes', title: 'Fringes earned by every account in this category' },
        total: moneyCell(rep.fringe_account.amount),
      },
    });
  }
  rows.push({
    key: 'cat-total',
    cls: 'summary first grand',
    selectable: false,
    cells: {
      name: { text: `Total ${entry.cat.name || entry.cat.code}` },
      fringe_total: rep ? { text: money(rep.fringe_total), cls: frCls } : NA,
      total: rep ? moneyCell(rep.total) : NA,
      ...(ix.variance ? varianceCells(ix.variance.categories[entry.cat.code]) : {}),
    },
  });
  return rows;
}

// ---------------------------------------------------------------------------
// Detail rows in one account

// Every detail column, in display order. width is the preferred width and min
// the narrowest it gets when the window is short of room (grid.js shrinks
// columns before it scrolls sideways); grow takes spare room first. pin keeps
// a column in view while the grid scrolls sideways. optional columns can be
// hidden from the grid's column menu (listed by `name`, else `label`); `on` is
// whether they show until the user chooses.
export const DETAIL_COLUMNS = [
  { key: 'num', label: '#', cls: 'num', width: 44, min: 44, pin: 'left' },
  { key: 'description', label: 'Description', width: 240, min: 130, pin: 'left', grow: true },
  { key: 'amount', label: 'Amount', cls: 'num', width: 110, min: 70 },
  { key: 'unit', label: 'Unit', width: 72, min: 50 },
  { key: 'x', label: 'X', name: 'X (multiplier)', cls: 'num', title: 'Multiplier (blank counts as 1)', width: 52, min: 40, optional: true, on: true },
  { key: 'unit2', label: 'X unit', title: 'Unit shown next to X', width: 72, min: 54, optional: true, on: false },
  { key: 'rate', label: 'Rate', cls: 'num', width: 100, min: 78 },
  { key: 'unit3', label: 'Rate unit', title: 'Unit shown next to the rate', width: 76, min: 54, optional: true, on: false },
  { key: 'currency', label: 'Cur', name: 'Currency', title: 'Currency (blank is the base currency)', width: 52, min: 42, optional: true, on: true },
  { key: 'x4', label: '4X', name: '4X (fourth factor)', cls: 'num', title: 'Fourth factor (blank counts as 1)', width: 52, min: 40, optional: true, on: false },
  { key: 'unit4', label: '4X unit', title: 'Unit shown next to 4X', width: 72, min: 54, optional: true, on: false },
  { key: 'fringes', label: 'Fringes', title: 'Fringes on this line, separated by commas', width: 140, min: 64, optional: true, on: true },
  { key: 'fringe_pct', label: 'Fringe %', cls: 'num', title: 'Fringe dollars as a percentage of the line total', width: 76, min: 70, optional: true, on: true },
  { key: 'fringe_total', label: 'Fringe $', cls: 'num', title: 'Fringe dollars this line earns', width: 104, min: 92, optional: true, on: true },
  { key: 'groups', label: 'Groups', title: 'Groups, separated by commas', width: 130, min: 60, optional: true, on: true },
  { key: 'set', label: 'Set', width: 90, min: 60, optional: true, on: false },
  { key: 'location', label: 'Location', width: 110, min: 70, optional: true, on: false },
  // With a baseline only (`baseline: true`), shown by default then.
  { key: 'orig', label: 'Orig', name: 'Orig (original total)', cls: 'num', title: 'The total when the baseline was locked', width: 120, min: 96, optional: true, on: 'baseline', baseline: true },
  { key: 'var', label: 'Var', name: 'Var (change since the baseline)', cls: 'num', title: 'How far the total has moved since the baseline was locked', width: 120, min: 96, optional: true, on: 'baseline', baseline: true },
  { key: 'total', label: 'Total', cls: 'num', width: 130, min: 116, pin: 'right' },
];

/** The optional detail columns, for the column menu. */
export const OPTIONAL_DETAIL_COLUMNS = DETAIL_COLUMNS.filter((c) => c.optional);

/** The Orig and Var columns, which every level can show when the budget has a baseline. */
export const BASELINE_COLUMNS = DETAIL_COLUMNS.filter((c) => c.baseline);

/**
 * Whether an optional column shows, given saved choices ({ key: boolean })
 * and `ctx.baseline` (the budget has a baseline). Orig and Var never show
 * without one, and show with one unless the user hid them.
 */
export function columnShown(col, choices, ctx = {}) {
  if (!col.optional) return true;
  if (col.baseline && !ctx.baseline) return false;
  const v = choices ? choices[col.key] : undefined;
  if (typeof v === 'boolean') return v;
  return col.on === 'baseline' ? Boolean(ctx.baseline) : col.on;
}

/** The detail columns to draw, given saved choices (and `ctx.baseline`). */
export function detailColumns(choices, ctx = {}) {
  return DETAIL_COLUMNS.filter((c) => columnShown(c, choices, ctx));
}

/**
 * A formula cell: plain numbers as typed (rates with the budget's rate
 * decimals), formulas by their value unless a global in them is shown by
 * name (display.js numberCell). `ctx` has the field, the row's evaluated
 * values and the budget's globals, global info and display settings.
 */
function formulaCell(f, issue, ctx = {}) {
  return numberCell(f, { ...ctx, value: ctx.values ? ctx.values[ctx.field] : undefined, issue });
}

/** A cell with a note mark and the notes as its tooltip, when there are notes. */
function noted(cell, notes) {
  const title = notesTitle(notes);
  return title ? { ...cell, cls: cell.cls ? `${cell.cls} has-note` : 'has-note', title } : cell;
}

/**
 * A line's fringes or groups: short codes shown where the items have them
 * (full names in the tooltip), names when editing or copying.
 */
function tagCell(b, table, list, issue) {
  const t = tagCellText(b, table, list);
  const cell = { text: t.text, edit: t.edit };
  if (t.title) cell.title = t.title;
  if (issue) cell.error = issue;
  return cell;
}

function textCell(v, issue) {
  const s = v === null || v === undefined ? '' : String(v);
  const cell = { text: s, edit: s };
  if (issue) cell.error = issue;
  return cell;
}

const HEX_COLOR = /^#(?:[0-9a-f]{3}|[0-9a-f]{6})$/i;

/**
 * The text color for a detail row: its own color, else the first of its groups
 * (by name) that has one. Colors are `#rgb` or `#rrggbb`; anything else is
 * ignored. Null for the default color.
 */
export function rowColor(d, groups) {
  if (typeof d.color === 'string' && HEX_COLOR.test(d.color)) return d.color;
  for (const g of d.groups || []) {
    const c = groups && groups[g] ? groups[g].color : null;
    if (typeof c === 'string' && HEX_COLOR.test(c)) return c;
  }
  return null;
}

/**
 * Where each row sits in a fringe range, by row index: 'single', 'start',
 * 'mid' or 'end' (null outside one). Mirrors the engine: a range is a run of
 * counted lines and adjustments sharing `fringe_range`; subtotal and separator
 * rows, and rows that don't count, never break a run, and the bracket spans
 * across them.
 */
export function fringeRangeMarks(details, rows) {
  const marks = new Array(details.length).fill(null);
  let run = null; // { n, first, last }
  const close = () => {
    if (!run) return;
    if (run.first === run.last) marks[run.first] = 'single';
    else {
      for (let i = run.first; i <= run.last; i++) marks[i] = 'mid';
      marks[run.first] = 'start';
      marks[run.last] = 'end';
    }
    run = null;
  };
  details.forEach((d, i) => {
    const kind = d.kind || 'line';
    const costed = kind === 'line' || kind === 'adjustment';
    const counted = !rows || !rows[i] || rows[i].included;
    if (!costed || !counted) return;
    const n = typeof d.fringe_range === 'number' ? d.fringe_range : null;
    if (run && n !== null && n === run.n) {
      run.last = i;
      return;
    }
    close();
    if (n !== null) run = { n, first: i, last: i };
  });
  close();
  return marks;
}

/** A line with nothing typed in it yet (a newly inserted one): no description and no numbers. */
export function isFreshLine(d) {
  const blank = (v) => v === undefined || v === null || String(v).trim() === '';
  return blank(d.description) && blank(d.amount) && blank(d.x) && blank(d.rate) && blank(d.x4);
}

/** Deepest indent the grid draws (the app allows 0 to 4). */
export const MAX_INDENT = 4;

/**
 * CSS classes for a detail row (the row-styling hook; styles.css has the
 * matching "Detail row kinds" and "Row styles" rules):
 *   line | subtotal | adjustment | separator   the row's kind
 *   suppressed                                a group keeps it out of the total
 *   fr fr-single|start|mid|end                its place in a fringe range
 *   row-bold, row-italic, indent-1 ... indent-4   text style
 * The text color is not a class here: rowColor() picks it and grid.js draws it.
 */
export function rowClasses(d, rr, rangeMark) {
  const cls = [d.kind || 'line'];
  if (rr && !rr.included && d.kind !== 'separator') cls.push('suppressed');
  if (rangeMark) cls.push('fr', `fr-${rangeMark}`);
  if (d.bold === true) cls.push('row-bold');
  if (d.italic === true) cls.push('row-italic');
  const indent = Number.isInteger(d.indent) ? Math.min(Math.max(d.indent, 0), MAX_INDENT) : 0;
  if (indent) cls.push(`indent-${indent}`);
  return cls;
}

/** Fringe dollars as a percentage of a line's total, e.g. "18.50%"; '' when the total is 0. */
export function fringePercent(fringeCents, totalCents) {
  if (!totalCents) return '';
  const hundredths = Math.round((fringeCents * 10000) / totalCents);
  return `${decimal((hundredths / 100).toFixed(2))}%`;
}

/**
 * The rows of account `ai` in category `entry`. `full` is the view's account
 * on screen ({ details, rows, values }): the account's rows in file-format
 * JSON, their totals (rows is null without totals) and their formula cells'
 * values. `shown` (a Set of detail column keys on screen) turns on the
 * hidden-factor marker: a line whose hidden X, 4X or currency changes its
 * total gets a mark in its # cell, with what it is on hover (never printed).
 */
export function detailRows(ix, entry, ai, full, shown = null) {
  const acct = (entry.cat.accounts || [])[ai];
  if (!acct) return [];
  const rep = entry.rep ? entry.rep.accounts[ai] : null;
  const details = full && Array.isArray(full.details) ? full.details : acct.details || [];
  const rowTotals = full ? full.rows : rep && rep.rows;
  const ar = rep && Array.isArray(rowTotals) ? { ...rep, rows: rowTotals } : null;
  const base = ix.b.base_currency;
  const groups = ix.b.groups || {};
  const values = full && Array.isArray(full.values) ? full.values : [];
  const numCtx = { globals: ix.b.globals || {}, info: ix.b.global_info || {}, prefs: budgetDisplay(ix.b) };

  // The rate an adjustment row applies to: lines above it, back to the previous
  // counted adjustment (subtotal rows don't reset it), as the engine does.
  const adjBase = new Map();
  if (ar) {
    let since = 0;
    details.forEach((d, i) => {
      const rr = ar.rows[i];
      const kind = d.kind || 'line';
      if (kind === 'subtotal' || kind === 'separator') return;
      if (kind === 'adjustment') {
        adjBase.set(d.id, since);
        if (rr.included) since = 0;
        else since += rr.total;
      } else {
        since += rr.total;
      }
    });
  }
  const ranges = fringeRangeMarks(details, ar ? ar.rows : null);

  const rows = details.map((d, i) => {
    const kind = d.kind || 'line';
    const rr = ar ? ar.rows[i] : null;
    const issues = ix.cellIssues.get(d.id) || {};
    const line = kind === 'line';
    const sub = kind === 'subtotal';
    const style = {
      key: `r${d.id}`,
      id: d.id,
      kind,
      cls: rowClasses(d, rr, ranges[i]).join(' '),
      color: rowColor(d, groups),
    };
    const note = notesTitle(d.notes);
    const num = { text: String(i + 1), cls: 'muted' };
    if (ranges[i]) num.title = `Fringe range ${d.fringe_range}: percent fringe cutoffs apply to these lines together`;
    const hidden = hiddenFactors(d, values[i], shown, base);
    if (hidden.length) {
      num.cls = 'muted hidden-factor';
      const what = `A hidden column changes this line's total: ${hidden.join(', ')}`;
      num.title = num.title ? `${what}\n${num.title}` : what;
    }
    const cellCtx = (field) => ({ ...numCtx, field, values: values[i] });

    // A separator is a drawn rule: nothing to edit or total, but it can be
    // selected and deleted. Editing moves past it (editSkip).
    if (kind === 'separator') {
      const description = { text: '', cls: 'rule' };
      if (note) Object.assign(description, { cls: 'rule has-note', title: note });
      return { ...style, editSkip: true, cells: { num, description } };
    }

    const description = {
      ...textCell(d.description, issues.description),
      placeholder: sub ? 'Subtotal' : kind === 'adjustment' ? 'Adjustment' : line && isFreshLine(d) ? 'Type a description' : '',
    };
    if (note) {
      description.cls = 'has-note';
      description.title = note;
    }
    const cells = { num, description };
    if (!sub) {
      cells.amount = formulaCell(d.amount, issues.amount, cellCtx('amount'));
      cells.unit = textCell(d.unit);
      cells.x = formulaCell(d.x, issues.x, cellCtx('x'));
      cells.unit2 = textCell(d.unit2);
      cells.x4 = formulaCell(d.x4, issues.x4, cellCtx('x4'));
      cells.unit4 = textCell(d.unit4);
      cells.groups = tagCell(ix.b, 'groups', d.groups, issues.groups);
      cells.set = textCell(d.set);
      cells.location = textCell(d.location);
    }
    if (line) {
      cells.rate = formulaCell(d.rate, issues.rate, cellCtx('rate'));
      cells.unit3 = textCell(d.unit3);
      cells.currency = textCell(d.currency, issues.currency);
      cells.fringes = tagCell(ix.b, 'fringes', d.fringes, issues.fringes);
    } else if (kind === 'adjustment') {
      cells.rate = adjBase.has(d.id)
        ? { text: money(adjBase.get(d.id)), cls: 'auto', title: 'Sum of the lines above, back to the previous adjustment' }
        : {};
    }

    if (!rr) {
      cells.total = sub ? {} : { ...NA };
    } else if (sub) {
      cells.total = moneyCell(rr.subtotal);
    } else if (!rr.included) {
      cells.total = { text: '', placeholder: 'not counted', cls: 'muted', title: 'A group on this line is out of the budget total' };
    } else {
      const foreign = rr.currency && rr.currency !== base;
      cells.total = moneyCell(rr.total, foreign ? { title: `${money(rr.local_total)} ${rr.currency}` } : {});
    }
    if (rr && line && rr.fringe_total) {
      const parts = Object.entries(rr.fringes).map(([n, c]) => `${n}: ${money(c)}`);
      cells.fringe_total = { text: money(rr.fringe_total), cls: 'muted', title: parts.join('\n') };
    }
    // Effective fringe percent: blank when the line has no total, or neither
    // fringes nor fringe dollars (0.00% still shows a fringe that earned nothing).
    if (ix.variance && rr && (line || kind === 'adjustment')) Object.assign(cells, varianceCells(ix.variance.rows[d.id]));
    if (rr && line && rr.included && rr.total && (rr.fringe_total || (d.fringes || []).length)) {
      const parts = Object.entries(rr.fringes).map(([n, c]) => `${n}: ${fringePercent(c, rr.total)}`);
      cells.fringe_pct = { text: fringePercent(rr.fringe_total, rr.total), cls: 'muted', title: parts.join('\n') };
    }

    return { ...style, cells };
  });

  const posting = postingKind(ix);
  // The baseline keeps the account's total as the topsheet shows it: with
  // its fringes only when they post to the account.
  const acctChange = ix.variance ? varianceCells(ix.variance.accounts[acct.code]) : {};
  const withFringes = posting === 'account' && ar;
  rows.push({
    key: 'acct-total',
    cls: 'summary first grand',
    selectable: false,
    cells: {
      description: { text: `Total ${acct.name || acct.code}` },
      total: ar ? moneyCell(ar.details_total) : NA,
      fringe_total: ar ? { text: money(ar.fringe_total), cls: 'muted' } : NA,
      fringe_pct: ar && ar.details_total && ar.fringe_total ? { text: fringePercent(ar.fringe_total, ar.details_total), cls: 'muted' } : {},
      ...(withFringes ? {} : acctChange),
    },
  });
  if (withFringes) {
    rows.push({
      key: 'acct-total-fr',
      cls: 'summary',
      selectable: false,
      cells: { description: { text: 'Total with fringes' }, total: moneyCell(ar.total), ...acctChange },
    });
  }
  return rows;
}
