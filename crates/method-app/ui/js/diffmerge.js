// File > What Changed… and File > Merge Changes From… (src/diffmerge.rs;
// decisions.md, entry 44).
//
// What Changed: at the left, what to compare the budget in front with (an
// open budget, one of its versions, its file as last saved, or a file picked
// in the Open dialog); at the right, every change in plain words, grouped
// under the budget's settings, each setup table, the topsheet and then
// account by account, with the money that moved. A click on a change goes to
// it in the budget (a row's cell, an account, a category, a setup tab).
//
// Merge Changes From: at the left, the returned copy (an open budget or a
// file) and the base (found from the copy's provenance when it was shared
// from a version of this budget, else a version, an open budget or a file
// picked); at the right, every conflict with Ours and Theirs to pick, then
// what the merge brings into this budget. Merge applies it as one undo step.
//
// The engine works everything out in Rust: money arrives as integer cents
// and every sentence is Rust's. Budget text goes in with textContent only;
// no inline styles (the CSP).

import { el } from './dom.js';
import { customDialog } from './modal.js';
import { money } from './format.js';
import { createdText, versionsOf } from './versions.js';

let uid = 0;
function nextId(prefix) {
  uid += 1;
  return `${prefix}-${uid}`;
}

/** A change of money: "+1,500.00", "(1,500.00)". */
export function deltaText(cents) {
  if (cents === null || cents === undefined || !Number.isFinite(cents)) return '';
  return cents > 0 ? `+${money(cents)}` : money(cents);
}

/** Same source? */
export function sameSource(a, b) {
  if (!a || !b || a.kind !== b.kind) return false;
  if (a.kind === 'tab') return a.doc === b.doc;
  if (a.kind === 'version') return a.id === b.id;
  if (a.kind === 'file') return a.token === b.token;
  return true;
}

/** Changes in order, grouped under their headings: [{ group, kind, items }]. */
export function groupChanges(changes) {
  const out = [];
  for (const c of Array.isArray(changes) ? changes : []) {
    const last = out[out.length - 1];
    if (last && last.group === c.group) last.items.push(c);
    else out.push({ group: c.group, kind: c.group_kind, items: [c] });
  }
  return out;
}

// What each budget was last compared with or merged from: doc id -> state.
// Kept for the session only.
const memory = new Map();

function remembered(doc) {
  if (!memory.has(doc)) memory.set(doc, { against: null, copy: null, base: { kind: 'auto' }, files: [] });
  return memory.get(doc);
}

/** A radio choice in the left column. */
function choice(name, src, current, label, sub, title, onPick) {
  const id = nextId('wc');
  const box = el('input', { attrs: { type: 'radio', name, id } });
  box.checked = sameSource(src, current);
  box.addEventListener('change', () => {
    if (box.checked) onPick(src);
  });
  return el(
    'label',
    { class: box.checked ? 'cmp-cand wc-cand is-on' : 'cmp-cand wc-cand', title: title || label, attrs: { for: id } },
    box,
    el('span', { class: 'cmp-cand-text' }, el('span', { class: 'cmp-cand-name', text: label }), sub ? el('span', { class: 'cmp-cand-sub', text: sub }) : null),
  );
}

function section(title, items, extra = null, group = null) {
  return el(
    'section',
    { class: 'cmp-side-section', attrs: group ? { role: 'radiogroup', 'aria-label': group } : {} },
    el('h3', { class: 'cmp-side-head', text: title }),
    items.length ? el('div', { class: 'cmp-side-list' }, ...items) : el('p', { class: 'cmp-side-empty', text: 'None' }),
    extra,
  );
}

/** Adds a picked file to the remembered list; its source. */
function keepPicked(m, p) {
  const src = p && p.source;
  if (!src) return null;
  if (src.kind === 'file' && !m.files.some((f) => f.token === src.token)) m.files.push({ token: src.token, label: p.label, path: p.path });
  return src;
}

