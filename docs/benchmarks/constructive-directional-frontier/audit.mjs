// Offline evidence only: this does not validate a layout or change planner decisions.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';

const hash = value => createHash('sha256').update(value).digest('hex');
const sorted = values => [...new Set(values)].sort();

export function geometryAudit(node, catalog, strip = 1) {
  if (!Number.isSafeInteger(strip) || strip < 1) throw new Error('Expected positive strip width');
  const objects = [];
  const add = (kind, id, x0, y0, x1 = x0, y1 = y0) => {
    if (![x0, y0, x1, y1].every(Number.isSafeInteger) || x0 > x1 || y0 > y1)
      throw new Error(`Invalid geometry: ${kind}/${id}`);
    objects.push({ kind, id, x0, y0, x1, y1 });
  };
  for (const p of node.placements)
    add('facility', p.instance, p.x, p.y, p.x + p.width - 1, p.y + p.height - 1);
  for (const net of node.transport_networks) {
    for (const c of net.cells) add('network-cell', `${net.id}/cell:${c.x}:${c.y}`, c.x, c.y);
    for (const [i, s] of net.segments.entries())
      add('network-segment', `${net.id}/segment:${i}`, Math.min(s.from.x, s.to.x),
        Math.min(s.from.y, s.to.y), Math.max(s.from.x, s.to.x), Math.max(s.from.y, s.to.y));
    for (const t of net.terminals)
      if (t.endpoint.kind === 'external') add('external-terminal', t.id, t.position.x, t.position.y);
  }
  for (const c of node.logistics_components) {
    const spec = catalog.components.find(s => s.id === c.component);
    // Current production components are single-cell. Do not silently invent larger anchors.
    if (!spec || spec.footprint.width !== 1 || spec.footprint.height !== 1)
      throw new Error(`Unsupported component footprint: ${c.component}`);
    add('component', c.id, c.position.x, c.position.y);
  }
  if (!objects.length) throw new Error('Cannot audit empty geometry');
  const extrema = { x_min: Infinity, x_max: -Infinity, y_min: Infinity, y_max: -Infinity };
  for (const o of objects) {
    extrema.x_min = Math.min(extrema.x_min, o.x0);
    extrema.x_max = Math.max(extrema.x_max, o.x1);
    extrema.y_min = Math.min(extrema.y_min, o.y0);
    extrema.y_max = Math.max(extrema.y_max, o.y1);
  }
  const width = extrema.x_max - extrema.x_min + 1;
  const height = extrema.y_max - extrema.y_min + 1;
  if (width !== node.bounds.width || height !== node.bounds.height)
    throw new Error(`Reported bounds do not match used geometry: ${node.id}`);
  const sides = Object.fromEntries(Object.entries(extrema).map(([side, value]) => {
    const key = `${side[0]}${side.endsWith('min') ? 0 : 1}`;
    const intersects = o => side.endsWith('min') ? o[key] < value + strip : o[key] > value - strip;
    const refs = list => list.map(({ kind, id }) => ({ kind, id }));
    return [side, { value, owners: refs(objects.filter(o => o[key] === value)),
      strip_objects: refs(objects.filter(intersects)) }];
  }));
  return { width, height, area: width * height, strip_width: strip, sides };
}

export function frontierAudit(node, wiring) {
  const members = new Set(node.member_instances);
  const covered = new Set([...node.internal_requirements, ...node.external_requirements]);
  const ledger = wiring.edges.filter(e => !covered.has(e.id));
  const pending = ledger.filter(e => members.has(e.source) || members.has(e.target));
  const projected = new Map(node.boundary_requirements.map(b => [b.requirement, b]));
  const rows = pending.map(e => {
    const b = projected.get(e.id);
    return { requirement: e.id, item: e.item, rate: e.rate, source: e.source, target: e.target,
      kind: e.kind, both_placed: members.has(e.source) && members.has(e.target),
      projected: Boolean(b), direction: b?.direction ?? null,
      attachments: b?.attachments.length ?? 0,
      attachment_kinds: sorted((b?.attachments ?? []).map(a => a.kind)),
      new_port_edges: sorted((b?.attachments ?? []).filter(a => a.kind === 'new-port').map(a => a.port.edge)) };
  });
  const active = new Set(pending.flatMap(e => [e.source, e.target]).filter(id => members.has(id)));
  return { facilities: members.size, unresolved_incident_requirements: rows.length,
    active_facilities: active.size, incident_closed_facilities: members.size - active.size,
    input_only: [...active].filter(id => pending.some(e => e.target === id) && !pending.some(e => e.source === id)).length,
    output_only: [...active].filter(id => pending.some(e => e.source === id) && !pending.some(e => e.target === id)).length,
    both_directions: [...active].filter(id => pending.some(e => e.source === id) && pending.some(e => e.target === id)).length,
    requirements: rows };
}

export function reportAudit(report, wiring, catalog) {
  if (report.schema_version !== 2 || !report.final_node) throw new Error('Expected production schema 2 with final node');
  const internal = report.steps.filter(s => s.stage === 'internal-growth').at(-1)?.node;
  const finalGeometry = geometryAudit(report.final_node, catalog);
  const internalGeometry = internal && geometryAudit(internal, catalog);
  return { success: report.success, complete: report.complete, internal_complete: report.internal_complete,
    final_node_sha256: hash(JSON.stringify(report.final_node)),
    final_geometry: finalGeometry, last_internal_geometry: internalGeometry ?? null,
    external_completion_area_delta: report.internal_complete && internalGeometry
      ? finalGeometry.area - internalGeometry.area : null,
    frontier: frontierAudit(report.final_node, wiring),
    history: report.steps.map(s => {
      const { requirements, ...counts } = frontierAudit(s.node, wiring);
      return { step: s.index, stage: s.stage, ...counts };
    }) };
}

function main() {
  const [binary, recipes, sourcePlan, components, output, ...reports] = process.argv.slice(2);
  if (!reports.length) throw new Error('Usage: node audit.mjs BINARY RECIPES SOURCE_PLAN COMPONENTS NEW_OUTPUT_JSON REPORT...');
  // Capture the legacy recipe CLI output privately; never replay its full JSON to stdout.
  const wiringBytes = execFileSync(path.resolve(binary), ['recipes', 'contextual-instance-wiring',
    '--file', path.resolve(recipes), '--request', path.resolve(sourcePlan)], { maxBuffer: 16 * 1024 * 1024 });
  const wiring = JSON.parse(wiringBytes);
  if (!wiring.success) throw new Error(`Wiring projection failed: ${sourcePlan}`);
  const catalog = JSON.parse(fs.readFileSync(components));
  const result = { schema_version: 1, stage: 'directional-frontier-offline-audit',
    limitations: ['Not a layout validator or joint port/capacity feasibility test.',
      'Incident-closed facilities can still participate in shared networks needed by future consumers.',
      'Cell coordinates and segment indices are snapshot-local references, not stable repair IDs.',
      'Projected attachments are constructor options, not an enumeration of every legal option.'],
    inputs: Object.fromEntries([binary, recipes, sourcePlan, components].map(p => [p, hash(fs.readFileSync(p))])),
    wiring_sha256: hash(wiringBytes),
    reports: reports.map(p => { const bytes = fs.readFileSync(p); return { file: p, sha256: hash(bytes),
      ...reportAudit(JSON.parse(bytes), wiring, catalog) }; }) };
  fs.writeFileSync(output, `${JSON.stringify(result, null, 2)}\n`, { flag: 'wx' });
  console.log(JSON.stringify({ success: true, reports: reports.length, output }));
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
