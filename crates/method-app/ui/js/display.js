// How numbers show on screen (roadmap 2.10, 2.18, 2.19). Display only: the
// engine keeps every value exact (decisions.md, entries 1 and 2), and nothing
// here changes a stored value or a total. Decimal strings from the engine are
// rounded as strings (BigInt), never through floats.
//
// Two kinds of setting:
//   - Budget settings, saved in the file (`budget.display`, Budget > Settings):
//     rate decimals with a cutoff band, whether a rate's first global with
//     its own decimals wins, the topsheet's summary rows and net total. And
//     each global's own info (`budget.global_info`): description, unit,
//     decimals, "show by name", folder.
//   - Per-user choices, kept in this web view's local storage (View menu):
//     totals with or without cents, hiding zero accounts, grid zoom, and
//     whether selected cells show their formulas.
//
// The pure functions at the top are what levels.js uses to build cells; the
// commands at the bottom are the View menu's items (main.js wires them).

import { decimal } from './format.js';
import { readPref, writePref } from './prefs.js';

// ---------------------------------------------------------------------------
// Exact decimal strings

const PLACES = 9; // the engine's fixed decimal places
const PLAIN = /^[+-]?(\d+\.?\d*|\.\d+)$/;
const DEC = /^([+-]?)(\d*)(?:\.(\d*))?$/;
const ONE = 10n ** BigInt(PLACES);
/** Most decimal places a display setting can ask for (MAX_DISPLAY_DECIMALS in Rust). */
export const MAX_PLACES = 8;

/** A decimal string ("12.5", "-0.25", "+3.") as a BigInt of billionths, or null. */
export function scaled(s) {
  const m = DEC.exec(String(s === null || s === undefined ? '' : s).trim());
  if (!m) return null;
  const int = m[2] || '';
  const frac = m[3] || '';
  if (int === '' && frac === '') return null;
  if (frac.length > PLACES) return null;
  const n = BigInt((int || '0') + frac.padEnd(PLACES, '0'));
  return m[1] === '-' ? -n : n;
}

/** `n` if it is a whole number of decimal places a setting can hold (0 to 8), else null. */
export function placesOf(n) {
  return Number.isInteger(n) && n >= 0 && n <= MAX_PLACES ? n : null;
}

/**
 * A decimal string rounded to `places` decimals, half away from zero, and
 * padded with zeros: roundPlaces("1.005", 2) is "1.01", roundPlaces("2", 2)
 * is "2.00". Null when `s` isn't a decimal.
 */
export function roundPlaces(s, places) {
  const n = scaled(s);
  const p = placesOf(places);
  if (n === null || p === null) return null;
  const neg = n < 0n;
  const a = neg ? -n : n;
  const div = 10n ** BigInt(PLACES - p);
  let q = a / div;
  if ((a % div) * 2n >= div) q += 1n;
  let out = q.toString();
  if (p > 0) {
    out = out.padStart(p + 1, '0');
    out = `${out.slice(0, -p)}.${out.slice(-p)}`;
  }
  return neg && q !== 0n ? `-${out}` : out;
}

/**
 * Cents for display with 2 decimals ("1,234.56") or 0 ("1,235", rounded half
 * away from zero); negatives in parentheses.
 */
export function moneyAt(cents, places = 2) {
  if (cents === null || cents === undefined || !Number.isFinite(cents)) return '';
  if (places !== 0) {
    const neg = cents < 0;
    const a = Math.abs(cents);
    return decimal(`${neg ? '-' : ''}${Math.trunc(a / 100)}.${String(a % 100).padStart(2, '0')}`);
  }
  const whole = Math.trunc((Math.abs(cents) + 50) / 100);
  return decimal(cents < 0 && whole !== 0 ? `-${whole}` : String(whole));
}

// ---------------------------------------------------------------------------
// Formula cells: values, rate decimals, globals shown by name

/** The globals a formula names, in order, each once (names the budget has). */
export function globalsIn(formula, globals) {
  const out = [];
  const have = globals && typeof globals === 'object' ? globals : {};
  // Split as the formula reader splits it: a run with a point is a number,
  // any other run of letters, digits and _ a name (1ZG is one).
  for (const name of String(formula || '').match(/[\d.]*\.[\d.]*|[A-Za-z0-9_]+/g) || []) {
    if (Object.prototype.hasOwnProperty.call(have, name) && !out.includes(name)) out.push(name);
  }
  return out;
}

