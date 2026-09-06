import { add, compare, dto, multiply, parseRate, rate, capacity } from '../constructive-interconnect/rates.mjs';
export const diagnostic = (code, stage, message, entity = null, severity = 'error') => ({ code: `chain-${code}`, severity, stage, message, entity });
export const fail = (code, message, entity = null) => { throw Object.assign(new Error(message), diagnostic(code, 'chain-input', message, entity)); };
const fields = (v, keys, entity) => {
  if (!v || typeof v !== 'object' || Array.isArray(v) || Object.keys(v).some(k => !keys.includes(k)) || keys.some(k => !(k in v)))
    fail('input-invalid', `Expected exactly: ${keys.join(', ')}`, entity);
};
export function positive(v, entity) {
  fields(v, ['numerator', 'denominator'], entity);
  if (![v.numerator, v.denominator].every(n => Number.isSafeInteger(n) && n > 0)) fail('input-invalid', 'Expected positive exact rational', entity);
  return parseRate(v);
}
const name = s => typeof s === 'string' && s.length > 0 && s.length <= 200;
export const marginKey = (instance, direction, item) => JSON.stringify([instance, direction, item]);
export function recipeMargins(node, recipe) {
  const result = new Map();
  for (const [direction, amounts] of [['input', recipe.inputs], ['output', recipe.outputs]]) for (const a of amounts) {
    const key = marginKey(node.id, direction, a.item), value = multiply(rate(BigInt(a.quantity) * 1000n, recipe.duration_ms), parseRate(node.utilization));
    result.set(key, add(result.get(key) ?? rate(0), value));
  }
  return result;
}
export function parseGraph(input, catalogs, { minimumGap = 1, allowedFamilies = ['straight', 'left', 'right'] } = {}) {
  fields(input, ['schema_version', 'id', 'nodes', 'edges', 'external', 'policy', 'budget'], 'request');
  if (input.schema_version !== 1 || !name(input.id) || !Array.isArray(input.nodes) || input.nodes.length < 2 || input.nodes.length > 16
    || !Array.isArray(input.edges) || !Array.isArray(input.external)) fail('input-invalid', 'Expected schema 1 and a 2–16-node graph', input.id);
  fields(input.policy, ['families', 'gaps', 'ports_per_side'], 'policy');
  const { families, gaps, ports_per_side: ports } = input.policy;
  if (!Array.isArray(families) || !families.length || new Set(families).size !== families.length || families.some(f => !allowedFamilies.includes(f))
    || !Array.isArray(gaps) || !gaps.length || new Set(gaps).size !== gaps.length || gaps.some(g => !Number.isSafeInteger(g) || g < minimumGap || g > 16)
    || !Number.isSafeInteger(ports) || ports < 1 || ports > 16) fail('input-invalid', 'Invalid family, gap or port proposal policy');
  const budgetKeys = ['proposals', 'route_expansions', 'validations', 'elapsed_ms'];
  fields(input.budget, budgetKeys, 'budget');
  if (budgetKeys.some(k => !Number.isSafeInteger(input.budget[k]) || input.budget[k] <= 0)) fail('input-invalid', 'Expected positive integer work budgets');
  const nodes = new Map(), expected = new Map(), actual = new Map(), edgeIds = new Set();
  const sum = (map, key, value) => map.set(key, add(map.get(key) ?? rate(0), value));
  for (const n of input.nodes) {
    fields(n, ['id', 'recipe', 'utilization'], 'node');
    if (!name(n.id) || nodes.has(n.id)) fail('input-invalid', 'Invalid or duplicate node ID', n.id);
    const u = positive(n.utilization, n.id), recipe = catalogs.recipes.recipes.find(r => r.id === n.recipe);
    const facility = catalogs.facilities.facilities.find(f => f.id === recipe?.facility);
    if (compare(u, rate(1)) > 0 || !recipe || !facility) fail('input-invalid', 'Unknown recipe/facility or utilization above one', n.id);
    if (![facility.footprint.width, facility.footprint.height].every(x => Number.isSafeInteger(x) && x > 0 && x <= 1000)
      || facility.footprint.width * facility.footprint.height > 10000 || !facility.allowed_rotations?.length
      || facility.allowed_rotations.some(r => ![0, 90, 180, 270].includes(r))) fail('input-invalid', 'Invalid or over-guard catalog geometry', n.id);
    if (!Number.isSafeInteger(recipe.duration_ms) || recipe.duration_ms <= 0 || [...recipe.inputs, ...recipe.outputs].some(a => !Number.isSafeInteger(a.quantity) || a.quantity <= 0))
      fail('input-invalid', 'Invalid catalog recipe rate', n.id);
    if ([...recipe.inputs, ...recipe.outputs].some(a => !catalogs.items.items.some(i => i.id === a.item))) fail('input-invalid', 'Recipe references an unknown catalog item', n.id);
    const node = { ...structuredClone(n), utilization: dto(u), facility: facility.id, recipe_data: recipe, facility_data: facility };
    nodes.set(n.id, node);
    for (const [k, v] of recipeMargins(node, recipe)) expected.set(k, v);
  }
  const incoming = new Map([...nodes.keys()].map(id => [id, []])), outgoing = new Map([...nodes.keys()].map(id => [id, []]));
  const edges = input.edges.map(e => {
    fields(e, ['id', 'source', 'target', 'item', 'rate'], 'edge');
    if (!name(e.id) || edgeIds.has(e.id) || !nodes.has(e.source) || !nodes.has(e.target) || e.source === e.target) fail('unsupported-topology', 'Invalid, duplicate or self-referential chain edge', e.id);
    edgeIds.add(e.id); const value = positive(e.rate, e.id), item = catalogs.items.items.find(i => i.id === e.item);
    const transport = catalogs.transports.transports.find(t => t.kind === item?.transport);
    if (!transport) fail('input-invalid', 'Unknown edge item/transport', e.id);
    if (compare(value, capacity(transport.capacity)) > 0) fail('unsupported-lanes', 'This prototype stage supports one capacity lane per internal edge', e.id);
    const edge = { ...structuredClone(e), rate: dto(value), transport: item.transport };
    incoming.get(e.target).push(edge); outgoing.get(e.source).push(edge);
    sum(actual, marginKey(e.source, 'output', e.item), value); sum(actual, marginKey(e.target, 'input', e.item), value);
    return edge;
  });
  for (const e of input.external) {
    fields(e, ['id', 'instance', 'direction', 'item', 'rate'], 'external');
    if (!name(e.id) || edgeIds.has(e.id) || !nodes.has(e.instance) || !['input', 'output'].includes(e.direction)) fail('input-invalid', 'Invalid external obligation', e.id);
    edgeIds.add(e.id); sum(actual, marginKey(e.instance, e.direction, e.item), positive(e.rate, e.id));
  }
  for (const k of new Set([...expected.keys(), ...actual.keys()])) if (compare(expected.get(k) ?? rate(0), actual.get(k) ?? rate(0)))
    fail('input-invalid', 'Declared internal and external rates do not equal the utilized recipe ledger', k);
  return { id: input.id, nodes, edges, incoming, outgoing, expected, external: structuredClone(input.external), policy: structuredClone(input.policy), budget: structuredClone(input.budget) };
}
export function prepare(input, catalogs) {
  const graph = parseGraph(input, catalogs), { nodes, edges, incoming, outgoing } = graph;
  const sinks = [...nodes.keys()].filter(id => outgoing.get(id).length === 0);
  if (edges.length !== nodes.size - 1 || sinks.length !== 1 || [...nodes.keys()].some(id => incoming.get(id).length > 1 || outgoing.get(id).length > 1))
    fail('unsupported-topology', 'Expected a simple chain; branches, merges, cycles and parallel edges are later milestones');
  const order = [], growthEdges = []; let current = sinks[0];
  while (current && !order.includes(current)) { order.push(current); const e = incoming.get(current)[0]; if (e) growthEdges.push(e); current = e?.source; }
  if (order.length !== nodes.size || current) fail('unsupported-topology', 'Disconnected or cyclic chain input');
  let belt = false;
  for (const e of growthEdges) { if (belt && e.transport === 'pipe') fail('unsupported-pipe-order', 'This sink-first chain would add a pipe after belt growth', e.id); belt ||= e.transport === 'belt'; }
  return { ...graph, order, growthEdges };
}

