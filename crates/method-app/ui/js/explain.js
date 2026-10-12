// The tools panel's Explain tab (roadmap 3.5; src/explain.rs): how a total is
// worked out, as a tree whose steps add up to it, in plain words.
//
// "Explain This Number…" (the grid's right-click menu, Edit menu, or its key
// on the cursor cell) names a total with explainTargetAt() and hands it to
// setExplainTarget(); the tab asks Rust for it (explain_number) and draws the
// tree. Each step shows its label, the working in words and what it comes to;
// "less" marks a step taken off (credits). A click on a step goes to its line,
// account or category (ctx.jumpTo); the arrow beside it opens or closes it.
// Deep steps of a big total arrive closed with nothing under them (`more`)
// and are fetched when opened. When the budget changes the tree is asked for
// again, keeping what was open.
//
// Keys on a step: Up and Down move, Right opens (or goes to the first step
// under it), Left closes (or goes to the step above it), Enter goes to its
// place, Space opens or closes it. Home and End go to the first and last.
//
// Budget text goes in with textContent only.

import { el } from './dom.js';
import { invoke } from './api.js';
import { money } from './format.js';
import { shortcutText } from './shortcuts.js';

const st = {
  host: null,
  ctx: null,
  doc: null,
  rev: null,
  target: null, // what the page names, sent to explain_number
  data: null, // ExplanationDto
  error: '',
  loading: false,
  seq: 0,
  open: new Set(['']), // paths of open steps ('' is the root, '0.2' its first child's third child)
  extra: new Map(), // path -> children fetched for a step sent with `more`
  pending: new Set(), // paths being fetched
  focus: null, // path to focus after a redraw
};

/**
 * Explains `target` (a TargetDto: `{ kind, ... }`) in budget `doc` the next
 * time the tab is drawn; focus goes to the tree's first step.
 */
export function setExplainTarget(target, doc = null) {
  if (!target || typeof target !== 'object' || typeof target.kind !== 'string') return;
  st.target = target;
  if (typeof doc === 'number') st.doc = doc;
  st.data = null;
  st.error = '';
  st.open = new Set(['']);
  st.extra = new Map();
  st.pending = new Set();
  st.rev = null; // fetch on the next draw
  st.focus = '';
}

/** Draws the Explain tab into `host`. */
export function renderExplain(host, view, ctx) {
  st.host = host;
  st.ctx = ctx;
  const doc = view && typeof view.doc === 'number' ? view.doc : null;
  if (doc !== st.doc && st.doc !== null && st.target) {
    // Another budget in front: what was explained belongs to the other one.
    st.target = null;
    st.data = null;
    st.error = '';
  }
  st.doc = doc;
  const fresh = view && view.rev !== st.rev;
  st.rev = view ? view.rev : null;
  draw();
  if (fresh && st.target) fetchRoot();
}

// ---------------------------------------------------------------------------
// Naming the total under a grid cell

/**
 * The total a grid cell shows, as explain_number takes it, or null. `level`
 * is 'topsheet', 'accounts' or 'details'; `row` the grid row spec; `colKey`
 * the column; `ix` the indexed view (levels.js indexView); `cat` the
 * category entry on screen ({ ci, ... }) and `ai` the account's place in it.
 */
export function explainTargetAt({ level, row, colKey, ix, cat = null, ai = null }) {
  if (!row || !ix || colKey === 'orig' || colKey === 'var') return null;
  const key = String(row.key || '');
  if (level === 'topsheet') {
    let m = /^t(\d+)$/.exec(key);
    if (m) {
      const ti = Number(m[1]);
      const c = ix.cats.find((x) => x.ti === ti);
      if (c) return { kind: 'category', ci: c.ci };
      const lv = ix.levels.find((x) => x.ti === ti);
      return lv ? { kind: 'level', index: lv.li } : null;
    }
    m = /^f(\d+)$/.exec(key);
    if (m) {
      const lv = ix.levels.find((x) => x.ti === Number(m[1]));
      return lv ? { kind: 'level_fringes', index: lv.li } : null;
    }
    m = /^s-(ch|cr)(\d+)$/.exec(key);
    if (m) return { kind: m[1] === 'ch' ? 'charge' : 'credit', index: Number(m[2]) };
    const fixed = { 's-atl': 'atl', 's-btl': 'btl', 's-atlbtl': 'atl_btl', 's-fr': 'budget_fringes', 's-grand': 'grand_total', 's-net': 'net_total' };
    return fixed[key] ? { kind: fixed[key] } : null;
  }
  if (!cat || typeof cat.ci !== 'number') return null;
  const ci = cat.ci;
  const fringeCol = colKey === 'fringe_total' || colKey === 'fringe_pct' || colKey === 'fringe_full_pct';
  if (level === 'accounts') {
    const m = /^a(\d+)$/.exec(key);
    if (m) return { kind: fringeCol ? 'account_fringes' : 'account', ci, ai: Number(m[1]) };
    if (key === 'fa') return { kind: 'category_fringes', ci };
    if (key === 'cat-total') return fringeCol ? { kind: 'category_fringes', ci } : { kind: 'category', ci };
    return null;
  }
  if (level !== 'details' || typeof ai !== 'number') return null;
  const m = /^r(\d+)$/.exec(key);
  if (m) {
    if (row.kind === 'separator') return null;
    const rowId = Number(m[1]);
    if (fringeCol && (row.kind || 'line') === 'line') return { kind: 'line_fringes', row_id: rowId };
    return { kind: 'line', row_id: rowId };
  }
  if (key === 'acct-total') return { kind: fringeCol ? 'account_fringes' : 'account_lines', ci, ai };
  if (key === 'acct-total-fr') return { kind: 'account', ci, ai };
  return null;
}

