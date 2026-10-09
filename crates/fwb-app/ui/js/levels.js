// Turns a View (budget JSON + report) into grid columns and rows for each of
// the three levels. Pure functions: no DOM, no commands.

import { money, decimal, names } from './format.js';

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

  // Problems on detail cells: row id -> { field: message }.
  const cellIssues = new Map();
  for (const issue of view.issues || []) {
    const t = issue.target || {};
    if (t.kind !== 'cell') continue;
    const m = cellIssues.get(t.row_id) || {};
    m[t.field] = m[t.field] ? `${m[t.field]}; ${issue.message}` : issue.message;
    cellIssues.set(t.row_id, m);
  }
  const accountHasIssue = (acct) => (acct.details || []).some((d) => cellIssues.has(d.id));
  const categoryHasIssue = (cat) => (cat.accounts || []).some(accountHasIssue);

  return { b, r, topsheet, cats, levels, cellIssues, accountHasIssue, categoryHasIssue };
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

// ---------------------------------------------------------------------------
// Topsheet

export const TOPSHEET_COLUMNS = [
  { key: 'code', label: 'Acct #' },
  { key: 'name', label: 'Description' },
  { key: 'total', label: 'Total', cls: 'num' },
];

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
    const bad = ix.categoryHasIssue(row);
    rows.push({
      key: `t${ti}`,
      cls: 'category',
      ti,
      kind: 'category',
      activatable: true,
      cells: {
        code: { text: row.code, edit: row.code },
        name: { text: row.name, edit: row.name },
        total: c && c.rep ? moneyCell(c.rep.total, bad ? { error: PROBLEM } : {}) : NA,
      },
    });
  }
  if (!r) return rows;

  rows.push(staticRow('s-atl', 'summary first', 'Total above-the-line', r.total_atl));
  rows.push(staticRow('s-btl', 'summary', 'Total below-the-line', r.total_btl));
  rows.push(staticRow('s-atlbtl', 'summary', 'Total above and below-the-line', r.atl_btl));
  if (r.budget_fringes) rows.push(staticRow('s-fr', 'summary', 'Total fringes', r.budget_fringes));
  r.charges.forEach((c, i) => {
    rows.push(staticRow(`s-ch${i}`, 'summary charge', c.name, c.amount));
  });
  rows.push(staticRow('s-grand', 'summary grand', 'Grand total', r.grand_total));
  r.credits.forEach((c, i) => {
    rows.push(staticRow(`s-cr${i}`, 'summary credit', `Less ${c.name}`, -c.amount));
  });
  if (r.credits.length) rows.push(staticRow('s-net', 'summary grand', 'Net total', r.net_total));
  if (r.display) {
    const cur = r.totals_currency;
    rows.push(staticRow('s-disp', 'summary', `Net total in ${cur}`, r.display.net_total));
  }
  return rows;
}

// ---------------------------------------------------------------------------
// Accounts in one category

export const ACCOUNT_COLUMNS = [
  { key: 'code', label: 'Acct #' },
  { key: 'name', label: 'Description' },
  { key: 'fringe_total', label: 'Fringes', cls: 'num' },
  { key: 'total', label: 'Total', cls: 'num' },
];

const FRINGE_POSTED = {
  account: 'Fringes earned by lines in this account; they post to the account, inside its total',
  category: "Fringes earned by lines in this account; they post to the category's fringe account",
  production_level: 'Fringes earned by lines in this account; they post above the production total',
  budget: "Fringes earned by lines in this account; they post to the budget's total fringes",
};

