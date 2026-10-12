// Display settings in Budget Setup: the "Numbers on screen" group of
// Settings (topsheet-setup.js), and each global's details in the Globals
// table (setup.js): its description and unit as cells, and all of them
// (description, unit, decimals, show by name, folder) in a small form.
// Both are saved with the budget and are display only: no value or total
// changes (src/display.rs).
//
// They use setup.js's kit (`k`): its command queue, focus handling, error
// lines and controls, so they look and behave like the rest of the panel.
// Budget text only reaches the page through textContent and input values.

import { MAX_PLACES, budgetDisplay } from './display.js';
import { anchoredForm } from './modal.js';

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

/** Units a global can be in: the budget's, plus `current` when it is gone. */
function unitOptions(k, current) {
  const units = k.budget().units && typeof k.budget().units === 'object' ? k.budget().units : {};
  const names = Object.keys(units).sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
  const out = [['', 'None'], ...names.map((u) => [u, u])];
  // A unit the budget no longer has stays listed, so the pick shows it.
  if (current && !names.includes(current)) out.push([current, `${current} (not in Units)`]);
  return out;
}

/**
 * A global's description or unit as a cell of Budget Setup's Globals
 * table (`field` 'description' or 'unit'), saved on its own as one undo
 * step. `itemKey` is the row's item key (errors show there).
 */
export function globalInfoCell(k, name, itemKey, field) {
  const now = currentInfo(k, name);
  const fieldKey = k.key('globals', name, 'info', field);
  const onCommit = (v, old) =>
    k.run(itemKey, () => ['set_global_info', { name, info: { ...currentInfo(k, name), [field]: v } }], { restore: [fieldKey, old] });
  if (field === 'unit') {
    return k.selectInput({ fieldKey, value: now.unit, options: unitOptions(k, now.unit), label: `Units of ${name}`, cls: 'sw-cell-input', onCommit });
  }
  return k.textInput({ fieldKey, value: now.description, label: `Description of ${name}`, cls: 'sw-cell-input', onCommit });
}

/**
 * A global's details (description, unit, decimals, folder, show by name)
 * in a small form next to `anchor`; one undo step, none when nothing
 * changed. Resolves once it closes.
 */
export function globalDetails(k, name, anchor) {
  const now = currentInfo(k, name);
  const ctx = k.state().ctx;
  return anchoredForm({
    anchor,
    title: `Details of ${name}`,
    message: 'Display only: no value or total changes.',
    fields: [
      { key: 'description', label: 'Description', value: now.description },
      { key: 'unit', label: 'Unit', type: 'select', options: unitOptions(k, now.unit), value: now.unit },
      { key: 'decimals', label: 'Decimals', type: 'select', options: placesOptions('As calculated'), value: placesValue(now.decimals) },
      { key: 'folder', label: 'Folder', value: now.folder, placeholder: 'None' },
      { key: 'show_by_name', label: 'Show by name: cells that use it show their formula', type: 'check', value: now.show_by_name },
    ],
    onSubmit: (v) => {
      const info = {
        description: k.str(v.description),
        unit: k.str(v.unit),
        decimals: placesArg(v.decimals),
        show_by_name: Boolean(v.show_by_name),
        folder: k.str(v.folder),
      };
      if (JSON.stringify(info) === JSON.stringify(currentInfo(k, name))) return Promise.resolve();
      if (!ctx || typeof ctx.call !== 'function') return Promise.reject('Not connected');
      return ctx.call('set_global_info', { name, info });
    },
  });
}
