// Topsheet setup tabs: charges, credits, sets, locations, budget info,
// settings (currencies, fringe posting, account numbers) and the baseline.
//
// These tabs live in the setup panel. setup.js owns the panel, the tab bar, the
// command queue and focus handling, and passes its helpers in as `kit`, so
// every tab looks and behaves the same. Budget text only reaches the page
// through textContent and input values, never innerHTML.

export const EXTRA_TABS = [
  { id: 'sets', label: 'Sets' },
  { id: 'locations', label: 'Locations' },
  { id: 'charges', label: 'Charges' },
  { id: 'credits', label: 'Credits' },
  { id: 'info', label: 'Info' },
  { id: 'settings', label: 'Settings' },
  { id: 'baseline', label: 'Baseline' },
];

const DECIMAL = /^[+-]?(\d+\.?\d*|\.\d+)$/;
const DATE = /^\d{4}-\d{2}-\d{2}$/;
const SEPARATORS = [
  ['-', 'Hyphen  11-01'],
  ['.', 'Point  11.01'],
  [' ', 'Space  11 01'],
  ['/', 'Slash  11/01'],
  [':', 'Colon  11:01'],
  [',', 'Comma  11,01'],
  [';', 'Semicolon  11;01'],
  ['', 'None  1101'],
];

let k = null; // the kit from setup.js, set on every render

// <details> sections that are open, by key, so a re-render keeps them open.
const openSections = new Set();

export function renderExtraTab(tab, panel, kit) {
  k = kit;
  const render = {
    sets: renderSets,
    locations: renderLocations,
    charges: renderCharges,
    credits: renderCredits,
    info: renderInfo,
    settings: renderSettings,
    baseline: renderBaseline,
  }[tab];
  if (render) render(panel);
}

// ---------------------------------------------------------------------------
// Small helpers

function arr(v) {
  return Array.isArray(v) ? v : [];
}

function obj(v) {
  return v && typeof v === 'object' && !Array.isArray(v) ? v : {};
}

function isNone(v) {
  return v === undefined || v === null;
}

function intro(text) {
  return k.el('p', 'setup-intro', text);
}

function heading(text) {
  return k.el('h3', 'tss-heading', text);
}

function issuesAt(kind, index) {
  const list = k.view() && k.view().issues;
  if (!Array.isArray(list)) return [];
  return list.filter((i) => i && i.target && i.target.kind === kind && i.target.index === index);
}

// The report entry for item `index` of `list`: reports only list applied items.
function appliedEntry(items, reported, index) {
  if (!Array.isArray(reported)) return undefined;
  if (obj(items[index]).applied === false) return undefined;
  let n = 0;
  for (let i = 0; i < index; i += 1) if (obj(items[i]).applied !== false) n += 1;
  return reported[n];
}

function button(text, cls, fieldKey, onClick, label) {
  const b = k.el('button', cls, text);
  b.type = 'button';
  if (fieldKey) b.dataset.key = fieldKey;
  if (label) {
    b.setAttribute('aria-label', label);
    b.title = label;
  }
  b.addEventListener('click', () => {
    if (k.isBusy() || b.disabled) return;
    onClick();
  });
  return b;
}

function checkbox(fieldKey, checked, labelText, onChange) {
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

// Up and down buttons for an ordered list. Focus follows the moved item.
function moveButtons(tab, index, count, what, name, command) {
  const wrap = k.el('span', 'tss-move');
  const go = (to) => {
    k.setFocusKey(k.key(tab, to, to === 0 ? 'down' : 'up'));
    k.run(k.key(tab, index), () => [command, { index, to }], {
      onError: () => k.setFocusKey(null),
    });
  };
  const up = button('↑', 'tss-move-btn', k.key(tab, index, 'up'), () => go(index - 1), `Move ${what} ${name} up`);
  const down = button('↓', 'tss-move-btn', k.key(tab, index, 'down'), () => go(index + 1), `Move ${what} ${name} down`);
  up.disabled = index === 0;
  down.disabled = index >= count - 1;
  wrap.append(up, down);
  return wrap;
}

// A collapsible section that stays open across re-renders.
function section(sectionKey, title, ...children) {
  const d = k.el('details', 'tss-more');
  d.open = openSections.has(sectionKey);
  const s = k.el('summary', 'tss-more-title', title);
  s.dataset.key = k.key('section', sectionKey);
  d.addEventListener('toggle', () => {
    if (d.open) openSections.add(sectionKey);
    else openSections.delete(sectionKey);
  });
  d.append(s, ...children);
  return d;
}

// Chips for a set of names picked from `options`, with a select to add one.
function chips({ fieldKey, label, selected, options, onChange, emptyText = 'None' }) {
  const wrap = k.el('div', 'setup-field setup-field-wide');
  const head = k.el('span', 'setup-label', label);
  head.id = k.nextId();
  const row = k.el('div', 'tss-chips');
  row.setAttribute('role', 'list');
  row.setAttribute('aria-labelledby', head.id);
  if (!selected.length) row.append(k.el('span', 'tss-chips-empty', emptyText));
  let addSel = null;
  selected.forEach((name) => {
    const chip = k.el('span', 'tss-chip');
    chip.setAttribute('role', 'listitem');
    chip.append(k.el('span', 'tss-chip-text', name));
    chip.append(
      button(
        '×',
        'tss-chip-x',
        k.key(fieldKey, 'remove', name),
        () => {
          // The chip goes away; keep focus in this field.
          if (addSel) addSel.focus({ preventScroll: true });
          onChange(selected.filter((n) => n !== name));
        },
        `Remove ${name}`,
      ),
    );
    row.append(chip);
  });
  const left = options.filter((o) => !selected.includes(o));
  if (left.length) {
    const sel = k.el('select', 'setup-input setup-select tss-chip-add');
    addSel = sel;
    sel.id = k.nextId();
    sel.dataset.key = k.key(fieldKey, 'add');
    sel.setAttribute('aria-label', `Add to ${label}`);
    const first = k.el('option', null, 'Add…');
    first.value = '';
    sel.append(first);
    for (const o of left) {
      const opt = k.el('option', null, o);
      opt.value = o;
      sel.append(opt);
    }
    sel.addEventListener('change', () => {
      if (sel.value === '' || k.isBusy()) {
        sel.value = '';
        return;
      }
      onChange([...selected, sel.value]);
    });
    row.append(sel);
  }
  wrap.append(head, row);
  return wrap;
}

// A yes/no box under an item, in the same style as setup.js's delete confirm.
function confirmBox(itemKey, opener, text, yesText, danger, build) {
  const li = k.findByData('item', itemKey);
  if (!li) return;
  const open = li.querySelector(':scope > .setup-confirm');
  if (open) {
    open.querySelector('button').focus();
    return;
  }
  const box = k.el('div', 'setup-confirm');
  box.setAttribute('role', 'group');
  const p = k.el('p', 'setup-confirm-text', text);
  p.id = k.nextId();
  box.setAttribute('aria-describedby', p.id);
  const yes = k.el('button', danger ? 'setup-btn setup-btn-danger' : 'setup-btn setup-btn-primary', yesText);
  yes.type = 'button';
  const no = k.el('button', 'setup-btn setup-btn-quiet', 'Cancel');
  no.type = 'button';
  const close = () => {
    box.remove();
    opener.focus();
  };
  no.addEventListener('click', close);
  box.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      close();
    }
  });
  yes.addEventListener('click', () => {
    if (k.isBusy()) return;
    k.run(itemKey, build);
  });
  const actions = k.el('div', 'setup-confirm-actions');
  actions.append(yes, no);
  box.append(p, actions);
  li.append(box);
  yes.focus();
}

