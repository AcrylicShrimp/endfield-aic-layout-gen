import test from 'node:test';
import assert from 'node:assert/strict';
import { movementPlan } from './movement.mjs';
import { extractChains } from './model.mjs';
import { closeTerminalGroups, compoundCandidates } from './compound.mjs';
const rate = { numerator: 1, denominator: 1 };
const p = (x, y = 2) => ({ x, y });
const facility = (instance, x) => ({ instance, x, y: 2, width: 1, height: 1 });
function fixture() {
  const cells = [3, 4, 5, 6, 7].map(x => p(x));
  return { placements: [facility('a', 2), facility('b', 8)], logistics_components: [],
    transport_networks: [{ id: 'connection', item: 'fixture-only', transport: 'belt', cells, component_ids: [],
      segments: cells.slice(1).map((to, i) => ({ from: cells[i], to, rate })),
      terminals: [{ id: 'a-out', direction: 'output', position: cells[0], endpoint: { kind: 'facility', instance: 'a' }, rate },
        { id: 'b-in', direction: 'input', position: cells.at(-1), endpoint: { kind: 'facility', instance: 'b' }, rate }] }] };
}

test('simultaneous exchange succeeds when either sequential move collides, rerouting the interface', () => {
  const w = fixture(), before = structuredClone(w), { chains, anchors } = extractChains(w);
  const moves = [{ instances: ['a'], dx: 6, dy: 0 }, { instances: ['b'], dx: -6, dy: 0 }];
  for (const m of moves) assert.throws(() => movementPlan(w, chains, anchors, [m]), { code: 'local-body-collision' });
  const r = movementPlan(w, chains, anchors, moves);
  assert.deepEqual(r.shiftFacility(w.placements[0]), p(8)); assert.deepEqual(r.shiftFacility(w.placements[1]), p(2));
  assert.deepEqual(r.shiftAnchor('belt', p(3)), p(9)); assert.deepEqual(r.shiftAnchor('belt', p(7)), p(1));
  assert.equal(r.fixedChains.size, 0); assert.deepEqual([...r.changed], [0]); assert.deepEqual(w, before);
});

test('same shifts preserve interiors, conflicting moved anchors and overlapping membership reject', () => {
  const w = fixture(), { chains, anchors } = extractChains(w);
  const same = movementPlan(w, chains, anchors, [{ instances: ['a'], dx: 0, dy: 3 }, { instances: ['b'], dx: 0, dy: 3 }]);
  assert.equal(same.fixedChains.size, 1); assert.equal(same.changed.size, 0);
  assert.throws(() => movementPlan(w, chains, anchors, [{ instances: ['a'], dx: 2, dy: 0 }, { instances: ['b'], dx: -2, dy: 0 }]), { code: 'local-carried-collision' });
  assert.throws(() => movementPlan(w, chains, anchors, [{ instances: ['a'], dx: 0, dy: 0 }, { instances: ['a'], dx: 1, dy: 0 }]), { code: 'local-move-input' });
  assert.throws(() => movementPlan(w, chains, anchors, { instances: ['a'], dx: 0, dy: 0 }), { code: 'local-move-input' });
});

test('shared zero-length owners must receive equal translations even in separate primitives', () => {
  const w = fixture(); w.placements[1].x = 4;
  const n = w.transport_networks[0]; n.cells = [p(3)]; n.segments = []; n.terminals[1].position = p(3);
  const { chains, anchors } = extractChains(w);
  assert.throws(() => movementPlan(w, chains, anchors, [{ instances: ['a'], dx: 0, dy: 1 }, { instances: ['b'], dx: 0, dy: 2 }]), { code: 'local-shared-terminal-move' });
  const r = movementPlan(w, chains, anchors, [{ instances: ['a'], dx: 0, dy: 1 }, { instances: ['b'], dx: 0, dy: 1 }]);
  assert.deepEqual(r.shiftAnchor('belt', p(3)), p(3, 3));
});

test('candidate groups close shared terminals transitively and exclude fixed external ownership', () => {
  const w = fixture(); w.placements.push(facility('c', 12));
  const n = w.transport_networks[0]; n.terminals[1].position = p(3);
  n.terminals.push({ position: p(10), endpoint: { kind: 'facility', instance: 'b' } }, { position: p(10), endpoint: { kind: 'facility', instance: 'c' } });
  const before = structuredClone(w);
  assert.deepEqual(closeTerminalGroups(w, []), [{ instances: ['a', 'b', 'c'], kind: 'terminal-closed' }]);
  assert.deepEqual(closeTerminalGroups(w, [], 2), []); assert.deepEqual(w, before);
  const reversed = structuredClone(w); reversed.placements.reverse(); reversed.transport_networks[0].terminals.reverse();
  assert.deepEqual(closeTerminalGroups(reversed, []), closeTerminalGroups(w, []));
  n.terminals.push({ position: p(10), endpoint: { kind: 'external' } });
  assert.deepEqual(closeTerminalGroups(w, []), []);
});

test('compound proposal budgets are explicit and candidate generation leaves its inputs intact', () => {
  const w = fixture(); w.placements.push(facility('c', 12));
  const d = { witness: w, labels: { a: 'A', b: 'B', c: 'C' } }, before = structuredClone(d);
  const graph = { nodes: w.placements.map(p => ({ id: p.instance, recipe: 'geometry-only' })), edges: [] }, catalogs = { recipes: { recipes: [] } };
  const r = compoundCandidates(d, graph, catalogs, { limit: 2, pairBudget: 1 });
  assert.equal(r.pairs_considered, 1); assert.equal(r.pair_budget_exhausted, true);
  assert.ok(r.candidates.length <= 2); assert.deepEqual(d, before);
  assert.deepEqual(compoundCandidates(d, graph, catalogs, { limit: 2, pairBudget: 1 }), r);
  assert.throws(() => compoundCandidates(d, graph, catalogs, { pairBudget: 0 }), { code: 'compound-budget' });
});
