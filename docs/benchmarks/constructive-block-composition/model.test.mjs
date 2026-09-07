import { loadGraphSnapshot } from '../fixtures/constructive.mjs';
import fs from 'node:fs';
import test from 'node:test';
import assert from 'node:assert/strict';
import { normalizeGraph, extractCycles } from '../constructive-cycle-blocks/graph.mjs';
import { constructCycle } from '../constructive-cycle-blocks/model.mjs';
import { composeBlocks, readyConsumers } from './model.mjs';
import { compactInterface, witnessScore } from '../constructive-auto-interface/compact.mjs';
import { rate, add, parseRate, dto } from '../constructive-interconnect/rates.mjs';
const read = p => JSON.parse(fs.readFileSync(new URL(p, import.meta.url)));
const catalogs = Object.fromEntries(['recipes', 'facilities', 'items', 'transports', 'components'].map(k => [k,
  read(`../../../data/game/normalized/${k === 'components' ? 'logistics-components' : k}.json`)]));
const graph = normalizeGraph(loadGraphSnapshot(), catalogs), inventory = extractCycles(graph, catalogs);
const id = label => graph.nodes.find(n => n.label === label).id;
const regions = inventory.blocks.map(b => ({ id: b.id, detail: compactInterface(constructCycle(catalogs, graph, b, 'root'), catalogs) }));
function inputs(targets) {
  const suppliers = new Set(graph.edges.filter(e => targets.includes(e.target)).map(e => e.source));
  return regions.filter(r => r.detail.witness.placements.some(p => suppliers.has(p.instance)));
}
const sum = xs => dto(xs.reduce((s, r) => add(s, parseRate(r)), rate(0)));

test('nine-facility composition preserves every interior and leaves the shared supplier remainder pending', () => {
  const source = inputs([id('F28')]), original = JSON.stringify(source), d = composeBlocks(catalogs, graph, source, [id('F28')]);
  assert.equal(JSON.stringify(source), original); assert.equal(d.witness.placements.length, 9);
  assert.equal(new Set(d.witness.placements.map(p => p.instance)).size, 9);
  assert.deepEqual(sum(d.block.pending_edges.filter(e => e.source === id('F42')).map(e => e.rate)), { numerator: 1, denominator: 2 });
  assert.equal(d.witness.transport_networks.some(n => n.terminals.some(t => t.endpoint.kind === 'external')), false);
  for (const r of source) {
    const offset = d.composition_plan.regions.find(t => t.region === r.id);
    for (const p of r.detail.witness.placements) assert.deepEqual(d.witness.placements.find(q => q.instance === p.instance), { ...p, x: p.x + offset.x, y: p.y + offset.y });
    for (const n of r.detail.witness.transport_networks) {
      const current = d.witness.transport_networks.find(m => m.item === n.item);
      const edges = new Map(current.segments.map(s => [JSON.stringify([s.from, s.to]), s.rate]));
      for (const s of n.segments) assert.deepEqual(edges.get(JSON.stringify([{ x: s.from.x + offset.x, y: s.from.y + offset.y },
        { x: s.to.x + offset.x, y: s.to.y + offset.y }])), s.rate);
    }
  }
  const c = compactInterface(d, catalogs);
  assert.ok(witnessScore(c.witness).area <= witnessScore(d.witness).area);
  for (const g of [...c.port_groups.sources, ...c.port_groups.targets])
    assert.deepEqual({ x: g.port_x, y: g.port_y }, c.catalog_ports[g.node].find(p => p.id === g.port).connection);
  assert.equal(Object.hasOwn(c, 'composition_plan'), false);
});

test('fourteen-facility closure consumes the shared output once and is independent of argument ordering', () => {
  const targets = [id('F28'), id('F29')], source = inputs(targets), d = composeBlocks(catalogs, graph, source, targets);
  assert.equal(d.witness.placements.length, 14); assert.equal(d.block.pending_edges.some(e => e.source === id('F42')), false);
  assert.deepEqual(composeBlocks(catalogs, graph, [...source].reverse(), [...targets].reverse()).witness, d.witness);
  assert.equal(d.composition_plan.inversions, 0);
});

test('explicit normalized region placements are retained while frontier routing stays shared', () => {
  const targets = [id('F28'), id('F29')], source = inputs(targets), control = composeBlocks(catalogs, graph, source, targets);
  const offsets = control.composition_plan.regions;
  const d = composeBlocks(catalogs, graph, source, targets, { regionOffsets: offsets });
  assert.deepEqual(d.witness, control.witness);
  assert.equal(d.composition_plan.region_placement, 'explicit-occupied-packing');
  assert.throws(() => composeBlocks(catalogs, graph, source, targets, { regionOffsets: offsets.map(o => ({ ...o, x: o.x + 1 })) }), { code: 'composition-offset-origin' });
});

