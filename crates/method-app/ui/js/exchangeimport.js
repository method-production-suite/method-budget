// File > Import Budget Export… (src/export_import.rs; the importer is
// method_format::import::exchange):
//
// - Rust shows the Open dialog, reads the export and builds the budget; the
//   page names the file by a token only.
// - The review follows Import Spreadsheet (sheetimport.js): what came over,
//   the check against the export's own totals (kind by kind, then every
//   figure that differs), and the notes: what was worked out and should be
//   checked, and what the export can't carry. The layout is detected; it
//   can be read as another. Nothing changes until Import, which opens the
//   budget in a new tab, unsaved.
// - An export opened with Open, from Finder or dropped on the window
//   (`review.convert`) gets the same review, worded as converting it; the
//   new tab shows where it came from in a banner until it is closed or
//   first saved (the view's `converted`, convertbanner.js; decisions.md,
//   entry 67).
//
// Everything is drawn with el() and textContent: text from a file is never
// read as HTML.

import { el } from './dom.js';
import { money } from './format.js';
import { customDialog } from './modal.js';
import { t } from './i18n.js';

/** How each kind of difference reads in the list. */
const KIND_LABELS = {
  line: t('exchangeimport.kind.line'),
  account: t('exchangeimport.kind.account'),
  account_fringes: t('exchangeimport.kind.account_fringes'),
  fringe_account: t('exchangeimport.kind.fringe_account'),
  category: t('exchangeimport.kind.category'),
  category_fringes: t('exchangeimport.kind.category_fringes'),
  fringe_row: t('exchangeimport.kind.fringe_row'),
  charge: t('exchangeimport.kind.charge'),
  credit: t('exchangeimport.kind.credit'),
  grand_total: '',
  net_total: '',
};

/** The headings the notes are listed under (with their count), by kind. */
const NOTE_GROUPS = [
  ['check', (count) => t('exchangeimport.notes.check', { count }), true],
  ['not_imported', (count) => t('exchangeimport.notes.not_imported', { count }), true],
  ['info', (count) => t('exchangeimport.notes.info', { count }), false],
];

/**
 * The review's title, introduction and button: converting an export that
 * was opened, importing one picked with Import Budget Export….
 */
export function reviewWording(p) {
  if (p.convert) {
    return {
      title: t('exchangeimport.convert_title', { file: p.file }),
      intro: t('exchangeimport.convert_intro'),
      button: t('exchangeimport.convert_button'),
    };
  }
  return {
    title: t('exchangeimport.import_title', { file: p.file }),
    intro: t('exchangeimport.import_intro'),
    button: t('exchangeimport.import_button'),
  };
}

/** What came over, in one sentence. */
export function cameOver(p) {
  const n = {
    categories: t('exchangeimport.n.categories', { count: p.categories }),
    accounts: t('exchangeimport.n.accounts', { count: p.accounts }),
    lines: t('exchangeimport.n.lines', { count: p.lines }),
    adjustments: t('exchangeimport.n.adjustments', { count: p.adjustments }),
    fringes: t('exchangeimport.n.fringes', { count: p.fringes }),
    groups: t('exchangeimport.n.groups', { count: p.groups }),
    sets: t('exchangeimport.n.sets', { count: p.sets }),
    locations: t('exchangeimport.n.locations', { count: p.locations }),
    currencies: t('exchangeimport.n.currencies', { count: p.currencies }),
    charges: t('exchangeimport.n.charges', { count: p.charges }),
    credits: t('exchangeimport.n.credits', { count: p.credits }),
  };
  const charges = !!(p.charges || p.credits);
  if (p.adjustments) return charges ? t('exchangeimport.came_over_adjustments_charges', n) : t('exchangeimport.came_over_adjustments', n);
  return charges ? t('exchangeimport.came_over_charges', n) : t('exchangeimport.came_over', n);
}

/**
 * File > Import Budget Export…: picks a file (Rust's dialog) and shows the
 * review. `ctx`: { invoke, apply(view), toast, focus() }.
 */
export async function importBudgetExport(ctx) {
  let picked;
  try {
    picked = await ctx.invoke('export_import_pick');
  } catch (e) {
    ctx.toast(e);
    ctx.focus();
    return;
  }
  if (!picked) {
    ctx.focus();
    return;
  }
  showExportDialog(ctx, picked);
}

