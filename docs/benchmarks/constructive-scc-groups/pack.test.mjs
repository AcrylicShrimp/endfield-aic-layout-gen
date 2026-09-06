import test from 'node:test';
import assert from 'node:assert/strict';
import { packBanks, prepareBanks, coverageBeam, atomicUpgrades } from './pack.mjs';
import { regionInterface } from './interface.mjs';
import { footprint, collides } from '../constructive-cluster-packing/nesting.mjs';
import { witnessScore } from '../constructive-auto-interface/compact.mjs';
const catalogs = { facilities: { facilities: [{ id: 'fixture-unit', footprint: { width: 1, height: 1 }, allowed_rotations: [0],
  ports: [{ id: 'out', direction: 'output', transport: 'belt', edge: 'north', position: { x: 0, y: 0 } }] }] },
  recipes: { recipes: [{ id: 'fixture-recipe', facility: 'fixture-unit' }] }, items: { items: [] },
  transports: { transports: [{ kind: 'belt', capacity: { quantity: 1, duration_ms: 1000 } }] } };
const variant = points => ({ directory: 'fixture', detail: { witness: { schema_version: 1, placements: points.map(([instance, x, y]) =>
  ({ instance, recipe: 'fixture-recipe', facility: 'fixture-unit', rotation: 0, x, y, width: 1, height: 1 })),
  utilizations: Object.fromEntries(points.map(([id]) => [id, { numerator: 1, denominator: 1 }])), transport_networks: [], logistics_components: [] } } });
const graphOf = ids => ({ nodes: ids.map(id => ({ id, label: id, recipe: 'fixture-recipe' })), edges: [] });
const bank = (id, required_members, variants) => ({ id, required_members, variants });

test('variable membership retains stop and grown coverage, preserves ownership and caller state', () => {
  const a = bank('a', ['a'], [variant([['a', 0, 0]]), variant([['a', 0, 0], ['a2', 1, 0]])]), b = bank('b', ['b'], [variant([['b', 0, 0]])]);
  const banks = [a, b], graph = graphOf(['a', 'a2', 'b']), before = JSON.stringify(banks), r = packBanks(graph, banks, catalogs, { beamWidth: 8, limit: 8 });
  assert.ok(r.candidates.some(c => c.detail.witness.placements.length === 2)); assert.ok(r.candidates.some(c => c.detail.witness.placements.length === 3));
  for (const c of r.candidates) {
    assert.equal(new Set(c.detail.witness.placements.map(p => p.instance)).size, c.detail.witness.placements.length);
    assert.ok(c.detail.witness.placements.every(p => p.x >= 1 && p.y >= 1));
    const [a, b] = c.choices, af = a.variant.f, bf = b.variant.f;
    assert.equal(collides(af, bf, b.x + bf.x - a.x - af.x, b.y + bf.y - a.y - af.y), false);
  }
  assert.equal(JSON.stringify(banks), before);
  const overlap = structuredClone(b); overlap.variants.push(variant([['b', 0, 0], ['a2', 2, 0]]));
  assert.throws(() => prepareBanks(graph, [a, overlap], catalogs), { code: 'scc-bank-owner-overlap' });
  const missing = structuredClone(a); missing.variants[0] = variant([['a2', 0, 0]]);
  assert.throws(() => prepareBanks(graph, [missing], catalogs), { code: 'scc-variant-members' });
  graph.edges.push({ source: 'b', target: 'a2' });
  assert.throws(() => prepareBanks(graph, banks, catalogs), { code: 'scc-variant-open-input' });
});

test('concave occupied shapes interlock, including nonzero source origins', () => {
  const banks = [bank('a', ['a'], [variant([['a', 4, 7], ['a2', 4, 8], ['a3', 5, 8]])]),
    bank('b', ['b'], [variant([['b', 0, 0], ['b2', 1, 0], ['b3', 1, 1]])])];
  const r = packBanks(graphOf(['a', 'a2', 'a3', 'b', 'b2', 'b3']), banks, catalogs, { beamWidth: 16, limit: 16 });
  assert.ok(r.candidates.some(c => witnessScore(c.detail.witness).area === 6));
  assert.ok(r.candidates.every(c => footprint(c.detail.witness).x === 1));
});

