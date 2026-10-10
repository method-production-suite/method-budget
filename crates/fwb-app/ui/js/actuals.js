// The tools panel's Actuals tab (roadmap 3.7; src/actuals.rs): the
// transactions brought in with File > Import Actuals…, and each account's
// estimate to complete.
//
// - **Estimate to complete** for the account at the grid's cursor (on the
//   account list, or the account on screen): its budget, actual, ETC, EAC
//   and variance from the view's cost figures, and the method: budget minus
//   actual (the default), a manual amount or percent complete. Set is one
//   undo step.
// - **Transactions**, filtered by account (or the unassigned ones), vendor
//   and dates, by date: the first 500 matches with the count and total of
//   all of them (actuals_list, fetched again whenever the budget changes).
//   Tick transactions to Edit… (one: every field, and the line it pays for),
//   Move to… another account (or unassigned), or Delete; each is one undo
//   step. A double-click edits one.
// - **Imports**: each file imported, with how many of its transactions are
//   left; Remove takes the whole import back out (one undo step).
//
// tools.js calls renderActuals(host, view, ctx) on every render; ctx.call
// runs a command and redraws from its view. Text from a budget goes in
// with textContent and input values only.

import { el } from './dom.js';
import { invoke } from './api.js';
import { money } from './format.js';
import { customDialog, confirmDialog } from './modal.js';
import { plural } from './util.js';

const METHODS = [
  { id: 'budget_minus_actual', label: 'Budget minus actual' },
  { id: 'manual', label: 'Manual amount' },
  { id: 'percent_complete', label: 'Percent complete' },
];

// What the tab remembers between renders (the panel is redrawn on every view).
const st = {
  host: null,
  view: null,
  ctx: null,
  doc: null,
  rev: null,
  filter: { account: '', unassigned: false, vendor: '', text: '', from: '', to: '', batch: null },
  list: null, // the last actuals_list reply
  error: '',
  selected: new Set(),
  fetching: false,
  again: false,
  timer: null,
  etcDraft: null, // { code, method, value } while typing
};

/** Draws the Actuals tab into `host`. */
export function renderActuals(host, view, ctx) {
  st.host = host;
  st.view = view;
  st.ctx = ctx;
  const doc = view && typeof view.doc === 'number' ? view.doc : null;
  const fresh = doc !== st.doc || view.rev !== st.rev;
  if (doc !== st.doc) {
    st.list = null;
    st.selected.clear();
    st.error = '';
    st.etcDraft = null;
  }
  st.doc = doc;
  st.rev = view ? view.rev : null;
  draw();
  if (fresh) fetchList();
}

function filterArgs() {
  const f = st.filter;
  return {
    account: f.unassigned ? '' : f.account,
    unassigned: f.unassigned,
    vendor: f.vendor,
    text: f.text,
    from: f.from.trim(),
    to: f.to.trim(),
    batch: f.batch,
  };
}

async function fetchList() {
  if (st.fetching) {
    st.again = true;
    return;
  }
  st.fetching = true;
  try {
    do {
      st.again = false;
      const doc = st.doc;
      try {
        const args = { filter: filterArgs() };
        if (doc !== null) args.doc = doc;
        const r = await invoke('actuals_list', args);
        if (doc !== st.doc) continue;
        st.list = r;
        st.error = '';
        // Ticks on transactions no longer listed go.
        const ids = new Set((r.transactions || []).map((t) => t.id));
        for (const id of [...st.selected]) if (!ids.has(id)) st.selected.delete(id);
      } catch (e) {
        if (doc !== st.doc) continue;
        st.error = typeof e === 'string' ? e : String(e);
      }
    } while (st.again);
  } finally {
    st.fetching = false;
  }
  if (st.host && st.host.isConnected) draw();
}

function refetchSoon() {
  clearTimeout(st.timer);
  st.timer = setTimeout(fetchList, 180);
}

// ---------------------------------------------------------------------------
// The account at the grid's cursor, and its cost figures

