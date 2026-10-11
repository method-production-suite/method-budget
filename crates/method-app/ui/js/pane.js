// Split view (roadmap 2.26): the window shows one pane, or two side by side,
// each with its own tab strip, breadcrumb, level buttons, grid and footer
// totals (decisions.md, entry 40). main.js keeps what each pane shows in a
// Pane (its budget, level, category, account, cursor memory, footer counters,
// grid and tab strip); one pane is the target, which the toolbar, the menus,
// the keys, the tools panel and the formula bar act on.
//
// This file holds the pane itself and the page around the panes: the first
// pane is the page's own elements (index.html); the second is made here the
// first time the window splits. Split, the first pane's tab strip and footer
// move into a column beside the second pane's, with a divider between them
// that can be dragged (or moved with the arrow keys; the split is saved per
// user). Closing the split puts every element back where index.html has it,
// so one pane looks exactly as it did before split view.
//
// Nothing here uses inline styles: the divider's place is a generated rule
// (sheet.js) with numbers only.

import { el, $ } from './dom.js';
import { setRules, dynamicStyles } from './sheet.js';
import { readPref, writePref } from './prefs.js';

/** What one pane shows, and its elements. */
export class Pane {
  constructor(index, els) {
    this.index = index;
    this.root = els.root || null; // the pane's own column (split only for the first)
    this.tabbar = els.tabbar;
    this.main = els.main;
    this.crumbs = els.crumbs;
    this.banner = els.banner;
    this.actions = els.actions;
    this.host = els.host; // the grid's host
    this.footer = els.footer;
    // Where the pane is (main.js reads these through its `state`).
    this.st = {
      doc: null, // the budget in front in this pane
      view: null,
      ix: null,
      level: 'topsheet',
      cat: null,
      acct: null,
      selection: null,
      memory: {},
      changes: null,
    };
    // Where each budget not in front here was left: doc id -> { level, ... }.
    this.tabMemory = new Map();
    this.grid = null; // main.js makes it
    this.strip = null; // main.js makes it (tabs.js)
  }

  /** True when `node` is inside this pane. */
  contains(node) {
    if (!node) return false;
    if (this.root) return this.root.contains(node);
    return [this.tabbar, this.main, this.footer].some((e) => e && e.contains(node));
  }
}

/** The first pane: the page's own elements. */
export function firstPane() {
  return new Pane(0, {
    tabbar: $('tabbar'),
    main: document.querySelector('.workspace > .main'),
    crumbs: $('crumbs'),
    banner: $('banner'),
    actions: $('actions'),
    host: $('grid-host'),
    footer: $('footer'),
  });
}

/** The second pane's elements (made the first time the window splits). */
export function secondPane() {
  const tabbar = el('div', { class: 'tabbar', attrs: { 'aria-label': 'Budgets in the right pane' } });
  const crumbs = el('nav', { class: 'crumbs', attrs: { 'aria-label': 'Budget level, right pane' } });
  const banner = el('div', { class: 'banner', attrs: { role: 'status' } });
  banner.hidden = true;
  const actions = el('div', { class: 'actions' });
  const host = el('div', { class: 'grid-host', attrs: { 'aria-label': 'Budget grid, right pane' } });
  const main = el('div', { class: 'main' }, crumbs, banner, actions, host);
  const footer = el('footer', { class: 'footer' });
  const root = el('section', { class: 'pane pane-1', attrs: { 'aria-label': 'Right pane' } }, tabbar, main, footer);
  return new Pane(1, { root, tabbar, main, crumbs, banner, actions, host, footer });
}

// ---------------------------------------------------------------------------
// The page around the panes

let box = null; // .panes: both panes and the divider
let firstColumn = null; // the first pane's column, split
let divider = null;

/** True when the window shows two panes. */
export function isSplitLayout() {
  return Boolean(box && box.isConnected);
}

/**
 * Lays the page out for one pane (`second` null) or two. Moving elements
 * keeps them (and the grids in them) as they are.
 */
export function layoutPanes(first, second) {
  const app = document.querySelector('.app');
  const ws = $('workspace');
  if (!app || !ws) return;
  if (second) {
    if (!box) build();
    if (isSplitLayout() && second.root.parentNode === box) return;
    firstColumn.append(first.tabbar, first.main, first.footer);
    first.root = firstColumn;
    box.replaceChildren(firstColumn, divider, second.root);
    ws.prepend(box);
    app.classList.add('is-split');
    applyRatio();
    return;
  }
  if (!isSplitLayout()) return;
  // One pane: the strip under the toolbar, the grid in the workspace, the
  // totals along the bottom, as index.html has them.
  const toolbar = app.querySelector('.toolbar');
  if (toolbar) toolbar.after(first.tabbar);
  ws.prepend(first.main);
  moveFormulaBar(first);
  app.append(first.footer);
  box.remove();
  first.root = null;
  app.classList.remove('is-split');
}

