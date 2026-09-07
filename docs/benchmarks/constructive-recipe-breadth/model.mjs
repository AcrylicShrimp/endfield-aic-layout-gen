import { normalizeGraph } from '../constructive-cycle-blocks/graph.mjs';
import { rotateFacility } from '../constructive-placement-only/model.mjs';
import { project, pendingOutputAccess } from '../constructive-factory-closure/model.mjs';
import { placeRegions } from '../constructive-block-composition/model.mjs';
import { usedBounds } from '../constructive-bound-interconnect/view.mjs';
import { compare, parseRate } from '../constructive-interconnect/rates.mjs';
const fail = (code, message, entity = null) => { throw Object.assign(new Error(message), { code, severity: 'error', stage: 'recipe-breadth', entity }); };

export function graphFromWiring(wiring, catalogs) {
  if (wiring.schema_version !== 1 || wiring.success !== true || !Array.isArray(wiring.nodes) || !Array.isArray(wiring.edges))
    fail('breadth-wiring-schema', 'Expected successful schema-1 facility-instance wiring');
  const nodes = new Map(wiring.nodes.map(n => [n.id, n]));
  if (nodes.size !== wiring.nodes.length || wiring.nodes.some(n => typeof n.id !== 'string' || !n.id || !['facility', 'external', 'target', 'surplus'].includes(n.kind)))
    fail('breadth-wiring-nodes', 'Expected unique typed wiring nodes');
  const facilities = wiring.nodes.filter(n => n.kind === 'facility'), ids = new Set(facilities.map(n => n.id));
  for (const e of wiring.edges) {
    const a = nodes.get(e.source), b = nodes.get(e.target);
    if (!a || !b || (!ids.has(e.source) && !ids.has(e.target)) ||
        (!ids.has(e.source) && (a.kind !== 'external' || a.item !== e.item)) ||
        (!ids.has(e.target) && (!['target', 'surplus'].includes(b.kind) || b.item !== e.item)))
      fail('breadth-wiring-endpoint', 'Unknown or incompatible original external endpoint', e.id);
  }
  // Project identities/margins into the existing normalizer; no facility positions are constructed.
  const graph = normalizeGraph({ placements: facilities.map((n, i) => ({ instance: n.id, recipe: n.recipe, facility: n.facility, label: `F${String(i).padStart(2, '0')}` })),
    internal_requirements: wiring.edges.filter(e => ids.has(e.source) && ids.has(e.target)),
    external_requirements: wiring.edges.filter(e => !ids.has(e.source) || !ids.has(e.target)) }, catalogs);
  for (const n of graph.nodes) if (compare(parseRate(n.utilization), parseRate(nodes.get(n.id).work_seconds_per_second)))
    fail('breadth-wiring-utilization', 'Wiring utilization differs from original recipe margins', n.id);
  return graph;
}

export function externalSeed(catalogs, graph, node) {
  if (!graph.nodes.some(n => n.id === node.id) || !graph.edges.some(e => e.target === node.id) || graph.edges.some(e => e.internal && e.target === node.id))
    fail('breadth-seed-inputs', 'An external seed must have only actual original external inputs', node.id);
  const recipe = catalogs.recipes.recipes.find(r => r.id === node.recipe), facility = catalogs.facilities.facilities.find(f => f.id === recipe.facility);
  const shape = facility.allowed_rotations.map(r => rotateFacility(facility, r)).find(s => ['input', 'output'].every(d =>
    s.ports.some(p => p.transport === 'belt' && p.direction === d && p.edge === (d === 'input' ? 'west' : 'east'))));
  if (!shape) fail('breadth-seed-orientation', 'This seed family requires a catalog east-flow belt orientation', node.id);
  const witness = { schema_version: 1, placements: [{ instance: node.id, recipe: node.recipe, facility: recipe.facility,
    x: 0, y: 0, width: shape.width, height: shape.height, rotation: shape.rotation }], utilizations: { [node.id]: { ...node.utilization } },
    transport_networks: [], logistics_components: [] };
  return { schema_version: 1, case: 'actual-graph-external-seed', embedding: 'external-seed', witness, catalog_ports: project(witness, catalogs),
    labels: Object.fromEntries(graph.nodes.map(n => [n.id, n.label])), accepted: false, factory_validated: false,
    port_groups: { sources: [], targets: [] }, pending_external_inputs: graph.edges.filter(e => e.target === node.id).map(e => e.id) };
}

