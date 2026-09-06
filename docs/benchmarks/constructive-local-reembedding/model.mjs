import { shortestPath, key, direction, distance, directions } from './path.mjs';
import { project } from '../constructive-factory-closure/model.mjs';
import { witnessScore } from '../constructive-auto-interface/compact.mjs';
const equalRate = (a, b) => BigInt(a.numerator) * BigInt(b.denominator) === BigInt(b.numerator) * BigInt(a.denominator);
const fail = (code, message, entity = null) => { throw Object.assign(new Error(message), { code, severity: 'error', stage: 'local-reembedding', entity }); };
const cellsOf = p => Array.from({ length: p.width * p.height }, (_, i) => ({ x: p.x + i % p.width, y: p.y + Math.floor(i / p.width) }));

export function extractChains(w) {
  const chains = [], anchors = { belt: new Set(), pipe: new Set() }, terminals = new Map();
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
      if (anchors[n.transport].has(k) || layerOwners.get(`${n.transport}:${k}`) > 1 || a.length !== 1 || b.length !== 1 || !equalRate(a[0].rate, b[0].rate)) local.add(k);
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

export function reembed(detail, catalogs, { move = null, chainIds = [], order = 'near', workBudget = 200000 } = {}) {
  const began = performance.now(), original = detail.witness, { chains, anchors } = extractChains(original), bounds = extentOf(original);
  if (!['near', 'far'].includes(order)) fail('local-route-order', 'Expected near or far route order');
  const changed = new Set(chainIds);
  if ([...changed].some(i => !Number.isSafeInteger(i) || !chains[i])) fail('local-chain-id', 'Unknown chain id');
  const w = structuredClone(original), moving = move && w.placements.find(p => p.instance === move.instance);
  const movingTerminals = { belt: new Set(), pipe: new Set() };
  let dx = 0, dy = 0;
  if (move) {
    if (!moving || !Number.isSafeInteger(move.x) || !Number.isSafeInteger(move.y)) fail('local-move-input', 'Unknown facility or noninteger position', move);
    dx = move.x - moving.x; dy = move.y - moving.y;
    for (const n of w.transport_networks) for (const t of n.terminals)
      if (t.endpoint.kind === 'facility' && t.endpoint.instance === move.instance) movingTerminals[n.transport].add(key(t.position));
    for (const n of w.transport_networks) for (const t of n.terminals)
      if (movingTerminals[n.transport].has(key(t.position)) && (t.endpoint.kind !== 'facility' || t.endpoint.instance !== move.instance))
        fail('local-shared-terminal-move', 'Moving a zero-length shared terminal needs a separate network rewrite', move.instance);
    const body = new Set(cellsOf({ ...moving, x: move.x, y: move.y }).map(key));
    for (const p of w.placements) if (p !== moving && cellsOf(p).some(q => body.has(key(q)))) fail('local-body-collision', 'Moved body intersects another facility', move.instance);
    for (const c of w.logistics_components) if (body.has(key(c.position))) fail('local-component-collision', 'Moved body intersects a fixed component', c.id);
    for (const transport of ['belt', 'pipe']) for (const k of anchors[transport])
      if (body.has(k) && !movingTerminals[transport].has(k)) fail('local-anchor-collision', 'Moved body intersects a fixed route anchor', k);
    for (const n of w.transport_networks) for (const t of n.terminals) if (t.endpoint.kind === 'facility' && t.endpoint.instance === move.instance) {
      t.position.x += dx; t.position.y += dy;
      for (const transport of ['belt', 'pipe']) if (anchors[transport].has(key(t.position)) && !movingTerminals[transport].has(key(t.position)))
        fail('local-terminal-collision', 'Moved terminal intersects a fixed anchor', t.id);
    }
    moving.x = move.x; moving.y = move.y;
    const movedConnections = new Set(w.transport_networks.flatMap(n => n.terminals.filter(t => t.endpoint.kind === 'facility' && t.endpoint.instance === move.instance).map(t => key(t.position))));
    for (const c of chains) if ([...c.startTerminals, ...c.endTerminals].some(t => t.endpoint.kind === 'facility' && t.endpoint.instance === move.instance) ||
      c.cells.some(p => body.has(key(p)) || movedConnections.has(key(p)))) changed.add(c.id);
  }
  const relocated = (p, ts) => ts.some(t => t.endpoint.kind === 'facility' && t.endpoint.instance === move?.instance) ? { x: p.x + dx, y: p.y + dy } : p;
  const jobs = [...changed].map(i => {
    const c = chains[i], start = relocated(c.cells[0], c.startTerminals), end = relocated(c.cells.at(-1), c.endTerminals);
    return { ...c, start, end };
  }).sort((a, b) => (order === 'near' ? 1 : -1) * (distance(a.start, a.end) - distance(b.start, b.end)) || a.id - b.id);
  const body = new Set(w.placements.flatMap(cellsOf).map(key)), occupied = { belt: new Set(), pipe: new Set() };
  for (const c of chains) if (!changed.has(c.id)) for (const p of c.cells) occupied[c.transport].add(key(p));
  for (const n of w.transport_networks) for (const t of n.terminals) occupied[n.transport].add(key(t.position));
  for (const c of w.logistics_components) occupied[c.transport].add(key(c.position));
  for (const j of jobs) { occupied[j.transport].add(key(j.start)); occupied[j.transport].add(key(j.end)); }
  const reservations = { belt: new Map(), pipe: new Map() };
  for (const j of jobs) {
    for (const p of [{ x: j.start.x + directions[j.first][0], y: j.start.y + directions[j.first][1] },
      { x: j.end.x - directions[j.last][0], y: j.end.y - directions[j.last][1] }]) {
      const k = key(p); if (!reservations[j.transport].has(k)) reservations[j.transport].set(k, new Set()); reservations[j.transport].get(k).add(j.id);
    }
  }
  const results = [], paths = new Map();
  for (const job of jobs) {
    const blocked = new Set([...body, ...occupied[job.transport]]);
    for (const [k, owners] of reservations[job.transport]) if (!owners.has(job.id)) blocked.add(k);
    if (body.has(key(job.start)) || body.has(key(job.end))) fail('local-endpoint-body', 'Route endpoint intersects a body', job.id);
    blocked.delete(key(job.start)); blocked.delete(key(job.end));
    const result = shortestPath(job.start, job.end, blocked, bounds, { first: job.first, last: job.last, workBudget });
    results.push({ id: job.id, item: job.item, status: result.status, expanded: result.expanded, old_steps: job.cells.length - 1, steps: result.steps, turns: result.turns });
    if (result.status !== 'found') return { status: 'rejected', diagnostics: [{ code: `local-${result.status}`, severity: 'info', stage: 'local-reembedding', entity: job.id,
      message: 'Affected chain could not be restored within this candidate neighborhood' }], affected_chains: jobs.map(j => ({ id: j.id, item: j.item })), routes: results, elapsed_ms: performance.now() - began };
    paths.set(job.id, result.cells); for (const p of result.cells) occupied[job.transport].add(key(p));
  }
  if ((!move || (dx === 0 && dy === 0)) && jobs.every(j => paths.get(j.id).length === j.cells.length && paths.get(j.id).every((p, i) => key(p) === key(j.cells[i]))))
    return { status: 'unchanged', routes: results, elapsed_ms: performance.now() - began };
  for (const [ni, n] of w.transport_networks.entries()) if (jobs.some(j => j.network === ni)) {
    const usedBefore = new Set(original.transport_networks[ni].segments.flatMap(s => [key(s.from), key(s.to)]));
    const cells = new Map(n.cells.filter(p => !usedBefore.has(key(p))).map(p => [key(p), p])), segments = [];
    for (const c of chains.filter(c => c.network === ni)) {
      const ps = paths.get(c.id) ?? c.cells;
      for (const p of ps) cells.set(key(p), p);
      for (let i = 1; i < ps.length; i++) segments.push({ from: ps[i - 1], to: ps[i], rate: c.rate });
    }
    n.cells = [...cells.values()]; n.segments = segments;
  }
  const score = witnessScore(w), candidate = { schema_version: 1, case: 'complete-local-reembedding', embedding: 'local-astar', labels: structuredClone(detail.labels),
    witness: structuredClone(w), catalog_ports: project(w, catalogs), port_groups: { sources: [], targets: [] }, accepted: false, factory_validated: false,
    reembedding: { move, order, affected_chains: jobs.map(j => j.id), routes: results } };
  return { status: 'candidate', detail: candidate, score, affected_chains: jobs.map(j => ({ id: j.id, item: j.item })), routes: results, elapsed_ms: performance.now() - began };
}

export function relocationCandidates(detail, { limit = 64 } = {}) {
  if (!Number.isSafeInteger(limit) || limit < 1 || limit > 512) fail('local-move-budget', 'Expected candidate limit 1..512');
  const w = detail.witness, { chains, anchors } = extractChains(w), bounds = extentOf(w), candidates = [];
  for (const p of w.placements) {
    const ts = w.transport_networks.flatMap(n => n.terminals.map(t => ({ ...t, transport: n.transport }))), own = ts.filter(t => t.endpoint.kind === 'facility' && t.endpoint.instance === p.instance);
    const ownKeys = new Set(own.map(t => `${t.transport}:${key(t.position)}`));
    if (ts.some(t => ownKeys.has(`${t.transport}:${key(t.position)}`) && (t.endpoint.kind !== 'facility' || t.endpoint.instance !== p.instance))) continue;
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
      if (delta >= 0) continue;
      const body = new Set(cellsOf({ ...p, x, y }).map(key));
      if (w.logistics_components.some(c => body.has(key(c.position))) || ['belt', 'pipe'].some(k => [...anchors[k]].some(a => body.has(a) && !ownKeys.has(`${k}:${a}`)))) continue;
      if (own.some(t => { const q = { x: t.position.x + x - p.x, y: t.position.y + y - p.y }, k = key(q);
        return q.x < bounds.minX || q.x > bounds.maxX || q.y < bounds.minY || q.y > bounds.maxY ||
          ['belt', 'pipe'].some(tr => anchors[tr].has(k) && !ownKeys.has(`${tr}:${k}`)) ||
          w.placements.some(a => a !== p && q.x >= a.x && q.x < a.x + a.width && q.y >= a.y && q.y < a.y + a.height);
      })) continue;
      local.push({ instance: p.instance, label: detail.labels[p.instance], x, y, endpoint_delta: delta });
    }
    local.sort((a, b) => a.endpoint_delta - b.endpoint_delta || a.y - b.y || a.x - b.x);
    // Diversify across facilities before deepening any one facility's position shortlist.
    for (const [rank, c] of local.slice(0, 8).entries()) candidates.push({ ...c, rank });
  }
  return candidates.sort((a, b) => a.rank - b.rank || a.endpoint_delta - b.endpoint_delta || a.instance.localeCompare(b.instance)).slice(0, limit);
}
