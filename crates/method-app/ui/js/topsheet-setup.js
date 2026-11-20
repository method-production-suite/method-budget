// Budget Setup's Charges, Credits, Info, Settings and Baseline tabs: the
// budget's own setup (currencies are the Currency tab's band, setupbands.js;
// sets and locations are lists in apply.js).
//
// setupwin.js draws them through setup.js, which owns the command queue and
// focus handling and passes its helpers in as `kit`, so every tab looks and
// behaves the same. Each tab is drawn in the shared layout (setupframe.js;
// the classes are documented at the top of setup.css): an action strip, the
// tab's content scrolling under it, and its totals in the window's footer.
// Charges and Credits list their items in the left rail (railHost()), with
// the add form under the list, and edit the picked item at the area's full
// width. Budget text only reaches the page through textContent and input
// values, never innerHTML.

import { invoke } from './api.js';
import { saveItemToLibrary } from './library.js';
import { mountFrame, actionStrip, stripButton, gridTable, band, setFooterTotals, railHost } from './setupframe.js';
import { obj } from './util.js';
import { t } from './i18n.js';
import { DECIMAL, arr, isNone, issuesAt, appliedEntry, button, checkbox, windowCtx, bandInput, bandTitle, bandButton, chips, groupNames, fringeNames, locationNames, categoryNames, accountNames, withKept, addPicker, head, cell, textCell, lead, gridRow, acts, centsSum, decimalSum, useKit as useCommonKit } from './topsheet-setup-common.js';
import { estimatedRows, estimateOf, programsBox, showEstimates, useKit as useIncentivesKit } from './topsheet-setup-incentives.js';
import { renderInfo, useKit as useInfoKit } from './topsheet-setup-info.js';
import { renderSettings, useKit as useSettingsKit } from './topsheet-setup-settings.js';
import { renderBaseline, useKit as useBaselineKit } from './topsheet-setup-baseline.js';

let k = null; // the kit from setup.js, set on every render

export function renderExtraTab(tab, panel, kit) {
  k = kit;
  // Each part of this tab keeps the kit too (topsheet-setup-*.js).
  for (const use of [useCommonKit, useIncentivesKit, useInfoKit, useSettingsKit, useBaselineKit]) use(kit);
  const render = {
    charges: renderCharges,
    credits: renderCredits,
    info: renderInfo,
    settings: renderSettings,
    baseline: renderBaseline,
  }[tab];
  if (render) render(panel);
}

// ---------------------------------------------------------------------------
// Charges and credits: the items listed in the left rail, the picked item's
// editor filling the area under the strip. Every item's editor is drawn and
// only the picked one shows, so picking another needs no redraw.

// The picked item of each tab, by index, kept across redraws; and the index
// a newly added item will have, picked once it shows up.
export const picked = { charges: 0, credits: 0 };
const pickNew = { charges: null, credits: null };

/**
 * Draws `tab` into `panel`: its items in the left rail (railHost()) with
 * `add` (the add form) under them, and the picked item's editor under the
 * action strip (New…, Move Up, Move Down, Delete… and Save to Library…).
 * `items` are { name, applied, figure, title, bad, editor }, in order;
 * `library` is the library kind (Save to Library…); `move` the reorder
 * command; `deleteText(i)` and `deleteBuild(i)` the delete question and
 * command; `foot(i)` the footer's totals with item i picked; `onShow(i)`
 * runs when item i's editor shows; `extra` are more nodes after the editors.
 */
