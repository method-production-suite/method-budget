// Budget Setup's Baseline tab (a part of topsheet-setup.js).

import { mountFrame, actionStrip, stripButton, gridTable, setFooterTotals } from './setupframe.js';
import { obj } from './util.js';
import { t } from './i18n.js';
import { isNone, checkbox, confirmBox, head, cell, textCell, lead, gridRow } from './topsheet-setup-common.js';

let k = null; // the kit from setup.js, set by useKit() on every render

/** Takes the kit for this render (topsheet-setup.js calls it). */
export function useKit(kit) {
  k = kit;
}

// ---------------------------------------------------------------------------
// Baseline

function changedCount(map) {
  let n = 0;
  for (const [, c] of k.entries(map)) if (obj(c).variance !== 0) n += 1;
  return n;
}

// The action strip (Lock current totals, or Lock again… and Clear…), the
// original, current and change of the grand and net totals as a table, the
// freeze switch (while frozen, Lock again… and Clear… are off; decisions.md,
// entry 61), and what has changed since in the footer.
export function renderBaseline(panel) {
  const b = k.budget();
  const view = k.view() || {};
  const variance = view.variance && typeof view.variance === 'object' ? view.variance : null;
  const has = b.baseline && typeof b.baseline === 'object';
  const ik = k.key('baseline');
  const box = k.el('div', 'tss-body');
  box.dataset.item = ik;
  const actions = [];
  const foot = [];

  if (has) {
    const ul = k.el('ul');
    ul.setAttribute('aria-label', t('topsheet.original_and_current_totals'));
    const line = (label, ch0, fallback) => {
      const ch = obj(ch0);
      const orig = isNone(ch.original) ? fallback : ch.original;
      const v = ch.variance;
      const change = textCell(isNone(v) ? '' : k.formatCents(v), { num: true });
      if (typeof v === 'number' && v !== 0) change.classList.add(v > 0 ? 'tss-up' : 'tss-down');
      const li = gridRow();
      li.append(
        lead(),
        cell(k.el('span', 'setup-name', label)),
        textCell(isNone(orig) ? '' : k.formatCents(orig), { num: true }),
        textCell(isNone(ch.current) ? '' : k.formatCents(ch.current), { num: true }),
        change,
      );
      ul.append(li);
    };
    line(t('topsheet.grand_total'), variance && variance.grand_total, b.baseline.grand_total);
    line(t('topsheet.net_total'), variance && variance.net_total, b.baseline.net_total);
    box.append(gridTable('baseline', [lead(), head(''), head(t('topsheet.original'), { num: true }), head(t('topsheet.current'), { num: true }), head(t('topsheet.change'), { num: true })], ul));
    if (variance) {
      // Views list only the rows on screen; the count covers every row.
      const lines = typeof variance.rows_changed === 'number' ? variance.rows_changed : changedCount(variance.rows);
      const accounts = changedCount(variance.accounts);
      foot.push(
        lines || accounts
          ? { label: t('topsheet.changed_since_locked'), value: t('topsheet.lines_in_accounts', { lines: t('topsheet.count.lines', { count: lines }), accounts: t('topsheet.count.accounts', { count: accounts }) }) }
          : { label: '', value: t('topsheet.nothing_has_changed_since_the_totals_were') },
      );
    } else {
      foot.push({ label: '', value: t('topsheet.current_totals_cant_be_worked_out_right') });
    }
    if (view.partial) box.append(k.el('p', 'setup-hint tss-note', t('topsheet.current_totals_are_partial_while_the_budget')));
    const frozen = b.baseline.frozen === true;
    const frozenKey = k.key('baseline', 'frozen');
    box.append(
      checkbox(frozenKey, frozen, t('topsheet.freeze_the_original_totals'), (on) =>
        k.run(ik, () => ['set_baseline_frozen', { frozen: on }], { restore: [frozenKey, !on] }),
      ),
    );
    const offWhileFrozen = t('topsheet.off_while_the_original_totals_are_frozen');
    const relock = stripButton(t('topsheet.lock_again_ellipsis'), k.key('baseline', 'relock'), () => {
      if (!k.isBusy()) confirmBox(ik, relock, t('topsheet.replace_the_original_totals_with_the_current'), t('topsheet.lock_again'), false, () => ['lock_baseline', {}]);
    }, { title: frozen ? offWhileFrozen : t('topsheet.replace_the_original_totals_with_the_current_2'), enabled: !frozen });
    const clear = stripButton(t('topsheet.clear_ellipsis'), k.key('baseline', 'clear'), () => {
      if (!k.isBusy()) confirmBox(ik, clear, t('topsheet.clear_the_original_totals_the_budget_stops'), t('topsheet.clear'), true, () => ['clear_baseline', {}]);
    }, { title: frozen ? offWhileFrozen : t('topsheet.clear_the_original_totals'), enabled: !frozen });
    actions.push(relock, clear);
  } else {
    box.append(
      k.emptyNote(
        view.error
          ? t('topsheet.no_original_totals_locked_yet_the_budget')
          : t('topsheet.no_original_totals_locked_yet'),
      ),
    );
    actions.push(
      stripButton(t('topsheet.lock_current_totals'), k.key('baseline', 'lock'), () => {
        if (k.isBusy()) return;
        k.setFocusKey(k.key('baseline', 'relock'));
        k.run(ik, () => ['lock_baseline', {}], { onError: () => k.setFocusKey(null) });
      }, { primary: true, title: t('topsheet.keep_the_current_totals_as_the_original') }),
    );
  }
  if (k.state().errors.has(ik)) box.append(k.errorLine(k.state().errors.get(ik)));
  mountFrame(panel, { strip: actionStrip(t('topsheet.baseline'), actions), body: [box] });
  setFooterTotals(foot);
}
