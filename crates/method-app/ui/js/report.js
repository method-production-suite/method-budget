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
import { money, decimal, currencyMoney } from './format.js';
import { rowColor, fringePercent, chargeLabel, fringeRowOf, topsheetOrder } from './levels.js';
import { setRules } from './sheet.js';
import { orderedNames } from './order.js';
import { invoke, connected } from './api.js';
import { flatCountWords } from './util.js';
import { t } from './i18n.js';

// ---------------------------------------------------------------------------
// The spec

/** Sections in their usual order, with the names the setup dialog shows. */
export const SECTIONS = [
  { key: 'topsheet', label: t('report.section.topsheet') },
  { key: 'accounts', label: t('report.section.accounts') },
  { key: 'detail', label: t('report.section.detail') },
  { key: 'fringes', label: t('report.section.fringes') },
  { key: 'globals', label: t('report.section.globals') },
  { key: 'groups', label: t('report.section.groups') },
  { key: 'notes', label: t('report.section.notes') },
  { key: 'info', label: t('report.section.info') },
  { key: 'chart', label: t('report.section.chart') },
  { key: 'units', label: t('report.section.units') },
  { key: 'sets', label: t('report.section.sets') },
  { key: 'locations', label: t('report.section.locations') },
  { key: 'currencies', label: t('report.section.currencies') },
  { key: 'cost_report', label: t('report.section.cost_report') },
  { key: 'bid', label: t('report.section.bid') },
  { key: 'incentives', label: t('report.section.incentives') },
];
const SECTION_KEYS = new Set(SECTIONS.map((s) => s.key));

// Optional detail columns in grid order. w is the column's width in pixels
// at the medium type size; figures columns (`keep`) never get narrower, the
// others give way to the description when the page is full. `#`, the
// description and the total always print.
export const REPORT_COLUMNS = [
  { key: 'amount', label: t('report.col.amount'), cls: 'num', w: 70, keep: true },
  { key: 'unit', label: t('report.col.unit'), w: 44 },
  { key: 'x', label: t('report.col.x'), cls: 'num', w: 34 },
  { key: 'unit2', label: t('report.col.unit2'), w: 44 },
  { key: 'rate', label: t('report.col.rate'), cls: 'num', w: 72, keep: true },
  { key: 'unit3', label: t('report.col.unit3'), w: 46 },
  { key: 'currency', label: t('report.col.cur'), name: t('report.col.currency'), w: 34 },
  { key: 'x4', label: t('report.col.x4'), cls: 'num', w: 34 },
  { key: 'unit4', label: t('report.col.unit4'), w: 44 },
  { key: 'fringes', label: t('report.col.fringes'), w: 66 },
  { key: 'fringe_pct', label: t('report.col.fringe_pct'), cls: 'num', w: 52, keep: true },
  { key: 'fringe_total', label: t('report.col.fringe_total'), cls: 'num', w: 78, keep: true },
  { key: 'groups', label: t('report.col.groups'), w: 62 },
  { key: 'set', label: t('report.col.set'), w: 50 },
  { key: 'location', label: t('report.col.location'), w: 60 },
  { key: 'original', label: t('report.col.original'), cls: 'num', w: 90, keep: true, baseline: true },
  { key: 'variance', label: t('report.col.variance'), cls: 'num', w: 90, keep: true, baseline: true },
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
  { kind: 'title', label: t('report.band.title') },
  { kind: 'section', label: t('report.band.section') },
  { kind: 'printed', label: t('report.band.printed') },
  { kind: 'date', label: t('report.band.date') },
  { kind: 'time', label: t('report.band.time') },
  { kind: 'file', label: t('report.band.file') },
  { kind: 'page', label: t('report.band.page') },
  { kind: 'info', label: t('report.band.info') },
  { kind: 'global', label: t('report.band.global') },
  { kind: 'text', label: t('report.band.text') },
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
    bid: bidLayout(s.bid),
  };
}

// --- Bid layouts (docs/FORMAT.md, "Bid layout"; decisions.md, entry 92) ---

function text(v) {
  return typeof v === 'string' ? v : '';
}

/** One pick of a bid section in the file's shape, or null. */
function bidPick(p) {
  if (!p || typeof p !== 'object') return null;
  if (p.kind === 'category' || p.kind === 'account') return { kind: p.kind, code: text(p.code) };
  if (p.kind === 'range') return { kind: 'range', from: text(p.from), to: text(p.to) };
  return null;
}

/** One line of a bid in the file's shape, or null. */
function bidLine(l) {
  if (!l || typeof l !== 'object') return null;
  const name = text(l.name);
  switch (l.kind) {
    case 'section':
      return { kind: 'section', name, picks: (Array.isArray(l.picks) ? l.picks : []).map(bidPick).filter(Boolean) };
    case 'subtotal':
      return { kind: 'subtotal', name };
    case 'percent':
      return { kind: 'percent', name, percent: text(l.percent) || '0', of: strings(l.of) };
    case 'flat':
      return { kind: 'flat', name, amount: Number.isSafeInteger(l.amount) ? l.amount : 0 };
    default:
      return null;
  }
}

/** A bid layout from the file's shape (every part filled in), or null when there is none. */
export function bidLayout(b) {
  if (!b || typeof b !== 'object' || Array.isArray(b)) return null;
  return {
    fields: (Array.isArray(b.fields) ? b.fields : []).filter((f) => f && typeof f === 'object').map((f) => ({ label: text(f.label), value: text(f.value) })),
    lines: (Array.isArray(b.lines) ? b.lines : []).map(bidLine).filter(Boolean),
    show_accounts: b.show_accounts === true,
  };
}