function pickEditor(panel, { tab, what, label, library, items, add, empty, move, deleteText, deleteBuild, foot, onShow, extra = [] }) {
  const count = items.length;
  if (pickNew[tab] !== null && pickNew[tab] < count) {
    picked[tab] = pickNew[tab];
    pickNew[tab] = null;
  }
  let at = count ? Math.min(Math.max(0, Number(picked[tab]) || 0), count - 1) : 0;
  picked[tab] = at;
  const editors = k.el('div', 'tss-editors');

  // Moves the picked item by `by`; focus stays on the button pressed (or
  // goes to `focusKey`).
  const moveBy = (by, focusKey) => {
    const from = at;
    const to = at + by;
    if (!count || to < 0 || to >= count) return;
    k.setFocusKey(focusKey || k.key(tab, by < 0 ? 'up' : 'down'));
    k.run(
      k.key(tab, from),
      () => {
        picked[tab] = to;
        return [move, { index: from, to }];
      },
      {
        onError: () => {
          picked[tab] = from;
          k.setFocusKey(null);
        },
      },
    );
  };

  // The delete question, at the top of the picked item's editor, as
  // setup.js asks it for a list item.
  const confirmDelete = () => {
    if (!count) return;
    const open = editors.querySelector('.tss-confirm');
    if (open) {
      open.querySelector('.setup-btn-danger').focus();
      return;
    }
    const i = at;
    const ik = k.key(tab, i);
    const box = k.el('div', 'setup-confirm tss-confirm');
    box.setAttribute('role', 'group');
    box.setAttribute('aria-label', t('topsheet.delete_what_named', { what, name: items[i].name }));
    const text = k.el('p', 'setup-confirm-text', deleteText(i));
    text.id = k.nextId();
    const yes = k.el('button', 'setup-btn setup-btn-danger', t('topsheet.delete'));
    yes.type = 'button';
    yes.dataset.key = k.key('del-yes', ik);
    yes.setAttribute('aria-describedby', text.id);
    const no = k.el('button', 'setup-btn setup-btn-quiet', t('topsheet.cancel'));
    no.type = 'button';
    no.dataset.key = k.key('del-no', ik);
    const close = () => {
      box.remove();
      del.focus();
    };
    no.addEventListener('click', close);
    box.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        close();
      }
    });
    yes.addEventListener('click', () => {
      if (k.isBusy()) return;
      // The item that takes its place gets focus, else the add form.
      k.setFocusKey(count > 1 ? k.key(tab, 'pick', Math.min(i, count - 2)) : k.key('add', tab, 'name'));
      k.run(ik, () => deleteBuild(i), {
        onError: () => {
          k.setFocusKey(null);
          box.remove();
          const b = k.findByData('key', k.key('del', tab));
          if (b) b.focus();
        },
      });
    });
    const actions = k.el('div', 'setup-confirm-actions');
    actions.append(yes, no);
    box.append(text, actions);
    items[i].editor.prepend(box);
    yes.focus();
  };

  // New…: the add form's first box, in the rail.
  const focusAdd = () => {
    const f = k.findByData('key', k.key('add', tab, 'name'));
    if (!f) return;
    f.focus();
    if (typeof f.scrollIntoView === 'function') f.scrollIntoView({ block: 'nearest' });
  };

  const busy = (fn) => () => {
    if (!k.isBusy()) fn();
  };
  const neu = stripButton(t('topsheet.new_what', { what }), k.key('new', tab), focusAdd, { title: t('topsheet.add_a_what', { what }) });
  const up = stripButton(t('topsheet.move_up'), k.key(tab, 'up'), busy(() => moveBy(-1)));
  const down = stripButton(t('topsheet.move_down'), k.key(tab, 'down'), busy(() => moveBy(1)));
  const del = stripButton(t('topsheet.delete_ellipsis'), k.key('del', tab), busy(confirmDelete));
  const save = stripButton(t('topsheet.save_to_library'), k.key('save', tab), () => {
    if (count) saveItemToLibrary(windowCtx(), library, items[at].name);
  });

  const ul = k.el('ul', 'sw-tree tss-pick');
  ul.setAttribute('aria-label', label);
  const buttons = items.map((it, n) => {
    const li = k.el('li');
    const b = k.el('button', 'sw-tree-item tss-pick-item');
    if (!it.applied) b.classList.add('is-off');
    if (it.bad) b.classList.add('is-bad');
    b.type = 'button';
    b.dataset.key = k.key(tab, 'pick', n);
    b.dataset.nobusy = '1';
    b.title = it.title;
    b.setAttribute('aria-keyshortcuts', 'Alt+Shift+ArrowUp Alt+Shift+ArrowDown');
    b.append(k.el('span', 'sw-tree-name', it.name), k.el('span', it.applied ? 'sw-tree-count setup-num' : 'sw-tree-count', it.figure));
    if (it.bad) b.append(k.el('span', 'setup-sr', t('topsheet.what_has_a_problem', { what })));
    b.addEventListener('click', () => pick(n));
    b.addEventListener('keydown', (e) => listKey(e, n));
    li.append(b);
    ul.append(li);
    return b;
  });

  // Arrows pick the next or previous item; Option+Shift+arrows move it.
  const listKey = (e, n) => {
    if (e.altKey && e.shiftKey && !e.metaKey && !e.ctrlKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown')) {
      e.preventDefault();
      e.stopPropagation();
      if (k.isBusy()) return;
      if (n !== at) pick(n);
      const by = e.key === 'ArrowUp' ? -1 : 1;
      moveBy(by, k.key(tab, 'pick', n + by));
      return;
    }
    if (e.altKey || e.metaKey || e.ctrlKey || e.shiftKey) return;
    const j = { ArrowDown: n + 1, ArrowUp: n - 1, Home: 0, End: count - 1 }[e.key];
    if (j === undefined) return;
    e.preventDefault();
    e.stopPropagation();
    if (j < 0 || j >= count) return;
    pick(j);
    buttons[j].focus();
  };

  // The strip's buttons, the list and the footer follow the picked item.
  const sync = () => {
    items.forEach((it, n) => {
      it.editor.hidden = n !== at;
      buttons[n].setAttribute('aria-current', n === at ? 'true' : 'false');
    });
    const name = count ? items[at].name : '';
    const able = (b, on, text, none) => {
      if (on) b.removeAttribute('aria-disabled');
      else b.setAttribute('aria-disabled', 'true');
      b.title = on ? text : none;
      b.setAttribute('aria-label', on ? text : none);
    };
    const first = t('topsheet.add_a_what_first', { what });
    able(up, count > 0 && at > 0, t('topsheet.move_what_up', { what, name }), count ? t('topsheet.name_is_first', { name }) : first);
    able(down, count > 0 && at < count - 1, t('topsheet.move_what_down', { what, name }), count ? t('topsheet.name_is_last', { name }) : first);
    able(del, count > 0, t('topsheet.delete_what_named', { what, name }), first);
    able(save, count > 0, t('topsheet.copy_to_library', { what, name }), first);
    setFooterTotals(foot(at));
  };

  const pick = (n) => {
    if (n === at) return;
    at = n;
    picked[tab] = n;
    const open = editors.querySelector('.tss-confirm');
    if (open) open.remove();
    sync();
    if (onShow) onShow(n);
  };

  const listLabel = k.el('span', 'sw-rail-label', label);
  listLabel.setAttribute('aria-hidden', 'true');
  const list = k.el('div', 'sw-rail-field');
  list.append(listLabel, count ? ul : k.el('p', 'tss-pick-empty', t('topsheet.none_yet')));
  add.classList.add('tss-rail-add');
  const rail = railHost();
  if (rail) rail.append(list, add);

  if (!count) editors.append(k.emptyNote(empty));
  items.forEach((it, n) => {
    it.editor.id = k.nextId();
    buttons[n].setAttribute('aria-controls', it.editor.id);
    editors.append(it.editor);
  });
  mountFrame(panel, {
    strip: actionStrip(label, [neu, up, down, del, save]),
    // No rail (it shows the library): the list and the add form go first.
    body: rail ? [editors, ...extra] : [list, add, editors, ...extra],
  });
  sync();
  if (count && onShow) onShow(at);
}

// An item's editor: errors and problems go at its end (setup.js finds it by
// data-item).
function editorShell(itemKey, label) {
  const ed = k.el('section', 'tss-ed');
  ed.dataset.item = itemKey;
  ed.setAttribute('aria-label', label);
  return ed;
}

// ---------------------------------------------------------------------------
// Charges

function chargeInput(c0) {
  const c = obj(c0);
  return {
    name: k.str(c.name),
    account: isNone(c.account) ? null : k.str(c.account),
    applied: c.applied !== false,
    kind: c.kind === 'flat' ? 'flat' : 'percent',
    percent: isNone(c.percent) ? null : k.str(c.percent),
    amount: isNone(c.amount) ? null : k.centsToText(c.amount),
    currency: isNone(c.currency) ? null : k.str(c.currency),
    excluded_amounts: arr(c.excluded_amounts).map((x) => ({
      description: k.str(obj(x).description),
      amount: k.centsToText(obj(x).amount),
    })),
    excluded_groups: arr(c.excluded_groups).map(k.str),
    excluded_categories: arr(c.excluded_categories).map(k.str),
    excluded_accounts: arr(c.excluded_accounts).map(k.str),
    label_percent: c.label_percent === true,
    label_excluded: c.label_excluded === true,
  };
}

// set_charge for charge `index`, from the newest view, with `patch` applied.
// `patch` may be a function of the current input.
function chargeArgs(index, patch) {
  const cur = chargeInput(arr(k.budget().charges)[index]);
  const p = typeof patch === 'function' ? patch(cur) : patch;
  return ['set_charge', { index, charge: { ...cur, ...p } }];
}

function currencyOptions(current) {
  const base = k.baseCode();
  const codes = k.entries(k.budget().currencies).map(([c]) => c);
  if (current && current !== base && !codes.includes(current)) codes.push(current);
  return [['', base ? t('topsheet.base_named', { base }) : t('topsheet.base_currency')], ...codes.map((c) => [c, c])];
}

// Per charge, what fills its editor's left-out totals when it shows, and
// its footer figures as saved: { applied, entry }.
let chargeLoaders = [];
let chargeFeet = [];

// The footer with charge `i` picked: how many charges, its total and what
// it leaves out. `live` is a total figured from typed text (preview_charge);
// `stale` dims it while the text isn't a number yet.
function chargeFoot(i, live = null, stale = false) {
  const out = [{ label: '', value: t('topsheet.count.charges', { count: chargeFeet.length }) }];
  const f = chargeFeet[i];
  if (!f) return out;
  if (!f.applied) {
    out.push({ label: t('topsheet.total_charge'), value: t('topsheet.not_applied') });
    return out;
  }
  const tot = live || f.entry;
  if (!tot) return out;
  out.push(
    {
      label: t('topsheet.total_charge'),
      value: k.formatCents(tot.amount),
      title: stale ? t('topsheet.finish_typing_a_number_to_see_the') : chargeTitle(tot, Boolean(live)),
      dim: stale,
    },
    { label: t('topsheet.left_out'), value: k.formatCents(tot.excluded), title: t('topsheet.everything_the_charge_leaves_out_each_line'), dim: stale },
  );
  return out;
}

