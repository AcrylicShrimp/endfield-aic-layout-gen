import { loadGraphSnapshot } from '../fixtures/constructive.mjs';
import fs from 'node:fs';
import test from 'node:test';
import assert from 'node:assert/strict';
import { normalizeGraph, extractCycles } from '../constructive-cycle-blocks/graph.mjs';
import { constructCycle } from '../constructive-cycle-blocks/model.mjs';
import { composeBlocks, readyConsumers } from '../constructive-block-composition/model.mjs';
import { compactInterface, witnessScore } from '../constructive-auto-interface/compact.mjs';
import { growCandidates, ready, routeTemplates, inspectCoverage, placements, pendingOutputAccess, project } from './model.mjs';
const read = p => JSON.parse(fs.readFileSync(new URL(p, import.meta.url)));
const catalogs = Object.fromEntries(['recipes', 'facilities', 'items', 'transports', 'components'].map(k => [k,
  read(`../../../data/game/normalized/${k === 'components' ? 'logistics-components' : k}.json`)]));
const graph = normalizeGraph(loadGraphSnapshot(), catalogs);
let cached;
function baseline() {
  if (cached) return cached;
  let regions = extractCycles(graph, catalogs).blocks.map(block => ({ id: block.id, detail: compactInterface(constructCycle(catalogs, graph, block, 'root'), catalogs) }));
  for (let i = 0; i < 3; i++) {
    const targets = readyConsumers(graph, regions.flatMap(r => r.detail.witness.placements.map(p => p.instance)), catalogs);
    const portfolio = composeBlocks(catalogs, graph, regions, targets, { lanePolicy: 'portfolio' });
    const ds = portfolio.candidates.map(d => compactInterface(d, catalogs)).sort((a, b) => witnessScore(a.witness).area - witnessScore(b.witness).area);
    regions = [{ id: 'retained', detail: ds[0] }];
  }
  cached = regions[0].detail; return cached;
}

test('mixed ready set is graph-derived; first expansion preserves all retained interiors', () => {
  const base = baseline(), before = JSON.stringify(base), ns = ready(graph, base.witness);
  assert.deepEqual(ns.map(n => n.label).sort(), ['F02', 'F03', 'F09']);
  const result = growCandidates(catalogs, graph, base, ns[0], 8);
  assert.ok(result.candidates.length > 0); assert.equal(JSON.stringify(base), before);
  for (const d of result.candidates) {
    assert.equal(d.witness.placements.length, 54);
    assert.deepEqual(d.witness.placements.slice(0, 53), base.witness.placements);
    for (const n of base.witness.transport_networks) {
      const q = d.witness.transport_networks.find(m => m.id === n.id);
      assert.deepEqual(q.segments.slice(0, n.segments.length), n.segments);
      assert.deepEqual(q.terminals.slice(0, n.terminals.length), n.terminals);
    }
    const actual = d.witness.transport_networks.flatMap(n => n.terminals).filter(t => t.endpoint.kind === 'external');
    const expected = graph.edges.filter(e => !e.internal && e.target === ns[0].id);
    assert.equal(actual.length, expected.length);
    assert.ok(actual.every(t => expected.some(e => e.source === t.endpoint.node)));
    assert.equal(d.factory_validated, false);
    assert.ok(d.closure_plan.pending_output_access.every(g => g.available >= g.required));
    assert.deepEqual(d.closure_plan.pending_output_space.failures, []);
  }
  const utilization = structuredClone(ns[0].utilization);
  result.candidates[0].witness.utilizations[ns[0].id].numerator += 1;
  assert.deepEqual(ns[0].utilization, utilization);
  assert.throws(() => growCandidates(catalogs, graph, base, graph.nodes.find(n => n.label === 'F00')), { code: 'closure-frontier-unready' });
  assert.throws(() => growCandidates(catalogs, graph, base, ns[0], 0), { code: 'closure-candidate-budget' });
});

function outputFixture() {
  const nodes = ['F11', 'F17'].map(label => graph.nodes.find(n => n.label === label));
  const w = { placements: nodes.map((n, i) => {
    const facility = catalogs.recipes.recipes.find(r => r.id === n.recipe).facility;
    // This actual failing adjacency came from the density-cut native render.
    const size = catalogs.facilities.facilities.find(f => f.id === facility).footprint;
    return { instance: n.id, facility, recipe: n.recipe, rotation: 90, x: i ? 61 : 58, y: i ? 0 : 3,
      width: size.height, height: size.width };
  }), transport_networks: [], logistics_components: [] };
  return { nodes, w };
}

