import { loadGraphSnapshot } from '../fixtures/constructive.mjs';
import fs from 'node:fs';
import test from 'node:test';
import assert from 'node:assert/strict';
import { boundedFlow, concreteAllocation, sourceSccs, readyAtoms, projectRegion } from './project.mjs';
import { regionInterface } from './interface.mjs';
import { normalizeGraph, extractCycles } from '../constructive-cycle-blocks/graph.mjs';
import { stripPortfolio } from '../constructive-reciprocal-strips/model.mjs';
import { directAttachments } from '../constructive-reciprocal-strips/direct.mjs';
import { applyReallocation } from '../constructive-reciprocal-strips/graph-delta.mjs';
import { compare, parseRate } from '../constructive-interconnect/rates.mjs';
import { project } from '../constructive-factory-closure/model.mjs';
const read = p => JSON.parse(fs.readFileSync(new URL(p, import.meta.url)));
const catalogs = Object.fromEntries(['recipes', 'facilities', 'items', 'transports', 'components'].map(k => [k,
  read(`../../../data/game/normalized/${k === 'components' ? 'logistics-components' : k}.json`)]));
const rational = n => ({ numerator: n, denominator: 1 });

test('rational max flow matches exhaustive cuts, conserves each vertex and respects every capacity', () => {
  const endpoints = [['s', 'a'], ['s', 'b'], ['a', 'b'], ['b', 'a'], ['a', 't'], ['b', 't']];
  for (let sample = 0; sample < 729; sample++) {
    let code = sample;
    const arcs = endpoints.map(([from, to]) => { const capacity = rational(code % 3); code = Math.floor(code / 3); return { from, to, capacity }; });
    const before = JSON.stringify(arcs), r = boundedFlow(arcs, 's', 't');
    const cuts = [0, 1, 2, 3].map(mask => {
      const own = new Set(['s', ...(mask & 1 ? ['a'] : []), ...(mask & 2 ? ['b'] : [])]);
      return arcs.filter(a => own.has(a.from) && !own.has(a.to)).reduce((s, a) => s + a.capacity.numerator, 0);
    });
    assert.equal(r.total.numerator, Math.min(...cuts)); assert.equal(r.total.denominator, 1);
    const balance = new Map(); arcs.forEach((a, i) => {
      assert.ok(r.flows[i].numerator >= 0 && compare(parseRate(r.flows[i]), parseRate(a.capacity)) <= 0);
      balance.set(a.from, (balance.get(a.from) ?? 0) - r.flows[i].numerator); balance.set(a.to, (balance.get(a.to) ?? 0) + r.flows[i].numerator);
    });
    assert.equal(balance.get('a'), 0); assert.equal(balance.get('b'), 0); assert.equal(balance.get('t'), r.total.numerator);
    assert.equal(JSON.stringify(arcs), before);
  }
  assert.deepEqual(boundedFlow([{ from: 's', to: 'a', capacity: { numerator: 2, denominator: 3 } },
    { from: 'a', to: 't', capacity: { numerator: 1, denominator: 6 } }], 's', 't').total, { numerator: 1, denominator: 6 });
  assert.throws(() => boundedFlow([], 's', 's'), { code: 'scc-flow-schema' });
});

test('source SCC discovery and frontier growth preserve an entire dependent cycle', () => {
  const graph = { nodes: ['a', 'b', 'c', 'd', 'e'].map(id => ({ id })), edges: [['a', 'b'], ['b', 'a'], ['b', 'c'], ['c', 'd'], ['d', 'c'], ['d', 'e']]
    .map(([source, target]) => ({ source, target, internal: true })) };
  assert.deepEqual(sourceSccs(graph).map(s => s.members), [['a', 'b']]);
  assert.deepEqual(readyAtoms(graph, ['a', 'b']), [['c', 'd']]);
  assert.deepEqual(readyAtoms(graph, ['a', 'b', 'c', 'd']), [['e']]);
  graph.edges.push({ source: 'external', target: 'a', internal: false }); assert.deepEqual(sourceSccs(graph), []);
});

test('concrete flow decomposition preserves terminal margins and rejects missing paths/circulation', () => {
  const n = { item: 'fixture-item', transport: 'belt', segments: [{ from: { x: 0, y: 0 }, to: { x: 1, y: 0 }, rate: rational(1) }],
    terminals: [{ id: 's', direction: 'output', endpoint: { kind: 'facility', instance: 'a' }, position: { x: 0, y: 0 }, rate: rational(1) },
      { id: 't', direction: 'input', endpoint: { kind: 'facility', instance: 'b' }, position: { x: 1, y: 0 }, rate: rational(1) }] };
  const before = JSON.stringify(n); assert.deepEqual(concreteAllocation(n), [{ source: 'a', target: 'b', item: n.item, transport: 'belt', rate: rational(1), internal: true }]);
  assert.equal(JSON.stringify(n), before);
  const broken = structuredClone(n); broken.segments = []; assert.throws(() => concreteAllocation(broken), { code: 'scc-allocation-path-missing' });
  const loop = structuredClone(n); loop.segments.push({ from: { x: 9, y: 9 }, to: { x: 9, y: 9 }, rate: rational(1) });
  assert.throws(() => concreteAllocation(loop), { code: 'scc-allocation-residual' });
  const shared = structuredClone(n); shared.segments = []; shared.terminals[1].position = { x: 0, y: 0 }; assert.equal(concreteAllocation(shared).length, 1);
});

