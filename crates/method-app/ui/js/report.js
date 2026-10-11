// Printable budget reports, built from the full view (get_full_view: the
// whole budget JSON and every row's totals) and a report spec: which sections
// print and in what order, which categories and accounts, what to leave out,
// the detail columns, the layout, the header and the footer. Our own layout.
//
// The report is laid out here into Letter-sized page boxes, each with its own
// header and footer, because the system web view repeats no table headers
// and draws no page numbers when it prints. Rows are measured once, then
// dealt onto pages, keeping headings with what follows. Printing hands those
// pages to the print sheet, or straight into a PDF file (src/reports.rs);
// each box is exactly one sheet of paper.
//
// Everything from the budget goes in through textContent; no inline styles,
// so it works under the app's CSP. Sizes that depend on the spec (column
// widths, the preview's scale, the page size) are constructed-stylesheet
// rules holding only numbers (sheet.js). print.css holds the rest.
//
//   renderReport(container, view, spec)   lays the report out into container
//   printReport(view, spec, { pdf })      prints it (or saves it as a PDF)
//   saveReportPdf(view, spec)             printReport with pdf: true
//   renderHtml(container, node, opts)     lays any table or element out into pages
//   printHtml(node, opts)                 prints that (opts.pdf for a PDF)
//
// `spec` is a report spec in the file format's shape (docs/FORMAT.md, "Report
// preset"), or one of the older kinds "topsheet", "detail" or "both".

import { el } from './dom.js';
import { money, decimal } from './format.js';
import { rowColor, fringePercent, chargeLabel } from './levels.js';
import { setRules } from './sheet.js';
import { orderedNames } from './order.js';
import { invoke, connected } from './api.js';

// ---------------------------------------------------------------------------
// The spec

/** Sections in their usual order, with the names the setup dialog shows. */
export const SECTIONS = [
  { key: 'topsheet', label: 'Topsheet' },
  { key: 'accounts', label: 'Account level' },
  { key: 'detail', label: 'Detail level' },
  { key: 'fringes', label: 'Fringes' },
  { key: 'globals', label: 'Globals' },
  { key: 'groups', label: 'Groups with totals' },
  { key: 'notes', label: 'Notes list' },
  { key: 'info', label: 'Budget information' },
  { key: 'chart', label: 'Chart of accounts' },
  { key: 'units', label: 'Units' },
  { key: 'sets', label: 'Sets with totals' },
  { key: 'locations', label: 'Locations with totals' },
  { key: 'currencies', label: 'Currencies' },
  { key: 'cost_report', label: 'Cost report' },
];
const SECTION_KEYS = new Set(SECTIONS.map((s) => s.key));

// Optional detail columns in grid order. w is the column's width in pixels
// at the medium type size; figures columns (`keep`) never get narrower, the
// others give way to the description when the page is full. `#`, the
// description and the total always print.
export const REPORT_COLUMNS = [
  { key: 'amount', label: 'Amount', cls: 'num', w: 70, keep: true },
  { key: 'unit', label: 'Unit', w: 44 },
  { key: 'x', label: 'X', cls: 'num', w: 34 },
  { key: 'unit2', label: 'X unit', w: 44 },
  { key: 'rate', label: 'Rate', cls: 'num', w: 72, keep: true },
  { key: 'unit3', label: 'Rate unit', w: 46 },
  { key: 'currency', label: 'Cur', name: 'Currency', w: 34 },
  { key: 'x4', label: '4X', cls: 'num', w: 34 },
  { key: 'unit4', label: '4X unit', w: 44 },
  { key: 'fringes', label: 'Fringes', w: 66 },
  { key: 'fringe_pct', label: 'Fringe %', cls: 'num', w: 52, keep: true },
  { key: 'fringe_total', label: 'Fringe $', cls: 'num', w: 78, keep: true },
  { key: 'groups', label: 'Groups', w: 62 },
  { key: 'set', label: 'Set', w: 50 },
  { key: 'location', label: 'Location', w: 60 },
  { key: 'original', label: 'Original', cls: 'num', w: 90, keep: true, baseline: true },
  { key: 'variance', label: 'Variance', cls: 'num', w: 90, keep: true, baseline: true },
];
const COLUMN = new Map(REPORT_COLUMNS.map((c) => [c.key, c]));

/**
 * The columns a spec without its own choice prints. Original and variance
 * print only when the budget has a baseline, then on the topsheet and
 * account level too.
 */
export const STANDARD_COLUMNS = ['amount', 'unit', 'x', 'rate', 'currency', 'x4', 'fringes', 'original', 'variance'];

/** What a header or footer slot can show (`info`, `global` and `text` carry a value). */
export const BAND_FIELDS = [
  { kind: 'title', label: 'Budget title' },
  { kind: 'section', label: 'Section' },
  { kind: 'printed', label: 'Date and time printed' },
  { kind: 'date', label: 'Date printed' },
  { kind: 'time', label: 'Time printed' },
  { kind: 'file', label: 'File name' },
  { kind: 'page', label: 'Page X of Y' },
  { kind: 'info', label: 'Budget info field' },
  { kind: 'global', label: 'Global' },
  { kind: 'text', label: 'Text' },
];
const BAND_KINDS = new Set(BAND_FIELDS.map((f) => f.kind));

export const STANDARD_HEADER = { left: { kind: 'title' }, center: { kind: 'section' }, right: { kind: 'printed' } };
export const STANDARD_FOOTER = { right: { kind: 'page' } };

const FONT_SIZES = new Set(['small', 'medium', 'large']);

/** A fresh spec: the topsheet and the detail, everything in, standard layout. */
export function defaultSpec() {
  return normalizeSpec({ sections: ['topsheet', 'detail'] });
}

const KIND_SECTIONS = { topsheet: ['topsheet'], detail: ['detail'], both: ['topsheet', 'detail'] };

function strings(list) {
  return Array.isArray(list) ? [...new Set(list.filter((s) => typeof s === 'string'))] : [];
}

function band(b) {
  if (!b || typeof b !== 'object') return null;
  const out = {};
  for (const slot of ['left', 'center', 'right']) {
    const f = b[slot];
    if (!f || typeof f !== 'object' || !BAND_KINDS.has(f.kind)) continue;
    if (f.kind === 'info') out[slot] = { kind: 'info', label: String(f.label || '') };
    else if (f.kind === 'global') out[slot] = { kind: 'global', name: String(f.name || '') };
    else if (f.kind === 'text') out[slot] = { kind: 'text', text: String(f.text || '') };
    else out[slot] = { kind: f.kind };
  }
  if (b.first_page_only === true) out.first_page_only = true;
  return out;
}

/**
 * A complete spec from a partial one, a saved preset (its name and default
 * flag are dropped), or an older kind string. Unknown values are dropped.
 */
export function normalizeSpec(spec) {
  const s = typeof spec === 'string' ? { sections: KIND_SECTIONS[spec] || KIND_SECTIONS.both } : spec || {};
  const sections = strings(s.sections).filter((k) => SECTION_KEYS.has(k));
  return {
    sections: Array.isArray(s.sections) ? sections : ['topsheet', 'detail'],
    excluded_categories: strings(s.excluded_categories),
    excluded_accounts: strings(s.excluded_accounts),
    skip_empty_accounts: s.skip_empty_accounts === true,
    skip_zero_accounts: s.skip_zero_accounts === true,
    skip_suppressed_lines: s.skip_suppressed_lines === true,
    skip_notes: s.skip_notes === true,
    skip_done_notes: s.skip_done_notes === true,
    only_groups: strings(s.only_groups),
    columns: Array.isArray(s.columns) ? strings(s.columns).filter((k) => COLUMN.has(k)) : null,
    landscape: s.landscape === true,
    font_size: FONT_SIZES.has(s.font_size) ? s.font_size : 'medium',
    shading: s.shading === true,
    group_colors: s.group_colors === true,
    vertical_rules: s.vertical_rules === true,
    collapse_details: s.collapse_details === true,
    topsheet_fringes: s.topsheet_fringes === true,
    topsheet_pages: s.topsheet_pages === true,
    header: band(s.header),
    footer: band(s.footer),
  };
}

/**
 * The spec in the file's shape, fields at their default left out, ready to
 * save as a preset (with `name` and `default` added).
 */
export function compactSpec(spec) {
  const s = normalizeSpec(spec);
  const out = {};
  if (s.sections.length) out.sections = s.sections;
  for (const k of ['excluded_categories', 'excluded_accounts']) if (s[k].length) out[k] = [...s[k]].sort();
  for (const k of ['skip_empty_accounts', 'skip_zero_accounts', 'skip_suppressed_lines', 'skip_notes', 'skip_done_notes']) if (s[k]) out[k] = true;
  if (s.only_groups.length) out.only_groups = [...s.only_groups].sort();
  if (s.columns) out.columns = s.columns;
  if (s.landscape) out.landscape = true;
  if (s.font_size !== 'medium') out.font_size = s.font_size;
  if (s.shading) out.shading = true;
  if (s.group_colors) out.group_colors = true;
  for (const k of ['vertical_rules', 'collapse_details', 'topsheet_fringes', 'topsheet_pages']) if (s[k]) out[k] = true;
  if (s.header) out.header = s.header;
  if (s.footer) out.footer = s.footer;
  return out;
}

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
  const whole = (int + padded.slice(0, 2)).replace(/^0+(?=\d)/, '');
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

function printedDate(now) {
  try {
    return now.toLocaleDateString(undefined, { year: 'numeric', month: 'long', day: 'numeric' });
  } catch {
    return now.toISOString().slice(0, 10);
  }
}

function printedTime(now) {
  try {
    return now.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' });
  } catch {
    return now.toISOString().slice(11, 16);
  }
}

const HEX = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i;

/** "#AbC" -> "abbcc"... as six lowercase hex digits, or null. */
function hex6(c) {
  if (typeof c !== 'string' || !HEX.test(c)) return null;
  const h = c.slice(1).toLowerCase();
  return h.length === 3 ? h.replace(/./g, (x) => x + x) : h;
}

