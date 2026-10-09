// Tiny DOM helpers. Text always goes in through textContent, never innerHTML,
// so nothing in a budget file can turn into markup or script.

/**
 * Creates an element.
 * props: { class, text, title, attrs: {name: value}, data: {key: value}, on: {event: fn} }
 * children: nodes or strings (strings become text nodes); null/false are skipped.
 */
export function el(tag, props = {}, ...children) {
  const e = document.createElement(tag);
  if (props.class) e.className = props.class;
  if (props.text !== undefined && props.text !== null) e.textContent = String(props.text);
  if (props.title) e.title = props.title;
  if (props.attrs) {
    for (const [k, v] of Object.entries(props.attrs)) {
      if (v === false || v === null || v === undefined) continue;
      e.setAttribute(k, v === true ? '' : String(v));
    }
  }
  if (props.data) {
    for (const [k, v] of Object.entries(props.data)) e.dataset[k] = String(v);
  }
  if (props.on) {
    for (const [k, fn] of Object.entries(props.on)) e.addEventListener(k, fn);
  }
  for (const c of children) {
    if (c === null || c === undefined || c === false) continue;
    e.append(typeof c === 'string' ? document.createTextNode(c) : c);
  }
  return e;
}

export function $(id) {
  return document.getElementById(id);
}

/** True on macOS, where shortcuts use Command instead of Control. */
export const isMac = /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);

/** A shortcut for display, e.g. shortcut('mod+shift+s') is "⇧⌘S" on a Mac. */
export function shortcut(spec) {
  const parts = spec.toLowerCase().split('+');
  const key = parts.pop();
  const names = {
    enter: isMac ? '↩' : 'Enter',
    backspace: isMac ? '⌫' : 'Backspace',
    escape: 'Esc',
  };
  const k = names[key] || key.toUpperCase();
  if (isMac) {
    let s = '';
    if (parts.includes('alt')) s += '⌥';
    if (parts.includes('shift')) s += '⇧';
    if (parts.includes('mod')) s += '⌘';
    return s + k;
  }
  const mods = [];
  if (parts.includes('mod')) mods.push('Ctrl');
  if (parts.includes('alt')) mods.push('Alt');
  if (parts.includes('shift')) mods.push('Shift');
  return [...mods, k].join('+');
}

/** True when a keyboard event has the platform's command modifier. */
export function hasMod(e) {
  return isMac ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey;
}

/** True when focus is in a text field (where the browser's own editing keys apply). */
export function isTextField(node) {
  if (!node || !node.tagName) return false;
  const t = node.tagName;
  return t === 'INPUT' || t === 'TEXTAREA' || t === 'SELECT' || node.isContentEditable;
}