test('actual F11 adjacency rejects all three output cells inside F17 without requiring all unused ports', () => {
  const { nodes, w } = outputFixture(), before = JSON.stringify(w);
  const a = pendingOutputAccess(graph, w, catalogs).find(g => g.instance === nodes[0].id);
  assert.equal(a.available, 0); assert.equal(a.required, 1);
  assert.deepEqual(a.ports.map(p => p.connection), [{ x: 61, y: 3 }, { x: 61, y: 4 }, { x: 61, y: 5 }]);
  assert.ok(a.ports.every(p => p.obstruction === 'facility-body')); assert.equal(JSON.stringify(w), before);
  w.placements[0].y = w.placements[1].height - 1;
  const b = pendingOutputAccess(graph, w, catalogs).find(g => g.instance === nodes[0].id);
  assert.equal(b.available, 2); assert.ok(b.available >= b.required);
});

test('pending access subtracts routed margins, aggregates same-item fragments and keeps different items separate', () => {
  const { nodes, w } = outputFixture(); w.placements.pop();
  const edge = graph.edges.find(e => e.source === nodes[0].id), cap = catalogs.transports.transports.find(t => t.kind === 'belt').capacity;
  const amount = { numerator: cap.quantity * 1000, denominator: cap.duration_ms };
  const g = { edges: [0, 1].map(i => ({ ...edge, id: `fragment-${i}`, rate: { numerator: amount.numerator, denominator: amount.denominator * 2 } })) };
  assert.equal(pendingOutputAccess(g, w, catalogs)[0].required, 1);
  g.edges[1].item = 'distinct-test-commodity'; assert.equal(pendingOutputAccess(g, w, catalogs)[0].required, 2);
  g.edges[1].item = edge.item;
  w.transport_networks.push({ item: edge.item, transport: 'belt', cells: [], terminals: [{ direction: 'output', rate: amount,
    endpoint: { kind: 'facility', instance: nodes[0].id, port: 'output-belt-0' } }] });
  assert.deepEqual(pendingOutputAccess(g, w, catalogs), []);
  w.transport_networks[0].terminals[0].rate = { numerator: amount.numerator * 2, denominator: amount.denominator };
  assert.throws(() => pendingOutputAccess(g, w, catalogs), { code: 'closure-output-margin-exceeded' });
});

test('output access respects used ports, transport layers, component cells and negative coordinates', () => {
  const { nodes, w } = outputFixture(); w.placements.pop();
  const connections = project(w, catalogs)[nodes[0].id].filter(p => p.direction === 'output').map(p => p.connection);
  w.transport_networks.push({ item: 'test-liquid', transport: 'pipe', cells: connections, terminals: [] });
  assert.equal(pendingOutputAccess(graph, w, catalogs)[0].available, 3);
  w.logistics_components.push({ transport: 'belt', position: connections[0] });
  assert.equal(pendingOutputAccess(graph, w, catalogs)[0].available, 2);
  w.transport_networks.push({ item: 'test-item', transport: 'belt', cells: [connections[1]], terminals: [] });
  assert.equal(pendingOutputAccess(graph, w, catalogs)[0].available, 1);
  const ports = project(w, catalogs); ports[nodes[0].id].find(p => p.id === 'output-belt-2').connection.y = -1;
  const a = pendingOutputAccess(graph, w, catalogs, ports)[0]; assert.equal(a.available, 0);
  assert.equal(a.ports.at(-1).obstruction, 'negative-coordinate');
});

test('corridor templates respect layers, bodies, fixed output reservations and one-cell interfaces', () => {
  const occupied = { body: new Set(), layers: { belt: new Set(['2,2']), pipe: new Set() } }, extent = { width: 6, height: 6 };
  const a = { x: 1, y: 2 }, b = { x: 3, y: 2 };
  assert.equal(routeTemplates(a, b, occupied, 'pipe', extent).cells.length, 3);
  assert.ok(routeTemplates(a, b, occupied, 'belt', extent).cells.length > 3);
  occupied.body.add('1,2'); assert.equal(routeTemplates(a, b, occupied, 'pipe', extent), null);
  occupied.body.clear(); assert.equal(routeTemplates(a, a, occupied, 'pipe', extent).cells.length, 1);
  assert.equal(routeTemplates(a, { x: 7, y: 2 }, occupied, 'pipe', extent, extent), null);
});

