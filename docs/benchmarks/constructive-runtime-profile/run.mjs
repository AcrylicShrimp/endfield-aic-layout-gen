// Run from any directory AFTER building release and waiting for compiler/tests to finish.
// Usage: node run.mjs NEW_OUTPUT_DIRECTORY [NATIVE_SAMPLE_TEXT]
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn, execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { gzipSync } from 'node:zlib';

const root = fileURLToPath(new URL('../../../', import.meta.url));
const output = process.argv[2];
if (!output) throw new Error('Expected a new output directory');
fs.mkdirSync(output); // Refuse to overwrite a prior experiment.
const directory = path.resolve(output);
const digest = value => createHash('sha256').update(JSON.stringify(value)).digest('hex');
const summary = {
  schema_version: 1,
  base_revision: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim(),
  build: 'target/release/aic-cli; build separately, no compiler/test overlap',
  runs: [],
};
let reference;
for (const [index, profile] of [false, true, true].entries()) {
  const prefix = path.join(directory, `run-${index}`);
  const args = [
    'layouts', 'construct-production',
    '--recipes', 'data/game/normalized/recipes.json',
    '--source-plan', 'data/examples/source-plan.game-heavy-xiranite-forge.request.json',
    '--facility-catalog', 'data/game/normalized/facilities.json',
    '--item-catalog', 'data/game/normalized/items.json',
    '--transport-catalog', 'data/game/normalized/transports.json',
    '--logistics-component-catalog', 'data/game/normalized/logistics-components.json',
    '--target-instance', 'facility-instance:recipe-occurrence:/target:0',
    '--max-steps', '256', '--max-placement-attempts', '256', '--max-route-searches', '4096',
    '--report-output', `${prefix}.json`, '--visualization-output', `${prefix}.html`,
    ...(profile ? ['--profile'] : []),
  ];
  const stderr = fs.openSync(`${prefix}.stderr`, 'wx');
  const started = performance.now();
  let code;
  let stdoutBytes = 0;
  const stdoutChunks = [];
  try {
    code = await new Promise((resolve, reject) => {
      const child = spawn('/usr/bin/time', ['-lp', 'target/release/aic-cli', ...args], {
        cwd: root, stdio: ['ignore', 'pipe', stderr],
      });
      child.stdout.on('data', chunk => {
        stdoutBytes += chunk.length;
        if (stdoutBytes <= 4096) stdoutChunks.push(chunk);
      });
      child.on('error', reject);
      child.on('close', resolve);
    });
  } finally {
    fs.closeSync(stderr);
  }
  const wallSeconds = (performance.now() - started) / 1000;
  const raw = fs.readFileSync(`${prefix}.json`, 'utf8');
  const report = JSON.parse(raw);
  if (stdoutBytes > 4096) throw new Error(`Run ${index} emitted ${stdoutBytes} stdout bytes instead of a short summary`);
  const stdout = Buffer.concat(stdoutChunks).toString('utf8');
  const completion = JSON.parse(stdout);
  if (stdout.split('\n').length !== 2 || completion.kind !== 'constructive-production-summary'
      || completion.success !== true || completion.status !== report.status
      || completion.report_output !== `${prefix}.json`
      || completion.visualization_output !== `${prefix}.html`) {
    throw new Error(`Invalid completion summary for run ${index}`);
  }
  const errorText = fs.readFileSync(`${prefix}.stderr`, 'utf8');
  const operationProfile = profile ? JSON.parse(errorText.split('\n').find(s => s.startsWith('{'))) : null;
  if (operationProfile) fs.writeFileSync(`${prefix}.profile.json`, JSON.stringify(operationProfile, null, 2) + '\n', { flag: 'wx' });
  const signature = digest({
    success: report.success, complete: report.complete, final: report.final_node,
    steps: report.steps.map(s => ({ node: s.node, statistics: s.statistics, attempts: s.placement_attempts })),
    failed_statistics: report.failed_statistics, rejected_reasons: report.rejected_reasons,
  });
  reference ??= signature;
  if (code !== 0 || !report.success || !report.complete || signature !== reference) {
    throw new Error(`Run ${index} failed or changed layout/choices: inspect ${prefix}`);
  }
  const run = {
    index, profile_enabled: profile, exit_code: code, wall_seconds: wallSeconds,
    user_seconds: Number(errorText.match(/^user (\S+)/m)?.[1]),
    system_seconds: Number(errorText.match(/^sys (\S+)/m)?.[1]),
    max_rss_bytes: Number(errorText.match(/^\s*(\d+)\s+maximum resident set size/m)?.[1]),
    planner_seconds: report.elapsed_ms / 1000,
    stdout_bytes: stdoutBytes, completion_summary: completion,
    final_node_sha256: digest(report.final_node), layout_and_choices_sha256: signature,
    report_bytes: Buffer.byteLength(raw), html_bytes: fs.statSync(`${prefix}.html`).size,
    steps: report.steps.length, facilities: report.final_node.member_instances.length,
    internal_requirements: report.final_node.internal_requirements.length,
    external_requirements: report.final_node.external_requirements.length,
    astar_calls: report.steps.reduce((n, s) => n + s.statistics.astar_searches, 0),
    operation_profile: operationProfile,
  };
  summary.runs.push(run);
  console.log(JSON.stringify({ index, wallSeconds, plannerSeconds: run.planner_seconds, sameLayoutAndChoices: signature === reference }));
}
if (process.argv[3]) {
  const native = fs.readFileSync(process.argv[3]);
  fs.writeFileSync(path.join(directory, 'native-sample.txt.gz'), gzipSync(native), { flag: 'wx' });
  summary.native_sample_sha256 = createHash('sha256').update(native).digest('hex');
}
fs.writeFileSync(path.join(directory, 'summary.json'), JSON.stringify(summary, null, 2) + '\n', { flag: 'wx' });
