// Turns a View (the budget's outline and totals, plus the account on screen in
// full) into grid columns and rows for each of the three levels. Pure
// functions: no DOM, no commands.

import { decimal } from './format.js';
import { tagCellText, notesTitle } from './order.js';
import { totalText, currencyTotalText, numberCell, budgetDisplay, hiddenFactors } from './display.js';

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
  // Cost figures (actual, ETC, EAC...) when the budget has actuals: in step
  // with the report's categories and levels (src/actuals.rs).
  const cost = view.cost && typeof view.cost === 'object' && Array.isArray(view.cost.categories) ? view.cost : null;
  // Each person lines pay, across accounts (decisions.md, entry 79): null
  // when no line has one.
  const persons = view.persons && typeof view.persons === 'object' ? view.persons : null;
  // Some line has a person even when the totals failed: the Person column stays.
  const hasPersons = Boolean(view.has_persons) || Boolean(persons);

  // Lines, accounts and categories with review comments to settle
  // (comments.js): a mark on their number.
  const rv = view.review && typeof view.review === 'object' ? view.review : null;
  const commented = {
    rows: new Set(rv && Array.isArray(rv.rows) ? rv.rows : []),
    accounts: new Set(rv && Array.isArray(rv.accounts) ? rv.accounts : []),
    categories: new Set(rv && Array.isArray(rv.categories) ? rv.categories : []),
  };

  return { b, r, topsheet, cats, levels, cellIssues, accountHasIssue, categoryHasIssue, variance, cost, persons, hasPersons, commented };
}

// ---------------------------------------------------------------------------
// Cost report columns (Actual, ETC, EAC, EAC Var, % Spent): on the topsheet
// and the account list when the budget has actuals (view.cost), before Total
// like Orig and Var; the lines get Actual (transactions assigned to a line).
// Shown until the user hides them from the column menu; never without
// actuals. An over-budget row's EAC variance is marked (cost-over).

/** "12.5" (Rust's percent with up to two places) as "12.50%". */
export function percentText(p) {
  if (p === null || p === undefined || p === '') return '';
  const s = String(p);
  const neg = s.startsWith('-');
  const [int, frac = ''] = (neg ? s.slice(1) : s).split('.');
  return `${neg ? '-' : ''}${decimal(`${int}.${frac.padEnd(2, '0').slice(0, 2)}`)}%`;
}

/** The cost cells for one row's figures ({ budget, actual, etc, eac, variance, percent, over, count }). */
export function costCells(f, opts = {}) {
  if (!f || typeof f !== 'object') return {};
  const n = typeof f.count === 'number' ? f.count : 0;
  const act = { text: money(f.actual), title: `${n} ${n === 1 ? 'transaction' : 'transactions'}` };
  const out = {
    act,
    etc: { text: money(f.etc), title: opts.method ? opts.method : 'Estimate to complete' },
    eac: { text: money(f.eac), title: 'Estimate at completion: actual plus estimate to complete' },
    cvar: { text: money(f.variance), title: f.over ? 'Over budget: the estimate at completion is more than the budget' : 'Budget minus the estimate at completion' },
    spent: { text: percentText(f.percent), title: 'Actual as a percentage of the budget' },
  };
  if (f.over) out.cvar.cls = 'cost-over';
  return out;
}

/** How an account's estimate to complete is worked out, in words, for a tooltip. */
function methodText(m) {
  if (!m || typeof m !== 'object') return 'Estimate to complete: budget minus actual';
  if (m.kind === 'manual') return `Estimate to complete: typed (${money(m.amount)})`;
  if (m.kind === 'percent_complete') return `Estimate to complete: ${m.percent}% complete`;
  return 'Estimate to complete: budget minus actual';
}

