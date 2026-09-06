import { rotateFacility } from '../constructive-placement-only/model.mjs';
import { allocateBuckets } from '../constructive-auto-interface/allocation.mjs';
import { orderedApproach } from '../constructive-auto-interface/model.mjs';
import { planPortInterface } from '../constructive-auto-interface/embedding.mjs';
import { rate, add, multiply, parseRate, dto, capacity } from '../constructive-interconnect/rates.mjs';
import { polyline } from '../constructive-interconnect/geometry.mjs';
const key = p => `${p.x},${p.y}`, pair = p => [p.x, p.y];
const delta = { north: [0, -1], east: [1, 0], south: [0, 1], west: [-1, 0] };
const fail = (code, message, entity = null) => { throw Object.assign(new Error(message), { code, severity: 'error', stage: 'block-composition', entity }); };
const sum = xs => xs.reduce(add, rate(0));
const median = xs => [...xs].sort((a, b) => a - b)[Math.floor((xs.length - 1) / 2)];

function bounds(w) {
  let x = Infinity, y = Infinity, right = -Infinity, bottom = -Infinity;
  const include = p => { x = Math.min(x, p.x); y = Math.min(y, p.y); right = Math.max(right, p.x); bottom = Math.max(bottom, p.y); };
  for (const p of w.placements) { include(p); include({ x: p.x + p.width - 1, y: p.y + p.height - 1 }); }
  for (const n of w.transport_networks) for (const p of n.cells) include(p);
  for (const c of w.logistics_components) include(c.position);
  if (!Number.isFinite(x) || !Number.isFinite(y)) fail('composition-empty-region', 'Cannot place an empty region');
  return { x, y, width: right - x + 1, height: bottom - y + 1 };
}

function stack(regions, order, catalogs) {
  const sizes = new Map(regions.map(r => [r.id, bounds(r.detail.witness)])), width = Math.max(...[...sizes.values()].map(b => b.width));
  let cursor = 0;
  const offsets = order.map(id => { const b = sizes.get(id), offset = { region: id, x: width - b.width - b.x, y: cursor - b.y }; cursor += b.height + 1; return offset; });
  return { ...placeRegions(regions, offsets, catalogs), width };
}