function renderCharges(panel) {
  const b = k.budget();
  const r = k.report();
  const charges = arr(b.charges);
  chargeLoaders = [];
  chargeFeet = [];
  preview.live = null;
  const items = charges.map((c0, i) => {
    const c = chargeInput(c0);
    const entry = appliedEntry(charges, r && r.charges, i);
    const issues = issuesAt('charge', i);
    chargeFeet[i] = { applied: c.applied, entry: entry || null };
    const fig = !c.applied ? t('topsheet.not_applied') : entry ? k.formatCents(entry.amount) : '';
    const figTitle = !c.applied ? t('topsheet.not_applied') : entry ? t('topsheet.total_charge_on', { on: chargeTitle(entry, false).toLowerCase() }) : '';
    return {
      name: c.name,
      applied: c.applied,
      figure: fig,
      title: [c.name, figTitle, issues.length ? t('topsheet.this_charge_has_a_problem') : ''].filter(Boolean).join('\n'),
      bad: issues.length > 0,
      editor: chargeEditor(i, c0, c, issues),
    };
  });

  const taken = {};
  for (const c of charges) taken[k.str(obj(c).name)] = true;
  const add = k.addForm('charges', {
    title: t('topsheet.new_charge'),
    fields: [
      k.addInput('charges', 'name', { label: t('topsheet.name'), placeholder: t('topsheet.insurance') }),
      k.addSelect('charges', 'kind', t('topsheet.type'), [
        ['percent', t('topsheet.percent')],
        ['flat', t('topsheet.flat_fee')],
      ]),
      k.addInput('charges', 'value', { label: t('topsheet.percent_or_amount'), placeholder: '10', num: true }),
    ],
    validate: (v) => {
      const name = v.name.trim();
      if (name === '') return ['name', t('topsheet.enter_a_charge_name')];
      if (taken[name]) return ['name', t('topsheet.charge_exists', { name })];
      const val = v.value.trim();
      if (val === '') return ['value', v.kind === 'flat' ? t('topsheet.enter_an_amount') : t('topsheet.enter_a_percent')];
      if (v.kind !== 'flat' && !DECIMAL.test(val)) return ['value', t('topsheet.the_percent_must_be_a_number_like')];
      return null;
    },
    build: (v) => {
      const flat = v.kind === 'flat';
      // The new charge is picked once it shows up.
      pickNew.charges = arr(k.budget().charges).length;
      return [
        'set_charge',
        {
          index: null,
          charge: {
            name: v.name.trim(),
            account: null,
            applied: true,
            kind: flat ? 'flat' : 'percent',
            percent: flat ? null : v.value.trim(),
            amount: flat ? v.value.trim() : null,
            currency: null,
            excluded_amounts: [],
            excluded_groups: [],
            excluded_categories: [],
            excluded_accounts: [],
            label_percent: false,
            label_excluded: false,
          },
        },
      ];
    },
  });

  pickEditor(panel, {
    tab: 'charges',
    what: t('topsheet.what.charge'),
    label: t('topsheet.charges'),
    library: 'charge',
    items,
    add,
    empty: t('topsheet.no_charges_yet'),
    move: 'move_charge',
    deleteText: (i) => t('topsheet.delete_charge', { name: items[i].name }),
    deleteBuild: (i) => ['delete_charge', { index: i }],
    foot: (i) => chargeFoot(i),
    onShow: (i) => {
      if (chargeLoaders[i]) chargeLoaders[i]();
    },
  });
}

// Charge `i`'s editor: a band with its name, Applied, type, value and
// account; one with the topsheet label choices; then what it leaves out.
// Its total and what it leaves out in all are the footer's.
function chargeEditor(i, c0, c, issues) {
  const ik = k.key('charges', i);
  const fk = (f) => k.key('charges', i, f);
  const commit = (f, patchOf) => (v, old) =>
    k.run(ik, () => chargeArgs(i, patchOf(v)), { restore: [fk(f), old] });
  const ed = editorShell(ik, t('topsheet.charge_named', { name: c.name }));

  const name = k.textInput({ fieldKey: fk('name'), value: c.name, onCommit: commit('name', (v) => ({ name: v })) });
  const applied = checkbox(fk('applied'), c.applied, t('topsheet.applied'), (v) =>
    k.run(ik, () => chargeArgs(i, { applied: v }), { restore: [fk('applied'), !v] }),
  );
  const kind = k.selectInput({
    fieldKey: fk('kind'),
    value: c.kind,
    options: [
      ['percent', t('topsheet.percent_of_the_total')],
      ['flat', t('topsheet.flat_fee')],
    ],
    onCommit: commit('kind', (v) => (cur) =>
      v === 'flat'
        ? { kind: 'flat', amount: cur.amount === null ? '0' : cur.amount }
        : { kind: 'percent', percent: cur.percent === null ? '0' : cur.percent },
    ),
  });
  const values = [];
  if (c.kind === 'percent') {
    const pct = k.textInput({
      fieldKey: fk('percent'),
      value: k.str(c.percent),
      num: true,
      onCommit: commit('percent', (v) => ({ percent: v.trim() })),
    });
    livePreview(pct, i);
    values.push(bandInput(t('topsheet.percent'), pct, { size: 'tss-w-pct', unit: '%' }));
  } else {
    const amount = k.textInput({
      fieldKey: fk('amount'),
      value: isNone(c0.amount) ? '' : k.formatCents(c0.amount),
      num: true,
      onCommit: commit('amount', (v) => ({ amount: v.trim() })),
    });
    livePreview(amount, i);
    const cur = k.selectInput({
      fieldKey: fk('currency'),
      value: c.currency || '',
      options: currencyOptions(c.currency),
      onCommit: commit('currency', (v) => ({ currency: v === '' ? null : v })),
    });
    values.push(bandInput(t('topsheet.amount'), amount, { size: 'tss-w-num' }), bandInput(t('topsheet.currency'), cur));
  }
  const account = k.textInput({
    fieldKey: fk('account'),
    value: c.account || '',
    placeholder: t('topsheet.none'),
    onCommit: commit('account', (v) => ({ account: v.trim() === '' ? null : v.trim() })),
  });
  ed.append(
    band(
      t('topsheet.charge_named', { name: c.name }),
      bandInput(t('topsheet.name'), name, { size: 'tss-w-name' }),
      applied,
      bandInput(t('topsheet.type'), kind),
      ...values,
      bandInput(t('topsheet.posts_to_account'), account, { size: 'tss-w-acct' }),
    ),
  );

  // What the topsheet label shows besides the name (display only).
  const labels = [];
  if (c.kind === 'percent') {
    labels.push(
      checkbox(fk('label-percent'), c.label_percent, t('topsheet.show_the_percent_on_the_topsheet'), (v) =>
        k.run(ik, () => chargeArgs(i, { label_percent: v }), { restore: [fk('label-percent'), !v] }),
      ),
    );
  }
  labels.push(
    checkbox(fk('label-excluded'), c.label_excluded, t('topsheet.show_the_amount_left_out_on_the'), (v) =>
      k.run(ik, () => chargeArgs(i, { label_excluded: v }), { restore: [fk('label-excluded'), !v] }),
    ),
  );
  ed.append(band(t('topsheet.topsheet_label'), bandTitle(t('topsheet.topsheet_label')), ...labels));
  ed.append(...chargeExclusions(i, c, ik));
  k.finishItem(ed, ik, issues, null);
  return ed;
}