function infoText(v) {
  if (!v || typeof v !== 'object') return '';
  return v.kind === 'number' ? decimal(v.value) : String(v.value ?? '');
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
  const mapOf = (o) => {
    const out = new Map();
    if (o && typeof o === 'object') {
      for (const [k, c] of Object.entries(o)) {
        const ch = change(c);
        if (ch) out.set(k, ch);
      }
    }
    return out;
  };
  return {
    categories: mapOf(src.categories),
    accounts: mapOf(src.accounts),
    rows: mapOf(src.rows),
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

// ---------------------------------------------------------------------------
// What prints: categories, accounts and lines after the spec's choices

function catLabel(cat) {
  return [cat.code, cat.name].filter(Boolean).join('  ');
}

/** A category's accounts that print in the account and detail sections. */
function accountsOf(ctx, it) {
  const s = ctx.spec;
  const cat = it.row;
  if (ctx.exCats.has(cat.code || '')) return [];
  const accounts = Array.isArray(cat.accounts) ? cat.accounts : [];
  const out = [];
  accounts.forEach((a, ai) => {
    const acct = a || {};
    const ar = it.rep && Array.isArray(it.rep.accounts) ? it.rep.accounts[ai] || null : null;
    if (ctx.exAccts.has(acct.code || '')) return;
    const details = Array.isArray(acct.details) ? acct.details : [];
    if (s.skip_empty_accounts && details.length === 0) return;
    if (s.skip_zero_accounts && ar && ar.total === 0) return;
    out.push({ acct, ar, ai });
  });
  return out;
}

/** Whether a detail row prints, given the spec's line filters. */
function lineShown(ctx, d, rr) {
  const kind = d.kind || 'line';
  if (ctx.spec.skip_suppressed_lines && rr && !rr.included && (kind === 'line' || kind === 'adjustment')) return false;
  if (ctx.onlyGroups.size) {
    if (kind === 'subtotal' || kind === 'separator') return false;
    return (Array.isArray(d.groups) ? d.groups : []).some((g) => ctx.onlyGroups.has(g));
  }
  return true;
}

/**
 * The notes on a line, account or category that print: not blank, and not
 * ticked off as reviewed when the spec leaves those out.
 */
function printedNotes(ctx, notes) {
  if (!Array.isArray(notes)) return [];
  return notes.filter((n) => n && typeof n.text === 'string' && n.text.trim() && !(ctx.spec.skip_done_notes && n.done));
}

const NOTE_DATE = (() => {
  try {
    return new Intl.DateTimeFormat(undefined, { dateStyle: 'medium' });
  } catch {
    return null;
  }
})();

/** "9 Oct 2026 · Alex · reviewed": when a note was written, by whom, and whether it is ticked off. */
function noteMeta(n) {
  const parts = [];
  const d = n.created ? new Date(n.created) : null;
  if (d && !Number.isNaN(d.getTime()) && NOTE_DATE) parts.push(NOTE_DATE.format(d));
  else if (n.created) parts.push(String(n.created));
  if (n.author) parts.push(String(n.author));
  if (n.done) parts.push('reviewed');
  return parts.join(' · ');
}

/**
 * Footnote numbers for the notes on printed lines, in budget order, so the
 * detail pages and the notes list agree whichever prints first.
 */
function noteNumbers(ctx) {
  const out = new Map();
  if (ctx.spec.skip_notes || ctx.spec.collapse_details || !ctx.spec.sections.includes('detail')) return out;
  let n = 0;
  for (const it of ctx.items) {
    if (it.kind !== 'category') continue;
    for (const { acct, ar } of accountsOf(ctx, it)) {
      const details = Array.isArray(acct.details) ? acct.details : [];
      const rows = ar && Array.isArray(ar.rows) ? ar.rows : [];
      let shown = 0;
      details.forEach((d, i) => {
        if (lineShown(ctx, d, rows[i] || null)) shown += 1;
      });
      if (ctx.onlyGroups.size && shown === 0) continue;
      details.forEach((d, i) => {
        if (d && printedNotes(ctx, d.notes).length && lineShown(ctx, d, rows[i] || null)) {
          n += 1;
          out.set(d.id, n);
        }
      });
    }
  }
  return out;
}

// ---------------------------------------------------------------------------
// Sections. Each returns flows: { name, cls, cols, head, rows, intro }, where
// rows are [{ el, keep }] and `keep` holds a row on the same page as the next.

function row(el, keep = false) {
  return { el, keep };
}

function colgroup(cols) {
  return el('colgroup', {}, ...cols.map((c) => el('col', { class: c })));
}

function warning(view) {
  const r = view && view.report;
  if (r && !view.partial) return null;
  const e = view && view.error;
  const where = e ? [e.at, e.message].filter(Boolean).join(': ') : '';
  const head = r ? 'Totals may be incomplete.' : 'Totals could not be calculated. Amounts are left blank.';
  const tail = r ? ' Cells that cannot be calculated are counted as blank.' : '';
  return el('div', { class: 'rpt-warning', attrs: { role: 'note' } }, el('strong', { text: head }), where ? ` ${where}.` : '', tail);
}

function titleBlock(ctx, subtitle) {
  const b = ctx.b;
  const r = ctx.r;
  const facts = [];
  facts.push(['Base currency', b.base_currency || (r && r.base_currency) || '']);
  const tc = b.totals_currency || (r && r.totals_currency !== r.base_currency ? r.totals_currency : '');
  if (tc) facts.push(['Totals currency', tc]);
  const info = b.info && typeof b.info === 'object' ? b.info : {};
  for (const [label, v] of Object.entries(info)) {
    if (!v || typeof v !== 'object') continue;
    facts.push([label, infoText(v)]);
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
    warning(ctx.view),
  );
}

/** The figures columns of the topsheet and account level: original, variance. */
function baseCols(ctx) {
  return ctx.baseline ? ['original', 'variance'].filter((k) => ctx.cols.includes(k)) : [];
}

/** Original and variance cells; variance falls back to total minus original. */
function baseCells(ctx, keys, total, ch) {
  const orig = ch ? ch.original : null;
  let diff = ch ? ch.variance : null;
  if (diff === null && orig !== null && typeof total === 'number') diff = total - orig;
  return keys.map((k) => (k === 'original' ? td(m(orig), 'num base') : td(m(diff), 'num var')));
}

function topsheet(ctx) {
  const r = ctx.r;
  const base = ctx.baseline;
  const bc = baseCols(ctx);
  // Optional columns: the page each category's detail starts on, its fringes.
  const pages = ctx.spec.topsheet_pages;
  const fringes = ctx.spec.topsheet_fringes;
  const cols = ['c-code', 'c-name', ...(pages ? ['c-page'] : []), ...(fringes ? ['c-money'] : []), 'c-money', ...bc.map(() => 'c-money')];
  const head = el(
    'tr',
    { class: 'rpt-cols' },
    th('Acct #'),
    th('Description'),
    pages ? th('Page', 'num') : null,
    fringes ? th('Fringes', 'num') : null,
    th('Total', 'num'),
    ...bc.map((k) => th(COLUMN.get(k).label, 'num')),
  );
  const rows = [];
  // extra: { page: category index whose detail page goes here, fringes: cents }
  const line = (cls, code, label, total, ch, keep = false, extra = {}) => {
    const tr = el('tr', { class: cls }, td(code, 'code'), td(label, 'name'));
    if (pages) {
      const cell = td('', 'num page');
      if (extra.page !== undefined) ctx.pageCells.push({ ci: extra.page, cell });
      tr.append(cell);
    }
    if (fringes) tr.append(td(extra.fringes ? money(extra.fringes) : '', 'num muted-fig'));
    tr.append(td(m(total), 'num'), ...baseCells(ctx, bc, total, ch));
    rows.push(row(tr, keep));
  };
  const baseFor = (code) => (base && base.categories.get(code)) || null;
  const catCodes = [];
  for (const it of ctx.items) {
    if (it.kind === 'category') {
      catCodes[it.ci] = it.row.code;
      line('cat', it.row.code || '', it.row.name || '', it.rep ? it.rep.total : null, baseFor(it.row.code), false, { page: it.ci, fringes: it.rep ? it.rep.fringe_total : 0 });
    } else {
      if (it.rep && it.rep.posted_fringes) line('posted', '', 'Fringes', it.rep.posted_fringes, null, true);
      // A level's original only when every category in it has one.
      let lvBase = null;
      if (base && it.rep) {
        let sum = 0;
        let complete = it.rep.end > it.rep.start;
        for (let i = it.rep.start; i < it.rep.end; i++) {
          const v = baseFor(catCodes[i]);
          if (!v || v.original === null) complete = false;
          else sum += v.original;
        }
        if (complete && !it.rep.posted_fringes) lvBase = { original: sum, variance: null };
      }
      // A level's fringes: what its categories' lines earned, wherever posted.
      let lvFringes = 0;
      if (fringes && it.rep && r && Array.isArray(r.categories)) {
        for (let i = it.rep.start; i < it.rep.end; i++) lvFringes += (r.categories[i] && r.categories[i].fringe_total) || 0;
      }
      line('level', '', it.row.name || '', it.rep ? it.rep.total : null, lvBase, false, { fringes: lvFringes });
    }
  }
  // The summary stays together.
  const sum = [];
  const sline = (cls, label, total, ch) => sum.push([cls, label, total, ch]);
  if (r) {
    sline('sum first', 'Total above-the-line', r.total_atl, null);
    sline('sum', 'Total below-the-line', r.total_btl, null);
    sline('sum strong', 'Total above and below-the-line', r.atl_btl, null);
    if (r.budget_fringes) sline('sum', 'Total fringes', r.budget_fringes, null);
    (r.charges || []).forEach((c, i) => sline('sum charge', chargeLabel(ctx.b, c, i, money), c.amount, null));
    sline('sum grand', 'Grand total', r.grand_total, base ? base.grand_total : null);
    const credits = r.credits || [];
    for (const c of credits) sline('sum credit', `Less ${c.name || 'credit'}`, -c.amount, null);
    if (credits.length) sline('sum grand', 'Net total', r.net_total, base ? base.net_total : null);
    if (r.display) {
      const cur = r.totals_currency;
      const rate = `1 ${cur} = ${decimal(r.totals_rate)} ${r.base_currency}`;
      sline('sum display first', `Grand total in ${cur} (${rate})`, r.display.grand_total, null);
      if (credits.length) sline('sum display', `Net total in ${cur}`, r.display.net_total, null);
    }
  } else {
    sline('sum first', 'Totals unavailable', null, null);
  }
  sum.forEach(([cls, label, total, ch], i) => line(cls, '', label, total, ch, i < sum.length - 1));
  if (ctx.items.length === 0) rows.unshift(row(el('tr', { class: 'rpt-row empty' }, td('This budget has no categories yet.', 'muted', cols.length))));
  return [{ name: 'Topsheet', cls: `rpt-ts${bc.length ? ' has-baseline' : ''}${pages || fringes ? ' has-extra' : ''}`, cols, head, rows }];
}

function accountLevel(ctx) {
  const bc = baseCols(ctx);
  const cols = ['c-code', 'c-name', 'c-money', 'c-money', ...bc.map(() => 'c-money')];
  const span = cols.length;
  const head = el('tr', { class: 'rpt-cols' }, th('Acct #'), th('Description'), th('Fringes', 'num'), th('Total', 'num'), ...bc.map((k) => th(COLUMN.get(k).label, 'num')));
  const rows = [];
  const base = ctx.baseline;
  const cats = ctx.items.filter((it) => it.kind === 'category');
  let any = false;
  for (const it of cats) {
    const accts = accountsOf(ctx, it);
    if (!accts.length && (ctx.exCats.has(it.row.code || '') || ctx.filtered)) continue;
    any = true;
    const cap = td('', 'cat', span);
    cap.append(el('span', { class: 'cat-code', text: it.row.code || '' }), el('span', { class: 'cat-name', text: it.row.name || '' }));
    rows.push(row(el('tr', { class: 'rpt-al-cat' }, cap), true));
    accts.forEach(({ acct, ar }, k) => {
      const tr = el('tr', { class: 'rpt-al-acct' }, td(acct.code || '', 'code'), td(acct.name || '', 'name'));
      tr.append(td(ar && ar.fringe_total ? money(ar.fringe_total) : '', 'num muted'), td(ar ? m(ar.total) : '', 'num'));
      tr.append(...baseCells(ctx, bc, ar ? ar.total : null, base ? base.accounts.get(acct.code) || null : null));
      rows.push(row(tr, k === 0));
    });
    if (!accts.length) rows.push(row(el('tr', { class: 'rpt-al-acct empty' }, td('', 'code'), td('No accounts in this report', 'name muted', span - 1)), true));
    const rep = it.rep;
    if (rep && rep.fringe_account) {
      const tr = el('tr', { class: 'rpt-al-fa' }, td(rep.fringe_account.code || '', 'code'), td('Fringes', 'name'), td('', 'num'), td(m(rep.fringe_account.amount), 'num'), ...bc.map(() => td('', 'num')));
      rows.push(row(tr, true));
    }
    const tot = el('tr', { class: 'rpt-al-total' }, td('', 'code'), td(`Total ${catLabel(it.row)}`, 'name'));
    tot.append(td(rep ? m(rep.fringe_total) : '', 'num muted'), td(rep ? m(rep.total) : '', 'num'));
    tot.append(...baseCells(ctx, bc, rep ? rep.total : null, base ? base.categories.get(it.row.code) || null : null));
    rows.push(row(tot));
  }
  if (!any) rows.push(row(el('tr', { class: 'rpt-row empty' }, td('No categories in this report.', 'muted', span))));
  return [{ name: 'Account level', cls: `rpt-al${bc.length ? ' has-baseline' : ''}`, cols, head, rows }];
}

// --- Detail ---

/** The detail columns: { key, label, cls, w } for every printed column, in order. */
function detailColumns(ctx) {
  const chosen = ctx.cols.filter((k) => COLUMN.has(k) && (!COLUMN.get(k).baseline || ctx.baseline));
  // One line per account has no line figures to show, only totals.
  const mid = ctx.spec.collapse_details ? [] : chosen.filter((k) => !COLUMN.get(k).baseline).map((k) => COLUMN.get(k));
  const after = chosen.filter((k) => COLUMN.get(k).baseline).map((k) => COLUMN.get(k));
  return [
    { key: 'no', label: '#', cls: 'num', w: 28, keep: true },
    { key: 'description', label: 'Description', w: 0 },
    ...mid,
    { key: 'total', label: 'Total', cls: 'num', w: 100, keep: true },
    ...after,
  ];
}

const FONT_SCALE = { small: 7.5 / 8.5, medium: 1, large: 10 / 8.5 };
const MIN_DESCRIPTION = 130;

/**
 * Width rules for the detail columns (numbers only, as percentages of the
 * page): every column at its width for the type size; when that leaves the
 * description less than its minimum, the columns that may give way shrink
 * (to 45% at most) and the money columns keep theirs. Only when even that
 * can't fit does everything shrink together.
 */
function detailWidths(cols, spec) {
  const width = pageSize(spec.landscape).w - 2 * MARGIN.side;
  const k = FONT_SCALE[spec.font_size] || 1;
  const fixed = cols.filter((c) => c.key !== 'description');
  const keep = fixed.filter((c) => c.keep).reduce((a, c) => a + c.w * k, 0);
  const flex = fixed.filter((c) => !c.keep).reduce((a, c) => a + c.w * k, 0);
  const room = width - MIN_DESCRIPTION;
  let fFlex = 1;
  if (keep + flex > room && flex > 0) fFlex = Math.max(0.45, (room - keep) / flex);
  const used = keep + flex * fFlex;
  const all = used > width - 60 ? (width - 60) / used : 1;
  const rules = fixed.map((c) => {
    const px = c.w * k * (c.keep ? 1 : fFlex) * all;
    return `.method-report .rpt-dt col.k-${c.key} { width: ${((px / width) * 100).toFixed(3)}%; }`;
  });
  rules.push('.method-report .rpt-dt col.k-description { width: auto; }');
  return rules.join('\n');
}

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

/** Text style classes for a detail row (bold, italic, indent 1 to 4), each with a leading space. */
function rowStyle(d) {
  let cls = '';
  if (d.bold === true) cls += ' row-bold';
  if (d.italic === true) cls += ' row-italic';
  const indent = Number.isInteger(d.indent) ? Math.min(Math.max(d.indent, 0), 4) : 0;
  if (indent) cls += ` indent-${indent}`;
  return cls;
}

/** Notes longer than this print cut short under their line; the notes list has them whole. */
const FOOTNOTE_CHARS = 700;

function detailCells(ctx, dc, d, rr, kind, adjBase, i) {
  const baseCur = ctx.b.base_currency || (ctx.r && ctx.r.base_currency) || '';
  const shown = new Set(dc.map((c) => c.key));
  const cur = d.currency && d.currency !== baseCur ? d.currency : '';
  const ch = ctx.baseline ? ctx.baseline.rows.get(String(d.id)) || null : null;
  const counted = rr && rr.included;
  const cells = [];
  for (const c of dc) {
    const k = c.key;
    const cls = c.cls || '';
    let cell;
    if (k === 'no') {
      cell = td(String(i + 1), 'num no');
    } else if (k === 'description') {
      cell = el('td', { class: 'desc' });
      const text = d.description || (kind === 'subtotal' ? 'Subtotal' : kind === 'adjustment' ? 'Adjustment' : '');
      cell.append(el('span', { class: 'desc-text', text }));
      const n = ctx.notes.get(d.id);
      if (n) cell.append(el('sup', { class: 'rpt-fn', text: String(n) }));
      if (rr && !rr.included && kind !== 'subtotal') cell.append(el('span', { class: 'flag', text: 'Not in total' }));
      const bad = ctx.cellIssues.get(d.id);
      if (bad && bad.size) cell.append(el('span', { class: 'flag issue', text: 'Not calculated' }));
    } else if (k === 'total') {
      if (kind === 'subtotal') cell = td(rr ? m(rr.subtotal) : '', 'num total');
      else {
        cell = td(counted ? m(rr.total) : '', 'num total');
        if (counted && cur && typeof rr.local_total === 'number' && kind === 'line') {
          cell.append(el('span', { class: 'local', text: `${money(rr.local_total)} ${cur}` }));
        }
      }
    } else if (kind === 'subtotal') {
      cell = td('', cls);
    } else if (k === 'original' || k === 'variance') {
      const v = ch ? (k === 'original' ? ch.original : ch.variance) : null;
      cell = td(m(v), `num ${k === 'original' ? 'base' : 'var'}`);
    } else if (kind === 'adjustment') {
      if (k === 'amount') {
        const pct = percentOf(d.amount);
        cell = td(pct.text, `num${pct.formula ? ' formula' : ''}`);
      } else if (k === 'unit') cell = td('of', 'unit-col');
      else if (k === 'rate') cell = td(adjBase !== undefined ? money(adjBase) : '', 'num adj-base');
      else if (k === 'fringes') cell = td(Array.isArray(d.fringes) ? d.fringes.join(', ') : '', 'fr');
      else if (k === 'fringe_total') cell = td(rr && rr.fringe_total ? money(rr.fringe_total) : '', 'num');
      else if (k === 'fringe_pct') cell = td(rr && rr.fringe_total ? fringePercent(rr.fringe_total, rr.total) : '', 'num');
      else if (k === 'groups') cell = td(Array.isArray(d.groups) ? d.groups.join(', ') : '', 'txt');
      else cell = td('', cls);
    } else {
      switch (k) {
        case 'amount':
          cell = factorCell(d.amount, '', 'num');
          break;
        case 'unit':
          cell = td(d.unit || '', 'unit-col');
          break;
        case 'x':
          cell = factorCell(d.x, shown.has('unit2') ? '' : d.unit2, 'num');
          break;
        case 'rate':
          cell = factorCell(d.rate, shown.has('unit3') ? '' : d.unit3, 'num');
          break;
        case 'x4':
          cell = factorCell(d.x4, shown.has('unit4') ? '' : d.unit4, 'num');
          break;
        case 'unit2':
        case 'unit3':
        case 'unit4':
          cell = td(d[k] || '', 'unit-col');
          break;
        case 'currency':
          cell = td(cur, 'cur');
          break;
        case 'fringes':
          cell = td(Array.isArray(d.fringes) ? d.fringes.join(', ') : '', 'fr');
          break;
        case 'fringe_pct':
          cell = td(rr && rr.fringe_total ? fringePercent(rr.fringe_total, rr.total) : '', 'num');
          break;
        case 'fringe_total':
          cell = td(rr && rr.fringe_total ? money(rr.fringe_total) : '', 'num');
          break;
        case 'groups':
          cell = td(Array.isArray(d.groups) ? d.groups.join(', ') : '', 'txt');
          break;
        case 'set':
          cell = td(d.set || '', 'txt');
          break;
        case 'location':
          cell = td(d.location || '', 'txt');
          break;
        default:
          cell = td('', cls);
      }
    }
    cell.dataset.k = k;
    cells.push(cell);
  }
  const bad = ctx.cellIssues.get(d.id);
  if (bad && bad.size && kind === 'line') {
    for (const cell of cells) if (bad.has(cell.dataset.k)) cell.classList.add('issue');
  }
  for (const cell of cells) delete cell.dataset.k;
  return cells;
}

/** A totals row: label across the middle columns, then the total and original/variance. */
function sumRow(dc, cls, label, cents, note, ch) {
  const ti = dc.findIndex((c) => c.key === 'total');
  const tr = el('tr', { class: `rpt-sum ${cls}` }, td('', 'no'));
  const lab = td(label, 'label', ti - 1);
  if (note) lab.append(el('span', { class: 'note', text: ` (${note})` }));
  tr.append(lab, td(m(cents), 'num total'));
  for (const c of dc.slice(ti + 1)) {
    if (!ch) tr.append(td('', 'num'));
    else {
      let v = c.key === 'original' ? ch.original : ch.variance;
      if (c.key === 'variance' && v === null && ch.original !== null && typeof cents === 'number') v = cents - ch.original;
      tr.append(td(m(v), `num ${c.key === 'original' ? 'base' : 'var'}`));
    }
  }
  return tr;
}

function accountRows(ctx, dc, acct, ar, catRep) {
  const b = ctx.b;
  const span = dc.length;
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
      if (kind === 'subtotal' || kind === 'separator') return;
      if (kind === 'adjustment') {
        adjBase.set(i, since);
        since = rr.included ? 0 : since + rr.total;
      } else {
        since += rr.total;
      }
    });
  }

  const out = [];
  const hc = td('', 'acct', span);
  hc.append(el('span', { class: 'acct-code', text: acct.code || '' }), el('span', { class: 'acct-name', text: acct.name || '' }));
  out.push(row(el('tr', { class: 'rpt-acct-head' }, hc), true));

  const lines = [];
  const footnotes = [];
  let suppressed = 0;
  let hidden = 0;
  let alt = false;
  details.forEach((d0, i) => {
    const d = d0 || {};
    const kind = d.kind || 'line';
    const rr = rows[i] || null;
    if (!lineShown(ctx, d, rr)) {
      hidden += 1;
      return;
    }
    if (kind === 'separator') {
      lines.push(el('tr', { class: 'rpt-row separator' }, td(String(i + 1), 'num no'), td('', 'rule', span - 1)));
      return;
    }
    const out2 = rr && !rr.included && kind !== 'subtotal';
    if (out2) suppressed += 1;
    let cls = `rpt-row ${kind}${out2 ? ' excluded' : ''}${rowStyle(d)}`;
    if (kind === 'line') {
      if (alt) cls += ' alt';
      alt = !alt;
    }
    if (ctx.spec.group_colors) {
      const c = hex6(rowColor(d, ctx.b.groups || {}));
      if (c) {
        cls += ` tc-${c}`;
        ctx.colors.add(c);
      }
    }
    const tr = el('tr', { class: cls }, ...detailCells(ctx, dc, d, rr, kind, adjBase.get(i), i));
    lines.push(tr);
    const n = ctx.notes.get(d.id);
    if (n) {
      // Every printed note on the line, each with its date, cut short together.
      const text = printedNotes(ctx, d.notes)
        .map((x) => {
          const meta = noteMeta(x);
          return meta ? `${x.text.trim()} (${meta})` : x.text.trim();
        })
        .join('\n');
      const cut = text.length > FOOTNOTE_CHARS ? `${text.slice(0, FOOTNOTE_CHARS).trimEnd()}... (in full in the notes list)` : text;
      const cell = td('', 'fn-text', span - 1);
      cell.append(el('span', { class: 'rpt-fn', text: String(n) }), el('span', { class: 'fn-body', text: cut }));
      footnotes.push(el('tr', { class: 'rpt-fnote' }, td('', 'no'), cell));
    }
  });
  if (lines.length === 0) {
    lines.push(el('tr', { class: 'rpt-row empty' }, td('', 'no'), td(details.length ? 'No lines in this report' : 'No detail lines', 'desc muted', span - 1)));
  }
  // Keep the heading with the first two lines, and the last line with the totals.
  lines.forEach((tr, k) => out.push(row(tr, k < 2 || k === lines.length - 1)));

  const foot = [];
  const ch = ctx.baseline ? ctx.baseline.accounts.get(acct.code) || null : null;
  if (ar) {
    foot.push(sumRow(dc, 'sub first', 'Detail total', ar.details_total));
    if (ar.fringe_total) {
      const byName = new Map();
      for (const rr of rows) {
        for (const [n, c] of Object.entries((rr && rr.fringes) || {})) byName.set(n, (byName.get(n) || 0) + c);
      }
      const names = [...byName.keys()].sort();
      const detailNote = names.length > 1 ? names.map((n) => `${n} ${money(byName.get(n))}`).join(', ') : names[0] || '';
      foot.push(sumRow(dc, 'sub', 'Fringes', ar.fringe_total, [detailNote, postingNote(b, catRep)].filter(Boolean).join('; ')));
    }
    foot.push(sumRow(dc, 'acct-total', `Total ${acct.code || ''} ${acct.name || ''}`.trim(), ar.total, null, ch));
  } else {
    foot.push(sumRow(dc, 'acct-total', `Total ${acct.code || ''} ${acct.name || ''}`.trim(), null));
  }
  const notes = [];
  if (suppressed) {
    notes.push(`${suppressed === 1 ? 'One line is' : `${suppressed} lines are`} shown but not counted (a group on the line is out of the budget total).`);
  }
  if (hidden) {
    notes.push(`${hidden === 1 ? 'One row is' : `${hidden} rows are`} left out of this report; the totals include ${hidden === 1 ? 'it' : 'them'}.`);
  }
  for (const text of notes) foot.push(el('tr', { class: 'rpt-note-row' }, td('', 'no'), td(text, 'note', span - 1)));
  foot.forEach((tr, k) => out.push(row(tr, k < foot.length - 1 || footnotes.length > 0)));
  footnotes.forEach((tr, k) => out.push(row(tr, k === 0 && footnotes.length === 1)));
  return out;
}

