// Edit > Move Up and Move Down in the budget window (a part of main.js).

import { t } from './i18n.js';
import { $ } from './dom.js';
import { findCategory } from './levels.js';
import { state, grid, call, toast } from './main.js';

// One move at a time, each built from the newest view (accounts and
// topsheet rows are named by place, which a move changes). A held key adds
// at most one press behind the one running, so moves never pile up.
let moveQueue = Promise.resolve();
let movesWaiting = 0;

export function moveSelected(by) {
  if (!state.view || movesWaiting > 1) return;
  movesWaiting += 1;
  moveQueue = moveQueue
    .then(() => moveOnce(by))
    .catch((e) => toast(e))
    .finally(() => {
      movesWaiting -= 1;
    });
}

async function moveOnce(by) {
  const keys = grid.selectedKeys();
  const cursor = grid.cursorKey();
  let cmd;
  let args;
  let keyFor;
  if (state.level === 'details') {
    const ids = keys.filter((k) => /^r\d+$/.test(k)).map((k) => Number(k.slice(1)));
    if (!ids.length) return;
    [cmd, args, keyFor] = ['move_rows', { rowIds: ids, by }, (id) => `r${id}`];
  } else if (state.level === 'accounts') {
    const entry = state.cat ? findCategory(state.ix, state.cat.code, state.cat.ti) : null;
    const accts = entry ? entry.cat.accounts || [] : [];
    const codes = keys
      .filter((k) => /^a\d+$/.test(k))
      .map((k) => accts[Number(k.slice(1))])
      .filter(Boolean)
      .map((a) => a.code);
    if (!codes.length) return;
    [cmd, args, keyFor] = ['move_accounts', { codes, by }, (i) => `a${i}`];
  } else {
    const indexes = keys.filter((k) => /^t\d+$/.test(k)).map((k) => Number(k.slice(1)));
    if (!indexes.length) return;
    [cmd, args, keyFor] = ['move_topsheet_rows', { indexes, by }, (i) => `t${i}`];
  }
  const v = await call(cmd, args);
  if (!v || !Array.isArray(v.moved)) return;
  // The moved rows stay selected and the cursor stays on its row: lines
  // keep their keys; accounts and topsheet rows are found by their order
  // among the selected ones, which a move keeps.
  const moved = v.moved.map(keyFor);
  let at = cursor;
  if (state.level !== 'details') {
    const i = keys.indexOf(cursor);
    at = moved[i >= 0 ? i : 0] || cursor;
  }
  grid.selectRows(moved, at);
}
