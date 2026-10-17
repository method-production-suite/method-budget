// Headless Chrome over --remote-debugging-pipe, for scripts/web-smoke.mjs and
// scripts/capture-screens.mjs. No npm packages: Chrome's DevTools protocol
// over a pipe (JSON messages ending in a NUL byte on file descriptors 3 and
// 4), so no WebSocket client is needed.
//
// Chrome is found on the PATH (google-chrome, chromium...) or in the usual
// Mac place; CHROME= names another. Node 18 or later.

import { spawn } from 'node:child_process';
import { existsSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

export function findChrome() {
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

/**
 * Starts `chrome` headless with a throwaway profile (removed on close) and
 * `extraArgs`. `send(method, params, sessionId)` resolves with a command's
 * result or rejects with its error; `on(handler)` hears every event.
 */
export function launch(chrome, extraArgs = []) {
  const profile = mkdtempSync(join(tmpdir(), 'method-chrome-'));
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
      ...extraArgs,
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
