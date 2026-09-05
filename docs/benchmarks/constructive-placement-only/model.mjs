// Offline geometry proposal only. Never imports or invokes the production constructor or router.
const directions = ['north', 'east', 'south', 'west'];
const delta = { north: [0, -1], east: [1, 0], south: [0, 1], west: [-1, 0] };
const integer = n => Number.isSafeInteger(n);
function fail(code, entity, message) {
  throw Object.assign(new Error(message), { code, entity, stage: 'placement-only' });
}
const unique = (values, entity) => {
  if (new Set(values).size !== values.length) fail('placement-probe-duplicate-id', entity, 'Duplicate IDs');
};
const overlapsWithGap = (a, b) => a.x < b.x + b.width + 1 && a.x + a.width + 1 > b.x
  && a.y < b.y + b.height + 1 && a.y + a.height + 1 > b.y;

export function rotateFacility(spec, rotation) {
  let width = spec.footprint.width, height = spec.footprint.height;
  let ports = structuredClone(spec.ports);
  for (let r = 0; r < rotation; r += 90) {
    ports = ports.map(p => ({ ...p, position: { x: height - 1 - p.position.y, y: p.position.x },
      edge: directions[(directions.indexOf(p.edge) + 1) % 4] }));
    [width, height] = [height, width];
  }
  return { width, height, rotation, ports };
}

function orient(spec, groups) {
  const candidates = spec.allowed_rotations.map(rotation => {
    const shape = rotateFacility(spec, rotation);
    const counts = {};
    for (const kind of ['belt', 'pipe']) for (const direction of ['output', 'input']) {
      const required = groups.filter(g => g.transport === kind && g.direction === direction);
      counts[`${kind}_${direction}`] = { required: required.length,
        aligned: required.filter(g => shape.ports.some(p => p.transport === g.transport
          && p.direction === g.direction && p.edge === (direction === 'output' ? 'east' : 'west'))).length };
    }
    return { ...shape, alignment: counts };
  });
  const keys = ['belt_output', 'belt_input', 'pipe_output', 'pipe_input'];
  candidates.sort((a, b) => {
    for (const key of keys) if (a.alignment[key].aligned !== b.alignment[key].aligned)
      return b.alignment[key].aligned - a.alignment[key].aligned;
    return a.rotation - b.rotation;
  });
  return candidates[0];
}

