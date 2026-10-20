// Updates in the page: the quiet indicator in the toolbar, What's New once
// after an update, and Help > Release Notes. The work is done in Rust
// (src/updater.rs): it checks, downloads, says where an update is
// (`update-status` events and `update_status`) and, on Restart to update,
// saves every budget and starts again. This file only draws what it says.
//
// Nothing here is a modal while you work: an automatic check is silent
// until an update is ready, and a check from Help > Check for Updates
// answers in the indicator for a few seconds. What's New opens only at the
// start, after any other start-up question.
//
// Release notes arrive as plain text pieces (title, headline, sections of
// bullets) and go in with textContent through dom.js's el(), never as markup.

import { invoke } from './api.js';
import { el } from './dom.js';
import { customDialog, isModalOpen } from './modal.js';
import { isHelpOpen } from './help.js';

/** How long a one-time answer (up to date, couldn't check) stays, in ms. */
export const SAY_MS = 8000;

/** The indicator's words, by phase and by answer. */
export const TEXT = {
  checking: 'Checking for updates…',
  downloading: 'Downloading the update…',
  ready: 'Update ready: Restart to update',
  installing: 'Restarting to update…',
  current: 'Method Budget is up to date.',
  failed: 'Couldn’t check for updates',
  // A manifest with no build for this computer (an Intel Mac before releases
  // were universal again in 0.6, or anything that isn't a Mac).
  'no-build': 'There’s no update for this computer yet. This version stays as it is.',
  'install-failed': 'Couldn’t install the update. Try again in a moment.',
};

const PHASES = new Set(['idle', 'checking', 'downloading', 'ready', 'installing']);

/** A status from Rust, or null when it isn't one (src/updater.rs, Status). */
export function readStatus(s) {
  if (!s || typeof s !== 'object' || !PHASES.has(s.phase)) return null;
  return {
    phase: s.phase,
    version: typeof s.version === 'string' ? s.version : null,
    manual: s.manual === true,
    said: typeof s.said === 'string' && Object.hasOwn(TEXT, s.said) ? s.said : null,
    automatic: s.automatic !== false,
  };
}

/**
 * What the indicator shows for `status` with the answer `said` still
 * showing: { hidden, text, button: null | { label, disabled }, tip }.
 * Pure, so the harness can test every state.
 */
export function view(status, said) {
  const v = status && status.version ? status.version : '';
  switch (status && status.phase) {
    case 'ready':
      return {
        hidden: false,
        text: said === 'install-failed' ? TEXT['install-failed'] : '',
        button: { label: TEXT.ready, disabled: false },
        tip: `Method Budget ${v} is ready. Restarting saves your budgets first, and your tabs come back.`,
      };
    case 'installing':
      return { hidden: false, text: '', button: { label: TEXT.installing, disabled: true }, tip: '' };
    case 'checking':
    case 'downloading':
      if (status.manual) return { hidden: false, text: TEXT[status.phase], button: null, tip: '' };
      break;
    default:
      break;
  }
  if (said && said !== 'install-failed') return { hidden: false, text: TEXT[said], button: null, tip: '' };
  return { hidden: true, text: '', button: null, tip: '' };
}

/**
 * The toolbar indicator. `box` holds `text` and `button`; `onRestart()` runs
 * on a click of the ready button. `timers` is { setTimeout, clearTimeout }
 * (the window's, or the harness's clock). Returns { apply(status), state() }.
 */
export function createIndicator({ box, text, button, onRestart, timers = window }) {
  let status = readStatus({ phase: 'idle' });
  let said = null;
  let sayTimer = null;

  function draw() {
    const v = view(status, said);
    box.hidden = v.hidden;
    text.textContent = v.text;
    text.hidden = !v.text;
    button.hidden = !v.button;
    if (v.button) {
      button.textContent = v.button.label;
      button.disabled = v.button.disabled;
    }
    button.title = v.tip;
  }

  function apply(raw) {
    const next = readStatus(raw);
    if (!next) return;
    status = next;
    if (next.said) {
      said = next.said;
      timers.clearTimeout(sayTimer);
      sayTimer = timers.setTimeout(() => {
        said = null;
        sayTimer = null;
        draw();
      }, SAY_MS);
    } else if (next.manual && (next.phase === 'checking' || next.phase === 'downloading')) {
      // A new check the user asked for replaces an old answer.
      said = null;
    }
    draw();
  }

  button.addEventListener('click', () => {
    if (!button.disabled) onRestart();
  });
  draw();
  return { apply, state: () => ({ status, said, view: view(status, said) }) };
}

