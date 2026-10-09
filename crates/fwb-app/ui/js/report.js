// Printable budget reports: a topsheet and a full detail listing, built from a
// View (budget JSON + report). Our own layout. Everything from the budget goes
// in through textContent; no inline styles, so it works under the app's CSP.
//
//   renderReport(container, view, kind)  builds the report into `container`
//   printReport(view, kind)              renders into #print-root and prints
//
// kind is "topsheet", "detail" or "both". print.css holds the styles.

import { el } from './dom.js';
import { money, decimal } from './format.js';

const KINDS = new Set(['topsheet', 'detail', 'both']);

// ---------------------------------------------------------------------------
// Small helpers

function td(text, cls, span) {
  const c = el('td', { class: cls || undefined, text: text === '' ? undefined : text });
  if (span && span > 1) c.colSpan = span;
  return c;
}

function th(text, cls, span) {
  const c = el('th', { class: cls || undefined, text: text === '' ? undefined : text });
  if (span && span > 1) c.colSpan = span;
  c.scope = 'col';
  return c;
}

/** Money cell text; blank when the value is unknown. */
function m(cents) {
  return typeof cents === 'number' && Number.isFinite(cents) ? money(cents) : '';
}

const PLAIN_NUMBER = /^[+-]?(\d+\.?\d*|\.\d+)$/;

/** A formula as typed, with plain numbers shown formatted. */
function formula(f) {
  const t = (f || '').trim();
  if (!t) return { text: '', formula: false };
  if (PLAIN_NUMBER.test(t)) return { text: decimal(normalize(t)), formula: false };
  return { text: t, formula: true };
}

/** "+.5" -> "0.5", "7." -> "7"; keeps the sign. */
function normalize(t) {
  let s = t.replace(/^\+/, '');
  const neg = s.startsWith('-');
  if (neg) s = s.slice(1);
  let [int, frac] = s.split('.');
  int = int || '0';
  frac = frac || '';
  return (neg ? '-' : '') + int + (frac ? `.${frac}` : '');
}

/** An adjustment's fraction ("0.1") as a percent ("10%"), without floats. */
function percentOf(f) {
  const t = (f || '').trim();
  if (!t) return { text: '', formula: false };
  if (!PLAIN_NUMBER.test(t)) return { text: t, formula: true };
  const n = normalize(t);
  const neg = n.startsWith('-');
  const [int, frac = ''] = (neg ? n.slice(1) : n).split('.');
  const padded = frac.padEnd(2, '0');
  let whole = (int + padded.slice(0, 2)).replace(/^0+(?=\d)/, '');
  const rest = padded.slice(2).replace(/0+$/, '');
  const s = whole + (rest ? `.${rest}` : '');
  return { text: `${decimal((neg ? '-' : '') + s)}%`, formula: false };
}

function printedAt(now) {
  try {
    return now.toLocaleString(undefined, {
      year: 'numeric',
      month: 'long',
      day: 'numeric',
      hour: 'numeric',
      minute: '2-digit',
    });
  } catch {
    return now.toISOString().slice(0, 16).replace('T', ' ');
  }
}

/**
 * One compared figure, { original, variance } with either possibly null.
 * Accepts the app's variance entries ({ original, current, variance }) or a
 * bare baseline amount in cents.
 */
function change(v) {
  const num = (x) => (typeof x === 'number' && Number.isFinite(x) ? x : null);
  if (typeof v === 'number') return { original: num(v), variance: null };
  if (v && typeof v === 'object') return { original: num(v.original), variance: num(v.variance) };
  return null;
}

/**
 * Baseline figures for comparison, if the view has any: view.variance (from
 * the app) first, else the budget's own baseline. Null when there is none.
 */
function baselineOf(view) {
  const v = view.variance && typeof view.variance === 'object' ? view.variance : null;
  const b = view.budget && view.budget.baseline && typeof view.budget.baseline === 'object' ? view.budget.baseline : null;
  const src = v || b;
  if (!src) return null;
  const categories = new Map();
  if (src.categories && typeof src.categories === 'object') {
    for (const [code, c] of Object.entries(src.categories)) {
      const ch = change(c);
      if (ch) categories.set(code, ch);
    }
  }
  return {
    categories,
    grand_total: change(src.grand_total),
    net_total: change(src.net_total),
  };
}

