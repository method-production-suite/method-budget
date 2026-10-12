// Working together on one budget through a shared folder (src/shared.rs,
// src/shared_app.rs; docs/APP.md, "Sharing a budget"): the main window's
// side of it.
//
// - File > Share… (and the Share button): one dialog that starts sharing
//   through a folder (the budget's own, or one chosen in Rust's folder
//   dialog, where the file is moved or copied after a confirmation), and,
//   once shared, lists the people seen and this person's name and color.
// - The status item in the toolbar: "Synced through the folder · 12 s ago",
//   "Sharing 4 changes…", "Can't reach the shared folder", with a count to
//   review; clicking it opens a small panel with the folder, the people
//   and when the file was last written.
// - Presence: initials in the person's color next to the status item, and
//   next to the topsheet rows and accounts other people are in (from their
//   heartbeat files, so a minute old at worst).
// - Changes by other people arrive through the `share-changed` event: the
//   view is fetched again and the changed cells get a brief wash of the
//   editor's color. The cursor, selection and scroll never move.
// - Conflicts (a value changed by two people): a badge on the cell with a
//   popover to keep or put back either value. Who Changed This… on a cell
//   lists every value it has had and who typed it.
//
// Everything is drawn with el() and textContent; names and labels come
// from other people's files and are never read as HTML. No inline styles
// (the CSP): colors are classes from the palette below.

import { el, $ } from './dom.js';
import { customDialog, confirmDialog, isModalOpen } from './modal.js';
import { readPref, writePref } from './prefs.js';
import { setRules } from './sheet.js';

/** The person colors Rust names (src/shared.rs COLORS); CSS has one class per name. */
export const PERSON_COLORS = ['teal', 'blue', 'purple', 'orange', 'rose', 'green', 'amber', 'slate'];

function colorClass(color) {
  return PERSON_COLORS.includes(color) ? `pc-${color}` : 'pc-slate';
}

// { view(), doc(), invoke, apply(view, opts), toast, refresh() -> Promise,
//   grid(), level(), place() -> { category, account }, focus(), showTab(tab),
//   rowKeyFor(place) -> { key, col } | null, jumpTo(entry) }
let ctx = null;

const st = {
  status: new Map(), // doc -> { status, at: Date.now() when received }
  panel: null, // the status panel, while open
  popover: null, // a conflict popover, while open
  placeSent: '', // the last place reported to Rust, as JSON
  placeTimer: null,
  tickTimer: null,
  flashTimers: [],
  reportedDoc: null,
  revSeen: null, // the view rev the status was last asked for
};

/** How long a changed cell keeps the editor's color (design 9.4). */
const FLASH_MS = 1600;

export function initShare(context) {
  ctx = context;
  const btn = $('btn-share');
  if (btn) btn.addEventListener('click', () => openShareDialog());
  const status = $('share-status');
  if (status) status.addEventListener('click', () => toggleStatusPanel());
  listen('share-status', (p) => p && typeof p === 'object' && statusArrived(p));
  listen('share-changed', (p) => p && typeof p === 'object' && remoteArrived(p));
  // "12 s ago" keeps counting between status events.
  st.tickTimer = setInterval(() => {
    if (ctx && ctx.view() && ctx.view().shared) renderBar(ctx.view());
  }, 5000);
}

function listen(name, fn) {
  const events = window.__TAURI__ && window.__TAURI__.event;
  if (!events || typeof events.listen !== 'function') return;
  events.listen(name, (e) => fn(e.payload), { target: 'main' }).catch(() => {});
}

function current() {
  const doc = ctx ? ctx.doc() : null;
  const s = st.status.get(doc);
  return s ? s.status : null;
}

// ---------------------------------------------------------------------------
// The toolbar: status item and people

function statusArrived(p) {
  if (typeof p.doc !== 'number' || !p.status) return;
  st.status.set(p.doc, { status: p.status, at: Date.now() });
  if (ctx && p.doc === ctx.doc() && ctx.view()) {
    renderBar(ctx.view());
    decorateShare(ctx.view());
    if (st.panel) drawPanel();
  }
}

