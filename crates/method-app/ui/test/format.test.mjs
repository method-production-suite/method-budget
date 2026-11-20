// Money and numbers as the grid shows them (format.js, levels.js).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { money, currencyMoney, decimal, names } from '../js/format.js';
import { percentText, topsheetOrder } from '../js/levels.js';

test('money groups thousands and puts negatives in parentheses', () => {
  assert.equal(money(123456789), '1,234,567.89');
  assert.equal(money(5), '0.05');
  assert.equal(money(0), '0.00');
  assert.equal(money(-100000), '(1,000.00)');
});

test('money leaves out what is not a number', () => {
  for (const v of [null, undefined, NaN, Infinity]) assert.equal(money(v), '');
});

test('currencyMoney follows the currency entry', () => {
  assert.equal(currencyMoney(123456, { symbol: '$' }), '$1,234.56');
  assert.equal(currencyMoney(123456, { symbol: '€', symbol_after: true, separators: 'point_comma' }), '1.234,56 €');
  assert.equal(currencyMoney(123456, null), '1,234.56');
  assert.equal(currencyMoney(-123456, { symbol: '$' }), '($1,234.56)');
});

test('currencyMoney with no places rounds half away from zero', () => {
  assert.equal(currencyMoney(150, {}, 0), '2');
  assert.equal(currencyMoney(149, {}, 0), '1');
  assert.equal(currencyMoney(-150, {}, 0), '(2)');
  // A negative that rounds to nothing isn't shown as negative.
  assert.equal(currencyMoney(-49, {}, 0), '0');
});

test('decimal keeps the engine digits and groups the whole part', () => {
  assert.equal(decimal('1234567.125'), '1,234,567.125');
  assert.equal(decimal('-3'), '(3)');
  assert.equal(decimal('0.333333333'), '0.333333333');
  assert.equal(decimal(null), '');
});

test('names joins a list and ignores anything else', () => {
  assert.equal(names(['a', 'b']), 'a, b');
  assert.equal(names('a'), '');
});

test('percentText pads to two places', () => {
  assert.equal(percentText('12.5'), '12.50%');
  assert.equal(percentText('-3'), '-3.00%');
  assert.equal(percentText('1234.567'), '1,234.56%');
  assert.equal(percentText(''), '');
});

test('topsheetOrder lists newest first', () => {
  assert.deepEqual(topsheetOrder(3), [2, 1, 0]);
  assert.deepEqual(topsheetOrder(0), []);
});
