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

/** The chart groups (src/start.rs, ChartChoice.group), in the order shown. */
const CHART_GROUPS = ['Feature', 'Television', 'Commercial', 'Other'];

/** The chart group a production type looks in first. */
const GROUP_FOR_KIND = {
  feature: 'Feature',
  tv_episode: 'Television',
  tv_season: 'Television',
  commercial: 'Commercial',
  documentary: 'Other',
  other: 'Other',
};

/**
 * The bundled charts ([{id, name, group, description, ours, accounts}]) to
 * list for production type `kind`, as [{group, ours: [...], studio: [...]}]:
 * the type's own group first, then the others in their usual order; in each,
 * our own charts, then the studios' by name. With `query`, only the charts
 * whose name or description has every word of it. Empty groups are left out.
 */
export function chartSections(charts, kind, query = '') {
  const list = Array.isArray(charts) ? charts.filter((c) => c && typeof c.id === 'string' && typeof c.name === 'string') : [];
  const want = words(query);
  const hit = (c) => {
    const hay = `${c.name}\n${c.description || ''}`.toLowerCase();
    return want.every((w) => hay.includes(w));
  };
  const first = GROUP_FOR_KIND[kind] || 'Feature';
  const order = [first, ...CHART_GROUPS.filter((g) => g !== first)];
  const groupOf = (c) => (CHART_GROUPS.includes(c.group) ? c.group : 'Other');
  const byName = (a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base' });
  return order
    .map((group) => {
      const inGroup = list.filter((c) => groupOf(c) === group && hit(c));
      return {
        group,
        ours: inGroup.filter((c) => c.ours),
        studio: inGroup.filter((c) => !c.ours).sort(byName),
      };
    })
    .filter((s) => s.ours.length || s.studio.length);
}

/** "212 accounts", "1 account" (blank when unknown). */
export function accountsText(n) {
  if (!Number.isFinite(n) || n <= 0) return '';
  return `${n} ${n === 1 ? 'account' : 'accounts'}`;
}

/**
 * A file name (without `.mthd`) from a budget title, as src/app.rs's
 * safe_file_stem makes one: characters file systems refuse become spaces,
 * and dots and spaces at the ends go. Blank: "Untitled".
 */
export function fileNameFrom(title) {
  const cleaned = String(title || '')
    .replace(/[/\\:*?"<>|\u0000-\u001f\u007f-\u009f]/g, ' ')
    .trim()
    .replace(/^\.+|\.+$/g, '')
    .trim();
  return cleaned || 'Untitled';
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
 * `blank`, `chart:<id>` or `file`), with the picked file's `token`, Save
 * In's folder token (`folder`; null: no file yet), the setup step's ticks
 * (`setup`: { library: bool, packs: [token] }) and whether to make a
 * project folder for it in that folder (`project`).
 */
export function specFrom(values, startFrom, token, folder = null, setup = null, project = false) {
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
    folder: Number.isSafeInteger(folder) ? folder : null,
    file_name: s('file_name'),
    library: !!(setup && setup.library === true),
    packs: setup && Array.isArray(setup.packs) ? [...new Set(setup.packs.filter(Number.isSafeInteger))] : [],
    project: project === true && Number.isSafeInteger(folder),
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

/**
 * A listed pack's words in New Budget's setup step (src/packs.rs,
 * ListedPack): its title (else its file's name), then its version,
 * effective date, region and item count, and whether it ships with the app.
 */
export function packText(p) {
  if (!p || typeof p !== 'object') return { name: '', detail: '' };
  const name = String(p.title || p.file || '').trim();
  const bits = [];
  if (p.version) bits.push(`version ${p.version}`);
  if (p.effective) bits.push(`from ${p.effective}`);
  if (p.region) bits.push(String(p.region));
  const n = Number.isFinite(p.items) ? p.items : 0;
  bits.push(`${n} ${n === 1 ? 'item' : 'items'}`);
  if (p.place === 'bundled') bits.push('comes with the app');
  return { name, detail: bits.join(', ') };
}

/**
 * A thumbnail's bars (src/start.rs, StartEntry.bars: [{label, share}],
 * share in thousandths) as rectangles in a `width` by `height` box: one
 * column each, bottom-aligned, the largest bar the full height, every bar
 * above nothing at least 1 high. Each with its label and share as words
 * ("21 Camera, 32%").
 */
export function thumbRects(bars, width = 56, height = 24) {
  const list = (Array.isArray(bars) ? bars : []).filter((b) => b && Number.isFinite(b.share));
  if (!list.length) return [];
  const max = Math.max(...list.map((b) => b.share));
  if (!(max > 0)) return [];
  const step = width / list.length;
  const gap = step > 3 ? 1 : 0;
  return list.map((b, i) => {
    const h = b.share > 0 ? Math.max(1, Math.round((b.share / max) * height)) : 0;
    return {
      x: Math.round(i * step * 100) / 100,
      y: height - h,
      w: Math.max(0.5, Math.round((step - gap) * 100) / 100),
      h,
      text: `${String(b.label || '')}, ${shareText(b.share)}`,
    };
  });
}

/** A share in thousandths as a percent: "32%", "0.4%". */
export function shareText(share) {
  if (!Number.isFinite(share) || share <= 0) return '0%';
  if (share < 100) return `${(share / 10).toFixed(1).replace(/\.0$/, '')}%`;
  return `${Math.round(share / 10)}%`;
}

/** The rows of `project` ({items}) that match `query`, as filterEntries does. */
export function projectEntries(project, query) {
  return filterEntries(project && Array.isArray(project.items) ? project.items : [], query);
}