/** The cost figures of category `ci` and its account `ai` (null when there are none). */
function costOf(ix, ci, ai = null) {
  const c = ix.cost && ix.cost.categories ? ix.cost.categories[ci] : null;
  if (!c) return null;
  return ai === null ? c : c.accounts[ai] || null;
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

/**
 * Charge `i` of the report's charges as the topsheet names it: its name,
 * then its percent and the amount left out of its base when the charge's
 * switches say so (Budget > Charges; display only). `fmt` formats cents.
 * Reports list only applied charges, in order, so the budget's applied
 * charges line up with them.
 */
export function chargeLabel(b, entry, i, fmt) {
  const name = (entry && entry.name) || 'Charge';
  const applied = (Array.isArray(b && b.charges) ? b.charges : []).filter((c) => c && c.applied !== false);
  const c = applied[i];
  if (!c || c.name !== entry.name) return name;
  let label = name;
  if (c.label_percent === true && c.kind === 'percent' && c.percent !== undefined && c.percent !== null) label += ` ${c.percent}%`;
  if (c.label_excluded === true) label += ` (excluding ${fmt(entry.excluded)})`;
  return label;
}

/**
 * The budget's one fringe row (fringes posted by budget; decisions.md, entry
 * 59): its account number ('' for none), description ("Total Fringes" unless
 * the budget says otherwise) and place: above topsheet row `position`, or
 * after the summary rows when `position` is null. Display only.
 */
export function fringeRowOf(b) {
  const f = b && b.fringe_row && typeof b.fringe_row === 'object' ? b.fringe_row : {};
  const text = (v) => (typeof v === 'string' ? v.trim() : '');
  return {
    account: text(f.account),
    description: text(f.description) || 'Total Fringes',
    position: Number.isInteger(f.position) && f.position >= 0 ? f.position : null,
  };
}

/**
 * Indexes into a list of `n` charges or credits in the order the topsheet
 * lists them: the reverse of the budget's order (charges newest first,
 * credits in reverse application order; decisions.md, entry 60).
 */
export function topsheetOrder(n) {
  return Array.from({ length: n }, (_, i) => n - 1 - i);
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
function isZero(total, bad, change, cost = null) {
  if (total !== 0 || bad) return false;
  // A row with actuals or an estimate to complete isn't empty.
  if (cost && cost.figures && (cost.figures.actual !== 0 || cost.figures.etc !== 0)) return false;
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

/** Columns with the shown Orig and Var (and cost) columns put before Total. */
function withBaselineColumns(cols, choices, ctx) {
  const extra = [...BASELINE_COLUMNS, ...COST_COLUMNS].filter((c) => columnShown(c, choices, ctx)).map(({ key, label, cls, title }) => ({ key, label, cls, title }));
  if (!extra.length) return cols;
  const at = cols.findIndex((c) => c.key === 'total');
  return [...cols.slice(0, at), ...extra, ...cols.slice(at)];
}

/** Topsheet columns, with Orig and Var when the budget has a baseline (and they aren't hidden). */
export function topsheetColumns(ix, choices) {
  return withBaselineColumns(TOPSHEET_COLUMNS, choices, { baseline: Boolean(ix.variance), cost: Boolean(ix.cost) });
}

export function topsheetRows(ix, choices = null) {
  const r = ix.r;
  const rows = [];
  const lvCost = (lv) => (ix.cost && lv && ix.cost.levels[lv.li]) || null;
  // The budget's fringe row: after the summary rows, or above the topsheet
  // row the budget places it at (Budget Setup > Settings). Display only.
  const fr = fringeRowOf(ix.b);
  const frShown = Boolean(r && r.budget_fringes);
  const frAt = frShown && fr.position !== null ? Math.min(fr.position, ix.topsheet.length) : null;
  const frCost = ix.cost && ix.cost.budget_fringes ? costCells(ix.cost.budget_fringes.figures) : {};
  const fringeRow = (cls) => staticRow('s-fr', cls, fr.description, r.budget_fringes, { code: { text: fr.account }, ...frCost });
  for (const [ti, row] of ix.topsheet.entries()) {
    if (frAt === ti) rows.push(fringeRow('posted'));
    if (row.kind === 'production_total') {
      const lv = ix.levels.find((l) => l.ti === ti);
      const lc = lvCost(lv);
      if (lv && lv.rep && lv.rep.posted_fringes) {
        rows.push(staticRow(`f${ti}`, 'posted', 'Fringes', lv.rep.posted_fringes, lc ? costCells(lc.posted_fringes) : {}));
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
          ...(lc ? costCells(lc.figures) : {}),
        },
      });
      continue;
    }
    const c = ix.cats.find((x) => x.ti === ti);
    const bad = ix.categoryHasIssue(ti);
    const cc = c ? costOf(ix, c.ci) : null;
    rows.push({
      key: `t${ti}`,
      cls: 'category',
      ti,
      kind: 'category',
      activatable: true,
      cells: {
        code: commentedCell({ text: row.code, edit: row.code }, ix.commented.categories.has(row.code)),
        name: noted({ text: row.name, edit: row.name }, row.notes),
        total: c && c.rep ? moneyCell(c.rep.total, bad ? { error: PROBLEM } : {}) : NA,
        ...(ix.variance ? varianceCells(ix.variance.categories[row.code]) : {}),
        ...(cc ? costCells(cc.figures) : {}),
      },
      zero: Boolean(c && c.rep) && isZero(c.rep.total, bad, ix.variance && ix.variance.categories[row.code], cc),
    });
  }
  if (frAt !== null && frAt >= ix.topsheet.length) rows.push(fringeRow('posted'));
  if (!r) return rows;
  const cost = ix.cost;
  const costShown = Boolean(cost) && COST_COLUMNS.some((col) => columnShown(col, choices, { cost: true }));

  // The budget's display settings can leave out the summary rows and the
  // net total (Budget > Settings). Display only.
  const disp = budgetDisplay(ix.b);
  if (disp.summary_rows) {
    rows.push(staticRow('s-atl', 'summary first', 'Total above-the-line', r.total_atl, cost ? costCells(cost.total_atl) : {}));
    rows.push(staticRow('s-btl', 'summary', 'Total below-the-line', r.total_btl, cost ? costCells(cost.total_btl) : {}));
    rows.push(staticRow('s-atlbtl', 'summary', 'Total above and below-the-line', r.atl_btl, cost ? costCells(cost.atl_btl) : {}));
  }
  if (frShown && frAt === null) rows.push(fringeRow(disp.summary_rows ? 'summary' : 'summary first'));
  // Charges newest first (the reverse of the budget's list).
  for (const i of topsheetOrder(r.charges.length)) {
    const c = r.charges[i];
    rows.push(staticRow(`s-ch${i}`, 'summary charge', chargeLabel(ix.b, c, i, money), c.amount, cost && cost.charges[i] ? costCells(cost.charges[i].figures) : {}));
  }
  // Transactions no account in the chart claims: in the actual, never in the budget.
  if (costShown && cost.unassigned && cost.unassigned.count) {
    const row = staticRow('s-unassigned', 'summary cost-unassigned', 'Unassigned actuals', null, costCells(cost.unassigned));
    row.cells.total = { text: '', title: 'Not in the budget: transactions whose account number isn’t in the chart' };
    row.cells.name.title = 'Transactions whose account number isn’t in the chart, or have none (the Actuals tab lists them)';
    rows.push(row);
  }
  rows.push(staticRow('s-grand', 'summary grand', 'Grand total', r.grand_total, { ...(ix.variance ? varianceCells(ix.variance.grand_total) : {}), ...(cost ? costCells(cost.total) : {}) }));
  // Credits in reverse application order, then their subtotal with two or more.
  for (const i of topsheetOrder(r.credits.length)) {
    const c = r.credits[i];
    rows.push(staticRow(`s-cr${i}`, 'summary credit', `Less ${c.name}`, -c.amount));
  }
  if (r.credits.length >= 2) rows.push(staticRow('s-crsub', 'summary credit', 'Sub Total', -r.credits_total));
  if (r.credits.length && disp.net_total) rows.push(staticRow('s-net', 'summary grand', 'Net total', r.net_total, ix.variance ? varianceCells(ix.variance.net_total) : {}));
  if (r.display && disp.net_total) {
    const cur = r.totals_currency;
    const row = staticRow('s-disp', 'summary', `Net total in ${cur}`, r.display.net_total);
    // In that currency's own separators and symbol (decisions.md, entry 61).
    row.cells.total = { text: currencyTotalText(r.display.net_total, (ix.b.currencies || {})[cur]) };
    rows.push(row);
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
  return withBaselineColumns(cols, choices, { baseline: Boolean(ix.variance), cost: Boolean(ix.cost) });
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
    const ac = costOf(ix, entry.ci, ai);
    return {
      key: `a${ai}`,
      code: a.code,
      ai,
      cls: 'account',
      activatable: true,
      cells: {
        code: commentedCell({ text: a.code, edit: a.code }, ix.commented.accounts.has(a.code)),
        name: noted({ text: a.name, edit: a.name }, a.notes),
        fringe_total: ar ? { text: ar.fringe_total ? money(ar.fringe_total) : '', cls: frCls } : NA,
        total: ar ? moneyCell(ar.total, bad ? { error: PROBLEM } : {}) : NA,
        ...(ix.variance ? varianceCells(ix.variance.accounts[a.code]) : {}),
        ...(ac ? costCells(ac.figures, { method: methodText(ac.method) }) : {}),
      },
      zero: Boolean(ar) && isZero(ar.total, bad, ix.variance && ix.variance.accounts[a.code], ac),
    };
  });
  const catCost = costOf(ix, entry.ci);
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
        ...(catCost && catCost.fringe_account ? costCells(catCost.fringe_account.figures, { method: methodText(catCost.fringe_account.method) }) : {}),
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
      ...(catCost ? costCells(catCost.figures) : {}),
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
  // The full fringe percent: every fringe before cutoffs and ranges (decisions.md, entry 61).
  { key: 'fringe_full_pct', label: 'Full fringe %', name: 'Full fringe % (before cutoffs)', cls: 'num', title: 'The line’s fringes before cutoffs and ranges, as a percentage of the line total', width: 96, min: 84, optional: true, on: false },
  { key: 'fringe_total', label: 'Fringe $', cls: 'num', title: 'Fringe dollars this line earns', width: 104, min: 92, optional: true, on: true },
  { key: 'groups', label: 'Groups', title: 'Groups, separated by commas', width: 130, min: 60, optional: true, on: true },
  { key: 'set', label: 'Set', width: 90, min: 60, optional: true, on: false },
  { key: 'location', label: 'Location', width: 110, min: 70, optional: true, on: false },
  // The person a line pays (decisions.md, entry 79): shown by default once a line has one.
  { key: 'person', label: 'Person', title: 'The person this line pays: their percent fringe cutoffs apply once, across every account', width: 120, min: 70, optional: true, on: 'persons' },
  // With a baseline only (`baseline: true`), shown by default then.
  { key: 'orig', label: 'Orig', name: 'Orig (original total)', cls: 'num', title: 'The total when the baseline was locked', width: 120, min: 96, optional: true, on: 'baseline', baseline: true },
  { key: 'var', label: 'Var', name: 'Var (change since the baseline)', cls: 'num', title: 'How far the total has moved since the baseline was locked', width: 120, min: 96, optional: true, on: 'baseline', baseline: true },
  // With actuals only (`cost: true`), shown by default then: the transactions assigned to each line.
  { key: 'act', label: 'Actual', name: 'Actual (actual to date)', cls: 'num', title: 'Actual to date: the imported transactions (on a line, those assigned to it)', width: 120, min: 96, optional: true, on: 'cost', cost: true },
  { key: 'total', label: 'Total', cls: 'num', width: 130, min: 116, pin: 'right' },
];