// The footer figure's tooltip: what the charge is figured on.
function chargeTitle(tot, typing) {
  const params = { base: k.formatCents(tot.base), excluded: k.formatCents(tot.excluded) };
  return typing ? t('topsheet.charge_on_typing', params) : t('topsheet.charge_on', params);
}

// Live preview: while a charge's percent, amount or a left-out amount is
// being typed, its total (and what it leaves out) is figured from the typed
// text, before Enter commits it, and shown in the footer. Charges whose
// total isn't shown (not applied) have nothing to update.
const preview = { timer: null, seq: 0, live: null };
const PREVIEW_MS = 120;

function livePreview(input, i) {
  input.addEventListener('input', () => {
    clearTimeout(preview.timer);
    preview.timer = setTimeout(() => runPreview(i), PREVIEW_MS);
  });
}

// Charge `i` as the editor shows it right now: the saved charge with every
// typed-but-not-committed value in its place.
function draftCharge(i) {
  const cur = chargeInput(arr(k.budget().charges)[i]);
  const typed = (f) => {
    const e = k.findByData('key', k.key('charges', i, f));
    return e && typeof e.value === 'string' ? e.value : null;
  };
  if (cur.kind === 'percent') {
    const v = typed('percent');
    if (v !== null) cur.percent = v.trim();
  } else {
    const v = typed('amount');
    if (v !== null) cur.amount = v.trim();
  }
  cur.excluded_amounts = cur.excluded_amounts.map((x, n) => {
    const v = typed(`amount-value-${n}`);
    return v === null ? x : { ...x, amount: v.trim() };
  });
  return cur;
}

async function runPreview(i) {
  const f = chargeFeet[i];
  if (!f || !f.applied || !f.entry || picked.charges !== i) return;
  const seq = ++preview.seq;
  const shown = () => seq === preview.seq && picked.charges === i && chargeFeet[i] === f;
  try {
    const got = await invoke('preview_charge', { index: i, charge: draftCharge(i) });
    if (!shown()) return;
    preview.live = { i, t: got };
    setFooterTotals(chargeFoot(i, got));
  } catch {
    // Not a number yet: keep the last total, dimmed as not current.
    if (!shown()) return;
    const last = preview.live && preview.live.i === i ? preview.live.t : null;
    setFooterTotals(chargeFoot(i, last, true));
  }
}

// What leaving out one group, category or account takes off a charge, as
// the totals work it out (each line with its fringes): the saved charge with
// only that exclusion, through preview_charge. The figure doesn't depend on
// the charge, so it is kept per budget revision by what is left out; the
// last one shows until the new one arrives.
const leftOut = { stamp: '', now: new Map(), last: new Map() };

function leftOutTotal(i, f, name, span) {
  const v = obj(k.view());
  const stamp = `${v.doc}:${v.rev}`;
  if (leftOut.stamp !== stamp) {
    leftOut.stamp = stamp;
    leftOut.now = new Map();
  }
  const id = JSON.stringify([v.doc, f, name]);
  if (leftOut.last.has(id)) span.textContent = k.formatCents(leftOut.last.get(id));
  let p = leftOut.now.get(id);
  if (!p) {
    const one = {
      ...chargeInput(arr(k.budget().charges)[i]),
      excluded_amounts: [],
      excluded_groups: [],
      excluded_categories: [],
      excluded_accounts: [],
      [f]: [name],
    };
    p = invoke('preview_charge', { index: i, charge: one }).then((got) => {
      leftOut.last.set(id, got.excluded);
      return got.excluded;
    });
    leftOut.now.set(id, p);
  }
  p.then(
    (cents) => {
      span.textContent = k.formatCents(cents);
    },
    () => {
      span.textContent = '';
    },
  );
}

// What charge `i` leaves out: a band with the ways to leave more out, then
// one table: amounts typed in, then groups, categories and accounts, each
// with what it takes off. An item the charge names that the budget no
// longer has stays listed until it is removed.
function chargeExclusions(i, c, ik) {
  const fk = (f) => k.key('charges', i, f);
  const rawAmounts = arr(obj(arr(k.budget().charges)[i]).excluded_amounts);
  const groupDefs = obj(k.budget().groups);
  const cats = categoryNames();
  const accts = accountNames();

  const leave = (f, value) =>
    k.run(ik, () => chargeArgs(i, (cur) => ({ [f]: [...cur[f].filter((x) => x !== value), value] })));
  const named = (map, f) =>
    [...map.keys()].filter((code) => !c[f].includes(code)).map((code) => [code, `${code} ${map.get(code)}`.trim()]);
  const adders = band(
    t('topsheet.left_out_of', { name: c.name }),
    bandTitle(t('topsheet.left_out')),
    bandButton(t('topsheet.exclude_amount'), fk('amount-add'), () => {
      k.setFocusKey(fk(`amount-desc-${c.excluded_amounts.length}`));
      k.run(ik, () => chargeArgs(i, (cur) => ({ excluded_amounts: [...cur.excluded_amounts, { description: '', amount: '0' }] })), {
        onError: () => k.setFocusKey(null),
      });
    }),
    addPicker(
      fk('excl-add-group'),
      t('topsheet.exclude_group'),
      groupNames()
        .filter((g) => !c.excluded_groups.includes(g))
        .map((g) => [g, g]),
      t('topsheet.no_groups_left'),
      (g) => leave('excluded_groups', g),
    ),
    addPicker(fk('excl-add-category'), t('topsheet.exclude_category'), named(cats, 'excluded_categories'), t('topsheet.no_categories_left'), (v) =>
      leave('excluded_categories', v),
    ),
    addPicker(fk('excl-add-account'), t('topsheet.exclude_account'), named(accts, 'excluded_accounts'), t('topsheet.no_accounts_left'), (v) =>
      leave('excluded_accounts', v),
    ),
  );

  const ul = k.el('ul');
  ul.setAttribute('aria-label', t('topsheet.left_out_of', { name: c.name }));
  // Each row's delete key, so focus can go to a neighbour once one is removed.
  const rows = [];
  const focusAfter = (p) => {
    const next = rows[p + 1];
    const prev = rows[p - 1];
    if (next) return next.amount !== undefined && rows[p].amount !== undefined ? fk(`amount-del-${next.amount - 1}`) : next.del;
    if (prev) return prev.del;
    return fk('amount-add');
  };
  const removeButton = (delKey, label, build) => {
    const p = rows.length;
    return button('×', 'setup-del', delKey, () => {
      k.setFocusKey(focusAfter(p));
      k.run(ik, () => chargeArgs(i, build), { onError: () => k.setFocusKey(null) });
    }, label);
  };

  c.excluded_amounts.forEach((x, n) => {
    const desc = k.textInput({
      fieldKey: fk(`amount-desc-${n}`),
      value: x.description,
      placeholder: t('topsheet.description'),
      label: t('topsheet.description_of_amount', { n: n + 1 }),
      cls: 'sw-cell-input',
      onCommit: (v, old) =>
        k.run(
          ik,
          () => chargeArgs(i, (cur) => ({ excluded_amounts: cur.excluded_amounts.map((y, j) => (j === n ? { ...y, description: v } : y)) })),
          { restore: [fk(`amount-desc-${n}`), old] },
        ),
    });
    const amt = k.textInput({
      fieldKey: fk(`amount-value-${n}`),
      value: k.formatCents(obj(rawAmounts[n]).amount),
      num: true,
      label: t('topsheet.amount_n', { n: n + 1 }),
      cls: 'sw-cell-input',
      onCommit: (v, old) =>
        k.run(
          ik,
          () => chargeArgs(i, (cur) => ({ excluded_amounts: cur.excluded_amounts.map((y, j) => (j === n ? { ...y, amount: v.trim() } : y)) })),
          { restore: [fk(`amount-value-${n}`), old] },
        ),
    });
    livePreview(amt, i);
    const delKey = fk(`amount-del-${n}`);
    const del = removeButton(delKey, t('topsheet.remove_amount', { n: n + 1 }), (cur) => ({
      excluded_amounts: cur.excluded_amounts.filter((_, j) => j !== n),
    }));
    rows.push({ del: delKey, amount: n });
    const li = gridRow();
    li.append(lead(), textCell(t('topsheet.amount'), { muted: true }), cell(desc, 'tss-span2'), cell(amt), acts(del));
    ul.append(li);
  });

  const loads = [];
  const namedRow = (type, f, name, desc, known, title) => {
    const delKey = fk(`excl-${type}-del-${name}`);
    const removeLabel = { group: t('topsheet.remove_group', { name }), category: t('topsheet.remove_category', { name }), account: t('topsheet.remove_account', { name }) }[type];
    const del = removeButton(delKey, removeLabel, (cur) => ({ [f]: cur[f].filter((x) => x !== name) }));
    rows.push({ del: delKey });
    const figure = k.el('span', 'sw-num');
    const total = cell(figure, 'sw-num');
    if (known) {
      total.title = title;
      loads.push(() => leftOutTotal(i, f, name, figure));
    }
    const descCell = known ? textCell(desc, { title: desc }) : textCell(t('topsheet.not_in_this_budget'), { muted: true });
    const li = gridRow();
    li.append(
      lead(),
      textCell({ group: t('topsheet.excl.group'), category: t('topsheet.excl.category'), account: t('topsheet.excl.account') }[type], { muted: true }),
      textCell(name, { title: name }),
      descCell,
      total,
      acts(del),
    );
    ul.append(li);
  };
  const usual = t('topsheet.its_lines_with_their_fringes_as_the');
  for (const g of c.excluded_groups) {
    const known = Object.prototype.hasOwnProperty.call(groupDefs, g);
    const def = obj(groupDefs[g]);
    const title = def.in_budget_total === false ? t('topsheet.left_out_of_the_budget_total_so') : usual;
    namedRow('group', 'excluded_groups', g, k.str(def.description), known, title);
  }
  for (const code of c.excluded_categories) {
    namedRow('category', 'excluded_categories', code, cats.get(code) || '', cats.has(code), usual);
  }
  for (const code of c.excluded_accounts) {
    namedRow('account', 'excluded_accounts', code, accts.get(code) || '', accts.has(code), usual);
  }
  chargeLoaders[i] = () => loads.forEach((fn) => fn());
  if (!rows.length) return [adders, k.emptyNote(t('topsheet.nothing_left_out'))];
  return [
    adders,
    gridTable(
      'charge-excl',
      [lead(), head(t('topsheet.type')), head(t('topsheet.name')), head(t('topsheet.description')), head(t('topsheet.total'), { num: true, title: t('topsheet.what_leaving_it_out_takes_off_the') }), head('')],
      ul,
    ),
  ];
}

