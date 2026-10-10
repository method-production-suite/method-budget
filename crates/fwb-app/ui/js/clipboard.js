// Copying and pasting whole rows with everything on them (src/clipboard.rs).
//
// With whole rows selected (Select All, or a selection across every column),
// Copy and Cut put the rows' cells on the system clipboard as tab-separated
// text, as for cells, and Rust keeps the rows themselves: every field, with
// the fringes, groups, units, globals, sets, locations and currencies they
// use. On the account list the rows are accounts, on the topsheet
// categories, each with everything in it. Rust keeps one copy for the whole
// app, so it pastes into any budget.
//
// Paste checks whether the clipboard text is the copied rows' text. If it
// is, the rows go in above the selected row (accounts above the selected
// account, categories above the selected category), after a review when
// setup items will be added or are set up differently here; otherwise it is
// a paste of cells, as before. Edit > Copy Rows, Cut Rows and Paste Rows do
// the same from the menu bar and the right-click menu, whatever columns are
// selected.

import { invoke } from './api.js';
import { el } from './dom.js';
import { customDialog } from './modal.js';
import { findCategory } from './levels.js';

let ctx = null;
// The text the last row copy put on the system clipboard (normalized), or
// null when it couldn't, or nothing has been copied as rows.
let lastText = null;
// Rust holds copied rows (put on the clipboard or not).
let held = false;
// While Copy Rows asks the browser to copy: the text to put out.
let forced = null;

/**
 * ctx: { grid, level(), ix(), cat(), acct(), act(cmd, args, opts),
 *        render(opts), toast(message) } from main.js.
 */
export function initClipboard(c) {
  ctx = c;
  // Copy Rows from a menu raises a copy event; it carries the rows' text,
  // wherever focus is.
  document.addEventListener(
    'copy',
    (e) => {
      if (!forced || !e.clipboardData) return;
      e.preventDefault();
      e.stopImmediatePropagation();
      e.clipboardData.setData('text/plain', forced.text);
      forced.done = true;
    },
    true,
  );
}

/** Clipboard text as Rust compares it: \n line ends, no trailing newline. */
export function normalize(text) {
  return String(text).replace(/\r\n?/g, '\n').replace(/\n+$/, '');
}

const NOUNS = {
  rows: ['line', 'lines'],
  accounts: ['account', 'accounts'],
  categories: ['category', 'categories'],
};

function count(n, what) {
  const [one, many] = NOUNS[what] || ['row', 'rows'];
  return `${n} ${n === 1 ? one : many}`;
}

function currentCategory() {
  const c = ctx.cat();
  return c ? findCategory(ctx.ix(), c.code, c.ti) : null;
}

/** What Rust copies for these grid rows at this level, or null when nothing. */
function pickFor(keys) {
  const level = ctx.level();
  if (level === 'details') {
    const rowIds = keys.filter((k) => /^r\d+$/.test(k)).map((k) => Number(k.slice(1)));
    return rowIds.length ? { what: 'rows', rowIds } : null;
  }
  if (level === 'accounts') {
    const entry = currentCategory();
    const accts = entry ? entry.cat.accounts || [] : [];
    const accounts = keys
      .filter((k) => /^a\d+$/.test(k))
      .map((k) => accts[Number(k.slice(1))])
      .filter(Boolean)
      .map((a) => a.code);
    return accounts.length ? { what: 'accounts', accounts } : null;
  }
  const topsheet = ctx.ix().topsheet;
  const categories = keys
    .filter((k) => /^t\d+$/.test(k))
    .map((k) => Number(k.slice(1)))
    .filter((ti) => topsheet[ti] && topsheet[ti].kind !== 'production_total');
  return categories.length ? { what: 'categories', categories } : null;
}

function nothingToCopy() {
  const level = ctx.level();
  if (level === 'details') return 'Select the lines to copy first.';
  if (level === 'accounts') return 'Select the accounts to copy first.';
  return 'Select the categories to copy first (production totals are not copied).';
}

/** Hands the copied rows to Rust; a cut also deletes them (one undo step). */
async function keep(pick, text, onClipboard, cut) {
  lastText = onClipboard ? normalize(text) : null;
  held = true;
  if (cut) {
    await ctx.act('clip_cut', { pick, text, onClipboard });
    ctx.grid.focus();
    return;
  }
  try {
    await invoke('clip_copy', { pick, text, onClipboard });
  } catch (e) {
    ctx.toast(e);
  }
}

/**
 * The copy and cut events (main.js): with whole rows selected, puts their
 * text on the clipboard, has Rust keep the rows (a cut deletes them too) and
 * returns true. Returns false for cells, which main.js copies as before.
 */
