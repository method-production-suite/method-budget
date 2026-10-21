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
// - Roles (decisions.md, entry 78): the Share dialog lists what each person
//   may change; the owner and admins set it there. A chip in the toolbar
//   says when this person may change less than everything. Rust refuses
//   an edit beyond the role with a sentence the page shows as a toast.
// - Muted cells and access requests (design 9.7): a department editor's
//   categories and accounts outside their scope are muted with a lock
//   (shareLocks(), used by main.js on the rows it draws); typing in one
//   says so and offers to ask for access (showLocked()). A request goes
//   into this computer's heartbeat file in the folder, nothing is sent
//   anywhere; whoever may give that role sees it in the status panel and
//   as a toolbar badge, with Allow, Allow those categories only and
//   Decline.
//
// Everything is drawn with el() and textContent; names and labels come
// from other people's files and are never read as HTML. No inline styles
// (the CSP): colors are classes from the palette below.

import { el, $ } from './dom.js';
import { customDialog, confirmDialog, isModalOpen } from './modal.js';
import { readPref, writePref } from './prefs.js';
import { setRules } from './sheet.js';
import { t } from './i18n.js';

/** The person colors Rust names (src/shared.rs COLORS); CSS has one class per name. */
export const PERSON_COLORS = ['teal', 'blue', 'purple', 'orange', 'rose', 'green', 'amber', 'slate'];

function colorClass(color) {
  return PERSON_COLORS.includes(color) ? `pc-${color}` : 'pc-slate';
}

// { view(), doc(), invoke, apply(view, opts), toast(message, action), refresh() -> Promise,
//   render() (draws the grid again: the locks changed),
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
  locksSeen: new Map(), // doc -> the locked categories and accounts last drawn, as JSON
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
  const locks = JSON.stringify([p.status.locked_categories || [], p.status.locked_accounts || []]);
  const locksChanged = st.locksSeen.has(p.doc) ? st.locksSeen.get(p.doc) !== locks : locks !== '[[],[]]';
  st.locksSeen.set(p.doc, locks);
  if (ctx && p.doc === ctx.doc() && ctx.view()) {
    if (locksChanged && typeof ctx.render === 'function') ctx.render();
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
    setStatusText(t('share.shared_through_a_folder'), 'quiet');
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
  } else if (s.replaced) {
    text = t('share.status.replaced');
    tone = 'bad';
  } else if (s.key_warning) {
    text = t('share.status.key_warning');
    tone = 'bad';
  } else if (s.owner_key_warning) {
    text = t('share.status.owner_key_warning');
    tone = 'bad';
  } else if (s.unshared > 0) {
    text = t('share.status.sharing', { count: s.unshared });
    tone = 'busy';
  } else if (s.waiting > 0) {
    text = t('share.status.waiting', { count: s.waiting });
    tone = 'warn';
  } else if (synced !== null) {
    text = t('share.status.synced', { ago: ago(synced) });
  } else {
    text = t('share.shared_through_a_folder');
  }
  setStatusText(text, tone, s.review_count, (s.requests || []).length);
  renderPeople(s);
}

function setStatusText(text, tone, review = 0, requests = 0) {
  const b = $('share-status');
  const label = $('share-status-text');
  if (!b || !label) return;
  label.textContent = text;
  b.dataset.tone = tone;
  b.title = t('share.status_tip', { text });
  let badge = b.querySelector('.share-review-count');
  if (review > 0) {
    if (!badge) {
      badge = el('span', { class: 'share-review-count' });
      b.append(badge);
    }
    badge.textContent = t('share.to_review', { count: review });
  } else if (badge) {
    badge.remove();
  }
  let asks = b.querySelector('.share-request-count');
  if (requests > 0) {
    if (!asks) {
      asks = el('span', { class: 'share-request-count' });
      b.append(asks);
    }
    asks.textContent = t('share.requests.badge', { count: requests });
  } else if (asks) {
    asks.remove();
  }
}

/** "12 s ago", "3 min ago", "2 h ago". */
export function ago(secs) {
  if (secs < 60) return t('share.ago.secs', { count: Math.max(0, Math.round(secs)) });
  if (secs < 3600) return t('share.ago.mins', { count: Math.round(secs / 60) });
  if (secs < 86400) return t('share.ago.hours', { count: Math.round(secs / 3600) });
  return t('share.ago.days', { count: Math.round(secs / 86400) });
}

function whereText(p) {
  if (p.account) return t('share.where.in', { name: p.account });
  if (p.category) return t('share.where.in', { name: p.category });
  return t('share.where.topsheet');
}

function personTitle(p) {
  const when = p.me ? t('share.when.you') : typeof p.seen_secs === 'number' ? (p.away ? t('share.when.away_last_seen', { ago: ago(p.seen_secs) }) : ago(p.seen_secs)) : t('share.when.not_seen');
  return t('share.person_title', { name: p.name, where: whereText(p), when });
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
    nodes.push(el('span', { class: 'share-avatar share-more', text: `+${s.people.length - shown.length}`, title: t('share.people_opened', { count: s.people.length }) }));
  }
  if (!s.people.length) nodes.push(el('span', { class: 'share-alone', text: t('share.only_you'), title: t('share.only_you_tip') }));
  const chip = roleChip(s.me);
  if (chip) nodes.unshift(chip);
  host.replaceChildren(...nodes);
}

/** Roles that change everything need no chip; the others say what this person may change. */
const FULL_ROLES = ['owner', 'co_owner', 'admin', 'editor'];

const CHIP_TEXT = () => ({ department: t('share.chip.department'), actuals: t('share.chip.actuals'), comment: t('share.chip.comment'), read_only: t('share.chip.read_only') });