/** { code, name, figures, method } for the account the grid points at, or null. */
function cursorAccount() {
  const v = st.view;
  const ctx = st.ctx;
  if (!v || !v.cost || !ctx || typeof ctx.selection !== 'function') return null;
  const sel = ctx.selection() || {};
  const topsheet = v.budget && Array.isArray(v.budget.topsheet) ? v.budget.topsheet : [];
  let ti = null;
  let ai = null;
  if (sel.level === 'details' && sel.account) {
    ti = sel.account.ti;
    ai = sel.account.ai;
  } else if (sel.level === 'accounts' && sel.cat && typeof sel.cursor === 'string' && /^a\d+$/.test(sel.cursor)) {
    ti = sel.cat.ti;
    ai = Number(sel.cursor.slice(1));
  }
  if (ti === null || ai === null || !topsheet[ti] || topsheet[ti].kind === 'production_total') return null;
  const ci = topsheet.slice(0, ti).filter((r) => r.kind !== 'production_total').length;
  const cat = v.cost.categories[ci];
  const acct = (topsheet[ti].accounts || [])[ai];
  const entry = cat && cat.accounts[ai];
  if (!acct || !entry) return null;
  return { code: acct.code, name: acct.name || '', figures: entry.figures, method: entry.method };
}

// ---------------------------------------------------------------------------
// Drawing

function draw() {
  const host = st.host;
  if (!host) return;
  const active = document.activeElement;
  const focusKey = host.contains(active) && active.dataset ? active.dataset.key : null;
  const box = el('div', { class: 'actuals' });
  const cost = st.view && st.view.cost;
  if (!cost) {
    box.append(
      el('p', {
        class: 'actuals-empty',
        text: 'No actuals yet. File > Import Actuals… brings in transactions from your accounting system’s CSV or Excel export, and the cost report columns (Actual, ETC, EAC) appear in the grid.',
      }),
    );
    host.replaceChildren(box);
    return;
  }
  box.append(summary(cost));
  if (cost.error) box.append(el('p', { class: 'actuals-error', text: cost.error, attrs: { role: 'alert' } }));
  const acct = cursorAccount();
  box.append(acct ? etcBlock(acct) : el('p', { class: 'actuals-hint', text: 'Put the cursor on an account to see and set its estimate to complete.' }));
  box.append(filters());
  if (st.error) box.append(el('p', { class: 'actuals-error', text: st.error, attrs: { role: 'alert' } }));
  box.append(transactions());
  box.append(imports());
  host.replaceChildren(box);
  if (focusKey) {
    const again = [...host.querySelectorAll('[data-key]')].find((e) => e.dataset.key === focusKey);
    if (again) again.focus({ preventScroll: true });
  }
}

function summary(cost) {
  const t = cost.total;
  const parts = [`${plural(cost.transactions, 'transaction', 'transactions')}, ${money(t.actual)} actual to date.`];
  if (cost.unassigned.count) parts.push(`${cost.unassigned.count} unassigned (${money(cost.unassigned.actual)}).`);
  const p = el('p', { class: 'actuals-summary' }, el('span', { text: parts.join(' ') }));
  const line = el(
    'p',
    { class: 'actuals-totals' },
    el('span', { text: `Estimate at completion ${money(t.eac)}` }),
    el('span', { class: t.over ? 'actuals-over' : '', text: ` ${t.variance < 0 ? 'over' : 'under'} budget by ${money(Math.abs(t.variance))}` }),
  );
  return el('div', { class: 'actuals-head' }, p, line);
}

function figure(label, cents, cls = '') {
  return el('div', { class: `actuals-fig ${cls}` }, el('span', { class: 'actuals-fig-label', text: label }), el('span', { class: 'actuals-fig-value', text: money(cents) }));
}

