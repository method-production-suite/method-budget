// File > Import Schedule Days… (src/schedule_days.rs; the logic is
// method-session's schedule_days module):
//
// - Rust shows the Open dialog and reads the schedule's days file; the page
//   names it by a token only.
// - The review follows the other imports (sheetimport.js, exchangeimport.js):
//   every figure the file counts, grouped (shoot days, units, sets,
//   locations, weeks, then the elements by category), each with the global
//   it would set, that global's value now and the new one. The page only
//   keeps the picks; Rust works out every row again (schedule_days_review)
//   when a figure is picked or a global renamed. Nothing changes until Set
//   Globals, which is one undo step.
//
// Everything is drawn with el() and textContent: text from a file is never
// read as HTML.

import { el } from './dom.js';
import { customDialog } from './modal.js';
import { plural } from './util.js';

/** The groups the figures are listed under, in order. */
const GROUPS = [
  ['total', 'Shoot days'],
  ['missing', 'Set from this schedule before, no longer in it (tick to set to 0)'],
  ['unit', 'Shoot days by unit'],
  ['set', 'Shoot days by set'],
  ['location', 'Shoot days by location'],
  ['week', 'Shoot days by week'],
];

/** What a row would do, in words. */
export function rowStatus(f) {
  if (f.problem) return f.problem;
  if (f.changed) return 'Changed in the schedule';
  if (f.action === 'add') return 'New global';
  if (f.action === 'update') return 'New value';
  return f.linked ? 'Up to date' : 'Same value';
}

/** The review's first line: the schedule, its sub-board and its days. */
export function scheduleLine(r) {
  const where = [r.board, r.sub_board].filter((s) => s && s.trim()).join(', ');
  let s = `${r.schedule || 'Untitled schedule'}${where ? ` (${where})` : ''}: ${plural(r.shoot_days, 'shoot day', 'shoot days')}`;
  if (r.end && r.end !== r.start) s += `, ${r.start} to ${r.end}`;
  else if (r.shoot_days) s += `, ${r.start}`;
  return `${s}.`;
}

/** What Set Globals would do, in one sentence. */
export function applyLine(r) {
  const parts = [];
  if (r.adds) parts.push(plural(r.adds, 'new global', 'new globals'));
  if (r.updates) parts.push(plural(r.updates, 'global changes', 'globals change'));
  if (r.same) parts.push(plural(r.same, 'global already matches', 'globals already match'));
  if (!parts.length) return 'Nothing is picked.';
  let s = `${parts.join(', ')}.`;
  const unlinked = r.figures.filter((f) => f.picked && !f.linked).length;
  if (unlinked && r.can_apply) s += ` Set Globals marks ${unlinked === 1 ? 'it' : 'them'} as set from this schedule, so the next import finds ${unlinked === 1 ? 'it' : 'them'} again.`;
  return s;
}

/**
 * File > Import Schedule Days…: picks a file (Rust's dialog) and shows the
 * review. `ctx`: { invoke, apply(view), toast, focus(), globals() }.
 */
export async function importScheduleDays(ctx) {
  let picked;
  try {
    picked = await ctx.invoke('schedule_days_pick');
  } catch (e) {
    ctx.toast(e);
    ctx.focus();
    return;
  }
  if (!picked) {
    ctx.focus();
    return;
  }
  showScheduleDays(ctx, picked);
}

