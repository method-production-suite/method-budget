// File > Print: the report setup dialog. Choices on the left (sections and
// their order, categories and accounts, what to leave out, detail columns,
// layout, header and footer), the report as it will print on the right, page
// by page, redrawn as the choices change. Saved setups ("report presets") live
// in the budget (src/reports.rs): pick one, save, rename, delete, mark one as
// the default the dialog opens with. Print sends the pages to the print sheet;
// Save as PDF writes them straight into a PDF file (report.js).
//
// Everything from the budget goes in through textContent or input values.

import { el } from './dom.js';
import { customDialog } from './modal.js';
import { invoke } from './api.js';
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
} from './report.js';

const SLOTS = [
  ['left', 'Left'],
  ['center', 'Center'],
  ['right', 'Right'],
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
  const pages = el('div', { class: 'rs-pages rpt-preview', attrs: { tabindex: '0', 'aria-label': 'Report preview' } });
  const pageCount = el('span', { class: 'rs-count', attrs: { 'aria-live': 'polite' } });
  const jump = el('select', { class: 'rs-jump', attrs: { 'aria-label': 'Go to a section of the preview' } });
  // Page navigation: first, previous, the page shown (type a number), next, last.
  const navBtn = (text, label) => el('button', { class: 'btn btn-quiet rs-nav', text, title: label, attrs: { type: 'button', 'aria-label': label } });
  const firstBtn = navBtn('«', 'First page');
  const prevBtn = navBtn('‹', 'Previous page');
  const nextBtn = navBtn('›', 'Next page');
  const lastBtn = navBtn('»', 'Last page');
  const pageInput = el('input', { class: 'modal-input rs-page-input', attrs: { type: 'text', inputmode: 'numeric', autocomplete: 'off', spellcheck: 'false', 'aria-label': 'Page shown', size: '3' } });
  const status = el('p', { class: 'rs-status', attrs: { role: 'status' } });
  const presetSelect = el('select', { class: 'rs-preset', attrs: { 'aria-label': 'Report preset' } });
  const nameInput = el('input', { class: 'modal-input rs-name', attrs: { type: 'text', autocomplete: 'off', spellcheck: 'false', placeholder: 'Preset name', 'aria-label': 'Preset name', maxlength: '80' } });
  const defaultBox = el('input', { attrs: { type: 'checkbox', id: 'rs-default' } });
  const saveBtn = el('button', { class: 'btn', text: 'Save Preset', attrs: { type: 'button' } });
  const saveNewBtn = el('button', { class: 'btn', text: 'Save as New', attrs: { type: 'button' } });
  const deleteBtn = el('button', { class: 'btn btn-quiet btn-danger-text', text: 'Delete', attrs: { type: 'button' } });
  const printBtn = el('button', { class: 'btn btn-primary', text: 'Print…', attrs: { type: 'button', 'data-autofocus': '' } });
  const pdfBtn = el('button', { class: 'btn', text: 'Save as PDF…', attrs: { type: 'button' } });
  const closeBtn = el('button', { class: 'btn', text: 'Close', attrs: { type: 'button' } });

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
  const draw = () => {
    clearTimeout(timer);
    timer = 0;
    if (!pages.isConnected) return;
    const top = pages.scrollTop;
    const out = renderReport(pages, full, spec);
    const width = Math.max(240, pages.clientWidth - 40);
    scalePreview(pages, width, spec.landscape);
    sections = out.sections;
    pageTotal = out.pages;
    pageCount.textContent = `of ${out.pages}`;
    jump.replaceChildren(el('option', { text: 'Go to section', attrs: { value: '' } }), ...sections.map((s, i) => el('option', { text: `${s.name} (page ${s.page})`, attrs: { value: String(i) } })));
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
        check(s.label, i >= 0, (v) =>
          change(() => {
            spec.sections = v ? [...on, key] : on.filter((k) => k !== key);
          }),
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
      const up = el('button', { class: 'btn btn-quiet rs-move', text: '↑', title: `Print ${s.label} earlier`, attrs: { type: 'button', id: `rs-up-${key}`, 'aria-label': `Move ${s.label} up` } });
      const down = el('button', { class: 'btn btn-quiet rs-move', text: '↓', title: `Print ${s.label} later`, attrs: { type: 'button', id: `rs-down-${key}`, 'aria-label': `Move ${s.label} down` } });
      up.disabled = i <= 0;
      down.disabled = i < 0 || i >= on.length - 1;
      up.addEventListener('click', () => move(-1));
      down.addEventListener('click', () => move(1));
      item.append(el('span', { class: 'rs-moves' }, up, down));
      list.append(item);
    });
    return group('Sections, in print order', list);
  }

  function treeGroup() {
    const exC = new Set(spec.excluded_categories);
    const exA = new Set(spec.excluded_accounts);
    const tree = el('ul', { class: 'rs-tree' });
    const allIn = categories.every((c) => !exC.has(c.code || '') && (c.accounts || []).every((a) => !exA.has(a.code || '')));
    const all = check('All categories and accounts', allIn, (v) =>
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
        attrs: { type: 'button', id: `rs-open-${ci}`, 'aria-expanded': String(openCats.has(code)), 'aria-label': `Accounts in ${code} ${c.name || ''}`.trim() },
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
    if (!categories.length) tree.append(el('li', { class: 'rs-hint', text: 'This budget has no categories yet.' }));
    return group('Categories and accounts', el('p', { class: 'rs-hint', text: 'For the account and detail levels and the notes list. The topsheet always lists every category.' }), all, tree);
  }

  function leaveOutGroup() {
    const flag = (key, label) => check(label, spec[key], (v) => change(() => (spec[key] = v)), { key });
    const parts = [
      flag('skip_empty_accounts', 'Accounts with no lines'),
      flag('skip_zero_accounts', 'Accounts that total zero'),
      flag('skip_suppressed_lines', 'Lines not in the budget total'),
      flag('skip_notes', 'Notes under detail lines'),
      flag('skip_done_notes', 'Notes ticked off as reviewed'),
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
      parts.push(el('p', { class: 'rs-sub', text: 'Detail pages print only lines in these groups (none checked: every line)' }), list);
    }
    return group('Leave out', ...parts, el('p', { class: 'rs-hint', text: 'Totals stay the budget’s own; a note says when rows are left out.' }));
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
          { key: `col-${c.key}`, disabled: base && !hasBaseline, title: base && !hasBaseline ? 'Lock a baseline (Budget tab) to compare against' : '' },
        ),
      );
    }
    const reset = el('button', { class: 'btn btn-quiet rs-reset', text: 'Standard columns', attrs: { type: 'button', id: 'rs-std-cols' } });
    reset.disabled = !spec.columns;
    reset.addEventListener('click', () => change(() => (spec.columns = null)));
    return group(
      'Detail columns',
      el('p', { class: 'rs-hint', text: '#, description and total always print. Original and variance print on the topsheet and account level too.' }),
      box,
      reset,
    );
  }

  function layoutGroup() {
    const orient = el('div', { class: 'rs-radios', attrs: { role: 'radiogroup', 'aria-label': 'Orientation' } });
    for (const [v, label] of [
      [false, 'Portrait'],
      [true, 'Landscape'],
    ]) {
      const id = `rs-orient-${v ? 'l' : 'p'}`;
      const r = el('input', { attrs: { id, type: 'radio', name: 'rs-orient' } });
      r.checked = spec.landscape === v;
      r.addEventListener('change', () => change(() => (spec.landscape = v)));
      orient.append(el('label', { class: 'rs-check', attrs: { for: id } }, r, el('span', { text: label })));
    }
    const size = el('select', { class: 'modal-input rs-select', attrs: { id: 'rs-size', 'aria-label': 'Type size' } });
    for (const [v, label] of [
      ['small', 'Small'],
      ['medium', 'Medium'],
      ['large', 'Large'],
    ]) {
      const o = el('option', { text: label });
      o.value = v;
      size.append(o);
    }
    size.value = spec.font_size;
    size.addEventListener('change', () => change(() => (spec.font_size = size.value)));
    return group(
      'Layout',
      orient,
      el('label', { class: 'rs-inline' }, el('span', { text: 'Type size' }), size),
      check('Shading on alternate lines and totals', spec.shading, (v) => change(() => (spec.shading = v)), { key: 'shading' }),
      check('Lines in their group colors', spec.group_colors, (v) => change(() => (spec.group_colors = v)), { key: 'colors' }),
      check('Lines between columns', spec.vertical_rules, (v) => change(() => (spec.vertical_rules = v)), { key: 'rules' }),
      check('Detail level: one line per account', spec.collapse_details, (v) => change(() => (spec.collapse_details = v)), {
        key: 'collapse',
        title: 'Each account prints as one line with its total instead of its lines',
      }),
    );
  }

  function topsheetGroup() {
    const noDetail = !spec.sections.includes('detail');
    return group(
      'Topsheet columns',
      check('Fringes', spec.topsheet_fringes, (v) => change(() => (spec.topsheet_fringes = v)), { key: 'ts-fringes', title: 'What each category’s lines earn in fringes, wherever they post' }),
      check('Page its detail starts on', spec.topsheet_pages, (v) => change(() => (spec.topsheet_pages = v)), {
        key: 'ts-pages',
        title: noDetail ? 'Prints blank until the detail level is in the report' : 'The page of this printout where each category’s detail begins',
      }),
      noDetail && spec.topsheet_pages ? el('p', { class: 'rs-hint', text: 'Add the detail level to fill in the page numbers.' }) : null,
    );
  }

  function fieldSelect(band, slot, which) {
    const f = band[slot] || null;
    const sel = el('select', { class: 'modal-input rs-select', attrs: { id: `rs-${which}-${slot}`, 'aria-label': `${which} ${slot}` } });
    const opts = [
      ['', 'Nothing'],
      ['title', 'Budget title'],
      ['section', 'Section'],
      ['printed', 'Date and time printed'],
      ['date', 'Date printed'],
      ['time', 'Time printed'],
      ['file', 'File name'],
      ['page', 'Page X of Y'],
      ...infoLabels.map((l) => [`info:${l}`, l]),
      ...globalNames.map((g) => [`global:${g}`, `Global: ${g}`]),
      ['text', 'Text…'],
    ];
    if (f && f.kind === 'info' && !infoLabels.includes(f.label)) opts.splice(opts.length - 1, 0, [`info:${f.label}`, f.label]);
    if (f && f.kind === 'global' && !globalNames.includes(f.name)) opts.splice(opts.length - 1, 0, [`global:${f.name}`, `Global: ${f.name} (not in this budget)`]);
    for (const [v, label] of opts) {
      const o = el('option', { text: label });
      o.value = v;
      sel.append(o);
    }
    sel.value = !f ? '' : f.kind === 'info' ? `info:${f.label}` : f.kind === 'global' ? `global:${f.name}` : f.kind;
    const text = el('input', { class: 'modal-input rs-text', attrs: { type: 'text', maxlength: '200', 'aria-label': `${which} ${slot} text`, placeholder: 'Text' } });
    text.value = f && f.kind === 'text' ? f.text : '';
    text.hidden = !(f && f.kind === 'text');
    const key = which === 'Header' ? 'header' : 'footer';
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
    const reset = el('button', { class: 'btn btn-quiet rs-reset', text: 'Standard header and footer', attrs: { type: 'button', id: 'rs-std-bands' } });
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
      'Header and footer',
      el('p', { class: 'rs-sub', text: 'Header' }),
      ...SLOTS.map(([s]) => fieldSelect(header, s, 'Header')),
      firstOnly('header', 'Header on the first page only'),
      el('p', { class: 'rs-sub', text: 'Footer' }),
      ...SLOTS.map(([s]) => fieldSelect(footer, s, 'Footer')),
      firstOnly('footer', 'Footer on the first page only'),
      reset,
    );
  }

  function drawSettings() {
    const scroll = left.scrollTop;
    const active = document.activeElement;
    const focusId = active && left.contains(active) ? active.id : '';
    left.replaceChildren(sectionsGroup(), treeGroup(), leaveOutGroup(), topsheetGroup(), columnsGroup(), layoutGroup(), bandsGroup());
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
    presetSelect.replaceChildren(el('option', { text: 'New setup (not saved)', attrs: { value: '' } }), ...presets.map((p, i) => el('option', { text: p.default ? `${p.name} (default)` : p.name, attrs: { value: String(i) } })));
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
      say('Give the preset a name first.', true);
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
      say(index === null ? `Saved “${name}” in the budget.` : `Updated “${name}”.`);
    } catch (e) {
      say(String(e), true);
    }
  };
  saveBtn.addEventListener('click', () => save(current));
  saveNewBtn.addEventListener('click', () => {
    if (current !== null && nameInput.value.trim().toLowerCase() === presets[current].name.trim().toLowerCase()) {
      say('Type a new name for the copy first.', true);
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
      say(`Deleted “${gone}”. Edit > Undo brings it back.`);
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
    say(pdf ? 'Saving the PDF…' : 'Printing…');
    try {
      const r = await printReport(full, spec, { pdf, name: current === null ? '' : presets[current].name });
      if (pdf && !r.system) say(r.done && r.file ? `Saved ${r.file}.` : '');
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
        el('label', { class: 'rs-inline' }, el('span', { text: 'Preset' }), presetSelect),
        nameInput,
        el('label', { class: 'rs-check', attrs: { for: 'rs-default' } }, defaultBox, el('span', { text: 'Default' })),
        saveBtn,
        saveNewBtn,
        deleteBtn,
      );
      const toolbar = el(
        'div',
        { class: 'rs-previewbar' },
        el('div', { class: 'rs-nav-group', attrs: { role: 'group', 'aria-label': 'Pages' } }, firstBtn, prevBtn, el('span', { class: 'rs-page-label', text: 'Page' }), pageInput, pageCount, nextBtn, lastBtn),
        jump,
      );
      const box = el(
        'div',
        { class: 'modal rs-dialog', attrs: { role: 'dialog', 'aria-modal': 'true', 'aria-labelledby': 'rs-title' } },
        el('div', { class: 'rs-head' }, el('h2', { class: 'modal-title', text: 'Print', attrs: { id: 'rs-title' } }), presetBar),
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
