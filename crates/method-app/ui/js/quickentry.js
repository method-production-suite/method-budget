// Quick entry on the detail grid: type-ahead in cells (autocomplete.js),
// adding a unit, set, location, currency or global from the cell, the line
// note editor, the fringe range keys, and the formula bar.
//
// main.js wires these in: editAssist is the grid's onEditStart hook,
// showFormula its onCursor hook, commitDetailCell sends a detail cell edit,
// and editNote and fringeRange are commands from the shortcut list.
//
// Adding from the cell is one command (set_cell_creating), so the new setup
// item and the cell are one undo step, and a refusal changes neither.

import { $ } from './dom.js';
import { decimal } from './format.js';
import { Assist, canonicalName } from './autocomplete.js';
import { anchoredDialog } from './modal.js';
import { orderedNames, hasCustomOrder } from './order.js';

// { grid, view(), level(), act(cmd, args), call(cmd, args), toast(message) }
let ctx = null;

export function initQuickEntry(context) {
  ctx = context;
}

const UNIT_FIELDS = new Set(['unit', 'unit2', 'unit3', 'unit4']);
const FORMULA_FIELDS = new Set(['amount', 'x', 'rate', 'x4']);
const PLAIN_NUMBER = /^[+-]?(\d+\.?\d*|\.\d+)$/;

function budget() {
  const v = ctx && ctx.view();
  return (v && v.budget) || {};
}

function table(name) {
  const t = budget()[name];
  return t && typeof t === 'object' && !Array.isArray(t) ? t : {};
}

function text(v) {
  return v === undefined || v === null ? '' : String(v);
}

/** The setup kind a column's typed names can add, or null. */
export function addableKind(colKey) {
  if (UNIT_FIELDS.has(colKey)) return 'unit';
  if (colKey === 'set' || colKey === 'location' || colKey === 'currency') return colKey;
  return null;
}

/** Every name a kind of setup item has now (a currency list includes the base). */
function namesOf(kind) {
  if (kind === 'unit') return Object.keys(table('units'));
  if (kind === 'set') return Object.keys(table('sets'));
  if (kind === 'location') return Object.keys(table('locations'));
  if (kind === 'currency') {
    const base = text(budget().base_currency);
    return [...(base ? [base] : []), ...Object.keys(table('currencies'))];
  }
  return [];
}

/**
 * A table's items for the type-ahead, in the budget's order, each with its
 * description (and short code, which the type-ahead also matches).
 */
function described(tableName) {
  const map = table(tableName);
  return orderedNames(budget(), tableName).map((name) => {
    const def = map[name] || {};
    const code = typeof def.short_code === 'string' ? def.short_code : '';
    const hint = [code, text(def.description)].filter(Boolean).join(' · ');
    return code ? { name, hint, code } : { name, hint };
  });
}

/** Whether the type-ahead keeps `tableName`'s own order (it has a hand-made one). */
function ordered(tableName) {
  return hasCustomOrder(budget(), tableName);
}

/** What the type-ahead offers in a detail column (see autocomplete.js), or null. */
export function assistConfig(colKey) {
  if (UNIT_FIELDS.has(colKey)) {
    const units = table('units');
    const candidates = orderedNames(budget(), 'units').map((name) => {
      const u = units[name] || {};
      const hours = u.hours !== undefined && u.hours !== null ? `${decimal(u.hours)} h` : '';
      return { name, hint: text(u.description) || hours };
    });
    return { mode: 'whole', candidates, addKind: 'unit', addLabel: 'unit', label: 'Units', ordered: ordered('units') };
  }
  if (FORMULA_FIELDS.has(colKey)) {
    const v = ctx.view();
    const values = (v && v.report && v.report.globals) || {};
    const globals = table('globals');
    const candidates = orderedNames(budget(), 'globals').map((name) => ({
      name,
      hint: values[name] !== undefined ? decimal(values[name]) : text(globals[name]),
    }));
    return { mode: 'formula', candidates, addKind: 'global', label: 'Globals', ordered: ordered('globals') };
  }
  if (colKey === 'currency') {
    const base = text(budget().base_currency);
    const currencies = table('currencies');
    const candidates = orderedNames(budget(), 'currencies').map((name) => {
      const c = currencies[name] || {};
      return { name, hint: text(c.description) || (c.rate !== undefined ? `${decimal(c.rate)} ${base}` : '') };
    });
    if (base) candidates.push({ name: base, hint: 'Base currency' });
    return { mode: 'whole', candidates, addKind: 'currency', addLabel: 'currency', label: 'Currencies', ordered: ordered('currencies') };
  }
  if (colKey === 'set') return { mode: 'whole', candidates: described('sets'), addKind: 'set', addLabel: 'set', label: 'Sets', ordered: ordered('sets') };
  if (colKey === 'location') {
    return { mode: 'whole', candidates: described('locations'), addKind: 'location', addLabel: 'location', label: 'Locations', ordered: ordered('locations') };
  }
  if (colKey === 'fringes') return { mode: 'list', candidates: described('fringes'), label: 'Fringes', ordered: ordered('fringes') };
  if (colKey === 'groups') return { mode: 'list', candidates: described('groups'), label: 'Groups', ordered: ordered('groups') };
  return null;
}

