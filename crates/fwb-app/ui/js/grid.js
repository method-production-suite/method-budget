// A keyboard-first grid used for all three levels (topsheet, accounts, details).
//
// The grid knows nothing about budgets. It draws rows and columns it is given,
// keeps a cursor cell and a row selection, and edits cells in place. Changes go
// out through handlers; the new data comes back through setData().
//
// Row spec:  { key, cls, selectable = true, activatable = false, editSkip = false,
//              color, cells: { [colKey]: { text, edit, cls, title, error, placeholder } } }
//   `edit` is the raw value to edit (a string); leave it undefined for
//   read-only cells. `editSkip` rows (separators) can be selected, but moving
//   on after an edit passes over them. `color` is a `#hex` text color.
// Column:    { key, label, cls, title, width, min, pin, head }
//   width/min: preferred and narrowest width in px. When every column has a
//   width, the grid sizes them itself: preferred widths when they fit, else
//   squeezed toward min before the grid scrolls sideways.
//   pin: 'left' or 'right' keeps the column in view while scrolling sideways.
//   grow: true takes spare room first (up to twice its width).
//   head: a function returning a node to show in the header instead of label.
//
// Handlers:  onCommit(rowKey, colKey, value)        one cell edited
//            onCommitMany(rowKeys, colKey, value)   same value into many rows
//            onClearCells(cells)                    Backspace/Delete on a range of
//                                                   editable cells [{ rowKey, colKey }]
//            onActivate(rowKey)                     Enter / double-click on an activatable row
//            onEscape()                             Escape with nothing to cancel
//            onSelect(rowKeys)                      cursor or selection changed
//            onTabOut(dir)                          Shift+Tab (dir -1) at the first cell;
//                                                   true if it moved focus elsewhere
//            enterMovesDown                         true: Enter moves down (details)
//
// Selection: a set of rows (Shift+Up/Down, Shift-click, Cmd/Ctrl-click) and a
// run of columns (Shift+Left/Right). The selected cells are those rows across
// those columns; copy, paste, fill down and clearing work on them.

import { el, hasMod } from './dom.js';
import { setRules } from './sheet.js';

const SAFE_KEY = /^[a-z0-9_]+$/i;
const HEX_COLOR = /^#(?:[0-9a-f]{3}|[0-9a-f]{6})$/i;
let gridCount = 0;

// ---------------------------------------------------------------------------
// Tab-separated text, the way spreadsheets put cells on the clipboard

