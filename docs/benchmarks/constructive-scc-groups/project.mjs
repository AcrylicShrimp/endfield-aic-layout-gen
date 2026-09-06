import { components } from '../constructive-cycle-blocks/graph.mjs';
import { parseRate, dto, rate, add, compare } from '../constructive-interconnect/rates.mjs';
import { project } from '../constructive-factory-closure/model.mjs';
const key = p => `${p.x},${p.y}`;
const fail = (code, message, entity = null) => { throw Object.assign(new Error(message), { code, severity: 'error', stage: 'scc-projection', entity }); };
const minus = (a, b) => rate(a.n * b.d - b.n * a.d, a.d * b.d);

export function concreteAllocation(network) {
  if (network.terminals.some(t => t.endpoint.kind !== 'facility')) fail('scc-allocation-external', 'Source-region allocation must have only facility terminals');
  const sources = network.terminals.filter(t => t.direction === 'output').map(t => ({ t, remaining: parseRate(t.rate) }));
  const sinks = network.terminals.filter(t => t.direction === 'input').map(t => ({ t, remaining: parseRate(t.rate) }));
  const arcs = network.segments.map(s => ({ s, remaining: parseRate(s.rate) })), adjacency = new Map(), pairs = new Map();
  for (const [i, a] of arcs.entries()) { const k = key(a.s.from); if (!adjacency.has(k)) adjacency.set(k, []); adjacency.get(k).push(i); }
  for (const source of sources) while (source.remaining.n) {
    const initial = key(source.t.position), queue = [initial], previous = new Map([[initial, null]]); let target;
    for (let i = 0; i < queue.length; i++) {
      target = sinks.find(s => s.remaining.n && key(s.t.position) === queue[i]); if (target) break;
      for (const j of adjacency.get(queue[i]) ?? []) if (arcs[j].remaining.n) {
        const next = key(arcs[j].s.to); if (!previous.has(next)) { previous.set(next, j); queue.push(next); }
      }
    }
    if (!target) fail('scc-allocation-path-missing', 'Positive retained supply cannot reach residual demand', source.t.id);
    let amount = compare(source.remaining, target.remaining) < 0 ? source.remaining : target.remaining;
    const path = []; for (let k = key(target.t.position); k !== initial;) { const j = previous.get(k); path.push(j); if (compare(arcs[j].remaining, amount) < 0) amount = arcs[j].remaining; k = key(arcs[j].s.from); }
    source.remaining = minus(source.remaining, amount); target.remaining = minus(target.remaining, amount);
    for (const j of path) arcs[j].remaining = minus(arcs[j].remaining, amount);
    const from = source.t.endpoint.instance, to = target.t.endpoint.instance, id = JSON.stringify([from, to]);
    const old = pairs.get(id); pairs.set(id, { source: from, target: to, item: network.item, transport: network.transport,
      value: add(old?.value ?? rate(0), amount) });
  }
  if (sinks.some(s => s.remaining.n) || arcs.some(a => a.remaining.n)) fail('scc-allocation-residual', 'Retained network has unmet demand or residual circulation');
  return [...pairs.values()].map(({ value, ...e }) => ({ ...e, rate: dto(value), internal: true }));
}

export function sourceSccs(graph) {
  return components(graph).filter(ids => {
    const own = new Set(ids);
    return (ids.length > 1 || graph.edges.some(e => e.internal && e.source === ids[0] && e.target === ids[0])) &&
      !graph.edges.some(e => own.has(e.target) && !own.has(e.source));
  }).map((members, i) => ({ id: `source-scc-${i}`, members }));
}

export function readyAtoms(graph, members) {
  const own = new Set(members);
  return components(graph).filter(ids => {
    if (ids.some(id => own.has(id))) return false;
    const next = new Set([...members, ...ids]);
    return graph.edges.some(e => own.has(e.source) && ids.includes(e.target)) &&
      graph.edges.filter(e => ids.includes(e.target)).every(e => e.internal && next.has(e.source));
  });
}

// Exact rational capacities are scaled once; residual arcs permit reassignment.
export function boundedFlow(arcs, source, sink) {
  if (source === sink || arcs.some(a => a.from === a.to)) fail('scc-flow-schema', 'Expected distinct flow endpoints and non-self arcs');
  const gcd = (a, b) => b ? gcd(b, a % b) : a;
  const scale = arcs.reduce((s, a) => { const d = parseRate(a.capacity).d; return s / gcd(s, d) * d; }, 1n);
  const adjacency = new Map(), original = [];
  const list = id => { if (!adjacency.has(id)) adjacency.set(id, []); return adjacency.get(id); };
  for (const a of arcs) {
    const value = parseRate(a.capacity), cap = value.n * (scale / value.d), u = list(a.from), v = list(a.to);
    const forward = { to: a.to, capacity: cap, initial: cap, reverse: v.length }, back = { to: a.from, capacity: 0n, reverse: u.length };
    u.push(forward); v.push(back); original.push(forward);
  }
  let total = 0n, augmentations = 0;
  while (true) {
    const queue = [source], previous = new Map([[source, null]]);
    for (let i = 0; i < queue.length && !previous.has(sink); i++) for (const [index, edge] of (adjacency.get(queue[i]) ?? []).entries())
      if (edge.capacity > 0n && !previous.has(edge.to)) { previous.set(edge.to, { from: queue[i], index }); queue.push(edge.to); }
    if (!previous.has(sink)) break;
    let amount = null;
    for (let id = sink; id !== source;) { const p = previous.get(id), e = adjacency.get(p.from)[p.index]; amount = amount === null || e.capacity < amount ? e.capacity : amount; id = p.from; }
    for (let id = sink; id !== source;) { const p = previous.get(id), e = adjacency.get(p.from)[p.index]; e.capacity -= amount; adjacency.get(id)[e.reverse].capacity += amount; id = p.from; }
    total += amount; augmentations++;
  }
  return { total: dto(rate(total, scale)), flows: original.map(e => dto(rate(e.initial - e.capacity, scale))), augmentations };
}

