// The tab strip: one tab per open budget (decisions.md, entries 19 and 25),
// between the toolbar and the breadcrumb. A tab shows the file's name (or
// the budget's title before it has a file) and a dot for unsaved changes;
// the tab in front, and the one under the pointer, show a close button.
//
// A click (or Left, Right, Home and End on a focused tab) brings a tab to
// the front; the close button or a middle-click closes it; dragging it (or
// Option+Shift+Left and Right) moves it; "+" opens a new budget. When the
// tabs don't fit, the strip scrolls sideways (the mouse wheel too), with
// arrow buttons at its ends.
//
// main.js owns what the buttons do: initTabs(host, { switchTo(id),
// close(id), add(), move(id, to) }). renderTabs(tabs) draws the list Rust
// sends with every view (src/docs.rs, TabInfo).
//
// Budget text goes in with textContent only; drag feedback is classes (the
// CSP allows no inline styles).

import { el } from './dom.js';
import { withShortcut } from './shortcuts.js';

let ctx = null;
let host = null;
let strip = null;
let leftBtn = null;
let rightBtn = null;
let tabs = [];
let drawnSig = '';
let shownActive = null;
// A drag just ended on a tab: the click that follows it isn't a click.
let dragged = false;

const DRAG_START_PX = 5;

function ariaLabel(t) {
  const parts = [t.label];
  if (t.dirty) parts.push('unsaved changes');
  if (t.conflict) parts.push('changed outside Film Wizard');
  return parts.join(', ');
}

function tooltip(t) {
  const lines = [t.title && t.title !== t.label ? `${t.label}: ${t.title}` : t.label, t.path || 'Not saved yet'];
  if (t.dirty) lines.push('Unsaved changes');
  if (t.conflict) lines.push('Changed outside Film Wizard: autosave is paused until you choose which version to keep');
  return lines.join('\n');
}

/** Sets up the strip in `hostEl`. */
export function initTabs(hostEl, handlers) {
  host = hostEl;
  ctx = handlers;
  strip = el('div', {
    class: 'tabbar-tabs',
    attrs: { role: 'tablist', 'aria-label': 'Open budgets', 'aria-orientation': 'horizontal' },
  });
  leftBtn = el('button', {
    class: 'tabbar-scroll tabbar-scroll-left',
    title: 'Scroll the tabs left',
    attrs: { type: 'button', tabindex: '-1', 'aria-label': 'Scroll the tabs left' },
    on: { click: () => scrollByPage(-1) },
  });
  rightBtn = el('button', {
    class: 'tabbar-scroll tabbar-scroll-right',
    title: 'Scroll the tabs right',
    attrs: { type: 'button', tabindex: '-1', 'aria-label': 'Scroll the tabs right' },
    on: { click: () => scrollByPage(1) },
  });
  const add = el('button', {
    class: 'tabbar-new',
    text: '+',
    title: withShortcut('New budget in a new tab', 'file.new_tab'),
    attrs: { type: 'button', 'aria-label': 'New tab' },
    on: { click: () => ctx.add() },
  });
  host.replaceChildren(leftBtn, strip, rightBtn, add);
  strip.addEventListener('scroll', updateArrows);
  strip.addEventListener(
    'wheel',
    (e) => {
      // An ordinary (up and down) wheel scrolls the tabs sideways.
      if (Math.abs(e.deltaY) <= Math.abs(e.deltaX) || strip.scrollWidth <= strip.clientWidth) return;
      e.preventDefault();
      strip.scrollLeft += e.deltaY;
    },
    { passive: false },
  );
  if (typeof ResizeObserver === 'function') new ResizeObserver(() => updateArrows()).observe(strip);
  else window.addEventListener('resize', updateArrows);
}

/**
 * Draws `list` (TabInfo, in tab order), when it differs from what is drawn
 * (every view carries it). Keeps focus on the tab that had it.
 */
export function renderTabs(list) {
  if (!strip || !Array.isArray(list)) return;
  const sig = JSON.stringify(list);
  if (sig === drawnSig) return;
  drawnSig = sig;
  tabs = list;
  const focused = strip.contains(document.activeElement) ? Number(document.activeElement.dataset.doc) : null;
  strip.replaceChildren(...list.map((t) => tabEl(t)));
  const active = list.find((t) => t.active);
  if (active && active.id !== shownActive) {
    shownActive = active.id;
    revealTab(active.id);
  }
  if (focused !== null) focusTab(list.some((t) => t.id === focused) ? focused : active && active.id);
  updateArrows();
}

