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
import { t } from './i18n.js';

/** Kinds as a pack lists them: [kind, heading]. */
export const PACK_KINDS = [
  ['global', t('library.kind.global')],
  ['fringe', t('library.kind.fringe')],
  ['group', t('library.kind.group')],
  ['unit', t('library.kind.unit')],
  ['set', t('library.kind.set')],
  ['location', t('library.kind.location')],
  ['currency', t('library.kind.currency')],
  ['charge', t('library.kind.charge')],
  ['credit', t('library.kind.credit')],
  ['tag_preset', t('library.kind.tag_preset')],
  ['line_preset', t('library.kind.line_preset')],
  ['report_preset', t('library.kind.report_preset')],
  ['ot_pattern', t('library.kind.ot_pattern')],
  ['rate_table', t('library.kind.rate_table')],
];

const keyOf = (kind, name) => `${kind}\u0000${name}`;

/** A budget file's extension (a pack is a budget file). */
const BUDGET_EXT = '.mthd';

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
  const area = el('textarea', { class: 'modal-input pk-text', attrs: { rows: String(rows), spellcheck: 'true' } });
  area.value = value;
  return area;
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
      r.comes.textContent = by ? t('packs.comes_with', { names: by.map((x) => x.name).join(', ') }) : '';
      r.comes.hidden = !by;
    }
    for (const [box, keys] of groupBoxes) {
      const on = keys.filter((k) => included.has(k)).length;
      box.checked = on === keys.length && keys.length > 0;
      box.indeterminate = on > 0 && on < keys.length;
    }
    const n = included.size;
    summary.textContent = n ? t('packs.in_pack', { count: n }) : t('packs.tick_prompt');
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
    const box = el('input', { attrs: { type: 'checkbox', 'aria-label': t('packs.tick_all', { label }) } });
    box.addEventListener('change', () => tickAll(keys));
    groupBoxes.push([box, keys]);
    return el('label', { class: `rs-check ${cls}` }, box, el('span', { text: label }));
  };

  const itemRow = (it) => {
    const k = keyOf(it.kind, it.name);
    const box = el('input', { attrs: { type: 'checkbox', 'aria-label': t('packs.tick_one', { name: it.name }) } });
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
    title: input(str(data.title), { 'aria-label': t('packs.pack_title') }),
    version: input('', { placeholder: '2026.1' }),
    effective: input('', { type: 'date' }),
    region: input('', { placeholder: t('packs.region_hint') }),
    author: input('', { placeholder: t('packs.author_hint') }),
    description: textarea('', 2),
    notes: textarea('', 2),
    urls: textarea('', 2),
  };

  customDialog(
    (closeDialog) => {
      const titleId = nextId('pk-title');
      const parts = [
        el('h2', { class: 'modal-title', text: source === 'library' ? t('packs.export_title_library') : t('packs.export_title', { source: str(data.source) }), attrs: { id: titleId } }),
        el('p', {
          class: 'modal-message',
          text: t('packs.export_intro'),
        }),
      ];
      const scroll = el('div', { class: 'imp-scroll pk-scroll' });
      for (const [kind, heading] of PACK_KINDS) {
        const group = items.filter((it) => it.kind === kind);
        if (!group.length) continue;
        scroll.append(groupBox(t('packs.heading_count', { heading, count: group.length }), group.map((it) => keyOf(it.kind, it.name)), 'imp-kind'));
        const folders = [...new Set(group.map((it) => it.folder).filter(Boolean))].sort();
        const unfiled = group.filter((it) => !it.folder);
        if (unfiled.length) scroll.append(el('ul', { class: 'safety-list imp-list' }, ...unfiled.map(itemRow)));
        for (const f of folders) {
          const inFolder = group.filter((it) => it.folder === f);
          scroll.append(groupBox(t('packs.heading_count', { heading: f, count: inFolder.length }), inFolder.map((it) => keyOf(it.kind, it.name)), 'pk-folder'));
          scroll.append(el('ul', { class: 'safety-list imp-list pk-in-folder' }, ...inFolder.map(itemRow)));
        }
      }
      if (!items.length) scroll.append(el('p', { class: 'modal-message', text: t('packs.export_empty') }));
      parts.push(scroll);
      parts.push(
        el('h3', { class: 'si-heading', text: t('packs.details') }),
        el(
          'div',
          { class: 'pk-form' },
          field(t('packs.field.title'), form.title),
          field(t('packs.field.version'), form.version),
          field(t('packs.field.effective'), form.effective),
          field(t('packs.field.region'), form.region),
          field(t('packs.field.author'), form.author),
          el('div', { class: 'pk-wide' }, field(t('packs.field.description'), form.description)),
          el('div', { class: 'pk-wide' }, field(t('packs.field.notes'), form.notes)),
          el('div', { class: 'pk-wide' }, field(t('packs.field.urls'), form.urls)),
        ),
      );
      summary = el('p', { class: 'imp-summary', attrs: { 'aria-live': 'polite' } });
      error = el('p', { class: 'modal-error', attrs: { role: 'alert' } });
      error.hidden = true;
      const cancel = el('button', { class: 'btn', text: t('modal.cancel'), attrs: { type: 'button', 'data-autofocus': true } });
      ok = el('button', { class: 'btn btn-primary', text: t('packs.export_ok'), attrs: { type: 'button' } });
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
  for (const [line, v] of [
    [(value) => t('packs.detail.version', { value }), d.version],
    [(value) => t('packs.detail.effective', { value }), d.effective],
    [(value) => t('packs.detail.region', { value }), d.region],
    [(value) => t('packs.detail.author', { value }), d.author],
    [(value) => t('packs.detail.created', { value }), str(pack.created).slice(0, 10)],
    [(value) => t('packs.detail.source', { value }), pack.source_title],
  ]) {
    if (str(v).trim()) out.push(line(str(v).trim()));
  }
  return out;
}

/** The review's header: the pack's details and sources, as nodes. */
function packHeader(pack) {
  if (!pack || !pack.is_pack || !pack.details) {
    return [el('p', { class: 'modal-message', text: t('packs.not_a_pack') })];
  }
  const d = pack.details;
  const box = el('div', { class: 'pk-about' });
  box.append(el('div', { class: 'pk-about-title', text: d.title }));
  const lines = detailLines(pack);
  if (lines.length) box.append(el('div', { class: 'pk-about-line', text: lines.join(' · ') }));
  if (str(d.description).trim()) box.append(el('div', { class: 'pk-about-line', text: d.description }));
  if (str(d.notes).trim()) box.append(el('div', { class: 'pk-about-line', text: t('packs.sources', { notes: d.notes }) }));
  // Links are text: the app never opens them.
  for (const u of d.urls || []) box.append(el('div', { class: 'pk-about-link', text: u, title: u }));
  return [box];
}

/** What a pack review says, by where it goes. */
function reviewText(r) {
  const title = r.pack && r.pack.details ? r.pack.details.title : r.source;
  if (r.into === 'library') {
    return {
      title: () => t('packs.lib.title', { title }),
      intro: t('packs.lib.intro'),
      okText: t('packs.lib.ok'),
      keepText: t('packs.lib.keep'),
      takeText: t('packs.take'),
      renameText: (to) => t('packs.rename', { to }),
      choiceTitle: t('packs.lib.choice_tip'),
      hereLabel: t('packs.lib.here'),
      otherLabel: t('packs.other'),
      status: { new: t('packs.lib.status_new'), same: t('packs.lib.status_same'), differs: t('packs.lib.status_differs') },
      emptyText: t('packs.review_empty'),
    };
  }
  return {
    title: () => t('packs.title', { title }),
    intro: t('packs.intro'),
    okText: t('importsetup.ok'),
    keepText: t('importsetup.keep'),
    takeText: t('packs.take'),
    renameText: (to) => t('packs.rename', { to }),
    choiceTitle: t('packs.choice_tip'),
    hereLabel: t('importsetup.here'),
    otherLabel: t('packs.other'),
    emptyText: t('packs.review_empty'),
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
  const parts = [el('h3', { class: 'pk-section-title', text: t('packs.section_title') })];
  parts.push(
    el(
      'div',
      { class: 'library-actions' },
      smallButton(t('packs.export_btn'), 'packs:export', () => exportPack(withRefresh, 'budget'), { title: t('packs.export_tip') }),
      smallButton(t('packs.import_btn'), 'packs:import', () => importPack(withRefresh, 'budget'), { title: t('packs.import_tip') }),
      opts.hasLibrary
        ? smallButton(t('packs.export_library_btn'), 'packs:export-library', () => exportPack(withRefresh, 'library'), { title: t('packs.export_library_tip') })
        : null,
      opts.hasLibrary
        ? smallButton(t('packs.import_library_btn'), 'packs:import-library', () => importPack(withRefresh, 'library'), {
            enabled: opts.libraryWritable,
            title: opts.libraryWritable ? t('packs.import_library_tip') : t('packs.library_in_tab'),
          })
        : null,
    ),
  );
  const v = packsState.view;
  if (packsState.error) parts.push(el('p', { class: 'setup-error apply-error', text: packsState.error, attrs: { role: 'alert' } }));
  else if (!v) parts.push(el('p', { class: 'setup-empty', text: t('packs.looking') }));
  else {
    const list = el('ul', { class: 'apply-items pk-list', attrs: { 'aria-label': t('packs.section_title') } });
    for (const p of v.packs || []) {
      const title = str(p.title) || str(p.file);
      const meta = [
        p.version ? t('packs.meta.version', { version: p.version }) : '',
        p.effective ? t('packs.meta.effective', { date: p.effective }) : '',
        p.region,
        p.place === 'bundled' ? t('packs.meta.bundled') : '',
        t('packs.meta.items', { count: p.items }),
      ]
        .filter(Boolean)
        .join(' · ');
      const head = el('div', { class: 'pk-item-text' }, el('span', { class: 'apply-name', text: title, title: `${str(p.file)}${BUDGET_EXT}` }), el('span', { class: 'apply-desc', text: p.problem && !p.items ? p.problem : meta }));
      const actions = el(
        'div',
        { class: 'library-actions' },
        smallButton(t('packs.add_to_budget'), `packs:add:${p.token}`, () => reviewListed(withRefresh, p.token, 'budget'), { enabled: p.items > 0 }),
        opts.hasLibrary ? smallButton(t('packs.add_to_library'), `packs:lib:${p.token}`, () => reviewListed(withRefresh, p.token, 'library'), { enabled: p.items > 0 && opts.libraryWritable }) : null,
      );
      list.append(el('li', { class: 'pk-item' }, head, actions));
    }
    if (!(v.packs || []).length) {
      parts.push(el('p', { class: 'setup-empty', text: t('packs.none_yet') }));
    }
    parts.push(list);
    parts.push(
      el(
        'div',
        { class: 'library-actions' },
        smallButton(t('packs.show_folder'), 'packs:reveal', async () => {
          try {
            await ctx.call('packs_reveal', {});
          } catch (e) {
            ctx.toast(e);
          }
        }, { title: str(v.folder) }),
        smallButton(t('packs.refresh'), 'packs:refresh', refresh, { title: t('packs.refresh_tip') }),
      ),
    );
  }
  return el('section', { class: 'pk-section', attrs: { 'aria-label': t('packs.section_title') } }, ...parts);
}
