// File > Print: the report setup dialog. Choices on the left (sections and
// their order, categories and accounts, what to leave out, detail columns,
// layout, header and footer), the report as it will print on the right, page
// by page, redrawn as the choices change. Saved setups ("report presets") live
// in the budget (src/reports.rs): pick one, save, rename, delete, mark one as
// the default the dialog opens with. Print sends the pages to the print sheet;
// Save as PDF writes them straight into a PDF file (report.js).
//
// A preset that prints the Bid section carries a bid layout the user builds
// here (decisions.md, entry 92): header fields, sections that take accounts,
// subtotals and markups. The preview's figures come from the engine
// (`bid_sheet`), never from the page.
//
// Everything from the budget goes in through textContent or input values.

import { el } from './dom.js';
import { customDialog } from './modal.js';
import { invoke } from './api.js';
import { money } from './format.js';
import { t } from './i18n.js';
import {
  SECTIONS,
  REPORT_COLUMNS,
  STANDARD_COLUMNS,
  STANDARD_HEADER,
  STANDARD_FOOTER,
  normalizeSpec,
  compactSpec,
  defaultSpec,
  renderReport,
  scalePreview,
  printReport,
  withBid,
  bidLayout,
} from './report.js';

const SLOTS = [
  ['left', t('reportsetup.slot.left'), t('reportsetup.slot.left_lower')],
  ['center', t('reportsetup.slot.center'), t('reportsetup.slot.center_lower')],
  ['right', t('reportsetup.slot.right'), t('reportsetup.slot.right_lower')],
];

let uid = 0;

function presetsOf(view) {
  const p = view && view.budget && Array.isArray(view.budget.report_presets) ? view.budget.report_presets : [];
  return p.filter((x) => x && typeof x.name === 'string');
}

/** A checkbox with its label. `key` gives it a stable id, so focus survives a redraw. */
function check(label, checked, onChange, { disabled = false, title = '', key = '' } = {}) {
  const id = key ? `rs-${key}` : `rs-c${(uid += 1)}`;
  const box = el('input', { attrs: { id, type: 'checkbox' } });
  box.checked = checked;
  box.disabled = disabled;
  box.addEventListener('change', () => onChange(box.checked));
  return el('label', { class: 'rs-check', title: title || undefined, attrs: { for: id } }, box, el('span', { text: label }));
}

function group(title, ...children) {
  return el('fieldset', { class: 'rs-group' }, el('legend', { text: title }), ...children);
}

/**
 * Opens the dialog. ctx: { view() (the lean view on screen), call(cmd, args)
 * (a command whose view the app applies), toast(message) }.
 */
