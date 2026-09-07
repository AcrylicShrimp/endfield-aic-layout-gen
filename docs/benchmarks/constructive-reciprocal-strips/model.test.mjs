import { loadGraphSnapshot } from '../fixtures/constructive.mjs';
import fs from 'node:fs';
import test from 'node:test';
import assert from 'node:assert/strict';
import { stripCandidates, reciprocalCore, stripPortfolio } from './model.mjs';
import { compatibleAllocations } from './allocation.mjs';
import { normalizeGraph, extractCycles } from '../constructive-cycle-blocks/graph.mjs';
import { inspectBlockCoverage } from '../constructive-cycle-blocks/model.mjs';
import { rate, parseRate, add, multiply, dto } from '../constructive-interconnect/rates.mjs';
import { witnessScore } from '../constructive-auto-interface/compact.mjs';
import { distance, key } from '../constructive-local-reembedding/path.mjs';
const read = p => JSON.parse(fs.readFileSync(new URL(p, import.meta.url)));
const catalogs = Object.fromEntries(['recipes', 'facilities', 'items', 'transports', 'components'].map(k => [k, read(`../../../data/game/normalized/${k === 'components' ? 'logistics-components' : k}.json`)]));
const graph = normalizeGraph(loadGraphSnapshot(), catalogs), blocks = extractCycles(graph, catalogs).blocks;
const freeze = x => { if (x && typeof x === 'object' && !Object.isFrozen(x)) { Object.freeze(x); Object.values(x).forEach(freeze); } return x; };
const snapshot = structuredClone({ graph, catalogs, blocks }); freeze(graph); freeze(catalogs); freeze(blocks);
const result = stripCandidates(graph, catalogs, blocks[0]);

test('all orders, catalog rotations and strip axes discover compact reciprocal geometry', () => {
  assert.equal(result.layouts, 6 * 4 ** 3 * 2);
  assert.equal(result.enumeration_complete, true); assert.ok(result.candidates.length > 0);
  assert.equal(result.rejected + result.candidates.length, result.layouts);
  for (const d of result.candidates) {
    assert.equal(d.factory_validated, false); assert.equal(d.witness.placements.length, 3);
    assert.equal(witnessScore(d.witness).area, 105); assert.equal(witnessScore(d.witness).physical_tiles, 8);
    assert.equal(d.witness.logistics_components.length, 0);
    const usedPorts = [], cells = [];
    for (const n of d.witness.transport_networks) {
      assert.equal(n.cells.length, n.segments.length * 2);
      n.segments.forEach(s => assert.equal(distance(s.from, s.to), 1));
      n.terminals.forEach(t => usedPorts.push(JSON.stringify([t.endpoint.instance, t.endpoint.port])));
      cells.push(...n.cells.map(p => `${n.transport}:${key(p)}`));
    }
    assert.equal(new Set(usedPorts).size, usedPorts.length);
    assert.equal(new Set(cells).size, cells.length);
    const ordered = [...d.witness.placements].sort((a, b) => a[d.strip.axis] - b[d.strip.axis]);
    assert.equal(ordered[0].rotation, ordered[2].rotation);
    assert.equal((ordered[0].rotation + 180) % 360, ordered[1].rotation);
  }
  assert.deepEqual({ graph, catalogs, blocks }, snapshot);
});

test('exact outgoing cuts remain pending and larger reciprocal core is explicit', () => {
  const core = reciprocalCore(graph, blocks[0]);
  assert.ok(core.pending_edges.length > 0);
  assert.ok(core.pending_edges.every(e => core.members.includes(e.source) && !core.members.includes(e.target)));
  assert.equal(core.pending_edges.length, 2);
  assert.throws(() => stripCandidates(graph, catalogs, blocks.at(-1)), { code: 'strip-member-bound' });
  assert.throws(() => stripCandidates(graph, catalogs, blocks[0], { assignmentBudget: 0 }), { code: 'strip-budget-input' });
  const exhausted = stripCandidates(graph, catalogs, blocks[0], { assignmentBudget: 1 });
  assert.equal(exhausted.enumeration_complete, false); assert.equal(exhausted.candidates.length, 0);
  assert.ok(exhausted.diagnostics.every(d => d.code === 'strip-assignment-budget'));
});