/** Problem cells: row id -> Set of field names. */
function cellIssues(view) {
  const out = new Map();
  for (const issue of Array.isArray(view.issues) ? view.issues : []) {
    const t = (issue && issue.target) || {};
    if (t.kind !== 'cell') continue;
    if (!out.has(t.row_id)) out.set(t.row_id, new Set());
    out.get(t.row_id).add(t.field);
  }
  return out;
}

/** Pairs topsheet entries from the budget with their report totals. */
function index(view) {
  const b = (view && view.budget) || {};
  const r = (view && view.report) || null;
  const topsheet = Array.isArray(b.topsheet) ? b.topsheet : [];
  const items = []; // { kind: 'category'|'level', row, rep, ci?, li? }
  let ci = 0;
  let li = 0;
  for (const row of topsheet) {
    if (!row || typeof row !== 'object') continue;
    if (row.kind === 'production_total') {
      items.push({ kind: 'level', row, rep: r && r.levels ? r.levels[li] || null : null, li });
      li += 1;
    } else {
      items.push({ kind: 'category', row, rep: r && r.categories ? r.categories[ci] || null : null, ci });
      ci += 1;
    }
  }
  return { b, r, items };
}

/** The running header row repeated at the top of every printed page. */
function runningHead(ctx, section, span) {
  const cell = el(
    'th',
    { class: 'rpt-run' },
    el(
      'div',
      { class: 'rpt-run-inner' },
      el('span', { class: 'rpt-run-title', text: ctx.title }),
      el('span', { class: 'rpt-run-section', text: section }),
      el('span', { class: 'rpt-run-date', text: `Printed ${ctx.printed}` }),
    ),
  );
  cell.colSpan = span;
  return el('tr', { class: 'rpt-run-row' }, cell);
}

function warning(view) {
  const r = view && view.report;
  if (r && !view.partial) return null;
  const e = view && view.error;
  const where = e ? [e.at, e.message].filter(Boolean).join(': ') : '';
  const head = r
    ? 'Totals may be incomplete.'
    : 'Totals could not be calculated. Amounts are left blank.';
  const tail = r
    ? ' Cells that cannot be calculated are counted as blank.'
    : '';
  return el(
    'div',
    { class: 'rpt-warning', attrs: { role: 'note' } },
    el('strong', { text: head }),
    where ? ` ${where}.` : '',
    tail,
  );
}

function titleBlock(ctx, view, subtitle) {
  const b = ctx.b;
  const r = ctx.r;
  const facts = [];
  facts.push(['Base currency', b.base_currency || (r && r.base_currency) || '']);
  const tc = b.totals_currency || (r && r.totals_currency !== r.base_currency ? r.totals_currency : '');
  if (tc) facts.push(['Totals currency', tc]);
  const info = b.info && typeof b.info === 'object' ? b.info : {};
  for (const [label, v] of Object.entries(info)) {
    if (!v || typeof v !== 'object') continue;
    const val = v.kind === 'number' ? decimal(v.value) : String(v.value ?? '');
    facts.push([label, val]);
  }

  const dl = el('dl', { class: 'rpt-facts' });
  for (const [k, v] of facts) {
    dl.append(el('div', { class: 'rpt-fact' }, el('dt', { text: k }), el('dd', { text: v })));
  }
  return el(
    'header',
    { class: 'rpt-title-block' },
    el('div', { class: 'rpt-kicker', text: subtitle }),
    el('h1', { class: 'rpt-title', text: ctx.title }),
    dl,
    warning(view),
  );
}

// ---------------------------------------------------------------------------
// Topsheet