export async function showReportSetup(ctx) {
  let full;
  try {
    full = await invoke('get_full_view');
  } catch (e) {
    ctx.toast(e);
    return;
  }
  let presets = presetsOf(full);
  const hasBaseline = Boolean(full.variance || (full.budget && full.budget.baseline));
  const b = full.budget || {};
  const categories = (Array.isArray(b.topsheet) ? b.topsheet : []).filter((r) => r && r.kind !== 'production_total');
  const groupNames = Object.keys(b.groups && typeof b.groups === 'object' ? b.groups : {}).sort();
  const infoLabels = Object.keys(b.info && typeof b.info === 'object' ? b.info : {});
  const globalNames = Object.keys(b.globals && typeof b.globals === 'object' ? b.globals : {}).sort();

  // The preset being edited (an index into presets, or null for a new setup).
  const def = presets.findIndex((p) => p.default === true);
  let current = def >= 0 ? def : null;
  let spec = current === null ? defaultSpec() : normalizeSpec(presets[current]);
  let name = current === null ? '' : presets[current].name;
  let isDefault = current !== null && presets[current].default === true;
  const openCats = new Set();

  let close = null;
  const left = el('div', { class: 'rs-settings' });
  const pages = el('div', { class: 'rs-pages rpt-preview', attrs: { tabindex: '0', 'aria-label': t('reportsetup.report_preview') } });
  const pageCount = el('span', { class: 'rs-count', attrs: { 'aria-live': 'polite' } });
  const jump = el('select', { class: 'rs-jump', attrs: { 'aria-label': t('reportsetup.go_to_section_of_preview') } });
  // Page navigation: first, previous, the page shown (type a number), next, last.
  const navBtn = (text, label) => el('button', { class: 'btn btn-quiet rs-nav', text, title: label, attrs: { type: 'button', 'aria-label': label } });
  const firstBtn = navBtn('«', t('reportsetup.first_page'));
  const prevBtn = navBtn('‹', t('reportsetup.previous_page'));
  const nextBtn = navBtn('›', t('reportsetup.next_page'));
  const lastBtn = navBtn('»', t('reportsetup.last_page'));
  const pageInput = el('input', { class: 'modal-input rs-page-input', attrs: { type: 'text', inputmode: 'numeric', autocomplete: 'off', spellcheck: 'false', 'aria-label': t('reportsetup.page_shown'), size: '3' } });
  const status = el('p', { class: 'rs-status', attrs: { role: 'status' } });
  const presetSelect = el('select', { class: 'rs-preset', attrs: { 'aria-label': t('reportsetup.report_preset') } });
  const nameInput = el('input', { class: 'modal-input rs-name', attrs: { type: 'text', autocomplete: 'off', spellcheck: 'false', placeholder: t('reportsetup.preset_name'), 'aria-label': t('reportsetup.preset_name'), maxlength: '80' } });
  const defaultBox = el('input', { attrs: { type: 'checkbox', id: 'rs-default' } });
  const saveBtn = el('button', { class: 'btn', text: t('reportsetup.save_preset'), attrs: { type: 'button' } });
  const saveNewBtn = el('button', { class: 'btn', text: t('reportsetup.save_as_new'), attrs: { type: 'button' } });
  const deleteBtn = el('button', { class: 'btn btn-quiet btn-danger-text', text: t('reportsetup.delete'), attrs: { type: 'button' } });
  const printBtn = el('button', { class: 'btn btn-primary', text: t('reportsetup.print_dots'), attrs: { type: 'button', 'data-autofocus': '' } });
  const pdfBtn = el('button', { class: 'btn', text: t('reportsetup.save_as_pdf'), attrs: { type: 'button' } });
  const closeBtn = el('button', { class: 'btn', text: t('reportsetup.close'), attrs: { type: 'button' } });

  const say = (text, bad = false) => {
    status.textContent = text || '';
    status.classList.toggle('bad', bad);
  };

  // --- Preview ---------------------------------------------------------

  let timer = 0;
  let sections = [];
  let pageTotal = 0;
  let shown = 1;

  /** The page at the top of the preview (the last one whose top has scrolled past). */
  const pageInView = () => {
    const sheets = pages.querySelectorAll('.rpt-sheet');
    const top = pages.scrollTop + 24;
    let n = 1;
    sheets.forEach((sh, i) => {
      if (sh.offsetTop <= top) n = i + 1;
    });
    return n;
  };
  const drawNav = () => {
    if (document.activeElement !== pageInput) pageInput.value = pageTotal ? String(shown) : '';
    firstBtn.disabled = prevBtn.disabled = shown <= 1;
    nextBtn.disabled = lastBtn.disabled = shown >= pageTotal;
  };
  const goPage = (n) => {
    if (!pageTotal) return;
    const k = Math.min(Math.max(1, n), pageTotal);
    const sheet = pages.querySelector(`.rpt-page[data-page="${k}"]`);
    if (sheet) pages.scrollTop = sheet.parentElement.offsetTop - 16;
    shown = k;
    drawNav();
  };
  let navFrame = 0;
  pages.addEventListener(
    'scroll',
    () => {
      if (navFrame) return;
      navFrame = requestAnimationFrame(() => {
        navFrame = 0;
        shown = pageInView();
        drawNav();
      });
    },
    { passive: true },
  );
  firstBtn.addEventListener('click', () => goPage(1));
  prevBtn.addEventListener('click', () => goPage(shown - 1));
  nextBtn.addEventListener('click', () => goPage(shown + 1));
  lastBtn.addEventListener('click', () => goPage(pageTotal));
  const typedPage = () => {
    const n = Number.parseInt(pageInput.value, 10);
    if (Number.isFinite(n)) goPage(n);
    else drawNav();
  };
  pageInput.addEventListener('change', typedPage);
  pageInput.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      e.stopPropagation();
      typedPage();
    }
  });
  pageInput.addEventListener('blur', drawNav);
  let drawSeq = 0;
  const draw = async () => {
    clearTimeout(timer);
    timer = 0;
    if (!pages.isConnected) return;
    // The bid's figures come from the engine; a later draw wins.
    const seq = (drawSeq += 1);
    const view = await withBid(full, spec);
    if (seq !== drawSeq || !pages.isConnected) return;
    showBidNotes(view);
    const top = pages.scrollTop;
    const out = renderReport(pages, view, spec);
    const width = Math.max(240, pages.clientWidth - 40);
    scalePreview(pages, width, spec.landscape);
    sections = out.sections;
    pageTotal = out.pages;
    pageCount.textContent = t('reportsetup.page_of', { total: out.pages });
    jump.replaceChildren(el('option', { text: t('reportsetup.go_to_section'), attrs: { value: '' } }), ...sections.map((s, i) => el('option', { text: t('reportsetup.section_on_page', { name: s.name, page: s.page }), attrs: { value: String(i) } })));
    pages.scrollTop = top;
    shown = Math.min(pageInView(), Math.max(1, pageTotal));
    drawNav();
  };
  const redraw = () => {
    clearTimeout(timer);
    timer = setTimeout(draw, 150);
  };
  jump.addEventListener('change', () => {
    const s = sections[Number(jump.value)];
    jump.value = '';
    if (!s) return;
    const sheet = pages.querySelector(`.rpt-page[data-page="${s.page}"]`);
    if (sheet) pages.scrollTop = sheet.parentElement.offsetTop - 16;
    shown = s.page;
    drawNav();
  });
  // The pages don't depend on the pane's size, only their scale does.
  const resize = new ResizeObserver(() => {
    if (pages.isConnected) scalePreview(pages, Math.max(240, pages.clientWidth - 40), spec.landscape);
  });

  // --- The choices -----------------------------------------------------

  const change = (fn) => {
    fn();
    say('');
    drawSettings();
    redraw();
  };

  function sectionsGroup() {
    const on = spec.sections;
    const order = [...on, ...SECTIONS.map((s) => s.key).filter((k) => !on.includes(k))];
    const list = el('ol', { class: 'rs-sections' });
    order.forEach((key) => {
      const s = SECTIONS.find((x) => x.key === key);
      const i = on.indexOf(key);
      const item = el('li', { class: `rs-section${i < 0 ? ' off' : ''}` });
      item.append(
        check(
          s.label,
          i >= 0,
          (v) => {
            change(() => {
              spec.sections = v ? [...on, key] : on.filter((k) => k !== key);
            });
            if (key === 'bid' && v && !spec.bid) startBid();
          },
          { key: `sec-${key}` },
        ),
      );
      const move = (d) =>
        change(() => {
          const next = [...on];
          const j = i + d;
          [next[i], next[j]] = [next[j], next[i]];
          spec.sections = next;
        });
      const up = el('button', { class: 'btn btn-quiet rs-move', text: '↑', title: t('reportsetup.print_earlier', { name: s.label }), attrs: { type: 'button', id: `rs-up-${key}`, 'aria-label': t('reportsetup.move_up', { name: s.label }) } });
      const down = el('button', { class: 'btn btn-quiet rs-move', text: '↓', title: t('reportsetup.print_later', { name: s.label }), attrs: { type: 'button', id: `rs-down-${key}`, 'aria-label': t('reportsetup.move_down', { name: s.label }) } });
      up.disabled = i <= 0;
      down.disabled = i < 0 || i >= on.length - 1;
      up.addEventListener('click', () => move(-1));
      down.addEventListener('click', () => move(1));
      item.append(el('span', { class: 'rs-moves' }, up, down));
      list.append(item);
    });
    return group(t('reportsetup.sections_in_print_order'), list);
  }

  function treeGroup() {
    const exC = new Set(spec.excluded_categories);
    const exA = new Set(spec.excluded_accounts);
    const tree = el('ul', { class: 'rs-tree' });
    const allIn = categories.every((c) => !exC.has(c.code || '') && (c.accounts || []).every((a) => !exA.has(a.code || '')));
    const all = check(t('reportsetup.all_categories_and_accounts'), allIn, (v) =>
      change(() => {
        spec.excluded_categories = v ? [] : categories.map((c) => c.code || '');
        spec.excluded_accounts = [];
      }),
      { key: 'all' },
    );
    categories.forEach((c, ci) => {
      const code = c.code || '';
      const accounts = Array.isArray(c.accounts) ? c.accounts : [];
      const catIn = !exC.has(code);
      const someOut = accounts.some((a) => exA.has(a.code || ''));
      const row = el('li', { class: 'rs-cat' });
      const toggle = el('button', {
        class: 'btn btn-quiet rs-twisty',
        text: openCats.has(code) ? '▾' : '▸',
        attrs: { type: 'button', id: `rs-open-${ci}`, 'aria-expanded': String(openCats.has(code)), 'aria-label': t('reportsetup.accounts_in', { code, name: c.name || '' }).trim() },
      });
      toggle.disabled = accounts.length === 0;
      toggle.addEventListener('click', () => {
        if (openCats.has(code)) openCats.delete(code);
        else openCats.add(code);
        drawSettings();
      });
      const box = check(`${code}  ${c.name || ''}`.trim(), catIn, (v) =>
        change(() => {
          const s = new Set(spec.excluded_categories);
          if (v) s.delete(code);
          else s.add(code);
          spec.excluded_categories = [...s];
          // Checking a category brings all its accounts back.
          if (v) {
            const codes = new Set(accounts.map((a) => a.code || ''));
            spec.excluded_accounts = spec.excluded_accounts.filter((x) => !codes.has(x));
          }
        }),
        { key: `cat-${ci}` },
      );
      const input = box.querySelector('input');
      input.indeterminate = catIn && someOut;
      row.append(el('div', { class: 'rs-cat-row' }, toggle, box));
      if (openCats.has(code) && accounts.length) {
        const list = el('ul', { class: 'rs-accts' });
        accounts.forEach((a0, ai) => {
          const a = a0 || {};
          const acode = a.code || '';
          list.append(
            el(
              'li',
              {},
              check(`${acode}  ${a.name || ''}`.trim(), catIn && !exA.has(acode), (v) =>
                change(() => {
                  const s = new Set(spec.excluded_accounts);
                  if (v) s.delete(acode);
                  else s.add(acode);
                  spec.excluded_accounts = [...s];
                  if (v) spec.excluded_categories = spec.excluded_categories.filter((x) => x !== code);
                }),
                { disabled: !catIn, key: `acct-${ci}-${ai}` },
              ),
            ),
          );
        });
        row.append(list);
      }
      tree.append(row);
    });
    if (!categories.length) tree.append(el('li', { class: 'rs-hint', text: t('reportsetup.no_categories_yet') }));
    return group(t('reportsetup.categories_and_accounts'), el('p', { class: 'rs-hint', text: t('reportsetup.categories_hint') }), all, tree);
  }

  function leaveOutGroup() {
    const flag = (key, label) => check(label, spec[key], (v) => change(() => (spec[key] = v)), { key });
    const parts = [
      flag('skip_empty_accounts', t('reportsetup.skip.empty_accounts')),
      flag('skip_zero_accounts', t('reportsetup.skip.zero_accounts')),
      flag('skip_suppressed_lines', t('reportsetup.skip.suppressed_lines')),
      flag('skip_notes', t('reportsetup.skip.notes')),
      flag('skip_done_notes', t('reportsetup.skip.done_notes')),
    ];
    if (groupNames.length) {
      const only = new Set(spec.only_groups);
      const list = el('div', { class: 'rs-groups' });
      groupNames.forEach((g, gi) => {
        list.append(
          check(g, only.has(g), (v) =>
            change(() => {
              const s = new Set(spec.only_groups);
              if (v) s.add(g);
              else s.delete(g);
              spec.only_groups = [...s];
            }),
            { key: `group-${gi}` },
          ),
        );
      });
      parts.push(el('p', { class: 'rs-sub', text: t('reportsetup.only_groups_hint') }), list);
    }
    return group(t('reportsetup.leave_out'), ...parts, el('p', { class: 'rs-hint', text: t('reportsetup.leave_out_hint') }));
  }

  function columnsGroup() {
    const chosen = new Set(spec.columns || STANDARD_COLUMNS);
    const box = el('div', { class: 'rs-columns' });
    for (const c of REPORT_COLUMNS) {
      const base = c.key === 'original' || c.key === 'variance';
      box.append(
        check(
          c.name || c.label,
          chosen.has(c.key),
          (v) =>
            change(() => {
              const cur = new Set(spec.columns || STANDARD_COLUMNS);
              if (v) cur.add(c.key);
              else cur.delete(c.key);
              spec.columns = REPORT_COLUMNS.map((x) => x.key).filter((k) => cur.has(k));
            }),
          { key: `col-${c.key}`, disabled: base && !hasBaseline, title: base && !hasBaseline ? t('reportsetup.lock_baseline_tip') : '' },
        ),
      );
    }
    const reset = el('button', { class: 'btn btn-quiet rs-reset', text: t('reportsetup.standard_columns'), attrs: { type: 'button', id: 'rs-std-cols' } });
    reset.disabled = !spec.columns;
    reset.addEventListener('click', () => change(() => (spec.columns = null)));
    return group(
      t('reportsetup.detail_columns'),
      el('p', { class: 'rs-hint', text: t('reportsetup.detail_columns_hint') }),
      box,
      reset,
    );
  }

  function layoutGroup() {
    const orient = el('div', { class: 'rs-radios', attrs: { role: 'radiogroup', 'aria-label': t('reportsetup.orientation') } });
    for (const [v, label] of [
      [false, t('reportsetup.portrait')],
      [true, t('reportsetup.landscape')],
    ]) {
      const id = `rs-orient-${v ? 'l' : 'p'}`;
      const r = el('input', { attrs: { id, type: 'radio', name: 'rs-orient' } });
      r.checked = spec.landscape === v;
      r.addEventListener('change', () => change(() => (spec.landscape = v)));
      orient.append(el('label', { class: 'rs-check', attrs: { for: id } }, r, el('span', { text: label })));
    }
    const size = el('select', { class: 'modal-input rs-select', attrs: { id: 'rs-size', 'aria-label': t('reportsetup.type_size') } });
    for (const [v, label] of [
      ['small', t('reportsetup.size.small')],
      ['medium', t('reportsetup.size.medium')],
      ['large', t('reportsetup.size.large')],
    ]) {
      const o = el('option', { text: label });
      o.value = v;
      size.append(o);
    }
    size.value = spec.font_size;
    size.addEventListener('change', () => change(() => (spec.font_size = size.value)));
    return group(
      t('reportsetup.layout'),
      orient,
      el('label', { class: 'rs-inline' }, el('span', { text: t('reportsetup.type_size') }), size),
      check(t('reportsetup.shading'), spec.shading, (v) => change(() => (spec.shading = v)), { key: 'shading' }),
      check(t('reportsetup.group_colors'), spec.group_colors, (v) => change(() => (spec.group_colors = v)), { key: 'colors' }),
      check(t('reportsetup.vertical_rules'), spec.vertical_rules, (v) => change(() => (spec.vertical_rules = v)), { key: 'rules' }),
      check(t('reportsetup.collapse_details'), spec.collapse_details, (v) => change(() => (spec.collapse_details = v)), {
        key: 'collapse',
        title: t('reportsetup.collapse_details_tip'),
      }),
    );
  }

  function topsheetGroup() {
    const noDetail = !spec.sections.includes('detail');
    return group(
      t('reportsetup.topsheet_columns'),
      check(t('reportsetup.topsheet_fringes'), spec.topsheet_fringes, (v) => change(() => (spec.topsheet_fringes = v)), { key: 'ts-fringes', title: t('reportsetup.topsheet_fringes_tip') }),
      check(t('reportsetup.topsheet_pages'), spec.topsheet_pages, (v) => change(() => (spec.topsheet_pages = v)), {
        key: 'ts-pages',
        title: noDetail ? t('reportsetup.topsheet_pages_blank_tip') : t('reportsetup.topsheet_pages_tip'),
      }),
      noDetail && spec.topsheet_pages ? el('p', { class: 'rs-hint', text: t('reportsetup.topsheet_pages_hint') }) : null,
    );
  }

  function fieldSelect(band, slot, which) {
    const f = band[slot] || null;
    const slotWord = SLOTS.find((s) => s[0] === slot)[2];
    const isHeader = which === 'Header';
    const sel = el('select', {
      class: 'modal-input rs-select',
      attrs: { id: `rs-${which}-${slot}`, 'aria-label': isHeader ? t('reportsetup.header_slot', { slot: slotWord }) : t('reportsetup.footer_slot', { slot: slotWord }) },
    });
    const opts = [
      ['', t('reportsetup.field.nothing')],
      ['title', t('reportsetup.field.title')],
      ['section', t('reportsetup.field.section')],
      ['printed', t('reportsetup.field.printed')],
      ['date', t('reportsetup.field.date')],
      ['time', t('reportsetup.field.time')],
      ['file', t('reportsetup.field.file')],
      ['page', t('reportsetup.field.page')],
      ...infoLabels.map((l) => [`info:${l}`, l]),
      ...globalNames.map((g) => [`global:${g}`, t('reportsetup.field.global', { name: g })]),
      ['text', t('reportsetup.field.text')],
    ];
    if (f && f.kind === 'info' && !infoLabels.includes(f.label)) opts.splice(opts.length - 1, 0, [`info:${f.label}`, f.label]);
    if (f && f.kind === 'global' && !globalNames.includes(f.name)) opts.splice(opts.length - 1, 0, [`global:${f.name}`, t('reportsetup.field.global_missing', { name: f.name })]);
    for (const [v, label] of opts) {
      const o = el('option', { text: label });
      o.value = v;
      sel.append(o);
    }
    sel.value = !f ? '' : f.kind === 'info' ? `info:${f.label}` : f.kind === 'global' ? `global:${f.name}` : f.kind;
    const text = el('input', { class: 'modal-input rs-text', attrs: { type: 'text', maxlength: '200', 'aria-label': isHeader ? t('reportsetup.header_slot_text', { slot: slotWord }) : t('reportsetup.footer_slot_text', { slot: slotWord }), placeholder: t('reportsetup.text') } });
    text.value = f && f.kind === 'text' ? f.text : '';
    text.hidden = !(f && f.kind === 'text');
    const key = isHeader ? 'header' : 'footer';
    const set = (field) => {
      const next = { ...(spec[key] || (key === 'header' ? STANDARD_HEADER : STANDARD_FOOTER)) };
      if (field) next[slot] = field;
      else delete next[slot];
      spec[key] = next;
    };
    sel.addEventListener('change', () => {
      const v = sel.value;
      if (v === 'text') {
        set({ kind: 'text', text: text.value });
        text.hidden = false;
        redraw();
        text.focus();
        return;
      }
      const field = v === '' ? null : v.startsWith('info:') ? { kind: 'info', label: v.slice(5) } : v.startsWith('global:') ? { kind: 'global', name: v.slice(7) } : { kind: v };
      change(() => set(field));
    });
    text.addEventListener('input', () => {
      set({ kind: 'text', text: text.value });
      redraw();
    });
    return el('div', { class: 'rs-slot' }, el('span', { class: 'rs-slot-label', text: SLOTS.find((s) => s[0] === slot)[1] }), sel, text);
  }

  function bandsGroup() {
    const header = spec.header || STANDARD_HEADER;
    const footer = spec.footer || STANDARD_FOOTER;
    const reset = el('button', { class: 'btn btn-quiet rs-reset', text: t('reportsetup.standard_bands'), attrs: { type: 'button', id: 'rs-std-bands' } });
    reset.disabled = !spec.header && !spec.footer;
    reset.addEventListener('click', () =>
      change(() => {
        spec.header = null;
        spec.footer = null;
      }),
    );
    const firstOnly = (key, label) =>
      check(label, (spec[key] || {}).first_page_only === true, (v) =>
        change(() => {
          const next = { ...(spec[key] || (key === 'header' ? STANDARD_HEADER : STANDARD_FOOTER)) };
          if (v) next.first_page_only = true;
          else delete next.first_page_only;
          spec[key] = next;
        }),
        { key: `${key}-first` },
      );
    return group(
      t('reportsetup.header_and_footer'),
      el('p', { class: 'rs-sub', text: t('reportsetup.header') }),
      ...SLOTS.map(([s]) => fieldSelect(header, s, 'Header')),
      firstOnly('header', t('reportsetup.header_first_only')),
      el('p', { class: 'rs-sub', text: t('reportsetup.footer') }),
      ...SLOTS.map(([s]) => fieldSelect(footer, s, 'Footer')),
      firstOnly('footer', t('reportsetup.footer_first_only')),
      reset,
    );
  }

  // --- The bid layout ------------------------------------------------

  const bidNotes = el('div', { class: 'rs-bid-notes', attrs: { 'aria-live': 'polite' } });
  function showBidNotes(view) {
    const sheet = view && view.bid_sheet;
    const notes = [];
    if (view && view.bid_error) notes.push(view.bid_error);
    if (sheet) {
      const n = Array.isArray(sheet.unassigned) ? sheet.unassigned.length : 0;
      if (n) notes.push(t('reportsetup.unassigned_note', { count: n }));
      for (const w of sheet.warnings || []) notes.push(`${w.charAt(0).toUpperCase()}${w.slice(1)}.`);
    }
    bidNotes.replaceChildren(...notes.map((note) => el('p', { class: 'rs-hint', text: note })));
  }

  /** Starts the layout from the example the engine has (bid_sheet with no layout). */
  async function startBid() {
    try {
      const sheet = await invoke('bid_sheet', { layout: null });
      if (!spec.bid) spec.bid = bidLayout(sheet.layout) || { fields: [], lines: [], show_accounts: false };
      drawSettings();
      redraw();
    } catch (e) {
      say(String(e), true);
    }
  }

  const sameName = (a, b) => a.trim().toLowerCase() === b.trim().toLowerCase();

  /** "10, 11" and "2001, 4000 to 4999" from a section's picks. */
  function pickTexts(picks) {
    const cats = picks.filter((p) => p.kind === 'category').map((p) => p.code);
    const accts = picks.filter((p) => p.kind !== 'category').map((p) => (p.kind === 'range' ? `${p.from} to ${p.to}` : p.code));
    return [cats.join(', '), accts.join(', ')];
  }

  /** A section's picks from its two typed lists. */
  function picksOf(catText, acctText) {
    const list = (t) =>
      t
        .split(',')
        .map((x) => x.trim())
        .filter(Boolean);
    const picks = list(catText).map((code) => ({ kind: 'category', code }));
    for (const item of list(acctText)) {
      const m2 = /^(.+?)\s+to\s+(.+)$/i.exec(item);
      picks.push(m2 ? { kind: 'range', from: m2[1].trim(), to: m2[2].trim() } : { kind: 'account', code: item });
    }
    return picks;
  }

  const PERCENT = /^-?\d+(\.\d+)?$/;
  const AMOUNT = /^-?(\d{1,3}(,\d{3})+|\d+)(\.\d{1,2})?$/;

  /** Typed money ("1,250.50") as cents, or null. No floats. */
  function cents(t) {
    const v = t.trim();
    if (!AMOUNT.test(v)) return null;
    const neg = v.startsWith('-');
    const [int, frac = ''] = v.replace(/^-/, '').replace(/,/g, '').split('.');
    const c = Number(int) * 100 + Number(frac.padEnd(2, '0'));
    return Number.isSafeInteger(c) ? (neg ? -c : c) : null;
  }

  function bidInput(id, label, value, onInput, { placeholder = '', maxlength = '80', cls = '' } = {}) {
    const input = el('input', { class: `modal-input rs-text ${cls}`.trim(), attrs: { id, type: 'text', autocomplete: 'off', spellcheck: 'false', 'aria-label': label, placeholder, maxlength } });
    input.value = value;
    input.addEventListener('input', () => {
      const ok = onInput(input.value);
      input.setAttribute('aria-invalid', ok === false ? 'true' : 'false');
      if (ok !== false) redraw();
    });
    return input;
  }

  function bidButton(id, text, label, onClick, disabled = false) {
    const b2 = el('button', { class: 'btn btn-quiet rs-move', text, title: label, attrs: { type: 'button', id, 'aria-label': label } });
    b2.disabled = disabled;
    b2.addEventListener('click', onClick);
    return b2;
  }

  function bidGroup() {
    if (!spec.sections.includes('bid')) return null;
    const bid = spec.bid;
    if (!bid) {
      const start = el('button', { class: 'btn', text: t('reportsetup.edit_bid_layout'), attrs: { type: 'button', id: 'rs-bid-start' } });
      start.addEventListener('click', startBid);
      return group(t('reportsetup.bid_layout'), el('p', { class: 'rs-hint', text: t('reportsetup.bid_example_hint') }), start, bidNotes);
    }
    const parts = [];
    parts.push(
      check(t('reportsetup.list_section_accounts'), bid.show_accounts, (v) => change(() => (bid.show_accounts = v)), { key: 'bid-accounts' }),
      el('p', { class: 'rs-sub', text: t('reportsetup.header_fields') }),
    );
    bid.fields.forEach((f, i) => {
      parts.push(
        el(
          'div',
          { class: 'rs-slot rs-bid-field' },
          bidInput(`rs-bid-f${i}-label`, t('reportsetup.header_field_label', { n: i + 1 }), f.label, (v) => (f.label = v), { placeholder: t('reportsetup.label') }),
          bidInput(`rs-bid-f${i}-value`, t('reportsetup.header_field_value', { n: i + 1 }), f.value, (v) => (f.value = v), { placeholder: t('reportsetup.value'), maxlength: '200' }),
          bidButton(`rs-bid-f${i}-del`, '×', t('reportsetup.remove_header_field', { name: f.label || i + 1 }), () => change(() => bid.fields.splice(i, 1))),
        ),
      );
    });
    const addField = el('button', { class: 'btn btn-quiet rs-reset', text: t('reportsetup.add_header_field'), attrs: { type: 'button', id: 'rs-bid-add-field' } });
    addField.addEventListener('click', () => change(() => bid.fields.push({ label: t('reportsetup.default.field'), value: '' })));
    parts.push(addField, el('p', { class: 'rs-sub', text: t('reportsetup.lines_in_print_order') }));
    const sectionNames = bid.lines.filter((l) => l.kind === 'section').map((l) => l.name);
    const KIND = { section: t('reportsetup.kind.section'), subtotal: t('reportsetup.kind.subtotal'), percent: t('reportsetup.kind.percent'), flat: t('reportsetup.kind.flat') };
    bid.lines.forEach((l, i) => {
      const id = `rs-bid-l${i}`;
      const box = el('div', { class: 'rs-bid-line' });
      const head = el(
        'div',
        { class: 'rs-slot' },
        el('span', { class: 'rs-slot-label', text: KIND[l.kind] }),
        bidInput(`${id}-name`, t('reportsetup.line_name', { kind: KIND[l.kind], n: i + 1 }), l.name, (v) => {
          // Markups on a section follow its new name.
          if (l.kind === 'section') {
            for (const x of bid.lines) if (x.kind === 'percent') x.of = x.of.map((n) => (sameName(n, l.name) ? v : n));
          }
          l.name = v;
        }, { placeholder: t('reportsetup.name') }),
        el(
          'span',
          { class: 'rs-moves' },
          bidButton(`${id}-up`, '↑', t('reportsetup.move_up', { name: l.name || t('reportsetup.line') }), () => change(() => ([bid.lines[i - 1], bid.lines[i]] = [bid.lines[i], bid.lines[i - 1]])), i === 0),
          bidButton(`${id}-down`, '↓', t('reportsetup.move_down', { name: l.name || t('reportsetup.line') }), () => change(() => ([bid.lines[i + 1], bid.lines[i]] = [bid.lines[i], bid.lines[i + 1]])), i === bid.lines.length - 1),
          bidButton(`${id}-del`, '×', t('reportsetup.remove', { name: l.name || t('reportsetup.line') }), () => change(() => bid.lines.splice(i, 1))),
        ),
      );
      box.append(head);
      if (l.kind === 'section') {
        const [catText, acctText] = pickTexts(l.picks);
        let cats = catText;
        let accts = acctText;
        box.append(
          el('label', { class: 'rs-inline' }, el('span', { text: t('reportsetup.categories') }), bidInput(`${id}-cats`, t('reportsetup.categories_takes', { name: l.name }), catText, (v) => ((cats = v), (l.picks = picksOf(cats, accts))), { placeholder: t('reportsetup.category_codes'), maxlength: '2000' })),
          el('label', { class: 'rs-inline' }, el('span', { text: t('reportsetup.accounts') }), bidInput(`${id}-accts`, t('reportsetup.accounts_takes', { name: l.name }), acctText, (v) => ((accts = v), (l.picks = picksOf(cats, accts))), { placeholder: t('reportsetup.numbers_or_ranges'), maxlength: '2000' })),
        );
      } else if (l.kind === 'percent') {
        box.append(
          el(
            'label',
            { class: 'rs-inline' },
            el('span', { text: t('reportsetup.percent') }),
            bidInput(`${id}-pct`, t('reportsetup.line_percent', { name: l.name }), l.percent, (v) => {
              if (!PERCENT.test(v.trim())) return false;
              l.percent = v.trim();
              return true;
            }, { placeholder: t('reportsetup.percent'), maxlength: '20' }),
          ),
        );
        const of = el('div', { class: 'rs-groups' });
        sectionNames.forEach((n, k) => {
          of.append(
            check(n || t('reportsetup.no_name'), l.of.some((x) => sameName(x, n)), (v) =>
              change(() => {
                l.of = v ? [...l.of, n] : l.of.filter((x) => !sameName(x, n));
              }),
              { key: `bid-l${i}-of${k}` },
            ),
          );
        });
        box.append(el('p', { class: 'rs-hint', text: t('reportsetup.of_sections_hint') }), of);
      } else if (l.kind === 'flat') {
        box.append(
          el(
            'label',
            { class: 'rs-inline' },
            el('span', { text: t('reportsetup.amount') }),
            bidInput(`${id}-amt`, t('reportsetup.line_amount', { name: l.name }), money(l.amount).replace(/^\((.*)\)$/, '-$1'), (v) => {
              const c = cents(v);
              if (c === null) return false;
              l.amount = c;
              return true;
            }, { placeholder: t('reportsetup.amount'), maxlength: '24' }),
          ),
        );
      }
      parts.push(box);
    });
    const add = (kind, label, line) => {
      const b2 = el('button', { class: 'btn btn-quiet rs-reset', text: label, attrs: { type: 'button', id: `rs-bid-add-${kind}` } });
      b2.addEventListener('click', () => change(() => bid.lines.push(line())));
      return b2;
    };
    const fresh = () => {
      let n = 1;
      while (bid.lines.some((x) => sameName(x.name, t('reportsetup.default.section_n', { n })))) n += 1;
      return t('reportsetup.default.section_n', { n });
    };
    parts.push(
      el(
        'div',
        { class: 'rs-bid-adds' },
        add('section', t('reportsetup.add_section'), () => ({ kind: 'section', name: fresh(), picks: [] })),
        add('subtotal', t('reportsetup.add_subtotal'), () => ({ kind: 'subtotal', name: t('reportsetup.default.subtotal') })),
        add('percent', t('reportsetup.add_markup'), () => ({ kind: 'percent', name: t('reportsetup.default.markup'), percent: '10', of: [] })),
        add('flat', t('reportsetup.add_flat'), () => ({ kind: 'flat', name: t('reportsetup.default.fee'), amount: 0 })),
      ),
      el('p', { class: 'rs-hint', text: t('reportsetup.bid_accounts_hint') }),
      bidNotes,
    );
    return group(t('reportsetup.bid_layout'), ...parts);
  }

  function drawSettings() {
    const scroll = left.scrollTop;
    const active = document.activeElement;
    const focusId = active && left.contains(active) ? active.id : '';
    left.replaceChildren(...[sectionsGroup(), bidGroup()].filter(Boolean), treeGroup(), leaveOutGroup(), topsheetGroup(), columnsGroup(), layoutGroup(), bandsGroup());
    left.scrollTop = scroll;
    if (focusId) {
      const again = [...left.querySelectorAll('[id]')].find((n) => n.id === focusId);
      // A move button that just became disabled hands focus to its row's box.
      if (again && !again.disabled) again.focus();
      else if (focusId.startsWith('rs-up-') || focusId.startsWith('rs-down-')) {
        const boxEl = left.querySelector(`#rs-sec-${focusId.replace(/^rs-(up|down)-/, '')}`);
        if (boxEl) boxEl.focus();
      }
    }
  }

  // --- Presets ---------------------------------------------------------

  function drawPresets() {
    presetSelect.replaceChildren(el('option', { text: t('reportsetup.new_setup'), attrs: { value: '' } }), ...presets.map((p, i) => el('option', { text: p.default ? t('reportsetup.preset_default', { name: p.name }) : p.name, attrs: { value: String(i) } })));
    presetSelect.value = current === null ? '' : String(current);
    nameInput.value = name;
    defaultBox.checked = isDefault;
    deleteBtn.disabled = current === null;
    saveNewBtn.disabled = current === null;
  }

  presetSelect.addEventListener('change', () => {
    const v = presetSelect.value;
    current = v === '' ? null : Number(v);
    if (current === null) {
      name = '';
      isDefault = false;
    } else {
      spec = normalizeSpec(presets[current]);
      name = presets[current].name;
      isDefault = presets[current].default === true;
    }
    drawPresets();
    drawSettings();
    say('');
    redraw();
  });
  nameInput.addEventListener('input', () => (name = nameInput.value));
  defaultBox.addEventListener('change', () => (isDefault = defaultBox.checked));

  const save = async (index) => {
    const n = nameInput.value.trim();
    if (!n) {
      say(t('reportsetup.name_first'), true);
      nameInput.focus();
      return;
    }
    const preset = { name: n, ...(defaultBox.checked ? { default: true } : {}), ...compactSpec(spec) };
    try {
      const v = await ctx.call('save_report_preset', { index, preset });
      presets = presetsOf(v);
      current = index === null ? presets.length - 1 : index;
      name = presets[current].name;
      isDefault = presets[current].default === true;
      drawPresets();
      say(index === null ? t('reportsetup.saved_preset', { name }) : t('reportsetup.updated_preset', { name }));
    } catch (e) {
      say(String(e), true);
    }
  };
  saveBtn.addEventListener('click', () => save(current));
  saveNewBtn.addEventListener('click', () => {
    if (current !== null && nameInput.value.trim().toLowerCase() === presets[current].name.trim().toLowerCase()) {
      say(t('reportsetup.new_name_first'), true);
      nameInput.focus();
      nameInput.select();
      return;
    }
    save(null);
  });
  deleteBtn.addEventListener('click', async () => {
    if (current === null) return;
    const gone = presets[current].name;
    try {
      const v = await ctx.call('delete_report_preset', { index: current });
      presets = presetsOf(v);
      current = null;
      name = '';
      isDefault = false;
      drawPresets();
      say(t('reportsetup.deleted_preset', { name: gone }));
    } catch (e) {
      say(String(e), true);
    }
  });

  // --- Print and PDF ---------------------------------------------------

  const busy = (on) => {
    for (const btn of [printBtn, pdfBtn, closeBtn]) btn.disabled = on;
  };
  const output = async (pdf) => {
    busy(true);
    say(pdf ? t('reportsetup.saving_pdf') : t('reportsetup.printing'));
    try {
      const r = await printReport(full, spec, { pdf, name: current === null ? '' : presets[current].name });
      if (pdf && !r.system) say(r.done && r.file ? t('reportsetup.saved_file', { file: r.file }) : '');
      else say('');
    } catch (e) {
      say(String(e), true);
    } finally {
      busy(false);
      if (pages.isConnected) draw();
    }
  };
  printBtn.addEventListener('click', () => output(false));
  pdfBtn.addEventListener('click', () => output(true));

  // --- The dialog ------------------------------------------------------

  close = customDialog(
    (done) => {
      closeBtn.addEventListener('click', done);
      const presetBar = el(
        'div',
        { class: 'rs-presetbar' },
        el('label', { class: 'rs-inline' }, el('span', { text: t('reportsetup.preset') }), presetSelect),
        nameInput,
        el('label', { class: 'rs-check', attrs: { for: 'rs-default' } }, defaultBox, el('span', { text: t('reportsetup.default') })),
        saveBtn,
        saveNewBtn,
        deleteBtn,
      );
      const toolbar = el(
        'div',
        { class: 'rs-previewbar' },
        el('div', { class: 'rs-nav-group', attrs: { role: 'group', 'aria-label': t('reportsetup.pages') } }, firstBtn, prevBtn, el('span', { class: 'rs-page-label', text: t('reportsetup.page') }), pageInput, pageCount, nextBtn, lastBtn),
        jump,
      );
      const box = el(
        'div',
        { class: 'modal rs-dialog', attrs: { role: 'dialog', 'aria-modal': 'true', 'aria-labelledby': 'rs-title' } },
        el('div', { class: 'rs-head' }, el('h2', { class: 'modal-title', text: t('reportsetup.print'), attrs: { id: 'rs-title' } }), presetBar),
        el('div', { class: 'rs-main' }, left, el('div', { class: 'rs-preview-pane' }, toolbar, pages)),
        el('div', { class: 'rs-foot' }, status, el('div', { class: 'modal-actions' }, closeBtn, pdfBtn, printBtn)),
      );
      return box;
    },
    {
      onClose: () => {
        clearTimeout(timer);
        if (navFrame) cancelAnimationFrame(navFrame);
        resize.disconnect();
      },
    },
  );
  drawPresets();
  drawSettings();
  draw();
  resize.observe(pages);
  return close;
}
