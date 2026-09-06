import { parseGraph, fields, fail } from './input.mjs';
import { capacity, compare, dto, lanesFor, parseRate, rate } from '../constructive-interconnect/rates.mjs';

export const terminalId = (r, which) => JSON.stringify(['factory', r.kind, r.id, r.lane, which]);
export const externalNode = id => JSON.stringify(['external', id]);
export function prepareFactory(input, catalogs) {
  const graph = parseGraph(input, catalogs, { schemaVersion: 2, minimumGap: 0,
    allowedFamilies: ['straight', 'left', 'right', 'fold'], extraRootFields: ['boundary'], extraPolicyFields: ['slides_per_pose', 'port_order'] });
  if (!Number.isSafeInteger(input.policy.slides_per_pose) || input.policy.slides_per_pose < 1 || input.policy.slides_per_pose > 32
    || !['spatial', 'center'].includes(input.policy.port_order)) fail('input-invalid', 'Invalid slide count or spatial port-order policy');
  fields(input.boundary, ['margins', 'endpoint_limit'], 'boundary');
  if (!Array.isArray(input.boundary.margins) || !input.boundary.margins.length || new Set(input.boundary.margins).size !== input.boundary.margins.length
    || input.boundary.margins.some(n => !Number.isSafeInteger(n) || n < 1 || n > 32)
    || !Number.isSafeInteger(input.boundary.endpoint_limit) || input.boundary.endpoint_limit < 1 || input.boundary.endpoint_limit > 128) fail('input-invalid', 'Invalid boundary routing trial policy');
  const { nodes, edges, outgoing } = graph, pairKeys = edges.map(e => JSON.stringify([e.source, e.target]));
  if (new Set(pairKeys).size !== edges.length) fail('unsupported-factory-topology', 'Parallel internal requirements are not supported by this stage');
  const reachable = new Map();
  for (const id of nodes.keys()) {
    const seen = new Set(), visit = n => { if (seen.has(n)) return; seen.add(n); for (const e of outgoing.get(n)) visit(e.target); };
    visit(id); reachable.set(id, seen);
  }
  const components = [], owner = new Map();
  for (const id of nodes.keys()) if (!owner.has(id)) {
    const members = [...nodes.keys()].filter(n => reachable.get(id).has(n) && reachable.get(n).has(id));
    if (members.length > 2) fail('unsupported-factory-topology', 'Only singleton and reciprocal two-node components are supported', members);
    const component = { members, suppliers: [], consumers: [] }; components.push(component); for (const n of members) owner.set(n, component);
  }
  for (const c of components) {
    c.consumers = [...new Set(edges.filter(e => owner.get(e.source) === c && owner.get(e.target) !== c).map(e => owner.get(e.target)))];
    c.suppliers = [...new Set(edges.filter(e => owner.get(e.target) === c && owner.get(e.source) !== c).map(e => owner.get(e.source)))];
    if (c.consumers.length > 1) fail('unsupported-factory-topology', 'The component graph must be an upstream tree, not a reconvergent DAG', c.members);
  }
  const sinks = components.filter(c => !c.consumers.length);
  if (sinks.length !== 1 || sinks[0].members.length !== 1) fail('unsupported-factory-topology', 'Expected one singleton consumer sink');
  const describe = c => {
    if (c.signature) return;
    for (const supplier of c.suppliers) describe(supplier);
    c.depth = c.members.length + Math.max(0, ...c.suppliers.map(s => s.depth));
    c.signature = JSON.stringify([c.members.map(id => { const n = nodes.get(id); return [n.recipe, n.utilization]; }).sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b))),
      c.suppliers.map(s => s.signature).sort()]);
    c.suppliers.sort((a, b) => b.depth - a.depth || a.signature.localeCompare(b.signature) || [...a.members].sort()[0].localeCompare([...b.members].sort()[0]));
  };
  describe(sinks[0]);
  const order = [], transactions = [], growthEdges = [];
  const walk = c => {
    const attachment = edges.filter(e => owner.get(e.source) === c && owner.get(e.target) !== c).sort((a, b) => a.item.localeCompare(b.item) || a.id.localeCompare(b.id))[0];
    const first = attachment?.source ?? c.members[0], members = [first, ...c.members.filter(n => n !== first)];
    for (const [i, id] of members.entries()) {
      if (order.length) growthEdges.push(i === 0 ? attachment : edges.find(e => e.source === id && e.target === first));
      order.push(id);
    }
    transactions.push({ members, depth: c.depth, signature: c.signature });
    for (const supplier of c.suppliers) walk(supplier);
  };
  walk(sinks[0]);
  if (order.length !== nodes.size || growthEdges.some(e => !e)) fail('unsupported-factory-topology', 'Disconnected or unsupported component attachment');
  const lanes = edges.map(e => ({ ...e, kind: 'internal', lane: 0 }));
  for (const e of graph.external) {
    const item = catalogs.items.items.find(i => i.id === e.item), transport = catalogs.transports.transports.find(t => t.kind === item?.transport);
    if (!transport) fail('input-invalid', 'Unknown external transport', e.id);
    const cap = capacity(transport.capacity); let left = parseRate(e.rate);
    if (lanesFor(left, cap) > 64) fail('unsupported-lanes', 'External obligation exceeds the diagnostic lane-count guard', e.id);
    for (let lane = 0; left.n; lane++) {
      const value = compare(left, cap) > 0 ? cap : left;
      lanes.push({ ...e, rate: dto(value), kind: 'external', lane, transport: item.transport,
        source: e.direction === 'input' ? externalNode(e.id) : e.instance, target: e.direction === 'input' ? e.instance : externalNode(e.id) });
      left = rate(left.n * value.d - value.n * left.d, left.d * value.d);
    }
  }
  if (new Set(lanes.map(e => e.transport)).size !== 1) fail('unsupported-factory-transport', 'This factory stage supports a single transport kind');
  return { ...graph, order, growthEdges, transactions, lanes, boundary: structuredClone(input.boundary) };
}
