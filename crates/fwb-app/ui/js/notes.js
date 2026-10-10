// The tools panel's Notes tab (roadmap 2.8): the dated notes on what the
// grid's cursor is on, and every note in the budget as a review checklist.
//
// On screen, top to bottom:
// - The notes of the cursor's line (detail level) and of the account on
//   screen; on the account list, the cursor's account and its category; on
//   the topsheet, the cursor's category. Each note shows when it was written
//   (and edited) and by whom, with Edit and Delete; below them a box adds a
//   new one (Add, or Cmd/Ctrl+Enter). Editing saves with Save or
//   Cmd/Ctrl+Enter, Escape or Cancel puts the text back; saving empty text
//   deletes the note. Delete asks first. Every change is one undo step.
// - "All notes": every note in the budget (notes_list, src/notes.rs), in
//   budget order, each with a box that ticks it off as reviewed
//   (set_note_done) and a button that goes to its line, account or
//   category. "Open only" hides the ticked ones.
// - "Your name on new notes": free text kept in this window's preferences
//   (never sent anywhere but into the notes you add).
//
// Times are the page's clock as ISO 8601 text (UTC), shown in local time.
// What is typed and not saved is kept per owner while the panel redraws.
//
// tools.js calls renderNotes(host, view, ctx) on every render; ctx.selection()
// gives the grid's level and cursor row, ctx.call runs a command and
// ctx.jumpTo(entry) goes to a note's place. Budget text only reaches the
// page through textContent and input values.

import { invoke } from './api.js';
import { el, isMac } from './dom.js';
import { anchoredForm } from './modal.js';
import { readPref, writePref } from './prefs.js';
import { obj, str, errText } from './util.js';

const st = {
  host: null,
  view: null,
  ctx: null,
  list: null, // NoteEntry[] from notes_list
  listRev: null,
  listError: '',
  drafts: new Map(), // `${ownerKey}|new` or `${ownerKey}|${id}` -> text typed and not saved
  editing: new Set(), // `${ownerKey}|${id}` of notes open for editing
  error: '',
  errorAt: '', // owner key the error is about
  busy: false,
  openOnly: false,
};

/** Draws the Notes tab into `host`. */
export function renderNotes(host, view, ctx) {
  st.host = host;
  st.view = view && typeof view === 'object' ? view : {};
  st.ctx = ctx || {};
  draw();
  if (st.listRev !== st.view.rev) fetchList();
}

// The cursor moved: show what it is on now. A move sends both events, one
// after the other: draw once, after both.
let drawSoon = false;
for (const type of ['fwb:selection', 'fwb:cursor']) {
  document.addEventListener(type, () => {
    if (drawSoon) return;
    drawSoon = true;
    queueMicrotask(() => {
      drawSoon = false;
      if (st.host && st.host.isConnected && !st.host.closest('[hidden]')) draw();
    });
  });
}

// ---------------------------------------------------------------------------
// Small helpers

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

/** An ISO time as local text; anything else as given ('' stays ''). */
function when(iso) {
  const s = str(iso);
  if (!s) return '';
  const d = new Date(s);
  if (Number.isNaN(d.getTime()) || !STAMP) return s;
  return STAMP.format(d);
}

function ownerKey(owner) {
  if (owner.row !== undefined) return `row:${owner.row}`;
  if (owner.account !== undefined) return `account:${owner.account}`;
  return `category:${owner.category}`;
}

function topsheet() {
  const t = obj(st.view.budget).topsheet;
  return Array.isArray(t) ? t : [];
}

function selection() {
  return typeof st.ctx.selection === 'function' ? st.ctx.selection() : null;
}

/**
 * What the Notes tab shows notes for, from the grid's cursor:
 * [{ owner, title, notes }], first the cursor's own item.
 */
