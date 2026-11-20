// Setup lists in the order the budget shows them (order.js).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { byteOrder, orderedNames, hasCustomOrder, shortCode } from '../js/order.js';
import { obj, str, errText } from '../js/util.js';

test('byteOrder sorts by code unit, uppercase first', () => {
  assert.deepEqual(['b', 'B', 'a'].sort(byteOrder), ['B', 'a', 'b']);
});

test('orderedNames puts the hand-made order first, then the rest by byte order', () => {
  const budget = { fringes: { c: {}, a: {}, b: {} }, setup_order: { fringes: ['b', 'gone', 'b', 7] } };
  assert.deepEqual(orderedNames(budget, 'fringes'), ['b', 'a', 'c']);
  assert.ok(hasCustomOrder(budget, 'fringes'));
  assert.ok(!hasCustomOrder(budget, 'groups'));
  assert.deepEqual(orderedNames(null, 'fringes'), []);
});

test('shortCode reads the item, or gives none', () => {
  const budget = { groups: { Cast: { short_code: 'C' }, Crew: {} } };
  assert.equal(shortCode(budget, 'groups', 'Cast'), 'C');
  assert.equal(shortCode(budget, 'groups', 'Crew'), '');
  assert.equal(shortCode(budget, 'groups', 'Nobody'), '');
});

test('obj, str and errText take whatever arrives', () => {
  assert.deepEqual(obj([1]), {});
  assert.deepEqual(obj(null), {});
  assert.equal(str(null), '');
  assert.equal(str(0), '0');
  assert.equal(errText('no'), 'no');
  assert.equal(errText(new Error('bad')), 'bad');
  assert.equal(errText(5), '5');
});
