// Usage: node run.mjs BASELINE_BINARY CANDIDATE_BINARY NEW_OUTPUT_DIRECTORY
// AIC_ORDER_ROUNDS=1..3 repeats deterministic runs for timing, not independent layout seeds.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn, execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
const root = fileURLToPath(new URL('../../../', import.meta.url));
const [baseline, candidate, output] = process.argv.slice(2).map(p => path.resolve(p));
if (process.argv.length !== 5) throw new Error('Expected baseline, candidate and new output directory');
const rounds = Number(process.env.AIC_ORDER_ROUNDS ?? 1);
const fixedFlow = process.env.AIC_FLOW_EXPERIMENT === '1';
if (!Number.isInteger(rounds) || rounds < 1 || rounds > 3) throw new Error('Expected 1..3 rounds');
const hash = v => createHash('sha256').update(v).digest('hex');
const files = ['data/game/normalized/recipes.json', 'data/game/normalized/facilities.json',
  'data/game/normalized/items.json', 'data/game/normalized/transports.json',
  'data/game/normalized/logistics-components.json',
  'data/examples/source-plan.game-heavy-xiranite-forge.request.json',
  'data/examples/source-plan.game-seed-cycle-small.request.json'];
const result = { schema_version: 1, source_parent: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim(),
  tracked_diff_sha256: hash(execFileSync('git', ['diff', 'HEAD'], { cwd: root })),
  order_source_sha256: hash(fs.readFileSync(path.join(root, 'crates/aic-data/src/layouts/constructive/production/order.rs'))),
  ...(fixedFlow ? { flow_direction: 'east', orientation_source_sha256: hash(fs.readFileSync(path.join(root, 'crates/aic-data/src/layouts/constructive/production/orientation.rs'))) } : {}),
  binary_sha256: { baseline: hash(fs.readFileSync(baseline)), candidate: hash(fs.readFileSync(candidate)) },
  input_sha256: Object.fromEntries(files.map(f => [f, hash(fs.readFileSync(path.join(root, f)))])),
  build: 'release; no build/test overlap during measured runs',
  budgets: { steps: 256, placement_trials_per_growth: 256, astar_per_growth: 4096, wall_guard_seconds_per_run: 180 },
  runs: [] };
fs.mkdirSync(output);
const variants = fixedFlow ? [['baseline', baseline, 0, false], ['unchanged', candidate, 0, false],
  ['closed-coarse', baseline, 0, true], ['fixed-east-coarse', candidate, 0, true, 'east'],
  ['closed-nearby-4', baseline, 4, true], ['fixed-east-nearby-4', candidate, 4, true, 'east']]
  : [['baseline', baseline, 0, false], ['unchanged', candidate, 0, false],
  ['closed-coarse', candidate, 0, true], ['nearby-4', candidate, 4, false], ['closed-nearby-4', candidate, 4, true]];
