import { placeRegions } from '../constructive-block-composition/model.mjs';
import { project } from '../constructive-factory-closure/model.mjs';
import { capacity, lanesFor, parseRate } from '../constructive-interconnect/rates.mjs';
const fail = (code, message) => { throw Object.assign(new Error(message), { code, severity: 'error', stage: 'cluster-nesting' }); };

// Exact occupied-cell rows for a fixed concrete region, not its rectangular envelope.
export function footprint(w) {
  if (w.placements.some(p => !Number.isSafeInteger(p.width) || !Number.isSafeInteger(p.height) || p.width < 1 || p.height < 1)) fail('nesting-footprint-schema', 'Expected positive integer facility dimensions');
  if ([...w.transport_networks, ...w.logistics_components].some(n => !['belt', 'pipe'].includes(n.transport))) fail('nesting-layer-schema', 'Expected belt or pipe occupied layers');
  const cells = [...w.placements.flatMap(p => [p, { x: p.x + p.width - 1, y: p.y + p.height - 1 }]), ...w.transport_networks.flatMap(n => n.cells), ...w.logistics_components.map(c => c.position)];
  if (!cells.length || cells.some(p => !Number.isSafeInteger(p.x) || !Number.isSafeInteger(p.y))) fail('nesting-coordinate-schema', 'Expected nonempty integer occupied geometry');
  const x = Math.min(...cells.map(p => p.x)), y = Math.min(...cells.map(p => p.y));
  const width = Math.max(...cells.map(p => p.x)) - x + 1, height = Math.max(...cells.map(p => p.y)) - y + 1;
  if (width > 1024 || height > 1024) fail('nesting-row-budget', 'Fixed-region row bitset diagnostic extent exceeds 1024; not a game limit');
  const rows = Array.from({ length: height }, () => ({ body: 0n, belt: 0n, pipe: 0n }));
  const put = (p, layer) => { rows[p.y - y][layer] |= 1n << BigInt(p.x - x); };
  for (const p of w.placements) for (let dy = 0; dy < p.height; dy++) for (let dx = 0; dx < p.width; dx++) put({ x: p.x + dx, y: p.y + dy }, 'body');
  for (const n of w.transport_networks) for (const p of n.cells) put(p, n.transport);
  for (const c of w.logistics_components) put(c.position, c.transport);
  return { x, y, width, height, rows };
}

export function collides(a, b, dx, dy) {
  if (!Number.isSafeInteger(dx) || !Number.isSafeInteger(dy)) fail('nesting-offset-schema', 'Expected integer translations');
  if (dx >= a.width || dx + b.width <= 0 || dy >= a.height || dy + b.height <= 0) return false;
  const shiftA = BigInt(Math.max(0, -dx)), shiftB = BigInt(Math.max(0, dx));
  for (let y = Math.max(0, dy); y < Math.min(a.height, dy + b.height); y++) {
    const ar = a.rows[y], br = b.rows[y - dy];
    const ab = ar.body << shiftA, al = ar.belt << shiftA, ap = ar.pipe << shiftA;
    const bb = br.body << shiftB, bl = br.belt << shiftB, bp = br.pipe << shiftB;
    if ((ab & (bb | bl | bp)) || (bb & (al | ap)) || (al & bl) || (ap & bp)) return true;
  }
  return false;
}

export function blockedAt(f, x, y, transport) {
  if (!Number.isSafeInteger(x) || !Number.isSafeInteger(y) || !['belt', 'pipe'].includes(transport)) fail('nesting-access-schema', 'Expected integer access cell and belt or pipe layer');
  if (x < 0 || y < 0 || x >= f.width || y >= f.height) return false;
  return Boolean((f.rows[y].body | f.rows[y][transport]) & (1n << BigInt(x)));
}

// Necessary access for the current fixed-interior, unused-port, separate-edge closure.
// This is not an impossibility proof for trunk sharing, port reassignment or rip-up.
export function pendingPorts(graph, region, catalogs) {
  const w = region.detail.witness, f = footprint(w), members = new Set(w.placements.map(p => p.instance));
  const projected = project(w, catalogs), used = new Set(w.transport_networks.flatMap(n => n.terminals)
    .filter(t => t.endpoint.kind === 'facility').map(t => JSON.stringify([t.endpoint.instance, t.endpoint.port])));
  const groups = new Map();
  for (const e of graph.edges.filter(e => members.has(e.source) && !members.has(e.target))) {
    const id = JSON.stringify([e.source, e.transport]);
    if (!groups.has(id)) groups.set(id, { instance: e.source, transport: e.transport, required: 0 });
    groups.get(id).required += lanesFor(parseRate(e.rate), capacity(catalogs.transports.transports.find(t => t.kind === e.transport).capacity));
  }
  return [...groups.values()].map(g => ({ ...g, ports: projected[g.instance]
    .filter(p => p.direction === 'output' && p.transport === g.transport && !used.has(JSON.stringify([g.instance, p.id])))
    .map(p => ({ id: p.id, x: p.connection.x - f.x, y: p.connection.y - f.y }))
    .filter(p => !blockedAt(f, p.x, p.y, g.transport)) }));
}

