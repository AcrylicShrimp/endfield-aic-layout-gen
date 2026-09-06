import { key, occupancy, ports, vectors } from './geometry.mjs';
import { terminalId } from './factory-input.mjs';

// Proposal heuristic, not a game rule: future facilities inside a closed pocket could be legal.
// Charge every flood expansion to the shared routing-work budget and report it separately.
export function pendingPortAccess(graph, w, work) {
  const present = new Set(w.transport_networks.flatMap(n => n.terminals.map(t => t.id))), demands = [];
  const used = new Set(w.transport_networks.flatMap(n => n.terminals.filter(t => t.endpoint.kind === 'facility').map(t => JSON.stringify([t.node, t.endpoint.port]))));
  for (const p of w.placements) for (const direction of ['input', 'output']) {
    const pending = graph.lanes.filter(e => {
      const which = direction === 'input' ? 'target' : 'source';
      return (e.kind === 'internal' ? e[which] === p.instance : e.instance === p.instance && e.direction === direction) && !present.has(terminalId(e, which));
    });
    if (pending.length) demands.push({ instance: p.instance, direction, transport: pending[0].transport, required: pending.length,
      ports: ports(p, graph.nodes.get(p.instance)).filter(q => q.direction === direction && q.transport === pending[0].transport && !used.has(JSON.stringify([p.instance, q.id]))) });
  }
  if (!demands.length) return { accepted: true, expanded: 0, demands: [] };
  const blocked = occupancy(w, demands[0].transport), points = [...w.placements.flatMap(p => [p, { x: p.x + p.width - 1, y: p.y + p.height - 1 }]), ...w.transport_networks.flatMap(n => n.cells)];
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
  for (const p of points) { minX = Math.min(minX, p.x); minY = Math.min(minY, p.y); maxX = Math.max(maxX, p.x); maxY = Math.max(maxY, p.y); }
  minX--; minY--; maxX++; maxY++;
  const bounds = { minX, minY, maxX, maxY }, width = maxX - minX + 1, height = maxY - minY + 1;
  if (width * height > 250000) return { accepted: false, expanded: 0, status: 'access-grid-guard', bounds };
  const seen = new Set(), queue = [], push = (x, y) => { const k = `${x},${y}`; if (!blocked.has(k) && !seen.has(k)) { seen.add(k); queue.push({ x, y }); } };
  for (let x = minX; x <= maxX; x++) { push(x, minY); push(x, maxY); }
  for (let y = minY + 1; y < maxY; y++) { push(minX, y); push(maxX, y); }
  let expanded = 0;
  for (let i = 0; i < queue.length; i++) {
    if (!work.take('route_expansions')) return { accepted: false, expanded, status: 'work-budget', bounds };
    expanded++; const p = queue[i];
    for (const [dx, dy] of [[1, 0], [0, 1], [-1, 0], [0, -1]]) {
      const x = p.x + dx, y = p.y + dy;
      if (x >= minX && x <= maxX && y >= minY && y <= maxY) push(x, y);
    }
  }
  const checked = demands.map(d => {
    const candidates = d.ports.map(p => {
      const [dx, dy] = vectors[p.edge], corridor = Array.from({ length: d.required }, (_, i) => ({ x: p.connection.x + dx * i, y: p.connection.y + dy * i }));
      return { id: p.id, connection: p.connection, reachable: seen.has(key(p.connection)), corridor_clear: corridor.every(c => !blocked.has(key(c))), corridor };
    });
    return { ...d, ports: candidates, available: candidates.filter(p => p.reachable && p.corridor_clear).length };
  });
  return { accepted: checked.every(d => d.available >= d.required), expanded, status: 'checked', bounds, demands: checked };
}