export function copyEvent(data, { cut = false } = {}) {
  const grid = ctx.grid;
  if (!grid.wholeRows()) return false;
  const keys = grid.selectedKeys();
  const pick = pickFor(keys);
  if (!pick) return false;
  const text = grid.rowsText(keys);
  data.setData('text/plain', text);
  keep(pick, text, true, cut);
  return true;
}

/** Cells were copied: they replace any copied rows. */
export function cellsCopied() {
  forget();
}

function forget() {
  if (!held) return;
  held = false;
  lastText = null;
  invoke('clip_forget').catch(() => {});
}

/**
 * Edit > Copy Rows or Cut Rows: the selected rows, whatever columns are
 * selected. The text goes on the system clipboard when the web view allows
 * it from here; if it doesn't, Paste Rows still pastes the rows.
 */
export function copyRows(cut = false) {
  const grid = ctx.grid;
  const keys = grid.selectedKeys();
  const pick = pickFor(keys);
  if (!pick) {
    ctx.toast(nothingToCopy());
    return;
  }
  const text = grid.rowsText(keys);
  forced = { text, done: false };
  try {
    document.execCommand('copy');
  } catch {
    // refused: tried below another way
  }
  const done = forced.done;
  forced = null;
  if (done) {
    keep(pick, text, true, cut);
    return;
  }
  const clip = navigator.clipboard;
  const write = clip && typeof clip.writeText === 'function' ? clip.writeText(text).then(() => true, () => false) : Promise.resolve(false);
  write.then((ok) => keep(pick, text, ok, cut));
}

/**
 * The paste event (main.js): when `text` is the copied rows' text, pastes
 * the rows; otherwise runs `cellPaste` (cells, as before) right away.
 */
export async function pasteEvent(text, cellPaste) {
  if (lastText === null || normalize(text) !== lastText) {
    // Not the rows: something else was copied since (a copy that never
    // reached the clipboard stays for Paste Rows).
    if (lastText !== null) forget();
    cellPaste();
    return;
  }
  let preview = null;
  try {
    preview = await invoke('clip_preview', { text });
  } catch (e) {
    ctx.toast(e);
    return;
  }
  if (!preview) {
    lastText = null;
    held = false;
    cellPaste();
    return;
  }
  await land(preview);
}

/** Edit > Paste Rows: the copied rows, whatever the system clipboard holds. */
export async function pasteRows() {
  let preview = null;
  try {
    preview = await invoke('clip_preview', { text: null });
  } catch (e) {
    ctx.toast(e);
    return;
  }
  if (!preview) {
    ctx.toast('There are no copied rows to paste. Select whole rows and copy them first.');
    return;
  }
  await land(preview);
}

/** Where copied rows of this kind go from here: { target, opts } or { error }. */
function targetFor(what) {
  const level = ctx.level();
  const keys = ctx.grid.selectedKeys();
  if (what === 'rows') {
    const acct = ctx.acct();
    if (level !== 'details' || !acct) return { error: 'These are lines: open an account to paste them.' };
    const key = keys.find((k) => /^r\d+$/.test(k));
    return { target: { account: acct.code, beforeRow: key ? Number(key.slice(1)) : null }, opts: {} };
  }
  if (what === 'accounts') {
    const entry = currentCategory();
    if (level !== 'accounts' || !entry) return { error: 'These are accounts: open a category to paste them.' };
    const accts = entry.cat.accounts || [];
    const key = keys.find((k) => /^a\d+$/.test(k));
    const ai = key ? Number(key.slice(1)) : accts.length;
    const before = accts[ai] ? accts[ai].code : null;
    return {
      target: { category: entry.cat.code, beforeAccount: before },
      opts: { focusKey: `a${before ? ai : accts.length}`, focusCol: 'name' },
    };
  }
  if (level !== 'topsheet') return { error: 'These are categories: go to the topsheet to paste them.' };
  const key = keys.find((k) => /^t\d+$/.test(k));
  const at = key ? Number(key.slice(1)) : ctx.ix().topsheet.length;
  return { target: { at }, opts: { focusKey: `t${at}`, focusCol: 'name' } };
}

/** Pastes what `preview` describes here, after the review when it needs one. */
async function land(preview) {
  const where = targetFor(preview.what);
  if (where.error) {
    ctx.toast(where.error);
    return;
  }
  let resolutions = [];
  if (preview.review) {
    resolutions = await review(preview);
    if (!resolutions) {
      ctx.grid.focus();
      return;
    }
  }
  const v = await ctx.act('clip_paste', { token: preview.token, target: where.target, resolutions }, where.opts);
  if (v && preview.what === 'rows' && typeof v.created_row === 'number') {
    ctx.render({ focusKey: `r${v.created_row}`, focusCol: 'description' });
  }
  ctx.grid.focus();
}

