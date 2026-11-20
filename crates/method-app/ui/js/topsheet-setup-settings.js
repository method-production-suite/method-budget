// Budget Setup's Settings tab (a part of topsheet-setup.js).

import { displaySettingsGroup } from './display-settings.js';
import { applyCoding, pendingRenumber, previewBox } from './renumber.js';
import { mountFrame, actionStrip } from './setupframe.js';
import { obj } from './util.js';
import { fringeRowOf } from './levels.js';
import { t } from './i18n.js';
import { SEPARATORS, arr, isNone, heading, checkbox, categoryCodes } from './topsheet-setup-common.js';

let k = null; // the kit from setup.js, set by useKit() on every render

/** Takes the kit for this render (topsheet-setup.js calls it). */
export function useKit(kit) {
  k = kit;
}

// ---------------------------------------------------------------------------
// Settings: fringe posting, calculation, new lines, numbers on screen,
// account numbers (the base and totals currency are the Currency tab's)

function coding() {
  const c = obj(k.budget().account_coding);
  const w = (f, d) => (Number.isInteger(c[f]) ? c[f] : d);
  return {
    category_width: w('category_width', 2),
    account_width: w('account_width', 2),
    sub_width: w('sub_width', 0),
    set_width: w('set_width', 0),
    separator: typeof c.separator === 'string' ? c.separator : '-',
  };
}

function fit(text, width) {
  if (!width || !text) return '';
  return text.slice(0, width).padStart(width, '0');
}

// An example number in the pattern: category 11, account 1, sub 2, set 3.
function exampleNumber(p) {
  const segs = [fit('11', p.category_width), fit('1', p.account_width), fit('2', p.sub_width), fit('3', p.set_width)];
  return segs.filter((s) => s !== '').join(p.separator);
}

// The pattern rules from method_core::coding, so the form can explain a problem
// before it is applied. The app checks them again.
function codingProblem(p) {
  if (p.category_width < 1) return t('topsheet.every_account_number_starts_with_a_category');
  // The sub-account and the set each need the account; the set may follow
  // the account directly when there is no sub-account.
  if (p.sub_width > 0 && p.account_width === 0) return t('topsheet.the_sub_account_part_needs_the_account');
  if (p.set_width > 0 && p.account_width === 0) return t('topsheet.the_set_part_needs_the_account_part');
  return null;
}

function fringeAccountExample(p, suffix) {
  const first = categoryCodes()[0] || '11';
  if (!p.account_width || !suffix) return null;
  const cat = fit(first, p.category_width);
  return { cat: first, number: [cat, fit(suffix, p.account_width)].join(p.separator) };
}

// A settings row: a fixed-width label on the left, its controls on the right.
// `hint` is one short muted line after the controls; `tip` is the longer
// explanation, shown as the label's tooltip and read out with the controls.
// With `group`, the label names several controls (each with its own label).
function tsRow(labelText, controls, { hint = '', tip = '', group = false } = {}) {
  const row = k.el('div', 'ts-row');
  const label = k.el(group ? 'span' : 'label', 'ts-row-label', labelText);
  label.id = k.nextId();
  if (!group && controls[0] && controls[0].id) label.htmlFor = controls[0].id;
  if (tip) {
    label.title = tip;
    label.classList.add('ts-has-tip');
  }
  const ctl = k.el('div', 'ts-row-ctl');
  if (group) {
    ctl.setAttribute('role', 'group');
    ctl.setAttribute('aria-labelledby', label.id);
  }
  ctl.append(...controls);
  const described = [];
  if (hint) {
    const h = k.el('span', 'ts-hint', hint);
    h.id = k.nextId();
    if (tip) h.title = tip;
    ctl.append(h);
    described.push(h.id);
  }
  if (tip) {
    const tipEl = k.el('span', 'setup-sr', tip);
    tipEl.id = k.nextId();
    ctl.append(tipEl);
    described.push(tipEl.id);
  }
  if (described.length) {
    for (const c of ctl.querySelectorAll('input, select')) {
      const prior = c.getAttribute('aria-describedby');
      c.setAttribute('aria-describedby', [prior, ...described].filter(Boolean).join(' '));
    }
  }
  row.append(label, ctl);
  return row;
}

