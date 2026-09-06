import { rotateFacility } from '../constructive-placement-only/model.mjs';
import { project } from '../constructive-factory-closure/model.mjs';
import { capacity, compare, parseRate } from '../constructive-interconnect/rates.mjs';
import { key, distance } from '../constructive-local-reembedding/path.mjs';
import { bodyCells } from '../constructive-local-reembedding/movement.mjs';
import { compatibleAllocations } from './allocation.mjs';
const fail = (code, message, entity = null) => { throw Object.assign(new Error(message), { code, severity: 'info', stage: 'reciprocal-strip', entity }); };
const permutations = xs => xs.length ? xs.flatMap((x, i) => permutations(xs.filter((_, j) => j !== i)).map(t => [x, ...t])) : [[]];

export function reciprocalCore(graph, block) {
  const members = [...block.cycle], owned = new Set(members);
  const cuts = graph.edges.filter(e => owned.has(e.source) !== owned.has(e.target));
  if (cuts.some(e => !owned.has(e.source))) fail('strip-open-input', 'Reciprocal core has an incoming graph obligation', block.id);
  return { ...block, id: `${block.id}-core`, members, consumers: [...block.returners],
    request: { ...block.request, consumers: block.request.consumers.filter(n => owned.has(n.id)) },
    covered_edges: graph.edges.filter(e => owned.has(e.source) && owned.has(e.target)), pending_edges: cuts };
}

export function bindAdjacentEdges(w, catalogs, edges, ports, budget) {
  const bodies = new Set(w.placements.flatMap(bodyCells).map(key)), usedPorts = new Set(), occupied = { belt: new Set(), pipe: new Set() };
  const jobs = edges.map(e => {
    const cap = capacity(catalogs.transports.transports.find(t => t.kind === e.transport).capacity);
    if (compare(parseRate(e.rate), cap) > 0) return { edge: e, options: [] };
    const pairs = [];
    for (const a of ports[e.source]) for (const b of ports[e.target]) {
      if (a.direction !== 'output' || b.direction !== 'input' || a.transport !== e.transport || b.transport !== e.transport ||
        distance(a.connection, b.connection) !== 1 || bodies.has(key(a.connection)) || bodies.has(key(b.connection))) continue;
      pairs.push({ a, b });
    }
    return { edge: e, options: pairs };
  }).sort((a, b) => a.options.length - b.options.length || a.edge.id.localeCompare(b.edge.id));
  let visited = 0; const selected = [];
  const visit = i => {
    if (++visited > budget) fail('strip-assignment-budget', 'Two-cell port assignment budget exhausted');
    if (i === jobs.length) return true;
    const { edge: e, options } = jobs[i];
    for (const { a, b } of options) {
      const ap = JSON.stringify([e.source, a.id]), bp = JSON.stringify([e.target, b.id]), ac = key(a.connection), bc = key(b.connection), layer = occupied[e.transport];
      if (usedPorts.has(ap) || usedPorts.has(bp) || layer.has(ac) || layer.has(bc)) continue;
      usedPorts.add(ap); usedPorts.add(bp); layer.add(ac); layer.add(bc); selected.push({ e, a, b });
      if (visit(i + 1)) return true;
      selected.pop(); usedPorts.delete(ap); usedPorts.delete(bp); layer.delete(ac); layer.delete(bc);
    }
    return false;
  };
  if (!visit(0)) return null;
  const networks = new Map();
  for (const { e, a, b } of selected) {
    const id = JSON.stringify([e.transport, e.item]);
    if (!networks.has(id)) networks.set(id, { id, item: e.item, transport: e.transport, cells: [], segments: [], terminals: [], component_ids: [], requirement_ids: [] });
    const n = networks.get(id); n.requirement_ids.push(e.id); n.cells.push(a.connection, b.connection);
    n.segments.push({ from: a.connection, to: b.connection, rate: e.rate });
    for (const [node, port, direction] of [[e.source, a, 'output'], [e.target, b, 'input']])
      n.terminals.push({ id: JSON.stringify([e.id, direction]), node, endpoint: { kind: 'facility', instance: node, port: port.id }, direction, position: port.connection, rate: e.rate });
  }
  return { networks: [...networks.values()], visited };
}

