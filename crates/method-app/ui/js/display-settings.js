// Display settings in the tools panel: the "Numbers on screen" group of
// Budget > Settings (topsheet-setup.js), and each global's details in the
// Globals tab (setup.js): description, unit, decimals, show by name, folder.
// Both are saved with the budget and are display only: no value or total
// changes (src/display.rs).
//
// They use setup.js's kit (`k`): its command queue, focus handling, error
// lines and controls, so they look and behave like the rest of the panel.
// Budget text only reaches the page through textContent and input values.

import { MAX_PLACES, budgetDisplay } from './display.js';

// Globals whose details are open, by name, so a re-render keeps them open.
const openDetails = new Set();

function checkbox(k, fieldKey, checked, labelText, onChange) {
  const box = k.el('input', 'setup-check');
  box.type = 'checkbox';
  box.id = k.nextId();
  box.checked = checked;
  box.dataset.key = fieldKey;
  box.addEventListener('change', () => onChange(box.checked));
  const label = k.el('label', 'setup-check-label', labelText);
  label.htmlFor = box.id;
  const wrap = k.el('div', 'setup-field setup-check-row');
  wrap.append(box, label);
  return wrap;
}

// A Budget > Settings row (the same markup as topsheet-setup.js's rows): a
// label naming a group of controls, each with its own label.
function settingsRow(k, labelText, controls) {
  const row = k.el('div', 'ts-row');
  const label = k.el('span', 'ts-row-label', labelText);
  label.id = k.nextId();
  const ctl = k.el('div', 'ts-row-ctl');
  ctl.setAttribute('role', 'group');
  ctl.setAttribute('aria-labelledby', label.id);
  ctl.append(...controls);
  row.append(label, ctl);
  return row;
}

function inline(k, labelText, control) {
  const wrap = k.el('span', 'ts-inline');
  const label = k.el('label', 'ts-inline-label', labelText);
  label.htmlFor = control.id;
  wrap.append(label, control);
  return wrap;
}

/** Options for a decimals select: `none` first (value ''), then 0 to 8. */
function placesOptions(none) {
  const out = [['', none]];
  for (let n = 0; n <= MAX_PLACES; n += 1) out.push([String(n), n === 1 ? '1 decimal' : `${n} decimals`]);
  return out;
}

function placesValue(n) {
  return Number.isInteger(n) ? String(n) : '';
}

function placesArg(v) {
  return v === '' ? null : Number(v);
}

// ---------------------------------------------------------------------------
// Budget > Settings: numbers on screen

/** The settings as the command takes them, from the newest view. */
function currentPrefs(k) {
  const d = budgetDisplay(k.budget());
  return {
    rate_cutoff: d.rate_cutoff,
    rate_decimals_below: d.rate_decimals_below,
    rate_decimals_above: d.rate_decimals_above,
    rate_global_decimals: d.rate_global_decimals,
    summary_rows: d.summary_rows,
    net_total: d.net_total,
  };
}

/** The "Numbers on screen" group for Budget > Settings. */
export function displaySettingsGroup(k) {
  const gk = k.key('settings', 'display');
  const group = k.el('div', 'tss-group');
  group.dataset.item = gk;
  const now = currentPrefs(k);
  const save = (patch, restore) => k.run(gk, () => ['set_display_prefs', { prefs: { ...currentPrefs(k), ...patch } }], { restore });

  const cutKey = k.key('settings', 'display', 'cutoff');
  const cutoff = k.textInput({
    fieldKey: cutKey,
    value: now.rate_cutoff === null ? '' : now.rate_cutoff,
    num: true,
    placeholder: 'None',
    onCommit: (v, old) => save({ rate_cutoff: v.trim() === '' ? null : v.trim() }, [cutKey, old]),
  });
  const belowKey = k.key('settings', 'display', 'below');
  const below = k.selectInput({
    fieldKey: belowKey,
    value: placesValue(now.rate_decimals_below),
    options: placesOptions('As typed'),
    onCommit: (v, old) => save({ rate_decimals_below: placesArg(v) }, [belowKey, old]),
  });
  const aboveKey = k.key('settings', 'display', 'above');
  const above = k.selectInput({
    fieldKey: aboveKey,
    value: placesValue(now.rate_decimals_above),
    options: placesOptions('As typed'),
    onCommit: (v, old) => save({ rate_decimals_above: placesArg(v) }, [aboveKey, old]),
  });
  cutoff.classList.add('ts-w-xs');
  below.classList.add('ts-w-s');
  above.classList.add('ts-w-s');
  const flag = (f, label) => {
    const fieldKey = k.key('settings', 'display', f);
    return checkbox(k, fieldKey, now[f], label, (on) => save({ [f]: on }, [fieldKey, !on]));
  };
  const heading = k.el('h3', 'tss-heading', 'Numbers on screen');
  heading.title = 'How rates and the topsheet show on screen. Only the display changes: every value and total stays exact.';
  group.append(
    heading,
    settingsRow(k, 'Rate decimals', [
      inline(k, 'Cutoff', cutoff),
      inline(k, 'Below', below),
      inline(k, now.rate_cutoff === null ? 'Rates' : 'At or above', above),
    ]),
    settingsRow(k, 'Rates using a global', [flag('rate_global_decimals', 'Show the global’s own decimals')]),
    settingsRow(k, 'Topsheet shows', [
      flag('summary_rows', 'Above- and below-the-line totals'),
      flag('net_total', 'Net total'),
    ]),
    k.el('p', 'ts-hint ts-row-note', 'Display only: every value and total stays exact.'),
  );
  if (k.state().errors.has(gk)) group.append(k.errorLine(k.state().errors.get(gk)));
  return group;
}

