import { rotateFacility } from '../constructive-placement-only/model.mjs';
import { minimizeLinearCost } from './length.mjs';
const key = p => `${p.x},${p.y}`;
const sameRate = (a, b) => BigInt(a.numerator) * BigInt(b.denominator) === BigInt(b.numerator) * BigInt(a.denominator);
const delta = { north: [0, -1], east: [1, 0], south: [0, 1], west: [-1, 0] };
const fail = (code, message, entity = null) => { throw Object.assign(new Error(message), { code, severity: 'error', stage: 'embedding-compaction', entity }); };

// Least feasible solution of lower-bound difference constraints. Original values certify feasibility.
export function solveDifferences(lower, edges, original, relaxationBudget = 2000000) {
  if (original.length !== lower.length || lower.some((v, i) => !Number.isSafeInteger(v) || !Number.isSafeInteger(original[i]) || v > original[i]))
    fail('compaction-original-certificate', 'Original coordinates violate a lower bound');
  const out = Array.from({ length: lower.length }, () => []);
  for (const e of edges) {
    if (!Number.isSafeInteger(e.from) || !Number.isSafeInteger(e.to) || !out[e.from] || !out[e.to] || !Number.isSafeInteger(e.weight) || original[e.to] < original[e.from] + e.weight)
      fail('compaction-original-certificate', 'Original coordinates violate a difference constraint', e);
    out[e.from].push(e);
  }
  const values = [...lower], queue = lower.map((_, i) => i), queued = new Set(queue); let relaxations = 0, examined = 0;
  for (let i = 0; i < queue.length; i++) {
    const u = queue[i]; queued.delete(u);
    for (const e of out[u]) {
      if (++examined > relaxationBudget) fail('compaction-relaxation-budget', 'Difference-constraint work budget exhausted', { examined, relaxationBudget });
      const value = values[u] + e.weight;
      if (values[e.to] >= value) continue;
      values[e.to] = value; relaxations++;
      if (!queued.has(e.to)) { queued.add(e.to); queue.push(e.to); }
    }
  }
  return { values, relaxations, examined };
}

