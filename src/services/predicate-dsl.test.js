import test from 'node:test';
import assert from 'node:assert/strict';
import { predicateToDsl, reachabilityQuery, parseStateList } from './predicate-dsl.js';

const en = { and: 'AND', or: 'OR', exists: 'exists path', temporal: { eventually: 'eventually' } };
const pt = { and: 'E', or: 'OU', exists: 'existe caminho', temporal: { eventually: 'eventualmente' } };

const states = [
  { label: 'M0', bounds: [{ place: 'IR', low: 3, upp: 3 }, { place: 'IR_BUFFER', low: 0, upp: 0 }] },
  { label: 'M1', bounds: [{ place: 'IR', low: 2, upp: 2 }, { place: 'IR_BUFFER', low: 1, upp: 1 }] },
];

test('a state set becomes a disjunction of exact markings, in both languages', () => {
  assert.equal(predicateToDsl(states, en), '(IR = 3 AND IR_BUFFER = 0) OR (IR = 2 AND IR_BUFFER = 1)');
  assert.equal(predicateToDsl(states, pt), '(IR = 3 E IR_BUFFER = 0) OU (IR = 2 E IR_BUFFER = 1)');
});

test('enabledness bounds render as lower and upper limits and drop empty ones', () => {
  const enabled = [{ label: 't0', bounds: [{ place: 'IR', low: 2, upp: null }, { place: 'B', low: 0, upp: 2 }, { place: 'Free', low: 0, upp: null }] }];
  assert.equal(predicateToDsl(enabled, en), '(IR >= 2 AND B <= 2)');
  assert.equal(predicateToDsl([{ label: 't', bounds: [{ place: 'Free', low: 0, upp: null }] }], en), 'TRUE');
});

test('the shortest-path query wraps the predicate', () => {
  assert.equal(reachabilityQuery('(IR = 3)', en), 'exists path eventually ((IR = 3))');
});

test('state lists accept ids, M-ids and ranges, and reject nonsense', () => {
  assert.deepEqual(parseStateList('0, M2, 4-6'), [0, 2, 4, 5, 6]);
  assert.equal(parseStateList('3-1'), null);
  assert.equal(parseStateList('abc'), null);
  assert.deepEqual(parseStateList(''), []);
});
