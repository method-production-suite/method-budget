// The tools panel: two tab panes stacked (docked at the right, or in its own
// window) or side by side (docked at the bottom), decisions.md entry 21.
//
// Default tabs: Fringes, Groups, Globals, Units and Find in the first pane; Sets,
// Locations, Currency, Notes and Budget in the second. Budget has its own
// tabs: Charges, Credits, Info, Settings and Baseline. A tab can be dragged
// to another place in its pane or to the other pane (or moved with
// Option+Shift+arrows); the arrangement, each pane's tab and the Budget tab's
// tab are remembered (prefs.js). The header shows what is selected in the
// grid and three buttons for where the panel goes.
//
// renderTools(host, view, ctx) draws everything on every new view, as the
// side panel always has: lists of fringes, groups, sets, locations and
// currencies come from apply.js, globals, units and the Budget tabs from
// setup.js and topsheet-setup.js, notes from notes.js, find and replace from
// find.js. ctx:
//   call(cmd, args)      runs a command and redraws from its view
//   selection()          the grid's { level, keys, cat, cursor }
//   position             'right', 'bottom' or 'window'
//   setPosition(where)   docks the panel or pops it out
//   jumpTo(entry)        goes to a noted line (notes.js) or a found cell
//                        (find.js: `field` names the column, and no `row_id`
//                        means an account's name or number)
//   onLayout(layout)     the tab arrangement changed (the tools window tells
//                        the main window, which keeps the preference)
//
// Nothing here uses inline styles or innerHTML: drag feedback is classes,
// icons are SVG elements built with DOM calls.

import { el, isMac } from './dom.js';
import { readPref, writePref } from './prefs.js';
import { applyView, applyHeadHost, applyListHost, APPLY_LISTS } from './apply.js';
import { setupBegin, renderSetupTab, setupEnd } from './setup.js';
import { EXTRA_TABS } from './topsheet-setup.js';
import { renderNotes } from './notes.js';
import { renderFind } from './find.js';

const TOOL_TABS = [
  { id: 'fringes', label: 'Fringes' },
  { id: 'groups', label: 'Groups' },
  { id: 'globals', label: 'Globals' },
  { id: 'units', label: 'Units' },
  { id: 'sets', label: 'Sets' },
  { id: 'locations', label: 'Locations' },
  { id: 'currencies', label: 'Currency' },
  { id: 'notes', label: 'Notes' },
  { id: 'budget', label: 'Budget' },
  { id: 'find', label: 'Find' },
];

/** The Budget tab's own tabs. */
const BUDGET_TABS = EXTRA_TABS;

const PANES = ['top', 'bottom'];
const TAB_IDS = TOOL_TABS.map((t) => t.id);
const BUDGET_IDS = BUDGET_TABS.map((t) => t.id);

const DEFAULT_LAYOUT = Object.freeze({
  panes: { top: ['fringes', 'groups', 'globals', 'units', 'find'], bottom: ['sets', 'locations', 'currencies', 'notes', 'budget'] },
  active: { top: 'fringes', bottom: 'sets' },
  budget: 'charges',
});

const POSITIONS = [
  { id: 'right', label: 'Dock at right', icon: 'right' },
  { id: 'bottom', label: 'Dock at bottom', icon: 'bottom' },
  { id: 'window', label: 'Pop out into a window', icon: 'window' },
];

/**
 * A layout with every tab exactly once: unknown tabs dropped, missing ones
 * added to the second pane, each pane's tab one it has.
 */
