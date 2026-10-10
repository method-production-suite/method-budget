// File > Compare Budgets…: the budget in front (the base) side by side with 1
// to 15 others: open budgets (tabs), versions saved in this budget, and .fwb
// files picked in the Open dialog (src/compare.rs; decisions.md, entries 17,
// 19 and 27).
//
// Left: what to compare with, a check box each; the letters A, B, C... follow
// the order they were ticked. Right: categories (open one for its accounts,
// an account for its lines), each production total, above and below the line,
// fringes, charges (each one under their total), the grand total, credits
// (the same) and the net total. Columns: each
// budget's total, then (decision 19) both each budget's difference from this
// one (amount and percent of this one) and, when ticked, the other budgets'
// Sum and Average, their Variance (sum minus this one) and Variance % (of the
// average). A row missing from some budgets says which.
//
// A click on a budget's figure goes there when that budget is open in a tab
// (this one included); a click on a row's name goes to it in this budget (or
// the first open one that has it). Going closes the comparison; opening it
// again brings back the same budgets.
//
// Print… and Save as PDF… lay the comparison as shown (the columns ticked, the
// rows open) out into landscape pages through the report module's printHtml,
// with a legend of which budget each letter is.
//
// Money is integer cents from Rust and percents are Rust's text (two
// decimals): no arithmetic here. Budget text goes in with textContent only;
// no inline styles (the CSP).

import { invoke } from './api.js';
import { el } from './dom.js';
import { customDialog } from './modal.js';
import { money } from './format.js';
import { readPref, writePref } from './prefs.js';
import { createdText, versionsOf } from './versions.js';

/** Most budgets compared with this one at once (src/compare.rs MAX_OTHERS). */
export const MAX_OTHERS = 15;

/** The optional columns, as the header's check boxes list them. */
export const COLUMN_CHOICES = [
  { key: 'diff', label: 'Differences', title: 'Each budget minus this one, and that as a percent of this one', on: true },
  { key: 'sum', label: 'Sum', title: 'The other budgets added together', on: false },
  { key: 'average', label: 'Average', title: 'The other budgets’ sum divided by how many there are', on: false },
  { key: 'variance', label: 'Variance', title: 'The other budgets’ sum minus this one', on: false },
  { key: 'percent', label: 'Variance %', title: 'The variance as a percent of the average', on: false },
];

/** Column choices saved per user, with defaults for the rest. */
export function readColumns() {
  const saved = readPref('compareColumns', null);
  const out = {};
  for (const c of COLUMN_CHOICES) out[c.key] = saved && typeof saved[c.key] === 'boolean' ? saved[c.key] : c.on;
  return out;
}

/** "A", "B", ... "O" for the others, by place (0 is this budget). */
export function letter(i) {
  return i <= 0 ? '' : String.fromCharCode(64 + i);
}

/** A percent from Rust ("-12.50") for display: "(12.50%)" when negative; '' for none. */
export function percentText(p) {
  if (p === null || p === undefined || p === '') return '';
  const s = String(p);
  return s.startsWith('-') ? `(${s.slice(1)}%)` : `${s}%`;
}

/** Same source? */
export function sameSource(a, b) {
  if (!a || !b || a.kind !== b.kind) return false;
  if (a.kind === 'tab') return a.doc === b.doc;
  if (a.kind === 'version') return a.id === b.id;
  return a.token === b.token;
}

// What each base budget was last compared with: doc id -> { sources, files,
// open }. Kept for the session only.
const memory = new Map();

function remembered(doc) {
  let m = memory.get(doc);
  if (!m) {
    m = { sources: [], files: [], open: new Set() };
    memory.set(doc, m);
  }
  return m;
}

/**
 * The columns to draw: [{ kind, i }] where kind is 'value' (budget i's
 * total), 'diff' and 'pct' (budget i against this one), or an aggregate.
 */
