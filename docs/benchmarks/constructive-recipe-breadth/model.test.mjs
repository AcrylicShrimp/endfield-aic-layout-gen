import fs from 'node:fs';
import test from 'node:test';
import assert from 'node:assert/strict';
import { graphFromWiring, externalSeed, regionsForConsumer, joinSupplierRegions, frontierDepth, inspectFutureEscape } from './model.mjs';
import { finishBoundary, inspectCoverage, routeTemplates } from '../constructive-factory-closure/model.mjs';
import { rate, dto, add, parseRate } from '../constructive-interconnect/rates.mjs';
const read = p => JSON.parse(fs.readFileSync(new URL(p, import.meta.url)));
const catalogs = Object.fromEntries(['recipes', 'facilities', 'items', 'transports', 'components'].map(k => [k, read(`../../../data/game/normalized/${k === 'components' ? 'logistics-components' : k}.json`)]));
const request = read('./requests/battery-3.json'), recipe = catalogs.recipes.recipes.find(r => r.id === request.source_selections[0].source.recipe);
const one = { numerator: 1, denominator: 1 };
function fixture(id = 'producer') {
  const amount = a => dto(rate(BigInt(a.quantity) * 1000n, recipe.duration_ms));
  return { schema_version: 1, success: true, nodes: [{ kind: 'facility', id, recipe: recipe.id, facility: recipe.facility, work_seconds_per_second: one },
    ...recipe.inputs.map((a, i) => ({ kind: 'external', id: `${id}-input-${i}`, item: a.item })),
    ...recipe.outputs.map((a, i) => ({ kind: 'target', id: `${id}-output-${i}`, item: a.item }))],
    edges: [...recipe.inputs.map((a, i) => ({ id: `${id}-in-${i}`, source: `${id}-input-${i}`, target: id, item: a.item, rate: amount(a) })),
      ...recipe.outputs.map((a, i) => ({ id: `${id}-out-${i}`, source: id, target: `${id}-output-${i}`, item: a.item, rate: amount(a) }))] };
}
function terminalReport(graph, detail) {
  // Coverage-only fixture. Independent physical validation is exercised separately by the CLI audit.
  const required = new Map(), routed = new Map(), ids = new Set(detail.witness.placements.map(p => p.instance));
  const sum = (m, key, value) => m.set(key, add(m.get(key) ?? rate(0), parseRate(value)));
  for (const e of graph.edges) {
    if (ids.has(e.source)) sum(required, JSON.stringify([e.source, 'output', e.item]), e.rate);
    if (ids.has(e.target)) sum(required, JSON.stringify([e.target, 'input', e.item]), e.rate);
  }
  for (const n of detail.witness.transport_networks) for (const t of n.terminals) if (t.endpoint.kind === 'facility')
    sum(routed, JSON.stringify([t.endpoint.instance, t.direction, n.item]), t.rate);
  return { covered_geometry_validated: true, recipe_coverage_complete: [...required].every(([id, total]) => JSON.stringify(dto(total)) === JSON.stringify(dto(routed.get(id) ?? rate(0)))),
    recipe_terminals: [...required].map(([id, total]) => {
      const [instance, direction, item] = JSON.parse(id), actual = routed.get(id) ?? rate(0), pending = rate(total.n * actual.d - actual.n * total.d, total.d * actual.d);
      return { instance, direction, item, required_rate: dto(total), routed_rate: dto(actual), pending_rate: dto(pending) };
    }) };
}
test('from-wiring graph derives exact original identities/margins without placement coordinates', () => {
  const w = fixture(), before = structuredClone(w), graph = graphFromWiring(w, catalogs);
  assert.equal(graph.nodes.length, 1); assert.deepEqual(graph.nodes[0].utilization, one); assert.ok(graph.edges.every(e => !e.internal));
  assert.ok(graph.nodes.every(n => !Object.hasOwn(n, 'x') && !Object.hasOwn(n, 'y'))); assert.deepEqual(w, before);
});
test('from-wiring graph rejects forged success, nodes, endpoint roles/items and utilization', () => {
  const w = fixture(); assert.throws(() => graphFromWiring({ ...w, success: false }, catalogs), { code: 'breadth-wiring-schema' });
  const duplicate = structuredClone(w); duplicate.nodes.push(duplicate.nodes[0]); assert.throws(() => graphFromWiring(duplicate, catalogs), { code: 'breadth-wiring-nodes' });
  for (const patch of [{ kind: 'surplus' }, { item: 'unknown-item' }, { id: 'undeclared-input' }]) {
    const bad = structuredClone(w); Object.assign(bad.nodes.find(n => n.kind === 'external'), patch);
    assert.throws(() => graphFromWiring(bad, catalogs), { code: 'breadth-wiring-endpoint' });
  }
  const bad = structuredClone(w); bad.nodes[0].work_seconds_per_second = { numerator: 1, denominator: 2 };
  assert.throws(() => graphFromWiring(bad, catalogs), { code: 'breadth-wiring-utilization' });
});
test('external seed retains only declared external demand and cannot pretend to satisfy internal inputs', () => {
  const graph = graphFromWiring(fixture(), catalogs), before = structuredClone(graph), d = externalSeed(catalogs, graph, graph.nodes[0]);
  assert.equal(d.witness.placements.length, 1); assert.equal(d.witness.transport_networks.length, 0); assert.equal(d.factory_validated, false);
  assert.deepEqual(d.pending_external_inputs, graph.edges.filter(e => e.target === 'producer').map(e => e.id)); assert.deepEqual(graph, before);
  graph.edges[0].internal = true; assert.throws(() => externalSeed(catalogs, graph, graph.nodes[0]), { code: 'breadth-seed-inputs' });
});
test('pending-external coverage is explicit, preserves default semantics and cannot validate a complete factory', () => {
  const graph = graphFromWiring(fixture(), catalogs), d = externalSeed(catalogs, graph, graph.nodes[0]), v = terminalReport(graph, d);
  assert.equal(inspectCoverage(graph, d, v).accepted, false);
  assert.equal(inspectCoverage(graph, d, v, false, { pendingExternalInputs: true }).accepted, true);
  assert.equal(inspectCoverage(graph, d, v, true).factory_validated, false);
  assert.throws(() => inspectCoverage(graph, d, v, true, { pendingExternalInputs: true }), { code: 'closure-coverage-stage' });
  const internal = structuredClone(graph); internal.edges[0].internal = true;
  assert.equal(inspectCoverage(internal, d, v, false, { pendingExternalInputs: true }).accepted, false);
  const missing = structuredClone(v); missing.recipe_terminals.pop(); assert.equal(inspectCoverage(graph, d, missing, false, { pendingExternalInputs: true }).accepted, false);
  const inflated = structuredClone(v); inflated.recipe_terminals[0].pending_rate.numerator++;
  assert.equal(inspectCoverage(graph, d, inflated, false, { pendingExternalInputs: true }).accepted, false);
});
test('full boundary routes real identities in a private translated witness with no changes to retained seed', () => {
  const graph = graphFromWiring(fixture(), catalogs), d = externalSeed(catalogs, graph, graph.nodes[0]), before = structuredClone(d), result = finishBoundary(catalogs, graph, d);
  assert.deepEqual(d, before); assert.equal(result.factory_validated, false); assert.equal(result.boundary_plan.routes.length, graph.edges.length);
  assert.equal(result.witness.placements[0].x, d.witness.placements[0].x + 1);
  const external = result.witness.transport_networks.flatMap(n => n.terminals.filter(t => t.endpoint.kind === 'external'));
  assert.ok(external.some(t => t.endpoint.side === 'west')); assert.equal(external.length, graph.edges.length);
  assert.equal(inspectCoverage(graph, result, terminalReport(graph, result), true).factory_validated, true);
  assert.throws(() => finishBoundary(catalogs, graph, result), { code: 'closure-boundary-already-realized' });
  assert.throws(() => finishBoundary(catalogs, { ...graph, nodes: [...graph.nodes, { id: 'missing' }] }, d), { code: 'closure-incomplete-membership' });
  const forged = structuredClone(result); external[0].endpoint.node = 'undeclared-external';
  assert.equal(inspectCoverage(graph, result, terminalReport(graph, result), true).factory_validated, false);
  assert.equal(inspectCoverage(graph, forged, terminalReport(graph, forged), true).factory_validated, true);
});
test('supplier joining translates only involved concrete regions and leaves them immutable', () => {
  const a = graphFromWiring(fixture('a'), catalogs), b = graphFromWiring(fixture('b'), catalogs), graph = { nodes: [...a.nodes, ...b.nodes], edges: [...a.edges, ...b.edges,
    { internal: true, source: 'a', target: 'consumer' }] };
  const regions = [{ id: 'a', detail: externalSeed(catalogs, a, a.nodes[0]) }, { id: 'b', detail: externalSeed(catalogs, b, b.nodes[0]) }], before = structuredClone(regions);
  assert.deepEqual(regionsForConsumer(graph, regions, 'consumer'), [regions[0]]);
  const joined = joinSupplierRegions(catalogs, graph, regions); assert.equal(joined.witness.placements.length, 2);
  assert.notEqual(joined.witness.placements[0].y, joined.witness.placements[1].y); assert.deepEqual(regions, before);
  assert.throws(() => regionsForConsumer(graph, [regions[1]], 'consumer'), { code: 'breadth-consumer-unready' });
  assert.equal(frontierDepth(graph, 'consumer', new Map([['a', 2]])), 3);
  assert.throws(() => frontierDepth(graph, 'consumer', new Map()), { code: 'breadth-frontier-depth' });
});