export function compactEmbedding(detail, catalogs, axis = 'x', { pointBudget = 100000, relaxationBudget = 2000000, cellBudget = 400000, tie: cutTie = 'least', layerPolicy = 'coupled' } = {}) {
  if (!['x', 'y'].includes(axis)) fail('compaction-axis', 'Expected x or y axis', axis);
  if (!['coupled', 'independent'].includes(layerPolicy)) fail('compaction-layer-policy', 'Expected coupled or independent layers', layerPolicy);
  const started = performance.now(), cross = axis === 'x' ? 'y' : 'x', w = detail.witness;
  const points = [], lookup = new Map(), equalities = [], critical = new Set();
  const pointKey = (p, transport = 'belt') => layerPolicy === 'independent' ? `${transport}:${key(p)}` : key(p);
  const point = (p, transport = 'belt') => {
    if (![p.x, p.y].every(Number.isSafeInteger)) fail('compaction-coordinate', 'Expected integer point', p);
    const k = pointKey(p, transport); if (lookup.has(k)) return lookup.get(k);
    if (points.length >= pointBudget) fail('compaction-point-budget', 'Geometric point budget exhausted', pointBudget);
    const id = points.length; points.push({ x: p.x, y: p.y, ...(layerPolicy === 'independent' ? { transport } : {}) }); lookup.set(k, id); return id;
  };
  const protect = (p, transport) => { const id = point(p, transport); critical.add(id); return id; };
  const tie = (a, b, offset) => equalities.push({ a, b, offset }); // coordinate(b) = coordinate(a) + offset
  const facilities = new Map(w.placements.map(p => [p.instance, p]));
  for (const p of w.placements) {
    const shape = rotateFacility(catalogs.facilities.facilities.find(f => f.id === p.facility), p.rotation);
    if (shape.width !== p.width || shape.height !== p.height) fail('compaction-facility-shape', 'Retained body does not match catalog', p.instance);
    const anchor = protect(p);
    for (let y = p.y; y < p.y + p.height; y++) for (let x = p.x; x < p.x + p.width; x++)
      for (const tr of layerPolicy === 'independent' ? ['belt', 'pipe'] : ['belt'])
        tie(anchor, protect({ x, y }, tr), (axis === 'x' ? x : y) - p[axis]);
  }
  for (const c of w.logistics_components) protect(c.position, c.transport);
  const externals = [], indexes = w.transport_networks.map(n => {
    const ins = new Map(), outs = new Map();
    for (const p of n.cells) point(p, n.transport);
    for (const s of n.segments) {
      if (Math.abs(s.to.x - s.from.x) + Math.abs(s.to.y - s.from.y) !== 1) fail('compaction-unit-segment', 'Input must contain unit directed segments', n.id);
      for (const [map, p] of [[ins, s.to], [outs, s.from]]) { const k = key(p); if (!map.has(k)) map.set(k, []); map.get(k).push(s); }
      if (s.from[cross] !== s.to[cross]) tie(protect(s.from, n.transport), protect(s.to, n.transport), 0);
    }
    for (const t of n.terminals) {
      const id = protect(t.position, n.transport);
      if (t.endpoint.kind !== 'facility') { externals.push({ position: t.position, transport: n.transport }); continue; }
      const p = facilities.get(t.endpoint.instance);
      if (!p) fail('compaction-terminal-owner', 'Missing terminal facility', t.id);
      const shape = rotateFacility(catalogs.facilities.facilities.find(f => f.id === p.facility), p.rotation);
      const port = shape.ports.find(q => q.id === t.endpoint.port);
      if (!port || t.position.x !== p.x + port.position.x + delta[port.edge][0] || t.position.y !== p.y + port.position.y + delta[port.edge][1])
        fail('compaction-terminal-attachment', 'Terminal is not at its catalog connection', t.id);
      tie(protect(p), id, t.position[axis] - p[axis]);
    }
    for (const p of n.cells) {
      const a = ins.get(key(p)) ?? [], b = outs.get(key(p)) ?? [];
      if (a.length !== 1 || b.length !== 1 || a[0].from[cross] !== p[cross] || b[0].to[cross] !== p[cross] ||
          Math.sign(p[axis] - a[0].from[axis]) !== Math.sign(b[0].to[axis] - p[axis]) || !sameRate(a[0].rate, b[0].rate)) protect(p, n.transport);
    }
    return { ins, outs };
  });
  if (!critical.size) fail('compaction-empty-witness', 'No retained geometry');
  let min = Infinity, max = -Infinity;
  for (const p of points) { min = Math.min(min, p[axis]); max = Math.max(max, p[axis]); }
  const low = points.length, high = low + 1;
  points.push({ [axis]: min, [cross]: null }, { [axis]: max, [cross]: null });
  for (const { position: p, transport } of externals) {
    if (p[axis] === min) tie(low, protect(p, transport), 0);
    if (p[axis] === max) tie(high, protect(p, transport), 0);
  }
  const retained = [...critical, low, high], eq = Array.from({ length: points.length }, () => []);
  for (const { a, b, offset } of equalities) { eq[a].push([b, offset]); eq[b].push([a, -offset]); }
  const groups = [], owner = new Map(), offsets = new Map();
  for (const id of retained) if (!owner.has(id)) {
    const group = groups.length, members = [id]; owner.set(id, group); offsets.set(id, 0);
    for (let i = 0; i < members.length; i++) for (const [to, weight] of eq[members[i]]) {
      const offset = offsets.get(members[i]) + weight;
      if (owner.has(to)) {
        if (owner.get(to) !== group || offsets.get(to) !== offset) fail('compaction-rigid-conflict', 'Inconsistent rigid offsets');
      } else { owner.set(to, group); offsets.set(to, offset); members.push(to); }
    }
    for (const m of members) if (points[m][axis] !== points[id][axis] + offsets.get(m)) fail('compaction-original-certificate', 'Original geometry violates a rigid offset');
    groups.push({ original: points[id][axis] - min, lower: members.reduce((v, m) => Math.max(v, -offsets.get(m)), -Infinity) });
  }
  const edges = new Map();
  const constrain = (a, b, separation) => {
    const from = owner.get(a), to = owner.get(b), weight = offsets.get(a) - offsets.get(b) + separation;
    if (from === to) { if (weight > 0) fail('compaction-order-conflict', 'Rigid object violates row order'); return; }
    const k = `${from}:${to}`, old = edges.get(k);
    if (!old || weight > old.weight) edges.set(k, { from, to, weight });
  };
  const rows = new Map();
  for (const id of critical) {
    const row = layerPolicy === 'independent' ? `${points[id].transport}:${points[id][cross]}` : points[id][cross];
    if (!rows.has(row)) rows.set(row, []); rows.get(row).push(id);
    constrain(low, id, 0); constrain(id, high, 0);
  }
  for (const ids of rows.values()) {
    ids.sort((a, b) => points[a][axis] - points[b][axis]);
    for (let i = 1; i < ids.length; i++) constrain(ids[i - 1], ids[i], 1);
  }
  const modelMs = performance.now() - started, solveStart = performance.now();
  const solution = solveDifferences(groups.map(g => g.lower), [...edges.values()], groups.map(g => g.original), relaxationBudget);
  const extentSolveMs = performance.now() - solveStart, lengthStart = performance.now();
  const chains = w.transport_networks.map((n, i) => {
    const result = [];
    for (const s of n.segments) if (critical.has(lookup.get(pointKey(s.from, n.transport)))) {
      let end = s.to, steps = 0;
      while (!critical.has(lookup.get(pointKey(end, n.transport)))) {
        const next = indexes[i].outs.get(key(end));
        if (next?.length !== 1 || !sameRate(s.rate, next[0].rate) || ++steps > n.cells.length) fail('compaction-straight-chain', 'Invalid suppressed chain', n.id);
        end = next[0].to;
      }
      result.push({ from: s.from, to: end, rate: s.rate });
    }
    return result;
  });
  const span = solution.values[owner.get(high)] + offsets.get(high), upper = groups.map(() => Infinity), costs = groups.map(() => 0), seen = new Set();
  let costOffset = 0, originalCost = 0;
  for (const id of retained) upper[owner.get(id)] = Math.min(upper[owner.get(id)], span - offsets.get(id));
  upper[owner.get(low)] = Math.min(upper[owner.get(low)], -offsets.get(low));
  const tightUpper = solveDifferences(upper.map(x => -x), [...edges.values()].map(e => ({ from: e.to, to: e.from, weight: e.weight })), solution.values.map(x => -x), relaxationBudget).values.map(x => -x);
  for (let i = 0; i < chains.length; i++) for (const c of chains[i]) {
    const from = lookup.get(pointKey(c.from, w.transport_networks[i].transport)), to = lookup.get(pointKey(c.to, w.transport_networks[i].transport)), sign = Math.sign(c.to[axis] - c.from[axis]);
    const physical = `${w.transport_networks[i].transport}:${Math.min(from, to)}:${Math.max(from, to)}`;
    if (!sign || seen.has(physical)) continue; seen.add(physical);
    costs[owner.get(from)] -= sign; costs[owner.get(to)] += sign;
    costOffset += sign * (offsets.get(to) - offsets.get(from));
    originalCost += Math.abs(c.to[axis] - c.from[axis]);
  }
  const lengthLower = solution.values, length = minimizeLinearCost(lengthLower, tightUpper, [...edges.values()], costs, { tie: cutTie });
  solution.values = length.values;
  const lengthMs = performance.now() - lengthStart, solveMs = performance.now() - solveStart, lowerStart = performance.now();
  const move = (p, transport = 'belt') => {
    const id = lookup.get(pointKey(p, transport));
    if (!owner.has(id)) fail('compaction-unretained-event', 'Attempted to translate a suppressed interior', p);
    return { ...p, [axis]: solution.values[owner.get(id)] + offsets.get(id) };
  };
  let materialized = 0;
  const networks = w.transport_networks.map((n, i) => {
    const cells = new Map(n.cells.filter(p => critical.has(lookup.get(pointKey(p, n.transport)))).map(p => { const q = move(p, n.transport); return [key(q), q]; })), segments = [];
    for (const s of chains[i]) {
      const end = s.to;
      const from = move(s.from, n.transport), to = move(end, n.transport), dx = Math.sign(to.x - from.x), dy = Math.sign(to.y - from.y);
      const length = Math.abs(to.x - from.x) + Math.abs(to.y - from.y);
      if ((!dx && !dy) || (dx && dy) || Math.sign(end[axis] - s.from[axis]) !== Math.sign(to[axis] - from[axis]))
        fail('compaction-chain-direction', 'Compaction changed chain direction', n.id);
      if ((materialized += length) > cellBudget) fail('compaction-cell-budget', 'Expanded segment budget exhausted', cellBudget);
      for (let step = 0; step < length; step++) {
        const a = { x: from.x + dx * step, y: from.y + dy * step }, b = { x: a.x + dx, y: a.y + dy };
        cells.set(key(a), a); cells.set(key(b), b); segments.push({ from: a, to: b, rate: { ...s.rate } });
      }
    }
    return { ...n, cells: [...cells.values()], segments, terminals: n.terminals.map(t => ({ ...t, position: move(t.position, n.transport) })) };
  });
  const witness = { ...w, placements: w.placements.map(p => move(p)), logistics_components: w.logistics_components.map(c => ({ ...c, position: move(c.position, c.transport) })), transport_networks: networks };
  const tiles = w => new Set(w.transport_networks.flatMap(n => n.cells.map(p => `${n.transport}:${key(p)}`))).size;
  const tileDelta = tiles(w) - tiles(witness), predictedDelta = originalCost - (length.primal + costOffset);
  if (tileDelta !== predictedDelta) fail('compaction-length-certificate', 'Chain cost does not match actual physical tile change', { tileDelta, predictedDelta });
  const ports = Object.create(null);
  for (const p of witness.placements) {
    const shape = rotateFacility(catalogs.facilities.facilities.find(f => f.id === p.facility), p.rotation);
    ports[p.instance] = shape.ports.map(q => ({ ...q, position: { x: p.x + q.position.x, y: p.y + q.position.y },
      connection: { x: p.x + q.position.x + delta[q.edge][0], y: p.y + q.position.y + delta[q.edge][1] } }));
  }
  const certificate = { axis, layer_policy: layerPolicy, points: points.length - 2, retained_points: critical.size, groups: groups.length, constraints: edges.size,
    relaxations: solution.relaxations, examined: solution.examined, original_extent: max - min + 1,
    resulting_extent: solution.values[owner.get(high)] + offsets.get(high) + 1,
    scope: `minimum extent then physical tiles for fixed perpendicular geometry, ${layerPolicy === 'coupled' ? 'joint-layer row order and coincidences' : 'independent-layer row order'}, and chain topology`,
    length: { ...length, costs, lower: lengthLower, upper: tightUpper, cost_offset: costOffset, original_chain_cost: originalCost, resulting_chain_cost: length.primal + costOffset,
      actual_tile_delta: tileDelta, predicted_tile_delta: predictedDelta },
    coordinates: retained.map(id => ({ original: points[id], group: owner.get(id), offset: offsets.get(id) })),
    lower: groups.map(g => g.lower), original: groups.map(g => g.original), edges: [...edges.values()], values: solution.values };
  // Geometry plans refer to the old coordinates; do not expose them as current placement decisions.
  const { witness: oldWitness, catalog_ports, accepted, factory_validated, compaction_certificate, compaction_timings, pre_embedding_compaction, ...metadata } = detail;
  const candidate = structuredClone({ schema_version: detail.schema_version, case: detail.case, embedding: `${detail.embedding}-fixed-${axis}`,
    labels: detail.labels, witness, catalog_ports: ports, port_groups: { sources: [], targets: [] },
    accepted: false, factory_validated: false, pre_embedding_compaction: pre_embedding_compaction ?? metadata, compaction_certificate: certificate });
  candidate.compaction_timings = { model_ms: modelMs, solve_ms: solveMs, extent_solve_ms: extentSolveMs, length_ms: lengthMs,
    lowering_and_copy_ms: performance.now() - lowerStart, total_ms: performance.now() - started };
  return candidate;
}
