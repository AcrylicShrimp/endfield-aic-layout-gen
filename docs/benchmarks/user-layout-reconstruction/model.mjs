import { rotateFacility } from '../constructive-placement-only/model.mjs';
import { polyline } from '../constructive-interconnect/geometry.mjs';
import { multiply, rate, parseRate, dto } from '../constructive-interconnect/rates.mjs';
import { usedBounds } from '../constructive-bound-interconnect/view.mjs';
const delta = { north: [0, -1], east: [1, 0], south: [0, 1], west: [-1, 0] };
const key = p => `${p.x},${p.y}`;
const fail = (message, entity = null) => { throw Object.assign(new Error(message), {
  code: 'reconstruction-input', severity: 'error', stage: 'screenshot-reconstruction', entity }); };
const position = p => Array.isArray(p) && p.length === 2 && p.every(n => Number.isSafeInteger(n) && n >= 0);

export function reconstruct(spec, catalogs, buildings) {
  if (spec.schema_version !== 1 || !spec.placements?.length || !Array.isArray(spec.routes)) fail('Expected schema 1 placements and routes');
  const witness = { schema_version: 1, placements: [], utilizations: {}, transport_networks: [], logistics_components: [] };
  const catalog_ports = {}, recipes = new Map(), used = new Set(), routes = [], networks = new Map();
  const u = parseRate(spec.utilization_assumption);
  if (u.n <= 0n || u.n > u.d) fail('Expected utilization in (0,1]');
  for (const p of spec.placements) {
    const recipe = catalogs.recipes.recipes.find(r => r.id === p.recipe), facility = catalogs.facilities.facilities.find(f => f.id === recipe?.facility);
    if (!p.id || recipes.has(p.id) || !position(p.position) || !facility?.allowed_rotations.includes(p.rotation)) fail('Invalid facility pose, recipe or duplicate identity', p.id);
    const shape = rotateFacility(facility, p.rotation), [x, y] = p.position;
    recipes.set(p.id, recipe);
    witness.placements.push({ instance: p.id, recipe: recipe.id, facility: facility.id, x, y, width: shape.width, height: shape.height, rotation: p.rotation });
    witness.utilizations[p.id] = dto(u);
    catalog_ports[p.id] = shape.ports.map(q => ({ ...q, position: { x: x + q.position.x, y: y + q.position.y },
      connection: { x: x + q.position.x + delta[q.edge][0], y: y + q.position.y + delta[q.edge][1] } }));
  }
  for (const r of spec.routes) {
    if (!r.id || routes.some(p => p.id === r.id) || !r.corners?.length || !r.corners.every(position)) fail('Invalid route identity or corners', r.id);
    if (r.corners.some((p, i) => i > 0 && p[0] !== r.corners[i - 1][0] && p[1] !== r.corners[i - 1][1])) fail('Nonorthogonal route', r.id);
    const [source, target] = [r.source, r.target].map((end, i) => {
      if (!Array.isArray(end) || end.length !== 2) fail('Expected facility/port endpoint', r.id);
      const p = catalog_ports[end[0]]?.find(p => p.id === end[1]);
      if (!p || p.direction !== (i ? 'input' : 'output') || used.has(JSON.stringify(end))) fail('Unknown, reversed or reused port', end);
      used.add(JSON.stringify(end)); return p;
    });
    const recipe = recipes.get(r.source[0]);
    if (recipe.outputs.length !== 1) fail('This transcription supports a single-output source per route', r.id);
    const amount = recipe.outputs[0], item = catalogs.items.items.find(i => i.id === amount.item);
    if (!item || item.transport !== source.transport || source.transport !== target.transport || !recipes.get(r.target[0]).inputs.some(a => a.item === item.id)) fail('Recipe or transport mismatch', r.id);
    const value = dto(multiply(rate(BigInt(amount.quantity) * 1000n, recipe.duration_ms), u));
    const cells = r.corners.length === 1 ? [{ x: r.corners[0][0], y: r.corners[0][1] }] : polyline(r.corners);
    if (key(cells[0]) !== key(source.connection) || key(cells.at(-1)) !== key(target.connection) || new Set(cells.map(key)).size !== cells.length) fail('Route endpoints or repeated cells disagree with transcription', r.id);
    if (!networks.has(item.id)) networks.set(item.id, { id: `network:${item.id}`, item: item.id, transport: item.transport, requirement_ids: [], cells: [], segments: [], terminals: [], component_ids: [] });
    const n = networks.get(item.id); n.requirement_ids.push(r.id); n.cells.push(...cells);
    for (let i = 1; i < cells.length; i++) n.segments.push({ from: cells[i - 1], to: cells[i], rate: value });
    for (const [end, p] of [[r.source, source], [r.target, target]]) n.terminals.push({ id: `${r.id}:${p.direction}`, node: end[0], direction: p.direction,
      endpoint: { kind: 'facility', instance: end[0], port: end[1] }, position: p.connection, rate: value });
    // Include facility-cell center to connection-cell center arms in the visual turn count.
    const path = [source.position, ...cells, target.position]; let turns = 0;
    for (let i = 2; i < path.length; i++) if (path[i].x - path[i - 1].x !== path[i - 1].x - path[i - 2].x || path[i].y - path[i - 1].y !== path[i - 1].y - path[i - 2].y) turns++;
    routes.push({ ...structuredClone(r), item: item.id, rate: value, cells, physical_cells: cells.length, turns_including_port_arms: turns });
  }
  witness.transport_networks = [...networks.values()];
  const annotations = (spec.annotations ?? []).map(a => {
    const range = buildings[a.building]?.range;
    if (!range || !position(a.position) || ![range.width, range.depth].every(n => Number.isSafeInteger(n) && n > 0)) fail('Unknown annotation footprint', a.id);
    return { ...a, x: a.position[0], y: a.position[1], width: range.width, height: range.depth, validated_by_witness: false };
  });
  const occupied = new Set(witness.transport_networks.flatMap(n => n.cells.map(key)));
  for (const p of witness.placements) for (let y = p.y; y < p.y + p.height; y++) for (let x = p.x; x < p.x + p.width; x++) occupied.add(`${x},${y}`);
  for (const a of annotations) for (let y = a.y; y < a.y + a.height; y++) for (let x = a.x; x < a.x + a.width; x++) {
    if (occupied.has(`${x},${y}`)) fail('Inferred annotation overlaps reconstructed geometry', a.id); occupied.add(`${x},${y}`);
  }
  const bounds = usedBounds(witness);
  return { schema_version: 1, case: 'user-screenshot-reconstruction', accepted: false, factory_validated: false, witness, catalog_ports,
    labels: Object.fromEntries(spec.placements.map(p => [p.id, p.id])), port_groups: { sources: [], targets: [] },
    reconstruction: { id: spec.id, calibration: spec.calibration, utilization_assumption: spec.utilization_assumption, annotations, routes, unobserved: spec.unobserved,
      metrics: { bounds, area: bounds.width * bounds.height, facility_cells: witness.placements.reduce((s, p) => s + p.width * p.height, 0),
        physical_belt_cells: new Set(witness.transport_networks.flatMap(n => n.cells.map(key))).size, turns_including_port_arms: routes.reduce((s, r) => s + r.turns_including_port_arms, 0),
        annotation_cells: annotations.reduce((s, a) => s + a.width * a.height, 0) } } };
}