/** Marks the target pane (split only): an accent on its strip, its tab in front bold. */
export function markTarget(panes, target) {
  const split = isSplitLayout();
  for (const p of panes) {
    if (p && p.root) p.root.classList.toggle('is-target', split && p === target);
  }
}

// The one formula bar (index.html), kept: it may be in a pane that is off the page.
let formulaBar = null;

/** Puts the one formula bar in the target pane, above its grid. */
export function moveFormulaBar(pane) {
  if (!formulaBar) formulaBar = $('formula-bar');
  const bar = formulaBar;
  if (!bar || !pane || !pane.main || !pane.host) return;
  if (bar.parentNode === pane.main && bar.nextSibling === pane.host) return;
  pane.main.insertBefore(bar, pane.host);
}

// ---------------------------------------------------------------------------
// The divider: the share of the width the first pane gets (saved per user)

const RATIO_MIN = 0.2;
const RATIO_MAX = 0.8;
let ratio = readRatio();

function readRatio() {
  const r = readPref('splitRatio', 0.5);
  return typeof r === 'number' && Number.isFinite(r) ? clamp(r) : 0.5;
}

function clamp(r) {
  return Math.min(RATIO_MAX, Math.max(RATIO_MIN, r));
}

function applyRatio() {
  const a = Math.round(ratio * 1000) / 1000;
  const b = Math.round((1 - ratio) * 1000) / 1000;
  setRules('split-ratio', `.panes > .pane-0 { flex-grow: ${a}; } .panes > .pane-1 { flex-grow: ${b}; }`);
  if (divider) {
    divider.setAttribute('aria-valuenow', String(Math.round(ratio * 100)));
    divider.hidden = !dynamicStyles;
  }
}

/** Sets the first pane's share (null: back to half). */
export function setRatio(r, { save = false } = {}) {
  ratio = r === null ? 0.5 : clamp(r);
  applyRatio();
  if (save) writePref('splitRatio', ratio);
}

function build() {
  box = el('div', { class: 'panes', attrs: { id: 'panes' } });
  firstColumn = el('section', { class: 'pane pane-0', attrs: { 'aria-label': 'Left pane' } });
  divider = el('div', {
    class: 'pane-divider',
    title: 'Drag to change how the panes share the width; double-click to split it evenly',
    attrs: {
      role: 'separator',
      tabindex: '0',
      'aria-orientation': 'vertical',
      'aria-label': 'Split between the panes',
      'aria-valuemin': String(RATIO_MIN * 100),
      'aria-valuemax': String(RATIO_MAX * 100),
      'aria-controls': 'panes',
    },
  });
  divider.addEventListener('pointerdown', (e) => {
    if (e.button !== 0) return;
    e.preventDefault();
    try {
      divider.setPointerCapture(e.pointerId);
    } catch {
      // Not a real pointer (a test); moves still arrive.
    }
    document.body.classList.add('panes-resizing');
    const move = (ev) => {
      const r = box.getBoundingClientRect();
      if (r.width > 0) setRatio((ev.clientX - r.left) / r.width);
    };
    const end = () => {
      divider.removeEventListener('pointermove', move);
      divider.removeEventListener('pointerup', end);
      divider.removeEventListener('pointercancel', end);
      document.body.classList.remove('panes-resizing');
      writePref('splitRatio', ratio);
    };
    divider.addEventListener('pointermove', move);
    divider.addEventListener('pointerup', end);
    divider.addEventListener('pointercancel', end);
  });
  divider.addEventListener('dblclick', () => setRatio(null, { save: true }));
  // Left and Right move it (Shift for bigger steps), Home and End to the
  // ends, Enter or a double-click back to half.
  divider.addEventListener('keydown', (e) => {
    const step = e.shiftKey ? 0.1 : 0.02;
    let next = null;
    if (e.key === 'ArrowLeft') next = ratio - step;
    else if (e.key === 'ArrowRight') next = ratio + step;
    else if (e.key === 'Home') next = RATIO_MIN;
    else if (e.key === 'End') next = RATIO_MAX;
    else if (e.key === 'Enter') next = 0.5;
    if (next === null) return;
    e.preventDefault();
    e.stopPropagation();
    setRatio(next, { save: true });
  });
}