/** A bid layout in the file's shape, parts at their default left out. */
export function compactBid(b) {
  const l = bidLayout(b);
  if (!l) return null;
  const out = {};
  if (l.fields.length) out.fields = l.fields.map((f) => (f.value ? { label: f.label, value: f.value } : { label: f.label }));
  if (l.lines.length) {
    out.lines = l.lines.map((x) => {
      if (x.kind === 'section') return x.picks.length ? x : { kind: 'section', name: x.name };
      if (x.kind === 'percent') return x.of.length ? x : { kind: 'percent', name: x.name, percent: x.percent };
      return x;
    });
  }
  if (l.show_accounts) out.show_accounts = true;
  return out;
}

/**
 * The view with the bid worked out for `spec` (`bid_sheet`), when the spec
 * prints the bid section: the figures come from the engine, never from the
 * page. On a refusal the view says why instead (`bid_error`).
 */
export async function withBid(view, spec) {
  const s = normalizeSpec(spec);
  if (!s.sections.includes('bid')) return view;
  try {
    const sheet = await invoke('bid_sheet', { layout: compactBid(s.bid) });
    return { ...view, bid_sheet: sheet, bid_error: '' };
  } catch (e) {
    return { ...view, bid_sheet: null, bid_error: String(e) };
  }
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
  if (s.bid) out.bid = compactBid(s.bid);
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
  const items = []; // { kind: 'category'|'level', row, rep, ci?, li?, ti }
  let ci = 0;
  let li = 0;
  for (const [ti, row] of topsheet.entries()) {
    if (!row || typeof row !== 'object') continue;
    if (row.kind === 'production_total') {
      items.push({ kind: 'level', row, rep: r && r.levels ? r.levels[li] || null : null, li, ti });
      li += 1;
    } else {
      items.push({ kind: 'category', row, rep: r && r.categories ? r.categories[ci] || null : null, ci, ti });
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
  const head = r ? t('report.totals_incomplete') : t('report.totals_not_calculated');
  const tail = r ? ` ${t('report.cells_counted_blank')}` : '';
  return el('div', { class: 'rpt-warning', attrs: { role: 'note' } }, el('strong', { text: head }), where ? ` ${where}.` : '', tail);
}

function titleBlock(ctx, subtitle) {
  const b = ctx.b;
  const r = ctx.r;
  const facts = [];
  facts.push([t('report.base_currency'), b.base_currency || (r && r.base_currency) || '']);
  const tc = b.totals_currency || (r && r.totals_currency !== r.base_currency ? r.totals_currency : '');
  if (tc) facts.push([t('report.totals_currency'), tc]);
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
    th(t('report.col.acct')),
    th(t('report.col.description')),
    pages ? th(t('report.col.page'), 'num') : null,
    fringes ? th(t('report.col.fringes'), 'num') : null,
    th(t('report.col.total'), 'num'),
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
    tr.append(td(extra.text !== undefined ? extra.text : m(total), 'num'), ...baseCells(ctx, bc, total, ch));
    rows.push(row(tr, keep));
  };
  const baseFor = (code) => (base && base.categories.get(code)) || null;
  const catCodes = [];
  // The budget's fringe row, where the budget places it (decisions.md, entry 60).
  const fr = fringeRowOf(ctx.b);
  const frShown = Boolean(r && r.budget_fringes);
  const tsLen = Array.isArray(ctx.b.topsheet) ? ctx.b.topsheet.length : 0;
  const frAt = frShown && fr.position !== null ? Math.min(fr.position, tsLen) : null;
  for (const it of ctx.items) {
    if (frAt !== null && it.ti === frAt) line('posted', fr.account, fr.description, r.budget_fringes, null, true);
    if (it.kind === 'category') {
      catCodes[it.ci] = it.row.code;
      line('cat', it.row.code || '', it.row.name || '', it.rep ? it.rep.total : null, baseFor(it.row.code), false, { page: it.ci, fringes: it.rep ? it.rep.fringe_total : 0 });
    } else {
      if (it.rep && it.rep.posted_fringes) line('posted', '', t('report.fringes'), it.rep.posted_fringes, null, true);
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
  if (frAt !== null && frAt >= tsLen) line('posted', fr.account, fr.description, r.budget_fringes, null, true);
  // The summary stays together.
  const sum = [];
  const sline = (cls, label, total, ch, code = '', text = undefined) => sum.push([cls, label, total, ch, code, text]);
  if (r) {
    sline('sum first', t('report.total_atl'), r.total_atl, null);
    sline('sum', t('report.total_btl'), r.total_btl, null);
    sline('sum strong', t('report.total_atl_btl'), r.atl_btl, null);
    if (frShown && frAt === null) sline('sum', fr.description, r.budget_fringes, null, fr.account);
    // Charges newest first, credits in reverse application order with their
    // subtotal (decisions.md, entry 60).
    const charges = r.charges || [];
    for (const i of topsheetOrder(charges.length)) sline('sum charge', chargeLabel(ctx.b, charges[i], i, money), charges[i].amount, null);
    sline('sum grand', t('report.grand_total'), r.grand_total, base ? base.grand_total : null);
    const credits = r.credits || [];
    for (const i of topsheetOrder(credits.length)) sline('sum credit', t('report.less', { name: credits[i].name || t('report.credit') }), -credits[i].amount, null);
    if (credits.length >= 2) sline('sum credit', t('report.sub_total'), -r.credits_total, null);
    if (credits.length) sline('sum grand', t('report.net_total'), r.net_total, base ? base.net_total : null);
    if (r.display) {
      const cur = r.totals_currency;
      const rate = `1 ${cur} = ${decimal(r.totals_rate)} ${r.base_currency}`;
      // In that currency's own separators and symbol (decisions.md, entry 61).
      const c = (ctx.b.currencies || {})[cur];
      sline('sum display first', t('report.grand_total_in', { currency: cur, rate }), r.display.grand_total, null, '', currencyMoney(r.display.grand_total, c));
      if (credits.length) sline('sum display', t('report.net_total_in', { currency: cur }), r.display.net_total, null, '', currencyMoney(r.display.net_total, c));
    }
  } else {
    sline('sum first', t('report.totals_unavailable'), null, null);
  }
  sum.forEach(([cls, label, total, ch, code, text], i) => line(cls, code, label, total, ch, i < sum.length - 1, text === undefined ? {} : { text }));
  if (ctx.items.length === 0) rows.unshift(row(el('tr', { class: 'rpt-row empty' }, td(t('report.no_categories_yet'), 'muted', cols.length))));
  return [{ name: t('report.section.topsheet'), cls: `rpt-ts${bc.length ? ' has-baseline' : ''}${pages || fringes ? ' has-extra' : ''}`, cols, head, rows }];
}

function accountLevel(ctx) {
  const bc = baseCols(ctx);
  const cols = ['c-code', 'c-name', 'c-money', 'c-money', ...bc.map(() => 'c-money')];
  const span = cols.length;
  const head = el('tr', { class: 'rpt-cols' }, th(t('report.col.acct')), th(t('report.col.description')), th(t('report.col.fringes'), 'num'), th(t('report.col.total'), 'num'), ...bc.map((k) => th(COLUMN.get(k).label, 'num')));
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
    if (!accts.length) rows.push(row(el('tr', { class: 'rpt-al-acct empty' }, td('', 'code'), td(t('report.no_accounts_in_report'), 'name muted', span - 1)), true));
    const rep = it.rep;
    if (rep && rep.fringe_account) {
      const tr = el('tr', { class: 'rpt-al-fa' }, td(rep.fringe_account.code || '', 'code'), td(t('report.fringes'), 'name'), td('', 'num'), td(m(rep.fringe_account.amount), 'num'), ...bc.map(() => td('', 'num')));
      rows.push(row(tr, true));
    }
    const tot = el('tr', { class: 'rpt-al-total' }, td('', 'code'), td(t('report.total_named', { name: catLabel(it.row) }), 'name'));
    tot.append(td(rep ? m(rep.fringe_total) : '', 'num muted'), td(rep ? m(rep.total) : '', 'num'));
    tot.append(...baseCells(ctx, bc, rep ? rep.total : null, base ? base.categories.get(it.row.code) || null : null));
    rows.push(row(tot));
  }
  if (!any) rows.push(row(el('tr', { class: 'rpt-row empty' }, td(t('report.no_categories_in_report'), 'muted', span))));
  return [{ name: t('report.section.accounts'), cls: `rpt-al${bc.length ? ' has-baseline' : ''}`, cols, head, rows }];
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
    { key: 'description', label: t('report.col.description'), w: 0 },
    ...mid,
    { key: 'total', label: t('report.col.total'), cls: 'num', w: 100, keep: true },
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
  if (kind === 'account') return t('report.posting.included');
  if (kind === 'budget') return t('report.posting.total_fringes');
  if (kind === 'production_level') return t('report.posting.production_total');
  if (rep && rep.fringe_account && rep.fringe_account.code) return t('report.posting.to_account', { code: rep.fringe_account.code });
  return t('report.posting.category');
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
      const text = d.description || (kind === 'subtotal' ? t('report.subtotal') : kind === 'adjustment' ? t('report.adjustment') : '');
      cell.append(el('span', { class: 'desc-text', text }));
      const n = ctx.notes.get(d.id);
      if (n) cell.append(el('sup', { class: 'rpt-fn', text: String(n) }));
      if (rr && !rr.included && kind !== 'subtotal') cell.append(el('span', { class: 'flag', text: t('report.not_in_total') }));
      const bad = ctx.cellIssues.get(d.id);
      if (bad && bad.size) cell.append(el('span', { class: 'flag issue', text: t('report.not_calculated') }));
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
      } else if (k === 'unit') cell = td(t('report.adjustment_of'), 'unit-col');
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
      const cut = text.length > FOOTNOTE_CHARS ? t('report.footnote_cut', { text: text.slice(0, FOOTNOTE_CHARS).trimEnd() }) : text;
      const cell = td('', 'fn-text', span - 1);
      cell.append(el('span', { class: 'rpt-fn', text: String(n) }), el('span', { class: 'fn-body', text: cut }));
      footnotes.push(el('tr', { class: 'rpt-fnote' }, td('', 'no'), cell));
    }
  });
  if (lines.length === 0) {
    lines.push(el('tr', { class: 'rpt-row empty' }, td('', 'no'), td(details.length ? t('report.no_lines_in_report') : t('report.no_detail_lines'), 'desc muted', span - 1)));
  }
  // Keep the heading with the first two lines, and the last line with the totals.
  lines.forEach((tr, k) => out.push(row(tr, k < 2 || k === lines.length - 1)));

  const foot = [];
  const ch = ctx.baseline ? ctx.baseline.accounts.get(acct.code) || null : null;
  if (ar) {
    foot.push(sumRow(dc, 'sub first', t('report.detail_total'), ar.details_total));
    if (ar.fringe_total) {
      const byName = new Map();
      for (const rr of rows) {
        for (const [n, c] of Object.entries((rr && rr.fringes) || {})) byName.set(n, (byName.get(n) || 0) + c);
      }
      const names = [...byName.keys()].sort();
      const detailNote = names.length > 1 ? names.map((n) => `${n} ${money(byName.get(n))}`).join(', ') : names[0] || '';
      foot.push(sumRow(dc, 'sub', t('report.fringes'), ar.fringe_total, [detailNote, postingNote(b, catRep)].filter(Boolean).join('; ')));
    }
    foot.push(sumRow(dc, 'acct-total', t('report.total_named', { name: `${acct.code || ''} ${acct.name || ''}`.trim() }).trim(), ar.total, null, ch));
  } else {
    foot.push(sumRow(dc, 'acct-total', t('report.total_named', { name: `${acct.code || ''} ${acct.name || ''}`.trim() }).trim(), null));
  }
  const notes = [];
  if (suppressed) {
    notes.push(t('report.lines_not_counted', { count: suppressed }));
  }
  if (hidden) {
    notes.push(t('report.rows_left_out', { count: hidden }));
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
      rows.push(row(el('tr', { class: 'rpt-row empty' }, td('', 'no'), td(accounts.length ? t('report.no_accounts_in_report') : t('report.no_accounts'), 'desc muted', dc.length - 1)), true));
    }
    const foot = [];
    if (rep && rep.fringe_account) {
      const ti = dc.findIndex((c) => c.key === 'total');
      const tr = el('tr', { class: 'rpt-sum fringe-acct first' }, td('', 'no'));
      const lab = td('', 'label', ti - 1);
      lab.append(el('span', { class: 'acct-code', text: rep.fringe_account.code || '' }), t('report.fringes'));
      tr.append(lab, td(m(rep.fringe_account.amount), 'num total'), ...dc.slice(ti + 1).map(() => td('', 'num')));
      foot.push(tr);
    }
    const ch = ctx.baseline ? ctx.baseline.categories.get(cat.code) || null : null;
    foot.push(sumRow(dc, `cat-total${rep && rep.fringe_account ? '' : ' first'}`, t('report.total_named', { name: catLabel(cat) }), rep ? rep.total : null, null, ch));
    foot.forEach((tr, k) => rows.push(row(tr, k < foot.length - 1)));
    flows.push({ name: t('report.flow.detail_named', { name: catLabel(cat) }), cls: 'rpt-dt', cols, head: head(), rows });
  }
  // One line per account: the categories run on, one after another, rather
  // than each starting a page.
  if (ctx.spec.collapse_details && flows.length) {
    return [{ name: t('report.flow.detail_one_line'), cls: 'rpt-dt collapsed', cols, head: head(), rows: flows.flatMap((f) => f.rows) }];
  }
  if (flows.length === 0) {
    flows.push({ name: t('report.flow.detail'), cls: 'rpt-dt', cols, head: head(), rows: [row(el('tr', { class: 'rpt-row empty' }, td(t('report.no_categories_in_report'), 'muted', dc.length)))] });
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
    [t('report.col.fringe'), 'c-name2'],
    [t('report.col.description'), 'c-desc'],
    [t('report.col.type'), 'c-short'],
    [t('report.col.rate'), 'c-num', 'num'],
    [t('report.col.cutoff_or_unit'), 'c-mid'],
    [t('report.col.lines'), 'c-short', 'num'],
    [t('report.col.total'), 'c-money', 'num'],
  ];
  const rows = [];
  let total = 0;
  for (const name of orderedNames(ctx.b, 'fringes')) {
    const f = fr[name] || {};
    const sum = sums[name] || null;
    const pct = f.kind !== 'flat';
    const rate = formula(f.rate);
    const limit = pct ? (typeof f.cutoff === 'number' ? t('report.up_to', { amount: money(f.cutoff) }) : '') : `${f.unit ? t('report.per_unit', { unit: f.unit }) : t('report.per_line')}${flatCountWords(f, ctx.b.units)}`;
    rows.push(row(el('tr', { class: 'rpt-row' }, td(name, 'name'), td(f.description || '', 'txt'), td(pct ? t('report.percent') : t('report.flat'), 'txt'), td(rate.text ? (pct ? `${rate.text}%` : rate.text) : '', `num${rate.formula ? ' formula' : ''}`), td(limit, 'txt'), td(sum ? String(sum.uses) : '', 'num'), td(sum ? m(sum.total) : '', 'num'))));
    if (sum) total += sum.total;
  }
  if (!rows.length) rows.push(emptyRow(t('report.no_fringes'), spec.length));
  else if (ctx.r) rows.push(row(el('tr', { class: 'rpt-st-total' }, td(t('report.total_fringes'), 'name', spec.length - 1), td(money(total), 'num'))));
  return [simpleTable(t('report.section.fringes'), 'rpt-fringes', spec, rows)];
}