test('north-side exact pending demands survive packing without an east-ray restriction', () => {
  const graph = graphOf(['a', 'b']);
  graph.edges.push({ source: 'a', target: 'external', internal: false, item: 'fixture-item', transport: 'belt', rate: { numerator: 1, denominator: 2 } });
  const r = packBanks(graph, [bank('a', ['a'], [variant([['a', 0, 0]])]), bank('b', ['b'], [variant([['b', 0, 0]])])], catalogs);
  assert.ok(r.search.output_obstructions > 0); assert.ok(r.candidates.length > 0);
  assert.ok(r.candidates.every(c => regionInterface(graph, c.detail.witness, catalogs).accepted));
});

test('coverage strata keep both extremes instead of treating a smaller partial factory as better', () => {
  const candidates = [2, 3, 4, 5, 6].map(covered => ({ covered, area: covered, width: covered, height: 1, tiles: 0, turns: 0, signature: `${covered}` }));
  assert.deepEqual(coverageBeam(candidates, 2).map(c => c.covered), [2, 6]);
  assert.deepEqual(coverageBeam([], 4), []);
  assert.throws(() => packBanks({}, [], catalogs), { code: 'scc-bank-schema' });
  assert.throws(() => packBanks({}, [], catalogs, { beamWidth: 0 }), { code: 'scc-packing-budget' });
});

test('beam selection replaces a snug cavity-closing placement with an accessible alternative', () => {
  const points = [['a', 1, 2], ['a0', 0, 0], ['a1', 2, 0], ['a2', 0, 1], ['a3', 2, 1], ['a4', 0, 2], ['a5', 2, 2]];
  const graph = graphOf([...points.map(p => p[0]), 'b']);
  graph.edges.push({ source: 'a', target: 'external', internal: false, item: 'fixture-item', transport: 'belt', rate: { numerator: 1, denominator: 2 } });
  const r = packBanks(graph, [bank('a', ['a'], [variant(points)]), bank('b', ['b'], [variant([['b', 0, 0]])])], catalogs);
  assert.ok(r.search.stages.some(s => s.boundary_rejected > 0)); assert.ok(r.candidates.length > 0);
  assert.ok(r.candidates.every(c => witnessScore(c.detail.witness).area > 9));
  assert.ok(r.candidates.every(c => regionInterface(graph, c.detail.witness, catalogs).accepted));
});

test('geometry-only atomic upgrade adds an entire dependent cycle and leaves the input choice immutable', () => {
  const graph = graphOf(['a', 'b', 'c', 'd', 'z']);
  graph.edges = [['a', 'b'], ['b', 'a'], ['b', 'c'], ['c', 'd'], ['d', 'c']].map(([source, target]) =>
    ({ source, target, internal: true, item: 'fixture-item', transport: 'belt', rate: { numerator: 1, denominator: 4 } }));
  const banks = prepareBanks(graph, [bank('ab', ['a', 'b'], [variant([['a', 0, 0], ['b', 2, 0]]), variant([['a', 0, 0], ['b', 2, 0], ['c', 0, 2], ['d', 2, 2]])]),
    bank('z', ['z'], [variant([['z', 0, 0]])])], catalogs);
  const choices = [{ region: 'ab', variant: banks[0].variants[0], x: 0, y: 0 }, { region: 'z', variant: banks[1].variants[0], x: 6, y: 0 }], before = structuredClone(choices);
  const result = atomicUpgrades(graph, choices, banks, catalogs);
  assert.deepEqual(result.atoms, [['c', 'd']]); assert.ok(result.candidates.length > 0);
  for (const c of result.candidates) {
    assert.deepEqual(c.detail.atomic_upgrade.added, ['c', 'd']); assert.equal(c.detail.witness.placements.length, 5);
    assert.deepEqual(c.choices.find(c => c.region === 'z'), choices[1]); assert.ok(regionInterface(graph, c.detail.witness, catalogs).accepted);
  }
  assert.deepEqual(choices, before);
  const missing = structuredClone(banks); missing[0].variants.pop();
  assert.equal(atomicUpgrades(graph, choices, missing, catalogs).failures[0].code, 'scc-atomic-template-missing');
  assert.throws(() => atomicUpgrades(graph, choices, banks, catalogs, 0), { code: 'scc-atomic-budget' });
});
