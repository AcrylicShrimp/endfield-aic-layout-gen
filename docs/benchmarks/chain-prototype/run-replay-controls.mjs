import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { prepareReplayPolicy } from './replay-policy.mjs';
import { prepareFactory } from './factory-input.mjs';

try {
  const catalogKeys = ['recipes', 'facilities', 'items', 'transports', 'components'];
  const keys = ['request', 'policy', ...catalogKeys, 'validator', 'output-directory'], args = process.argv.slice(2), options = {};
  for (let i = 0; i < args.length; i += 2) {
    const k = args[i].slice(2);
    if (!args[i].startsWith('--') || !keys.includes(k) || options[k] || !args[i + 1]) throw new Error(`Invalid argument ${args[i]}`);
    options[k] = args[i + 1];
  }
  for (const k of keys) if (!options[k]) throw new Error(`Missing --${k}`);
  const read = p => JSON.parse(fs.readFileSync(p, 'utf8')), hash = p => createHash('sha256').update(fs.readFileSync(p)).digest('hex');
  const input = read(options.request), policy = prepareReplayPolicy(read(options.policy));
  prepareFactory(input, Object.fromEntries(catalogKeys.map(k => [k, read(options[k])])));
  const provenance = ['request', 'policy'].map(k => ({ path: path.resolve(options[k]), sha256: hash(options[k]) }));
  const output = path.resolve(options['output-directory']), results = []; fs.mkdirSync(output);
  for (const proposal_order of ['exhaustive', 'interleaved']) for (const ranking of ['partial-area', 'remaining-fit']) {
    const directory = path.join(output, `${proposal_order}-${ranking}`); fs.mkdirSync(directory);
    const policyPath = path.join(directory, 'policy.json'); fs.writeFileSync(policyPath, JSON.stringify({ ...policy, proposal_order, ranking }, null, 2) + '\n', { flag: 'wx' });
    const child = spawnSync(process.execPath, [fileURLToPath(new URL('./run-replay.mjs', import.meta.url)), '--policy', policyPath,
      ...['request', ...catalogKeys, 'validator'].flatMap(k => [`--${k}`, options[k]]), '--output-directory', path.join(directory, 'run')], { encoding: 'utf8', timeout: input.budget.elapsed_ms + 10000 });
    if (child.error || ![0, 2].includes(child.status)) throw new Error(`Control ${proposal_order}/${ranking} failed: ${child.error?.message ?? child.stderr}`);
    const reportPath = path.join(directory, 'run/report.json'), report = read(reportPath);
    results.push({ proposal_order, ranking, factory_validated: report.factory_validated, score: report.selected?.score ?? null,
      covered: report.retained?.validation.coverage.covered ?? 0, trials: report.trials.length, rollbacks: report.rollbacks.length, budget: report.budget, report: reportPath, report_sha256: hash(reportPath) });
  }
  for (const p of provenance) if (hash(p.path) !== p.sha256) throw new Error(`Input changed during controls: ${p.path}`);
  const report = { schema_version: 1, scope: 'Same original graph and global caps; proposal-order by ranking controls, not a shared-budget portfolio',
    provenance, runner_sha256: hash(fileURLToPath(import.meta.url)), results };
  fs.writeFileSync(path.join(output, 'report.json'), JSON.stringify(report, null, 2) + '\n', { flag: 'wx' }); console.log(JSON.stringify(report));
} catch (e) { console.error(JSON.stringify({ code: 'chain-replay-control-failed', severity: 'error', stage: 'replay-controls', message: e.message, entity: null })); process.exitCode = 1; }
