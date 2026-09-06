import { capacity, compare, parseRate } from './rates.mjs';
import { planTracks } from './tracks.mjs';
const key = p => `${p.x},${p.y}`;
const directions = ['north', 'east', 'south', 'west'];
const direction = (a, b) => b.x > a.x ? 'east' : b.x < a.x ? 'west' : b.y > a.y ? 'south' : 'north';
const rotate = (d, r) => directions[(directions.indexOf(d) + r / 90) % 4];
const unit = (a, b) => Math.abs(a.x - b.x) + Math.abs(a.y - b.y) === 1;
const fail = (code, entity, message) => { throw Object.assign(new Error(message), { code, entity, stage: 'structural-interconnect', severity: 'error' }); };

export function polyline(points) {
  const out = [{ x: points[0][0], y: points[0][1] }];
  for (const [x, y] of points.slice(1)) {
    let a = out.at(-1);
    if (a.x !== x && a.y !== y) throw new Error('Nonorthogonal polyline');
    while (a.x !== x || a.y !== y) { a = { x: a.x + Math.sign(x - a.x), y: a.y + Math.sign(y - a.y) }; out.push(a); }
  }
  return out;
}

// Plan the entire monotone channel, then materialize only its actual crossings.
export function permutationModule(lanes, target, catalogs, maxCrossings = 10000) {
  if (!lanes.length || new Set(lanes.map(l => l.id)).size !== lanes.length || target.length !== lanes.length ||
      new Set(target).size !== lanes.length || target.some(id => !lanes.some(l => l.id === id)))
    fail('interconnect-invalid-permutation', null, 'Expected a bijection of nonempty unique lane IDs');
  if (!Number.isSafeInteger(maxCrossings) || maxCrossings < 0) fail('interconnect-invalid-budget', null, 'Invalid crossing resource budget');
  const transport = lanes[0].transport, source = lanes.map(l => l.id), dest = new Map(target.map((id, i) => [id, i]));
  if (lanes.some(l => l.transport !== transport)) fail('interconnect-mixed-layer', null, 'Compile belt and pipe permutations separately');
  let inversions = 0;
  for (let i = 0; i < lanes.length; i++) for (let j = i + 1; j < lanes.length; j++) if (dest.get(lanes[i].id) > dest.get(lanes[j].id)) {
    if (lanes[i].item === lanes[j].item) fail('interconnect-same-item-order', lanes[j].id, 'Preserve same-item lane order or reallocate compatible flow upstream');
    if (++inversions > maxCrossings) fail('interconnect-crossing-budget', null, 'Research crossing budget exhausted; no infeasibility claim');
  }
  const verticalSteps = source.reduce((n, id, i) => n + 3 * Math.abs(dest.get(id) - i), 0);
  if (lanes.length * 2 + verticalSteps > 200000) fail('interconnect-cell-budget', null, 'Diagnostic path-cell lower bound exceeds budget; no infeasibility claim');
  const plan = planTracks(source, target), pathCells = lanes.length * plan.width + verticalSteps;
  if (pathCells > 200000) fail('interconnect-cell-budget', { path_cells: pathCells }, 'Diagnostic path-cell budget exhausted; no infeasibility claim');
  const paths = lanes.map((l, i) => ({ ...l, cells: polyline(plan.lanes[i].points) }));
  const visits = new Map(), modules = [], components = [];
  for (const lane of paths) for (const [index, p] of lane.cells.entries()) {
    const k = key(p), xs = visits.get(k) ?? []; xs.push({ lane, index, p }); visits.set(k, xs);
  }
  const poses = catalogs.components.components.filter(c => c.transport === transport && c.kind === 'bridge' && c.footprint.width === 1 && c.footprint.height === 1)
    .flatMap(c => c.allowed_rotations.map(rotation => ({ c, rotation })));
  for (const xs of visits.values()) if (xs.length > 1) {
    if (xs.length !== 2) fail('interconnect-track-collision', xs[0].p, 'More than two channels intersect');
    const pose = poses.find(({ c, rotation }) => xs.every(({ lane, index, p }) => index > 0 && index < lane.cells.length - 1 &&
      c.input_directions.map(d => rotate(d, rotation)).includes(direction(p, lane.cells[index - 1])) &&
      c.output_directions.map(d => rotate(d, rotation)).includes(direction(p, lane.cells[index + 1]))));
    if (!pose) fail('interconnect-no-bridge', xs[0].p, 'No catalog bridge pose supports this monotone crossing');
    const id = `crossing-${components.length}`, position = { ...xs[0].p };
    modules.push({ id, ...position, width: 1, height: 1, lanes: xs.map(x => x.lane.id) });
    components.push({ id, component: pose.c.id, kind: 'bridge', transport, rotation: pose.rotation, position });
  }
  if (components.length !== inversions) fail('interconnect-track-crossing-count', null, 'Direct channel introduced avoidable crossings or omitted an inversion');
  const result = { schema_version: 2, status: 'permutation-geometry-only', transport, width: plan.width, height: plan.height,
    construction: { family: 'monotone-tracks', track_order: plan.order, track_columns: plan.columns, precedence: plan.constraints, inversions,
      path_cells: pathCells, scope: 'Manhattan-shortest fixed-terminal lanes and one crossing per inversion; not global minimum area' },
    source_order: source, target_order: target, modules, components,
    lanes: paths, game_validated: false, factory_validated: false,
    pending: ['capacity-aware-fanout-fanin', 'facility-port-binding', 'independent-production-validator'] };
  result.certificate = checkGeometry(result, catalogs);
  if (!result.certificate.valid) fail('interconnect-geometry-rejected', null, JSON.stringify(result.certificate.issues));
  return result;
}

