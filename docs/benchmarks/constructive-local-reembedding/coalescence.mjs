import { connectedGroups } from './groups.mjs';
import { closeTerminalGroups } from './compound.mjs';
import { extractChains, extentOf } from './model.mjs';
import { releaseBridgeDraft } from './components.mjs';
import { movementPlan } from './movement.mjs';
import { distance, key, directions } from './path.mjs';
const box = ps => ({ x: Math.min(...ps.map(p => p.x)), y: Math.min(...ps.map(p => p.y)),
  right: Math.max(...ps.map(p => p.x + p.width)), bottom: Math.max(...ps.map(p => p.y + p.height)) });
const overlap = (a, b) => a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;

export function incidentBridgeIds(w, chains, memberIds) {
  const incident = chains.filter(c => [...c.startTerminals, ...c.endTerminals].some(t => memberIds.has(t.endpoint.instance)));
  return w.logistics_components.filter(c => c.kind === 'bridge' && incident.some(j => j.transport === c.transport && j.cells.some(p => key(p) === key(c.position)))).map(c => c.id);
}

export function terminalLinks(chains) {
  const links = new Map(), starts = new Map();
  for (const c of chains) { const k = `${c.network}:${key(c.cells[0])}`; if (!starts.has(k)) starts.set(k, []); starts.get(k).push(c); }
  for (const root of chains) for (const source of root.startTerminals.filter(t => t.direction === 'output' && t.endpoint.kind === 'facility')) {
    const queue = [root], seen = new Set();
    for (let i = 0; i < queue.length; i++) {
      const c = queue[i]; if (seen.has(c.id)) continue; seen.add(c.id);
      const sinks = c.endTerminals.filter(t => t.direction === 'input' && t.endpoint.kind === 'facility');
      for (const target of sinks) if (source.endpoint.instance !== target.endpoint.instance)
        links.set(JSON.stringify([source.id, target.id]), { source, target, transport: c.transport, item: c.item });
      if (!sinks.length) queue.push(...(starts.get(`${c.network}:${key(c.cells.at(-1))}`) ?? []));
    }
  }
  return [...links.values()];
}

