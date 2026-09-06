import test from 'node:test';
import assert from 'node:assert/strict';
import { diagonalProduct, interleave } from './fair.mjs';

test('diagonal Cartesian traversal covers each pair exactly once and advances both axes early', () => {
  const xs = [0, 1, 2], ys = [3, 4, 5, 6], before = structuredClone([xs, ys]);
  const pairs = [...diagonalProduct(xs, ys)];
  assert.deepEqual(pairs.slice(0, 3), [[0, 3], [0, 4], [1, 3]]);
  assert.equal(pairs.length, 12); assert.equal(new Set(pairs.map(JSON.stringify)).size, 12);
  assert.deepEqual([xs, ys], before);
  assert.deepEqual([...diagonalProduct([], ys)], []); assert.deepEqual([...diagonalProduct(xs, [])], []);
});

test('round robin does not starve later pairs behind a large first domain', () => {
  const make = () => interleave([[...Array(100).keys()].values(), ['a', 'b'].values(), [].values(), ['c'].values()]);
  const stream = make(), first = Array.from({ length: 4 }, () => stream.next().value);
  assert.deepEqual(first, [{ index: 0, value: 0 }, { index: 1, value: 'a' }, { index: 3, value: 'c' }, { index: 0, value: 1 }]);
  assert.equal([...stream].length, 99);
  assert.deepEqual([...make()], [...make()]);
  assert.deepEqual([...interleave([])], []);
});
