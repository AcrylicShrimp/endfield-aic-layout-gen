import { rate, add, multiply, parseRate, dto, compare } from '../constructive-interconnect/rates.mjs';
export const fail = (code, message, entity = null) => { throw Object.assign(new Error(message), {
  code, severity: 'error', stage: 'cycle-block', entity }); };
const sum = xs => xs.reduce(add, rate(0));

export function normalizeGraph(snapshot, catalogs) {
  if (!Array.isArray(snapshot.placements) || !snapshot.placements.length || snapshot.placements.length > 256 ||
      !Array.isArray(snapshot.internal_requirements) || !Array.isArray(snapshot.external_requirements))
    fail('cycle-graph-schema', 'Expected bounded facility and internal/external requirement arrays');
  const members = new Set(snapshot.placements.map(p => p.instance));
  if (snapshot.placements.some(p => typeof p.instance !== 'string' || !p.instance) || members.size !== snapshot.placements.length)
    fail('cycle-graph-duplicate', 'Missing or duplicate graph facility identity');
  const edges = [...snapshot.internal_requirements, ...snapshot.external_requirements].map(e => {
    const item = catalogs.items.items.find(i => i.id === e.item);
    const internal = snapshot.internal_requirements.includes(e);
    if (typeof e.id !== 'string' || !e.id || typeof e.source !== 'string' || !e.source || typeof e.target !== 'string' || !e.target ||
        !Number.isSafeInteger(e.rate?.numerator) || !Number.isSafeInteger(e.rate?.denominator) ||
        e.rate.numerator <= 0 || e.rate.denominator <= 0 || !item || (e.transport && e.transport !== item.transport) ||
        (internal ? !members.has(e.source) || !members.has(e.target) : members.has(e.source) === members.has(e.target)))
      fail('cycle-graph-edge', 'Unknown item, endpoint, transport mismatch or nonpositive graph rate', e.id);
    return { id: e.id, source: e.source, target: e.target, item: e.item, transport: item.transport, rate: dto(parseRate(e.rate)), kind: e.kind, internal };
  });
  if (new Set(edges.map(e => e.id)).size !== edges.length) fail('cycle-graph-duplicate', 'Duplicate requirement identity');
  const nodes = snapshot.placements.map(p => {
    const recipe = catalogs.recipes.recipes.find(r => r.id === p.recipe);
    if (!recipe || recipe.facility !== p.facility || !catalogs.facilities.facilities.some(f => f.id === p.facility))
      fail('cycle-graph-recipe', 'Recipe/facility mismatch or absent runtime facility', p.instance);
    let utilization;
    for (const [direction, field] of [['input', 'target'], ['output', 'source']]) {
      const incident = edges.filter(e => e[field] === p.instance), amounts = recipe[direction + 's'];
      if (incident.some(e => !amounts.some(a => a.item === e.item))) fail('cycle-graph-margin', 'Unexpected recipe terminal commodity', p.instance);
      for (const item of new Set(amounts.map(a => a.item))) {
        const base = sum(amounts.filter(a => a.item === item).map(a => rate(BigInt(a.quantity) * 1000n, recipe.duration_ms)));
        const actual = sum(incident.filter(e => e.item === item).map(e => parseRate(e.rate)));
        if (!base.n || !actual.n) fail('cycle-graph-margin', 'Missing or zero recipe terminal margin', p.instance);
        const u = multiply(actual, rate(base.d, base.n));
        if (compare(u, rate(1)) > 0 || (utilization && compare(u, utilization))) fail('cycle-graph-margin', 'Graph terminal margins imply inconsistent utilization', p.instance);
        utilization = u;
      }
    }
    if (!utilization) fail('cycle-graph-margin', 'Recipe has no terminal margins', p.instance);
    return { id: p.instance, label: p.label ?? p.instance, recipe: recipe.id, utilization: dto(utilization) };
  });
  return { schema_version: 1, nodes, edges };
}