function targets() {
  const sel = selection();
  if (!sel) return [];
  const key = typeof sel.cursor === 'string' ? sel.cursor : Array.isArray(sel.keys) ? sel.keys[0] : null;
  const top = topsheet();
  const named = (x) => `${str(x.code)} ${str(x.name)}`.trim();
  const out = [];
  const pushAccount = (a) => {
    if (a && a.code !== undefined) out.push({ owner: { account: str(a.code) }, title: `Account ${named(a)}`, notes: a.notes });
  };
  const pushCategory = (c) => {
    if (c && c.kind !== 'production_total' && c.code !== undefined) out.push({ owner: { category: str(c.code) }, title: `Category ${named(c)}`, notes: c.notes });
  };
  if (sel.level === 'details') {
    const account = obj(st.view.account);
    const details = Array.isArray(account.details) ? account.details : [];
    if (typeof key === 'string' && key.startsWith('r')) {
      const id = Number(key.slice(1));
      const i = details.findIndex((d) => d && d.id === id);
      if (i >= 0) {
        const d = details[i];
        const what = str(d.description).trim();
        out.push({ owner: { row: id }, title: `Line ${i + 1}${what ? `: ${what}` : ''}`, notes: d.notes });
      }
    }
    const cat = top[account.ti];
    pushAccount(cat && Array.isArray(cat.accounts) ? cat.accounts[account.ai] : null);
  } else if (sel.level === 'accounts') {
    const cat = sel.cat ? top[sel.cat.ti] : null;
    if (cat && typeof key === 'string' && key.startsWith('a')) {
      pushAccount((Array.isArray(cat.accounts) ? cat.accounts : [])[Number(key.slice(1))]);
    }
    pushCategory(cat);
  } else if (sel.level === 'topsheet' && typeof key === 'string' && key.startsWith('t')) {
    pushCategory(top[Number(key.slice(1))]);
  }
  return out;
}

async function fetchList() {
  const rev = st.view.rev;
  st.listRev = rev;
  try {
    const list = await invoke('notes_list');
    if (st.listRev !== rev) return;
    st.list = Array.isArray(list) ? list : [];
    st.listError = '';
  } catch (e) {
    if (st.listRev !== rev) return;
    st.listError = errText(e);
  }
  if (st.host && st.host.isConnected) draw();
}

function captureFocus() {
  const a = document.activeElement;
  if (!a || !st.host || !st.host.contains(a) || !a.dataset || !a.dataset.key) return null;
  const info = { key: a.dataset.key };
  if (typeof a.selectionStart === 'number') {
    info.start = a.selectionStart;
    info.end = a.selectionEnd;
  }
  return info;
}

function byKey(key) {
  return st.host ? [...st.host.querySelectorAll('[data-key]')].find((e) => e.dataset.key === key) || null : null;
}

function restoreFocus(info) {
  if (!info) return;
  const t = byKey(info.key);
  if (!t) return;
  t.focus({ preventScroll: true });
  if (info.start !== undefined && typeof t.setSelectionRange === 'function') {
    try {
      t.setSelectionRange(info.start, info.end);
    } catch {
      // Not a text control.
    }
  }
}

function draw() {
  const host = st.host;
  if (!host) return;
  const focus = captureFocus();
  const parts = [];
  const shown = targets();
  if (!shown.length) {
    parts.push(
      el(
        'section',
        { class: 'notes-line' },
        el('h3', { class: 'notes-heading', text: 'Notes' }),
        el('p', { class: 'apply-hint', text: 'Select a line, an account or a category to see or write its notes.' }),
      ),
    );
  }
  shown.forEach((t, i) => parts.push(ownerSection(t, i)));
  parts.push(listSection(), authorSection());
  host.replaceChildren(...parts);
  restoreFocus(focus);
}

// ---------------------------------------------------------------------------
// Running commands

async function run(okey, cmd, args) {
  if (st.busy || typeof st.ctx.call !== 'function') return false;
  st.busy = true;
  st.error = '';
  st.errorAt = okey;
  try {
    await st.ctx.call(cmd, args);
    return true;
  } catch (e) {
    st.error = errText(e);
    return false;
  } finally {
    st.busy = false;
    draw();
  }
}

function modSave(e) {
  return e.key === 'Enter' && (isMac ? e.metaKey : e.ctrlKey) && !e.isComposing;
}

