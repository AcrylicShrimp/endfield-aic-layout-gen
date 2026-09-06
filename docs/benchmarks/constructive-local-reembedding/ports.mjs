import { project } from '../constructive-factory-closure/model.mjs';
import { key, distance } from './path.mjs';
import { interleave } from './fair.mjs';
import { planFacilityRotations } from './facility-rotation.mjs';
const tag = (tr, p) => `${tr}:${key(p)}`;
const fail = (message, entity) => { throw Object.assign(new Error(message), { code: 'local-port-assignment', stage: 'local-ports', severity: 'info', entity }); };

export function planPortAssignments(w, catalogs, assignments, eligible) {
  if (!Array.isArray(assignments) || assignments.length > 16 || assignments.some(a => !a || typeof a.terminal !== 'string' || typeof a.port !== 'string') ||
    new Set(assignments.map(a => a.terminal)).size !== assignments.length)
    fail('Expected at most sixteen unique terminal assignments', assignments);
  const ports = project(w, catalogs), changes = new Map(), positions = new Map();
  for (const a of assignments) {
    const matches = w.transport_networks.flatMap(n => n.terminals.filter(t => t.id === a.terminal).map(t => ({ n, t })));
    if (matches.length !== 1) fail('Assignment must identify one existing terminal', a.terminal);
    const { n, t } = matches[0];
    if (t.endpoint.kind !== 'facility' || !eligible.has(t.id)) fail('Only independently identified ordinary facility terminals can change ports', a.terminal);
    const q = ports[t.endpoint.instance].find(p => p.id === a.port && p.direction === t.direction && p.transport === n.transport);
    if (!q) fail('Assigned port has incompatible identity, direction or transport', a);
    if (a.port !== t.endpoint.port) {
      changes.set(t.id, { ...t, endpoint: { ...t.endpoint, port: a.port }, position: { ...q.connection } });
      positions.set(t.id, { ...q.connection });
    }
  }
  const networks = w.transport_networks.map(n => ({ ...n, terminals: n.terminals.map(t => changes.get(t.id) ?? t) }));
  const used = new Set(), counts = new Map();
  for (const n of networks) for (const t of n.terminals) {
    const k = tag(n.transport, t.position); counts.set(k, (counts.get(k) ?? 0) + 1);
    if (t.endpoint.kind !== 'facility') continue;
    const slot = JSON.stringify([t.endpoint.instance, t.endpoint.port]);
    if (used.has(slot)) fail('Two terminals claim the same facility port', t.id); used.add(slot);
  }
  for (const n of networks) for (const t of n.terminals) if (changes.has(t.id) &&
    (counts.get(tag(n.transport, t.position)) !== 1 || w.logistics_components.some(c => c.transport === n.transport && key(c.position) === key(t.position))))
    fail('Assigned port collides with another terminal or component', t.id);
  return { networks, positions };
}

// Paired slots of an equal-rate commodity may be permuted without changing any terminal rate.
export function boundaryPortSwaps(w, chains, moves) {
  const selected = new Set(moves.flatMap(m => m.instances)), groups = new Map();
  for (const c of chains) if ([...c.startTerminals, ...c.endTerminals].some(t => t.endpoint.kind === 'facility' && selected.has(t.endpoint.instance)))
    for (const t of [...c.startTerminals, ...c.endTerminals]) if (t.endpoint.kind === 'facility') {
      const k = JSON.stringify([t.endpoint.instance, t.direction, c.item, c.transport]);
      if (!groups.has(k)) groups.set(k, new Map()); groups.get(k).set(t.id, t);
    }
  const result = [];
  for (const ts of groups.values()) {
    const values = [...ts.values()].sort((a, b) => a.position.y - b.position.y || a.position.x - b.position.x || a.id.localeCompare(b.id));
    for (let i = 1; i < values.length; i++) {
      const a = values[i - 1], b = values[i];
      if (BigInt(a.rate.numerator) * BigInt(b.rate.denominator) !== BigInt(b.rate.numerator) * BigInt(a.rate.denominator)) continue;
      result.push([{ terminal: a.id, port: b.endpoint.port }, { terminal: b.id, port: a.endpoint.port }]);
    }
  }
  return result;
}

export function vacantPortCandidates(w, chains, catalogs, moves, { chainIds, limit = 64, combinationBudget = 4096, facilityRotations = [] } = {}) {
  if (!Array.isArray(chainIds) || new Set(chainIds).size !== chainIds.length || chainIds.some(id => !Number.isSafeInteger(id) || !chains.some(c => c.id === id)) ||
    !Number.isSafeInteger(limit) || limit < 1 || limit > 64 || !Number.isSafeInteger(combinationBudget) || combinationBudget < 1 || combinationBudget > 65536)
    fail('Expected known unique chain IDs, candidate limit 1..64 and combination budget 1..65536', chainIds);
  const shifts = new Map(moves.flatMap(m => m.instances.map(id => [id, m])));
  const rotated = planFacilityRotations(w, catalogs, moves, facilityRotations);
  const shift = (p, id) => ({ x: p.x + (shifts.get(id)?.dx ?? 0), y: p.y + (shifts.get(id)?.dy ?? 0) });
  const placements = w.placements.map(p => ({ ...p, ...(rotated.get(p.instance)?.pose ?? shift(p, p.instance)) })), projected = project({ ...w, placements }, catalogs);
  const occupiedPorts = new Set(w.transport_networks.flatMap(n => n.terminals.filter(t => t.endpoint.kind === 'facility').map(t => JSON.stringify([t.endpoint.instance, t.endpoint.port]))));
  const choices = (c, end) => {
    const ts = end ? c.endTerminals : c.startTerminals, p = end ? c.cells.at(-1) : c.cells[0];
    if (ts.length !== 1 || ts[0].endpoint.kind !== 'facility') return [{ position: p, assignment: null }];
    const t = ts[0], retained = { position: rotated.get(t.endpoint.instance)?.terminalPositions.get(t.id) ?? shift(t.position, t.endpoint.instance), assignment: null };
    return [retained, ...projected[t.endpoint.instance].filter(q => q.direction === t.direction && q.transport === c.transport &&
      !occupiedPorts.has(JSON.stringify([t.endpoint.instance, q.id])) &&
      !placements.some(p => q.connection.x >= p.x && q.connection.x < p.x + p.width && q.connection.y >= p.y && q.connection.y < p.y + p.height))
      .map(q => ({ position: q.connection, assignment: { terminal: t.id, port: q.id } }))];
  };
  function* combinations(id) {
    const c = chains.find(c => c.id === id);
    for (const a of choices(c, false)) for (const b of choices(c, true))
      yield { chain: id, portAssignments: [a.assignment, b.assignment].filter(Boolean), endpoint_distance: distance(a.position, b.position) };
  }
  const stream = interleave(chainIds.map(combinations)), seen = new Set(), candidates = []; let examined = 0;
  while (examined < combinationBudget) {
    const next = stream.next(); if (next.done) break; examined++;
    const p = next.value.value, signature = JSON.stringify(p.portAssignments);
    if (!p.portAssignments.length || seen.has(signature)) continue; seen.add(signature);
    candidates.push({ ...p, signature });
  }
  candidates.sort((a, b) => a.endpoint_distance - b.endpoint_distance || a.chain - b.chain || a.signature.localeCompare(b.signature));
  return { combinations_examined: examined, combination_budget: combinationBudget, combination_budget_reached: examined === combinationBudget,
    available_candidates: candidates.length, candidate_limit: limit,
    candidates: candidates.slice(0, limit).map(({ signature, ...p }) => p) };
}