export function components(graph) {
  const seen = new Map(), low = new Map(), stack = [], active = new Set(), result = [];
  const adjacency = new Map(graph.nodes.map(n => [n.id, graph.edges.filter(e => e.internal && e.source === n.id).map(e => e.target)]));
  let index = 0;
  const visit = id => {
    seen.set(id, index); low.set(id, index++); stack.push(id); active.add(id);
    for (const next of adjacency.get(id)) {
      if (!seen.has(next)) { visit(next); low.set(id, Math.min(low.get(id), low.get(next))); }
      else if (active.has(next)) low.set(id, Math.min(low.get(id), seen.get(next)));
    }
    if (low.get(id) === seen.get(id)) { const group = []; let v; do { v = stack.pop(); active.delete(v); group.push(v); } while (v !== id); result.push(group.sort()); }
  };
  for (const id of [...adjacency.keys()].sort()) if (!seen.has(id)) visit(id);
  return result;
}

export function extractCycles(graph, catalogs) {
  const byId = new Map(graph.nodes.map(n => [n.id, n])), owned = new Set(), blocks = [], failures = [];
  const recipes = new Map(graph.nodes.map(n => [n.id, catalogs.recipes.recipes.find(r => r.id === n.recipe)]));
  const cycles = components(graph).filter(c => c.length > 1 || graph.edges.some(e => e.source === c[0] && e.target === c[0]));
  for (const cycle of cycles) {
    const set = new Set(cycle), inside = graph.edges.filter(e => set.has(e.source) && set.has(e.target));
    const items = [...new Set(inside.map(e => e.item))];
    const forwardItems = items.filter(item => graph.edges.some(e => e.item === item && set.has(e.source) && !set.has(e.target)));
    if (items.length !== 2 || forwardItems.length !== 1 || inside.some(e => e.transport !== 'belt') ||
        cycle.some(id => recipes.get(id).inputs.length !== 1 || recipes.get(id).outputs.length !== 1)) {
      failures.push({ code: 'cycle-shape-unsupported', severity: 'warning', stage: 'cycle-block', message: 'Expected productive two-commodity single-input/output belt SCC', entity: cycle }); continue;
    }
    const forward = forwardItems[0], back = items.find(i => i !== forward);
    const producers = cycle.filter(id => recipes.get(id).outputs[0].item === forward);
    const returners = cycle.filter(id => recipes.get(id).outputs[0].item === back);
    const consumers = [...new Set(graph.edges.filter(e => producers.includes(e.source) && e.item === forward).map(e => e.target))].sort();
    const ids = [...new Set([...producers, ...consumers])].sort(), members = new Set(ids);
    const cuts = graph.edges.filter(e => members.has(e.source) !== members.has(e.target));
    if (!producers.length || !returners.length || producers.length + returners.length !== cycle.length ||
        consumers.some(id => !recipes.has(id) || recipes.get(id).inputs.length !== 1 || recipes.get(id).inputs[0].item !== forward) ||
        cuts.some(e => !members.has(e.source) || e.item === forward || e.item === back) ||
        producers.some(id => recipes.get(id).inputs[0].item !== back) || ids.some(id => owned.has(id))) {
      failures.push({ code: 'cycle-boundary-unsupported', severity: 'warning', stage: 'cycle-block', message: 'Cycle cannot own a closed forward consumer bank with output-only cuts', entity: cycle }); continue;
    }
    ids.forEach(id => owned.add(id));
    blocks.push({ id: `cycle-${blocks.length}`, cycle, members: ids, forward_item: forward, return_item: back, producers, returners, consumers,
      request: { schema_version: 1, producers: producers.map(id => { const { label, ...n } = byId.get(id); return n; }),
        consumers: consumers.map(id => { const { label, ...n } = byId.get(id); return n; }) },
      covered_edges: graph.edges.filter(e => members.has(e.source) && members.has(e.target)), pending_edges: cuts });
  }
  return { schema_version: 1, blocks, failures, total_facilities: graph.nodes.length, total_requirements: graph.edges.length,
    cyclic_components: cycles.length, owned_facilities: owned.size, unowned_nodes: graph.nodes.filter(n => !owned.has(n.id)), factory_validated: false };
}
