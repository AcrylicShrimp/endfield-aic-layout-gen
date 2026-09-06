import { connectedGroups, emptyRectangles } from './groups.mjs';
import { extractChains, extentOf } from './model.mjs';
import { movementPlan } from './movement.mjs';
import { bridgeTopologyView, releaseBridgeDraft, bridgeReleaseLimit } from './components.mjs';
import { incidentBridgeIds } from './coalescence.mjs';
import { detachablePairs } from './terminal-identity.mjs';
import { interleave } from './fair.mjs';
import { key, distance, directions } from './path.mjs';
const overlap = (p, q) => p.x < q.x + q.width && q.x < p.x + p.width && p.y < q.y + q.height && q.y < p.y + p.height;
const contains = (p, q) => q.x >= p.x && q.x < p.x + p.width && q.y >= p.y && q.y < p.y + p.height;
const box = ps => ({ minX: Math.min(...ps.map(p => p.x)), minY: Math.min(...ps.map(p => p.y)),
  maxX: Math.max(...ps.map(p => p.x + p.width - 1)), maxY: Math.max(...ps.map(p => p.y + p.height - 1)) });
const diagnostic = (code, message, entity) => ({ code, message, entity, severity: 'info', stage: 'geometric-closure' });

export function closeGeometry(w, seedIds, dx, dy, { maxMembers = 16 } = {}) {
  const ids = new Set(w.placements.map(p => p.instance));
  if (!Array.isArray(seedIds) || !seedIds.length || new Set(seedIds).size !== seedIds.length || seedIds.some(id => !ids.has(id)) ||
    ![dx, dy, maxMembers].every(Number.isSafeInteger) || !dx && !dy || maxMembers < 1 || maxMembers > 16)
    throw Object.assign(new Error('Expected known unique seed IDs, nonzero integer translation and member budget 1..16'),
      diagnostic('push-invalid-input', 'Invalid geometric closure input', seedIds));
  const selected = new Set(seedIds), additions = [], ps = [...w.placements].sort((a, b) => a.instance.localeCompare(b.instance));
  const ts = w.transport_networks.flatMap(n => n.terminals.map(t => ({ ...t, transport: n.transport })));
  const slots = new Map(), detached = detachablePairs(w);
  for (const t of ts) { const k = `${t.transport}:${key(t.position)}`; if (!slots.has(k)) slots.set(k, []); slots.get(k).push(t); }
  const owns = t => t.endpoint.kind === 'facility' && selected.has(t.endpoint.instance);
  const shift = p => ({ x: p.x + dx, y: p.y + dy });
  let failure = seedIds.length > maxMembers ? diagnostic('push-member-budget', 'Seed exceeds the member budget', { members: seedIds.length, maxMembers }) : null, rounds = 0;
  const requireOwner = (id, reason, trigger) => {
    if (failure || selected.has(id)) return;
    if (!ids.has(id)) { failure = diagnostic('push-invalid-input', 'Terminal references an unknown facility', id); return; }
    if (selected.size === maxMembers) { failure = diagnostic('push-member-budget', 'Contact closure exceeds the member budget', { instance: id, reason, trigger, maxMembers }); return; }
    selected.add(id); additions.push({ instance: id, reason, trigger });
  };
  const requireTerminal = (t, reason, trigger) => {
    if (failure) return;
    if (t.endpoint.kind === 'external') failure = diagnostic('push-fixed-external', 'Translation contacts an immutable external terminal', { terminal: t.id, reason, trigger });
    else requireOwner(t.endpoint.instance, reason, trigger);
  };
  while (!failure) {
    rounds++; const before = selected.size;
    for (const [k, owners] of slots) if (!detached.has(k) && owners.some(owns))
      for (const t of owners) if (!owns(t)) requireTerminal(t, 'shared-terminal', k);
    for (const p of ps) if (selected.has(p.instance)) {
      const moved = { ...p, ...shift(p) };
      for (const q of ps) if (!selected.has(q.instance) && overlap(moved, q)) requireOwner(q.instance, 'body-body', p.instance);
      for (const t of ts) if (!owns(t) && contains(moved, t.position)) requireTerminal(t, 'body-terminal', p.instance);
    }
    for (const t of ts) if (owns(t)) {
      const position = shift(t.position);
      for (const p of ps) if (!selected.has(p.instance) && contains(p, position)) requireOwner(p.instance, 'terminal-body', t.id);
      for (const q of slots.get(`${t.transport}:${key(position)}`) ?? []) if (!owns(q)) requireTerminal(q, 'terminal-terminal', t.id);
    }
    if (selected.size === before) break;
  }
  return { status: failure ? 'rejected' : 'closed', instances: [...selected].sort(), additions, rounds, ...(failure ? { diagnostic: failure } : {}) };
}