// ---------------------------------------------------------------------------
// Fetching

function errText(e) {
  return typeof e === 'string' ? e : e && e.message ? e.message : String(e);
}

function args(target) {
  return st.doc === null ? { target } : { doc: st.doc, target };
}

async function fetchRoot() {
  const seq = ++st.seq;
  const target = st.target;
  st.loading = true;
  st.extra = new Map();
  st.pending = new Set();
  draw();
  try {
    const data = await invoke('explain_number', args(target));
    if (seq !== st.seq) return;
    st.data = data && data.root ? data : null;
    st.error = '';
  } catch (e) {
    if (seq !== st.seq) return;
    st.data = null;
    st.error = errText(e);
  } finally {
    if (seq === st.seq) st.loading = false;
  }
  if (st.host && st.host.isConnected) draw();
}

/** Fetches the children of the step at `path` (sent with `more`). */
async function fetchMore(path, step) {
  if (st.pending.has(path) || !step.expand) return;
  st.pending.add(path);
  const seq = st.seq;
  try {
    const data = await invoke('explain_number', args(step.expand));
    if (seq !== st.seq) return;
    st.extra.set(path, data && data.root && Array.isArray(data.root.children) ? data.root.children : []);
  } catch (e) {
    if (seq !== st.seq) return;
    st.extra.set(path, [{ label: 'Could not explain this', text: errText(e), amount: null, role: 'note', minus: false, jump: null, expand: null, more: false, children: [] }]);
  } finally {
    st.pending.delete(path);
  }
  if (st.host && st.host.isConnected) draw();
}

/** A step's children: sent with it, fetched, or null while unknown. */
function kids(step, path) {
  if (!step.more) return Array.isArray(step.children) ? step.children : [];
  return st.extra.has(path) ? st.extra.get(path) : null;
}

function hasKids(step) {
  return step.more || (Array.isArray(step.children) && step.children.length > 0);
}

// ---------------------------------------------------------------------------
// Drawing

function draw() {
  const host = st.host;
  if (!host) return;
  const active = document.activeElement;
  const hadFocus = active && host.contains(active) && active.dataset && active.dataset.path !== undefined ? active.dataset.path : null;
  const box = el('div', { class: 'explain' });
  if (!st.target) {
    const key = shortcutText('edit.explain');
    box.append(
      el('p', {
        class: 'explain-empty',
        text: `Right-click a total and choose Explain This Number…${key ? `, or put the cursor on it and press ${key}` : ''}. Every step of how it is worked out shows here, and the steps add up to it exactly.`,
      }),
    );
    host.replaceChildren(box);
    return;
  }
  if (st.error) box.append(el('p', { class: 'setup-error', text: st.error, attrs: { role: 'alert' } }));
  const d = st.data;
  if (!d) {
    if (st.loading) box.append(el('p', { class: 'explain-empty', text: 'Working it out…' }));
    host.replaceChildren(box);
    return;
  }
  box.append(
    el(
      'div',
      { class: 'explain-head' },
      el('span', { class: 'explain-title', text: d.title }),
      el('span', { class: 'explain-total', text: money(d.reported) }),
    ),
  );
  if (d.partial) {
    box.append(el('p', { class: 'explain-partial', text: 'Some cells have problems; the totals, and this, count them as blank.' }));
  }
  const tree = el('ul', { class: 'explain-tree', attrs: { role: 'tree', 'aria-label': `How ${d.title} is worked out` } });
  tree.append(item(d.root, '', 1));
  box.append(tree);
  host.replaceChildren(box);

  const want = st.focus !== null ? st.focus : hadFocus;
  st.focus = null;
  const rows = [...host.querySelectorAll('.explain-row')];
  const target = rows.find((r) => r.dataset.path === want) || null;
  for (const r of rows) r.tabIndex = -1;
  const first = target || rows[0];
  if (first) first.tabIndex = 0;
  if (target && want !== null) target.focus({ preventScroll: false });
}