function groupNames() {
  return k.entries(k.budget().groups).map(([n]) => n);
}

function fringeNames() {
  return k.entries(k.budget().fringes).map(([n]) => n);
}

function categoryCodes() {
  return arr(k.budget().topsheet)
    .filter((r) => obj(r).kind === 'category')
    .map((r) => k.str(r.code));
}

function accountCodes() {
  const out = [];
  for (const r of arr(k.budget().topsheet)) {
    if (obj(r).kind !== 'category') continue;
    for (const a of arr(r.accounts)) out.push(k.str(obj(a).code));
  }
  return out;
}

// Names already picked stay listed even if they no longer exist.
function withKept(options, picked) {
  const out = [...options];
  for (const p of picked) if (!out.includes(p)) out.push(p);
  return out;
}

// ---------------------------------------------------------------------------
// Sets and locations

function breakdownStat(label, bd) {
  const b = obj(bd);
  let total;
  try {
    total = BigInt(b.details_total || 0) + BigInt(b.fringe_total || 0);
  } catch {
    return null;
  }
  const wrap = k.el('span', 'setup-stat');
  wrap.append(k.el('span', 'setup-muted', label), k.el('span', 'setup-num', k.formatCents(total)));
  wrap.title = `Lines ${k.formatCents(b.details_total || 0)} plus fringes ${k.formatCents(b.fringe_total || 0)}`;
  return wrap;
}

function setArgs(name, patch) {
  const s = obj(k.own(k.budget().sets, name));
  return ['set_set', { name, set: { description: k.str(s.description), ...patch } }];
}

function renderSets(panel) {
  const b = k.budget();
  const r = k.report();
  panel.append(intro('Sets tag lines with the scene or build they are for, so you can see what each one costs.'));
  const items = k.entries(b.sets);
  const names = items.map(([n]) => n);
  if (!items.length) panel.append(k.emptyNote('No sets yet.'));
  const ul = k.list('Sets');
  items.forEach(([name, s0], i) => {
    const s = obj(s0);
    const ik = k.key('sets', name);
    const li = k.itemShell(ik, 'setup-block');
    const head = k.el('div', 'setup-head');
    const nameEl = k.el('span', 'setup-name', name);
    nameEl.title = name;
    const figs = k.el('span', 'setup-figs');
    const totals = r ? k.own(r.sets, name) : undefined;
    const stat = totals ? breakdownStat('Total', totals) : null;
    if (stat) figs.append(stat);
    else if (r) figs.append(k.el('span', 'setup-muted', 'Not used'));
    head.append(
      nameEl,
      figs,
      k.deleteButton({
        itemKey: ik,
        name,
        what: 'set',
        confirmText: `Delete set ${name}? Lines tagged with it lose the tag.`,
        build: () => ['delete_set', { name }],
        neighborKey: k.neighbor('sets', names, i, k.key('add', 'sets', 'name')),
      }),
    );
    const grid = k.el('div', 'setup-fields');
    const desc = k.textInput({
      fieldKey: k.key('sets', name, 'description'),
      value: k.str(s.description),
      onCommit: (v, old) =>
        k.run(ik, () => setArgs(name, { description: v }), { restore: [k.key('sets', name, 'description'), old] }),
    });
    grid.append(k.field('Description', desc, { wide: true }));
    li.append(head, grid);
    k.finishItem(li, ik, [], null);
    ul.append(li);
  });
  if (items.length) panel.append(ul);
  panel.append(
    k.addForm('sets', {
      title: 'New set',
      fields: [
        k.addInput('sets', 'name', { label: 'Name', placeholder: 'Diner' }),
        k.addInput('sets', 'description', { label: 'Description', placeholder: 'Interior, night' }),
      ],
      validate: (v) => {
        const bad = k.checkName(v.name.trim(), b.sets, 'set');
        return bad ? ['name', bad] : null;
      },
      build: (v) => ['set_set', { name: v.name.trim(), set: { description: v.description.trim() } }],
    }),
  );
}

function locationArgs(name, patch) {
  const l = obj(k.own(k.budget().locations, name));
  return [
    'set_location',
    {
      name,
      location: {
        description: k.str(l.description),
        city: k.str(l.city),
        region: k.str(l.region),
        country: k.str(l.country),
        ...patch,
      },
    },
  ];
}

