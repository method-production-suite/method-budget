// Budget Setup's Credits tab: incentive programs side by side and What if…
// (a part of topsheet-setup.js).

import { invoke } from './api.js';
import { gridTable, band, setFooterTotals } from './setupframe.js';
import { obj } from './util.js';
import { t } from './i18n.js';
import { picked, creditFoot, overCap, money } from './topsheet-setup.js';
import { arr, isNone, button, bandInput, bandTitle, locationNames, categoryNames, accountNames, head, cell, textCell, lead, gridRow } from './topsheet-setup-common.js';

let k = null; // the kit from setup.js, set by useKit() on every render

/** Takes the kit for this render (topsheet-setup.js calls it). */
export function useKit(kit) {
  k = kit;
}

// ---------------------------------------------------------------------------
// Incentive programs side by side (roadmap 3.6; decisions.md, entry 80)

// Every credit's estimate, applied or not, and spend by location, from the
// read-only `incentives` command: fetched once per budget revision, the last
// one kept for the footer until the new one comes.
const estimates = { stamp: '', p: null, last: null, lastStamp: '' };

function viewStamp() {
  const v = obj(k.view());
  return `${v.doc}:${v.rev}`;
}

function loadEstimates() {
  const stamp = viewStamp();
  if (estimates.stamp !== stamp || !estimates.p) {
    estimates.stamp = stamp;
    estimates.p = invoke('incentives', {}).then(
      (got) => {
        if (estimates.stamp === stamp) {
          estimates.last = obj(got);
          estimates.lastStamp = stamp;
        }
        return obj(got);
      },
      () => null,
    );
  }
  return estimates.p;
}

// An unapplied credit's row figures as if applied (decisions.md, entry 93),
// from the estimates of this very revision only; none until they come.
export function estimatedRows(i) {
  if (estimates.lastStamp !== viewStamp()) return [];
  const p = estimateOf(i);
  return p && p.figures && Array.isArray(p.figures.rows) ? p.figures.rows.map(obj) : [];
}

// Fills the row figures of unapplied credit `i`'s rows table in place, once
// the estimates come (no redraw, so focus stays where it is).
function fillEstimatedRows(i, rows) {
  rows.forEach((rt, n) => {
    const set = (f, text, title) => {
      const c = k.findByData('fig', k.key('credits', i, n, f));
      if (!c) return;
      const span = c.querySelector('.sw-num');
      if (span) span.textContent = text;
      c.title = title ? t('topsheet.estimate_title', { title }) : '';
    };
    if (isNone(rt.net)) return;
    const row = obj(arr(obj(arr(k.budget().credits)[i]).rows)[n]);
    const capKind = obj(row.cap).kind || 'none';
    const [overText, overTitle] = overCap(rt, { cap: { kind: capKind } });
    set('gross', k.formatCents(rt.gross), t('topsheet.the_rows_spend_before_fringes_are_left'));
    set('fringes', k.formatCents(rt.excluded_fringes), t('topsheet.fringes_left_out_of_the_spend'));
    set('net', k.formatCents(rt.net), t('topsheet.the_spend_the_percent_applies_to'));
    set('credit', money(rt.credit), t('topsheet.exactly_rows_added', { exact: k.formatDecimal(rt.credit) }));
    set('over', overText, overTitle);
  });
}

export function estimateOf(i) {
  const got = estimates.last;
  return got ? arr(got.programs).map(obj).find((p) => p.index === i) || null : null;
}

// The Credits tab's box under the editors: the programs side by side and
// the spend at each location, filled once the estimates come.
export function programsBox() {
  const box = k.el('section', 'tss-programs');
  box.setAttribute('aria-label', t('topsheet.programs_side_by_side'));
  box.dataset.key = 'credits-programs';
  return box;
}

export function showEstimates() {
  loadEstimates().then((got) => {
    const box = k.findByData('key', 'credits-programs');
    if (!box || !got) return;
    box.replaceChildren();
    const programs = arr(got.programs).map(obj);
    // Unapplied credits show their estimate in the list too.
    for (const p of programs) {
      if (p.applied || isNone(p.amount)) continue;
      const b = k.findByData('key', k.key('credits', 'pick', p.index));
      const fig = b && b.querySelector('.sw-tree-count');
      if (fig) {
        fig.textContent = t('topsheet.est_amount', { amount: k.formatCents(p.amount) });
        b.title = t('topsheet.est_title', { name: p.name });
      }
    }
    if (picked.credits < programs.length) setFooterTotals(creditFoot(picked.credits));
    // The picked credit, when not applied: its rows' estimated figures.
    const pickedProgram = programs.find((p) => p.index === picked.credits);
    if (pickedProgram && !pickedProgram.applied && estimates.lastStamp === viewStamp()) {
      fillEstimatedRows(pickedProgram.index, estimatedRows(pickedProgram.index));
    }
    // From a cleaned copy of a budget with problems: say the figures may be short.
    if (got.partial === true) {
      box.append(k.el('p', 'tss-note', t('topsheet.some_cells_have_problems_these_figures_count')));
    }
    if (programs.length) box.append(programsTable(programs));
    const spend = arr(got.locations).map(obj);
    if (spend.length) box.append(locationsTable(spend));
    if (programs.length) box.append(whatIfPanel());
  });
}