/** How many detail rows name each global in a formula. */
function globalUses(ctx, names) {
  const counts = new Map(names.map((n) => [n, 0]));
  if (!names.length) return counts;
  const lower = new Map(names.map((n) => [n.toLowerCase(), n]));
  // Split as the formula reader splits it (1ZG is a name, 1.5 a number).
  const word = /[\d.]*\.[\d.]*|[A-Za-z0-9_]+/g;
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
    [t('report.col.global'), 'c-name2'],
    [t('report.col.formula'), 'c-desc'],
    [t('report.col.value'), 'c-num', 'num'],
    [t('report.col.lines_using_it'), 'c-mid', 'num'],
  ];
  const rows = names.map((n) => {
    const f = formula(g[n]);
    return row(el('tr', { class: 'rpt-row' }, td(n, 'name'), td(f.text, f.formula ? 'formula' : 'txt'), td(values[n] !== undefined ? decimal(values[n]) : '', 'num'), td(String(uses.get(n) || 0), 'num')));
  });
  if (!rows.length) rows.push(emptyRow(t('report.no_globals'), spec.length));
  return [simpleTable(t('report.section.globals'), 'rpt-globals', spec, rows)];
}

function groupsSection(ctx) {
  const groups = ctx.b.groups && typeof ctx.b.groups === 'object' ? ctx.b.groups : {};
  const sums = (ctx.r && ctx.r.groups) || {};
  // The group rule "fringes in group totals" (display only).
  const rules = ctx.b.group_rules && typeof ctx.b.group_rules === 'object' ? ctx.b.group_rules : {};
  const withFringes = rules.fringes_in_totals !== 'exclude';
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
    [t('report.col.group'), 'c-name2'],
    [t('report.col.description'), 'c-desc'],
    [t('report.col.in_total'), 'c-short'],
    [t('report.col.lines'), 'c-short', 'num'],
    [t('report.col.lines_total'), 'c-money', 'num'],
    [t('report.col.fringes'), 'c-money', 'num'],
    [t('report.col.total'), 'c-money', 'num'],
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
    rows.push(row(el('tr', { class: cls }, td(name, 'name'), td(gr.description || '', 'txt'), td(gr.in_budget_total === false ? t('report.no') : t('report.yes'), 'txt'), td(String(lines.get(name) || 0), 'num'), td(s ? m(s.details_total) : '', 'num'), td(s ? m(s.fringe_total) : '', 'num'), td(s ? m(s.details_total + (withFringes ? s.fringe_total : 0)) : '', 'num'))));
  }
  if (!rows.length) rows.push(emptyRow(t('report.no_groups'), spec.length));
  else rows.push(row(el('tr', { class: 'rpt-note-row' }, td(withFringes ? t('report.groups_note') : t('report.groups_note_without_fringes'), 'note', spec.length))));
  return [simpleTable(t('report.flow.groups'), 'rpt-groups', spec, rows)];
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
    [t('report.col.account'), 'c-mid'],
    [t('report.col.no'), 'c-short', 'num'],
    [t('report.col.line'), 'c-name2'],
    [t('report.col.note'), 'c-wide'],
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
    for (const note of printedNotes(ctx, it.row.notes)) add('', place(it.row.code, it.row.name), '', t('report.note_on_category'), note);
    for (const { acct } of accountsOf(ctx, it)) {
      for (const note of printedNotes(ctx, acct.notes)) add('', place(acct.code, acct.name), '', t('report.note_on_account'), note);
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
  if (!rows.length) rows.push(emptyRow(t('report.no_notes'), spec.length));
  return [simpleTable(t('report.flow.notes'), 'rpt-notes', spec, rows)];
}

