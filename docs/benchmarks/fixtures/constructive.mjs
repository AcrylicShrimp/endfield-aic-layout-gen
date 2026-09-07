// Test-only input assembly. No planner, router, or historical report is invoked.
import fs from 'node:fs';
import { createHash } from 'node:crypto';
import { rotateFacility } from '../constructive-placement-only/model.mjs';
import { normalizeGraph } from '../constructive-cycle-blocks/graph.mjs';

const read = name => JSON.parse(fs.readFileSync(new URL(name, import.meta.url)));
const catalogBytes = name => fs.readFileSync(new URL(`../../../data/game/normalized/${name}.json`, import.meta.url));
const delta = { north: [0, -1], east: [1, 0], south: [0, 1], west: [-1, 0] };
const fail = message => { throw new Error(`Constructive test fixture: ${message}`); };
const identity = (data, ref) => {
  if (!Object.hasOwn(data.identities, ref)) fail(`unknown identity reference ${ref}`);
  return data.identities[ref];
};
const expandEdge = (data, edge) => ({ ...edge, source: identity(data, edge.source), target: identity(data, edge.target) });

export function loadGraphSnapshot() {
  const data = read('./graph.json');
  if (data.schema_version !== 1 || new Set(Object.values(data.identities)).size !== Object.keys(data.identities).length)
    fail('unsupported schema or duplicate expanded identity');
  return { schema_version: 1,
    placements: data.facilities.map(p => ({ instance: identity(data, p.ref), label: p.ref, recipe: p.recipe, facility: p.facility })),
    internal_requirements: data.internal_requirements.map(e => expandEdge(data, e)),
    external_requirements: data.external_requirements.map(e => expandEdge(data, e)) };
}

export function loadBankGraph(catalogs) {
  const data = read('./graph.json'), bank = read('./banks.json'), graph = normalizeGraph(loadGraphSnapshot(), catalogs);
  if (bank.schema_version !== 1) fail('unsupported banks schema');
  const edges = new Map(graph.edges.map(e => [e.id, e]));
  return { ...graph, edges: bank.edges.map(e => {
    if (!Object.hasOwn(e, 'base')) return expandEdge(data, e);
    if (!edges.has(e.base)) fail(`unknown base requirement ${e.base}`);
    return structuredClone(edges.get(e.base));
  }) };
}

export function loadOrderFixture() {
  return read('./order-certificate.json');
}

export function loadPlacementFixture(variant) {
  if (!['heavy', 'gap6'].includes(variant)) fail(`unknown placement variant ${variant}`);
  const data = read('./graph.json'), layout = read(`./${variant}-layout.json`), graph = loadGraphSnapshot();
  if (layout.schema_version !== 1) fail('unsupported layout schema');
  const bytes = Object.fromEntries(['facilities', 'items'].map(k => [k, catalogBytes(k)]));
  const facilities = JSON.parse(bytes.facilities), items = JSON.parse(bytes.items);
  const specs = new Map(facilities.facilities.map(f => [f.id, f]));
  const kinds = new Map(items.items.map(i => [i.id, i.transport]));
  const nodes = new Map(graph.placements.map(p => [p.instance, p]));
  const requirements = [...graph.internal_requirements, ...graph.external_requirements];
  const placements = layout.placements.map(([ref, x, y, rotation], placement_index) => {
    const instance = identity(data, ref), node = nodes.get(instance), spec = specs.get(node?.facility);
    if (!spec || !spec.allowed_rotations.includes(rotation) || ![x, y].every(Number.isSafeInteger))
      fail(`invalid catalog rotation, facility, or coordinates for ${ref}`);
    const shape = rotateFacility(spec, rotation);
    const groups = new Map();
    for (const e of requirements) for (const direction of ['input', 'output']) {
      if (e[direction === 'input' ? 'target' : 'source'] !== instance) continue;
      const transport = kinds.get(e.item);
      if (!transport) fail(`unknown item ${e.item}`);
      groups.set(`${direction}:${transport}:${e.item}`, { direction, transport });
    }
    // Match the diagnostic's distinct commodity/direction groups at the frozen rotation.
    const alignment = {};
    for (const kind of ['belt', 'pipe']) for (const direction of ['output', 'input']) {
      const required = [...groups.values()].filter(g => g.transport === kind && g.direction === direction).length;
      alignment[`${kind}_${direction}`] = { required, aligned: shape.ports.some(p => p.transport === kind
        && p.direction === direction && p.edge === (direction === 'output' ? 'east' : 'west')) ? required : 0 };
    }
    return { ...node, x, y, width: shape.width, height: shape.height, rotation, placement_index, alignment,
      catalog_ports: shape.ports.map(p => ({ ...p, position: { x: x + p.position.x, y: y + p.position.y },
        connection: { x: x + p.position.x + delta[p.edge][0], y: y + p.position.y + delta[p.edge][1] } })) };
  });
  const world = new Map(placements.map(p => [p.instance, p]));
  if (world.size !== nodes.size || placements.length !== nodes.size || !world.has(identity(data, layout.target)))
    fail('layout must contain every facility exactly once and a known target');
  for (let i = 0; i < placements.length; i++) for (let j = i + 1; j < placements.length; j++) {
    const a = placements[i], b = placements[j];
    if (!(a.x + a.width + 1 <= b.x || b.x + b.width + 1 <= a.x || a.y + a.height + 1 <= b.y || b.y + b.height + 1 <= a.y))
      fail(`facility clearance failed for ${a.label}/${b.label}`);
  }
  const blocks = layout.blocks?.map(b => {
    const members = b.members.map(ref => identity(data, ref)), ps = members.map(id => world.get(id));
    if (new Set(members).size !== members.length) fail(`duplicate member in ${b.id}`);
    if (ps.some(p => !p || p.block !== undefined)) fail(`missing or multiply owned member in ${b.id}`);
    ps.forEach(p => { p.block = b.id; });
    const x = Math.min(...ps.map(p => p.x)), y = Math.min(...ps.map(p => p.y));
    return { id: b.id, members, x, y, width: Math.max(...ps.map(p => p.x + p.width)) - x,
      height: Math.max(...ps.map(p => p.y + p.height)) - y, channel: b.channel };
  });
  if (blocks && (new Set(blocks.map(b => b.id)).size !== blocks.length || placements.some(p => p.block === undefined)))
    fail('block ownership must cover each facility exactly once');
  return { ...graph, schema_version: 1, kind: 'placement-only-visual-diagnostic', success: true,
    target_instance: identity(data, layout.target), required_facilities: placements.length, placements,
    placement_geometry_valid: true, routing_performed: false, factory_validated: false,
    bounds: { width: Math.max(...placements.map(p => p.x + p.width)), height: Math.max(...placements.map(p => p.y + p.height)) },
    pipe_relations: graph.internal_requirements.filter(e => kinds.get(e.item) === 'pipe').map(e => {
      const a = world.get(e.source), b = world.get(e.target);
      const faces = [...new Set(b.catalog_ports.filter(p => p.transport === 'pipe' && p.direction === 'input').map(p => p.edge))];
      return { edge: e.id, source: e.source, target: e.target, consumer_input_faces: faces,
        north_preference_applicable: faces.includes('north'), supplier_north: a.y + a.height + 1 <= b.y };
    }),
    provenance: { inputs: Object.fromEntries(Object.entries(bytes).map(([key, value]) => [key,
      { sha256: createHash('sha256').update(value).digest('hex') }])) },
    ...(blocks ? { block_experiment: { blocks } } : {}) };
}
