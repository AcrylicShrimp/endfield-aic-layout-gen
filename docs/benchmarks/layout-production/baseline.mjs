import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { args, read, write, hash, digest, sourcePaths, Probe, catalogKeys } from '../layout-rl/io.mjs';
import { prepareRoot, checkRoot } from '../layout-rl/coverage.mjs';
import { score, compare } from '../layout-rl/score.mjs';
import { density, mechanicalWaste, routeDiagnostics } from './metrics.mjs';

const checkedHash = (p, expected) => { if (hash(p) !== expected) throw new Error(`Changed artifact: ${p}`); };
const checkedProvenance = entries => { for (const p of entries) checkedHash(p.path, p.sha256); };
const percentile = (values, q) => [...values].sort((a,b) => a-b)[Math.max(0, Math.ceil(q * values.length) - 1)] ?? null;

export async function baseline(options) {
  const planPath = path.resolve(options.plan), plan = read(planPath), resolve = p => path.resolve(path.dirname(planPath), p);
  if (plan.schema_version !== 1 || !Array.isArray(plan.references) || !plan.references.length) throw new Error(`Invalid baseline plan: ${planPath}`);
  const rootsPath = resolve(plan.roots), dataset = read(rootsPath); checkedProvenance(dataset.provenance);
  const root = dataset.roots.find(r => r.id === plan.target_root);
  if (!root) throw new Error(`Unknown mandatory factory root: ${plan.target_root}`);
  const catalogs = Object.fromEntries(catalogKeys.map(k => [k, read(dataset.options[k])]));
  const graph = prepareRoot(root, catalogs);
  const inputs = new Set([planPath, rootsPath, ...sourcePaths(fileURLToPath(import.meta.url)), ...dataset.provenance.map(p => p.path)]);
  const output = path.resolve(options['output-directory']); fs.mkdirSync(output);
  const probe = new Probe(dataset.options);
  const cases = [], policyRows = [];
  try {
    for (const source of plan.references) {
      if (!/^[a-z0-9-]+$/.test(source.id)) throw new Error(`Invalid source identity: ${source.id}`);
      let witness, episode = null, expected = null;
      if (source.kind === 'root') {
        const r = dataset.roots.find(r => r.id === source.root);
        if (!r) throw new Error(`Unknown source root: ${source.root}`);
        witness = r.witness; expected = r.score.tuple;
      } else {
        const reportPath = resolve(source.report), report = read(reportPath); inputs.add(reportPath);
        if (source.kind === 'evaluation-row') {
          const row = report.main.evaluation.rows.find(r => r.index === source.row);
          if (!row || row.root !== root.id) throw new Error(`Evaluation row does not match mandatory root: ${source.row}`);
          checkedHash(row.artifact.path, row.artifact.sha256); inputs.add(row.artifact.path);
          episode = read(row.artifact.path); witness = episode.best_witness; expected = row.best;
          if (digest(witness) !== row.best_witness_sha256) throw new Error(`Changed learned witness: ${row.artifact.path}`);
        } else if (source.kind === 'selected-directory') {
          const p = path.join(report.selected_directory, 'witness.json'); checkedHash(p, report.witness_sha256); inputs.add(p); witness = read(p);
          // Historical turn metrics can differ; always recompute the common current score.
        } else throw new Error(`Unknown reference kind: ${source.kind}`);
      }
      const validation = await probe.check(witness), coverage = checkRoot(root, graph, witness, validation), currentScore = score(witness, catalogs);
      if (!coverage.factory_validated || expected && compare(currentScore.tuple, expected)) throw new Error(`Baseline validation/score failed: ${source.id}`);
      const directory = path.join(output, source.id); fs.mkdirSync(directory); write(path.join(directory, 'witness.json'), witness);
      const result = { id: source.id, source, witness: { path: path.join(directory, 'witness.json'), sha256: hash(path.join(directory, 'witness.json')), semantic_sha256: digest(witness) },
        facilities: witness.placements.length, score: currentScore, density: density(witness,catalogs), routes: routeDiagnostics(witness),
        mechanical_waste: episode ? mechanicalWaste(episode) : null, validation, coverage, visual_verdict: 'pending-direct-inspection' };
      write(path.join(directory,'audit.json'),result); cases.push(result);
    }
    const evaluationPath = resolve(plan.policy_evaluation), evaluation = read(evaluationPath), evalDataset = read(evaluation.roots);
    inputs.add(evaluationPath); inputs.add(evaluation.roots); checkedProvenance(evaluation.provenance); checkedProvenance(evalDataset.provenance);
    const graphs = new Map();
    for (const row of evaluation.rows) {
      if (row.policy.model_seed === null) continue;
      checkedHash(row.artifact.path,row.artifact.sha256);
      const episode = read(row.artifact.path), r = evalDataset.roots.find(r => r.id === row.root);
      if (!r) throw new Error(`Unknown policy root: ${row.root}`);
      if (!graphs.has(r.id)) graphs.set(r.id,prepareRoot(r,catalogs));
      const validation = await probe.check(episode.best_witness), coverage = checkRoot(r,graphs.get(r.id),episode.best_witness,validation);
      if (!coverage.factory_validated || compare(score(episode.best_witness,catalogs).tuple,row.best_score.tuple)) throw new Error(`Policy baseline validation failed: ${row.artifact.path}`);
      policyRows.push({ root: row.root, split: r.split, family: r.family, policy: row.policy.name, budget: row.budget,
        score: row.best_score.tuple, density: density(episode.best_witness,catalogs), waste: mechanicalWaste(episode),
        artifact: row.artifact, validation_sha256: digest({ ...validation, elapsed_ms: 0 }), factory_validated: true });
    }
  } finally { probe.close(); }
  const methods = [...new Set(policyRows.map(r => r.policy))], summaries = [];
  for (const method of methods) for (const budget of [...new Set(policyRows.map(r => r.budget.proposals))]) {
    const rows = policyRows.filter(r => r.policy === method && r.budget.proposals === budget), rates = rows.map(r => r.waste.futile_fraction);
    const mean = rates.reduce((a,b) => a+b,0)/rates.length, p95=percentile(rates,.95);
    summaries.push({ method, proposals: budget, factories: rows.length, mean_futile_fraction: mean, p95_futile_fraction: p95,
      mechanical_efficiency_gate: mean <= .1 && p95 <= .2, median_empty_fraction: percentile(rows.map(r => r.density.empty_fraction),.5) });
  }
  const report = { schema_version: 1, status: 'baseline-measured', readiness_verdict: 'not-ready', impossibility_verdict: 'not-established',
    mandatory_target: { root: root.id, original_request: root.origin.request, required_facilities: root.witness.placements.length },
    cases, policies: { scope: 'one-seed-validation-pilot', test_evaluations: 0, summaries, rows: policyRows },
    provenance: [...inputs].map(p => ({ path:p,sha256:hash(p) })) };
  write(path.join(output,'report.json'),report);
  console.log(JSON.stringify({ status:report.status, readiness:report.readiness_verdict,
    cases:cases.map(c => ({ id:c.id,area:c.score.area,empty_percent:c.density.percent_empty,density_gate:c.density.density_gate })),
    output }));
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { await baseline(args(['plan','output-directory'])); }
  catch(e) { console.error(JSON.stringify({ code:'ready-baseline-failed',severity:'error',stage:'readiness-baseline',message:e.message,entity:e.path??null }));process.exitCode=1; }
}
