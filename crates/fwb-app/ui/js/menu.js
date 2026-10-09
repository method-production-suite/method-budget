// A small popup menu, opened from a button: the breadcrumb's jump lists and
// the detail grid's column chooser. And the grid's right-click menu
// (openContextMenu, at the end of this file), which adds separators,
// shortcut hints, disabled items and submenus.
//
//   openMenu(anchor, { owner, label, items, checkable, onPick, returnFocus })
//     items: [{ id, label, title, checked }]
//     checkable false: a list to pick one from (Enter or a click picks and closes)
//     checkable true: on/off choices (Enter, Space or a click toggles; stays open)
//   updateMenu(items)  redraws the open menu's items, keeping focus
//   setMenuAnchor(owner, button)  the button was redrawn: follow the new one
//   closeMenu(owner)   closes it (only if `owner` opened it, when given)
//
// Keys: Up/Down, Home/End, a typed prefix to jump, Enter or Space to choose,
// Escape (or Tab) to close and go back to the button. Keys the menu doesn't use
// reach the app as usual. One menu is open at a time.

import { el } from './dom.js';
import { setRules } from './sheet.js';

let open = null; // { root, anchor, opts, items, typed, typedAt }

export function isMenuOpen(owner) {
  return Boolean(open && (!owner || open.opts.owner === owner));
}

export function closeMenu(owner, { focusAnchor = false } = {}) {
  if (!open || (owner && open.opts.owner !== owner)) return;
  const { root, anchor, opts } = open;
  open = null;
  root.remove();
  document.removeEventListener('mousedown', onOutside, true);
  window.removeEventListener('resize', onWindowChange);
  window.removeEventListener('blur', onWindowChange);
  if (anchor.isConnected) anchor.setAttribute('aria-expanded', 'false');
  if (focusAnchor) {
    const target = typeof opts.returnFocus === 'function' ? opts.returnFocus() : anchor;
    if (target && target.isConnected) target.focus();
  }
}

function onOutside(e) {
  if (!open) return;
  if (open.root.contains(e.target) || open.anchor.contains(e.target)) return;
  closeMenu();
}

function onWindowChange() {
  closeMenu();
}

function itemButtons() {
  return open ? [...open.root.querySelectorAll('.popup-item')] : [];
}

function focusIndex(i) {
  const items = itemButtons();
  if (!items.length) return;
  const n = items.length;
  items[((i % n) + n) % n].focus();
}

function currentIndex() {
  return itemButtons().indexOf(document.activeElement);
}

function choose(btn) {
  if (!open || !btn) return;
  const item = open.items.find((it) => String(it.id) === btn.dataset.id);
  if (!item) return;
  const { opts } = open;
  if (!opts.checkable) closeMenu();
  opts.onPick(item);
}

function onKey(e) {
  if (!open) return;
  const i = currentIndex();
  let handled = true;
  switch (e.key) {
    case 'ArrowDown':
      focusIndex(i + 1);
      break;
    case 'ArrowUp':
      focusIndex(i < 0 ? -1 : i - 1);
      break;
    case 'Home':
      focusIndex(0);
      break;
    case 'End':
      focusIndex(-1);
      break;
    case 'Enter':
    case ' ':
      choose(document.activeElement && document.activeElement.closest('.popup-item'));
      break;
    case 'Escape':
      closeMenu(null, { focusAnchor: true });
      break;
    case 'Tab':
      // Back to the button; the browser then moves on from there.
      closeMenu(null, { focusAnchor: true });
      handled = false;
      break;
    default:
      handled = typeAhead(e);
  }
  if (handled) {
    e.preventDefault();
    e.stopPropagation();
  } else if (e.key === 'Tab') {
    e.stopPropagation();
  }
}

/** Typing letters or digits jumps to the next item starting with them. */
function typeAhead(e) {
  if (e.key.length !== 1 || e.metaKey || e.ctrlKey || e.altKey || !e.key.trim()) return false;
  const now = performance.now();
  open.typed = now - open.typedAt < 700 ? open.typed + e.key.toLowerCase() : e.key.toLowerCase();
  open.typedAt = now;
  const items = itemButtons();
  const from = Math.max(0, currentIndex());
  for (let k = 0; k < items.length; k++) {
    const idx = (from + (open.typed.length > 1 ? 0 : 1) + k) % items.length;
    if (items[idx].dataset.label.toLowerCase().startsWith(open.typed)) {
      items[idx].focus();
      break;
    }
  }
  return true;
}

