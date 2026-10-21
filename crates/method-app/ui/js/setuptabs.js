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

import { t } from './i18n.js';

export const SETUP_TABS = Object.freeze([
  {
    id: 'fringes',
    label: t('setup.tab.fringes'),
    library: 'fringe',
    list: true,
    hint: t('setup.tab.fringes_hint'),
  },
  {
    id: 'groups',
    label: t('setup.tab.groups'),
    library: 'group',
    list: true,
    hint: t('setup.tab.groups_hint'),
  },
  {
    id: 'globals',
    label: t('setup.tab.globals'),
    library: 'global',
    list: true,
    hint: t('setup.tab.globals_hint'),
  },
  {
    id: 'units',
    label: t('setup.tab.units'),
    library: 'unit',
    list: true,
    hint: t('setup.tab.units_hint'),
  },
  { id: 'sets', label: t('setup.tab.sets'), library: 'set', list: true, hint: t('setup.tab.sets_hint') },
  {
    id: 'locations',
    label: t('setup.tab.locations'),
    library: 'location',
    list: true,
    hint: t('setup.tab.locations_hint'),
  },
  {
    id: 'currencies',
    label: t('setup.tab.currencies'),
    library: 'currency',
    list: true,
    hint: t('setup.tab.currencies_hint'),
  },
  {
    id: 'charges',
    label: t('setup.tab.charges'),
    library: 'charge',
    list: false,
    hint: t('setup.tab.charges_hint'),
  },
  {
    id: 'credits',
    label: t('setup.tab.credits'),
    library: 'credit',
    list: false,
    hint: t('setup.tab.credits_hint'),
  },
  {
    id: 'rates',
    label: t('setup.tab.rates'),
    library: 'rate_table',
    list: false,
    hint: t('setup.tab.rates_hint'),
  },
  { id: 'info', label: t('setup.tab.info'), library: null, list: false, hint: t('setup.tab.info_hint') },
  { id: 'settings', label: t('setup.tab.settings'), library: null, list: false, hint: '' },
  {
    id: 'baseline',
    label: t('setup.tab.baseline'),
    library: null,
    list: false,
    hint: t('setup.tab.baseline_hint'),
  },
]);

export const SETUP_TAB_IDS = Object.freeze(SETUP_TABS.map((t) => t.id));

/** True when `id` names a Budget Setup tab. */
export function isSetupTab(id) {
  return SETUP_TAB_IDS.includes(id);
}