test('unready inputs, duplicate ownership and mismatched retained transport cannot be hidden', () => {
  const source = inputs([id('F28')]);
  assert.throws(() => composeBlocks(catalogs, graph, source, [id('F29')]), { code: 'composition-frontier-not-ready' });
  assert.throws(() => composeBlocks(catalogs, graph, [...source, { ...source[0], id: 'duplicate' }], [id('F28')]), { code: 'composition-duplicate-owner' });
  const changed = structuredClone(source); changed[0].detail.witness.transport_networks[0].transport = 'pipe';
  assert.throws(() => composeBlocks(catalogs, graph, changed, [id('F28')]), { code: 'composition-network-transport' });
});

test('retained identifiers are reindexed without recursively escaping prior compositions', () => {
  const source = structuredClone(inputs([id('F28')]));
  const nested = value => { for (let i = 0; i < 16; i++) value = JSON.stringify(['region', 'retained', value]); return value; };
  for (const r of source) {
    for (const c of r.detail.witness.logistics_components) c.id = nested(c.id);
    for (const n of r.detail.witness.transport_networks) {
      n.requirement_ids = n.requirement_ids.map(nested); n.component_ids = n.component_ids.map(nested);
    }
  }
  const d = composeBlocks(catalogs, graph, source, [id('F28')]);
  const ids = [...d.witness.logistics_components.map(c => c.id), ...d.witness.transport_networks.flatMap(n => n.requirement_ids)];
  assert.ok(ids.every(s => s.length < 200)); assert.equal(new Set(ids).size, ids.length);
  const components = new Set(d.witness.logistics_components.map(c => c.id));
  assert.ok(d.witness.transport_networks.every(n => n.component_ids.every(k => components.has(k))));
});

test('prototype-named facility IDs survive normalization, portfolio cloning and JSON serialization', () => {
  const snapshot = loadGraphSnapshot(), previous = id('F28');
  snapshot.placements.find(p => p.instance === previous).instance = '__proto__';
  for (const e of [...snapshot.internal_requirements, ...snapshot.external_requirements]) {
    if (e.source === previous) e.source = '__proto__'; if (e.target === previous) e.target = '__proto__';
  }
  const renamed = normalizeGraph(snapshot, catalogs), result = composeBlocks(catalogs, renamed, inputs([previous]), ['__proto__'], { lanePolicy: 'portfolio' });
  assert.deepEqual(result.failures, []);
  for (const raw of result.candidates) {
    const d = JSON.parse(JSON.stringify(raw));
    assert.deepEqual(d.witness.utilizations.__proto__, renamed.nodes.find(n => n.id === '__proto__').utilization);
    assert.ok(Object.hasOwn(d.catalog_ports, '__proto__')); assert.ok(d.catalog_ports.__proto__.length > 0);
  }
});

test('three frontiers reach 53 unique facilities and straight process chains use one shared port cell', () => {
  let current = regions;
  for (const count of [41, 47, 53]) {
    const targets = readyConsumers(graph, current.flatMap(r => r.detail.witness.placements.map(p => p.instance)), catalogs);
    const result = composeBlocks(catalogs, graph, current, targets, { lanePolicy: 'portfolio' });
    assert.deepEqual(result.failures, []); assert.equal(result.candidates.length, 2);
    const candidates = result.candidates.map(d => compactInterface(d, catalogs));
    candidates.sort((a, b) => witnessScore(a.witness).area - witnessScore(b.witness).area);
    const best = candidates[0]; assert.equal(best.witness.placements.length, count);
    for (const n of best.witness.transport_networks) for (const s of n.segments)
      assert.equal(Math.abs(s.from.x - s.to.x) + Math.abs(s.from.y - s.to.y), 1);
    if (count > 41) {
      const plan = best.pre_compaction_plan.composition_plan;
      assert.equal(plan.inversions, 0);
      for (const l of plan.bindings) {
        const a = best.port_groups.sources.find(g => g.id === l.source_group), b = best.port_groups.targets.find(g => g.id === l.target_group);
        assert.equal(a.port_x, b.port_x); assert.equal(a.port_y, b.port_y);
      }
      assert.ok(plan.stack_cache.hits > 0);
    }
    current = [{ id: 'retained', detail: best }];
  }
  assert.deepEqual(readyConsumers(graph, current[0].detail.witness.placements.map(p => p.instance), catalogs), []);
});