/**
 * An account collapsed to one line (the "one line per account" layout): its
 * code and name across the line's columns, then its total and, with a
 * baseline, its original and variance. `k` is its place in the category,
 * for shading.
 */
function accountLine(ctx, dc, acct, ar, k) {
  const ti = dc.findIndex((c) => c.key === 'total');
  const lab = td('', 'desc', ti);
  lab.append(el('span', { class: 'acct-code', text: acct.code || '' }), el('span', { class: 'desc-text', text: acct.name || '' }));
  const tr = el('tr', { class: `rpt-row line acct-line${k % 2 ? ' alt' : ''}` }, lab, td(ar ? m(ar.total) : '', 'num total'));
  const ch = ctx.baseline ? ctx.baseline.accounts.get(acct.code) || null : null;
  for (const c of dc.slice(ti + 1)) {
    let v = ch ? (c.key === 'original' ? ch.original : ch.variance) : null;
    if (c.key === 'variance' && v === null && ch && ch.original !== null && ar && typeof ar.total === 'number') v = ar.total - ch.original;
    tr.append(td(m(v), `num ${c.key === 'original' ? 'base' : 'var'}`));
  }
  return tr;
}

function detail(ctx) {
  const dc = detailColumns(ctx);
  ctx.detailWidths = detailWidths(dc, ctx.spec);
  const cols = dc.map((c) => `k-${c.key}`);
  const head = () => el('tr', { class: 'rpt-cols' }, ...dc.map((c) => th(c.label, c.cls || '')));
  const flows = [];
  const cats = ctx.items.filter((it) => it.kind === 'category');
  for (const it of cats) {
    const cat = it.row;
    if (ctx.exCats.has(cat.code || '')) continue;
    const rep = it.rep;
    const rows = [];
    const cap = td('', 'cat', dc.length);
    cap.append(el('span', { class: 'cat-code', text: cat.code || '' }), el('span', { class: 'cat-name', text: cat.name || '' }));
    const capRow = el('tr', { class: 'rpt-cat-head' }, cap);
    ctx.catHeads.set(capRow, it.ci);
    rows.push(row(capRow, true));
    let printed = 0;
    for (const { acct, ar } of accountsOf(ctx, it)) {
      if (ctx.onlyGroups.size) {
        const details = Array.isArray(acct.details) ? acct.details : [];
        const rr = ar && Array.isArray(ar.rows) ? ar.rows : [];
        if (!details.some((d, i) => lineShown(ctx, d || {}, rr[i] || null))) continue;
      }
      if (ctx.spec.collapse_details) rows.push(row(accountLine(ctx, dc, acct, ar, printed), false));
      else rows.push(...accountRows(ctx, dc, acct, ar, rep));
      printed += 1;
    }
    if (printed === 0 && ctx.filtered) continue;
    if (printed === 0) {
      const accounts = Array.isArray(cat.accounts) ? cat.accounts : [];
      rows.push(row(el('tr', { class: 'rpt-row empty' }, td('', 'no'), td(accounts.length ? 'No accounts in this report' : 'No accounts', 'desc muted', dc.length - 1)), true));
    }
    const foot = [];
    if (rep && rep.fringe_account) {
      const ti = dc.findIndex((c) => c.key === 'total');
      const tr = el('tr', { class: 'rpt-sum fringe-acct first' }, td('', 'no'));
      const lab = td('', 'label', ti - 1);
      lab.append(el('span', { class: 'acct-code', text: rep.fringe_account.code || '' }), 'Fringes');
      tr.append(lab, td(m(rep.fringe_account.amount), 'num total'), ...dc.slice(ti + 1).map(() => td('', 'num')));
      foot.push(tr);
    }
    const ch = ctx.baseline ? ctx.baseline.categories.get(cat.code) || null : null;
    foot.push(sumRow(dc, `cat-total${rep && rep.fringe_account ? '' : ' first'}`, `Total ${catLabel(cat)}`, rep ? rep.total : null, null, ch));
    foot.forEach((tr, k) => rows.push(row(tr, k < foot.length - 1)));
    flows.push({ name: `Detail: ${catLabel(cat)}`, cls: 'rpt-dt', cols, head: head(), rows });
  }
  // One line per account: the categories run on, one after another, rather
  // than each starting a page.
  if (ctx.spec.collapse_details && flows.length) {
    return [{ name: 'Detail, one line per account', cls: 'rpt-dt collapsed', cols, head: head(), rows: flows.flatMap((f) => f.rows) }];
  }
  if (flows.length === 0) {
    flows.push({ name: 'Detail', cls: 'rpt-dt', cols, head: head(), rows: [row(el('tr', { class: 'rpt-row empty' }, td('No categories in this report.', 'muted', dc.length)))] });
  }
  return flows;
}

