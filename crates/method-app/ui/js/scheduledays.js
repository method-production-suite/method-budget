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
import { t } from './i18n.js';

/** The groups the figures are listed under, in order: [kind, heading with the count]. */
const GROUPS = [
  ['total', (count) => t('scheduledays.group.total', { count })],
  ['missing', (count) => t('scheduledays.group.missing', { count })],
  ['unit', (count) => t('scheduledays.group.unit', { count })],
  ['set', (count) => t('scheduledays.group.set', { count })],
  ['location', (count) => t('scheduledays.group.location', { count })],
  ['week', (count) => t('scheduledays.group.week', { count })],
];

/** What a row would do, in words. */
export function rowStatus(f) {
  if (f.problem) return f.problem;
  if (f.changed) return t('scheduledays.status.changed');
  if (f.action === 'add') return t('scheduledays.status.new_global');
  if (f.action === 'update') return t('scheduledays.status.new_value');
  return f.linked ? t('scheduledays.status.up_to_date') : t('scheduledays.status.same_value');
}

/** The review's first line: the schedule, its sub-board and its days. */
export function scheduleLine(r) {
  const where = [r.board, r.sub_board].filter((s) => s && s.trim()).join(', ');
  const name = r.schedule || t('scheduledays.untitled');
  const schedule = where ? `${name} (${where})` : name;
  const days = t('scheduledays.n_shoot_days', { count: r.shoot_days });
  if (r.end && r.end !== r.start) return t('scheduledays.line_range', { schedule, days, start: r.start, end: r.end });
  if (r.shoot_days) return t('scheduledays.line_start', { schedule, days, start: r.start });
  return t('scheduledays.line', { schedule, days });
}

/** What Set Globals would do, in one sentence. */
export function applyLine(r) {
  const parts = [];
  if (r.adds) parts.push(t('scheduledays.apply.adds', { count: r.adds }));
  if (r.updates) parts.push(t('scheduledays.apply.updates', { count: r.updates }));
  if (r.same) parts.push(t('scheduledays.apply.same', { count: r.same }));
  if (!parts.length) return t('scheduledays.apply.nothing');
  let s = `${parts.join(', ')}.`;
  const unlinked = r.figures.filter((f) => f.picked && !f.linked).length;
  if (unlinked && r.can_apply) s += ` ${t('scheduledays.apply.marks', { count: unlinked })}`;
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
    const cb = el('input', { attrs: { type: 'checkbox', 'aria-label': t('scheduledays.use_aria', { figure: f.label }) }, data: { key: f.key, role: 'use' } });
    cb.checked = f.picked;
    cb.addEventListener('change', () => setPick(f, cb.checked, f.global));
    const name = el('input', {
      class: 'modal-input sd-global',
      attrs: { type: 'text', list: 'sd-globals', spellcheck: 'false', autocomplete: 'off', 'aria-label': t('scheduledays.global_aria', { figure: f.label }) },
      data: { key: f.key, role: 'global' },
    });
    name.value = f.global;
    name.addEventListener('change', () => setPick(f, true, name.value.trim()));
    const now = f.current === null || f.current === undefined ? t('scheduledays.none') : f.current_value ? `${f.current} (${f.current_value})` : f.current;
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
            el('th', { text: t('scheduledays.col.use') }),
            el('th', { text: t('scheduledays.col.figure') }),
            el('th', { class: 'num', text: t('scheduledays.col.days') }),
            el('th', { text: t('scheduledays.col.global') }),
            el('th', { class: 'num', text: t('scheduledays.col.now') }),
            el('th', { class: 'num', text: t('scheduledays.col.new') }),
            el('th', { text: '' }),
          ),
        ),
        el('tbody', {}, ...rows.map(figureRow)),
      ),
    );

  const renderTop = () => {
    const r = st.review;
    const parts = [el('p', { class: 'si-summary-main', text: scheduleLine(r) })];
    if (r.generated) parts.push(el('p', { class: 'si-note', text: t('scheduledays.written', { when: r.generated }) }));
    if (r.changed.length) {
      parts.push(el('p', { class: 'si-bad', text: t('scheduledays.changed', { names: r.changed.join(', ') }) }));
    }
    if (r.notes.length) {
      parts.push(
        el('details', { class: 'si-details', attrs: { open: true } }, el('summary', { text: t('scheduledays.notes', { count: r.notes.length }) }), el('ul', { class: 'si-notes' }, ...r.notes.map((n) => el('li', { text: n })))),
      );
    }
    ui.top.replaceChildren(...parts);
  };

  // `id`: the group's name in st.open; `heading`: its words, with the count.
  const group = (id, heading, open, rows) => {
    const d = el('details', { class: 'si-details', attrs: { open: st.open.has(id) ? st.open.get(id) : open } }, el('summary', { text: heading }), table(rows));
    d.addEventListener('toggle', () => st.open.set(id, d.open));
    return d;
  };

  const renderFigures = () => {
    const r = st.review;
    // The control with the focus, found again after the rows are redrawn.
    const a = document.activeElement;
    const focused = a && a.dataset && a.dataset.key ? [a.dataset.key, a.dataset.role] : null;
    const parts = [];
    for (const [kind, heading] of GROUPS) {
      const rows = r.figures.filter((f) => f.kind === kind);
      if (!rows.length) continue;
      const open = kind === 'total' || kind === 'missing' || kind === 'location' || rows.some((f) => f.picked);
      parts.push(group(kind, heading(rows.length), open, rows));
    }
    const elements = r.figures.filter((f) => f.kind === 'element');
    const other = t('scheduledays.other_category');
    const categories = [...new Set(elements.map((f) => f.category || other))];
    for (const c of categories) {
      const rows = elements.filter((f) => (f.category || other) === c);
      parts.push(group(`element:${c}`, t('scheduledays.group.category', { category: c, count: rows.length }), rows.some((f) => f.picked), rows));
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
      const title = el('h2', { class: 'modal-title', text: t('scheduledays.title', { file: first.file }), attrs: { id: 'sd-title' } });
      const intro = el('p', {
        class: 'modal-message',
        text: t('scheduledays.intro'),
      });
      ui.top = el('div', { class: 'si-summary', attrs: { 'aria-live': 'polite' } });
      ui.figures = el('div', { class: 'si-detail-list' });
      ui.summary = el('div', { class: 'si-summary', attrs: { 'aria-live': 'polite' } });
      ui.list = el('datalist', { attrs: { id: 'sd-globals' } });
      ui.error = el('p', { class: 'modal-error', attrs: { role: 'alert' } });
      ui.error.hidden = true;
      const cancel = el('button', { class: 'btn', text: t('modal.cancel'), attrs: { type: 'button', 'data-autofocus': true } });
      ui.ok = el('button', { class: 'btn btn-primary', text: t('scheduledays.set_globals'), attrs: { type: 'button' } });
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
        el('h3', { class: 'si-heading', text: t('scheduledays.figures_heading') }),
        ui.figures,
        el('h3', { class: 'si-heading', text: t('scheduledays.apply_heading') }),
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
