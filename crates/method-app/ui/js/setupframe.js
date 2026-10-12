// The shared layout of a Budget Setup tab (setupwin.js; the classes are in
// setup.css, documented at its top). A tab is drawn as one frame filling the
// window's main area, top to bottom:
//
//   .sw-strip   the action strip: the tab's actions, a search box that
//               takes the rest of the row, and Undo and Redo at its end
//   .sw-band    a thin band of tab-wide settings (optional)
//   .sw-scroll  what the tab lists (a .sw-grid), scrolling on its own
//
// and the window's footer (.sw-foot, drawn by setupwin.js) shows the tab's
// totals at the left and Close at the right. A tab sets its totals with
// setFooterTotals() whenever it draws; setupwin.js clears them before each
// tab is drawn, so a tab without totals shows none.
//
// The left rail (setupwin.js) has a Budget and Library switch at the top;
// under it, in Budget, a tab can put its own content (railHost(): a list
// of the tab's items, say) and the list tabs show their sort and Show.
//
// setupwin.js gives the hooks (setFrameHooks): menu(id) sends a menu
// command to the main window (Undo, Redo), close() closes the window and
// view() is the view drawn. Elements are made with dom.js el(): text only
// through textContent, styling only through classes.

import { el } from './dom.js';

const hooks = { menu: null, close: null, view: null };

/** setupwin.js: { menu(id), close(), view() }. */
export function setFrameHooks(h) {
  Object.assign(hooks, h || {});
}

// ---------------------------------------------------------------------------
// The frame

/**
 * Fills `panel` (the tab's main area) with the frame: `strip` (actionStrip),
 * `band` (or null), then `body` in the scrolling area, and anything in
 * `after` below it in the same scroll. Returns the scrolling element.
 */
export function mountFrame(panel, { strip = null, band = null, body = [], after = [] } = {}) {
  panel.classList.add('sw-framed');
  const scroll = el('div', { class: 'sw-scroll' }, ...[].concat(body), ...[].concat(after));
  panel.append(el('div', { class: 'sw-frame' }, strip, band, scroll));
  return scroll;
}

// ---------------------------------------------------------------------------
// The action strip

/**
 * A strip button: `key` is its data-key (focus comes back to it after a
 * redraw). A disabled one is aria-disabled, so it keeps focus and says why
 * in its title.
 */
export function stripButton(text, key, onClick, { title = '', label = '', enabled = true, primary = false } = {}) {
  const b = el('button', {
    class: `sw-act${primary ? ' sw-act-primary' : ''}`,
    text,
    title: title || undefined,
    attrs: { type: 'button', 'aria-label': label || null },
    data: { key, nobusy: '1' },
  });
  if (!enabled) b.setAttribute('aria-disabled', 'true');
  b.addEventListener('click', () => {
    if (b.getAttribute('aria-disabled') === 'true') return;
    onClick(b);
  });
  return b;
}

/**
 * The search box: filters as it is typed (`onChange(text)`); Escape clears
 * it when it has text (else Escape goes on and closes the window).
 */
export function searchBox({ key, value = '', label, placeholder = 'Search', onChange }) {
  const input = el('input', {
    class: 'setup-input sw-search',
    attrs: {
      type: 'text',
      placeholder,
      'aria-label': label,
      autocomplete: 'off',
      spellcheck: 'false',
      autocapitalize: 'off',
      autocorrect: 'off',
    },
    data: { key },
  });
  input.value = value;
  input.addEventListener('input', (e) => {
    if (!e.isComposing) onChange(input.value);
  });
  input.addEventListener('compositionend', () => onChange(input.value));
  input.addEventListener('keydown', (e) => {
    if (e.key === 'Escape' && input.value) {
      e.preventDefault();
      e.stopPropagation();
      input.value = '';
      onChange('');
    }
  });
  return input;
}

function undoRedo() {
  const v = typeof hooks.view === 'function' ? hooks.view() || {} : {};
  const send = (id) => () => {
    if (typeof hooks.menu === 'function') hooks.menu(id);
  };
  return el(
    'div',
    { class: 'sw-strip-end', attrs: { role: 'group', 'aria-label': 'Undo and redo' } },
    stripButton('Undo', 'sw:undo', send('edit.undo'), { title: 'Undo the last change to this budget', enabled: v.can_undo !== false }),
    stripButton('Redo', 'sw:redo', send('edit.redo'), { title: 'Redo the change undone last', enabled: v.can_redo !== false }),
  );
}

