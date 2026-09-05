// Sequential, bounded release-mode matrix. Detailed data stays in files.
import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
const [directory] = process.argv.slice(2);
if (!directory || fs.existsSync(directory)) throw new Error('Provide one new output directory');
fs.mkdirSync(directory, { recursive: true });
const folder = path.resolve(directory), rows = [];
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const frozen = 'docs/benchmarks/constructive-supplier-groups/heavy.json';
const baseline = JSON.parse(fs.readFileSync(frozen));
const modelHash = hash(fs.readFileSync('docs/benchmarks/constructive-chain-blocks/model.mjs'));
const catalogArgs = ['--facilities', 'data/game/normalized/facilities.json', '--items', 'data/game/normalized/items.json'];
for (const contract of [false, true]) for (const gap of [1, 3, 6]) {
  const name = `${contract ? 'chains' : 'singletons'}-gap-${gap}`, samples = [];
  let report, witness;
  for (let sample = 0; sample < 3; sample++) {
    const json = path.join(folder, `${name}-${sample}.json`), html = path.join(folder, `${name}-${sample}.html`);
    const completion = JSON.parse(execFileSync(process.execPath, ['docs/benchmarks/constructive-chain-blocks/place.mjs',
      '--placement', frozen, ...catalogArgs, '--report-output', json, '--visualization-output', html,
      '--contract', String(contract), '--minimum-gap', String(gap)], { encoding: 'utf8', timeout: 30000 }));
    report = JSON.parse(fs.readFileSync(json));
    if (report.provenance.model_sha256 !== modelHash) throw new Error('Model changed during measurement');
    const current = hash(JSON.stringify(report.placements));
    if (witness && witness !== current) throw new Error(`Non-deterministic ${name}`); witness = current;
    if (JSON.stringify(report.internal_requirements) !== JSON.stringify(baseline.internal_requirements)
      || JSON.stringify(report.external_requirements) !== JSON.stringify(baseline.external_requirements)
      || report.placements.length !== baseline.placements.length) throw new Error(`Lost requirements in ${name}`);
    samples.push({ placement_ms: completion.placement_ms, body_ms: completion.body_ms, phases: report.block_experiment.timings });
  }
  const output = path.join(folder, `${name}-routing.json`);
  let routing;
  try {
    execFileSync('target/release/aic-fixed-routing-probe', ['--placement', path.join(folder, `${name}-0.json`), ...catalogArgs,
      '--transports', 'data/game/normalized/transports.json', '--components', 'data/game/normalized/logistics-components.json',
      '--report-output', output], { encoding: 'utf8', timeout: 60000 });
    const r = JSON.parse(fs.readFileSync(output));
    if (!r.covered_routes_validated || r.factory_validated) throw new Error('Unexpected routing validation status');
    for (const p of report.placements) {
      const q = r.final_node.placements.find(q => q.instance === p.instance);
      if (!q || q.x !== p.x + r.search_translation || q.y !== p.y + r.search_translation || q.rotation !== p.rotation)
        throw new Error(`Router moved ${p.instance}`);
    }
    for (const a of r.attempts) if (a.astar_searches > 64 || a.astar_searches !== a.port_trials.reduce((n, t) => n + t.astar_searches, 0))
      throw new Error('Unexpected routing search accounting');
    const labels = new Map(report.placements.map(p => [p.instance, p.label]));
    const edge = report.internal_requirements.find(e => e.id === r.unresolved_internal[0]);
    routing = { connected: r.required_internal - r.unresolved_internal.length, required: r.required_internal,
      elapsed_ms: r.elapsed_ms, external_pending: r.unresolved_external.length,
      first_failure: edge ? [labels.get(edge.source), labels.get(edge.target)] : null,
      astar_calls: r.attempts.reduce((n, a) => n + a.astar_searches, 0),
      chain_links_connected: report.block_experiment.extraction.filter(e => e.contracted && !r.unresolved_internal.includes(e.requirement)).length,
      invalid_witness_rejections: r.attempts.reduce((n, a) => n + a.witness_rejections, 0) };
  } catch (error) {
    if (error.code !== 'ETIMEDOUT') throw error;
    routing = { status: 'process-timeout', wall_limit_ms: 60000 };
  }
  rows.push({ name, contract, minimum_gap: gap, bounds: report.bounds, ...report.block_experiment.summary,
    placements_sha256: witness, samples, routing });
  fs.writeFileSync(path.join(folder, 'summary.json'), JSON.stringify({ schema_version: 1,
    scope: 'warm-file sequential fresh processes; three placement samples and one release routing run per cell; not factory validation',
    frozen_sha256: hash(fs.readFileSync(frozen)), model_sha256: modelHash, binary_sha256: hash(fs.readFileSync('target/release/aic-fixed-routing-probe')),
    rows }, null, 2) + '\n');
  console.log(JSON.stringify({ name, bounds: report.bounds, placement_ms: samples.map(s => s.placement_ms).sort((a, b) => a - b)[1], routing }));
}
