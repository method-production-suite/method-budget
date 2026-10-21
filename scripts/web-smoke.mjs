#!/usr/bin/env node
// A headless smoke test of the browser build (docs/research/web-session.md):
//
//   cargo xtask web && node scripts/web-smoke.mjs [<build folder>/web]
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
// dialog is a named modal dialog that gives focus back when it closes. A bid
// layout is worked out by the engine and drawn into report pages. The page's
// words come from the English string table (decision 94): looked up with
// names and plural forms, falling back to English, the language picked from
// the system's.
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
const DIR = resolve(process.argv[2] || join(process.env.CARGO_TARGET_DIR || join(REPO, 'target'), 'web'));
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

    // 1a. The words come from the English table (decisions.md, entry 94):
    // the page loads it from its own files, looks keys up with names filled
    // in and plural forms picked by Intl.PluralRules, falls back to English
    // for a key a language lacks (with a warning), and picks the system's
    // language, English when there is no table for it.
    if (!requests.includes('/i18n/en.js')) fail(`the English table wasn't loaded from the page's files: ${requests.filter((p) => p.startsWith('/i18n') || p.endsWith('i18n.js')).join(', ') || 'no request'}`);
    const words = await evaluate(`import('./js/i18n.js').then((m) => {
      const warned = [];
      const tables = { en: { hi: 'Hello {name}', rows: { one: '{count} row', other: '{count} rows' }, only: 'English only' }, xx: { hi: 'Xello {name}', rows: { one: '{count} xrow', other: '{count} xrows' } } };
      const tx = m.translator(tables, 'xx', { warn: (w) => warned.push(w) });
      return {
        hi: tx('hi', { name: 'Field Station' }),
        one: tx('rows', { count: 1 }),
        many: tx('rows', { count: 3 }),
        fallback: tx('only'),
        again: tx('only'),
        missing: tx('nowhere.at_all'),
        warned: warned.length,
        picked: [m.pickLanguage(['pt-BR', 'en-GB']), m.pickLanguage(['xx-YY', 'en'], ['en', 'xx']), m.pickLanguage([]), m.pickLanguage(['fr'])],
        language: m.LANGUAGE,
        lang: document.documentElement.lang,
        grand: m.t('footer.grand_total'),
        selected: [m.t('footer.selected_rows', { count: 1 }), m.t('footer.selected_rows', { count: 3 })],
      };
    })`);
    const wantWords = {
      hi: 'Xello Field Station', one: '1 xrow', many: '3 xrows', fallback: 'English only', again: 'English only', missing: 'nowhere.at_all', warned: 2,
      picked: ['en', 'xx', 'en', 'en'], language: 'en', lang: 'en', grand: 'Grand total', selected: ['Selected 1 row', 'Selected 3 rows'],
    };
    if (JSON.stringify(words) !== JSON.stringify(wantWords)) fail(`the string table: ${JSON.stringify(words)}, not ${JSON.stringify(wantWords)}`);
    // The shortcut sheet is drawn from the table too: its title, the group
    // headings and the first command's label.
    const sheet = await evaluate(`import('./js/help.js').then((m) => {
      m.openHelp();
      const d = document.querySelector('.help');
      const r = { title: d.querySelector('.help-title').textContent, groups: [...d.querySelectorAll('h3')].slice(0, 3).map((h) => h.textContent), first: d.querySelector('th').textContent };
      m.closeHelp();
      return r;
    })`);
    const wantSheet = { title: 'Keyboard shortcuts', groups: ['File', 'Edit', 'Cells'], first: 'New budget, or open a recent one (the start window)' };
    if (JSON.stringify(sheet) !== JSON.stringify(wantSheet)) fail(`the shortcut sheet: ${JSON.stringify(sheet)}, not ${JSON.stringify(wantSheet)}`);
    console.log(`web-smoke: the words come from the English table ("${words.grand}", "${words.selected[1]}", "${sheet.title}")`);

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

    // 2c. A bid layout (decisions.md, entry 92): the engine works the bid
    // out (bid_sheet) and the report draws it into pages, reconciled: the
    // budget's net total plus the bid's own markups.
    const bid = await evaluate(`(async () => {
      const { renderReport, withBid } = await import('./js/report.js');
      const full = await window.__METHOD_WEB__.invoke('smoke', 'get_full_view', {});
      const spec = {
        sections: ['bid'],
        bid: {
          fields: [{ label: 'Client', value: 'Smoke Test Coffee' }],
          lines: [
            { kind: 'section', name: 'Story', picks: [{ kind: 'category', code: '10' }] },
            { kind: 'subtotal', name: 'Subtotal' },
            { kind: 'flat', name: 'Handling', amount: 100000 },
          ],
          show_accounts: true,
        },
      };
      const view = await withBid(full, spec);
      const box = document.createElement('div');
      document.body.append(box);
      const out = renderReport(box, view, spec);
      const text = box.textContent;
      box.remove();
      return { pages: out.pages, text, net: full.report.net_total, sheet: view.bid_sheet, error: view.bid_error };
    })()`);
    if (!bid.sheet) fail(`the bid wasn't worked out: ${bid.error}`);
    if (bid.sheet.total !== bid.net + 100000) fail(`the bid total ${bid.sheet.total} isn't the net total ${bid.net} plus 1,000.00`);
    const fmt = (c) => `${Math.trunc(c / 100).toLocaleString('en-US')}.${String(c % 100).padStart(2, '0')}`;
    for (const want of ['Smoke Test Coffee', 'Total Story', 'Unassigned', 'Handling', 'Bid total', fmt(bid.sheet.total)]) {
      if (!bid.text.includes(want)) fail(`the bid's pages don't show "${want}"`);
    }
    if (!(bid.pages >= 1)) fail('the bid laid out no pages');
    console.log(`web-smoke: a bid layout draws ${bid.pages} page(s), its total ${fmt(bid.sheet.total)} the net total plus its markups`);

    // 2d. The bid layout in the report setup: ticking Bid starts the
    // example layout, adding a section and naming its accounts redraws the
    // preview with the engine's figures.
    await evaluate(`(async () => {
      const { showReportSetup } = await import('./js/reportsetup.js');
      window.__smokeCloseSetup = await showReportSetup({ view: () => null, call: (c, a) => window.__METHOD_WEB__.invoke('smoke', c, a), toast: (m) => console.error(String(m)) });
      document.getElementById('rs-sec-bid').click();
      return true;
    })()`);
    await until('the example bid layout in the setup', `document.querySelectorAll('.rs-bid-line').length === 6 && document.getElementById('rs-bid-l0-name').value === 'Pre-production'`, 10_000);
    await evaluate(`(() => {
      const cats = document.getElementById('rs-bid-l0-cats');
      cats.value = '10, 11';
      cats.dispatchEvent(new Event('input'));
      return true;
    })()`);
    await until('the bid in the preview', `(() => { const t = document.querySelector('.rs-pages').textContent; return t.includes('Bid total') && t.includes('Pre-production') && !t.includes('takes category'); })()`, 10_000);
    await evaluate(`(window.__smokeCloseSetup(), true)`);
    console.log('web-smoke: the report setup edits a bid layout and previews it');

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

    // 4b2. The desktop's start window and New Budget form (start.js), drawn
    // here with invented answers in place of the desktop's commands: their
    // words come from the string table, plurals included.
    const start = await evaluate(`(async () => {
      const m = await import('./js/start.js');
      const answers = {
        start_list: {
          pinned: [{ title: 'Smoke Pinned', file: 'smoke.mthd', folder: '/smoke', net_total: 1000000, currency: 'USD', modified: Date.now(), pinned: true, openable: true, shared: true, people: [{ name: 'Bo', place: 'account 2101', at: new Date().toISOString() }], bars: [{ label: 'Cast', share: 500 }] }],
          recent: [],
          projects: [{ name: 'Smoke Project', folder: '/smoke', token: 1, items: [] }],
          fresh: true,
        },
        new_budget_charts: [{ id: 'feature', name: 'Smoke Chart', group: 'Feature', description: '', ours: true, accounts: 1, categories: 2 }],
        new_budget_setup: { library: { name: 'Smoke Library', items: 3 }, packs: [{ title: 'Smoke Pack', file: 'p.json', place: 'bundled', items: 1, version: '2', effective: '2026-01-01', token: 5 }] },
      };
      const ctx = { invoke: async (cmd) => (cmd in answers ? answers[cmd] : null), apply: () => {}, toast: (e) => console.error(String(e)), focus: () => {} };
      await m.showStart(ctx);
      const box = document.querySelector('.st-dialog');
      const home = box.textContent;
      [...box.querySelectorAll('button')].find((b) => b.textContent.startsWith('New Budget…')).click();
      await new Promise((r) => setTimeout(r, 200));
      const form = box.textContent;
      const attrs = [...box.querySelectorAll('[title],[aria-label],[placeholder]')].map((e) => [e.title, e.getAttribute('aria-label'), e.getAttribute('placeholder')].join(' ')).join(' ');
      m.closeStart();
      return { home, form, attrs };
    })()`);
    for (const want of ['Budgets', 'Pinned', 'Smoke Pinned', 'shared', 'Pin', 'Unpin', 'Projects', 'Bo in account 2101']) {
      if (!start.home.includes(want) && !start.attrs.includes(want)) fail(`the start window doesn't show "${want}"`);
    }
    for (const want of ['New Budget', 'Chart of accounts', 'Smoke Chart', '1 account', 'Smoke Chart: 1 account in 2 categories.', 'Everything in my setup library (Smoke Library)', '3 items', 'version 2, from 2026-01-01, 1 item, comes with the app', 'Create Budget', 'Production type']) {
      if (!start.form.includes(want)) fail(`New Budget doesn't show "${want}"`);
    }
    const startRaw = (start.home + start.form + start.attrs).match(/\\b(?:start|main|modal)\\.[a-z_.]+\\b/);
    if (startRaw) fail(`the start window shows a key instead of words: ${startRaw[0]}`);
    console.log('web-smoke: the start window and New Budget draw their words from the string table');

    // 4c. Budget Setup (a frame in the browser) draws every tab, its words
    // from the string table (decision 94): the tab names, and each list
    // tab's table with its column heads.
    await evaluate(`(document.getElementById('btn-setup').click(), true)`);
    await until('the Budget Setup frame', `(() => { const f = document.querySelector('.mw-setup-frame'); return Boolean(f && f.contentDocument && f.contentDocument.querySelectorAll('.sw-tab').length > 5); })()`, 15_000);
    const setupTabs = await evaluate(`(async () => {
      const d = document.querySelector('.mw-setup-frame').contentDocument;
      const names = [...d.querySelectorAll('.sw-tab')].map((b) => b.textContent);
      const drawn = [];
      for (const id of ['fringes', 'groups', 'globals', 'units', 'sets', 'locations', 'currencies', 'rates']) {
        d.getElementById('sw-tab-' + id).click();
        await new Promise((r) => setTimeout(r, 150));
        const panel = d.getElementById('sw-panel') || d.body;
        drawn.push(id + ':' + (panel.textContent.trim().length > 0));
        // A key with no words shows as the key itself.
        const raw = (d.body.innerText + [...d.querySelectorAll('[title],[aria-label],[placeholder]')].map((e) => [e.title, e.getAttribute('aria-label'), e.getAttribute('placeholder')].join(' ')).join(' ')).match(/\b(?:apply|setup|rates|main|find|start|footer|modal|help|menu|shortcut)\.[a-z_.]+\b/);
        if (raw) drawn.push(id + ':key ' + raw[0] + ':false');
      }
      return { names, drawn, heads: [...d.querySelectorAll('.sw-head')].map((h) => h.textContent).filter(Boolean) };
    })()`);
    const wantTabs = ['Fringes', 'Groups', 'Globals', 'Units', 'Sets', 'Locations', 'Currency', 'Charges', 'Credits', 'Rate Tables', 'Info', 'Settings', 'Baseline'];
    if (JSON.stringify(setupTabs.names) !== JSON.stringify(wantTabs)) fail(`Budget Setup's tabs: ${JSON.stringify(setupTabs.names)}`);
    if (setupTabs.drawn.some((x) => x.endsWith(':false'))) fail(`Budget Setup tabs drew nothing: ${setupTabs.drawn.join(', ')}`);
    console.log(`web-smoke: Budget Setup draws its ${setupTabs.names.length} tabs from the string table`);

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