function renderLocations(panel) {
  const b = k.budget();
  const r = k.report();
  panel.append(intro('Places you shoot. Tag lines with a location to see what each one costs.'));
  const items = k.entries(b.locations);
  const names = items.map(([n]) => n);
  if (!items.length) panel.append(k.emptyNote('No locations yet.'));
  const ul = k.list('Locations');
  items.forEach(([name, l0], i) => {
    const l = obj(l0);
    const ik = k.key('locations', name);
    const li = k.itemShell(ik, 'setup-block');
    const head = k.el('div', 'setup-head');
    const nameEl = k.el('span', 'setup-name', name);
    nameEl.title = name;
    const figs = k.el('span', 'setup-figs');
    const totals = r ? k.own(r.locations, name) : undefined;
    const stat = totals ? breakdownStat('Total', totals) : null;
    if (stat) figs.append(stat);
    else if (r) figs.append(k.el('span', 'setup-muted', 'Not used'));
    head.append(
      nameEl,
      figs,
      k.deleteButton({
        itemKey: ik,
        name,
        what: 'location',
        confirmText: `Delete location ${name}? Lines tagged with it lose the tag.`,
        build: () => ['delete_location', { name }],
        neighborKey: k.neighbor('locations', names, i, k.key('add', 'locations', 'name')),
      }),
    );
    const input = (f) =>
      k.textInput({
        fieldKey: k.key('locations', name, f),
        value: k.str(l[f]),
        onCommit: (v, old) =>
          k.run(ik, () => locationArgs(name, { [f]: v }), { restore: [k.key('locations', name, f), old] }),
      });
    const grid = k.el('div', 'setup-fields');
    grid.append(k.field('Description', input('description'), { wide: true }));
    const place = k.el('div', 'setup-fields setup-field-wide tss-fields-3');
    place.append(k.field('City', input('city')), k.field('Region', input('region')), k.field('Country', input('country')));
    grid.append(place);
    li.append(head, grid);
    k.finishItem(li, ik, [], null);
    ul.append(li);
  });
  if (items.length) panel.append(ul);
  panel.append(
    k.addForm('locations', {
      title: 'New location',
      fields: [
        k.addInput('locations', 'name', { label: 'Name', placeholder: 'Stage 4' }),
        k.addInput('locations', 'city', { label: 'City', placeholder: 'Burbank' }),
      ],
      validate: (v) => {
        const bad = k.checkName(v.name.trim(), b.locations, 'location');
        return bad ? ['name', bad] : null;
      },
      build: (v) => [
        'set_location',
        { name: v.name.trim(), location: { description: '', city: v.city.trim(), region: '', country: '' } },
      ],
    }),
  );
}

// ---------------------------------------------------------------------------
// Charges

function chargeInput(c0) {
  const c = obj(c0);
  return {
    name: k.str(c.name),
    account: isNone(c.account) ? null : k.str(c.account),
    applied: c.applied !== false,
    kind: c.kind === 'flat' ? 'flat' : 'percent',
    percent: isNone(c.percent) ? null : k.str(c.percent),
    amount: isNone(c.amount) ? null : k.centsToText(c.amount),
    currency: isNone(c.currency) ? null : k.str(c.currency),
    excluded_amounts: arr(c.excluded_amounts).map((x) => ({
      description: k.str(obj(x).description),
      amount: k.centsToText(obj(x).amount),
    })),
    excluded_groups: arr(c.excluded_groups).map(k.str),
    excluded_categories: arr(c.excluded_categories).map(k.str),
    excluded_accounts: arr(c.excluded_accounts).map(k.str),
  };
}

// set_charge for charge `index`, from the newest view, with `patch` applied.
// `patch` may be a function of the current input.
function chargeArgs(index, patch) {
  const cur = chargeInput(arr(k.budget().charges)[index]);
  const p = typeof patch === 'function' ? patch(cur) : patch;
  return ['set_charge', { index, charge: { ...cur, ...p } }];
}

function currencyOptions(current) {
  const base = k.baseCode();
  const codes = k.entries(k.budget().currencies).map(([c]) => c);
  if (current && current !== base && !codes.includes(current)) codes.push(current);
  return [['', base ? `${base} (base)` : 'Base currency'], ...codes.map((c) => [c, c])];
}