/** Draws the toolbar's share area for `view` (hidden when it isn't shared). */
export function renderBar(view) {
  const area = $('share-area');
  if (!area) return;
  if (!view || !view.shared) {
    area.hidden = true;
    closeStatusPanel();
    return;
  }
  area.hidden = false;
  // A new revision of the budget (an edit, a value put back, an undo): the
  // review badges and counts may have changed, so ask now rather than at
  // the next timer pass.
  if (view.rev !== st.revSeen) {
    st.revSeen = view.rev;
    fetchStatus();
  }
  const s = current();
  if (!s) {
    // The first status for this budget is on its way (or asked for here).
    setStatusText('Shared through a folder', 'quiet');
    $('share-people').replaceChildren();
    fetchStatus();
    return;
  }
  const entry = st.status.get(ctx.doc());
  const extra = Math.floor((Date.now() - entry.at) / 1000);
  const synced = typeof s.synced_secs === 'number' ? s.synced_secs + extra : null;
  let text = '';
  let tone = 'ok';
  if (s.error) {
    text = s.error;
    tone = 'bad';
  } else if (s.unshared > 0) {
    text = s.unshared === 1 ? 'Sharing 1 change…' : `Sharing ${s.unshared} changes…`;
    tone = 'busy';
  } else if (s.waiting > 0) {
    text = s.waiting === 1 ? 'Waiting for 1 change that hasn’t arrived' : `Waiting for ${s.waiting} changes that haven’t arrived`;
    tone = 'warn';
  } else if (synced !== null) {
    text = `Synced through the folder · ${ago(synced)}`;
  } else {
    text = 'Shared through a folder';
  }
  setStatusText(text, tone, s.review_count);
  renderPeople(s);
}

function setStatusText(text, tone, review = 0) {
  const b = $('share-status');
  const t = $('share-status-text');
  if (!b || !t) return;
  t.textContent = text;
  b.dataset.tone = tone;
  b.title = `${text}. Click for who is in this budget and where the changes go.`;
  let badge = b.querySelector('.share-review-count');
  if (review > 0) {
    if (!badge) {
      badge = el('span', { class: 'share-review-count' });
      b.append(badge);
    }
    badge.textContent = review === 1 ? '1 to review' : `${review} to review`;
  } else if (badge) {
    badge.remove();
  }
}

/** "12 s ago", "3 min ago", "2 h ago". */
export function ago(secs) {
  if (secs < 60) return `${Math.max(0, Math.round(secs))} s ago`;
  if (secs < 3600) return `${Math.round(secs / 60)} min ago`;
  if (secs < 86400) return `${Math.round(secs / 3600)} h ago`;
  return `${Math.round(secs / 86400)} d ago`;
}

function whereText(p) {
  if (p.account) return `in ${p.account}`;
  if (p.category) return `in ${p.category}`;
  return 'on the topsheet';
}

function personTitle(p) {
  const when = p.me ? 'you' : typeof p.seen_secs === 'number' ? (p.away ? `away, last seen ${ago(p.seen_secs)}` : ago(p.seen_secs)) : 'not seen yet';
  return `${p.name} · ${whereText(p)} · ${when}`;
}

/** An avatar: initials in the person's color. */
export function avatar(p, extraClass = '') {
  return el('span', {
    class: `share-avatar ${colorClass(p.color)} ${p.away ? 'is-away' : ''} ${extraClass}`,
    text: p.initials || '?',
    title: personTitle(p),
    attrs: { 'aria-label': personTitle(p) },
  });
}

function renderPeople(s) {
  const host = $('share-people');
  if (!host) return;
  const here = s.people.filter((p) => !p.away);
  const away = s.people.filter((p) => p.away);
  const shown = [...here, ...away].slice(0, 4);
  const nodes = shown.map((p) => avatar(p));
  if (s.people.length > shown.length) {
    nodes.push(el('span', { class: 'share-avatar share-more', text: `+${s.people.length - shown.length}`, title: `${s.people.length} people have opened this budget` }));
  }
  if (!s.people.length) nodes.push(el('span', { class: 'share-alone', text: 'Only you so far', title: 'Nobody else has opened this budget from the shared folder yet' }));
  host.replaceChildren(...nodes);
}

async function fetchStatus() {
  if (!ctx) return;
  const doc = ctx.doc();
  try {
    const s = await ctx.invoke('share_status', { doc });
    if (s) statusArrived({ doc, status: s });
  } catch {
    // The next status event draws it.
  }
}

// ---------------------------------------------------------------------------
// The status panel

function toggleStatusPanel() {
  if (st.panel) closeStatusPanel();
  else openStatusPanel();
}

