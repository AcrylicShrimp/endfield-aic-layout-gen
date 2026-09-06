import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { args, diagnostic, digest, hash, read, write, sourcePaths, Probe } from './io.mjs';
import { Environment } from './environment.mjs';
import { compare, score } from './score.mjs';
import { checkRoot, semanticKey } from './coverage.mjs';

try {
  const options = args(['roots', 'evaluation', 'output-directory']), dataset = read(options.roots), evaluation = read(options.evaluation), output = path.resolve(options['output-directory']);
  const sources = [path.resolve(options.roots), path.resolve(options.evaluation), ...sourcePaths(fileURLToPath(import.meta.url))];
  const provenance = sources.map(p => ({ path: p, sha256: hash(p) }));
  if (evaluation.status !== 'complete' || evaluation.roots !== path.resolve(options.roots)) throw new Error('Incomplete evaluation or different imported roots');
  for (const p of evaluation.provenance) if (hash(p.path) !== p.sha256) throw new Error(`Evaluation source changed: ${p.path}`);
  const workerProvenancePath = path.join(path.dirname(path.resolve(options.evaluation)), 'worker', 'provenance.json');
  const workerProvenance = read(workerProvenancePath);
  provenance.push({ path: workerProvenancePath, sha256: hash(workerProvenancePath) });
  for (const p of [workerProvenance.roots, ...workerProvenance.implementation, ...dataset.provenance])
    if (hash(p.path) !== p.sha256) throw new Error(`Evaluation environment or root source changed: ${p.path}`);
  fs.mkdirSync(output); const probe = new Probe(dataset.options), env = new Environment(dataset, probe), rows = [], began = performance.now();
  const observationsPath = path.join(output, 'observations.jsonl'), observationFile = fs.openSync(observationsPath, 'wx');
  const maxWork = Math.max(...evaluation.config.budgets.filter(b => b.mode === 'work').map(b => b.proposals)), firstSeed = evaluation.config.rollout_seeds[0];
  let replayed = 0, modelObservations = 0;
  try {
    for (const [index, row] of evaluation.rows.entries()) {
      if (hash(row.artifact.path) !== row.artifact.sha256) throw new Error(`Changed episode artifact ${row.artifact.path}`);
      const episode = read(row.artifact.path), root = env.roots.get(episode.root);
      if (!root || digest(episode.best_witness) !== row.best_witness_sha256 || digest(episode.best_witness) !== episode.best_witness_sha256
        || semanticKey(episode.best_witness) !== root.semantic_key) throw new Error(`Changed root/selected witness semantics in row ${index}`);
      const actualScore = score(episode.best_witness, env.catalogs);
      if (compare(actualScore.tuple, row.best_score.tuple) || compare(actualScore.tuple, episode.best_score.tuple) || compare(actualScore.tuple, root.score.tuple) > 0)
        throw new Error(`Incorrect or worse retained score in row ${index}`);
      const validation = await probe.check(episode.best_witness), coverage = checkRoot(root, root.graph, episode.best_witness, validation);
      if (!coverage.factory_validated) throw new Error(`Fresh complete validation rejected row ${index}`);
      if (episode.trace.length !== row.work.proposals || episode.trace.reduce((sum, t) => sum + t.route_expansions, 0) !== row.work.route_expansions
        || episode.trace.filter(t => t.validator_attempted).length !== row.work.validations
        || episode.trace.some(t => t.route_expansions > dataset.environment.route_expansions_per_action)) throw new Error(`Incorrect work ledger in row ${index}`);
      for (let i = 1; i < episode.incumbents.length; i++) if (compare(episode.incumbents[i].score, episode.incumbents[i - 1].score) >= 0)
        throw new Error(`Non-improving incumbent replacement in row ${index}`);
      const audited = { row: index, root: root.id, score: actualScore.tuple, factory_validated: true, trace_proposals: episode.trace.length,
        selected_witness_sha256: digest(episode.best_witness), replayed: false };
      // Cover every method/root at the largest work cap. Other final witnesses are still all audited.
      if (row.budget.mode === 'work' && row.budget.proposals === maxWork && row.rollout_seed === firstSeed) {
        await env.reset(root.id, { steps: row.budget.proposals, elapsed_ms: 300000 });
        const states = new Map([[digest(env.current), { witness: env.current, score: env.currentScore }]]);
        for (const [step, expected] of episode.trace.entries()) {
          const parent = states.get(expected.parent_sha256);
          if (!parent) throw new Error(`Missing validated parent at row ${index} step ${step}`);
          env.current = parent.witness; env.currentScore = parent.score;
          const observation = env.observation(), actionIndex = env.actions.findIndex(a => digest(a) === digest(expected.action));
          if (actionIndex < 0 || !observation.features.action_mask[actionIndex]) throw new Error(`Recorded action is absent or masked at row ${index} step ${step}`);
          if (row.policy.checkpoint) {
            fs.writeSync(observationFile, JSON.stringify({ row: index, step, features: observation.features, action_index: actionIndex }) + '\n'); modelObservations++;
          }
          const actual = await env.evaluate(expected.action);
          if (actual.accepted !== expected.accepted || actual.status !== expected.status || actual.route_expansions !== expected.route_expansions
            || actual.validator_attempted !== expected.validator_attempted || actual.witness_sha256 !== expected.witness_sha256
            || JSON.stringify(actual.score) !== JSON.stringify(expected.score) || JSON.stringify(actual.best) !== JSON.stringify(expected.best))
            throw new Error(`Fresh transition mismatch at row ${index} step ${step}: ${JSON.stringify({ actual, expected })}`);
          if (actual.accepted) states.set(actual.witness_sha256, { witness: env.current, score: env.currentScore });
          replayed++;
        }
        if (digest(env.best) !== episode.best_witness_sha256) throw new Error(`Replayed incumbent differs in row ${index}`);
        audited.replayed = true;
      }
      rows.push(audited);
      if (rows.length % 64 === 0) console.log(JSON.stringify({ audited: rows.length, total: evaluation.rows.length, replayed, elapsed_ms: performance.now() - began }));
    }
    for (const p of provenance) if (hash(p.path) !== p.sha256) throw new Error(`Audit source changed: ${p.path}`);
  } finally { fs.closeSync(observationFile); probe.close(); }
  const report = { schema_version: 1, status: 'complete', evaluation: path.resolve(options.evaluation), rows, replayed_transitions: replayed,
    model_observations: modelObservations, observations: { path: observationsPath, sha256: hash(observationsPath) }, elapsed_ms: performance.now() - began, provenance };
  write(path.join(output, 'report.json'), report);
  console.log(JSON.stringify({ status: report.status, audited: rows.length, replayed, model_observations: modelObservations, output }));
} catch (e) { console.error(JSON.stringify(diagnostic('audit-failed', 'replay-audit', e.message))); process.exitCode = 1; }