test('bounded A-star routes a multi-bend corridor that the finite template family cannot represent', () => {
  const belt = new Set();
  for (const x of [2, 4, 6]) for (let y = 0; y < 9; y++) if (y !== (x === 4 ? 0 : 8)) belt.add(`${x},${y}`);
  const occupied = { body: new Set(), layers: { belt, pipe: new Set() } }, a = { x: 0, y: 4 }, b = { x: 8, y: 4 }, extent = { width: 9, height: 9, minX: 0, minY: 0 };
  assert.equal(routeTemplates(a, b, occupied, 'belt', extent, extent, 'perimeter'), null);
  const work = { calls: 0, remaining: 10000, expanded: 0 }, p = routeTemplates(a, b, occupied, 'belt', extent, extent, 'search', work);
  assert.ok(p); assert.equal(work.calls, 1); assert.ok(work.expanded > 0 && work.remaining < 10000);
  assert.deepEqual(p.cells[0], a); assert.deepEqual(p.cells.at(-1), b); assert.ok(p.score[2] >= 6);
  for (const c of p.cells) assert.ok(!belt.has(`${c.x},${c.y}`));
  assert.equal(new Set(p.cells.map(p => `${p.x},${p.y}`)).size, p.cells.length);
  const stopped = { calls: 0, remaining: 0, expanded: 0 };
  assert.equal(routeTemplates(a, b, occupied, 'belt', extent, extent, 'search', stopped), null); assert.equal(stopped.calls, 0);
  assert.equal(routeTemplates(a, b, occupied, 'pipe', extent, extent, 'search', stopped).cells.length, 9);
});

