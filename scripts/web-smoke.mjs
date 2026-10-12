#!/usr/bin/env node
// A headless smoke test of the browser build (docs/research/web-session.md):
//
//   cargo xtask web && node scripts/web-smoke.mjs [target/web]
//
// Serves the built folder on 127.0.0.1, opens it in headless Chrome or
// Chromium with the demo's invented sample budget (?open=), and checks that
// the topsheet draws with its totals, that an edit updates the footer's grand
// total, that the chart list and one chart are fetched only when asked for,
// and that the page logs no errors (a Content-Security-Policy refusal among
// them).
//
// No npm packages: Node's own http server and Chrome's DevTools protocol over
// a pipe (--remote-debugging-pipe: JSON messages ending in a NUL byte on file
// descriptors 3 and 4), so it needs no WebSocket client. Chrome is found on
// the PATH (google-chrome, chromium...) or in the usual Mac place; CHROME=
// names another. Node 18 or later. Exits 0 when every check passes.

import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { existsSync, mkdtempSync, readFileSync, rmSync, statSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, extname, join, normalize, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const REPO = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const DIR = resolve(process.argv[2] || join(REPO, 'target', 'web'));
const SAMPLE = join(REPO, 'crates', 'method-web', 'demo', 'lanterns-over-kettle-lake.mthd');
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
// Chrome over --remote-debugging-pipe

function findChrome() {
  if (process.env.CHROME) return process.env.CHROME;
  const names = ['google-chrome', 'google-chrome-stable', 'chromium', 'chromium-browser', 'chrome'];
  for (const dir of (process.env.PATH || '').split(':')) {
    for (const n of names) if (dir && existsSync(join(dir, n))) return join(dir, n);
  }
  for (const p of [
    '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
    '/Applications/Chromium.app/Contents/MacOS/Chromium',
  ]) {
    if (existsSync(p)) return p;
  }
  return null;
}

function launch(chrome) {
  const profile = mkdtempSync(join(tmpdir(), 'web-smoke-'));
  const proc = spawn(
    chrome,
    [
      '--headless=new',
      '--remote-debugging-pipe',
      `--user-data-dir=${profile}`,
      '--no-first-run',
      '--no-default-browser-check',
      '--disable-gpu',
      '--disable-extensions',
      '--disable-background-networking',
      '--disable-sync',
      // GitHub's Ubuntu runners don't allow Chrome's sandbox.
      ...(process.platform === 'linux' ? ['--no-sandbox'] : []),
      'about:blank',
    ],
    { stdio: ['ignore', 'ignore', 'pipe', 'pipe', 'pipe'] },
  );
  let stderr = '';
  proc.stderr.on('data', (d) => (stderr += d));
  const out = proc.stdio[3];
  const inp = proc.stdio[4];
  let nextId = 1;
  const waiting = new Map();
  const handlers = [];
  let buf = Buffer.alloc(0);
  inp.on('data', (chunk) => {
    buf = Buffer.concat([buf, chunk]);
    let end;
    while ((end = buf.indexOf(0)) !== -1) {
      const msg = JSON.parse(buf.subarray(0, end).toString('utf8'));
      buf = buf.subarray(end + 1);
      if (msg.id && waiting.has(msg.id)) {
        const { ok, no } = waiting.get(msg.id);
        waiting.delete(msg.id);
        if (msg.error) no(new Error(`${msg.error.message} (${msg.error.code})`));
        else ok(msg.result);
      } else if (msg.method) {
        for (const h of handlers) h(msg);
      }
    }
  });
  const send = (method, params = {}, sessionId) =>
    new Promise((ok, no) => {
      const id = nextId++;
      waiting.set(id, { ok, no });
      out.write(`${JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) })}\0`);
    });
  const close = () => {
    try {
      out.write(`${JSON.stringify({ id: nextId++, method: 'Browser.close' })}\0`);
    } catch (_) {}
    setTimeout(() => {
      proc.kill();
      rmSync(profile, { recursive: true, force: true });
    }, 500);
  };
  return { send, on: (h) => handlers.push(h), close, proc, stderr: () => stderr };
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
    await until('the topsheet', `document.body.innerText.includes('Story and rights') && document.body.innerText.includes('Lanterns Over Kettle Lake')`);
    const before = await until('the grand total', `(${GRAND}) || null`);
    console.log(`web-smoke: the topsheet draws (grand total ${(before / 100).toFixed(2)})`);
    if (requests.some((p) => p.startsWith('/charts/'))) fail(`charts were fetched before anyone asked: ${requests.join(', ')}`);

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

    // 3. The charts of accounts: the list, then one chart, fetched when asked.
    const charts = await evaluate(`window.__METHOD_WEB__.invoke('smoke', 'chart_templates', {})`);
    if (!Array.isArray(charts) || charts.length < 5) fail(`the chart list has ${charts && charts.length} charts`);
    const first = charts[0].id;
    if (requests.some((p) => p.endsWith('.mthd') && p.startsWith('/charts/'))) fail('a chart was fetched before it was picked');
    const made = await evaluate(`window.__METHOD_WEB__.invoke('smoke', 'chart_template_new', { id: ${JSON.stringify(first)} })`);
    if (!made || made.budget.title !== 'Untitled' || !made.budget.topsheet.length) fail(`chart ${first} didn't open as a new budget`);
    if (!requests.includes(`/charts/${first}.mthd`)) fail(`chart ${first} wasn't fetched`);
    console.log(`web-smoke: ${charts.length} charts listed; ${first} fetched when picked and opened`);

    // 4. Nothing went wrong on the way.
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
