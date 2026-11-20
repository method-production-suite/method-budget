// The budget window's column menu (a part of main.js): optional detail
// columns, Orig and Var, and the cost columns, saved per user.

import { t } from './i18n.js';
import { el } from './dom.js';
import { detailColumns, columnShown, OPTIONAL_DETAIL_COLUMNS, COST_COLUMNS } from './levels.js';
import { openMenu, updateMenu, isMenuOpen, setMenuAnchor } from './menu.js';
import { readPref, writePref } from './prefs.js';
import { P, state, renderAll } from './main.js';

// ---------------------------------------------------------------------------
// View: optional detail columns (the menu in the grid's top-left corner; saved per user)

export function readColumnChoices() {
  const saved = readPref('detailColumns', null);
  const out = {};
  if (saved && typeof saved === 'object') {
    for (const c of [...OPTIONAL_DETAIL_COLUMNS, ...COST_COLUMNS]) if (typeof saved[c.key] === 'boolean') out[c.key] = saved[c.key];
  }
  return out;
}

/**
 * What the column choices depend on: whether the budget has a baseline (Orig
 * and Var) and actuals (the cost report columns, levels.js).
 */
function columnContext() {
  return { baseline: Boolean(state.ix && state.ix.variance), cost: Boolean(state.ix && state.ix.cost), persons: Boolean(state.ix && state.ix.hasPersons) };
}

export function gridDetailColumns() {
  return detailColumns(state.columns, columnContext()).map((c) => (c.key === 'num' ? { ...c, head: columnMenuButton } : c));
}

/**
 * Topsheet and account-list columns: with a baseline, the column menu (Orig
 * and Var) sits in the Acct # header, as it does in the # header of lines.
 */
export function withColumnMenu(cols) {
  const ctx = columnContext();
  if (!ctx.baseline && !ctx.cost) return cols;
  const head = (label) => () => el('span', { class: 'col-head-menu' }, columnMenuButton(), el('span', { text: label }));
  return cols.map((c) => (c.key === 'code' ? { ...c, head: head(c.label) } : c));
}

/** The column menu: every optional column on the lines, Orig, Var and the cost columns elsewhere. */
function columnMenuItems() {
  const ctx = columnContext();
  const list = state.level === 'details' ? OPTIONAL_DETAIL_COLUMNS : [...BASELINE_COLUMNS, ...COST_COLUMNS];
  return list
    .filter((c) => (!c.baseline || ctx.baseline) && (!c.cost || ctx.cost))
    .map((c) => ({
      id: c.key,
      label: c.name || c.label,
      title: c.title,
      checked: columnShown(c, state.columns, ctx),
    }));
}

function columnsIcon() {
  const NS = 'http://www.w3.org/2000/svg';
  const svg = document.createElementNS(NS, 'svg');
  for (const [k, v] of Object.entries({ viewBox: '0 0 16 16', width: '14', height: '14', 'aria-hidden': 'true', focusable: 'false' })) {
    svg.setAttribute(k, v);
  }
  for (const x of [1.5, 6.5, 11.5]) {
    const r = document.createElementNS(NS, 'rect');
    for (const [k, v] of Object.entries({ x: String(x), y: '2', width: '3', height: '12', rx: '0.8' })) r.setAttribute(k, v);
    svg.append(r);
  }
  return svg;
}

function openColumnMenu(anchor) {
  openMenu(anchor, {
    owner: 'columns',
    label: t('main.columns'),
    checkable: true,
    items: columnMenuItems(),
    returnFocus: () => P().host.querySelector('.col-chooser'),
    onPick: (item) => {
      const col = OPTIONAL_DETAIL_COLUMNS.find((c) => c.key === item.id) || COST_COLUMNS.find((c) => c.key === item.id);
      if (!col) return;
      state.columns = { ...state.columns, [col.key]: !columnShown(col, state.columns, columnContext()) };
      writePref('detailColumns', state.columns);
      renderAll();
      updateMenu(columnMenuItems());
    },
  });
}

/** The header button for the column menu (the grid draws it in the # column's header). */
function columnMenuButton() {
  const b = el(
    'button',
    {
      class: 'col-chooser',
      title: t('main.show_or_hide_columns'),
      attrs: { type: 'button', 'aria-label': t('main.choose_columns'), 'aria-haspopup': 'menu', 'aria-expanded': 'false' },
      data: { focusId: 'col-chooser' },
      on: {
        click: (e) => openColumnMenu(e.currentTarget),
        keydown: (e) => {
          if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
          e.preventDefault();
          e.stopPropagation();
          openColumnMenu(e.currentTarget);
        },
      },
    },
    columnsIcon(),
  );
  // The grid redraws while the menu is open (a column was toggled): follow it.
  if (isMenuOpen('columns')) setMenuAnchor('columns', b);
  return b;
}
