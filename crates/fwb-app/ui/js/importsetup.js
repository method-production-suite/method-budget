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

/** The review's groups, in order: [heading, kinds]. */
const GROUPS = [
  ['Globals', ['global']],
  ['Fringes', ['fringe']],
  ['Groups', ['group']],
  ['Units', ['unit']],
  ['Sets', ['set']],
  ['Locations', ['location']],
  ['Currencies', ['currency']],
  ['Charges', ['charge']],
  ['Credits', ['credit']],
  ['Tag presets', ['tag_preset']],
  ['Line blocks', ['line_preset']],
  ['Report presets', ['report_preset']],
  ['Budget settings', ['account_coding', 'fringe_posting', 'new_line_defaults']],
];

const KIND_LABELS = {
  global: 'Global',
  fringe: 'Fringe',
  group: 'Group',
  unit: 'Unit',
  set: 'Set',
  location: 'Location',
  currency: 'Currency',
  charge: 'Charge',
  credit: 'Credit',
  tag_preset: 'Tag preset',
  line_preset: 'Line block',
  report_preset: 'Report preset',
  account_coding: 'Account numbering',
  fringe_posting: 'Fringe posting',
  new_line_defaults: 'New line defaults',
};

const STATUS_TEXT = {
  new: 'New',
  same: 'Same here',
  differs: 'Different here',
};

const keyOf = (kind, name) => `${kind}\u0000${name}`;

/** What the review row calls an item: its name, or a setting's label. */
export function itemLabel(it) {
  return it.name ? it.name : KIND_LABELS[it.kind] || it.kind;
}