test('future-output screen distinguishes an open exit from a same-layer enclosed port pocket', () => {
  const graph = graphFromWiring(fixture(), catalogs), d = externalSeed(catalogs, graph, graph.nodes[0]);
  assert.equal(inspectFutureEscape(catalogs, graph, d).accepted, true);
  const ports = d.catalog_ports.producer.filter(p => p.direction === 'output' && p.transport === 'belt'), x = ports[0].connection.x;
  const low = Math.min(...ports.map(p => p.connection.y)) - 1, high = Math.max(...ports.map(p => p.connection.y)) + 1;
  const cells = Array.from({ length: high - low + 1 }, (_, i) => ({ x: x + 1, y: low + i })); cells.push({ x, y: low }, { x, y: high });
  // Geometry-only obstacle isolates the screen, not a claim about network flow validity.
  d.witness.transport_networks.push({ item: 'geometry-only-ring', transport: 'belt', cells, terminals: [], segments: [] });
  const before = structuredClone(d), blocked = inspectFutureEscape(catalogs, graph, d);
  assert.equal(blocked.accepted, false); assert.equal(blocked.diagnostics[0].code, 'breadth-pending-output-escape'); assert.deepEqual(d, before);
  d.witness.transport_networks[0].transport = 'pipe'; assert.equal(inspectFutureEscape(catalogs, graph, d).accepted, true);
});
