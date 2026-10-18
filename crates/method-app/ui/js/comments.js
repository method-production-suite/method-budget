// The tools panel's Comments tab (roadmap 3.3; src/review.rs): a review
// file's comments on the budget, each shown at its place.
//
// - Reviewing (File > Open for Review…): the budget is read-only. A box adds
//   a comment on the cursor's line, account or category, or on the whole
//   budget; each comment can be edited, resolved (ticked off) or deleted.
//   The desktop writes the review file beside the budget after every
//   change; the browser offers it as a download.
// - Reading (File > Open Review Comments…): the author's budget. Each
//   comment can be merged in as a dated note on its place or dismissed;
//   Merge All merges every one that can be, as one undo step. A comment on
//   a setup item or the whole budget, or whose place is gone, says why it
//   can't become a note, and stays listed.
//
// The list comes from review_list, fetched again when the budget or the
// comments change (view.rev, view.review.rev). Every command returns a view
// (ctx.call applies it). Budget and comment text goes in with textContent
// and input values only.

import { invoke } from './api.js';
import { el, isMac } from './dom.js';
import { anchoredForm } from './modal.js';
import { readPref } from './prefs.js';
import { isWeb } from './host.js';
import { obj, str, errText, plural } from './util.js';

const st = {
  host: null,
  view: null,
  ctx: null,
  list: null,
  listKey: null,
  listError: '',
  editing: new Set(),
  drafts: new Map(),
  target: 0,
  error: '',
  busy: false,
  toSettleOnly: false,
};

/** Draws the Comments tab into `host`. */
export function renderComments(host, view, ctx) {
  st.host = host;
  st.view = view && typeof view === 'object' ? view : {};
  st.ctx = ctx || {};
  const r = review();
  const key = r ? `${st.view.doc}|${st.view.rev}|${r.rev}` : null;
  if (!r) st.list = null;
  draw();
  if (r && st.listKey !== key) fetchList(key);
}

for (const type of ['method:selection', 'method:cursor']) {
  document.addEventListener(type, () => {
    if (st.host && st.host.isConnected && !st.host.closest('[hidden]') && review() && review().mode === 'reviewing') draw();
  });
}

function review() {
  const r = st.view && st.view.review;
  return r && typeof r === 'object' ? r : null;
}

function author() {
  const a = readPref('noteAuthor', '');
  return typeof a === 'string' ? a : '';
}

function now() {
  return new Date().toISOString();
}

const STAMP = (() => {
  try {
    return new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' });
  } catch {
    return null;
  }
})();

function when(iso) {
  const s = str(iso);
  if (!s) return '';
  const d = new Date(s);
  if (Number.isNaN(d.getTime()) || !STAMP) return s;
  return STAMP.format(d);
}

async function fetchList(key) {
  st.listKey = key;
  try {
    const list = await invoke('review_list', { doc: st.view.doc });
    if (st.listKey !== key) return;
    st.list = list && Array.isArray(list.entries) ? list : null;
    st.listError = '';
  } catch (e) {
    if (st.listKey !== key) return;
    st.listError = errText(e);
  }
  if (st.host && st.host.isConnected) draw();
}

async function run(cmd, args) {
  if (st.busy || typeof st.ctx.call !== 'function') return false;
  st.busy = true;
  st.error = '';
  try {
    const reply = await st.ctx.call(cmd, { doc: st.view.doc, ...args });
    // A reply that isn't a view (a download in the browser): draw afresh.
    if (reply && typeof reply === 'object' && !('budget' in reply)) await st.ctx.call('get_view', { doc: st.view.doc });
    return true;
  } catch (e) {
    st.error = errText(e);
    return false;
  } finally {
    st.busy = false;
    draw();
  }
}

function button(text, cls, onClick, { title = '', label = '' } = {}) {
  const b = el('button', { class: cls, text, title: title || undefined, attrs: { type: 'button', 'aria-label': label || null }, data: { nobusy: '1' } });
  b.addEventListener('click', () => {
    if (!st.busy) onClick(b);
  });
  return b;
}

function modSave(e) {
  return e.key === 'Enter' && (isMac ? e.metaKey : e.ctrlKey) && !e.isComposing;
}

// ---------------------------------------------------------------------------
// Drawing

