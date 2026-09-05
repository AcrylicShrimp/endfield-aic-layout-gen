// Local independent-lane certificates. No global placement or routing success is inferred.
const gcd = (a, b) => b ? gcd(b, a % b) : a;
const fraction = (n, d) => { const g = gcd(n, d); return [n / g, d / g]; };
const readRate = r => {
  if (![r?.numerator, r?.denominator].every(Number.isSafeInteger) || r.numerator <= 0 || r.denominator <= 0) throw new Error('Invalid positive rational rate');
  return fraction(BigInt(r.numerator), BigInt(r.denominator));
};
const jsonRate = ([n, d]) => {
  if (n > BigInt(Number.MAX_SAFE_INTEGER) || d > BigInt(Number.MAX_SAFE_INTEGER)) throw new Error('Rate exceeds JSON integer range');
  return { numerator: Number(n), denominator: Number(d) };
};
const unique = a => [...new Set(a)];
const fail = (code, message, extra = {}) => ({ status: 'unresolved', code, message, ...extra });
const byName = (a, b) => a.localeCompare(b);
export function inversions(edges, sources, targets) {
  const s = new Map(sources.map((id, i) => [id, i])), t = new Map(targets.map((id, i) => [id, i]));
  const pairs = [];
  for (let i = 0; i < edges.length; i++) for (let j = i + 1; j < edges.length; j++) {
    const a = edges[i], b = edges[j];
    if ((s.get(a.source) - s.get(b.source)) * (t.get(a.target) - t.get(b.target)) < 0) pairs.push([a.id, b.id]);
  }
  return pairs;
}
export function k22(edges) {
  const sources = unique(edges.map(e => e.source)).sort(byName);
  for (let i = 0; i < sources.length; i++) for (let j = i + 1; j < sources.length; j++) {
    const a = sources[i], b = sources[j];
    const common = unique(edges.filter(e => e.source === a).map(e => e.target)).filter(t => edges.some(e => e.source === b && e.target === t)).sort(byName);
    if (common.length >= 2) {
      const targets = common.slice(0, 2), witness = [a, b].flatMap(source => targets.map(target => edges.find(e => e.source === source && e.target === target)));
      return { sources: [a, b], targets, requirements: witness.map(e => e.id), items: unique(witness.map(e => e.item)),
        scope: 'independent links between two opposing ordered faces only; not a commodity-network or global infeasibility proof' };
    }
  }
  return null;
}
function* permutations(values, prefix = []) {
  if (!values.length) { yield prefix; return; }
  for (let i = 0; i < values.length; i++) yield* permutations(values.filter((_, j) => i !== j), [...prefix, values[i]]);
}
function targetOrder(edges, sources, preferred) {
  const rank = new Map(sources.map((id, i) => [id, i])), adjacency = new Map(preferred.map(id => [id, new Set()]));
  for (const a of edges) for (const b of edges) if (rank.get(a.source) < rank.get(b.source) && a.target !== b.target) adjacency.get(a.target).add(b.target);
  const remaining = new Set(preferred), result = [];
  while (remaining.size) {
    const next = preferred.find(id => remaining.has(id) && [...remaining].every(other => !adjacency.get(other).has(id)));
    if (next === undefined) return null;
    result.push(next); remaining.delete(next);
  }
  return result;
}
export function solveOrder(edges, sources, targets, budget = 100000) {
  if (!Number.isSafeInteger(budget) || budget < 1 || budget > 1000000) throw new Error('Source-order budget must be 1..1000000');
  if (new Set(sources).size !== sources.length || new Set(targets).size !== targets.length
    || edges.some(e => !sources.includes(e.source) || !targets.includes(e.target))) throw new Error('Invalid corridor membership');
  const witness = k22(edges);
  if (witness) return fail(witness.items.length === 1 ? 'zero-crossing-explicit-junction-required' : 'zero-crossing-independent-order-obstruction',
    'Fixed-pair K2,2 cannot be ordered as four independent links. Same-item flow reassignment may remove this pattern; otherwise explicit network synthesis or a different embedding is unresolved.', { witness, examined: 0 });
  let examined = 0;
  for (const order of permutations(sources)) {
    if (examined === budget) return fail('zero-crossing-order-budget', 'Source-order search exhausted its explicit bound; result is unknown.', { examined });
    examined++;
    const target = targetOrder(edges, order, targets);
    if (target) return { status: 'zero-inversions', sources: order, targets: target, examined, inversion_pairs: inversions(edges, order, target) };
  }
  return fail('zero-crossing-independent-order-exhausted', 'All source orders exhausted for this fixed two-face independent-link model only.', { examined });
}
function composeOrders(placement, corridors, oracle, capacity) {
  if (!oracle) return { status: 'awaiting-joint-order-oracle', updates: [], candidate: structuredClone(placement) };
  if (!['OPTIMAL', 'FEASIBLE'].includes(oracle.status) || oracle.kind !== 'zero-crossing-local-order-oracle') throw new Error('Cannot compose an uncertified order oracle');
  const members = new Map(placement.placements.map(p => [p.instance, p]));
  const blocks = new Map(placement.block_experiment.blocks.map(b => [b.id, b]));
  const before = new Map([...blocks.keys()].map(id => [id, new Set()])), updates = [];
  const reaches = (a, b) => {
    const pending = [...before.get(a)], seen = new Set();
    while (pending.length) { const id = pending.pop(); if (id === b) return true;
      if (seen.has(id)) continue; seen.add(id); pending.push(...before.get(id)); } return false;
  };
  if (unique(oracle.block_order).length !== blocks.size || oracle.block_order.some(id => !blocks.has(id))) throw new Error('Invalid solver block permutation');
  const rank = new Map(oracle.block_order.map((id, i) => [id, i])), usedPorts = new Set();
  for (const supplied of oracle.corridors) {
    const c = corridors.find(c => c.id === supplied.id); if (!c) throw new Error('Unknown solver corridor');
    const sorter = (a, b) => rank.get(members.get(a).block) - rank.get(members.get(b).block);
    const order = { sources: [...c.original.sources].sort(sorter), targets: [...c.original.targets].sort(sorter) };
    if (inversions(c.edges, order.sources, order.targets).length) throw new Error('Independent checker rejected joint block order');
    const certificate = embedCorridor(placement, c.edges, order, capacity, supplied.lanes);
    if (certificate.status !== 'local-zero-crossing-certificate') throw new Error('Independent geometric checker rejected solver ports: ' + JSON.stringify(certificate));
    for (const lane of certificate.lanes) for (const role of ['source', 'target']) {
      const key = `${lane[role]}:${role}:${lane[role + '_port']}`;
      if (usedPorts.has(key)) throw new Error('Solver reused a physical port across corridors'); usedPorts.add(key);
    }
    c.certificate = certificate; c.joint_order = order;
    for (const side of [order.sources, order.targets]) for (let i = 1; i < side.length; i++) {
      const a = members.get(side[i - 1]).block, b = members.get(side[i]).block;
      before.get(a).add(b);
    }
    updates.push({ corridor: c.id, status: 'ordered', order });
  }
  if ([...before.keys()].some(id => reaches(id, id))) throw new Error('Joint solver order contains a cycle');
  const remaining = new Set(blocks.keys()), positions = new Map();
  while (remaining.size) {
    const ready = [...remaining].filter(id => [...remaining].every(other => !before.get(other).has(id)))
      .sort((a, b) => rank.get(a) - rank.get(b));
    if (!ready.length) throw new Error('Composition retained a cyclic block order');
    const id = ready[0], b = blocks.get(id); let y = 0;
    for (const [prior, at] of positions) if (reaches(prior, id)) y = Math.max(y, at.y + blocks.get(prior).height + Math.max(b.channel, blocks.get(prior).channel));
    let conflict;
    do {
      conflict = [...positions].find(([other, at]) => {
        const q = blocks.get(other), gap = Math.max(b.channel, q.channel);
        return b.x < q.x + q.width + gap && b.x + b.width + gap > q.x && y < at.y + q.height + gap && y + b.height + gap > at.y;
      });
      if (conflict) y = conflict[1].y + blocks.get(conflict[0]).height + Math.max(b.channel, blocks.get(conflict[0]).channel);
    } while (conflict);
    positions.set(id, { x: b.x, y }); remaining.delete(id);
  }
  const placements = placement.placements.map(p => translate(p, 0, positions.get(p.block).y - blocks.get(p.block).y));
  const world = new Map(placements.map(p => [p.instance, p]));
  for (const c of corridors) {
    const s = [...c.original.sources].sort((a, b) => world.get(a).y - world.get(b).y), t = [...c.original.targets].sort((a, b) => world.get(a).y - world.get(b).y);
    const update = updates.find(u => u.corridor === c.id);
    c.composed = { status: update?.status ?? 'unresolved', code: update?.code ?? (c.status === 'unresolved' ? c.order.code ?? c.certificate?.code : undefined),
      sources: s, targets: t, inversion_pairs: inversions(c.edges, s, t) };
    if (c.composed.status === 'ordered' && c.composed.inversion_pairs.length) throw new Error('Composed order lost its zero-inversion invariant');
  }
  return { status: 'joint-sat-order-applied', oracle_scope: oracle.scope, updates, candidate: { ...placement, placements, bounds: { width: placement.bounds.width, height: Math.max(...placements.map(p => p.y + p.height)) },
    placement_decisions: [], placement_ms: null, pipe_relations: placement.pipe_relations.map(e => ({ ...e,
      supplier_north: world.get(e.source).y + world.get(e.source).height + 1 <= world.get(e.target).y })),
    policy: { ...placement.policy, traversal: 'preserved-frozen-index-for-comparison', row_alignment: 'zero-crossing-interface-precedence', routing_allowed: false },
    block_experiment: { ...placement.block_experiment, decisions: [], timings: {}, blocks: [...blocks.values()].map(b => ({ ...b, ...positions.get(b.id) })) },
    diagnostics: [{ code: 'zero-crossing-unaccepted-candidate', stage: 'global-composition', severity: 'warning',
      message: 'Only certified two-face orders are resolved. Remaining topology, cross-corridor geometry and external interfaces still block routing.' }] } };
}
const translate = (p, x, y) => ({ ...p, x: p.x + x, y: p.y + y, catalog_ports: p.catalog_ports.map(q => ({ ...q,
  position: { x: q.position.x + x, y: q.position.y + y }, connection: { x: q.connection.x + x, y: q.connection.y + y } })) });
