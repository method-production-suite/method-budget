// The tools panel's Find tab: find and replace across the budget (src/find.rs).
//
// A search looks for one kind of thing: description text, any formula's
// text, a global used in formulas (as a whole name, never part of another),
// a fringe, group, set, location, unit or currency on lines, note text, or an
// account's name or number. Text searches have Match case and Whole word.
// Setup names (and globals) are offered as you type. Results come grouped by
// category and account, each line with the column that matched; a click goes
// there (the account opens with that cell selected), and Enter and
// Shift+Enter in the search box, or F3 and Shift+F3 anywhere, step to the
// next and previous result. Results are searched again after every change to
// the budget.
//
// Replace mode (Cmd/Ctrl+Shift+F, or the Replace switch) adds a "Replace
// with" box and a checkbox on every result, all checked to begin with:
// Replace changes the checked results only, as one undo step, and says how
// many places changed. A global is replaced by another global, a number or a
// formula, as a whole name inside the formula; an item by another one.
// Remove takes an item off the checked lines, or puts a global's current
// value in its place. Rename everywhere gives the global or item itself a
// new name, with every reference to it (rename.js), and the search follows.
//
// tools.js calls renderFind(host, view, ctx) on every render; ctx.call runs a
// command and redraws from its view, ctx.jumpTo(entry) goes to a result.
// Budget text only reaches the page through textContent and input values.

import { invoke } from './api.js';
import { el } from './dom.js';
import { t } from './i18n.js';
import { rank, canonicalName } from './autocomplete.js';
import { withShortcut } from './shortcuts.js';
import { renameItem } from './rename.js';
import { obj, str } from './util.js';

const SCOPES = [
  { id: 'description', label: t('find.description'), kind: 'text', hint: t('find.text_in_line_descriptions') },
  { id: 'formula', label: t('find.formula_text'), kind: 'text', hint: t('find.text_in_amount_x_rate') },
  { id: 'global', label: t('find.global_in_formulas'), kind: 'global', hint: t('find.a_global_by_its_whole') },
  { id: 'fringe', label: t('find.fringe'), kind: 'item', table: 'fringes' },
  { id: 'group', label: t('find.group'), kind: 'item', table: 'groups' },
  { id: 'set', label: t('find.set'), kind: 'item', table: 'sets' },
  { id: 'location', label: t('find.location'), kind: 'item', table: 'locations' },
  { id: 'unit', label: t('find.unit'), kind: 'item', table: 'units' },
  { id: 'currency', label: t('find.currency'), kind: 'item', table: 'currencies' },
  { id: 'note', label: t('find.notes'), kind: 'text', hint: t('find.text_in_line_notes') },
  { id: 'account', label: t('find.account_name_or_number'), kind: 'text', hint: t('find.account_names_and_numbers') },
];

const FIELD_LABELS = {
  description: t('find.field.description'),
  amount: t('find.field.amount'),
  x: t('find.field.x'),
  rate: t('find.field.rate'),
  x4: t('find.field.x4'),
  unit: t('find.field.unit'),
  unit2: t('find.field.unit2'),
  unit3: t('find.field.unit3'),
  unit4: t('find.field.unit4'),
  fringes: t('find.field.fringes'),
  groups: t('find.field.groups'),
  set: t('find.field.set'),
  location: t('find.field.location'),
  currency: t('find.field.currency'),
  note: t('find.field.note'),
  code: t('find.field.code'),
  name: t('find.field.name'),
};

const SEARCH_DELAY_MS = 180;
const MAX_SUGGESTIONS = 8;

const st = {
  host: null,
  view: null,
  ctx: null,
  scope: 'description',
  text: '',
  matchCase: false,
  wholeWord: false,
  replace: false,
  withText: '',
  found: null, // { count, hits, truncated }
  foundSig: '', // the query the results are for
  foundRev: null, // and the budget revision
  foundDoc: null,
  error: '',
  message: '', // "N updated", until the budget changes again
  messageRev: null,
  unchecked: new Set(), // result keys the user unchecked
  current: null, // key of the result last gone to
  currentIndex: -1,
  busy: false,
  suggest: null, // { field: 'text' | 'with', items, active }
  timer: null,
  seq: 0,
  cache: null, // { sig, node } the drawn results, reused while nothing changed
  drawing: false, // true while the tab is redrawn (a removed box may fire blur)
};

