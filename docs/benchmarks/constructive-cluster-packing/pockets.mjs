// A necessary space condition for fixed geometry, not a route or placement solver.
const key = p => `${p.x},${p.y}`;
export function probeSpace(start, blocked, bounds, limit = 4096) {
  if (!Number.isSafeInteger(limit) || limit < 1 || !Number.isSafeInteger(start.x) || !Number.isSafeInteger(start.y) ||
      !['minX', 'minY', 'maxX', 'maxY'].every(k => Number.isSafeInteger(bounds[k])) || bounds.minX > bounds.maxX || bounds.minY > bounds.maxY)
    throw Object.assign(new Error('Expected integer flood coordinates, bounds and positive cell budget'), { code: 'nesting-space-schema' });
  const queue = [start], seen = new Set();
  for (let i = 0; i < queue.length; i++) {
    const p = queue[i], k = key(p); if (seen.has(k) || blocked(p.x, p.y)) continue; seen.add(k);
    if (p.x <= bounds.minX || p.x >= bounds.maxX || p.y <= bounds.minY || p.y >= bounds.maxY)
      return { cells: [...seen], count: seen.size, escaped: true, capped: false };
    if (seen.size >= limit) return { cells: [...seen], count: seen.size, escaped: false, capped: true };
    queue.push({ x: p.x - 1, y: p.y }, { x: p.x + 1, y: p.y }, { x: p.x, y: p.y - 1 }, { x: p.x, y: p.y + 1 });
  }
  return { cells: [...seen], count: seen.size, escaped: false, capped: false };
}

export function spaceFailures(requirements, blocked, bounds, budget = 4096) {
  const failures = [], cache = new Map();
  const demands = requirements.flatMap(g => g.demands);
  const limit = demands.some(d => d.escape) ? budget : Math.min(budget, Math.max(1, ...demands.map(d => d.minimum_cells)));
  for (const g of requirements) {
    const spaces = g.ports.map(p => {
      const k = `${g.transport}:${key(p)}`;
      if (!cache.has(k)) {
        const result = probeSpace(p, (x, y) => blocked(x, y, g.transport), bounds, limit);
        // Every visited cell belongs to the same connected region. Capped counts are lower bounds.
        for (const cell of result.cells) cache.set(`${g.transport}:${cell}`, result);
        if (!result.cells.length) cache.set(k, result);
      }
      const { cells, ...size } = cache.get(k); return { port: p.id, ...size };
    });
    const thresholds = g.demands.flatMap(d => Array.from({ length: d.lanes }, () => d)).sort((a, b) =>
      Number(b.escape) - Number(a.escape) || (b.minimum_cells ?? 0) - (a.minimum_cells ?? 0));
    // Threshold-compatible sets are nested; each earlier matched port also qualifies for later ones.
    for (let i = 0; i < thresholds.length; i++) {
      const d = thresholds[i], available = spaces.filter(s => s.escaped || s.capped || (!d.escape && s.count >= d.minimum_cells)).length;
      if (available > i) continue;
      failures.push({ code: 'nesting-output-pocket-too-small', severity: 'info', stage: 'cluster-nesting',
        message: 'Fixed output region lacks enough distinct eligible ports with space for the pending consumer',
        instance: g.instance, transport: g.transport, target: d.target, minimum_cells: d.minimum_cells,
        requires_escape: d.escape, required_ports_at_threshold: i + 1, available, ports: spaces });
      break;
    }
  }
  return failures;
}
