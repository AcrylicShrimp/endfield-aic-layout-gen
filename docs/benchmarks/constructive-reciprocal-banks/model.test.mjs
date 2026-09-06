import fs from 'node:fs';
import test from 'node:test';
import assert from 'node:assert/strict';
import { bankCandidates, reciprocalFamily, allocateAdjacent, adjacentPairs } from './model.mjs';
import { normalizeGraph } from '../constructive-cycle-blocks/graph.mjs';
import { applyReallocation } from '../constructive-reciprocal-strips/graph-delta.mjs';
import { witnessScore } from '../constructive-auto-interface/compact.mjs';
import { add, rate, parseRate, dto } from '../constructive-interconnect/rates.mjs';
const read = p => JSON.parse(fs.readFileSync(new URL(p, import.meta.url)));
const catalogs = Object.fromEntries(['recipes', 'items', 'facilities', 'transports', 'components'].map(k => [k,
  read(`../../../data/game/normalized/${k === 'components' ? 'logistics-components' : k}.json`)]));
const graph = read('../constructive-scc-groups/regions.json').allocations.reduce(applyReallocation,
  normalizeGraph(read('../constructive-reciprocal-strips/attached-graph.json'), catalogs));
const selected = ['F32', 'F33', 'F38', 'F45', 'F46', 'F23', 'F26'];
const members = graph.nodes.filter(n => selected.includes(n.label)).map(n => n.id);
const margins = edges => {
  const m = new Map();
  for (const e of edges) for (const field of ['source', 'target']) {
    const k = JSON.stringify([field, e[field], e.item, e.transport]); m.set(k, add(m.get(k) ?? rate(0), parseRate(e.rate)));
  }
  return [...m].map(([k, v]) => [k, dto(v)]).sort(([a], [b]) => a.localeCompare(b));
};

test('adjacency flow reallocates three sources across four targets without changing any exact margin', () => {
  const item = catalogs.items.items.find(i => i.transport === 'belt').id;
  const names = ['P0', 'R0', 'P1', 'R1', 'P2', 'C0', 'C1'], family = { members: names.slice(0, 5), producers: ['P0', 'P1', 'P2'], forward: item };
  const e = (source, target, numerator, denominator) => ({ id: `${source}:${target}`, source, target, item, transport: 'belt', internal: true, rate: { numerator, denominator } });
  const g = { nodes: names.map(id => ({ id })), edges: [e('P0', 'R0', 1, 3), e('P0', 'R1', 1, 9), e('P1', 'R1', 2, 9),
    e('P1', 'C0', 2, 9), e('P2', 'C0', 1, 9), e('P2', 'C1', 1, 3)] }, before = structuredClone(g);
  const pairs = new Set([['P0', 'R0'], ['P1', 'R0'], ['P1', 'R1'], ['P2', 'R1']].map(JSON.stringify));
  const result = allocateAdjacent(g, family, pairs, catalogs, family.members);
  assert.deepEqual(margins(result.graph.edges), margins(g.edges)); assert.deepEqual(g, before);
  assert.ok(!result.graph.edges.some(e => e.source === 'P0' && e.target === 'R1'));
  assert.deepEqual(result.graph.edges.find(e => e.source === 'P0' && e.target === 'C0').rate, { numerator: 1, denominator: 9 });
  assert.deepEqual(result.graph.edges.find(e => e.source === 'P1' && e.target === 'R1').rate, { numerator: 1, denominator: 3 });
  assert.equal(allocateAdjacent(g, family, new Set(), catalogs, family.members), null);
});

test('recipe identities discover a reciprocal family without using facility labels', () => {
  const family = reciprocalFamily(graph, catalogs, members), renamed = structuredClone(graph);
  renamed.nodes.forEach((n, i) => { n.label = `renamed-${i}`; });
  assert.equal(family.members.length, 5); assert.equal(family.producers.length, 3); assert.equal(family.returners.length, 2);
  assert.equal(family.consumers.length, 2); assert.deepEqual(reciprocalFamily(renamed, catalogs, members), family);
  assert.throws(() => reciprocalFamily(graph, catalogs, [...members, members[0]]), { code: 'bank-member-schema' });
  assert.throws(() => reciprocalFamily(graph, catalogs, ['absent']), { code: 'bank-member-schema' });
  assert.throws(() => bankCandidates(graph, catalogs, members, { maxLayouts: 0 }), { code: 'bank-layout-budget' });
});

test('retained return edges constrain feasible strips and all generated internal wires are adjacent', () => {
  const before = structuredClone(graph), r = bankCandidates(graph, catalogs, members);
  assert.equal(r.counts.layouts, 384); assert.deepEqual(graph, before);
  assert.equal(r.candidates.length, 8); assert.equal(r.counts.return_nonadjacent, 376);
  for (const c of r.candidates) {
    assert.deepEqual(margins(c.graph.edges), margins(graph.edges));
    assert.deepEqual(c.graph.edges.filter(e => e.item === c.family.back), graph.edges.filter(e => e.item === c.family.back));
    const pairs = adjacentPairs(c.detail.witness, catalogs);
    for (const e of c.graph.edges.filter(e => c.family.members.includes(e.source) && c.family.members.includes(e.target)))
      assert.ok(pairs.has(JSON.stringify([e.source, e.target])));
    assert.deepEqual(witnessScore(c.detail.witness).tuple, [175, 12, 0]); assert.equal(c.detail.witness.logistics_components.length, 0);
    assert.equal(new Set(c.detail.witness.placements.map(p => p.instance)).size, 5);
  }
});