export function stripCandidates(graph, catalogs, block, { assignmentBudget = 10000 } = {}) {
  const core = reciprocalCore(graph, block);
  if (core.members.length < 2 || core.members.length > 3) fail('strip-member-bound', 'Touching strip enumeration currently covers 2..3 cyclic members', core.members.length);
  if (!Number.isSafeInteger(assignmentBudget) || assignmentBudget < 1) fail('strip-budget-input', 'Expected a positive port-assignment budget');
  const nodes = new Map(graph.nodes.map(n => [n.id, n])), shapes = new Map();
  for (const id of core.members) {
    const n = nodes.get(id), recipe = catalogs.recipes.recipes.find(r => r.id === n.recipe), f = catalogs.facilities.facilities.find(f => f.id === recipe.facility);
    shapes.set(id, f.allowed_rotations.map(rotation => ({ ...rotateFacility(f, rotation), facility: f.id })));
  }
  const candidates = [], diagnostics = []; let layouts = 0, rejected = 0;
  for (const order of permutations(core.members)) for (const axis of ['x', 'y']) {
    const enumerate = placements => {
      if (placements.length < order.length) {
        const instance = order[placements.length], n = nodes.get(instance), offset = placements.reduce((v, p) => v + (axis === 'x' ? p.width : p.height), 0);
        for (const s of shapes.get(instance)) enumerate([...placements, { instance, recipe: n.recipe, facility: s.facility, rotation: s.rotation, width: s.width, height: s.height,
          x: axis === 'x' ? offset + 1 : 1, y: axis === 'y' ? offset + 1 : 1 }]);
        return;
      }
      const ordinal = layouts++, w = { schema_version: 1, placements, utilizations: Object.fromEntries(core.members.map(id => [id, nodes.get(id).utilization])), logistics_components: [], transport_networks: [] };
      const ports = project(w, catalogs); let result;
      try { result = bindAdjacentEdges(w, catalogs, core.covered_edges, ports, assignmentBudget); }
      catch (e) { if (e.code !== 'strip-assignment-budget') throw e; diagnostics.push({ code: e.code, severity: e.severity, stage: e.stage, message: e.message, ordinal }); return; }
      if (!result) { rejected++; return; }
      w.transport_networks = result.networks;
      // Normalize by actual occupied cells, not the unused experimental one-cell margin.
      const points = [...placements.flatMap(bodyCells), ...w.transport_networks.flatMap(n => n.cells)];
      const dx = -Math.min(...points.map(p => p.x)), dy = -Math.min(...points.map(p => p.y)), move = p => ({ ...p, x: p.x + dx, y: p.y + dy });
      w.placements = w.placements.map(move);
      w.transport_networks = w.transport_networks.map(n => ({ ...n, cells: n.cells.map(move), segments: n.segments.map(s => ({ ...s, from: move(s.from), to: move(s.to) })), terminals: n.terminals.map(t => ({ ...t, position: move(t.position) })) }));
      candidates.push(structuredClone({ schema_version: 1, case: 'actual-graph-cycle-block', embedding: `touching-strip-${ordinal}`, accepted: false, factory_validated: false,
        witness: w, catalog_ports: project(w, catalogs), labels: Object.fromEntries(graph.nodes.map(n => [n.id, n.label])), block: core,
        port_groups: { sources: [], targets: [] }, strip: { ordinal, axis, order, assignment_nodes: result.visited }, pending: ['independent-cycle-validation', 'outgoing-graph-obligations'] }));
    };
    enumerate([]);
  }
  return { core, candidates, layouts, rejected, diagnostics, enumeration_complete: !diagnostics.length };
}

export function stripPortfolio(graph, catalogs, block, options = {}) {
  const retained = stripCandidates(graph, catalogs, block, options), families = [{ name: 'retained', ...retained }];
  for (const [i, plan] of compatibleAllocations(graph, block).entries()) {
    const r = stripCandidates(plan.graph, catalogs, block, options);
    for (const d of r.candidates) { d.embedding = `allocation-${i}-${d.embedding}`; d.reallocation = structuredClone(plan.reallocation); }
    families.push({ name: `allocation-${i}`, ...r });
  }
  return { candidates: families.flatMap(f => f.candidates), families: families.map(f => ({ name: f.name, layouts: f.layouts, rejected: f.rejected, candidates: f.candidates.length,
    enumeration_complete: f.enumeration_complete })), layouts: families.reduce((n, f) => n + f.layouts, 0), rejected: families.reduce((n, f) => n + f.rejected, 0),
    diagnostics: families.flatMap(f => f.diagnostics), enumeration_complete: families.every(f => f.enumeration_complete) };
}