/** The optional detail columns, for the column menu. */
export const OPTIONAL_DETAIL_COLUMNS = DETAIL_COLUMNS.filter((c) => c.optional);

/** The Orig and Var columns, which every level can show when the budget has a baseline. */
export const BASELINE_COLUMNS = DETAIL_COLUMNS.filter((c) => c.baseline);

/**
 * The cost report columns the topsheet and the account list show when the
 * budget has actuals (Actual is also a detail column, for lines).
 */
export const COST_COLUMNS = [
  DETAIL_COLUMNS.find((c) => c.key === 'act'),
  { key: 'etc', label: 'ETC', name: 'ETC (estimate to complete)', cls: 'num', title: 'Estimate to complete: budget minus actual unless the Actuals tab sets another way', optional: true, on: 'cost', cost: true },
  { key: 'eac', label: 'EAC', name: 'EAC (estimate at completion)', cls: 'num', title: 'Estimate at completion: actual plus estimate to complete', optional: true, on: 'cost', cost: true },
  { key: 'cvar', label: 'EAC Var', name: 'EAC Var (budget minus EAC)', cls: 'num', title: 'Budget minus the estimate at completion; marked when over budget', optional: true, on: 'cost', cost: true },
  { key: 'spent', label: '% Spent', name: '% Spent (actual of budget)', cls: 'num', title: 'Actual as a percentage of the budget', optional: true, on: 'cost', cost: true },
];

