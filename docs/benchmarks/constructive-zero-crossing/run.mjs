import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { analyzeZeroCrossing } from './model.mjs';
import { zeroView } from './view.mjs';
import { rotateFacility } from '../constructive-placement-only/model.mjs';
try {
  const args = process.argv.slice(2), options = {};
  const required = ['placement', 'items', 'facilities', 'transports', 'report-output', 'visualization-output'];
  for (let i = 0; i < args.length; i += 2) {
    const key = args[i].replace(/^--/, '');
    if (!args[i].startsWith('--') || ![...required, 'order-budget', 'order-report'].includes(key) || options[key] || !args[i + 1]) throw new Error(`Invalid option ${args[i]}`);
    options[key] = args[i + 1];
  }
  for (const key of required) if (!options[key]) throw new Error(`Missing --${key}`);
  const outputs = ['report-output', 'visualization-output'].map(k => path.resolve(options[k]));
  if (new Set(outputs).size !== 2 || outputs.some(p => fs.existsSync(p))) throw new Error('Output paths must be distinct new files');
  const inputs = {}, load = key => { const file = path.resolve(options[key]), bytes = fs.readFileSync(file);
    inputs[key] = { path: file, sha256: createHash('sha256').update(bytes).digest('hex') }; return JSON.parse(bytes); };
  const placement = load('placement'), items = load('items'), facilities = load('facilities'), transports = load('transports');
  for (const key of ['items', 'facilities']) if (placement.provenance.inputs[key].sha256 !== inputs[key].sha256) throw new Error(`Frozen ${key} catalog mismatch`);
  if (facilities.schema_version !== 3) throw new Error('Expected facilities v3');
  const specs = new Map(facilities.facilities.map(f => [f.id, f])), delta = { north: [0, -1], east: [1, 0], south: [0, 1], west: [-1, 0] };
  for (const p of placement.placements) {
    const f = specs.get(p.facility); if (!f?.allowed_rotations.includes(p.rotation)) throw new Error(`Invalid catalog rotation: ${p.instance}`);
    const shape = rotateFacility(f, p.rotation), ports = shape.ports.map(q => ({ ...q,
      position: { x: q.position.x + p.x, y: q.position.y + p.y },
      connection: { x: q.position.x + p.x + delta[q.edge][0], y: q.position.y + p.y + delta[q.edge][1] } }));
    if (shape.width !== p.width || shape.height !== p.height || JSON.stringify(ports) !== JSON.stringify(p.catalog_ports)) throw new Error(`Invalid catalog geometry: ${p.instance}`);
  }
  const oracle = options['order-report'] ? load('order-report') : null;
  if (oracle && (oracle.placement_sha256 !== inputs.placement.sha256 || oracle.transports_sha256 !== inputs.transports.sha256)) throw new Error('Order oracle input identity mismatch');
  const result = analyzeZeroCrossing(placement, items, transports, Number(options['order-budget'] ?? 100000), oracle);
  result.provenance = { inputs, node_version: process.version,
    model_sha256: createHash('sha256').update(fs.readFileSync(new URL('./model.mjs', import.meta.url))).digest('hex') };
  fs.writeFileSync(outputs[0], JSON.stringify(result, null, 2) + '\n', { flag: 'wx' });
  fs.writeFileSync(outputs[1], zeroView(result), { flag: 'wx' });
  console.log(JSON.stringify({ status: result.status, routing_allowed: false, ...result.summary, elapsed_ms: result.elapsed_ms, report_output: outputs[0] }));
} catch (error) { console.error(JSON.stringify({ code: 'zero-crossing-diagnostic-error', stage: 'zero-crossing', message: error.message })); process.exitCode = 1; }