export function boundaryCohorts(w, { maxMembers = 16, bandLimit = 4 } = {}) {
  if (!Number.isSafeInteger(maxMembers) || maxMembers < 1 || maxMembers > 16 || !Number.isSafeInteger(bandLimit) || bandLimit < 1 || bandLimit > 16)
    throw Object.assign(new Error('Expected cohort member and band budgets 1..16'), diagnostic('push-invalid-input', 'Invalid boundary seed budgets', null));
  const seeds = new Map(), diagnostics = [];
  if (!w.placements.length) return { seeds: [], diagnostics };
  const b = box(w.placements);
  for (const side of ['minX', 'maxX', 'minY', 'maxY']) for (let band = 1; band <= bandLimit; band++) {
    const instances = w.placements.filter(p => side === 'minX' ? p.x <= b.minX + band - 1 : side === 'maxX' ? p.x + p.width - 1 >= b.maxX - band + 1 :
      side === 'minY' ? p.y <= b.minY + band - 1 : p.y + p.height - 1 >= b.maxY - band + 1).map(p => p.instance).sort();
    const signature = JSON.stringify(instances); if (seeds.has(signature)) continue;
    if (instances.length > maxMembers) { diagnostics.push(diagnostic('push-boundary-seed-budget', 'Envelope support cohort exceeds the member budget', { side, band, members: instances.length, maxMembers })); continue; }
    seeds.set(signature, { instances, kind: 'boundary', boundary: { side, band } });
  }
  return { seeds: [...seeds.values()], diagnostics };
}

