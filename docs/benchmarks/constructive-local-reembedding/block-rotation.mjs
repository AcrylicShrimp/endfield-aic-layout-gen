import { connectedGroups, emptyRectangles } from './groups.mjs';
import { boundaryCohorts, closeGeometry } from './push.mjs';
import { extractChains, extentOf } from './model.mjs';
import { movementPlan, bodyCells } from './movement.mjs';
import { planFacilityRotations } from './facility-rotation.mjs';
import { bridgeTopologyView, releaseBridgeDraft, bridgeReleaseLimit } from './components.mjs';
import { incidentBridgeIds } from './coalescence.mjs';
import { interleave } from './fair.mjs';
import { key, distance, directions } from './path.mjs';
const diagnostic = (code, message, entity = null) => ({ code, message, entity, severity: 'info', stage: 'block-rotation' });
const fail = (code, message, entity) => { throw Object.assign(new Error(message), diagnostic(code, message, entity)); };
const envelope = ps => {
  const x = Math.min(...ps.map(p => p.x)), y = Math.min(...ps.map(p => p.y));
  return { x, y, width: Math.max(...ps.map(p => p.x + p.width)) - x, height: Math.max(...ps.map(p => p.y + p.height)) - y };
};
function mergeMoves(moves) {
  const groups = new Map();
  for (const m of moves) {
    const k = `${m.dx},${m.dy}`; if (!groups.has(k)) groups.set(k, { instances: [], dx: m.dx, dy: m.dy });
    groups.get(k).instances.push(...m.instances);
  }
  return [...groups.values()].map(m => ({ ...m, instances: m.instances.sort() }));
}

export function rigidBlockPose(w, catalogs, instances, turns, origin) {
  if (!Array.isArray(instances) || instances.length < 2 || instances.length > 4 || new Set(instances).size !== instances.length ||
    instances.some(id => !w.placements.some(p => p.instance === id)) || ![1, 2, 3].includes(turns) || !origin || !Number.isSafeInteger(origin.x) || !Number.isSafeInteger(origin.y))
    fail('block-rotation-input', 'Expected two to four known unique facilities, quarter turn 1..3 and integer origin', instances);
  const members = instances.map(id => w.placements.find(p => p.instance === id)), old = envelope(members);
  const placements = members.map(p => {
    const x = p.x - old.x, y = p.y - old.y;
    const [rx, ry, width, height] = turns === 1 ? [old.height - y - p.height, x, p.height, p.width] :
      turns === 2 ? [old.width - x - p.width, old.height - y - p.height, p.width, p.height] : [y, old.width - x - p.width, p.height, p.width];
    const rotation = (p.rotation + turns * 90) % 360, spec = catalogs.facilities.facilities.find(f => f.id === p.facility);
    if (!spec?.allowed_rotations.includes(rotation)) fail('block-rotation-catalog', 'A group member does not allow the proposed catalog rotation', p.instance);
    return { ...p, x: origin.x + rx, y: origin.y + ry, width, height, rotation };
  });
  const moves = mergeMoves(placements.map((p, i) => ({ instances: [p.instance], dx: p.x - members[i].x, dy: p.y - members[i].y })));
  return { moves, facilityRotations: placements.map(p => ({ instance: p.instance, rotation: p.rotation })), placements, envelope: envelope(placements) };
}

export function blockRotationGroups(w, graph, catalogs) {
  const groups = new Map(), adjacent = new Map(w.placements.map(p => [p.instance, new Set()]));
  for (const e of graph.edges.filter(e => e.internal)) { adjacent.get(e.source)?.add(e.target); adjacent.get(e.target)?.add(e.source); }
  const add = (instances, kind) => {
    if (instances.length < 2 || instances.length > 4 || instances.some(id => !adjacent.has(id))) return;
    const members = new Set(instances), queue = [instances[0]], seen = new Set(queue);
    for (let i = 0; i < queue.length; i++) for (const next of adjacent.get(queue[i])) if (members.has(next) && !seen.has(next)) { seen.add(next); queue.push(next); }
    if (seen.size !== members.size) return;
    const ids = [...members].sort(), signature = JSON.stringify(ids); if (!groups.has(signature)) groups.set(signature, { instances: ids, kind });
  };
  for (const g of connectedGroups(graph, catalogs)) add(g.instances, g.kind);
  for (const axis of ['x', 'y']) {
    const cross = axis === 'x' ? 'y' : 'x', size = axis === 'x' ? 'width' : 'height', crossSize = axis === 'x' ? 'height' : 'width', lines = new Map();
    for (const p of w.placements) { const k = `${p[cross]},${p[crossSize]}`; if (!lines.has(k)) lines.set(k, []); lines.get(k).push(p); }
    for (const line of lines.values()) {
      line.sort((a, b) => a[axis] - b[axis]);
      for (let i = 0; i < line.length; i++) for (let j = i + 1; j < Math.min(line.length, i + 4); j++) {
        if (line[j - 1][axis] + line[j - 1][size] !== line[j][axis]) break;
        add(line.slice(i, j + 1).map(p => p.instance), `aligned-${axis}`);
      }
    }
  }
  return [...groups.values()];
}

