// Help > Licenses and Notices: the app's two licences (MIT, Apache 2.0) and
// the third-party notices, in a dialog. Rust builds the three files into the
// app and hands them over as plain text when the dialog opens (`licenses`,
// src/help.rs); nothing is fetched before that.
//
// The notices are long (several hundred KB of markdown), so the dialog shows
// one file at a time, as preformatted text in one text node: no markup is
// made from them (textContent through dom.js's el()), and the browser lays
// out a single block of text quickly.

import { invoke } from './api.js';
import { el } from './dom.js';
import { t } from './i18n.js';
import { customDialog, isModalOpen } from './modal.js';
import { isHelpOpen } from './help.js';

/** The files, in the order the dialog's buttons show them. */
export const FILES = [
  { key: 'mit', label: t('licenses.mit') },
  { key: 'apache', label: t('licenses.apache') },
  { key: 'notices', label: t('licenses.notices') },
];

/** The text for `key` in what Rust sent, or '' when it isn't there. */
export function textFor(texts, key) {
  const v = texts && typeof texts === 'object' ? texts[key] : '';
  return typeof v === 'string' ? v : '';
}

/**
 * The dialog: a row of buttons picking a file, then that file as plain text.
 * `texts` is { mit, apache, notices }; `close` closes it.
 */
export function licensesBox(texts, close, first = 'mit') {
  const intro = el('p', {
    class: 'licenses-intro',
    text: t('licenses.intro'),
  });
  const text = el('pre', { class: 'licenses-text', attrs: { tabindex: '0', role: 'region', 'aria-label': '' } });
  const picks = el('div', { class: 'licenses-picks', attrs: { role: 'group', 'aria-label': t('licenses.which_file') } });
  const buttons = new Map();
  function show(key) {
    const f = FILES.find((x) => x.key === key) || FILES[0];
    for (const [k, b] of buttons) b.setAttribute('aria-pressed', String(k === f.key));
    text.setAttribute('aria-label', f.label);
    text.textContent = textFor(texts, f.key) || t('licenses.missing');
    text.scrollTop = 0;
  }
  for (const f of FILES) {
    const b = el('button', { class: 'btn', text: f.label, attrs: { type: 'button', 'aria-pressed': 'false' } });
    b.addEventListener('click', () => show(f.key));
    buttons.set(f.key, b);
    picks.append(b);
  }
  show(first);
  const ok = el('button', { class: 'btn btn-primary', text: t('licenses.close'), attrs: { type: 'button', 'data-autofocus': true } });
  ok.addEventListener('click', close);
  return el(
    'div',
    { class: 'modal modal-wide licenses-dialog', attrs: { role: 'dialog', 'aria-modal': 'true', 'aria-labelledby': 'licenses-title' } },
    el('h2', { class: 'modal-title', text: t('licenses.title'), attrs: { id: 'licenses-title' } }),
    intro,
    picks,
    text,
    el('div', { class: 'modal-actions' }, ok),
  );
}

/**
 * Sets Licenses and Notices up in the main window. `ctx`: { toast }.
 * Returns { commands }: Help > Licenses and Notices.
 */
export function initLicenses(ctx) {
  let texts = null;
  async function open() {
    if (isModalOpen() || isHelpOpen()) return;
    if (!texts) {
      try {
        texts = await invoke('licenses');
      } catch (e) {
        ctx.toast(e);
        return;
      }
    }
    customDialog((close) => licensesBox(texts, close));
  }
  return { commands: { 'help.licenses': open } };
}
