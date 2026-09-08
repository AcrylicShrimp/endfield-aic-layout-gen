import test from 'node:test';
import assert from 'node:assert/strict';
import { movementPlan, movementIndex } from './movement.mjs';
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

test('same shifts preserve interiors, isolated pairs rejoin and ambiguous anchor collisions reject', () => {
  const w = fixture(), { chains, anchors } = extractChains(w);
  const same = movementPlan(w, chains, anchors, [{ instances: ['a'], dx: 0, dy: 3 }, { instances: ['b'], dx: 0, dy: 3 }]);
  assert.equal(same.fixedChains.size, 1); assert.equal(same.changed.size, 0);
  const inward = [{ instances: ['a'], dx: 2, dy: 0 }, { instances: ['b'], dx: -2, dy: 0 }];
  assert.deepEqual([...movementPlan(w, chains, anchors, inward).joined], [0]);
  const ambiguous = structuredClone(w); ambiguous.transport_networks[0].terminals[0].rate = { numerator: 2, denominator: 1 };
  const extracted = extractChains(ambiguous);
  assert.throws(() => movementPlan(ambiguous, extracted.chains, extracted.anchors, inward), { code: 'local-carried-collision' });
  assert.throws(() => movementPlan(w, chains, anchors, [{ instances: ['a'], dx: 0, dy: 0 }, { instances: ['a'], dx: 1, dy: 0 }]), { code: 'local-move-input' });
  assert.throws(() => movementPlan(w, chains, anchors, { instances: ['a'], dx: 0, dy: 0 }), { code: 'local-move-input' });
});

test('isolated shared zero-length owners may separate without losing endpoint identity', () => {
  const w = fixture(); w.placements[1].x = 4;
  const n = w.transport_networks[0]; n.cells = [p(3)]; n.segments = []; n.terminals[1].position = p(3);
  const { chains, anchors } = extractChains(w);
  const split = movementPlan(w, chains, anchors, [{ instances: ['a'], dx: 0, dy: 1 }, { instances: ['b'], dx: 0, dy: 2 }]);
  assert.deepEqual(split.shiftEndpoint(chains[0], false), p(3, 3));
  assert.deepEqual(split.shiftEndpoint(chains[0], true), p(3, 4));
  assert.equal(split.changed.size, 1);
  const r = movementPlan(w, chains, anchors, [{ instances: ['a'], dx: 0, dy: 1 }, { instances: ['b'], dx: 0, dy: 1 }]);
  assert.deepEqual(r.shiftAnchor('belt', p(3)), p(3, 3));
});

test('shared movement indexes isolate failed candidates and mutable plan results', () => {
  const w = fixture(), before = structuredClone(w), { chains, anchors } = extractChains(w), index = movementIndex(w, chains);
  const together = [{ instances: ['a', 'b'], dx: 0, dy: 3 }];
  const first = movementPlan(w, chains, anchors, together, { index });
  assert.equal(first.fixedChains.size, 1); assert.equal(first.changed.size, 0);
  assert.throws(() => movementPlan(w, chains, anchors, [{ instances: ['a'], dx: 6, dy: 0 }], { index }), { code: 'local-body-collision' });
  const swap = movementPlan(w, chains, anchors, [{ instances: ['a'], dx: 6, dy: 0 }, { instances: ['b'], dx: -6, dy: 0 }], { index });
  assert.deepEqual([...swap.changed], [0]); assert.equal(swap.fixedChains.size, 0);
  first.fixedChains.clear(); first.changed.add(999); first.body.clear(); first.moved.clear();
  const again = movementPlan(w, chains, anchors, together, { index });
  assert.deepEqual([...again.fixedChains.values()], [chains[0].cells.map(q => ({ x: q.x, y: q.y + 3 }))]);
  assert.equal(again.changed.size, 0); assert.equal(again.body.size, 2); assert.equal(again.moved.size, 2);
  assert.deepEqual(w, before);
  assert.throws(() => movementPlan(structuredClone(w), chains, anchors, together, { index }), { code: 'local-movement-index' });
  assert.throws(() => movementPlan(w, [...chains], anchors, together, { index }), { code: 'local-movement-index' });
});

test('indexed body intersections find both transport layers in original chain order', () => {
  const w = fixture(); w.placements.push({ ...facility('free', 5), y: 5 });
  const pipe = structuredClone(w.transport_networks[0]); pipe.id = 'pipe'; pipe.transport = 'pipe';
  for (const t of pipe.terminals) { t.id += '-pipe'; t.endpoint = { kind: 'external', node: t.id }; }
  w.transport_networks.unshift(pipe);
  const { chains, anchors } = extractChains(w), index = movementIndex(w, chains);
  const plan = movementPlan(w, chains, anchors, [{ instances: ['free'], dx: 0, dy: -3 }], { index });
  assert.deepEqual([...plan.changed], chains.map(c => c.id)); assert.equal(plan.fixedChains.size, 0);
  assert.deepEqual(chains.map(c => c.transport), ['pipe', 'belt']);
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
