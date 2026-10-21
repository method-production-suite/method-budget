// The banner on a budget converted from an export (decisions.md, entry 67):
// "Converted from <file>. The original file is left as it was. Saving keeps
// this budget as a Method Budget file (.mthd)."
//
// Rust keeps the text on the budget's session and sends it with every view
// (`view.converted`) until the first Save, so the banner belongs to that tab:
// switching tabs shows or hides it with the tab, and it is never written into
// the file. Closing it is remembered here, by tab, for this run of the app.
// Each pane has its own banner element (pane.js); a budget shown in both
// panes loses it in both.

import { el } from './dom.js';
import { t } from './i18n.js';

/** Tabs (doc ids) whose banner was closed in this run. */
const dismissed = new Set();

/**
 * The banner text for `view`, or '' when there is none to show (not a
 * converted budget, saved since, or closed for this tab). Pure: `closed` is
 * the set of closed doc ids.
 */
export function convertedText(view, closed = dismissed) {
  if (!view || typeof view.converted !== 'string' || !view.converted) return '';
  if (closed.has(view.doc)) return '';
  return view.converted;
}

/**
 * Draws (or hides) the converted-budget banner for `view` in `host`.
 * `onClosed()` runs after the person closes it (to put focus back).
 */
export function renderConvertBanner(host, view, onClosed) {
  if (!host) return;
  const text = convertedText(view);
  if (!text) {
    host.hidden = true;
    host.replaceChildren();
    delete host.dataset.doc;
    return;
  }
  const doc = view.doc;
  // Unchanged: leave it (and any focus on its button) alone.
  if (!host.hidden && host.dataset.doc === String(doc) && host.dataset.text === text) return;
  const close = el('button', {
    class: 'btn btn-quiet convert-banner-close',
    text: '×',
    title: t('convertbanner.close'),
    attrs: { type: 'button', 'aria-label': t('convertbanner.close_label') },
  });
  const dismiss = () => {
    dismissed.add(doc);
    for (const b of document.querySelectorAll('.convert-banner')) {
      if (b.dataset.doc === String(doc)) {
        b.hidden = true;
        b.replaceChildren();
        delete b.dataset.doc;
      }
    }
    if (onClosed) onClosed();
  };
  close.addEventListener('click', dismiss);
  close.addEventListener('keydown', (e) => {
    if (e.key !== 'Escape') return;
    e.preventDefault();
    e.stopPropagation();
    dismiss();
  });
  host.replaceChildren(el('span', { class: 'convert-banner-text', text }), close);
  host.dataset.doc = String(doc);
  host.dataset.text = text;
  host.hidden = false;
}