/** The money table: label, before, after, change. */
function moneyTable(lines, caption, go) {
  if (!Array.isArray(lines) || !lines.length) return null;
  const body = el('tbody');
  for (const l of lines) {
    const name = l.go ? el('button', { class: 'wc-go', text: l.label, attrs: { type: 'button' } }) : document.createTextNode(l.label);
    if (l.go) name.addEventListener('click', () => go(l.go));
    body.append(
      el(
        'tr',
        {},
        el('th', { class: 'wc-money-name', attrs: { scope: 'row' } }, name),
        el('td', { class: 'num', text: l.before === null || l.before === undefined ? 'none' : money(l.before) }),
        el('td', { class: 'num', text: l.after === null || l.after === undefined ? 'none' : money(l.after) }),
        el('td', { class: l.delta < 0 ? 'num wc-delta is-down' : 'num wc-delta', text: deltaText(l.delta) }),
      ),
    );
  }
  return el(
    'table',
    { class: 'wc-money' },
    el('caption', { text: caption }),
    el(
      'thead',
      {},
      el(
        'tr',
        {},
        el('th', { attrs: { scope: 'col' } }),
        el('th', { class: 'num', text: 'Before', attrs: { scope: 'col' } }),
        el('th', { class: 'num', text: 'After', attrs: { scope: 'col' } }),
        el('th', { class: 'num', text: 'Change', attrs: { scope: 'col' } }),
      ),
    ),
    body,
  );
}

/** The changes, grouped under their headings; each one a button when it can be gone to. */
function changeList(changes, go) {
  const groups = groupChanges(changes);
  const host = el('div', { class: 'wc-list' });
  for (const g of groups) {
    const list = el('ul', { class: 'wc-items' });
    for (const c of g.items) {
      const text = c.go
        ? el('button', { class: 'wc-go wc-text', text: c.text, attrs: { type: 'button' } })
        : el('span', { class: 'wc-text', text: c.text });
      if (c.go) text.addEventListener('click', () => go(c.go));
      list.append(el('li', { class: `wc-item is-${c.kind}` }, el('span', { class: 'wc-kind', text: c.kind }), text));
    }
    host.append(el('section', { class: 'wc-group' }, el('h3', { class: 'wc-group-head', text: g.group }), list));
  }
  return host;
}

/**
 * File > What Changed…: what changed from another budget to the one in
 * front. `ctx`: { doc(), view(), tabs(), invoke(cmd, args), go(target) }.
 */
