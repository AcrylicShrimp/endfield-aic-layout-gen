import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { constructInterface } from './model.mjs';
import { render } from '../constructive-bound-interconnect/view.mjs';

try {
  const keys = ['request', 'recipes', 'facilities', 'items', 'transports', 'components', 'validator', 'output-directory'];
  const args = process.argv.slice(2), options = {};
  for (let i = 0; i < args.length; i += 2) {
    const k = args[i].slice(2);
    if (!args[i].startsWith('--') || !keys.includes(k) || options[k] || !args[i + 1]) throw new Error(`Invalid argument ${args[i]}`);
    options[k] = args[i + 1];
  }
  for (const k of keys) if (!options[k]) throw new Error(`Missing --${k}`);
  const output = path.resolve(options['output-directory']);
  fs.mkdirSync(output, { recursive: false });
  const write = (name, value) => fs.writeFileSync(path.join(output, name), JSON.stringify(value, null, 2) + '\n', { flag: 'wx' });
  const read = filename => JSON.parse(fs.readFileSync(filename));
  let detail;
  try {
    const catalogs = Object.fromEntries(['recipes', 'facilities', 'items', 'transports', 'components'].map(k => [k, read(options[k])]));
    detail = constructInterface(catalogs, read(options.request));
    write('detail.json', detail); write('witness.json', detail.witness);
    const validationStart = performance.now();
    const run = spawnSync(path.resolve(options.validator), ['--witness', path.join(output, 'witness.json'),
      ...['recipes', 'facilities', 'items', 'transports', 'components'].flatMap(k => [`--${k}`, options[k]]),
      '--report-output', path.join(output, 'validation.json')], { encoding: 'utf8', timeout: 30000, maxBuffer: 1024 * 1024 });
    fs.writeFileSync(path.join(output, 'validator.log'), (run.stdout ?? '') + (run.stderr ?? ''), { flag: 'wx' });
    if (run.error || !fs.existsSync(path.join(output, 'validation.json'))) throw new Error(`Validator failed: ${run.error?.message ?? run.status}`);
    const validation = read(path.join(output, 'validation.json'));
    const accepted = run.status === 0 && validation.covered_geometry_validated && validation.recipe_coverage_complete;
    const report = { schema_version: 1, status: accepted ? 'complete-local-interface' : 'independent-validation-rejected',
      accepted, factory_validated: false, request: path.resolve(options.request),
      construction_ms: detail.construction_ms, validation_process_ms: performance.now() - validationStart,
      facilities: detail.witness.placements.length, lanes: detail.bindings.length,
      bridges: detail.witness.logistics_components.filter(c => c.kind === 'bridge').length,
      splitters: detail.witness.logistics_components.filter(c => c.kind === 'splitter').length,
      convergers: detail.witness.logistics_components.filter(c => c.kind === 'converger').length, decisions: detail.decisions,
      pending_recipe_terminals: validation.recipe_terminals.filter(t => t.pending_rate.numerator > 0).length };
    write('report.json', report);
    fs.writeFileSync(path.join(output, 'index.html'), render(detail, validation, catalogs), { flag: 'wx' });
    console.log(JSON.stringify({ status: report.status, facilities: report.facilities, lanes: report.lanes,
      bridges: report.bridges, splitters: report.splitters, convergers: report.convergers, construction_ms: report.construction_ms, output }));
    if (!accepted) process.exitCode = 2;
  } catch (e) {
    const diagnostic = { code: e.code ?? 'interface-execution-failed', severity: 'error', stage: e.stage ?? 'execution',
      message: e.message, entity: e.entity ?? null, evidence: e.evidence ?? null, request: path.resolve(options.request), candidate_preserved: Boolean(detail) };
    write('failure.json', diagnostic); throw e;
  }
} catch (e) {
  console.error(JSON.stringify({ code: e.code ?? 'interface-execution-failed', message: e.message })); process.exitCode = 1;
}