// A small control with its own short label, for rows holding several.
function tsInline(labelText, control) {
  const wrap = k.el('span', 'ts-inline');
  const label = k.el('label', 'ts-inline-label', labelText);
  label.htmlFor = control.id;
  wrap.append(label, control);
  return wrap;
}

export function renderSettings(panel) {
  const b = k.budget();
  panel.classList.add('ts-settings');

  // Currency: a pointer to where it moved.
  const cur = k.el('div', 'tss-group');
  cur.append(heading(t('topsheet.currency')), k.el('p', 'ts-hint ts-pointer', t('topsheet.the_base_currency_and_show_totals_in')));

  // Fringe posting
  const fk = k.key('settings', 'fringes');
  const fp = obj(b.fringe_posting);
  const mode = ['budget', 'production_level', 'category', 'account'].includes(fp.kind) ? fp.kind : 'category';
  const suffix = mode === 'category' ? (isNone(fp.suffix) ? '99' : k.str(fp.suffix)) : '99';
  const fr = k.el('div', 'tss-group');
  fr.dataset.item = fk;
  const modeSel = k.selectInput({
    fieldKey: k.key('settings', 'posting'),
    value: mode,
    options: [
      ['category', t('topsheet.an_account_at_the_end_of_each')],
      ['account', t('topsheet.a_line_at_the_end_of_each')],
      ['production_level', t('topsheet.a_row_above_each_production_total')],
      ['budget', t('topsheet.one_total_for_the_whole_budget')],
    ],
    onCommit: (v, old) => k.run(fk, () => ['set_fringe_posting', { mode: v, suffix: null }], { restore: [k.key('settings', 'posting'), old] }),
  });
  modeSel.classList.add('ts-w-l');
  fr.append(
    heading(t('topsheet.fringe_totals')),
    tsRow(
      t('topsheet.fringes_show_as'),
      [modeSel],
      mode === 'budget'
        ? {
            hint: t('topsheet.percent_charges_count_it'),
            tip: t('topsheet.a_budget_wide_fringe_total_sits_outside'),
          }
        : {},
    ),
  );
  if (mode === 'category') {
    const suf = k.textInput({
      fieldKey: k.key('settings', 'suffix'),
      value: suffix,
      onCommit: (v, old) =>
        k.run(fk, () => ['set_fringe_posting', { mode: 'category', suffix: v.trim() }], { restore: [k.key('settings', 'suffix'), old] }),
    });
    suf.classList.add('ts-w-s');
    const ex = fringeAccountExample(coding(), suffix);
    fr.append(tsRow(t('topsheet.account_number_ends_in'), [suf], { hint: ex ? t('topsheet.category_number_example', { code: ex.cat, number: ex.number }) : '' }));
  }
  if (mode === 'budget') fr.append(...fringeRowRows(fk));
  if (k.state().errors.has(fk)) fr.append(k.errorLine(k.state().errors.get(fk)));

  mountFrame(panel, {
    strip: actionStrip(t('topsheet.settings')),
    body: [
      cur,
      fr,
      calculationGroup(),
      newLinesGroup(),
      // Numbers on screen: rate decimals, summary rows, net total (display-settings.js).
      displaySettingsGroup(k),
      codingForm(),
    ],
  });
}

