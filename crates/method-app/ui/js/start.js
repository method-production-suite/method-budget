// The start window (roadmap 3.19; decisions.md, entries 63, 65, 77 and 91;
// design: docs/research/new-project-window.md; src/start.rs): pinned and
// recent budgets with their net totals and thumbnails, search, open, pin,
// duplicate, show in folder and take off the list; project folders down the
// left, each with its budgets, what came from them, its schedules and
// actuals; and the New Budget form. main.js shows it at launch when nothing
// else is open, and for File > New… and the toolbar's New.
//
// Rust holds every path: a row is named by the token of the last listing
// (start_list), and the budget file New Budget starts from is picked in
// Rust's Open dialog and named by its token. Everything typed goes through
// the same checks as the setup tabs (src/start.rs, build).

import { el, hasMod, shortcut } from './dom.js';
import { customDialog } from './modal.js';
import { money } from './format.js';
import { readPref, writePref } from './prefs.js';
import { menuRunsAppKeys } from './shortcuts.js';
import {
  KINDS,
  accountsText,
  chartSections,
  fileNameFrom,
  filterEntries,
  packText,
  peopleText,
  projectEntries,
  SORTS,
  sortEntries,
  specFrom,
  startForKind,
  thumbRects,
  tvFields,
  whenText,
} from './startlist.js';

const SVG = 'http://www.w3.org/2000/svg';

/**
 * A budget's thumbnail: its categories as bars (src/start.rs, bars), drawn
 * as SVG shapes sized by attributes (the page's CSP allows no inline
 * styles). Hidden from screen readers: the row says the same in words.
 */
function thumbnail(bars) {
  const svg = document.createElementNS(SVG, 'svg');
  svg.setAttribute('class', 'st-thumb');
  svg.setAttribute('viewBox', '0 0 56 24');
  svg.setAttribute('width', '56');
  svg.setAttribute('height', '24');
  svg.setAttribute('aria-hidden', 'true');
  svg.setAttribute('focusable', 'false');
  const rects = thumbRects(bars, 56, 24);
  if (!rects.length) {
    const line = document.createElementNS(SVG, 'rect');
    line.setAttribute('class', 'st-thumb-none');
    for (const [k, v] of Object.entries({ x: 0, y: 23, width: 56, height: 1 })) line.setAttribute(k, String(v));
    svg.append(line);
    return svg;
  }
  for (const r of rects) {
    const rect = document.createElementNS(SVG, 'rect');
    rect.setAttribute('class', 'st-thumb-bar');
    for (const [k, v] of Object.entries({ x: r.x, y: r.y, width: r.w, height: r.h })) rect.setAttribute(k, String(v));
    const title = document.createElementNS(SVG, 'title');
    title.textContent = r.text;
    rect.append(title);
    svg.append(rect);
  }
  return svg;
}

/** The open start window: { box, close, menu(id) }, or null. */
let current = null;

/**
 * The open start window, or null. Another dialog put in its place (only
 * one shows at a time, modal.js) closes it without its onClose, so a window
 * no longer on the page counts as closed.
 */
function live() {
  if (current && !current.box.isConnected) current = null;
  return current;
}

/** Whether the start window is open. */
export function isStartOpen() {
  return live() !== null;
}

/** Closes the start window, if it is open (a budget opened another way). */
export function closeStart() {
  if (live()) current.close();
}

/**
 * A menu command while the start window is open (the menu bar, or keys
 * macOS gives the menu): New shows the form, Open the Open dialog. True when
 * it was answered here.
 */
export function startMenu(id) {
  return live() ? current.menu(id) : false;
}

/** Whether to show the start window when the app starts (the window's own box). */
export function startAtLaunch() {
  return readPref('startAtLaunch', true) !== false;
}

/**
 * Whether closing the last tab shows the start window (over an untouched
 * Untitled budget) rather than closing the window. The same box as at
 * launch governs it: someone who turned the window off doesn't get it here
 * either (decisions.md, entry 65).
 */
export function startOnLastClose() {
  return startAtLaunch();
}

/**
 * Shows the start window. `ctx`: { invoke, apply(view, cmd), toast, focus }.
 * `opts.list`: a start_list result already fetched. Already open: just
 * brought back to its lists.
 */
export async function showStart(ctx, opts = {}) {
  if (live()) return;
  let data = opts.list || null;
  if (!data) {
    try {
      data = await ctx.invoke('start_list');
    } catch (e) {
      ctx.toast(e);
      data = null;
    }
  }
  if (live()) return;
  // The bundled charts with their sizes (src/start.rs, new_budget_charts).
  let charts = [];
  try {
    charts = (await ctx.invoke('new_budget_charts')) || [];
  } catch (_) {
    charts = [];
  }
  if (live()) return;
  open(ctx, data || { pinned: [], recent: [], fresh: false }, charts);
}

