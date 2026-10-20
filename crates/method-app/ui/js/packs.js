// Setup packs (src/packs.rs; decisions.md, entry 50):
//
// - Export Setup Pack…: from the budget in front or the library. The items
//   are listed by kind and folder; ticking one ticks what it needs (as
//   Import Setup's review does, importsetup.js), a folder's box ticks the
//   folder. The pack's details (title, version, effective date, region,
//   who made it, description, source notes and links) go with it; Rust
//   checks them, walks the dependencies again and shows the Save dialog.
// - Import Setup Pack…: Rust's Open dialog (or a pack from the Packs list),
//   then Import Setup's review with the pack's details on top and each
//   item's source under it, into this budget (one undo step) or the
//   library. What comes in is stamped with the pack's title, version and
//   effective date.
// - The Packs section of the Library tab: the packs in your Packs folder
//   and the ones that come with the app.
//
// Setup lists in spreadsheets are setuplists.js (the main window's File
// menu only).
//
// Text from files only reaches the page through textContent (dom.js el()).
// `ctx` everywhere: { call(cmd, args), toast(message), focus(), apply(view)?,
// refresh()? }.

import { el } from './dom.js';
import { customDialog } from './modal.js';
import { selectionState, showReview } from './importsetup.js';
import { localStamp } from './versions.js';

/** Kinds as a pack lists them: [kind, heading]. */
export const PACK_KINDS = [
  ['global', 'Globals'],
  ['fringe', 'Fringes'],
  ['group', 'Groups'],
  ['unit', 'Units'],
  ['set', 'Sets'],
  ['location', 'Locations'],
  ['currency', 'Currencies'],
  ['charge', 'Charges'],
  ['credit', 'Credits'],
  ['tag_preset', 'Tag presets'],
  ['line_preset', 'Line blocks'],
  ['report_preset', 'Report presets'],
  ['ot_pattern', 'Overtime patterns'],
  ['rate_table', 'Rate tables'],
];

const keyOf = (kind, name) => `${kind}\u0000${name}`;

function str(v) {
  return typeof v === 'string' ? v : '';
}

let uid = 0;
function nextId(prefix) {
  uid += 1;
  return `${prefix}-${uid}`;
}

function field(label, input) {
  return el('label', { class: 'modal-field pk-field' }, el('span', { text: label }), input);
}

function input(value = '', attrs = {}) {
  const i = el('input', { class: 'modal-input', attrs: { type: 'text', autocomplete: 'off', spellcheck: 'false', ...attrs } });
  i.value = value;
  return i;
}

function textarea(value = '', rows = 2) {
  const t = el('textarea', { class: 'modal-input pk-text', attrs: { rows: String(rows), spellcheck: 'true' } });
  t.value = value;
  return t;
}

/** The export's items, flat: { kind, name, folder, definition, needs }. */
export function flatItems(packItems) {
  const out = [];
  for (const k of (packItems && packItems.kinds) || []) {
    for (const it of k.items || []) out.push({ kind: k.kind, name: it.name, folder: it.folder || null, definition: str(it.definition), needs: it.needs || [] });
  }
  return out;
}

/** The pack details the form holds, as Rust takes them. */
export function detailsFrom(form) {
  return {
    title: form.title.value,
    description: form.description.value,
    author: form.author.value,
    version: form.version.value,
    effective: form.effective.value,
    region: form.region.value,
    notes: form.notes.value,
    urls: form.urls.value
      .split(/\s+/)
      .map((u) => u.trim())
      .filter(Boolean),
  };
}

// ---------------------------------------------------------------------------
// Export Setup Pack…

/**
 * Export Setup Pack… from `source` ('budget': the budget in front, or
 * 'library'). `picked`: [{ kind, name }] to tick at the start.
 */
