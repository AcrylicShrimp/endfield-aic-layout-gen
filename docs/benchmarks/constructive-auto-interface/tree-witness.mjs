import { rotateFacility } from '../constructive-placement-only/model.mjs';
import { treeModule } from './trees.mjs';
import { polyline } from '../constructive-interconnect/geometry.mjs';
import { rate, add, multiply, capacity, parseRate, dto } from '../constructive-interconnect/rates.mjs';
const key = p => `${p.x},${p.y}`;

// Witness v1 needs a facility. Its unconnected recipe is explicitly pending:
// these probes validate only boundary-to-boundary component geometry.
export function treeBoundaryWitness(catalogs, anchorRecipe, transport, kind, count) {
  const recipe = catalogs.recipes.recipes.find(r => r.id === anchorRecipe);
  const facility = catalogs.facilities.facilities.find(f => f.id === recipe.facility);
  const shape = rotateFacility(facility, facility.allowed_rotations[0]);
  const cap = capacity(catalogs.transports.transports.find(t => t.kind === transport).capacity);
  const item = catalogs.items.items.find(i => i.transport === transport).id;
  const group = { id: 'probe', item, lanes: Array.from({ length: count }, (_, i) =>
    ({ id: `leaf-${i}`, item, transport, rate: dto(multiply(cap, rate(1, count))) })) };
  const module = treeModule(group, kind, { x: shape.width + 3, y: shape.height + 3 }, catalogs), east = module.x + module.width;
  const cells = new Map(), segments = new Map(), terminals = [];
  const terminal = (id, position, direction, side, value) => terminals.push({ id, node: id, direction, position,
    endpoint: { kind: 'external', node: id, side }, rate: dto(value) });
  const split = kind === 'splitter';
  terminal('root', { x: split ? 0 : east, y: module.root.y }, split ? 'output' : 'input', split ? 'west' : 'east', cap);
  for (const [i, p] of module.paths.entries()) {
    const value = parseRate(group.lanes.find(l => l.id === p.id).rate), y = split ? p.cells.at(-1).y : p.cells[0].y;
    terminal(`leaf-${i}`, { x: split ? east : 0, y }, split ? 'input' : 'output', split ? 'east' : 'west', value);
    const line = polyline([[0, split ? module.root.y : y], ...p.cells.map(p => [p.x, p.y]), [east, split ? y : module.root.y]]);
    for (const p of line) cells.set(key(p), p);
    for (let j = 1; j < line.length; j++) {
      const from = line[j - 1], to = line[j], tag = key(from) + '>' + key(to), previous = segments.get(tag);
      segments.set(tag, { from, to, rate: dto(add(previous ? parseRate(previous.rate) : rate(0), value)) });
    }
  }
  return { schema_version: 1,
    placements: [{ instance: 'anchor', recipe: recipe.id, facility: facility.id, x: 1, y: 1, width: shape.width, height: shape.height, rotation: shape.rotation }],
    utilizations: { anchor: { numerator: 1, denominator: 1 } },
    transport_networks: [{ id: 'tree', item, transport, requirement_ids: group.lanes.map(l => l.id), cells: [...cells.values()],
      segments: [...segments.values()], terminals, component_ids: module.components.map(c => c.id) }],
    logistics_components: module.components.map(({ items, ...c }) => c) };
}