/** Draws the Find tab into `host`. */
export function renderFind(host, view, ctx) {
  st.host = host;
  st.view = view && typeof view === 'object' ? view : {};
  st.ctx = ctx || {};
  if (st.foundDoc !== null && st.view.doc !== st.foundDoc) {
    // Another budget: its own results.
    st.current = null;
    st.currentIndex = -1;
    st.unchecked.clear();
    st.message = '';
  }
  if (st.message && st.messageRev !== null && st.view.rev !== st.messageRev) st.message = '';
  draw();
  if (needsSearch()) search();
}

/**
 * Shows find (or replace) mode and puts focus in the search box. Called
 * after the panel shows the Find tab (Cmd/Ctrl+F, Cmd/Ctrl+Shift+F).
 */
export function openFind({ replace = false } = {}) {
  if (replace !== st.replace) {
    st.replace = replace;
    st.message = '';
  }
  if (st.host && st.host.isConnected) draw();
  focusKey('find:text', { select: true });
}

/**
 * Searches for the uses of setup item `name` of setup table `table`
 * (`fringes`, `globals`...): the setup lists' line counts call it once the
 * panel shows the Find tab. Find mode, focus in the search box.
 */
export function findFor(table, name) {
  const scope = SCOPES.find((x) => x.table === table) || (table === 'globals' ? SCOPES.find((x) => x.id === 'global') : null);
  if (!scope) return;
  st.scope = scope.id;
  st.text = String(name);
  if (st.replace) {
    st.replace = false;
    st.message = '';
  }
  st.unchecked.clear();
  st.current = null;
  st.currentIndex = -1;
  if (st.host && st.host.isConnected) {
    draw();
    search();
  }
  focusKey('find:text', { select: true });
}

/**
 * Goes to the next (1) or previous (-1) result (F3, Shift+F3), searching
 * again first. False when there is nothing to search for.
 */
export function findStep(delta) {
  if (!query()) return false;
  step(delta, { keepFocus: false, force: true });
  return true;
}

// ---------------------------------------------------------------------------
// Small helpers

function scopeInfo(id = st.scope) {
  return SCOPES.find((s) => s.id === id) || SCOPES[0];
}

function budget() {
  return obj(st.view && st.view.budget);
}

/** Names offered for a scope: setup items, or globals. */
function names(scope = scopeInfo()) {
  if (scope.kind === 'global') return Object.keys(obj(budget().globals));
  if (scope.kind !== 'item') return [];
  const list = Object.keys(obj(budget()[scope.table]));
  const base = str(budget().base_currency);
  if (scope.id === 'currency' && base && !list.includes(base)) list.push(base);
  return list;
}

function hint(scope, name) {
  if (scope.kind === 'global') {
    const values = obj(st.view && st.view.report && st.view.report.globals);
    return values[name] !== undefined ? str(values[name]) : str(obj(budget().globals)[name]);
  }
  if (scope.id === 'currency' && name === str(budget().base_currency)) return t('find.base_currency');
  return str(obj(obj(budget()[scope.table])[name]).description);
}

/** What the search box asks for, as the find command takes it, or null. */
function query() {
  const scope = scopeInfo();
  let text = st.text;
  if (scope.kind !== 'text') {
    text = text.trim();
    if (!text) return null;
    text = canonicalName(names(scope), text) || text;
  } else if (!text) {
    return null;
  }
  return {
    scope: scope.id,
    text,
    match_case: scope.kind === 'text' && st.matchCase,
    whole_word: scope.kind === 'text' && st.wholeWord,
  };
}

function keyOf(h) {
  return h.row_id === null || h.row_id === undefined ? `a:${h.account}:${h.field}` : `r:${h.row_id}:${h.field}`;
}

/** Results Replace can act on (an account's number is found, not replaced). */
function replaceable(h) {
  return h.field !== 'code';
}

function needsSearch() {
  const q = query();
  const sig = q ? JSON.stringify(q) : '';
  if (!sig) return Boolean(st.found);
  return sig !== st.foundSig || st.foundRev !== st.view.rev;
}

// ---------------------------------------------------------------------------
// Searching