function normalizeLayout(raw) {
  const src = raw && typeof raw === 'object' ? raw : {};
  const panesIn = src.panes && typeof src.panes === 'object' ? src.panes : {};
  const seen = new Set();
  const panes = {};
  for (const p of PANES) {
    const list = Array.isArray(panesIn[p]) ? panesIn[p] : [];
    panes[p] = list.filter((id) => TAB_IDS.includes(id) && !seen.has(id) && seen.add(id));
  }
  if (!Array.isArray(panesIn.top) && !Array.isArray(panesIn.bottom)) {
    return normalizeLayout(JSON.parse(JSON.stringify(DEFAULT_LAYOUT)));
  }
  for (const id of TAB_IDS) if (!seen.has(id)) panes[DEFAULT_LAYOUT.panes.top.includes(id) ? 'top' : 'bottom'].push(id);
  const activeIn = src.active && typeof src.active === 'object' ? src.active : {};
  const active = {};
  for (const p of PANES) active[p] = panes[p].includes(activeIn[p]) ? activeIn[p] : panes[p][0] || null;
  const budget = BUDGET_IDS.includes(src.budget) ? src.budget : DEFAULT_LAYOUT.budget;
  return { panes, active, budget };
}

let layout = normalizeLayout(readPref('toolsLayout', null));

/** The current tab arrangement (a copy). */
export function toolsLayout() {
  return JSON.parse(JSON.stringify(layout));
}

/** Takes an arrangement from elsewhere (the tools window); true when it changed. */
export function setToolsLayout(raw, { save = true } = {}) {
  const next = normalizeLayout(raw);
  const changed = JSON.stringify(next) !== JSON.stringify(layout);
  layout = next;
  if (changed && save) writePref('toolsLayout', layout);
  return changed;
}

function saveLayout() {
  writePref('toolsLayout', layout);
  if (last && last.ctx && typeof last.ctx.onLayout === 'function') last.ctx.onLayout(toolsLayout());
}

/**
 * Shows `tab` in its pane (a Budget tab: the Budget tab, on that tab). Returns
 * the key of its tab button, to focus once the panel is drawn, or null.
 */
export function selectToolsTab(tab) {
  let id = tab;
  let key = `tools:tab:${tab}`;
  if (BUDGET_IDS.includes(tab)) {
    layout.budget = tab;
    id = 'budget';
    key = `tools:subtab:${tab}`;
  }
  const pane = PANES.find((p) => layout.panes[p].includes(id));
  if (!pane) return null;
  layout.active[pane] = id;
  writePref('toolsLayout', layout);
  return key;
}

/** Focuses the control with data-key `key` in the drawn panel (a tab). */
export function focusToolsKey(key) {
  if (!last || !last.host || !key) return false;
  const t = [...last.host.querySelectorAll('[data-key]')].find((e) => e.dataset.key === key);
  if (!t) return false;
  t.focus();
  if (typeof t.scrollIntoView === 'function') t.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  return true;
}

// The last render's arguments, for redrawing after a tab change.
let last = null;
// True right after a drag, so the click that ends it doesn't select the tab.
let dragged = false;

/** Draws the tools panel into `host` (an element that stays on the page). */
export function renderTools(host, view, ctx) {
  last = { host, view, ctx };
  const position = ctx.position || 'right';
  const focus = setupBegin(host, view, ctx);
  applyView(view, ctx);
  const root = el('div', { class: `tools setup tools-at-${position}` });
  root.append(header(position), panes(view, ctx));
  setupEnd(host, root, focus);
}

function rerender() {
  if (last && last.host && last.host.isConnected) renderTools(last.host, last.view, last.ctx);
}

// ---------------------------------------------------------------------------
// Header: what is selected, and where the panel goes

function header(position) {
  const group = el('div', { class: 'tools-places', attrs: { role: 'group', 'aria-label': 'Tools panel position' } });
  for (const p of POSITIONS) {
    const b = el(
      'button',
      {
        class: 'tools-place',
        title: p.label,
        attrs: { type: 'button', 'aria-label': p.label, 'aria-pressed': String(p.id === position) },
        data: { key: `tools:place:${p.id}`, nobusy: '1' },
        on: {
          click: () => {
            if (p.id !== position && last && typeof last.ctx.setPosition === 'function') last.ctx.setPosition(p.id);
          },
        },
      },
      icon(p.icon),
    );
    group.append(b);
  }
  return el('div', { class: 'tools-head' }, applyHeadHost(), group);
}