/** Cells (an array of rows of strings) as TSV. Cells holding a tab, newline or quote are quoted. */
export function toTSV(matrix) {
  const cell = (v) => {
    const s = v === null || v === undefined ? '' : String(v);
    return /[\t\n\r"]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
  };
  return matrix.map((row) => row.map(cell).join('\t')).join('\n');
}

/** TSV text as rows of cells. Understands spreadsheet quoting and a trailing newline. */
export function parseTSV(text) {
  let s = String(text).replace(/\r\n?/g, '\n');
  if (s.endsWith('\n')) s = s.slice(0, -1);
  const rows = [];
  let row = [];
  let cell = '';
  let quoted = false;
  let fresh = true; // at the start of a cell
  for (let i = 0; i < s.length; i++) {
    const ch = s[i];
    if (quoted) {
      if (ch === '"' && s[i + 1] === '"') {
        cell += '"';
        i++;
      } else if (ch === '"') {
        quoted = false;
      } else {
        cell += ch;
      }
      continue;
    }
    if (ch === '"' && fresh) {
      quoted = true;
      fresh = false;
    } else if (ch === '\t') {
      row.push(cell);
      cell = '';
      fresh = true;
    } else if (ch === '\n') {
      row.push(cell);
      rows.push(row);
      row = [];
      cell = '';
      fresh = true;
    } else {
      cell += ch;
      fresh = false;
    }
  }
  row.push(cell);
  rows.push(row);
  return rows;
}

export class Grid {
  constructor(host, handlers) {
    this.host = host;
    this.h = handlers;
    this.columns = [];
    this.rows = [];
    this.nav = []; // indexes into rows of selectable rows
    this.cur = { row: 0, col: 0 }; // index into nav, index into columns
    this.selected = new Set(); // explicit multi-row selection (row keys)
    this.anchor = null; // nav index where a shift-selection started
    this.anchorCol = null; // column where a shift-selection started
    this.allCols = false; // select-all covers every column
    this.editor = null;
    this.rerendering = false;
    this.lastSelectSig = '';
    gridCount += 1;
    this.uid = `grid-${gridCount}`;
    this.fitSig = null;
    this.colorSig = '';

    this.table = el('table', { class: `grid ${this.uid}`, attrs: { role: 'grid' } });
    host.replaceChildren(this.table);
    host.tabIndex = 0;
    host.classList.add(`${this.uid}-host`);

    this.table.addEventListener('mousedown', (e) => this.onMouseDown(e));
    this.table.addEventListener('dblclick', (e) => this.onDoubleClick(e));
    if (typeof ResizeObserver === 'function') {
      let queued = false;
      new ResizeObserver(() => {
        if (queued) return;
        queued = true;
        requestAnimationFrame(() => {
          queued = false;
          this.fit();
        });
      }).observe(host);
    }
  }

  // -------------------------------------------------------------------------
  // Column sizes, pinned columns and row colors (constructed stylesheets; see sheet.js)

  /**
   * Sizes the columns to the space there is: preferred widths when they fit,
   * otherwise every column squeezed the same fraction of the way to its
   * minimum, and only then a sideways scroll (with pinned columns in view).
   */
  fit() {
    const cols = this.columns;
    const sized = cols.length > 0 && cols.every((c) => c.width > 0 && SAFE_KEY.test(c.key));
    if (!sized) {
      if (this.fitSig !== '') setRules(this.uid, '');
      this.fitSig = '';
      return;
    }
    const avail = this.host.clientWidth;
    if (!avail) return; // hidden; fits again when shown
    const want = cols.reduce((n, c) => n + c.width, 0);
    const room = cols.reduce((n, c) => n + (c.width - Math.min(c.min || c.width, c.width)), 0);
    const f = want > avail && room > 0 ? Math.min(1, (want - avail) / room) : 0;
    const widths = cols.map((c) => Math.floor(c.width - (c.width - Math.min(c.min || c.width, c.width)) * f));
    // Room to spare goes to `grow` columns first (up to twice their width);
    // the table's min-width spreads any rest over every column.
    let spare = avail - want;
    cols.forEach((c, i) => {
      if (!c.grow || spare <= 0) return;
      const add = Math.min(spare, c.width);
      widths[i] += add;
      spare -= add;
    });
    const sig = `${cols.map((c) => c.key).join(',')}:${widths.join(',')}`;
    if (sig === this.fitSig) return;
    this.fitSig = sig;
    // A fixed table width makes the browser keep these widths (an auto-width
    // table grows columns to fit their text); min-width: 100% in styles.css
    // still stretches it to fill a wider window.
    const sum = widths.reduce((n, w) => n + w, 0);
    const rules = [`table.${this.uid} { width: ${sum}px; }`];
    cols.forEach((c, i) => rules.push(`.${this.uid} col.c-${c.key} { width: ${widths[i]}px; }`));
    // Offsets for pinned columns: the widths of the pinned columns outside them.
    let left = 0;
    cols.forEach((c, i) => {
      if (c.pin !== 'left') return;
      rules.push(`.${this.uid} .pin-l.c-${c.key} { left: ${left}px; }`);
      left += widths[i];
    });
    let right = 0;
    for (let i = cols.length - 1; i >= 0; i--) {
      if (cols[i].pin !== 'right') continue;
      rules.push(`.${this.uid} .pin-r.c-${cols[i].key} { right: ${right}px; }`);
      right += widths[i];
    }
    // Keyboard moves scroll a cell clear of the pinned columns, not under them.
    rules.push(`.${this.uid}-host { scroll-padding-left: ${left}px; scroll-padding-right: ${right}px; }`);
    setRules(this.uid, rules.join('\n'));
  }

  /** One rule per text color in use: `tr.tc-2a6fdb` takes #2a6fdb. */
  paintColors() {
    const colors = new Set();
    for (const r of this.rows) if (r.color && HEX_COLOR.test(r.color)) colors.add(r.color.toLowerCase());
    const sig = [...colors].sort().join(',');
    if (sig === this.colorSig) return;
    this.colorSig = sig;
    const rules = [...colors].map((c) => `.${this.uid} tr.tc-${c.slice(1)} { color: ${c}; }`);
    setRules(`${this.uid}-colors`, rules.join('\n'));
  }

  /** Class names that pin a column, with an edge line on the side facing the scrolling columns. */
  pinClass(ci) {
    const c = this.columns[ci];
    if (c.pin === 'left') {
      const next = this.columns[ci + 1];
      return next && next.pin === 'left' ? 'pin pin-l' : 'pin pin-l pin-edge-l';
    }
    if (c.pin === 'right') {
      const prev = this.columns[ci - 1];
      return prev && prev.pin === 'right' ? 'pin pin-r' : 'pin pin-r pin-edge-r';
    }
    return '';
  }

  // -------------------------------------------------------------------------
  // Data

  /**
   * Replaces the rows. The cursor stays on the same row and column key when
   * they still exist; `focusKey` moves it to that row instead.
   */
  setData(columns, rows, { focusKey = null, focusCol = null, resetSelection = false } = {}) {
    const prevRow = this.cursorKey();
    const prevCol = this.columns[this.cur.col] && this.columns[this.cur.col].key;
    const prevIndex = this.cur.row;
    this.columns = columns;
    this.rows = rows;
    this.nav = [];
    rows.forEach((r, i) => {
      if (r.selectable !== false) this.nav.push(i);
    });

    const wantRow = focusKey !== null ? focusKey : prevRow;
    let idx = this.nav.findIndex((i) => this.rows[i].key === wantRow);
    if (idx < 0) idx = Math.min(prevIndex, this.nav.length - 1);
    this.cur.row = Math.max(0, idx);
    const wantCol = focusCol || prevCol;
    let col = columns.findIndex((c) => c.key === wantCol);
    if (col < 0) col = this.firstEditableCol();
    this.cur.col = Math.max(0, Math.min(col, columns.length - 1));

    const keys = new Set(this.nav.map((i) => this.rows[i].key));
    if (resetSelection || focusKey !== null) {
      this.selected.clear();
      this.anchorCol = null;
      this.allCols = false;
    }
    for (const k of [...this.selected]) if (!keys.has(k)) this.selected.delete(k);
    if (this.anchor !== null && this.anchor >= this.nav.length) this.anchor = null;
    if (this.anchorCol !== null && this.anchorCol >= columns.length) this.anchorCol = null;

    this.render();
  }

  firstEditableCol() {
    const r = this.rowAt(this.cur.row);
    if (!r) return 0;
    const i = this.columns.findIndex((c) => r.cells[c.key] && r.cells[c.key].edit !== undefined);
    return i < 0 ? 0 : i;
  }

  rowAt(navIndex) {
    const i = this.nav[navIndex];
    return i === undefined ? null : this.rows[i];
  }

  cursorKey() {
    const r = this.rowAt(this.cur.row);
    return r ? r.key : null;
  }

  cursorColKey() {
    const c = this.columns[this.cur.col];
    return c ? c.key : null;
  }

  /** Selected row keys, in display order; just the cursor row when nothing else is selected. */
  selectedKeys() {
    if (this.selected.size === 0) {
      const k = this.cursorKey();
      return k === null ? [] : [k];
    }
    return this.nav.map((i) => this.rows[i].key).filter((k) => this.selected.has(k));
  }

  /** The selected columns as [first, last] indexes. */
  colRange() {
    if (this.allCols) return [0, this.columns.length - 1];
    if (this.anchorCol === null) return [this.cur.col, this.cur.col];
    return [Math.min(this.anchorCol, this.cur.col), Math.max(this.anchorCol, this.cur.col)];
  }

  /** True when more than the cursor cell is selected. */
  hasRange() {
    const [a, b] = this.colRange();
    return this.selected.size > 1 || a !== b;
  }

  /** The selected cells: rows (display order) and columns (left to right). */
  cellRange() {
    const keys = new Set(this.selectedKeys());
    const rows = this.nav.map((i) => this.rows[i]).filter((r) => keys.has(r.key));
    const [a, b] = this.colRange();
    return { rows, cols: this.columns.slice(a, b + 1) };
  }

  /** What a cell copies as: its raw value when editable, else what it shows. */
  static cellValue(row, col) {
    const cell = row.cells[col.key] || {};
    if (cell.edit !== undefined) return String(cell.edit);
    return cell.text === undefined || cell.text === null ? '' : String(cell.text);
  }

  static editable(row, col) {
    const cell = row.cells[col.key];
    return Boolean(cell) && cell.edit !== undefined;
  }

  /** The selected cells as tab-separated text. */
  copyText() {
    const { rows, cols } = this.cellRange();
    return toTSV(rows.map((r) => cols.map((c) => Grid.cellValue(r, c))));
  }

  /** The selected cells that can be edited: [{ rowKey, colKey }]. */
  editableCells() {
    const { rows, cols } = this.cellRange();
    const out = [];
    for (const r of rows) for (const c of cols) if (Grid.editable(r, c)) out.push({ rowKey: r.key, colKey: c.key });
    return out;
  }

  /**
   * Where pasted cells go: from the top-left selected cell, down and right. A
   * single copied cell fills every selected cell. Read-only cells are skipped.
   * Returns { cells: [{ rowKey, colKey, value }], rowsShort } where rowsShort
   * counts pasted rows that ran past the last row.
   */
  pasteTargets(matrix) {
    const { rows, cols } = this.cellRange();
    const cells = [];
    if (!rows.length || !cols.length || !matrix.length) return { cells, rowsShort: 0 };
    if (matrix.length === 1 && matrix[0].length === 1) {
      for (const r of rows) for (const c of cols) if (Grid.editable(r, c)) cells.push({ rowKey: r.key, colKey: c.key, value: matrix[0][0] });
      return { cells, rowsShort: 0 };
    }
    const top = this.nav.findIndex((i) => this.rows[i].key === rows[0].key);
    const left = this.columns.indexOf(cols[0]);
    let rowsShort = 0;
    matrix.forEach((values, dr) => {
      const r = this.rowAt(top + dr);
      if (!r) {
        rowsShort += 1;
        return;
      }
      values.forEach((value, dc) => {
        const c = this.columns[left + dc];
        if (c && Grid.editable(r, c)) cells.push({ rowKey: r.key, colKey: c.key, value });
      });
    });
    return { cells, rowsShort };
  }

  /**
   * Fill down: in each selected column, the top selected row's value goes into
   * the selected rows below it. With one row selected, the row above is copied
   * into it. Returns [{ rowKey, colKey, value }] for editable cells that change.
   */
  fillDownTargets() {
    let { rows, cols } = this.cellRange();
    if (rows.length === 1) {
      const above = this.rowAt(this.nav.findIndex((i) => this.rows[i].key === rows[0].key) - 1);
      if (!above) return [];
      rows = [above, rows[0]];
    }
    const out = [];
    for (const c of cols) {
      const src = rows[0];
      if (!Grid.editable(src, c)) continue;
      const value = Grid.cellValue(src, c);
      for (const r of rows.slice(1)) {
        if (Grid.editable(r, c) && Grid.cellValue(r, c) !== value) out.push({ rowKey: r.key, colKey: c.key, value });
      }
    }
    return out;
  }

  // -------------------------------------------------------------------------
  // Drawing

  render() {
    const editing = this.editor;
    if (editing) this.rerendering = true;

    // A header control (the column menu button) keeps focus across redraws.
    const a = document.activeElement;
    const headFocus = a && a.closest && this.table.contains(a) && a.closest('thead') ? a.dataset.focusId || null : null;

    this.fit();
    this.paintColors();
    const pins = this.columns.map((c, ci) => this.pinClass(ci));

    const colgroup = el('colgroup');
    for (const c of this.columns) colgroup.append(el('col', { class: `c-${c.key}` }));

    const head = el('tr');
    this.columns.forEach((c, ci) => {
      const th = el('th', { class: `c-${c.key} ${c.cls || ''} ${pins[ci]}`, title: c.title, attrs: { scope: 'col' } });
      const node = typeof c.head === 'function' ? c.head() : null;
      if (node) th.append(node);
      else th.textContent = c.label;
      head.append(th);
    });
    const thead = el('thead', {}, head);

    const tbody = el('tbody');
    const curKey = this.cursorKey();
    const selKeys = new Set(this.selectedKeys());
    const ranged = this.hasRange();
    const [ca, cb] = this.colRange();
    this.rows.forEach((r, ri) => {
      const tr = el('tr', { class: r.cls || '', data: { row: ri } });
      if (r.color && HEX_COLOR.test(r.color)) tr.classList.add(`tc-${r.color.toLowerCase().slice(1)}`);
      if (r.selectable === false) tr.classList.add('static');
      if (r.selectable !== false && selKeys.has(r.key)) tr.classList.add('sel');
      if (r.activatable) tr.classList.add('drill');
      this.columns.forEach((c, ci) => {
        const cell = r.cells[c.key] || {};
        const td = el('td', {
          class: `c-${c.key} ${c.cls || ''} ${cell.cls || ''} ${pins[ci]}`,
          title: cell.error && cell.title ? `${cell.error}\n\n${cell.title}` : cell.error || cell.title,
          data: { col: ci },
        });
        if (cell.edit !== undefined) td.classList.add('editable');
        else td.classList.add('ro');
        if (cell.error) td.classList.add('has-error');
        const text = cell.text === undefined || cell.text === null ? '' : String(cell.text);
        if (text === '' && cell.placeholder) {
          td.append(el('span', { class: 'ph', text: cell.placeholder }));
        } else {
          td.textContent = text;
        }
        if (r.key === curKey && ci === this.cur.col && r.selectable !== false) td.classList.add('cursor');
        if (ranged && r.selectable !== false && selKeys.has(r.key) && ci >= ca && ci <= cb) td.classList.add('in-range');
        tr.append(td);
      });
      tbody.append(tr);
    });

    this.table.replaceChildren(colgroup, thead, tbody);
    if (headFocus) {
      const again = thead.querySelector(`[data-focus-id="${headFocus.replace(/[^a-z0-9_-]/gi, '')}"]`);
      if (again) again.focus();
    }

    if (editing) {
      const td = this.cellFor(editing.rowKey, editing.colKey);
      if (td) {
        td.classList.add('editing');
        td.append(editing.input);
        editing.td = td;
        const { selectionStart, selectionEnd } = editing;
        editing.input.focus();
        try {
          editing.input.setSelectionRange(selectionStart, selectionEnd);
        } catch {
          // Not all inputs support selection ranges.
        }
      } else {
        this.editor = null;
      }
      this.rerendering = false;
    }
    this.scrollToCursor();
    this.notifySelect();
  }

  cellFor(rowKey, colKey) {
    const ri = this.rows.findIndex((r) => r.key === rowKey);
    const ci = this.columns.findIndex((c) => c.key === colKey);
    if (ri < 0 || ci < 0) return null;
    const tr = this.table.tBodies[0] && this.table.tBodies[0].rows[ri];
    return tr ? tr.cells[ci] : null;
  }

  scrollToCursor() {
    const k = this.cursorKey();
    const c = this.cursorColKey();
    if (k === null || c === null) return;
    const td = this.cellFor(k, c);
    if (td) td.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  }

  /** Redraws just the cursor and selection marks (cheap; used while moving). */
  paintCursor() {
    const body = this.table.tBodies[0];
    if (!body) return;
    const curKey = this.cursorKey();
    const selKeys = new Set(this.selectedKeys());
    const ranged = this.hasRange();
    const [ca, cb] = this.colRange();
    for (const tr of body.rows) {
      const r = this.rows[Number(tr.dataset.row)];
      if (!r || r.selectable === false) continue;
      const sel = selKeys.has(r.key);
      tr.classList.toggle('sel', sel);
      for (const td of tr.cells) {
        const ci = Number(td.dataset.col);
        td.classList.toggle('cursor', r.key === curKey && ci === this.cur.col);
        td.classList.toggle('in-range', ranged && sel && ci >= ca && ci <= cb);
      }
    }
    this.scrollToCursor();
    this.notifySelect();
  }

  notifySelect() {
    const keys = this.selectedKeys();
    const sig = keys.join('|');
    if (sig === this.lastSelectSig) return;
    this.lastSelectSig = sig;
    if (this.h.onSelect) this.h.onSelect(keys);
  }

  focus() {
    if (this.editor) this.editor.input.focus();
    else this.host.focus({ preventScroll: true });
  }

  // -------------------------------------------------------------------------
  // Movement

  move(dRow, dCol, { extend = false, wrap = false } = {}) {
    if (!this.nav.length) return;
    let row = this.cur.row + dRow;
    let col = this.cur.col + dCol;
    if (wrap) {
      if (col >= this.columns.length) {
        if (row + 1 < this.nav.length) {
          row += 1;
          col = 0;
        } else col = this.columns.length - 1;
      } else if (col < 0) {
        if (row > 0) {
          row -= 1;
          col = this.columns.length - 1;
        } else col = 0;
      }
    }
    row = Math.max(0, Math.min(row, this.nav.length - 1));
    col = Math.max(0, Math.min(col, this.columns.length - 1));
    this.setCursor(row, col, { extend });
  }

  setCursor(row, col, { extend = false, toggle = false } = {}) {
    if (extend) {
      if (this.anchor === null) this.anchor = this.cur.row;
      if (this.anchorCol === null) this.anchorCol = this.allCols ? 0 : this.cur.col;
      this.allCols = false;
      this.cur = { row, col };
      const [a, b] = [Math.min(this.anchor, row), Math.max(this.anchor, row)];
      this.selected = new Set(this.nav.slice(a, b + 1).map((i) => this.rows[i].key));
    } else if (toggle) {
      if (this.selected.size === 0) {
        const k = this.cursorKey();
        if (k !== null) this.selected.add(k);
      }
      this.cur = { row, col };
      const k = this.cursorKey();
      if (this.selected.has(k)) this.selected.delete(k);
      else this.selected.add(k);
      this.anchor = row;
      this.anchorCol = null;
      this.allCols = false;
    } else {
      this.cur = { row, col };
      this.selected.clear();
      this.anchor = null;
      this.anchorCol = null;
      this.allCols = false;
    }
    this.paintCursor();
  }

  /** Moves the cursor to a row (and optionally a column) by key. */
  goTo(rowKey, colKey = null) {
    const idx = this.nav.findIndex((i) => this.rows[i].key === rowKey);
    if (idx < 0) return;
    let col = this.cur.col;
    if (colKey) {
      const c = this.columns.findIndex((x) => x.key === colKey);
      if (c >= 0) col = c;
    }
    this.setCursor(idx, col);
  }

  /** Selects every row, across every column. */
  selectAll() {
    this.selected = new Set(this.nav.map((i) => this.rows[i].key));
    this.anchor = 0;
    this.anchorCol = null;
    this.allCols = true;
    this.paintCursor();
  }

  /** Clears a multi-cell selection, keeping the cursor. False if there was none. */
  clearRange() {
    if (!this.hasRange() && this.selected.size === 0) return false;
    const had = this.hasRange();
    this.selected.clear();
    this.anchor = null;
    this.anchorCol = null;
    this.allCols = false;
    this.paintCursor();
    return had;
  }

  /** The cursor row's spec, or null. */
  cursorRow() {
    return this.rowAt(this.cur.row);
  }

  // -------------------------------------------------------------------------
  // Editing

  cursorCell() {
    const r = this.rowAt(this.cur.row);
    const c = this.columns[this.cur.col];
    if (!r || !c) return null;
    return { row: r, col: c, cell: r.cells[c.key] || {} };
  }

  /** Starts editing the cursor cell. `text` replaces the value (type-to-edit); null keeps it. */
  startEdit(text = null) {
    const at = this.cursorCell();
    if (!at || at.cell.edit === undefined) return false;
    const td = this.cellFor(at.row.key, at.col.key);
    if (!td) return false;
    const original = String(at.cell.edit);
    const input = el('input', {
      class: 'cell-editor',
      attrs: { type: 'text', spellcheck: 'false', autocomplete: 'off', autocapitalize: 'off', 'aria-label': at.col.label },
    });
    input.value = text === null ? original : text;
    this.editor = {
      input,
      td,
      rowKey: at.row.key,
      colKey: at.col.key,
      original,
      typed: text !== null, // started by typing: arrow keys move on
      selectionStart: input.value.length,
      selectionEnd: input.value.length,
    };
    input.addEventListener('blur', () => {
      if (this.rerendering || !this.editor || this.editor.input !== input) return;
      this.commitEdit(null);
    });
    input.addEventListener('input', () => {
      if (this.editor) {
        this.editor.selectionStart = input.selectionStart;
        this.editor.selectionEnd = input.selectionEnd;
      }
    });
    input.addEventListener('keyup', () => {
      if (this.editor) {
        this.editor.selectionStart = input.selectionStart;
        this.editor.selectionEnd = input.selectionEnd;
      }
    });
    td.classList.add('editing');
    td.append(input);
    input.focus();
    if (text === null) input.select();
    else input.setSelectionRange(input.value.length, input.value.length);
    return true;
  }

  /** Ends the edit, sending the value if it changed, then moves the cursor. */
  commitEdit(dir, { many = false } = {}) {
    const ed = this.editor;
    if (!ed) return;
    this.editor = null;
    const value = ed.input.value;
    ed.input.remove();
    if (ed.td) ed.td.classList.remove('editing');
    const keys = this.selectedKeys();
    if (many && keys.length > 1 && this.h.onCommitMany) {
      this.h.onCommitMany(keys, ed.colKey, value);
    } else if (value !== ed.original && this.h.onCommit) {
      this.h.onCommit(ed.rowKey, ed.colKey, value);
    }
    this.afterEdit(dir);
  }

  cancelEdit() {
    const ed = this.editor;
    if (!ed) return;
    this.editor = null;
    ed.input.remove();
    if (ed.td) ed.td.classList.remove('editing');
    this.host.focus({ preventScroll: true });
  }

  /** The next row from `from` in direction `d` that editing can land on, or null. */
  nextEditRow(from, d) {
    for (let i = from + d; i >= 0 && i < this.nav.length; i += d) {
      if (!this.rowAt(i).editSkip) return i;
    }
    return null;
  }

  afterEdit(dir) {
    this.host.focus({ preventScroll: true });
    if (dir === 'down' || dir === 'up') {
      const next = this.nextEditRow(this.cur.row, dir === 'down' ? 1 : -1);
      if (next !== null) this.setCursor(next, this.cur.col);
      else if (dir === 'down' && this.h.onPastEnd) this.h.onPastEnd();
    } else if (dir === 'right' || dir === 'left') {
      const d = dir === 'right' ? 1 : -1;
      const before = this.cur.row;
      this.move(0, d, { wrap: true });
      const r = this.cursorRow();
      if (this.cur.row !== before && r && r.editSkip) {
        const next = this.nextEditRow(this.cur.row, d);
        if (next !== null) this.setCursor(next, this.cur.col);
      }
    }
  }

  // -------------------------------------------------------------------------
  // Keyboard and mouse

  /** Handles a key while focus is in the grid. Returns true if it did something. */
  handleKey(e) {
    if (this.editor) return this.handleEditorKey(e);
    const mod = hasMod(e);
    switch (e.key) {
      // Cmd/Ctrl+Up and Down change level (main.js), so they aren't ours.
      case 'ArrowUp':
        if (mod) return false;
        this.move(-1, 0, { extend: e.shiftKey });
        return true;
      case 'ArrowDown':
        if (mod) return false;
        this.move(1, 0, { extend: e.shiftKey });
        return true;
      case 'ArrowLeft':
        if (mod) return false;
        this.move(0, -1, { extend: e.shiftKey });
        return true;
      case 'ArrowRight':
        if (mod) return false;
        this.move(0, 1, { extend: e.shiftKey });
        return true;
      case 'Home':
        if (mod) this.setCursor(0, this.cur.col, { extend: e.shiftKey });
        else this.setCursor(this.cur.row, 0, { extend: e.shiftKey });
        return true;
      case 'End':
        if (mod) this.setCursor(this.nav.length - 1, this.cur.col, { extend: e.shiftKey });
        else this.setCursor(this.cur.row, this.columns.length - 1, { extend: e.shiftKey });
        return true;
      case 'Tab':
        if (e.shiftKey && this.cur.row === 0 && this.cur.col === 0 && this.h.onTabOut && this.h.onTabOut(-1)) return true;
        this.move(0, e.shiftKey ? -1 : 1, { wrap: true });
        return true;
      case 'Enter': {
        if (mod) return false;
        const r = this.rowAt(this.cur.row);
        if (!this.h.enterMovesDown && r && r.activatable && this.h.onActivate) {
          this.h.onActivate(r.key);
        } else {
          this.move(e.shiftKey ? -1 : 1, 0);
        }
        return true;
      }
      case 'F2':
        this.startEdit(null);
        return true;
      case 'Escape':
        if (!this.clearRange() && this.h.onEscape) this.h.onEscape();
        return true;
      case 'Delete':
      case 'Backspace': {
        if (mod || e.altKey) return false;
        if (this.hasRange() && this.h.onClearCells) {
          const cells = this.editableCells();
          if (cells.length) this.h.onClearCells(cells);
          return true;
        }
        const at = this.cursorCell();
        if (at && at.cell.edit !== undefined && at.cell.edit !== '' && this.h.onCommit) {
          this.h.onCommit(at.row.key, at.col.key, '');
        }
        return true;
      }
      default:
        break;
    }
    if (e.key.length === 1 && !e.metaKey && !e.ctrlKey) {
      return this.startEdit(e.key);
    }
    return false;
  }

  handleEditorKey(e) {
    const ed = this.editor;
    const mod = hasMod(e);
    switch (e.key) {
      case 'Enter':
        if (mod) this.commitEdit(null, { many: true });
        else this.commitEdit(this.h.enterMovesDown ? (e.shiftKey ? 'up' : 'down') : null);
        return true;
      case 'Tab':
        this.commitEdit(e.shiftKey ? 'left' : 'right');
        return true;
      case 'Escape':
        this.cancelEdit();
        return true;
      case 'ArrowUp':
        if (mod) return false;
        this.commitEdit('up');
        return true;
      case 'ArrowDown':
        if (mod) return false;
        this.commitEdit('down');
        return true;
      case 'ArrowLeft':
        if (ed.typed && ed.input.selectionStart === 0 && ed.input.selectionEnd === 0) {
          this.commitEdit('left');
          return true;
        }
        return false;
      case 'ArrowRight':
        if (ed.typed && ed.input.selectionStart === ed.input.value.length) {
          this.commitEdit('right');
          return true;
        }
        return false;
      default:
        return false;
    }
  }

  hit(e) {
    const td = e.target.closest('td');
    if (!td || !this.table.contains(td)) return null;
    const tr = td.parentElement;
    const ri = Number(tr.dataset.row);
    const row = this.rows[ri];
    if (!row || row.selectable === false) return null;
    return { nav: this.nav.indexOf(ri), col: Number(td.dataset.col), row };
  }

  onMouseDown(e) {
    if (e.button !== 0) return;
    if (this.editor && e.target === this.editor.input) return;
    const h = this.hit(e);
    if (!h) return;
    e.preventDefault(); // keep focus handling ours; no text selection drag
    if (this.editor) this.commitEdit(null);
    this.host.focus({ preventScroll: true });
    this.setCursor(h.nav, h.col, { extend: e.shiftKey, toggle: !e.shiftKey && hasMod(e) });
  }

  onDoubleClick(e) {
    const h = this.hit(e);
    if (!h || this.editor) return;
    if (h.row.activatable && this.h.onActivate) {
      this.h.onActivate(h.row.key);
      return;
    }
    this.startEdit(null);
  }
}
