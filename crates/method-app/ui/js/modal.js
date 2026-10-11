// In-app dialogs: a small form (add a category, rename...), a confirm, a
// small one-field dialog anchored to a grid cell (a line's note, a new
// global's value), and a small form anchored to a side panel item (a setup
// item's editor, apply.js). One at a time. Escape cancels, Enter submits.

import { el, $, isMac } from './dom.js';
import { setRules } from './sheet.js';

let open = null;

export function isModalOpen() {
  return open !== null;
}

// The page behind an open dialog is inert, as behind the shortcut sheet
// (help.js): Tab can't leave the dialog, and keys can't reach the grid or a
// tools panel item under it. Only what this made inert is given back.
let behind = [];

function shutBehind() {
  openBehind();
  const root = $('modal-root');
  behind = [...document.body.children].filter((n) => n !== root && n.id !== 'toast' && !n.inert);
  for (const n of behind) n.inert = true;
}

function openBehind() {
  for (const n of behind) n.inert = false;
  behind = [];
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
      openBehind();
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
    shutBehind();
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
      openBehind();
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
    shutBehind();
  });
}

/**
 * A small form next to `anchor` (an item in the side panel) with several
 * fields: the editor for one setup item (apply.js). Each field is
 *   { key, label, type, value, options, suffix, placeholder, showIf }
 * where type is 'text' (the default), 'select' (options: [[value, label]]),
 * 'check' (value true or false), 'swatches' (options: [{ id, label, cls }],
 * value an id: a row of color chips, one picked) or 'note' (a quiet line of
 * `text`, a string or a function of the current values; no value of its
 * own). `suffix` is text after a
 * text box, or a function of the current values; `showIf(values)` hides the
 * field while it returns false. Enter saves from any field, Escape cancels.
 * `onSubmit(values)` returns a promise; if it rejects, the message shows and
 * the form stays open. Resolves true when saved, false when cancelled.
 * With no fields it is a question next to the item (`danger` makes the
 * button red). `extra` ({ text, title, run }) adds a button at the left that
 * closes the form (resolving false) and then calls `run`.
 */
export function anchoredForm({ anchor, title, message = '', fields = [], submitText = 'Save', danger = false, extra = null, onSubmit }) {
  return new Promise((resolve) => {
    const root = $('modal-root');
    const previous = document.activeElement;
    const error = el('p', { class: 'modal-error', attrs: { role: 'alert' } });
    error.hidden = true;
    const form = el('form', { class: 'modal popover popover-form', attrs: { role: 'dialog', 'aria-labelledby': 'popover-title', novalidate: true } });
    form.append(el('h2', { class: 'modal-title popover-title', text: title, attrs: { id: 'popover-title' } }));
    if (message) form.append(el('p', { class: 'modal-message', text: message }));
    const box = el('div', { class: 'popover-fields' });
    const parts = fields.map((f, i) => formField(f, `popover-f${i}`));
    box.append(...parts.map((p) => p.wrap));
    form.append(box, error);
    const cancel = el('button', { class: 'btn', text: 'Cancel', attrs: { type: 'button' } });
    const ok = el('button', { class: danger ? 'btn btn-danger' : 'btn btn-primary', text: submitText, attrs: { type: 'submit' } });
    const more = extra
      ? el('button', { class: 'btn btn-quiet btn-danger-text popover-extra', text: extra.text, title: extra.title || undefined, attrs: { type: 'button' } })
      : null;
    form.append(el('div', { class: 'modal-actions' }, more, cancel, ok));
    const backdrop = el('div', { class: 'popover-backdrop' }, form);

    const values = () => Object.fromEntries(parts.map((p) => [p.key, p.get()]));
    const refresh = () => {
      const v = values();
      for (const p of parts) p.refresh(v);
    };
    form.addEventListener('input', refresh);
    form.addEventListener('change', refresh);
    form.addEventListener('method:pick', refresh);

    const close = (result) => {
      open = null;
      backdrop.remove();
      setRules('popover', '');
      openBehind();
      if (previous && previous.isConnected && previous.focus) previous.focus({ preventScroll: true });
      resolve(result);
    };
    cancel.addEventListener('click', () => close(false));
    if (more) {
      more.addEventListener('click', () => {
        close(false);
        extra.run();
      });
    }
    backdrop.addEventListener('mousedown', (e) => {
      if (e.target === backdrop) close(false);
    });
    form.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        close(false);
      } else if (e.key === 'Enter' && !e.isComposing && e.target !== cancel && e.target !== ok && e.target !== more) {
        // Enter saves from any field, a pop-up list or a color chip included.
        e.preventDefault();
        form.requestSubmit();
      }
    });
    form.addEventListener('submit', async (e) => {
      e.preventDefault();
      ok.disabled = true;
      try {
        if (onSubmit) await onSubmit(values());
        close(true);
      } catch (err) {
        error.textContent = String(err);
        error.hidden = false;
        ok.disabled = false;
      }
    });

    open = backdrop;
    root.replaceChildren(backdrop);
    refresh();
    placePopover(form, anchor);
    const first = parts.find((p) => !p.wrap.hidden && !p.note);
    if (first) first.focus();
    else ok.focus();
    shutBehind();
  });
}

