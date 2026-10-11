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

/**
 * Where setup item `name` of `kind` (`fringe`, `global`...) came from, in
 * words: the setup pack it was imported from, its effective date, notes and
 * links (the budget's `setup_sources`, docs/FORMAT.md). Empty when nothing
 * is recorded.
 */
export function sourceText(view, kind, name) {
  const all = view && view.budget && view.budget.setup_sources;
  const s = all && all[kind] && Object.prototype.hasOwnProperty.call(all[kind], name) ? all[kind][name] : null;
  if (!s || typeof s !== 'object') return '';
  const parts = [];
  if (str(s.pack)) parts.push(`From ${s.pack}${str(s.pack_version) ? ` (${s.pack_version})` : ''}${str(s.effective) ? `, effective ${s.effective}` : ''}`);
  else if (str(s.effective)) parts.push(`Effective ${s.effective}`);
  if (str(s.notes)) parts.push(str(s.notes));
  for (const u of Array.isArray(s.urls) ? s.urls : []) if (str(u)) parts.push(str(u));
  return parts.join('. ');
}
