#!/usr/bin/env node
// A headless smoke test of the browser build (docs/research/web-session.md):
//
//   cargo xtask web && node scripts/web-smoke.mjs [target/web]
//
// Serves the built folder on 127.0.0.1, opens it in headless Chrome or
// Chromium with the demo's invented sample budget, Field Station (?open=), and checks that
// the topsheet draws with its totals, that an edit updates the footer's grand
// total, that the chart list and one chart are fetched only when asked for,
// that comments written while reviewing and not downloaded yet make closing
// the page and Stop Reviewing ask first, that New Sub-Budget from This
// Category (from the keyboard's context menu) opens the row alone with its
// total, and that the page logs no errors (a
// Content-Security-Policy refusal among them). It also reads the page the way
// a screen reader does (Chrome's accessibility tree, decision 82): every
// control has a name, the budget grid is an ARIA grid whose cursor cell says
// its column and row, cells say whether they are selected, the cursor cell's
// id stays the same across a redraw, an edit is announced politely, and a
// dialog is a named modal dialog that gives focus back when it closes.
//
// No npm packages: Node's own http server and Chrome's DevTools protocol over
// a pipe (scripts/chrome-pipe.mjs, which also finds Chrome; CHROME= names
// another). Node 18 or later. Exits 0 when every check passes.

import { createServer } from 'node:http';
import { existsSync, readFileSync, statSync } from 'node:fs';
import { dirname, extname, join, normalize, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

import { findChrome, launch } from './chrome-pipe.mjs';

const REPO = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const DIR = resolve(process.argv[2] || join(REPO, 'target', 'web'));
const SAMPLE = join(REPO, 'fixtures', 'field-station', 'field-station.mthd');
const SAMPLE_PATH = '/smoke-sample.mthd';
const TIMEOUT_MS = 90_000;

const TYPES = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json',
  '.wasm': 'application/wasm',
  '.mthd': 'application/json',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
};

function fail(message) {
  console.error(`web-smoke: ${message}`);
  process.exitCode = 1;
  throw new Error(message);
}

// ---------------------------------------------------------------------------
// The static server: the built folder, plus the sample budget

function serve() {
  const requests = [];
  const server = createServer((req, res) => {
    const path = decodeURIComponent(new URL(req.url, 'http://x').pathname);
    requests.push(path);
    if (/^\/favicon\.(ico|svg)$/.test(path)) {
      res.writeHead(204).end(); // the site's, outside the build
      return;
    }
    let file = path === SAMPLE_PATH ? SAMPLE : normalize(join(DIR, path));
    if (file !== SAMPLE && !(file + sep).startsWith(DIR + sep) && file !== DIR) {
      res.writeHead(403).end();
      return;
    }
    if (existsSync(file) && statSync(file).isDirectory()) file = join(file, 'index.html');
    if (!existsSync(file)) {
      res.writeHead(404).end();
      return;
    }
    res.writeHead(200, { 'content-type': TYPES[extname(file)] || 'application/octet-stream' });
    res.end(readFileSync(file));
  });
  return new Promise((ok) => server.listen(0, '127.0.0.1', () => ok({ server, requests, port: server.address().port })));
}

// ---------------------------------------------------------------------------
// The checks