function drawItems() {
  const { opts } = open;
  const role = opts.checkable ? 'menuitemcheckbox' : 'menuitemradio';
  const nodes = open.items.map((it) =>
    el(
      'button',
      {
        class: `popup-item${it.checked ? ' checked' : ''}`,
        title: it.title,
        attrs: { type: 'button', role, 'aria-checked': String(Boolean(it.checked)), tabindex: '-1' },
        data: { id: it.id, label: it.label },
        on: { click: (e) => choose(e.currentTarget) },
      },
      el('span', { class: 'popup-check', text: it.checked ? '✓' : '', attrs: { 'aria-hidden': 'true' } }),
      el('span', { class: 'popup-label', text: it.label }),
    ),
  );
  open.root.replaceChildren(...nodes);
}

/** Puts the menu under its button, inside the window. */
function place(anchor) {
  const r = anchor.getBoundingClientRect();
  const w = Math.min(open.root.offsetWidth || 240, window.innerWidth - 16);
  const left = Math.round(Math.max(8, Math.min(r.left, window.innerWidth - w - 8)));
  const top = Math.round(r.bottom + 4);
  const maxH = Math.max(120, Math.round(window.innerHeight - top - 12));
  setRules('popup-menu', `.popup-menu.placed { left: ${left}px; top: ${top}px; max-height: ${maxH}px; transform: none; }`);
  open.root.classList.add('placed');
}

export function openMenu(anchor, opts) {
  const reopen = open && open.anchor === anchor;
  closeContextMenu();
  closeMenu();
  if (reopen) return; // a second click on the button closes it
  const root = el('div', {
    class: 'popup-menu',
    attrs: { role: 'menu', 'aria-label': opts.label || '' },
    on: { keydown: onKey },
  });
  open = { root, anchor, opts, items: opts.items, typed: '', typedAt: 0 };
  drawItems();
  document.body.append(root);
  place(anchor);
  anchor.setAttribute('aria-expanded', 'true');
  document.addEventListener('mousedown', onOutside, true);
  window.addEventListener('resize', onWindowChange);
  window.addEventListener('blur', onWindowChange);
  const items = itemButtons();
  const start = items.findIndex((b) => b.getAttribute('aria-checked') === 'true');
  const target = opts.checkable || start < 0 ? items[0] : items[start];
  if (target) {
    target.focus();
    target.scrollIntoView({ block: 'nearest' });
  }
}

/** Moves the open menu to a new button (its old one was redrawn). */
export function setMenuAnchor(owner, anchor) {
  if (!open || open.opts.owner !== owner) return;
  open.anchor = anchor;
  anchor.setAttribute('aria-expanded', 'true');
}

/** Redraws the open menu's items (after a toggle), keeping the focused one. */
export function updateMenu(items) {
  if (!open) return;
  const focused = document.activeElement && document.activeElement.dataset ? document.activeElement.dataset.id : null;
  open.items = items;
  drawItems();
  const again = itemButtons().find((b) => b.dataset.id === focused);
  if (again) again.focus();
}

// ---------------------------------------------------------------------------
// Context menus: the grid's right-click menu (context.js), opened at a point.
//
//   openContextMenu({ x, y }, { label, items, returnFocus })
//     items: [{ label, shortcut, checked, disabled, swatch, title, run, items }]
//       shortcut  text shown at the right ("⌘B")
//       checked   true, false or 'mixed' (a check box item); undefined: a plain item
//       swatch    a palette id: a color chip before the label
//       run()     what choosing it does (after the menu has closed)
//       items     a submenu, opened by Right, Enter, a click or hovering
//     { separator: true } draws a rule.
//   closeContextMenu({ focusBack })   focusBack calls returnFocus()
//   isContextMenuOpen()
//
// Keys: Up and Down (passing over rules and disabled items), Home, End, Right
// or Enter opens a submenu, Left closes it, Enter or Space chooses, a letter
// jumps, Escape closes one level (the menu itself at the top), Tab closes it.
// A key with Command or Ctrl closes the menu and goes on to the app, so its
// shortcuts work from here too.

let cm = null; // { levels: [{ root, items, depth }], opts, hoverTimer }

export function isContextMenuOpen() {
  return cm !== null;
}

export function closeContextMenu({ focusBack = false } = {}) {
  if (!cm) return;
  const { levels, opts } = cm;
  clearTimeout(cm.hoverTimer);
  cm = null;
  for (const lv of levels) lv.root.remove();
  for (let d = 0; d < levels.length; d++) setRules(`ctx-menu-${d}`, '');
  document.removeEventListener('mousedown', onContextOutside, true);
  window.removeEventListener('resize', onContextWindow);
  window.removeEventListener('blur', onContextWindow);
  if (focusBack && typeof opts.returnFocus === 'function') opts.returnFocus();
}

function onContextOutside(e) {
  if (!cm) return;
  if (cm.levels.some((lv) => lv.root.contains(e.target))) return;
  closeContextMenu();
}