function where(list) {
  const names = arr(list).map(k.str);
  return names.length ? names.join(', ') : t('topsheet.anywhere');
}

function programsTable(programs) {
  const ul = k.el('ul');
  ul.setAttribute('aria-label', t('topsheet.programs_side_by_side'));
  for (const p of programs) {
    const li = gridRow();
    const spend = arr(p.by_location)
      .map(obj)
      .map((s) => `${isNone(s.location) ? t('topsheet.no_location') : k.str(s.location)} ${k.formatCents(s.net)}`)
      .join('; ');
    const f = obj(p.figures);
    const below = f.below_minimum === true;
    // Decisions.md, entry 88: the minimum, the shared cap and the fee.
    const minimum = isNone(p.minimum_spend) ? '' : k.formatCents(p.minimum_spend);
    const capName = isNone(p.shared_cap) ? '' : k.str(p.shared_cap);
    const capOf = isNone(p.shared_cap_amount) ? t('topsheet.not_in_the_budget') : k.formatCents(p.shared_cap_amount);
    const shared = capName === '' ? '' : isNone(f.shared) ? capName : `${capName}: ${k.formatCents(f.shared)}`;
    const sharedTitle =
      capName === ''
        ? ''
        : isNone(p.shared_cap_amount)
          ? t('topsheet.shared_cap_not_in_budget', { cap: capName })
          : isNone(f.shared)
            ? t('topsheet.shares_cap_within', { cap: capName, amount: capOf })
            : t('topsheet.share_of_cap', { cap: capName, amount: capOf });
    const fee = isNone(f.fee) || f.fee === 0 ? (p.broker_fee ? k.formatCents(0) : '') : k.formatCents(f.fee);
    li.append(
      lead(),
      textCell(k.str(p.name)),
      textCell(where(p.locations), { muted: !arr(p.locations).length }),
      textCell(k.formatCents(p.qualified), { num: true, title: spend ? t('topsheet.net_spend_by_location', { spend }) : '' }),
      textCell(minimum, {
        num: true,
        title: minimum === '' ? '' : below ? t('topsheet.below_the_minimum_spend_the_program_pays') : t('topsheet.the_qualified_spend_is_at_least_the'),
      }),
      textCell(shared, { muted: isNone(f.shared), title: sharedTitle }),
      textCell(fee, { num: true, title: p.broker_fee ? t('topsheet.flat_fee_title', { amount: k.formatCents(p.broker_fee) }) : '' }),
      textCell(isNone(p.amount) ? '' : k.formatCents(p.amount), {
        num: true,
        title: !isNone(p.problem)
          ? k.str(p.problem)
          : below
            ? t('topsheet.below_the_minimum_spend')
            : p.capped
              ? t('topsheet.held_down_by_the_credits_cap')
              : '',
      }),
      textCell(
        isNone(p.amount) ? t('topsheet.problem') : !isNone(p.problem) ? t('topsheet.may_be_short') : below ? t('topsheet.below_minimum') : p.applied ? t('topsheet.applied') : t('topsheet.estimate'),
        {
          muted: !p.applied,
          title: !isNone(p.problem)
            ? k.str(p.problem)
            : below
              ? t('topsheet.below_minimum_title', { qualified: k.formatCents(p.qualified), minimum })
              : p.applied
                ? ''
                : t('topsheet.not_applied_worked_out_as_if_applied'),
        },
      ),
    );
    ul.append(li);
  }
  return gridTable('programs', [
    lead(),
    head(t('topsheet.program')),
    head(t('topsheet.spend_at')),
    head(t('topsheet.qualified'), { num: true, title: t('topsheet.the_rows_net_spend_added_up') }),
    head(t('topsheet.minimum'), { num: true, title: t('topsheet.the_qualified_spend_below_which_the_program') }),
    head(t('topsheet.shared_cap'), { title: t('topsheet.a_cap_the_program_shares_with_others') }),
    head(t('topsheet.fee'), { num: true, title: t('topsheet.the_brokers_flat_fee_taken_off_after') }),
    head(t('topsheet.credit'), { num: true, title: t('topsheet.after_the_minimum_the_caps_the_discount') }),
    head(''),
  ], ul);
}

