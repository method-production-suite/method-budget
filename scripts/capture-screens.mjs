#!/usr/bin/env node
// Takes the README and site pictures (SUITE-STANDARDS.md, section 7). Run by
// `cargo xtask capture-screens` (crates/xtask/src/screens.rs), which builds
// the page, runs the command line for the terminal picture, and copies what
// this writes into assets/screens/ and site/img/ when it changed:
//
//   node scripts/capture-screens.mjs --web <browser build> --budget <.mthd>
//        --sheet <.csv> --terminal <terminal.json> --out <dir>
//
// Writes <out>/<name>-<light|dark>.png for hero, explain, import and file,
// and the unframed captures in <out>/raw/. Docs: docs/WEBSITE.md, "Screenshots".
//
// The real page (crates/method-app/ui) runs against the real engine compiled
// to WebAssembly (the browser build, docs/research/web-session.md), served on
// 127.0.0.1 and driven in headless Chrome over the DevTools protocol
// (scripts/chrome-pipe.mjs, no npm). It opens the invented Field Station
// budget and is stepped into each state the way a person gets there: clicks
// on the grid, the menu commands the menu bar sends, the file Import
// Spreadsheet asks for.
//
// Capture mode lives here and nowhere else: the desktop app and the browser
// build ship nothing of it. Before the page's own scripts run, this installs
// a wrapper around the shim's `invoke` (CAPTURE below) that:
//   - answers `host_info` as the desktop does (kind "desktop"), so the page
//     lays itself out as the Mac app: no File button, no browser-only notes;
//   - answers the few desktop-only commands the page asks at start-up as the
//     desktop would with nothing to show (no update, no start window);
//   - hands Import Spreadsheet the invented sheet instead of a file picker;
//   - fails the capture, naming the command, if the page asks anything else
//     the browser build can't answer.
// It also hides what changes from run to run: the text cursor's blink,
// animations and transitions. The window frame (title bar, traffic lights,
// rounded corners, shadow) is drawn by a second page around each capture.
//
// Every wait is for a state the page reaches, never a fixed sleep; a state
// not reached in time fails the run naming the step.

import { createServer } from 'node:http';
import { mkdirSync, readFileSync, writeFileSync, existsSync, statSync } from 'node:fs';
import { extname, join, normalize, resolve, sep } from 'node:path';

import { findChrome, launch } from './chrome-pipe.mjs';

// The window: 1280 by 800 logical pixels, a 28-pixel title bar over the page.
export const WINDOW = { width: 1280, height: 800, titlebar: 28 };
export const SCALE = 2;
// The space around the window for its shadow.
const PAD = { top: 36, side: 44, bottom: 60 };
const CANVAS = { width: WINDOW.width + 2 * PAD.side, height: WINDOW.height + PAD.top + PAD.bottom };
const PAGE = { width: WINDOW.width, height: WINDOW.height - WINDOW.titlebar };
const THEMES = ['light', 'dark'];
const STEP_MS = 30_000;
const BUDGET_NAME = 'Field Station.mthd';
const SHEET_NAME = 'Camera department.csv';

const TYPES = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json',
  '.wasm': 'application/wasm',
  '.mthd': 'application/json',
  '.csv': 'text/csv',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
};

class Failed extends Error {}
function fail(message) {
  throw new Failed(message);
}

function args(argv) {
  const want = ['--web', '--budget', '--sheet', '--terminal', '--out'];
  const out = {};
  for (let i = 0; i < argv.length; i++) {
    if (!want.includes(argv[i]) || i + 1 >= argv.length) fail(`usage: capture-screens.mjs ${want.map((w) => `${w} <path>`).join(' ')}`);
    out[argv[i].slice(2)] = resolve(argv[++i]);
  }
  for (const w of want) if (!out[w.slice(2)]) fail(`${w} is needed`);
  return out;
}

// ---------------------------------------------------------------------------
// The server: the browser build, the budget, and the frame's pictures