function topsheet(ctx, view) {
  const r = ctx.r;
  const base = ctx.baseline;
  const cols = base ? 5 : 3;

  const section = el('section', { class: 'rpt-section rpt-topsheet' });
  section.append(titleBlock(ctx, view, 'Budget topsheet'));

  const table = el('table', { class: `rpt-table rpt-ts${base ? ' has-baseline' : ''}` });
  const cg = el('colgroup', {}, el('col', { class: 'c-code' }), el('col', { class: 'c-name' }), el('col', { class: 'c-money' }));
  if (base) cg.append(el('col', { class: 'c-money' }), el('col', { class: 'c-money' }));
  table.append(cg);

  const head = el('tr', { class: 'rpt-cols' }, th('Code'), th('Description'), th('Total', 'num'));
  if (base) head.append(th('Baseline', 'num'), th('Variance', 'num'));
  table.append(el('thead', {}, runningHead(ctx, 'Topsheet', cols), head));

  const body = el('tbody', { class: 'rpt-ts-body' });
  const catTotals = []; // per category index: { code, total }
  // ch: { original, variance } or null. Variance falls back to total minus original.
  const row = (cls, code, label, total, ch) => {
    const tr = el('tr', { class: cls }, td(code, 'code'), td(label, 'name'), td(m(total), 'num'));
    if (base) {
      const orig = ch ? ch.original : null;
      let diff = ch ? ch.variance : null;
      if (diff === null && orig !== null && typeof total === 'number') diff = total - orig;
      tr.append(td(m(orig), 'num base'));
      tr.append(td(m(diff), 'num var'));
    }
    return tr;
  };
  const baseFor = (code) => (base && base.categories.get(code)) || null;

  for (const it of ctx.items) {
    if (it.kind === 'category') {
      const total = it.rep ? it.rep.total : null;
      catTotals[it.ci] = it.row.code;
      body.append(row('cat', it.row.code || '', it.row.name || '', total, baseFor(it.row.code)));
    } else {
      if (it.rep && it.rep.posted_fringes) {
        body.append(row('posted', '', 'Fringes', it.rep.posted_fringes, null));
      }
      // A level's baseline only when every category in it has one.
      let lvBase = null;
      if (base && it.rep) {
        let sum = 0;
        let complete = it.rep.end > it.rep.start;
        for (let i = it.rep.start; i < it.rep.end; i++) {
          const v = baseFor(catTotals[i]);
          if (!v || v.original === null) complete = false;
          else sum += v.original;
        }
        if (complete && !it.rep.posted_fringes) lvBase = { original: sum, variance: null };
      }
      body.append(row('level', '', it.row.name || '', it.rep ? it.rep.total : null, lvBase));
    }
  }
  table.append(body);

  const sum = el('tbody', { class: 'rpt-ts-summary' });
  if (r) {
    sum.append(row('sum first', '', 'Total above-the-line', r.total_atl, null));
    sum.append(row('sum', '', 'Total below-the-line', r.total_btl, null));
    sum.append(row('sum strong', '', 'Total above and below-the-line', r.atl_btl, null));
    if (r.budget_fringes) sum.append(row('sum', '', 'Total fringes', r.budget_fringes, null));
    for (const c of r.charges || []) sum.append(row('sum charge', '', c.name || 'Charge', c.amount, null));
    sum.append(row('sum grand', '', 'Grand total', r.grand_total, base ? base.grand_total : null));
    const credits = r.credits || [];
    for (const c of credits) {
      sum.append(row('sum credit', '', `Less ${c.name || 'credit'}`, -c.amount, null));
    }
    if (credits.length) {
      sum.append(row('sum grand', '', 'Net total', r.net_total, base ? base.net_total : null));
    }
    if (r.display) {
      const cur = r.totals_currency;
      const rate = `1 ${cur} = ${decimal(r.totals_rate)} ${r.base_currency}`;
      sum.append(row('sum display first', '', `Grand total in ${cur} (${rate})`, r.display.grand_total, null));
      if (credits.length) sum.append(row('sum display', '', `Net total in ${cur}`, r.display.net_total, null));
    }
  } else {
    sum.append(row('sum first', '', 'Totals unavailable', null, null));
  }
  table.append(sum);
  section.append(table);
  if (ctx.items.length === 0) section.append(el('p', { class: 'rpt-empty', text: 'This budget has no categories yet.' }));
  return section;
}

// ---------------------------------------------------------------------------
// Detail

const DETAIL_COLS = [
  ['No.', 'c-no', 'num'],
  ['Description', 'c-desc', ''],
  ['Amount', 'c-amt', 'num'],
  ['Unit', 'c-unit', ''],
  ['X', 'c-x', 'num'],
  ['Rate', 'c-rate', 'num'],
  ['Cur', 'c-cur', ''],
  ['4X', 'c-x4', 'num'],
  ['Total', 'c-total', 'num'],
  ['Fringes', 'c-fr', ''],
];
const NCOLS = DETAIL_COLS.length;
const LABEL_SPAN = 7; // Description through 4X

/** A formula cell with an optional unit after it. */
function factorCell(f, unit, cls) {
  const v = formula(f);
  const c = el('td', { class: `${cls}${v.formula ? ' formula' : ''}` });
  if (v.text) c.append(v.text);
  if (v.text && unit) c.append(el('span', { class: 'unit', text: ` ${unit}` }));
  return c;
}