function locationsTable(spend) {
  const ul = k.el('ul');
  ul.setAttribute('aria-label', t('topsheet.spend_by_location'));
  for (const l of spend) {
    const li = gridRow();
    const name = isNone(l.location) ? t('topsheet.no_location') : k.str(l.location);
    li.append(
      lead(),
      textCell(name, { muted: isNone(l.location), title: l.known === false ? t('topsheet.not_in_the_budgets_locations') : '' }),
      textCell(k.formatCents(l.lines), { num: true }),
      textCell(k.formatCents(l.fringes), { num: true }),
      textCell(k.formatCents(l.total), { num: true }),
    );
    ul.append(li);
  }
  return gridTable('location-spend', [
    lead(),
    head(t('topsheet.location')),
    head(t('topsheet.lines'), { num: true }),
    head(t('topsheet.fringes'), { num: true, title: t('topsheet.the_fringes_the_lines_earn_wherever_fringes') }),
    head(t('topsheet.spend'), {
      num: true,
      title:
        t('topsheet.each_line_with_the_fringes_it_earns'),
    }),
  ], ul);
}

// ---------------------------------------------------------------------------
// What if… (decisions.md, entry 93): move lines to another location on paper
// and see every program before and after. `what_if` changes nothing;
// Apply runs `apply_what_if`, one undo step in the budget window. The
// panel's choices and last figures live here, so a redraw keeps them.

const whatIf = {
  open: false,
  lines: '', // 'category:CODE' or 'account:CODE'
  from: '*', // '*' anywhere, '' no location, else a location
  to: null, // a location, or '' for no location
  except: [],
  got: null, // the last figures, for `stamp`
  stamp: '',
  status: '',
};

function whatIfPick() {
  const [kind, code] = whatIf.lines.split(/:(.*)/s);
  return {
    accounts: kind === 'account' ? [code] : [],
    categories: kind === 'category' ? [code] : [],
    from: whatIf.from === '*' ? null : { location: whatIf.from === '' ? null : whatIf.from },
    except: whatIf.except,
    to: whatIf.to === '' ? null : whatIf.to,
  };
}

function placeName(l) {
  return isNone(l) || l === '' ? t('topsheet.no_location') : k.str(l);
}

// Works the what-if out and shows it (nothing changes).
function runWhatIf() {
  if (whatIf.lines === '' || whatIf.to === null) {
    whatIf.got = null;
    whatIf.status = t('topsheet.pick_the_lines_and_where_to_move');
    redrawWhatIf();
    return;
  }
  const stamp = viewStamp();
  whatIf.status = t('topsheet.working_it_out');
  redrawWhatIf();
  invoke('what_if', { pick: whatIfPick() }).then(
    (got) => {
      whatIf.got = obj(got);
      whatIf.stamp = stamp;
      whatIf.status = '';
      redrawWhatIf();
    },
    (e) => {
      whatIf.got = null;
      whatIf.status = k.str(e && e.message ? e.message : e);
      redrawWhatIf();
    },
  );
}

// Redraws the panel in place, focus staying on the same control.
function redrawWhatIf() {
  const old = k.findByData('item', 'credits-whatif');
  if (!old) return;
  const a = document.activeElement;
  const key = a && old.contains(a) && a.dataset ? a.dataset.key : '';
  old.replaceWith(whatIfPanel());
  const again = key ? k.findByData('key', key) : null;
  if (again) again.focus({ preventScroll: true });
}

