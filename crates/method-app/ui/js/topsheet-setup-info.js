// Budget Setup's Info tab (a part of topsheet-setup.js).

import { anchoredForm } from './modal.js';
import { redrawLists } from './setuplist.js';
import { mountFrame, actionStrip, stripButton, searchBox, gridTable, setFooterTotals } from './setupframe.js';
import { obj } from './util.js';
import { t } from './i18n.js';
import { DECIMAL, DATE, windowCtx, head, cell, lead, acts } from './topsheet-setup-common.js';

let k = null; // the kit from setup.js, set by useKit() on every render

/** Takes the kit for this render (topsheet-setup.js calls it). */
export function useKit(kit) {
  k = kit;
}

// ---------------------------------------------------------------------------
// Budget info

const INFO_KINDS = [
  ['text', t('topsheet.kind.text')],
  ['number', t('topsheet.kind.number')],
  ['date', t('topsheet.kind.date')],
];

function infoCheck(kind, value) {
  const v = value.trim();
  if (kind === 'number' && !DECIMAL.test(v)) return t('topsheet.enter_a_number_like_30_or_2');
  if (kind === 'date' && !DATE.test(v)) return t('topsheet.enter_a_date_as_yyyy_mm_dd');
  return null;
}

// New field…: a small form next to the strip's button; focus goes to the
// new field's value once it is added, else back to the button.
function newInfoField(anchor) {
  const ctx = windowCtx();
  let added = null;
  anchoredForm({
    anchor,
    title: t('topsheet.new_field'),
    fields: [
      { key: 'name', label: t('topsheet.label'), value: '', placeholder: t('topsheet.director') },
      { key: 'kind', label: t('topsheet.kind'), type: 'select', options: INFO_KINDS, value: 'text' },
      { key: 'value', label: t('topsheet.value'), value: '' },
    ],
    submitText: t('topsheet.add'),
    onSubmit: (v) => {
      const name = k.str(v.name).trim();
      if (name === '') return Promise.reject(t('topsheet.enter_a_label'));
      if (k.own(k.budget().info, name) !== undefined) return Promise.reject(t('topsheet.field_exists', { name }));
      const kind = k.str(v.kind) || 'text';
      const bad = infoCheck(kind, k.str(v.value));
      if (bad) return Promise.reject(bad);
      return ctx.call('set_info', { name, value: { kind, value: k.str(v.value).trim() } }).then(() => {
        added = name;
      });
    },
  }).then(() => {
    const f = k.findByData('key', added === null ? k.key('add', 'info') : k.key('info', added, 'value'));
    if (f) f.focus();
  });
}

// One table: a row per field (name, kind, value, delete), an action strip
// over it (New field…, Delete… for the selected row, the search) and the
// count in the footer.
export function renderInfo(panel) {
  const b = k.budget();
  const st = k.state();
  if (!st.filters) st.filters = {};
  if (!st.selected) st.selected = {};
  const q = k.str(st.filters.info).trim().toLowerCase();
  const all = k.entries(b.info);
  const items = all.filter(([n, v0]) => !q || [n, obj(v0).value].some((x) => k.str(x).toLowerCase().includes(q)));
  const names = items.map(([n]) => n);
  const selected = () => (st.selected.info !== undefined && k.own(k.budget().info, st.selected.info) !== undefined ? st.selected.info : null);

  const remove = stripButton(t('topsheet.delete_ellipsis'), k.key('strip', 'info', 'delete'), () => {
    const n = selected();
    if (n === null || k.isBusy()) return;
    const d = k.findByData('key', k.key('del', k.key('info', n)));
    if (d) d.click();
  });
  const syncStrip = () => {
    const n = selected();
    if (n === null) remove.setAttribute('aria-disabled', 'true');
    else remove.removeAttribute('aria-disabled');
    remove.title = n === null ? t('topsheet.select_a_field_first') : t('topsheet.delete_field', { name: n });
  };
  const strip = actionStrip(
    t('topsheet.info'),
    [stripButton(t('topsheet.new_field_ellipsis'), k.key('add', 'info'), (btn) => newInfoField(btn), { title: t('topsheet.add_a_field_to_this_budget') }), remove],
    searchBox({
      key: k.key('search', 'info'),
      value: k.str(st.filters.info),
      label: t('topsheet.search_info'),
      placeholder: t('topsheet.search_info'),
      onChange: (v) => {
        st.filters.info = v;
        redrawLists();
      },
    }),
  );

  const ul = k.list(t('topsheet.budget_information'));
  items.forEach(([name, v0], i) => {
    const v = obj(v0);
    const kind = ['text', 'number', 'date'].includes(v.kind) ? v.kind : 'text';
    const ik = k.key('info', name);
    const li = k.itemShell(ik, 'sw-grid-row');
    if (st.selected.info === name) li.classList.add('is-selected');
    li.addEventListener('focusin', () => {
      if (st.selected.info === name) return;
      st.selected.info = name;
      for (const x of ul.querySelectorAll(':scope > .is-selected')) x.classList.remove('is-selected');
      li.classList.add('is-selected');
      syncStrip();
    });
    const nameEl = k.el('span', 'setup-name', name);
    nameEl.title = name;
    const valueKey = k.key('info', name, 'value');
    const value = k.textInput({
      fieldKey: valueKey,
      value: k.str(v.value),
      num: kind === 'number',
      placeholder: kind === 'date' ? t('topsheet.date_placeholder') : '',
      label: t('topsheet.name_value', { name }),
      cls: 'sw-cell-input',
      onCommit: (val, old) => k.run(ik, () => ['set_info', { name, value: { kind, value: val } }], { restore: [valueKey, old] }),
    });
    const kindSel = k.selectInput({
      fieldKey: k.key('info', name, 'kind'),
      value: kind,
      options: INFO_KINDS,
      label: t('topsheet.name_kind', { name }),
      cls: 'sw-cell-input',
      onCommit: (nk, old) =>
        k.run(ik, () => ['set_info', { name, value: { kind: nk, value: k.str(obj(k.own(k.budget().info, name)).value) } }], {
          restore: [k.key('info', name, 'kind'), old],
        }),
    });
    li.append(
      lead(),
      cell(nameEl),
      cell(kindSel),
      cell(value),
      acts(
        k.deleteButton({
          itemKey: ik,
          name,
          what: 'field',
          confirmText: t('topsheet.delete_the_field', { name }),
          build: () => ['delete_info', { name }],
          neighborKey: k.neighbor('info', names, i, k.key('add', 'info')),
        }),
      ),
    );
    k.finishItem(li, ik, [], null);
    ul.append(li);
  });
  const body = [];
  if (!all.length) body.push(k.emptyNote(t('topsheet.no_fields_yet')));
  else if (!items.length) body.push(k.emptyNote(t('topsheet.nothing_matches_the_search')));
  else body.push(gridTable('info', [lead(), head(t('topsheet.name')), head(t('topsheet.kind')), head(t('topsheet.value')), head('')], ul));
  mountFrame(panel, { strip, body });
  syncStrip();
  setFooterTotals([
    { label: '', value: items.length === all.length ? t('topsheet.count.fields', { count: all.length }) : t('topsheet.shown_of_fields', { shown: items.length, count: all.length }) },
  ]);
}
