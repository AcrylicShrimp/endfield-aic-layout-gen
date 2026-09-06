import { project } from '../constructive-factory-closure/model.mjs';

export function cooperativePortCandidates(w, catalogs, moves, terminalIds, { limit = 4096, nodeBudget = 65536 } = {}) {
  const fail = (message, entity) => { throw Object.assign(new Error(message), { code: 'local-cooperative-ports', severity: 'info', stage: 'local-ports', entity }); };
  if (!Array.isArray(terminalIds) || !terminalIds.length || terminalIds.length > 8 || new Set(terminalIds).size !== terminalIds.length ||
    !Number.isSafeInteger(limit) || limit < 1 || limit > 4096 || !Number.isSafeInteger(nodeBudget) || nodeBudget < 1 || nodeBudget > 65536)
    fail('Expected one to eight unique facility-terminal IDs, limit 1..4096 and node budget 1..65536', terminalIds);
  const selected = new Set(terminalIds), all = w.transport_networks.flatMap(n => n.terminals.map(t => ({ t, transport: n.transport })));
  const terminals = terminalIds.map(id => {
    const matches = all.filter(({ t }) => t.id === id);
    if (matches.length !== 1 || matches[0].t.endpoint.kind !== 'facility') fail('Expected one existing facility terminal per selected ID', id);
    return matches[0];
  });
  const shifts = new Map(moves.flatMap(m => m.instances.map(id => [id, m])));
  const placements = w.placements.map(p => ({ ...p, x: p.x + (shifts.get(p.instance)?.dx ?? 0), y: p.y + (shifts.get(p.instance)?.dy ?? 0) }));
  const ports = project({ ...w, placements }, catalogs), slot = (instance, port) => JSON.stringify([instance, port]);
  const reserved = new Set(all.filter(({ t }) => t.endpoint.kind === 'facility' && !selected.has(t.id)).map(({ t }) => slot(t.endpoint.instance, t.endpoint.port)));
  const domains = terminals.map(({ t, transport }) => ({ terminal: t.id, instance: t.endpoint.instance, retained: t.endpoint.port,
    ports: ports[t.endpoint.instance].filter(q => q.transport === transport && q.direction === t.direction && !reserved.has(slot(t.endpoint.instance, q.id)) &&
      !placements.some(p => q.connection.x >= p.x && q.connection.x < p.x + p.width && q.connection.y >= p.y && q.connection.y < p.y + p.height)).map(q => q.id).sort() }));
  const candidates = [], chosen = [], used = new Set(); let nodes = 0, termination = 'exhausted';
  const visit = depth => {
    if (nodes === nodeBudget) { termination = 'node-budget'; return false; } nodes++;
    if (depth === domains.length) {
      if (candidates.length === limit) { termination = 'candidate-limit'; return false; }
      candidates.push({ portAssignments: chosen.flatMap((port, i) => port === domains[i].retained ? [] : [{ terminal: domains[i].terminal, port }]) });
      return true;
    }
    const d = domains[depth];
    for (const port of d.ports) {
      const k = slot(d.instance, port); if (used.has(k)) continue;
      used.add(k); chosen.push(port); const keepGoing = visit(depth + 1); chosen.pop(); used.delete(k);
      if (!keepGoing) return false;
    }
    return true;
  };
  visit(0);
  return { domains, candidates, nodes, node_budget: nodeBudget, candidate_limit: limit, termination };
}
