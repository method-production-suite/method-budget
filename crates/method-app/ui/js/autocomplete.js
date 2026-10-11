// Type-ahead for the cell editor: a small list of setup names under the cell
// being edited (units, globals, currencies, sets, locations, fringes, groups).
//
// suggest() is pure: what to offer for a value and caret. Assist is the list
// itself, attached to one cell editor by quickentry.js through the grid's
// onEditStart hook; the grid hands it editor keys first.
//
// Modes:
//   whole    the cell holds one name (units, currency, set, location): match
//            the whole text; offer to add it when nothing matches exactly
//   list     a comma list (fringes, groups): complete the item at the caret
//   formula  a formula (amount, X, rate, 4X): complete the global name at the
//            caret; offer to create it when no global has that name
//
// Keys while the list shows: Up and Down move the highlight, Enter and Tab put
// the highlighted name in the cell and then do what they always do (finish the
// edit and move on), Escape closes the list (a second Escape cancels the edit).
// Left, Right, Home and End close it and move the caret. A click puts the name
// in and keeps editing.
//
// The list is a listbox; the editor is its combobox (aria-activedescendant
// points at the highlighted option), so screen readers follow the highlight
// while focus stays in the cell. Names go in with textContent. The list is
// placed by a generated rule (sheet.js), never a style attribute.

import { el } from './dom.js';
import { setRules } from './sheet.js';

// A global name: letters, digits and _, with at least one letter or _ (it
// may start with digits, 1ZG; decisions.md, entry 56).
const IDENT = /^(?=[A-Za-z0-9_]*[A-Za-z_])[A-Za-z0-9_]+$/;
const IDENT_CHAR = /[A-Za-z0-9_]/;
const MAX_ITEMS = 60;
let listCount = 0;

function lower(s) {
  return String(s).toLowerCase();
}

function byName(a, b) {
  const x = lower(a.name);
  const y = lower(b.name);
  if (x !== y) return x < y ? -1 : 1;
  return a.name < b.name ? -1 : a.name > b.name ? 1 : 0;
}

/**
 * Candidates for `query`: names (or short codes, for candidates with a
 * `code`) starting with it first, then names that contain it elsewhere
 * (`contains` false: prefix only), each group in alphabetical order ignoring
 * case, or in the candidates' own order with `ordered` (a setup table in the
 * budget's hand-made order). `skip` names are left out.
 */
export function rank(candidates, query, { contains = true, skip = null, ordered = false } = {}) {
  const q = lower(query);
  const head = [];
  const rest = [];
  for (const c of candidates) {
    if (skip && skip.has(c.name)) continue;
    const n = lower(c.name);
    const k = c.code ? lower(c.code) : '';
    if (n.startsWith(q) || (q && k && k.startsWith(q))) head.push(c);
    else if (contains && q && n.includes(q)) rest.push(c);
  }
  if (!ordered) {
    head.sort(byName);
    rest.sort(byName);
  }
  return [...head, ...rest];
}

/** The name in `names` equal to `typed` ignoring case: the exact one when there is one. */
export function canonicalName(names, typed) {
  const t = String(typed).trim();
  if (!t) return null;
  let found = null;
  for (const n of names) {
    if (n === t) return n;
    if (found === null && lower(n) === lower(t)) found = n;
  }
  return found;
}

/** The identifier around `caret` in a formula: { from, to, token }, or null. */
export function formulaToken(value, caret) {
  let from = caret;
  while (from > 0 && IDENT_CHAR.test(value[from - 1])) from -= 1;
  let to = caret;
  while (to < value.length && IDENT_CHAR.test(value[to])) to += 1;
  const token = value.slice(from, to);
  // A number followed by letters (1e3, 2DAYS) is not a name.
  if (!IDENT.test(token)) return null;
  if (from > 0 && /[0-9.]/.test(value[from - 1])) return null;
  return { from, to, token };
}

/** The comma-list item around `caret`: { from, to, token } (token trimmed). */
export function listToken(value, caret) {
  const from = value.lastIndexOf(',', caret - 1) + 1;
  let to = value.indexOf(',', caret);
  if (to < 0) to = value.length;
  return { from, to, token: value.slice(from, to).trim() };
}

/**
 * What to offer for `value` with the caret at `caret`.
 *   config: { mode, candidates: [{ name, hint }], addKind, addLabel }
 * Returns { items, active, from, to, token } or null for no list. Items are
 *   { kind: 'match', name, hint } | { kind: 'add', name, label } | { kind: 'create', name, label }
 * `active` is the item highlighted at first (-1 for none).
 */