function openStatusPanel() {
  const anchor = $('share-status');
  if (!anchor || !ctx || !ctx.view() || !ctx.view().shared) return;
  const panel = el('div', { class: 'share-panel', attrs: { role: 'dialog', 'aria-label': 'Sharing status' } });
  document.body.append(panel);
  st.panel = panel;
  drawPanel();
  const r = anchor.getBoundingClientRect();
  // Placed through a stylesheet rule, never an inline style (the CSP).
  setRules('share-panel', `.share-panel.placed { top: ${Math.round(r.bottom + 6)}px; right: ${Math.max(8, Math.round(window.innerWidth - r.right))}px; }`);
  panel.classList.add('placed');
  setTimeout(() => {
    document.addEventListener('mousedown', onPanelOutside, true);
    window.addEventListener('keydown', onPanelKey, true);
  }, 0);
  const first = panel.querySelector('button');
  if (first) first.focus();
}

function onPanelOutside(e) {
  if (st.panel && !st.panel.contains(e.target) && e.target !== $('share-status') && !$('share-status').contains(e.target)) closeStatusPanel();
}

function onPanelKey(e) {
  if (e.key === 'Escape' && st.panel) {
    e.preventDefault();
    closeStatusPanel();
    $('share-status').focus();
  }
}

function closeStatusPanel() {
  if (!st.panel) return;
  st.panel.remove();
  st.panel = null;
  document.removeEventListener('mousedown', onPanelOutside, true);
  window.removeEventListener('keydown', onPanelKey, true);
}

function line(label, value, cls = '') {
  return el('div', { class: `share-line ${cls}` }, el('span', { class: 'share-line-label', text: label }), el('span', { class: 'share-line-value', text: value, title: value }));
}

function drawPanel() {
  const panel = st.panel;
  const s = current();
  if (!panel) return;
  if (!s) {
    panel.replaceChildren(el('p', { class: 'share-muted', text: 'Looking at the shared folder…' }));
    return;
  }
  const entry = st.status.get(ctx.doc());
  const extra = Math.floor((Date.now() - entry.at) / 1000);
  const rows = [];
  rows.push(el('h3', { class: 'share-panel-title', text: 'Shared through a folder' }));
  rows.push(line('Folder', s.folder));
  rows.push(line('File', s.file));
  rows.push(line('Keeps the file', s.keeper ? 'This computer' : (s.people.find((p) => p.keeper) || {}).name || 'Another computer'));
  if (s.file_written) rows.push(line('File last written', localTime(s.file_written)));
  if (s.error) rows.push(el('p', { class: 'share-problem', text: s.error, attrs: { role: 'alert' } }));
  rows.push(line('Your changes not yet shared', String(s.unshared), s.unshared ? 'is-busy' : ''));
  if (s.waiting) rows.push(line('Changes waiting for ones that haven’t arrived', String(s.waiting), 'is-warn'));
  if (typeof s.synced_secs === 'number') rows.push(line('Folder last read', ago(s.synced_secs + extra)));
  rows.push(line('Changes in all', String(s.changes)));
  const people = el('div', { class: 'share-people-list' });
  people.append(personRow(s.me));
  for (const p of s.people) people.append(personRow(p));
  rows.push(el('h4', { class: 'share-panel-sub', text: 'People' }), people);
  const actions = el('div', { class: 'share-panel-actions' });
  if (s.review_count > 0) {
    actions.append(el('button', { class: 'btn btn-primary', text: s.review_count === 1 ? 'Review 1 item' : `Review ${s.review_count} items`, attrs: { type: 'button' }, on: { click: () => { closeStatusPanel(); ctx.showTab('review'); } } }));
  }
  actions.append(el('button', { class: 'btn', text: 'Activity', attrs: { type: 'button' }, on: { click: () => { closeStatusPanel(); ctx.showTab('activity'); } } }));
  actions.append(el('button', { class: 'btn', text: 'Share settings…', attrs: { type: 'button' }, on: { click: () => { closeStatusPanel(); openShareDialog(); } } }));
  rows.push(actions);
  panel.replaceChildren(...rows);
}

function personRow(p) {
  const when = p.me ? 'you' : typeof p.seen_secs === 'number' ? (p.away ? `away · last seen ${ago(p.seen_secs)}` : ago(p.seen_secs)) : 'not seen yet';
  return el(
    'div',
    { class: `share-person ${p.away ? 'is-away' : ''}` },
    avatar(p),
    el('span', { class: 'share-person-name', text: p.name, title: p.name }),
    el('span', { class: 'share-person-where', text: p.keeper ? `${whereText(p)} · keeps the file` : whereText(p) }),
    el('span', { class: 'share-person-when', text: when }),
  );
}

