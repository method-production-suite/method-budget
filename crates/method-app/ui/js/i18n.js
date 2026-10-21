// The words of the page, looked up by key (docs/APP.md, "Languages";
// decisions.md, entry 94).
//
//   t('find.replace_all')                  "Replace All"
//   t('start.opened', { name })            "Opened {name}" with the name put in
//   t('footer.selected_rows', { count })   "Selected 1 row", "Selected 3 rows"
//
// Each language is a table in ui/i18n/<code>.js: one object of key to words,
// strict JSON after `export default`, so the menu bar (src/i18n.rs) reads the
// same file. A value is a string, or for a count an object of plural forms
// ("one", "other", and "zero", "two", "few", "many" where the language has
// them), picked by the platform's Intl.PluralRules with `count`. `{name}`
// in the words is replaced by params.name.
//
// The language is the system's (the browser's list of languages), English
// when no table matches. A key missing from the language falls back to
// English, and one missing from English too shows the key itself, with a
// warning in the console; a test (src/i18n.rs) keeps both from happening.
//
// The page's own words in index.html carry their key: data-i18n for the
// text, data-i18n-aria-label and data-i18n-title for those attributes, and
// fillPage() puts the language's words in at boot (boot.js). The English
// stays written in index.html too, for the moment before the scripts run; a
// test (src/i18n.rs) keeps it the same as the table's.
//
// Adding a language: put its table in ui/i18n/, import it here and add it
// to TABLES.

import en from '../i18n/en.js';

/** The tables, by language code. English is the fallback for every key. */
export const TABLES = Object.freeze({ en });

/** The fallback language. */
export const FALLBACK = 'en';

/**
 * The first of `wanted` (language tags, most wanted first: "pt-BR", "fr")
 * that has a table, matching the whole tag first and then its language
 * ("pt-BR" takes "pt"); English when none does.
 */
export function pickLanguage(wanted, available = Object.keys(TABLES)) {
  const have = new Map(available.map((a) => [a.toLowerCase(), a]));
  for (const w of Array.isArray(wanted) ? wanted : []) {
    const tag = String(w || '').toLowerCase();
    if (!tag) continue;
    if (have.has(tag)) return have.get(tag);
    const base = tag.split('-')[0];
    if (have.has(base)) return have.get(base);
  }
  return FALLBACK;
}

function systemLanguages() {
  if (typeof navigator === 'undefined') return [];
  if (Array.isArray(navigator.languages) && navigator.languages.length) return navigator.languages;
  return navigator.language ? [navigator.language] : [];
}

/** `{name}` in `text` replaced by params.name; an unknown name stays as written. */
function fill(text, params) {
  if (!params) return text;
  return text.replace(/\{([A-Za-z0-9_]+)\}/g, (whole, name) =>
    Object.prototype.hasOwnProperty.call(params, name) && params[name] !== undefined && params[name] !== null ? String(params[name]) : whole,
  );
}

/**
 * A lookup over `tables` in `lang`, falling back to `fallback`. `warn` hears
 * each missing key once. The page's own is `t` below; tests make others.
 */
export function translator(tables, lang, { fallback = FALLBACK, warn = (m) => console.warn(m) } = {}) {
  const table = tables[lang] || tables[fallback] || {};
  const backup = tables[fallback] || {};
  const rules = new Intl.PluralRules(lang);
  const warned = new Set();
  return function t(key, params) {
    let value = Object.prototype.hasOwnProperty.call(table, key) ? table[key] : undefined;
    let pluralLang = rules;
    if (value === undefined && Object.prototype.hasOwnProperty.call(backup, key)) {
      value = backup[key];
      pluralLang = new Intl.PluralRules(fallback);
      if (!warned.has(key)) {
        warned.add(key);
        warn(`i18n: "${key}" is missing from ${lang}; using ${fallback}`);
      }
    }
    if (value === undefined) {
      if (!warned.has(key)) {
        warned.add(key);
        warn(`i18n: "${key}" is in no table`);
      }
      return key;
    }
    if (value && typeof value === 'object') {
      const n = Number(params && params.count);
      const form = pluralLang.select(Number.isFinite(n) ? n : 0);
      value = value[form] !== undefined ? value[form] : value.other;
    }
    return fill(String(value), params);
  };
}

/** The language the page shows. */
export const LANGUAGE = pickLanguage(systemLanguages());

/** The page's lookup: t(key, params). */
export const t = translator(TABLES, LANGUAGE);

if (typeof document !== 'undefined' && document.documentElement) document.documentElement.lang = LANGUAGE;

/** The attributes fillPage() fills, each from its data-i18n-<attribute> key. */
const FILLED_ATTRS = ['aria-label', 'title'];

/**
 * Puts the language's words into the elements under `root` that name a key:
 * data-i18n="key" sets the text, data-i18n-aria-label and data-i18n-title
 * set those attributes (index.html).
 */
export function fillPage(root = typeof document === 'undefined' ? null : document) {
  if (!root || typeof root.querySelectorAll !== 'function') return;
  for (const node of root.querySelectorAll('[data-i18n]')) node.textContent = t(node.getAttribute('data-i18n'));
  for (const attr of FILLED_ATTRS) {
    for (const node of root.querySelectorAll(`[data-i18n-${attr}]`)) node.setAttribute(attr, t(node.getAttribute(`data-i18n-${attr}`)));
  }
}