// Translate concrete regions under an explicit placement-level packing decision.
export function placeRegions(regions, offsets, catalogs) {
  if (new Set(regions.map(r => r.id)).size !== regions.length || offsets.length !== regions.length ||
      new Set(offsets.map(o => o.region)).size !== regions.length || offsets.some(o => !regions.some(r => r.id === o.region) || !Number.isSafeInteger(o.x) || !Number.isSafeInteger(o.y)))
    fail('composition-region-placement-schema', 'Expected one integer translation per unique retained region');
  const witness = { schema_version: 1, placements: [], utilizations: Object.create(null), transport_networks: [], logistics_components: [] };
  const translations = [], ports = Object.create(null), rowRight = new Map(), used = new Set(), networks = new Map();
  const occupy = (x, y) => rowRight.set(y, Math.max(rowRight.get(y) ?? -Infinity, x));
  for (const { region: id, x: dx, y: dy } of offsets) {
    const region = regions.find(r => r.id === id); translations.push({ region: id, x: dx, y: dy });
    const move = p => ({ ...p, x: p.x + dx, y: p.y + dy }), w = region.detail.witness;
    // Reindex this concrete region once; never recursively encode prior namespace strings.
    const componentIds = new Map(w.logistics_components.map((c, i) => [c.id, JSON.stringify(['region', id, 'component', i])])), requirementIds = new Map();
    const requirementId = old => {
      if (!requirementIds.has(old)) requirementIds.set(old, JSON.stringify(['region', id, 'requirement', requirementIds.size]));
      return requirementIds.get(old);
    };
    const componentId = old => {
      if (!componentIds.has(old)) fail('composition-component-reference', 'Retained network references an absent component', old);
      return componentIds.get(old);
    };
    for (const p of w.placements) {
      if (Object.hasOwn(witness.utilizations, p.instance)) fail('composition-duplicate-owner', 'Facility belongs to more than one region', p.instance);
      const q = move(p); witness.placements.push(q); witness.utilizations[p.instance] = structuredClone(w.utilizations[p.instance]);
      const shape = rotateFacility(catalogs.facilities.facilities.find(f => f.id === p.facility), p.rotation);
      if (shape.width !== p.width || shape.height !== p.height) fail('composition-region-footprint', 'Retained footprint does not match runtime catalog rotation', p.instance);
      ports[p.instance] = shape.ports.map(t => ({ ...t, position: { x: q.x + t.position.x, y: q.y + t.position.y },
        connection: { x: q.x + t.position.x + delta[t.edge][0], y: q.y + t.position.y + delta[t.edge][1] } }));
      for (let y = q.y; y < q.y + q.height; y++) occupy(q.x + q.width - 1, y);
    }
    witness.logistics_components.push(...w.logistics_components.map(c => ({ ...c, id: componentId(c.id), position: move(c.position) })));
    for (const c of w.logistics_components) if (c.transport === 'belt') { const p = move(c.position); occupy(p.x, p.y); }
    for (const n of w.transport_networks) {
      if (!networks.has(n.item)) networks.set(n.item, { id: `network:${n.item}`, item: n.item, transport: n.transport,
        requirement_ids: [], cells: [], segments: [], terminals: [], component_ids: [] });
      const m = networks.get(n.item);
      if (m.transport !== n.transport || catalogs.items.items.find(i => i.id === n.item)?.transport !== n.transport)
        fail('composition-network-transport', 'Retained commodity transport must match the runtime catalog', n.item);
      m.requirement_ids.push(...n.requirement_ids.map(requirementId)); m.cells.push(...n.cells.map(move));
      m.segments.push(...n.segments.map(s => ({ ...s, rate: { ...s.rate }, from: move(s.from), to: move(s.to) })));
      m.terminals.push(...n.terminals.map(t => ({ ...t, rate: { ...t.rate }, endpoint: { ...t.endpoint }, position: move(t.position) })));
      m.component_ids.push(...n.component_ids.map(componentId));
      for (const t of n.terminals) {
        if (t.endpoint.kind !== 'facility') fail('composition-external-in-region', 'Input regions must retain true pending cuts instead of external terminals', id);
        used.add(JSON.stringify([t.endpoint.instance, t.endpoint.port]));
      }
      if (n.transport === 'belt') for (const p of n.cells) { const q = move(p); occupy(q.x, q.y); }
    }
  }
  witness.transport_networks = [...networks.values()];
  return { witness, ports, rowRight, used, translations };
}

export function readyConsumers(graph, members, catalogs) {
  const set = new Set(members);
  return graph.nodes.filter(n => !set.has(n.id) && graph.edges.some(e => e.target === n.id) &&
    graph.edges.filter(e => e.target === n.id).every(e => e.internal && set.has(e.source) && e.transport === 'belt') &&
    [...catalogs.recipes.recipes.find(r => r.id === n.recipe).inputs, ...catalogs.recipes.recipes.find(r => r.id === n.recipe).outputs]
      .every(a => catalogs.items.items.find(i => i.id === a.item)?.transport === 'belt')).map(n => n.id);
}

