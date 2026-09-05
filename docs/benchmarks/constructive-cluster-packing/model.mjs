import { readyConsumers, placeRegions } from '../constructive-block-composition/model.mjs';
import { witnessScore } from '../constructive-auto-interface/compact.mjs';
const fail = (code, message, entity = null) => { throw Object.assign(new Error(message), { code, severity: 'error', stage: 'cluster-packing', entity }); };

export function discoverClusters(graph, regions, catalogs) {
  const owner = new Map();
  for (const r of regions) for (const p of r.detail.witness.placements) {
    if (owner.has(p.instance)) fail('cluster-duplicate-owner', 'Source facility belongs to multiple regions', p.instance);
    owner.set(p.instance, r.id);
  }
  if (new Set(regions.map(r => r.id)).size !== regions.length) fail('cluster-duplicate-region', 'Region IDs must be unique');
  const parent = new Map(regions.map(r => [r.id, r.id]));
  const root = id => { while (parent.get(id) !== id) id = parent.get(id); return id; };
  const targets = readyConsumers(graph, [...owner.keys()], catalogs);
  for (const id of targets) {
    const suppliers = [...new Set(graph.edges.filter(e => e.target === id).map(e => owner.get(e.source)))].sort();
    for (const r of suppliers.slice(1)) parent.set(root(r), root(suppliers[0]));
  }
  const groups = new Map();
  for (const r of [...regions].sort((a, b) => a.id.localeCompare(b.id))) {
    const id = root(r.id); if (!groups.has(id)) groups.set(id, []); groups.get(id).push(r);
  }
  return [...groups.values()].map((rs, i) => ({ id: `cluster-${i}`, regions: rs,
    frontier: readyConsumers(graph, rs.flatMap(r => r.detail.witness.placements.map(p => p.instance)), catalogs) }));
}

export function packingCandidates(graph, clusters, catalogs) {
  if (!clusters.length || clusters.length > 8) fail('cluster-packing-count', 'Expected 1..8 concrete clusters for bounded shelf exploration');
  const sizes = new Map(clusters.map(c => [c.id, witnessScore(c.detail.witness)]));
  // Bounded deterministic orders: canonical/reverse and largest-first by each dimension.
  const orders = [clusters.map(c => c.id).sort(), clusters.map(c => c.id).sort().reverse(),
    ...['width', 'height', 'area'].map(field => [...clusters].sort((a, b) => sizes.get(b.id)[field] - sizes.get(a.id)[field] || a.id.localeCompare(b.id)).map(c => c.id))];
  const unique = new Set(), candidates = [];
  for (const order of orders) for (const axis of ['x', 'y']) for (const align of ['start', 'end']) {
    const cross = axis === 'x' ? 'y' : 'x', span = axis === 'x' ? 'width' : 'height', breadth = axis === 'x' ? 'height' : 'width';
    const maximum = Math.max(...[...sizes.values()].map(s => s[breadth])); let cursor = 0;
    const offsets = order.map(id => {
      const w = clusters.find(c => c.id === id).detail.witness, cells = [...w.placements, ...w.transport_networks.flatMap(n => n.cells)];
      const origin = { x: Math.min(...cells.map(p => p.x)), y: Math.min(...cells.map(p => p.y)) }, size = sizes.get(id);
      const offset = { region: id, [axis]: cursor - origin[axis], [cross]: (align === 'end' ? maximum - size[breadth] : 0) - origin[cross] };
      cursor += size[span] + 1; return offset;
    });
    const signature = JSON.stringify([...offsets].sort((a, b) => a.region.localeCompare(b.region)));
    if (unique.has(signature)) continue; unique.add(signature);
    const placed = placeRegions(clusters, offsets, catalogs), members = placed.witness.placements.map(p => p.instance), set = new Set(members);
    const nodes = graph.nodes.filter(n => set.has(n.id)).map(({ label, ...n }) => n);
    const detail = { schema_version: 1, case: 'actual-graph-cluster-packing', embedding: `shelf-${axis}-${align}-${candidates.length}`,
      witness: placed.witness, catalog_ports: placed.ports, labels: Object.fromEntries(graph.nodes.map(n => [n.id, n.label])),
      port_groups: { sources: [], targets: [] }, accepted: false, factory_validated: false,
      block: { id: 'packed-clusters', members, request: { schema_version: 1, producers: nodes, consumers: [] },
        pending_edges: graph.edges.filter(e => set.has(e.source) !== set.has(e.target)) },
      packing_plan: { axis, align, order, translations: offsets, clusters: clusters.map(c => ({ id: c.id, members: c.detail.witness.placements.map(p => p.instance), score: sizes.get(c.id) })) } };
    candidates.push(detail);
  }
  return candidates;
}