/**
 * The budget's display settings with their defaults filled in (the file
 * leaves out a setting at its default).
 */
export function budgetDisplay(b) {
  const d = b && b.display && typeof b.display === 'object' ? b.display : {};
  return {
    rate_cutoff: typeof d.rate_cutoff === 'string' && d.rate_cutoff !== '' ? d.rate_cutoff : null,
    rate_decimals_below: placesOf(d.rate_decimals_below),
    rate_decimals_above: placesOf(d.rate_decimals_above),
    rate_global_decimals: d.rate_global_decimals !== false,
    summary_rows: d.summary_rows !== false,
    net_total: d.net_total !== false,
  };
}

/**
 * Decimal places for a rate of `value` under the cutoff band, or null for
 * "as typed". Below the cutoff (by size) uses the below setting; at or above
 * it, or with no cutoff, the above setting.
 */
export function ratePlaces(value, prefs) {
  const p = prefs || {};
  const above = placesOf(p.rate_decimals_above);
  const below = placesOf(p.rate_decimals_below);
  const cut = p.rate_cutoff === null || p.rate_cutoff === undefined ? null : scaled(p.rate_cutoff);
  if (cut === null) return above;
  const v = scaled(value);
  if (v === null) return null;
  return (v < 0n ? -v : v) < cut ? below : above;
}

function shownAs(value, places) {
  const r = places === null || places === undefined ? null : roundPlaces(value, places);
  return decimal(r === null ? String(value) : r);
}

/**
 * A number cell (Amount, X, Rate, 4X) for the grid.
 *
 * - Blank: empty.
 * - A plain number: as typed, except a rate takes the rate band's decimals.
 * - A formula: its value, when the engine sent one (`value`), with the first
 *   global it names that has its own decimals setting them (for a rate,
 *   when the budget lets globals win; else the rate band). The formula as
 *   typed is the cell's tooltip and its `alt` text (shown while the cell is
 *   selected, when the user wants formulas there). A formula that names a
 *   global shown by name, or has no value (a problem), shows as typed.
 *
 * `ctx`: { field, value, globals (budget.globals), info (budget.global_info),
 * prefs (budgetDisplay()), issue }. The cell's `edit` is always the text as typed.
 */
export function numberCell(src, ctx = {}) {
  const s = src === null || src === undefined ? '' : String(src);
  const t = s.trim();
  const cell = { edit: s, text: '' };
  if (ctx.issue) cell.error = ctx.issue;
  if (!t) return cell;
  const prefs = ctx.prefs || {};
  const isRate = ctx.field === 'rate';
  if (PLAIN.test(t)) {
    const plain = t.replace(/^\+/, '');
    cell.text = shownAs(plain, isRate ? ratePlaces(plain, prefs) : null);
    return cell;
  }
  cell.cls = 'formula';
  const info = ctx.info && typeof ctx.info === 'object' ? ctx.info : {};
  const names = globalsIn(t, ctx.globals);
  const byName = names.some((n) => info[n] && info[n].show_by_name === true);
  const value = ctx.value;
  if (byName || ctx.issue || typeof value !== 'string' || scaled(value) === null) {
    cell.text = s;
    return cell;
  }
  const own = names.map((n) => placesOf(info[n] ? info[n].decimals : null)).find((p) => p !== null);
  let places = own === undefined ? null : own;
  if (isRate && (own === undefined || prefs.rate_global_decimals === false)) places = ratePlaces(value, prefs);
  cell.text = shownAs(value, places);
  cell.title = `Formula: ${s}`;
  cell.alt = s;
  return cell;
}

/** A global's value for the Globals list: with its own decimals when it has them. */
export function globalValueText(value, info) {
  if (value === null || value === undefined) return '';
  return shownAs(String(value), placesOf(info ? info.decimals : null));
}

// ---------------------------------------------------------------------------
// Hidden factors (2.19)

function factorText(src, value) {
  const t = String(src === null || src === undefined ? '' : src).trim();
  if (!t) return null;
  if (PLAIN.test(t)) return t.replace(/^\+/, '');
  return typeof value === 'string' ? value : null;
}

