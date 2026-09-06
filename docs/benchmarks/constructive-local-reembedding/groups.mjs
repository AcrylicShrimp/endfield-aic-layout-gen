import { extractCycles } from '../constructive-cycle-blocks/graph.mjs';
import { extractChains, extentOf } from './model.mjs';
import { movementPlan } from './movement.mjs';
import { distance, key } from './path.mjs';
import { bodyCells } from './movement.mjs';

export function emptyRectangles(w, limit = 8) {
  const bounds = extentOf(w), width = bounds.maxX - bounds.minX + 1, height = bounds.maxY - bounds.minY + 1;
  if (!Number.isSafeInteger(width * height) || width * height > 250000 || !Number.isSafeInteger(limit) || limit < 1 || limit > 64) throw new Error('Empty-rectangle diagnostic budget exceeded');
  const occupied = new Set([...w.placements.flatMap(bodyCells), ...w.transport_networks.flatMap(n => n.cells), ...w.logistics_components.map(c => c.position)].map(key));
  const heights = Array(width).fill(0), rectangles = [];
  for (let y = bounds.minY; y <= bounds.maxY; y++) {
    for (let x = 0; x < width; x++) heights[x] = occupied.has(`${x + bounds.minX},${y}`) ? 0 : heights[x] + 1;
    const stack = [];
    for (let x = 0; x <= width; x++) {
      const h = x === width ? 0 : heights[x]; let left = x;
      while (stack.length && stack.at(-1).height > h) {
        const r = stack.pop(); left = r.left;
        rectangles.push({ x: bounds.minX + r.left, y: y - r.height + 1, width: x - r.left, height: r.height, area: (x - r.left) * r.height });
      }
      if (h && (!stack.length || stack.at(-1).height < h)) stack.push({ left, height: h });
    }
  }
  const result = [];
  for (const r of rectangles.sort((a, b) => b.area - a.area || a.y - b.y || a.x - b.x)) {
    if (result.some(p => r.x >= p.x && r.y >= p.y && r.x + r.width <= p.x + p.width && r.y + r.height <= p.y + p.height)) continue;
    result.push(r); if (result.length === limit) break;
  }
  return result;
}

export function connectedGroups(graph, catalogs) {
  const groups = new Map(), add = (ids, kind) => {
    const sorted = [...new Set(ids)].sort(); if (sorted.length < 2 || sorted.length > 8) return;
    const k = JSON.stringify(sorted); if (!groups.has(k)) groups.set(k, { instances: sorted, kind });
  };
  for (const b of extractCycles(graph, catalogs).blocks) { add(b.members, 'cycle-with-consumer'); add(b.cycle, 'cycle'); }
  const edges = graph.edges.filter(e => e.internal);
  for (const e of edges) add([e.source, e.target], 'pair');
  for (const n of graph.nodes) {
    const incoming = [...new Set(edges.filter(e => e.target === n.id).map(e => e.source))], outgoing = [...new Set(edges.filter(e => e.source === n.id).map(e => e.target))];
    if (incoming.length === 1 && outgoing.length === 1) add([incoming[0], n.id, outgoing[0]], 'serial');
  }
  return [...groups.values()];
}

