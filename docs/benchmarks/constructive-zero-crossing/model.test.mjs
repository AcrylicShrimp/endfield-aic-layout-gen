import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { inversions, solveOrder, validateGeometry, analyzeZeroCrossing } from './model.mjs';
import { redistribute } from './redistribute.mjs';
const edge = (source, target, item = 'fixture') => ({ id: source + target, source, target, item });
test('two reversed independent edges are solved, not removed or assigned a penalty', () => {
  const edges = [edge('a', 'y'), edge('b', 'x')], before = inversions(edges, ['a', 'b'], ['x', 'y']);
  assert.equal(before.length, 1);
  const result = solveOrder(edges, ['a', 'b'], ['x', 'y']);
  assert.equal(result.status, 'zero-inversions'); assert.equal(result.inversion_pairs.length, 0);
  assert.equal(edges.length, 2);
});
test('fixed-pair K2,2 remains an ordering obstruction, not a commodity impossibility', () => {
  const result = solveOrder(['a', 'b'].flatMap(a => ['x', 'y'].map(b => edge(a, b))), ['a', 'b'], ['x', 'y']);
  assert.equal(result.code, 'zero-crossing-explicit-junction-required');
  assert.equal(result.witness.requirements.length, 4);
  assert.equal(result.witness.items.length, 1);
});
test('source-order budget produces unknown instead of a false obstruction proof', () => {
  const edges = [edge('a', 'x'), edge('b', 'y'), edge('c', 'x')];
  assert.equal(solveOrder(edges, ['a', 'b', 'c'], ['x', 'y'], 1).code, 'zero-crossing-order-budget');
  assert.equal(solveOrder(edges, ['a', 'b', 'c'], ['x', 'y'], 20).status, 'zero-inversions');
  assert.throws(() => solveOrder(edges, ['a', 'b', 'c'], ['x', 'y'], 0), /budget/);
});
test('tile checker rejects crossings even for compatible items, jumps and facility intrusion', () => {
  const lane = cells => ({ id: 'a', start: cells[0], end: cells.at(-1), cells });
  const a = lane([{ x: 0, y: 1 }, { x: 1, y: 1 }, { x: 2, y: 1 }]);
  const b = lane([{ x: 1, y: 0 }, { x: 1, y: 1 }, { x: 1, y: 2 }]);
  assert.equal(validateGeometry([], [a]).valid, true);
  assert(validateGeometry([], [a, b]).issues.includes('shared-or-crossing-tile'));
  assert(validateGeometry([{ x: 1, y: 1, width: 1, height: 1 }], [a]).issues.includes('facility-intrusion'));
  assert(validateGeometry([], [lane([{ x: 0, y: 0 }, { x: 2, y: 0 }])]).issues.includes('non-unit-step'));
});
test('full graph accounts for every requirement and never opens routing on local success', () => {
  const load = name => JSON.parse(fs.readFileSync(new URL(name, import.meta.url)));
  const p = load('../constructive-chain-blocks/gap-6-placement.json');
  const r = analyzeZeroCrossing(p, load('../../../data/game/normalized/items.json'), load('../../../data/game/normalized/transports.json'));
  assert.equal(r.routing_allowed, false); assert.equal(r.global_zero_crossing_certified, false);
  assert.equal(r.gate.unresolved.length, 96); assert.equal(new Set(r.gate.unresolved.map(e => e.requirement)).size, 96);
  assert.equal(r.summary.locally_certified, 12); assert.equal(r.summary.local_lanes, 52);
  const c = r.corridors.find(c => c.id === '42:47');
  assert.equal(c.original.inversion_pairs.length, 5); assert.equal(c.certificate.lane_count, 12);
  assert.equal(validateGeometry(c.certificate.placements, c.certificate.lanes).valid, true);
  assert.equal(r.corridors.find(c => c.order.witness).edges.length, 10);
});
test('joint solver witness preserves all facilities and resolves all twelve selected orders simultaneously', () => {
  const load = name => JSON.parse(fs.readFileSync(new URL(name, import.meta.url)));
  const p = load('../constructive-chain-blocks/gap-6-placement.json'), oracle = load('./order-slice.json');
  const items = load('../../../data/game/normalized/items.json'), transports = load('../../../data/game/normalized/transports.json');
  const r = analyzeZeroCrossing(p, items, transports, 100000, oracle);
  assert.equal(r.composition.status, 'joint-sat-order-applied');
  const solved = r.corridors.filter(c => c.composed.status === 'ordered');
  assert.equal(solved.length, 12); assert(solved.every(c => c.composed.inversion_pairs.length === 0));
  assert.equal(r.composition.candidate.placements.length, 59);
  assert.deepEqual(r.composition.candidate.internal_requirements, p.internal_requirements);
  assert.deepEqual(r.composition.candidate.external_requirements, p.external_requirements);
  assert.equal(r.routing_allowed, false);
  const broken = structuredClone(oracle), lane = broken.corridors.find(c => c.lanes.length >= 2).lanes[0];
  lane.source_port = 'not-a-catalog-port';
  assert.throws(() => analyzeZeroCrossing(p, items, transports, 100000, broken), /checker rejected/);
});
test('transportation margins remove a same-item K2,2 without retaining arbitrary pair flows', () => {
  const edges = ['a', 'b'].flatMap(source => ['x', 'y'].map(target => ({ source, target, item: 'fixture', transport: 'belt', rate: { numerator: 1, denominator: 4 } })));
  const r = redistribute(edges, ['a', 'b'], ['x', 'y']);
  assert.deepEqual(r.flows.map(e => [e.source, e.target, e.rate]), [['a', 'x', { numerator: 1, denominator: 2 }], ['b', 'y', { numerator: 1, denominator: 2 }]]);
  assert.equal(r.terminal_rates_preserved, true); assert.equal(r.physical_routes_validated, false);
  assert.throws(() => redistribute(edges, ['a'], ['x', 'y']), /membership/);
});