function draw() {
  const host = st.host;
  if (!host) return;
  const r = review();
  if (!r) {
    host.replaceChildren(
      el('h3', { class: 'notes-heading', text: 'Comments' }),
      el('p', {
        class: 'apply-hint',
        text: 'No review file is open. File > Open for Review… opens a budget to comment on without changing it; File > Open Review Comments… brings a reviewer’s comments into your budget.',
      }),
    );
    return;
  }
  const parts = [head(r)];
  if (r.mode === 'reviewing') parts.push(addBox());
  parts.push(listSection(r));
  if (st.error) parts.push(el('p', { class: 'setup-error', text: st.error, attrs: { role: 'alert' } }));
  host.replaceChildren(...parts);
}

function head(r) {
  const sec = el('section', { class: 'notes-line comments-head' });
  const reviewing = r.mode === 'reviewing';
  sec.append(
    el('h3', { class: 'notes-heading', text: reviewing ? 'Reviewing' : 'Review comments' }),
    el('p', {
      class: 'apply-hint',
      text: reviewing
        ? `This budget is read-only while you review it. Comments go in ${str(r.file)}${isWeb() ? '; download it to keep them' : ', saved as you write them'}.`
        : `From ${str(r.file)}: ${plural(r.to_settle, 'comment', 'comments')} to merge or dismiss.`,
    }),
  );
  if (!r.matches && !reviewing) {
    sec.append(el('p', { class: 'setup-error', text: `This review was written on a budget titled “${str(r.title)}”. Check it is about this budget before merging.` }));
  }
  if (r.unsaved) sec.append(el('p', { class: 'apply-hint', text: 'The review file has changes not written yet.' }));
  const actions = el('div', { class: 'notes-actions' });
  const mergeable = st.list ? st.list.entries.some((e) => e.can_merge && (e.status === 'open' || e.status === 'resolved')) : r.to_settle > 0;
  if (!reviewing && mergeable) {
    actions.append(button('Merge All', 'setup-btn setup-btn-primary', mergeAll, { title: 'Merge every comment that can become a note, as one undo step' }));
  }
  actions.append(
    button(isWeb() ? 'Download Review File' : 'Save Review File As…', 'setup-btn setup-btn-quiet', () => run('review_save', { save_as: true })),
    button(reviewing ? 'Stop Reviewing' : 'Close Review File', 'setup-btn setup-btn-quiet', () => run('review_close', {})),
  );
  sec.append(actions);
  return sec;
}

/** What the cursor is on, to comment on: [{ on, title }], then the whole budget. */
function targets() {
  const sel = typeof st.ctx.selection === 'function' ? st.ctx.selection() : null;
  const top = Array.isArray(obj(st.view.budget).topsheet) ? st.view.budget.topsheet : [];
  const out = [];
  const named = (x) => `${str(x.code)} ${str(x.name)}`.trim();
  const key = sel && (typeof sel.cursor === 'string' ? sel.cursor : Array.isArray(sel.keys) ? sel.keys[0] : null);
  const pushAccount = (a) => a && a.code !== undefined && out.push({ on: { account: str(a.code) }, title: `Account ${named(a)}` });
  const pushCategory = (c) => c && c.kind !== 'production_total' && c.code !== undefined && out.push({ on: { category: str(c.code) }, title: `Category ${named(c)}` });
  if (sel && sel.level === 'details') {
    const account = obj(st.view.account);
    const details = Array.isArray(account.details) ? account.details : [];
    if (typeof key === 'string' && key.startsWith('r')) {
      const id = Number(key.slice(1));
      const i = details.findIndex((d) => d && d.id === id);
      if (i >= 0) {
        const what = str(details[i].description).trim();
        out.push({ on: { row: id }, title: `Line ${i + 1}${what ? `: ${what}` : ''}` });
      }
    }
    const cat = top[account.ti];
    pushAccount(cat && Array.isArray(cat.accounts) ? cat.accounts[account.ai] : null);
    pushCategory(cat);
  } else if (sel && sel.level === 'accounts') {
    const cat = sel.cat ? top[sel.cat.ti] : null;
    if (cat && typeof key === 'string' && key.startsWith('a')) pushAccount((Array.isArray(cat.accounts) ? cat.accounts : [])[Number(key.slice(1))]);
    pushCategory(cat);
  } else if (sel && sel.level === 'topsheet' && typeof key === 'string' && key.startsWith('t')) {
    pushCategory(top[Number(key.slice(1))]);
  }
  out.push({ on: 'budget', title: 'The whole budget' });
  return out;
}