// ---------------------------------------------------------------------------
// Credits

function creditRowInput(r0) {
  const r = obj(r0);
  const src = obj(r.source);
  const cap = obj(r.cap);
  return {
    description: k.str(r.description),
    source:
      src.kind === 'group'
        ? { kind: 'group', group: k.str(src.group) }
        : src.kind === 'category'
          ? { kind: 'category', category: k.str(src.category) }
          : src.kind === 'account'
            ? { kind: 'account', account: k.str(src.account) }
            : { kind: 'flat', amount: k.centsToText(isNone(src.amount) ? 0 : src.amount) },
    percent: k.str(r.percent) || '0',
    cap:
      cap.kind === 'qualified_spend' || cap.kind === 'credit'
        ? { kind: cap.kind, amount: k.centsToText(cap.amount) }
        : { kind: 'none' },
  };
}

function creditInput(c0) {
  const c = obj(c0);
  return {
    name: k.str(c.name),
    account: isNone(c.account) ? null : k.str(c.account),
    applied: c.applied !== false,
    cap: isNone(c.cap) ? null : k.centsToText(c.cap),
    discount_percent: k.str(c.discount_percent) || '0',
    // One list per credit, for all its group, category and account rows
    // (decisions.md, entries 57 and 61), so changing a row's spend keeps it.
    excluded_fringes: arr(c.excluded_fringes).map(k.str),
    // Where the spend has to be (decisions.md, entry 80); none: anywhere.
    locations: arr(c.locations).map(k.str),
    // The program's terms (decisions.md, entry 88). A shared cap's amount
    // is the budget's, sent only when it changes.
    minimum_spend: isNone(c.minimum_spend) ? null : k.centsToText(c.minimum_spend),
    broker_fee: isNone(c.broker_fee) || c.broker_fee === 0 ? null : k.centsToText(c.broker_fee),
    shared_cap: isNone(c.shared_cap) ? null : k.str(c.shared_cap),
    shared_cap_amount: null,
    rows: arr(c.rows).map(creditRowInput),
  };
}

// The budget's shared caps by name, in cents (decisions.md, entry 88).
function sharedCaps() {
  const out = {};
  for (const [n, v] of k.entries(k.budget().shared_caps)) out[n] = obj(v).amount;
  return out;
}

function creditArgs(index, patch) {
  const cur = creditInput(arr(k.budget().credits)[index]);
  const p = typeof patch === 'function' ? patch(cur) : patch;
  return ['set_credit', { index, credit: { ...cur, ...p } }];
}

function rowPatch(n, f) {
  return (cur) => ({ rows: cur.rows.map((r, j) => (j === n ? { ...r, ...f(r) } : r)) });
}

// Per credit, its footer figures: { applied, entry, rows (the report's) }.
let creditFeet = [];