// --- Setup tables ---

function simpleTable(name, cls, spec, rows) {
  const cols = spec.map((c) => c[1]);
  const head = el('tr', { class: 'rpt-cols' }, ...spec.map(([label, , thCls]) => th(label, thCls || '')));
  return { name, cls: `rpt-st ${cls}`, cols, head, rows };
}

function emptyRow(text, span) {
  return row(el('tr', { class: 'rpt-row empty' }, td(text, 'muted', span)));
}

function fringesSection(ctx) {
  const fr = ctx.b.fringes && typeof ctx.b.fringes === 'object' ? ctx.b.fringes : {};
  const sums = (ctx.r && ctx.r.fringes) || {};
  const spec = [
    ['Fringe', 'c-name2'],
    ['Description', 'c-desc'],
    ['Type', 'c-short'],
    ['Rate', 'c-num', 'num'],
    ['Cutoff or unit', 'c-mid'],
    ['Lines', 'c-short', 'num'],
    ['Total', 'c-money', 'num'],
  ];
  const rows = [];
  let total = 0;
  for (const name of orderedNames(ctx.b, 'fringes')) {
    const f = fr[name] || {};
    const sum = sums[name] || null;
    const pct = f.kind !== 'flat';
    const rate = formula(f.rate);
    const limit = pct ? (typeof f.cutoff === 'number' ? `Up to ${money(f.cutoff)}` : '') : f.unit ? `Per ${f.unit}` : 'Per line';
    rows.push(row(el('tr', { class: 'rpt-row' }, td(name, 'name'), td(f.description || '', 'txt'), td(pct ? 'Percent' : 'Flat', 'txt'), td(rate.text ? (pct ? `${rate.text}%` : rate.text) : '', `num${rate.formula ? ' formula' : ''}`), td(limit, 'txt'), td(sum ? String(sum.uses) : '', 'num'), td(sum ? m(sum.total) : '', 'num'))));
    if (sum) total += sum.total;
  }
  if (!rows.length) rows.push(emptyRow('This budget has no fringes.', spec.length));
  else if (ctx.r) rows.push(row(el('tr', { class: 'rpt-st-total' }, td('Total fringes', 'name', spec.length - 1), td(money(total), 'num'))));
  return [simpleTable('Fringes', 'rpt-fringes', spec, rows)];
}

