import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { Environment, args, catalogKeys, diagnostic, digest, fail, fingerprint, hash, loadGraphs, proposal, read, rewards, rng, write } from './environment.mjs';
import { placement } from '../chain-prototype/geometry.mjs';
import { features, cheapChoice, tokenFields, actionFields } from './features.mjs';

export function validatePlan(p) {
  const counts = ['train', 'validation', 'test', 'out-of-family'];
  if (p.schema_version !== 1 || !Number.isSafeInteger(p.seed) || !Array.isArray(p.requests) || p.requests.length < 2 || new Set(p.requests.map(r => r.id)).size !== p.requests.length
    || p.requests.some(r => typeof r.id !== 'string' || !r.id || typeof r.path !== 'string' || !['main', 'out-of-family'].includes(r.split))
    || !['main', 'out-of-family'].every(s => p.requests.some(r => r.split === s)) || !p.counts || Object.keys(p.counts).length !== counts.length
    || counts.some(k => !Number.isSafeInteger(p.counts[k]) || p.counts[k] < 1)
    || ['actions', 'max_state_attempts', 'route_expansions_per_action', 'validator_ms', 'elapsed_ms', 'max_gap'].some(k => !Number.isSafeInteger(p[k]) || p[k] < 1)
    || p.actions < 2 || p.actions > 64 || p.max_gap > 16) fail('input-invalid', 'Invalid schema-1 experiment plan');
}
export function sourcePaths(entry, found = new Set()) {
  if (found.has(entry)) return found; found.add(entry);
  for (const m of fs.readFileSync(entry, 'utf8').matchAll(/from\s+['"]([^'"]+)['"]/g)) if (m[1].startsWith('.')) sourcePaths(path.resolve(path.dirname(entry), m[1]), found);
  return found;
}
export function generate(options) {
  const plan = read(options.plan); validatePlan(plan);
  const requests = loadGraphs(plan, options.plan, options), output = options['output-directory'];
  const sources = [...Object.entries(options).filter(([k]) => k !== 'output-directory').map(([kind, p]) => ({ kind, path: p })),
    ...requests.map(r => ({ kind: 'request', path: r.path })), ...[...sourcePaths(fileURLToPath(import.meta.url))].map(p => ({ kind: 'implementation', path: p }))].map(p => ({ ...p, sha256: hash(p.path) }));
  fs.mkdirSync(output); write(path.join(output, 'provenance.json'), sources); write(path.join(output, 'plan.json'), plan);
  const env = new Environment(options, plan, output), random = rng(plan.seed), samples = [], seen = new Set(), attempts = [];
  try {
    for (const [split, count] of Object.entries(plan.counts)) {
      const families = requests.filter(r => r.split === (split === 'out-of-family' ? split : 'main'));
      for (let index = 0; index < count;) {
        env.guard(); if (attempts.length >= plan.max_state_attempts) fail('budget-exhausted', 'State attempt budget exhausted', plan.max_state_attempts);
        const request = families[index % families.length], graph = request.graph;
        const attempt = { ordinal: attempts.length, split, request: request.id, status: 'pending' }; attempts.push(attempt);
        const sink = graph.nodes.get(graph.order[0]);
        let base = { schema_version: 1, placements: [placement(sink, random.pick(sink.facility_data.allowed_rotations))], utilizations: { [sink.id]: sink.utilization }, transport_networks: [], logistics_components: [] };
        const seed = env.validate(graph, base, { seed: attempt.ordinal });
        if (!seed.accepted) fail('seed-rejected', 'Catalog-backed seed did not validate', seed);
        attempt.prefix_validations = [seed.artifact]; let prefixFailed = false;
        for (const edge of graph.growthEdges.slice(0, -1)) {
          let accepted = null;
          for (let retry = 0; retry < plan.actions; retry++) {
            const a = proposal(graph, base, edge, random, plan.max_gap), outcome = env.evaluate(graph, base, edge, a);
            if (outcome.accepted) { accepted = outcome; break; }
          }
          if (!accepted) { prefixFailed = true; break; }
          base = env.witness(accepted); attempt.prefix_validations.push(accepted.artifact);
        }
        if (prefixFailed) { attempt.status = 'prefix-exhausted'; continue; }
        const edge = graph.growthEdges.at(-1), unique = new Map();
        for (let retry = 0; retry < plan.actions * 20 && unique.size < plan.actions; retry++) {
          const a = proposal(graph, base, edge, random, plan.max_gap); unique.set(digest(a), a);
        }
        if (unique.size !== plan.actions) { attempt.status = 'proposal-duplicates'; continue; }
        const actions = random.shuffle([...unique.values()]), outcomes = actions.map(a => env.evaluate(graph, base, edge, a));
        attempt.candidate_statuses = outcomes.map(o => o.status);
        const accepted = outcomes.map((o, i) => o.accepted ? i : -1).filter(i => i >= 0);
        if (new Set(accepted.map(i => JSON.stringify(outcomes[i].score.tuple))).size < 2) { attempt.status = 'uninformative'; continue; }
        const current = random.pick(accepted), witness = env.witness(outcomes[current]), fp = fingerprint(graph, witness);
        if (seen.has(fp)) { attempt.status = 'duplicate-state'; continue; } seen.add(fp);
        const f = features(graph, base, witness, edge, actions);
        samples.push({ id: samples.length, request: request.id, split, fingerprint: fp, base, current, actions, outcomes, rewards: rewards(outcomes), features: f,
          cheap_choice: cheapChoice(base, actions, f.actions) });
        attempt.status = 'selected'; attempt.sample = samples.length - 1; index++;
        if (index % 32 === 0) console.log(JSON.stringify({ stage: 'generation', split, generated: index, requested: count, elapsed_ms: performance.now() - env.started }));
      }
    }
    for (const p of sources) if (hash(p.path) !== p.sha256) fail('stale-provenance', 'Source changed during generation', p.path);
    write(path.join(output, 'dataset.json'), { schema_version: 1, factory_validated: false, token_fields: tokenFields, action_fields: actionFields, samples });
    write(path.join(output, 'generation.json'), { schema_version: 1, status: 'complete', factory_validated: false, elapsed_ms: performance.now() - env.started, work: env.work, attempts,
      counts: plan.counts, dataset_sha256: hash(path.join(output, 'dataset.json')), provenance_sha256: hash(path.join(output, 'provenance.json')), node: process.version });
    console.log(JSON.stringify({ status: 'complete', samples: samples.length, work: env.work, output }));
  } catch (e) {
    write(path.join(output, 'generation.json'), { schema_version: 1, status: 'failed', completed_samples: samples.length, elapsed_ms: performance.now() - env.started, work: env.work, attempts,
      diagnostics: [diagnostic(e.code?.replace(/^learning-/, '') ?? 'failed', 'generation', e.message, e.entity)] }); throw e;
  }
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { generate(args(['plan', ...catalogKeys, 'validator', 'output-directory'])); }
  catch (e) { console.error(JSON.stringify(diagnostic(e.code?.replace(/^learning-/, '') ?? 'input-invalid', 'generation', e.message, e.entity))); process.exitCode = 1; }
}