/** A 16x16 icon: the window with the panel at the right or bottom, or popped out. */
function icon(kind) {
  const NS = 'http://www.w3.org/2000/svg';
  const svg = document.createElementNS(NS, 'svg');
  const set = (node, attrs) => {
    for (const [k, v] of Object.entries(attrs)) node.setAttribute(k, v);
    return node;
  };
  set(svg, { viewBox: '0 0 16 16', width: '16', height: '16', 'aria-hidden': 'true', focusable: 'false' });
  const shape = (tag, attrs) => svg.append(set(document.createElementNS(NS, tag), attrs));
  const line = { fill: 'none', stroke: 'currentColor', 'stroke-width': '1.3' };
  if (kind === 'window') {
    shape('rect', { x: '1.5', y: '4.5', width: '9', height: '9', rx: '1.2', ...line });
    shape('path', { d: 'M7 2.5h6.5V9', ...line, 'stroke-linecap': 'round', 'stroke-linejoin': 'round' });
    shape('path', { d: 'M13.2 2.8 7.5 8.5', ...line, 'stroke-linecap': 'round' });
  } else {
    shape('rect', { x: '1.5', y: '2.5', width: '13', height: '11', rx: '1.2', ...line });
    if (kind === 'right') shape('rect', { x: '9.5', y: '3', width: '4.5', height: '10', fill: 'currentColor', opacity: '0.55' });
    else shape('rect', { x: '2', y: '9', width: '12', height: '4', fill: 'currentColor', opacity: '0.55' });
  }
  return svg;
}

// ---------------------------------------------------------------------------
// The two panes

function paneLabel(p) {
  return p === 'top' ? 'First tools pane' : 'Second tools pane';
}

function panes(view, ctx) {
  const wrap = el('div', { class: 'tools-panes' });
  for (const p of PANES) wrap.append(pane(p, view, ctx));
  return wrap;
}

function pane(which, view, ctx) {
  const ids = layout.panes[which];
  const active = layout.active[which];
  const sec = el('section', {
    class: `tools-pane${ids.length ? '' : ' is-empty'}`,
    attrs: { 'aria-label': paneLabel(which) },
    data: { pane: which },
  });
  const strip = el('div', {
    class: 'tools-tabs',
    attrs: { role: 'tablist', 'aria-label': paneLabel(which) },
    data: { pane: which },
  });
  for (const id of ids) strip.append(tabButton(which, id, id === active));
  if (!ids.length) strip.append(el('span', { class: 'tools-drop-hint', text: 'Drag a tab here' }));
  sec.append(strip);
  if (active) {
    const body = el('div', {
      class: 'tools-body',
      attrs: { role: 'tabpanel', id: `tools-panel-${which}`, 'aria-labelledby': `tools-tab-${active}` },
    });
    content(body, active, view, ctx);
    sec.append(body);
  }
  return sec;
}

function content(body, id, view, ctx) {
  if (APPLY_LISTS.includes(id)) body.append(applyListHost(id));
  else if (id === 'globals' || id === 'units') renderSetupTab(body, id);
  else if (id === 'notes') {
    const host = el('div', { class: 'notes' });
    renderNotes(host, view, ctx);
    body.append(host);
  } else if (id === 'find') {
    const host = el('div', { class: 'find' });
    renderFind(host, view, ctx);
    body.append(host);
  } else if (id === 'budget') budgetTab(body);
}

function tabLabel(id) {
  const t = TOOL_TABS.find((x) => x.id === id);
  return t ? t.label : id;
}

function tabButton(which, id, selected) {
  const b = el('button', {
    class: 'tools-tab',
    text: tabLabel(id),
    title: `${tabLabel(id)}: drag to move it, or ${isMac ? 'Option' : 'Alt'}+Shift+arrows`,
    attrs: {
      type: 'button',
      role: 'tab',
      id: `tools-tab-${id}`,
      'aria-selected': String(selected),
      'aria-controls': `tools-panel-${which}`,
      tabindex: selected ? '0' : '-1',
      'aria-keyshortcuts': 'Alt+Shift+ArrowLeft Alt+Shift+ArrowRight Alt+Shift+ArrowUp Alt+Shift+ArrowDown',
    },
    data: { key: `tools:tab:${id}`, tab: id, nobusy: '1' },
  });
  b.addEventListener('click', () => {
    if (dragged) {
      dragged = false;
      return;
    }
    showTab(which, id);
  });
  b.addEventListener('keydown', (e) => tabKey(e, which, id));
  b.addEventListener('pointerdown', (e) => startDrag(e, b, id));
  return b;
}

