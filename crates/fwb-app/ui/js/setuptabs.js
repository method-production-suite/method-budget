// The Budget Setup window's tabs (setupwin.js, src/setupwin.rs), in order:
// the setup tables, then the budget's own setup. View > Budget Setup lists
// them with their keys (src/menu.rs), and those keys open the window on the
// tab, from any window (main.js, toolswin.js, setupwin.js).
//
// `library` is the setup library's kind for the table (library.js), shown
// beside the tab when the window is wide enough; null for none.

export const SETUP_TABS = Object.freeze([
  { id: 'fringes', label: 'Fringes', library: 'fringe' },
  { id: 'groups', label: 'Groups', library: 'group' },
  { id: 'globals', label: 'Globals', library: 'global' },
  { id: 'units', label: 'Units', library: 'unit' },
  { id: 'sets', label: 'Sets', library: 'set' },
  { id: 'locations', label: 'Locations', library: 'location' },
  { id: 'currencies', label: 'Currency', library: 'currency' },
  { id: 'charges', label: 'Charges', library: 'charge' },
  { id: 'credits', label: 'Credits', library: 'credit' },
  { id: 'info', label: 'Info', library: null },
  { id: 'settings', label: 'Settings', library: null },
  { id: 'baseline', label: 'Baseline', library: null },
]);

export const SETUP_TAB_IDS = Object.freeze(SETUP_TABS.map((t) => t.id));

/** True when `id` names a Budget Setup tab. */
export function isSetupTab(id) {
  return SETUP_TAB_IDS.includes(id);
}
