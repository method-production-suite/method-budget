// Edit > Fill Rate from Table… and the line's right-click menu (roadmap
// 3.9; src/rates.rs; decisions.md, entry 90): pick a classification from
// one of the budget's rate tables to fill the line's rate. The figure is
// written once, as a number, with where it came from (one undo step); it
// never follows the table on its own. When the table now says something
// else, the dialog offers Update Rate, which is also one undo step.
//
// Commands: rate_choices, rate_fill and rate_update (the main window's,
// capabilities/main.json).

import { el } from './dom.js';
import { customDialog } from './modal.js';
import { errText } from './util.js';

/**
 * Opens the picker for line `rowId`. `ctx`: { call(cmd, args), toast(msg),
 * openRates() } (Budget Setup on the Rate Tables tab).
 */
export async function openRatePicker(rowId, ctx) {
  let data;
  try {
    data = await ctx.call('rate_choices', { row_id: rowId, unit: null });
  } catch (e) {
    ctx.toast(errText(e));
    return;
  }
  let filter = '';
  const list = el('ul', { class: 'ratepick-list', attrs: { role: 'listbox', 'aria-label': 'Classifications' } });
  const error = el('p', { class: 'modal-error', attrs: { role: 'alert' } });
  error.hidden = true;
  const current = el('div', { class: 'ratepick-current' });
  let closeDialog = () => {};

  const fail = (e) => {
    error.textContent = errText(e);
    error.hidden = false;
  };

  const choose = async (c) => {
    try {
      await ctx.call('rate_fill', { row_id: rowId, table: c.table, classification: c.classification, unit: data.unit || null, as_of: c.as_of });
      closeDialog();
      ctx.toast(`Rate filled from ${c.table}: ${c.classification}, ${c.rate} per ${data.unit || c.table_unit}.`);
    } catch (e) {
      fail(e);
    }
  };

  const drawCurrent = () => {
    const c = data.current;
    current.replaceChildren();
    if (!c) return;
    const words = c.words.charAt(0).toUpperCase() + c.words.slice(1);
    current.append(el('p', { class: 'modal-message', text: `${words}.` }));
    if (c.state === 'changed') {
      const update = el('button', { class: 'btn btn-primary', text: `Update Rate to ${c.now}`, attrs: { type: 'button' } });
      update.addEventListener('click', async () => {
        try {
          await ctx.call('rate_update', { row_ids: [rowId] });
          closeDialog();
          ctx.toast(`Rate updated to ${c.now}, as its table says now.`);
        } catch (e) {
          fail(e);
        }
      });
      current.append(update);
    }
  };

  const drawList = () => {
    list.replaceChildren();
    const needle = filter.trim().toLowerCase();
    const shown = data.choices.filter((c) => !needle || `${c.classification} ${c.table}`.toLowerCase().includes(needle));
    if (!shown.length) {
      list.append(el('li', { class: 'ratepick-empty', text: data.choices.length ? 'Nothing matches the search.' : 'This budget has no rate tables yet.' }));
      return;
    }
    for (const c of shown) {
      const figure = c.rate !== null && c.rate !== undefined ? `${c.rate} per ${data.unit || c.table_unit}` : 'no rate for this unit';
      const how = c.converted ? `converted from ${c.table_rate} per ${c.table_unit}` : '';
      const b = el(
        'button',
        {
          class: 'ratepick-choice',
          title: c.problem || [c.notes, how].filter(Boolean).join('\n') || undefined,
          attrs: { type: 'button', role: 'option', 'aria-disabled': c.rate === null || c.rate === undefined ? 'true' : null },
        },
        el('span', { class: 'ratepick-class', text: c.classification }),
        el('span', { class: 'ratepick-table', text: c.period ? `${c.table}, ${c.period}` : c.table }),
        el('span', { class: 'ratepick-rate', text: figure }),
      );
      b.addEventListener('click', () => {
        if (b.getAttribute('aria-disabled') === 'true') return;
        choose(c);
      });
      list.append(el('li', {}, b));
    }
  };

  const units = el('select', { class: 'modal-input', attrs: { 'aria-label': 'Rate per' } });
  if (!data.unit) units.append(el('option', { text: '(the table’s unit)', attrs: { value: '' } }));
  for (const u of data.units) units.append(el('option', { text: u, attrs: { value: u } }));
  units.value = data.unit || '';
  units.addEventListener('change', async () => {
    try {
      data = await ctx.call('rate_choices', { row_id: rowId, unit: units.value || null });
      drawList();
    } catch (e) {
      fail(e);
    }
  });
  const search = el('input', { class: 'modal-input', attrs: { type: 'text', placeholder: 'Search', 'aria-label': 'Search classifications', 'data-autofocus': '' } });
  search.addEventListener('input', () => {
    filter = search.value;
    drawList();
  });

  drawCurrent();
  drawList();
  closeDialog = customDialog((close) => {
    const cancel = el('button', { class: 'btn', text: 'Close', attrs: { type: 'button' } });
    cancel.addEventListener('click', () => close());
    const setup = el('button', { class: 'btn', text: 'Rate Tables…', title: 'Budget Setup, Rate Tables: keep and edit the tables', attrs: { type: 'button' } });
    setup.addEventListener('click', () => {
      close();
      ctx.openRates();
    });
    return el(
      'div',
      { class: 'modal ratepick', attrs: { 'aria-labelledby': 'ratepick-title' } },
      el('h2', { class: 'modal-title', text: 'Fill Rate from Table', attrs: { id: 'ratepick-title' } }),
      current,
      el('div', { class: 'ratepick-filters' }, el('label', { class: 'modal-field' }, el('span', { text: 'Rate per' }), units), el('label', { class: 'modal-field' }, el('span', { text: 'Search' }), search)),
      list,
      el('p', { class: 'modal-message', text: 'Picking one writes its figure into the line once; the line remembers where it came from. A table changed later never changes the line on its own.' }),
      error,
      el('div', { class: 'modal-actions' }, setup, cancel),
    );
  });
}