export function groupCandidates(detail, graph, catalogs, { limit = 64 } = {}) {
  if (!Number.isSafeInteger(limit) || limit < 1 || limit > 512) throw new Error('Expected group candidate limit 1..512');
  const w = detail.witness, { chains, anchors } = extractChains(w), bounds = extentOf(w), candidates = [], diagnostics = [];
  const groups = connectedGroups(graph, catalogs), voids = emptyRectangles(w);
  for (const [ordinal, group] of groups.entries()) {
    const ids = new Set(group.instances), moving = w.placements.filter(p => ids.has(p.instance)), retained = w.placements.filter(p => !ids.has(p.instance));
    const box = ps => ({ minX: Math.min(...ps.map(p => p.x)), minY: Math.min(...ps.map(p => p.y)), maxX: Math.max(...ps.map(p => p.x + p.width - 1)), maxY: Math.max(...ps.map(p => p.y + p.height - 1)) });
    const a = box(moving), b = box(retained), width = a.maxX - a.minX + 1, height = a.maxY - a.minY + 1;
    let seed;
    try { seed = movementPlan(w, chains, anchors, { instances: group.instances, dx: 0, dy: 0 }); }
    catch (e) { diagnostics.push({ group: ordinal, code: e.code, message: e.message }); continue; }
    const xs = new Set([bounds.minX - a.minX, bounds.maxX - a.maxX, 0]), ys = new Set([bounds.minY - a.minY, bounds.maxY - a.maxY, 0]);
    for (const p of retained) {
      for (const x of [p.x - width - 1, p.x + p.width + 1, p.x]) xs.add(x - a.minX);
      for (const y of [p.y - height - 1, p.y + p.height + 1, p.y]) ys.add(y - a.minY);
    }
    const voidShifts = new Set();
    const carriedPoints = [...moving.flatMap(bodyCells), ...seed.fixedChains.values()].flat();
    const hull = { minX: Math.min(...carriedPoints.map(p => p.x)), minY: Math.min(...carriedPoints.map(p => p.y)), maxX: Math.max(...carriedPoints.map(p => p.x)), maxY: Math.max(...carriedPoints.map(p => p.y)) };
    for (const r of voids) if (hull.maxX - hull.minX + 1 <= r.width && hull.maxY - hull.minY + 1 <= r.height)
      for (const dx of [r.x - hull.minX, r.x + r.width - 1 - hull.maxX]) for (const dy of [r.y - hull.minY, r.y + r.height - 1 - hull.maxY]) {
        xs.add(dx); ys.add(dy); voidShifts.add(`${dx},${dy}`);
      }
    const boundary = chains.filter(c => seed.movable(c.transport, c.cells[0]) !== seed.movable(c.transport, c.cells.at(-1)));
    const oldDistance = boundary.reduce((s, c) => s + distance(c.cells[0], c.cells.at(-1)), 0), raw = [];
    for (const dx of xs) for (const dy of ys) {
      if ((!dx && !dy) || a.minX + dx < bounds.minX || a.maxX + dx > bounds.maxX || a.minY + dy < bounds.minY || a.maxY + dy > bounds.maxY) continue;
      if (moving.some(p => retained.some(q => p.x + dx < q.x + q.width && q.x < p.x + dx + p.width && p.y + dy < q.y + q.height && q.y < p.y + dy + p.height))) continue;
      const endpointDelta = boundary.reduce((s, c) => { const shift = p => seed.movable(c.transport, p) ? { x: p.x + dx, y: p.y + dy } : p;
        return s + distance(shift(c.cells[0]), shift(c.cells.at(-1))); }, 0) - oldDistance;
      const envelope = (Math.max(b.maxX, a.maxX + dx) - Math.min(b.minX, a.minX + dx) + 1) * (Math.max(b.maxY, a.maxY + dy) - Math.min(b.minY, a.minY + dy) + 1);
      raw.push({ ...group, labels: group.instances.map(id => detail.labels[id]), dx, dy, endpoint_delta: endpointDelta, body_envelope_area: envelope, group: ordinal, void_candidate: voidShifts.has(`${dx},${dy}`) });
    }
    const valid = [], seen = new Set();
    for (const policy of ['void', 'packing', 'locality']) {
      const sorted = raw.filter(r => policy !== 'void' || r.void_candidate).sort((a, b) => (policy !== 'locality' ? a.body_envelope_area - b.body_envelope_area || a.endpoint_delta - b.endpoint_delta :
        a.endpoint_delta - b.endpoint_delta || a.body_envelope_area - b.body_envelope_area) || a.dy - b.dy || a.dx - b.dx);
      let count = 0;
      for (const move of sorted.slice(0, 128)) {
        const k = `${move.dx},${move.dy}`; if (seen.has(k)) continue;
        try {
          const plan = movementPlan(w, chains, anchors, move);
          // Every carried cell, not only its body, must stay inside this explicit neighborhood.
          if ([...plan.fixedChains.values()].flat().some(p => p.x < bounds.minX || p.x > bounds.maxX || p.y < bounds.minY || p.y > bounds.maxY)) continue;
          if ([...plan.moved].some(tag => { const p = tag.slice(tag.indexOf(':') + 1).split(',').map(Number); return p[0] + move.dx < bounds.minX || p[0] + move.dx > bounds.maxX || p[1] + move.dy < bounds.minY || p[1] + move.dy > bounds.maxY; })) continue;
          seen.add(k); valid.push({ ...move, policy, carried_components: plan.components.length, translated_chains: plan.fixedChains.size }); if (++count === 4) break;
        } catch { /* Rejected geometry is not a routing trial or a proof of infeasibility. */ }
      }
    }
    for (const [rank, move] of valid.entries()) candidates.push({ ...move, rank });
  }
  return { groups: groups.length, voids, diagnostics, candidates: candidates.sort((a, b) => a.rank - b.rank || a.body_envelope_area - b.body_envelope_area ||
    a.endpoint_delta - b.endpoint_delta || a.group - b.group).slice(0, limit) };
}