/** An ISO 8601 UTC time as the local clock shows it. */
export function localTime(iso) {
  const d = new Date(iso);
  if (!Number.isFinite(d.getTime())) return String(iso || '');
  const now = new Date();
  const time = d.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
  if (d.toDateString() === now.toDateString()) return time;
  return `${d.toLocaleDateString([], { month: 'short', day: 'numeric' })} ${time}`;
}

// ---------------------------------------------------------------------------
// Where this person is (the heartbeat), reported when it changes

export function reportPlace(view) {
  if (!ctx || !view || !view.shared) return;
  const place = ctx.place() || {};
  const text = JSON.stringify([ctx.doc(), place.category || null, place.account || null]);
  if (text === st.placeSent) return;
  st.placeSent = text;
  clearTimeout(st.placeTimer);
  st.placeTimer = setTimeout(() => {
    ctx.invoke('share_place', { doc: ctx.doc(), category: place.category || null, account: place.account || null }).catch(() => {});
  }, 300);
}

// ---------------------------------------------------------------------------
// Changes by other people: fetch the view, then flash the cells

async function remoteArrived(p) {
  if (!ctx || typeof p.doc !== 'number') return;
  if (p.doc !== ctx.doc()) return; // its tab's view is fetched when it comes to the front
  await ctx.refresh();
  const remote = Array.isArray(p.remote) ? p.remote : [];
  for (const rc of remote) flashPlaces(rc);
}

function flashPlaces(rc) {
  const grid = ctx.grid();
  if (!grid) return;
  const cls = colorClass(rc.color);
  const reduce = window.matchMedia && window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  for (const place of Array.isArray(rc.places) ? rc.places : []) {
    const at = ctx.rowKeyFor(place);
    if (!at) continue;
    const tr = grid.rowEl(at.key);
    if (!tr) continue;
    const td = at.col ? grid.cellFor(at.key, at.col) : null;
    const target = td || tr;
    if (grid.editor && td && grid.editor.td === td) continue; // never touch the cell being typed in
    target.classList.add('share-flash', cls);
    target.title = `${rc.name}: ${rc.label}`;
    if (!reduce) {
      const t = setTimeout(() => target.classList.remove('share-flash', cls), FLASH_MS);
      st.flashTimers.push(t);
    } else {
      const t = setTimeout(() => target.classList.remove('share-flash', cls), 400);
      st.flashTimers.push(t);
    }
    // A dot that stays until the row is looked at.
    tr.classList.add('share-changed', cls);
  }
}

/** The row the cursor landed on has been looked at: its dot goes. */
export function lookedAt(tr) {
  if (tr && tr.classList && tr.classList.contains('share-changed')) {
    tr.classList.remove('share-changed', ...PERSON_COLORS.map((c) => `pc-${c}`));
  }
}

// ---------------------------------------------------------------------------
// After every draw: presence next to rows, badges on conflicted cells

export function decorateShare(view) {
  if (!ctx || !view || !view.shared) return;
  const s = current();
  const grid = ctx.grid();
  if (!s || !grid) return;
  for (const n of grid.table.querySelectorAll('.share-here, .share-conflict-badge')) n.remove();
  for (const td of grid.table.querySelectorAll('.share-conflict')) td.classList.remove('share-conflict');
  // People in categories and accounts (folder mode: account level only).
  const level = ctx.level();
  for (const p of s.people) {
    if (p.away) continue;
    let at = null;
    if (level === 'topsheet' && p.category) at = ctx.rowKeyFor({ kind: 'category', key: p.category });
    else if (level === 'accounts' && p.account) at = ctx.rowKeyFor({ kind: 'account', key: p.account, parent: p.category });
    if (!at) continue;
    const td = grid.cellFor(at.key, 'name');
    if (!td || td.classList.contains('editing')) continue;
    td.append(el('span', { class: `share-here ${colorClass(p.color)}`, text: p.initials, title: personTitle(p), attrs: { 'aria-label': personTitle(p) } }));
  }
  // Values changed by two people.
  for (const c of Array.isArray(s.conflicts) ? s.conflicts : []) {
    const at = ctx.rowKeyFor(c.place);
    if (!at || !at.col) continue;
    const td = grid.cellFor(at.key, at.col);
    if (!td || td.classList.contains('editing')) continue;
    td.classList.add('share-conflict');
    const badge = el('button', {
      class: 'share-conflict-badge',
      text: '!',
      title: c.sentence,
      attrs: { type: 'button', 'aria-label': `Changed by two people: ${c.sentence}` },
      on: {
        click: (e) => {
          e.stopPropagation();
          e.preventDefault();
          openConflict(td, c);
        },
        mousedown: (e) => e.stopPropagation(),
      },
    });
    td.append(badge);
  }
}