// The budget's one fringe row (fringes posted by budget; decisions.md, entry
// 59): its account number, description and place on the topsheet. Display
// only; the file leaves it out at its defaults (no number, "Total Fringes",
// after the summary rows).
function fringeRowRows(fk) {
  const b = k.budget();
  const now = fringeRowOf(b);
  const send = (patch) => ({
    account: now.account,
    description: obj(b.fringe_row).description || '',
    position: now.position,
    ...patch,
  });
  const acct = k.textInput({
    fieldKey: k.key('settings', 'fringe_row', 'account'),
    value: now.account,
    onCommit: (v, old) =>
      k.run(fk, () => ['set_fringe_row', { row: send({ account: v.trim() }) }], { restore: [k.key('settings', 'fringe_row', 'account'), old] }),
  });
  acct.classList.add('ts-w-s');
  const desc = k.textInput({
    fieldKey: k.key('settings', 'fringe_row', 'description'),
    value: now.description,
    onCommit: (v, old) =>
      k.run(fk, () => ['set_fringe_row', { row: send({ description: v.trim() }) }], { restore: [k.key('settings', 'fringe_row', 'description'), old] }),
  });
  desc.classList.add('ts-w-m');
  // Its place: after the summary rows, or above one of the topsheet's rows.
  const ts = arr(b.topsheet);
  const options = [['', t('topsheet.after_the_summary_rows')]];
  ts.forEach((r, i) => {
    const row = obj(r);
    const label = row.kind === 'production_total' ? k.str(row.name) : `${k.str(row.code)} ${k.str(row.name)}`.trim();
    options.push([String(i), t('topsheet.above_row', { name: label })]);
  });
  if (ts.length) options.push([String(ts.length), t('topsheet.after_the_last_topsheet_row')]);
  const at = now.position === null ? '' : String(Math.min(now.position, ts.length));
  const place = k.selectInput({
    fieldKey: k.key('settings', 'fringe_row', 'position'),
    value: at,
    options,
    onCommit: (v, old) =>
      k.run(fk, () => ['set_fringe_row', { row: send({ position: v === '' ? null : Number(v) }) }], { restore: [k.key('settings', 'fringe_row', 'position'), old] }),
  });
  place.classList.add('ts-w-l');
  return [
    tsRow(t('topsheet.fringe_row_number'), [acct], { hint: t('topsheet.blank_for_none') }),
    tsRow(t('topsheet.fringe_row_description'), [desc]),
    tsRow(t('topsheet.fringe_row_place'), [place], {
      tip: t('topsheet.where_the_budgets_fringe_row_shows_on'),
    }),
  ];
}

