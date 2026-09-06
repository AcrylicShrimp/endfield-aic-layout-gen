const fail = (code, message) => { throw Object.assign(new Error(message), { code, stage: 'retained-order', severity: 'error' }); };

// An equality-only ablation, not a placement or routing candidate. Bodies couple
// both layers; an incidental belt/pipe overlap need not couple their coordinates.
export function rigidFacilityGroups(w, axis, retainCoincidences = true, pointBudget = 200000) {
  if (!['x', 'y'].includes(axis) || typeof retainCoincidences !== 'boolean' || !Number.isSafeInteger(pointBudget) || pointBudget < 1)
    fail('order-rigidity-input', 'Invalid axis, coincidence policy or point budget');
  const cross = axis === 'x' ? 'y' : 'x', parents = new Map(), positions = new Map();
  const point = (p, tr) => {
    const xy = `${p.x},${p.y}`, k = `${tr}:${xy}`;
    if (!parents.has(k)) {
      if (parents.size >= pointBudget) fail('order-point-budget', 'Rigidity point budget exhausted');
      parents.set(k, k); if (!positions.has(xy)) positions.set(xy, []); positions.get(xy).push(k);
    }
    return k;
  };
  const root = k => { let r = k; while (parents.get(r) !== r) r = parents.get(r);
    while (parents.get(k) !== k) { const next = parents.get(k); parents.set(k, r); k = next; } return r; };
  const tie = (a, b) => { const ra = root(a), rb = root(b); if (ra !== rb) parents.set(rb, ra); };
  const facilities = new Map();
  for (const p of w.placements) {
    const anchor = point(p, 'belt'); facilities.set(p.instance, anchor);
    for (let x = p.x; x < p.x + p.width; x++) for (let y = p.y; y < p.y + p.height; y++)
      for (const tr of ['belt', 'pipe']) tie(anchor, point({ x, y }, tr));
  }
  for (const n of w.transport_networks) {
    for (const p of n.cells) point(p, n.transport);
    for (const s of n.segments) if (s.from[cross] !== s.to[cross]) tie(point(s.from, n.transport), point(s.to, n.transport));
    for (const t of n.terminals) if (t.endpoint.kind === 'facility') {
      const anchor = facilities.get(t.endpoint.instance); if (!anchor) fail('order-terminal-owner', 'Unknown facility terminal owner');
      tie(anchor, point(t.position, n.transport));
    }
  }
  if (retainCoincidences) for (const keys of positions.values()) for (const k of keys.slice(1)) tie(keys[0], k);
  const groups = new Map();
  for (const [id, k] of facilities) { const r = root(k); if (!groups.has(r)) groups.set(r, []); groups.get(r).push(id); }
  return [...groups.values()].map(g => g.sort()).sort((a, b) => b.length - a.length || a[0].localeCompare(b[0]));
}

