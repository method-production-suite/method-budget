// The string table (decision 94; docs/APP.md, "Languages").
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { pickLanguage, translator, TABLES } from '../js/i18n.js';

test('pickLanguage takes the whole tag, then its language, then English', () => {
  const have = ['en', 'pt', 'fr-CA'];
  assert.equal(pickLanguage(['fr-CA'], have), 'fr-CA');
  assert.equal(pickLanguage(['pt-BR'], have), 'pt');
  assert.equal(pickLanguage(['de', 'pt'], have), 'pt');
  assert.equal(pickLanguage(['de'], have), 'en');
  assert.equal(pickLanguage(null, have), 'en');
});

test('translator fills names and leaves unknown ones as written', () => {
  const warned = [];
  const t = translator({ en: { hi: 'Hi {name}, {who}' } }, 'en', { warn: (m) => warned.push(m) });
  assert.equal(t('hi', { name: 'Alex' }), 'Hi Alex, {who}');
  assert.deepEqual(warned, []);
});

test('translator picks plural forms by count', () => {
  const t = translator({ en: { n: { one: '{count} line', other: '{count} lines' } } }, 'en', { warn: () => {} });
  assert.equal(t('n', { count: 1 }), '1 line');
  assert.equal(t('n', { count: 2 }), '2 lines');
  assert.equal(t('n', {}), '{count} lines');
});

test('translator falls back to English and warns once per key', () => {
  const warned = [];
  const t = translator({ en: { a: 'A', b: 'B' }, fr: { a: 'À' } }, 'fr', { warn: (m) => warned.push(m) });
  assert.equal(t('a'), 'À');
  assert.equal(t('b'), 'B');
  assert.equal(t('b'), 'B');
  assert.equal(t('nowhere'), 'nowhere');
  assert.equal(warned.length, 2);
});

test('the English table is plain strings or plural objects with "other"', () => {
  for (const [key, value] of Object.entries(TABLES.en)) {
    if (typeof value === 'string') continue;
    assert.equal(typeof value, 'object', key);
    assert.equal(typeof value.other, 'string', `${key} has no "other"`);
  }
});
