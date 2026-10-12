// The start window (roadmap 3.19; decisions.md, entry 63; design:
// docs/research/new-project-window.md; src/start.rs): pinned and recent
// budgets with their net totals, search, open, pin, show in folder and
// take off the list, and the New Budget form. main.js shows it at launch
// when nothing else is open, and for File > New… and the toolbar's New.
//
// Rust holds every path: a row is named by the token of the last listing
// (start_list), and the budget file New Budget starts from is picked in
// Rust's Open dialog and named by its token. Everything typed goes through
// the same checks as the setup tabs (src/start.rs, build).

import { el, hasMod, shortcut } from './dom.js';
import { customDialog } from './modal.js';
import { money } from './format.js';
import { readPref, writePref } from './prefs.js';
import { groupCharts } from './importsetup.js';
import { menuRunsAppKeys } from './shortcuts.js';
import { KINDS, filterEntries, specFrom, startForKind, tvFields, whenText } from './startlist.js';

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
  let charts = [];
  try {
    charts = (await ctx.invoke('chart_templates')) || [];
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
  let busy = false;
  let box = null;
  let close = () => {};
  // New Budget's form, kept while the window is open (Back keeps what was typed).
  let form = null;

  const entries = () => [...(data.pinned || []), ...(data.recent || [])];

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
      const v = await ctx.invoke('open_budget');
      if (!v) return; // cancelled: the window stays
      close();
      ctx.apply(v, 'open_budget');
    });

  /** Redraws from a fresh listing, keeping the focus on the same file. */
  const relist = (next, keep) => {
    data = next;
    drawLists();
    focusRow(keep);
  };

  const pin = (e) =>
    once(async () => relist(await ctx.invoke('start_pin', { token: e.token, pinned: !e.pinned }), e));

  const forget = (e) =>
    once(async () => relist(await ctx.invoke('start_forget', { token: e.token }), e));

  const reveal = (e) => once(() => ctx.invoke('start_reveal', { token: e.token }));

  const totalText = (e) => {
    if (e.problem || e.net_total === null || e.net_total === undefined) return '';
    return `${money(e.net_total)} ${e.currency || ''}`.trim();
  };

  const row = (e) => {
    const where = [e.file, e.folder].filter(Boolean).join('  ');
    const note = e.problem || (e.open ? 'Open in a tab' : '');
    const main = el(
      'button',
      {
        class: 'st-open',
        attrs: { type: 'button', 'aria-label': `${e.title || e.file}, ${where}${note ? `, ${note}` : ''}` },
        data: { key: `${e.folder}\n${e.file}` },
      },
      el('span', { class: 'st-title', text: e.title || e.file }),
      el('span', { class: 'st-total', text: totalText(e) }),
      el('span', { class: 'st-when', text: whenText(e.modified) }),
      el('span', { class: 'st-where', text: where }),
      el('span', { class: e.problem ? 'st-note st-problem' : 'st-note', text: note }),
    );
    main.title = where;
    main.addEventListener('click', () => (e.problem ? null : openEntry(e)));
    main.addEventListener('keydown', (ev) => onRowKey(ev, e));
    const actions = el('span', { class: 'st-actions' });
    const act = (label, fn) => {
      const b = el('button', { class: 'btn btn-quiet st-act', text: label, attrs: { type: 'button' } });
      b.addEventListener('click', () => fn(e));
      actions.append(b);
    };
    if (!e.problem || e.pinned) act(e.pinned ? 'Unpin' : 'Pin', pin);
    if (!e.problem) act('Show in Folder', reveal);
    if (e.recent || e.problem) act('Remove', forget);
    return el('li', { class: `st-row${e.problem ? ' st-missing' : ''}` }, main, actions);
  };

  const section = (title, list) =>
    el(
      'section',
      { class: 'st-section' },
      el('h3', { class: 'st-section-title', text: title }),
      el('ul', { class: 'st-list' }, ...list.map(row)),
    );

  const drawLists = () => {
    const pinned = filterEntries(data.pinned, query);
    const recent = filterEntries(data.recent, query);
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
    if (ev.key === 'p' || ev.key === 'P') {
      ev.preventDefault();
      if (!e.problem || e.pinned) pin(e);
      return;
    }
    if (ev.key === 'Delete' || ev.key === 'Backspace') {
      ev.preventDefault();
      if (e.recent || e.problem) forget(e);
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
      const shown = [...filterEntries(data.pinned, query), ...filterEntries(data.recent, query)].filter((e) => !e.problem);
      if (shown.length) openEntry(shown[0]);
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
    drawLists();
    return [
      el(
        'div',
        { class: 'st-head' },
        el('h2', { class: 'modal-title', text: 'Budgets', attrs: { id: 'st-title' } }),
        newBtn,
        openBtn,
        search,
      ),
      lists,
      el('p', {
        class: 'st-hint',
        text: 'Up and Down move, Enter opens, P pins, Delete takes a budget off the list (the file stays).',
      }),
      el(
        'div',
        { class: 'st-foot' },
        el('label', { class: 'rs-check', attrs: { for: 'st-at-start' } }, atStart, el('span', { text: 'Show this window when the app starts' })),
        closeBtn,
      ),
    ];
  };

  // -------------------------------------------------------------------------
  // New Budget

  const showForm = () => {
    if (!form) form = makeForm();
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
    const heading = (text) => el('h3', { class: 'st-group-title', text });

    const title = field('title', 'Title', { span: 4, max: 200, placeholder: 'The production’s name' });
    const kind = select('kind', 'Production type', KINDS, { span: 2, value: 'feature' });
    const season = field('season', 'Season', { max: 50 });
    const episode = field('episode', 'Episode', { max: 50 });
    const episodes = field('episodes', 'Episodes', { max: 3, placeholder: '10' });
    const date = (key, label) => field(key, label, { placeholder: 'YYYY-MM-DD', max: 10 });
    const num = (key, label) => field(key, label, { max: 12 });

    // Start From: our starting points, the bundled charts by group, a picked file.
    const startSel = el('select', { class: 'modal-input', attrs: { id: 'st-f-start' } });
    const ours = el('optgroup', { attrs: { label: 'Starting points' } });
    ours.append(
      el('option', { text: 'Starter budget: a small sample with placeholder rates', attrs: { value: 'starter' } }),
      el('option', { text: 'Empty: no categories yet', attrs: { value: 'blank' } }),
    );
    startSel.append(ours);
    for (const [group, cs] of groupCharts(charts)) {
      const g = el('optgroup', { attrs: { label: `Charts of accounts: ${group}` } });
      for (const c of cs) g.append(el('option', { text: c.name, attrs: { value: `chart:${c.id}` } }));
      startSel.append(g);
    }
    const fileGroup = el('optgroup', { attrs: { label: 'A budget file' } });
    let picked = null;
    let startTouched = false;
    startSel.value = startForKind('feature', charts) || 'starter';
    startSel.addEventListener('change', () => {
      startTouched = true;
    });
    const pickBtn = el('button', { class: 'btn', text: 'Use a Budget File…', attrs: { type: 'button' } });
    pickBtn.addEventListener('click', () =>
      once(async () => {
        const p = await ctx.invoke('new_budget_pick_template');
        if (!p) return;
        picked = p;
        const label = p.title && p.title !== p.file ? `${p.file} (${p.title})` : p.file;
        fileGroup.replaceChildren(el('option', { text: label, attrs: { value: 'file' } }));
        if (!fileGroup.parentNode) startSel.append(fileGroup);
        startSel.value = 'file';
        startTouched = true;
        startSel.focus();
      }),
    );

    const tvShown = () => {
      const tv = tvFields(inputs.kind.value);
      season.hidden = !tv.season;
      episode.hidden = !tv.episode;
      episodes.hidden = !tv.episodes;
    };
    inputs.kind.addEventListener('change', () => {
      tvShown();
      const s = startTouched ? null : startForKind(inputs.kind.value, charts);
      if (s) startSel.value = s;
    });
    tvShown();

    const error = el('p', { class: 'modal-error', attrs: { role: 'alert' } });
    error.hidden = true;
    const back = el('button', { class: 'btn', text: 'Back', attrs: { type: 'button' } });
    back.addEventListener('click', showHome);
    const create = el('button', { class: 'btn btn-primary', attrs: { type: 'button' } }, 'Create Budget', keyHint('mod+enter'));

    const submit = () =>
      once(async () => {
        const values = {};
        for (const [k, input] of Object.entries(inputs)) values[k] = input.value;
        const spec = specFrom(values, startSel.value, picked ? picked.token : null);
        if (!spec.title) {
          error.textContent = 'Give the budget a title.';
          error.hidden = false;
          inputs.title.focus();
          return;
        }
        create.disabled = true;
        try {
          const r = await ctx.invoke('new_budget_create', { spec });
          if (spec.currency) writePref('newBudget.currency', spec.currency);
          close();
          if (r && r.view) ctx.apply(r.view, 'new_budget_create');
          if (r && r.message) ctx.toast(r.message);
        } catch (e) {
          error.textContent = String(e);
          error.hidden = false;
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
      heading('Location and money'),
      field('country', 'Country'),
      field('region', 'Region or state'),
      field('city', 'City'),
      field('currency', 'Base currency', { placeholder: 'USD', max: 10, value: readPref('newBudget.currency', '') || '' }),
      heading('Start from'),
      el('label', { class: 'modal-field st-span3', attrs: { for: startSel.id } }, 'Chart of accounts or budget', startSel),
      el('div', { class: 'st-span1 st-pick' }, pickBtn),
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
    // Enter in a text box, or Cmd+Enter anywhere, creates the budget.
    grid.addEventListener('keydown', (ev) => {
      if (ev.key !== 'Enter' || ev.isComposing) return;
      const inText = ev.target instanceof HTMLInputElement;
      if (hasMod(ev) || (inText && !ev.shiftKey && !ev.altKey)) {
        ev.preventDefault();
        submit();
      }
    });
    const parts = [
      el('h2', { class: 'modal-title', text: 'New Budget', attrs: { id: 'st-new-title' } }),
      el('p', {
        class: 'modal-message',
        text: 'Only the title is needed. The rest goes into the budget info, globals and settings, where it can be changed later.',
      }),
      el('div', { class: 'st-form-scroll' }, grid),
      error,
      el('div', { class: 'modal-actions' }, back, create),
    ];
    return { parts, title: inputs.title };
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
