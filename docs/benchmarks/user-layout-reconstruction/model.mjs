import { rotateFacility } from '../constructive-placement-only/model.mjs';
import { polyline } from '../constructive-interconnect/geometry.mjs';
import { add, compare, multiply, rate, parseRate, dto } from '../constructive-interconnect/rates.mjs';
import { usedBounds } from '../constructive-bound-interconnect/view.mjs';
const delta = { north: [0, -1], east: [1, 0], south: [0, 1], west: [-1, 0] };
const key = p => `${p.x},${p.y}`;
const fail = (message, entity = null) => { throw Object.assign(new Error(message), {
  code: 'reconstruction-input', severity: 'error', stage: 'screenshot-reconstruction', entity }); };
const position = p => Array.isArray(p) && p.length === 2 && p.every(n => Number.isSafeInteger(n) && n >= 0);
const positiveRate = (r, entity) => {
  if (!r || ![r.numerator, r.denominator].every(n => Number.isSafeInteger(n) && n > 0)) fail('Expected positive exact rational rate', entity);
  return parseRate(r);
};

export function reconstruct(spec, catalogs, buildings) {
  if (spec.schema_version !== 2 || !spec.placements?.length || !Array.isArray(spec.routes) || !Array.isArray(spec.expected_pending))
    fail('Expected schema 2 with explicit placement utilizations, route rates, and expected_pending; migrate schema 1');
  if ('utilization_assumption' in spec) fail('Removed uniform utilization_assumption; use each placement utilization');
  const witness = { schema_version: 1, placements: [], utilizations: {}, transport_networks: [], logistics_components: [] };
  const catalog_ports = {}, recipes = new Map(), used = new Set(), routes = [], networks = new Map();
  const assigned = new Map();
  for (const p of spec.placements) {
    const recipe = catalogs.recipes.recipes.find(r => r.id === p.recipe), facility = catalogs.facilities.facilities.find(f => f.id === recipe?.facility);
    if (!p.id || recipes.has(p.id) || !position(p.position) || !facility?.allowed_rotations.includes(p.rotation)) fail('Invalid facility pose, recipe or duplicate identity', p.id);
    const shape = rotateFacility(facility, p.rotation), [x, y] = p.position;
    const u = positiveRate(p.utilization, p.id);
    if (u.n > u.d) fail('Expected utilization in (0,1]', p.id);
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
    const amount = recipe.outputs.find(a => a.item === r.item), item = catalogs.items.items.find(i => i.id === amount?.item);
    if (!item || item.transport !== source.transport || source.transport !== target.transport || !recipes.get(r.target[0]).inputs.some(a => a.item === item.id)) fail('Recipe or transport mismatch', r.id);
    const value = dto(positiveRate(r.rate, r.id));
    for (const [end, direction] of [[r.source, 'output'], [r.target, 'input']]) {
      const id = JSON.stringify([end[0], direction, item.id]), rec = recipes.get(end[0]);
      const quantity = rec[direction === 'output' ? 'outputs' : 'inputs'].find(a => a.item === item.id).quantity;
      const available = multiply(rate(BigInt(quantity) * 1000n, rec.duration_ms), parseRate(witness.utilizations[end[0]]));
      const sum = add(assigned.get(id) ?? rate(0), parseRate(value));
      if (compare(sum, available) > 0) fail('Assigned route rates exceed the utilized recipe margin', [end[0], direction, item.id]);
      assigned.set(id, sum);
    }
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
    labels: Object.fromEntries(spec.placements.map(p => [p.id, p.label ?? p.id])), port_groups: { sources: [], targets: [] },
    reconstruction: { id: spec.id, calibration: spec.calibration, rate_assumptions: spec.rate_assumptions,
      utilizations: structuredClone(witness.utilizations), expected_pending: structuredClone(spec.expected_pending), annotations, routes, unobserved: spec.unobserved,
      metrics: { bounds, area: bounds.width * bounds.height, facility_cells: witness.placements.reduce((s, p) => s + p.width * p.height, 0),
        physical_belt_cells: new Set(witness.transport_networks.flatMap(n => n.cells.map(key))).size, turns_including_port_arms: routes.reduce((s, r) => s + r.turns_including_port_arms, 0),
        annotation_cells: annotations.reduce((s, a) => s + a.width * a.height, 0) } } };
}

// The independent validator derives actual margins; the input declares the intended boundary.
// Equality catches missing internal routes even when covered geometry alone is legal.
export function verifyReferenceMargins(spec, validation) {
  if (!validation.covered_geometry_validated || !Array.isArray(validation.recipe_terminals)) fail('Independent covered-geometry validation did not pass', spec.id);
  const normalize = (rows, expected) => {
    const seen = new Set();
    return rows.map(t => {
      const id = JSON.stringify([t.instance, t.direction, t.item]);
      if (!spec.placements.some(p => p.id === t.instance) || !['input', 'output'].includes(t.direction) || !t.item || seen.has(id)) fail('Invalid or duplicate reference margin', id);
      seen.add(id); const value = positiveRate(expected ? t.rate : t.pending_rate, id);
      return [id, `${value.n}/${value.d}`];
    }).sort((a, b) => a[0].localeCompare(b[0]));
  };
  const expected = normalize(spec.expected_pending, true);
  const actual = normalize(validation.recipe_terminals.filter(t => t.pending_rate.numerator > 0), false);
  if (JSON.stringify(expected) !== JSON.stringify(actual)) fail('Pending recipe margins do not equal the declared external obligations', { expected, actual });
  return { status: 'internal-reference-validated', factory_validated: false, expected_pending: spec.expected_pending };
}