export function regionsForConsumer(graph, regions, target) {
  const needed = new Set(graph.edges.filter(e => e.internal && e.target === target).map(e => e.source));
  const selected = regions.filter(r => r.detail.witness.placements.some(p => needed.has(p.instance)));
  const members = new Set(selected.flatMap(r => r.detail.witness.placements.map(p => p.instance)));
  if (!needed.size || [...needed].some(id => !members.has(id)) || members.has(target)) fail('breadth-consumer-unready', 'Consumer requires retained original suppliers', target);
  return selected;
}

export function joinSupplierRegions(catalogs, graph, regions) {
  const sizes = regions.map(r => ({ id: r.id, bounds: usedBounds(r.detail.witness) })).sort((a, b) => b.bounds.width * b.bounds.height - a.bounds.width * a.bounds.height || a.id.localeCompare(b.id));
  const width = Math.max(...sizes.map(r => r.bounds.width));
  let y = 0;
  const offsets = sizes.map(({ id, bounds: b }) => { const offset = { region: id, x: width - b.width - b.x, y: y - b.y }; y += b.height + 1; return offset; });
  const joined = placeRegions(regions, offsets, catalogs);
  return { schema_version: 1, case: 'actual-graph-supplier-regions', embedding: 'private-supplier-union', accepted: false, factory_validated: false,
    witness: joined.witness, catalog_ports: joined.ports, labels: Object.fromEntries(graph.nodes.map(n => [n.id, n.label])),
    port_groups: { sources: [], targets: [] }, region_translations: joined.translations };
}

export function frontierDepth(graph, target, depths) {
  const inputs = graph.edges.filter(e => e.internal && e.target === target);
  if (!inputs.length || inputs.some(e => !depths.has(e.source))) fail('breadth-frontier-depth', 'Frontier depth requires retained suppliers', target);
  return 1 + Math.max(...inputs.map(e => depths.get(e.source)));
}

// Constructive reservation heuristic, not a proof that a pocket could never hold a consumer.
export function inspectFutureEscape(catalogs, graph, detail) {
  const w = detail.witness, bounds = usedBounds(w), groups = pendingOutputAccess(graph, w, catalogs), diagnostics = [], layers = new Map();
  if ((bounds.width + 2) * (bounds.height + 2) > 250000)
    fail('breadth-escape-grid-budget', 'Future-output escape screening exceeds the local grid budget');
  for (const transport of new Set(groups.map(g => g.transport))) {
    const blocked = new Set();
    for (const p of w.placements) for (let dy = 0; dy < p.height; dy++) for (let dx = 0; dx < p.width; dx++) blocked.add(`${p.x + dx},${p.y + dy}`);
    for (const n of w.transport_networks.filter(n => n.transport === transport)) for (const p of n.cells) blocked.add(`${p.x},${p.y}`);
    for (const c of w.logistics_components.filter(c => c.transport === transport)) blocked.add(`${c.position.x},${c.position.y}`);
    const minX = bounds.x - 1, minY = bounds.y - 1, maxX = bounds.x + bounds.width, maxY = bounds.y + bounds.height;
    const queue = [{ x: minX, y: minY }], reached = new Set([`${minX},${minY}`]);
    for (let i = 0; i < queue.length; i++) for (const [dx, dy] of [[1, 0], [0, 1], [-1, 0], [0, -1]]) {
      const p = { x: queue[i].x + dx, y: queue[i].y + dy }, key = `${p.x},${p.y}`;
      if (p.x < minX || p.x > maxX || p.y < minY || p.y > maxY || reached.has(key) || blocked.has(key)) continue;
      reached.add(key); queue.push(p);
    }
    layers.set(transport, reached);
  }
  const access = groups.map(g => ({ ...g, escaping_ports: g.ports.filter(p => !p.obstruction && layers.get(g.transport).has(`${p.connection.x},${p.connection.y}`)).map(p => p.port) }));
  for (const g of access) if (g.escaping_ports.length < g.required) diagnostics.push({ code: 'breadth-pending-output-escape', severity: 'warning', stage: 'future-output-screen',
    message: 'Constructive candidate does not reserve enough unobstructed exits for its remaining output margins', entity: g.instance, transport: g.transport, required: g.required, escaping_ports: g.escaping_ports });
  return { accepted: !diagnostics.length, diagnostics, access };
}
