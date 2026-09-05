// Fresh-process graph + placement timing. Detailed output goes to a new directory only.
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';

const [directory, countText = '20'] = process.argv.slice(2);
const count = Number(countText);
if (!directory || !Number.isInteger(count) || count < 2 || count > 100 || process.argv.length > 4)
  throw new Error('Usage (from repository root): node docs/benchmarks/constructive-placement-only/time.mjs NEW_DIRECTORY [2..100_RUNS]');
const output = path.resolve(directory);
fs.mkdirSync(output); // Refuse to overwrite previous measurements.
const sha = value => createHash('sha256').update(value).digest('hex');
const cli = path.resolve('target/release/aic-cli');
const probe = path.resolve('docs/benchmarks/constructive-placement-only/place.mjs');
const request = path.resolve('data/examples/source-plan.game-heavy-xiranite-forge.request.json');
const facilities = path.resolve('data/game/normalized/facilities.json');
const items = path.resolve('data/game/normalized/items.json');
const recipes = path.resolve('data/game/normalized/recipes.json');
const run = (binary, args, log) => {
  const fd = fs.openSync(log, 'wx');
  try {
    const start = performance.now();
    const result = spawnSync(binary, args, { stdio: ['ignore', fd, 'pipe'], timeout: 60000 });
    const elapsed = performance.now() - start;
    if (result.error || result.status !== 0)
      throw new Error(`${binary} failed (${result.status}): ${result.error?.message ?? result.stderr?.toString().slice(0, 2000)}; output: ${log}`);
    return elapsed;
  } finally { fs.closeSync(fd); }
};
const samples = [];
for (let i = 0; i < count; i++) {
  const prefix = path.join(output, String(i).padStart(2, '0'));
  const wiring = prefix + '-wiring.json', reportFile = prefix + '-placement.json';
  const pipelineStart = performance.now();
  const graph_cli_wall_ms = run(cli, ['recipes', 'contextual-instance-wiring', '--file', recipes,
    '--request', request], wiring);
  const placement_cli_wall_ms = run(process.execPath, [probe, '--wiring', wiring,
    '--facilities', facilities, '--items', items, '--target', 'facility-instance:recipe-occurrence:/target:0',
    '--report-output', reportFile, '--visualization-output', prefix + '.html',
    '--timing-output', prefix + '-timing.json'], prefix + '-completion.json');
  const pipeline_wall_ms = performance.now() - pipelineStart;
  const report = JSON.parse(fs.readFileSync(reportFile));
  if (report.placements.length !== 59 || !report.placement_geometry_valid || report.routing_performed
      || report.factory_validated || report.transport_networks.length)
    throw new Error(`Invalid placement-only result: ${reportFile}`);
  const { placement_ms, provenance, ...deterministic } = report;
  const geometry_sha256 = sha(JSON.stringify(deterministic));
  if (samples.length && geometry_sha256 !== samples[0].geometry_sha256)
    throw new Error(`Nondeterministic placement: ${reportFile}`);
  const timing = JSON.parse(fs.readFileSync(prefix + '-timing.json'));
  samples.push({ run: i, graph_cli_wall_ms, placement_cli_wall_ms, pipeline_wall_ms,
    ...timing, geometry_sha256, bounds: report.bounds });
}
const metrics = Object.keys(samples[0]).filter(key => key.endsWith('_ms'));
const stats = rows => Object.fromEntries(metrics.map(key => {
  const values = rows.map(row => row[key]).sort((a, b) => a - b), n = values.length;
  return [key, { min: values[0], median: (values[Math.floor((n - 1) / 2)] + values[Math.floor(n / 2)]) / 2,
    p95: values[Math.ceil(n * .95) - 1], max: values.at(-1) }];
}));
const summary = { schema_version: 1, kind: 'corrected-port-placement-timing', runs: count,
  scope: 'Release graph CLI including JSON file output, then a fresh Node process through placement JSON and HTML output. No routing. Build and tests excluded. Warm filesystem; no cold-cache claim. First run retained, not discarded.',
  node_version: process.version, platform: process.platform, architecture: process.arch,
  inputs: [cli, probe, request, facilities, items, recipes].map(file => ({ path: file, sha256: sha(fs.readFileSync(file)) })),
  raw_directory: output, statistics: stats(samples), repeat_statistics_excluding_first: stats(samples.slice(1)), samples };
fs.writeFileSync(path.join(output, 'summary.json'), JSON.stringify(summary, null, 2) + '\n', { flag: 'wx' });
console.log(JSON.stringify({ runs: count, summary: path.join(output, 'summary.json'),
  pipeline_median_ms: summary.statistics.pipeline_wall_ms.median,
  placement_median_ms: summary.statistics.placement_ms.median }));