// The footer with credit `i` picked: how many credits, its rows' spend,
// fringes left out, net spend and credit added up, then the discount and the
// net credit taken off the grand total.
export function creditFoot(i) {
  const out = [{ label: '', value: t('topsheet.count.credits', { count: creditFeet.length }) }];
  const f = creditFeet[i];
  if (!f) return out;
  if (!f.applied) {
    // Not taken off the grand total: its estimate, once it has come.
    const p = estimateOf(i);
    if (p && !isNone(p.amount)) {
      out.push({ label: t('topsheet.qualified'), value: k.formatCents(p.qualified), title: t('topsheet.the_rows_net_spend_added_up') });
      out.push({
        label: isNone(p.problem) ? t('topsheet.estimate') : t('topsheet.estimate_may_be_short'),
        value: k.formatCents(p.amount),
        title: isNone(p.problem) ? t('topsheet.what_this_credit_would_take_off_the') : k.str(p.problem),
      });
    } else {
      out.push({ label: t('topsheet.net_credit'), value: t('topsheet.not_applied') });
    }
    return out;
  }
  if (f.rows.length) {
    const add = (field) => {
      const s = centsSum(f.rows.map((r) => (isNone(r[field]) ? 0 : r[field])));
      return s === null ? '' : k.formatCents(s);
    };
    const credit = decimalSum(f.rows.map((r) => r.credit));
    out.push(
      { label: t('topsheet.gross'), value: add('gross'), title: t('topsheet.the_rows_spend_before_fringes_are_left_2') },
      { label: t('topsheet.fringes_out'), value: add('excluded_fringes'), title: t('topsheet.fringes_left_out_of_the_spend') },
      { label: t('topsheet.net'), value: add('net'), title: t('topsheet.the_spend_the_percents_apply_to') },
      {
        label: t('topsheet.credit'),
        value: credit === null ? '' : money(credit),
        title: credit === null ? '' : t('topsheet.rows_added_up_exactly', { exact: k.formatDecimal(credit) }),
      },
    );
  }
  if (f.entry) {
    const e = f.entry;
    const fee = isNone(e.fee) ? 0n : BigInt(e.fee);
    // Below the minimum spend the credit is 0 (decisions.md, entry 88).
    if (e.below_minimum === true) {
      out.push({ label: t('topsheet.minimum'), value: t('topsheet.not_met'), title: t('topsheet.the_rows_net_spend_is_below_the') });
    }
    // Its share of a cap it shares with other credits, in whole cents.
    if (!isNone(e.shared)) {
      out.push({ label: t('topsheet.shared_cap'), value: k.formatCents(e.shared), title: t('topsheet.share_of_shared_cap', { exact: k.formatDecimal(e.capped) }) });
    }
    // The discount comes off after the caps, then the fee; the credit is
    // rounded once.
    const before = !isNone(e.shared) ? BigInt(e.shared) : dollarsToCents(e.capped);
    const after = BigInt(e.amount) + fee;
    if (before !== null && before !== after) {
      out.push({
        label: t('topsheet.discount'),
        value: k.formatCents(before - after),
        title: t('topsheet.exactly_before_discount', { exact: isNone(e.shared) ? k.formatDecimal(e.capped) : k.formatCents(e.shared) }),
      });
    }
    if (fee !== 0n) out.push({ label: t('topsheet.brokers_fee'), value: k.formatCents(fee), title: t('topsheet.the_flat_fee_taken_off_after_the') });
    out.push({ label: t('topsheet.net_credit'), value: k.formatCents(e.amount), title: t('topsheet.credit_taken_off_the_grand_total') });
  }
  return out;
}

function renderCredits(panel) {
  const b = k.budget();
  const r = k.report();
  const credits = arr(b.credits);
  creditFeet = [];
  const items = credits.map((c0, i) => {
    const c = creditInput(c0);
    const entry = appliedEntry(credits, r && r.credits, i);
    const issues = issuesAt('credit', i);
    creditFeet[i] = { applied: c.applied, entry: entry || null, rows: entry && Array.isArray(entry.rows) ? entry.rows.map(obj) : [] };
    return {
      name: c.name,
      applied: c.applied,
      figure: !c.applied ? t('topsheet.not_applied') : entry ? k.formatCents(entry.amount) : '',
      title: [c.name, !c.applied ? t('topsheet.not_applied') : entry ? t('topsheet.credit_taken_off_the_grand_total') : '', issues.length ? t('topsheet.this_credit_has_a_problem') : '']
        .filter(Boolean)
        .join('\n'),
      bad: issues.length > 0,
      editor: creditEditor(i, c0, c, entry, issues),
    };
  });

  const taken = {};
  for (const c of credits) taken[k.str(obj(c).name)] = true;
  const add = k.addForm('credits', {
    title: t('topsheet.new_credit'),
    fields: [k.addInput('credits', 'name', { label: t('topsheet.name'), placeholder: t('topsheet.state_incentive') })],
    validate: (v) => {
      const name = v.name.trim();
      if (name === '') return ['name', t('topsheet.enter_a_credit_name')];
      if (taken[name]) return ['name', t('topsheet.credit_exists', { name })];
      return null;
    },
    build: (v) => {
      // The new credit is picked once it shows up.
      pickNew.credits = arr(k.budget().credits).length;
      return [
        'set_credit',
        {
          index: null,
          credit: {
            name: v.name.trim(),
            account: null,
            applied: true,
            cap: null,
            discount_percent: '0',
            excluded_fringes: [],
            locations: [],
            minimum_spend: null,
            broker_fee: null,
            shared_cap: null,
            shared_cap_amount: null,
            rows: [],
          },
        },
      ];
    },
  });

  pickEditor(panel, {
    tab: 'credits',
    what: t('topsheet.what.credit'),
    label: t('topsheet.credits'),
    library: 'credit',
    items,
    add,
    empty: t('topsheet.no_credits_yet'),
    move: 'move_credit',
    deleteText: (i) => t('topsheet.delete_credit', { name: items[i].name, count: arr(obj(credits[i]).rows).length }),
    deleteBuild: (i) => ['delete_credit', { index: i }],
    foot: (i) => creditFoot(i),
    extra: [programsBox()],
  });
  showEstimates();
}

