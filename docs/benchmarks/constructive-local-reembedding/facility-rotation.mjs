import { rotateFacility } from '../constructive-placement-only/model.mjs';
const directions = { north: [0, -1], east: [1, 0], south: [0, 1], west: [-1, 0] };
const fail = (message, entity = null) => { throw Object.assign(new Error(message), { code: 'local-facility-rotation', severity: 'info', stage: 'local-facility-pose', entity }); };

export function planFacilityRotations(w, catalogs, moves, rotations = []) {
  if (!Array.isArray(rotations) || rotations.length > 16 || rotations.some(r => !r || typeof r.instance !== 'string' || !Number.isSafeInteger(r.rotation)) ||
    new Set(rotations.map(r => r.instance)).size !== rotations.length) fail('Expected up to sixteen unique facility rotations', rotations);
  const changes = new Map(); if (!rotations.length) return changes;
  if (!Array.isArray(moves) || moves.some(m => !m || !Array.isArray(m.instances) || !Number.isSafeInteger(m.dx) || !Number.isSafeInteger(m.dy))) fail('Rotations require explicit valid translation primitives');
  const shifts = new Map(moves.flatMap(m => m.instances.map(id => [id, m])));
  for (const r of rotations) {
    const p = w.placements.find(p => p.instance === r.instance), spec = catalogs.facilities?.facilities?.find(f => f.id === p?.facility), shift = shifts.get(r.instance);
    if (!p || !spec || !shift || ![0, 90, 180, 270].includes(r.rotation) || !spec.allowed_rotations.includes(r.rotation)) fail('Unknown/nonselected facility or unsupported catalog rotation', r);
    const shape = rotateFacility(spec, r.rotation), pose = { x: p.x + shift.dx, y: p.y + shift.dy, width: shape.width, height: shape.height, rotation: r.rotation };
    const ports = shape.ports.map(q => ({ ...q, position: { x: pose.x + q.position.x, y: pose.y + q.position.y },
      connection: { x: pose.x + q.position.x + directions[q.edge][0], y: pose.y + q.position.y + directions[q.edge][1] } }));
    const terminalPositions = new Map();
    for (const n of w.transport_networks) for (const t of n.terminals) if (t.endpoint.kind === 'facility' && t.endpoint.instance === r.instance) {
      const q = ports.find(q => q.id === t.endpoint.port && q.direction === t.direction && q.transport === n.transport);
      if (!q) fail('Rotated facility terminal has no compatible catalog port', t.id);
      terminalPositions.set(t.id, { ...q.connection });
    }
    if (r.rotation !== p.rotation) changes.set(r.instance, { pose, ports, terminalPositions, turns: ((r.rotation - p.rotation) / 90 + 4) % 4 });
  }
  return changes;
}