function open(ctx, first, charts) {
  let data = first;
  let query = '';
  let page = 'home';
  // The place shown on the right: null for the pinned and recent budgets,
  // else a project, by its folder (tokens change with every listing).
  let place = null;
  // The lists' order (startlist.js, SORTS), kept for next time.
  let sortBy = String(readPref('start.sort', 'recent') || 'recent');
  let busy = false;
  let box = null;
  let close = () => {};
  // New Budget's form, kept while the window is open (Back keeps what was typed).
  let form = null;

  /** The project shown, or null (the pinned and recent budgets). */
  const project = () => {
    const p = place === null ? null : (data.projects || []).find((x) => x.folder === place) || null;
    if (!p) place = null;
    return p;
  };

  // -------------------------------------------------------------------------
  // The lists

  const search = el('input', {
    class: 'modal-input st-search',
    attrs: {
      type: 'search',
      placeholder: 'Search by title, file or folder',
      'aria-label': 'Search budgets',
      'data-autofocus': true,
      spellcheck: 'false',
      autocomplete: 'off',
    },
  });
  const lists = el('div', { class: 'st-lists', attrs: { role: 'region', 'aria-label': 'Budgets' } });
  const sortSelect = el('select', { class: 'modal-input st-sort', attrs: { 'aria-label': 'Sort by' } });
  for (const [v, label] of SORTS) sortSelect.append(el('option', { text: label, attrs: { value: v } }));
  if (!SORTS.some(([v]) => v === sortBy)) sortBy = 'recent';
  sortSelect.value = sortBy;
  sortSelect.addEventListener('change', () => {
    sortBy = sortSelect.value;
    writePref('start.sort', sortBy);
    drawLists();
  });
  /** `list` searched and sorted as the window shows it. */
  const shown = (list) => sortEntries(filterEntries(list, query), sortBy);

  /** Runs `fn` with nothing else running; its error goes to the toast. */
  const once = async (fn) => {
    if (busy) return;
    busy = true;
    try {
      await fn();
    } catch (e) {
      ctx.toast(e);
    } finally {
      busy = false;
    }
  };

  const openEntry = (e) =>
    once(async () => {
      const v = await ctx.invoke('start_open', { token: e.token });
      close();
      if (v) ctx.apply(v, 'start_open');
    });

  const openFile = () =>
    once(async () => {
      const r = await ctx.invoke('open_budget');
      if (!r) return; // cancelled: the window stays
      close();
      // A budget, or a budget export or spreadsheet for its import (main.js).
      if (ctx.opened) ctx.opened(r, 'open_budget');
      else ctx.apply(r.kind === 'budget' ? r.view : r, 'open_budget');
    });

  /** Redraws from a fresh listing, keeping the focus on the same file. */
  const relist = (next, keep) => {
    data = next;
    drawRail();
    drawLists();
    focusRow(keep);
  };

  const pin = (e) =>
    once(async () => relist(await ctx.invoke('start_pin', { token: e.token, pinned: !e.pinned }), e));

  const forget = (e) =>
    once(async () => relist(await ctx.invoke('start_forget', { token: e.token }), e));

  const reveal = (e) => once(() => ctx.invoke('start_reveal', { token: e.token }));

  const duplicate = (e) =>
    once(async () => {
      const r = await ctx.invoke('start_duplicate', { token: e.token });
      close();
      if (r && r.view) ctx.apply(r.view, 'start_duplicate');
      if (r && r.message) ctx.toast(r.message);
    });

  const canOpen = (e) => !e.problem && e.openable !== false;

  const totalText = (e) => {
    if (e.problem || e.net_total === null || e.net_total === undefined) return '';
    return `${money(e.net_total)} ${e.currency || ''}`.trim();
  };

  /** A row's kind in words, when it isn't a plain budget ("sub-budget", "schedule"…). */
  const kindNote = (e) => (e.kind && e.kind !== 'budget' ? e.kind : '');

  const row = (e) => {
    const inProject = place !== null;
    const where = inProject ? e.file : [e.file, e.folder].filter(Boolean).join('  ');
    const note = e.problem || [kindNote(e), e.shared ? 'shared' : '', e.open ? 'Open in a tab' : ''].filter(Boolean).join(', ');
    const total = totalText(e);
    const who = e.shared ? peopleText(e.people) : '';
    const main = el(
      'button',
      {
        class: 'st-open',
        attrs: { type: 'button', 'aria-label': [e.title || e.file, total, where, note, who].filter(Boolean).join(', ') },
        data: { key: `${e.folder}\n${e.file}` },
      },
      e.openable === false ? el('span', { class: 'st-thumb st-thumb-file', attrs: { 'aria-hidden': 'true' } }) : thumbnail(e.bars),
      el('span', { class: 'st-title', text: e.title || e.file }),
      el('span', { class: 'st-total', text: total }),
      el('span', { class: 'st-when', text: whenText(e.modified) }),
      el('span', { class: 'st-where', text: where }),
      el('span', { class: e.problem ? 'st-note st-problem' : 'st-note', text: note }),
      who ? el('span', { class: 'st-people', text: who }) : null,
    );
    main.title = where;
    main.addEventListener('click', () => {
      if (canOpen(e)) openEntry(e);
      else if (!e.problem) reveal(e);
    });
    main.addEventListener('keydown', (ev) => onRowKey(ev, e));
    const actions = el('span', { class: 'st-actions' });
    const act = (label, fn) => {
      const b = el('button', { class: 'btn btn-quiet st-act', text: label, attrs: { type: 'button' } });
      b.addEventListener('click', () => fn(e));
      actions.append(b);
    };
    if ((canOpen(e) || e.pinned) && e.openable !== false) act(e.pinned ? 'Unpin' : 'Pin', pin);
    if (canOpen(e)) act('Duplicate', duplicate);
    if (!e.problem) act('Show in Folder', reveal);
    if (!inProject && (e.recent || e.problem)) act('Remove', forget);
    const cls = ['st-row', e.problem ? 'st-missing' : '', e.under ? 'st-under' : '', e.openable === false ? 'st-file' : ''].filter(Boolean).join(' ');
    return el('li', { class: cls }, main, actions);
  };

  const section = (title, list) =>
    el(
      'section',
      { class: 'st-section' },
      el('h3', { class: 'st-section-title', text: title }),
      el('ul', { class: 'st-list' }, ...list.map(row)),
    );

  // -------------------------------------------------------------------------
  // Projects: folders down the left, each one's files on the right

  const rail = el('nav', { class: 'st-rail', attrs: { 'aria-label': 'Places' } });

  const placeButtons = () => [...rail.querySelectorAll('.st-place')];

  const choosePlace = (folder, focusIt = true) => {
    place = folder;
    drawRail();
    drawLists();
    if (focusIt) {
      const b = placeButtons().find((x) => x.getAttribute('aria-current') === 'true');
      if (b) b.focus();
    }
  };

  const addFolder = () =>
    once(async () => {
      const next = await ctx.invoke('start_add_project');
      if (!next) return; // cancelled
      data = next;
      const added = (data.projects || [])[(data.projects || []).length - 1];
      choosePlace(added ? added.folder : place);
    });

  const onPlaceKey = (ev) => {
    if (ev.metaKey || ev.ctrlKey || ev.altKey) return;
    const all = placeButtons();
    const at = all.indexOf(ev.currentTarget);
    if (ev.key === 'ArrowDown' || ev.key === 'ArrowUp') {
      ev.preventDefault();
      const next = all[Math.max(0, Math.min(all.length - 1, at + (ev.key === 'ArrowDown' ? 1 : -1)))];
      if (next) next.focus();
    } else if (ev.key === 'ArrowRight') {
      ev.preventDefault();
      focusRow(null);
    } else if (ev.key.length === 1 && ev.key !== ' ') {
      search.focus();
    }
  };

  const drawRail = () => {
    const button = (label, folder, detail) => {
      const b = el(
        'button',
        { class: 'st-place', attrs: { type: 'button', 'aria-current': place === folder ? 'true' : 'false', title: detail || label } },
        el('span', { class: 'st-place-name', text: label }),
      );
      b.addEventListener('click', () => choosePlace(folder));
      b.addEventListener('keydown', onPlaceKey);
      return b;
    };
    const projects = data.projects || [];
    const add = el('button', { class: 'btn btn-quiet st-add-folder', text: 'Add Folder…', attrs: { type: 'button' } });
    add.addEventListener('click', addFolder);
    const parts = [button('Pinned and recent', null), el('h3', { class: 'st-section-title', text: 'Projects' }), ...projects.map((p) => button(p.name, p.folder, p.folder))];
    if (!projects.length) parts.push(el('p', { class: 'st-aside st-rail-empty', text: 'A folder per production, with its budgets.' }));
    rail.replaceChildren(...parts, add);
  };

  /** New Budget Here: the form, saving in project `p`'s folder. */
  const newHere = (p) =>
    once(async () => {
      const f = await ctx.invoke('start_project_folder', { token: p.token });
      showForm({ folder: f });
    });

  const removeProject = (p) =>
    once(async () => {
      data = await ctx.invoke('start_remove_project', { token: p.token });
      choosePlace(null);
    });

  const drawProject = (p) => {
    const btn = (label, fn) => {
      const b = el('button', { class: 'btn btn-quiet st-act', text: label, attrs: { type: 'button' } });
      b.addEventListener('click', () => fn(p));
      return b;
    };
    const head = el(
      'div',
      { class: 'st-project-head' },
      el('h3', { class: 'st-project-name', text: p.name }),
      el('span', { class: 'st-where', text: p.folder }),
      el(
        'span',
        { class: 'st-project-actions' },
        p.problem ? null : btn('New Budget Here…', newHere),
        p.problem ? null : btn('Show Folder', (x) => reveal(x)),
        btn('Remove from List', removeProject),
      ),
    );
    const items = sortEntries(projectEntries(p, query), sortBy);
    const parts = [head];
    if (p.problem) parts.push(el('p', { class: 'st-empty st-problem', text: `${p.problem} Remove it from the list, or put the folder back.` }));
    else if (items.length) parts.push(el('ul', { class: 'st-list' }, ...items.map(row)));
    else if (query) parts.push(el('p', { class: 'st-empty', text: `Nothing in this folder matches "${query.trim()}".` }));
    else parts.push(el('p', { class: 'st-empty', text: 'No budgets in this folder yet. New Budget Here… makes one.' }));
    if (p.more) parts.push(el('p', { class: 'st-aside', text: 'This folder holds more files than are read here. Show Folder shows them all.' }));
    lists.replaceChildren(...parts);
  };

  const drawLists = () => {
    const p = project();
    if (p) {
      drawProject(p);
      return;
    }
    const pinned = shown(data.pinned);
    const recent = shown(data.recent);
    const parts = [];
    if (pinned.length) parts.push(section('Pinned', pinned));
    if (recent.length) parts.push(section('Recent', recent));
    if (!parts.length) {
      const empty = query
        ? `No budget matches "${query.trim()}". Open… finds any budget file.`
        : 'No recent budgets yet. Start one with New Budget, or open a budget file.';
      parts.push(el('p', { class: 'st-empty', text: empty }));
    }
    lists.replaceChildren(...parts);
  };

  const rows = () => [...lists.querySelectorAll('.st-open')];

  const focusRow = (e) => {
    const all = rows();
    const key = e ? `${e.folder}\n${e.file}` : null;
    const hit = all.find((b) => b.dataset.key === key) || all[0];
    if (hit) hit.focus();
    else search.focus();
  };

  const onRowKey = (ev, e) => {
    if (ev.metaKey || ev.ctrlKey || ev.altKey) return;
    const all = rows();
    const at = all.indexOf(ev.currentTarget);
    const go = (i) => {
      ev.preventDefault();
      if (i < 0) search.focus();
      else if (all[Math.min(i, all.length - 1)]) all[Math.min(i, all.length - 1)].focus();
    };
    if (ev.key === 'ArrowDown') return go(at + 1);
    if (ev.key === 'ArrowUp') return go(at - 1);
    if (ev.key === 'Home') return go(0);
    if (ev.key === 'End') return go(all.length - 1);
    if (ev.key === 'ArrowLeft') {
      ev.preventDefault();
      const b = placeButtons().find((x) => x.getAttribute('aria-current') === 'true');
      if (b) b.focus();
      return;
    }
    if (ev.key === 'p' || ev.key === 'P') {
      ev.preventDefault();
      if ((canOpen(e) || e.pinned) && e.openable !== false) pin(e);
      return;
    }
    if (ev.key === 'd' || ev.key === 'D') {
      ev.preventDefault();
      if (canOpen(e)) duplicate(e);
      return;
    }
    if (ev.key === 'Delete' || ev.key === 'Backspace') {
      ev.preventDefault();
      if (place === null && (e.recent || e.problem)) forget(e);
      return;
    }
    // Typing anywhere in the lists types into the search box.
    if (ev.key.length === 1 && ev.key !== ' ') search.focus();
  };

  search.addEventListener('input', () => {
    query = search.value;
    drawLists();
  });
  search.addEventListener('keydown', (ev) => {
    if (ev.key === 'ArrowDown') {
      ev.preventDefault();
      const first = rows()[0];
      if (first) first.focus();
    } else if (ev.key === 'Enter') {
      ev.preventDefault();
      const p = project();
      const first = (p ? sortEntries(projectEntries(p, query), sortBy) : [...shown(data.pinned), ...shown(data.recent)]).filter(canOpen);
      if (first.length) openEntry(first[0]);
    } else if (ev.key === 'Escape' && search.value) {
      // Escape clears the search first, then closes.
      ev.preventDefault();
      ev.stopPropagation();
      search.value = '';
      query = '';
      drawLists();
    }
  });

  const keyHint = (spec) => el('span', { class: 'st-key', text: shortcut(spec) });

  const home = () => {
    const newBtn = el('button', { class: 'btn btn-primary', attrs: { type: 'button' } }, 'New Budget…', keyHint('mod+n'));
    const openBtn = el('button', { class: 'btn', attrs: { type: 'button' } }, 'Open…', keyHint('mod+o'));
    newBtn.addEventListener('click', () => showForm());
    openBtn.addEventListener('click', openFile);
    const atStart = el('input', { attrs: { type: 'checkbox', id: 'st-at-start' } });
    atStart.checked = startAtLaunch();
    atStart.addEventListener('change', () => writePref('startAtLaunch', atStart.checked));
    const closeBtn = el('button', { class: 'btn', text: 'Close', attrs: { type: 'button' } });
    closeBtn.addEventListener('click', () => close());
    drawRail();
    drawLists();
    return [
      el(
        'div',
        { class: 'st-head' },
        el('h2', { class: 'modal-title', text: 'Budgets', attrs: { id: 'st-title' } }),
        newBtn,
        openBtn,
        search,
        sortSelect,
      ),
      el('div', { class: 'st-body' }, rail, lists),
      el('p', {
        class: 'st-hint',
        text: 'Up and Down move, Enter opens, P pins, D duplicates, Delete takes a budget off the list (the file stays), Left and Right go between the places and the list.',
      }),
      el(
        'div',
        { class: 'st-foot' },
        el('label', { class: 'rs-check', attrs: { for: 'st-at-start' } }, atStart, el('span', { text: 'Show this window when the app starts and when the last tab closes' })),
        closeBtn,
      ),
    ];
  };

  // -------------------------------------------------------------------------
  // New Budget

  /** New Budget's form; `opts.folder`: a project's folder to save in (New Budget Here). */
  const showForm = (opts = {}) => {
    if (!form) form = makeForm();
    if (opts.folder) form.saveIn(opts.folder, false);
    page = 'form';
    box.setAttribute('aria-labelledby', 'st-new-title');
    box.classList.add('st-form-page');
    box.replaceChildren(...form.parts);
    form.title.focus();
    form.title.select();
  };

  const showHome = () => {
    page = 'home';
    box.setAttribute('aria-labelledby', 'st-title');
    box.classList.remove('st-form-page');
    box.replaceChildren(...home());
    search.focus();
  };

  const makeForm = () => {
    const inputs = {};
    const field = (key, label, { placeholder = '', span = 1, value = '', max = 100 } = {}) => {
      const input = el('input', {
        class: 'modal-input',
        attrs: { type: 'text', id: `st-f-${key}`, placeholder, maxlength: String(max), spellcheck: 'false', autocomplete: 'off' },
      });
      input.value = value;
      inputs[key] = input;
      return el('label', { class: `modal-field st-span${span}`, attrs: { for: input.id } }, label, input);
    };
    const select = (key, label, options, { span = 1, value = '' } = {}) => {
      const s = el('select', { class: 'modal-input', attrs: { id: `st-f-${key}` } });
      for (const [v, text] of options) s.append(el('option', { text, attrs: { value: v } }));
      if (value) s.value = value;
      inputs[key] = s;
      return el('label', { class: `modal-field st-span${span}`, attrs: { for: s.id } }, label, s);
    };
    const heading = (text, id) => el('h3', { class: 'st-group-title', text, attrs: { id } });

    const title = field('title', 'Title', { span: 4, max: 200, placeholder: 'The production’s name' });
    const kind = select('kind', 'Production type', KINDS, { span: 2, value: 'feature' });
    const season = field('season', 'Season', { max: 50 });
    const episode = field('episode', 'Episode', { max: 50 });
    const episodes = field('episodes', 'Episodes', { max: 3, placeholder: '10' });
    const date = (key, label) => field(key, label, { placeholder: 'YYYY-MM-DD', max: 10 });
    const num = (key, label) => field(key, label, { max: 12 });

    // -----------------------------------------------------------------------
    // Chart of accounts: a bundled chart (the type's own by default), the
    // starter budget, an empty budget or a picked budget file.

    let from = charts.length ? 'chart' : 'starter';
    let chartId = (startForKind('feature', charts) || '').replace(/^chart:/, '') || (charts[0] && charts[0].id) || '';
    let fromTouched = false; // From or the chart changed by hand: the type no longer picks
    let picked = null;
    let chartQuery = '';

    const fromChoices = [
      ['chart', 'A chart of accounts', 'Categories and accounts, no lines'],
      ['starter', 'Starter budget', 'A small sample with placeholder rates'],
      ['blank', 'Empty', 'No categories yet'],
      ['file', 'A budget file', 'Its chart and setup, no lines'],
    ];
    const fromRadios = {};
    const fromRow = el('div', { class: 'st-from st-span4', attrs: { role: 'radiogroup', 'aria-labelledby': 'st-from-title' } });
    for (const [value, label, hint] of fromChoices) {
      const r = el('input', { attrs: { type: 'radio', name: 'st-from', value, id: `st-from-${value}` } });
      if (value === 'chart' && !charts.length) r.disabled = true;
      fromRadios[value] = r;
      fromRow.append(
        el(
          'label',
          { class: 'st-from-choice', attrs: { for: r.id } },
          r,
          el('span', { class: 'st-from-name', text: label }),
          el('span', { class: 'st-from-hint', text: hint }),
        ),
      );
    }

    const chartSearch = el('input', {
      class: 'modal-input st-chart-search',
      attrs: { type: 'search', placeholder: 'Find a chart by name', 'aria-label': 'Find a chart', spellcheck: 'false', autocomplete: 'off' },
    });
    const chartList = el('div', { class: 'st-chart-list', attrs: { role: 'radiogroup', 'aria-label': 'Charts of accounts' } });
    const chartPane = el('div', { class: 'st-chart-pane st-span4' }, chartSearch, chartList);
    const pickBtn = el('button', { class: 'btn', text: 'Choose File…', attrs: { type: 'button' } });
    const pickedText = el('span', { class: 'st-picked' });
    const filePane = el('div', { class: 'st-file-pane st-span4' }, pickBtn, pickedText);
    const fromNote = el('p', { class: 'st-from-note st-span4', attrs: { 'aria-live': 'polite' } });

    const chartById = (id) => charts.find((c) => c.id === id) || null;

    const drawCharts = () => {
      const sections = chartSections(charts, inputs.kind.value, chartQuery);
      const row = (c) => {
        const r = el('input', { attrs: { type: 'radio', name: 'st-chart', value: c.id, id: `st-chart-${c.id}` } });
        r.checked = c.id === chartId;
        r.addEventListener('change', () => {
          chartId = c.id;
          fromTouched = true;
          drawNote();
        });
        return el(
          'label',
          { class: 'st-chart', attrs: { for: r.id } },
          r,
          el('span', { class: 'st-chart-name', text: c.name }),
          el('span', { class: 'st-chart-count', text: accountsText(c.accounts) }),
          el('span', { class: 'st-chart-desc', text: c.description || '' }),
        );
      };
      const parts = [];
      for (const s of sections) {
        parts.push(el('h4', { class: 'st-chart-group', text: s.group }));
        parts.push(...s.ours.map(row));
        if (s.studio.length) {
          parts.push(el('p', { class: 'st-chart-sub', text: 'Studio and network charts: numbers, names and levels only' }));
          parts.push(...s.studio.map(row));
        }
      }
      if (!parts.length) parts.push(el('p', { class: 'st-empty', text: `No chart matches "${chartQuery.trim()}".` }));
      chartList.replaceChildren(...parts);
    };

    const drawNote = () => {
      let text = '';
      if (from === 'chart') {
        const c = chartById(chartId);
        text = c ? `${c.name}: ${accountsText(c.accounts)} in ${c.categories} ${c.categories === 1 ? 'category' : 'categories'}.` : 'Choose a chart.';
      } else if (from === 'starter') text = 'The starter budget: a few categories with sample lines and placeholder rates.';
      else if (from === 'blank') text = 'An empty budget: add categories and accounts yourself.';
      else if (from === 'file') {
        text = picked
          ? `${picked.file}: ${accountsText(picked.accounts) || 'no accounts'} in ${picked.categories} ${picked.categories === 1 ? 'category' : 'categories'}.`
          : 'Choose the budget file to start from.';
      }
      fromNote.textContent = text;
    };

    const showFrom = () => {
      for (const [v, r] of Object.entries(fromRadios)) r.checked = v === from;
      chartPane.hidden = from !== 'chart';
      filePane.hidden = from !== 'file';
      pickedText.textContent = picked ? (picked.title && picked.title !== picked.file ? `${picked.file} (${picked.title})` : picked.file) : '';
      drawNote();
    };

    const pickFile = () =>
      once(async () => {
        const p = await ctx.invoke('new_budget_pick_template');
        if (p) {
          picked = p;
          from = 'file';
          fromTouched = true;
        } else if (!picked && from === 'file') {
          // Cancelled with nothing picked: back to a chart (or the starter).
          from = charts.length ? 'chart' : 'starter';
        }
        showFrom();
        fromRadios[from].focus();
      });

    for (const [v, r] of Object.entries(fromRadios)) {
      r.addEventListener('change', () => {
        if (!r.checked) return;
        from = v;
        fromTouched = true;
        showFrom();
        if (v === 'file' && !picked) pickFile();
      });
    }
    pickBtn.addEventListener('click', pickFile);
    chartSearch.addEventListener('input', () => {
      chartQuery = chartSearch.value;
      drawCharts();
    });
    chartSearch.addEventListener('keydown', (ev) => {
      if (ev.key === 'Escape' && chartSearch.value) {
        ev.preventDefault();
        ev.stopPropagation();
        chartSearch.value = '';
        chartQuery = '';
        drawCharts();
      } else if (ev.key === 'ArrowDown' || ev.key === 'Enter') {
        // Into the list: the chosen chart, else the first shown.
        ev.preventDefault();
        ev.stopPropagation();
        const radios = [...chartList.querySelectorAll('input[type=radio]')];
        const r = radios.find((x) => x.checked) || radios[0];
        if (r) {
          if (!r.checked) {
            r.checked = true;
            r.dispatchEvent(new Event('change'));
          }
          r.focus();
        }
      }
    });

    const tvShown = () => {
      const tv = tvFields(inputs.kind.value);
      season.hidden = !tv.season;
      episode.hidden = !tv.episode;
      episodes.hidden = !tv.episodes;
    };
    inputs.kind.addEventListener('change', () => {
      tvShown();
      if (!fromTouched) {
        const s = startForKind(inputs.kind.value, charts);
        if (s) {
          from = 'chart';
          chartId = s.slice('chart:'.length);
        }
      }
      drawCharts();
      showFrom();
    });
    tvShown();
    drawCharts();
    showFrom();

    // -----------------------------------------------------------------------
    // Save In: a folder (named by its token; Rust holds the path) and a file
    // name that follows the title until it is typed by hand.

    let folder = null; // { token, folder }
    let folderGiven = false; // New Budget Here chose it: the default folder doesn't replace it
    const folderText = el('span', { class: 'st-folder-text', attrs: { id: 'st-folder-text' } });
    // A project folder of its own, named like the file, in the folder above
    // (decision 91). On unless the folder is a project already.
    const projectBox = el('input', { attrs: { type: 'checkbox', id: 'st-project' } });
    projectBox.checked = readPref('newBudget.projectFolder', true) !== false;
    const fileAside = el('span', { class: 'st-span1 st-aside' });
    const showProject = () => {
      projectBox.disabled = !folder;
      fileAside.textContent = projectBox.checked && folder ? '.mthd, in a new folder of the same name' : '.mthd, never over another file';
    };
    projectBox.addEventListener('change', () => {
      writePref('newBudget.projectFolder', projectBox.checked);
      showProject();
    });
    const folderBtn = el('button', { class: 'btn', text: 'Choose Folder…', attrs: { type: 'button', 'aria-describedby': 'st-folder-text' } });
    const showFolder = () => {
      folderText.textContent = folder ? folder.folder : 'No folder chosen: the budget opens without a file until you save it.';
      folderText.classList.toggle('st-folder-none', !folder);
      showProject();
    };
    folderBtn.addEventListener('click', () =>
      once(async () => {
        const f = await ctx.invoke('new_budget_pick_folder', { current: folder ? folder.token : null });
        if (f) {
          folder = f;
          folderGiven = true;
        }
        showFolder();
        folderBtn.focus();
      }),
    );
    showFolder();
    ctx
      .invoke('new_budget_folder')
      .then((f) => {
        if (f && !folder && !folderGiven) folder = f;
        showFolder();
      })
      .catch(() => showFolder());

    // -----------------------------------------------------------------------
    // Link a Schedule: a schedule days file (Method Schedule's) fills Shoot
    // days and the shoot dates here, and at Create its shoot days go to
    // SHOOT_DAYS linked to the schedule, and its prep start and wrap end to
    // the budget info (src/start.rs, build_with; decision 91).

    let linked = null; // the review of the picked file: { token, file, schedule, shoot_days, start, end }
    const scheduleText = el('span', { class: 'st-folder-text st-folder-none', attrs: { id: 'st-schedule-text' } });
    const scheduleBtn = el('button', { class: 'btn', text: 'Link a Schedule…', attrs: { type: 'button', 'aria-describedby': 'st-schedule-text' } });
    const unlinkBtn = el('button', { class: 'btn btn-quiet', text: 'Unlink', attrs: { type: 'button' } });
    const showSchedule = () => {
      unlinkBtn.hidden = !linked;
      scheduleText.classList.toggle('st-folder-none', !linked);
      scheduleText.textContent = linked
        ? `${linked.file}: ${linked.schedule || 'a schedule'}, ${linked.shoot_days} shoot ${linked.shoot_days === 1 ? 'day' : 'days'}. Its prep start and wrap end come too, and Shoot days stays linked to it.`
        : 'No schedule linked: type the schedule length, or link the days file Method Schedule writes.';
    };
    scheduleBtn.addEventListener('click', () =>
      once(async () => {
        const r = await ctx.invoke('schedule_days_pick', {});
        if (r) {
          linked = r;
          inputs.shoot_days.value = String(r.shoot_days);
          if (r.start) inputs.shoot_start.value = r.start;
          if (r.end) inputs.shoot_end.value = r.end;
        }
        showSchedule();
        scheduleBtn.focus();
      }),
    );
    unlinkBtn.addEventListener('click', () => {
      linked = null;
      showSchedule();
      scheduleBtn.focus();
    });
    showSchedule();

    const fileName = field('file_name', 'File name', { span: 3, max: 200 });
    let fileTouched = false;
    const followTitle = () => {
      if (!fileTouched) inputs.file_name.value = inputs.title.value.trim() ? fileNameFrom(inputs.title.value) : '';
      inputs.file_name.placeholder = fileNameFrom(inputs.title.value);
    };
    inputs.title.addEventListener('input', followTitle);
    inputs.file_name.addEventListener('input', () => {
      fileTouched = inputs.file_name.value.trim() !== '';
    });
    followTitle();

    // -----------------------------------------------------------------------
    // Setup: everything in the setup library and any setup packs, brought in
    // before the details below (src/start.rs, new_budget_setup; decision 77).
    // Packs are named by the Packs list's tokens; ticks are kept by file
    // when the list is read again.

    let setupData = { library: null, packs: [] };
    let libraryTicked = false;
    const packsTicked = new Set(); // `${place}/${file}`
    const packKey = (p) => `${p.place}/${p.file}`;
    const setupList = el('div', { class: 'st-setup st-span4', attrs: { role: 'group', 'aria-labelledby': 'st-setup-title' } });
    const tickRow = (id, checked, disabled, name, detail, onChange) => {
      const box = el('input', { attrs: { type: 'checkbox', id } });
      box.checked = checked && !disabled;
      box.disabled = disabled;
      box.addEventListener('change', () => onChange(box.checked));
      return el(
        'label',
        { class: 'st-setup-choice', attrs: { for: id } },
        box,
        el('span', { class: 'st-setup-name', text: name }),
        el('span', { class: 'st-setup-detail', text: detail }),
      );
    };
    const drawSetup = () => {
      const rows = [];
      const lib = setupData.library;
      if (lib) {
        const empty = !lib.problem && !lib.items;
        const detail = lib.problem || (empty ? 'Nothing in it yet' : `${lib.items} ${lib.items === 1 ? 'item' : 'items'}`);
        rows.push(
          tickRow('st-setup-library', libraryTicked, !!lib.problem || empty, `Everything in my setup library (${lib.name})`, detail, (on) => {
            libraryTicked = on;
          }),
        );
      }
      (setupData.packs || []).forEach((p, i) => {
        const t = packText(p);
        const unusable = !p.items;
        rows.push(
          tickRow(`st-setup-pack-${i}`, packsTicked.has(packKey(p)), unusable, t.name, p.problem && unusable ? p.problem : t.detail, (on) => {
            if (on) packsTicked.add(packKey(p));
            else packsTicked.delete(packKey(p));
          }),
        );
      });
      if (!rows.length) {
        rows.push(el('p', { class: 'st-aside', text: 'No setup library or packs yet. Make them in the Library tab of the tools panel, then they show here.' }));
      } else {
        rows.push(el('p', { class: 'st-aside', text: 'What you tick takes the place of the starting point\u2019s items of the same name. The budget settings come from this form.' }));
      }
      setupList.replaceChildren(...rows);
    };
    const readSetup = () =>
      ctx
        .invoke('new_budget_setup')
        .then((d) => {
          setupData = d && typeof d === 'object' ? { library: d.library || null, packs: Array.isArray(d.packs) ? d.packs : [] } : { library: null, packs: [] };
          drawSetup();
        })
        .catch(() => drawSetup());
    const setupTicks = () => ({
      library: libraryTicked && !!setupData.library,
      packs: (setupData.packs || []).filter((p) => packsTicked.has(packKey(p))).map((p) => p.token),
    });
    drawSetup();
    readSetup();

    const error = el('p', { class: 'modal-error', attrs: { role: 'alert' } });
    error.hidden = true;
    const showError = (text, extra = null) => {
      error.replaceChildren(el('span', { text }), ...(extra ? [' ', extra] : []));
      error.hidden = false;
    };
    const back = el('button', { class: 'btn', text: 'Back', attrs: { type: 'button' } });
    back.addEventListener('click', showHome);
    const create = el('button', { class: 'btn btn-primary', attrs: { type: 'button' } }, 'Create Budget', keyHint('mod+enter'));

    const startValue = () => (from === 'chart' ? `chart:${chartId}` : from);

    const submit = () =>
      once(async () => {
        const values = {};
        for (const [k, input] of Object.entries(inputs)) values[k] = input.value;
        const spec = specFrom(values, startValue(), picked ? picked.token : null, folder ? folder.token : null, setupTicks(), projectBox.checked);
        if (linked && Number.isSafeInteger(linked.token)) spec.schedule = linked.token;
        if (!spec.title) {
          showError('Give the budget a title.');
          inputs.title.focus();
          return;
        }
        if (from === 'file' && !picked) {
          showError('Choose the budget file to start from.');
          pickBtn.focus();
          return;
        }
        create.disabled = true;
        try {
          const r = await ctx.invoke('new_budget_create', { spec });
          if (r && r.taken) {
            // A file of that name is in the folder: nothing was made.
            const use = el('button', { class: 'btn btn-quiet', text: `Use "${r.taken}.mthd"`, attrs: { type: 'button' } });
            use.addEventListener('click', () => {
              inputs.file_name.value = r.taken;
              fileTouched = true;
              error.hidden = true;
              submit();
            });
            showError(r.message || 'A file with that name is already there.', use);
            use.focus();
            return;
          }
          if (spec.currency) writePref('newBudget.currency', spec.currency);
          close();
          if (r && r.view) ctx.apply(r.view, 'new_budget_create');
          if (r && r.message) ctx.toast(r.message);
        } catch (e) {
          showError(String(e));
          // The Packs list may have been read again elsewhere: read it
          // again here too (ticks are kept by file).
          readSetup();
        } finally {
          create.disabled = false;
        }
      });
    create.addEventListener('click', submit);

    const grid = el(
      'div',
      { class: 'st-form' },
      title,
      kind,
      season,
      episode,
      episodes,
      heading('Chart of accounts', 'st-from-title'),
      fromRow,
      chartPane,
      filePane,
      fromNote,
      heading('Setup to bring in', 'st-setup-title'),
      setupList,
      heading('Save in'),
      el('div', { class: 'st-folder st-span4' }, folderText, folderBtn),
      el('label', { class: 'rs-check st-span4', attrs: { for: 'st-project' } }, projectBox, el('span', { text: 'Make a project folder for it, named like the file, and list it with the projects' })),
      fileName,
      fileAside,
      heading('Dates'),
      date('prep_start', 'Prep start'),
      date('shoot_start', 'Shoot start'),
      date('shoot_end', 'Shoot end'),
      date('wrap_end', 'Wrap end'),
      heading('Schedule length'),
      num('prep_weeks', 'Prep weeks'),
      num('shoot_days', 'Shoot days'),
      num('wrap_weeks', 'Wrap weeks'),
      el('span', { class: 'st-span1 st-aside', text: 'Kept as globals for formulas.' }),
      el('div', { class: 'st-folder st-span4' }, scheduleText, unlinkBtn, scheduleBtn),
      heading('Location and money'),
      field('country', 'Country'),
      field('region', 'Region or state'),
      field('city', 'City'),
      field('currency', 'Base currency', { placeholder: 'USD', max: 10, value: readPref('newBudget.currency', '') || '' }),
      heading('Settings'),
      select(
        'fringe_posting',
        'Fringes post',
        [
          ['budget', 'One total for the whole budget'],
          ['category', 'An account at the end of each category'],
          ['account', 'A line at the end of each account'],
          ['production_level', 'A row above each production total'],
        ],
        { span: 2, value: 'budget' },
      ),
      select(
        'arithmetic',
        'Arithmetic',
        [
          ['binary', 'Spreadsheet (binary)'],
          ['exact', 'Exact decimal'],
        ],
        { span: 2, value: 'binary' },
      ),
    );
    // Enter in a text box, or Cmd+Enter anywhere, creates the budget (not
    // in the chart search, where Enter goes into the list).
    grid.addEventListener('keydown', (ev) => {
      if (ev.key !== 'Enter' || ev.isComposing || ev.defaultPrevented) return;
      const inText = ev.target instanceof HTMLInputElement && (ev.target.type === 'text' || ev.target.type === 'radio');
      if (hasMod(ev) || (inText && !ev.shiftKey && !ev.altKey)) {
        ev.preventDefault();
        submit();
      }
    });
    const parts = [
      el('h2', { class: 'modal-title', text: 'New Budget', attrs: { id: 'st-new-title' } }),
      el('p', {
        class: 'modal-message',
        text: 'Only the title is needed. The budget is saved in the folder below as soon as it is made. The rest goes into the budget info, globals and settings, where it can be changed later.',
      }),
      el('div', { class: 'st-form-scroll' }, grid),
      error,
      el('div', { class: 'modal-actions' }, back, create),
    ];
    return {
      parts,
      title: inputs.title,
      /** Saves in folder `f` ({token, folder}), with or without a project folder of its own. */
      saveIn: (f, ownFolder) => {
        folder = f;
        folderGiven = true;
        projectBox.checked = ownFolder;
        showFolder();
      },
    };
  };

  // -------------------------------------------------------------------------
  // The window

  const onBoxKey = (ev) => {
    if (ev.key === 'Escape' && page === 'form') {
      // Escape on the form goes back to the lists, keeping what was typed
      // (before the dialog's own Escape, on this same element, closes it).
      ev.preventDefault();
      ev.stopImmediatePropagation();
      showHome();
      return;
    }
    // Where the page answers the app's keys (not macOS, whose menu does: startMenu).
    if (menuRunsAppKeys || !hasMod(ev) || ev.shiftKey || ev.altKey) return;
    const k = ev.key.toLowerCase();
    if (k === 'n' && page === 'home') {
      ev.preventDefault();
      showForm();
    } else if (k === 'o') {
      ev.preventDefault();
      openFile();
    }
  };

  close = customDialog(
    () => {
      box = el('div', {
        class: 'modal st-dialog',
        attrs: { role: 'dialog', 'aria-modal': 'true', 'aria-labelledby': 'st-title' },
      });
      box.append(...home());
      box.addEventListener('keydown', onBoxKey);
      return box;
    },
    {
      onClose: () => {
        current = null;
        ctx.focus();
      },
    },
  );
  current = {
    box,
    close: () => close(),
    menu: (id) => {
      if (id === 'file.new') {
        if (page === 'home') showForm();
        return true;
      }
      if (id === 'file.open') {
        openFile();
        return true;
      }
      return false;
    },
  };
  // Focus starts in the search box, so typing searches at once.
  search.focus();
}
