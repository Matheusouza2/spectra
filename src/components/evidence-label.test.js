import test from 'node:test';
import assert from 'node:assert/strict';
import { evidenceTitle } from './evidence-label.js';

test('identifies each P-invariant card by its sequence', () => {
  assert.equal(
    evidenceTitle({ property: 'P_INVARIANTS', sequence: 0, definition: { name: 'P-Invariants' } }),
    'P-Invariant #1',
  );
  assert.equal(
    evidenceTitle({ property: 'P_INVARIANTS', sequence: 2, definition: { name: 'P-Invariants' } }),
    'P-Invariant #3',
  );
});