test('actual two-member SCC projection keeps exact internal flows and leaves omitted production pending', () => {
  const original = normalizeGraph(loadGraphSnapshot(), catalogs), block = extractCycles(original, catalogs).blocks[0];
  const core = stripPortfolio(original, catalogs, block).candidates.find(d => d.reallocation && d.witness.placements.every(p => p.rotation === 0 || p.rotation === 180));
  const graph = applyReallocation(original, core.reallocation), target = block.consumers.find(id => !block.cycle.includes(id));
  const parent = directAttachments(catalogs, graph, core, target)[0], seed = sourceSccs(graph).find(s => s.members.every(id => block.cycle.includes(id)));
  assert.equal(seed.members.length, 2); const before = JSON.stringify(parent), d = projectRegion(graph, parent, seed.members, catalogs);
  assert.equal(d.witness.placements.length, 2); assert.equal(d.factory_validated, false);
  assert.equal(d.witness.transport_networks.flatMap(n => n.cells).length, 4);
  assert.ok(d.block.pending_edges.length > 0); assert.ok(d.block.pending_edges.every(e => seed.members.includes(e.source)));
  assert.ok(d.witness.transport_networks.every(n => n.terminals.every(t => seed.members.includes(t.endpoint.instance))));
  assert.equal(JSON.stringify(parent), before);
  const boundary = regionInterface(graph, d.witness, catalogs);
  assert.equal(boundary.accepted, true); assert.equal(boundary.coordinate_frame, 'local-translatable');
  assert.ok(boundary.access.every(g => g.available >= g.required));
  const blocked = structuredClone(d.witness);
  // Fill every pending connection. This synthetic negative geometry is rejected
  // before it can be mistaken for unbounded local access.
  blocked.transport_networks[0].cells.push({ x: -1, y: 0 });
  assert.throws(() => regionInterface(graph, blocked, catalogs), { code: 'scc-interface-coordinates' });
  assert.throws(() => projectRegion(graph, parent, seed.members.slice(0, 1), catalogs), { code: 'scc-open-input' });
  const bridged = structuredClone(parent); bridged.witness.logistics_components.push({ id: 'test-bridge', kind: 'bridge' });
  bridged.witness.transport_networks[0].component_ids.push('test-bridge');
  assert.throws(() => projectRegion(graph, bridged, seed.members, catalogs), { code: 'scc-bridge-projection-unsupported' });
  const next = readyAtoms(graph, seed.members).find(ids => ids.length === 1 && block.cycle.includes(ids[0]));
  const variants = directAttachments(catalogs, graph, d, next[0], { orientationPolicy: 'all' });
  assert.ok(variants.some(v => v.witness.placements.at(-1).rotation !== 90));
  assert.throws(() => directAttachments(catalogs, graph, d, next[0], { orientationPolicy: 'unknown' }), { code: 'direct-orientation-policy' });
});

test('detached output access allows translation but never body or same-layer obstructions', () => {
  const recipe = catalogs.recipes.recipes.find(r => {
    const f = catalogs.facilities.facilities.find(f => f.id === r.facility);
    return f.allowed_rotations.includes(0) && f.ports.some(p => p.transport === 'belt' && p.direction === 'output' && p.edge === 'north');
  }), facility = catalogs.facilities.facilities.find(f => f.id === recipe.facility);
  const placement = { instance: 'fixture-source', recipe: recipe.id, facility: facility.id, rotation: 0, x: 0, y: 0, ...facility.footprint };
  const witness = { placements: [placement], transport_networks: [], logistics_components: [] };
  const graph = { nodes: [], edges: [{ source: placement.instance, target: 'fixture-export', item: 'fixture-item', transport: 'belt', rate: rational(1) }] };
  const before = JSON.stringify(witness), boundary = regionInterface(graph, witness, catalogs);
  assert.ok(boundary.access[0].ports.some(p => p.connection.y === -1 && !p.obstruction));
  assert.equal(boundary.accepted, true); assert.equal(JSON.stringify(witness), before);
  const shifted = structuredClone(witness); shifted.placements[0].x += 2; shifted.placements[0].y += 2;
  const ports = project(shifted, catalogs)[placement.instance].filter(p => p.transport === 'belt' && p.direction === 'output');
  shifted.transport_networks.push({ transport: 'belt', cells: ports.map(p => p.connection), terminals: [] });
  assert.ok(regionInterface(graph, shifted, catalogs).diagnostics.some(d => d.code === 'scc-pending-output-obstructed'));
});