test('runtime capacity is enforced, and facility names are not a pattern rule', () => {
  const low = structuredClone(catalogs); low.transports.transports.find(t => t.kind === 'belt').capacity.duration_ms *= 2;
  assert.equal(stripCandidates(graph, low, blocks[0]).candidates.length, 0);
  const renamed = structuredClone(catalogs), ids = new Map(renamed.facilities.facilities.map((f, i) => [f.id, `renamed-facility-${i}`]));
  renamed.facilities.facilities.forEach(f => { f.id = ids.get(f.id); });
  renamed.recipes.recipes.forEach(r => { r.facility = ids.get(r.facility); });
  const other = stripCandidates(graph, renamed, blocks[0]);
  assert.deepEqual(other.candidates.map(d => witnessScore(d.witness)), result.candidates.map(d => witnessScore(d.witness)));
  other.candidates[0].witness.utilizations[result.core.members[0]].numerator++;
  assert.deepEqual({ graph, catalogs, blocks }, snapshot);
});

test('coverage rejects hidden outgoing margins, changed utilization and missing terminal reports', () => {
  // Coverage-only unit test; the experiment CLI independently validates physical geometry.
  const d = result.candidates[0], terminals = [];
  for (const p of d.witness.placements) {
    const recipe = catalogs.recipes.recipes.find(r => r.id === p.recipe);
    for (const direction of ['input', 'output']) for (const a of recipe[direction + 's']) {
      const required = multiply(rate(BigInt(a.quantity) * 1000n, recipe.duration_ms), parseRate(d.witness.utilizations[p.instance]));
      const routed = d.witness.transport_networks.filter(n => n.item === a.item).flatMap(n => n.terminals)
        .filter(t => t.endpoint.instance === p.instance && t.direction === direction).reduce((s, t) => add(s, parseRate(t.rate)), rate(0));
      terminals.push({ instance: p.instance, item: a.item, direction, required_rate: dto(required), routed_rate: dto(routed),
        pending_rate: dto(rate(required.n * routed.d - routed.n * required.d, required.d * routed.d)) });
    }
  }
  const v = { covered_geometry_validated: true, recipe_terminals: terminals };
  assert.equal(inspectBlockCoverage(d, v, catalogs).accepted, true);
  const hidden = structuredClone(d); hidden.block.pending_edges = [];
  assert.equal(inspectBlockCoverage(hidden, v, catalogs).accepted, false);
  const altered = structuredClone(d); altered.witness.utilizations[altered.block.members[0]].denominator *= 2;
  assert.equal(inspectBlockCoverage(altered, v, catalogs).accepted, false);
  assert.equal(inspectBlockCoverage(d, { ...v, recipe_terminals: terminals.slice(1) }, catalogs).accepted, false);
});

test('compatible reallocation preserves every whole-graph commodity margin and outgoing consumer', () => {
  const margins = g => {
    const m = new Map();
    for (const e of g.edges) for (const [direction, node] of [['output', e.source], ['input', e.target]]) {
      const k = JSON.stringify([direction, node, e.item, e.transport]); m.set(k, add(m.get(k) ?? rate(0), parseRate(e.rate)));
    }
    return [...m].sort(([a], [b]) => a.localeCompare(b)).map(([k, v]) => [k, dto(v)]);
  };
  const plans = compatibleAllocations(graph, blocks[0]); assert.equal(plans.length, 2);
  for (const p of plans) {
    assert.deepEqual(margins(p.graph), margins(graph));
    assert.equal(p.reallocation.replaced_edge_ids.length, 4);
    assert.equal(p.reallocation.replacement_edges.length, 2);
    assert.equal(new Set(p.graph.edges.map(e => e.id)).size, p.graph.edges.length);
    const ids = new Set(p.reallocation.replaced_edge_ids);
    assert.deepEqual(p.graph.edges.filter(e => !p.reallocation.replacement_edges.includes(e)), graph.edges.filter(e => !ids.has(e.id)));
  }
  assert.deepEqual({ graph, catalogs, blocks }, snapshot);
});

test('three-connection core beats split supply at equal area without hiding downstream demand', () => {
  const p = stripPortfolio(graph, catalogs, blocks[0]);
  const best = p.candidates.filter(d => witnessScore(d.witness).physical_tiles === 6);
  assert.ok(best.length > 0); assert.ok(p.candidates.some(d => witnessScore(d.witness).physical_tiles === 8));
  for (const d of best) {
    assert.equal(witnessScore(d.witness).area, 105);
    assert.equal(d.witness.transport_networks.reduce((n, t) => n + t.segments.length, 0), 3);
    assert.equal(d.block.pending_edges.length, 1);
    assert.equal(d.reallocation.replacement_edges.length, 2);
    assert.equal(d.factory_validated, false);
  }
});