const witnesses = new Map();
function score(node) {
  if (!node) return null;
  let turns = 0;
  for (const net of node.transport_networks) {
    const arms = new Map();
    const at = p => { const k = `${p.x}:${p.y}`; if (!arms.has(k)) arms.set(k, { ins: [], outs: [] }); return arms.get(k); };
    for (const s of net.segments) { const d = [s.to.x - s.from.x, s.to.y - s.from.y]; at(s.from).outs.push(d); at(s.to).ins.push(d); }
    for (const c of net.cells) for (const i of at(c).ins) for (const o of at(c).outs)
      if (i[0] !== o[0] || i[1] !== o[1]) turns++;
  }
  return [node.bounds.width * node.bounds.height,
    new Set(node.transport_networks.flatMap(n => n.cells.map(c => `${n.transport}:${c.x}:${c.y}`))).size, turns];
}
for (const [workload, plan] of [['heavy-xiranite', files[5]], ['seed-cycle', files[6]]]) {
  const wiring = JSON.parse(execFileSync(candidate, ['recipes', 'contextual-instance-wiring', '--file', files[0], '--request', plan], { cwd: root }));
  if (!wiring.success) throw new Error('Invalid wiring');
  const ids = new Set(wiring.nodes.filter(n => n.kind === 'facility').map(n => n.id));
  const internal = wiring.edges.filter(e => ids.has(e.source) && ids.has(e.target));
  for (let round = 0; round < rounds; round++) {
    const shift = round % variants.length;
    for (const [variant, binary, nearby, closed, flow] of [...variants.slice(shift), ...variants.slice(0, shift)]) {
      const prefix = path.join(output, `${workload}-${round}-${variant}`);
      const args = ['layouts', 'construct-production', '--recipes', files[0], '--facility-catalog', files[1],
        '--item-catalog', files[2], '--transport-catalog', files[3], '--logistics-component-catalog', files[4],
        '--source-plan', plan, '--target-instance', 'facility-instance:recipe-occurrence:/target:0',
        '--max-steps', '256', '--max-placement-attempts', '256', '--max-route-searches', '4096',
        '--nearby-candidates', String(nearby), ...(closed ? ['--downstream-closed-growth'] : []),
        ...(flow ? ['--flow-direction', flow] : []),
        '--report-output', prefix + '.json', '--visualization-output', prefix + '.html'];
      const err = fs.openSync(prefix + '.stderr', 'wx');
      const chunks = []; let stdoutBytes = 0, timedOut = false;
      const start = performance.now();
      const exitCode = await new Promise((resolve, reject) => {
        const child = spawn('/usr/bin/time', ['-lp', binary, ...args], { cwd: root, stdio: ['ignore', 'pipe', err], detached: true });
        const timer = setTimeout(() => { timedOut = true; process.kill(-child.pid, 'SIGKILL'); }, 180000);
        child.stdout.on('data', c => { stdoutBytes += c.length; if (stdoutBytes <= 4096) chunks.push(c); });
        child.on('error', e => { clearTimeout(timer); reject(e); });
        child.on('close', code => { clearTimeout(timer); resolve(code); });
      }).finally(() => fs.closeSync(err));
      const wall = (performance.now() - start) / 1000;
      if (timedOut) {
        result.runs.push({ workload, round, variant, status: 'harness-wall-guard', wall_seconds: wall });
      } else {
        if (![0, 1].includes(exitCode) || stdoutBytes > 4096) throw new Error(`Unexpected CLI completion: ${prefix}`);
        const summary = JSON.parse(Buffer.concat(chunks));
        const r = JSON.parse(fs.readFileSync(prefix + '.json'));
        if (summary.kind !== 'constructive-production-summary' || summary.success !== r.success) throw new Error('Summary mismatch');
        const nodes = [...r.steps.map(s => s.node), ...(r.final_node ? [r.final_node] : [])];
        const outputViolations = nodes.reduce((n, node) => n + internal.filter(e =>
          node.member_instances.includes(e.source) && !node.internal_requirements.includes(e.id)).length, 0);
        if (closed && outputViolations) throw new Error(`Internal output closure violated: ${prefix}`);
        const orientations = new Map((r.orientation_decisions ?? []).map(d => [d.instance, d.rotation]));
        if (flow && nodes.some(n => n.placements.some(p => orientations.get(p.instance) !== p.rotation)))
          throw new Error(`Fixed orientation violated: ${prefix}`);
        const witness = hash(JSON.stringify({ final: r.final_node, steps: r.steps.map(s => ({ node: s.node, statistics: s.statistics, placement_attempts: s.placement_attempts })),
          failed: r.failed_statistics, diagnostics: r.diagnostics }));
        const key = workload + ':' + (variant === 'unchanged' ? 'baseline' : variant);
        if (witnesses.has(key) && witnesses.get(key) !== witness) throw new Error(`Changed witness: ${key}`);
        witnesses.set(key, witness);
        const stderr = fs.readFileSync(prefix + '.stderr', 'utf8');
        result.runs.push({ workload, round, variant, exit_code: exitCode, complete: r.complete,
          internal_complete: r.internal_complete, status: r.status, facilities: r.final_node?.member_instances.length ?? 0,
          internal_requirements: r.final_node?.internal_requirements.length ?? 0,
          external_requirements: r.final_node?.external_requirements.length ?? 0,
          score: score(r.final_node), bounds: r.final_node?.bounds, planner_seconds: r.elapsed_ms / 1000, wall_seconds: wall,
          rss_bytes: Number(stderr.match(/^\s*(\d+)\s+maximum resident set size/m)?.[1]),
          steps: r.steps.length, group_sizes: r.steps.filter(s => s.stage === 'internal-growth').map(s => s.added_instances.length),
          group_trials: r.growth_decisions?.length ?? 0, last_group_trials: r.growth_decisions?.slice(-4) ?? [],
          astar_calls: r.steps.reduce((s, step) => s + step.statistics.astar_searches, r.failed_statistics.astar_searches),
          placements: r.steps.reduce((s, step) => s + step.placement_attempts, r.failed_placement_attempts),
          internal_output_violation_occurrences: outputViolations,
          ...(flow ? { orientation_decisions: r.orientation_decisions, flow_direction: flow } : {}),
          failed_statistics: r.failed_statistics,
          pending_boundaries: (r.final_node?.boundary_requirements ?? []).map(b => ({
            requirement: b.requirement, item: b.item, direction: b.direction,
            kind: wiring.edges.find(e => e.id === b.requirement)?.kind,
            attachments: b.attachments.length,
            attachment_kinds: [...new Set(b.attachments.map(a => a.kind))],
            port_edges: [...new Set(b.attachments.map(a => a.port.edge))] })),
          diagnostics: r.diagnostics, rejected_reasons: r.rejected_reasons,
          failed_groups: r.attempted_groups_at_failure,
          obstruction: r.last_obstruction && { connection: r.last_obstruction.connection, blocked: r.last_obstruction.blocked_requirements },
          final_node_sha256: hash(JSON.stringify(r.final_node)), witness_sha256: witness, stdout_bytes: stdoutBytes });
      }
      fs.writeFileSync(path.join(output, 'summary.json'), JSON.stringify(result, null, 2) + '\n');
    }
  }
}
console.log(JSON.stringify({ success: true, runs: result.runs.length, output: path.join(output, 'summary.json') }));
