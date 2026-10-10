// Small helpers the panel's and Budget Setup's modules share: reading view
// data that may be missing or of the wrong type, a count with its noun, and
// a rejection as text.

/** `v` when it is a plain object, else an empty one. */
export function obj(v) {
  return v && typeof v === 'object' && !Array.isArray(v) ? v : {};
}

/** `v` as text: '' for undefined and null. */
export function str(v) {
  return v === undefined || v === null ? '' : String(v);
}

/** "1 line", "3 lines". */
export function plural(n, one, many) {
  return `${n} ${n === 1 ? one : many}`;
}

/** A rejection as text: the message string commands reject with (api.js), or an Error's message. */
export function errText(e) {
  if (typeof e === 'string') return e;
  if (e && typeof e.message === 'string') return e.message;
  return String(e);
}