function addBox() {
  const choices = targets();
  if (st.target >= choices.length) st.target = 0;
  const select = el('select', { class: 'setup-input', attrs: { 'aria-label': 'Comment on' } });
  choices.forEach((t, i) => {
    const o = el('option', { text: t.title, attrs: { value: String(i) } });
    if (i === st.target) o.selected = true;
    select.append(o);
  });
  select.addEventListener('change', () => {
    st.target = Number(select.value) || 0;
  });
  const area = el('textarea', {
    class: 'setup-input notes-text notes-new',
    attrs: { rows: '3', spellcheck: 'true', placeholder: 'Add a comment…', 'aria-label': 'New comment' },
  });
  area.value = st.drafts.get('new') || '';
  area.addEventListener('input', () => st.drafts.set('new', area.value));
  const submit = async () => {
    const text = area.value;
    if (!text.trim()) return;
    const t = choices[st.target] || choices[choices.length - 1];
    if (await run('review_add', { on: t.on, text, created: now(), author: author() || null })) {
      st.drafts.delete('new');
      draw();
    }
  };
  area.addEventListener('keydown', (e) => {
    if (modSave(e)) {
      e.preventDefault();
      e.stopPropagation();
      submit();
    }
  });
  return el(
    'section',
    { class: 'notes-add' },
    el('label', { class: 'setup-label', text: 'Comment on' }),
    select,
    area,
    el(
      'div',
      { class: 'notes-actions' },
      el('span', { class: 'apply-hint', text: `${isMac ? '⌘' : 'Ctrl+'}Enter adds it. Signed with your name from the Notes tab.` }),
      button('Add', 'setup-btn setup-btn-primary', submit),
    ),
  );
}

const STATUS = { open: 'Open', resolved: 'Resolved', merged: 'In the budget as a note', dismissed: 'Dismissed' };

function listSection(r) {
  const sec = el('section', { class: 'notes-all' });
  const only = button('To settle only', 'apply-pill', () => {
    st.toSettleOnly = !st.toSettleOnly;
    draw();
  });
  only.setAttribute('aria-pressed', String(st.toSettleOnly));
  sec.append(el('div', { class: 'notes-all-head' }, el('h3', { class: 'notes-heading', text: `All comments (${r.comments})` }), only));
  if (st.listError) {
    sec.append(el('p', { class: 'setup-error', text: st.listError, attrs: { role: 'alert' } }));
    return sec;
  }
  if (!st.list) {
    sec.append(el('p', { class: 'apply-hint', text: 'Loading…' }));
    return sec;
  }
  const entries = st.list.entries.filter((e) => !st.toSettleOnly || e.status === 'open' || e.status === 'resolved');
  if (!entries.length) {
    sec.append(el('p', { class: 'setup-empty', text: st.list.entries.length ? 'Nothing left to settle.' : 'No comments yet.' }));
    return sec;
  }
  const ul = el('ul', { class: 'notes-cards', attrs: { 'aria-label': 'Review comments' } });
  for (const e of entries) ul.append(el('li', {}, card(r, e)));
  sec.append(ul);
  return sec;
}

function meta(e) {
  const parts = [when(e.created) || 'Undated'];
  if (e.author) parts.push(str(e.author));
  if (e.edited) parts.push(`edited ${when(e.edited)}`);
  parts.push(STATUS[e.status] || str(e.status));
  return parts.join(' · ');
}

const SETUP_TAB = { global: 'globals', fringe: 'fringes', group: 'groups', unit: 'units', set: 'sets', location: 'locations', currency: 'currencies', charge: 'charges', credit: 'credits' };

function goThere(e) {
  if (!e.found) return;
  const on = obj(e.on);
  if (on.setup && typeof st.ctx.openSetup === 'function') {
    st.ctx.openSetup(SETUP_TAB[on.setup.kind] || 'fringes', str(on.setup.name));
    return;
  }
  if (typeof st.ctx.jumpTo !== 'function') return;
  if (on.category !== undefined) st.ctx.jumpTo({ kind: 'category', ti: e.ti, category: e.category });
  else if (on.account !== undefined) st.ctx.jumpTo({ ti: e.ti, ai: e.ai, account: e.account, row_id: null });
  else if (on.row !== undefined) st.ctx.jumpTo({ ti: e.ti, ai: e.ai, account: e.account, row_id: e.row_id });
}

