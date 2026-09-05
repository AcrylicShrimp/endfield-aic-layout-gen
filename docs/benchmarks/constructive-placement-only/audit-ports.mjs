// Read-only semantic cross-check: source rotations against non-corner normals, then corner entries.
import fs from 'node:fs';
import { createHash } from 'node:crypto';
const [sourceFile, catalogFile, baselineFile, output] = process.argv.slice(2);
if (process.argv.length !== 6) throw new Error('Usage: node audit-ports.mjs SOURCE_BUILDINGS CATALOG BASELINE_REPORT NEW_OUTPUT');
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const inputs = [sourceFile, catalogFile, baselineFile].map(path => ({ path, bytes: fs.readFileSync(path) }));
const [source, catalog, baseline] = inputs.map(i => JSON.parse(i.bytes));
const samples = [], mapping = new Map();
for (const f of catalog.facilities) {
  const raw = Object.values(source).find(b => b.id.replaceAll('_', '-') === f.id.split('-mode-')[0]);
  if (!raw) throw new Error(`Missing source building: ${f.id}`);
  for (const p of f.ports) {
    const rp = raw[p.direction === 'input' ? 'inputPorts' : 'outputPorts'].find(q =>
      q.index === Number(p.id.split('-').at(-1)) && q.isPipe === (p.transport === 'pipe'));
    if (!rp) throw new Error(`Missing source port: ${f.id}/${p.id}`);
    const x = rp.trans.position.x, y = rp.trans.position.z, possible = [];
    if (x !== p.position.x || y !== p.position.y) throw new Error(`Coordinate discrepancy: ${f.id}/${p.id}`);
    if (y === 0) possible.push('north');
    if (x === raw.range.width - 1) possible.push('east');
    if (y === raw.range.depth - 1) possible.push('south');
    if (x === 0) possible.push('west');
    const key = p.direction + ':' + rp.trans.rotation.y;
    if (possible.length === 1) {
      if (!mapping.has(key)) mapping.set(key, new Set());
      mapping.get(key).add(possible[0]);
    }
    samples.push({ facility: f.id, port: p.id, direction: p.direction, rotation_y: rp.trans.rotation.y,
      key, stored_edge: p.edge, possible_edges: possible, position: [x, y] });
  }
}
const mismatches = samples.filter(p => mapping.get(p.key)?.size === 1
  && !mapping.get(p.key).has(p.stored_edge)).map(p => ({ ...p, expected_edge: [...mapping.get(p.key)][0] }));
const bad = new Set(mismatches.map(p => p.facility + ':' + p.port));
const node = baseline.final_node;
if (!node) throw new Error('Expected baseline final node');
const placements = new Map(node.placements.map(p => [p.instance, p]));
const used = new Map();
for (const net of node.transport_networks) for (const t of net.terminals) {
  if (t.endpoint.kind !== 'facility') continue;
  const p = placements.get(t.endpoint.instance);
  if (bad.has(p.facility + ':' + t.endpoint.port)) used.set(p.instance + ':' + t.endpoint.port,
    { instance: p.instance, facility: p.facility, port: t.endpoint.port });
}
const report = { schema_version: 1, stage: 'source-port-direction-audit',
  inputs: inputs.map(i => ({ path: i.path, sha256: hash(i.bytes) })),
  noncorner_direction_mapping: Object.fromEntries([...mapping].map(([k, v]) => [k, [...v]])),
  total_ports: samples.length, affected_ports: mismatches.length,
  affected_facility_modes: new Set(mismatches.map(p => p.facility)).size, mismatches,
  baseline_complete: baseline.complete, baseline_used_affected_ports: [...used.values()],
  interpretation: 'Mismatch inferred from source rotation/direction classes with unambiguous non-corner normals. This audit changes no catalog or geometry. Zero affected terminals in this baseline does not validate every historical search result or prove source equivalence for all game semantics.' };
fs.writeFileSync(output, JSON.stringify(report, null, 2) + '\n', { flag: 'wx' });
console.log(JSON.stringify({ affected_ports: mismatches.length, affected_facility_modes: report.affected_facility_modes,
  baseline_used_affected_ports: used.size, output }));