async function search() {
  clearTimeout(st.timer);
  st.timer = null;
  const q = query();
  const my = ++st.seq;
  if (!q) {
    st.found = null;
    st.foundSig = '';
    st.error = '';
    draw();
    return;
  }
  const sig = JSON.stringify(q);
  const rev = st.view.rev;
  const doc = st.view.doc;
  if (sig !== st.foundSig) {
    st.unchecked.clear();
    st.current = null;
    st.currentIndex = -1;
  }
  try {
    const found = await invoke('find', { query: q });
    if (my !== st.seq) return;
    st.found = found && typeof found === 'object' && Array.isArray(found.hits) ? found : { count: 0, hits: [], truncated: false };
    st.error = '';
  } catch (e) {
    if (my !== st.seq) return;
    st.found = null;
    st.error = String(e);
  }
  st.foundSig = sig;
  st.foundRev = rev;
  st.foundDoc = doc;
  if (st.host && st.host.isConnected) draw();
}

function searchSoon() {
  clearTimeout(st.timer);
  st.timer = setTimeout(search, SEARCH_DELAY_MS);
}

/** Results for the current query and budget, searched again if stale. */
async function fresh() {
  if (st.timer || needsSearch()) await search();
  return st.found && Array.isArray(st.found.hits) ? st.found.hits : [];
}

async function step(delta, { keepFocus, force = false }) {
  if (force) await search();
  const hits = await fresh();
  if (!hits.length) return;
  let i = st.current ? hits.findIndex((h) => keyOf(h) === st.current) : -1;
  let next;
  if (i >= 0) next = (i + delta + hits.length) % hits.length;
  else if (st.currentIndex >= 0) {
    // The one we were on is gone (replaced): carry on from its place.
    i = Math.min(st.currentIndex, hits.length);
    next = delta > 0 ? i % hits.length : (i - 1 + hits.length) % hits.length;
  } else next = delta > 0 ? 0 : hits.length - 1;
  await goTo(hits[next], next, { keepFocus });
}

async function goTo(h, index, { keepFocus }) {
  st.current = keyOf(h);
  st.currentIndex = index;
  draw();
  if (typeof st.ctx.jumpTo !== 'function') return;
  await st.ctx.jumpTo({
    row_id: h.row_id,
    ti: h.ti,
    ai: h.ai,
    category: h.category,
    account: h.account,
    field: h.field,
  });
  if (keepFocus) focusKey('find:text');
}

// ---------------------------------------------------------------------------
// Replacing

function checkedHits() {
  const hits = st.found && Array.isArray(st.found.hits) ? st.found.hits : [];
  return hits.filter((h) => replaceable(h) && !st.unchecked.has(keyOf(h)));
}

async function doReplace(remove) {
  const scope = scopeInfo();
  const targets = checkedHits().map((h) => ({ row_id: h.row_id, account: h.row_id === null || h.row_id === undefined ? h.account : null, field: h.field }));
  if (!targets.length || st.busy || typeof st.ctx.call !== 'function') return;
  const q = query();
  if (!q) return;
  let withValue = null;
  if (!remove) {
    withValue = st.withText;
    if (scope.kind !== 'text') {
      withValue = withValue.trim();
      if (!withValue) return;
      withValue = canonicalName(withNames(scope), withValue) || withValue;
    }
  }
  st.busy = true;
  st.error = '';
  st.message = '';
  draw();
  try {
    const v = await st.ctx.call('replace', { query: q, targets, with: withValue });
    const n = v && Number.isSafeInteger(v.updated) ? v.updated : targets.length;
    st.message = t('find.updated', { count: n });
    st.messageRev = v && v.rev !== undefined ? v.rev : null;
    st.current = null;
    st.currentIndex = -1;
  } catch (e) {
    st.error = String(e);
  } finally {
    st.busy = false;
  }
  // The view that came back redrew the panel and searched again; make sure.
  if (needsSearch()) await search();
  else draw();
}

/** Names the Replace box offers: the same kind as what was found, but it. */
function withNames(scope = scopeInfo()) {
  const found = query();
  return names(scope).filter((n) => !found || n !== found.text);
}

// ---------------------------------------------------------------------------
// Suggestions (setup names, globals)