export function suggest(config, value, caret) {
  const v = String(value);
  const c = Math.max(0, Math.min(caret, v.length));
  const candidates = config.candidates || [];
  let span;
  let matches;
  if (config.mode === 'formula') {
    span = formulaToken(v, c);
    if (!span) return null;
    matches = rank(candidates, span.token, { contains: false, ordered: Boolean(config.ordered) });
  } else if (config.mode === 'list') {
    span = listToken(v, c);
    const others = new Set(
      v
        .split(',')
        .map((x) => x.trim())
        .filter((x) => x && x !== span.token),
    );
    matches = rank(candidates, span.token, { skip: others, ordered: Boolean(config.ordered) });
    if (!span.token && !matches.length) return null;
  } else {
    span = { from: 0, to: v.length, token: v.trim() };
    if (!span.token) return null;
    matches = rank(candidates, span.token, { ordered: Boolean(config.ordered) });
  }
  const token = span.token;
  const exact = candidates.some((x) => lower(x.name) === lower(token));
  const items = matches.slice(0, MAX_ITEMS).map((m) => ({ kind: 'match', name: m.name, hint: m.hint || '' }));
  let extra = null;
  if (config.addKind && token && !exact && !token.includes(',')) {
    if (config.mode === 'formula') {
      extra = { kind: 'create', name: token, label: `Create global “${token}”` };
    } else {
      extra = { kind: 'add', name: token, label: `Add ${config.addLabel || config.addKind} “${token}”` };
    }
  }
  if (extra) items.push(extra);
  // Nothing to offer, or just what is already there.
  if (!items.length) return null;
  if (items.length === 1 && items[0].kind === 'match' && items[0].name === token) return null;
  // Highlight the first match. With no match, highlight "Add" (committing adds
  // it anyway) but not "Create global": Enter then keeps the formula as typed.
  let active = items[0].kind === 'match' ? 0 : -1;
  if (active < 0 && extra && extra.kind === 'add') active = items.indexOf(extra);
  return { items, active, from: span.from, to: span.to, token };
}

/**
 * The cell text after choosing `item`: { value, caret }. Whole cells become
 * the name; a list item or formula name is replaced in place.
 */
export function applyChoice(config, value, s, item) {
  const v = String(value);
  if (config.mode === 'whole') return { value: item.name, caret: item.name.length };
  if (config.mode === 'list') {
    const before = v.slice(0, s.from);
    const after = v.slice(s.to);
    const lead = s.from > 0 ? ' ' : '';
    const text = before + lead + item.name;
    return { value: text + after, caret: text.length };
  }
  const text = v.slice(0, s.from) + item.name;
  return { value: text + v.slice(s.to), caret: text.length };
}

/**
 * The list on one cell editor.
 *   new Assist({ input, anchor, host, config, onCreate })
 *   anchor(): the cell element to sit under (null when it's gone)
 *   onCreate(item): "Create global" was chosen; the caller takes over the edit
 * key(e) is the grid's hook: true when the list used the key.
 */
export class Assist {
  constructor({ input, anchor, host, config, onCreate }) {
    this.input = input;
    this.anchor = anchor;
    this.host = host;
    this.config = config;
    this.onCreate = onCreate;
    this.state = null; // suggest() result while open
    listCount += 1;
    this.id = `ac-list-${listCount}`;
    this.root = el('div', {
      class: 'ac-list',
      attrs: { id: this.id, role: 'listbox', 'aria-label': config.label || 'Suggestions' },
    });
    // Keep focus in the cell editor: a press on the list must not blur it.
    this.root.addEventListener('mousedown', (e) => {
      e.preventDefault();
      const opt = e.target.closest('.ac-item');
      if (opt) this.choose(Number(opt.dataset.index), { viaMouse: true });
    });
    input.setAttribute('role', 'combobox');
    input.setAttribute('aria-autocomplete', 'list');
    input.setAttribute('aria-expanded', 'false');
    input.setAttribute('aria-controls', this.id);
    this.onInput = () => this.refresh();
    input.addEventListener('input', this.onInput);
    this.onScroll = () => this.place();
    if (host) host.addEventListener('scroll', this.onScroll);
    this.onResize = () => this.hide();
    window.addEventListener('resize', this.onResize);
  }

  get open() {
    return this.state !== null;
  }

  /** Works out the list again from the editor's text and caret. */
  refresh() {
    const caret = this.input.selectionStart === null ? this.input.value.length : this.input.selectionStart;
    const next = suggest(this.config, this.input.value, caret);
    if (!next) {
      this.hide();
      return;
    }
    this.state = next;
    this.draw();
    if (!this.root.isConnected) document.body.append(this.root);
    this.input.setAttribute('aria-expanded', 'true');
    this.place();
  }