export function composeBlocks(catalogs, graph, regions, targetIds, { orderBudget = 128, lanePolicy = 'source', regionOffsets = null } = {}) {
  if (!['packed', 'source', 'portfolio'].includes(lanePolicy)) fail('composition-lane-policy', 'Expected packed, source or portfolio lane rows', lanePolicy);
  const start = performance.now(), members = new Set(regions.flatMap(r => r.detail.witness.placements.map(p => p.instance)));
  if (!regions.length || new Set(regions.map(r => r.id)).size !== regions.length || !targetIds.length || new Set(targetIds).size !== targetIds.length ||
      targetIds.some(id => !readyConsumers(graph, members, catalogs).includes(id))) fail('composition-frontier-not-ready', 'Expected unique closed-input belt consumers of the retained regions', targetIds);
  if (!Number.isSafeInteger(orderBudget) || orderBudget < 1 || orderBudget > 512) fail('composition-order-budget', 'Diagnostic order budget must be 1..512');
  const incoming = graph.edges.filter(e => targetIds.includes(e.target));
  const producerIds = [...new Set(incoming.map(e => e.source))], items = [...new Set(incoming.map(e => e.item))].sort();
  const cap = capacity(catalogs.transports.transports.find(t => t.kind === 'belt').capacity);
  const fixed = regionOffsets === null ? null : placeRegions(regions, regionOffsets, catalogs);
  if (fixed) {
    const b = bounds(fixed.witness);
    if (b.x !== 0 || b.y !== 0) fail('composition-offset-origin', 'Explicit packing must normalize used geometry to zero');
    fixed.width = b.width;
  }
  const stackCache = new Map(), cacheMetrics = { hits: 0, builds: 0 }, profile = { stack_ms: 0, node_setup_ms: 0, allocation_ms: 0 };
  const evaluate = (order, targetOrder) => {
    const stackStart = performance.now(), cacheKey = fixed ? 'explicit-packing' : JSON.stringify(order);
    if (!stackCache.has(cacheKey)) {
      stackCache.set(cacheKey, fixed ?? stack(regions, order, catalogs)); cacheMetrics.builds++;
      if (stackCache.size > 8) stackCache.delete(stackCache.keys().next().value);
    } else cacheMetrics.hits++;
    profile.stack_ms += performance.now() - stackStart;
    const nodeStart = performance.now();
    const base = stackCache.get(cacheKey), nodes = new Map(), sx = base.width;
    for (const id of producerIds) {
      const free = base.ports[id].filter(p => p.direction === 'output' && p.transport === 'belt' && p.edge === 'east' && !base.used.has(JSON.stringify([id, p.id])) &&
        (base.rowRight.get(p.connection.y) ?? -Infinity) < p.connection.x).sort((a, b) => a.position.y - b.position.y || a.id.localeCompare(b.id));
      nodes.set(id, { input: [], output: free, inputs: [], outputs: items.flatMap(item => {
        const value = sum(incoming.filter(e => e.source === id && e.item === item).map(e => parseRate(e.rate)));
        return value.n ? [{ item, value }] : [];
      }) });
    }
    for (const id of targetIds) {
      const n = graph.nodes.find(n => n.id === id), recipe = catalogs.recipes.recipes.find(r => r.id === n.recipe);
      const facility = catalogs.facilities.facilities.find(f => f.id === recipe.facility), shape = facility.allowed_rotations.map(r => rotateFacility(facility, r)).find(s =>
        ['input', 'output'].every(d => s.ports.some(p => p.transport === 'belt' && p.direction === d && p.edge === (d === 'input' ? 'west' : 'east'))));
      if (!shape) fail('composition-orientation-unsupported', 'No east-flow catalog orientation', id);
      nodes.set(id, { ...n, recipe, shape, output: [], input: shape.ports.filter(p => p.direction === 'input' && p.transport === 'belt' && p.edge === 'west').sort((a, b) => a.position.y - b.position.y),
        outputs: [], inputs: recipe.inputs.map(a => ({ item: a.item, value: multiply(rate(BigInt(a.quantity) * 1000n, recipe.duration_ms), parseRate(n.utilization)) })) });
    }
    const producers = [...producerIds].sort((a, b) => (nodes.get(a).output[0]?.connection.y ?? Infinity) - (nodes.get(b).output[0]?.connection.y ?? Infinity) || a.localeCompare(b));
    profile.node_setup_ms += performance.now() - nodeStart;
    const allocationStart = performance.now();
    const plan = allocateBuckets(nodes, items, producers, targetOrder, cap);
    profile.allocation_ms += performance.now() - allocationStart;
    return { base, nodes, plan, order, targetOrder, sx };
  };
  let trials = 1, best = evaluate(regions.map(r => r.id).sort(), [...targetIds].sort());
  const score = c => [c.plan.pressure.reduce((s, p) => s + p.count - p.available_ports, 0), c.plan.inversions, c.plan.lanes.length];
  const better = (a, b) => { const x = score(a), y = score(b); for (let i = 0; i < x.length; i++) if (x[i] !== y[i]) return x[i] < y[i]; return false; };
  while (trials < orderBudget) {
    let next = best;
    for (const bank of fixed ? ['targetOrder'] : ['order', 'targetOrder']) for (let i = 0; i < best[bank].length; i++) for (let j = 0; j < best[bank].length; j++) {
      if (i === j || trials >= orderBudget) continue;
      const order = [...best[bank]], [id] = order.splice(i, 1); order.splice(j, 0, id);
      const candidate = evaluate(bank === 'order' ? order : best.order, bank === 'targetOrder' ? order : best.targetOrder); trials++;
      if (better(candidate, next)) next = candidate;
    }
    if (next === best) break; best = next;
  }
  const orderMs = performance.now() - start;
  const finish = (prepared, policy) => {
  const finishStart = performance.now(), { base, nodes, plan, sx } = prepared;
  // structuredClone restores ordinary object prototypes in portfolio candidates.
  base.ports = Object.assign(Object.create(null), base.ports);
  base.witness.utilizations = Object.assign(Object.create(null), base.witness.utilizations);
  if (plan.pressure.length) fail('composition-port-access-pressure', 'Free straight-access ports cannot carry the frontier margins', plan.pressure);
  for (const id of plan.producers) plan.sourceGroups.filter(g => g.node === id).forEach((g, i) => {
    const p = nodes.get(id).output[i]; g.port = p.id; g.port_x = p.connection.x; g.port_y = p.connection.y;
  });
  let cursor = 0;
  for (const id of plan.consumers) {
    const n = nodes.get(id), groups = plan.targetGroups.filter(g => g.node === id);
    const preferred = groups.flatMap((g, i) => g.lanes.map(l => plan.sourceGroups.find(s => s.id === l.source_group).port_y - n.input[i].position.y));
    n.y = Math.max(cursor, median(preferred)); cursor = n.y + n.shape.height + 1;
    groups.forEach((g, i) => { g.port = n.input[i].id; g.port_y = n.y + n.input[i].position.y; });
  }
  const laneRows = [];
  for (const l of plan.sourceOrder) laneRows.push(Math.max(plan.sourceGroups.find(g => g.id === l.source_group).port_y, (laneRows.at(-1) ?? -3) + 3));
  const interfaceStart = performance.now();
  const geometry = planPortInterface(catalogs, plan, orderedApproach, sx, { allowSharedCell: true, laneRows: policy === 'source' ? laneRows : null }), witness = base.witness;
  const loweringStart = performance.now();
  for (const id of plan.consumers) {
    const n = nodes.get(id); n.x = geometry.tx + 1;
    witness.placements.push({ instance: id, recipe: n.recipe.id, facility: n.recipe.facility, x: n.x, y: n.y, width: n.shape.width, height: n.shape.height, rotation: n.shape.rotation });
    witness.utilizations[id] = structuredClone(n.utilization);
    base.ports[id] = n.shape.ports.map(p => ({ ...p, position: { x: n.x + p.position.x, y: n.y + p.position.y },
      connection: { x: n.x + p.position.x + delta[p.edge][0], y: n.y + p.position.y + delta[p.edge][1] } }));
  }
  const nets = new Map(witness.transport_networks.map(n => [n.item, { ...n, cells: new Map(n.cells.map(p => [key(p), p])), segments: new Map(n.segments.map(s => [key(s.from) + '>' + key(s.to), s])) }]));
  const network = item => {
    if (!nets.has(item)) nets.set(item, { id: `network:${item}`, item, transport: 'belt', requirement_ids: [], cells: new Map(), segments: new Map(), terminals: [], component_ids: [] });
    return nets.get(item);
  };
  for (const g of [...plan.sourceGroups, ...plan.targetGroups]) {
    const p = base.ports[g.node].find(p => p.id === g.port); g.port_x = p.connection.x;
    network(g.item).terminals.push({ id: JSON.stringify([g.node, g.port]), node: g.node, direction: p.direction,
      endpoint: { kind: 'facility', instance: g.node, port: g.port }, position: p.connection, rate: dto(g.value) });
  }
  for (const l of plan.lanes) {
    const n = network(l.item), g = plan.sourceGroups.find(g => g.id === l.source_group);
    const points = [[g.port_x, g.port_y], ...geometry.paths.get(l.id).map(pair)];
    const count = 1 + points.slice(1).reduce((s, p, i) => s + Math.abs(p[0] - points[i][0]) + Math.abs(p[1] - points[i][1]), 0);
    if (count > 200000) fail('composition-path-budget', 'Frontier path exceeds diagnostic materialization budget', { lane: l.id, cells: count });
    const cells = polyline(points);
    n.requirement_ids.push(JSON.stringify(['frontier', l.id]));
    for (const p of cells) n.cells.set(key(p), p);
    for (let i = 1; i < cells.length; i++) {
      const from = cells[i - 1], to = cells[i], id = key(from) + '>' + key(to), old = n.segments.get(id);
      n.segments.set(id, { from, to, rate: dto(add(old ? parseRate(old.rate) : rate(0), parseRate(l.rate))) });
    }
  }
  for (const { items, ...c } of geometry.components) {
    const id = JSON.stringify(['frontier', c.id]); witness.logistics_components.push({ ...c, id });
    for (const item of items) network(item).component_ids.push(id);
  }
  witness.transport_networks = [...nets.values()].map(n => ({ ...n, cells: [...n.cells.values()], segments: [...n.segments.values()] }));
  const all = [...members, ...targetIds], included = new Set(all), expected = id => { const { label, ...n } = graph.nodes.find(n => n.id === id); return n; };
  const serialize = gs => gs.map(({ value, lanes, ...g }) => ({ ...g, rate: dto(value), lanes: lanes.map(l => l.id) }));
  return { schema_version: 1, case: 'actual-graph-block-composition', embedding: `composed-${policy}`, accepted: false, factory_validated: false,
    witness, labels: Object.fromEntries(graph.nodes.map(n => [n.id, n.label])), catalog_ports: base.ports,
    block: structuredClone({ id: 'composition', members: all, request: { schema_version: 1, producers: [...members].map(expected), consumers: targetIds.map(expected) },
      pending_edges: graph.edges.filter(e => included.has(e.source) !== included.has(e.target)) }),
    port_groups: { sources: serialize(plan.sourceGroups), targets: serialize(plan.targetGroups) }, tree_modules: geometry.trees,
    permutation: geometry.permutation, permutation_origin: geometry.permutationOrigin, permutation_transform: geometry.permutationTransform,
    composition_plan: { regions: base.translations, frontier: targetIds, consumed_edges: incoming, bindings: plan.lanes, order_trials: trials,
      order_budget: orderBudget, order_budget_exhausted: trials >= orderBudget, source_order: prepared.order, consumer_order: prepared.targetOrder, inversions: plan.inversions,
      lane_policy: policy, lane_rows: policy === 'source' ? laneRows : null, stack_cache: cacheMetrics,
      region_placement: fixed ? 'explicit-occupied-packing' : 'rectangular-stack' },
    timings: { ...profile, order_ms: orderMs, port_placement_ms: interfaceStart - finishStart, interface_ms: loweringStart - interfaceStart,
      lowering_ms: performance.now() - loweringStart, embedding_lowering_ms: performance.now() - finishStart }, construction_ms: performance.now() - finishStart };
  };
  if (lanePolicy !== 'portfolio') { const result = finish(best, lanePolicy); result.construction_ms = performance.now() - start; return result; }
  const candidates = [], failures = [];
  for (const policy of ['packed', 'source']) {
    try { candidates.push(finish(structuredClone(best), policy)); }
    catch (e) { failures.push({ embedding: `composed-${policy}`, code: e.code ?? 'composition-geometry-failed', severity: 'error', stage: e.stage ?? 'geometry', message: e.message, entity: e.entity }); }
  }
  return { kind: 'unvalidated-composition-portfolio', candidates, failures, accepted: false, construction_ms: performance.now() - start };
}