export function pushCandidates(detail, graph, catalogs, { limit = 128, poseBudget = 4096, maxMembers = 16, junctionPolicy = 'downstream', seedPolicy = 'contact' } = {}) {
  if (!Number.isSafeInteger(limit) || limit < 1 || limit > 512 || !Number.isSafeInteger(poseBudget) || poseBudget < 1 || poseBudget > 65536 ||
    !Number.isSafeInteger(maxMembers) || maxMembers < 1 || maxMembers > 16 || !['internal', 'downstream'].includes(junctionPolicy) || !['contact', 'boundary'].includes(seedPolicy))
    throw Object.assign(new Error('Invalid geometric-push candidate, pose, member or junction budget'), diagnostic('push-invalid-input', 'Invalid proposal budgets', null));
  const w = detail.witness, bounds = extentOf(w), voids = emptyRectangles(w), topology = bridgeTopologyView(w, catalogs), indexed = extractChains(topology.witness, topology.released).chains;
  const seeds = new Map(), boundary = seedPolicy === 'boundary' ? boundaryCohorts(w, { maxMembers }) : { seeds: [], diagnostics: [] };
  for (const g of [...w.placements.map(p => ({ instances: [p.instance] })), ...connectedGroups(graph, catalogs)]) {
    const instances = [...g.instances].sort(); if (instances.length <= maxMembers) seeds.set(JSON.stringify(instances), { instances, kind: 'contact' });
  }
  for (const g of boundary.seeds) if (!seeds.has(JSON.stringify(g.instances))) seeds.set(JSON.stringify(g.instances), g);
  function* poses({ instances, kind, boundary }) {
    const b = box(w.placements.filter(p => instances.includes(p.instance))), seen = new Set();
    function* cardinal() { for (let step = 1; step <= 8; step++) for (const [x, y] of directions) yield { dx: x * step, dy: y * step }; }
    function* aligned() {
      for (const r of voids) if (b.maxX - b.minX + 1 <= r.width && b.maxY - b.minY + 1 <= r.height)
        for (const x of [r.x, r.x + r.width - (b.maxX - b.minX + 1)]) for (const y of [r.y, r.y + r.height - (b.maxY - b.minY + 1)]) yield { dx: x - b.minX, dy: y - b.minY };
    }
    for (const { value: shift } of interleave([cardinal(), aligned()])) { const k = JSON.stringify(shift); if (!shift.dx && !shift.dy || seen.has(k)) continue; seen.add(k); yield { instances, seed_kind: kind, boundary, ...shift }; }
  }
  const stream = interleave([...seeds.values()].map(poses)), seen = new Set(), proposals = [], rejected = {}, diagnostics = [];
  const record = d => { rejected[d.code] = (rejected[d.code] ?? 0) + 1; if (diagnostics.length < 64) diagnostics.push(d); };
  boundary.diagnostics.forEach(record);
  let examined = 0, closed = 0, grown = 0, exhausted = false;
  while (examined < poseBudget) {
    const next = stream.next(); if (next.done) { exhausted = true; break; } examined++;
    const { value: p, index: seed } = next.value, closure = closeGeometry(w, p.instances, p.dx, p.dy, { maxMembers });
    if (closure.status !== 'closed') { record(closure.diagnostic); continue; } closed++; if (closure.additions.length) grown++;
    const move = { instances: closure.instances, dx: p.dx, dy: p.dy }, signature = JSON.stringify(move);
    if (seen.has(signature)) continue; seen.add(signature);
    const memberIds = new Set(move.instances), placements = w.placements.map(q => memberIds.has(q.instance) ? { ...q, x: q.x + p.dx, y: q.y + p.dy } : q);
    const b = box(placements);
    if (b.minX < bounds.minX || b.maxX > bounds.maxX || b.minY < bounds.minY || b.maxY > bounds.maxY) { record(diagnostic('push-bounds', 'Closed bodies leave the retained extent', seed)); continue; }
    const releaseBridges = [...new Set([...incidentBridgeIds(w, indexed, memberIds), ...w.logistics_components.filter(c => c.kind === 'bridge' &&
      placements.some(q => memberIds.has(q.instance) && contains(q, c.position))).map(c => c.id)])];
    if (releaseBridges.length > bridgeReleaseLimit) { record(diagnostic('push-bridge-budget', 'Proposal exceeds the local bridge-release budget', seed)); continue; }
    try {
      const draft = releaseBridgeDraft(w, catalogs, releaseBridges), local = extractChains(draft.witness, draft.released);
      const rerouteChains = new Set(local.chains.filter(c => c.cells.some(q => draft.released.has(`${c.transport}:${key(q)}`))).map(c => c.id));
      const plan = movementPlan(draft.witness, local.chains, local.anchors, [move], { junctionPolicy, rerouteChains });
      const points = [...plan.fixedChains.values()].flat().concat(w.transport_networks.flatMap(n => n.terminals.map(t => plan.shiftTerminal(t))));
      if (points.some(q => q.x < bounds.minX || q.x > bounds.maxX || q.y < bounds.minY || q.y > bounds.maxY)) { record(diagnostic('push-bounds', 'Carried geometry leaves the retained extent', seed)); continue; }
      const delta = local.chains.reduce((s, c) => s + distance(plan.shiftEndpoint(c, false), plan.shiftEndpoint(c, true)) - distance(c.cells[0], c.cells.at(-1)), 0);
      proposals.push({ moves: [move], seed, seed_instances: p.instances, seed_kind: p.seed_kind, boundary: p.boundary, closure, releaseBridges,
        body_envelope_area: (b.maxX - b.minX + 1) * (b.maxY - b.minY + 1), endpoint_delta: delta, kind: 'geometric-push' });
    } catch (e) { if (!e.code) throw e; record({ code: e.code, severity: e.severity, stage: e.stage, message: e.message, entity: e.entity }); }
  }
  proposals.sort((a, b) => a.body_envelope_area - b.body_envelope_area || a.endpoint_delta - b.endpoint_delta || a.seed - b.seed);
  const counts = new Map(); for (const p of proposals) { p.rank = counts.get(p.seed) ?? 0; counts.set(p.seed, p.rank + 1); }
  return { groups: seeds.size, seed_policy: seedPolicy, boundary_seeds: [...seeds.values()].filter(g => g.kind === 'boundary').length,
    voids, poses_examined: examined, pose_budget: poseBudget, pose_budget_exhausted: !exhausted, member_budget: maxMembers,
    closures: closed, grown_closures: grown, rejected, diagnostics, diagnostics_truncated: Object.values(rejected).reduce((a, b) => a + b, 0) > diagnostics.length,
    feasible_positions: proposals.length, candidates: proposals.sort((a, b) => a.rank - b.rank || a.body_envelope_area - b.body_envelope_area || a.endpoint_delta - b.endpoint_delta || a.seed - b.seed).slice(0, limit) };
}
