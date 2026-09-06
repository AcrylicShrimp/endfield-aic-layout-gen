import { body, key, occupancy, placement } from './geometry.mjs';
import { compareTuple } from './replay-score.mjs';

const include = (b, p) => ({ minX: Math.min(b.minX, p.x), minY: Math.min(b.minY, p.y), maxX: Math.max(b.maxX, p.x + p.width - 1), maxY: Math.max(b.maxY, p.y + p.height - 1) });
const size = b => ({ width: b.maxX - b.minX + 1, height: b.maxY - b.minY + 1 });
const area = b => { const s = size(b); return s.width * s.height; };

// Greedy body-only accommodation on the frozen routed mask. Never returned to the constructor.
export function remainingFit(graph, w, work) {
  const occupied = occupancy(w), present = new Set(w.placements.map(p => p.instance)), temporary = [];
  let bounds = { minX: Infinity, minY: Infinity, maxX: -Infinity, maxY: -Infinity }, checks = 0;
  for (const c of occupied) { const [x, y] = c.split(',').map(Number); bounds = include(bounds, { x, y, width: 1, height: 1 }); }
  const missing = [...graph.nodes.values()].filter(n => !present.has(n.id)).map(node => {
    const shapes = new Map(node.facility_data.allowed_rotations.map(r => { const p = placement(node, r); return [`${p.width},${p.height}`, p]; }));
    return { node, shapes: [...shapes.values()] };
  }).sort((a, b) => b.shapes[0].width * b.shapes[0].height - a.shapes[0].width * a.shapes[0].height || a.node.recipe.localeCompare(b.node.recipe) || a.node.id.localeCompare(b.node.id));
  let stopped = false;
  for (const { shapes } of missing) {
    let best = null;
    const test = p => {
      if (!work.take('fit_checks')) { stopped = true; return false; }
      checks++;
      if (body(p).some(c => occupied.has(key(c)))) return true;
      const next = include(bounds, p), s = size(next), tuple = [area(next), Math.max(s.width, s.height), p.y, p.x, p.rotation];
      if (!best || compareTuple(tuple, best.tuple) < 0) best = { pose: p, bounds: next, tuple };
      return true;
    };
    // Always test outward extensions in all four directions; none is a final canvas ceiling.
    for (const shape of shapes) for (const [x, y] of [[bounds.minX - shape.width, bounds.minY], [bounds.maxX + 1, bounds.minY],
      [bounds.minX, bounds.minY - shape.height], [bounds.minX, bounds.maxY + 1]]) {
      if (!test({ ...shape, x, y })) break;
    }
    // Scan actual holes. A first in-envelope fit is sufficient for this deliberately greedy step.
    search: for (const shape of shapes) for (let y = bounds.minY; y + shape.height - 1 <= bounds.maxY; y++) for (let x = bounds.minX; x + shape.width - 1 <= bounds.maxX; x++) {
      if (stopped || !test({ ...shape, x, y })) break search;
      if (best.tuple[0] === area(bounds)) break search;
    }
    if (stopped || !best) break;
    temporary.push(best.pose); bounds = best.bounds; for (const c of body(best.pose)) occupied.add(key(c));
  }
  const complete = temporary.length === missing.length;
  return { schema_version: 1, scope: 'Greedy body-only estimate; no routes, no factory validity or minimum-area proof',
    status: complete ? 'body-estimate-complete' : 'fit-work-exhausted', complete, checks, missing: missing.length, accommodated: temporary.length,
    bounds, estimated_area: complete ? area(bounds) : null, temporary_bodies: temporary, factory_validated: false };
}
