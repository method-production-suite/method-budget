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
//   new tab says once where it came from (the view's notice; decisions.md,
//   entry 67).
//
// Everything is drawn with el() and textContent: text from a file is never
// read as HTML.

import { el } from './dom.js';
import { money } from './format.js';
import { customDialog } from './modal.js';
import { plural } from './util.js';

/** How each kind of difference reads in the list. */
const KIND_LABELS = {
  line: 'Line',
  account: 'Account',
  account_fringes: 'Account fringes',
  fringe_account: 'Fringe account',
  category: 'Category',
  category_fringes: 'Category fringes',
  fringe_row: 'Fringe row',
  charge: 'Charge',
  credit: 'Credit',
  grand_total: '',
  net_total: '',
};

/** The headings the notes are listed under, by kind. */
const NOTE_GROUPS = [
  ['check', 'To check: worked out from the export’s figures', true],
  ['not_imported', 'Not in the export, or not brought over', true],
  ['info', 'Notes', false],
];

/**
 * The review's title, introduction and button: converting an export that
 * was opened, importing one picked with Import Budget Export….
 */
export function reviewWording(p) {
  if (p.convert) {
    return {
      title: `Convert budget export: ${p.file}`,
      intro: 'This budget export becomes a new Method Budget tab. Its totals are worked out again and checked against the export’s own, figure by figure. The original file is left as it is. Nothing changes until you convert.',
      button: 'Convert to Method Budget',
    };
  }
  return {
    title: `Import budget export: ${p.file}`,
    intro: 'A whole budget from another program’s export, as a new budget tab. Its totals are worked out again and checked against the export’s own, figure by figure. Nothing changes until you import.',
    button: 'Import as New Budget',
  };
}

/** What came over, in one sentence. */
export function cameOver(p) {
  let s = `${plural(p.categories, 'category', 'categories')}, ${plural(p.accounts, 'account', 'accounts')} and ${plural(p.lines, 'line', 'lines')}`;
  if (p.adjustments) s += ` (${plural(p.adjustments, 'adjustment row', 'adjustment rows')})`;
  const setup = [
    plural(p.fringes, 'fringe', 'fringes'),
    plural(p.groups, 'group', 'groups'),
    plural(p.sets, 'set', 'sets'),
    plural(p.locations, 'location', 'locations'),
    plural(p.currencies, 'currency', 'currencies'),
  ];
  s += `; ${setup.join(', ')}`;
  if (p.charges || p.credits) s += `; ${plural(p.charges, 'charge', 'charges')} and ${plural(p.credits, 'credit', 'credits')}`;
  return `${s}.`;
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
    const select = el('select', { class: 'modal-input', attrs: { 'aria-label': 'Read the file as' } });
    for (const f of p.formats) select.append(el('option', { text: f.label, attrs: { value: f.key } }));
    select.value = st.format;
    select.addEventListener('change', () => {
      st.format = select.value;
      reread();
    });
    ui.top.replaceChildren(el('label', { class: 'modal-field si-field' }, el('span', { text: 'Read as' }), select));
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
      parts.push(el('p', { class: 'modal-error', text: `The imported budget’s totals can’t be worked out: ${p.calc_error}` }));
      ui.recon.replaceChildren(...parts);
      return;
    }
    const compared = p.tallies.reduce((a, t) => a + t.compared, 0);
    if (p.matches) {
      parts.push(el('p', { class: 'si-ok', text: `All ${compared} of the export’s figures come out the same (grand total ${money(p.grand)} ${cur}${p.net !== p.grand ? `, net total ${money(p.net)} ${cur}` : ''}).` }));
    } else {
      parts.push(
        el('p', {
          class: 'si-bad',
          text: `${p.difference_count} of the export’s ${compared} figures come out differently. Grand total: export ${money(p.export_grand)} ${cur}, imported ${money(p.grand)} ${cur}.`,
        }),
      );
    }
    const rows = p.tallies.map((t) =>
      el(
        'tr',
        { class: t.different ? 'si-differs' : '' },
        el('td', { class: 'si-desc', text: t.label }),
        el('td', { class: 'num', text: String(t.compared) }),
        el('td', { class: 'num', text: t.different ? String(t.different) : 'none' }),
      ),
    );
    parts.push(
      el(
        'div',
        { class: 'si-table-wrap' },
        el(
          'table',
          { class: 'si-table' },
          el('thead', {}, el('tr', {}, el('th', { text: 'Compared' }), el('th', { class: 'num', text: 'Figures' }), el('th', { class: 'num', text: 'Different' }))),
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
          return el('li', {
            text: `${kind ? `${kind} ` : ''}${d.what}: export ${money(d.export)}, imported ${money(d.imported)}, difference ${money(d.imported - d.export)}`,
          });
        }),
      );
      if (p.difference_count > p.differences.length) list.append(el('li', { class: 'si-more', text: `and ${p.difference_count - p.differences.length} more` }));
      parts.push(el('details', { class: 'si-details', attrs: { open: true } }, el('summary', { text: `Differences (${p.difference_count})` }), list));
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
        el('summary', { text: `${label} (${items.length})` }),
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
      const cancel = el('button', { class: 'btn', text: 'Cancel', attrs: { type: 'button', 'data-autofocus': true } });
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
        el('h3', { class: 'si-heading', text: 'What comes over' }),
        ui.summary,
        el('h3', { class: 'si-heading', text: 'Check against the export’s totals' }),
        ui.recon,
        el('h3', { class: 'si-heading', text: 'Notes' }),
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