/**
 * The action strip: `actions` (stripButton()s and the like) at the left,
 * `search` (searchBox(), or null) taking the rest of the row, then Undo and
 * Redo. `label` names the toolbar for screen readers.
 */
export function actionStrip(label, actions = [], search = null) {
  return el(
    'div',
    { class: 'sw-strip', attrs: { role: 'toolbar', 'aria-label': label } },
    el('div', { class: 'sw-strip-actions' }, ...actions),
    search || el('span', { class: 'sw-strip-gap' }),
    undoRedo(),
  );
}

// ---------------------------------------------------------------------------
// The table

/**
 * A dense table: the column heads `head` (cells, in order; sortable ones
 * are setuplist.js sortButton()s) over `list`, a ul whose li.sw-grid-row
 * rows hold the same cells. `cols` names the column template: the class
 * sw-cols-<cols> sets it (setup.css, or a tab's own stylesheet). The heads
 * stay in view while the rows scroll.
 */
export function gridTable(cols, head, list) {
  list.classList.add('sw-grid');
  return el('div', { class: `sw-table sw-cols-${cols}` }, el('div', { class: 'sw-grid-head' }, ...head), list);
}

// ---------------------------------------------------------------------------
// The band of tab-wide settings

/** A thin band: `fields` (bandField()s, notes) in one row. */
export function band(label, ...fields) {
  return el('div', { class: 'sw-band', attrs: { role: 'group', 'aria-label': label } }, ...fields);
}

/** One setting in a band: its label, then the control. */
export function bandField(text, control, title = '') {
  if (control.id === '') control.id = nextId();
  return el(
    'span',
    { class: 'sw-band-field', title: title || undefined },
    el('label', { class: 'sw-band-label', text, attrs: { for: control.id } }),
    control,
  );
}

let uid = 0;
function nextId() {
  uid += 1;
  return `sw-f${uid}`;
}

// ---------------------------------------------------------------------------
// The footer's totals

let footItems = [];
let footHost = null;

function drawTotals() {
  if (!footHost) return;
  footHost.replaceChildren(
    ...footItems.map((t) =>
      el(
        'span',
        { class: `sw-foot-stat${t.dim ? ' is-dim' : ''}`, title: t.title || undefined },
        t.label ? el('span', { class: 'sw-foot-label', text: t.label }) : null,
        el('span', { class: 'sw-foot-value', text: t.value === undefined || t.value === null ? '' : String(t.value) }),
      ),
    ),
  );
}

/**
 * The tab's totals in the footer: `items` is [{ label, value, title, dim }]
 * (a blank label shows the value alone, like "6 fringes"; `dim` marks a
 * figure as not current, like a total while its value is being typed).
 * Shown at once, and kept until the tab sets others or setupwin.js draws
 * another tab.
 */
export function setFooterTotals(items) {
  footItems = Array.isArray(items) ? items.filter(Boolean) : [];
  drawTotals();
}

/** setupwin.js, before a tab is drawn: no totals until it sets some. */
export function clearFooterTotals() {
  footItems = [];
}

/** setupwin.js: the footer, with the totals set so far and Close at the right. */
export function footerBar() {
  footHost = el('div', { class: 'sw-foot-totals', attrs: { role: 'group', 'aria-label': 'Totals' } });
  drawTotals();
  const close = el('button', {
    class: 'sw-act sw-foot-close',
    text: 'Close',
    title: 'Close Budget Setup (Escape)',
    attrs: { type: 'button' },
    data: { key: 'setup:close', nobusy: '1' },
  });
  close.addEventListener('click', () => {
    if (typeof hooks.close === 'function') hooks.close();
  });
  return el('footer', { class: 'sw-foot' }, footHost, close);
}

// ---------------------------------------------------------------------------
// The rail

let railEl = null;

/** setupwin.js: where the drawn tab may put its own rail content (null for none). */
export function setRailHost(node) {
  railEl = node || null;
}

/**
 * The left rail's Budget content for the tab being drawn, for the tab to
 * fill (a list of its items, say); null when the tab has no rail or the
 * rail shows the library.
 */
export function railHost() {
  return railEl;
}
