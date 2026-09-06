import test from 'node:test';
import assert from 'node:assert/strict';
import { minimizeLinearCost } from './length.mjs';
import { solveDifferences } from './model.mjs';

test('minimum-cut costs match 1000 independently enumerated bounded integer systems', () => {
  let seed = 237; const rand = n => { seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0; return Math.floor(seed / 65536) % n; };
  let movable = 0, negativeEdges = 0;
  for (let sample = 0; sample < 1000; sample++) {
    const original = Array.from({ length: 3 }, () => rand(4)), edges = [], costs = Array.from({ length: 3 }, () => rand(7) - 3);
    for (let from = 0; from < 3; from++) for (let to = 0; to < 3; to++) if (rand(2)) {
      const weight = original[to] - original[from] - rand(3); edges.push({ from, to, weight }); if (weight < 0) negativeEdges++;
    }
    const lower = solveDifferences([0, 0, 0], edges, original).values;
    const upper = solveDifferences([-3, -3, -3], edges.map(e => ({ from: e.to, to: e.from, weight: e.weight })), original.map(x => -x)).values.map(x => -x);
    const feasible = [];
    for (let a = 0; a <= 3; a++) for (let b = 0; b <= 3; b++) for (let c = 0; c <= 3; c++) {
      const v = [a, b, c]; if (v.every((x, i) => x >= lower[i] && x <= upper[i]) && edges.every(e => v[e.to] >= v[e.from] + e.weight)) feasible.push(v);
    }
    if (feasible.length > 1) movable++;
    const result = minimizeLinearCost(lower, upper, edges, costs);
    const minimum = Math.min(...feasible.map(v => v.reduce((s, x, i) => s + costs[i] * x, 0)));
    assert.equal(result.primal, minimum); assert.equal(result.dual, minimum);
    assert.ok(feasible.some(v => v.every((x, i) => x === result.values[i])));
  }
  assert.ok(movable > 500); assert.ok(negativeEdges > 1000);
});

test('fixed extent still permits shorter chains; zero-cost ties stay deterministic', () => {
  const lower = [0, 0, 10], upper = [0, 9, 10], edges = [{ from: 1, to: 2, weight: 1 }];
  const result = minimizeLinearCost(lower, upper, edges, [0, -1, 1]);
  assert.deepEqual(result.values, [0, 9, 10]); assert.equal(result.primal, 1);
  assert.deepEqual(minimizeLinearCost(lower, upper, edges, [0, 0, 0]).values, lower);
  assert.deepEqual(minimizeLinearCost([2], [2], [], [5]).values, [2]);
});

test('budget and arithmetic failures never return a purported optimum', () => {
  const args = [[0, 0], [3, 3], [{ from: 0, to: 1, weight: 0 }], [-1, 1]];
  for (const [option, code] of [['nodeBudget', 'length-node-budget'], ['edgeBudget', 'length-edge-budget'], ['workBudget', 'length-work-budget']])
    assert.throws(() => minimizeLinearCost(...args, { [option]: 0 }), { code });
  assert.throws(() => minimizeLinearCost([0], [1], [], [Number.MAX_SAFE_INTEGER]), { code: 'length-integer-cost' });
  assert.throws(() => minimizeLinearCost([0, 0], [1, 2], [{ from: 0, to: 1, weight: 1 }], [1, 1]), { code: 'length-bound-certificate' });
  const m = Number.MAX_SAFE_INTEGER;
  assert.equal(minimizeLinearCost([m, 2, m], [m, 2, m], [], [1, 1, -1]).primal, 2);
  assert.equal(minimizeLinearCost([m, m - 1], [m, m - 1], [], [3, -3]).primal, 3);
  assert.deepEqual(minimizeLinearCost([m - 1], [m], [], [-1]).values, [m]);
  assert.throws(() => minimizeLinearCost([0], [1], [], [0], { workBudget: NaN }), { code: 'length-budget-schema' });
  assert.throws(() => minimizeLinearCost([0], [1000], Array.from({ length: 100 }, () => ({ from: 0, to: 0, weight: 0 })), [0], { workBudget: 1500 }), { code: 'length-work-budget' });
});
