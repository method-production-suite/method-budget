// In-app dialogs: a small form (add a category, rename...), a confirm, and a
// small one-field dialog anchored to a grid cell (a line's note, a new
// global's value). One at a time. Escape cancels, Enter submits.

import { el, $, isMac } from './dom.js';
import { setRules } from './sheet.js';

let open = null;

export function isModalOpen() {
  return open !== null;
}

/**
 * Shows a form. `fields`: [{ key, label, value, placeholder }].
 * `onSubmit(values)` returns a promise; if it rejects with a message, the
 * message is shown and the dialog stays open. Resolves true when submitted,
 * false when cancelled.
 */
export function formDialog({ title, message = '', fields = [], submitText = 'OK', danger = false, onSubmit }) {
  return new Promise((resolve) => {
    const root = $('modal-root');
    const previous = document.activeElement;
    const error = el('p', { class: 'modal-error', attrs: { role: 'alert' } });
    error.hidden = true;
    const inputs = {};
    const form = el('form', { class: 'modal', attrs: { 'aria-labelledby': 'modal-title', novalidate: true } });
    form.append(el('h2', { class: 'modal-title', text: title, attrs: { id: 'modal-title' } }));
    if (message) form.append(el('p', { class: 'modal-message', text: message }));
    for (const f of fields) {
      const id = `modal-${f.key}`;
      const input = el('input', {
        class: 'modal-input',
        attrs: { id, type: 'text', autocomplete: 'off', spellcheck: 'false', placeholder: f.placeholder || '' },
      });
      input.value = f.value || '';
      inputs[f.key] = input;
      form.append(el('label', { class: 'modal-field', attrs: { for: id } }, el('span', { text: f.label }), input));
    }
    form.append(error);
    const cancel = el('button', { class: 'btn', text: 'Cancel', attrs: { type: 'button' } });
    const ok = el('button', { class: danger ? 'btn btn-danger' : 'btn btn-primary', text: submitText, attrs: { type: 'submit' } });
    form.append(el('div', { class: 'modal-actions' }, cancel, ok));
    const backdrop = el('div', { class: 'modal-backdrop' }, form);

    const close = (result) => {
      open = null;
      backdrop.remove();
      if (previous && previous.focus) previous.focus({ preventScroll: true });
      resolve(result);
    };
    cancel.addEventListener('click', () => close(false));
    backdrop.addEventListener('mousedown', (e) => {
      if (e.target === backdrop) close(false);
    });
    form.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        close(false);
      }
    });
    form.addEventListener('submit', async (e) => {
      e.preventDefault();
      const values = {};
      for (const [k, input] of Object.entries(inputs)) values[k] = input.value;
      ok.disabled = true;
      try {
        if (onSubmit) await onSubmit(values);
        close(true);
      } catch (err) {
        error.textContent = String(err);
        error.hidden = false;
        ok.disabled = false;
      }
    });

    open = backdrop;
    root.replaceChildren(backdrop);
    const first = fields.length ? inputs[fields[0].key] : ok;
    first.focus();
    if (first.select) first.select();
  });
}

/** Asks a yes/no question. Resolves true for yes. */
export function confirmDialog({ title, message, confirmText = 'OK', danger = false }) {
  return formDialog({ title, message, submitText: confirmText, danger });
}

/**
 * A small dialog next to `anchor` (a grid cell) with one field: a text box,
 * or with `multiline` a text area (Enter adds a line there; Cmd/Ctrl+Enter
 * saves). `onSubmit(value)` returns a promise; if it rejects, the message
 * shows and the dialog stays open. Resolves true when submitted, false when
 * cancelled (Escape, Cancel, or a click outside).
 */
