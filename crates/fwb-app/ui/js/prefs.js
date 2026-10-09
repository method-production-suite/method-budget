// Per-user display choices (which detail columns show, the setup panel's
// width), kept in the web view's localStorage. Never budget data. Storage can
// be missing or full; then a choice lasts until the window closes.

const PREFIX = 'fwb.';

/** The stored value for `name`, or `fallback` when there is none or it can't be read. */
export function readPref(name, fallback) {
  try {
    const s = window.localStorage.getItem(PREFIX + name);
    return s === null ? fallback : JSON.parse(s);
  } catch {
    return fallback;
  }
}

export function writePref(name, value) {
  try {
    window.localStorage.setItem(PREFIX + name, JSON.stringify(value));
  } catch {
    // No storage: the choice holds for this session only.
  }
}