/** Account-list columns; the Fringes header says where the fringes post. */
export function accountColumns(ix) {
  const title = FRINGE_POSTED[postingKind(ix)] || FRINGE_POSTED.category;
  return ACCOUNT_COLUMNS.map((c) => (c.key === 'fringe_total' ? { ...c, title } : c));
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
    const bad = ix.accountHasIssue(a);
    return {
      key: `a${ai}`,
      code: a.code,
      ai,
      cls: 'account',
      activatable: true,
      cells: {
        code: { text: a.code, edit: a.code },
        name: { text: a.name, edit: a.name },
        fringe_total: ar ? { text: ar.fringe_total ? money(ar.fringe_total) : '', cls: frCls } : NA,
        total: ar ? moneyCell(ar.total, bad ? { error: PROBLEM } : {}) : NA,
      },
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
  { key: 'total', label: 'Total', cls: 'num', width: 130, min: 116, pin: 'right' },
];

/** The optional detail columns, for the column menu. */
export const OPTIONAL_DETAIL_COLUMNS = DETAIL_COLUMNS.filter((c) => c.optional);

/** Whether an optional column shows, given saved choices ({ key: boolean }). */
export function columnShown(col, choices) {
  if (!col.optional) return true;
  const v = choices ? choices[col.key] : undefined;
  return typeof v === 'boolean' ? v : col.on;
}

/** The detail columns to draw, given saved choices. */
export function detailColumns(choices) {
  return DETAIL_COLUMNS.filter((c) => columnShown(c, choices));
}

const PLAIN_NUMBER = /^[+-]?(\d+\.?\d*|\.\d+)$/;

/** A formula cell: plain numbers shown formatted, formulas as typed. */
function formulaCell(f, issue) {
  const s = f || '';
  const t = s.trim();
  const cell = { edit: s, text: PLAIN_NUMBER.test(t) ? decimal(t.replace(/^\+/, '')) : s };
  if (t && !PLAIN_NUMBER.test(t)) cell.cls = 'formula';
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

export function detailRows(ix, entry, ai) {
  const acct = (entry.cat.accounts || [])[ai];
  if (!acct) return [];
  const ar = entry.rep ? entry.rep.accounts[ai] : null;
  const details = acct.details || [];
  const base = ix.b.base_currency;
  const groups = ix.b.groups || {};

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
    const note = typeof d.note === 'string' && d.note.trim() ? d.note : '';
    const num = { text: String(i + 1), cls: 'muted' };
    if (ranges[i]) num.title = `Fringe range ${d.fringe_range}: percent fringe cutoffs apply to these lines together`;

    // A separator is a drawn rule: nothing to edit or total, but it can be
    // selected and deleted. Editing moves past it (editSkip).
    if (kind === 'separator') {
      const description = { text: '', cls: 'rule' };
      if (note) Object.assign(description, { cls: 'rule has-note', title: `Note: ${note}` });
      return { ...style, editSkip: true, cells: { num, description } };
    }

    const description = {
      ...textCell(d.description, issues.description),
      placeholder: sub ? 'Subtotal' : kind === 'adjustment' ? 'Adjustment' : '',
    };
    if (note) {
      description.cls = 'has-note';
      description.title = `Note: ${note}`;
    }
    const cells = { num, description };
    if (!sub) {
      cells.amount = formulaCell(d.amount, issues.amount);
      cells.unit = textCell(d.unit);
      cells.x = formulaCell(d.x, issues.x);
      cells.unit2 = textCell(d.unit2);
      cells.x4 = formulaCell(d.x4, issues.x4);
      cells.unit4 = textCell(d.unit4);
      cells.groups = textCell(names(d.groups), issues.groups);
      cells.set = textCell(d.set);
      cells.location = textCell(d.location);
    }
    if (line) {
      cells.rate = formulaCell(d.rate, issues.rate);
      cells.unit3 = textCell(d.unit3);
      cells.currency = textCell(d.currency, issues.currency);
      cells.fringes = textCell(names(d.fringes), issues.fringes);
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
    if (rr && line && rr.included && rr.total && (rr.fringe_total || (d.fringes || []).length)) {
      const parts = Object.entries(rr.fringes).map(([n, c]) => `${n}: ${fringePercent(c, rr.total)}`);
      cells.fringe_pct = { text: fringePercent(rr.fringe_total, rr.total), cls: 'muted', title: parts.join('\n') };
    }

    return { ...style, cells };
  });

  const posting = postingKind(ix);
  rows.push({
    key: 'acct-total',
    cls: 'summary first grand',
    selectable: false,
    cells: {
      description: { text: `Total ${acct.name || acct.code}` },
      total: ar ? moneyCell(ar.details_total) : NA,
      fringe_total: ar ? { text: money(ar.fringe_total), cls: 'muted' } : NA,
      fringe_pct: ar && ar.details_total && ar.fringe_total ? { text: fringePercent(ar.fringe_total, ar.details_total), cls: 'muted' } : {},
    },
  });
  if (posting === 'account' && ar) {
    rows.push({
      key: 'acct-total-fr',
      cls: 'summary',
      selectable: false,
      cells: { description: { text: 'Total with fringes' }, total: moneyCell(ar.total) },
    });
  }
  return rows;
}