function postingNote(b, rep) {
  const kind = b.fringe_posting && b.fringe_posting.kind ? b.fringe_posting.kind : 'category';
  if (kind === 'account') return 'included in this account';
  if (kind === 'budget') return 'posted to total fringes';
  if (kind === 'production_level') return 'posted at the production total';
  if (rep && rep.fringe_account && rep.fringe_account.code) return `posted to ${rep.fringe_account.code}`;
  return 'posted to the category';
}

function sumRow(cls, label, cents, note) {
  const tr = el('tr', { class: `rpt-sum ${cls}` }, td('', 'no'));
  const lab = td(label, 'label', LABEL_SPAN);
  if (note) lab.append(el('span', { class: 'note', text: ` (${note})` }));
  tr.append(lab, td(m(cents), 'num total'), td('', 'fr'));
  return tr;
}

/** Text style classes for a detail row (bold, italic, indent 1 to 4), each with a leading space. */
function rowStyle(d) {
  let cls = '';
  if (d.bold === true) cls += ' row-bold';
  if (d.italic === true) cls += ' row-italic';
  const indent = Number.isInteger(d.indent) ? Math.min(Math.max(d.indent, 0), 4) : 0;
  if (indent) cls += ` indent-${indent}`;
  return cls;
}

function accountBlock(ctx, acct, ar, catRep) {
  const b = ctx.b;
  const baseCur = b.base_currency || (ctx.r && ctx.r.base_currency) || '';
  const details = Array.isArray(acct.details) ? acct.details : [];
  const rows = ar && Array.isArray(ar.rows) ? ar.rows : [];

  // The base an adjustment applies to: lines above it, back to the previous
  // counted adjustment (subtotal rows don't reset it), as the engine does.
  const adjBase = new Map();
  if (ar) {
    let since = 0;
    details.forEach((d, i) => {
      const rr = rows[i];
      if (!rr) return;
      const kind = d.kind || 'line';
      if (kind === 'subtotal') return;
      if (kind === 'adjustment') {
        adjBase.set(i, since);
        since = rr.included ? 0 : since + rr.total;
      } else {
        since += rr.total;
      }
    });
  }

  const head = el('tr', { class: 'rpt-acct-head' });
  const hc = td('', 'acct', NCOLS);
  hc.append(el('span', { class: 'acct-code', text: acct.code || '' }), el('span', { class: 'acct-name', text: acct.name || '' }));
  head.append(hc);

  const trs = [];
  let suppressed = 0;
  details.forEach((d, i) => {
    const kind = d.kind || 'line';
    const rr = rows[i] || null;
    // A separator prints as a rule across the row, nothing else.
    if (kind === 'separator') {
      trs.push(el('tr', { class: 'rpt-row separator' }, td(String(i + 1), 'num no'), td('', 'rule', NCOLS - 1)));
      return;
    }
    const out = rr && !rr.included && kind !== 'subtotal';
    if (out) suppressed += 1;
    const tr = el('tr', { class: `rpt-row ${kind}${out ? ' excluded' : ''}${rowStyle(d)}` });
    tr.append(td(String(i + 1), 'num no'));
    const desc = el('td', { class: 'desc' });
    const text = d.description || (kind === 'subtotal' ? 'Subtotal' : kind === 'adjustment' ? 'Adjustment' : '');
    desc.append(el('span', { class: 'desc-text', text }));
    if (out) desc.append(el('span', { class: 'flag', text: 'Not in total' }));
    const bad = ctx.cellIssues.get(d.id);
    if (bad && bad.size) desc.append(el('span', { class: 'flag issue', text: 'Not calculated' }));
    tr.append(desc);

    if (kind === 'subtotal') {
      tr.append(td('', '', 6), td(rr ? m(rr.subtotal) : '', 'num total'), td('', 'fr'));
    } else if (kind === 'adjustment') {
      const pct = percentOf(d.amount);
      tr.append(td(pct.text, `num${pct.formula ? ' formula' : ''}`), td('of', 'unit-col'), td(''));
      tr.append(td(adjBase.has(i) ? money(adjBase.get(i)) : '', 'num'), td(''), td(''));
      tr.append(td(rr && rr.included ? m(rr.total) : '', 'num total'), td('', 'fr'));
    } else {
      tr.append(factorCell(d.amount, '', 'num'));
      tr.append(td(d.unit || '', 'unit-col'));
      tr.append(factorCell(d.x, d.unit2, 'num'));
      tr.append(factorCell(d.rate, d.unit3, 'num'));
      const cur = d.currency && d.currency !== baseCur ? d.currency : '';
      tr.append(td(cur, 'cur'));
      tr.append(factorCell(d.x4, d.unit4, 'num'));
      const tot = td(rr && rr.included ? m(rr.total) : '', 'num total');
      if (rr && rr.included && cur && typeof rr.local_total === 'number') {
        tot.append(el('span', { class: 'local', text: `${money(rr.local_total)} ${cur}` }));
      }
      tr.append(tot);
      tr.append(td(Array.isArray(d.fringes) ? d.fringes.join(', ') : '', 'fr'));
    }
    if (bad && bad.size) {
      // Cell positions: 2 amount, 4 X, 5 rate, 6 currency, 7 4X, 9 fringes.
      const at = { amount: 2, x: 4, rate: 5, currency: 6, x4: 7, fringes: 9, description: 1 };
      for (const f of bad) {
        const cell = at[f] !== undefined ? tr.children[at[f]] : null;
        if (cell && kind === 'line') cell.classList.add('issue');
      }
    }
    trs.push(tr);
  });
  if (details.length === 0) {
    const tr = el('tr', { class: 'rpt-row empty' }, td('', 'no'), td('No detail lines', 'desc muted', NCOLS - 1));
    trs.push(tr);
  }

  // Keep the account heading with its first lines.
  const lead = el('tbody', { class: 'rpt-acct rpt-keep' }, head, ...trs.slice(0, 3));
  const rest = el('tbody', { class: 'rpt-acct' }, ...trs.slice(3));

  const foot = el('tbody', { class: 'rpt-acct-foot rpt-keep' });
  if (ar) {
    foot.append(sumRow('sub first', 'Detail total', ar.details_total));
    if (ar.fringe_total) {
      const byName = new Map();
      for (const rr of rows) {
        for (const [n, c] of Object.entries((rr && rr.fringes) || {})) byName.set(n, (byName.get(n) || 0) + c);
      }
      const names = [...byName.keys()].sort();
      const detailNote = names.length > 1 ? names.map((n) => `${n} ${money(byName.get(n))}`).join(', ') : names[0] || '';
      foot.append(sumRow('sub', 'Fringes', ar.fringe_total, [detailNote, postingNote(b, catRep)].filter(Boolean).join('; ')));
    }
    foot.append(sumRow('acct-total', `Total ${acct.code || ''} ${acct.name || ''}`.trim(), ar.total));
  } else {
    foot.append(sumRow('acct-total', `Total ${acct.code || ''} ${acct.name || ''}`.trim(), null));
  }
  if (suppressed) {
    const tr = el('tr', { class: 'rpt-note-row' }, td('', 'no'));
    tr.append(td(`${suppressed === 1 ? 'One line is' : `${suppressed} lines are`} shown but not counted (a group on the line is out of the budget total).`, 'note', NCOLS - 1));
    foot.append(tr);
  }
  return [lead, rest, foot].filter((t) => t.childElementCount > 0);
}

