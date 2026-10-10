// The History tab of the tools panel (roadmap 2.23; src/history.rs): every
// undo step of the budget the panel shows, oldest first, each named by the
// command that made it ("Edit rate on 21-01 line 3", "Paste 12 rows") with
// its time. The first row is the start of the list (when the budget was
// opened or created, or the oldest step still kept). Steps after the current
// one are the ones Redo would bring back, shown fainter.
//
// A click on a step (or Enter) undoes or redoes the budget to just after it.
// Each step's "Version…" button saves the budget as it was then as a named
// version, leaving it as it is now (one undo step, like any version save).
// Up and Down move between steps, Right to a step's button, Left back.
//
// renderHistory(host, view, ctx) draws the list from what it fetched last
// and fetches it again (history_list) when the budget changed (a new view
// rev). Jumping and saving go through ctx.historyJump(to) and
// ctx.historySave(step, name): the main window runs them (like Undo), and
// the tools window passes them on to it (toolswin.js).
//
// Step labels are budget text: they go in with textContent only.

import { el } from './dom.js';
import { invoke } from './api.js';
import { formDialog } from './modal.js';

// What was fetched last, and for which budget and rev.
const st = {
  doc: null,
  rev: null,
  history: null, // { steps: [{ label, at, done }], current, trimmed }
  error: '',
  fetching: false,
  again: false,
  host: null,
  ctx: null,
  scrolledTo: null, // the doc:current the list was last scrolled to
};

/** Draws the History tab into `host`. */
export function renderHistory(host, view, ctx) {
  st.host = host;
  st.ctx = ctx;
  const doc = view && typeof view.doc === 'number' ? view.doc : null;
  const fresh = doc !== st.doc || view.rev !== st.rev;
  if (doc !== st.doc) {
    st.history = null;
    st.error = '';
  }
  st.doc = doc;
  st.rev = view ? view.rev : null;
  draw();
  if (fresh) fetchHistory();
}

