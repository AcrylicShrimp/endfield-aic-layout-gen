import { probeSpace } from '../constructive-cluster-packing/pockets.mjs';
const key = p => `${p.x},${p.y}`;

// Necessary footprint fit in a fully explored closed free component. The source
// connection must remain outside the consumer body. Port orientation/path fit is
// deliberately left to the actual router and independent validator.
export function pocketFit(cells, sizes, source, blockedBody = () => false) {
  const free = new Set(cells), points = cells.map(k => { const [x, y] = k.split(',').map(Number); return { x, y }; });
  for (const { width, height } of sizes) for (const origin of points) {
    if (source.x >= origin.x && source.x < origin.x + width && source.y >= origin.y && source.y < origin.y + height) continue;
    let fits = true;
    for (let y = origin.y; fits && y < origin.y + height; y++) for (let x = origin.x; x < origin.x + width; x++)
      if (!free.has(`${x},${y}`) || blockedBody(x, y)) { fits = false; break; }
    if (fits) return { ...origin, width, height };
  }
  return null;
}

export function pendingOutputSpace(graph, w, catalogs, access, occupied, { cellBudget = 4096 } = {}) {
  const members = new Set(w.placements.map(p => p.instance)), points = [...w.placements.map(p => ({ x: p.x + p.width, y: p.y + p.height })),
    ...w.transport_networks.flatMap(n => n.cells), ...w.logistics_components.map(c => c.position)];
  const bounds = { minX: 0, minY: 0, maxX: Math.max(...points.map(p => p.x)) + 1, maxY: Math.max(...points.map(p => p.y)) + 1 };
  const components = new Map(), failures = []; let explored = 0, probes = 0;
  const blockedBody = (x, y) => occupied.body.has(`${x},${y}`) || occupied.layers.belt.has(`${x},${y}`) || occupied.layers.pipe.has(`${x},${y}`);
  for (const g of access) {
    const pending = new Set(g.pending_items.map(i => i.item));
    const demands = [...new Map(graph.edges.filter(e => e.source === g.instance && e.transport === g.transport && pending.has(e.item) && !members.has(e.target))
      .map(e => [e.target, e])).values()];
    for (const e of demands) {
      const node = graph.nodes.find(n => n.id === e.target), facility = node && catalogs.facilities.facilities.find(f =>
        f.id === catalogs.recipes.recipes.find(r => r.id === node.recipe).facility);
      const sizes = facility ? [...new Map(facility.allowed_rotations.map(rotation => {
        const { width, height } = facility.footprint, size = rotation % 180 ? { width: height, height: width } : { width, height };
        return [`${size.width},${size.height}`, size];
      })).values()] : [];
      const pockets = []; let eligible = false;
      for (const p of g.ports.filter(p => !p.obstruction)) {
        const id = `${g.transport}:${key(p.connection)}`;
        if (!components.has(id)) {
          const result = probeSpace(p.connection, (x, y) => occupied.body.has(`${x},${y}`) || occupied.layers[g.transport].has(`${x},${y}`), bounds, cellBudget);
          probes++; explored += result.count;
          components.set(id, result);
          for (const cell of result.cells) components.set(`${g.transport}:${cell}`, result);
        }
        const space = components.get(id);
        const fit = !space.escaped && !space.capped && node ? pocketFit(space.cells, sizes, p.connection, blockedBody) : null;
        pockets.push({ port: p.port, connection: p.connection, count: space.count, escaped: space.escaped, capped: space.capped, fit });
        if (space.escaped || space.capped || fit) { eligible = true; break; }
      }
      if (!eligible) failures.push({ instance: g.instance, transport: g.transport, target: e.target, item: e.item,
        requires_escape: !node, allowed_footprints: sizes, ports: pockets });
    }
  }
  return { failures, probes, explored, cell_budget: cellBudget };
}
