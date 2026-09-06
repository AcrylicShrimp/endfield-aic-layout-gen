import { connectedGroups } from './groups.mjs';
import { closeTerminalGroups } from './compound.mjs';
import { extractChains, extentOf } from './model.mjs';
import { releaseBridgeDraft, bridgeTopologyView, bridgeReleaseLimit } from './components.mjs';
import { movementPlan } from './movement.mjs';
import { distance, key, directions } from './path.mjs';
import { diagonalProduct, interleave } from './fair.mjs';
const box = ps => ({ x: Math.min(...ps.map(p => p.x)), y: Math.min(...ps.map(p => p.y)),
  right: Math.max(...ps.map(p => p.x + p.width)), bottom: Math.max(...ps.map(p => p.y + p.height)) });
const overlap = (a, b) => a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;
const linkKey = link => JSON.stringify([link.source.id, link.target.id]);
const poseOrder = (a, b) => a.body_envelope_area - b.body_envelope_area || a.displacement - b.displacement || a.signature.localeCompare(b.signature);

function* pairPoses(w, bounds, { a, b, link }) {
  const ids = new Set([...a.instances, ...b.instances]), retained = w.placements.filter(p => !ids.has(p.instance));
  const rb = retained.length ? box(retained) : null;
  function* directionPoses(vx, vy) {
    const relative = { dx: link.source.position.x + vx - link.target.position.x, dy: link.source.position.y + vy - link.target.position.y };
    const moving = w.placements.filter(p => ids.has(p.instance)).map(p => b.instances.includes(p.instance) ? { ...p, x: p.x + relative.dx, y: p.y + relative.dy } : p);
    if (moving.some((p, i) => moving.some((q, j) => i < j && overlap(p, q)))) return;
    const bb = box(moving), xs = new Set([0, -relative.dx, bounds.minX - bb.x, bounds.maxX + 1 - bb.right]), ys = new Set([0, -relative.dy, bounds.minY - bb.y, bounds.maxY + 1 - bb.bottom]);
    for (const q of retained) for (const gap of [0, 1]) {
      xs.add(q.x + q.width + gap - bb.x); xs.add(q.x - gap - bb.right);
      ys.add(q.y + q.height + gap - bb.y); ys.add(q.y - gap - bb.bottom);
    }
    const order = (a, b) => Math.abs(a) - Math.abs(b) || a - b;
    for (const [dx, dy] of diagonalProduct([...xs].sort(order), [...ys].sort(order))) {
      // Null poses still consume the common sampling budget.
      if (bb.x + dx < bounds.minX || bb.right + dx > bounds.maxX + 1 || bb.y + dy < bounds.minY || bb.bottom + dy > bounds.maxY + 1 ||
        moving.some(p => retained.some(q => overlap({ ...p, x: p.x + dx, y: p.y + dy }, q)))) { yield null; continue; }
      const moves = [{ instances: a.instances, dx, dy }, { instances: b.instances, dx: dx + relative.dx, dy: dy + relative.dy }];
      const envelope = rb ? (Math.max(rb.right, bb.right + dx) - Math.min(rb.x, bb.x + dx)) * (Math.max(rb.bottom, bb.bottom + dy) - Math.min(rb.y, bb.y + dy)) : (bb.right - bb.x) * (bb.bottom - bb.y);
      yield { moves, body_envelope_area: envelope, displacement: Math.abs(dx) + Math.abs(dy), signature: JSON.stringify(moves),
        link: { source: link.source.id, target: link.target.id }, kind: 'link-coalescence' };
    }
  }
  for (const { value } of interleave(directions.map(([vx, vy]) => directionPoses(vx, vy)))) yield value;
}

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
  const { witness: w, released } = bridgeTopologyView(detail.witness, catalogs);
  const { chains } = extractChains(w, released), bounds = extentOf(w), groups = closeTerminalGroups(w, connectedGroups(graph, catalogs));
  const pairs = [], links = terminalLinks(chains);
  for (const link of links) for (const a of groups.filter(g => g.instances.includes(link.source.endpoint.instance)))
    for (const b of groups.filter(g => g.instances.includes(link.target.endpoint.instance)))
      if (a.instances.length + b.instances.length <= 16 && !a.instances.some(id => b.instances.includes(id))) pairs.push({ a, b, link });
  pairs.sort((a, b) => distance(b.link.source.position, b.link.target.position) - distance(a.link.source.position, a.link.target.position) ||
    (a.a.instances.length + a.b.instances.length) - (b.a.instances.length + b.b.instances.length));
  const proposals = [], seen = new Set(), rejected = {}, diagnostics = [], raw = pairs.map(() => []), rawSeen = pairs.map(() => new Set());
  const sampledPairs = new Set(), linkCoverage = new Map(links.map(link => [linkKey(link), { source: link.source.id, target: link.target.id,
    labels: [detail.labels[link.source.endpoint.instance], detail.labels[link.target.endpoint.instance]], poses: 0, selected: 0 }]));
  let screened = 0, examined = 0, exhausted = false;
  const stream = interleave(pairs.map(pair => pairPoses(w, bounds, pair)));
  while (examined < poseBudget) {
    const next = stream.next(); if (next.done) break;
    const { index: pair, value: p } = next.value; examined++; sampledPairs.add(pair);
    linkCoverage.get(linkKey(pairs[pair].link)).poses++;
    if (!p || rawSeen[pair].has(p.signature)) continue;
    raw[pair].push({ ...p, pair }); rawSeen[pair].add(p.signature); raw[pair].sort(poseOrder);
    if (raw[pair].length > 64) rawSeen[pair].delete(raw[pair].pop().signature);
  }
  // No unbudgeted lookahead: reaching the cap leaves completion unproven.
  exhausted = examined === poseBudget;
  for (const [pair, { a, b }] of pairs.entries()) {
    if (!raw[pair].length) continue;
    const memberIds = new Set([...a.instances, ...b.instances]);
    const pairBridges = incidentBridgeIds(detail.witness, chains, memberIds);
    if (pairBridges.length > bridgeReleaseLimit) {
      const code = 'local-bridge-release-budget'; rejected[code] = (rejected[code] ?? 0) + 1;
      diagnostics.push({ code, severity: 'info', stage: 'bridge-release', entity: { pair, bridges: pairBridges },
        message: `Coalescence pair needs ${pairBridges.length} bridge releases; local transaction limit is ${bridgeReleaseLimit}` });
      continue;
    }
    const draft = releaseBridgeDraft(detail.witness, catalogs, pairBridges), local = extractChains(draft.witness, draft.released);
    const rerouteChains = new Set(local.chains.filter(c => c.cells.some(p => draft.released.has(`${c.transport}:${key(p)}`))).map(c => c.id));
    let count = 0;
    for (const p of raw[pair]) {
      if (seen.has(p.signature)) continue; seen.add(p.signature);
      screened++;
      try {
        const plan = movementPlan(draft.witness, local.chains, local.anchors, p.moves, { junctionPolicy, rerouteChains });
        const carried = [...plan.fixedChains.values()].flat();
        for (const n of w.transport_networks) for (const t of n.terminals) carried.push(plan.shiftTerminal(t));
        if (carried.some(p => p.x < bounds.minX || p.x > bounds.maxX || p.y < bounds.minY || p.y > bounds.maxY)) continue;
        const delta = local.chains.reduce((s, c) => s + distance(plan.shiftEndpoint(c, false), plan.shiftEndpoint(c, true)) - distance(c.cells[0], c.cells.at(-1)), 0);
        const { signature, displacement, ...candidate } = p;
        proposals.push({ ...candidate, releaseBridges: pairBridges, endpoint_delta: delta }); if (++count === 8) break;
      } catch (e) { if (!e.code) throw e; rejected[e.code] = (rejected[e.code] ?? 0) + 1; }
    }
  }
  const ranks = new Map();
  proposals.sort((a, b) => a.body_envelope_area - b.body_envelope_area || a.endpoint_delta - b.endpoint_delta || a.pair - b.pair);
  for (const p of proposals) { const k = JSON.stringify([p.link.source, p.link.target]), rank = ranks.get(k) ?? 0; p.rank = rank; ranks.set(k, rank + 1); }
  const candidates = proposals.sort((a, b) => a.rank - b.rank || a.body_envelope_area - b.body_envelope_area || a.endpoint_delta - b.endpoint_delta).slice(0, limit);
  for (const p of candidates) linkCoverage.get(JSON.stringify([p.link.source, p.link.target])).selected++;
  return { groups: groups.length, pairs_available: pairs.length, pairs_considered: sampledPairs.size, pose_budget: poseBudget, poses_examined: examined, pose_budget_exhausted: exhausted,
    links_available: new Set(pairs.map(p => linkKey(p.link))).size, links_sampled: [...linkCoverage.values()].filter(l => l.poses).length,
    links_selected: [...linkCoverage.values()].filter(l => l.selected).length, link_coverage: [...linkCoverage.values()], raw_pose_limit_per_pair: 64, valid_pose_limit_per_pair: 8,
    screened, rejected, diagnostics, feasible_positions: proposals.length, candidates };
}