function etcBlock(acct) {
  const f = acct.figures;
  const draft = st.etcDraft && st.etcDraft.code === acct.code ? st.etcDraft : null;
  const method = draft ? draft.method : acct.method ? acct.method.kind : 'budget_minus_actual';
  const current = acct.method ? (acct.method.kind === 'manual' ? money(acct.method.amount) : acct.method.percent || '') : '';
  const value = draft ? draft.value : current;
  const select = el('select', { class: 'setup-input setup-select', attrs: { 'aria-label': `Estimate to complete method for ${acct.code}` }, data: { key: 'actuals:etc-method' } });
  for (const m of METHODS) select.append(el('option', { text: m.label, attrs: { value: m.id } }));
  select.value = method;
  const input = el('input', {
    class: 'setup-input setup-num',
    attrs: {
      type: 'text',
      'aria-label': method === 'percent_complete' ? 'Percent complete' : 'Estimate to complete',
      placeholder: method === 'percent_complete' ? 'e.g. 40' : method === 'manual' ? 'Amount' : '',
      autocomplete: 'off',
      spellcheck: 'false',
    },
    data: { key: 'actuals:etc-value' },
  });
  input.value = value;
  input.disabled = method === 'budget_minus_actual';
  const set = el('button', { class: 'setup-btn', text: 'Set', attrs: { type: 'button' }, data: { key: 'actuals:etc-set' } });
  const remember = () => {
    st.etcDraft = { code: acct.code, method: select.value, value: input.value };
  };
  select.addEventListener('change', () => {
    remember();
    draw();
  });
  input.addEventListener('input', remember);
  const apply = async () => {
    try {
      await st.ctx.call('actuals_set_etc', { account: acct.code, method: select.value, value: input.value });
      st.etcDraft = null;
    } catch (e) {
      if (st.ctx.toast) st.ctx.toast(e);
    }
  };
  set.addEventListener('click', apply);
  input.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      apply();
    }
  });
  const pct = f.percent === null || f.percent === undefined ? '' : `${f.percent}% spent`;
  return el(
    'section',
    { class: 'actuals-etc', attrs: { 'aria-label': `Estimate to complete for ${acct.code}` } },
    el('h3', { class: 'actuals-heading', text: `${acct.code} ${acct.name}`.trim() }),
    el(
      'div',
      { class: 'actuals-figs' },
      figure('Budget', f.budget),
      figure('Actual', f.actual),
      figure('ETC', f.etc),
      figure('EAC', f.eac),
      figure('Variance', f.variance, f.over ? 'actuals-over' : ''),
    ),
    pct ? el('p', { class: 'actuals-hint', text: `${pct}, ${plural(f.count, 'transaction', 'transactions')}` }) : null,
    el('div', { class: 'find-row' }, select, input, set),
  );
}

function filters() {
  const f = st.filter;
  const chart = st.list && Array.isArray(st.list.chart) ? st.list.chart : [];
  const account = el('select', { class: 'setup-input setup-select', attrs: { 'aria-label': 'Account' }, data: { key: 'actuals:f-account' } });
  account.append(el('option', { text: 'All accounts', attrs: { value: '' } }));
  account.append(el('option', { text: 'Unassigned', attrs: { value: ':unassigned' } }));
  for (const c of chart) account.append(el('option', { text: `${c.code} ${c.name}`.trim(), attrs: { value: c.code } }));
  account.value = f.unassigned ? ':unassigned' : f.account;
  if (account.value !== (f.unassigned ? ':unassigned' : f.account)) account.value = '';
  account.addEventListener('change', () => {
    f.unassigned = account.value === ':unassigned';
    f.account = f.unassigned ? '' : account.value;
    st.selected.clear();
    fetchList();
  });
  const text = (key, label, value, set, placeholder = '') => {
    const input = el('input', { class: 'setup-input', attrs: { type: 'text', 'aria-label': label, placeholder, autocomplete: 'off', spellcheck: 'false' }, data: { key } });
    input.value = value;
    input.addEventListener('input', () => {
      set(input.value);
      refetchSoon();
    });
    return input;
  };
  const vendor = text('actuals:f-vendor', 'Vendor', f.vendor, (v) => (f.vendor = v), 'Vendor');
  const search = text('actuals:f-text', 'Search vendor, description, reference and tags', f.text, (v) => (f.text = v), 'Search');
  const from = text('actuals:f-from', 'From date (YYYY-MM-DD)', f.from, (v) => (f.from = v), 'From YYYY-MM-DD');
  const to = text('actuals:f-to', 'To date (YYYY-MM-DD)', f.to, (v) => (f.to = v), 'To YYYY-MM-DD');
  return el(
    'div',
    { class: 'find-form actuals-filters', attrs: { role: 'search', 'aria-label': 'Filter transactions' } },
    el('div', { class: 'find-row' }, account),
    el('div', { class: 'find-row' }, vendor, search),
    el('div', { class: 'find-row' }, from, to),
  );
}

