import test from 'node:test';
import assert from 'node:assert/strict';
import { read } from '../layout-rl/io.mjs';
import { compare } from '../layout-rl/score.mjs';

test('committed pilot snapshot preserves action budgets, sealed tests and exact-score comparisons', () => {
  const result = read(new URL('./pilot-results.json', import.meta.url));
  assert.equal(result.test_evaluations, 0);
  assert.equal(result.training.length, 4);
  assert.ok(result.training.every(r => r.actions === 8192));
  assert.equal(result.training.find(r => r.signal === 'zero').changed_weight_elements, 0);
  assert.equal(result.independently_validated_incumbents,
    result.training.reduce((n, r) => n + r.episodes, 0) + result.validation.length);
  const longest = h => result.validation.filter(r => r.policy === `h${h}/00-terminal-7/sample` && r.budget.proposals === 1024);
  assert.equal(longest(16).length, 9);
  for (const h of [32, 64]) {
    const outcomes = [0, 0, 0];
    for (const row of longest(h)) {
      const before = longest(16).find(r => r.root === row.root);
      outcomes[compare(row.best, before.best) + 1]++;
      assert.equal(row.best[0], before.best[0]);
    }
    assert.deepEqual(outcomes, h === 32 ? [0, 9, 0] : [0, 8, 1]);
  }
  assert.equal(longest(64).filter(r => r.improvements_after_128 > 0).length, 6);
});