export async function exportPack(ctx, source = 'budget', picked = []) {
  let data;
  try {
    data = await ctx.call('pack_items', { source });
  } catch (e) {
    ctx.toast(e);
    return;
  }
  const items = flatItems(data);
  const explicit = new Set();
  for (const p of picked) if (items.some((it) => it.kind === p.kind && it.name === p.name)) explicit.add(keyOf(p.kind, p.name));
  const rows = new Map(); // key -> { box, comes }
  const groupBoxes = []; // [box, keys]
  let summary = null;
  let ok = null;
  let error = null;

  const refresh = () => {
    const { needed, included } = selectionState(items, explicit);
    for (const [k, r] of rows) {
      const by = needed.get(k);
      r.box.checked = included.has(k);
      r.box.disabled = Boolean(by) && !explicit.has(k);
      r.comes.textContent = by ? `Comes with ${by.map((x) => x.name).join(', ')}` : '';
      r.comes.hidden = !by;
    }
    for (const [box, keys] of groupBoxes) {
      const on = keys.filter((k) => included.has(k)).length;
      box.checked = on === keys.length && keys.length > 0;
      box.indeterminate = on > 0 && on < keys.length;
    }
    const n = included.size;
    summary.textContent = n ? `${n} ${n === 1 ? 'item' : 'items'} in the pack.` : 'Tick the items for the pack.';
    ok.disabled = n === 0;
  };

  const tickAll = (keys) => {
    const { needed } = selectionState(items, explicit);
    const on = keys.every((k) => explicit.has(k) || needed.has(k));
    for (const k of keys) {
      if (on) explicit.delete(k);
      else explicit.add(k);
    }
    refresh();
  };

  const groupBox = (label, keys, cls) => {
    const box = el('input', { attrs: { type: 'checkbox', 'aria-label': `Put every item in ${label} in the pack` } });
    box.addEventListener('change', () => tickAll(keys));
    groupBoxes.push([box, keys]);
    return el('label', { class: `rs-check ${cls}` }, box, el('span', { text: label }));
  };

  const itemRow = (it) => {
    const k = keyOf(it.kind, it.name);
    const box = el('input', { attrs: { type: 'checkbox', 'aria-label': `Put ${it.name} in the pack` } });
    box.addEventListener('change', () => {
      if (box.checked) explicit.add(k);
      else explicit.delete(k);
      refresh();
    });
    const comes = el('div', { class: 'safety-item-sub imp-comes' });
    comes.hidden = true;
    const body = el(
      'div',
      { class: 'safety-item-text' },
      el('div', { class: 'safety-item-main' }, el('span', { text: it.name })),
      it.definition ? el('div', { class: 'safety-item-sub', text: it.definition, title: it.definition }) : null,
      comes,
    );
    rows.set(k, { box, comes });
    return el('li', { class: 'safety-item imp-item' }, el('label', { class: 'imp-tick' }, box), body);
  };

  const form = {
    title: input(str(data.title), { 'aria-label': 'Pack title' }),
    version: input('', { placeholder: '2026.1' }),
    effective: input('', { type: 'date' }),
    region: input('', { placeholder: 'Region or jurisdiction' }),
    author: input('', { placeholder: 'Who made it' }),
    description: textarea('', 2),
    notes: textarea('', 2),
    urls: textarea('', 2),
  };

  customDialog(
    (closeDialog) => {
      const titleId = nextId('pk-title');
      const parts = [
        el('h2', { class: 'modal-title', text: source === 'library' ? 'Export a setup pack from your library' : `Export a setup pack from ${str(data.source)}`, attrs: { id: titleId } }),
        el('p', {
          class: 'modal-message',
          text: 'A setup pack is a budget file with setup items and no lines, to share or keep. Tick what goes in it; what an item needs comes with it. The details say what the pack is and where its figures come from.',
        }),
      ];
      const scroll = el('div', { class: 'imp-scroll pk-scroll' });
      for (const [kind, heading] of PACK_KINDS) {
        const group = items.filter((it) => it.kind === kind);
        if (!group.length) continue;
        scroll.append(groupBox(`${heading} (${group.length})`, group.map((it) => keyOf(it.kind, it.name)), 'imp-kind'));
        const folders = [...new Set(group.map((it) => it.folder).filter(Boolean))].sort();
        const unfiled = group.filter((it) => !it.folder);
        if (unfiled.length) scroll.append(el('ul', { class: 'safety-list imp-list' }, ...unfiled.map(itemRow)));
        for (const f of folders) {
          const inFolder = group.filter((it) => it.folder === f);
          scroll.append(groupBox(`${f} (${inFolder.length})`, inFolder.map((it) => keyOf(it.kind, it.name)), 'pk-folder'));
          scroll.append(el('ul', { class: 'safety-list imp-list pk-in-folder' }, ...inFolder.map(itemRow)));
        }
      }
      if (!items.length) scroll.append(el('p', { class: 'modal-message', text: 'There are no setup items to put in a pack.' }));
      parts.push(scroll);
      parts.push(
        el('h3', { class: 'si-heading', text: 'Details' }),
        el(
          'div',
          { class: 'pk-form' },
          field('Title', form.title),
          field('Version', form.version),
          field('Effective date', form.effective),
          field('Region', form.region),
          field('Made by', form.author),
          el('div', { class: 'pk-wide' }, field('Description', form.description)),
          el('div', { class: 'pk-wide' }, field('Where the figures come from', form.notes)),
          el('div', { class: 'pk-wide' }, field('Source links (one per line)', form.urls)),
        ),
      );
      summary = el('p', { class: 'imp-summary', attrs: { 'aria-live': 'polite' } });
      error = el('p', { class: 'modal-error', attrs: { role: 'alert' } });
      error.hidden = true;
      const cancel = el('button', { class: 'btn', text: 'Cancel', attrs: { type: 'button', 'data-autofocus': true } });
      ok = el('button', { class: 'btn btn-primary', text: 'Export Pack…', attrs: { type: 'button' } });
      cancel.addEventListener('click', () => closeDialog());
      ok.addEventListener('click', async () => {
        const picks = items.filter((it) => explicit.has(keyOf(it.kind, it.name))).map((it) => ({ kind: it.kind, name: it.name }));
        if (!picks.length) return;
        ok.disabled = true;
        error.hidden = true;
        try {
          const r = await ctx.call('pack_export', { source, picks, details: detailsFrom(form), created: localStamp() });
          if (r) {
            closeDialog();
            ctx.toast(r.message);
            if (typeof ctx.refresh === 'function') ctx.refresh();
          } else {
            ok.disabled = false;
          }
        } catch (e) {
          error.textContent = String(e);
          error.hidden = false;
          ok.disabled = false;
        }
      });
      parts.push(summary, error, el('div', { class: 'modal-actions' }, cancel, ok));
      return el('div', { class: 'modal modal-wide imp-dialog pk-dialog', attrs: { role: 'dialog', 'aria-modal': 'true', 'aria-labelledby': titleId } }, ...parts);
    },
    { onClose: () => ctx.focus() },
  );
  refresh();
}