export function projectRegion(graph, parent, members, catalogs, id = 'scc-region') {
  const own = new Set(members), w = parent.witness;
  if (!members.length || own.size !== members.length || members.some(id => !w.placements.some(p => p.instance === id))) fail('scc-member-schema', 'Expected unique retained members');
  const cuts = graph.edges.filter(e => own.has(e.source) !== own.has(e.target));
  if (cuts.some(e => own.has(e.target))) fail('scc-open-input', 'Projected source region must retain every predecessor');
  const inside = graph.edges.filter(e => own.has(e.source) && own.has(e.target)), retainedComponents = new Set(), networks = [], evidence = [];
  for (const n of w.transport_networks) {
    const requirements = inside.filter(e => e.item === n.item && e.transport === n.transport);
    if (!requirements.length) continue;
    if (w.logistics_components.some(c => n.component_ids.includes(c.id) && c.kind === 'bridge')) fail('scc-bridge-projection-unsupported', 'Do not mix retained bridge channels in an ordinary-cell flow projection', n.id);
    const sources = new Map(), sinks = new Map(), accumulate = (m, id, value) => m.set(id, add(m.get(id) ?? rate(0), parseRate(value)));
    for (const e of requirements) { accumulate(sources, e.source, e.rate); accumulate(sinks, e.target, e.rate); }
    const arcs = [], vertex = p => `cell:${key(p)}`;
    for (const [instance, value] of sources) arcs.push({ from: 'source', to: `supply:${instance}`, capacity: dto(value) });
    for (const [instance, value] of sinks) arcs.push({ from: `demand:${instance}`, to: 'sink', capacity: dto(value) });
    const segments = n.segments.map(s => { const index = arcs.length; arcs.push({ from: vertex(s.from), to: vertex(s.to), capacity: s.rate }); return { s, index }; });
    const terminals = n.terminals.filter(t => t.endpoint.kind === 'facility' && own.has(t.endpoint.instance)).map(t => {
      const index = arcs.length, output = t.direction === 'output';
      arcs.push({ from: output ? `supply:${t.endpoint.instance}` : vertex(t.position), to: output ? vertex(t.position) : `demand:${t.endpoint.instance}`, capacity: t.rate });
      return { t, index };
    });
    const flow = boundedFlow(arcs, 'source', 'sink'), expected = dto([...sources.values()].reduce(add, rate(0)));
    if (BigInt(flow.total.numerator) * BigInt(expected.denominator) !== BigInt(expected.numerator) * BigInt(flow.total.denominator))
      fail('scc-retained-flow-insufficient', 'Existing routes cannot realize the exact retained source and sink margins', { item: n.item, expected, actual: flow.total });
    const positive = r => r.numerator > 0;
    const ss = segments.filter(e => positive(flow.flows[e.index])).map(e => ({ ...e.s, rate: flow.flows[e.index] }));
    const ts = terminals.filter(e => positive(flow.flows[e.index])).map(e => ({ ...e.t, rate: flow.flows[e.index] }));
    const cells = new Map([...ss.flatMap(s => [s.from, s.to]), ...ts.map(t => t.position)].map(p => [key(p), p]));
    const cs = w.logistics_components.filter(c => n.component_ids.includes(c.id) && cells.has(key(c.position)) &&
      (ss.filter(s => key(s.from) === key(c.position)).length > 1 || ss.filter(s => key(s.to) === key(c.position)).length > 1));
    cs.forEach(c => retainedComponents.add(c.id));
    networks.push({ ...n, cells: [...cells.values()], segments: ss, terminals: ts, component_ids: cs.map(c => c.id), requirement_ids: requirements.map(e => e.id) });
    evidence.push({ item: n.item, expected, actual: flow.total, augmentations: flow.augmentations, before_cells: n.cells.length, after_cells: cells.size });
  }
  const witness = structuredClone({ ...w, placements: w.placements.filter(p => own.has(p.instance)),
    utilizations: Object.fromEntries(members.map(id => [id, w.utilizations[id]])), transport_networks: networks,
    logistics_components: w.logistics_components.filter(c => retainedComponents.has(c.id)) });
  const points = [...witness.placements, ...networks.flatMap(n => n.cells)], dx = Math.min(...points.map(p => p.x)), dy = Math.min(...points.map(p => p.y));
  const move = p => ({ ...p, x: p.x - dx, y: p.y - dy });
  witness.placements = witness.placements.map(move); witness.logistics_components = witness.logistics_components.map(c => ({ ...c, position: move(c.position) }));
  witness.transport_networks = witness.transport_networks.map(n => ({ ...n, cells: n.cells.map(move), segments: n.segments.map(s => ({ ...s, from: move(s.from), to: move(s.to) })), terminals: n.terminals.map(t => ({ ...t, position: move(t.position) })) }));
  return { schema_version: 1, case: 'actual-graph-cycle-block', embedding: 'scc-flow-projection', accepted: false, factory_validated: false,
    witness, catalog_ports: project(witness, catalogs), labels: parent.labels, port_groups: { sources: [], targets: [] },
    block: { id, cycle: [...members], members: [...members], pending_edges: structuredClone(cuts) }, projection: evidence };
}