  draw() {
    const { items, active } = this.state;
    const nodes = items.map((it, i) => {
      const cls = `ac-item ac-${it.kind}${i === active ? ' active' : ''}`;
      const node = el('div', {
        class: cls,
        attrs: { id: `${this.id}-${i}`, role: 'option', 'aria-selected': String(i === active) },
        data: { index: i },
      });
      if (it.kind === 'match') {
        node.append(el('span', { class: 'ac-name', text: it.name }));
        if (it.hint) node.append(el('span', { class: 'ac-hint', text: it.hint }));
      } else {
        node.append(el('span', { class: 'ac-name', text: it.label }));
      }
      return node;
    });
    this.root.replaceChildren(...nodes);
    this.markActive();
  }

  markActive() {
    const { active } = this.state;
    const items = this.root.children;
    for (let i = 0; i < items.length; i++) {
      const on = i === active;
      items[i].classList.toggle('active', on);
      items[i].setAttribute('aria-selected', String(on));
    }
    if (active >= 0 && items[active]) {
      this.input.setAttribute('aria-activedescendant', items[active].id);
      items[active].scrollIntoView({ block: 'nearest' });
    } else {
      this.input.removeAttribute('aria-activedescendant');
    }
  }

  /** Puts the list under the cell (above it when there's no room below). */
  place() {
    if (!this.open) return;
    const td = this.anchor();
    if (!td || !td.isConnected) {
      this.hide();
      return;
    }
    const r = td.getBoundingClientRect();
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    const width = Math.round(Math.max(r.width, 200));
    const left = Math.round(Math.max(8, Math.min(r.left, vw - width - 8)));
    const below = vh - r.bottom - 12;
    const above = r.top - 12;
    const up = below < 140 && above > below;
    const maxH = Math.round(Math.max(80, Math.min(280, up ? above : below)));
    const pos = up ? `bottom: ${Math.round(vh - r.top + 2)}px;` : `top: ${Math.round(r.bottom + 2)}px;`;
    setRules('ac-list', `.ac-list.placed { left: ${left}px; ${pos} min-width: ${width}px; max-height: ${maxH}px; }`);
    this.root.classList.add('placed');
  }

  hide() {
    this.state = null;
    this.root.remove();
    this.root.classList.remove('placed');
    this.input.setAttribute('aria-expanded', 'false');
    this.input.removeAttribute('aria-activedescendant');
  }

  /** The editor closed: take the list and listeners away. */
  close() {
    this.hide();
    this.input.removeEventListener('input', this.onInput);
    if (this.host) this.host.removeEventListener('scroll', this.onScroll);
    window.removeEventListener('resize', this.onResize);
  }

  move(d) {
    const n = this.state.items.length;
    const a = this.state.active;
    this.state.active = a < 0 ? (d > 0 ? 0 : n - 1) : (a + d + n) % n;
    this.markActive();
  }

  /**
   * Puts item `i` in the cell. Returns true when the caller should stop
   * (the item started its own flow), false to go on with the key.
   */
  choose(i, { viaMouse = false } = {}) {
    const st = this.state;
    const item = st && st.items[i];
    if (!item) return false;
    if (item.kind === 'create') {
      this.hide();
      if (this.onCreate) this.onCreate(item);
      return true;
    }
    const out = applyChoice(this.config, this.input.value, st, item);
    this.input.value = out.value;
    try {
      this.input.setSelectionRange(out.caret, out.caret);
    } catch {
      // Not every input supports selection ranges.
    }
    // The grid keeps the caret across redraws from these events.
    this.input.dispatchEvent(new KeyboardEvent('keyup'));
    this.hide();
    if (viaMouse) this.input.focus();
    return false;
  }

  /** The grid's editor-key hook. */
  key(e) {
    if (!this.open) return false;
    switch (e.key) {
      case 'ArrowDown':
        if (e.metaKey || e.ctrlKey || e.altKey || e.shiftKey) return false;
        this.move(1);
        return true;
      case 'ArrowUp':
        if (e.metaKey || e.ctrlKey || e.altKey || e.shiftKey) return false;
        this.move(-1);
        return true;
      case 'Enter':
      case 'Tab': {
        if (e.isComposing) return false;
        // Ctrl+Tab steps budget tabs (main.js), not through the list.
        if (e.key === 'Tab' && (e.ctrlKey || e.metaKey)) return false;
        const a = this.state.active;
        if (a < 0) {
          this.hide();
          return false;
        }
        return this.choose(a);
      }
      case 'Escape':
        this.hide();
        return true;
      case 'ArrowLeft':
      case 'ArrowRight':
      case 'Home':
      case 'End':
        this.hide();
        return false;
      default:
        return false;
    }
  }
}
