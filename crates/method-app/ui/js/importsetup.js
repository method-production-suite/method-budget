// Starting from another budget (src/import_setup.rs, src/templates.rs):
//
// - File > Import Setup From…: Rust shows the Open dialog and sends the
//   review: every setup item in the picked budget, by kind, with how it
//   stands here (new, the same, different) and what it needs. Ticking an item
//   ticks what it needs too (shown as coming along, not untickable while
//   needed). An item that differs keeps this budget's version unless the
//   user picks the imported one or a new name, as when pasting rows
//   (clipboard.js). Import is one undo step.
// - File > New From Template… and File > Save as Template…: a short dialog
//   with "Keep the detail lines", then Rust's Open or Save dialog.
//
// Everything is drawn with el() and textContent: names and definitions from
// a budget file are never read as HTML.

import { el } from './dom.js';
import { customDialog } from './modal.js';
import { t } from './i18n.js';
import { chartGroupName } from './startlist.js';

/** The review's groups, in order: [heading, kinds]. */
const GROUPS = [
  [t('library.kind.global'), ['global']],
  [t('library.kind.fringe'), ['fringe']],
  [t('library.kind.group'), ['group']],
  [t('library.kind.unit'), ['unit']],
  [t('library.kind.set'), ['set']],
  [t('library.kind.location'), ['location']],
  [t('library.kind.currency'), ['currency']],
  [t('library.kind.charge'), ['charge']],
  [t('library.kind.credit'), ['credit']],
  [t('library.kind.tag_preset'), ['tag_preset']],
  [t('library.kind.line_preset'), ['line_preset']],
  [t('library.kind.report_preset'), ['report_preset']],
  [t('library.kind.ot_pattern'), ['ot_pattern']],
  [t('library.kind.rate_table'), ['rate_table']],
  [t('importsetup.group.settings'), ['account_coding', 'fringe_posting', 'fringe_row', 'cutoff_factor', 'new_line_defaults']],
];

const KIND_LABELS = {
  global: t('importsetup.kind.global'),
  fringe: t('importsetup.kind.fringe'),
  group: t('importsetup.kind.group'),
  unit: t('importsetup.kind.unit'),
  set: t('importsetup.kind.set'),
  location: t('importsetup.kind.location'),
  currency: t('importsetup.kind.currency'),
  charge: t('importsetup.kind.charge'),
  credit: t('importsetup.kind.credit'),
  tag_preset: t('importsetup.kind.tag_preset'),
  line_preset: t('importsetup.kind.line_preset'),
  report_preset: t('importsetup.kind.report_preset'),
  ot_pattern: t('importsetup.kind.ot_pattern'),
  rate_table: t('importsetup.kind.rate_table'),
  account_coding: t('importsetup.kind.account_coding'),
  fringe_posting: t('importsetup.kind.fringe_posting'),
  fringe_row: t('importsetup.kind.fringe_row'),
  cutoff_factor: t('importsetup.kind.cutoff_factor'),
  new_line_defaults: t('importsetup.kind.new_line_defaults'),
};

const STATUS_TEXT = {
  new: t('importsetup.status_new'),
  same: t('importsetup.status_same'),
  differs: t('importsetup.status_differs'),
};

const keyOf = (kind, name) => `${kind}\u0000${name}`;

/** What the review row calls an item: its name, or a setting's label. */
export function itemLabel(it) {
  return it.name ? it.name : KIND_LABELS[it.kind] || it.kind;
}

/** A named item's kind and name in a sentence, by kind ("fringe FICA"). */
const REFS = {
  global: (name) => t('importsetup.ref.global', { name }),
  fringe: (name) => t('importsetup.ref.fringe', { name }),
  group: (name) => t('importsetup.ref.group', { name }),
  unit: (name) => t('importsetup.ref.unit', { name }),
  set: (name) => t('importsetup.ref.set', { name }),
  location: (name) => t('importsetup.ref.location', { name }),
  currency: (name) => t('importsetup.ref.currency', { name }),
  charge: (name) => t('importsetup.ref.charge', { name }),
  credit: (name) => t('importsetup.ref.credit', { name }),
  tag_preset: (name) => t('importsetup.ref.tag_preset', { name }),
  line_preset: (name) => t('importsetup.ref.line_preset', { name }),
  report_preset: (name) => t('importsetup.ref.report_preset', { name }),
  ot_pattern: (name) => t('importsetup.ref.ot_pattern', { name }),
  rate_table: (name) => t('importsetup.ref.rate_table', { name }),
};

