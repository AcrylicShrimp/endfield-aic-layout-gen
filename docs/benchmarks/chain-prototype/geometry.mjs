import { rotateFacility } from '../constructive-placement-only/model.mjs';
export const vectors = { north: [0, -1], east: [1, 0], south: [0, 1], west: [-1, 0] };
const sides = ['north', 'east', 'south', 'west'];
export const key = p => `${p.x},${p.y}`;
export const turn = (edge, quarters) => sides[(sides.indexOf(edge) + quarters + 4) % 4];
export function placement(node, rotation, x = 0, y = 0) {
  const shape = rotateFacility(node.facility_data, rotation);
  return { instance: node.id, recipe: node.recipe, facility: node.facility, rotation, x, y, width: shape.width, height: shape.height };
}
export function ports(p, node) {
  return rotateFacility(node.facility_data, p.rotation).ports.map(q => ({ ...q,
    position: { x: p.x + q.position.x, y: p.y + q.position.y },
    connection: { x: p.x + q.position.x + vectors[q.edge][0], y: p.y + q.position.y + vectors[q.edge][1] } }));
}
export const body = p => Array.from({ length: p.width * p.height }, (_, i) => ({ x: p.x + i % p.width, y: p.y + Math.floor(i / p.width) }));
export function occupancy(w, layer = null) {
  return new Set([...w.placements.flatMap(body), ...w.transport_networks.filter(n => layer === null || n.transport === layer).flatMap(n => n.cells)].map(key));
}
const centerRank = (p, owner) => ['north', 'south'].includes(p.edge) ? Math.abs(2 * (p.position.x - owner.x) - owner.width + 1)
  : Math.abs(2 * (p.position.y - owner.y) - owner.height + 1);
export const rankedPorts = (ps, owner, count) => [...ps].sort((a, b) => centerRank(a, owner) - centerRank(b, owner)
  || a.position.y - b.position.y || a.position.x - b.position.x || a.id.localeCompare(b.id)).slice(0, count);
export function* proposals(parent, parentNode, sourceNode, edge, family, policy, used) {
  const available = ports(parent, parentNode).filter(p => p.direction === 'input' && p.transport === edge.transport && !used.has(JSON.stringify([parent.instance, p.id])));
  const targets = policy.port_order === 'spatial' ? available.sort((a, b) => a.position.y - b.position.y || a.position.x - b.position.x).slice(0, policy.ports_per_side)
    : rankedPorts(available, parent, policy.ports_per_side);
  for (const gap of policy.gaps) for (const target of targets) {
    const outputSide = turn(target.edge, family === 'straight' ? 2 : family === 'left' ? 1 : family === 'right' ? 3 : 0);
    for (const rotation of [...sourceNode.facility_data.allowed_rotations].sort((a, b) => a - b)) {
      const local = placement(sourceNode, rotation);
      const sources = rankedPorts(ports(local, sourceNode).filter(p => p.direction === 'output' && p.transport === edge.transport && p.edge === outputSide), local, policy.ports_per_side);
      for (const source of sources) {
        if (family === 'fold') {
          // A U-fold aligns the input/output faces and puts bodies beside each other.
          // Try both tangential sides; the router must validate the returning connection.
          for (const side of [-1, 1]) {
            const verticalFace = ['east', 'west'].includes(target.edge);
            const x = verticalFace ? target.connection.x - source.connection.x : side < 0 ? parent.x - local.width - gap : parent.x + parent.width + gap;
            const y = verticalFace ? side < 0 ? parent.y - local.height - gap : parent.y + parent.height + gap : target.connection.y - source.connection.y;
            const pose = placement(sourceNode, rotation, x, y);
            yield { pose, source: ports(pose, sourceNode).find(p => p.id === source.id), target, gap, fold_side: side };
          }
          continue;
        }
        // Body contact at a bend must not move the route endpoint into the consumer.
        const offset = family === 'straight' ? gap - 1 : Math.max(0, gap - 1);
        const [nx, ny] = vectors[target.edge], desired = { x: target.connection.x + nx * offset, y: target.connection.y + ny * offset };
        if (family !== 'straight') {
          if (outputSide === 'south') desired.y = parent.y - gap;
          if (outputSide === 'north') desired.y = parent.y + parent.height + gap - 1;
          if (outputSide === 'east') desired.x = parent.x - gap;
          if (outputSide === 'west') desired.x = parent.x + parent.width + gap - 1;
        }
        const pose = placement(sourceNode, rotation, desired.x - source.connection.x, desired.y - source.connection.y);
        yield { pose, source: ports(pose, sourceNode).find(p => p.id === source.id), target, gap };
      }
    }
  }
}
export function canonical(w) {
  const points = [...w.placements, ...w.transport_networks.flatMap(n => n.cells)];
  let minX = Infinity, minY = Infinity;
  for (const p of points) { minX = Math.min(minX, p.x); minY = Math.min(minY, p.y); }
  const dx = -minX, dy = -minY;
  const move = p => ({ ...p, x: p.x + dx, y: p.y + dy });
  return { ...w, placements: w.placements.map(move), transport_networks: w.transport_networks.map(n => ({ ...n,
    cells: n.cells.map(move), segments: n.segments.map(s => ({ ...s, from: move(s.from), to: move(s.to) })), terminals: n.terminals.map(t => ({ ...t, position: move(t.position) })) })) };
}
export function routeBounds(w, a, b, gap) {
  const points = [...w.placements.flatMap(p => [p, { x: p.x + p.width - 1, y: p.y + p.height - 1 }]), ...w.transport_networks.flatMap(n => n.cells), a, b];
  const pad = Math.max(...w.placements.flatMap(p => [p.width, p.height])) + gap;
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
  for (const p of points) { minX = Math.min(minX, p.x); minY = Math.min(minY, p.y); maxX = Math.max(maxX, p.x); maxY = Math.max(maxY, p.y); }
  return { minX: minX - pad, minY: minY - pad, maxX: maxX + pad, maxY: maxY + pad };
}