function renderCharges(panel) {
  const b = k.budget();
  const r = k.report();
  const charges = arr(b.charges);
  panel.append(
    intro(
      'Fees worked out on the above- plus below-the-line total, like contingency or insurance. Each one is figured on that total alone, never on another charge.',
    ),
  );
  if (!charges.length) panel.append(k.emptyNote('No charges yet.'));
  const ul = k.list('Charges');
  charges.forEach((c0, i) => {
    const c = chargeInput(c0);
    const ik = k.key('charges', i);
    const li = k.itemShell(ik, 'setup-block');
    if (!c.applied) li.classList.add('setup-item-off');
    const fk = (f) => k.key('charges', i, f);
    const commit = (f, patchOf) => (v, old) =>
      k.run(ik, () => chargeArgs(i, patchOf(v)), { restore: [fk(f), old] });

    const head = k.el('div', 'setup-head');
    const nameEl = k.el('span', 'setup-name', c.name);
    nameEl.title = c.name;
    const figs = k.el('span', 'setup-figs');
    const entry = appliedEntry(charges, r && r.charges, i);
    if (!c.applied) figs.append(k.el('span', 'setup-muted', 'Not applied'));
    else if (entry) {
      const t = k.el('span', 'setup-num', k.formatCents(entry.amount));
      t.title = `On ${k.formatCents(entry.base)}, less ${k.formatCents(entry.excluded)} left out`;
      figs.append(t);
    }
    head.append(
      nameEl,
      figs,
      moveButtons('charges', i, charges.length, 'charge', c.name, 'move_charge'),
      k.deleteButton({
        itemKey: ik,
        name: c.name,
        what: 'charge',
        confirmText: `Delete charge ${c.name}?`,
        build: () => ['delete_charge', { index: i }],
        neighborKey: charges.length > 1 ? k.key('del', k.key('charges', i < charges.length - 1 ? i : i - 1)) : k.key('add', 'charges', 'name'),
      }),
    );

    const grid = k.el('div', 'setup-fields');
    const name = k.textInput({ fieldKey: fk('name'), value: c.name, onCommit: commit('name', (v) => ({ name: v })) });
    const applied = checkbox(fk('applied'), c.applied, 'Applied', (v) =>
      k.run(ik, () => chargeArgs(i, { applied: v }), { restore: [fk('applied'), !v] }),
    );
    const kind = k.selectInput({
      fieldKey: fk('kind'),
      value: c.kind,
      options: [
        ['percent', 'Percent of the total'],
        ['flat', 'Flat fee'],
      ],
      onCommit: commit('kind', (v) => (cur) =>
        v === 'flat'
          ? { kind: 'flat', amount: cur.amount === null ? '0' : cur.amount }
          : { kind: 'percent', percent: cur.percent === null ? '0' : cur.percent },
      ),
    });
    grid.append(k.field('Name', name), applied, k.field('Kind', kind));
    if (c.kind === 'percent') {
      const pct = k.textInput({
        fieldKey: fk('percent'),
        value: k.str(c.percent),
        num: true,
        onCommit: commit('percent', (v) => ({ percent: v.trim() })),
      });
      grid.append(k.field('Percent', pct, { extra: '%' }));
    } else {
      const amount = k.textInput({
        fieldKey: fk('amount'),
        value: isNone(c0.amount) ? '' : k.formatCents(c0.amount),
        num: true,
        onCommit: commit('amount', (v) => ({ amount: v.trim() })),
      });
      grid.append(k.field('Amount', amount));
      const cur = k.selectInput({
        fieldKey: fk('currency'),
        value: c.currency || '',
        options: currencyOptions(c.currency),
        onCommit: commit('currency', (v) => ({ currency: v === '' ? null : v })),
      });
      grid.append(k.field('Currency', cur));
    }
    const account = k.textInput({
      fieldKey: fk('account'),
      value: c.account || '',
      placeholder: 'None',
      onCommit: commit('account', (v) => ({ account: v.trim() === '' ? null : v.trim() })),
    });
    grid.append(k.field('Posts to account', account, { wide: c.kind === 'percent' }));
    li.append(head, grid, chargeExclusions(i, c, ik));
    k.finishItem(li, ik, issuesAt('charge', i), null);
    ul.append(li);
  });
  if (charges.length) panel.append(ul);

  const taken = {};
  for (const c of charges) taken[k.str(obj(c).name)] = true;
  panel.append(
    k.addForm('charges', {
      title: 'New charge',
      fields: [
        k.addInput('charges', 'name', { label: 'Name', placeholder: 'Insurance' }),
        k.addSelect('charges', 'kind', 'Kind', [
          ['percent', 'Percent'],
          ['flat', 'Flat fee'],
        ]),
        k.addInput('charges', 'value', { label: 'Percent or amount', placeholder: '10', num: true }),
      ],
      validate: (v) => {
        const name = v.name.trim();
        if (name === '') return ['name', 'Enter a charge name.'];
        if (taken[name]) return ['name', `There is already a charge named ${name}.`];
        const val = v.value.trim();
        if (val === '') return ['value', v.kind === 'flat' ? 'Enter an amount.' : 'Enter a percent.'];
        if (v.kind !== 'flat' && !DECIMAL.test(val)) return ['value', 'The percent must be a number, like 10 or 2.5.'];
        return null;
      },
      build: (v) => {
        const flat = v.kind === 'flat';
        return [
          'set_charge',
          {
            index: null,
            charge: {
              name: v.name.trim(),
              account: null,
              applied: true,
              kind: flat ? 'flat' : 'percent',
              percent: flat ? null : v.value.trim(),
              amount: flat ? v.value.trim() : null,
              currency: null,
              excluded_amounts: [],
              excluded_groups: [],
              excluded_categories: [],
              excluded_accounts: [],
            },
          },
        ];
      },
    }),
  );
}

function chargeExclusions(i, c, ik) {
  const fk = (f) => k.key('charges', i, f);
  const count =
    c.excluded_groups.length + c.excluded_amounts.length + (c.kind === 'percent' ? c.excluded_categories.length + c.excluded_accounts.length : 0);
  const body = k.el('div', 'setup-fields tss-more-body');
  const pick = (f, label, options) =>
    chips({
      fieldKey: fk(f),
      label,
      selected: c[f],
      options: withKept(options, c[f]),
      onChange: (list) => k.run(ik, () => chargeArgs(i, { [f]: list })),
    });
  body.append(pick('excluded_groups', 'Groups', groupNames()));
  if (c.kind === 'percent') {
    body.append(pick('excluded_categories', 'Categories', categoryCodes()));
    body.append(pick('excluded_accounts', 'Accounts', accountCodes()));
  } else {
    body.append(k.el('p', 'setup-hint setup-field-wide', 'A flat fee only leaves out groups and the amounts below.'));
  }

  // Amounts typed in by hand.
  const rawAmounts = arr(obj(arr(k.budget().charges)[i]).excluded_amounts);
  const amounts = k.el('div', 'setup-field setup-field-wide');
  amounts.append(k.el('span', 'setup-label', 'Amounts'));
  const rows = k.el('div', 'tss-amounts');
  c.excluded_amounts.forEach((x, n) => {
    const row = k.el('div', 'tss-amount');
    const desc = k.textInput({
      fieldKey: fk(`amount-desc-${n}`),
      value: x.description,
      placeholder: 'Description',
      label: `Description of amount ${n + 1}`,
      onCommit: (v, old) =>
        k.run(
          ik,
          () => chargeArgs(i, (cur) => ({ excluded_amounts: cur.excluded_amounts.map((y, j) => (j === n ? { ...y, description: v } : y)) })),
          { restore: [fk(`amount-desc-${n}`), old] },
        ),
    });
    const amt = k.textInput({
      fieldKey: fk(`amount-value-${n}`),
      value: k.formatCents(obj(rawAmounts[n]).amount),
      num: true,
      label: `Amount ${n + 1}`,
      onCommit: (v, old) =>
        k.run(
          ik,
          () => chargeArgs(i, (cur) => ({ excluded_amounts: cur.excluded_amounts.map((y, j) => (j === n ? { ...y, amount: v.trim() } : y)) })),
          { restore: [fk(`amount-value-${n}`), old] },
        ),
    });
    const del = button('×', 'setup-del', fk(`amount-del-${n}`), () =>
      k.run(ik, () => chargeArgs(i, (cur) => ({ excluded_amounts: cur.excluded_amounts.filter((_, j) => j !== n) }))),
      `Remove amount ${n + 1}`,
    );
    row.append(desc, amt, del);
    rows.append(row);
  });
  const add = button('Add amount', 'setup-btn tss-small', fk('amount-add'), () => {
    k.setFocusKey(fk(`amount-desc-${c.excluded_amounts.length}`));
    k.run(ik, () => chargeArgs(i, (cur) => ({ excluded_amounts: [...cur.excluded_amounts, { description: '', amount: '0' }] })), {
      onError: () => k.setFocusKey(null),
    });
  });
  rows.append(add);
  amounts.append(rows);
  body.append(amounts);
  return section(`charge-${i}`, count ? `Left out of this charge (${count})` : 'Left out of this charge', body);
}

