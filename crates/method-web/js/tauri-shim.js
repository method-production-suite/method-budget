// Method Budget in a browser (docs/research/web-session.md, phase 2): the
// part of Tauri the page (crates/method-app/ui) uses, over method-web's Host
// running as WebAssembly in this page. Loaded as a classic script before the
// page's modules, so window.__TAURI__ is there when api.js looks for it.
//
//   core.invoke(cmd, args)   Host.dispatch, with a reply as the desktop
//                            gives it (a refusal rejects with its message)
//   event.listen / emitTo    one event bus for the page and the Budget Setup
//   / emit                   window, which shows over the page in a frame
//
// Where the desktop shows an Open dialog the browser's file picker shows,
// and the picked file goes in as `file: {name, bytes}` (bytes as base64);
// a reply's `downloads` are offered as downloads and the rest is the
// desktop's reply. Save downloads the budget as a .mthd file. Commands only
// the desktop has (windows, autosave, recovery, folder sharing, the updater,
// the setup library's file) are refused with a plain message, or answered
// as "nothing there" where the page asks at start-up.
//
// The page's address can say ?open=<url of a .mthd file> to start with that
// budget instead of a new one, and ?readonly=1 to show every budget
// read-only.
//
// The charts of accounts New From Template offers aren't in the module:
// their list and files are fetched from charts/ beside the page, the list
// when the dialog first asks and a chart's file when it is picked.
//
// A partner embedding the page needs this file beside the page, with
// tauri-shim.css, method_web.js and method_web_bg.wasm beside this file, and
// charts/ beside the page (all of it as `cargo xtask web` lays it out).

