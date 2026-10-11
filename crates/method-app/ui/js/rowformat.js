// Text style for the selected detail lines: bold, italic, indent, text color
// and clearing it all (Format menu, its keys, and the grid's right-click menu).
// Each change is one set_row_style command (src/rowstyle.rs), so one undo step
// and one redraw. Nothing here touches the math.
//
// The palette: eight colors plus none, the same for line text and group
// colors. A color is stored as its `light` value (the file keeps one `#hex`);
// context.css shows the `dark` value instead in a dark window, so both read at
// 4.5:1 or better on the grid's backgrounds, selected rows included. A color
// typed into a file by hand still shows as stored.

// { grid, view(), level(), act(cmd, args) }
let ctx = null;

export function initRowFormat(context) {
  ctx = context;
}

/** The text colors, in menu order. `id` names the Format > Text Color item. */
export const PALETTE = [
  { id: 'red', label: 'Red', light: '#c03530', dark: '#ef8079' },
  { id: 'orange', label: 'Orange', light: '#a8551c', dark: '#f08a45' },
  { id: 'gold', label: 'Gold', light: '#836521', dark: '#d39b1f' },
  { id: 'green', label: 'Green', light: '#1f7a36', dark: '#3fbf62' },
  { id: 'teal', label: 'Teal', light: '#1e7673', dark: '#2fbfb8' },
  { id: 'blue', label: 'Blue', light: '#1d63c9', dark: '#6ca0e5' },
  { id: 'purple', label: 'Purple', light: '#7c3fd1', dark: '#b58de2' },
  { id: 'pink', label: 'Pink', light: '#b72e78', dark: '#e779b4' },
];

/** The palette entry for a stored color, or null (none, or not one of ours). */
export function paletteColor(hex) {
  if (typeof hex !== 'string') return null;
  const h = hex.toLowerCase();
  return PALETTE.find((c) => c.light === h) || null;
}

const MAX_INDENT = 4; // src/rowstyle.rs, method_core::MAX_INDENT

/** The selected rows of the account on screen, as file-format JSON, in order. */
export function selectedDetails() {
  if (!ctx || ctx.level() !== 'details') return [];
  const v = ctx.view();
  const details = v && v.account && Array.isArray(v.account.details) ? v.account.details : [];
  const byId = new Map(details.map((d) => [d.id, d]));
  return ctx.grid
    .selectedKeys()
    .filter((k) => String(k).startsWith('r'))
    .map((k) => byId.get(Number(k.slice(1))))
    .filter(Boolean);
}

/** 'all', 'some' or 'none' of `rows` pass `test`. */
function share(rows, test) {
  const n = rows.filter(test).length;
  return n === 0 ? 'none' : n === rows.length ? 'all' : 'some';
}

/**
 * What the selection looks like, for check marks in menus: bold and italic
 * ('all', 'some', 'none'), whether indenting can go further either way, and
 * the color every selected row has ('' for none, null when they differ).
 */
export function styleState(rows = selectedDetails()) {
  const colors = new Set(rows.map((d) => (typeof d.color === 'string' ? d.color.toLowerCase() : '')));
  const indent = (d) => (Number.isInteger(d.indent) ? d.indent : 0);
  return {
    count: rows.length,
    bold: share(rows, (d) => d.bold === true),
    italic: share(rows, (d) => d.italic === true),
    canIndent: rows.some((d) => indent(d) < MAX_INDENT),
    canOutdent: rows.some((d) => indent(d) > 0),
    color: colors.size === 1 ? [...colors][0] : null,
    styled: rows.some((d) => d.bold === true || d.italic === true || indent(d) > 0 || typeof d.color === 'string'),
  };
}

function style(rows, patch) {
  if (!rows.length) return null;
  return ctx.act('set_row_style', { rowIds: rows.map((d) => d.id), ...patch });
}

/** Bold or italic on every selected row, or off when every one has it already. */
export function toggleStyle(field) {
  const rows = selectedDetails();
  if (!rows.length) return null;
  const all = share(rows, (d) => d[field] === true) === 'all';
  return style(rows, { [field]: !all });
}

/** One indent step more (1) or less (-1) on each selected row. */
export function indent(by) {
  const rows = selectedDetails();
  const s = styleState(rows);
  if ((by > 0 && !s.canIndent) || (by < 0 && !s.canOutdent)) return null;
  return style(rows, { indentBy: by });
}

/** A palette color by id on every selected row; 'none' (or null) clears it. */
export function setColor(id) {
  const c = PALETTE.find((x) => x.id === id);
  return style(selectedDetails(), { color: c ? c.light : null });
}

/** Back to plain text: not bold or italic, no indent, no color of its own. */
export function clearFormatting() {
  const rows = selectedDetails();
  if (!styleState(rows).styled) return null;
  return style(rows, { bold: false, italic: false, indent: 0, color: null });
}
