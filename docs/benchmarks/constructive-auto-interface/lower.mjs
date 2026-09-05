import { rate, add, multiply, compare, parseRate, dto, capacity } from '../constructive-interconnect/rates.mjs';
import { polyline } from '../constructive-interconnect/geometry.mjs';
const key = p => `${p.x},${p.y}`, pair = p => [p.x, p.y];
const delta = { north: [0, -1], east: [1, 0], south: [0, 1], west: [-1, 0] };

// Lower commodity paths independently of how they share ports and trunks.
export function lowerInterface(catalogs, nodes, plan) {
  const witness = { schema_version: 1, placements: [], utilizations: Object.create(null), transport_networks: [], logistics_components: [] };
  const ports = Object.create(null), networks = new Map(), usedPorts = new Set();
  const network = item => {
    if (!networks.has(item)) networks.set(item, { id: `network:${item}`, item, transport: catalogs.items.items.find(i => i.id === item).transport,
      requirement_ids: [], cells: new Map(), segments: new Map(), terminals: [], component_ids: [] });
    return networks.get(item);
  };
  for (const n of nodes.values()) {
    witness.placements.push({ instance: n.id, recipe: n.recipe.id, facility: n.recipe.facility, x: n.x, y: n.y,
      width: n.shape.width, height: n.shape.height, rotation: n.shape.rotation });
    witness.utilizations[n.id] = n.utilization;
    ports[n.id] = n.shape.ports.map(p => ({ ...p, position: { x: n.x + p.position.x, y: n.y + p.position.y },
      connection: { x: n.x + p.position.x + delta[p.edge][0], y: n.y + p.position.y + delta[p.edge][1] } }));
  }
  const terminal = (node, portId, item, value) => {
    const p = ports[node].find(p => p.id === portId), id = JSON.stringify([node, portId]), n = network(item);
    if (!p || p.transport !== n.transport || usedPorts.has(id)) throw new Error(`Missing, incompatible or reused port ${id}`);
    usedPorts.add(id); n.terminals.push({ id, node, direction: p.direction, endpoint: { kind: 'facility', instance: node, port: portId }, position: p.connection, rate: dto(value) });
    return p.connection;
  };
  const path = (item, value, cells) => {
    const n = network(item);
    for (const p of cells) n.cells.set(key(p), p);
    for (let i = 1; i < cells.length; i++) {
      const from = cells[i - 1], to = cells[i], id = key(from) + '>' + key(to), old = n.segments.get(id);
      if (Math.abs(from.x - to.x) + Math.abs(from.y - to.y) !== 1) throw new Error('Non-unit lowered path');
      n.segments.set(id, { from, to, rate: dto(add(old ? parseRate(old.rate) : rate(0), value)) });
    }
  };
  for (const g of [...plan.sourceGroups, ...plan.targetGroups]) terminal(g.node, g.port, g.item, g.value);
  for (const l of plan.lanes) { network(l.item).requirement_ids.push(l.id); path(l.item, parseRate(l.rate), plan.paths.get(l.id)); }
  for (const { items, ...c } of plan.components) {
    witness.logistics_components.push(c);
    for (const item of items) network(item).component_ids.push(c.id);
  }
  for (const n of nodes.values()) for (const direction of ['input', 'output']) for (const a of n.recipe[direction + 's']) {
    const net = network(a.item), required = multiply(rate(BigInt(a.quantity) * 1000n, n.recipe.duration_ms), parseRate(n.utilization));
    const used = net.terminals.filter(t => t.node === n.id && t.direction === direction).reduce((s, t) => add(s, parseRate(t.rate)), rate(0));
    let remaining = rate(required.n * used.d - used.n * required.d, required.d * used.d);
    const cap = capacity(catalogs.transports.transports.find(t => t.kind === net.transport).capacity);
    while (remaining.n) {
      const value = compare(remaining, cap) < 0 ? remaining : cap;
      const p = ports[n.id].find(p => p.direction === direction && p.transport === net.transport && p.edge === (direction === 'input' ? 'west' : 'east') && !usedPorts.has(JSON.stringify([n.id, p.id])));
      if (!p) throw Object.assign(new Error(`No external capacity port at ${n.id} for ${a.item}`), { code: 'interface-external-port-pressure', entity: n.id });
      const connection = terminal(n.id, p.id, a.item, value), outside = { x: direction === 'input' ? 0 : plan.externalEast, y: connection.y }, id = `external:${JSON.stringify([n.id, p.id])}`;
      net.requirement_ids.push(id); net.terminals.push({ id, node: id, direction: direction === 'input' ? 'output' : 'input',
        endpoint: { kind: 'external', node: id, side: direction === 'input' ? 'west' : 'east' }, position: outside, rate: dto(value) });
      path(a.item, value, polyline((direction === 'input' ? [outside, connection] : [connection, outside]).map(pair)));
      remaining = rate(remaining.n * value.d - value.n * remaining.d, remaining.d * value.d);
    }
  }
  witness.transport_networks = [...networks.values()].filter(n => n.cells.size).map(n => ({ ...n, cells: [...n.cells.values()], segments: [...n.segments.values()] }));
  return { witness, catalog_ports: ports };
}
