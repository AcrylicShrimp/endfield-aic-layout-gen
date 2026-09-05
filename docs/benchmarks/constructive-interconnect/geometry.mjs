import { capacity, compare, parseRate } from './rates.mjs';
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

// Reserve each swap's geometry first; lowering only fills known template cells, never searches.
export function permutationModule(lanes, target, catalogs, maxSwaps = 10000) {
  if (!lanes.length || new Set(lanes.map(l => l.id)).size !== lanes.length || target.length !== lanes.length ||
      new Set(target).size !== lanes.length || target.some(id => !lanes.some(l => l.id === id)))
    fail('interconnect-invalid-permutation', null, 'Expected a bijection of nonempty unique lane IDs');
  if (!Number.isSafeInteger(maxSwaps) || maxSwaps < 0) fail('interconnect-invalid-budget', null, 'Invalid swap resource budget');
  const transport = lanes[0].transport, order = lanes.map(l => l.id), byId = new Map(lanes.map(l => [l.id, l]));
  if (lanes.some(l => l.transport !== transport)) fail('interconnect-mixed-layer', null, 'Compile belt and pipe permutations separately');
  const bridge = catalogs.components.components.find(c => c.transport === transport && c.kind === 'bridge' &&
    c.footprint.width === 1 && c.footprint.height === 1 && c.allowed_rotations.includes(0) &&
    ['west', 'south'].every(d => c.input_directions.includes(d)) && ['east', 'north'].every(d => c.output_directions.includes(d)));
  if (!bridge) fail('interconnect-no-bridge', transport, 'No matching catalog bridge template');
  const swaps = [];
  for (let i = 0; i < target.length; i++) {
    let j = order.indexOf(target[i]);
    while (j > i) {
      if (byId.get(order[j - 1]).item === byId.get(order[j]).item)
        fail('interconnect-same-item-order', order[j], 'Preserve same-item lane order or reallocate compatible flow upstream');
      if (swaps.length >= maxSwaps) fail('interconnect-swap-budget', null, 'Research swap budget exhausted; no infeasibility claim');
      swaps.push(j - 1); [order[j - 1], order[j]] = [order[j], order[j - 1]]; j--;
    }
  }
  // Bound diagnostic materialization independently of swap count; this is not a game limit.
  if (lanes.length * Math.max(1, swaps.length) * 16 > 200000)
    fail('interconnect-cell-budget', null, 'Conservative diagnostic path-cell budget exhausted; no infeasibility claim');
  const stages = [], last = Array(lanes.length).fill(-1);
  for (const [ordinal, upper] of swaps.entries()) {
    const stage = Math.max(last[upper], last[upper + 1]) + 1;
    (stages[stage] ??= []).push({ ordinal, upper }); last[upper] = last[upper + 1] = stage;
  }
  if (!stages.length) stages.push([]);
  const paths = new Map(lanes.map(l => [l.id, []])), current = lanes.map(l => l.id), modules = [], components = [];
  for (const [stage, active] of stages.entries()) {
    const x = stage * 6, byPosition = new Map(active.flatMap(s => [[s.upper, s], [s.upper + 1, s]]));
    for (let i = 0; i < current.length; i++) {
      const swap = byPosition.get(i), upper = swap?.upper, y = upper * 3;
      const points = swap && i === upper ? [[0, y], [2, y], [2, y + 3], [5, y + 3]] :
        swap ? [[0, y + 3], [1, y + 3], [1, y + 5], [3, y + 5], [3, y], [5, y]] : [[0, i * 3], [5, i * 3]];
      paths.get(current[i]).push(...polyline(points).map(p => ({ x: p.x + x, y: p.y })));
    }
    for (const { ordinal, upper } of active) {
      const id = `swap-${ordinal}`, y = upper * 3;
      modules.push({ id, x, y, width: 6, height: 6, lanes: [current[upper], current[upper + 1]] });
      components.push({ id, component: bridge.id, kind: bridge.kind, transport, rotation: 0, position: { x: x + 3, y: y + 3 } });
      [current[upper], current[upper + 1]] = [current[upper + 1], current[upper]];
    }
  }
  if (current.some((id, i) => id !== target[i])) fail('interconnect-stage-order', null, 'Scheduled swaps changed the target permutation');
  const result = { schema_version: 1, status: 'permutation-geometry-only', transport,
    width: stages.length * 6, height: (lanes.length - 1) * 3 + 3,
    schedule: { stages, stage_count: stages.length, serial_stage_count: Math.max(1, swaps.length), scope: 'minimum depth for the retained position-dependency order, not global minimum area' },
    source_order: lanes.map(l => l.id), target_order: target, modules, components,
    lanes: lanes.map(l => ({ ...l, cells: paths.get(l.id) })), game_validated: false, factory_validated: false,
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
