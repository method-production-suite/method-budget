// Copies for other people and filtered copies (src/sharing.rs; decisions.md,
// entry 39):
//
// - File > Save a Copy for Sharing…: a short dialog (what to share: the
//   budget as it is or one of its versions; what to leave out; a note), then
//   Rust's Save dialog, suggesting "<title> - <version> - <date>.mthd".
// - A copy saved for sharing opens read-only. Its banner says where it came
//   from and offers Make Editable Copy, and, when the budget it came from is
//   open in another tab, Compare With Original and Store as a Version of the
//   Original. Opening a returned copy (shared or editable) next to its
//   original asks once whether to compare them or store the copy.
// - File > New Sub-Budget…: pick categories and accounts, then groups, sets
//   and locations (any of the ticked values within a kind, every kind with a
//   tick), and what to leave out; the sub-budget opens in a new tab.
//
// Everything is drawn with el() and textContent: names, titles and notes from
// a budget file are never read as HTML. No inline styles (the CSP).

import { el, $ } from './dom.js';
import { customDialog, formDialog, isModalOpen } from './modal.js';
import { localStamp, createdText, versionsOf } from './versions.js';

let uid = 0;
function nextId(prefix) {
  uid += 1;
  return `${prefix}-${uid}`;
}

function provenanceOf(view) {
  const p = view && view.budget && view.budget.provenance;
  return p && typeof p === 'object' ? p : null;
}

function check(label, checked = false) {
  const id = nextId('share-check');
  const box = el('input', { attrs: { type: 'checkbox', id } });
  box.checked = checked;
  const row = el('label', { class: 'rs-check', attrs: { for: id } }, box, el('span', { text: label, title: label }));
  return { box, row };
}

function names(map) {
  return map && typeof map === 'object' && !Array.isArray(map) ? Object.keys(map).sort() : [];
}

// ---------------------------------------------------------------------------
// File > Save a Copy for Sharing…

/**
 * File > Save a Copy for Sharing…. `ctx`: { view(), invoke, toast, focus }.
 */
export function saveShareCopy(ctx) {
  const view = ctx.view();
  if (!view) return;
  const versions = versionsOf(view);
  customDialog(
    (close) => {
      const what = el('select', { class: 'modal-input', attrs: { id: nextId('share-what') } });
      what.append(el('option', { text: 'The budget as it is now', attrs: { value: '' } }));
      for (const v of versions) {
        const when = createdText(v.created);
        what.append(el('option', { text: when ? `Version: ${v.name} (${when})` : `Version: ${v.name}`, attrs: { value: String(v.id) } }));
      }
      const notes = check('Notes on lines, accounts and categories');
      const info = check('Budget info fields');
      const vers = check('Saved versions');
      const base = check('Original totals (baseline)');
      const unused = check('Setup items nothing uses');
      // Only a budget with actuals has them to leave out (the view's cost figures say so).
      const hasActuals = Boolean(view.cost);
      const acts = check('Actuals (transactions and estimates to complete)');
      const note = el('textarea', { class: 'modal-input share-note', attrs: { id: nextId('share-note'), rows: '3', maxlength: '10000' } });
      const error = el('p', { class: 'modal-error', attrs: { role: 'alert' } });
      error.hidden = true;
      const cancel = el('button', { class: 'btn', text: 'Cancel', attrs: { type: 'button' } });
      const ok = el('button', { class: 'btn btn-primary', text: 'Save Copy…', attrs: { type: 'button', 'data-autofocus': true } });
      cancel.addEventListener('click', () => close());
      ok.addEventListener('click', async () => {
        ok.disabled = true;
        error.hidden = true;
        try {
          const r = await ctx.invoke('share_copy_save', {
            version: what.value === '' ? null : Number(what.value),
            leaveOut: {
              notes: notes.box.checked,
              info: info.box.checked,
              versions: vers.box.checked,
              baseline: base.box.checked,
              unused_setup: unused.box.checked,
              actuals: hasActuals && acts.box.checked,
            },
            note: note.value,
            saved: localStamp(),
          });
          if (r) {
            close();
            ctx.toast(`Saved the copy ${r.name}. It opens read-only.`);
            ctx.focus();
            return;
          }
        } catch (e) {
          error.textContent = String(e);
          error.hidden = false;
        }
        ok.disabled = false;
      });
      return el(
        'div',
        { class: 'modal modal-wide', attrs: { role: 'dialog', 'aria-modal': 'true', 'aria-labelledby': 'share-title' } },
        el('h2', { class: 'modal-title', text: 'Save a Copy for Sharing', attrs: { id: 'share-title' } }),
        el('p', {
          class: 'modal-message',
          text: 'Saves a snapshot of this budget to a new file to send to someone. The copy opens read-only and says where it came from. This budget and its file are not changed.',
        }),
        // With no versions there is nothing to pick, so say what gets shared
        // instead of showing a one-item list.
        versions.length
          ? el('label', { class: 'modal-field', attrs: { for: what.id } }, el('span', { text: 'Share' }), what)
          : el('p', { class: 'modal-message', text: 'Shares the budget as it is now. To share an earlier state, save it as a version first and it shows up here.' }),
        el(
          'fieldset',
          { class: 'share-fieldset' },
          el('legend', { text: 'Leave out' }),
          notes.row,
          info.row,
          ...(versions.length ? [vers.row] : []),
          base.row,
          ...(hasActuals ? [acts.row] : []),
          unused.row,
        ),
        el('label', { class: 'modal-field', attrs: { for: note.id } }, el('span', { text: 'Note for the copy (optional)' }), note),
        error,
        el('div', { class: 'modal-actions' }, cancel, ok),
      );
    },
    { onClose: () => ctx.focus() },
  );
}