// ---------------------------------------------------------------------------
// Import Setup Pack…

/** A pack's details as lines of text ("Effective: 2026-01-01"). */
export function detailLines(pack) {
  const d = pack && pack.details;
  if (!d) return [];
  const out = [];
  for (const [label, v] of [
    ['Version', d.version],
    ['Effective', d.effective],
    ['Region', d.region],
    ['Made by', d.author],
    ['Made', str(pack.created).slice(0, 10)],
    ['From the budget', pack.source_title],
  ]) {
    if (str(v).trim()) out.push(`${label}: ${str(v).trim()}`);
  }
  return out;
}

/** The review's header: the pack's details and sources, as nodes. */
function packHeader(pack) {
  if (!pack || !pack.is_pack || !pack.details) {
    return [el('p', { class: 'modal-message', text: 'This file is not a setup pack; its setup items can still be imported.' })];
  }
  const d = pack.details;
  const box = el('div', { class: 'pk-about' });
  box.append(el('div', { class: 'pk-about-title', text: d.title }));
  const lines = detailLines(pack);
  if (lines.length) box.append(el('div', { class: 'pk-about-line', text: lines.join(' · ') }));
  if (str(d.description).trim()) box.append(el('div', { class: 'pk-about-line', text: d.description }));
  if (str(d.notes).trim()) box.append(el('div', { class: 'pk-about-line', text: `Sources: ${d.notes}` }));
  // Links are text: the app never opens them.
  for (const u of d.urls || []) box.append(el('div', { class: 'pk-about-link', text: u, title: u }));
  return [box];
}