function whatIfPanel() {
  const box = k.el('section', 'tss-whatif');
  box.dataset.item = 'credits-whatif';
  box.setAttribute('aria-label', t('topsheet.what_if_lines_moved'));
  const toggle = button(whatIf.open ? t('topsheet.close_what_if') : t('topsheet.what_if_ellipsis'), 'sw-act', 'credits-whatif-open', () => {
    whatIf.open = !whatIf.open;
    redrawWhatIf();
  });
  toggle.title = t('topsheet.see_what_each_program_would_come_to');
  box.append(band(t('topsheet.what_if'), bandTitle(t('topsheet.what_if_lines_were_shot_somewhere_else')), toggle));
  if (!whatIf.open) return box;

  const cats = categoryNames();
  const accts = accountNames();
  const places = locationNames();
  if (whatIf.to === null && places.length) whatIf.to = places[0];
  const pickSel = k.selectInput({
    fieldKey: 'credits-whatif-lines',
    value: whatIf.lines,
    options: [
      ['', t('topsheet.pick')],
      ...[...cats].map(([code, name]) => [`category:${code}`, t('topsheet.category_code_name', { code, name }).trim()]),
      ...[...accts].map(([code, name]) => [`account:${code}`, t('topsheet.account_code_name', { code, name }).trim()]),
    ],
    label: t('topsheet.lines_in'),
    onCommit: (v) => {
      whatIf.lines = v;
      whatIf.except = [];
      runWhatIf();
    },
  });
  const fromSel = k.selectInput({
    fieldKey: 'credits-whatif-from',
    value: whatIf.from,
    options: [['*', t('topsheet.anywhere')], ['', t('topsheet.no_location')], ...places.map((n) => [n, n])],
    label: t('topsheet.now_at'),
    onCommit: (v) => {
      whatIf.from = v;
      whatIf.except = [];
      runWhatIf();
    },
  });
  const toSel = k.selectInput({
    fieldKey: 'credits-whatif-to',
    value: whatIf.to === null ? '' : whatIf.to,
    options: [...places.map((n) => [n, n]), ['', t('topsheet.no_location')]],
    label: t('topsheet.move_to'),
    onCommit: (v) => {
      whatIf.to = v;
      runWhatIf();
    },
  });
  box.append(
    band(
      t('topsheet.what_if_which_lines_and_where'),
      bandInput(t('topsheet.lines_in'), pickSel, { title: t('topsheet.every_line_of_this_category_or_account') }),
      bandInput(t('topsheet.now_at'), fromSel, { title: t('topsheet.only_the_lines_at_this_location_now') }),
      bandInput(t('topsheet.move_to'), toSel, { title: t('topsheet.where_they_would_be_shot') }),
    ),
  );

  const got = whatIf.got;
  const fresh = got && whatIf.stamp === viewStamp();
  if (whatIf.status) box.append(k.el('p', 'tss-note', whatIf.status));
  if (!got) return box;
  if (!fresh) {
    box.append(k.el('p', 'tss-note', t('topsheet.the_budget_has_changed_since_pick_again')));
    return box;
  }
  if (got.partial === true) {
    box.append(k.el('p', 'tss-note', t('topsheet.some_cells_have_problems_these_figures_count_2')));
  }
  box.append(whatIfLines(got), whatIfPrograms(got), whatIfTotals(got));
  const spend = arr(got.locations).map(obj).filter((l) => l.before !== l.after);
  if (spend.length) box.append(whatIfSpend(spend));

  const moving = arr(got.lines).length;
  const apply = button(
    t('topsheet.move_lines_to', { count: moving, place: placeName(got.to) }),
    'setup-btn setup-btn-primary',
    'credits-whatif-apply',
    () => {
      const pick = whatIfPick();
      k.run('credits-whatif', () => ['apply_what_if', { pick }], {
        onOk: () => {
          whatIf.got = null;
          whatIf.except = [];
          whatIf.status = t('topsheet.moved_lines_to', { count: got.changed, place: placeName(got.to) });
        },
      });
    },
  );
  apply.disabled = !got.changed;
  apply.title = got.changed
    ? t('topsheet.changes_these_lines_location_in_the_budget')
    : t('topsheet.these_lines_are_already_there');
  box.append(band(t('topsheet.apply_the_what_if'), apply, k.el('span', 'tss-hint', t('topsheet.nothing_changes_until_you_apply_it'))));
  return box;
}

// The lines it moves, each with a box to leave it out.
function whatIfLines(got) {
  const ul = k.el('ul');
  ul.setAttribute('aria-label', t('topsheet.lines_it_moves'));
  const lines = arr(got.lines).map(obj);
  for (const l of lines) {
    const li = gridRow();
    const box = k.el('input', 'setup-check');
    box.type = 'checkbox';
    box.checked = true;
    box.dataset.key = k.key('credits-whatif-line', l.id);
    box.setAttribute('aria-label', t('topsheet.move_named', { name: k.str(l.description) || t('topsheet.line_n', { n: l.line }) }));
    box.addEventListener('change', () => {
      if (k.isBusy()) return;
      whatIf.except = [...whatIf.except, l.id];
      runWhatIf();
      const pick = k.findByData('key', 'credits-whatif-lines');
      if (pick) pick.focus({ preventScroll: true });
    });
    li.append(
      lead(),
      cell(box),
      textCell(`${k.str(l.account)}:${l.line}`),
      textCell(k.str(l.description)),
      textCell(placeName(l.location), { muted: isNone(l.location) }),
      textCell(k.formatCents(l.total), { num: true, title: t('topsheet.fringes_it_earns', { amount: k.formatCents(l.fringes) }) }),
    );
    ul.append(li);
  }
  if (!lines.length) return k.emptyNote(t('topsheet.no_lines_there_nothing_would_move'));
  return gridTable('whatif-lines', [lead(), head(''), head(t('topsheet.line')), head(t('topsheet.description')), head(t('topsheet.now_at')), head(t('topsheet.total'), { num: true })], ul);
}