function suggestFor(field) {
  const scope = scopeInfo();
  if (scope.kind === 'text') return [];
  const typed = (field === 'text' ? st.text : st.withText).trim();
  // A formula in the Replace box: only offer while it is one name.
  if (scope.kind === 'global' && typed && !/^(?=[A-Za-z0-9_]*[A-Za-z_])[A-Za-z0-9_]+$/.test(typed)) return [];
  const pool = (field === 'text' ? names(scope) : withNames(scope)).map((name) => ({ name }));
  const ranked = rank(pool, typed);
  if (ranked.length === 1 && ranked[0].name === typed) return [];
  return ranked.slice(0, MAX_SUGGESTIONS).map((c) => ({ name: c.name, hint: hint(scope, c.name) }));
}

function openSuggest(field) {
  const items = suggestFor(field);
  st.suggest = items.length ? { field, items, active: -1 } : null;
}

function pickSuggestion(i) {
  const s = st.suggest;
  if (!s || !s.items[i]) return;
  if (s.field === 'text') {
    st.text = s.items[i].name;
    st.suggest = null;
    st.message = '';
    search();
    draw();
    focusKey('find:text', { end: true });
  } else {
    st.withText = s.items[i].name;
    st.suggest = null;
    draw();
    focusKey('find:with', { end: true });
  }
}

/** Keys for a box with suggestions; true when handled. */
function suggestKey(e, field) {
  const s = st.suggest && st.suggest.field === field ? st.suggest : null;
  if (!s) {
    if (e.key === 'ArrowDown' && !e.altKey && !e.metaKey && !e.ctrlKey && scopeInfo().kind !== 'text') {
      openSuggest(field);
      if (st.suggest) {
        st.suggest.active = 0;
        draw();
        return true;
      }
    }
    return false;
  }
  if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
    const n = s.items.length;
    s.active = e.key === 'ArrowDown' ? (s.active + 1) % n : (s.active - 1 + n) % n;
    draw();
    return true;
  }
  if ((e.key === 'Enter' || e.key === 'Tab') && s.active >= 0 && !e.shiftKey) {
    pickSuggestion(s.active);
    return true;
  }
  // Enter on a partial name takes the first name offered.
  const typed = (field === 'text' ? st.text : st.withText).trim();
  const pool = field === 'text' ? names() : withNames();
  if (e.key === 'Enter' && !e.shiftKey && canonicalName(pool, typed) === null) {
    pickSuggestion(0);
    return true;
  }
  if (e.key === 'Escape') {
    st.suggest = null;
    draw();
    return true;
  }
  return false;
}

// ---------------------------------------------------------------------------
// Drawing

function captureFocus() {
  const a = document.activeElement;
  if (!a || !st.host || !st.host.contains(a) || !a.dataset || !a.dataset.key) return null;
  const info = { key: a.dataset.key };
  if (typeof a.selectionStart === 'number') {
    info.start = a.selectionStart;
    info.end = a.selectionEnd;
  }
  return info;
}

function byKey(key) {
  if (!st.host) return null;
  return [...st.host.querySelectorAll('[data-key]')].find((e) => e.dataset.key === key) || null;
}

function restoreFocus(info) {
  if (!info) return;
  const t = byKey(info.key);
  if (!t) return;
  t.focus({ preventScroll: true });
  if (info.start !== undefined && typeof t.setSelectionRange === 'function') {
    try {
      t.setSelectionRange(info.start, info.end);
    } catch {
      // Not a text control.
    }
  }
}

function focusKey(key, { select = false, end = false } = {}) {
  const t = byKey(key);
  if (!t) return;
  t.focus();
  if (typeof t.setSelectionRange === 'function') {
    try {
      if (select) t.select();
      else if (end) t.setSelectionRange(t.value.length, t.value.length);
    } catch {
      // Not a text control.
    }
  }
}

function draw() {
  const host = st.host;
  if (!host) return;
  const focus = captureFocus();
  st.drawing = true;
  try {
    host.replaceChildren(form(), status(), results());
  } finally {
    st.drawing = false;
  }
  restoreFocus(focus);
}

/** Opens or updates the suggestions for a box; redraws only when they changed. */
function refreshSuggest(field) {
  const before = JSON.stringify(st.suggest);
  openSuggest(field);
  if (JSON.stringify(st.suggest) !== before) draw();
}

