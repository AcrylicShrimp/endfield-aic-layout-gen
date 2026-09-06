import { key, distance, directions } from './path.mjs';
import { bodyCells } from './movement.mjs';
const fail = (message, entity = null) => { throw Object.assign(new Error(message), { code: 'local-component-pose', stage: 'component-pose', severity: 'info', entity }); };

export function releaseBridgeDraft(w, catalogs, ids = []) {
  const reject = message => { throw Object.assign(new Error(message), { code: 'local-bridge-release', stage: 'bridge-release', severity: 'info' }); };
  if (!Array.isArray(ids) || ids.length > 8 || new Set(ids).size !== ids.length) reject('Expected up to eight unique existing bridge IDs');
  const released = new Set();
  for (const id of ids) {
    const c = w.logistics_components.find(c => c.id === id), def = catalogs.components.components.find(d => d.id === c?.component);
    if (!c || c.kind !== 'bridge' || def?.kind !== 'bridge') reject('Only existing catalog bridges may be released');
    const owners = w.transport_networks.filter(n => n.component_ids.includes(id));
    if (owners.length !== 2) reject('This release family requires two bridge owners');
    for (const n of owners) {
      const ins = n.segments.filter(s => key(s.to) === key(c.position)), outs = n.segments.filter(s => key(s.from) === key(c.position));
      if (n.terminals.some(t => key(t.position) === key(c.position)) || ins.length !== 1 || outs.length !== 1 ||
        ins[0].from.x + outs[0].to.x !== 2 * c.position.x || ins[0].from.y + outs[0].to.y !== 2 * c.position.y ||
        BigInt(ins[0].rate.numerator) * BigInt(outs[0].rate.denominator) !== BigInt(outs[0].rate.numerator) * BigInt(ins[0].rate.denominator))
        reject('A released bridge must have terminal-free straight equal-rate owner paths');
    }
    released.add(`${c.transport}:${key(c.position)}`);
  }
  return { released, witness: { ...w, logistics_components: w.logistics_components.filter(c => !ids.includes(c.id)),
    transport_networks: w.transport_networks.map(n => ({ ...n, component_ids: n.component_ids.filter(id => !ids.includes(id)) })) } };
}

export function planComponentPoses(w, catalogs, poses) {
  if (!Array.isArray(poses) || poses.length > 8 || poses.some(p => !p || typeof p.id !== 'string' || ![p.x, p.y, p.rotation].every(Number.isSafeInteger)) ||
    new Set(poses.map(p => p.id)).size !== poses.length) fail('Expected up to eight unique component poses');
  const changes = new Map();
  for (const p of poses) {
    const c = w.logistics_components.find(c => c.id === p.id), d = catalogs.components.components.find(d => d.id === c?.component);
    if (!c || !d || d.kind !== c.kind || d.transport !== c.transport || d.footprint.width !== 1 || d.footprint.height !== 1 || !d.allowed_rotations.includes(p.rotation))
      fail('Unknown component, unsupported footprint or illegal runtime rotation', p);
    if (w.transport_networks.some(n => n.transport === c.transport && n.terminals.some(t => key(t.position) === key(c.position)))) fail('Component anchor must not own a facility/external terminal', p.id);
    // An explicit original pose pins this junction against implicit group translation.
    changes.set(c.id, { position: { x: p.x, y: p.y }, rotation: p.rotation, turns: ((p.rotation - c.rotation) / 90 + 4) % 4 });
  }
  return changes;
}

export function componentPoseCandidates(w, chains, catalogs, moves, bounds, { limit = 64, affectedOnly = false } = {}) {
  if (!Number.isSafeInteger(limit) || limit < 1 || limit > 512) fail('Expected component candidate limit 1..512');
  const shifts = new Map(moves.flatMap(m => m.instances.map(id => [id, m])));
  const shift = p => { const m = shifts.get(p.instance); return m ? { ...p, x: p.x + m.dx, y: p.y + m.dy } : p; };
  const body = new Set(w.placements.flatMap(p => bodyCells(shift(p))).map(key));
  const terminalPoint = t => { const m = t.endpoint.kind === 'facility' && shifts.get(t.endpoint.instance);
    return m ? { x: t.position.x + m.dx, y: t.position.y + m.dy } : t.position; };
  const terminals = new Set(w.transport_networks.flatMap(n => n.terminals.map(t => `${n.transport}:${key(terminalPoint(t))}`)));
  const result = []; let examined = 0;
  for (const c of w.logistics_components) {
    const def = catalogs.components.components.find(d => d.id === c.component);
    if (!def || def.footprint.width !== 1 || def.footprint.height !== 1) continue;
    const jobs = chains.filter(j => j.transport === c.transport && [j.cells[0], j.cells.at(-1)].some(p => key(p) === key(c.position)));
    if (!jobs.length || (affectedOnly && !jobs.some(j => [...j.startTerminals, ...j.endTerminals].some(t => t.endpoint.kind === 'facility' && shifts.has(t.endpoint.instance))))) continue;
    const endpoints = jobs.map(j => {
      const start = key(j.cells[0]) === key(c.position), ts = start ? j.endTerminals : j.startTerminals;
      const other = ts.length ? terminalPoint(ts[0]) : start ? j.cells.at(-1) : j.cells[0];
      return { other, direction: start ? j.first : (j.last + 2) % 4 };
    });
    const oldDistance = endpoints.reduce((s, e) => s + distance(c.position, e.other), 0), local = [];
    const occupiedAnchors = new Set(w.logistics_components.filter(q => q.id !== c.id && q.transport === c.transport).map(q => key(q.position)));
    const xs = new Set([c.position.x]), ys = new Set([c.position.y]);
    for (const e of endpoints) for (const gap of [-2, -1, 0, 1, 2]) { xs.add(e.other.x + gap); ys.add(e.other.y + gap); }
    for (const x of xs) for (const y of ys) {
      if (x < bounds.minX || y < bounds.minY || x > bounds.maxX || y > bounds.maxY) continue;
      const p = { x, y }, k = key(p); if (body.has(k) || occupiedAnchors.has(k) || terminals.has(`${c.transport}:${k}`)) continue;
      for (const rotation of def.allowed_rotations) {
        examined++; if (x === c.position.x && y === c.position.y && rotation === c.rotation) continue;
        const turns = ((rotation - c.rotation) / 90 + 4) % 4;
        if (endpoints.some(e => { const d = directions[(e.direction + turns) % 4], q = { x: x + d[0], y: y + d[1] };
          return q.x < bounds.minX || q.y < bounds.minY || q.x > bounds.maxX || q.y > bounds.maxY || body.has(key(q)) || occupiedAnchors.has(key(q)); })) continue;
        const proxy = endpoints.reduce((s, e) => { const d = directions[(e.direction + turns) % 4]; return s + 1 + distance({ x: x + d[0], y: y + d[1] }, e.other); }, 0);
        local.push({ componentPoses: [{ id: c.id, x, y, rotation }], kind: c.kind, endpoint_delta: proxy - oldDistance, component: c.id });
      }
    }
    local.sort((a, b) => a.endpoint_delta - b.endpoint_delta || a.componentPoses[0].y - b.componentPoses[0].y || a.componentPoses[0].x - b.componentPoses[0].x || a.componentPoses[0].rotation - b.componentPoses[0].rotation);
    for (const [rank, p] of local.slice(0, 32).entries()) result.push({ ...p, rank });
  }
  return { examined, candidates: result.sort((a, b) => a.rank - b.rank || a.endpoint_delta - b.endpoint_delta || a.component.localeCompare(b.component)).slice(0, limit) };
}