// Every program before and after, and the change.
function whatIfPrograms(got) {
  const ul = k.el('ul');
  ul.setAttribute('aria-label', t('topsheet.programs_before_and_after'));
  for (const p of arr(got.programs).map(obj)) {
    const li = gridRow();
    const d = isNone(p.difference) ? '' : p.difference === 0 ? t('topsheet.no_change') : `${p.difference > 0 ? '+' : ''}${k.formatCents(p.difference)}`;
    li.append(
      lead(),
      textCell(k.str(p.name)),
      textCell(p.applied ? t('topsheet.applied') : t('topsheet.estimate'), { muted: !p.applied, title: p.applied ? t('topsheet.taken_off_the_grand_total') : t('topsheet.not_applied_worked_out_as_if_applied') }),
      textCell(isNone(p.before) ? '' : k.formatCents(p.before), { num: true, title: t('topsheet.qualified_spend_amount', { amount: k.formatCents(p.qualified_before) }) }),
      textCell(isNone(p.after) ? '' : k.formatCents(p.after), { num: true, title: t('topsheet.qualified_spend_amount', { amount: k.formatCents(p.qualified_after) }) }),
      textCell(d, { num: true, title: isNone(p.problem) ? '' : k.str(p.problem) }),
    );
    ul.append(li);
  }
  return gridTable('whatif-programs', [lead(), head(t('topsheet.program')), head(''), head(t('topsheet.before'), { num: true }), head(t('topsheet.after'), { num: true }), head(t('topsheet.change'), { num: true })], ul);
}

// The credits taken off, the estimates and the net total, before and after.
function whatIfTotals(got) {
  const ul = k.el('ul');
  ul.setAttribute('aria-label', t('topsheet.totals_before_and_after'));
  const b = obj(got.before);
  const a = obj(got.after);
  for (const [label, f, title] of [
    [t('topsheet.credits_taken_off'), 'credits_total', t('topsheet.the_applied_programs')],
    [t('topsheet.estimates_not_applied'), 'estimates_total', t('topsheet.the_programs_not_applied_each_as_if')],
    [t('topsheet.net_total_after_credits'), 'net_total', t('topsheet.the_grand_total_less_the_applied_programs')],
  ]) {
    const li = gridRow();
    const d = a[f] - b[f];
    li.append(
      lead(),
      textCell(label, { title }),
      textCell(k.formatCents(b[f]), { num: true }),
      textCell(k.formatCents(a[f]), { num: true }),
      textCell(d === 0 ? t('topsheet.no_change') : `${d > 0 ? '+' : ''}${k.formatCents(d)}`, { num: true }),
    );
    ul.append(li);
  }
  return gridTable('whatif-totals', [lead(), head(t('topsheet.totals')), head(t('topsheet.before'), { num: true }), head(t('topsheet.after'), { num: true }), head(t('topsheet.change'), { num: true })], ul);
}

// The spend at the locations that change.
function whatIfSpend(spend) {
  const ul = k.el('ul');
  ul.setAttribute('aria-label', t('topsheet.spend_by_location_before_and_after'));
  for (const l of spend) {
    const li = gridRow();
    li.append(
      lead(),
      textCell(placeName(l.location), { muted: isNone(l.location) }),
      textCell(k.formatCents(l.before), { num: true }),
      textCell(k.formatCents(l.after), { num: true }),
      textCell(`${l.after - l.before > 0 ? '+' : ''}${k.formatCents(l.after - l.before)}`, { num: true }),
    );
    ul.append(li);
  }
  return gridTable('whatif-totals', [lead(), head(t('topsheet.spend_at')), head(t('topsheet.before'), { num: true }), head(t('topsheet.after'), { num: true }), head(t('topsheet.change'), { num: true })], ul);
}
