// Compare frozen placement geometry, never treating center spans as routed distances.
import fs from 'node:fs';
import { createHash } from 'node:crypto';
const [beforeFile, afterFile, output] = process.argv.slice(2);
if (process.argv.length !== 5) throw new Error('Usage: node compare.mjs BEFORE AFTER NEW_OUTPUT');
const load = file => {
  const bytes = fs.readFileSync(file), report = JSON.parse(bytes);
  if (report.kind !== 'placement-only-visual-diagnostic' || !report.placement_geometry_valid
      || report.routing_performed || report.factory_validated) throw new Error(`Expected unrouted placement: ${file}`);
  return { report, file, sha256: createHash('sha256').update(bytes).digest('hex') };
};
const before = load(beforeFile), after = load(afterFile);
const ids = r => r.placements.map(p => p.instance).sort();
if (JSON.stringify(ids(before.report)) !== JSON.stringify(ids(after.report))) throw new Error('Facility instance sets differ');
if (JSON.stringify(before.report.internal_requirements) !== JSON.stringify(after.report.internal_requirements)
    || JSON.stringify(before.report.external_requirements) !== JSON.stringify(after.report.external_requirements))
  throw new Error('Logical requirements differ');
const metrics = ({ report: r, file, sha256 }) => {
  const by = new Map(r.placements.map(p => [p.instance, p]));
  const span = transport => r.internal_requirements.filter(e => e.transport === transport).reduce((sum, e) => {
    const a = by.get(e.source), b = by.get(e.target);
    return sum + Math.abs(a.x + a.width / 2 - b.x - b.width / 2) + Math.abs(a.y + a.height / 2 - b.y - b.height / 2);
  }, 0);
  const pipes = r.internal_requirements.filter(e => e.transport === 'pipe').map(e => {
    const a = by.get(e.source), b = by.get(e.target);
    const faces = [...new Set(b.catalog_ports.filter(p => p.transport === 'pipe' && p.direction === 'input').map(p => p.edge))];
    return { edge: e.id, source: a.instance, source_label: a.label, target: b.instance, target_label: b.label,
      consumer_input_faces: faces, north_applicable: faces.includes('north'), supplier_north: a.y + a.height + 1 <= b.y };
  });
  return { file, sha256, facilities: r.placements.length, bounds: r.bounds, area: r.bounds.width * r.bounds.height,
    logical_belt_center_manhattan_span: span('belt'), logical_pipe_center_manhattan_span: span('pipe'),
    nonforward_requirements: r.non_forward_requirements.length,
    north_pipe_preferences_met: pipes.filter(p => p.north_applicable && p.supplier_north).length,
    north_pipe_preferences_total: pipes.filter(p => p.north_applicable).length, pipes };
};
const oldBy = new Map(before.report.placements.map(p => [p.instance, p]));
const moved = after.report.placements.map(p => {
  const old = oldBy.get(p.instance);
  return { instance: p.instance, before_label: old.label, after_label: p.label,
    before: { x: old.x, y: old.y, rotation: old.rotation }, after: { x: p.x, y: p.y, rotation: p.rotation } };
});
const result = { schema_version: 1, kind: 'placement-geometry-comparison',
  interpretation: 'Center Manhattan spans are logical distance proxies, not routed lengths. North preference is rectangle geometry, not port accessibility. Origins are independently normalized; raw coordinate changes include translation.',
  before: metrics(before), after: metrics(after), instance_comparison: moved };
fs.writeFileSync(output, JSON.stringify(result, null, 2) + '\n', { flag: 'wx' });
console.log(JSON.stringify({ comparison: output, facilities: result.after.facilities, before_area: result.before.area, after_area: result.after.area }));
