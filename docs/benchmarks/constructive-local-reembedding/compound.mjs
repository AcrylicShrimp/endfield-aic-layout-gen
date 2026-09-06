import { connectedGroups } from './groups.mjs';
import { extractChains, extentOf } from './model.mjs';
import { movementPlan } from './movement.mjs';
import { distance } from './path.mjs';
import { detachablePairs } from './terminal-identity.mjs';
const box = ps => ({ x: Math.min(...ps.map(p => p.x)), y: Math.min(...ps.map(p => p.y)),
  right: Math.max(...ps.map(p => p.x + p.width)), bottom: Math.max(...ps.map(p => p.y + p.height)) });

export function closeTerminalGroups(w, seeds, limit = 16) {
  if (!Number.isSafeInteger(limit) || limit < 1 || limit > 16) throw Object.assign(new Error('Expected membership limit 1..16'), { code: 'compound-budget' });
  const ids = new Set(w.placements.map(p => p.instance)), adjacent = new Map([...ids].map(id => [id, new Set()])), fixed = new Set(), slots = new Map();
  const detachable = detachablePairs(w);
  for (const n of w.transport_networks) for (const t of n.terminals) {
    const k = `${n.transport}:${t.position.x},${t.position.y}`; if (!slots.has(k)) slots.set(k, []); slots.get(k).push(t.endpoint);
  }
  for (const [slot, owners] of slots) {
    if (detachable.has(slot)) continue;
    const members = owners.filter(e => e.kind === 'facility').map(e => e.instance);
    if (members.some(id => !ids.has(id))) throw Object.assign(new Error('Unknown shared-terminal owner'), { code: 'compound-owner' });
    if (owners.some(e => e.kind === 'external')) members.forEach(id => fixed.add(id));
    for (const a of members) for (const b of members) if (a !== b) adjacent.get(a).add(b);
  }
  const result = new Map();
  for (const seed of [...seeds, ...[...ids].map(id => ({ instances: [id], kind: 'singleton' }))]) {
    if (seed.instances.some(id => !ids.has(id))) throw Object.assign(new Error('Unknown group seed'), { code: 'compound-owner' });
    const members = new Set(seed.instances), queue = [...members];
    for (let i = 0; i < queue.length; i++) for (const next of adjacent.get(queue[i])) if (!members.has(next)) { members.add(next); queue.push(next); }
    if (members.size > limit || [...members].some(id => fixed.has(id))) continue;
    const instances = [...members].sort(), signature = JSON.stringify(instances);
    if (!result.has(signature)) result.set(signature, { instances, kind: 'terminal-closed' });
  }
  return [...result.values()].sort((a, b) => JSON.stringify(a.instances).localeCompare(JSON.stringify(b.instances)));
}