/** How many detail rows name each global in a formula. */
function globalUses(ctx, names) {
  const counts = new Map(names.map((n) => [n, 0]));
  if (!names.length) return counts;
  const lower = new Map(names.map((n) => [n.toLowerCase(), n]));
  const word = /[A-Za-z_][A-Za-z0-9_]*/g;
  for (const it of ctx.items) {
    if (it.kind !== 'category') continue;
    for (const a of Array.isArray(it.row.accounts) ? it.row.accounts : []) {
      for (const d of Array.isArray(a && a.details) ? a.details : []) {
        const seen = new Set();
        for (const f of [d.amount, d.x, d.rate, d.x4]) {
          if (typeof f !== 'string') continue;
          for (const w of f.match(word) || []) {
            const n = lower.get(w.toLowerCase());
            if (n) seen.add(n);
          }
        }
        for (const n of seen) counts.set(n, counts.get(n) + 1);
      }
    }
  }
  return counts;
}

function globalsSection(ctx) {
  const g = ctx.b.globals && typeof ctx.b.globals === 'object' ? ctx.b.globals : {};
  const values = (ctx.r && ctx.r.globals) || {};
  const names = orderedNames(ctx.b, 'globals');
  const uses = globalUses(ctx, names);
  const spec = [
    ['Global', 'c-name2'],
    ['Formula', 'c-desc'],
    ['Value', 'c-num', 'num'],
    ['Lines using it', 'c-mid', 'num'],
  ];
  const rows = names.map((n) => {
    const f = formula(g[n]);
    return row(el('tr', { class: 'rpt-row' }, td(n, 'name'), td(f.text, f.formula ? 'formula' : 'txt'), td(values[n] !== undefined ? decimal(values[n]) : '', 'num'), td(String(uses.get(n) || 0), 'num')));
  });
  if (!rows.length) rows.push(emptyRow('This budget has no globals.', spec.length));
  return [simpleTable('Globals', 'rpt-globals', spec, rows)];
}

function groupsSection(ctx) {
  const groups = ctx.b.groups && typeof ctx.b.groups === 'object' ? ctx.b.groups : {};
  const sums = (ctx.r && ctx.r.groups) || {};
  const lines = new Map();
  for (const it of ctx.items) {
    if (it.kind !== 'category') continue;
    for (const a of Array.isArray(it.row.accounts) ? it.row.accounts : []) {
      for (const d of Array.isArray(a && a.details) ? a.details : []) {
        for (const gname of Array.isArray(d.groups) ? d.groups : []) lines.set(gname, (lines.get(gname) || 0) + 1);
      }
    }
  }
  const spec = [
    ['Group', 'c-name2'],
    ['Description', 'c-desc'],
    ['In total', 'c-short'],
    ['Lines', 'c-short', 'num'],
    ['Lines total', 'c-money', 'num'],
    ['Fringes', 'c-money', 'num'],
    ['Total', 'c-money', 'num'],
  ];
  const rows = [];
  for (const name of orderedNames(ctx.b, 'groups')) {
    const gr = groups[name] || {};
    const s = sums[name] ? sums[name].full : null;
    let cls = 'rpt-row';
    if (ctx.spec.group_colors) {
      const c = hex6(gr.color);
      if (c) {
        cls += ` tc-${c}`;
        ctx.colors.add(c);
      }
    }
    rows.push(row(el('tr', { class: cls }, td(name, 'name'), td(gr.description || '', 'txt'), td(gr.in_budget_total === false ? 'No' : 'Yes', 'txt'), td(String(lines.get(name) || 0), 'num'), td(s ? m(s.details_total) : '', 'num'), td(s ? m(s.fringe_total) : '', 'num'), td(s ? m(s.details_total + s.fringe_total) : '', 'num'))));
  }
  if (!rows.length) rows.push(emptyRow('This budget has no groups.', spec.length));
  else rows.push(row(el('tr', { class: 'rpt-note-row' }, td('Totals count every tagged line, in the budget total or not. A line in several groups counts in each.', 'note', spec.length))));
  return [simpleTable('Groups', 'rpt-groups', spec, rows)];
}

/** A long note split into pieces that each fit on a page: paragraphs, then about 900 characters. */
function notePieces(text) {
  const out = [];
  for (const para of text.split(/\n+/)) {
    let p = para.trim();
    while (p.length > 900) {
      let cut = p.lastIndexOf(' ', 900);
      if (cut < 600) cut = 900;
      out.push(p.slice(0, cut));
      p = p.slice(cut).trimStart();
    }
    if (p) out.push(p);
  }
  return out.length ? out : [text];
}

function notesSection(ctx) {
  const spec = [
    ['', 'c-mark', 'num'],
    ['Account', 'c-mid'],
    ['No.', 'c-short', 'num'],
    ['Line', 'c-name2'],
    ['Note', 'c-wide'],
  ];
  const rows = [];
  // One note: its date, author and text, split to fit pages.
  const add = (mark, whereCell, no, what, note) => {
    const pieces = notePieces(note.text.trim());
    const meta = noteMeta(note);
    const text = td('', 'note-text');
    if (meta) text.append(el('span', { class: 'note-meta', text: meta }), el('br'));
    text.append(pieces[0]);
    const first = el('tr', { class: `rpt-row rpt-note${note.done ? ' done' : ''}` }, td(mark, 'num mark'), whereCell, td(no, 'num'), td(what, 'txt'), text);
    rows.push(row(first, pieces.length > 1));
    pieces.slice(1).forEach((p, k) => {
      rows.push(row(el('tr', { class: 'rpt-row rpt-note more' }, td('', ''), td('', ''), td('', ''), td('', ''), td(p, 'note-text')), k < pieces.length - 2));
    });
  };
  const place = (code, name) => {
    const c = td('', 'txt');
    c.append(el('span', { class: 'acct-code', text: code || '' }), el('span', { text: name || '' }));
    return c;
  };
  for (const it of ctx.items) {
    if (it.kind !== 'category' || ctx.exCats.has(it.row.code || '')) continue;
    for (const note of printedNotes(ctx, it.row.notes)) add('', place(it.row.code, it.row.name), '', 'Category', note);
    for (const { acct } of accountsOf(ctx, it)) {
      for (const note of printedNotes(ctx, acct.notes)) add('', place(acct.code, acct.name), '', 'Account', note);
      const details = Array.isArray(acct.details) ? acct.details : [];
      details.forEach((d, i) => {
        if (!d) return;
        const n = ctx.notes.get(d.id);
        printedNotes(ctx, d.notes).forEach((note, k) => {
          add(n && k === 0 ? String(n) : '', place(acct.code, acct.name), String(i + 1), d.description || '', note);
        });
      });
    }
  }
  if (!rows.length) rows.push(emptyRow('No notes in this report.', spec.length));
  return [simpleTable('Notes', 'rpt-notes', spec, rows)];
}

const POSTING = {
  budget: 'One total fringes row for the budget',
  production_level: 'Above each production total',
  category: 'To a fringe account in each category',
  account: 'Inside each account',
};