/** Closes a box's suggestions when it loses focus (not when it is redrawn). */
function closeSuggestOnBlur(field) {
  if (st.drawing || !st.suggest || st.suggest.field !== field) return;
  st.suggest = null;
  setTimeout(() => {
    if (st.host && st.host.isConnected) draw();
  }, 0);
}

function setDisabled(b, off) {
  if (off) b.setAttribute('aria-disabled', 'true');
  else b.removeAttribute('aria-disabled');
}

function button(text, key, { primary = false, title = '' } = {}) {
  return el('button', {
    class: `setup-btn${primary ? ' setup-btn-primary' : ''}`,
    text,
    title,
    attrs: { type: 'button' },
    data: { key, nobusy: '1' },
  });
}

function checkbox(label, key, checked, onChange) {
  const box = el('input', { attrs: { type: 'checkbox' }, data: { key } });
  box.checked = checked;
  box.addEventListener('change', () => onChange(box.checked));
  return el('label', { class: 'find-option' }, box, label);
}

function suggestionList(field, input) {
  const s = st.suggest && st.suggest.field === field ? st.suggest : null;
  const id = `find-suggest-${field}`;
  input.setAttribute('role', 'combobox');
  input.setAttribute('aria-autocomplete', 'list');
  input.setAttribute('aria-expanded', String(Boolean(s)));
  input.setAttribute('aria-controls', id);
  if (!s) return null;
  if (s.active >= 0) input.setAttribute('aria-activedescendant', `${id}-${s.active}`);
  const ul = el('ul', { class: 'find-suggest', attrs: { role: 'listbox', id, 'aria-label': field === 'text' ? t('find.suggestions') : t('find.replace_suggestions') } });
  s.items.forEach((item, i) => {
    const li = el(
      'li',
      {
        class: `find-suggest-item${i === s.active ? ' is-active' : ''}`,
        attrs: { role: 'option', id: `${id}-${i}`, 'aria-selected': String(i === s.active) },
      },
      el('span', { class: 'find-suggest-name', text: item.name }),
      item.hint ? el('span', { class: 'find-suggest-hint', text: item.hint }) : null,
    );
    // Keep focus in the box: choose on mousedown.
    li.addEventListener('mousedown', (e) => {
      e.preventDefault();
      pickSuggestion(i);
    });
    ul.append(li);
  });
  return ul;
}