// ---------------------------------------------------------------------------
// Conflicts (design 9.5)

function closePopover() {
  if (!st.popover) return;
  st.popover.remove();
  st.popover = null;
  document.removeEventListener('mousedown', onPopoverOutside, true);
  window.removeEventListener('keydown', onPopoverKey, true);
}

function onPopoverOutside(e) {
  if (st.popover && !st.popover.contains(e.target)) closePopover();
}

function onPopoverKey(e) {
  if (e.key === 'Escape' && st.popover) {
    e.preventDefault();
    closePopover();
    ctx.focus();
  }
}

function openConflict(td, c) {
  closePopover();
  const place = c.place || {};
  const box = el('div', { class: 'share-popover', attrs: { role: 'dialog', 'aria-label': 'Changed by two people' } });
  box.append(el('div', { class: 'share-popover-title', text: 'Changed by two people' }));
  const list = el('div', { class: 'share-values' });
  for (const v of c.values || []) {
    list.append(
      el(
        'div',
        { class: `share-value ${v.kept ? 'is-kept' : ''}` },
        el('span', { class: `share-avatar share-avatar-sm ${colorClass(v.color)}`, text: initialsOf(v.name) }),
        el('span', { class: 'share-value-who', text: v.me ? 'You' : v.name }),
        el('span', { class: 'share-value-text', text: v.value || '(blank)', title: v.value }),
        el('span', { class: 'share-value-when', text: `${localTime(v.time)}${v.kept ? ' · kept' : ''}` }),
      ),
    );
  }
  box.append(list);
  const actions = el('div', { class: 'share-popover-actions' });
  for (const v of c.values || []) {
    actions.append(
      el('button', {
        class: v.kept ? 'btn btn-primary' : 'btn',
        text: v.kept ? `Keep ${short(v.value)}` : `Use ${short(v.value)}`,
        attrs: { type: 'button' },
        on: { click: () => useValue(place, v.index) },
      }),
    );
  }
  actions.append(el('button', { class: 'btn btn-quiet', text: 'History…', attrs: { type: 'button' }, on: { click: () => { closePopover(); whoChangedThis(place); } } }));
  box.append(actions);
  document.body.append(box);
  st.popover = box;
  const r = td.getBoundingClientRect();
  const width = Math.min(380, window.innerWidth - 16);
  const left = Math.max(8, Math.min(Math.round(r.left), window.innerWidth - width - 8));
  setRules('share-popover', `.share-popover.placed { left: ${left}px; top: ${Math.round(r.bottom + 4)}px; }`);
  box.classList.add('placed');
  setTimeout(() => {
    document.addEventListener('mousedown', onPopoverOutside, true);
    window.addEventListener('keydown', onPopoverKey, true);
  }, 0);
  const first = box.querySelector('button');
  if (first) first.focus();
}

function short(v) {
  const s = String(v === undefined || v === null || v === '' ? '(blank)' : v);
  return s.length > 24 ? `${s.slice(0, 23)}…` : s;
}

function initialsOf(name) {
  const words = String(name || '').split(/\s+/).filter(Boolean).slice(0, 2);
  const s = words.map((w) => w[0].toUpperCase()).join('');
  return s || '?';
}

async function useValue(place, index) {
  closePopover();
  try {
    const v = await ctx.invoke('share_use_value', { doc: ctx.doc(), kind: place.kind, key: place.key, field: place.field, index });
    if (v) ctx.apply(v, { cmd: 'share_use_value' });
  } catch (e) {
    ctx.toast(e);
  }
  ctx.focus();
}

// ---------------------------------------------------------------------------
// Who changed this (design 9.6)

/**
 * Shows every value a field has had and who typed it. `place` is
 * { kind, key, field, text } (ctx.rowKeyFor's inverse: main.js works it
 * out from the cell right-clicked).
 */