/** An item's kind and name in a sentence ("fringe FICA", "Fringe posting"). */
function describeRef(it) {
  if (!it.name) return KIND_LABELS[it.kind] || it.kind;
  return REFS[it.kind] ? REFS[it.kind](it.name) : `${it.kind} ${it.name}`;
}

/**
 * The picks' state, without the page: `explicit` (ticked by the user) and
 * what that brings (`needed`: key -> the picked items that need it).
 */
export function selectionState(items, explicit) {
  const byKey = new Map(items.map((it) => [keyOf(it.kind, it.name), it]));
  const needed = new Map();
  for (const k of explicit) {
    const it = byKey.get(k);
    if (!it) continue;
    for (const n of it.needs || []) {
      const nk = keyOf(n.kind, n.name);
      if (nk === k) continue;
      if (!needed.has(nk)) needed.set(nk, []);
      needed.get(nk).push(it);
    }
  }
  const included = new Set([...explicit, ...needed.keys()]);
  return { needed, included };
}

/**
 * The import request from the review state: the user's picks, and a choice
 * for every included item that differs.
 */
export function importRequest(items, explicit, choices) {
  const { included } = selectionState(items, explicit);
  const picks = [];
  const resolutions = [];
  for (const it of items) {
    const k = keyOf(it.kind, it.name);
    if (explicit.has(k)) picks.push({ kind: it.kind, name: it.name });
    if (it.status === 'differs' && included.has(k)) {
      resolutions.push({ kind: it.kind, name: it.name, choice: choices.get(k) || 'keep' });
    }
  }
  return { picks, resolutions };
}

/** The review's words: Import Setup's, unless a caller (library.js) says otherwise. */
const REVIEW_TEXT = {
  title: (p) => t('importsetup.title', { source: p.source }),
  intro: t('importsetup.intro'),
  okText: t('importsetup.ok'),
  keepText: t('importsetup.keep'),
  takeText: t('importsetup.take'),
  renameText: (to) => t('importsetup.rename', { to }),
  choiceTitle: t('importsetup.choice_tip'),
  hereLabel: t('importsetup.here'),
  otherLabel: t('importsetup.other'),
  status: STATUS_TEXT,
  emptyText: t('importsetup.empty'),
  picked: (count) => t('importsetup.picked', { count }),
  pickedAlong: (count, along) => t('importsetup.picked_along', { count, along }),
  tickPrompt: t('importsetup.tick_prompt'),
  tickAll: (heading) => t('importsetup.tick_all', { heading }),
  tickOne: (what) => t('importsetup.tick_one', { what }),
};

function choiceSelect(it, text) {
  const kind = KIND_LABELS[it.kind] || it.kind;
  const label = it.name ? t('importsetup.what_to_do_named', { kind, name: it.name }) : t('importsetup.what_to_do', { kind });
  const select = el('select', { class: 'modal-input imp-choice', attrs: { 'aria-label': label } });
  const options = [
    ['keep', text.keepText],
    ['imported', text.takeText],
  ];
  if (it.rename_to) options.push(['rename', text.renameText(it.rename_to)]);
  for (const [value, label2] of options) select.append(el('option', { text: label2, attrs: { value } }));
  select.value = 'keep';
  select.title = text.choiceTitle;
  return select;
}

/**
 * File > Import Setup From…: picks a budget file (Rust's dialog), shows the
 * review and imports what is ticked. `ctx`: { invoke, apply(view), toast,
 * focus() }.
 */