/** Puts keyboard focus on tab `id`. */
export function focusTab(id) {
  const b = strip && strip.querySelector(`.doc-tab-btn[data-doc="${Number(id)}"]`);
  if (!b) return false;
  b.focus();
  revealTab(id);
  return true;
}

/** True when keyboard focus is on the strip. */
export function tabsHaveFocus() {
  return Boolean(host && host.contains(document.activeElement));
}

function tabEl(t) {
  const btn = el(
    'button',
    {
      class: 'doc-tab-btn',
      title: tooltip(t),
      attrs: {
        type: 'button',
        role: 'tab',
        id: `doc-tab-${t.id}`,
        'aria-selected': String(Boolean(t.active)),
        'aria-label': ariaLabel(t),
        tabindex: t.active ? '0' : '-1',
        'aria-keyshortcuts': 'Alt+Shift+ArrowLeft Alt+Shift+ArrowRight',
      },
      data: { doc: t.id },
    },
    el('span', { class: 'doc-tab-label', text: t.label }),
    t.dirty ? el('span', { class: 'doc-tab-dot', attrs: { 'aria-hidden': 'true' } }) : null,
  );
  btn.addEventListener('click', () => {
    if (dragged) {
      dragged = false;
      return;
    }
    if (!t.active) ctx.switchTo(t.id);
  });
  btn.addEventListener('keydown', (e) => tabKey(e, t.id));
  btn.addEventListener('pointerdown', (e) => startDrag(e, btn, t.id));

  // Mouse only: the keyboard closes a tab with Cmd+W, which the shortcut
  // sheet and the tooltip name.
  const close = el('button', {
    class: 'doc-tab-close',
    text: '\u00d7',
    title: withShortcut('Close tab', 'file.close_tab'),
    attrs: { type: 'button', tabindex: '-1', 'aria-hidden': 'true' },
    data: { doc: t.id },
  });
  close.addEventListener('mousedown', (e) => e.preventDefault());
  close.addEventListener('pointerdown', (e) => e.stopPropagation());
  close.addEventListener('click', (e) => {
    e.stopPropagation();
    ctx.close(t.id);
  });

  const cls = ['doc-tab'];
  if (t.active) cls.push('is-active');
  if (t.dirty) cls.push('is-dirty');
  const wrap = el('div', { class: cls.join(' '), attrs: { role: 'presentation' }, data: { doc: t.id } }, btn, close);
  // Middle-click closes, as in browsers (and keeps Windows from starting
  // its scroll mode).
  wrap.addEventListener('mousedown', (e) => {
    if (e.button === 1) e.preventDefault();
  });
  wrap.addEventListener('auxclick', (e) => {
    if (e.button !== 1) return;
    e.preventDefault();
    ctx.close(t.id);
  });
  return wrap;
}

function tabKey(e, id) {
  const ids = tabs.map((t) => t.id);
  const i = ids.indexOf(id);
  if (i < 0) return;
  if (e.altKey && e.shiftKey && !e.metaKey && !e.ctrlKey) {
    let to = null;
    if (e.key === 'ArrowLeft') to = i - 1;
    else if (e.key === 'ArrowRight') to = i + 1;
    if (to === null) return;
    e.preventDefault();
    e.stopPropagation();
    if (to >= 0 && to < ids.length) ctx.move(id, to);
    return;
  }
  if (e.altKey || e.metaKey || e.ctrlKey || e.shiftKey) return;
  let j = null;
  if (e.key === 'ArrowRight') j = (i + 1) % ids.length;
  else if (e.key === 'ArrowLeft') j = (i - 1 + ids.length) % ids.length;
  else if (e.key === 'Home') j = 0;
  else if (e.key === 'End') j = ids.length - 1;
  if (j === null) return;
  e.preventDefault();
  e.stopPropagation();
  ctx.switchTo(ids[j], { keepFocus: true });
}

// ---------------------------------------------------------------------------
// Scrolling