function serve(opts, frames) {
  const web = opts.web;
  const server = createServer((req, res) => {
    const path = decodeURIComponent(new URL(req.url, 'http://x').pathname);
    if (path === `/fs/${BUDGET_NAME}`) return send(res, readFileSync(opts.budget), '.mthd');
    if (path.startsWith('/capture/')) {
      const body = frames.get(path);
      return body ? send(res, body, extname(path)) : res.writeHead(404).end();
    }
    if (/^\/favicon\.(ico|svg)$/.test(path)) return res.writeHead(204).end();
    let file = normalize(join(web, path));
    if (!(file + sep).startsWith(web + sep) && file !== web) return res.writeHead(403).end();
    if (existsSync(file) && statSync(file).isDirectory()) file = join(file, 'index.html');
    if (!existsSync(file)) return res.writeHead(404).end();
    return send(res, readFileSync(file), extname(file));
  });
  function send(res, body, ext) {
    res.writeHead(200, { 'content-type': TYPES[ext] || 'application/octet-stream', 'cache-control': 'no-store' });
    res.end(body);
  }
  return new Promise((ok) => server.listen(0, '127.0.0.1', () => ok({ server, port: server.address().port })));
}

// ---------------------------------------------------------------------------
// Capture mode, installed before the page's scripts (see the top of the file)

function captureScript(sheet) {
  return `(() => {
  const SHEET = ${JSON.stringify(sheet)};
  // What the desktop answers to the commands only it has, with nothing to show.
  const DESKTOP = {
    update_status: { state: 'none' },
    start_list: { fresh: false, folders: [], recent: [], pinned: [] },
    whats_new: null,
  };
  const NOT_HERE = 'That is only in the Method Budget app, not in the browser.';
  const capture = (window.__CAPTURE__ = { refused: [], errors: [] });
  let tauri;
  Object.defineProperty(window, '__TAURI__', {
    configurable: true,
    get: () => tauri,
    set(v) {
      if (v && v.core && typeof v.core.invoke === 'function') {
        const inner = v.core.invoke;
        v.core.invoke = async (cmd, args) => {
          if (cmd === 'host_info') return { ...(await inner(cmd, args)), kind: 'desktop' };
          if (Object.prototype.hasOwnProperty.call(DESKTOP, cmd)) return DESKTOP[cmd];
          if (cmd === 'sheet_import_pick') args = { ...(args || {}), file: SHEET };
          try {
            return await inner(cmd, args);
          } catch (e) {
            if (e === NOT_HERE) capture.refused.push(cmd);
            else capture.errors.push(cmd + ': ' + (e && e.message ? e.message : String(e)));
            throw e;
          }
        };
      }
      tauri = v;
    },
  });
  // Nothing that moves: no blinking text cursor, no animation, no scrollbars
  // (the Mac draws them only while scrolling).
  const style = document.createElement('style');
  style.textContent = '*, *::before, *::after { caret-color: transparent !important; animation: none !important; transition: none !important; } ::-webkit-scrollbar { display: none; }';
  document.addEventListener('DOMContentLoaded', () => document.head.append(style));
})();`;
}

// ---------------------------------------------------------------------------
// The shots, in the suite's order

// Each shot's steps run on a freshly opened Field Station budget; `ready` is
// the state the picture is taken in.
// The tools panel a little wider than its first-run width, as someone who
// works in it drags it (its edge is saved per user, js/panelsize.js).
const PANEL = { 'method.panelWidth': 470 };
const ALL_TABS = ['fringes', 'groups', 'globals', 'units', 'presets', 'find', 'explain', 'sets', 'locations', 'currencies', 'notes', 'library', 'history', 'overtime', 'actuals', 'review', 'activity'];