function transactions() {
  const l = st.list;
  const wrap = el('section', { class: 'actuals-list', attrs: { 'aria-label': 'Transactions' } });
  if (!l) {
    wrap.append(el('p', { class: 'actuals-hint', text: 'Loading the transactions…' }));
    return wrap;
  }
  const shown = l.transactions.length;
  const head = `${plural(l.count, 'transaction', 'transactions')}, ${money(l.total)}${l.count > shown ? ` (the first ${shown} shown)` : ''}`;
  wrap.append(el('p', { class: 'find-count', text: head }));
  const list = el('ul', { class: 'find-hits' });
  for (const t of l.transactions) list.append(row(t));
  if (!shown) list.append(el('li', { class: 'actuals-hint', text: 'No transactions match.' }));
  wrap.append(list);
  const n = st.selected.size;
  const btn = (key, text, enabled, fn) => {
    const b = el('button', { class: 'setup-btn', text, attrs: { type: 'button' }, data: { key } });
    b.disabled = !enabled;
    b.addEventListener('click', fn);
    return b;
  };
  wrap.append(
    el(
      'div',
      { class: 'find-row actuals-actions' },
      btn('actuals:edit', 'Edit…', n === 1, () => editOne([...st.selected][0])),
      btn('actuals:move', n > 1 ? `Move ${n} to…` : 'Move to…', n > 0, () => moveSelected()),
      btn('actuals:delete', n > 1 ? `Delete ${n}` : 'Delete', n > 0, () => deleteSelected()),
    ),
  );
  return wrap;
}

function row(t) {
  const box = el('input', { attrs: { type: 'checkbox', 'aria-label': `Select ${t.date || 'undated'} ${t.vendor} ${money(t.amount)}` }, data: { key: `actuals:pick:${t.id}` } });
  box.checked = st.selected.has(t.id);
  box.addEventListener('change', () => {
    if (box.checked) st.selected.add(t.id);
    else st.selected.delete(t.id);
    draw();
  });
  const where = [t.date || 'No date', t.account ? `${t.account}${t.assigned ? '' : ' (unassigned)'}` : 'Unassigned', t.vendor].filter(Boolean).join(' · ');
  const desc = [t.description, t.reference].filter(Boolean).join(' · ');
  const entry = el(
    'button',
    {
      class: 'find-entry',
      title: [t.account_name, t.line_label, t.tags && t.tags.length ? `Tags: ${t.tags.join(', ')}` : ''].filter(Boolean).join('\n') || 'Double-click to edit',
      attrs: { type: 'button' },
      data: { key: `actuals:row:${t.id}` },
    },
    el('span', { class: 'find-where actuals-where' }, el('span', { text: where }), el('span', { class: 'actuals-amount', text: money(t.amount) })),
    el('span', { class: 'find-desc', text: desc || ' ' }),
  );
  entry.addEventListener('click', () => {
    if (st.selected.has(t.id)) st.selected.delete(t.id);
    else st.selected.add(t.id);
    draw();
  });
  entry.addEventListener('dblclick', () => editOne(t.id));
  return el('li', { class: 'find-hit' }, box, entry);
}

