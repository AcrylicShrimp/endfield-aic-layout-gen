import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { constructInterface } from './model.mjs';

try {
  const keys = ['recipes', 'facilities', 'items', 'transports', 'components', 'validator', 'output-directory'];
  const args = process.argv.slice(2), options = {};
  for (let i = 0; i < args.length; i += 2) {
    const k = args[i].slice(2);
    if (!args[i].startsWith('--') || !keys.includes(k) || options[k] || !args[i + 1]) throw new Error(`Invalid argument ${args[i]}`);
    options[k] = args[i + 1];
  }
  for (const k of keys) if (!options[k]) throw new Error(`Missing --${k}`);
  const output = path.resolve(options['output-directory']); fs.mkdirSync(output);
  const read = p => JSON.parse(fs.readFileSync(p));
  const hash = p => createHash('sha256').update(fs.readFileSync(p)).digest('hex');
  const write = (p, v) => fs.writeFileSync(p, JSON.stringify(v, null, 2) + '\n', { flag: 'wx' });
  const catalogs = Object.fromEntries(['recipes', 'facilities', 'items', 'transports', 'components'].map(k => [k, read(options[k])]));
  const four = read(new URL('./four.json', import.meta.url));
  const cases = ['four', 'five', 'carbon'].map(name => ({ name, request: read(new URL(`./${name}.json`, import.meta.url)) }));
  const renamed = structuredClone(four); [...renamed.producers, ...renamed.consumers].forEach((n, i) => { n.id = `node-${9 - i}`; });
  cases.push({ name: 'renamed', request: renamed });
  for (const count of [3, 4]) cases.push({ name: `fanout-${count}`, expected_failure: count === 4 ? 'interface-port-pressure' : null,
    request: { ...structuredClone(four), consumers: Array.from({ length: count }, (_, i) => ({ ...four.consumers[0], id: `C${i}`, utilization: { numerator: 1, denominator: count } })) } });
  for (const count of [2, 3, 4]) cases.push({ name: `replicas-${count}`, request: { schema_version: 1,
    ...Object.fromEntries(['producers', 'consumers'].map(bank => [bank, Array.from({ length: count }, (_, i) => four[bank].map(n => ({ ...n, id: `${n.id}-${i}` }))).flat()])) } });
  const results = [];
  for (const c of cases) {
    const requestPath = path.join(output, `${c.name}.json`); write(requestPath, c.request);
    const destination = path.join(output, c.name);
    const processResult = spawnSync(process.execPath, [new URL('./run.mjs', import.meta.url).pathname,
      '--request', requestPath, ...keys.filter(k => k !== 'output-directory').flatMap(k => [`--${k}`, options[k]]),
      '--output-directory', destination], { encoding: 'utf8', timeout: 45000, maxBuffer: 1024 * 1024 });
    fs.writeFileSync(path.join(output, `${c.name}.log`), (processResult.stdout ?? '') + (processResult.stderr ?? ''), { flag: 'wx' });
    if (c.expected_failure) {
      const failure = read(path.join(destination, 'failure.json'));
      results.push({ name: c.name, passed: processResult.status === 1 && failure.code === c.expected_failure, failure });
      continue;
    }
    if (processResult.status !== 0) { results.push({ name: c.name, passed: false, status: processResult.status, error: processResult.error?.message }); continue; }
    const report = read(path.join(destination, 'report.json')), validation = read(path.join(destination, 'validation.json'));
    const detail = read(path.join(destination, 'detail.json')), times = [];
    for (let i = 0; i < 21; i++) { const r = constructInterface(catalogs, c.request); if (i) times.push(r.construction_ms); }
    times.sort((a, b) => a - b);
    const tiles = new Set(detail.witness.transport_networks.flatMap(n => n.cells.map(p => `${n.transport}:${p.x},${p.y}`))).size;
    results.push({ name: c.name, passed: report.accepted, facilities: report.facilities, lanes: report.lanes, bridges: report.bridges,
      bounds: validation.bounds, physical_tiles: tiles, pending_recipe_terminals: report.pending_recipe_terminals,
      fresh_process_construction_ms: report.construction_ms, validator_ms: validation.elapsed_ms,
      warm_construction: { samples: times.length, min_ms: times[0], median_ms: (times[9] + times[10]) / 2, p95_ms: times[18] },
      decisions: report.decisions, request_sha256: hash(requestPath), witness_sha256: hash(path.join(destination, 'witness.json')) });
  }
  const broken = read(path.join(output, 'four', 'witness.json'));
  broken.logistics_components = [];
  for (const n of broken.transport_networks) n.component_ids = [];
  const mutationPath = path.join(output, 'missing-bridge.json'); write(mutationPath, broken);
  const control = spawnSync(path.resolve(options.validator), ['--witness', mutationPath,
    ...['recipes', 'facilities', 'items', 'transports', 'components'].flatMap(k => [`--${k}`, options[k]]),
    '--report-output', path.join(output, 'missing-bridge-validation.json')], { encoding: 'utf8', timeout: 30000, maxBuffer: 1024 * 1024 });
  fs.writeFileSync(path.join(output, 'missing-bridge.log'), (control.stdout ?? '') + (control.stderr ?? ''), { flag: 'wx' });
  const controlValidation = read(path.join(output, 'missing-bridge-validation.json'));
  results.push({ name: 'missing-bridge-control', passed: control.status === 2 && !controlValidation.covered_geometry_validated,
    diagnostics: controlValidation.diagnostics });
  const report = { schema_version: 1, scope: 'automatic local belt interfaces; no full-factory or global optimality claim',
    provenance: ['recipes', 'facilities', 'items', 'transports', 'components', 'validator'].map(k => ({ kind: k, path: path.resolve(options[k]), sha256: hash(options[k]) })),
    all_passed: results.every(r => r.passed), cases: results };
  write(path.join(output, 'matrix.json'), report);
  console.log(JSON.stringify({ status: report.all_passed ? 'interface-audit-passed' : 'interface-audit-failed', cases: results.length, output }));
  if (!report.all_passed) process.exitCode = 1;
} catch (e) { console.error(JSON.stringify({ code: 'interface-audit-failed', message: e.message })); process.exitCode = 1; }