export function showWhatChanged(ctx) {
  const doc = ctx.doc();
  if (doc === null || doc === undefined) return;
  const view = ctx.view();
  const m = remembered(doc);
  const tabIds = new Set((ctx.tabs() || []).map((t) => t.id));
  const versionIds = new Set(versionsOf(view).map((v) => v.id));
  const hasFile = Boolean(view && view.file && view.file.path);
  const a = m.against;
  if (!a || (a.kind === 'tab' && (!tabIds.has(a.doc) || a.doc === doc)) || (a.kind === 'version' && !versionIds.has(a.id)) || (a.kind === 'saved' && !hasFile)) {
    m.against = hasFile ? { kind: 'saved' } : null;
  }

  let result = null;
  let problem = '';
  let seq = 0;
  let closeFn = null;
  const side = el('div', { class: 'cmp-side', attrs: { role: 'radiogroup', 'aria-label': 'Compare with' } });
  const host = el('div', { class: 'cmp-table-host wc-host', attrs: { tabindex: '-1' } });
  const status = el('p', { class: 'cmp-status', attrs: { role: 'status' } });

  function go(target) {
    if (closeFn) closeFn();
    ctx.go(target);
  }

  function pick(src) {
    m.against = src;
    drawSide();
    refresh();
  }

  async function addFile() {
    let p;
    try {
      p = await ctx.invoke('merge_pick_file', { purpose: 'diff' });
    } catch (e) {
      problem = String(e);
      draw();
      return;
    }
    const src = keepPicked(m, p);
    if (!src) return;
    if (src.kind === 'tab' && src.doc === doc) {
      problem = 'That file is this budget.';
      draw();
      return;
    }
    pick(src);
  }

  function drawSide() {
    const name = 'wc-against';
    const tabs = (ctx.tabs() || []).filter((t) => t.id !== doc);
    const addBtn = el('button', { class: 'btn cmp-add-files', text: 'Choose a File…', attrs: { type: 'button' } });
    addBtn.addEventListener('click', addFile);
    side.replaceChildren(
      el('p', { class: 'cmp-side-count', text: 'What changed in this budget since:' }),
      section('This budget', hasFile ? [choice(name, { kind: 'saved' }, m.against, 'Its file as last saved', view.file.dirty ? 'Unsaved changes since' : 'No unsaved changes', view.file.path, pick)] : []),
      section(
        'Versions of this budget',
        versionsOf(view).map((v) => choice(name, { kind: 'version', id: v.id }, m.against, v.name, createdText(v.created), v.note || v.name, pick)),
      ),
      section(
        'Open budgets',
        tabs.map((t) => choice(name, { kind: 'tab', doc: t.id }, m.against, t.label, t.dirty ? 'Unsaved changes' : '', t.path || t.title, pick)),
      ),
      section(
        'Files',
        m.files.map((f) => choice(name, { kind: 'file', token: f.token }, m.against, f.label, '', f.path, pick)),
        addBtn,
      ),
    );
  }

  async function refresh() {
    const mine = ++seq;
    problem = '';
    result = null;
    if (!m.against) {
      draw();
      return;
    }
    status.textContent = 'Comparing…';
    draw();
    try {
      const r = await ctx.invoke('what_changed', { doc, against: m.against });
      if (mine !== seq) return;
      result = r;
    } catch (e) {
      if (mine !== seq) return;
      problem = String(e);
    }
    status.textContent = '';
    draw();
  }

  function draw() {
    if (problem) {
      host.replaceChildren(el('p', { class: 'cmp-empty is-error', text: problem }));
      return;
    }
    if (!m.against) {
      host.replaceChildren(el('p', { class: 'cmp-empty', text: 'Choose what to compare this budget with.' }));
      return;
    }
    if (!result) {
      host.replaceChildren();
      return;
    }
    const n = result.changes.length;
    const head = el(
      'p',
      { class: 'wc-summary' },
      `${n === 0 ? 'No changes' : n === 1 ? '1 change' : `${n} changes`} from `,
      el('strong', { text: result.from.label }),
      ' to ',
      el('strong', { text: result.to.label }),
      '.',
    );
    const notes = [];
    if (!result.money_known) notes.push(el('p', { class: 'cmp-note', text: 'A budget can’t be totalled, so no money is shown.' }));
    if (result.from.partial || result.to.partial) notes.push(el('p', { class: 'cmp-note', text: 'Some cells have problems; their money counts as blank, as in the grid.' }));
    const parts = [
      head,
      ...notes,
      moneyTable(result.totals, 'Topsheet totals', go),
      n ? changeList(result.changes, go) : null,
      moneyTable(result.categories, 'Categories', go),
      moneyTable(result.accounts, 'Accounts', go),
    ];
    host.replaceChildren(...parts.filter(Boolean));
  }

  drawSide();
  draw();
  refresh();

  closeFn = customDialog((close) => {
    const done = el('button', { class: 'btn', text: 'Close', attrs: { type: 'button' } });
    done.addEventListener('click', close);
    const title = view && view.budget && view.budget.title ? view.budget.title : 'this budget';
    return el(
      'div',
      { class: 'modal compare-dialog wc-dialog', attrs: { role: 'dialog', 'aria-labelledby': 'wc-title' } },
      el('header', { class: 'cmp-head' }, el('h2', { class: 'modal-title', text: `What changed in ${title}`, attrs: { id: 'wc-title' } }), status, el('span', { class: 'spacer' }), done),
      el('div', { class: 'cmp-body' }, side, host),
    );
  });
  const first = side.querySelector('input:checked') || side.querySelector('input');
  if (first) first.focus();
}

/**
 * File > Merge Changes From…: merges a returned copy into the budget in
 * front. `ctx`: { doc(), view(), tabs(), invoke(cmd, args), apply(view),
 * toast(text), go(target) }.
 */