// Relative bounds use no absolute coordinate lower bounds. Every reported bound is
// the sum of an actual directed path; it applies only to this retained-order model.
export function relativeBounds(certificate, source, budget = 2000000) {
  const { original, edges } = certificate, count = original.length;
  if (!Number.isSafeInteger(source) || source < 0 || source >= count || !Number.isSafeInteger(budget) || budget < 1 ||
    original.some(v => !Number.isSafeInteger(v))) fail('order-input', 'Invalid source, budget or original coordinates');
  const out = Array.from({ length: count }, () => []);
  for (const e of edges) {
    if (!Number.isSafeInteger(e.from) || !Number.isSafeInteger(e.to) || !out[e.from] || !out[e.to] || !Number.isSafeInteger(e.weight) ||
      original[e.to] < original[e.from] + e.weight) fail('order-certificate', 'Original coordinates do not certify an inequality');
    out[e.from].push(e);
  }
  const values = Array(count).fill(-Infinity), queue = [source], queued = new Set(queue); values[source] = 0;
  let examined = 0;
  for (let i = 0; i < queue.length; i++) {
    const from = queue[i]; queued.delete(from);
    for (const e of out[from]) {
      if (++examined > budget) fail('order-work-budget', 'Relative-bound work budget exhausted');
      const v = values[from] + e.weight;
      if (values[e.to] >= v) continue;
      values[e.to] = v; if (!queued.has(e.to)) { queued.add(e.to); queue.push(e.to); }
    }
  }
  // BFS on tight edges avoids cyclic predecessor chains in zero-weight cycles.
  const predecessors = Array(count).fill(null), bfs = [source], seen = new Set(bfs);
  for (let i = 0; i < bfs.length; i++) for (const e of out[bfs[i]]) {
    if (++examined > budget) fail('order-work-budget', 'Tight-path work budget exhausted');
    if (!seen.has(e.to) && values[e.to] === values[e.from] + e.weight) {
      predecessors[e.to] = e; seen.add(e.to); bfs.push(e.to);
    }
  }
  for (let i = 0; i < count; i++) if (Number.isFinite(values[i]) && !seen.has(i)) fail('order-trace', 'Reachable bound has no tight path');
  const pathTo = target => {
    if (!Number.isSafeInteger(target) || target < 0 || target >= count) fail('order-input', 'Invalid path target');
    if (!seen.has(target)) return null;
    const path = []; for (let at = target; at !== source; at = predecessors[at].from) path.push(predecessors[at]);
    return path.reverse();
  };
  return { values, pathTo, examined };
}

export function explainRectangle(certificate, rectangle, { pairLimit = 4096, workBudget = 2000000 } = {}) {
  if (!Number.isSafeInteger(pairLimit) || pairLimit < 1 || pairLimit > 4096 ||
    !['x', 'y'].includes(certificate.axis) || !['x', 'y', 'width', 'height'].every(k => Number.isSafeInteger(rectangle[k])) ||
    rectangle.width < 1 || rectangle.height < 1) fail('order-rectangle', 'Invalid rectangle, axis or pair limit');
  const axis = certificate.axis, cross = axis === 'x' ? 'y' : 'x', size = axis === 'x' ? 'width' : 'height', other = axis === 'x' ? 'height' : 'width';
  const points = certificate.coordinates.filter(p => p.original[cross] !== null && p.original[cross] >= rectangle[cross] && p.original[cross] < rectangle[cross] + rectangle[other]);
  const sources = points.filter(p => p.original[axis] === rectangle[axis] - 1), targets = points.filter(p => p.original[axis] === rectangle[axis] + rectangle[size]);
  const cache = new Map(), pairs = [];
  let examined = 0, considered = 0, truncated = false;
  outer: for (const a of sources) for (const b of targets) {
    if (++considered > pairLimit) { truncated = true; break outer; }
    if (!cache.has(a.group)) {
      const r = relativeBounds(certificate, a.group, workBudget - examined); examined += r.examined; cache.set(a.group, r);
    }
    const r = cache.get(a.group), bound = r.values[b.group];
    const required = Number.isFinite(bound) ? bound + b.offset - a.offset : null, actual = b.original[axis] - a.original[axis];
    if (required !== null && required > actual) fail('order-separation', 'Derived separation exceeds original geometry');
    pairs.push({ source: a, target: b, actual_separation: actual, required_separation: required,
      locked: required === actual, rigid: a.group === b.group, path: r.pathTo(b.group) });
  }
  pairs.sort((a, b) => Number(b.locked) - Number(a.locked) || (b.required_separation ?? -Infinity) - (a.required_separation ?? -Infinity) ||
    (a.path?.length ?? Infinity) - (b.path?.length ?? Infinity));
  return { axis, rectangle, boundary_source_points: sources.length, boundary_target_points: targets.length, considered: pairs.length,
    truncated, examined, strongest: pairs[0] ?? null,
    scope: 'Required separation only under retained perpendicular geometry, order, coincidences and chain topology; not game necessity.' };
}
