import test from 'node:test';
import assert from 'node:assert/strict';
import { packShapes, outputDemands, clearEast, diverse, fillMetrics, growthCut } from './model.mjs';
import { footprint, collides } from '../constructive-cluster-packing/nesting.mjs';
const catalogs = { facilities: { facilities: [{ id: 'unit', footprint: { width: 1, height: 1 },
  ports: [{ id: 'out', position: { x: 0, y: 0 }, edge: 'east', direction: 'output', transport: 'belt' }] }] },
transports: { transports: [{ kind: 'belt', capacity: { quantity: 1, duration_ms: 1000 } }] } };
const region = (id, cells) => ({ directory: id, detail: { witness: { placements: cells.map(([x, y], i) =>
  ({ instance: `${id}-${i}`, facility: 'unit', rotation: 0, x, y, width: 1, height: 1 })), transport_networks: [], logistics_components: [] } } });

test('actual concave cells interlock instead of reserving two rectangular envelopes', () => {
  const a = region('a', [[0, 0], [0, 1], [1, 1]]), b = region('b', [[0, 0], [1, 0], [1, 1]]);
  const banks = [{ id: 'a', variants: [a] }, { id: 'b', variants: [b] }], before = JSON.stringify(banks);
  const r = packShapes({ edges: [] }, banks, catalogs);
  assert.ok(r.candidates.some(c => c.plan.partial_score[0] === 6));
  for (const c of r.candidates) {
    const [af, bf] = c.regions.map(r => footprint(r.detail.witness)), [ao, bo] = c.offsets;
    assert.equal(collides(af, bf, bo.x + bf.x - ao.x - af.x, bo.y + bf.y - ao.y - af.y), false);
  }
  assert.equal(JSON.stringify(banks), before);
  assert.deepEqual(fillMetrics(a.detail.witness), { width: 2, height: 2, area: 4, occupied_xy: 3, facility_xy: 3, fill_ratio: .75, empty_xy: 1 });
});

test('packing jointly chooses alternative shapes, including translated source origins', () => {
  const a = region('a', [[-4, 7], [-4, 8], [-3, 8]]);
  const banks = [{ id: 'a', variants: [a] }, { id: 'b', variants: [region('b', [[0, 0], [0, 1], [1, 1]]), region('b', [[5, 4], [6, 4], [6, 5]])] }];
  const r = packShapes({ edges: [] }, banks, catalogs, { beamWidth: 16, limit: 16 });
  assert.ok(r.candidates.some(c => c.plan.partial_score[0] === 6 && c.plan.choices.find(p => p.region === 'b').variant === 1));
  assert.equal(r.search.input_variants[1].count, 2);
  for (const c of r.candidates) for (const p of c.regions.flatMap((r, i) => r.detail.witness.placements.map(p => ({ x: p.x + c.offsets[i].x, y: p.y + c.offsets[i].y }))))
    assert.ok(p.x >= 0 && p.y >= 0);
});

test('east-access screen reserves only demanded ports and aggregates compatible fragments', () => {
  const source = region('a', [[0, 0]]), g = { edges: ['x', 'y'].map(target => ({ source: 'a-0', target, item: 'fixture-item', transport: 'belt', rate: { numerator: 1, denominator: 3 } })) };
  const demands = outputDemands(g, source, catalogs); assert.equal(demands[0].required, 1);
  const obstacle = footprint(region('b', [[0, 0]]).detail.witness);
  assert.equal(clearEast(obstacle, { x: 1, y: 0 }, 4, 0), false);
  assert.equal(clearEast(obstacle, { x: 1, y: 0 }, -1, 0), true);
  const banks = [{ id: 'a', variants: [source] }, { id: 'b', variants: [region('b', [[0, 0]])] }];
  const r = packShapes(g, banks, catalogs);
  assert.ok(r.search.output_obstructions > 0);
  for (const c of r.candidates) {
    const a = c.offsets.find(p => p.region === 'a'), b = c.offsets.find(p => p.region === 'b');
    assert.equal(clearEast(obstacle, { x: a.x + 1, y: a.y }, b.x, b.y), true);
  }
  source.detail.witness.transport_networks.push({ transport: 'belt', cells: [{ x: 1, y: 0 }], terminals: [{ endpoint: { kind: 'facility', instance: 'a-0', port: 'out' } }] });
  assert.equal(outputDemands(g, source, catalogs)[0].ports.length, 0);
});

test('fill counts overlapping independent transport layers once, not as extra utilization', () => {
  const w = region('a', [[0, 0]]).detail.witness;
  w.transport_networks = ['belt', 'pipe'].map(transport => ({ transport, cells: [{ x: 1, y: 0 }], terminals: [] }));
  assert.equal(fillMetrics(w).occupied_xy, 2); assert.equal(fillMetrics(w).fill_ratio, 1);
});

test('fill boundary keeps the first joined seed, treats equality as admissible and does not mutate a candidate', () => {
  const retained = region('a', [[0, 0]]).detail.witness, candidate = region('a', [[0, 0], [2, 0]]).detail.witness;
  const before = JSON.stringify({ retained, candidate });
  assert.equal(growthCut(null, candidate, .9), null);
  assert.equal(growthCut(retained, candidate, 2 / 3), null);
  assert.equal(growthCut(retained, candidate, 0), null);
  const cut = growthCut(retained, candidate, .7);
  assert.equal(cut.code, 'shape-growth-fill-cut'); assert.equal(cut.retained_fill.fill_ratio, 1);
  assert.equal(cut.candidate_fill.fill_ratio, 2 / 3); assert.equal(JSON.stringify({ retained, candidate }), before);
  assert.throws(() => growthCut(retained, candidate, NaN), { code: 'shape-fill-threshold' });
});

test('shape diversity retains a larger envelope and rejects invalid ownership/budgets', () => {
  const candidates = [[3, 2], [3, 2], [2, 4]].map(([width, height], i) => ({ width, height, area: width * height, tiles: 0, turns: 0, signature: `${i}` }));
  assert.deepEqual(diverse(candidates, 2).map(c => c.signature), ['0', '2']);
  assert.throws(() => packShapes({}, [], catalogs), { code: 'shape-bank-schema' });
  const a = region('a', [[0, 0]]), bank = { id: 'a', variants: [a] };
  assert.throws(() => packShapes({}, [bank], catalogs, { beamWidth: 0 }), { code: 'shape-search-budget' });
  assert.throws(() => packShapes({ edges: [] }, [bank, { id: 'b', variants: [a] }], catalogs), { code: 'shape-duplicate-owner' });
  assert.throws(() => packShapes({ edges: [] }, [{ id: 'a', variants: [a, region('b', [[0, 0]])] }], catalogs), { code: 'shape-variant-members' });
});
