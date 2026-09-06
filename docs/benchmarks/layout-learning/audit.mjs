import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { Environment, args, diagnostic, fail, hash, loadGraphs, read, write, improves, fingerprint } from './environment.mjs';
import { features } from './features.mjs';

export function audit(options) {
  const trainingPath = path.join(options['training-directory'], 'training.json'), training = read(trainingPath), dataDirectory = training.dataset_directory;
  const generation = read(path.join(dataDirectory, 'generation.json')), provenance = read(path.join(dataDirectory, 'provenance.json'));
  if (training.status !== 'complete' || training.dataset_sha256 !== generation.dataset_sha256 || hash(path.join(dataDirectory, 'dataset.json')) !== generation.dataset_sha256
    || hash(path.join(dataDirectory, 'provenance.json')) !== generation.provenance_sha256) fail('audit-provenance', 'Dataset/training hashes differ', dataDirectory);
  for (const p of [...provenance, ...read(path.join(options['training-directory'], 'provenance.json'))]) if (hash(p.path) !== p.sha256) fail('audit-provenance', 'Stale source', p.path);
  for (const r of training.runs) if (hash(path.join(options['training-directory'], r.checkpoint.file)) !== r.checkpoint.sha256) fail('audit-provenance', 'Changed checkpoint', r.name);
  const envOptions = Object.fromEntries(provenance.filter(p => p.kind !== 'implementation' && p.kind !== 'request').map(p => [p.kind, p.path]));
  const plan = read(envOptions.plan), requests = loadGraphs(plan, envOptions.plan, envOptions), data = read(path.join(dataDirectory, 'dataset.json'));
  const samples = new Map(data.samples.map(s => [s.id, s])), unique = new Map();
  const add = (sample, action) => {
    if (!samples.has(sample) || !['test', 'out-of-family'].includes(samples.get(sample).split) || !Number.isInteger(action) || !samples.get(sample).actions[action]) fail('audit-input', 'Invalid held-out selection', { sample, action });
    unique.set(`${sample}:${action}`, { sample, action });
  };
  for (const r of training.runs) for (const selections of Object.values(r.choices)) for (const p of selections) add(p.sample, p.action);
  for (const selections of Object.values(training.screened_choices)) for (const p of selections) add(p.sample, p.action);
  for (const s of data.samples.filter(s => ['test', 'out-of-family'].includes(s.split))) {
    add(s.id, s.current); add(s.id, s.cheap_choice); add(s.id, s.rewards.indexOf(1));
  }
  const output = options['output-directory']; fs.mkdirSync(output);
  const env = new Environment(envOptions, plan, output), results = [], checked = new Set();
  try {
    for (const p of unique.values()) {
      const s = samples.get(p.sample), graph = requests.find(r => r.id === s.request).graph, expected = s.outcomes[p.action];
      if (!checked.has(s.id)) {
        const current = s.outcomes[s.current], dir = path.join(dataDirectory, current.artifact.directory);
        if (hash(path.join(dir, 'witness.json')) !== current.artifact.witness_sha256 || hash(path.join(dir, 'local-witness.json')) !== current.artifact.local_witness_sha256
          || hash(path.join(dir, 'validation.json')) !== current.artifact.report_sha256) fail('audit-provenance', 'Changed current witness evidence', dir);
        const w = read(path.join(dir, 'local-witness.json'));
        if (fingerprint(graph, w) !== s.fingerprint || JSON.stringify(features(graph, s.base, w, graph.growthEdges.at(-1), s.actions)) !== JSON.stringify(s.features)) fail('audit-features', 'Features/fingerprint differ from original geometry', s.id);
        if (!env.validate(graph, s.base, { sample: s.id, stage: 'frozen-prefix' }).accepted) fail('audit-prefix', 'Frozen prefix did not freshly validate', s.id);
        checked.add(s.id);
      }
      const savedBase = JSON.stringify(s.base), actual = env.evaluate(graph, s.base, graph.growthEdges.at(-1), s.actions[p.action]);
      if (JSON.stringify(s.base) !== savedBase) fail('audit-mutation', 'Action modified frozen input', s.id);
      if (actual.accepted !== expected.accepted || actual.status !== expected.status || (actual.accepted && (JSON.stringify(actual.score) !== JSON.stringify(expected.score)
        || actual.artifact.witness_sha256 !== expected.artifact.witness_sha256))) fail('audit-replay', 'Fresh routing/validation differs from cached outcome', { ...p, actual, expected });
      const committed = improves(actual, s.outcomes[s.current]);
      results.push({ ...p, status: actual.status, committed, retained_current: !committed, artifact: actual.artifact ?? null });
    }
    write(path.join(output, 'audit.json'), { schema_version: 1, status: 'complete', factory_validated: false, training_sha256: hash(trainingPath),
      dataset_sha256: generation.dataset_sha256, unique_selected_actions: results.length, held_out_states: checked.size, elapsed_ms: performance.now() - env.started, work: env.work, results });
    console.log(JSON.stringify({ status: 'complete', held_out_states: checked.size, unique_selected_actions: results.length, work: env.work, output }));
  } catch (e) {
    write(path.join(output, 'audit.json'), { schema_version: 1, status: 'failed', work: env.work, results, diagnostic: diagnostic('audit-failed', 'audit', e.message, e.entity) }); throw e;
  }
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { audit(args(['training-directory', 'output-directory'])); }
  catch (e) { console.error(JSON.stringify(diagnostic('audit-failed', 'audit', e.message, e.entity))); process.exitCode = 1; }
}
