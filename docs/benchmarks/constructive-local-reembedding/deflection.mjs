import { boundaryCohorts, closeGeometry } from './push.mjs';
import { emptyRectangles } from './groups.mjs';
import { extractChains, extentOf } from './model.mjs';
import { movementPlan } from './movement.mjs';
import { bridgeTopologyView, releaseBridgeDraft, bridgeReleaseLimit } from './components.mjs';
import { incidentBridgeIds } from './coalescence.mjs';
import { interleave } from './fair.mjs';
import { key, distance, directions } from './path.mjs';
import { planFacilityRotations } from './facility-rotation.mjs';
import { rotateFacility } from '../constructive-placement-only/model.mjs';
const contains = (p, q) => q.x >= p.x && q.x < p.x + p.width && q.y >= p.y && q.y < p.y + p.height;
const box = ps => ({ minX: Math.min(...ps.map(p => p.x)), minY: Math.min(...ps.map(p => p.y)),
  maxX: Math.max(...ps.map(p => p.x + p.width - 1)), maxY: Math.max(...ps.map(p => p.y + p.height - 1)) });
const diagnostic = (code, message, entity) => ({ code, message, entity, severity: 'info', stage: 'group-deflection' });

export function deflectionCandidates(detail, catalogs, { limit = 128, poseBudget = 16384, stepLimit = 4, maxMembers = 16, junctionPolicy = 'downstream', rotationPolicy = 'retained' } = {}) {
  if (!Number.isSafeInteger(limit) || limit < 1 || limit > 512 || !Number.isSafeInteger(poseBudget) || poseBudget < 1 || poseBudget > 65536 ||
    !Number.isSafeInteger(stepLimit) || stepLimit < 1 || stepLimit > 8 || !Number.isSafeInteger(maxMembers) || maxMembers < 2 || maxMembers > 16 || !['internal', 'downstream'].includes(junctionPolicy) || !['retained', 'catalog'].includes(rotationPolicy))
    throw Object.assign(new Error('Invalid deflection candidate, pose, step, member or junction budget'), diagnostic('deflection-invalid-input', 'Invalid proposal options', null));
  const w = detail.witness, bounds = extentOf(w), voids = emptyRectangles(w), boundary = boundaryCohorts(w, { maxMembers });
  const groups = [], seenGroups = new Set(), rejected = {}, diagnostics = [];
  const record = d => { rejected[d.code] = (rejected[d.code] ?? 0) + 1; if (diagnostics.length < 64) diagnostics.push(d); };
  boundary.diagnostics.forEach(record);
  for (const seed of boundary.seeds) for (let step = 1; step <= stepLimit; step++) {
    const [dx, dy] = { minX: [step, 0], maxX: [-step, 0], minY: [0, step], maxY: [0, -step] }[seed.boundary.side];
    const closure = closeGeometry(w, seed.instances, dx, dy, { maxMembers });
    if (closure.status !== 'closed') { record(closure.diagnostic); continue; }
    const signature = JSON.stringify([closure.instances, dx, dy]);
    if (closure.instances.length < 2 || seenGroups.has(signature)) continue;
    seenGroups.add(signature); groups.push({ seed, closure, dx, dy });
  }
  function* poses(group, instance) {
    const body = w.placements.find(p => p.instance === instance), seen = new Set(), spec = rotationPolicy === 'catalog' ? catalogs.facilities.facilities.find(f => f.id === body.facility) : null;
    const shapes = rotationPolicy === 'catalog' ? [body.rotation, ...spec.allowed_rotations.filter(r => r !== body.rotation)].map(r => rotateFacility(spec, r)) : [body];
    function* variant(shape) {
      function* aligned() {
        for (const r of voids) if (shape.width <= r.width && shape.height <= r.height)
          for (const x of [r.x, r.x + r.width - shape.width]) for (const y of [r.y, r.y + r.height - shape.height]) yield { dx: x - body.x, dy: y - body.y, void_candidate: true };
      }
      function* nearby() {
        if (shape.rotation !== body.rotation) yield { dx: 0, dy: 0 };
        for (let step = 1; step <= 8; step++) for (const [dx, dy] of directions) yield { dx: step * dx, dy: step * dy };
      }
      for (const { value: shift } of interleave([aligned(), nearby()])) {
        const signature = JSON.stringify([shift.dx, shift.dy, shape.rotation]);
        if (shift.dx === group.dx && shift.dy === group.dy && shape.rotation === body.rotation || seen.has(signature)) continue;
        seen.add(signature);
        yield { group, instance, void_candidate: shift.void_candidate ?? false, facilityRotations: shape.rotation === body.rotation ? [] : [{ instance, rotation: shape.rotation }],
          moves: [{ instances: group.closure.instances.filter(id => id !== instance), dx: group.dx, dy: group.dy }, { instances: [instance], dx: shift.dx, dy: shift.dy }] };
      }
    }
    for (const { value } of interleave(shapes.map(variant))) yield value;
  }
  const topology = bridgeTopologyView(w, catalogs), indexed = extractChains(topology.witness, topology.released).chains;
  const stream = interleave(groups.flatMap(g => g.closure.instances.map(id => poses(g, id)))), seen = new Set(), cache = new Map(), proposals = [];
  let examined = 0, exhausted = false;
  while (examined < poseBudget) {
    const next = stream.next(); if (next.done) { exhausted = true; break; } examined++;
    const { value: p, index: origin } = next.value, signature = JSON.stringify([p.moves, p.facilityRotations]); if (seen.has(signature)) continue; seen.add(signature);
    const shifts = new Map(p.moves.flatMap(m => m.instances.map(id => [id, m]))), memberIds = new Set(shifts.keys());
    const facilityChanges = planFacilityRotations(w, catalogs, p.moves, p.facilityRotations);
    const placements = w.placements.map(q => { const s = shifts.get(q.instance); return s ? { ...q, ...(facilityChanges.get(q.instance)?.pose ?? { x: q.x + s.dx, y: q.y + s.dy }) } : q; }), b = box(placements);
    if (b.minX < bounds.minX || b.maxX > bounds.maxX || b.minY < bounds.minY || b.maxY > bounds.maxY) { record(diagnostic('deflection-bounds', 'Proposed bodies leave the retained extent', origin)); continue; }
    const releaseBridges = [...new Set([...incidentBridgeIds(w, indexed, memberIds), ...w.logistics_components.filter(c => c.kind === 'bridge' &&
      placements.some(q => memberIds.has(q.instance) && contains(q, c.position))).map(c => c.id)])].sort();
    if (releaseBridges.length > bridgeReleaseLimit) { record(diagnostic('deflection-bridge-budget', 'Proposal exceeds the local bridge-release budget', origin)); continue; }
    try {
      const signature = JSON.stringify(releaseBridges);
      if (!cache.has(signature)) {
        const draft = releaseBridgeDraft(w, catalogs, releaseBridges), local = extractChains(draft.witness, draft.released);
        const rerouteChains = new Set(local.chains.filter(c => c.cells.some(q => draft.released.has(`${c.transport}:${key(q)}`))).map(c => c.id));
        cache.set(signature, { draft, local, rerouteChains });
      }
      const { draft, local, rerouteChains } = cache.get(signature), plan = movementPlan(draft.witness, local.chains, local.anchors, p.moves, { junctionPolicy, facilityChanges, rerouteChains });
      const points = [...plan.fixedChains.values()].flat().concat(w.transport_networks.flatMap(n => n.terminals.map(t => plan.shiftTerminal(t))),
        draft.witness.logistics_components.map(c => plan.shiftAnchor(c.transport, c.position)));
      if (points.some(q => q.x < bounds.minX || q.x > bounds.maxX || q.y < bounds.minY || q.y > bounds.maxY)) { record(diagnostic('deflection-bounds', 'Carried geometry leaves the retained extent', origin)); continue; }
      const delta = local.chains.reduce((s, c) => s + distance(plan.shiftEndpoint(c, false), plan.shiftEndpoint(c, true)) - distance(c.cells[0], c.cells.at(-1)), 0);
      proposals.push({ moves: p.moves, facilityRotations: p.facilityRotations, origin, closure: p.group.closure, seed_instances: p.group.seed.instances, boundary: p.group.seed.boundary, deflected_instance: p.instance,
        releaseBridges, void_candidate: p.void_candidate, body_envelope_area: (b.maxX - b.minX + 1) * (b.maxY - b.minY + 1), endpoint_delta: delta, kind: 'boundary-deflection' });
    } catch (e) { if (!e.code) throw e; record({ code: e.code, severity: e.severity, stage: e.stage, message: e.message, entity: e.entity }); }
  }
  proposals.sort((a, b) => a.body_envelope_area - b.body_envelope_area || a.endpoint_delta - b.endpoint_delta || a.origin - b.origin);
  const ranks = new Map(); for (const p of proposals) { p.rank = ranks.get(p.origin) ?? 0; ranks.set(p.origin, p.rank + 1); }
  return { groups: groups.length, boundary_seeds: boundary.seeds.length, rotation_policy: rotationPolicy, voids, pose_budget: poseBudget, poses_examined: examined, pose_budget_exhausted: !exhausted,
    member_budget: maxMembers, step_limit: stepLimit, release_indexes: cache.size, feasible_positions: proposals.length, rejected, diagnostics,
    diagnostics_truncated: Object.values(rejected).reduce((a, b) => a + b, 0) > diagnostics.length,
    candidates: proposals.sort((a, b) => a.rank - b.rank || a.body_envelope_area - b.body_envelope_area || a.endpoint_delta - b.endpoint_delta || a.origin - b.origin).slice(0, limit) };
}
