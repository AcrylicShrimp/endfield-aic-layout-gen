// Explicit offline diagnostic CLI; never announces factory success.
import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { placeOnly } from './model.mjs';
import { render } from './render.mjs';
const hash = value => createHash('sha256').update(value).digest('hex');
const required = ['wiring', 'facilities', 'items', 'target', 'report-output', 'visualization-output'];
try {
  const args = process.argv.slice(2), options = {};
  for (let i = 0; i < args.length; i += 2) {
    const key = args[i].replace(/^--/, '');
    if (!args[i].startsWith('--') || !required.includes(key) || options[key] || !args[i + 1])
      throw new Error(`Expected explicit ${required.map(k => '--' + k + ' VALUE').join(' ')}; invalid argument ${args[i]}`);
    options[key] = args[i + 1];
  }
  for (const key of required) if (!options[key]) throw new Error(`Missing --${key}`);
  const output = path.resolve(options['report-output']), html = path.resolve(options['visualization-output']);
  if (output === html || fs.existsSync(output) || fs.existsSync(html)) throw new Error('Output paths must be distinct new files');
  const identities = {};
  const load = key => {
    const file = path.resolve(options[key]);
    try { const bytes = fs.readFileSync(file); identities[key] = { path: file, sha256: hash(bytes) }; return JSON.parse(bytes); }
    catch (error) { throw new Error(`Cannot load ${key} from ${file}: ${error.message}`); }
  };
  const wiring = load('wiring'), facilities = load('facilities'), items = load('items');
  const start = performance.now();
  const report = placeOnly(wiring, facilities, items, options.target);
  report.placement_ms = performance.now() - start;
  report.provenance = { inputs: identities, node_version: process.version,
    model_sha256: hash(fs.readFileSync(new URL('./model.mjs', import.meta.url))),
    renderer_sha256: hash(fs.readFileSync(new URL('./render.mjs', import.meta.url))) };
  fs.writeFileSync(output, JSON.stringify(report, null, 2) + '\n', { flag: 'wx' });
  fs.writeFileSync(html, render(report), { flag: 'wx' });
  console.log(JSON.stringify({ kind: report.kind, status: 'placement-only-unrouted',
    facilities: report.placements.length, routes: 0, factory_validated: false, report_output: output, visualization_output: html }));
} catch (error) {
  console.error(JSON.stringify({ code: error.code ?? 'placement-probe-input-or-output-error', stage: 'placement-only',
    entity: error.entity, message: error.message }));
  process.exitCode = 1;
}