function roleChip(me) {
  if (!me || !me.role || FULL_ROLES.includes(me.role)) return null;
  return el('span', {
    class: `share-role-chip ${me.role === 'read_only' ? 'is-read-only' : ''}`,
    text: CHIP_TEXT()[me.role] || t('share.chip.limited'),
    title: t('share.chip_tip', { role: me.role_text }),
    attrs: { 'aria-label': t('share.chip_tip', { role: me.role_text }) },
  });
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
  const panel = el('div', { class: 'share-panel', attrs: { role: 'dialog', 'aria-label': t('share.panel.label') } });
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
    panel.replaceChildren(el('p', { class: 'share-muted', text: t('share.panel.looking') }));
    return;
  }
  const entry = st.status.get(ctx.doc());
  const extra = Math.floor((Date.now() - entry.at) / 1000);
  const rows = [];
  rows.push(el('h3', { class: 'share-panel-title', text: t('share.shared_through_a_folder') }));
  rows.push(line(t('share.folder'), s.folder));
  rows.push(line(t('share.file'), s.file));
  rows.push(line(t('share.panel.keeps_file'), s.keeper ? t('share.panel.this_computer') : (s.people.find((p) => p.keeper) || {}).name || t('share.panel.another_computer')));
  if (s.file_written) rows.push(line(t('share.panel.file_written'), localTime(s.file_written)));
  if (s.error) rows.push(el('p', { class: 'share-problem', text: s.error, attrs: { role: 'alert' } }));
  if (s.key_warning) rows.push(el('p', { class: 'share-problem', text: s.key_warning, attrs: { role: 'alert' } }));
  const keyRows = ownerKeyRows(s);
  if (keyRows) rows.push(...keyRows);
  if (s.notice) rows.push(el('p', { class: 'share-notice', text: s.notice, attrs: { role: 'status' } }));
  rows.push(line(t('share.panel.unshared'), String(s.unshared), s.unshared ? 'is-busy' : ''));
  if (s.waiting) rows.push(line(t('share.panel.waiting'), String(s.waiting), 'is-warn'));
  if (typeof s.synced_secs === 'number') rows.push(line(t('share.panel.folder_read'), ago(s.synced_secs + extra)));
  rows.push(line(t('share.panel.changes'), String(s.changes)));
  const people = el('div', { class: 'share-people-list' });
  people.append(personRow(s.me, s));
  for (const p of s.people) people.append(personRow(p, s));
  rows.push(el('h4', { class: 'share-panel-sub', text: t('share.people') }), people);
  const asked = requestRows(s);
  if (asked) rows.push(...asked);
  const refused = refusedList(s);
  if (refused) rows.push(...refused);
  const actions = el('div', { class: 'share-panel-actions' });
  if (s.review_count > 0) {
    actions.append(el('button', { class: 'btn btn-primary', text: t('share.panel.review', { count: s.review_count }), attrs: { type: 'button' }, on: { click: () => { closeStatusPanel(); ctx.showTab('review'); } } }));
  }
  actions.append(el('button', { class: 'btn', text: t('share.panel.activity'), attrs: { type: 'button' }, on: { click: () => { closeStatusPanel(); ctx.showTab('activity'); } } }));
  actions.append(el('button', { class: 'btn', text: t('share.panel.settings'), attrs: { type: 'button' }, on: { click: () => { closeStatusPanel(); openShareDialog(); } } }));
  rows.push(actions);
  panel.replaceChildren(...rows);
}

/**
 * A key in the owner's name (docs/research/owner-key.md): Start Again on the
 * owner's computer, Reopen once the owner started the budget again, and
 * Trust the New Key when the owner's key isn't the one this computer saw.
 * Each asks first.
 */
function ownerKeyRows(s) {
  const out = [];
  const confirm = el('div', { class: 'share-first-role', attrs: { role: 'group' } });
  confirm.hidden = true;
  const error = el('p', { class: 'share-problem', attrs: { role: 'alert' } });
  error.hidden = true;
  const ask = (button, label, message, goText, run) => {
    button.addEventListener('click', () => {
      error.hidden = true;
      const go = el('button', { class: 'btn btn-primary', text: goText, attrs: { type: 'button' } });
      const keep = el('button', { class: 'btn', text: t('share.cancel'), attrs: { type: 'button' } });
      keep.addEventListener('click', () => { confirm.hidden = true; button.focus(); });
      go.addEventListener('click', async () => {
        go.disabled = true;
        try {
          await run();
          confirm.hidden = true;
        } catch (e) {
          error.textContent = String(e);
          error.hidden = false;
        }
        go.disabled = false;
      });
      confirm.setAttribute('aria-label', label);
      confirm.replaceChildren(el('p', { class: 'modal-message', text: message }), el('div', { class: 'modal-actions' }, keep, go));
      confirm.hidden = false;
      go.focus();
    });
  };
  const reopen = async (trust) => {
    const v = await ctx.invoke('share_reopen', { doc: ctx.doc(), trust });
    if (v) ctx.apply(v, { cmd: 'share_reopen' });
    fetchStatus();
  };
  if (s.replaced) {
    out.push(el('p', { class: 'share-problem', text: s.replaced, attrs: { role: 'alert' } }));
    const lost = s.not_in_new || [];
    if (lost.length) {
      out.push(el('h4', { class: 'share-panel-sub', text: t('share.not_in_new', { count: lost.length }) }));
      out.push(el('p', { class: 'share-muted', text: t('share.not_in_new_note') }));
      out.push(el('ul', { class: 'share-refused' }, ...lost.map((label) => el('li', { text: label }))));
    }
    const b = el('button', { class: 'btn btn-primary', text: t('share.reopen'), attrs: { type: 'button' } });
    b.addEventListener('click', async () => {
      b.disabled = true;
      try {
        await reopen(false);
      } catch (e) {
        error.textContent = String(e);
        error.hidden = false;
      }
      b.disabled = false;
    });
    out.push(el('div', { class: 'share-panel-actions' }, b));
  }
  if (s.can_start_again) {
    const b = el('button', { class: 'btn btn-primary', text: t('share.start_again_button'), attrs: { type: 'button' } });
    ask(b, t('share.start_again_label'), START_AGAIN_NOTE(), t('share.start_again'), async () => {
      const status = await ctx.invoke('share_start_again', { doc: ctx.doc() });
      if (status) statusArrived({ doc: ctx.doc(), status });
      ctx.toast(t('share.started_again'));
    });
    out.push(el('div', { class: 'share-panel-actions' }, b));
  }
  if (s.owner_key_warning) {
    out.push(el('p', { class: 'share-problem', text: s.owner_key_warning, attrs: { role: 'alert' } }));
    const b = el('button', { class: 'btn', text: t('share.trust_button'), attrs: { type: 'button' } });
    ask(b, t('share.trust_label'), TRUST_NOTE(), t('share.trust'), () => reopen(true));
    out.push(el('div', { class: 'share-panel-actions' }, b));
  }
  if (!out.length) return null;
  out.push(confirm, error);
  return out;
}