export function columnPlan(count, cols) {
  const out = [];
  for (let i = 0; i < count; i++) {
    out.push({ kind: 'value', i });
    if (i > 0 && cols.diff) out.push({ kind: 'diff', i }, { kind: 'pct', i });
  }
  if (count > 1) {
    if (cols.sum) out.push({ kind: 'sum' });
    if (cols.average) out.push({ kind: 'average' });
    if (cols.variance) out.push({ kind: 'variance' });
    if (cols.percent) out.push({ kind: 'percent' });
  }
  return out;
}

/** The text of one figure cell for a compared line. */
export function cellText(line, col) {
  const agg = line.agg;
  switch (col.kind) {
    case 'value': {
      const v = line.values[col.i];
      return v === null || v === undefined ? '' : money(v);
    }
    case 'diff':
      return money(line.diffs[col.i].amount);
    case 'pct':
      return percentText(line.diffs[col.i].percent);
    case 'sum':
      return agg ? money(agg.sum) : '';
    case 'average':
      return agg ? money(agg.average) : '';
    case 'variance':
      return agg ? money(agg.variance) : '';
    case 'percent':
      return agg ? percentText(agg.percent) : '';
    default:
      return '';
  }
}

/** Which budgets lack the line: their letters ("this" for the base). */
export function missingFrom(line) {
  const out = [];
  line.values.forEach((v, i) => {
    if (v === null || v === undefined) out.push(i === 0 ? 'this budget' : letter(i));
  });
  return out;
}

/**
 * Opens the compare screen for the budget on screen. `ctx`: { doc(), view(),
 * tabs(), jump(place), pickFiles() }. `opts.version`: compare with just
 * that version (from the Versions dialog).
 */