/**
 * What hidden columns do to a line's total: ["X is 2", "Currency is EUR"].
 * A hidden X or 4X counts when it holds anything other than blank or 1
 * (formulas by their value), a hidden currency when it isn't the base one.
 * The unit columns are labels and never change a total, so they never count.
 * `shown` is the set of detail column keys on screen; `values` the row's
 * evaluated formula cells (view.account.values); `base` the base currency.
 */
export function hiddenFactors(d, values, shown, base) {
  const out = [];
  if (!d || !shown || typeof shown.has !== 'function') return out;
  const kind = d.kind || 'line';
  if (kind !== 'line' && kind !== 'adjustment') return out;
  const v = values && typeof values === 'object' ? values : {};
  for (const [field, label] of [
    ['x', 'X'],
    ['x4', '4X'],
  ]) {
    if (shown.has(field)) continue;
    const text = factorText(d[field], v[field]);
    if (text === null) continue;
    const n = scaled(text);
    if (n === null || n === ONE) continue;
    out.push(`${label} is ${decimal(text)}`);
  }
  if (kind === 'line' && !shown.has('currency') && typeof d.currency === 'string' && d.currency !== '' && d.currency !== base) {
    out.push(`Currency is ${d.currency}`);
  }
  return out;
}

// ---------------------------------------------------------------------------
// Zero rows (topsheet and account list)

/**
 * Rows without the ones marked `zero` (a category or account totalling 0,
 * with no problem and no baseline difference), keeping those in `keep` (the
 * cursor's row, so a new account doesn't vanish under the cursor).
 */
export function withoutZeroRows(rows, keep) {
  const k = keep instanceof Set ? keep : new Set(keep || []);
  return rows.filter((r) => !r.zero || k.has(r.key));
}

// ---------------------------------------------------------------------------
// Per-user choices (local storage, never the budget)

export const ZOOM_MIN = 0.7;
export const ZOOM_MAX = 1.5;
const DEFAULTS = { totalsPlaces: 2, hideZero: false, zoom: 1, formulasInSelection: true };

function clampZoom(z) {
  const n = Math.round(Number(z) * 10) / 10;
  if (!Number.isFinite(n)) return 1;
  return Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, n));
}

function normalize(p) {
  const v = p && typeof p === 'object' ? p : {};
  return {
    totalsPlaces: v.totalsPlaces === 0 ? 0 : 2,
    hideZero: v.hideZero === true,
    zoom: clampZoom(v.zoom === undefined ? 1 : v.zoom),
    formulasInSelection: v.formulasInSelection !== false,
  };
}

let user = normalize(readPref('display', DEFAULTS));

/** The per-user display choices. */
export function userDisplay() {
  return user;
}

/** Changes per-user choices and remembers them. */
export function setUserDisplay(patch) {
  user = normalize({ ...user, ...patch });
  writePref('display', user);
  return user;
}

/** Totals for display with the user's choice of decimals. */
export function totalText(cents) {
  return moneyAt(cents, user.totalsPlaces);
}

// ---------------------------------------------------------------------------
// View menu commands

/**
 * The View menu's display commands, by shortcut id. `grid` is the main
 * grid, `render` redraws the screen, `toast` says what changed.
 */
export function displayCommands({ grid, render, toast }) {
  const say = (msg) => {
    if (typeof toast === 'function') toast(msg);
  };
  const zoom = (z) => {
    const u = setUserDisplay({ zoom: z });
    grid.setZoom(u.zoom);
    say(`Grid zoom ${Math.round(u.zoom * 100)}%`);
  };
  return {
    'view.formulas': () => {
      const u = setUserDisplay({ formulasInSelection: !user.formulasInSelection });
      grid.setShowFormulas(u.formulasInSelection);
      say(u.formulasInSelection ? 'Selected cells show their formulas' : 'Selected cells show their values');
    },
    'view.hide_zero': () => {
      const u = setUserDisplay({ hideZero: !user.hideZero });
      render();
      say(u.hideZero ? 'Categories and accounts that total zero are hidden' : 'Every category and account shows');
    },
    'view.totals_cents': () => {
      const u = setUserDisplay({ totalsPlaces: user.totalsPlaces === 2 ? 0 : 2 });
      render();
      say(u.totalsPlaces ? 'Totals show cents' : 'Totals show whole amounts');
    },
    'view.zoom_in': () => zoom(user.zoom + 0.1),
    'view.zoom_out': () => zoom(user.zoom - 0.1),
    'view.zoom_reset': () => zoom(1),
  };
}