async function main() {
  if (!existsSync(join(DIR, 'index.html')) || !existsSync(join(DIR, 'method-web', 'method_web_bg.wasm'))) {
    fail(`${DIR} isn't a web build: run \`cargo xtask web\` first`);
  }
  const chrome = findChrome();
  if (!chrome) fail('no Chrome or Chromium found; set CHROME=<path>');

  const { server, requests, port } = await serve();
  const browser = launch(chrome);
  const timer = setTimeout(() => {
    console.error(browser.stderr());
    fail(`timed out after ${TIMEOUT_MS / 1000} s`);
    process.exit(1);
  }, TIMEOUT_MS);
  const problems = [];
  try {
    const { targetId } = await browser.send('Target.createTarget', { url: 'about:blank' });
    const { sessionId } = await browser.send('Target.attachToTarget', { targetId, flatten: true });
    const send = (m, p) => browser.send(m, p, sessionId);
    browser.on((msg) => {
      if (msg.sessionId !== sessionId) return;
      if (msg.method === 'Runtime.exceptionThrown') {
        problems.push(`exception: ${msg.params.exceptionDetails.exception?.description || msg.params.exceptionDetails.text}`);
      } else if (msg.method === 'Runtime.consoleAPICalled' && msg.params.type === 'error') {
        problems.push(`console.error: ${msg.params.args.map((a) => a.value ?? a.description).join(' ')}`);
      } else if (msg.method === 'Log.entryAdded' && msg.params.entry.level === 'error') {
        problems.push(`log: ${msg.params.entry.text} ${msg.params.entry.url || ''}`);
      }
    });
    await send('Runtime.enable');
    await send('Log.enable');
    await send('Page.enable');
    await send('Accessibility.enable');
    await send('Emulation.setDeviceMetricsOverride', { width: 1280, height: 800, deviceScaleFactor: 1, mobile: false });

    const evaluate = async (expression) => {
      const r = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
      if (r.exceptionDetails) {
        const e = r.exceptionDetails.exception;
        throw new Error(e?.description || e?.value || r.exceptionDetails.text);
      }
      return r.result.value;
    };
    const until = async (what, expression, ms = 30_000) => {
      const end = Date.now() + ms;
      for (;;) {
        const v = await evaluate(expression).catch(() => null);
        if (v) return v;
        if (Date.now() > end) fail(`${what}: not seen within ${ms / 1000} s`);
        await new Promise((r) => setTimeout(r, 200));
      }
    };
    // The footer's grand total, in cents.
    const GRAND = `(() => {
      const s = [...document.querySelectorAll('#footer .stat')].find((e) => e.querySelector('.stat-label')?.textContent === 'Grand total');
      const t = s && s.querySelector('.stat-value').textContent.replace(/[^0-9.-]/g, '');
      return t ? Math.round(Number(t) * 100) : null;
    })()`;

    await send('Page.navigate', { url: `http://127.0.0.1:${port}/?open=${SAMPLE_PATH.slice(1)}` });

    // 1. The topsheet draws, with the sample's categories and totals.
    await until('the sample budget in the tab', `document.title.startsWith('smoke-sample.mthd')`);
    await until('the topsheet', `document.body.innerText.includes('Story and screenplay') && document.body.innerText.includes('Field Station')`);
    const before = await until('the grand total', `(${GRAND}) || null`);
    console.log(`web-smoke: the topsheet draws (grand total ${(before / 100).toFixed(2)})`);
    if (requests.some((p) => p.startsWith('/charts/'))) fail(`charts were fetched before anyone asked: ${requests.join(', ')}`);

    // 1b. What a screen reader is given (decision 82).
    const ax = async () => (await send('Accessibility.getFullAXTree')).nodes.filter((n) => !n.ignored);
    const prop = (n, name) => {
      const p = (n.properties || []).find((x) => x.name === name);
      return p ? p.value.value : undefined;
    };
    const named = (n) => String((n.name && n.name.value) || '').trim();
    const NAMED_ROLES = new Set(['button', 'checkbox', 'combobox', 'textbox', 'tab', 'link', 'menuitem', 'radio', 'switch', 'slider', 'spinbutton']);
    const unnamed = (nodes) => nodes.filter((n) => NAMED_ROLES.has(n.role && n.role.value) && !named(n)).map((n) => `${n.role.value} (node ${n.backendDOMNodeId})`);
    await evaluate(`document.getElementById('grid-host').focus()`);
    await new Promise((r) => setTimeout(r, 200));
    let nodes = await ax();
    const missing = unnamed(nodes);
    if (missing.length) fail(`controls without an accessible name: ${missing.join(', ')}`);
    for (const label of ['New', 'Open', 'Save', 'Undo', 'Setup', 'Tools', 'New tab']) {
      if (!nodes.some((n) => n.role.value === 'button' && named(n) === label)) fail(`no button named "${label}" in the accessibility tree`);
    }
    const gridNode = nodes.find((n) => n.role.value === 'grid');
    if (!gridNode || named(gridNode) !== 'Budget grid' || !prop(gridNode, 'focused')) fail('the budget grid is not a focused ARIA grid named "Budget grid"');
    const count = (role) => nodes.filter((n) => n.role.value === role).length;
    if (count('row') < 10 || count('columnheader') < 3 || count('gridcell') < 20) {
      fail(`the grid's rows and cells are missing (rows ${count('row')}, column headers ${count('columnheader')}, cells ${count('gridcell')})`);
    }
    if (!nodes.some((n) => n.role.value === 'gridcell' && prop(n, 'readonly') === true)) fail('no computed cell is marked read-only');
    // The cursor cell, found the way a screen reader finds it: through the
    // grid's aria-activedescendant.
    const activeId = async () => evaluate(`document.getElementById('grid-host').getAttribute('aria-activedescendant')`);
    const axOf = async (id) => {
      const { result } = await send('Runtime.evaluate', { expression: `document.getElementById(${JSON.stringify(id)})` });
      if (!result.objectId) return null;
      const { node } = await send('DOM.describeNode', { objectId: result.objectId });
      return nodes.find((n) => n.backendDOMNodeId === node.backendNodeId) || null;
    };
    const firstId = await activeId();
    const active = firstId && (await axOf(firstId));
    if (!active || !/Acct #, Story and screenplay$/.test(named(active))) fail(`the cursor cell's name lacks its column and row: "${active ? named(active) : 'none'}"`);
    // In a multi-select grid every selectable cell says whether it is
    // selected; the cursor on its own is not a selection.
    if (prop(active, 'selected') !== false) fail(`the lone cursor cell reports selected=${prop(active, 'selected')}, not false`);
    if (!nodes.some((n) => n.role.value === 'gridcell' && prop(n, 'selected') === false && n !== active)) fail('no unselected cell reports not-selected');
    const unmarked = await evaluate(`document.querySelectorAll('#grid-host tr[role="row"]:not(.static) > [role="gridcell"]:not([aria-selected]), #grid-host tr[role="row"]:not(.static) > [role="rowheader"]:not([aria-selected])').length`);
    if (unmarked) fail(`${unmarked} selectable cells have no aria-selected`);
    const staticMarked = await evaluate(`document.querySelectorAll('#grid-host tr.static > [aria-selected]').length`);
    if (staticMarked) fail(`${staticMarked} cells of rows that can't be selected carry aria-selected`);
    // Each cell has its own id, from its row and column, not its drawn place.
    if (/-active$/.test(firstId) || !/--code$/.test(firstId)) fail(`the cursor cell's id isn't its own: ${firstId}`);
    const clash = await evaluate(`import('./js/grid.js').then(({ Grid }) => {
      const id = (r, c) => Grid.prototype.cellId.call({ uid: 'g' }, r, c);
      return [id('a-', 'b'), id('a', '-b'), id('a_2d_', 'b'), id('a--b', 'c'), id('a', 'b--c')];
    })`);
    if (new Set(clash).size !== clash.length) fail(`different cells share an id: ${clash.join(', ')}`);
    const key = (k) => `document.getElementById('grid-host').dispatchEvent(new KeyboardEvent('keydown', { key: '${k}', bubbles: true, cancelable: true }))`;
    await evaluate(key('ArrowDown'));
    const downId = await activeId();
    await evaluate(key('ArrowUp'));
    if (downId === firstId || (await activeId()) !== firstId) fail(`moving the cursor doesn't change the active descendant to the next cell and back (${firstId}, ${downId})`);
    const places = await evaluate(`(() => {
      const host = document.getElementById('grid-host');
      const id = host.getAttribute('aria-activedescendant');
      const td = id && document.getElementById(id);
      return { rowcount: host.getAttribute('aria-rowcount'), row: td && td.parentElement.getAttribute('aria-rowindex'), col: td && td.getAttribute('aria-colindex') };
    })()`);
    if (!places.row || places.col !== '1' || !(Number(places.rowcount) > 10)) fail(`the cursor cell's place isn't given: ${JSON.stringify(places)}`);
    console.log(`web-smoke: the screen reader's view: named controls, a grid of ${count('row')} rows, the cursor on "${named(active)}", unselected cells marked`);


    // 2. An edit (a fringe's rate, as Budget Setup sends it) updates the
    // page's grand total to the engine's.
    const after = await evaluate(`(async () => {
      const web = window.__METHOD_WEB__;
      const v = await web.invoke('smoke', 'get_view', {});
      const f = v.budget.fringes.FICA;
      const fringe = { kind: f.kind, description: f.description || '', rate: '10', cutoff: f.cutoff == null ? '' : String(f.cutoff / 100) };
      const r = await web.invoke('smoke', 'set_fringe', { name: 'FICA', fringe });
      return r.report.grand_total;
    })()`);
    if (!(after > before)) fail(`raising FICA didn't raise the grand total (${before} -> ${after})`);
    await until('the edited grand total on the page', `(${GRAND}) === ${after}`);
    console.log(`web-smoke: an edit updates the total (grand total ${(after / 100).toFixed(2)})`);
    // ...and a screen reader hears the new net total, politely.
    const said = await until('the edit announced', `(() => { const a = document.getElementById('announce'); return a && /^Net total .+, up .+/.test(a.textContent) && a.getAttribute('aria-live') === 'polite' ? a.textContent : null; })()`, 10_000);
    console.log(`web-smoke: the edit is announced ("${said}")`);

    // 2a. The cursor cell keeps its id when its row is drawn again: rename
    // the first category (the cursor's row) and the new cell has the same id.
    const keptId = await activeId();
    await evaluate(`(window.__smokeCell = document.getElementById(${JSON.stringify(keptId)}), true)`);
    await evaluate(`window.__METHOD_WEB__.invoke('smoke', 'update_topsheet_row', { index: 0, code: null, name: 'Story and screenplay, revised' })`);
    const redrawn = await until('the cursor row redrawn', `(() => {
      const td = document.getElementById(${JSON.stringify(keptId)});
      return td && td !== window.__smokeCell && td.parentElement.textContent.includes('Story and screenplay, revised')
        ? { cursor: td.classList.contains('cursor'), id: document.getElementById('grid-host').getAttribute('aria-activedescendant'), name: td.getAttribute('aria-label') }
        : null;
    })()`, 10_000);
    if (!redrawn.cursor || redrawn.id !== keptId || !/Story and screenplay, revised$/.test(redrawn.name || '')) {
      fail(`after a redraw the active descendant is ${redrawn.id} ("${redrawn.name}"), not ${keptId}`);
    }
    console.log(`web-smoke: the cursor cell keeps its id (${keptId}) across a redraw`);

    // 2b. A dialog (Rename budget, on the topsheet): a named modal dialog
    // with focus inside and the page behind inert; Escape closes it and
    // focus goes back to the button that opened it.
    await evaluate(`(() => {
      const b = [...document.querySelectorAll('#actions button')].find((x) => x.textContent.trim() === 'Rename budget');
      b.focus();
      b.click();
      return true;
    })()`);
    await until('the Rename budget dialog', `Boolean(document.querySelector('#modal-root [role="dialog"]'))`, 5_000);
    nodes = await ax();
    const dialog = nodes.find((n) => n.role.value === 'dialog');
    if (!dialog || !named(dialog) || prop(dialog, 'modal') !== true) fail(`the dialog isn't a named modal dialog: ${dialog ? `"${named(dialog)}" modal=${prop(dialog, 'modal')}` : 'none'}`);
    const trapped = await evaluate(`document.querySelector('#modal-root').contains(document.activeElement) && document.querySelector('.app').inert`);
    if (!trapped) fail('focus is not held in the dialog');
    await evaluate(`document.activeElement.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }))`);
    await until('focus back on Rename budget', `!document.querySelector('#modal-root [role="dialog"]') && document.activeElement.textContent.trim() === 'Rename budget'`, 5_000);
    console.log(`web-smoke: the "${named(dialog)}" dialog is modal, holds focus and gives it back`);

    // 3. The charts of accounts: the list, then one chart, fetched when asked.
    const charts = await evaluate(`window.__METHOD_WEB__.invoke('smoke', 'chart_templates', {})`);
    if (!Array.isArray(charts) || charts.length < 5) fail(`the chart list has ${charts && charts.length} charts`);
    const first = charts[0].id;
    if (requests.some((p) => p.endsWith('.mthd') && p.startsWith('/charts/'))) fail('a chart was fetched before it was picked');
    const made = await evaluate(`window.__METHOD_WEB__.invoke('smoke', 'chart_template_new', { id: ${JSON.stringify(first)} })`);
    if (!made || made.budget.title !== 'Untitled' || !made.budget.topsheet.length) fail(`chart ${first} didn't open as a new budget`);
    if (!requests.includes(`/charts/${first}.mthd`)) fail(`chart ${first} wasn't fetched`);
    console.log(`web-smoke: ${charts.length} charts listed; ${first} fetched when picked and opened`);

    // 4. Review comments not downloaded yet: closing the page and Stop
    // Reviewing ask first (comments.js, tauri-shim.js; decisions.md, entry 81).
    await browser.send('Browser.setDownloadBehavior', { behavior: 'deny' });
    // Every budget saved, so only the review can make the page ask.
    await evaluate(`(async () => {
      const web = window.__METHOD_WEB__;
      for (const t of await web.invoke('smoke', 'list_tabs', {})) await web.invoke('smoke', 'save_mthd', { doc: t.id });
    })()`);
    // The sample in front again in the Host, as the page still shows it (the
    // chart opened above went to the front there).
    await evaluate(`window.__METHOD_WEB__.invoke('smoke', 'list_tabs', {}).then((tabs) => window.__METHOD_WEB__.invoke('smoke', 'switch_tab', { doc: tabs.find((t) => t.name === 'smoke-sample.mthd').id }))`);
    const asksToLeave = `(() => { const e = new Event('beforeunload', { cancelable: true }); window.dispatchEvent(e); return e.defaultPrevented; })()`;
    if (await evaluate(asksToLeave)) fail('the page asks before closing with nothing unsaved');
    await evaluate(`(async () => {
      document.getElementById('btn-file').click();
      await new Promise((r) => setTimeout(r, 100));
      const it = [...document.querySelectorAll('[role=menuitem]')].find((e) => e.textContent.includes('Open for Review'));
      it.click();
    })()`);
    await until('the review banner', `!document.getElementById('review-banner').hidden`);
    await until('the Comments tab', `!!document.querySelector('.comments-host textarea')`);
    await evaluate(`(() => {
      const a = document.querySelector('.comments-host textarea');
      a.value = 'Smoke comment';
      a.dispatchEvent(new Event('input'));
      [...document.querySelectorAll('.comments-host button')].find((b) => b.textContent.trim() === 'Add').click();
    })()`);
    await until('the comment listed', `document.querySelector('.comments-host').textContent.includes('Smoke comment')`);
    if (!(await evaluate(asksToLeave))) fail('closing the page with comments not downloaded doesn\'t ask');
    const clickBanner = (label) => evaluate(`[...document.querySelectorAll('#review-banner button')].find((b) => b.textContent === ${JSON.stringify(label)}).click()`);
    await clickBanner('Stop Reviewing');
    await until('the close question', `!!document.querySelector('.modal-backdrop .modal') && document.querySelector('.modal').textContent.includes('not downloaded')`);
    await evaluate(`[...document.querySelectorAll('.modal button')].find((b) => b.textContent === 'Cancel').click()`);
    await new Promise((r) => setTimeout(r, 200));
    if (await evaluate(`document.getElementById('review-banner').hidden`)) fail('Cancel closed the review anyway');
    await evaluate(`[...document.querySelectorAll('.comments-host button')].find((b) => b.textContent === 'Download Review File').click()`);
    await until('the review downloaded', `!document.querySelector('.comments-host').textContent.includes('not written yet')`);
    if (await evaluate(asksToLeave)) fail('the page still asks after the review was downloaded');
    await clickBanner('Stop Reviewing');
    await until('the review closed without a question', `document.getElementById('review-banner').hidden`);
    console.log('web-smoke: unsaved review comments make closing ask first');

    // 4b. New Sub-Budget from This Category, the keyboard's way: Shift+F10
    // on the first category opens the context menu; its item opens the
    // category as a budget of its own in a new tab, with the same total
    // (decisions.md, entry 91).
    const catTotal = await evaluate(`(async () => {
      const web = window.__METHOD_WEB__;
      const tabs = await web.invoke('smoke', 'list_tabs', {});
      const v = await web.invoke('smoke', 'get_view', { doc: tabs.find((t) => t.name === 'smoke-sample.mthd').id });
      return v.report.categories[0].total;
    })()`);
    await evaluate(`(() => {
      const h = document.getElementById('grid-host');
      h.focus();
      h.dispatchEvent(new KeyboardEvent('keydown', { key: 'F10', shiftKey: true, bubbles: true, cancelable: true }));
      return true;
    })()`);
    await until('the topsheet context menu', `[...document.querySelectorAll('[role=menuitem]')].some((e) => e.textContent.includes('New Sub-Budget from This Category'))`, 5_000);
    await evaluate(`[...document.querySelectorAll('[role=menuitem]')].find((e) => e.textContent.includes('New Sub-Budget from This Category')).click()`);
    const sub = await until('the sub-budget in front', `window.__METHOD_WEB__.invoke('smoke', 'get_view', {}).then((v) => v.budget.title.endsWith(' - Story and screenplay, revised') ? { title: v.budget.title, cats: v.report.categories.length, total: v.report.categories[0].total, charges: v.report.charges.length } : null)`, 10_000);
    if (sub.cats !== 1 || sub.total !== catTotal || sub.charges !== 0) fail(`the sub-budget from a row isn't the row alone with its total: ${JSON.stringify(sub)} (the row's total ${catTotal})`);
    console.log(`web-smoke: New Sub-Budget from This Category opens "${sub.title}" with the row's total (${(sub.total / 100).toFixed(2)})`);

    // 5. Nothing went wrong on the way.
    await new Promise((r) => setTimeout(r, 300));
    if (problems.length) fail(`the page reported problems:\n  ${problems.join('\n  ')}`);
    console.log('web-smoke: ok');
  } finally {
    clearTimeout(timer);
    browser.close();
    server.close();
  }
}

main().catch((e) => {
  if (!process.exitCode) console.error(`web-smoke: ${e.stack || e}`);
  process.exitCode = 1;
});