// How totals are worked out (budget settings): rounding, account totals in
// whole units, the percent fringe cutoff factor and the fringes of
// categories after the last production total. The file leaves out each at
// its default (binary, off, 1.0, keep).
function calculationGroup() {
  const b = k.budget();
  const ck = k.key('settings', 'calculation');
  const group = k.el('div', 'tss-group');
  group.dataset.item = ck;
  const arith = b.arithmetic === 'exact' ? 'exact' : 'binary';
  const trailing = b.trailing_level_fringes === 'drop' ? 'drop' : 'keep';
  const arithSel = k.selectInput({
    fieldKey: k.key('settings', 'arithmetic'),
    value: arith,
    options: [
      ['binary', t('topsheet.spreadsheet_binary')],
      ['exact', t('topsheet.exact_decimal')],
    ],
    onCommit: (v, old) => k.run(ck, () => ['set_arithmetic', { value: v }], { restore: [k.key('settings', 'arithmetic'), old] }),
  });
  const trailingSel = k.selectInput({
    fieldKey: k.key('settings', 'trailing'),
    value: trailing,
    options: [
      ['keep', t('topsheet.keep')],
      ['drop', t('topsheet.drop')],
    ],
    onCommit: (v, old) =>
      k.run(ck, () => ['set_trailing_level_fringes', { value: v }], { restore: [k.key('settings', 'trailing'), old] }),
  });
  const factors = ['1.0', '1.5', '2.0', '2.5', '3.0'];
  const factor = factors.includes(k.str(b.cutoff_factor)) ? k.str(b.cutoff_factor) : '1.0';
  const factorSel = k.selectInput({
    fieldKey: k.key('settings', 'cutoff_factor'),
    value: factor,
    options: factors.map((f) => [f, f]),
    onCommit: (v, old) => k.run(ck, () => ['set_cutoff_factor', { value: v }], { restore: [k.key('settings', 'cutoff_factor'), old] }),
  });
  const wholeKey = k.key('settings', 'round_account_totals');
  const whole = checkbox(wholeKey, b.round_account_totals === true, t('topsheet.round_account_totals_to_whole_units'), (on) =>
    k.run(ck, () => ['set_round_account_totals', { value: on }], { restore: [wholeKey, !on] }),
  );
  arithSel.classList.add('ts-w-m');
  trailingSel.classList.add('ts-w-s');
  factorSel.classList.add('ts-w-xs');
  group.append(
    heading(t('topsheet.calculation')),
    tsRow(t('topsheet.rounding'), [arithSel], {
      hint: t('topsheet.rounding_example'),
      tip: t('topsheet.spreadsheet_rounding_works_in_binary_numbers_so'),
    }),
    tsRow(t('topsheet.account_totals'), [whole], {
      hint: t('topsheet.categories_and_the_grand_total_add_up'),
      tip: t('topsheet.each_accounts_total_is_rounded_to_the'),
    }),
    tsRow(t('topsheet.percent_fringe_cutoff_factor'), [factorSel], {
      hint: t('topsheet.multiplies_every_percent_fringes_cutoff'),
      tip: t('topsheet.every_percent_fringes_cutoff_is_multiplied_by'),
    }),
    tsRow(t('topsheet.fringes_after_the_last_total'), [trailingSel], {
      hint: t('topsheet.only_when_fringes_post_above_each_production'),
      tip: t('topsheet.fringes_of_categories_after_the_last_production'),
    }),
  );
  if (k.state().errors.has(ck)) group.append(k.errorLine(k.state().errors.get(ck)));
  return group;
}

// What a line inserted in the grid copies from its neighbor (a budget setting).
const NEW_LINE_FIELDS = [
  ['fringes', t('topsheet.fringes')],
  ['groups', t('topsheet.groups')],
  ['set', t('topsheet.set')],
  ['location', t('topsheet.location')],
  ['currency', t('topsheet.currency')],
];

function newLineDefaults() {
  const d = obj(k.budget().new_line_defaults);
  // The file leaves the setting out at its default: fringes on, the rest off.
  const has = Object.keys(d).length > 0;
  const out = {};
  for (const [f] of NEW_LINE_FIELDS) out[f] = has ? d[f] === true : f === 'fringes';
  return out;
}

function newLinesGroup() {
  const nk = k.key('settings', 'new_lines');
  const group = k.el('div', 'tss-group');
  group.dataset.item = nk;
  const now = newLineDefaults();
  const boxes = NEW_LINE_FIELDS.map(([f, label]) => {
    const fieldKey = k.key('settings', 'new_lines', f);
    return checkbox(fieldKey, now[f], label, (on) =>
      k.run(nk, () => ['set_new_line_defaults', { defaults: { ...newLineDefaults(), [f]: on } }], { restore: [fieldKey, !on] }),
    );
  });
  group.append(
    heading(t('topsheet.new_lines')),
    tsRow(t('topsheet.copy_from_the_line_above'), boxes, {
      group: true,
      tip: t('topsheet.a_line_you_insert_copies_these_from'),
    }),
  );
  if (k.state().errors.has(nk)) group.append(k.errorLine(k.state().errors.get(nk)));
  return group;
}

