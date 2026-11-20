// Follow (design 9.3; decision 100): what each new status does while
// following someone. The browser part (moving the grid) stays in share.js.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { followStep } from '../js/share.js';

const cell = (key) => ({ kind: 'row', key, parent: '50-01', field: 'rate' });
const following = (at = null) => ({ actor: 'a1', name: 'Jordan', doc: 3, at });
const status = (person, live_on = true) => ({ live_on, people: person ? [person] : [] });
const jordan = (extra = {}) => ({ actor: 'a1', linked: true, cell: cell('7'), ...extra });

test('a new cell moves the view there', () => {
  assert.deepEqual(followStep(following(), status(jordan()), 3), { go: cell('7'), at: JSON.stringify(cell('7')) });
});

test('the same cell stays put', () => {
  const f = following(JSON.stringify(cell('7')));
  assert.equal(followStep(f, status(jordan()), 3), 'stay');
});

test('no cell stays until they say more', () => {
  assert.equal(followStep(following(), status(jordan({ cell: null })), 3), 'stay');
});

test('an open dialog holds the move', () => {
  assert.equal(followStep(following(), status(jordan()), 3, true), 'stay');
});

test('the link ending stops it', () => {
  assert.equal(followStep(following(), status(jordan({ linked: false })), 3), 'stop');
});

test('the person leaving stops it', () => {
  assert.equal(followStep(following(), status(null), 3), 'stop');
  assert.equal(followStep(following(), status({ ...jordan(), actor: 'b2' }), 3), 'stop');
});

test('live mode turning off stops it', () => {
  assert.equal(followStep(following(), status(jordan(), false), 3), 'stop');
});

test('another budget in front stops it', () => {
  assert.equal(followStep(following(), status(jordan()), 4), 'stop');
});

test('a missing or odd status stops it rather than throwing', () => {
  assert.equal(followStep(following(), null, 3), 'stop');
  assert.equal(followStep(following(), { live_on: true }, 3), 'stop');
});