// ---------------------------------------------------------------------------
// The shared copy's banner, and the offer when a returned copy opens

// doc id -> { key, original } (the last answer of share_copy_original).
const originals = new Map();
// Copies already asked about, by doc id.
const offered = new Set();

function tabsKey(tabs) {
  return (tabs || []).map((t) => `${t.id}:${t.title}`).join('|');
}

/** The open budget copy `doc` came from: { doc, label } or null (asked once per set of tabs). */
async function originalOf(ctx, doc, tabs) {
  const key = tabsKey(tabs);
  const known = originals.get(doc);
  if (known && known.key === key) return known.original;
  let original = null;
  try {
    original = await ctx.invoke('share_copy_original', { doc });
  } catch (e) {
    original = null;
  }
  originals.set(doc, { key, original });
  return original;
}

function sourceText(p) {
  const when = createdText(p.saved);
  const version = p.source_version ? ` (version “${p.source_version}”)` : '';
  return `“${p.source_title || 'Untitled'}”${version}${when ? `, saved ${when}` : ''}`;
}

/**
 * Draws the shared-copy banner for `view` into #share-banner (hidden for any
 * other budget). `ctx`: { doc(), tabs(), invoke, apply(view), toast,
 * compare(originalDoc, copyDoc), focus }.
 */
export function renderShareBanner(view, ctx) {
  const host = $('share-banner');
  if (!host) return;
  const p = provenanceOf(view);
  if (!p || p.kind !== 'shared_copy') {
    host.hidden = true;
    host.replaceChildren();
    return;
  }
  const doc = view.doc;
  const make = el('button', { class: 'btn btn-quiet', text: 'Make Editable Copy', attrs: { type: 'button' } });
  make.addEventListener('click', () => makeEditable(ctx, doc));
  const actions = el('span', { class: 'share-banner-actions' }, make);
  host.replaceChildren(
    el('strong', { text: 'Read-only copy saved for sharing.' }),
    el('span', { class: 'share-banner-detail', text: `From ${sourceText(p)}.` }),
    p.note ? el('span', { class: 'share-banner-detail', text: `Note: ${p.note}` }) : null,
    actions,
  );
  host.hidden = false;
  originalOf(ctx, doc, ctx.tabs()).then((o) => {
    if (!o || ctx.doc() !== doc || !host.contains(actions)) return;
    actions.append(...originalButtons(ctx, doc, o));
  });
}

function originalButtons(ctx, doc, original) {
  const compare = el('button', { class: 'btn btn-quiet', text: 'Compare With Original', title: `Compare with ${original.label}`, attrs: { type: 'button' } });
  compare.addEventListener('click', () => ctx.compare(original.doc, doc));
  const store = el('button', { class: 'btn btn-quiet', text: 'Store as a Version of the Original', title: `Store this copy as a version of ${original.label}`, attrs: { type: 'button' } });
  store.addEventListener('click', () => storeAsVersion(ctx, doc, original));
  return [compare, store];
}

