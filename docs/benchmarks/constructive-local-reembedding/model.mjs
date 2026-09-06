import { shortestPath, key, direction, distance, directions } from './path.mjs';
import { project } from '../constructive-factory-closure/model.mjs';
import { witnessScore } from '../constructive-auto-interface/compact.mjs';
import { movementPlan } from './movement.mjs';
import { crossingOptions } from './crossing.mjs';
import { planPortAssignments } from './ports.mjs';
import { planComponentPoses, releaseBridgeDraft } from './components.mjs';
import { detachablePairs } from './terminal-identity.mjs';
import { planFacilityRotations } from './facility-rotation.mjs';
const equalRate = (a, b) => BigInt(a.numerator) * BigInt(b.denominator) === BigInt(b.numerator) * BigInt(a.denominator);
const fail = (code, message, entity = null) => { throw Object.assign(new Error(message), { code, severity: 'error', stage: 'local-reembedding', entity }); };
const cellsOf = p => Array.from({ length: p.width * p.height }, (_, i) => ({ x: p.x + i % p.width, y: p.y + Math.floor(i / p.width) }));

export function extractChains(w, released = new Set()) {
  const chains = [], anchors = { belt: new Set(), pipe: new Set() }, terminals = new Map();
  const pairs = detachablePairs(w);
  for (const c of w.logistics_components) anchors[c.transport].add(key(c.position));
  const layerOwners = new Map();
  for (const n of w.transport_networks) for (const p of n.cells) {
    const tag = `${n.transport}:${key(p)}`; layerOwners.set(tag, (layerOwners.get(tag) ?? 0) + 1);
  }
  for (const [ni, n] of w.transport_networks.entries()) {
    const ins = new Map(), outs = new Map(), local = new Set();
    for (const s of n.segments) {
      if (direction(s.from, s.to) < 0) fail('local-nonunit-segment', 'Expected unit directed segment', n.id);
      for (const [m, p] of [[ins, s.to], [outs, s.from]]) { const k = key(p); if (!m.has(k)) m.set(k, []); m.get(k).push(s); }
    }
    for (const t of n.terminals) {
      local.add(key(t.position));
      const tag = `${ni}:${key(t.position)}`; if (!terminals.has(tag)) terminals.set(tag, []); terminals.get(tag).push(t);
    }
    for (const p of n.cells) {
      const k = key(p), a = ins.get(k) ?? [], b = outs.get(k) ?? [];
      if (anchors[n.transport].has(k) || (layerOwners.get(`${n.transport}:${k}`) > 1 && !released.has(`${n.transport}:${k}`)) || a.length !== 1 || b.length !== 1 || !equalRate(a[0].rate, b[0].rate)) local.add(k);
    }
    const visited = new Set();
    for (const s of n.segments) if (local.has(key(s.from))) {
      const path = [s.from], edges = []; let current = s;
      while (current) {
        if (visited.has(current)) fail('local-chain-cycle', 'Chain repeats an edge', n.id);
        visited.add(current); edges.push(current); path.push(current.to);
        if (local.has(key(current.to))) break;
        current = outs.get(key(current.to))?.[0];
      }
      if (!local.has(key(path.at(-1)))) fail('local-open-chain', 'Chain has no fixed endpoint', n.id);
      chains.push({ id: chains.length, network: ni, transport: n.transport, item: n.item, cells: path, rate: s.rate,
        first: direction(path[0], path[1]), last: direction(path.at(-2), path.at(-1)),
        startTerminals: terminals.get(`${ni}:${key(path[0])}`) ?? [], endTerminals: terminals.get(`${ni}:${key(path.at(-1))}`) ?? [] });
    }
    if (visited.size !== n.segments.length) fail('local-unanchored-cycle', 'A directed cycle has no terminal or component anchor', n.id);
    for (const p of pairs.values()) if (p.network === ni) chains.push({ id: chains.length, network: ni, transport: n.transport, item: n.item,
      zero_length: true, cells: [p.source.position], rate: p.source.rate, first: null, last: null, startTerminals: [p.source], endTerminals: [p.target] });
    for (const k of local) anchors[n.transport].add(k);
  }
  return { chains, anchors };
}

