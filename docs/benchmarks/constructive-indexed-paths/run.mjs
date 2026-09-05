// Usage: node run.mjs BASELINE_BINARY CANDIDATE_BINARY NEW_OUTPUT_DIRECTORY
// Build both binaries and finish tests before running. Large artifacts remain outside the repo.
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { fileURLToPath } from 'node:url';
import { spawn, execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';

const root = fileURLToPath(new URL('../../../', import.meta.url));
const [baseline, candidate, output] = process.argv.slice(2).map(p => path.resolve(p));
if (!baseline || !candidate || !output) throw new Error('Expected two binaries and a new output directory');
fs.mkdirSync(output);
const hash = value => createHash('sha256').update(value).digest('hex');
const digest = value => hash(JSON.stringify(value));
const files = [
  'data/game/normalized/recipes.json', 'data/game/normalized/facilities.json',
  'data/game/normalized/items.json', 'data/game/normalized/transports.json',
  'data/game/normalized/logistics-components.json',
  'data/examples/source-plan.game-heavy-xiranite-forge.request.json',
  'data/examples/source-plan.game-seed-cycle-small.request.json',
  'data/examples/constructive-path-queries.json',
];
const binaries = { baseline, candidate };
const summary = {
  schema_version: 1,
  baseline_revision: 'f8ce096',
  source_parent_revision: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim(),
  platform: { os: os.type(), release: os.release(), arch: os.arch(), cpus: os.cpus().length },
  rustc: execFileSync('rustc', ['--version'], { encoding: 'utf8' }).trim(),
  build: 'release, opt-level=3; no compiler/test overlap with timings',
  binary_sha256: Object.fromEntries(Object.entries(binaries).map(([key, binary]) => [key, hash(fs.readFileSync(binary))])),
  input_sha256: Object.fromEntries(files.map(file => [file, hash(fs.readFileSync(path.join(root, file)))])),
  runs: [],
};
const references = new Map();
for (const [workload, sourcePlan] of [
  ['heavy-xiranite', files[5]], ['seed-cycle', files[6]],
]) {
  for (let round = 0; round < 6; round++) {
    const profile = round === 5;
    const order = round % 2 === 0 ? ['baseline', 'candidate'] : ['candidate', 'baseline'];
    for (const variant of order) {
      const prefix = path.join(output, `${workload}-${round}-${variant}`);
      const args = ['layouts', 'construct-production',
        '--recipes', files[0], '--source-plan', sourcePlan, '--facility-catalog', files[1],
        '--item-catalog', files[2], '--transport-catalog', files[3], '--logistics-component-catalog', files[4],
        '--target-instance', 'facility-instance:recipe-occurrence:/target:0',
        '--max-steps', '256', '--max-placement-attempts', '256', '--max-route-searches', '4096',
        '--report-output', `${prefix}.json`, '--visualization-output', `${prefix}.html`,
        ...(profile ? ['--profile'] : []),
      ];
      const errorFile = fs.openSync(`${prefix}.stderr`, 'wx');
      const chunks = [];
      let stdoutBytes = 0;
      const started = performance.now();
      let code;
      try {
        code = await new Promise((resolve, reject) => {
          const child = spawn('/usr/bin/time', ['-lp', binaries[variant], ...args], {
            cwd: root, stdio: ['ignore', 'pipe', errorFile],
          });
          child.stdout.on('data', chunk => {
            stdoutBytes += chunk.length;
            if (stdoutBytes <= 4096) chunks.push(chunk);
          });
          child.on('error', reject);
          child.on('close', resolve);
        });
      } finally { fs.closeSync(errorFile); }
      const wall = (performance.now() - started) / 1000;
      if (stdoutBytes > 4096) throw new Error(`Oversized stdout from ${prefix}`);
      const stdout = Buffer.concat(chunks).toString('utf8');
      const completion = JSON.parse(stdout);
      const report = JSON.parse(fs.readFileSync(`${prefix}.json`, 'utf8'));
      if (code !== 0 || !report.complete || !report.success || !completion.success
        || completion.kind !== 'constructive-production-summary' || stdout.split('\n').length !== 2
        || completion.report_output !== `${prefix}.json` || completion.visualization_output !== `${prefix}.html`
        || completion.status !== report.status) throw new Error(`Failed completion: ${prefix}`);
      const signature = digest({
        success: report.success, complete: report.complete, final: report.final_node,
        steps: report.steps.map(s => ({ node: s.node, statistics: s.statistics, attempts: s.placement_attempts })),
        failed_statistics: report.failed_statistics, rejected_reasons: report.rejected_reasons,
        diagnostics: report.diagnostics,
      });
      if (!references.has(workload)) references.set(workload, signature);
      if (signature !== references.get(workload)) throw new Error(`Changed witness/search choices: ${prefix}`);
      const errorText = fs.readFileSync(`${prefix}.stderr`, 'utf8');
      const record = {
        workload, round, variant, profile_enabled: profile, exit_code: code,
        wall_seconds: wall, planner_seconds: report.elapsed_ms / 1000,
        user_seconds: Number(errorText.match(/^user (\S+)/m)?.[1]),
        system_seconds: Number(errorText.match(/^sys (\S+)/m)?.[1]),
        rss_bytes: Number(errorText.match(/^\s*(\d+)\s+maximum resident set size/m)?.[1]),
        stdout_bytes: stdoutBytes, report_bytes: fs.statSync(`${prefix}.json`).size,
        html_bytes: fs.statSync(`${prefix}.html`).size,
        final_node_sha256: digest(report.final_node), witness_and_choices_sha256: signature,
        facilities: report.final_node.member_instances.length,
        internal_requirements: report.final_node.internal_requirements.length,
        external_requirements: report.final_node.external_requirements.length,
        steps: report.steps.length,
        astar_calls: report.steps.reduce((n, s) => n + s.statistics.astar_searches, 0),
        profile: profile ? JSON.parse(errorText.split('\n').find(line => line.startsWith('{'))) : null,
      };
      summary.runs.push(record);
      // Persist partial evidence if a later run fails; never lose completed measurements.
      fs.writeFileSync(path.join(output, 'summary.json'), JSON.stringify(summary, null, 2) + '\n');
      console.log(JSON.stringify({ workload, round, variant, profile, wall, planner: record.planner_seconds, sameWitness: true }));
    }
  }
}
const distribution = values => {
  const sorted = [...values].sort((a, b) => a - b);
  return { min: sorted[0], median: sorted[Math.floor(sorted.length / 2)], max: sorted.at(-1), count: sorted.length };
};
summary.distributions = {};
for (const workload of references.keys()) {
  summary.distributions[workload] = {};
  for (const variant of Object.keys(binaries)) {
    const runs = summary.runs.filter(r => r.workload === workload && r.variant === variant && !r.profile_enabled);
    summary.distributions[workload][variant] = Object.fromEntries(
      ['wall_seconds', 'planner_seconds', 'rss_bytes'].map(key => [key, distribution(runs.map(r => r[key]))]));
  }
}
fs.writeFileSync(path.join(output, 'summary.json'), JSON.stringify(summary, null, 2) + '\n');
console.log(JSON.stringify(summary.distributions, null, 2));
