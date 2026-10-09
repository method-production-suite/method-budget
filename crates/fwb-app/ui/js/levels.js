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
  { key: 'fringe_total', label: 'Fringes', cls: 'num', title: 'Fringes earned by lines in this account' },
  { key: 'total', label: 'Total', cls: 'num' },
];

export function accountRows(ix, entry) {
  const accounts = entry.cat.accounts || [];
  const rep = entry.rep;
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
        fringe_total: ar ? { text: ar.fringe_total ? money(ar.fringe_total) : '', cls: 'muted' } : NA,
        total: ar ? moneyCell(ar.total, bad ? { error: PROBLEM } : {}) : NA,
      },
    };
  });
  if (rep && rep.fringe_account) {
    rows.push({
      key: 'fa',
      cls: 'posted',
      selectable: false,
      cells: {
        code: { text: rep.fringe_account.code },
        name: { text: 'Fringes' },
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
      fringe_total: rep ? { text: money(rep.fringe_total), cls: 'muted' } : NA,
      total: rep ? moneyCell(rep.total) : NA,
    },
  });
  return rows;
}

// ---------------------------------------------------------------------------
// Detail rows in one account

export const DETAIL_COLUMNS = [
  { key: 'num', label: '#', cls: 'num' },
  { key: 'description', label: 'Description' },
  { key: 'amount', label: 'Amount', cls: 'num' },
  { key: 'unit', label: 'Unit' },
  { key: 'x', label: 'X', cls: 'num', title: 'Multiplier (blank counts as 1)' },
  { key: 'rate', label: 'Rate', cls: 'num' },
  { key: 'currency', label: 'Cur', title: 'Currency (blank is the base currency)' },
  { key: 'x4', label: '4X', cls: 'num', title: 'Fourth factor (blank counts as 1)' },
  { key: 'total', label: 'Total', cls: 'num' },
  { key: 'fringes', label: 'Fringes', title: 'Fringes on this line, separated by commas' },
  { key: 'fringe_total', label: 'Fringe $', cls: 'num', title: 'Fringe dollars this line earns' },
  { key: 'groups', label: 'Groups', title: 'Groups, separated by commas' },
  { key: 'set', label: 'Set' },
  { key: 'location', label: 'Location' },
];

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

export function detailRows(ix, entry, ai) {
  const acct = (entry.cat.accounts || [])[ai];
  if (!acct) return [];
  const ar = entry.rep ? entry.rep.accounts[ai] : null;
  const details = acct.details || [];
  const base = ix.b.base_currency;

  // The rate an adjustment row applies to: lines above it, back to the previous
  // counted adjustment (subtotal rows don't reset it), as the engine does.
  const adjBase = new Map();
  if (ar) {
    let since = 0;
    details.forEach((d, i) => {
      const rr = ar.rows[i];
      const kind = d.kind || 'line';
      if (kind === 'subtotal') return;
      if (kind === 'adjustment') {
        adjBase.set(d.id, since);
        if (rr.included) since = 0;
        else since += rr.total;
      } else {
        since += rr.total;
      }
    });
  }

  const rows = details.map((d, i) => {
    const kind = d.kind || 'line';
    const rr = ar ? ar.rows[i] : null;
    const issues = ix.cellIssues.get(d.id) || {};
    const line = kind === 'line';
    const sub = kind === 'subtotal';
    const cells = {
      num: { text: String(i + 1), cls: 'muted' },
      description: {
        ...textCell(d.description, issues.description),
        placeholder: sub ? 'Subtotal' : kind === 'adjustment' ? 'Adjustment' : '',
      },
    };
    if (!sub) {
      cells.amount = formulaCell(d.amount, issues.amount);
      cells.unit = textCell(d.unit);
      cells.x = formulaCell(d.x, issues.x);
      cells.x4 = formulaCell(d.x4, issues.x4);
      cells.groups = textCell(names(d.groups), issues.groups);
      cells.set = textCell(d.set);
      cells.location = textCell(d.location);
    }
    if (line) {
      cells.rate = formulaCell(d.rate, issues.rate);
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

    const cls = [kind];
    if (rr && !rr.included) cls.push('suppressed');
    return { key: `r${d.id}`, id: d.id, kind, cls: cls.join(' '), cells };
  });

  const posting = ix.b.fringe_posting ? ix.b.fringe_posting.kind : 'category';
  rows.push({
    key: 'acct-total',
    cls: 'summary first grand',
    selectable: false,
    cells: {
      description: { text: `Total ${acct.name || acct.code}` },
      total: ar ? moneyCell(ar.details_total) : NA,
      fringe_total: ar ? { text: money(ar.fringe_total), cls: 'muted' } : NA,
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