/**
 * The release notes as a dialog box: the heading, the headline, then each
 * section's title and bullets (a bold claim, then the rest). Built from
 * text pieces with el(); `close` closes it.
 */
export function notesBox(notes, heading, close) {
  const body = el('div', { class: 'notes-body', attrs: { tabindex: '0', role: 'region', 'aria-label': heading } });
  if (notes.headline) body.append(el('p', { class: 'notes-headline', text: notes.headline }));
  for (const s of Array.isArray(notes.sections) ? notes.sections : []) {
    body.append(el('h3', { class: 'notes-section', text: String(s.title || '') }));
    const list = el('ul', { class: 'notes-list' });
    for (const it of Array.isArray(s.items) ? s.items : []) {
      const li = el('li');
      if (it.lead) li.append(el('strong', { text: String(it.lead) }), it.body ? ' ' : '');
      if (it.body) li.append(String(it.body));
      list.append(li);
    }
    body.append(list);
  }
  const ok = el('button', { class: 'btn btn-primary', text: 'Close', attrs: { type: 'button', 'data-autofocus': true } });
  ok.addEventListener('click', close);
  return el(
    'div',
    { class: 'modal modal-wide notes-dialog', attrs: { role: 'dialog', 'aria-modal': 'true', 'aria-labelledby': 'notes-title' } },
    el('h2', { class: 'modal-title', text: heading, attrs: { id: 'notes-title' } }),
    body,
    el('div', { class: 'modal-actions' }, ok),
  );
}

/** Shows `notes` under `heading`, after any dialog already open. */
export async function showNotes(notes, heading, { wait = (ms) => new Promise((r) => setTimeout(r, ms)) } = {}) {
  while (isModalOpen() || isHelpOpen()) await wait(300);
  customDialog((close) => notesBox(notes, heading, close));
}

function listen(name, fn) {
  const events = window.__TAURI__ && window.__TAURI__.event;
  if (!events || typeof events.listen !== 'function') return;
  events.listen(name, (e) => fn(e.payload)).catch(() => {});
}

/**
 * Sets updates up in the main window. `ctx`: { grid, toast }. Returns
 * { commands, start }: Help > Release Notes, and what to run once the first
 * view is drawn and the start-up questions are asked (What's New, once).
 */
export function initUpdates(ctx) {
  const box = document.getElementById('update-status');
  const text = document.getElementById('update-text');
  const button = document.getElementById('btn-update');
  let indicator = null;
  if (box && text && button) {
    indicator = createIndicator({
      box,
      text,
      button,
      onRestart: async () => {
        // Finish a cell being typed first, so it is saved with the rest.
        if (ctx.grid && ctx.grid.editor) ctx.grid.commitEdit(null);
        try {
          await invoke('update_restart');
        } catch (e) {
          ctx.toast(e);
        }
      },
    });
    listen('update-status', (s) => indicator.apply(s));
  }

  async function releaseNotes() {
    let notes = null;
    try {
      notes = await invoke('release_notes');
    } catch (e) {
      ctx.toast(e);
      return;
    }
    if (!notes) {
      ctx.toast('There are no release notes for this version.');
      return;
    }
    await showNotes(notes, notes.title || `Method Budget ${notes.version}`);
  }

  async function start() {
    if (indicator) {
      try {
        indicator.apply(await invoke('update_status'));
      } catch (e) {
        // An older or test build without the updater: no indicator.
      }
    }
    // What's New, once after an update: after the start-up questions
    // (recovered budgets, a file to open), never on top of them.
    while (isModalOpen() || isHelpOpen()) await new Promise((r) => setTimeout(r, 300));
    let notes = null;
    try {
      notes = await invoke('whats_new');
    } catch (e) {
      return;
    }
    if (notes) await showNotes(notes, `What’s New in ${notes.version}`);
  }

  return { commands: { 'help.release_notes': releaseNotes }, start };
}