export function coalescenceCandidates(detail, graph, catalogs, { limit = 128, poseBudget = 250000, junctionPolicy = 'downstream' } = {}) {
  if (!Number.isSafeInteger(limit) || limit < 1 || limit > 512 || !Number.isSafeInteger(poseBudget) || poseBudget < 1 || poseBudget > 2000000)
    throw Object.assign(new Error('Expected candidate limit 1..512 and pose budget 1..2000000'), { code: 'coalescence-budget' });
  const releaseBridges = detail.witness.logistics_components.filter(c => c.kind === 'bridge').map(c => c.id);
  const { witness: w, released } = releaseBridgeDraft(detail.witness, catalogs, releaseBridges);
  const { chains } = extractChains(w, released), bounds = extentOf(w), groups = closeTerminalGroups(w, connectedGroups(graph, catalogs));
  const pairs = [], links = terminalLinks(chains);
  for (const link of links) for (const a of groups.filter(g => g.instances.includes(link.source.endpoint.instance)))
    for (const b of groups.filter(g => g.instances.includes(link.target.endpoint.instance)))
      if (a.instances.length + b.instances.length <= 16 && !a.instances.some(id => b.instances.includes(id))) pairs.push({ a, b, link });
  pairs.sort((a, b) => distance(b.link.source.position, b.link.target.position) - distance(a.link.source.position, a.link.target.position) ||
    (a.a.instances.length + a.b.instances.length) - (b.a.instances.length + b.b.instances.length));
  const proposals = [], seen = new Set(), rejected = {}; let screened = 0, examined = 0, exhausted = false, pairsConsidered = 0;
  outer: for (const [pair, { a, b, link }] of pairs.entries()) {
    pairsConsidered++;
    const memberIds = new Set([...a.instances, ...b.instances]);
    const pairBridges = incidentBridgeIds(detail.witness, chains, memberIds);
    const draft = releaseBridgeDraft(detail.witness, catalogs, pairBridges), local = extractChains(draft.witness, draft.released);
    const rerouteChains = new Set(local.chains.filter(c => c.cells.some(p => draft.released.has(`${c.transport}:${key(p)}`))).map(c => c.id));
    for (const [vx, vy] of directions) {
    const relative = { dx: link.source.position.x + vx - link.target.position.x, dy: link.source.position.y + vy - link.target.position.y };
    const ids = new Set([...a.instances, ...b.instances]);
    const moving = w.placements.filter(p => ids.has(p.instance)).map(p => b.instances.includes(p.instance) ? { ...p, x: p.x + relative.dx, y: p.y + relative.dy } : p);
    if (moving.some((p, i) => moving.some((q, j) => i < j && overlap(p, q)))) continue;
    const retained = w.placements.filter(p => !ids.has(p.instance)), bb = box(moving), rb = box(retained);
    const xs = new Set([0, -relative.dx, bounds.minX - bb.x, bounds.maxX + 1 - bb.right]), ys = new Set([0, -relative.dy, bounds.minY - bb.y, bounds.maxY + 1 - bb.bottom]);
    for (const q of retained) for (const gap of [0, 1]) {
      xs.add(q.x + q.width + gap - bb.x); xs.add(q.x - gap - bb.right);
      ys.add(q.y + q.height + gap - bb.y); ys.add(q.y - gap - bb.bottom);
    }
    const raw = [];
    for (const dx of xs) for (const dy of ys) {
      if (examined++ >= poseBudget) { exhausted = true; break outer; }
      if (bb.x + dx < bounds.minX || bb.right + dx > bounds.maxX + 1 || bb.y + dy < bounds.minY || bb.bottom + dy > bounds.maxY + 1) continue;
      if (moving.some(p => retained.some(q => overlap({ ...p, x: p.x + dx, y: p.y + dy }, q)))) continue;
      const moves = [{ instances: a.instances, dx, dy }, { instances: b.instances, dx: dx + relative.dx, dy: dy + relative.dy }];
      const signature = JSON.stringify(moves); if (seen.has(signature)) continue; seen.add(signature);
      const envelope = (Math.max(rb.right, bb.right + dx) - Math.min(rb.x, bb.x + dx)) * (Math.max(rb.bottom, bb.bottom + dy) - Math.min(rb.y, bb.y + dy));
      raw.push({ moves, body_envelope_area: envelope, pair, link: { source: link.source.id, target: link.target.id }, kind: 'link-coalescence' });
    }
    raw.sort((a, b) => a.body_envelope_area - b.body_envelope_area || Math.abs(a.moves[0].dx) + Math.abs(a.moves[0].dy) - Math.abs(b.moves[0].dx) - Math.abs(b.moves[0].dy));
    let count = 0;
    for (const p of raw.slice(0, 128)) {
      screened++;
      try {
        const plan = movementPlan(draft.witness, local.chains, local.anchors, p.moves, { junctionPolicy, rerouteChains });
        const carried = [...plan.fixedChains.values()].flat();
        for (const n of w.transport_networks) for (const t of n.terminals) carried.push(plan.shiftTerminal(t));
        if (carried.some(p => p.x < bounds.minX || p.x > bounds.maxX || p.y < bounds.minY || p.y > bounds.maxY)) continue;
        const delta = local.chains.reduce((s, c) => s + distance(plan.shiftEndpoint(c, false), plan.shiftEndpoint(c, true)) - distance(c.cells[0], c.cells.at(-1)), 0);
        proposals.push({ ...p, releaseBridges: pairBridges, endpoint_delta: delta, rank: count++ }); if (count === 32) break;
      } catch (e) { if (!e.code) throw e; rejected[e.code] = (rejected[e.code] ?? 0) + 1; }
    }
  }
  }
  return { groups: groups.length, pairs_available: pairs.length, pairs_considered: pairsConsidered, pose_budget: poseBudget, poses_examined: Math.min(examined, poseBudget), pose_budget_exhausted: exhausted,
    screened, rejected, feasible_positions: proposals.length,
    candidates: proposals.sort((a, b) => a.body_envelope_area - b.body_envelope_area || a.rank - b.rank || a.endpoint_delta - b.endpoint_delta).slice(0, limit) };
}
