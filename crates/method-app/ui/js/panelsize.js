// The docked tools panel's size (main.js docks it; tools.js draws it): drag
// its edge, or use the arrow keys on the edge, saved per user. Its width at
// the right, its height at the bottom. The sizes are generated rules
// (sheet.js) holding numbers only; where constructed stylesheets aren't
// available the edge is hidden and the standard sizes in the CSS apply.

import { el, $ } from './dom.js';
import { setRules, dynamicStyles } from './sheet.js';
import { readPref, writePref } from './prefs.js';
import { t } from './i18n.js';

const SIZES = {
  // styles.css .panel and tools.css .workspace.tools-bottom > .panel
  right: { pref: 'panelWidth', rule: 'panel-width', fallback: 380, min: 260, max: () => Math.min(760, window.innerWidth - 480) },
  bottom: { pref: 'panelHeight', rule: 'panel-height', fallback: 300, min: 160, max: () => window.innerHeight - 260 },
};
let resizer = null;
let ctx = null; // { position(), focusGrid() }
const panelSize = { right: null, bottom: null }; // null: the default size

function sizeOf(where) {
  return SIZES[where === 'bottom' ? 'bottom' : 'right'];
}

function panelMax(where) {
  const s = sizeOf(where);
  return Math.max(s.min, s.max());
}

/** Sets the docked panel's size (null: back to the default). Saves it when `save`. */
function setPanelSize(where, n, { save = false } = {}) {
  const s = sizeOf(where);
  const key = where === 'bottom' ? 'bottom' : 'right';
  const v = n === null ? null : Math.round(Math.max(s.min, Math.min(n, panelMax(where))));
  panelSize[key] = v;
  const rule = key === 'bottom' ? `.workspace.tools-bottom > .panel { height: ${v}px; }` : `.workspace > .panel { width: ${v}px; }`;
  setRules(s.rule, v === null ? '' : rule);
  if (save) writePref(s.pref, v);
  syncResizer();
}

function currentPanelSize(where) {
  const key = where === 'bottom' ? 'bottom' : 'right';
  if (panelSize[key] !== null) return panelSize[key];
  const r = $('panel').getBoundingClientRect();
  return (key === 'bottom' ? r.height : r.width) || sizeOf(where).fallback;
}

/** Reapplies the saved sizes (the window was resized, or the panel moved). */
export function sizePanel() {
  for (const k of ['right', 'bottom']) if (panelSize[k] !== null) setPanelSize(k, panelSize[k]);
  syncResizer();
}

/** The edge's role, label and numbers follow where the panel is. */
export function syncResizer() {
  if (!resizer) return;
  const panel = $('panel');
  resizer.hidden = panel.hidden || !dynamicStyles;
  const bottom = ctx.position() === 'bottom';
  const s = sizeOf(ctx.position());
  const key = bottom ? 'bottom' : 'right';
  resizer.setAttribute('aria-orientation', bottom ? 'horizontal' : 'vertical');
  resizer.setAttribute('aria-label', bottom ? t('panelsize.height_label') : t('panelsize.width_label'));
  resizer.title = bottom ? t('panelsize.height_title') : t('panelsize.width_title');
  resizer.setAttribute('aria-valuemin', String(s.min));
  resizer.setAttribute('aria-valuemax', String(panelMax(ctx.position())));
  resizer.setAttribute('aria-valuenow', String(panelSize[key] === null ? s.fallback : panelSize[key]));
}

/** True when `node` is the panel's edge. */
export function isResizer(node) {
  return resizer !== null && node === resizer;
}

/**
 * Puts the draggable edge before #panel, at the saved sizes. `c`: {
 * position() ('right', 'bottom' or 'window'), focusGrid() (Escape on the
 * edge) }.
 */
export function wirePanelResizer(c) {
  ctx = c;
  resizer = el('div', {
    class: 'panel-resizer',
    attrs: { role: 'separator', tabindex: '0', 'aria-controls': 'panel' },
  });
  $('panel').before(resizer);
  for (const k of ['right', 'bottom']) {
    const saved = readPref(SIZES[k].pref, null);
    if (typeof saved === 'number' && Number.isFinite(saved)) setPanelSize(k, saved);
  }
  syncResizer();

  resizer.addEventListener('pointerdown', (e) => {
    if (e.button !== 0) return;
    e.preventDefault();
    const where = ctx.position();
    const bottom = where === 'bottom';
    const start = bottom ? e.clientY : e.clientX;
    const startSize = currentPanelSize(where);
    resizer.setPointerCapture(e.pointerId);
    document.body.classList.add(bottom ? 'panel-resizing-y' : 'panel-resizing');
    // The panel grows as its edge moves away from it: left at the right, up at the bottom.
    const move = (ev) => setPanelSize(where, startSize + (start - (bottom ? ev.clientY : ev.clientX)));
    const end = () => {
      resizer.removeEventListener('pointermove', move);
      resizer.removeEventListener('pointerup', end);
      resizer.removeEventListener('pointercancel', end);
      document.body.classList.remove('panel-resizing', 'panel-resizing-y');
      const key = bottom ? 'bottom' : 'right';
      if (panelSize[key] !== null) writePref(sizeOf(where).pref, panelSize[key]);
    };
    resizer.addEventListener('pointermove', move);
    resizer.addEventListener('pointerup', end);
    resizer.addEventListener('pointercancel', end);
  });
  resizer.addEventListener('dblclick', () => setPanelSize(ctx.position(), null, { save: true }));
  // At the right, Left makes the panel wider (the edge moves left) and Right
  // narrower; at the bottom, Up makes it taller and Down shorter. Shift for
  // bigger steps; Home and End for the smallest and biggest; Escape to the grid.
  resizer.addEventListener('keydown', (e) => {
    const where = ctx.position();
    const bottom = where === 'bottom';
    const step = e.shiftKey ? 80 : 20;
    const n = currentPanelSize(where);
    const grow = bottom ? 'ArrowUp' : 'ArrowLeft';
    const shrink = bottom ? 'ArrowDown' : 'ArrowRight';
    let next = null;
    if (e.key === grow) next = n + step;
    else if (e.key === shrink) next = n - step;
    else if (e.key === 'Home') next = sizeOf(where).min;
    else if (e.key === 'End') next = panelMax(where);
    else if (e.key === 'Escape') {
      e.preventDefault();
      ctx.focusGrid();
      return;
    } else return;
    e.preventDefault();
    e.stopPropagation();
    setPanelSize(where, next, { save: true });
  });
  window.addEventListener('resize', sizePanel);
}