export function blockRotationCandidates(detail, graph, catalogs, { limit = 128, poseBudget = 16384, junctionPolicy = 'downstream' } = {}) {
  if (!Number.isSafeInteger(limit) || limit < 1 || limit > 512 || !Number.isSafeInteger(poseBudget) || poseBudget < 1 || poseBudget > 65536 || !['internal', 'downstream'].includes(junctionPolicy))
    fail('block-rotation-budget', 'Expected candidate limit 1..512, pose budget 1..65536 and known junction policy');
  const w = detail.witness, bounds = extentOf(w), groups = blockRotationGroups(w, graph, catalogs), voids = emptyRectangles(w), closures = [], closed = new Set();
  const rejected = {}, diagnostics = [], record = d => { rejected[d.code] = (rejected[d.code] ?? 0) + 1; if (diagnostics.length < 64) diagnostics.push(d); };
  const boundary = boundaryCohorts(w); boundary.diagnostics.forEach(record);
  for (const seed of boundary.seeds) for (let step = 1; step <= 4; step++) {
    const [dx, dy] = { minX: [step, 0], maxX: [-step, 0], minY: [0, step], maxY: [0, -step] }[seed.boundary.side];
    const closure = closeGeometry(w, seed.instances, dx, dy);
    if (closure.status !== 'closed') { record(closure.diagnostic); continue; }
    const signature = JSON.stringify([closure.instances, dx, dy]); if (closed.has(signature)) continue;
    closed.add(signature); closures.push({ ...closure, dx, dy, seed });
  }
  function* poses(group, turns) {
    const b = envelope(w.placements.filter(p => group.instances.includes(p.instance))), width = turns % 2 ? b.height : b.width, height = turns % 2 ? b.width : b.height;
    function* fitting() {
      for (const r of voids) if (r.width >= width && r.height >= height)
        for (let y = r.y; y <= r.y + r.height - height; y++) for (let x = r.x; x <= r.x + r.width - width; x++) yield { x, y, void_candidate: true };
    }
    function* nearby() { yield { x: b.x, y: b.y }; for (let step = 1; step <= 4; step++) for (const [dx, dy] of directions) yield { x: b.x + dx * step, y: b.y + dy * step }; }
    const seen = new Set(), compatible = closures.filter(c => c.instances.some(id => group.instances.includes(id)));
    for (const { value: origin } of interleave([fitting(), nearby()])) {
      const k = key(origin); if (seen.has(k)) continue; seen.add(k);
      for (const closure of [null, ...compatible]) yield { group, turns, origin, closure };
    }
  }
  const stream = interleave(groups.flatMap(g => [1, 2, 3].map(turns => poses(g, turns)))), topology = bridgeTopologyView(w, catalogs), indexed = extractChains(topology.witness, topology.released).chains;
  const proposals = [], seen = new Set(), cache = new Map(); let examined = 0, exhausted = false;
  while (examined < poseBudget) {
    const next = stream.next(); if (next.done) { exhausted = true; break; } examined++;
    const { value: p, index: originIndex } = next.value;
    try {
      const rotated = rigidBlockPose(w, catalogs, p.group.instances, p.turns, p.origin), rest = p.closure?.instances.filter(id => !p.group.instances.includes(id)) ?? [];
      const moves = mergeMoves([...rotated.moves, ...(rest.length ? [{ instances: rest, dx: p.closure.dx, dy: p.closure.dy }] : [])]);
      if (moves.length > 4 || moves.flatMap(m => m.instances).length > 16) { record(diagnostic('block-rotation-primitive-budget', 'Combined group transform exceeds the local movement budget', p.group.instances)); continue; }
      const signature = JSON.stringify([moves, rotated.facilityRotations]); if (seen.has(signature)) continue; seen.add(signature);
      const facilityChanges = planFacilityRotations(w, catalogs, moves, rotated.facilityRotations), shifts = new Map(moves.flatMap(m => m.instances.map(id => [id, m])));
      const placements = w.placements.map(q => ({ ...q, ...(facilityChanges.get(q.instance)?.pose ?? { x: q.x + (shifts.get(q.instance)?.dx ?? 0), y: q.y + (shifts.get(q.instance)?.dy ?? 0) }) }));
      const b = envelope(placements);
      if (b.x < bounds.minX || b.y < bounds.minY || b.x + b.width - 1 > bounds.maxX || b.y + b.height - 1 > bounds.maxY) { record(diagnostic('block-rotation-bounds', 'Transformed bodies leave the current extent')); continue; }
      const body = new Set(placements.filter(q => shifts.has(q.instance)).flatMap(bodyCells).map(key));
      if (body.size !== placements.filter(q => shifts.has(q.instance)).reduce((s, q) => s + q.width * q.height, 0) || placements.filter(q => !shifts.has(q.instance)).some(q => bodyCells(q).some(c => body.has(key(c))))) {
        record(diagnostic('local-body-collision', 'Combined rotated and translated bodies overlap')); continue;
      }
      const releaseBridges = [...new Set([...incidentBridgeIds(w, indexed, new Set(shifts.keys())), ...w.logistics_components.filter(c => c.kind === 'bridge' && body.has(key(c.position))).map(c => c.id)])].sort();
      if (releaseBridges.length > bridgeReleaseLimit) { record(diagnostic('block-rotation-bridge-budget', 'Combined pose exceeds the bridge-release budget')); continue; }
      const releaseKey = JSON.stringify(releaseBridges);
      if (!cache.has(releaseKey)) {
        const draft = releaseBridgeDraft(w, catalogs, releaseBridges), local = extractChains(draft.witness, draft.released), rerouteChains = new Set(local.chains.filter(c => c.cells.some(q => draft.released.has(`${c.transport}:${key(q)}`))).map(c => c.id));
        cache.set(releaseKey, { draft, local, rerouteChains });
      }
      const { draft, local, rerouteChains } = cache.get(releaseKey), plan = movementPlan(draft.witness, local.chains, local.anchors, moves, { junctionPolicy, facilityChanges, rerouteChains });
      const points = [...plan.fixedChains.values()].flat().concat(w.transport_networks.flatMap(n => n.terminals.map(t => plan.shiftTerminal(t))), draft.witness.logistics_components.map(c => plan.shiftAnchor(c.transport, c.position)));
      if (points.some(q => q.x < bounds.minX || q.y < bounds.minY || q.x > bounds.maxX || q.y > bounds.maxY)) { record(diagnostic('block-rotation-bounds', 'Transformed endpoints or carried routes leave the current extent')); continue; }
      const delta = local.chains.reduce((s, c) => s + distance(plan.shiftEndpoint(c, false), plan.shiftEndpoint(c, true)) - distance(c.cells[0], c.cells.at(-1)), 0);
      proposals.push({ moves, facilityRotations: rotated.facilityRotations, releaseBridges, group: p.group, turns: p.turns, origin: p.origin, boundary_closure: p.closure,
        origin_index: originIndex, void_candidate: p.origin.void_candidate ?? false, body_envelope_area: b.width * b.height, endpoint_delta: delta, kind: 'rigid-block-rotation' });
    } catch (e) { if (!e.code) throw e; record({ code: e.code, severity: e.severity, stage: e.stage, message: e.message, entity: e.entity }); }
  }
  proposals.sort((a, b) => a.body_envelope_area - b.body_envelope_area || a.endpoint_delta - b.endpoint_delta || a.origin_index - b.origin_index);
  const ranks = new Map(); for (const p of proposals) { const k = JSON.stringify(p.group.instances); p.rank = ranks.get(k) ?? 0; ranks.set(k, p.rank + 1); }
  return { groups: groups.length, boundary_seeds: boundary.seeds.length, closures: closures.length, voids, rotation_policy: 'rigid-block', poses_examined: examined, pose_budget: poseBudget, pose_budget_exhausted: !exhausted,
    feasible_positions: proposals.length, release_indexes: cache.size, rejected, diagnostics, diagnostics_truncated: Object.values(rejected).reduce((a, b) => a + b, 0) > diagnostics.length,
    candidates: proposals.sort((a, b) => a.rank - b.rank || a.body_envelope_area - b.body_envelope_area || a.endpoint_delta - b.endpoint_delta || a.origin_index - b.origin_index).slice(0, limit) };
}
