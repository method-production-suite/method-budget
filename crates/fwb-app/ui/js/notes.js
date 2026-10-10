// The tools panel's Notes tab: the note on the line under the grid's cursor,
// edited in place, and every line in the budget that has a note, each a
// button that goes to the line.
//
// The line's note is the same note as Edit > Note (Shift+Cmd+Space): Save, or
// Cmd/Ctrl+Enter, keeps it with set_row_note (one undo step), an empty note
// removes it, Revert or Escape puts back what is saved. What is typed but not
// saved is kept per line while the panel redraws. The list comes from
// notes_list (src/tools.rs) and is fetched again when the budget changes.
//
// tools.js calls renderNotes(host, view, ctx) on every render; ctx.selection()
// gives the grid's level and cursor row, ctx.call runs a command and
// ctx.jumpTo(entry) opens the line's account with the line selected. Budget
// text only reaches the page through textContent and input values.

import { invoke } from './api.js';
import { el, isMac } from './dom.js';

const st = {
  host: null,
  view: null,
  ctx: null,
  list: null, // [{ row_id, ti, ai, category, account, account_name, line, description, note, more }]
  listRev: null,
  listError: '',
  drafts: new Map(), // row id -> text typed and not saved
  error: '',
  saving: false,
};

/** Draws the Notes tab into `host`. */
export function renderNotes(host, view, ctx) {
  st.host = host;
  st.view = view && typeof view === 'object' ? view : {};
  st.ctx = ctx || {};
  draw();
  if (st.listRev !== st.view.rev) fetchList();
}

// The cursor moved: show that line's note.
for (const type of ['fwb:selection', 'fwb:cursor']) {
  document.addEventListener(type, () => {
    if (st.host && st.host.isConnected && !st.host.closest('[hidden]')) draw();
  });
}

function obj(v) {
  return v && typeof v === 'object' && !Array.isArray(v) ? v : {};
}

function str(v) {
  return v === undefined || v === null ? '' : String(v);
}

/** The line under the cursor: { id, num, detail, account } or null. */
function cursorLine() {
  const sel = typeof st.ctx.selection === 'function' ? st.ctx.selection() : null;
  if (!sel || sel.level !== 'details') return null;
  const key = typeof sel.cursor === 'string' ? sel.cursor : Array.isArray(sel.keys) ? sel.keys[0] : null;
  if (typeof key !== 'string' || !key.startsWith('r')) return null;
  const id = Number(key.slice(1));
  const account = obj(st.view.account);
  const details = Array.isArray(account.details) ? account.details : [];
  const i = details.findIndex((d) => d && d.id === id);
  if (i < 0) return { id, num: null, detail: null, account };
  return { id, num: i + 1, detail: details[i], account };
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
    st.listError = String(e);
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

function restoreFocus(info) {
  if (!info) return;
  const t = [...st.host.querySelectorAll('[data-key]')].find((e) => e.dataset.key === info.key);
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
  host.replaceChildren(lineSection(), listSection());
  restoreFocus(focus);
}

function heading(text, id) {
  return el('h3', { class: 'notes-heading', text, attrs: { id } });
}

function lineSection() {
  const sec = el('section', { class: 'notes-line', attrs: { 'aria-labelledby': 'notes-line-h' } });
  const line = cursorLine();
  if (!line || !line.detail) {
    sec.append(
      heading('This line', 'notes-line-h'),
      el('p', { class: 'apply-hint', text: line ? 'Loading the line…' : 'Select a line in an account to see or write its note.' }),
    );
    return sec;
  }
  const d = line.detail;
  const what = str(d.description).trim();
  const title = `Line ${line.num}${what ? `: ${what}` : ''}`;
  sec.append(heading(title, 'notes-line-h'));
  const saved = typeof d.note === 'string' ? d.note : '';
  const value = st.drafts.has(line.id) ? st.drafts.get(line.id) : saved;
  const text = el('textarea', {
    class: 'setup-input notes-text',
    attrs: {
      rows: '5',
      spellcheck: 'true',
      placeholder: 'No note. Type one here.',
      'aria-labelledby': 'notes-line-h',
      'aria-describedby': 'notes-hint',
    },
    data: { key: 'notes:text', orig: saved },
  });
  text.value = value;
  const changed = () => text.value !== saved;
  const save = el('button', {
    class: 'setup-btn setup-btn-primary',
    text: 'Save',
    attrs: { type: 'button' },
    data: { key: 'notes:save', nobusy: '1' },
  });
  const revert = el('button', {
    class: 'setup-btn setup-btn-quiet',
    text: 'Revert',
    attrs: { type: 'button' },
    data: { key: 'notes:revert', nobusy: '1' },
  });
  const paint = () => {
    for (const b of [save, revert]) {
      if (changed() && !st.saving) b.removeAttribute('aria-disabled');
      else b.setAttribute('aria-disabled', 'true');
    }
  };
  const doSave = async () => {
    if (!changed() || st.saving || typeof st.ctx.call !== 'function') return;
    st.saving = true;
    st.error = '';
    paint();
    const note = text.value;
    try {
      await st.ctx.call('set_row_note', { rowId: line.id, note: note.trim() ? note : null });
      st.drafts.delete(line.id);
    } catch (e) {
      st.error = String(e);
    } finally {
      st.saving = false;
      draw();
    }
  };
  const doRevert = () => {
    st.drafts.delete(line.id);
    text.value = saved;
    paint();
  };
  text.addEventListener('input', () => {
    if (changed()) st.drafts.set(line.id, text.value);
    else st.drafts.delete(line.id);
    paint();
  });
  text.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && (isMac ? e.metaKey : e.ctrlKey) && !e.isComposing) {
      e.preventDefault();
      e.stopPropagation();
      doSave();
    } else if (e.key === 'Escape' && changed()) {
      e.preventDefault();
      e.stopPropagation();
      doRevert();
    }
  });
  save.addEventListener('click', () => {
    if (save.getAttribute('aria-disabled') !== 'true') doSave();
  });
  revert.addEventListener('click', () => {
    if (revert.getAttribute('aria-disabled') !== 'true') doRevert();
  });
  paint();
  sec.append(
    text,
    el(
      'div',
      { class: 'notes-actions' },
      el('span', { class: 'apply-hint', text: `${isMac ? '⌘' : 'Ctrl+'}Enter saves; an empty note is removed`, attrs: { id: 'notes-hint' } }),
      revert,
      save,
    ),
  );
  if (st.error) sec.append(el('p', { class: 'setup-error', text: st.error, attrs: { role: 'alert' } }));
  return sec;
}

