import { rotateFacility } from '../constructive-placement-only/model.mjs';
import { project } from '../constructive-factory-closure/model.mjs';
import { bodyCells } from '../constructive-local-reembedding/movement.mjs';
import { key, directions } from '../constructive-local-reembedding/path.mjs';
import { capacity, compare, parseRate } from '../constructive-interconnect/rates.mjs';

export function directAttachments(catalogs, graph, core, target) {
  const incoming = graph.edges.filter(e => e.target === target);
  if (incoming.length !== 1 || !incoming[0].internal || incoming[0].transport !== 'belt') return [];
  const edge = incoming[0], original = core.witness;
  if (!original.placements.some(p => p.instance === edge.source) || original.placements.some(p => p.instance === target)) return [];
  if (compare(parseRate(edge.rate), capacity(catalogs.transports.transports.find(t => t.kind === 'belt').capacity)) > 0) return [];
  const node = graph.nodes.find(n => n.id === target), recipe = catalogs.recipes.recipes.find(r => r.id === node.recipe), facility = catalogs.facilities.facilities.find(f => f.id === recipe.facility);
  const ports = project(original, catalogs), used = new Set(original.transport_networks.flatMap(n => n.terminals.filter(t => t.endpoint.kind === 'facility').map(t => JSON.stringify([t.endpoint.instance, t.endpoint.port]))));
  const body = new Set(original.placements.flatMap(bodyCells).map(key)), all = new Set([...body, ...original.transport_networks.flatMap(n => n.cells.map(key)), ...original.logistics_components.map(c => key(c.position))]);
  const belt = new Set(original.transport_networks.filter(n => n.transport === 'belt').flatMap(n => n.cells.map(key)));
  original.logistics_components.filter(c => c.transport === 'belt').forEach(c => belt.add(key(c.position)));
  const candidates = [], seen = new Set();
  for (const rotation of facility.allowed_rotations) {
    const shape = rotateFacility(facility, rotation);
    if (!shape.ports.some(p => p.direction === 'output' && p.transport === 'belt' && p.edge === 'east')) continue;
    for (const a of ports[edge.source].filter(p => p.direction === 'output' && p.transport === 'belt' && !used.has(JSON.stringify([edge.source, p.id]))))
      for (const b of shape.ports.filter(p => p.direction === 'input' && p.transport === 'belt' && p.edge === 'west')) for (const [dx, dy] of [[0, 0], ...directions]) {
        const connection = { x: a.connection.x + dx, y: a.connection.y + dy };
        const p = { instance: target, recipe: recipe.id, facility: facility.id, rotation, width: shape.width, height: shape.height,
          x: connection.x - b.position.x + 1, y: connection.y - b.position.y };
        const signature = JSON.stringify([rotation, p.x, p.y, a.id, b.id]); if (seen.has(signature)) continue; seen.add(signature);
        const newBody = new Set(bodyCells(p).map(key));
        if ([...newBody].some(k => all.has(k)) || [a.connection, connection].some(q => body.has(key(q)) || newBody.has(key(q)) || belt.has(key(q)))) continue;
        const projected = project({ placements: [p] }, catalogs)[target];
        const occupied = new Set([...body, ...newBody, ...belt, key(a.connection), key(connection)]);
        const rowRight = new Map(); for (const k of occupied) { const [x, y] = k.split(',').map(Number); rowRight.set(y, Math.max(rowRight.get(y) ?? -Infinity, x)); }
        if (!projected.some(q => q.direction === 'output' && q.transport === 'belt' && q.edge === 'east' && (rowRight.get(q.connection.y) ?? -Infinity) < q.connection.x)) continue;
        const w = structuredClone(original); w.placements.push(p); w.utilizations[target] = structuredClone(node.utilization);
        let n = w.transport_networks.find(n => n.item === edge.item && n.transport === edge.transport);
        if (!n) { n = { id: JSON.stringify(['direct', edge.item]), item: edge.item, transport: edge.transport, cells: [], segments: [], terminals: [], requirement_ids: [], component_ids: [] }; w.transport_networks.push(n); }
        n.requirement_ids.push(edge.id); n.cells.push(a.connection);
        if (dx || dy) { n.cells.push(connection); n.segments.push({ from: a.connection, to: connection, rate: edge.rate }); }
        for (const [instance, port, position, direction] of [[edge.source, a.id, a.connection, 'output'], [target, b.id, connection, 'input']])
          n.terminals.push({ id: JSON.stringify(['direct', edge.id, direction]), node: instance, endpoint: { kind: 'facility', instance, port }, position, direction, rate: edge.rate });
        const points = [...w.placements.flatMap(bodyCells), ...w.transport_networks.flatMap(n => n.cells), ...w.logistics_components.map(c => c.position)];
        const minX = Math.min(...points.map(p => p.x)), minY = Math.min(...points.map(p => p.y)), move = q => ({ ...q, x: q.x - minX, y: q.y - minY });
        w.placements = w.placements.map(move); w.logistics_components = w.logistics_components.map(c => ({ ...c, position: move(c.position) }));
        w.transport_networks = w.transport_networks.map(n => ({ ...n, cells: n.cells.map(move), segments: n.segments.map(s => ({ ...s, from: move(s.from), to: move(s.to) })), terminals: n.terminals.map(t => ({ ...t, position: move(t.position) })) }));
        const members = w.placements.map(p => p.instance), set = new Set(members);
        candidates.push(structuredClone({ schema_version: 1, case: 'actual-graph-block-composition', embedding: 'direct-port-attachment', accepted: false, factory_validated: false,
          witness: w, labels: core.labels, catalog_ports: project(w, catalogs), port_groups: { sources: [], targets: [] }, reallocation: core.reallocation ?? null,
          block: { id: core.block.id, members, request: { schema_version: 1, producers: graph.nodes.filter(n => set.has(n.id)), consumers: [] },
            pending_edges: graph.edges.filter(e => set.has(e.source) !== set.has(e.target)) },
          direct_attachment: { edge: edge.id, source_port: a.id, target_port: b.id, physical_cells: dx || dy ? 2 : 1 } }));
      }
  }
  return candidates;
}