// Credit `i`'s editor: a band with its name, Applied, cap, discount and
// account; one with the fringes left out of its spend; one with the
// locations its spend has to be at; one with its minimum spend, broker's fee
// and shared cap; one with Add row;
// then its rows as one table. Its figures are the footer's.
function creditEditor(i, c0, c, entry, issues) {
  const raw = obj(c0);
  const ik = k.key('credits', i);
  const fk = (f) => k.key('credits', i, f);
  const commit = (f, patchOf) => (v, old) =>
    k.run(ik, () => creditArgs(i, patchOf(v)), { restore: [fk(f), old] });
  const ed = editorShell(ik, t('topsheet.credit_named', { name: c.name }));

  const name = k.textInput({ fieldKey: fk('name'), value: c.name, onCommit: commit('name', (v) => ({ name: v })) });
  const applied = checkbox(fk('applied'), c.applied, t('topsheet.applied'), (v) =>
    k.run(ik, () => creditArgs(i, { applied: v }), { restore: [fk('applied'), !v] }),
  );
  const cap = k.textInput({
    fieldKey: fk('cap'),
    value: isNone(raw.cap) ? '' : k.formatCents(raw.cap),
    num: true,
    placeholder: t('topsheet.none'),
    onCommit: commit('cap', (v) => ({ cap: v.trim() === '' ? null : v.trim() })),
  });
  const discount = k.textInput({
    fieldKey: fk('discount'),
    value: c.discount_percent,
    num: true,
    onCommit: commit('discount', (v) => ({ discount_percent: v.trim() })),
  });
  const account = k.textInput({
    fieldKey: fk('account'),
    value: c.account || '',
    placeholder: t('topsheet.none'),
    onCommit: commit('account', (v) => ({ account: v.trim() === '' ? null : v.trim() })),
  });
  ed.append(
    band(
      t('topsheet.credit_named', { name: c.name }),
      bandInput(t('topsheet.name'), name, { size: 'tss-w-name' }),
      applied,
      bandInput(t('topsheet.cap_on_the_credit'), cap, { size: 'tss-w-num' }),
      bandInput(t('topsheet.discount'), discount, { size: 'tss-w-pct', unit: '%' }),
      bandInput(t('topsheet.posts_to_account'), account, { size: 'tss-w-acct' }),
    ),
  );

  ed.append(
    band(
      t('topsheet.fringes_left_out_of_the_spend'),
      chips({
        fieldKey: fk('fringes'),
        label: t('topsheet.fringes_left_out_of_the_spend'),
        selected: c.excluded_fringes,
        options: withKept(fringeNames(), c.excluded_fringes),
        onChange: (list) => k.run(ik, () => creditArgs(i, { excluded_fringes: list })),
      }),
    ),
  );

  // Only lines at these locations count toward its group, category and
  // account rows (decisions.md, entry 80); none picked: every line.
  ed.append(
    band(
      t('topsheet.spend_at_these_locations'),
      chips({
        fieldKey: fk('locations'),
        label: t('topsheet.spend_at_these_locations'),
        selected: c.locations,
        options: withKept(locationNames(), c.locations),
        emptyText: t('topsheet.anywhere'),
        onChange: (list) => k.run(ik, () => creditArgs(i, { locations: list })),
      }),
    ),
  );

  // The program's terms (decisions.md, entry 88): a minimum spend, a flat
  // broker's fee, and a cap it shares with other credits by name.
  const caps = sharedCaps();
  const capName = c.shared_cap || '';
  const minimum = k.textInput({
    fieldKey: fk('minimum'),
    value: isNone(raw.minimum_spend) ? '' : k.formatCents(raw.minimum_spend),
    num: true,
    placeholder: t('topsheet.none'),
    onCommit: commit('minimum', (v) => ({ minimum_spend: v.trim() === '' ? null : v.trim() })),
  });
  const fee = k.textInput({
    fieldKey: fk('fee'),
    value: isNone(raw.broker_fee) || raw.broker_fee === 0 ? '' : k.formatCents(raw.broker_fee),
    num: true,
    placeholder: t('topsheet.none'),
    onCommit: commit('fee', (v) => ({ broker_fee: v.trim() === '' ? null : v.trim() })),
  });
  const capAmount = k.textInput({
    fieldKey: fk('shared-amount'),
    value: capName !== '' && !isNone(caps[capName]) ? k.formatCents(caps[capName]) : '',
    num: true,
    placeholder: capName === '' ? t('topsheet.no_shared_cap') : t('topsheet.amount'),
    onCommit: commit('shared-amount', (v) => ({ shared_cap_amount: v.trim() === '' ? null : v.trim() })),
  });
  if (capName === '') capAmount.disabled = true;
  const shared = k.textInput({
    fieldKey: fk('shared'),
    value: capName,
    placeholder: t('topsheet.none'),
    // A new name keeps the amount shown, so it is never a cap without one.
    onCommit: commit('shared', (v) => {
      const n = v.trim();
      if (n === '') return { shared_cap: null };
      const amount = capName !== '' && !isNone(caps[capName]) ? k.centsToText(caps[capName]) : null;
      return { shared_cap: n, shared_cap_amount: isNone(caps[n]) ? amount : null };
    }),
  });
  const sharing = Object.keys(caps).length
    ? t('topsheet.shared_caps_in_budget', {
        list: Object.entries(caps)
          .map(([n, a]) => `${n} ${k.formatCents(a)}`)
          .join(', '),
      })
    : t('topsheet.credits_naming_the_same_shared_cap_never');
  ed.append(
    band(
      t('topsheet.terms_of', { name: c.name }),
      bandInput(t('topsheet.minimum_spend'), minimum, {
        size: 'tss-w-num',
        title: t('topsheet.below_this_qualified_spend_the_rows_net'),
      }),
      bandInput(t('topsheet.brokers_fee'), fee, { size: 'tss-w-num', title: t('topsheet.a_flat_fee_taken_off_after_the') }),
      bandInput(t('topsheet.shared_cap'), shared, { size: 'tss-w-name', title: sharing }),
      bandInput(t('topsheet.shared_cap_amount'), capAmount, {
        size: 'tss-w-num',
        title: t('topsheet.the_shared_caps_amount_for_every_credit'),
      }),
    ),
  );

  const addRow = bandButton(t('topsheet.add_row'), fk('add-row'), () => {
    k.setFocusKey(k.key('credits', i, c.rows.length, 'description'));
    k.run(
      ik,
      () =>
        creditArgs(i, (cur) => ({
          rows: [...cur.rows, { description: '', source: { kind: 'flat', amount: '0' }, percent: '0', cap: { kind: 'none' } }],
        })),
      { onError: () => k.setFocusKey(null) },
    );
  });
  ed.append(band(t('topsheet.rows_of', { name: c.name }), bandTitle(t('topsheet.rows')), addRow));
  ed.append(creditGrid(i, c, raw, entry, ik));
  k.finishItem(ed, ik, issues, null);
  return ed;
}

// A credit's rows as one table: what each is a percent of, its figures from
// the totals, its cap. Their sums are the footer's.
function creditGrid(i, c, raw, entry, ik) {
  if (!c.rows.length) return k.emptyNote(t('topsheet.no_rows_yet'));
  // An unapplied credit shows the figures it would have applied, marked as
  // an estimate (decisions.md, entry 93).
  const est = !c.applied;
  const totals = est ? estimatedRows(i) : entry && Array.isArray(entry.rows) ? entry.rows.map(obj) : [];
  const ul = k.el('ul');
  ul.setAttribute('aria-label', est ? t('topsheet.rows_of_estimated', { name: c.name }) : t('topsheet.rows_of', { name: c.name }));
  c.rows.forEach((row, n) => ul.append(creditRow(i, n, row, raw, totals[n] || null, ik, c.rows.length, est)));
  const table = gridTable('credit-rows', [
    lead(),
    head(t('topsheet.spend')),
    head(t('topsheet.description')),
    head(t('topsheet.amount'), { num: true }),
    head(t('topsheet.gross'), { num: true, title: t('topsheet.the_rows_spend_before_fringes_are_left') }),
    head(t('topsheet.fringes_out'), { num: true, title: t('topsheet.fringes_left_out_of_the_spend') }),
    head(t('topsheet.net'), { num: true, title: t('topsheet.the_spend_the_percent_applies_to') }),
    head(t('topsheet.cap_type')),
    head(t('topsheet.cap'), { num: true }),
    head('%', { num: true }),
    head(t('topsheet.credit'), { num: true }),
    head(t('topsheet.over_cap'), { num: true }),
    head(''),
  ], ul);
  if (!est) return table;
  const wrap = k.el('div', 'tss-estimated');
  wrap.append(k.el('p', 'tss-note', t('topsheet.not_applied_the_row_figures_are_an')), table);
  return wrap;
}