function button(text, cls, key, onClick, { label = '', title = '' } = {}) {
  const b = el('button', {
    class: cls,
    text,
    title: title || undefined,
    attrs: { type: 'button', 'aria-label': label || null },
    data: { key, nobusy: '1' },
  });
  b.addEventListener('click', () => {
    if (b.getAttribute('aria-disabled') !== 'true' && !st.busy) onClick();
  });
  return b;
}

// ---------------------------------------------------------------------------
// One owner's notes

function ownerSection(t, index) {
  const okey = ownerKey(t.owner);
  const hid = `notes-owner-h-${index}`;
  const sec = el('section', { class: 'notes-line', attrs: { 'aria-labelledby': hid } });
  sec.append(el('h3', { class: 'notes-heading', text: t.title, title: t.title, attrs: { id: hid } }));
  const notes = (Array.isArray(t.notes) ? t.notes : []).filter((n) => n && typeof n.text === 'string');
  if (notes.length) {
    const ul = el('ul', { class: 'notes-cards', attrs: { 'aria-label': `Notes on ${t.title}` } });
    for (const n of notes) ul.append(el('li', {}, card(t, okey, n)));
    sec.append(ul);
  }
  sec.append(newBox(t, okey, hid));
  if (st.error && st.errorAt === okey) sec.append(el('p', { class: 'setup-error', text: st.error, attrs: { role: 'alert' } }));
  return sec;
}

function meta(n) {
  const parts = [];
  const made = when(n.created);
  parts.push(made || 'Undated');
  if (n.author) parts.push(str(n.author));
  if (n.edited) parts.push(`edited ${when(n.edited)}`);
  if (n.done) parts.push('reviewed');
  return parts.join(' · ');
}

function card(t, okey, n) {
  const ekey = `${okey}|${n.id}`;
  const box = el('div', { class: `notes-card${n.done ? ' is-done' : ''}` });
  if (st.editing.has(ekey)) {
    const saved = str(n.text);
    const area = el('textarea', {
      class: 'setup-input notes-text',
      attrs: { rows: '4', spellcheck: 'true', 'aria-label': `Edit note on ${t.title}` },
      data: { key: `notes:edit:${ekey}` },
    });
    area.value = st.drafts.has(ekey) ? st.drafts.get(ekey) : saved;
    const done = () => {
      st.editing.delete(ekey);
      st.drafts.delete(ekey);
    };
    const save = async () => {
      const text = area.value;
      if (text === saved) {
        done();
        draw();
        byKey(`notes:editbtn:${ekey}`)?.focus();
        return;
      }
      const ok = await run(okey, 'update_note', { owner: t.owner, id: n.id, text, edited: now() });
      if (ok) {
        done();
        draw();
      }
    };
    const cancel = () => {
      done();
      draw();
      byKey(`notes:editbtn:${ekey}`)?.focus();
    };
    area.addEventListener('input', () => st.drafts.set(ekey, area.value));
    area.addEventListener('keydown', (e) => {
      if (modSave(e)) {
        e.preventDefault();
        e.stopPropagation();
        save();
      } else if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        cancel();
      }
    });
    box.append(
      el('p', { class: 'notes-meta', text: meta(n) }),
      area,
      el(
        'div',
        { class: 'notes-actions' },
        el('span', { class: 'apply-hint', text: `${isMac ? '⌘' : 'Ctrl+'}Enter saves; empty deletes it` }),
        button('Cancel', 'setup-btn setup-btn-quiet', `notes:cancel:${ekey}`, cancel),
        button('Save', 'setup-btn setup-btn-primary', `notes:save:${ekey}`, save),
      ),
    );
    return box;
  }
  const del = button('Delete', 'setup-btn setup-btn-quiet notes-mini', `notes:del:${ekey}`, () => remove(t, okey, n, del), {
    label: 'Delete this note',
  });
  box.append(
    el('p', { class: 'notes-meta', text: meta(n) }),
    el('p', { class: 'notes-body', text: str(n.text) }),
    el(
      'div',
      { class: 'notes-card-actions' },
      button('Edit', 'setup-btn setup-btn-quiet notes-mini', `notes:editbtn:${ekey}`, () => {
        st.editing.add(ekey);
        draw();
        byKey(`notes:edit:${ekey}`)?.focus();
      }, { label: 'Edit this note' }),
      del,
    ),
  );
  return box;
}

