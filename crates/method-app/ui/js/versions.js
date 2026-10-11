// File > Versions…: named copies of the budget saved inside its file
// (src/versions.rs; decisions.md, entries 17 and 27). Save the budget as a
// version, then restore, rename, delete or compare any of them.
//
// Every change is a command that returns the view (one undo step each), so
// the dialog draws its list from the view's `budget.versions` (id, name,
// created, note: never a version's content). Restoring keeps the budget it
// replaces as a version first; Undo takes the whole restore back.
//
// Questions are asked inside the dialog, on the version's own row (one dialog
// is open at a time, modal.js). Budget text goes in with textContent only.

import { el } from './dom.js';
import { customDialog } from './modal.js';

/**
 * A date-time as the page's local time, ISO 8601 with the offset:
 * "2026-10-09T14:30:00-07:00". The engine never reads the clock.
 */
export function localStamp(d = new Date()) {
  const p = (n) => String(Math.trunc(Math.abs(n))).padStart(2, '0');
  const off = -d.getTimezoneOffset();
  const sign = off >= 0 ? '+' : '-';
  return (
    `${String(d.getFullYear()).padStart(4, '0')}-${p(d.getMonth() + 1)}-${p(d.getDate())}` +
    `T${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}` +
    `${sign}${p(off / 60)}:${p(off % 60)}`
  );
}

/** A version's `created` for display, in the user's own format; as stored when it doesn't parse. */
export function createdText(created) {
  const d = new Date(created);
  return Number.isFinite(d.getTime()) ? d.toLocaleString() : String(created || '');
}

/** The versions in a view, newest first. */
export function versionsOf(view) {
  const list = view && view.budget && Array.isArray(view.budget.versions) ? view.budget.versions : [];
  return list.slice().reverse();
}

/** A name for the next version: "Version N", N one past the count. */
function suggestedName(view) {
  return `Version ${versionsOf(view).length + 1}`;
}

/**
 * Opens the Versions dialog. `ctx`: { view() (the view on screen), call(cmd,
 * args) (runs a command and redraws the budget; resolves the view, rejects
 * with a message), compare(versionId) (opens the compare screen with that
 * version) }.
 */