async function makeEditable(ctx, doc) {
  try {
    const v = await ctx.invoke('share_copy_editable', { doc });
    if (v) ctx.apply(v);
  } catch (e) {
    ctx.toast(e);
  }
  ctx.focus();
}

async function storeAsVersion(ctx, doc, original) {
  const view = ctx.view();
  const p = provenanceOf(view) || {};
  const day = typeof p.saved === 'string' ? p.saved.slice(0, 10) : '';
  const suggested = p.source_version ? `Returned copy of ${p.source_version}` : `Returned copy${day ? ` ${day}` : ''}`;
  await formDialog({
    title: 'Store as a Version of the Original',
    message: `Saves this copy inside ${original.label} as a new version, to compare or restore later. ${original.label} itself doesn't change, and Undo there takes it back.`,
    fields: [{ key: 'name', label: 'Version name', value: suggested }],
    submitText: 'Store Version',
    onSubmit: async (values) => {
      const v = await ctx.invoke('share_copy_store', { doc, original: original.doc, name: values.name, created: localStamp() });
      if (v) ctx.apply(v);
      ctx.toast(`Stored as the version “${values.name.trim()}” of ${original.label}.`);
    },
  });
  ctx.focus();
}

/**
 * After a view arrives: when its budget is a returned copy (shared or
 * editable) seen for the first time and the budget it came from is open,
 * asks once whether to compare them or store the copy as a version.
 */
export function offerReturnedCopy(view, ctx) {
  const p = provenanceOf(view);
  if (!p || (p.kind !== 'shared_copy' && p.kind !== 'editable_copy')) return;
  const doc = view.doc;
  if (offered.has(doc)) return;
  offered.add(doc);
  originalOf(ctx, doc, ctx.tabs()).then((o) => {
    if (!o || ctx.doc() !== doc || isModalOpen()) return;
    customDialog(
      (close) => {
        const later = el('button', { class: 'btn', text: 'Not Now', attrs: { type: 'button', 'data-autofocus': true } });
        later.addEventListener('click', () => close());
        const compare = el('button', { class: 'btn', text: 'Compare With Original', attrs: { type: 'button' } });
        compare.addEventListener('click', () => {
          close();
          ctx.compare(o.doc, doc);
        });
        const store = el('button', { class: 'btn btn-primary', text: 'Store as a Version…', attrs: { type: 'button' } });
        store.addEventListener('click', () => {
          close();
          storeAsVersion(ctx, doc, o);
        });
        return el(
          'div',
          { class: 'modal', attrs: { role: 'dialog', 'aria-modal': 'true', 'aria-labelledby': 'share-offer-title' } },
          el('h2', { class: 'modal-title', text: 'A Copy of an Open Budget', attrs: { id: 'share-offer-title' } }),
          el('p', { class: 'modal-message', text: `This budget is a copy of ${sourceText(p)}, which is open as ${o.label}. Compare the two, or keep this copy inside ${o.label} as a version?` }),
          el('div', { class: 'modal-actions' }, later, compare, store),
        );
      },
      { onClose: () => ctx.focus() },
    );
  });
}

// ---------------------------------------------------------------------------
// File > New Sub-Budget…

function tagGroup(title, values, noneLabel) {
  const boxes = values.map((n) => ({ name: n, ...check(n) }));
  const none = check(noneLabel);
  const list = el('div', { class: 'sub-list sub-tags' }, ...boxes.map((b) => b.row));
  const fs = el(
    'fieldset',
    { class: 'share-fieldset' },
    el('legend', { text: title }),
    values.length ? list : el('p', { class: 'modal-message', text: 'None in this budget.' }),
    none.row,
  );
  return {
    fs,
    read: () => ({ values: boxes.filter((b) => b.box.checked).map((b) => b.name), include_untagged: none.box.checked }),
  };
}

/**
 * File > New Sub-Budget…. `ctx`: { view(), invoke, apply(view), toast, focus }.
 */