/**
 * Whether an optional column shows, given saved choices ({ key: boolean })
 * and `ctx.baseline` (the budget has a baseline). Orig and Var never show
 * without one, and show with one unless the user hid them.
 */
export function columnShown(col, choices, ctx = {}) {
  if (!col.optional) return true;
  if (col.baseline && !ctx.baseline) return false;
  if (col.cost && !ctx.cost) return false;
  const v = choices ? choices[col.key] : undefined;
  if (typeof v === 'boolean') return v;
  if (col.on === 'cost') return Boolean(ctx.cost);
  if (col.on === 'persons') return Boolean(ctx.persons);
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

const COMMENTED = 'Review comments to settle: see the Comments tab';

/** A number cell marked when its row has review comments to settle. */
function commentedCell(cell, on) {
  if (!on) return cell;
  return { ...cell, cls: cell.cls ? `${cell.cls} has-comment` : 'has-comment', title: cell.title ? `${COMMENTED}\n${cell.title}` : COMMENTED };
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
 * A person's name as lines are matched by it (decisions.md, entry 79): ends
 * trimmed, inner runs of spaces as one, lower case (as Rust's person_key).
 */
export function personKey(name) {
  return typeof name === 'string' ? name.trim().split(/\s+/).join(' ').toLowerCase() : '';
}

/**
 * A line's Person cell: the name, with the person's running totals across
 * accounts as its tooltip (what they are paid, and each percent fringe's pay
 * against its cutoff); a cutoff reached is marked.
 */
export function personCell(ix, person) {
  const cell = textCell(person);
  const key = personKey(person);
  const p = key && ix.persons ? ix.persons[key] : null;
  const name = p && p.name ? p.name : String(person || '').trim();
  if (!p) return cell;
  const lines = `${p.lines} ${p.lines === 1 ? 'line' : 'lines'}`;
  const where = `${p.accounts.length === 1 ? 'account' : 'accounts'} ${p.accounts.join(', ')}`;
  const parts = [`${name}: paid ${money(p.pay)} on ${lines} in ${where}`];
  for (const f of p.fringes || []) {
    if (f.cutoff === null || f.cutoff === undefined) continue;
    parts.push(
      f.reached
        ? `${f.fringe}: ${money(f.pay)} of a ${money(f.cutoff)} cutoff, reached; ${money(f.amount)} across the lines`
        : `${f.fringe}: ${money(f.pay)} of a ${money(f.cutoff)} cutoff, ${money(f.cutoff - f.pay)} left`,
    );
  }
  cell.title = parts.join('\n');
  if ((p.fringes || []).some((f) => f.reached)) cell.cls = cell.cls ? `${cell.cls} person-reached` : 'person-reached';
  return cell;
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
    // A line with a person leaves its range: the person is its earner.
    const person = typeof d.person === 'string' && d.person.trim() !== '';
    if (!costed || !counted || person) return;
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

/**
 * The account's whole actual on its total row (every transaction coded to
 * it, assigned to a line or not), when the budget has actuals.
 */
function acctActual(ix, entry, ai) {
  const ac = costOf(ix, entry.ci, ai);
  if (!ac) return {};
  const f = ac.figures;
  return { act: { text: money(f.actual), title: `The account's actual to date (${f.count} ${f.count === 1 ? 'transaction' : 'transactions'}; lines show the ones assigned to them)` } };
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
    if (ix.commented.rows.has(d.id)) Object.assign(num, commentedCell(num, true));
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
      cells.person = personCell(ix, d.person);
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
      // An adjustment row can carry fringes too (decisions.md, entry 56).
      cells.fringes = tagCell(ix.b, 'fringes', d.fringes, issues.fringes);
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
    if (rr && (line || kind === 'adjustment') && rr.fringe_total) {
      const parts = Object.entries(rr.fringes).map(([n, c]) => `${n}: ${money(c)}`);
      cells.fringe_total = { text: money(rr.fringe_total), cls: 'muted', title: parts.join('\n') };
    }
    // Effective fringe percent: blank when the line has no total, or neither
    // fringes nor fringe dollars (0.00% still shows a fringe that earned nothing).
    if (ix.variance && rr && (line || kind === 'adjustment')) Object.assign(cells, varianceCells(ix.variance.rows[d.id]));
    if (ix.cost && (line || kind === 'adjustment')) {
      const got = ix.cost.lines ? ix.cost.lines[d.id] : null;
      if (got) cells.act = { text: money(got.amount), title: `${got.count} ${got.count === 1 ? 'transaction' : 'transactions'} assigned to this line` };
    }
    if (rr && (line || kind === 'adjustment') && rr.included && rr.total && (rr.fringe_total || (d.fringes || []).length)) {
      const parts = Object.entries(rr.fringes).map(([n, c]) => `${n}: ${fringePercent(c, rr.total)}`);
      cells.fringe_pct = { text: fringePercent(rr.fringe_total, rr.total), cls: 'muted', title: parts.join('\n') };
      cells.fringe_full_pct = {
        text: fringePercent(rr.fringe_full_total || 0, rr.total),
        cls: 'muted',
        title: 'Every fringe on the line before cutoffs and ranges',
      };
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
      ...(acctActual(ix, entry, ai)),
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
