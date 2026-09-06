import test from 'node:test';
import assert from 'node:assert/strict';
import { relativeBounds, explainRectangle, rigidFacilityGroups } from './order.mjs';

test('independent-layer equality ablation releases incidental crossings but keeps bodies rigid', () => {
  const facility = (instance, x, y) => ({ instance, x, y, width: 1, height: 1 });
  const route = (transport, instance, from, to) => ({ transport, cells: [from, to], segments: [{ from, to }],
    terminals: [{ position: from, endpoint: { kind: 'facility', instance } }] });
  const a = { x: 1, y: 0 }, b = { x: 1, y: 2 }, mid = { x: 1, y: 1 };
  const w = { placements: [facility('a', 0, 0), facility('b', 2, 2)],
    transport_networks: [route('belt', 'a', a, mid), route('pipe', 'b', b, mid)] };
  const before = structuredClone(w);
  assert.deepEqual(rigidFacilityGroups(w, 'x'), [['a', 'b']]);
  assert.deepEqual(rigidFacilityGroups(w, 'x', false), [['a'], ['b']]);
  w.transport_networks[1].transport = 'belt';
  assert.deepEqual(rigidFacilityGroups(w, 'x', false), [['a', 'b']]);
  w.transport_networks[1].transport = 'pipe'; assert.deepEqual(w, before);
  assert.throws(() => rigidFacilityGroups(w, 'x', true, 1), { code: 'order-point-budget' });
});

test('relative bounds carry actual tight paths through negative edges and zero cycles', () => {
  const c = { original: [3, 5, 2, 9, 0], edges: [{ from: 0, to: 1, weight: 2 }, { from: 1, to: 2, weight: -3 },
    { from: 2, to: 1, weight: 3 }, { from: 2, to: 3, weight: 7 }, { from: 0, to: 3, weight: 1 }] };
  const before = structuredClone(c), r = relativeBounds(c, 0);
  assert.deepEqual(r.values, [0, 2, -1, 6, -Infinity]);
  assert.equal(r.pathTo(3).reduce((s, e) => s + e.weight, 0), 6);
  assert.equal(r.pathTo(4), null); assert.deepEqual(c, before);
  assert.throws(() => relativeBounds(c, 0, 1), { code: 'order-work-budget' });
  assert.throws(() => relativeBounds({ original: [0, 0], edges: [{ from: 0, to: 1, weight: 1 }] }, 0), { code: 'order-certificate' });
});

test('a visually empty gap can be rigidly locked, order locked, or have unforced slack', () => {
  const r = { x: 1, y: 1, width: 1, height: 4 };
  const point = (y, group, offset = 0) => ({ original: { x: 1, y }, group, offset });
  const rigid = explainRectangle({ axis: 'y', original: [0], edges: [], coordinates: [point(0, 0), point(5, 0, 5)] }, r);
  assert.equal(rigid.strongest.locked, true); assert.equal(rigid.strongest.rigid, true);
  for (const weight of [1, 5]) {
    const result = explainRectangle({ axis: 'y', original: [0, 5], edges: [{ from: 0, to: 1, weight }], coordinates: [point(0, 0), point(5, 1)] }, r);
    assert.equal(result.strongest.locked, weight === 5); assert.equal(result.strongest.required_separation, weight);
  }
  const absent = explainRectangle({ axis: 'y', original: [0], edges: [], coordinates: [] }, r);
  assert.equal(absent.strongest, null);
});

test('relative bounds match exhaustive simple paths on 200 certified small graphs', () => {
  let seed = 41; const rand = n => { seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0; return seed % n; };
  for (let trial = 0; trial < 200; trial++) {
    const original = Array.from({ length: 5 }, () => rand(10)), edges = [];
    for (let from = 0; from < 5; from++) for (let to = 0; to < 5; to++) if (from !== to && rand(3) === 0)
      edges.push({ from, to, weight: original[to] - original[from] - rand(3) });
    const expected = Array(5).fill(-Infinity);
    const visit = (u, value, seen) => { expected[u] = Math.max(expected[u], value);
      for (const e of edges.filter(e => e.from === u && !seen.has(e.to))) visit(e.to, value + e.weight, new Set([...seen, e.to])); };
    visit(0, 0, new Set([0])); assert.deepEqual(relativeBounds({ original, edges }, 0).values, expected);
  }
});
