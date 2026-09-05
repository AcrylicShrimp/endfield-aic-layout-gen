import { constructInterface, orderedApproach } from '../constructive-auto-interface/model.mjs';
import { allocateBuckets } from '../constructive-auto-interface/allocation.mjs';
import { treeModule, treeShape } from '../constructive-auto-interface/trees.mjs';
import { rate, add, multiply, compare, parseRate, dto, capacity } from '../constructive-interconnect/rates.mjs';
import { polyline } from '../constructive-interconnect/geometry.mjs';
import { fail } from './graph.mjs';
const key = p => `${p.x},${p.y}`, pair = p => [p.x, p.y];

export function constructCycle(catalogs, graph, block, family = 'leaf') {
  if (!['leaf', 'root'].includes(family)) fail('cycle-return-family', 'Expected leaf or root return geometry', family);
  const start = performance.now(), forward = constructInterface(catalogs, block.request), old = forward.witness;
  const retained = old.transport_networks.filter(n => n.item === block.forward_item);
  if (retained.length !== 1 || retained[0].terminals.some(t => t.endpoint.kind !== 'facility'))
    fail('cycle-forward-boundary', 'Forward commodity must be wholly internal', block.id);
  const positions = new Map(old.placements.map(p => [p.instance, p]));
  const sort = ids => [...ids].sort((a, b) => positions.get(a).y - positions.get(b).y || a.localeCompare(b));
  const sources = sort(block.returners), targets = sort(block.producers), nodes = new Map();
  for (const id of [...sources, ...targets]) {
    const ports = forward.catalog_ports[id];
    const terminals = direction => old.transport_networks.filter(n => n.item === block.return_item).flatMap(n => n.terminals)
      .filter(t => t.endpoint.kind === 'facility' && t.endpoint.instance === id && t.direction === direction);
    const values = direction => { const ts = terminals(direction); return ts.length ? [{ item: block.return_item,
      value: ts.reduce((s, t) => add(s, parseRate(t.rate)), rate(0)) }] : []; };
    nodes.set(id, { input: ports.filter(p => p.transport === 'belt' && p.direction === 'input' && p.edge === 'west'),
      output: ports.filter(p => p.transport === 'belt' && p.direction === 'output' && p.edge === 'east'), inputs: values('input'), outputs: values('output') });
  }
  const plan = allocateBuckets(nodes, [block.return_item], sources, targets, capacity(catalogs.transports.transports.find(t => t.kind === 'belt').capacity));
  if (plan.pressure.length || !plan.lanes.length || plan.inversions) fail('cycle-return-allocation', 'Return capacity ports or monotone commodity order failed', plan.pressure);
  for (const [gs, bank, direction] of [[plan.sourceGroups, sources, 'output'], [plan.targetGroups, targets, 'input']])
    for (const id of bank) gs.filter(g => g.node === id).forEach((g, i) => { g.id = 'return:' + g.id; const p = nodes.get(id)[direction][i];
      g.port = p.id; g.port_y = p.connection.y; g.port_x = p.connection.x; });
  // Group objects are shared by the allocator; refresh fragment references after namespacing.
  for (const l of plan.lanes) { l.id = 'return:' + l.id; l.source_group = 'return:' + l.source_group; l.target_group = 'return:' + l.target_group; }
  const laneCount = family === 'leaf' ? plan.lanes.length : plan.sourceGroups.length;
  const treeWidth = Math.max(...[...plan.sourceGroups, ...plan.targetGroups].map(g => treeShape(g.lanes.length).width));
  const shiftX = laneCount + treeWidth * (family === 'leaf' ? 1 : 2) + plan.targetGroups.length + 3, shiftY = laneCount + 2;
  const move = p => ({ ...p, x: p.x + shiftX, y: p.y + shiftY });
  const witness = { ...old, utilizations: structuredClone(old.utilizations), placements: old.placements.map(move),
    transport_networks: retained.map(n => ({ ...n, cells: n.cells.map(move), segments: n.segments.map(s => ({ ...s, from: move(s.from), to: move(s.to) })),
      terminals: n.terminals.map(t => ({ ...t, position: move(t.position) })) })),
    logistics_components: old.logistics_components.filter(c => retained[0].component_ids.includes(c.id)).map(c => ({ ...c, position: move(c.position) })) };
  const ports = Object.fromEntries(Object.entries(forward.catalog_ports).map(([id, ps]) => [id, ps.map(p => ({ ...p, position: move(p.position), connection: move(p.connection) }))]));
  const targetX = Math.min(...plan.targetGroups.map(g => g.port_x)) + shiftX;
  const sourceX = Math.max(...plan.sourceGroups.map(g => g.port_x)) + shiftX;
  if (plan.targetGroups.some(g => g.port_x + shiftX !== targetX) || plan.sourceGroups.some(g => g.port_x + shiftX !== sourceX))
    fail('cycle-return-faces', 'Return endpoints must share their bank face', block.id);
  const mergeX = targetX - plan.targetGroups.length - 2 - (treeWidth - 1), sourceFace = sourceX + plan.sourceGroups.length + 2;
  const splitX = family === 'leaf' ? sourceFace : mergeX - treeWidth;
  const rank = new Map(plan.sourceOrder.map((l, i) => [l.id, i]));
  for (const g of [...plan.sourceGroups, ...plan.targetGroups]) {
    g.port_y += shiftY; g.port_x += shiftX;
    g.row = shiftY + rank.get(g.lanes[0].id); g.root_row = g.row + treeShape(g.lanes.length).root_index;
  }
  const approachesIn = orderedApproach(plan.sourceGroups.map(g => ({ id: g.id, start: { x: sourceX, y: g.port_y }, end: { x: sourceFace, y: g.root_row } })));
  const approachesOut = orderedApproach(plan.targetGroups.map(g => ({ id: g.id, start: { x: mergeX + treeWidth - 1, y: g.root_row }, end: { x: targetX, y: g.port_y } })));
  const trees = [], splitPaths = new Map(), mergePaths = new Map();
  for (const [groups, kind, x, paths] of [[plan.sourceGroups, 'splitter', splitX, splitPaths], [plan.targetGroups, 'converger', mergeX, mergePaths]])
    for (const g of groups) {
      const tree = treeModule(g, kind, { x, y: g.row }, catalogs, { width: treeWidth }); trees.push(tree);
      witness.logistics_components.push(...tree.components.map(({ items, ...c }) => c));
      for (const p of tree.paths) paths.set(p.id, p.cells);
    }
  const cells = new Map(), segments = new Map(), terminals = [];
  for (const [groups, direction] of [[plan.sourceGroups, 'output'], [plan.targetGroups, 'input']]) for (const g of groups)
    terminals.push({ id: JSON.stringify([g.node, g.port]), node: g.node, direction,
      endpoint: { kind: 'facility', instance: g.node, port: g.port }, position: { x: g.port_x, y: g.port_y }, rate: dto(g.value) });
  const perimeters = [];
  for (const l of plan.sourceOrder) {
    const group = plan.sourceGroups.find(g => g.id === l.source_group);
    const i = family === 'leaf' ? rank.get(l.id) : plan.sourceGroups.indexOf(group);
    const sy = family === 'leaf' ? splitPaths.get(l.id).at(-1).y : group.root_row;
    const ty = family === 'leaf' ? mergePaths.get(l.id)[0].y : group.root_row;
    const right = sourceFace + (family === 'leaf' ? treeWidth : 1) + i;
    const left = (family === 'leaf' ? mergeX : splitX) - 1 - i, top = shiftY - 1 - i;
    const around = [[right, sy], [right, top], [left, top], [left, ty]];
    const sourceLeg = family === 'leaf' ? [...splitPaths.get(l.id).map(pair), ...around] : [...around, ...splitPaths.get(l.id).map(pair)];
    const path = polyline([...approachesIn.get(l.source_group), ...sourceLeg,
      ...mergePaths.get(l.id).map(pair), ...approachesOut.get(l.target_group)]);
    perimeters.push({ id: l.id, corners: around });
    for (const p of path) cells.set(key(p), p);
    for (let j = 1; j < path.length; j++) {
      const from = path[j - 1], to = path[j], tag = key(from) + '>' + key(to), old = segments.get(tag);
      segments.set(tag, { from, to, rate: dto(add(old ? parseRate(old.rate) : rate(0), parseRate(l.rate))) });
    }
    if (cells.size + segments.size > 100000) fail('cycle-cell-budget', 'Return geometry exceeds diagnostic materialization budget', block.id);
  }
  witness.transport_networks.push({ id: `network:${block.return_item}`, item: block.return_item, transport: 'belt',
    requirement_ids: plan.lanes.map(l => l.id), cells: [...cells.values()], segments: [...segments.values()], terminals,
    component_ids: trees.flatMap(t => t.components.map(c => c.id)) });
  const serialize = gs => gs.map(({ value, lanes, ...g }) => ({ ...g, rate: dto(value), lanes: lanes.map(l => l.id) }));
  return { schema_version: 1, case: 'actual-graph-cycle-block', embedding: `cycle-${family}`, accepted: false, factory_validated: false,
    witness, catalog_ports: ports, labels: Object.fromEntries(graph.nodes.map(n => [n.id, n.label])), block: structuredClone(block),
    port_groups: { sources: serialize(plan.sourceGroups), targets: serialize(plan.targetGroups) }, tree_modules: trees,
    pre_return_plan: { forward_embedding: forward.embedding, forward_bindings: forward.bindings }, return_plan: { family, perimeter_channels: laneCount, bindings: plan.lanes, perimeters },
    construction_ms: performance.now() - start, pending: ['independent-cycle-validation', 'outgoing-graph-obligations'] };
}