function amountText(step) {
  if (step.amount === null || step.amount === undefined) return '';
  return money(step.amount);
}

function item(step, path, level) {
  const open = st.open.has(path);
  const expandable = hasKids(step);
  const li = el('li', {
    class: `explain-step role-${step.role}${step.minus ? ' is-minus' : ''}`,
    attrs: { role: 'treeitem', 'aria-level': String(level), 'aria-expanded': expandable ? String(open) : null },
  });
  const toggle = el('span', {
    class: `explain-toggle${expandable ? (open ? ' is-open' : ' is-closed') : ''}`,
    attrs: { 'aria-hidden': 'true' },
    on: {
      click: (e) => {
        e.stopPropagation();
        if (expandable) setOpen(path, step, !open);
      },
    },
  });
  const label = `${step.minus ? 'less ' : ''}${step.label}`;
  const title = [label, step.text, amountText(step)].filter(Boolean).join(': ');
  const row = el(
    'div',
    {
      class: `explain-row${step.jump ? ' has-jump' : ''}`,
      title: step.jump ? `${title}\nClick to go there` : title,
      attrs: { tabindex: '-1' },
      data: { path, key: `explain:${path}` },
      on: {
        click: () => {
          if (step.jump) goTo(step);
          else if (expandable) setOpen(path, step, !open);
        },
        keydown: (e) => onKey(e, step, path, expandable, open),
      },
    },
    toggle,
    el('span', { class: 'explain-label', text: label }),
    step.text ? el('span', { class: 'explain-text', text: step.text }) : null,
    el('span', { class: `explain-amount${step.role === 'note' ? ' is-note' : ''}`, text: amountText(step) }),
  );
  li.append(row);
  if (expandable && open) {
    const children = kids(step, path);
    const group = el('ul', { class: 'explain-group', attrs: { role: 'group' } });
    if (children === null) {
      group.append(el('li', { class: 'explain-loading', text: 'Working it out…', attrs: { role: 'none' } }));
      fetchMore(path, step);
    } else {
      children.forEach((c, i) => group.append(item(c, path === '' ? String(i) : `${path}.${i}`, level + 1)));
    }
    li.append(group);
  }
  return li;
}

function setOpen(path, step, on) {
  if (on) st.open.add(path);
  else {
    // Closing a step closes what is under it too.
    for (const p of [...st.open]) if (p === path || p.startsWith(path === '' ? '' : `${path}.`)) st.open.delete(p);
    if (path === '') st.open.delete('');
  }
  st.focus = path;
  draw();
}

function goTo(step) {
  if (!step.jump || !st.ctx || typeof st.ctx.jumpTo !== 'function') return;
  st.ctx.jumpTo(step.jump);
}

function parentPath(path) {
  if (path === '') return null;
  const i = path.lastIndexOf('.');
  return i < 0 ? '' : path.slice(0, i);
}

function onKey(e, step, path, expandable, open) {
  if (e.altKey || e.metaKey || e.ctrlKey) return;
  const rows = [...st.host.querySelectorAll('.explain-row')];
  const i = rows.findIndex((r) => r.dataset.path === path);
  let to = null;
  switch (e.key) {
    case 'ArrowDown':
      to = rows[Math.min(rows.length - 1, i + 1)];
      break;
    case 'ArrowUp':
      to = rows[Math.max(0, i - 1)];
      break;
    case 'Home':
      to = rows[0];
      break;
    case 'End':
      to = rows[rows.length - 1];
      break;
    case 'ArrowRight':
      if (expandable && !open) {
        e.preventDefault();
        setOpen(path, step, true);
        return;
      }
      to = expandable ? rows[i + 1] : null;
      break;
    case 'ArrowLeft':
      if (expandable && open) {
        e.preventDefault();
        setOpen(path, step, false);
        return;
      }
      to = rows.find((r) => r.dataset.path === parentPath(path)) || null;
      break;
    case 'Enter':
      e.preventDefault();
      if (step.jump) goTo(step);
      else if (expandable) setOpen(path, step, !open);
      return;
    case ' ':
      e.preventDefault();
      if (expandable) setOpen(path, step, !open);
      return;
    default:
      return;
  }
  if (!to) return;
  e.preventDefault();
  e.stopPropagation();
  for (const r of rows) r.tabIndex = -1;
  to.tabIndex = 0;
  to.focus();
}