function infoSection(ctx) {
  const b = ctx.b;
  const r = ctx.r;
  const spec = [
    ['', 'c-label'],
    ['', 'c-wide'],
  ];
  const rows = [];
  const fact = (k, v, cls = '') => rows.push(row(el('tr', { class: `rpt-row ${cls}` }, td(k, 'label'), td(v, 'txt'))));
  const headRow = (text) => rows.push(row(el('tr', { class: 'rpt-info-head' }, td(text, '', 2)), true));
  headRow('Budget');
  fact('Title', ctx.title);
  if (ctx.file) fact('File', ctx.file);
  fact('Base currency', b.base_currency || '');
  if (b.totals_currency) fact('Totals shown in', b.totals_currency);
  const posting = b.fringe_posting && b.fringe_posting.kind ? b.fringe_posting.kind : 'category';
  fact('Fringes post', POSTING[posting] || posting);
  let cats = 0;
  let accts = 0;
  let lines = 0;
  for (const it of ctx.items) {
    if (it.kind !== 'category') continue;
    cats += 1;
    for (const a of Array.isArray(it.row.accounts) ? it.row.accounts : []) {
      accts += 1;
      lines += Array.isArray(a && a.details) ? a.details.length : 0;
    }
  }
  fact('Size', `${cats} ${cats === 1 ? 'category' : 'categories'}, ${accts} ${accts === 1 ? 'account' : 'accounts'}, ${lines} ${lines === 1 ? 'row' : 'rows'}`);
  if (r) fact('Grand total', money(r.grand_total));
  if (r && (r.credits || []).length) fact('Net total', money(r.net_total));
  const info = b.info && typeof b.info === 'object' ? Object.entries(b.info) : [];
  if (info.length) {
    headRow('Information');
    for (const [label, v] of info) fact(label, infoText(v));
  }
  const curs = b.currencies && typeof b.currencies === 'object' ? Object.entries(b.currencies) : [];
  if (curs.length) {
    headRow('Currencies');
    const at = new Map(orderedNames(b, 'currencies').map((n, i) => [n, i]));
    for (const [code, c] of curs.sort(([a], [z]) => (at.get(a) ?? 0) - (at.get(z) ?? 0))) {
      const rate = c && c.rate !== undefined ? `1 ${code} = ${decimal(c.rate)} ${b.base_currency || ''}` : '';
      fact(code, [c && c.description, rate].filter(Boolean).join(', '));
    }
  }
  return [{ ...simpleTable('Budget information', 'rpt-info', spec, rows), head: null }];
}

// --- Chart of accounts and the other setup tables ---

/** Every detail row of the budget, in order. */
function* allDetails(ctx) {
  for (const it of ctx.items) {
    if (it.kind !== 'category') continue;
    for (const a of Array.isArray(it.row.accounts) ? it.row.accounts : []) {
      for (const d of Array.isArray(a && a.details) ? a.details : []) if (d) yield d;
    }
  }
}

/** How many rows `key(d)` gives each name for. */
function countBy(ctx, key) {
  const out = new Map();
  for (const d of allDetails(ctx)) {
    for (const n of new Set(key(d))) if (n) out.set(n, (out.get(n) || 0) + 1);
  }
  return out;
}

function chartSection(ctx) {
  const spec = [
    ['Acct #', 'c-code2'],
    ['Description', 'c-desc'],
    ['Accounts', 'c-short', 'num'],
    ['Rows', 'c-short', 'num'],
  ];
  const rows = [];
  for (const it of ctx.items) {
    if (it.kind === 'level') {
      rows.push(row(el('tr', { class: 'rpt-row rpt-chart-level' }, td('', 'code'), td(it.row.name || 'Production total', 'txt', 3))));
      continue;
    }
    const accounts = Array.isArray(it.row.accounts) ? it.row.accounts : [];
    const lines = accounts.reduce((a, x) => a + (Array.isArray(x && x.details) ? x.details.length : 0), 0);
    rows.push(row(el('tr', { class: 'rpt-row rpt-chart-cat' }, td(it.row.code || '', 'code'), td(it.row.name || '', 'name'), td(String(accounts.length), 'num'), td(String(lines), 'num')), accounts.length > 0));
    accounts.forEach((a0, k) => {
      const a = a0 || {};
      const n = Array.isArray(a.details) ? a.details.length : 0;
      rows.push(row(el('tr', { class: 'rpt-row rpt-chart-acct' }, td(a.code || '', 'code'), td(a.name || '', 'txt'), td('', 'num'), td(String(n), 'num')), k === 0));
    });
  }
  if (!rows.length) rows.push(emptyRow('This budget has no categories yet.', spec.length));
  return [simpleTable('Chart of accounts', 'rpt-chart', spec, rows)];
}

function unitsSection(ctx) {
  const units = ctx.b.units && typeof ctx.b.units === 'object' ? ctx.b.units : {};
  const uses = countBy(ctx, (d) => [d.unit, d.unit2, d.unit3, d.unit4]);
  const spec = [
    ['Unit', 'c-name2'],
    ['Plural', 'c-mid'],
    ['Description', 'c-desc'],
    ['Hours', 'c-short', 'num'],
    ['Rows using it', 'c-mid', 'num'],
  ];
  const rows = Object.keys(units)
    .sort()
    .map((name) => {
      const u = units[name] || {};
      const hours = u.hours !== undefined && u.hours !== null && u.hours !== '' ? decimal(String(u.hours)) : '';
      return row(el('tr', { class: 'rpt-row' }, td(name, 'name'), td(u.plural || '', 'txt'), td(u.description || '', 'txt'), td(hours, 'num'), td(String(uses.get(name) || 0), 'num')));
    });
  if (!rows.length) rows.push(emptyRow('This budget has no units.', spec.length));
  return [simpleTable('Units', 'rpt-units', spec, rows)];
}

/** Sets or locations: each with its description, how many rows, and what they add up to. */
function breakdownSection(ctx, { title, cls, table, key, sums, field, label, empty, place }) {
  const items = table && typeof table === 'object' ? table : {};
  const uses = countBy(ctx, (d) => [d[field]]);
  const spec = [
    [label, 'c-name2'],
    ['Description', 'c-desc'],
    ['Rows', 'c-short', 'num'],
    ['Lines total', 'c-money', 'num'],
    ['Fringes', 'c-money', 'num'],
    ['Total', 'c-money', 'num'],
  ];
  const rows = [];
  for (const name of key ? orderedNames(ctx.b, key) : Object.keys(items).sort()) {
    const it = items[name] || {};
    const sum = sums && sums[name] ? sums[name] : null;
    const desc = [it.description, place ? place(it) : ''].filter(Boolean).join('; ');
    rows.push(row(el('tr', { class: 'rpt-row' }, td(name, 'name'), td(desc, 'txt'), td(String(uses.get(name) || 0), 'num'), td(sum ? m(sum.details_total) : '', 'num'), td(sum ? m(sum.fringe_total) : '', 'num'), td(sum ? m(sum.details_total + sum.fringe_total) : '', 'num'))));
  }
  if (!rows.length) rows.push(emptyRow(empty, spec.length));
  else rows.push(row(el('tr', { class: 'rpt-note-row' }, td('Totals count the tagged lines that are in the budget total.', 'note', spec.length))));
  return [simpleTable(title, cls, spec, rows)];
}

function setsSection(ctx) {
  return breakdownSection(ctx, { title: 'Sets', cls: 'rpt-sets', table: ctx.b.sets, key: 'sets', sums: ctx.r && ctx.r.sets, field: 'set', label: 'Set', empty: 'This budget has no sets.' });
}

function locationsSection(ctx) {
  return breakdownSection(ctx, {
    title: 'Locations',
    cls: 'rpt-locations',
    table: ctx.b.locations,
    key: 'locations',
    sums: ctx.r && ctx.r.locations,
    field: 'location',
    label: 'Location',
    empty: 'This budget has no locations.',
    place: (l) => [l.city, l.region, l.country].filter(Boolean).join(', '),
  });
}

function currenciesSection(ctx) {
  const b = ctx.b;
  const base = b.base_currency || (ctx.r && ctx.r.base_currency) || '';
  const curs = b.currencies && typeof b.currencies === 'object' ? b.currencies : {};
  // A line with no currency is in the base currency.
  const uses = countBy(ctx, (d) => [d.currency || base]);
  const spec = [
    ['Code', 'c-short'],
    ['Description', 'c-desc'],
    ['Symbol', 'c-short'],
    ['Rate', 'c-mid'],
    ['Rows', 'c-short', 'num'],
  ];
  const rows = [];
  if (base) rows.push(row(el('tr', { class: 'rpt-row' }, td(base, 'name'), td('Base currency', 'txt'), td(curs[base] && curs[base].symbol ? curs[base].symbol : '', 'txt'), td('', 'txt'), td(String(uses.get(base) || 0), 'num'))));
  for (const code of orderedNames(ctx.b, 'currencies')) {
    if (code === base) continue;
    const c = curs[code] || {};
    const rate = c.rate !== undefined ? `1 ${code} = ${decimal(String(c.rate))} ${base}` : '';
    rows.push(row(el('tr', { class: 'rpt-row' }, td(code, 'name'), td(c.description || '', 'txt'), td(c.symbol || '', 'txt'), td(rate, 'txt'), td(String(uses.get(code) || 0), 'num'))));
  }
  if (b.totals_currency) rows.push(row(el('tr', { class: 'rpt-note-row' }, td(`Totals are also shown in ${b.totals_currency}.`, 'note', spec.length))));
  if (!rows.length) rows.push(emptyRow('This budget has no currencies.', spec.length));
  return [simpleTable('Currencies', 'rpt-currencies', spec, rows)];
}

// --- Cost report (actuals; src/actuals.rs) ---

/** "12.5" (Rust's percent spent) as "12.50%". */
function pct(p) {
  if (p === null || p === undefined || p === '') return '';
  const t = String(p);
  const neg = t.startsWith('-');
  const [int, frac = ''] = (neg ? t.slice(1) : t).split('.');
  return `${neg ? '-' : ''}${decimal(`${int}.${frac.padEnd(2, '0').slice(0, 2)}`)}%`;
}

/**
 * Budget, baseline (when the budget has one), actual, ETC, EAC, variance
 * and percent spent for every account, category fringe account, category,
 * production level and summary row, from the view's cost figures. Accounts
 * the spec leaves out are left out; totals never change.
 */