function form() {
  const scope = scopeInfo();
  const wrap = el('div', { class: 'find-form', attrs: { role: 'search', 'aria-label': t('find.find_in_the_budget') } });

  // What to look for.
  const select = el('select', {
    class: 'setup-input setup-select find-scope',
    attrs: { 'aria-label': t('find.find_what') },
    data: { key: 'find:scope' },
  });
  for (const s of SCOPES) {
    const o = el('option', { text: s.label, attrs: { value: s.id } });
    if (s.id === scope.id) o.selected = true;
    select.append(o);
  }
  select.addEventListener('change', () => {
    st.scope = scopeInfo(select.value).id;
    st.suggest = null;
    st.message = '';
    st.withText = '';
    search();
  });

  const placeholder = scope.kind === 'text' ? t('find.find_placeholder', { what: scope.hint.toLowerCase() }) : scope.kind === 'global' ? t('find.global_name') : t('find.item_name', { kind: scope.label });
  const input = el('input', {
    class: 'setup-input find-text',
    attrs: {
      type: 'search',
      placeholder,
      'aria-label': t('find.find'),
      spellcheck: 'false',
      autocomplete: 'off',
      'aria-describedby': 'find-keys',
      'aria-keyshortcuts': 'Enter Shift+Enter',
    },
    data: { key: 'find:text' },
  });
  input.value = st.text;
  input.addEventListener('input', (e) => {
    st.text = input.value;
    if (e.isComposing) return; // compositionend follows with the text
    if (st.message) {
      st.message = '';
      draw();
    }
    refreshSuggest('text');
    searchSoon();
  });
  input.addEventListener('compositionend', () => {
    st.text = input.value;
    refreshSuggest('text');
    searchSoon();
  });
  input.addEventListener('keydown', (e) => {
    if (e.isComposing) return;
    if (suggestKey(e, 'text')) {
      e.preventDefault();
      e.stopPropagation();
      return;
    }
    if (e.key === 'Enter' && !e.altKey && !e.metaKey && !e.ctrlKey) {
      e.preventDefault();
      e.stopPropagation();
      step(e.shiftKey ? -1 : 1, { keepFocus: true });
    }
  });
  input.addEventListener('blur', () => closeSuggestOnBlur('text'));

  const prev = button('‹', 'find:prev', { title: t('find.previous_result_shift_enter') });
  prev.setAttribute('aria-label', t('find.previous_result'));
  const next = button('›', 'find:next', { title: t('find.next_result_enter') });
  next.setAttribute('aria-label', t('find.next_result'));
  const none = !(st.found && st.found.hits.length);
  setDisabled(prev, none);
  setDisabled(next, none);
  prev.addEventListener('click', () => step(-1, { keepFocus: false }));
  next.addEventListener('click', () => step(1, { keepFocus: false }));

  const mode = el('button', {
    class: 'setup-btn setup-btn-quiet find-mode',
    text: st.replace ? t('find.hide_replace') : t('find.replace_open'),
    title: withShortcut(t('find.show_or_hide_replace'), 'edit.replace'),
    attrs: { type: 'button', 'aria-pressed': String(st.replace) },
    data: { key: 'find:mode', nobusy: '1' },
  });
  mode.addEventListener('click', () => {
    st.replace = !st.replace;
    st.message = '';
    draw();
  });

  wrap.append(el('div', { class: 'find-row' }, select, mode), el('div', { class: 'find-row find-combo' }, input, prev, next));
  const list = suggestionList('text', input);
  if (list) wrap.append(list);

  if (scope.kind === 'text') {
    wrap.append(
      el(
        'div',
        { class: 'find-row find-options' },
        checkbox(t('find.match_case'), 'find:case', st.matchCase, (on) => {
          st.matchCase = on;
          search();
        }),
        checkbox(t('find.whole_word'), 'find:word', st.wholeWord, (on) => {
          st.wholeWord = on;
          search();
        }),
      ),
    );
  }

  if (st.replace) wrap.append(replaceForm(scope));
  wrap.append(el('p', { class: 'apply-hint find-keys', text: t('find.enter_shift_enter_next_previous'), attrs: { id: 'find-keys' } }));
  return wrap;
}

function replaceForm(scope) {
  const sec = el('div', { class: 'find-replace' });
  const placeholder =
    scope.kind === 'text'
      ? t('find.replace_with_text')
      : scope.kind === 'global'
        ? t('find.replace_with_global')
        : t('find.replace_with_item', { kind: scope.label.toLowerCase() });
  const input = el('input', {
    class: 'setup-input find-with',
    attrs: { type: 'text', placeholder, 'aria-label': t('find.replace_with'), spellcheck: 'false', autocomplete: 'off' },
    data: { key: 'find:with' },
  });
  input.value = st.withText;
  const checked = checkedHits().length;
  const go = button(t('find.replace'), 'find:replace', { primary: true });
  const canReplace = () => checked > 0 && !st.busy && (scope.kind === 'text' || st.withText.trim() !== '');
  setDisabled(go, !canReplace());
  go.title = checked ? t('find.replace_in_checked', { count: checked }) : t('find.check_first');
  go.addEventListener('click', () => {
    if (go.getAttribute('aria-disabled') !== 'true') doReplace(false);
  });
  input.addEventListener('input', (e) => {
    st.withText = input.value;
    if (e.isComposing) return;
    // The Replace button follows what is typed.
    setDisabled(go, !canReplace());
    refreshSuggest('with');
  });
  input.addEventListener('keydown', (e) => {
    if (e.isComposing) return;
    if (suggestKey(e, 'with')) {
      e.preventDefault();
      e.stopPropagation();
    }
  });
  input.addEventListener('blur', () => closeSuggestOnBlur('with'));
  const row = el('div', { class: 'find-row find-combo' }, input, go);
  if (scope.kind !== 'text') {
    const remove = button(t('find.remove'), 'find:remove', {
      title:
        scope.kind === 'global'
          ? t('find.remove_global_tip')
          : t('find.remove_item_tip', { kind: scope.label.toLowerCase() }),
    });
    setDisabled(remove, !(checked > 0 && !st.busy));
    remove.addEventListener('click', () => {
      if (remove.getAttribute('aria-disabled') !== 'true') doReplace(true);
    });
    row.append(remove);
  }
  sec.append(row);
  const list = suggestionList('with', input);
  if (list) sec.append(list);
  const link = renameLink(scope);
  if (link) sec.append(link);
  return sec;
}