export async function importSetup(ctx) {
  let preview;
  try {
    preview = await ctx.invoke('import_setup_pick');
  } catch (e) {
    ctx.toast(e);
    ctx.focus();
    return;
  }
  if (!preview) {
    ctx.focus();
    return;
  }
  showReview(ctx, preview);
}

/**
 * The review dialog: Import Setup's, or (library.js) a copy to or from the
 * library, or (packs.js) a setup pack. `opts`: words to use instead of
 * REVIEW_TEXT's, `picked` (items to tick at the start), `header` (nodes
 * shown under the intro: a pack's details), `itemNote(item)` (a line of
 * text under an item: where it came from) and `run(request)`, which does the
 * copy and resolves with { view, message } (Import Setup's runs
 * `import_setup`).
 */
export function showReview(ctx, p, opts = {}) {
  const text = { ...REVIEW_TEXT, ...opts };
  const items = p.items || [];
  const explicit = new Set();
  for (const it of opts.picked || []) {
    const k = keyOf(it.kind, it.name);
    if (items.some((x) => keyOf(x.kind, x.name) === k)) explicit.add(k);
  }
  const run = typeof opts.run === 'function' ? opts.run : (req) => ctx.invoke('import_setup', { token: p.token, picks: req.picks, resolutions: req.resolutions });
  const choices = new Map();
  const rows = new Map(); // key -> { it, box, select, sub }
  const groupBoxes = []; // [box, keys]
  let summary = null;
  let ok = null;
  let error = null;

  const refresh = () => {
    const { needed, included } = selectionState(items, explicit);
    for (const [k, r] of rows) {
      const by = needed.get(k);
      r.box.checked = included.has(k);
      // Needed by something picked: it comes along while that stays picked.
      r.box.disabled = Boolean(by) && !explicit.has(k);
      r.comes.textContent = by ? t('importsetup.comes_with', { what: by.map(describeRef).join(', ') }) : '';
      r.comes.hidden = !by;
      if (r.select) r.select.disabled = !included.has(k);
    }
    for (const [box, keys] of groupBoxes) {
      const on = keys.filter((k) => included.has(k)).length;
      box.checked = on === keys.length && keys.length > 0;
      box.indeterminate = on > 0 && on < keys.length;
    }
    const picked = included.size;
    const along = picked - [...explicit].filter((k) => rows.has(k)).length;
    summary.textContent = picked ? (along > 0 ? text.pickedAlong(picked, along) : text.picked(picked)) : text.tickPrompt;
    ok.disabled = picked === 0;
  };

  customDialog((closeDialog) => {
    const parts = [];
    parts.push(el('h2', { class: 'modal-title', text: text.title(p), attrs: { id: 'imp-title' } }));
    parts.push(el('p', { class: 'modal-message', text: text.intro }));
    for (const node of opts.header || []) if (node) parts.push(node);
    for (const note of p.notes || []) parts.push(el('p', { class: 'modal-message', text: note }));
    const scroll = el('div', { class: 'imp-scroll' });
    for (const [heading, kinds] of GROUPS) {
      const group = items.filter((it) => kinds.includes(it.kind));
      if (!group.length) continue;
      const keys = group.map((it) => keyOf(it.kind, it.name));
      const all = el('input', { attrs: { type: 'checkbox', 'aria-label': text.tickAll(heading) } });
      all.addEventListener('change', () => {
        const { needed } = selectionState(items, explicit);
        const on = keys.every((k) => explicit.has(k) || needed.has(k));
        for (const k of keys) {
          if (on) explicit.delete(k);
          else explicit.add(k);
        }
        refresh();
      });
      groupBoxes.push([all, keys]);
      scroll.append(el('label', { class: 'rs-check imp-kind' }, all, el('span', { text: t('importsetup.heading_count', { heading, count: group.length }) })));
      const list = el('ul', { class: 'safety-list imp-list' });
      for (const it of group) {
        const k = keyOf(it.kind, it.name);
        const box = el('input', { attrs: { type: 'checkbox', 'aria-label': text.tickOne(describeRef(it)) } });
        box.addEventListener('change', () => {
          if (box.checked) explicit.add(k);
          else explicit.delete(k);
          refresh();
        });
        const body = el('div', { class: 'safety-item-text' });
        const main = el('div', { class: 'safety-item-main' }, el('span', { text: itemLabel(it) }), el('span', { class: `imp-status imp-${it.status}`, text: text.status[it.status] || it.status }));
        body.append(main);
        if (it.status === 'differs') {
          body.append(el('div', { class: 'safety-item-sub', text: t('importsetup.side', { side: text.hereLabel, what: it.here || '' }), title: it.here || '' }));
          body.append(el('div', { class: 'safety-item-sub', text: t('importsetup.side', { side: text.otherLabel, what: it.imported }), title: it.imported }));
        } else if (it.imported) {
          body.append(el('div', { class: 'safety-item-sub', text: it.imported, title: it.imported }));
        }
        const note = typeof opts.itemNote === 'function' ? opts.itemNote(it) : '';
        if (note) body.append(el('div', { class: 'safety-item-sub imp-source', text: note, title: note }));
        const comes = el('div', { class: 'safety-item-sub imp-comes' });
        comes.hidden = true;
        body.append(comes);
        let select = null;
        if (it.status === 'differs') {
          select = choiceSelect(it, text);
          select.addEventListener('change', () => choices.set(k, select.value));
        }
        const row = el('li', { class: 'safety-item imp-item' }, el('label', { class: 'imp-tick' }, box), body, select ? el('div', { class: 'safety-item-actions' }, select) : null);
        rows.set(k, { it, box, select, comes });
        list.append(row);
      }
      scroll.append(list);
    }
    if (!items.length) scroll.append(el('p', { class: 'modal-message', text: text.emptyText }));
    parts.push(scroll);
    summary = el('p', { class: 'imp-summary', attrs: { 'aria-live': 'polite' } });
    parts.push(summary);
    error = el('p', { class: 'modal-error', attrs: { role: 'alert' } });
    error.hidden = true;
    parts.push(error);
    const cancel = el('button', { class: 'btn', text: t('modal.cancel'), attrs: { type: 'button', 'data-autofocus': true } });
    ok = el('button', { class: 'btn btn-primary', text: text.okText, attrs: { type: 'button' } });
    cancel.addEventListener('click', () => closeDialog());
    ok.addEventListener('click', async () => {
      const req = importRequest(items, explicit, choices);
      if (!req.picks.length) return;
      ok.disabled = true;
      try {
        const r = await run(req);
        closeDialog();
        if (r && r.view) ctx.apply(r.view);
        if (r && r.message) ctx.toast(r.message);
      } catch (e) {
        error.textContent = String(e);
        error.hidden = false;
        ok.disabled = false;
      }
    });
    parts.push(el('div', { class: 'modal-actions' }, cancel, ok));
    return el('div', { class: 'modal modal-wide imp-dialog', attrs: { role: 'dialog', 'aria-modal': 'true', 'aria-labelledby': 'imp-title' } }, ...parts);
  }, { onClose: () => ctx.focus() });
  refresh();
}