export function showMerge(ctx) {
  const doc = ctx.doc();
  if (doc === null || doc === undefined) return;
  const view = ctx.view();
  const m = remembered(doc);
  const tabIds = new Set((ctx.tabs() || []).map((t) => t.id));
  const versionIds = new Set(versionsOf(view).map((v) => v.id));
  const stale = (s) => !s || (s.kind === 'tab' && (!tabIds.has(s.doc) || s.doc === doc)) || (s.kind === 'version' && !versionIds.has(s.id));
  if (stale(m.copy)) m.copy = null;
  if (m.base.kind !== 'auto' && stale(m.base)) m.base = { kind: 'auto' };

  let preview = null;
  let problem = '';
  let busy = false;
  let seq = 0;
  let picks = {};
  let closeFn = null;
  const side = el('div', { class: 'cmp-side' });
  const host = el('div', { class: 'cmp-table-host wc-host', attrs: { tabindex: '-1' } });
  const status = el('p', { class: 'cmp-status', attrs: { role: 'status' } });
  const mergeBtn = el('button', { class: 'btn btn-primary', text: 'Merge', attrs: { type: 'button' } });

  async function addFile(purpose) {
    let p;
    try {
      p = await ctx.invoke('merge_pick_file', { purpose });
    } catch (e) {
      problem = String(e);
      draw();
      return;
    }
    const src = keepPicked(m, p);
    if (!src) return;
    if (src.kind === 'tab' && src.doc === doc) {
      problem = purpose === 'copy' ? 'That file is this budget.' : 'This budget can’t be the base.';
      draw();
      return;
    }
    if (purpose === 'copy') pickCopy(src);
    else pickBase(src);
  }

  function pickCopy(src) {
    m.copy = src;
    picks = {};
    drawSide();
    refresh();
  }

  function pickBase(src) {
    m.base = src;
    picks = {};
    drawSide();
    refresh();
  }

  function drawSide() {
    const tabs = (ctx.tabs() || []).filter((t) => t.id !== doc);
    const copyBtn = el('button', { class: 'btn cmp-add-files', text: 'Choose a File…', attrs: { type: 'button' } });
    copyBtn.addEventListener('click', () => addFile('copy'));
    const baseBtn = el('button', { class: 'btn cmp-add-files', text: 'Choose a File…', attrs: { type: 'button' } });
    baseBtn.addEventListener('click', () => addFile('base'));
    const files = (name, current, onPick) => m.files.map((f) => choice(name, { kind: 'file', token: f.token }, current, f.label, '', f.path, onPick));
    const tabChoices = (name, current, onPick) => tabs.map((t) => choice(name, { kind: 'tab', doc: t.id }, current, t.label, t.dirty ? 'Unsaved changes' : '', t.path || t.title, onPick));
    side.replaceChildren(
      section('Returned copy', [...tabChoices('mg-copy', m.copy, pickCopy), ...files('mg-copy', m.copy, pickCopy)], copyBtn, 'Returned copy'),
      section(
        'Base: where both started',
        [
          choice('mg-base', { kind: 'auto' }, m.base, 'From the copy', 'The version of this budget it was shared from', 'Found from where the copy says it came from', pickBase),
          ...versionsOf(view).map((v) => choice('mg-base', { kind: 'version', id: v.id }, m.base, v.name, createdText(v.created), v.note || v.name, pickBase)),
          ...tabChoices('mg-base', m.base, pickBase),
          ...files('mg-base', m.base, pickBase),
        ],
        baseBtn,
        'Base',
      ),
    );
  }

  async function refresh() {
    const mine = ++seq;
    problem = '';
    preview = null;
    if (!m.copy) {
      draw();
      return;
    }
    status.textContent = 'Merging…';
    draw();
    try {
      const r = await ctx.invoke('merge_preview', { doc, copy: m.copy, base: m.base, picks });
      if (mine !== seq) return;
      preview = r;
    } catch (e) {
      if (mine !== seq) return;
      problem = String(e);
    }
    status.textContent = '';
    draw();
  }

  function setPick(key, side) {
    picks = { ...picks, [key]: side };
    refresh();
  }

  function conflictRow(c) {
    const name = nextId('mg-pick');
    const opt = (sideKey, label, value) => {
      const id = nextId('mg-opt');
      const box = el('input', { attrs: { type: 'radio', name, id } });
      box.checked = c.taken === sideKey;
      box.addEventListener('change', () => {
        if (box.checked) setPick(c.key, sideKey);
      });
      return el('label', { class: 'mg-opt', attrs: { for: id } }, box, el('span', { class: 'mg-opt-label', text: label }), el('span', { class: 'mg-opt-value', text: value }));
    };
    return el(
      'li',
      { class: 'mg-conflict' },
      el('p', { class: 'mg-what', text: c.what }),
      c.base !== null && c.base !== undefined ? el('p', { class: 'mg-base', text: `Was: ${c.base}` }) : null,
      el('div', { class: 'mg-opts', attrs: { role: 'radiogroup', 'aria-label': c.what } }, opt('ours', 'Ours', c.ours), opt('theirs', 'Theirs', c.theirs)),
    );
  }

  function draw() {
    mergeBtn.disabled = busy || !preview || !preview.base;
    if (problem) {
      host.replaceChildren(el('p', { class: 'cmp-empty is-error', text: problem }));
      return;
    }
    if (!m.copy) {
      host.replaceChildren(el('p', { class: 'cmp-empty', text: 'Choose the returned copy to merge from.' }));
      return;
    }
    if (!preview) {
      host.replaceChildren();
      return;
    }
    if (!preview.base) {
      host.replaceChildren(el('p', { class: 'cmp-empty', text: preview.needs_base || 'Choose the base: the budget both started from.' }));
      return;
    }
    const parts = [
      el(
        'p',
        { class: 'wc-summary' },
        'Merging ',
        el('strong', { text: preview.copy.label }),
        ' against ',
        el('strong', { text: preview.base.label }),
        '.',
      ),
    ];
    if (preview.problem) parts.push(el('p', { class: 'cmp-note is-error', text: `The merged budget has a problem: ${preview.problem}` }));
    if (preview.renumbered) parts.push(el('p', { class: 'cmp-note', text: `${preview.renumbered === 1 ? 'A row or version' : `${preview.renumbered} rows or versions`} added on both sides with the same id: the copy’s got a new id.` }));
    const nc = preview.conflicts.length;
    if (nc) {
      const groups = groupChanges(preview.conflicts.map((c) => ({ ...c, group_kind: '' })));
      const list = el('div', { class: 'wc-list' });
      for (const g of groups) list.append(el('section', { class: 'wc-group' }, el('h3', { class: 'wc-group-head', text: g.group }), el('ul', { class: 'mg-conflicts' }, ...g.items.map(conflictRow))));
      parts.push(el('h3', { class: 'mg-head', text: nc === 1 ? '1 conflict: pick a side' : `${nc} conflicts: pick a side for each` }), list);
    } else {
      parts.push(el('p', { class: 'cmp-note', text: 'No conflicts.' }));
    }
    const n = preview.changes.length;
    parts.push(el('h3', { class: 'mg-head', text: n === 0 ? 'The merge changes nothing in this budget.' : n === 1 ? 'What the merge brings in: 1 change' : `What the merge brings in: ${n} changes` }));
    parts.push(moneyTable(preview.totals, 'Topsheet totals', null));
    if (n) parts.push(changeList(preview.changes.map((c) => ({ ...c, go: null })), () => {}));
    host.replaceChildren(...parts.filter(Boolean));
  }

  async function doMerge() {
    if (busy || !preview || !preview.base) return;
    busy = true;
    draw();
    try {
      const v = await ctx.invoke('merge_apply', { doc, copy: m.copy, base: m.base, picks });
      const label = preview.copy.label;
      const nc = preview.conflicts.length;
      if (closeFn) closeFn();
      ctx.apply(v);
      ctx.toast(nc ? `Merged changes from ${label}; ${nc === 1 ? '1 conflict' : `${nc} conflicts`} settled as picked.` : `Merged changes from ${label}.`);
    } catch (e) {
      busy = false;
      problem = String(e);
      draw();
    }
  }
  mergeBtn.addEventListener('click', doMerge);

  drawSide();
  draw();
  refresh();

  closeFn = customDialog((close) => {
    const cancel = el('button', { class: 'btn', text: 'Cancel', attrs: { type: 'button' } });
    cancel.addEventListener('click', close);
    const title = view && view.budget && view.budget.title ? view.budget.title : 'this budget';
    return el(
      'div',
      { class: 'modal compare-dialog wc-dialog', attrs: { role: 'dialog', 'aria-labelledby': 'mg-title' } },
      el('header', { class: 'cmp-head' }, el('h2', { class: 'modal-title', text: `Merge changes into ${title}`, attrs: { id: 'mg-title' } }), status, el('span', { class: 'spacer' }), cancel, mergeBtn),
      el('div', { class: 'cmp-body' }, side, host),
    );
  });
  const first = side.querySelector('input:checked') || side.querySelector('input');
  if (first) first.focus();
}
