// The Review tab of the tools panel (src/shared_app.rs; design 9.5):
// everything in a shared budget that needs a person. A value changed by
// two people (keep one or put the other back), a deleted item still in
// use, two items with one name, numbers that don't fit, globals that refer
// back to themselves. Each item says what happened in a sentence and has
// the actions that settle it; "Go there" jumps to the place.
//
// renderReview(host, view, ctx) draws from what it fetched last and fetches
// again (share_review) when the budget changed (a new view rev). Settling
// goes through share_use_value, which returns a view (ctx.call applies
// it). Budget text goes in with textContent only.

import { el } from './dom.js';
import { invoke } from './api.js';
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
};

export function renderReview(host, view, ctx) {
  st.host = host;
  st.ctx = ctx;
  const doc = view && typeof view.doc === 'number' ? view.doc : null;
  const fresh = doc !== st.doc || view.rev !== st.rev;
  if (doc !== st.doc) {
    st.items = null;
    st.error = '';
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
        const items = await invoke('share_review', doc === null ? {} : { doc });
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

function localTime(iso) {
  const d = new Date(iso);
  if (!Number.isFinite(d.getTime())) return '';
  const now = new Date();
  const time = d.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
  if (d.toDateString() === now.toDateString()) return time;
  return `${d.toLocaleDateString([], { month: 'short', day: 'numeric' })} ${time}`;
}

/** What ctx.jumpTo needs for a place, or null when it can't be reached. */
export function jumpEntry(place) {
  if (!place || typeof place !== 'object') return null;
  if (place.kind === 'row' && place.parent) return { row_id: Number(place.key), account: place.parent, ti: -1, ai: -1, field: place.field || null };
  if (place.kind === 'account') return { row_id: null, account: place.key, ti: -1, ai: -1, field: place.field === 'code' ? 'code' : 'name' };
  if (place.kind === 'category') return { kind: 'category', category: place.key, ti: -1 };
  return null;
}

function draw() {
  const host = st.host;
  if (!host) return;
  const box = el('div', { class: 'review' });
  if (!st.shared) {
    box.append(el('p', { class: 'history-empty', text: t('review.not_shared') }));
    host.replaceChildren(box);
    return;
  }
  if (st.error) box.append(el('p', { class: 'history-error', text: st.error, attrs: { role: 'alert' } }));
  const items = st.items;
  if (!items) {
    box.append(el('p', { class: 'history-empty', text: t('review.looking') }));
    host.replaceChildren(box);
    return;
  }
  if (!items.length) {
    box.append(el('p', { class: 'history-empty', text: t('review.nothing_to_review') }));
    host.replaceChildren(box);
    return;
  }
  const list = el('ol', { class: 'review-list', attrs: { 'aria-label': t('review.things_to_review') } });
  for (const it of items) list.append(item(it));
  box.append(list);
  host.replaceChildren(box);
}

function item(it) {
  const li = el('li', { class: `review-item review-${it.key}` });
  li.append(el('p', { class: 'review-sentence', text: it.sentence }));
  const actions = el('div', { class: 'review-actions' });
  const entry = jumpEntry(it.place);
  if (entry && st.ctx && typeof st.ctx.jumpTo === 'function') {
    actions.append(el('button', { class: 'btn btn-quiet', text: t('review.go_there'), attrs: { type: 'button' }, on: { click: () => st.ctx.jumpTo(entry) } }));
  }
  if (it.key === 'overwritten' && Array.isArray(it.values) && it.values.length) {
    const values = el('div', { class: 'share-values' });
    for (const v of it.values) {
      values.append(
        el(
          'div',
          { class: `share-value ${v.kept ? 'is-kept' : ''}` },
          el('span', { class: `share-avatar share-avatar-sm ${colorClass(v.color)}`, text: initials(v.name) }),
          el('span', { class: 'share-value-who', text: v.me ? t('review.you') : v.name }),
          el('span', { class: 'share-value-text', text: v.value || t('review.blank'), title: v.value }),
          el('span', { class: 'share-value-when', text: v.kept ? t('review.time_kept', { time: localTime(v.time) }) : localTime(v.time) }),
        ),
      );
      actions.append(
        el('button', {
          class: v.kept ? 'btn btn-primary' : 'btn',
          text: v.kept ? t('review.keep', { value: short(v.value) }) : t('review.use', { value: short(v.value) }),
          attrs: { type: 'button' },
          on: { click: () => useValue(it.place, v.index) },
        }),
      );
    }
    li.append(values);
  }
  li.append(actions);
  return li;
}

function short(v) {
  const s = String(v === undefined || v === null || v === '' ? t('review.blank') : v);
  return s.length > 24 ? `${s.slice(0, 23)}…` : s;
}

function initials(name) {
  const words = String(name || '').split(/\s+/).filter(Boolean).slice(0, 2);
  return words.map((w) => w[0].toUpperCase()).join('') || '?';
}

async function useValue(place, index) {
  if (!st.ctx || !place) return;
  try {
    await st.ctx.call('share_use_value', { kind: place.kind, key: place.key, field: place.field, index });
  } catch (e) {
    if (typeof st.ctx.toast === 'function') st.ctx.toast(e);
  }
}