export async function whoChangedThis(place) {
  if (!ctx || !place || !place.field) return;
  let history;
  try {
    history = await ctx.invoke('share_blame', { doc: ctx.doc(), kind: place.kind, key: place.key, field: place.field });
  } catch (e) {
    ctx.toast(e);
    return;
  }
  const title = place.text ? `${fieldName(place.field)} on ${place.text}` : fieldName(place.field);
  customDialog(
    (close) => {
      const list = el('div', { class: 'share-values share-values-dialog' });
      if (!history.length) list.append(el('p', { class: 'share-muted', text: 'Nobody has changed this since the budget was shared.' }));
      for (const v of [...history].reverse()) {
        const row = el(
          'div',
          { class: `share-value ${v.kept ? 'is-kept' : ''}` },
          el('span', { class: 'share-value-text', text: v.value || '(blank)', title: v.value }),
          el('span', { class: `share-avatar share-avatar-sm ${colorClass(v.color)}`, text: initialsOf(v.name) }),
          el('span', { class: 'share-value-who', text: v.me ? 'You' : v.name }),
          el('span', { class: 'share-value-when', text: localTime(v.time) }),
          el('span', { class: 'share-value-label', text: v.label, title: v.label }),
        );
        if (v.kept) row.append(el('span', { class: 'share-value-kept', text: 'now' }));
        else {
          row.append(
            el('button', {
              class: 'btn btn-quiet',
              text: 'Put back',
              attrs: { type: 'button' },
              on: {
                click: () => {
                  close();
                  useValue(place, v.index);
                },
              },
            }),
          );
        }
        list.append(row);
      }
      const done = el('button', { class: 'btn btn-primary', text: 'Close', attrs: { type: 'button', 'data-autofocus': true }, on: { click: () => close() } });
      return el(
        'div',
        { class: 'modal modal-wide share-dialog', attrs: { role: 'dialog', 'aria-modal': 'true', 'aria-labelledby': 'share-blame-title' } },
        el('h2', { class: 'modal-title', text: `Who changed ${title}`, attrs: { id: 'share-blame-title' } }),
        el('p', { class: 'modal-message', text: 'Every value this cell has had since the budget was shared, newest first. Putting one back is a change like any other.' }),
        list,
        el('div', { class: 'modal-actions' }, done),
      );
    },
    { onClose: () => ctx.focus() },
  );
}

function fieldName(field) {
  const names = { amount: 'amount', x: 'X', rate: 'rate', x4: '4X', unit: 'unit', unit2: 'X unit', unit3: 'rate unit', unit4: '4X unit', description: 'description', currency: 'currency', fringes: 'fringes', groups: 'groups', set: 'set', location: 'location', name: 'name', code: 'number' };
  return names[field] || field;
}

// ---------------------------------------------------------------------------
// File > Share…

export async function openShareDialog() {
  if (!ctx || !ctx.view() || isModalOpen()) return;
  let setup;
  try {
    setup = await ctx.invoke('share_setup', { doc: ctx.doc() });
  } catch (e) {
    ctx.toast(e);
    return;
  }
  const view = ctx.view();
  const title = (view.budget && view.budget.title) || 'Untitled budget';
  if (view.shared) sharedDialog(title, setup);
  else startDialog(title, setup);
}

function colorPicker(chosen, onPick) {
  const host = el('div', { class: 'share-colors', attrs: { role: 'radiogroup', 'aria-label': 'Your color' } });
  let current = chosen;
  const buttons = [];
  for (const c of PERSON_COLORS) {
    const b = el('button', {
      class: `share-color ${colorClass(c)}`,
      title: c,
      attrs: { type: 'button', role: 'radio', 'aria-label': c, 'aria-checked': String(c === current) },
      on: {
        click: () => {
          current = c;
          for (const x of buttons) x.setAttribute('aria-checked', String(x.dataset.color === c));
          onPick(c);
        },
      },
      data: { color: c },
    });
    buttons.push(b);
    host.append(b);
  }
  return host;
}