export function placeOnly(wiring, catalog, items, target) {
  if (wiring.schema_version !== 1 || !wiring.success || catalog.schema_version !== 3 || items.schema_version !== 1)
    fail('placement-probe-schema-invalid', target, 'Expected successful wiring v1, facilities v3 and items v1');
  unique(wiring.nodes.map(n => n.id), 'wiring.nodes');
  unique(wiring.edges.map(e => e.id), 'wiring.edges');
  unique(catalog.facilities.map(f => f.id), 'facilities');
  unique(items.items.map(i => i.id), 'items');
  const allIds = new Set(wiring.nodes.map(n => n.id));
  const facilities = wiring.nodes.filter(n => n.kind === 'facility');
  const byId = new Map(facilities.map(n => [n.id, n]));
  const byItem = new Map(items.items.map(i => [i.id, i]));
  const bySpec = new Map(catalog.facilities.map(f => [f.id, f]));
  if (!byId.has(target)) fail('placement-probe-target-invalid', target, 'Target must be a required facility instance');
  for (const e of wiring.edges) {
    if (!allIds.has(e.source) || !allIds.has(e.target) || !['belt', 'pipe'].includes(byItem.get(e.item)?.transport))
      fail('placement-probe-edge-invalid', e.id, 'Missing endpoint or supported transport item');
  }
  const shapes = new Map();
  for (const n of facilities) {
    const f = bySpec.get(n.facility);
    if (!f || !integer(f.footprint?.width) || !integer(f.footprint?.height)
      || f.footprint.width < 1 || f.footprint.height < 1 || !f.allowed_rotations?.length
      || f.allowed_rotations.some(r => ![0, 90, 180, 270].includes(r)) || !Array.isArray(f.ports))
      fail('placement-probe-facility-invalid', n.facility, 'Expected a catalog footprint and legal quarter-turn rotations');
    unique(f.ports.map(p => p.id), n.facility);
    for (const p of f.ports) {
      const { x, y } = p.position ?? {};
      if (!integer(x) || !integer(y) || x < 0 || y < 0 || x >= f.footprint.width || y >= f.footprint.height
        || !directions.includes(p.edge) || !['input', 'output'].includes(p.direction)
        || !['belt', 'pipe'].includes(p.transport)
        || p.edge === 'north' && y !== 0 || p.edge === 'south' && y !== f.footprint.height - 1
        || p.edge === 'west' && x !== 0 || p.edge === 'east' && x !== f.footprint.width - 1)
        fail('placement-probe-port-invalid', `${n.facility}/${p.id}`, 'Port must be on its declared footprint edge');
    }
    const groups = [...new Map(wiring.edges.filter(e => e.source === n.id || e.target === n.id).flatMap(e => {
      const transport = byItem.get(e.item).transport;
      return [e.source === n.id ? 'output' : null, e.target === n.id ? 'input' : null].filter(Boolean)
        .map(direction => [`${direction}:${transport}:${e.item}`, { direction, transport, item: e.item }]);
    })).values()];
    shapes.set(n.id, orient(f, groups));
  }
  const edges = wiring.edges.filter(e => byId.has(e.source) && byId.has(e.target))
    .map(e => ({ ...e, transport: byItem.get(e.item).transport }));
  const incoming = new Map(facilities.map(n => [n.id, []]));
  for (const e of edges) incoming.get(e.target).push(e.source);
  const pipeEdges = edges.filter(e => e.transport === 'pipe');
  const branchPriority = new Map(facilities.map(n => {
    const ancestors = new Set(), pending = [n.id];
    while (pending.length) {
      const id = pending.pop();
      if (ancestors.has(id)) continue;
      ancestors.add(id);
      pending.push(...incoming.get(id));
    }
    return [n.id, pipeEdges.filter(e => ancestors.has(e.source)).length];
  }));
  const northInput = id => shapes.get(id).ports.some(p => p.transport === 'pipe'
    && p.direction === 'input' && p.edge === 'north');
  const northEdges = pipeEdges.filter(e => northInput(e.target));
  const placed = new Map(), decisions = [], queue = [];
  const place = id => {
    const node = byId.get(id), shape = shapes.get(id);
    const consumers = [...new Set(edges.filter(e => e.source === id && placed.has(e.target)).map(e => e.target))].sort();
    const suppliers = [...new Set(edges.filter(e => e.target === id && placed.has(e.source)).map(e => e.source))].sort();
    const upstream = consumers.length > 0;
    const peers = (upstream ? consumers : suppliers).map(id => placed.get(id));
    const ys = peers.map(p => p.y).sort((a, b) => a - b);
    const row = ys.length ? ys[Math.floor(ys.length / 2)] : 0;
    const x = peers.length ? upstream ? Math.min(...peers.map(p => p.x)) - shape.width - 1
      : Math.max(...peers.map(p => p.x + p.width)) + 1 : 0;
    const p = { instance: id, recipe: node.recipe, facility: node.facility,
      x, y: row, width: shape.width, height: shape.height, rotation: shape.rotation,
      branch_pipe_outputs: branchPriority.get(id) };
    const blockers = [...placed.values()].filter(b => x < b.x + b.width + 1 && x + p.width + 1 > b.x);
    const relations = northEdges.filter(e => e.source === id && placed.has(e.target)
      || e.target === id && placed.has(e.source));
    const anchors = relations.map(e => e.source === id ? placed.get(e.target).y - p.height - 1
      : placed.get(e.source).y + placed.get(e.source).height + 1);
    const events = [...new Set([row, ...anchors,
      ...blockers.flatMap(b => [b.y - p.height - 1, b.y + b.height + 1])])];
    const rankRow = y => ({ y,
      north_pipe_violations: relations.filter(e => e.source === id
        ? y + p.height + 1 > placed.get(e.target).y
        : placed.get(e.source).y + placed.get(e.source).height + 1 > y).length,
      branch_priority_inversions: blockers.filter(b => p.branch_pipe_outputs > b.branch_pipe_outputs
        ? y + p.height + 1 > b.y
        : p.branch_pipe_outputs < b.branch_pipe_outputs && b.y + b.height + 1 > y).length,
      row_distance: Math.abs(y - row) });
    const candidates = events.filter(y => blockers.every(b => !overlapsWithGap({ ...p, y }, b)))
      .map(rankRow).sort((a, b) => a.north_pipe_violations - b.north_pipe_violations
        || a.branch_priority_inversions - b.branch_priority_inversions
        || a.row_distance - b.row_distance || a.y - b.y);
    const chosen = candidates[0], y = chosen?.y;
    if (y === undefined) fail('placement-probe-row-exhausted', id, 'No collision-free obstacle-edge row');
    p.y = y;
    if (![p.x, p.y, p.x + p.width, p.y + p.height].every(integer))
      fail('placement-probe-coordinate-overflow', id, 'Placement coordinate exceeds safe integer range');
    placed.set(id, p);
    decisions.push({ instance: id, consumers, suppliers, side: peers.length ? upstream ? 'upstream' : 'downstream' : 'component-seed',
      preferred_row: row, row_shift: y - row, row_events: events.length, aligned_row: row === y,
      branch_pipe_outputs: p.branch_pipe_outputs, north_pipe_relations: relations.map(e => e.id),
      row_choice: chosen,
      mixed_neighbor_roles: consumers.length > 0 && suppliers.length > 0 });
    queue.push(id);
  };
  const traverse = seed => {
    place(seed);
    for (let head = 0; head < queue.length; head++) {
      const current = queue[head];
      const frontier = edges.filter(e => e.source === current || e.target === current).map(e => ({
        id: e.source === current ? e.target : e.source, upstream: e.target === current,
        transport: e.transport, edge: e.id,
      })).sort((a, b) => branchPriority.get(b.id) - branchPriority.get(a.id)
        || Number(a.transport === 'pipe') - Number(b.transport === 'pipe')
        || Number(!a.upstream) - Number(!b.upstream) || (a.edge < b.edge ? -1 : a.edge > b.edge ? 1 : 0));
      for (const next of frontier) if (!placed.has(next.id)) place(next.id);
    }
  };
  traverse(target);
  for (const id of [...byId.keys()].sort()) if (!placed.has(id)) traverse(id);
  const minX = Math.min(...[...placed.values()].map(p => p.x));
  const minY = Math.min(...[...placed.values()].map(p => p.y));
  const placements = [...placed.values()].map((p, index) => {
    const x = p.x - minX, y = p.y - minY, shape = shapes.get(p.instance);
    return { ...p, x, y, label: `F${String(index).padStart(2, '0')}`, placement_index: index,
      alignment: shape.alignment, catalog_ports: shape.ports.map(port => ({ ...port,
        position: { x: x + port.position.x, y: y + port.position.y },
        connection: { x: x + port.position.x + delta[port.edge][0], y: y + port.position.y + delta[port.edge][1] },
      })) };
  });
  for (let i = 0; i < placements.length; i++) for (let j = i + 1; j < placements.length; j++)
    if (overlapsWithGap(placements[i], placements[j])) fail('placement-probe-overlap', placements[i].instance, 'Facility clearance check failed');
  const pById = new Map(placements.map(p => [p.instance, p]));
  const pipeRelations = pipeEdges.map(e => {
    const a = pById.get(e.source), b = pById.get(e.target);
    return { edge: e.id, source: e.source, target: e.target,
      consumer_input_faces: [...new Set(b.catalog_ports.filter(p => p.transport === 'pipe' && p.direction === 'input').map(p => p.edge))],
      north_preference_applicable: northInput(e.target), supplier_north: a.y + a.height + 1 <= b.y };
  });
  const diagnostics = placements.filter(p => Object.values(p.alignment).some(g => g.aligned < g.required))
    .map(p => ({ code: 'placement-probe-mixed-port-faces', severity: 'warning', stage: 'placement-only',
      entity: p.instance, message: 'Some required interfaces do not face the global belt-flow direction; no port assignment or routing has been tested.' }));
  for (const r of pipeRelations) if (r.north_preference_applicable && !r.supplier_north)
    diagnostics.push({ code: 'placement-probe-north-pipe-preference-unmet', severity: 'warning',
      stage: 'placement-only', entity: r.edge, message: 'Supplier is not north of this north-input consumer. Soft placement preference only; routing is untested.' });
  return { schema_version: 1, kind: 'placement-only-visual-diagnostic', success: true,
    routing_performed: false, factory_validated: false, placement_geometry_valid: true,
    policy: { flow_direction: 'east', orientation_priority: 'belt-before-pipe', traversal: 'output-first-breadth-first-pipe-branch-priority',
      branch_priority: 'distinct-internal-pipe-outputs-in-upstream-closure',
      row_priority: ['north-input-pipe-supply', 'pipe-branch-north-order', 'top-edge-distance', 'north-tie-break'],
      row_alignment: 'top-edge-unless-pipe-preference-or-collision', empty_cell_gap: 1, port_assignment: 'none' },
    target_instance: target, required_facilities: facilities.length, placements,
    bounds: { width: Math.max(...placements.map(p => p.x + p.width)), height: Math.max(...placements.map(p => p.y + p.height)) },
    placement_decisions: decisions.map(d => ({ ...d, preferred_row: d.preferred_row - minY,
      row_choice: { ...d.row_choice, y: d.row_choice.y - minY } })),
    pipe_relations: pipeRelations,
    internal_requirements: edges, external_requirements: wiring.edges.filter(e => !byId.has(e.source) || !byId.has(e.target)),
    non_forward_requirements: edges.filter(e => {
      const a = pById.get(e.source), b = pById.get(e.target);
      return a.x + a.width > b.x;
    }).map(e => e.id),
    transport_networks: [], logistics_components: [], diagnostics };
}