function laneRates(r, capacity) {
  let [n, d] = readRate(r); const [cn, cd] = capacity, result = [];
  const count = (n * cd + d * cn - 1n) / (d * cn);
  if (count > 1000n) throw new Error('Local corridor lane budget exceeds 1000');
  while (n > 0n) {
    const amount = n * cd < cn * d ? [n, d] : [cn, cd]; result.push(jsonRate(amount));
    [n, d] = fraction(n * amount[1] - amount[0] * d, d * amount[1]);
  }
  return result;
}
function pathThrough(start, end, column) {
  const cells = [], p = { ...start };
  const move = (key, value) => { while (p[key] !== value) { p[key] += Math.sign(value - p[key]); cells.push({ ...p }); } };
  cells.push({ ...p }); move('x', column); move('y', end.y); move('x', end.x); return cells;
}
export function validateGeometry(placements, lanes) {
  const occupied = new Map(), issues = [];
  for (const lane of lanes) {
    if (JSON.stringify(lane.cells[0]) !== JSON.stringify(lane.start) || JSON.stringify(lane.cells.at(-1)) !== JSON.stringify(lane.end)) issues.push('endpoint-mismatch');
    for (let i = 0; i < lane.cells.length; i++) {
      const p = lane.cells[i], key = `${p.x},${p.y}`;
      if (!Number.isSafeInteger(p.x) || !Number.isSafeInteger(p.y)) issues.push('non-grid-cell');
      if (i && Math.abs(p.x - lane.cells[i - 1].x) + Math.abs(p.y - lane.cells[i - 1].y) !== 1) issues.push('non-unit-step');
      if (occupied.has(key)) issues.push('shared-or-crossing-tile'); occupied.set(key, lane.id);
      if (placements.some(f => f.x <= p.x && p.x < f.x + f.width && f.y <= p.y && p.y < f.y + f.height)) issues.push('facility-intrusion');
    }
  }
  return { valid: issues.length === 0, issues: unique(issues), distinct_tiles: occupied.size };
}
export function embedCorridor(placement, edges, order, capacity, assignments = null) {
  const members = new Map(placement.placements.map(p => [p.instance, p]));
  const blocks = new Map(placement.block_experiment.blocks.map(b => [b.id, b]));
  const localBlocks = new Map(), endpoints = [...order.sources, ...order.targets];
  if (unique(endpoints.map(id => members.get(id).block)).length !== endpoints.length)
    return fail('zero-crossing-block-side-interface', 'Multiple corridor endpoints share a rigid block; split or jointly embed it before certification.');
  const lanes = edges.flatMap(e => laneRates(e.rate, capacity).map((rate, i) => ({ id: `${e.id}/lane-${i}`, requirement: e.id, source: e.source, target: e.target, item: e.item, rate })));
  if (lanes.length > 1000) return fail('zero-crossing-lane-budget', 'Corridor exceeds 1000 geometric lanes.');
  const leftWidth = Math.max(...order.sources.map(id => blocks.get(members.get(id).block).width)), rightX = leftWidth + lanes.length + 3;
  const geometry = [];
  for (const [side, ids] of [['source', order.sources], ['target', order.targets]]) {
    let y = 0;
    for (const id of ids) {
      const block = blocks.get(members.get(id).block), x = side === 'source' ? leftWidth - block.width : rightX;
      localBlocks.set(block.id, { id: block.id, x, y, width: block.width, height: block.height, members: block.members });
      geometry.push(...block.members.map(id => translate(members.get(id), x - block.x, y - block.y)));
      y += block.height + 3;
    }
  }
  for (const [side, direction, edge, partners] of [['source', 'output', 'east', order.targets], ['target', 'input', 'west', order.sources]]) {
    for (const id of order[side === 'source' ? 'sources' : 'targets']) {
      const p = geometry.find(p => p.instance === id), b = localBlocks.get(p.block);
      const domain = p.catalog_ports.filter(q => q.transport === 'belt' && q.direction === direction && q.edge === edge
        && q.connection.x === (side === 'source' ? b.x + b.width : b.x - 1)).sort((a, b) => a.connection.y - b.connection.y || byName(a.id, b.id));
      const needed = lanes.filter(l => l[side] === id).sort((a, b) => partners.indexOf(a[side === 'source' ? 'target' : 'source'])
        - partners.indexOf(b[side === 'source' ? 'target' : 'source']) || byName(a.id, b.id));
      if (needed.length > domain.length) return fail('zero-crossing-separate-lane-port-shortage',
        'Independent lanes exceed exposed facing ports. Compatible sharing or block splitting remains unresolved.', { instance: id, needed: needed.length, available: domain.length });
      for (let i = 0; i < needed.length; i++) {
        const provided = assignments?.find(a => a.requirement === needed[i].requirement && a.lane === Number(needed[i].id.split('/lane-').at(-1)));
        const chosen = assignments ? domain.find(p => p.id === provided?.[side + '_port']) : domain[i];
        if (!chosen) return fail('zero-crossing-oracle-port-invalid', 'Missing or invalid explicit oracle port');
        needed[i][side === 'source' ? 'start' : 'end'] = chosen.connection;
        needed[i][`${side}_port`] = chosen.id;
      }
    }
  }
  lanes.sort((a, b) => a.start.y - b.start.y);
  if (lanes.some((l, i) => i && (l.start.y <= lanes[i - 1].start.y || l.end.y <= lanes[i - 1].end.y)))
    return fail('zero-crossing-port-order', 'Assigned port order is not strictly monotone across the corridor.');
  const bends = [...lanes.filter(l => l.end.y < l.start.y), ...lanes.filter(l => l.end.y >= l.start.y).reverse()];
  bends.forEach((lane, i) => { lane.cells = pathThrough(lane.start, lane.end, leftWidth + i + 1); });
  const validation = validateGeometry(geometry, lanes);
  if (!validation.valid) return fail('zero-crossing-geometric-witness-rejected', 'Independent grid certificate rejected the candidate.', { validation });
  for (const e of edges) {
    let sum = [0n, 1n];
    for (const l of lanes.filter(l => l.requirement === e.id)) { const r = readRate(l.rate); sum = fraction(sum[0] * r[1] + r[0] * sum[1], sum[1] * r[1]); }
    if (JSON.stringify(jsonRate(sum)) !== JSON.stringify(jsonRate(readRate(e.rate)))) throw new Error('Lane bundle lost demand');
  }
  return { status: 'local-zero-crossing-certificate', scope: 'isolated corridor geometry only; all lanes independent, no shared tiles or crossings',
    game_validated: false, global_placement: false, placements: geometry, blocks: [...localBlocks.values()], lanes, validation,
    lane_count: lanes.length, width: Math.max(...geometry.map(p => p.x + p.width)), height: Math.max(...geometry.map(p => p.y + p.height)) };
}
export function analyzeZeroCrossing(placement, items, transports, budget = 100000, oracle = null) {
  const start = performance.now();
  if (placement.schema_version !== 1 || !placement.block_experiment || placement.routing_performed
    || items.schema_version !== 1 || transports.schema_version !== 1) throw new Error('Expected unrouted block placement and runtime items/transports v1');
  const kind = new Map(items.items.map(i => [i.id, i.transport]));
  const transport = transports.transports.find(t => t.kind === 'belt');
  const capacity = readRate({ numerator: transport.capacity.quantity * 1000, denominator: transport.capacity.duration_ms });
  const members = new Map(placement.placements.map(p => [p.instance, p]));
  const groups = new Map(), pending = [];
  for (const e of placement.internal_requirements) {
    const a = members.get(e.source), b = members.get(e.target);
    if (!a || !b || e.transport !== kind.get(e.item)) throw new Error(`Invalid endpoint/transport: ${e.id}`);
    readRate(e.rate);
    if (a.block === b.block) { pending.push({ requirement: e.id, code: 'zero-crossing-internal-block-pending', stage: 'global-composition' }); continue; }
    const outputs = a.catalog_ports.filter(p => p.transport === 'belt' && p.direction === 'output' && p.edge === 'east');
    const inputs = b.catalog_ports.filter(p => p.transport === 'belt' && p.direction === 'input' && p.edge === 'west');
    if (e.transport !== 'belt' || !outputs.length || !inputs.length || a.x + a.width >= b.x) {
      pending.push({ requirement: e.id, code: e.transport !== 'belt' ? 'zero-crossing-pipe-corridor-pending' : 'zero-crossing-return-or-side-corridor-pending', stage: 'corridor-topology' }); continue;
    }
    const key = `${a.x + a.width}:${b.x - 1}`;
    if (!groups.has(key)) groups.set(key, []); groups.get(key).push(e);
  }
  const results = [];
  const sortY = ids => unique(ids).sort((a, b) => members.get(a).y - members.get(b).y || byName(a, b));
  for (const [id, edges] of [...groups].sort(([a], [b]) => byName(a, b))) {
    edges.sort((a, b) => byName(a.id, b.id));
    const sources = sortY(edges.map(e => e.source)), targets = sortY(edges.map(e => e.target));
    const order = solveOrder(edges, sources, targets, budget);
    const certificate = order.status === 'zero-inversions' ? embedCorridor(placement, edges, order, capacity) : null;
    const status = certificate?.status === 'local-zero-crossing-certificate' ? 'locally-certified' : 'unresolved';
    results.push({ id, status, original: { sources, targets, inversion_pairs: inversions(edges, sources, targets) }, edges, order, certificate });
    for (const e of edges) pending.push({ requirement: e.id, stage: status === 'locally-certified' ? 'global-composition' : 'corridor-topology',
      code: status === 'locally-certified' ? 'zero-crossing-global-composition-pending' : certificate?.code ?? order.code });
  }
  for (const e of placement.external_requirements) pending.push({ requirement: e.id, code: 'zero-crossing-external-corridor-pending', stage: 'external-interfaces' });
  const composition = composeOrders(placement, results, oracle, capacity);
  composition.candidate.placement_ms = performance.now() - start;
  if (pending.length !== placement.internal_requirements.length + placement.external_requirements.length || unique(pending.map(p => p.requirement)).length !== pending.length)
    throw new Error('Full-graph requirement accounting failed');
  return { schema_version: 1, kind: 'zero-crossing-research-report', status: 'blocked', routing_allowed: false,
    factory_validated: false, physical_factory_routing_performed: false, global_zero_crossing_certified: false,
    gate: { code: 'zero-crossing-global-certificate-missing', message: 'Local corridor certificates cannot be composed by assumption. No full placement DTO or router fallback is emitted.',
      required_internal: placement.internal_requirements.length, required_external: placement.external_requirements.length, unresolved: pending },
    scope: 'independent eastward belt corridors on the frozen interface partition; return paths, pipes, external links and shared-block consistency stay explicit unresolved obligations',
    labels: Object.fromEntries(placement.placements.map(p => [p.instance, p.label])),
    corridors: results, composition, elapsed_ms: performance.now() - start,
    summary: { corridors: results.length, locally_certified: results.filter(r => r.status === 'locally-certified').length,
      local_requirements: results.filter(r => r.status === 'locally-certified').reduce((n, r) => n + r.edges.length, 0),
      local_lanes: results.filter(r => r.status === 'locally-certified').reduce((n, r) => n + r.certificate.lane_count, 0),
      unresolved_corridors: results.filter(r => r.status !== 'locally-certified').length } };
}