function creditRow(i, n, row, raw, rt, ik, count, est = false) {
  const li = gridRow();
  const fk = (f) => k.key('credits', i, n, f);
  const send = (f, old) => (v) => k.run(ik, () => creditArgs(i, rowPatch(n, () => v)), old === undefined ? {} : { restore: [f, old] });
  const rawRow = obj(arr(raw.rows)[n]);
  const of = (what) => t('topsheet.of_row', { what, n: n + 1 });

  const groups = groupNames();
  const cats = categoryNames();
  const accts = accountNames();
  const current =
    row.source.kind === 'group'
      ? `group:${row.source.group}`
      : row.source.kind === 'category'
        ? `category:${row.source.category}`
        : row.source.kind === 'account'
          ? `account:${row.source.account}`
          : 'flat';
  const catLabel = (code) => (cats.get(code) ? t('topsheet.category_colon_named', { code, name: cats.get(code) }) : t('topsheet.category_colon', { code }));
  const acctLabel = (code) => (accts.get(code) ? t('topsheet.account_colon_named', { code, name: accts.get(code) }) : t('topsheet.account_colon', { code }));
  const spend = k.selectInput({
    fieldKey: fk('source'),
    value: current,
    cls: 'sw-cell-input',
    options: [
      ['flat', t('topsheet.amount_typed_in')],
      ...withKept(groups, row.source.kind === 'group' ? [row.source.group] : []).map((g) => [`group:${g}`, t('topsheet.group_colon', { name: g })]),
      // A topsheet category's total with its fringes (decisions.md, entry 57).
      ...withKept([...cats.keys()], row.source.kind === 'category' ? [row.source.category] : []).map((code) => [`category:${code}`, catLabel(code)]),
      // An account's total with its fringes (decisions.md, entry 61).
      ...withKept([...accts.keys()], row.source.kind === 'account' ? [row.source.account] : []).map((code) => [`account:${code}`, acctLabel(code)]),
    ],
    onCommit: (v, old) =>
      send(fk('source'), old)(
        v === 'flat'
          ? { source: { kind: 'flat', amount: '0' } }
          : v.startsWith('category:')
            ? { source: { kind: 'category', category: v.slice(9) } }
            : v.startsWith('account:')
              ? { source: { kind: 'account', account: v.slice(8) } }
              : { source: { kind: 'group', group: v.slice(6) } },
      ),
  });
  spend.setAttribute('aria-label', of(t('topsheet.spend')));
  spend.title = spend.options[spend.selectedIndex] ? spend.options[spend.selectedIndex].textContent : '';

  const desc = k.textInput({
    fieldKey: fk('description'),
    value: row.description,
    label: of(t('topsheet.description')),
    cls: 'sw-cell-input',
    onCommit: (v, old) => send(fk('description'), old)({ description: v }),
  });

  const amount =
    row.source.kind === 'flat'
      ? k.textInput({
          fieldKey: fk('amount'),
          value: k.formatCents(obj(rawRow.source).amount || 0),
          num: true,
          label: of(t('topsheet.amount')),
          cls: 'sw-cell-input',
          onCommit: (v, old) => send(fk('amount'), old)({ source: { kind: 'flat', amount: v.trim() } }),
        })
      : null;

  const capKind = k.selectInput({
    fieldKey: fk('cap-kind'),
    value: row.cap.kind,
    cls: 'sw-cell-input',
    options: [
      ['none', t('topsheet.none')],
      ['qualified_spend', t('topsheet.qualified_spend')],
      ['credit', t('topsheet.credit_cap')],
    ],
    onCommit: (v, old) => send(fk('cap-kind'), old)({ cap: v === 'none' ? { kind: 'none' } : { kind: v, amount: row.cap.amount || '0' } }),
  });
  capKind.setAttribute('aria-label', of(t('topsheet.cap_type')));
  capKind.title = t('topsheet.qualified_spend_caps_the_spend_the_percent');
  const capAmount =
    row.cap.kind !== 'none'
      ? k.textInput({
          fieldKey: fk('cap-amount'),
          value: k.formatCents(obj(rawRow.cap).amount || 0),
          num: true,
          label: of(t('topsheet.cap')),
          cls: 'sw-cell-input',
          onCommit: (v, old) => send(fk('cap-amount'), old)({ cap: { kind: row.cap.kind, amount: v.trim() } }),
        })
      : null;
  const percent = k.textInput({
    fieldKey: fk('percent'),
    value: row.percent,
    num: true,
    label: of(t('topsheet.percent')),
    cls: 'sw-cell-input',
    onCommit: (v, old) => send(fk('percent'), old)({ percent: v.trim() }),
  });

  const has = rt && !isNone(rt.net);
  // Each figure cell is found by its key, so an estimate can fill it in
  // when it comes; an estimate's tooltip says so.
  const fig = (v, title, f) => {
    const c = textCell(has ? v : '', { num: true, title: has && title ? (est ? t('topsheet.estimate_title', { title }) : title) : '' });
    c.dataset.fig = k.key('credits', i, n, f);
    return c;
  };
  const [overText, overTitle] = has ? overCap(rt, row) : ['', ''];

  // Focus goes to the row taking this one's place, else the one before,
  // else Add row.
  const after = n < count - 1 ? fk('del') : n > 0 ? k.key('credits', i, n - 1, 'del') : k.key('credits', i, 'add-row');
  const del = button('×', 'setup-del', fk('del'), () => {
    k.setFocusKey(after);
    k.run(ik, () => creditArgs(i, (cur) => ({ rows: cur.rows.filter((_, j) => j !== n) })), { onError: () => k.setFocusKey(null) });
  }, t('topsheet.delete_row', { n: n + 1 }));

  li.append(
    lead(),
    cell(spend),
    cell(desc),
    cell(amount),
    fig(has ? k.formatCents(rt.gross) : '', t('topsheet.the_rows_spend_before_fringes_are_left'), 'gross'),
    fig(has ? k.formatCents(rt.excluded_fringes) : '', t('topsheet.fringes_left_out_of_the_spend'), 'fringes'),
    fig(has ? k.formatCents(rt.net) : '', t('topsheet.the_spend_the_percent_applies_to'), 'net'),
    cell(capKind),
    cell(capAmount),
    cell(percent),
    fig(has ? money(rt.credit) : '', has ? t('topsheet.exactly_rows_added', { exact: k.formatDecimal(rt.credit) }) : '', 'credit'),
    fig(overText, overTitle, 'over'),
    acts(del),
  );
  return li;
}

// A credit row against its cap: how far over it is (in parentheses when
// under), and a tooltip that says which and by exactly how much.
export function overCap(rt, row) {
  if (isNone(rt.cap_variance) || row.cap.kind === 'none') return ['', ''];
  const exact = k.formatDecimal(k.str(rt.cap_variance).replace(/^-/, ''));
  const c = dollarsToCents(rt.cap_variance);
  if (c === null) return [k.str(rt.cap_variance), t('topsheet.exactly_against_cap', { exact })];
  if (c === 0n) return [k.formatCents(0), t('topsheet.at_the_cap')];
  const over = c < 0n;
  const spend = row.cap.kind === 'qualified_spend';
  const title = spend
    ? over
      ? t('topsheet.spend_over_cap', { exact })
      : t('topsheet.spend_under_cap', { exact })
    : over
      ? t('topsheet.credit_over_cap', { exact })
      : t('topsheet.credit_under_cap', { exact });
  return [k.formatCents(-c), title];
}

// Dollars as a decimal string ("1307.375") rounded half away from zero to
// cents, for showing an unrounded credit figure. Null when not a number.
function dollarsToCents(v) {
  const m = /^(-?)(\d+)(?:\.(\d+))?$/.exec(k.str(v));
  if (!m) return null;
  const frac = (m[3] || '').padEnd(3, '0');
  let cents = BigInt(m[2]) * 100n + BigInt(frac.slice(0, 2));
  if (Number(frac[2]) >= 5) cents += 1n;
  return m[1] && cents !== 0n ? -cents : cents;
}

export function money(v) {
  const c = dollarsToCents(v);
  return c === null ? k.str(v) : k.formatCents(c);
}