// Independent cell-walk checker: it does not use the swap sequence or template to infer validity.
export function checkGeometry(result, catalogs) {
  const issues = [], addIssue = (code, entity) => issues.push({ code, severity: 'error', stage: 'interconnect-geometry-check', entity });
  const cells = new Map(), bridgeMap = new Map(), moduleMap = new Map(result.modules.map(m => [m.id, m]));
  const laneIds = result.lanes.map(l => l.id), equalSet = xs => xs.length === laneIds.length && new Set(xs).size === laneIds.length && xs.every(id => laneIds.includes(id));
  if (new Set(laneIds).size !== laneIds.length || !equalSet(result.source_order) || !equalSet(result.target_order)) addIssue('invalid-lane-membership', null);
  if (moduleMap.size !== result.modules.length || result.modules.length !== result.components.length) addIssue('invalid-module-membership', null);
  for (const [i, m] of result.modules.entries()) {
    if (![m.x, m.y, m.width, m.height].every(Number.isSafeInteger) || m.x < 0 || m.y < 0 || m.width <= 0 || m.height <= 0 ||
        m.x + m.width > result.width || m.y + m.height > result.height || m.lanes.length !== 2 || new Set(m.lanes).size !== 2 || m.lanes.some(id => !laneIds.includes(id))) addIssue('invalid-reserved-envelope', m.id);
    for (const n of result.modules.slice(i + 1)) if (m.x < n.x + n.width && n.x < m.x + m.width && m.y < n.y + n.height && n.y < m.y + m.height) addIssue('overlapping-reserved-modules', m.id);
  }
  const catalogItems = new Map(catalogs.items.items.map(i => [i.id, i]));
  const cap = catalogs.transports.transports.find(t => t.kind === result.transport);
  for (const c of result.components) {
    if (bridgeMap.has(key(c.position))) addIssue('duplicate-bridge', c.id);
    bridgeMap.set(key(c.position), c);
  }
  for (const lane of result.lanes) {
    const seen = new Set(), points = lane.cells;
    if (!cap || catalogItems.get(lane.item)?.transport !== lane.transport || lane.transport !== result.transport ||
        parseRate(lane.rate).n === 0n || compare(parseRate(lane.rate), capacity(cap.capacity)) > 0) addIssue('lane-commodity-or-capacity', lane.id);
    if (points.length < 2 || points[0].x !== 0 || points.at(-1).x !== result.width - 1 ||
        points[0].y !== result.source_order.indexOf(lane.id) * 3 || points.at(-1).y !== result.target_order.indexOf(lane.id) * 3 ||
        direction(points[0], points[1]) !== 'east' || direction(points.at(-2), points.at(-1)) !== 'east') addIssue('boundary-contract', lane.id);
    for (const [i, p] of points.entries()) {
      if (!Number.isSafeInteger(p.x) || !Number.isSafeInteger(p.y) || p.x < 0 || p.y < 0 || p.x >= result.width || p.y >= result.height)
        addIssue('outside-envelope', lane.id);
      if (seen.has(key(p))) addIssue('lane-self-intersection', lane.id); seen.add(key(p));
      for (const m of result.modules) if (p.x >= m.x && p.x < m.x + m.width && p.y >= m.y && p.y < m.y + m.height && !m.lanes.includes(lane.id)) addIssue('unrelated-lane-in-reserved-module', lane.id);
      if (i && !unit(points[i - 1], p)) addIssue('non-unit-step', lane.id);
      const list = cells.get(key(p)) ?? []; list.push({ lane, i, p }); cells.set(key(p), list);
    }
  }
  for (const [position, visits] of cells) {
    const component = bridgeMap.get(position);
    if (visits.length === 1 && !component) continue;
    if (!component || visits.length !== 2) { addIssue('unplanned-or-single-channel-crossing', position); continue; }
    const definition = catalogs.components.components.find(c => c.id === component.component), module = moduleMap.get(component.id);
    if (!definition || definition.kind !== 'bridge' || definition.transport !== result.transport || component.transport !== result.transport || component.kind !== 'bridge' ||
        definition.footprint.width !== 1 || definition.footprint.height !== 1 || !definition.allowed_rotations.includes(component.rotation)) {
      addIssue('invalid-bridge-catalog', component.id); continue;
    }
    if (!module || visits.some(v => !module.lanes.includes(v.lane.id) || v.p.x < module.x || v.p.x >= module.x + module.width || v.p.y < module.y || v.p.y >= module.y + module.height)) addIssue('unreserved-bridge', component.id);
    const axes = [];
    for (const { lane, i, p } of visits) {
      const a = lane.cells[i - 1], b = lane.cells[i + 1];
      if (!a || !b || !unit(a, p) || !unit(p, b) || direction(a, p) !== direction(p, b)) { addIssue('turn-at-bridge', lane.id); continue; }
      const input = direction(p, a), output = direction(p, b);
      if (!definition.input_directions.map(d => rotate(d, component.rotation)).includes(input) ||
          !definition.output_directions.map(d => rotate(d, component.rotation)).includes(output) ||
          compare(parseRate(lane.rate), capacity(definition.capacity)) > 0) addIssue('bridge-arm-or-capacity', lane.id);
      axes.push(a.y === p.y);
    }
    if (visits[0].lane.item === visits[1].lane.item || axes.length !== 2 || axes[0] === axes[1]) addIssue('non-independent-channels', component.id);
  }
  for (const [p, c] of bridgeMap) if (!cells.has(p)) addIssue('unused-bridge', c.id);
  return { valid: issues.length === 0, issues, physical_tiles: cells.size, independent_game_validation: false };
}