// Each shot's steps run on a freshly opened Field Station budget, with the
// preferences a person would have saved (`prefs`, the page's localStorage);
// the picture is taken when the last step's state is reached.
const SHOTS = [
  {
    name: 'hero',
    prefs: PANEL,
    async steps(p) {
      await p.click('the Cast category', rowCell('13', 'Cast', 'Cast'));
      await p.until('the tools panel on Cast', `document.body.innerText.includes('All lines in 13 Cast')`);
      await p.click('the Locations tab', tab('Locations'));
      await p.until('the locations list', `document.body.innerText.includes('Hollins Ridge fire road')`);
    },
  },
  {
    name: 'explain',
    // Every tool in one pane, so the explanation has the panel's height.
    prefs: { ...PANEL, 'method.toolsLayout': { panes: { top: ALL_TABS, bottom: [] }, active: { top: 'explain', bottom: null } } },
    async steps(p) {
      await p.click('the Cast category', rowCell('13', 'Cast', 'last'), 2);
      await p.click('the Principal cast account', rowCell('13-01', 'Principal cast', 'last'), 2);
      await p.until('the Cast detail lines', `document.body.innerText.includes('DELACROIX') && document.body.innerText.includes('Rehearsal day, three leads')`);
      await p.click("DELACROIX's fringes", rowCell('DELACROIX', null, '2,926.00'));
      await p.menu('edit.explain');
      await p.until('the explanation', `(() => { const t = document.body.innerText; return t.includes('SUI') && t.includes('7,000.00'); })()`);
      // Open the fringe that stops at its cutoff, and the one that doesn't reach its own.
      for (const name of ['SUI', 'FICA']) {
        await p.click(`the ${name} step`, explainToggle(name));
        await p.until(`the ${name} step's working`, `(() => { const r = ${explainToggle(name)}; return r && r.classList.contains('is-open') && !document.querySelector('.explain-loading'); })()`);
      }
    },
  },
  {
    name: 'import',
    prefs: PANEL,
    async steps(p) {
      await p.menu('file.import_spreadsheet');
      await p.until('the import review', `(() => { const d = document.querySelector('.si-dialog'); return d && d.querySelector('.si-recon:not([hidden])') && d.querySelector('.si-table'); })()`);
      // Scrolled down to the preview, as a person reads on.
      await p.scrollTo('the preview', `[...document.querySelectorAll('.si-dialog .si-heading')].find((h) => h.innerText.trim() === 'Preview') || null`);
    },
  },
];

/**
 * The grid cell in the one row whose cells include `first` (and `second`):
 * its last cell, or the cell reading `which`.
 */
function rowCell(first, second, which) {
  return `(() => {
    const rows = [...document.querySelectorAll('tr')].filter((tr) => {
      const t = [...tr.children].map((td) => td.innerText.trim());
      return t.includes(${JSON.stringify(first)}) ${second ? `&& t.includes(${JSON.stringify(second)})` : ''};
    });
    if (rows.length !== 1) return null;
    const cells = [...rows[0].children];
    return ${which === 'last' ? 'cells[cells.length - 1]' : `cells.find((td) => td.innerText.trim() === ${JSON.stringify(which)}) || null`};
  })()`;
}

/** The open-or-close arrow of the Explain step labelled `label`. */
function explainToggle(label) {
  return `(() => {
    const r = [...document.querySelectorAll('.explain-row')].filter((e) => e.querySelector('.explain-label')?.innerText.trim() === ${JSON.stringify(label)});
    return r.length === 1 ? r[0].querySelector('.explain-toggle') : null;
  })()`;
}

function tab(label) {
  return `(() => {
    const t = [...document.querySelectorAll('[role=tab], button')].filter((b) => b.innerText.trim() === ${JSON.stringify(label)});
    return t.length === 1 ? t[0] : null;
  })()`;
}

// ---------------------------------------------------------------------------
// The frame: the capture inside a drawn Mac window