(function () {
  'use strict';

  const params = new URLSearchParams(window.location.search);
  const panel = params.get('panel');
  // This window's label, as the desktop names its windows.
  const label = panel === 'setup' ? 'setup' : panel === '1' ? 'tools' : 'main';

  // The Budget Setup window is a frame inside the main page: it shares the
  // main page's Host and event bus.
  let shared = null;
  try {
    if (window.parent !== window && window.parent.__METHOD_WEB__) shared = window.parent.__METHOD_WEB__;
  } catch (e) {
    shared = null; // another site's frame: run on our own
  }
  const web = shared || makeWeb();
  if (!shared) window.__METHOD_WEB__ = web;

  window.__TAURI__ = {
    core: { invoke: (cmd, args) => web.invoke(label, cmd, args || {}) },
    event: {
      listen: (name, handler, opts) => web.listen(name, handler, opts, window),
      emitTo: (target, name, payload) => web.emit(typeof target === 'string' ? target : target && target.label, name, payload),
      emit: (name, payload) => web.emit(null, name, payload),
    },
  };

  function makeWeb() {
    const here = document.currentScript ? document.currentScript.src : window.location.href;
    const doc = document;

    // -----------------------------------------------------------------
    // Starting: the module, the Host, the budget to start with

    let commands = new Set(); // every command the Host runs
    const ready = (async () => {
      const m = await import(new URL('method_web.js', here).href);
      await m.default({ module_or_path: new URL('method_web_bg.wasm', here) });
      const host = new m.Host();
      commands = new Set(m.commands());
      const open = params.get('open');
      if (open) {
        try {
          const url = new URL(open, window.location.href);
          const res = await fetch(url);
          if (!res.ok) throw new Error(`the server answered ${res.status}`);
          const name = decodeURIComponent(url.pathname.split('/').pop() || 'Budget.mthd');
          host.openMthd(name, await res.text());
        } catch (e) {
          notice(`Couldn't open ${open}: ${message(e)}`);
        }
      }
      if (params.get('readonly') === '1') host.setReadOnly(true);
      return host;
    })();
    ready.catch((e) => notice(`Method Budget couldn't start in this browser: ${message(e)}`));

    // -----------------------------------------------------------------
    // Events

    const listeners = new Map(); // id -> { name, target, handler, owner }
    let nextId = 1;

    function listen(name, handler, opts, owner) {
      const id = nextId++;
      const target = opts && opts.target !== undefined ? (typeof opts.target === 'string' ? opts.target : opts.target.label) : null;
      listeners.set(id, { name, target: target || null, handler, owner });
      return Promise.resolve(() => listeners.delete(id));
    }

    /**
     * Sends event `name` to the windows labelled `target` (null: every
     * window), and to listeners that take any window's events; `except`
     * leaves one window out (the one whose command changed the budget).
     */
    function emit(target, name, payload, except = null) {
      for (const [id, l] of listeners) {
        if (l.name !== name) continue;
        if (target && l.target && l.target !== target) continue;
        if (except && l.target === except) continue;
        if (except && !l.target) continue;
        setTimeout(() => {
          if (!listeners.has(id)) return;
          try {
            l.handler({ event: name, id, payload });
          } catch (e) {
            console.error(e);
          }
        }, 0);
      }
      return Promise.resolve();
    }

    function forgetWindow(owner) {
      for (const [id, l] of listeners) if (l.owner === owner) listeners.delete(id);
    }

    // -----------------------------------------------------------------
    // Commands

    // Where the desktop shows an Open dialog: what the picker offers.
    const BUDGET = '.mthd';
    const SHEET = '.xlsx,.csv,.tsv,.txt';
    const PICKS = {
      open_budget: { accept: '.mthd,.json,.xml,.xlsx,.csv,.tsv,.txt,.tab' },
      template_new: { accept: BUDGET },
      import_setup_pick: { accept: BUDGET },
      merge_pick_file: { accept: BUDGET },
      pack_import_pick: { accept: BUDGET },
      compare_pick_files: { accept: BUDGET, multiple: true },
      sheet_import_pick: { accept: SHEET },
      actuals_pick: { accept: SHEET },
      setup_sheet_pick: { accept: SHEET },
      coa_import_pick: { accept: `${SHEET},.mthd` },
      export_import_pick: { accept: '.json,.xml,.xlsx,.txt,.tab,.tsv' },
    };

    // Desktop commands the page asks at start-up: answered as "nothing
    // there" rather than refused, so nothing is shown.
    const QUIET = {
      autosave_setting: false,
      open_pending: null,
      recovery_list: [],
      whats_new: null,
      close_tools_window: null,
      // The page prints itself (report.js falls back to the browser's print).
      print_report: { supported: false },
    };

    const NOT_HERE = 'That is only in the Method Budget app, not in the browser.';

    let lastRev = null;
    let tabs = [];
    let host = null;

    async function invoke(from, cmd, args) {
      host = await ready;
      if (Object.prototype.hasOwnProperty.call(QUIET, cmd)) return QUIET[cmd];
      if (cmd === 'save_budget') return save(from, args);
      if (cmd === 'setup_window') return showSetup();
      if (cmd === 'close_setup_window') return closeSetup();
      // The charts of accounts are files beside the page, fetched when
      // New From Template lists them and when one is picked.
      if (cmd === 'chart_templates') await chartList().catch((e) => console.error(e));
      if (cmd === 'chart_template_new') {
        try {
          await loadChart(String(args.id));
        } catch (e) {
          throw `Couldn't load that chart: ${message(e)}. Check the connection and try again.`;
        }
      }
      const pick = PICKS[cmd];
      if (pick && args.file === undefined && args.files === undefined) {
        const files = await pickFiles(pick);
        if (!files.length) return null;
        args = { ...args };
        if (pick.multiple) args.files = await Promise.all(files.map(fileIn));
        else args.file = await fileIn(files[0]);
      }
      let reply = dispatch(cmd, args);
      if (reply && typeof reply === 'object' && Array.isArray(reply.downloads)) {
        const { downloads, ...rest } = reply;
        for (const f of downloads) download(f.name, decode(f.bytes));
        reply = rest;
      }
      after(from, reply);
      return reply;
    }

    /** Runs a command on the Host: its reply, or a rejection with the page's message. */
    function dispatch(cmd, args) {
      if (!commands.has(cmd)) throw NOT_HERE;
      try {
        return JSON.parse(host.dispatch(cmd, JSON.stringify(args)));
      } catch (e) {
        if (e instanceof WebAssembly.RuntimeError) {
          throw 'Method Budget stopped working in this page. Download what you can with Save, then reload the page.';
        }
        throw message(e);
      }
    }

    /**
     * After a command: the other windows hear the budget changed (a new
     * rev), the page's title follows the budget in front, and closing the
     * page asks first while anything is unsaved.
     */
    function after(from, reply) {
      const view = viewIn(reply);
      if (!view) return;
      if (Array.isArray(view.tabs)) tabs = view.tabs;
      if (view.rev !== lastRev) {
        lastRev = view.rev;
        emit(null, 'view-changed', { doc: view.doc, rev: view.rev }, from);
      }
      const front = tabs.find((t) => t.active);
      if (front && from === 'main') doc.title = `${front.name}${front.dirty ? ' (edited)' : ''} - Method Budget`;
    }

    function viewIn(reply) {
      if (!reply || typeof reply !== 'object') return null;
      if (typeof reply.rev === 'number' && typeof reply.doc === 'number' && 'budget' in reply) return reply;
      return viewIn(reply.view);
    }

    window.addEventListener('beforeunload', (e) => {
      if (!tabs.some((t) => t.dirty)) return;
      e.preventDefault();
      e.returnValue = '';
    });

    // -----------------------------------------------------------------
    // The charts of accounts: not in the module (they would be 6 MB of
    // it). `cargo xtask web` puts charts/index.json (each chart's id, name,
    // group, description and size) and charts/<id>.mthd beside the page;
    // the list is fetched once, when New From Template first asks, and a
    // chart's file only when it is picked.

    const chartsAt = new URL('../charts/', here);
    let listing = null; // a promise of the list
    const loaded = new Set(); // charts handed to the Host

    function chartList() {
      if (!listing) {
        listing = (async () => {
          const res = await fetch(new URL('index.json', chartsAt));
          if (!res.ok) throw new Error(`the server answered ${res.status}`);
          const text = await res.text();
          host.setCharts(text);
          return JSON.parse(text);
        })();
        listing.catch(() => {
          listing = null; // asked again next time
        });
      }
      return listing;
    }

    async function loadChart(id) {
      if (loaded.has(id)) return;
      const entry = (await chartList()).find((c) => c && c.id === id);
      if (!entry) return; // not one of the list: the Host says so
      const res = await fetch(new URL(`${encodeURIComponent(entry.id)}.mthd`, chartsAt));
      if (!res.ok) throw new Error(`the server answered ${res.status}`);
      host.addChart(entry.id, await res.text());
      loaded.add(entry.id);
    }

    // -----------------------------------------------------------------
    // Save and Save As: the budget downloads as a .mthd file

    function save(from, args) {
      const docId = typeof args.doc === 'number' ? args.doc : undefined;
      let name;
      // Save As, or a budget never saved: ask for a name, as the desktop's
      // Save dialog does (the budget's title for a new one).
      const v = dispatch('get_view', { doc: docId === undefined ? null : docId });
      if (args.saveAs || !v.file.path) {
        const title = v.budget && typeof v.budget.title === 'string' ? v.budget.title.trim() : '';
        const suggested = v.file.path ? v.file.name : `${title || 'Untitled'}.mthd`;
        let asked;
        try {
          asked = window.prompt('Save the budget as', suggested);
        } catch (e) {
          asked = suggested; // a browser without prompt(): the suggested name
        }
        if (asked === null || !asked.trim()) return null;
        name = asked.trim();
      }
      let saved;
      try {
        saved = host.saveMthd(docId, name);
      } catch (e) {
        throw message(e);
      }
      download(saved.name, new TextEncoder().encode(saved.text), 'application/json');
      const view = dispatch('get_view', { doc: docId === undefined ? null : docId });
      after(from, view);
      return view;
    }

    // -----------------------------------------------------------------
    // Files in and out

    function pickFiles({ accept, multiple }) {
      return new Promise((resolve) => {
        const input = doc.createElement('input');
        input.type = 'file';
        if (accept) input.accept = accept;
        input.multiple = Boolean(multiple);
        input.style.display = 'none';
        doc.body.append(input);
        let done = false;
        const finish = (files) => {
          if (done) return;
          done = true;
          input.remove();
          resolve(files);
        };
        input.addEventListener('change', () => finish([...(input.files || [])]));
        input.addEventListener('cancel', () => finish([]));
        input.click();
      });
    }

    async function fileIn(f) {
      return { name: f.name, bytes: encode(new Uint8Array(await f.arrayBuffer())) };
    }

    function encode(bytes) {
      let s = '';
      for (let i = 0; i < bytes.length; i += 0x8000) s += String.fromCharCode.apply(null, bytes.subarray(i, i + 0x8000));
      return btoa(s);
    }

    function decode(b64) {
      const s = atob(b64);
      const out = new Uint8Array(s.length);
      for (let i = 0; i < s.length; i++) out[i] = s.charCodeAt(i);
      return out;
    }

    function download(name, bytes, type = 'application/octet-stream') {
      const url = URL.createObjectURL(new Blob([bytes], { type }));
      const a = doc.createElement('a');
      a.href = url;
      a.download = name;
      a.style.display = 'none';
      doc.body.append(a);
      a.click();
      a.remove();
      setTimeout(() => URL.revokeObjectURL(url), 60000);
    }

    // -----------------------------------------------------------------
    // The Budget Setup window: a frame over the page

    let setup = null; // { box, frame }

    function showSetup() {
      if (setup) {
        setup.box.hidden = false;
        setup.frame.focus();
        return true;
      }
      style();
      const box = doc.createElement('div');
      box.className = 'mw-setup';
      const frame = doc.createElement('iframe');
      frame.className = 'mw-setup-frame';
      frame.title = 'Budget Setup';
      const url = new URL(window.location.href);
      url.search = '?panel=setup';
      url.hash = '';
      frame.src = url.href;
      box.append(frame);
      doc.body.append(box);
      setup = { box, frame };
      frame.addEventListener('load', () => frame.focus());
      return false;
    }

    function closeSetup() {
      if (!setup) return null;
      const { box, frame } = setup;
      setup = null;
      if (frame.contentWindow) forgetWindow(frame.contentWindow);
      box.remove();
      emit('main', 'setup-closed', null);
      return null;
    }

    // The frame's styles are a file beside this one (not a <style>
    // element), so a page whose Content-Security-Policy allows only its own
    // stylesheets can show it.
    let styled = false;
    function style() {
      if (styled) return;
      styled = true;
      const link = doc.createElement('link');
      link.rel = 'stylesheet';
      link.href = new URL('tauri-shim.css', here).href;
      doc.head.append(link);
    }

    // -----------------------------------------------------------------
    // Messages

    function message(e) {
      if (typeof e === 'string') return e;
      return (e && e.message) || String(e);
    }

    /** A message where the page shows its own (its toast). */
    function notice(text) {
      console.error(text);
      const show = () => {
        const t = doc.getElementById('toast');
        if (!t) return;
        t.textContent = text;
        t.hidden = false;
        setTimeout(() => {
          t.hidden = true;
        }, 8000);
      };
      if (doc.readyState === 'loading') doc.addEventListener('DOMContentLoaded', show);
      else show();
    }

    return { invoke, listen, emit, ready };
  }
})();
