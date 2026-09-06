import { extentOf } from '../constructive-local-reembedding/model.mjs';
import { bodyCells } from '../constructive-local-reembedding/movement.mjs';
import { key } from '../constructive-local-reembedding/path.mjs';
import { compare } from './score.mjs';
import { fail } from './io.mjs';

export const tokenFields = ['facility', 'network', 'component', 'belt', 'pipe', 'x', 'y', 'width', 'height', 'rotation_cos', 'rotation_sin',
  'physical_cells', 'input_terminals', 'output_terminals', 'external_terminals'];
export const actionFields = ['translate', 'compact', 'dx', 'dy', 'members', 'x', 'y', 'width', 'height', 'body_area_ratio',
  'endpoint_distance_delta', 'incident_networks', 'external_terminals', 'body_collision', 'outside_neighborhood',
  'remaining', 'current_area', 'current_cells', 'current_turns', 'best_area', 'best_cells', 'best_turns', 'current_vs_best'];

export const contextFor = (current, best, initial, remaining, allotted) => [remaining / allotted,
  current.area / initial.area, current.physical_tiles / Math.max(initial.physical_tiles, 1), current.turns / Math.max(initial.turns, 1),
  best.area / initial.area, best.physical_tiles / Math.max(initial.physical_tiles, 1), best.turns / Math.max(initial.turns, 1), compare(current.tuple, best.tuple)];

export function actionsFor(w, graph, plan) {
  const ids = new Set(w.placements.map(p => p.instance)), groups = w.placements.map(p => [p.instance]);
  if (plan.connected_pairs) {
    const pairs = new Map();
    for (const e of graph.edges) if (ids.has(e.source) && ids.has(e.target) && e.source !== e.target) {
      const pair = [e.source, e.target].sort(); pairs.set(JSON.stringify(pair), pair);
    }
    groups.push(...pairs.values());
  }
  const actions = groups.flatMap(instances => plan.translation_steps.flatMap(step => [[step, 0], [-step, 0], [0, step], [0, -step]]
    .map(([dx, dy]) => ({ kind: 'translate', instances, dx, dy }))));
  actions.push({ kind: 'compact' });
  if (actions.length > plan.max_actions) fail('action-bound', 'Action guard exceeded; no silent action truncation', actions.length);
  return actions;
}