function startDialog(title, setup) {
  customDialog(
    (close) => {
      const me = { name: setup.name || '', color: setup.color || 'teal' };
      const error = el('p', { class: 'modal-error', attrs: { role: 'alert' } });
      error.hidden = true;
      const name = el('input', { class: 'modal-input', attrs: { type: 'text', id: 'share-name', maxlength: '60', placeholder: 'Your name, as others see it' } });
      name.value = me.name;
      const colors = colorPicker(me.color, (c) => { me.color = c; });
      // Where: the budget's own folder, or one chosen in Rust's dialog.
      const where = { picked: setup.picked || null, move: true };
      const whereText = el('span', { class: 'share-where-text' });
      const choose = el('button', { class: 'btn', text: 'Choose another folder…', attrs: { type: 'button' } });
      const moveCopy = el('div', { class: 'share-movecopy' });
      const drawWhere = () => {
        whereText.textContent = where.picked ? where.picked : setup.folder || '';
        moveCopy.hidden = !where.picked || where.picked === setup.folder;
        moveCopy.replaceChildren(
          radio('share-mc', 'Move the budget there', where.move, () => { where.move = true; }),
          radio('share-mc', 'Copy it there (this file stays as it is, not shared)', !where.move, () => { where.move = false; }),
        );
      };
      choose.addEventListener('click', async () => {
        try {
          const p = await ctx.invoke('share_pick_folder', { doc: ctx.doc() });
          if (p) where.picked = p;
          drawWhere();
        } catch (e) {
          error.textContent = String(e);
          error.hidden = false;
        }
      });
      drawWhere();
      const cancel = el('button', { class: 'btn', text: 'Cancel', attrs: { type: 'button' } });
      const ok = el('button', { class: 'btn btn-primary', text: 'Start Sharing', attrs: { type: 'button' } });
      cancel.addEventListener('click', () => close());
      ok.addEventListener('click', async () => {
        error.hidden = true;
        const usePicked = Boolean(where.picked && where.picked !== setup.folder);
        if (usePicked) {
          const what = where.move ? `Move ${setup.file} to ${where.picked}?` : `Copy ${setup.file} to ${where.picked}?`;
          const yes = await confirmDialog({
            title: where.move ? 'Move the budget' : 'Copy the budget',
            message: `${what} The budget will be shared from there.${where.move ? ' The file is taken out of its current folder.' : ' Changes made here from now on go to the copy in the shared folder.'}`,
            confirmText: where.move ? 'Move and Share' : 'Copy and Share',
          });
          if (!yes) return;
        }
        ok.disabled = true;
        try {
          const v = await ctx.invoke('share_start', {
            doc: ctx.doc(),
            use_picked: usePicked,
            move_file: where.move,
            me: { name: name.value, color: me.color },
          });
          close();
          if (v) ctx.apply(v, { cmd: 'share_start' });
          firstRunNotice();
          fetchStatus();
          return;
        } catch (e) {
          error.textContent = String(e);
          error.hidden = false;
        }
        ok.disabled = false;
      });
      const blocked = setup.blocked ? el('p', { class: 'modal-error', text: setup.blocked, attrs: { role: 'alert' } }) : null;
      if (blocked) ok.disabled = true;
      return el(
        'div',
        { class: 'modal modal-wide share-dialog', attrs: { role: 'dialog', 'aria-modal': 'true', 'aria-labelledby': 'share-title' } },
        el('h2', { class: 'modal-title', text: `Share “${title}”`, attrs: { id: 'share-title' } }),
        el('p', { class: 'modal-message', text: 'How do people get each other’s changes?' }),
        el(
          'div',
          { class: 'share-mode' },
          radio('share-mode', 'Through a shared folder', true, () => {}),
          el('p', { class: 'share-mode-text', text: 'A folder you already share through a sync service. Works offline; changes arrive in a few seconds. Everyone who opens the budget from that folder works in the same budget.' }),
          el('div', { class: 'share-where' }, el('span', { class: 'share-line-label', text: 'Folder' }), whereText, choose),
          moveCopy,
        ),
        el('div', { class: 'share-mode is-later' }, radio('share-mode', 'Live link only', false, () => {}, true), el('p', { class: 'share-mode-text', text: 'Changes arrive instantly while you’re both online, with no shared folder. Coming later.' })),
        el('div', { class: 'share-me' }, el('label', { class: 'modal-field', attrs: { for: 'share-name' } }, el('span', { text: 'Your name' }), name), el('div', { class: 'modal-field' }, el('span', { text: 'Your color' }), colors)),
        blocked,
        error,
        el('div', { class: 'modal-actions' }, cancel, ok),
      );
    },
    { onClose: () => ctx.focus() },
  );
}

function radio(group, label, checked, onPick, disabled = false) {
  const input = el('input', { attrs: { type: 'radio', name: group } });
  input.checked = checked;
  input.disabled = disabled;
  input.addEventListener('change', () => { if (input.checked) onPick(); });
  return el('label', { class: `share-radio ${disabled ? 'is-disabled' : ''}` }, input, el('span', { text: label }));
}