// ---------------------------------------------------------------------------
// The review: what will be added, what is set up differently here

const KIND_LABELS = {
  fringe: 'Fringe',
  group: 'Group',
  global: 'Global',
  unit: 'Unit',
  set: 'Set',
  location: 'Location',
  currency: 'Currency',
};

function itemRow(lines, ...actions) {
  const text = el('div', { class: 'safety-item-text' });
  lines.forEach((line, i) => {
    if (!line) return;
    text.append(el('div', { class: i === 0 ? 'safety-item-main' : 'safety-item-sub', text: line, title: line }));
  });
  return el('li', { class: 'safety-item' }, text, el('div', { class: 'safety-item-actions' }, ...actions));
}

function choiceSelect(item) {
  const label = `${KIND_LABELS[item.kind] || item.kind} ${item.name}`;
  const select = el('select', { class: 'modal-input', attrs: { 'aria-label': `What to do with ${label}` } });
  const options = [
    ['keep', "Keep this budget's version"],
    ['copied', 'Use the copied version everywhere here'],
  ];
  if (item.rename_to) options.push(['rename', `Add the copied one as ${item.rename_to}`]);
  for (const [value, text] of options) select.append(el('option', { text, attrs: { value } }));
  select.value = 'keep';
  select.title = 'The copied version replaces this budget\'s for every line here that uses it; a new name leaves this budget\'s as it is.';
  return select;
}

/**
 * Shows what pasting will add and what is set up differently here, with a
 * choice for each difference (this budget's version unless changed).
 * Resolves with the choices ([{ kind, name, choice }]) on Paste, or null on
 * Cancel, Escape or a click outside. `action` names the button and the
 * title ('Insert' for a line block, presets.js).
 */
export function review(p, { action = 'Paste' } = {}) {
  return new Promise((resolve) => {
    let settled = false;
    const finish = (value) => {
      if (settled) return;
      settled = true;
      resolve(value);
    };
    const differs = p.items.filter((i) => i.status === 'differs');
    const adds = p.items.filter((i) => i.status === 'add');
    const selects = [];
    let box = null;
    customDialog((closeDialog) => {
      const parts = [];
      const from = p.source ? ` from ${p.source}` : '';
      parts.push(el('h2', { class: 'modal-title', text: `${action} ${count(p.count, p.what)}${from}`, attrs: { id: 'clip-review-title' } }));
      if (differs.length) {
        parts.push(
          el('p', {
            class: 'modal-message',
            text: 'Set up differently here. This budget keeps its own version unless you choose otherwise, so pasted lines may total differently than where they were copied.',
          }),
        );
        const list = el('ul', { class: 'safety-list' });
        for (const it of differs) {
          const select = choiceSelect(it);
          selects.push([it, select]);
          list.append(itemRow([`${KIND_LABELS[it.kind] || it.kind} ${it.name}`, `Here: ${it.here || ''}`, `Copied: ${it.copied}`], select));
        }
        parts.push(list);
      }
      if (adds.length) {
        parts.push(el('p', { class: 'modal-message', text: 'Not in this budget yet. These will be added.' }));
        const list = el('ul', { class: 'safety-list' });
        for (const it of adds) {
          list.append(itemRow([`${KIND_LABELS[it.kind] || it.kind} ${it.name}`, it.copied], el('span', { class: 'safety-item-sub', text: 'Will be added' })));
        }
        parts.push(list);
      }
      for (const note of p.notes || []) parts.push(el('p', { class: 'modal-message', text: note }));
      const cancel = el('button', { class: 'btn', text: 'Cancel', attrs: { type: 'button' } });
      const ok = el('button', { class: 'btn btn-primary', text: action, attrs: { type: 'button', 'data-autofocus': true } });
      cancel.addEventListener('click', () => {
        finish(null);
        closeDialog();
      });
      ok.addEventListener('click', () => {
        finish(selects.map(([it, s]) => ({ kind: it.kind, name: it.name, choice: s.value })));
        closeDialog();
      });
      parts.push(el('div', { class: 'modal-actions' }, cancel, ok));
      box = el('div', { class: 'modal modal-wide', attrs: { role: 'dialog', 'aria-modal': 'true', 'aria-labelledby': 'clip-review-title' } }, ...parts);
      return box;
    });
    // Escape and a click outside close the dialog without telling us: that
    // is a Cancel. (The dialog sits in its backdrop, in the modal root.)
    const root = box.parentElement && box.parentElement.parentElement;
    if (!root) {
      finish(null);
      return;
    }
    const watch = new MutationObserver(() => {
      if (box.isConnected) return;
      watch.disconnect();
      finish(null);
    });
    watch.observe(root, { childList: true });
  });
}