export function observe(w, graph, actions, current, best, initial, remaining, allotted, plan) {
  const extent = extentOf(w), sx = Math.max(initial.width, 1), sy = Math.max(initial.height, 1), spatial = Math.max(sx, sy),
    cellsScale = Math.max(initial.physical_tiles, 1);
  const xy = p => [(p.x - extent.minX) / sx, (p.y - extent.minY) / sy], tokens = [], links = [], facilities = new Map(), networks = new Map();
  const terminals = w.transport_networks.flatMap(n => n.terminals.map(t => ({ ...t, transport: n.transport })));
  for (const p of w.placements) {
    const own = terminals.filter(t => t.endpoint.kind === 'facility' && t.endpoint.instance === p.instance);
    facilities.set(p.instance, tokens.length);
    tokens.push([1, 0, 0, Number(own.some(t => t.transport === 'belt')), Number(own.some(t => t.transport === 'pipe')), ...xy(p), p.width / sx, p.height / sy,
      Math.round(Math.cos(p.rotation * Math.PI / 180)), Math.round(Math.sin(p.rotation * Math.PI / 180)), 0,
      own.filter(t => t.direction === 'input').length / 8, own.filter(t => t.direction === 'output').length / 8, 0]);
  }
  for (const n of w.transport_networks) {
    const xs = n.cells.map(p => p.x), ys = n.cells.map(p => p.y), p = { x: Math.min(...xs), y: Math.min(...ys) };
    networks.set(n.id, tokens.length);
    tokens.push([0, 1, 0, Number(n.transport === 'belt'), Number(n.transport === 'pipe'), ...xy(p),
      (Math.max(...xs) - p.x + 1) / sx, (Math.max(...ys) - p.y + 1) / sy, 0, 0, n.cells.length / cellsScale,
      n.terminals.filter(t => t.direction === 'input').length / 8, n.terminals.filter(t => t.direction === 'output').length / 8,
      n.terminals.filter(t => t.endpoint.kind === 'external').length / 8]);
    for (const t of n.terminals) if (t.endpoint.kind === 'facility') links.push(t.direction === 'output'
      ? [facilities.get(t.endpoint.instance), networks.get(n.id)] : [networks.get(n.id), facilities.get(t.endpoint.instance)]);
  }
  for (const c of w.logistics_components) {
    const i = tokens.length;
    tokens.push([0, 0, 1, Number(c.transport === 'belt'), Number(c.transport === 'pipe'), ...xy(c.position), 1 / sx, 1 / sy,
      Math.round(Math.cos(c.rotation * Math.PI / 180)), Math.round(Math.sin(c.rotation * Math.PI / 180)), 1 / cellsScale, 0, 0, 0]);
    for (const n of w.transport_networks.filter(n => n.component_ids.includes(c.id))) links.push([i, networks.get(n.id)], [networks.get(n.id), i]);
  }
  if (tokens.length > plan.max_tokens) fail('token-bound', 'State-token guard exceeded; no silent factory truncation', tokens.length);
  const adjacency = tokens.map(() => tokens.map(() => 0));
  for (const [a, b] of links) { if (a === undefined || b === undefined) fail('feature-invalid', 'Missing terminal identity'); adjacency[a][b] = 1; }
  const context = contextFor(current, best, initial, remaining, allotted);
  const masks = [], candidates = [], maxStep = Math.max(...plan.translation_steps);
  const bodyOwners = new Map(w.placements.flatMap(p => bodyCells(p).map(c => [key(c), p.instance])));
  for (const a of actions) {
    if (a.kind === 'compact') { candidates.push([0, 1, ...Array(13).fill(0), ...context]); masks.push(true); continue; }
    const selected = new Set(a.instances), ps = w.placements.filter(p => selected.has(p.instance)), others = w.placements.filter(p => !selected.has(p.instance));
    const moved = ps.map(p => ({ ...p, x: p.x + a.dx, y: p.y + a.dy }));
    const collision = moved.some(p => bodyCells(p).some(q => { const owner = bodyOwners.get(key(q)); return owner !== undefined && !selected.has(owner); }));
    const outside = moved.some(p => p.x < extent.minX || p.y < extent.minY || p.x + p.width - 1 > extent.maxX || p.y + p.height - 1 > extent.maxY);
    const xmin = Math.min(...ps.map(p => p.x)), ymin = Math.min(...ps.map(p => p.y)), xmax = Math.max(...ps.map(p => p.x + p.width)), ymax = Math.max(...ps.map(p => p.y + p.height));
    const all = [...others, ...moved], area = (Math.max(...all.map(p => p.x + p.width)) - Math.min(...all.map(p => p.x))) * (Math.max(...all.map(p => p.y + p.height)) - Math.min(...all.map(p => p.y)));
    const incident = w.transport_networks.filter(n => n.terminals.some(t => t.endpoint.kind === 'facility' && selected.has(t.endpoint.instance)));
    let distanceDelta = 0;
    for (const n of incident) for (const t of n.terminals.filter(t => t.endpoint.kind === 'facility' && selected.has(t.endpoint.instance)))
      for (const other of n.terminals.filter(other => other.direction !== t.direction && !(other.endpoint.kind === 'facility' && selected.has(other.endpoint.instance))))
        distanceDelta += Math.abs(t.position.x + a.dx - other.position.x) + Math.abs(t.position.y + a.dy - other.position.y)
          - Math.abs(t.position.x - other.position.x) - Math.abs(t.position.y - other.position.y);
    candidates.push([1, 0, a.dx / maxStep, a.dy / maxStep, ps.length / 2, ...xy({ x: xmin, y: ymin }), (xmax - xmin) / sx, (ymax - ymin) / sy,
      area / initial.area, distanceDelta / spatial, incident.length / 8, incident.reduce((sum, n) => sum + n.terminals.filter(t => t.endpoint.kind === 'external').length, 0) / 8,
      Number(collision), Number(outside), ...context]);
    masks.push(!collision && !outside);
  }
  if (tokens.some(t => t.length !== tokenFields.length || t.some(v => !Number.isFinite(v))) || candidates.some(a => a.length !== actionFields.length || a.some(v => !Number.isFinite(v))))
    fail('feature-invalid', 'Feature shape or finite-value invariant failed');
  return { tokens, adjacency, actions: candidates, action_mask: masks };
}