/**
 * "Rename everywhere…" for the global or item being searched for (when it
 * exists): replacing swaps it for another one in the checked places, this
 * gives it a new name everywhere.
 */
function renameLink(scope) {
  if (scope.kind === 'text') return null;
  const q = query();
  if (!q) return null;
  const name = q.text;
  const table = scope.kind === 'global' ? 'globals' : scope.table;
  const isBase = scope.id === 'currency' && name === str(budget().base_currency);
  if (!isBase && !Object.prototype.hasOwnProperty.call(obj(budget()[table]), name)) return null;
  const b = button(t('find.rename_everywhere'), 'find:rename', {
    title: t('find.give_a_new_name_with', { name }),
  });
  b.addEventListener('click', () => {
    if (b.getAttribute('aria-disabled') === 'true' || !st.ctx || typeof st.ctx.call !== 'function') return;
    renameItem({
      anchor: b,
      kind: scope.id,
      name,
      ctx: { call: (cmd, args) => st.ctx.call(cmd, args), toast: st.ctx.toast, budget },
    }).then((to) => {
      if (to === null) return;
      // Keep looking for it, under its new name.
      st.text = to;
      searchSoon();
      draw();
    });
  });
  setDisabled(b, st.busy);
  return el('div', { class: 'find-row find-rename' }, b);
}

function status() {
  const box = el('div', { class: 'find-status', attrs: { role: 'status', 'aria-live': 'polite' } });
  if (st.message) box.append(el('p', { class: 'find-message', text: st.message }));
  if (st.error) {
    box.append(el('p', { class: 'setup-error', text: st.error, attrs: { role: 'alert' } }));
    return box;
  }
  const f = st.found;
  if (!query()) {
    box.append(el('p', { class: 'apply-hint', text: scopeInfo().kind === 'text' ? t('find.type_to_search') : t('find.type_or_pick') }));
  } else if (!f) {
    box.append(el('p', { class: 'apply-hint', text: t('find.searching') }));
  } else if (!f.count) {
    box.append(el('p', { class: 'apply-hint', text: t('find.nothing_found') }));
  } else {
    const accounts = new Set(f.hits.map((h) => h.account)).size;
    let text = t('find.found_in_accounts', { found: f.count, count: accounts });
    if (f.truncated) text = t('find.found_showing_first', { found: f.count, shown: f.hits.length });
    box.append(el('p', { class: 'find-count', text }));
  }
  return box;
}

function resultsSig() {
  return JSON.stringify([st.foundSig, st.foundRev, st.foundDoc, st.replace, st.current, [...st.unchecked].sort(), Boolean(st.found)]);
}

function results() {
  const f = st.found;
  if (!f || !f.hits.length || st.error) return el('div', { class: 'find-results' });
  const sig = resultsSig();
  if (st.cache && st.cache.sig === sig) return st.cache.node;
  const node = buildResults(f.hits);
  st.cache = { sig, node };
  return node;
}

function setChecked(keys, on) {
  for (const k of keys) {
    if (on) st.unchecked.delete(k);
    else st.unchecked.add(k);
  }
  st.message = '';
  draw();
}

function tristate(box, keys) {
  const on = keys.filter((k) => !st.unchecked.has(k)).length;
  box.checked = on === keys.length && on > 0;
  box.indeterminate = on > 0 && on < keys.length;
}