function costReportSection(ctx) {
  const cost = ctx.view && ctx.view.cost && Array.isArray(ctx.view.cost.categories) ? ctx.view.cost : null;
  const withBase = Boolean(cost && cost.total && cost.total.baseline !== null && cost.total.baseline !== undefined);
  const spec = [
    ['Acct #', 'c-code2'],
    ['Description', 'c-desc'],
    ['Budget', 'c-cm', 'num'],
    ...(withBase ? [['Baseline', 'c-cm', 'num']] : []),
    ['Actual', 'c-cm', 'num'],
    ['ETC', 'c-cm', 'num'],
    ['EAC', 'c-cm', 'num'],
    ['Variance', 'c-cm', 'num'],
    ['% Spent', 'c-pct', 'num'],
  ];
  const rows = [];
  if (!cost) {
    rows.push(emptyRow('This budget has no actuals. File > Import Actuals… brings them in from an accounting export.', spec.length));
    return [simpleTable('Cost report', 'rpt-cost', spec, rows)];
  }
  const figs = (f) => {
    if (!f) return spec.slice(2).map(() => td('', 'num'));
    return [
      td(m(f.budget), 'num'),
      ...(withBase ? [td(m(f.baseline), 'num base')] : []),
      td(m(f.actual), 'num'),
      td(m(f.etc), 'num'),
      td(m(f.eac), 'num'),
      td(m(f.variance), `num${f.over ? ' over' : ''}`),
      td(pct(f.percent), 'num'),
    ];
  };
  const line = (cls, code, label, f, keep = false) => {
    rows.push(row(el('tr', { class: `rpt-row ${cls}` }, td(code, 'code'), td(label, cls.includes('total') ? 'name' : 'txt'), ...figs(f)), keep));
  };
  for (const it of ctx.items) {
    if (it.kind === 'level') {
      const lc = cost.levels[it.li];
      if (lc && lc.posted_fringes) line('rpt-cost-other', '', 'Fringes', lc.posted_fringes);
      if (lc && it.row.name) line('rpt-st-total rpt-cost-level', '', it.row.name, lc.figures);
      continue;
    }
    const cc = cost.categories[it.ci];
    if (!cc || ctx.exCats.has(it.row.code || '')) continue;
    const accts = accountsOf(ctx, it);
    const cap = td('', 'cat', spec.length);
    cap.append(el('span', { class: 'cat-code', text: it.row.code || '' }), el('span', { class: 'cat-name', text: it.row.name || '' }));
    rows.push(row(el('tr', { class: 'rpt-cost-cat' }, cap), true));
    for (const { acct, ai } of accts) {
      const ac = cc.accounts[ai];
      if (ac) line('rpt-cost-acct', acct.code || '', acct.name || '', ac.figures);
    }
    if (cc.fringe_account) line('rpt-cost-acct', cc.fringe_account.code, 'Fringes', cc.fringe_account.figures);
    line('rpt-st-total', '', `Total ${catLabel(it.row)}`, cc.figures);
  }
  const sum = [];
  sum.push(['Total above-the-line', cost.total_atl]);
  sum.push(['Total below-the-line', cost.total_btl]);
  sum.push(['Total above and below-the-line', cost.atl_btl]);
  if (cost.budget_fringes) sum.push(['Total fringes', cost.budget_fringes.figures]);
  (cost.charges || []).forEach((c, i) => sum.push([ctx.r && ctx.r.charges && ctx.r.charges[i] ? chargeLabel(ctx.b, ctx.r.charges[i], i, money) : c.name, c.figures]));
  if (cost.unassigned && cost.unassigned.count) sum.push(['Unassigned actuals', cost.unassigned]);
  sum.forEach(([label, f], i) => line(i === 0 ? 'rpt-cost-sum first' : 'rpt-cost-sum', '', label, f, true));
  line('rpt-st-total rpt-cost-grand', '', 'Grand total', cost.total);
  rows.push(
    row(
      el(
        'tr',
        { class: 'rpt-note-row' },
        td(
          `Actual: ${cost.transactions} ${cost.transactions === 1 ? 'transaction' : 'transactions'} imported. ETC is the estimate to complete (budget minus actual unless set per account), EAC the estimate at completion (actual plus ETC), Variance the budget minus EAC: amounts in parentheses are over budget. Unassigned actuals have an account number this budget doesn't have.`,
          'note',
          spec.length,
        ),
      ),
    ),
  );
  return [simpleTable('Cost report', 'rpt-cost', spec, rows)];
}

const BUILD = {
  topsheet,
  accounts: accountLevel,
  detail,
  fringes: fringesSection,
  globals: globalsSection,
  groups: groupsSection,
  notes: notesSection,
  info: infoSection,
  chart: chartSection,
  units: unitsSection,
  sets: setsSection,
  locations: locationsSection,
  currencies: currenciesSection,
  cost_report: costReportSection,
};

// ---------------------------------------------------------------------------
// Pages

// Letter paper in CSS pixels (96 to the inch), and the page's own margins.
const PAPER = { w: 816, h: 1056 };
const MARGIN = { top: 40, bottom: 36, side: 48 };
// Height kept spare on each page for rounding between measuring and drawing.
const SLACK = 6;

function pageSize(landscape) {
  return landscape ? { w: PAPER.h, h: PAPER.w } : { ...PAPER };
}

function bandText(f, ctx, page) {
  if (!f) return '';
  switch (f.kind) {
    case 'title':
      return ctx.title;
    case 'section':
      return page.section || '';
    case 'printed':
      return `Printed ${ctx.printed}`;
    case 'date':
      return ctx.printedDate;
    case 'time':
      return ctx.printedTime;
    case 'file':
      return ctx.file || '';
    case 'page':
      return `Page ${page.n} of ${page.of}`;
    case 'info': {
      const v = ctx.b.info && ctx.b.info[f.label];
      return v ? `${f.label}: ${infoText(v)}` : '';
    }
    case 'global': {
      const v = ctx.r && ctx.r.globals ? ctx.r.globals[f.name] : undefined;
      return v !== undefined && v !== null ? `${f.name}: ${decimal(v)}` : '';
    }
    case 'text':
      return f.text || '';
    default:
      return '';
  }
}

function drawBand(b, ctx, page, cls) {
  const box = el('div', { class: `rpt-band ${cls}` });
  for (const slot of ['left', 'center', 'right']) {
    const f = b ? b[slot] : null;
    box.append(el('span', { class: `rpt-slot ${slot}${f && f.kind === 'title' ? ' strong' : ''}`, text: bandText(f, ctx, page) }));
  }
  return box;
}

function bandEmpty(b) {
  return !b || !(b.left || b.center || b.right);
}

/** Whether a band prints on page `n` (1-based): every page, or only the first. */
function bandOn(b, n) {
  return !bandEmpty(b) && (n === 1 || b.first_page_only !== true);
}

/** The report root's classes for this spec. */
function rootClass(spec) {
  let c = `method-report fs-${spec.font_size}`;
  if (spec.landscape) c += ' landscape';
  if (spec.shading) c += ' shaded';
  if (spec.group_colors) c += ' colors';
  if (spec.vertical_rules) c += ' rules';
  return c;
}

function tableFor(flow) {
  const t = el('table', { class: `rpt-table ${flow.cls}` }, colgroup(flow.cols));
  return t;
}

/**
 * Measures every flow's rows in a hidden page body, then deals them onto
 * pages. Returns [{ flow, intro, rows: [tr] }] with each page's rows moved
 * out of the measuring table.
 */
function paginate(flows, spec, header, footer, ctx) {
  const size = pageSize(spec.landscape);
  const host = el('div', { class: `${rootClass(spec)} rpt-measure`, attrs: { 'aria-hidden': 'true' } });
  const body = el('div', { class: 'rpt-measure-body' });
  host.append(body);
  document.body.append(host);
  try {
    // Bands: one line each, the same on every page.
    const probe = { section: 'Section', n: 88, of: 88 };
    const hb = bandEmpty(header) ? null : drawBand(header, ctx, probe, 'rpt-head');
    const fb = bandEmpty(footer) ? null : drawBand(footer, ctx, probe, 'rpt-foot');
    host.append(...[hb, fb].filter(Boolean));
    const outer = (b) => {
      if (!b) return 0;
      const cs = getComputedStyle(b);
      return b.getBoundingClientRect().height + (parseFloat(cs.marginTop) || 0) + (parseFloat(cs.marginBottom) || 0);
    };
    // A band on the first page only leaves later pages more room.
    const availFirst = size.h - MARGIN.top - MARGIN.bottom - outer(hb) - outer(fb) - SLACK;
    const availLater = size.h - MARGIN.top - MARGIN.bottom - (bandOn(header, 2) ? outer(hb) : 0) - (bandOn(footer, 2) ? outer(fb) : 0) - SLACK;
    if (hb) hb.remove();
    if (fb) fb.remove();

    const pages = [];
    for (const flow of flows) {
      const t = tableFor(flow);
      const thead = el('thead');
      if (flow.head) thead.append(flow.head);
      const tbody = el('tbody', {}, ...flow.rows.map((r) => r.el));
      t.append(thead, tbody);
      body.replaceChildren(...[flow.intro, t].filter(Boolean));
      const introH = flow.intro ? flow.intro.getBoundingClientRect().height : 0;
      const headH = flow.head ? flow.head.getBoundingClientRect().height : 0;
      const hs = flow.rows.map((r) => r.el.getBoundingClientRect().height);

      let cur = { flow, intro: flow.intro || null, rows: [] };
      let used = introH + headH;
      let avail = pages.length === 0 ? availFirst : availLater;
      const room = Math.min(availFirst, availLater) - headH;
      const newPage = () => {
        pages.push(cur);
        cur = { flow, intro: null, rows: [] };
        used = headH;
        avail = availLater;
      };
      const n = flow.rows.length;
      let i = 0;
      while (i < n) {
        let j = i;
        let h = hs[i];
        while (flow.rows[j].keep && j + 1 < n && h + hs[j + 1] <= room) {
          j += 1;
          h += hs[j];
        }
        if (used + h > avail && cur.rows.length) newPage();
        if (used + h > avail) {
          // Taller than a page even alone: place row by row.
          for (let k = i; k <= j; k++) {
            if (used + hs[k] > avail && cur.rows.length) newPage();
            cur.rows.push(flow.rows[k].el);
            used += hs[k];
          }
        } else {
          for (let k = i; k <= j; k++) cur.rows.push(flow.rows[k].el);
          used += h;
        }
        i = j + 1;
      }
      pages.push(cur);
    }
    return pages;
  } finally {
    host.remove();
  }
}

/** Draws the dealt pages into `container`. Returns { pages, sections: [{ name, page }] }. */
function drawPages(container, dealt, spec, header, footer, ctx) {
  const root = el('div', { class: `${rootClass(spec)} rpt-pages`, attrs: { 'aria-label': `${ctx.title} report` } });
  const sections = [];
  let lastFlow = null;
  dealt.forEach((p, k) => {
    const page = { section: p.flow.name, n: k + 1, of: dealt.length };
    if (p.flow !== lastFlow) {
      sections.push({ name: p.flow.name, page: k + 1 });
      lastFlow = p.flow;
    }
    const t = tableFor(p.flow);
    if (p.flow.head) t.append(el('thead', {}, k === 0 || p.intro || dealt[k - 1].flow !== p.flow ? p.flow.head : p.flow.head.cloneNode(true)));
    t.append(el('tbody', {}, ...p.rows));
    const bodyBox = el('div', { class: 'rpt-body' }, p.intro, t);
    const sheet = el(
      'div',
      { class: 'rpt-page', data: { page: k + 1 } },
      bandOn(header, k + 1) ? drawBand(header, ctx, page, 'rpt-head') : null,
      bodyBox,
      bandOn(footer, k + 1) ? drawBand(footer, ctx, page, 'rpt-foot') : null,
    );
    root.append(el('div', { class: 'rpt-sheet' }, sheet));
  });
  container.replaceChildren(root);
  return { pages: dealt.length, sections };
}

