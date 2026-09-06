import { planFacilityRotations } from './facility-rotation.mjs';
import { movementPlan, bodyCells } from './movement.mjs';
import { extractChains, extentOf } from './model.mjs';
import { bridgeTopologyView, releaseBridgeDraft, bridgeReleaseLimit } from './components.mjs';
import { incidentBridgeIds } from './coalescence.mjs';
import { interleave } from './fair.mjs';
import { directions, distance, key } from './path.mjs';

export function rotationCandidates(detail, catalogs, { limit = 128, poseBudget = 4096, stepLimit = 2, junctionPolicy = 'downstream' } = {}) {
  const diagnostic = (code, message, entity = null) => ({ code, message, entity, stage: 'facility-rotation-proposals', severity: 'info' });
  if (!Number.isSafeInteger(limit) || limit < 1 || limit > 512 || !Number.isSafeInteger(poseBudget) || poseBudget < 1 || poseBudget > 65536 ||
    !Number.isSafeInteger(stepLimit) || stepLimit < 0 || stepLimit > 8 || !['internal', 'downstream'].includes(junctionPolicy))
    throw Object.assign(new Error('Invalid rotation proposal budget'), diagnostic('rotation-invalid-input', 'Expected candidate limit 1..512, pose budget 1..65536, step limit 0..8 and known junction policy'));
  const w = detail.witness, bounds = extentOf(w), topology = bridgeTopologyView(w, catalogs), indexed = extractChains(topology.witness, topology.released).chains;
  const cache = new Map(), proposals = [], rejected = {}, diagnostics = [];
  const record = d => { rejected[d.code] = (rejected[d.code] ?? 0) + 1; if (diagnostics.length < 64) diagnostics.push(d); };
  function* poses(p) {
    const spec = catalogs.facilities.facilities.find(f => f.id === p.facility);
    const shifts = [{ dx: 0, dy: 0 }, ...Array.from({ length: stepLimit }, (_, i) => directions.map(([dx, dy]) => ({ dx: dx * (i + 1), dy: dy * (i + 1) }))).flat()];
    for (const shift of shifts) for (const rotation of spec.allowed_rotations.filter(r => r !== p.rotation))
      yield { moves: [{ instances: [p.instance], ...shift }], facilityRotations: [{ instance: p.instance, rotation }] };
  }
  const stream = interleave(w.placements.map(poses)); let examined = 0, exhausted = false;
  while (examined < poseBudget) {
    const next = stream.next(); if (next.done) { exhausted = true; break; } examined++;
    const { value: proposal, index: origin } = next.value, instance = proposal.facilityRotations[0].instance;
    try {
      const changes = planFacilityRotations(w, catalogs, proposal.moves, proposal.facilityRotations), pose = changes.get(instance).pose;
      const body = new Set(bodyCells(pose).map(key));
      const releaseBridges = [...new Set([...incidentBridgeIds(w, indexed, new Set([instance])), ...w.logistics_components.filter(c => c.kind === 'bridge' && body.has(key(c.position))).map(c => c.id)])].sort();
      if (releaseBridges.length > bridgeReleaseLimit) { record(diagnostic('rotation-bridge-budget', 'Proposed pose exceeds the bridge-release budget', instance)); continue; }
      const signature = JSON.stringify(releaseBridges);
      if (!cache.has(signature)) {
        const draft = releaseBridgeDraft(w, catalogs, releaseBridges), local = extractChains(draft.witness, draft.released);
        const rerouteChains = new Set(local.chains.filter(c => c.cells.some(p => draft.released.has(`${c.transport}:${key(p)}`))).map(c => c.id));
        cache.set(signature, { draft, local, rerouteChains });
      }
      const { draft, local, rerouteChains } = cache.get(signature), plan = movementPlan(draft.witness, local.chains, local.anchors, proposal.moves, { facilityChanges: changes, junctionPolicy, rerouteChains });
      const placements = w.placements.map(p => ({ ...p, ...plan.shiftFacility(p) }));
      const points = placements.flatMap(p => [p, { x: p.x + p.width - 1, y: p.y + p.height - 1 }]);
      const envelope = (Math.max(...points.map(p => p.x)) - Math.min(...points.map(p => p.x)) + 1) * (Math.max(...points.map(p => p.y)) - Math.min(...points.map(p => p.y)) + 1);
      points.push(...[...plan.fixedChains.values()].flat(), ...w.transport_networks.flatMap(n => n.terminals.map(t => plan.shiftTerminal(t))), ...draft.witness.logistics_components.map(c => plan.shiftAnchor(c.transport, c.position)));
      if (points.some(p => p.x < bounds.minX || p.x > bounds.maxX || p.y < bounds.minY || p.y > bounds.maxY)) { record(diagnostic('rotation-bounds', 'Proposed pose leaves the retained extent', instance)); continue; }
      const delta = local.chains.reduce((s, c) => s + distance(plan.shiftEndpoint(c, false), plan.shiftEndpoint(c, true)) - distance(c.cells[0], c.cells.at(-1)), 0);
      proposals.push({ ...proposal, releaseBridges, origin, body_envelope_area: envelope, endpoint_delta: delta, kind: 'catalog-rotation' });
    } catch (e) { if (!e.code) throw e; record({ code: e.code, message: e.message, entity: e.entity, severity: e.severity, stage: e.stage }); }
  }
  proposals.sort((a, b) => a.body_envelope_area - b.body_envelope_area || a.endpoint_delta - b.endpoint_delta || a.origin - b.origin);
  const ranks = new Map(); for (const p of proposals) { p.rank = ranks.get(p.origin) ?? 0; ranks.set(p.origin, p.rank + 1); }
  return { rotation_policy: 'catalog', pose_budget: poseBudget, poses_examined: examined, pose_budget_exhausted: !exhausted, step_limit: stepLimit,
    release_indexes: cache.size, feasible_positions: proposals.length, rejected, diagnostics, diagnostics_truncated: Object.values(rejected).reduce((a, b) => a + b, 0) > diagnostics.length,
    candidates: proposals.sort((a, b) => a.rank - b.rank || a.body_envelope_area - b.body_envelope_area || a.endpoint_delta - b.endpoint_delta || a.origin - b.origin).slice(0, limit) };
}
