// The keyboard shortcut sheet (Help > Keyboard Shortcuts, or mod+shift+/). Built
// from the shortcut list, so it always shows what the app really does, with
// this platform's keys. A modal dialog: the rest of the window is inert while
// it's open, Tab stays inside it, and Escape closes it.

import { el, isMac } from './dom.js';
import { t } from './i18n.js';
import { SHORTCUTS, GROUPS, GROUP_NAMES, keysFor, keyText, matches, menuRunsAppKeys, shortcutById } from './shortcuts.js';

let open = null; // { root, previous }

export function isHelpOpen() {
  return open !== null;
}

export function toggleHelp() {
  if (open) closeHelp();
  else openHelp();
}

function keysCell(entry) {
  const td = el('td', { class: 'help-keys' });
  keysFor(entry).forEach((spec, i) => {
    if (i > 0) td.append(el('span', { class: 'help-or', text: ` ${t('help.or')} ` }));
    td.append(el('kbd', { text: keyText(spec) }));
  });
  return td;
}

function sheet() {
  const groups = GROUPS.map((g) => {
    const rows = SHORTCUTS.filter((s) => s.group === g && keysFor(s).length > 0).map((s) =>
      el('tr', {}, el('th', { text: s.label, attrs: { scope: 'row' } }), keysCell(s)),
    );
    if (!rows.length) return null;
    const id = `help-group-${g.toLowerCase()}`;
    return el(
      'section',
      { class: 'help-group', attrs: { 'aria-labelledby': id } },
      el('h3', { text: GROUP_NAMES[g], attrs: { id } }),
      el('table', { class: 'help-table' }, el('tbody', {}, ...rows)),
    );
  });
  return groups.filter(Boolean);
}

export function openHelp() {
  if (open) return;
  const previous = document.activeElement;
  const close = el('button', { class: 'btn', text: t('help.close'), attrs: { type: 'button' } });
  const body = el(
    'div',
    { class: 'help-body', attrs: { tabindex: '0', role: 'region', 'aria-label': t('help.shortcut_list') } },
    ...sheet(),
  );
  const dialog = el(
    'div',
    { class: 'help', attrs: { role: 'dialog', 'aria-modal': 'true', 'aria-labelledby': 'help-title' } },
    el('header', { class: 'help-head' }, el('h2', { class: 'help-title', text: t('help.title'), attrs: { id: 'help-title' } }), close),
    body,
    el('p', { class: 'help-foot', text: t('help.foot', { mod: isMac ? t('help.key.command') : t('help.key.ctrl') }) }),
  );
  const root = el('div', { class: 'help-backdrop' }, dialog);

  close.addEventListener('click', () => closeHelp());
  root.addEventListener('mousedown', (e) => {
    if (e.target === root) closeHelp();
  });
  dialog.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      closeHelp();
      return;
    }
    if (e.key === 'Tab') {
      // Keep focus inside: the list, then Close, then back to the list.
      const stops = [body, close];
      const i = stops.indexOf(document.activeElement);
      const next = e.shiftKey ? (i <= 0 ? stops.length - 1 : i - 1) : (i + 1) % stops.length;
      e.preventDefault();
      stops[next].focus();
      return;
    }
    // The same key closes it again. On macOS the menu item does that.
    if (!menuRunsAppKeys && keysFor(shortcutById('help.shortcuts')).some((k) => matches(e, k))) {
      e.preventDefault();
      closeHelp();
    }
  });

  const app = document.querySelector('.app');
  if (app) app.inert = true;
  document.body.append(root);
  open = { root, previous };
  body.focus();
}

export function closeHelp() {
  if (!open) return;
  const { root, previous } = open;
  open = null;
  root.remove();
  const app = document.querySelector('.app');
  if (app) app.inert = false;
  if (previous && previous.isConnected && typeof previous.focus === 'function') {
    previous.focus({ preventScroll: true });
  }
}
