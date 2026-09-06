import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { compactEmbedding } from './model.mjs';
import { explainRectangle, rigidFacilityGroups } from './order.mjs';
import { emptyRectangles } from '../constructive-local-reembedding/groups.mjs';
import { extentOf } from '../constructive-local-reembedding/model.mjs';
const read = p => JSON.parse(fs.readFileSync(p));
const write = (p, v) => fs.writeFileSync(p, JSON.stringify(v, null, 2) + '\n', { flag: 'wx' });
const hash = p => createHash('sha256').update(fs.readFileSync(p)).digest('hex');
try {
  const keys = ['baseline', 'catalog-directory', 'output-directory'], options = {}, args = process.argv.slice(2);
  for (let i = 0; i < args.length; i += 2) {
    const k = args[i].slice(2); if (!args[i].startsWith('--') || !keys.includes(k) || options[k] || !args[i + 1]) throw new Error(`Invalid argument ${args[i]}`);
    options[k] = args[i + 1];
  }
  for (const k of keys) if (!options[k]) throw new Error(`Missing --${k}`);
  const report = read(options.baseline), source = path.join(report.selected_directory, 'detail.json'), detail = read(source);
  if (!report.factory_validated) throw new Error('Expected a complete validated incumbent report');
  const catalogs = { facilities: read(path.join(options['catalog-directory'], 'facilities.json')) }, w = detail.witness, bounds = extentOf(w);
  const rectangles = emptyRectangles(w, 32).filter(r => r.x > bounds.minX && r.y > bounds.minY && r.x + r.width <= bounds.maxX && r.y + r.height <= bounds.maxY).slice(0, 8);
  const output = path.resolve(options['output-directory']); fs.mkdirSync(output); const results = [], rigidity = [];
  for (const axis of ['x', 'y']) {
    const compacted = compactEmbedding(detail, catalogs, axis), c = compacted.compaction_certificate;
    write(path.join(output, `${axis}-certificate.json`), c);
    const groups = Array.from({ length: c.groups }, (_, group) => {
      const points = c.coordinates.filter(p => p.group === group).map(p => p.original), keys = new Set(points.map(p => `${p.x},${p.y}`));
      return { group, facilities: w.placements.filter(p => keys.has(`${p.x},${p.y}`)).map(p => ({ instance: p.instance, label: detail.labels[p.instance] })),
        networks: w.transport_networks.filter(n => n.cells.some(p => keys.has(`${p.x},${p.y}`))).map(n => ({ id: n.id, item: n.item, transport: n.transport })),
        points };
    });
    write(path.join(output, `${axis}-groups.json`), groups);
    const retained = rigidFacilityGroups(w, axis), independent = rigidFacilityGroups(w, axis, false);
    const signature = gs => JSON.stringify(gs.map(g => [...g].sort()).sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b))));
    const matches = signature(retained) === signature(groups.map(g => g.facilities.map(f => f.instance)).filter(g => g.length));
    if (!matches) throw new Error(`Equality reconstruction does not match actual ${axis} certificate`);
    rigidity.push({ axis, reconstruction_matches_certificate: matches,
      retained: retained.map(g => g.map(id => detail.labels[id] ?? id)), independent: independent.map(g => g.map(id => detail.labels[id] ?? id)),
      scope: 'Equality-only cross-layer coincidence ablation; not a feasible layout or predicted area.' });
    for (const r of rectangles) {
      const result = explainRectangle(c, r), s = result.strongest;
      const ids = s ? [...new Set([s.source.group, ...(s.path ?? []).map(e => e.to), s.target.group])] : [];
      results.push({ ...result, involved_groups: ids.map(id => ({ ...groups[id], points: undefined })) });
    }
  }
  const result = { schema_version: 1, stage: 'retained-order', baseline: path.resolve(options.baseline), source_sha256: hash(source), score: report.score,
    results, rigidity, source_hashes: Object.fromEntries(['order.mjs', 'order-probe.mjs', 'model.mjs'].map(p => [p, hash(new URL(p, import.meta.url))])) };
  write(path.join(output, 'report.json'), result);
  console.log(JSON.stringify({ status: 'order-diagnostic-complete', rectangles: rectangles.length, locked_axes: results.filter(r => r.strongest?.locked).length, output }));
} catch (e) { console.error(JSON.stringify({ code: e.code ?? 'order-probe-failed', stage: 'retained-order', message: e.message })); process.exitCode = 1; }