export function showCompare(ctx, opts = {}) {
  const base = ctx.doc();
  if (base === null || base === undefined) return;
  const view = ctx.view();
  const m = remembered(base);
  if (opts.version !== undefined && opts.version !== null) {
    m.sources = [{ kind: 'version', id: opts.version }];
    m.open = new Set();
  }
  // Just that open budget (a returned copy, from sharing.js).
  if (typeof opts.tab === 'number' && opts.tab !== base) {
    m.sources = [{ kind: 'tab', doc: opts.tab }];
    m.open = new Set();
  }
  // Drop what has gone since: closed tabs, deleted versions.
  const tabIds = new Set((ctx.tabs() || []).map((t) => t.id));
  const versionIds = new Set(versionsOf(view).map((v) => v.id));
  m.sources = m.sources.filter((s) => (s.kind === 'tab' ? tabIds.has(s.doc) && s.doc !== base : s.kind === 'version' ? versionIds.has(s.id) : true));

  let cols = readColumns();
  let result = null; // the last compare_budgets reply
  let problem = '';
  let loading = false;
  let seq = 0;
  const details = new Map(); // account key -> DetailDto | { loading } | { error }

  const sidebar = el('div', { class: 'cmp-side', attrs: { role: 'group', 'aria-label': 'Budgets to compare with' } });
  const tableHost = el('div', { class: 'cmp-table-host', attrs: { tabindex: '-1' } });
  const status = el('p', { class: 'cmp-status', attrs: { role: 'status' } });

  let closeFn = null;

  // -------------------------------------------------------------------------
  // The sources

  function selected(src) {
    return m.sources.findIndex((s) => sameSource(s, src));
  }

  function toggle(src, on) {
    const at = selected(src);
    if (on && at < 0) {
      if (m.sources.length >= MAX_OTHERS) return;
      m.sources.push(src);
    } else if (!on && at >= 0) {
      m.sources.splice(at, 1);
    }
    details.clear();
    drawSide();
    refresh();
  }

  function candidate(src, label, sub, title) {
    const at = selected(src);
    const box = el('input', { attrs: { type: 'checkbox', 'aria-label': `Compare with ${label}` } });
    box.checked = at >= 0;
    box.disabled = at < 0 && m.sources.length >= MAX_OTHERS;
    box.addEventListener('change', () => toggle(src, box.checked));
    const badge = el('span', { class: 'cmp-letter', text: at >= 0 ? letter(at + 1) : '', attrs: { 'aria-hidden': 'true' } });
    return el(
      'label',
      { class: at >= 0 ? 'cmp-cand is-on' : 'cmp-cand', title: title || label },
      box,
      badge,
      el('span', { class: 'cmp-cand-text' }, el('span', { class: 'cmp-cand-name', text: label }), sub ? el('span', { class: 'cmp-cand-sub', text: sub }) : null),
    );
  }

  function section(title, items, extra = null) {
    return el(
      'section',
      { class: 'cmp-side-section' },
      el('h3', { class: 'cmp-side-head', text: title }),
      items.length ? el('div', { class: 'cmp-side-list' }, ...items) : el('p', { class: 'cmp-side-empty', text: 'None' }),
      extra,
    );
  }

  async function addFiles() {
    let picked;
    try {
      picked = await ctx.pickFiles();
    } catch (e) {
      problem = String(e);
      drawTable();
      return;
    }
    if (!Array.isArray(picked) || !picked.length) return;
    for (const p of picked) {
      const src = p.source;
      if (!src) continue;
      if (src.kind === 'file' && !m.files.some((f) => f.token === src.token)) {
        m.files.push({ token: src.token, label: p.label, path: p.path });
      }
      if (selected(src) < 0 && m.sources.length < MAX_OTHERS && !(src.kind === 'tab' && src.doc === base)) m.sources.push(src);
    }
    details.clear();
    drawSide();
    refresh();
  }

  function drawSide() {
    const tabs = (ctx.tabs() || []).filter((t) => t.id !== base);
    const versions = versionsOf(view);
    const addBtn = el('button', { class: 'btn cmp-add-files', text: 'Add Files…', attrs: { type: 'button' } });
    addBtn.disabled = m.sources.length >= MAX_OTHERS;
    addBtn.addEventListener('click', addFiles);
    const count = m.sources.length;
    sidebar.replaceChildren(
      el('p', {
        class: 'cmp-side-count',
        text: count ? `Comparing with ${count} of up to ${MAX_OTHERS}` : `Tick up to ${MAX_OTHERS} budgets to compare with this one.`,
      }),
      section(
        'Open budgets',
        tabs.map((t) => candidate({ kind: 'tab', doc: t.id }, t.label, t.dirty ? 'Unsaved changes' : '', t.path || t.title)),
      ),
      section(
        'Versions of this budget',
        versions.map((v) => candidate({ kind: 'version', id: v.id }, v.name, createdText(v.created), v.note || v.name)),
      ),
      section(
        'Files',
        m.files.map((f) => candidate({ kind: 'file', token: f.token }, f.label, '', f.path)),
        addBtn,
      ),
    );
  }

  // -------------------------------------------------------------------------
  // Fetching

  async function refresh() {
    const mine = ++seq;
    problem = '';
    if (!m.sources.length) {
      result = null;
      loading = false;
      drawTable();
      return;
    }
    loading = true;
    drawTable();
    try {
      const r = await invoke('compare_budgets', { doc: base, sources: m.sources });
      if (mine !== seq) return;
      result = r;
    } catch (e) {
      if (mine !== seq) return;
      result = null;
      problem = String(e);
    }
    loading = false;
    drawTable();
  }

  /** Fetches one account's lines (its row shows "Comparing lines…" meanwhile). */
  async function loadDetail(key, code) {
    details.set(key, { loading: true });
    const mine = seq;
    try {
      const d = await invoke('compare_detail', { doc: base, sources: m.sources, code });
      if (mine !== seq) return;
      details.set(key, d);
    } catch (e) {
      if (mine !== seq) return;
      details.set(key, { error: String(e) });
    }
    drawTable();
  }

  // -------------------------------------------------------------------------
  // The table

  function go(place) {
    if (!place) return;
    if (closeFn) closeFn();
    ctx.jump(place);
  }

  /** Where line `i`'s budget has the row, when it is open; null otherwise. */
  function placeFor(sources, i, make) {
    const s = sources[i];
    if (!s || typeof s.doc !== 'number') return null;
    return make(i, s.doc);
  }

  /** The first open budget with the row: this one if it has it. */
  function firstPlace(sources, make) {
    for (let i = 0; i < sources.length; i++) {
      const p = placeFor(sources, i, make);
      if (p) return p;
    }
    return null;
  }

  function headCells(sources, plan) {
    const th = (top, name, cls, title) =>
      el('th', { class: cls, title: title || undefined, attrs: { scope: 'col' } }, el('span', { class: 'cmp-h-top', text: top }), el('span', { class: 'cmp-h-name', text: name }));
    const n = sources.length;
    const others = n > 2 ? `A to ${letter(n - 1)}` : 'A';
    const cells = [th('Acct #', '', 'cmp-code'), th('Description', '', 'cmp-name')];
    for (const c of plan) {
      const s = sources[c.i];
      const partial = s && s.partial ? ' (some cells with problems count as blank)' : '';
      if (c.kind === 'value') {
        const top = c.i === 0 ? 'This budget' : letter(c.i);
        const kind = s.kind === 'version' ? `Version saved ${createdText(s.detail)}` : s.detail || s.label;
        cells.push(th(top, s.label, s.partial ? 'num cmp-val is-partial' : 'num cmp-val', `${s.label}\n${kind}${partial}`));
      } else if (c.kind === 'diff') {
        cells.push(th(`${letter(c.i)} − this`, 'Difference', 'num cmp-diff', `${s.label} minus this budget`));
      } else if (c.kind === 'pct') {
        cells.push(th(`${letter(c.i)} %`, 'of this', 'num cmp-pct', `${s.label}’s difference as a percent of this budget`));
      } else if (c.kind === 'sum') {
        cells.push(th('Sum', others, 'num cmp-agg', `${others} added together`));
      } else if (c.kind === 'average') {
        cells.push(th('Average', others, 'num cmp-agg', `${others} added together, divided by ${n - 1}`));
      } else if (c.kind === 'variance') {
        cells.push(th('Variance', 'Sum − this', 'num cmp-agg', 'The sum of the other budgets minus this budget'));
      } else if (c.kind === 'percent') {
        cells.push(th('Variance %', 'of average', 'num cmp-agg', 'The variance as a percent of the average'));
      }
    }
    return cells;
  }

  /**
   * One row. spec: { cls, code, name, line, toggle: { open, label, onClick },
   * place(i, doc) -> place or null, missingNote, title }.
   */
  function row(sources, plan, spec) {
    const tr = el('tr', { class: spec.cls || '' });
    const missing = spec.line ? missingFrom(spec.line) : [];
    if (missing.length) tr.classList.add('unmatched');
    const codeTd = el('td', { class: 'cmp-code' });
    if (spec.toggle) {
      const t = el('button', {
        class: 'cmp-toggle',
        text: spec.toggle.open ? '▾' : '▸',
        title: spec.toggle.label,
        attrs: { type: 'button', 'aria-expanded': String(spec.toggle.open), 'aria-label': spec.toggle.label },
      });
      t.dataset.key = spec.toggle.key;
      t.addEventListener('click', (e) => {
        e.stopPropagation();
        spec.toggle.onClick();
      });
      codeTd.append(t);
    }
    if (spec.code) codeTd.append(el('span', { class: 'cmp-code-text', text: spec.code }));
    tr.append(codeTd);

    const nameTd = el('td', { class: 'cmp-name' });
    const target = spec.place ? firstPlace(sources, spec.place) : null;
    if (target) {
      const where = sources.findIndex((s) => s && s.doc === target.doc);
      const b = el('button', {
        class: 'cmp-go',
        text: spec.name || spec.code || '',
        title: `Show in ${where === 0 ? 'this budget' : sources[where].label}`,
        attrs: { type: 'button' },
      });
      b.addEventListener('click', () => go(target));
      nameTd.append(b);
    } else {
      nameTd.append(el('span', { text: spec.name || '', title: spec.title || undefined }));
    }
    if (missing.length) {
      nameTd.append(el('span', { class: 'cmp-missing', text: `not in ${missing.join(', ')}` }));
    }
    tr.append(nameTd);

    for (const c of plan) {
      const text = spec.line ? cellText(spec.line, c) : '';
      const td = el('td', { class: `num cmp-${c.kind === 'value' ? 'val' : c.kind === 'diff' || c.kind === 'pct' ? c.kind : 'agg'}` });
      if (c.kind === 'value' && spec.line && (spec.line.values[c.i] === null || spec.line.values[c.i] === undefined)) {
        td.classList.add('none');
        td.textContent = 'none';
        td.title = 'Not in this budget';
      } else {
        td.textContent = text;
      }
      if (c.kind === 'value' && spec.place && spec.line && spec.line.values[c.i] !== null && spec.line.values[c.i] !== undefined) {
        const p = placeFor(sources, c.i, spec.place);
        if (p) {
          td.classList.add('go');
          td.title = `Show in ${c.i === 0 ? 'this budget' : sources[c.i].label}`;
          td.addEventListener('click', () => go(p));
        }
      }
      tr.append(td);
    }
    return tr;
  }

  function detailRows(sources, plan, d) {
    const out = [];
    const width = plan.length + 2;
    const note = (text, cls = 'cmp-note') => {
      const td = el('td', { class: cls, text });
      td.colSpan = width;
      return el('tr', { class: 'cmp-detail-note' }, td);
    };
    if (!d || d.loading) return [note('Comparing lines…')];
    if (d.error) return [note(d.error, 'cmp-note is-error')];
    if (!d.rows.length) out.push(note('No lines in any of these budgets.'));
    const how = d.matched_by === 'id' ? 'Lines matched as the same line across versions.' : 'Lines matched by description.';
    out.push(note(how));
    for (const r of d.rows) {
      out.push(
        row(sources, plan, {
          cls: `cmp-line kind-${r.kind}`,
          code: '',
          name: r.description || (r.kind === 'adjustment' ? 'Adjustment' : '(no description)'),
          line: r.line,
          place: (i, doc) => {
            const at = d.at[i];
            const id = r.ids[i];
            return at && id !== null && id !== undefined ? { doc, kind: 'row', ci: at[0], ai: at[1], rowId: id } : null;
          },
        }),
      );
    }
    out.push(row(sources, plan, { cls: 'cmp-line-total', code: '', name: 'Total lines', line: d.details_total }));
    return out;
  }

  function drawTable() {
    status.textContent = loading ? 'Comparing…' : '';
    const nothing = !result || !m.sources.length || Boolean(problem);
    printBtn.disabled = nothing;
    pdfBtn.disabled = nothing;
    if (problem) {
      tableHost.replaceChildren(el('p', { class: 'cmp-empty is-error', text: problem }));
      return;
    }
    if (!m.sources.length) {
      tableHost.replaceChildren(el('p', { class: 'cmp-empty', text: 'Tick a budget on the left to compare it with this one.' }));
      return;
    }
    if (!result) {
      if (!loading) tableHost.replaceChildren();
      return;
    }
    const sources = result.sources;
    const plan = columnPlan(sources.length, cols);
    const body = el('tbody');
    const pending = []; // accounts open whose lines aren't here yet
    for (const cat of result.categories) {
      const ckey = `c:${cat.code}`;
      const copen = m.open.has(ckey);
      body.append(
        row(sources, plan, {
          cls: 'cmp-cat',
          code: cat.code,
          name: cat.name,
          line: cat.line,
          toggle: {
            key: ckey,
            open: copen,
            label: copen ? `Hide the accounts in ${cat.code}` : `Show the accounts in ${cat.code}`,
            onClick: () => {
              if (copen) m.open.delete(ckey);
              else m.open.add(ckey);
              drawTable();
            },
          },
          place: (i, doc) => (cat.at[i] === null || cat.at[i] === undefined ? null : { doc, kind: 'category', ci: cat.at[i] }),
        }),
      );
      if (!copen) continue;
      for (const a of cat.accounts) {
        const akey = `a:${cat.code}|${a.code}|${a.fringe_account ? 'f' : ''}`;
        const aopen = !a.fringe_account && m.open.has(akey);
        body.append(
          row(sources, plan, {
            cls: a.fringe_account ? 'cmp-acct cmp-fringe-acct' : 'cmp-acct',
            code: a.code,
            name: a.fringe_account ? 'Fringes' : a.name,
            line: a.line,
            toggle: a.fringe_account
              ? null
              : {
                  key: akey,
                  open: aopen,
                  label: aopen ? `Hide the lines in ${a.code}` : `Compare the lines in ${a.code}`,
                  onClick: () => {
                    if (aopen) {
                      m.open.delete(akey);
                    } else {
                      m.open.add(akey);
                      const had = details.get(akey);
                      if (had && had.error) details.delete(akey);
                    }
                    drawTable();
                  },
                },
            place: (i, doc) => {
              const at = a.at[i];
              return at ? { doc, kind: 'account', ci: at[0], ai: at[1] } : null;
            },
          }),
        );
        if (aopen) {
          if (!details.has(akey)) {
            details.set(akey, { loading: true });
            pending.push([akey, a.code]);
          }
          for (const tr of detailRows(sources, plan, details.get(akey))) body.append(tr);
        }
      }
    }
    const summary = (cls, name, line, title) => row(sources, plan, { cls: `cmp-sum ${cls}`, code: '', name, line, title });
    result.levels.forEach((l, i) => body.append(summary(i === 0 ? 'cmp-level first' : 'cmp-level', l.name, l.line, 'A production total')));
    const t = result.totals;
    body.append(summary(result.levels.length ? '' : 'first', 'Total above-the-line', t.total_atl));
    body.append(summary('', 'Total below-the-line', t.total_btl));
    body.append(summary('cmp-info', 'Fringes (all)', t.fringes, 'Every fringe dollar, wherever it posts; already inside the totals above'));
    // Each charge and credit under its total, matched by name; one only some
    // budgets have says which.
    const items = (list, what) => (Array.isArray(list) ? list : []).forEach((x) => body.append(summary('cmp-item', x.name || `(unnamed ${what})`, x.line, `One ${what}, matched by name`)));
    body.append(summary('', 'Charges', t.charges, 'Contractual charges, added to the grand total'));
    items(result.charges, 'charge');
    body.append(summary('grand', 'Grand total', t.grand_total));
    body.append(summary('', 'Credits', t.credits, 'Taken off the grand total'));
    items(result.credits, 'credit');
    body.append(summary('grand', 'Net total', t.net_total));

    const table = el('table', { class: 'cmp-table' }, el('thead', {}, el('tr', {}, ...headCells(sources, plan))), body);
    const keep = { top: tableHost.scrollTop, left: tableHost.scrollLeft };
    const a = document.activeElement;
    const focusedKey = a && tableHost.contains(a) && a.dataset ? a.dataset.key : null;
    tableHost.replaceChildren(table);
    tableHost.scrollTop = keep.top;
    tableHost.scrollLeft = keep.left;
    if (focusedKey) {
      // A toggle redraws the table: keep the keyboard on the row it was on.
      const again = [...tableHost.querySelectorAll('.cmp-toggle')].find((b) => b.dataset.key === focusedKey);
      if (again) again.focus();
    }
    for (const [key, code] of pending) loadDetail(key, code);
  }

  // -------------------------------------------------------------------------
  // Printing: the table as shown, without its buttons, and a legend.

  /** The comparison as a plain table for the printed pages; null when there is none. */
  function printable() {
    const shown = tableHost.querySelector('table.cmp-table');
    if (!shown || !result) return null;
    const t = shown.cloneNode(true);
    t.className = 'cmp-print';
    for (const b of t.querySelectorAll('.cmp-toggle')) b.remove();
    for (const b of t.querySelectorAll('button')) b.replaceWith(document.createTextNode(b.textContent));
    for (const td of t.querySelectorAll('td.go')) td.classList.remove('go');
    return t;
  }

  function legend() {
    const sources = result ? result.sources : [];
    return el(
      'div',
      { class: 'cmp-legend' },
      ...sources.map((s, i) => {
        const what =
          s.kind === 'version' ? `version saved ${createdText(s.detail)}` : s.kind === 'file' ? s.detail : s.detail || (i === 0 ? 'not saved yet' : 'open, not saved yet');
        return el('div', {}, `${i === 0 ? 'This budget' : letter(i)}: ${s.label}${what ? ` (${what})` : ''}`);
      }),
    );
  }

  async function output(pdf) {
    const t = printable();
    if (!t) return;
    let report;
    try {
      report = await import('./report.js');
    } catch {
      report = null;
    }
    if (!report || typeof report.printHtml !== 'function') {
      status.textContent = 'Printing the comparison is not available in this version.';
      return;
    }
    const title = view && view.budget && view.budget.title ? view.budget.title : '';
    status.textContent = pdf ? 'Saving the PDF…' : 'Printing…';
    try {
      const r = await report.printHtml(t, {
        title,
        section: 'Budget comparison',
        file: view && view.file ? view.file.name : '',
        landscape: true,
        intro: legend(),
        pdf,
      });
      status.textContent = pdf && r && r.done ? (r.file ? `Saved ${r.file}.` : 'Saved.') : '';
    } catch (e) {
      status.textContent = String(e);
    }
  }

  const printBtn = el('button', { class: 'btn', text: 'Print…', attrs: { type: 'button' } });
  printBtn.addEventListener('click', () => output(false));
  const pdfBtn = el('button', { class: 'btn', text: 'Save as PDF…', attrs: { type: 'button' } });
  pdfBtn.addEventListener('click', () => output(true));

  // -------------------------------------------------------------------------
  // The column choices

  const toggles = el(
    'div',
    { class: 'cmp-cols', attrs: { role: 'group', 'aria-label': 'Columns' } },
    el('span', { class: 'cmp-cols-label', text: 'Columns:' }),
    ...COLUMN_CHOICES.map((c) => {
      const box = el('input', { attrs: { type: 'checkbox' } });
      box.checked = cols[c.key];
      box.addEventListener('change', () => {
        cols = { ...cols, [c.key]: box.checked };
        writePref('compareColumns', cols);
        drawTable();
      });
      return el('label', { class: 'cmp-col-choice', title: c.title }, box, c.label);
    }),
  );

  drawSide();
  drawTable();
  refresh();

  closeFn = customDialog((close) => {
    const done = el('button', { class: 'btn', text: 'Close', attrs: { type: 'button' } });
    done.addEventListener('click', close);
    const title = view && view.budget && view.budget.title ? view.budget.title : 'this budget';
    return el(
      'div',
      { class: 'modal compare-dialog', attrs: { role: 'dialog', 'aria-labelledby': 'compare-title' } },
      el(
        'header',
        { class: 'cmp-head' },
        el('h2', { class: 'modal-title', text: `Compare ${title}`, attrs: { id: 'compare-title' } }),
        status,
        el('span', { class: 'spacer' }),
        toggles,
        printBtn,
        pdfBtn,
        done,
      ),
      el('div', { class: 'cmp-body' }, sidebar, tableHost),
    );
  });
  // Keyboard users start on the list of budgets.
  const first = sidebar.querySelector('input:not(:disabled)');
  if (first) first.focus();
}
