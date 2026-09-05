import { rotateFacility } from '../constructive-placement-only/model.mjs';
const key = p => `${p.x},${p.y}`;
const equalRate = (a, b) => BigInt(a.numerator) * BigInt(b.denominator) === BigInt(b.numerator) * BigInt(a.denominator);
const delta = { north: [0, -1], east: [1, 0], south: [0, 1], west: [-1, 0] };

// Remove only event-free grid strips: no body, component, terminal, turn,
// branch, merge or transverse segment. Topology and every edge rate stay fixed.
function contractAxis(witness, axis) {
  const cross = axis === 'x' ? 'y' : 'x', extent = axis === 'x' ? 'width' : 'height', critical = new Set();
  const protect = p => critical.add(p[axis]);
  for (const p of witness.placements) for (let i = 0; i < p[extent]; i++) critical.add(p[axis] + i);
  for (const c of witness.logistics_components) protect(c.position);
  const indexes = witness.transport_networks.map(n => {
    const incoming = new Map(), outgoing = new Map();
    for (const s of n.segments) {
      for (const [map, p] of [[incoming, s.to], [outgoing, s.from]]) { const id = key(p), list = map.get(id) ?? []; list.push(s); map.set(id, list); }
    }
    for (const t of n.terminals) protect(t.position);
    for (const p of n.cells) {
      const ins = incoming.get(key(p)) ?? [], outs = outgoing.get(key(p)) ?? [];
      if (ins.length !== 1 || outs.length !== 1 || ins[0].from[cross] !== p[cross] || outs[0].to[cross] !== p[cross] ||
          Math.sign(p[axis] - ins[0].from[axis]) !== Math.sign(outs[0].to[axis] - p[axis]) || !equalRate(ins[0].rate, outs[0].rate)) protect(p);
    }
    return { incoming, outgoing };
  });
  let maximum = -1;
  for (const v of critical) maximum = Math.max(maximum, v);
  const removed = [];
  const translated = new Map(); let next = 0;
  for (let v = 0; v <= maximum; v++) { translated.set(v, next); if (critical.has(v)) next++; else removed.push(v); }
  if (!removed.length) return { witness, removed, translate: v => v };
  const move = p => ({ ...p, [axis]: translated.get(p[axis]) });
  const networks = witness.transport_networks.map((n, i) => {
    const segments = [];
    for (const s of n.segments) if (critical.has(s.from[axis])) {
      let end = s.to, steps = 0;
      while (!critical.has(end[axis])) {
        const successors = indexes[i].outgoing.get(key(end));
        if (successors?.length !== 1 || !equalRate(s.rate, successors[0].rate) || ++steps > maximum + 1) throw new Error('Invalid contracted straight chain');
        end = successors[0].to;
      }
      const from = move(s.from), to = move(end);
      if (Math.abs(from.x - to.x) + Math.abs(from.y - to.y) !== 1) throw new Error('Compaction produced a non-unit edge');
      segments.push({ from, to, rate: s.rate });
    }
    return { ...n, cells: n.cells.filter(p => critical.has(p[axis])).map(move), segments,
      terminals: n.terminals.map(t => ({ ...t, position: move(t.position) })) };
  });
  return { removed, translate: v => translated.get(v), witness: { ...witness, placements: witness.placements.map(move),
    logistics_components: witness.logistics_components.map(c => ({ ...c, position: move(c.position) })), transport_networks: networks } };
}

export function compactInterface(detail, catalogs) {
  const start = performance.now(), x = contractAxis(detail.witness, 'x'), y = contractAxis(x.witness, 'y');
  const ports = Object.create(null);
  for (const p of y.witness.placements) {
    const shape = rotateFacility(catalogs.facilities.facilities.find(f => f.id === p.facility), p.rotation);
    ports[p.instance] = shape.ports.map(q => ({ ...q, position: { x: p.x + q.position.x, y: p.y + q.position.y },
      connection: { x: p.x + q.position.x + delta[q.edge][0], y: p.y + q.position.y + delta[q.edge][1] } }));
  }
  const { tree_modules, trunk_plan, permutation, permutation_origin, ...base } = detail;
  return { ...base, embedding: detail.embedding + '-compacted', witness: y.witness, catalog_ports: ports,
    port_groups: Object.fromEntries(Object.entries(detail.port_groups).map(([side, gs]) => [side, gs.map(({ row, root_row, ...g }) => ({ ...g,
      port_y: y.translate(g.port_y) }))])),
    pre_compaction_plan: { tree_modules, trunk_plan, permutation, permutation_origin, port_groups: detail.port_groups },
    compaction: { removed_columns: x.removed, removed_rows: y.removed, elapsed_ms: performance.now() - start,
      scope: 'event-free whole-grid strip contraction; not global minimum area' }, accepted: false };
}

export function witnessScore(w) {
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
  const include = p => { minX = Math.min(minX, p.x); minY = Math.min(minY, p.y); maxX = Math.max(maxX, p.x); maxY = Math.max(maxY, p.y); };
  for (const p of w.placements) { include(p); include({ x: p.x + p.width - 1, y: p.y + p.height - 1 }); }
  for (const n of w.transport_networks) for (const p of n.cells) include(p);
  if (!Number.isFinite(minX)) throw new Error('Cannot score empty geometry');
  const width = maxX - minX + 1, height = maxY - minY + 1;
  const tiles = new Set(w.transport_networks.flatMap(n => n.cells.map(p => `${n.transport}:${key(p)}`))).size;
  let turns = 0;
  for (const n of w.transport_networks) {
    const ins = new Map(), outs = new Map();
    for (const s of n.segments) for (const [map, p] of [[ins, s.to], [outs, s.from]]) { const list = map.get(key(p)) ?? []; list.push(s); map.set(key(p), list); }
    for (const p of n.cells) for (const a of ins.get(key(p)) ?? []) for (const b of outs.get(key(p)) ?? [])
      if ((p.x - a.from.x) * (b.to.x - p.x) + (p.y - a.from.y) * (b.to.y - p.y) === 0) turns++;
  }
  return { width, height, area: width * height, physical_tiles: tiles, turns, tuple: [width * height, tiles, turns] };
}
