// Explicit offline diagnostic CLI; never announces factory success.
import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { placeOnly } from './model.mjs';
import { render } from './render.mjs';
const hash = value => createHash('sha256').update(value).digest('hex');
const required = ['wiring', 'facilities', 'items', 'target', 'report-output', 'visualization-output'];
const allowed = [...required, 'timing-output'];
try {
  const args = process.argv.slice(2), options = {};
  for (let i = 0; i < args.length; i += 2) {
    const key = args[i].replace(/^--/, '');
    if (!args[i].startsWith('--') || !allowed.includes(key) || options[key] || !args[i + 1])
      throw new Error(`Expected explicit ${required.map(k => '--' + k + ' VALUE').join(' ')}; invalid argument ${args[i]}`);
    options[key] = args[i + 1];
  }
  for (const key of required) if (!options[key]) throw new Error(`Missing --${key}`);
  const output = path.resolve(options['report-output']), html = path.resolve(options['visualization-output']);
  const timingOutput = options['timing-output'] && path.resolve(options['timing-output']);
  const outputs = [output, html, ...(timingOutput ? [timingOutput] : [])];
  if (new Set(outputs).size !== outputs.length || outputs.some(p => fs.existsSync(p))) throw new Error('Output paths must be distinct new files');
  const bodyStart = performance.now(), timings = {};
  const measure = (name, fn) => { const start = performance.now(); const value = fn(); timings[name] = performance.now() - start; return value; };
  const identities = {};
  const load = key => {
    const file = path.resolve(options[key]);
    try { const bytes = fs.readFileSync(file); identities[key] = { path: file, sha256: hash(bytes) }; return JSON.parse(bytes); }
    catch (error) { throw new Error(`Cannot load ${key} from ${file}: ${error.message}`); }
  };
  const [wiring, facilities, items] = measure('input_loading_ms', () => [load('wiring'), load('facilities'), load('items')]);
  const report = measure('placement_ms', () => placeOnly(wiring, facilities, items, options.target));
  report.placement_ms = timings.placement_ms;
  report.provenance = measure('provenance_ms', () => ({ inputs: identities, node_version: process.version,
    model_sha256: hash(fs.readFileSync(new URL('./model.mjs', import.meta.url))),
    renderer_sha256: hash(fs.readFileSync(new URL('./render.mjs', import.meta.url))) }));
  const json = measure('report_serialization_ms', () => JSON.stringify(report, null, 2) + '\n');
  measure('report_write_ms', () => fs.writeFileSync(output, json, { flag: 'wx' }));
  const rendered = measure('html_render_ms', () => render(report));
  measure('html_write_ms', () => fs.writeFileSync(html, rendered, { flag: 'wx' }));
  timings.measured_body_ms = performance.now() - bodyStart;
  if (timingOutput) fs.writeFileSync(timingOutput, JSON.stringify({ schema_version: 1,
    clock: 'monotonic-wall-time', scope: 'input-loading-through-artifact-writes; excludes process startup and timing-file write',
    ...timings, report_bytes: Buffer.byteLength(json), html_bytes: Buffer.byteLength(rendered) }, null, 2) + '\n', { flag: 'wx' });
  console.log(JSON.stringify({ kind: report.kind, status: 'placement-only-unrouted',
    facilities: report.placements.length, routes: 0, factory_validated: false, report_output: output, visualization_output: html }));
} catch (error) {
  console.error(JSON.stringify({ code: error.code ?? 'placement-probe-input-or-output-error', stage: 'placement-only',
    entity: error.entity, message: error.message }));
  process.exitCode = 1;
}