/** The review for a picked export (exported for the browser harness). */
export function showExportDialog(ctx, first) {
  const st = { review: first, format: first.format, failed: false };
  let seq = 0;
  let busy = false;
  const ui = {};

  const setError = (msg) => {
    ui.error.textContent = msg ? String(msg) : '';
    ui.error.hidden = !msg;
  };

  const updateButton = () => {
    const p = st.review;
    ui.ok.disabled = busy || st.failed || !p || !!p.calc_error || p.categories === 0;
  };

  const reread = async () => {
    const mine = ++seq;
    busy = true;
    updateButton();
    try {
      const p = await ctx.invoke('export_import_preview', { token: st.review.token, format: st.format });
      if (mine !== seq) return;
      st.review = p;
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

  const renderTop = () => {
    const p = st.review;
    const select = el('select', { class: 'modal-input', attrs: { 'aria-label': t('exchangeimport.read_file_as') } });
    for (const f of p.formats) select.append(el('option', { text: f.label, attrs: { value: f.key } }));
    select.value = st.format;
    select.addEventListener('change', () => {
      st.format = select.value;
      reread();
    });
    ui.top.replaceChildren(el('label', { class: 'modal-field si-field' }, el('span', { text: t('exchangeimport.read_as') }), select));
  };

  const renderSummary = () => {
    const p = st.failed ? null : st.review;
    if (!p) {
      ui.summary.replaceChildren();
      return;
    }
    ui.summary.replaceChildren(
      el('p', { class: 'si-summary-main', text: `${p.title}: ${cameOver(p)}` }),
    );
  };

  const renderCheck = () => {
    const p = st.failed ? null : st.review;
    if (!p) {
      ui.recon.replaceChildren();
      return;
    }
    const cur = p.currency;
    const parts = [];
    if (p.calc_error) {
      parts.push(el('p', { class: 'modal-error', text: t('exchangeimport.calc_error', { error: p.calc_error }) }));
      ui.recon.replaceChildren(...parts);
      return;
    }
    const compared = p.tallies.reduce((a, tally) => a + tally.compared, 0);
    if (p.matches) {
      const totals = { count: compared, grand: money(p.grand), net: money(p.net), currency: cur };
      parts.push(el('p', { class: 'si-ok', text: p.net !== p.grand ? t('exchangeimport.all_same_net', totals) : t('exchangeimport.all_same', totals) }));
    } else {
      parts.push(
        el('p', {
          class: 'si-bad',
          text: t('exchangeimport.some_differ', { different: p.difference_count, count: compared, exported: money(p.export_grand), imported: money(p.grand), currency: cur }),
        }),
      );
    }
    const rows = p.tallies.map((tally) =>
      el(
        'tr',
        { class: tally.different ? 'si-differs' : '' },
        el('td', { class: 'si-desc', text: tally.label }),
        el('td', { class: 'num', text: String(tally.compared) }),
        el('td', { class: 'num', text: tally.different ? String(tally.different) : t('exchangeimport.none_different') }),
      ),
    );
    parts.push(
      el(
        'div',
        { class: 'si-table-wrap' },
        el(
          'table',
          { class: 'si-table' },
          el('thead', {}, el('tr', {}, el('th', { text: t('exchangeimport.col.compared') }), el('th', { class: 'num', text: t('exchangeimport.col.figures') }), el('th', { class: 'num', text: t('exchangeimport.col.different') }))),
          el('tbody', {}, ...rows),
        ),
      ),
    );
    if (p.differences.length) {
      const list = el(
        'ul',
        { class: 'si-notes' },
        ...p.differences.map((d) => {
          const kind = KIND_LABELS[d.kind] === undefined ? d.kind : KIND_LABELS[d.kind];
          const figures = { kind, what: d.what, exported: money(d.export), imported: money(d.imported), difference: money(d.imported - d.export) };
          return el('li', { text: kind ? t('exchangeimport.difference_kind', figures) : t('exchangeimport.difference', figures) });
        }),
      );
      if (p.difference_count > p.differences.length) list.append(el('li', { class: 'si-more', text: t('exchangeimport.and_more', { count: p.difference_count - p.differences.length }) }));
      parts.push(el('details', { class: 'si-details', attrs: { open: true } }, el('summary', { text: t('exchangeimport.differences', { count: p.difference_count }) }), list));
    }
    ui.recon.replaceChildren(...parts);
  };

  const renderNotes = () => {
    const p = st.failed ? null : st.review;
    if (!p) {
      ui.notes.replaceChildren();
      return;
    }
    const parts = NOTE_GROUPS.map(([kind, label, open]) => {
      const items = p.notes.filter((n) => n.kind === kind);
      if (!items.length) return null;
      return el(
        'details',
        { class: 'si-details', attrs: open ? { open: true } : {} },
        el('summary', { text: label(items.length) }),
        el('ul', { class: 'si-notes' }, ...items.map((n) => el('li', { text: n.text }))),
      );
    });
    ui.notes.replaceChildren(...parts.filter(Boolean));
  };

  function render() {
    renderTop();
    renderSummary();
    renderCheck();
    renderNotes();
    updateButton();
  }

  customDialog(
    (closeDialog) => {
      const words = reviewWording(first);
      const title = el('h2', { class: 'modal-title', text: words.title, attrs: { id: 'xi-title' } });
      const intro = el('p', { class: 'modal-message', text: words.intro });
      ui.top = el('div', { class: 'si-top' });
      ui.summary = el('div', { class: 'si-summary', attrs: { 'aria-live': 'polite' } });
      ui.recon = el('div', { class: 'si-recon' });
      ui.notes = el('div', { class: 'si-detail-list' });
      ui.error = el('p', { class: 'modal-error', attrs: { role: 'alert' } });
      ui.error.hidden = true;
      const cancel = el('button', { class: 'btn', text: t('modal.cancel'), attrs: { type: 'button', 'data-autofocus': true } });
      ui.ok = el('button', { class: 'btn btn-primary', text: words.button, attrs: { type: 'button' } });
      cancel.addEventListener('click', () => closeDialog());
      ui.ok.addEventListener('click', async () => {
        if (ui.ok.disabled) return;
        busy = true;
        updateButton();
        try {
          const r = await ctx.invoke('export_import', { token: st.review.token, format: st.format });
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
        el('h3', { class: 'si-heading', text: t('exchangeimport.what_comes_over') }),
        ui.summary,
        el('h3', { class: 'si-heading', text: t('exchangeimport.check_heading') }),
        ui.recon,
        el('h3', { class: 'si-heading', text: t('exchangeimport.notes_heading') }),
        ui.notes,
      );
      ui.root = el(
        'div',
        { class: 'modal si-dialog', attrs: { role: 'dialog', 'aria-modal': 'true', 'aria-labelledby': 'xi-title' } },
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

  render();
}
