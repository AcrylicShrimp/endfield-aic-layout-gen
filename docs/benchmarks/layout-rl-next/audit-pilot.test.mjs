import test from 'node:test';
import assert from 'node:assert/strict';
import { trajectoryStats } from './audit-pilot.mjs';

test('late-improvement curves and stagnation tails use the full incumbent history', () => {
  const sample = { work: { proposals: 1024 }, trace: [{ status: 'accepted' }, { status: 'unchanged' }],
    incumbents: [{ proposal: 0, score: [100, 20, 3] }, { proposal: 8, score: [90, 25, 4] },
      { proposal: 200, score: [80, 30, 5] }, { proposal: 800, score: [80, 20, 5] }] };
  const result = trajectoryStats(sample);
  assert.equal(result.first_improvement, 8); assert.equal(result.last_improvement, 800);
  assert.equal(result.no_improvement_tail, 224); assert.equal(result.improvements_after_128, 2);
  assert.deepEqual(result.curve.find(p => p.proposal === 128).score, [90, 25, 4]);
  assert.deepEqual(result.curve.find(p => p.proposal === 512).score, [80, 30, 5]);
  sample.incumbents = sample.incumbents.slice(0, 1);
  const flat = trajectoryStats(sample);
  assert.equal(flat.last_improvement, null); assert.equal(flat.no_improvement_tail, 1024);
  assert.equal(flat.improvements_after_128, 0);
});
