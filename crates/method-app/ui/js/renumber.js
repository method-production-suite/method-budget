// Renumbering accounts when the account-number pattern changes (Settings >
// Account numbers; src/renumber.rs; decisions.md, entry 39).
//
// Applying a new pattern first asks Rust what every category, account and
// category fringe account number would become. When nothing would change,
// the pattern is applied as before. Otherwise a preview opens under the
// form: old and new numbers, anything that stops it (a number that doesn't
// fit, two numbers that would become one), then "Renumber accounts to the
// new pattern" (one undo step: the pattern and every number, with the
// charges, credits, report presets and original totals that name them),
// "Change the pattern only" (numbers keep their spelling), or Cancel.
//
// The preview survives the panel being drawn again (every view redraws it):
// it is kept here, per budget, until it is answered. Built with the setup
// kit's el() (textContent only).

import { invoke } from './api.js';

// { doc, q, plan } while a preview is open.
let pending = null;

/** The open preview for budget `doc`, if any: { q, plan }. */
export function pendingRenumber(doc) {
  return pending && pending.doc === doc ? pending : null;
}

function args(q) {
  return { ...q, separator: q.separator === '' ? null : q.separator };
}

/**
 * Applies pattern `q` from the form: straight away when no number would
 * change, else opens the preview in `form`. `k` is the setup kit, `ck` the
 * form's item key; `onProblem(text)` shows a message on the form.
 */
export async function applyCoding(k, ck, q, form, { onProblem, onCancel }) {
  const doc = k.view() && k.view().doc;
  let plan = null;
  try {
    plan = await invoke('renumber_preview', args(q));
  } catch (e) {
    onProblem(String(e));
    return;
  }
  if (!plan || (!plan.changes && plan.conflicts.length === 0)) {
    pending = null;
    k.setFocusKey(k.key('settings', 'coding', 'category_width'));
    k.run(ck, () => ['set_account_coding', args(q)], { onError: () => k.setFocusKey(null) });
    return;
  }
  pending = { doc, q, plan };
  const old = form.querySelector(':scope > .rn-box');
  if (old) old.remove();
  form.append(previewBox(k, ck, pending, onCancel));
  const first = form.querySelector('.rn-box button');
  if (first) first.focus();
}

function row(k, cls, cells) {
  const tr = k.el('tr', cls);
  cells.forEach((c, i) => tr.append(k.el(i === 0 ? 'th' : 'td', null, c)));
  if (tr.firstChild) tr.firstChild.scope = 'row';
  return tr;
}

function section(k, tbody, title, changes) {
  if (!Array.isArray(changes) || changes.length === 0) return;
  const head = k.el('tr', 'rn-kind');
  const th = k.el('th', null, title);
  th.colSpan = 2;
  th.scope = 'colgroup';
  head.append(th);
  tbody.append(head);
  for (const c of changes) {
    const bad = c.new === '';
    const cls = bad ? 'rn-bad' : c.old === c.new ? 'rn-same' : null;
    tbody.append(row(k, cls, [c.old, bad ? 'Doesn’t fit' : c.new]));
  }
}

/** The preview: the table, problems and the three choices. */
export function previewBox(k, ck, p, onCancel) {
  const box = k.el('div', 'rn-box');
  box.setAttribute('role', 'group');
  box.setAttribute('aria-label', 'Renumber accounts');
  const { plan, q } = p;
  const moved = [...plan.categories, ...plan.accounts].filter((c) => c.old !== c.new).length;
  box.append(
    k.el(
      'p',
      'setup-hint',
      plan.conflicts.length
        ? 'Some numbers can’t be carried over to this pattern. Fix them first, or choose another pattern, or change the pattern only.'
        : `${k.plural(moved, 'number changes', 'numbers change')} with the new pattern. Renumbering also updates the charges, credits, report setups and original totals that name them.`,
    ),
  );
  const table = k.el('table', 'rn-table');
  const thead = k.el('thead');
  const hr = k.el('tr');
  for (const h of ['Now', 'Becomes']) {
    const th = k.el('th', null, h);
    th.scope = 'col';
    hr.append(th);
  }
  thead.append(hr);
  const tbody = k.el('tbody');
  section(k, tbody, 'Categories', plan.categories);
  section(k, tbody, 'Accounts', plan.accounts);
  section(k, tbody, 'Fringe accounts', plan.fringe_accounts);
  table.append(thead, tbody);
  const host = k.el('div', 'rn-table-host');
  host.append(table);
  box.append(host);
  const list = (items, cls) => {
    const ul = k.el('ul', `rn-list ${cls}`);
    for (const t of items) ul.append(k.el('li', null, t));
    return ul;
  };
  if (plan.conflicts.length) box.append(list(plan.conflicts, 'rn-conflicts'));
  if (plan.warnings.length) box.append(list(plan.warnings, 'setup-hint'));

  const actions = k.el('div', 'setup-confirm-actions');
  const renumber = k.el('button', 'setup-btn setup-btn-primary', 'Renumber accounts to the new pattern');
  renumber.type = 'button';
  renumber.disabled = plan.conflicts.length > 0;
  renumber.addEventListener('click', () => {
    if (k.isBusy() || renumber.disabled) return;
    pending = null;
    k.setFocusKey(k.key('settings', 'coding', 'category_width'));
    k.run(ck, () => ['renumber_accounts', args(q)], { onError: () => k.setFocusKey(null) });
  });
  const only = k.el('button', 'setup-btn', 'Change the pattern only');
  only.type = 'button';
  only.addEventListener('click', () => {
    if (k.isBusy()) return;
    pending = null;
    k.setFocusKey(k.key('settings', 'coding', 'category_width'));
    k.run(ck, () => ['set_account_coding', args(q)], { onError: () => k.setFocusKey(null) });
  });
  const cancel = k.el('button', 'setup-btn setup-btn-quiet', 'Cancel');
  cancel.type = 'button';
  cancel.addEventListener('click', () => {
    pending = null;
    box.remove();
    if (onCancel) onCancel();
  });
  box.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      cancel.click();
    }
  });
  actions.append(renumber, only, cancel);
  box.append(actions);
  return box;
}
