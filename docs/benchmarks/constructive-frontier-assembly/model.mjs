import { footprint } from '../constructive-cluster-packing/nesting.mjs';
import { project } from '../constructive-factory-closure/model.mjs';
import { outputDemands, diverse } from '../constructive-shape-packing/model.mjs';
import { witnessScore } from '../constructive-auto-interface/compact.mjs';
import { discoverClusters } from '../constructive-cluster-packing/model.mjs';
const fail = (code, message, entity = null) => { throw Object.assign(new Error(message), { code, stage: 'frontier-assembly', entity }); };

export function rotateRegion(detail, angle, catalogs) {
  if (![0, 90, 180, 270].includes(angle)) fail('frontier-pose-angle', 'Expected a quarter-turn pose');
  let w = structuredClone(detail.witness);
  if (w.transport_networks.some(n => n.terminals.some(t => t.endpoint.kind !== 'facility'))) fail('frontier-pose-external', 'Rigid source poses require pending rather than routed external boundaries');
  for (const p of w.placements) {
    const f = catalogs.facilities.facilities.find(f => f.id === p.facility);
    if (!f?.allowed_rotations.includes((p.rotation + angle) % 360)) fail('frontier-pose-facility-rotation', 'Unsupported runtime facility orientation', p.instance);
  }
  for (const c of w.logistics_components) {
    const spec = catalogs.components.components.find(s => s.id === c.component);
    if (!spec?.allowed_rotations.includes((c.rotation + angle) % 360)) fail('frontier-pose-component-rotation', 'Unsupported runtime component orientation', c.id);
  }
  // Normalize the input even for the zero-angle alternative.
  const origin = footprint(w), move = p => ({ ...p, x: p.x - origin.x, y: p.y - origin.y });
  w.placements = w.placements.map(move); w.logistics_components = w.logistics_components.map(c => ({ ...c, position: move(c.position) }));
  w.transport_networks = w.transport_networks.map(n => ({ ...n, cells: n.cells.map(move), segments: n.segments.map(s => ({ ...s, from: move(s.from), to: move(s.to) })), terminals: n.terminals.map(t => ({ ...t, position: move(t.position) })) }));
  for (let a = 0; a < angle; a += 90) {
    const { height } = footprint(w), turn = p => ({ ...p, x: height - 1 - p.y, y: p.x });
    w.placements = w.placements.map(p => ({ ...p, x: height - p.y - p.height, y: p.x, width: p.height, height: p.width, rotation: (p.rotation + 90) % 360 }));
    w.logistics_components = w.logistics_components.map(c => ({ ...c, position: turn(c.position), rotation: (c.rotation + 90) % 360 }));
    w.transport_networks = w.transport_networks.map(n => ({ ...n, cells: n.cells.map(turn), segments: n.segments.map(s => ({ ...s, from: turn(s.from), to: turn(s.to) })), terminals: n.terminals.map(t => ({ ...t, position: turn(t.position) })) }));
  }
  return { schema_version: 1, case: 'actual-graph-block-composition', embedding: 'rigid-source-pose', accepted: false, factory_validated: false,
    witness: w, catalog_ports: project(w, catalogs), labels: { ...detail.labels }, block: structuredClone(detail.block), port_groups: { sources: [], targets: [] },
    source_pose: { angle, original_embedding: detail.embedding } };
}

export function eastInterface(graph, detail, catalogs) {
  const demands = outputDemands(graph, { detail }, catalogs);
  return { accepted: demands.every(g => g.ports.length >= g.required), demands };
}

export function sourcePortfolio(graph, alternatives, catalogs, { cut = 'max', limit = 4 } = {}) {
  if (!['min', 'max'].includes(cut) || !Number.isSafeInteger(limit) || limit < 1 || limit > 32) fail('frontier-source-policy', 'Expected min|max cut and limit 1..32');
  const candidates = [], exclusions = [], seen = new Set();
  for (const v of alternatives) for (const angle of [0, 90, 180, 270]) {
    try {
      const detail = rotateRegion(v.detail, angle, catalogs), boundary = eastInterface(graph, detail, catalogs);
      if (!boundary.accepted) { exclusions.push({ directory: v.directory, angle, code: 'frontier-source-east-interface', unavailable: boundary.demands.filter(g => g.ports.length < g.required) }); continue; }
      const signature = JSON.stringify(detail.witness); if (seen.has(signature)) continue; seen.add(signature);
      const score = witnessScore(detail.witness);
      candidates.push({ directory: v.directory, detail, covered: detail.witness.placements.length, ...score, tiles: score.physical_tiles, signature });
    } catch (e) { if (!e.code) throw e; exclusions.push({ directory: v.directory, angle, code: e.code, message: e.message, entity: e.entity }); }
  }
  const covered = candidates.length ? Math[cut](...candidates.map(c => c.covered)) : null;
  const selectedCut = candidates.filter(c => c.covered === covered);
  const memberships = new Set(selectedCut.map(c => JSON.stringify(c.detail.witness.placements.map(p => p.instance).sort())));
  if (memberships.size > 1) fail('frontier-source-membership', 'A source cut must have identical membership across its shape alternatives');
  return { variants: diverse(selectedCut, limit), covered, exclusions,
    available_cuts: [...new Set(candidates.map(c => c.covered))].sort((a, b) => a - b), generated: candidates.length };
}

export function frontierGroups(graph, banks, catalogs) {
  const groups = discoverClusters(graph, banks.map(b => ({ id: b.id, detail: b.variants[0].detail })), catalogs);
  return groups.map(g => ({ banks: g.regions.map(r => banks.find(b => b.id === r.id)), targets: g.frontier }));
}
