// The Budget Setup window's tabs (setupwin.js, src/setupwin.rs), in order:
// the setup tables, then the budget's own setup. View > Budget Setup lists
// them with their keys (src/menu.rs), and those keys open the window on the
// tab, from any window (main.js, toolswin.js, setupwin.js).
//
// `library` is the setup library's kind for the table (library.js): the
// left rail's Library switch shows the library's items of that kind; null
// for none (and no rail). `list` marks the setup lists (setuplist.js),
// whose rail shows their sort and Show. `hint` says what the tab is for,
// in the tab's tooltip.

export const SETUP_TABS = Object.freeze([
  {
    id: 'fringes',
    label: 'Fringes',
    library: 'fringe',
    list: true,
    hint: 'Fringes add to the lines that carry them: a percent of pay, up to an optional cutoff per earner, or a flat amount per line or per unit of time.',
  },
  {
    id: 'groups',
    label: 'Groups',
    library: 'group',
    list: true,
    hint: 'Groups gather lines from any account for reports and totals. A group that does not count in the total leaves its lines out of the budget total.',
  },
  {
    id: 'globals',
    label: 'Globals',
    library: 'global',
    list: true,
    hint: 'Named numbers you can use in any formula, like DAYS * 5. A global can use other globals.',
  },
  {
    id: 'units',
    label: 'Units',
    library: 'unit',
    list: true,
    hint: 'Units for quantities on lines. Hours lets flat fringes convert between time units; leave it blank for units like Flat.',
  },
  { id: 'sets', label: 'Sets', library: 'set', list: true, hint: 'Sets mark lines with the set they belong to, for reports and totals.' },
  {
    id: 'locations',
    label: 'Locations',
    library: 'location',
    list: true,
    hint: 'Locations mark lines with where the work happens, for reports and totals.',
  },
  {
    id: 'currencies',
    label: 'Currency',
    library: 'currency',
    list: true,
    hint: 'Currencies other than the base currency, each with its rate. Lines in another currency are converted at that rate.',
  },
  {
    id: 'charges',
    label: 'Charges',
    library: 'charge',
    list: false,
    hint: 'Fees worked out on the above- plus below-the-line total, like contingency or insurance. Each one is figured on that total alone, never on another charge.',
  },
  {
    id: 'credits',
    label: 'Credits',
    library: 'credit',
    list: false,
    hint: 'Incentives and rebates taken off the grand total to give the net total. Each row is a percent of some spend; the row credits add up, then the cap and discount apply.',
  },
  { id: 'info', label: 'Info', library: null, list: false, hint: 'Facts about the production. Notes only; they don’t feed formulas.' },
  { id: 'settings', label: 'Settings', library: null, list: false, hint: '' },
  {
    id: 'baseline',
    label: 'Baseline',
    library: null,
    list: false,
    hint: 'Lock the current totals as the original budget, so every line, account and category shows how far it has moved.',
  },
]);

export const SETUP_TAB_IDS = Object.freeze(SETUP_TABS.map((t) => t.id));

/** True when `id` names a Budget Setup tab. */
export function isSetupTab(id) {
  return SETUP_TAB_IDS.includes(id);
}