function onContextWindow() {
  closeContextMenu();
}

/** Closes the submenus below `depth`. */
function trimLevels(depth) {
  if (!cm) return;
  while (cm.levels.length > depth + 1) {
    const lv = cm.levels.pop();
    lv.root.remove();
    setRules(`ctx-menu-${lv.depth}`, '');
    if (lv.parent) lv.parent.setAttribute('aria-expanded', 'false');
  }
}

function usable(it) {
  return Boolean(it) && !it.separator && !it.disabled;
}

function levelButtons(lv) {
  return [...lv.root.querySelectorAll(':scope > .popup-item')];
}

function itemOf(lv, btn) {
  return btn ? lv.items[Number(btn.dataset.index)] : null;
}

/** Focuses the next usable item from `from` in direction `d` (wrapping). */
function stepFocus(lv, from, d) {
  const buttons = levelButtons(lv);
  const n = buttons.length;
  if (!n) return;
  for (let k = 1; k <= n; k++) {
    const i = (((from + d * k) % n) + n) % n;
    if (usable(itemOf(lv, buttons[i]))) {
      buttons[i].focus({ preventScroll: false });
      return;
    }
  }
}

function contextItem(it, index, depth) {
  if (it.separator) return el('div', { class: 'ctx-sep', attrs: { role: 'separator' } });
  const check = it.checked === undefined ? null : it.checked === 'mixed' ? 'mixed' : String(Boolean(it.checked));
  const sub = Array.isArray(it.items);
  const mark = check === 'true' ? '✓' : check === 'mixed' ? '–' : '';
  const b = el(
    'button',
    {
      class: `popup-item ctx-item${it.disabled ? ' disabled' : ''}`,
      title: it.title,
      attrs: {
        type: 'button',
        role: check === null ? 'menuitem' : 'menuitemcheckbox',
        'aria-checked': check,
        'aria-disabled': it.disabled ? 'true' : null,
        'aria-haspopup': sub ? 'menu' : null,
        'aria-expanded': sub ? 'false' : null,
        tabindex: '-1',
      },
      data: { index, label: it.label },
    },
    el('span', { class: 'popup-check', text: mark, attrs: { 'aria-hidden': 'true' } }),
    it.swatch ? el('span', { class: `ctx-swatch sw-${String(it.swatch).replace(/[^a-z]/g, '')}`, attrs: { 'aria-hidden': 'true' } }) : null,
    el('span', { class: 'popup-label', text: it.label }),
    sub
      ? el('span', { class: 'ctx-more', text: '›', attrs: { 'aria-hidden': 'true' } })
      : it.shortcut
        ? el('span', { class: 'ctx-key', text: it.shortcut, attrs: { 'aria-hidden': 'true' } })
        : null,
  );
  if (it.shortcut && !sub) b.setAttribute('aria-keyshortcuts', it.shortcut);
  b.addEventListener('click', () => activate(depth, b, { keyboard: false }));
  b.addEventListener('mouseenter', () => hover(depth, b));
  return b;
}

/** The mouse is over an item: focus it, and open its submenu after a moment. */
function hover(depth, btn) {
  if (!cm || !cm.levels[depth]) return;
  const lv = cm.levels[depth];
  const it = itemOf(lv, btn);
  clearTimeout(cm.hoverTimer);
  if (!usable(it)) {
    btn.focus({ preventScroll: true });
    cm.hoverTimer = setTimeout(() => trimLevels(depth), 200);
    return;
  }
  btn.focus({ preventScroll: true });
  const open = cm.levels[depth + 1];
  if (open && open.parent === btn) return;
  cm.hoverTimer = setTimeout(() => {
    if (!cm || !btn.isConnected) return;
    trimLevels(depth);
    if (Array.isArray(it.items)) openLevel(it.items, depth + 1, btn, { focusFirst: false });
  }, 160);
}

function activate(depth, btn, { keyboard }) {
  if (!cm || !cm.levels[depth]) return;
  const it = itemOf(cm.levels[depth], btn);
  if (!usable(it)) return;
  if (Array.isArray(it.items)) {
    clearTimeout(cm.hoverTimer);
    const open = cm.levels[depth + 1];
    if (open && open.parent === btn) {
      if (keyboard) stepFocus(open, -1, 1);
      return;
    }
    trimLevels(depth);
    openLevel(it.items, depth + 1, btn, { focusFirst: true });
    return;
  }
  closeContextMenu({ focusBack: true });
  if (typeof it.run === 'function') it.run();
}

