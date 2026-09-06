import test from 'node:test';
import assert from 'node:assert/strict';
import { shortestPath, distance, key, direction } from './path.mjs';
const bounds = { minX: 0, minY: 0, maxX: 3, maxY: 2 }, start = { x: 0, y: 1 }, end = { x: 3, y: 1 };

test('A* matches exhaustive simple paths on every small obstacle grid', () => {
  const cells = Array.from({ length: 12 }, (_, i) => ({ x: i % 4, y: Math.floor(i / 4) })).filter(p => key(p) !== key(start) && key(p) !== key(end));
  for (let mask = 0; mask < 1 << cells.length; mask++) {
    const blocked = new Set(cells.filter((_, i) => mask & (1 << i)).map(key));
    let expected = null;
    const walk = (p, path, turns) => {
      if (expected && path.length - 1 + distance(p, end) > expected[0]) return;
      if (key(p) === key(end)) { const cost = [path.length - 1, turns];
        if (!expected || cost[0] < expected[0] || cost[0] === expected[0] && cost[1] < expected[1]) expected = cost; return; }
      for (const q of [start, end, ...cells]) if (distance(p, q) === 1 && !blocked.has(key(q)) && !path.some(v => key(v) === key(q)))
        walk(q, [...path, q], turns + (path.length > 1 && direction(path.at(-2), p) !== direction(p, q) ? 1 : 0));
    };
    walk(start, [start], 0); const result = shortestPath(start, end, blocked, bounds);
    if (!expected) assert.equal(result.status, 'no-path');
    else { assert.equal(result.status, 'found'); assert.deepEqual([result.steps, result.turns], expected); }
  }
});

test('directed arms, bounds and explicit work failures are retained', () => {
  const r = shortestPath(start, end, new Set(), bounds, { first: 3, last: 1 });
  assert.equal(r.status, 'found'); assert.equal(direction(r.cells[0], r.cells[1]), 3); assert.equal(direction(r.cells.at(-2), r.cells.at(-1)), 1);
  assert.equal(shortestPath(start, end, new Set(), bounds, { workBudget: 1 }).status, 'work-budget');
  assert.equal(shortestPath(start, end, new Set([key(end)]), bounds).status, 'blocked-endpoint');
  assert.throws(() => shortestPath(start, end, new Set(), bounds, { workBudget: NaN }), { code: 'path-invalid-input' });
  assert.throws(() => shortestPath(start, end, new Set(), bounds, { first: 5 }), { code: 'path-invalid-input' });
});

test('explicit zero-length paths require opt-in, free in-bounds endpoints and no directed arms', () => {
  const before = structuredClone(start);
  assert.equal(shortestPath(start, start, new Set(), bounds).status, 'same-endpoint');
  const r = shortestPath(start, start, new Set(), bounds, { allowZeroLength: true });
  assert.equal(r.status, 'found'); assert.deepEqual(r.cells, [start]);
  assert.deepEqual([r.steps, r.turns, r.expanded], [0, 0, 0]); assert.deepEqual(r.crossings, []);
  assert.deepEqual(start, before); assert.notEqual(r.cells[0], start);
  for (const arms of [{ first: 0 }, { last: 2 }, { first: 1, last: 1 }])
    assert.equal(shortestPath(start, start, new Set(), bounds, { allowZeroLength: true, ...arms }).status, 'same-endpoint');
  assert.equal(shortestPath(start, start, new Set([key(start)]), bounds, { allowZeroLength: true }).status, 'blocked-endpoint');
  const outside = { x: -1, y: 1 };
  assert.equal(shortestPath(outside, outside, new Set(), bounds, { allowZeroLength: true }).status, 'blocked-endpoint');
  assert.throws(() => shortestPath(start, start, new Set(), bounds, { allowZeroLength: 'true' }), { code: 'path-invalid-input' });
});