function detailCategory(ctx, it, first) {
  const cat = it.row;
  const rep = it.rep;
  const label = [cat.code, cat.name].filter(Boolean).join('  ');
  const section = el('section', { class: `rpt-section rpt-detail${first ? ' first' : ''}` });

  const table = el('table', { class: 'rpt-table rpt-dt' });
  const cg = el('colgroup');
  for (const [, cls] of DETAIL_COLS) cg.append(el('col', { class: cls }));
  table.append(cg);
  const cols = el('tr', { class: 'rpt-cols' });
  for (const [name, , cls] of DETAIL_COLS) cols.append(th(name, cls));
  table.append(el('thead', {}, runningHead(ctx, `Detail: ${label}`, NCOLS), cols));

  const capHead = el('tbody', { class: 'rpt-cat-head' });
  const cap = td('', 'cat', NCOLS);
  cap.append(el('span', { class: 'cat-code', text: cat.code || '' }), el('span', { class: 'cat-name', text: cat.name || '' }));
  capHead.append(el('tr', {}, cap));
  table.append(capHead);

  const accounts = Array.isArray(cat.accounts) ? cat.accounts : [];
  accounts.forEach((a, ai) => {
    const ar = rep && Array.isArray(rep.accounts) ? rep.accounts[ai] || null : null;
    for (const tb of accountBlock(ctx, a || {}, ar, rep)) table.append(tb);
  });
  if (accounts.length === 0) {
    table.append(el('tbody', {}, el('tr', { class: 'rpt-row empty' }, td('', 'no'), td('No accounts', 'desc muted', NCOLS - 1))));
  }

  const foot = el('tbody', { class: 'rpt-cat-foot rpt-keep' });
  if (rep && rep.fringe_account) {
    const tr = el('tr', { class: 'rpt-sum fringe-acct first' }, td('', 'no'));
    const lab = td('', 'label', LABEL_SPAN);
    lab.append(el('span', { class: 'acct-code', text: rep.fringe_account.code || '' }), 'Fringes');
    tr.append(lab, td(m(rep.fringe_account.amount), 'num total'), td('', 'fr'));
    foot.append(tr);
  }
  foot.append(sumRow(`cat-total${rep && rep.fringe_account ? '' : ' first'}`, `Total ${label}`, rep ? rep.total : null));
  table.append(foot);

  section.append(table);
  return section;
}