function imports() {
  const l = st.list;
  const batches = l && Array.isArray(l.batches) ? l.batches : [];
  if (!batches.length) return el('div');
  const list = el('ul', { class: 'find-hits actuals-imports' });
  for (const b of batches) {
    const when = b.imported ? b.imported.slice(0, 16).replace('T', ' ') : '';
    const remove = el('button', { class: 'setup-btn', text: 'Remove', attrs: { type: 'button', 'aria-label': `Remove the import of ${b.file || 'a file'}` }, data: { key: `actuals:batch:${b.id}` } });
    remove.addEventListener('click', async () => {
      const ok = await confirmDialog({
        title: 'Remove this import?',
        message: `Takes out the ${plural(b.remaining, 'transaction', 'transactions')} imported from ${b.file || 'this file'}${when ? ` on ${when}` : ''}, edited ones included. Undo brings them back.`,
        confirmText: 'Remove Import',
        danger: true,
      });
      if (!ok) return;
      try {
        await st.ctx.call('actuals_remove_batch', { batch: b.id });
      } catch (e) {
        if (st.ctx.toast) st.ctx.toast(e);
      }
    });
    list.append(
      el(
        'li',
        { class: 'find-hit actuals-import' },
        el('span', { class: 'find-entry actuals-import-text' }, el('span', { class: 'find-desc', text: b.file || 'Imported file' }), el('span', { class: 'find-where', text: `${when}${when ? ' · ' : ''}${plural(b.remaining, 'transaction', 'transactions')} of ${b.rows}` })),
        remove,
      ),
    );
  }
  return el('details', { class: 'actuals-batches' }, el('summary', { text: `Imports (${batches.length})` }), list);
}

// ---------------------------------------------------------------------------
// Editing

function transaction(id) {
  return st.list ? st.list.transactions.find((t) => t.id === id) : null;
}

function accountSelect(value, label) {
  const chart = st.list && Array.isArray(st.list.chart) ? st.list.chart : [];
  const s = el('select', { class: 'modal-input', attrs: { 'aria-label': label } });
  s.append(el('option', { text: 'Unassigned', attrs: { value: '' } }));
  for (const c of chart) s.append(el('option', { text: `${c.code} ${c.name}`.trim(), attrs: { value: c.code } }));
  s.value = value;
  // A number the chart doesn't have stays as it is unless another is picked.
  if (s.value !== value) {
    s.prepend(el('option', { text: `${value} (not in this budget)`, attrs: { value: `:keep` } }));
    s.value = ':keep';
  }
  return s;
}

function field(label, input) {
  return el('label', { class: 'modal-field' }, el('span', { text: label }), input);
}

function textInput(value, extra = {}) {
  const i = el('input', { class: 'modal-input', attrs: { type: 'text', autocomplete: 'off', spellcheck: 'false', ...extra } });
  i.value = value || '';
  return i;
}