export function extentOf(w) {
  const points = [...w.placements.flatMap(p => [p, { x: p.x + p.width - 1, y: p.y + p.height - 1 }]),
    ...w.transport_networks.flatMap(n => n.cells), ...w.logistics_components.map(c => c.position)];
  return points.reduce((a, p) => ({ minX: Math.min(a.minX, p.x), maxX: Math.max(a.maxX, p.x), minY: Math.min(a.minY, p.y), maxY: Math.max(a.maxY, p.y) }),
    { minX: Infinity, maxX: -Infinity, minY: Infinity, maxY: -Infinity });
}

export function flexibleTerminalAnchors(w, chains) {
  const tag = (tr, p) => `${tr}:${key(p)}`, terminals = new Map(), incident = new Map();
  const components = new Set(w.logistics_components.map(c => tag(c.transport, c.position)));
  for (const n of w.transport_networks) for (const t of n.terminals) {
    const k = tag(n.transport, t.position); terminals.set(k, (terminals.get(k) ?? 0) + 1);
  }
  for (const c of chains) for (const p of [c.cells[0], c.cells.at(-1)]) {
    const k = tag(c.transport, p); incident.set(k, (incident.get(k) ?? 0) + 1);
  }
  return new Set([...terminals].filter(([k, n]) => n === 1 && incident.get(k) === 1 && !components.has(k)).map(([k]) => k));
}