export function anchoredDialog({ anchor, title, message = '', label, value = '', placeholder = '', multiline = false, submitText = 'Save', onSubmit }) {
  return new Promise((resolve) => {
    const root = $('modal-root');
    const previous = document.activeElement;
    const error = el('p', { class: 'modal-error', attrs: { role: 'alert' } });
    error.hidden = true;
    const id = 'popover-field';
    const input = multiline
      ? el('textarea', { class: 'modal-input popover-text', attrs: { id, rows: '6', spellcheck: 'true', placeholder } })
      : el('input', { class: 'modal-input', attrs: { id, type: 'text', autocomplete: 'off', spellcheck: 'false', placeholder } });
    input.value = value;
    const form = el('form', { class: 'modal popover', attrs: { role: 'dialog', 'aria-labelledby': 'popover-title', novalidate: true } });
    form.append(el('h2', { class: 'modal-title popover-title', text: title, attrs: { id: 'popover-title' } }));
    if (message) form.append(el('p', { class: 'modal-message', text: message }));
    form.append(el('label', { class: 'modal-field', attrs: { for: id } }, el('span', { text: label }), input));
    if (multiline) {
      form.append(el('p', { class: 'popover-hint', text: `${isMac ? '⌘' : 'Ctrl+'}Enter saves, Esc cancels` }));
    }
    form.append(error);
    const cancel = el('button', { class: 'btn', text: 'Cancel', attrs: { type: 'button' } });
    const ok = el('button', { class: 'btn btn-primary', text: submitText, attrs: { type: 'submit' } });
    form.append(el('div', { class: 'modal-actions' }, cancel, ok));
    const backdrop = el('div', { class: 'popover-backdrop' }, form);

    const close = (result) => {
      open = null;
      backdrop.remove();
      setRules('popover', '');
      if (previous && previous.focus) previous.focus({ preventScroll: true });
      resolve(result);
    };
    cancel.addEventListener('click', () => close(false));
    backdrop.addEventListener('mousedown', (e) => {
      if (e.target === backdrop) close(false);
    });
    form.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        close(false);
      } else if (multiline && e.key === 'Enter' && (isMac ? e.metaKey : e.ctrlKey)) {
        e.preventDefault();
        form.requestSubmit();
      }
    });
    form.addEventListener('submit', async (e) => {
      e.preventDefault();
      ok.disabled = true;
      try {
        if (onSubmit) await onSubmit(input.value);
        close(true);
      } catch (err) {
        error.textContent = String(err);
        error.hidden = false;
        ok.disabled = false;
      }
    });

    open = backdrop;
    root.replaceChildren(backdrop);
    placePopover(form, anchor);
    input.focus();
    if (input.select) input.select();
  });
}

/** Puts the dialog under the cell, or above it when there isn't room below. */
function placePopover(form, anchor) {
  if (!anchor || !anchor.isConnected) return;
  const r = anchor.getBoundingClientRect();
  const w = Math.min(form.offsetWidth || 360, window.innerWidth - 16);
  const h = form.offsetHeight || 200;
  const left = Math.round(Math.max(8, Math.min(r.left, window.innerWidth - w - 8)));
  const fitsBelow = r.bottom + 6 + h <= window.innerHeight - 8;
  const top = Math.round(fitsBelow ? r.bottom + 6 : Math.max(8, r.top - 6 - h));
  setRules('popover', `.popover-backdrop > .popover.placed { left: ${left}px; top: ${top}px; }`);
  form.classList.add('placed');
}

/**
 * A dialog whose content the caller builds (lists with a button per item:
 * the recovered budgets and backups dialogs in safety.js). `build(close)`
 * returns the dialog element (a `.modal`). Escape, or a click outside, calls
 * `close`, which is also returned.
 */
export function customDialog(build) {
  const root = $('modal-root');
  const previous = document.activeElement;
  let backdrop = null;
  const close = () => {
    if (!backdrop || open !== backdrop) return;
    open = null;
    backdrop.remove();
    if (previous && previous.focus) previous.focus({ preventScroll: true });
  };
  const box = build(close);
  box.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      close();
    }
  });
  backdrop = el('div', { class: 'modal-backdrop' }, box);
  backdrop.addEventListener('mousedown', (e) => {
    if (e.target === backdrop) close();
  });
  open = backdrop;
  root.replaceChildren(backdrop);
  // Focus the button marked data-autofocus (a safe one), else the first.
  const first = box.querySelector('[data-autofocus]') || box.querySelector('button');
  if (first) first.focus();
  return close;
}