function remove(t, okey, n, anchor) {
  anchoredForm({
    anchor,
    title: 'Delete this note?',
    message: str(n.text).length > 120 ? `${str(n.text).slice(0, 120)}…` : str(n.text),
    submitText: 'Delete',
    danger: true,
    onSubmit: () => st.ctx.call('delete_note', { owner: t.owner, id: n.id }),
  }).then(() => {
    const next = byKey(`notes:new:${okey}`);
    if (next) next.focus({ preventScroll: true });
  });
}

function newBox(t, okey, hid) {
  const dkey = `${okey}|new`;
  const area = el('textarea', {
    class: 'setup-input notes-text notes-new',
    attrs: {
      rows: '3',
      spellcheck: 'true',
      placeholder: 'Add a note…',
      'aria-label': `New note on ${t.title}`,
      'aria-describedby': `${hid}-hint`,
    },
    data: { key: `notes:new:${okey}`, new: '1' },
  });
  area.value = st.drafts.get(dkey) || '';
  const add = el('button', {
    class: 'setup-btn setup-btn-primary',
    text: 'Add',
    attrs: { type: 'button' },
    data: { key: `notes:add:${okey}`, nobusy: '1' },
  });
  const paint = () => {
    if (area.value.trim() && !st.busy) add.removeAttribute('aria-disabled');
    else add.setAttribute('aria-disabled', 'true');
  };
  const submit = async () => {
    const text = area.value;
    if (!text.trim()) return;
    const ok = await run(okey, 'add_note', { owner: t.owner, text, created: now(), author: author() || null });
    if (ok) {
      st.drafts.delete(dkey);
      draw();
      byKey(`notes:new:${okey}`)?.focus({ preventScroll: true });
    }
  };
  area.addEventListener('input', () => {
    if (area.value) st.drafts.set(dkey, area.value);
    else st.drafts.delete(dkey);
    paint();
  });
  area.addEventListener('keydown', (e) => {
    if (modSave(e)) {
      e.preventDefault();
      e.stopPropagation();
      submit();
    } else if (e.key === 'Escape' && area.value) {
      e.preventDefault();
      e.stopPropagation();
      area.value = '';
      st.drafts.delete(dkey);
      paint();
    }
  });
  add.addEventListener('click', () => {
    if (add.getAttribute('aria-disabled') !== 'true') submit();
  });
  paint();
  return el(
    'div',
    { class: 'notes-add' },
    area,
    el(
      'div',
      { class: 'notes-actions' },
      el('span', { class: 'apply-hint', text: `${isMac ? '⌘' : 'Ctrl+'}Enter adds it`, attrs: { id: `${hid}-hint` } }),
      add,
    ),
  );
}

// ---------------------------------------------------------------------------
// Every note in the budget: the review checklist

function entryWhere(n) {
  const acct = `${str(n.account)} ${str(n.account_name)}`.trim();
  if (n.kind === 'category') return `Category ${`${str(n.category)} ${str(n.category_name)}`.trim()}`;
  if (n.kind === 'account') return `Account ${acct}`;
  return `${acct} · line ${n.line}`;
}

function entryWhat(n) {
  if (n.kind === 'row') return str(n.description).trim() || '(no description)';
  if (n.kind === 'account') return str(n.account_name).trim() || str(n.account);
  return str(n.category_name).trim() || str(n.category);
}