/** An item's kind and name in a sentence ("fringe FICA", "Fringe posting"). */
function describeRef(it) {
  return it.name ? `${(KIND_LABELS[it.kind] || it.kind).toLowerCase()} ${it.name}` : KIND_LABELS[it.kind] || it.kind;
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

function choiceSelect(it) {
  const label = `${KIND_LABELS[it.kind] || it.kind} ${it.name}`.trim();
  const select = el('select', { class: 'modal-input imp-choice', attrs: { 'aria-label': `What to do with ${label}` } });
  const options = [
    ['keep', "Keep this budget's version"],
    ['imported', 'Use the imported version'],
  ];
  if (it.rename_to) options.push(['rename', `Add the imported one as ${it.rename_to}`]);
  for (const [value, text] of options) select.append(el('option', { text, attrs: { value } }));
  select.value = 'keep';
  select.title = "The imported version replaces this budget's for everything here that uses it; a new name leaves this budget's as it is.";
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

function showReview(ctx, p) {
  const items = p.items || [];
  const explicit = new Set();
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
      r.comes.textContent = by ? `Comes with ${by.map(describeRef).join(', ')}` : '';
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
    summary.textContent = picked
      ? `${picked} ${picked === 1 ? 'item' : 'items'} to import${along > 0 ? `, ${along} of them needed by the others` : ''}.`
      : 'Tick the items to import.';
    ok.disabled = picked === 0;
  };

  customDialog((closeDialog) => {
    const parts = [];
    parts.push(el('h2', { class: 'modal-title', text: `Import setup from ${p.source}`, attrs: { id: 'imp-title' } }));
    parts.push(
      el('p', {
        class: 'modal-message',
        text: "Tick what to bring into this budget. What an item needs comes with it. Where this budget has an item of the same name set up differently, it keeps its own version unless you choose otherwise.",
      }),
    );
    for (const note of p.notes || []) parts.push(el('p', { class: 'modal-message', text: note }));
    const scroll = el('div', { class: 'imp-scroll' });
    for (const [heading, kinds] of GROUPS) {
      const group = items.filter((it) => kinds.includes(it.kind));
      if (!group.length) continue;
      const keys = group.map((it) => keyOf(it.kind, it.name));
      const all = el('input', { attrs: { type: 'checkbox', 'aria-label': `Import every item in ${heading}` } });
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
      scroll.append(el('label', { class: 'rs-check imp-kind' }, all, el('span', { text: `${heading} (${group.length})` })));
      const list = el('ul', { class: 'safety-list imp-list' });
      for (const it of group) {
        const k = keyOf(it.kind, it.name);
        const box = el('input', { attrs: { type: 'checkbox', 'aria-label': `Import ${describeRef(it)}` } });
        box.addEventListener('change', () => {
          if (box.checked) explicit.add(k);
          else explicit.delete(k);
          refresh();
        });
        const text = el('div', { class: 'safety-item-text' });
        const main = el('div', { class: 'safety-item-main' }, el('span', { text: itemLabel(it) }), el('span', { class: `imp-status imp-${it.status}`, text: STATUS_TEXT[it.status] || it.status }));
        text.append(main);
        if (it.status === 'differs') {
          text.append(el('div', { class: 'safety-item-sub', text: `Here: ${it.here || ''}`, title: it.here || '' }));
          text.append(el('div', { class: 'safety-item-sub', text: `Imported: ${it.imported}`, title: it.imported }));
        } else if (it.imported) {
          text.append(el('div', { class: 'safety-item-sub', text: it.imported, title: it.imported }));
        }
        const comes = el('div', { class: 'safety-item-sub imp-comes' });
        comes.hidden = true;
        text.append(comes);
        let select = null;
        if (it.status === 'differs') {
          select = choiceSelect(it);
          select.addEventListener('change', () => choices.set(k, select.value));
        }
        const row = el('li', { class: 'safety-item imp-item' }, el('label', { class: 'imp-tick' }, box), text, select ? el('div', { class: 'safety-item-actions' }, select) : null);
        rows.set(k, { it, box, select, comes });
        list.append(row);
      }
      scroll.append(list);
    }
    if (!items.length) scroll.append(el('p', { class: 'modal-message', text: 'That budget has no setup items.' }));
    parts.push(scroll);
    summary = el('p', { class: 'imp-summary', attrs: { 'aria-live': 'polite' } });
    parts.push(summary);
    error = el('p', { class: 'modal-error', attrs: { role: 'alert' } });
    error.hidden = true;
    parts.push(error);
    const cancel = el('button', { class: 'btn', text: 'Cancel', attrs: { type: 'button', 'data-autofocus': true } });
    ok = el('button', { class: 'btn btn-primary', text: 'Import', attrs: { type: 'button' } });
    cancel.addEventListener('click', () => closeDialog());
    ok.addEventListener('click', async () => {
      const req = importRequest(items, explicit, choices);
      if (!req.picks.length) return;
      ok.disabled = true;
      try {
        const r = await ctx.invoke('import_setup', { token: p.token, picks: req.picks, resolutions: req.resolutions });
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

/**
 * A short dialog before a template's Open or Save dialog: what it does, and
 * "Keep the detail lines". Resolves { keepLines } on go, null on cancel.
 */
function templateDialog({ title, message, submitText }) {
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
        const cancel = el('button', { class: 'btn', text: 'Cancel', attrs: { type: 'button' } });
        const ok = el('button', { class: 'btn btn-primary', text: submitText, attrs: { type: 'button', 'data-autofocus': true } });
        cancel.addEventListener('click', () => closeDialog());
        ok.addEventListener('click', () => {
          done({ keepLines: keep.checked });
          closeDialog();
        });
        return el(
          'div',
          { class: 'modal', attrs: { role: 'dialog', 'aria-modal': 'true', 'aria-labelledby': 'tpl-title' } },
          el('h2', { class: 'modal-title', text: title, attrs: { id: 'tpl-title' } }),
          el('p', { class: 'modal-message', text: message }),
          el('label', { class: 'rs-check', attrs: { for: 'tpl-keep-lines' } }, keep, el('span', { text: 'Keep the detail lines (without their notes)' })),
          el('div', { class: 'modal-actions' }, cancel, ok),
        );
      },
      { onClose: () => done(null) },
    );
  });
}

/** File > New From Template…: a new budget from a picked file, in a new tab. */
export async function newFromTemplate(ctx) {
  const opts = await templateDialog({
    title: 'New From Template',
    message:
      'Starts a new budget, in a new tab, from another budget file: its categories, accounts, setup and settings. Its info, baseline, versions and notes are left out. Any budget file can be a template.',
    submitText: 'Choose Template…',
  });
  if (!opts) return ctx.focus();
  try {
    const v = await ctx.invoke('template_new', { keepLines: opts.keepLines });
    if (v) ctx.apply(v);
  } catch (e) {
    ctx.toast(e);
  }
  ctx.focus();
}

/** File > Save as Template…: a stripped copy of the budget, in a new file. */
export async function saveAsTemplate(ctx) {
  const opts = await templateDialog({
    title: 'Save as Template',
    message:
      'Saves a copy of this budget to start others from: its categories, accounts, setup and settings, without its info, baseline, versions or notes. This budget and its file are not changed.',
    submitText: 'Save Template…',
  });
  if (!opts) return ctx.focus();
  try {
    const r = await ctx.invoke('template_save', { keepLines: opts.keepLines });
    if (r) ctx.toast(`Saved the template ${r.name}.`);
  } catch (e) {
    ctx.toast(e);
  }
  ctx.focus();
}