export function showVersions(ctx) {
  customDialog((close) => {
    let view = ctx.view();
    // The row asking a question or being renamed: { id, mode: 'restore' | 'delete' | 'rename' }.
    let asking = null;
    let busy = false;

    const error = el('p', { class: 'modal-error', attrs: { role: 'alert' } });
    error.hidden = true;
    const showError = (e) => {
      error.textContent = String(e);
      error.hidden = false;
    };
    const clearError = () => {
      error.hidden = true;
      error.textContent = '';
    };

    /** Runs a command; the dialog redraws from the view it returns. */
    const run = async (cmd, args, after) => {
      if (busy) return false;
      busy = true;
      clearError();
      try {
        const v = await ctx.call(cmd, args);
        if (v && v.budget) view = v;
        asking = null;
        if (after) after();
        return true;
      } catch (e) {
        showError(e);
        return false;
      } finally {
        busy = false;
        draw();
      }
    };

    // Saving: a name (required) and a note.
    const nameInput = el('input', {
      class: 'modal-input',
      attrs: { id: 'version-name', type: 'text', autocomplete: 'off', spellcheck: 'false', maxlength: '200', 'data-autofocus': true },
    });
    nameInput.value = suggestedName(view);
    const noteInput = el('textarea', {
      class: 'modal-input versions-note',
      attrs: { id: 'version-note', rows: '2', spellcheck: 'true', placeholder: 'Optional: what this version is for' },
    });
    const saveBtn = el('button', { class: 'btn btn-primary', text: 'Save Version', attrs: { type: 'submit' } });
    const form = el(
      'form',
      { class: 'versions-save', attrs: { novalidate: true, 'aria-label': 'Save the budget as a version' } },
      el('label', { class: 'modal-field', attrs: { for: 'version-name' } }, el('span', { text: 'Name' }), nameInput),
      el('label', { class: 'modal-field', attrs: { for: 'version-note' } }, el('span', { text: 'Note' }), noteInput),
      el('div', { class: 'versions-save-actions' }, saveBtn),
    );
    form.addEventListener('submit', async (e) => {
      e.preventDefault();
      const ok = await run('save_version', { name: nameInput.value, note: noteInput.value, created: localStamp() });
      if (ok) {
        nameInput.value = suggestedName(view);
        noteInput.value = '';
        nameInput.focus();
        nameInput.select();
      }
    });
    // Cmd/Ctrl+Enter in the note saves, as in other note fields.
    noteInput.addEventListener('keydown', (e) => {
      if (e.key === 'Enter' && (e.metaKey || e.ctrlKey) && !e.isComposing) {
        e.preventDefault();
        form.requestSubmit();
      }
    });

    const list = el('ul', { class: 'safety-list versions-list', attrs: { 'aria-label': 'Saved versions' } });
    const empty = el('p', { class: 'versions-empty', text: 'No versions yet. Save one to keep a copy of the budget as it is now.' });

    const button = (text, cls, onClick, title = '') => {
      const b = el('button', { class: cls, text, title: title || undefined, attrs: { type: 'button' } });
      b.addEventListener('click', onClick);
      return b;
    };

    /** Focuses the row's first button after a redraw (keyboard users stay put). */
    const focusRow = (id, sel = 'button') => {
      const row = list.querySelector(`[data-version="${id}"]`);
      const target = row && row.querySelector(sel);
      if (target) target.focus();
    };

    function item(v) {
      const mode = asking && asking.id === v.id ? asking.mode : null;
      const text = el('div', { class: 'safety-item-text' });
      const actions = el('div', { class: 'safety-item-actions' });
      const li = el('li', { class: 'safety-item versions-item', data: { version: v.id } }, text, actions);

      if (mode === 'rename') {
        const input = el('input', {
          class: 'modal-input versions-rename',
          attrs: { type: 'text', autocomplete: 'off', spellcheck: 'false', maxlength: '200', 'aria-label': `New name for ${v.name}` },
        });
        input.value = v.name;
        const save = () => run('rename_version', { id: v.id, name: input.value }, () => setTimeout(() => focusRow(v.id), 0));
        input.addEventListener('keydown', (e) => {
          if (e.key === 'Enter' && !e.isComposing) {
            e.preventDefault();
            save();
          } else if (e.key === 'Escape') {
            e.preventDefault();
            e.stopPropagation();
            asking = null;
            draw();
            focusRow(v.id);
          }
        });
        text.append(input);
        actions.append(
          button('Cancel', 'btn', () => {
            asking = null;
            draw();
            focusRow(v.id);
          }),
          button('Rename', 'btn btn-primary', save),
        );
        setTimeout(() => {
          input.focus();
          input.select();
        }, 0);
        return li;
      }

      text.append(el('div', { class: 'safety-item-main', text: v.name, title: v.name }));
      text.append(el('div', { class: 'safety-item-sub', text: `Saved ${createdText(v.created)}`, title: v.created }));
      if (v.note) text.append(el('div', { class: 'safety-item-sub versions-item-note', text: v.note, title: v.note }));

      if (mode === 'restore' || mode === 'delete') {
        li.classList.add('asking');
        const question =
          mode === 'restore'
            ? `Put "${v.name}" in place of the budget? The budget as it is now is kept as a version first, and Undo takes the restore back.`
            : `Delete "${v.name}"? Undo brings it back.`;
        text.append(el('div', { class: 'versions-question', text: question, attrs: { role: 'status' } }));
        actions.append(
          button('Cancel', 'btn', () => {
            asking = null;
            draw();
            focusRow(v.id);
          }),
          mode === 'restore'
            ? button('Restore', 'btn btn-primary', () => run('restore_version', { id: v.id, created: localStamp() }, () => setTimeout(() => list.querySelector('button') && list.querySelector('button').focus(), 0)))
            : button('Delete', 'btn btn-danger', () => run('delete_version', { id: v.id }, () => setTimeout(() => nameInput.focus(), 0))),
        );
        setTimeout(() => focusRow(v.id, '.btn-primary, .btn-danger'), 0);
        return li;
      }

      actions.append(
        button('Compare', 'btn', () => {
          close();
          ctx.compare(v.id);
        }, 'Compare this version with the budget as it is now'),
        button('Restore…', 'btn', () => {
          asking = { id: v.id, mode: 'restore' };
          clearError();
          draw();
        }, 'Put this version in place of the budget'),
        button('Rename', 'btn', () => {
          asking = { id: v.id, mode: 'rename' };
          clearError();
          draw();
        }),
        button('Delete…', 'btn btn-danger-text', () => {
          asking = { id: v.id, mode: 'delete' };
          clearError();
          draw();
        }),
      );
      return li;
    }

    function draw() {
      const items = versionsOf(view);
      empty.hidden = items.length > 0;
      list.hidden = items.length === 0;
      list.replaceChildren(...items.map(item));
    }
    draw();

    const done = button('Close', 'btn', close);
    return el(
      'div',
      { class: 'modal versions-dialog', attrs: { role: 'dialog', 'aria-labelledby': 'versions-title', 'aria-describedby': 'versions-message' } },
      el('h2', { class: 'modal-title', text: 'Versions', attrs: { id: 'versions-title' } }),
      el('p', {
        class: 'modal-message',
        text: 'Named copies of this budget, kept inside its file. Save one before a big change; restore it, or compare it with the budget as it is now, at any time.',
        attrs: { id: 'versions-message' },
      }),
      form,
      error,
      empty,
      list,
      el('div', { class: 'modal-actions' }, done),
    );
  });
}