/** One field of anchoredForm: { key, wrap, get(), refresh(values), focus() }. */
function formField(f, id) {
  const type = f.type || 'text';
  let control;
  let get;
  let wrap;
  let adorn = null;
  if (type === 'note') {
    const line = el('p', { class: 'popover-note', attrs: { id } });
    return {
      key: f.key,
      note: true,
      wrap: line,
      get: () => '',
      refresh: (v) => {
        line.hidden = typeof f.showIf === 'function' && !f.showIf(v);
        line.textContent = typeof f.text === 'function' ? f.text(v) : String(f.text || '');
      },
      focus: () => {},
    };
  }
  if (type === 'check') {
    control = el('input', { attrs: { id, type: 'checkbox' } });
    control.checked = Boolean(f.value);
    get = () => control.checked;
    wrap = el('label', { class: 'popover-check', attrs: { for: id } }, control, el('span', { text: f.label }));
  } else if (type === 'swatches') {
    let picked = f.value === undefined || f.value === null ? '' : String(f.value);
    const chips = (f.options || []).map((o) =>
      el('button', {
        class: `swatch ${o.cls || ''}`,
        title: o.label,
        attrs: { type: 'button', role: 'radio', 'aria-label': o.label },
        data: { id: o.id },
      }),
    );
    const row = el('div', { class: 'swatch-row', attrs: { role: 'radiogroup', 'aria-labelledby': `${id}-l` } }, ...chips);
    const paint = () => {
      for (const c of chips) {
        const on = c.dataset.id === picked;
        c.setAttribute('aria-checked', String(on));
        c.tabIndex = on || (!chips.some((x) => x.dataset.id === picked) && c === chips[0]) ? 0 : -1;
      }
    };
    const pick = (c) => {
      picked = c.dataset.id;
      paint();
      c.focus();
      row.dispatchEvent(new CustomEvent('method:pick', { bubbles: true }));
    };
    for (const c of chips) {
      c.addEventListener('click', () => pick(c));
      c.addEventListener('keydown', (e) => {
        const step = e.key === 'ArrowRight' || e.key === 'ArrowDown' ? 1 : e.key === 'ArrowLeft' || e.key === 'ArrowUp' ? -1 : 0;
        if (!step) return;
        e.preventDefault();
        const i = chips.indexOf(c);
        pick(chips[(i + step + chips.length) % chips.length]);
      });
    }
    paint();
    control = row;
    get = () => picked;
    wrap = el('div', { class: 'modal-field', attrs: { role: 'group' } }, el('span', { text: f.label, attrs: { id: `${id}-l` } }), row);
    return {
      key: f.key,
      wrap,
      get,
      refresh: (v) => {
        wrap.hidden = typeof f.showIf === 'function' && !f.showIf(v);
      },
      focus: () => (chips.find((c) => c.tabIndex === 0) || chips[0] || row).focus(),
    };
  } else if (type === 'select') {
    control = el('select', { class: 'modal-input', attrs: { id } });
    for (const [v, label] of f.options || []) {
      const o = el('option', { text: label });
      o.value = v;
      control.append(o);
    }
    control.value = f.value === undefined || f.value === null ? '' : String(f.value);
    get = () => control.value;
    wrap = el('label', { class: 'modal-field', attrs: { for: id } }, el('span', { text: f.label }), control);
  } else {
    control = el('input', {
      class: 'modal-input',
      attrs: { id, type: 'text', autocomplete: 'off', spellcheck: 'false', autocapitalize: 'off', placeholder: f.placeholder || '' },
    });
    control.value = f.value === undefined || f.value === null ? '' : String(f.value);
    get = () => control.value;
    const label = el('span', { text: f.label });
    if (f.suffix) {
      adorn = el('span', { class: 'popover-adorn', attrs: { 'aria-hidden': 'true' } });
      wrap = el('label', { class: 'modal-field', attrs: { for: id } }, label, el('div', { class: 'popover-adorned' }, control, adorn));
    } else {
      wrap = el('label', { class: 'modal-field', attrs: { for: id } }, label, control);
    }
  }
  return {
    key: f.key,
    wrap,
    get,
    refresh: (v) => {
      wrap.hidden = typeof f.showIf === 'function' && !f.showIf(v);
      if (adorn) adorn.textContent = typeof f.suffix === 'function' ? f.suffix(v) : String(f.suffix);
    },
    focus: () => {
      control.focus();
      if (type === 'text' && control.select) control.select();
    },
  };
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
 * `close`, which is also returned. `onClose` runs once it has closed, however
 * it closed.
 */
export function customDialog(build, { onClose = null } = {}) {
  const root = $('modal-root');
  const previous = document.activeElement;
  let backdrop = null;
  const close = () => {
    if (!backdrop || open !== backdrop) return;
    open = null;
    backdrop.remove();
    openBehind();
    if (previous && previous.focus) previous.focus({ preventScroll: true });
    if (onClose) onClose();
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
  shutBehind();
  return close;
}
