import { rate, add, parseRate, dto } from '../constructive-interconnect/rates.mjs';
const fail = message => { throw Object.assign(new Error(message), { code: 'strip-graph-delta', severity: 'error', stage: 'reciprocal-integration' }); };
const margins = edges => {
  const m = new Map();
  for (const e of edges) for (const [d, id] of [['output', e.source], ['input', e.target]]) {
    const k = JSON.stringify([d, id, e.item, e.transport]); m.set(k, add(m.get(k) ?? rate(0), parseRate(e.rate)));
  }
  return JSON.stringify([...m].sort(([a], [b]) => a.localeCompare(b)).map(([k, v]) => [k, dto(v)]));
};

export function applyReallocation(graph, delta) {
  if (!delta) return structuredClone(graph);
  if (!Array.isArray(delta.replaced_edge_ids) || !delta.replaced_edge_ids.length || !Array.isArray(delta.replacement_edges) || !delta.replacement_edges.length)
    fail('Expected nonempty replaced-edge IDs and replacement edges');
  const ids = new Set(delta.replaced_edge_ids), nodes = new Set(graph.nodes.map(n => n.id));
  const removed = graph.edges.filter(e => ids.has(e.id));
  if (ids.size !== delta.replaced_edge_ids.length || removed.length !== ids.size || removed.some(e => !e.internal || e.item !== delta.item))
    fail('Allocation delta has stale/duplicate IDs or changes another commodity');
  const transports = new Set(removed.map(e => e.transport));
  if (transports.size !== 1 || delta.replacement_edges.some(e => typeof e.id !== 'string' || !e.id || !e.internal || e.item !== delta.item ||
    !transports.has(e.transport) || !nodes.has(e.source) || !nodes.has(e.target) || !Number.isSafeInteger(e.rate?.numerator) ||
    !Number.isSafeInteger(e.rate?.denominator) || e.rate.numerator <= 0 || e.rate.denominator <= 0)) fail('Replacement edge has invalid identity, rate, node or transport');
  if (margins(removed) !== margins(delta.replacement_edges)) fail('Allocation delta changes exact commodity supply or demand');
  const edges = [...graph.edges.filter(e => !ids.has(e.id)), ...delta.replacement_edges];
  if (new Set(edges.map(e => e.id)).size !== edges.length) fail('Allocation delta creates duplicate edge identities');
  return structuredClone({ ...graph, edges });
}