function listSection() {
  const list = st.list;
  const sec = el('section', { class: 'notes-all', attrs: { 'aria-labelledby': 'notes-all-h' } });
  const open = list ? list.filter((n) => !n.done).length : 0;
  const head = el(
    'div',
    { class: 'notes-all-head' },
    el('h3', {
      class: 'notes-heading',
      text: list ? `All notes (${list.length}${list.length ? `, ${open} open` : ''})` : 'All notes',
      attrs: { id: 'notes-all-h' },
    }),
  );
  const only = button('Open only', 'apply-pill', 'notes:openonly', () => {
    st.openOnly = !st.openOnly;
    draw();
  }, { title: 'Hide the notes ticked off as reviewed' });
  only.setAttribute('aria-pressed', String(st.openOnly));
  head.append(only);
  sec.append(head);
  if (st.listError) {
    sec.append(el('p', { class: 'setup-error', text: st.listError, attrs: { role: 'alert' } }));
    return sec;
  }
  if (!list) {
    sec.append(el('p', { class: 'apply-hint', text: 'Loading…' }));
    return sec;
  }
  const shown = st.openOnly ? list.filter((n) => !n.done) : list;
  if (!list.length) {
    sec.append(el('p', { class: 'setup-empty', text: 'No notes in this budget yet.' }));
    return sec;
  }
  if (!shown.length) {
    sec.append(el('p', { class: 'setup-empty', text: 'Every note is ticked off.' }));
    return sec;
  }
  const here = new Set(targets().map((t) => ownerKey(t.owner)));
  const ul = el('ul', { class: 'notes-list', attrs: { 'aria-label': 'Every note in the budget' } });
  for (const n of shown) {
    const okey = ownerKey(n.owner || {});
    const ekey = `${okey}|${n.id}`;
    const check = el(
      'button',
      {
        class: 'notes-check',
        title: n.done ? 'Reviewed: click to mark it open again' : 'Tick it off as reviewed',
        attrs: { type: 'button', role: 'checkbox', 'aria-checked': n.done ? 'true' : 'false', 'aria-label': `Reviewed: ${entryWhat(n)}` },
        data: { key: `notes:done:${ekey}`, nobusy: '1' },
      },
      el('span', { class: 'apply-box', attrs: { 'aria-hidden': 'true' } }),
    );
    check.addEventListener('click', () => {
      if (!st.busy) run('list', 'set_note_done', { owner: n.owner, id: n.id, done: !n.done });
    });
    const stamp = [when(n.created), str(n.author)].filter(Boolean).join(' · ');
    const go = el(
      'button',
      {
        class: 'notes-entry',
        title: `Go to ${entryWhere(n)}`,
        attrs: { type: 'button', 'aria-current': here.has(okey) ? 'true' : null },
        data: { key: `notes:go:${ekey}`, nobusy: '1' },
      },
      el('span', { class: 'notes-where', text: stamp ? `${entryWhere(n)} · ${stamp}` : entryWhere(n) }),
      el('span', { class: 'notes-desc', text: entryWhat(n) }),
      el('span', { class: 'notes-snippet', text: `${str(n.note)}${n.more ? '…' : ''}` }),
    );
    go.addEventListener('click', () => {
      if (typeof st.ctx.jumpTo === 'function') st.ctx.jumpTo(n);
    });
    go.addEventListener('keydown', (e) => {
      const step = e.key === 'ArrowDown' ? 1 : e.key === 'ArrowUp' ? -1 : 0;
      if (!step) return;
      const all = [...ul.querySelectorAll('.notes-entry')];
      const next = all[all.indexOf(go) + step];
      if (next) {
        e.preventDefault();
        next.focus();
      }
    });
    ul.append(el('li', { class: `notes-item${n.done ? ' is-done' : ''}` }, check, go));
  }
  sec.append(ul);
  if (st.error && st.errorAt === 'list') sec.append(el('p', { class: 'setup-error', text: st.error, attrs: { role: 'alert' } }));
  return sec;
}

// ---------------------------------------------------------------------------
// The name new notes are signed with

function authorSection() {
  const id = 'notes-author';
  const input = el('input', {
    class: 'setup-input notes-author-input',
    attrs: { id, type: 'text', autocomplete: 'off', spellcheck: 'false', maxlength: '100', placeholder: 'Leave blank to sign nothing' },
    data: { key: 'notes:author' },
  });
  input.value = author();
  input.addEventListener('change', () => writePref('noteAuthor', input.value.trim()));
  return el(
    'section',
    { class: 'notes-author' },
    el('label', { class: 'setup-label', text: 'Your name on new notes', attrs: { for: id } }),
    input,
  );
}