export function checkState(graph, witness, validation) {
  const error = (message, entity = null) => { throw Object.assign(new Error(message), diagnostic('margin-mismatch', 'chain-validation', message, entity)); };
  if (!validation?.covered_geometry_validated || !Array.isArray(validation.recipe_terminals)) error('Independent geometry validation did not pass');
  const members = new Set(witness.placements.map(p => p.instance));
  if (members.size !== witness.placements.length || !graph.order.slice(0, members.size).every(id => members.has(id))) error('State is not an exact connected growth prefix');
  for (const p of witness.placements) {
    const n = graph.nodes.get(p.instance);
    if (!n || p.recipe !== n.recipe || p.facility !== n.facility || compare(positive(witness.utilizations[p.instance], p.instance), parseRate(n.utilization))) error('Changed inventory or utilization', p.instance);
  }
  const edges = graph.edges.filter(e => members.has(e.source) && members.has(e.target)), completed = new Set();
  for (const net of witness.transport_networks) {
    if (net.terminals.length !== net.requirement_ids.length * 2) error('Unexpected network terminals', net.id);
    for (const id of net.requirement_ids) {
      const e = edges.find(e => e.id === id);
      if (!e || completed.has(id) || net.item !== e.item || net.transport !== e.transport) error('Unexpected or duplicate internal requirement', id);
      completed.add(id);
      for (const [which, direction] of [['source', 'output'], ['target', 'input']]) {
        const t = net.terminals.find(t => t.id === `${id}:${which}`);
        if (!t || t.endpoint.kind !== 'facility' || t.endpoint.instance !== e[which] || t.node !== e[which] || t.direction !== direction || compare(positive(t.rate, id), parseRate(e.rate))) error('Changed graph edge terminal', id);
      }
    }
  }
  if (completed.size !== edges.length) error('Missing internal requirement');
  const routed = new Map();
  for (const e of edges) for (const [instance, direction] of [[e.source, 'output'], [e.target, 'input']]) {
    const k = marginKey(instance, direction, e.item); routed.set(k, add(routed.get(k) ?? rate(0), parseRate(e.rate)));
  }
  const seen = new Set();
  for (const t of validation.recipe_terminals) {
    const k = marginKey(t.instance, t.direction, t.item), required = graph.expected.get(k), r = routed.get(k) ?? rate(0);
    if (!members.has(t.instance) || !required || seen.has(k) || compare(parseRate(t.required_rate), required) || compare(parseRate(t.routed_rate), r)
      || compare(add(parseRate(t.pending_rate), r), required)) error('Independent recipe ledger differs from the graph prefix', k);
    seen.add(k);
  }
  if (seen.size !== [...graph.expected.keys()].filter(k => members.has(JSON.parse(k)[0])).length) error('Incomplete independent margin report');
  return true;
}
