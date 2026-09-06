import { rotateFacility } from '../constructive-placement-only/model.mjs';
import { project } from '../constructive-factory-closure/model.mjs';
import { bindAdjacentEdges } from '../constructive-reciprocal-strips/model.mjs';
import { applyReallocation } from '../constructive-reciprocal-strips/graph-delta.mjs';
import { boundedFlow } from '../constructive-scc-groups/project.mjs';
import { rate, add, compare, parseRate, capacity, dto } from '../constructive-interconnect/rates.mjs';
import { bodyCells } from '../constructive-local-reembedding/movement.mjs';
import { key, distance } from '../constructive-local-reembedding/path.mjs';
const fail = (code, message) => { throw Object.assign(new Error(message), { code, severity: 'info', stage: 'reciprocal-bank' }); };
const sum = xs => xs.reduce(add, rate(0));
const permutations = xs => xs.length ? xs.flatMap((x, i) => permutations(xs.filter((_, j) => j !== i)).map(rest => [x, ...rest])) : [[]];

export function reciprocalFamily(graph, catalogs, members) {
  if (!members.length || new Set(members).size !== members.length || members.some(id => !graph.nodes.some(n => n.id === id)))
    fail('bank-member-schema', 'Expected nonempty unique known source-bank members');
  const recipes = new Map(graph.nodes.filter(n => members.includes(n.id)).map(n => [n.id, catalogs.recipes.recipes.find(r => r.id === n.recipe)]));
  const simple = [...recipes].filter(([, r]) => r.inputs.length === 1 && r.outputs.length === 1 && r.inputs[0].item !== r.outputs[0].item &&
    [...r.inputs, ...r.outputs].every(a => catalogs.items.items.find(i => i.id === a.item)?.transport === 'belt'));
  const pairs = new Map();
  for (const [, a] of simple) for (const [, b] of simple) if (a.inputs[0].item === b.outputs[0].item && a.outputs[0].item === b.inputs[0].item)
    pairs.set(JSON.stringify([a.inputs[0].item, a.outputs[0].item].sort()), [a.inputs[0].item, a.outputs[0].item]);
  if (pairs.size !== 1) fail('bank-reciprocal-family', 'Expected one reciprocal two-commodity recipe family');
  const items = [...pairs.values()][0], core = simple.filter(([, r]) => items.includes(r.inputs[0].item) && items.includes(r.outputs[0].item)).map(([id]) => id);
  const cuts = graph.edges.filter(e => core.includes(e.source) !== core.includes(e.target));
  if (cuts.some(e => core.includes(e.target)) || new Set(cuts.map(e => e.item)).size !== 1) fail('bank-open-family', 'Expected input-closed reciprocal family with one outgoing commodity');
  const forward = cuts[0].item, back = items.find(i => i !== forward);
  return { members: core, forward, back, producers: core.filter(id => recipes.get(id).outputs[0].item === forward),
    returners: core.filter(id => recipes.get(id).outputs[0].item === back), consumers: members.filter(id => !core.includes(id)) };
}

export function adjacentPairs(w, catalogs) {
  const ports = project(w, catalogs), body = new Set(w.placements.flatMap(bodyCells).map(key)), pairs = new Set();
  for (const a of w.placements) for (const b of w.placements) if (a.instance !== b.instance && ports[a.instance].some(p =>
    p.direction === 'output' && !body.has(key(p.connection)) && ports[b.instance].some(q => q.direction === 'input' && q.transport === p.transport &&
      !body.has(key(q.connection)) && distance(p.connection, q.connection) === 1))) pairs.add(JSON.stringify([a.instance, b.instance]));
  return pairs;
}

export function allocateAdjacent(graph, family, pairs, catalogs, order) {
  const edges = graph.edges.filter(e => family.producers.includes(e.source) && e.item === family.forward);
  if (!edges.length || edges.some(e => !e.internal)) fail('bank-allocation-boundary', 'Expected internal forward consumers at this source-bank cut');
  const sources = order.filter(id => family.producers.includes(id));
  const targets = [...new Set(edges.map(e => e.target))].sort((a, b) =>
    Number(!family.members.includes(a)) - Number(!family.members.includes(b)) || order.indexOf(a) - order.indexOf(b) || a.localeCompare(b));
  const total = (field, id) => sum(edges.filter(e => e[field] === id).map(e => parseRate(e.rate)));
  const cap = capacity(catalogs.transports.transports.find(t => t.kind === 'belt').capacity), arcs = [], choices = [];
  const s = id => JSON.stringify(['source', id]), t = id => JSON.stringify(['target', id]);
  for (const id of sources) arcs.push({ from: 'root', to: s(id), capacity: dto(total('source', id)) });
  for (const source of sources) for (const target of targets) {
    const inside = family.members.includes(target);
    if (inside && !pairs.has(JSON.stringify([source, target]))) continue;
    const limits = [total('source', source), total('target', target), ...(inside ? [cap] : [])];
    const value = limits.reduce((a, b) => compare(a, b) < 0 ? a : b);
    choices.push({ source, target, index: arcs.length }); arcs.push({ from: s(source), to: t(target), capacity: dto(value) });
  }
  for (const id of targets) arcs.push({ from: t(id), to: 'sink', capacity: dto(total('target', id)) });
  const flow = boundedFlow(arcs, 'root', 'sink'), expected = sum(sources.map(id => total('source', id)));
  if (compare(parseRate(flow.total), expected)) return null;
  const replacement_edges = choices.filter(c => flow.flows[c.index].numerator > 0).map(c => ({ id: JSON.stringify(['adjacent-bank', family.forward, c.source, c.target]),
    source: c.source, target: c.target, item: family.forward, transport: 'belt', internal: true, rate: flow.flows[c.index] }));
  const delta = { item: family.forward, replaced_edge_ids: edges.map(e => e.id), replacement_edges,
    invariant: 'exact-per-source-and-per-target-commodity-margins' };
  return { graph: applyReallocation(graph, delta), delta, flow: { expected: dto(expected), actual: flow.total, augmentations: flow.augmentations } };
}