function card(r, e) {
  const done = e.status === 'merged' || e.status === 'dismissed';
  const box = el('div', { class: `notes-card${done || e.status === 'resolved' ? ' is-done' : ''}` });
  const place = e.found && e.on !== 'budget'
    ? button(str(e.place), 'notes-entry', () => goThere(e), { title: `Go to ${str(e.place)}` })
    : el('p', { class: 'notes-where', text: e.found ? str(e.place) : `Was on ${str(e.place)} (no longer in the budget)` });
  box.append(place, el('p', { class: 'notes-meta', text: meta(e) }));
  const ekey = String(e.id);
  if (r.mode === 'reviewing' && st.editing.has(ekey)) {
    const area = el('textarea', { class: 'setup-input notes-text', attrs: { rows: '4', 'aria-label': 'Edit comment' } });
    area.value = st.drafts.has(ekey) ? st.drafts.get(ekey) : str(e.text);
    area.addEventListener('input', () => st.drafts.set(ekey, area.value));
    const finish = () => {
      st.editing.delete(ekey);
      st.drafts.delete(ekey);
      draw();
    };
    const save = async () => {
      if (area.value === str(e.text) || (await run('review_update', { id: e.id, text: area.value, edited: now() }))) finish();
    };
    area.addEventListener('keydown', (k) => {
      if (modSave(k)) {
        k.preventDefault();
        k.stopPropagation();
        save();
      } else if (k.key === 'Escape') {
        k.preventDefault();
        k.stopPropagation();
        finish();
      }
    });
    box.append(
      area,
      el('div', { class: 'notes-actions' }, button('Cancel', 'setup-btn setup-btn-quiet', finish), button('Save', 'setup-btn setup-btn-primary', save)),
    );
    return box;
  }
  box.append(el('p', { class: 'notes-body', text: str(e.text) }));
  if (e.why_not && !done) box.append(el('p', { class: 'apply-hint', text: str(e.why_not) }));
  const actions = el('div', { class: 'notes-card-actions' });
  if (r.mode === 'reviewing') {
    actions.append(
      button('Edit', 'setup-btn setup-btn-quiet notes-mini', () => {
        st.editing.add(ekey);
        draw();
      }),
      button(e.resolved ? 'Reopen' : 'Resolve', 'setup-btn setup-btn-quiet notes-mini', () => run('review_resolve', { id: e.id, resolved: !e.resolved })),
      button('Delete', 'setup-btn setup-btn-quiet notes-mini', (b) =>
        anchoredForm({ anchor: b, title: 'Delete this comment?', message: str(e.text).slice(0, 120), submitText: 'Delete', danger: true, onSubmit: () => st.ctx.call('review_delete', { doc: st.view.doc, id: e.id }) }),
      ),
    );
  } else if (e.status === 'dismissed') {
    actions.append(button('Restore', 'setup-btn setup-btn-quiet notes-mini', () => run('review_restore', { id: e.id }), { title: 'Open this comment again' }));
  } else if (e.status !== 'merged') {
    if (e.can_merge) actions.append(button('Merge as Note', 'setup-btn setup-btn-quiet notes-mini', () => settle([{ id: e.id, action: 'merge' }])));
    actions.append(button('Dismiss', 'setup-btn setup-btn-quiet notes-mini', () => settle([{ id: e.id, action: 'dismiss' }])));
  }
  if (actions.childNodes.length) box.append(actions);
  return box;
}

async function settle(picks) {
  if (st.busy || typeof st.ctx.call !== 'function') return;
  st.busy = true;
  st.error = '';
  try {
    const reply = await st.ctx.call('review_settle', { doc: st.view.doc, picks, at: now(), by: author() || null });
    // The reply is the view and what was done: draw the view.
    await st.ctx.call('get_view', { doc: st.view.doc });
    if (reply && reply.message && typeof st.ctx.toast === 'function') st.ctx.toast(reply.message);
  } catch (e) {
    st.error = errText(e);
  } finally {
    st.busy = false;
    draw();
  }
}

function mergeAll() {
  const entries = st.list ? st.list.entries : [];
  const picks = entries.filter((e) => e.can_merge && (e.status === 'open' || e.status === 'resolved')).map((e) => ({ id: e.id, action: 'merge' }));
  if (!picks.length) {
    st.error = 'None of these comments can become a note: they are on setup items or the whole budget, or their place is gone. Dismiss them, or keep them in the file.';
    draw();
    return;
  }
  settle(picks);
}