export function reembed(detail, catalogs, options = {}) {
  if (Object.hasOwn(options, 'move')) fail('local-move-input', 'Move schema 2 is obsolete; supply schema-3 moves array');
  const { moves = [], facilityRotations = [], componentPoses = [], releaseBridges = [], chainIds = [], order = 'near', workBudget = 200000, endpointPolicy = 'retained', junctionPolicy = 'internal', routePriority = [], maxNewCrossings = 0, portAssignments = [] } = options;
  const began = performance.now(), { witness: original, released } = releaseBridgeDraft(detail.witness, catalogs, releaseBridges);
  const { chains, anchors } = extractChains(original, released), bounds = extentOf(original);
  if (!['near', 'far'].includes(order)) fail('local-route-order', 'Expected near or far route order');
  if (!['retained', 'terminal-flexible'].includes(endpointPolicy)) fail('local-endpoint-policy', 'Expected retained or terminal-flexible endpoint policy');
  if (!['internal', 'downstream'].includes(junctionPolicy)) fail('local-junction-policy', 'Expected internal or downstream junction ownership');
  if (!Number.isSafeInteger(maxNewCrossings) || maxNewCrossings < 0 || maxNewCrossings > 8) fail('local-crossing-budget', 'Expected new crossing budget 0..8');
  const flexible = endpointPolicy === 'terminal-flexible' ? flexibleTerminalAnchors(original, chains) : new Set();
  const changed = new Set(chainIds);
  for (const c of chains) if (c.cells.some(p => released.has(`${c.transport}:${key(p)}`))) changed.add(c.id);
  if ([...changed].some(i => !Number.isSafeInteger(i) || !chains[i])) fail('local-chain-id', 'Unknown chain id');
  if (!Array.isArray(routePriority) || new Set(routePriority).size !== routePriority.length || routePriority.some(i => !Number.isSafeInteger(i) || !chains[i]))
    fail('local-route-priority', 'Expected unique known chain IDs in route priority');
  const priority = new Map(routePriority.map((id, i) => [id, i]));
  if (!Array.isArray(moves)) fail('local-move-input', 'Expected schema-3 moves array');
  const componentChanges = planComponentPoses(original, catalogs, componentPoses);
  const facilityChanges = planFacilityRotations(original, catalogs, moves, facilityRotations);
  const rerouteChains = new Set(chains.filter(c => c.cells.some(p => released.has(`${c.transport}:${key(p)}`))).map(c => c.id));
  const movement = moves.length || componentChanges.size ? movementPlan(original, chains, anchors, moves, { junctionPolicy, componentChanges, facilityChanges, rerouteChains }) : null;
  const shifted = (p, tr) => movement?.movable(tr, p) ? movement.shiftAnchor(tr, p) : p;
  // Private append-only draft: untouched nested values are read-only until successful detachment.
  const w = { ...original,
    placements: original.placements.map(p => movement?.selected.has(p.instance) ? { ...p, ...movement.shiftFacility(p) } : p),
    logistics_components: original.logistics_components.map(c => movement?.components.includes(c.id) ? { ...c, position: movement.shiftAnchor(c.transport, c.position), rotation: componentChanges.get(c.id)?.rotation ?? c.rotation } : c),
    transport_networks: original.transport_networks.map(n => ({ ...n, component_ids: [...n.component_ids], terminals: n.terminals.map(t => movement ? { ...t, position: movement.shiftTerminal(t) } : t) })) };
  const paths = new Map(movement?.fixedChains ?? []);
  if (movement) {
    for (const id of movement.changed) changed.add(id);
    for (const id of paths.keys()) changed.delete(id);
    const carried = [...w.placements.flatMap(p => [p, { x: p.x + p.width - 1, y: p.y + p.height - 1 }]), ...w.logistics_components.map(c => c.position), ...w.transport_networks.flatMap(n => n.terminals.map(t => t.position)), ...paths.values()].flat();
    if (carried.some(p => p.x < bounds.minX || p.x > bounds.maxX || p.y < bounds.minY || p.y > bounds.maxY))
      fail('local-move-bounds', 'Translated geometry leaves the explicit current-extent neighborhood', moves.flatMap(m => m.instances));
  }
  if (!Array.isArray(portAssignments)) fail('local-port-assignment', 'Expected a terminal assignment array');
  const singleton = portAssignments.length ? flexibleTerminalAnchors(original, chains) : new Set();
  const zeroTerminals = new Set(chains.filter(c => c.zero_length).flatMap(c => [...c.startTerminals, ...c.endTerminals].map(t => t.id)));
  const eligible = new Set(original.transport_networks.flatMap(n => n.terminals.filter(t => zeroTerminals.has(t.id) || singleton.has(`${n.transport}:${key(t.position)}`)).map(t => t.id)));
  const ports = portAssignments.length ? planPortAssignments(w, catalogs, portAssignments, eligible) : { networks: w.transport_networks, positions: new Map() };
  w.transport_networks = ports.networks;
  const changedPort = (c, end) => (end ? c.endTerminals : c.startTerminals).some(t => ports.positions.has(t.id));
  const newPorts = new Set(w.transport_networks.flatMap(n => n.terminals.filter(t => ports.positions.has(t.id)).map(t => `${n.transport}:${key(t.position)}`)));
  const finalTerminals = new Map(w.transport_networks.flatMap(n => n.terminals.map(t => [t.id, t.position])));
  for (const p of ports.positions.values()) if (p.x < bounds.minX || p.x > bounds.maxX || p.y < bounds.minY || p.y > bounds.maxY)
    fail('local-move-bounds', 'Reassigned port leaves the explicit current-extent neighborhood');
  if (newPorts.size) for (const c of chains) if (changedPort(c, false) || changedPort(c, true) ||
    (paths.get(c.id) ?? c.cells).some(p => newPorts.has(`${c.transport}:${key(p)}`))) { paths.delete(c.id); changed.add(c.id); }
  const relocated = (c, end) => {
    const ts = end ? c.endTerminals : c.startTerminals;
    return ts.length === 1 ? finalTerminals.get(ts[0].id) : movement ? movement.shiftEndpoint(c, end) : end ? c.cells.at(-1) : c.cells[0];
  };
  const jobs = [...changed].map(i => {
    const c = chains[i], start = relocated(c, false), end = relocated(c, true);
    const arm = (p, d) => d === null ? null : (d + (movement?.anchorTurns.get(`${c.transport}:${key(p)}`) ?? 0)) % 4;
    return { ...c, start, end, first: changedPort(c, false) || flexible.has(`${c.transport}:${key(c.cells[0])}`) ? null : arm(c.cells[0], c.first),
      last: changedPort(c, true) || flexible.has(`${c.transport}:${key(c.cells.at(-1))}`) ? null : arm(c.cells.at(-1), c.last) };
  }).sort((a, b) => (priority.get(a.id) ?? routePriority.length) - (priority.get(b.id) ?? routePriority.length) ||
    (order === 'near' ? 1 : -1) * (distance(a.start, a.end) - distance(b.start, b.end)) || a.id - b.id);
  const body = new Set(w.placements.flatMap(cellsOf).map(key)), occupied = { belt: new Set(), pipe: new Set() };
  for (const c of chains) if (!changed.has(c.id)) for (const p of paths.get(c.id) ?? c.cells) occupied[c.transport].add(key(p));
  for (const n of w.transport_networks) for (const t of n.terminals) occupied[n.transport].add(key(t.position));
  for (const c of w.logistics_components) occupied[c.transport].add(key(c.position));
  for (const j of jobs) { occupied[j.transport].add(key(j.start)); occupied[j.transport].add(key(j.end)); }
  const reservations = { belt: new Map(), pipe: new Map() };
  for (const j of jobs) {
    for (const p of [j.first === null ? null : { x: j.start.x + directions[j.first][0], y: j.start.y + directions[j.first][1] },
      j.last === null ? null : { x: j.end.x - directions[j.last][0], y: j.end.y - directions[j.last][1] }].filter(Boolean)) {
      const k = key(p); if (!reservations[j.transport].has(k)) reservations[j.transport].set(k, new Set()); reservations[j.transport].get(k).add(j.id);
    }
  }
  const results = [], installedCrossings = [];
  for (const job of jobs) {
    const blocked = new Set([...body, ...occupied[job.transport]]);
    for (const [k, owners] of reservations[job.transport]) if (!owners.has(job.id)) blocked.add(k);
    if (body.has(key(job.start)) || body.has(key(job.end))) fail('local-endpoint-body', 'Route endpoint intersects a body', job.id);
    blocked.delete(key(job.start)); blocked.delete(key(job.end));
    const excluded = new Set([...body, ...reservations[job.transport].keys(),
      ...w.logistics_components.filter(c => c.transport === job.transport).map(c => key(c.position)),
      ...w.transport_networks.filter(n => n.transport === job.transport).flatMap(n => n.terminals.map(t => key(t.position))),
      ...jobs.filter(j => j.transport === job.transport).flatMap(j => [key(j.start), key(j.end)])]);
    const carriers = maxNewCrossings > installedCrossings.length ? chains.filter(c => !changed.has(c.id) || paths.has(c.id)).map(c => ({ ...c, cells: paths.get(c.id) ?? c.cells })) : [];
    const sites = carriers.length ? crossingOptions(carriers, job, catalogs, excluded) : new Map();
    const result = shortestPath(job.start, job.end, blocked, bounds, { first: job.first, last: job.last, workBudget,
      crossingOptions: sites, crossingLimit: maxNewCrossings - installedCrossings.length, allowZeroLength: job.zero_length === true });
    results.push({ id: job.id, item: job.item, first: job.first, last: job.last, status: result.status, expanded: result.expanded, old_steps: job.cells.length - 1,
      steps: result.steps, turns: result.turns, crossing_sites: sites.size, crossings: result.crossings ?? [] });
    if (result.status !== 'found') return { status: 'rejected', diagnostics: [{ code: `local-${result.status}`, severity: 'info', stage: 'local-reembedding', entity: job.id,
      message: 'Affected chain could not be restored within this candidate neighborhood' }], affected_chains: jobs.map(j => ({ id: j.id, item: j.item })), routes: results, elapsed_ms: performance.now() - began };
    for (const site of result.crossings) {
      let ordinal = w.logistics_components.length, id;
      do { id = JSON.stringify(['local-crossing', job.transport, ordinal++]); } while (w.logistics_components.some(c => c.id === id) || detail.witness.logistics_components.some(c => c.id === id));
      const component = { id, component: site.component, kind: 'bridge', transport: site.transport, rotation: site.rotation, position: { ...site.position } };
      w.logistics_components.push(component); installedCrossings.push({ ...site, id, routed_chain: job.id });
      for (const ni of [site.carrier_network, job.network]) w.transport_networks[ni].component_ids.push(id);
    }
    paths.set(job.id, result.cells); for (const p of result.cells) occupied[job.transport].add(key(p));
  }
  const unchangedPoses = [...componentChanges].every(([id, p]) => {
    const c = original.logistics_components.find(c => c.id === id); return c.rotation === p.rotation && key(c.position) === key(p.position);
  });
  if (!released.size && unchangedPoses && !facilityChanges.size && !portAssignments.length && moves.every(m => m.dx === 0 && m.dy === 0) && jobs.every(j => paths.get(j.id).length === j.cells.length && paths.get(j.id).every((p, i) => key(p) === key(j.cells[i]))))
    return { status: 'unchanged', routes: results, elapsed_ms: performance.now() - began };
  for (const [ni, n] of w.transport_networks.entries()) if (chains.some(c => c.network === ni && paths.has(c.id)) ||
    original.transport_networks[ni].terminals.some(t => movement?.movable(n.transport, t.position))) {
    const usedBefore = new Set(chains.filter(c => c.network === ni).flatMap(c => c.cells.map(key)));
    const cells = new Map(original.transport_networks[ni].cells.filter(p => !usedBefore.has(key(p))).map(p => shifted(p, n.transport)).map(p => [key(p), p])), segments = [];
    for (const c of chains.filter(c => c.network === ni)) {
      const ps = paths.get(c.id) ?? c.cells;
      for (const p of ps) cells.set(key(p), p);
      for (let i = 1; i < ps.length; i++) segments.push({ from: ps[i - 1], to: ps[i], rate: c.rate });
    }
    n.cells = [...cells.values()]; n.segments = segments;
  }
  const score = witnessScore(w), candidate = { schema_version: 1, case: 'complete-local-reembedding', embedding: 'local-astar', labels: structuredClone(detail.labels),
    witness: structuredClone(w), catalog_ports: project(w, catalogs), port_groups: { sources: [], targets: [] }, accepted: false, factory_validated: false,
    reembedding: { chain_schema_version: 2, move_schema_version: 4, facility_rotations: structuredClone(facilityRotations), endpoint_policy: endpointPolicy, junction_policy: junctionPolicy, route_priority: [...routePriority], max_new_crossings: maxNewCrossings,
      new_crossings: installedCrossings, released_bridges: [...releaseBridges], port_assignments: structuredClone(portAssignments), component_poses: structuredClone(componentPoses), moves: structuredClone(moves), order, carried_components: movement?.components ?? [], translated_chains: movement ? [...movement.fixedChains.keys()].filter(id => !changed.has(id)) : [], affected_chains: jobs.map(j => j.id), routes: results } };
  return { status: 'candidate', detail: candidate, score, affected_chains: jobs.map(j => ({ id: j.id, item: j.item })), routes: results, elapsed_ms: performance.now() - began };
}