/** What a pack review says, by where it goes. */
function reviewText(r) {
  const title = r.pack && r.pack.details ? r.pack.details.title : r.source;
  if (r.into === 'library') {
    return {
      title: () => `Import ${title} into your library`,
      intro: "Tick what to copy into your library. What an item needs comes with it. Where the library has an item of the same name set up differently, it keeps its own version unless you choose otherwise. What comes in is marked with the pack's title, version and effective date.",
      okText: 'Import into Library',
      keepText: "Keep the library's version",
      takeText: "Use the pack's version",
      renameText: (to) => `Add the pack's as ${to}`,
      choiceTitle: "The pack's version replaces the library's; a new name leaves the library's as it is.",
      hereLabel: 'Library',
      otherLabel: 'Pack',
      status: { new: 'New to the library', same: 'Same in the library', differs: 'Different in the library' },
      emptyText: 'The pack has no setup items.',
      doing: 'to import',
      tickAll: (heading) => `Import every item in ${heading}`,
      tickOne: (what) => `Import ${what}`,
    };
  }
  return {
    title: () => `Import ${title}`,
    intro: "Tick what to bring into this budget. What an item needs comes with it. Where this budget has an item of the same name set up differently, it keeps its own version unless you choose otherwise. What comes in is marked with the pack's title, version and effective date.",
    okText: 'Import',
    keepText: "Keep this budget's version",
    takeText: "Use the pack's version",
    renameText: (to) => `Add the pack's as ${to}`,
    choiceTitle: "The pack's version replaces this budget's for everything here that uses it; a new name leaves this budget's as it is.",
    hereLabel: 'Here',
    otherLabel: 'Pack',
    emptyText: 'The pack has no setup items.',
    doing: 'to import',
    tickAll: (heading) => `Import every item in ${heading}`,
    tickOne: (what) => `Import ${what}`,
  };
}

/** Shows the review `r` (a PackReview) and imports what is ticked. */
export function showPackReview(ctx, r) {
  const sources = new Map(((r.pack && r.pack.sources) || []).map((s) => [keyOf(s.kind, s.name), s.text]));
  const reviewCtx = { invoke: (cmd, args) => ctx.call(cmd, args), apply: (v) => ctx.apply && ctx.apply(v), toast: ctx.toast, focus: ctx.focus };
  showReview(reviewCtx, r, {
    ...reviewText(r),
    header: packHeader(r.pack),
    itemNote: (it) => sources.get(keyOf(it.kind, it.name)) || '',
    run: async (req) => {
      if (r.into === 'library') {
        const done = await ctx.call('pack_import_library', { token: r.token, sourceToken: r.source_token, picks: req.picks, resolutions: req.resolutions });
        if (typeof ctx.libraryChanged === 'function' && done && done.library) ctx.libraryChanged(done.library);
        return { message: done && done.message };
      }
      const done = await ctx.call('pack_import', { token: r.token, picks: req.picks, resolutions: req.resolutions });
      return { view: done && done.view, message: done && done.message };
    },
  });
}

/** Import Setup Pack…: Rust's Open dialog, then the review (`into`: 'budget' or 'library'). */
export async function importPack(ctx, into = 'budget') {
  let r;
  try {
    r = await ctx.call('pack_import_pick', { into });
  } catch (e) {
    ctx.toast(e);
    ctx.focus();
    return;
  }
  if (!r) {
    ctx.focus();
    return;
  }
  showPackReview(ctx, r);
}

/** The review of listed pack `token`, into the budget or the library. */
export async function reviewListed(ctx, token, into) {
  let r;
  try {
    r = await ctx.call('pack_review', { token, into });
  } catch (e) {
    ctx.toast(e);
    return;
  }
  showPackReview(ctx, r);
}

// ---------------------------------------------------------------------------
// The Packs section (library.js draws it at the foot of the Library tab)

const packsState = { view: null, loading: false, error: '' };

/** Loads the Packs list; `redraw` after. */
export async function loadPacks(call, redraw) {
  if (packsState.loading) return;
  packsState.loading = true;
  try {
    packsState.view = await call('packs_list', {});
    packsState.error = '';
  } catch (e) {
    packsState.error = String(e);
  } finally {
    packsState.loading = false;
    redraw();
  }
}