const START_AGAIN_NOTE = () => t('share.start_again_note');

// ---------------------------------------------------------------------------
// Muted cells and access requests (design 9.7; docs/APP.md, "Asking for access")

/**
 * What this person may not change, for the rows main.js draws: the
 * categories and accounts outside a department editor's scope, or null
 * when nothing is locked (every other role; a read-only person keeps the
 * read-only state). { categories: Set, accounts: Set, tip }
 */
export function shareLocks() {
  const s = current();
  if (!s || !ctx || !ctx.view() || !ctx.view().shared) return null;
  const cats = s.locked_categories || [];
  const accounts = s.locked_accounts || [];
  if (!cats.length && !accounts.length) return null;
  return { categories: new Set(cats), accounts: new Set(accounts), tip: t('share.locked.tip') };
}

/** Typing in a locked cell: says why, and offers to ask for access to it. */
export function showLocked(locked) {
  if (!ctx || !locked) return;
  const s = current();
  const what = [locked.code, locked.name].filter(Boolean).join(' ');
  const message = t('share.locked.message', { what });
  if (!s || !s.can_ask) {
    ctx.toast(message);
    return;
  }
  const mine = s.my_request;
  if (mine && (mine.codes || []).includes(locked.code)) {
    ctx.toast(`${message} ${t('share.locked.asked', { name: s.ask_name })}`);
    return;
  }
  ctx.toast(message, { label: t('share.ask_for_access', { name: s.ask_name }), run: () => askForAccess([locked.code], '') });
}

/** Writes this person's request into their heartbeat file (Rust checks it). */
async function askForAccess(codes, note) {
  const doc = ctx.doc();
  try {
    const status = await ctx.invoke('share_request_access', { doc, role: 'department', codes, note });
    if (status) statusArrived({ doc, status });
    ctx.toast(t('share.requests.sent', { name: status ? status.ask_name : '' }));
  } catch (e) {
    ctx.toast(String(e));
  }
}

/**
 * The status panel's part about access: requests this person could allow,
 * with Allow, Allow those only and Decline; or this person's own request,
 * with Withdraw; or a way to ask.
 */
function requestRows(s) {
  const out = [];
  const error = el('p', { class: 'share-problem', attrs: { role: 'alert' } });
  error.hidden = true;
  const doc = ctx.doc();
  const run = async (button, cmd, args, done) => {
    button.disabled = true;
    error.hidden = true;
    try {
      const status = await ctx.invoke(cmd, { doc, ...args });
      if (status) statusArrived({ doc, status });
      if (done) ctx.toast(done);
    } catch (e) {
      error.textContent = String(e);
      error.hidden = false;
      button.disabled = false;
    }
  };
  const list = s.requests || [];
  if (list.length) {
    out.push(el('h4', { class: 'share-panel-sub', text: t('share.requests.title', { count: list.length }) }));
    for (const r of list) {
      const what = r.codes_text ? t('share.requests.asks_about', { name: r.name, codes: r.codes_text }) : t('share.requests.asks', { name: r.name });
      const allow = el('button', { class: 'btn btn-primary', text: t('share.requests.allow'), title: t('share.requests.allow_tip', { name: r.name, role: r.role_text }), attrs: { type: 'button' } });
      allow.addEventListener('click', () => run(allow, 'share_allow_request', { actor: r.actor, at: r.at, only: false }, t('share.role_saved', { name: r.name })));
      const only = r.only ? el('button', { class: 'btn', text: t('share.requests.allow_only', { codes: r.codes_text }), attrs: { type: 'button' } }) : null;
      if (only) only.addEventListener('click', () => run(only, 'share_allow_request', { actor: r.actor, at: r.at, only: true }, t('share.role_saved', { name: r.name })));
      const decline = el('button', { class: 'btn', text: t('share.requests.decline'), title: t('share.requests.decline_tip'), attrs: { type: 'button' } });
      decline.addEventListener('click', () => run(decline, 'share_decline_request', { actor: r.actor, at: r.at }, null));
      out.push(el(
        'div',
        { class: 'share-request', attrs: { role: 'group', 'aria-label': what } },
        el('div', {}, el('span', { class: `share-avatar ${colorClass(r.color)}`, text: initialsOf(r.name), attrs: { 'aria-hidden': 'true' } }), el('span', { class: 'share-person-name', text: what })),
        el('div', { class: 'share-muted', text: t('share.requests.gives', { role: r.role_text, time: localTime(r.at) }) }),
        r.note ? el('div', { class: 'share-request-note', text: r.note }) : null,
        el('div', { class: 'share-panel-actions' }, allow, only, decline),
      ));
    }
  }
  if (s.my_request) {
    const r = s.my_request;
    const withdraw = el('button', { class: 'btn', text: t('share.requests.withdraw'), attrs: { type: 'button' } });
    withdraw.addEventListener('click', () => run(withdraw, 'share_withdraw_request', {}, t('share.requests.withdrawn')));
    out.push(
      el('h4', { class: 'share-panel-sub', text: t('share.requests.yours') }),
      el('p', { class: 'share-muted', text: r.codes_text ? t('share.requests.yours_text_about', { name: s.ask_name, time: localTime(r.at), codes: r.codes_text }) : t('share.requests.yours_text', { name: s.ask_name, time: localTime(r.at) }) }),
      el('div', { class: 'share-panel-actions' }, withdraw),
    );
  } else if (s.can_ask) {
    const place = (ctx.place && ctx.place()) || {};
    const code = place.account || place.category || null;
    const note = el('input', { class: 'share-request-input', attrs: { type: 'text', maxlength: '200', placeholder: t('share.requests.note_placeholder'), 'aria-label': t('share.requests.note_label') } });
    const ask = el('button', { class: 'btn', text: t('share.ask_for_access', { name: s.ask_name }), attrs: { type: 'button' } });
    ask.addEventListener('click', () => run(ask, 'share_request_access', { role: 'editor', codes: code ? [code] : [], note: note.value }, t('share.requests.sent', { name: s.ask_name })));
    out.push(
      el('h4', { class: 'share-panel-sub', text: t('share.requests.ask_title') }),
      el('p', { class: 'share-muted', text: t('share.requests.ask_note', { name: s.ask_name }) }),
      note,
      el('div', { class: 'share-panel-actions' }, ask),
    );
  }
  if (!out.length) return null;
  out.push(error);
  return out;
}