function showTab(which, id, focus = true) {
  const changed = layout.active[which] !== id;
  layout.active[which] = id;
  if (changed) saveLayout();
  // Also when the layout came from elsewhere since the panel was drawn.
  const drawn = last && last.host.querySelector(`.tools-pane[data-pane="${which}"] .tools-tab[aria-selected="true"]`);
  if (changed || !drawn || drawn.dataset.tab !== id) rerender();
  if (focus) focusToolsKey(`tools:tab:${id}`);
}

function tabKey(e, which, id) {
  const ids = layout.panes[which];
  const i = ids.indexOf(id);
  const move = e.altKey && e.shiftKey && !e.metaKey && !e.ctrlKey;
  if (move) {
    let pane = which;
    let at = null;
    if (e.key === 'ArrowLeft') at = Math.max(0, i - 1);
    else if (e.key === 'ArrowRight') at = Math.min(ids.length - 1, i + 1);
    else if (e.key === 'ArrowUp' && which !== 'top') {
      pane = 'top';
      at = layout.panes.top.length;
    } else if (e.key === 'ArrowDown' && which !== 'bottom') {
      pane = 'bottom';
      at = layout.panes.bottom.length;
    }
    if (at === null) return;
    e.preventDefault();
    e.stopPropagation();
    moveTab(id, pane, pane === which && at > i ? at + 1 : at);
    return;
  }
  if (e.altKey || e.metaKey || e.ctrlKey) return;
  let j = null;
  if (e.key === 'ArrowRight') j = (i + 1) % ids.length;
  else if (e.key === 'ArrowLeft') j = (i - 1 + ids.length) % ids.length;
  else if (e.key === 'Home') j = 0;
  else if (e.key === 'End') j = ids.length - 1;
  if (j === null) return;
  e.preventDefault();
  showTab(which, ids[j]);
}

/**
 * Moves tab `id` to pane `to`, before the tab now at `index` (the end when
 * past it), and shows it there.
 */
function moveTab(id, to, index) {
  if (!TAB_IDS.includes(id) || !PANES.includes(to)) return;
  const from = PANES.find((p) => layout.panes[p].includes(id));
  if (!from) return;
  const src = layout.panes[from];
  const at = src.indexOf(id);
  src.splice(at, 1);
  let i = Math.max(0, Math.min(index, layout.panes[to].length + (from === to ? 1 : 0)));
  if (from === to && i > at) i -= 1;
  layout.panes[to].splice(Math.min(i, layout.panes[to].length), 0, id);
  layout.active[to] = id;
  if (from !== to && layout.active[from] === id) layout.active[from] = layout.panes[from][0] || null;
  saveLayout();
  rerender();
  focusToolsKey(`tools:tab:${id}`);
}

// ---------------------------------------------------------------------------
// Dragging a tab (pointer events: HTML drag and drop is unreliable in some
// system web views)

const DRAG_START_PX = 5;

