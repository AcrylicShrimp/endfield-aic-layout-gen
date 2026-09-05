import { rotateFacility } from '../constructive-placement-only/model.mjs';
import { polyline } from '../constructive-interconnect/geometry.mjs';
import { rate, add, compare, parseRate, dto, capacity } from '../constructive-interconnect/rates.mjs';
const key = p => `${p.x},${p.y}`, pair = p => [p.x, p.y];
const delta = { north: [0, -1], east: [1, 0], south: [0, 1], west: [-1, 0] };
const fail = (code, message, entity = null) => { throw Object.assign(new Error(message), { code, severity: 'error', stage: 'factory-closure', entity }); };
const minus = (a, b) => rate(a.n * b.d - b.n * a.d, a.d * b.d);
const distance = (a, b) => Math.abs(a.x - b.x) + Math.abs(a.y - b.y);
const median = a => [...a].sort((x, y) => x - y)[Math.floor((a.length - 1) / 2)];
export const better = (a, b) => { for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return a[i] < b[i]; return false; };
function extentOf(w) {
  const points = [...w.placements.flatMap(p => [p, { x: p.x + p.width - 1, y: p.y + p.height - 1 }]), ...w.transport_networks.flatMap(n => n.cells)];
  return { width: Math.max(...points.map(p => p.x)) + 1, height: Math.max(...points.map(p => p.y)) + 1,
    minX: Math.min(...points.map(p => p.x)), minY: Math.min(...points.map(p => p.y)) };
}
const areaWith = (extent, points) => (Math.max(extent.width, ...points.map(p => p.x + 1)) - Math.min(extent.minX ?? 0, ...points.map(p => p.x))) *
  (Math.max(extent.height, ...points.map(p => p.y + 1)) - Math.min(extent.minY ?? 0, ...points.map(p => p.y)));

export function project(w, catalogs) {
  return Object.fromEntries(w.placements.map(p => [p.instance, rotateFacility(catalogs.facilities.facilities.find(f => f.id === p.facility), p.rotation).ports.map(q => ({ ...q,
    position: { x: p.x + q.position.x, y: p.y + q.position.y }, connection: { x: p.x + q.position.x + delta[q.edge][0], y: p.y + q.position.y + delta[q.edge][1] } }))]));
}
function index(w) {
  const body = new Set(), layers = { belt: new Set(), pipe: new Set() }, used = new Set();
  for (const p of w.placements) for (let y = p.y; y < p.y + p.height; y++) for (let x = p.x; x < p.x + p.width; x++) body.add(`${x},${y}`);
  for (const n of w.transport_networks) {
    for (const p of n.cells) layers[n.transport].add(key(p));
    for (const t of n.terminals) if (t.endpoint.kind === 'facility') used.add(JSON.stringify([t.endpoint.instance, t.endpoint.port]));
  }
  for (const c of w.logistics_components) layers[c.transport].add(key(c.position));
  return { body, layers, used };
}
function shape(node, catalogs) {
  const recipe = catalogs.recipes.recipes.find(r => r.id === node.recipe), f = catalogs.facilities.facilities.find(f => f.id === recipe.facility);
  const s = f.allowed_rotations.map(r => rotateFacility(f, r)).find(s => ['input', 'output'].every(d =>
    s.ports.some(p => p.transport === 'belt' && p.direction === d && p.edge === (d === 'input' ? 'west' : 'east'))));
  if (!s) fail('closure-orientation-unsupported', 'No catalog east-flow belt orientation', node.id);
  return { ...s, recipe };
}
export function ready(graph, w) {
  const members = new Set(w.placements.map(p => p.instance));
  return graph.nodes.filter(n => !members.has(n.id) && graph.edges.filter(e => e.internal && e.target === n.id).every(e => members.has(e.source)));
}
function portsFor(ports, occupied, id, direction, transport) {
  return (ports[id] ?? []).filter(p => p.direction === direction && p.transport === transport && !occupied.used.has(JSON.stringify([id, p.id])));
}