async function fetchHistory() {
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
        const h = await invoke('history_list', doc === null ? {} : { doc });
        if (doc !== st.doc) continue;
        st.history = h && Array.isArray(h.steps) ? h : { steps: [], current: 0, trimmed: false };
        st.error = '';
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

/** "2:41 PM", with the date too when it isn't today. */
function timeText(ms) {
  const d = new Date(ms);
  if (!Number.isFinite(d.getTime()) || ms <= 0) return '';
  const now = new Date();
  const time = d.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
  if (d.toDateString() === now.toDateString()) return time;
  return `${d.toLocaleDateString([], { month: 'short', day: 'numeric' })} ${time}`;
}

function draw() {
  const host = st.host;
  if (!host) return;
  const focusKey = host.contains(document.activeElement) && document.activeElement.dataset ? document.activeElement.dataset.key : null;
  const box = el('div', { class: 'history' });
  if (st.error) box.append(el('p', { class: 'history-error', text: st.error, attrs: { role: 'alert' } }));
  const h = st.history;
  if (!h) {
    box.append(el('p', { class: 'history-empty', text: 'Loading the history…' }));
    host.replaceChildren(box);
    return;
  }
  if (!h.steps.length) {
    box.append(el('p', { class: 'history-empty', text: 'No changes yet. Every change you make is listed here, and you can go back to any of them.' }));
  }
  const list = el('ol', { class: 'history-list', attrs: { 'aria-label': 'Undo history, oldest first' } });
  // Position 0 (the start), then each step: position i is just after step i.
  const rows = [
    {
      pos: 0,
      label: h.trimmed ? 'Start (older changes not kept)' : 'Start',
      title: h.trimmed ? 'The oldest state still kept' : 'As the budget was opened or created',
      at: 0,
      done: true,
    },
    ...h.steps.map((s, i) => ({ pos: i + 1, label: s.label, title: s.label, at: s.at, done: s.done })),
  ];
  if (!h.steps.length) rows.length = 0;
  for (const r of rows) list.append(row(r, h.current));
  box.append(list);
  host.replaceChildren(box);
  if (focusKey) {
    const again = [...host.querySelectorAll('[data-key]')].find((e) => e.dataset.key === focusKey);
    if (again) again.focus({ preventScroll: true });
  }
  // The current step in view, once per change of it.
  const mark = `${st.doc}:${h.current}:${h.steps.length}`;
  if (mark !== st.scrolledTo) {
    st.scrolledTo = mark;
    const cur = list.querySelector('.history-step.is-current');
    if (cur && typeof cur.scrollIntoView === 'function') cur.scrollIntoView({ block: 'nearest' });
  }
}

function row(r, current) {
  const isCurrent = r.pos === current;
  const cls = ['history-step'];
  if (isCurrent) cls.push('is-current');
  if (r.pos > current) cls.push('is-undone');
  const jump = el(
    'button',
    {
      class: 'history-jump',
      title: isCurrent ? `${r.title} (where the budget is now)` : r.pos > current ? `Redo to here: ${r.title}` : `Undo to here: ${r.title}`,
      attrs: {
        type: 'button',
        tabindex: isCurrent ? '0' : '-1',
        'aria-current': isCurrent ? 'step' : null,
      },
      data: { key: `history:jump:${r.pos}`, pos: r.pos },
      on: {
        click: () => {
          if (!isCurrent && st.ctx && typeof st.ctx.historyJump === 'function') st.ctx.historyJump(r.pos);
        },
        keydown: (e) => stepKey(e, r.pos, 'jump'),
      },
    },
    el('span', { class: 'history-label', text: r.label }),
    el('span', { class: 'history-time', text: timeText(r.at) }),
  );
  const save = el('button', {
    class: 'history-version',
    text: 'Version…',
    title: 'Save the budget as it was here as a named version (the budget stays as it is now)',
    attrs: { type: 'button', tabindex: '-1', 'aria-label': `Save as a version: ${r.label}` },
    data: { key: `history:version:${r.pos}`, pos: r.pos },
    on: {
      click: () => saveVersion(r),
      keydown: (e) => stepKey(e, r.pos, 'version'),
    },
  });
  return el('li', { class: cls.join(' ') }, jump, save);
}

/** Up and Down between steps, Right and Left between a step and its button, Home and End. */
function stepKey(e, pos, part) {
  if (e.altKey || e.metaKey || e.ctrlKey || e.shiftKey) return;
  const host = st.host;
  if (!host) return;
  const jumps = [...host.querySelectorAll('.history-jump')];
  const i = jumps.findIndex((b) => Number(b.dataset.pos) === pos);
  let to = null;
  if (e.key === 'ArrowDown') to = jumps[Math.min(jumps.length - 1, i + 1)];
  else if (e.key === 'ArrowUp') to = jumps[Math.max(0, i - 1)];
  else if (e.key === 'Home') to = jumps[0];
  else if (e.key === 'End') to = jumps[jumps.length - 1];
  else if (e.key === 'ArrowRight' && part === 'jump') to = jumps[i] && jumps[i].nextElementSibling;
  else if (e.key === 'ArrowLeft' && part === 'version') to = jumps[i];
  if (!to) return;
  e.preventDefault();
  e.stopPropagation();
  for (const b of jumps) b.tabIndex = -1;
  if (to.classList.contains('history-jump')) to.tabIndex = 0;
  to.focus();
}

/** Asks for a name, then saves the budget as it was at `r` as a version. */
async function saveVersion(r) {
  if (!st.ctx || typeof st.ctx.historySave !== 'function') return;
  const name = r.pos === 0 ? 'Start of the history' : r.label;
  await formDialog({
    title: 'Save as a version',
    message: 'The budget as it was at this step becomes a named version. The budget itself stays as it is now.',
    fields: [{ key: 'name', label: 'Version name', value: name.slice(0, 200) }],
    submitText: 'Save Version',
    onSubmit: (f) => {
      const n = String(f.name || '').trim();
      if (!n) return Promise.reject('A version needs a name.');
      return st.ctx.historySave(r.pos, n);
    },
  });
}
