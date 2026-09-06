import test from 'node:test';
import assert from 'node:assert/strict';
import { applyReallocation } from './graph-delta.mjs';
const edge = (id, source, target, numerator = 1, denominator = 2) => ({ id, source, target, item: 'commodity', transport: 'belt', internal: true, rate: { numerator, denominator } });
const graph = { nodes: ['a', 'b', 'c', 'd'].map(id => ({ id })), edges: [edge('ac', 'a', 'c'), edge('ad', 'a', 'd'), edge('bc', 'b', 'c'), edge('bd', 'b', 'd')] };
const delta = { item: 'commodity', replaced_edge_ids: ['ac', 'ad', 'bc', 'bd'], replacement_edges: [edge('new-ac', 'a', 'c', 1, 1), edge('new-bd', 'b', 'd', 1, 1)] };
test('graph deltas conserve exact margins and detach their outputs', () => {
  const before = structuredClone({ graph, delta }), g = applyReallocation(graph, delta);
  assert.equal(g.edges.length, 2); g.edges[0].rate.numerator++;
  assert.deepEqual({ graph, delta }, before);
  const unchanged = applyReallocation(graph, null); unchanged.nodes.pop(); assert.deepEqual(graph, before.graph);
});
test('stale, duplicate, unbalanced and cross-commodity deltas fail explicitly', () => {
  assert.throws(() => applyReallocation(applyReallocation(graph, delta), delta), { code: 'strip-graph-delta' });
  for (const mutate of [d => d.replaced_edge_ids.push('ac'), d => d.replacement_edges[0].rate.numerator++,
    d => d.replacement_edges[0].item = 'other', d => d.replacement_edges[0].transport = 'pipe',
    d => d.replacement_edges[0].target = 'unknown', d => d.replacement_edges[0].rate.denominator = 0,
    d => d.replacement_edges[1].id = d.replacement_edges[0].id]) {
    const d = structuredClone(delta); mutate(d); assert.throws(() => applyReallocation(graph, d), { code: 'strip-graph-delta' });
  }
});