function codingForm() {
  const ck = k.key('settings', 'coding');
  const p = coding();
  const form = k.el('form', 'tss-group');
  form.noValidate = true;
  form.dataset.item = ck;
  const widths = [
    ['category_width', t('topsheet.category'), 1],
    ['account_width', t('topsheet.account'), 0],
    ['sub_width', t('topsheet.sub_account'), 0],
    ['set_width', t('topsheet.set'), 0],
  ];
  const sels = {};
  const widthCells = [];
  for (const [f, label, min] of widths) {
    const sel = k.el('select', 'setup-input setup-select');
    sel.id = k.nextId();
    sel.name = f;
    sel.dataset.key = k.key('settings', 'coding', f);
    for (let w = min; w <= 6; w += 1) {
      const o = k.el('option', null, w === 0 ? t('topsheet.not_used') : String(w));
      o.value = String(w);
      sel.append(o);
    }
    sel.value = String(p[f]);
    sel.classList.add('ts-w-xs');
    sels[f] = sel;
    widthCells.push(tsInline(label, sel));
  }
  const sep = k.el('select', 'setup-input setup-select');
  sep.id = k.nextId();
  sep.name = 'separator';
  sep.dataset.key = k.key('settings', 'coding', 'separator');
  const known = SEPARATORS.some(([v]) => v === p.separator);
  for (const [v, label] of known ? SEPARATORS : [...SEPARATORS, [p.separator, `“${p.separator}”`]]) {
    const o = k.el('option', null, label);
    o.value = v;
    sep.append(o);
  }
  sep.value = p.separator;
  sep.classList.add('ts-w-m');

  const example = k.el('span', 'tss-example');
  example.setAttribute('aria-live', 'polite');
  const problem = k.el('p', 'setup-hint setup-hint-bad');
  const read = () => ({
    category_width: Number(sels.category_width.value),
    account_width: Number(sels.account_width.value),
    sub_width: Number(sels.sub_width.value),
    set_width: Number(sels.set_width.value),
    separator: sep.value,
  });
  const apply = k.el('button', 'sw-act sw-act-primary', t('topsheet.apply'));
  apply.type = 'submit';
  apply.dataset.key = k.key('settings', 'coding', 'apply');
  const update = () => {
    const q = read();
    const bad = codingProblem(q);
    example.replaceChildren(k.el('span', 'setup-muted', t('topsheet.example')), k.el('strong', 'setup-num', bad ? '?' : exampleNumber(q)));
    problem.textContent = bad || '';
    problem.hidden = !bad;
    const same = Object.keys(q).every((f) => q[f] === p[f]);
    apply.hidden = same;
  };
  // A renumber preview still open (renumber.js) keeps the pattern it is for.
  const open = pendingRenumber(k.view() && k.view().doc);
  if (open) {
    for (const f of Object.keys(sels)) sels[f].value = String(open.q[f]);
    sep.value = open.q.separator;
  }
  const reset = () => {
    for (const f of Object.keys(sels)) sels[f].value = String(p[f]);
    sep.value = p.separator;
    update();
    sels.category_width.focus();
  };
  for (const s of [...Object.values(sels), sep]) s.addEventListener('change', update);
  update();

  form.addEventListener('submit', (e) => {
    e.preventDefault();
    if (k.isBusy()) return;
    const q = read();
    const bad = codingProblem(q);
    if (bad) {
      problem.textContent = bad;
      problem.hidden = false;
      return;
    }
    // Offers to renumber the accounts when any number would change.
    applyCoding(k, ck, q, form, {
      onProblem: (text) => {
        problem.textContent = text;
        problem.hidden = false;
      },
      onCancel: reset,
    });
  });
  const tip =
    t('topsheet.numbers_you_type_from_now_on_are');
  form.append(
    heading(t('topsheet.account_numbers')),
    tsRow(t('topsheet.digits_per_part'), widthCells, { group: true, tip }),
    tsRow(t('topsheet.separator'), [sep, example, apply]),
    problem,
  );
  if (k.state().errors.has(ck)) form.append(k.errorLine(k.state().errors.get(ck)));
  if (open) form.append(previewBox(k, ck, open, reset));
  return form;
}
