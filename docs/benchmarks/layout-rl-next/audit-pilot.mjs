import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { args, read, write, hash, digest, Probe, catalogKeys } from '../layout-rl/io.mjs';
import { prepareRoot, checkRoot, semanticKey } from '../layout-rl/coverage.mjs';
import { compare, score } from '../layout-rl/score.mjs';

export function trajectoryStats(episode) {
  const improvements = episode.incumbents.filter(i => i.proposal > 0);
  const last = improvements.at(-1)?.proposal ?? 0;
  return { improvements: improvements.length, first_improvement: improvements[0]?.proposal ?? null,
    last_improvement: improvements.at(-1)?.proposal ?? null, no_improvement_tail: episode.work.proposals - last,
    improvements_after_128: improvements.filter(i => i.proposal > 128).length,
    curve: [0, 16, 32, 64, 128, 256, 512, 1024].filter(p => p <= episode.work.proposals).map(proposal =>
      ({ proposal, score: episode.incumbents.filter(i => i.proposal <= proposal).at(-1).score })),
    statuses: Object.fromEntries([...new Set(episode.trace.map(t => t.status))].map(status =>
      [status, episode.trace.filter(t => t.status === status).length])) };
}

export async function auditPilot(options) {
  const planPath = path.resolve(options.plan), plan = read(planPath);
  if (plan.schema_version !== 1 || !Array.isArray(plan.training_reports)) throw new Error(`Invalid audit plan: ${planPath}`);
  const dataset = read(plan.roots), roots = new Map(dataset.roots.map(r => [r.id, r]));
  for (const p of dataset.provenance) if (hash(p.path) !== p.sha256) throw new Error(`Stale corpus source: ${p.path}`);
  const catalogs = Object.fromEntries(catalogKeys.map(k => [k, read(dataset.options[k])])), graphs = new Map();
  const inputs = [planPath, plan.roots, plan.corpus_report, ...plan.training_reports, plan.evaluation_report, fileURLToPath(import.meta.url)];
  const probe = new Probe(dataset.options), training = [], validation = [], audits = [];
  async function check(artifact, rootId, expected, split) {
    const root = roots.get(rootId);
    if (!root || root.split !== split) throw new Error(`Unexpected ${split} root: ${rootId}`);
    if (hash(artifact.path) !== artifact.sha256) throw new Error(`Changed trajectory: ${artifact.path}`);
    const episode = read(artifact.path), witness = episode.best_witness;
    if (episode.root !== rootId || episode.trace.length !== episode.work.proposals
      || digest(witness) !== episode.best_witness_sha256 || semanticKey(witness) !== root.semantic_key
      || compare(episode.initial_score.tuple, root.score.tuple) || compare(episode.best_score.tuple, expected)
      || compare(score(witness, catalogs).tuple, expected) || compare(expected, root.score.tuple) > 0)
      throw new Error(`Trajectory invariants failed: ${artifact.path}`);
    for (let i = 1; i < episode.incumbents.length; i++) {
      const before = episode.incumbents[i - 1], after = episode.incumbents[i];
      if (after.proposal <= before.proposal || after.proposal > episode.work.proposals || compare(after.score, before.score) >= 0)
        throw new Error(`Invalid incumbent history: ${artifact.path}`);
    }
    if (!graphs.has(rootId)) graphs.set(rootId, prepareRoot(root, catalogs));
    const report = await probe.check(witness), coverage = checkRoot(root, graphs.get(rootId), witness, report);
    if (!coverage.factory_validated) throw new Error(`Independent validation failed: ${artifact.path}`);
    audits.push({ artifact, root: rootId, split, witness_sha256: digest(witness), score: expected,
      validation_sha256: digest({ ...report, elapsed_ms: 0 }), factory_validated: coverage.factory_validated });
    return episode;
  }
  try {
    for (const reportPath of plan.training_reports) {
      const report = read(reportPath);
      if (report.status !== 'complete' || report.roots !== plan.roots) throw new Error(`Invalid training report: ${reportPath}`);
      for (const p of report.provenance) if (hash(p.path) !== p.sha256) throw new Error(`Changed training source: ${p.path}`);
      for (const run of report.runs) {
        if (hash(run.checkpoint.path) !== run.checkpoint.sha256) throw new Error(`Changed checkpoint: ${run.checkpoint.path}`);
        inputs.push(run.checkpoint.path);
        const detailedPath = path.join(path.dirname(run.checkpoint.path), 'training.json'), details = read(detailedPath);
        inputs.push(detailedPath);
        if (run.sampled_transitions !== plan.training_actions_per_arm || run.signal === 'zero' && run.changed_weight_elements !== 0)
          throw new Error(`Unequal training work or changed zero-control weights: ${detailedPath}`);
        let charged = 0, episodes = 0, improved = 0, informative = 0, lastQuarterImprovements = 0;
        const statuses = {}, sampledRoots = new Set();
        for (const group of details.groups) {
          sampledRoots.add(group.root);
          informative += Number(group.terminal_advantages.some(a => a !== 0));
          if (group.artifacts.length !== report.config.workers || group.summaries.length !== report.config.workers)
            throw new Error(`Worker batch accounting differs: ${detailedPath}/${group.group}`);
          for (const [i, artifact] of group.artifacts.entries()) {
            const summary = group.summaries[i], episode = await check(artifact, group.root, summary.best_score.tuple, 'train');
            if (episode.work.proposals !== report.config.horizon) throw new Error(`Episode length differs: ${artifact.path}`);
            charged += episode.work.proposals; episodes++;
            improved += Number(compare(episode.best_score.tuple, episode.initial_score.tuple) < 0);
            lastQuarterImprovements += episode.incumbents.filter(v => v.proposal > report.config.horizon * .75).length;
            for (const [status, count] of Object.entries(trajectoryStats(episode).statuses)) statuses[status] = (statuses[status] ?? 0) + count;
          }
        }
        if (charged !== plan.training_actions_per_arm) throw new Error(`Training ledger differs: ${detailedPath}`);
        training.push({ horizon: report.config.horizon, signal: run.signal, seed: run.seed, actions: charged, episodes,
          improved_episodes: improved, informative_groups: informative, groups: details.groups.length,
          sampled_training_roots: sampledRoots.size, last_quarter_improvements: lastQuarterImprovements, statuses,
          changed_weight_elements: run.changed_weight_elements, checkpoint: run.checkpoint, elapsed_seconds: run.elapsed_seconds });
      }
    }
    const evaluation = read(plan.evaluation_report);
    if (evaluation.status !== 'complete' || evaluation.roots !== plan.roots || JSON.stringify(evaluation.config.splits) !== '["validation"]')
      throw new Error(`Expected validation-only evaluation: ${plan.evaluation_report}`);
    for (const row of evaluation.rows) {
      const episode = await check(row.artifact, row.root, row.best_score.tuple, 'validation');
      validation.push({ root: row.root, family: roots.get(row.root).family, policy: row.policy.name, signal: row.policy.signal,
        rollout_seed: row.rollout_seed, budget: row.budget, initial: episode.initial_score.tuple, best: episode.best_score.tuple,
        work: row.work, elapsed_ms: row.elapsed_ms, termination: row.termination, ...trajectoryStats(episode) });
    }
  } finally { probe.close(); }
  const corpus = read(plan.corpus_report);
  const readiness = training.filter(r => r.signal === 'terminal').every(r => r.changed_weight_elements > 0 && r.informative_groups > 0);
  write(path.resolve(options['report-output']), { schema_version: 1, status: 'complete', scope: 'one-seed-validation-pilot',
    mechanism_gate: readiness, optimality_certified: false, test_evaluations: 0,
    corpus: { roots: corpus.roots, new_roots: corpus.new_roots, new_families: corpus.new_families, counts: corpus.counts,
      splits: Object.fromEntries([...new Set(dataset.roots.map(r => r.split))].map(split => [split, {
        count: dataset.roots.filter(r => r.split === split).length,
        facility_counts: Object.fromEntries([...new Set(dataset.roots.filter(r => r.split === split).map(r => r.witness.placements.length))].sort((a,b) => a-b)
          .map(n => [n, dataset.roots.filter(r => r.split === split && r.witness.placements.length === n).length])) }])) },
    training, validation, independently_validated_incumbents: audits.length, audits,
    provenance: inputs.map(p => ({ path: p, sha256: hash(p) })) });
  console.log(JSON.stringify({ status: 'complete', mechanism_gate: readiness, audited: audits.length, test_evaluations: 0 }));
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { await auditPilot(args(['plan', 'report-output'])); }
  catch (e) { console.error(JSON.stringify({ code: 'rl-next-audit-failed', severity: 'error', stage: 'pilot-audit', message: e.message, entity: null })); process.exitCode = 1; }
}
