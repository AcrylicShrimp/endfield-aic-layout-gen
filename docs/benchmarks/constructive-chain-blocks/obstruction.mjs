// Read-only first-failure audit: occupancy is evidence, not a global infeasibility proof.
import fs from 'node:fs';
import { createHash } from 'node:crypto';
const [placementFile, routingFile, transportFile, output] = process.argv.slice(2);
if (process.argv.length !== 6 || !output || fs.existsSync(output))
  throw new Error('Usage: obstruction.mjs PLACEMENT ROUTING TRANSPORTS NEW_JSON');
const bytes = fs.readFileSync(placementFile), placement = JSON.parse(bytes), routing = JSON.parse(fs.readFileSync(routingFile));
if (routing.provenance.placement_sha256 !== createHash('sha256').update(bytes).digest('hex')) throw new Error('Wrong frozen placement');
const n = routing.first_failure_state, edge = placement.internal_requirements.find(e => e.id === routing.unresolved_internal[0]);
if (!n || !edge) throw new Error('No first-failure snapshot');
const rate = fs.readFileSync(transportFile), transports = JSON.parse(rate).transports;
const capacity = transports.find(t => t.kind === edge.transport).capacity;
const labels = new Map(placement.placements.map(p => [p.instance, p.label]));
const endpoints = [['source', 'output'], ['target', 'input']].map(([role, direction]) => {
  const p = placement.placements.find(p => p.instance === edge[role]);
  const cells = p.catalog_ports.filter(p => p.direction === direction && p.transport === edge.transport).map(port => {
    const networks = n.transport_networks.filter(n => n.transport === edge.transport && n.cells.some(c =>
      c.x === port.connection.x + routing.search_translation && c.y === port.connection.y + routing.search_translation));
    return { port: port.id, connection: port.connection, occupied_by: networks.map(n => ({ network: n.id, item: n.item,
      requirements: n.requirement_ids.map(id => { const e = placement.internal_requirements.find(e => e.id === id);
        return e ? { requirement: id, source: labels.get(e.source), target: labels.get(e.target) } : { requirement: id }; }) })) };
  });
  return { role, instance: p.instance, label: p.label, cells,
    geometrically_unoccupied: cells.filter(c => !c.occupied_by.length).length,
    attachment_domains: n.boundary_requirements.filter(b => b.requirement === edge.id && b.inside_instance === p.instance) };
});
const result = { schema_version: 1, scope: 'exact first-failure snapshot; no relocation or global infeasibility proof',
  snapshot_connections: n.internal_requirements.length, requirement: edge.id, source: labels.get(edge.source), target: labels.get(edge.target),
  transport: edge.transport, demand: edge.rate, capacity,
  minimum_full_capacity_lanes: Math.ceil(edge.rate.numerator * capacity.duration_ms / (edge.rate.denominator * capacity.quantity * 1000)),
  endpoints, attempt: routing.attempts.find(a => a.requirement === edge.id) };
fs.writeFileSync(output, JSON.stringify(result, null, 2) + '\n', { flag: 'wx' });
console.log(JSON.stringify({ source: result.source, target: result.target, minimum_lanes: result.minimum_full_capacity_lanes,
  free_endpoint_cells: endpoints.map(e => ({ label: e.label, free: e.geometrically_unoccupied })), output }));