/** The review for a picked file. */
export function showScheduleDays(ctx, first) {
  const st = {
    review: first,
    picks: first.figures.filter((f) => f.picked).map((f) => ({ key: f.key, global: f.global })),
    failed: false,
    // Groups opened or closed by hand, by name: kept as the rows redraw.
    open: new Map(),
  };
  let seq = 0;
  let busy = false;
  const ui = {};

  const setError = (msg) => {
    ui.error.textContent = msg ? String(msg) : '';
    ui.error.hidden = !msg;
  };

  const updateButton = () => {
    const r = st.review;
    ui.ok.disabled = busy || st.failed || !r || !r.can_apply;
  };

  const reread = async () => {
    const mine = ++seq;
    busy = true;
    updateButton();
    try {
      const r = await ctx.invoke('schedule_days_review', { token: st.review.token, picks: st.picks });
      if (mine !== seq) return;
      st.review = r;
      st.failed = false;
      setError('');
    } catch (e) {
      if (mine !== seq) return;
      st.failed = true;
      setError(e);
    } finally {
      if (mine === seq) {
        busy = false;
        render();
      }
    }
  };

  const setPick = (f, on, global) => {
    const i = st.picks.findIndex((p) => p.key === f.key);
    if (!on) {
      if (i >= 0) st.picks.splice(i, 1);
    } else if (i >= 0) {
      st.picks[i].global = global;
    } else {
      st.picks.push({ key: f.key, global });
    }
    reread();
  };

  const figureRow = (f) => {
    const cb = el('input', { attrs: { type: 'checkbox', 'aria-label': `Set a global from ${f.label}` }, data: { key: f.key, role: 'use' } });
    cb.checked = f.picked;
    cb.addEventListener('change', () => setPick(f, cb.checked, f.global));
    const name = el('input', {
      class: 'modal-input sd-global',
      attrs: { type: 'text', list: 'sd-globals', spellcheck: 'false', autocomplete: 'off', 'aria-label': `Global for ${f.label}` },
      data: { key: f.key, role: 'global' },
    });
    name.value = f.global;
    name.addEventListener('change', () => setPick(f, true, name.value.trim()));
    const now = f.current === null || f.current === undefined ? 'none' : f.current_value ? `${f.current} (${f.current_value})` : f.current;
    const cls = [f.problem ? 'si-differs' : '', f.picked ? '' : 'sd-off'].filter(Boolean).join(' ');
    return el(
      'tr',
      { class: cls },
      el('td', {}, cb),
      el('td', { class: 'si-desc', text: f.kind === 'element' ? f.name : f.label }),
      el('td', { class: 'num', text: String(f.days) }),
      el('td', {}, name),
      el('td', { class: 'num', text: now }),
      el('td', { class: 'num', text: f.new }),
      // The link's note (found by words only, an id that only half
      // matches, a figure gone) shows ticked or not.
      el('td', { class: f.problem || f.changed ? 'si-bad' : '', text: [f.picked ? rowStatus(f) : '', f.note || ''].filter(Boolean).join(' ') }),
    );
  };

  const table = (rows) =>
    el(
      'div',
      { class: 'si-table-wrap' },
      el(
        'table',
        { class: 'si-table sd-table' },
        el(
          'thead',
          {},
          el(
            'tr',
            {},
            el('th', { text: 'Use' }),
            el('th', { text: 'Figure' }),
            el('th', { class: 'num', text: 'Days' }),
            el('th', { text: 'Global' }),
            el('th', { class: 'num', text: 'Now' }),
            el('th', { class: 'num', text: 'New' }),
            el('th', { text: '' }),
          ),
        ),
        el('tbody', {}, ...rows.map(figureRow)),
      ),
    );

  const renderTop = () => {
    const r = st.review;
    const parts = [el('p', { class: 'si-summary-main', text: scheduleLine(r) })];
    if (r.generated) parts.push(el('p', { class: 'si-note', text: `Written ${r.generated}.` }));
    if (r.changed.length) {
      parts.push(el('p', { class: 'si-bad', text: `The schedule changed since these globals were set from it: ${r.changed.join(', ')}.` }));
    }
    if (r.notes.length) {
      parts.push(
        el('details', { class: 'si-details', attrs: { open: true } }, el('summary', { text: `Notes on the file (${r.notes.length})` }), el('ul', { class: 'si-notes' }, ...r.notes.map((n) => el('li', { text: n })))),
      );
    }
    ui.top.replaceChildren(...parts);
  };

  const group = (name, open, rows) => {
    const d = el('details', { class: 'si-details', attrs: { open: st.open.has(name) ? st.open.get(name) : open } }, el('summary', { text: `${name} (${rows.length})` }), table(rows));
    d.addEventListener('toggle', () => st.open.set(name, d.open));
    return d;
  };

  const renderFigures = () => {
    const r = st.review;
    // The control with the focus, found again after the rows are redrawn.
    const a = document.activeElement;
    const focused = a && a.dataset && a.dataset.key ? [a.dataset.key, a.dataset.role] : null;
    const parts = [];
    for (const [kind, label] of GROUPS) {
      const rows = r.figures.filter((f) => f.kind === kind);
      if (!rows.length) continue;
      const open = kind === 'total' || kind === 'missing' || kind === 'location' || rows.some((f) => f.picked);
      parts.push(group(label, open, rows));
    }
    const elements = r.figures.filter((f) => f.kind === 'element');
    const categories = [...new Set(elements.map((f) => f.category || 'Other'))];
    for (const c of categories) {
      const rows = elements.filter((f) => (f.category || 'Other') === c);
      parts.push(group(`${c} days`, rows.some((f) => f.picked), rows));
    }
    ui.figures.replaceChildren(...parts);
    if (focused) {
      const again = [...ui.figures.querySelectorAll('[data-key]')].find((n) => n.dataset.key === focused[0] && n.dataset.role === focused[1]);
      if (again) again.focus({ preventScroll: true });
    }
  };

  const renderSummary = () => {
    const r = st.review;
    const parts = [el('p', { class: r.problems.length ? 'si-bad' : 'si-note', text: applyLine(r) })];
    for (const p of r.problems) parts.push(el('p', { class: 'modal-error', text: p }));
    ui.summary.replaceChildren(...parts);
  };

  const renderGlobals = () => {
    const names = (ctx.globals && ctx.globals()) || [];
    ui.list.replaceChildren(...names.map((n) => el('option', { attrs: { value: n } })));
  };

  function render() {
    if (!st.failed) {
      renderTop();
      renderFigures();
      renderSummary();
    }
    updateButton();
  }

  customDialog(
    (closeDialog) => {
      const title = el('h2', { class: 'modal-title', text: `Import schedule days: ${first.file}`, attrs: { id: 'sd-title' } });
      const intro = el('p', {
        class: 'modal-message',
        text: 'Day counts from a schedule set the budget’s globals. Shoot days in all go to SHOOT_DAYS; tick any other figure to set a global from it, and name the global. Prep and wrap globals are left alone. Nothing changes until you set the globals.',
      });
      ui.top = el('div', { class: 'si-summary', attrs: { 'aria-live': 'polite' } });
      ui.figures = el('div', { class: 'si-detail-list' });
      ui.summary = el('div', { class: 'si-summary', attrs: { 'aria-live': 'polite' } });
      ui.list = el('datalist', { attrs: { id: 'sd-globals' } });
      ui.error = el('p', { class: 'modal-error', attrs: { role: 'alert' } });
      ui.error.hidden = true;
      const cancel = el('button', { class: 'btn', text: 'Cancel', attrs: { type: 'button', 'data-autofocus': true } });
      ui.ok = el('button', { class: 'btn btn-primary', text: 'Set Globals', attrs: { type: 'button' } });
      cancel.addEventListener('click', () => closeDialog());
      ui.ok.addEventListener('click', async () => {
        if (ui.ok.disabled) return;
        busy = true;
        updateButton();
        try {
          const r = await ctx.invoke('schedule_days_apply', { token: st.review.token, picks: st.picks });
          closeDialog();
          if (r && r.view) ctx.apply(r.view);
          if (r && r.message) ctx.toast(r.message);
        } catch (e) {
          setError(e);
          busy = false;
          updateButton();
        }
      });
      const scroll = el(
        'div',
        { class: 'si-scroll' },
        ui.top,
        el('h3', { class: 'si-heading', text: 'Figures and globals' }),
        ui.figures,
        el('h3', { class: 'si-heading', text: 'What Set Globals does' }),
        ui.summary,
        ui.list,
      );
      ui.root = el(
        'div',
        { class: 'modal si-dialog', attrs: { role: 'dialog', 'aria-modal': 'true', 'aria-labelledby': 'sd-title' } },
        title,
        intro,
        scroll,
        ui.error,
        el('div', { class: 'modal-actions' }, cancel, ui.ok),
      );
      return ui.root;
    },
    {
      onClose: () => {
        seq++;
        ctx.focus();
      },
    },
  );

  renderGlobals();
  render();
}