function sharedDialog(title, setup) {
  const s = current() || { people: [], me: null, folder: setup.folder, file: setup.file };
  customDialog(
    (close) => {
      const me = { name: setup.name || '', color: setup.color || 'teal' };
      const error = el('p', { class: 'modal-error', attrs: { role: 'alert' } });
      error.hidden = true;
      const name = el('input', { class: 'modal-input', attrs: { type: 'text', id: 'share-name', maxlength: '60' } });
      name.value = me.name;
      const colors = colorPicker(me.color, (c) => { me.color = c; });
      const people = el('div', { class: 'share-people-list share-people-dialog' });
      if (s.me) people.append(personRow({ ...s.me, name: `${s.me.name} (you)` }));
      for (const p of s.people) people.append(personRow(p));
      if (!s.people.length) people.append(el('p', { class: 'share-muted', text: 'Nobody else has opened this budget from the shared folder yet. When they do, they join by opening it; their name and color show here.' }));
      const save = el('button', { class: 'btn', text: 'Save Name and Color', attrs: { type: 'button' } });
      save.addEventListener('click', async () => {
        error.hidden = true;
        try {
          await ctx.invoke('share_set_me', { name: name.value, color: me.color });
          ctx.toast('Saved. Other people see the new name and color within a minute.');
          fetchStatus();
        } catch (e) {
          error.textContent = String(e);
          error.hidden = false;
        }
      });
      const done = el('button', { class: 'btn btn-primary', text: 'Done', attrs: { type: 'button', 'data-autofocus': true } });
      done.addEventListener('click', () => close());
      return el(
        'div',
        { class: 'modal modal-wide share-dialog', attrs: { role: 'dialog', 'aria-modal': 'true', 'aria-labelledby': 'share-title' } },
        el('h2', { class: 'modal-title', text: `Share “${title}”`, attrs: { id: 'share-title' } }),
        el('p', { class: 'modal-message', text: 'This budget is shared through a folder. Everyone who opens it from there works in the same budget; changes arrive in a few seconds while they are online, or the next time they open it.' }),
        line('Folder', s.folder || setup.folder || ''),
        line('File', s.file || setup.file || ''),
        el('h4', { class: 'share-panel-sub', text: 'People' }),
        people,
        el('p', { class: 'share-muted', text: 'To invite someone, give them access to the folder; opening the budget there joins them. Roles and invite links come later: for now everyone who can open the file can edit it.' }),
        el('div', { class: 'share-me' }, el('label', { class: 'modal-field', attrs: { for: 'share-name' } }, el('span', { text: 'Your name' }), name), el('div', { class: 'modal-field' }, el('span', { text: 'Your color' }), colors), save),
        error,
        el('div', { class: 'modal-actions' }, done),
      );
    },
    { onClose: () => ctx.focus() },
  );
}

/** Once per budget: what sharing means for saving (design 9.2). */
export function firstRunNotice() {
  if (!ctx || !ctx.view() || !ctx.view().shared) return;
  const v = ctx.view();
  const key = `shareNoticed:${(v.file && v.file.path) || v.doc}`;
  if (readPref(key, false)) return;
  writePref(key, true);
  if (isModalOpen()) return;
  customDialog(
    (close) => {
      const ok = el('button', { class: 'btn btn-primary', text: 'Got it', attrs: { type: 'button', 'data-autofocus': true }, on: { click: () => close() } });
      return el(
        'div',
        { class: 'modal share-dialog', attrs: { role: 'dialog', 'aria-modal': 'true', 'aria-labelledby': 'share-notice-title' } },
        el('h2', { class: 'modal-title', text: 'This budget is shared through a folder', attrs: { id: 'share-notice-title' } }),
        el('p', { class: 'modal-message', text: 'Your changes are kept on this computer and shared through the folder. Others see them in a few seconds when they’re online, or the next time they open the budget. You can keep working offline.' }),
        el('p', { class: 'modal-message', text: 'There’s nothing to save: everything you type is kept. Undo takes back your own changes only.' }),
        el('div', { class: 'modal-actions' }, ok),
      );
    },
    { onClose: () => ctx.focus() },
  );
}

/** The budget on screen changed (a tab switch): the first-run notice for a budget joined on opening. */
export function shareShown(view) {
  if (!view || !view.shared) return;
  firstRunNotice();
}