// Enumerate a small family of orthogonal corridor templates, never a cell-search router.
export function routeTemplates(a, b, occupied, transport, extent, boundary = null, family = 'simple') {
  if (!['simple', 'perimeter'].includes(family)) fail('closure-route-family', 'Expected simple or perimeter corridor templates', family);
  const templates = [[pair(a), [b.x, a.y], pair(b)], [pair(a), [a.x, b.y], pair(b)]];
  const xs = new Set([a.x, b.x, Math.floor((a.x + b.x) / 2)]), ys = new Set([a.y, b.y, Math.floor((a.y + b.y) / 2)]);
  for (const d of [-8, -4, -2, -1, 1, 2, 4, 8]) { xs.add(a.x + d); xs.add(b.x + d); ys.add(a.y + d); ys.add(b.y + d); }
  xs.add(extent.width); ys.add(extent.height);
  for (const x of xs) if (x >= 0) templates.push([pair(a), [x, a.y], [x, b.y], pair(b)]);
  for (const y of ys) if (y >= 0) templates.push([pair(a), [a.x, y], [b.x, y], pair(b)]);
  // A reserved transfer row plus a separate ingress/egress column avoids approaching
  // a north-facing input through the consumer body from below (and the dual case).
  if (family === 'perimeter') for (const y of new Set([0, extent.height, Math.max(a.y, b.y) + 1, Math.min(a.y, b.y) - 1])) {
    if (y < 0) continue;
    for (const d of [-8, -4, -2, -1, 1, 2, 4, 8]) {
      if (b.x + d >= 0) templates.push([pair(a), [a.x, y], [b.x + d, y], [b.x + d, b.y], pair(b)]);
      if (a.x + d >= 0) templates.push([pair(a), [a.x + d, a.y], [a.x + d, y], [b.x, y], pair(b)]);
    }
  }
  let selected;
  for (const corners of templates) {
    const length = corners.slice(1).reduce((s, p, i) => s + Math.abs(p[0] - corners[i][0]) + Math.abs(p[1] - corners[i][1]), 1);
    if (length > 10000 || corners.some(p => p.some(v => !Number.isSafeInteger(v) || v < 0) || (boundary && (p[0] >= boundary.width || p[1] >= boundary.height)))) continue;
    const area = areaWith(extent, corners.map(([x, y]) => ({ x, y })));
    if (selected && !better([area, length, 0], selected.score)) continue;
    const cells = polyline(corners), seen = new Set();
    if (cells.some(p => { const k = key(p); if (seen.has(k) || occupied.body.has(k) || occupied.layers[transport].has(k)) return true; seen.add(k); return false; })) continue;
    let turns = 0;
    for (let i = 2; i < cells.length; i++) if (cells[i].x - cells[i - 1].x !== cells[i - 1].x - cells[i - 2].x || cells[i].y - cells[i - 1].y !== cells[i - 1].y - cells[i - 2].y) turns++;
    const candidate = { cells, corners, score: [area, cells.length, turns] };
    if (!selected || better(candidate.score, selected.score)) selected = candidate;
  }
  return selected ?? null;
}
function append(w, occupied, edge, source, target, value, path, ordinal) {
  let n = w.transport_networks.find(n => n.item === edge.item);
  if (!n) { n = { id: `network:${edge.item}`, item: edge.item, transport: edge.transport, requirement_ids: [], cells: [], segments: [], terminals: [], component_ids: [] }; w.transport_networks.push(n); }
  const id = JSON.stringify(['closure', edge.id, ordinal]); n.requirement_ids.push(id);
  for (const [end, direction] of [[source, 'output'], [target, 'input']]) {
    const terminalId = JSON.stringify([id, direction]);
    const endpoint = end.external ? { kind: 'external', node: end.node, side: end.edge } : { kind: 'facility', instance: end.node, port: end.id };
    n.terminals.push({ id: terminalId, node: end.node, endpoint, direction, position: end.connection, rate: dto(value) });
    if (!end.external) occupied.used.add(JSON.stringify([end.node, end.id]));
  }
  n.cells.push(...path.cells); for (const p of path.cells) occupied.layers[edge.transport].add(key(p));
  for (let i = 1; i < path.cells.length; i++) n.segments.push({ from: path.cells[i - 1], to: path.cells[i], rate: dto(value) });
  return { edge: edge.id, source: source.external ? source.node : [source.node, source.id], target: target.external ? target.node : [target.node, target.id], rate: dto(value), corners: path.corners };
}
function connect(w, ports, occupied, edge, catalogs, boundary = null, family = 'simple') {
  let left = parseRate(edge.rate), ordinal = 0; const routes = [];
  const cap = capacity(catalogs.transports.transports.find(t => t.kind === edge.transport).capacity);
  while (left.n) {
    const value = compare(left, cap) > 0 ? cap : left, extent = extentOf(w), own = id => w.placements.some(p => p.instance === id);
    const source = portsFor(ports, occupied, edge.source, 'output', edge.transport).map(p => ({ ...p, node: edge.source }));
    const target = portsFor(ports, occupied, edge.target, 'input', edge.transport).map(p => ({ ...p, node: edge.target }));
    const external = (node, opposite) => {
      const xs = new Set(opposite.flatMap(p => Array.from({ length: 33 }, (_, i) => p.connection.x + i - 16)));
      xs.add(extent.width); xs.add(extent.width + 1);
      const ends = [...xs].filter(x => x >= 0 && (!boundary || x < boundary.width)).flatMap(x => [
        { external: true, node, edge: 'north', connection: { x, y: 0 } },
        ...(boundary ? [{ external: true, node, edge: 'south', connection: { x, y: boundary.height - 1 } }] : [])]);
      if (boundary) {
        const ys = new Set(opposite.flatMap(p => Array.from({ length: 33 }, (_, i) => p.connection.y + i - 16)));
        ends.push(...[...ys].filter(y => y >= 0 && y < boundary.height).map(y => ({ external: true, node, edge: 'east', connection: { x: boundary.width - 1, y } })));
      }
      return ends;
    };
    if (!own(edge.source)) source.push(...external(edge.source, target));
    if (!own(edge.target)) target.push(...external(edge.target, source));
    const pairs = source.flatMap(a => target.map(b => ({ a, b, distance: distance(a.connection, b.connection) })));
    pairs.sort((a, b) => a.distance - b.distance);
    let selected;
    for (const { a, b } of pairs) {
      const minimum = [areaWith(extent, [a.connection, b.connection]), distance(a.connection, b.connection) + 1, 0];
      if (selected && !better(minimum, selected.path.score)) continue;
      const path = routeTemplates(a.connection, b.connection, occupied, edge.transport, extent, boundary, family);
      if (path && (!selected || better(path.score, selected.path.score))) selected = { a, b, path };
    }
    if (!selected) fail('closure-port-template-exhausted', 'No unused port pair and collision-free corridor template', { edge: edge.id, item: edge.item, pairs: pairs.length });
    routes.push(append(w, occupied, edge, selected.a, selected.b, value, selected.path, ordinal++)); left = minus(left, value);
  }
  return routes;
}
export function placements(node, graph, base, catalogs, family = 'forward') {
  if (!['forward', 'two-sided'].includes(family)) fail('closure-placement-family', 'Expected forward or two-sided placement family', family);
  const s = shape(node, catalogs), ports = project(base.witness, catalogs), incoming = graph.edges.filter(e => e.internal && e.target === node.id);
  const supplierPorts = incoming.flatMap(e => ports[e.source].filter(p => p.direction === 'output' && p.transport === e.transport));
  const belt = supplierPorts.filter(p => p.transport === 'belt'), anchors = belt.length ? belt : supplierPorts;
  if (!anchors.length) fail('closure-seed-unsupported', 'Closure requires at least one retained internal supplier', node.id);
  const x = belt.length ? Math.max(...anchors.map(p => p.connection.x)) + 1 : median(anchors.map(p => p.connection.x)) - Math.floor(s.width / 2);
  const y = belt.length ? Math.max(1, median(anchors.map(p => p.connection.y)) - Math.floor(s.height / 2)) : Math.max(...anchors.map(p => p.connection.y)) + 1;
  const positions = [], seen = new Set(), occupied = index(base.witness), bounds = extentOf(base.witness);
  const offsets = [0, 1, 2, 3, 4, 6, 8, 12, 16, 24];
  if (family === 'two-sided') offsets.push(...offsets.filter(x => x > 0).map(x => -x));
  for (const dx of offsets) for (const dy of [0, -1, 1, -2, 2, -4, 4, -8, 8, -12, 12, -24, 24]) {
    const p = { instance: node.id, recipe: node.recipe, facility: s.recipe.facility, x: x + dx, y: y + dy, width: s.width, height: s.height, rotation: s.rotation };
    if (p.x < 0 || p.y < 1 || seen.has(key(p))) continue; seen.add(key(p));
    let collision = false;
    for (let py = p.y; py < p.y + p.height; py++) for (let px = p.x; px < p.x + p.width; px++)
      if (occupied.body.has(`${px},${py}`) || Object.values(occupied.layers).some(l => l.has(`${px},${py}`))) collision = true;
    if (!collision) positions.push({ p, distance: anchors.reduce((sum, a) => sum + distance(a.connection, { x: p.x, y: p.y + s.height / 2 }), 0),
      area: areaWith(bounds, [p, { x: p.x + p.width - 1, y: p.y + p.height - 1 }]) });
  }
  positions.sort((a, b) => a.area - b.area || a.distance - b.distance || a.p.y - b.p.y);
  return positions.map(c => c.p);
}
export function growCandidates(catalogs, graph, base, node, budget = 64, routeFamily = 'simple', placementFamily = 'forward') {
  if (!['simple', 'perimeter'].includes(routeFamily)) fail('closure-route-family', 'Expected simple or perimeter corridor templates', routeFamily);
  if (!Number.isSafeInteger(budget) || budget < 1 || budget > 512) fail('closure-candidate-budget', 'Expected placement trial budget 1..512');
  if (!ready(graph, base.witness).some(n => n.id === node.id)) fail('closure-frontier-unready', 'Internal predecessors must already be retained', node.id);
  const start = performance.now(), candidates = [], failures = []; let trials = 0;
  const copying = { draft_count: 0, draft_ms: 0, materialized_count: 0, materialize_ms: 0 };
  const retainedPorts = project(base.witness, catalogs);
  for (const p of placements(node, graph, base, catalogs, placementFamily).slice(0, budget)) {
    trials++;
    const draftStart = performance.now();
    // connect/append only push into these private containers; retained nested DTOs are read-only.
    const w = { ...base.witness, placements: [...base.witness.placements, p],
      utilizations: { ...base.witness.utilizations, [node.id]: node.utilization },
      transport_networks: base.witness.transport_networks.map(n => ({ ...n,
        requirement_ids: [...n.requirement_ids], cells: [...n.cells], segments: [...n.segments],
        terminals: [...n.terminals], component_ids: [...n.component_ids] })) };
    copying.draft_count++; copying.draft_ms += performance.now() - draftStart;
    const ports = { ...retainedPorts, ...project({ placements: [p] }, catalogs) }, occupied = index(w), routes = [];
    try {
      const edges = graph.edges.filter(e => e.target === node.id).sort((a, b) => (a.transport === 'pipe' ? 0 : 1) - (b.transport === 'pipe' ? 0 : 1) || a.id.localeCompare(b.id));
      for (const edge of edges) routes.push(...connect(w, ports, occupied, edge, catalogs, null, routeFamily));
      const materializeStart = performance.now();
      candidates.push(structuredClone({ schema_version: 1, case: 'actual-graph-factory-closure', embedding: 'closure', witness: w, catalog_ports: ports, labels: { ...base.labels },
        accepted: false, factory_validated: false, port_groups: { sources: [], targets: [] }, closure_plan: { added: node.id, placement: p, routes, trials, route_family: routeFamily, placement_family: placementFamily } }));
      copying.materialized_count++; copying.materialize_ms += performance.now() - materializeStart;
      if (candidates.length === 8) break;
    } catch (e) { failures.push({ code: e.code ?? 'closure-candidate-failed', message: e.message, entity: e.entity, placement: p }); }
  }
  return { candidates, failures, trials, copying, construction_ms: performance.now() - start };
}
export function finishOutputs(catalogs, graph, base, routeFamily = 'simple') {
  const start = performance.now(), w = structuredClone(base.witness), ports = project(w, catalogs), occupied = index(w), routes = [];
  const extent = extentOf(w), boundary = { width: extent.width + 1, height: extent.height + 1 };
  if (w.placements.length !== graph.nodes.length) fail('closure-incomplete-membership', 'All original facilities are required before final exports');
  for (const edge of graph.edges.filter(e => !e.internal && graph.nodes.some(n => n.id === e.source)).sort((a, b) => a.id.localeCompare(b.id)))
    routes.push(...connect(w, ports, occupied, edge, catalogs, boundary, routeFamily));
  return { ...base, witness: w, catalog_ports: ports, accepted: false, factory_validated: false, closure_plan: { added: null, routes, route_family: routeFamily }, construction_ms: performance.now() - start };
}

