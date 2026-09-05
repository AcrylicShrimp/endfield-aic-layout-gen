import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { chainBlocks } from './model.mjs';
import { render } from '../constructive-placement-only/render.mjs';
import { rotateFacility } from '../constructive-placement-only/model.mjs';
const hash = data => createHash('sha256').update(data).digest('hex');
try {
  const args = process.argv.slice(2), options = {};
  const required = ['placement', 'facilities', 'items', 'report-output', 'visualization-output'];
  const allowed = [...required, 'minimum-gap', 'sweeps', 'contract'];
  for (let i = 0; i < args.length; i += 2) {
    const key = args[i].replace(/^--/, '');
    if (!args[i].startsWith('--') || !allowed.includes(key) || options[key] !== undefined || args[i + 1] === undefined)
      throw new Error(`Invalid option ${args[i]}; use explicit --NAME VALUE flags`);
    options[key] = args[i + 1];
  }
  for (const key of required) if (!options[key]) throw new Error(`Missing --${key}`);
  if (options.contract !== undefined && !['true', 'false'].includes(options.contract)) throw new Error('--contract must be true or false');
  const outputs = ['report-output', 'visualization-output'].map(k => path.resolve(options[k]));
  if (new Set(outputs).size !== outputs.length || outputs.some(p => fs.existsSync(p))) throw new Error('Output paths must be distinct new files');
  const start = performance.now(), identities = {};
  const load = key => { const p = path.resolve(options[key]); const data = fs.readFileSync(p);
    identities[key] = { path: p, sha256: hash(data) }; return JSON.parse(data); };
  const input = load('placement'), catalog = load('facilities'), items = load('items');
  for (const key of ['facilities', 'items']) if (input.provenance?.inputs?.[key]?.sha256 !== identities[key].sha256)
    throw new Error(`Frozen placement does not match ${options[key]}; regenerate its catalog projection first`);
  if (catalog.schema_version !== 3) throw new Error(`Expected facilities v3: ${options.facilities}`);
  const specs = new Map(catalog.facilities.map(f => [f.id, f]));
  const delta = { north: [0, -1], east: [1, 0], south: [0, 1], west: [-1, 0] };
  for (const p of input.placements) {
    const spec = specs.get(p.facility);
    if (!spec?.allowed_rotations.includes(p.rotation)) throw new Error(`Illegal frozen rotation: ${p.instance}`);
    const shape = rotateFacility(spec, p.rotation);
    const projected = shape.ports.map(q => ({ ...q, position: { x: q.position.x + p.x, y: q.position.y + p.y },
      connection: { x: q.position.x + p.x + delta[q.edge][0], y: q.position.y + p.y + delta[q.edge][1] } }));
    if (shape.width !== p.width || shape.height !== p.height || JSON.stringify(projected) !== JSON.stringify(p.catalog_ports))
      throw new Error(`Frozen geometry does not match runtime catalog: ${p.instance}`);
  }
  const report = chainBlocks(input, items, { contract: options.contract !== 'false',
    minimumGap: Number(options['minimum-gap'] ?? 3), sweeps: Number(options.sweeps ?? 2) });
  report.provenance = { inputs: identities, node_version: process.version,
    model_sha256: hash(fs.readFileSync(new URL('./model.mjs', import.meta.url))),
    renderer_sha256: hash(fs.readFileSync(new URL('../constructive-placement-only/render.mjs', import.meta.url))) };
  fs.writeFileSync(outputs[0], JSON.stringify(report, null, 2) + '\n', { flag: 'wx' });
  fs.writeFileSync(outputs[1], render(report), { flag: 'wx' });
  console.log(JSON.stringify({ status: 'placement-only-unrouted', ...report.block_experiment.summary,
    placement_ms: report.placement_ms, body_ms: performance.now() - start, report_output: outputs[0], visualization_output: outputs[1] }));
} catch (error) {
  console.error(JSON.stringify({ code: error.code ?? 'chain-block-input-or-output-error', stage: 'chain-block-placement',
    entity: error.entity, message: error.message })); process.exitCode = 1;
}