export function newSubBudget(ctx) {
  const view = ctx.view();
  if (!view || !view.budget) return;
  const b = view.budget;
  const cats = (Array.isArray(b.topsheet) ? b.topsheet : []).filter((r) => r && r.kind === 'category');
  customDialog(
    (close) => {
      // The chart: a category's box keeps all its accounts; an account's box keeps just it.
      const chart = el('div', { class: 'sub-list sub-chart' });
      const catBoxes = [];
      const acctBoxes = [];
      for (const c of cats) {
        const cb = check(`${c.code} ${c.name || ''}`.trim());
        catBoxes.push({ code: c.code, box: cb.box });
        const accts = el('div', { class: 'sub-accounts' });
        for (const a of Array.isArray(c.accounts) ? c.accounts : []) {
          const ab = check(`${a.code} ${a.name || ''}`.trim());
          acctBoxes.push({ code: a.code, box: ab.box, cat: cb.box });
          accts.append(ab.row);
        }
        // Ticking a category ticks (and locks) its accounts: they come along.
        cb.box.addEventListener('change', () => {
          for (const x of acctBoxes) {
            if (x.cat !== cb.box) continue;
            x.box.checked = cb.box.checked;
            x.box.disabled = cb.box.checked;
          }
        });
        chart.append(el('div', { class: 'sub-cat' }, cb.row, accts));
      }
      const groups = tagGroup('Groups', names(b.groups), 'Lines with no group');
      const sets = tagGroup('Sets', names(b.sets), 'Lines with no set');
      const locations = tagGroup('Locations', names(b.locations), 'Lines with no location');
      const empty = check('Leave out accounts with no lines left');
      const fringes = check('Take the fringes off every line (totals change)');
      const notes = check('Leave out notes');
      const error = el('p', { class: 'modal-error', attrs: { role: 'alert' } });
      error.hidden = true;
      const cancel = el('button', { class: 'btn', text: 'Cancel', attrs: { type: 'button' } });
      const ok = el('button', { class: 'btn btn-primary', text: 'Create Sub-Budget', attrs: { type: 'button', 'data-autofocus': true } });
      cancel.addEventListener('click', () => close());
      ok.addEventListener('click', async () => {
        ok.disabled = true;
        error.hidden = true;
        const pickedCats = new Set(catBoxes.filter((x) => x.box.checked).map((x) => x.code));
        const spec = {
          categories: [...pickedCats],
          accounts: acctBoxes.filter((x) => x.box.checked && !x.cat.checked).map((x) => x.code),
          groups: groups.read(),
          sets: sets.read(),
          locations: locations.read(),
          skip_empty_accounts: empty.box.checked,
          drop_fringes: fringes.box.checked,
          drop_notes: notes.box.checked,
        };
        try {
          const v = await ctx.invoke('sub_budget_new', { spec, saved: localStamp() });
          close();
          if (v) ctx.apply(v);
          ctx.toast('Opened the sub-budget in a new tab. It has no file until you save it.');
          return;
        } catch (e) {
          error.textContent = String(e);
          error.hidden = false;
        }
        ok.disabled = false;
      });
      return el(
        'div',
        { class: 'modal modal-wide sub-dialog', attrs: { role: 'dialog', 'aria-modal': 'true', 'aria-labelledby': 'sub-title' } },
        el('h2', { class: 'modal-title', text: 'New Sub-Budget', attrs: { id: 'sub-title' } }),
        el('p', {
          class: 'modal-message',
          text: 'Opens a filtered copy of this budget in a new tab, with no link back to it. Nothing ticked under the chart keeps every category. Lines are kept when they have any ticked value of each kind that has a tick.',
        }),
        el('fieldset', { class: 'share-fieldset' }, el('legend', { text: 'Categories and accounts' }), cats.length ? chart : el('p', { class: 'modal-message', text: 'This budget has no categories.' })),
        el('div', { class: 'sub-tag-row' }, groups.fs, sets.fs, locations.fs),
        el('fieldset', { class: 'share-fieldset' }, el('legend', { text: 'Options' }), empty.row, fringes.row, notes.row),
        error,
        el('div', { class: 'modal-actions' }, cancel, ok),
      );
    },
    { onClose: () => ctx.focus() },
  );
}