/** The grid's onEditStart hook: a type-ahead list for setup-backed detail cells. */
export function editAssist({ input, rowKey, colKey, typed, anchor, host }) {
  if (!ctx || ctx.level() !== 'details' || !String(rowKey).startsWith('r')) return null;
  const config = assistConfig(colKey);
  if (!config) return null;
  const assist = new Assist({ input, anchor, host, config, onCreate: (item) => createGlobal(item) });
  // Started by typing a letter: show what it matches right away.
  if (typed) assist.refresh();
  return assist;
}

/**
 * "Create global NAME" was chosen in a formula cell: asks for its value next
 * to the cell, then adds the global and sets the cell as one step. Cancel goes
 * back to editing the cell with the formula as typed.
 */
async function createGlobal(item) {
  const grid = ctx.grid;
  const anchor = grid.editor ? grid.editor.td : null;
  const taken = grid.takeEdit();
  if (!taken) return;
  const rowId = Number(taken.rowKey.slice(1));
  const ok = await anchoredDialog({
    anchor,
    title: `New global ${item.name}`,
    message: 'Its value can be a number, or a formula that uses other globals.',
    label: 'Value',
    placeholder: 'e.g. 30',
    submitText: 'Create',
    onSubmit: (value) =>
      ctx.call('set_cell_creating', {
        rowId,
        field: taken.colKey,
        value: taken.value,
        create: { kind: 'global', name: item.name, value },
      }),
  });
  if (!ok) grid.resumeEdit(taken.rowKey, taken.colKey, taken.value);
  else grid.focus();
}

/**
 * Sends a detail cell edit. A unit, set, location or currency the budget
 * doesn't have yet is added to setup along with the edit (one undo step); one
 * it has under another capitalization is written the way setup spells it.
 */
export function commitDetailCell(rowKey, colKey, value) {
  const rowId = Number(rowKey.slice(1));
  const kind = addableKind(colKey);
  const typed = value.trim();
  if (kind && typed && !typed.includes(',')) {
    const known = canonicalName(namesOf(kind), typed);
    if (known === null) {
      return ctx
        .act('set_cell_creating', { rowId, field: colKey, value: typed, create: { kind, name: typed } })
        .then((v) => {
          if (v && kind === 'currency') {
            const base = text(budget().base_currency);
            ctx.toast(`Added ${typed} to Currencies at 1 ${base} per ${typed}. Set its exchange rate there.`);
          }
          return v;
        });
    }
    value = known;
  }
  return ctx.act('set_detail_field', { rowId, field: colKey, value });
}

// ---------------------------------------------------------------------------
// Line notes (Edit > Note)

function detailById(id) {
  const v = ctx.view();
  const details = v && v.account && Array.isArray(v.account.details) ? v.account.details : [];
  return details.find((d) => d.id === id) || null;
}

/**
 * Adds a dated note (Shift+Cmd+Space, Edit > Add Note, the right-click menu)
 * to what the cursor is on: the line on an account's lines, the account on
 * the account list, the category on the topsheet. A small dialog next to its
 * description or name; the Notes tab shows, edits and deletes the notes.
 */
