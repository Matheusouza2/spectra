import test from 'node:test';
import assert from 'node:assert/strict';
import { deriveGraphProperties } from './graph-properties.js';

const n = (id) => ({ id });
const e = (source, target, transition) => ({ source, target, transition });

test('cycle is reversible, deadlock free, with an unused transition', () => {
  const r = deriveGraphProperties([n(0), n(1)], [e(0, 1, 0), e(1, 0, 1)], [0, 1, 2]);
  assert.equal(r.deadlockFree, true);
  assert.equal(r.reversible, true);
  assert.deepEqual(r.deadTransitions, [2]);
});

test('chain ends in a deadlock and cannot return to M0', () => {
  const r = deriveGraphProperties([n(0), n(1), n(2)], [e(0, 1, 0), e(1, 2, 1)], [0, 1]);
  assert.equal(r.deadlockFree, false);
  assert.deepEqual(r.deadlockNodes, [2]);
  assert.equal(r.reversible, false);
  assert.deepEqual(r.nonReversibleNodes, [1, 2]);
  assert.deepEqual(r.deadTransitions, []);
});
