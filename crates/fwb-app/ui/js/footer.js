// The totals strip along the bottom: budget totals on the left, totals for the
// selected detail rows on the right.

import { el } from './dom.js';
import { money } from './format.js';

function stat(label, cents, cls = '') {
  const value = cents === null || cents === undefined ? 'n/a' : money(cents);
  return el('div', { class: `stat ${cls}` }, el('span', { class: 'stat-label', text: label }), el('span', { class: 'stat-value', text: value }));
}

/**
 * `selection`: null when not on a detail grid, else { count, totals } where
 * totals is the selection_totals result (or null while it loads).
 */
export function renderFooter(host, view, selection) {
  const r = view && view.report;
  const left = el(
    'div',
    { class: 'footer-totals' },
    stat('Above the line', r ? r.total_atl : null),
    stat('Below the line', r ? r.total_btl : null),
    stat('Grand total', r ? r.grand_total : null, 'strong'),
  );
  if (r && (r.credits.length || r.net_total !== r.grand_total)) {
    left.append(stat('Net total', r.net_total, 'strong'));
  }
  if (view && view.partial) {
    left.append(el('span', { class: 'footer-note', text: 'Cells with problems count as blank', title: 'Fix the flagged cells for exact totals' }));
  }

  const right = el('div', { class: 'footer-selection' });
  if (selection && selection.count > 0) {
    const t = selection.totals;
    const rows = selection.count === 1 ? '1 row' : `${selection.count} rows`;
    right.append(
      el('span', { class: 'sel-count', text: `Selected ${rows}` }),
      stat('Lines', t ? t.lines.details_total : null),
      stat('Fringes', t ? t.lines.fringe_total : null),
    );
  }
  host.replaceChildren(left, right);
}
