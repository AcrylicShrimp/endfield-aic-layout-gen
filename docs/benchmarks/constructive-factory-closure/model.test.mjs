import fs from 'node:fs';
import test from 'node:test';
import assert from 'node:assert/strict';
import { normalizeGraph, extractCycles } from '../constructive-cycle-blocks/graph.mjs';
import { constructCycle } from '../constructive-cycle-blocks/model.mjs';
import { composeBlocks, readyConsumers } from '../constructive-block-composition/model.mjs';
import { compactInterface, witnessScore } from '../constructive-auto-interface/compact.mjs';
import { growCandidates, ready, routeTemplates, inspectCoverage } from './model.mjs';
const read = p => JSON.parse(fs.readFileSync(new URL(p, import.meta.url)));
const catalogs = Object.fromEntries(['recipes', 'facilities', 'items', 'transports', 'components'].map(k => [k,
  read(`../../../data/game/normalized/${k === 'components' ? 'logistics-components' : k}.json`)]));
const graph = normalizeGraph(read('../constructive-chain-blocks/gap-1-placement.json'), catalogs);
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
  }
  const utilization = structuredClone(ns[0].utilization);
  result.candidates[0].witness.utilizations[ns[0].id].numerator += 1;
  assert.deepEqual(ns[0].utilization, utilization);
  assert.throws(() => growCandidates(catalogs, graph, base, graph.nodes.find(n => n.label === 'F00')), { code: 'closure-frontier-unready' });
  assert.throws(() => growCandidates(catalogs, graph, base, ns[0], 0), { code: 'closure-candidate-budget' });
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

test('coverage cannot accept a complete claim or an empty terminal report for a partial factory', () => {
  const base = baseline(), v = { covered_geometry_validated: true, recipe_coverage_complete: true, recipe_terminals: [] };
  const c = inspectCoverage(graph, base, v, true);
  assert.equal(c.accepted, false); assert.equal(c.factory_validated, false);
  assert.ok(c.diagnostics.some(d => d.code === 'closure-member-mismatch'));
  assert.ok(c.diagnostics.some(d => d.code === 'closure-missing-terminal-report'));
});