/** The groups bundled charts are listed under, in order (src/coa_import.rs). */
export const CHART_GROUPS = ['Feature', 'Television', 'Commercial', 'Other'];


/** Bundled charts grouped for the dialog: [[group, [chart...]]...], empty groups left out. */
export function groupCharts(charts) {
  const list = Array.isArray(charts) ? charts.filter((c) => c && typeof c.id === 'string' && typeof c.name === 'string') : [];
  const known = new Set(CHART_GROUPS);
  const groups = CHART_GROUPS.map((g) => [g, list.filter((c) => c.group === g)]);
  const other = groups.find(([g]) => g === 'Other');
  for (const c of list) if (!known.has(c.group)) other[1].push(c);
  return groups.filter(([, cs]) => cs.length);
}

/**
 * A short dialog before a template's Open or Save dialog: what it does, and
 * "Keep the detail lines". With `charts` (New From Template), the bundled
 * charts of accounts are listed too, by group. Resolves { keepLines } on go,
 * { chart: id } for a bundled chart, null on cancel.
 */
function templateDialog({ title, message, submitText, charts }) {
  return new Promise((resolve) => {
    let settled = false;
    const done = (v) => {
      if (settled) return;
      settled = true;
      resolve(v);
    };
    customDialog(
      (closeDialog) => {
        const keep = el('input', { attrs: { type: 'checkbox', id: 'tpl-keep-lines' } });
        const cancel = el('button', { class: 'btn', text: t('modal.cancel'), attrs: { type: 'button' } });
        const ok = el('button', { class: 'btn btn-primary', text: submitText, attrs: { type: 'button', 'data-autofocus': true } });
        cancel.addEventListener('click', () => closeDialog());
        ok.addEventListener('click', () => {
          done({ keepLines: keep.checked });
          closeDialog();
        });
        const parts = [
          el('h2', { class: 'modal-title', text: title, attrs: { id: 'tpl-title' } }),
          el('p', { class: 'modal-message', text: message }),
          el('label', { class: 'rs-check', attrs: { for: 'tpl-keep-lines' } }, keep, el('span', { text: t('importsetup.keep_lines') })),
        ];
        const groups = groupCharts(charts);
        if (groups.length) {
          const sections = groups.map(([group, cs]) =>
            el(
              'div',
              { class: 'tpl-chart-group' },
              el('h4', { class: 'tpl-chart-group-title', text: chartGroupName(group) }),
              ...cs.map((c) => {
                const b = el(
                  'button',
                  { class: 'tpl-chart', attrs: { type: 'button' }, data: { chart: c.id } },
                  el('span', { class: 'tpl-chart-name', text: c.name }),
                  el('span', { class: 'tpl-chart-desc', text: c.description || '' }),
                );
                b.addEventListener('click', () => {
                  done({ chart: c.id });
                  closeDialog();
                });
                return b;
              }),
            ),
          );
          parts.push(
            el('h3', { class: 'si-heading', text: t('importsetup.charts_heading') }),
            el('p', { class: 'modal-message', text: t('importsetup.charts_intro') }),
            el('div', { class: 'tpl-charts' }, ...sections),
          );
        }
        parts.push(el('div', { class: 'modal-actions' }, cancel, ok));
        return el('div', { class: `modal${groups.length ? ' tpl-dialog' : ''}`, attrs: { role: 'dialog', 'aria-modal': 'true', 'aria-labelledby': 'tpl-title' } }, ...parts);
      },
      { onClose: () => done(null) },
    );
  });
}

