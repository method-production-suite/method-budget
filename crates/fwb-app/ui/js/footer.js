// The totals strip along the bottom: budget totals on the left, then (when
// drilled down) the current account's and category's totals; on the right the
// totals of the selected detail rows and how much this session has changed the
// net total.

import { el } from './dom.js';
import { totalText as money } from './display.js';

function stat(label, cents, cls = '', title = '') {
  const value = cents === null || cents === undefined ? 'n/a' : money(cents);
  return el(
    'div',
    { class: `stat ${cls}`, title },
    el('span', { class: 'stat-label', text: label }),
    el('span', { class: 'stat-value', text: value }),
  );
}

/** A change in the net total: "+1,250.00", "(300.00)" or "0.00". */
function change(label, cents, title) {
  const s = stat(label, cents, 'change', title);
  const v = s.lastChild;
  if (cents > 0) {
    v.textContent = `+${money(cents)}`;
    s.classList.add('up');
  } else if (cents < 0) {
    s.classList.add('down');
  }
  return s;
}

/**
 * `selection`: null when not on a detail grid, else { count, totals } where
 * totals is the selection_totals result (or null while it loads).
 * `extra.context`: [{ label, cents, title }], the account and category in view.
 * `extra.changes`: { session, last } in cents, or null when there are no totals.
 */
export function renderFooter(host, view, selection, extra = {}) {
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

  const context = el('div', { class: 'footer-context' });
  for (const c of extra.context || []) {
    context.append(stat(c.label, c.cents, 'context', `${c.title}: ${c.label}`));
  }

  const right = el('div', { class: 'footer-right' });
  if (selection && selection.count > 0) {
    const t = selection.totals;
    const rows = selection.count === 1 ? '1 row' : `${selection.count} rows`;
    right.append(
      el(
        'div',
        { class: 'footer-selection' },
        el('span', { class: 'sel-count', text: `Selected ${rows}` }),
        stat('Lines', t ? t.lines.details_total : null),
        stat('Fringes', t ? t.lines.fringe_total : null),
      ),
    );
  }
  const ch = extra.changes;
  if (ch) {
    right.append(
      el(
        'div',
        { class: 'footer-changes' },
        change('This session', ch.session, 'Net total now, less the net total when this budget was opened or created'),
        change('Last change', ch.last, 'How much the most recent edit moved the net total'),
      ),
    );
  }
  host.replaceChildren(left, context, right);
}
