import { shortestPath } from '../constructive-local-reembedding/path.mjs';
import { diagnostic } from './input.mjs';
import { occupancy, routeBounds } from './geometry.mjs';
import { terminalId } from './factory-input.mjs';

export const usedPorts = w => new Set(w.transport_networks.flatMap(n => n.terminals.filter(t => t.endpoint.kind === 'facility').map(t => JSON.stringify([t.endpoint.instance, t.endpoint.port]))));
export function routeConnection(parent, lane, ends, event, work, { bounds = null, blocked = null } = {}) {
  if (!work.alive()) return null;
  const remaining = work.limits.route_expansions - work.used.route_expansions;
  if (remaining <= 0) { work.reason = 'route_expansions'; return null; }
  const w = structuredClone(parent);
  let net = w.transport_networks.find(n => n.item === lane.item && n.transport === lane.transport);
  if (!net) {
    net = { id: JSON.stringify(['commodity', lane.transport, lane.item]), item: lane.item, transport: lane.transport,
      requirement_ids: [], cells: [], segments: [], terminals: [], component_ids: [] };
    w.transport_networks.push(net);
  }
  if (!net.requirement_ids.includes(lane.id)) net.requirement_ids.push(lane.id);
  for (const [which, direction] of [['source', 'output'], ['target', 'input']]) {
    const p = ends[which], endpoint = p.external ? { kind: 'external', node: lane[which], side: p.edge } : { kind: 'facility', instance: lane[which], port: p.id };
    net.terminals.push({ id: terminalId(lane, which), node: lane[which], direction, endpoint, position: p.connection, rate: lane.rate });
  }
  bounds ??= routeBounds(w, ends.source.connection, ends.target.connection, event.gap ?? 0);
  let result;
  try { result = shortestPath(ends.source.connection, ends.target.connection, blocked ?? occupancy(w, lane.transport), bounds, { workBudget: remaining, allowZeroLength: true }); }
  catch (e) { result = { status: e.code ?? 'route-error', expanded: 0, message: e.message }; }
  work.used.route_expansions += result.expanded;
  event.route = { requirement: lane.id, lane: lane.lane, kind: lane.kind, source: ends.source.external ? ends.source.edge : ends.source.id,
    target: ends.target.external ? ends.target.edge : ends.target.id, bounds, status: result.status, expanded: result.expanded,
    physical_cells: result.cells?.length ?? null, message: result.message ?? null };
  if (result.status !== 'found') {
    if (result.status === 'work-budget') work.reason = 'route_expansions';
    event.diagnostic = diagnostic(work.reason ? 'budget-exhausted' : 'route-failed', 'factory-routing', `Local A* ended with ${result.status}; not global infeasibility`, lane.id, 'info'); return null;
  }
  net.cells.push(...result.cells);
  for (let i = 1; i < result.cells.length; i++) net.segments.push({ from: result.cells[i - 1], to: result.cells[i], rate: lane.rate });
  return w;
}
