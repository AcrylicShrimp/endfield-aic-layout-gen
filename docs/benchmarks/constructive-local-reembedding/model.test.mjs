import fs from 'node:fs';
import test from 'node:test';
import assert from 'node:assert/strict';
import { normalizeGraph, extractCycles } from '../constructive-cycle-blocks/graph.mjs';
import { constructCycle } from '../constructive-cycle-blocks/model.mjs';
import { compactInterface } from '../constructive-auto-interface/compact.mjs';
import { reembed, relocationCandidates, extractChains, flexibleTerminalAnchors } from './model.mjs';
import { key, direction } from './path.mjs';
import { witnessKey } from '../constructive-embedding-compaction/transactions.mjs';
const read = p => JSON.parse(fs.readFileSync(new URL(p, import.meta.url)));
const catalogs = Object.fromEntries(['recipes', 'facilities', 'items', 'transports', 'components'].map(k =>
  [k, read(`../../../data/game/normalized/${k === 'components' ? 'logistics-components' : k}.json`)]));
const graph = normalizeGraph(read('../constructive-chain-blocks/gap-1-placement.json'), catalogs), blocks = extractCycles(graph, catalogs).blocks;
const make = b => structuredClone(compactInterface(constructCycle(catalogs, graph, b, 'root'), catalogs));
const margins = w => w.transport_networks.map(n => ({ id: n.id, item: n.item, transport: n.transport, requirement_ids: n.requirement_ids, terminals: n.terminals, component_ids: n.component_ids }));
function freeze(value) { if (value && typeof value === 'object' && !Object.isFrozen(value)) { Object.freeze(value); for (const v of Object.values(value)) freeze(v); } return value; }

test('explicit route priority changes only ordering, validates IDs and leaves the input intact', () => {
  const d = freeze(make(blocks[0])), snapshot = structuredClone(d), { chains } = extractChains(d.witness), ids = chains.map(c => c.id);
  assert.ok(ids.length > 1);
  const r = reembed(d, catalogs, { chainIds: ids, routePriority: [...ids].reverse() });
  assert.equal(r.routes[0].id, ids.at(-1));
  assert.deepEqual(d, snapshot);
  const without = reembed(d, catalogs, { chainIds: [ids[0]] });
  const unrelated = reembed(d, catalogs, { chainIds: [ids[0]], routePriority: [ids.at(-1)] });
  assert.deepEqual(unrelated.routes, without.routes);
  assert.throws(() => reembed(d, catalogs, { routePriority: [ids[0], ids[0]] }), { code: 'local-route-priority' });
  assert.throws(() => reembed(d, catalogs, { routePriority: [-1] }), { code: 'local-route-priority' });
});

test('only ordinary singleton terminal anchors release the retained segment arm', () => {
  const a = { x: 0, y: 0 }, b = { x: 2, y: 0 };
  const w = { logistics_components: [], transport_networks: [{ transport: 'belt', terminals: [{ position: a }, { position: b }] }] };
  const chains = [{ transport: 'belt', cells: [a, { x: 1, y: 0 }, b] }];
  assert.deepEqual([...flexibleTerminalAnchors(w, chains)], ['belt:0,0', 'belt:2,0']);
  w.logistics_components.push({ transport: 'belt', position: a });
  assert.deepEqual([...flexibleTerminalAnchors(w, chains)], ['belt:2,0']);
  w.transport_networks[0].terminals.push({ position: b });
  assert.equal(flexibleTerminalAnchors(w, chains).size, 0);
  w.transport_networks[0].terminals.pop();
  chains.push({ transport: 'belt', cells: [b, { x: 3, y: 0 }] });
  assert.equal(flexibleTerminalAnchors(w, chains).size, 0);
});

test('flexible terminal reroutes preserve actual catalog margins and immutable input', () => {
  let changedArm = 0;
  for (const block of blocks) {
    const d = freeze(make(block)), snapshot = structuredClone(d), { chains } = extractChains(d.witness);
    for (const c of chains) {
      const r = reembed(d, catalogs, { chainIds: [c.id], endpointPolicy: 'terminal-flexible' });
      assert.deepEqual(d, snapshot);
      if (r.status !== 'candidate') continue;
      assert.deepEqual(margins(r.detail.witness), margins(d.witness));
      assert.deepEqual(r.detail.witness.placements, d.witness.placements);
      const updated = extractChains(r.detail.witness).chains.find(a => a.network === c.network && key(a.cells[0]) === key(c.cells[0]) && key(a.cells.at(-1)) === key(c.cells.at(-1)));
      assert.ok(updated);
      if (updated.first !== c.first || updated.last !== c.last) changedArm++;
      assert.equal(r.detail.reembedding.endpoint_policy, 'terminal-flexible');
      r.detail.witness.transport_networks[0].terminals[0].rate.numerator++;
      assert.deepEqual(d, snapshot);
    }
  }
  assert.ok(changedArm > 0, 'At least one actual catalog terminal must turn differently');
  assert.throws(() => reembed(make(blocks[0]), catalogs, { endpointPolicy: 'unknown' }), { code: 'local-endpoint-policy' });
});

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
    const d = freeze(make(b)), snapshot = structuredClone(d), { chains } = extractChains(d.witness);
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
  assert.throws(() => reembed(d, catalogs, { moves: [{ instances: ['unknown'], dx: 0, dy: 0 }] }), { code: 'local-move-input' });
  const [a, b] = d.witness.placements;
  assert.throws(() => reembed(d, catalogs, { moves: [{ instances: [a.instance], dx: b.x - a.x, dy: b.y - a.y }] }), { code: 'local-body-collision' });
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
  const snapshot = structuredClone(d), move = { instances: [p.instance], dx: 0, dy: 18 }, r = reembed(freeze(d), catalogs, { moves: [move] });
  assert.deepEqual(d, snapshot);
  assert.ok(r.affected_chains.some(c => c.item === 'test-only'), 'The unrelated wire intersecting the new body must be scheduled for rerouting');
  assert.equal(r.status, 'candidate');
  assert.deepEqual(r.detail.witness.placements.filter(a => a.instance !== p.instance), original.witness.placements.filter(a => a.instance !== p.instance));
  const moved = r.detail.witness.placements.find(a => a.instance === p.instance);
  assert.equal(moved.y, p.y + move.dy);
  assert.ok(r.detail.witness.transport_networks.at(-1).cells.every(q => q.x < moved.x || q.x >= moved.x + moved.width || q.y < moved.y || q.y >= moved.y + moved.height));
  r.detail.reembedding.moves[0].dx++;
  assert.deepEqual(move, { instances: [p.instance], dx: 0, dy: 18 });
});
