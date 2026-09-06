import { bodyCells } from '../constructive-local-reembedding/movement.mjs';
import { extentOf } from '../constructive-local-reembedding/model.mjs';
import { digest } from './io.mjs';

const sides = ['north', 'east', 'south', 'west'];
export function transform(w, quarters = 0) {
  const rotate = p => { let { x, y } = p; for (let i = 0; i < quarters; i++) [x, y] = [-y, x]; return { ...p, x, y }; };
  const result = { ...w,
    placements: w.placements.map(p => {
      const cells = bodyCells(p).map(rotate), xs = cells.map(c => c.x), ys = cells.map(c => c.y);
      return { ...p, x: Math.min(...xs), y: Math.min(...ys), width: Math.max(...xs) - Math.min(...xs) + 1,
        height: Math.max(...ys) - Math.min(...ys) + 1, rotation: (p.rotation + quarters * 90) % 360 };
    }),
    logistics_components: w.logistics_components.map(c => ({ ...c, position: rotate(c.position), rotation: (c.rotation + quarters * 90) % 360 })),
    transport_networks: w.transport_networks.map(n => ({ ...n, cells: n.cells.map(rotate), segments: n.segments.map(s => ({ ...s, from: rotate(s.from), to: rotate(s.to) })),
      terminals: n.terminals.map(t => ({ ...t, position: rotate(t.position), endpoint: t.endpoint.kind !== 'external' ? t.endpoint
        : { ...t.endpoint, side: sides[(sides.indexOf(t.endpoint.side) + quarters) % 4] } })) })) };
  const bounds = extentOf(result), move = p => ({ ...p, x: p.x - bounds.minX, y: p.y - bounds.minY });
  return { ...result, placements: result.placements.map(move), logistics_components: result.logistics_components.map(c => ({ ...c, position: move(c.position) })),
    transport_networks: result.transport_networks.map(n => ({ ...n, cells: n.cells.map(move), segments: n.segments.map(s => ({ ...s, from: move(s.from), to: move(s.to) })),
      terminals: n.terminals.map(t => ({ ...t, position: move(t.position) })) })) };
}
export function canonicalKey(w, catalogs) {
  const sort = xs => [...xs].sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b)));
  const variants = [];
  for (let q = 0; q < 4; q++) {
    if (w.placements.some(p => !catalogs.facilities.facilities.find(f => f.id === p.facility)?.allowed_rotations.includes((p.rotation + q * 90) % 360))) continue;
    const v = transform(w, q);
    variants.push(JSON.stringify({ ...v, placements: sort(v.placements), utilizations: Object.fromEntries(Object.entries(v.utilizations).sort()), logistics_components: sort(v.logistics_components),
      transport_networks: sort(v.transport_networks.map(n => ({ ...n, requirement_ids: sort(n.requirement_ids), component_ids: sort(n.component_ids), cells: sort(n.cells), segments: sort(n.segments), terminals: sort(n.terminals) }))) }));
  }
  if (!variants.length) throw new Error('No legal canonical orientation');
  return digest(variants.sort()[0]);
}