// ---------------------------------------------------------------------------
// Credits

function creditRowInput(r0) {
  const r = obj(r0);
  const src = obj(r.source);
  const cap = obj(r.cap);
  return {
    description: k.str(r.description),
    source:
      src.kind === 'group'
        ? { kind: 'group', group: k.str(src.group), excluded_fringes: arr(src.excluded_fringes).map(k.str) }
        : { kind: 'flat', amount: k.centsToText(isNone(src.amount) ? 0 : src.amount) },
    percent: k.str(r.percent) || '0',
    cap:
      cap.kind === 'qualified_spend' || cap.kind === 'credit'
        ? { kind: cap.kind, amount: k.centsToText(cap.amount) }
        : { kind: 'none' },
  };
}

function creditInput(c0) {
  const c = obj(c0);
  return {
    name: k.str(c.name),
    account: isNone(c.account) ? null : k.str(c.account),
    applied: c.applied !== false,
    cap: isNone(c.cap) ? null : k.centsToText(c.cap),
    discount_percent: k.str(c.discount_percent) || '0',
    rows: arr(c.rows).map(creditRowInput),
  };
}

function creditArgs(index, patch) {
  const cur = creditInput(arr(k.budget().credits)[index]);
  const p = typeof patch === 'function' ? patch(cur) : patch;
  return ['set_credit', { index, credit: { ...cur, ...p } }];
}

function rowPatch(n, f) {
  return (cur) => ({ rows: cur.rows.map((r, j) => (j === n ? { ...r, ...f(r) } : r)) });
}

function renderCredits(panel) {
  const b = k.budget();
  const r = k.report();
  const credits = arr(b.credits);
  panel.append(
    intro(
      'Incentives and rebates taken off the grand total to give the net total. Each row is a percent of some spend; the row credits add up, then the cap and discount apply.',
    ),
  );
  if (!credits.length) panel.append(k.emptyNote('No credits yet.'));
  const ul = k.list('Credits');
  credits.forEach((c0, i) => {
    const c = creditInput(c0);
    const raw = obj(c0);
    const ik = k.key('credits', i);
    const li = k.itemShell(ik, 'setup-block');
    if (!c.applied) li.classList.add('setup-item-off');
    const fk = (f) => k.key('credits', i, f);
    const commit = (f, patchOf) => (v, old) =>
      k.run(ik, () => creditArgs(i, patchOf(v)), { restore: [fk(f), old] });
    const entry = appliedEntry(credits, r && r.credits, i);

    const head = k.el('div', 'setup-head');
    const nameEl = k.el('span', 'setup-name', c.name);
    nameEl.title = c.name;
    const figs = k.el('span', 'setup-figs');
    if (!c.applied) figs.append(k.el('span', 'setup-muted', 'Not applied'));
    else if (entry) {
      const t = k.el('span', 'setup-num', k.formatCents(entry.amount));
      t.title = 'Credit taken off the grand total';
      figs.append(t);
    }
    head.append(
      nameEl,
      figs,
      moveButtons('credits', i, credits.length, 'credit', c.name, 'move_credit'),
      k.deleteButton({
        itemKey: ik,
        name: c.name,
        what: 'credit',
        confirmText: `Delete credit ${c.name} and its ${k.plural(c.rows.length, 'row', 'rows')}?`,
        build: () => ['delete_credit', { index: i }],
        neighborKey: credits.length > 1 ? k.key('del', k.key('credits', i < credits.length - 1 ? i : i - 1)) : k.key('add', 'credits', 'name'),
      }),
    );

    const grid = k.el('div', 'setup-fields');
    grid.append(
      k.field('Name', k.textInput({ fieldKey: fk('name'), value: c.name, onCommit: commit('name', (v) => ({ name: v })) })),
      checkbox(fk('applied'), c.applied, 'Applied', (v) =>
        k.run(ik, () => creditArgs(i, { applied: v }), { restore: [fk('applied'), !v] }),
      ),
      k.field(
        'Cap on the credit',
        k.textInput({
          fieldKey: fk('cap'),
          value: isNone(raw.cap) ? '' : k.formatCents(raw.cap),
          num: true,
          placeholder: 'None',
          onCommit: commit('cap', (v) => ({ cap: v.trim() === '' ? null : v.trim() })),
        }),
      ),
      k.field(
        'Discount',
        k.textInput({
          fieldKey: fk('discount'),
          value: c.discount_percent,
          num: true,
          onCommit: commit('discount', (v) => ({ discount_percent: v.trim() })),
        }),
        { extra: '%' },
      ),
      k.field(
        'Posts to account',
        k.textInput({
          fieldKey: fk('account'),
          value: c.account || '',
          placeholder: 'None',
          onCommit: commit('account', (v) => ({ account: v.trim() === '' ? null : v.trim() })),
        }),
        { wide: true },
      ),
    );

    const rows = k.el('ol', 'tss-rows');
    rows.setAttribute('aria-label', `Rows of ${c.name}`);
    c.rows.forEach((row, n) => rows.append(creditRow(i, n, row, raw, entry, ik)));
    const addRow = button('Add row', 'setup-btn tss-small', fk('add-row'), () => {
      k.setFocusKey(k.key('credits', i, c.rows.length, 'description'));
      k.run(
        ik,
        () =>
          creditArgs(i, (cur) => ({
            rows: [...cur.rows, { description: '', source: { kind: 'flat', amount: '0' }, percent: '0', cap: { kind: 'none' } }],
          })),
        { onError: () => k.setFocusKey(null) },
      );
    });
    const rowsWrap = k.el('div', 'tss-rows-wrap');
    rowsWrap.append(k.el('p', 'setup-label', c.rows.length ? 'Rows' : 'No rows yet'), rows, addRow);
    li.append(head, grid, rowsWrap);
    k.finishItem(li, ik, issuesAt('credit', i), null);
    ul.append(li);
  });
  if (credits.length) panel.append(ul);

  const taken = {};
  for (const c of credits) taken[k.str(obj(c).name)] = true;
  panel.append(
    k.addForm('credits', {
      title: 'New credit',
      fields: [k.addInput('credits', 'name', { label: 'Name', placeholder: 'State incentive' })],
      validate: (v) => {
        const name = v.name.trim();
        if (name === '') return ['name', 'Enter a credit name.'];
        if (taken[name]) return ['name', `There is already a credit named ${name}.`];
        return null;
      },
      build: (v) => [
        'set_credit',
        { index: null, credit: { name: v.name.trim(), account: null, applied: true, cap: null, discount_percent: '0', rows: [] } },
      ],
    }),
  );
}