const FRAME_CSS = `
html, body { margin: 0; background: transparent; }
body { width: ${CANVAS.width}px; height: ${CANVAS.height}px; position: relative; overflow: hidden;
  font-family: -apple-system, BlinkMacSystemFont, 'Helvetica Neue', sans-serif; }
.win { position: absolute; border-radius: 10px; overflow: hidden;
  box-shadow: 0 22px 50px rgba(0,0,0,.28), 0 0 0 0.5px rgba(0,0,0,.32); }
.bar { height: ${WINDOW.titlebar}px; position: relative; display: flex; align-items: center; justify-content: center;
  font-size: 13px; font-weight: 600; box-sizing: border-box; }
.lights { position: absolute; left: 13px; top: 0; height: 100%; display: flex; gap: 8px; align-items: center; }
.lights i { width: 12px; height: 12px; border-radius: 50%; box-sizing: border-box; display: block; }
.lights i:nth-child(1) { background: #ff5f57; border: 0.5px solid #e0443e; }
.lights i:nth-child(2) { background: #febc2e; border: 0.5px solid #dea123; }
.lights i:nth-child(3) { background: #28c840; border: 0.5px solid #1aab29; }
.win img { display: block; }
.light .bar { background: #e8e6e3; color: #4d4d4d; border-bottom: 1px solid #d1cfcc; }
.dark .bar { background: #2f2f2f; color: #c8c8c8; border-bottom: 1px solid #1c1c1c; }
.dark.win { box-shadow: 0 22px 50px rgba(0,0,0,.45), 0 0 0 0.5px rgba(255,255,255,.18); }
.term pre { margin: 0; padding: 12px 14px; font: 12.5px/17px Menlo, Monaco, monospace; tab-size: 16; white-space: pre; }
.light .term-body { background: #ffffff; color: #1d1d1f; }
.dark .term-body { background: #1e1e1e; color: #e6e6e6; }
.prompt { color: #2f7d32; }
.dark .prompt { color: #7bd88f; }
`;

function windowHtml(theme, { left, top, width, height, title, body }) {
  return `<div class="win ${theme}" style="left:${left}px;top:${top}px;width:${width}px;height:${height}px">
  <div class="bar"><span class="lights"><i></i><i></i><i></i></span>${escape(title)}</div>${body}</div>`;
}

function frameHtml(theme, raw) {
  const body = `<img src="${raw}" width="${PAGE.width}" height="${PAGE.height}" alt="">`;
  return page(theme, windowHtml(theme, { left: PAD.side, top: PAD.top, width: WINDOW.width, height: WINDOW.height, title: 'Method Budget', body }));
}

/** The file you own: the app's window, and in front of it a terminal with the command line's real output. */
function fileHtml(theme, raw, terminal) {
  const scale = 0.74;
  const app = windowHtml(theme, {
    left: PAD.side,
    top: PAD.top,
    width: WINDOW.width * scale,
    height: WINDOW.titlebar + PAGE.height * scale,
    title: 'Method Budget',
    body: `<img src="${raw}" width="${PAGE.width * scale}" height="${PAGE.height * scale}" alt="">`,
  });
  const lines = terminal.output.replace(/\n$/, '').split('\n').length + 2;
  const height = WINDOW.titlebar + 24 + lines * 17;
  const width = 560;
  const text = `<span class="prompt">field-station %</span> ${escape(terminal.command)}\n${escape(terminal.output)}<span class="prompt">field-station %</span> `;
  const term = windowHtml(theme, {
    left: CANVAS.width - PAD.side - width,
    top: CANVAS.height - PAD.bottom - height,
    width,
    height,
    title: 'field-station',
    body: `<div class="term-body" style="height:${height - WINDOW.titlebar}px"><pre>${text}</pre></div>`,
  });
  return page(theme, app + term.replace('class="win', 'class="win term'));
}

function page(theme, inner) {
  return `<!doctype html><html><head><meta charset="utf-8"><style>${FRAME_CSS}</style></head><body class="${theme}">${inner}</body></html>`;
}

