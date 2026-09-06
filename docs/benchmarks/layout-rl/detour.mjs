import fs from 'node:fs';
import path from 'node:path';
import { args, diagnostic, read, write, Probe, hash, sourcePaths } from './io.mjs';
import { Environment } from './environment.mjs';
import { compare } from './score.mjs';
import { stateKey, rankedActions } from './search.mjs';
import { fileURLToPath } from 'node:url';

async function explore(env, monotone, maxDepth) {
  const queue = [{ witness: env.current, score: env.currentScore, path: [] }], seen = new Set([stateKey(env.current)]);
  let expanded = 0, unknowns = 0, masked = 0;
  while (queue.length && !env.done) {
    const parent = queue.shift();
    if (parent.path.length >= maxDepth) continue;
    env.current = parent.witness; env.currentScore = parent.score; env.candidates.clear(); expanded++;
    const observation = env.observation(); masked += observation.features.action_mask.filter(x => !x).length;
    for (const { action } of rankedActions(env)) {
      if (env.done) break;
      const result = await env.evaluate(action, { advance: false });
      if (result.status.includes('late') || result.status.includes('budget') || result.routes?.some(r => r.status === 'work-budget')) unknowns++;
      if (!result.accepted) continue;
      const child = env.candidates.get(result.candidate);
      if (monotone && compare(child.score.tuple, parent.score.tuple) > 0) continue;
      const key = stateKey(child.witness);
      if (seen.has(key)) continue; seen.add(key);
      const trace = [...parent.path, { action, before: parent.score.tuple, after: child.score.tuple, witness: child.witness }];
      if (compare(child.score.tuple, env.initial.tuple) < 0)
        return { found: true, path: trace, expanded, states: seen.size, unknowns, masked, exhausted: false, work: { ...env.work } };
      queue.push({ ...child, path: trace });
    }
  }
  return { found: false, path: null, expanded, states: seen.size, unknowns, masked, exhausted: queue.length === 0 && !env.done, work: { ...env.work } };
}

try {
  const options = args(['roots', 'root', 'output-directory']), dataset = read(options.roots), output = path.resolve(options['output-directory']);
  const provenance = [{ path: path.resolve(options.roots), sha256: hash(options.roots) }, ...[...sourcePaths(fileURLToPath(import.meta.url))].map(p => ({ path: p, sha256: hash(p) }))];
  fs.mkdirSync(output); const probe = new Probe(dataset.options), env = new Environment(dataset, probe), began = performance.now();
  try {
    await env.reset(options.root, { steps: 4096, elapsed_ms: 30000 });
    const monotone = await explore(env, true, Infinity); write(path.join(output, 'monotone-trace.json'), env.summary({ trace: true }));
    await env.reset(options.root, { steps: 4096, elapsed_ms: 30000 });
    const nonmonotone = await explore(env, false, 4); write(path.join(output, 'nonmonotone-trace.json'), env.summary({ trace: true }));
    const proved = monotone.exhausted && !monotone.found && monotone.unknowns === 0 && nonmonotone.found
      && nonmonotone.path.some(p => compare(p.after, p.before) > 0);
    const report = { schema_version: 1, root: options.root, initial_score: env.initial, monotone, nonmonotone, temporary_deterioration_required: proved,
      proof_scope: 'All non-increasing transitions in the enumerated deterministic local action/router system, including equal-score states. Not every legal game relocation, route, port or canvas.',
      per_phase_budget: { proposals: 4096, elapsed_ms: 30000, route_expansions_per_action: env.plan.route_expansions_per_action },
      elapsed_ms: performance.now() - began, provenance };
    for (const source of provenance) if (hash(source.path) !== source.sha256) throw new Error(`Source changed: ${source.path}`);
    write(path.join(output, 'report.json'), report);
    console.log(JSON.stringify({ root: options.root, temporary_deterioration_required: proved, monotone_states: monotone.states,
      monotone_exhausted: monotone.exhausted, detour: nonmonotone.path?.map(p => ({ action: p.action, before: p.before, after: p.after })), output }));
  } finally { probe.close(); }
} catch (e) { console.error(JSON.stringify(diagnostic('detour-failed', 'local-transition-audit', e.message))); process.exitCode = 1; }