export async function editNote() {
  if (!ctx) return;
  const grid = ctx.grid;
  const row = grid.cursorRow();
  if (!row) return;
  const level = ctx.level();
  const v = ctx.view() || {};
  const top = Array.isArray(v.budget && v.budget.topsheet) ? v.budget.topsheet : [];
  let owner = null;
  let what = '';
  let count = 0;
  let col = 'name';
  const noteCount = (notes) => (Array.isArray(notes) ? notes.filter((n) => n && text(n.text).trim()).length : 0);
  if (level === 'details' && String(row.key).startsWith('r')) {
    const rowId = Number(row.key.slice(1));
    const d = detailById(rowId);
    if (!d) return;
    const num = row.cells && row.cells.num ? row.cells.num.text : '';
    const desc = text(d.description).trim();
    owner = { row: rowId };
    what = desc ? `Line ${num}: ${desc}` : `Line ${num}`;
    count = noteCount(d.notes);
    col = 'description';
  } else if (level === 'accounts' && String(row.key).startsWith('a')) {
    const cat = top.find((c) => c && Array.isArray(c.accounts) && c.accounts.some((a) => a.code === row.code));
    const a = cat ? cat.accounts.find((x) => x.code === row.code) : null;
    if (!a) return;
    owner = { account: text(a.code) };
    what = `Account ${text(a.code)} ${text(a.name)}`.trim();
    count = noteCount(a.notes);
  } else if (level === 'topsheet' && row.kind === 'category') {
    const c = top[row.ti];
    if (!c) return;
    owner = { category: text(c.code) };
    what = `Category ${text(c.code)} ${text(c.name)}`.trim();
    count = noteCount(c.notes);
  } else {
    return;
  }
  const anchor = grid.cellFor(row.key, col) || grid.cellFor(row.key, grid.cursorColKey());
  const had = count ? ` (${count === 1 ? 'it has 1 note' : `it has ${count} notes`}; the Notes tab shows them)` : '';
  let author = '';
  try {
    const a = JSON.parse(window.localStorage.getItem('method.noteAuthor') || '""');
    author = typeof a === 'string' ? a : '';
  } catch {
    author = '';
  }
  await anchoredDialog({
    anchor,
    title: 'Add note',
    message: `${what}${had}`,
    label: 'Note',
    value: '',
    multiline: true,
    submitText: 'Add',
    onSubmit: (note) =>
      note.trim() ? ctx.call('add_note', { owner, text: note, created: new Date().toISOString(), author: author || null }) : Promise.resolve(),
  });
  grid.focus();
}

// ---------------------------------------------------------------------------
// Fringe ranges from the keyboard (the apply panel has the same actions)

/** Makes the selected lines one fringe range (`make`), or takes them out of theirs. */
export function fringeRange(make) {
  if (!ctx || ctx.level() !== 'details') return;
  const ids = ctx.grid
    .selectedKeys()
    .filter((k) => String(k).startsWith('r'))
    .map((k) => Number(k.slice(1)));
  if (!ids.length) return;
  ctx.act(make ? 'make_fringe_range' : 'clear_fringe_range', { rowIds: ids });
}

// ---------------------------------------------------------------------------
// The formula bar: what the selected cell holds, as typed

/** The grid's onCursor hook. */
export function showFormula(at) {
  const bar = $('formula-bar');
  if (!bar) return;
  const label = $('formula-label');
  const value = $('formula-text');
  const cell = at ? at.cell : null;
  label.textContent = at && at.col.key !== 'num' ? at.col.label : '';
  let shown = '';
  let cls = '';
  if (cell && cell.edit !== undefined) {
    shown = String(cell.edit);
    if (FORMULA_FIELDS.has(at.col.key) && shown.trim() && !PLAIN_NUMBER.test(shown.trim())) cls = 'formula';
  } else if (cell && cell.text !== undefined && cell.text !== null && at.col.key !== 'num') {
    shown = String(cell.text);
    cls = 'ro';
  }
  value.textContent = shown;
  value.className = `formula-text${cls ? ` ${cls}` : ''}`;
  bar.classList.toggle('empty', !shown);
}
