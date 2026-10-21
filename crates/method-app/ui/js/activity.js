// The Activity tab of the tools panel (src/shared_app.rs; design 9.6): the
// recent changes to a shared budget by everyone, newest first, grouped by
// hour, each with the person's initials in their color, what they did and
// when; a filter by person; "Go there" jumps to the place. Fetched again
// (share_activity) when the budget changes (a new view rev).

import { el } from './dom.js';
import { invoke } from './api.js';
import { jumpEntry } from './review.js';
import { t } from './i18n.js';

const st = {
  doc: null,
  rev: null,
  items: null,
  error: '',
  fetching: false,
  again: false,
  host: null,
  ctx: null,
  shared: false,
  person: '', // actor filter ('' for everyone)
};

export function renderActivity(host, view, ctx) {
  st.host = host;
  st.ctx = ctx;
  const doc = view && typeof view.doc === 'number' ? view.doc : null;
  const fresh = doc !== st.doc || view.rev !== st.rev;
  if (doc !== st.doc) {
    st.items = null;
    st.error = '';
    st.person = '';
  }
  st.doc = doc;
  st.rev = view ? view.rev : null;
  st.shared = Boolean(view && view.shared);
  draw();
  if (fresh && st.shared) fetchItems();
}

async function fetchItems() {
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
        const items = await invoke('share_activity', doc === null ? { limit: 300 } : { doc, limit: 300 });
        if (doc !== st.doc) continue;
        st.items = Array.isArray(items) ? items : [];
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

function colorClass(color) {
  return ['teal', 'blue', 'purple', 'orange', 'rose', 'green', 'amber', 'slate'].includes(color) ? `pc-${color}` : 'pc-slate';
}

function initials(name) {
  const words = String(name || '').split(/\s+/).filter(Boolean).slice(0, 2);
  return words.map((w) => w[0].toUpperCase()).join('') || '?';
}

/** "Today 2 PM", "Oct 3, 4 PM": the hour a change falls in. */
function hourLabel(iso) {
  const d = new Date(iso);
  if (!Number.isFinite(d.getTime())) return t('activity.some_time');
  const now = new Date();
  const hour = d.toLocaleTimeString([], { hour: 'numeric' });
  if (d.toDateString() === now.toDateString()) return t('activity.today_hour', { hour });
  return `${d.toLocaleDateString([], { month: 'short', day: 'numeric' })}, ${hour}`;
}

function timeText(iso) {
  const d = new Date(iso);
  if (!Number.isFinite(d.getTime())) return '';
  return d.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
}

function draw() {
  const host = st.host;
  if (!host) return;
  const box = el('div', { class: 'activity' });
  if (!st.shared) {
    box.append(el('p', { class: 'history-empty', text: t('activity.not_shared') }));
    host.replaceChildren(box);
    return;
  }
  if (st.error) box.append(el('p', { class: 'history-error', text: st.error, attrs: { role: 'alert' } }));
  const items = st.items;
  if (!items) {
    box.append(el('p', { class: 'history-empty', text: t('activity.looking') }));
    host.replaceChildren(box);
    return;
  }
  // The people filter.
  const people = new Map();
  for (const it of items) if (!people.has(it.actor)) people.set(it.actor, it);
  if (people.size > 1) {
    const sel = el('select', { class: 'activity-filter', attrs: { 'aria-label': t('activity.show_changes_by') } });
    sel.append(el('option', { text: t('activity.everyone'), attrs: { value: '' } }));
    for (const [actor, it] of people) sel.append(el('option', { text: it.me ? t('activity.name_you', { name: it.name }) : it.name, attrs: { value: actor } }));
    sel.value = st.person;
    sel.addEventListener('change', () => {
      st.person = sel.value;
      draw();
    });
    box.append(el('div', { class: 'activity-bar' }, sel));
  }
  const shown = st.person ? items.filter((it) => it.actor === st.person) : items;
  if (!shown.length) {
    box.append(el('p', { class: 'history-empty', text: t('activity.no_changes') }));
    host.replaceChildren(box);
    return;
  }
  let group = null;
  let list = null;
  for (const it of shown) {
    const h = hourLabel(it.time);
    if (h !== group) {
      group = h;
      box.append(el('h4', { class: 'activity-hour', text: h }));
      list = el('ol', { class: 'activity-list' });
      box.append(list);
    }
    list.append(row(it));
  }
  host.replaceChildren(box);
}

function row(it) {
  const entry = jumpEntry(it.place);
  const main = el(
    'div',
    { class: 'activity-main' },
    el('span', { class: `share-avatar share-avatar-sm ${colorClass(it.color)}`, text: initials(it.name), title: it.name }),
    el('span', { class: 'activity-text' }, el('span', { class: 'activity-who', text: it.me ? t('activity.you') : it.name }), ' ', el('span', { class: 'activity-label', text: it.label, title: it.label })),
    el('span', { class: 'activity-time', text: it.signed ? t('activity.time_signed', { time: timeText(it.time) }) : timeText(it.time), title: it.signed ? t('activity.signed_tip') : '' }),
  );
  const li = el('li', { class: `activity-item ${it.me ? 'is-me' : ''}` }, main);
  if (it.place && it.place.text) {
    const where = el('div', { class: 'activity-where' }, el('span', { text: it.place.text }));
    if (entry && st.ctx && typeof st.ctx.jumpTo === 'function') {
      where.append(el('button', { class: 'btn btn-quiet activity-go', text: t('activity.go_there'), attrs: { type: 'button' }, on: { click: () => st.ctx.jumpTo(entry) } }));
    }
    li.append(where);
  }
  return li;
}
