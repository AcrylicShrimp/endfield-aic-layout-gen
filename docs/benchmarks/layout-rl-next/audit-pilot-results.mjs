import assert from 'node:assert/strict';
import path from 'node:path';
import { read } from '../layout-rl/io.mjs';
import { compare } from '../layout-rl/score.mjs';

// These measured outcomes describe the historical seed-7 horizon pilot only.
// Audit reported fields; do not rerun validation or establish solver correctness.
const scope = 'historical-seed-7-horizon-pilot-reported-fields-only';
let input = null;
try {
  const argv = process.argv.slice(2);
  if (argv.length !== 2 || argv[0] !== '--input' || !argv[1] || argv[1].startsWith('--'))
    throw new Error('Usage: node audit-pilot-results.mjs --input <pilot-results.json>');
  input = path.resolve(argv[1]);
  const result = read(input);
  assert.equal(result.schema_version, 1);
  assert.equal(result.status, 'pilot-complete-not-generalization-proof');
  assert.equal(result.test_evaluations, 0);
  assert.equal(result.training.length, 4);
  assert.ok(result.training.every(r => r.actions === 8192));
  assert.equal(result.training.find(r => r.signal === 'zero').changed_weight_elements, 0);
  assert.equal(result.independently_validated_incumbents,
    result.training.reduce((n, r) => n + r.episodes, 0) + result.validation.length);
  const longest = h => result.validation.filter(r => r.policy === `h${h}/00-terminal-7/sample` && r.budget.proposals === 1024);
  assert.equal(longest(16).length, 9);
  for (const h of [32, 64]) {
    assert.equal(longest(h).length, 9);
    const outcomes = [0, 0, 0];
    for (const row of longest(h)) {
      const before = longest(16).find(r => r.root === row.root);
      assert.ok(before, `Missing h16 comparison for ${row.root}`);
      outcomes[compare(row.best, before.best) + 1]++;
      assert.equal(row.best[0], before.best[0]);
    }
    assert.deepEqual(outcomes, h === 32 ? [0, 9, 0] : [0, 8, 1]);
  }
  assert.equal(longest(64).filter(r => r.improvements_after_128 > 0).length, 6);
  console.log(JSON.stringify({ status: 'passed', scope, input }));
} catch (error) {
  console.error(JSON.stringify({ code: 'pilot-results-audit-failed', severity: 'error',
    stage: 'evidence-audit', scope, entity: input, message: error.message }));
  process.exitCode = 1;
}
