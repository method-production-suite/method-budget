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
import { t } from './i18n.js';
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
      placeholder: t('start.search_by_title_file_or'),
      'aria-label': t('start.search_budgets'),
      'data-autofocus': true,
      spellcheck: 'false',
      autocomplete: 'off',
    },
  });
  const lists = el('div', { class: 'st-lists', attrs: { role: 'region', 'aria-label': t('start.budgets') } });
  const sortSelect = el('select', { class: 'modal-input st-sort', attrs: { 'aria-label': t('start.sort_by') } });
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
    const note = e.problem || [kindNote(e), e.shared ? t('start.shared') : '', e.open ? t('start.open_in_a_tab') : ''].filter(Boolean).join(', ');
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
    if ((canOpen(e) || e.pinned) && e.openable !== false) act(e.pinned ? t('start.unpin') : t('start.pin'), pin);
    if (canOpen(e)) act(t('start.duplicate'), duplicate);
    if (!e.problem) act(t('start.show_in_folder'), reveal);
    if (!inProject && (e.recent || e.problem)) act(t('start.remove'), forget);
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

  const rail = el('nav', { class: 'st-rail', attrs: { 'aria-label': t('start.places') } });

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
    const add = el('button', { class: 'btn btn-quiet st-add-folder', text: t('start.add_folder'), attrs: { type: 'button' } });
    add.addEventListener('click', addFolder);
    const parts = [button(t('start.pinned_and_recent'), null), el('h3', { class: 'st-section-title', text: t('start.projects') }), ...projects.map((p) => button(p.name, p.folder, p.folder))];
    if (!projects.length) parts.push(el('p', { class: 'st-aside st-rail-empty', text: t('start.a_folder_per_production_with') }));
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
        p.problem ? null : btn(t('start.new_budget_here'), newHere),
        p.problem ? null : btn(t('start.show_folder'), (x) => reveal(x)),
        btn(t('start.remove_from_list'), removeProject),
      ),
    );
    const items = sortEntries(projectEntries(p, query), sortBy);
    const parts = [head];
    if (p.problem) parts.push(el('p', { class: 'st-empty st-problem', text: t('start.remove_it_from_the_list', { problem: p.problem }) }));
    else if (items.length) parts.push(el('ul', { class: 'st-list' }, ...items.map(row)));
    else if (query) parts.push(el('p', { class: 'st-empty', text: t('start.nothing_in_folder_matches', { query: query.trim() }) }));
    else parts.push(el('p', { class: 'st-empty', text: t('start.no_budgets_in_this_folder') }));
    if (p.more) parts.push(el('p', { class: 'st-aside', text: t('start.this_folder_holds_more_files') }));
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
    if (pinned.length) parts.push(section(t('start.pinned'), pinned));
    if (recent.length) parts.push(section(t('start.recent'), recent));
    if (!parts.length) {
      const empty = query
        ? t('start.no_budget_matches', { query: query.trim() })
        : t('start.no_recent_budgets');
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
    const newBtn = el('button', { class: 'btn btn-primary', attrs: { type: 'button' } }, t('start.new_budget_open'), keyHint('mod+n'));
    const openBtn = el('button', { class: 'btn', attrs: { type: 'button' } }, t('start.open'), keyHint('mod+o'));
    newBtn.addEventListener('click', () => showForm());
    openBtn.addEventListener('click', openFile);
    const atStart = el('input', { attrs: { type: 'checkbox', id: 'st-at-start' } });
    atStart.checked = startAtLaunch();
    atStart.addEventListener('change', () => writePref('startAtLaunch', atStart.checked));
    const closeBtn = el('button', { class: 'btn', text: t('start.close'), attrs: { type: 'button' } });
    closeBtn.addEventListener('click', () => close());
    drawRail();
    drawLists();
    return [
      el(
        'div',
        { class: 'st-head' },
        el('h2', { class: 'modal-title', text: t('start.budgets'), attrs: { id: 'st-title' } }),
        newBtn,
        openBtn,
        search,
        sortSelect,
      ),
      el('div', { class: 'st-body' }, rail, lists),
      el('p', {
        class: 'st-hint',
        text: t('start.up_and_down_move_enter'),
      }),
      el(
        'div',
        { class: 'st-foot' },
        el('label', { class: 'rs-check', attrs: { for: 'st-at-start' } }, atStart, el('span', { text: t('start.show_this_window_when_the') })),
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

    const title = field('title', t('start.field.title'), { span: 4, max: 200, placeholder: t('start.the_production_s_name') });
    const kind = select('kind', t('start.field.kind'), KINDS, { span: 2, value: 'feature' });
    const season = field('season', t('start.field.season'), { max: 50 });
    const episode = field('episode', t('start.field.episode'), { max: 50 });
    const episodes = field('episodes', t('start.field.episodes'), { max: 3, placeholder: '10' });
    const date = (key, label) => field(key, label, { placeholder: t('start.yyyy_mm_dd'), max: 10 });
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
      ['chart', t('start.from.chart'), t('start.from.chart_hint')],
      ['starter', t('start.from.starter'), t('start.from.starter_hint')],
      ['blank', t('start.from.blank'), t('start.from.blank_hint')],
      ['file', t('start.from.file'), t('start.from.file_hint')],
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
      attrs: { type: 'search', placeholder: t('start.find_a_chart_by_name'), 'aria-label': t('start.find_a_chart'), spellcheck: 'false', autocomplete: 'off' },
    });
    const chartList = el('div', { class: 'st-chart-list', attrs: { role: 'radiogroup', 'aria-label': t('start.charts_of_accounts') } });
    const chartPane = el('div', { class: 'st-chart-pane st-span4' }, chartSearch, chartList);
    const pickBtn = el('button', { class: 'btn', text: t('start.choose_file'), attrs: { type: 'button' } });
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
          parts.push(el('p', { class: 'st-chart-sub', text: t('start.studio_and_network_charts_numbers') }));
          parts.push(...s.studio.map(row));
        }
      }
      if (!parts.length) parts.push(el('p', { class: 'st-empty', text: t('start.no_chart_matches', { query: chartQuery.trim() }) }));
      chartList.replaceChildren(...parts);
    };

    const drawNote = () => {
      let text = '';
      if (from === 'chart') {
        const c = chartById(chartId);
        text = c ? t('start.note.chart', { name: c.name, accounts: accountsText(c.accounts), count: c.categories }) : t('start.note.choose_chart');
      } else if (from === 'starter') text = t('start.note.starter');
      else if (from === 'blank') text = t('start.note.blank');
      else if (from === 'file') {
        text = picked
          ? t('start.note.chart', { name: picked.file, accounts: accountsText(picked.accounts) || t('start.note.no_accounts'), count: picked.categories })
          : t('start.choose_file_to_start');
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
      fileAside.textContent = projectBox.checked && folder ? t('start.file_in_new_folder') : t('start.file_never_over');
    };
    projectBox.addEventListener('change', () => {
      writePref('newBudget.projectFolder', projectBox.checked);
      showProject();
    });
    const folderBtn = el('button', { class: 'btn', text: t('start.choose_folder'), attrs: { type: 'button', 'aria-describedby': 'st-folder-text' } });
    const showFolder = () => {
      folderText.textContent = folder ? folder.folder : t('start.no_folder_chosen');
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
    const scheduleBtn = el('button', { class: 'btn', text: t('start.link_a_schedule'), attrs: { type: 'button', 'aria-describedby': 'st-schedule-text' } });
    const unlinkBtn = el('button', { class: 'btn btn-quiet', text: t('start.unlink'), attrs: { type: 'button' } });
    const showSchedule = () => {
      unlinkBtn.hidden = !linked;
      scheduleText.classList.toggle('st-folder-none', !linked);
      scheduleText.textContent = linked
        ? t('start.schedule_linked', { file: linked.file, schedule: linked.schedule || t('start.a_schedule'), count: linked.shoot_days })
        : t('start.no_schedule_linked');
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

    const fileName = field('file_name', t('start.field.file_name'), { span: 3, max: 200 });
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
        const detail = lib.problem || (empty ? t('start.library_empty') : t('start.items', { count: lib.items }));
        rows.push(
          tickRow('st-setup-library', libraryTicked, !!lib.problem || empty, t('start.whole_library', { name: lib.name }), detail, (on) => {
            libraryTicked = on;
          }),
        );
      }
      (setupData.packs || []).forEach((p, i) => {
        const pk = packText(p);
        const unusable = !p.items;
        rows.push(
          tickRow(`st-setup-pack-${i}`, packsTicked.has(packKey(p)), unusable, pk.name, p.problem && unusable ? p.problem : pk.detail, (on) => {
            if (on) packsTicked.add(packKey(p));
            else packsTicked.delete(packKey(p));
          }),
        );
      });
      if (!rows.length) {
        rows.push(el('p', { class: 'st-aside', text: t('start.no_setup_library_or_packs') }));
      } else {
        rows.push(el('p', { class: 'st-aside', text: t('start.what_you_tick_takes_the') }));
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
    const back = el('button', { class: 'btn', text: t('start.back'), attrs: { type: 'button' } });
    back.addEventListener('click', showHome);
    const create = el('button', { class: 'btn btn-primary', attrs: { type: 'button' } }, t('start.create_budget'), keyHint('mod+enter'));

    const startValue = () => (from === 'chart' ? `chart:${chartId}` : from);

    const submit = () =>
      once(async () => {
        const values = {};
        for (const [k, input] of Object.entries(inputs)) values[k] = input.value;
        const spec = specFrom(values, startValue(), picked ? picked.token : null, folder ? folder.token : null, setupTicks(), projectBox.checked);
        if (linked && Number.isSafeInteger(linked.token)) spec.schedule = linked.token;
        if (!spec.title) {
          showError(t('start.give_a_title'));
          inputs.title.focus();
          return;
        }
        if (from === 'file' && !picked) {
          showError(t('start.choose_file_to_start'));
          pickBtn.focus();
          return;
        }
        create.disabled = true;
        try {
          const r = await ctx.invoke('new_budget_create', { spec });
          if (r && r.taken) {
            // A file of that name is in the folder: nothing was made.
            const use = el('button', { class: 'btn btn-quiet', text: t('start.use_mthd', { taken: r.taken }), attrs: { type: 'button' } });
            use.addEventListener('click', () => {
              inputs.file_name.value = r.taken;
              fileTouched = true;
              error.hidden = true;
              submit();
            });
            showError(r.message || t('start.name_taken'), use);
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
      heading(t('start.heading.chart'), 'st-from-title'),
      fromRow,
      chartPane,
      filePane,
      fromNote,
      heading(t('start.heading.setup'), 'st-setup-title'),
      setupList,
      heading(t('start.heading.save_in')),
      el('div', { class: 'st-folder st-span4' }, folderText, folderBtn),
      el('label', { class: 'rs-check st-span4', attrs: { for: 'st-project' } }, projectBox, el('span', { text: t('start.make_a_project_folder_for') })),
      fileName,
      fileAside,
      heading(t('start.heading.dates')),
      date('prep_start', t('start.field.prep_start')),
      date('shoot_start', t('start.field.shoot_start')),
      date('shoot_end', t('start.field.shoot_end')),
      date('wrap_end', t('start.field.wrap_end')),
      heading(t('start.heading.schedule')),
      num('prep_weeks', t('start.field.prep_weeks')),
      num('shoot_days', t('start.field.shoot_days')),
      num('wrap_weeks', t('start.field.wrap_weeks')),
      el('span', { class: 'st-span1 st-aside', text: t('start.kept_as_globals_for_formulas') }),
      el('div', { class: 'st-folder st-span4' }, scheduleText, unlinkBtn, scheduleBtn),
      heading(t('start.heading.location')),
      field('country', t('start.field.country')),
      field('region', t('start.field.region')),
      field('city', t('start.field.city')),
      field('currency', t('start.field.currency'), { placeholder: t('start.usd'), max: 10, value: readPref('newBudget.currency', '') || '' }),
      heading(t('start.heading.settings')),
      select(
        'fringe_posting',
        t('start.field.fringe_posting'),
        [
          ['budget', t('start.posting.budget')],
          ['category', t('start.posting.category')],
          ['account', t('start.posting.account')],
          ['production_level', t('start.posting.production_level')],
        ],
        { span: 2, value: 'budget' },
      ),
      select(
        'arithmetic',
        t('start.field.arithmetic'),
        [
          ['binary', t('start.arithmetic.binary')],
          ['exact', t('start.arithmetic.exact')],
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
      el('h2', { class: 'modal-title', text: t('start.new_budget'), attrs: { id: 'st-new-title' } }),
      el('p', {
        class: 'modal-message',
        text: t('start.only_the_title_is_needed'),
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