function startDrag(e, b, id) {
  if (e.button !== 0) return;
  dragged = false;
  const x0 = e.clientX;
  const y0 = e.clientY;
  let dragging = false;
  let target = null;
  try {
    b.setPointerCapture(e.pointerId);
  } catch {
    // Not a real pointer (a test); moves still arrive on the document.
  }
  const move = (ev) => {
    if (!dragging) {
      if (Math.hypot(ev.clientX - x0, ev.clientY - y0) < DRAG_START_PX) return;
      dragging = true;
      b.classList.add('is-dragging');
      document.body.classList.add('tools-dragging');
    }
    target = dropTarget(ev.clientX, ev.clientY, id);
    paintDrop(target);
  };
  const end = () => {
    b.removeEventListener('pointermove', move);
    b.removeEventListener('pointerup', end);
    b.removeEventListener('pointercancel', cancel);
    document.body.classList.remove('tools-dragging');
    b.classList.remove('is-dragging');
    paintDrop(null);
    if (!dragging) return;
    dragged = true;
    // A click follows the drag on the same button; if none does, forget it.
    setTimeout(() => {
      dragged = false;
    }, 0);
    if (target) moveTab(id, target.pane, target.index);
  };
  const cancel = () => {
    target = null;
    end();
  };
  b.addEventListener('pointermove', move);
  b.addEventListener('pointerup', end);
  b.addEventListener('pointercancel', cancel);
}

/** Where a tab dropped at (x, y) goes: { pane, index, el, side }, or null. */
function dropTarget(x, y, id) {
  const hit = typeof document.elementFromPoint === 'function' ? document.elementFromPoint(x, y) : null;
  if (!hit || !last || !last.host.contains(hit)) return null;
  const pane = hit.closest('.tools-pane');
  if (!pane) return null;
  const which = pane.dataset.pane;
  const tab = hit.closest('.tools-tab');
  const ids = layout.panes[which];
  if (tab && tab.dataset.tab !== id) {
    const r = tab.getBoundingClientRect();
    const after = x > r.left + r.width / 2;
    const i = ids.indexOf(tab.dataset.tab);
    return { pane: which, index: after ? i + 1 : i, el: tab, side: after ? 'after' : 'before' };
  }
  if (tab) return null; // onto itself: nothing to do
  return { pane: which, index: ids.length, el: pane.querySelector('.tools-tabs'), side: 'end' };
}

function paintDrop(target) {
  if (!last || !last.host) return;
  for (const n of last.host.querySelectorAll('.drop-before, .drop-after, .drop-end')) {
    n.classList.remove('drop-before', 'drop-after', 'drop-end');
  }
  if (target && target.el) target.el.classList.add(`drop-${target.side}`);
}

// ---------------------------------------------------------------------------
// The Budget tab and its own tabs

function budgetTab(body) {
  const sub = BUDGET_IDS.includes(layout.budget) ? layout.budget : BUDGET_IDS[0];
  const strip = el('div', { class: 'tools-subtabs', attrs: { role: 'tablist', 'aria-label': 'Budget' } });
  BUDGET_TABS.forEach((t, i) => {
    const on = t.id === sub;
    const b = el('button', {
      class: 'tools-subtab',
      text: t.label,
      attrs: {
        type: 'button',
        role: 'tab',
        id: `tools-subtab-${t.id}`,
        'aria-selected': String(on),
        'aria-controls': 'tools-subpanel',
        tabindex: on ? '0' : '-1',
      },
      data: { key: `tools:subtab:${t.id}`, nobusy: '1' },
    });
    const go = (id) => {
      if (layout.budget !== id) {
        layout.budget = id;
        saveLayout();
        rerender();
      }
      focusToolsKey(`tools:subtab:${id}`);
    };
    b.addEventListener('click', () => go(t.id));
    b.addEventListener('keydown', (e) => {
      if (e.altKey || e.metaKey || e.ctrlKey) return;
      const n = BUDGET_TABS.length;
      let j = null;
      if (e.key === 'ArrowRight') j = (i + 1) % n;
      else if (e.key === 'ArrowLeft') j = (i - 1 + n) % n;
      else if (e.key === 'Home') j = 0;
      else if (e.key === 'End') j = n - 1;
      if (j === null) return;
      e.preventDefault();
      go(BUDGET_TABS[j].id);
    });
    strip.append(b);
  });
  const panel = el('div', {
    class: 'tools-subpanel',
    attrs: { role: 'tabpanel', id: 'tools-subpanel', 'aria-labelledby': `tools-subtab-${sub}` },
  });
  renderSetupTab(panel, sub);
  body.append(strip, panel);
}
