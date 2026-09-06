import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { prepareFactory } from './factory-input.mjs';

try {
  const catalogKeys = ['recipes', 'facilities', 'items', 'transports', 'components'];
  const keys = ['request', ...catalogKeys, 'validator', 'output-directory'], args = process.argv.slice(2), options = {};
  for (let i = 0; i < args.length; i += 2) {
    const k = args[i].slice(2);
    if (!args[i].startsWith('--') || !keys.includes(k) || options[k] || !args[i + 1]) throw new Error(`Invalid argument ${args[i]}`);
    options[k] = args[i + 1];
  }
  for (const k of keys) if (!options[k]) throw new Error(`Missing --${k}`);
  const read = p => JSON.parse(fs.readFileSync(p, 'utf8')), hash = p => createHash('sha256').update(fs.readFileSync(p)).digest('hex');
  const input = read(options.request), catalogs = Object.fromEntries(catalogKeys.map(k => [k, read(options[k])]));
  prepareFactory(input, catalogs);
  const sourceHash = hash(options.request), output = path.resolve(options['output-directory']), results = []; fs.mkdirSync(output);
  for (const first of input.policy.families) {
    const directory = path.join(output, first); fs.mkdirSync(directory);
    const request = structuredClone(input); request.policy.families = [first, ...input.policy.families.filter(f => f !== first)];
    const requestPath = path.join(directory, 'request.json'); fs.writeFileSync(requestPath, JSON.stringify(request, null, 2) + '\n', { flag: 'wx' });
    const child = spawnSync(process.execPath, [fileURLToPath(new URL('./run-factory.mjs', import.meta.url)), '--request', requestPath,
      ...[...catalogKeys, 'validator'].flatMap(k => [`--${k}`, options[k]]), '--output-directory', path.join(directory, 'run')], { encoding: 'utf8', timeout: input.budget.elapsed_ms + 10000 });
    if (child.error || ![0, 2].includes(child.status)) throw new Error(`Control ${first} failed: ${child.error?.message ?? child.stderr}`);
    const reportPath = path.join(directory, 'run/report.json'), report = read(reportPath);
    results.push({ first, status: report.status, factory_validated: report.factory_validated, score: report.selected?.score ?? null,
      covered: report.retained?.validation.coverage.covered ?? 0, budget: report.budget, report: reportPath, report_sha256: hash(reportPath) });
  }
  if (hash(options.request) !== sourceHash) throw new Error(`Input changed during controls: ${options.request}`);
  const report = { schema_version: 1, scope: 'Same original graph and work caps; first-completion controls, not equal-consumed-work optimization',
    source: { path: path.resolve(options.request), sha256: sourceHash }, runner_sha256: hash(fileURLToPath(import.meta.url)), results };
  fs.writeFileSync(path.join(output, 'report.json'), JSON.stringify(report, null, 2) + '\n', { flag: 'wx' }); console.log(JSON.stringify(report));
} catch (e) { console.error(JSON.stringify({ code: 'chain-control-failed', severity: 'error', stage: 'factory-controls', message: e.message, entity: null })); process.exitCode = 1; }
