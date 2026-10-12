// The start window's plain helpers (start.js): searching the lists, the
// words for a row, the production types and the chart each one starts
// from, and the New Budget form's values as the spec src/start.rs takes.
// No DOM here.

/** The production types New Budget offers: [value, label] (src/start.rs, KINDS). */
export const KINDS = [
  ['feature', 'Feature'],
  ['tv_episode', 'TV episode'],
  ['tv_season', 'TV season'],
  ['commercial', 'Commercial'],
  ['documentary', 'Documentary'],
  ['other', 'Other'],
];

/** Our own bundled chart each type starts from, until Start From is changed by hand. */
const CHART_FOR_KIND = {
  feature: 'feature',
  tv_episode: 'tv-one-hour',
  tv_season: 'tv-one-hour',
  commercial: 'commercial',
  documentary: 'documentary',
};

/**
 * The Start From value for production type `kind` (`chart:<id>`), when that
 * chart is among `charts` ([{id}]); null to leave Start From as it is.
 */
export function startForKind(kind, charts) {
  const id = CHART_FOR_KIND[kind];
  if (!id || !Array.isArray(charts) || !charts.some((c) => c && c.id === id)) return null;
  return `chart:${id}`;
}

/** Fields shown for a type: the season for TV, the episode number for an episode, the count for a season. */
export function tvFields(kind) {
  return {
    season: kind === 'tv_episode' || kind === 'tv_season',
    episode: kind === 'tv_episode',
    episodes: kind === 'tv_season',
  };
}

function words(s) {
  return String(s || '')
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean);
}

/** True when every word of `query` is in the entry's title, file name or folder. */
export function matches(entry, query) {
  const want = words(query);
  if (!want.length) return true;
  const hay = [entry.title, entry.file, entry.folder].map((s) => String(s || '').toLowerCase()).join('\n');
  return want.every((w) => hay.includes(w));
}

/** The entries of `list` that match `query`. */
export function filterEntries(list, query) {
  return (Array.isArray(list) ? list : []).filter((e) => e && matches(e, query));
}

const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

/**
 * When a file last changed, short: "Today 14:05", "Yesterday", "Oct 9"
 * this year, else "Oct 9, 2025". `ms` since 1970; `now` a Date. Local time.
 */
export function whenText(ms, now = new Date()) {
  if (!Number.isFinite(ms)) return '';
  const d = new Date(ms);
  const day = (x) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const days = Math.round((day(now) - day(d)) / 86400000);
  if (days === 0) return `Today ${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
  if (days === 1) return 'Yesterday';
  const md = `${MONTHS[d.getMonth()]} ${d.getDate()}`;
  return d.getFullYear() === now.getFullYear() ? md : `${md}, ${d.getFullYear()}`;
}

/**
 * The New Budget spec (src/start.rs, NewBudgetSpec) from the form's
 * `values` (strings by field name) and its Start From value (`starter`,
 * `blank`, `chart:<id>` or `file`), with the picked file's `token`.
 */
export function specFrom(values, startFrom, token) {
  const v = values && typeof values === 'object' ? values : {};
  const s = (k) => (typeof v[k] === 'string' ? v[k].trim() : '');
  const spec = {
    title: s('title'),
    kind: s('kind') || 'feature',
    season: '',
    episode: '',
    episodes: '',
    prep_start: s('prep_start'),
    shoot_start: s('shoot_start'),
    shoot_end: s('shoot_end'),
    wrap_end: s('wrap_end'),
    prep_weeks: s('prep_weeks'),
    shoot_days: s('shoot_days'),
    wrap_weeks: s('wrap_weeks'),
    country: s('country'),
    region: s('region'),
    city: s('city'),
    currency: s('currency').toUpperCase(),
    start: 'starter',
    chart: '',
    template: null,
    fringe_posting: s('fringe_posting'),
    arithmetic: s('arithmetic'),
  };
  const tv = tvFields(spec.kind);
  if (tv.season) spec.season = s('season');
  if (tv.episode) spec.episode = s('episode');
  if (tv.episodes) spec.episodes = s('episodes');
  const from = String(startFrom || 'starter');
  if (from.startsWith('chart:')) {
    spec.start = 'chart';
    spec.chart = from.slice('chart:'.length);
  } else if (from === 'file') {
    spec.start = 'file';
    spec.template = Number.isSafeInteger(token) ? token : null;
  } else if (from === 'blank') {
    spec.start = 'blank';
  }
  return spec;
}