function detail(ctx, view, withTitle) {
  const frag = document.createDocumentFragment();
  const cats = ctx.items.filter((it) => it.kind === 'category');
  if (withTitle) {
    const intro = el('section', { class: 'rpt-section rpt-detail-intro' });
    intro.append(titleBlock(ctx, view, 'Budget detail'));
    frag.append(intro);
  }
  cats.forEach((it, i) => frag.append(detailCategory(ctx, it, withTitle && i === 0)));
  if (cats.length === 0) frag.append(el('p', { class: 'rpt-empty', text: 'This budget has no categories yet.' }));
  return frag;
}

// ---------------------------------------------------------------------------
// Public API

/**
 * Builds a printable report into `container`, replacing what it held.
 * kind: "topsheet", "detail" or "both".
 */
export function renderReport(container, view, kind = 'both') {
  if (!container) return;
  const k = KINDS.has(kind) ? kind : 'both';
  const { b, r, items } = index(view || {});
  const ctx = {
    b,
    r,
    items,
    title: (typeof b.title === 'string' && b.title.trim()) || 'Untitled budget',
    printed: printedAt(new Date()),
    baseline: baselineOf(view || {}),
    cellIssues: cellIssues(view || {}),
  };
  const root = el('article', { class: `fwb-report kind-${k}`, attrs: { 'aria-label': `${ctx.title} report` } });
  if (k === 'topsheet' || k === 'both') root.append(topsheet(ctx, view || {}));
  if (k === 'detail' || k === 'both') root.append(detail(ctx, view || {}, k === 'detail'));
  container.replaceChildren(root);
}

let pending = null;

/**
 * Renders the report into a hidden #print-root and opens the system print
 * dialog (which on macOS offers Save as PDF). Resolves once printing is over.
 */
export function printReport(view, kind = 'both') {
  if (pending) pending();

  let root = document.getElementById('print-root');
  if (!root) {
    root = document.createElement('div');
    root.id = 'print-root';
    document.body.append(root);
  }
  renderReport(root, view, kind);

  const html = document.documentElement;
  const oldTitle = document.title;
  const t = view && view.budget && typeof view.budget.title === 'string' ? view.budget.title.trim() : '';
  if (t) document.title = t; // the default file name for Save as PDF
  html.classList.add('printing');

  return new Promise((resolve) => {
    let timer = 0;
    const cleanup = () => {
      if (pending !== cleanup) return;
      pending = null;
      clearTimeout(timer);
      window.removeEventListener('afterprint', cleanup);
      document.removeEventListener('pointerdown', cleanup, true);
      document.removeEventListener('keydown', cleanup, true);
      html.classList.remove('printing');
      document.title = oldTitle;
      root.replaceChildren();
      resolve();
    };
    pending = cleanup;
    window.addEventListener('afterprint', cleanup);

    const started = Date.now();
    try {
      // On macOS, Tauri replaces window.print() with a call to the native
      // print sheet (WebviewWindow::print), which returns a promise.
      const shown = window.print();
      if (shown && typeof shown.then === 'function') shown.catch(cleanup);
    } catch {
      cleanup();
      return;
    }
    // Some web views return from print() at once and show the dialog as a
    // sheet; others block until it closes. If it blocked, we are done. If not,
    // clean up on afterprint, on the next input after the dialog closes, or
    // after a long fallback timeout. The print root is hidden on screen either
    // way, so lingering costs nothing visible.
    if (Date.now() - started > 1000) {
      setTimeout(cleanup, 0);
      return;
    }
    setTimeout(() => {
      if (pending !== cleanup) return;
      document.addEventListener('pointerdown', cleanup, true);
      document.addEventListener('keydown', cleanup, true);
    }, 500);
    timer = setTimeout(cleanup, 10 * 60 * 1000);
  });
}