function onContextKey(e) {
  if (!cm) return;
  const depth = cm.levels.findIndex((lv) => lv.root.contains(e.target));
  const lv = cm.levels[depth];
  if (!lv) return;
  if (e.metaKey || e.ctrlKey) {
    // A shortcut: close the menu and let the app have the key.
    closeContextMenu({ focusBack: true });
    return;
  }
  const buttons = levelButtons(lv);
  const i = buttons.indexOf(document.activeElement);
  let handled = true;
  switch (e.key) {
    case 'ArrowDown':
      stepFocus(lv, i < 0 ? -1 : i, 1);
      break;
    case 'ArrowUp':
      stepFocus(lv, i < 0 ? buttons.length : i, -1);
      break;
    case 'Home':
      stepFocus(lv, -1, 1);
      break;
    case 'End':
      stepFocus(lv, buttons.length, -1);
      break;
    case 'ArrowRight': {
      const it = itemOf(lv, buttons[i]);
      if (usable(it) && Array.isArray(it.items)) activate(depth, buttons[i], { keyboard: true });
      break;
    }
    case 'ArrowLeft':
      if (depth > 0) {
        const parent = lv.parent;
        trimLevels(depth - 1);
        if (parent && parent.isConnected) parent.focus();
      }
      break;
    case 'Enter':
    case ' ':
      if (i >= 0) activate(depth, buttons[i], { keyboard: true });
      break;
    case 'Escape':
      if (depth > 0) {
        const parent = lv.parent;
        trimLevels(depth - 1);
        if (parent && parent.isConnected) parent.focus();
      } else {
        closeContextMenu({ focusBack: true });
      }
      break;
    case 'Tab':
      closeContextMenu({ focusBack: true });
      break;
    default:
      handled = contextTypeAhead(e, lv, buttons, i);
  }
  if (handled) {
    e.preventDefault();
    e.stopPropagation();
  }
}

/** A typed letter or digit jumps to the next usable item starting with it. */
function contextTypeAhead(e, lv, buttons, from) {
  if (e.key.length !== 1 || e.altKey || !e.key.trim()) return false;
  const ch = e.key.toLowerCase();
  const n = buttons.length;
  for (let k = 1; k <= n; k++) {
    const b = buttons[(Math.max(from, -1) + k + n) % n];
    if (usable(itemOf(lv, b)) && b.dataset.label.toLowerCase().startsWith(ch)) {
      b.focus();
      break;
    }
  }
  return true;
}

/**
 * Builds a menu level and puts it at the point (the top level) or beside its
 * parent item (a submenu), inside the window.
 */
function openLevel(items, depth, parent, { at = null, focusFirst = true } = {}) {
  const root = el('div', {
    class: `popup-menu ctx-menu ctx-l${depth}`,
    attrs: { role: 'menu', 'aria-label': parent ? parent.dataset.label : cm.opts.label || 'Menu' },
    on: { keydown: onContextKey, contextmenu: (e) => e.preventDefault() },
  });
  root.append(...items.map((it, i) => contextItem(it, i, depth)));
  const lv = { root, items, depth, parent };
  cm.levels[depth] = lv;
  document.body.append(root);

  const w = Math.min(root.offsetWidth || 220, window.innerWidth - 16);
  const h = Math.min(root.offsetHeight || 200, window.innerHeight - 16);
  let left;
  let top;
  if (parent) {
    const r = parent.getBoundingClientRect();
    const box = parent.parentElement.getBoundingClientRect();
    left = r.right + w + 4 <= window.innerWidth - 8 ? box.right - 4 : box.left - w + 4;
    top = r.top - 5;
  } else {
    left = at.x + w <= window.innerWidth - 8 ? at.x : at.x - w;
    top = at.y + h <= window.innerHeight - 8 ? at.y : at.y - h;
  }
  left = Math.round(Math.max(8, Math.min(left, window.innerWidth - w - 8)));
  top = Math.round(Math.max(8, Math.min(top, window.innerHeight - h - 8)));
  const maxH = Math.max(120, Math.round(window.innerHeight - 16));
  setRules(`ctx-menu-${depth}`, `.ctx-menu.ctx-l${depth}.placed { left: ${left}px; top: ${top}px; max-height: ${maxH}px; transform: none; }`);
  root.classList.add('placed');
  if (parent) parent.setAttribute('aria-expanded', 'true');
  if (focusFirst) stepFocus(lv, -1, 1);
  return lv;
}

/** Opens a context menu at a point in the window (client coordinates). */
export function openContextMenu(at, opts) {
  closeMenu();
  closeContextMenu();
  if (!opts || !Array.isArray(opts.items) || !opts.items.length) return;
  cm = { levels: [], opts, hoverTimer: null };
  openLevel(opts.items, 0, null, { at: { x: Math.round(at.x), y: Math.round(at.y) } });
  document.addEventListener('mousedown', onContextOutside, true);
  window.addEventListener('resize', onContextWindow);
  window.addEventListener('blur', onContextWindow);
}