function creditRow(i, n, row, raw, entry, ik) {
  const li = k.el('li', 'tss-row');
  const fk = (f) => k.key('credits', i, n, f);
  const send = (f, old) => (v) => k.run(ik, () => creditArgs(i, rowPatch(n, () => v)), old === undefined ? {} : { restore: [f, old] });
  const rawRow = obj(arr(raw.rows)[n]);

  const head = k.el('div', 'tss-row-head');
  head.append(k.el('span', 'tss-row-num', `Row ${n + 1}`));
  const figs = k.el('span', 'setup-figs');
  const rt = entry && Array.isArray(entry.rows) ? obj(entry.rows[n]) : null;
  if (rt && !isNone(rt.net)) {
    const t = k.el('span', 'setup-muted', `On ${k.formatCents(rt.net)}`);
    t.title = `Spend ${k.formatCents(rt.gross)}, less ${k.formatCents(rt.excluded_fringes)} of fringes left out`;
    figs.append(t);
  }
  head.append(
    figs,
    button('×', 'setup-del', fk('del'), () => k.run(ik, () => creditArgs(i, (cur) => ({ rows: cur.rows.filter((_, j) => j !== n) }))), `Delete row ${n + 1}`),
  );

  const grid = k.el('div', 'setup-fields');
  grid.append(
    k.field(
      'Description',
      k.textInput({
        fieldKey: fk('description'),
        value: row.description,
        onCommit: (v, old) => send(fk('description'), old)({ description: v }),
      }),
      { wide: true },
    ),
  );

  const groups = groupNames();
  const current = row.source.kind === 'group' ? `group:${row.source.group}` : 'flat';
  const srcOptions = [['flat', 'Amount typed in'], ...withKept(groups, row.source.kind === 'group' ? [row.source.group] : []).map((g) => [`group:${g}`, `Group: ${g}`])];
  grid.append(
    k.field(
      'Spend',
      k.selectInput({
        fieldKey: fk('source'),
        value: current,
        options: srcOptions,
        onCommit: (v, old) =>
          send(fk('source'), old)(
            v === 'flat'
              ? { source: { kind: 'flat', amount: '0' } }
              : { source: { kind: 'group', group: v.slice(6), excluded_fringes: [] } },
          ),
      }),
      { wide: true },
    ),
  );
  if (row.source.kind === 'flat') {
    grid.append(
      k.field(
        'Amount',
        k.textInput({
          fieldKey: fk('amount'),
          value: k.formatCents(obj(rawRow.source).amount || 0),
          num: true,
          onCommit: (v, old) => send(fk('amount'), old)({ source: { kind: 'flat', amount: v.trim() } }),
        }),
      ),
    );
  }
  grid.append(
    k.field(
      'Percent',
      k.textInput({
        fieldKey: fk('percent'),
        value: row.percent,
        num: true,
        onCommit: (v, old) => send(fk('percent'), old)({ percent: v.trim() }),
      }),
      { extra: '%' },
    ),
  );
  grid.append(
    k.field(
      'Cap',
      k.selectInput({
        fieldKey: fk('cap-kind'),
        value: row.cap.kind,
        options: [
          ['none', 'No cap'],
          ['qualified_spend', 'Cap the spend'],
          ['credit', 'Cap the credit'],
        ],
        onCommit: (v, old) =>
          send(fk('cap-kind'), old)({ cap: v === 'none' ? { kind: 'none' } : { kind: v, amount: row.cap.amount || '0' } }),
      }),
    ),
  );
  if (row.cap.kind !== 'none') {
    grid.append(
      k.field(
        'Cap amount',
        k.textInput({
          fieldKey: fk('cap-amount'),
          value: k.formatCents(obj(rawRow.cap).amount || 0),
          num: true,
          onCommit: (v, old) => send(fk('cap-amount'), old)({ cap: { kind: row.cap.kind, amount: v.trim() } }),
        }),
      ),
    );
  }
  if (row.source.kind === 'group') {
    grid.append(
      chips({
        fieldKey: fk('fringes'),
        label: 'Fringes left out of the spend',
        selected: row.source.excluded_fringes,
        options: withKept(fringeNames(), row.source.excluded_fringes),
        onChange: (list) =>
          k.run(ik, () => creditArgs(i, rowPatch(n, (r) => ({ source: { ...r.source, excluded_fringes: list } })))),
      }),
    );
  }
  li.append(head, grid);
  return li;
}

// ---------------------------------------------------------------------------
// Budget info

const INFO_KINDS = [
  ['text', 'Text'],
  ['number', 'Number'],
  ['date', 'Date'],
];

function infoCheck(kind, value) {
  const v = value.trim();
  if (kind === 'number' && !DECIMAL.test(v)) return 'Enter a number, like 30 or 2.5.';
  if (kind === 'date' && !DATE.test(v)) return 'Enter a date as YYYY-MM-DD, like 2027-03-01.';
  return null;
}

