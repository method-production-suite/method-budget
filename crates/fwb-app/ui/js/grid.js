// A keyboard-first grid used for all three levels (topsheet, accounts, details).
//
// The grid knows nothing about budgets. It draws rows and columns it is given,
// keeps a cursor cell and a row selection, and edits cells in place. Changes go
// out through handlers; the new data comes back through setData().
//
// Row spec:  { key, cls, selectable = true, activatable = false,
//              cells: { [colKey]: { text, edit, cls, title, error, placeholder } } }
//   `edit` is the raw value to edit (a string); leave it undefined for
//   read-only cells.
// Column:    { key, label, cls, title }
//
// Handlers:  onCommit(rowKey, colKey, value)        one cell edited
//            onCommitMany(rowKeys, colKey, value)   same value into many rows
//            onActivate(rowKey)                     Enter / double-click on an activatable row
//            onEscape()                             Escape with nothing to cancel
//            onSelect(rowKeys)                      cursor or selection changed
//            enterMovesDown                         true: Enter moves down (details)

import { el, hasMod } from './dom.js';

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
    this.editor = null;
    this.rerendering = false;
    this.lastSelectSig = '';

    this.table = el('table', { class: 'grid', attrs: { role: 'grid' } });
    host.replaceChildren(this.table);
    host.tabIndex = 0;

    this.table.addEventListener('mousedown', (e) => this.onMouseDown(e));
    this.table.addEventListener('dblclick', (e) => this.onDoubleClick(e));
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
    if (resetSelection || focusKey !== null) this.selected.clear();
    for (const k of [...this.selected]) if (!keys.has(k)) this.selected.delete(k);
    if (this.anchor !== null && this.anchor >= this.nav.length) this.anchor = null;

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

  // -------------------------------------------------------------------------
  // Drawing

  render() {
    const editing = this.editor;
    if (editing) this.rerendering = true;

    const colgroup = el('colgroup');
    for (const c of this.columns) colgroup.append(el('col', { class: `c-${c.key}` }));

    const head = el('tr');
    for (const c of this.columns) {
      head.append(el('th', { class: `c-${c.key} ${c.cls || ''}`, text: c.label, title: c.title, attrs: { scope: 'col' } }));
    }
    const thead = el('thead', {}, head);

    const tbody = el('tbody');
    const curKey = this.cursorKey();
    const selKeys = new Set(this.selectedKeys());
    this.rows.forEach((r, ri) => {
      const tr = el('tr', { class: r.cls || '', data: { row: ri } });
      if (r.selectable === false) tr.classList.add('static');
      if (r.selectable !== false && selKeys.has(r.key)) tr.classList.add('sel');
      if (r.activatable) tr.classList.add('drill');
      this.columns.forEach((c, ci) => {
        const cell = r.cells[c.key] || {};
        const td = el('td', {
          class: `c-${c.key} ${c.cls || ''} ${cell.cls || ''}`,
          title: cell.error || cell.title,
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
        tr.append(td);
      });
      tbody.append(tr);
    });

    this.table.replaceChildren(colgroup, thead, tbody);

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
    for (const tr of body.rows) {
      const r = this.rows[Number(tr.dataset.row)];
      if (!r || r.selectable === false) continue;
      tr.classList.toggle('sel', selKeys.has(r.key));
      for (const td of tr.cells) {
        td.classList.toggle('cursor', r.key === curKey && Number(td.dataset.col) === this.cur.col);
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
    } else {
      this.cur = { row, col };
      this.selected.clear();
      this.anchor = null;
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

  selectAll() {
    this.selected = new Set(this.nav.map((i) => this.rows[i].key));
    this.anchor = 0;
    this.paintCursor();
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

  afterEdit(dir) {
    this.host.focus({ preventScroll: true });
    if (dir === 'down') {
      if (this.cur.row + 1 >= this.nav.length && this.h.onPastEnd) this.h.onPastEnd();
      else this.move(1, 0);
    } else if (dir === 'up') this.move(-1, 0);
    else if (dir === 'right') this.move(0, 1, { wrap: true });
    else if (dir === 'left') this.move(0, -1, { wrap: true });
  }

  // -------------------------------------------------------------------------
  // Keyboard and mouse

  /** Handles a key while focus is in the grid. Returns true if it did something. */
  handleKey(e) {
    if (this.editor) return this.handleEditorKey(e);
    const mod = hasMod(e);
    switch (e.key) {
      case 'ArrowUp':
        if (mod) this.setCursor(0, this.cur.col, { extend: e.shiftKey });
        else this.move(-1, 0, { extend: e.shiftKey });
        return true;
      case 'ArrowDown':
        if (mod) this.setCursor(this.nav.length - 1, this.cur.col, { extend: e.shiftKey });
        else this.move(1, 0, { extend: e.shiftKey });
        return true;
      case 'ArrowLeft':
        this.move(0, -1);
        return true;
      case 'ArrowRight':
        this.move(0, 1);
        return true;
      case 'Home':
        this.setCursor(this.cur.row, 0);
        return true;
      case 'End':
        this.setCursor(this.cur.row, this.columns.length - 1);
        return true;
      case 'Tab':
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
        if (this.selected.size > 1) {
          this.selected.clear();
          this.anchor = null;
          this.paintCursor();
        } else if (this.h.onEscape) this.h.onEscape();
        return true;
      case 'Delete':
      case 'Backspace': {
        if (mod) return false;
        const at = this.cursorCell();
        if (at && at.cell.edit !== undefined && at.cell.edit !== '' && this.h.onCommit) {
          this.h.onCommit(at.row.key, at.col.key, '');
        }
        return true;
      }
      default:
        break;
    }
    if (mod && !e.shiftKey && !e.altKey && e.key.toLowerCase() === 'a') {
      this.selectAll();
      return true;
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
        this.commitEdit('up');
        return true;
      case 'ArrowDown':
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