// Compare against the original graph, never a candidate-supplied list of remaining obligations.
export function inspectCoverage(graph, detail, validation, complete = false) {
  const diagnostics = [], error = (code, entity) => diagnostics.push({ code, severity: 'error', stage: 'factory-coverage', message: code.replaceAll('-', ' '), entity });
  const w = detail.witness, members = new Set(w.placements.map(p => p.instance));
  if (!validation.covered_geometry_validated) error('closure-geometry-rejected', null);
  if (members.size !== w.placements.length || (complete && (members.size !== graph.nodes.length || graph.nodes.some(n => !members.has(n.id))))) error('closure-member-mismatch', null);
  for (const p of w.placements) {
    const n = graph.nodes.find(n => n.id === p.instance), u = w.utilizations[p.instance];
    if (!n || n.recipe !== p.recipe || !u || compare(parseRate(u), parseRate(n.utilization))) error('closure-recipe-mismatch', p.instance);
  }
  const pending = new Map(), external = new Map(), actual = new Map(), required = new Map();
  const addTo = (m, id, r) => m.set(id, add(m.get(id) ?? rate(0), parseRate(r)));
  for (const e of graph.edges) {
    const hasSource = members.has(e.source), hasTarget = members.has(e.target);
    if (hasSource) addTo(required, JSON.stringify([e.source, 'output', e.item]), e.rate);
    if (hasTarget) addTo(required, JSON.stringify([e.target, 'input', e.item]), e.rate);
    if (hasTarget && e.internal && !hasSource) error('closure-missing-internal-input', e.id);
    if (hasSource && !hasTarget && (e.internal || !complete)) addTo(pending, JSON.stringify([e.source, 'output', e.item]), e.rate);
    if (!e.internal && (hasTarget || (complete && hasSource))) addTo(external, JSON.stringify([hasTarget ? e.source : e.target, hasTarget ? 'output' : 'input', e.item]), e.rate);
  }
  const seen = new Set();
  for (const t of validation.recipe_terminals) {
    const id = JSON.stringify([t.instance, t.direction, t.item]);
    if (seen.has(id) || !required.has(id)) error('closure-terminal-shape', id); seen.add(id);
    const total = required.get(id);
    if (total && (compare(parseRate(t.required_rate), total) || compare(add(parseRate(t.pending_rate), parseRate(t.routed_rate)), total))) error('closure-terminal-rate-mismatch', id);
    if (compare(parseRate(t.pending_rate), pending.get(id) ?? rate(0))) error('closure-pending-mismatch', id);
    pending.delete(id);
  }
  if (pending.size) error('closure-missing-terminal-report', [...pending.keys()]);
  for (const id of required.keys()) if (!seen.has(id)) error('closure-missing-terminal-report', id);
  for (const n of w.transport_networks) for (const t of n.terminals) if (t.endpoint.kind === 'external') addTo(actual, JSON.stringify([t.endpoint.node, t.direction, n.item]), t.rate);
  for (const id of new Set([...external.keys(), ...actual.keys()])) if (compare(external.get(id) ?? rate(0), actual.get(id) ?? rate(0))) error('closure-external-margin-mismatch', id);
  if (complete && !validation.recipe_coverage_complete) error('closure-incomplete-recipes', null);
  return { accepted: diagnostics.length === 0, factory_validated: complete && diagnostics.length === 0, diagnostics };
}