function buildResults(hits) {
  const wrap = el('div', { class: 'find-results' });
  const all = hits.filter(replaceable).map(keyOf);
  if (st.replace && all.length) {
    const box = el('input', { attrs: { type: 'checkbox', 'aria-label': t('find.check_every_result') }, data: { key: 'find:all' } });
    tristate(box, all);
    box.addEventListener('change', () => setChecked(all, box.checked));
    wrap.append(el('label', { class: 'find-option find-all' }, box, t('find.all_results')));
  }
  const list = el('ul', { class: 'find-list', attrs: { 'aria-label': t('find.results') } });
  let cat = null;
  let acct = null;
  let acctList = null;
  hits.forEach((h, index) => {
    if (h.category !== (cat && cat.code) || h.ti !== (cat && cat.ti)) {
      cat = { code: h.category, ti: h.ti };
      acct = null;
      list.append(el('li', { class: 'find-cat', text: `${str(h.category)} ${str(h.category_name)}`.trim() }));
    }
    if (!acct || acct.code !== h.account || acct.ai !== h.ai) {
      acct = { code: h.account, ai: h.ai };
      const keys = hits.filter((x) => x.ti === h.ti && x.ai === h.ai && replaceable(x)).map(keyOf);
      const count = hits.filter((x) => x.ti === h.ti && x.ai === h.ai).length;
      const head = el('div', { class: 'find-acct-head' });
      if (st.replace && keys.length) {
        const box = el('input', {
          attrs: { type: 'checkbox', 'aria-label': t('find.check_the_results_in', { account: h.account }) },
          data: { key: `find:acct:${h.ti}:${h.ai}` },
        });
        tristate(box, keys);
        box.addEventListener('change', () => setChecked(keys, box.checked));
        head.append(box);
      }
      head.append(
        el('span', { class: 'find-acct-name', text: `${str(h.account)} ${str(h.account_name)}`.trim() }),
        el('span', { class: 'find-acct-count', text: String(count) }),
      );
      acctList = el('ul', { class: 'find-hits' });
      list.append(el('li', { class: 'find-acct' }, head, acctList));
    }
    acctList.append(hitItem(h, index));
  });
  wrap.append(list);
  return wrap;
}

function hitItem(h, index) {
  const key = keyOf(h);
  const li = el('li', { class: 'find-hit' });
  if (st.replace) {
    if (replaceable(h)) {
      const box = el('input', {
        attrs: { type: 'checkbox', 'aria-label': t('find.replace_here', { what: hitLabel(h) }) },
        data: { key: `find:check:${key}` },
      });
      box.checked = !st.unchecked.has(key);
      box.addEventListener('change', () => setChecked([key], box.checked));
      li.append(box);
    } else {
      li.append(el('span', { class: 'find-nocheck', title: t('find.account_numbers_are_changed_in'), attrs: { 'aria-hidden': 'true' } }));
    }
  }
  const field = FIELD_LABELS[h.field] || h.field;
  const isRow = h.row_id !== null && h.row_id !== undefined;
  const where = isRow ? t('find.line_field', { line: h.line, field }) : field;
  const desc = isRow ? str(h.description).trim() || t('find.no_description') : '';
  const showText = !isRow || h.field !== 'description';
  const b = el(
    'button',
    {
      class: 'find-entry',
      title: t('find.go_to', { what: hitLabel(h) }),
      attrs: { type: 'button', 'aria-current': key === st.current ? 'true' : null },
      data: { key: `find:hit:${key}`, nobusy: '1' },
    },
    el('span', { class: 'find-where', text: where }),
    desc ? el('span', { class: 'find-desc', text: desc }) : null,
    showText ? el('span', { class: 'find-snippet', text: str(h.text) || t('find.blank') }) : null,
  );
  b.addEventListener('click', () => goTo(h, index, { keepFocus: false }));
  b.addEventListener('keydown', (e) => {
    const stepBy = e.key === 'ArrowDown' ? 1 : e.key === 'ArrowUp' ? -1 : 0;
    if (!stepBy || e.altKey || e.metaKey || e.ctrlKey) return;
    const all = [...st.host.querySelectorAll('.find-entry')];
    const next = all[all.indexOf(b) + stepBy];
    if (next) {
      e.preventDefault();
      next.focus();
    }
  });
  li.append(b);
  return li;
}

function hitLabel(h) {
  const field = FIELD_LABELS[h.field] || h.field;
  if (h.row_id === null || h.row_id === undefined) return t('find.field_of_account', { field: field.toLowerCase(), account: h.account });
  return t('find.line_of_account', { line: h.line, account: h.account, field: field.toLowerCase() });
}