function renderInfo(panel) {
  const b = k.budget();
  panel.append(intro('Facts about the production, like the director, shoot days or start date. They are notes only and don’t feed formulas.'));
  const items = k.entries(b.info);
  const names = items.map(([n]) => n);
  if (!items.length) panel.append(k.emptyNote('No fields yet.'));
  const ul = k.list('Budget information');
  items.forEach(([name, v0], i) => {
    const v = obj(v0);
    const kind = ['text', 'number', 'date'].includes(v.kind) ? v.kind : 'text';
    const ik = k.key('info', name);
    const li = k.itemShell(ik, 'setup-block');
    const head = k.el('div', 'setup-head');
    const nameEl = k.el('span', 'setup-name', name);
    nameEl.title = name;
    head.append(
      nameEl,
      k.el('span', 'setup-figs'),
      k.deleteButton({
        itemKey: ik,
        name,
        what: 'field',
        confirmText: `Delete the field ${name}?`,
        build: () => ['delete_info', { name }],
        neighborKey: k.neighbor('info', names, i, k.key('add', 'info', 'name')),
      }),
    );
    const grid = k.el('div', 'setup-fields tss-info');
    const valueKey = k.key('info', name, 'value');
    const value = k.textInput({
      fieldKey: valueKey,
      value: k.str(v.value),
      num: kind === 'number',
      placeholder: kind === 'date' ? 'YYYY-MM-DD' : '',
      onCommit: (val, old) => k.run(ik, () => ['set_info', { name, value: { kind, value: val } }], { restore: [valueKey, old] }),
    });
    const kindSel = k.selectInput({
      fieldKey: k.key('info', name, 'kind'),
      value: kind,
      options: INFO_KINDS,
      onCommit: (nk, old) =>
        k.run(ik, () => ['set_info', { name, value: { kind: nk, value: k.str(obj(k.own(k.budget().info, name)).value) } }], {
          restore: [k.key('info', name, 'kind'), old],
        }),
    });
    grid.append(k.field('Kind', kindSel), k.field('Value', value));
    li.append(head, grid);
    k.finishItem(li, ik, [], null);
    ul.append(li);
  });
  if (items.length) panel.append(ul);
  panel.append(
    k.addForm('info', {
      title: 'New field',
      fields: [
        k.addInput('info', 'name', { label: 'Label', placeholder: 'Director' }),
        k.addSelect('info', 'kind', 'Kind', INFO_KINDS),
        k.addInput('info', 'value', { label: 'Value', placeholder: '' }),
      ],
      validate: (v) => {
        const name = v.name.trim();
        if (name === '') return ['name', 'Enter a label.'];
        if (k.own(b.info, name) !== undefined) return ['name', `There is already a field named ${name}.`];
        const bad = infoCheck(v.kind, v.value);
        return bad ? ['value', bad] : null;
      },
      build: (v) => ['set_info', { name: v.name.trim(), value: { kind: v.kind, value: v.value.trim() } }],
    }),
  );
}

// ---------------------------------------------------------------------------
// Settings: currencies, fringe posting, account numbers

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

// The pattern rules from fwb_core::coding, so the form can explain a problem
// before it is applied. The app checks them again.
function codingProblem(p) {
  if (p.category_width < 1) return 'Every account number starts with a category, so its width is at least 1.';
  const w = [p.category_width, p.account_width, p.sub_width, p.set_width];
  const names = ['category', 'account', 'sub-account', 'set'];
  for (let i = 1; i < 4; i += 1) {
    if (w[i] > 0 && w[i - 1] === 0) return `The ${names[i]} part needs the ${names[i - 1]} part before it.`;
  }
  return null;
}

function fringeAccountExample(p, suffix) {
  const first = categoryCodes()[0] || '11';
  if (!p.account_width || !suffix) return null;
  const cat = fit(first, p.category_width);
  return { cat: first, number: [cat, fit(suffix, p.account_width)].join(p.separator) };
}

function renderSettings(panel) {
  const b = k.budget();
  const base = k.baseCode();
  const ik = k.key('settings', 'currency');

  // Currency
  const cur = k.el('div', 'tss-group');
  cur.dataset.item = ik;
  const baseInput = k.textInput({
    fieldKey: k.key('settings', 'base'),
    value: base,
    onCommit: (v, old) => k.run(ik, () => ['set_base_currency', { code: v.trim() }], { restore: [k.key('settings', 'base'), old] }),
  });
  const totals = k.selectInput({
    fieldKey: k.key('settings', 'totals'),
    value: k.str(b.totals_currency),
    options: currencyOptions(k.str(b.totals_currency)),
    onCommit: (v, old) =>
      k.run(ik, () => ['set_totals_currency', { code: v === '' ? null : v }], { restore: [k.key('settings', 'totals'), old] }),
  });
  const grid = k.el('div', 'setup-fields');
  grid.append(k.field('Base currency', baseInput), k.field('Show totals in', totals));
  cur.append(
    heading('Currency'),
    grid,
    k.el('p', 'setup-hint tss-note', 'Everything is worked out in the base currency. Renaming it doesn’t convert any amounts or rates.'),
  );
  if (k.state().errors.has(ik)) cur.append(k.errorLine(k.state().errors.get(ik)));
  panel.append(cur);

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
      ['category', 'An account at the end of each category'],
      ['account', 'A line at the end of each account'],
      ['production_level', 'A row above each production total'],
      ['budget', 'One total for the whole budget'],
    ],
    onCommit: (v, old) => k.run(fk, () => ['set_fringe_posting', { mode: v, suffix: null }], { restore: [k.key('settings', 'posting'), old] }),
  });
  const fgrid = k.el('div', 'setup-fields');
  fgrid.append(k.field('Fringes show as', modeSel, { wide: true }));
  if (mode === 'category') {
    const suf = k.textInput({
      fieldKey: k.key('settings', 'suffix'),
      value: suffix,
      onCommit: (v, old) =>
        k.run(fk, () => ['set_fringe_posting', { mode: 'category', suffix: v.trim() }], { restore: [k.key('settings', 'suffix'), old] }),
    });
    fgrid.append(k.field('Account number ends in', suf));
    const ex = fringeAccountExample(coding(), suffix);
    if (ex) fgrid.append(k.el('p', 'setup-hint tss-example-inline', `Category ${ex.cat}: ${ex.number}`));
  }
  fr.append(heading('Fringe totals'), fgrid);
  if (mode === 'budget') {
    fr.append(k.el('p', 'setup-hint tss-note', 'A budget-wide fringe total sits outside above- and below-the-line, so charges don’t apply to it.'));
  }
  if (k.state().errors.has(fk)) fr.append(k.errorLine(k.state().errors.get(fk)));
  panel.append(fr);

  panel.append(codingForm());
}

