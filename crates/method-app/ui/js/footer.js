// The totals strip along the bottom: budget totals on the left, then (when
// drilled down) the current account's and category's totals; on the right the
// totals of the selected detail rows and how much this session has changed the
// net total.

import { el } from './dom.js';
import { totalText as money } from './display.js';
import { t } from './i18n.js';

function stat(label, cents, cls = '', title = '') {
  const value = cents === null || cents === undefined ? t('footer.none') : money(cents);
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
    stat(t('footer.above_the_line'), r ? r.total_atl : null),
    stat(t('footer.below_the_line'), r ? r.total_btl : null),
    stat(t('footer.grand_total'), r ? r.grand_total : null, 'strong'),
  );
  if (r && (r.credits.length || r.net_total !== r.grand_total)) {
    left.append(stat(t('footer.net_total'), r.net_total, 'strong'));
  }
  if (view && view.partial) {
    left.append(el('span', { class: 'footer-note', text: t('footer.partial'), title: t('footer.partial_tip') }));
  }

  const context = el('div', { class: 'footer-context' });
  for (const c of extra.context || []) {
    context.append(stat(c.label, c.cents, 'context', t('footer.context_tip', { what: c.title, name: c.label })));
  }

  const right = el('div', { class: 'footer-right' });
  if (selection && selection.count > 0) {
    const tot = selection.totals;
    right.append(
      el(
        'div',
        { class: 'footer-selection' },
        el('span', { class: 'sel-count', text: t('footer.selected_rows', { count: selection.count }) }),
        stat(t('footer.lines'), tot ? tot.lines.details_total : null),
        stat(t('footer.fringes'), tot ? tot.lines.fringe_total : null),
      ),
    );
  }
  const ch = extra.changes;
  if (ch) {
    right.append(
      el(
        'div',
        { class: 'footer-changes' },
        change(t('footer.this_session'), ch.session, t('footer.this_session_tip')),
        change(t('footer.last_change'), ch.last, t('footer.last_change_tip')),
      ),
    );
  }
  host.replaceChildren(left, context, right);
}