export function compoundCandidates(detail, graph, catalogs, { limit = 128, pairBudget = 20000, junctionPolicy = 'downstream' } = {}) {
  if (!Number.isSafeInteger(limit) || limit < 1 || limit > 512 || !Number.isSafeInteger(pairBudget) || pairBudget < 1 || pairBudget > 50000)
    throw Object.assign(new Error('Expected compound limit 1..512 and pair budget 1..50000'), { code: 'compound-budget' });
  const w = detail.witness, bounds = extentOf(w), { chains, anchors } = extractChains(w);
  const groups = closeTerminalGroups(w, connectedGroups(graph, catalogs)).map(g => ({ ...g, box: box(w.placements.filter(p => g.instances.includes(p.instance))) }));
  const proposals = [], rejected = new Map(); let screened = 0, pairs = 0, exhausted = false;
  outer: for (let i = 0; i < groups.length; i++) for (let j = i + 1; j < groups.length; j++) {
    if (pairs >= pairBudget) { exhausted = true; break outer; } pairs++;
    const a = groups[i], b = groups[j]; if (a.instances.some(id => b.instances.includes(id)) || a.instances.length + b.instances.length > 16) continue;
    const offsets = [{ kind: 'exchange', ax: b.box.x - a.box.x, ay: b.box.y - a.box.y, bx: a.box.x - b.box.x, by: a.box.y - b.box.y }];
    for (const axis of ['x', 'y']) {
      const end = axis === 'x' ? 'right' : 'bottom', cross = axis === 'x' ? 'y' : 'x';
      const [first, second] = a.box[axis] <= b.box[axis] ? [a, b] : [b, a];
      if (first.box[end] > second.box[axis]) continue;
      for (const gap of [0, 1]) for (const align of [false, true]) {
        const start = first.box[axis], ds = start - second.box[axis], df = start + second.box[end] - second.box[axis] + gap - first.box[axis];
        const shifts = new Map([[first, { [axis]: df, [cross]: 0 }], [second, { [axis]: ds, [cross]: align ? first.box[cross] - second.box[cross] : 0 }]]);
        offsets.push({ kind: `reverse-${axis}-${gap}-${align ? 'aligned' : 'retained'}`, ax: shifts.get(a).x, ay: shifts.get(a).y, bx: shifts.get(b).x, by: shifts.get(b).y });
      }
    }
    const seen = new Set();
    for (const o of offsets) {
      const signature = JSON.stringify([o.ax, o.ay, o.bx, o.by]); if (seen.has(signature)) continue; seen.add(signature);
      const moves = [{ instances: a.instances, dx: o.ax, dy: o.ay }, { instances: b.instances, dx: o.bx, dy: o.by }];
      const shifts = new Map(moves.flatMap(m => m.instances.map(id => [id, m]))), ps = w.placements.map(p => {
        const d = shifts.get(p.instance); return d ? { ...p, x: p.x + d.dx, y: p.y + d.dy } : p;
      });
      if (ps.some(p => p.x < bounds.minX || p.y < bounds.minY || p.x + p.width - 1 > bounds.maxX || p.y + p.height - 1 > bounds.maxY)) continue;
      screened++;
      if (ps.some((p, index) => shifts.has(p.instance) && ps.some((q, other) => index !== other &&
        p.x < q.x + q.width && q.x < p.x + p.width && p.y < q.y + q.height && q.y < p.y + p.height))) {
        rejected.set('local-body-collision', (rejected.get('local-body-collision') ?? 0) + 1); continue;
      }
      let plan;
      try {
        plan = movementPlan(w, chains, anchors, moves, { junctionPolicy });
        const carried = [...plan.fixedChains.values()].flat();
        for (const n of w.transport_networks) for (const t of n.terminals) carried.push(plan.shiftTerminal(t));
        if (carried.some(p => p.x < bounds.minX || p.y < bounds.minY || p.x > bounds.maxX || p.y > bounds.maxY)) continue;
      } catch (e) { if (!e.code) throw e; rejected.set(e.code, (rejected.get(e.code) ?? 0) + 1); continue; }
      const bb = box(ps), envelope = (bb.right - bb.x) * (bb.bottom - bb.y);
      const delta = chains.reduce((n, c) => n + distance(plan.shiftEndpoint(c, false), plan.shiftEndpoint(c, true)) - distance(c.cells[0], c.cells.at(-1)), 0);
      proposals.push({ moves, kind: o.kind, pair: [i, j], labels: moves.map(m => m.instances.map(id => detail.labels[id])), body_envelope_area: envelope, endpoint_delta: delta });
    }
  }
  // Retain at most two positions per pair before deepening a pair's variant list.
  const counts = new Map();
  proposals.sort((a, b) => a.body_envelope_area - b.body_envelope_area || a.endpoint_delta - b.endpoint_delta || a.pair[0] - b.pair[0] || a.pair[1] - b.pair[1]);
  for (const p of proposals) { const k = JSON.stringify(p.pair), n = counts.get(k) ?? 0; counts.set(k, n + 1); p.rank = Math.floor(n / 2); }
  return { groups: groups.length, screened, pairs_considered: pairs, pair_budget: pairBudget, pair_budget_exhausted: exhausted,
    feasible_positions: proposals.length, rejected: Object.fromEntries(rejected),
    candidates: proposals.sort((a, b) => a.rank - b.rank || a.body_envelope_area - b.body_envelope_area || a.endpoint_delta - b.endpoint_delta).slice(0, limit) };
}
