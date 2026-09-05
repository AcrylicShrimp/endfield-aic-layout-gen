import fs from 'node:fs';
import test from 'node:test';
import assert from 'node:assert/strict';
import { normalizeGraph, extractCycles } from '../constructive-cycle-blocks/graph.mjs';
import { constructCycle } from '../constructive-cycle-blocks/model.mjs';
import { compactInterface, witnessScore } from '../constructive-auto-interface/compact.mjs';
import { composeBlocks, readyConsumers, placeRegions } from '../constructive-block-composition/model.mjs';
import { discoverClusters, packingCandidates } from './model.mjs';
import { nestingCandidates, pendingPorts, footprint, accessFailures } from './nesting.mjs';
const read = p => JSON.parse(fs.readFileSync(new URL(p, import.meta.url)));
const catalogs = Object.fromEntries(['recipes', 'facilities', 'items', 'transports', 'components'].map(k => [k,
  read(`../../../data/game/normalized/${k === 'components' ? 'logistics-components' : k}.json`)]));
const graph = normalizeGraph(read('../constructive-chain-blocks/gap-1-placement.json'), catalogs);
const sources = extractCycles(graph, catalogs).blocks.map(b => ({ id: b.id, detail: compactInterface(constructCycle(catalogs, graph, b, 'root'), catalogs) }));

test('dependency clusters preserve shared suppliers and ignore input ordering and display labels', () => {
  const groups = discoverClusters(graph, sources, catalogs);
  assert.equal(groups.length, 2); assert.deepEqual(groups.map(c => c.regions.length), [3, 5]);
  assert.deepEqual(groups.map(c => c.frontier.length), [2, 4]);
  assert.deepEqual(discoverClusters(graph, [...sources].reverse(), catalogs), groups);
  const renamed = structuredClone(graph); for (const n of renamed.nodes) n.label = 'display-name';
  assert.deepEqual(discoverClusters(renamed, sources, catalogs), groups);
  const ids = groups.flatMap(c => c.regions.flatMap(r => r.detail.witness.placements.map(p => p.instance)));
  assert.equal(ids.length, 35); assert.equal(new Set(ids).size, 35);
  assert.throws(() => discoverClusters(graph, [...sources, { ...sources[0], id: 'duplicate' }], catalogs), { code: 'cluster-duplicate-owner' });
});

test('translated shelves preserve all 53 facilities and interior rate geometry without shared mutable DTOs', () => {
  const clusters = discoverClusters(graph, sources, catalogs).map(c => {
    let regions = c.regions, targets = c.frontier;
    while (targets.length) {
      const ds = composeBlocks(catalogs, graph, regions, targets, { lanePolicy: 'portfolio' }).candidates.map(d => compactInterface(d, catalogs));
      ds.sort((a, b) => witnessScore(a.witness).area - witnessScore(b.witness).area);
      regions = [{ id: c.id, detail: ds[0] }]; targets = readyConsumers(graph, ds[0].witness.placements.map(p => p.instance), catalogs);
    }
    return regions[0];
  });
  assert.deepEqual(clusters.map(c => c.detail.witness.placements.length), [18, 35]);
  const original = JSON.stringify(clusters), candidates = packingCandidates(graph, clusters, catalogs);
  assert.equal(candidates.length, 8); assert.equal(JSON.stringify(clusters), original);
  for (const d of candidates) {
    assert.equal(d.witness.placements.length, 53); assert.equal(new Set(d.witness.placements.map(p => p.instance)).size, 53);
    assert.equal(d.factory_validated, false);
    for (const r of clusters) {
      const o = d.packing_plan.translations.find(t => t.region === r.id);
      for (const p of r.detail.witness.placements) assert.deepEqual(d.witness.placements.find(q => q.instance === p.instance), { ...p, x: p.x + o.x, y: p.y + o.y });
      for (const n of r.detail.witness.transport_networks) {
        const q = d.witness.transport_networks.find(m => m.item === n.item), edges = new Map(q.segments.map(s => [JSON.stringify([s.from, s.to]), s.rate]));
        for (const s of n.segments) assert.deepEqual(edges.get(JSON.stringify([{ x: s.from.x + o.x, y: s.from.y + o.y }, { x: s.to.x + o.x, y: s.to.y + o.y }])), s.rate);
      }
    }
  }
  candidates[0].witness.transport_networks[0].segments[0].rate.numerator += 1;
  assert.equal(JSON.stringify(clusters), original);
  const nested = nestingCandidates(graph, clusters, catalogs, 4);
  assert.equal(nested.length, 4); assert.equal(JSON.stringify(clusters), original);
  assert.deepEqual(nested.map(d => d.packing_plan.offset), nestingCandidates(graph, [...clusters].reverse(), catalogs, 4).map(d => d.packing_plan.offset));
  for (const d of nested) {
    assert.equal(d.witness.placements.length, 53); assert.equal(d.factory_validated, false);
    const search = d.packing_plan.search;
    assert.equal(search.collision_free, search.output_accessible + search.output_obstructed);
    for (const r of clusters) {
      const other = clusters.find(c => c.id !== r.id), o = d.packing_plan.translations.find(t => t.region === r.id), q = d.packing_plan.translations.find(t => t.region === other.id);
      const a = footprint(r.detail.witness), b = footprint(other.detail.witness);
      assert.deepEqual(accessFailures(pendingPorts(graph, r, catalogs), b, q.x + b.x - o.x - a.x, q.y + b.y - o.y - a.y), []);
      for (const p of r.detail.witness.placements) assert.deepEqual(d.witness.placements.find(v => v.instance === p.instance), { ...p, x: p.x + o.x, y: p.y + o.y });
    }
  }
});

test('region placement rejects duplicate/unknown/noninteger translations instead of losing members', () => {
  const rs = sources.slice(0, 2), good = rs.map((r, i) => ({ region: r.id, x: i * 50, y: 0 }));
  assert.throws(() => placeRegions(rs, [good[0], good[0]], catalogs), { code: 'composition-region-placement-schema' });
  assert.throws(() => placeRegions(rs, [good[0], { ...good[1], x: 0.5 }], catalogs), { code: 'composition-region-placement-schema' });
  assert.throws(() => packingCandidates(graph, [], catalogs), { code: 'cluster-packing-count' });
});