function codingForm() {
  const ck = k.key('settings', 'coding');
  const p = coding();
  const form = k.el('form', 'tss-group');
  form.noValidate = true;
  form.dataset.item = ck;
  const widths = [
    ['category_width', 'Category', 1],
    ['account_width', 'Account', 0],
    ['sub_width', 'Sub-account', 0],
    ['set_width', 'Set', 0],
  ];
  const sels = {};
  const grid = k.el('div', 'setup-fields tss-widths');
  for (const [f, label, min] of widths) {
    const sel = k.el('select', 'setup-input setup-select');
    sel.id = k.nextId();
    sel.name = f;
    sel.dataset.key = k.key('settings', 'coding', f);
    for (let w = min; w <= 6; w += 1) {
      const o = k.el('option', null, w === 0 ? 'Not used' : String(w));
      o.value = String(w);
      sel.append(o);
    }
    sel.value = String(p[f]);
    sels[f] = sel;
    grid.append(k.field(label, sel));
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
  grid.append(k.field('Separator', sep, { wide: true }));

  const example = k.el('p', 'tss-example');
  example.setAttribute('aria-live', 'polite');
  const problem = k.el('p', 'setup-hint setup-hint-bad');
  const read = () => ({
    category_width: Number(sels.category_width.value),
    account_width: Number(sels.account_width.value),
    sub_width: Number(sels.sub_width.value),
    set_width: Number(sels.set_width.value),
    separator: sep.value,
  });
  const apply = k.el('button', 'setup-btn setup-btn-primary', 'Apply');
  apply.type = 'submit';
  apply.dataset.key = k.key('settings', 'coding', 'apply');
  const update = () => {
    const q = read();
    const bad = codingProblem(q);
    example.replaceChildren(k.el('span', 'setup-muted', 'Example '), k.el('strong', 'setup-num', bad ? '?' : exampleNumber(q)));
    problem.textContent = bad || '';
    problem.hidden = !bad;
    const same = Object.keys(q).every((f) => q[f] === p[f]);
    apply.hidden = same;
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
    k.setFocusKey(k.key('settings', 'coding', 'category_width'));
    k.run(ck, () => ['set_account_coding', { ...q, separator: q.separator === '' ? null : q.separator }], {
      onError: () => k.setFocusKey(null),
    });
  });
  const actions = k.el('div', 'setup-add-actions tss-coding-actions');
  actions.append(example, apply);
  form.append(
    heading('Account numbers'),
    grid,
    actions,
    problem,
    k.el('p', 'setup-hint tss-note', 'Numbers you type from now on are read and written with this pattern. Numbers already in the budget keep their spelling.'),
  );
  if (k.state().errors.has(ck)) form.append(k.errorLine(k.state().errors.get(ck)));
  return form;
}

// ---------------------------------------------------------------------------
// Baseline

function changedCount(map) {
  let n = 0;
  for (const [, c] of k.entries(map)) if (obj(c).variance !== 0) n += 1;
  return n;
}

function renderBaseline(panel) {
  const b = k.budget();
  const view = k.view() || {};
  const variance = view.variance && typeof view.variance === 'object' ? view.variance : null;
  const has = b.baseline && typeof b.baseline === 'object';
  const ik = k.key('baseline');
  panel.append(
    intro(
      'Lock the current totals as the original budget. From then on every line, account and category can show its original total and how far it has moved.',
    ),
  );
  const box = k.el('div', 'tss-group');
  box.dataset.item = ik;

  if (has) {
    const table = k.el('table', 'tss-table');
    const thead = k.el('thead');
    const hr = k.el('tr');
    for (const h of ['', 'Original', 'Current', 'Change']) {
      const th = k.el('th', h ? 'setup-right' : null, h);
      th.scope = 'col';
      hr.append(th);
    }
    thead.append(hr);
    const tbody = k.el('tbody');
    const line = (label, ch0, fallback) => {
      const ch = obj(ch0);
      const tr = k.el('tr');
      const th = k.el('th', null, label);
      th.scope = 'row';
      const orig = isNone(ch.original) ? fallback : ch.original;
      const v = ch.variance;
      const change = k.el('td', 'setup-num setup-right', isNone(v) ? '' : k.formatCents(v));
      if (typeof v === 'number' && v !== 0) change.classList.add(v > 0 ? 'tss-up' : 'tss-down');
      tr.append(
        th,
        k.el('td', 'setup-num setup-right', isNone(orig) ? '' : k.formatCents(orig)),
        k.el('td', 'setup-num setup-right', isNone(ch.current) ? '' : k.formatCents(ch.current)),
        change,
      );
      tbody.append(tr);
    };
    line('Grand total', variance && variance.grand_total, b.baseline.grand_total);
    line('Net total', variance && variance.net_total, b.baseline.net_total);
    table.append(thead, tbody);
    box.append(table);
    if (variance) {
      const lines = changedCount(variance.rows);
      const accounts = changedCount(variance.accounts);
      box.append(
        k.el(
          'p',
          'setup-hint tss-note',
          lines || accounts
            ? `Changed since locked: ${k.plural(lines, 'line', 'lines')} in ${k.plural(accounts, 'account', 'accounts')}.`
            : 'Nothing has changed since the totals were locked.',
        ),
      );
    } else {
      box.append(k.el('p', 'setup-hint tss-note', 'Current totals can’t be worked out right now, so there is nothing to compare.'));
    }
    if (view.partial) box.append(k.el('p', 'setup-hint tss-note', 'Current totals are partial while the budget has errors.'));
    const actions = k.el('div', 'tss-actions');
    const relock = button('Lock again', 'setup-btn', k.key('baseline', 'relock'), () =>
      confirmBox(ik, relock, 'Replace the original totals with the current ones?', 'Lock again', false, () => ['lock_baseline', {}]),
    );
    const clear = button('Clear', 'setup-btn', k.key('baseline', 'clear'), () =>
      confirmBox(ik, clear, 'Clear the original totals? The budget stops showing what changed.', 'Clear', true, () => ['clear_baseline', {}]),
    );
    actions.append(relock, clear);
    box.append(actions);
  } else {
    box.append(k.el('p', 'setup-empty', 'No original totals locked yet.'));
    if (view.error) {
      box.append(k.el('p', 'setup-hint tss-note', 'The budget has a problem to fix before its totals can be locked.'));
    }
    const actions = k.el('div', 'tss-actions');
    actions.append(
      button('Lock current totals', 'setup-btn setup-btn-primary', k.key('baseline', 'lock'), () => {
        k.setFocusKey(k.key('baseline', 'relock'));
        k.run(ik, () => ['lock_baseline', {}], { onError: () => k.setFocusKey(null) });
      }),
    );
    box.append(actions);
  }
  if (k.state().errors.has(ik)) box.append(k.errorLine(k.state().errors.get(ik)));
  panel.append(box);
}