function listSection() {
  const list = st.list;
  const count = list ? list.length : 0;
  const sec = el('section', { class: 'notes-all', attrs: { 'aria-labelledby': 'notes-all-h' } });
  sec.append(heading(list ? `Noted lines (${count})` : 'Noted lines', 'notes-all-h'));
  if (st.listError) {
    sec.append(el('p', { class: 'setup-error', text: st.listError, attrs: { role: 'alert' } }));
    return sec;
  }
  if (!list) {
    sec.append(el('p', { class: 'apply-hint', text: 'Loading…' }));
    return sec;
  }
  if (!count) {
    sec.append(el('p', { class: 'setup-empty', text: 'No line in this budget has a note yet.' }));
    return sec;
  }
  const here = cursorLine();
  const ul = el('ul', { class: 'notes-list', attrs: { 'aria-label': 'Lines with notes' } });
  for (const n of list) {
    const where = `${str(n.account)} ${str(n.account_name)}`.trim();
    const desc = str(n.description).trim() || '(no description)';
    const note = `${str(n.note)}${n.more ? '…' : ''}`;
    const b = el(
      'button',
      {
        class: 'notes-entry',
        title: `Go to line ${n.line} in ${where}`,
        attrs: { type: 'button', 'aria-current': here && here.id === n.row_id ? 'true' : null },
        data: { key: `notes:go:${n.row_id}`, nobusy: '1' },
      },
      el('span', { class: 'notes-where', text: `${where} · line ${n.line}` }),
      el('span', { class: 'notes-desc', text: desc }),
      el('span', { class: 'notes-snippet', text: note }),
    );
    b.addEventListener('click', () => {
      if (typeof st.ctx.jumpTo === 'function') st.ctx.jumpTo(n);
    });
    b.addEventListener('keydown', (e) => {
      const step = e.key === 'ArrowDown' ? 1 : e.key === 'ArrowUp' ? -1 : 0;
      if (!step) return;
      const all = [...ul.querySelectorAll('.notes-entry')];
      const next = all[all.indexOf(b) + step];
      if (next) {
        e.preventDefault();
        next.focus();
      }
    });
    ul.append(el('li', {}, b));
  }
  sec.append(ul);
  return sec;
}
