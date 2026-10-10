// Saving safety in the page: the Autosave switch, budgets recovered after a
// crash, File > Revert to Backup, File > Open Recent and files the system asks
// the app to open. The work is done in Rust (src/safety_app.rs). Budget text
// goes in with textContent only (dom.js).

import { invoke } from './api.js';
import { el } from './dom.js';
import { customDialog, isModalOpen } from './modal.js';
import { isHelpOpen, closeHelp } from './help.js';

/** A time in milliseconds since 1970, in the user's own format. */
export function formatTime(ms) {
  const d = new Date(ms);
  return Number.isFinite(d.getTime()) ? d.toLocaleString() : '';
}

function sizeText(bytes) {
  if (bytes < 1024) return `${bytes} bytes`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function item(lines, ...buttons) {
  const text = el('div', { class: 'safety-item-text' });
  lines.forEach((line, i) => {
    if (!line) return;
    text.append(el('div', { class: i === 0 ? 'safety-item-main' : 'safety-item-sub', text: line, title: line }));
  });
  return el('li', { class: 'safety-item' }, text, el('div', { class: 'safety-item-actions' }, ...buttons));
}

/**
 * Lists budgets recovered after the app closed without saving them.
 * `entries`: [{ key, title, name, path, saved_at_ms }] (recovery_list).
 * `open(key)` opens one (the dialog closes first); `discard(key)` deletes
 * one and resolves with the rest. Later (or Escape) keeps them all for the
 * next start.
 */
export function showRecoveryDialog(entries, { open, discard }) {
  customDialog((close) => {
    const list = el('ul', { class: 'safety-list' });
    const draw = (items) => {
      if (!items.length) {
        close();
        return;
      }
      list.replaceChildren(
        ...items.map((e) => {
          const openBtn = el('button', { class: 'btn btn-primary', text: 'Open Recovered', attrs: { type: 'button' } });
          const discardBtn = el('button', { class: 'btn', text: 'Discard', attrs: { type: 'button' } });
          openBtn.addEventListener('click', () => {
            close();
            open(e.key);
          });
          // Discarding can't be undone, so it takes a second click.
          discardBtn.addEventListener('click', async () => {
            if (!discardBtn.classList.contains('btn-danger')) {
              discardBtn.classList.add('btn-danger');
              discardBtn.textContent = 'Discard for Good';
              return;
            }
            discardBtn.disabled = true;
            draw(await discard(e.key));
          });
          const where = e.path ? e.path : 'Untitled (never saved)';
          return item([e.title || e.name, where, `Autosaved ${formatTime(e.saved_at_ms)}`], openBtn, discardBtn);
        }),
      );
    };
    draw(entries);
    const later = el('button', { class: 'btn', text: 'Later', attrs: { type: 'button', 'data-autofocus': true } });
    later.addEventListener('click', close);
    return el(
      'div',
      { class: 'modal modal-wide', attrs: { role: 'dialog', 'aria-labelledby': 'safety-title' } },
      el('h2', { class: 'modal-title', text: 'Recovered budgets', attrs: { id: 'safety-title' } }),
      el('p', {
        class: 'modal-message',
        text: 'Film Wizard Budgeting closed before these changes were saved. Open one to keep working on it, or discard it. Later keeps them for next time.',
      }),
      list,
      el('div', { class: 'modal-actions' }, later),
    );
  });
}

/**
 * File > Revert to Backup: the open budget's backups, newest first.
 * `name` is the budget's file name; `entries`: [{ id, saved_at_ms, bytes }]
 * (backup_list). `restore(id)` puts one in place of the budget (one undo
 * step); `reveal()` shows the backups folder.
 */
export function showBackupsDialog(name, entries, { restore, reveal }) {
  customDialog((close) => {
    const message = entries.length
      ? `Earlier versions of ${name}, kept each time a save replaced them. Restoring one puts it in place of the budget; Undo brings this version back.`
      : 'There are no backups of this budget yet. A backup is kept each time a save replaces a budget file.';
    const list = el(
      'ul',
      { class: 'safety-list' },
      ...entries.map((b) => {
        const btn = el('button', { class: 'btn', text: 'Restore', attrs: { type: 'button' } });
        btn.addEventListener('click', () => {
          close();
          restore(b.id, formatTime(b.saved_at_ms));
        });
        return item([formatTime(b.saved_at_ms), sizeText(b.bytes)], btn);
      }),
    );
    const show = el('button', { class: 'btn', text: 'Show Backups Folder', attrs: { type: 'button' } });
    show.addEventListener('click', () => reveal());
    const done = el('button', { class: 'btn btn-primary', text: 'Close', attrs: { type: 'button', 'data-autofocus': true } });
    done.addEventListener('click', close);
    return el(
      'div',
      { class: 'modal modal-wide', attrs: { role: 'dialog', 'aria-labelledby': 'safety-title' } },
      el('h2', { class: 'modal-title', text: 'Revert to Backup', attrs: { id: 'safety-title' } }),
      el('p', { class: 'modal-message', text: message }),
      entries.length ? list : null,
      el('div', { class: 'modal-actions' }, show, done),
    );
  });
}

/**
 * The budget's file changed outside the app (a sync service, another
 * computer, another app) since it was opened or last saved. Autosave into it
 * is paused until a choice is made. `keep()`, `load()` and `copy()` carry
 * out the three choices; Decide Later (or Escape) just closes.
 */
export function showConflictDialog(name, { keep, load, copy }) {
  customDialog((close) => {
    const choice = (text, fn) => {
      const b = el('button', { class: 'btn', text, attrs: { type: 'button' } });
      b.addEventListener('click', () => {
        close();
        fn();
      });
      return b;
    };
    const later = el('button', { class: 'btn btn-primary', text: 'Decide Later', attrs: { type: 'button', 'data-autofocus': true } });
    later.addEventListener('click', close);
    return el(
      'div',
      { class: 'modal modal-wide', attrs: { role: 'dialog', 'aria-labelledby': 'safety-title' } },
      el('h2', { class: 'modal-title', text: 'This budget was changed outside Film Wizard', attrs: { id: 'safety-title' } }),
      el('p', {
        class: 'modal-message',
        text: `${name} was changed on disk by something else (a sync service, another computer or another app) since you opened or last saved it. Autosave is paused for this budget until you choose.`,
      }),
      el(
        'ul',
        { class: 'safety-list' },
        item(['Keep mine', 'Save this budget over the file. The version on disk is kept in Backups first.'], choice('Keep Mine', keep)),
        item(['Load theirs', 'Open the version on disk. This budget is kept in Backups first, and Undo brings it back.'], choice('Load Theirs', load)),
        item(['Save mine as a copy', 'Save this budget as a new file and leave the one on disk as it is.'], choice('Save Mine As…', copy)),
      ),
      el('div', { class: 'modal-actions' }, later),
    );
  });
}

// ---------------------------------------------------------------------------
// Wiring: the Autosave switch, events from Rust, menu commands and start-up

function listen(name, fn) {
  const events = window.__TAURI__ && window.__TAURI__.event;
  if (!events || typeof events.listen !== 'function') return;
  events.listen(name, (e) => fn(e.payload)).catch(() => {});
}

/**
 * Sets up saving safety in the page. `ctx`: { grid, toast, call, view(),
 * openBy(cmd, args) (opens the budget the command returns, in a tab),
 * fileState({doc, rev, file, tabs}) (a budget was saved without a command:
 * autosave), saveAs() (File > Save As), showDoc(doc) (brings a budget's tab
 * to the front), doc() (the budget in front) }.
 * Returns { commands, start }: menu command handlers by id, and what to run
 * once the first view is drawn.
 */
export function initSafety(ctx) {
  const toggle = document.getElementById('autosave-switch');
  const setToggle = (on) => {
    if (toggle) toggle.checked = Boolean(on);
  };
  if (toggle) {
    toggle.closest('label').title =
      'Save into the budget file a few seconds after each change. A budget not saved yet gets a recovery copy instead.';
    toggle.addEventListener('change', async () => {
      try {
        setToggle(await invoke('set_autosave', { on: toggle.checked }));
      } catch (e) {
        ctx.toast(e);
      }
    });
  }
  listen('autosave-changed', setToggle);
  listen('file-state', (p) => p && typeof p === 'object' && ctx.fileState(p));
  listen('autosave-error', (msg) => ctx.toast(msg));
  listen('open-request', () => openRequested());
  listen('external-change', (p) => p && typeof p === 'object' && externalChange(p));

  // A budget's file changed outside the app ({doc, name}): ask about one at a
  // time, after any dialog, with that budget's tab in front.
  const waiting = []; // budgets still to ask about, in the order they came
  let asking = false;
  async function externalChange(p) {
    if (!waiting.some((w) => w.doc === p.doc)) waiting.push({ doc: p.doc, name: String(p.name || '') });
    if (asking) return;
    asking = true;
    try {
      while (waiting.length) {
        if (isHelpOpen()) closeHelp();
        while (isModalOpen()) await new Promise((r) => setTimeout(r, 300));
        const { doc, name } = waiting.shift();
        if (ctx.grid.editor) ctx.grid.commitEdit(null);
        if (typeof doc === 'number' && doc !== ctx.doc()) await ctx.showDoc(doc);
        // Closed meanwhile, or couldn't come to the front: nothing to ask.
        if (typeof doc === 'number' && doc !== ctx.doc()) continue;
        const resolveWith = async (choice) => {
          try {
            await ctx.call('resolve_external_change', { choice, doc });
          } catch (e) {
            ctx.toast(e);
          }
          ctx.grid.focus();
        };
        showConflictDialog(name, {
          keep: () => resolveWith('keep'),
          load: () => resolveWith('load'),
          copy: () => ctx.saveAs(),
        });
        // One question at a time.
        while (isModalOpen()) await new Promise((r) => setTimeout(r, 300));
      }
    } finally {
      asking = false;
    }
  }

  // A file from the system (double-click, Open With): wait for any dialog,
  // finish a cell edit, then open it in a tab.
  let opening = false;
  async function openRequested() {
    if (opening) return;
    opening = true;
    try {
      if (isHelpOpen()) closeHelp();
      while (isModalOpen()) await new Promise((r) => setTimeout(r, 300));
      if (ctx.grid.editor) ctx.grid.commitEdit(null);
      await ctx.openBy('open_pending');
    } finally {
      opening = false;
    }
  }

  async function revertToBackup() {
    const v = ctx.view();
    let entries = [];
    try {
      entries = await invoke('backup_list');
    } catch (e) {
      ctx.toast(e);
      return;
    }
    showBackupsDialog(v ? v.file.name : '', entries, {
      restore: async (id, when) => {
        try {
          await ctx.call('restore_backup', { id });
          ctx.toast(`Restored the version from ${when}. Undo brings back the one before.`);
        } catch (e) {
          ctx.toast(e);
        }
        ctx.grid.focus();
      },
      reveal: () => invoke('reveal_backups').catch((e) => ctx.toast(e)),
    });
  }

  const commands = { 'file.revert_backup': revertToBackup };
  for (let i = 0; i < 10; i++) commands[`file.recent.${i}`] = () => ctx.openBy('open_recent', { index: i });

  async function start() {
    try {
      setToggle(await invoke('autosave_setting'));
    } catch (e) {
      ctx.toast(e);
    }
    // A file the app was started with (double-click, Open With, command line).
    await ctx.openBy('open_pending');
    let recovered = [];
    try {
      recovered = await invoke('recovery_list');
    } catch (e) {
      ctx.toast(e);
    }
    if (recovered.length) {
      showRecoveryDialog(recovered, {
        open: (key) => ctx.openBy('open_recovered', { key }),
        discard: (key) => invoke('discard_recovered', { key }).catch((e) => {
          ctx.toast(e);
          return [];
        }),
      });
    }
  }

  return { commands, start };
}
