// A small popup menu, opened from a button: the breadcrumb's jump lists and
// the detail grid's column chooser.
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
