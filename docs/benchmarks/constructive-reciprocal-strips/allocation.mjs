import { rate, add, compare, parseRate, dto } from '../constructive-interconnect/rates.mjs';
const subtract = (a, b) => rate(a.n * b.d - b.n * a.d, a.d * b.d);
const permutations = xs => xs.length ? xs.flatMap((x, i) => permutations(xs.filter((_, j) => j !== i)).map(t => [x, ...t])) : [[]];

// Compatible commodity edge endpoints are allocation decisions, not recipe identities.
// Preserve each producer's total supply and each consumer's total demand exactly.
export function compatibleAllocations(graph, block) {
  const edges = graph.edges.filter(e => e.item === block.forward_item && block.producers.includes(e.source) && block.consumers.includes(e.target));
  const sources = [...new Set(edges.map(e => e.source))].sort(), targets = [...new Set(edges.map(e => e.target))].sort();
  if (!edges.length || sources.length > 3 || targets.length > 3) return [];
  const total = (field, id) => edges.filter(e => e[field] === id).reduce((s, e) => add(s, parseRate(e.rate)), rate(0));
  const ids = new Set(edges.map(e => e.id)), results = [], seen = new Set();
  for (const sourceOrder of permutations(sources)) for (const targetOrder of permutations(targets)) {
    const supply = new Map(sources.map(id => [id, total('source', id)])), demand = new Map(targets.map(id => [id, total('target', id)])), replacement = [];
    for (const source of sourceOrder) for (const target of targetOrder) {
      const a = supply.get(source), b = demand.get(target), value = compare(a, b) < 0 ? a : b;
      if (!value.n) continue;
      replacement.push({ id: JSON.stringify(['compatible-strip-allocation', source, target, block.forward_item]), source, target,
        item: block.forward_item, transport: edges[0].transport, rate: dto(value), internal: true });
      supply.set(source, subtract(a, value)); demand.set(target, subtract(b, value));
    }
    if ([...supply.values(), ...demand.values()].some(r => r.n !== 0n)) throw new Error('Compatible allocation did not conserve all margins');
    replacement.sort((a, b) => a.id.localeCompare(b.id));
    const signature = JSON.stringify(replacement.map(e => [e.source, e.target, e.rate]));
    if (seen.has(signature)) continue; seen.add(signature);
    results.push({ graph: { ...graph, edges: [...graph.edges.filter(e => !ids.has(e.id)), ...replacement] },
      reallocation: { replaced_edge_ids: [...ids], replacement_edges: replacement, item: block.forward_item,
        invariant: 'exact-per-source-and-per-target-commodity-margins' } });
  }
  return results;
}