function escape(s) {
  return String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

// ---------------------------------------------------------------------------
// Driving the page

async function main() {
  const opts = args(process.argv.slice(2));
  if (!existsSync(join(opts.web, 'method-web', 'method_web_bg.wasm'))) fail(`${opts.web} isn't a browser build (cargo xtask web)`);
  const terminal = JSON.parse(readFileSync(opts.terminal, 'utf8'));
  if (typeof terminal.command !== 'string' || typeof terminal.output !== 'string' || !terminal.output.trim()) fail(`${opts.terminal} has no command output`);
  const sheet = { name: SHEET_NAME, bytes: readFileSync(opts.sheet).toString('base64') };
  mkdirSync(join(opts.out, 'raw'), { recursive: true });

  const chrome = findChrome();
  if (!chrome) fail('no Chrome or Chromium found; set CHROME=<path>');
  const frames = new Map([['/capture/blank.html', Buffer.from('<!doctype html><title>capture</title>')]]);
  const { server, port } = await serve(opts, frames);
  const origin = `http://127.0.0.1:${port}`;
  const browser = launch(chrome, ['--hide-scrollbars', '--font-render-hinting=none', '--force-color-profile=srgb']);
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
      }
    });
    await send('Runtime.enable');
    await send('Page.enable');
    await send('Page.addScriptToEvaluateOnNewDocument', { source: captureScript(sheet) });

    const evaluate = async (expression) => {
      const r = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
      if (r.exceptionDetails) {
        const e = r.exceptionDetails.exception;
        throw new Error(e?.description || e?.value || r.exceptionDetails.text);
      }
      return r.result.value;
    };
    const until = async (what, expression, ms = STEP_MS) => {
      const end = Date.now() + ms;
      for (;;) {
        const v = await evaluate(expression).catch(() => null);
        if (v) return v;
        if (Date.now() > end) fail(`${what}: not reached within ${ms / 1000} s`);
        await new Promise((r) => setTimeout(r, 100));
      }
    };
    // Two frames after the fonts are in: what is on screen is drawn.
    const settle = () => evaluate(`document.fonts.ready.then(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(() => r(true)))))`);
    const p = {
      until,
      /** Clicks (or double-clicks) the middle of the element `finder` returns. */
      async click(what, finder, count = 1) {
        const box = await until(what, `(() => { const e = ${finder}; if (!e) return null; e.scrollIntoView({ block: 'nearest' }); const r = e.getBoundingClientRect(); return r.width && r.height ? { x: r.left + r.width / 2, y: r.top + r.height / 2 } : null; })()`);
        for (let n = 1; n <= count; n++) {
          await send('Input.dispatchMouseEvent', { type: 'mousePressed', x: box.x, y: box.y, button: 'left', clickCount: n });
          await send('Input.dispatchMouseEvent', { type: 'mouseReleased', x: box.x, y: box.y, button: 'left', clickCount: n });
        }
        await settle();
      },
      /** Scrolls the element `finder` returns to the top of its scrolling box, as the scroll wheel would. */
      async scrollTo(what, finder) {
        await until(what, `(() => { const e = ${finder}; if (!e) return null; e.scrollIntoView({ block: 'start' }); return true; })()`);
        await settle();
      },
      /** A menu command, as the menu bar sends it to the window. */
      async menu(id) {
        await evaluate(`window.__TAURI__.event.emitTo('main', 'menu', ${JSON.stringify(id)})`);
        await settle();
      },
    };

    const shoot = async (clip) => {
      const { data } = await send('Page.captureScreenshot', { format: 'png', captureBeyondViewport: false, ...(clip ? { clip: { ...clip, scale: 1 } } : {}) });
      return Buffer.from(data, 'base64');
    };
    // The page as it stays: captured until two captures in a row are the same.
    const still = async (what) => {
      let last = await shoot();
      for (let n = 0; n < 20; n++) {
        await settle();
        const now = await shoot();
        if (now.equals(last)) return now;
        last = now;
      }
      fail(`${what}: the page kept changing`);
    };
    const open = async (prefs) => {
      await send('Storage.clearDataForOrigin', { origin, storageTypes: 'local_storage,session_storage' });
      await send('Page.navigate', { url: `${origin}/capture/blank.html` });
      await until('a blank page', `document.readyState === 'complete' && location.pathname === '/capture/blank.html'`);
      await evaluate(`(() => { const p = ${JSON.stringify(prefs || {})}; for (const k in p) localStorage.setItem(k, JSON.stringify(p[k])); return true; })()`);
      await send('Emulation.setDefaultBackgroundColorOverride', {});
      await send('Emulation.setDeviceMetricsOverride', { width: PAGE.width, height: PAGE.height, deviceScaleFactor: SCALE, mobile: false });
      await send('Page.navigate', { url: `${origin}/?open=fs/${encodeURIComponent(BUDGET_NAME)}` });
      await until('the Field Station topsheet', `document.title.startsWith(${JSON.stringify(BUDGET_NAME)}) && document.body.innerText.includes('Story and screenplay') && document.body.innerText.includes('393,018.91')`);
      await until('the desktop layout', `!document.getElementById('btn-file')`);
      await settle();
    };
    const check = async (what) => {
      const c = await evaluate('window.__CAPTURE__');
      if (c.refused.length) fail(`${what}: the page asked for ${[...new Set(c.refused)].join(', ')}, which the browser build doesn't have; answer it in capture-screens.mjs`);
      if (c.errors.length) fail(`${what}: a command failed: ${c.errors.join('; ')}`);
      if (problems.length) fail(`${what}: the page reported problems:\n  ${problems.join('\n  ')}`);
    };
    const framed = async (theme, name, html) => {
      const key = `/capture/${theme}-${name}.html`;
      frames.set(key, Buffer.from(html));
      await send('Emulation.setDeviceMetricsOverride', { width: CANVAS.width, height: CANVAS.height, deviceScaleFactor: SCALE, mobile: false });
      await send('Emulation.setDefaultBackgroundColorOverride', { color: { r: 0, g: 0, b: 0, a: 0 } });
      await send('Page.navigate', { url: `${origin}${key}` });
      await until(`the ${name} frame`, `document.readyState === 'complete' && [...document.images].every((i) => i.complete && i.naturalWidth > 0)`);
      await settle();
      const png = await shoot();
      writeFileSync(join(opts.out, `${name}-${theme}.png`), png);
      console.log(`capture-screens: ${name}-${theme}.png`);
    };

    for (const theme of THEMES) {
      await send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: theme }] });
      const raws = {};
      for (const shot of SHOTS) {
        await open(shot.prefs);
        try {
          await shot.steps(p);
        } catch (e) {
          // What the page showed when it stopped, to see why.
          const seen = join(opts.out, 'raw', `${shot.name}-${theme}-failed.png`);
          await shoot().then((png) => writeFileSync(seen, png)).catch(() => {});
          if (e instanceof Failed) fail(`${shot.name} (${theme}): ${e.message} (the page as it was: ${seen})`);
          throw e;
        }
        // The pointer rests on the footer, where nothing changes under it.
        await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: PAGE.width / 2, y: PAGE.height - 8 });
        await settle();
        await check(`${shot.name} (${theme})`);
        const raw = await still(`${shot.name} (${theme})`);
        writeFileSync(join(opts.out, 'raw', `${shot.name}-${theme}.png`), raw);
        raws[shot.name] = `/capture/raw-${theme}-${shot.name}.png`;
        frames.set(raws[shot.name], raw);
      }
      for (const shot of SHOTS) await framed(theme, shot.name, frameHtml(theme, raws[shot.name]));
      await framed(theme, 'file', fileHtml(theme, raws.hero, terminal));
    }
  } finally {
    browser.close();
    server.close();
  }
}

main().catch((e) => {
  console.error(`capture-screens: ${e instanceof Failed ? e.message : e.stack || e}`);
  process.exitCode = 1;
});