function scrollByPage(dir) {
  strip.scrollLeft += dir * Math.max(80, strip.clientWidth * 0.8);
  updateArrows();
}

function updateArrows() {
  if (!strip) return;
  const over = strip.scrollWidth > strip.clientWidth + 1;
  host.classList.toggle('is-overflowing', over);
  leftBtn.hidden = !over;
  rightBtn.hidden = !over;
  leftBtn.disabled = strip.scrollLeft <= 0;
  rightBtn.disabled = strip.scrollLeft + strip.clientWidth >= strip.scrollWidth - 1;
}

/** Scrolls the strip just enough to show tab `id` whole. */
function revealTab(id) {
  const wrap = strip.querySelector(`.doc-tab[data-doc="${Number(id)}"]`);
  if (!wrap) return;
  const left = wrap.offsetLeft;
  const right = left + wrap.offsetWidth;
  if (left < strip.scrollLeft) strip.scrollLeft = left;
  else if (right > strip.scrollLeft + strip.clientWidth) strip.scrollLeft = right - strip.clientWidth;
}

// ---------------------------------------------------------------------------
// Dragging a tab (pointer events, as the tools panel's tabs: HTML drag and
// drop is unreliable in some system web views)

function startDrag(e, btn, id) {
  if (e.button !== 0) return;
  dragged = false;
  const x0 = e.clientX;
  const y0 = e.clientY;
  let dragging = false;
  let target = null;
  try {
    btn.setPointerCapture(e.pointerId);
  } catch {
    // Not a real pointer (a test); moves still arrive.
  }
  const wrap = btn.closest('.doc-tab');
  const move = (ev) => {
    if (!dragging) {
      if (Math.hypot(ev.clientX - x0, ev.clientY - y0) < DRAG_START_PX) return;
      dragging = true;
      wrap.classList.add('is-dragging');
      document.body.classList.add('tabs-dragging');
    }
    // Near an end of a strip that scrolls: scroll it along.
    const r = strip.getBoundingClientRect();
    if (ev.clientX < r.left + 16) strip.scrollLeft -= 12;
    else if (ev.clientX > r.right - 16) strip.scrollLeft += 12;
    target = dropTarget(ev.clientX, id);
    paintDrop(target);
  };
  const end = () => {
    btn.removeEventListener('pointermove', move);
    btn.removeEventListener('pointerup', end);
    btn.removeEventListener('pointercancel', cancel);
    document.body.classList.remove('tabs-dragging');
    wrap.classList.remove('is-dragging');
    paintDrop(null);
    if (!dragging) return;
    dragged = true;
    // A click follows the drag on the same button; if none does, forget it.
    setTimeout(() => {
      dragged = false;
    }, 0);
    if (target) ctx.move(id, target.to);
  };
  const cancel = () => {
    target = null;
    end();
  };
  btn.addEventListener('pointermove', move);
  btn.addEventListener('pointerup', end);
  btn.addEventListener('pointercancel', cancel);
}

/**
 * Where tab `id` dropped at `x` goes: { to (its place afterwards), el, side },
 * or null when it would stay where it is.
 */
function dropTarget(x, id) {
  const wraps = [...strip.querySelectorAll('.doc-tab')];
  if (!wraps.length) return null;
  let over = null;
  let after = false;
  for (const w of wraps) {
    const r = w.getBoundingClientRect();
    if (x >= r.left && x < r.right) {
      over = w;
      after = x > r.left + r.width / 2;
      break;
    }
  }
  if (!over) {
    const first = wraps[0].getBoundingClientRect();
    over = x < first.left ? wraps[0] : wraps[wraps.length - 1];
    after = over !== wraps[0] || x >= first.left;
  }
  const ids = tabs.map((t) => t.id);
  const overId = Number(over.dataset.doc);
  if (overId === id) return null;
  const before = ids.indexOf(overId) + (after ? 1 : 0);
  const from = ids.indexOf(id);
  const to = before > from ? before - 1 : before;
  if (to === from) return null;
  return { to, el: over, side: after ? 'after' : 'before' };
}

function paintDrop(target) {
  for (const n of strip.querySelectorAll('.drop-before, .drop-after')) n.classList.remove('drop-before', 'drop-after');
  if (target && target.el) target.el.classList.add(`drop-${target.side}`);
}