const POSTING = {
  budget: t('report.post.budget'),
  production_level: t('report.post.production_level'),
  category: t('report.post.category'),
  account: t('report.post.account'),
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
  headRow(t('report.info.budget'));
  fact(t('report.info.title'), ctx.title);
  if (ctx.file) fact(t('report.info.file'), ctx.file);
  fact(t('report.base_currency'), b.base_currency || '');
  if (b.totals_currency) fact(t('report.info.totals_shown_in'), b.totals_currency);
  const posting = b.fringe_posting && b.fringe_posting.kind ? b.fringe_posting.kind : 'category';
  fact(t('report.info.fringes_post'), POSTING[posting] || posting);
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
  fact(
    t('report.info.size'),
    t('report.info.size_value', {
      categories: t('report.count.categories', { count: cats }),
      accounts: t('report.count.accounts', { count: accts }),
      rows: t('report.count.rows', { count: lines }),
    }),
  );
  if (r) fact(t('report.grand_total'), money(r.grand_total));
  if (r && (r.credits || []).length) fact(t('report.net_total'), money(r.net_total));
  const info = b.info && typeof b.info === 'object' ? Object.entries(b.info) : [];
  if (info.length) {
    headRow(t('report.info.information'));
    for (const [label, v] of info) fact(label, infoText(v));
  }
  const curs = b.currencies && typeof b.currencies === 'object' ? Object.entries(b.currencies) : [];
  if (curs.length) {
    headRow(t('report.section.currencies'));
    const at = new Map(orderedNames(b, 'currencies').map((n, i) => [n, i]));
    for (const [code, c] of curs.sort(([a], [z]) => (at.get(a) ?? 0) - (at.get(z) ?? 0))) {
      const rate = c && c.rate !== undefined ? `1 ${code} = ${decimal(c.rate)} ${b.base_currency || ''}` : '';
      fact(code, [c && c.description, rate].filter(Boolean).join(', '));
    }
  }
  return [{ ...simpleTable(t('report.section.info'), 'rpt-info', spec, rows), head: null }];
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
    [t('report.col.acct'), 'c-code2'],
    [t('report.col.description'), 'c-desc'],
    [t('report.col.accounts'), 'c-short', 'num'],
    [t('report.col.rows'), 'c-short', 'num'],
  ];
  const rows = [];
  for (const it of ctx.items) {
    if (it.kind === 'level') {
      rows.push(row(el('tr', { class: 'rpt-row rpt-chart-level' }, td('', 'code'), td(it.row.name || t('report.production_total'), 'txt', 3))));
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
  if (!rows.length) rows.push(emptyRow(t('report.no_categories_yet'), spec.length));
  return [simpleTable(t('report.section.chart'), 'rpt-chart', spec, rows)];
}

function unitsSection(ctx) {
  const units = ctx.b.units && typeof ctx.b.units === 'object' ? ctx.b.units : {};
  const uses = countBy(ctx, (d) => [d.unit, d.unit2, d.unit3, d.unit4]);
  const spec = [
    [t('report.col.unit'), 'c-name2'],
    [t('report.col.plural'), 'c-mid'],
    [t('report.col.description'), 'c-desc'],
    [t('report.col.hours'), 'c-short', 'num'],
    [t('report.col.rows_using_it'), 'c-mid', 'num'],
  ];
  const rows = Object.keys(units)
    .sort()
    .map((name) => {
      const u = units[name] || {};
      const hours = u.hours !== undefined && u.hours !== null && u.hours !== '' ? decimal(String(u.hours)) : '';
      return row(el('tr', { class: 'rpt-row' }, td(name, 'name'), td(u.plural || '', 'txt'), td(u.description || '', 'txt'), td(hours, 'num'), td(String(uses.get(name) || 0), 'num')));
    });
  if (!rows.length) rows.push(emptyRow(t('report.no_units'), spec.length));
  return [simpleTable(t('report.section.units'), 'rpt-units', spec, rows)];
}

/** Sets or locations: each with its description, how many rows, and what they add up to. */
function breakdownSection(ctx, { title, cls, table, key, sums, field, label, empty, place }) {
  const items = table && typeof table === 'object' ? table : {};
  const uses = countBy(ctx, (d) => [d[field]]);
  const spec = [
    [label, 'c-name2'],
    [t('report.col.description'), 'c-desc'],
    [t('report.col.rows'), 'c-short', 'num'],
    [t('report.col.lines_total'), 'c-money', 'num'],
    [t('report.col.fringes'), 'c-money', 'num'],
    [t('report.col.total'), 'c-money', 'num'],
  ];
  const rows = [];
  for (const name of key ? orderedNames(ctx.b, key) : Object.keys(items).sort()) {
    const it = items[name] || {};
    const sum = sums && sums[name] ? sums[name] : null;
    const desc = [it.description, place ? place(it) : ''].filter(Boolean).join('; ');
    rows.push(row(el('tr', { class: 'rpt-row' }, td(name, 'name'), td(desc, 'txt'), td(String(uses.get(name) || 0), 'num'), td(sum ? m(sum.details_total) : '', 'num'), td(sum ? m(sum.fringe_total) : '', 'num'), td(sum ? m(sum.details_total + sum.fringe_total) : '', 'num'))));
  }
  if (!rows.length) rows.push(emptyRow(empty, spec.length));
  else rows.push(row(el('tr', { class: 'rpt-note-row' }, td(t('report.breakdown_note'), 'note', spec.length))));
  return [simpleTable(title, cls, spec, rows)];
}

function setsSection(ctx) {
  return breakdownSection(ctx, { title: t('report.flow.sets'), cls: 'rpt-sets', table: ctx.b.sets, key: 'sets', sums: ctx.r && ctx.r.sets, field: 'set', label: t('report.col.set'), empty: t('report.no_sets') });
}

function locationsSection(ctx) {
  return breakdownSection(ctx, {
    title: t('report.flow.locations'),
    cls: 'rpt-locations',
    table: ctx.b.locations,
    key: 'locations',
    sums: ctx.r && ctx.r.locations,
    field: 'location',
    label: t('report.col.location'),
    empty: t('report.no_locations'),
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
    [t('report.col.code'), 'c-short'],
    [t('report.col.description'), 'c-desc'],
    [t('report.col.symbol'), 'c-short'],
    [t('report.col.rate'), 'c-mid'],
    [t('report.col.rows'), 'c-short', 'num'],
  ];
  const rows = [];
  if (base) rows.push(row(el('tr', { class: 'rpt-row' }, td(base, 'name'), td(t('report.base_currency'), 'txt'), td(curs[base] && curs[base].symbol ? curs[base].symbol : '', 'txt'), td('', 'txt'), td(String(uses.get(base) || 0), 'num'))));
  for (const code of orderedNames(ctx.b, 'currencies')) {
    if (code === base) continue;
    const c = curs[code] || {};
    const rate = c.rate !== undefined ? `1 ${code} = ${decimal(String(c.rate))} ${base}` : '';
    rows.push(row(el('tr', { class: 'rpt-row' }, td(code, 'name'), td(c.description || '', 'txt'), td(c.symbol || '', 'txt'), td(rate, 'txt'), td(String(uses.get(code) || 0), 'num'))));
  }
  if (b.totals_currency) rows.push(row(el('tr', { class: 'rpt-note-row' }, td(t('report.totals_also_shown', { currency: b.totals_currency }), 'note', spec.length))));
  if (!rows.length) rows.push(emptyRow(t('report.no_currencies'), spec.length));
  return [simpleTable(t('report.section.currencies'), 'rpt-currencies', spec, rows)];
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
    [t('report.col.acct'), 'c-code2'],
    [t('report.col.description'), 'c-desc'],
    [t('report.col.budget'), 'c-cm', 'num'],
    ...(withBase ? [[t('report.col.baseline'), 'c-cm', 'num']] : []),
    [t('report.col.actual'), 'c-cm', 'num'],
    [t('report.col.etc'), 'c-cm', 'num'],
    [t('report.col.eac'), 'c-cm', 'num'],
    [t('report.col.variance'), 'c-cm', 'num'],
    [t('report.col.spent'), 'c-pct', 'num'],
  ];
  const rows = [];
  if (!cost) {
    rows.push(emptyRow(t('report.no_actuals'), spec.length));
    return [simpleTable(t('report.section.cost_report'), 'rpt-cost', spec, rows)];
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
      if (lc && lc.posted_fringes) line('rpt-cost-other', '', t('report.fringes'), lc.posted_fringes);
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
    if (cc.fringe_account) line('rpt-cost-acct', cc.fringe_account.code, t('report.fringes'), cc.fringe_account.figures);
    line('rpt-st-total', '', t('report.total_named', { name: catLabel(it.row) }), cc.figures);
  }
  const sum = [];
  sum.push([t('report.total_atl'), cost.total_atl]);
  sum.push([t('report.total_btl'), cost.total_btl]);
  sum.push([t('report.total_atl_btl'), cost.atl_btl]);
  if (cost.budget_fringes) sum.push([t('report.total_fringes'), cost.budget_fringes.figures]);
  (cost.charges || []).forEach((c, i) => sum.push([ctx.r && ctx.r.charges && ctx.r.charges[i] ? chargeLabel(ctx.b, ctx.r.charges[i], i, money) : c.name, c.figures]));
  if (cost.unassigned && cost.unassigned.count) sum.push([t('report.unassigned_actuals'), cost.unassigned]);
  sum.forEach(([label, f], i) => line(i === 0 ? 'rpt-cost-sum first' : 'rpt-cost-sum', '', label, f, true));
  line('rpt-st-total rpt-cost-grand', '', t('report.grand_total'), cost.total);
  rows.push(
    row(
      el(
        'tr',
        { class: 'rpt-note-row' },
        td(t('report.cost_note', { count: cost.transactions }), 'note', spec.length),
      ),
    ),
  );
  return [simpleTable(t('report.section.cost_report'), 'rpt-cost', spec, rows)];
}

/**
 * The bid (decisions.md, entry 92), from the view's bid_sheet: its header
 * fields, then its lines in the layout's order, the Unassigned bucket, the
 * budget's own charges and credits and the bid total, with a line saying how
 * it reconciles. Every figure is the engine's.
 */
function bidSection(ctx) {
  const sheet = ctx.view && ctx.view.bid_sheet && Array.isArray(ctx.view.bid_sheet.lines) ? ctx.view.bid_sheet : null;
  const spec = [
    [t('report.col.acct'), 'c-code2'],
    [t('report.col.description'), 'c-desc'],
    [t('report.col.amount'), 'c-money', 'num'],
  ];
  const rows = [];
  if (!sheet) {
    const why = ctx.view && ctx.view.bid_error ? ctx.view.bid_error : t('report.bid_failed');
    rows.push(emptyRow(why, spec.length));
    return [simpleTable(t('report.section.bid'), 'rpt-bid', spec, rows)];
  }
  const showAccounts = Boolean(ctx.spec.bid && ctx.spec.bid.show_accounts);
  // The header fields head the same table, so they print with the bid.
  const fields = Array.isArray(sheet.fields) ? sheet.fields : [];
  fields.forEach((f, i) => {
    const cell = td('', 'txt', spec.length);
    cell.append(el('span', { class: 'rpt-bid-label', text: f.label || '' }), el('span', { class: 'rpt-bid-value', text: f.value || '' }));
    rows.push(row(el('tr', { class: `rpt-row rpt-bid-field${i === fields.length - 1 ? ' last' : ''}` }, cell), true));
  });
  const line = (cls, code, label, cents, keep = false) => rows.push(row(el('tr', { class: `rpt-row ${cls}` }, td(code, 'code'), td(label, cls.includes('total') ? 'name' : 'txt'), td(m(cents), 'num')), keep));
  const heading = (text) => {
    const cap = td('', 'cat', spec.length);
    cap.append(el('span', { class: 'cat-name', text }));
    rows.push(row(el('tr', { class: 'rpt-bid-head' }, cap), true));
  };
  const accounts = (list) => {
    for (const a of list) line('rpt-bid-acct', a.code || '', a.name || '', a.amount);
  };
  for (const l of sheet.lines) {
    if (l.kind === 'section' && showAccounts) {
      heading(l.name);
      accounts(l.accounts || []);
      line('rpt-st-total', '', t('report.total_named', { name: l.name }), l.total);
    } else if (l.kind === 'section') {
      line('rpt-bid-section', '', l.name, l.total);
    } else if (l.kind === 'subtotal') {
      line('rpt-st-total', '', l.name, l.amount);
    } else if (l.kind === 'markup') {
      const label = l.percent === null || l.percent === undefined ? l.name : t('report.markup', { name: l.name, percent: decimal(l.percent), base: money(l.base) });
      line('rpt-bid-markup', '', label, l.amount);
    }
  }
  const unassigned = Array.isArray(sheet.unassigned) ? sheet.unassigned : [];
  if (unassigned.length) {
    heading(t('report.unassigned'));
    const shown = unassigned.filter((a) => a.amount !== 0);
    accounts(shown);
    const zero = unassigned.length - shown.length;
    if (zero) rows.push(row(el('tr', { class: 'rpt-note-row' }, td(t('report.more_at_zero', { count: zero }), 'note', spec.length))));
    line('rpt-st-total', '', t('report.total_unassigned'), sheet.unassigned_total);
  }
  if (sheet.unposted_fringes) line('rpt-bid-budget', '', t('report.unposted_fringes'), sheet.unposted_fringes);
  (sheet.charges || []).forEach((c, i) => line(`rpt-bid-budget${i === 0 ? ' first' : ''}`, '', c.name, c.amount, true));
  for (const c of sheet.credits || []) line('rpt-bid-budget', '', t('report.less', { name: c.name }), -c.amount, true);
  line('rpt-st-total rpt-bid-total', '', t('report.bid_total'), sheet.total, true);
  rows.push(
    row(
      el(
        'tr',
        { class: 'rpt-note-row' },
        td(t('report.bid_note', { net: money(sheet.budget_net), markups: money(sheet.markups) }), 'note', spec.length),
      ),
    ),
  );
  for (const w of sheet.warnings || []) rows.push(row(el('tr', { class: 'rpt-note-row' }, td(t('report.check', { warning: w }), 'note', spec.length))));
  return [simpleTable(t('report.section.bid'), 'rpt-bid', spec, rows)];
}

// --- Incentives by location (decisions.md, entry 93) ---

/**
 * Each location's spend and what each incentive program counts there, then
 * every program with its terms, then the totals: the credits applied are
 * the topsheet's credits, the net total the topsheet's. From the full
 * view's `incentives` (the same estimates as the Credits tab).
 */
function incentivesSection(ctx) {
  const inc = ctx.view && ctx.view.incentives && Array.isArray(ctx.view.incentives.locations) ? ctx.view.incentives : null;
  const where = (l) => (l === null || l === undefined ? t('report.no_location') : String(l));
  const status = (p) => (p.amount === null || p.amount === undefined ? t('report.status.problem') : p.applied ? t('report.status.applied') : t('report.status.estimate'));
  const spendSpec = [
    [t('report.col.location_and_program'), 'c-desc'],
    [t('report.col.status'), 'c-short'],
    [t('report.col.spend'), 'c-cm', 'num'],
    [t('report.col.qualified'), 'c-cm', 'num'],
  ];
  if (!inc) {
    return [simpleTable(t('report.section.incentives'), 'rpt-incent', spendSpec, [emptyRow(t('report.incentives_blocked'), spendSpec.length)])];
  }
  const programs = Array.isArray(inc.programs) ? inc.programs : [];
  const spendRows = [];
  for (const l of inc.locations) {
    const counted = Array.isArray(l.programs) ? l.programs : [];
    spendRows.push(row(el('tr', { class: 'rpt-row rpt-incent-loc' }, td(where(l.location), 'name'), td(l.known === false ? t('report.not_in_list') : '', 'txt'), td(m(l.total), 'num'), td('', 'num')), counted.length > 0));
    for (const c of counted) {
      const p = programs.find((q) => q.index === c.index) || c;
      spendRows.push(row(el('tr', { class: 'rpt-row indent-1' }, td(c.name, 'desc'), td(status(p), 'txt'), td('', 'num'), td(m(c.qualified_here), 'num'))));
    }
    if (!counted.length) spendRows.push(row(el('tr', { class: 'rpt-row indent-1' }, td(t('report.no_program_counts'), 'desc muted'), td('', 'txt'), td('', 'num'), td('', 'num'))));
  }
  spendRows.push(row(el('tr', { class: 'rpt-row rpt-st-total' }, td(t('report.spend_everywhere'), 'name'), td('', 'txt'), td(m(inc.spend_total), 'num'), td('', 'num')), true));
  spendRows.push(row(el('tr', { class: 'rpt-note-row' }, td(t('report.spend_note'), 'note', spendSpec.length))));

  const progSpec = [
    [t('report.col.program'), 'c-desc'],
    [t('report.col.status'), 'c-short'],
    [t('report.col.qualified'), 'c-cm', 'num'],
    [t('report.col.minimum'), 'c-cm', 'num'],
    [t('report.col.cap'), 'c-cm', 'num'],
    [t('report.col.discount'), 'c-cm', 'num'],
    [t('report.col.fee'), 'c-cm', 'num'],
    [t('report.col.credit'), 'c-cm', 'num'],
  ];
  const progRows = [];
  for (const p of programs) {
    const notes = [];
    const places = Array.isArray(p.locations) && p.locations.length ? p.locations.join(', ') : t('report.anywhere');
    notes.push(t('report.spend_at', { places }));
    if (p.below_minimum) notes.push(t('report.below_minimum'));
    if (p.capped) notes.push(t('report.held_to_cap'));
    if (p.shared_cap) notes.push(p.shared === null || p.shared === undefined ? t('report.shares_cap', { cap: p.shared_cap }) : t('report.share_of_cap', { cap: p.shared_cap, amount: m(p.shared) }));
    if (p.typed) notes.push(t('report.includes_typed', { amount: m(p.typed) }));
    if (p.problem) notes.push(String(p.problem));
    progRows.push(
      row(
        el('tr', { class: 'rpt-row' }, td(p.name, 'name'), td(status(p), 'txt'), td(m(p.qualified), 'num'), td(p.minimum_spend === null || p.minimum_spend === undefined ? '' : m(p.minimum_spend), 'num'), td(p.cap === null || p.cap === undefined ? '' : m(p.cap), 'num'), td(m(p.discount), 'num'), td(m(p.fee), 'num'), td(m(p.amount), 'num')),
        true,
      ),
    );
    progRows.push(row(el('tr', { class: 'rpt-row indent-1' }, td(notes.join('; '), 'desc muted', progSpec.length))));
  }
  if (!programs.length) progRows.push(emptyRow(t('report.no_programs'), progSpec.length));
  const sum = (label, v, first) => progRows.push(row(el('tr', { class: `rpt-row${first ? ' rpt-st-total' : ''}` }, td(label, 'name', progSpec.length - 1), td(m(v), 'num')), first));
  sum(t('report.credits_applied'), inc.applied_total, true);
  sum(t('report.estimates_not_applied'), inc.estimates_total, false);
  sum(t('report.grand_total'), inc.grand_total, false);
  sum(t('report.net_total'), inc.net_total, false);
  progRows.push(row(el('tr', { class: 'rpt-note-row' }, td(inc.partial ? t('report.incentives_note_partial') : t('report.incentives_note'), 'note', progSpec.length))));
  return [simpleTable(t('report.section.incentives'), 'rpt-incent', spendSpec, spendRows), simpleTable(t('report.flow.incentive_programs'), 'rpt-incent', progSpec, progRows)];
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
  bid: bidSection,
  incentives: incentivesSection,
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
      return t('report.band.printed_at', { when: ctx.printed });
    case 'date':
      return ctx.printedDate;
    case 'time':
      return ctx.printedTime;
    case 'file':
      return ctx.file || '';
    case 'page':
      return t('report.band.page_of', { page: page.n, pages: page.of });
    case 'info': {
      const v = ctx.b.info && ctx.b.info[f.label];
      return v ? t('report.band.labeled', { label: f.label, value: infoText(v) }) : '';
    }
    case 'global': {
      const v = ctx.r && ctx.r.globals ? ctx.r.globals[f.name] : undefined;
      return v !== undefined && v !== null ? t('report.band.labeled', { label: f.name, value: decimal(v) }) : '';
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
  const root = el('div', { class: `${rootClass(spec)} rpt-pages`, attrs: { 'aria-label': t('report.aria_pages', { title: ctx.title }) } });
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
    title: (typeof b.title === 'string' && b.title.trim()) || t('report.untitled'),
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
    flows.push({ name: t('report.flow.report'), cls: 'rpt-st', cols: ['c-wide'], head: null, rows: [emptyRow(t('report.choose_a_section'), 1)] });
  }
  flows[0].intro = el('div', { class: 'rpt-intro' }, titleBlock(ctx, SECTIONS.find((x) => x.key === s.sections[0])?.label || t('report.flow.report')));
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
      outcome = await invoke('print_report', { pdf, landscape, title: title || t('report.flow.report') });
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
  const title = view && view.budget && typeof view.budget.title === 'string' ? view.budget.title.trim() : '';
  return [title || t('report.job_budget'), extra].filter(Boolean).join(' - ');
}

/**
 * Prints a report: through the print sheet, or with `{ pdf: true }` straight
 * into a PDF file. `name` (a preset's name) goes into the job's title.
 * Resolves with { done, file }.
 */
export async function printReport(view, spec = 'both', { pdf = false, name = '' } = {}) {
  const s = normalizeSpec(spec);
  const v = await withBid(view, s);
  return printPages((root) => renderReport(root, v, s), { landscape: s.landscape, pdf, title: jobTitle(view, name || t('report.flow.report')) });
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
    title: [opts.title, opts.section].filter(Boolean).join(' - ') || t('report.flow.report'),
  });
}