export function accessFailures(requirements, other, dx, dy) {
  return requirements.flatMap(g => {
    const available = g.ports.filter(p => !blockedAt(other, p.x - dx, p.y - dy, g.transport)).length;
    return available < g.required ? [{ code: 'nesting-pending-output-obstructed', severity: 'info', stage: 'cluster-nesting',
      message: 'Insufficient free unused output connections for the fixed-interior closure strategy',
      instance: g.instance, transport: g.transport, required: g.required, available }] : [];
  });
}

export function nestingCandidates(graph, clusters, catalogs, limit = 12) {
  if (clusters.length !== 2) fail('nesting-cluster-count', 'This diagnostic enumerates translations of exactly two fixed clusters');
  if (!Number.isSafeInteger(limit) || limit < 1 || limit > 32) fail('nesting-candidate-budget', 'Expected candidate limit 1..32');
  const start = performance.now(), regions = [...clusters].sort((a, b) => a.id.localeCompare(b.id));
  const [a, b] = regions.map(r => footprint(r.detail.witness)), valid = [], rejected = [];
  const [ar, br] = regions.map(r => pendingPorts(graph, r, catalogs)); let tested = 0, collisionFree = 0, minimumCollisionArea = null;
  // A finite contact neighborhood chosen from measured dimensions, not a factory canvas bound.
  for (let dy = -b.height - 1; dy <= a.height + 1; dy++) for (let dx = -b.width - 1; dx <= a.width + 1; dx++) {
    tested++; if (collides(a, b, dx, dy)) continue;
    const width = Math.max(a.width, dx + b.width) - Math.min(0, dx), height = Math.max(a.height, dy + b.height) - Math.min(0, dy);
    collisionFree++; minimumCollisionArea = Math.min(minimumCollisionArea ?? Infinity, width * height);
    const failures = [...accessFailures(ar, b, dx, dy), ...accessFailures(br, a, -dx, -dy)];
    if (failures.length) { rejected.push({ dx, dy, area: width * height, failures }); continue; }
    valid.push({ dx, dy, width, height, area: width * height });
  }
  valid.sort((a, b) => a.area - b.area || Math.max(a.width, a.height) - Math.max(b.width, b.height) || a.dy - b.dy || a.dx - b.dx);
  const selected = [], separation = Math.max(2, Math.floor(Math.min(a.width, a.height, b.width, b.height) / 8));
  for (const p of valid) {
    if (selected.some(q => Math.abs(p.dx - q.dx) + Math.abs(p.dy - q.dy) < separation)) continue;
    selected.push(p); if (selected.length === limit) break;
  }
  const summary = { tested_translations: tested, collision_free: collisionFree, output_accessible: valid.length, output_obstructed: rejected.length,
    minimum_collision_free_partial_area: minimumCollisionArea,
    lowest_area_obstructions: rejected.sort((a, b) => a.area - b.area || a.dy - b.dy || a.dx - b.dx).slice(0, 4),
    selected: selected.length, candidate_limit: limit, offset_separation: separation,
    minimum_sampled_partial_area: valid[0]?.area ?? null, enumeration_ms: performance.now() - start,
    domain: { min_dx: -b.width - 1, max_dx: a.width + 1, min_dy: -b.height - 1, max_dy: a.height + 1 } };
  return selected.map((p, i) => {
    const x = -Math.min(0, p.dx), y = -Math.min(0, p.dy), translations = [
      { region: regions[0].id, x: x - a.x, y: y - a.y }, { region: regions[1].id, x: x + p.dx - b.x, y: y + p.dy - b.y }];
    const placed = placeRegions(regions, translations, catalogs);
    return { schema_version: 1, case: 'actual-graph-cluster-packing', embedding: `nested-${i}`, witness: placed.witness, catalog_ports: placed.ports,
      labels: Object.fromEntries(graph.nodes.map(n => [n.id, n.label])), port_groups: { sources: [], targets: [] }, accepted: false, factory_validated: false,
      packing_plan: { family: 'occupied-cell-nesting', translations, offset: p, search: summary } };
  });
}