export function relocationCandidates(detail, { limit = 64 } = {}) {
  if (!Number.isSafeInteger(limit) || limit < 1 || limit > 512) fail('local-move-budget', 'Expected candidate limit 1..512');
  const w = detail.witness, { chains, anchors } = extractChains(w), bounds = extentOf(w), candidates = [];
  const detachable = detachablePairs(w);
  for (const p of w.placements) {
    const ts = w.transport_networks.flatMap(n => n.terminals.map(t => ({ ...t, transport: n.transport }))), own = ts.filter(t => t.endpoint.kind === 'facility' && t.endpoint.instance === p.instance);
    const ownKeys = new Set(own.map(t => `${t.transport}:${key(t.position)}`));
    if (ts.some(t => ownKeys.has(`${t.transport}:${key(t.position)}`) && !detachable.has(`${t.transport}:${key(t.position)}`) && (t.endpoint.kind !== 'facility' || t.endpoint.instance !== p.instance))) continue;
    const incident = chains.flatMap(c => {
      const own = ts => ts.some(t => t.endpoint.kind === 'facility' && t.endpoint.instance === p.instance);
      return own(c.startTerminals) ? [{ own: c.cells[0], other: c.cells.at(-1) }] : own(c.endTerminals) ? [{ own: c.cells.at(-1), other: c.cells[0] }] : [];
    });
    if (!incident.length) continue;
    const xs = new Set([p.x, bounds.minX, bounds.maxX - p.width + 1]), ys = new Set([p.y, bounds.minY, bounds.maxY - p.height + 1]);
    for (const q of w.placements) if (q !== p) {
      for (const x of [q.x - p.width - 1, q.x + q.width + 1, q.x]) xs.add(x);
      for (const y of [q.y - p.height - 1, q.y + q.height + 1, q.y]) ys.add(y);
    }
    for (const e of incident) { xs.add(e.other.x - (e.own.x - p.x)); ys.add(e.other.y - (e.own.y - p.y)); }
    const oldDistance = incident.reduce((s, e) => s + distance(e.own, e.other), 0), local = [];
    for (const x of xs) for (const y of ys) {
      if ((x === p.x && y === p.y) || x < bounds.minX || y < bounds.minY || x + p.width - 1 > bounds.maxX || y + p.height - 1 > bounds.maxY) continue;
      if (w.placements.some(q => q !== p && x < q.x + q.width && q.x < x + p.width && y < q.y + q.height && q.y < y + p.height)) continue;
      const delta = incident.reduce((s, e) => s + distance({ x: e.own.x + x - p.x, y: e.own.y + y - p.y }, e.other), 0) - oldDistance;
      const ps = w.placements.map(q => q === p ? { ...p, x, y } : q);
      const envelope = (Math.max(...ps.map(q => q.x + q.width)) - Math.min(...ps.map(q => q.x))) * (Math.max(...ps.map(q => q.y + q.height)) - Math.min(...ps.map(q => q.y)));
      const body = new Set(cellsOf({ ...p, x, y }).map(key));
      if (w.logistics_components.some(c => body.has(key(c.position))) || ['belt', 'pipe'].some(k => [...anchors[k]].some(a => body.has(a) && !ownKeys.has(`${k}:${a}`)))) continue;
      if (own.some(t => { const q = { x: t.position.x + x - p.x, y: t.position.y + y - p.y }, k = key(q);
        return q.x < bounds.minX || q.x > bounds.maxX || q.y < bounds.minY || q.y > bounds.maxY ||
          ['belt', 'pipe'].some(tr => anchors[tr].has(k) && !ownKeys.has(`${tr}:${k}`)) ||
          w.placements.some(a => a !== p && q.x >= a.x && q.x < a.x + a.width && q.y >= a.y && q.y < a.y + a.height);
      })) continue;
      local.push({ instances: [p.instance], labels: [detail.labels[p.instance]], dx: x - p.x, dy: y - p.y, endpoint_delta: delta, body_envelope_area: envelope });
    }
    local.sort((a, b) => a.body_envelope_area - b.body_envelope_area || a.endpoint_delta - b.endpoint_delta || a.dy - b.dy || a.dx - b.dx);
    // Diversify across facilities before deepening any one facility's position shortlist.
    for (const [rank, c] of local.slice(0, 8).entries()) candidates.push({ ...c, rank });
  }
  return candidates.sort((a, b) => a.body_envelope_area - b.body_envelope_area || a.rank - b.rank || a.endpoint_delta - b.endpoint_delta || a.instances[0].localeCompare(b.instances[0])).slice(0, limit);
}