/** File > New From Template…: a new budget from a picked file or a bundled chart, in a new tab. */
export async function newFromTemplate(ctx) {
  let charts = [];
  try {
    charts = (await ctx.invoke('chart_templates')) || [];
  } catch (_) {
    charts = [];
  }
  const opts = await templateDialog({
    title: t('importsetup.new_from_template'),
    message: t('importsetup.new_from_template_intro'),
    submitText: t('importsetup.choose_template'),
    charts,
  });
  if (!opts) return ctx.focus();
  try {
    const v = opts.chart ? await ctx.invoke('chart_template_new', { id: opts.chart }) : await ctx.invoke('template_new', { keepLines: opts.keepLines });
    if (v) ctx.apply(v);
  } catch (e) {
    ctx.toast(e);
  }
  ctx.focus();
}

/** File > Save as Template…: a stripped copy of the budget, in a new file. */
export async function saveAsTemplate(ctx) {
  const opts = await templateDialog({
    title: t('importsetup.save_as_template'),
    message: t('importsetup.save_as_template_intro'),
    submitText: t('importsetup.save_template'),
  });
  if (!opts) return ctx.focus();
  try {
    const r = await ctx.invoke('template_save', { keepLines: opts.keepLines });
    if (r) ctx.toast(t('importsetup.saved_template', { name: r.name }));
  } catch (e) {
    ctx.toast(e);
  }
  ctx.focus();
}