test('two-sided placement includes every forward slot, preserves orientation and rejects all occupied geometry', () => {
  const base = baseline(), original = JSON.stringify(base); let additional = 0;
  for (const node of ready(graph, base.witness)) {
    const forward = placements(node, graph, base, catalogs), expanded = placements(node, graph, base, catalogs, 'two-sided');
    assert.deepEqual(forward, placements(node, graph, base, catalogs, 'forward'));
    const ids = new Set(expanded.map(p => JSON.stringify(p)));
    for (const p of forward) assert.ok(ids.has(JSON.stringify(p)));
    additional += expanded.length - forward.length;
    assert.equal(ids.size, expanded.length);
    for (const p of expanded) {
      assert.ok(p.x >= 0 && p.y >= 1); assert.equal(p.rotation, forward[0].rotation);
      assert.ok(base.witness.placements.every(q => p.x + p.width <= q.x || q.x + q.width <= p.x || p.y + p.height <= q.y || q.y + q.height <= p.y));
      assert.ok(base.witness.transport_networks.flatMap(n => n.cells).every(q => q.x < p.x || q.x >= p.x + p.width || q.y < p.y || q.y >= p.y + p.height));
    }
  }
  assert.ok(additional > 0); assert.equal(JSON.stringify(base), original);
  assert.throws(() => placements(ready(graph, base.witness)[0], graph, base, catalogs, 'unknown'), { code: 'closure-placement-family' });
});

test('coverage cannot accept a complete claim or an empty terminal report for a partial factory', () => {
  const base = baseline(), v = { covered_geometry_validated: true, recipe_coverage_complete: true, recipe_terminals: [] };
  const c = inspectCoverage(graph, base, v, true);
  assert.equal(c.accepted, false); assert.equal(c.factory_validated, false);
  assert.ok(c.diagnostics.some(d => d.code === 'closure-member-mismatch'));
  assert.ok(c.diagnostics.some(d => d.code === 'closure-missing-terminal-report'));
});

test('perimeter transfer approaches a north input around its body without weakening collisions', () => {
  const occupied = { body: new Set(['1,6', '3,6', '2,5', ...Array.from({ length: 6 }, (_, i) => `12,${i + 1}`)]), layers: { belt: new Set(), pipe: new Set() } };
  const a = { x: 2, y: 6 }, b = { x: 12, y: 0 }, extent = { width: 15, height: 8 };
  assert.equal(routeTemplates(a, b, occupied, 'pipe', extent), null);
  const p = routeTemplates(a, b, occupied, 'pipe', extent, null, 'perimeter'); assert.ok(p);
  assert.equal(new Set(p.cells.map(p => `${p.x},${p.y}`)).size, p.cells.length);
  assert.ok(p.cells.every(p => !occupied.body.has(`${p.x},${p.y}`)));
  occupied.layers.pipe.add('2,6'); assert.equal(routeTemplates(a, b, occupied, 'pipe', extent, null, 'perimeter'), null);
  assert.throws(() => routeTemplates(a, b, occupied, 'pipe', extent, null, 'unknown'), { code: 'closure-route-family' });
});

test('append-only drafts support frozen inputs and return fully detached successful object graphs', () => {
  const objects = value => { const seen = new Set(); const visit = o => {
    if (!o || typeof o !== 'object' || seen.has(o)) return; seen.add(o); Object.values(o).forEach(visit);
  }; visit(value); return seen; };
  const frozen = value => { for (const o of objects(value)) Object.freeze(o); return value; };
  const base = frozen(structuredClone(baseline())), cats = frozen(structuredClone(catalogs)), g = frozen(structuredClone(graph));
  const node = ready(g, base.witness)[0], original = JSON.stringify(base);
  const result = growCandidates(cats, g, base, node, 64, 'simple', 'two-sided');
  assert.ok(result.candidates.length >= 2);
  assert.equal(result.copying.draft_count, result.trials);
  assert.equal(result.copying.materialized_count, result.candidates.length);
  const forbidden = objects({ base, cats, g });
  for (const c of result.candidates) for (const o of objects(c)) {
    assert.equal(forbidden.has(o), false); forbidden.add(o);
  }
  const second = JSON.stringify(result.candidates[1]);
  const first = result.candidates[0];
  first.witness.transport_networks[0].segments[0].rate.numerator++;
  first.witness.transport_networks[0].terminals[0].endpoint.instance = 'mutated';
  first.witness.logistics_components[0].position.x++;
  first.witness.placements[0].x++; first.witness.utilizations[node.id].numerator++;
  Object.values(first.catalog_ports)[0][0].connection.x++;
  assert.equal(JSON.stringify(result.candidates[1]), second); assert.equal(JSON.stringify(base), original);

  const broken = structuredClone(g);
  for (const e of broken.edges.filter(e => e.target === node.id)) e.rate = { numerator: 1000, denominator: 1 };
  const failed = growCandidates(cats, frozen(broken), base, broken.nodes.find(n => n.id === node.id), 4, 'simple', 'two-sided');
  assert.equal(failed.candidates.length, 0); assert.equal(failed.copying.materialized_count, 0);
  assert.ok(failed.trials > 0); assert.ok(failed.failures.every(f => f.code === 'closure-port-template-exhausted'));
  assert.equal(JSON.stringify(base), original);
});
