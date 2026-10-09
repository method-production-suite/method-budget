// In-app dialogs: a small form (add a category, rename...) and a confirm.
// One at a time. Escape cancels, Enter submits.

import { el, $ } from './dom.js';

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