// ---------------------------------------------------------------------------
// Globals tab: one global's details

/** What global `name` says about itself, as the command takes it, from the newest view. */
function currentInfo(k, name) {
  const g = k.own(k.budget().global_info, name) || {};
  return {
    description: k.str(g.description),
    unit: k.str(g.unit),
    decimals: Number.isInteger(g.decimals) ? g.decimals : null,
    show_by_name: g.show_by_name === true,
    folder: k.str(g.folder),
  };
}

/**
 * A global's details, under its row in the Globals tab: a disclosure whose
 * summary says what is set, holding the fields. `itemKey` is the row's item
 * key (errors show there).
 */
export function globalInfoBlock(k, name, itemKey) {
  const now = currentInfo(k, name);
  const save = (patch, restore) =>
    k.run(itemKey, () => ['set_global_info', { name, info: { ...currentInfo(k, name), ...patch } }], { restore });

  const details = k.el('details', 'setup-more');
  details.open = openDetails.has(name);
  details.addEventListener('toggle', () => {
    if (details.open) openDetails.add(name);
    else openDetails.delete(name);
  });
  const units = k.budget().units && typeof k.budget().units === 'object' ? k.budget().units : {};
  const unitPlural = now.unit && units[now.unit] && units[now.unit].plural ? units[now.unit].plural : now.unit;
  const said = [now.description, unitPlural, now.folder].filter(Boolean);
  if (now.decimals !== null) said.push(now.decimals === 1 ? '1 decimal' : `${now.decimals} decimals`);
  if (now.show_by_name) said.push('shown by name');
  const summary = k.el('summary', 'setup-more-summary', said.length ? said.join(', ') : 'Details');
  summary.setAttribute('aria-label', `Details of ${name}${said.length ? `: ${said.join(', ')}` : ''}`);
  summary.dataset.key = k.key('globals', name, 'details');
  details.append(summary);

  const fk = (f) => k.key('globals', name, 'info', f);
  const desc = k.textInput({
    fieldKey: fk('description'),
    value: now.description,
    label: `Description of ${name}`,
    onCommit: (v, old) => save({ description: v }, [fk('description'), old]),
  });
  const unitNames = Object.keys(units).sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
  const unitOptions = [['', 'None'], ...unitNames.map((u) => [u, u])];
  // A unit the budget no longer has stays listed, so the select shows it.
  if (now.unit && !unitNames.includes(now.unit)) unitOptions.push([now.unit, `${now.unit} (not in Units)`]);
  const unit = k.selectInput({
    fieldKey: fk('unit'),
    value: now.unit,
    options: unitOptions,
    onCommit: (v, old) => save({ unit: v }, [fk('unit'), old]),
  });
  const decimals = k.selectInput({
    fieldKey: fk('decimals'),
    value: placesValue(now.decimals),
    options: placesOptions('As calculated'),
    onCommit: (v, old) => save({ decimals: placesArg(v) }, [fk('decimals'), old]),
  });
  const folder = k.textInput({
    fieldKey: fk('folder'),
    value: now.folder,
    placeholder: 'None',
    onCommit: (v, old) => save({ folder: v }, [fk('folder'), old]),
  });
  const grid = k.el('div', 'setup-fields');
  grid.append(
    k.field('Description', desc, { wide: true }),
    k.field('Unit', unit),
    k.field('Decimals', decimals),
    k.field('Folder', folder, { wide: true }),
    checkbox(k, fk('show_by_name'), now.show_by_name, 'Show by name: cells that use it show their formula', (on) =>
      save({ show_by_name: on }, [fk('show_by_name'), !on]),
    ),
  );
  details.append(grid);
  return details;
}