const TRUST_NOTE = () => t('share.trust_note');

/** Changes left out on this computer: who, when, what and why (decisions.md, entry 78). */
function refusedList(s) {
  const list = s.refused_changes || [];
  if (!list.length) return null;
  const title = t('share.left_out', { count: s.refused });
  const items = el('ul', { class: 'share-refused' });
  for (const r of [...list].reverse()) {
    items.append(el(
      'li',
      {},
      el('span', { class: 'share-person-name', text: r.name }),
      el('span', { class: 'share-person-when', text: ` · ${localTime(r.time)} · ` }),
      el('span', { text: r.label }),
      el('div', { class: 'share-muted', text: r.why }),
    ));
  }
  return [el('h4', { class: 'share-panel-sub', text: title }), items];
}

/** Whether a person's changes are signed, in words (decisions.md, entry 89). */
function signText(p, s) {
  if (!s || !s.signed) return '';
  return p.signs ? t('share.signs') : t('share.no_key_yet');
}

function personRow(p, s) {
  const when = p.me ? t('share.when.you') : typeof p.seen_secs === 'number' ? (p.away ? t('share.when.away_last_seen_dot', { ago: ago(p.seen_secs) }) : ago(p.seen_secs)) : t('share.when.not_seen');
  const signed = signText(p, s);
  return el(
    'div',
    { class: `share-person ${p.away ? 'is-away' : ''}` },
    avatar(p),
    el('span', { class: 'share-person-name', text: p.name, title: p.name }),
    el('span', { class: 'share-person-where', text: p.keeper ? t('share.where_keeps_file', { where: whereText(p) }) : whereText(p) }),
    el('span', { class: 'share-person-when', text: when }),
    p.role_text ? el('span', { class: 'share-person-role', text: p.role_text, title: p.role_text }) : null,
    signed ? el('span', { class: 'share-person-role share-person-signed', text: signed, title: signed }) : null,
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
      const timer = setTimeout(() => target.classList.remove('share-flash', cls), FLASH_MS);
      st.flashTimers.push(timer);
    } else {
      const timer = setTimeout(() => target.classList.remove('share-flash', cls), 400);
      st.flashTimers.push(timer);
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
      attrs: { type: 'button', 'aria-label': t('share.conflict_label', { sentence: c.sentence }) },
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
  const box = el('div', { class: 'share-popover', attrs: { role: 'dialog', 'aria-label': t('share.conflict_title') } });
  box.append(el('div', { class: 'share-popover-title', text: t('share.conflict_title') }));
  const list = el('div', { class: 'share-values' });
  for (const v of c.values || []) {
    list.append(
      el(
        'div',
        { class: `share-value ${v.kept ? 'is-kept' : ''}` },
        el('span', { class: `share-avatar share-avatar-sm ${colorClass(v.color)}`, text: initialsOf(v.name) }),
        el('span', { class: 'share-value-who', text: v.me ? t('share.you') : v.name }),
        el('span', { class: 'share-value-text', text: v.value || t('share.blank'), title: v.value }),
        el('span', { class: 'share-value-when', text: v.kept ? t('share.time_kept', { time: localTime(v.time) }) : localTime(v.time) }),
      ),
    );
  }
  box.append(list);
  const actions = el('div', { class: 'share-popover-actions' });
  for (const v of c.values || []) {
    actions.append(
      el('button', {
        class: v.kept ? 'btn btn-primary' : 'btn',
        text: v.kept ? t('share.keep_value', { value: short(v.value) }) : t('share.use_value', { value: short(v.value) }),
        attrs: { type: 'button' },
        on: { click: () => useValue(place, v.index) },
      }),
    );
  }
  actions.append(el('button', { class: 'btn btn-quiet', text: t('share.history'), attrs: { type: 'button' }, on: { click: () => { closePopover(); whoChangedThis(place); } } }));
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
  const s = String(v === undefined || v === null || v === '' ? t('share.blank') : v);
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
  const title = place.text ? t('share.field_on', { field: fieldName(place.field), place: place.text }) : fieldName(place.field);
  customDialog(
    (close) => {
      const list = el('div', { class: 'share-values share-values-dialog' });
      if (!history.length) list.append(el('p', { class: 'share-muted', text: t('share.blame.none') }));
      for (const v of [...history].reverse()) {
        const row = el(
          'div',
          { class: `share-value ${v.kept ? 'is-kept' : ''}` },
          el('span', { class: 'share-value-text', text: v.value || t('share.blank'), title: v.value }),
          el('span', { class: `share-avatar share-avatar-sm ${colorClass(v.color)}`, text: initialsOf(v.name) }),
          el('span', { class: 'share-value-who', text: v.me ? t('share.you') : v.name }),
          el('span', { class: 'share-value-when', text: localTime(v.time) }),
          el('span', { class: 'share-value-label', text: v.label, title: v.label }),
        );
        if (v.kept) row.append(el('span', { class: 'share-value-kept', text: t('share.blame.now') }));
        else {
          row.append(
            el('button', {
              class: 'btn btn-quiet',
              text: t('share.blame.put_back'),
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
      const done = el('button', { class: 'btn btn-primary', text: t('share.close'), attrs: { type: 'button', 'data-autofocus': true }, on: { click: () => close() } });
      return el(
        'div',
        { class: 'modal modal-wide share-dialog', attrs: { role: 'dialog', 'aria-modal': 'true', 'aria-labelledby': 'share-blame-title' } },
        el('h2', { class: 'modal-title', text: t('share.blame.title', { what: title }), attrs: { id: 'share-blame-title' } }),
        el('p', { class: 'modal-message', text: t('share.blame.message') }),
        list,
        el('div', { class: 'modal-actions' }, done),
      );
    },
    { onClose: () => ctx.focus() },
  );
}

function fieldName(field) {
  const names = { amount: t('share.field.amount'), x: t('share.field.x'), rate: t('share.field.rate'), x4: t('share.field.x4'), unit: t('share.field.unit'), unit2: t('share.field.unit2'), unit3: t('share.field.unit3'), unit4: t('share.field.unit4'), description: t('share.field.description'), currency: t('share.field.currency'), fringes: t('share.field.fringes'), groups: t('share.field.groups'), set: t('share.field.set'), location: t('share.field.location'), name: t('share.field.name'), code: t('share.field.code') };
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
  const title = (view.budget && view.budget.title) || t('share.untitled');
  if (view.shared) sharedDialog(title, setup);
  else startDialog(title, setup);
}

function colorPicker(chosen, onPick) {
  const host = el('div', { class: 'share-colors', attrs: { role: 'radiogroup', 'aria-label': t('share.your_color') } });
  let current = chosen;
  const buttons = [];
  for (const c of PERSON_COLORS) {
    const b = el('button', {
      class: `share-color ${colorClass(c)}`,
      title: colorName(c),
      attrs: { type: 'button', role: 'radio', 'aria-label': colorName(c), 'aria-checked': String(c === current) },
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

/** A person color's name, in words. */
function colorName(c) {
  const names = { teal: t('share.color.teal'), blue: t('share.color.blue'), purple: t('share.color.purple'), orange: t('share.color.orange'), rose: t('share.color.rose'), green: t('share.color.green'), amber: t('share.color.amber'), slate: t('share.color.slate') };
  return names[c] || c;
}

function startDialog(title, setup) {
  customDialog(
    (close) => {
      const me = { name: setup.name || '', color: setup.color || 'teal' };
      const error = el('p', { class: 'modal-error', attrs: { role: 'alert' } });
      error.hidden = true;
      const name = el('input', { class: 'modal-input', attrs: { type: 'text', id: 'share-name', maxlength: '60', placeholder: t('share.name_placeholder') } });
      name.value = me.name;
      const colors = colorPicker(me.color, (c) => { me.color = c; });
      // Where: the budget's own folder, or one chosen in Rust's dialog.
      const where = { picked: setup.picked || null, move: true };
      const whereText = el('span', { class: 'share-where-text' });
      const choose = el('button', { class: 'btn', text: t('share.choose_folder'), attrs: { type: 'button' } });
      const moveCopy = el('div', { class: 'share-movecopy' });
      const drawWhere = () => {
        whereText.textContent = where.picked ? where.picked : setup.folder || '';
        moveCopy.hidden = !where.picked || where.picked === setup.folder;
        moveCopy.replaceChildren(
          radio('share-mc', t('share.move_there'), where.move, () => { where.move = true; }),
          radio('share-mc', t('share.copy_there'), !where.move, () => { where.move = false; }),
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
      const cancel = el('button', { class: 'btn', text: t('share.cancel'), attrs: { type: 'button' } });
      const ok = el('button', { class: 'btn btn-primary', text: t('share.start_sharing'), attrs: { type: 'button' } });
      cancel.addEventListener('click', () => close());
      ok.addEventListener('click', async () => {
        error.hidden = true;
        const usePicked = Boolean(where.picked && where.picked !== setup.folder);
        if (usePicked) {
          const yes = await confirmDialog({
            title: where.move ? t('share.move_title') : t('share.copy_title'),
            message: where.move ? t('share.move_message', { file: setup.file, folder: where.picked }) : t('share.copy_message', { file: setup.file, folder: where.picked }),
            confirmText: where.move ? t('share.move_and_share') : t('share.copy_and_share'),
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
        el('h2', { class: 'modal-title', text: t('share.dialog_title', { title }), attrs: { id: 'share-title' } }),
        el('p', { class: 'modal-message', text: t('share.how') }),
        el(
          'div',
          { class: 'share-mode' },
          radio('share-mode', t('share.mode.folder'), true, () => {}),
          el('p', { class: 'share-mode-text', text: t('share.mode.folder_text') }),
          el('div', { class: 'share-where' }, el('span', { class: 'share-line-label', text: t('share.folder') }), whereText, choose),
          moveCopy,
        ),
        el('div', { class: 'share-mode is-later' }, radio('share-mode', t('share.mode.live'), false, () => {}, true), el('p', { class: 'share-mode-text', text: t('share.mode.live_text') })),
        el('div', { class: 'share-me' }, el('label', { class: 'modal-field', attrs: { for: 'share-name' } }, el('span', { text: t('share.your_name') }), name), el('div', { class: 'modal-field' }, el('span', { text: t('share.your_color') }), colors)),
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
      if (s.me) people.append(personRow({ ...s.me, name: t('share.name_you', { name: s.me.name }) }, s));
      for (const p of s.people) {
        people.append(personRow(p, s));
        const picker = s.can_set_roles ? rolePicker(p, s, setup, error) : null;
        if (picker) people.append(picker);
      }
      if (!s.people.length) people.append(el('p', { class: 'share-muted', text: t('share.nobody_yet') }));
      const save = el('button', { class: 'btn', text: t('share.save_me'), attrs: { type: 'button' } });
      save.addEventListener('click', async () => {
        error.hidden = true;
        try {
          await ctx.invoke('share_set_me', { name: name.value, color: me.color });
          ctx.toast(t('share.saved_me'));
          fetchStatus();
        } catch (e) {
          error.textContent = String(e);
          error.hidden = false;
        }
      });
      const done = el('button', { class: 'btn btn-primary', text: t('share.done'), attrs: { type: 'button', 'data-autofocus': true } });
      done.addEventListener('click', () => close());
      return el(
        'div',
        { class: 'modal modal-wide share-dialog', attrs: { role: 'dialog', 'aria-modal': 'true', 'aria-labelledby': 'share-title' } },
        el('h2', { class: 'modal-title', text: t('share.dialog_title', { title }), attrs: { id: 'share-title' } }),
        el('p', { class: 'modal-message', text: t('share.shared_message') }),
        line(t('share.folder'), s.folder || setup.folder || ''),
        line(t('share.file'), s.file || setup.file || ''),
        el('h4', { class: 'share-panel-sub', text: t('share.people') }),
        people,
        el('p', { class: 'share-muted', text: rolesNote(s) }),
        inviteSection(s, setup, error),
        s.key_warning ? el('p', { class: 'share-problem', text: s.key_warning, attrs: { role: 'alert' } }) : null,
        s.signed_text ? el('p', { class: 'share-muted', text: s.signed_text }) : null,
        el('div', { class: 'share-me' }, el('label', { class: 'modal-field', attrs: { for: 'share-name' } }, el('span', { text: t('share.your_name') }), name), el('div', { class: 'modal-field' }, el('span', { text: t('share.your_color') }), colors), save),
        error,
        el('div', { class: 'modal-actions' }, done),
      );
    },
    { onClose: () => ctx.focus() },
  );
}

/** An app older than this one can't read a change file that holds a role or a signature. */
const OLDER_APPS_NOTE = () => t('share.older_apps_note');

/** What Remove does (decisions.md, entry 89). */
function removeNote(name) {
  return t('share.remove_note', { name });
}

/** The owner is one computer: an admin (or co-owner) is the way back if it is lost. */
const ADMIN_NOTE = () => t('share.admin_note');

/** A scope only goes with Edits some categories (Rust refuses it too). */
const SCOPE_NOTE = () => t('share.scope_note');

function rolesNote(s) {
  if (!s.roles_started) return s.can_set_roles ? t('share.roles.unset_can') : t('share.roles.unset_cannot');
  return s.can_set_roles ? t('share.roles.set_can') : t('share.roles.set_cannot');
}

/** The roles a person can be given, in the words the picker shows. */
const ROLE_CHOICES = () => [
  ['editor', t('share.role.editor')],
  ['department', t('share.role.department')],
  ['actuals', t('share.role.actuals')],
  ['comment', t('share.role.comment')],
  ['read_only', t('share.role.read_only')],
];

/**
 * The owner's or an admin's picker for one person's role: a menu, the
 * category checklist for "Edits some categories", and a Set button. Only
 * the owner makes admins or changes an admin's role (Rust checks it too).
 */
function rolePicker(p, s, setup, error) {
  if (p.role === 'owner') return null;
  const mine = s.me ? s.me.role : '';
  const owner = mine === 'owner';
  // Only the owner changes a co-owner; the owner and co-owners change admins.
  if (p.role === 'co_owner' && !owner) return null;
  if (p.role === 'admin' && !owner && mine !== 'co_owner') return null;
  const choices = [
    ...(owner ? [['co_owner', t('share.role.co_owner')]] : []),
    ...(owner || mine === 'co_owner' ? [['admin', t('share.role.admin')]] : []),
    ...ROLE_CHOICES(),
  ];
  const id = `share-role-${p.actor.slice(0, 12)}`;
  const select = el('select', { class: 'modal-input share-role-select', attrs: { id, 'aria-label': t('share.role_label', { name: p.name }) } });
  for (const [key, label] of choices) {
    const o = el('option', { text: label, attrs: { value: key } });
    if (key === p.role) o.selected = true;
    select.append(o);
  }
  const picked = new Set(p.scope || []);
  const cats = el('div', { class: 'share-role-cats', attrs: { role: 'group', 'aria-label': t('share.role_cats_label', { name: p.name }) } });
  for (const c of setup.categories || []) {
    const box = el('input', { attrs: { type: 'checkbox' } });
    box.checked = picked.has(c.code);
    box.addEventListener('change', () => {
      if (box.checked) picked.add(c.code);
      else picked.delete(c.code);
      drawCats();
    });
    cats.append(el('label', { class: 'share-radio' }, box, el('span', { text: `${c.code} ${c.name}` })));
  }
  // Ticked categories stay in view with another role, so they can be unticked.
  const drawCats = () => { cats.hidden = select.value !== 'department' && picked.size === 0; };
  select.addEventListener('change', drawCats);
  drawCats();
  const set = el('button', { class: 'btn', text: t('share.set'), attrs: { type: 'button' } });
  // The first role: the question, in place (a second dialog would hide this
  // one), with a choice of admin to name at the same time.
  const first = el('div', { class: 'share-first-role', attrs: { role: 'group', 'aria-label': t('share.first_role_label') } });
  first.hidden = true;
  const apply = async (adminActor) => {
    set.disabled = true;
    try {
      let status = await ctx.invoke('share_set_role', {
        doc: ctx.doc(),
        actor: p.actor,
        role: select.value,
        scope: [...picked],
      });
      if (adminActor) {
        status = await ctx.invoke('share_set_role', { doc: ctx.doc(), actor: adminActor, role: 'admin', scope: [] });
      }
      if (status) statusArrived({ doc: ctx.doc(), status });
      ctx.toast(t('share.role_saved', { name: p.name }));
      Object.assign(s, status || {});
      first.hidden = true;
    } catch (e) {
      error.textContent = String(e);
      error.hidden = false;
    }
    set.disabled = false;
  };
  const askFirst = () => {
    const others = s.people.filter((x) => x.actor !== p.actor && x.role !== 'owner');
    const names = others.map((x) => x.name);
    const adminId = `share-first-admin-${p.actor.slice(0, 12)}`;
    const admin = el('select', { class: 'modal-input share-role-select', attrs: { id: adminId } });
    admin.append(el('option', { text: t('share.not_now'), attrs: { value: '' } }));
    const candidates = select.value === 'admin' || select.value === 'co_owner' ? others : [p, ...others];
    for (const x of candidates) admin.append(el('option', { text: t('share.make_admin', { name: x.name }), attrs: { value: x.actor } }));
    const go = el('button', { class: 'btn btn-primary', text: t('share.set_role'), attrs: { type: 'button' } });
    const cancel = el('button', { class: 'btn', text: t('share.cancel'), attrs: { type: 'button' } });
    go.addEventListener('click', () => {
      const chosen = admin.value;
      // Naming the person being set as the admin makes them one instead.
      if (chosen === p.actor) select.value = 'admin';
      apply(chosen && chosen !== p.actor ? chosen : null);
    });
    cancel.addEventListener('click', () => { first.hidden = true; set.focus(); });
    first.replaceChildren(
      el('p', { class: 'modal-message', text: names.length ? t('share.first_role.note_names', { names: names.join(', ') }) : t('share.first_role.note') }),
      el('p', { class: 'modal-message', text: OLDER_APPS_NOTE() }),
      el('p', { class: 'modal-message', text: ADMIN_NOTE() }),
      el('label', { class: 'modal-field', attrs: { for: adminId } }, el('span', { text: t('share.an_admin') }), admin),
      el('div', { class: 'modal-actions' }, cancel, go),
    );
    first.hidden = false;
    admin.focus();
  };
  set.addEventListener('click', () => {
    error.hidden = true;
    if (select.value !== 'department' && picked.size > 0) {
      error.textContent = SCOPE_NOTE();
      error.hidden = false;
      return;
    }
    if (!s.roles_started && owner) {
      askFirst();
      return;
    }
    apply(null);
  });
  // Remove: read only, with a cutoff at what has reached this computer.
  const remove = p.removed ? null : el('button', { class: 'btn btn-quiet', text: t('share.remove_button'), attrs: { type: 'button', 'aria-label': t('share.remove_label', { name: p.name }) } });
  const confirmRemove = el('div', { class: 'share-first-role', attrs: { role: 'group', 'aria-label': t('share.remove_label', { name: p.name }) } });
  confirmRemove.hidden = true;
  if (remove) {
    remove.addEventListener('click', () => {
      error.hidden = true;
      const go = el('button', { class: 'btn btn-primary', text: t('share.remove'), attrs: { type: 'button' } });
      const keep = el('button', { class: 'btn', text: t('share.cancel'), attrs: { type: 'button' } });
      keep.addEventListener('click', () => { confirmRemove.hidden = true; remove.focus(); });
      go.addEventListener('click', async () => {
        go.disabled = true;
        try {
          const status = await ctx.invoke('share_remove_person', { doc: ctx.doc(), actor: p.actor });
          if (status) statusArrived({ doc: ctx.doc(), status });
          ctx.toast(t('share.removed', { name: p.name }));
          Object.assign(s, status || {});
          confirmRemove.hidden = true;
        } catch (e) {
          error.textContent = String(e);
          error.hidden = false;
        }
        go.disabled = false;
      });
      confirmRemove.replaceChildren(el('p', { class: 'modal-message', text: removeNote(p.name) }), el('div', { class: 'modal-actions' }, keep, go));
      confirmRemove.hidden = false;
      go.focus();
    });
  }
  return el('div', { class: 'share-role-row' }, el('label', { class: 'share-line-label', text: t('share.may_change'), attrs: { for: id } }), select, set, remove, cats, first, confirmRemove);
}

// ---------------------------------------------------------------------------
// Invites (decisions.md, entry 97): Invite someone in the Share dialog, and
// File > Join from Link…

/** How long an invite lasts, in days, as the dialog offers it (src/shared.rs INVITE_DAYS). */
const INVITE_DAYS = () => [
  [1, t('share.invite.days_1')],
  [7, t('share.invite.days_7')],
  [30, t('share.invite.days_30')],
];

/** A date as the local calendar shows it. */
function dateText(iso) {
  const d = new Date(iso);
  if (!Number.isFinite(d.getTime())) return String(iso || '');
  return d.toLocaleDateString([], { year: 'numeric', month: 'short', day: 'numeric' });
}

/** Puts `text` on the clipboard; true when it got there. */
async function copyText(text, input) {
  const clip = navigator.clipboard;
  if (clip && typeof clip.writeText === 'function') {
    try {
      await clip.writeText(text);
      return true;
    } catch {
      // refused: the selection below
    }
  }
  if (!input) return false;
  input.focus();
  input.select();
  try {
    return document.execCommand('copy');
  } catch {
    return false;
  }
}

/**
 * Invite someone: a role (with categories for Edits some categories), how
 * long the link lasts, and Create invite link, which shows the link once.
 * Below it, the invites waiting to be used, with Cancel, and the used ones
 * with who joined. Only for whoever may give roles.
 */
function inviteSection(s, setup, error) {
  if (!s.can_invite) return null;
  const mine = s.me ? s.me.role : '';
  const choices = [
    ...(mine === 'owner' ? [['co_owner', t('share.role.co_owner')]] : []),
    ...(mine === 'owner' || mine === 'co_owner' ? [['admin', t('share.role.admin')]] : []),
    ...ROLE_CHOICES(),
  ];
  const role = el('select', { class: 'modal-input share-role-select', attrs: { id: 'share-invite-role' } });
  for (const [key, label] of choices) {
    const o = el('option', { text: label, attrs: { value: key } });
    if (key === 'editor') o.selected = true;
    role.append(o);
  }
  const days = el('select', { class: 'modal-input share-role-select', attrs: { id: 'share-invite-days' } });
  for (const [n, label] of INVITE_DAYS()) {
    const o = el('option', { text: label, attrs: { value: String(n) } });
    if (n === 7) o.selected = true;
    days.append(o);
  }
  const picked = new Set();
  const cats = el('div', { class: 'share-role-cats', attrs: { role: 'group', 'aria-label': t('share.invite.cats_label') } });
  for (const c of setup.categories || []) {
    const box = el('input', { attrs: { type: 'checkbox' } });
    box.addEventListener('change', () => {
      if (box.checked) picked.add(c.code);
      else picked.delete(c.code);
    });
    cats.append(el('label', { class: 'share-radio' }, box, el('span', { text: `${c.code} ${c.name}` })));
  }
  const drawCats = () => { cats.hidden = role.value !== 'department'; };
  role.addEventListener('change', drawCats);
  drawCats();
  const create = el('button', { class: 'btn', text: t('share.invite.create'), attrs: { type: 'button' } });
  // The link, shown once: nothing keeps it after this dialog.
  const made = el('div', { class: 'share-invite-made', attrs: { role: 'status' } });
  made.hidden = true;
  const list = el('div', { class: 'share-invite-list' });
  const drawList = () => {
    const rows = [];
    for (const inv of s.invites || []) {
      if (inv.state === 'used') {
        rows.push(el('p', { class: 'share-muted', text: t('share.invite.used', { name: inv.joined || '', role: inv.role_text }) }));
        continue;
      }
      const cancel = inv.can_cancel ? el('button', { class: 'btn btn-quiet', text: t('share.invite.cancel'), attrs: { type: 'button', 'aria-label': t('share.invite.cancel_label', { role: inv.role_text }) } }) : null;
      if (cancel) {
        cancel.addEventListener('click', async () => {
          error.hidden = true;
          cancel.disabled = true;
          try {
            const status = await ctx.invoke('share_cancel_invite', { doc: ctx.doc(), invite: inv.id });
            if (status) statusArrived({ doc: ctx.doc(), status });
            Object.assign(s, status || {});
            ctx.toast(t('share.invite.cancelled'));
            drawList();
          } catch (e) {
            error.textContent = String(e);
            error.hidden = false;
            cancel.disabled = false;
          }
        });
      }
      rows.push(
        el(
          'div',
          { class: 'share-invite-row' },
          el('span', { text: t('share.invite.pending', { role: inv.role_text, name: inv.by, date: dateText(inv.expires) }) }),
          cancel,
        ),
      );
    }
    list.replaceChildren(...rows);
    list.hidden = rows.length === 0;
  };
  drawList();
  create.addEventListener('click', async () => {
    error.hidden = true;
    create.disabled = true;
    try {
      const inv = await ctx.invoke('share_create_invite', {
        doc: ctx.doc(),
        role: role.value,
        scope: role.value === 'department' ? [...picked] : [],
        days: Number(days.value),
      });
      const box = el('input', { class: 'modal-input share-invite-link', attrs: { type: 'text', readonly: 'readonly', 'aria-label': t('share.invite.link_label') } });
      box.value = inv.link;
      const copy = el('button', { class: 'btn btn-primary', text: t('share.invite.copy'), attrs: { type: 'button' } });
      copy.addEventListener('click', async () => {
        const ok = await copyText(inv.link, box);
        ctx.toast(ok ? t('share.invite.copied') : t('share.invite.copy_failed'));
      });
      made.replaceChildren(
        el('div', { class: 'share-invite-linkrow' }, box, copy),
        el('p', { class: 'modal-message', text: t('share.invite.link_note', { role: inv.role_text, date: dateText(inv.expires) }) }),
        el('p', { class: 'share-muted', text: t('share.invite.folder_note') }),
      );
      made.hidden = false;
      box.focus();
      box.select();
      fetchStatus().then(() => {
        Object.assign(s, current() || {});
        drawList();
      });
    } catch (e) {
      error.textContent = String(e);
      error.hidden = false;
    }
    create.disabled = false;
  });
  return el(
    'div',
    { class: 'share-invite' },
    el('h4', { class: 'share-panel-sub', text: t('share.invite.title') }),
    el(
      'div',
      { class: 'share-role-row' },
      el('label', { class: 'share-line-label', text: t('share.invite.can'), attrs: { for: 'share-invite-role' } }),
      role,
      el('label', { class: 'share-line-label', text: t('share.invite.expires'), attrs: { for: 'share-invite-days' } }),
      days,
      create,
      cats,
    ),
    made,
    list,
  );
}

/**
 * File > Join from Link…: paste the link, then pick the budget's file in
 * this computer's copy of the shared folder (Rust's own dialog), with the
 * name and color other people will see.
 */
export function openJoinDialog() {
  if (!ctx || isModalOpen()) return;
  customDialog(
    (close) => {
      const error = el('p', { class: 'modal-error', attrs: { role: 'alert' } });
      error.hidden = true;
      const fail = (e) => {
        error.textContent = String(e);
        error.hidden = false;
      };
      const link = el('textarea', { class: 'modal-input share-join-link', attrs: { id: 'share-join-link', rows: '3', spellcheck: 'false', placeholder: t('share.join.placeholder') } });
      const me = { name: '', color: 'teal' };
      const name = el('input', { class: 'modal-input', attrs: { type: 'text', id: 'share-join-name', maxlength: '60', placeholder: t('share.name_placeholder') } });
      const second = el('div', { class: 'share-join-next' });
      second.hidden = true;
      const cancel = el('button', { class: 'btn', text: t('share.cancel'), attrs: { type: 'button' } });
      const go = el('button', { class: 'btn btn-primary', text: t('share.join.continue'), attrs: { type: 'button' } });
      let checked = null;
      cancel.addEventListener('click', () => close());
      const join = async () => {
        go.disabled = true;
        try {
          const out = await ctx.invoke('share_join_from_link', { link: link.value, me: { name: name.value, color: me.color } });
          if (!out) {
            go.disabled = false;
            return;
          }
          if (out.view) ctx.apply(out.view, { cmd: 'share_join' });
          fetchStatus();
          if (out.error) {
            fail(out.error);
            go.textContent = t('share.join.try_again');
          } else {
            close();
            ctx.toast(t('share.join.joined', { role: out.role_text || '' }));
          }
        } catch (e) {
          fail(e);
        }
        go.disabled = false;
      };
      go.addEventListener('click', async () => {
        error.hidden = true;
        if (checked) {
          join();
          return;
        }
        try {
          checked = await ctx.invoke('share_join_check', { link: link.value });
        } catch (e) {
          fail(e);
          return;
        }
        name.value = checked.name || '';
        me.color = checked.color || 'teal';
        link.readOnly = true;
        second.replaceChildren(
          el('p', { class: 'modal-message', text: t('share.join.pick_note', { file: checked.file }) }),
          el('div', { class: 'share-me' }, el('label', { class: 'modal-field', attrs: { for: 'share-join-name' } }, el('span', { text: t('share.your_name') }), name), el('div', { class: 'modal-field' }, el('span', { text: t('share.your_color') }), colorPicker(me.color, (c) => { me.color = c; }))),
        );
        second.hidden = false;
        go.textContent = t('share.join.choose');
        name.focus();
      });
      return el(
        'div',
        { class: 'modal modal-wide share-dialog', attrs: { role: 'dialog', 'aria-modal': 'true', 'aria-labelledby': 'share-join-title' } },
        el('h2', { class: 'modal-title', text: t('share.join.title'), attrs: { id: 'share-join-title' } }),
        el('p', { class: 'modal-message', text: t('share.join.message') }),
        el('label', { class: 'modal-field', attrs: { for: 'share-join-link' } }, el('span', { text: t('share.join.link') }), link),
        second,
        error,
        el('div', { class: 'modal-actions' }, cancel, go),
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
      const ok = el('button', { class: 'btn btn-primary', text: t('share.got_it'), attrs: { type: 'button', 'data-autofocus': true }, on: { click: () => close() } });
      return el(
        'div',
        { class: 'modal share-dialog', attrs: { role: 'dialog', 'aria-modal': 'true', 'aria-labelledby': 'share-notice-title' } },
        el('h2', { class: 'modal-title', text: t('share.notice.title'), attrs: { id: 'share-notice-title' } }),
        el('p', { class: 'modal-message', text: t('share.notice.kept') }),
        el('p', { class: 'modal-message', text: t('share.notice.nothing_to_save') }),
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