/** Every field of one transaction, and the line it pays for. */
function editOne(id) {
  const t = transaction(id);
  if (!t) return;
  customDialog((close) => {
    const date = textInput(t.date, { placeholder: 'YYYY-MM-DD' });
    const account = accountSelect(t.account, 'Account');
    const line = el('select', { class: 'modal-input', attrs: { 'aria-label': 'Line it pays for' } });
    const vendor = textInput(t.vendor);
    const description = textInput(t.description);
    const reference = textInput(t.reference);
    const tags = textInput((t.tags || []).join(', '), { placeholder: 'Separated by commas' });
    const amount = textInput(money(t.amount));
    const error = el('p', { class: 'modal-error', attrs: { role: 'alert' } });
    error.hidden = true;
    const fillLines = async (code, keep) => {
      line.replaceChildren(el('option', { text: 'No particular line', attrs: { value: '' } }));
      if (!code || code === ':keep') return;
      try {
        const args = { account: code };
        if (st.doc !== null) args.doc = st.doc;
        const lines = await invoke('actuals_lines', args);
        for (const l of lines) line.append(el('option', { text: `Line ${l.n}${l.description ? `: ${l.description}` : ''}`, attrs: { value: String(l.id) } }));
        if (keep !== null && keep !== undefined) line.value = String(keep);
        if (line.value === '' && keep) line.value = '';
      } catch {
        // No lines to offer: the transaction keeps none.
      }
    };
    fillLines(t.account, t.line);
    account.addEventListener('change', () => fillLines(account.value, null));
    const cancel = el('button', { class: 'btn', text: 'Cancel', attrs: { type: 'button' } });
    const ok = el('button', { class: 'btn btn-primary', text: 'Save', attrs: { type: 'button' } });
    cancel.addEventListener('click', () => close());
    ok.addEventListener('click', async () => {
      ok.disabled = true;
      error.hidden = true;
      const edit = {
        date: date.value,
        vendor: vendor.value,
        description: description.value,
        reference: reference.value,
        tags: tags.value,
        amount: amount.value,
      };
      if (account.value !== ':keep') edit.account = account.value;
      if (line.value === '') edit.clear_line = true;
      else edit.line = Number(line.value);
      try {
        await st.ctx.call('actuals_update', { ids: [t.id], edit });
        close();
      } catch (e) {
        error.textContent = String(e);
        error.hidden = false;
        ok.disabled = false;
      }
    });
    return el(
      'div',
      { class: 'modal actuals-dialog', attrs: { role: 'dialog', 'aria-modal': 'true', 'aria-label': 'Edit transaction' } },
      el('h2', { class: 'modal-title', text: 'Edit transaction' }),
      field('Date', date),
      field('Account', account),
      field('Line', line),
      field('Vendor', vendor),
      field('Description', description),
      field('Reference', reference),
      field('Tags', tags),
      field('Amount', amount),
      error,
      el('div', { class: 'modal-actions' }, cancel, ok),
    );
  });
}

/** The ticked transactions to another account, or unassigned. */
function moveSelected() {
  const ids = [...st.selected];
  if (!ids.length) return;
  customDialog((close) => {
    const account = accountSelect('', 'Move to account');
    const error = el('p', { class: 'modal-error', attrs: { role: 'alert' } });
    error.hidden = true;
    const cancel = el('button', { class: 'btn', text: 'Cancel', attrs: { type: 'button' } });
    const ok = el('button', { class: 'btn btn-primary', text: 'Move', attrs: { type: 'button' } });
    cancel.addEventListener('click', () => close());
    ok.addEventListener('click', async () => {
      ok.disabled = true;
      try {
        await st.ctx.call('actuals_update', { ids, edit: { account: account.value } });
        st.selected.clear();
        close();
      } catch (e) {
        error.textContent = String(e);
        error.hidden = false;
        ok.disabled = false;
      }
    });
    return el(
      'div',
      { class: 'modal actuals-dialog', attrs: { role: 'dialog', 'aria-modal': 'true', 'aria-label': 'Move transactions' } },
      el('h2', { class: 'modal-title', text: `Move ${plural(ids.length, 'transaction', 'transactions')}` }),
      el('p', { class: 'modal-message', text: 'Moving to another account takes off a line they named in this one.' }),
      field('To account', account),
      error,
      el('div', { class: 'modal-actions' }, cancel, ok),
    );
  });
}

async function deleteSelected() {
  const ids = [...st.selected];
  if (!ids.length) return;
  const ok = await confirmDialog({
    title: `Delete ${plural(ids.length, 'transaction', 'transactions')}?`,
    message: 'They leave the actuals and the cost report. Undo brings them back.',
    confirmText: 'Delete',
    danger: true,
  });
  if (!ok) return;
  try {
    await st.ctx.call('actuals_delete', { ids });
    st.selected.clear();
  } catch (e) {
    if (st.ctx.toast) st.ctx.toast(e);
  }
}