function colorRules(colors) {
  return [...colors].map((c) => `.method-report.colors tr.tc-${c} td { color: #${c}; }`).join('\n');
}

function makeContext(view, spec) {
  const v = view || {};
  const { b, r, items } = index(v);
  const now = new Date();
  const ctx = {
    view: v,
    b,
    r,
    items,
    spec,
    title: (typeof b.title === 'string' && b.title.trim()) || 'Untitled budget',
    printed: printedAt(now),
    printedDate: printedDate(now),
    printedTime: printedTime(now),
    file: v.file && typeof v.file.name === 'string' ? v.file.name : '',
    baseline: baselineOf(v),
    cellIssues: cellIssues(v),
    cols: spec.columns || STANDARD_COLUMNS,
    exCats: new Set(spec.excluded_categories),
    exAccts: new Set(spec.excluded_accounts),
    onlyGroups: new Set(spec.only_groups),
    colors: new Set(),
    detailWidths: '',
    // Topsheet page cells to fill once the pages are dealt: [{ ci, cell }],
    // and the detail's category heading rows (row -> category index).
    pageCells: [],
    catHeads: new Map(),
    // Accounts are being left out, so a category with none left is skipped.
    filtered: spec.only_groups.length > 0 || spec.excluded_accounts.length > 0 || spec.skip_empty_accounts || spec.skip_zero_accounts,
  };
  ctx.notes = noteNumbers(ctx);
  return ctx;
}

/**
 * Lays a report out into `container` (which must be in the document),
 * replacing what it held. `spec`: a report spec, a preset, or "topsheet",
 * "detail" or "both". Returns { pages, sections: [{ name, page }] }.
 */
export function renderReport(container, view, spec = 'both') {
  if (!container) return { pages: 0, sections: [] };
  const s = normalizeSpec(spec);
  const ctx = makeContext(view, s);
  const flows = [];
  for (const key of s.sections) flows.push(...BUILD[key](ctx));
  if (flows.length === 0) {
    flows.push({ name: 'Report', cls: 'rpt-st', cols: ['c-wide'], head: null, rows: [emptyRow('Choose at least one section to print.', 1)] });
  }
  flows[0].intro = el('div', { class: 'rpt-intro' }, titleBlock(ctx, SECTIONS.find((x) => x.key === s.sections[0])?.label || 'Report'));
  setRules('report-detail-cols', ctx.detailWidths);
  setRules('report-colors', s.group_colors ? colorRules(ctx.colors) : '');
  const header = s.header || STANDARD_HEADER;
  const footer = s.footer || STANDARD_FOOTER;
  const dealt = paginate(flows, s, header, footer, ctx);
  fillPageCells(ctx, dealt);
  return drawPages(container, dealt, s, header, footer, ctx);
}

/**
 * The topsheet's page column: the page each category's detail starts on
 * (blank when its detail doesn't print). The cells were measured empty and
 * have a fixed width, so filling them in moves nothing.
 */
function fillPageCells(ctx, dealt) {
  if (!ctx.pageCells.length) return;
  const first = new Map();
  dealt.forEach((p, k) => {
    for (const tr of p.rows) {
      const ci = ctx.catHeads.get(tr);
      if (ci !== undefined && !first.has(ci)) first.set(ci, k + 1);
    }
  });
  for (const { ci, cell } of ctx.pageCells) {
    const n = first.get(ci);
    cell.textContent = n ? String(n) : '';
  }
}

/**
 * Lays any table (its header row repeats on every page) or other element
 * (its children become rows) out into pages in `container`, with the usual
 * header and footer. opts: { title, section, file, landscape, fontSize,
 * header, footer, intro } (header and footer as in a report spec).
 */
export function renderHtml(container, node, opts = {}) {
  if (!container || !node) return { pages: 0, sections: [] };
  const s = normalizeSpec({ sections: [], landscape: opts.landscape === true, font_size: opts.fontSize, header: opts.header, footer: opts.footer });
  const ctx = makeContext({ budget: { title: opts.title || '' }, file: { name: opts.file || '' } }, s);
  const name = opts.section || '';
  let flow;
  if (node.tagName === 'TABLE') {
    const cg = node.querySelector('colgroup');
    const cols = cg ? [...cg.children].map((c) => c.className) : [];
    const headRow = node.tHead && node.tHead.rows.length ? node.tHead.rows[node.tHead.rows.length - 1].cloneNode(true) : null;
    const rows = [];
    for (const tb of node.tBodies) for (const tr of tb.rows) rows.push(row(tr.cloneNode(true)));
    flow = { name, cls: `rpt-custom ${node.className || ''}`.trim(), cols, head: headRow, rows };
  } else {
    const rows = [...node.children].map((c) => row(el('tr', {}, el('td', {}, c.cloneNode(true)))));
    flow = { name, cls: 'rpt-custom rpt-blocks', cols: ['c-wide'], head: null, rows };
  }
  if (opts.intro) flow.intro = el('div', { class: 'rpt-intro' }, opts.intro);
  const header = s.header || STANDARD_HEADER;
  const footer = s.footer || STANDARD_FOOTER;
  setRules('report-colors', '');
  const dealt = paginate([flow], s, header, footer, ctx);
  return drawPages(container, dealt, s, header, footer, ctx);
}

/**
 * Fits the pages in `container` to `width` pixels for an on-screen preview
 * (a scale rule; the pages keep their real layout).
 */
export function scalePreview(container, width, landscape) {
  const size = pageSize(landscape);
  const scale = Math.max(0.2, Math.min(1.25, width / size.w));
  setRules(
    'report-preview',
    `.rpt-preview .rpt-sheet { width: ${Math.round(size.w * scale)}px; height: ${Math.round(size.h * scale)}px; }\n` +
      `.rpt-preview .rpt-page { transform: scale(${scale.toFixed(4)}); }`,
  );
  return scale;
}

// ---------------------------------------------------------------------------
// Printing

let pending = null;

/**
 * Prints what `draw(root)` puts into #print-root: through the print sheet,
 * or with `pdf` into a PDF file the user picks (macOS; src/reports.rs).
 * Elsewhere, the system print dialog (whose PDF choice saves a file).
 * Resolves with { done, file } once it is over (`system` when the system
 * dialog was used, which says nothing about how it ended).
 */
async function printPages(draw, { landscape = false, pdf = false, title = '' } = {}) {
  if (pending) pending();
  let root = document.getElementById('print-root');
  if (!root) {
    root = document.createElement('div');
    root.id = 'print-root';
    document.body.append(root);
  }
  draw(root);
  const size = pageSize(landscape);
  setRules('report-page', `@page { size: ${size.w / 96}in ${size.h / 96}in; margin: 0; }`);

  const html = document.documentElement;
  const oldTitle = document.title;
  if (title) document.title = title; // the default file name for a PDF from the dialog
  html.classList.add('printing');
  let finished = false;
  const cleanup = () => {
    if (finished) return;
    finished = true;
    if (pending === cleanup) pending = null;
    html.classList.remove('printing');
    document.title = oldTitle;
    root.replaceChildren();
    setRules('report-page', '');
  };
  pending = cleanup;

  if (connected) {
    let outcome = null;
    try {
      outcome = await invoke('print_report', { pdf, landscape, title: title || 'Report' });
    } catch (e) {
      cleanup();
      throw e;
    }
    if (outcome && outcome.supported) {
      cleanup();
      return { done: outcome.done === true, file: outcome.file || null };
    }
  }
  return systemPrint(cleanup);
}

/** window.print(), cleaning up afterwards however the web view reports it. */
function systemPrint(cleanup) {
  return new Promise((resolve) => {
    let timer = 0;
    const end = () => {
      clearTimeout(timer);
      window.removeEventListener('afterprint', end);
      document.removeEventListener('pointerdown', end, true);
      document.removeEventListener('keydown', end, true);
      cleanup();
      resolve({ done: true, file: null, system: true });
    };
    window.addEventListener('afterprint', end);
    const started = Date.now();
    try {
      // On macOS, Tauri replaces window.print() with the native print sheet.
      const shown = window.print();
      if (shown && typeof shown.then === 'function') shown.catch(end);
    } catch {
      end();
      return;
    }
    // Some web views return from print() at once and show the dialog as a
    // sheet; others block until it closes. If it blocked, we are done. If not,
    // clean up on afterprint, on the next input after the dialog closes, or
    // after a long fallback timeout.
    if (Date.now() - started > 1000) {
      setTimeout(end, 0);
      return;
    }
    setTimeout(() => {
      document.addEventListener('pointerdown', end, true);
      document.addEventListener('keydown', end, true);
    }, 500);
    timer = setTimeout(end, 10 * 60 * 1000);
  });
}

function jobTitle(view, extra) {
  const t = view && view.budget && typeof view.budget.title === 'string' ? view.budget.title.trim() : '';
  return [t || 'Budget', extra].filter(Boolean).join(' - ');
}

/**
 * Prints a report: through the print sheet, or with `{ pdf: true }` straight
 * into a PDF file. `name` (a preset's name) goes into the job's title.
 * Resolves with { done, file }.
 */
export function printReport(view, spec = 'both', { pdf = false, name = '' } = {}) {
  const s = normalizeSpec(spec);
  return printPages((root) => renderReport(root, view, s), { landscape: s.landscape, pdf, title: jobTitle(view, name || 'Report') });
}

/** printReport into a PDF file. */
export function saveReportPdf(view, spec = 'both', opts = {}) {
  return printReport(view, spec, { ...opts, pdf: true });
}

/** Prints renderHtml's pages (opts as there, plus `pdf`). */
export function printHtml(node, opts = {}) {
  return printPages((root) => renderHtml(root, node, opts), {
    landscape: opts.landscape === true,
    pdf: opts.pdf === true,
    title: [opts.title, opts.section].filter(Boolean).join(' - ') || 'Report',
  });
}
