import fs from 'node:fs';
import test from 'node:test';
import assert from 'node:assert/strict';
import { normalizeGraph, extractCycles } from '../constructive-cycle-blocks/graph.mjs';
import { constructCycle } from '../constructive-cycle-blocks/model.mjs';
import { compactInterface } from '../constructive-auto-interface/compact.mjs';
import { reembed, relocationCandidates, extractChains } from './model.mjs';
import { key, direction } from './path.mjs';
import { witnessKey } from '../constructive-embedding-compaction/transactions.mjs';
const read = p => JSON.parse(fs.readFileSync(new URL(p, import.meta.url)));
const catalogs = Object.fromEntries(['recipes', 'facilities', 'items', 'transports', 'components'].map(k =>
  [k, read(`../../../data/game/normalized/${k === 'components' ? 'logistics-components' : k}.json`)]));
const graph = normalizeGraph(read('../constructive-chain-blocks/gap-1-placement.json'), catalogs), blocks = extractCycles(graph, catalogs).blocks;
const make = b => structuredClone(compactInterface(constructCycle(catalogs, graph, b, 'root'), catalogs));
const margins = w => w.transport_networks.map(n => ({ id: n.id, item: n.item, transport: n.transport, requirement_ids: n.requirement_ids, terminals: n.terminals, component_ids: n.component_ids }));

test('chains cover every directed segment and retain rates, not old bends', () => {
  for (const b of blocks) {
    const d = make(b), { chains } = extractChains(d.witness);
    for (const [ni, n] of d.witness.transport_networks.entries()) {
      const expected = n.segments.map(s => `${key(s.from)}>${key(s.to)}:${s.rate.numerator}/${s.rate.denominator}`).sort();
      const actual = chains.filter(c => c.network === ni).flatMap(c => c.cells.slice(1).map((p, i) => `${key(c.cells[i])}>${key(p)}:${c.rate.numerator}/${c.rate.denominator}`)).sort();
      assert.deepEqual(actual, expected);
    }
    assert.ok(chains.some(c => c.cells.slice(2).some((p, i) => direction(c.cells[i], c.cells[i + 1]) !== direction(c.cells[i + 1], p))));
  }
});

test('fixed-chain rerouting preserves immutable semantic margins and unselected layers', () => {
  let candidates = 0;
  for (const b of blocks) {
    const d = make(b), snapshot = structuredClone(d), { chains } = extractChains(d.witness);
    for (const c of chains) {
      const r = reembed(d, catalogs, { chainIds: [c.id] });
      assert.deepEqual(d, snapshot);
      if (r.status !== 'candidate') continue;
      candidates++; assert.equal(r.detail.factory_validated, false);
      assert.deepEqual(margins(r.detail.witness), margins(d.witness));
      assert.deepEqual(r.detail.witness.placements, d.witness.placements);
      assert.deepEqual(r.detail.witness.utilizations, d.witness.utilizations);
      assert.deepEqual(r.detail.witness.logistics_components, d.witness.logistics_components);
      for (const [i, n] of d.witness.transport_networks.entries()) if (i !== c.network) assert.deepEqual(r.detail.witness.transport_networks[i], n);
      assert.notEqual(witnessKey(r.detail.witness), witnessKey(d.witness));
      r.detail.witness.placements[0].x++; assert.deepEqual(d, snapshot);
    }
  }
  assert.ok(candidates > 0);
});

test('relocation candidates and diagnostics are deterministic and nonmutating', () => {
  const d = make(blocks[0]), snapshot = structuredClone(d), candidates = relocationCandidates(d, { limit: 32 });
  assert.deepEqual(candidates, relocationCandidates(d, { limit: 32 })); assert.deepEqual(d, snapshot);
  assert.throws(() => relocationCandidates(d, { limit: NaN }), { code: 'local-move-budget' });
  assert.throws(() => reembed(d, catalogs, { chainIds: [-1] }), { code: 'local-chain-id' });
  assert.throws(() => reembed(d, catalogs, { move: { instance: 'unknown', x: 0, y: 0 } }), { code: 'local-move-input' });
  const [a, b] = d.witness.placements;
  assert.throws(() => reembed(d, catalogs, { move: { instance: a.instance, x: b.x, y: b.y } }), { code: 'local-body-collision' });
  assert.equal(reembed(d, catalogs, { chainIds: [] }).status, 'unchanged');
  assert.deepEqual(d, snapshot);
});

test('a moved body rips up a nonincident crossing path and restores it or rejects atomically', () => {
  // Geometry-only DTO exercise. Complete recipe validation is covered by the actual-factory audit.
  const d = make(blocks[0]), original = structuredClone(d), p = d.witness.placements.find(p => blocks[0].consumers.includes(p.instance));
  // Stretch the research neighborhood without changing facility identities or catalog data.
  const maxX = Math.max(...d.witness.placements.map(p => p.x + p.width)) + 10;
  const ps = [...Array.from({ length: maxX + 1 }, (_, x) => ({ x, y: p.y + 20 })),
    ...Array.from({ length: 6 }, (_, i) => ({ x: maxX, y: p.y + 21 + i }))], rate = { numerator: 1, denominator: 10 };
  d.witness.transport_networks.push({ id: 'geometry-only-obstruction', item: 'test-only', transport: 'pipe', cells: ps,
    segments: ps.slice(1).map((q, i) => ({ from: ps[i], to: q, rate })), terminals: [
      { id: 'left', node: 'left', endpoint: { kind: 'external', node: 'left', side: 'west' }, direction: 'output', position: ps[0], rate },
      { id: 'right', node: 'right', endpoint: { kind: 'external', node: 'right', side: 'east' }, direction: 'input', position: ps.at(-1), rate }], component_ids: [], requirement_ids: [] });
  const snapshot = structuredClone(d), move = { instance: p.instance, x: p.x, y: p.y + 18 }, r = reembed(d, catalogs, { move });
  assert.deepEqual(d, snapshot);
  assert.ok(r.affected_chains.some(c => c.item === 'test-only'), 'The unrelated wire intersecting the new body must be scheduled for rerouting');
  assert.equal(r.status, 'candidate');
  assert.deepEqual(r.detail.witness.placements.filter(a => a.instance !== p.instance), original.witness.placements.filter(a => a.instance !== p.instance));
  const moved = r.detail.witness.placements.find(a => a.instance === p.instance);
  assert.equal(moved.y, move.y);
  assert.ok(r.detail.witness.transport_networks.at(-1).cells.every(q => q.x < moved.x || q.x >= moved.x + moved.width || q.y < moved.y || q.y >= moved.y + moved.height));
});