// Recipe coverage alone is insufficient: check exactly which graph obligations remain.
export function inspectBlockCoverage(detail, validation, catalogs) {
  const diagnostics = [], error = (code, message, entity) => diagnostics.push({ code, severity: 'error', stage: 'cycle-coverage', message, entity });
  if (!validation.covered_geometry_validated) error('cycle-geometry-rejected', 'Independent covered-geometry validation failed', detail.block.id);
  for (const n of detail.witness.transport_networks) for (const t of n.terminals) if (t.endpoint.kind === 'external')
    error('cycle-fake-external', 'Cycle block must not replace internal return flow or pending outputs with external terminals', t.id);
  const members = detail.witness.placements.map(p => p.instance);
  if (members.length !== detail.block.members.length || new Set(members).size !== members.length ||
      detail.block.members.some(id => !members.includes(id))) error('cycle-membership-mismatch', 'Witness membership must match original block ownership exactly', detail.block.members);
  const required = new Map();
  for (const n of [...detail.block.request.producers, ...detail.block.request.consumers]) {
    const p = detail.witness.placements.find(p => p.instance === n.id), u = detail.witness.utilizations[n.id];
    if (p?.recipe !== n.recipe || !u || compare(parseRate(u), parseRate(n.utilization)))
      error('cycle-recipe-mismatch', 'Witness recipe and utilization must match original graph margins', n.id);
    const recipe = catalogs.recipes.recipes.find(r => r.id === n.recipe);
    if (!recipe) { error('cycle-recipe-mismatch', 'Original graph recipe is absent from runtime catalog', n.id); continue; }
    for (const direction of ['input', 'output']) for (const a of recipe[direction + 's']) {
      const id = JSON.stringify([n.id, direction, a.item]);
      required.set(id, add(required.get(id) ?? rate(0), multiply(rate(BigInt(a.quantity) * 1000n, recipe.duration_ms), parseRate(n.utilization))));
    }
  }
  const expected = new Map();
  for (const e of detail.block.pending_edges) {
    if (!detail.block.members.includes(e.source) || detail.block.members.includes(e.target))
      error('cycle-cut-mismatch', 'Pending edge must be an outgoing original block obligation', e.id);
    const id = JSON.stringify([e.source, 'output', e.item]); expected.set(id, add(expected.get(id) ?? rate(0), parseRate(e.rate)));
  }
  const seen = new Set();
  for (const t of validation.recipe_terminals) {
    const id = JSON.stringify([t.instance, t.direction, t.item]), value = expected.get(id) ?? rate(0), total = required.get(id);
    if (seen.has(id) || !total) error('cycle-terminal-report-shape', 'Duplicate or unexpected validated recipe terminal', id);
    seen.add(id);
    if (compare(parseRate(t.pending_rate), value)) error('cycle-pending-mismatch', 'Pending recipe rate differs from declared outgoing graph obligations',
      { instance: t.instance, direction: t.direction, item: t.item, expected: dto(value), actual: t.pending_rate });
    if (total && (compare(parseRate(t.required_rate), total) || compare(add(parseRate(t.pending_rate), parseRate(t.routed_rate)), total)))
      error('cycle-terminal-rate-mismatch', 'Validated terminal total differs from original graph recipe margin', id);
    expected.delete(id);
  }
  for (const id of required.keys()) if (!seen.has(id)) error('cycle-terminal-report-shape', 'Missing validated recipe terminal', id);
  for (const id of expected.keys()) error('cycle-cut-mismatch', 'Outgoing graph obligation has no matching recipe terminal', id);
  return { accepted: diagnostics.length === 0, diagnostics };
}