function smallButton(text, key, onClick, { title = '', enabled = true } = {}) {
  const b = el('button', { class: 'setup-btn apply-btn', text, title: title || undefined, attrs: { type: 'button' }, data: { key, nobusy: '1' } });
  if (!enabled) b.setAttribute('aria-disabled', 'true');
  b.addEventListener('click', () => {
    if (b.getAttribute('aria-disabled') === 'true') return;
    onClick(b);
  });
  return b;
}

/**
 * The Packs section: export and import buttons, then each listed pack with
 * Add to This Budget… and Add to Library…. `opts`: { libraryWritable,
 * hasLibrary }. `ctx` as at the top, with `redraw()`.
 */
export function packsSection(ctx, opts = {}) {
  if (packsState.view === null && !packsState.loading && !packsState.error) loadPacks(ctx.call, ctx.redraw);
  const refresh = () => loadPacks(ctx.call, ctx.redraw);
  const withRefresh = { ...ctx, refresh };
  const parts = [el('h3', { class: 'pk-section-title', text: 'Setup packs' })];
  parts.push(
    el(
      'div',
      { class: 'library-actions' },
      smallButton('Export Pack…', 'packs:export', () => exportPack(withRefresh, 'budget'), { title: "Make a setup pack of this budget's setup items" }),
      smallButton('Import Pack…', 'packs:import', () => importPack(withRefresh, 'budget'), { title: 'Bring setup items from a pack file into this budget' }),
      opts.hasLibrary
        ? smallButton('Export Library…', 'packs:export-library', () => exportPack(withRefresh, 'library'), { title: 'Make a setup pack of items from your library' })
        : null,
      opts.hasLibrary
        ? smallButton('Import into Library…', 'packs:import-library', () => importPack(withRefresh, 'library'), {
            enabled: opts.libraryWritable,
            title: opts.libraryWritable ? 'Bring setup items from a pack file into your library' : 'The library is open as a tab: change it there',
          })
        : null,
    ),
  );
  const v = packsState.view;
  if (packsState.error) parts.push(el('p', { class: 'setup-error apply-error', text: packsState.error, attrs: { role: 'alert' } }));
  else if (!v) parts.push(el('p', { class: 'setup-empty', text: 'Looking for packs…' }));
  else {
    const list = el('ul', { class: 'apply-items pk-list', attrs: { 'aria-label': 'Setup packs' } });
    for (const p of v.packs || []) {
      const title = str(p.title) || str(p.file);
      const meta = [p.version ? `v${p.version}` : '', p.effective ? `effective ${p.effective}` : '', p.region, p.place === 'bundled' ? 'comes with the app' : '', `${p.items} ${p.items === 1 ? 'item' : 'items'}`].filter(Boolean).join(' · ');
      const head = el('div', { class: 'pk-item-text' }, el('span', { class: 'apply-name', text: title, title: `${str(p.file)}.mthd` }), el('span', { class: 'apply-desc', text: p.problem && !p.items ? p.problem : meta }));
      const actions = el(
        'div',
        { class: 'library-actions' },
        smallButton('Add to This Budget…', `packs:add:${p.token}`, () => reviewListed(withRefresh, p.token, 'budget'), { enabled: p.items > 0 }),
        opts.hasLibrary ? smallButton('Add to Library…', `packs:lib:${p.token}`, () => reviewListed(withRefresh, p.token, 'library'), { enabled: p.items > 0 && opts.libraryWritable }) : null,
      );
      list.append(el('li', { class: 'pk-item' }, head, actions));
    }
    if (!(v.packs || []).length) {
      parts.push(el('p', { class: 'setup-empty', text: 'No packs yet. Packs you export to your Packs folder, or drop into it, are listed here.' }));
    }
    parts.push(list);
    parts.push(
      el(
        'div',
        { class: 'library-actions' },
        smallButton('Show Packs Folder', 'packs:reveal', async () => {
          try {
            await ctx.call('packs_reveal', {});
          } catch (e) {
            ctx.toast(e);
          }
        }, { title: str(v.folder) }),
        smallButton('Refresh', 'packs:refresh', refresh, { title: 'Look for packs again' }),
      ),
    );
  }
  return el('section', { class: 'pk-section', attrs: { 'aria-label': 'Setup packs' } }, ...parts);
}