export function bankCandidates(graph, catalogs, members, { maxLayouts = 2048 } = {}) {
  const family = reciprocalFamily(graph, catalogs, members);
  if (family.members.length > 7 || family.producers.length > 4 || family.returners.length > 3 || family.producers.length !== family.returners.length + 1)
    fail('bank-strip-scope', 'Alternating strip probe supports up to four producers and three returners');
  if (!Number.isSafeInteger(maxLayouts) || maxLayouts < 1) fail('bank-layout-budget', 'Expected positive layout budget');
  const nodes = new Map(graph.nodes.map(n => [n.id, n])), recipes = new Map(family.members.map(id => [id, catalogs.recipes.recipes.find(r => r.id === nodes.get(id).recipe)]));
  const candidateShapes = (ids, rotation) => ids.every(id => catalogs.facilities.facilities.find(f => f.id === recipes.get(id).facility).allowed_rotations.includes(rotation));
  const backEdges = graph.edges.filter(e => family.members.includes(e.source) && family.members.includes(e.target) && e.item === family.back);
  const candidates = [], counts = { layouts: 0, return_nonadjacent: 0, flow_rejected: 0, binding_rejected: 0 }, seen = new Set();
  for (const producers of permutations(family.producers)) for (const returners of permutations(family.returners)) {
    const order = producers.flatMap((id, i) => i < returners.length ? [id, returners[i]] : [id]);
    for (const axis of ['x', 'y']) for (const pr of [0, 90, 180, 270]) for (const rr of [0, 90, 180, 270]) {
      if (!candidateShapes(producers, pr) || !candidateShapes(returners, rr)) continue;
      if (++counts.layouts > maxLayouts) fail('bank-layout-budget', 'Alternating strip layout budget exhausted');
      let offset = 0;
      const placements = order.map(instance => {
        const r = recipes.get(instance), f = catalogs.facilities.facilities.find(f => f.id === r.facility), rotation = producers.includes(instance) ? pr : rr;
        const shape = rotateFacility(f, rotation), p = { instance, recipe: r.id, facility: f.id, rotation, width: shape.width, height: shape.height,
          x: axis === 'x' ? offset + 1 : 1, y: axis === 'y' ? offset + 1 : 1 }; offset += axis === 'x' ? shape.width : shape.height; return p;
      });
      const w = { schema_version: 1, placements, utilizations: Object.fromEntries(order.map(id => [id, nodes.get(id).utilization])), logistics_components: [], transport_networks: [] };
      const pairs = adjacentPairs(w, catalogs);
      if (backEdges.some(e => !pairs.has(JSON.stringify([e.source, e.target])))) { counts.return_nonadjacent++; continue; }
      const allocation = allocateAdjacent(graph, family, pairs, catalogs, order);
      if (!allocation) { counts.flow_rejected++; continue; }
      const internal = allocation.graph.edges.filter(e => family.members.includes(e.source) && family.members.includes(e.target));
      const bound = bindAdjacentEdges(w, catalogs, internal, project(w, catalogs), 10000);
      if (!bound) { counts.binding_rejected++; continue; }
      w.transport_networks = bound.networks;
      const points = [...placements.flatMap(bodyCells), ...w.transport_networks.flatMap(n => n.cells)];
      const x = Math.min(...points.map(p => p.x)), y = Math.min(...points.map(p => p.y)), move = p => ({ ...p, x: p.x - x, y: p.y - y });
      w.placements = w.placements.map(move); w.transport_networks = w.transport_networks.map(n => ({ ...n, cells: n.cells.map(move), segments: n.segments.map(s => ({ ...s, from: move(s.from), to: move(s.to) })), terminals: n.terminals.map(t => ({ ...t, position: move(t.position) })) }));
      const signature = JSON.stringify(w); if (seen.has(signature)) continue; seen.add(signature);
      const detail = { schema_version: 1, case: 'actual-graph-cycle-block', embedding: 'adjacency-flow-bank', accepted: false, factory_validated: false,
        witness: w, catalog_ports: project(w, catalogs), labels: Object.fromEntries(graph.nodes.map(n => [n.id, n.label])), port_groups: { sources: [], targets: [] },
        reallocation: allocation.delta, block: { id: 'adjacent-reciprocal-bank', members: family.members, cycle: family.members },
        adjacent_bank: { axis, order, producer_rotation: pr, returner_rotation: rr, flow: allocation.flow, assignment_nodes: bound.visited } };
      candidates.push({ detail, graph: allocation.graph, family });
    }
  }
  return { family, candidates, counts };
}
