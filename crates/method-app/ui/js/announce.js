// Polite announcements for screen readers (decision 82): one visually hidden
// live region, #announce in index.html, outside the app so a dialog making
// the page inert doesn't silence it. Calls close together speak only the
// last one, a moment after the last call, so a run of quick edits is one
// sentence about where the totals ended up.

import { $ } from './dom.js';

const WAIT_MS = 700;
let timer = null;

/** Says `text` politely once things settle; a newer call replaces it. */
export function announce(text) {
  clearTimeout(timer);
  timer = setTimeout(() => {
    const box = $('announce');
    if (!box) return;
    // Emptied first, so the same sentence twice is spoken twice.
    box.textContent = '';
    setTimeout(() => {
      box.textContent = String(text);
    }, 50);
  }, WAIT_MS);
}
