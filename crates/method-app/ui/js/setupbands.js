// Budget-wide settings drawn as a band over a Budget Setup table
// (setupframe.js band()): the Currency tab's base currency and the currency
// totals show in. Only the Budget Setup window draws them (apply.js, setup
// mode); the commands are its own (capabilities/setup.json).
//
// Each field saves on its own, one undo step: the base currency on Enter or
// when it loses focus (Escape puts the old code back), the totals currency
// when picked. Budget text only reaches the page through input values.

import { el } from './dom.js';
import { band, bandField } from './setupframe.js';
import { orderedNames } from './order.js';
import { obj, str, errText } from './util.js';
import { t } from './i18n.js';

/**
 * The Currency band for `budget`. `ctx`: { call(cmd, args) (resolves once
 * the view is redrawn), onError(message) }.
 */
export function currencyBand(budget, ctx) {
  const b = obj(budget);
  const base = str(b.base_currency);
  const fail = (e, undo) => {
    undo();
    if (typeof ctx.onError === 'function') ctx.onError(errText(e));
  };

  // Renaming the base currency renames it on every line, charge and setting
  // that names it too (src/rename.rs), as one undo step.
  const baseInput = el('input', {
    class: 'setup-input sw-band-short',
    attrs: { type: 'text', autocomplete: 'off', spellcheck: 'false', autocapitalize: 'off', autocorrect: 'off' },
    data: { key: 'apply:band:base', orig: base },
  });
  baseInput.value = base;
  const commit = () => {
    if (!baseInput.isConnected) return;
    const to = baseInput.value.trim();
    const old = baseInput.dataset.orig;
    if (baseInput.value === old) return;
    baseInput.dataset.orig = baseInput.value;
    if (to === '' || to === old) {
      baseInput.value = old;
      baseInput.dataset.orig = old;
      return;
    }
    ctx.call('rename_setup_item', { kind: 'currency', from: old, to }).catch((e) =>
      fail(e, () => {
        baseInput.value = old;
        baseInput.dataset.orig = old;
      }),
    );
  };
  baseInput.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && !e.isComposing) {
      e.preventDefault();
      commit();
    } else if (e.key === 'Escape' && baseInput.value !== baseInput.dataset.orig) {
      e.preventDefault();
      e.stopPropagation();
      baseInput.value = baseInput.dataset.orig;
    }
  });
  baseInput.addEventListener('blur', commit);

  const codes = orderedNames(b, 'currencies').filter((c) => Object.prototype.hasOwnProperty.call(obj(b.currencies), c));
  const totals = str(b.totals_currency);
  if (totals && totals !== base && !codes.includes(totals)) codes.push(totals);
  const totalsSel = el('select', { class: 'setup-input setup-select', data: { key: 'apply:band:totals' } });
  for (const [v, text] of [['', base ? t('setup.band.base_named', { base }) : t('setup.band.base_currency')], ...codes.map((c) => [c, c])]) {
    totalsSel.append(el('option', { text, attrs: { value: v } }));
  }
  totalsSel.value = totals;
  totalsSel.addEventListener('change', () => {
    const v = totalsSel.value;
    ctx.call('set_totals_currency', { code: v === '' ? null : v }).catch((e) =>
      fail(e, () => {
        totalsSel.value = totals;
      }),
    );
  });

  const row = band(t('setup.tab.currencies'), bandField(t('setup.band.base_currency'), baseInput), bandField(t('setup.band.totals_in'), totalsSel));
  row.title = t('setup.everything_is_worked_out_in');
  return row;
}
